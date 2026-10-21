#!/usr/bin/env python3
"""Turns results/results.json into results/summary.md, the tables EQUILIBRIUM.md section 3 quotes.

    python3 summarize.py [results/results.json]
"""
from __future__ import annotations

import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))


def b(x, nd=2):
    if x is None:
        return "n/a"
    lo, mid, hi = x
    return f"{mid:.{nd}f} [{lo:.{nd}f}, {hi:.{nd}f}]"


def over_s(o):
    return ", ".join(f"{k}={v:g}" if isinstance(v, (int, float)) and not isinstance(v, bool) else f"{k}={v}"
                     for k, v in o.items()) or "default"


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "results", "results.json")
    d = json.load(open(path))
    out = [f"# Equilibrium model results\n\nSource: `{os.path.relpath(path, HERE)}`. Bands are the 5th, "
           "50th and 95th percentiles over seeds, written `median [p5, p95]`. Ratios are population over "
           "capacity (section 3.1 of EQUILIBRIUM.md); \"late\" is the last 30 years of a 60-year run.\n"]

    keys = ["herd_animals", "herds_alive", "dragons", "village_pop", "villages_alive", "ribbit_pop",
            "frostmaws", "umvuthana", "worms", "camp_pool", "camps_live", "pirate_ships"]

    def steady_table(rows, title):
        out.append(f"\n## {title}\n")
        out.append("| mode | player | over | " + " | ".join(keys) + " | herd min | herd ext./world | events/day |")
        out.append("|" + "---|" * (len(keys) + 6))
        for r in rows:
            cells = [b(r[k]["mean"]) if k in r else "n/a" for k in keys]
            hmin = b(r["herd_animals"]["min"]) if "herd_animals" in r else "n/a"
            out.append(f"| {r['mode']} | {r['player']} | {over_s(r['over'])} | " + " | ".join(cells)
                       + f" | {hmin} | {r['herd_extinctions_per_world']} | {r['events_per_day']} |")

    st = d.get("steady", [])
    steady_table([r for r in st if not r["over"]], "Steady state, 60 years, mean of the late window")
    steady_table([r for r in st if r["over"]], "Knife-edge sweeps (steady)")

    out.append("\n## Herd N/K by decade\n")
    out.append("| mode | player | over | by decade |\n|---|---|---|---|")
    for r in st:
        out.append(f"| {r['mode']} | {r['player']} | {over_s(r['over'])} | {r['herd_N_over_K_by_decade']} |")

    out.append("\n## Kill all of one kind at year 20, then a peaceful player\n")
    out.append("Pre is the mean in the year before the shock. The control is the same seed with no kill (the "
               "player turns peaceful at the same day), so 'vs control' compares like with like while a "
               "kind is still settling from its founding.\n")
    out.append("| mode | kind | over | pre | back to 50% | days to 50% | back to 90% | days to 90% | end/pre | "
               "back to 90% of control | days | end/control |")
    out.append("|---|---|---|---|---|---|---|---|---|---|---|---|")
    for r in sorted(d.get("killall", []), key=lambda r: (r["kind"], r["mode"], str(r["over"]))):
        out.append(f"| {r['mode']} | {r['kind']} | {over_s(r['over'])} | {r['pre_mean']} | "
                   f"{r.get('recovered50_share', float('nan')):.2f} | {b(r.get('recovery50_days'), 0)} | {r['recovered_share']:.2f} | "
                   f"{b(r['recovery_days'], 0)} | {b(r['end_over_pre'])} | "
                   f"{r.get('recovered_ctrl_share', float('nan')):.2f} | {b(r.get('recovery_ctrl_days'), 0)} | "
                   f"{b(r.get('end_over_control'))} |")

    out.append("\n## Farming one site every 2 days for 60 years\n")
    out.append("| mode | site | over | kills per year | pressure at the end |\n|---|---|---|---|---|")
    for r in d.get("farm", []):
        out.append(f"| {r['mode']} | {r['site']} | {over_s(r['over'])} | {b(r['kills_per_year'])} | {b(r['P_end'])} |")
    out.append("\nClosed forms (EQUILIBRIUM.md 3.3): " + "; ".join(
        f"{k}: {v}" for k, v in d.get("closed_form", {}).items()))

    out.append("\n## Unattended server, by century\n")
    for r in d.get("unattended", []):
        out.append(f"\n**{r['mode']}**, {r['days']} days ({r['days'] / 96:.0f} years), {r['seconds']} s\n")
        cols = [k for k in r["by_century"][0] if k != "from_year"]
        out.append("| from year | " + " | ".join(cols) + " |")
        out.append("|" + "---|" * (len(cols) + 1))
        for row in r["by_century"]:
            out.append(f"| {row['from_year']} | " + " | ".join(
                "n/a" if row[c] is None else f"{row[c]:.3f}" for c in cols) + " |")

    out.append("\n## Strategy (b): one faction against one player base, 50 years, 64 seeds\n")
    out.append("| sigma | mode | player | wars/yr (yr 1-10, 41-50) | killed/yr | tribute/yr | loot lost/yr | "
               "defence (yr 1-10, 41-50) | player cost/yr, last decade |")
    out.append("|---|---|---|---|---|---|---|---|---|")
    for r in sorted(d.get("strategy", []), key=lambda r: (r["sigma"], r["mode"], r["player"])):
        f = lambda k: f"{r[k][0]:.1f}, {r[k][-1]:.1f}"
        out.append(f"| {r['sigma']:g} | {r['mode']} | {r['player']} | {f('war_per_year')} | {f('killed_per_year')} | "
                   f"{f('tribute_per_year')} | {f('loot_lost_per_year')} | {f('defence')} | "
                   f"{r['player_cost_per_year_last_decade']} |")

    dst = os.path.join(os.path.dirname(path), "summary.md" if "quick" not in path else "summary-quick.md")
    with open(dst, "w") as fh:
        fh.write("\n".join(out) + "\n")
    print("wrote", dst)


if __name__ == "__main__":
    main()
