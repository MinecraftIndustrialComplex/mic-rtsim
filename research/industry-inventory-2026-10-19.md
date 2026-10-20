# Pack machine and logistics mods: what runs unloaded

**2026-10-19.** A lookup by a Sonnet sub-agent for the industry workstream (C15, C19).

- **Coverage:** all 168 server jars in the smoke server, scanned for capability registrations,
  SavedData subclasses, tick events and chunk tickets. Key classes were then read from bytecode or
  source.
- **Versions** are the server-jar versions. The pack lists mic_climate 0.2.0 and Distant Horizons
  3.3.1; the jars are 0.1.0 and 3.3.0.

**Legend**

| Abbreviation | Means |
|---|---|
| I | `IItemHandler` |
| F | `IFluidHandler` |
| E | `IEnergyStorage` (FE) |
| SU | Create stress units |
| BE | the state lives in block-entity NBT |
| No | the mod does nothing while its chunk is not ticking |

## Not in the pack

- **No unloaded simulation of any kind:** no Unloaded Activity, no FTB Chunks or claims, no tick
  accelerators, no lazy-chunk mods, nothing that simulates unloaded areas.
- **The nearest relatives don't do it either:**
  - Chunky only pregenerates, using region tickets.
  - C2ME, Lithium and ModernFix are performance mods.
  - ServerCore has dynamic simulation distance, but it is disabled in the smoke server config.
    UNVERIFIED for the config the pack ships.
  - Server Sleep is a gamerule datapack.

## Create core

| Mod | Ver | Factory content | Capabilities | Works unloaded? | State | Hooks |
|---|---|---|---|---|---|---|
| Create | 6.0.10 | basin, mixer, press, saw, millstone, crushing wheels, deployer, depot, belts, chutes, mechanical crafter, spout, drain, hose pulley, tanks, vault, packager, repackager, frogport, postbox, stock ticker, chain conveyor, trains, contraptions | **I:** basin, belt, tunnels, chute, vault, depot, deployer, crafter, millstone, saw, packager, repackager, frogport, postbox, station, ticker, toolbox, portable interface<br>**F:** basin, tank, drain, spout, pulley, portable interface<br>**No E.** Kinetics is not a capability, and each BE leaves its network in `KineticBlockEntity.remove()` on unload | **Yes:**<br>• `GlobalRailwayManager.tick`, from the overworld `LevelTickEvent.Post` only (trains, schedules, navigation)<br>• `GlobalStation.runMailTransfer()` moves packages between `Carriage.storage` (`TrainCargoManager`, items and fluids) and ports, using `GlobalPackagePort.offlineBuffer` when `Level.isLoaded` is false<br>• `GlobalLogisticsManager.tick` runs promise queues<br>**No:**<br>• stock request fulfilment (`LogisticallyLinkedBehaviour.getAllPresent` sees only loaded links)<br>• `RedstoneLinkNetworkHandler` (loaded only)<br>• chain conveyors<br>• contraption entities | `RailwaySavedData` (graphs, signals, trains, cargo), `LogisticsNetworkSavedData`, BE | **Events:** `BlockEntityBehaviourEvent`, `TrackGraphMergeEvent`, `PipeCollisionEvent`, `DeployerRecipeSearchEvent`. No recipe-finished event.<br>**Mixin points:** `BasinOperatingBlockEntity.applyBasinRecipe`, `PressingBehaviour.applyOnBasin` and `applyInWorld`, `FanProcessing.applyProcessing`.<br>**Poll:** `Create.RAILWAYS.trains`, `Create.LOGISTICS.logisticsNetworks` |

## Create addons (processing)

None of these works unloaded.

| Mod | Ver | Content | Capabilities | State | Hooks |
|---|---|---|---|---|---|
| Create Vintage | 1.3.9 | vibrating table, grinder, centrifuge, curving press, helve hammer, laser, lathe, coiling machine, vacuum chamber | I | BE | none |
| Create Metallurgy | 1.0.3 | foundry basin, mixer and lid, casting basin and table, crucible, belt grinder, labeling station, faucet | I and F on casting, foundry, crucible; I on grinder, labeler | BE | none |
| Create Diesel Generators | 1.3.15 | diesel engines (large, huge, modular), burner, bulk fermenter, distillation tank, pumpjack, canister, barrel, chemical turret | F; I on the fermenter | BE, plus `OilChunksSavedData` (ChunkPos to int, per level) | KubeJS `GetChunkOilAmountEventJS`; static `OilChunksSavedData.get/setChunkOilAmount` |
| Create Enchantment Industry | 2.5.4 | blaze enchanter, composer, forger, printer, infuser, gem cutter, XP hatch, grindstone drain | I, F | BE | none |
| Create Ore Excavation | 1.6.8 | drilling machine, extractor, sample drill | I, F and E on the IO block entities | `OreDataAttachment` on the LevelChunk | `OreData.extract/getResourcesRemaining`; data-driven `VeinRecipe` |
| Create Connected | 1.3.3 | brass chute, item silo, fluid vessel, inventory access port and bridge, fan catalysts | I, F | BE | none |
| Ultimate Factory, Interiors, Bells and Whistles, Copycats+, Deco, Pipe Organs, CC Better Recipes, Addon Compat | various | mostly recipes and decor; Deco has a shipping container, Pipe Organs a tracker bar | I where noted | BE | none |
| Create Dragons Plus, Encapsulated | 1.11.9 / 1.1.1 | a library with a fluid hatch and a fragile tank; a capture-ball item | F | BE | `UpdateRecipesEvent` |

## Chemistry

| Mod | Ver | Content | Capabilities | Works unloaded? | State | Hooks |
|---|---|---|---|---|---|---|
| Destroy (+ Petrolpark lib 1.5.11) | 0.4.1 | vat controller and side, bubble cap, centrifuge, cooler, dynamo, arc furnace, aging barrel, keypunch, pumpjack, sieve, extrusion die, tree tap, siphon, element tank, catalytic converter | **F:** bubble cap, converter, centrifuge, cooler, element tank, pumpjack, mixture tank, siphon, tap<br>**I and F:** vat side, barrel<br>**I:** test tube rack<br>Mixtures are `MixtureFluid` FluidStacks | **Machines: no.** `LevelPollution.tick` runs per level on `LevelTickEvent.Post`, but the emitters are block entities. | BE; attachments `level_pollution`, `chunk_pollution`, `chunk_crude_oil`; `CircuitPatternHandler` SavedData | Petrolpark `ChunkTickEvent.Pre/Post`, `PollutionEvents`; `ReactionRecipe` data |

## Power

| Mod | Ver | Content | Capabilities | Works unloaded? | State |
|---|---|---|---|---|---|
| Create Power Grid | 0.6.2 | generators, motors, transformers, batteries, switches, heaters, fans, FE inverter, wires | I on the punch card reader; E on the FE inverter only. Electricity is `IElectric` / `ElectricBlockEntity` nodes | **Partly.** `WorldNetworks` ticks per level (`GlobalElectricNetworks.preTick`), and wire `TransmissionLine`s span unloaded chunks (`tryGrabUnloadedPart`). Block-entity nodes are dropped on unload (`nodeHolderUnloaded`), so generators and loads stop. | BE; `WorldNetworks` SavedData holds only line parts |
| mic-destroy-electric | 1.0.0 | mixins that electrify Dynamo and Blacklight | none | No | BE |
| Create Nuclear | 1.3.2-b3 | enriching campfire, reactor controller, core, frame, cooler, input, output | I on the reactor input only; output is SU (`GeneratingKineticBlockEntity`) | No | BE (fuel, cooler and heat timers) |
| Create Propulsion | 1.1.5 | thrusters, burners, Stirling engine, coral generator, platinum tank, FE cable, hub, relay | F, I and E per block entity | No | BE |

## Logistics and transport

| Mod | Ver | Content | Works unloaded? | State | Hooks |
|---|---|---|---|---|---|
| Ender Transmission | 2.1.1 | item, fluid and energy transmitters (kinetic), and a chunk loader | **Buffers yes, transfer no.** `MatterTransmitterNetwork.channels` is saved in `MatterWorldSavedData` via the overworld data storage. `EnergyNetwork.channels` holds loaded block entities only. | SavedData and BE | none |
| Numismatics | 1.1.0 | vendor, salepoint, depositors, bank terminal | **The ledger yes:** `GlobalBankManager`, `BankSavedData` (UUID to `BankAccount`). Vendors need a loaded block entity. | SavedData | `GlobalBankManager.getAccount(uuid).deposit/deduct` |
| Drive-By-Wire | 0.3.0 | signal wiring | UNVERIFIED | `WireNetworkSavedData` | none |
| CC: Tweaked, CCC Bridge, CC:Sable | 1.120.2 / 1.7.3 / 1.3.4 | computers, turtles, modems, Create peripherals | **No.** Computers run only in loaded block entities, and `WirelessNetwork` sees only loaded modems. | files in the world's `computercraft` folder, plus BE | Create's `PackageEvent`, `RepackageEvent`, `SignalStateChangeEvent` |
| Sable, the Aeronautics bundle (Aeronautics, Offroad, Simulated), Tracks | 2.0.5 / 1.3.2 / 1.0.1 | physics sublevels (ships), propellers, hot-air burner, docking connector, portable engine | **Frozen** when the holding chunk unloads. `PhysicsChunkTicketManager` adds vanilla tickets around ships. | `SubLevelOccupancySavedData`, `SubLevelTicketsSavedData`, `SubLevelTrackingPointSavedData`, sublevel region files | `ForgeSablePre/PostPhysicsTickEvent`, `ForgeSableSubLevelContainerReadyEvent` |

- **Capabilities:** Ender Transmission exposes I and F backed by one shared buffer per named channel,
  plus E. Numismatics exposes I on vendors and depositors. CC: Tweaked registers I, F and E for its own
  blocks, plus a custom `PeripheralCapability`. The Sable and Aeronautics jars reference no I, F or E at
  all.

## Storage

| Mod | Ver | Content | Capabilities | Works unloaded? | State |
|---|---|---|---|---|---|
| Sophisticated Backpacks and Core | 3.26.3 / 1.5.1 | backpacks, the placed backpack block entity, upgrades | I, F and E on the placed block and on the item | Contents persist globally. Upgrades tick only in loaded block entities or while carried (UNVERIFIED). | `BackpackStorage` SavedData (by backpack UUID), `LinkedStorageGroupsSavedData`, `MountedStorageData` |

## Defence and combat machines

None of these works unloaded, except the cases noted.

| Mod | Ver | Content | Capabilities | State | Notes |
|---|---|---|---|---|---|
| Create Big Cannons | 5.11.7 | casting, boring, cannon builder, mounts, loaders, ammo containers, fluid shells | F on cast, drill, fluid shell; I on mounts (block and entity) | BE; `PartialBlockDamageSaveData` | Projectiles force chunks while in flight (Ritchie's `ChunkManager`). Hook: `ProjectileDamageEvent`. |
| CBC Neo Warfare | 0.0.6v | sights and mounted weapons | I | BE | |
| Create Radars | 0.4.9.4 | radars, fire controller, yaw and pitch controllers, data link, IFF, jammer | I | `RadarContactRegistryData`, `NetworkData`, `WeaponNetworkData`, `IDManager` | Contact TTL and group coordination run on `LevelTickEvent.Post`, but need loaded block entities (UNVERIFIED). |

## Chunk loading

| Mod | Ver | How it works |
|---|---|---|
| create_power_loader | 2.0.5 | **Mechanism:** a NeoForge `TicketController` (`create_power_loader:chunk_load_manager`) with entity-ticking forced tickets that survive restarts.<br>**Modes:** STATIC, CONTRAPTION, TRAIN, STATION.<br>**Static mode:** needs at least 30 RPM × 2^range × `speedMultiplier` (1×1, 3×3 or 5×5) and costs 16 SU per RPM. It refreshes every 10 ticks with a 20-tick grace.<br>**Trains:** mixins on `Train`, `GlobalStation`, `GlobalRailwayManager` and `TrackEdgePoint` save a per-train `CPLData` in the train NBT. |
| mic-grid-loader | 1.0.0 | **Block:** `ElectricChunkLoaderBlockEntity`, a PowerGrid `ElectricBlockEntity` that acts as a 15 ohm load.<br>**Cost:** it draws 100 W × size² (size 1 to 5, at most 8, with hysteresis).<br>**Mechanism:** the same `ChunkLoadManager`. `stationRequiresPower` defaults to false. |
| Ender Transmission loader | 2.1.1 | `LoaderBlockEntity.tick` calls `ServerLevel.setChunkForced`; the radius comes from rotational speed. |
| Others | | Sable physics tickets; Ritchie's projectile `ChunkManager`; the Ice and Fire Summoning Crystal (`setChunkForced`); Chunky and Lootr use region tickets for one-off jobs. |

## Other mods

None of these is factory-critical:
- Cobblemon 1.8.1 (no SavedData, no chunk loading) and CobblemonRF 1.0.0 (E on the healing machine);
- Overgeared 1.6.18.1 (I);
- Exposure 1.9.18 (I; `ExposureData`);
- TaCZ r6.

mic-climate, Thermoo, Serene Seasons and Project Atmosphere change rates over time but hold no machines.

## Hard cases for "measure at the boundary, replay the rates"

None of these shows up as item, fluid or FE flow through capabilities.

1. **Kinetic stress and speed.** Kinetics has no capability, and the network is transient: each block
   entity leaves it on unload. A network split at a district edge changes speed, and shafts and cogs
   crossing the boundary carry power invisibly.
2. **PowerGrid electricity.** Wire entities, node voltage and current cross the boundary, but only the FE
   inverter shows up as E. Loads inside an unloaded district vanish from `WorldNetworks`.
3. **Destroy mixtures, vat reactions and pollution.** Output depends on composition, temperature and
   time, and pollution lives in level and chunk attachments.
4. **Chunk-bound resources.** These can't be read or written without loading the chunk: Ore Excavation
   vein depletion (`OreDataAttachment`), Diesel `OilChunksSavedData` oil, and Destroy `chunk_crude_oil`.
5. **World-mutating machines.** Drills, harvesters, ploughs, saws, tree taps, deployers and fans change
   blocks and item entities.
6. **Entity-based production:** mob farms, Cobblemon pastures.
7. **Create Nuclear.** SU depends on fuel and cooler item timers and on heat, the core can explode
   (`ReactorCoreEntity`), and the only capability is the reactor input.
8. **Control state.** Redstone links, CC programs and Drive-By-Wire wiring stop with the chunk, so
   replayed rates may not match what the controller would really have decided. Stock Ticker visibility
   depends on loaded links, and radar and weapon networks need loaded block entities.
9. **Mobile assets.** Trains are fully global: the one thing that keeps moving, and observable by
   polling. Sable ships are frozen and re-ticketed, and Aeronautics fuel has no handlers at all.
   Backpack upgrades tick only when carried or placed.
10. **Environment inputs.** Temperature, season and weather scale rates but are not flows.
11. **Hazards.** Explosions (CBC, Destroy explosives), fires and pollution have no flow signature.

## Takeaways

- **The only things that keep working unloaded:**
  - Create trains, schedules and mail (overworld only);
  - Create logistics promise queues;
  - the Numismatics ledger;
  - Ender Transmission buffers (storage, not transfer);
  - PowerGrid wire lines;
  - SavedData-only stores such as backpack contents.
- **No mod provides a recipe-finished or throughput event.** Observing throughput means mixins on the
  Create and Destroy methods named above, or wrapping `BlockEntityTicker`s.
- **A doc claim to check.** `pack/modDocs/create-power-loader.md` says random ticks need a nearby player.
  Vanilla `ServerChunkCache.tickChunks` random-ticks any block-ticking chunk, so crops probably grow
  under forced tickets (UNVERIFIED).
