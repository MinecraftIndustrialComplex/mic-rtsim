#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Adapted from mic-native's tools/natives/build-natives.sh (commit 63add99, MPL-2.0), itself copied
# from Deep Time's (db1844e6) and relicensed MPL-2.0 by their copyright holder. Changes: one plugin
# library (crate rtsim-jni, librtsim_jni) and no CPU probe (a plugin takes the host's choice of
# build); the manifest's version and format_version come from the Gradle build (the mod version,
# and the mic_native C ABI it was built for); the CPU-level and instruction checks are inlined.
#
# Cross-builds rtsim-jni, the mic_native plugin of mic_rtsim, for the eight platforms, twice on
# each x86-64 platform (the baseline and x86-64-v3), checks each file, and writes the layout that
# `./gradlew build -Prelease` packs (neoforge/core/build.gradle, checkJniReleaseNatives):
#
#   target/natives-dist-jni/<platform key>/baseline/<library>    all eight platforms
#   target/natives-dist-jni/<platform key>/x86-64-v3/<library>   the four x86-64 platforms
#   target/natives-dist-jni/natives.json                         the manifest (schema 2), all 12
#
#   tools/natives/build-natives.sh [--only <key>[,<key>...]]
#
# Run from the mic-rtsim repository root inside a shell with the eight Rust targets, zig 0.13,
# cargo-zigbuild and cargo-xwin. mic-native's dev shell has exactly that (the same Rust 1.98.1):
#
#   nix develop ../mic-native -c bash tools/natives/build-natives.sh
#
# Builders, flags and checks are mic_native's: the `ship` profile (fat LTO, one codegen unit);
# linux gnu through cargo zigbuild at the glibc 2.17 floor; musl with a dynamic C runtime
# (.cargo/config.toml); windows through cargo xwin; macOS through cargo zigbuild with SDKROOT unset
# and an @rpath install name (crates/rtsim-jni/build.rs). Checks: RUSTFLAGS unset; each library
# carries the CPU level of its folder (lib.rs CPU_LEVEL); the v3 build has BMI1/BMI2 instructions
# and the baseline none; musl needs no glibc; gnu needs at most GLIBC_2.17; macOS needs at most
# 11.7.1 and only system dylibs; the file format and machine; the JNI entry points are exported.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"
[ -f crates/rtsim-jni/Cargo.toml ] || { echo "build-natives: run inside the mic-rtsim repository" >&2; exit 2; }

SHIP_PROFILE=ship
V3_LEVEL=x86-64-v3
V3_TARGET_DIR=target/x86-64-v3
GLIBC_FLOOR=2.17
MACOS_FLOOR=11.7.1
# A real x86-64-v3 build carries BMI1/BMI2 instructions (LLVM emits the BMI2 shifts for nearly every
# variable shift once it may); a build whose target-cpu was dropped has none.
V3_MIN_INSNS=50
LIB_STEM=rtsim_jni
JNI_PROBE_SYMBOL=Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nOpen

TARGETS=(
    "linux-x86_64-gnu|x86_64-unknown-linux-gnu|zig-gnu|lib${LIB_STEM}.so|v3"
    "linux-aarch64-gnu|aarch64-unknown-linux-gnu|zig-gnu|lib${LIB_STEM}.so|"
    "linux-x86_64-musl|x86_64-unknown-linux-musl|zig|lib${LIB_STEM}.so|v3"
    "linux-aarch64-musl|aarch64-unknown-linux-musl|zig|lib${LIB_STEM}.so|"
    "windows-x86_64|x86_64-pc-windows-msvc|xwin|${LIB_STEM}.dll|v3"
    "windows-aarch64|aarch64-pc-windows-msvc|xwin|${LIB_STEM}.dll|"
    "macos-x86_64|x86_64-apple-darwin|macos|lib${LIB_STEM}.dylib|v3"
    "macos-aarch64|aarch64-apple-darwin|macos|lib${LIB_STEM}.dylib|"
)

ONLY=""
while [ $# -gt 0 ]; do
    case "$1" in
        --only) ONLY="$2"; shift 2 ;;
        -h|--help) sed -n '2,40p' "$0"; exit 0 ;;
        *) echo "build-natives: unknown argument $1" >&2; exit 2 ;;
    esac
done

if [ -n "${RUSTFLAGS:-}${CARGO_ENCODED_RUSTFLAGS:-}" ]; then
    echo "build-natives: RUSTFLAGS is set; it would replace .cargo/config.toml's per-target flags" \
         "and the x86-64-v3 build's target-cpu. Unset it." >&2
    exit 1
fi

# The manifest's version and ABI, as the Gradle build reads them.
prop() { sed -n "s/^$2[[:space:]]*=[[:space:]]*//p" "$1" | head -1 | tr -d '[:space:]'; }
MOD_VERSION="$(prop neoforge/core/gradle.properties mod_version)"
ABI_VERSION="$(prop neoforge/gradle.properties mic_native_abi)"
[ -n "$MOD_VERSION" ] && [ -n "$ABI_VERSION" ] || { echo "build-natives: no mod_version or mic_native_abi in neoforge/" >&2; exit 1; }

macos_min() { case "$1" in x86_64-*) echo 10.12 ;; *) echo 11.0 ;; esac; }

READELF="$(command -v readelf || command -v llvm-readelf || true)"
[ -n "$READELF" ] || { echo "build-natives: needs readelf (binutils) or llvm-readelf" >&2; exit 1; }
OBJDUMP="$(command -v llvm-objdump || true)"

DIST=target/natives-dist-jni
rm -rf "$DIST"
mkdir -p "$DIST"

fail() { echo "build-natives: $*" >&2; exit 1; }
max_glibc() { "$READELF" -V --wide "$1" | grep -o 'GLIBC_[0-9][0-9.]*' | sed 's/^GLIBC_//' | sort -V | tail -1; }
version_le() { [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -1)" = "$1" ]; }

cpu_level_of() {
    if grep -aq "$V3_LEVEL" "$1"; then echo "$V3_LEVEL"
    elif grep -aq baseline "$1"; then echo baseline
    fi
}

v3_insn_count() {
    local dis
    dis="$(command -v llvm-objdump || command -v objdump || true)"
    [ -n "$dis" ] || { echo "?"; return 0; }
    "$dis" -d --no-show-raw-insn "$1" 2>/dev/null \
        | grep -cE '[[:space:]](shlx|shrx|sarx|rorx|andn|mulx|bzhi)[lq]?[[:space:]]' || true
}

check_elf() { # file key triple
    local f="$1" key="$2" triple="$3" needed machine
    machine="$("$READELF" -h "$f" | sed -n 's/^ *Machine: *//p')"
    case "$triple" in
        x86_64-*) [[ "$machine" == *X86-64* ]] || fail "$key: ELF machine is '$machine', expected X86-64" ;;
        aarch64-*) [[ "$machine" == *AArch64* ]] || fail "$key: ELF machine is '$machine', expected AArch64" ;;
    esac
    needed="$("$READELF" -d --wide "$f" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p' | tr '\n' ' ')"
    echo "  NEEDED: $needed"
    case "$triple" in
        *-linux-musl)
            [[ " $needed " != *" libc.so.6 "* ]] || fail "$key: NEEDED libc.so.6 (glibc) in a musl build"
            [[ "$needed" != *ld-linux* ]] || fail "$key: NEEDED ld-linux in a musl build"
            [ -z "$(max_glibc "$f")" ] || fail "$key: GLIBC_ symbol versions in a musl build"
            ;;
        *-linux-gnu)
            local g; g="$(max_glibc "$f")"
            echo "  highest GLIBC_ version: ${g:-none}"
            [ -z "$g" ] || version_le "$g" "$GLIBC_FLOOR" || fail "$key: needs GLIBC_$g, above the $GLIBC_FLOOR floor"
            ;;
    esac
}

check_pe() { # file key triple
    local f="$1" key="$2" triple="$3" magic
    magic="$(python3 - "$f" <<'PY'
import struct, sys
b = open(sys.argv[1], 'rb').read(4096)
off = struct.unpack_from('<I', b, 0x3c)[0]
assert b[:2] == b'MZ' and b[off:off + 4] == b'PE\0\0', 'not a PE file'
print(hex(struct.unpack_from('<H', b, off + 4)[0]))
PY
)" || fail "$key: not a PE file"
    case "$triple" in
        x86_64-*) [ "$magic" = 0x8664 ] || fail "$key: PE machine $magic, expected 0x8664 (x86-64)" ;;
        aarch64-*) [ "$magic" = 0xaa64 ] || fail "$key: PE machine $magic, expected 0xaa64 (arm64)" ;;
    esac
    [ -n "$OBJDUMP" ] && echo "  imports: $("$OBJDUMP" -p "$f" | sed -n 's/^ *DLL Name: //p' | tr '\n' ' ')"
    return 0
}

check_macho() { # file key triple
    local f="$1" key="$2" triple="$3" minos libs id
    [ -n "$OBJDUMP" ] || { echo "  (no llvm-objdump: Mach-O checks skipped)"; return 0; }
    "$OBJDUMP" --macho --private-header "$f" | grep -q "$( [[ $triple == x86_64-* ]] && echo X86_64 || echo ARM64 )" \
        || fail "$key: Mach-O CPU type is not $triple's"
    minos="$("$OBJDUMP" --macho --private-headers "$f" | awk '/LC_BUILD_VERSION/{b=1} b&&/minos/{print $2; exit} /LC_VERSION_MIN_MACOSX/{v=1} v&&/version/{print $2; exit}')"
    echo "  minimum macOS: ${minos:-unknown}"
    [ -z "$minos" ] || version_le "$minos" "$MACOS_FLOOR" || fail "$key: minimum macOS $minos is above $MACOS_FLOOR"
    id="$("$OBJDUMP" --macho --dylib-id "$f" | tail -n 1)"
    echo "  install name: $id"
    [ "$id" = "@rpath/$(basename "$f")" ] || fail "$key: install name $id, expected @rpath/$(basename "$f")"
    libs="$("$OBJDUMP" --macho --dylibs-used "$f" | tail -n +3 | awk '{print $1}' | tr '\n' ' ')"
    echo "  dylibs: $libs"
    for l in $libs; do
        case "$l" in /usr/lib/*|/System/Library/*) ;; *) fail "$key: links $l, which is not part of macOS" ;; esac
    done
}

build() { # builder triple cargo args...
    local builder=$1 triple=$2; shift 2
    local args=(--profile "$SHIP_PROFILE" --locked -p rtsim-jni --lib "$@")
    case "$builder" in
        zig-gnu) cargo zigbuild "${args[@]}" --target "$triple.$GLIBC_FLOOR" ;;
        zig) cargo zigbuild "${args[@]}" --target "$triple" ;;
        xwin) cargo xwin build "${args[@]}" --target "$triple" ;;
        macos) env -u SDKROOT MACOSX_DEPLOYMENT_TARGET="$(macos_min "$triple")" \
                   cargo zigbuild "${args[@]}" --target "$triple" ;;
    esac
}

place() { # src key dir triple
    local src="$1" key="$2" dir="$3" triple="$4" dest
    [ -f "$src" ] || fail "$key/$dir: cargo produced no $src"
    mkdir -p "$DIST/$key/$dir"
    dest="$DIST/$key/$dir/$(basename "$src")"
    cp "$src" "$dest"
    echo "  $dest: $(stat -c %s "$dest" 2>/dev/null || wc -c <"$dest") bytes"
    case "$triple" in
        *-linux-*) check_elf "$dest" "$key/$dir" "$triple" ;;
        *-windows-*) check_pe "$dest" "$key/$dir" "$triple" ;;
        *-darwin) check_macho "$dest" "$key/$dir" "$triple" ;;
    esac
    grep -aq "$JNI_PROBE_SYMBOL" "$dest" || fail "$key/$dir: no JNI entry point $JNI_PROBE_SYMBOL"
    grep -aq mic_native_plugin_init "$dest" || fail "$key/$dir: no mic_native_plugin_init"
    PLACED="$dest"
}

check_level() { # file key variant
    local level; level="$(cpu_level_of "$1")"
    echo "  cpu level: ${level:-none}"
    [ "$level" = "$3" ] || fail "$2/$3: the library carries cpu level ${level:-nothing}, expected $3"
}

built=0
for row in "${TARGETS[@]}"; do
    IFS='|' read -r key triple builder file v3 <<<"$row"
    if [ -n "$ONLY" ] && [[ ",$ONLY," != *",$key,"* ]]; then continue; fi

    echo "== $key baseline ($triple, profile $SHIP_PROFILE)"
    build "$builder" "$triple"
    place "target/$triple/$SHIP_PROFILE/$file" "$key" baseline "$triple"
    check_level "$PLACED" "$key" baseline
    base_count="$(v3_insn_count "$PLACED")"
    echo "  BMI1/BMI2 instructions: $base_count"

    if [ -n "$v3" ]; then
        echo "== $key $V3_LEVEL ($triple, profile $SHIP_PROFILE)"
        build "$builder" "$triple" --target-dir "$V3_TARGET_DIR" \
            --config "target.$triple.rustflags=[\"-C\", \"target-cpu=$V3_LEVEL\"]"
        place "$V3_TARGET_DIR/$triple/$SHIP_PROFILE/$file" "$key" "$V3_LEVEL" "$triple"
        check_level "$PLACED" "$key" "$V3_LEVEL"
        v3_count="$(v3_insn_count "$PLACED")"
        echo "  BMI1/BMI2 instructions: $v3_count (baseline: $base_count)"
        if [ "$v3_count" != "?" ] && [ "$base_count" != "?" ]; then
            [ "$v3_count" -ge "$V3_MIN_INSNS" ] && [ "$v3_count" -gt $((base_count * 4)) ] \
                || fail "$key/$V3_LEVEL: $v3_count BMI1/BMI2 instructions against the baseline's $base_count; was -C target-cpu=$V3_LEVEL dropped?"
        fi
    fi
    built=$((built + 1))
done
[ "$built" -gt 0 ] || fail "--only '$ONLY' names no platform key"

GIT_SHA="$(git rev-parse --short HEAD 2>/dev/null || true)"
python3 - "$DIST" "$GIT_SHA" "$V3_LEVEL" "$MOD_VERSION" "$ABI_VERSION" "${TARGETS[@]}" <<'PY'
import hashlib, json, os, re, sys

dist, git_sha, v3, version, abi, rows = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5], sys.argv[6:]

def entry(key, triple, path, variant):
    data = open(path, 'rb').read()
    floor = None
    if triple.endswith('-linux-gnu'):
        vs = [tuple(int(p or 0) for p in m) for m in re.findall(rb'GLIBC_(\d+)\.(\d+)(?:\.(\d+))?', data)]
        if vs:
            a, b, c = max(vs)
            floor = f'{a}.{b}' if c == 0 else f'{a}.{b}.{c}'
    return {'platform': key, 'variant': variant, 'file': os.path.basename(path), 'rust_target': triple,
            'glibc_floor': floor, 'sha256': hashlib.sha256(data).hexdigest(), 'size': len(data)}

libs = []
for row in rows:
    key, triple, _builder, name, _v3 = row.split('|')
    for variant in ('baseline', v3):
        path = os.path.join(dist, key, variant, name)
        if os.path.isfile(path):
            libs.append(entry(key, triple, path, variant))

manifest = {'schema': 2, 'version': version, 'format_version': abi}
if git_sha:
    manifest['git_sha'] = git_sha
manifest['libraries'] = libs
manifest['probes'] = []
with open(os.path.join(dist, 'natives.json'), 'w', encoding='utf-8') as f:
    json.dump(manifest, f, indent=2)
    f.write('\n')
print(f"natives.json: {len(libs)} libraries, version {version}, format {abi}")
for l in libs:
    print(f"  {l['platform']:<20} {l['variant']:<10} {l['file']:<22} {l['size']:>9} bytes  "
          f"sha256 {l['sha256'][:12]}  glibc {l['glibc_floor'] or '-'}")
PY

if command -v file >/dev/null; then
    (cd "$DIST" && file */*/*)
fi
