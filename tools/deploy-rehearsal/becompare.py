"""Compares block entity and entity counts of two prune.py scans over the chunks of the first.

usage: python3 -I becompare.py BEFORE_SCAN.json AFTER_SCAN.json
Only chunks present in BEFORE are counted in both, so chunks generated later do not show up as gains.
"""
import collections
import json
import os
import sys


def load(path):
    s = json.load(open(path))
    root = os.path.commonpath(list(s['dims'].values())) if len(s['dims']) > 1 else list(s['dims'].values())[0]
    be, ents = {}, {}
    for f, info in s['files'].items():
        if isinstance(info['chunks'], str):
            continue
        dim = next(n for n, b in s['dims'].items() if os.path.dirname(os.path.dirname(f)) == b)
        for c in info['chunks']:
            if isinstance(c[2], str):
                continue
            if info['kind'] == 'region':
                be[(dim, c[0], c[1])] = c[4]
            elif info['kind'] == 'entities':
                ents[(dim, c[0], c[1])] = c[2]
    return be, ents


def main():
    b_be, b_en = load(sys.argv[1])
    a_be, a_en = load(sys.argv[2])
    keys = set(b_be)
    missing_chunks = [k for k in keys if k not in a_be]
    cb, ca = collections.Counter(), collections.Counter()
    for k in keys:
        cb.update(b_be[k])
        ca.update(a_be.get(k, {}))
    eb, ea = collections.Counter(), collections.Counter()
    for k in set(b_en) | set(k for k in a_en if k[:3] in keys or k in keys):
        if k in keys:
            eb.update(b_en.get(k, {}))
            ea.update(a_en.get(k, {}))
    print('chunks before %d, missing after %d' % (len(keys), len(missing_chunks)))
    print('block entities: before %d after %d' % (sum(cb.values()), sum(ca.values())))
    for bid in sorted(set(cb) | set(ca), key=lambda x: -(abs(cb[x] - ca[x]))):
        if cb[bid] != ca[bid]:
            print('  BE %-55s %6d -> %6d' % (bid, cb[bid], ca[bid]))
    print('entities: before %d after %d' % (sum(eb.values()), sum(ea.values())))
    for eid in sorted(set(eb) | set(ea), key=lambda x: -(abs(eb[x] - ea[x]))):
        if eb[eid] != ea[eid]:
            print('  E  %-55s %6d -> %6d' % (eid, eb[eid], ea[eid]))


if __name__ == '__main__':
    main()
