# Overnight choices: phase 1 on the native engine (2026-10-20)

Reversible choices made while wiring the phase 1 engine (`crates/rtsim-world`: settle, trade, travel
and knowledge composed) into the native engine (`crates/rtsim-jni`), one line each with the reason.
None is in DECISIONS.md; each can be undone or put to Ben. Numbers marked PLACEHOLDER are not
measured. The code is `crates/rtsim-jni/src/engine/phase1.rs` and its folder; docs/boundary.md §5
item 10 sums it up.

## Structure

| # | Choice | Reason |
|---|---|---|
| V1 | Phase 1 lives in `engine/phase1.rs` (with `catalog.rs`, `cmds.rs`, `log.rs`), a child of `engine` beside `engine/industry.rs`; engine.rs calls it at the same points as the industry link (session start, latest-wins records, journaled records, simulate, write, save, orders), each a line next to industry's. | Both links need the step's private state (dictionary, sequences, `Replay`); one pattern for both keeps engine.rs a dispatcher and the merge mechanical. |
| V2 | The scripted phase 1 commands are orders 16 to 21 (MARKET, MERCHANT_OFFERS, WRITE_GOSSIP, SENTIMENT, PLACE_HOUSE, SPAWN_RESIDENT), written by the same writer as the sim's own decisions; 8 and 9 are industry's, 10 to 15 stay free. `ScriptedEngine` carries the stub's phase 1 methods, so the 11 GameTests that skipped on the native engine run on both. | The tests keep their scripts and exercise the native writer, dictionary, sequences and outbox (J21's approach). |
| V3 | A test's village is marked with order 22 (`scriptVillage`, sent by `RtsimTests.adoptArena` before the adoption): the engine applies none of its observations and sends no command of its own for it. The set is saved in the pages. | A scripted MARKET or SENTIMENT would otherwise be overwritten by the engine's own; the adoption of an earlier run's arena village replays from the journal in a later session and must stay ignored. |

## Observations

| # | Choice | Reason |
|---|---|---|
| V4 | A TRADE is booked as the counts the bridge reports (`World::trade_exact`, `caravan_trade_exact`, new in `rtsim-world/src/bridge.rs`), never quoted again; where the sim's store holds less than the bridge moved, the rest is not booked and the input is flagged clamped. A caravan's chests hold its flow cargo and its purse together, so a sale takes from the cargo first, then the purse (and the other way for a payment). | boundary.md §4.1: "the counts are what was really paid and received". Re-quoting could refuse a trade the player already made, or book other numbers than the chest shows. |
| V9 | The first VILLAGE_CENSUS of a village sets its population (its adoption guessed one from its pieces); later censuses only raise it. | Growth books a resident before it exists (P17), and a census of a partly loaded village counts low: lowering the population on every census would undo growth and wobble the targets. |
| V24 | HURT and RESCUE by mobs or other entities are ignored; a resident hurt or killed by a player is a village report even when flagged VANILLA_GOSSIP (rtsim-knowledge never writes back what vanilla saw first-hand, E36, so nothing is echoed). | The report still moves the sim's sentiment and travels as news to villages vanilla knows nothing of. |
| V25 | CARGO_CHANGED: items the sim does not trade are ignored (they were never in its books); negative deltas are a robbery (by the player named, else goods simply leave); positive deltas enter the caravan's cargo from the player's side. | The robbery is what matters (C24); the rest keeps the totals honest. |
| V27 | VILLAGE_GOSSIP is read as latest state and not acted on. | rtsim-knowledge keeps its own record; what to do with vanilla's sums (seeding sentiment, a check) is a later question. |

## The world and its catalogue

| # | Choice | Reason |
|---|---|---|
| V5 | PLACEHOLDER catalogue (`catalog.rs`): rtsim-world's synthetic goods and recipes, mapped onto vanilla items (`wool` is `minecraft:white_wool`); a village's resources from its structure (plains farm and keep cattle, taiga shears and smelts, desert farms a little and mines emeralds, snowy smelts and shears, savanna keeps cattle, ribbit villages work the swamp and a geode); its population before a census a third of its pieces (human) or half (ribbit), 4 to 30; the world's seed a constant (`0x6d69635f7031`); Ribbits' 14 house templates in name order; a band puppet has the plugin's 4 members (the world counts 3, E57). | Nothing the bridge observes says what land a village works or how many live there before a census; the synthetic catalogue is the one the economy was tuned on. P9 and P18 replace these. |
| V28 | The phase 1 world runs to the hand-over's game time once per step (`simulate`), and every input is applied at its record's game time or the world's, whichever is later. | The world is event-driven, so a quiet step costs nothing, and an input never lands in the world's past. |

## Saves (C9, PLAN.md §4.9)

| # | Choice | Reason |
|---|---|---|
| V6 | The pages keep the phase 1 world's **input log**, not a snapshot: every input (a join, a population, a trade, a promotion, a robbery, a member taken or killed, a report, a base, a house placed) with the world tick it was applied at. `start` rebuilds the world by replaying it and checks the replayed `state_hash` against the one the save wrote (a mismatch is logged and the replayed world stands). | rtsim-world has no decoder for its markets, pipelines, knowledge or wheels; the world is deterministic, inputs are small and a quiet world replays fast (6 inputs in about 1 ms in the protocol test). A snapshot codec is the follow-up (see Incomplete); the hash check catches a build that reads the inputs differently. |
| V7 | Phase 1's pages are table 0x5000: page 0 the meta page (world tick, hash, the test offset, the next epoch, scripted villages, the kept commands), pages 1 and up the log in chunks of 1,024 inputs; a save writes the meta page and only the chunks that changed. | Its own table, as industry has 0x4900 to 0x4902; the log only grows, so a save writes one or two small pages. The kernel follow-up's paging and two save generations were not on origin/main when this merged. |
| V14 | Op ids from identity (§2.2): a house's is its village's nth house (from rtsim-world's growth seed), a resident's its house and place in it, a gossip write's its village, player, report kind, report time and gossip type. `World::grow` and `house_placed` now derive them (`rtsim_world::identity_op`) instead of counting. | A counter rebuilt from the pages hands its ids to different commands after a crash, and the bridge answers those DUPLICATE. |
| V16 | WRITE_GOSSIP, PLACE_HOUSE and SPAWN_RESIDENT (scripted or decided) are kept in phase 1's outbox until their COMMAND_RESULT, saved with the pages, and resent after a restart under their sequence; the step merges the core's, industry's and phase 1's resends in sequence order. MARKET, MERCHANT_OFFERS, SENTIMENT and puppet commands are per session (latest wins, decided again). | Java skips a command at or below the last sequence it applied, so resends must rise across all three queues. |
| V17 | HOUSE_PLACED is not filtered by op id (its op is the PLACE_HOUSE's, whose COMMAND_RESULT follows and must reach the outbox); a repeat is a no-op by state (the house is no longer pending). TRADE and CARGO_CHANGED go through `Replay` with their op ids. | The kernel remembers each op id once; the second record with the same id would have been refused. |

## Commands

| # | Choice | Reason |
|---|---|---|
| V10 | MERCHANT_OFFERS are priced fair (rtsim-world's `caravan_offers` with no player's markup) and sent once, right after the SPAWN_GROUP; a caravan too small to fill a bundle sends none. | MERCHANT_OFFERS is per puppet, not per player; refreshing on news is a later step. |
| V15 | WRITE_GOSSIP amounts below 2 are sent as 2 (a hearsay trade, 2 at half weight); FIRST_HAND comes from rtsim-knowledge, whose `VanillaGossip` now carries the report's kind, time and whether it was first-hand. | Vanilla drops an entry below 2 (boundary.md §4.3). The report kind was read from the report table, where a robbery expires after 1.5 days, before the write went out. |
| V18 | With knowledge, settlements, trade and travel all off (the MODULES mask) phase 1 decides nothing new; captures and kept commands still go. | A module switch must stop the decisions, not strand a puppet or lose an answer. |
| V23 | MARKET goes to villages within 192 blocks (PLACEHOLDER) of a player, every 100 game ticks (PLACEHOLDER) when its fingerprint (treasury, every good's stock, target and anchor) changed, and right after a resident's trade there; it lists every good the catalogue trades but money; a newer one replaces an older one still queued. | The bridge prices only on a trading screen; sending every village's snapshot every quantum would fill the buffer for nothing. |

## Caravans near players (C21, C25)

| # | Choice | Reason |
|---|---|---|
| V8 | PLACEHOLDER tunables: a pass over players every 20 game ticks; travellers within 96 blocks materialize, at most 2 groups per player and 16 in all; a group no player is within 144 blocks of is captured back; steering every 100 ticks at speed 0.6; a player unreported for 200 ticks is gone; a rejected spawn backs off for 6,000 world ticks. | Inside the entity-ticking area of a default view distance, few enough to cost nothing per tick. |
| V11 | A traveller that materializes is promoted to an individual row first (`World::promote`, logged as an input). | Its trades, robberies and captures need a stable identity; promotion keeps the flows' totals exact (SCALE.md §3). |
| V12 | A caravan spawns at its sampled road position at the player's height with SPAWN_GROUP's new `FLAG_SURFACE` (each member on the surface at its own x, z, found once the position ticks); llamas at strength 1 (3 chest slots), cargo in whole items chest by chest, the purse in the last carrier, what does not fit staying in the sim; a group with a member gone is not materialized again. | The engine knows no terrain; whole items only exist in chests; the bridge spawns whole groups. |
| V13 | A materialized caravan is captured back when its sim row's leg changes (it arrived, turned back, or dissolved) or no player is near; what its members carried is compared with the row and a difference is logged and counted, not booked. | The row keeps walking in the sim while the puppet walks in the world; once the sim says it arrived, the puppet must not keep selling the old cargo. Reconciling unexplained differences waits for a rule (every change should have come as TRADE or CARGO_CHANGED). |
| V26 | Phase 1 agent ids are bit 62 over 55 bits of the caravan's or band's identity; epochs a counter saved with the pages. | Test agents sit below 2^40 and `/rtsim stub` ones at 2^40; an epoch must be new for every materialization, across sessions too. |

## Tests

| # | Choice | Reason |
|---|---|---|
| V19 | Two TEST-mode orders: 23 `P1_SPAWN_NEAR` (the caravan with wares nearest a search point, materialized at a chosen place) and 24 `P1_ADVANCE` (the world runs that many more ticks ahead of game time; the offset is saved). | A real caravan leaves only when its dock fills or after two days, so a test cannot wait for one to walk past; these exercise the engine's own flows, cargo, offers and news without waiting days. |
| V20 | Inspection: `nPhase1Stats` (16 counters, any mode), `nPhase1Sentiment` (TEST mode, from the published inspection), and `Engine::phase1_state` (the hash, the replay check, capture mismatches; Rust tests only, it takes the state lock). | No game thread ever waits on a lock a step holds. |
| V21 | The live GameTest's villages sit at a random place two million blocks out, and the caravan is materialized at the arena; native engine only (the stub decides nothing). | The GameTest world and the engine's pages are reused (B37): earlier runs' villages near the arena would otherwise be the new ones' neighbours. |
| V22 | The Rust fake Java side (`tests/common`, shared with industry) answers phase 1's kept commands (HOUSE_PLACED, COMMAND_RESULT), dematerializes on DESPAWN_GROUP and reports players every tick. | The protocol test runs phase 1 over the real buffers, host pool and IO lane. |
