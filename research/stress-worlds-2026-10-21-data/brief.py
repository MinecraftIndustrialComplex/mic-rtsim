import json, sys
r = json.load(open(sys.argv[1]))
L = r['level']
print("bytes", r['total_bytes'], "dv", L.get('DataVersion'), "time", L.get('Time'), "players", r['playerdata_files'])
ml = L.get('forge_mod_list') or []
print("mods", len(ml))
T = r['tracks']
if T: print("TRACKS", {k: v for k, v in T.items() if k not in ('per_graph','other_keys')}); print(" graphs", T['per_graph'][:3])
print("data_files", [(n,s) for n,s in r['data_files'] if s>200000 or 'create' in n])
for d, D in r['dimensions'].items():
    print("DIM", d, "regions", D['region_files'], "MB", round(D['region_bytes']/1e6,1), "chunks", D['chunks'], "err", D['errors'], "inhab>1200", D['inhabited_chunks_gt_1200'])
    print("  BE total", D['block_entities_total'], "ns", dict(list(D['block_entities_by_namespace'].items())[:8]))
    print("  top types", dict(list(D['block_entities_by_type'].items())[:25]))
    print("  keys", [k for k in D.keys()][:40])
