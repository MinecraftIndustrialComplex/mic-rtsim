#!/usr/bin/env bash
#
# mic_rtsim's headless GameTest suite (and the pure-Java unit tests first).
#
#   ./gametest.sh                 run both, the server's engine the Rust one (builds rtsim-jni)
#   ./gametest.sh -PrtsimEngine=stub -PskipJni   the same tests on the pure-Java stub, no cargo
#   ./gametest.sh -PgametestDebugLog   with DEBUG logging (shows "Mixing ... into ..." lines)
#
# Extra arguments go to Gradle as they are. Boots a dedicated NeoForge server with mic_rtsim and
# mic_native (the host of the Rust engine) on it, runs every @GameTest in the mic_rtsim namespace, and
# exits non-zero if anything failed. The full log is left in core/build/gametest.log.
#
# Two things can fail, and they fail differently:
#   * a mixin that no longer matches its target: mic_rtsim.mixins.json is defaultRequire 1, so the
#     launch dies before a test runs. Look for "InjectionError" or "MixinTransformerError";
#   * an assertion: every test logs what it compared on a "[gametest]" line.
#
# Heavy-job etiquette on the shared laptop: one heavy job at a time (the flock), niced, 4 workers.

set -uo pipefail
cd "$(dirname "$0")"

LOG=core/build/gametest.log
mkdir -p core/build

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

start=$(date +%s)
flock "$LOCK" nice -n 10 bash -c "$(declare -f run); run ./gradlew --max-workers=4 --console=plain :core:test :core:runGameTestServer $*" 2>&1 | tee "$LOG"
status=${PIPESTATUS[0]}
elapsed=$(( $(date +%s) - start ))

echo
echo "--------------------------------------------------------------------"
if ! grep -q "GAME TESTS COMPLETE\|All [0-9]* required tests passed" "$LOG"; then
    echo "FAILED: the server never got as far as finishing the tests."
    grep -nE "FAILED|Failed to start|MixinTransformerError|InjectionError|InvalidInjectionException|Mod loading has failed" "$LOG" | head -20
    echo "Full log: $LOG"
    exit 1
fi
grep -E "BoundaryProtocolTest > |GAME TESTS COMPLETE|required tests (failed|passed)|tests are now running|::.*(failed|passed)|\[gametest\]" "$LOG" | tail -60
echo
echo "Wall time: ${elapsed}s   Full log: $LOG"
exit "$status"
