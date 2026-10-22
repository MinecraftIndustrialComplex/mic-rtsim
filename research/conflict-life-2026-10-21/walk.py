#!/usr/bin/env python3
"""Writes results/walkthroughs.md: the player walkthroughs of CONFLICT-LIFE.md section 3, from the life
model. Each walkthrough is the seed (of 16) whose headline count is nearest the median of the 16, so the
story shown is a typical one, not a picked one.

  nice -n 10 python3 walk.py
"""
from __future__ import annotations

import collections
import os
from dataclasses import replace

import numpy as np

import model
import run

YEAR = model.YEAR
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "results")
SEEDS = range(1000, 1016)


def median_seed(worlds, metric):
    xs = [metric(w) for w in worlds]
    med = float(np.median(xs))
    i = min(range(len(xs)), key=lambda k: (abs(xs[k] - med), k))
    return worlds[i], xs[i], med


def fights_on(w, who):
    return sum(1 for e in w.events if e["act"] in run.FIGHT and e["tgt"] in who)


def touches(w, e, focus):
    """An event belongs to a player's story if it is aimed at the focus, done by a focus faction toward
    a player, or is a succession, death or arrival in a focus faction."""
    if e["tgt"] in focus:
        return True
    if e["f"] in focus and e["act"] in ("succession", "killed", "arrive", "robbed", "fight", "hunted", "guards"):
        return True
    return False


def walk(name, focus_of, metric, years, title, note, **change):
    sc = replace(run.scenarios(50 * YEAR)[name], **change)
    worlds = [model.World(sc, "life", s).run() for s in SEEDS]
    w, x, med = median_seed(worlds, metric)
    focus = focus_of(w)
    L = [f"## {title}\n", f"{note} Seed {w.seed}: {x:.0f} against a median of {med:.0f} over 16 seeds.\n"]
    for e in w.events:
        if e["t"] >= years * YEAR:
            break
        if touches(w, e, focus):
            ln = run.fmt(w, e)
            if ln:
                L.append(f"- {ln}")
    # the whole run, by decade
    L.append("\nThe whole 50 years, by decade (fights aimed at the focus; attackers killed there):\n")
    L.append("| Decade | Fights | Of them lost | Attackers killed | Envoys | Paid |")
    L.append("|---|---|---|---|---|---|")
    for d in range(5):
        es = [e for e in w.events if d * 10 * YEAR <= e["t"] < (d + 1) * 10 * YEAR and e["tgt"] in focus]
        fs = [e for e in es if e["act"] in run.FIGHT]
        dm = [e for e in es if e["act"] == "demand"]
        L.append(f"| {d * 10 + 1}-{d * 10 + 10} | {len(fs)} | {sum(1 for e in fs if not e.get('win'))} | "
                 f"{sum(e.get('dead', 0) for e in fs):.0f} | {len(dm)} | {sum(1 for e in dm if e.get('paid'))} |")
    L.append("")
    return L


def rail_sweep():
    """Train robberies a year by traffic: the Frostmaw feels legs a day through MOWZIE's rail meter."""
    base = run.scenarios(20 * YEAR)["railway"]
    L = ["Robberies and warnings a year by traffic on the line (life, 8 seeds x 20 years each):\n",
         "| Legs a day | Rail meter grievance (MOWZIE 4.3) | Frostmaw lurks | Frostmaw roars | Goblin ambushes | "
         "Guards put on |", "|---|---|---|---|---|---|"]
    for legs in (2, 4, 8, 12, 24):
        c = collections.Counter()
        for s in range(1000, 1008):
            w = model.World(replace(base, rail_legs=float(legs)), "life", s).run()
            for e in w.events:
                if e["tgt"] in ("RAIL", "P:rail"):
                    c[(w.factions[e["f"]].c.name, e["act"])] += 1
        n = 8 * 20
        L.append(f"| {legs} | {8.66 * max(0, legs - 2):.1f} | {c[('frostmaw', 'ambush')] / n:.2f} | "
                 f"{c[('frostmaw', 'warn')] / n:.2f} | {c[('goblin', 'ambush')] / n:.2f} | "
                 f"{(c[('goblin', 'guards')] + c[('frostmaw', 'guards')]) / n:.2f} |")
    L.append("")
    return L


def alone(years_full=3):
    sc = run.scenarios(50 * YEAR)["alone"]
    worlds = [model.World(sc, "life", s).run() for s in SEEDS]

    def camp_fights(w):
        return sum(1 for e in w.events if e["act"] in run.FIGHT and e["tgt"] in w.factions)

    w, x, med = median_seed(worlds, camp_fights)
    L = ["## A world left alone for 50 years\n",
         f"No player. Seed {w.seed}: {x} fights between camps in 50 years against a median of {med:.0f} over "
         "16 seeds. Each decade's line is its count of each kind of event, then the decade's bloodiest fight "
         "between camps.\n",
         "| Decade | Fights (camp, village, caravan) | Lost | Truces | Chiefs replaced | Dragons | "
         "Bloodiest camp fight |", "|---|---|---|---|---|---|---|"]
    for d in range(5):
        es = [e for e in w.events if d * 10 * YEAR <= e["t"] < (d + 1) * 10 * YEAR]
        fs = [e for e in es if e["act"] in run.FIGHT]
        cf = [e for e in fs if e["tgt"] in w.factions]
        vf = [e for e in fs if e["tgt"] in w.villages]
        af = [e for e in fs if e["tgt"] in ("CARAVAN", "RAIL")]
        top = max(cf, key=lambda e: (e.get("dead", 0) + (e.get("d", 0) or 0), -e["t"]), default=None)
        L.append(f"| {d * 10 + 1}-{d * 10 + 10} | {len(fs)} ({len(cf)}, {len(vf)}, {len(af)}) | "
                 f"{sum(1 for e in fs if not e.get('win'))} | "
                 f"{sum(1 for e in es if e['act'] == 'truce' and e.get('accepted'))} | "
                 f"{sum(1 for e in es if e['act'] == 'succession')} | {sum(1 for e in es if e['act'] == 'dragon')} | "
                 f"{run.fmt(w, top) if top else 'none'} |")
    # who fought whom, the whole run
    pairs = collections.Counter()
    for e in w.events:
        if e["act"] in run.FIGHT and e["tgt"] in w.factions:
            pairs[(w.factions[e["f"]].name, w.factions[e["tgt"]].name)] += 1
    L.append("\nFights between camps over the 50 years, attacker on target: "
             + "; ".join(f"{a} on {b} {n}" for (a, b), n in pairs.most_common()) + ".\n")
    # the second world on the same setup, for contrast
    w2 = worlds[(worlds.index(w) + 1) % len(worlds)]
    p2 = collections.Counter()
    for e in w2.events:
        if e["act"] in run.FIGHT and e["tgt"] in w2.factions:
            p2[(w2.factions[e["f"]].name, w2.factions[e["tgt"]].name)] += 1
    L.append(f"The next seed ({w2.seed}) on the same map: "
             + "; ".join(f"{a} on {b} {n}" for (a, b), n in p2.most_common()) + ".\n")
    L.append(f"The first {years_full} years of seed {w.seed} in full:\n")
    for e in w.events:
        if e["t"] >= years_full * YEAR:
            break
        ln = run.fmt(w, e)
        if ln:
            L.append(f"- {ln}")
    L.append("")
    return L


def main():
    L = ["# Walkthroughs (life model)\n",
         "Written by `walk.py`. Days are game days: a year is 96 days, four seasons of 24.\n"]
    L += walk("fortress", lambda w: {"P:Ben"}, lambda w: fights_on(w, {"P:Ben"}), 8,
              "A strong base that leaves the Umvuthana alone",
              "Ben's walled base (defence 80) sits in reach of the Sun Grove (Umvuthana), Mudhollow (goblins) "
              "and the Grey Outpost (pillagers); its industry makes 3 grievance a day for each; he never "
              "attacks them and never pays.")
    L += walk("farmer", lambda w: {"P:Ben", "F1", "F2"}, lambda w: sum(1 for e in w.events if e["act"] == "killed"),
              8, "Farming the Frostmaw den",
              "Ben (defence 30, pays envoys) kills the Frostmaw at the north den whenever it is back, and steals "
              "the crystal from the sleeping Frostmaw at the south den now and then. The Sun Grove is also in "
              "reach. The headline count is Frostmaw deaths.")
    L += walk("railway", lambda w: {"RAIL", "P:rail"}, lambda w: fights_on(w, {"RAIL"}), 8,
              "A railway through goblin land",
              "A line runs past Ashpit (goblins) and the Pass Den (a Frostmaw), 12 legs a day. The line's owner "
              "(defence 30) never pays; after three robberies in 30 days he puts guards on the trains for a "
              "year.", rail_legs=12.0)
    L += rail_sweep()
    L += walk("multi", lambda w: {"P:Ash", "P:Bea"}, lambda w: fights_on(w, {"P:Bea"}), 8,
              "Multiplayer: one provokes, another suffers",
              "Ash (defence 50) hunts Ashpit's goblins every few weeks and kills its envoys; Bea (defence 8) "
              "lives near Ash, pays what she is asked, and has done nothing. Both are in reach of Ashpit and "
              "the Grey Outpost. A camp that cannot tell who killed its warriors sometimes blames the other "
              "player near it (blame spill, 0.15). The headline count is fights on Bea.")
    L += alone()
    with open(os.path.join(OUT, "walkthroughs.md"), "w") as fh:
        fh.write("\n".join(L) + "\n")


if __name__ == "__main__":
    main()
