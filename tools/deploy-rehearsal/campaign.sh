#!/usr/bin/env bash
# The rehearsal campaign: three side-by-side pairs, then reload and uninstall. Logs to runs/campaign.log.
set -uo pipefail
M=$HOME/mic-livesrv; R=$M/runs; T=$M/tools
J=/nix/store/x92dxglx7ziz2sc3wfvcyl6v2n34w4c4-openjdk-21.0.12.1+1/bin/java; S=$(cat $M/seed.txt)  # private
log() { echo "$(date +%T) $*" >> $R/campaign.log; }
scen() { python3 -I $T/scenarios.py "$1" "$2" $J "$3" $S "$R/scen-$4.json" "$5"; }
A=0,2,4,6; B=8,10,12,14  # four physical cores each (siblings are adjacent on this laptop)
run() {  # run DIR SCEN [TAG]
  local tag=${3:-$1}
  python3 -I $T/runsrv.py "$R/$1" "$R/scen-$2.json" "$R/out-$tag.json" > "$R/runsrv-$tag.log" 2>&1
  local rc=$?
  cp "$R/$1/logs/latest.log" "$R/log-$tag.log" 2>/dev/null
  log "done $tag exit $rc"
}
pair() {  # pair NAME_A SCEN_A NAME_B SCEN_B
  log "pair start $1 + $3"
  run "$1" "$2" & a=$!
  sleep 5
  run "$3" "$4" & b=$!
  wait $a $b
}
STEP=${1:-all}
# each pair splits A and B; rtsim and upgrade swap sets between p1 and p2
scen plain 25871 21.1.230 control $A; scen plain 25881 21.1.249 upgrade $B; scen rtsim 25891 21.1.249 rtsim $A
scen plain 25901 21.1.249 upgrade2 $A; scen rtsim 25911 21.1.249 rtsim2 $B
scen reload 25891 21.1.249 reload $A; scen uninstall 25921 21.1.249 uninstall $A; scen reset 25931 21.1.249 reset $A
case $STEP in all|p0) pair r0-control control r0-upgrade upgrade;; esac
case $STEP in all|p1) pair r1-upgrade upgrade r1-rtsim rtsim;; esac
case $STEP in all|p2) pair r2-rtsim rtsim2 r2-upgrade upgrade2;; esac
case $STEP in all|reload) log "reload start"; run r1-rtsim reload r1-rtsim-reload;; esac
case $STEP in all|uninstall)
  log "uninstall start"
  [ -e $R/r-uninstall ] || { cp -a --reflink=always $R/upgrade-base $R/r-uninstall && cp -a --reflink=always $R/r1-rtsim/world $R/r-uninstall/world; }
  run r-uninstall uninstall;;
esac
case $STEP in all|reset)  # SRV2's sim reset: the rtsim world after reload, with world/rtsim deleted, rtsim installed
  log "reset start"
  [ -e $R/r-reset ] || { bash $T/prep.sh r-reset upgrade-base $R/r1-rtsim/world rtsim > /dev/null && rm -rf $R/r-reset/world/rtsim; }
  run r-reset reset;;
esac
log "campaign $STEP finished"
