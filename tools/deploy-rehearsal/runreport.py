"""Summarizes runsrv.py outputs: boot, MSPT with spreads, CPU, RSS, load, and the replies worth reading.

usage: python3 -I runreport.py OUT.json [OUT.json ...]          # one block per run
       python3 -I runreport.py --pair A.json B.json            # the two side by side, B minus A per sample
Spreads: mean, p10/p50/p90, min/max over the 5 s samples of the sampling window; overworld and Overall.
"""
import json
import statistics
import sys


def stats(xs):
    if not xs:
        return None
    s = sorted(xs)
    n = len(s)
    return {'n': n, 'mean': round(statistics.fmean(s), 2), 'sd': round(statistics.pstdev(s), 2),
            'p10': round(s[int(n * 0.1)], 2), 'p50': round(s[n // 2], 2), 'p90': round(s[min(n - 1, int(n * 0.9))], 2),
            'min': round(s[0], 2), 'max': round(s[-1], 2)}


def window(out):
    for st in out.get('steps', []):
        if 'samples' in st:
            return st
    return None


def series(st, dim):
    res = []
    for t, tps, ps, la in st['samples']:
        for k, v in tps.items():
            if k == dim or (dim == 'overworld' and k.lower().endswith('overworld')):
                res.append((t, v[0]))
                break
    return res


def summary(path):
    out = json.load(open(path))
    r = {'run': path.rsplit('/', 1)[-1], 'boot_wall_s': out.get('boot_wall_s'), 'done_s': out.get('done_s'),
         'error': out.get('error'), 'died': out.get('died')}
    st = window(out)
    if st:
        r['overall_ms'] = stats([v for _, v in series(st, 'Overall')])
        r['overworld_ms'] = stats([v for _, v in series(st, 'overworld')])
        r['cpu_cores'] = st.get('cpu_cores')
        r['rss_mb_max'] = st.get('rss_mb_max')
        r['loadavg_mean'] = st.get('loadavg_mean')
        dims = {}
        for _, tps, _, _ in st['samples']:
            for k, v in tps.items():
                dims.setdefault(k, []).append(v[0])
        r['dims_mean_ms'] = {k: round(statistics.fmean(v), 2) for k, v in sorted(dims.items(), key=lambda kv: -statistics.fmean(kv[1]))[:6]}
    errs = [(s['step'], s['error']) for s in out.get('steps', []) if 'error' in s]
    if errs:
        r['step_errors'] = errs
    stop = [s for s in out.get('steps', []) if 'stop' in s['step']]
    if stop:
        r['stop_s'], r['exit'] = stop[0].get('stop_s'), stop[0].get('exit')
    return r, out


def replies(out, prefixes=('rtsim', 'execute', 'forceload query')):
    for s in out.get('steps', []):
        c = s['step'].get('cmd', '')
        if c.startswith(prefixes) and 'reply' in s:
            yield s['t'], c, s['reply']


def pair(a, b):
    ra, oa = summary(a)
    rb, ob = summary(b)
    sa, sb = window(oa), window(ob)
    print(json.dumps({'A': ra, 'B': rb}, indent=1))
    if not (sa and sb):
        return
    for dim in ('Overall', 'overworld'):
        xa, xb = series(sa, dim), series(sb, dim)
        # pair samples by order (the two servers sample on the same 5 s clock, started 5 s apart)
        n = min(len(xa), len(xb))
        d = [xb[i][1] - xa[i][1] for i in range(n)]
        print(dim, 'B-A per sample', json.dumps(stats(d)))


def main():
    if sys.argv[1] == '--pair':
        pair(sys.argv[2], sys.argv[3])
        return
    for p in [a for a in sys.argv[1:] if not a.startswith('--')]:
        r, out = summary(p)
        print(json.dumps(r, indent=1))
        if '--replies' in sys.argv:
            for t, c, rep in replies(out):
                print('  [%s] %s\n    %s' % (t, c, rep.replace('\n', '\n    ')[:3000]))


if __name__ == '__main__':
    main()
