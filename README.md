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
| `crates/rtsim-cli` | `rtsim-cli bench scale`, the scale bench of SCALE.md §10. |
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
| `codec`, `page` | Page encoding; incremental page store (LZ4 + CRC32, manifest last, crash-safe) | SCALE §8, PLAN §4.9 |
| `journal` | Observation sequences, operation ids, idempotent replay, command outbox | A9 |
| `profile` | Named zones, microseconds per system | |
| `store` | Agent storage facade (`RowStore`) over plain columns; P4 stays open | P4 |
| `synth` | Synthetic kinds for tests and the bench. Scaffolding, to be deleted | |

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
