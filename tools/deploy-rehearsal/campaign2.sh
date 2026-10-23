#!/usr/bin/env bash
# Round 2 of the rehearsal (2026-10-23): today's pack and mic-rtsim main on the 2026-10-21 copy, the host's
# JVM arguments (scenarios.py), the glass box on. Every server boot holds the shared heavy lock, one step at a
# time (this script never has more than one waiter on it). Run it at nice 10: runsrv.py aims java at an
# absolute nice of 10. Logs to r2/campaign.log.
#
#   campaign2.sh prep|p1|p2|hippo|reload|undo|after|july|all
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
again() {  # again SRC DST [rtsim]: a reflink copy of a finished run, its old latest.log gone (runsrv's Done check)
  [ -e $R/$2 ] && return 0
  cp -a --reflink=always $R/$1 $R/$2 && rm -f $R/$2/logs/latest.log $R/$2/world/session.lock
  [ "${3:-}" = rtsim ] && cp $JARS/mic_rtsim*.jar $JARS/mic_native-*.jar $R/$2/mods/
  return 0
}
scan() { python3 -I $T/prune.py scan $R/$1/world $R/scan-$1.json > $R/scan-$1.log 2>&1; }

scen plain 26101 pack-a; scen rtsim 26111 rtsim-b
scen rtsim 26121 rtsim-a; scen plain 26131 pack-b
scen hippo 26141 hippo-on; scen hippo-off 26151 hippo-off
scen reload 26161 reload
scen uninstall 26171 uninstall; scen reset 26181 reset
scen reload 26191 reset-reload; scen reinstall 26201 reinstall
scen plain 26211 july 21.1.230; scen plain 26221 pack-c

STEP=${1:-all}
case $STEP in prep|all)
  for n in r1-pack r2-pack; do [ -e $R/$n ] || bash $T/prep.sh $n pack-base $M/world-pruned; done
  for n in r1-rtsim r2-rtsim; do [ -e $R/$n ] || bash $T/prep.sh $n pack-base $M/world-pruned rtsim; done
  for n in h-on h-off; do [ -e $R/$n ] || bash $T/prep.sh $n pack-base $M/snap-2026-10-21/server/world; done
  log "prep done";;
esac
case $STEP in p1|all) locked pair r1-pack pack-a r1-rtsim rtsim-b; scan r1-pack; scan r1-rtsim;; esac
case $STEP in p2|all) locked pair r2-rtsim rtsim-a r2-pack pack-b;; esac
case $STEP in hippo|all) locked pair h-on hippo-on h-off hippo-off;; esac
case $STEP in reload|all)  # r1-rtsim's world booted again: save, stop, reload
  again r1-rtsim r1-reload; locked run r1-reload reload; scan r1-reload;;
esac
case $STEP in undo|all)  # the uninstall (8.6) and the sim reset (SRV2), both from the reloaded world, side by side
  if [ ! -e $R/r-uninstall ]; then
    cp -a --reflink=always $R/pack-base $R/r-uninstall && cp -a --reflink=always $R/r1-reload/world $R/r-uninstall/world
    rm -f $R/r-uninstall/world/session.lock
  fi
  if [ ! -e $R/r-reset ]; then
    bash $T/prep.sh r-reset pack-base $R/r1-reload/world rtsim > /dev/null && rm -rf $R/r-reset/world/rtsim
  fi
  locked pair r-uninstall uninstall r-reset reset; scan r-uninstall;;
esac
case $STEP in after|all)  # the reset world booted again, and the uninstalled world with rtsim put back (UR-1)
  again r-reset r-reset-reload; again r-uninstall r-reinstall rtsim
  locked pair r-reset-reload reset-reload r-reinstall reinstall;;
esac
case $STEP in july)  # only if the pack upgrade's findings need re-checking: the July pack beside today's
  for n in r0-july; do [ -e $R/$n ] || bash $T/prep.sh $n july-base $M/world-pruned; done
  for n in r0-pack; do [ -e $R/$n ] || bash $T/prep.sh $n pack-base $M/world-pruned; done
  locked pair r0-july july r0-pack pack-c; scan r0-july; scan r0-pack;;
esac
log "campaign $STEP finished"
