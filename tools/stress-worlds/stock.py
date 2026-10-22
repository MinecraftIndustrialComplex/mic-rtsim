#!/usr/bin/env python3
"""A synthetic world's factory stock and port vaults, from saved worlds, read only (choice SG38).

    python3 -I stock.py <stressgen.json> <world> [<world> ...] [--diff N]

For each world, inside every generated district's chunk box:
- chests: every minecraft:chest block entity (the factories' input chests), totals by item;
- vaults: create:item_vault block entities within 2 blocks (x, z) and 5 (y) of a dock, i.e. a station's
  Portable Storage Interface; below the interface is the port's sink vault, above it the supply vault.
Then, for every world after the first, the chests whose count of an item differs from the first world's
by more than N (default 64), worst first. Give the rtsim-off arm first to compare the rtsim arms with it.

Needs nbtlib (nix shell --impure --expr 'with import (builtins.getFlake "nixpkgs") {};
python3.withPackages (p: [p.nbtlib])').
"""
import collections
import io
import json
import struct
import sys
import zlib

import nbtlib


def chunks(world, cx0, cz0, cx1, cz1):
    regs = {}
    for cx in range(cx0, cx1 + 1):
        for cz in range(cz0, cz1 + 1):
            path = f"{world}/region/r.{cx >> 5}.{cz >> 5}.mca"
            if path not in regs:
                try:
                    regs[path] = open(path, "rb").read()
                except FileNotFoundError:
                    regs[path] = b""
            data = regs[path]
            i = (cx & 31) + (cz & 31) * 32
            if len(data) < 4096:
                continue
            off = int.from_bytes(data[i * 4:i * 4 + 3], "big")
            if off == 0:
                continue
            o = off * 4096
            length, comp = struct.unpack(">IB", data[o:o + 5])
            raw = data[o + 5:o + 4 + length]
            raw = zlib.decompress(raw) if comp == 2 else raw
            yield nbtlib.File.parse(io.BytesIO(raw))


def items(t, out):
    """Adds every item stack ({id, count}) found anywhere under t to out."""
    if isinstance(t, dict):
        if "id" in t and "count" in t and isinstance(t.get("id"), str):
            out[str(t["id"])] += int(t["count"])
            return
        for v in t.values():
            items(v, out)
    elif isinstance(t, list):
        for v in t:
            items(v, out)


def census(world, g, docks):
    chest = {}
    vault = {"sink": collections.Counter(), "supply": collections.Counter()}
    for dist in g["districts"]:
        x0, z0, x1, z1 = dist["chunks"]
        for c in chunks(world, x0, z0, x1, z1):
            for be in c.get("block_entities", []):
                bid = str(be["id"])
                p = (int(be["x"]), int(be["y"]), int(be["z"]))
                if bid == "minecraft:chest":
                    out = collections.Counter()
                    items(be, out)
                    chest[p] = out
                elif bid == "create:item_vault":
                    for d in docks:
                        if abs(p[0] - d[0]) <= 2 and abs(p[2] - d[2]) <= 2 and abs(p[1] - d[1]) <= 5:
                            out = collections.Counter()
                            items(be, out)
                            vault["sink" if p[1] < d[1] else "supply"].update(out)
                            break
    return chest, vault


def main():
    args = sys.argv[1:]
    thr = 64
    if "--diff" in args:
        i = args.index("--diff")
        thr = int(args[i + 1])
        del args[i:i + 2]
    g = json.load(open(args[0]))
    docks = [tuple(d) for t in g["trains"] for d in t["docks"]]
    res = [(w, *census(w, g, docks)) for w in args[1:]]
    base = res[0][1]
    for w, chest, vault in res:
        tot = collections.Counter()
        for c in chest.values():
            tot.update(c)
        print(f"== {w}: {len(chest)} chests")
        print("   chests:", dict(sorted(tot.items())))
        print("   sink vaults:", dict(sorted(vault["sink"].items())))
        print("   supply vaults:", dict(sorted(vault["supply"].items())))
        if chest is base:
            continue
        drops = []
        for p, c in chest.items():
            b = base.get(p, collections.Counter())
            for it in set(b) | set(c):
                d = c[it] - b[it]
                if abs(d) > thr:
                    drops.append((d, p, it))
        drops.sort()
        print(f"   chests off the first world by more than {thr}: {len(drops)}; worst", drops[:6])


if __name__ == "__main__":
    main()
