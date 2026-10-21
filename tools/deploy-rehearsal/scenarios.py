"""Writes the rehearsal scenarios for runsrv.py.

usage: python3 -I scenarios.py KIND PORT JAVA NEOFORGE SEED OUT.json [CPUS]
KIND: plain (load the bases and measure), rtsim (plus district adoption and rtsim reports),
      reload (rtsim reports right after boot, then measure), uninstall (boot and look, no rtsim commands),
      reset (as reload, on a world whose world/rtsim was deleted: SRV2's sim reset).
"""
import json
import os
import sys

# Where the players' bases are: a private JSON file (BASES, default ~/mic-livesrv/bases.json) with
# "strips", "villages" and "small" (forceload boxes x1 z1 x2 z2, each under the 256-chunk cap),
# "adopt_at" (x y z for /rtsim district adopt 16) and "village_at" (x y z inside an adopted village).
B = json.load(open(os.environ.get('BASES', os.path.expanduser('~/mic-livesrv/bases.json'))))
STRIPS, VILLAGES, SMALL = [tuple(b) for b in B['strips']], [tuple(b) for b in B['villages']], [tuple(b) for b in B['small']]
ADOPT_AT, VILLAGE_AT = [tuple(b) for b in B['adopt_at']], [tuple(b) for b in B['village_at']]


def forceload():
    # one box at a time, 20 s apart: 1,400 chunks ticketed at once made the main thread spend over a
    # minute promoting chunks in one tick (the 60 s watchdog fired); players arrive one at a time anyway
    out = []
    for b in STRIPS + VILLAGES + SMALL:
        out += [{'cmd': 'forceload add %d %d %d %d' % b}, {'wait': 20}]
    return out


def main():
    kind, port, java, neo, seed, out = sys.argv[1:7]
    port = int(port)
    sc = {'port': port, 'rcon_port': port + 1, 'java': java, 'neoforge': neo, 'seed': seed, 'nice': 10,
          'boot_timeout': 2400, 'jvm': ['-Xms2G', '-Xmx8G', '-XX:ActiveProcessorCount=4',
                                    # the live host has 4 cores: mic_native's defaults there (P24) are a
                                    # 4-thread foreground pool and a 2-thread background pool
                                    '-Dmic_native.fgThreads=4', '-Dmic_native.bgThreads=2',
                                    # the live set after each collection, for the heap size in the runbook
                                    '-Xlog:gc:file=logs/gc.log:uptime'],
          'props': {'simulation-distance': 16, 'view-distance': 16, 'motd': 'deploy rehearsal',
                    # live has 60000. Loading the bases here makes single ticks of tens of seconds on a
                    # shared laptop, and 2 of 4 servers of attempt 4 died to the 60 s watchdog; the
                    # longest ticks are read from the "Can't keep up" lines instead
                    'max-tick-time': 300000}}
    if kind in ('reload', 'reset'):
        sc['jvm'].append('-Dmixin.debug.verbose=true')  # one "Mixing X into Y" line per applied mixin
    steps = [{'wait': 20}, {'cmd': 'neoforge tps'}]
    if kind in ('rtsim', 'reload', 'reset'):
        steps += [{'cmd': 'rtsim perf'}, {'cmd': 'rtsim district list'}]
    steps += forceload()
    steps += [{'wait': 120}, {'cmd': 'forceload query'}]
    if kind in ('rtsim', 'reload', 'reset'):
        steps += [{'cmd': 'rtsim perf'}, {'cmd': 'rtsim district list'}]
        steps += [{'cmd': 'execute positioned %d %d %d in minecraft:overworld run rtsim village' % v} for v in VILLAGE_AT]
    if kind == 'rtsim':
        steps += [{'cmd': 'execute positioned %d %d %d in minecraft:overworld run rtsim district adopt 16' % a}
                  for a in ADOPT_AT]
        steps += [{'wait': 60}, {'cmd': 'rtsim district list'}, {'cmd': 'rtsim district perf'}]
    else:
        steps += [{'wait': 60}]  # keeps the sampling window level with an rtsim run started beside it
    steps += [{'sample_tps': {'seconds': 300, 'every': 5}}, {'cmd': 'neoforge tps'}]
    if kind in ('rtsim', 'reload', 'reset'):
        steps += [{'cmd': 'rtsim perf'}, {'cmd': 'rtsim district perf'}, {'cmd': 'rtsim district list'}]
        for name in ('info', 'debug'):
            steps += [{'cmd': 'execute positioned %d %d %d in minecraft:overworld run rtsim district %s' % (ADOPT_AT[0] + (name,))}]
        # the main base's district by id (district 1 in every run here): its glass compile and sweeps
        steps += [{'cmd': 'rtsim district %s 1' % name} for name in ('info', 'debug')]
    # entity census and the one gamerule rtsim changes (it turns doTraderSpawning off while it owns traders)
    steps += [{'cmd': 'execute if entity @e[type=%s]' % t} for t in
              ('minecraft:villager', 'minecraft:wandering_trader', 'ribbits:ribbit', 'minecraft:iron_golem')]
    steps += [{'cmd': 'execute if entity @e'}, {'cmd': 'gamerule doTraderSpawning'}]
    steps += [{'cmd': 'save-all flush'}, {'wait': 10}, {'stop': True, 'timeout': 900}]
    sc['steps'] = steps
    if len(sys.argv) > 7 and sys.argv[7]:
        sc['cpus'] = sys.argv[7]
    json.dump(sc, open(out, 'w'), indent=1)


if __name__ == '__main__':
    main()
