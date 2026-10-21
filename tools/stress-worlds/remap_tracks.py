#!/usr/bin/env python3
"""Turn Steam 'n' Rails track into Create andesite track in a COPY of a 1.19.2 world (option A clean-up).

    python3 -I remap_tracks.py <world_copy_dir> [--dry-run] [--drop NAMESPACE ...]

The pack has no Steam 'n' Rails, so its track blocks would load as air while the track graph keeps the
edges (Create falls back to andesite for an unknown material): a ghost graph. This rewrites, in place:

  * every chunk palette entry `railways:track_*` (phantom, tieless, the wood and monorail variants)
    to `create:track` with the same properties (the blocks share TrackBlock's state properties, and
    their block entities are already `create:track`);
  * every `Material: "railways:*"` in a track block entity's bezier connections to `create:andesite`;
  * every edge material `railways:*` in data/create_tracks.dat to `create:andesite`;
  * Steam 'n' Rails bogeys to Create's, and foreign schedule instructions and conditions (see
    fix_schedules: Create 6.0.10 cannot save a train holding one).

With `--drop NS` (repeatable) it also turns every palette entry of namespace NS into air and removes NS's block
entities: what the server does anyway with a missing mod, minus one "Skipping BlockEntity" error line per
block entity (StressA has 150,000 Framed Blocks block entities).

Only chunks whose bytes mention `railways:` (or a dropped namespace) are parsed and re-encoded; a region file is rewritten only
when one of its chunks changed (sectors laid out again, timestamps kept). Needs nbtlib. Never run it on
an original download.
"""
import gzip
import io
import os
import struct
import sys
import zlib

import nbtlib
from nbtlib import String

TRACK_PREFIX = "railways:track_"
KEEP = {"railways:track_buffer", "railways:track_buffer_wood_variant", "railways:track_coupler",
        "railways:track_switch_andesite", "railways:track_switch_brass"}  # not track pieces


def remap_palette(chunk, stats, drop=()):
    changed = False
    for sec in chunk.get("sections", []):
        bs = sec.get("block_states")
        if not bs:
            continue
        for e in bs.get("palette", []):
            n = str(e["Name"])
            if drop and n.split(":", 1)[0] in drop:
                e["Name"] = String("minecraft:air")
                if "Properties" in e:
                    del e["Properties"]
                stats["dropped_palette"] = stats.get("dropped_palette", 0) + 1
                changed = True
                continue
            if n.startswith(TRACK_PREFIX) and n not in KEEP and not n.startswith("railways:track_switch"):
                e["Name"] = String("create:track")
                stats["palette"] = stats.get("palette", 0) + 1
                names = stats.setdefault("names", {})
                names[n] = names.get(n, 0) + 1
                changed = True
    if drop and "block_entities" in chunk:
        bes = chunk["block_entities"]
        keep = [be for be in bes if str(be.get("id", "")).split(":", 1)[0] not in drop]
        if len(keep) != len(bes):
            stats["dropped_bes"] = stats.get("dropped_bes", 0) + len(bes) - len(keep)
            bes.clear()
            bes.extend(keep)
            changed = True
    for be in chunk.get("block_entities", []):
        for c in be.get("Connections", []) if "Connections" in be else []:
            m = str(c.get("Material", ""))
            if m.startswith("railways:"):
                c["Material"] = String("create:andesite")
                stats["be_materials"] = stats.get("be_materials", 0) + 1
                changed = True
    return changed


def fix_schedules(trains):
    """Create 6.0.10 reads an unknown schedule instruction or wait condition as null and then throws a
    NullPointerException in Train.write on every save (seen on StressA: Create Railways Navigator's).
    A delay-like foreign condition becomes create:delay with its Value and TimeUnit; any other foreign
    condition is dropped; an entry whose instruction is foreign is dropped (CurrentEntry follows, and
    the condition progress resets when the current entry changes)."""
    st = {}
    for tr in trains:
        rt = tr.get("Runtime")
        if rt is None or "Schedule" not in rt:
            continue
        entries = rt["Schedule"].get("Entries", [])
        keep = []
        cur = int(rt.get("CurrentEntry", 0))
        new_cur = None
        for i, e in enumerate(entries):
            ins = str(e["Instruction"]["Id"])
            if not ins.startswith("create:"):
                st["dropped_entry " + ins] = st.get("dropped_entry " + ins, 0) + 1
                continue
            for cl in e.get("Conditions", []):
                kept = []
                for c in cl:
                    cid = str(c["Id"])
                    if cid.startswith("create:"):
                        kept.append(c)
                        continue
                    d = c.get("Data", nbtlib.Compound())
                    if "Value" in d and "TimeUnit" in d:
                        c["Id"] = String("create:delay")
                        c["Data"] = nbtlib.Compound({"Value": d["Value"], "TimeUnit": d["TimeUnit"]})
                        kept.append(c)
                        st["as_delay " + cid] = st.get("as_delay " + cid, 0) + 1
                    else:
                        st["dropped_condition " + cid] = st.get("dropped_condition " + cid, 0) + 1
                cl.clear()
                cl.extend(kept)
            if new_cur is None and i >= cur:
                new_cur = len(keep)
            keep.append(e)
        if len(keep) != len(entries):
            entries.clear()
            entries.extend(keep)
            nc = (new_cur or 0) % max(1, len(keep))
            if nc != cur:
                rt["CurrentEntry"] = nbtlib.Int(nc)
                for k in ("ConditionProgress", "ConditionContext"):
                    if k in rt:
                        del rt[k]
    return st


def read_region(path):
    data = open(path, "rb").read()
    chunks = {}
    if len(data) < 8192:
        return chunks, data[4096:8192] if len(data) >= 8192 else b"\0" * 4096
    for i in range(1024):
        off = int.from_bytes(data[i * 4:i * 4 + 3], "big")
        cnt = data[i * 4 + 3]
        if off == 0 or cnt == 0:
            continue
        start = off * 4096
        ln = struct.unpack_from(">I", data, start)[0]
        ct = data[start + 4]
        chunks[i] = (ct, data[start + 5:start + 4 + ln])
    return chunks, data[4096:8192]


def write_region(path, chunks, stamps):
    header = bytearray(4096)
    body = bytearray()
    sector = 2
    for i in range(1024):
        if i not in chunks:
            continue
        ct, payload = chunks[i]
        blob = struct.pack(">IB", len(payload) + 1, ct) + payload
        pad = (-len(blob)) % 4096
        blob += b"\0" * pad
        n = len(blob) // 4096
        if n > 255:
            raise ValueError("chunk too big for one entry")
        header[i * 4:i * 4 + 3] = sector.to_bytes(3, "big")
        header[i * 4 + 3] = n
        body += blob
        sector += n
    tmp = path + ".tmp"
    with open(tmp, "wb") as f:
        f.write(bytes(header) + bytes(stamps) + bytes(body))
    os.replace(tmp, path)


def decode(ct, payload):
    if ct == 2:
        return zlib.decompress(payload)
    if ct == 1:
        return gzip.decompress(payload)
    if ct == 3:
        return payload
    raise ValueError(f"compression {ct}")


def main():
    world = sys.argv[1]
    dry = "--dry-run" in sys.argv
    drop = {sys.argv[i + 1] for i, x in enumerate(sys.argv) if x == "--drop"}
    needles = [b"railways:"] + [d.encode() + b":" for d in drop]
    stats = {"chunks_changed": 0, "regions_changed": 0}
    reg = os.path.join(world, "region")
    for f in sorted(os.listdir(reg)):
        if not f.endswith(".mca"):
            continue
        path = os.path.join(reg, f)
        chunks, stamps = read_region(path)
        dirty = False
        for i, (ct, payload) in list(chunks.items()):
            raw = decode(ct, payload)
            if not any(n in raw for n in needles):
                continue
            nb = nbtlib.File.parse(io.BytesIO(raw))
            if remap_palette(nb, stats, drop):
                buf = io.BytesIO()
                nb.write(buf)
                chunks[i] = (2, zlib.compress(buf.getvalue()))
                stats["chunks_changed"] += 1
                dirty = True
        if dirty:
            stats["regions_changed"] += 1
            if not dry:
                write_region(path, chunks, stamps)
    tracks = os.path.join(world, "data", "create_tracks.dat")
    if os.path.exists(tracks):
        nb = nbtlib.load(tracks)
        n = [0]

        def walk(t):
            if isinstance(t, nbtlib.Compound):
                for k in list(t.keys()):
                    v = t[k]
                    if k == "Material" and isinstance(v, String) and str(v).startswith("railways:"):
                        t[k] = String("create:andesite")
                        n[0] += 1
                    else:
                        walk(v)
            elif isinstance(t, nbtlib.List):
                for v in t:
                    walk(v)

        walk(nb["data"]["RailGraphs"])
        walk(nb["data"]["Trains"])  # travelling points carry edge data too
        # Bogeys: Create 6.0.10's CarriageBogey.read casts BLOCK.get(Type) to AbstractBogeyBlock, so an
        # unknown bogey block (air) is a ClassCastException, not a fallback. Map them to Create's own.
        bogeys = {}
        for t in nb["data"]["Trains"]:
            for c in t.get("Carriages", []):
                for key in ("FirstBogey", "SecondBogey"):
                    if key not in c:
                        continue
                    b = c[key]
                    ty = str(b.get("Type", ""))
                    if ty.startswith("railways:"):
                        big = any(w in ty for w in ("large", "doubleaxle", "double_axle", "triple"))
                        b["Type"] = String("create:large_bogey" if big else "create:small_bogey")
                        bogeys[ty] = bogeys.get(ty, 0) + 1
                    bd = b.get("BogeyData")
                    if bd is not None and str(bd.get("BogeyStyle", "")).startswith("railways:"):
                        bd["BogeyStyle"] = String("create:standard")
        stats["bogeys"] = bogeys
        stats["schedules"] = fix_schedules(nb["data"]["Trains"])
        n = n[0] + sum(bogeys.values()) + sum(stats["schedules"].values())
        stats["graph_materials"] = n
        if not dry and n:
            nb.save(tracks)
    print(stats)


if __name__ == "__main__":
    main()
