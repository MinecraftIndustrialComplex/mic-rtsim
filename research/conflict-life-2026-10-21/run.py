#!/usr/bin/env python3
"""Runs the conflict-life model over many seeded worlds and writes results/results.json,
results/stories.md and results/summary.md.

    nice -n 10 python3 run.py --jobs 4            # everything, about 5 minutes on 4 jobs
    nice -n 10 python3 run.py --quick             # 8 seeds, 20 years

Metrics (CONFLICT-LIFE.md section 3):
  fights per world-year     war parties that fight: raids, invasions, bands, ambushes, plunders,
                            skirmishes
  losing raids              share of those fights the attacker lost, and lost fights per world-year
  extinction                a camp held at one member or none for 96 days running
  runaway                   an endless war: one pair of factions (either way round) fighting with no
                            96-day gap for five years running; or a surge: a world whose last decade
                            has more than twice the fights of its second
  permanent peace           a world with no camp-on-camp fight in the whole run
  peace                     a decade with no camp-on-camp fight, and a decade with no fight at all
  interval CV               spread of the days between one faction's fights (0 is a clock, 1 Poisson)
  pair CV                   the same, between one attacker's fights on one target (the ladder's dwell
                            is a clock here even where its plunder is Poisson)
  to the day                share of a faction's next acts whose kind, target and day (within 1)
                            a watcher could call from its last two acts
  culture spread            the same camp in the same world on different seeds: Jensen-Shannon
                            divergence of its act mix, and the spread of its fights a year
  story overlap             two seeds of the same world: share of the first ten years' acts that
                            match (same faction, act and target within 3 days)
"""
from __future__ import annotations

import argparse
import collections
import json
import math
import os
import sys
import time
from concurrent.futures import ProcessPoolExecutor
from dataclasses import asdict

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import model  # noqa: E402
from model import YEAR, PlayerScript, Scenario  # noqa: E402

FIGHT = ("raid", "invasion", "band", "ambush", "plunder", "skirmish")
ACTS = ("raid", "invasion", "band", "ambush", "plunder", "skirmish", "demand", "warn", "scout", "truce", "hunt")
MODES = ("ladder", "gate", "life")
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "results")


def scenarios(days):
    S = {}
    S["alone"] = Scenario("alone", [], days=days)
    S["fortress"] = Scenario("fortress", [PlayerScript("P:Ben", "fortress", d=80, pays=False, sigma=3,
                                                       reach=("U1", "G2", "R1"))], days=days)
    S["farmer"] = Scenario("farmer", [PlayerScript("P:Ben", "farmer", d=30, pays=True, sigma=1,
                                                   reach=("F1", "F2", "U1"), farm_den="F1", rob_den="F2")],
                           days=days)
    S["railway"] = Scenario("railway", [PlayerScript("P:rail", "railway", d=30, pays=False, sigma=2,
                                                     reach=("G1",))], rail=True, days=days)
    S["multi"] = Scenario("multi", [
        PlayerScript("P:Ash", "hunter", d=50, pays=False, sigma=3, reach=("G1", "R1"), kills_envoys=True,
                     hunts="G1"),
        PlayerScript("P:Bea", "weak", d=8, pays=True, sigma=1, reach=("G1", "R1"))], days=days)
    S["multi_nospill"] = Scenario("multi_nospill", S["multi"].players, blame_spill=0.0, days=days)
    for kind, d, pays in (("fortress", 80, False), ("weak", 6, True), ("stubborn", 6, False), ("adaptive", 12, True)):
        S["duel_" + kind] = Scenario("duel_" + kind, [PlayerScript("P:Ben", kind, d=d, pays=pays, sigma=9,
                                                                   reach=("G1",), trips=False)],
                                     duel=True, dragons=0.0, days=days)
    return S


# ------------------------------------------------------------------------------------------ one world
def world_stats(w):
    sc = w.sc
    years = sc.days // YEAR
    ev = w.events
    fights = [e for e in ev if e["act"] in FIGHT]
    per_year = np.zeros(years)
    cc = np.zeros(years)
    lost_year = np.zeros(years)
    cat = collections.Counter()
    cat_lost = collections.Counter()
    for e in fights:
        y = min(years - 1, e["t"] // YEAR)
        per_year[y] += 1
        if not e.get("win"):
            lost_year[y] += 1
        x = e["tgt"]
        if x in w.factions:
            cc[y] += 1
            k = "camp"
        elif x in w.villages:
            k = "village"
        elif x in w.players:
            k = "player"
        else:
            k = "train_or_caravan"
        cat[k] += 1
        if not e.get("win"):
            cat_lost[k] += 1
    # endless war: one pair of factions, either way round, with no 96-day gap for five years running
    endless = False
    by_pair = collections.defaultdict(list)
    by_dir = collections.defaultdict(list)
    for e in fights:
        by_dir[(e["f"], e["tgt"])].append(e["t"])
        if e["tgt"] in w.factions:
            by_pair[tuple(sorted((e["f"], e["tgt"])))].append(e["t"])
    for ts in by_pair.values():
        ts = sorted(ts)
        start = ts[0]
        for a, b in zip(ts, ts[1:]):
            if b - a >= YEAR:
                start = b
            elif b - start >= 5 * YEAR:
                endless = True
                break
    pair_cvs = []
    for ts in by_dir.values():
        if len(ts) >= 5:
            gaps = np.diff(sorted(ts)).astype(float)
            if gaps.mean() > 0:
                pair_cvs.append(float(gaps.std() / gaps.mean()))
    dec = per_year[: (years // 10) * 10].reshape(-1, 10).sum(1) if years >= 20 else per_year
    ccdec = cc[: (years // 10) * 10].reshape(-1, 10).sum(1) if years >= 20 else cc
    surge = bool(len(dec) >= 3 and dec[-1] > 2 * max(dec[1], 1))
    extinct = any(d >= 96 for d in w.extinct_days.values())
    # timing and predictability per faction
    cvs, hits, total = [], 0, 0
    acts_by_f = collections.defaultdict(list)
    for e in ev:
        if e["act"] in ACTS:
            acts_by_f[e["f"]].append(e)
    for f, es in acts_by_f.items():
        ts = [e["t"] for e in es if e["act"] in FIGHT]
        if len(ts) >= 5:
            gaps = np.diff(ts).astype(float)
            if gaps.mean() > 0:
                cvs.append(float(gaps.std() / gaps.mean()))
        # a watcher who knows the faction's habit: the next act repeats the last kind and target
        # after the last gap. ladder and gate repeat; a mind does not.
        for i in range(2, len(es)):
            a, b, c = es[i - 2], es[i - 1], es[i]
            guess_t = b["t"] + (b["t"] - a["t"])
            total += 1
            if c["act"] == b["act"] and c["tgt"] == b["tgt"] and abs(c["t"] - guess_t) <= 1:
                hits += 1
    mix = collections.Counter(e["act"] for e in ev if e["act"] in ACTS)
    mix_by_f = {f: [sum(1 for e in es if e["act"] == a) for a in ACTS] for f, es in acts_by_f.items()}
    fights_by_f = {f: sum(1 for e in es if e["act"] in FIGHT) / years for f, es in acts_by_f.items()}
    motives = collections.Counter(e.get("motive", "rule") for e in fights)
    drivers = collections.Counter(d for e in fights for d in e.get("drivers", ["rule"]))
    loot_only = sum(1 for e in fights if e.get("drivers") == ["loot"])
    occasions = collections.Counter(e.get("occasion", "clock") for e in fights)
    out = {
        "fights_per_year": per_year.tolist(),
        "camp_fights_per_year": cc.tolist(),
        "lost_per_year": lost_year.tolist(),
        "fights": len(fights),
        "lost": int(sum(1 for e in fights if not e.get("win"))),
        "cat": dict(cat),
        "cat_lost": dict(cat_lost),
        "endless": endless,
        "surge": surge,
        "extinct": extinct,
        "peace_decade_camp": bool(len(ccdec) and (ccdec == 0).any()),
        "peace_decade_all": bool(len(dec) and (dec == 0).any()),
        "peace_forever": bool(cc.sum() == 0),
        "pair_cv": pair_cvs,
        "drivers": dict(drivers),
        "loot_only": loot_only,
        "decades": dec.tolist(),
        "cv": cvs,
        "to_day_hits": hits,
        "to_day_total": total,
        "mix": dict(mix),
        "mix_by_f": mix_by_f,
        "fights_by_f": fights_by_f,
        "motives": dict(motives),
        "occasions": dict(occasions),
        "story": [(e["t"], e["f"], e["act"], e["tgt"]) for e in ev if e["act"] in ACTS and e["t"] < 10 * YEAR],
        "by_player": {p: sum(1 for e in fights if e["tgt"] == p) / years for p in w.players},
        "by_player_lost": {p: sum(e.get("dead", 0) for e in ev if e["tgt"] == p and e["act"] in FIGHT) / years
                           for p in w.players},
        "tribute_by_player": {p: sum(e.get("T", 0) for e in ev if e["tgt"] == p and e["act"] == "demand"
                                     and e.get("paid")) / years for p in w.players},
        "sittings_per_faction_year": getattr(w, "sittings", 0) / years / max(1, len(w.factions)),
        "matters_per_sitting": getattr(w, "matters", 0) / max(1, getattr(w, "sittings", 0)),
        "acts_per_faction_year": len([e for e in ev if e["act"] in ACTS]) / years / max(1, len(w.factions)),
    }
    if sc.duel:
        p = w.players["P:Ben"]
        war = [e for e in fights if e["tgt"] == "P:Ben"]
        wy = np.zeros(years)
        killed = np.zeros(years)
        for e in war:
            y = min(years - 1, e["t"] // YEAR)
            wy[y] += 1
            killed[y] += e.get("dead", 0)
        trib = np.zeros(years)
        for e in ev:
            if e["act"] == "demand" and e.get("paid"):
                trib[min(years - 1, e["t"] // YEAR)] += e["T"]
        out["duel"] = {"war": wy.tolist(), "killed": killed.tolist(), "tribute": trib.tolist(), "d_end": p.d}
        out["duel"]["d_by_year"] = getattr(w, "d_by_year", [])
    return out


def one(args):
    name, mode, seed, days = args
    sc = scenarios(days)[name]
    w = model.World(sc, mode, seed)
    if sc.duel:
        d_by_year = []
        for t in range(sc.days):
            w.t = t
            w.world_day()
            for f in w.factions.values():
                w.day_faction(f)
            for f in w.factions.values():
                {"ladder": w.ladder_day, "gate": w.gate_day, "life": w.life_day}[mode](f)
            if t % YEAR == YEAR - 1:
                d_by_year.append(w.players["P:Ben"].d)
        w.d_by_year = d_by_year
    else:
        w.run()
    st = world_stats(w)
    st.update(name=name, mode=mode, seed=seed)
    return st


KEEP = set(FIGHT) | {"demand", "succession", "dragon", "killed", "robbed", "fight", "truce", "arrive", "hunted",
                     "ground_lost", "hunt", "warn", "clash", "guards"}


def stories(name, mode, seed, days, limit=40):
    sc = scenarios(days)[name]
    w = model.World(sc, mode, seed).run()
    lines = []
    for e in w.events:
        txt = fmt(w, e)
        if txt is None:
            continue
        lines.append(txt)
        if len(lines) >= limit:
            break
    return lines


def fmt(w, e):
    """One event as a line of story, or None for the kinds a story leaves out."""
    seasons = ("spring", "summer", "autumn", "winter")
    if e["act"] not in KEEP:
        return None
    y, d = divmod(int(e["t"]), YEAR)
    f = w.factions[e["f"]]
    head = f"Y{y + 1} d{d + 1} ({seasons[d // 24]}) {f.name}"
    x = w.tname(e["tgt"]) if e["tgt"] else ""
    a = e["act"]
    if a in FIGHT:
        res = "won" if e.get("win") else "lost"
        txt = f"{head}: {a} on {x} by {e.get('size', 0):.0f} ({e.get('speaker', '')}); {res}, {e.get('dead', 0):.0f} dead"
        if e.get("loot"):
            txt += f", {e['loot']:.0f} taken"
        txt += f". Why: {e['reason']}."
    elif a == "demand":
        ans = "paid" if e.get("paid") else ("killed the envoy" if e.get("envoy_killed") else "refused")
        txt = f"{head}: envoy to {x} asking {e['T']:.0f} ({e.get('speaker', '')}); {ans}. Why: {e['reason']}."
    elif a == "truce":
        txt = f"{head}: {e['reason']}; {'accepted' if e.get('accepted') else 'rejected'}."
    else:
        txt = f"{head}: {e['reason']}."
    return txt


# ------------------------------------------------------------------------------------------ summaries
def js(p, q):
    p = np.asarray(p, float)
    q = np.asarray(q, float)
    if p.sum() == 0 or q.sum() == 0:
        return float("nan")
    p /= p.sum()
    q /= q.sum()
    m = 0.5 * (p + q)

    def kl(a, b):
        nz = a > 0
        return float((a[nz] * np.log2(a[nz] / b[nz])).sum())
    return 0.5 * kl(p, m) + 0.5 * kl(q, m)


def overlap(a, b, tol=3):
    """Share of a's acts matched by b's: same faction, act and target within tol days."""
    if not a and not b:
        return 1.0
    idx = collections.defaultdict(list)
    for t, f, act, x in b:
        idx[(f, act, x)].append(t)
    used = set()
    m = 0
    for t, f, act, x in a:
        for j, tb in enumerate(idx[(f, act, x)]):
            if (f, act, x, j) not in used and abs(tb - t) <= tol:
                used.add((f, act, x, j))
                m += 1
                break
    return 2 * m / (len(a) + len(b))


def pct(a, q):
    return float(np.percentile(a, q)) if len(a) else float("nan")


def summarize(rows):
    by = collections.defaultdict(list)
    for r in rows:
        by[(r["name"], r["mode"])].append(r)
    out = {}
    for (name, mode), rs in sorted(by.items()):
        fy = np.concatenate([r["fights_per_year"] for r in rs])
        cy = np.concatenate([r["camp_fights_per_year"] for r in rs])
        ly = np.concatenate([r["lost_per_year"] for r in rs])
        fights = sum(r["fights"] for r in rs)
        lost = sum(r["lost"] for r in rs)
        cvs = [c for r in rs for c in r["cv"]]
        pcvs = [c for r in rs for c in r["pair_cv"]]
        hits = sum(r["to_day_hits"] for r in rs)
        tot = sum(r["to_day_total"] for r in rs)
        # culture spread: the same faction slot across seeds
        jsd, cvf = [], []
        fids = set().union(*[r["mix_by_f"].keys() for r in rs])
        for f in sorted(fids):
            mixes = [r["mix_by_f"].get(f) for r in rs if r["mix_by_f"].get(f) and sum(r["mix_by_f"][f]) >= 5]
            if len(mixes) >= 4:
                ds = [js(mixes[i], mixes[j]) for i in range(len(mixes)) for j in range(i + 1, min(len(mixes), i + 9))]
                ds = [d for d in ds if not math.isnan(d)]
                if ds:
                    jsd.append(float(np.mean(ds)))
            fr = [r["fights_by_f"].get(f, 0.0) for r in rs]
            if np.mean(fr) > 0.2:
                cvf.append(float(np.std(fr) / np.mean(fr)))
        ov = [overlap(rs[i]["story"], rs[i + 1]["story"]) for i in range(len(rs) - 1)]
        cat = collections.Counter()
        cat_lost = collections.Counter()
        mot = collections.Counter()
        occ = collections.Counter()
        drv = collections.Counter()
        for r in rs:
            cat.update(r["cat"])
            cat_lost.update(r.get("cat_lost", {}))
            mot.update(r["motives"])
            occ.update(r["occasions"])
            drv.update(r["drivers"])
        years = len(fy) / len(rs)
        row = {
            "worlds": len(rs),
            "years": years,
            "fights_per_year": {"mean": float(fy.mean()), "p5": pct(fy, 5), "median": pct(fy, 50), "p95": pct(fy, 95),
                                "zero_years": float((fy == 0).mean())},
            "camp_fights_per_year": {"mean": float(cy.mean()), "p95": pct(cy, 95), "zero_years": float((cy == 0).mean())},
            "lost_share": lost / fights if fights else float("nan"),
            "lost_per_year": float(ly.mean()),
            "by_target": {k: v / (len(rs) * years) for k, v in cat.items()},
            "lost_share_by_target": {k: cat_lost[k] / v for k, v in cat.items() if v},
            "extinct": float(np.mean([r["extinct"] for r in rs])),
            "endless": float(np.mean([r["endless"] for r in rs])),
            "surge": float(np.mean([r["surge"] for r in rs])),
            "peace_decade_camp": float(np.mean([r["peace_decade_camp"] for r in rs])),
            "peace_decade_all": float(np.mean([r["peace_decade_all"] for r in rs])),
            "peace_forever": float(np.mean([r["peace_forever"] for r in rs])),
            "pair_cv": {"median": pct(pcvs, 50), "p10": pct(pcvs, 10), "p90": pct(pcvs, 90), "n": len(pcvs)},
            "drivers": {k: v / max(1, fights) for k, v in drv.most_common()},
            "loot_only": sum(r["loot_only"] for r in rs) / max(1, fights),
            "by_player": {p: float(np.mean([r["by_player"][p] for r in rs])) for p in rs[0]["by_player"]},
            "by_player_lost": {p: float(np.mean([r["by_player_lost"][p] for r in rs])) for p in rs[0]["by_player"]},
            "tribute_by_player": {p: float(np.mean([r["tribute_by_player"][p] for r in rs]))
                                  for p in rs[0]["by_player"]},
            "sittings_per_faction_year": float(np.mean([r["sittings_per_faction_year"] for r in rs])),
            "matters_per_sitting": float(np.mean([r["matters_per_sitting"] for r in rs])),
            "acts_per_faction_year": float(np.mean([r["acts_per_faction_year"] for r in rs])),
            "decades_mean": np.mean([r["decades"] for r in rs], axis=0).tolist(),
            "interval_cv": {"median": pct(cvs, 50), "p10": pct(cvs, 10), "p90": pct(cvs, 90), "n": len(cvs)},
            "to_the_day": hits / tot if tot else float("nan"),
            "culture_js": float(np.mean(jsd)) if jsd else float("nan"),
            "culture_cv_fights": float(np.mean(cvf)) if cvf else float("nan"),
            "story_overlap": float(np.mean(ov)) if ov else float("nan"),
            "motives": {k: v / max(1, fights) for k, v in mot.most_common()},
            "occasions": {k: v / max(1, fights) for k, v in occ.most_common()},
        }
        if rs[0].get("duel"):
            war = np.array([r["duel"]["war"] for r in rs])
            kil = np.array([r["duel"]["killed"] for r in rs])
            tri = np.array([r["duel"]["tribute"] for r in rs])
            dd = np.array([r["duel"]["d_by_year"] for r in rs])
            last = slice(-10, None)
            row["duel"] = {"war_first5": float(war[:, :5].mean()), "war_last10": float(war[:, last].mean()),
                           "killed_last10": float(kil[:, last].mean()), "tribute_last10": float(tri[:, last].mean()),
                           "d_year10": float(dd[:, 9].mean()) if dd.shape[1] > 9 else float("nan"),
                           "d_last": float(dd[:, -1].mean())}
        out[f"{name}/{mode}"] = row
    return out


def long_run(args):
    seed, years = args
    sc = Scenario("alone_long", [], days=years * YEAR)
    w = model.World(sc, "life", seed).run()
    per = np.zeros(years)
    cc = np.zeros(years)
    for e in w.events:
        if e["act"] in FIGHT:
            per[e["t"] // YEAR] += 1
            if e["tgt"] in w.factions:
                cc[e["t"] // YEAR] += 1
    cent = per.reshape(-1, 100).mean(1).tolist()
    ccent = cc.reshape(-1, 100).mean(1).tolist()
    return {"seed": seed, "fights_by_century": cent, "camp_by_century": ccent,
            "pools_end": {k: f.pool for k, f in w.factions.items()},
            "extinct_days": dict(w.extinct_days)}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--quick", action="store_true")
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--seeds", type=int, default=0)
    ap.add_argument("--only", default="")
    a = ap.parse_args()
    seeds = a.seeds or (8 if a.quick else 64)
    years = 20 if a.quick else 50
    days = years * YEAR
    names = list(scenarios(days))
    if a.only:
        names = [n for n in names if n in a.only.split(",")]
    jobs = [(n, m, 1000 + s, days) for n in names for m in MODES for s in range(seeds)]
    t0 = time.time()
    with ProcessPoolExecutor(a.jobs) as ex:
        rows = list(ex.map(one, jobs, chunksize=4))
        longs = list(ex.map(long_run, [(7000 + s, 200 if a.quick else 500) for s in range(4 if a.quick else 8)]))
    summ = summarize(rows)
    os.makedirs(OUT, exist_ok=True)
    story = {}
    for n in ("alone", "fortress", "farmer", "railway", "multi"):
        if n in names:
            story[n] = {f"seed {s}": stories(n, "life", s, days) for s in (1, 2)}
            story[n]["ladder seed 1"] = stories(n, "ladder", 1, days, limit=15)
    res = {"seeds": seeds, "years": years, "seconds": round(time.time() - t0, 1), "summary": summ,
           "long": longs, "life_params": model.LIFE_DEFAULTS,
           "cultures": {k: {kk: vv for kk, vv in asdict(c).items() if kk != "party"} for k, c in model.CULTURES.items()}}
    with open(os.path.join(OUT, "results.json"), "w") as fh:
        json.dump(res, fh, indent=1)
    with open(os.path.join(OUT, "stories.md"), "w") as fh:
        fh.write("# Story samples (life mode unless marked)\n\n")
        for n, d in story.items():
            fh.write(f"## {n}\n\n")
            for k, lines in d.items():
                fh.write(f"### {k}\n\n")
                for ln in lines:
                    fh.write(f"- {ln}\n")
                fh.write("\n")
    import summarize_md
    summarize_md.write(res, os.path.join(OUT, "summary.md"))
    print("done in", round(time.time() - t0, 1), "s")


if __name__ == "__main__":
    main()
