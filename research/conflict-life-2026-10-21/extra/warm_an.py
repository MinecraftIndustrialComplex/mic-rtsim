import os
import json, collections, sys
import numpy as np
base = json.load(open(sys.argv[1])); warm = json.load(open(sys.argv[2]))
rows = base + warm
by = collections.defaultdict(dict)
for r in rows: by[(r["name"], r["seed"])][r["arm"]] = r
for nm in sorted({r["name"] for r in warm}):
    keys = [k for k in by if k[0] == nm and "warm" in by[k]]
    print("==", nm, len(keys))
    for a in ("off", "on", "warm"):
        rs = [by[k][a] for k in keys]
        al = [r["ally"] or {} for r in rs]
        akeys = sorted({x for d in al for x in d})
        def m(f): return np.mean([f(r) for r in rs])
        print(f" {a}: fights {m(lambda r: r['fights']):.3f} camp {m(lambda r: r['camp']):.3f} extinct {m(lambda r: r['extinct']):.3f} endless {m(lambda r: r['endless']):.3f}")
        jt = collections.Counter(); jw = collections.Counter()
        for r in rs:
            for c, v in r["c"].items():
                if c.startswith("joint|"): jt[c[6:]] += v / len(rs)
                if c.startswith("joint_won|"): jw[c[10:]] += v / len(rs)
        print("   joint:", {k: (round(v, 4), round(jw[k], 4)) for k, v in jt.items()})
        for p in sorted({c.split('|')[1] for r in rs for c in r['c'] if c.startswith('on|')}):
            on = m(lambda r: r['c'].get(f'on|{p}', 0)); lo = m(lambda r: r['c'].get(f'on_lost|{p}', 0))
            print(f"   {p}: fights {on:.3f} lost {lo:.3f} share {lo/on:.2f} loot {m(lambda r: r['loot'].get(p,0)):.2f}")
        if akeys:
            num = [k for k in akeys if all(isinstance(d.get(k, 0), (int, float)) for d in al)]
            print("   ally a year:", {k: round(float(np.mean([d.get(k, 0) for d in al])) / 50, 4) for k in num if k != 'years'})
            pdays = [x for d in al for x in d.get('pact_days', [])]
            if pdays: print("   pact days median", np.median(pdays), "mean", round(np.mean(pdays)), "n", len(pdays))
            ends = collections.Counter()
            for d in al: ends.update(d.get('ends', {}))
            print("   ends a world-year:", {k: round(v / 50 / len(al), 4) for k, v in ends.items()})
            pr = collections.Counter()
            for d in al: pr.update(d.get('pairs', {}))
            print("   pairs:", pr.most_common(6))
