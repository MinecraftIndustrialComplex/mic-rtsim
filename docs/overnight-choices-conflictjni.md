# Overnight choices: the conflict engine in the native engine (2026-10-20)

**Overnight agent 24.** Reversible choices made while Ben was asleep, one line each with the reason,
numbered C2-#. None is in DECISIONS.md, PLAN.md or OPEN-QUESTIONS.md; each can be undone or put to
Ben. Branch `overnight/conflict-jni`. The work: the conflict engine (`crates/rtsim-conflict`,
choices K2-1 to K2-32 in docs/overnight-choices-conflict.md) wired into rtsim-jni
(`crates/rtsim-jni/src/engine/conflict.rs`, docs/boundary.md §5 item 15 and §9), fed its signals
inside the engine, and K2-13's recommendation applied to the industry threat interface. Numbers
are PLACEHOLDER unless marked measured. No Java.

## The threat interface (K2-13)

| # | Choice | Why |
|---|---|---|
| C2-1 | **K2-13's recommendation, as the smallest change**: `rtsim_industry::threat::Threat` gains `loads: Option<Loads>` (`carry` items and `blocks` broken, totals over the attackers, of which the survivors' share applies, floored). `None` keeps the per-class tunables, so natural mobs, dragons and ships are unchanged (their 11 literals get `loads: None`; the field is skipped in JSON when `None`). The conflict's war bands set it per class from their units: the culture data's `carry` and a new `breaks` per unit (serde default 0; goblin warrior 1, brute 1, bandit thug 1, raider vindicator 1, ravager 1, the rest 0, PLACEHOLDER); a train ambush carries the band's whole carry and breaks nothing. The scenario refinery's per-class override (ground melee 8 items and 1 block, ranged 4) is gone: the units carry the same. | The brief: threats carry and break per threat from the attacking units. Per class totals keep I25's per-class cooldown meaning one attack per class. **`sim conflict` measured**: every event, rung, raid (2 blocks, 20 items stolen offline) and check identical to the committed results; only the conflict's state hashes change (its save holds the culture fingerprint, which now covers `breaks`); the world's hashes are unchanged. `scenario/results/sim-conflict.json` refreshed. |

## The module in the native engine

| # | Choice | Why |
|---|---|---|
| C2-2 | **`engine/conflict.rs`**, beside `ships.rs` and `herds.rs`, which it mirrors: one-line hooks in `engine.rs` at observe (SESSION_START, the latest-wins arm, the `Replay::Apply` arm, `op_of`), simulate (after the herds), write (after the herds' queue), save (its page rides with the industry, dragon, ship and herd pages, deleted by table when gone), start and orders. Pages: table **0x4300** ('C', reserved by K2-26), one page (`RTCF` version 1, the state hash, `Conflict::save`), checked on load; dropped loudly (the conflict starts afresh) when unreadable or without the phase 1 world it was saved with. While the conflict is active (a faction, a source or a party) it taints the phase 1 link, which then snapshots at every save in the same commit. Seed: `CONFLICT_SEED` (PLACEHOLDER, as V5, S14, U1 and Y3). | The brief: wired the way the others are. The conflict lives in the phase 1 world (havens are its nodes, marches its legs, chiefs its notables), so it shares that world's pages' fate. |
| C2-3 | **Every conflict command stays per session** (K2-27): sequences from the core's counter after every other module's queue, none kept or resent, none with an engine op id. Where items do move, the brief's "op ids from identity, resends in sequence order" holds by the modules that move them: Java's op ids on CHIEF_KILLED, TRIBUTE_PAID, GIFT_GIVEN and BAND_LOOTED go through `Replay::admit` (`conflict::op_of`) and the conflict's own applied set; a district raid's theft and broken blocks are the industry engine's CHUNK_RECONCILE, with its op ids from identity (allocator 0x49) and its resends merged in sequence order (W-choices). | No conflict command moves an item (tribute, gifts and loot are the plugin's records). Tested across a crash (C2-14). |
| C2-4 | **Ids.** Java's ids are translated to the conflict's own before `Conflict::ingest`: items to the phase 1 catalogue's goods (`item_of`; an item no good stands for is dropped, U16's rule), bodies to the culture data's (`body_id`; unknown is 0), structures to the conflict's structure ids (every culture's camp structures in data order), pollution types to the tunables' table order. The conflict's own dictionary is defined once, when the link makes the engine, so its saved state never depends on Java's numbering. Dimensions are Java's ids (the conflict's frames). Commands name the dimension, bodies and the tribute item through the engine's dictionary (DICT first); an envoy or band in a dimension Java never named, or naming a body with no name, is answered as Java would (BAND_SPAWN_REJECTED 3 or 9; 6 when it can never fit) and never written; a TERRITORY or FACTION_NOTICE in such a dimension is dropped (TERRITORY goes again at the next session start). An envoy's deadline and a steer's arrival are game time (the world's tick minus a test's P1_ADVANCE offset); a test order's ticks are game time already. | The ships' U16 and the herds' rules; the deadline Java shows a player must be in Java's clock. |
| C2-5 | **The shared services** (`engine/shared.rs`): the conflict is the fifth module there: a `rtsim_world::interest::Viewer` in `with_viewers` (after the herds; its legs are owner bit 59), its next event in `run_to`'s rounds, its view radius in the shared radius, and `shared::introduce_conflict` (as Y1's `introduce_herds`) when it starts running in the middle of a session (a first FACTION_FOUND, the switch turned on), so a player standing still sees an envoy that is near. `conflict::run_engine` never runs behind its own present: an overdue event runs at once rather than holding the clock's rounds at its time. | The brief: war bands and envoys entering a player's view through the shared interest service. A module that only returns early when behind its present could stall `run_to` if an event were ever booked behind it. |
| C2-6 | **The switch is `Module.CONFLICT`** (bit 5, Java's ordinal), with U10's rule unchanged (U10 is open): off, the conflict ignores its records (still admitted by `Replay` for their sequence and op id, then counted), runs nothing, sees no area, writes nothing (its queue is dropped) and keeps the page it had; on again, its bodies of before are gone (its session reset, TERRITORY again for every faction) and it runs on from where it stopped. | The brief: the current switch rule, the dragons' and ships' behaviour. |

## Signals inside the engine

| # | Choice | Why |
|---|---|---|
| C2-7 | **Pollution from the industry engine** (K2-9's unloaded path, and I14's attribution): at each simulate point once a faction exists, every district's learned model for the current season gives the positive rate of each deposit good whose registry id (before the `/`) is a Destroy pollution type in the tunables' table: the Destroy plugin's attribution as the industry plugins branch writes it, `d/0/destroy:smog/<hash of the district>` (Q9). Per day, scaled, it is the settlers' source keyed by the district's id (the key a POLLUTION record uses), at the district's centre. It counts while the district runs: loaded (unless a conflict plugin reported its POLLUTION this session), or unloaded with its replay running; a stopped replay (a machine lost to a raid, a frozen district) emits nothing. A source changes only when its rate does (compared exactly with the rate the conflict keeps), so a steady district costs no event and a restart derives the same sources from the same pages. | No conflict plugin exists; the industry engine already learns each district's pollution, loaded or not, so one feed serves both states. **Measured** in the native test: the mill's 1 smog a batch every 100 ticks was learned as 0.01 a tick, fed as 240 a day, and stopped when the raid took its machine. The industry plugins branch had not merged into main when this branch finished, so in game nothing writes those goods yet. |
| C2-8 | **Mining from drained deposits**: what an unloaded district's replay drains of any other deposit (a negative learned rate) is the settlers' mining source, one signal unit per deposit unit (an Ore Excavation extraction cycle), crude oil (`destroy:crude_oil`, `createdieselgenerators:oil`) one per 1,000 (millibuckets, UNVERIFIED). PLACEHOLDER. While loaded the plugin counts mining itself (TERRITORY_SIGNAL). | docs/boundary.md §9.3: ore drained by an unloaded district reaches the engine from the industry engine's deposits. Deposit units differ by mod; a bucket of oil as one block of ore keeps a pumpjack from starting a war in an hour. Not covered by a test (the fake district holds one deposit, used for smog). |
| C2-9 | **Territory signals**: TERRITORY_SIGNAL goes to the conflict as the plugin sends it (P14: the acting player's UUID, (0, 0) the settlers). In the conflict crate, `Conflict::session_start` now sends TERRITORY for every faction (replacing any queued one) and a watched fight whose bodies went with the session goes on abstractly at its next event (Fighting to Waiting, as when its player leaves). | docs/boundary.md §9.4: TERRITORY again at every session start was the native engine's job; the plugin forgets territories with the session. |
| C2-10 | **Raids on villages and havens** need no wiring: the conflict runs on the phase 1 world itself, so a village raid moves stock with `World::stock_to_held` and lands it with `World::land_held` in the world the phase 1 link saves, and the phase 1 market pass resends a changed village's MARKET by its fingerprint. | The brief: through the phase 1 world. Not exercised by a native test (no village of a hated culture in the test world; plunder comes every 12 days on average). |
| C2-11 | **BAND_STATE is counted, not read** (as the herds' HERD_GROUP_STATE, Y19). | A party's place is its march's closed form; the record is there for a later drift correction. |
| C2-12 | **A refused demand is never a cut signal** (`rtsim-conflict`): ENVOY_ANSWER refused or attacked now settles the demand as unmet with DEMAND_SETTLED outcome 4 (refused), scouts if grievance is at their threshold, unless it was paid in full. | Found by the protocol test: a refusal was an expiry, and an expiry with no new impulse of the demanded signal (killing, logging) counted as "the signal stopped", so four refused demands in a row were each settled as met, a share of the grievance forgiven each time. `sim conflict` never refuses, so it is unchanged. |
| C2-13 | **District raids reach the industry engine only while its module runs** (`IndustryLink`; otherwise `NoInfrastructure`, and a raid finds nothing to raid), as the dragons and the ships. | One rule for every module that raids districts. |

## Tests

| # | Choice | Why |
|---|---|---|
| C2-14 | **The fake Java side plays a conflict plugin** (`tests/common`: `Java::conflict_obs`, `conflict_flush`, `conflict_command`), with `rtsim_conflict::synth::FakeConflict` answering commands through the reference codec, ids translated to Java's. Rust only: no conflict plugin exists (P8). | As the ships' and herds' fakes. |
| C2-15 | **CONFLICT_COMMAND, order 29 (TEST mode)**: a conflict command record in Java's ids (bodies, items, the dimension) and game time, with the dictionary entries it names, queued as the conflict's own (its sequences and DICT records). The conflict knows nothing of its agent. Rust only. | As DRAGON_COMMAND (Z2), SHIP_COMMAND (U15) and HERD_COMMAND (Y5): a plugin's GameTests drive the Java side without days of game time. |
| C2-16 | **Inspection**: `Engine::with_conflict`, `conflict_restore_check`, `conflict_stats` (the engine's counters and the link's) and `conflict_log(keep)` (the conflict's own log, in memory and never saved, for tests and `/rtsim`). | The other modules' pattern; the log made the C2-12 bug readable. |
| C2-17 | **The fake industry world's deposits can rise**: a recipe's negative `deposit_per_batch` adds to its district's deposit (minted in its books), so a test district emits as the Destroy plugin's attributed pollution does. | A deposit-class good cannot be an inventory output in the fake (stack size 0), and the real attribution is a deposit sample, not stock. Scaffolding. |

## Results

- **Rust, measured**: `cargo test --workspace` (see the branch's last commit message for the count),
  `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` clean.
- `rtsim-cli sim conflict`: unchanged except the conflict's state hashes (C2-1); every check passes.
- New native tests: `factions_take_notice_send_envoys_take_tribute_once_and_raid_in_waves_across_a_crash`
  (protocol.rs: a camp joins as a haven with its TERRITORY; 300 logs raise grievance by 45; the
  envoy materializes in view with an emerald demand in Java's ids and a game-time deadline, is
  paid, and the same TRIBUTE_PAID again changes nothing; a second player's killing climbs through a
  refused demand and scouts to a raid on the director's arc 112 blocks around the watching player,
  then an invasion bigger than the spawn quota in two waves (the first of 16) that loses four in ten of each wave, whose
  war chief's death routs it, the heir taking over, the same CHIEF_KILLED again changing nothing; a
  save, a gift and a crash: the pages read back to their state hash and the gift applies once; a
  clean restart gives the same state hash),
  `switched_off_conflict_ignores_its_records_and_keeps_its_page`,
  `a_conflict_command_order_goes_out_in_the_engines_dictionary` and
  `a_war_band_raids_a_district_while_away_and_lands_once_across_a_crash` (industry.rs: the mill's
  learned smog feeds the settlers' feud; with nobody there an envoy expires, scouts go, a raid band
  of goblins meets the unloaded mill abstractly, 80 items stolen and 3 blocks broken by its own
  loads, the machine lost and the smog stopped; the world saves while away; the reconcile lands at
  the load; after a crash the restarted engine derives the same reconcile from the same pages and
  the world that never saw it gets it once, the same blocks broken and the same goods taken).

## Incomplete

- **No Java**: no conflict plugin exists (P8); nothing in Java sends conflict records or encodes
  CONFLICT_COMMAND, and the bridge sends no dictionary kind 0x20 (no POLLUTION sender either).
- **The pollution goods are the industry plugins branch's** (C2-7): not on main when this branch
  merged, so in game no district emits yet.
- **Mining from deposits** (C2-8) and **village raids** (C2-10) are not covered by a native test.
- **Phase 1 trades at a haven** still do not call `Conflict::traded` (K2-30).
- **BAND_STATE** is not read (C2-11): a body that drifts from its march is not corrected.
- The conflict crate's own Incomplete list (docs/overnight-choices-conflict.md) otherwise stands:
  sieges and supply-line ambushes are untested, faction relations are hatred from data only.
