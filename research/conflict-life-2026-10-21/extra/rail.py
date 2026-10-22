import os
import sys, collections
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import model, run
from concurrent.futures import ProcessPoolExecutor
YEAR = 96
def job(seed):
    sc = run.scenarios(50 * YEAR)["railway"]
    w = model.World(sc, "life", seed).run()
    c = collections.Counter()
    for e in w.events:
        if e["act"] in run.FIGHT and e["tgt"] in ("RAIL", "CARAVAN"):
            cu = w.factions[e["f"]].c.name
            c[(cu, e["tgt"], bool(e.get("win")))] += 1
    return c
if __name__ == "__main__":
    tot = collections.Counter()
    with ProcessPoolExecutor(3) as ex:
        for c in ex.map(job, range(1000, 1064)):
            tot.update(c)
    for k, v in sorted(tot.items()):
        print(k, round(v / 64 / 50, 3))
