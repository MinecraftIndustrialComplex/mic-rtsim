"""One-knob arms against the design (CONFLICT-LIFE.md 4.5, CL-7 (c)).

Usage: python3 knobs_an.py base.json knobs.json
base.json holds the on/off arms from paired.py; knobs.json the extra arms, run as
  ARMS='{"long": {"pact_days": 384}, "easy": {"pact_effort": 1}, "kin6": {"kin_pact": 6},
         "hate6": {"hate_pact": 6}}' python3 paired.py multi,fortress 64 long,easy,kin6,hate6 knobs.json
Each arm is paired by seed with the design ("on"): the paired difference a year in the
player's fights and loot taken, with a two-sided sign test.
"""
import collections, json, sys
from math import comb
import numpy as np

def sign_p(d):
    pos = sum(1 for x in d if x > 0); neg = sum(1 for x in d if x < 0); n = pos + neg
    if n == 0: return 1.0
    k = min(pos, neg)
    return min(1.0, 2 * sum(comb(n, i) for i in range(k + 1)) / 2 ** n)

base = json.load(open(sys.argv[1])); extra = json.load(open(sys.argv[2]))
by = collections.defaultdict(dict)
for r in base + extra: by[(r["name"], r["seed"])][r["arm"]] = r
arms = sorted({r["arm"] for r in extra})
for nm in sorted({r["name"] for r in extra}):
    players = sorted({c.split('|')[1] for r in extra if r["name"] == nm for c in r['c'] if c.startswith('on|')})
    for a in ["on"] + arms:
        keys = [k for k in by if k[0] == nm and a in by[k] and "on" in by[k]]
        rs = [by[k][a] for k in keys]; ons = [by[k]["on"] for k in keys]
        al = [r["ally"] or {} for r in rs]
        def ya(key): return float(np.mean([d.get(key, 0) for d in al])) / 50
        line = f"{nm} {a} n {len(keys)}: camp fights {np.mean([r['camp'] for r in rs]):.3f}; sworn {ya('sworn'):.4f} offers {ya('offers'):.4f} joint {ya('joint'):.4f} demands {ya('joint_demands'):.4f}"
        pd = [x for d in al for x in d.get('pact_days', [])]
        if pd: line += f"; pact median {np.median(pd):.0f}"
        ends = collections.Counter()
        for d in al: ends.update(d.get('ends', {}))
        line += f"; betrayed {ends.get('betrayed', 0) / 50 / len(al):.4f}"
        print(line)
        for p in players:
            f = [r['c'].get(f'on|{p}', 0) for r in rs]; f0 = [r['c'].get(f'on|{p}', 0) for r in ons]
            lo = [r['loot'].get(p, 0) for r in rs]; lo0 = [r['loot'].get(p, 0) for r in ons]
            jp = np.mean([r['c'].get(f'joint|{p}', 0) for r in rs]); jw = np.mean([r['c'].get(f'joint_won|{p}', 0) for r in rs])
            df = [x - y for x, y in zip(f, f0)]; dl = [x - y for x, y in zip(lo, lo0)]
            pr = collections.Counter()
            for d in al: pr.update({k: v for k, v in (d.get('pairs') or {}).items() if k.endswith(p)})
            print(f"   {p}: fights {np.mean(f):.3f} (paired {np.mean(df):+.4f}, p {sign_p(df):.2g}); loot {np.mean(lo):.2f} (paired {np.mean(dl):+.2f}, p {sign_p(dl):.2g}); joint {jp:.4f} won {jw:.4f}; pacts against {sum(pr.values())}")
