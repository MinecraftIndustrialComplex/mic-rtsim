#!/usr/bin/env python3
"""Allocation rate and GC work from a -Xlog:gc* log, over a time window.

    python3 -I gcrate.py <gc.log> [<from_uptime_s> <to_uptime_s>]

Allocation between two pauses is (heap before this pause) - (heap after the previous pause); the sum over
the window divided by its length is the allocation rate. Also prints the pause count and total pause
time, the concurrent mark cycles started, and the heap after the last pause.
"""
import re
import sys

PAUSE = re.compile(r"\[(\d+\.\d+)s\] GC\(\d+\) Pause (Young|Full|Remark|Cleanup)[^\n]*? (\d+)M->(\d+)M\((\d+)M\) ([\d.]+)ms")
CONC = re.compile(r"\[(\d+\.\d+)s\] GC\(\d+\) Concurrent Mark Cycle$")


def main():
    path = sys.argv[1]
    lo = float(sys.argv[2]) if len(sys.argv) > 2 else 0.0
    hi = float(sys.argv[3]) if len(sys.argv) > 3 else 1e18
    prev_after = None
    alloc = 0.0
    pauses = 0
    pause_ms = 0.0
    young = 0
    conc = 0
    first = last = None
    last_after = None
    for line in open(path, errors="replace"):
        m = PAUSE.search(line)
        if m:
            t = float(m.group(1))
            before, after = int(m.group(3)), int(m.group(4))
            if lo <= t <= hi:
                if prev_after is not None and m.group(2) in ("Young", "Full"):
                    alloc += max(0, before - prev_after)
                pauses += 1
                pause_ms += float(m.group(6))
                young += m.group(2) == "Young"
                first = t if first is None else first
                last = t
                last_after = after
            if m.group(2) in ("Young", "Full"):
                prev_after = after
            continue
        c = CONC.search(line.rstrip())
        if c and lo <= float(c.group(1)) <= hi:
            conc += 1
    span = (last - first) if first is not None and last is not None and last > first else 0
    print(f"window {first}..{last} s: allocated {alloc:.0f} MB, {alloc / span if span else 0:.0f} MB/s; "
          f"{pauses} pauses ({young} young) {pause_ms:.0f} ms; {conc} concurrent mark cycles; heap after last {last_after} MB")


if __name__ == "__main__":
    main()
