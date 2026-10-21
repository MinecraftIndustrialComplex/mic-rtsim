#!/usr/bin/env bash
# Every 60 s, for each rehearsal server (java with cwd under runs/), appends GC.heap_info and VmRSS to runs/heap-<dir>.log.
J=/nix/store/x92dxglx7ziz2sc3wfvcyl6v2n34w4c4-openjdk-21.0.12.1+1/bin
R=$HOME/mic-livesrv/runs
while true; do
  for d in /proc/[0-9]*; do
    pid=${d#/proc/}
    c=$(readlink $d/cwd 2>/dev/null) || continue
    case "$c" in $R/r*) ;; *) continue;; esac
    grep -q java $d/comm 2>/dev/null || continue
    n=$(basename "$c")
    { echo "=== $(date +%T) pid $pid $(grep VmRSS $d/status)"; nice -n 10 $J/jcmd $pid GC.heap_info 2>&1 | head -12; } >> $R/heap-$n.log
  done
  sleep 60
done
