#!/usr/bin/env python3
"""S9 (EQUILIBRIUM.md section 4, docs/choices-mowzie-build.md "M12 S9"): the grove's warriors and the
Umvuthi on the shared renewal rule, in an event-stepped model with the engine's own rules, to check the
numbers the design quotes. Run: python3 -I mowzie.py

The rules, as the engine has them (crates/rtsim-conflict/src/engine/camp.rs plan_arrival, camp_arrival):
  pressure P: each warrior a player kills adds 1/K, each Umvuthi 1; P halves every H days.
  a site that is short (whole warriors at home + away < K) books ONE wait, min(cap, base * 2^P) days with
  P read when the wait starts; a running wait is never pushed out; when it ends one warrior arrives and,
  if the site is still short, the next wait starts at once (so arrivals chain).
  the Umvuthi: after a death, an heir is due at max(cowed_until, min(192, 6 * 2^Pc)) days, and takes a
  whole warrior (the grove falls by one, no pressure) when one is home.
All three farmers below are scripted players; every number is a game day (a game year is 96 days).
"""
import math
import random
import sys

H = 96.0


def decay(p, dt, h=H):
    return p * 2 ** (-dt / h)


class Grove:
    """Warriors of one grove under the rule. `shared` makes the Umvuthi's pressure the warriors' too."""

    def __init__(self, k, base=6.0, cap=96.0, h=H, n=None):
        self.k, self.base, self.cap, self.h = k, base, cap, h
        self.n = k if n is None else n
        self.p, self.tp = 0.0, 0.0
        self.wait_end = None
        self.arrivals = 0
        self.t = 0.0

    def P(self, t):
        return decay(self.p, t - self.tp, self.h)

    def bump(self, t, amount):
        self.p, self.tp = self.P(t) + amount, t

    def plan(self, t):
        if self.n < self.k and self.wait_end is None:
            self.wait_end = t + min(self.cap, self.base * 2 ** self.P(t))
        elif self.n >= self.k:
            self.wait_end = None

    def lose(self, t, count, player=True):
        count = min(count, self.n)
        self.n -= count
        if player and count:
            self.bump(t, count / self.k)
        self.plan(t)
        return count

    def arrive(self):
        t = self.wait_end
        self.wait_end = None
        self.n += 1
        self.arrivals += 1
        self.plan(t)
        return t


def run_to(g, t_end):
    """Arrivals up to t_end, in order."""
    while g.wait_end is not None and g.wait_end <= t_end:
        g.arrive()


# ---- the den's closed form (EQUILIBRIUM.md section 3.3) ----------------------------------------------

def farmed_closed_form(k, base, cap, c, h=H):
    """P* after a kill and the days per arrival for a site farmed one member at a time, c days after it
    arrives (EQUILIBRIUM.md 3.3): P* = (1/k) / (1 - 2^(-gap/h)), gap = min(cap, base * 2^P*) + c."""
    lo, hi = 0.0, 50.0
    for _ in range(200):
        mid = (lo + hi) / 2
        gap = min(cap, base * 2 ** mid) + c
        f = (1.0 / k) / (1 - 2 ** (-gap / h))
        if f > mid:
            lo = mid
        else:
            hi = mid
    p = (lo + hi) / 2
    return p, min(cap, base * 2 ** p) + c


# ---- kill all -----------------------------------------------------------------------------------------

def kill_all(k, base, cap=96.0):
    g = Grove(k, base, cap)
    g.lose(0.0, k)
    out = {}
    marks = {"50%": math.ceil(0.5 * k), "90%": math.ceil(0.9 * k), "all": k}
    while g.n < k:
        t = g.arrive()
        for name, need in marks.items():
            if name not in out and g.n >= need:
                out[name] = t
    return out


# ---- farming ------------------------------------------------------------------------------------------

def farm_each_arrival(k, base, cap, c, years=60, h=H):
    """The table's farmer: each arrival is killed c days after it comes (one at a time)."""
    g = Grove(k, base, cap, h, n=0)
    g.p, g.tp = 1.0, 0.0
    g.plan(0.0)
    end = years * 96.0
    kills, lastk = 0, 0.0
    # arrivals chain; every arrival is killed c days later (a kill of an arrival that is still there)
    pending = []  # kill times
    t = 0.0
    while True:
        nxt_arr = g.wait_end if g.wait_end is not None else 1e18
        nxt_kill = pending[0] if pending else 1e18
        if min(nxt_arr, nxt_kill) > end:
            break
        if nxt_arr <= nxt_kill:
            t = g.arrive()
            pending.append(t + c)
            pending.sort()
        else:
            t = pending.pop(0)
            g.lose(t, 1)
            kills += 1
    return kills / years, g.P(end)


def farm_visits(k, base, cap, every, years=60, h=H, start_full=True):
    """The realistic farmer: visits every `every` days and kills every whole warrior the grove has."""
    g = Grove(k, base, cap, h)
    end = years * 96.0
    t, kills = 0.0, 0
    nxt = every
    pmax = 0.0
    while nxt <= end:
        run_to(g, nxt)
        t = nxt
        n = g.n
        if n:
            g.lose(t, n)
            kills += n
        pmax = max(pmax, g.P(t))
        nxt += every
    return kills / years, g.P(end), pmax


# ---- unattended: the grove's own losses -----------------------------------------------------------------

def unattended(k, base, cap, mean_gap, q, years=60, seeds=40, h=H, mood=False):
    """No player. The grove plunders every mean_gap days (exponential); the pack, one pool unit, is lost
    with probability q (Lanchester: a pack of strength 2.6 against a village of pop >= 5 never comes home).
    The loss is the faction's (no pressure) but the grove is short until the chain refills it."""
    bands = []
    for s in range(seeds):
        rnd = random.Random(s)
        g = Grove(k, base, cap, h)
        end = years * 96.0
        t = 0.0
        area = 0.0
        grief, gt = 0.0, 0.0  # the plunder mood's grief (S7, EQC287): a share of the cap lost, halving in 96 d

        def gap(now):
            if not mood:
                return rnd.expovariate(1.0 / mean_gap)
            gr = grief * 2 ** (-(now - gt) / 96.0)
            return rnd.expovariate(1.0 / (mean_gap * min(3.0, max(0.5, math.exp(2.8 * gr)))))

        nxt = gap(0.0)
        while t < end:
            ta = g.wait_end if g.wait_end is not None else 1e18
            te = min(ta, nxt, end)
            area += g.n * (te - t)
            t = te
            if t >= end:
                break
            if ta <= nxt:
                g.arrive()
            else:
                if rnd.random() < q and g.n > 0:
                    g.lose(t, 1, player=False)
                    grief, gt = grief * 2 ** (-(t - gt) / 96.0) + 1.0 / k, t
                nxt = t + gap(t)
        bands.append(area / (end * k))
    bands.sort()
    return sum(bands) / len(bands), bands[len(bands) // 20], bands[-len(bands) // 20 - 1]


# ---- the Umvuthi's cycle -------------------------------------------------------------------------------

def umvuthi_farm(k, base, cap, shared, years=60, c=2.0, cowed=6.0, h=H, warriors_die=True,
                 chief_base=6.0, chief_cap=192.0):
    """A farmer who kills the Umvuthi `c` days after each heir is seated. The Umvuthi's `die` kills the
    warriors near it (BY_NONE: no one's loss, MZB-253); the heir takes one warrior home (no pressure)."""
    g = Grove(k, base, cap, h)
    pc_p, pc_t = 0.0, 0.0  # the chief's own pressure (used when not shared)
    end = years * 96.0
    t = 0.0
    seats = 0
    area, last = 0.0, 0.0
    chief = True  # alive, seated at t=0
    kill_at = c
    heir_due = None
    pending_seat_check = None
    while t < end:
        cand = [end]
        if chief:
            cand.append(kill_at)
        if heir_due is not None:
            cand.append(heir_due)
        if g.wait_end is not None:
            cand.append(g.wait_end)
        te = min(cand)
        if te >= end:
            break
        area += g.n * (te - last)
        last = t = te
        if g.wait_end is not None and abs(g.wait_end - t) < 1e-9:
            g.arrive()
            continue
        if chief and abs(kill_at - t) < 1e-9:
            chief = False
            if shared:
                g.bump(t, 1.0)
                pc = g.P(t)
            else:
                pc_p, pc_t = decay(pc_p, t - pc_t, h) + 1.0, t
                pc = pc_p
            if warriors_die:
                g.lose(t, g.n, player=False)
            heir_due = max(t + cowed, t + min(chief_cap, chief_base * 2 ** pc))
            continue
        if heir_due is not None and abs(heir_due - t) < 1e-9:
            if g.n >= 1:
                g.n -= 1
                g.plan(t)
                chief = True
                seats += 1
                kill_at = t + c
                heir_due = None
            else:
                # waits for a warrior: look again at the next arrival
                heir_due = g.wait_end if g.wait_end is not None else t + 1.0
    area += g.n * (end - last)
    return seats / years, area / (end * k)


def main():
    print("== the den (K 1, base 6, cap 192, H 96), farmed one member at a time ==")
    for c in (0.05, 2.0, 7.0):
        p, gap = farmed_closed_form(1, 6.0, 192.0, c)
        print(f"  c {c:5.2f} d: P* {p:.2f}, a kill every {gap:6.2f} d, {96.0 / gap:.2f} a game year")

    print("== grove kill-all: days to 50%, 90% and all of K, by base (cap 96) ==")
    for base in (2.0, 3.0, 6.0, 12.0):
        row = []
        for k in (5, 7, 9):
            r = kill_all(k, base)
            row.append(f"K{k}: {r['50%']:.0f}/{r['90%']:.0f}/{r['all']:.0f}")
        print(f"  base {base:4.1f}: " + "  ".join(row))

    print("== grove farmed, warriors per game year (K 7, cap 96) ==")
    for base in (2.0, 3.0, 6.0, 12.0):
        p, gap = farmed_closed_form(7, base, 96.0, 2.0)
        table = 96.0 / gap
        each, pe = farm_each_arrival(7, base, 96.0, 2.0)
        v2, p2, pm2 = farm_visits(7, base, 96.0, 2.0)
        v8, p8, pm8 = farm_visits(7, base, 96.0, 8.0)
        v24, p24, pm24 = farm_visits(7, base, 96.0, 24.0)
        print(f"  base {base:4.1f}: table {table:5.2f} (P* {p:.2f}); each arrival killed after 2 d {each:5.2f} (P {pe:.2f}); "
              f"visits every 2 d {v2:5.2f}, 8 d {v8:5.2f}, 24 d {v24:5.2f}")

    print("== grove farmed, by size (base 6, visits every 2 d) ==")
    for k in (5, 7, 9):
        p, gap = farmed_closed_form(k, 6.0, 96.0, 2.0)
        v2, p2, pm2 = farm_visits(k, 6.0, 96.0, 2.0)
        each, pe = farm_each_arrival(k, 6.0, 96.0, 2.0)
        print(f"  K {k}: table {96.0 / gap:5.2f}, each arrival {each:5.2f}, visits {v2:5.2f}")

    print("== grove with no player: its own plunder loses its pack (mean gap 12 d), share of K held ==")
    for base in (3.0, 6.0, 12.0):
        row = []
        for k in (5, 7, 9):
            m, lo, hi = unattended(k, base, 96.0, 12.0, 1.0)
            row.append(f"K{k} {m:.3f} [{lo:.3f},{hi:.3f}]")
        print(f"  base {base:4.1f}, q 1.0: " + "  ".join(row))
    for q in (0.25, 0.5):
        m, lo, hi = unattended(7, 6.0, 96.0, 12.0, q)
        print(f"  base  6.0, q {q}: K7 {m:.3f} [{lo:.3f},{hi:.3f}]")
    m, lo, hi = unattended(7, 0.5, 96.0, 12.0, 1.0)
    print(f"  base  0.5 (today's 2 a day), q 1.0: K7 {m:.3f}")
    row = []
    for k in (5, 7, 9):
        m, lo, hi = unattended(k, 6.0, 96.0, 12.0, 1.0, mood=True)
        row.append(f"K{k} {m:.3f} [{lo:.3f},{hi:.3f}]")
    print("  base  6.0, q 1.0, with the raider's plunder mood (grief 2.8, clamp 0.5 to 3): " + "  ".join(row))

    print("== a grove of 5 to 9 (each size alike), base 6: days to 90%, and the share held with no player ==")
    mix90 = sum(kill_all(k, 6.0)["90%"] for k in (5, 6, 7, 8, 9)) / 5
    mixu = sum(unattended(k, 6.0, 96.0, 12.0, 1.0)[0] for k in (5, 6, 7, 8, 9)) / 5
    print(f"  90% back in {mix90:.0f} d on average; held {mixu:.3f} of K")

    print("== the Umvuthi farmed (Sol Visages per game year; K 7, base 6; its death kills the warriors near it) ==")
    pc, gap = farmed_closed_form(1, 6.0, 192.0, 2.0)
    for shared in (False, True):
        y, fill = umvuthi_farm(7, 6.0, 96.0, shared)
        ys, fills = umvuthi_farm(7, 6.0, 96.0, shared, warriors_die=False)
        print(f"  {'one pressure for both' if shared else 'a pressure each      '}: {y:5.2f} a game year, "
              f"warriors held {fill:.2f} of K (warriors spared by the death: {ys:5.2f}, {fills:.2f}); "
              f"closed form {96.0 / gap:.2f}")
    # a warrior farmer's pressure on the heir's wait, if the two shared one pressure
    for p_w in (1.27, 1.41):
        print(f"  a warrior farmer at P {p_w}: the next Umvuthi's heir waits {min(192, 6 * 2 ** (1 + p_w)):.1f} d on one "
              f"pressure against 12.0 d on its own")


if __name__ == "__main__":
    sys.exit(main())
