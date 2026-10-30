#!/usr/bin/env python3
"""S7 (EQUILIBRIUM.md section 4, docs/overnight-choices-equilibrium.md "S7: conflict pressure"):
the pool camp's pressure, break and refound rules, in a day-stepped model with the engine's own
recruit rates (goblin 60 at 2 a day, bandit 40 at 1.5, raider 50 at 2), to check the numbers the
design quotes. Run: python3 -I camps.py

The rules (all TUNABLE, as the design's EQC200 to EQC215 have them):
  pressure P: each warrior a player kills adds 1/cap; P halves every H days.
  recruit rate: max(r * 2^-P, 1/96) a day, with P read at the last event (the "anchor"), or read
    every step ("exact").
  break: members below 10% of cap and P >= bp, with nothing out: the camp scatters.
  refound: min(384, 48 * 2^P) days later, a quarter of the cap.
"""
import math

CAMPS = [("goblin", 60, 2.0), ("bandit", 40, 1.5), ("raider", 50, 2.0)]


def rate(r, p):
    return max(r * 2 ** (-p), 1.0 / 96.0)


def farm(cap, r, h, bp, anchored=True, years=60, every=2.0, dt=0.25):
    """A player kills every whole warrior the camp has, every `every` days."""
    pool, p, pt, ap, t = float(cap), 0.0, 0.0, 0.0, 0.0
    broken, due = False, 0.0
    breaks, broken_t, first, max_p = 0, 0.0, None, 0.0
    nxt, end = every, years * 96.0
    while t < end:
        pc = p * 2 ** (-(t - pt) / h)
        if broken:
            broken_t += dt
        else:
            pool = min(cap, pool + rate(r, ap if anchored else pc) * dt)
        t += dt
        pc = p * 2 ** (-(t - pt) / h)
        if broken and t >= due:
            broken, pool, ap = False, cap * 0.25, pc
        if t >= nxt and not broken:
            n = math.floor(pool)
            if n >= 1:
                pool -= n
                p, pt, ap = pc + n / cap, t, pc + n / cap
            nxt += every
        pc = p * 2 ** (-(t - pt) / h)
        max_p = max(max_p, pc)
        if not broken and pool < 0.1 * cap and pc >= bp:
            broken, due, pool = True, t + min(384.0, 48.0 * 2 ** pc), 0.0
            breaks += 1
            first = first if first is not None else t
    return breaks / years, broken_t / end, first, max_p


def recover(cap, r, h, anchored, dt=0.05):
    """Days to 50% and 90% of the cap after one kill-all (P 1.0)."""
    pool, t, ap, t50 = 0.0, 0.0, 1.0, None
    while t < 400:
        pc = 1.0 * 2 ** (-t / h)
        pool = min(cap, pool + rate(r, ap if anchored else pc) * dt)
        t += dt
        if t50 is None and pool >= 0.5 * cap:
            t50 = t
        if pool >= 0.9 * cap:
            return t50, t
    return t50, None


def wipes(cap, r, h, every):
    """A player wipes whatever the camp has every `every` days, from a full camp."""
    pool, p, pt, ap, t, out = float(cap), 0.0, 0.0, 0.0, 0.0, []
    for k in range(8):
        te = k * every
        pool = min(cap, pool + rate(r, ap) * (te - t))
        t = te
        n = math.floor(pool)
        pc = p * 2 ** (-(t - pt) / h)
        p, pt, ap = pc + n / cap, t, pc + n / cap
        pool -= n
        thin = pool < 0.1 * cap
        out.append((t, n, p, thin and p >= 2.0))
        if thin and p >= 2.0:
            break
    return out


if __name__ == "__main__":
    print("A. the farmed steady state P* (a player kills every whole warrior every 2 days)")
    for name, cap, r in CAMPS:
        for h in (96, 192):
            *_, mp = farm(cap, r, h, 99.0)
            print(f"  {name:7s} H {h:3d}: P reaches {mp:.2f}")
    print("B. break pressure against half-life, farmed as in A, 60 years")
    for name, cap, r in CAMPS:
        for h, bp in ((96, 2.0), (96, 1.5), (96, 1.25), (192, 2.0)):
            b, s, f, mp = farm(cap, r, h, bp)
            print(f"  {name:7s} H {h:3d} bp {bp:4.2f}: breaks/yr {b:.2f}, broken share {s:.2f}, first break day {f}")
    print("C. the recruit rate read at the last event against every step, H 192, bp 2")
    for name, cap, r in CAMPS:
        for a in (True, False):
            b, s, f, _ = farm(cap, r, 192, 2.0, anchored=a)
            print(f"  {name:7s} anchored {a!s:5s}: breaks/yr {b:.2f}, broken share {s:.2f}, first break day {f}")
    print("D. one kill-all (P 1.0): days to 50% and 90% of the cap")
    for name, cap, r in CAMPS:
        for h in (96, 192):
            for a in (True, False):
                t50, t90 = recover(cap, r, h, a)
                print(f"  {name:7s} H {h:3d} anchored {a!s:5s}: 50% day {t50:.0f}, 90% day {t90:.0f}")
    print("E. a raider outpost (cap 50, H 192) wiped every 40 days")
    for t, n, p, brk in wipes(50, 2.0, 192, 40.0):
        print(f"  day {t:4.0f}: {n:2d} killed, P after {p:.2f}, thin and P>=2: {brk}")
