# Sections: factories cut at links and stores

**Status:** design approved by Ben on 2026-10-22 (all six questions in §12 answered, each the
recommended option). Built: S1 and S2a to S2c (§10.1); S2d onward to come. Every other choice is
listed in §13 and can be undone.

**Decided, and binding here:**
- **SEC-1** (Ben, 2026-10-21, "Cut at links and stores"): districts stop merging across chain conveyors
  and split where goods cross a countable link (packages, frogports, packagers, train stations,
  postboxes). Inside a district, sections split at stores (vaults, tanks, stock chests), and each
  section learns, replays and goes stale on its own. A shared shaft network or a belt straight into the
  next machine cannot be split. The design was written against a graph dump of a large player base and
  comes back to Ben before building.
- **DS-10** (only machines seed; a chain conveyor is a link between districts, counted at the ports on
  either end; a station is an anchor, not a seed), **DS-11** (a detection runs over several ticks under
  `rescanBlockEntitiesPerTick`, commits in one tick), **DS-12** (adopt detects around its own chunks).
- **G2-9** and choices **G2-80 to G2-85** (the glass box's state lives in the page store, one page per
  district, no migration), **G2-16** (a power link sends its districts to the black box), **G2-22**
  (replay parts), **G2-11** (partial-load cut), **G-B6** (Ben: flow across a loaded and unloaded edge is
  modelled, not zero), **GB3** and **GB5** (sample and command records).
- **GA-2** (storage networks and wireless links are one shared store in Rust with ports, G4; districts
  stay small), **I13** (PowerGrid lines couple running states; Ender Transmission is local only),
  **I16** (gauge networks across districts: warn, then fulfil), **I7** (the unloaded part of a partly
  loaded district keeps being simulated), **I5** (stale districts freeze while unloaded).
- **UR-1** (rtsim can be deleted and reinstalled at any time; a reset loses only the sim's own
  knowledge and production owed to unloaded chunks; it never holds the only copy of a player's items),
  **SRV2**, **DR-0**, and the rule of no backward compatibility before release.
- **D1:** performance on a weak PC is the only judge.

**Sources.** Create facts are `javap` on the pack's `create-1.21.1-6.0.10.jar`, with line numbers from
its `LineNumberTable` (a method's first line, or the line of the call named). NeoForge facts are
`javap` on `neoforge-21.1.249-universal.jar`. Vanilla facts are the decompiled 1.21.1 sources in the
NeoForm cache. Addon facts are `javap` on the pack's jars (create_connected 1.3.3, Ender Transmission
2.1.1, PowerGrid 0.6.2). Repository facts name the file and line at `0bc2f02`. Anything not read from a
jar or the code is marked UNVERIFIED.

**Numbers:** every number is a verified fact, arithmetic from one, a labelled TUNABLE, or a labelled
ESTIMATE. Where a number or a fact from the reference base matters, the text says "measured on the
reference base, see the local notes": those notes stay outside the repository, with the dump and the
scripts that made them.

---

## 0. What changes and what stays

| Part | Today | With sections |
|---|---|---|
| What joins two chunks into one district | a face touch of industrial block entities across the border, a plugin link (fans, Ender channels), and **any 8-neighbour member chunk** (`Districts.commit`'s 8-neighbour union loop before slice S6) | a face touch, a plugin link, one inventory spanning the border, rotation across the border that is not a face touch, an arm or ejector reaching across; never bare adjacency (§2.1) |
| Chain conveyors | not a link since DS-10, but chunks still join by adjacency, so a short chain usually sits inside one district | never a join; a chain network whose frogports sit in more than one district is a **package store** owned by no district, with a port at each of its frogports (§2.5) |
| Packages at frogports | not counted (`IndRecords.Port.PACKAGE` is defined and never sent) | on a package store's network, every package leaving or landing is an `IND_PACKAGE` with a new direction (`CHAIN_OUT`, `CHAIN_IN`) booked into the store, never into the district's links (§2.5) |
| A kinetic network shared by districts | each district's compile decides its driver from its own engines only (`CreateCompile.java:413` to `:418`, `:452`); districts disagree | a **shared network** in Rust: every district reports its own part from its own compile, the network's state is decided over all parts, and the districts on it replay in one cluster (§2.6) |
| The unit that learns, replays and goes stale | the district | the **section**: a working line between stores; the district stays the unit of loading, ports, the black box and the checksum (§3) |
| An edit | drops the district's open window and stops its glass replay until a window passes or Java recompiles (`glass/mod.rs:313` to `:332`) | stales the sections whose node box holds the edit and drops the district's open window; the recompile keeps every section whose build did not change (§3.6) |
| A replay that cannot go on | the district goes to the black box (`glass_mode` in `engine.rs`, `dg.fallback.is_some()`) | the sections the failure names freeze, and the replay goes on over the rest from the tick of the failure; the black box takes the district only when no section replays (§3.5) |
| Measurement | one window per district per good | one window per section per good, plus the district's own window as the checksum; flows into stores shared by sections are counted at Create's movers (§4) |
| Chunk loading | the compile, Create's chain conveyors and Create's network recount read through `Level.getBlockEntity`, which loads chunks and pulls an away district into a partial load | the compile reads through `getChunkNow`, and mixins keep Create's chain and network lookups from loading a chunk (§2.8, slice S1) |
| Saved state | `IndustryData` format 2; glass page `IGL1` | `IndustryData` format 3 (package stores) then 4 (join pairs); glass page `IGL2`, `IGL3`, `IGL4` then `IGL5` (sections, the judging units' windows, the station joins, then the counted stores' bases, inside the district's page); package store pages; a loud reset on load (§5, §6) |

---

## 1. What the reference base showed

A copy of a large survival base built with the MIC pack was loaded on the laptop with the stub
engine and a local hook that dumped every district commit and every glass compile. The hook is not
committed; §8.1 makes the same dump an operator command. In general terms, each measured on the
reference base, see the local notes:

1. **Bare adjacency holds big districts together.** With only physical joins, a big district splits
   into a few large pieces and some small ones, and no chunk is left without a seed.
2. **Physical joins alone cut goods edges at multiblock inventories that span a chunk border.** A
   vault or silo whose controller sits in the next chunk is not a face touch of two block entities in
   the walk's sense, so "one inventory spanning chunks joins them" (J3) is required.
3. **One kinetic network can span many districts.** Chain conveyors carry rotation
   (`ChainConveyorBlockEntity.addPropagationLocations` :660, `propagateRotationTo` :667), also through
   chunks that belong to no district, and the districts on such a network can disagree about its
   driver: the bug of §2.6.
4. **After the split, the frogports of one chain network can sit in several districts,** and the chain
   edges between them then cross districts. Package networks across districts are a case to design
   for, not an edge case (§2.5).
5. **Whether kinetic networks join sections decides how big sections get** (Q1). With networks as a
   shared input and the fold of §3.2, sections are working lines and storage systems, and few stores
   are shared by sections. The moves at shared stores ran along face-adjacent, chute and pipe edges,
   which movers at the store make and which can be counted where they act (§4.2), vanilla hoppers
   excepted.
6. **No tool dumps a live world's district graph.** §8.1 adds one.

---

## 2. Level 1: districts cut at links

### 2.1 What joins two chunks

A district is a connected group of member chunks holding at least one seed (DS-10). Two member chunks
are connected only by these joins:

| # | Join | Where it is found | Cost |
|---|---|---|---|
| J1 | **Face touch** of two industrial block entities across the border, horizontal faces (`Districts.scanChunk`, its touch loop), **except** where either side is a `ChainConveyorBlockEntity` or a `StationBlockEntity`; two stores side by side still do not join (DS-10). A station's touch names J6's candidates and nothing else | the walk; the class test is `IndustryPlugin.touchJoins` (`CreatePlugin.touchJoins`) | one class test per touch |
| J2 | **Plugin link** (`IndustryPlugin.links`): a fan's stream (`FanLinks`), an Ender channel; new: a mechanical arm's interaction points (`ArmBlockEntity.inputs`, `outputs`, `ArmInteractionPoint.getPos()`, read through the existing `CreateGlassAccess.armInputs` and `armOutputs`) and a weighted ejector's target (`EjectorBlockEntity.getTargetPosition()` :589) when they lie in another chunk. An arm's lists are empty, never null, until its first tick reads its saved points (`ArmBlockEntity.updateInteractionPoints`, read by `CreateGlassAccess.armPointsPending`, PATCHES.md row 84), so the chunk of an arm that has not ticked keeps its saved pairs (`IndustryPlugin.linksKnown`, choice SC157) | `CreatePlugin.links`: `FanLinks`, then `JoinLinks` | the arm's points, the ejector's one target |
| J3 | **One inventory across chunks:** a store whose canonical position (`canonicalInventory`, `CreatePlugin.canonicalInventory`: an `IMultiBlockEntityContainer`'s controller, a double chest's lower half) lies in another chunk joins the two chunks; an item vault (`ItemVaultBlockEntity`), a fluid tank, a create_connected item silo (`ItemSiloBlockEntity implements IMultiBlockEntityContainer$Inventory`) or fluid vessel | the walk canonicalizes every candidate and records the pair (`Districts.inventoryJoin`), for tagged storage and plugin inventories. `canonicalInventory` returns null when the controller's chunk is not loaded; the saved pair then stands (§2.3) | one pair per such store |
| J4 | **Rotation across the border that is not a face touch:** diagonal cogs (`KineticBlockEntity.addPropagationLocations` :559 adds 4 diagonal positions when a small cog can propagate diagonally, the lambda at its line 565; a large cog is a `SimpleKineticBlockEntity`, whose override :26 to :34 adds all 12 edge diagonals) and any class that overrides `addPropagationLocations` (Ender Transmission's `EnergyTransmitterBlockEntity` :33), confirmed by Create's own rule (`RotationPropagator.isConnected`, public, :403); chain conveyors excluded on both sides (§2.6) | `JoinLinks.rotation` through `CreatePlugin.links`, for kinetic block entities within one block of a border only, and for overriding classes (`StaleKinetics.ownPropagation`, a per-class cache); a target in another chunk only, read through `getChunkNow` | a few neighbour reads per border cog (at most 11 for a large cog at a chunk corner, 7 diagonals and 4 faces), not counted in the DS-11 budget (§7.1) |
| J5 | **Merge links** (`/rtsim district merge`) and **pins** (`split`), unchanged | `IndustryData` | none |
| J6 | **A station's chunk** with no seed of its own joins **exactly one** district: the one holding the inventories its docked interfaces reached last, saved per station (its position to the served chunk) so it outlives a restart, and a chunk with several stations goes by the one at the lowest position that has served; before the first docking, the lowest district id among those 8-adjacent to it or linked by what touches it (DS-10's anchor rule, narrowed to one). `TrainWatch.ports` resolves each inventory its docked interfaces reach by chunk to whichever district holds it; the district holding the most is served (ties to the lowest id), and `Districts.stationServed` saves the pair (choices SC156, SC158). Before S6 it kept only the station's own district's inventories, so it could never learn another district | the commit (`Rescan.stationJoins`); the pair from `TrainWatch.ports` | none |

Bare 8-neighbour adjacency is gone (slice S6): the commit's union runs over the joins the walk recorded
and the saved pairs (§2.3), and the merge links. An incremental detection still starts from the districts
owning a dirty chunk or a chunk next to it, because an edit in one chunk can create a join to the next
(the walk then reaches across it). `Rescan.grown()` adds only what the walk reached: each reached chunk's
own district and pin, the far end of a merge link, and for a station chunk the districts beside it and the
one holding its served chunk (J6's candidates). An edit in one of two factories side by side does not walk
the other.

### 2.2 The cut points, per mod

| Mod | Block (class) | Level 1: districts | Level 2: sections | Where it is counted |
|---|---|---|---|---|
| Create | chain conveyor (`ChainConveyorBlockEntity`) | **cut**: no touch join (J1) and no rotation join (J4); its network is a package store when its frogports sit in more than one district (§2.5), and a shared kinetic network when its rotation reaches another district (§2.6) | not a node | at the frogports |
| Create | frogport (`FrogportBlockEntity extends PackagePortBlockEntity`) | stays in its district by its touches to its packager and inventory | a `MOVER` node that claims its 18-slot package inventory (`CreateCompile.java:323` to `:324`, `:1056` to `:1063`; `GraphBuilder.java:229` to `:236`): no store, no cut. On a chain wholly inside the district it keeps today's `CHAIN` edges to the other frogports (`CreateCompile.frogportEdges` :1849) | on a package store's network: leaving at `PackagePortTarget$ChainConveyorFrogportTarget.export` :114 (the conveyor's `addLoopingPackage` :122 or `addTravellingPackage` :123, on a call that is not a simulation); landing at `FrogportBlockEntity.tick` :155, the `insertItem` at :212 after the catch animation (`startAnimation` :223, started by `ChainConveyorBlockEntity.exportToPort` :361) |
| Create | packager (`PackagerBlockEntity`) | **not a cut** (Q6): it joins its target inventory by touch, and by J3 when the target spans chunks | a mover between its target store and its frogport or postbox | its packages, at the frogport or postbox they go through |
| Create | train station (`StationBlockEntity`) | **never a touch join** (J1); an anchor that joins exactly one district (J6) | not a node; its portable storage interfaces expose the train's storage, and the movers beside them pull and push | `TRAIN_CARGO` at departure: the whole train's change, with no inventory (`TrainWatch.java:366` to `:381`), mapped to sections through `IND_STATION_PORTS` (§4.4) |
| Create | postbox (`PostboxBlockEntity extends PackagePortBlockEntity`) | a member of its district | a `MOVER` node claiming its inventory, like a frogport: no cut | mail at `GlobalStation.runMailTransfer` :167 (`MailWatch`, `IND_PACKAGE` IN and OUT with the postbox's inventory), unchanged |
| Create | item vault, fluid tank (`ItemVaultBlockEntity`, `FluidTankBlockEntity`) | J3 across a border | a `STORE`: a cut, unless it is a boiler (a `BOILER` node joins its engines) | at the movers of each section that uses it, when it is shared (§4.2) |
| Create | chest, barrel (stock tag) | J1 only with a non-store neighbour | a `STORE`: a cut | at the movers; a vanilla hopper uses the `Container` directly and is not counted, so its store makes a judging group (§4.3) |
| Create | belt, funnel, tunnel, chute, pipe, pump, depot, basin, spout, drain, press, mixer, saw, deployer, crafter, millstone, crushing wheel, fan, arm, burner, steam engine | J1 (and J2 for arms, ejectors, fans) | **never a cut**: a belt straight into the next machine is one section (SEC-1) | inside its section's window; at a shared store, funnels, chutes, arms and pipes count their own moves (§4.2) |
| Create | shaft, cog, gearbox, encased chain drive, clutch, gearshift, water wheel | J1, J4 | not nodes: members of a kinetic domain, a shared input (Q1) | not counted |
| Create | factory gauge, stock link, stock ticker, redstone requester (`LogisticallyLinkedBehaviour`) | **no join**: data coupling, I16 (`GaugeNetworks` warns) | a controller in the section it gates | not goods |
| Create | redstone link | **no join**: data coupling | not modelled across districts (§14) | not goods |
| create_connected | item silo, fluid vessel (`ItemSiloBlockEntity`, `FluidVesselBlockEntity`) | J3 | a `STORE`: a cut | at the movers |
| create_connected | inventory bridge, access port (`InventoryBridge`, `InventoryAccessPort`) | J1 | the proxied store is the store; the bridge's nodes join it (UNVERIFIED: the proxy's capability path) | at the movers on the proxy |
| create_connected | kinetic bridge (`KineticBridgeBlockEntity`, `KineticBridgeDestinationBlockEntity extends GeneratingKineticBlockEntity`) | J1 (the two halves are adjacent) | two networks already: the destination is a generator fed by the source's network | not counted |
| Ender Transmission | item, fluid and energy transmitters (`EnergyTransmitterBlockEntity.isCustomConnection` :23, `addPropagationLocations` :33) | J2 (`ChannelLinks`), J4; local only (I13) | the channel's buffer is global per channel and password and has no block position; the repository calls it "not stock, not a port, not sampled" (`ChannelLinks.java:31` to `:32`). It is no store and no cut; the compile models no transmitter, so transmitters fall into residual groups, and the groups at the ends of one channel join (§3.1) | not counted |
| PowerGrid | wires (entities), batteries | no join; `IND_POWER_LINK` couples districts (I13, `PowerLinks`), unchanged | no glass compiler models PowerGrid; its blocks are boundary inventories or residual, cut or joined by §3.1's rules like any other | the link record, unchanged |
| Sophisticated Storage | controller, chests, barrels, connectors | `#mic_rtsim:district_not_seed` | stores; a storage network is GA-2's shared store (G4), not this design | G4 |
| create_power_loader, Numismatics, create_ultimate_factory | station chunk loader; no package block entities; recipes only | nothing | nothing | nothing |

### 2.3 Saved joins and incremental detection

- **Each district saves its join pairs** (two chunk positions, 16 bytes a pair) next to its seed counts,
  as a set (a pair is stored once, smaller chunk first). A join needs both chunks loaded to be seen, so
  an unloaded member keeps the joins its district last saw: an incremental detection replaces a
  district's pairs only between chunks it walked in that detection (`visited`), and keeps the
  rest. A loaded chunk of the district with a saved pair to a member that is not loaded is a root of the
  walk (`Rescan.joinedToAway`), as one touching an unloaded member across its border is, so a chunk held only
  by a link that reaches past the border (an ejector's target, an arm's point) stays while the other end is
  away. Two exceptions keep a pair the walk did not see: an arm that has not ticked (J2; its chunk keeps
  every saved pair it is in, and the walk reaches their other chunks) and a station that has not docked
  since (J6; its served chunk is saved apart, per station, choice SC156). A district keeps the pairs
  whose two chunks it holds, so a pair whose chunk is no longer a member of the district is dropped at
  the commit (SC157). Before S6 the 8-neighbour rule needed no memory, which is why this is new.
- The walk records join pairs where it records touches today. A DS-12 incremental detection records
  only its own walk's joins, so the commit unions the recorded pairs with the saved pairs of the
  districts in question, never with the walk's touches alone (a trap met while making the dump).
- `IndustryData.FORMAT` goes to 4 with join pairs in slice S6 (3 is S4's package stores;
  `IndustryData.java:52`). The load path for an older format is §6's.
- `AdoptGameTests`' rule stays: an adopt gives the same districts a full detection gives. An adopt adds
  seeds, never joins, so it no longer merges two districts through a chunk between them: that chunk, if
  anything joined it, was a seedless member already.

### 2.4 Stations

A station is an anchor, not a seed (DS-10), and never a touch join (J1's exception), so a station
between two factories no longer glues them. Its chunk joins exactly one district (J6): the one its
docked interfaces serve, which is where its cargo moves. Train cargo is attributed to that district,
not to the district at the station's chunk as `TrainWatch` did before S6 (`TrainWatch.java:236` at
arrival, `:347` to `:350` after a restart); with J6 the station's chunk is a member of that district, so
`districtAt` answers it. Inside the district, the cargo goes to sections as §4.4 says.

### 2.5 Package networks across districts

A **chain network** is the conveyors reachable by their `connections`, walked through `getChunkNow`
(§2.8). A chain network wholly inside one district changes nothing: the compile keeps its `CHAIN`
edges between frogports (`CreateCompile.frogportEdges` :1849), and no record is sent. A chain network
whose frogports sit in more than one district is a **package store** (Q4): the packages in transit,
owned by no district, with a port at each of its frogports. This is GA-2's shared store (G4), built
here first.

**What Create does.** The design follows these facts (Create 6.0.10):

- Each conveyor keeps its own routing table (`ChainConveyorRoutingTable`). A frogport puts its address
  filter into its own conveyor's table from its `lazyTick` (`ChainConveyorFrogportTarget.register`
  :157, then `receivePortInfo`) at distance 0, or 1000 for a `*` filter, with a timeout of 20 ticks
  (`PORT_ENTRY_TIMEOUT`). Every conveyor advertises its table to its neighbours about every 80 ticks
  (`shouldAdvertise`, line 49), one hop further, with a timeout of 100 ticks (`ENTRY_TIMEOUT`,
  `RoutingTableEntry.copyForNeighbour`). Only a ticking conveyor refreshes the entries it gives out.
- A package heads for the exit of the least distance among the entries its address matches
  (`getExitFor`, lines 53 to 60), and the first matching frogport it passes takes it
  (`ChainConveyorBlockEntity.tick`, lines 229 and 280). Addresses match as globs in both directions,
  and a blank filter matches only a blank address (`PackageItem.matchAddress`, lines 185 to 192). With
  no matching entry, `getExitFor` returns `BlockPos.ZERO` and the package circles where it is.
- A frogport exports only after a simulated export says its conveyor has room
  (`tryPullingFromOwnAndAdjacentInventories`, lines 296 to 298; the export's simulation, lines 118 to
  119; `canAcceptPackagesFor`, 107 to 113; `canAcceptMorePackages`, 102 to 103): the conveyor's looping
  packages plus its connections that carry travelling packages stay below `chainConveyorCapacity` (20,
  `CLogistics:15`). A conveyor hands a package on only when the next one passes the same test (the
  lookup at `tick` line 296, the test at 297). A package arriving at the end of a connection joins the
  next conveyor's loop with no test (`addLoopingPackage`, lines 248 to 250), so a conveyor can hold a
  few more than 20.
- So when a frogport's chunk stops ticking, its route expires within 20 ticks on its own conveyor and
  within 100 ticks on the rest of the chain. Packages for it then circle where they are, mostly on the
  exporter's conveyor, until a loaded `*` frogport takes them or the exporter's conveyor is full and
  the exporter stops.

**The package store.**

- **Identity.** Java keeps each package store in `IndustryData` (format 3, slice S4a; `PackageStores`): a
  persisted id given at first sight, its frogports (district, inventory, address filter, the position of
  the conveyor it serves) and the hop counts between those conveyors along the walk. The chunks of no
  district the chain passes through (§3.6) wait for a later slice; S4a keeps none. It is sent to Rust as
  a new `IND_PACKAGE_STORE` record when it changes, at its first walk after a restart, and all at once
  when the engine forgot what it was told (the plugin's `announce`, as for districts). A store is found
  where its frogports are: a district's compile walks each chain network its frogports serve, once per
  compile, and a package at a frogport on no known store walks its chain, at most once in 1200 ticks per
  frogport. A frogport in a chunk of no district counts as an owner of its own, so a chain from a
  district to it is a store. A network of more than 200 frogports stays edges, so its record stays under
  64 KiB. A
  re-walk that finds the same frogports keeps the id. When a network splits, the piece holding the
  lowest frogport position keeps the id, the rest get new ids, and what the store owes each frogport
  goes with that frogport; when networks merge, the lowest id stays and the contents add. A walk that
  stops at an unloaded chunk keeps the frogports and hops it last saw beyond it.
- **The compile.** On a package store's network, the compile makes no `CHAIN` edges; each frogport's
  chain side is a port.
- **Records.** Every package that leaves a frogport of a package store onto the chain, and every one
  that lands in one, is an `IND_CHAIN_PACKAGE` (journaled, `IndRecords.ChainPackage`: store, district,
  inventory, direction `CHAIN_OUT` (3) or `CHAIN_IN` (4), the frogport's index in the store, the
  destination, packages, address, op id, contents; choice SC97). One mixin on `FrogportBlockEntity.tick`
  wraps the two calls that move a package between a frogport and its chain (PATCHES.md row 78): the
  export onto the chain, counted when it answers true, and the insert into the frogport's inventory,
  counted when nothing is left over. The engine books it into the store and as a port of the district's
  window. While the district is loaded and not replaying, it also learns a link,
  `LinkKey::Chain` (the inventory, the destination frogport's position, the good): a `CHAIN_OUT`
  toward a frogport that is in a district books it negative, a `CHAIN_IN` that Create moved books it
  positive (choice SC108). A district with chain links runs the black box, as one with mail links
  does (SC107; replaying a chain port in the glass box is a follow-up). A `CHAIN_IN` that an
  IND_WRITE_CHAIN wrote carries the engine's op id and is no link: the replay already drew it (SC113). A
  package between two frogports of one district on a package store's network is that district's
  `CHAIN_OUT` and `CHAIN_IN`, counted once each way, because the compile has no `CHAIN` edge there.
- **Routing.** Java resolves each (exporter frogport, address) to the frogport Create would deliver to
  and sends it as the record's destination (choice SC100: Java has the walk and Create's own
  `matchAddress`): the matching frogport of the least distance, hops along the chain from the
  exporter's conveyor plus 1000 for a `*` filter, as Create's tables count; then the first matching
  frogport on a least-hop way there, which takes it on the way. Create breaks ties by advert timing,
  which neither side can know; Java takes the lowest frogport position. A package no frogport matches has
  no destination (it circles). A `CHAIN_IN`'s destination is the frogport itself. The store keeps one pool per destination frogport and one for packages
  with no route, in goods and in packages, and a `CHAIN_OUT` books into the pool its (exporter,
  address) resolves to. The no-route pool is drawn by no one; at the exporter's reconcile its packages
  are written back into the exporter's inventory, room permitting (Create would have them circling).
- **Room.** Packages that wait for a full or stopped importer back up along their route through
  Create's gates, about 20 on each conveyor, until the exporter's own conveyor is full and it stops.
  So the store's room toward one destination from one exporter is 20 times the conveyors on the route
  (an ESTIMATE of where Create stops the exporter: arrivals overshoot a gate by a few, and routes that
  share conveyors share their room). An exporter's replay books an export only while its destination's
  pool holds less than that room, less the real packages the last census counted toward that
  destination, in whole batches: the batch is the frogport's last `CHAIN_OUT`, else the good's stack
  (choice SC110).
- **The census.** A player can hang packages on a chain or take them off, and none of that is counted.
  While every chunk of a store ticks, Java counts the packages on its conveyors (looping and travelling,
  by the destination their address resolves to from the conveyor they are on) and in its frogports'
  arrival animations, every 200 ticks (choice SC99), and sends the count as a latest-wins
  `IND_STORE_CENSUS`; Rust sets the pools to it, and the difference is the store's residue, shown by `/rtsim district
  sections`. The census runs after its tick's block entities, so it already counts that tick's crossings,
  whose journaled records reach the engine after it: Rust orders them by the records' own game times and
  books into the pools only the crossings of later ticks. A `CHAIN_IN` beyond its pool takes the pool to
  zero, never below.
- **An end that does not replay.** A district on the store that is frozen, or whose replay cluster
  would pass `cluster_districts_max` (its chain links are dropped, `chain_links_dropped`, logged once
  for each store; choice SC112), neither books into the store nor draws from it while away; the store
  keeps what it holds, and the other ends see it as it is. A destination frogport in no district is
  no route: its exporter's `CHAIN_OUT` learns no link.

**Both ends away** (slice S4b). A replay that starts joins the clusters that hold its district's chain
pools, so the exporter and the importer of one store replay as one cluster, up to
`cluster_districts_max` districts (choice SC112). Each (store, destination frogport, good) is one
cluster pool, `PoolKey::Chain`, which starts from what the store owes that frogport (choice SC109). The
exporter's link books its exports into the pool toward the room as its target, and stops there; it
starts again one batch below (SC110). The importer's link draws from the pool into the district's stock;
once it stops on an empty pool, it starts again no earlier than the route's travel time, the hops
to the farthest frogport of the store times `chain_ticks_per_hop` (60; choice SC111, an ESTIMATE for
a chain at 64 RPM, where GLASSBOX.md §1 gives `RPM / 360` blocks per tick). A replay covers only
the district's unloaded inventories, so a district replayed in part whose frogport's chunk is loaded
has no chain link there: packages Create moves at that frogport are an ordinary port, never the
replay's stock, and the engine writes to it as to any loaded frogport. When the cluster stops, each
pool goes back to what the store that lists the frogport now owes it (a split may have moved it);
with none, to its own store, which keeps it for a frogport that comes back; with that store gone
too, it is lost (`chain_pools_lost`).

**Writes** (choice SC113). While a frogport is loaded and its store owes it packages, or a cluster
holds a pool toward it, the engine sends one IND_WRITE_CHAIN for each good, every
`chain_write_ticks` (200) at most, and none while an earlier write there is unanswered: one still
in the engine's outbox always holds the next, and an answer missing ten periods after the engine
handed the write on no longer does (a crash between the command and its result). Whole batches, at most 18 packages (a frogport's slots),
or one part package when no cluster draws on it. Java writes what fits into the frogport's inventory
as real packages addressed to its own filter, keeping one slot free for a package Create is
catching (one that lands in a full frogport drops into the world), where Create moves them on as
packages that came off the chain, and journals one `CHAIN_IN` with the command's op id; the engine
takes it from the cluster's pool first, then from what the store owes (never below zero). Nothing
written is FAILED, which answers the write, and the engine writes again later. A frogport is saved
with its chunk, so the chunk carries the `CHAIN_IN` and the COMMAND_RESULT until the engine
acknowledges them. The engine's Replay takes each op once while it remembers the op; a chunk that
unloaded before the acknowledgement journals them again at its next load, after a world save let
Replay forget it, so the store also marks each frogport's last written record (its game time and
op): a written record no later than the mark is dropped (`chain_writes_again`). The record names its
frogport by position as well as by index, since a store declared again may list its frogports in
another order. A written record of a write the engine still waits for is that write's answer, taken
at the frogport the write named and never judged by the mark: after a crash the world's clock goes
back to its last save, and Java's next writes are recorded before a mark that a record from the lost
stretch set. A write unanswered a hundred periods after it was handed on (into the link's queue,
not yet Java's hands) is forgotten when the engine next looks to write toward its frogport, which
it does only while the frogport is loaded and owed or pooled; at most eight are kept toward a
frogport, so elsewhere up to eight wait uncounted (`chain_writes_unanswered`). A late answer to a
forgotten one is judged by the mark, which may drop it when a crash moved the clock back meanwhile
(accepted: it needs a crash and a write left unanswered for a hundred periods). The marks are
looked up in the record's store and in the store of its dimension that lists the frogport now, so
a frogport rebuilt into another store still turns its old records away. What the store owes
a frogport that is still away is production owed to an unloaded chunk, which UR-1 lets a reset lose
(`chain_owed_lost` counts what a store removed with no heir held).

**One end loaded, the other away** (Q5). Recommended, the G2-11 way:

- **No chunk loads.** Slice S1's guards (§2.8) already keep Create's chain from loading the far chunk:
  a conveyor in a chunk that does not tick is refused at the hand-off as a full one is, and is not
  looked up.
- **Routes kept.** While a far frogport's chunk does not tick, a mixin keeps its route entry on the
  boundary conveyor (the last ticking conveyor on the way to it), with its filter and the distance of
  Create's own entry as it expired (choice SC135: the store's saved walk has no hops from a boundary
  conveyor), through an accessor for the private `ChainConveyorRoutingTable.insert` and the public
  `RoutingTableEntry` record. Create's own adverts carry it back along the near side. When the next
  conveyor ticks again (the boundary moved out) the kept route moves on along the chain to the new
  boundary conveyor. The kept routes are saved with the frontier, which covers a restart: Create's
  tables are not saved (Create 6.0.10's `ChainConveyorRoutingTable` has no write), so they start empty.
- **The hold.** Packages for the far frogport then travel to the boundary conveyor and circle there,
  refused at its hand-off; behind them the near side backs up through Create's own gates. A held package
  stays in the world. When the frontier first holds a package it writes a key into the package's
  vanilla `minecraft:custom_data` component (the store's id, the frontier's epoch, which every
  industry announce changes and so every engine reset, and a serial; choice SC136) and sends an
  `IND_CHAIN_PACKAGE` with the direction `CHAIN_HELD` (5) carrying the key. It is no port: a held
  package was already its exporter's `CHAIN_OUT`. A held key is followed, never stripped: Java tells
  it again when it moves to another store or destination, and once in each session. It leaves the
  hold where a frogport takes the package (its `CHAIN_IN` carries the key), where Create drops it or
  a player takes it (`CHAIN_RELEASED`, 6), and at a census, which releases every key its walk did not
  find (choice SC137). Once in each session Java lists the keys it holds (`IND_HELD_LIST`) and the
  engine strikes any other key of that epoch, and each boundary conveyor's first tick in the session
  releases the packages Java last held there that it no longer carries (choice SC140: a crash can leave
  either side holding a key no package has). A component type rtsim registered may not decode once rtsim is removed
  (UNVERIFIED for 1.21.1), so the vanilla one is used either way.
- **The draw** (slice S4c2, choice SC141). The far district's replay draws held packages as G2-11's
  simulated side draws a live store's stock. While a replay covers the far frogport, the store's write
  wake asks Java to draw the packages held toward it that carry a good some cluster's pool toward that
  frogport has (a command, `IND_HELD_DRAW` with the action Draw: store id, epoch and serials), each key
  once a session. Java marks each key drawn in the frontier's saved state and answers `CHAIN_DRAWN`
  (7, key only; none while the far frogport ticks); only at that record does the engine move the
  package to the store's drawn ones and its goods into the replay: the pool a cluster holds toward the
  frogport, else what the store owes it.
- **Paying back** (choices SC142 and SC143). A drawn package is never handed on: the frontier refuses
  its every hand-off, even after the far chunk ticks again. Once the replay that drew it has reached
  the world (its district replays no more and has no reconcile pending, and what the replay did not
  use is written in), the engine asks Java to remove it (Remove): Java takes it off the chain and
  answers `CHAIN_RELEASED`, whose journal file waits for the chunk (R21). If the boundary conveyor's
  chunk is not loaded then, the removal waits for that conveyor's first tick once it loads. A drawn
  package whose district is gone is given back (Return): Java clears the mark and tells the key again
  where it is held next. The session's `IND_HELD_LIST` names the drawn keys last, and the engine puts
  the marks as Java has them. A key that names another engine epoch, or one Java does not hold, is
  stripped and its package released, so after a reset nothing is removed: a duplicate at worst, never
  a loss (UR-1; with rtsim removed a drawn package is an ordinary package again, and Create routes it
  on). Nothing is removed where a package lands: a drawn package that reaches a frogport another way
  (a frogport on the boundary conveyor takes it, or a player carried it) goes back into the replay's
  pool first and lands as a normal `CHAIN_IN`.
- **The other way.** Packages the far district's replay sends to a loaded frogport are written into its
  inventory as real packages, room permitting, as in the both-away case, one `CHAIN_IN` each.
- **The limit.** Drawn packages wait for the far reconcile, as G2-11's owed draws wait for theirs, so a
  long absence moves at most the near side's room toward the far district: about 20 packages on each
  conveyor between the exporter and the boundary, plus the exporter's 18 slots. Past that the near
  factory backs up, as Create's does in front of a full importer.

### 2.6 Kinetic networks across districts

Chain conveyors carry rotation, so cutting districts at chains cuts kinetic networks between districts.
Inside a district nothing changes here (Q1 is about sections).

- **The driver bug (before slice S5-1).** The compile built `enginesByNet` from the district's own
  engines and asked `singleBoiler` about them, which returned null for engines on several tanks. Two
  districts on one network saw different drivers, or none, and a district with no engine of its own
  took the network as one fixed level, which kept running when the boiler stopped. Since S5-1 a
  domain lists every source on its network with its rows (choices SC144 and SC145): each tank drives
  its own engine shafts, and the rules below hold within a district. Since S5-2 a network that
  crosses districts is shared (below).
- **What Create keeps.** A network's `sources` and `members` keep a block entity whose chunk unloaded,
  as it last was: `SmartBlockEntity.setRemoved` skips `remove()` on a chunk unload (lines 135 to 137).
  After a restart, the saved network's `unloadedCapacity` and `unloadedStress` stand for the members
  not loaded yet (`KineticNetwork.initFromTE`, lines 28 to 34; `addSilently` takes off what loads). A
  recount (`calculateCapacity`, `calculateStress`) looks every listed block entity up through
  `Level.getBlockEntity` (lines 139 and 155) and drops those no longer there; that lookup loads their
  chunks, and S1's guard skips the ones whose chunk is not loaded, leaving them as Create last saw them
  (§2.8). So the loaded half of a network keeps an away source's last capacity.
- **The shared network** (Q2; slice S5-2, choice SC153). A network whose members lie in chunks of more
  than one district, or of no district, is a shared network in Rust, owned by no district. Each
  district's compile reports **its own part**, a SHARED domain (IND_GRAPH_DOMAIN v3,
  docs/industry-boundary.md §9): every source Create lists on the network, by position, each with its
  ratio in the part's frame (its own sources as S5-1 has them, driven by one of its boilers or fixed; a
  fixed source in chunks of no district at its one row; every other source foreign, with no rows); the
  members in its own chunks with their stress impact; the members in chunks of no district as a tail
  (chain conveyors on the way); and its peers, the other districts whose chunks hold a member or a
  source of the network or a chunk it reaches. Members in another district's chunks are that
  district's to list. The speed rows add each foreign source's possible speeds (an engine's shaft its
  16 to 64 RPM, any other its speed now). A crossing network Java cannot share stays bounded, as S1 has
  it: sources that disagree in sign, a driven source with an unknown ratio, a split tank, more than 32
  rows, or no source with a known ratio. Rust keeps every part as its district last compiled it, so an
  unloaded district's part is known.
- **The registry** (`glass/net.rs`, derived from the compiled graphs, never saved). Parts of one
  dimension that list a source at the same position are one network (a union-find; Create's network
  id is a hint only, it changes when another source takes over). A network is coupled when it has at
  most `industry.clusterDistrictsMax` parts, one per district, a part for every peer a part names, an
  anchor (the lowest source position every part lists with a known ratio: the frame every part's
  speeds go through), every source tabulated by some part, and in every part a speed row for each
  speed an owner's source gives. Otherwise (incomplete, or past the cap) each of its parts runs
  decoupled: at its compile row, its members bounded (why `SHARED_NETWORK`), S1's rule at run time. A
  source's owner is the part with its driven rows, else the lowest district tabulating it; the tail
  counts in the lowest district's part.
- **The network's state** is its owners' source rows. As in Create, the fastest source sets the speed
  (a steam engine runs at 16, 32, 48 or 64 RPM by its boiler's efficiency,
  `PoweredShaftBlockEntity.getGeneratedSpeed` :110 and `getSpeedModifier` :118; the efficiency comes
  from heat, boiler size, water supply and engine count, `BoilerData.getEngineEfficiency` :179), and
  every source adds its capacity at its own speed, a slower one included
  (`GeneratingKineticBlockEntity.applyNewSpeed`, lines 142 to 163; `updateGeneratedRotation`, lines 94
  to 97). Each part's solve takes what the rest gives it (`NetInput`: the fastest speed each way other
  owners' sources give in its frame, their capacity, and the other parts' stress per RPM; in the
  regime's key) and runs while the whole capacity carries the whole stress at that speed, stopping
  otherwise. No district is picked as "the source". A boiler with water and no heat (its burner out)
  runs its engines on the passive row, as Create does (choice SC154).
- **Replay** (choices SC151 to SC153). Each district replays in its own cluster; the network couples
  them. A replaying district publishes its sources' rows: a driven source the row of the part holding
  its boiler (the least row over a running cycle, SC151), none while that boiler is gated, frozen, or
  its district does not replay on glass (SC152); a fixed source its one row. A loaded district's rows
  are its check solve's. When an owner's rows change at a tick, each replaying reader's cluster settles
  to that tick and its parts on the network re-run on the new input, for up to 8 rounds a tick;
  after that a reader whose own rows an input of round 8 or later changed falls back as oscillating
  when its input changes again, and a reader whose rows do not move still takes its input. G2-16's rule (a power link sends its districts to
  the black box: the `dist.links` arm of `glass_mode` in `engine.rs`, and its doc) does not apply: both sides model the network. A black-box district on a shared network runs its box
  as today, ungated by the network, and gives the network no driven rows (§14).
- **Loaded districts.** A loaded district's network is Create's. Its check solves read the other
  parts' rows from their last solves (the live view), so its windows are judged on the whole network.
  The other way is not modelled: a loaded half runs on Create's last view of an away source, whatever
  that source's replay does (§14).
- **Tested.** In slice S5-1: the loaded half keeps its speed, is not overstressed and keeps the away
  source's capacity when the source's chunk unloads, with S1's guard on, for 600 ticks and through a
  recount (S1's `aNetworkRecountKeepsAnUnloadedSource`, extended; choice SC148). In S5-2:
  `aBoilerOnAChainDrivesTheNextDistrict` (§9.1) and the shared network's engine tests (§9.3).

### 2.7 Data coupling across districts

Factory gauges, stock links and redstone links never join districts. Gauge networks across districts are
I16's (warn, then fulfil); `GaugeNetworks` already warns. A redstone link from one district's switch to
another district's clutch is out of scope here (§14).

### 2.8 Reads that load chunks

`Level.getBlockEntity` loads a chunk on the server thread (decompiled `Level.java:707` to `:713`). Each
load runs `Districts.onChunkLoad` (`Districts.java:404`) and `Reconciler.chunkLoaded`
(`Reconciler.java:122`, which sends `IND_CHUNK_LOADED`), and the engine takes the chunk out of the
replay (`engine.rs:873` to `:878`, I7) until it unloads again. Cut at chains and shafts, these loads
cross districts: an away district on a chain or a shared network would be partly loaded at every
advert. Slice S1 removes them, before anything splits:

- **The compile.** Every block entity read reachable from a district's nodes goes through `getChunkNow`
  and stops at a chunk that is not loaded. Two of `CreateCompile`'s neighbour reads (`getBlockEntity`,
  `getCapability`, `BlockEntityBehaviour.get`, each of which loads) are guarded today (`:1939`,
  `:2424`); the others are not: `:359`, `:426` (the network root), `:435`, `:440`, `:813`,
  `:1268`, `:1413`, `:1440`, `:1498`, `:1551`, `:1555`, `:1570`, `:1638` to `:1660`, `:1690`, `:1732`,
  `:1739`, `:1749`, `:1778`, `:1859` (a frogport's face neighbours), `:1877` (`chainNetwork` walking
  conveyors), `:1990` and `:2135`. A network's listed members are filtered the same way.
- **Create's chain conveyors** (mixins, in PATCHES.md with contract tests). `ChainConveyorBlockEntity.tick`
  looks connected conveyors up at line 161 (adverts), 175 (a reversal moves packages), 248 (an arriving
  package joins the far loop) and 296 (the hand-off, tested at 297). A conveyor in a chunk that does not
  tick (`Level.shouldTickBlocksAt`, the test vanilla's block entity loop uses, `Level.java:508`) is not
  looked up. An advert to it is skipped: `advertiseTo` (lines 64 to 70) writes only into the far table,
  which does not tick, so no routing changes. A hand-off to it is refused as to a full conveyor:
  returning null at 296 would skip the test at 297 and let the package travel, so the mixin refuses the
  hand-off itself. An arriving package stays at the end of its connection, as Create keeps one whose
  target is not a conveyor. On a reversal, Create moves each package travelling toward a connected
  conveyor into that conveyor's list, mirrored, so that it travels back (line 175); with the far
  conveyor not looked up, the package stays in this conveyor's list, goes on to the end of the
  connection and waits there, and joins the far loop once the far chunk ticks: it keeps going where it
  was sent (`aPackageIntoAChunkThatDoesNotTick`). `ChainConveyorFrogportTarget.canAcceptPackagesFor`
  (:110) answers false for a conveyor whose chunk does not tick, and a frogport whose conveyor reversed
  does not move itself to a far end that does not tick (PATCHES.md row 52).
- **Create's network recount.** `KineticNetwork.calculateCapacity` and `calculateStress` (the lookups at
  lines 139 and 155) skip a listed block entity whose chunk is not loaded and keep it as Create last saw
  it. A contract test compares the answer with Create's own with that chunk loaded.
- **Rotation** (found while building S1). `RotationPropagator` follows a chain conveyor's connections
  and a large cog's diagonals (`addPropagationLocations`) with no loaded test, so a speed change, an
  added source or a removed one on a chain into an away district loaded the far conveyor's chunk. It
  stops at a chunk that is not loaded now (SC36, PATCHES.md row 55), and the far part keeps its last
  speed. When that chunk loads, Create's attach breaks a block whose speed no longer matches its
  neighbour's (it already does this to a shaft across a chunk border whose source changed meanwhile);
  the stale re-sync gives the part its source's speed instead (SC37, row 56). A chunk that dropped below
  FULL and stayed in memory comes back with no load and no attach; it is re-synced at its promotion
  instead (SC45, row 58), with any network update held back while it was away (row 54). A chunk that
  loads FULL and does not tick (level 33) attaches nothing either: it is noted at its load and re-synced
  at its first FULL report, and every block entity the re-sync lists again before its first tick, in that
  chunk or a FULL one around it, first joins its network as its first tick would (SC73).
- **The ring around a ticking chunk** (found in the GameTests). A chunk that block-ticks (level 32 or
  less) keeps its eight neighbours FULL (33) and the ring past them at 34: in memory, not FULL, so
  `getChunkNow` answers null, `Level.isLoaded` false, and its block entities do not exist (a chunk read
  from disk at 34 has none until it is promoted). A district two chunks from a ticking one is therefore
  away, and every guard above keys on FULL or ticking, never on the holder. A chunk that drops from FULL
  to 34 or below (up to 44) fires no `ChunkEvent.Unload`, and one promoted back fires no `Load` and
  attaches nothing (its block entities are the same objects): the index counts it loaded throughout,
  nothing ticks it meanwhile, as vanilla leaves it, and the stale re-sync runs at its promotion (SC45).
- **The test.** An away district on a package store, and one on a shared network, each stay away for
  200 ticks (past two adverts) with no `IND_CHUNK_LOADED`, and no chunk of theirs becomes FULL
  (`SectionsChunkLoadGameTests`).

---

## 3. Level 2: sections cut at stores

### 3.1 The cut

Sections are computed in Rust from the district's compile (`glass::graph::Graph`), at every accepted
compile and on restore.

- **Union** along every goods edge (adjacency, belt, chute, pipe, air zone, arm reach, drop, and the
  `CHAIN` and `LINE` edges inside the district), every heat edge, every control and observe edge, and
  between the nodes of one inventory (a machine's input and output buffers), **except** where either
  end is a `STORE` node.
- **Cut only at `STORE` nodes** (a store joins nothing). A `CHAIN` edge joins two frogports of one
  district on a chain wholly inside it. A `LINE` edge is a depot's buffer, press and fan zone chain
  (`CreateCompile.java:1345` to `:1348`), not a train line, and joins. Frogports, postboxes and
  packagers are `MOVER` nodes (§2.2) and join.
- **Kinetic domains do not join** (Q1, recommended): a domain shared by sections is an input to each.
  A boiler is a `BOILER` node, not a store, so it joins its engines, its heaters and its water supply.
- **Residual groups.** Today one `BLACKBOX` residual node holds every unclaimed block entity with a
  capability in the district, at the first watch position or `BlockPos.ZERO` (`GraphBuilder.java:30`,
  `:264` to `:271`), so it would glue every line holding an unknown block. Instead the compile makes one
  residual node per face-connected group of unclaimed block entities (a Java change, §10.3), and the
  groups at the ends of one Ender channel (`ChannelLinks.java:26` to `:38`, which treats a channel's
  endpoints as one) are one group. A group joins the non-store nodes it touches; stores stay cuts.
- **Virtual nodes** (the void sink) have no position: they are in no node box (§3.6) and enter a
  fingerprint by kind only (§3.3). A residual group's box and fingerprint use the positions it holds.

### 3.2 Folding

The raw cut makes many small pieces: lone funnels, a belt between two vaults, a hopper chain. They are
folded so each section is a working line:

1. A **working** section holds a `MACHINE`, `HEATER`, `BOILER`, `SINK` or `SOURCE`, or a residual
   group that touches a store (an unknown machine between stores, kept apart so that its window fails
   alone).
2. A **transfer** section (movers, junctions, buffers only) folds into the nearest working section it
   reaches through one store, downstream first, then upstream; ties go to the working piece (the
   section's seed, not what has folded into it) with the lowest node position, then to the one whose
   nodes' records hash lowest (never Java's node order).
3. Transfer sections that reach no working section merge with each other across the stores they share:
   a storage system (vaults, funnels and belts sorting goods between them) is one section.
4. An **idle** piece (no machine and no store, or a residual group that touches nothing) keeps no state;
   its nodes belong to no section and carry no flow in the solve.
5. If more than `glass.sectionsMax` (TUNABLE, 64) sections remain, the two smallest sections (by
   nodes) that share a store merge, else the two smallest that share a domain, else the two smallest
   in the district, ties by the lowest node position, then by the nodes' records, until it holds. The same merge serves the row
   caps of §7.2. A merged section starts learning; its parts' windows are dropped.

This leaves sections the size of working lines and storage systems, and few shared stores, measured on
the reference base, see the local notes (measured with one residual node per district, before the
residual groups above).

### 3.3 Section state

| Field | Per | Notes |
|---|---|---|
| `section_of` | node | `u16`, `NONE` for idle nodes; recomputed, never saved alone |
| `fingerprint` | section | a canonical hash of the section's build (below) |
| window start | section | the sweep its open window began at; window length and sweeps are the district's |
| open window rows | (section, good) | `Acc` and `Cum` as today (`check.rs:47`, `:66`), in one sorted `Vec` keyed by (section, good), not a map per section; per-store rows keyed by inventory index as today (`check.rs:97`) and remapped by store position at a recompile |
| `passed`, `strikes`, `edited`, `fallback` | section | today's `Compiled.passed`, `strikes`, `edited` and `DistrictGlass.fallback`, moved per section |
| least capacity run at | (section, domain) | the least capacity (summed over the network's sources at its state, §2.6) at which the section's passed window saw the domain running, for §3.6's domain check |
| node box | section | the bounding box of its nodes' positions, grown by one block; recomputed, never saved |

**The fingerprint** is FNV-1a 64 (`rtsim_kernel::rng::fnv1a64`, `rng.rs:54`; the solve's regime key
computes the same hash inline at `solve.rs:106`) over a canonical encoding sorted by position:

- every node of the section with its kind, position and every build field (G2-81's build: every
  parameter except the state slots, `state_slots` in `glass/mod.rs`), and its recipe contents;
- every edge with an end in the section, with its end positions and every build field (kind, order,
  flags, capacity, latency, param, filter);
- the build of every store it touches;
- for each domain it touches, its **own** nodes' ratio times the level speed, not the whole network's
  members, so a machine added elsewhere on the network leaves it alone.

A level is the node's own speed there (its ratio times the level's speed): which member Create takes as
the network's root depends on the chunks loaded, and a new root scales every ratio and every level's
speed but not their products. State slots (a heater's burn left and heat level, a boiler's active heat
and water supply, a controller's output and level, a domain's live level, stress and capacity) are not
part of it. A network's load on the section is §3.6's domain check, not the fingerprint.

### 3.4 Learning and verdicts

- Every section has its own window rows and verdicts (§4.1). A section **passes** when its window
  passes by today's rules (G-B4, the check of `check.rs`), applied to its rows.
- **Liveness per section.** Today a sweep in which any member chunk does not block-tick discards the
  district's window (`NOT_LIVE`, `report.rs:67` to `:68`). A section's window is discarded only when a
  chunk holding one of its nodes or its shared stores did not block-tick, so a section learns while
  another part of its district is out of simulation distance. The district window keeps today's rule.
- The district's own window stays: it is the checksum over everything (stores in no section, the
  residue at shared stores, the ports no section takes, the black box), and today's verdict for districts
  with one section.
- **Which verdict wins.** A section replays only while its own last window passed **and** the
  district's last concluded window did not fail on any good the section touches. A district window that
  fails stops every section on its failing goods; a section that fails stops itself whatever the
  district window says.
- Sections in a **judging group** (§4.3) pass or fail together.
- The black-box learner (C19) stays per district: it is the fallback and the checksum on the model.

### 3.5 Replay

- A section **replays** while the verdict of §3.4 allows, it is not stale, and it is not frozen.
- The replay is built over the replaying sections only: the other sections' nodes are gated off
  (`Regime.gate_off`, `solve.rs:84`, as controllers gate nodes today), so their stores become pools
  that only replaying neighbours touch. A gated-off node keeps its stress in its domain: Create's stress
  does not depend on work.
- **Parts are solve units, not freeze units.** G2-22's parts (`partition`, `replay.rs:568`, its doc at
  `:560` to `:567`) join along every edge, every domain with its boiler, and every store with a goods
  edge into it, and a G2-11 cut or a residual keeps the district one part. So every section on a shared
  kinetic network replays in one part, and so does every section of a district with a residual or a
  partial load.
- **A fallback freezes the sections it names, and the replay goes on over the rest** (SEC-1: each
  section replays on its own). A replay falls back when the regime has no fixed point, oscillates, sticks
  at a breakpoint or meets a stop it does not model (`Fallback`, `replay.rs:68`). Today a fallback ends
  the district's glass replay and sends the district to the black box (`glass_mode` in `engine.rs`,
  `dg.fallback.is_some()`; the module doc, `replay.rs:30` to `:32`). With sections, the replay counts,
  per section, the watches that fired, the processes that blocked and the flags that did not settle
  since the last outside event (new). The sections those name freeze (their nodes gated off), and the
  replay is rebuilt over the remaining sections **at the tick of the fallback**, keeping every booking
  made so far: the cluster keeps no snapshot to go back to, and `c.replace_proc` (`replay.rs:1017`)
  replaces a process from a tick on. Each section can freeze this way at most once per replay. A
  fallback that names no section, or every replaying section of the part, ends the part's glass replay
  as today.
- **Frozen sections or the black box** (SC23, a change of behaviour). A district with frozen sections
  goes on replaying the rest in glass, and goes to the black box only when no section replays. A frozen
  section makes nothing while away, where today's black box would run the district's learned rates; the
  glass is the better model where it works, and the black box's rates are the whole district's, not
  one line's.
- A frozen section holding a network's boiler stops that domain for every section on it, and drops
  the boiler's sources from a shared network (choice SC152): the other districts' parts run on what
  the network's other sources give, if anything (conservative: nothing runs on an engine the sim does
  not trust).
- **Partly loaded districts** (I7, G2-11). Sections whose nodes all lie in ticking chunks are live and
  learn. The others replay if the verdict allows; G2-11's cut runs between live nodes and the simulated
  nodes of replaying sections, as it runs between live and simulated nodes today. A section that may
  not replay has its simulated nodes gated off.
- A district with no replaying section goes to the black box when fallbacks did it, as today, and
  freezes when staleness did it (I5).

### 3.6 Staleness

- **By node box only.** `IND_EDIT` (`IndRecords.java:71`) already carries the position (`Obs::Edit`,
  `obs.rs:467`). An edit stales the sections whose node box (§3.3) holds the position, and a judging
  group as a whole when any section of it goes stale. It also drops the district's open window, as
  today: that window covers every section. A command edit (aux 4) stales the whole district. No kinetic
  network id is sent: belts, mixers and fans are kinetic block entities (`BeltBlockEntity extends
  KineticBlockEntity`, `MechanicalMixerBlockEntity extends BasinOperatingBlockEntity`), so staling by
  network would stale every line on a shared shaft.
- **Edits between districts.** Java sends `IND_EDIT` only for member chunks, but a package store or a
  shared network can pass through chunks of no district. Java keeps those chunks with the store and the
  network (from the walk and the compiles), and an edit to a chain conveyor or a kinetic block in one
  is sent as an `IND_EDIT` to every district on that object: it lies in no node box, so it stales no
  section, and it asks those districts to recompile, which updates the store or the network's part.
  S2d sends it for the kinetic networks, at any kinetic block entity a player places or breaks in such a
  chunk (a plugin's network part; a chest or a sign there edits nothing) (SC77); package stores follow in
  S4.
- Stale sections stop replaying at once (I5, I7). The others keep their windows and their replay.
- **Other discards** (`report.rs`, the `discard` reasons). `REVISION` and `SESSION` discard every
  section's open window and the district's: the inventory indices, or the session the window counts
  in, are gone. `RECOMPILE` discards only the sections the recompile changed (below). `NOT_LIVE` is
  per section (§3.4). A station's `Discard.OTHER` (`TrainWatch.java`: a cargo change no one measured)
  carries the station (`IND_DISCARD` offset 32, SC76) and discards the units holding the inventories the
  station's last `IND_STATION_PORTS` named, every unit when it named none, and the district window; it
  counts as `STATION`. The other `IND_DISCARD` reasons stay the learner's.
- **The recompile** (G-B3). The new compile's sections are matched to the held ones by fingerprint. A
  section whose fingerprint is unchanged keeps its passed window, strikes, replay standing and open
  window rows (remapped to the new node indices and store positions). A changed or new section starts
  learning; only those restart their open window, so a player who keeps building elsewhere does not
  hold back the lines he left alone.
- **The domain check.** A kept section on a domain stays passed while the domain's stress (on a shared
  network, summed over every part, §2.6) is no more than the least capacity its passed window ran at
  (§3.3). When it is more, the network would have stopped in a state the window ran through, and the
  section goes stale. An edit in another district on a shared network changes that district's part
  only; Rust runs the domain check for every district on the network at once, with no recompile of
  theirs.
- A block placed outside every box stales nothing at once; the recompile finds the new or changed
  section, and the domain check catches its load on a network.

### 3.7 What stays per district

Loading and presence, ports and their journal, reconcile, threats, the black box, the district window,
the compile and its regime cache, the replay's cluster process set, and G2-81's compile numbering.

---

## 4. Accounting: the flows across a cut

### 4.1 Per section, per good

For a section `s` and a good `g`: its interior stores `I(s)` (every automation edge is to `s`), the
shared stores `H(s)` (edges to `s` and to another section), and `ports(s,g)`, the ports of §4.4 that
belong to `s`.

- **Measured:** `Y(s,g) = Σ_{i∈I(s)} ΔS(i,g) + Σ_{h∈H(s)} (in(h,s,g) - out(h,s,g)) - ports(s,g)`, where
  `in` and `out` are what `s`'s own movers put into and took from `h`, counted (§4.2).
- **Predicted:** `P(s,g) = Σ_{i∈I(s)} pred(i,g) + Σ_{h∈H(s)} (Σ_{e: s→h} L(e,g) - Σ_{e: h→s} L(e,g))`,
  from the solve's per-store predictions (`Solution.per_store`, `solve.rs:167`) and its exact per-edge
  goods flows (`Solution.edge_goods`, `:178`). Bounds and variance follow by the same sums of `Cum`.
  Sections cut only at stores, so no goods edge runs between two sections.
- **The identity, tested.** A shared store's change is what the sections' movers moved, plus the
  ports into it, plus a **residue** `r(h,g)`: moves no counted mover and no port made (another mod's
  mover, a block broken into it). Summing over sections gives the district:
  `Y(d,g) = Σ_s Y(s,g) + Σ_{stores in no section} (ΔS - ports) + Σ_h r(h,g) - ports_d(g)`, where
  `ports_d` are the ports no section takes (a player's toss or pickup, cargo that maps to no section).
  The ports into shared stores cancel. On the predicted side every flow the model holds at a counted
  store is at a counted mover, so its predicted residue is zero by construction: a measured residue is
  a move the model lacks, and it shows in the district's window, as it does today, and in the judging
  group it makes (§4.3).
- A section is never judged on goods it does not touch. When the two windows disagree, §3.4's rule
  decides.

### 4.2 Counting at shared stores (Q3)

Recommended: **count at the movers**.

- Rust tells Java which movers to count at which stores (`IND_COUNT_SET` 0x8408, a command
  without an op id, as `IND_PROBE_SET` 0x8406 is, choice GB5): the (mover, store) node pairs of a
  compile, sent when the district's units change and the set differs from the one Java holds (SC89).
  Java keys each pair's counters by the mover's and the store's positions, so a pair a later set
  names again keeps them (a compile or a revision change does not clear them), and answers in each
  sweep, before the sweep's END, with cumulative amounts per (mover, store, good) in and out since the
  pair's `since` (`IND_COUNT_SAMPLE` 0x004F, not counted in the sweep's records, as `IND_PROBE_SAMPLE`
  0x004D is, choice GB3), in the numbers of the latest compile whose END it sent (SC90). A pair's counts
  are taken at the moment the sampler reads the store's inventory in that sweep, so a count and the stock
  it explains belong to one instant, and a store the sweep did not read sends nothing (SC88). A set is
  ignored when its compile is not Java's latest complete one (the engine's outbox sends it again,
  SC89); a district on a moving frame (a Sable contraption) has no counted store, so the engine sends
  none and its shared stores keep their S2 judging group (SC122); a set that names a pair keeps its counters, one that leaves it out drops
  them, and an empty set drops the district's. The mover's position names its node, so the unit is
  known with no face mapping; movers at one block (a packager's two nodes) share its counts, each
  direction taken once (SC94).
- The count points are Create's own movers, where they move real goods. Each wraps the one call that
  touches the inventory (MixinExtras `@WrapOperation`, PATCHES.md rows 64 to 76), counts only a real
  call (not `simulate`, a server level) and only when the mover's block has a counted pair, so a move
  is counted once; the inventory is the target's canonical inventory (`Districts.canonicalInventory`):
  - `InvManipulationBehaviour.extract` and `insert` (rows 64, 65): the andesite and brass funnels, the
    funnel under a belt, the mechanical crafter's output and the smart observer's extraction; the
    mover is the behaviour's block entity. An arm's insert at a funnel point goes through the funnel's
    behaviour, and is counted once there under the arm (row 81, SC125): the compile's edge is the arm's;
  - the chute's and the smart chute's transfers (rows 66 to 68): `handleInput`, the draw from the
    inventory above (`startLocation` 1) or below (0), `handleDownwardOutput` and `handleUpwardOutput`;
  - the arm's `ArmInteractionPoint.insert` and `extract` (rows 69, 70), the base class's calls: the
    special points (funnel, belt, depot, composter) override them and do not reach an inventory
    through them;
  - the basin's spout push, the real insert in `BasinBlockEntity.tryClearingSpoutputOverflow` (row 71);
    `acceptOutputsInner` only fills the spout buffer and moves nothing. A basin's spout turns sideways
    only for a belt, a funnel or a block with a `DirectBeltInputBehaviour` (`BasinBlock.canOutputTo`),
    and none of those is a store, but a funnel on a vault takes the pour into the vault behind it: the
    funnel offers no item handler, so the basin inserts into the funnel's own `InvManipulationBehaviour`
    inventory and skips the behaviour's count point; the hook counts that under the funnel's position and
    the store behind it, the pair the engine names from the funnel's adjacency edge (SC82, SC120);
  - the frogport's push into and pull from the inventory below it (rows 72, 73), a package counting as
    its contents;
  - the packager's extraction from its target (`PackagerBlockEntity.attemptToSend`, row 74) and its
    unpacking into it (`DefaultUnpackingHandler.unpack`, row 75); the special unpacking handlers
    (basin, crafter, a block that voids) and a mod's own are not counted;
  - a repackager's draws out of its target (`RepackagerBlockEntity.attemptToRepackage`) and its unwrapped
    box into it (`unwrapBox`), which override the packager's two calls without `super` (row 76, SC121);
  - fluids at `FluidNetwork.tick` :66, where a pipe network fills and drains its endpoints
    (UNVERIFIED: which position names the mover, the pump or the pipe end at the store). Not in S3:
    a fluid store is never counted, so it makes a judging group (SC79).
- Not counted in these slices: vanilla hoppers and droppers (they use the `Container` directly), the
  drill's cobblestone shortcut into a hopper below (`DrillBlockEntity`, lines 76 to 79), and other mods'
  movers. A shared store that one of them reaches makes a judging group (§4.3).
- **A unit's row at a counted store.** A unit's measured series is its interior stock, less the
  ports at its inventories, plus what its own movers put into and took from the counted stores
  between units, plus each such store's residue since the window opened: the store's stock less
  its ports and every pair's counted moves there (SC88). A residue is another mod's mover, a
  player's hand or a count missed, and it fails every unit at the store on that good, so the
  district learns as a judging group would.
- A unit's window opens only at a sweep whose sample holds every pair at its counted stores. A
  sweep missing one discards the unit's window, with warm-up; a pair whose `since` changed (Java
  forgot it: a restart, a set without it) starts the window again from that sweep, with no
  warm-up (SC91). A counted store a unit's takers run dry is short in their units, split by flow
  share, as a shortfall at an interior store is (SC88).
- **Why not at the store.** A counting wrapper around the store's handler, put in place at NeoForge's
  `BlockCapability.getCapability` (:200), changes Create's behaviour:
  - an item vault's handler is a `VersionedInventoryWrapper`, and `VersionedInventoryTrackerBehaviour`
    tests for that class (`stillWaiting`, its line 25; `awaitNewVersion`, line 36), so funnels, chutes,
    smart chutes, packagers, threshold switches and smart observers would rescan every tick behind a
    wrapper: a weak-PC cost. The packager (lines 211, 216, 401), the threshold switch (245) and the
    basin (447, 595) test handler classes too;
  - `level.invalidateCapabilities` does not refresh `CapManipulationBehaviourBase`'s cached handler,
    which refreshes only on a target change, every 64 ticks, or when it is null (lines 104 to 106 and
    111 to 114; the lookups at :143 and :147), and belt tunnels keep theirs in a plain field
    (`BeltTunnelBlockEntity` lines 66 to 68, `BrassTunnelBlockEntity` 746 to 751);
  - arms always ask with `Direction.UP` (`ArmInteractionPoint.getHandler`, through
    `BlockCapabilityCache.create`, lines 99 to 103), so a face cannot tell an arm from a funnel on top.

### 4.3 Judging groups

The sections at a shared store `h` that cannot be told apart for some goods form a **judging group**
for those goods. A group is judged on one row in which `h` is interior: in `Y`, the counted moves of
its sections at `h` give way to `ΔS(h,g) - ports(h,g)`; in `P`, the predicted flows at `h` give way to
`pred(h,g)`. Both sides then hold every move at `h`, counted or not, and the row balances when the
model is right. A group forms:

- **from the compile**, at a shared store that a modelled mover reaches with no count point: a vanilla
  hopper (the compile models hoppers as movers, `CreateCompile.java:329` to `:330`, `:738` to `:739`),
  the drill's shortcut, or any store past `glass.countedStoresMax` (§7.3);
- **for one window**, at a counted store whose residue over the window is not zero: the model lacks that
  move, so the group's row fails on those goods as the district window does, and the district's other
  sections are judged apart. As built (S3, SC88) no group is formed: the residue is added to the row
  of every unit at the store, which fails each of them on those goods, as the group's row would;
- **for a station's ports**: the units holding the inventories one station's interfaces reach are
  judged as one, since its cargo cannot be split between them (§4.4, SC75).

A player's session at a shared store is a port into it (`obs.rs:150`), not a residue. A group is
smaller than the district. Shared stores after the fold are few, measured on the reference base, see
the local notes; how many of them a hopper reaches was not measured (§14). A group goes stale as a
whole (§3.6).

### 4.4 Ports

- A port with an inventory (`PortTransfer.inventory`, `obs.rs:163`) belongs to the section of the node
  that holds or claims that inventory: a store's section when the store is interior, the mover's
  section for a frogport or postbox (they claim their inventory, §2.2). A port into a shared store is
  in no section's `Y` and cancels in the identity of §4.1, except inside a judging group's row, where
  the store is interior.
- **Train cargo** is the whole train's change at departure, with no inventory (`TrainWatch.java:366` to
  `:381`). Portable storage interfaces look up no neighbour: they expose the train's storage, and the
  movers beside them pull and push. The compile claims each interface that reaches a district inventory with no node (SC95), so a mover
  beside one has no supply in the graph: what it moves is the cargo, booked as the port. `IND_STATION_PORTS` (`TrainWatch.ports`, `TrainWatch.java:470`, its record at `:502`) names the district
  inventories the docked interfaces reach through conveyance. The units holding one station's ports are joined
  into one (§4.3, SC75), so the cargo is that unit's port, booked once at its first inventory. Cargo
  naming inventories of several units (before the station's ports joined them) or of a unit and of none
  costs those units their open windows (`STATION`); cargo naming no inventory, or none a unit holds,
  stays in `ports_d`. Java journals the departure's `IND_STATION_PORTS` before its `TRAIN_CARGO`
  (`TrainWatch.java`), and the engine finds the station through the train's arrival, which the
  departure that follows removes.
  A split per interface would need a mixin on the interface's transfer (§10.3, not in these slices).
- A player's toss or pickup has no inventory and stays in `ports_d`; a player's session at an inventory
  is a port with that inventory.
- Frogport packages (§2.5) and postbox mail carry their inventory, so they always belong to a section.

---

## 5. Paging

- Sections live **inside** the district's glass page (G2-80, `persist::GLASS_TABLE` 0x4903), not in a
  table of their own: the per-section fields of §3.3, and the open window rows as one sorted array.
  `section_of` is not saved: it is recomputed from the compile's records on restore, as `Graph::build`
  rebuilds the indices.
- The page magic becomes `IGL2` (`glass/page.rs:32`) in slice S2a, with the sections, `IGL3` in
  S2b, with the judging units' windows (SC63), `IGL4` in S2d, with the station joins and the
  `STATION` discard count (SC76), and `IGL5` in S3, with each unit's counted pairs and residue
  bases (SC92). A restored page whose stored fingerprints do not match
  the sections recomputed from its compile (a changed tunable such as `sectionsMax`, or a changed
  fold), or one with an older magic (`IGL1` to `IGL4`), drops **that district's** glass state, and the
  district learns again; the restore logs one line with the number of districts dropped. Only an
  undecodable page fails the whole restore, as G2-82 says.
- **Package stores** each get one page in the same page store (a new table next to `GLASS_TABLE`, id
  given at build), in slice S4: the pools per destination frogport and the no-route pool, the held
  packages and the drawn keys with the engine's epoch, and what is owed per frogport. Shared kinetic
  networks hold no state of their own: they are rebuilt from the districts' compiles and replays.
- Rows are a `Vec` sorted by (section, good), never a map per section: a one-entry map node costs far
  more than its row.
- The page grows by the extra rows (each good counted once per section that touches it, instead of once
  per district). Today every page is written whole at every save (G2-83); writing only changed pages is
  For Ben 15, unchanged here.

---

## 6. Migration

No migration code (no backward compatibility before release; G2-85). On load:

- **An older `IndustryData` format with districts.** Today an older format only warns and loads the
  districts, dissolving them as detection reaches them. New (format 3, slice S4a; choice SC98): Java
  drops them on load and keeps the rest (exclusions, merges, patches, the id counter), and at the
  session's start journals an `IND_DISTRICT_REMOVED` for each dropped id, so Rust drops every district
  Java no longer has, with its pages, whether or not a glass page exists. The engine itself is not
  reset. Districts re-derive from the seed attachments at the first detection, under new ids. Slice
  S6's format 4 (join pairs) goes through the same path.
- **A glass page with magic `IGL1`** (slice S2, before any format change) drops its district's glass
  state, as §5 says; the removals of a format drop remove the page with its district.
- The operator sees one line at start: "rtsim industry: the save format changed (SEC-1); learned
  factory state was reset and districts are detected again." Nothing is lost silently, and only the
  sim's own knowledge is lost (UR-1).
- **Plugins' state.** A drop from a format before 3 also drops the plugins' state in `IndustryData`
  (format 3 is the first to have any). Slice S6's format 4 keeps it (`PLUGINS_KEPT_SINCE`, choice
  SC155): the engine keeps its package stores in pages through the districts' removal (SC143), so
  Java's store ids and frogports stay theirs instead of starting again at 1. The drawn keys of §2.5 name the engine's epoch, which a format drop
  does not change; a key Java no longer holds is stripped and its package released.
- No live world runs rtsim (DR-0). GameTest worlds and rtsim-view's fixtures are regenerated after
  landing, as G2-85 did.

---

## 7. Cost on a weak PC

### 7.1 Detection (DS-11)

- J1 adds a class test per touch. J3 adds one pair per store whose controller lies in another chunk,
  from a canonical position the walk already computes.
- J2 adds the arm's points and the ejector's target for the arms and ejectors the walk already visits.
- J4 adds, for kinetic block entities within one block of a border only, the diagonal positions
  Create lists (4 for a small cog, 12 for a large one, of which at most 7 lie outside its chunk) and an
  `isConnected` test, read through `getChunkNow` (never loading a chunk). These reads, like J1's face
  reads and J2's target reads, are not counted in `rescanBlockEntitiesPerTick`: the budget counts the
  chunk's own block entities and its inventory candidates (`scanChunk`'s return).
- The commit's union becomes cheaper: over the recorded joins instead of nine lookups per member chunk.
- Worst case: a district made of border cogs. Each costs at most 11 neighbour reads (7 diagonals and 4
  faces for a large cog at a chunk corner) besides itself, so a tick's reads stay within a small multiple
  of `rescanBlockEntitiesPerTick` plus one chunk; the time limit (`rescanSliceMicros`, R2-11) bounds the
  tick whatever the reads.
- `Rescan.grown()` no longer pulls in the 8-neighbour districts of every reached chunk (§2.1), so an
  incremental detection walks the districts its edit can join, not a whole patch of factories side by
  side. `Rescan.joinedToAway` reads each district's saved pairs once a detection.
- Join pairs: 16 bytes each; a district has at most its touches' worth.

### 7.2 Sections (engine)

- Sections are computed once per accepted compile: a union-find and a breadth-first fold, linear in
  nodes and edges, next to a compile that is itself linear and rare (edits only). A fingerprint is one
  pass over the section's records.
- Memory: a section header of about 64 bytes (ESTIMATE), `section_of` 2 bytes per node, and per
  (section, good) row about 350 bytes (ESTIMATE: the open row's `Acc` and `Cum` and the passed
  window's `GoodResult`).
- The rows that are new are the goods of stores shared by sections, counted once per section. The
  extra memory is small next to today's glass state, measured on the reference base, see the local
  notes.
- **Bounds.** The caps count only the **extra rows**: a district's rows beyond what one section per
  district would hold, which are today's rows and stay unbounded as today. Per district,
  `glass.sectionRowsMax` (TUNABLE, 1,024 extra rows, about 350 KiB, ESTIMATE); per world,
  `glass.sectionRowsWorldMax` (TUNABLE, 16,384 extra rows, about 5.5 MiB, ESTIMATE). Over either, the
  district with the most extra rows (ties by the lowest district id) merges sections by §3.2 step 5,
  at the sweep that passed the cap (SC57), until both hold. Merging all of a district's sections reaches
  no extra rows, so the caps can always be met. A merged section starts learning (§3.2): coarser
  sections, and the merged sections' learning is lost.
- **Saves.** Every glass page is written whole at every save today (G2-83); the extra rows add to that
  until For Ben 15 writes only changed pages. Package store pages are small: a pool per destination
  and the held packages.
- **Clusters.** Every district on one package store replays in one cluster. The work is the sum of
  the members' replays, but a cluster settles as one model, and its catch-up costs its breakpoints
  times its diffusers (`cluster.rs`, the module comment: one multiply per diffuser per settle).
  Coupling n districts of similar size costs about n times more per breakpoint than replaying them
  apart. `industry.clusterDistrictsMax` (TUNABLE, 16) bounds it: a package store whose districts
  exceed it is not coupled, and moves nothing while any of its ends is away.
- **Shared networks** (choice SC153). Each district replays in its own cluster, never merged by a
  network. A change in an owner's published rows settles each replaying reader's cluster to that tick
  and re-runs the reader's parts: one settle and one re-solve per reader per change, at most 8 rounds
  a tick (`NET_ROUNDS`); after that a reader whose rows keep changing falls back as oscillating. The registry is
  rebuilt (a union-find over the parts' source positions, linear in sources) at a compile accepted or
  dropped, a district removed and a restore, and is never saved. A network with parts in more
  districts than `industry.clusterDistrictsMax` is past the cap: its parts run decoupled (each bounds
  its members on it and learns their rates, as S1's unstable domains). A network's status change is a
  note in the engine's industry log, so the lost coupling is never silent (Q2, Q4).
- The replay costs what it costs today inside a district: parts are formed as today, over the
  replaying nodes. A fallback's rebuild repeats at most one replay step per frozen section.

### 7.3 Counters (Java)

- One static null test per real move at a count point (`Counting.active()` is null while nothing is
  counted), then one hash lookup on the mover's position when something is; the target's canonical
  inventory is worked out, and a counter updated, only for a counted mover. Counted movers are only those
  at shared stores, which the fold keeps few. A cap
  `glass.countedStoresMax` (TUNABLE, 256 per district, SC78) turns the rest into judging groups.
- S1's guards add one `shouldTickBlocksAt` or `getChunkNow` test per chain lookup and per member in a
  network recount, and each saves a chunk load that today costs far more.
- The frontier adds a route entry per far frogport on each boundary conveyor, refreshed once per advert,
  and one key per held package. The census walks a store's conveyors once per sweep while all of it
  ticks.

---

## 8. Reach

### 8.1 Commands

- `/rtsim district sections [id]`: per section, its nodes, stores, verdict (learning, passed, failed,
  stale, frozen, replaying), the last window's worst good, and the shared stores with their counting
  state and residue. `/rtsim district info` gains one line: sections, and how many replay.
- `/rtsim district graph <id>`: writes the district's compile and members to a file in the server's
  debug folder, the operator command the dump of §1 needed (slice S1); its sections, package stores,
  shared networks and join pairs join the file as slices S2, S4, S5 and S6 bring them. It reads only
  what the engine and the index already hold.

### 8.2 rtsim-cli

`rtsim-cli glass <world> [district] [--json | --graph]`: the paged glass state of a saved world, read
only through the read-only open, as `rtsim-cli lines` reads train lines (`crates/rtsim-cli/src/lines.rs`).
`--graph` writes the compile with `section_of` for offline checks (§9.4). `rtsim-cli stores <world>`
lists package stores with their pools, held packages and drawn keys.

### 8.3 Viewer

- rtsim-view's district card gains its sections (verdict, nodes, shared stores) and the district's
  join pairs; the map gains a sections layer below 1 block per pixel (VIEWER.md §7 already plans a glass
  layer), package stores as edges between frogports, and shared networks.
- mic-viewer is a public repository: the layer is code only; no world data goes into it.

---

## 9. Tests

Fixtures are built by code on the GameTest yard templates (`IndustryKit.YARD` for the Create module,
`industry_yard` for core), as the glass corpus is (`CorpusFactory`, `GlassCorpusKit`), never from world
copies. A factory that must cross a chunk border places itself relative to the absolute chunk grid
inside the yard, as `CreateSeedGameTests`' chain helper does.

### 9.1 GameTests, districts and links (Level 1)

| Test | Fixture | Checks |
|---|---|---|
| `aCompileLoadsNoChunk` | a district beside an unloaded chunk holding the network root, a conveyor and a frogport's neighbour | the compile leaves the chunk unloaded (§2.8) |
| `anAwayDistrictOnAChainStaysAway`, `anAwayDistrictOnASharedNetworkStaysAway` | a loaded factory and an away one on one chain, then on one shaft network, with packages moving and the boiler's level changing | 200 ticks with no `IND_CHUNK_LOADED` for the away district (§2.8) |
| `aNetworkRecountKeepsAnUnloadedSource` | a network whose source's chunk is unloaded, then a recount (a member added); loaded and unloaded again, then another recount | no chunk load; the capacity equals Create's with that chunk loaded (the contract test of §2.8); one listed object per position after the second visit |
| `aDemotedChunkComesBackInStep` (built in S1) | an away chunk held in memory below FULL while its source's speed changes, then FULL again | no load; the part takes its source's speed and the network update held back (§2.8, SC45) |
| `aChunkLoadedFullComesBackInStep` (built in S1) | two away chunks on a chain, unloaded while its source reverses and taken out of the network as a restart leaves them, then loaded FULL and not ticking, the far one first | both take the source's speed at the near one's load, every part joins its network once (the unloaded share drops by exactly their share), and nothing breaks when the chunks tick (§2.8, SC73) |
| `twoFactoriesSideBySideStayTwo` | two press lines in neighbouring chunks, nothing between them | two districts (today one) |
| `aVaultAcrossABorderJoinsItsChunks` | a packager in one chunk under an item vault whose controller is in the next | one district (J3) |
| `diagonalCogsAcrossABorderJoin` | a large cog meshing diagonally with a small cog across a border | one district (J4) |
| `anArmReachingAcrossABorderJoins` | a mechanical arm taking from a depot in the next chunk | one district (J2); a detection before the arm's first tick keeps the pair |
| `aChainInsideOneDistrictStaysEdges` | two frogports of one factory on a chain inside it | `CHAIN` edges, no package store, no `CHAIN_*` record |
| `aChainBetweenTwoFactoriesLeavesThemTwo` (exists as `aChainBetweenTwoDistrictsLeavesThemTwo`) | two factories on one chain, frogports at both ends | two districts; one package store; every package a `CHAIN_OUT` at one end and a `CHAIN_IN` at the other; both stay glass; both windows balance |
| `twoFrogportsOfOneDistrictOnASharedChain` | a third frogport of the first factory on the same chain | its packages to the first frogport are counted once each way, with no `CHAIN` edge |
| `anUnloadedMemberKeepsItsJoins` | a district with one member chunk unloaded before a detection | the same district after it (saved pairs) |
| `aStationBetweenTwoFactories` | a station touching two factories, its interfaces serving the one whose chunk it does not share | two districts; the station's chunk and its cargo with the one its interfaces serve, also after a restart before the next docking |
| `aBoilerOnAChainDrivesTheNextDistrict` | a boiler (one burner, a pumped tank, one engine) and a millstone line in one district, a chain carrying rotation to a millstone line in the next | loaded, both compiles report SHARED parts of one network (each the other's one peer, the engine's shaft driven in A and foreign in B, no tail, members not bounded); away, both lines grind at the boiler's speed until the burner runs out in A's replay and at the passive 16 RPM after (SC154), as each district's reconcile shows in the wheat |
| `aPackageIntoAChunkThatDoesNotTick` | a chain from a ticking chunk into one that does not tick, with a reversal | first records what Create does (route expiry, the reversal at line 175); with S1's guards the package waits on the last ticking conveyor and the next chunk is not loaded |
| `aRouteToAnAwayFrogportIsKept` | the same, the far frogport's district away, then a restart | with the frontier, packages for the far frogport reach the boundary conveyor and are held there, before and after the restart; one `CHAIN_HELD` each |
| `aHeldPackageDrawnIsRemovedOnce` | the far district drew a held package | it is never handed on; after the far reconcile it is gone from the chain; after an engine reset before the reconcile its key is stripped and it lands as a normal `CHAIN_IN` |
| `aPlayerTakesAPackageOffTheChain` | a player takes a package off a store's chain and hangs another | the next census sets the pools; no pool goes below zero |
| `aNetworkRecountKeepsAnUnloadedSource` (S1's, extended in S5-1) | a motor's network across two districts, the motor's chunk unloaded | the loaded half keeps its speed, is not overstressed and keeps the motor's capacity, 600 ticks on and through a recount, with S1's guard on (§2.6, SC148) |
| `AdoptGameTests`, `RescanBudgetGameTests`, `DistrictSeedGameTests`, `ReinstallGameTests`, `CreateSeedGameTests` | existing, layouts changed in S6 | adopt equals a full detection, the budget holds. Their chunks side by side were one district by adjacency; S6 gives the budget, adopt and reinstall layouts an inert block on each side of a border that should join (J1), the adopt's bridge chunk becomes a district of its own (an adopt adds seeds, not joins), and the seed test's diagonal marker is a district of its own |

### 9.2 GameTests, sections (Level 2)

New `CorpusFactory` fixtures next to `PackageLoopFactory` and `BoilerOverstressFactory`:

| Fixture | What it is | Checks |
|---|---|---|
| `VaultBetweenMixersFactory` | a vault between two lines (one fills it by chute, one draws by smart chute; SC83) | two sections, the vault shared and counted at its two movers, the counts exact against the vault's stock; each passes its own window; break and replace the second line's press in one tick: the first keeps its passed window, the counters go on (SC84), and both replay while away |
| `HopperAtASharedVaultFactory` | the same with a vanilla hopper also feeding the vault | the hopper is modelled and not counted, so the compile makes the two sections a judging group with the vault interior; the group's row balances and passes |
| `ResidueAtASharedVaultFactory` | `VaultBetweenMixersFactory` with the test inserting into the vault through its capability for one window (another mod's mover) | that window's residue fails both units on the inserted good, as a judging group would (SC88); the next window judges the two apart again |
| `BasinIntoASharedVaultFactory` | a mixer's basin pouring into a chute onto a vault another line draws from (a basin cannot pour into a store itself; SC82) | two sections, each passes; the basin's pair, forced by the test, counts the recipe's output and the chute counts it on into the vault |
| `BeltIntoPressFactory` | a belt straight from a press into a saw | one section (SEC-1's "cannot be split") |
| `SharedBoilerTwoLinesFactory` | one boiler network driving a crushing line and a press line that share nothing else | three sections: the boiler's, and two lines on one domain (Q1 a); an edit to the press line's belt stales only it; adding presses until the stress passes the least capacity the crushing line's window ran at stales the crushing line too |
| `StationPressLineFactory` | a station's portable storage interface emptied by a smart chute into a vault feeding a press line (`StationCargoGameTests`' kit) | the cargo maps through `IND_STATION_PORTS` to the press line's unit: a loaded call in the middle of its window leaves the window to pass, with the unit's goods bounded; cargo whose ports name that unit's vault and an inventory of no unit takes the `STATION` discard (the negative control) |
| `FrozenNeighbourFactory` | two lines on a vault, the first made to oscillate | the fallback names the first section, which freezes; the replay is rebuilt at the fallback's tick, and the second goes on drawing from the vault until it is empty; the district does not go to the black box |
| `HalfLoadedFactory` | two lines in one district, one in chunks out of simulation distance | the ticking line learns and passes; the other replays if passed |

Each runs the corpus cycle: loaded for a window, away for a replay, back for a reconcile that matches.

### 9.3 Engine tests (`glass/tests.rs`, `cluster.rs` tests)

- The cut and the fold on hand-built graphs: a vault between two mixers, a belt into a machine, a
  shared domain, a transfer chain, a storage system, idle movers, residual groups, `sectionsMax` and
  the merge fallbacks.
- Fingerprints: the same factory with other state slots keeps every section; the same factory with its
  nodes in another order gives the same fingerprints; one changed section restarts alone; a machine
  added elsewhere on a domain changes no fingerprint; the domain check stales only past the least
  capacity run at.
- Accounting: the identity of §4.1 on random graphs, with ports into shared stores and residues; counted
  and uncounted shared stores; a judging group's row with its store interior balances where the
  sections' rows cannot; train cargo counted once; ports by inventory; the verdict rule of §3.4; each
  discard reason's reach (§3.6).
- Replay: a gated-off section's stores are touched only by its neighbours; a fallback freezes the
  sections it names and rebuilds at its tick, once per section, keeping earlier bookings; a district
  goes to the black box only when no section replays; a frozen boiler stops its domain; per-section
  liveness; G2-84's paging tests extended to sections (a save and restore before each step, equal pages
  and commands).
- Shared objects: a shared network's parts joined through their frames (a ratio 2 chain), a peer
  without a part, more parts than the cap, two parts in one district, parts that cannot agree, the
  tail counted once; a part solving on its input and one without an input (decoupled, bounded); a
  boiler in one district driving the next's press line until its coal runs out, through a restore; a
  frozen boiler giving its network nothing; a peer with no compile; a cycle's least row (SC151); a
  burner out running its engines on the passive row (SC154); a package store's split and merge; routing per
  exporter with a frogport on the way, `*` last and ties by position; room per route; the census and
  the floor at zero; an end in the black box neither books nor draws; held packages, drawn keys,
  epochs and what is owed; `clusterDistrictsMax` and its fallback.
- The page: `IGL5` round trip; a page whose fingerprints disagree with its compile, or an `IGL1`
  to `IGL4` page, drops only its district's glass state; the row caps count extra rows only, and merging
  meets them.

### 9.4 The reference base, offline

The dump of §8.1 and `rtsim-cli glass --graph` let the reference base's numbers in the local notes be
repeated on the laptop copy after each slice. That check stays local; nothing from it is committed.

---

## 10. Build plan

### 10.1 Slices

| Slice | What | Tests | Depends on |
|---|---|---|---|
| **S1** | No chunk loads (§2.8): every compile read through `getChunkNow`, members filtered; mixins on Create's chain lookups (lines 161, 175, 248, 296 to 297 of `ChainConveyorBlockEntity.tick`; `canAcceptPackagesFor` :110) and network recount (`calculateCapacity`, `calculateStress`, `sync`, `remove`); built: also rotation propagation and the stale re-sync, the frogport's far end and the exports to ports (PATCHES.md rows 50 to 59). The driver: a network whose members lie in more than one district, or in chunks of no district, is marked in every district on it as today's unstable domains are (`CreateCompile.java:487`, `:641`: its members are bounded, their rates learned), the boiler's district included, so no district replays a driver or a load it cannot see. `/rtsim district graph` (compile and members); `rtsim-cli glass` | `aCompileLoadsNoChunk`, `anAwayDistrictOnAChainStaysAway`, `anAwayDistrictOnASharedNetworkStaysAway`, `aNetworkRecountKeepsAnUnloadedSource`, `aPackageIntoAChunkThatDoesNotTick` (its first half), `aDemotedChunkComesBackInStep`, `aChunkLoadedFullComesBackInStep`, `aBoilerOnAChainDrivesTheNextDistrict` (driver part), the CLI on a GameTest world | nothing |
| **S2** | Sections in the engine: cut, residual groups (Java), fold, state, fingerprints, per-section windows, liveness and discards, the verdict rule, judging groups from the compile, staleness by box, edits between districts and the domain check, per-section freeze with the rebuild at the fallback's tick, `IGL2` with the per-district drop; train cargo through `IND_STATION_PORTS` | §9.3, `BeltIntoPressFactory`, `SharedBoilerTwoLinesFactory`, `FrozenNeighbourFactory`, `StationPressLineFactory`, `HalfLoadedFactory`, `HopperAtASharedVaultFactory` | nothing: sections inside today's districts already pay off |
| **S3** | Counting at the movers (Q3): `IND_COUNT_SET`, `IND_COUNT_SAMPLE`, the mover mixins, judging groups by residue | `VaultBetweenMixersFactory`, `ResidueAtASharedVaultFactory`, `BasinIntoASharedVaultFactory` | S2 |
| **S4** | Package stores (Q4): `IndustryData` format 3 with the drop and the engine reset (§6), the store record and identity, `CHAIN_OUT` and `CHAIN_IN` at the frogports of package stores, no `CHAIN` edges on them, pools per destination with routing, room and the census, and the both-away replay on the first cluster that couples districts (a pool shared between them); then the frontier (Q5): routes kept, the hold, `CHAIN_HELD`, drawn keys and their removal | `aChainInsideOneDistrictStaysEdges`, `aChainBetweenTwoFactoriesLeavesThemTwo`, `twoFrogportsOfOneDistrictOnASharedChain`, `aPlayerTakesAPackageOffTheChain`, then `aRouteToAnAwayFrogportIsKept`, `aHeldPackageDrawnIsRemovedOnce`, the store's engine tests | S1. Useful before S6: chains between districts that are apart today are counted nowhere since DS-10 |
| **S5** | Shared kinetic networks (Q2), in two parts: S5-1 every domain's sources and speed rows within a district (SC144 to SC148), then S5-2 a part per compile, the network registry, the network's state in each part's solve, readers settled at each change (SC151 to SC154) | the extended `aNetworkRecountKeepsAnUnloadedSource` first (S5-1), then `aBoilerOnAChainDrivesTheNextDistrict` (away part), the shared network's engine tests | S1, S2 (the domain check), S4 (the clusters) |
| **S6** | Level 1: joins J1 to J6, no bare adjacency, saved join pairs, the station rule with `TrainWatch.ports` resolving any district, `IndustryData` format 4 through S4's reset; the stale comment in `CreatePlugin` on chain conveyors | §9.1's join and station rows, and the existing detection tests | S1, S4, S5: no district is split before its packages and its network have somewhere to go |
| **S7** | Reach: `sections` command, info line, `rtsim-cli stores`, viewer layer and card | command GameTests, rtsim-view fixtures | S2; S4 and S5 for stores and networks |

New mixins go in PATCHES.md (P7) with contract tests.

### 10.2 Reuse

| Need | Reused | Where |
|---|---|---|
| The walk, touches, canonical inventories, plugin links, merge links, pins, budgets | district detection as built | `Districts.java` (DS-10 to DS-12) |
| Arm points | `CreateGlassAccess.armInputs`, `armOutputs` | the glass compile |
| A link found by a plugin | `FanLinks` | `CreatePlugin.links` |
| Rotation across a border | Create's own `RotationPropagator.isConnected` and `addPropagationLocations` | Create |
| The docked interfaces' inventories | `IND_STATION_PORTS` | `TrainWatch` |
| A package record's layout | `IndRecords.Package` (not its engine path) | `MailWatch` |
| Writing packages into a loaded frogport | the mail write-back | INDUSTRY.md §8.2 |
| Holding packages back | Create's own hand-off test and route adverts | `ChainConveyorBlockEntity`, `ChainConveyorRoutingTable` |
| An unloaded source's last capacity | Create's network lists, which keep unloaded block entities | `KineticNetwork` |
| Districts replayed together | `Cluster` and its `requires` constraint (I13) | `cluster.rs` |
| Flow across a loaded and unloaded edge | G2-11's cut pools and what is owed | `replay.rs`, `engine.rs` |
| A domain the district cannot drive | the unstable domain's bounded members | `CreateCompile` |
| Gating nodes off | `Regime.gate_off` and `gates` | `solve.rs`, `check.rs` |
| Replay parts | `partition` (G2-22) | `replay.rs:568` |
| The same factory, the build fields | `same_factory`, `state_slots` (G2-81) | `glass/mod.rs` |
| A stable hash | `fnv1a64` | `rtsim_kernel::rng` |
| A command and a sample record for Java-side measurement | `IND_PROBE_SET`, `IND_PROBE_SAMPLE` (GB3, GB5) | `GlassRecords` |
| Paging | the glass page (G2-80) and the page store | `glass/page.rs`, `persist.rs` |
| A read-only CLI view of a saved world | `rtsim-cli lines` | `lines.rs` |
| Fixtures | `CorpusFactory`, `GlassCorpusKit`, `PackageLoopFactory`, `BoilerOverstressFactory`, `StationCargoGameTests` | the Create GameTests |

### 10.3 Custom (new code)

| Piece | Why nothing existing does it |
|---|---|
| Join pairs in `District`, format 3, J3's pair | the 8-neighbour rule needed no memory |
| J4 for border cogs | the walk only tests faces |
| The station's one district | DS-10's anchor rule joins every adjacent district |
| `getChunkNow` at every compile read; the guards on Create's chain lookups and network recount | they load chunks today, and each load pulls an away district into a partial load |
| The package store: Java's record and identity, `CHAIN_OUT` and `CHAIN_IN` and their engine path, pools per destination with Create's routing, room and the census, a pool shared between districts | G4 is not built; every cluster pool belongs to one district; the compile models no addresses |
| Two frogport mixins | Create has no event for a package leaving or landing |
| The frontier (Q5 a): kept routes, the hold, `CHAIN_HELD`, drawn keys with the engine's epoch, the removal at reconcile | Create's routes to a frogport that does not tick expire, and nothing in rtsim draws from packages in the world |
| Shared kinetic networks: parts per compile, the summed level, glass-to-glass coupling across districts in a cluster | each compile sees only its own engines; G2-16 sends power-linked districts to the black box |
| Residual groups in `GraphBuilder` | one residual per district glues every line with an unknown block |
| Sections in Rust (cut, fold, fingerprint, rows, verdict rule, domain check) | the window, the verdict and the staleness are per district today |
| Per-section fallback naming and the rebuild at the fallback's tick | a fallback sends the whole district to the black box today |
| The mover mixins and the count records | the sweep reads levels, not flows |
| Edits between districts sent to the districts on a store or network | Java sends `IND_EDIT` for member chunks only |
| A per-interface split of train cargo (later, not in these slices) | `TRAIN_CARGO` is the whole train's change |
| `sections` and `graph` commands, the CLI views, the viewer layer | new state to show |

---

## 11. A player's walkthrough

Ana builds a brass works next to her rail line.

1. **The build.** A train station with a portable storage interface: a funnel beside it pulls crushed
   copper and zinc ore into item vault A. Funnels pull the ore onto a belt through a washing fan into
   vault B. Two mixers over blaze burners take from vault B by funnels and fill vault C with brass. A
   packager under vault C sends brass to a frogport on a chain conveyor. Forty blocks away on the same
   chain, a second frogport and packager fill vault D, and a belt from vault D runs through three
   mechanical presses into vault E. A boiler with four steam engines beside the brass works drives both
   buildings: its shaft runs along the chain.
2. **Districts.** Two districts, the brass works and the press hall: the chain is a link, not a join
   (§2.1). The chain is a package store with a port at each frogport (§2.5). The press hall's shaft
   network is the boiler's: both districts report their part of one shared network (§2.6). Before
   sections, if the two buildings' chunks touched, they were one district.
3. **Sections.** The brass works has three working sections: the washing line (vault A to vault B; the
   station's cargo maps to it through the funnel at the interface), the mixing line (vault B to vault
   C, with the packager and frogport folded in downstream), and the boiler with its engines, its
   burners and the pump that feeds it. Vault B is shared, and its funnels count their moves. The press
   hall has one section (vault D to vault E). `/rtsim district sections` lists them, each "passed"
   after a loaded window.
4. **An edit.** Ana breaks a mixer to put a third one in. The mixer's box is the mixing line's, so the
   mixing line goes stale; the washing line does not, and the lighter load on the shaft passes the
   domain check. She flies off before a new window passes. While she is away the washing line replays
   and vault B fills; when it is full the fan line stops, as Create's would. The mixing line is stale,
   so no brass leaves on the chain, and the press hall uses up vault D and stops. Before sections, the
   whole brass works froze at the edit.
5. **Back home.** Ana returns; the reconcile writes vault B's level and vault D's emptiness into the
   world. The mixing line passes a window with three mixers, and the next trip away replays all three
   sections. If the third mixer's stress passes the least capacity the washing line's window ran at,
   the domain check stales the washing line too, and it learns again in the same window.
6. **Standing at the brass works.** On her next visit Ana stays at the brass works while the press hall
   is away. Brass packages for the press hall travel to the last conveyor that ticks and wait there;
   the press hall's replay draws them, and they leave the chain when the press hall loads and its
   reconcile writes what it made. If she stays long, the waiting packages fill the near conveyors and
   the brass works' packager backs up, as it would before a full press hall.
7. **The boiler.** If the boiler's burner runs out while both buildings are away, the brass works'
   replay drops its engines to Create's passive heat (16 RPM, SC154). The press hall's replay re-runs
   at that speed at the same tick, slower, or stops if the network's capacity no longer carries both
   buildings' stress there. While a building is loaded, Create's own network does the same.

---

## 12. Questions for Ben

All six answered by Ben on 2026-10-22, each the recommended option (DECISIONS.md, SEC-1 Q1 to Q6).
The options below stay as the record of what was offered.

1. **Kinetic networks inside a district.** **Answered (Ben, 2026-10-22): (a), as recommended: a shared input that sections cut through.** One shaft network often runs every machine in a building.
   - **(a) Recommended. A shared input.** Sections cut through a network; each takes its speed from
     it, and goes stale when the network's stress passes the least capacity its window ran at. Cost: a
     network is one solve unit, so the sections on it replay in one part; a failure freezes only the
     section it names, and the part goes on over the rest (§3.5). This keeps sections the size of
     working lines, measured on the reference base, see the local notes.
   - (b) One section per network, as SEC-1's "a shared shaft network cannot be split" reads literally.
     A building run from one network is then one section, and sections gain little there, measured on
     the reference base, see the local notes.
   - (c) Split only fixed-speed networks (water wheels, motors); a boiler network joins its sections.
2. **Rotation across a district cut.** **Answered (Ben, 2026-10-22): (a), as recommended: districts stay apart on a shared network and replay together; past `industry.clusterDistrictsMax` they are not coupled.** A chain conveyor carries rotation as well as packages.
   - **(a) Recommended. Districts stay apart on a shared network:** each district reports its own part,
     the network's state is decided over all parts, and the districts on the network replay together.
     Cost: new glass-to-glass coupling across districts (§10.3), and a cluster's catch-up costs its
     breakpoints times its diffusers. Past `industry.clusterDistrictsMax` (16 districts, TUNABLE, §7.2)
     a network is not coupled: its districts bound their members on it and learn their rates, as
     unstable domains today, logged and shown by `/rtsim district info`.
   - (b) A chain that carries rotation joins districts. A network that runs through a whole base then
     joins its districts back into one, the giant district GA-2 turned down, measured on the reference
     base, see the local notes.
   - (c) No coupling: a district without the boiler takes the network at one fixed level, as today. It
     keeps running when the boiler stops, making more than Create would.
3. **How flows into a store shared by two sections are counted.** **Answered (Ben, 2026-10-22): (a), as recommended: counted at Create's movers.**
   - **(a) Recommended. At Create's movers:** mixins where funnels, chutes, arms, basins, frogports,
     packagers and pipes move real goods; the mover's position names its section. Vanilla hoppers and
     other mods' movers at a shared store are not counted, and their sections are judged together (a
     judging group).
   - (b) At the store, by a counting wrapper on NeoForge's capability lookup: counts every mod's movers,
     but it must subclass Create's `VersionedInventoryWrapper` or funnels and chutes rescan every tick,
     cached handlers take up to 64 ticks to see it, and arms cannot be told from a funnel on top
     (§4.2).
   - (c) No counting: the sections at a shared store are always judged together, and only stores with
     a single section on each side split cleanly.
4. **What holds packages between districts.** **Answered (Ben, 2026-10-22): (a), as recommended: a package store owned by no district.**
   - **(a) Recommended. A package store owned by no district** (GA-2's shared store, built here
     first): its own saved data (an id, its frogports, pools per destination, paged), Create's routing
     and room, a census while it is all loaded, and a pool the districts on it share in one replay
     cluster. Past `industry.clusterDistrictsMax` it moves nothing while any end is away, logged and
     shown.
   - (b) No store: a `CHAIN_OUT` is booked as the importer's import after the travel time, routed at
     export. No new shared object, but packages for a full or away importer have nowhere to wait, so
     the exporter's replay stops at once, and a partly loaded chain gets no frontier.
5. **Packages on a chain while exactly one end is loaded.** **Answered (Ben, 2026-10-22): (a), as recommended: modelled both ways, the G2-11 way.** G-B6 says flow across a loaded and unloaded
   edge is modelled. Create itself forgets the route to a frogport whose chunk does not tick within 100
   ticks, after which packages for it circle near the exporter (§2.5).
   - **(a) Recommended. Modelled, the G2-11 way:** a mixin keeps the far frogport's route, packages
     wait in the world on the last ticking conveyor, the far district's replay draws them, and they
     leave the chain when that district's reconcile pays back what it drew; packages it sends to a
     loaded frogport are written in as real packages, as mail is. A reset leaves at worst a duplicate,
     never a loss. Cost: mixins into Create's routing table and hand-off, and keys on held packages.
     The flow toward the far side is bounded by the near side's room, about 20 packages per conveyor
     on the route.
   - (b) Modelled toward the loaded side only: the away district's packages are written into the
     loaded frogport, and packages toward the away side do what Create does (they circle, and the near
     factory backs up). No kept routes, no keys, no draws; the far factory starves while you stand at
     the near one.
   - (c) Zero both ways: packages wait on the chain as in Create, and nothing is written in. Simplest;
     both-away flow still works.
   - (d) Rust takes the packages off the chain as the far replay draws them: unbounded flow, but Rust
     then holds the only copy of a player's items until the far side loads, an exception to UR-1 only
     you can grant.
6. **Packagers at a district cut.** **Answered (Ben, 2026-10-22): (a), as recommended: cut at the chain, not the packager.** SEC-1's list of countable links names packagers.
   - **(a) Recommended. Cut at the chain, not the packager:** a packager joins the store it serves,
     and its packages are counted at the frogport or postbox they go through.
   - (b) A packager touching a block entity across a chunk border does not join: its chunk and its
     target's chunk are separate districts, and what it takes from the target crosses between them
     uncounted unless a new port is added at the packager (a mixin on its extraction).

---

## 13. Choices (Claude's, reversible)

| # | Choice | Why |
|---|---|---|
| SC1 | Joins J1 to J6; bare 8-adjacency is gone | SEC-1; adjacency held big districts together where nothing physical joined them, measured on the reference base, see the local notes |
| SC2 | J3: a multiblock inventory across a border joins its chunks, with a recorded pair | physical joins alone cut goods edges there, measured on the reference base, see the local notes |
| SC3 | J4 uses Create's own `isConnected`, for border kinetic blocks and overriding classes only | Create's rule, at a cost bounded by the border |
| SC4 | Arms and ejectors are plugin links, like fans; an arm that has not ticked keeps its pair | they move goods across borders without touching; an arm's lists are empty until its first tick reads its saved points (`updateInteractionPoints`, PATCHES.md row 84) |
| SC5 | Join pairs saved per district as a set, replaced only between chunks looked at, dropped with their chunk's membership; format 4 | unloaded members must keep their joins (DS-11) |
| SC6 | A station never joins by touch; its chunk joins exactly one district, the one its interfaces serve, found by chunk in any district and saved as a pair; its cargo goes there | a station between two factories must not glue them; cargo belongs where it moves; a restart must not forget it |
| SC7 | Package store frogports send `IND_PACKAGE` with `CHAIN_OUT` and `CHAIN_IN` on a new engine path; a chain inside one district keeps its `CHAIN` edges | the mail path forces the black box; one count per crossing |
| SC8 | Package store identity, split and merge; pools per destination resolved per exporter with capture on the way and ties by position; room of 20 per conveyor on the route (ESTIMATE); the census sets the pools, floor zero; an end in the black box or frozen neither books nor draws | Create's own rules (§2.5), and the moves no one counts |
| SC9 | The frontier keeps far routes alive on the boundary conveyor, holds packages by refusing the hand-off, keys them in `minecraft:custom_data` with the store, the engine's epoch and a serial, and removes only keys of the current epoch that the far reconcile lists; nothing is removed at a landing | Create forgets routes to a chunk that does not tick; UR-1 after a reset; a mod component may not survive an uninstall |
| SC10 | No chunk loads: every compile read through `getChunkNow`, Create's chain lookups and network recount guarded, all in S1 | each load pulls an away district into a partial load |
| SC11 | Until S5, a network whose members lie in more than one district is an unstable domain in every district on it, the boiler's included | defined behaviour that exists today, the same everywhere |
| SC12 | A shared network has no source district: its state is the tuple of boiler levels, its speed the fastest source's, its capacity every source's at its own speed; members in no district reported by the root's district (S5-2: every part lists them as its tail and the lowest district's part counts them, SC153) | Create's rules; several districts can hold boilers |
| SC13 | Sections cut only at `STORE` nodes; `CHAIN` and `LINE` edges join | SEC-1's stores; `LINE` is a depot's own chain |
| SC14 | Residual groups, one per face-connected group of unclaimed block entities, joined across an Ender channel; working when they touch a store, idle when they touch nothing; virtual nodes have no position | one residual per district glues every line; an unknown machine fails alone |
| SC15 | The fold: transfer into working, downstream first, ties by lowest position; storage systems merge; idle pieces keep nothing; merges by store, then domain, then size, ties by position, and a merged section starts learning | sections are working lines; shared stores become few; the cap is always reachable |
| SC16 | `glass.sectionsMax` 64, `glass.sectionRowsMax` 1,024 and `glass.sectionRowsWorldMax` 16,384 extra rows, `glass.countedStoresMax` 256, `industry.clusterDistrictsMax` 16 (TUNABLE) | weak-PC bounds; caps on extra rows can always be met |
| SC17 | The fingerprint: FNV-1a 64 over every build field, recipe and edge, and only the section's own nodes on a domain | stable across node order; a machine elsewhere on a network changes nothing |
| SC18 | Rows in one sorted `Vec` per district, store rows remapped by position | a map node per one-entry section costs more than its row; inventory indices change by revision |
| SC19 | A recompile keeps every section whose fingerprint did not change, open window included; only changed sections restart | G2-81's rule, per section; a builder elsewhere does not hold lines back |
| SC20 | Edits stale by node box; judging groups stale whole; commands stale all; an edit drops the district window; edits in chunks of no district on a store or network go to its districts as a recompile; the domain check compares stress with the least capacity run at | positions are in `IND_EDIT` already; every Create line is kinetic |
| SC21 | Liveness per section; `REVISION` and `SESSION` discard every section, `RECOMPILE` only changed ones, a station's `OTHER` the sections its ports named | a section learns while another part of its district is out of range; each discard reaches only what it spoils |
| SC22 | The verdict rule: a section replays only while it passed and the district window did not fail on its goods | the district window stays the checksum |
| SC23 | A fallback freezes the sections it names and rebuilds the replay at its tick, once per section; a part when it names none; the black box only when no section replays | SEC-1: each section replays on its own; the glass is the better model where it works |
| SC24 | The district window stays as the checksum and judges residues and ports no section takes | nothing is lost against today |
| SC25 | `IGL2`, sections recomputed on restore, a fingerprint mismatch or an `IGL1` page drops only that district's glass state | a tunable change must not reset the world's learning |
| SC26 | Migration is a loud reset: Java drops old districts and the format change resets the engine, at S4 (format 3) and S6 (format 4) | no backward compatibility before release; UR-1 |
| SC27 | The black-box learner stays per district | it is the fallback and the checksum |
| SC28 | A gated-off node keeps its stress; a frozen boiler stops its domain everywhere | Create's stress does not depend on work; conservative |
| SC29 | Train cargo to the sections `IND_STATION_PORTS` names; a judging group when several or a shared store; mapped after the whole step is read | interfaces expose the train's storage; movers do the moving; cargo is journaled before its ports |
| SC30 | Counting starts at the first sweep that carries samples after a count set changes | no window mixes counted and uncounted sweeps |
| SC31 | Judging groups judge on one row with the shared store interior; they form from the compile (an uncounted modelled mover, a store past the cap), per window (a residue) and for cargo | the only row that balances when a move is not counted |
| SC32 | Count points: funnels and other `InvManipulationBehaviour` users, chutes, arms, basins, frogports, packagers, pipes; not hoppers, the drill's shortcut or other mods | Create's movers at stores; the rest make judging groups |
| SC33 | Past `industry.clusterDistrictsMax`, a shared network falls back to S1's rule and a package store moves nothing while an end is away, logged and shown | bounded cost, never silent |
| SC34 | Slice S1's guards: a chain conveyor and a frogport look only at conveyors and ports in chunks that tick (`shouldTickBlocksAt` and `LevelChunk.isTicking`, the two tests a block entity's ticker makes, PATCHES.md row 59); rotation, the network recount and the compile read only chunks that are FULL (`getChunkNow`) | a FULL chunk that does not tick holds its block entities, and reading them loads nothing; a package handed into one stops there (§14) |
| SC35 | Create's `ChainConveyorBlockEntity.destroy` (a conveyor broken) stays unguarded; `removeInvalidConnections` and `notifyConnectedToValidate` keep Create's own `isLoaded` test | a break is a player's act, once, and the far end must drop its connection; the validations already ask |
| SC36 | Rotation stops at a chunk that is not loaded, chain connections and large cog diagonals included; the far part keeps its last speed until its chunk loads | Create's own rule for a shaft across a chunk border, applied to the links Create forgot |
| SC37 | The stale re-sync (PATCHES.md rows 56 and 58): at a block entity's first attach, a source in another loaded chunk that gives it another speed or network makes it wait one tick, then Create's `propagateMissingSource` on it and `propagateNewSource` from the source; a chunk that comes back FULL without loading, or loads FULL and does not tick, gets the same at its promotion (SC45, SC73) | Create breaks the block otherwise; the tick lets the loaded chunk's block entities join their network first; it also mends the break Create does at a shaft across a chunk border |
| SC38 | `/rtsim district graph <id>` lives in its own class (`GlassGraphCommand`), writes JSON to the server's `debug/` folder, and reads only the last compile and the index | the dump of §1 as an operator command; nothing is compiled or loaded to answer it |
| SC39 | `rtsim-cli glass` takes a district id, not a name | ids are what the paged state holds; names can repeat |
| SC40 | The compile reads a FULL chunk that does not tick as it is, and stops at one that is not FULL: an item dropped or thrown into a chunk that is not loaded goes to the sink, a crafter grid that reaches one is not modelled, and every stop is counted (`unloadedStops`) | a ticking district's eight neighbours are FULL, so stops happen only on long walks; a stop never invents a spill |
| SC41 | The frogport's far-end guard (PATCHES.md row 52) is checked by the launch only | a test would need a port in a ticking chunk whose conveyor reverses while the far end is away; the guard is one null answer Create already handles |
| SC42 | S1's GameTests build the away state with no forced chunk and a radius-1 ticket on A (A ticks, its ring is FULL, B two chunks away is at level 34 and never FULL); a creative motor stands in for boiler levels except in one boiler test, which fills its boiler through the tank's fluid capability; one batch per test | the away state of a district two chunks from the loaded one; a motor's speed changes on demand; the chunk load counts and the re-sync counter are server-wide, so no two of these tests run at once |
| SC43 | A pipe walk that meets a chunk that is not loaded stops there and bounds its pump | the far ends are not known, and a stop must never read as a spill |
| SC44 | A network is crossing when a listed source or member lies outside the district, or a listed member inside links out of it (a face across the district's border, a chain connection, a large cog's diagonal) to a chunk that is not loaded or to a kinetic block entity of the same network; with its root unlisted its ratios are taken from the lowest loaded listed member that turns | structural: Create's own count of members it has not seen (`unloadedMembers`) is wrong both ways (it is a saved number, decremented with a clamp, and reset by whichever member's save starts the network), and a link into a chunk that is not loaded may reach members that never loaded this session; the root's own speed is not held. `aCompileLoadsNoChunk` takes the away part out of the network, as a restart leaves it |
| SC45 | A chunk reported below FULL is noted until it unloads; when it is reported FULL or more again it is queued and re-synced (`StaleKinetics.chunkFull`) at the next server tick's start or end, not inside the promotion. Chunks that never left FULL are not queued; chunks loaded anew are, since SC73 | the promotion runs inside the chunk system's own task queue, and a propagation there could re-enter it; a promotion by several levels at once reports only its last status. Left open: the chunk is readable and ticks from its promotion, and the report of it runs later, from the main thread's task queue, so for up to one tick its block entities run before the re-sync; a source that changes in that tick, or Create's periodic validation falling in it, meets the stale part, and Create's own rule applies to it. A promotion reported for a chunk that a demotion has taken below FULL since leaves it noted |
| SC46 | `Loaded.capability` answers null for a Create multiblock (a vault, a tank) with any chunk of its box not loaded, the box taken from its controller (the least corner: `getHeight` along the main axis, `getWidth` across) | a vault's controller builds its handler from every member through `Level.getBlockEntity`, which loads; a tank's does not, so for a tank this is stricter than needed, and a tank across an away chunk reads as away |
| SC47 | S2 is built in four sub-slices: S2a the sections themselves (the cut, the fold, fingerprints, judging groups, `Compiled.sections`, the `IGL2` page with the per-district drop, `rtsim-cli glass` output, Java's residual groups), changing no window or replay rule (but SC55's residual groups change the graph a district with residual blocks sends: a node per group, not one per district, so its solve's SCCs, latencies and node count, and so its window or verdict, can differ); S2b per-section windows and verdicts, liveness, discards, staleness by box, recompile matching by fingerprint and the row caps; S2c per-section replay, the freeze and rebuild at the fallback's tick, the black box only when no section replays, the domain check; S2d edits between districts, train cargo through `IND_STATION_PORTS`, §9.3's GameTest fixtures and a residual groups GameTest | each lands with its own review and tests; S2a moves no replay, so a fault in the cut shows in its tests and its output before any window reads it |
| SC48 | The void sink (`SINK_VOID`) is a cut, as a store is, and lies in no box; a piece with an edge into it is working | one void sink per district would glue every line that drops into it; dropping goods into the void is work a window measures |
| SC49 | Only goods edges make a store a section's: an `OBSERVE` edge claims no store, and `HEAT` and `CONTROL` edges join the nodes they link, as any edge between two nodes that are not cuts does | a controller reading a vault another line fills moves nothing there; a heater and its machine, a switch and what it drives, run as one |
| SC50 | A section's box holds its nodes, the stores it touches (interior and shared) and each residual group's extent, grown by one block | staleness by box (S2b) must see an edit beside a section's store and inside an unknown machine's blocks |
| SC51 | The fold's ties: a transfer piece takes the section of a piece it reaches through one store, downstream (a store it gives into) first, then upstream, by the working piece's own lowest position, then the sum of its nodes' record hashes; each round's moves apply together at its end. The cap's merge ranks sections by (nodes, lowest position, record hashes) and takes the two smallest of a store's group (the group whose two are smallest together), else a domain's, else of all. Sections are ordered by (lowest position, fingerprint, first node), the last only between sections with the same records | §3.2's "never Java's node order": the result does not depend on the order pieces are visited in |
| SC52 | A section's fingerprint is FNV-1a 64 over its records, each encoded alone with its length and the encodings sorted: each node by codec kind, model, position (a virtual node by kind only), flags, rule, capacity, whether it has a domain, params with state slots zeroed (a boiler's active heat and water supply are state, as a heater's burn left and heat level are: the solve takes a boiler's level from its heat edges and water inflow) and its rates (a level by its selector and the node's own speed there, its ratio times the level's speed, when it is listed on its domain; a recipe by its contents); its stores; the edges with an end in it; per domain the `STRESS_DISABLED` flag, its levels' selectors and the section's own members with their speeds at each level, never the driver | the same factory in any node order gives the same print; a shared domain's other members are other sections', and a driver can change while the section does not; Create's root (which loaded chunks choose) scales ratios and level speeds both, not their products |
| SC53 | A glass page of format `IGL1`, or whose sections are not those its compile cuts into now, is read as absent: the district is restored with no glass state (as `rtsim-view`'s inspect restores one alone), its key is marked so the next save deletes the page, the host logs it, and the district learns again. A page that does not decode still fails the whole restore | §5's per-district drop: every other district's glass state is kept |
| SC54 | Until S2c the sections are in no `Report` and not in the bridge's `to_f64s`; `rtsim-cli glass` shows them (count, merges by the cap, judging groups, shared stores, each section's nodes, stores, box and fingerprint, and each node's section with `--graph`), cut with the default `glass.sectionsMax` | the bridge's report header is fixed, and Java needs the sections only once they replay |
| SC55 | Residual groups: the residual positions sorted and joined by their faces and by any plugin's `links` (not only Ender Transmission's) when both ends are residual, numbered by their lowest position; a group's node sits at its lowest corner with params `dx, dy, dz` (its extent) and the FNV-1a hash of its sorted positions, made when a plugin's edge first reaches it; the compile is a black box one when any group node was made | SC14's groups; the hook is the district detection's own, so a mod's wires join here as they join there; the hash tells two groups of one box apart |
| SC56 | S2b judges a **judging unit** (a judging group of §4.3: in S2, which counts no mover, every section sharing a store with another is in its group) on its own rows, and only with two units or more: one unit's window would be the district's. A unit's bounds per good: L and the class from its own store edges (the solve's class of the component past each edge, which never crosses a store); `U(u) = max(U(d) - (L(d) - L(u)), L(u))`, infinite when the district's U is or when the unit's own edges for the good are unbounded (past a black box or a port, by the solve's class); an exact unit is bounded only when the district's own why is `U_INFINITE` (another unit's black box does not loosen it); its variance is its own sources' and rolls', netted by the share of what they made that reaches its stores; its batch the largest into plus out of its stores | §4.1's per-section P, bounds and variance by the same sums, from what `Solution` holds today (`per_store`, the goods' bounds): no per-edge LP per unit |
| SC57 | The row caps cut at the sweep whose rows passed a cap, to the most sections below the district's count whose units would hold no more extra rows than allowed (the goods of the units each new unit merges, a binary search over `Sections::compute`'s counts: a lower count only merges more). The cut is kept in the district's glass state (`sections_cap`, saved) for its later compiles and its restores, under `glass.sectionsMax`. It is not a `report::discard` reason (that array is Java's report layout): units the cut leaves keep their windows, merged ones start learning | rows only grow at sweeps; keeping the cut stops a district from growing past the cap and merging again at every compile |
| SC58 | Liveness per unit: a sweep with IND_SWEEP_END's live flag runs every unit but those SC67 stops; one without runs the units whose chunks are all loaded, reported block-ticking by IND_CHUNK_TICKING and did not stop (unload, or stop ticking) since the last sweep that sampled (`DistrictGlass::stopped`, saved, cleared by a session start). For units an empty ticking set means nothing ticks: Java reports a chunk when it starts ticking and never one that has not, so G2-17's reading (an empty set ticks every loaded chunk) stays the district window's alone, as does the engine's `chunk_runs`. A unit whose chunks cannot be listed (a moving frame, a residual group past 256 chunks) never runs on a partial sweep. A unit that did not run warms up afresh, whether or not its window was open. A partial sweep in which no unit ran is today's discard of everything | §3.4: a chunk that unloaded and loaded again between two sweeps did not run throughout, though it is loaded at both; a district between the simulation and the view distance reports no ticking chunk, and its units must not learn that nothing moved |
| SC59 | A district that goes away (`Presence::Unloaded`) discards every unit's window, and no unit runs until it is back (`DistrictGlass::away`, saved), whatever a sweep still in flight at the unload says, live or not; its return (its presence leaves `Unloaded`: `Glass::back`) and a session start each discard every unit's window again, so each warms up afresh from there; the district window keeps today's rules | Java may send no per-chunk unloads (`per_chunk_unloads`), and a unit's window must not span a stretch its chunks did not run |
| SC60 | Ports per unit are kept per inventory a unit holds, cumulative since the compile (`Compiled::ports_at`): reconcile results applied to an inventory, a destroyed block's spill (out), port transfers naming an inventory, and packages in or out of an inventory whether the district is loaded or simulated; only for the compile's own revision. A unit's measured series is its stores' stock less them. Ports naming no inventory (a player's toss, cargo mapped to none) are the district's alone; station cargo maps through the station's ports (SC75) | §4.4's ports that belong to a section, from the records that carry an inventory today |
| SC61 | A unit's window follows the district's rules: the district's target length, three samples at least, warm-up from the compile and after each discard of its own, a pass clearing its edit and strikes, a fail adding a strike. A unit's failure derates nothing and probes nothing: the district's window still does that, but with the units active its search for the limiting node starts from the store edges of the unit whose own last window failed worst on the good (the lowest η, then the lowest unit), with that window's η and direction; with no such unit, or no node found there, over the district. A unit's window that started before the last derate (`Compiled::derated_at`, saved, carried by a recompile) was judged against another model and names no node; a derate on a unit's node (`UnitState::derated_at`) stales the unit's pass from before it, and the unit does not carry across a recompile, whose model has no derates (§3.6) | the verdict rule (§3.4) is what S2b adds; derates and probes act on the one solve the district has, and the district's η on one line of two would derate the line that matched |
| SC62 | A recompile of the same revision carries each unit whose key (its sections' fingerprints, sorted) is unique in both compiles: its passed window, strikes, verdicts and open window, which the old compile's model integrates up to the recompile; the recompile clears its edit (the compile found it as it was). Only when the engine knows the district (the new compile's first solve reads its stores); never across a revision change | §3.6; two units with one key cannot be told apart, so neither carries |
| SC63 | `IGL3`: the district page adds `sections_cap`, `stopped` and `away`, the compile adds `veto`, each unit's state, the units' rows (one `Vec` sorted by unit and good) and `ports_at`. The units and their bounds are recomputed on restore from the sections and the solve, and the saved unit state is checked against them (a mismatch is an invalid page). `IGL2` pages read as dropped, as `IGL1` pages do (SC53) | §5; nothing derived is saved |
| SC64 | Extra rows are counted as the (unit, good) pairs open or in a unit's last window, less the district's distinct goods among them; counted per district after each sweep, edit, discard and compile, and summed for the world cap (recounted on restore, not saved) | §7.2's "beyond what one section per district would hold": a good of one unit only costs nothing extra |
| SC65 | A unit may replay while its own last window passed (SC66), no edit staled it since, and the district's last concluded window failed on none of the goods the unit touches: those of its last window, its open rows, its bounds under the current solve, and every good its windows' rows held since the compile (`UnitState::touched`, saved: a discard drops the rows, not what the unit touched); with fewer than two units the district's rule (G-B4). Ports are kept only while the units are active. `rtsim-cli glass` lists each unit (sections, stores, chunks, window, last and passed windows, strikes, edit, may replay) with the extra rows and the cut; the units are in the `Report` since S2d (SC74) | §3.4's verdict rule; S2c's replay reads it |
| SC66 | A unit's passed window counts only if at least one of its goods passed, or if its goods are all unbounded (a black box, a port, a model demoted here, whose rows start unbounded as the solve treats it) or it has none (a unit with no store, as a boiler its pumps fill from a source, which only drives a network; or a window in which none of the unit's stores measured a good, as an idle line's), and the district's rule allows a replay (G-B4: a passed district window no edit staled): such a window judged nothing, so the unit leans on the district's pass, as the same line as a district alone does. Gated, a unit with no store would stop every unit on the network it drives, which passed with it running. A recompile keeps the district's veto: the new compile carries the goods the old one's last concluded window failed on (`Compiled::veto`, saved) until its own first district window concludes | §3.4; a recompile follows every edit, and the new compile's district window takes a warm-up and a window to conclude |
| SC67 | A chunk that stopped since the last sweep that sampled stops its units on a live sweep too (the sweep between was dropped, a skip under GB101, so the stop was never taken): the live flag has no dimension, so the stop is matched by chunk x and z, and a unit whose chunks cannot be listed does not run while any stop is recorded. The district window keeps today's rule | a dropped sweep must not let a unit's window span its chunk's unload |
| SC68 | S2c gates by unit (§3.5): with two units or more, the replay gates off (`Regime.gate_off`) every simulated node other than a store whose unit does not replay (it may not, SC65, or a fallback froze it), and every simulated node on a domain such a node drives, so a boiler of a unit that does not replay stops its network; stores stay, as pools only replaying neighbours touch, so the residual learned from the district's pass (G-B13) books into no store of a unit that is not on: shares are weighed over every store and a gated unit's share is dropped, not given to the others, and a freeze recomputes the residual of a replay that runs as one part (a split one started with none, and units only go off while it runs). Idle nodes (in no unit) and live nodes are never gated. With one unit or none, the district's rule as before. The domains are the district's compile's own: a network another district's compile also holds is not reached from here (the shared network's parts are S5's, §2.6) | §3.5; a gated node keeps its stress, so the gate changes no domain level |
| SC69 | A fallback names units (§3.5): each part counts per unit its `hits` (stops and watches of the unit's stores and controllers that fired since the part's last outside event or rebuild, the stop past `glass.breakpointsPerEventMax` included) and its `flips` (the unit's stores' and controllers' flags that flipped in the last regime build). Oscillating and Stuck name the units replaying in the part with the most hits, Unsettled those with the most flips; Unexpected, or a fallback where no candidate scored, names every unit replaying in the part. A named unit stays frozen for the rest of the replay, so each freezes at most once | §3.5's "the sections those name freeze"; naming every unit of the part ends that part's glass replay, as §3.5 says |
| SC70 | Freeze and rebuild (§3.5): at the fallback's tick, the part that fell back and every part whose own nodes' gates changed are rebuilt from the levels as they are, keeping every booking (`Cluster::replace_proc`); a part that falls back in its rebuild names its units in turn. The district's replay ends, with today's fallback (the black box until the district is live again), only when no unit's simulated node runs anywhere or a part falls back with no unit replaying in it. A start whose first regime falls back freezes the units it names and tries again without them, by the same rule. Frozen units and the freeze count are saved with the replay (`IGL3`) | the cluster keeps no snapshot to go back to; a part with nothing left to freeze cannot go on |
| SC71 | Glass or freeze (§3.5, I5): a district replays in glass while some unit may replay (`Compiled::replayable`), and freezes when none may; a fallback still sends it to the black box. A replay whose units' allowance changed since its start (a unit went stale, failed, passed or was cut anew, `Replay::same_units`) is started again at the next sync, and so is one of an older compile: a recompile that keeps every unit (Java recompiles after every edit) would otherwise leave a replay that stops at its next step, so its bounds and timers would never be acted on while it still reported glass. `Report.eligible` reads `replayable`. The units, each with its frozen flag, are in the `Report` since S2d (SC74); the freeze count is not, and none of them is in `GlassStats` (`GlassStats` lives in the globals page, which has no format version); `rtsim-cli glass` shows them (units replaying and frozen, freezes, nodes gated off), and the engine notes each fallback that froze units | as SC54 |
| SC72 | The domain check (§3.6) runs at a same-revision recompile, on each unit it carries: a unit's window records, per domain it is on, the least capacity the solve's running level had over its sweeps (`DomCap`, by the level's selector), kept with its passed window. The recompile finds each domain again by the unit's lowest node position on it and whether that node drives it (a domain's index is its compile's; an anchor two domains of the unit share is dropped). A carried unit goes stale (as an edit in its box makes it) when a domain of its open or passed window is not found again, or when the new compile's stress at the passed level's speed is above the capacity recorded; a domain with stress disabled never fails, one without that level always does. Only the district's own compile: the check across districts on a shared network is S5's (§2.6) | §3.6; Java recompiles after every edit, so the carry is where a load added elsewhere on the network shows |
| SC73 | A chunk loaded anew (`ChunkEvent.Load` on the server thread) is noted as a demoted one is and re-synced at its first FULL report (SC45); `StaleKinetics.chunkFull` takes every kinetic block entity, attached or not. A block entity that has not joined its network (before Create's `initialize`) joins it as its first tick would (`KineticJoin`: `initFromTE` if not yet, then `addSilently`) wherever Create lists it again or takes it out first: at `removeSource` and `setSpeed`, before the speed the join reads changes, at a `setNetwork` to another network, at `remove`, a server-side `read` and `clearKineticInformation`, a generator at `updateGeneratedRotation`, and a powered shaft at its engine's `update` and `remove`, before they change its fields. A join at `clearKineticInformation` outside a `read` that listed the part leaves the network again at once (one already a member stays, as in Create). A join during a `sync` of its network takes its share out of the unloaded share and lists it once the sync ends, unless it left by then, and the network then recounts at once (at a tick when the sync or a change threw); every such change runs even when one throws, and a change that threw recounts the network. Nothing joins off the server (a client, a ponder scene) | in a FULL chunk that does not tick (level 33, around every ticking chunk) nothing attaches, so row 56's re-sync never ran there and a compile read the stale speeds; the re-sync's `add` would leave a part that had not joined counted in the unloaded share too, and its frontier reaches such parts in every FULL chunk around, so a join of the re-synced chunk alone missed some; a removal or a generator's update (a steam engine updates its shaft two blocks away) would leave or list such parts likewise; `sync` iterates `members`, and a part an `onSpeedChanged` changes there must not enter it; a `networkDirty` mark set after a sync is cleared by the tick that ran it; `clearKineticInformation` (a `setBlock` to another axis outside Create's `switchToBlockState`, as `/setblock` gives it) forgets the network without taking the part out. Left open: a generator other than a powered shaft whose generated speed changes before its first tick (a water wheel's flow, a hand crank a deployer turns, a creative motor a computer sets) has its share taken out at its new speed, not the one the network counted (clamped at 0), and so does a `/data` merge that changes a generator's own fields read before Create's `read` (a hand crank's, a windmill bearing's); an addon generator's update during a sync of its own network ends it with Create's own ConcurrentModificationException; if C2ME posts `ChunkEvent.Load` off the server thread, its loads are not noted, and a chunk it loads FULL is re-synced only at its first attach, once it ticks |
| SC74 | The `Report` carries the judging units (§3.4): after the goods, the units' count and per unit (`UNIT_STRIDE` 15) its sections, nodes, box (its sections' node boxes, each grown by one block), open window ticks, windows completed, last verdict, strikes, flags (passed, stale, may replay, frozen, unbounded) and ready tick; no unit with one unit or none. The header grows to 42, slot 41 the `STATION` discard count. Java reads them with `GlassReport.units()` | the GameTests of §9.2 judge each unit; the flat form already crosses the JNI, so no new call |
| SC75 | A station's ports join their units (§4.3, §4.4). At each `IND_STATION_PORTS` the engine gives the glass every set of the district's stations' ports with two inventories or more, numbering the district's revision. The glass keeps them (`DistrictGlass::joins`, saved) and regroups that revision's compile: `Units::build` unions the groups holding one set's inventories (in a section's nodes or its stores), and each unit left unchanged keeps its state, as a recompile adopts it (SC62); a later compile of the revision groups so from the start. A loaded call's `TRAIN_CARGO` (no inventory) maps through the station the train stands at (the engine's arrival for the train): its ports all in one unit, that unit's port at the first of them, once; ports in several units, or in a unit and in none, discard those units' windows (`STATION`); none in a unit, the district's alone. Java journals `IND_STATION_PORTS` before the departure's `TRAIN_CARGO` | discarding the units on every call that reaches two of them would keep a station-fed line from ever passing; the units cannot tell the cargo's split, one unit over them can |
| SC76 | A station's `Discard.OTHER` carries the station (`IND_DISCARD` grows to 40 bytes, the station at 32, 0 for none): the engine discards the district's window and the windows of the units holding the station's last ports, every unit's when they named none. A discard with no station stays the learner's alone, as before. The glass counts it as discard reason `STATION` (8). The discard counts are saved, so the glass page goes to `IGL4` with SC75's joins; `IGL3` pages read as dropped (SC53) | §3.6 |
| SC77 | Edits between districts, the kinetic part (§3.6). Each compile keeps the chunks outside its district that its kinetic networks reach (`GraphBuilder.reachedChunks`): a listed member's chunk, each chunk a member links into (`reachesOut`, which now looks at every member of every network instead of stopping at the first link out), and each loaded chunk outside the district across a face a listed member may turn, though nothing is there yet (a shaft or cog run to the district's border: the first part a player places beyond it is an edit). The compiler indexes them per dimension and chunk, for this session: a compile replaces its district's, the district's removal drops them, and a compile that is not compilable keeps the last ones (an edit there still asks the district to recompile). A network part a player places or breaks in a chunk of no district (`IndustryPlugin.networkPart`: Create's kinetic block entities, chain conveyors among them; a chest or a sign is none) is an `IND_EDIT` (placed or broken, at its position) of every district indexed there: it lies in none of their node boxes, so it stales no unit; it drops their open windows, makes a learned district stale as any edit does (I5), notes the player as one of their builders, and recompiles them. Left open: a chunk a network reaches only after the compile, past the chunks across its members' faces (a shaft run on through a new chunk before the recompile its first edit asks for), is indexed at that recompile; package stores are S4's and the shared network's parts S5's | §3.6; the recompile's domain check (SC72) is where a load added outside the district shows |
| SC78 | `glass.countedStoresMax` (§4.2, §7.3) is per district, not per dimension: past it the district's counted stores are the lowest positions first, and a row cap's cut (SC57) counts only the stores the full cut counts and still shares, so it never counts a store the full cut did not | per dimension, one district's compile would change another's cut, and the cut must stay a function of the compile and the tunables (§3.1) |
| SC79 | A shared store is counted (S3) when it is an item store and every goods edge at it runs to one of §4.2's count points (`COUNTED_MODELS`: chute, andesite and brass funnels, arm, smart chute, packager, frogport, basin); observe edges do not count (SC49). Fluid stores are not counted in S3: they stay in their sections' judging group | the fluid mover's position at `FluidNetwork.tick` is UNVERIFIED; a store a hopper or another mod reaches cannot be counted at all |
| SC80 | `HopperAtASharedVaultFactory` (§9.2) presses its second line's brass into sheets instead of mixing it (no mixing recipe takes brass), and draws from the vault by a smart chute under it, not a funnel; `FrozenNeighbourFactory` is the same two lines without the hopper. `StationPressLineFactory` empties the interface into its vault by a smart chute, not a funnel | the fixture tests the judging group a hopper makes, which needs only a mover that draws from the vault; the corpus' press line is already learned |
| SC81 | `SharedBoilerTwoLinesFactory` (§9.2) stands a millstone line for the crushing line (a crushing wheel needs the item-drop machinery around it), and drives the press through a gearbox off the 128 RPM stage, so the press, its extra presses and the millstone all sit on the boiler's domain. Its edit breaks the press line's input smart chute and places a plain chute there (the node's model changes, so the press section's fingerprint does, and the revision does not; a smart chute placed again with a filter on the only good it moves would change nothing the compile records), and shafts stand from the build where the extra presses go, so a press changes no chunk and the recompile carries the units | the fixture tests two units on one domain and the domain check, which need only two lines that share the boiler's network and nothing else; an edit that changes nothing is cleared by its recompile (SC62), and a new chunk changes the revision, which carries no unit |
| SC82 | `BasinIntoASharedVaultFactory` (§9.2) is `BrassMixingFactory`'s line with its spout chute onto the vault (the build `VaultBetweenMixersFactory` shares), not a basin pouring into the vault: `BasinBlock.canOutputTo` turns a spout sideways only for a belt, a funnel or a block with a `DirectBeltInputBehaviour` below and beside it (a chute, another basin, a drain, a saw, a drill, a millstone, a crafter), and no store has one, so a vault there leaves the spout facing down and the goods in the basin's own output. A basin cannot pour into a vault directly in Create 6.0.10, but it can through a funnel facing up on the vault (SC120, `BasinThroughFunnelFactory`); `BasinIntoASharedVaultFactory` is the chute variant: the move into the vault is counted at the spout chute (`ChuteBlockEntityMixin`), and the row's "two sections, each passes" is checked on that build. The basin's own count point (row 71) is tested on its real hop, basin to chute, with the pair (basin, chute) forced onto the district (`Counting.holdForTest`: every later set keeps it, and the engine skips it in a sample): the basin counts the recipe's output and the chute counts the same goods on into the vault, within what the chute holds. `COUNTED_MODELS` (SC79) names BASIN, but a basin under a mixer or press is a MIXER or PRESS node (`CreateCompile.basinNode`); a modded store with a `DirectBeltInputBehaviour` that a basin fed directly would not be counted until the engine adds those models there, which needs no Java change (the hook is at the basin block) | a basin pouring into a vault directly cannot be built with Create's blocks, but the pour through a funnel can (SC120, `BasinThroughFunnelFactory`); this fixture checks the chute variant |
| SC83 | `VaultBetweenMixersFactory` (§9.2) fills the vault by the corpus' spout chute over it and draws it by the press line's smart chute under it, as SC80 does, so its counted movers are a chute and a smart chute, not two funnels. Create's `handleInput` (the smart chute's draw), `handleDownwardOutput` (the chute's push) and the count's exactness (over 600 ticks the stock changes by what the pairs counted in less out) are checked there; the funnel, arm, frogport, packager and unpacking count points (PATCHES.md rows 64, 65, 68 to 70, 72 to 75) have no fixture, and are checked only by the launch (`defaultRequire 1`: a Create update that moves their call fails the boot). A mixin whose call is still there but no longer reaches the inventory would count nothing, which shows as a residue and a failed window (SC88), not a wrong pass | each needs a build the corpus lacks (an arm or a frogport at a shared store, a packager with a unit on each side); the funnel and the other point's hooks are the same wrap on one call |
| SC84 | An edit that spans a compile resets the units at a shared counted store, and with them Java's counters: `VaultBetweenMixersFactory` first broke the second line's press and placed it 20 ticks later, and a compile between the two (line B without its press) left both units with 0 windows and no verdict, and the pairs with a new `since`, as a changed section fingerprint drops a unit's state (SC62; the vault is held by one section's nodes or by two, and the fingerprints name their stores). The fixture now breaks and places the press in one tick, which is the table's "break and replace": the recompile finds the press as it was and clears the second unit's edit (§3.6), the first keeps its passed window, and the pairs keep their `since`. The second unit is therefore not stale after the recompile, which the fixture did not expect at first. A player who replaces a machine over several ticks resets what the two lines at a shared vault had learned (the lines learn again, and nothing is judged wrong in the meantime) | the observed behaviour of the engine's recompile; the cause (the fingerprint change) is read from the engine, not isolated by a test |
| SC88 | A unit's row at its counted stores (§4.2): interior stock less the ports at its interior inventories, plus its own pairs' moves (in less out), plus each counted store's residue (the store's stock less the ports at its inventory and every pair's moves there) less the residue at the window's first sample. The residue goes to every unit at the store, so a move no pair counted fails each of them on its good, and the district learns as it would with one judging group. Ports naming a counted store's inventory are booked there and are no unit's production. In the bounds a counted store's edge is its mover's unit's, and a drain shortfall at a counted store goes to its takers' units, split by flow share | the identity of §4.1 stays exact for each unit, and a residue cannot be split between the units, so it is charged to all of them |
| SC89 | `IND_COUNT_SET` is an outbox: the glass keeps the (compile, pairs) Java holds per district, marks a district at every change to its units (and at a compile kept across a session) and, at the end of each ingest, sends a set where they differ: the (mover, store) pairs of its counted stores between units (stores whose sections lie in two units or more; one inside one unit needs no count), ascending. An empty set goes when Java holds one and nothing is counted now (no compile, one unit, a compile of the last session's numbers). An ignored resend marks what Java holds unknown, so the set goes again; a new session clears it | a set at every END would repeat every `glass.resendTicks`; a set only at an accepted compile would miss a cut a sweep or a station's ports changed |
| SC90 | Java's count contract: it keys each pair's counters by the mover's and the store's positions, so a pair a later set names again keeps its counts and `since`, and an empty set drops the district's counters. Each sweep's `IND_COUNT_SAMPLE` goes before that sweep's END and is in the node numbers of the latest compile whose END Java sent, leaving out pairs that compile lacks; a pair that counted nothing yet comes with no good, for its `since`. The engine reads a sample only of the compile it holds | a recompile that renumbers the nodes must not restart a line's counters; samples of a compile the engine has not accepted yet name other blocks |
| SC91 | A unit's window opens only at a sweep whose sample holds every pair at its counted stores. Its pairs' `since`, by position, is kept with the window (`UnitState::count_since`): a sweep missing a pair discards the unit's window with warm-up; a pair whose `since` changed (Java forgot it) starts the window again from that sweep, with no warm-up. A unit with no counted store is unaffected | a missing sample leaves the unit's moves unknown for that sweep; a restart of Java's counters is a new base, not a fault of the line |
| SC92 | `IGL5`: each unit's state adds its pairs' `since` and its residue bases, and the page's cut keys add a hash of the counted stores, so a changed `glass.countedStoresMax` drops the district's glass state as a changed `sectionsMax` does (SC53); `IGL4` pages read as dropped | a window's base must survive a save for the window to go on |
| SC93 | The counting stats (samples read, sets sent) are `CountStats`, in memory only, not in `GlassStats`: the globals page has no format version, so a field added there fails the whole industry restore | as SC71 |
| SC94 | Movers at one block share Java's counters, which are keyed by position: a packager is two nodes (packing and unpacking) at its block, and Java names the same counts under both pairs. The engine takes each direction once per (block, store): into the store from the lowest pair with an edge that way, out of it likewise, else the lowest pair (`Units::pair_dirs`) | counting both would double the packager's moves in its unit and in the store's residue; Java cannot tell which node a move was |
| SC95 | The Create compile claims each portable storage interface with no node (§4.4) when its conveying walk (`TrainWatch.reached`, the walk of `IND_STATION_PORTS`) reaches a district inventory; one that reaches none stays residual | it exposes a docked train's storage, whose change is the station's cargo, booked through `IND_STATION_PORTS`; left to the residual, the mover beside it reached a black box, so every station-fed unit was unbounded and unbooked cargo could not fail its window. An interface that reaches no inventory (onto a belt into a machine) has no port to book its cargo on: claimed, what it fed would rise against a prediction of none and fail every window, so it keeps the black box |
| SC96 | The section fixtures (§9.2, `SectionsFxGameTests`) are opt-in: `./gametest.sh create -Psec1fx` adds their namespace to the create run | each is a full corpus cycle of several windows; on by default they lengthen every create run on the shared heavy lock, for every session |
| SC97 | A package store's frogport packages are a new journaled record, `IND_CHAIN_PACKAGE` (0x0419; directions `CHAIN_OUT` 3, `CHAIN_IN` 4, later `CHAIN_HELD` 5), not `IND_PACKAGE` with new directions (SC7's record); it carries the store, the frogport's index in the store and the destination (§2.5) | the engine books it into a store, not a district's mail path, and the destination (SC100) is a field `IND_PACKAGE` does not have; one record kind per path leaves the mail path as it is |
| SC98 | `IndustryData` format 3 drops an older format's districts on load and journals an `IND_DISTRICT_REMOVED` for each at the session's start, instead of resetting the engine; exclusions, merges, patches and the id counter stay (§6) | the removals reach Rust by the path a dissolved district already takes, need no host hook, and leave the engine's other state alone; new districts never reuse a dropped id |
| SC99 | A store's census every 200 ticks (TUNABLE, `PackageStores.CENSUS_TICKS`) while every chunk of it ticks, as a latest-wins record of its own (`IND_STORE_CENSUS`, 0x0051), not with a district's sweep | a store belongs to no district, so no sweep carries it; one walk and count a store every 10 s bounds the cost (D1) and keeps the residue fresh enough for `/rtsim district sections` |
| SC100 | Java resolves a chain package's destination: the matching frogport of the least distance (hops, plus 1000 for a `*` filter), ties to the lowest position, then the first matching frogport on a least-hop way there; the record carries it and Rust books by it | Java has the walk and Create's own `matchAddress`; routing in Rust would need the filters and a glob matcher kept in step with Create's |
| SC101 | A store's frogports are found by a scan of the loaded chunks of its walk's conveyors and their neighbours, from each district's compile and from a package at a frogport of no known store (at most once in 1200 ticks per frogport, TUNABLE) | a frogport stands within `packagePortRange` (5 blocks) of its conveyor; the scan reads loaded chunks only (§2.8), and the compile already walks the chain |
| SC102 | A network of more than 200 frogports (TUNABLE, `PackageStores.MAX_PORTS`) is no store: it keeps its `CHAIN` edges, with one warning | its `IND_PACKAGE_STORE` must fit one record (64 KiB: 48 bytes a frogport, 2 a pair) |
| SC103 | A frogport in a chunk of no district counts as an owner of its own, so a chain from a district to frogports outside every district is a store | packages the district sends there leave it, and as edges they would lead to no node |
| SC104 | A walk that leaves frogports behind (they now serve another network) walks their network at once, at most 4 deep; a later census or compile carries on past that | each piece of a split gets its store at once, at a bounded cost per call |
| SC105 | The chunks of no district a store's chain passes through (§2.5, §3.6) wait for a later slice; S4a keeps none | nothing in S4a reads them; §3.6's staleness is their first user |
| SC106 | A store is declared again at its first walk after a restart, and every store at once when the engine forgot what it was told (`PackageStores.announce`, run with the districts' announce) | Java keeps no copy of the last declaration across a restart; the engine keeps its stores in pages, rolled back with them (P29), so a repeat changes nothing there. A store whose saved hops do not fit its frogports keeps its id with unknown hops until the next walk |
| SC107 | A district with chain links runs the black box, as one with mail links does; replaying a chain port in the glass box is left for a follow-up | the glass box has no port for a link that moves goods out of the district's window; the black box already replays links at their learned rates |
| SC108 | `LinkKey::Chain` (inventory, destination frogport, good) is learned only while the district is loaded and not replaying: a `CHAIN_OUT` toward a frogport in a district books it negative, a `CHAIN_IN` with no op id positive; a `CHAIN_IN` an IND_WRITE_CHAIN wrote (op id set) is a port, never a link | a write is the replay's own draw coming back; learning from it would count the draw twice. An export to a frogport of no district has no importer to replay it |
| SC109 | A cluster pool `PoolKey::Chain` (store, destination frogport, good), save tag 4, starts from what the store owes that frogport and goes back, when the cluster stops, to the store that lists the frogport now; with none, to its own store (kept as its `left` pools are); with that gone too, it is lost and counted (`chain_pools_lost`) | the store's owed count is the one ledger both ends see while loaded; a pool that outlived its store has nowhere else to go, and UR-1 lets a reset lose production owed to unloaded chunks |
| SC110 | The export's room is a target: `chain_room_per_conveyor` (20, TUNABLE) times the conveyors from the exporter to the destination (hops plus 1), less the real packages the last census counted toward it; the link stops there and starts again one batch below. The batch is the frogport's last `CHAIN_OUT`, else the good's stack | §2.5's room estimate, in whole batches as Create's own exports leave; the restart gap keeps a full pool from toggling every step |
| SC111 | The import link draws from the pool into stock; after it stops on an empty pool it restarts no earlier than the hops to the store's farthest frogport times `chain_ticks_per_hop` (60, TUNABLE). A replay's chain links are those of its part's (unloaded) inventories, so a loaded frogport of a district replayed in part is a port and is written, never drawn by the replay | an ESTIMATE that needs no chain speed: a replay does not know the RPM of an unloaded chain, and the farthest hop bounds every route of the store |
| SC112 | A replay that starts joins the clusters that hold its district's chain pools; past `cluster_districts_max` (16) it drops its chain links instead (`chain_links_dropped`, logged once for each store) | one cluster per store keeps both ends on one pool; the cap bounds a cluster's step cost (D1), and a district without its links replays as one that does not replay the store |
| SC113 | IND_WRITE_CHAIN (0x8409): while a frogport is loaded and owed, or a cluster holds a pool toward it, one command for each good every `chain_write_ticks` (200, TUNABLE), of min(whole batches, 18) packages, or one part package when no cluster holds the pool; none while an earlier write there is unanswered (one in the engine's outbox always holds the next; an answer missing 10 periods after the drain no longer does; forgotten after 100 periods at the next look toward a loaded, owed or pooled frogport, at most 8 kept, `chain_writes_unanswered`). Java writes what fits, one slot kept free, addressed to the frogport's filter, and journals one `CHAIN_IN` with the op id, which the chunk carries with the COMMAND_RESULT until acknowledged and Replay takes once by op; the store marks each frogport's last written record (game time, ops) and drops a written record no later than the mark (a chunk saved before the acknowledgement journals it again after Replay forgot the op), looked up by the frogport's position, which the record carries (an index shifts when a store is declared again), in the record's store and the store of its dimension that lists the position; a record of a write the engine still waits for is that write's answer, taken at the write's frogport and never judged by the mark (a crash moves the clock back behind it). The engine takes it from the pool, then from owed, never below zero. FAILED moved nothing, answers the write, and the engine writes again | real packages written into a loaded frogport keep UR-1 (Rust never holds the only copy of a package that exists in the world); carrying the record in the chunk covers a chunk saved before the journal reaches disk |
| SC120 | A basin that pours through a funnel into a store is counted at the funnel: `BasinBlockEntityMixin` finds no item handler at the spout block, takes the block's `InvManipulationBehaviour` and counts (funnel position, `getTarget().getConnectedPos()`), the pair the engine names from the funnel's adjacency edge (`CreateCompile.funnelEdges`). The basin's own pair is kept for a spout block that offers an item handler. `BasinThroughFunnelFactory` (`sec1fx_basin_funnel_vault`) is the basin line of `BasinIntoASharedVaultFactory` with an andesite funnel facing up on the vault where that fixture has the chute | review B1 found the pour uncounted (the basin skips the behaviour's own count point), leaving residue at the vault and false failures at the line that draws from it; counting under the funnel is what the engine's pair asks for and needs no engine change |
| SC121 | A repackager's moves are counted: `RepackagerBlockEntityMixin` wraps the four `extractItem` calls of `attemptToRepackage` (the real removals count out) and the `insertItem` of `unwrapBox` (counts in), as `PackagerBlockEntityMixin` and `DefaultUnpackingHandlerMixin` do for a packager. Not tested by a fixture (SC83) | the engine compiles a repackager as a packager, so it names pairs (repackager, store); the class overrides both methods the packager hooks without calling `super`, so its moves went uncounted and left residue at a shared store (review S1). The alternative, compiling a repackager to a model the engine does not count, would put the store back into group judgement for a block that is common in package lines |
| SC122 | A district on a moving frame (`end.frame != 0`) has no counted stores: `Sections::compute_counted` returns none, so every cut (`cut_counted`, the page's and the check's) judges its shared stores as in S2, one group per shared store | Java ignores a count set on a moving frame and never samples one (`num.frame != 0`), so a store counted there had no cover, its units were discarded at every sweep and never judged (a regression against S2, where the district was judged), and the engine resent a set Java ignored at every END resend. Doing it in `compute_counted` keeps the units, the page and the persisted cut on the same rule |
| SC123 | A count sample's records are cut only between (mover node, store node) pairs (`Counting.cuts`), not every 1637 entries; one pair with more goods than a record holds is split, and may lose part of itself with a lost record | a record lost on a full outbound ring then leaves a pair missing, which the engine reads as no cover and discards the sweep's unit, where a pair cut in two kept its `since` and read the goods in the lost half as not moved (a false residue and a window that could fail) (review S3). The split case is reachable: a key keeps every good ever moved through its pair and never prunes it, and goods differing in components (damaged gear, Destroy mixtures, enchanted books) count apart, so a long-lived pair can pass the 1,600 or so entries one record holds. So each entry of a pair carries the pair's total entry count (the reserved u32 of Count40, `E_ENTRIES`), and the engine gives no cover to a pair it got fewer entries of than that (`CountTake::since_of`), where it would read the goods of a lost part as not moved. Pruning a good's counters at a snapshot is not done: it would shift the cumulative counts the open window's residue base was taken from and read as a residue, and a counter restarts only with its key (`since`) |
| SC124 | The review's nits, taken together: a resend END marks the count set unknown only for a district that was sent one (no empty `IND_COUNT_SET` to every district per resend); `session_start` and a numbers reset clear the last count sample; each count point is guarded (`Counting.isMover` and `moved` catch, and each hook's whole body, its argument evaluation included, sits in a try that hands the exception to `Counting.failed`, which logs the first failure and each doubling; the move goes uncounted: Create's tick is never broken); a set that leaves no pair drops its `Held` so `Counting.active()` goes null; a malformed set (negative or too large a count, a header too short, none of its nodes in the compile) is counted in `setsIgnored` and leaves the counters; the insert hooks count a copy of the stack taken before the original call, and only when the block is a counted mover and the call is not a simulation (the test comes first, so an uncounted block pays no copy). The arm hooks (rows 69, 70) are documented as mattering for third-party store blocks only: no point of Create's own lands on a store node. Not done: the CLI report's use of the default `counted_stores_max` (nit 7), the test helper's stale index (5), the per-move canonical-inventory cost (9) | each is a robustness or tidiness change with no effect on a correct set or move; the three left out change nothing a player or a verdict sees |
| SC125 | A mechanical arm's insert at a funnel point is counted under the arm, not the funnel: `ArmFunnelPointMixin` (row 81) names the arm for the call, and the funnel's count point counts under `Counting.moverOf` | the compile models an arm putting into a funnel on a store as an ARM edge from the arm to the store, with the funnel a junction whose one edge is its adjacency to the store; with nothing else feeding it the funnel is a section of its own, the store is counted between the two, and a count under the funnel put the arm's goods in the funnel's unit: neither unit balanced and the district never replayed (`glassreplay.arm_burners_depots` on the S3 landing round). Changing the compile to an ARM edge into the funnel would change the solve's arm rule for every such line |
| SC126 | S4p's text is docs/boundary.md §3.2 "Generations and copies", under §3 (the journal), and the code's references name it there | no §2.5 exists on any branch; generations are the journal's, not a record kind's |
| SC127 | `rtsim-host` names each session by a hash of the world save it opens (`java::nonce_of`), not a random nonce | forked host runs must write identical pages (the lockstep test compares them byte for byte), and the host has no holders, so nothing ever judges a copy by its nonce there |
| SC128 | A session whose first record past the pages is not a JOURNAL_GEN opens an unnamed generation (`UNNAMED_GEN` with the session's first sequence) and logs it | a bridge that names no generation must still close the one before where it ended, or its sequences (a crash hands them out again) would read as that generation's |
| SC129 | A record too large to wrap (32 bytes plus the record above `MAX_RECORD`) is journaled again plainly, as before generations, and counted (`/rtsim status`, "too large to copy"); a holder's records carry their generation in `PendingRecords` entries and in `EpochEntry.gen` (0 when the record has none: always new) | only an IND_RECONCILE_RESULT within 32 bytes of the record limit gets there (thousands of entries or broken blocks in one chunk); its plain recommit is judged as before S4p, by its op id, and losing the copy's protection there is better than a record that cannot be journaled |
| SC130 | Java's journal keeps the newest acknowledged JOURNAL_GEN outside its entries (`Journal.pinnedGen`, NBT key `pinnedGen`, the host's `WorldSave.pinned_gen`), checked at load (a JOURNAL_GEN below the first entry, or the journal is corrupt), sent before them, not counted in `size`, `bytes` or `firstSeq` | in the entries it would move `JOURNAL_FIRST_OBS_SEQ` below a real gap (lost observations reported as in sync), and a flag in the saved list could not tell it from an unacknowledged JOURNAL_GEN; the stub now forgets its transfer and command op ids at WORLD_SAVED (its event op counts stay, a test counter), refuses pages from before (magic `RTS2`), and opens the unnamed generation as the engine does, and `appliedCmdOps` counts the entries holding each op (the review's nits) |
| SC135 | The frontier keeps a route entry on a conveyor just before the tick that would expire it (timeout 1 or less, `Frontier.beforeRoutes` before `ChainConveyorRoutingTable.tick`, row 80) when its connection is still the conveyor's, its next conveyor does not tick and its port is the filter of a frogport of the conveyor's store whose chunk does not tick: the entry gets Create's full timeout (100) and is kept with its port, the distance it had, its connection and its store; each conveyor with kept routes inserts them again every `KEEP_TICKS` (TUNABLE, 20; a turn by game time and position) through Create's private `insert` (row 79), and the kept routes are saved with the frontier (SC139); one goes when its store, its connection or every far frogport with its filter is gone. When its next conveyor ticks again (the boundary moved out) it moves on along the chain, one hop nearer per conveyor, while each conveyor has one way on (but the way back) that ticks, and is kept and inserted on the first whose way on does not tick (`Frontier.handOver`, at most `HAND_OVER_MAX`, TUNABLE, 64); it goes instead when a conveyor on the way has a route of its port already, the way forks or ends, or it reaches the port's own conveyor | §2.5 named the store's saved walk for the distance, but the walk only has hops between frogports' conveyors, none from a boundary conveyor; Create's own distance is the one its adverts would have kept; without the hand-over a boundary that moves out loses the route (the new boundary conveyor has no entry to advertise) and the packages behind it circle unrouted. Limits: a far side that stopped ticking before any capture (no route to it ever reached the boundary conveyor) gets no kept route until it ticks once; a route through a chunk that does not tick to a frogport that does is not kept; a package already on the connection when the route expired waits unkeyed; a far port is the store's first frogport with the filter whose chunk does not tick (`awayPort`), so two far frogports of one filter are held toward the first; the hand-over follows linear chains only (a fork drops the route, and its packages circle unrouted until the far side ticks); a far frogport whose own chunk does not tick but whose conveyor's does gets no kept route (its port's entry ends on a conveyor that ticks), so its packages circle unrouted until it ticks; a conveyor's table is read only at the tick its soonest entry may expire (`scanAt`, a field the mixin adds to each conveyor, row 80: Create lowers a timeout by one a tick, and a new or refreshed entry starts at 100) and, for the insert, on its turn, so any other conveyor tick costs a modulo and two compares |
| SC136 | A held package's key is `{store, epoch, serial}` under `mic_rtsim_held` in vanilla `minecraft:custom_data`; the epoch is the frontier's, random and nonzero at first, raised by one (skipping 0) at every industry announce (`PackageStores.announce`: an engine whose pages lack what the journal dropped, a sim reset, or the industry switched on again), and sent in each IND_PACKAGE_STORE (at 36) and in a keyed IND_CHAIN_PACKAGE (epoch at 84, serial at 88; 0 at 84 is no key); a serial's high 32 bits are random per session; the engine compares epochs for equality only and adopts the declaration's | §2.5 said "the engine's epoch", but Java writes the keys, so Java owns it and the engine learns it; an epoch counted up from 1 would meet old keys again after an engine reset that also lost the plugin's state, and serials handed out again after a crash before a save would match keys still in the world |
| SC137 | The held packages are listed twice: the engine's store keeps them toward each frogport, oldest first (`PackageStore::on_hold`, store pages IST3), booked in the pools like any package; Java's frontier keeps each by serial (store, dimension, destination, conveyor) in its saved state. Java follows a held key and tells it again (`CHAIN_HELD`, the same key) when its store or destination changed and once in each session; the engine then moves it, keeping its place (`held_again`; across stores too), and turns away only a hold with no key or of another epoch than the last declaration's (`held_stale`). Java moves its entries as the engine moves its own: with a frogport that changes store (`from`), into the store a removal names, forgotten with a removal into none. A key leaves on both sides: a keyed `CHAIN_IN` (`held_landed`) or a `CHAIN_RELEASED` (direction 6, no frogport, destination, packages or goods; `held_released`) strikes it wherever it is held; Java releases a key when Create drops its package (`ChainConveyorBlockEntity.drop`, row 80), a player takes it off the chain (row 82), a frogport takes it but no `CHAIN_IN` carried the key (a partial insert, a frogport of no store), the hold finds a key of this epoch Java does not hold, or a census's complete walk did not find a key held for the store or last held on one of its conveyors (strays of this epoch it finds are stripped and released); a census no longer clears anything; a declaration of another epoch drops the keys of any other | S4c2's draw needs both sides to agree on which packages the far district may take; the engine never decides a key is Java's on its own. A census that cleared the held lists would let a store that ticks whole, for a moment, forget packages still bound for a frogport that stops ticking again; Java's walk sees each package, so it is the one that can tell which left. Telling a key again once per session covers a crash that kept the key in its chunk and in `IndustryData` but lost the record. The other way round can happen too: `IndustryData` is a plain SavedData written at every save, so it can be a save ahead of the journal's held-back file (SC138), and a chunk saved at unload can be ahead of both; the held list and the first-tick look (SC140) cover that. Limits: a package that leaves some way no hook sees (a contraption moves its conveyor, a command) stays held until its store's census, which needs all of the store to tick; the package dropped keeps nothing of rtsim's but where the census strips a stray |
| SC138 | The hold is at the S1 guard's refusal (`ChainConveyorBlockEntityMixin.exitTicks`, row 80): a package whose route (the first entry `PackageItem.matchAddress` takes) is a far frogport of the conveyor's store is keyed once and journaled as `CHAIN_HELD` (frogport and destination the far frogport's index, position the boundary conveyor, district none, op none), the conveyor's chunk noted for the journal barrier (`Industry.journalDependsOn`, now public: the journal's next file waits for that chunk, R21); a package held already, here and told this session, costs one key read; one held but with no route here to a far frogport stays held toward its frogport while that does not tick, and is released otherwise; any other key is stripped (and released when of this epoch). Keys are stripped where a frogport takes the package (`FrogportChainMixin`, row 78, its `CHAIN_IN` carrying the key), where Create drops it or a player takes it (and released, SC137), and where a census finds a key Java does not hold. They are not stripped at `CHAIN_OUT` | the key must travel with the package for S4c2 (a drawn package that lands another way is struck, §2.5); a chunk saved ahead of the journal is the order §3.1 allows, so the barrier makes the record reach disk no earlier than the key. Keys are stripped whether or not the industry is on (UR-1: a package keeps no stray tag); a strip while it is off releases nothing, and the next session's held list and first-tick look (SC140) clear what it left. Where a frogport's insert leaves something over, Create drops the original package rather than the copy it offered, so the original is stripped too (row 78). Limits: a chunk saved with a new key whose record and `IndustryData` entry a crash both lost carries a key Java does not hold, so its next hold strips it and keys the package again (a new serial, one more CHAIN_HELD; the lost record never reached the engine); one whose `IndustryData` entry was saved but whose record was lost is told again at its next hold, once per session (SC137) |
| SC139 | The frontier's state is its own `IndustryData` plugin entry (`mic_rtsim_create:frontier`: the epoch, the kept routes as one list, the held packages one tag each by serial), updated in place at each change, apart from the package stores' own state | a hold is one tag written, not every store's state; one plugin per mod (D4) still holds, as both entries are `mic_rtsim_create`'s |
| SC140 | Once in each session, at the first package-store tick (`PackageStores.tick`, `Frontier.listOnce`), Java journals one IND_HELD_LIST (0x041B: the frontier's epoch and the serials it holds, ascending), and the engine strikes from each store of that epoch every held key the list does not name (`PackageStore::keep_listed`, counted `held_unlisted`; stores of another epoch keep theirs). The first tick of each conveyor in the session (`scanAt` 0) looks for the packages loaded from `IndustryData` as last held on it that no hook has seen this session (`Frontier.firstTick`): one it does not carry, looping or on a connection, is released (`CHAIN_RELEASED`, counted `unfound`). The engine also checks a told-again hold against its new store before striking it from the old one (`PackageStore::takes_hold`), so a hold turned away leaves the key where it was | the second review found that a crash can leave the engine holding a key that neither Java nor any package has (a landing's keyed `CHAIN_IN` in the held-back journal file while `IndustryData` and the frogport's chunk were saved), and Java holding one no package has (a chunk saved at unload after a player took the package, then a crash before its release and `IndustryData` reached disk); S4c2's draw would take such a ghost, and only a whole-store census would clear it, which needs the far side to tick. Java writes the keys, so its list is the one that can say; the first tick is where the conveyor's own saved packages can be compared with Java's. Limits: one list a session of at most `HeldList.MAX_SERIALS` (8,187) serials; past that none is sent (logged) and only releases and censuses clear the engine's extras; a frontier made again within a session (the industry switched off and on) does not look on conveyors that tick already, but its announce raises the epoch anyway; a package that left its boundary conveyor on the way on unseen is released there and keyed again where it is held next |
| SC141 | The draw (slice S4c2): at a store's write wake (`Engine::store_write_wake`), for each frogport a replay covers, the packages held toward it with a good in some cluster's pool toward it are asked drawn, each key once a session (`draws_asked`, cleared at the session start), by IND_HELD_DRAW (0x840B: store u64 at 16, epoch u32 at 24, action u32 at 28, Draw 0, Remove 1, Return 2; serials u64 from 32, aux n, at most (0xFFF8 - 32) / 8 a record; no op id, no COMMAND_RESULT; 0x840A left free). Java (`Frontier.heldDraw`) marks the key drawn (its held tag's `drawn`) and journals `CHAIN_DRAWN` (IND_CHAIN_PACKAGE direction 7, key only, at the boundary conveyor), again for a key drawn already, none while the far frogport ticks (a declined Draw: Java forgets that it told the key, so its next hold tells it again and the engine, at that `CHAIN_HELD`, may ask again). The engine books the draw at that record (`Engine::chain_drawn`): the package moves from `PackageStore::on_hold` to `PackageStore::drawn` (store pages IST4) and its goods into the cluster's pool toward the frogport, else the store's owed. Every hand-off of a drawn package is refused (`Frontier.refusesDrawn`, first in the `exitTicks` wrap, PATCHES.md row 51; a count of drawn packages keeps the check to one field read while there are none), so it circles its boundary conveyor, and the store census does not count it (its goods are the replay's until it is removed or given back) | Java's mark comes first, so the goods move to the replay only once the package is pinned in the world (UR-1: the package is still there, only refused); asking once a session bounds the commands; the key-only layout is the release's; a `CHAIN_DRAWN` told again is harmless (the engine holds it drawn already) and covers one a crash lost |
| SC142 | The removal: at a store's write wake (`Engine::remove_drawn`), a drawn package whose drawing district replays no more and has no reconcile pending, toward a frogport the store owes nothing, writes nothing to, and toward which no cluster holds a pool (what the replay did not use is written in first), is asked removed (Remove, once a session, `removes_asked`). Java takes it off its conveyor (the looping or a travelling list, then `notifyUpdate`) while the conveyor's chunk is loaded, else marks it `removing` (saved) and takes it at that conveyor's first tick once it loads (`Frontier.firstTick`, after SC140's look); then journals `CHAIN_RELEASED`, the chunk noted for the journal barrier (R21). The engine strikes it (`drawn_removed`). A Remove of a key Java holds but not drawn removes it all the same (the engine drew it: Java's mark was lost); a package not on its conveyor is released (`unfound`) | the replay's results are then in the world, so the package's goods have reached it; the barrier keeps the journal from claiming a removal the chunk on disk lacks; a chunk saved without the package and a crash before the release reached the journal is SC140's case (released at the first tick) |
| SC143 | Giving back and restarts: a drawn package whose drawing district is gone, and a `CHAIN_DRAWN` of a key the engine does not hold or whose frogport no replay pool covers now (it loaded meanwhile), are given back (Return): Java clears the mark and forgets that it told the key this session, so the next refused hand-off tells it again (`CHAIN_HELD`); the engine's own drawn package goes back into its pools (`undraw`). IND_HELD_LIST carries the drawn keys last (u32 m at 28, the last m serials; each part ascending). The engine asks Draw again for a key it drew that Java lists as held or tells again as held (a `CHAIN_HELD` of a key it holds drawn), and takes a key Java lists drawn that it only holds as that key's `CHAIN_DRAWN` (booked when a replay still covers it, else given back). A drawn key that lands (a keyed `CHAIN_IN`) goes back first into the pool of the frogport it landed at (in the store it landed in) and then lands there (into no pool when a census counted the landing already, that census counting no drawn package); one released (a census, a drop, a player) is struck; a drawn key of another epoch goes back into the pools at the declaration. Known limits, each a duplicate at worst: a crash after `IndustryData` saved a removal but before the chunk did brings the package back with a key Java no longer holds (stripped and held again); a player who takes a drawn package keeps goods the replay also has; a store removed with no heir drops its drawn packages in the engine, and the packages go on | UR-1 prefers a duplicate to a loss; the held list makes the marks agree after either side lost some, as SC140 does for the keys |
| SC144 | Slice S5-1: every kinetic domain lists every source Create lists on its network (`Source32`, docs/industry-boundary.md §9), ascending by position, with its ratio to the reference. An engine shaft on a tank the compile has a boiler node for is a driven source: 19 rows, boiler level 0 to 18 (20 since SC154: the passive row), each its own speed and its capacity at that speed; any other source is fixed at one row, its speed now. The domain's speed rows are the nonzero source row speeds over their ratios, distinct; rates index them. In the solve each driven source takes the row of the largest selector at or below its boiler's level; the fastest source sets the speed, every source adds its row's capacity, a slower one included, and the domain runs while that carries its members' stress at that speed (or always with stress disabled). `singleBoiler` goes: each tank drives its own shafts, so engines on several tanks are modelled. The LP's possible rows are an upper bound: a row some allowed source row gives where every source has an allowed row no faster and of no other sign, and the summed most capacity carries the stress at the row's speed, matching within twice the solve's rounding so it never drops a row the solve reaches. An engine's shaft and tank are read without loading a chunk; a speed row needs capacity too (a capacity of 0 in the config gives none, as `getGeneratedSpeed`) | Create's own rules (§2.6): `PoweredShaftBlockEntity.getGeneratedSpeed`, `GeneratingKineticBlockEntity.applyNewSpeed`; the old ladder modelled one tank and took the network as one fixed level otherwise, which kept running when the boiler stopped; rows by speed keep rate tables at a few rows, not 19 |
| SC145 | A domain Java cannot model has every source fixed and one speed row, the reference's speed now, and its members bounded: a crossing network S5-2 cannot share (SC153), more than 32 speed rows, sources whose speeds over their ratios disagree in sign, a driven source whose ratio is unknown (a powered shaft on a stopped network), a tank whose engines turn shafts on two networks of the compile, an engine's shaft whose tank or engine the district does not model (outside it, or not loaded), or a speed or quotient that does not fit an i32 over a u32 exactly. A shaft that two engines could power takes the tank of the engine Create powers it from (`enginePos`), else the first seen. A network listing two block entities at one position (inside a deferred join) gives one source, the one there now. In the solve, sources that disagree in sign stop the domain; a driven domain without a row for a speed one of its sources gives is refused by `Graph::build` (Java builds every one), so the solve's row is always that speed's, within rounding. A chute hand-off whose arm's domain has no current row gets no rate | the safe side; Create breaks a block where speeds disagree (`RotationPropagator`); a missing row was a fallback the LP bound did not cover (the S5-1 review) |
| SC146 | The domain check (§3.6) keeps the least capacity a window ran at per speed it ran at (a faster row may have more capacity and fail first), keyed by the own speed of the domain's lowest source with a known ratio, by position, else by the reference's speed; a recompile without that source counts as past the capacity (the unit goes stale). The capacity is the solve's sum over the sources at the solved state | a root change moves the reference (SC44), a source's own speed does not; unknown is the safe side |
| SC147 | Glass pages go to IGL6 (the solve's per-source rows and capacities, the domain check's keys); an IGL5 page is read as dropped and its district learns again | the format changed; no compatibility before a release |
| SC148 | The away-source check (§2.6) extends S1's `aNetworkRecountKeepsAnUnloadedSource` instead of a new `aNetworkWhoseSourceChunkUnloads`: with B (the motor) away, millstone A turns at 64 RPM, is not overstressed and the network keeps the motor's capacity, at once, 600 ticks on and after the recount | that test already builds the layout (a motor 13 chunks away, the chunks between held below FULL, a watch that no chunk two or more from A loads); one fewer 16,000-tick test |
| SC151 | While a replay's cycle runs, a part publishes for each driven source the least row over the cycle's marks (least by speed, then capacity; none when a mark has none); outside a cycle, its running regime's row | a cycle repeats, so readers following each mark would re-run at every mark and never settle; the least row is the safe side. Cost: where an owner's boiler level changes within its cycle (a burner a controller lights and puts out), its readers run on the cycle's slowest row all trip, slower than the average Create would give. Option (a) of two, decided by the coordinator (reversible); (b), following each mark, may never settle |
| SC152 | Refines SC28 for shared networks: an away owner that is frozen, gated (its unit does not replay) or not replaying on glass (the black box, none) gives its network no driven rows, as if its boilers were cold; its fixed sources always count, at their one row; a loaded owner gives its check solve's rows. Inside a district a gated boiler still stops its whole domain (SC68) | the safe side: nothing runs on an engine the sim does not trust; a fixed source (a creative motor, a water wheel) needs no model |
| SC153 | Slice S5-2: a crossing network is a SHARED part in each district's compile (§2.6), its parts joined in Rust by shared source positions (`glass/net.rs`, derived from the compiled graphs, never saved), in each part's frame through the network's anchor. A coupled network's part solves with what the rest gives (`NetInput` per domain, in the regime and its key). One cluster per district: an owner's change settles each reader to that tick and re-runs its parts, up to `NET_ROUNDS` (8) rounds a tick, then oscillating. An incomplete network or one past `industry.clusterDistrictsMax` parts runs decoupled: each part at its compile row, its members bounded (why `SHARED_NETWORK`, 20). After each rebuild the domain check runs across districts (§3.6): a unit whose passed capacity is below its own stress plus the others' at that speed goes stale. Glass pages go to IGL7 (the regimes' and replays' network inputs and published rows); an IGL6 page is read as dropped | one cluster per district keeps the costs S4 bounds and needs no merge for a network (plan review, fix 3); a part per compile keeps each compile reading only its own chunks (S1); positions identify a network where Create's id changes when another source takes over; no compatibility before a release |
| SC154 | A boiler with water and size but no heat (its burners out: a blaze burner below FADING) is passive, as Create's `BoilerData.isPassive`: its engines run at an efficiency of 1/8 shared by its engines (16 RPM, 128 SU per RPM over the engine count). Each driven source has a 20th row (selector 19, `PASSIVE_SELECTOR`), which the solve takes when every heater under the boiler is out and it has water and size; Java's current row is 19 when Create's boiler is passive | Create keeps a line turning slowly on a passive boiler (`BoilerHeaters.blazeBurner` gives passive heat below FADING); without the row, a burner out stopped the line in the model. Limits: heaters Create counts as passive by tag (a campfire, lava, magma) are not heaters in the model, so a boiler on them alone is not passive there: Java's current row says passive (19), the solve says stopped, and the unit's windows can be expected to fail where its counts move (§14) |
| SC155 | `IndustryData` format 4 (slice S6) drops the districts as format 3 does (SC98) and keeps the plugins' state when the old format is 3 or newer (`PLUGINS_KEPT_SINCE`): the package stores' ids and frogports | the engine keeps its package stores in pages through `IND_DISTRICT_REMOVED` (SC143); dropping Java's would number new stores from 1 against stores the engine keeps |
| SC156 | A station's served chunk (J6) is saved per dimension in `IndustryData` (`stationJoins`: the station's position to the chunk of the served district's lowest inventory), not among a district's pairs. A chunk with several stations goes by the one at the lowest position (`BlockPos.asLong`) that has an entry (`Districts.servedByChunk`). The commit moves each group of member chunks with no seed that holds a station's chunk to the district holding its served chunk, else to the lowest district id 8-adjacent to its station chunk or touched by its station, else it is no district; the districts here are the seeded ones, so one station group never joins another. An entry goes when a detection looked at either chunk and it is in no district, and only an entry the detection began with (one a departure saved meanwhile waits for the next detection) | before the attach the two chunks lie in different districts or none, so the pair has no district to live in. Keyed by station chunk, two stations of one chunk serving two districts overwrote each other at every departure and moved the chunk to and fro (each move a redeclare and a glass recompile); keyed by station with the lowest one deciding, the chunk stays put and the other station's ports go unlearned (§14) |
| SC157 | Join pairs are replaced at chunk grain: a saved pair goes when the walk walked both its chunks, unless either holds a block whose links are not known yet (`IndustryPlugin.linksKnown`: an arm with saved points it has not read, PATCHES.md row 84), whose chunk keeps every saved pair it is in and whose walk reaches their other chunks. A loaded chunk with a saved pair to a member that is not loaded roots the walk (`Rescan.joinedToAway`). Each district keeps the pairs whose two chunks it holds | SC5's rule: what the walk did not see stays as it was last seen, and an arm's empty lists before its first tick are not a sighting |
| SC158 | A station serves the district holding the most of the inventories its docked interfaces reach (ties to the lowest id). `IND_STATION_PORTS` still names the station's own district and its inventories only, where its cargo is booked | the engine matches a call's cargo against the ports of the district its arrival names (the station's chunk's), so a record for another district would join that district's units for cargo that never comes there (SC75) and leave the station's own exchange without ports; after the attach the two are one |
| SC159 | A touch of a station or a chain conveyor is no touch: it joins nothing and gives no crossing code (`notLearnable` at a split); a station's touch names J6's candidates. Test districts (`addChunkForTest`) join a new chunk to an 8-adjacent member, else to the district's anchor chunk | J1's exceptions; a crossing code describes a join the operator cut, and these never join. The test helper keeps the fixtures' districts whole now adjacency joins nothing |

---

## 14. Risks and unverified points

- **UNVERIFIED, each the first GameTest of its slice:** whether Create saves its routing tables, which decides how
  much the kept routes do after a restart (§2.5, S4); and that a network's loaded half keeps its speed
  when its source's chunk unloads with S1's guard on (§2.6, S5). Each slice follows what its test
  shows.
- **Create's chunk loads change Create's own behaviour when guarded.** Today a package can enter a
  conveyor in a chunk that is loaded but does not tick and stop there; with S1's guard it waits on the
  last ticking conveyor instead. A recount today loads a source's chunk and drops its old block entity;
  with the guard it keeps the old one. The contract tests compare both with Create's answer with the
  chunk loaded, and the mixins are listed in PATCHES.md, so a Create update that moves these lines
  fails a test rather than a world.
- **A part of a network that comes back stale** (§2.8): Create breaks it at the attach when its speed no
  longer matches. S1 makes this more common (rotation no longer reaches an away chain conveyor), and the
  re-sync (SC37) mends it at the first attach after the load, which also mends Create's own break at a
  shaft across a chunk border, and at the promotion of a chunk that comes back FULL without loading
  (SC45; one tick's window is left open there). With C2ME, which the pack runs and the GameTests do not,
  the chunk system is C2ME's: its status reports reach the same hook from the main thread, with
  demotions late too, and this path is untested there; if it posts chunk loads off the server thread, a
  chunk it loads FULL is re-synced only at its first attach, once it ticks (SC73). A part that comes back with its source's chunk not loaded is left to
  Create, which is how it was before S1. The re-sync calls Create's private propagation steps
  (PATCHES.md row 56), which a Create update can move: the mixin then fails the launch.
- **The kept routes reach into Create's routing table** (an accessor for a private method, a public
  record). A Create update can break them; the contract test catches it, and Q5 (b) is the fallback.
- **One gated boiler stops a domain two boilers drive** (S5-1): the solve gates the whole domain when
  any of its boilers is gated, the safe side, where Create would run on the other tank's share. S5-2's
  SC152 does better only across districts (an away owner's gated boiler drops its own sources from the
  network); inside a district the gate still stops the whole domain.
- **A loaded half of a shared network runs on Create's last view of an away source** (§2.6): if the
  away boiler starves in the sim, the loaded machines keep Create's speed. Writing the sim's boiler
  state into a loaded network is not part of this design.
- **What S5-2 leaves out of the shared network** (§2.6, SC151 to SC154):
  - A black-box district on a shared network runs its box as before, not gated by the network; a
    black-box owner's box state is not read either, so its driven sources give the network nothing
    (SC152, the safe side).
  - A district with no compile (a cold district) has no part: the network is incomplete and its other
    parts run decoupled until that district compiles.
  - Peers go stale when districts appear, merge or split: a part names the districts of its last
    compile until its district compiles again, so the network can stay incomplete (decoupled, bounded)
    or count a restored part's members Create no longer lists until then.
  - The live view reads the owners' last check solves, so a loaded part's input lags an owner's change
    by up to one sweep.
  - Heaters Create counts as passive by block tag (a campfire, lava, magma) are not heaters in the
    model: a boiler on them alone is not passive there (SC154). Java's current row is Create's
    (passive), the solve's is stopped: the unit's windows fail where its counts move (it then does
    not replay), and a unit whose counts do not move passes on a stopped line.
  - An engine's shaft in another district's chunk than its tank's is tabulated by neither part (the
    tank's district lists it as foreign, the shaft's district as an engine shaft without its tank):
    the network is incomplete and both parts run decoupled, bounded.
  - No test drives the oscillation cap (`NET_ROUNDS`); a network that changes at every round is
    reasoned, not shown.
  - A network's status shows only in the engine's industry log, not in `/rtsim district info`.
- **What S6 leaves out of the joins** (§2.1, SC155 to SC159):
  - The docking that first tells a station's chunk where it serves books its cargo to the station
    chunk's district, which names none of those inventories, so that cargo stays the district's (as
    before S6); from the next docking, after the detection moves the chunk, it goes to the served
    district.
  - A station's chunk with a seed of its own stays with its seed's district (J6 moves seedless chunks
    only); its interfaces' inventories in another district are no ports of it, as before S6.
  - J3 is seen from a chunk whose block leads to the controller in another chunk: a vault whose other
    chunks hold nothing the walk reaches stays in its controller's chunk's district alone, and those
    chunks are no members.
  - J6's attach needs the served district in the detection: `Rescan.grown()` adds the district holding a
    reached station chunk's served chunk, and `stationServed` marks both chunks when the served chunk
    changes. A station chunk the walk does not reach (an unloaded member) adds no candidates, so its group
    attaches among the districts the detection already holds.
  - Two stations in one seedless chunk that serve two districts: the chunk goes with the station at the
    lowest position (SC156); the other station's cargo is booked to that district, which holds none of
    its interfaces' inventories, so it learns no ports there and its cargo stays the district's.
  - A station chunk in no district never learns where it serves: an arrival there is unwatched
    (`IND_TRAIN_UNWATCHED`, SG50), so its departure runs no `ports` and calls no `stationServed`. A
    platform whose station block lies outside every district while its carriages dock in a factory stays
    outside, as before S6, unless a seeded district is 8-adjacent to the station's chunk or touched by
    its station (J6's fallback).
  - An arm in a loaded chunk that does not tick (or whose range is not loaded) keeps its points unread,
    so its chunk keeps its saved pairs until it ticks (SC157), whatever has changed around it.
- **Fluid counting** (§4.2): which position names a pipe network's mover is UNVERIFIED. Until it is, a
  store shared by two sections through pipes is a judging group.
- **Counting at the movers** (§4.2, S3). Verified by GameTest at a vault between two lines
  (`VaultBetweenMixersFactory`, `ResidueAtASharedVaultFactory`): a chute's push and a smart chute's draw are each
  counted once (over 600 ticks the vault's stock changes by what the pairs counted in less out), Java takes the
  engine's sets for the far and the control copy, nothing counts while the far copy is away, a move no count
  point knows (a test's capability insert) leaves a residue that fails both units' window on that good (SC88),
  and the next window passes both again with their strikes back at 0. UNVERIFIED by any GameTest, and checked
  only by the launch (`defaultRequire 1`): the funnel, arm, frogport, packager and unpacking count points (SC83),
  and whether a Create update adds a path into an inventory that no hooked call touches (it would show as a
  residue and a failed window, not a wrong pass). A basin has no store to pour into in Create 6.0.10 (SC82), so
  its hook is tested only on its hop to a chute.
- **Uncounted movers at shared stores** (vanilla hoppers, the drill's shortcut, other mods) make judging
  groups (§4.3). How many shared stores a hopper reaches was not measured; on a base with many hoppers
  at shared stores, more sections are judged together.
- **The custom data key** on a held package: that a mod-registered component would not decode after an
  uninstall is UNVERIFIED for 1.21.1; the design uses the vanilla component either way. Create keeps a
  package's components as they are while it travels (UNVERIFIED; `aRouteToAnAwayFrogportIsKept` checks
  the key survives the hold and the hand-off).
- **Redstone links between districts** (a switch in one, a clutch in the other) are data coupling the
  compile does not model across districts. Splitting districts makes this more common; until it is
  modelled, the gated machine takes its compile-time state.
- **Members and sources in chunks of no district** on a shared network are reported as last loaded by
  each part's compile (its tail, counted by the lowest district's part, SC153; §2.6); an edit there
  now asks for a recompile (§3.6), but a change that is not an edit (a source's chunk unloading) is
  not seen until a district on the network compiles again.
- **A unit's edit box is its nodes' positions** (a store's controller block, a residual group's
  extent; §3.6). An edit at another block of a multiblock store, or along a pipe or belt between
  two nodes, lands in no unit's box: the district's window goes, the units go on. Java recompiles
  after every edit, and the recompile carries no unit whose sections' fingerprints changed, so the
  gap is the stretch from the edit to the new compile's END. Java sends no store's extent and no
  route's blocks today; both would close it.
- **Section counts on other bases** may differ from the reference base: `sectionsMax` and the row caps
  bound the cost either way, at the price of coarser sections.
- **Stores shared by many sections** are few after the fold, measured on the reference base, see the
  local notes; the design handles them (counting at movers, groups) but they are untested at scale.
