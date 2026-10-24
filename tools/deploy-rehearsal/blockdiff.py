"""Which blocks differ between two copies of a world inside a few boxes: what village growth replaced.

usage: python3 -I blockdiff.py WORLD_A WORLD_B BOXES.json
BOXES.json (private): [[x1, z1, x2, z2], ...] block columns, all heights. Prints, per box, the number of
blocks that differ and the commonest (block in A -> block in B) pairs, block states stripped. Chunks
missing from either world are counted and skipped. Run it on two worlds that started from the same copy
(one with rtsim, one without) to see what the sim built and what it built over.
"""
import collections
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import anvil  # noqa: E402
import struct  # noqa: E402

_value = anvil.Reader.value


def _value_with_longs(self, t, skip_keys=None):
    # anvil.Reader skips long arrays (scans never need them); block states are one
    if t == anvil.TAG_LONG_ARRAY:
        n = self.i32()
        v = struct.unpack_from('>%dq' % n, self.b, self.p)
        self.p += 8 * n
        return list(v)
    return _value(self, t, skip_keys)


anvil.Reader.value = _value_with_longs


def chunks(world, want):
    """{(cx, cz): {section y: [4096 block names]}} for the chunks in want."""
    out = {}
    regions = collections.defaultdict(set)
    for cx, cz in want:
        regions[(cx >> 5, cz >> 5)].add((cx, cz))
    for (rx, rz), cs in regions.items():
        path = os.path.join(world, 'region', 'r.%d.%d.mca' % (rx, rz))
        if not os.path.exists(path):
            continue
        for _, cx, cz, _, payload, comp, ext in anvil.iter_region(path):
            if (cx, cz) not in cs:
                continue
            mcc = os.path.join(world, 'region', 'c.%d.%d.mcc' % (cx, cz)) if ext else None
            nbt = anvil.parse_nbt(anvil.decompress(payload, comp, mcc))
            secs = {}
            for s in nbt.get('sections', []):
                bs = s.get('block_states')
                if not bs:
                    continue
                pal = [p['Name'] for p in bs['palette']]
                data = bs.get('data')
                if not data:
                    secs[s['Y']] = [pal[0]] * 4096
                    continue
                bits = max(4, (len(pal) - 1).bit_length())
                per, mask = 64 // bits, (1 << bits) - 1
                blocks = []
                for v in data:
                    v &= (1 << 64) - 1
                    for k in range(per):
                        blocks.append(pal[(v >> (k * bits)) & mask])
                secs[s['Y']] = blocks[:4096]
            out[(cx, cz)] = secs
    return out


def main():
    wa, wb, boxes = sys.argv[1], sys.argv[2], json.load(open(sys.argv[3]))
    for n, (x1, z1, x2, z2) in enumerate(boxes, 1):
        x1, x2, z1, z2 = min(x1, x2), max(x1, x2), min(z1, z2), max(z1, z2)
        want = {(cx, cz) for cx in range(x1 >> 4, (x2 >> 4) + 1) for cz in range(z1 >> 4, (z2 >> 4) + 1)}
        a, b = chunks(wa, want), chunks(wb, want)
        pairs, diff, missing = collections.Counter(), 0, 0
        for key in want:
            if key not in a or key not in b:
                missing += 1
                continue
            cx, cz = key
            for y in set(a[key]) | set(b[key]):
                sa, sb = a[key].get(y), b[key].get(y)
                if sa is None or sb is None:
                    continue
                for i in range(4096):
                    x, z = cx * 16 + (i & 15), cz * 16 + ((i >> 4) & 15)
                    if x1 <= x <= x2 and z1 <= z <= z2 and sa[i] != sb[i]:
                        diff += 1
                        pairs[(sa[i], sb[i])] += 1
        print('box %d: %d chunks (%d missing from a world), %d blocks differ' % (n, len(want), missing, diff))
        for (fa, fb), c in pairs.most_common(40):
            print('  %7d  %s -> %s' % (c, fa, fb))


if __name__ == '__main__':
    main()
