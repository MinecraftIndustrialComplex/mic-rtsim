#!/usr/bin/env python3
"""Write the industry GameTest structure under create/src/main/resources/data/mic_rtsim_create/structure.

    python3 tools/make_industry_structure.py create/src/main/resources/data/mic_rtsim_create/structure

writes `yard.nbt`: a 34 x 6 x 34 box with a polished andesite floor (y = 0) and air above. 34 blocks
always span at least two chunk borders in x and in z, wherever the GameTest runner puts the structure,
so a test can place blocks on either side of a chunk border (district detection merges chunks; a sweep
of a district spans chunks). The test finds the borders from the absolute coordinates.
Uses the encoder of tools/make_test_structure.py.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import make_test_structure as m  # noqa: E402


def write_yard(path: str, size_x: int, size_y: int, size_z: int) -> None:
    import gzip
    import struct
    names = ["minecraft:polished_andesite", "minecraft:air"]
    palette = m.compound_list(
        [m.named(m.TAG_STRING, "Name", m.s(n)) + struct.pack(">B", m.TAG_END) for n in names]
    )
    blocks = []
    for x in range(size_x):
        for y in range(size_y):
            for z in range(size_z):
                blocks.append(
                    m.named(m.TAG_LIST, "pos", m.int_list([x, y, z]))
                    + m.named(m.TAG_INT, "state", struct.pack(">i", 0 if y == 0 else 1))
                    + struct.pack(">B", m.TAG_END)
                )
    root = (
        m.named(m.TAG_INT, "DataVersion", struct.pack(">i", m.DATA_VERSION))
        + m.named(m.TAG_LIST, "size", m.int_list([size_x, size_y, size_z]))
        + m.named(m.TAG_LIST, "palette", palette)
        + m.named(m.TAG_LIST, "blocks", m.compound_list(blocks))
        + m.named(m.TAG_LIST, "entities", m.empty_list())
        + struct.pack(">B", m.TAG_END)
    )
    document = struct.pack(">B", m.TAG_COMPOUND) + m.s("") + root
    with gzip.open(path, "wb") as f:
        f.write(document)
    print(f"wrote {path}: {len(blocks)} blocks, size {size_x}x{size_y}x{size_z}")


if __name__ == "__main__":
    out_dir = sys.argv[1]
    os.makedirs(out_dir, exist_ok=True)
    write_yard(os.path.join(out_dir, "yard.nbt"), 34, 6, 34)
