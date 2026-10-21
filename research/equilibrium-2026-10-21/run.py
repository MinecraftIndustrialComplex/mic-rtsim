#!/usr/bin/env python3
"""Runs every EQUILIBRIUM.md scenario on the model and writes results/results.json and summary.md.

    nice -n 10 python3 run.py [--quick] [--only NAME[,NAME]] [--jobs 4]
"""
from __future__ import annotations

import argparse
import json
import math
import os
import time
from dataclasses import asdict, replace
from multiprocessing import Pool

import numpy as np

import game
import model
from model import YEAR, Params

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "results")

RATIOS = {   # census key -> capacity key (None: a count already)
    "herd_animals": "herd_k",
    "herds_alive": "herds_founded",
    "dragons": "lairs",
    "village_pop": "village_beds",
    "villages_alive": "villages",
    "ribbit_pop": "ribbit_houses",
    "frostmaws": "dens",
    "umvuthana": "grove_k",
    "worms": "nest_k",
    "camp_pool": "camp_cap",
    "camps_live": "camps",
    "pirate_ships": "havens",     # two slots per haven: divided by 2 below
}
SHOCK_KEY = {"herds": "herd_animals", "herds_local": "herd_local", "dragons": "dragons", "villages": "village_pop",
             "ribbits": "ribbit_pop", "dens": "frostmaws", "groves": "umvuthana", "nests": "worms",
             "camps": "camp_pool", "pirates": "pirate_ships"}


def series(hist, key):
    return np.stack([np.asarray(c[key], float) for c in hist], axis=1)      # (S, T)


def ratio(hist, key):
    cap = RATIOS[key]
    num = series(hist, key)
    den = series(hist, cap) * (2 if key == "pirate_ships" else 1)
    return np.where(den > 0, num / np.maximum(den, 1e-9), np.nan)


def band(x):
    x = x[np.isfinite(x)]
    if x.size == 0:
        return None
    return [round(float(np.percentile(x, q)), 3) for q in (5, 50, 95)]


# --------------------------------------------------------------------------- scenario bodies

def steady(args):
    mode, player, S, years, seed, over = args
    p = replace(Params(mode=mode), **over)
    t0 = time.time()
    w, hist = model.run(p, S=S, days=years * YEAR, seed=seed, player=player)
    ts = np.array([c["t"] for c in hist])
    late = ts >= (years - 30) * YEAR if years > 35 else ts >= years * YEAR / 2
    out = {"mode": mode, "player": player, "years": years, "seconds": round(time.time() - t0, 1),
           "over": over}
    for k in RATIOS:
        r = ratio(hist, k)
        if np.all(~np.isfinite(r)):
            continue
        rl = r[:, late]
        out[k] = {"mean": band(np.nanmean(rl, axis=1)), "min": band(np.nanmin(rl, axis=1)),
                  "max": band(np.nanmax(rl, axis=1))}
    out["herd_extinctions_per_world"] = round(w.extinctions / S, 2)
    out["camp_feud_raids_per_world_year"] = round(w.c_raids / S / years, 2)
    out["herd_top_share"] = band(series(hist, "herd_top_share")[:, late].mean(1))
    if mode == "baseline":
        sim = series(hist, "village_sim_pop")[:, late]
        real = series(hist, "village_pop")[:, late]
        out["village_sim_over_real"] = band((sim / np.maximum(real, 1)).mean(1))
    ev = series(hist, "events")
    out["events_per_day"] = round(float(ev[:, late].mean() / 24), 3)
    out["herd_N_over_K_by_decade"] = [round(float(np.nanmean(ratio(hist, "herd_animals")[:, (ts >= d * YEAR) & (ts < (d + 10) * YEAR)])), 3)
                                      for d in range(0, years, 10)]
    return out


def killall(args):
    mode, kind, S, years, seed, shock_year, over = args
    p = replace(Params(mode=mode), **over)
    w, hist = model.run(p, S=S, days=years * YEAR, seed=seed, player="roamer",
                        shock_kind=kind, shock_day=shock_year * YEAR)
    key = SHOCK_KEY[kind]
    x = series(hist, key)
    ts = np.array([c["t"] for c in hist])
    pre = x[:, (ts >= (shock_year - 1) * YEAR) & (ts < shock_year * YEAR)].mean(1)
    post_idx = np.where(ts > shock_year * YEAR)[0]
    def rec_at(frac):
        out = []
        for s in range(S):
            hit = [i for i in post_idx if x[s, i] >= frac * pre[s]]
            out.append((ts[hit[0]] - shock_year * YEAR) if hit and pre[s] > 0 else (math.inf if pre[s] > 0 else math.nan))
        return np.array(out, float)
    rec = rec_at(0.9)
    r50 = rec_at(0.5)
    fin = rec[np.isfinite(rec)]
    f50 = r50[np.isfinite(r50)]
    end = x[:, -24:].mean(1) / np.maximum(pre, 1e-9)
    return {"mode": mode, "kind": kind, "over": over, "pre_mean": round(float(np.nanmean(pre)), 1),
            "recovered_share": round(float(np.isfinite(rec).sum() / max(1, np.sum(~np.isnan(rec)))), 3),
            "recovery_days": band(fin) if fin.size else None,
            "recovered50_share": round(float(np.isfinite(r50).sum() / max(1, np.sum(~np.isnan(r50)))), 3),
            "recovery50_days": band(f50) if f50.size else None,
            "end_over_pre": band(end)}


def farm(args):
    mode, site, S, years, seed, over = args
    p = replace(Params(mode=mode), **over)
    w, hist = model.run(p, S=S, days=years * YEAR, seed=seed, player="farmer", farm=site)
    y, x = model.CY, model.CX + 2
    if site == "lair":
        kills = w.lair_kills[:, y, x].sum(-1)
        P = w.lair_P.now(years * YEAR)[:, y, x, 0]
    elif site == "den":
        kills, P = w.dens.kills[:, y, x], w.dens.P.now(years * YEAR)[:, y, x]
    elif site == "grove":
        kills, P = w.groves.kills[:, y, x], w.groves.P.now(years * YEAR)[:, y, x]
    else:
        kills, P = w.v_kills[:, y, x], w.v_P.now(years * YEAR)[:, y, x]
    return {"mode": mode, "site": site, "over": over,
            "kills_per_year": band(kills / years), "P_end": band(P)}


def closed_form(base, H, cap, K=1, check=2.0):
    """Steady pressure and kills per year when every arrival at a site of K is killed within `check`
    days (EQUILIBRIUM.md 3.3): each kill adds 1/K, so P* = (1/K) / (1 - 2^(-gap/H)) with
    gap = min(cap, base 2^P*) + check, one kill per gap."""
    f = lambda P: P - (1.0 / K) / (1.0 - 2 ** (-(min(cap, base * 2 ** P) + check) / H))
    lo, hi = 0.0, 64.0                  # f rises with P, so bisect for its one root
    for _ in range(200):
        mid = 0.5 * (lo + hi)
        lo, hi = (mid, hi) if f(mid) < 0 else (lo, mid)
    P = 0.5 * (lo + hi)
    gap = min(cap, base * 2 ** P) + check
    return {"P": round(P, 3), "days_per_kill": round(gap, 1), "per_year": round(YEAR / gap, 2)}


def closed_forms():
    return {
        "lair (base 20, cap 192)": closed_form(20, 96, 192),
        "den (base 24, cap 192)": closed_form(24, 96, 192),
        "den, MOWZIE base 6 with pressure (cap 192)": closed_form(6, 96, 192),
        "den, MOWZIE draft (6 days flat)": {"per_year": round(YEAR / 8, 2)},
        "grove member, K=7 (base 6, cap 96)": closed_form(6, 96, 96, K=7),
        "village settlers, K=12, pairs (base 16, cap 192)": closed_form(16, 96, 192, K=6),
        "den, H=48": closed_form(24, 48, 192),
        "den, H=192": closed_form(24, 192, 192),
    }


def unattended(args):
    mode, S, days, seed = args
    p = Params(mode=mode, dt=4)
    t0 = time.time()
    w, hist = model.run(p, S=S, days=days, seed=seed, player="none", record_every=960)
    ts = np.array([c["t"] for c in hist])
    cent = [(c0, c0 + 100 * YEAR) for c0 in range(0, days, 100 * YEAR)]
    out = {"mode": mode, "days": days, "seconds": round(time.time() - t0, 1), "by_century": []}
    for a, b in cent:
        m = (ts >= a) & (ts < b)
        row = {"from_year": a // YEAR}
        for k in ("herd_animals", "herds_alive", "dragons", "village_pop", "ribbit_pop", "frostmaws",
                  "umvuthana", "worms", "camp_pool", "pirate_ships"):
            r = ratio(hist, k)[:, m]
            row[k] = round(float(np.nanmean(r)), 3) if np.isfinite(r).any() else None
        row["events_per_day"] = round(float(series(hist, "events")[:, m].mean() / 960), 3)
        out["by_century"].append(row)
    return out


def strategy(args):
    mode, kind, sigma, seeds, years = args
    r = game.run(mode, kind, seeds=seeds, days=years * YEAR, sigma=sigma)
    dec = lambda k, a, b: round(float(r[k][:, a:b].mean()), 2)
    return {"mode": mode, "player": kind, "sigma": sigma,
            "war_per_year": [dec("war", d, d + 10) for d in range(0, years, 10)],
            "killed_per_year": [dec("killed", d, d + 10) for d in range(0, years, 10)],
            "tribute_per_year": [dec("tribute", d, d + 10) for d in range(0, years, 10)],
            "loot_lost_per_year": [dec("loot", d, d + 10) for d in range(0, years, 10)],
            "defence": [dec("d", d, d + 10) for d in range(0, years, 10)],
            "player_cost_per_year_last_decade": round(dec("tribute", years - 10, years) + dec("loot", years - 10, years), 1)}


# --------------------------------------------------------------------------- driver

def call(job):
    fn, args = job
    return fn.__name__, args, globals()[fn.__name__](args)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--quick", action="store_true")
    ap.add_argument("--only", default="")
    ap.add_argument("--jobs", type=int, default=4)
    a = ap.parse_args()
    q = a.quick
    S = 6 if q else 24
    S2 = 4 if q else 16
    Y = 20 if q else 60
    only = set(a.only.split(",")) if a.only else None
    jobs = []
    want = lambda n: only is None or n in only
    if want("steady"):
        for mode in ("baseline", "proposed"):
            for player in ("none", "homebody", "roamer"):
                jobs.append((steady, (mode, player, S, Y, 11, {})))
    if want("killall"):
        for mode in ("baseline", "proposed"):
            for kind in SHOCK_KEY:
                jobs.append((killall, (mode, kind, S2, Y, 21, min(20, Y // 3), {})))
    if want("farm"):
        for mode, site, over in (("baseline", "lair", {}), ("proposed", "lair", {}),
                                 ("proposed", "den", {"den_base": 6.0, "pressure": False}),
                                 ("proposed", "den", {}), ("proposed", "grove", {"pressure": False}),
                                 ("proposed", "grove", {}), ("proposed", "village", {}),
                                 ("proposed", "den", {"H": 48.0}), ("proposed", "den", {"H": 192.0})):
            jobs.append((farm, (mode, site, S2, Y, 31, over)))
    if want("knife"):
        for mult in (1, 4, 16, 64):
            for ref in ("on", "off"):
                over = {"dragon_eat": 0.6 * mult}
                if ref == "off":
                    over.update(refuge_frac=0.0, refuge_min=0)
                jobs.append((steady, ("proposed", "roamer", S2, Y, 41, over)))
            jobs.append((steady, ("baseline", "roamer", S2, Y, 41, {"dragon_eat": 0.6 * mult})))
        for fr in (0.0, 0.05, 0.25, 1.0):
            jobs.append((killall, ("proposed", "herds", S2, Y, 51, min(20, Y // 3), {"frontier_rate": fr})))
        for ff in (0.0, 0.2):
            jobs.append((killall, ("proposed", "herds", S2, Y, 51, min(20, Y // 3), {"founders_frac": ff})))
            jobs.append((killall, ("proposed", "herds_local", S2, Y, 52, min(20, Y // 3), {"founders_frac": ff})))
        jobs.append((killall, ("proposed", "herds", S2, Y, 51, min(20, Y // 3), {"loaded_regrow_rate": 2.0})))
        jobs.append((steady, ("proposed", "homebody", S2, Y, 61, {"bh": False})))
        for hd in (1.0, 1.5, 4.0):
            jobs.append((steady, ("proposed", "homebody", S2, Y, 62, {"msy_headroom": hd})))
            jobs.append((steady, ("baseline", "homebody", S2, Y, 62, {"msy_headroom": hd})))
        for sb in (4.0, 16.0, 64.0):
            jobs.append((killall, ("proposed", "villages", S2, Y, 71, min(20, Y // 3), {"settler_base": sb})))
        for sl in (0.2, 0.35, 0.5):
            jobs.append((steady, ("proposed", "roamer", S2, Y, 81, {"dragon_eat": 9.6, "starve_leave": sl})))
    if want("disperse"):
        for dd, dt, df in ((0.8, 0.5, 0.02), (0.8, 0.5, 0.1), (0.4, 0.25, 0.02), (0.4, 0.25, 0.1)):
            over = {"disperse_donor": dd, "disperse_thin": dt, "disperse_frac": df}
            jobs.append((killall, ("proposed", "herds_local", S2, Y, 52, min(20, Y // 3), over)))
    if want("strategy"):
        for sigma in (6.0, 9.0, 12.0):
            for mode in ("ladder", "gate"):
                for kind in ("fortress", "weak", "stubborn", "adaptive"):
                    jobs.append((strategy, (mode, kind, sigma, 16 if q else 64, 50)))
    if want("unattended"):
        for mode in ("baseline", "proposed"):
            jobs.append((unattended, (mode, 4 if q else 8, 2_000 * YEAR if q else 262_980, 91)))
    t0 = time.time()
    results = {}
    with Pool(a.jobs) as pool:
        for name, args, res in pool.imap_unordered(call, jobs):
            results.setdefault(name, []).append(res)
            print(f"[{time.time() - t0:7.0f}s] {name} {args[:3]}", flush=True)
    results["closed_form"] = closed_forms()
    os.makedirs(OUT, exist_ok=True)
    name = ("results-quick" if q else "results") + ("-" + "-".join(sorted(only)) if only else "") + ".json"
    with open(os.path.join(OUT, name), "w") as f:
        json.dump(results, f, indent=1, default=lambda o: None)
    print("wrote", name, "in", round(time.time() - t0), "s")


if __name__ == "__main__":
    main()
