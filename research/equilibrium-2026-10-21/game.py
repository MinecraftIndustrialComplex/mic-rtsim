#!/usr/bin/env python3
"""Strategy model (b) for EQUILIBRIUM.md: one goblin faction against one player base.

`ladder` mirrors rtsim-conflict at f20234e (plan_feud and escalate, crates/rtsim-conflict/src/
engine.rs 1536-1760, and data/cultures/goblin.json): grievance climbs with industry and with every
raider the player kills, each threshold crossed sends the next rung once the pool holds half the
party, and war rungs repeat at their dwell while grievance stays up. Nothing asks whether the party
can win.

`gate` keeps the grievance and its thresholds as the motive, and adds the EQUILIBRIUM.md section
3.2 rule: a war rung is sent only when its expected value, against the faction's belief of the
defence, is positive; a credible threat is first offered as a demand at the midpoint of the
bargaining range; otherwise the faction holds and probes now and then with scouts.
"""
from __future__ import annotations

import math
from dataclasses import dataclass

import numpy as np

THR = [0.0, 10.0, 30.0, 50.0, 70.0, 140.0, 260.0]     # calm notice envoy scouts raid invasion siege
DWELL = [0.0, 0.5, 0.5, 1.0, 4.0, 6.0, 8.0]
HALF_LIFE = 6.0
HYST = 0.6
KILL_W = 8.0
TRIB_PER_G = 0.5
TRIB_RELIEF = 0.7
RESPITE = 6.0
COWED = 6.0
POOL_CAP = 60.0
RECRUIT = 2.0
DEMAND_DAYS = 3.0
# party: (size, strength, carry)
PARTY = {3: (2, 1.6, 8), 4: (12, 11.2, 80), 5: (26, 30.4, 192), 6: (34, 44.0, 264)}


@dataclass
class GateParams:
    warrior_value: float = 3.0      # TUNABLE: value of a lost warrior, in loot items
    revenge_max: float = 10.0       # TUNABLE: bounded value of hurting a hated player
    revenge_half: float = 100.0
    prior_defence: float = 15.0
    belief_half_life: float = 96.0  # TUNABLE: how fast an unprobed defence is forgotten
    probe_days: float = 48.0        # TUNABLE
    stock: float = 400.0            # items at risk at the base


def battle(rng, A, d):
    pw = A * A / (A * A + d * d)
    win = rng.random() < pw
    if win:
        loss = 1 - math.sqrt(max(0.0, 1 - (d / A) ** 2)) if A > d else 0.5
    else:
        loss = min(1.0, 0.5 * max(1.0, d / A))
    return win, loss


class PlayerBase:
    def __init__(self, kind):
        self.kind = kind
        self.d = {"fortress": 80.0, "weak": 6.0, "stubborn": 6.0, "adaptive": 12.0}[kind]
        self.quiet = 0.0

    def expected_loss(self, rung, stock):
        size, strength, carry = PARTY[rung]
        return strength ** 2 / (strength ** 2 + self.d ** 2) * min(carry, stock)

    def pays(self, T, X):
        """Myopic best response: pay when the demand costs less than the attack it heads off."""
        if self.kind in ("fortress", "stubborn"):
            return False
        return T <= X + 1e-9

    def after_battle(self, lost):
        if self.kind == "adaptive" and lost:
            self.d += 6.0
        self.quiet = 0.0

    def day(self):
        if self.kind == "adaptive":
            self.quiet += 1
            if self.quiet >= 24:
                self.d = max(6.0, self.d - 1.0)
                self.quiet = 0.0


def run_one(mode, kind, seed, days=50 * 96, sigma=9.0, gp: GateParams = GateParams()):
    rng = np.random.default_rng(seed)
    pl = PlayerBase(kind)
    g = 0.0
    step = 0
    pool = POOL_CAP
    next_ok = 0.0
    cowed_until = 0.0
    belief = gp.prior_defence
    last_probe = -1e9
    demand = None            # (deadline, T, rung)
    refused_at = -1e9
    log = {k: np.zeros(days // 96 + 1) for k in
           ("war", "scouts", "envoys", "tribute", "killed", "loot", "d", "g")}
    decay = 2 ** (-1 / HALF_LIFE)

    def attack(rung, t):
        nonlocal pool, g, belief, cowed_until
        size, strength, carry = PARTY[rung]
        party = min(pool, size)
        A = strength * party / size
        win, loss = battle(rng, A, pl.d)
        dead = round(party * loss)
        pool -= dead
        g += KILL_W * dead
        y = int(t // 96)
        log["killed"][y] += dead
        if rung >= 4:
            log["war"][y] += 1
        else:
            log["scouts"][y] += 1
        if win:
            log["loot"][y] += min(carry * (1 - loss), gp.stock)
        else:
            cowed_until = t + COWED
        belief = 0.5 * belief + 0.5 * pl.d
        pl.after_battle(lost=win)

    for t in range(days):
        y = t // 96
        g = g * decay + sigma
        pool = min(POOL_CAP, pool + RECRUIT)
        pl.day()
        belief = gp.prior_defence + (belief - gp.prior_defence) * 2 ** (-1 / gp.belief_half_life)
        log["d"][y] += pl.d / 96
        log["g"][y] += g / 96
        target = max(k for k in range(7) if g >= THR[k])
        if step > 0 and g < HYST * THR[step]:
            step -= 1
        if t < cowed_until or t < next_ok:
            continue
        if mode == "ladder":
            if target > step:
                step += 1
                rung = step
            elif step >= 4:
                rung = step
            else:
                continue
            if rung == 2:
                T = math.ceil(TRIB_PER_G * g)
                log["envoys"][y] += 1
                if pl.pays(T, pl.expected_loss(4, gp.stock)):
                    log["tribute"][y] += T
                    g *= 1 - TRIB_RELIEF
                    next_ok = t + RESPITE
                    continue
            elif rung >= 3:
                size = PARTY[rung][0]
                if pool < size / 2:
                    continue
                attack(rung, t)
            next_ok = t + DWELL[rung]
            continue
        # ---- gate: best response to the believed defence
        if demand is not None:
            deadline, T, rung = demand
            if t < deadline:
                continue
            demand = None
            refused_at = t
            if pool >= PARTY[rung][0] / 2:
                attack(rung, t)
                next_ok = t + DWELL[rung]
            continue
        best, best_ev = None, 0.0
        rev = gp.revenge_max * g / (g + gp.revenge_half)
        for rung in (4, 5, 6):
            if g < THR[rung]:
                continue
            size, strength, carry = PARTY[rung]
            if pool < size / 2:
                continue
            party = min(pool, size)
            A = strength * party / size
            pw = A * A / (A * A + belief * belief)
            lw = 1 - math.sqrt(max(0.0, 1 - (belief / A) ** 2)) if A > belief else 0.5
            ll = min(1.0, 0.5 * max(1.0, belief / A))
            ev = pw * min(carry, gp.stock) - gp.warrior_value * party * (pw * lw + (1 - pw) * ll) + rev
            if ev > best_ev:
                best, best_ev = rung, ev
        if best is not None:
            size, strength, carry = PARTY[best]
            A = strength * min(pool, size) / size
            pw = A * A / (A * A + belief * belief)
            X = pw * min(carry, gp.stock)
            T = max(1, math.ceil(0.5 * (best_ev + max(X, best_ev))))
            if g >= THR[2] and t - refused_at > 24:
                log["envoys"][y] += 1
                if pl.pays(T, pl.expected_loss(best, gp.stock)):
                    log["tribute"][y] += T
                    g *= 1 - TRIB_RELIEF
                    next_ok = t + RESPITE
                else:
                    demand = (t + DEMAND_DAYS, T, best)
                continue
            attack(best, t)
            next_ok = t + DWELL[best]
            continue
        if g >= THR[3] and t - last_probe >= gp.probe_days and pool >= 1:
            last_probe = t
            attack(3, t)
            next_ok = t + DWELL[3]
    return log


def run(mode, kind, seeds=64, days=50 * 96, **kw):
    logs = [run_one(mode, kind, 1000 + s, days, **kw) for s in range(seeds)]
    return {k: np.stack([l[k] for l in logs]) for k in logs[0]}


if __name__ == "__main__":
    import sys
    sigma = float(sys.argv[1]) if len(sys.argv) > 1 else 9.0
    for mode in ("ladder", "gate"):
        for kind in ("fortress", "weak", "stubborn", "adaptive"):
            r = run(mode, kind, seeds=16, sigma=sigma)
            war = r["war"].mean(0)
            print(mode, kind, "war/yr y1-5 %.1f y41-50 %.1f" % (war[:5].mean(), war[40:50].mean()),
                  "killed/yr %.0f" % r["killed"].mean(0)[40:50].mean(),
                  "tribute/yr %.0f" % r["tribute"].mean(0)[40:50].mean(),
                  "d %.0f" % r["d"].mean(0)[40:50].mean())
