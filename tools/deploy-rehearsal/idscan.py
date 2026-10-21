"""Collect every namespaced id string in a world's chunk and entity NBT, and every id a mods folder defines.

usage:
  python3 -I idscan.py world WORLD OUT.json      # {id: [chunks containing it, sample chunk]} per dimension
  python3 -I idscan.py mods  MODS_DIR OUT.json   # ids from blockstates, item models, lang keys, molecule data
Compare the two with idscan.py diff WORLDIDS OLDMODS NEWMODS: ids present in the world and defined by the old
pack but not by the new one are what a pack upgrade would lose.
"""
import io
import json
import os
import re
import sys
import zipfile
from concurrent.futures import ProcessPoolExecutor

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import anvil  # noqa: E402
import prune  # noqa: E402

ID_RE = re.compile(rb'(?<![a-z0-9_.\-])([a-z0-9_.\-]{2,40}):([a-z0-9_/.\-]{2,80})')
IGNORE_NS = {b'http', b'https', b'file', b'jar'}


def scan_file(args):
    path, kind = args
    found = {}
    d = os.path.dirname(path)
    for i, cx, cz, ts, payload, comp, ext in anvil.iter_region(path):
        try:
            raw = anvil.decompress(payload, comp, os.path.join(d, 'c.%d.%d.mcc' % (cx, cz)) if ext else None)
        except Exception:  # noqa: BLE001
            continue
        seen = set()
        for m in ID_RE.finditer(raw):
            if m.group(1) in IGNORE_NS:
                continue
            seen.add((m.group(1) + b':' + m.group(2)).decode())
        for s in seen:
            f = found.setdefault(s, [0, None])
            f[0] += 1
            if f[1] is None:
                f[1] = [kind, cx, cz]
    return path, found


def cmd_world(world, out, workers):
    jobs = []
    dimmap = {}
    for name, base in prune.dims(world):
        for kind in ('region', 'entities'):
            dd = os.path.join(base, kind)
            if os.path.isdir(dd):
                for f in sorted(os.listdir(dd)):
                    if f.endswith('.mca'):
                        jobs.append((os.path.join(dd, f), kind))
                        dimmap[os.path.join(dd, f)] = name
    res = {}
    with ProcessPoolExecutor(workers) as ex:
        for path, found in ex.map(scan_file, jobs, chunksize=2):
            dim = res.setdefault(dimmap[path], {})
            for k, (n, sample) in found.items():
                cur = dim.setdefault(k, [0, sample])
                cur[0] += n
    json.dump(res, open(out, 'w'))


LANG_RE = re.compile(r'^(block|item|entity|fluid|fluid_type|effect|enchantment)\.([a-z0-9_.\-]+?)\.([a-z0-9_./\-]+)$')


def jar_ids(data, out, depth=0):
    try:
        z = zipfile.ZipFile(io.BytesIO(data))
    except zipfile.BadZipFile:
        return
    for n in z.namelist():
        p = n.split('/')
        if len(p) >= 4 and p[0] == 'assets' and p[2] == 'blockstates' and n.endswith('.json'):
            out.add('%s:%s' % (p[1], '/'.join(p[3:])[:-5]))
        elif len(p) >= 5 and p[0] == 'assets' and p[2] == 'models' and p[3] == 'item' and n.endswith('.json'):
            out.add('%s:%s' % (p[1], '/'.join(p[4:])[:-5]))
        elif len(p) == 4 and p[0] == 'assets' and p[2] == 'lang' and p[3] == 'en_us.json':
            try:
                lang = json.loads(z.read(n).decode('utf-8', 'replace'), strict=False)
            except Exception:  # noqa: BLE001
                continue
            for k in lang:
                m = LANG_RE.match(k)
                if m:
                    out.add('%s:%s' % (m.group(2), m.group(3)))
        elif len(p) >= 4 and p[0] == 'data' and n.endswith('.json') and ('molecules' in p or 'destroy' in p[2:4]):
            out.add('%s:%s' % (p[1], p[-1][:-5]))
        elif n.startswith('META-INF/jarjar/') and n.endswith('.jar') and depth < 3:
            jar_ids(z.read(n), out, depth + 1)


def cmd_mods(mods, out):
    ids = set()
    for f in sorted(os.listdir(mods)):
        if f.endswith('.jar'):
            with open(os.path.join(mods, f), 'rb') as fh:
                jar_ids(fh.read(), ids)
    json.dump(sorted(ids), open(out, 'w'))


def cmd_diff(world_ids, old_ids, new_ids):
    w = json.load(open(world_ids))
    old, new = set(json.load(open(old_ids))), set(json.load(open(new_ids)))
    old_ns = {i.split(':')[0] for i in old}
    new_ns = {i.split(':')[0] for i in new}
    report = {}
    for dim, ids in w.items():
        lost = {k: v for k, v in ids.items() if k in old and k not in new}
        gone_ns = {k: v for k, v in ids.items() if k.split(':')[0] in old_ns - new_ns}
        report[dim] = {'lost_ids': dict(sorted(lost.items(), key=lambda kv: -kv[1][0])),
                       'gone_namespace_ids': dict(sorted(gone_ns.items(), key=lambda kv: -kv[1][0]))}
    json.dump(report, sys.stdout, indent=1)


if __name__ == '__main__':
    c = sys.argv[1]
    if c == 'world':
        cmd_world(sys.argv[2], sys.argv[3], int(os.environ.get('WORKERS', '2')))
    elif c == 'mods':
        cmd_mods(sys.argv[2], sys.argv[3])
    elif c == 'diff':
        cmd_diff(sys.argv[2], sys.argv[3], sys.argv[4])
