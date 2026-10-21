"""The "Can't keep up" lines of a rehearsal run, by phase: boot, base loading, settling, sampling, stop.

usage: python3 -I spikes.py OUT.json LOG [...]   (pairs of a runsrv OUT.json and the run's latest.log)
A line says the server is N ms behind (vanilla logs it when it falls over 2 s behind, at most every 15 s),
so the largest N in a phase bounds how far one stretch of ticks fell behind there.
"""
import datetime
import json
import re
import sys

LINE = re.compile(r"^\[(\d\d)(\w{3})(\d{4}) (\d\d):(\d\d):(\d\d)\.(\d+)\].*Can't keep up!.*?Running (\d+)ms or (\d+) ticks")
MON = {m: i + 1 for i, m in enumerate(['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'])}


def phases(out):
    t0 = datetime.datetime.strptime(out['started'], '%Y-%m-%d %H:%M:%S').timestamp()
    marks = [('boot', 0.0)]
    first_fl = next((s['t'] for s in out['steps'] if s['step'].get('cmd', '').startswith('forceload add')), None)
    if first_fl is not None:
        marks.append(('bases', first_fl))
    q = next((s['t'] for s in out['steps'] if s['step'].get('cmd') == 'forceload query'), None)
    if q is not None:
        marks.append(('settle', q - 120))
    samp = next((s for s in out['steps'] if 'sample_tps' in s['step']), None)
    if samp is not None:
        marks.append(('sample', samp['t']))
        marks.append(('stop', samp['t'] + samp['step']['sample_tps']['seconds']))
    return t0, marks


def main():
    a = sys.argv[1:]
    for out_path, log_path in zip(a[0::2], a[1::2]):
        out = json.load(open(out_path))
        t0, marks = phases(out)
        by = {}
        for ln in open(log_path, errors='replace'):
            m = LINE.match(ln)
            if not m:
                continue
            d, mon, y, hh, mm, ss, frac, ms, ticks = m.groups()
            ts = datetime.datetime(int(y), MON[mon], int(d), int(hh), int(mm), int(ss)).timestamp() - t0
            ph = [n for n, t in marks if ts >= t][-1]
            by.setdefault(ph, []).append(int(ms))
        row = []
        for n, _ in marks:
            v = by.get(n, [])
            row.append('%s n=%d max=%.1fs' % (n, len(v), max(v) / 1000 if v else 0))
        print('%-28s %s' % (out_path.split('/')[-1], ' | '.join(row)))


if __name__ == '__main__':
    main()
