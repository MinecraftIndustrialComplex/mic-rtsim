"""The main base's glass solves and the engine's hand-overs, from a round 2 run's per-sample replies.

usage: python3 -I glassreads.py OUT.json [OUT.json ...]
Reads sample_tps's "also" replies ('rtsim perf', 'rtsim district info 1') and every other reply of those
two commands. A new solve shows as a higher solve count; its time is the "last N us" read with it, so a
read that finds two or more new solves sees only the last of them (counted, not timed).
Prints per run: hand-overs submitted and ticks waited over the sampling window and the whole run, the
glass compile line's rounds and convergence, solves seen and timed, solve time spread, the checksum's
sweeps sampled, and the final save's line from the run's log when LOG=path is given for a single run.
"""
import json
import re
import statistics
import sys

HAND = re.compile(r'hand-overs: (\d+) submitted, (\d+) ticks waited on the engine')
GLASS = re.compile(r'glass: compile (\d+): (\d+) nodes, (\d+) edges, (\d+) rounds \(([^)]*)\), (\d+) solves, last (\d+) us')
CHECK = re.compile(r'glass checksum: .*?(\d+) sweeps \((\d+) sampled\).*?revision (\d+) \(district (\w+)\)')
STEPS = re.compile(r'(\d+) steps, ')


def reads(out):
    """(t, text) of every 'rtsim perf' and 'rtsim district info 1' reply, in time order."""
    res = []
    for st in out.get('steps', []):
        c = st['step'].get('cmd', '')
        if c in ('rtsim perf', 'rtsim district info 1') and 'reply' in st:
            res.append((st['t'], st['reply']))
        for row in st.get('also', []):
            for text in row[1:]:
                res.append((row[0], text))
    return sorted(res, key=lambda r: r[0])


def window(out):
    for st in out.get('steps', []):
        if 'also' in st:
            return st['also'][0][0], st['also'][-1][0]
    return None


def pct(xs, q):
    s = sorted(xs)
    return s[min(len(s) - 1, int(len(s) * q))]


def report(path):
    out = json.load(open(path))
    rs = reads(out)
    hands = [(t, int(m.group(1)), int(m.group(2))) for t, x in rs for m in [HAND.search(x)] if m]
    glass = [(t, m) for t, x in rs for m in [GLASS.search(x)] if m]
    checks = [(t, m) for t, x in rs for m in [CHECK.search(x)] if m]
    r = {'run': path.rsplit('/', 1)[-1]}
    w = window(out)
    if hands:
        r['handovers_end'] = {'submitted': hands[-1][1], 'waited': hands[-1][2]}
        if w:
            inw = [h for h in hands if w[0] <= h[0] <= w[1]]
            if len(inw) >= 2:
                sub, wai = inw[-1][1] - inw[0][1], inw[-1][2] - inw[0][2]
                r['handovers_window'] = {'submitted': sub, 'waited': wai,
                                         'waited_pct': round(100.0 * wai / sub, 2) if sub else None}
    if glass:
        last = glass[-1][1]
        r['compile'] = {'compile': int(last.group(1)), 'nodes': int(last.group(2)), 'edges': int(last.group(3)),
                        'rounds': int(last.group(4)), 'state': last.group(5)}
        timed, seen, prev = [], 0, None
        states = {}
        for t, m in glass:
            n, us = int(m.group(6)), int(m.group(7))
            states[m.group(5)] = states.get(m.group(5), 0) + 1
            if prev is not None and n > prev:
                seen += n - prev
                timed.append(us)
            prev = n
        r['solves'] = {'at_end': prev, 'first_read': int(glass[0][1].group(6)), 'new_seen': seen,
                       'timed': len(timed), 'compile_states_read': states}
        if timed:
            r['solve_us'] = {'mean': round(statistics.fmean(timed)), 'p50': pct(timed, 0.5), 'p90': pct(timed, 0.9),
                             'max': max(timed), 'min': min(timed)}
        r['last_solve_us_end'] = int(last.group(7))
    if checks:
        m = checks[-1][1]
        r['checksum'] = {'sweeps': int(m.group(1)), 'sampled': int(m.group(2)), 'revision': int(m.group(3)),
                         'district_revision': m.group(4)}
    steps = [(t, int(m.group(1))) for t, x in rs for m in [STEPS.search(x)] if m]
    if steps:
        r['engine_steps_end'] = steps[-1][1]
    print(json.dumps(r, indent=1))


if __name__ == '__main__':
    for p in sys.argv[1:]:
        report(p)
