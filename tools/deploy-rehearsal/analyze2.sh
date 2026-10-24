#!/usr/bin/env bash
# analyze2.sh: round 2's numbers and checks from r2/, into r2/analysis/. Private output; safe to rerun as steps finish.
set -uo pipefail
M=$HOME/mic-livesrv; R=$M/r2; T=$(cd "$(dirname "$0")" && pwd); A=$R/analysis
mkdir -p $A
py() { python3 -I "$@"; }
have() { for f in "$@"; do [ -e "$f" ] || return 1; done; }
# MSPT, B minus A per sample, side by side
have $R/out-r1-pack.json $R/out-r1-rtsim.json && py $T/runreport.py --pair $R/out-r1-pack.json $R/out-r1-rtsim.json > $A/pair-p1.txt 2>&1
have $R/out-r2-pack.json $R/out-r2-rtsim.json && py $T/runreport.py --pair $R/out-r2-pack.json $R/out-r2-rtsim.json > $A/pair-p2.txt 2>&1
have $R/out-h-off.json $R/out-h-on.json && py $T/runreport.py --pair $R/out-h-off.json $R/out-h-on.json > $A/pair-hippo.txt 2>&1
have $R/out-r-uninstall.json $R/out-r-reset.json && py $T/runreport.py --pair $R/out-r-uninstall.json $R/out-r-reset.json > $A/pair-undo.txt 2>&1
have $R/out-r-reinstall.json $R/out-r-reset-reload.json && py $T/runreport.py --pair $R/out-r-reinstall.json $R/out-r-reset-reload.json > $A/pair-after.txt 2>&1
have $R/out-r0-july.json $R/out-r0-pack.json && py $T/runreport.py --pair $R/out-r0-july.json $R/out-r0-pack.json > $A/pair-july.txt 2>&1
have $R/out-r3-pack.json $R/out-r3-rtsim.json && py $T/runreport.py --pair $R/out-r3-pack.json $R/out-r3-rtsim.json > $A/pair-p3.txt 2>&1
have $R/out-h-on.json $R/out-h-nodh.json && py $T/runreport.py --pair $R/out-h-on.json $R/out-h-nodh.json > $A/pair-nodh.txt 2>&1
ALL=(r1-pack r1-rtsim r2-rtsim r2-pack h-on h-off r1-reload r-uninstall r-reset r-reset-reload r-reinstall r3-pack r3-rtsim r-hoff h-nodh r0-july r0-pack)
RT=(r1-rtsim r2-rtsim r1-reload r-reset r-reset-reload r-reinstall r3-rtsim)
for t in "${ALL[@]}"; do
  have $R/out-$t.json && py $T/runreport.py $R/out-$t.json --replies > $A/replies-$t.txt 2>&1
done
# the glass box and the hand-overs, per sample
args=(); for t in "${RT[@]}"; do have $R/out-$t.json && args+=($R/out-$t.json); done
[ ${#args[@]} -gt 0 ] && py $T/glassreads.py "${args[@]}" > $A/glass.txt 2>&1
# log groups: today's pack against round 1's upgrade run, rtsim against the pack run beside it, and so on
have $R/log-r1-pack.log $M/runs/log-r1-upgrade.log && py $T/logsum.py $R/log-r1-pack.log $M/runs/log-r1-upgrade.log > $A/logsum-pack-vs-round1.txt 2>&1
have $R/log-r1-pack.log && py $T/logsum.py $R/log-r1-pack.log > $A/logsum-pack.txt 2>&1
have $R/log-r1-rtsim.log $R/log-r1-pack.log && py $T/logsum.py $R/log-r1-rtsim.log $R/log-r1-pack.log > $A/logsum-rtsim-vs-pack.txt 2>&1
have $R/log-r2-rtsim.log $R/log-r2-pack.log && py $T/logsum.py $R/log-r2-rtsim.log $R/log-r2-pack.log > $A/logsum-rtsim2-vs-pack2.txt 2>&1
have $R/log-r1-rtsim.log $M/runs/log-r1-rtsim.log && py $T/logsum.py $R/log-r1-rtsim.log $M/runs/log-r1-rtsim.log > $A/logsum-rtsim-vs-round1.txt 2>&1
have $R/log-h-on.log $R/log-h-off.log && py $T/logsum.py $R/log-h-on.log $R/log-h-off.log > $A/logsum-hon-vs-hoff.txt 2>&1
have $R/log-h-off.log $R/log-h-on.log && py $T/logsum.py $R/log-h-off.log $R/log-h-on.log > $A/logsum-hoff-vs-hon.txt 2>&1
have $R/log-r1-reload.log $R/log-r1-rtsim.log && py $T/logsum.py $R/log-r1-reload.log $R/log-r1-rtsim.log > $A/logsum-reload-vs-rtsim.txt 2>&1
have $R/log-r-uninstall.log $R/log-r1-pack.log && py $T/logsum.py $R/log-r-uninstall.log $R/log-r1-pack.log > $A/logsum-uninstall-vs-pack.txt 2>&1
have $R/log-r-reset.log $R/log-r1-rtsim.log && py $T/logsum.py $R/log-r-reset.log $R/log-r1-rtsim.log > $A/logsum-reset-vs-rtsim.txt 2>&1
have $R/log-r-reset-reload.log $R/log-r-reset.log && py $T/logsum.py $R/log-r-reset-reload.log $R/log-r-reset.log > $A/logsum-reset-reload-vs-reset.txt 2>&1
have $R/log-r-reinstall.log $R/log-r1-rtsim.log && py $T/logsum.py $R/log-r-reinstall.log $R/log-r1-rtsim.log > $A/logsum-reinstall-vs-rtsim.txt 2>&1
have $R/log-r3-rtsim.log $R/log-r3-pack.log && py $T/logsum.py $R/log-r3-rtsim.log $R/log-r3-pack.log > $A/logsum-rtsim3-vs-pack3.txt 2>&1
have $R/log-r-hoff.log $R/log-r1-pack.log && py $T/logsum.py $R/log-r-hoff.log $R/log-r1-pack.log > $A/logsum-hoff-vs-pack.txt 2>&1
have $R/log-h-nodh.log $R/log-h-on.log && py $T/logsum.py $R/log-h-nodh.log $R/log-h-on.log > $A/logsum-nodh-vs-hon.txt 2>&1
have $R/log-r0-pack.log $R/log-r0-july.log && py $T/logsum.py $R/log-r0-pack.log $R/log-r0-july.log > $A/logsum-pack-vs-july.txt 2>&1
# mixins applied (verbose runs)
for t in r1-reload r-reset r-reinstall h-on h-off h-nodh; do
  have $R/log-$t.log && grep -oE 'Mixing [A-Za-z0-9_.$]+ from mic_[a-z_]*\.mixins\.json into [A-Za-z0-9_.$/]+' $R/log-$t.log | sort -u > $A/mixins-$t.txt
done
# the checks, one file: DR1, DR4, DR9 and the uninstall and reset lines
{
  for t in "${ALL[@]}"; do
    f=$R/log-$t.log; have $f || continue
    echo "== $t"
    echo "hippocampus guard lines: $(grep -c 'hippocampus' $f)"; grep -m3 'hippocampus' $f | cut -c1-240
    echo "HippocampusExplorationGoalMixin: $(grep -c 'HippocampusExplorationGoalMixin' $f)"
    echo "DR4 invalid return: $(grep -c 'invalid return' $f)"
    echo "DR4 symbol '/': $(grep -c "Pattern references symbol '/'" $f)"
    echo "DR4 create_power_loader.js lines: $(grep -c 'create_power_loader' $f)"; grep -m3 'create_power_loader' $f | cut -c1-240
    echo "DR4 radar_integration.js lines: $(grep -c 'radar_integration' $f)"; grep -m3 'radar_integration' $f | cut -c1-240
    echo "DR4 copycat_catwalk lines: $(grep -c 'copycat_catwalk' $f)"; grep -m3 'copycat_catwalk' $f | cut -c1-240
    echo "DR4 petrochem lines: $(grep -ci 'petrochem' $f)"; grep -i 'petrochem' $f | grep -iE 'error|warn|fail' | head -3 | cut -c1-240
    echo "final save failed: $(grep -ci 'final save failed' $f)"; grep -i 'final save failed' $f | head -3 | cut -c1-240
    echo "re-announce (observations lost): $(grep -c 'lack observations the journal no longer holds' $f)"
    grep -m4 'lack observations the journal no longer holds' $f | cut -c1-240
    echo "crash/exception lines: $(grep -cE 'Exception|Error:' $f)"
    echo "Done line: $(grep -m1 -oE 'Done \([0-9.]+s\)' $f)"
  done
} > $A/checks.txt 2>&1
# gamerule replies (C8a) and the entity census, from the runs' replies
for t in "${ALL[@]}"; do have $R/out-$t.json && py -c '
import json, sys
o = json.load(open(sys.argv[1]))
print("==", sys.argv[2])
for s in o["steps"]:
    c = s["step"].get("cmd", "")
    if c.startswith(("gamerule", "execute if entity")) and "reply" in s:
        print("  [%.0f] %s: %s" % (s["t"], c, s["reply"].strip()))
' $R/out-$t.json $t; done > $A/gamerule-census.txt 2>&1
# GC: the live set after each full or young collection's end, the largest pause
for t in "${ALL[@]}"; do
  f=$R/gc-$t.log; have $f || continue
  echo "== $t $(grep -c 'Pause' $f) pauses; full: $(grep -c 'Pause Full' $f)"
  grep -oE 'Pause (Young|Full|Remark|Cleanup)[^)]*\)[^>]*->[0-9]+M\([0-9]+M\) [0-9.]+ms' $f | awk '{ms=$NF; sub("ms","",ms); if (ms+0>mx) {mx=ms+0; l=$0}} END {print "longest: " l}'
  grep -oE '[0-9]+M->[0-9]+M\([0-9]+M\)' $f | tail -1 | sed 's/^/last: /'
done > $A/gc.txt 2>&1
# the longest stretches behind, by phase
args=(); for t in "${ALL[@]}"; do have $R/out-$t.json $R/log-$t.log && args+=($R/out-$t.json $R/log-$t.log); done
[ ${#args[@]} -gt 0 ] && py $T/spikes.py "${args[@]}" > $A/spikes.txt 2>&1
# block entities: each scanned world against the pruned start
for t in r1-pack r1-rtsim r1-reload r-uninstall r0-july r0-pack; do
  have $R/scan-$t.json && py $T/becompare.py $M/scan-pruned-b.json $R/scan-$t.json > $A/becompare-$t.txt 2>&1
done
command ls $A
