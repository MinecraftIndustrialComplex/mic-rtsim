#!/usr/bin/env bash
# Round 2's follow-up runs (2026-10-23), after campaign2.sh: a third control pair (p2 ran at load 46 to 49)
# and DR1's negative control on the bases with the guard off, beside the hippocampus run again without the
# live Distant Horizons database (h-on filled the 8 GiB heap with humongous allocations as its chunks loaded:
# is it DH's database?). Same lock rules and logging as campaign2.sh.
#
#   campaign2b.sh p3|hoff|all
set -uo pipefail
M=$HOME/mic-livesrv; R=$M/r2; T=$(cd "$(dirname "$0")" && pwd)
export RUNS=$R JARS=$R/jars HIPPO=$R/hippo-chunks.json
J=/nix/store/x92dxglx7ziz2sc3wfvcyl6v2n34w4c4-openjdk-21.0.12.1+1/bin/java; S=$(cat $M/seed.txt)  # private
NEO=21.1.249
LOCK=/tmp/mic-rtsim-bridge-heavy.lock
log() { echo "$(date +%T) $*" >> $R/campaign.log; }
scen() { python3 -I $T/scenarios.py "$1" "$2" $J "${4:-$NEO}" $S "$R/scen-$3.json"; }
locked() { log "lock wait: $*"; ( flock 9; log "lock held: $*"; "$@" ) 9>$LOCK; log "lock released: $*"; }
run() {  # run DIR SCEN [TAG]
  local tag=${3:-$1}
  python3 -I $T/runsrv.py "$R/$1" "$R/scen-$2.json" "$R/out-$tag.json" > "$R/runsrv-$tag.log" 2>&1
  local rc=$?
  cp "$R/$1/logs/latest.log" "$R/log-$tag.log" 2>/dev/null
  cp "$R/$1/logs/gc.log" "$R/gc-$tag.log" 2>/dev/null
  log "done $tag exit $rc $(uptime | sed 's/.*load/load/')"
}
pair() {  # pair DIR_A SCEN_A DIR_B SCEN_B: side by side, B 5 s after A
  log "pair start $1 + $3"
  run "$1" "$2" & local a=$!
  sleep 5
  run "$3" "$4" & local b=$!
  wait $a $b
}
scen plain 26231 pack-p3; scen rtsim 26241 rtsim-p3; scen plain-hoff 26251 pack-hoff; scen hippo 26261 hippo-nodh
STEP=${1:-all}
case $STEP in p3|all)  # pack first, as p1
  [ -e $R/r3-pack ] || bash $T/prep.sh r3-pack pack-base $M/world-pruned > /dev/null
  [ -e $R/r3-rtsim ] || bash $T/prep.sh r3-rtsim pack-base $M/world-pruned rtsim > /dev/null
  locked pair r3-pack pack-p3 r3-rtsim rtsim-p3;;
esac
case $STEP in hoff|all)  # round 1's crash run: the pruned world, the bases loaded, no KubeJS script, no guard;
  # beside it h-on's run on a copy of the full world without DH's databases
  [ -e $R/r-hoff ] || bash $T/prep.sh r-hoff pack-base $M/world-pruned > /dev/null
  if [ ! -e $R/h-nodh ]; then
    bash $T/prep.sh h-nodh pack-base $M/snap-2026-10-21/server/world > /dev/null
    find $R/h-nodh/world -name 'DistantHorizons.sqlite*' -delete
  fi
  locked pair r-hoff pack-hoff h-nodh hippo-nodh;;
esac
log "campaign2b $STEP finished"
