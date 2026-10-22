"""Writes results/summary.md from the results dict run.py builds."""
from __future__ import annotations

import math


def f(x, d=2):
    if x is None or (isinstance(x, float) and math.isnan(x)):
        return "n/a"
    return f"{x:.{d}f}"


def write(res, path):
    S = res["summary"]
    L = []
    L.append("# Conflict-life model: results\n")
    L.append(f"{res['seeds']} seeds per scenario and mode, {res['years']} game years each; run took "
             f"{res['seconds']} s. `ladder` is today's engine (EQ-5 (c)), `gate` is EQUILIBRIUM 2.5 (EQ-5 (a)), "
             "`life` is CONFLICT-LIFE.md.\n")
    names = sorted({k.split("/")[0] for k in S})
    L.append("## Fights per world-year, losses, stability\n")
    L.append("| Scenario | Mode | Fights a year: mean [p5, median, p95] | Years with none | Camp-on-camp a year | "
             "Lost share | Lost a year | Endless war | Surge | Extinct | Decade with no camp fight | "
             "No camp fight at all |")
    L.append("|---|---|---|---|---|---|---|---|---|---|---|---|")
    for n in names:
        for m in ("ladder", "gate", "life"):
            r = S.get(f"{n}/{m}")
            if not r:
                continue
            fy = r["fights_per_year"]
            L.append(f"| {n} | {m} | {f(fy['mean'], 1)} [{f(fy['p5'], 0)}, {f(fy['median'], 0)}, {f(fy['p95'], 0)}] | "
                     f"{f(fy['zero_years'])} | {f(r['camp_fights_per_year']['mean'])} | {f(r['lost_share'])} | "
                     f"{f(r['lost_per_year'])} | {f(r['endless'])} | {f(r['surge'])} | {f(r['extinct'])} | "
                     f"{f(r['peace_decade_camp'])} | {f(r['peace_forever'])} |")
    L.append("\n## How formulaic\n")
    L.append("| Scenario | Mode | Interval CV median [p10, p90] | Pair CV median [p10, p90] | "
             "Next act called to the day | Culture spread (JS) | "
             "Spread of a camp's fights a year (CV over seeds) | Story overlap of two seeds |")
    L.append("|---|---|---|---|---|---|---|---|")
    for n in names:
        for m in ("ladder", "gate", "life"):
            r = S.get(f"{n}/{m}")
            if not r:
                continue
            c = r["interval_cv"]
            pc = r["pair_cv"]
            L.append(f"| {n} | {m} | {f(c['median'])} [{f(c['p10'])}, {f(c['p90'])}] | "
                     f"{f(pc['median'])} [{f(pc['p10'])}, {f(pc['p90'])}] | {f(r['to_the_day'])} | "
                     f"{f(r['culture_js'], 3)} | {f(r['culture_cv_fights'])} | {f(r['story_overlap'])} |")
    L.append("\n## Duels (game.py's setting: one goblin camp, one base, grievance 9 a day)\n")
    L.append("| Player | Mode | Wars a year, years 1-5 | Wars a year, last decade | Raiders killed a year | "
             "Tribute a year | Defence year 10 | Defence at the end |")
    L.append("|---|---|---|---|---|---|---|---|")
    for n in names:
        if not n.startswith("duel_"):
            continue
        for m in ("ladder", "gate", "life"):
            r = S.get(f"{n}/{m}")
            if not r or "duel" not in r:
                continue
            d = r["duel"]
            L.append(f"| {n[5:]} | {m} | {f(d['war_first5'], 1)} | {f(d['war_last10'], 1)} | {f(d['killed_last10'], 0)} | "
                     f"{f(d['tribute_last10'], 0)} | {f(d['d_year10'], 0)} | {f(d['d_last'], 0)} |")
    L.append("\n## What started the life-mode fights (share of fights)\n")
    L.append("A driver is any motive that was at least a quarter of the push toward the fight; a fight can have "
             "several. Loot only: greed for loot with no hunger, grudge, pride, glory or fear behind it.\n")
    L.append("| Scenario | Top motive | Drivers | Loot only | Occasion |")
    L.append("|---|---|---|---|---|")
    for n in names:
        r = S.get(f"{n}/life")
        if not r:
            continue
        mo = ", ".join(f"{k} {v:.2f}" for k, v in r["motives"].items())
        oc = ", ".join(f"{k} {v:.2f}" for k, v in r["occasions"].items())
        dv = ", ".join(f"{k} {v:.2f}" for k, v in r["drivers"].items())
        L.append(f"| {n} | {mo} | {dv} | {f(r['loot_only'])} | {oc} |")
    L.append("\n## Fights by target (a world-year; in brackets, the share the attacker lost)\n")
    L.append("| Scenario | Mode | Camps | Villages | Players | Trains and caravans |")
    L.append("|---|---|---|---|---|---|")
    for n in names:
        for m in ("ladder", "gate", "life"):
            r = S.get(f"{n}/{m}")
            if not r:
                continue
            b = r["by_target"]
            ls = r.get("lost_share_by_target", {})

            def cell(k):
                return f"{f(b.get(k, 0))} ({f(ls[k])})" if k in ls else f(b.get(k, 0))
            L.append(f"| {n} | {m} | {cell('camp')} | {cell('village')} | {cell('player')} | "
                     f"{cell('train_or_caravan')} |")
    L.append("\n## Each player's share (a year)\n")
    L.append("| Scenario | Mode | Player | Fights on him | Attackers killed there | Tribute paid |")
    L.append("|---|---|---|---|---|---|")
    for n in names:
        for m in ("ladder", "gate", "life"):
            r = S.get(f"{n}/{m}")
            if not r or n.startswith("duel_"):
                continue
            for p, v in r["by_player"].items():
                L.append(f"| {n} | {m} | {p} | {f(v)} | {f(r['by_player_lost'][p], 1)} | "
                         f"{f(r['tribute_by_player'][p], 0)} |")
    L.append("\n## Work (life): what the engine does a faction-year\n")
    L.append("| Scenario | Council sittings a faction-year | Relations weighed a sitting | Acts a faction-year |")
    L.append("|---|---|---|---|")
    for n in names:
        r = S.get(f"{n}/life")
        if r:
            L.append(f"| {n} | {f(r['sittings_per_faction_year'], 1)} | {f(r['matters_per_sitting'], 1)} | "
                     f"{f(r['acts_per_faction_year'], 1)} |")
    L.append("\n## Unattended for centuries (life, alone)\n")
    L.append("| Seed | Fights a year by century | Camp-on-camp a year by century |")
    L.append("|---|---|---|")
    for r in res["long"]:
        L.append(f"| {r['seed']} | {', '.join(f(x, 1) for x in r['fights_by_century'])} | "
                 f"{', '.join(f(x, 2) for x in r['camp_by_century'])} |")
    with open(path, "w") as fh:
        fh.write("\n".join(L) + "\n")
