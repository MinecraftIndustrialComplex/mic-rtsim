import json, sys
r = json.load(open(sys.argv[1]))
L = r['level']
print("WORLD", r['world_root'], "bytes", r['total_bytes'])
print("level:", {k: L.get(k) for k in ('LevelName','DataVersion','WasModded','ServerBrands','Hardcore','Time','DayTime')})
print("Version:", L.get('Version'))
print("top_keys:", L.get('top_keys'), "dims:", L.get('dimensions'))
if 'forge_mod_list' in L: print("forge_mod_list:", len(L['forge_mod_list']), L['forge_mod_list'][:60])
if 'forge_registry_namespaces' in L: print("registry ns:", {k: sum(v.values()) for k,v in L['forge_registry_namespaces'].items()})
if 'loader_keys' in L: print("loader_keys:", str(L['loader_keys'])[:1500])
print("data_files:", r['data_files'], "players:", r['playerdata_files'], "datapacks:", r['datapacks'])
T = r['tracks']
if T:
    print("TRACKS:", {k: v for k, v in T.items() if k not in ('per_graph','other_keys')})
    print(" per_graph top:", T['per_graph'][:5])
for d, D in r['dimensions'].items():
    print("DIM", d, "regions", D['region_files'], "MB", round(D['region_bytes']/1e6,1), "chunks", D['chunks'], "errors", D['errors'], D['err_sample'], "inhab>1200:", D['inhabited_chunks_gt_1200'])
    print("  DV", D['dataversions'])
    print("  BE total", D['block_entities_total'], "by ns", dict(list(D['block_entities_by_namespace'].items())[:15]))
    print("  BE types", dict(list(D['block_entities_by_type'].items())[:45]))
    print("  blocks ns", dict(list(D['blocks_by_namespace'].items())[:15]))
    print("  top non-vanilla blocks", dict(list(D['blocks_top_non_vanilla'].items())[:40]))
    print("  entities", D['entities_by_namespace'], dict(list(D['entities_by_type'].items())[:25]))
    print("  contraptions", D['contraption_entities'], "blocks", D['contraption_blocks_total'], D['contraption_biggest'][:6])
