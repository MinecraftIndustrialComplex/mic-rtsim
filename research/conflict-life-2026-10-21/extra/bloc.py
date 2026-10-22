"""Most pacts a camp holds at once, and pacts by pair, from the life event log."""
import sys, json, os, collections
from concurrent.futures import ProcessPoolExecutor
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import run, model
YEAR = 96
def job(a):
    name, seed = a
    sc = run.scenarios(50 * YEAR)[name]
    w = model.World(sc, "life", seed).run()
    open_ = collections.defaultdict(set)
    peak = collections.Counter()
    peak_t = 0
    pairs = collections.Counter()
    vs = collections.Counter()
    multi_vs = 0
    for e in w.events:
        if e["act"] == "pact" and e.get("accepted"):
            f, y = e["f"], e["tgt"]
            open_[f].add(y); open_[y].add(f)
            pairs["-".join(sorted((f, y)))] += 1
            vs[e.get("vs")] += 1
        elif e["act"] == "pact_end":
            f, y = e["f"], e["tgt"]
            open_[f].discard(y); open_[y].discard(f)
        for c, s in open_.items():
            peak[c] = max(peak[c], len(s))
        # a three-way bloc: a camp in two pacts at once
        if any(len(s) >= 2 for s in open_.values()):
            peak_t += 1
    return {"name": name, "seed": seed, "peak": dict(peak), "pairs": dict(pairs), "vs": {str(k): v for k, v in vs.items()},
            "two": peak_t}
if __name__ == "__main__":
    names = sys.argv[1].split(","); n = int(sys.argv[2])
    jobs = [(nm, 1000 + s) for nm in names for s in range(n)]
    with ProcessPoolExecutor(int(os.environ.get("J", "4"))) as ex:
        rows = list(ex.map(job, jobs, chunksize=1))
    json.dump(rows, open(sys.argv[3], "w"))
    for nm in names:
        rs = [r for r in rows if r["name"] == nm]
        pk = collections.Counter(max(r["peak"].values(), default=0) for r in rs)
        pr = collections.Counter(); vv = collections.Counter()
        for r in rs:
            pr.update(r["pairs"]); vv.update(r["vs"])
        print(nm, "worlds by most pacts any camp held at once:", dict(sorted(pk.items())),
              "pairs:", pr.most_common(6), "against:", vv.most_common(6))
