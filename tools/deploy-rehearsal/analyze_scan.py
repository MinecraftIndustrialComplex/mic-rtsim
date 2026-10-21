import json, sys, os, collections
s = json.load(open(sys.argv[1]))
def dim_of(path):
    return next(n for n, b in s['dims'].items() if os.path.dirname(os.path.dirname(path)) == b)
for dim in s['dims']:
    chunks = []; ents = {}; errs = 0
    for path, info in s['files'].items():
        if dim_of(path) != dim or isinstance(info['chunks'], str): continue
        for c in info['chunks']:
            if isinstance(c[2], str): errs += 1; continue
            if info['kind'] == 'region': chunks.append(c)
            elif info['kind'] == 'entities': ents[(c[0], c[1])] = c[2]
    print('=====', dim, len(chunks), 'chunks', errs, 'errors')
    if not chunks: continue
    its = sorted(c[2] for c in chunks)
    for th in (0, 1200, 6000, 24000, 72000, 288000, 1440000):
        print('  InhabitedTime >= %8d: %6d chunks' % (th, sum(1 for x in its if x >= th)))
    st = collections.Counter(c[3] for c in chunks); print('  status', dict(st.most_common(6)))
    be = collections.Counter(); beh = collections.Counter()
    for c in chunks:
        for k, v in c[4].items():
            be[k] += v
            if c[2] < 24000: beh[k.split(':')[0]] += v
    print('  top block entities', be.most_common(25))
    print('  BE namespaces in chunks with IT<24000', beh.most_common(30))
    sc = collections.Counter(); 
    for c in chunks:
        for sid, bbs in c[5]: sc[sid] += 1
    print('  structure starts', [x for x in sc.most_common(60) if 'village' in x[0] or 'ribbit' in x[0] or 'lair' in x[0] or 'dragon' in x[0]][:30])
    at = collections.Counter(a for c in chunks for a in c[6]); print('  attachments', at.most_common(10))
    ec = collections.Counter()
    for k, v in ents.items():
        for e, n in v.items(): ec[e] += n
    print('  entities', ec.most_common(40))
    top = sorted(chunks, key=lambda c: -c[2])[:12]
    print('  top inhabited', [(c[0]*16, c[1]*16, c[2]) for c in top])
