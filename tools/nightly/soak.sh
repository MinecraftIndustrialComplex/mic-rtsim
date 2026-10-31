#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# The long-horizon suite's runner (EQUILIBRIUM.md §4's S11 row; docs/overnight-choices-equilibrium.md
# EQC450 to EQC452, Ben's S11-1 (a) and S11-2 (d)). There is no CI: this script is the runner, on
# the laptop, and a red night is found in its report folder by whoever looks.
#
#   tools/nightly/soak.sh [run]      who_is_alive_soak on tonight's seed (1 + day of the year mod 8)
#   tools/nightly/soak.sh weekly     the 30-seed long run, the 30-seed kill-all table, the 8-seed
#                                    together kill-alls, one at a time
#   tools/nightly/soak.sh calibrate  who_is_alive_soak on seeds 1 to 8, four at a time (EQC442)
#   tools/nightly/soak.sh nightly    what the timer runs: `run`, then `weekly` on Sundays
#   tools/nightly/soak.sh install    a systemd user timer at 03:30 that runs `nightly` (no catch-up)
#
# Every mode but `install` first refuses (exit 3) unless the laptop is on AC with the battery at 25%
# or more, no Mowzie GameTest JVM is up and no other run holds /tmp/mic-rtsim-soak.lock. It fetches
# origin and exports origin/main with `git archive` into ~/.cache/mic-rtsim-nightly/src (no
# worktree), builds there under the cargo heavy lock at nice 19 with -j 8 and
# CARGO_TARGET_DIR=~/.cache/mic-rtsim-nightly/target (a btrfs subvolume), then runs outside the
# lock at nice 19 with idle IO. Reports go to ~/.local/state/mic-rtsim/nightly/: per night
# <date>-<sha>-seed<N>.log and .json, and `latest`, one line: the date, the commit, the seed, the
# verdict and the failing checks. The 30 newest nights are kept.
#
# Exit: 0 PASS, 1 FAIL, 2 INCOMPLETE, 3 refused, 4 the build failed.
#
# Environment (all optional): MIC_RTSIM_REPO (the repository to fetch; default the one this script
# lives in, or the one `install` baked in), RTSIM_SOAK_DAYS, RTSIM_SOAK_WALL_HOURS (passed to the
# test), SOAK_SEED (override tonight's seed).
set -euo pipefail

CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/mic-rtsim-nightly"
SRC="$CACHE/src"
TARGET="$CACHE/target"
STATE="${XDG_STATE_HOME:-$HOME/.local/state}/mic-rtsim/nightly"
LOCK=/tmp/mic-rtsim-soak.lock
HEAVY=/tmp/mic-rtsim-heavy.lock
KEEP=30

log() { printf '%s %s\n' "$(date '+%F %T')" "$*" >&2; }

repo() {
    if [ -n "${MIC_RTSIM_REPO:-}" ]; then
        printf '%s\n' "$MIC_RTSIM_REPO"
        return
    fi
    local here
    here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    git -C "$here" rev-parse --show-toplevel 2>/dev/null || {
        echo "soak.sh: not inside the repository and MIC_RTSIM_REPO is unset" >&2
        exit 3
    }
}

# Exit 3 unless the laptop may run a soak now.
refuse_unless_ready() {
    local ac bat mz
    ac="$(cat /sys/class/power_supply/AC/online 2>/dev/null || echo 0)"
    bat="$(cat /sys/class/power_supply/BAT*/capacity 2>/dev/null | head -1 || echo 0)"
    mz="$(ps -eo args | grep -c '[m]odFolders=mic_rtsim_mowziesmobs' || true)"
    if [ "$ac" != 1 ]; then log "refused: not on AC"; exit 3; fi
    if [ "${bat:-0}" -lt 25 ]; then log "refused: battery at ${bat}%"; exit 3; fi
    if [ "$mz" != 0 ]; then log "refused: a Mowzie GameTest JVM is up"; exit 3; fi
}

# Takes the soak lock for the rest of the script (exit 3 when another run holds it).
take_lock() {
    exec 9>"$LOCK"
    if ! flock -n 9; then log "refused: another soak holds $LOCK"; exit 3; fi
}

# Fetches origin and exports origin/main into $SRC; prints the commit.
export_main() {
    local r sha tmp
    r="$(repo)"
    git -C "$r" fetch -q origin
    sha="$(git -C "$r" rev-parse --short=8 origin/main)"
    mkdir -p "$CACHE"
    if [ ! -d "$TARGET" ]; then
        btrfs subvolume create "$TARGET" >/dev/null 2>&1 || mkdir -p "$TARGET"
    fi
    tmp="$CACHE/src.new"
    rm -rf "$tmp"
    mkdir -p "$tmp"
    git -C "$r" archive origin/main | tar -x -C "$tmp"
    # Unchanged files keep their times, so cargo rebuilds only what the commit changed.
    mkdir -p "$SRC"
    rsync -a --checksum --delete "$tmp/" "$SRC/"
    rm -rf "$tmp"
    printf '%s\n' "$sha"
}

# Runs a command in the exported source's dev shell.
in_shell() {
    (cd "$SRC" && nix develop "path:$SRC" -c env CARGO_TARGET_DIR="$TARGET" "$@")
}

# Builds a test binary under the heavy lock (offline first) and prints its path. Args: cargo
# package selection and target, e.g. `-p rtsim-host --test soak`.
build_test() {
    local out
    for net in --offline ""; do
        # shellcheck disable=SC2086
        if out="$(in_shell flock "$HEAVY" nice -n 19 cargo test --release -j 8 $net "$@" --no-run 2>&1)"; then
            printf '%s\n' "$out" | sed -n 's/.*Executable .*(\(.*\))$/\1/p' | tail -1
            return 0
        fi
    done
    printf '%s\n' "$out" | tail -40 >&2
    return 1
}

# Keeps the $KEEP newest nights' files.
prune() {
    local f
    command ls -1t "$STATE"/*.log 2>/dev/null | tail -n +$((KEEP + 1)) | while read -r f; do
        rm -f "$f" "${f%.log}.json"
    done
}

# One soak on seed $1 of commit $2 with the test binary $3 (file names tagged $4, if any): its
# log and report; prints the verdict line.
soak_one() {
    local seed="$1" sha="$2" bin="$3" tag="${4:-}" stem
    stem="$STATE/$(date +%F)-$sha$tag-seed$seed"
    (cd "$SRC" && nice -n 19 ionice -c 3 env \
        RTSIM_SOAK_SEED="$seed" RTSIM_SOAK_REPORT="$stem.json" RTSIM_SOAK_COMMIT="$sha" \
        ${RTSIM_SOAK_DAYS:+RTSIM_SOAK_DAYS="$RTSIM_SOAK_DAYS"} \
        ${RTSIM_SOAK_WALL_HOURS:+RTSIM_SOAK_WALL_HOURS="$RTSIM_SOAK_WALL_HOURS"} \
        "$bin" who_is_alive_soak --ignored --exact --nocapture --test-threads 1) \
        >"$stem.log" 2>&1 || true
    verdict_of "$stem.json"
}

# The verdict and failing checks a report holds ("FAIL no report" when the test died first).
verdict_of() {
    if [ ! -s "$1" ]; then
        echo "FAIL no report (the test died; see the log)"
        return
    fi
    python3 -I -c '
import json, sys
r = json.load(open(sys.argv[1]))
print(r.get("verdict", "FAIL"), " ".join(r.get("failing", [])))
' "$1"
}

code_of() {
    case "$1" in
        PASS*) echo 0 ;;
        INCOMPLETE*) echo 2 ;;
        *) echo 1 ;;
    esac
}

cmd_run() {
    refuse_unless_ready
    take_lock
    mkdir -p "$STATE"
    local sha seed bin v
    sha="$(export_main)"
    seed="${SOAK_SEED:-$((1 + 10#$(date +%j) % 8))}"
    log "commit $sha, seed $seed"
    if ! bin="$(build_test -p rtsim-host --test soak)" || [ -z "$bin" ]; then
        echo "$(date +%F) $sha seed $seed BUILD-FAILED" >"$STATE/latest"
        exit 4
    fi
    v="$(soak_one "$seed" "$sha" "$bin")"
    echo "$(date +%F) $sha seed $seed $v" >"$STATE/latest"
    log "$v"
    prune
    exit "$(code_of "$v")"
}

cmd_weekly() {
    refuse_unless_ready
    take_lock
    mkdir -p "$STATE"
    local sha every cli stem rc=0
    sha="$(export_main)"
    every="$(build_test -p rtsim-host --test every)" || exit 4
    cli="$(build_test -p rtsim-cli --bin rtsim-cli)" || exit 4
    stem="$STATE/$(date +%F)-$sha-weekly"
    log "weekly: long_horizon_stability_with_kills_30_seeds"
    (cd "$SRC" && nice -n 19 ionice -c 3 "$every" long_horizon_stability_with_kills_30_seeds \
        --ignored --exact --nocapture) >"$stem-long_horizon_stability_with_kills_30_seeds.log" 2>&1 \
        || rc=1
    # The kill-all table, alone and together, lives in rtsim-cli's `sim::killall`.
    for t in kill_all_every_kind_30_seeds kill_all_in_one_engine_8_seeds; do
        log "weekly: $t"
        (cd "$SRC" && nice -n 19 ionice -c 3 "$cli" "sim::killall::tests::$t" \
            --ignored --exact --nocapture) >"$stem-$t.log" 2>&1 || rc=1
    done
    echo "$(date +%F) $sha weekly $([ $rc = 0 ] && echo PASS || echo FAIL)" >"$STATE/latest-weekly"
    prune
    exit "$rc"
}

cmd_calibrate() {
    refuse_unless_ready
    take_lock
    mkdir -p "$STATE"
    local sha bin s pids=() worst=0 v c
    sha="$(export_main)"
    bin="$(build_test -p rtsim-host --test soak)" || exit 4
    for batch in "1 2 3 4" "5 6 7 8"; do
        pids=()
        for s in $batch; do
            soak_one "$s" "$sha" "$bin" -cal >"$STATE/.cal-$s" &
            pids+=("$!")
        done
        wait "${pids[@]}" || true
    done
    for s in 1 2 3 4 5 6 7 8; do
        v="$(cat "$STATE/.cal-$s")"
        rm -f "$STATE/.cal-$s"
        echo "seed $s: $v"
        c="$(code_of "$v")"
        [ "$c" -gt "$worst" ] && worst="$c"
    done
    exit "$worst"
}

# What the timer runs: origin/main's own copy of this script (fetched first, so a change to it
# needs no new install), `run` and on Sundays `weekly`.
cmd_nightly() {
    local r latest rc=0
    r="$(repo)"
    git -C "$r" fetch -q origin || true
    latest="$CACHE/soak-latest.sh"
    mkdir -p "$CACHE"
    if git -C "$r" show origin/main:tools/nightly/soak.sh >"$latest.new" 2>/dev/null; then
        mv "$latest.new" "$latest"
    else
        rm -f "$latest.new"
        cp "${BASH_SOURCE[0]}" "$latest"
    fi
    MIC_RTSIM_REPO="$r" bash "$latest" run || rc=$?
    if [ "$rc" != 3 ] && [ "$(date +%u)" = 7 ]; then
        MIC_RTSIM_REPO="$r" bash "$latest" weekly || true
    fi
    exit "$rc"
}

cmd_install() {
    local d bins=() b path r here
    d="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
    mkdir -p "$d"
    r="$(repo)"
    # A copy outside every checkout: a worktree can be removed, and `nightly` runs origin/main's
    # own copy anyway.
    here="${XDG_DATA_HOME:-$HOME/.local/share}/mic-rtsim/soak.sh"
    mkdir -p "$(dirname "$here")"
    cp "${BASH_SOURCE[0]}" "$here"
    chmod +x "$here"
    # The profiles' bin folders only: a shell's PATH also holds /nix/store paths a Nix GC can remove.
    for b in "$HOME/.nix-profile/bin" "/etc/profiles/per-user/$USER/bin" /run/current-system/sw/bin \
        /run/wrappers/bin /usr/bin /bin; do
        [ -d "$b" ] && bins+=("$b")
    done
    path="$(IFS=:; echo "${bins[*]}")"
    cat >"$d/mic-rtsim-soak.service" <<EOF
[Unit]
Description=mic-rtsim: the nightly who-is-alive soak (tools/nightly/soak.sh)

[Service]
Type=oneshot
Nice=19
IOSchedulingClass=idle
Environment=PATH=$path
Environment=MIC_RTSIM_REPO=$r
ExecStart=$here nightly
SuccessExitStatus=1 2 3 4
EOF
    cat >"$d/mic-rtsim-soak.timer" <<'EOF'
[Unit]
Description=mic-rtsim: the nightly soak at 03:30 (no catch-up after a missed night)

[Timer]
OnCalendar=*-*-* 03:30:00
Persistent=false

[Install]
WantedBy=timers.target
EOF
    systemctl --user daemon-reload
    systemctl --user enable --now mic-rtsim-soak.timer
    systemctl --user list-timers mic-rtsim-soak.timer --no-pager
}

case "${1:-run}" in
    run) cmd_run ;;
    weekly) cmd_weekly ;;
    calibrate) cmd_calibrate ;;
    nightly) cmd_nightly ;;
    install) cmd_install ;;
    *) sed -n '2,30p' "${BASH_SOURCE[0]}"; exit 64 ;;
esac
