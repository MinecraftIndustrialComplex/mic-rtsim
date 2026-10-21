#!/usr/bin/env python3
"""Audit a downloaded Minecraft world without booting it.

Run with:  python3 -I audit_world.py <world_root_or_parent> <out_json> [workers]

Reads only: level.dat, data/*.dat, region/*.mca, entities/*.mca, poi sizes.
Never executes anything from the world. Pure data parsing with a skipping NBT
reader (only keeps the keys we ask for) plus numpy palette unpacking for exact
block counts.
"""
import sys, os, json, struct, zlib, gzip, math, time, collections, re
from concurrent.futures import ProcessPoolExecutor
import numpy as np

# ------------------------------------------------------------------ NBT skipping reader
_S = struct.Struct
_i16 = _S('>h').unpack_from
_u16 = _S('>H').unpack_from
_i32 = _S('>i').unpack_from
_i64 = _S('>q').unpack_from
_f32 = _S('>f').unpack_from
_f64 = _S('>d').unpack_from
_PRIM = {1: 1, 2: 2, 3: 4, 4: 8, 5: 4, 6: 8}


def skip_payload(b, p, t):
    if t in _PRIM:
        return p + _PRIM[t]
    if t == 7:
        n = _i32(b, p)[0]; return p + 4 + n
    if t == 8:
        n = _u16(b, p)[0]; return p + 2 + n
    if t == 9:
        it = b[p]; n = _i32(b, p + 1)[0]; p += 5
        if it in _PRIM:
            return p + n * _PRIM[it]
        for _ in range(n):
            p = skip_payload(b, p, it)
        return p
    if t == 10:
        while True:
            tt = b[p]; p += 1
            if tt == 0:
                return p
            nl = _u16(b, p)[0]; p += 2 + nl
            p = skip_payload(b, p, tt)
    if t == 11:
        n = _i32(b, p)[0]; return p + 4 + 4 * n
    if t == 12:
        n = _i32(b, p)[0]; return p + 4 + 8 * n
    raise ValueError("bad tag %d" % t)


def read_full(b, p, t):
    """Fully parse a payload (small things only)."""
    if t == 1: return b[p] if b[p] < 128 else b[p] - 256, p + 1
    if t == 2: return _i16(b, p)[0], p + 2
    if t == 3: return _i32(b, p)[0], p + 4
    if t == 4: return _i64(b, p)[0], p + 8
    if t == 5: return _f32(b, p)[0], p + 4
    if t == 6: return _f64(b, p)[0], p + 8
    if t == 7:
        n = _i32(b, p)[0]; return bytes(b[p + 4:p + 4 + n]), p + 4 + n
    if t == 8:
        n = _u16(b, p)[0]; return bytes(b[p + 2:p + 2 + n]).decode('utf-8', 'replace'), p + 2 + n
    if t == 9:
        it = b[p]; n = _i32(b, p + 1)[0]; p += 5
        out = []
        for _ in range(n):
            v, p = read_full(b, p, it); out.append(v)
        return out, p
    if t == 10:
        out = {}
        while True:
            tt = b[p]; p += 1
            if tt == 0: return out, p
            nl = _u16(b, p)[0]; k = bytes(b[p + 2:p + 2 + nl]).decode('utf-8', 'replace'); p += 2 + nl
            out[k], p = read_full(b, p, tt)
    if t == 11:
        n = _i32(b, p)[0]; return list(struct.unpack_from('>%di' % n, b, p + 4)), p + 4 + 4 * n
    if t == 12:
        n = _i32(b, p)[0]; return bytes(b[p + 4:p + 4 + 8 * n]), p + 4 + 8 * n
    raise ValueError("bad tag %d" % t)


def extract(b, p, t, spec):
    """spec: True -> full value; 'len' -> length of list/array; 'raw' -> bytes of long array;
    dict -> filter compound (or each element of a list of compounds); None -> skip."""
    if spec is None:
        return None, skip_payload(b, p, t)
    if spec is True:
        return read_full(b, p, t)
    if spec == 'len':
        if t == 9:
            n = _i32(b, p + 1)[0]; return n, skip_payload(b, p, t)
        if t in (7, 11, 12):
            return _i32(b, p)[0], skip_payload(b, p, t)
        return None, skip_payload(b, p, t)
    if spec == 'raw':
        if t == 12:
            n = _i32(b, p)[0]; return b[p + 4:p + 4 + 8 * n], p + 4 + 8 * n
        return None, skip_payload(b, p, t)
    if isinstance(spec, dict):
        if t == 10:
            out = {}
            while True:
                tt = b[p]; p += 1
                if tt == 0: return out, p
                nl = _u16(b, p)[0]; k = bytes(b[p + 2:p + 2 + nl]).decode('utf-8', 'replace'); p += 2 + nl
                s = spec.get(k)
                if s is None:
                    p = skip_payload(b, p, tt)
                else:
                    out[k], p = extract(b, p, tt, s)
        if t == 9:
            it = b[p]; n = _i32(b, p + 1)[0]; p += 5
            out = []
            for _ in range(n):
                v, p = extract(b, p, it, spec); out.append(v)
            return out, p
    return None, skip_payload(b, p, t)


def parse_root(b, spec):
    t = b[0]
    nl = _u16(b, 1)[0]
    p = 3 + nl
    v, _ = extract(b, p, t, spec)
    return v


# ------------------------------------------------------------------ chunk extraction
CHUNK_SPEC = {
    'DataVersion': True, 'InhabitedTime': True, 'xPos': True, 'zPos': True, 'Status': True,
    'block_entities': {'id': True},
    'sections': {'Y': True, 'block_states': {'palette': {'Name': True}, 'data': 'raw'}},
    # pre-1.18
    'Level': {'InhabitedTime': True, 'xPos': True, 'zPos': True, 'Status': True,
              'TileEntities': {'id': True},
              'Sections': {'Y': True, 'Palette': {'Name': True}, 'BlockStates': 'raw'}},
}

ENT_SPEC = {
    'DataVersion': True, 'Position': True,
    'Entities': {'id': True, 'Contraption': {'Blocks': {'BlockList': 'len'}, 'Actors': 'len', 'Anchor': True}},
    'Level': {'Entities': {'id': True}},
}


def read_chunks(path):
    """Yield (cx_in_region, cz_in_region, compressed_type, raw bytes) for each stored chunk."""
    with open(path, 'rb') as f:
        data = f.read()
    if len(data) < 8192:
        return
    for i in range(1024):
        off = int.from_bytes(data[i * 4:i * 4 + 3], 'big')
        cnt = data[i * 4 + 3]
        if off == 0 or cnt == 0:
            continue
        start = off * 4096
        if start + 5 > len(data):
            continue
        ln = struct.unpack_from('>I', data, start)[0]
        ct = data[start + 4]
        payload = data[start + 5:start + 4 + ln]
        yield i & 31, i >> 5, ct, payload


def decomp(ct, payload):
    if ct == 2: return zlib.decompress(payload)
    if ct == 1: return gzip.decompress(payload)
    if ct == 3: return payload
    if ct == 4:
        try:
            import lz4.block
        except Exception:
            raise ValueError('lz4 unavailable')
        # Minecraft lz4 uses a custom frame; unsupported here
        raise ValueError('lz4 chunk')
    raise ValueError('compression %d' % ct)


def unpack_indices(raw, nvals, bits):
    arr = np.frombuffer(raw, dtype='>u8')
    epl = 64 // bits
    mask = np.uint64((1 << bits) - 1)
    shifts = (np.arange(epl, dtype=np.uint64) * np.uint64(bits))
    out = ((arr[:, None] >> shifts[None, :]) & mask).reshape(-1)
    return out[:nvals]


def scan_region_file(args):
    kind, path = args  # kind: 'region' or 'entities'
    res = {'path': path, 'kind': kind, 'chunks': 0, 'errors': 0, 'inhabited_chunks': 0,
           'inhabited_ticks': 0, 'dataversions': collections.Counter(),
           'be': collections.Counter(), 'blocks': collections.Counter(),
           'ent': collections.Counter(), 'contraptions': [],
           'chunk_inhabited': []}
    try:
        for cx, cz, ct, payload in read_chunks(path):
            try:
                b = decomp(ct, payload)
                if kind == 'region':
                    root = parse_root(b, CHUNK_SPEC)
                    lv = root.get('Level') if root.get('Level') else None
                    if lv:
                        bes = lv.get('TileEntities') or []
                        inh = lv.get('InhabitedTime') or 0
                        secs = []
                        for s in (lv.get('Sections') or []):
                            if s.get('Palette') and s.get('BlockStates') is not None:
                                secs.append(([e['Name'] for e in s['Palette']], s['BlockStates'], True))
                    else:
                        bes = root.get('block_entities') or []
                        inh = root.get('InhabitedTime') or 0
                        secs = []
                        for s in (root.get('sections') or []):
                            bs = s.get('block_states')
                            if not bs: continue
                            pal = [e['Name'] for e in (bs.get('palette') or [])]
                            secs.append((pal, bs.get('data'), False))
                    res['dataversions'][root.get('DataVersion', 0)] += 1
                    res['chunks'] += 1
                    res['inhabited_ticks'] += inh
                    if inh > 1200:
                        res['inhabited_chunks'] += 1
                    res['chunk_inhabited'].append((cx, cz, inh, len(bes)))
                    for e in bes:
                        res['be'][e.get('id', '?')] += 1
                    for pal, data, legacy in secs:
                        if len(pal) == 1:
                            res['blocks'][pal[0]] += 4096
                            continue
                        if data is None or len(data) == 0:
                            continue
                        bits = max(4, (len(pal) - 1).bit_length())
                        if legacy:
                            # 1.16 and earlier: longs may span entries only before 1.16; we assume DataVersion >= 2529 (1.16)
                            pass
                        idx = unpack_indices(data, 4096, bits)
                        cnt = np.bincount(idx.astype(np.int64), minlength=len(pal))
                        for i, name in enumerate(pal):
                            c = int(cnt[i]) if i < len(cnt) else 0
                            if c:
                                res['blocks'][name] += c
                else:
                    root = parse_root(b, ENT_SPEC)
                    ents = root.get('Entities')
                    if ents is None and root.get('Level'):
                        ents = root['Level'].get('Entities')
                    res['dataversions'][root.get('DataVersion', 0)] += 1
                    res['chunks'] += 1
                    for e in (ents or []):
                        eid = e.get('id', '?')
                        res['ent'][eid] += 1
                        if 'Contraption' in e and e['Contraption']:
                            c = e['Contraption']
                            res['contraptions'].append((eid, (c.get('Blocks') or {}).get('BlockList') or 0,
                                                        c.get('Actors') or 0))
            except Exception as ex:
                res['errors'] += 1
                res.setdefault('err_sample', str(ex)[:200])
    except Exception as ex:
        res['errors'] += 1
        res['err_sample'] = 'file: ' + str(ex)[:200]
    return res


# ------------------------------------------------------------------ level.dat + create_tracks
def load_nbt_file(path):
    import nbtlib
    return nbtlib.load(path)


def to_py(x, depth=0, maxlist=400):
    import nbtlib
    if isinstance(x, dict):
        return {k: to_py(v, depth + 1, maxlist) for k, v in x.items()}
    if isinstance(x, (list,)) and not isinstance(x, (nbtlib.ByteArray, nbtlib.IntArray, nbtlib.LongArray)):
        return [to_py(v, depth + 1, maxlist) for v in x[:maxlist]]
    if isinstance(x, (nbtlib.IntArray, nbtlib.LongArray, nbtlib.ByteArray)):
        return [int(v) for v in x[:64]]
    if isinstance(x, nbtlib.String): return str(x)
    if isinstance(x, (nbtlib.Byte, nbtlib.Short, nbtlib.Int, nbtlib.Long)): return int(x)
    if isinstance(x, (nbtlib.Float, nbtlib.Double)): return float(x)
    return str(x)


def audit_level(world):
    out = {}
    lp = os.path.join(world, 'level.dat')
    if not os.path.exists(lp):
        return {'error': 'no level.dat'}
    f = load_nbt_file(lp)
    root = f.get('', f)
    data = root.get('Data', root)
    out['top_keys'] = list(root.keys())
    out['data_keys'] = list(data.keys())
    for k in ('LevelName', 'DataVersion', 'version', 'WasModded', 'GameType', 'Hardcore', 'DayTime', 'Time', 'LastPlayed'):
        if k in data:
            out[k] = to_py(data[k])
    if 'Version' in data:
        out['Version'] = to_py(data['Version'])
    if 'ServerBrands' in data:
        out['ServerBrands'] = to_py(data['ServerBrands'])
    if 'DataPacks' in data:
        out['DataPacks'] = to_py(data['DataPacks'])
    if 'WorldGenSettings' in data:
        wg = data['WorldGenSettings']
        out['seed'] = int(wg.get('seed', 0)) if 'seed' in wg else None
        dims = wg.get('dimensions')
        if dims is not None:
            out['dimensions'] = list(dims.keys())
    # Forge
    fml = root.get('fml')
    if fml is not None:
        out['fml_keys'] = list(fml.keys())
        lml = fml.get('loadingModList') or fml.get('LoadingModList')
        if lml is not None:
            out['forge_mod_list'] = [(str(m.get('ModId')), str(m.get('ModVersion'))) for m in lml]
        regs = fml.get('registries')
        if regs is not None:
            ns = collections.OrderedDict()
            for rname, r in regs.items():
                ids = r.get('ids') or []
                for e in ids:
                    n = str(e.get('K', ''))
                    if ':' in n:
                        s = n.split(':', 1)[0]
                        ns.setdefault(s, {}).setdefault(str(rname), 0)
                        ns[s][str(rname)] += 1
            out['forge_registry_namespaces'] = {k: v for k, v in ns.items()}
    # NeoForge / others
    for k in root.keys():
        if k not in ('Data', 'fml') and 'forge' in k.lower() or 'fabric' in k.lower():
            out.setdefault('loader_keys', {})[k] = to_py(root[k], maxlist=50)
    return out


def _xyz(a):
    if 'X' in a:
        return int(a['X']), int(a['Y']), int(a['Z'])
    p = a['Pos']
    return int(p[0]), int(p[1]), int(p[2])


def edge_len(a, b):
    ax, ay, az = _xyz(a); bx, by, bz = _xyz(b)
    return math.sqrt((ax - bx) ** 2 + (ay - by) ** 2 + (az - bz) ** 2)


def audit_tracks(world):
    cands = [os.path.join(world, 'data', n) for n in ('create_tracks.dat',)]
    out = {}
    for c in cands:
        if not os.path.exists(c):
            continue
        f = load_nbt_file(c)
        root = f.get('', f)
        data = root.get('data', root)
        out['file_bytes'] = os.path.getsize(c)
        out['DataVersion'] = int(root['DataVersion']) if 'DataVersion' in root else None
        trains = data.get('Trains', [])
        out['trains'] = len(trains)
        carriages = 0; dbl = 0; sched = 0; sched_entries = 0; moving = 0; derailed = 0; with_passengers = 0
        speed_sum = 0.0
        for t in trains:
            cs = t.get('Carriages', [])
            carriages += len(cs)
            if int(t.get('Derailed', 0)): derailed += 1
            rt = t.get('Runtime') or {}
            sch = rt.get('Schedule')
            if sch is not None and 'Entries' in sch:
                sched += 1; sched_entries += len(sch['Entries'])
            if abs(float(t.get('Speed', 0))) > 0.001: moving += 1
            for c in cs:
                if c.get('Passengers'): with_passengers += 1
        out.update({'carriages': carriages, 'trains_with_schedule': sched, 'schedule_entries': sched_entries,
                    'trains_moving_at_save': moving, 'trains_derailed': derailed,
                    'carriages_with_passengers': with_passengers})
        bogeys = collections.Counter()
        for t in trains:
            for c in t.get('Carriages', []):
                for key in ('FirstBogey', 'SecondBogey'):
                    if key in c:
                        bogeys[str(c[key].get('Type'))] += 1
        out['bogey_types'] = dict(bogeys)
        graphs = data.get('RailGraphs', [])
        out['graphs'] = len(graphs)
        n_nodes = n_edges = n_st = n_sig = n_obs = 0
        total_len = 0.0
        mats = collections.Counter()
        curved = 0
        per_graph = []
        for g in graphs:
            nodes = g.get('Nodes', [])
            pts = g.get('Points', {})
            st = len(pts.get('create:station', [])) if pts else 0
            sg = len(pts.get('create:signal', [])) if pts else 0
            ob = len(pts.get('create:observer', [])) if pts else 0
            n_st += st; n_sig += sg; n_obs += ob
            n_nodes += len(nodes)
            ge = 0; gl = 0.0
            seen = set()
            for i, n in enumerate(nodes):
                for cn in n.get('Connections', []):
                    j = int(cn['To'])
                    key = (min(i, j), max(i, j))
                    if key in seen: continue
                    seen.add(key)
                    ge += 1
                    ed = cn.get('EdgeData', {})
                    mats[str(ed.get('Material', 'create:andesite'))] += 1
                    if 'Turn' in ed or 'TurnData' in ed:
                        curved += 1
                    gl += edge_len(n['Location'], nodes[j]['Location']) if j < len(nodes) else 0
            n_edges += ge; total_len += gl
            per_graph.append({'nodes': len(nodes), 'edges': ge, 'stations': st, 'signals': sg, 'observers': ob,
                              'chord_length_blocks': round(gl)})
        out.update({'nodes': n_nodes, 'edges': n_edges, 'stations': n_st, 'signals': n_sig, 'observers': n_obs,
                    'track_chord_length_blocks': round(total_len), 'edge_materials': dict(mats),
                    'curved_edges': curved, 'per_graph': sorted(per_graph, key=lambda x: -x['edges'])[:25]})
        out['signal_blocks'] = len(data.get('SignalBlocks', []))
        out['signal_edge_groups'] = len(data.get('SignalEdgeGroups', [])) if 'SignalEdgeGroups' in data else None
        out['dimension_palette'] = [str(d.get('Id')) for d in data.get('DimensionPalette', [])] if 'DimensionPalette' in data else None
        out['other_keys'] = list(data.keys())
    return out


def find_world_root(base):
    best = None
    for dirpath, dirnames, filenames in os.walk(base):
        depth = dirpath[len(base):].count(os.sep)
        if 'level.dat' in filenames:
            return dirpath
        if depth >= 4:
            dirnames[:] = []
    return best


def list_dimension_dirs(world):
    dims = []
    if os.path.isdir(os.path.join(world, 'region')):
        dims.append(('minecraft:overworld', world))
    if os.path.isdir(os.path.join(world, 'DIM-1')):
        dims.append(('minecraft:the_nether', os.path.join(world, 'DIM-1')))
    if os.path.isdir(os.path.join(world, 'DIM1')):
        dims.append(('minecraft:the_end', os.path.join(world, 'DIM1')))
    d = os.path.join(world, 'dimensions')
    if os.path.isdir(d):
        for ns in sorted(os.listdir(d)):
            for nm in sorted(os.listdir(os.path.join(d, ns))):
                p = os.path.join(d, ns, nm)
                if os.path.isdir(os.path.join(p, 'region')):
                    dims.append(('%s:%s' % (ns, nm), p))
    for x in sorted(os.listdir(world)):
        if x.startswith('DIM') and x not in ('DIM-1', 'DIM1') and os.path.isdir(os.path.join(world, x, 'region')):
            dims.append((x, os.path.join(world, x)))
    return dims


def dir_size(p):
    t = 0
    for dp, dn, fn in os.walk(p):
        for f in fn:
            try:
                t += os.path.getsize(os.path.join(dp, f))
            except OSError:
                pass
    return t


def main():
    base = sys.argv[1]
    outp = sys.argv[2]
    workers = int(sys.argv[3]) if len(sys.argv) > 3 else 6
    world = find_world_root(base)
    if not world:
        print('no level.dat under', base); sys.exit(1)
    t0 = time.time()
    rep = {'base': base, 'world_root': world, 'total_bytes': dir_size(base)}
    rep['level'] = audit_level(world)
    rep['tracks'] = audit_tracks(world)
    dd = os.path.join(world, 'data')
    rep['data_files'] = sorted([(n, os.path.getsize(os.path.join(dd, n))) for n in os.listdir(dd)]) if os.path.isdir(dd) else []
    pd = os.path.join(world, 'playerdata')
    rep['playerdata_files'] = len([n for n in os.listdir(pd) if n.endswith('.dat')]) if os.path.isdir(pd) else 0
    dps = os.path.join(world, 'datapacks')
    rep['datapacks'] = sorted(os.listdir(dps)) if os.path.isdir(dps) else []
    rep['top_level_entries'] = sorted(os.listdir(world))
    tasks = []
    dims = list_dimension_dirs(world)
    for name, dpath in dims:
        for sub, kind in (('region', 'region'), ('entities', 'entities')):
            sd = os.path.join(dpath, sub)
            if os.path.isdir(sd):
                for fn in sorted(os.listdir(sd)):
                    if fn.endswith('.mca'):
                        tasks.append((name, kind, os.path.join(sd, fn)))
    results = collections.defaultdict(lambda: {'region_files': 0, 'region_bytes': 0, 'chunks': 0, 'errors': 0,
                                               'inhabited_chunks': 0, 'inhabited_ticks': 0,
                                               'dataversions': collections.Counter(), 'be': collections.Counter(),
                                               'blocks': collections.Counter(), 'ent': collections.Counter(),
                                               'contraptions': [], 'regions': {}, 'err_sample': None})
    with ProcessPoolExecutor(max_workers=workers) as ex:
        futs = []
        for name, kind, path in tasks:
            futs.append((name, kind, path, ex.submit(scan_region_file, (kind, path))))
        for name, kind, path, fu in futs:
            r = fu.result()
            R = results[name]
            if kind == 'region':
                R['region_files'] += 1
                R['region_bytes'] += os.path.getsize(path)
                R['chunks'] += r['chunks']
                R['inhabited_chunks'] += r['inhabited_chunks']
                R['inhabited_ticks'] += r['inhabited_ticks']
                R['dataversions'].update(r['dataversions'])
                R['be'].update(r['be'])
                R['blocks'].update(r['blocks'])
                m = re.match(r'r\.(-?\d+)\.(-?\d+)\.mca', os.path.basename(path))
                if m:
                    rx, rz = int(m.group(1)), int(m.group(2))
                    nbe = sum(r['be'].values())
                    ncr = sum(v for k, v in r['be'].items() if k.startswith('create:') or k.startswith('railways:') or 'create' in k.split(':')[0])
                    R['regions'][f'{rx},{rz}'] = {'chunks': r['chunks'], 'be': nbe, 'create_be': ncr,
                                                   'inhabited_chunks': r['inhabited_chunks'],
                                                   'bytes': os.path.getsize(path)}
            else:
                R['ent'].update(r['ent'])
                R['contraptions'].extend(r['contraptions'])
            R['errors'] += r['errors']
            if r.get('err_sample') and not R['err_sample']:
                R['err_sample'] = r['err_sample']
    rep['dimensions'] = {}
    for name, R in results.items():
        blk = R['blocks']
        ns_blocks = collections.Counter()
        for k, v in blk.items():
            ns_blocks[k.split(':', 1)[0]] += v
        be_ns = collections.Counter()
        for k, v in R['be'].items():
            be_ns[k.split(':', 1)[0]] += v
        ent_ns = collections.Counter()
        for k, v in R['ent'].items():
            ent_ns[k.split(':', 1)[0]] += v
        cons = R['contraptions']
        rep['dimensions'][name] = {
            'region_files': R['region_files'], 'region_bytes': R['region_bytes'], 'chunks': R['chunks'],
            'errors': R['errors'], 'err_sample': R['err_sample'],
            'inhabited_chunks_gt_1200': R['inhabited_chunks'], 'inhabited_ticks': R['inhabited_ticks'],
            'dataversions': dict(R['dataversions']),
            'block_entities_total': sum(R['be'].values()),
            'block_entities_by_type': dict(R['be'].most_common(120)),
            'block_entities_by_namespace': dict(be_ns.most_common()),
            'blocks_by_namespace': dict(ns_blocks.most_common()),
            'blocks_top_non_vanilla': dict([(k, v) for k, v in blk.most_common() if not k.startswith('minecraft:')][:150]),
            'blocks_distinct_ids': len(blk),
            'entities_by_type': dict(R['ent'].most_common(80)),
            'entities_by_namespace': dict(ent_ns.most_common()),
            'contraption_entities': len(cons),
            'contraption_blocks_total': sum(c[1] for c in cons),
            'contraption_biggest': sorted(cons, key=lambda c: -c[1])[:12],
            'regions': R['regions'],
        }
    rep['elapsed_s'] = round(time.time() - t0, 1)
    with open(outp, 'w') as f:
        json.dump(rep, f, indent=1, default=str)
    print('wrote', outp, 'in', rep['elapsed_s'], 's')


if __name__ == '__main__':
    main()
