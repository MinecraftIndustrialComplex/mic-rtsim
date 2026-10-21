#!/usr/bin/env python3
"""Coupled population model for EQUILIBRIUM.md (mic-rtsim renewal and equilibrium).

A stochastic model, stepped every `dt` game days, of forage and herds, dragons and lairs, human
villages, Ribbit villages, conflict camps, Frostmaw dens, Umvuthana groves with their Umvuthi, oil
nests and pirate havens on a 12 x 12 grid of 512-block regions (6,144 blocks square), vectorised
over seeds with numpy.

Two modes:

  baseline  mirrors the engine at origin/main f20234e (research/population-inventory-2026-10-21.md):
            no density term in herd births, permanent herd extinction, shared grazing, a 20-day
            lair respawn with no pressure and no starve-leave, villages that never refill and a sim
            population that only ratchets up, Ribbit villages that only grow with new houses, crew
            pools that only grow by captures, and no renewal for bosses or nests (no code exists).
  proposed  the rules in EQUILIBRIUM.md section 2.

It models the rules' shape (fixed points, bounds, recovery, knife edges), not the engine's exact
numbers. Every rate EQUILIBRIUM.md marks TUNABLE is a field of Params.
"""
from __future__ import annotations

from dataclasses import dataclass

import numpy as np

YEAR = 96          # game days per Serene Seasons year (4 seasons x 3 sub-seasons x 8 days)
G = 12             # regions per side, 512 blocks each
CY, CX = 6, 6      # the world spawn region
INF = np.inf

SPECIES = ("sheep", "cow", "pig", "horse", "goat")
FERT = np.array([0.55, 0.40, 1.20, 0.30, 0.70])
CEIL_F = np.ceil(FERT)
M_YOUNG = np.array([0.25, 0.18, 0.35, 0.15, 0.30])     # goat is a model placeholder
M_ADULT = np.array([0.06, 0.04, 0.08, 0.03, 0.07])
M_OLD = np.array([0.25, 0.20, 0.30, 0.15, 0.25])
M_MIX = 0.75 * M_ADULT + 0.25 * M_OLD                  # adults and old as one class
DENSITY = np.array([24, 10, 12, 6, 8], dtype=float)


def bh_k_over_m(sp):
    """Deterministic equilibrium of the yearly Beverton-Holt map, per unit of M (homogeneous)."""
    s_y = np.exp(-M_YOUNG[sp])
    s_a = np.exp(-M_MIX[sp])
    a = s_y * FERT[sp] / (1 - s_a) - 1.0
    return a * (1 + (1 - s_a) / s_y)


@dataclass
class Params:
    mode: str = "proposed"
    dt: int = 1
    # ---- herds and forage
    k_per_density: float = 160.0        # K at habitat 1 = density x this
    forage_cap: float = 32_000.0        # 16 cells x 2,000 at habitat 1 (engine)
    forage_growth: float = 0.1          # per day (engine)
    forage_bite: float = 0.2            # share of standing forage edible per day
    claim_penalty: float = 0.4          # engine
    msy_headroom: float = 2.5           # forage regrowth at its best feeds 2.5 founding K
    forage_seed: float = 0.01           # regrowth floor: share of capacity seeded per day from outside
    starve_hazard: float = 0.2          # per day at zero satisfaction (engine)
    birth_doy: int = 24
    bh: bool = True                     # TUNABLE (rule): density term in births
    disperse_frac: float = 0.05         # TUNABLE: share of a donor herd's adults that leave each spring
    disperse_donor: float = 0.4         # TUNABLE: a herd above this share of K gives emigrants
    disperse_thin: float = 0.25         # TUNABLE: a neighbour below this share of K takes them
    frontier_rate: float = 0.25         # TUNABLE: wanderers found a fallow region, per region-year
    loaded_regrow_rate: float = 0.0     # per region-year while loaded (not in the proposed rule; a sweep)
    founders: int = 12                  # TUNABLE: a frontier herd's size
    founders_frac: float = 0.05         # TUNABLE: a frontier herd is max(founders, this x K)
    refuge_frac: float = 0.10           # TUNABLE: share of K dragons cannot reach
    refuge_min: int = 8
    # ---- dragons
    lair_rate: float = 0.7              # lairs per known region (IaF roost + cave sets, 0.5 chance)
    dragon_eat: float = 0.6             # animals per day: 8 fullness/day over 13 per beast
    dragon_half: float = 200.0          # prey weight at which a dragon eats half its need
    alt_village: float = 300.0          # village livestock and caravans in reach (P12)
    alt_wild: float = 30.0
    lifespan: int = 400
    arrive_age: int = 30
    lair_base: float = 20.0             # TUNABLE (engine 20)
    lair_cap: float = 192.0             # TUNABLE
    starve_leave: float = 0.35          # TUNABLE: fed share below which a dragon abandons its lair
    arrive_food: float = 0.5            # TUNABLE: food share a lair needs before a newcomer settles
    fed_tau: float = 30.0
    # ---- pressure, shared by every site
    pressure: bool = True
    H: float = 96.0                     # TUNABLE: pressure half-life, days
    # ---- bosses, members, nests
    den_base: float = 24.0              # TUNABLE
    den_cap: float = 192.0
    grove_base: float = 6.0             # TUNABLE: per member
    grove_cap: float = 96.0
    chief_base: float = 24.0            # TUNABLE
    chief_cap: float = 192.0
    nest_base: float = 12.0             # TUNABLE: per worm
    nest_cap: float = 96.0
    # ---- villages
    village_rate: float = 0.25
    ribbit_rate: float = 0.06
    breed_loaded: float = 0.03          # vanilla breeding while loaded, per villager-day
    zombie_loss: float = 0.004          # loaded losses per villager-day
    grow_unloaded: float = 0.01         # TUNABLE: sim growth toward beds while unloaded
    settler_base: float = 16.0          # TUNABLE
    settler_cap: float = 192.0
    settler_reach: int = 3              # regions (1,536 blocks)
    ribbit_clock: float = 10.0          # engine: one house and one ribbit per 10 prosperous days
    ribbit_extra: int = 8
    # ---- conflict camps
    camp_rate: float = 0.08
    feud_days: float = 14.0
    camp_refound_base: float = 48.0     # TUNABLE
    camp_refound_cap: float = 384.0
    camp_break_p: float = 2.0           # TUNABLE: pressure at which a starved camp breaks
    # ---- pirates
    haven_rate: float = 0.03
    ship_slots: int = 2
    crew: int = 6
    crew_cap: int = 18                  # TUNABLE
    recruit: float = 0.25               # TUNABLE: drifters per day at the haven
    ship_base: float = 4.0              # TUNABLE
    ship_cap: float = 96.0
    hunt_p: float = 0.1
    # ---- sites per region
    den_rate: float = 0.08
    grove_rate: float = 0.08
    nest_rate: float = 0.05

    @property
    def proposed(self):
        return self.mode == "proposed"


# --------------------------------------------------------------------------- helpers

def shift(a, dy, dx):
    """out[..., y, x] = a[..., y - dy, x - dx]; zero outside the grid."""
    out = np.zeros_like(a)
    ys = slice(max(dy, 0), G + min(dy, 0))
    yd = slice(max(-dy, 0), G + min(-dy, 0))
    xs = slice(max(dx, 0), G + min(dx, 0))
    xd = slice(max(-dx, 0), G + min(-dx, 0))
    out[..., ys, xs] = a[..., yd, xd]
    return out


DIR4 = [(-1, 0), (1, 0), (0, -1), (0, 1)]
DIR9 = [(dy, dx) for dy in (-1, 0, 1) for dx in (-1, 0, 1)]
YY, XX = np.mgrid[0:G, 0:G]


def cheb(cy, cx):
    return np.maximum(np.abs(YY - cy), np.abs(XX - cx))


def near(mask, r):
    """True where any cell of `mask` lies within Chebyshev distance r."""
    out = mask.copy()
    for dy in range(-r, r + 1):
        for dx in range(-r, r + 1):
            if dy or dx:
                out |= shift(mask, dy, dx)
    return out


class Pressure:
    """A decaying count of player-caused losses: P(t) = P(t0) 2^-((t - t0) / H)."""

    def __init__(self, shape, p: Params):
        self.v = np.zeros(shape)
        self.t = np.zeros(shape)
        self.H = p.H
        self.on = p.pressure

    def now(self, t):
        return self.v * np.exp2(-(t - self.t) / self.H) if self.on else np.zeros_like(self.v)

    def bump(self, mask, amount, t):
        if not self.on:
            return
        cur = self.now(t)
        self.v = np.where(mask, cur + amount, self.v)
        self.t = np.where(mask, t, self.t)

    def delay(self, t, base, cap):
        return np.minimum(cap, base * np.exp2(self.now(t)))


class Sites:
    """A site with K members that refill one at a time, each after min(cap, base 2^P)."""

    def __init__(self, exists, K, base, cap, p: Params, renew=True):
        self.exists = exists
        self.K = np.where(exists, K, 0).astype(np.int64)
        self.n = self.K.copy()
        self.base, self.cap, self.renew = base, cap, renew
        self.P = Pressure(exists.shape, p)
        self.due = np.full(exists.shape, INF)
        self.events = 0
        self.kills = np.zeros(exists.shape)

    def lose(self, mask, count, t, by_player):
        count = np.where(mask, np.minimum(count, self.n), 0)
        hit = count > 0
        self.n = self.n - count
        if by_player:
            self.P.bump(hit, count / np.maximum(self.K, 1), t)
            self.kills += count
        if self.renew:
            start = hit & np.isinf(self.due)
            self.due = np.where(start, t + self.P.delay(t, self.base, self.cap), self.due)

    def step(self, t, active, gate=None):
        ready = self.exists & active & (self.n < self.K) & (self.due <= t)
        if gate is not None:
            ready &= gate
        self.n = self.n + ready
        self.events += int(ready.sum())
        more = ready & (self.n < self.K)
        self.due = np.where(more, t + self.P.delay(t, self.base, self.cap),
                            np.where(ready, INF, self.due))
        return ready


# --------------------------------------------------------------------------- world

class World:
    def __init__(self, p: Params, S: int, seed: int, player):
        self.p, self.S = p, S
        rng = self.rng = np.random.default_rng(seed)
        self.player = player
        shp = (S, G, G)
        # biomes in 3 x 3-region patches pick the founding species
        coarse = rng.integers(0, 5, size=(S, 4, 4))
        self.sp = np.repeat(np.repeat(coarse, 3, axis=1), 3, axis=2)
        hab = rng.beta(2, 2, size=shp)
        hab[rng.random(shp) < 0.2] = 0.0
        self.hab = hab
        self.C = p.forage_cap * hab
        self.F = self.C.copy()
        self.set_species(np.ones(shp, bool))
        self.founded = np.zeros(shp, bool)
        self.tomb = np.zeros(shp, bool)
        self.Y = np.zeros(shp, np.int64)
        self.A = np.zeros(shp, np.int64)
        self.satE = np.ones(shp)
        self.herd_events = 0
        self.extinctions = 0
        far = cheb(CY, CX)[None] >= 2           # nothing hostile within ~1,000 blocks of spawn
        # dragons: up to two lairs per region
        nl = np.minimum(rng.poisson(p.lair_rate, size=shp), 2) * far
        self.lair_ex = np.stack([nl >= 1, nl >= 2], axis=-1)
        self.lair_occ = self.lair_ex.copy()
        self.lair_age = rng.integers(30, p.lifespan, size=shp + (2,))
        self.lair_due = np.full(shp + (2,), INF)
        self.lair_P = Pressure(shp + (2,), p)
        self.lair_fed = np.ones(shp + (2,))
        self.lair_kills = np.zeros(shp + (2,))
        self.lair_events = 0
        # other sites
        def draw(rate, mask=None):
            m = rng.random(shp) < rate
            return m & mask if mask is not None else m
        renew = p.proposed
        self.dens = Sites(draw(p.den_rate, far), np.ones(shp), p.den_base, p.den_cap, p, renew)
        grove_k = rng.integers(5, 10, size=shp)
        self.groves = Sites(draw(p.grove_rate, far), grove_k, p.grove_base, p.grove_cap, p, renew)
        self.chiefs = Sites(self.groves.exists, np.ones(shp), p.chief_base, p.chief_cap, p, renew)
        self.nests = Sites(draw(p.nest_rate, far), np.full(shp, 2), p.nest_base, p.nest_cap, p, renew)
        # villages
        self.v_ex = draw(p.village_rate)
        self.v_K = np.where(self.v_ex, rng.integers(6, 21, size=shp), 0)
        self.v_pop = np.where(self.v_ex, np.ceil(self.v_K * rng.uniform(0.6, 1.0, size=shp)), 0).astype(np.int64)
        self.v_sim = self.v_pop.copy()          # baseline: the sim's ratcheting belief
        self.v_P = Pressure(shp, p)
        self.v_due = np.full(shp, INF)
        self.v_events = 0
        self.v_kills = np.zeros(shp)
        self.r_ex = draw(p.ribbit_rate) & ~self.v_ex
        self.r_K0 = np.where(self.r_ex, rng.integers(4, 13, size=shp), 0)
        self.r_houses = self.r_K0.copy()
        self.r_pop = self.r_houses.copy().astype(np.int64)
        self.r_P = Pressure(shp, p)
        self.r_due = np.full(shp, INF)
        self.r_clock = rng.uniform(0, p.ribbit_clock, size=shp)
        self.r_events = 0
        # conflict camps
        self.c_ex = draw(p.camp_rate, far)
        culture = rng.integers(0, 3, size=shp)
        self.c_culture = culture
        self.c_cap = np.where(self.c_ex, np.array([60, 40, 50])[culture], 0).astype(float)
        self.c_r = np.array([2.0, 1.5, 2.0])[culture]
        self.c_pool = self.c_cap.copy()
        self.c_live = self.c_ex.copy()
        self.c_P = Pressure(shp, p)
        self.c_due = np.full(shp, INF)
        self.c_next = rng.uniform(0, p.feud_days, size=shp)
        self.c_raids = 0
        self.c_lost = 0.0
        self.c_events = 0
        # pirate havens
        self.h_ex = draw(p.haven_rate, far)
        self.h_pool = np.where(self.h_ex, p.crew if not p.proposed else p.crew_cap, 0).astype(float)
        self.h_ships = np.zeros(shp + (p.ship_slots,), bool)
        self.h_due = np.where(self.h_ex[..., None], 0.0, INF) * np.ones(shp + (p.ship_slots,))
        self.h_P = Pressure(shp, p)
        self.h_routes = rng.uniform(0.3, 0.9, size=shp + (3,))   # p_win per merchant route
        self.h_merch_due = np.zeros(shp + (3,))
        self.h_events = 0
        self.h_captures = 0
        self.known = np.zeros(shp, bool)

    def set_species(self, mask):
        p = self.p
        sp = self.sp
        K = np.maximum(DENSITY[sp] * p.k_per_density * self.hab, np.where(self.hab > 0, 8.0, 0.0))
        self.K = np.where(mask, K, getattr(self, "K", K))
        self.M = self.K / bh_k_over_m(sp)
        self.q = (p.forage_growth * p.forage_cap / 4) / (p.msy_headroom * DENSITY[sp] * p.k_per_density)
        self.R = np.maximum(p.refuge_min, p.refuge_frac * self.K)

    # ------------------------------------------------------------------ discovery
    def discover(self, known):
        new = known & ~self.known
        self.known = known
        nh = new & (self.hab > 0) & ~self.founded
        if nh.any():
            n0 = np.clip(np.round(self.K * self.rng.uniform(0.6, 1.0, size=self.K.shape)), 8, 4000)
            self.A = np.where(nh, n0.astype(np.int64), self.A)
            self.founded |= nh

    # ------------------------------------------------------------------ herds
    def graze(self, dt):
        p = self.p
        N = self.Y + self.A
        F = self.F
        F += p.forage_growth * dt * (F + p.forage_seed * self.C) * (1 - F / np.maximum(self.C, 1e-9))
        np.clip(F, 0, self.C, out=F)
        D = self.q * N * dt
        wn = [p.claim_penalty * shift(F, -dy, -dx) for dy, dx in DIR4]
        wsum = F + sum(wn)
        safe = np.where(wsum > 0, wsum, 1.0)
        d_self = np.where(wsum > 0, D * F / safe, 0.0)
        d_k = [np.where(wsum > 0, D * w / safe, 0.0) for w in wn]
        T = d_self + sum(shift(d, dy, dx) for d, (dy, dx) in zip(d_k, DIR4))
        avail = np.minimum(F, p.forage_bite * dt * F)
        phi = np.where(T > 0, np.minimum(1.0, avail / np.where(T > 0, T, 1.0)), 1.0)
        eaten = d_self * phi + sum(d * shift(phi, -dy, -dx) for d, (dy, dx) in zip(d_k, DIR4))
        self.F = F - T * phi
        sat = np.where(D > 0, eaten / np.where(D > 0, D, 1.0), 1.0)
        self.satE += (sat - self.satE) * min(1.0, dt / 30.0)
        return sat

    def herd_deaths(self, sat, dt):
        p = self.p
        st = p.starve_hazard * dt * np.clip(1 - sat / 0.5, 0, 1)
        py = 1 - (1 - np.minimum(1, M_YOUNG[self.sp] / YEAR * dt)) * (1 - st)
        pa = 1 - (1 - np.minimum(1, M_MIX[self.sp] / YEAR * dt)) * (1 - st)
        self.Y = self.Y - self.rng.binomial(self.Y, py)
        self.A = self.A - self.rng.binomial(self.A, pa)

    def births(self):
        p = self.p
        self.A = self.A + self.Y
        self.Y = np.zeros_like(self.Y)
        N = self.A
        dd = 1.0 / (1.0 + N / np.maximum(self.M, 1e-9)) if (p.proposed and p.bh) else 1.0
        prob = np.clip(FERT[self.sp] * self.satE * dd / CEIL_F[self.sp], 0, 1)
        self.Y = self.rng.binomial(self.A * CEIL_F[self.sp].astype(np.int64), prob)
        self.herd_events += int((self.Y > 0).sum())

    def disperse(self, t, loaded_year):
        """Spring: surplus walks to fallow or thin neighbours; wanderers refound fallow ground."""
        p = self.p
        N = self.Y + self.A
        live = self.founded & (N > 0)
        thin = self.founded & (self.hab > 0) & (N < p.disperse_thin * self.K)
        donors = live & (N > p.disperse_donor * self.K)
        # emigrants split over the thin 4-neighbours
        n_thin = sum(shift(thin.astype(float), -dy, -dx) for dy, dx in DIR4)
        emig = np.where(donors & (n_thin > 0), self.rng.binomial(self.A, p.disperse_frac), 0)
        self.A = self.A - emig
        share = emig / np.maximum(n_thin, 1)
        arrive = np.zeros_like(share)
        arrive_sp = np.full(self.sp.shape, -1)
        for dy, dx in DIR4:
            got = shift(share, dy, dx) * thin
            sp_in = shift(self.sp.astype(float) + 1, dy, dx).astype(int) - 1
            takes = (got > 0) & (arrive_sp < 0)
            arrive_sp = np.where(takes, sp_in, arrive_sp)
            arrive += got
        arrive = np.floor(arrive).astype(np.int64)
        fallow = thin & (N == 0)
        recol = fallow & (arrive > 0) & (arrive_sp >= 0)
        if recol.any():
            self.sp = np.where(recol, arrive_sp, self.sp)
            self.set_species(recol)
        self.A = self.A + arrive
        # wanderers from beyond the explored edge, and vanilla spawns a loaded region's census sees
        fallow = self.founded & (self.hab > 0) & (self.Y + self.A == 0)
        rate = p.frontier_rate + np.where(loaded_year, p.loaded_regrow_rate, 0.0)
        hit = fallow & (self.rng.random(fallow.shape) < 1 - np.exp(-rate))
        n_found = np.maximum(p.founders, np.round(p.founders_frac * self.K)).astype(np.int64)
        self.A = np.where(hit, n_found, self.A)
        self.herd_events += int(recol.sum() + hit.sum())

    def hunt(self, dt):
        """Dragons eat from herds in the 3 x 3 regions around their lair and from village stock."""
        p = self.p
        N = self.Y + self.A
        if p.proposed:
            avail = np.maximum(N - self.R, 0).astype(float)
            w = avail * avail / np.maximum(self.K, 1)        # type III: switching to what is common
        else:
            avail = N.astype(float)
            w = avail
        vill = near(self.v_ex & (self.v_pop > 0), 1)
        alt = np.where(vill, p.alt_village, p.alt_wild)
        Wq = alt + sum(shift(w, -dy, -dx) for dy, dx in DIR9)
        iota = Wq / (Wq + p.dragon_half)
        occ = self.lair_occ.sum(-1)
        c = occ * p.dragon_eat * dt * iota / Wq
        demand = w * sum(shift(c, dy, dx) for dy, dx in DIR9)
        kills = np.minimum(self.rng.poisson(demand), avail.astype(np.int64))
        ky = self.rng.binomial(kills, np.where(N > 0, self.Y / np.maximum(N, 1), 0))
        self.Y = self.Y - np.minimum(ky, self.Y)
        self.A = self.A - np.minimum(kills - ky, self.A)
        a = min(1.0, dt / p.fed_tau)
        self.lair_fed += (iota[..., None] - self.lair_fed) * a * self.lair_occ
        return iota

    def mark_extinct(self):
        N = self.Y + self.A
        gone = self.founded & (self.hab > 0) & (N == 0) & ~self.tomb
        newly = gone & ~getattr(self, "_was_gone", np.zeros_like(gone))
        self.extinctions += int(newly.sum())
        self._was_gone = gone
        if not self.p.proposed:
            self.tomb |= gone

    # ------------------------------------------------------------------ dragons
    def lairs(self, t, dt, iota):
        p = self.p
        occ = self.lair_occ
        self.lair_age = self.lair_age + dt * occ
        old = occ & (self.lair_age >= p.lifespan)
        leave = np.zeros_like(old)
        if p.proposed:
            leave = occ & (self.lair_fed < p.starve_leave) & (self.lair_age > 60)
        go = old | leave
        delay = (self.lair_P.delay(t, p.lair_base, p.lair_cap) if p.proposed
                 else np.full(go.shape, p.lair_base))
        self.lair_occ = occ & ~go
        self.lair_due = np.where(go, t + delay, self.lair_due)
        ready = self.lair_ex & ~self.lair_occ & (self.lair_due <= t) & self.known[..., None]
        if p.proposed:
            fed_ok = (iota >= p.arrive_food)[..., None]
            wait = ready & ~fed_ok
            self.lair_due = np.where(wait, t + 24, self.lair_due)
            ready &= fed_ok
        self.lair_occ = self.lair_occ | ready
        self.lair_age = np.where(ready, p.arrive_age, self.lair_age)
        self.lair_fed = np.where(ready, 1.0, self.lair_fed)
        self.lair_due = np.where(ready, INF, self.lair_due)
        self.lair_events += int(ready.sum())

    def kill_dragons(self, mask, t):
        p = self.p
        hit = mask & self.lair_occ
        self.lair_occ = self.lair_occ & ~hit
        self.lair_P.bump(hit, 1.0, t)
        delay = (self.lair_P.delay(t, p.lair_base, p.lair_cap) if p.proposed
                 else np.full(hit.shape, p.lair_base))
        self.lair_due = np.where(hit, t + delay, self.lair_due)
        self.lair_kills += hit

    # ------------------------------------------------------------------ villages
    def villages(self, t, dt, loaded):
        p, rng = self.p, self.rng
        pop, K = self.v_pop, self.v_K
        lv = self.v_ex & loaded
        room = np.clip(1 - pop / np.maximum(K, 1), 0, 1)
        born = np.where(lv & (pop >= 2), rng.poisson(p.breed_loaded * dt * pop * room), 0)
        lost = np.where(lv, rng.binomial(pop, min(1.0, p.zombie_loss * dt)), 0)
        pop = np.minimum(pop + born, K) - lost
        if p.proposed:
            grow = np.where(self.v_ex & ~loaded & (pop >= 2),
                            rng.poisson(p.grow_unloaded * dt * pop * room), 0)
            pop = np.minimum(pop + grow, K)
            self.v_events += int((born > 0).sum() + (grow > 0).sum())
            need = self.v_ex & (pop < 2) & (K >= 2) & self.known
            start = need & np.isinf(self.v_due)
            self.v_due = np.where(start, t + self.v_P.delay(t, p.settler_base, p.settler_cap), self.v_due)
            ready = need & (self.v_due <= t)
            if ready.any():
                donor_ok = self.v_ex & (pop > 0.6 * K) & (pop >= 4)
                for s, y, x in np.argwhere(ready):
                    y0, y1 = max(0, y - p.settler_reach), min(G, y + p.settler_reach + 1)
                    x0, x1 = max(0, x - p.settler_reach), min(G, x + p.settler_reach + 1)
                    cand = donor_ok[s, y0:y1, x0:x1] * pop[s, y0:y1, x0:x1]
                    if cand.max() > 0:      # a neighbour sends two; otherwise a family from afar
                        dy, dx = np.unravel_index(np.argmax(cand), cand.shape)
                        pop[s, y0 + dy, x0 + dx] -= 2
                    pop[s, y, x] += 2
                self.v_due = np.where(ready, INF, self.v_due)
                self.v_events += int(ready.sum())
        else:
            self.v_sim = np.maximum(self.v_sim, np.where(lv, pop, 0))
        self.v_pop = pop

    def kill_villagers(self, mask, t, count=None):
        n = self.v_pop if count is None else np.minimum(count, self.v_pop)
        n = np.where(mask, n, 0)
        self.v_pop = self.v_pop - n
        self.v_P.bump(n > 0, n / np.maximum(self.v_K, 1), t)
        self.v_kills += n

    def ribbits(self, t, dt):
        p, rng = self.p, self.rng
        self.r_clock -= dt
        tick = self.r_ex & (self.r_clock <= 0) & self.known
        self.r_clock = np.where(tick, self.r_clock + p.ribbit_clock, self.r_clock)
        prosper = tick & (rng.random(tick.shape) < 0.8) & (self.r_pop >= 1)
        build = prosper & (self.r_houses < self.r_K0 + p.ribbit_extra)
        if p.proposed:
            refill = prosper & (self.r_pop < self.r_houses)
            self.r_pop = self.r_pop + refill
            build &= ~refill
            self.r_events += int(refill.sum())
        self.r_houses = self.r_houses + build
        self.r_pop = self.r_pop + build
        if p.proposed:
            need = self.r_ex & (self.r_pop < 2) & self.known
            start = need & np.isinf(self.r_due)
            self.r_due = np.where(start, t + self.r_P.delay(t, p.settler_base, p.settler_cap), self.r_due)
            ready = need & (self.r_due <= t)
            self.r_pop = self.r_pop + 2 * ready
            self.r_due = np.where(ready, INF, self.r_due)
            self.r_events += int(ready.sum())

    def kill_ribbits(self, mask, t):
        n = np.where(mask, self.r_pop, 0)
        self.r_pop = self.r_pop - n
        self.r_P.bump(n > 0, n / np.maximum(self.r_houses, 1), t)

    # ------------------------------------------------------------------ conflict camps
    def camps(self, t, dt):
        p, rng = self.p, self.rng
        live = self.c_live
        rate = self.c_r * dt / (np.exp2(self.c_P.now(t)) if p.proposed else 1.0)
        self.c_pool = np.where(live, np.minimum(self.c_cap, self.c_pool + rate), 0.0)
        if p.proposed:
            # a camp the player keeps emptying breaks; its ground is settled again later
            brk = live & (self.c_pool < 0.1 * self.c_cap) & (self.c_P.now(t) >= p.camp_break_p)
            self.c_live = live & ~brk
            self.c_due = np.where(brk, t + self.c_P.delay(t, p.camp_refound_base, p.camp_refound_cap), self.c_due)
            back = self.c_ex & ~self.c_live & (self.c_due <= t) & self.known
            self.c_live |= back
            self.c_pool = np.where(back, self.c_cap / 4, self.c_pool)
            self.c_due = np.where(back, INF, self.c_due)
            self.c_events += int(brk.sum() + back.sum())
        # feuds between camps of different cultures within one region
        self.c_next -= dt
        due = self.c_live & (self.c_next <= 0) & self.known
        if not due.any():
            return
        self.c_next = np.where(due, p.feud_days, self.c_next)
        best = np.full(self.c_pool.shape, INF)
        bdy = np.zeros(self.c_pool.shape, int)
        bdx = np.zeros(self.c_pool.shape, int)
        for dy, dx in DIR9:
            if not (dy or dx):
                continue
            tp = shift(np.where(self.c_live, self.c_pool, INF), -dy, -dx)
            tc = shift(self.c_culture + 1, -dy, -dx) - 1
            ok = (tc >= 0) & (tc != self.c_culture) & np.isfinite(tp) & (tp < best)
            best = np.where(ok, tp, best)
            bdy = np.where(ok, dy, bdy)
            bdx = np.where(ok, dx, bdx)
        go = due & np.isfinite(best) & (self.c_pool >= 6)
        A = np.minimum(self.c_pool, 12) * 0.93
        D = np.where(np.isfinite(best), best, 0) * 0.93 + 1.0
        pwin = A * A / (A * A + D * D)
        if p.proposed:
            loss_w = pwin * (1 - np.sqrt(np.clip(1 - (D / np.maximum(A, 1e-9)) ** 2, 0, 1))) + (1 - pwin) * 0.5
            ev = pwin * 40.0 - 3.0 * loss_w * np.minimum(self.c_pool, 12)
            go &= ev > 0
        win = rng.random(go.shape) < pwin
        party = np.minimum(self.c_pool, 12)
        a_loss = np.where(win, party * (1 - np.sqrt(np.clip(1 - (D / np.maximum(A, 1e-9)) ** 2, 0, 1))), party * 0.5)
        d_loss = np.where(win, np.minimum(best, 0.5 * party), 0.3 * party)
        self.c_pool = np.where(go, np.maximum(0, self.c_pool - a_loss), self.c_pool)
        for s, y, x in np.argwhere(go):
            ty, tx = y + bdy[s, y, x], x + bdx[s, y, x]
            self.c_pool[s, ty, tx] = max(0.0, self.c_pool[s, ty, tx] - d_loss[s, y, x])
        self.c_raids += int(go.sum())
        self.c_lost += float(np.where(go, a_loss + d_loss, 0).sum())

    def kill_camp(self, mask, t):
        hit = mask & self.c_live & (self.c_pool > 0)
        frac = np.where(hit, self.c_pool / np.maximum(self.c_cap, 1), 0)
        self.c_pool = np.where(hit, 0.0, self.c_pool)
        self.c_P.bump(hit, frac, t)

    # ------------------------------------------------------------------ pirates
    def pirates(self, t, dt):
        p, rng = self.p, self.rng
        ex = self.h_ex
        if p.proposed:
            self.h_pool = np.where(ex, np.minimum(p.crew_cap, self.h_pool + p.recruit * dt * (1 - self.h_pool / p.crew_cap)), 0)
        # launch ships into empty slots whose rebuild is due
        for k in range(p.ship_slots):
            ready = ex & ~self.h_ships[..., k] & (self.h_due[..., k] <= t) & (self.h_pool >= p.crew) & self.known
            self.h_ships[..., k] |= ready
            self.h_pool = self.h_pool - p.crew * ready
            self.h_due[..., k] = np.where(ready, INF, self.h_due[..., k])
            self.h_events += int(ready.sum())
            short = ex & ~self.h_ships[..., k] & (self.h_due[..., k] <= t) & (self.h_pool < p.crew)
            self.h_due[..., k] = np.where(short, t + p.ship_base, self.h_due[..., k])
        # hunts
        avail = self.h_merch_due <= t
        for k in range(p.ship_slots):
            ship = self.h_ships[..., k]
            go = ship & (rng.random(ship.shape) < min(1.0, p.hunt_p * dt))
            if not go.any():
                continue
            if p.proposed:     # best response: the open route with the best expected value, if any
                ev = np.where(avail, self.h_routes * 3 * 1.0 - (1 - self.h_routes) * 6 * 1.0, -INF)
                r = np.argmax(ev, axis=-1)
                go &= np.take_along_axis(ev, r[..., None], -1)[..., 0] > 0
            else:
                pick = np.where(avail, rng.random(avail.shape), -1)
                r = np.argmax(pick, axis=-1)
                go &= np.take_along_axis(avail, r[..., None], -1)[..., 0]
            pw = np.take_along_axis(self.h_routes, r[..., None], -1)[..., 0]
            win = go & (rng.random(go.shape) < pw)
            lose = go & ~win
            gain = int(np.ceil(p.crew * 0.5))
            cap = p.crew_cap if p.proposed else 10 ** 9
            self.h_pool = np.where(win, np.minimum(cap, self.h_pool + gain), self.h_pool)
            self.h_captures += int(win.sum())
            md = np.take_along_axis(self.h_merch_due, r[..., None], -1)[..., 0]
            np.put_along_axis(self.h_merch_due, r[..., None], np.where(win, t + 2.0, md)[..., None], -1)
            self.h_ships[..., k] &= ~lose
            delay = self.h_P.delay(t, p.ship_base, p.ship_cap) if p.proposed else p.ship_base
            self.h_due[..., k] = np.where(lose, t + delay, self.h_due[..., k])

    def sink_ships(self, mask, t):
        p = self.p
        for k in range(p.ship_slots):
            hit = mask & self.h_ships[..., k]
            self.h_ships[..., k] &= ~hit
            self.h_P.bump(hit, 1.0 / p.ship_slots, t)
            delay = self.h_P.delay(t, p.ship_base, p.ship_cap) if p.proposed else p.ship_base
            self.h_due[..., k] = np.where(hit, t + delay, self.h_due[..., k])

    # ------------------------------------------------------------------ census
    def census(self):
        N = self.Y + self.A
        fh = self.founded & (self.hab > 0)
        tot = N.sum((1, 2))
        alive = (fh & (N > 0)).sum((1, 2))
        top = N.max((1, 2)) / np.maximum(tot, 1)
        Ksum = np.where(fh, self.K, 0).sum((1, 2))
        kn = self.known
        return {
            "herd_animals": tot,
            "herd_k": Ksum,
            "herd_local": np.where(cheb(CY, CX)[None] <= 1, N, 0).sum((1, 2)),
            "herds_alive": alive,
            "herds_founded": fh.sum((1, 2)),
            "herd_top_share": top,
            "dragons": (self.lair_occ & kn[..., None]).sum((1, 2, 3)),
            "lairs": (self.lair_ex & kn[..., None]).sum((1, 2, 3)),
            "village_pop": np.where(kn, self.v_pop, 0).sum((1, 2)),
            "village_beds": np.where(kn & self.v_ex, self.v_K, 0).sum((1, 2)),
            "villages_alive": (kn & self.v_ex & (self.v_pop >= 2)).sum((1, 2)),
            "villages": (kn & self.v_ex).sum((1, 2)),
            "village_sim_pop": np.where(kn, self.v_sim, 0).sum((1, 2)),
            "ribbit_pop": np.where(kn, self.r_pop, 0).sum((1, 2)),
            "ribbit_houses": np.where(kn & self.r_ex, self.r_houses, 0).sum((1, 2)),
            "frostmaws": np.where(kn, self.dens.n, 0).sum((1, 2)),
            "dens": (kn & self.dens.exists).sum((1, 2)),
            "umvuthana": np.where(kn, self.groves.n, 0).sum((1, 2)),
            "grove_k": np.where(kn, self.groves.K, 0).sum((1, 2)),
            "umvuthi": np.where(kn, self.chiefs.n, 0).sum((1, 2)),
            "worms": np.where(kn, self.nests.n, 0).sum((1, 2)),
            "nest_k": np.where(kn, self.nests.K, 0).sum((1, 2)),
            "camp_pool": np.where(kn & self.c_live, self.c_pool, 0).sum((1, 2)),
            "camp_cap": np.where(kn & self.c_ex, self.c_cap, 0).sum((1, 2)),
            "camps_live": (kn & self.c_live).sum((1, 2)),
            "camps": (kn & self.c_ex).sum((1, 2)),
            "pirate_ships": (self.h_ships & kn[..., None]).sum((1, 2, 3)),
            "pirate_crew": np.where(kn, self.h_pool, 0).sum((1, 2)),
            "havens": (kn & self.h_ex).sum((1, 2)),
        }

    def events(self):
        return (self.herd_events + self.lair_events + self.v_events + self.r_events + self.c_events
                + self.h_events + self.dens.events + self.groves.events + self.chiefs.events
                + self.nests.events)


# --------------------------------------------------------------------------- players

class Player:
    """none: unattended; homebody: one base; roamer: visits everywhere; farmer: camps one site."""

    def __init__(self, kind, S, seed, farm=None, kills=True):
        self.kind, self.S, self.farm, self.kills = kind, S, farm, kills
        self.rng = np.random.default_rng(seed + 7919)
        self.pos = np.tile([CY, CX], (S, 1))
        self.home = (CY, CX + 2) if kind == "farmer" else (CY, CX)

    def known(self, t):
        if self.kind == "roamer":
            r = min(6, 2 + int(t) // 48)
        elif self.kind == "none":
            r = 3
        else:
            r = 2
        if self.kind == "farmer":
            m = (cheb(self.home[0], self.home[1]) <= 1) | (cheb(CY, CX) <= r)
            return np.broadcast_to(m, (self.S, G, G)).copy()
        return np.broadcast_to(cheb(CY, CX) <= r, (self.S, G, G)).copy()

    def loaded(self, t):
        if self.kind == "none":
            return np.zeros((self.S, G, G), bool)
        if self.kind in ("homebody", "farmer"):
            return np.broadcast_to(cheb(*self.home) <= 1, (self.S, G, G)).copy()
        out = np.zeros((self.S, G, G), bool)
        for s in range(self.S):
            out[s] = cheb(self.pos[s, 0], self.pos[s, 1]) <= 1
        return out

    def here(self):
        out = np.zeros((self.S, G, G), bool)
        out[np.arange(self.S), self.pos[:, 0], self.pos[:, 1]] = True
        return out

    def move(self, t, dt, known):
        if self.kind != "roamer" or int(t) % 2:
            return
        step = self.rng.integers(-1, 2, size=(self.S, 2))
        nxt = np.clip(self.pos + step, 0, G - 1)
        ok = known[np.arange(self.S), nxt[:, 0], nxt[:, 1]]
        self.pos = np.where(ok[:, None], nxt, self.pos)

    def act(self, w: World, t, dt):
        if not self.kills:
            return
        r = self.rng
        S = self.S
        if self.kind == "roamer":
            h = self.here()
            u = lambda q: h & (r.random((S, G, G)) < q)
            w.kill_dragons(u(0.5)[..., None] & np.ones(2, bool), t)
            w.dens.lose(u(0.5), np.ones((S, G, G), np.int64), t, True)
            w.groves.lose(u(0.3), w.groves.n, t, True)
            w.chiefs.lose(u(0.3), np.ones((S, G, G), np.int64), t, True)
            w.nests.lose(u(0.5), w.nests.n, t, True)
            w.kill_camp(u(0.2), t)
            w.sink_ships(u(0.3), t)
        elif self.kind == "homebody":
            guard = cheb(*self.home) <= 2
            hit = (r.random((S, G, G, 2)) < 0.01 * dt) & guard[None, ..., None]
            w.kill_dragons(hit, t)
        elif self.kind == "farmer" and int(t) % 2 == 0:
            m = np.zeros((S, G, G), bool)
            m[:, self.home[0], self.home[1]] = True
            if self.farm == "lair":
                w.kill_dragons(m[..., None] & np.ones(2, bool), t)
            elif self.farm == "den":
                w.dens.lose(m, np.ones((S, G, G), np.int64), t, True)
            elif self.farm == "grove":
                w.groves.lose(m, w.groves.n, t, True)
                w.chiefs.lose(m, np.ones((S, G, G), np.int64), t, True)
            elif self.farm == "village":
                w.kill_villagers(m & (w.v_pop >= 2), t)


def force_farm_site(w: World, farm, home):
    y, x = home
    if farm == "lair":
        w.lair_ex[:, y, x, 0] = True
        w.lair_ex[:, y, x, 1] = False
        w.lair_occ[:, y, x, 0] = True
        w.lair_occ[:, y, x, 1] = False
        w.lair_age[:, y, x, 0] = 30
    elif farm == "den":
        w.dens.exists[:, y, x] = True
        w.dens.K[:, y, x] = 1
        w.dens.n[:, y, x] = 1
    elif farm == "grove":
        w.groves.exists[:, y, x] = True
        w.groves.K[:, y, x] = 7
        w.groves.n[:, y, x] = 7
        w.chiefs.exists[:, y, x] = True
        w.chiefs.K[:, y, x] = 1
        w.chiefs.n[:, y, x] = 1
    elif farm == "village":
        w.v_ex[:, y, x] = True
        w.v_K[:, y, x] = 12
        w.v_pop[:, y, x] = 12
        w.r_ex[:, y, x] = False


# --------------------------------------------------------------------------- shocks

def shock(w: World, kind, t):
    S = w.S
    kn = w.known
    one = np.ones((S, G, G), np.int64)
    if kind == "herds_local":           # a player clears every animal around spawn
        m = np.broadcast_to(cheb(CY, CX) <= 1, kn.shape)
        w.Y = np.where(m, 0, w.Y)
        w.A = np.where(m, 0, w.A)
    elif kind == "herds":
        gone = kn & ((w.Y + w.A) > 0)
        w.Y = np.where(kn, 0, w.Y)
        w.A = np.where(kn, 0, w.A)
        if not w.p.proposed:
            w.tomb |= gone
    elif kind == "dragons":
        w.kill_dragons(kn[..., None] & w.lair_occ, t)
    elif kind == "villages":
        w.kill_villagers(kn & w.v_ex, t)
    elif kind == "ribbits":
        w.kill_ribbits(kn & w.r_ex, t)
    elif kind == "dens":
        w.dens.lose(kn, one, t, True)
    elif kind == "groves":
        w.groves.lose(kn, w.groves.n, t, True)
        w.chiefs.lose(kn, one, t, True)
    elif kind == "nests":
        w.nests.lose(kn, w.nests.n, t, True)
    elif kind == "camps":
        w.kill_camp(kn, t)
    elif kind == "pirates":
        w.sink_ships(kn, t)
        w.h_pool = np.where(kn, 0.0, w.h_pool)
        w.h_P.bump(kn & w.h_ex, 1.0, t)


# --------------------------------------------------------------------------- run

def run(p: Params, S=24, days=60 * YEAR, seed=1, player="homebody", farm=None,
        shock_kind=None, shock_day=None, peaceful_after_shock=True, record_every=24):
    pl = Player(player, S, seed, farm=farm)
    w = World(p, S, seed, pl)
    if farm:
        force_farm_site(w, farm, pl.home)
    dt = p.dt
    hist = []
    loaded_year = np.zeros((S, G, G), bool)
    ev_last = 0
    t = 0
    while t < days:
        known = pl.known(t)
        w.discover(known)
        pl.move(t, dt, known)
        loaded = pl.loaded(t)
        loaded_year |= loaded
        sat = w.graze(dt)
        w.herd_deaths(sat, dt)
        iota = w.hunt(dt)
        doy = t % YEAR
        if doy <= p.birth_doy < doy + dt:
            w.births()
        if p.proposed and doy <= p.birth_doy + 6 < doy + dt:
            w.disperse(t, loaded_year)
            loaded_year[:] = False
        w.mark_extinct()
        w.lairs(t, dt, iota)
        w.dens.step(t, known)
        w.groves.step(t, known)
        heir = w.chiefs.step(t, known, gate=w.groves.n >= 1)
        if heir.any():               # an Umvuthana takes up the mask
            w.groves.lose(heir, np.ones((S, G, G), np.int64), t, False)
        w.nests.step(t, known)
        w.villages(t, dt, loaded)
        w.ribbits(t, dt)
        w.camps(t, dt)
        w.pirates(t, dt)
        if shock_kind and shock_day is not None and t <= shock_day < t + dt:
            if shock_kind != "control":      # a control run: same seed, the player turns peaceful, no kill
                shock(w, shock_kind, t)
            if peaceful_after_shock:
                pl.kills = False
        pl.act(w, t, dt)
        if t % record_every < dt:
            c = w.census()
            ev = w.events()
            c["events"] = np.full(S, (ev - ev_last) / S)
            ev_last = ev
            c["t"] = t
            hist.append(c)
        t += dt
    return w, hist
