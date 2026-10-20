# Overnight choices: phase 1 on the Minecraft side (2026-10-20)

Reversible choices made while building phase 1 of the Java side (`neoforge/`: the core `mic_rtsim`
and the plugins `mic_rtsim_ribbits` and `mic_rtsim_carryon`), one line each with the reason. None is
in DECISIONS.md; each can be undone or put to Ben. Numbers marked PLACEHOLDER are not measured. Where a
proposal in PLAN.md §17 is still open, its recommendation was taken and is named here (D10).

## Facts found while building (checked in the jars and the 1.21.1 source)

- **`overrideOffers` is not a hook.** On the server, `AbstractVillager.overrideOffers` and
  `RibbitEntity.overrideOffers` are both empty methods (they exist for the client's copy of a
  merchant). PLAN.md §7 and §8.5 name `overrideOffers` as the way to set offers; the bridge edits the
  list `getOffers()` returns instead (G3).
- **The structure start hook** PLAN.md §6.3 left UNVERIFIED: `ChunkGenerator.tryGenerateStructure`
  hands every valid start to `StructureManager.setStartForStructure` during the chunk's
  STRUCTURE_STARTS step, on a worldgen thread. `/place structure` does not pass through it (G1).
- **Ribbits' band master** (UNVERIFIED in PLAN.md §8.8): `RibbitPlayMusicGoal.canUse` runs by day
  only, scans ribbits within 64 x 16 x 64 that are playing and takes the master of each (the last
  wins); with none, `start` makes the ribbit its own master. A touring band entering a village whose
  band is playing would split up and join it (G16).
- **Ribbits' home position** (UNVERIFIED in PLAN.md §8.8): there is no setter. `finalizeSpawn` sets
  it to where the ribbit is spawned, an amethyst shard used while sneaking re-homes it, and NBT keeps
  it. A resident spawned in its new house is therefore homed there with no patch; a travelling puppet
  is re-homed through an accessor (G16).
- **Jigsaw blocks in Ribbits' house templates** (UNVERIFIED in PLAN.md §7): every house has a
  `ribbits:house_entrance` jigsaw (final state podzol), decor jigsaws (final state structure void) and
  one `ribbits:ribbits` jigsaw whose target names the resident's profession; vanilla's
  `JigsawReplacementProcessor` replaces them all (G13).
- **Ribbits' pillar processor** writes pillars with `ChunkAccess.setBlockState`, around the level: in
  a live level the light is updated but no client is told (G13).
- **Restock and demand.** Vanilla `Villager.restock` and `catchUpDemand`, and Ribbits'
  `RibbitEntity.restock` and `catchUpDemand`, reset every offer with `MerchantOffer.resetUses`; both
  `updateDemand` methods raise each offer's demand by its uses, and demand raises the price (G7).
- **Carry On's event** is posted before Carry On saves the entity (`CarryOnData.setEntity` calls
  `Entity.save`) and removes it, from a function `CommonEvents.onEntityRightClick` passes to
  `PickupHandler.tryPickupEntity` (G18).
- **GameTest coordinates.** Relative y 1 is a test structure's floor: the structure sits one block
  above the test's structure block, which `absolutePos` counts from. The bridge's `RtsimTests.centre`
  used y 1, so caravans spawned inside the arena's floor and took suffocation damage; it is y 2 now.

## Choices

| # | Choice | Reason |
|---|---|---|
| G1 | Adoption (C22) hooks `StructureManager.setStartForStructure` at HEAD with a mixin that only queues the starts of village structures (a set of `Structure` objects computed on the server thread at start and after a datapack reload), adopted at the end of the server tick; a chunk that loads with a village start the registry does not know is adopted then, flagged LATE. | It is the one place every valid start passes, at generation time, as C22 says. The set keeps registry and tag reads off worldgen threads. LATE covers worlds made before the mod and starts that bypass generation. |
| G2 | A village's id is its dimension's Java dictionary id over its start chunk (16, 24 and 24 bits); a base's is bit 62 over the dimension and its cell. Cultures travel as dictionary names, `human` (`#minecraft:village`) and `ribbit` (`ribbits:ribbit_village`), the names rtsim-settle uses. | rtsim-settle keys a village by "the packed chunk position of its structure start" and a base by "the bridge's id"; names keep the core free of a culture enum a plugin would have to extend. |
| G3 | The bridge sets offers by editing the list `getOffers()` returns (puppets: replaced; residents: repriced in place, one for one), never through `overrideOffers`. | `overrideOffers` is empty on the server in both vanilla and Ribbits (facts above). |
| G4 | Residents are repriced to exactly rtsim-trade's `Market::offer` bundle, from a Java mirror of `PriceCurve` and `Market::offer` (`MarketBook`, unit-tested with the Rust vectors). Only market-shaped offers are touched. A bundle that does not fit one stack is clamped only in the direction worse for the player, else the offer shows crossed out; a refused quote keeps the old counts and shows crossed out. | PLAN.md §8.5 puts the pricing on the server thread from the snapshot; mirroring the engine's rule means the engine applies exactly what the player saw, and the clamps keep "no loop of trades makes money". |
| G5 | MARKET carries `asOfObsSeq`; the bridge keeps each of its own trades under its TRADE's sequence and replays those above it on any new snapshot. | Otherwise a snapshot computed before a trade, applied after it, gives the treasury back and the player can sell twice. |
| G6 | Trades are observed at `MerchantResultSlot.onTake`, right after `Merchant.notifyTrade` (a core mixin), for every merchant. | One hook for villagers, wandering traders, Ribbits and any later merchant mod; NeoForge's `TradeWithVillagerEvent` covers `AbstractVillager` only. |
| G7 | A sim-priced offer's uses are min(vanilla max uses, the treasury rule), recomputed after every trade and set again after every restock (the restock mixins); vanilla's demand is held at 0 on those offers (the `updateDemand` mixins). Reputation (`specialPriceDiff`) is untouched. | C14: uses follow stock and treasury. Vanilla's demand would read the sim's uses as sales and raise the price a second time; reputation is how C20's gossip reaches prices, so it stays. |
| G8 | Residents are repriced when a player is about to trade (`PlayerInteractEvent.EntityInteract`, before vanilla's `updateSpecialPrices`), after each trade (and the new offers are sent to the trading player), not on every snapshot. | Prices only matter on the trading screen; repricing there costs nothing per tick, and walkthrough B's "the offer pays a little less" happens between two sales. |
| G9 | HURT is coalesced per (attacker, victim) over 100 ticks (PLACEHOLDER), hits and amounts summed; a resident's death is reported at once; a villager hurt or killed by a player is flagged VANILLA_GOSSIP. | A fight is one assault, not forty records; the flag is what lets the engine honour "nothing counts twice" (PLAN.md §8.7). |
| G10 | Robbery: each member's own cargo is compared with what the bridge last left there every puppet-state interval (20 ticks), when a horse inventory closes, and before a capture; a difference is CARGO_CHANGED, held against the nearest player within 8 blocks (PLACEHOLDER), flagged as such. | An untamed caravan llama's chest cannot be opened in vanilla, so robbery comes from other mods' inventory access; a diff catches every path without patching them. Proximity is the honest attribution available. |
| G11 | WRITE_GOSSIP writes into every loaded resident villager, each villager at most once per op id (it carries the last 64 op ids, PLACEHOLDER); entries for villagers not loaded wait in the village and are written at its census, aged by vanilla's own daily decay, until they would have decayed away. A first-hand report of what vanilla records itself is refused (VANILLA_KNOWS); gossip for a culture without vanilla gossip fails. | C20 and PLAN.md §8.7. The villager's own record makes "written once" hold across a crash that rolls back the journal; the decay makes a late write equal to what the villager would have had. |
| G12 | Blocks players place are marked per chunk section (a 4096-bit set, the `mic_rtsim:player_blocks` chunk attachment) from `BlockEvent.EntityPlaceEvent`; marks are never cleared. | "Never over player-placed blocks" needs to know them; vanilla does not. Conservative: a broken player block keeps its mark, which can only make a plot less free. |
| G13 | Houses are placed with the settings `SinglePoolElement.getSettings` gives a jigsaw piece (structure blocks ignored, jigsaw replacement, then the processor list), no template entities, position-seeded randomness, a few layers per tick under 512 blocks (PLACEHOLDER), searching rings 4 blocks apart (64 candidates a tick, PLACEHOLDER) from a seeded angle; a free plot keeps 2 blocks (PLACEHOLDER) from every village building, holds no player block or block entity, only air, plants, leaves or water above its floor layer, and has ground within 4 blocks (PLACEHOLDER) under most columns. The columns under a finished house are marked changed for clients. | YUNG's houses look as in worldgen; the quota bounds the tick; the sweep makes the pillar processor's writes visible. |
| G14 | Player bases (C26) take P30's recommendation, implemented in the bridge: 128-block cells (PLACEHOLDER), a dwell counter per player and cell fed by the 20-tick position samples, halving every 3 in-game days (PLACEHOLDER), marked at 1,800 samples (PLACEHOLDER, about 30 minutes of presence), unmarked below a quarter of that, at most 16 cells per player (PLACEHOLDER). BASE MARKED and UNMARKED; a puppet's TRADE carries the base it happened in. | P30 is open (Ben's); its recommendation needs only the samples the bridge already takes, and rtsim-settle's base nodes are keyed by "the bridge's id". |
| G15 | A ribbit merchant carries its caravan's cargo in its own 27-slot pack (PLACEHOLDER capacity; the attachment `mic_rtsim_ribbits:cargo`), sells out of it, and drops it when killed, the drops carrying the DIED. | P31's recommendation (P31 is open); ribbits have no pack animals. |
| G16 | A touring band is four nitwit musicians (one per Ribbits instrument), its leader the master. It plays only when stopped (within 6 blocks, PLACEHOLDER, of its last waypoint, or never steered); every `setMasterRibbit` the music goal makes for a band member names the band's leader; followers walk back to the leader beyond 6 blocks (PLACEHOLDER); travelling ribbits are re-homed to their waypoint through an accessor. | P16's recommendation (bands tour as a flow and play at each stop; P16 is open). Without the wrap a band would join a village's band (facts above). |
| G17 | Village growth takes P17's recommendation on the Minecraft side: a house is placed when its plot's chunks are loaded (the command waits otherwise), and a resident moves in through SPAWN_RESIDENT at the house's `ribbits:ribbits` jigsaw slot; when to grow is the engine's. | P17 is open; the bridge only executes, so either answer to P17 uses these commands. |
| G18 | The Carry On GameTest posts Carry On's own `EntityPickupEvent` (from its jar, on the GameTest server) instead of running `PickupHandler.tryPickupEntity`. | After the event Carry On sends its carry data as a custom payload, which a GameTest player's connection has not negotiated; the event is the hook, and the test shows `Entity.save` works on the creature afterwards. |
| G19 | The sorcerer's thresholds come from the engine in SENTIMENT (dislike at or below, friend at or above). With no snapshot a sorcerer behaves as in Ribbits. | The thresholds are P18 tunables on the engine's sentiment scale; keeping them there leaves the plugin with no rule of its own (P1). |
| G20 | A ribbit merchant's prices for a player the village dislikes use rtsim-trade's `markup_for(sentiment, worst)`, with `worst` from MARKET; vanilla villagers get no markup. | PLAN.md §8.7: ribbits have no gossip, so the sim's own quote carries it; villagers already have vanilla's reputation pricing (C20), so a markup would count twice. |
| G21 | Plugins compile against the pack's own jars (the MIC smoke server's mods folder, `packMods`, overridable) and their GameTest servers load those jars; each plugin's mods.toml requires its mod. | The verified versions, with no download; a plugin is only installed where its mod is. |
| G22 | The census visits one loaded village every 100 ticks (PLACEHOLDER), round robin, reading entities and POIs from loaded chunks only (`PoiManager.getInChunk` per loaded chunk). | Bounded cost per tick, and nothing waits on disk. |
| G23 | Resident spawns count against the per-tick spawn quota. A spawned resident and its chunk both carry the COMMAND_RESULT until the engine has it. | Bodies are bodies (PLAN.md §4.1); the pending-record rule (B42) is what makes a spawn exactly-once across a crash. |
| G24 | Puppets are never residents: the census, gossip writes and resident repricing skip tagged entities, and a puppet merchant's restock only re-caps its uses from its cargo. | A ribbit merchant caravan in a ribbit village is the same entity type as the residents. |
| G25 | A village counts 16 blocks (PLACEHOLDER) beyond its structure box; where boxes overlap, the smallest wins. GameTest villages use ids no start can have and no margin. | Residents wander a little outside the pieces; side-by-side test arenas must not claim each other's villagers. |
| G26 | A puppet TRADE's side comes from the engine's own offer flags (sells, buys); a resident's from its market's currency. | Only the engine knows what a caravan's currency is at that stop (border markets, P21). |
| G27 | PLACE_HOUSE is deduplicated by the journal, the village's house list (saved with the registry) and the builds in progress; a resend while building gets no second answer. | The same op id must never build twice; the first command's answer serves both. |
| G28 | `gametest.sh` runs each project's suite on its own server (core, then ribbits, then carryon), with `MIC_RTSIM_LOCK` and `MIC_RTSIM_WORKERS` to override the lock and worker count. | Plugins need their mods on the server and the core must run without them (D4). |
| G29 | GameTests that need players in the level use `FakePlayers`: server players with a connection that never log in. | `makeMockServerPlayerInLevel` logs in, and Ribbits and Carry On answer a login with payloads the mock connection has not negotiated (with Carry On the server's tick loop dies). |
| G30 | The phase 1 work runs in the server's session only; a second session (the journal GameTests build their own) does not adopt, count or build. | The village registry is the world's; work done in a test's private session would be journaled into the wrong journal. |
