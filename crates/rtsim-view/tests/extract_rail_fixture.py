#!/usr/bin/env python3
"""Cut the create-rail fixture out of a saved GameTest world.

    python3 -I extract_rail_fixture.py <world> <out>

<world> is neoforge/create/run-gametest/world after a run with MIC_RAIL_FIXTURE=1 on a fresh world
(RailFixtureGameTests builds the rail, the train and the logistics blocks at fixed coordinates in
region r.1.3). <out> is tests/data/create-rail. It writes:

  data/create_tracks.dat   Create's RailwaySavedData with only the fixture's graph, the signal groups
                           that graph or its train name, and the train "Fixture Express"; the other
                           tests' graphs and trains are dropped. Gzip, as Minecraft writes it.
  region/r.1.3.mca         the region file with only the chunks the fixture's blocks are in, each
                           chunk's stored bytes copied unchanged (compression and all).

Standard library only; tags keep their exact types, so a kept compound reads back as Create wrote it.
"""
import gzip
import io
import struct
import sys
from pathlib import Path

REGION = (1, 3)
# RailFixtureGameTests: x 608..638 and z 1600..1656 at most, so chunks x 38..39, z 100..103;
# chunk (38, 102) and (38, 103) hold nothing of the fixture.
CHUNKS = [(38, 100), (38, 101), (39, 100), (39, 101), (39, 102), (39, 103)]
STATION = "Fixture Central"
TRAIN = "Fixture Express"

END, BYTE, SHORT, INT, LONG, FLOAT, DOUBLE, BYTES, STRING, LIST, COMPOUND, INTS, LONGS = range(13)
SCALAR = {BYTE: ">b", SHORT: ">h", INT: ">i", LONG: ">q", FLOAT: ">f", DOUBLE: ">d"}


class Reader:
    def __init__(self, data):
        self.data, self.p = data, 0

    def take(self, fmt):
        v = struct.unpack_from(fmt, self.data, self.p)
        self.p += struct.calcsize(fmt)
        return v[0] if len(v) == 1 else v

    def string(self):
        n = self.take(">H")
        s = self.data[self.p:self.p + n]
        self.p += n
        return s.decode("utf-8")

    def payload(self, t):
        if t in SCALAR:
            return self.take(SCALAR[t])
        if t == BYTES:
            n = self.take(">i")
            v = self.data[self.p:self.p + n]
            self.p += n
            return bytes(v)
        if t == STRING:
            return self.string()
        if t == LIST:
            et, n = self.take(">b"), self.take(">i")
            return (et, [self.payload(et) for _ in range(n)])
        if t == COMPOUND:
            out = {}
            while True:
                tt = self.take(">b")
                if tt == END:
                    return out
                k = self.string()
                out[k] = (tt, self.payload(tt))
        if t in (INTS, LONGS):
            n = self.take(">i")
            fmt = ">%d%s" % (n, "i" if t == INTS else "q")
            v = list(struct.unpack_from(fmt, self.data, self.p))
            self.p += struct.calcsize(fmt)
            return v
        raise ValueError("tag type %d" % t)


def read_root(data):
    r = Reader(data)
    t = r.take(">b")
    name = r.string()
    return name, (t, r.payload(t))


def write_string(out, s):
    b = s.encode("utf-8")
    out.write(struct.pack(">H", len(b)))
    out.write(b)


def write_payload(out, t, v):
    if t in SCALAR:
        out.write(struct.pack(SCALAR[t], v))
    elif t == BYTES:
        out.write(struct.pack(">i", len(v)))
        out.write(v)
    elif t == STRING:
        write_string(out, v)
    elif t == LIST:
        et, items = v
        out.write(struct.pack(">bi", et, len(items)))
        for x in items:
            write_payload(out, et, x)
    elif t == COMPOUND:
        for k, (tt, x) in v.items():
            out.write(struct.pack(">b", tt))
            write_string(out, k)
            write_payload(out, tt, x)
        out.write(b"\x00")
    elif t in (INTS, LONGS):
        out.write(struct.pack(">i", len(v)))
        out.write(struct.pack(">%d%s" % (len(v), "i" if t == INTS else "q"), *v))
    else:
        raise ValueError("tag type %d" % t)


def write_root(name, tag):
    out = io.BytesIO()
    t, v = tag
    out.write(struct.pack(">b", t))
    write_string(out, name)
    write_payload(out, t, v)
    return out.getvalue()


def uuids(tag, found):
    """Every 4-int array under the tag: Create writes UUIDs as int arrays."""
    t, v = tag
    if t == INTS and len(v) == 4:
        found.add(tuple(v))
    elif t == COMPOUND:
        for x in v.values():
            uuids(x, found)
    elif t == LIST:
        et, items = v
        for x in items:
            uuids((et, x), found)
    return found


def strings(tag, found):
    t, v = tag
    if t == STRING:
        found.add(v)
    elif t == COMPOUND:
        for x in v.values():
            strings(x, found)
    elif t == LIST:
        et, items = v
        for x in items:
            strings((et, x), found)
    return found


def tracks(src, dst):
    name, (t, root) = read_root(gzip.decompress(src.read_bytes()))
    data = root["data"][1]
    graphs = data["RailGraphs"][1][1]
    keep = [g for g in graphs if STATION in strings((COMPOUND, g), set())]
    if len(keep) != 1:
        sys.exit("expected one graph with %r, found %d" % (STATION, len(keep)))
    trains = [tr for tr in data["Trains"][1][1] if TRAIN in tr["Name"][1]]
    if len(trains) != 1:
        sys.exit("expected one train %r, found %d" % (TRAIN, len(trains)))
    named = uuids((COMPOUND, keep[0]), set()) | uuids((COMPOUND, trains[0]), set())
    groups = [s for s in data["SignalBlocks"][1][1] if tuple(s["Id"][1]) in named]
    print("tracks: kept 1 of %d graphs, %d of %d signal groups, 1 of %d trains"
          % (len(graphs), len(groups), len(data["SignalBlocks"][1][1]), len(data["Trains"][1][1])))
    data["RailGraphs"] = (LIST, (COMPOUND, keep))
    data["SignalBlocks"] = (LIST, (COMPOUND, groups))
    data["Trains"] = (LIST, (COMPOUND, trains))
    dst.parent.mkdir(parents=True, exist_ok=True)
    # mtime 0 so the same tags give the same bytes.
    dst.write_bytes(gzip.compress(write_root(name, (t, root)), mtime=0))


def region(src, dst):
    raw = src.read_bytes()
    out = bytearray(8192)
    for cx, cz in CHUNKS:
        if (cx >> 5, cz >> 5) != REGION:
            sys.exit("chunk %d,%d is not in r.%d.%d" % (cx, cz, *REGION))
        i = (cx & 31) + (cz & 31) * 32
        off, count = int.from_bytes(raw[i * 4:i * 4 + 3], "big"), raw[i * 4 + 3]
        if off == 0:
            sys.exit("chunk %d,%d was not saved" % (cx, cz))
        o = off * 4096
        length = int.from_bytes(raw[o:o + 4], "big")
        if raw[o + 4] & 0x80:
            sys.exit("chunk %d,%d is stored outside the region file" % (cx, cz))
        body = raw[o:o + 4 + length]
        sector = len(out) // 4096
        sectors = (len(body) + 4095) // 4096
        out[i * 4:i * 4 + 4] = sector.to_bytes(3, "big") + bytes([sectors])
        out[4096 + i * 4:4096 + i * 4 + 4] = raw[4096 + i * 4:4096 + i * 4 + 4]
        out += body + bytes(sectors * 4096 - len(body))
    print("region: kept %d chunks, %d bytes" % (len(CHUNKS), len(out)))
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_bytes(bytes(out))


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    world, out = Path(sys.argv[1]), Path(sys.argv[2])
    tracks(world / "data" / "create_tracks.dat", out / "data" / "create_tracks.dat")
    name = "r.%d.%d.mca" % REGION
    region(world / "region" / name, out / "region" / name)


if __name__ == "__main__":
    main()
