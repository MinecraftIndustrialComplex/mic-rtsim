#!/usr/bin/env bash
# Waits until no process mentions the te-perf tree (cmdline or cwd) for QUIET seconds.
P="/tmp/te-""perf"
QUIET=${QUIET:-600}
LOG=${LOG:-$HOME/mic-omegasrv/runs/wait-quiet.log}
quiet_since=""
while true; do
  busy=0
  for d in /proc/[0-9]*; do
    pid=${d#/proc/}
    [ "$pid" = "$$" ] && continue
    cmd=$(tr '\0' ' ' < "$d/cmdline" 2>/dev/null) || continue
    cwd=$(readlink "$d/cwd" 2>/dev/null)
    case "$cmd" in *wait-quiet.sh*) continue;; esac
    case "$cmd$cwd" in *"$P"*) busy=1; break;; esac
  done
  now=$(date +%s)
  if [ $busy = 1 ]; then
    quiet_since=""
    echo "$(date +%T) busy" >> "$LOG"
  else
    [ -z "$quiet_since" ] && quiet_since=$now
    echo "$(date +%T) quiet for $((now-quiet_since)) s" >> "$LOG"
    if [ $((now-quiet_since)) -ge $QUIET ]; then echo "$(date +%T) QUIET" >> "$LOG"; exit 0; fi
  fi
  sleep 180
done
