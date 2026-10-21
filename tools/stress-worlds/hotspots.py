#!/usr/bin/env python3
"""Where a world's block entities are: per chunk counts, for choosing forceload boxes.

    python3 -I hotspots.py <world_dir> <out.json> [workers]

Reads region/*.mca only (never runs anything from the world), with the skipping NBT reader of
research/stress-worlds-2026-10-21-data/audit_world.py (loaded by path, since -I drops the script
directory from sys.path). Output: per chunk (x, z, all BEs, Create BEs, InhabitedTime, counts of
the interesting types) and the positions of every Create station, packager and vault.
"""
import collections
import importlib.util
import json
import os
import sys
from concurrent.futures import ProcessPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
AUDIT = os.path.join(HERE, "..", "..", "research", "stress-worlds-2026-10-21-data", "audit_world.py")
spec = importlib.util.spec_from_file_location("audit_world", AUDIT)
aw = importlib.util.module_from_spec(spec)
spec.loader.exec_module(aw)

SPEC = {
    "InhabitedTime": True, "xPos": True, "zPos": True,
    "block_entities": {"id": True, "x": True, "y": True, "z": True},
    "Level": {"InhabitedTime": True, "xPos": True, "zPos": True,
              "TileEntities": {"id": True, "x": True, "y": True, "z": True}},
}
POINT_TYPES = ("create:track_station", "create:packager", "create:item_vault", "create:package_frogport",
               "create:factory_panel", "create:track_signal", "create:depot")


def scan(path):
    rx, rz = [int(v) for v in os.path.basename(path).split(".")[1:3]]
    out = []
    for cx, cz, ct, payload in aw.read_chunks(path):
        try:
            b = aw.decomp(ct, payload)
            root = aw.parse_root(b, SPEC)
        except Exception:
            continue
        lv = root.get("Level") or root
        bes = lv.get("block_entities") or lv.get("TileEntities") or []
        x, z = rx * 32 + cx, rz * 32 + cz
        types = collections.Counter(e.get("id", "?") for e in bes)
        pts = [(e.get("id"), e.get("x"), e.get("y"), e.get("z")) for e in bes if e.get("id") in POINT_TYPES]
        out.append({"x": x, "z": z, "be": len(bes),
                    "create": sum(v for k, v in types.items() if str(k).startswith("create:")),
                    "inh": lv.get("InhabitedTime") or 0, "types": dict(types), "points": pts})
    return out


def main():
    world, out = sys.argv[1], sys.argv[2]
    workers = int(sys.argv[3]) if len(sys.argv) > 3 else 6
    reg = os.path.join(world, "region")
    files = [os.path.join(reg, f) for f in sorted(os.listdir(reg)) if f.endswith(".mca")]
    chunks = []
    with ProcessPoolExecutor(workers) as ex:
        for r in ex.map(scan, files):
            chunks.extend(r)
    chunks.sort(key=lambda c: -c["be"])
    tot = collections.Counter()
    for c in chunks:
        tot.update(c["types"])
    summary = {"world": world, "chunks": len(chunks), "be_total": sum(c["be"] for c in chunks),
               "be_chunks": sum(1 for c in chunks if c["be"]), "top_types": tot.most_common(40)}
    xs = [c["x"] for c in chunks if c["create"]]
    zs = [c["z"] for c in chunks if c["create"]]
    if xs:
        summary["create_bbox_chunks"] = [min(xs), min(zs), max(xs), max(zs)]
        summary["create_chunks"] = len(xs)
    json.dump({"summary": summary, "chunks": [c for c in chunks if c["be"]]}, open(out, "w"))
    print(json.dumps(summary, indent=1))


if __name__ == "__main__":
    main()
