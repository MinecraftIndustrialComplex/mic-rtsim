# Mining mcrtsim for ideas

**2026-10-19.** A read-only review of all of `/home/greencheetah/Projects/mcrtsim` against DECISIONS.md, SCALE.md, PLAN.md (mid-rewrite) and both research notes.

## 1. What mcrtsim was

- A NeoForge 1.21.1 Java library, `mcrtsim_lib`, plus a demo content mod, `travelling_traders`, modelled on Veloren's rtsim. Licence "All Rights Reserved", author Benjamin McIntyre. There is no git history; file times put all of it in one night (2026-06-07 22:39 to 06-08 03:53).
- `DESIGN.md` (1,369 lines) specifies a large library: behaviour trees, eight rules, mounts, factions and components. About 940 lines of Java exist: one worker thread, a change-log queue, a chunk tracker, a per-tick NPC loop, components stored as `CompoundTag`, and a trader demo (discard the trader on chunk unload, respawn it as a vanilla wandering trader on load).
- **State:** the class files are newer than every source, so it compiled. The only run log is a server boot of an earlier version. Nothing shows the trader loop ever ran. It could not have done much anyway: no code ever sets an NPC's target, so simulated NPCs never move, and nothing is saved.

## 2. Ideas worth considering

**2.1 Stolen goods go somewhere: predator havens.** README: pirates "take harbour at pirate friendly towns" and "convert crew members from merchant ships". Today, losses on an edge are rate × hazard (SCALE.md §3, PLAN §9), which can only be a sink in C13's ledger. Instead, send the stolen share as an inbound flow to the predator's home market (a pirate haven, a bandit camp); only dragons destroy. Conservation stays meaningful, and havens sell cheap stolen goods for players to buy or raid. One rate change per hazard change: events, not volume. Crew conversion is the same transfer, applied to group counts. Touches SCALE §3, C13 and PLAN §10.1 and §10.3. No conflict.

**2.2 Notables die of age and leave heirs, so caps rarely bite.** From DESIGN.md "Reproduction and Aging": `maxAge`, `parentId1/2`, and a life stage that is derived, never stored. SCALE §6 evicts by importance, so a dragon a player fought can silently vanish. Instead, give each notable:
- a lifespan: one timing-wheel event, O(1);
- a parent link: two 6-byte handles;
- an heir or home settlement that inherits its strongest memories (an offspring hunts its parent's killer).

Eviction becomes an event in the world. mcrtsim incremented every NPC's age every tick; do not copy that. Touches SCALE §6 and PLAN §4.4, §4.8 and §9. No conflict.

**2.3 A group materializes as one unit.** From DESIGN.md's "Mount System" (paired handovers, `mountNpcId`) and its "Group System" (leader, roles, offsets). PLAN §4.1's spawn quota ("the rest queue") and §6.1's per-puppet ticking test can split a caravan: the trader spawns this tick and the llamas next, or the trader is taken back while a llama stays frozen and hits the `TraderLlama.maybeDespawn` trap. So make the puppet group (trader and llamas, rider and mount, ship and crew, band and master) the unit of quota, spawn, capture and ticking checks. Its layout is data used only at spawn; no formations or boids in the engine. Touches PLAN §4.1, §6.1, §8.3 and P15. No conflict.

**2.4 Threats get bored.** README: dragons "harass player bases and villages before getting bored and flying away". An IaF dragon is a heavy puppet. A scheduled departure, brought forward by damage or hunger, bounds its embodied time. Touches PLAN §9, §4.1 and C16. No conflict with C17 if defences still decide the outcome: hurting it shortens the visit, and doing nothing still costs stock.

**2.5 Positive acts beyond trade.** From DESIGN.md `OnHelped` (HEAL, RESCUE, GIFT). The only positive act in PLAN §8.7 is a fair trade. Vanilla zombies already attack wandering traders (`Zombie` targets `AbstractVillager`, which `WanderingTrader` extends; 1.21.1 source), so phase 1 has a free rescue: kill what attacks a caravan, and its owner gets a positive report. Data only; the values go in P18. No conflict.

**2.6 Temperament from the seed.** `SimNpc.java` has bravery, sociability and aggression, and Veloren saves a personality too (research §1). PLAN §5.5 has utility curves per kind only. Derive it from a notable's seed (0 bytes) to weight those curves, so two dragons differ with no new content. No conflict.

**2.7 Data from a removed mod survives.** From DESIGN.md's components: unknown ids stay as opaque blobs and are saved again unchanged. PLAN §1.4 keeps a body kind with no candidate abstract, but nothing covers stock of a good whose mod left the pack; dropping it breaks the conservation asserts. **This is compat work, so under the standing rule it waits until Ben declares a release.**

**2.8 A switch per module.** mcrtsim's rules were all off by default and replaceable by name: noise for a curated pack. But a per-module server-config switch is cheap: a potato server can shed a module, and Ben can bisect a perf regression. Switching off trade or travel must restore `doTraderSpawning` (C8).

## 3. Already covered

- One brain for both modes, summed dt, seed staggering, priority `choose` and interrupts (PLAN §4.3, §5.5).
- Work off the server thread with typed messages, now buffers, a journal and op ids; PLAN's bridge never waits (§4.5, §4.9).
- Inbox, sentiment and known reports (§5.6); a spatial grid (SCALE §5); route risk as edge hazards (SCALE §3).
- Supply and demand per site, far stronger in C13.
- Growth (P17), migrations (§10.2), dragons (§9), traders, ships and pirates (C5), bandits (C16).
- Inventory captured on unload (§6.1); movement medium per kind (§10.1); dimensions as frames (§5.2).
- A library plus content mods (A1); region sharding (§4.7).

## 4. Traps visible in mcrtsim

1. **Design far ahead of code, written against invented APIs.** 1,369 lines of design, about 940 of Java, and a demo that cannot move an NPC. Its site discovery calls `structureManager().getAllStructures()`, `DimensionRegistry` and `VanillaStructureTypes`. None exist in the 1.21.1 source. PLAN's Appendix A avoids that half. **The volume half is not avoided:** mic-rtsim has about 26k words of design and no code. Phase 0's measured gates are the only guard, so they must stay first.
2. **Handoff on chunk unload, with no save suppression.** `TraderEntitySync` discards tagged traders inside `ChunkEvent.Unload`. In 1.21.1, `PersistentEntitySectionManager` stores and removes entities when a chunk turns HIDDEN, on a separate path, and `ChunkEvent.Unload` fires only after the chunk save. So the query most likely finds nothing and vanilla keeps the trader (source reading, untested in game). Handover back keys on chunk load and always spawns in the overworld. Avoided: PLAN §6.1.
3. **No persistence.** A restart loses every sim NPC and leaves the vanilla-saved tagged traders as orphans. Avoided: pages and the journal (§4.9).
4. **The server thread blocks, and old results replay.** `SimulationRunner` calls `pendingResult.get()` every tick, so a slow step makes a slow tick. `MCRTSimApi.lastResult` is never cleared, so on ticks with no step, consumers read the old result again. Avoided: §4.5, sequence numbers and op ids.
5. **Cost grows with population.** Every NPC is looped every tick; DESIGN adds per-tick aging and reference scans over everyone, and an O(sites²) trade-route scan. Avoided: SCALE §0.
6. **Heavy state on the Java heap.** `HashMap<UUID, …>`, "No ECS" by design, and components that allocate a `CompoundTag` on every write and decode it on every read. Avoided: A4 and the handles and columns in SCALE §9.
7. **Small but real:** a dimension id from `ResourceLocation.hashCode()` (collision-prone); a log line and a chat broadcast to every player per handover; no git; `flake.nix` and `personalbuild.sh` copied from CobblemonRF; `diff` is a stray CobblemonRF patch. Avoided: frames, A12.

## 5. Reusable as-is

Nothing. The Java is superseded by the Rust core and the bridge design, and there is no data, asset or config (`en_us.json` is empty). The README wish list feeds §6. The code is Ben's own ("All Rights Reserved"), so he is free to relicense it.

## 6. Questions for Ben

1. **Dragon breeding.** The README says dragons "lay eggs, and breed". Should sim dragons breed, and should taking an IaF egg from a roost be a slight the parent remembers?
2. **Patrols** (README: "marines patrolling the ocean"), plus havens and crew conversion (§2.1). Should defenders be flows that lower an edge's hazard, giving players a way to secure their lines (C16)?
3. **Villages advancing technologically** (README). Growth is houses only (P17). Should villages gain professions, Create stations or tech tiers?
4. **Decline and founding** (DESIGN: abandonment to ruins, overflow villages). Should villages die and found new ones? Residents are vanilla (C2), so decline needs removal on load, like C18.
5. **Player bases as market nodes** (DESIGN: player buildings scanned into sites). Should caravans visit a player's town, for example through Create table cloth shops?
6. **Hiring and escort** (DESIGN: HIRE and JOIN_CREW inputs). Should players hire or escort caravans, with no dialogue screen (D3)?
7. **Outside modders.** mcrtsim was meant to be a public library for "a whole suite of mods". mic-rtsim is private (A12) and plugins carry no Rust (P1). Is a third-party API ever a goal? It sets how stable the module API must be.
