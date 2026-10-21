"""Boot a NeoForge server dir, drive it over RCON, measure, stop. Deploy rehearsal harness.

usage: python3 -I runsrv.py SERVER_DIR SCENARIO.json OUT.json
SCENARIO: {"port": 25871, "rcon_port": 25872, "seed": "...", "jvm": ["-Xms6G", "-Xmx10G"],
           "neoforge": "21.1.230", "java": "/nix/store/.../bin/java", "nice": 10,
           "boot_timeout": 1800,
           "steps": [{"cmd": "forceload add 0 0 15 15"}, {"wait": 60},
                     {"sample_tps": {"seconds": 300, "every": 5}}, {"cmd": "save-all flush"}, {"stop": true}]}
Every command's reply is logged in OUT.json. A java process left over is killed only if it is ours.
"""
import json
import os
import re
import secrets
import signal
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rcon as rconlib  # noqa: E402


def set_props(path, updates):
    lines = open(path).read().splitlines()
    seen = set()
    out = []
    for ln in lines:
        k = ln.split('=', 1)[0] if '=' in ln and not ln.startswith('#') else None
        if k in updates:
            out.append('%s=%s' % (k, updates[k]))
            seen.add(k)
        else:
            out.append(ln)
    for k, v in updates.items():
        if k not in seen:
            out.append('%s=%s' % (k, v))
    open(path, 'w').write('\n'.join(out) + '\n')


TPS_RE = re.compile(r'([A-Za-z0-9_:./-]+):\s*([\d.,]+)\s*TPS\s*\(([\d.,]+)\s*ms/tick\)')


def parse_tps(text):
    # "Overall: 20.000 TPS (12.345 ms/tick)" and "<dimension>: ... TPS (... ms/tick)"; value is (ms, tps)
    res = {}
    for m in TPS_RE.finditer(text):
        res[m.group(1)] = (float(m.group(3).replace(',', '')), float(m.group(2).replace(',', '')))
    return res


def main():
    sdir, scen_path, out_path = sys.argv[1], sys.argv[2], sys.argv[3]
    sc = json.load(open(scen_path))
    pw = secrets.token_hex(16)
    props = {'server-port': sc.get('port', 25871), 'enable-rcon': 'true', 'rcon.port': sc.get('rcon_port', 25872),
             'rcon.password': pw, 'broadcast-rcon-to-ops': 'false'}
    if 'seed' in sc:
        props['level-seed'] = sc['seed']
    props.update(sc.get('props', {}))
    set_props(os.path.join(sdir, 'server.properties'), props)
    with open(os.path.join(sdir, 'user_jvm_args.txt'), 'w') as f:
        f.write('\n'.join(sc.get('jvm', ['-Xms6G', '-Xmx10G'])) + '\n')
    java = sc.get('java', 'java')
    args = ['nice', '-n', str(sc.get('nice', 10)), java, '@user_jvm_args.txt',
            '@libraries/net/neoforged/neoforge/%s/unix_args.txt' % sc['neoforge'], 'nogui']
    os.makedirs(os.path.join(sdir, 'logs'), exist_ok=True)
    console = open(os.path.join(sdir, 'console.log'), 'ab')
    t0 = time.time()
    proc = subprocess.Popen(args, cwd=sdir, stdin=subprocess.DEVNULL, stdout=console, stderr=subprocess.STDOUT,
                            start_new_session=True)
    result = {'scenario': scen_path, 'server': sdir, 'pid': proc.pid, 'started': time.strftime('%F %T'),
              'steps': []}

    def save():
        json.dump(result, open(out_path, 'w'), indent=1)

    latest = os.path.join(sdir, 'logs', 'latest.log')
    done = None
    deadline = t0 + sc.get('boot_timeout', 1800)
    while time.time() < deadline:
        if proc.poll() is not None:
            break
        try:
            with open(latest, 'rb') as f:
                txt = f.read().decode('utf-8', 'replace')
            m = re.search(r'Done \(([\d.]+)s\)!', txt)
            if m and os.path.getmtime(latest) >= t0 - 5:
                done = float(m.group(1))
                break
        except FileNotFoundError:
            pass
        time.sleep(2)
    result['boot_wall_s'] = round(time.time() - t0, 1)
    result['done_s'] = done
    if done is None:
        result['error'] = 'no Done line (exit %s)' % proc.poll()
        save()
        if proc.poll() is None:
            os.killpg(proc.pid, signal.SIGTERM)
            proc.wait(300)
        return
    save()
    r = rconlib.connect_retry('127.0.0.1', int(props['rcon.port']), pw)
    stopped = False
    for st in sc.get('steps', []):
        rec = {'step': st, 't': round(time.time() - t0, 1)}
        try:
            if 'cmd' in st:
                rec['reply'] = r.cmd(st['cmd'])
            elif 'wait' in st:
                time.sleep(st['wait'])
            elif 'sample_tps' in st:
                p = st['sample_tps']
                samples = []
                end = time.time() + p['seconds']
                while time.time() < end:
                    txt = r.cmd(p.get('command', 'neoforge tps'))
                    samples.append([round(time.time() - t0, 1), parse_tps(txt)])
                    time.sleep(p.get('every', 5))
                rec['samples'] = samples
                ov = [s[1]['Overall'][0] for s in samples if 'Overall' in s[1]]
                if ov:
                    ov2 = sorted(ov)
                    rec['overall_ms'] = {'n': len(ov), 'mean': round(sum(ov) / len(ov), 3),
                                         'p50': ov2[len(ov2) // 2], 'p90': ov2[int(len(ov2) * 0.9)],
                                         'min': ov2[0], 'max': ov2[-1]}
            elif 'stop' in st:
                ts = time.time()
                try:
                    rec['reply'] = r.cmd('stop')
                except (OSError, ConnectionError):
                    pass
                try:
                    proc.wait(st.get('timeout', 600))
                except subprocess.TimeoutExpired:
                    os.killpg(proc.pid, signal.SIGTERM)
                    proc.wait(300)
                rec['stop_s'] = round(time.time() - ts, 1)
                rec['exit'] = proc.returncode
                stopped = True
        except Exception as e:  # noqa: BLE001
            rec['error'] = repr(e)
            if proc.poll() is not None:
                result['steps'].append(rec)
                result['died'] = proc.returncode
                break
            try:
                r.close()
                r = rconlib.connect_retry('127.0.0.1', int(props['rcon.port']), pw, 120)
            except Exception as e2:  # noqa: BLE001
                rec['reconnect_error'] = repr(e2)
        result['steps'].append(rec)
        save()
    if not stopped and proc.poll() is None:
        try:
            r.cmd('stop')
        except Exception:  # noqa: BLE001
            os.killpg(proc.pid, signal.SIGTERM)
        proc.wait(600)
    result['total_s'] = round(time.time() - t0, 1)
    # scrub the password from server.properties so it never sits around
    set_props(os.path.join(sdir, 'server.properties'), {'rcon.password': '', 'enable-rcon': 'false'})
    save()


if __name__ == '__main__':
    main()
