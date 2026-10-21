"""Every 20 s, asks each running rehearsal server for its entity count and TPS; appends to runs/probe-<dir>.log.
usage: python3 -I probe.py DIR [DIR...]   (reads the run's RCON port and password from its server.properties)"""
import os, sys, time, threading
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rcon
R = os.path.expanduser('~/mic-omegasrv/runs')
def props(d):
    out = {}
    for ln in open(os.path.join(R, d, 'server.properties')):
        if '=' in ln and not ln.startswith('#'):
            k, v = ln.rstrip('\n').split('=', 1); out[k] = v
    return out
def watch(d):
    log = open(os.path.join(R, 'probe-%s.log' % d), 'a')
    c = None
    while True:
        try:
            if c is None:
                p = props(d)
                if p.get('enable-rcon') != 'true' or not p.get('rcon.password'):
                    return
                c = rcon.Rcon('127.0.0.1', int(p['rcon.port']), p['rcon.password'], timeout=90)
            t0 = time.time()
            e = c.cmd('execute if entity @e').strip()
            it = c.cmd('execute if entity @e[type=minecraft:item]').strip()
            tps = c.cmd('neoforge tps').strip().splitlines()[-1] if True else ''
            log.write('%s %.1fs | %s | items %s | %s\n' % (time.strftime('%T'), time.time() - t0, e, it, tps)); log.flush()
        except Exception as ex:  # noqa: BLE001
            log.write('%s error %r\n' % (time.strftime('%T'), ex)); log.flush()
            c = None
            time.sleep(10)
        time.sleep(20)
ts = [threading.Thread(target=watch, args=(d,)) for d in sys.argv[1:]]
[t.start() for t in ts]; [t.join() for t in ts]
