# The synthetic stress world

A flat world of N districts of running Create factories and M scheduled cargo trains between
neighbouring districts, each train loading and unloading through Portable Storage Interfaces at both
ends. It exists to measure rtsim at scales no public world reaches (DECISIONS.md ST1, "thousands of
Create machines") and to give station learning cargo ports and exchange rules (ST3). Results and the
choices behind it (SG1 to SGn) are in `docs/stress-worlds-results.md` section 10.

Dev and test code only: the builder is a GameTest in the create plugin
(`gametest/stress/StressWorld.java`) that runs only in the `stressGen` Gradle run. It adds no block,
item or entity (D3); every factory, track, station, train and port is Create's, placed through Create's
own APIs, and Create assembles and schedules the trains itself.

## Build a world

```
tools/stress-worlds/synth.sh <name> [DISTRICTS=40] [CELLS=32] [TRAINS=-1] [SEED=1] [DWELL=10] \
    [KINDS=millstone:4,press:2,saw:2,brass:1] [XMX=12G]
```

- `DISTRICTS`: districts, laid out in a grid of near-square aspect, filled row by row.
- `CELLS`: factories per district, eight per 16x32 machine chunk column.
- `TRAINS`: cargo trains. Each pair of horizontal neighbours in a grid row gets lanes in turn, up to
  four per pair; -1 means two per pair.
- `SEED`: picks each factory's kind and each line's two goods.
- `DWELL`: seconds a train waits at each station of its cyclic schedule.
- `KINDS`: the glass corpus factories (millstone flour, press sheets, saw shafts, brass mixing) and
  their weights.
- `XMX`: the GameTest server's heap.

It takes `/tmp/mic-rtsim-bridge-heavy.lock` and runs at `nice -n 10`. The output is:

- `~/mic-stress-worlds/synthetic/<name>/world`: the world. The folder is a btrfs subvolume outside
  git, so `/home`'s snapshots skip it. Never commit a world.
- `~/mic-stress-worlds/synthetic/<name>/stressgen.json`: what was built. It holds the parameters, the
  grid, factory counts by kind, and per district its chunk box and factories. Per train it holds the
  UUID, the two stations with their blocks and calibrated dock blocks, the two goods and the build
  ticks.
- `~/mic-stress-worlds/scenes/synthetic-<name>.json`: an `sw.py` scene. It forceloads every district
  and the track between them, runs `rtsim district adopt` across each district, and sets a 600 s busy
  window, which spans the 6,000-tick periodic detection pass.
- `neoforge/create/build/stressgen-<name>.log`: the GameTest server's log.

The build fails, and leaves no world, unless every line delivers: each train must make its
calibration stops and then carry its good into the other end's sink vault. The `[stressgen]` log
lines name the phase and any stuck lane.

`synth.sh` strips what the build session left behind, so a pack server starts the world fresh:

- rtsim's state from the stub engine;
- the GameTest server's `serverconfig`, so the pack's defaultconfigs apply;
- the experimental datapacks it enables (`bundle`, `trade_rebalance`).

The world keeps the GameTest world's flat terrain and game rules: no mob spawning, weather, fire tick
or random ticks.

The run directly, without the copy and clean-up:
`MIC_STRESS_NAME=x MIC_STRESS_DISTRICTS=40 ./gradlew :create:runStressGen -PskipJni` in `neoforge/`
(dev shell). The world lands in `neoforge/create/run-stressgen/world`, which is gitignored.

## Measure it

```
flock /tmp/mic-rtsim-bridge-heavy.lock python3 -I tools/stress-worlds/sw.py pair <tag> \
    --world ~/mic-stress-worlds/synthetic/<name>/world \
    --scene ~/mic-stress-worlds/scenes/synthetic-<name>.json \
    --arms off,on,on --jars ~/mic-stress-worlds/jars/<set> --slot 60
```

The third arm is the A/A control: two rtsim arms on one world show the noise between identical
servers. Per rtsim arm, `sw-result.json` holds:

- MSPT and CPU per tick for each window;
- `rtsim_reports.busy.tick_cost`, rtsim's own tick handlers over the busy window (mean, p50, p99,
  p99.9 and worst, from `/rtsim perf` after a `/rtsim perf reset`);
- `/rtsim district perf`, which reports detection, rescans and the trains' arrivals and departures.

The jar set must be built from a tree that has `/rtsim perf reset`.

Then read what the engine learned from the rtsim arm's saved world:

```
cargo run --release -p rtsim-cli -- lines ~/mic-stress-worlds/runs/<tag>-B-on/world [--json]
```

It lists each line edge (from and to station, trips, mean travel ticks, cargo per trip) and each
district's exchange rules (what a station unloads and loads).
