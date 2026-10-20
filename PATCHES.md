# mic-rtsim patch ledger

Every mixin, every reach into another mod's or vanilla's internals, and every other mod's API the
rtsim depends on, with the mod and versions it was checked against, the member, and why (P7, taken as
recommended: a ledger plus contract tests). Written 2026-10-20 for the core bridge `mic_rtsim` and the
phase 1 plugins `mic_rtsim_ribbits` and `mic_rtsim_carryon` (`neoforge/`).

**Contract tests.** Every mixin config is `defaultRequire 1`: a mixin whose target no longer matches
fails the launch, so booting a GameTest server is the first test of every row marked "mixin". The
"Checked by" column names the GameTest that exercises the behaviour (`./gametest.sh`; core tests run
with NeoForge alone, plugin tests with their mod's own jar).

**When a version moves** (a Minecraft, NeoForge, Ribbits or Carry On update): re-read each row's member
in the new jar with `javap -p -c` (or the mapped source), then run `./gametest.sh`. Ranges below are
what was verified, not what is known to break.

## Mixins

| # | Project | Target (mod, verified version) | Member | How | Why | Checked by |
|---|---|---|---|---|---|---|
| 1 | mic_rtsim | Minecraft 1.21.1 (NeoForge 21.1.249) | `Entity.shouldBeSaved()` | inject HEAD, cancellable | Puppets are never saved by vanilla's chunk save (PLAN.md §6.1 item 3). | `puppetIsNotSavedAcrossSaveAndReload` |
| 2 | mic_rtsim | Minecraft 1.21.1 | `Entity.saveAsPassenger(CompoundTag)` | inject HEAD, cancellable | A vehicle saves its passengers without asking `shouldBeSaved`. | `puppetRidingABoatIsNotSaved` |
| 3 | mic_rtsim | Minecraft 1.21.1 | `Llama.setStrength(int)` (private) | invoker | A captured llama comes back with its own strength (chest size). | `traderAndLlamasCaptureAsOneGroupAndNoLlamaDespawns` |
| 4 | mic_rtsim | Minecraft 1.21.1 | `ServerLevel.entityManager` (private) | accessor, GameTests only | The save tests run vanilla's real entity-chunk save. | `puppetIsNotSavedAcrossSaveAndReload`, `killingALlamaDropsItsCargoExactlyOnceAcrossACrash` |
| 5 | mic_rtsim | Minecraft 1.21.1 | `PersistentEntitySectionManager.permanentStorage` (private) | accessor, GameTests only | Read a saved entity chunk back from storage. | as row 4 |
| 6 | mic_rtsim | Minecraft 1.21.1 | `MerchantOffer.uses`, `MerchantOffer.demand` (private, no setters) | accessor (setters) | Uses follow the treasury and the cargo (C14, C21); demand is held at 0 on offers the sim prices. | `sellingDrainsTheTreasuryAndTheOfferCrossesOut`, `aCaravansOffersComeFromItsChestCargo` |
| 7 | mic_rtsim | Minecraft 1.21.1 | `MerchantResultSlot.onTake(Player, ItemStack)`, after its call to `Merchant.notifyTrade(MerchantOffer)` | inject (shift AFTER) with MixinExtras `@Local MerchantOffer`; `@Shadow merchant` | Every trade with any merchant, once, after the payment was taken: TRADE with an op id, the market or cargo moves (G6). | both trade tests, `aRibbitMerchantSellsOutOfItsPackAndDropsItWhenKilled` |
| 8 | mic_rtsim | Minecraft 1.21.1 | `StructureManager.setStartForStructure(SectionPos, Structure, StructureStart, StructureAccess)`; `@Shadow level` | inject HEAD (worldgen threads) | A village joins when its structure start generates (C22, G1). | `aVillageJoinsWhenItsStructureStartGenerates` |
| 9 | mic_rtsim | Minecraft 1.21.1 | `Villager.restock()` and `Villager.catchUpDemand()` (private): their calls to `MerchantOffer.resetUses()` | MixinExtras `@WrapOperation` | Restock through the treasury in adopted villages (C14, PLAN.md §8.5). | `sellingDrainsTheTreasuryAndTheOfferCrossesOut` (uses), indirectly |
| 10 | mic_rtsim | Minecraft 1.21.1 | `Villager.updateDemand()` (private): its calls to `MerchantOffer.updateDemand()` | `@WrapOperation` | Vanilla's demand would read the sim's uses as sales and raise the market's price (G7). | as row 9 |
| 11 | mic_rtsim_ribbits | Ribbits 4.1.6 | `RibbitApplyBuffGoal.canUse()` and `applyBuffs()` (private): their calls to `Level.getNearbyPlayers(TargetingConditions, LivingEntity, AABB)`; `@Shadow ribbit` | MixinExtras `@ModifyExpressionValue` | Sorcerers leave out players their village dislikes (C7, walkthrough F). | `aSorcererSkipsADislikedPlayer` |
| 12 | mic_rtsim_ribbits | Ribbits 4.1.6 | `RibbitPlayMusicGoal.canUse()` | inject HEAD, cancellable | A touring band does not start playing while it travels (G16). | `aTouringBandPlaysAroundItsOwnMaster` |
| 13 | mic_rtsim_ribbits | Ribbits 4.1.6 | `RibbitPlayMusicGoal`: calls to `RibbitEntity.setMasterRibbit(RibbitEntity)` in `canUse`, `start`, `tick` and the synthetic `lambda$canUse$0`, `lambda$tick$1`; `@Shadow ribbit` | `@WrapOperation` | A band member's master is always its band's leader (G16). **Fragile:** targets compiler-named lambdas; a Ribbits rebuild can rename them (the launch then fails, by design). | `aTouringBandPlaysAroundItsOwnMaster` |
| 14 | mic_rtsim_ribbits | Ribbits 4.1.6 | `RibbitEntity.restock()` and `RibbitEntity.catchUpDemand()` (private): calls to `MerchantOffer.resetUses()` | `@WrapOperation` | Ribbit merchants restock through their village's treasury (C23); a merchant puppet re-caps from its cargo. | `ribbitMerchantsTradeAndRestockThroughTheTreasury` |
| 15 | mic_rtsim_ribbits | Ribbits 4.1.6 | `RibbitEntity.updateDemand()` (private): calls to `MerchantOffer.updateDemand()` | `@WrapOperation` | As row 10, for ribbits. | `aRibbitMerchantSellsOutOfItsPackAndDropsItWhenKilled` (its restock raised the price before this) |
| 16 | mic_rtsim_ribbits | Ribbits 4.1.6 | `RibbitEntity.homePosition` (private, no setter) | accessor (setter) | A travelling puppet is homed at its waypoint, so Ribbits' GoHome goal does not walk it back. | `aTouringBandPlaysAroundItsOwnMaster` (steering), by inspection |
| 17 | mic_rtsim_ribbits | Ribbits 4.1.6 | `RibbitEntity.applyBuffGoal` (private) | accessor (getter), GameTests only | The sorcerer test drives the buff goal itself. | `aSorcererSkipsADislikedPlayer` |

## Other mods' APIs and data (no mixin)

| Project | Mod (verified version) | What | Why | Checked by |
|---|---|---|---|---|
| mic_rtsim_carryon | Carry On 2.2.6.13 | `tschipp.carryon.events.EntityPickupEvent` (fields `player`, `target`; cancellable; posted on `NeoForge.EVENT_BUS` by `CommonEvents.onEntityRightClick`'s function before `CarryOnData.setEntity` saves the entity and `Entity.remove` removes it). Listener at LOWEST, not for cancelled events. | A creature a player picks up leaves the sim (A10, PLAN.md §6.2). A listener of the same priority that cancels after ours leaves an ordinary entity in the world; it is out of the sim either way. | `aCarryOnPickupTakesTheCreatureOutOfTheSim` |
| mic_rtsim_ribbits | Ribbits 4.1.6 | `RibbitEntity`: `getRibbitData`, `setRibbitData`, `reassessGoals`, `getHomePosition`, `getOffers` (the list is edited in place), `restock`, `getMasterRibbit`, `setMasterRibbit`, `getPlayingInstrument`, `setPlayingInstrument`, `setBuffCooldown`; `RibbitData(profession, umbrella, instrument)`; `RibbitProfessionModule` (`NITWIT`, `SORCERER`, `MERCHANT`, `getProfession`), `RibbitUmbrellaTypeModule` (`UMBRELLA_1..3`, `getRandomUmbrellaType`), `RibbitInstrumentModule.NONE`; `RibbitApplyBuffGoal.canUse/start/tick/stop` (GameTests). | Making, reading and steering ribbits (C7). | the five Ribbits GameTests |
| mic_rtsim_ribbits | Ribbits 4.1.6 (data) | entity type `ribbits:ribbit`; structure `ribbits:ribbit_village`; templates `ribbits:houses/*` (14); processor list `ribbits:main`; template pool name `ribbits:ribbits` (the resident jigsaw); profession ids `ribbits:<name>`. | Adoption, residents, growth from YUNG's own templates. | `aRibbitHouseGoesOnAFreePlotAndAResidentMovesIn` |
| mic_rtsim | Minecraft 1.21.1 | `AbstractVillager.getOffers()` edited in place (wandering traders, villagers); `TraderLlama.getSlot(500 + k)` (the chest); `GossipContainer.add`, `getGossipEntries`, `getReputation`; `Villager` brain memory `MEETING_POINT`; `PoiManager.getInChunk`; `StructureTemplateManager.get`, `StructureTemplate.placeInWorld`, `filterBlocks`, `getBoundingBox`; `JigsawReplacementProcessor.INSTANCE`, `BlockIgnoreProcessor.STRUCTURE_BLOCK`; `ServerChunkCache.blockChanged`; `ServerPlayer.sendMerchantOffers`. | Phase 1 (PLAN.md §8). `overrideOffers` is not used: it is empty on the server (G3). | the core GameTests |
| mic_rtsim | NeoForge 21.1.249 | Events: `EntityJoinLevelEvent` (HIGHEST), `EntityLeaveLevelEvent`, `LivingDeathEvent` (LOWEST), `LivingDropsEvent` (LOWEST), `AnimalTameEvent` (LOWEST), `PlayerInteractEvent.EntityInteract`, `ChunkEvent.Load`, `LevelEvent.Save`, `ServerTickEvent.Post`, `ServerStartedEvent`, `ServerStoppingEvent`, `ServerStoppedEvent`, `RegisterCommandsEvent`, `ModConfigEvent.Reloading`, `LivingDamageEvent.Post` (LOWEST), `BlockEvent.EntityPlaceEvent` (LOWEST, includes multi-place), `TagsUpdatedEvent`, `PlayerContainerEvent.Close`; `IOUtilities.withIOWorker`; `ForcedChunkManager.hasForcedChunks`; the item handler capability. | Observing the world (PLAN.md §6.3). | the core GameTests |
| mic_rtsim_ribbits | NeoForge 21.1.249 | `LivingDropsEvent` (normal priority, before the core's lowest) | A ribbit's pack is added to its drops, so the drops carry its DIED (P31's recommendation). | `aRibbitMerchantSellsOutOfItsPackAndDropsItWhenKilled` |

## Data the rtsim attaches to other things

| Attachment | On | Saved | What |
|---|---|---|---|
| `mic_rtsim:agent` | puppet entities | with the entity (a copy keeps it) | engine id and epoch (PLAN.md §6.1) |
| `mic_rtsim:pending` | chunks, entities | with the holder | journal records the world side carries until the engine has them (docs/boundary.md §3.1) |
| `mic_rtsim:gossip_ops` | villagers | with the villager | the gossip op ids it was written with, the last 64 (C20, G11) |
| `mic_rtsim:player_blocks` | chunks | with the chunk | blocks players placed, per section (G12) |
| `mic_rtsim_ribbits:cargo` | ribbit merchants | with the ribbit | a ribbit caravan's pack (G15) |
