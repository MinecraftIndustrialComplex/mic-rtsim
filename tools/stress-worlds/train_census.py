#!/usr/bin/env python3
"""Create trains and stations per dimension in a world's data/create_tracks.dat (Create 6), read only.

    python3 -I train_census.py <world>/data/create_tracks.dat

Prints trains per dimension (a carriage's EntityPositioning names its dimension through the file's
DimensionPalette), the trains with a schedule, track nodes per dimension and stations per dimension.
Needs nbtlib (nix shell --impure --expr 'with import (builtins.getFlake "nixpkgs") {};
python3.withPackages (p: [p.nbtlib])').
"""
import collections
import json
import sys

import nbtlib


def main():
    d = nbtlib.load(sys.argv[1])["data"]
    pal = [str(p["Id"]) for p in d["DimensionPalette"]]
    per, sched, rows = collections.Counter(), collections.Counter(), []
    for t in d["Trains"]:
        ds = {pal[int(e["Dim"])] for c in t["Carriages"] for e in c["EntityPositioning"]}
        n = len(t["Runtime"]["Schedule"]["Entries"]) if "Schedule" in t.get("Runtime", {}) else 0
        key = ",".join(sorted(ds)) or "?"
        per[key] += 1
        if n:
            sched[key] += 1
        name = str(t["Name"])
        rows.append((json.loads(name) if name.startswith('"') else name, len(t["Carriages"]), key, n))
    print("trains", sum(per.values()), "per dimension", dict(per))
    print("scheduled per dimension", dict(sched))
    for name, cars, dim, entries in rows:
        if entries:
            print(f"  scheduled: {name!r}, {cars} carriages, {dim}, {entries} entries")
    nodes, stations = collections.Counter(), collections.Counter()
    for g in d["RailGraphs"]:
        for n in g["Nodes"]:
            nodes[pal[int(n["Location"]["D"])]] += 1
        for s in g["Points"].get("create:station", []):
            stations[pal[int(s["Edge"][0]["D"])]] += 1
    print("graphs", len(d["RailGraphs"]), "track nodes per dimension", dict(nodes))
    print("stations", sum(stations.values()), "per dimension", dict(stations))


if __name__ == "__main__":
    main()
