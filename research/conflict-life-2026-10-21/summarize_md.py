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
             "Spread of a camp's fights a year (CV over seeds) | Story overlap of two seeds (chance) | "
             "Excess dispersion of a camp's years median [p10, p90] |")
    L.append("|---|---|---|---|---|---|---|---|---|")
    for n in names:
        for m in ("ladder", "gate", "life"):
            r = S.get(f"{n}/{m}")
            if not r:
                continue
            c = r["interval_cv"]
            pc = r["pair_cv"]
            L.append(f"| {n} | {m} | {f(c['median'])} [{f(c['p10'])}, {f(c['p90'])}] | "
                     f"{f(pc['median'])} [{f(pc['p10'])}, {f(pc['p90'])}] | {f(r['to_the_day'])} | "
                     f"{f(r['culture_js'], 3)} | {f(r['culture_cv_fights'])} | {f(r['story_overlap'])} "
                     f"({f(r['story_overlap_chance'])}) | {f(r['dispersion']['median'])} "
                     f"[{f(r['dispersion']['p10'])}, {f(r['dispersion']['p90'])}] |")
    L.append("\n## Refusals and guards: can a watcher call the day?\n")
    L.append("Refusal lag: days from a refused envoy to that camp's next fight on the target (within a year). "
             "Commonest lag: the share of lags within a day of the commonest one. Guards lag: days from the "
             "railway taking its guards off to the next train robbed.\n")
    L.append("| Scenario | Mode | Refusals a year | Followed by a fight | Lag median [p10, p90] | Commonest lag "
             "(share within 1) | Fight within a day | Guards lag median [p10, p90] (n, CV) |")
    L.append("|---|---|---|---|---|---|---|---|")
    for n in names:
        for m in ("ladder", "gate", "life"):
            r = S.get(f"{n}/{m}")
            if not r or (r["refusal"]["refusals_per_year"] == 0 and r["guards_lag"]["n"] == 0):
                continue
            q = r["refusal"]
            g = r["guards_lag"]
            L.append(f"| {n} | {m} | {f(q['refusals_per_year'])} | {f(q['fought_within_year'])} | "
                     f"{f(q['lag_median'], 0)} [{f(q['lag_p10'], 0)}, {f(q['lag_p90'], 0)}] | "
                     f"{f(q['mode'], 0)} ({f(q['mode_share'])}) | {f(q['within_1_day'])} | "
                     f"{f(g['median'], 0)} [{f(g['p10'], 0)}, {f(g['p90'], 0)}] ({g['n']}, {f(g['cv'])}) |")
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
    L.append("| Scenario | Mode | Player | Fights on him | Attackers killed there | Loot taken | Tribute paid | "
             "Loot and tribute as a share of his income |")
    L.append("|---|---|---|---|---|---|---|---|")
    for n in names:
        for m in ("ladder", "gate", "life"):
            r = S.get(f"{n}/{m}")
            if not r or n.startswith("duel_"):
                continue
            for p, v in r["by_player"].items():
                L.append(f"| {n} | {m} | {p} | {f(v)} | {f(r['by_player_lost'][p], 1)} | "
                         f"{f(r['loot_by_player'][p], 0)} | {f(r['tribute_by_player'][p], 0)} | "
                         f"{f(r['income_share_by_player'][p])} |")
    L.append("\n## Alliances (life; section 2.16)\n")
    L.append("Each `*_noally` scenario is the same seeds with alliances off. Calls: the shares a sworn ally "
             "joined, refused or could not send (busy). Joint fights lost: the share the attackers lost. Ends: "
             "a year, by why the pact ended.\n")
    L.append("| Scenario | Offers a year | Sworn a year | Worlds with a pact | Pact days median (mean) | Calls a year "
             "(joined, refused, busy) | Joint fights a year (lost) | On players (lost) | On camps | Joint demands "
             "a year | Betrayals a year | Ends a year | Pairs a year |")
    L.append("|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    for n in names:
        r = S.get(f"{n}/life")
        if not r or "ally" not in r or n.startswith("duel_") or n.endswith("_noally"):
            continue
        a = r["ally"]
        ends = ", ".join(f"{k} {v:.3f}" for k, v in a["ends"].items())
        pairs = ", ".join(f"{k} {v:.3f}" for k, v in list(a["pairs"].items())[:3])
        L.append(f"| {n} | {f(a['offers_per_year'], 3)} | {f(a['sworn_per_year'], 3)} | {f(a['worlds_with_pact'])} | "
                 f"{f(a['pact_days_median'], 0)} ({f(a['pact_days_mean'], 0)}) | {f(a['calls_per_year'], 3)} "
                 f"({f(a['joined'])}, {f(a['refused'])}, {f(a['busy'])}) | {f(a['joint_per_year'], 3)} "
                 f"({f(a['joint_lost'])}) | {f(a['joint_player_per_year'], 3)} ({f(a['joint_player_lost'])}) | "
                 f"{f(a['joint_camp_per_year'], 3)} | {f(a['joint_demands_per_year'], 3)} | "
                 f"{f(a['betrayals_per_year'], 3)} | {ends} | {pairs} |")
    L.append("\n## Work (life): what the engine does a faction-year\n")
    L.append("| Scenario | Council sittings a faction-year | Sittings a year by culture | Relations weighed a "
             "sitting | Acts a faction-year | Rows touched a faction-day |")
    L.append("|---|---|---|---|---|---|")
    for n in names:
        r = S.get(f"{n}/life")
        if r:
            sc = ", ".join(f"{k} {v:.1f}" for k, v in sorted(r["sittings_by_culture"].items()))
            L.append(f"| {n} | {f(r['sittings_per_faction_year'], 1)} | {sc} | {f(r['matters_per_sitting'], 1)} | "
                     f"{f(r['acts_per_faction_year'], 1)} | {f(r['touches_per_faction_day'], 3)} |")
    L.append("\n## Unattended for centuries (life, alone)\n")
    L.append("| Seed | Fights a year by century | Camp-on-camp a year by century |")
    L.append("|---|---|---|")
    for r in res["long"]:
        L.append(f"| {r['seed']} | {', '.join(f(x, 1) for x in r['fights_by_century'])} | "
                 f"{', '.join(f(x, 2) for x in r['camp_by_century'])} |")
    with open(path, "w") as fh:
        fh.write("\n".join(L) + "\n")
