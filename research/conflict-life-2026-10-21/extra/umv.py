import os
import sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import model, run
from concurrent.futures import ProcessPoolExecutor
from collections import Counter
from model import YEAR

def farmer(seed):
    sc = run.scenarios(50 * YEAR)["farmer"]
    w = model.World(sc, "life", seed).run()
    inv = [e for e in w.events if e["act"] == "invasion" and e["f"] == "U1" and e["tgt"] == "P:Ben"]
    succ = [e for e in w.events if e["f"] == "U1" and e["act"] == "succession"]
    fell = sum(1 for e in inv if any(s["t"] == e["t"] for s in succ))
    whys = Counter(e.get("reason", "")[:60] for e in inv)
    leads = Counter(str(e.get("speaker", "")) for e in inv)
    return len(inv), sum(1 for e in inv if e.get("win")), fell, whys, leads

if __name__ == "__main__":
    with ProcessPoolExecutor(6) as ex:
        r = list(ex.map(farmer, range(1000, 1064)))
    print("inv", sum(a[0] for a in r), "won", sum(a[1] for a in r), "fell same day", sum(a[2] for a in r),
          "worlds", sum(1 for a in r if a[0]))
    w = Counter(); l = Counter()
    for a in r: w.update(a[3]); l.update(a[4])
    print(w.most_common(8)); print(l.most_common(8))
