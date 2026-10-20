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
| `crates/rtsim-cli` | `rtsim-cli bench scale`, the scale bench of SCALE.md §10 (now with market and edge sweeps); `rtsim-cli sim phase1`, the phase 1 scenario. |
| `crates/rtsim-settle` | Settlements as rows: two cultures, villages, player bases and havens as market nodes, edges by P18's recommended rule (PLAN.md §8.2). |
| `crates/rtsim-trade` | Storehouse markets (C13): labour costs with Hawkins-Simon, anchors, price as a function of stock, the exact player trade integral, the line law in closed form, money, exchange lines, offers through the treasury (C14, C23). |
| `crates/rtsim-travel` | Flows on edges as pipelines with holes, the caravan lattice, news snapshots, allocation by margin, caravans and groups, the leg index, raids to havens (SCALE.md §3, PLAN.md §8.6). |
| `crates/rtsim-knowledge` | Reports as 32-byte rows, witnesses, sentiment with lazy decay, noticeboards and gossip packets, vanilla gossip writes, notables and heirs, temperament (PLAN.md §5.6, §5.7, §8.7). |
| `crates/rtsim-world` | The phase 1 composition: build and join, the step loop, flows and payments, news at the speed of trade, caravans near players, player trades, conservation and state hash. Scaffolding until the module registry exists. |
| `scenario/results/` | Output of `rtsim-cli sim phase1`. |
| `docs/overnight-choices-phase1.md` | The phase 1 engine's reversible choices (E1 to E42), for Ben's review. |
| `tools/ci/scale-check.sh` | The CI-style scale check: fails when a growth slope exceeds its contract. |
| `bench/results/` | Committed bench results, with the machine and conditions. |
| `neoforge/` | The Java side: the core bridge mod `mic_rtsim` (puppets, journal, module switches, vanilla bodies) with a pure-Java stub engine; GameTests. See `neoforge/README.md`. |
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
| `codec`, `page` | Page encoding; incremental page store (LZ4 + CRC32, manifest last, crash-safe) | SCALE §8, PLAN §4.9 |
| `journal` | Observation sequences, operation ids, idempotent replay, command outbox | A9 |
| `profile` | Named zones, microseconds per system | |
| `store` | Agent storage facade (`RowStore`) over plain columns; P4 stays open | P4 |
| `synth` | Synthetic kinds for tests and the bench. Scaffolding, to be deleted | |

Not built yet: the native host `mic_native`, the JNI crate (rtsim-jni), the module registry the
phase 1 crates will register with (PLAN.md §5.4), paging with a residency budget (A16), and the
parallel step.

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
```

Six human villages, three ribbit villages beside a geode, a pillager haven raiding one road and a
player base trade for months; the run prints prices, stocks, treasuries, exchange rates, the
busiest flows, and how far a robbery and a rescue have spread. Every number in the world is a
PLACEHOLDER (P9, P18).

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
