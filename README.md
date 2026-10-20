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
| `crates/rtsim-industry` | The industry engine (INDUSTRY.md, C15 to C19), headless: learning, replay, reconciliation, links, deposits, threats, the ledger. Wire records in `docs/industry-boundary.md`; choices in `docs/overnight-choices-industry.md`. |
| `crates/rtsim-cli` | `rtsim-cli bench scale`, the scale bench of SCALE.md §10; `bench industry` and `sim industry`. |
| `tools/ci/scale-check.sh` | The CI-style scale check: fails when a growth slope exceeds its contract. |
| `bench/results/` | Committed bench results, with the machine and conditions. |

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
| `codec`, `page` | Page encoding; incremental page store (LZ4 + CRC32, manifest last, crash-safe) | SCALE §8, PLAN §4.9 |
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

Not built yet: the native host `mic_native`, the NeoForge bridge and JNI crate, the generic modules
(markets, flows, caravans, knowledge), paging with a residency budget (A16), and the parallel step.

## Build and test

Everything runs in the flake's dev shell (Rust 1.98.1, JDK 21). The dev shell only sees files git
tracks, so `git add` new files before `nix develop`. On Ben's laptop, wrap heavy jobs in the shared
lock:

```sh
nix develop -c flock /tmp/mic-rtsim-heavy.lock nice -n 10 env CARGO_BUILD_JOBS=6 cargo test --workspace
nix develop -c flock /tmp/mic-rtsim-heavy.lock nice -n 10 env CARGO_BUILD_JOBS=6 cargo clippy --workspace --all-targets -- -D warnings
nix develop -c cargo fmt --all --check
```

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
