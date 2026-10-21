"""Groups a server log's ERROR and WARN lines by logger and normalized message; diffs two runs.

usage: python3 -I logsum.py LOG [BASE_LOG]
With BASE_LOG, prints only groups that are new or at least doubled against the base.
Also prints counts for the deploy checks: mixin failures, unknown attachments, missing registry entries,
Sable, Destroy, rtsim and chunk load errors.
"""
import collections
import re
import sys

LINE = re.compile(r'^\[[^\]]*\] \[[^\]]*/(ERROR|WARN|FATAL)\] \[([^\]]*)\]: (.*)$')
CHECKS = {
    'mixin apply failures': re.compile(r'Mixin apply.*failed|MixinApplyError|InvalidInjectionException|Critical injection failure', re.I),
    'unknown attachments': re.compile(r'unknown or non-serializable data attachment'),
    'unknown attachment types': re.compile(r'unknown or non-serializable data attachment (\S+)'),
    'missing registry / unknown ids': re.compile(r'Unknown (block|item|entity|block entity)|Skipping BlockEntity with id|Tried to load invalid item|Unknown registry key|missing registry|Unidentified mapping|No key id', re.I),
    'chunk load errors': re.compile(r"Couldn't load chunk|Failed to load chunk|Recovering|Chunk file at .* is in the wrong location|Failed to read chunk", re.I),
    'sable': re.compile(r'sable|sub.?level', re.I),
    'destroy': re.compile(r'destroy|petrolpark|mixture', re.I),
    'rtsim': re.compile(r'mic_rtsim|rtsim|mic_native', re.I),
    "can't keep up": re.compile(r"Can't keep up"),
}


def norm(s):
    s = re.sub(r'/\S*?/runs/[^/\s]+', 'RUNDIR', s)
    s = re.sub(r'[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}', 'UUID', s)
    s = re.sub(r'-?\d+(\.\d+)?', 'N', s)
    return s[:150]


def groups(path):
    g = collections.Counter()
    checks = collections.Counter()
    types = collections.Counter()
    samples = {}
    with open(path, errors='replace') as f:
        for ln in f:
            ln = ln.rstrip('\n')
            m = LINE.match(ln)
            if m:
                key = '%s %s: %s' % (m.group(1), m.group(2), norm(m.group(3)))
                g[key] += 1
                samples.setdefault(key, ln[:300])
            for name, rx in CHECKS.items():
                mm = rx.search(ln)
                if mm and (m or name in ('mixin apply failures',)):
                    if name == 'unknown attachment types':
                        types[mm.group(1)] += 1
                    else:
                        checks[name] += 1
    return g, checks, types, samples


def main():
    g, checks, types, samples = groups(sys.argv[1])
    print('== checks (ERROR/WARN lines only):', dict(checks))
    if types:
        print('== unknown attachment types:', dict(types))
    if len(sys.argv) > 2:
        b, bchecks, _, _ = groups(sys.argv[2])
        print('== base checks:', dict(bchecks))
        print('== groups new or doubled against the base')
        for k, n in g.most_common():
            if n >= 2 * max(b.get(k, 0), 1) or k not in b:
                print('%6d (base %d) %s' % (n, b.get(k, 0), k))
    else:
        print('== top groups')
        for k, n in g.most_common(60):
            print('%6d %s' % (n, k))


if __name__ == '__main__':
    main()
