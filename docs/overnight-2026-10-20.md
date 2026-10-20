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
| Ships | `crates/rtsim-ships` (engine only; no plugin yet) | Sea and sky lanes, Veloren-style loop routes, timetabled ships that survive restarts, merchant flows in holds, pirates with havens, navy patrols lowering lane hazard, Lanchester raids never better than observed |
| Herds | `crates/rtsim-ecology` herds module, `engine/herds.rs`, vanilla adapter in `neoforge/core` (`herd` package) | Migrating herds of tens of thousands with latent members, identity derived from seeds, dyadic deaths, forage, obstacle detours, dragons hunting and following herds |
| Conflict | `crates/rtsim-conflict`, `engine/conflict.rs` (no Java plugin: bodies are P8) | Factions with culture tolerances, grievance per actor (P14: direct acts per player, pollution to a shared settlers actor), the escalation ladder from notice to siege, envoys with demands, tribute, war bands with heirs and boredom, offline raids through the industry threat interface, village raids to the haven |
| World model | `crates/rtsim-space`, `engine/worldmodel.rs`, the core's `habitat` package (HABITAT, docs/boundary.md §10: a boundary addition for Ben's review) | The engine knows terrain: 64-block cells sampled from chunk centres (water, ice, biome and tags, temperature, the biome's own creature spawns), sent once and again only on change; herds' forage, temperature and terrain from real biomes (water and unsampled ground closed), one herd founded per suitable region; ships' sea lanes over sampled water only; a provider trait a Deep Time reader can replace |
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

**World model:** the habitat sampler reads a 64-block cell in 8.5 µs on average (0.53 µs a chunk; 229 µs the first time a biome is seen in a session), and its chunk-load hook costs 1.3 µs a load ([bench/results/habitat-sampler.md](../bench/results/habitat-sampler.md)); a fully sampled region costs the engine about 6 KB.

**Herds:** six herds of 33,000 animals in total over two sim years in 0.8 s; cost per sim day flat from 100 to 1,000,000 animals per herd (slope −0.012), and a read after 10,000 unread days costs 0.02 to 0.04 µs.

**Ships:** the engine's own cost stays flat at 17 to 19 µs per sim day from 56 to 814 merchant ships; unobserved merchant ships cost nothing.

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
7. **Ships T5:** where a road and a sea lane join the same two markets, the two now share the destination's demand (best margin first). This changes phase 1 behaviour only in such worlds.
8. **K2-13 (conflict):** the industry threat interface's per-class defaults let ground raiders carry and break nothing; the conflict wiring is taking the recommendation (per-threat values from the attacking units).
9. **Plan facts the jars contradict:** `stealItems` is a pixie setting; the fire dragon is desert-only in effect; dragons do carry prey. (Fixed in PLAN.md and INDUSTRY.md on the morning of 2026-10-20.)

## Not done, or still running at the time of writing

- **Kernel follow-ups merged (5081491):** paging under a memory budget (resident pages flat at 4.0 MiB from 100,000 to 100 million regions), two save generations, an incremental save index, a SIGKILL crash test, the parallel step (same hash at 1 to 16 workers; 6.07x at 8 workers with realistic work per event), the P4 storage bench, and potato-profile runs. Two review bugs are open: two saves in flight can lose a page's changes, and an IO error inside a parallel step drops that step's wakes and messages. The modules are not yet moved onto paging.
- **Industry plugins merged (6efb288, morning):** Ore Excavation and Diesel deposits, Destroy pollution by position and crude oil, PowerGrid batteries and measured power links, Ender Transmission channels joined into one district (I13 as corrected), climate (Serene Seasons, mic-climate), radar, IND_WRITE_PACKAGE, threat sampling and the defence survey. GameTests pass on both engines for every plugin; native core passes 50 to 51 of 52 under load (a different phase 1 or herd test times out each run). Choices Q1 to Q26 are in `docs/overnight-choices-indplugins.md`. Left for the engine: retiring the channel-stock records, level-wide pollution live while unloaded (I14), IND_CLIMATE.
- **Done since:** ships run through the native link; one interest area per player is shared by phase 1, dragons and ships; module switches exist for dragons and ships; traders price pirate hazard on lanes (368 Rust tests pass).
- **Done since:** herds run on the native engine as vanilla animals (401 Rust tests; GameTests core 45/45, carryon 2/2, iceandfire 7/7 on both engines). No herd appears in a real world yet: only a test order founds one.
- **Done since:** the conflict engine (423 Rust tests). In its scenario a goblin tribe escalates against a polluting refinery to an offline raid applied on load; paying tribute while the smog runs becomes a protection racket; a catalytic converter ends the feud.
- **Done since:** conflict runs in the native engine (450 Rust tests): a district's learned smog feeds goblin grievance, an unwatched war band raids it, and the damage lands once across a crash. K2-13 was applied as an optional per-threat load. Refusing a demand now counts as refused (it used to forgive the grudge).
- **In progress:** a world-model sampler (HABITAT record) so herds are founded in real worlds and ships get a real water mask.
- **Not started:** plugins for ships and conflict (body mods are open, P8), kaiju, space and history generation.
- **Phase 1 rework merged (2d8764d, morning):** news by messengers (abstract flows, four a day, 1.5x caravan speed), longer report lifetimes, band circuits recomputed at home and touring player bases in good standing, Review P3 (a lost llama books exactly its chest), P21 coin exchange at border markets, U10 (phase 1 runs only with all four modules on), the engine sends ready-made offer bundles with reputation folded into the quote (G31). Choices PR1 to PR14 in `docs/overnight-choices-p1rework.md`. PR1 removes vanilla's positive reputation and Hero of the Village discounts on sim-priced offers (a two-sided discount is a money pump); Ben may object. Open: with phase 1 off, the other modules still advance its clock (U10 leak); band members taken or killed are not booked; caravan offers have no per-player price levels.
- **Industry and conflict rework merged (30e4861, morning):** learned coupling (I6), the unloaded part of a partly loaded district simulated (I7), reconcile ahead of arrival in the engine (the I-B stale-tick leak is gone: 9,269 ingots against the control's 9,296, never above), spawn-fed farms (I9), channel records retired (I13), level-wide pollution live while unloaded (I14), ledger and read-only mode removed (I18), the module switch (U10), pollution ownership by time spent per chunk (P14). 496 Rust tests pass. Choices IR1 to IR20 in `docs/choices-indrework.md`. Java still owes: applying the held reconcile at chunk load, IND_CHUNK_UNLOADED, U10's Java side, I9's code 6, and the ledger protocol's removal (the ledger removal agent took all but the first).
- **Glass box G1 started** after the rework merged: compile and solve, read only, on a corpus of 12 Create factories plus a sheep wool farm (C32).
- **I18 landed (9ba0fa5):** no player-facing ledger. The Create display source and chat ledger are gone, IND_LEDGER_REQUEST and IND_LEDGER are retired, `Ledgers` is `DistrictDiagnostics`; plugin warnings go to the server log and the operator-only `/rtsim district info|debug` (permission 2). Also IND_CHUNK_UNLOADED, U10's Java side (silent while off, a full announce when on) and I9's code 6. GameTests core 52/52, create 19/19, endertransmission 1/1, powergrid 2/2 on both engines.
- **OG-1 applied (f55af8c):** natural and spawner dragons (In Control's desert dragons) are left alone until the oil gate is measured; `adopt_natural_dragons` defaults false.
- **Designs written for Ben:** OILGATE.md, SINK.md, VOYAGE.md (f20d5a3) and VIEWER.md (277d251, with Deep Time's review applied); a combined Spacesheep briefing with an answer sheet is being built.
- **Started:** reconcile ahead of arrival (Java side) and repair schematics (I1).
- **API rate limits interrupted several agents once.** Every interrupted branch was saved as a WIP commit and pushed before resuming.
