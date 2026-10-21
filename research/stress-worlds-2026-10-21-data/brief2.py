import json, sys, collections
r = json.load(open(sys.argv[1]))
tot=collections.Counter(); chunks=0; be=0; cr=0
for d,D in r['dimensions'].items():
    chunks+=D['chunks']; be+=D['block_entities_total']; cr+=D['block_entities_by_namespace'].get('create',0)
    for k,v in D['block_entities_by_type'].items(): tot[k]+=v
print("chunks",chunks,"be",be,"create ns",cr, "dims", len(r['dimensions']))
for k in ['create:packager','create:frogport','create:item_vault','create:stock_ticker','create:redstone_requester','create:repackager','create:package_poster','create:stock_link','create:train_station','create:track','create:fake_track','create:belt','create:fluid_tank','create:mechanical_crafter','create:deployer','create:drill','create:steam_engine','create:signal','create:track_observer','create:schedule','create:display_link','create:mechanical_bearing','create:chassis','create:gantry']:
    print(k, tot.get(k))
print({k:v for k,v in tot.items() if k.startswith('create:') and ('pack' in k or 'frog' in k or 'stock' in k or 'request' in k or 'track' in k or 'station' in k or 'signal' in k or 'logist' in k)})
ce=collections.Counter(); cb=collections.Counter(); big=[]
for d,D in r['dimensions'].items():
    ce.update(D.get('contraption_entities') or {}) if isinstance(D.get('contraption_entities'),dict) else None
    print(d,'contraption_entities',D.get('contraption_entities'),'blocks',D.get('contraption_blocks_total'),'biggest',str(D.get('contraption_biggest'))[:200])
ent=collections.Counter()
for d,D in r['dimensions'].items():
    for k,v in (D.get('entities_by_type') or {}).items(): ent[k]+=v
print("entities total",sum(ent.values()), ent.most_common(14))
ml=r['level'].get('forge_mod_list') or []
print([m for m in ml if any(s in str(m).lower() for s in ('create','ftb','chunk','mowzie','iceandfire','ice_and'))][:60])
print(str(ml[:3]))
