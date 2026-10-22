import os
import sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import model, run
from concurrent.futures import ProcessPoolExecutor
from collections import Counter
from model import YEAR

def one(seed):
    sc = run.scenarios(50 * YEAR)["alone"]
    w = model.World(sc, "life", seed).run()
    ev = w.events
    out = []
    for i, e in enumerate(ev):
        if e["act"] == "pact_end" and e.get("why") == "betrayed":
            fights = [g for g in ev[max(0, i - 6):i] if g["f"] == e["f"] and g["tgt"] == e["tgt"] and g["t"] == e["t"] and "win" in g]
            g = fights[-1] if fights else {}
            out.append((e["f"], e["tgt"], g.get("act"), tuple(g.get("drivers", [])),
                        "had not sworn" in g.get("reason", ""), "broke" in g.get("reason", "")))
    return out

if __name__ == "__main__":
    with ProcessPoolExecutor(6) as ex:
        r = [x for l in ex.map(one, range(1000, 1064)) for x in l]
    print("betrayals", len(r))
    print("pairs", Counter((a, b) for a, b, *_ in r))
    print("acts", Counter(x[2] for x in r))
    print("drivers", Counter(x[3] for x in r).most_common(8))
    print("hunger in drivers", sum(1 for x in r if "hunger" in x[3]))
    print("not sworn", sum(1 for x in r if x[4]), "broke own", sum(1 for x in r if x[5]))
