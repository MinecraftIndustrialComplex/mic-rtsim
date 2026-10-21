"""Prune a Minecraft 1.21.1 world to the chunks players care about (LiveSrv deploy rehearsal).

usage:
  python3 -I prune.py scan  WORLD OUT.json            # per chunk facts, every dimension
  python3 -I prune.py plan  OUT.json PLAN.json [opts] # choose the keep set, print sizes
  python3 -I prune.py write WORLD PLAN.json DEST      # copy WORLD to DEST with only kept chunks

Keep rules (plan): InhabitedTime >= --inhabited (ticks); any block entity in a tech namespace;
any Create contraption or Sable-related entity; village structure starts and the chunks their
pieces cover (so late adoption can be tested); then a margin ring of --margin chunks around all of it.
region/, entities/ and poi/ are pruned to the same chunk set; everything else is copied as is.
"""
import json
import os
import shutil
import sys
from concurrent.futures import ProcessPoolExecutor

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import anvil  # noqa: E402

SKIP = {'sections', 'Heightmaps', 'block_ticks', 'fluid_ticks', 'PostProcessing', 'blending_data',
        'below_zero_retrogen', 'CarvingMasks', 'Lights'}

TECH_NS = ('create', 'destroy', 'petrolpark', 'powergrid', 'createdieselgenerators', 'createaddition',
           'vintage', 'createvintage', 'createbigcannons', 'cbc', 'aeronautics', 'sable', 'simulated',
           'numismatics', 'mic_', 'copycats', 'create_', 'createpropulsion', 'drivebywire', 'tracks',
           'computercraft', 'createoreexcavation', 'createendertransmission', 'createradar',
           'create_power_loader', 'sophisticatedstorage', 'overgeared', 'tacz', 'createdeco', 'design_decor',
           'propulsion', 'offroad', 'tfmg')


def dims(world):
    """(name, base dir) for every dimension that has a region folder."""
    out = []
    if os.path.isdir(os.path.join(world, 'region')):
        out.append(('minecraft:overworld', world))
    for name, sub in (('minecraft:the_nether', 'DIM-1'), ('minecraft:the_end', 'DIM1')):
        if os.path.isdir(os.path.join(world, sub, 'region')):
            out.append((name, os.path.join(world, sub)))
    droot = os.path.join(world, 'dimensions')
    if os.path.isdir(droot):
        for ns in sorted(os.listdir(droot)):
            for p in sorted(os.listdir(os.path.join(droot, ns))):
                base = os.path.join(droot, ns, p)
                if os.path.isdir(os.path.join(base, 'region')):
                    out.append(('%s:%s' % (ns, p), base))
    return out


def scan_region(args):
    path, kind = args
    res = []
    d = os.path.dirname(path)
    try:
        for i, cx, cz, ts, payload, comp, ext in anvil.iter_region(path):
            try:
                raw = anvil.decompress(payload, comp, os.path.join(d, 'c.%d.%d.mcc' % (cx, cz)) if ext else None)
                if kind == 'region':
                    nbt = anvil.parse_nbt(raw, SKIP)
                    bes = {}
                    for be in nbt.get('block_entities', []) or []:
                        bid = be.get('id', '?') if isinstance(be, dict) else '?'
                        bes[bid] = bes.get(bid, 0) + 1
                    starts = []
                    st = (nbt.get('structures') or {}).get('starts') or {}
                    for sid, s in st.items():
                        if not isinstance(s, dict) or s.get('id') == 'INVALID':
                            continue
                        bbs = [c.get('BB') for c in s.get('Children', []) if isinstance(c, dict) and c.get('BB')]
                        starts.append([sid, bbs])
                    att = sorted((nbt.get('neoforge:attachments') or {}).keys())
                    res.append([cx, cz, nbt.get('InhabitedTime', 0), nbt.get('Status', '?'), bes, starts, att,
                                len(payload)])
                elif kind == 'entities':
                    nbt = anvil.parse_nbt(raw)
                    ents = {}
                    for e in nbt.get('Entities', []) or []:
                        eid = e.get('id', '?') if isinstance(e, dict) else '?'
                        ents[eid] = ents.get(eid, 0) + 1
                    res.append([cx, cz, ents, len(payload)])
                else:
                    res.append([cx, cz, len(payload)])
            except Exception as e:  # noqa: BLE001
                res.append([cx, cz, 'ERROR %s' % e])
    except Exception as e:  # noqa: BLE001
        return path, kind, 'ERROR %s' % e
    return path, kind, res


def cmd_scan(world, out, workers=6):
    jobs = []
    meta = {}
    for name, base in dims(world):
        meta[name] = base
        for kind in ('region', 'entities', 'poi'):
            dd = os.path.join(base, kind)
            if os.path.isdir(dd):
                for f in sorted(os.listdir(dd)):
                    if f.endswith('.mca'):
                        jobs.append((os.path.join(dd, f), kind))
    result = {'dims': meta, 'files': {}}
    with ProcessPoolExecutor(workers) as ex:
        for path, kind, res in ex.map(scan_region, jobs, chunksize=2):
            result['files'][path] = {'kind': kind, 'chunks': res}
    with open(out, 'w') as f:
        json.dump(result, f)


def is_tech(bid):
    ns = bid.split(':', 1)[0]
    return any(ns == t or (t.endswith('_') and ns.startswith(t)) for t in TECH_NS) or ns.startswith('create')


def is_village(sid):
    return sid.startswith('minecraft:village_') or sid == 'ribbits:ribbit_village'


def cmd_plan(scan, plan, inhabited=24000, margin=4):
    s = json.load(open(scan))
    per_dim = {}
    reasons = {}
    for path, info in s['files'].items():
        dim = next(n for n, b in s['dims'].items() if os.path.dirname(os.path.dirname(path)) == b)
        pd = per_dim.setdefault(dim, {'chunks': {}, 'ents': {}, 'bytes': {'region': 0, 'entities': 0, 'poi': 0},
                                      'files': {'region': 0, 'entities': 0, 'poi': 0}})
        pd['files'][info['kind']] += 1
        pd['bytes'][info['kind']] += os.path.getsize(path)
        if isinstance(info['chunks'], str):
            continue
        for c in info['chunks']:
            if isinstance(c[2], str):
                continue
            if info['kind'] == 'region':
                pd['chunks'][(c[0], c[1])] = c
            elif info['kind'] == 'entities':
                pd['ents'][(c[0], c[1])] = c[2]
    out = {}
    for dim, pd in per_dim.items():
        keep = {}
        for (cx, cz), c in pd['chunks'].items():
            it, status, bes, starts = c[2], c[3], c[4], c[5]
            why = []
            if it >= inhabited:
                why.append('inhabited')
            if any(is_tech(b) for b in bes):
                why.append('tech')
            for sid, bbs in starts:
                if is_village(sid):
                    why.append('village')
                    for bb in bbs:
                        for x in range(bb[0] >> 4, (bb[3] >> 4) + 1):
                            for z in range(bb[2] >> 4, (bb[5] >> 4) + 1):
                                keep.setdefault((x, z), set()).add('village-piece')
            if why:
                keep.setdefault((cx, cz), set()).update(why)
        for (cx, cz), ents in pd['ents'].items():
            if any(e.startswith('create:') and 'contraption' in e for e in ents) or \
                    any(e.split(':')[0] in ('sable', 'aeronautics', 'simulated') for e in ents):
                keep.setdefault((cx, cz), set()).add('contraption')
        core = set(keep)
        ring = set()
        vmargin = int(os.environ.get('MARGIN_VILLAGE', margin))
        for (cx, cz) in core:
            m = vmargin if keep[(cx, cz)] <= {'village', 'village-piece'} else margin
            for dx in range(-m, m + 1):
                for dz in range(-m, m + 1):
                    p = (cx + dx, cz + dz)
                    if p not in core:
                        ring.add(p)
        kept = {p for p in core | ring if p in pd['chunks']}
        r = {}
        for p, w in keep.items():
            for x in w:
                r[x] = r.get(x, 0) + 1
        reasons[dim] = r
        out[dim] = {'keep': sorted([list(p) for p in kept]), 'core': len(core), 'ring': len(ring),
                    'kept_existing': len(kept), 'total_chunks': len(pd['chunks']),
                    'bytes_before': pd['bytes'], 'files_before': pd['files'], 'reasons': r}
    json.dump(out, open(plan, 'w'))
    for dim, o in out.items():
        print('%-28s chunks %7d -> %6d (core %d, ring %d)  bytes %s  reasons %s' % (
            dim, o['total_chunks'], o['kept_existing'], o['core'], o['ring'],
            {k: '%.0fM' % (v / 1e6) for k, v in o['bytes_before'].items()}, o['reasons']))


def cmd_write(world, plan, dest):
    p = json.load(open(plan))
    s_dims = dict(dims(world))
    if os.path.exists(dest):
        raise SystemExit('destination exists: %s' % dest)
    prune_dirs = set()
    for name, base in s_dims.items():
        for kind in ('region', 'entities', 'poi'):
            prune_dirs.add(os.path.normpath(os.path.join(base, kind)))

    def ignore(d, names):
        return [n for n in names if os.path.normpath(os.path.join(d, n)) in prune_dirs
                or n.startswith('DistantHorizons.sqlite') or n == 'session.lock']
    shutil.copytree(world, dest, ignore=ignore)
    totals = {}
    for name, base in s_dims.items():
        keep = {tuple(x) for x in p.get(name, {}).get('keep', [])}
        rel = os.path.relpath(base, world)
        for kind in ('region', 'entities', 'poi'):
            src = os.path.join(base, kind)
            if not os.path.isdir(src):
                continue
            dst = os.path.join(dest, rel, kind)
            os.makedirs(dst, exist_ok=True)
            for f in sorted(os.listdir(src)):
                if not f.endswith('.mca'):
                    continue
                chunks = []
                for i, cx, cz, ts, payload, comp, ext in anvil.iter_region(os.path.join(src, f)):
                    if (cx, cz) in keep:
                        chunks.append((i, ts, comp, payload))
                        if ext:
                            shutil.copy2(os.path.join(src, 'c.%d.%d.mcc' % (cx, cz)), dst)
                if chunks:
                    anvil.write_region(os.path.join(dst, f), chunks)
                    totals[(name, kind)] = totals.get((name, kind), 0) + os.path.getsize(os.path.join(dst, f))
    for k, v in sorted(totals.items()):
        print('%-28s %-9s %8.1f MB' % (k[0], k[1], v / 1e6))


if __name__ == '__main__':
    cmd = sys.argv[1]
    if cmd == 'scan':
        cmd_scan(sys.argv[2], sys.argv[3], int(os.environ.get('WORKERS', '6')))
    elif cmd == 'plan':
        cmd_plan(sys.argv[2], sys.argv[3], int(os.environ.get('INHABITED', '24000')),
                 int(os.environ.get('MARGIN', '4')))
    elif cmd == 'write':
        cmd_write(sys.argv[2], sys.argv[3], sys.argv[4])
