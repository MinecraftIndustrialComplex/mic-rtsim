# Overnight choices: the Java bridge (2026-10-20)

Reversible choices made while building `neoforge/` (the core bridge `mic_rtsim`) overnight, one line
each with the reason. None is in DECISIONS.md; each can be undone or put to Ben. Numbers marked
PLACEHOLDER are not measured.

| # | Choice | Reason |
|---|---|---|
| B1 | `neoforge/` is one Gradle build: project `core` (mod id `mic_rtsim`, package `com.minecraftindustrialcomplex.mic_rtsim`), plugin projects to be added beside it; ModDevGradle 2.0.74 declared once at the root with `apply false`. | Room for `mic_rtsim_<mod>` plugins (A1) on one MDG version. |
| B2 | `mod_license = AGPL-3.0-or-later` in the mod metadata. | A2 says AGPL-3.0; the repository's LICENSE is the FSF text, whose usual notice is "or later". Ben may want `-only`. |
| B3 | Buffers and records are little-endian by definition, not "native order". | Both engine targets are little-endian; a fixed order makes the layout a contract instead of a platform property. |
| B4 | Each buffer has a 64-byte header with its own `published` and `consumed` words; hand-over h uses observation and command buffer `h & 1`. | One acquire/release pair per buffer and direction is the whole synchronization; no lock, no extra control block. |
| B5 | At most one step in flight; Java submits step h only when step h-1 is done and step h-2's commands are fully applied. | A quota backlog then slows the sim, never the tick, and Java needs no heap copy of commands. |
| B6 | Journaled observations reach the buffer only from the journal (copied at each hand-over); latest-wins ones are dropped when a buffer is full, and counted. | The journal already holds them, so no Java-side overflow ring is needed and nothing journaled can be lost. |
| B7 | Each direction has its own id dictionary (dimension, item, body kind); Java's is per world and saved in the journal, the engine's is per session. | A replayed journal entry must mean the same in a later session; commands are never replayed. |
| B8 | A Java op id is the obsSeq of the record that carries it with the top bit set; engine op ids leave it clear. | Unique per world, identical on every replay, no extra counter. The kernel adopted the same bit (O10, main d809e7d); for a while each side flipped to match the other, and this is where both ended. |
| B9 | `Engine.start(lastAppliedCmdSeq)` returns `(pagesObsSeq, nextCmdSeq)`; Java moves `nextObsSeq` past `pagesObsSeq`. | A world save older than the engine's pages must never reuse an observation sequence the engine has seen. |
| B10 | Engine op ids applied in Minecraft are deduplicated through the COMMAND_RESULT(applied) entries still in the journal. | The journal is already the per-world record of what Java did; a separate set would be a second source of truth. |
| B11 | Every journaled record carries its own game time. | It may reach the engine ticks or sessions after it happened. |
| B12 | The `mic_rtsim:agent` attachment is serialized with the entity. | A copy made by another path keeps the tag, so the join check can recognize it (vanilla never saves the puppet itself). |
| B13 | The join check also requires the entity's UUID to be a live member of the group, not only (id, epoch). | A structure template or another mod can copy a puppet of the live epoch under a new UUID. |
| B14 | Player interest is the engine's decision (it sends DESPAWN_GROUP); the bridge itself only tests ticking. | The engine has the player positions; a GameTest server has no players. |
| B15 | "Ticking" is `ServerLevel.isPositionEntityTicking`, which in 1.21.1 is the section's TICKING visibility and the distance manager's entity-ticking range together. | Exactly the two tests `ServerLevel.tick` applies before it ticks an entity. |
| B16 | A member whose tracking ends (EntityLeaveLevelEvent without the bridge removing it) is captured inside the event, and the rest of its group at the end of the tick. | The event runs inside the entity manager's own iteration; removing other entities there is not safe. |
| B17 | A member that changes dimension makes the whole group be captured (reason UNLOADED). | A group is one unit (C25) and can no longer stand together. |
| B18 | Deaths come from LivingDeathEvent (lowest priority, not if cancelled) with the member's state at death; a DISCARDED removal the bridge did not make becomes TAKEN(removed by other). | Deaths are always kept; vanilla despawn or another mod's removal must still reach the engine. |
| B19 | A llama whose leash snapped is not re-leashed by the bridge; its MEMBER_STATE flags say so. | Re-leashing would create leads out of nothing (a snapped leash already dropped one). |
| B20 | Every pass sets the trader's despawnDelay to 48000 (WanderingTraderSpawner's value) and each llama's to 47999 (TraderLlama's initial value). | Closes the maybeDespawn trap for leashed and unleashed llamas alike; vanilla's own numbers, not new ones. |
| B21 | A human caravan's llamas stand at fixed offsets of plus and minus 2 blocks in x from the trader. PLACEHOLDER. | Reproducible spawns; vanilla scatters them within 4 blocks. The layout format is P15. |
| B22 | A trader llama's variant and strength are captured as body bits and restored on spawn (strength through an `@Invoker` on the private `Llama.setStrength`). | A llama comes back as the same llama with the same chest size (the Veloren re-roll lesson). |
| B23 | Cargo is captured as (slot, item, count); item components are not captured yet. | Enough for phase 1 goods; components need an encoding decision. |
| B24 | TRANSFER_TO_CONTAINER moves one stack per command, all or nothing, through the block's NeoForge item handler capability. | An exact simulated insert, any modded container, no partial transfers. |
| B25 | Every module switch defaults to on; the module dependency table exists but is empty. | Nothing decides which module needs which yet (PLAN.md §5.4 only says the check must name the missing one). |
| B26 | The sim owns traders only while trade and travel are both on; switching either off restores the gamerule's value from before the sim took it (kept in `data/mic_rtsim_bridge.dat`), and a sync while owned sets it false again if someone turned it on. | C25's wording ("turning off trade or travel restores vanilla trader spawning"); restoring the old value respects a world that had it off. |
| B27 | Config budgets: spawn quota 8 bodies per tick, full puppet pass every tick, PUPPET_STATE every 20 ticks, PLAYER every 20 ticks (PLAN.md §6.3). All PLACEHOLDER except the last. | Something to run with until phase 0 measures them (P24). |
| B28 | GameTests move config switches only through `RtsimConfig.Test`, unlocked by `-Dmic_rtsim.gametest=true`. | The mic-climate convention; a normal game cannot change a switch behind the file. |
| B29 | Until JNI exists the engine is `StubEngine`; it keeps pages in `<world>/rtsim/stub-pages.bin`. `/rtsim perf` and `/rtsim stub caravan` (op level 2) are there for looking at it in a real game. | Scaffolding, to be deleted with the stub. |
| B30 | An exception in the bridge's tick captures every puppet, journals it, and stops the bridge for the session instead of crashing the server. | Fail safe: nothing duplicated, nothing lost, the world keeps running. |
| B31 | Every puppet is captured at ServerStoppingEvent (before the world save), with no final hand-over. | The captures reach the journal and are replayed next session. |
| B32 | Accessor mixins on `ServerLevel.entityManager` and `PersistentEntitySectionManager.permanentStorage` live in the main mixin config, used only by the GameTests. | The save tests run vanilla's real entity-chunk save and read the chunk back from storage. |
| B33 | The boundary, the journal and the stub have plain JUnit 5 tests (`./gradlew :core:test`). | They are pure Java, so they test in seconds without a server. |
| B34 | The join check runs at EventPriority.HIGHEST; with no session (server still starting), every tagged entity is stale. | Stop copies before other mods see them; puppets never outlive a session. |
| B35 | A stale copy that is refused also ejects its passengers and leaves its vehicle. | A copy loaded with a stack must not leave entities pointing at it. |
| B36 | The pass checks taking before ticking; a taken member leaves its group, and a group with no members left sends DEMATERIALIZED(no members left). | A taken creature must not be captured back into the sim. |
| B37 | The GameTest world is reused between runs, as in mic-climate; the journal and the stub's pages carry over, so each run starts with a real replay. | Free coverage of the restart path. |
| B38 | SESSION_START also carries the journal's first sequence and the world save's own next sequence. | They are the inputs of the kernel's `Replay::reconcile`, which detects an engine ahead of the world (P29). |
| B39 | WORLD_SAVED (a new latest-wins record) reports when a world save is on disk, through a marker chained after the journal's write on NeoForge's SavedData IO chain. | The kernel's `Replay::forget_ops_through` needs "a world save past this sequence is durable", and the chain makes that exact. |
| B40 | A taken creature keeps the persistenceRequired flag the bridge set while it was embodied. | A creature a player took should not despawn; vanilla already keeps a leashed or tamed trader llama. |
