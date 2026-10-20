# Veloren rtsim deep dive

**2026-10-19.** Source-level research by an Opus sub-agent, after Ben asked whether Veloren's rtsim had
been researched properly; the first pass had only skimmed it.
- **Source read:** Veloren master `cb596f79` (2026-10-18), about 13k lines across `rtsim/`,
  `server/src/rtsim/`, `common/src/rtsim.rs` and `world/src/civ/`. File paths below are relative to
  that repo.
- **Web pass:** a Sonnet sub-agent read the devblogs and merge requests. The GitLab comments API
  refused it, so it read merge-request descriptions only, never the discussion threads.
- **Not covered here:** the economy (`world/src/site/economy`), which has its own report,
  `economy-2026-10-19.md`.
- **Marking:** anything marked UNVERIFIED came only from the web pass, or is inference made without
  running the game.

**Verdict.** Veloren's rtsim is a tidy in-memory database of NPCs, sites and reports, driven by a good
combinator AI. Little of it survives a restart or a chunk unload with full fidelity:
- airships run on a fixed timetable that resets on every boot;
- merchants carry nothing between towns;
- factions are labels.

Copy its brain library and its save discipline. Design handoff, airships and trade ourselves.

## 1. Architecture

**Tables** (`rtsim/src/data/mod.rs:39-70`)
- **Saved:** `nature`, `actors`, `sites`, `factions`, `reports`, `architect`, `quests`, `tick`,
  `time_of_day`, and a `should_purge` flag.
- **Not saved:** `airship_sim`.
- **IDs:** slotmap keys `ActorId`, `SiteId`, `FactionId` and `ReportId` (`common/src/rtsim.rs:30-35`).
  `QuestId` is a u64 from an atomic counter, so parallel brains can allocate them
  (`data/quest.rs:57`).
- **Players are actors too.** Since !5394 (2026-07), `Actor` holds either an `Npc` or a `Character`
  (`data/actor.rs:308-342`).

**What is persisted and what is derived**
- **Actor, saved:** seed, world position, direction, body, role, home, faction, and health fraction
  ("presence").
- **Npc, saved:** known reports, personality, sentiments, and job (hired or quest).
- **Never saved:**
  - controller, inbox and brain (`actor.rs:290-296`);
  - chunk position, current site and mode (`actor.rs:330-341`).
- **Names** come from the seed and are never stored (`actor.rs:531-539`).
- **Site, saved:** seed, world position, faction and reports. Population, the link to the worldgen site,
  and neighbours are rebuilt at boot (`data/site.rs:24-53`).

**Save format**
- MessagePack with field names (`data/mod.rs:111-113`), written to `data/rtsim/data.dat`.
- `CURRENT_VERSION = 11` exists only for deliberate hard purges. Ordinary schema changes rely on
  `#[serde(default)]` (`data/mod.rs:32-37`).
- Enums use one-character serde renames, and resource maps skip default values
  (`common/src/rtsim.rs:385-411`, `data/mod.rs:122-166`).
- A save that fails to load is renamed to a backup and the rtsim world regenerates
  (`server/src/rtsim/mod.rs:76-103`).
- Every 60 s the whole `Data` is cloned on the main thread and handed to a save thread, which does an
  atomic write (`server/src/rtsim/tick.rs:540-547`, `mod.rs:310-372`).
- At boot, rtsim sites are re-linked to worldgen sites by origin position. Orphans are deleted and NPCs
  re-homed (`rule/migrate.rs:28-119`).

**Rules and events**
- `RtState` holds type-keyed maps of resources, rules and handlers; `Rule::start` binds handlers.
- **Default rule order** (`lib.rs:199-209`): Migrate, Architect, ReplenishResources, ReportEvents,
  SyncNpcs, SimulateNpcs, NpcAi, CleanUp. The server adds DepleteResources.
- **Events** (`event.rs`): OnSetup, OnTick, OnDeath, OnHelped, OnHealthChange, OnTheft, OnMountVolume,
  plus the server-side OnBlockChange.
- Handlers run synchronously when an event is emitted; a TODO says to queue them instead
  (`lib.rs:305`).

**The NPC brain** (`ai/mod.rs`)
- **Actions.** An `Action` is ticked and returns Continue or Break, much like a future.
- **Combinators** (`ai/mod.rs:150-424`): `then`, `and_then`, `repeat`, `stop_if`, `interrupt_with`,
  `when_cancelled`, `map`, `with_state`, `map_state`, `debug`, `boxed`, and the constructors `now`,
  `until`, `just`, `finish`, `seq` and `choose`.
- **Priority** (`ai/mod.rs:656-738`):
  - `choose` re-runs its decision every tick, at three priorities: urgent (100), important (50) and
    casual (0).
  - Only a strictly higher priority replaces the running action, which then gets `on_cancel`.
- **Interrupts** (`ai/mod.rs:940-955`): `interrupt_with` polls every tick, runs the interrupting action
  to completion, then resumes.
- **The top-level humanoid tree** is interrupted by inbox handling, enemy checks and quest timeouts
  (`rule/npc_ai/mod.rs:1493-1569`).
- **The brain is never saved,** so every NPC restarts from scratch on each boot.

**Cadence and parallelism**
- rtsim runs every server tick (30 Hz) in the Create phase, after physics
  (`server/src/rtsim/mod.rs:381-383`).
- Unloaded NPCs think every 10th tick, staggered by seed; loaded NPCs think every tick
  (`npc_ai/mod.rs:93-97, 129`). The dt of the skipped ticks is summed and passed in
  (`npc_ai/mod.rs:147-181`).
- Per-second probabilities are converted for large dt (`ai/mod.rs:89-110`).
- **Only the brains run in parallel** (rayon).
  - Each brain's controller, inbox, sentiments and reports are moved out first, and the shared data is
    read-only (`npc_ai/mod.rs:122-207`).
  - Inventory access goes through a `Mutex` around the ECS storage (`ai/mod.rs:128`).
  - Everything else is single-threaded.

## 2. Loaded and unloaded

**Modes.** `SimulationMode` is either `Simulated` or `Loaded` (`actor.rs:42-49`). It is never saved.

**Unloaded to loaded**
- Every tick the server scans all vehicles, then all actors (`tick.rs:597-670`).
- Any actor in a loaded chunk becomes `Loaded`, and the server emits an event to create its NPC or ship.
  The entity is built from a profession asset with a deterministic RNG, and its health fraction is
  restored (`tick.rs:552-592`).
- Riders spawn together with their vehicle.

**Driving the real entity**
- Every tick, rtsim's activity, look direction, personality and queued actions are copied into the
  server AI's `rtsim_controller` (`tick.rs:698-714`).
- The entity's outbox (dialogue, interactions) is copied back into the rtsim inbox.
- The server AI does the actual pathing, flight and PID control
  (`server/agent/src/action_nodes.rs:273-470`).
- Position and body are copied back to rtsim every tick (`tick.rs:684-685`).

**Loaded to unloaded**
- The server deletes every non-player entity whose chunk unloaded, and calls
  `hook_rtsim_entity_unload`, which flips the mode back (`server/src/lib.rs` around 985-1037,
  `rtsim/mod.rs:246-255`).

**What is lost**
- All state that lives only in the ECS: inventory, AI targets and memory, PID state, buffs.
- **Merchant wares are re-rolled on every load** from the home site's economy stock, with a fresh RNG
  (`tick.rs:40-141`). UNVERIFIED inference: coins a merchant pays out come back at the next reload.
- **Off-screen action stops:** actions queued while unloaded are dropped, and attacks are not
  simulated at all (`simulate_npcs.rs:132-135`).
- The old "remember fights" memory was never ported to rtsim2
  (`server/src/sys/agent/behavior_tree/mod.rs:983-1033`).

**Duplication safeguards**
- An ECS entity whose actor is marked `Simulated` is deleted (`tick.rs:716-721`).
- The death hook runs only if the entity is really deleted, "otherwise... 2 entities having the same
  ActorId" (`events/entity_manipulation.rs:1362-1366`).
- A second unload is logged as an error.
- Startup migration deletes surplus airship captains.
- **History:** devblogs 217 and 218 describe captains and pilots duplicating en masse. Version 0.16
  fixed captains being generated en masse after `/reload_chunks`.

## 3. Travel

**Between sites**
- A* over the worldgen site graph, capped at 250 iterations, using track costs
  (`npc_ai/movement.rs:75-130`).
- The NPC then walks the track nodes chunk by chunk, each snapped to the nearest road
  (`movement.rs:426-479`).
- **Inside a site:** A* over the site's tiles, with a cost per tile type and rules for doors
  (`movement.rs:3-73`).
- **Fallback:** a straight line with waypoints every 48 blocks (`movement.rs:413-423`).
- **No caching:** `PathingMemory` is defined but never used (`actor.rs:51-62`).

**Positions while unloaded**
- `simulate_npcs` moves every present, living NPC every tick, straight toward its controller's target.
  Speed is the body's maximum speed × the speed factor × dt.
- Height is snapped to the approximate surface (plus flying height for fliers), and positions are
  clamped to the map. There is no terrain collision; boats are confined to river chunks
  (`simulate_npcs.rs:147-325`).
- This is not throttled, although the crate docs say it is.
- UNVERIFIED: !4421 found that NPCs beelining across unloaded chunks stalled the server.

## 4. Airships

**Two separate systems**
- **Balloons** pick a random dock and fly straight to it (`npc_ai/mod.rs:1175-1214`).
- **The default airship** has its own pilot, a 1,361-line timetable (`npc_ai/airship_ai.rs`).

**Routes** (`world/src/civ/airship_travel.rs`)
- **How they are built:**
  - Docks are airship-dock plots, Delaunay-triangulated.
  - The triangulation is made Eulerian, then split into closed loops of roughly equal length
    (`:2143-2240`).
  - The iteration budget falls as the number of docks grows (`:1143-1184`), and legs are capped at
    1,000 chunks (`:1139-1141`).
- **Six phases per leg:** DepartureCruise, ApproachCruise, Transition, Descent, Docked, Ascent
  (`:145-212`).
- **Timings:** ascent and descent take 30 s each, a ship stays docked 60 s, and ships on the same route
  depart 240 s apart.
- **Heights:** cruise heights are 400, 475, 550 and 625 by route index. A dock has 4 platforms, so at
  most 4 routes pass through it (`:503-536`).
- **Speed:** nominally 23 blocks/s (`common/src/comp/body/ship.rs:216`).
- **Spawn points** come from walking each route's clock (`:866-1000`).
- UNVERIFIED: !5265 fixed route generation looping forever on small worlds.

**Pilots**
- Each captain is an NPC mounted as the steerer of a vehicle actor (`rtsim/src/generate/mod.rs:174-207`).
  The vehicle itself has no brain (`npc_ai/mod.rs:127`).
- For each phase the pilot computes a duration from the schedule, corrects for lateness, and sets its
  speed to distance over time (`airship_ai.rs:310-414`).
- It steers toward the point where the timetable says it should be right now (`airship_ai.rs:889-912`).
- While docked it announces the next destination every 10 to 16 s (`airship_ai.rs:658-770`).
- UNVERIFIED quote: !5146 replaced the earlier autonomous collision avoidance, which had "proved
  fragile and problematic".

**Loaded and unloaded**
- **Unloaded,** a ship moves only in x and y, with its height set to terrain plus cruise height
  (`simulate_npcs.rs:185-251, 309-318`).
  - This means it can never pop in underground. It came from !5091, after ships that were flying low
    when unloaded got stuck on load.
  - Simulated ships therefore pass through mountains.
- **Loaded,** the server AI flies the ship with PID controllers.
  - The ship counts as stuck when its last 5 position samples lie within about 3 blocks. It then backs
    out 100 blocks and climbs 200 (`airship_ai.rs:100-226`).
  - Loaded ships descend 1.1 to 1.25 times faster (`airship_ai.rs:575-580`).
- A player who boards gets "welcome aboard" (`simulate_npcs.rs:32-49`).

**Restarts**
- Routes and pilot assignments are never saved (`data/airship.rs:13-24`).
- On every boot, the startup migration:
  - assigns the existing captains to spawn points in arbitrary order;
  - deletes extra captains and spawns any missing ones;
  - teleports each ship and captain to its spawn point (`migrate.rs:121-187`,
    `data/airship.rs:52-110`).
- **So airship positions and the timetable reset on every server restart.**

**Gaps**
- **Traffic control is unused.** `next_pilot` is never called (`data/airship.rs:201-229`); ships stay
  apart only through schedule spacing and route heights.
- **Airship logging is likely broken:** the `airship_log` feature imports `NpcId`, which no longer
  exists (`airship_logger.rs:1`).
- **A stopgap:** the pilot's current leg is stored in the controller, which the code admits is
  temporary (`actor.rs:73-82`).
- **No respawns:** vehicle respawn is `unimplemented!()` (`rule/architect.rs:578-581`). Captains are not
  in the wanted population (`generate/mod.rs:26-82`), so a killed captain seems not to respawn until a
  restart (inferred, UNVERIFIED in game).
- **Slow spawns:** each ship spawn scans every voxel of the ship to find its seats
  (`events/entity_creation.rs:304-322`).
- **Open issues** (UNVERIFIED):
  - #1974: terrain collisions that also drain CPU;
  - #2062: NPCs ignore airships;
  - #2079: fall-damage deaths on load.
- **Planned:** !5214 says a larger refactor is coming.

## 5. Merchants and travellers

**Who travels**
- **Merchants and adventurers** run `adventure()` (`npc_ai/mod.rs:584-613`):
  1. keep only sites with a workshop plot;
  2. drop 75% of them at random;
  3. pick the nearest one left, announce it, and travel there;
  4. stay 15 min (merchants) or 3 min (adventurers).
- **Pirates** gather hired crew and raid sites within 10 km (`npc_ai/mod.rs:372-582`).
- **Villagers** migrate when their home has more than 1.5 residents per house
  (`npc_ai/mod.rs:746-786`).

**What they carry: nothing in rtsim.**
- **Wares exist only on the loaded entity.** They are built from the home economy's unconsumed stock
  × 0.1, with hard-coded fixes such as "econsim doesn't produce recipes at all" (`tick.rs:54-105`).
- No goods move between sites, and there are no caravans.
- Trading uses the server's trade window. The dialogue code says "TODO: Include trading here!"
  (`dialogue.rs:306`).
- The only item kind rtsim knows about is `Coin` (`common/src/rtsim.rs:424-439`).

**Paid work**
- **Escort quests:** merchants offer escorts to sites 1 to 5 km away. The reward is distance/5 coins,
  taken from the loaded entity's inventory (`npc_ai/quest.rs:171-253, 16-63`).
- **Hiring:** adventurers can be hired by the day or the week, at prices that double with rank
  (`dialogue.rs:457-512`).

**Spawn counts per town** (`generate/mod.rs:31-46`): guards = plots/4, adventurers = plots/5,
merchants = plots/6 + 1.

## 6. Factions, sentiment, reports, quests

**Factions** (`generate/mod.rs:108-119`, `generate/site.rs:22-75`)
- 16 random factions, each with a random "good_or_evil" bit.
- Each site takes the nearest faction of its own alignment.
- Leaders are never set, and faction sentiments are never written, only cleaned up.
- The code calls the alignment bit "Very stupid, get rid of this" (`data/faction.rs:13`).

**Sentiment** (`data/sentiment.rs`)
- An i8 per target (an actor or a faction). Capped at 128 entries per NPC and 1,024 per faction.
- **Decay is random, and slower for strong feelings** (`:53-75`):

  | Sentiment | Decays in about |
  |---|---|
  | ±0.1 | 26 min |
  | ±0.6 | 21 h |
  | ±0.8 | 47 h |

- Decay runs every 30 ticks per NPC (`rule/cleanup.rs:25-31`).
- **Bug:** the cap cleanup drains a max-heap of absolute values, so it deletes the *strongest*
  sentiments first (`:78-96`).
- `limit_below` with a negative cap would wipe every negative sentiment. It is only ever called with
  +0.1.

**How player actions change sentiment**

| Action | Change |
|---|---|
| Each hit | −0.1, down to no lower than ENEMY (−0.6) |
| Healing | +0.05 |
| Saving the NPC | +0.3 |
| Witnessed murder | −0.75 toward the killer, or +0.25 if the victim was an enemy |
| Theft at the NPC's home site | −0.2 |

- Sources: `simulate_npcs.rs:51-85`, `npc_ai/mod.rs:1247-1477`.
- UNVERIFIED (!4760): damage and help create no report, so they never spread.

**What NPCs do with sentiment**
- Attack any actor within 24 blocks who is at ENEMY or worse (`npc_ai/mod.rs:1479-1491`).
- Refuse to talk to anyone at ENEMY (`npc_ai/mod.rs:219-235`).
- Hired hands quit at RIVAL (`npc_ai/mod.rs:636-648`).

**Reports**
- **Kinds:** only Death and Theft. They are pushed into the inboxes of NPCs within 32 blocks (death) or
  24 blocks (theft) (`rule/report.rs`).
- **Lifetimes** (`data/report.rs:32-47`):

  | Report | Kept for |
  |---|---|
  | Murder | 15 in-game days |
  | Other deaths | 5 in-game days |
  | Theft | 1.5 in-game days |

- **Gossip:** an NPC standing at its home site uploads all its reports to the site and downloads the
  ones it lacks, every tick (`sync_npcs.rs:108-124`, "TODO: Only share new reports").
- **Comments:** NPCs only remark aloud on reports less than 5 min old (`npc_ai/mod.rs:1256`).
- **No caps:** there is no global cap on reports and no cap per NPC.

**Quests** (`data/quest.rs`)
- A passive table ("Jira board") of Escort, Slay and Courier quests.
- Each quest has an arbiter, a coin deposit, a timeout, and a resolution that can be set only once.
- Quests are never garbage-collected (`data/quest.rs:22-24`).
- Timeouts and hire expiry use the server's `Time` clock, which appears to reset at each boot
  (UNVERIFIED).

## 7. World generation

**Civs and sites**
- 48 civs at 1,024² chunks (`world/src/civ/mod.rs:34-41`).
- Then three times that many sites, picked by weighted random kind with proximity rules.

**Roads** (`civ/mod.rs:1201-1348, 1446-1487`)
- Each town links to its 4 nearest towns within 400 chunks, using A* on the chunk grid (up to 100k
  iterations).
- A new road is built only if it costs less than a third of the existing route.
- Path bits are written into chunks, and Bridge sites are created along the way.

**Economy at worldgen**
- Road connections become trade-neighbour links in the economy.
- The economy is simulated once, at world generation (`civ/mod.rs:650-671`).

**No change over time**
- There is no history simulation: `World::tick` is an empty TODO (`world/src/lib.rs:169-171`).
- Sites never grow.
- The rtsim site docs say rtsim should "eventually" replace worldgen's site system
  (`data/site.rs:28-40`).

**Seeding rtsim** (`rule/architect.rs:55-252, 345`)
- One rtsim site per worldgen site, airships at every spawn point, and a wanted population.
- The Architect then spawns NPCs from fake "deaths", and only into sites with no loaded chunks.
- Since 0.18, a respawn waits at least one in-game day. Instant respawns had been farmed ("loot fever",
  UNVERIFIED).

**Nature** (`rule/replenish_resources.rs`, `server/src/rtsim/rule/deplete_resources.rs`)
- A whole-map grid of resource depletion per chunk.
- Block changes deplete it.
- Replenishment visits 8,192 chunks per tick and refills the map fully in 1 h.

## 8. Performance

**Measured numbers**
- I found none upstream.
- One fork reports these (second-hand, UNVERIFIED):

  | Measure | Figure |
  |---|---|
  | Cost per extra 1,000 simulated NPCs | about 0.2 ms |
  | 3,490 NPCs | 0.85 to 0.97 ms |
  | 12,000 NPCs | 2.3 ms |
  | Disk per NPC | about 380 bytes (4.3 to 8.9 MB in total) |
  | Save write | 18 to 23 ms |
  | Load | 23 to 30 ms |

**Full scans over every actor, every tick, at 30 Hz**

| Scan | Where |
|---|---|
| `sync_npcs`: chunk lookup, site lookup, grid update and report gossip | `sync_npcs.rs:93-158` |
| `cleanup`: filters every actor and keeps the live ones | `cleanup.rs:26-60` |
| The NPC AI's selection pass | `npc_ai/mod.rs:122-145` |
| `simulate_npcs` | `simulate_npcs.rs:122-351` |
| The server's load check over all vehicles and all actors | `tick.rs:597-670` |

**Other hot spots**
- **Linear scans of all actors:**
  - small talk, to find the nearest monster (`npc_ai/mod.rs:288-295`);
  - the slay-quest offer (`quest.rs:257-266`);
  - character login (`server/src/state_ext.rs:850-857`).
- **Uncached pathing:** site-graph pathing calls `Civs::neighbors`, which scans the whole track map for
  reverse edges, and never caches the result (`civ/mod.rs:748-762`).
- **Quadratic lookup:** the `nearby` lookup is marked "(theoretically) O(n^2)" (`actor.rs:811-813`).
- **Synchronous save clone:** every 60 s the whole `Data` is cloned synchronously ("TODO: Use slow
  jobs"). It includes about 1M chunks of nature data, roughly 46 MB by my estimate (UNVERIFIED).
- UNVERIFIED: !4421 notes that the default A* limit of 25K is "way too big".

## 9. Unfinished, abandoned or broken

- **Promised in the crate docs but absent** (`lib.rs:13-19`): economics, industry, trade, transport,
  wildlife populations, disasters, climate.
- **Dead or stubbed:**
  - `PathingMemory` and `next_pilot`;
  - faction sentiments and leaders;
  - quest garbage collection;
  - the broken airship log;
  - `unimplemented!()` for vehicles and for any body not listed (`tick.rs:434-436`).
- **Hard-coded:** sentiment deltas, and role hacks ("Roles themselves are kind of a hack",
  `npc_ai/mod.rs:1313`).
- **65 TODO or FIXME markers** in total.
- **Open issues** (UNVERIFIED):
  - #1839: rtsim messages arrive in bursts;
  - #1961: debt in the spawn pipeline;
  - #1970: quest generation.
- **What the developers say they will do:**
  - make players actors, "step 1 of many" (!5394);
  - refactor airships (!5214);
  - index reports by chunk instead of pushing them into inboxes (`rule/report.rs:35-37`);
  - queue events (`lib.rs:305`);
  - regrow resources lazily in closed form (`replenish_resources.rs:11-14`).
- **Abandoned:** autonomous airship pilots, replaced by timetables.

## 10. Lessons for mic-rtsim

**Copy:**
1. **Save discipline:**
   - flat ID-keyed tables;
   - a binary save with field names and defaults;
   - a version number kept only for hard purges;
   - atomic writes on a save thread;
   - re-linking to worldgen at boot.
2. **The action combinators:** priority `choose`, interrupts and cancel hooks. AI is written once and
   works in both modes.
3. **One abstract controller** (an activity plus discrete actions), consumed either by our own
   simulation or by whoever drives the real entity.
4. **Unloaded brains at a reduced rate,** staggered by seed, with summed dt and per-second chances
   converted.
5. **Parallel brains** that read a frozen snapshot, write to their own buffers, and merge afterwards.
6. **Timetabled sky ships.** Position is a function of the clock, so unloaded and loaded ships agree
   without physics. Force cruise altitude while unloaded.
7. **Bounded, decaying sentiment,** and reports with lifetimes, spread by gossip through the site
   noticeboard.
8. **An Architect** that refills populations after a delay, and never into loaded areas.

**Avoid:**
1. **Losing state on unload.** Persist inventory, cargo, health and job, and rebuild the entity from
   them.
2. **Unsaved brains and airship schedules.** Save each ship's clock offset and route assignment, so a
   restart doesn't teleport ships.
3. **Full scans every tick.** Keep spatial and dirty indexes, and gossip only what changed.
4. **Cloning the whole state synchronously to save.** Snapshot incrementally or copy on write.
5. **No caps on reports and quests,** and the inverted sentiment cleanup.
6. **Recomputing paths.** Cache routes on the site graph.
7. **Panicking on bodies or roles it doesn't handle.**

**What Veloren lacks that we need:**
1. **Visibility-based handoff.** Minecraft chunk loading is not visibility-based: spawn chunks, chunk
   loaders and forced chunks keep regions loaded.
   - Handoff must key on whether an entity is actually ticking, not on whether its chunk is loaded.
   - It needs a generation counter or a binding to the entity's UUID, to beat duplicates from
     save/reload races.
   - Entities are saved inside chunk NBT, so the copy vanilla saves must be suppressed or reconciled.
2. **Other mods own the entities.**
   - Our actor needs an adapter layer that spawns through the owning mod's API.
   - It should observe the entity's state rather than assume control of it.
   - It must tolerate the entity being killed, converted or moved by others.
   - When the far end of a link fails, fail gracefully, as the rtsim docs advise.
3. **Ships as multi-block structures** (Create contraptions, Sable ships).
   - Unloading must freeze the whole structure with its passengers, and loading must restore it.
   - Docks must be reserved terrain.
   - Passengers who log out mid-flight need a defined landing.
4. **Real cargo and trade:** goods tables per site and per traveller, so caravans and ships move
   inventory. Veloren never got this far.
5. **Site growth and history,** if villages are to change. Veloren has neither.
