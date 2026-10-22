import os
import json, collections, sys, math
import numpy as np
rows = json.load(open(sys.argv[1]))
by = collections.defaultdict(dict)
for r in rows:
    by[(r["name"], r["seed"])][r["arm"]] = r
names = sorted({r["name"] for r in rows})
arms = sorted({r["arm"] for r in rows})
def wilcoxon_sign(d):
    d = [x for x in d if abs(x) > 1e-12]
    n = len(d)
    if n == 0: return 1.0
    pos = sum(1 for x in d if x > 0)
    # sign test two-sided
    from math import comb
    k = min(pos, n - pos)
    p = 2 * sum(comb(n, i) for i in range(k + 1)) / 2 ** n
    return min(1.0, p)
for nm in names:
    keys = [k for k in by if k[0] == nm]
    print(f"== {nm} ({len(keys)} seeds)")
    allc = set()
    for k in keys:
        for a in arms:
            if a in by[k]: allc |= set(by[k][a]["c"])
    def mean(a, f):
        xs = [f(by[k][a]) for k in keys if a in by[k]]
        return np.mean(xs) if xs else float('nan')
    for a in arms:
        print(f"  {a}: fights {mean(a, lambda r: r['fights']):.3f} camp {mean(a, lambda r: r['camp']):.3f} "
              f"extinct {mean(a, lambda r: r['extinct']):.3f} endless {mean(a, lambda r: r['endless']):.3f}")
    # players
    ps = sorted({p for k in keys for a in arms if a in by[k] for p in by[k][a]['by_player']} |
                {c.split('|')[1] for c in allc if c.startswith('on|')})
    for p in ps:
        for a in arms:
            on = mean(a, lambda r: r['c'].get(f'on|{p}', 0))
            lost = mean(a, lambda r: r['c'].get(f'on_lost|{p}', 0))
            loot = mean(a, lambda r: r['loot'].get(p, 0))
            print(f"  {p} {a}: fights/yr {on:.3f} lost/yr {lost:.3f} share lost {lost/on if on else float('nan'):.2f} loot {loot:.2f}")
        if 'on' in arms and 'off' in arms:
            d = [by[k]['on']['c'].get(f'on|{p}', 0) - by[k]['off']['c'].get(f'on|{p}', 0) for k in keys]
            print(f"    paired on-off fights/yr {np.mean(d):+.4f} (sd {np.std(d):.4f}, same {sum(1 for x in d if abs(x)<1e-12)}/{len(d)}, sign p {wilcoxon_sign(d):.2f})")
    # Frostmaw
    fr = sorted({c for c in allc if c.startswith('frost_rail|')})
    for a in arms:
        s = ", ".join(f"{c.split('|')[1]} {mean(a, lambda r, c=c: r['c'].get(c, 0)):.3f}" for c in fr)
        kf = ", ".join(f"{c.split('|')[1]} {mean(a, lambda r, c=c: r['c'].get(c, 0)):.3f}" for c in sorted(allc) if c.startswith('killed|'))
        print(f"  {a} frost_rail: {s}; gob_rail {mean(a, lambda r: r['c'].get('gob_rail', 0)):.3f}")
        print(f"  {a} killed: {kf}")
        ff = sorted({c for c in allc if c.startswith('fight|F')})
        print(f"  {a} frostmaw fights: " + ", ".join(f"{c[6:]} {mean(a, lambda r, c=c: r['c'].get(c, 0)):.3f}" for c in ff))
        jt = sorted({c for c in allc if c.startswith('joint|')})
        print(f"  {a} joint: " + ", ".join(f"{c[6:]} {mean(a, lambda r, c=c: r['c'].get(c, 0)):.4f} (won {mean(a, lambda r, c=c: r['c'].get('joint_won|'+c[6:], 0)):.4f})" for c in jt))
