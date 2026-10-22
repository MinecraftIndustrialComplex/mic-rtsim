import os
import sys, collections
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import model, run
from concurrent.futures import ProcessPoolExecutor
from model import YEAR, Scenario
FIGHT = run.FIGHT

def fortress(seed):
    sc = run.scenarios(50 * YEAR)["fortress"]
    w = model.World(sc, "life", seed).run()
    f2 = [e for e in w.events if e["act"] in FIGHT and e["tgt"] == "P:Ben"]
    return any(e["t"] < 2 * YEAR for e in f2), min([e["t"] for e in f2], default=None)

def alone(seed):
    sc = run.scenarios(50 * YEAR)["alone"]
    w = model.World(sc, "life", seed).run()
    camp = sorted(e["t"] for e in w.events if e["act"] in FIGHT and e["tgt"] in w.factions)
    gaps = [b - a for a, b in zip([0] + camp, camp + [50 * YEAR])]
    truces = [e for e in w.events if e["act"] == "truce" and e.get("accepted")]
    # broken by the swearer: fights whose reason says "broke his own truce" on a camp
    broke_own = sum(1 for e in w.events if e["act"] in FIGHT and e["tgt"] in w.factions and "broke his own truce" in e["reason"])
    broke_other = sum(1 for e in w.events if e["act"] in FIGHT and e["tgt"] in w.factions and "had not sworn" in e["reason"])
    return max(gaps), len(truces), broke_own, broke_other

def adaptive(seed):
    sc = run.scenarios(50 * YEAR)["duel_adaptive"]
    w = model.World(sc, "life", seed).run()
    loot = sum(e.get("loot", 0) for e in w.events if e["act"] in FIGHT and e.get("win"))
    trib = sum(e.get("T", 0) for e in w.events if e["act"] == "demand" and e.get("paid"))
    return loot / 50, trib / 50

def rail12(args):
    mode, seed = args
    sc = run.scenarios(20 * YEAR)["railway"]
    sc.rail_legs = 12.0
    w = model.World(sc, mode, seed).run()
    fm = [e["t"] for e in w.events if e["act"] in FIGHT and e["f"] == "F2"]
    return mode, len(fm) / 20, (min(fm) if fm else None), sorted(fm)[:6]

if __name__ == "__main__":
    with ProcessPoolExecutor(4) as ex:
        fr = list(ex.map(fortress, range(1000, 1064)))
        print("fortress: share of seeds with a fight in years 1-2", sum(a for a, _ in fr) / 64,
              "first fight median", sorted(t for _, t in fr if t is not None)[len(fr)//2])
        al = list(ex.map(alone, range(1000, 1064)))
        print("alone: longest gap between camp fights (days) max", max(a[0] for a in al), "share > 20y", sum(a[0] > 20 * YEAR for a in al) / 64)
        tr = sum(a[1] for a in al); bo = sum(a[2] for a in al); bx = sum(a[3] for a in al)
        print("alone: truces accepted", tr, "camp fights by the swearer in truce", bo, "by a later chief", bx)
        ad = list(ex.map(adaptive, range(1000, 1064)))
        print("adaptive loot a year", sum(a for a, _ in ad) / 64, "tribute", sum(b for _, b in ad) / 64)
        for r in ex.map(rail12, [(m, s) for m in ("ladder", "life") for s in range(1000, 1008)]):
            print(r)
