"""Per-seed paired measurement: alliances on, off and variants, same seeds. Writes a JSON of per-seed stats.

Usage: paired.py SCENARIOS SEEDS ARMS OUT.json, e.g. `paired.py alone,farmer,fortress,multi,railway 64 on,off
base.json`; the warm arm of CONFLICT-LIFE.md 4.5 is
`ARMS='{"warm": {"pact_days": 384, "pact_effort": 1, "kin_pact": 6, "hate_pact": 6}}' paired.py ... warm warm.json`.
Read them with base_an.py base.json and warm_an.py base.json warm.json."""
import sys, json, os, collections
from concurrent.futures import ProcessPoolExecutor
from dataclasses import replace
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import run, model

ARMS = {"on": {}, "off": {"alliances": False}}
ARMS.update(json.loads(os.environ.get("ARMS", "{}")))
YEAR = 96

def job(a):
    name, seed, arm = a
    sc = run.scenarios(50 * YEAR)[name]
    sc = replace(sc, life={**sc.life, **ARMS[arm]})
    w = model.World(sc, "life", seed).run()
    st = run.world_stats(w)
    ev = w.events
    yrs = 50
    c = collections.Counter()
    for e in ev:
        f = e["f"]
        if e["act"] == "killed":
            c[("killed", f)] += 1
        if e["act"] in run.FIGHT:
            c[("fight", f, e["tgt"])] += 1
            if e.get("ally"):
                c[("joint", e["tgt"])] += 1
                c[("joint_won" if e["win"] else "joint_lost", e["tgt"])] += 1
            if e["tgt"].startswith("P:"):
                c[("on", e["tgt"])] += 1
                c[("on_lost" if not e["win"] else "on_won", e["tgt"])] += 1
        if e["culture"] == "frostmaw" and e["tgt"] == "RAIL":
            c[("frost_rail", e["act"])] += 1
        if e["culture"] == "goblin" and e["tgt"] == "RAIL" and e["act"] in run.FIGHT:
            c[("gob_rail",)] += 1
    out = {"name": name, "seed": seed, "arm": arm,
           "fights": float(sum(st["fights_per_year"]) / len(st["fights_per_year"])),
           "camp": float(sum(st["camp_fights_per_year"]) / len(st["camp_fights_per_year"])),
           "extinct": st["extinct"], "endless": st.get("endless", 0),
           "loot": {p: float(v) for p, v in st["loot_by_player"].items()},
           "by_player": {p: float(v) for p, v in st["by_player"].items()},
           "c": {"|".join(map(str, k)): v / yrs for k, v in c.items()},
           "ally": st.get("ally")}
    return out

if __name__ == "__main__":
    names = sys.argv[1].split(","); seeds = int(sys.argv[2]); arms = sys.argv[3].split(",")
    jobs = [(n, 1000 + s, a) for n in names for s in range(seeds) for a in arms]
    with ProcessPoolExecutor(int(os.environ.get("J", "6"))) as ex:
        rows = list(ex.map(job, jobs, chunksize=1))
    json.dump(rows, open(sys.argv[4], "w"), default=float)
    print("done", len(rows))
