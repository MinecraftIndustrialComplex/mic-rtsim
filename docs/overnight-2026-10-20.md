# Overnight report, 2026-10-20

What was built while Ben slept, what it measured, and what needs him first.

- **Answers wanted:** OPEN-QUESTIONS.md holds every open choice.
- **Choices made overnight:** every reversible choice made is logged in `docs/overnight-choices-*.md` and in DECISIONS.md (O1 to O19).

## What exists now

| Piece | Where | State |
|---|---|---|
| Engine kernel | `crates/rtsim-kernel` | Handles, Philox counter RNG, timing wheel, detail-ladder traits, lazy regions, integer conservation, page store, journal, profiler, save codecs |
| Scale bench | `rtsim-cli bench scale` | Sweeps each count across orders of magnitude; `--check` fails a breached slope |
| Native host | repo `mic-native` (private, MPL-2.0) | Foreground and background pools (background at nice 19), governor, IO lane, profiler, service registry, C ABI, `/micperf`; all 8 platforms build; Linux x86-64 run |
| Bridge | `neoforge/core` (mod `mic_rtsim`) | Puppets never saved by vanilla, stale-copy discard, groups as one unit, taking leaves the sim, save journal with op ids, module switches |
| Native link | `crates/rtsim-jni` | The bridge runs on the real engine through `mic_native` |
| Phase 1 engine | `rtsim-settle`, `-trade`, `-travel`, `-knowledge`, `-world` | Storehouse markets, two currencies, flows, caravans, news by travellers, sentiment, heirs, rescues, bands, sorcerer filter, ribbit growth, player bases |
| Phase 1 Minecraft side | `neoforge/core` and the `ribbits` and `carryon` plugins | Village adoption, treasury trades, gossip writes, llama-chest cargo, rescues, robbery, houses from YUNG's templates |
| Dragons (phase 2) | `crates/rtsim-ecology` (engine), `neoforge/iceandfire` (plugin `mic_rtsim_iceandfire`) | Lairs, hunger, scheduled legs, grudges, heirs, boredom, predation, raids on districts and trains, save and restore |
| Industry | `crates/rtsim-industry` (engine), `neoforge/core` industry module and the `create` plugin | Districts learned at the boundary, replay between breakpoints, exactly-once reconcile on load, threats, train ports, a ledger on a Create display |
| Persistence | phase 1, industry and dragons | Snapshot plus short log tail; restore gives the identical state hash |

## Numbers measured overnight

Measured on this laptop (Ryzen AI MAX PRO 390, shared with other sessions), not a weak PC except where a "potato" run is named.

**Hand-over cost on the server thread:** 11 to 20 µs per tick (p50), with phase 1 running on 64 villages and four players. The plan's budget is 1 ms.

**Engine start (restore)** for 1,024 villages after 100 days:

| | Time |
|---|---:|
| Before (replaying the log) | 4.3 s |
| After (snapshot) | 8.2 ms |
| 4,096 villages, after | 35 ms |

**Scale bench:**

| Measure | Result |
|---|---|
| CPU as latent individuals grow from 10⁴ to 10¹² | flat (slope 0.036) |
| Catch-up of a deterministic process | flat in elapsed time |
| Markets, 16 to 4,096 | slope 1.17, linear |
| Memory per market | 15.8 KB |

**Industry:**

| Measure | Result |
|---|---|
| Quiet districts | CPU flat at 13 to 14 µs per sim day up to 100,000 districts |
| Catch-up read | 430 ns, whether away 1 day or 1,000,000 days |
| Replay accuracy | 99.7% of a fully chunk-loaded control, never more |

**Dragons:** 10 to 14 µs per dragon per sim day, flat from 8 to 512 dragons.

**Economy:** a closed 11-village world goes fully quiet, from 36.5 to 0.00 events per market per day, after the round 2 fixes.

## Tests

- **Rust:** 331 workspace tests pass. Clippy and fmt are clean.
- **GameTests,** native engine and stub:

| Suite | Passing |
|---|---|
| core | 37 of 37 |
| iceandfire | 7 of 7 |
| ribbits | 5 of 5 |
| carryon | 1 of 1 |
| create | 14 of 14 |

- **The live caravan test:** a caravan from the engine's own flows appears at a player with real chest cargo, is traded with and robbed, each exactly once, and the robbery reaches two other villages' gossip days later.
- **Adversarial reviews** were run on each piece. Review 3, covering dragons and the phase 1 wiring, found 17 problems and fixed 16 with failing tests first.

## Needs Ben first (details in OPEN-QUESTIONS.md)

1. **The money sink** that made the economy settle (P9), and the caravan dock wait together with the report lifetimes.
2. **P3: coins duplicated through a killed llama's chest.**
3. **O5: random processes and elapsed time.** Slots, or the new dyadic counts.
4. **The industry stale-tick race:** four options.
5. **G31:** how vanilla's reputation discount stacks on sim prices. Also whether Java should keep its own copy of the price math.
6. **Dragons: M1 (breeding) was taken at its recommendation.** The In Control spawner change is a pack edit that waits for Ben; the edits are ready, disabled, in `neoforge/iceandfire/pack-proposal/config/incontrol/`.
7. **Plan facts the jars contradict:** `stealItems` is a pixie setting; the fire dragon is desert-only in effect; dragons do carry prey.

## Not done, or still running at the time of writing

- **Kernel follow-ups:** paging under a memory budget, two save generations (P29), the parallel step and the P4 storage bench. They are on branch `overnight/kernel2`, not yet merged.
- **Industry plugins:** PowerGrid, Destroy, Ore Excavation, Diesel Generators, Ender Transmission and threat sampling. Interrupted once by the API rate limit and resumed.
- **Not started:** ships, herds, conflict factions, kaiju, space and history generation.
- **API rate limits interrupted several agents once.** Every interrupted branch was saved as a WIP commit and pushed before resuming.
