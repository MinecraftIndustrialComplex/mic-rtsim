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
| `crates/rtsim-cli` | `rtsim-cli bench scale` (SCALE.md §10), `bench bsp` (the parallel step and its determinism gate), `bench o5` (slot commits against dyadic counts). |
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
| `codec`, `page` | Page encoding; incremental page store (LZ4 + CRC32, crash-safe): a log record per save, a paged location index, two save generations | SCALE §8, PLAN §4.9, P29 |
| `paging` | Lazy regions paged to the store under a memory budget: LRU eviction, load on read, prefetch from the wheel, read-ahead hooks | A16, SCALE §8 |
| `exec`, `bsp` | The executor the caller hands in (the engine owns no threads), and the bulk-synchronous parallel step: same state at any worker count | SCALE §5, PLAN §4.7 |
| `journal` | Observation sequences, operation ids, idempotent replay, command outbox | A9 |
| `profile` | Named zones, microseconds per system | |
| `store` | Agent storage facade (`RowStore`) over plain columns; P4 stays open | P4 |
| `synth` | Synthetic kinds for tests and the bench. Scaffolding, to be deleted | |

Not built yet: the native host `mic_native`, the NeoForge bridge and JNI crate, the generic modules
(markets, flows, caravans, knowledge). The kernel follow-ups of the second overnight run (paging,
saves, the parallel step) and their open ends are in `docs/overnight-choices-kernel2.md`.

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
full and incremental save bytes, and catch-up latency. The `paged` sweeps run the same world on
paged regions at a fixed memory budget. It fits log-log growth slopes and compares each with the
slope its sweep declares; `--check` exits 1 when one is exceeded. Options: `--repeats N`,
`--only latent,regions,events,elapsed,age,paged`, `--out DIR`, `--no-write`, `--tag NAME`.
`bench/results/README.md` explains the committed results.

```sh
flock /tmp/mic-rtsim-heavy.lock nice -n 10 ./target/release/rtsim-cli bench bsp --check   # the determinism gate, 1 to 16 workers
flock /tmp/mic-rtsim-heavy.lock nice -n 10 ./target/release/rtsim-cli bench o5            # O5: slots against dyadic counts
# The potato profile (A11): two physical cores (not SMT siblings; see lscpu -e), nice 10
flock /tmp/mic-rtsim-heavy.lock nice -n 10 taskset -c 2,4 ./target/release/rtsim-cli bench scale --tag potato
```
