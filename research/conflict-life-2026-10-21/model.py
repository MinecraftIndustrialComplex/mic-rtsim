#!/usr/bin/env python3
"""Conflict-life model for CONFLICT-LIFE.md: when and why factions fight.

One world: a valley of factions (two goblin camps, a bandit camp, a pillager outpost, an Umvuthana
grove, two Frostmaw dens), two villages, a caravan road, and per scenario a railway and scripted
players. The same physics (pools, recruits under EQUILIBRIUM's pressure, stores and seasons, battles by
Lanchester's square law, dragons, news) runs under three decision rules:

  ladder  today's grievance ladder (rtsim-conflict at aa898aa: plan_feud and escalate), EQ-5 (c)
  gate    EQUILIBRIUM.md section 2.5's gate, EQ-5 (a)
  life    CONFLICT-LIFE.md section 2: a council of voices with drives, beliefs and memories that
          wakes on occasions and chooses by a logit (quantal) response

Time is in game days (96 a year). Every random draw comes from the world's numpy Generator, seeded by
the caller, so a world is a pure function of (scenario, mode, seed).
"""
from __future__ import annotations

import math
from dataclasses import dataclass, field, replace

import numpy as np

YEAR = 96
SEASON_MULT = (1.3, 1.5, 1.2, 0.3)          # spring, summer, autumn, winter: food income (mean 1.075)
LARDER_DAYS = 45.0                          # stores a camp can hold, days of eating at full strength
HUNGRY_DAYS = 25.0                          # below this many days of stores a camp is hungry
RUNG_NAMES = ("calm", "notice", "envoy", "scouts", "raid", "invasion", "siege")


def season(t):
    return int(t // 24) % 4


def battle(rng, A, d):
    """Lanchester's square law, as threat.rs and game.py use it. Returns (won, attacker loss share,
    defender loss share)."""
    if A <= 0:
        return False, 1.0, 0.0
    d = max(d, 1e-6)
    pw = A * A / (A * A + d * d)
    win = rng.random() < pw
    if win:
        la = 1 - math.sqrt(max(0.0, 1 - (d / A) ** 2)) if A > d else 0.5
        ld = min(1.0, 0.5 * max(1.0, A / d))
    else:
        la = min(1.0, 0.5 * max(1.0, d / A))
        ld = 1 - math.sqrt(max(0.0, 1 - (A / d) ** 2)) if d > A else 0.5
    return win, la, ld


def tenure(rng, mean):
    """Days a chief leads before age, illness, a quarrel or his rivals end it: gamma, shape 3 (a chief
    seldom falls in his first weeks), with the culture's mean (today's chief_lifespan_days)."""
    return float(rng.gamma(3.0, mean / 3.0))


def lw_ll(A, d):
    lw = 1 - math.sqrt(max(0.0, 1 - (d / A) ** 2)) if A > d else 0.5
    ll = min(1.0, 0.5 * max(1.0, d / A))
    return lw, ll


# ---------------------------------------------------------------------------------------------
# Cultures. Physical numbers come from data/cultures/*.json and MOWZIE.md; the mind's numbers are
# CONFLICT-LIFE.md section 2.6 (all TUNABLE).
# ---------------------------------------------------------------------------------------------
@dataclass(frozen=True)
class Culture:
    name: str
    pool_cap: float
    recruits: float
    # parties: rung -> (size in pool members, strength, carry)
    party: dict
    thr: tuple
    dwell: tuple
    g_half: float
    hyst: float
    kill_w: float
    tribute_per_g: float
    tribute_relief: float
    respite: float
    cowed: float
    chief_days: float               # mean tenure of a chief (today's chief_lifespan_days)
    home_str: float                 # defence per member at home
    eat: float                      # stores eaten per member per day
    plunder_mean: float             # today's Poisson plunder (ladder and gate)
    # the mind (life)
    council: bool = True
    bold: float = 0.5
    aggr: float = 0.5
    soc: float = 0.5
    spread: float = 0.15
    tau: float = 5.0                # logit temperature, loot items
    greed: float = 1.0              # how much hunger raises the value of loot
    warrior_value: float = 3.0
    rev_max: float = 10.0
    pride_w: float = 12.0
    pride_half: float = 24.0
    refusal_pride: float = 0.5
    insult_pride: float = 0.8
    fear_w: float = 1.5
    fear_half: float = 32.0
    glory_w: float = 6.0
    restless_days: float = 120.0
    wake_days: float = 16.0
    effort: float = 3.0
    demand_bias: float = 0.0
    scout_w: float = 2.0
    warn_w: float = 4.0
    oath: float = 20.0              # cost of breaking a truce
    hunts: bool = False             # raptor hunts on livestock when hungry (Umvuthana)
    invasion_pride: float = 0.0     # invasion only with pride at least this, led by the chief
    chief_str: float = 0.0
    ambush_w: float = 1.0
    kin_cost: float = 25.0          # raiding a camp of one's own culture
    preempt_w: float = 0.0


def P(size, strength, carry):
    return (size, strength, carry)


GOBLIN = Culture(
    name="goblin", pool_cap=60, recruits=2.0,
    party={3: P(2, 1.6, 8), 4: P(12, 11.2, 80), 5: P(26, 30.4, 192), 6: P(34, 44.0, 264), 8: P(8, 7.6, 56)},
    thr=(0, 10, 30, 50, 70, 140, 260), dwell=(0, .5, .5, 1, 4, 6, 8), g_half=6, hyst=0.6, kill_w=8,
    tribute_per_g=0.5, tribute_relief=0.7, respite=6, cowed=6, chief_days=120, home_str=0.93 * 1.5,
    eat=0.1, plunder_mean=12,
    bold=0.55, aggr=0.6, soc=0.35, spread=0.2, tau=4.0, greed=1.6, pride_half=12, fear_w=2.5,
    fear_half=40, glory_w=5, restless_days=96, wake_days=12, effort=2, demand_bias=0, ambush_w=1.4)
BANDIT = Culture(
    name="bandit", pool_cap=40, recruits=1.5,
    party={3: P(1, 1.0, 8), 4: P(10, 11.2, 104), 5: P(20, 22.4, 208), 6: P(26, 29.2, 270), 8: P(8, 9.2, 76)},
    thr=(0, 8, 20, 35, 50, 110, 220), dwell=(0, .5, .5, .5, 3, 5, 8), g_half=10, hyst=0.6, kill_w=10,
    tribute_per_g=1.0, tribute_relief=0.8, respite=5, cowed=4, chief_days=90, home_str=1.1 * 1.5,
    eat=0.1, plunder_mean=6,
    bold=0.45, aggr=0.45, soc=0.6, spread=0.15, tau=2.0, greed=1.2, pride_half=24, fear_w=1.5,
    fear_half=30, glory_w=3, restless_days=150, wake_days=14, effort=4, demand_bias=6, ambush_w=1.6)
RAIDER = Culture(
    name="raider", pool_cap=50, recruits=2.0,
    party={3: P(2, 2.4, 12), 4: P(12, 16.2, 96), 5: P(23, 36.0, 180), 6: P(31, 56.4, 244), 8: P(8, 10.8, 64)},
    thr=(0, 6, 18, 30, 45, 100, 200), dwell=(0, .5, .5, .5, 3, 4, 6), g_half=15, hyst=0.7, kill_w=12,
    tribute_per_g=1.5, tribute_relief=0.6, respite=4, cowed=3, chief_days=150, home_str=1.35 * 1.5,
    eat=0.1, plunder_mean=8,
    bold=0.6, aggr=0.6, soc=0.35, spread=0.15, tau=2.5, greed=0.9, pride_w=20, pride_half=96,
    refusal_pride=0.8, insult_pride=1.0, fear_w=1.0, fear_half=24, glory_w=7, restless_days=120,
    wake_days=20, effort=4, demand_bias=2, oath=10, preempt_w=10)
UMVUTHANA = Culture(
    name="umvuthana", pool_cap=7, recruits=7 / 16.5,
    party={3: P(0, 2.6, 0), 4: P(4, 3.8, 24), 5: P(8, 7.6, 48), 6: P(8, 7.6, 48), 8: P(0, 2.6, 8)},
    thr=(0, 10, 30, 50, 70, 140, 260), dwell=(0, .5, .5, 1, 4, 6, 8), g_half=6, hyst=0.6, kill_w=8,
    tribute_per_g=0.5, tribute_relief=0.7, respite=6, cowed=6, chief_days=1e9, home_str=0.4 * 1.5 + 0.8,
    eat=0.3, plunder_mean=12,
    bold=0.5, aggr=0.45, soc=0.65, spread=0.15, tau=2.5, greed=1.2, pride_w=18, pride_half=48,
    refusal_pride=0.9, insult_pride=1.2, fear_w=1.5, fear_half=32, glory_w=3, restless_days=200,
    wake_days=16, effort=6, demand_bias=10, hunts=True, invasion_pride=0.8, chief_str=5.0)
FROSTMAW = Culture(
    name="frostmaw", pool_cap=1, recruits=0.0,
    party={3: P(0, 0, 0), 4: P(1, 41.7, 64), 5: P(1, 41.7, 64), 6: P(1, 41.7, 64), 8: P(1, 41.7, 64)},
    thr=(0, 10, 30, 50, 70, 140, 260), dwell=(0, .5, .5, 1, 4, 6, 8), g_half=6, hyst=0.6, kill_w=8,
    tribute_per_g=0.0, tribute_relief=0.0, respite=6, cowed=6, chief_days=1e9, home_str=41.7 * 1.2,
    eat=0.5, plunder_mean=1e9,
    council=False, bold=0.5, aggr=0.5, soc=0.1, spread=0.25, tau=3.0, greed=1.0, pride_w=25,
    pride_half=96, insult_pride=1.5, fear_w=1.0, fear_half=16, glory_w=0, restless_days=1e9,
    wake_days=30, effort=14, demand_bias=-99, scout_w=0, warn_w=8, ambush_w=1.0)

CULTURES = {c.name: c for c in (GOBLIN, BANDIT, RAIDER, UMVUTHANA, FROSTMAW)}
HATES = {"goblin": {"human", "bandit"}, "bandit": {"human", "ribbit"},
         "raider": {"human", "ribbit", "goblin", "bandit"}, "umvuthana": {"human"}, "frostmaw": set()}

NAMES = {
    "goblin": ["Grukk", "Snag", "Mott", "Brakka", "Ozzit", "Nub", "Skarl", "Gitz", "Vorr", "Lugg"],
    "bandit": ["Harl", "Black Tam", "Wenna", "Rook", "Old Cass", "Jory", "Fenn", "Mags"],
    "raider": ["Captain Vask", "Orren", "Captain Hesk", "Dorn", "Isk", "Captain Mol", "Talv"],
    "umvuthana": ["the Umvuthi"],
    "frostmaw": ["the Frostmaw"],
}


# ---------------------------------------------------------------------------------------------
@dataclass
class Voice:
    role: str               # chief, war (the hothead, usually the heir), elder
    name: str
    bold: float
    aggr: float
    soc: float
    standing: float
    since: float


@dataclass
class Rel:
    tgt: str
    sigma: float = 0.0               # grievance per day from standing causes (industry, hate, rail)
    g: float = 0.0
    W: float = 0.0                   # wounded pride toward this target, 0..2
    mu: float = math.log(15.0)       # belief of the target's defence, log-normal
    s2: float = 1.0
    ta: float = 1.0                  # trust that the target pays, Beta(ta, tb)
    tb: float = 1.0
    truce_until: float = -1.0
    pending: tuple | None = None     # (deadline, T, X)
    refused_at: float = -1e9
    paid_until: float = -1.0
    memories: list = field(default_factory=list)   # [salience, day, text, hostile]
    away_until: float = -1.0         # word came that the player is away from his base until about then
    truce_by: str = ""               # the chief who swore the truce running now
    last_warn: float = -1e9
    # ladder and gate state
    step: int = 0
    next_ok: float = 0.0
    cowed_until: float = 0.0
    last_probe: float = -1e9
    gate_demand: tuple | None = None


class Faction:
    def __init__(self, fid, name, culture: Culture, rng, t=0.0):
        self.id = fid
        self.name = name
        self.c = culture
        self.pool = culture.pool_cap
        self.away = 0.0
        self.P = 0.0
        self.P_t = 0.0
        self.stores = 40.0 * culture.eat * culture.pool_cap
        self.fear = 0.0
        self.last_fight = t
        self.busy_until = -1.0
        self.next_wake = t + rng.exponential(culture.wake_days)
        self.rel: dict[str, Rel] = {}
        self.ground = 1.0
        self.ground_until = -1.0
        self.year_mult = 1.0
        self.heir_until = -1.0
        self.next_plunder = t + rng.exponential(culture.plunder_mean)
        self.voices: list[Voice] = []
        self.chief_dies = t + tenure(rng, culture.chief_days)
        self.names_used = 0
        self.recent_names: list[str] = []
        if culture.council:
            for role, st in (("chief", 1.0), ("war", 0.7), ("elder", 0.6)):
                self.voices.append(self.new_voice(role, st, rng, t))
        else:
            self.voices.append(self.new_voice("chief", 1.0, rng, t))
        self.dead_until = -1.0          # Frostmaw: no body until renewal
        self.dead = False

    def new_voice(self, role, standing, rng, t):
        c = self.c
        sp = c.spread
        shift = {"chief": (0, 0, 0), "war": (0.12, 0.15, -0.1), "elder": (-0.15, -0.15, 0.1)}[role]
        b = float(np.clip(rng.normal(c.bold + shift[0], sp), 0.02, 0.98))
        a = float(np.clip(rng.normal(c.aggr + shift[1], sp), 0.02, 0.98))
        s = float(np.clip(rng.normal(c.soc + shift[2], sp), 0.02, 0.98))
        # no name of a living voice or of the last three chiefs, so a dead chief never seems to return
        taken = {x.name for x in self.voices} | set(self.recent_names[-3:])
        names = [n for n in NAMES[c.name] if n not in taken] or NAMES[c.name]
        nm = names[int(rng.integers(len(names)))]
        self.names_used += 1
        return Voice(role, nm, b, a, s, standing, t)

    def chief(self):
        return self.voices[0]

    def home(self):
        return max(0.0, self.pool - self.away)

    def defence(self):
        if self.dead:
            return 0.0
        if self.c.name == "frostmaw":
            return self.c.home_str
        return self.home() * self.c.home_str

    def hunger(self):
        need = HUNGRY_DAYS * self.c.eat * max(self.pool, 1.0)
        return float(np.clip(1.0 - self.stores / need, 0.0, 1.0))

    def store_room(self):
        return max(0.0, LARDER_DAYS * self.c.eat * max(self.c.pool_cap, 1.0) - self.stores)

    def restless(self, t):
        return min(1.0, (t - self.last_fight) / self.c.restless_days) * (self.pool / self.c.pool_cap)


# ---------------------------------------------------------------------------------------------
@dataclass
class PlayerScript:
    """A scripted player base. kind: fortress, weak, stubborn, adaptive, hunter, farmer, railway."""
    pid: str
    kind: str
    d: float
    pays: bool
    sigma: float                    # industry grievance a day to each faction in reach that hates humans
    reach: tuple                    # factions in reach
    kills_envoys: bool = False
    stock: float = 400.0
    hunts: str | None = None        # faction id he hunts
    hunt_every: float = 20.0
    farm_den: str | None = None
    rob_den: str | None = None
    rob_every: float = 40.0
    trips: bool = True              # leaves the base now and then
    strength: float = 60.0          # in person, for hunts and the den
    quiet: float = 0.0
    away_until: float = -1.0
    next_trip: float = 30.0
    next_hunt: float = 10.0
    next_rob: float = 20.0
    d0: float = 0.0
    escort_until: float = -1.0

    def defence(self, t):
        return self.d * (0.5 if t < self.away_until else 1.0)


@dataclass
class Scenario:
    name: str
    players: list
    rail: bool = False
    rail_legs: float = 4.0
    duel: bool = False             # one goblin camp and one player only (game.py's setting)
    blame_spill: float = 0.3       # share of an unattributed kill blamed on another player in reach
    days: int = 50 * YEAR
    dragons: float = 0.25          # dragon strikes per camp-year
    life: dict = field(default_factory=dict)   # overrides for the life mind


def base_world():
    """Fixed valley geography: who can reach whom."""
    factions = [("G1", "Ashpit", "goblin"), ("G2", "Mudhollow", "goblin"), ("B1", "Crowsnest", "bandit"),
                ("R1", "the Grey Outpost", "raider"), ("U1", "the Sun Grove", "umvuthana"),
                ("F1", "the North Den", "frostmaw"), ("F2", "the Pass Den", "frostmaw")]
    # nearest first: today's plunder takes the nearest hated node in reach
    reach = {"G1": ["V1", "B1", "G2", "R1"], "G2": ["G1", "V2", "U1", "R1"], "B1": ["V1", "G1", "R1", "V2"],
             "R1": ["G2", "G1", "V2", "B1", "V1"], "U1": ["V2", "G2"], "F1": [], "F2": []}
    villages = {"V1": ("Oakford", "human"), "V2": ("Lowmarsh", "ribbit")}
    caravan_near = ["B1", "G1", "R1"]
    return factions, reach, villages, caravan_near


# ---------------------------------------------------------------------------------------------
class World:
    def __init__(self, sc: Scenario, mode: str, seed: int):
        self.sc = sc
        self.mode = mode
        self.rng = np.random.default_rng(seed)
        self.seed = seed
        self.t = 0
        self.events = []            # every act: dict
        self.factions: dict[str, Faction] = {}
        self.villages = {}
        self.players = {p.pid: replace(p) for p in sc.players}
        for p in self.players.values():
            p.d0 = p.d
        self.rail = sc.rail
        self.rail_d = 1.0
        self.news = []              # (deliver_t, fid, kind, about, obs)
        self.life = dict(LIFE_DEFAULTS)
        self.life.update(sc.life)
        self.extinct_days = {}      # faction -> longest run of days held at one member or none
        self.low_run = {}
        rng = self.rng
        if sc.duel:
            self.factions["G1"] = Faction("G1", "Ashpit", GOBLIN, rng)
            reach = {"G1": []}
            self.caravan_near = []
        else:
            fs, reach, vs, self.caravan_near = base_world()
            for fid, nm, cn in fs:
                self.factions[fid] = Faction(fid, nm, CULTURES[cn], rng)
            for vid, (nm, kind) in vs.items():
                self.villages[vid] = {"name": nm, "kind": kind, "stock": 150.0, "d": 4.0}
        for fid, f in self.factions.items():
            for x in reach.get(fid, []):
                f.rel[x] = Rel(x)
                f.rel[x].sigma = self.hate_sigma(f, x)
        for p in self.players.values():
            for fid in p.reach:
                f = self.factions[fid]
                r = f.rel.setdefault(p.pid, Rel(p.pid))
                r.sigma = p.sigma if "human" in HATES[f.c.name] else 0.0
        # what neighbours know of each other at the start: camps and villages roughly, a new base not at all
        for f in self.factions.values():
            for x, r in f.rel.items():
                if x in self.factions or x in self.villages:
                    r.mu = math.log(max(self.target_defence(x), 0.5)) + self.rng.normal(0, 0.3)
                    r.s2 = 0.09
        if mode == "life":
            for fid in self.caravan_near:
                r = self.factions[fid].rel.setdefault("CARAVAN", Rel("CARAVAN"))
                r.mu, r.s2 = math.log(3.0), 0.1
        if self.rail:
            for fid in ("G1", "F2"):
                f = self.factions[fid]
                r = f.rel.setdefault("RAIL", Rel("RAIL"))
                r.mu = math.log(3.0)
                r.s2 = 0.5
                if f.c.name == "frostmaw":
                    # MOWZIE's rail meter: grievance settles at 8.66 x (legs a day - 2)
                    gstar = 8.66 * max(0.0, sc.rail_legs - 2)
                    r.sigma = gstar * (1 - 2 ** (-1 / f.c.g_half))
                else:
                    r.sigma = 3.0          # the railway is industry in goblin land
        self.dragon_until = {}
        self.debug = False
        self.sittings = 0           # council sittings (decide calls past the busy check)
        self.matters = 0            # relations weighed over all sittings
        self.hostile_pairs = sorted({tuple(sorted((a, b))) for a, f in self.factions.items() for b in f.rel
                                     if b in self.factions and (self.factions[b].c.name in HATES[f.c.name]
                                                               or f.c.name in HATES[self.factions[b].c.name])})

    # -- standing grievance from culture hatred (faction or village target)
    def hate_sigma(self, f, x):
        if x in self.villages:
            kind = self.villages[x]["kind"]
            return self.life["hate_sigma_village"] if kind in HATES[f.c.name] else 0.0
        if x in self.factions:
            return self.life["hate_sigma_faction"] if self.factions[x].c.name in HATES[f.c.name] else 0.0
        return 0.0

    # ------------------------------------------------------------------------------- logging
    def log(self, f, act, tgt, reason="", **kw):
        e = {"t": self.t, "f": f.id, "culture": f.c.name, "act": act, "tgt": tgt, "reason": reason,
             "speaker": kw.pop("speaker", ""), **kw}
        self.events.append(e)
        return e

    def remember(self, f, tgt, salience, text, hostile=True):
        r = f.rel.get(tgt)
        if r is None:
            return
        r.memories.append([salience, self.t, text, hostile])
        r.memories.sort(key=lambda m: -m[0])
        del r.memories[4:]

    def when(self, t):
        y, d = divmod(int(t), YEAR)
        return f"in the {('spring', 'summer', 'autumn', 'winter')[d // 24]} of year {y + 1}"

    def tname(self, x):
        if x in self.factions:
            return self.factions[x].name
        if x in self.villages:
            return self.villages[x]["name"]
        if x == "RAIL":
            return "the railway"
        if x == "CARAVAN":
            return "a caravan"
        return "player " + x.split(":")[1]

    # ------------------------------------------------------------------------------- targets
    def target_defence(self, x):
        t = self.t
        if x in self.factions:
            return self.factions[x].defence()
        if x in self.villages:
            return self.villages[x]["d"]
        if x == "RAIL":
            return self.rail_d
        if x == "CARAVAN":
            return 3.0
        return self.players[x].defence(t)

    def target_stock(self, x):
        if x in self.factions:
            return 0.5 * self.factions[x].stores
        if x in self.villages:
            return self.villages[x]["stock"]
        if x == "RAIL":
            return 64.0
        if x == "CARAVAN":
            return 60.0
        return self.players[x].stock

    def can_pay(self, x):
        return x in self.players

    # ------------------------------------------------------------------------------- physics
    def pressure(self, f):
        return f.P * 2 ** (-(self.t - f.P_t) / 96.0)

    def player_loss(self, f, n):
        """EQUILIBRIUM 2.4.5: warriors killed by a player add killed/K to the camp's pressure."""
        f.P = self.pressure(f) + n / max(f.c.pool_cap, 1)
        f.P_t = self.t

    def day_faction(self, f):
        t = self.t
        c = f.c
        if c.name == "frostmaw":
            if f.dead and t >= f.dead_until:
                f.dead = False
                f.pool = 1
                old = f.voices[0]
                f.voices = [f.new_voice("chief", 1.0, self.rng, t)]
                for r in f.rel.values():          # the heir takes the grudges at half (heir_grudge_factor)
                    r.g *= 0.5
                    r.W *= 0.5
                    for m in r.memories:
                        m[0] *= 0.5
                self.log(f, "arrive", "", reason=f"a new Frostmaw takes the den after {old.name} died")
        else:
            P = self.pressure(f)
            f.pool = min(c.pool_cap, f.pool + c.recruits / 2 ** P * (1.0 if f.stores > 0 else 0.25))
        sm = SEASON_MULT[season(t)]
        if t % YEAR == 0:
            f.year_mult = float(np.clip(self.rng.normal(1.0, 0.2), 0.5, 1.5))
        if t >= f.ground_until:
            f.ground = 1.0
        inc = c.eat * c.pool_cap * sm * f.year_mult * f.ground
        f.stores = min(f.stores + inc - c.eat * f.pool, LARDER_DAYS * c.eat * max(c.pool_cap, 1.0))
        if f.stores < 0:
            if c.name != "frostmaw":
                f.pool = max(0.0, f.pool - 0.02 * f.pool)   # hungry members drift away
            f.stores = 0.0
        f.fear *= 2 ** (-1 / c.fear_half)
        for r in f.rel.values():
            r.g = r.g * 2 ** (-1 / c.g_half) + r.sigma
            r.W *= 2 ** (-1 / c.pride_half)
            r.s2 = min(1.0, r.s2 + self.life["belief_drift"])
            r.ta = 1 + (r.ta - 1) * 2 ** (-1 / 192)
            r.tb = 1 + (r.tb - 1) * 2 ** (-1 / 192)
            for m in r.memories:
                m[0] *= 2 ** (-1 / (4 * c.pride_half))
        if f.pool <= 1.0 and c.name != "frostmaw":
            n = self.low_run.get(f.id, 0) + 1
            self.low_run[f.id] = n
            self.extinct_days[f.id] = max(self.extinct_days.get(f.id, 0), n)
        else:
            self.low_run[f.id] = 0
        # succession by age
        if c.council and t >= f.chief_dies:
            hows = ["died of a fever", "was killed in a quarrel", "died in a hunting accident", "was deposed"]
            if t - f.chief().since > c.chief_days:         # only one who has sat on the council long dies old
                hows.append("died of age")
            self.succeed(f, None, hows[int(self.rng.integers(len(hows)))])

    def succeed(self, f, killer, how):
        t = self.t
        old = f.chief()
        f.recent_names.append(old.name)
        if f.c.council:
            war = f.voices[1]
            war.role = "chief"
            war.standing = 1.0
            f.voices[0] = war
            f.voices[1] = f.new_voice("war", 0.7, self.rng, t)
            f.heir_until = t + YEAR
            f.chief_dies = t + tenure(self.rng, f.c.chief_days)
            for r in f.rel.values():
                r.g *= 0.5 + 0.5 * war.aggr      # the heir keeps the grudges he cares about
        if killer is not None and killer in f.rel:
            r = f.rel[killer]
            r.W = min(2.0, r.W + 1.0)
            r.g += 40
            self.remember(f, killer, 3.0, f"{self.tname(killer)} killed {old.name}, our chief, {self.when(t)}")
        self.log(f, "succession", killer or "", reason=f"{old.name} {how}; {f.chief().name} leads now")

    def world_day(self):
        t = self.t
        rng = self.rng
        for v in self.villages.values():
            v["stock"] = min(150.0, v["stock"] + 1.0)
        for p in self.players.values():
            p.stock = min(400.0, p.stock + 5.0)
            self.player_day(p)
        # dragons: a strike thins a camp (not a player's loss, no pressure) and burns stores
        if not self.sc.duel:
            for f in self.factions.values():
                if f.c.name == "frostmaw" or f.dead:
                    continue
                if rng.random() < self.sc.dragons / YEAR:
                    lost = f.pool * rng.uniform(0.3, 0.6)
                    f.pool -= lost
                    f.stores *= 0.5
                    f.fear = min(1.0, f.fear + 0.5)
                    self.log(f, "dragon", "", reason=f"a dragon struck {f.name}", dead=round(lost))
                    self.spread_news(f.id, "weak", f.defence())
                if f.ground == 1.0 and rng.random() < 0.1 / YEAR:
                    f.ground = 0.5
                    f.ground_until = t + rng.uniform(96, 192)
                    self.log(f, "ground_lost", "", reason=f"{f.name} lost its hunting ground")
            # border clashes: hunters and herders of two hostile neighbours meet; one side loses a few
            for a, b in self.hostile_pairs:
                fa, fb = self.factions[a], self.factions[b]
                if fa.dead or fb.dead:
                    continue
                rate = self.life["clash_rate"] * (0.5 + fa.hunger() + fb.hunger()) / YEAR
                if rng.random() < rate:
                    victim, doer = (fa, fb) if rng.random() < 0.5 else (fb, fa)
                    k = float(min(max(victim.pool - 1, 0), rng.integers(1, 3)))
                    victim.pool -= k
                    self.log(victim, "clash", doer.id, reason=f"{doer.name}'s hunters killed {k:.0f} of "
                             f"{victim.name}'s at the border", dead=k)
                    rr = victim.rel.get(doer.id)
                    if rr is not None:
                        rr.g += 15 + victim.c.kill_w * k
                        rr.W = min(2.0, rr.W + 0.3 * (1 - rr.W / 2))
                        self.remember(victim, doer.id, 1.0 + k / 2,
                                      f"{doer.name}'s hunters killed {k:.0f} of ours at the border {self.when(t)}")
                        self.occasion(victim, "provoked", doer.id, delay=1 + rng.exponential(3))
            # caravans pass the camps near the road: an opportunity
            if rng.random() < 1 / 3:
                for fid in self.caravan_near:
                    self.occasion(self.factions[fid], "caravan", "CARAVAN")
        if self.rail:
            n = rng.poisson(self.sc.rail_legs)
            for _ in range(n):
                for fid in ("G1", "F2"):
                    f = self.factions[fid]
                    if rng.random() < self.life["notice_train"]:
                        self.occasion(f, "train", "RAIL")
            if t >= self.players.get("P:rail", PlayerScript("", "", 0, False, 0, ())).escort_until:
                self.rail_d = 1.0
        # news arrives
        keep = []
        for item in self.news:
            if item[0] <= t:
                _, fid, kind, about, obs = item
                f = self.factions[fid]
                r = f.rel.get(about)
                if r is not None:
                    if kind == "away":
                        # a passing state, not his strength: the belief of the base is left alone
                        r.away_until = t + self.rng.uniform(3, 10)
                    else:
                        self.observe(r, obs, self.life["rumour_sd"])
                    self.occasion(f, "news", about)
            else:
                keep.append(item)
        self.news = keep

    def spread_news(self, about, kind, true_d, exclude=None):
        """Word of a camp's weakness reaches the factions that know it, late and inexact (the one that
        did the harm saw it for itself)."""
        for f in self.factions.values():
            if about in f.rel and f.id != exclude:
                obs = max(0.5, true_d) * math.exp(self.rng.normal(0, self.life["rumour_sd"]))
                self.news.append((self.t + self.rng.exponential(6.0), f.id, kind, about, obs))

    def observe(self, r, d, sd):
        """Kalman step on the log-normal belief."""
        z = math.log(max(d, 0.3))
        k = r.s2 / (r.s2 + sd * sd)
        r.mu += k * (z - r.mu)
        r.s2 = (1 - k) * r.s2

    def player_day(self, p):
        t = self.t
        rng = self.rng
        if p.trips and t >= p.next_trip and t >= p.away_until:
            p.away_until = t + rng.uniform(8, 20)
            p.next_trip = p.away_until + rng.exponential(60)
            for fid in p.reach:
                f = self.factions[fid]
                if rng.random() < 0.5:
                    self.news.append((t + rng.exponential(4.0), fid, "away", p.pid,
                                      p.defence(t) * math.exp(rng.normal(0, 0.3))))
        if p.kind == "adaptive":
            p.quiet += 1
            if p.quiet >= 24:
                p.d = max(p.d0, p.d - 1.0)
                p.quiet = 0
        if p.hunts and t >= p.next_hunt:
            p.next_hunt = t + rng.exponential(p.hunt_every)
            f = self.factions[p.hunts]
            if f.pool >= 2:
                k = float(min(f.pool - 1, rng.integers(3, 7)))
                f.pool -= k
                self.player_loss(f, k)
                self.hurt_by_player(f, p.pid, k, f"{self.tname(p.pid)} killed {k:.0f} of ours at our camp {self.when(t)}")
                self.log(f, "hunted", p.pid, reason=f"{self.tname(p.pid)} hunted the camp", dead=k)
                if f.c.council and rng.random() < 0.08:
                    self.succeed(f, self.blame(f, p.pid), "was killed by a player")
        if p.farm_den:
            f = self.factions[p.farm_den]
            if not f.dead and t >= f.dead_until + 2:
                self.kill_frostmaw(f, p.pid, f"killed at its den by {self.tname(p.pid)}")
        if p.rob_den and t >= p.next_rob:
            p.next_rob = t + rng.exponential(p.rob_every)
            f = self.factions[p.rob_den]
            if not f.dead:
                self.rob(f, p)

    def blame(self, f, pid):
        """Who the faction blames for a player's deed: usually the doer, sometimes a player it confuses
        him with (no witness lived, or the two are seen together)."""
        others = [x for x in f.rel if x in self.players and x != pid]
        if self.mode == "life" and others and self.rng.random() < self.sc.blame_spill:
            return others[int(self.rng.integers(len(others)))]
        return pid

    def hurt_by_player(self, f, pid, k, text):
        who = self.blame(f, pid)
        if who not in f.rel:
            return
        r = f.rel[who]
        r.g += f.c.kill_w * k
        r.W = min(2.0, r.W + 0.3 * min(k, 5) / 5 + 0.2)
        f.fear = min(1.0, f.fear + k / max(f.c.pool_cap, 1))
        self.remember(f, who, 1.0 + k / 5, text if who == pid else text.replace(self.tname(pid), self.tname(who)))
        self.occasion(f, "provoked", who)

    def kill_frostmaw(self, f, pid, how):
        t = self.t
        f.dead = True
        f.pool = 0
        P = self.pressure(f) + 1.0
        f.P, f.P_t = P, t
        f.dead_until = t + min(192.0, 6.0 * 2 ** P)    # EQ-4 (b), Ben 2026-10-21
        self.log(f, "killed", pid, reason=f"{f.voices[0].name} was {how}")

    def rob(self, f, p):
        """The player takes the Frostmaw's crystal while it sleeps; it may wake."""
        t = self.t
        rng = self.rng
        v = f.voices[0]
        wake = 0.15 + 0.6 * v.aggr * (1 - f.fear)
        r = f.rel.setdefault(p.pid, Rel(p.pid))
        if rng.random() < wake:
            win, la, ld = battle(rng, f.c.home_str / 1.2, p.strength)
            if win:
                p.stock *= 0.8
                r.W *= 0.5
                self.log(f, "fight", p.pid, reason=f"{v.name} woke and drove the thief {self.tname(p.pid)} off",
                         win=True, dead=0)
                self.remember(f, p.pid, 1.5, f"drove {self.tname(p.pid)} from the den {self.when(t)}")
            else:
                self.kill_frostmaw(f, p.pid, f"killed by the thief {self.tname(p.pid)} when it woke")
            return
        r.g += 60
        r.W = min(2.0, r.W + f.c.insult_pride)
        self.remember(f, p.pid, 3.0, f"{self.tname(p.pid)} stole the crystal {self.when(t)}")
        self.log(f, "robbed", p.pid, reason=f"{self.tname(p.pid)} stole {v.name}'s crystal")
        self.occasion(f, "provoked", p.pid, delay=1 + rng.exponential(3))   # it finds the crystal gone on waking

    # ------------------------------------------------------------------------------- acts
    def send(self, f, x, size, A, carry, act, reason, speaker="", chief_leads=False):
        """A war party fights target x now. Returns the event."""
        t = self.t
        rng = self.rng
        d = self.target_defence(x)
        if act == "skirmish":
            d *= max(self.life["field_share"], min(1.0, 4.0 / max(self.factions[x].pool, 1.0)))
        win, la, ld = battle(rng, A, d)
        dead = round(size * la, 1)
        e = self.log(f, act, x, reason=reason, speaker=speaker, win=win, dead=dead, size=size,
                     A=round(A, 1), d=round(d, 1))
        f.last_fight = t
        f.busy_until = t + 2
        if f.c.name == "frostmaw":
            e["dead"] = 0
            if not win and rng.random() < 0.5 and x in self.players:
                self.kill_frostmaw(f, x, f"killed attacking {self.tname(x)}")
                e["dead"] = 1
        else:
            f.pool = max(0.0, f.pool - dead)
        f.fear = min(1.0, f.fear + dead / max(f.c.pool_cap, 1) * 2 + (0.2 if not win else 0))
        r = f.rel.get(x)
        if r is not None:
            if x in self.players and t < self.players[x].away_until:
                self.observe(r, 2 * d, 0.3)    # it met his walls without him, and guesses him at home
            else:
                self.observe(r, d, 0.15)
        loot = 0.0
        if win:
            loot = min(carry * (1 - la), self.target_stock(x))
            f.stores += loot
            e["loot"] = round(loot, 1)
            if r is not None:
                r.g *= 0.5            # vengeance sated
                r.W *= 0.3            # honour answered
        else:
            if r is not None:
                r.cowed_until = t + f.c.cowed
                if self.mode == "life":
                    r.W = min(2.0, 0.6 * r.W + 0.2)     # honour answered, but a defeat stings
                    nd = f"{dead:.0f}"                  # as the log line shows it (0.5 shows as 0)
                    what = (f"we lost {nd} at {self.tname(x)}" if nd != "0"
                            else f"{self.tname(x)} drove us off")
                    self.remember(f, x, 1.0 + dead / 5, f"{what} {self.when(t)}")
        # chief at risk when he leads
        if chief_leads and f.c.council is not None:
            if rng.random() < (0.3 if not win else 0.03):
                killer = x if x in self.players or x in self.factions else None
                if f.c.council:
                    self.succeed(f, killer, f"fell leading the attack on {self.tname(x)}")
        # the target side
        if x in self.factions:
            g = self.factions[x]
            kd = g.home() * ld * (max(self.life["field_share"], min(1.0, 4.0 / max(g.pool, 1.0)))
                                  if act == "skirmish" else 1.0)
            g.pool = max(0.0, g.pool - kd)
            if win:
                g.stores -= loot
            rr = g.rel.setdefault(f.id, Rel(f.id))
            small = act == "skirmish"
            rr.g += g.c.kill_w * kd + (10 if small else 20)
            rr.W = min(2.0, rr.W + (0.3 if small else 0.6) * (1 - rr.W / 2))   # each insult adds less
            g.fear = min(1.0, g.fear + kd / max(g.c.pool_cap, 1) * 2)
            g.last_fight = t
            self.remember(g, f.id, 1.5 + kd / 5, f"{f.name} {'fell on our hunters' if small else 'raided us'}"
                          f" {self.when(t)}")
            self.occasion(g, "provoked", f.id, delay=1 + self.rng.exponential(3))
            self.spread_news(x, "weak", g.defence(), exclude=f.id)
        elif x in self.villages:
            if win:
                self.villages[x]["stock"] -= loot
        elif x in self.players:
            p = self.players[x]
            if win:
                p.stock -= loot
            self.player_loss(f, dead)        # the defenders are the player's: a player-caused loss
            if p.kind == "adaptive" and win:
                p.d += 6.0
            if p.kind == "adaptive":
                p.quiet = 0
            if r is not None and dead > 0:
                r.g += f.c.kill_w * dead
        elif x == "RAIL":
            p = self.players.get("P:rail")
            if p is not None:
                p.stock -= loot
                p.robbed = getattr(p, "robbed", [])
                p.robbed = [s for s in p.robbed if s > t - 30] + [t]
                if len(p.robbed) >= 3 and t >= p.escort_until:
                    p.escort_until = t + 96        # he puts guards on the trains for a year
                    self.rail_d = 20.0
                    self.log(f, "guards", "RAIL", reason="robbed three times in a month, the railway puts "
                             "guards on its trains for a year")
        return e

    def demand(self, f, x, T, X, speaker="", reason=""):
        """An envoy asks x for T now; the answer comes back the same day in the model."""
        t = self.t
        r = f.rel[x]
        pays = False
        killed = False
        top = X * (1 + self.life["raid_damage"]) if self.mode == "life" else X
        if x in self.villages:
            v = self.villages[x]
            pays = T <= top + 1e-9 and v["stock"] >= T
            if pays:
                v["stock"] -= T
        else:
            p = self.players[x]
            if p.kills_envoys:
                killed = True
            elif p.pays and T <= top + 1e-9 and p.stock >= T:
                pays = True
                p.stock -= T
        e = self.log(f, "demand", x, reason=reason, speaker=speaker, T=round(T, 1), paid=pays, envoy_killed=killed)
        if self.mode == "life" and not killed:
            self.observe(r, self.target_defence(x), 0.3)    # the envoy party walked to the base and saw it
        if pays:
            f.stores += T
            r.g *= 1 - max(f.c.tribute_relief, 0.3)
            r.W *= 0.3
            r.ta += 1
            r.next_ok = t + f.c.respite
            r.paid_until = t + self.life["tribute_respite"] * (0.5 + self.rng.random())
            r.truce_until = r.paid_until if self.mode == "life" else t + 24
            r.truce_by = f.chief().name
            self.remember(f, x, 0.5, f"{self.tname(x)} paid {T:.0f} {self.when(t)}", hostile=False)
        else:
            r.tb += 1
            r.refused_at = t
            jump = f.c.insult_pride if killed else f.c.refusal_pride
            if self.mode == "life":
                r.W = min(2.0, r.W + jump)
                r.g += 30 if killed else 10
                self.remember(f, x, 2.5 if killed else 1.5,
                              f"{self.tname(x)} {'killed our envoy' if killed else 'refused our envoy'} {self.when(t)}")
                self.occasion(f, "refused", x)
            if killed:
                f.pool = max(0.0, f.pool - 1)
                self.player_loss(f, 1)
        return e

    # ------------------------------------------------------------------------------- ladder
    def ladder_day(self, f):
        """Today's rule per relation: rungs in turn, war rungs repeat at their dwell."""
        t = self.t
        c = f.c
        if f.dead:
            return
        for x, r in f.rel.items():
            if not (x in self.players or x == "RAIL"):
                continue           # today's feuds are with actors; camps and villages get plunder
            target = max(k for k in range(7) if r.g >= c.thr[k])
            if r.step > 0 and r.g < c.hyst * c.thr[r.step]:
                r.step -= 1
            if t < r.cowed_until or t < r.next_ok or t < f.busy_until:
                continue
            if target > r.step:
                r.step += 1
                rung = r.step
            elif r.step >= 4:
                rung = r.step
            else:
                continue
            self.rung(f, x, r, rung, why="grievance crossed the " + RUNG_NAMES[rung] + " rung")
            r.next_ok = t + c.dwell[rung]
        self.today_plunder(f)

    def rung(self, f, x, r, rung, why):
        c = f.c
        if rung == 1:
            self.log(f, "warn", x, reason=why)
        elif rung == 2:
            if self.can_pay(x) and c.tribute_per_g > 0:
                T = math.ceil(c.tribute_per_g * r.g)
                X = self.expected_loss(f, x, 4)
                self.demand(f, x, T, X, reason=why)
                if r.ta > 1 and r.pending is None and r.refused_at < self.t:
                    r.g *= 1 - c.tribute_relief
            else:
                self.log(f, "warn", x, reason=why)
        elif rung == 3:
            size, A, carry = c.party[3]
            if size > 0 and f.home() >= 1 or c.name == "umvuthana":
                self.scout(f, x, why)
        else:
            size, A, carry = c.party[rung]
            if c.name == "frostmaw" and x == "RAIL":
                self.send(f, x, 1, A, carry, "ambush", why)
                return
            if f.home() < size / 2:
                return
            n = min(f.home(), size)
            self.send(f, x, n, A * n / size, carry, "raid" if rung == 4 else "invasion", why)

    def scout(self, f, x, why, speaker=""):
        size, A, carry = f.c.party[3]
        r = f.rel[x]
        if size == 0:          # a raptor pack or the Frostmaw's look: draws on no pool
            self.observe(r, self.target_defence(x), 0.25)
            self.log(f, "scout", x, reason=why, speaker=speaker, dead=0)
            return
        d = self.target_defence(x)
        win, la, ld = battle(self.rng, A, d)
        dead = round(size * la * 0.5, 1) if d > A else 0.0
        f.pool = max(0.0, f.pool - dead)
        if x in self.players and dead:
            self.player_loss(f, dead)
            r.g += f.c.kill_w * dead
        self.observe(r, d, 0.25)
        self.log(f, "scout", x, reason=why, speaker=speaker, dead=dead)

    def today_plunder(self, f, gate=False):
        """Today's plunder (engine.rs schedule_plunder and plunder): a Poisson clock per camp with mean
        plunder_mean_days, against the nearest hated village or camp in reach. The gate sends it only
        when it is worth sending against the target's true defence (EQUILIBRIUM 2.5 item 5)."""
        if self.t < f.next_plunder:
            return
        f.next_plunder = self.t + self.rng.exponential(f.c.plunder_mean)
        x = next((x for x in f.rel if self.hated(f, x)), None)
        if x is None or self.t < f.busy_until:
            return
        size, A0, carry = f.c.party[8]
        if size > 0 and f.home() < size / 2:
            return
        n = min(f.home(), size) if size > 0 else 0
        A = A0 * n / size if size > 0 else A0
        if gate:
            d = self.target_defence(x)
            pw = A * A / (A * A + d * d)
            lw, ll = lw_ll(A, d)
            ev = pw * min(carry, self.target_stock(x)) - GATE["warrior_value"] * n * (pw * lw + (1 - pw) * ll)
            if ev <= 0:
                return
        self.send(f, x, n, A, carry, "plunder", "the plunder clock rang" + (" and it was worth it" if gate else ""))

    def hated(self, f, x):
        if x in self.villages:
            return self.villages[x]["kind"] in HATES[f.c.name]
        if x in self.factions:
            return self.factions[x].c.name in HATES[f.c.name] and not self.factions[x].dead
        return False

    def expected_loss(self, f, x, rung):
        size, A, carry = f.c.party[rung]
        if rung == 5 and f.c.invasion_pride > 0:
            A += f.c.chief_str
        d = self.target_defence(x)
        return A * A / (A * A + d * d) * min(carry, self.target_stock(x))

    # ------------------------------------------------------------------------------- gate
    def gate_day(self, f):
        t = self.t
        c = f.c
        if f.dead:
            return
        gp = GATE
        for x, r in f.rel.items():
            if not (x in self.players or x == "RAIL"):
                continue
            # belief decays to the prior with a 96-day half-life (EQUILIBRIUM 2.5 item 1)
            r.mu = math.log(15.0) + (r.mu - math.log(15.0)) * 2 ** (-1 / gp["belief_half"])
            if t < r.cowed_until or t < r.next_ok or t < f.busy_until:
                continue
            if r.gate_demand is not None:
                deadline, T, rung = r.gate_demand
                if t < deadline:
                    continue
                r.gate_demand = None
                r.refused_at = t
                size = c.party[rung][0]
                if f.home() >= size / 2:
                    n = min(f.home(), size)
                    self.send(f, x, n, c.party[rung][1] * n / size, c.party[rung][2],
                              "raid" if rung == 4 else "invasion", "the demand was refused")
                    r.next_ok = t + c.dwell[rung]
                continue
            belief = math.exp(r.mu)
            g = r.g
            rev = gp["revenge_max"] * g / (g + gp["revenge_half"])
            best, best_ev = None, 0.0
            for rung in (4, 5, 6):
                if g < c.thr[rung] and not (c.name == "frostmaw" and x == "RAIL"):
                    continue
                size, A0, carry = c.party[rung]
                if size == 0 or f.home() < size / 2:
                    continue
                n = min(f.home(), size)
                A = A0 * n / size
                pw = A * A / (A * A + belief * belief)
                lw, ll = lw_ll(A, belief)
                ev = pw * min(carry, self.target_stock(x)) - gp["warrior_value"] * n * (pw * lw + (1 - pw) * ll) + rev
                if ev > best_ev:
                    best, best_ev = rung, ev
            if best is not None:
                size, A0, carry = c.party[best]
                n = min(f.home(), size)
                A = A0 * n / size
                pw = A * A / (A * A + belief * belief)
                X = pw * min(carry, self.target_stock(x))
                T = max(1, math.ceil(0.5 * (best_ev + max(X, best_ev))))
                if self.can_pay(x) and g >= c.thr[2] and t - r.refused_at > 24 and c.tribute_per_g > 0:
                    e = self.demand(f, x, T, self.expected_loss(f, x, best), reason="terms first (the gate)")
                    if not e["paid"]:
                        r.gate_demand = (t + 3, T, best)
                    continue
                act = "ambush" if x == "RAIL" else ("raid" if best == 4 else "invasion")
                self.send(f, x, n, A, carry, act, "the attack was worth sending (the gate)")
                r.next_ok = t + c.dwell[best]
                continue
            if g >= c.thr[3] and t - r.last_probe >= gp["probe_days"] and (f.home() >= 1 or c.party[3][0] == 0):
                r.last_probe = t
                self.scout(f, x, "the 48-day probe (the gate)")
                r.next_ok = t + c.dwell[3]
        self.today_plunder(f, gate=True)

    # ------------------------------------------------------------------------------- life
    def occasion(self, f, kind, about, delay=0.0):
        """Something the council will sit on: today, or after the war party has gathered (delay days)."""
        if self.mode != "life" or f.dead:
            return
        f.pending = getattr(f, "pending", [])
        f.pending.append((self.t + delay, kind, about))

    def life_day(self, f):
        t = self.t
        if f.dead:
            f.pending = []
            return
        occ = [o for o in getattr(f, "pending", []) if o[0] <= t]
        f.pending = [o for o in getattr(f, "pending", []) if o[0] > t]
        woke = False
        for _, kind, about in occ:
            p_attend = {"provoked": 1.0, "refused": 1.0, "news": 0.7, "caravan": self.life["caravan_attend"], "train": 1.0}[kind]
            if self.rng.random() < p_attend:
                self.decide(f, kind, about)
                woke = True
        if t >= f.next_wake:
            if not woke:
                self.decide(f, "restless", None)
            f.next_wake = t + self.rng.exponential(self.wake_mean(f))

    def wake_mean(self, f):
        c = f.c
        t = self.t
        h = f.hunger()
        R = f.restless(t)
        W = max((r.W for r in f.rel.values()), default=0.0)
        gmax = max((r.g for r in f.rel.values()), default=0.0)
        rate = (1 + 2.0 * h + 1.0 * R + 0.8 * W + 0.5 * gmax / (gmax + 50)) / c.wake_days
        return 1.0 / rate

    def speaker(self, f):
        vs = f.voices
        w = np.array([v.standing for v in vs])
        i = int(self.rng.choice(len(vs), p=w / w.sum()))
        v = vs[i]
        # the speaker carries the day, the rest of the council tempers him
        mb = float(np.dot(w, [x.bold for x in vs]) / w.sum())
        ma = float(np.dot(w, [x.aggr for x in vs]) / w.sum())
        ms = float(np.dot(w, [x.soc for x in vs]) / w.sum())
        k = self.life["speaker_weight"]
        return v, k * v.bold + (1 - k) * mb, k * v.aggr + (1 - k) * ma, k * v.soc + (1 - k) * ms

    def decide(self, f, kind, about):
        """One council sitting. Each matter (target) gets its best act against one draw from the
        faction's belief (Thompson sampling, bent by the speaker's boldness); the sitting then picks
        among staying home and the matters by a logit with the culture's temperature."""
        t = self.t
        c = f.c
        L = self.life
        rng = self.rng
        if t < f.busy_until:
            return
        self.sittings += 1
        self.matters += len(f.rel)
        v, bold, aggr, soc = self.speaker(f)
        h = f.hunger()
        R = f.restless(t)
        room = f.store_room()
        heir = 2.0 if t < f.heir_until and v.role in ("chief", "war") else 1.0
        fearm = 1 + c.fear_w * f.fear * (1.5 - bold)
        vloot = L["loot_base"] + c.greed * h

        def value(take):
            return min(take, room) * vloot + max(0.0, take - room) * L["surplus"]

        if kind in ("caravan", "train", "provoked", "refused", "news"):
            matters = [about] if about in f.rel else []
        else:
            matters = [x for x in f.rel if x != "CARAVAN"]
        options = [(0.0, "wait", None, None)]
        for x in matters:
            r = f.rel[x]
            if x in self.factions and self.factions[x].dead:
                continue
            war_ok = not (x == "RAIL" and kind != "train") and not (x == "CARAVAN" and kind != "caravan")
            best = None
            Dhat = math.exp(r.mu + math.sqrt(r.s2) * rng.normal() - L["bold_shift"] * (bold - 0.5))
            if t < r.away_until:
                Dhat *= 0.5                                  # the owner is away: only his walls are home
            stock = self.target_stock(x)
            g = r.g
            rev = c.rev_max * (0.5 + aggr) * g / (g + 100)
            pride = c.pride_w * (0.5 + aggr) * min(r.W, 1.5)    # scaled per act by the hope of answering it
            glory = c.glory_w * R * heir * (0.5 + bold)
            penalty = 0.0
            if t < r.truce_until:              # an oath binds the chief who swore it; an heir less
                penalty = c.oath * (1.5 - aggr) * (L["sworn_mult"] if f.chief().name == r.truce_by else 1.0)
            if x in self.factions and self.factions[x].c.name == c.name:
                penalty += c.kin_cost
            if x in self.players and r.ta > 1.5:
                # a raid on a tributary ends the tribute: what the payments were worth, by its record
                penalty += L["tributary"] * (r.ta - 1) / (r.ta + r.tb - 2 + 1e-9)
            if x in self.factions and r.sigma == 0:
                penalty += L["peace_cost"] * (1 - min(1.0, g / 30)) * (1 - min(1.0, r.W))
            preempt = 0.0
            if x in self.factions and c.preempt_w > 0:
                ratio = self.factions[x].pool / max(f.pool, 1.0)
                preempt = c.preempt_w * min(1.0, max(0.0, ratio - 1.2))
            threat = None          # the war act an envoy can name, even one pride has not yet unlocked
            if war_ok:
                sizes = (8, 4, 5) if c.name != "frostmaw" else (4,)
                for rung in sizes:
                    size, A0, carry = c.party[rung]
                    if c.name == "frostmaw":
                        if x != "RAIL" and x not in self.players:
                            continue
                        n, A = 1, A0
                    else:
                        if size == 0 or f.home() < max(1.0, size / 2):
                            continue
                        n = min(f.home(), size)
                        A = A0 * n / size
                    chief_leads = False
                    gated = False
                    if rung == 5 and c.invasion_pride > 0:
                        gated = r.W < c.invasion_pride     # the Umvuthi leads in person only on wounded pride
                        A += c.chief_str
                        chief_leads = True
                    pw = A * A / (A * A + Dhat * Dhat)
                    lw, ll = lw_ll(A, Dhat)
                    lost = n * (pw * lw + (1 - pw) * ll)
                    take = min(carry, stock)
                    gain = pw * value(take)
                    need = pw * min(take, room) * c.greed * h      # the part of the gain that is hunger
                    if x in ("RAIL", "CARAVAN"):
                        gain *= c.ambush_w
                        need *= c.ambush_w
                    if c.name == "frostmaw":
                        cost = (1 - pw) * 0.5 * L["frostmaw_life"] * fearm + c.effort
                    else:
                        cost = c.warrior_value * lost * fearm + L["effort_member"] * n + c.effort
                    hope = 0.25 + 0.75 * pw                        # a hopeless attack answers little
                    u = gain - cost + rev + pride * hope + glory + preempt - penalty
                    act = "ambush" if x in ("RAIL", "CARAVAN") else (
                        "invasion" if rung == 5 else ("raid" if rung == 4 else "band"))
                    if c.name == "frostmaw" and x in self.players:
                        act = "raid"
                    why = {"hunger": need, "loot": gain - need, "revenge": rev,
                           "pride": pride * hope, "glory": glory, "preempt": preempt}
                    opt = (u, act, x, (n, A, carry, chief_leads, why, pw))
                    if threat is None or u > threat[0]:
                        threat = opt
                    if not gated and (best is None or u > best[0]):
                        best = opt
                if x in self.factions and c.name != "frostmaw" and c.party[8][0] > 0 and f.home() >= 4:
                    size, A0, carry = c.party[8]
                    n = min(f.home(), size)
                    A = A0 * n / size
                    tg = self.factions[x]
                    share = max(L["field_share"], min(1.0, 4.0 / max(tg.pool, 1.0)))
                    Df = Dhat * share
                    pw = A * A / (A * A + Df * Df)
                    lw, ll = lw_ll(A, Df)
                    lost = n * (pw * lw + (1 - pw) * ll)
                    take = min(carry * 0.25, stock)            # a few beasts and what the hunters carried
                    gain = pw * value(take)
                    need = pw * min(take, room) * c.greed * h
                    cost = c.warrior_value * lost * fearm + L["effort_member"] * n + c.effort
                    hope = 0.25 + 0.75 * pw
                    u = gain - cost + rev + pride * hope + glory + preempt - penalty
                    why = {"hunger": need, "loot": gain - need, "revenge": rev,
                           "pride": pride * hope, "glory": glory, "preempt": preempt}
                    if best is None or u > best[0]:
                        best = (u, "skirmish", x, (n, A, carry * 0.25, False, why, pw))
            # demand: an envoy, where the threat is credible and no tribute is running
            if (threat is not None and threat[1] != "ambush" and self.can_pay(x) and t >= r.paid_until
                    and t - r.refused_at > 12 and c.demand_bias > -50):
                war_u = threat[0]
                n, A, carry, _, _, pw = threat[3]
                if pw > 0.25:
                    X = pw * min(carry, stock)                      # what the camp believes x stands to lose
                    material = war_u - (rev + threat[3][4]["pride"] + glory + preempt - penalty)
                    T_min = max(1.0, material / max(vloot, 1e-6))   # the war's worth to the camp, in items
                    Xd = X * (1 + L["raid_damage"])
                    if T_min < Xd:
                        k = 0.25 + 0.4 * aggr + 0.15 * h            # an aggressive speaker asks nearer the top
                        T = T_min + k * (Xd - T_min)
                        trust = r.ta / (r.ta + r.tb)
                        u = trust * value(T) * (1 + c.demand_bias / 10) + (1 - trust) * 0.9 * max(0.0, war_u) \
                            + 0.3 * pride - 0.5 * c.effort - 3 * L["effort_member"]
                        if best is None or u > best[0]:
                            rung = {"raid": 4, "invasion": 5, "band": 8}[threat[1]]
                            best = (u, "demand", x, (T, X, trust, rung))
            if c.scout_w > 0 and x != "CARAVAN":
                relev = g / (g + 50) + h * (stock > 0) + 0.5 * (kind == "news")
                u = c.scout_w * math.sqrt(r.s2) * relev - 0.8 * c.effort
                if best is None or u > best[0]:
                    best = (u, "scout", x, None)
            # a warning is for someone who can heed it: a player, the railway, or a camp we are angry with
            # (a player or the railway hears it at most once in 16 days: more would be noise to him)
            if (x in self.players or x == "RAIL") and t - r.last_warn > 16 or (x in self.factions and r.W > 0.5):
                u = c.warn_w * (0.5 + aggr) * (g / (g + 40) + 0.5 * min(r.W, 1.5)) - 0.3 * c.effort - 3.0
                if best is None or u > best[0]:
                    best = (u, "warn", x, None)
            if x in self.factions and t >= r.truce_until and (g > 30 or r.W > 0.5):
                u = 10 * soc + 12 * f.fear - pride - rev - 2
                if best is None or u > best[0]:
                    best = (u, "truce", x, None)
            if c.hunts and h > 0.2 and x in self.villages:
                u = value(8 * (1 + h)) - c.effort * 0.5
                if best is None or u > best[0]:
                    best = (u, "hunt", x, None)
            if best is not None:
                options.append(best)
        tau = c.tau * L["tau_mult"] * (0.75 + 0.5 * aggr)
        us = np.array([o[0] for o in options]) / tau
        us -= us.max()
        p = np.exp(us)
        p /= p.sum()
        i = int(rng.choice(len(options), p=p))
        u, act, x, info = options[i]
        n_ev = len(self.events)
        self.carry_out(f, v, act, x, info, kind, about, u)
        if self.debug and len(self.events) > n_ev:
            self.events[n_ev]["options"] = [(round(o[0], 1), o[1], o[2]) for o in options]

    def carry_out(self, f, v, act, x, info, kind, about, u):
        t = self.t
        c = f.c
        if act == "wait":
            return
        r = f.rel[x]
        tn = self.tname(x)
        if act in ("raid", "invasion", "band", "ambush", "skirmish"):
            n, A, carry, chief_leads, why, pw = info
            pos = {k: w for k, w in why.items() if w > 0}
            tot = sum(pos.values()) or 1.0
            drivers = sorted((k for k, w in pos.items() if w >= self.life["driver_share"] * tot),
                             key=lambda k: -pos[k]) or ["loot"]
            top = drivers[0]
            reason = self.story_reason(f, x, r, top, kind, about)
            if "pride" in drivers and "revenge" in drivers:        # one grudge, told once
                drivers.remove("pride" if pos["revenge"] >= pos["pride"] else "revenge")
                top = drivers[0]
                reason = self.story_reason(f, x, r, top, kind, about)
            if len(drivers) > 1:
                second = self.story_reason(f, x, r, drivers[1], "restless", about)
                if second not in reason:
                    reason += "; and " + second
            if self.t < r.truce_until:
                ch = f.chief().name
                reason += (f"; {ch} broke his own truce" if ch == r.truce_by
                           else f"; {ch} had not sworn {r.truce_by}'s truce")
            e = self.send(f, x, n, A, carry, act, reason, speaker=f"{v.name} ({v.role})",
                          chief_leads=chief_leads or (act == "invasion" and c.name == "raider"))
            e["motive"] = top
            e["drivers"] = drivers
            e["occasion"] = kind
            e["u"] = round(u, 1)
            e["p_believed"] = round(pw, 2)
            # standing: a win lifts whoever argued for it, a loss gives the floor to caution
            if f.c.council:
                if e["win"]:
                    v.standing = min(3.0, v.standing * 1.25)
                else:
                    v.standing = max(0.2, v.standing * 0.7)
                    f.voices[-1].standing = min(3.0, f.voices[-1].standing + 0.3)
        elif act == "demand":
            T, X, trust, rung = info
            reason = self.story_reason(f, x, r, "demand", kind, about)
            e = self.demand(f, x, T, self.expected_loss(f, x, rung), speaker=f"{v.name} ({v.role})", reason=reason)
            e["motive"] = "demand"
            e["occasion"] = kind
        elif act == "scout":
            self.scout(f, x, f"{v.name} wanted to know how strong {tn} is", speaker=f"{v.name} ({v.role})")
            self.events[-1]["occasion"] = kind
        elif act == "warn":
            what = {"frostmaw": "roared at", "umvuthana": "beat the gong at", "raider": "showed a war band to",
                    "bandit": "sent a warning to", "goblin": "howled at"}[c.name]
            self.log(f, "warn", x, reason=f"{v.name} {what} {tn}", speaker=f"{v.name} ({v.role})", occasion=kind)
            r.last_warn = t
            r.g *= 0.85
            r.W *= 0.8
        elif act == "truce":
            g = self.factions[x]
            rr = g.rel.get(f.id)
            accept = rr is None or (10 * g.chief().soc + 12 * g.fear - g.c.pride_w * min(rr.W, 1.5)) > 0 \
                or self.rng.random() < 0.2
            if accept:
                dur = self.rng.exponential(192)
                r.truce_until = t + dur
                r.truce_by = f.chief().name
                r.g *= 0.3
                r.W *= 0.3
                if rr is not None:
                    rr.truce_until = t + dur
                    rr.truce_by = g.chief().name
                    rr.g *= 0.3
                    rr.W *= 0.3
            self.log(f, "truce", x, reason=f"{v.name} offered {tn} a truce", accepted=accept,
                     speaker=f"{v.name} ({v.role})", occasion=kind)
        elif act == "hunt":
            loot = 8 * (1 + f.hunger())
            f.stores += loot
            if x in self.villages:
                self.villages[x]["stock"] = max(0.0, self.villages[x]["stock"] - loot)
            self.log(f, "hunt", x, reason=f"hungry, the raptors hunted {tn}'s animals", loot=loot, occasion=kind)
            f.last_fight = t

    def story_reason(self, f, x, r, top, kind, about):
        tn = self.tname(x)
        hostile = [m for m in r.memories if m[3]]
        mem = hostile[0][2] if hostile else None
        news = f"word came that {tn} was away; " if x in self.players else f"word came that {tn} was weak; "
        occ = {"provoked": "", "refused": "", "news": news,
               "caravan": "a caravan came by; ", "train": "a train came through; ", "restless": ""}[kind]
        if top == "revenge":
            if mem:
                return occ + "revenge: " + mem
            if x in self.players:
                return occ + f"anger at {tn}'s mines and smoke"
            return occ + f"old hatred of {tn}" if r.sigma > 0 else occ + f"a grudge against {tn}"
        if top == "pride":
            return occ + ("honour: " + mem if mem else "wounded pride")
        if top == "hunger":
            return occ + ("winter hunger" if season(self.t) == 3 else "empty stores")
        if top == "glory":
            return occ + ("the new war leader must prove himself" if self.t < f.heir_until else "restless warriors")
        if top == "preempt":
            return occ + f"{tn} is growing too strong"
        if top == "demand":
            kind_mem = [m for m in r.memories if not m[3]]
            if mem:
                return occ + "asking tribute after: " + mem
            if kind_mem:
                return occ + f"asking tribute again ({kind_mem[0][2]})"
            return occ + f"asking tribute of {tn}"
        return occ + "loot"

    # ------------------------------------------------------------------------------- run
    def run(self):
        days = self.sc.days
        for t in range(days):
            self.t = t
            self.world_day()
            for f in self.factions.values():
                self.day_faction(f)
            for f in self.factions.values():
                if self.mode == "ladder":
                    self.ladder_day(f)
                elif self.mode == "gate":
                    self.gate_day(f)
                else:
                    self.life_day(f)
        return self


GATE = {"warrior_value": 3.0, "revenge_max": 10.0, "revenge_half": 100.0, "belief_half": 96.0, "probe_days": 48.0}

LIFE_DEFAULTS = {
    "sworn_mult": 3.0,          # a truce costs this many times more to break for the chief who swore it
    "tributary": 12.0,          # cost of raiding a player who pays, at a record of always paying
    "loot_base": 0.2,           # value of an item of loot to a fed camp
    "bold_shift": 1.0,          # how far boldness lowers the believed defence (log units at bold 1)
    "belief_drift": 1.0 / 384,  # belief variance regained a day
    "rumour_sd": 0.5,
    "speaker_weight": 0.6,
    "tau_mult": 1.0,
    "notice_train": 0.05,
    "frostmaw_life": 60.0,      # what a Frostmaw's own life is worth to it, loot items
    "hate_sigma_faction": 1.0,  # grievance a day toward a hated neighbour camp
    "hate_sigma_village": 0.5,
    "surplus": 0.03,            # value of loot that does not fit the stores
    "effort_member": 0.3,       # cost of mustering, per member sent
    "tribute_respite": 48.0,    # days a paid tribute buys, on average
    "caravan_attend": 0.03,     # share of passing caravans a camp sits down to consider
    "clash_rate": 1.0,          # border clashes a year between two hostile neighbour camps (fed)
    "raid_damage": 0.5,         # what a lost raid costs the target beyond the loot, as a share of it
    "field_share": 0.15,        # share of a camp's strength out in the field, where a skirmish finds it
    "peace_cost": 8.0,          # falling on a camp with no quarrel and no old hate: making an enemy
    "driver_share": 0.25,       # a motive is named when it is at least this share of the push
}


def run_world(scenario: Scenario, mode: str, seed: int) -> World:
    return World(scenario, mode, seed).run()
