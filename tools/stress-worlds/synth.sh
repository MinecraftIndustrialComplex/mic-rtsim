#!/usr/bin/env bash
#
# The synthetic stress world (DECISIONS.md ST1 and ST3; docs/stress-worlds-results.md section 10):
# builds it with the create plugin's stressGen GameTest run and files it under
# ~/mic-stress-worlds/synthetic/<name>, ready for sw.py.
#
#   tools/stress-worlds/synth.sh <name> [KEY=VALUE ...]
#
#   DISTRICTS=40    districts (at least 2)
#   CELLS=32        factories per district (8 per machine chunk column)
#   TRAINS=-1       cargo trains; -1 is two per pair of neighbouring districts (at most four)
#   SEED=1          picks factory kinds and each line's two goods
#   DWELL=10        seconds each train waits at a station
#   KINDS=millstone:4,press:2,saw:2,brass:1   factory kinds and weights
#   XMX=12G         the GameTest server's heap
#
# Then, for example:
#   tools/stress-worlds/sw.py pair syn40 --world ~/mic-stress-worlds/synthetic/<name>/world \
#       --scene ~/mic-stress-worlds/scenes/synthetic-<name>.json --jars <jar dir>
#
# What it does: deletes neoforge/create/run-stressgen/world, runs the generator under the bridge
# lock (nice 10), checks that the test passed, copies the world, and strips what the build session
# left that a fresh pack server should make itself: the stub engine's rtsim state, the GameTest
# server's serverconfig (so the pack's defaultconfigs apply) and the experimental datapacks the
# GameTest server enables (bundle, trade_rebalance). The world keeps the GameTest world's flat
# terrain and game rules (no mob spawning, weather or random ticks). Choices: the SG rows, section 10.7.
#
# ~/mic-stress-worlds/synthetic is made a btrfs subvolume (/home's hourly snapshots skip it). Worlds
# there are never committed.

set -euo pipefail
[ $# -ge 1 ] || { sed -n '3,30p' "$0"; exit 64; }
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
name=$1
shift
case "$name" in *[!A-Za-z0-9._-]*) echo "synth.sh: name $name: letters, digits, . _ - only"; exit 64 ;; esac
xmx=12G
for kv in "$@"; do
    k=${kv%%=*}
    v=${kv#*=}
    case "$k" in
        DISTRICTS|CELLS|TRAINS|SEED|DWELL|KINDS) export "MIC_STRESS_$k=$v" ;;
        XMX) xmx=$v ;;
        *) echo "synth.sh: unknown setting $k"; exit 64 ;;
    esac
done
export MIC_STRESS_NAME=$name

root=${MIC_STRESS_ROOT:-$HOME/mic-stress-worlds}
mkdir -p "$root" "$root/scenes"
if [ ! -d "$root/synthetic" ]; then
    nix shell nixpkgs#btrfs-progs -c btrfs subvolume create "$root/synthetic" || mkdir -p "$root/synthetic"
fi

nf=$repo/neoforge
log=$nf/create/build/stressgen-$name.log
mkdir -p "$nf/create/build"
rm -rf "$nf/create/run-stressgen/world"
echo "synth.sh: building $name (log $log)"
start=$(date +%s)
(cd "$nf" && flock /tmp/mic-rtsim-bridge-heavy.lock nice -n 10 nix develop "$repo#" -c \
    ./gradlew --max-workers=4 --console=plain :create:runStressGen -PskipJni "-PstressXmx=$xmx") > "$log" 2>&1 || true
if ! grep -q "\[stressgen\] $name built in" "$log" || ! grep -q "All 1 required tests passed" "$log"; then
    echo "synth.sh: the build failed; see $log"
    grep -nE "\[stressgen\]|FAILED|Exception" "$log" | tail -20
    exit 1
fi
grep -E "\[stressgen\]" "$log" | sed 's/^.*\[stressgen\] /  /'

out=$root/synthetic/$name
rm -rf "$out"
mkdir -p "$out"
cp -a --reflink=auto "$nf/create/run-stressgen/world" "$out/world"
w=$out/world
rm -rf "$w/rtsim" "$w/serverconfig" "$w/session.lock"
find "$w/data" -maxdepth 1 -name 'mic_rtsim*' -delete
nix shell --impure --expr 'let p = import (builtins.getFlake "nixpkgs") {}; in p.python3.withPackages (ps: [ps.nbtlib])' \
    -c python3 -I - "$w/level.dat" <<'EOF'
import sys
import nbtlib
from nbtlib import List, String

f = nbtlib.load(sys.argv[1])
d = f["Data"]
experimental = {"bundle", "trade_rebalance"}
enabled = [str(x) for x in d["DataPacks"]["Enabled"] if str(x) not in experimental]
disabled = sorted({str(x) for x in d["DataPacks"]["Disabled"]} | experimental)
d["DataPacks"]["Enabled"] = List[String](enabled)
d["DataPacks"]["Disabled"] = List[String](disabled)
d["enabled_features"] = List[String]([x for x in d["enabled_features"] if str(x).split(":")[-1] not in experimental])
f.save()
print("  level.dat: datapacks", enabled, "features", [str(x) for x in d["enabled_features"]])
EOF
cp "$w/stressgen.json" "$out/stressgen.json"
cp "$w/stressgen-scene.json" "$root/scenes/synthetic-$name.json"
echo "synth.sh: $out/world ($(du -sh "$w" | cut -f1)), scene $root/scenes/synthetic-$name.json, $(( $(date +%s) - start )) s"
