# mic-rtsim

A world simulation for the MIC modpack (Minecraft 1.21.1, NeoForge): a Rust engine that owns
caravans, markets, herds, dragons and the rest, with Minecraft entities as its puppets. AGPL-3.0-or-later
(`LICENSE`). Private: `MinecraftIndustrialComplex/mic-rtsim`.

**Read first:** `DECISIONS.md` (authoritative, including "Overnight choices" waiting for Ben's
review), `SCALE.md` (the scale architecture), `PLAN.md` (the plan). `INDUSTRY.md` and `research/`
hold the industry design and the research behind them.

## What exists

| Path | What |
|---|---|
| `crates/rtsim-kernel` | The engine kernel: no Minecraft, no JNI, no threads of its own (PLAN.md §5.1). |
| `crates/rtsim-cli` | `rtsim-cli bench scale`, the scale bench of SCALE.md §10 (now with market and edge sweeps); `rtsim-cli sim phase1`, the phase 1 scenario; `sim quiet`, does a closed world settle; `sim memory`, where a world's memory goes; `bench industry` and `sim industry`; `sim dragons`, phase 2's dragons from autumn into winter (and the dragon sweep of `bench scale`); `bench restore`, the phase 1 world's snapshot against replaying its whole input log (`bench/results/restore.md`). |
| `crates/rtsim-industry` | The industry engine (INDUSTRY.md, C15 to C19), headless: learning, replay, reconciliation, links, deposits, threats, the ledger. Wire records in `docs/industry-boundary.md`; choices in `docs/overnight-choices-industry.md`. |
| `crates/rtsim-settle` | Settlements as rows: two cultures, villages, player bases and havens as market nodes, edges by P18's recommended rule (PLAN.md §8.2). |
| `crates/rtsim-trade` | Storehouse markets (C13): labour costs with Hawkins-Simon, anchors, price as a function of stock, the exact player trade integral, the line law in closed form (production below the target, a sink above the ceiling), money, exchange lines, offers through the treasury (C14, C23), and the decision rules that let a closed world settle (`Rules`). |
| `crates/rtsim-travel` | Flows on edges as pipelines with holes, the caravan lattice with its dock wait, news snapshots, allocation by margin with rationing, caravans and groups, the leg index, the road index, raids to havens (SCALE.md §3, PLAN.md §8.6). |
| `crates/rtsim-knowledge` | Reports as 32-byte rows, witnesses, sentiment with lazy decay, noticeboards and gossip packets, vanilla gossip writes, notables and heirs, temperament (PLAN.md §5.6, §5.7, §8.7). |
| `crates/rtsim-world` | The phase 1 composition: build and join, the step loop, flows and payments, news at the speed of trade, caravans near players, player trades, ribbit sorcerers, growth and touring bands, player bases and caravan trades, what players see, conservation, state hash and memory breakdown. Scaffolding until the module registry exists. |
| `crates/rtsim-ecology` | Phase 2's dragons (PLAN.md §9, C3), headless: dragons as notables on one timing wheel, closed-form hunger, flights in the shared leg index at a forced altitude, prey on caravan flows and village herds, grudge raids through the industry threat interface, heirs and lairs, the population the sim owns (P13), and the Ice and Fire plugin's records (`docs/boundary.md` §6). Choices in `docs/overnight-choices-dragons.md`. |
| `crates/rtsim-jni` | The native link: a `mic_native` plugin (cdylib) that owns the four boundary buffers, runs the engine step on the host's background pool and writes pages on its IO lane; a minimal world of puppet agents, the industry engine behind the boundary (`engine/industry.rs`, docs/industry-boundary.md §7, choices W1 to W23), the phase 1 world behind it (`engine/phase1.rs`: villages, markets, caravans materialized near players, news and gossip, growth; a snapshot of the world and the inputs since it in the pages; choices V1 to V28 in `docs/overnight-choices-phase1jni.md`, S1 to S7 in `docs/overnight-choices-persist.md`), and the dragons behind it (`engine/dragons.rs`, docs/boundary.md §6, S8 to S14). The only crate that uses `jni`. |
| `tools/natives/build-natives.sh` | rtsim-jni for the eight platforms plus x86-64-v3, in mic-native's dev shell (`nix develop ../mic-native -c bash tools/natives/build-natives.sh`). |
| `scenario/results/` | Output of `rtsim-cli sim phase1`. |
| `docs/overnight-choices-phase1.md` | The phase 1 engine's reversible choices (E1 to E42, round 2's E43 to E68 with before and after numbers), for Ben's review. |
| `docs/overnight-choices-phase1jni.md` | Phase 1 on the native engine: how the bridge's records reach the phase 1 world and its commands go back, saves by input log, caravans near players (V1 to V28). |
| `docs/overnight-choices-persist.md` | The phase 1 world's snapshot, the policy for pages a build cannot read, the dragons' save and their route through the native engine (S1 to S14). |
| `tools/ci/scale-check.sh` | The CI-style scale check: fails when a growth slope exceeds its contract. |
| `bench/results/` | Committed bench results, with the machine and conditions. |
| `neoforge/` | The Java side: the core bridge mod `mic_rtsim` (puppets, journal, module switches, vanilla bodies), its `NativeEngine` over rtsim-jni and a pure-Java stub engine for tests; GameTests. See `neoforge/README.md`. |
| `docs/boundary.md` | The binary contract between the bridge and the engine: four buffers, the hand-over, every record. |

The kernel's modules:

| Module | What | Spec |
|---|---|---|
| `handle` | Generational arenas; 6-byte handles (32-bit index, 16-bit generation) checked on every lookup | SCALE §9 |
| `rng` | Philox4x64-10 counter RNG keyed by (world seed, system, entity, slot); bounded ints; O(1) binomial | PLAN §4.7, SCALE §4 |
| `clock` | Ticks, 24,000 per day; fixed slots aligned to tick 0 | |
| `wheel` | Hierarchical timing wheel: O(1) schedule and pop, equal times in schedule order | PLAN §4.3 |
| `ladder` | The detail-ladder operations (fold, sample, query) and reusable law checks | SCALE §2 |
| `lazy` | Lazy regions: catch-up on read, slot commits, breakpoints on the wheel | SCALE §4, A15 |
| `conserve` | Totals, ledger, two-sided transfers, fixed-point error diffusion | SCALE §9, C13 |
| `dyadic` | Random counts over any interval in O(log horizon), the same for every reader (a prototype for O5) | O17 |
| `codec`, `page` | Page encoding; incremental page store (LZ4 + CRC32, manifest last, crash-safe); exact save codecs for arenas, wheels and lazy tables (`encode_with`, `decode_with`) | SCALE §8, PLAN §4.9 |
| `journal` | Observation sequences, operation ids, idempotent replay, command outbox | A9 |
| `profile` | Named zones, microseconds per system | |
| `store` | Agent storage facade (`RowStore`) over plain columns; P4 stays open | P4 |
| `synth` | Synthetic kinds for tests and the bench. Scaffolding, to be deleted | |

The industry engine's modules (`crates/rtsim-industry`):

| Module | What | Spec |
|---|---|---|
| `obs`, `cmd`, `wire` | Observations from the Java sampler, commands back, and their reference byte codec | docs/industry-boundary.md |
| `learn` | Windowed least-squares rates of `N = S - S0 - P`, the half-window steady test, learned targets and batch cycles | I5 |
| `cluster` | The replay: integer pools, all-or-nothing boxes and link transfers, exact or safe-side breakpoints, one `LazyModel` per cluster of coupled districts | C19, I6, I13 |
| `district`, `engine` | District state, model to replay member, load and unload, reconcile rounds and trims, trains and packages, power and channels, seasons, the read-only ledger | I7, I8, I11, I12, I15, I-A |
| `reconcile` | Pure per-inventory planning: proportional removals, slot-model additions, shortfall trims | I8 |
| `threat` | Measured pressure, the defence vector, Lanchester capped by observed fights, bounded damage, the leg index, a synthetic source | I19 to I25 |
| `deposit` | The Ore Excavation, Diesel oil and Destroy crude plugin interface | I11 |
| `fake`, `scenario` | A Java stand-in through the wire codec and the iron-line scenario. Scaffolding | |

```sh
./target/release/rtsim-cli sim industry      # the iron line, 10 days, five runs; JSON to bench/results/sim-industry.json
./target/release/rtsim-cli bench industry    # quiet and busy district sweeps, elapsed time; --quick, --check
```

The native host `mic_native` lives in its own repository (`mods/mic-native`, A18); rtsim-jni
depends on its runtime crate by git (docs/overnight-choices-jni.md J1). Not built yet: the module
registry the phase 1 crates will register with (PLAN.md §5.4), paging with a residency budget
(A16), and the parallel step.

## Build and test

Everything runs in the flake's dev shell (Rust 1.98.1, JDK 21). The dev shell only sees files git
tracks, so `git add` new files before `nix develop`. On Ben's laptop, wrap heavy jobs in the shared
lock:

```sh
nix develop -c flock /tmp/mic-rtsim-heavy.lock nice -n 10 env CARGO_BUILD_JOBS=6 cargo test --workspace
nix develop -c flock /tmp/mic-rtsim-heavy.lock nice -n 10 env CARGO_BUILD_JOBS=6 cargo clippy --workspace --all-targets -- -D warnings
nix develop -c cargo fmt --all --check
```

## The phase 1 scenario

```sh
nix develop -c flock /tmp/mic-rtsim-heavy.lock nice -n 10 env CARGO_BUILD_JOBS=6 cargo build --release -p rtsim-cli
./target/release/rtsim-cli sim phase1                     # 120 days, scripted player, JSON to scenario/results/
./target/release/rtsim-cli sim phase1 --days 60 --check   # conservation checked after every event
./target/release/rtsim-cli sim phase1 --trace 10          # what caused the last 10 days' market events
./target/release/rtsim-cli sim quiet --days 1000          # a closed world, no player: does it settle?
./target/release/rtsim-cli sim quiet --markets 1024       # the same on the bench's grid of villages
./target/release/rtsim-cli sim memory --markets 1024      # where the memory goes, per market
```

Six human villages, three ribbit villages beside a geode, a pillager haven raiding one road and a
player base trade for months; the run prints prices, stocks, treasuries, exchange rates, the
busiest flows, how far two robberies and a rescue have spread, the touring bands, houses grown, the
sorcerers' answers, a caravan trade at the player's base, and the size of the caravans on the roads.
Every number in the world is a PLACEHOLDER (P9, P18). `--rules` turns any of the quiescence rules
off for comparison (`rtsim_trade::Rules`; `--rules round1` is round 1's).

## The scale bench

```sh
nix develop -c flock /tmp/mic-rtsim-heavy.lock nice -n 10 env CARGO_BUILD_JOBS=6 cargo build --release -p rtsim-cli
flock /tmp/mic-rtsim-heavy.lock nice -n 10 ./target/release/rtsim-cli bench scale            # full: about a minute, JSON to bench/results/
flock /tmp/mic-rtsim-heavy.lock nice -n 10 ./target/release/rtsim-cli bench scale --quick    # seconds
nix develop -c flock /tmp/mic-rtsim-heavy.lock nice -n 10 tools/ci/scale-check.sh             # quick sweeps + --check
```

It varies one count at a time (latent individuals, regions, events per day, elapsed time between
reads, world age), runs each point in its own process, and measures CPU per sim day, heap and RSS,
full and incremental save bytes, and catch-up latency. It fits log-log growth slopes and compares each
with the slope its sweep declares; `--check` exits 1 when one is exceeded. Options: `--repeats N`,
`--only latent,regions,events,elapsed,age`, `--out DIR`, `--no-write`. `bench/results/README.md`
explains the committed results.
