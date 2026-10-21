"""Writes the rehearsal scenarios for runsrv.py.

usage: python3 -I scenarios.py KIND PORT JAVA NEOFORGE SEED OUT.json
KIND: plain (load the bases and measure), rtsim (plus district adoption and rtsim reports),
      reload (rtsim reports right after boot, then measure), uninstall (boot and look, no rtsim commands).
"""
import json
import os
import sys

# Where the players' bases are: a private JSON file (BASES, default ~/mic-omegasrv/bases.json) with
# "strips", "villages" and "small" (forceload boxes x1 z1 x2 z2, each under the 256-chunk cap),
# "adopt_at" (x y z for /rtsim district adopt 16) and "village_at" (x y z inside an adopted village).
B = json.load(open(os.environ.get('BASES', os.path.expanduser('~/mic-omegasrv/bases.json'))))
STRIPS, VILLAGES, SMALL = [tuple(b) for b in B['strips']], [tuple(b) for b in B['villages']], [tuple(b) for b in B['small']]
ADOPT_AT, VILLAGE_AT = [tuple(b) for b in B['adopt_at']], [tuple(b) for b in B['village_at']]


def forceload():
    return [{'cmd': 'forceload add %d %d %d %d' % b} for b in STRIPS + VILLAGES + SMALL]


def main():
    kind, port, java, neo, seed, out = sys.argv[1:7]
    port = int(port)
    sc = {'port': port, 'rcon_port': port + 1, 'java': java, 'neoforge': neo, 'seed': seed, 'nice': 10,
          'boot_timeout': 2400, 'jvm': ['-Xms2G', '-Xmx8G', '-XX:ActiveProcessorCount=4'],
          'props': {'simulation-distance': 16, 'view-distance': 16, 'motd': 'deploy rehearsal'}}
    steps = [{'wait': 20}, {'cmd': 'neoforge tps'}]
    if kind in ('rtsim', 'reload'):
        steps += [{'cmd': 'rtsim perf'}, {'cmd': 'rtsim district list'}]
    steps += forceload()
    steps += [{'wait': 240}, {'cmd': 'forceload query'}]
    if kind in ('rtsim', 'reload'):
        steps += [{'cmd': 'rtsim perf'}, {'cmd': 'rtsim district list'}]
        steps += [{'cmd': 'execute positioned %d %d %d in minecraft:overworld run rtsim village' % v} for v in VILLAGE_AT]
    if kind == 'rtsim':
        steps += [{'cmd': 'execute positioned %d %d %d in minecraft:overworld run rtsim district adopt 16' % a}
                  for a in ADOPT_AT]
        steps += [{'wait': 60}, {'cmd': 'rtsim district list'}, {'cmd': 'rtsim district perf'}]
    else:
        steps += [{'wait': 60}]  # keeps the sampling window level with an rtsim run started beside it
    steps += [{'sample_tps': {'seconds': 300, 'every': 5}}, {'cmd': 'neoforge tps'}]
    if kind in ('rtsim', 'reload'):
        steps += [{'cmd': 'rtsim perf'}, {'cmd': 'rtsim district perf'}, {'cmd': 'rtsim district list'}]
        for name in ('info', 'debug'):
            steps += [{'cmd': 'execute positioned %d %d %d in minecraft:overworld run rtsim district %s' % (ADOPT_AT[0] + (name,))}]
    steps += [{'cmd': 'save-all flush'}, {'wait': 10}, {'stop': True, 'timeout': 900}]
    sc['steps'] = steps
    json.dump(sc, open(out, 'w'), indent=1)


if __name__ == '__main__':
    main()
