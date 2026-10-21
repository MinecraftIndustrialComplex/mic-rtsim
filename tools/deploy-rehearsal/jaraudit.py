"""Jar-level audit of two NeoForge mods folders: mod ids, versions, mandatory dependency ranges.

usage: python3 -I jaraudit.py OLD_MODS NEW_MODS [EXTRA_JARS...] > audit.json
Prints a JSON report: per mod id, old and new version and jar; added, removed, changed; and
mandatory dependencies of the NEW set (plus extra jars) that are missing or out of range.
"""
import io
import json
import os
import re
import sys
import tomllib
import zipfile


def manifest_version(z):
    try:
        m = z.read('META-INF/MANIFEST.MF').decode('utf-8', 'replace')
    except KeyError:
        return None
    for line in m.splitlines():
        if line.startswith('Implementation-Version:'):
            return line.split(':', 1)[1].strip()
    return None


def read_jar(data, name, out, depth=0):
    try:
        z = zipfile.ZipFile(io.BytesIO(data))
    except zipfile.BadZipFile:
        return
    names = set(z.namelist())
    toml_name = None
    for cand in ('META-INF/neoforge.mods.toml', 'META-INF/mods.toml'):
        if cand in names:
            toml_name = cand
            break
    if toml_name:
        try:
            t = tomllib.loads(z.read(toml_name).decode('utf-8', 'replace'))
        except Exception as e:  # noqa: BLE001
            t = {}
            out.append({'jar': name, 'error': 'toml: %s' % e, 'nested': depth > 0})
        mv = manifest_version(z)
        for mod in t.get('mods', []):
            mid = mod.get('modId')
            ver = str(mod.get('version', ''))
            if '${' in ver and mv:
                ver = mv
            deps = []
            for d in t.get('dependencies', {}).get(mid, []) if isinstance(t.get('dependencies'), dict) else []:
                typ = d.get('type')
                mandatory = d.get('mandatory')
                if typ is None:
                    typ = 'required' if mandatory else 'optional'
                deps.append({'modId': d.get('modId'), 'type': str(typ).lower(),
                             'versionRange': d.get('versionRange', ''), 'side': d.get('side', 'BOTH')})
            out.append({'jar': name, 'modId': mid, 'version': ver, 'deps': deps, 'nested': depth > 0})
    elif 'fabric.mod.json' in names:
        try:
            fj = json.loads(z.read('fabric.mod.json').decode('utf-8', 'replace'), strict=False)
            out.append({'jar': name, 'modId': fj.get('id'), 'version': str(fj.get('version')),
                        'deps': [{'modId': k, 'type': 'required', 'versionRange': str(v), 'side': 'BOTH'}
                                 for k, v in (fj.get('depends') or {}).items()],
                        'nested': depth > 0, 'fabric': True})
        except Exception as e:  # noqa: BLE001
            out.append({'jar': name, 'error': 'fabric.mod.json: %s' % e, 'nested': depth > 0})
    for n in names:
        if n.startswith('META-INF/jarjar/') and n.endswith('.jar') and depth < 3:
            read_jar(z.read(n), name + '!' + n.split('/')[-1], out, depth + 1)
    for n in names:
        if n.startswith('META-INF/jars/') and n.endswith('.jar') and depth < 3:
            read_jar(z.read(n), name + '!' + n.split('/')[-1], out, depth + 1)


def scan(paths):
    out = []
    for p in paths:
        if os.path.isdir(p):
            files = sorted(os.path.join(p, f) for f in os.listdir(p) if f.endswith('.jar'))
        else:
            files = [p]
        for f in files:
            with open(f, 'rb') as fh:
                read_jar(fh.read(), os.path.basename(f), out)
    return out


def index(entries):
    by = {}
    for e in entries:
        if 'modId' not in e or not e['modId']:
            continue
        cur = by.get(e['modId'])
        # top-level jars win over nested copies
        if cur is None or (cur['nested'] and not e['nested']):
            by[e['modId']] = e
    return by


def vkey(v):
    parts = re.split(r'[.\-+_]', v)
    k = []
    for p in parts:
        if p.isdigit():
            k.append((0, int(p), ''))
        else:
            m = re.match(r'(\d+)(.*)', p)
            if m:
                k.append((0, int(m.group(1)), m.group(2)))
            else:
                k.append((1, 0, p))
    return k


def in_range(ver, rng):
    """Maven range check, loose (qualifiers compared as text)."""
    rng = (rng or '').strip()
    if not rng or rng == '*':
        return True
    if rng[0] not in '[(':
        # a bare version means "at least" in NeoForge's loose reading; treat as soft
        return vkey(ver) >= vkey(rng)
    for part in re.findall(r'[\[(][^\])]*[\])]', rng):
        lo_inc = part[0] == '['
        hi_inc = part[-1] == ']'
        body = part[1:-1]
        if ',' in body:
            lo, hi = body.split(',', 1)
        else:
            lo = hi = body
        lo, hi = lo.strip(), hi.strip()
        ok = True
        if lo:
            ok = ok and (vkey(ver) >= vkey(lo) if lo_inc else vkey(ver) > vkey(lo))
        if hi:
            ok = ok and (vkey(ver) <= vkey(hi) if hi_inc else vkey(ver) < vkey(hi))
        if ok:
            return True
    return False


def main():
    old_dir, new_dir = sys.argv[1], sys.argv[2]
    extra = sys.argv[3:]
    old = index(scan([old_dir]))
    new_entries = scan([new_dir] + extra)
    new = index(new_entries)
    top_old = {k: v for k, v in old.items() if not v['nested']}
    top_new = {k: v for k, v in new.items() if not v['nested']}
    report = {'added': [], 'removed': [], 'changed': [], 'same': 0, 'missing_deps': [], 'range_fail': []}
    for k in sorted(set(top_old) | set(top_new)):
        o, n = top_old.get(k), top_new.get(k)
        if o and not n:
            nn = new.get(k)
            report['removed'].append({'modId': k, 'old': o['version'], 'jar': o['jar'],
                                      'now_nested_in': nn['jar'] if nn else None})
        elif n and not o:
            report['added'].append({'modId': k, 'new': n['version'], 'jar': n['jar']})
        elif o['version'] != n['version']:
            report['changed'].append({'modId': k, 'old': o['version'], 'new': n['version'],
                                      'old_jar': o['jar'], 'new_jar': n['jar']})
        else:
            report['same'] += 1
    builtin = {'minecraft': '1.21.1', 'neoforge': sys.argv[0] and os.environ.get('NEO', '21.1.249'),
               'java': '21', 'fabricloader': '0.16.9', 'fabric': '0.16.9'}
    for e in new_entries:
        if 'modId' not in e or e.get('nested'):
            continue
        for d in e['deps']:
            if d['type'] not in ('required',) or d['side'] == 'CLIENT':
                continue
            mid = d['modId']
            have = new.get(mid)
            hv = have['version'] if have else builtin.get(mid)
            if hv is None:
                report['missing_deps'].append({'mod': e['modId'], 'jar': e['jar'], 'needs': mid,
                                               'range': d['versionRange']})
            elif not in_range(hv, d['versionRange']):
                report['range_fail'].append({'mod': e['modId'], 'needs': mid, 'range': d['versionRange'],
                                             'have': hv})
    json.dump(report, sys.stdout, indent=1)


if __name__ == '__main__':
    main()
