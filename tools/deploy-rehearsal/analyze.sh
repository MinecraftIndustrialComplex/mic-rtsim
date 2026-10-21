#!/usr/bin/env bash
# analyze.sh: the rehearsal's numbers and checks from runs/, into runs/analysis/. Private output.
set -uo pipefail
M=$HOME/mic-livesrv; R=$M/runs; T=$M/tools; A=$R/analysis
mkdir -p $A
py() { python3 -I "$@"; }
[ -e $R/out-r0-control.json ] && [ -e $R/out-r0-upgrade.json ] && py $T/runreport.py --pair $R/out-r0-control.json $R/out-r0-upgrade.json > $A/pair-p0.txt 2>&1
[ -e $R/out-r1-rtsim.json ] && py $T/runreport.py --pair $R/out-r1-upgrade.json $R/out-r1-rtsim.json > $A/pair-p1.txt 2>&1
[ -e $R/out-r2-rtsim.json ] && py $T/runreport.py --pair $R/out-r2-upgrade.json $R/out-r2-rtsim.json > $A/pair-p2.txt 2>&1
for t in r1-rtsim r2-rtsim r1-rtsim-reload r-uninstall r-reset r-reset-reload; do
  [ -e $R/out-$t.json ] && py $T/runreport.py $R/out-$t.json --replies > $A/replies-$t.txt 2>&1
done
for t in r0-control r0-upgrade r1-upgrade r2-upgrade; do
  [ -e $R/out-$t.json ] && py $T/runreport.py $R/out-$t.json --replies > $A/replies-$t.txt 2>&1
done
# log groups: upgrade against control, rtsim against the upgrade run beside it, uninstall against rtsim
[ -e $R/log-r0-upgrade.log ] && py $T/logsum.py $R/log-r0-upgrade.log $R/log-r0-control.log > $A/logsum-upgrade-vs-control.txt 2>&1
[ -e $R/log-r0-control.log ] && py $T/logsum.py $R/log-r0-control.log > $A/logsum-control.txt 2>&1
[ -e $R/log-r1-rtsim.log ] && py $T/logsum.py $R/log-r1-rtsim.log $R/log-r1-upgrade.log > $A/logsum-rtsim-vs-upgrade.txt 2>&1
[ -e $R/log-r1-rtsim-reload.log ] && py $T/logsum.py $R/log-r1-rtsim-reload.log $R/log-r1-rtsim.log > $A/logsum-reload-vs-rtsim.txt 2>&1
[ -e $R/log-r-uninstall.log ] && py $T/logsum.py $R/log-r-uninstall.log $R/log-r1-upgrade.log > $A/logsum-uninstall-vs-upgrade.txt 2>&1
[ -e $R/log-r-reset.log ] && py $T/logsum.py $R/log-r-reset.log $R/log-r1-rtsim.log > $A/logsum-reset-vs-rtsim.txt 2>&1
[ -e $R/log-r-reset-reload.log ] && py $T/logsum.py $R/log-r-reset-reload.log $R/log-r-reset.log > $A/logsum-reset-reload-vs-reset.txt 2>&1
# mixins: every applied mic_rtsim mixin, from the verbose reload log
[ -e $R/log-r1-rtsim-reload.log ] && grep -oE 'Mixing [A-Za-z0-9_.$]+ from mic_rtsim[a-z_]*\.mixins\.json into [A-Za-z0-9_.$/]+' $R/log-r1-rtsim-reload.log | sort -u > $A/mixins-applied.txt
[ -e $R/log-r-reset.log ] && grep -oE 'Mixing [A-Za-z0-9_.$]+ from mic_rtsim[a-z_]*\.mixins\.json into [A-Za-z0-9_.$/]+' $R/log-r-reset.log | sort -u > $A/mixins-applied-reset.txt
# the longest stretches behind, by phase (boot, bases loading, settling, sampling, stop)
args=(); for t in r0-control r0-upgrade r1-upgrade r1-rtsim r2-upgrade r2-rtsim r1-rtsim-reload r-uninstall r-reset r-reset-reload; do
  [ -e $R/out-$t.json ] && [ -e $R/log-$t.log ] && args+=($R/out-$t.json $R/log-$t.log)
done
py $T/spikes.py "${args[@]}" > $A/spikes.txt 2>&1
command ls $A
