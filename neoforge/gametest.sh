#!/usr/bin/env bash
#
# mic_rtsim's headless GameTest suites (and the pure-Java unit tests first).
#
#   ./gametest.sh                        core, then the plugins (ribbits, carryon, create, iceandfire, and the
#                                        industry plugins oreexcavation, dieselgenerators, endertransmission,
#                                        powergrid, destroy, climate, radar)
#   ./gametest.sh core                   one or more projects by name
#   ./gametest.sh core -PgametestDebugLog   with DEBUG logging (shows "Mixing ... into ..." lines)
#   ./gametest.sh -PrtsimEngine=stub -PskipJni   every suite on the pure-Java stub, no cargo
#
# Other arguments go to Gradle as they are. The engine of every server's session is the Rust one
# (rtsim-jni on mic_native, built with cargo) unless -PrtsimEngine=stub; mic_native's jar is on
# every server (the core requires it).
#
# Each project boots its own dedicated NeoForge server: the core with mic_native (no pack mod, D4),
# and each plugin with the core, mic_native and the mod it is built around (Ribbits with YUNG's API and
# GeckoLib; Carry On; Create; Ice and Fire with Uranus, Jupiter, GeckoLib and Architectury), loaded from the pack's own jars (gradle.properties: packMods). Every @GameTest
# in the project's namespace runs; the script exits non-zero if any suite failed. Each full log is left
# in <project>/build/gametest.log.
#
# Two things can fail, and they fail differently:
#   * a mixin that no longer matches its target: every mixin config is defaultRequire 1, so the launch
#     dies before a test runs. Look for "InjectionError" or "MixinTransformerError";
#   * an assertion: every test logs what it compared on a "[gametest]" line.
#
# Heavy-job etiquette on the shared laptop: one heavy job at a time (the flock), niced.

set -uo pipefail
cd "$(dirname "$0")"

projects=()
extra=()
for a in "$@"; do
    case "$a" in
        core|ribbits|carryon|create|iceandfire|oreexcavation|dieselgenerators|endertransmission|powergrid|destroy|climate|radar) projects+=("$a") ;;
        *) extra+=("$a") ;;
    esac
done
[ ${#projects[@]} -eq 0 ] && projects=(core ribbits carryon create iceandfire oreexcavation dieselgenerators endertransmission powergrid destroy climate radar)

# A JDK 21: the repository flake's dev shell when there is one, else nixpkgs' jdk21.
run() {
    if [ -n "${IN_NIX_SHELL:-}" ]; then
        "$@"
    elif [ -f ../flake.nix ]; then
        nix develop ..# -c "$@"
    else
        nix shell nixpkgs#jdk21 -c "$@"
    fi
}

LOCK=${MIC_RTSIM_LOCK:-/tmp/mic-rtsim-bridge-heavy.lock}
WORKERS=${MIC_RTSIM_WORKERS:-4}
overall=0
summary=()

for p in "${projects[@]}"; do
    LOG=$p/build/gametest.log
    mkdir -p "$p/build"
    tasks=":$p:runGameTestServer"
    [ "$p" = core ] && tasks=":core:test :core:runGameTestServer"
    start=$(date +%s)
    flock "$LOCK" nice -n 10 bash -c "$(declare -f run); run ./gradlew --max-workers=$WORKERS --console=plain $tasks ${extra[*]:-}" 2>&1 | tee "$LOG"
    status=${PIPESTATUS[0]}
    elapsed=$(( $(date +%s) - start ))

    echo
    echo "-------------------------------------------------------------------- $p"
    if ! grep -q "GAME TESTS COMPLETE\|All [0-9]* required tests passed" "$LOG"; then
        echo "FAILED: the $p server never got as far as finishing the tests."
        grep -nE "FAILED|Failed to start|MixinTransformerError|InjectionError|InvalidInjectionException|Mod loading has failed" "$LOG" | head -20
        echo "Full log: $LOG"
        overall=1
        summary+=("$p: did not finish")
        continue
    fi
    grep -E "BoundaryProtocolTest > |MarketBookTest > |GAME TESTS COMPLETE|required tests (failed|passed)|tests are now running|::.*(failed|passed)|failed at|\[gametest\]" "$LOG" | tail -80
    echo
    echo "Wall time: ${elapsed}s   Full log: $LOG"
    [ "$status" -ne 0 ] && overall=1
    summary+=("$p: $(grep -oE "All [0-9]+ required tests passed|[0-9]+ required tests failed" "$LOG" | tail -1)")
done

echo
for s in "${summary[@]}"; do echo "  $s"; done
exit "$overall"
