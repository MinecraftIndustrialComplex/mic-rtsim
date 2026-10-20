#!/usr/bin/env python3
"""Write the GameTest structure templates under core/src/main/resources/data/mic_rtsim/structure.

A GameTest structure template is a plain gzipped NBT compound, so no Minecraft runtime is needed to
make one. Adapted from mods/mic-climate/tools/make_test_structure.py.

    python3 tools/make_test_structure.py core/src/main/resources/data/mic_rtsim/structure

writes `arena.nbt`: an 11 x 5 x 11 box with a polished andesite floor (y = 0) and a two-block glass
ring (y = 1 and 2) around its edge, so a caravan's trader and llamas stay inside the test's bounds
while it runs. Everything else is air.
"""
import gzip
import os
import struct
import sys

DATA_VERSION = 3955  # 1.21.1

TAG_END = 0
TAG_INT = 3
TAG_STRING = 8
TAG_LIST = 9
TAG_COMPOUND = 10


def s(text: str) -> bytes:
    b = text.encode("utf-8")
    return struct.pack(">H", len(b)) + b


def named(tag_id: int, name: str, payload: bytes) -> bytes:
    return struct.pack(">B", tag_id) + s(name) + payload


def int_list(values) -> bytes:
    return struct.pack(">Bi", TAG_INT, len(values)) + b"".join(struct.pack(">i", v) for v in values)


def compound_list(payloads) -> bytes:
    return struct.pack(">Bi", TAG_COMPOUND, len(payloads)) + b"".join(payloads)


def empty_list() -> bytes:
    return struct.pack(">Bi", TAG_END, 0)


def write_arena(path: str, size_x: int, size_y: int, size_z: int) -> None:
    names = ["minecraft:polished_andesite", "minecraft:air", "minecraft:glass"]
    palette = compound_list(
        [named(TAG_STRING, "Name", s(n)) + struct.pack(">B", TAG_END) for n in names]
    )
    blocks = []
    for x in range(size_x):
        for y in range(size_y):
            for z in range(size_z):
                edge = x in (0, size_x - 1) or z in (0, size_z - 1)
                if y == 0:
                    state = 0
                elif edge and y <= 2:
                    state = 2
                else:
                    state = 1
                blocks.append(
                    named(TAG_LIST, "pos", int_list([x, y, z]))
                    + named(TAG_INT, "state", struct.pack(">i", state))
                    + struct.pack(">B", TAG_END)
                )
    root = (
        named(TAG_INT, "DataVersion", struct.pack(">i", DATA_VERSION))
        + named(TAG_LIST, "size", int_list([size_x, size_y, size_z]))
        + named(TAG_LIST, "palette", palette)
        + named(TAG_LIST, "blocks", compound_list(blocks))
        + named(TAG_LIST, "entities", empty_list())
        + struct.pack(">B", TAG_END)
    )
    document = struct.pack(">B", TAG_COMPOUND) + s("") + root
    with gzip.open(path, "wb") as f:
        f.write(document)
    print(f"wrote {path}: {len(blocks)} blocks, size {size_x}x{size_y}x{size_z}")


if __name__ == "__main__":
    out_dir = sys.argv[1]
    os.makedirs(out_dir, exist_ok=True)
    write_arena(os.path.join(out_dir, "arena.nbt"), 11, 5, 11)
