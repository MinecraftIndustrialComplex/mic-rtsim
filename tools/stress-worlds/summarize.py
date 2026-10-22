#!/usr/bin/env python3
"""Summarise sw.py runs: per run numbers, and on-minus-off deltas over pairs.

    python3 -I summarize.py <runs_dir> <tag> [<tag> ...] [--json out.json]

A tag is a pair name given to `sw.py pair` (runs <tag>-A-<arm>, <tag>-B-<arm>). For every pair with one
"off" and one "on" arm it prints on - off per window (MSPT mean, CPU ms per tick, server thread ms per
tick); over several pairs it prints the mean delta and the range. A pair whose arms are both "off" or
both "on" is an A/A control: its delta (B - A) is the slot noise.
"""
import json
import os
import re
import statistics
import sys


def load(runs, name):
    p = os.path.join(runs, name, "sw-result.json")
    return json.load(open(p)) if os.path.exists(p) else None


def sd(xs):
    xs = [x for x in xs if x is not None]
    return statistics.stdev(xs) if len(xs) > 1 else 0.0


def glass_lines(rep):
    out = {}
    for did, txt in (rep or {}).get("info", {}).items():
        g = [ln.strip() for ln in txt.splitlines() if "glass" in ln.lower() or "compile" in ln.lower()]
        out[did] = g
    return out


def tail(xs):
    """p95 and max of the window's samples (each one neoforge tps reading, a mean over the last 100 ticks)."""
    xs = sorted(x for x in xs if x is not None)
    if not xs:
        return 0.0, 0.0
    return xs[min(len(xs) - 1, int(round(0.95 * (len(xs) - 1))))], xs[-1]


def run_line(r):
    w = r.get("windows", {})
    parts = [f"boot {r.get('boot_wall_s', 0):.0f} s wall ({r.get('boot_done_s', 0):.1f} s Done, cpu {r.get('boot_cpu_s', 0):.0f} s)"]
    for k, v in w.items():
        p95, mx = tail(v.get("mspt_samples", []))
        parts.append(f"{k}: mspt {v.get('mspt_mean') or 0:.2f} sd {sd(v.get('mspt_samples', [])):.2f} "
                     f"p95 {p95:.2f} max {mx:.2f}, "
                     f"cpu/tick {v.get('cpu_ms_per_tick') or 0:.1f}, srv/tick {v.get('server_thread_ms_per_tick') or 0:.2f}, "
                     f"tps {v.get('tps') or 0:.2f}")
    h = r.get("heap") or {}
    if h:
        parts.append(f"heap live {h.get('live_heap_mb') or 0:.0f} MB, metaspace {h.get('metaspace_used_mb') or 0:.0f} MB, "
                     f"rss {h.get('rss_mb') or 0:.0f} MB")
    if "save_flush_s" in r:
        parts.append(f"save-all flush {r['save_flush_s']:.2f} s, world {r.get('world_bytes', 0) / 1e6:.1f} MB, "
                     f"rtsim dir {r.get('rtsim_dir_bytes', 0) / 1e3:.0f} kB, data {r.get('data_files')}")
    if r.get("stop_crash"):
        parts.append("crashed while stopping: " + r["stop_crash"])
    if r.get("error"):
        parts.append("ERROR " + r["error"].strip().splitlines()[-1])
    return parts


def main():
    a = sys.argv[1:]
    out_json = None
    if "--json" in a:
        i = a.index("--json")
        out_json = a[i + 1]
        del a[i:i + 2]
    runs, tags = a[0], a[1:]
    deltas = {}
    summary = {}
    for tag in tags:
        names = sorted(n for n in os.listdir(runs) if re.fullmatch(re.escape(tag) + r"-[A-Z]-(on|off)", n))
        rs = {n: load(runs, n) for n in names}
        rs = {n: r for n, r in rs.items() if r}
        for n, r in rs.items():
            print(f"== {n}")
            for p in run_line(r):
                print("   " + p)
            for key, rep in (r.get("rtsim_reports") or {}).items():
                trains = [ln for ln in (rep.get("district_perf") or "").splitlines() if ln.startswith("trains: ")]
                print(f"   rtsim {key}: {rep.get('n_districts')} districts, "
                      f"{sum(d['inventories'] for d in rep.get('districts', []))} inventories"
                      + (f"; {trains[0]}" if trains else ""))
                tc = rep.get("tick_cost")
                if tc:
                    print(f"   rtsim {key} tick cost: {tc['ticks']} ticks, mean {tc['mean_us']:.1f} us, p50 {tc['p50_us']:.1f}, "
                          f"p99 {tc['p99_us']:.1f}, p99.9 {tc['p999_us']:.1f}, worst {tc['worst_us']:.1f} us at tick {tc['worst_at']}")
            summary[n] = r
        # A pair is one comparison; a triple off,on,on (or on,on,off) is two on-off comparisons and an
        # A/A control between the two on arms (choice SG15).
        comps = []
        offs = [n for n in names if n.endswith("-off") and n in rs]
        ons = [n for n in names if n.endswith("-on") and n in rs]
        if len(rs) == 2 and len(offs) == 1:
            comps.append((rs[ons[0]], rs[offs[0]], "on-off"))
        elif len(rs) == 2:
            a_, b_ = [rs[n] for n in names if n in rs]
            comps.append((b_, a_, "A/A (B-A)"))
        elif len(offs) == 1 and len(ons) == 2:
            comps += [(rs[ons[0]], rs[offs[0]], "on-off"), (rs[ons[1]], rs[offs[0]], "on-off"),
                      (rs[ons[1]], rs[ons[0]], "A/A (on2-on1)")]
        for on, off, kind in comps:
            for wk, wv in on.get("windows", {}).items():
                ov = off.get("windows", {}).get(wk)
                if not ov:
                    continue
                d = {m: (wv.get(m) or 0) - (ov.get(m) or 0)
                     for m in ("mspt_mean", "cpu_ms_per_tick", "server_thread_ms_per_tick")}
                deltas.setdefault((kind, wk), []).append((tag, d))
                print(f"   {tag} {kind} {wk}: dMSPT {d['mspt_mean']:+.2f}, dCPU/tick {d['cpu_ms_per_tick']:+.1f}, "
                      f"dSrv/tick {d['server_thread_ms_per_tick']:+.2f}")
    print("== deltas over pairs")
    for (kind, wk), lst in sorted(deltas.items()):
        for m in ("mspt_mean", "cpu_ms_per_tick", "server_thread_ms_per_tick"):
            xs = [d[m] for _t, d in lst]
            print(f"   {kind:10s} {wk:9s} {m:26s} mean {statistics.mean(xs):+7.2f}  range {min(xs):+7.2f}..{max(xs):+7.2f}  n={len(xs)}")
    if out_json:
        json.dump({"deltas": {f"{k[0]}|{k[1]}": v for k, v in deltas.items()}}, open(out_json, "w"), indent=1)


if __name__ == "__main__":
    main()
