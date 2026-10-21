# Glass box: compiled Create factories

**Status:** design for Ben's review, 2026-10-20. Nothing is built. Ben approves this design before code.

**Decided, and binding here:**
- **C28:** distant factories are a compiled glass box. The real Create graph is compiled into a rate
  network (kinetic networks with speeds and stress, each machine's cycle time from Create's own formulas
  at its RPM, belt, chute, funnel, arm and pipe capacities, filters, heat and trains), solved
  analytically and run piecewise between breakpoints. Addon machines get plugin models, unknown machines
  fall back to measured black-box nodes, and boundary measurement checks the model.
- **C29:** the black-box code already built stays: reconcile, threats, ports, sweeps, ledger, journal.
  The black-box learner becomes the fallback for unknown machines and the checksum on the model.
- **I26:** order: (1) compile and solve, checked against the black box; (2) trains and stations;
  (3) addon machine models; (4) threats and rebuilding later.
- **Also in force:** I6 (coupling is learned, from starvation events), I7 (the unloaded part of a partly
  loaded district keeps being simulated), I9 (replay everything, mob farms included), I13 (PowerGrid
  lines couple districts; Ender Transmission is local only), I18 (no free visibility), C30 (player Sable
  ships doing logistics), D1 (performance on a weak PC is the only judge), SCALE.md §0 (cost never
  grows with item count or elapsed time), and every decision INDUSTRY.md relies on that C28 does not
  replace (I2 to I5, I8, I10 to I12, I14 to I17, I21 to I25).

**Sources.** Create formulas are read from `mc1.21.1/dev` at `7ff2760`, and every constant used here
was also found by `javap` in the pack's `create-1.21.1-6.0.10.jar`. Addon facts are `javap` on the
pack's jars, or marked UNVERIFIED. Config values are Create's defaults: the pack ships no
`create-server.toml` override (`pack/defaultconfigs`), and the compiler reads the live config anyway.

**Numbers:** every number is a verified fact, arithmetic from one, a labelled TUNABLE, or a labelled
ESTIMATE. One tick is 1/20 s. RPM is the magnitude of `KineticBlockEntity.getSpeed()`: Create's
formulas take its absolute value, and the sign only sets direction.

---

## 0. What changes and what stays

| Part | INDUSTRY.md (C19, built) | Glass box (C28) |
|---|---|---|
| What runs an unloaded district | one learned box: all-or-nothing rates per district | a compiled graph of stores, machines, movers and junctions, solved for steady rates per regime |
| Where rates come from | least-squares slopes of boundary stock | Create's own formulas at the compiled RPM, checked against those slopes |
| Unknown machines | the whole district is the box | one black-box node inside the graph; the box shrinks to what the model cannot see |
| Coupling | one process (I6 rework pending) | structural for modelled nodes; learned from starvation events for black-box nodes (I6) |
| Stock | pools per (district, good) | pools per (store, good): reconciles know exactly which vault gained what |
| Breakpoints | input empty, output full, learned target | the same, plus heat and fuel, kinetic overstress, controller bounds, periodic edges |
| Learning window | needed before any replay | one checksum window confirms a compiled model (G-B4) |

**Kept unchanged (C29):** district detection (I2), boundary stock (I3), the sampler and its sweeps (F5),
ports (§3.3 of INDUSTRY.md), reconcile with chunk epochs (I8), the station hooks (H18), mail (H30), the
journal and op ids (W3), threats and damage (§13), and `cluster.rs`'s integer pools, diffusers and
safe breakpoints, which now integrate compiled rates instead of learned ones (§4.1).

---

## 1. The compiled model

### 1.1 Nodes

| Node | What it is | State the engine keeps | Examples |
|---|---|---|---|
| **Store** | boundary stock (I3) | integer level per good; slot capacity (items) or mB capacity (fluids) or energy capacity | vault, chest, tank, silo, packager target, postbox, PowerGrid battery |
| **Buffer** | work in progress | nothing simulated; its capacity only decides "blocked or not" | basin slots, depot, machine input and output slots, belt contents, chute slot |
| **Machine** | turns goods into goods | a recipe table and a cycle table per speed level (§2.9) | millstone, crushing wheels, press, mixer, saw, deployer, fan zone, crafter, spout, drain |
| **Mover** | moves an amount per period | amount, period, filter | funnel, mechanical arm, hopper, chute pull, packager, frogport |
| **Junction** | splits or merges by a rule | rule, branch order, round-robin phase | brass tunnel, andesite tunnel, arm output list, pipe branches, belt side insertion |
| **Heater** | heat level from fuel | fuel store, burn ticks per item, heat level | blaze burner; the boiler's heat sum |
| **Source** | goods from outside the graph | a rate, or a deposit level (I11) | deposit, water source under a pump, learned mob farm (I9), cobblestone generator |
| **Sink** | goods leave the world | a rate cap | blasting burns an item, a fan into lava, items spilled on the ground |
| **BlackBox** | a measured process | learned rates and learned coupling (I6) | an unknown machine, the residual (§5.1) |
| **Controller** | gates nodes by stock or time | thresholds, or a period | threshold switch, factory gauge, redstone requester on a clock |
| **Port** | where goods cross the district | events only | train station, postbox mail, ship dock, players |

A **kinetic domain** is one Create `KineticNetwork`: its sources (speed, capacity) and members (stress
impact, speed ratio to the source). It is not a flow node; it sets every member machine's speed level
and switches the whole domain off when overstressed (§2.1).

### 1.2 Edges

| Edge | Capacity | Latency | Goods |
|---|---|---|---|
| Belt run | `RPM / 480` entries per tick, an entry being one inserted stack (§2.3) | length / speed | any items, capacity shared |
| Chute column | one stack per chute block in flight (§2.3) | height / item motion | items |
| Pipe run | the pump's transfer rate, split among outputs (§2.4) | propagation, a few ticks | one fluid |
| Chain conveyor | `RPM / 360` blocks per tick, 20 packages per conveyor | length / speed | packages, routed by address |
| Adjacency | instant: a machine output into a store or buffer next to it | 0 | as the target accepts |
| Air current zone | exposure ticks along a belt or depot (§2.2) | | items |
| Line (train or ship) | periodic batch: b units every P ticks (§4.2, §8) | travel time | the learned or compiled exchange |

### 1.3 Goods on the graph

- **A good** is the GOOD dictionary entry of docs/industry-boundary.md §2.2: item plus components, fluid
  plus components (a Destroy mixture is its own good), energy, deposit.
- **Reachable goods.** From every store and source, the compiler propagates which goods can reach each
  node: filters remove goods from an edge, machines map each input good to a recipe and its outputs.
  The result is a multi-commodity graph where every edge lists the goods it can carry.
- **Recipes** come from the server's `RecipeManager` at compile time, never from jar JSON, so KubeJS and
  datapack changes are seen. Each machine kind looks up recipes the way its own block entity does
  (for example `AllRecipeTypes.MILLING.find`, the crusher's crushing-then-milling fallback in
  `CrushingWheelControllerBlockEntity.findRecipe`, the fan types' smelting and blasting order in
  `AllFanProcessingTypes.BlastingType.process`).
- **Chance outputs** enter at their expected count, `chance × count` per roll, as exact rationals.
  Sequenced assembly's weighted result pool enters as each result's weight over the total weight
  (`SequencedAssemblyRecipe.rollResult`).

### 1.4 What is read while loaded, and how

Extraction runs in Java on the server thread, under the bridge budget, sliced over ticks. Every read of
a non-public member goes through one accessor class per target mod, listed in PATCHES.md (P7).

| Component | Read | From |
|---|---|---|
| Kinetic domain | members, sources, network id | `KineticBlockEntity.network` (public `Long`), `TorquePropagator.networks` (package-private static map per level: accessor), `KineticNetwork.sources` and `members` (public) |
| Speeds | each member's speed and its ratio to the source | `KineticBlockEntity.getTheoreticalSpeed()`, `GeneratingKineticBlockEntity.getGeneratedSpeed()` |
| Stress | impact and capacity per block | `BlockStressValues.getImpact(block)` and `getCapacity(block)` (static, no side effect; `calculateStressApplied` writes a field, so it is not called) |
| Overstress | the live state, for the compile check | `KineticBlockEntity.isOverStressed()` |
| Machines | facing, mode, filters, recipe in progress | block state; `FilteringBehaviour` (`getFilter`, `count`, `upTo`); per-kind fields (`SawBlockEntity.recipeIndex`, `ArmBlockEntity` inputs, outputs and `selectionMode`) |
| Belts | controller, length, speed, segment processors, funnels and tunnels on segments | `BeltBlockEntity.getController`, `beltLength`, `getBeltMovementSpeed`; `BeltInventory.getBeltProcessingAtSegment` |
| Tunnels | mode, filtered sides | `BrassTunnelBlockEntity.selectionMode`, flap filters |
| Funnels | kind, extract or insert, filter amount | block (`AllBlocks.BRASS_FUNNEL`, `ANDESITE_FUNNEL`), `FunnelBlock.EXTRACTING`, filter |
| Chutes | item motion, smart filter | `ChuteBlockEntity.getItemMotion()`, `SmartChuteBlockEntity` filter |
| Pumps and pipes | pump speed, pipe graph, open ends | `PumpBlockEntity` speed; `FluidPropagator.getPipe` walk |
| Burners | fuel type, remaining burn | `BlazeBurnerBlockEntity.getActiveFuel()`, `getRemainingBurnTime()` |
| Boilers | size, engines, heat, water supply | `BoilerData.activeHeat`, `attachedEngines`, `waterSupply`; `FluidTankBlockEntity.getTotalTankSize()` |
| Air currents | zone length and type per segment | `AirCurrent.maxDistance`, `AirCurrent.segments` |
| Controllers | thresholds | `ThresholdSwitchBlockEntity.onWhenAbove`, `offWhenBelow`; `FactoryPanelBehaviour.targetedBy`, `recipeOutput`, filter count |
| Logistics | packager target, port address filter, chain links | `PackagerBlockEntity`, `PackagePortBlockEntity`, `ChainConveyorBlockEntity` |
| Stores | inventories and capacities | the sampler's own capability reads (H8) |
| Recipes | per reachable good | `RecipeManager` through each kind's own finder |
| Addon machines | per plugin (§7) | the plugin's extractor |

The compiler emits the graph to the engine as latest-wins records with a revision and a record count,
like a sweep (F5): a dropped record throws the compile away and it is redone. Proposed kinds: 0x0048
IND_GRAPH_NODE, 0x0049 IND_GRAPH_EDGE, 0x004A IND_GRAPH_RECIPE, 0x004B IND_GRAPH_DOMAIN, 0x004C
IND_GRAPH_END. The engine pages the graph with its district (A16); it is never recompiled from memory.

### 1.5 When it compiles

- **On edits only.** A compile runs when a district is first detected, on every IND_EDIT (player block
  events), and when its **fingerprint** changes: a 64-bit hash over each block entity's position, type
  and compile-relevant configuration (filters, modes, scroll values, thresholds), updated round-robin
  alongside the district's sweep (G-B3). H29 found no generic "configured" hook, and contraptions,
  schematicannons and reconciles change blocks with no player; the fingerprint catches all of them.
- **Not on speed changes:** a source changing speed is a breakpoint inside the model (§4.1), because the
  compile tabulates every speed level a domain can take (§2.9).
- **Consistency:** a compile spans several ticks; it restarts if the fingerprint changed meanwhile.
- **Compile states** add to INDUSTRY.md §2.3: *compiling*, *modelled* (every node exact or bounded),
  *modelled with black box* (some nodes measured), *not compilable* (a reason; the whole district falls
  back to the C19 box).

---

## 2. Rates

Every formula here is Create's own, read from source and found in the jar. "Cycle" is ticks per
operation; "steady" is the period when the machine never waits for input. Worked values are at 256 RPM,
the default `maxRotationSpeed` (`CKinetics.maxRotationSpeed`, 256), unless marked.

### 2.1 Kinetics and stress

| Rule | Formula | Source |
|---|---|---|
| Speed across a shaft | ratio 1, or a gearbox's custom `propagateRotationTo` | `RotationPropagator.getRotationSpeedModifier` |
| Cog to cog | -1 | same |
| Large cog to small cog | -2; small to large -0.5 | same |
| Large cog to large cog | ±1 by axis | same |
| Encased chain drive, gearshift | `ChainDriveBlock.getRotationSpeedModifier` | same |
| Speed controller | a fixed target speed into its large cog, so it starts a sub-source | `SpeedControllerBlockEntity.getConveyedSpeed` |
| Faster source wins | a faster neighbour re-roots the tree; opposite directions or more than `maxRotationSpeed` (256) break the block | `RotationPropagator.propagateNewSource` |
| Stress | sum over members of `impact × abs(speed)` | `KineticNetwork.calculateStress`, `getActualStressOf` |
| Capacity | sum over sources of `capacity × abs(generatedSpeed)` | `KineticNetwork.calculateCapacity`, `getActualCapacityOf` |
| Overstress | `capacity < stress` (stress enabled); then every member's `getSpeed()` is 0 | `KineticBlockEntity.updateFromNetwork`, `getSpeed` |
| Minimum speed | the mixer needs `SpeedLevel.MEDIUM`, at least `mediumSpeed` (30 RPM) | `MechanicalMixerBlock.getMinimumRequiredSpeedLevel`, `CKinetics.mediumSpeed` |

**What follows for the model:**
- Stress does not depend on work: an idle mixer loads its network exactly as a busy one. Overstress
  changes only when a source's speed or capacity changes, or a member's speed does.
- A member's speed is its source's speed times a rational ratio fixed at compile (the product of
  modifiers along the propagation tree); a speed controller is a sub-source. A domain is binary:
  running at its tabulated speeds, or stopped.
- Defaults read at compile (`AllBlocks`, `CStress.setImpact`, `setCapacity`,
  `BlockStressValues.setGeneratorSpeed`; overrides are read live): water wheel 32 SU per RPM at 8 RPM,
  large water wheel 128 at 4; impacts millstone 4, crushing wheel 8, press 8, mixer 4, fan 2, pump 4,
  saw 4, deployer 4, drill 4, crafter 2, arm 2. Steam and fuel engines (§2.5, §7.4) change speed and
  capacity with heat, water and fuel: they are the domain breakpoints.

### 2.2 Processing machines

| Machine | Cycle, from Create's code | Steady at 256 RPM | Source |
|---|---|---|---|
| **Millstone** | timer = recipe `processing_time` D (100 if no recipe), minus `s = clamp(floor(RPM/16), 1, 512)` per tick; one input item per cycle; cycle = `ceil(D/s) + 1` (one tick sets the timer). Stops while any output slot is full. | wheat flour (D 150): s 16, 11 ticks per wheat | `MillstoneBlockEntity.tick`, `getProcessingSpeed`, `process` |
| **Crushing wheels** | controller speed `c = RPM/50` from the wheel next to it, `v = 4c`. A whole inserted stack of n is one batch. Remaining D counts down by `clamp(v / log2(n), 0.25, 20)` until below 20, when the recipe is applied to all n (one roll per item), then by `clamp(v, 0.25, 20)` until 0. For n = 1, `log(1) = 0`, so the float division gives infinity and the clamp gives 20. | raw iron (D 400), n = 64: v 20.48, 113 ticks per 64; n = 1: 21 ticks per item | `CrushingWheelControllerBlock.updateSpeed`, `CrushingWheelControllerBlockEntity.tick`, `itemInserted`, `applyRecipe` |
| **Crusher input** | slot 0 takes a stack only when the inventory is empty | batch size is whatever the feeder delivers | `ProcessingInventory.isItemValid` |
| **Press** | `runningTicks += s`, `s = (int) lerp(clamp(RPM/512, 0, 1), 1, 60)`; the step that crosses 120 snaps to 120 and applies; done above 240 (`CYCLE`). Cycle = `ceil(120/s) + floor(120/s) + 2`. On a belt one item per cycle (`bulkPressing` false); in a basin it repeats while the recipe still matches. | s 30, 10 ticks; at 64 RPM s 8, 32 ticks | `PressingBehaviour.tick`, `getRunningTickSpeed`; `BeltPressingCallbacks.whenItemHeld`; `MechanicalPressBlockEntity.onPressingCompleted` |
| **Mixer** | first batch: 20 ticks wind-up, then `P = max(log2((int)(512/RPM)) × ceil(15 × t/100) + 1, 1)` ticks (t = recipe time, or factor 1 when 0), apply, 20 ticks wind-down. While the recipe still matches, `continueWithPreviousRecipe` sets `runningTicks = 20`, so the steady period is `P + 1`. | brass: P 16, 17 ticks per 2 brass; at 64 RPM 47 ticks | `MechanicalMixerBlockEntity.tick`, `continueWithPreviousRecipe` |
| **Basin** | 9 input and 9 output slots of 64; two input and two output tanks of 1,000 mB; outputs go at once to the block it faces, or drop when facing down | | `BasinBlockEntity` constructor, `addBehaviours`, `tryClearingSpoutputOverflow` |
| **Heat requirement** | HEATED needs a burner not NONE and not SMOULDERING; SUPERHEATED needs SEETHING | | `HeatCondition.testBlazeBurner` |
| **Saw** | one item at a time (`bulkCutting` false sets slot limit 1). Remaining = D (cutting recipe) or 50 (stonecutting) × `max(1, n/5)`; minus `clamp(RPM/24, 1, 128)` per tick until below 5; apply, set 20, count down again. Several matching recipes rotate: each start advances `recipeIndex`. | andesite alloy to shafts (D 200): 21 ticks per alloy | `SawBlockEntity.start`, `tick`, `applyRecipe` |
| **Deployer** | timer speed `u = clamp(RPM × 2, 8, 512)` (int); expanding 1,000, retracting 1,000, waiting 500; each phase `ceil(T/u)` ticks plus the tick that changes state. Period = `2 × (ceil(1000/u) + 1) + ceil(500/u) + 1`. | u 512: 8 ticks per application | `DeployerBlockEntity.tick`, `getTimerSpeed`, `start` |
| **Fan processing** | per stack of n on a belt or depot: `fanProcessingTime × ((n-1)/16 + 1) + 1` ticks inside the current (150 by default: 151 ticks for up to 16 items, 601 for 64). Independent of RPM. | | `FanProcessing.applyProcessing`, `CKinetics.fanProcessingTime` |
| **Fan range** | `lerp(min(RPM/256, 1), 3, 20)` blocks, cut short by obstacles | 20 blocks | `IAirCurrentSource.getMaxDistance`, `AirCurrent.getFlowLimit` |
| **Fan types** | blasting tries smelting, then blasting recipes; it burns any non-fire-resistant item with no recipe, and food whose smoking result equals its smelting result; smoking, splashing and haunting use their own recipes | | `AllFanProcessingTypes.BlastingType.canProcess`, `process` |
| **Mechanical crafter** | countdown speed `c = clamp(RPM, 4, 250)`; each hand-off along the chain counts 1,000 (`floor(1000/c) + 1` ticks), the final craft 2,000 (`floor(2000/c) + 1`); cycle = fill time + 1 + d × hand-off + craft, d being the longest chain to the final crafter | c 250: hand-off 5, craft 9 ticks | `MechanicalCrafterBlockEntity.tick`, `getCountDownSpeed` |
| **Spout, drain** | 20 ticks per item (`FILLING_TIME`), not kinetic | | `SpoutBlockEntity`, `ItemDrainBlockEntity` |

**A fan line on a belt.** An item is processed only if it stays in the current for the whole time:
exposure = zone length on the belt / belt speed. At 256 RPM a belt moves 0.533 blocks per tick (§2.3),
so 151 ticks needs 80.5 blocks of current: impossible within the 20-block range. The compiler computes
the exposure; a zone that is too short passes items through unprocessed, as in game. Over a depot,
exposure is unlimited.

**Batch size matters.** The crushing wheels' per-item time falls with the batch: 113 ticks for 64 raw
iron against 21 ticks for one. The batch is set by the feeder (a brass funnel's stack, an andesite
funnel's single item), so the compiler carries each edge's entry size (§2.3) into the machine model.

### 2.3 Belts, funnels, tunnels, chutes, arms, depots

| Component | Rate, from Create's code | At 256 RPM | Source |
|---|---|---|---|
| **Belt speed** | `RPM / 480` blocks per tick | 0.533 | `BeltBlockEntity.getBeltMovementSpeed` |
| **Belt capacity** | items keep a spacing of 1 block, so at most `speed` entries pass a point per tick; an entry is one inserted stack | 0.533 entries per tick; 34.1 items per tick in 64-stacks | `BeltInventory.tick` (`spacing = 1`) |
| **Belt insertion** | blocked while an item inserted at the same segment from the same side is within 1 block | | `BeltInventory.canInsertAtFromSide`, `isBlocking` |
| **Belt length** | at most 20 | | `CKinetics.maxBeltLength` |
| **Funnel timer** | after a transfer, `defaultExtractionTimer` (8) ticks of cooldown, so one extraction per 9 ticks | | `FunnelBlockEntity.tick`, `startCooldown`, `CLogistics.defaultExtractionTimer` |
| **Andesite funnel** | one item per extraction: amount is 1 when filtering is inactive, and only brass funnels filter | 0.111 items per tick | `FunnelBlockEntity.getAmountToExtract`, `supportsFiltering` |
| **Brass funnel** | the filter's count (default 64, "up to") per extraction | up to 7.1 items per tick | same; `FilteringBehaviour` (`count = 64`, `upTo = true`) |
| **Funnel onto a belt** | waits until the belt segment is free | | `FunnelBlockEntity.activateExtractingBeltFunnel` |
| **Funnel into the world** | waits until its last dropped item leaves the scan area | | `FunnelBlockEntity.activateExtractor` |
| **Brass tunnel** | holds one stack; distributes 2 ticks after a new item, retries every `brassTunnelTimer` (10) when outputs refuse; filtered sides are tried first | about one stack per 3 ticks, calibrated (§2.9) | `BrassTunnelBlockEntity.tick`, `distribute`; `CLogistics.brassTunnelTimer` |
| **Andesite tunnel** | at a junction, a passing stack of more than one gives one item to each open side; a side that refuses stops the stack | | `BeltTunnelInteractionHandler.flapTunnelsAndCheckIfStuck` |
| **Chute motion** | `(clamp((push + pull)/64, -20, 20) + (motion ≤ 0 ? -4 : 0)) / 20` blocks per tick; push and pull are attached fans' RPM | gravity only: 0.2 blocks per tick | `ChuteBlockEntity.getItemMotion`, `calculatePull`, `calculatePush` |
| **Chute pull** | a plain chute pulls up to 16 from the inventory above; a smart chute its filter count, else 64 | | `ChuteBlockEntity.getExtractionAmount`, `SmartChuteBlockEntity.getExtractionAmount` |
| **Chute capacity** | one stack per chute block, handed down when the next is free: about one stack per 5 ticks under gravity, calibrated | 16 items per 5 ticks from a plain chute | `ChuteBlockEntity.tick`, `handleDownwardOutput` |
| **Mechanical arm** | movement progress `+= min(256, RPM)/1024` per tick; a cycle is four moves (wait to search inputs, move to input, wait to search outputs, move to output) = `4 × ceil(1024 / min(256, RPM))` | 16 ticks per cycle | `ArmBlockEntity.tick`, `tickMovementProgress`, `lazyTick` |
| **Arm amount** | up to 64 from one slot, limited to what the outputs accept (simulated first) | up to 4 items per tick | `ArmInteractionPoint.extract`, `ArmBlockEntity.getDistributableAmount` |
| **Arm modes** | prefer first, round robin, forced round robin, for inputs and outputs | | `ArmBlockEntity.searchForItem`, `searchForDestination` |
| **Arm range** | 5 blocks | | `CLogistics.mechanicalArmRange` |
| **Depot** | one held stack up to its max stack size, plus an 8-slot output buffer for processing results | | `DepotBehaviour` (`maxStackSize`, `processingOutputBuffer`) |
| **Hopper (vanilla)** | one item per 8 ticks | 0.125 items per tick | `HopperBlockEntity.MOVE_ITEM_SPEED` (8, server jar) |

### 2.4 Pumps and pipes

| Rule | Formula | Source |
|---|---|---|
| Pressure | `RPM`, divided among parallel branches | `PumpBlockEntity` (pressure = `Math.abs(getSpeed())`, `addPressure(... pressure / parallelBranches)`) |
| Transfer rate | `max(1, pressure / 2)` mB per tick at the network's source | `FluidNetwork.tick` (`transferSpeed`) |
| Outputs | the transfer is split evenly among reachable outputs, the remainder to the first ones; an output that refuses drops out and the rest share again | `FluidNetwork.tick` (`dividedTransfer`, `remainder`) |
| Range | 16 blocks of pipe | `CFluids.mechanicalPumpRange` |
| Worked | a 256-RPM pump on one branch moves 128 mB per tick | arithmetic |

### 2.5 Heat, fuel and steam

| Rule | Formula | Source |
|---|---|---|
| Burn | the burner's remaining time falls by 1 every tick, working or not | `BlazeBurnerBlockEntity.tick` |
| Fuel values | regular: the item's furnace burn time (coal 1,600), or 1,600 for the regular tag; superheated: data map `create:superheated_blaze_burner_fuels` (blaze cake 3,200), or 3,200 for the special tag | `BlazeBurnerBlockEntity.tryUpdateFuel`; the jar's data map |
| Superheated decay | when special fuel runs out it becomes regular for `MAX_HEAT_CAPACITY / 2` = 5,000 ticks | `BlazeBurnerBlockEntity.tick` |
| Refuelling | automation (arms, deployers) adds fuel of the same type only when remaining ≤ `INSERTION_THRESHOLD` (500); only a player may top up beyond, to 10,000 | `tryUpdateFuel`, `BlazeBurnerBlock.tryInsert` (`forceOverflow = !(player instanceof FakePlayer)`) |
| Heat level | regular fuel is KINDLED, or FADING below 1.25% of 10,000 (125 ticks left); special fuel is SEETHING | `BlazeBurnerBlockEntity.getHeatLevel` |
| Steady fuel use | an arm-fed burner on coal uses one coal per 1,600 ticks, whatever the throughput | arithmetic from the above |
| Boiler heat | per burner under the tank: 1 if at least FADING, 2 if SEETHING; passive heaters give passive heat | `BoilerHeaters.blazeBurner`, `BoilerData.updateTemperature` |
| Boiler level | `min(activeHeat, floor(ceil(waterSupply)/10), size/4)`, at most 18; water supply is mB per tick, sampled every 5 ticks, the maximum of the last 10 samples | `BoilerData.getMaxHeatLevelForWaterSupply`, `getMaxHeatLevelForBoilerSize`, `getActualHeat`, `tick` (`SAMPLE_RATE` 5) |
| Engine efficiency | 1 per engine while engines ≤ level, else level / engines | `BoilerData.getEngineEfficiency` |
| Engine output | speed `16 × m`, `m = 1 + (eff ≥ 1 ? 3 : min(2, floor(4 eff)))`; capacity per RPM `eff × 1024 / m` | `PoweredShaftBlockEntity.getGeneratedSpeed`, `getSpeedModifier`, `calculateAddedStressCapacity`; `AllBlocks.STEAM_ENGINE` capacity 1,024 |
| Worked | a full engine turns at 64 RPM with 256 SU per RPM: 16,384 SU; a 256-RPM pump (128 mB per tick) feeds 12 boiler levels | arithmetic |

### 2.6 Storage

| Store | Capacity | Source |
|---|---|---|
| Item vault | `vaultCapacity` (20) stacks per block; width at most 3, length at most 3 × width: 3 × 3 × 9 is 1,620 slots | `ItemVaultBlockEntity` (`new ItemStackHandler(vaultCapacity)`, `getMaxWidth`, `getMaxLength`) |
| Fluid tank | `fluidTankCapacity` (8) × 1,000 mB per block; width at most 3, height at most 32: 2,304,000 mB at most | `FluidTankBlockEntity.getCapacityMultiplier`, `getMaxSize`, `getMaxHeight` |
| Postbox, frogport | 18 package slots | `PackagePortBlockEntity` (`new SmartInventory(18, ...)`) |
| Package | 9 stacks | `PackageItem.SLOTS` |

### 2.7 Create 6 logistics

| Component | Rule | Source |
|---|---|---|
| Packager | one package of up to 9 stacks per `CYCLE` (20) animation ticks; queued packages wait | `PackagerBlockEntity.tick`, `attemptToSend` |
| Frogport | one package per animation, `animationProgress` chasing 1 at 0.1 per tick (10 ticks); pushes to and pulls from adjacent inventories on its lazy tick when idle | `FrogportBlockEntity.tick`, `startAnimation`, `lazyTick` |
| Chain conveyor | packages move `RPM / 360` blocks per tick; at most `chainConveyorCapacity` (20) per conveyor | `ChainConveyorBlockEntity.tick`; `CLogistics.chainConveyorCapacity` |
| Routing | a package goes where its address matches a port's filter: deterministic | `PackagePortBlockEntity` address filter |
| Stock ticker | requests are player actions or redstone requester pulses: events, or a periodic controller when the pulse is a clock | `RedstoneRequesterBlockEntity` |
| Factory gauge | at most one request per `factoryGaugeTimer` (100) ticks; target, inputs and output per batch from its fields | `FactoryPanelBehaviour`; `CLogistics.factoryGaugeTimer` |
| Train mail | every 21st tick at a station; unloaded ports use an 18-slot offline buffer | INDUSTRY.md §0 (verified there) |

### 2.8 Sequenced assembly

- A transitional item stacks to 1 (`SequencedAssemblyItem`, `stacksTo(1)`), so every belt entry is one
  item and belt capacity is `RPM / 480` items per tick.
- An assembly is `sequence length × loops` steps (`SequencedAssemblyRecipe`, default `loops` 5), each a
  deploying, pressing, filling or cutting step with its own extra inputs and its own machine cycle.
- A loop layout sends each item past the same machines `loops` times, so a machine's load is `loops ×`
  the line's item rate. The compiler unrolls the loop from the recipe (§3.4).
- **Worked:** precision mechanism is 3 deploying steps × 5 loops = 15 deployer cycles per item; its
  pool is 120 for the mechanism out of 150 total weight, so 0.8 mechanisms per gold sheet in expectation
  (the jar's `sequenced_assembly/precision_mechanism.json`).

### 2.9 Exact cycle counts, and what is calibrated

- **Exact ticks from Create's own arithmetic.** Create counts in Java `float` (the arm's `1024f`, the
  crusher's `Math.log`), so the compiler does not re-derive closed forms with float edge cases. For each
  (kind, recipe duration, batch size, speed level) it runs that kind's countdown on the JVM for one
  cycle (at most a few hundred steps: the crusher's slowest is D / 0.25) and caches the tick count.
- **Speed levels.** A domain takes only a few speeds (a steam engine's `16 × m` for m in 1 to 4, a
  diesel throttle level, 0), enumerated at compile, so a speed change while unloaded is a lookup.
- **Hand-offs** between components (a belt re-catching the next item under a press, a basin check
  deferred one tick by `DeferralBehaviour`, a brass tunnel's 2 ticks, a chute's hand-down) cost a few
  ticks. A calibration GameTest suite measures each pair kind at four speed levels and ships the table
  (TUNABLE until measured). Each model kind has a contract test beside the real block entity (P7).

### 2.10 Tunables

| Name | Meaning | Default |
|---|---|---|
| `glass.checksumZ` | standard errors allowed between model and measurement (§5.3) | 3, PLACEHOLDER |
| `glass.checksumWindowMin` | shortest window that can confirm a compiled model | 6,000 ticks, PLACEHOLDER |
| `glass.solveRoundsMax` | coupling rounds before the solve falls back to its lower bound (§3.2) | `2 × multi-input machines + 2`, PLACEHOLDER |
| `glass.breakpointsPerEventMax` | breakpoints allowed between two events before a district is marked oscillating (§4.5) | 64, PLACEHOLDER |
| `glass.compileSliceMicros` | server-thread time per tick for extraction | 250 µs, PLACEHOLDER (inside the 1 ms bridge budget) |
| `glass.mismatchStrikes` | consecutive failed windows before a node kind is demoted to measured (§5.3) | 3, PLACEHOLDER |
| Calibration table | hand-off ticks per component pair and speed level | measured (§2.9) |

---

## 3. The solve

### 3.1 What Create does, stated as math

Create is a **back-pressure push network**. Every mover moves as much as it can, as soon as the next
thing has room; every machine runs whenever it has inputs, power, heat and output space; nothing plans.
In steady state each node's rate is

```
rate(node) = min( own capacity at its speed level,
                  what its inputs supply, by its recipe ratios,
                  what its outputs accept )
```

with supply split at junctions by the junction's rule and acceptance summed over a junction's
branches. The steady state of the district is the **greatest fixed point** of this operator: start every
edge at its capacity and lower rates until nothing changes. It is not a throughput optimum. An LP that
maximizes output would route better than Create does, so it would make more than reality; the LP is
used only as an upper bound (§3.5).

### 3.2 The algorithm

1. **Condense cycles.** Tarjan's strongly connected components over the item and fluid graph. Most
   factories are acyclic; recognised loops are unrolled, the rest become one SCC node (§3.4).
2. **Backward pass (acceptance).** In reverse topological order, each node computes what it can accept
   per input edge: a store below capacity accepts up to the edge capacity; a full store accepts its
   outflow; a machine accepts `ratio × min(own capacity, the tightest output's acceptance / its yield)`,
   because a Create machine stops when **any** output is blocked (`MillstoneBlockEntity.tick` returns
   if any output slot is full); a junction accepts by its rule (§3.3).
3. **Forward pass (supply).** In topological order, each node runs at `min(supply by recipe ratio,
   capacity, acceptance)`; a junction allocates its supply to branches by its rule, capped by each
   branch's acceptance (water-filling for fair rules, in order for priority rules).
4. **Couple multi-input machines.** A mixer short of zinc consumes copper only at the zinc-limited
   rate; the surplus copper backs up. Set that input edge's acceptance to the consumed rate and repeat
   2 and 3. Acceptances only fall, so the rounds are monotone. Stop when no acceptance changes, or after
   `glass.solveRoundsMax` rounds; in the latter case use the lower bound L of §3.5 and log it.
5. **Kinetic gate.** Machines in an overstressed or stopped domain have capacity 0 (§3.6).

**Cost:** O(rounds × (nodes + edges)) rational operations per solve. Typical factories converge in two
or three rounds (ESTIMATE; the G1 corpus measures it, §11.3).

### 3.3 Routing rules

| Junction | Create's rule (source) | Model | Class |
|---|---|---|---|
| Brass tunnel SPLIT | each stack divided evenly among accepting sides (`distribute`) | equal shares with water-filling over acceptances | exact in rate |
| Brass tunnel FORCED_SPLIT | distributes only if every side takes its share | equal shares, all limited by the slowest side | exact in rate |
| Brass tunnel ROUND_ROBIN | whole stacks to the next accepting side (`previousOutputIndex`) | equal shares in stacks, water-filling | exact in rate |
| Brass tunnel FORCED_ROUND_ROBIN | waits for the next side in order | equal shares limited by the slowest side | exact in rate |
| Brass tunnel PREFER_NEAREST | first accepting side in distance order | priority fill in that order | exact |
| Brass tunnel RANDOMIZE | random start index | equal shares in expectation | bounded |
| Brass tunnel SYNCHRONIZE | linked tunnels release together | each at the rate of the slowest input | bounded |
| Brass tunnel filters | filtered sides are tried before unfiltered ones (`distributionTargets`) | filtered first, then the rest | exact |
| Andesite tunnel | one item per open side per passing stack; a refusing side stops the stack | fixed side rates per passing entry | exact in rate |
| Arm PREFER_FIRST | the first input or output that works | priority | exact |
| Arm ROUND_ROBIN | starts after the last used index, skips refusing ones | equal shares, water-filling | exact in rate |
| Arm FORCED_ROUND_ROBIN | only the next index | equal shares limited by the slowest | exact in rate |
| Pipe branches | even split with remainder, refusing outputs drop out (`FluidNetwork.tick`) | water-filling | exact |
| Belt side insertion | waits only for its own side's last insertion; items carried along the belt never block it (`canInsertAtFromSide`, `isBlocking`) | side inputs first, the main line takes the rest (GB72) | exact in rate; bounded when the segment is full |
| Belt funnel on the side | takes what matches its filter and fits; the rest passes | filter split, acceptance-limited | exact |
| Saw with several recipes | rotates `recipeIndex` at each start | equal shares among matching recipes, unless filtered | exact in rate |
| Package routing | address match | deterministic | exact |
| Merges of equal priority | arrival order | proportional to supply | bounded |

"Exact in rate" means the long-run rate is exact; the order of individual stacks is not modelled.

**Changed by Ben (GB72, 2026-10-21):** belt side insertion is modelled as Create has
it. `BeltInventory.canInsertAtFromSide` refuses an insertion only while an earlier insertion at the same
segment from the same side is within reach (`isBlocking`: inserted at 0.4375 along the segment, it blocks
until it has moved more than 1.0625 blocks); the items carried along the belt are never checked, so a
side feed merges into through traffic instead of waiting for gaps. A belt's end handing to the next belt,
a funnel or chute onto a belt, and a belt's corner into the next belt of a loop are all side insertions;
only the belt's own links from segment to segment are its through traffic (edge flag THROUGH). As math,
at a belt segment with capacity c (entries per tick), side inputs s_1..s_k and through traffic t:

```
inserted_i = s_i x min(1, c / sum(s))         (one proportional group, first)
through    = min(t, c - sum(inserted))        (the rest of the segment)
```

Below capacity both are exact. When the segment is full, which gaps each side wins is sub-tick timing,
so the merge is bounded (shared capacity) once more than one input flows. U is unchanged: it is
routing-free and contains both orders. The rejected design is the row's old text, "only into gaps;
items already on the belt pass first (`canInsertAtFromSide`)" with the model "main line first, side
inputs share the rest", class bounded; the solver keeps it behind `solve::BELT_INSERTIONS_FIRST` (false
gives it back).

### 3.4 Cycles

- **Sequenced assembly loops** are unrolled from the recipe: each machine in the loop carries `loops ×`
  the item rate. A loop fed faster than its machines finish products stalls (below, GB72).
- **Return loops** (overflow sent back upstream) carry nothing in steady state when downstream has spare
  capacity, which the solve checks; otherwise the downstream bottleneck sets the throughput.
- **Any other SCC** is one node solved as a small LP. That optimum is only an upper bound, so its replay
  rate is `min(LP bound, measured)` and it waits for a measurement before replaying (§3.5).

**Changed by Ben (GB72, 2026-10-21):** an over-fed sequenced-assembly loop is stalled,
not settled at capacity. Let mu be the host's rate of finished products (one per cycle, the cycle being
the slowest machine's work per product) and f the rate at which the product's base good comes onto the
loop from outside. A side insertion never waits for the items going round (§3.3), so while f > mu the
loop's population grows by at least f - mu per tick until every belt slot is full. Then nothing moves:
`BeltInventory.handleBeltProcessingAndCheckIfRemoved` returns before processing an item with no room to
move, so a stuck item is never worked on again, and an exit funnel takes only items that cross it. The
jam is absorbing, so the end state does not depend on tick order. Corpus 6 measured it with the feeder at
64 RPM (a sheet per 9 ticks against a product per 30): every one of the 28 loop segments full within the
1,000-tick warm-up and nothing out over 7,200 ticks; `precision_overfed` is that factory. The solve: once
L has converged, each saturated SEQUENCED host on a cycle whose loop (the goods edges' strongly connected
component around it) holds no store gets its feed measured by a probe solve with the loop as a sink. If
f > mu for any of its base goods, L is 0 on every goods edge into the loop and the fixed point is solved
again. f = mu runs. U keeps the capacity (the LP passes goods through the loop's recipe-less
deployers, as L does), and the goods stay bounded (cycle), so the checksum window `[0, U]` holds either
outcome. Not verified: once the jam reaches the feed point, the feed shares slots
with the queue (about one per two queue advances), so its rate falls to about half the head machine's
pass rate s; with mu = s / P (P passes per product at the head machine) the jam needs P > 2, and a loop of
one or two passes may settle at capacity instead. The rejected design is the old reading, which the
solver keeps behind `solve::OVERFED_LOOPS_STALL` (false gives it back): the loop settles at its machines'
capacity, the feed held back to mu.

### 3.5 Bounds, and never more than reality

For every output good the compiler produces two numbers:
- **U**, the physical upper bound: the LP maximum over the same graph with every routing choice free.
  Nothing Create can do beats it.
- **L**, the pessimistic lower bound: the fixed point of §3.2 with every *bounded* rule (random,
  synchronize, equal-priority merges, unconverged rounds) resolved against production.

When every rule on a good's paths is exact, L is the model's value. The replay uses

```
replay rate = clamp(measured, L, U)   for a good the confirming window measured
            = L                       for a good it did not (no flow during the window)
```

and never more than `measured + tolerance` (§5.3). So the physics bounds the measurement, and the
measurement picks the point inside the bounds. Four further guards:
1. **Rounding:** rates are exact rationals, converted to the kernel's 32.32 fixed point by floor.
2. **Integers:** a machine never consumes stock that is not there; breakpoints are exact or early, never
   late (F9, kept).
3. **Restarts:** after any stop, outputs resume only after the path latency (§4.4).
4. **Chance outputs:** exact expectation through integer diffusion (G-B11), never a lucky draw.

### 3.6 Kinetic domains in the solve

- At compile, each domain records its sources, every member's ratio and impact, and the speed levels.
- At a domain breakpoint (a source's speed or capacity changes), the engine recomputes `stress` and
  `capacity` with §2.1's sums: O(members). If overstressed, every member machine's capacity is 0 and the
  flow solve reruns; when capacity returns, they restart after their latency.
- **Fuel engines stop burning when overstressed** (Diesel, §7.4), so an overstressed domain does not
  drain its fuel. Steam boilers keep burning their burners regardless (§2.5).

### 3.7 Arithmetic

Rationals with i64 numerators and denominators and i128 intermediates, reduced by gcd. Belt speeds are
`RPM/480`, cycles are integers, gear ratios are 2, 1/2 and -1: denominators stay small. A rate whose
denominator would overflow is rounded down to a nearby rational with a smaller denominator: still a
lower bound. Fixed point only at the end (O8).

---

## 4. Time

### 4.1 Regimes and breakpoints

A **regime** is the set of stores at a bound (empty or full), domains running or stopped, heaters lit
or out, and controllers on or off. Within a regime every rate is constant, so every store level is
linear (or a step function, §4.2). The solve runs once per regime and hands `cluster.rs` one process
per district whose terms are the net rate per (store, good). The cluster integrates it with its
diffusers and finds the next breakpoint exactly as today (F9). `PoolKey` gains a store index:
`Store(DistrictId, StoreIdx, Good)`, and a district's item space becomes one `Space` per store.

| Breakpoint | Detected by | Then |
|---|---|---|
| An input store empties | the cluster's pool constraint | re-solve with that store at "empty: outflow ≤ inflow" |
| An output store fills | the cluster's pool or space constraint | re-solve with "full: inflow ≤ outflow" |
| A burner's fuel store empties | fuel pool; plus the remaining burn ticks (at most the fuel's burn time) | heat level drops; heated recipes stop; boiler level falls |
| A domain changes speed or overstresses | a source's fuel, water or heat crosses a level | §3.6 |
| A controller switches | its store crosses a threshold | §4.3 |
| A deposit runs dry | the deposit pool (I11, F15) | that source stops |
| A season or day boundary | IND_CONDITIONS (I12) | the season's model, or a periodic source's next phase |
| A train arrives, a package lands, a player edits, an attack lands | events (real, journaled) | apply, then re-solve if a bound changed |
| A simulated ship calls at a port | its periodic line (§8.2) | exchange, then re-solve if a bound changed |

### 4.2 Continuous and periodic edges

- **Continuous edges** carry a rate; a store fed only by them is linear between breakpoints.
- **Periodic edges** carry b units every P ticks at phase φ (a forecast train line, a simulated ship, a
  packager's packages (F26), a redstone clock, a day-night source). The cumulative count is the closed
  form `b × (floor((t - φ)/P) + 1)`: O(1), never a step per period.
- **Breakpoints with periodic terms.** The first time a store crosses a bound is found against a safe
  linear envelope: inflows as `b(t - φ)/P - b`, outflows as `b(t - φ)/P + b`. At that time the exact
  closed form is checked; if the crossing has not happened, the next envelope breakpoint is computed.
  The envelope error is at most the sum of the batches, so the refinement takes a bounded number of
  steps, independent of elapsed time.
- **Reconcile** uses the exact closed forms, so the deltas written on load are exact.

### 4.3 Controllers become bounds

A threshold switch that stops production at S and restarts at R makes Create cycle between them. The
sim does not step that limit cycle: over time the controlled stock's inflow equals its outflow, so the
controller is a **bound**: the store counts as full at R (the restart level, the lower of the two) with
inflow limited to outflow. This is one regime instead of two per cycle, costs nothing per cycle, and
holds the stock at the cycle's lowest level, never above reality. Factory gauges are the same, with
their target as R (§2.7). A clock-driven redstone controller is a periodic edge (§4.2). This replaces
the trickle restarts of the black box (`restart_min_run_ticks`, F10, overnight finding 4).

**Changed by Ben (G-B8, 2026-10-20):** threshold switches and gauges are simulated as on/off cycles,
not bounds. Strictly periodic cycles use G-B9's closed forms (§4.2), so their cost does not grow with
elapsed time. The bound above remains the check: a simulated cycle never averages above it. The build
works out the cycle model.

### 4.4 Latency and restarts

- **Path latency** is computed, not learned (replacing F23): belt length / speed, chute height / item
  motion, each machine's first cycle (the mixer's wind-up), the calibrated hand-offs. After a stop,
  outputs resume only after the latency of the path that fed them. Work in progress stays frozen in the
  world (I3), as today.

### 4.5 Catch-up cost

- **Per regime:** one solve, one cluster update. **Per catch-up:** O(breakpoints + events), whatever
  the elapsed time, because periodic edges and controllers are closed forms.
- **Breakpoints between events are bounded.** Without events, stores only drain inputs and fill
  outputs; each store changes status a bounded number of times. A district that exceeds
  `glass.breakpointsPerEventMax` between two events is marked *oscillating*: it falls back to the black
  box for that regime and the ledger says why. The bench counts these (§9.3).
- **Composition rule (O4, O18):** reads never re-anchor or re-solve; solves happen only at breakpoints
  and events, which do not depend on who reads when, so `catch_up(t0 → t2)` equals
  `catch_up(t1 → t2) ∘ catch_up(t0 → t1)`. Property-tested with periodic edges included.

---

## 5. Learned coupling and the black box

### 5.1 Black-box nodes

- **Unknown machines.** A block entity with an item, fluid or energy capability that no model claims
  is joined to the graph by the modelled edges that touch it (the funnel feeding it, the pipe out).
- **One residual per district.** Boundary data sees only district totals, so unknown nodes cannot be
  told apart. They share one **residual node** (G-B13) whose rates are the measured boundary rates minus
  the model's predictions, fitted by the existing learner (C29: windows, steady test, slopes). It runs
  like a machine: only while its coupled inputs are supplied and its outputs accept.
- **Not compilable at all** (a reason in the ledger): the district is the C19 box, unchanged.

### 5.2 Learned coupling (I6)

- Each black-box node keeps a dependency matrix `D[input][output]`, one bit per pair, starting "all
  coupled" (never over-produces). A **starvation event** is an input store reaching zero while loaded;
  an output that keeps its rate through it clears that bit. In the replay, output o runs while every
  input i with `D[i][o]` set is supplied.
- **For modelled nodes coupling is structural** (graph paths), and starvation events become a second
  checksum: the model predicts which outputs stop when an input empties, and the sampler sees which did.

### 5.3 Measurement as the checksum

- **While loaded,** the sampler's sweeps keep fitting slopes per good (F5, F7). The model predicts the
  same goods' net rates for the same window, from the regimes the district was in.
- **The test:** a good fails when `abs(measured - predicted) > glass.checksumZ × σ + q`, σ being the
  slope's standard error and q the quantization of batches over the window (b / W for a batch of b).
- **Model too optimistic** (measured lower):
  1. at once, a derating factor η = measured / predicted goes on the path's limiting node and the solve
     reruns, so inputs stay in ratio and conservation holds;
  2. **localization:** IND_PROBE_SET (district, buffer list) adds those internal buffers (basins, depots,
     belt counts) to that district's sweeps; a buffer whose level climbs sits upstream of a node slower
     than modelled;
  3. after `glass.mismatchStrikes` failed windows that node kind is demoted to measured for that
     district, and a report (the subgraph and both rates) is written so the model can be fixed.
- **Model too pessimistic** (measured higher): if measured ≤ U, the bounded rules are calibrated to the
  measurement (the replay uses `clamp(measured, L, U)`); if measured > U, something unmodelled produces:
  the excess goes to the residual node as a learned source, which must pass the usual steady test and
  is subject to the port rules (hand-feeding is a port, not production).
- **Event checksum:** a predicted stop that did not happen, or a stop the model did not predict, counts
  as a failed window for the nodes on that path.
- **No visibility (I18):** checksum results go to the server log and `/rtsim district` for operators,
  never to an in-world readout.

---

## 6. Partial loads, mob farms, Ender Transmission, PowerGrid

### 6.1 Partial loads (I7)

- Java reports per-chunk ticking changes (proposed IND_CHUNK_TICKING: district, chunk, ticking). Nodes
  in ticking chunks are **live** (Create runs them; their stores are sampled); the rest are **simulated**.
- The simulated subgraph is solved on its own. **Cut edges** (a belt, chute or pipe from a live chunk
  into a simulated one) carry zero flow both ways (G-B6): Create refuses to move items into chunks that
  are not loaded (`BeltTunnelInteractionHandler` returns stuck when `!world.isLoaded(outpos)`). A
  GameTest confirms it for a belt whose controller is loaded and whose far end is not.
- Live stores follow the sampler, simulated ones their pools and pending deltas (I8). When the cut
  closes (all loaded, or all unloaded) the district re-solves as one.

**Changed by Ben (G-B6, 2026-10-20):** flow across the loaded/unloaded edge is modelled, not zero. The
zero-flow text above is the rejected design. The build works out how cut edges carry flow without making
more than a fully loaded factory would (G-B2's ceiling still holds).

### 6.2 Mob farms (I9)

**Changed by Ben (G-B17, 2026-10-20):** mob farms model vanilla's spawning rules for their spawn spaces
(light, block, mob cap, distance and pack size rules), not learned sources. The learned-source text below
is the rejected design, kept as the check a modelled farm is measured against. C32 adds farms of thousands
of player-owned animals (a sheep airship), which must cost little while unloaded.

- Spawning and killing are not compiled. A mob farm is a **Source** node whose drop rates are measured
  while loaded (`LivingDropsEvent` inside the district, entering the collection system's first modelled
  edge) and replayed while unloaded, as I9 requires.
- It is **conditioned** on day or night and season (I12): a night-only farm is a periodic source with a
  24,000-tick period (§4.2). Everything after the drops (hoppers, funnels, belts, vaults) is compiled.

### 6.3 Ender Transmission (I13, local only)

- **Item and fluid transmitters** on one channel and password share one capability through
  `MatterTransmitterNetwork` (`AbstractMatterTransmitterBlockEntity.getInv`, jar). The compiler makes one
  Buffer node per channel, joined to every transmitter's neighbours.
- **Energy transmitters** add each other as kinetic propagation neighbours
  (`EnergyTransmitterBlockEntity.addPropagationLocations`, `isCustomConnection`, jar), so the two ends are
  one kinetic domain.
- Both join their endpoints into one district or one vehicle (I2's closure under physical connection),
  so a channel is never a link between distant districts. A transmitter's own rate limit is UNVERIFIED;
  until read, the channel buffer is limited only by the movers around it.

### 6.4 PowerGrid coupling (I13)

- **Inside a district,** the PowerGrid plugin compiles the electric subnetwork: generators and motors
  (each coupled to a kinetic domain), loads, batteries (Stores of energy, INDUSTRY.md §7), wires.
  PowerGrid solves its circuits with a linear solver (`ISolver` with `DirectSolver`, `BiCGSTABSolver`,
  `GMRESSolver`, jar), so the plugin rebuilds the same linear system in Rust (modified nodal analysis on
  the extracted element values), solves it per regime, and checks the result against PowerGrid's live
  node values at compile. Element parameter fields are UNVERIFIED.
- **Across districts,** a `TransmissionLine` makes a link: the source district's surplus power is a
  Source in the sink's graph, coupled through the existing cluster (F8, F21). The two are solved
  together when either changes regime.
- **Batteries** are energy stores with breakpoints (empty, full); a domain driven by an electric motor
  stops when its battery and generation fail.

---

## 7. Addon models

### 7.1 The plugin interface

Models come in the three tiers Ben chose for plugins (P1):

| Tier | What a model author writes | Runs where | For |
|---|---|---|---|
| 1. Template data | `data/<ns>/mic_rtsim/machine_model/<block>.json`: a template name and its constants | engine (Rust), from data | machines built on Create's base classes |
| 2. Native model | a Rust type implementing `MachineModel` in an SDK cdylib | engine, through mic_native | heavy math (PowerGrid circuits) |
| 3. Java extractor and evaluator | a `GlassModelProvider` registered with `IndustryApi` | server thread, at compile only | math that lives in the mod's Java (Destroy chemistry) |

- **Templates** (tier 1): `millstone`, `crusher`, `press`, `basin_operator` (the mixer's P), `saw`,
  `deployer`, `fan_zone`, `crafter`, `timed` (fixed ticks per item, optional heat), `kinetic_source`
  (speed, capacity, fuel store, burn rate, whether it burns while overstressed), `heater`, `store`,
  `buffer`, `mover`, `controller`, `black_box`; each with its constants, stress impact and recipe types.
- **`GlassModelProvider` (Java)**, extending `IndustryPlugin`: `claims(BlockEntity)`,
  `extract(BlockEntity, GraphSink)` (nodes, edges, recipes, cycle tables), `fingerprint(BlockEntity)`,
  and an optional `evaluate(BlockEntity, Budget)` for compile-time evaluation.
- **`MachineModel` (Rust SDK):** `caps(speed_level)`, `recipes()`, and optional `regime(&Inputs)` for
  rates that depend on inputs nonlinearly; it obeys P1's SDK contracts (cadence, catch-up and
  breakpoints, integer ledgers, no threads).
- **The engine never calls Java** (PLAN.md §3.2): Java evaluators run at compile and hand over tables,
  one entry per speed level or input regime. Every model has a contract GameTest (P7).

### 7.2 Create Vintage 1.3.9 (javap)

| Machine | Template | Evidence |
|---|---|---|
| Centrifuge | `basin_operator`: P = `clamp(log2((int)(512/RPM)) × ceil(15 × t/100) + 1, 1, 512)` | `CentrifugeBlockEntity.tick` bytecode: the mixer's formula with a clamp to 512 added |
| Vibrating table | `millstone`: `clamp((int)(RPM/16), 1, 512)` | `VibratingTableBlockEntity.getProcessingSpeed` |
| Grinder | `saw`: divisor 24, clamp 1 to 128, thresholds 5 and 20 | `GrinderBlockEntity.tick` constants; polishing speed limits 32 and 128 via `PolishingRecipe` (UNVERIFIED meaning) |
| Helve hammer | `millstone`-like speed `clamp(RPM/16, 1, 512)`; its 500-tick constants UNVERIFIED | `HelveBlockEntity.getProcessingSpeed(float)` |
| Curving press | `press` (`CurvingBehaviour`), cycle constants UNVERIFIED | `CurvingPressBlockEntity.getPressingBehaviour` |
| Vacuum chamber | `timed`, duration scaled by `clamp(speed level / 32, 0.5, 8)` (partly read) | `VacuumChamberBlockEntity.getScaledRecipeDuration` |
| Laser, lathe, coiling machine | `black_box` until read | UNVERIFIED |

### 7.3 Create Metallurgy 1.0.3 (javap)

| Machine | Template | Evidence |
|---|---|---|
| Foundry mixer | `basin_operator`, the mixer's own tick | `FoundryMixerBlockEntity extends MechanicalMixerBlockEntity` |
| Foundry basin | `buffer` with 9 slots and 1,000 mB tanks | `FoundryBasinBlockEntity extends BasinBlockEntity`; constants 9 and 1,000 |
| Belt grinder | `saw`: 24, 128, 5, 20 | `BeltGrinderBlockEntity.tick` |
| Casting table and basin | `timed`: the recipe's processing time, minus 1 per tick, or 2 in a fan's air current | `CastingBlockEntity.tick`, `startProcess`, `isInAirCurrent` |
| Crucible, faucet, labeling station | `black_box` until read | UNVERIFIED |
| Heat | Create's blaze burner rules (§2.5) | uses `BlazeBurnerBlock.HeatLevel` |

### 7.4 Create Diesel Generators 1.3.15 (javap)

- **Engines are `kinetic_source` nodes.** Per fuel type the data gives speed, strength and burn rate per
  engine kind (`FuelType.PerEngineProperties`): diesel is 96 RPM, 6,144 strength, 0.05 burn (normal);
  96, 8,192, 0.05 (modular); 224, 16,384, 0.05 (huge) (`fuel_type/diesel.json`).
- **Burn:** each tick `fuelDebt += burnRate × throttle`, and 1 mB is drained per whole unit of debt, but
  only while `enabled()` and not `isOverStressed()` (`DieselEngineBlockEntity.tick` bytecode). Normal
  diesel at full throttle uses 0.05 mB per tick, 1 mB per second.
- **Capacity** per RPM is the upgraded capacity over `max(0.01, upgraded speed)` (`tick` bytecode;
  `EngineUpgrades.getSpeed`, `getCapacity`), so an engine supplies its strength in SU at its speed. What
  the upgrades change is UNVERIFIED.
- **The fuel tank** is a Store with a breakpoint at empty (the domain loses that source).
- **Pumpjacks and oil** are deposits (I11, `OilChunksSavedData`, unchanged). Bulk fermenter, distillation
  tank and burner: `black_box` until read; the burner's `burner_multiplier` (0.6 for diesel) is UNVERIFIED.

### 7.5 Destroy 0.4.1 (javap)

- **Vats** run real chemistry: `VatControllerBlockEntity.tick` calls `LegacyMixture.reactForTick` and
  `heat`; `LegacyMixture.isAtEquilibrium` exists.
- **Tier 3 evaluator:** at compile, the plugin copies the vat's mixture (`LegacyMixture.readNBT` of its
  contents) and steps `reactForTick` on the copy, under a step budget, until `isAtEquilibrium`. If the
  inflow mixture is steady and the vat reaches equilibrium, the vat is a machine whose recipe is (inflow
  mixture to outflow mixture at that composition) at its throughput. If not, it is a `black_box` node
  with the mixture goods, and the ledger says "mixture composition not steady" (INDUSTRY.md §6 case 3).
- **Basin reactions** use `LegacyMixture.reactInBasin` the same way, per recipe.
- **Centrifuge, bubble cap, cooler, dynamo** (electrified by mic-destroy-electric): `black_box` until
  read; UNVERIFIED.
- **Pollution** stays I14: emission rates are attributed per node, applied as before.

### 7.6 Create Nuclear 1.3.2-beta.3 (javap)

- **Fuel and cooler timers:** `ReactorControllerBlockEntity.tick` counts `tmpUraniumTimer` and
  `tmpGraphiteTimer` down, extracts one item when one reaches zero, and resets it to `uraniumTimer` or
  `graphiteTimer`. Heat (`heat`) is compared with thresholds (600 is one of them).
- **Model:** a `timed` consumer of fuel and cooler at those periods, plus a `kinetic_source` whose
  output comes from `reactorPower` through `ReactorOutputEntity`. The output formula is UNVERIFIED.
- **Steady only:** a rising heat trend while loaded makes the node `black_box` and freezes the
  district's replay with "reactor not steady". Meltdown is never simulated (INDUSTRY.md §6 case 7).

### 7.7 Create Enchantment Industry 2.5.4 (javap)

| Machine | Rule | Evidence |
|---|---|---|
| Blaze enchanter | `timed`, `ENCHANTING_TIME` 200 ticks, heat at least FADING, SEETHING checked separately | `BlazeEnchanterBlockEntity` constants |
| Blaze forger | `timed`, `FORGING_TIME` 200 | `BlazeForgerBlockEntity` |
| Printer | `timed`, `PROCESSING_TIME` 50 | `PrinterBlockEntity` |
| Experience used | from the recipe through `EnchanterBehaviour` | UNVERIFIED |
| Grindstone drain, experience hatch | `black_box` until read | UNVERIFIED |

---

## 8. Trains, stations and Sable ships

### 8.1 Trains stay real; stations are compiled

Create moves every train while unloaded (INDUSTRY.md §0); the glass box adds the station side and
forecasts.
- **Compiled exchange.** A station's subgraph is compiled like the rest: Portable Storage Interfaces
  (which expose the train's inventory while connected), the movers around them, and the stores behind.
  At a simulated station (I15), the exchange is computed, not only replayed:
  `moved = min(carriage contents or room, store stock or room, mover rate × dwell)`.
- **Dwell** comes from the schedule's wait condition: "cargo idle for N s" is the transfer time plus N
  (`IdleCargoCondition`), a timed wait is its time, a threshold is reached when the amount moved meets it.
  The PSI releases after `psiTimeout` (60) ticks without transfer (`CLogistics.psiTimeout`).
- **Fallback:** a station whose subgraph has black-box or bounded parts keeps the learned exchange (I15,
  F19). Both go through `TrainCargoManager` (H17).
- **Train speed for forecasts:** top speed 28 blocks per second, 1.4 per tick (40 and 2.0 when powered),
  acceleration 3/400 blocks per tick² (`Train.maxSpeed`, `acceleration`; `CTrains`).

### 8.2 Lines as periodic edges

- A **line** is (station A, station B, period P, phase φ, travel times, batch per call per good). For
  trains it is learned from real arrivals (H18) and used only for forecasts and threat bookings: real
  arrivals are what move goods.
- **Bookings without growth:** a line is booked once into the region and time-window index (A15,
  `threat.rs` `LegIndex`) as a **standing route** (path, period, phase). A threat leg finds crossing
  times by modular arithmetic, so booking costs nothing per trip.

### 8.3 Player Sable ships (C30, I17)

**The sailbot.** A player's ship carries vaults between port A and port B, driven by its own autopilot.

1. **Learning while loaded.** The voyage plugin tracks the sub-level's pose (`SubLevel.logicalPose()`,
   jar) and its vaults (ordinary block entities in the sub-level), sampled as a **vehicle district**. A
   call is the pose staying within R of a learned dock pose for N ticks (both TUNABLE; an Aeronautics
   docking connector's state would be better, UNVERIFIED). Each call's cargo diff is the exchange, as I15.
2. **The edge:** period P, travel times each way, dwell per port, from observed round trips; capacity C
   is the ship's vault slots. Each call moves `min(learned exchange, available stock, room on the ship)`.
3. **While unloaded** it is a periodic edge between the two ports' graphs (§4.2): exact closed forms,
   O(1) per catch-up however many voyages passed. The vehicle's own machines compile like any district.
4. **Position. Changed by Ben (G-B7, 2026-10-20):** a simulated ship, a player sailbot included, may
   materialize anywhere a player can see it, not only at docks. The rest of this item is the rejected
   design. C32: a factory can itself be a moving Sable ship, compiled in the ship's own frame, and
   ship-to-ship ferries with both ends moving belong to the voyage plugin (C30).
   Rejected: a ship only ever materializes at a dock. On a port's load, a ship due there is
   moved to the dock pose with Sable's `PhysicsPipeline.teleport(body, position, rotation)` (jar). If a
   player loads the ship's last pose while it is at sea in the sim, it is moved to the next dock on its
   route at once; its cargo still follows the timetable. Whether `teleport` works on a sub-level whose
   holding chunk is unloaded is UNVERIFIED; a GameTest decides before this is built.
5. **Threats** book the route as a standing route (§8.2), reusing `rtsim-ships`' voyage phases. A ship attacked at sea loses cargo like a
   train (I24).
6. **Not learnable:** a ship whose route changes from trip to trip (no stable period over the window)
   stays frozen, as I17 was before C30, and the ledger says why.

---

## 9. Performance

### 9.1 Cost per district, per event

| Work | Thread | Cost | Grows with | Never with |
|---|---|---|---|---|
| Extraction | server, sliced at `glass.compileSliceMicros` | about 1 to 5 µs per block entity (ESTIMATE from H10's 0.5 to 1 µs per small inventory read) | block entities, per edit | items, time |
| Fingerprint | server, inside the sweep | one hash per block entity per sweep period | block entities ÷ period | items |
| Cycle tables | server, at compile | at most a few hundred float steps per new key, cached | distinct (kind, recipe, batch, speed) keys | |
| Solve | engine | O(rounds × (nodes + edges)), about 0.2 ms for 500 nodes and 4 rounds (ESTIMATE, 100 ns per rational edge step) | breakpoints and events | elapsed time, items |
| Integration | engine | the cluster's existing per-event cost | events | elapsed time |
| Periodic edges | engine | O(1) per query | | voyages passed |
| Checksum | engine | per sweep, per good | loaded districts | |
| Probes | server | a few extra buffer reads per sweep | disagreeing districts only | |

A district with no events and no breakpoints costs nothing per tick, as today (SCALE.md §7).

### 9.2 Memory

- **Hot** (resident): the regime summary: net rate and pool per (store, good), next breakpoint, flags;
  about 24 bytes per (store, good) plus a header (ESTIMATE), against the 8.6 KB measured per black-box
  district overnight.
- **Cold** (paged, A16): graph, recipes, cycle tables; about 48 bytes per node and 32 per edge, so 500
  nodes and 800 edges is about 60 KB (ESTIMATE). Loaded only to re-solve; the timing wheel's next
  breakpoints drive prefetch on the IO lane (SCALE.md §8). 10,000 districts keep only hot parts resident.

### 9.3 Complexity contracts for the scale bench

New sweeps in `rtsim-cli bench industry`, with the bench's existing tolerances (slope + 0.1, flat
within 4×, PLACEHOLDERS, P24):

| Sweep | Varies | Metric | Declared slope |
|---|---|---|---|
| `glass_nodes` | nodes per district (10 to 10,000) | compile + first solve CPU | 1 (rounds fixed) |
| `glass_items` | items per tick through the same graph (stack sizes, RPM) | CPU per sim day | 0 (flat) |
| `glass_elapsed` | days between reads | CPU per catch-up | 0 (flat) |
| `glass_periodic` | ship voyages or clock periods elapsed | CPU per catch-up | 0 (flat) |
| `glass_regimes` | breakpoints per day | CPU per sim day | 1 |
| `glass_quiet` | replaying districts with nothing happening | CPU per sim day | 0 (flat) |
| `glass_quiet` | same | resident heap | 1, hot part only |
| `glass_oscillation` | adversarial graphs | districts marked oscillating | reported, not a slope |

---

## 10. What cannot be modelled, and the fallback

| Thing | Why | Fallback |
|---|---|---|
| Item order and phase on belts, synchronize modes, beats between machines of different cycles | emergent timing, not rates | rates are exact or bounded (§3.3); the order of stacks is not kept |
| Random routing and chance outputs as realized | RNG | expectation (§3.5, G-B11) |
| Contraptions: drills, harvesters, ploughs, saws on bearings | they change the world; yield depends on blocks | learned Source for cyclic ones (I10); progressive ones freeze the district |
| Mob farms and spawners | entities and spawning | learned Source (§6.2, I9) |
| Crops and trees | random ticks | learned Source, per season (I12) |
| Item entities: fans over dropped items, items falling into crushers, hoppers collecting the ground | entity physics | the fan over a depot and the crusher are modelled; ground collection is a learned edge |
| Redstone beyond threshold switches, gauges and clocks; CC: Tweaked; Drive-By-Wire | arbitrary programs | black-box controller replaying its observed duty cycle (INDUSTRY.md §6 case 8) |
| Destroy reactions far from equilibrium | composition drifts | black box; "mixture not steady" |
| Create Nuclear heat rising, meltdowns, explosions, fires | not steady; destruction | freeze the district; never simulated |
| Unknown addon machines | no model yet | black-box node, residual (§5.1) |
| Players at the factory | they are ports | ports (INDUSTRY.md §3.3) |
| Graphs that oscillate between regimes | bounded work rule | black box for that regime (§4.5) |
| Ships without a stable route | no period | frozen (§8.3) |

---

## 11. Reuse, custom roll, phases

### 11.1 Reuse table

| Need | From | Hook | Status |
|---|---|---|---|
| Kinetic networks | Create 6.0.10 | `TorquePropagator.networks`, `KineticNetwork.sources/members`, `KineticBlockEntity.network/getTheoreticalSpeed/isOverStressed`, `BlockStressValues.getImpact/getCapacity` | src; jar for the constants named |
| Machine formulas | Create 6.0.10 | §2's classes and methods | src and jar |
| Recipes | Minecraft, Create | `RecipeManager`, `AllRecipeTypes.*.find`, `SequencedAssemblyRecipe` | src |
| Configs | Create | `AllConfigs.server()` (`CKinetics`, `CLogistics`, `CFluids`, `CRecipes`, `CTrains`) | src |
| Inventories, events, chunk attachments | NeoForge 21.1 | as INDUSTRY.md §17 | verified there |
| Pools, diffusers, breakpoints, books | rtsim-industry `cluster.rs` | `Pool`, `Body::Box`, `Space`, F9 envelopes | built |
| Learner (fallback, checksum) | rtsim-industry `learn.rs` | `Learner`, windows, slopes | built |
| Reconcile, epochs, wire, journal | rtsim-industry, Java `Reconciler` | I8, F14 to F16, W3 | built |
| Station hooks, mail, cargo deltas | Java `TrainWatch`, `MailWatch` | H17, H18, H30 | built |
| Leg index | rtsim-industry `threat.rs` | `LegIndex.book` | built; standing routes added |
| Voyage phases | rtsim-ships `boundary.rs` | departure, cruise, approach, arrival | built |
| Lazy evaluation, wheel | rtsim-kernel | `LazyModel`, `TimingWheel`, `Diffuser` | built |
| Destroy chemistry | Destroy 0.4.1 | `LegacyMixture.readNBT/reactForTick/isAtEquilibrium/reactInBasin` | javap |
| PowerGrid circuits | PowerGrid 0.6.2 | `ISolver` implementations (validation only), `BatteryBlockEntity` | javap |
| Diesel fuels | Diesel Generators 1.3.15 | `FuelType.PerEngineProperties`, `fuel_type/*.json` | javap |
| Ender channels | Ender Transmission 2.1.1 | `MatterTransmitterNetwork`, `EnergyTransmitterBlockEntity.addPropagationLocations` | javap |
| Ship pose and move | Sable 2.0.5 | `SubLevel.logicalPose`, `PhysicsPipeline.teleport` | javap; unloaded use UNVERIFIED |
| LP upper bound | `microlp` crate (pure Rust, MIT or Apache) or our own | simplex | candidate; version and licence to check (G-B15) |

### 11.2 Custom-roll ledger

No new blocks, items or entities (D3).

| Piece | Where | Size | Notes |
|---|---|---|---|
| Graph extraction for Create's components | Java | L | per kind, sliced, fingerprinted; accessors in PATCHES.md |
| Cycle evaluators | Java | M | one countdown per kind, run on the JVM; cache |
| Calibration and contract GameTests | Java | M | hand-offs at four speeds; model beside the real block entity |
| Graph wire records | Rust + Java | S | five latest-wins kinds, IND_PROBE_SET, IND_CHUNK_TICKING; contract fixtures (H32) |
| Graph store and paging | Rust | M | hot regime summary, cold graph pages |
| Solver | Rust | L | SCC, two passes, water-filling, coupling rounds, rationals, L and U |
| LP upper bound | Rust | M | or S with a crate |
| Regime integration | Rust | L | per-store pools, store spaces, periodic edges, controller bounds, latency |
| Checksum, derating, probes, demotion | Rust + Java | M | |
| Residual node and learned coupling | Rust | M | dependency bits, starvation windows |
| Partial-load cuts | Rust + Java | M | per-chunk ticking, cut edges |
| Template data and loader | Rust + Java | S | datapack JSON |
| Vintage, Metallurgy, Diesel, Enchantment Industry models | data + Java | S each | templates plus a few reads |
| Create Nuclear model | Java + data | M | timers, steady check |
| Destroy evaluator | Java | L | equilibrium stepping under budget |
| PowerGrid model | Rust SDK + Java | L | extraction, MNA solve, validation |
| Station subgraph exchange, lines, standing routes | Rust + Java | M | |
| Sable voyage plugin | Java + Rust | L | pose tracking, vehicle districts, dock snapping (built with the ships work) |
| Bench sweeps | Rust | M | §9.3 |

### 11.3 Phases and exit tests (I26 order)

| Phase | Builds | Exit test |
|---|---|---|
| **G1 Compile and solve, read only** | extraction, cycle tables, calibration suite, graph records, solver with L and U, checksum against the black box; the replay still uses the C19 box | A corpus of 12 GameTest factories: crushing wheels with washing and blasting; brass mixing on burners; pressing sheets; millstone flour; saw shafts; precision mechanisms on a deployer loop; a 3 × 3 crafter grid; arm-fed burners and depots; boilers and steam engines that overstress when water stops; brass tunnels in prefer-nearest and round robin; a packager, chain conveyor and frogport loop; a threshold-switched smelter. For each, loaded for a window: every exact good within the checksum tolerance, every bounded good inside [L, U], the solve's rounds recorded. |
| **G2 Glass-box replay** | regimes on per-store pools, breakpoints, controller bounds, periodic edges, latency, partial-load cuts (I7), learned sources (I9), residual node and learned coupling (I6), derating and demotion | Two identical factories, one chunk-loaded and one simulated, for each corpus factory: the simulated one ends at or below the control and within tolerance. Property tests: conservation, the composition rule with periodic edges, controller bounds never above the control. Bench sweeps of §9.3 within their slopes. |
| **G3 Trains, stations and ships** | compiled station exchange, lines, standing routes; the Sable voyage plugin with the ships work | INDUSTRY.md walkthrough 14.1 with a compiled interface station; a sailbot between two ports in a GameTest (if the teleport GameTest passes), or an engine-level test of periodic edges over 10,000 voyages at flat cost. |
| **G4 Addon models** | Vintage, Metallurgy, Diesel, Enchantment Industry templates; Nuclear, Destroy, PowerGrid models | One reference factory per mod passing G1's checksum and G2's control test; Destroy's vat reaching equilibrium in the evaluator within its budget. |
| **G5 Threats and rebuilding** | later (I26): destroyed nodes recompile the graph; repair schematics (I1) | per INDUSTRY.md phases I-F to I-H |

---

## 12. Proposals for Ben

Each lists the recommended option first. **Ben decided all of them on 2026-10-20** (DECISIONS.md, G-B1
to G-B19): the recommendation everywhere except G-B6, G-B7, G-B8 and G-B17, which he changed (see §4.3,
§6.1, §6.2 and §8).

| # | Question | Recommended | Alternatives |
|---|---|---|---|
| G-B1 | What the solve computes | Create's own behaviour: the greatest fixed point of the back-pressure push network (§3.1), with the LP only as an upper bound | (b) the LP's throughput optimum, which routes better than Create and makes more than reality; (c) measured rates only, as C19 |
| G-B2 | How measurement and model combine | replay at `clamp(measured, L, U)`, never above measured plus tolerance; L for a good the confirming window did not measure | (b) the model's value unclamped; (c) the measurement unclamped, with the model only for structure |
| G-B3 | What triggers a recompile | player edits plus a per-sweep fingerprint of block entities and their configuration; no mixins | (b) mixins on filter and scroll-value changes (exact, more patch surface); (c) a periodic full recompile |
| G-B4 | When a compiled district may replay | after one passing checksum window (`glass.checksumWindowMin`); before that it freezes (I5) | (b) at once at L, confirmed later; (c) only after the black box's full steady test |
| G-B5 | Cycle exactness | run each kind's own countdown on the JVM at compile for exact tick counts; calibrate hand-offs with GameTests | (b) closed-form approximations in Rust; (c) measure every machine in game |
| G-B6 | Partial loads (I7): flow across a live-to-simulated cut | zero both ways, as Create blocks at unloaded chunks (a GameTest confirms) | (b) carry cut flow as pending deltas into the receiving store, which makes more than vanilla would |
| G-B7 | Where a simulated Sable ship appears (C30) | only at docks: snapped with `PhysicsPipeline.teleport` when its port loads, moved to its next dock if its last pose loads mid-voyage; cargo always follows the timetable | (b) interpolated along the route, risking terrain and collisions; (c) cargo moves but the ship stays where it was left |
| G-B8 | Controllers (threshold switches, gauges) | treat as bounds at the restart level: no limit cycles, never above reality | (b) step every on-off cycle, which costs per cycle and grows with elapsed time |
| G-B9 | Periodic flows (ships, clocks, packages, day and night) | exact closed forms with a safe envelope for breakpoints | (b) continuous averages, losing batch timing; (c) one event per period |
| G-B10 | Addon model tiers | template data for addons built on Create's base classes, Rust SDK models for heavy math, Java evaluators at compile for math in the mod's Java | (b) Java models for everything; (c) black box for every addon |
| G-B11 | Chance outputs | exact expectation by integer diffusion, no RNG | (b) dyadic random counts (O17) for realistic variance |
| G-B12 | Kinetic stress | modelled exactly: domains stop when overstressed, including steam engines losing heat or water and fuel engines running dry | (b) assume stress as measured while loaded |
| G-B13 | Unknown machines | one residual node per district, joined at the unknown blocks' edges, with learned coupling (I6) | (b) one node per unknown block, which boundary data cannot tell apart; (c) the whole district falls back to the box |
| G-B14 | Disagreement policy | cap at once, derate the limiting node, probe internal buffers, demote the node kind after `glass.mismatchStrikes` windows and write a report | (b) freeze the district on any disagreement; (c) log only |
| G-B15 | The LP for upper bounds | the `microlp` crate (pure Rust) if its licence and version check out, else our own small simplex | (b) our own simplex from the start; (c) no U, so no calibration above L |
| G-B16 | Train stations | compute the exchange from the compiled station subgraph and the schedule's wait condition; learned exchange (I15) as the fallback | (b) learned exchange only, as built |
| G-B17 | Mob farms (I9) | a learned Source per farm, conditioned on day, night and season, feeding the compiled collection system | (b) one learned rate for the whole farm district |
| G-B18 | Phase plan | G1 read-only compile and checksum, G2 replay, G3 trains, stations and ships, G4 addons, G5 threats later, with the 12-factory corpus as G1's exit | (b) replay from the start with no read-only phase; (c) addons before trains |
| G-B19 | Graph wire format | latest-wins graph records with a count, as sweeps (0x0048 to 0x004C), plus IND_PROBE_SET and IND_CHUNK_TICKING | (b) journaled graph records, larger journal; (c) one opaque blob per compile |
