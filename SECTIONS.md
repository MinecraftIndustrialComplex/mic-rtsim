# Sections: factories cut at links and stores

**Status:** design approved by Ben on 2026-10-22 (all six questions in §12 answered, each the
recommended option); not built. Every other choice is listed in §13 and can be undone.

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
| What joins two chunks into one district | a face touch of industrial block entities across the border, a plugin link (fans, Ender channels), and **any 8-neighbour member chunk** (`Districts.commit`, the loop at `Districts.java:910` to `:915`) | a face touch, a plugin link, one inventory spanning the border, rotation across the border that is not a face touch, an arm or ejector reaching across; never bare adjacency (§2.1) |
| Chain conveyors | not a link since DS-10, but chunks still join by adjacency, so a short chain usually sits inside one district | never a join; a chain network whose frogports sit in more than one district is a **package store** owned by no district, with a port at each of its frogports (§2.5) |
| Packages at frogports | not counted (`IndRecords.Port.PACKAGE` is defined and never sent) | on a package store's network, every package leaving or landing is an `IND_PACKAGE` with a new direction (`CHAIN_OUT`, `CHAIN_IN`) booked into the store, never into the district's links (§2.5) |
| A kinetic network shared by districts | each district's compile decides its driver from its own engines only (`CreateCompile.java:413` to `:418`, `:452`); districts disagree | a **shared network** in Rust: every district reports its own part from its own compile, the network's state is decided over all parts, and the districts on it replay in one cluster (§2.6) |
| The unit that learns, replays and goes stale | the district | the **section**: a working line between stores; the district stays the unit of loading, ports, the black box and the checksum (§3) |
| An edit | drops the district's open window and stops its glass replay until a window passes or Java recompiles (`glass/mod.rs:313` to `:332`) | stales the sections whose node box holds the edit and drops the district's open window; the recompile keeps every section whose build did not change (§3.6) |
| A replay that cannot go on | the district goes to the black box (`glass_mode`, `engine.rs:1597`, `dg.fallback.is_some()`) | the sections the failure names freeze, and the replay goes on over the rest from the tick of the failure; the black box takes the district only when no section replays (§3.5) |
| Measurement | one window per district per good | one window per section per good, plus the district's own window as the checksum; flows into stores shared by sections are counted at Create's movers (§4) |
| Chunk loading | the compile, Create's chain conveyors and Create's network recount read through `Level.getBlockEntity`, which loads chunks and pulls an away district into a partial load | the compile reads through `getChunkNow`, and mixins keep Create's chain and network lookups from loading a chunk (§2.8, slice S1) |
| Saved state | `IndustryData` format 2; glass page `IGL1` | `IndustryData` format 3 (package stores) then 4 (join pairs); glass page `IGL2` (sections inside the district's page); package store pages; a loud reset on load (§5, §6) |

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
| J1 | **Face touch** of two industrial block entities across the border, horizontal faces, as today (`Districts.java:1275` to `:1296`), **except** where either side is a `ChainConveyorBlockEntity` or a `StationBlockEntity`; two stores side by side still do not join (DS-10) | the walk, unchanged | one class test per touch |
| J2 | **Plugin link**, as today (`Districts.java:1298` to `:1313`): a fan's stream (`FanLinks`), an Ender channel; new: a mechanical arm's interaction points (`ArmBlockEntity.inputs`, `outputs`, `ArmInteractionPoint.getPos()`, read through the existing `CreateGlassAccess.armInputs` and `armOutputs`) and a weighted ejector's target (`EjectorBlockEntity.getTargetPosition()` :589) when they lie in another chunk. `armInputs` is null until the arm's first tick (`CreateGlassAccess.java:93` to `:97`), so an arm that has not ticked keeps its saved pair | `CreatePlugin.links` (`CreatePlugin.java:142`) | the arm's points, the ejector's one target |
| J3 | **One inventory across chunks:** a store whose canonical position (`canonicalInventory`, `CreatePlugin.java:174`: an `IMultiBlockEntityContainer`'s controller, a double chest's lower half) lies in another chunk joins the two chunks; an item vault (`ItemVaultBlockEntity`), a fluid tank, a create_connected item silo (`ItemSiloBlockEntity implements IMultiBlockEntityContainer$Inventory`) or fluid vessel | the walk canonicalizes every candidate today (`Districts.java:1260` to `:1268`) but records no join; new: it records the pair. `canonicalInventory` returns null when the controller's chunk is not loaded (`Districts.java:1518` to `:1519`); the saved pair then stands (§2.3) | one pair per such store |
| J4 | **Rotation across the border that is not a face touch:** diagonal cogs (`KineticBlockEntity.addPropagationLocations` :559 adds 4 diagonal positions when the cog can propagate diagonally, the lambda at its line 565) and any class that overrides `addPropagationLocations` (Ender Transmission's `EnergyTransmitterBlockEntity` :33), confirmed by Create's own rule (`RotationPropagator.isConnected`, public, :403); chain conveyors excluded (§2.6) | the walk, for kinetic block entities within one block of a border only, and for overriding classes (a per-class cache) | a few neighbour reads per border cog, counted in the DS-11 budget |
| J5 | **Merge links** (`/rtsim district merge`) and **pins** (`split`), unchanged | `IndustryData` | none |
| J6 | **A station's chunk** with no seed of its own joins **exactly one** district: the one holding the inventories its docked interfaces reached last, saved as a join pair (station chunk, served chunk) so it outlives a restart; before the first docking, the lowest district id among those 8-adjacent to it or linked by what touches it (DS-10's anchor rule, narrowed to one). Today `TrainWatch.ports` keeps only inventories of the station's own district (`d.inventoryAt(canon)`, `TrainWatch.java:424`) and sends nothing when there are none (`:430`), so it could never learn another district; it resolves each inventory by chunk to whichever district holds it instead | the walk's station roots (`Districts.java:843`); the pair from `TrainWatch.ports` (`:394`) | none |

Bare 8-neighbour adjacency is gone: the loop at `Districts.java:910` to `:915` is replaced by a union
over the joins the walk recorded. `grown()` (`:867`) still grows an incremental detection's frontier
by the 8-neighbour districts, because an edit in one chunk can create a join to the next.

### 2.2 The cut points, per mod

| Mod | Block (class) | Level 1: districts | Level 2: sections | Where it is counted |
|---|---|---|---|---|
| Create | chain conveyor (`ChainConveyorBlockEntity`) | **cut**: no touch join (J1) and no rotation join (J4); its network is a package store when its frogports sit in more than one district (§2.5), and a shared kinetic network when its rotation reaches another district (§2.6) | not a node | at the frogports |
| Create | frogport (`FrogportBlockEntity extends PackagePortBlockEntity`) | stays in its district by its touches to its packager and inventory | a `MOVER` node that claims its 18-slot package inventory (`CreateCompile.java:323` to `:324`, `:1056` to `:1063`; `GraphBuilder.java:229` to `:236`): no store, no cut. On a chain wholly inside the district it keeps today's `CHAIN` edges to the other frogports (`CreateCompile.frogportEdges` :1849) | on a package store's network: leaving at `PackagePortTarget$ChainConveyorFrogportTarget.export` :114 (the conveyor's `addLoopingPackage` :122 or `addTravellingPackage` :123, on a call that is not a simulation); landing at `FrogportBlockEntity.tick` :155, the `insertItem` at :212 after the catch animation (`startAnimation` :223, started by `ChainConveyorBlockEntity.exportToPort` :361) |
| Create | packager (`PackagerBlockEntity`) | **not a cut** (Q6): it joins its target inventory by touch, and by J3 when the target spans chunks | a mover between its target store and its frogport or postbox | its packages, at the frogport or postbox they go through |
| Create | train station (`StationBlockEntity`) | **never a touch join** (J1); an anchor that joins exactly one district (J6) | not a node; its portable storage interfaces expose the train's storage, and the movers beside them pull and push | `TRAIN_CARGO` at departure: the whole train's change, with no inventory (`TrainWatch.java:316` to `:328`), mapped to sections through `IND_STATION_PORTS` (§4.4) |
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
  district's pairs only between chunks it looked at in that detection (`loadedAtTest`), and keeps the
  rest. Two exceptions keep a pair the walk did not see: an arm that has not ticked (J2) and a station
  that has not docked since (J6). A pair whose chunk is no longer a member of any district is dropped
  at the commit. Today the 8-neighbour rule needs no memory, which is why this is new.
- The walk records join pairs where it records touches today. A DS-12 incremental detection records
  only its own walk's joins, so the commit unions the recorded pairs with the saved pairs of the
  districts in question, never with the walk's touches alone (a trap met while making the dump).
- `IndustryData.FORMAT` goes to 4 with join pairs in slice S6 (3 is S4's package stores;
  `IndustryData.java:45`). The load path for an older format is §6's.
- `AdoptGameTests`' rule stays: an adopt gives the same districts a full detection gives.

### 2.4 Stations

A station is an anchor, not a seed (DS-10), and never a touch join (J1's exception), so a station
between two factories no longer glues them. Its chunk joins exactly one district (J6): the one its
docked interfaces serve, which is where its cargo moves. Train cargo is attributed to that district,
not to the district at the station's chunk as `TrainWatch` does today (`TrainWatch.java:218` at
arrival, `:305` after a restart); with J6 the station's chunk is a member of that district, so
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

- **Identity.** Java keeps each package store in `IndustryData` (format 3, slice S4): a persisted id
  given at first sight, its frogports (district, inventory, address filter, the position of the conveyor
  it serves), the hop counts between those conveyors along the walk, and the chunks of no district the
  chain passes through (§3.6). It is sent to Rust as a new `IND_PACKAGE_STORE` record when it changes. A
  re-walk that finds the same frogports keeps the id. When a network splits, the piece holding the
  lowest frogport position keeps the id, the rest get new ids, and what the store owes each frogport
  goes with that frogport; when networks merge, the lowest id stays and the contents add. A walk that
  stops at an unloaded chunk keeps the frogports and hops it last saw beyond it.
- **The compile.** On a package store's network, the compile makes no `CHAIN` edges; each frogport's
  chain side is a port.
- **Records.** Every package that leaves a frogport of a package store onto the chain, and every one
  that lands in one, is an `IND_PACKAGE` (the layout of `IndRecords.Package` :353: district, inventory,
  direction, address, packages, op id, contents) with the new direction `CHAIN_OUT` (3) or `CHAIN_IN`
  (4), from two mixins at the count points of §2.2. The engine books it into the store and as a port of
  the district's window. It never books it into `dist.links`: today's IN and OUT path for postbox mail
  adds a `LinkKey::Mail` that is never cleared (`engine.rs:2967` to `:2986`, `district.rs:300`), and
  `glass_mode` sends any district with links to the black box (`engine.rs:1591` to `:1599`). A
  package between two frogports of one district on a package store's network is that district's
  `CHAIN_OUT` and `CHAIN_IN`, counted once each way, because the compile has no `CHAIN` edge there.
- **Routing.** Rust resolves each (exporter frogport, address) to the frogport Create would deliver to:
  a walk by hops from the exporter's conveyor over the store's last walk, taking the first matching
  frogport on the least-distance path (so a frogport on the way takes it), `*` filters last, matching
  as `matchAddress` does. Create breaks ties by advert timing, which Rust cannot know; Rust takes the
  lowest frogport position. The store keeps one pool per destination frogport and one for packages
  with no route, in goods and in packages, and a `CHAIN_OUT` books into the pool its (exporter,
  address) resolves to. The no-route pool is drawn by no one; at the exporter's reconcile its packages
  are written back into the exporter's inventory, room permitting (Create would have them circling).
- **Room.** Packages that wait for a full or stopped importer back up along their route through
  Create's gates, about 20 on each conveyor, until the exporter's own conveyor is full and it stops.
  So the store's room toward one destination from one exporter is 20 times the conveyors on the route
  (an ESTIMATE of where Create stops the exporter: arrivals overshoot a gate by a few, and routes that
  share conveyors share their room). An exporter's replay books an export only while its destination's
  pool holds less than that room.
- **The census.** A player can hang packages on a chain or take them off, and none of that is counted.
  While every chunk of a store ticks, Java counts the packages on its conveyors (looping and travelling,
  by the destination their address resolves to) once per sweep and sends the count with the sweep;
  Rust sets the pools to it, and the difference is the store's residue, shown by `/rtsim district
  sections`. A `CHAIN_IN` beyond its pool takes the pool to zero, never below.
- **An end that does not replay.** A district on the store that runs the black box or is frozen neither
  books into the store nor draws from it while away; the store keeps what it holds, and the other ends
  see it as it is.

**Both ends away.** The exporter's replay books its exports into the pool while the room allows; the
importer's replay draws from its pool no earlier than the route's travel time (`RPM / 360` blocks per
tick along the chain, GLASSBOX.md §1). The store is a pool shared by the districts of one replay
cluster; today every pool belongs to one district (`cluster.rs`, the module comment), so this is new
(§10.3), and slice S4 builds it first. When an importer loads, the packages its pool holds are written
into its frogport as real packages, room permitting and the rest as room appears, as mail is written
into postboxes (INDUSTRY.md §8.2); Java journals one `CHAIN_IN` for each as it writes it. What the
pool holds for an importer that is still away is production owed to an unloaded chunk, which UR-1
lets a reset lose.

**One end loaded, the other away** (Q5). Recommended, the G2-11 way:

- **No chunk loads.** Slice S1's guards (§2.8) already keep Create's chain from loading the far chunk:
  a conveyor in a chunk that does not tick is refused at the hand-off as a full one is, and is not
  looked up.
- **Routes kept.** While a far frogport's chunk does not tick, a mixin keeps its route entry on the
  boundary conveyor (the last ticking conveyor on the way to it), with its filter and its distance
  from the store's saved walk, through an accessor for the private `ChainConveyorRoutingTable.insert`
  and the public `RoutingTableEntry` record. Create's own adverts carry it back along the near side.
  This also covers a restart, when Create's tables start empty (UNVERIFIED that the tables are not
  saved; the first GameTest of slice S4 records it).
- **The hold.** Packages for the far frogport then travel to the boundary conveyor and circle there,
  refused at its hand-off; behind them the near side backs up through Create's own gates. A held package
  stays in the world. When the frontier first holds a package it writes a key into the package's
  vanilla `minecraft:custom_data` component (the store's id, the engine's epoch, which every engine
  reset changes, and a serial) and sends an `IND_PACKAGE` with the new direction `CHAIN_HELD` (5)
  carrying the key. It is no port: a held package was already its exporter's `CHAIN_OUT`. A
  component type rtsim registered may not decode once rtsim is removed (UNVERIFIED for 1.21.1), so
  the vanilla one is used either way.
- **The draw.** The far district's replay draws held packages as G2-11's simulated side draws a live
  store's stock and owes the change. Each draw goes to Java at once (a command, `IND_PACKAGE_DRAWN`:
  store id and key), and Java keeps the drawn keys with the store.
- **Paying back.** A drawn package is never handed on: the frontier keeps refusing its hand-off, even
  after the far chunk ticks again, until the far district's reconcile removes it (the reconcile's
  command lists its key). If the boundary conveyor's chunk is not loaded then, the removal waits for
  that chunk's next load. A key that names another engine epoch, or one Java does not hold, is stripped
  and its package released, so after a reset nothing is removed: a duplicate at worst, never a loss
  (UR-1). Nothing is removed where a package lands: a drawn package that reaches a frogport another way
  (a player carried it) is struck from the ledger and lands as a normal `CHAIN_IN`.
- **The other way.** Packages the far district's replay sends to a loaded frogport are written into its
  inventory as real packages, room permitting, as in the both-away case, one `CHAIN_IN` each.
- **The limit.** Drawn packages wait for the far reconcile, as G2-11's owed draws wait for theirs, so a
  long absence moves at most the near side's room toward the far district: about 20 packages on each
  conveyor between the exporter and the boundary, plus the exporter's 18 slots. Past that the near
  factory backs up, as Create's does in front of a full importer.

### 2.6 Kinetic networks across districts

Chain conveyors carry rotation, so cutting districts at chains cuts kinetic networks between districts.
Inside a district nothing changes here (Q1 is about sections).

- **The driver bug (today).** The compile builds `enginesByNet` from the district's own engines
  (`CreateCompile.java:413` to `:418`) and asks `singleBoiler` (`:615` to `:626`) about them (`:452`,
  `:453`), which returns null for engines on several tanks. Two districts on one network see different
  drivers, or none, and a district with no engine of its own takes the network as one fixed level
  (`:481` to `:485`), which keeps running when the boiler stops.
- **What Create keeps.** A network's `sources` and `members` keep a block entity whose chunk unloaded,
  as it last was: `SmartBlockEntity.setRemoved` skips `remove()` on a chunk unload (lines 135 to 137).
  After a restart, the saved network's `unloadedCapacity` and `unloadedStress` stand for the members
  not loaded yet (`KineticNetwork.initFromTE`, lines 28 to 34; `addSilently` takes off what loads). A
  recount (`calculateCapacity`, `calculateStress`) looks every listed block entity up through
  `Level.getBlockEntity` (lines 139 and 155) and drops those no longer there; that lookup loads their
  chunks, and S1's guard skips the ones whose chunk is not loaded, leaving them as Create last saw them
  (§2.8). So the loaded half of a network keeps an away source's last capacity.
- **The shared network** (Q2). A network whose members lie in chunks of more than one district is a
  shared network in Rust, owned by no district. Each district's compile reports **its own part**: the
  members in its own chunks with their stress impact, and its own sources: each boiler with its engines
  (speed and capacity at each of its levels, tabulated as today, `:454` to `:473`) and each fixed
  source with its speed and capacity. Members and sources in chunks of no district (chain conveyors on
  the way) are reported by the district holding the network's root, else by the lowest district id on
  the network, as last loaded. Rust keeps every part as its district last compiled it, so an unloaded
  district's part is known.
- **The network's state** is the tuple of its boilers' levels. As in Create, the fastest source sets
  the speed (a steam engine runs at 16, 32, 48 or 64 RPM by its boiler's efficiency,
  `PoweredShaftBlockEntity.getGeneratedSpeed` :110 and `getSpeedModifier` :118; the efficiency comes
  from heat, boiler size, water supply and engine count, `BoilerData.getEngineEfficiency` :179), and
  every source adds its capacity at its own speed, a slower one included
  (`GeneratingKineticBlockEntity.applyNewSpeed`, lines 142 to 163; `updateGeneratedRotation`, lines 94
  to 97). The network runs while that capacity carries the summed stress at that speed, and stops
  otherwise. No district is picked as "the source".
- **Replay.** The districts on one shared network replay in one cluster (`Cluster`, as I13's power
  links already make clusters), stepped in time order, on the cross-district cluster slice S4 builds
  for package stores. A district holding a boiler writes its level changes into the shared network as
  breakpoints of the cluster, and every district reads the network's state from it. This is
  glass-to-glass coupling across districts, new code (§10.3). G2-16's rule (a power link sends its
  districts to the black box, `engine.rs:1594` to `:1595`, its doc at `:1577` to `:1580`) does not
  apply: both sides model the network. A black-box district on a shared network couples as I13's
  `requires` does: its part runs while the network runs, and a source it holds counts only while its
  box processes run (the safe side).
- **Loaded districts.** A loaded district's network is Create's. The shared network matters for away
  districts; a loaded district's boiler level reaches it from that district's sweeps. The other way is
  not modelled: a loaded half runs on Create's last view of an away source, whatever that source's
  replay does (§14).
- **UNVERIFIED, tested first:** that the loaded half keeps its speed when the source's chunk unloads,
  with S1's guard on, as the facts above imply (`aNetworkWhoseSourceChunkUnloads`).

### 2.7 Data coupling across districts

Factory gauges, stock links and redstone links never join districts. Gauge networks across districts are
I16's (warn, then fulfil); `GaugeNetworks` already warns. A redstone link from one district's switch to
another district's clutch is out of scope here (§14).

### 2.8 Reads that load chunks

`Level.getBlockEntity` loads a chunk on the server thread (decompiled `Level.java:707` to `:713`). Each
load runs `Districts.onChunkLoad` (`Districts.java:223` to `:231`) and `Reconciler.chunkLoaded`
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
  instead (SC45, row 58), with any network update held back while it was away (row 54).
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
   reaches through one store, downstream first, then upstream; ties go to the section holding the lowest
   node position (never Java's node order).
3. Transfer sections that reach no working section merge with each other across the stores they share:
   a storage system (vaults, funnels and belts sorting goods between them) is one section.
4. An **idle** piece (no machine and no store, or a residual group that touches nothing) keeps no state;
   its nodes belong to no section and carry no flow in the solve.
5. If more than `glass.sectionsMax` (TUNABLE, 64) sections remain, the two smallest sections (by
   nodes) that share a store merge, else the two smallest that share a domain, else the two smallest
   in the district, ties by the lowest node position, until it holds. The same merge serves the row
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

State slots (a heater's burn left, a controller's output, a domain's live level, stress and capacity)
are not part of it. A network's load on the section is §3.6's domain check, not the fingerprint.

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
  the district's glass replay and sends the district to the black box (`glass_mode`, `engine.rs:1597`,
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
- A frozen section holding a network's boiler stops that domain for every section on it, and for every
  district on a shared network (conservative: nothing runs on an engine the sim does not trust).
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
- Stale sections stop replaying at once (I5, I7). The others keep their windows and their replay.
- **Other discards** (`report.rs`, the `discard` reasons). `REVISION` and `SESSION` discard every
  section's open window and the district's: the inventory indices, or the session the window counts
  in, are gone. `RECOMPILE` discards only the sections the recompile changed (below). `NOT_LIVE` is
  per section (§3.4). A station's `Discard.OTHER` (`TrainWatch.java:306`, `:314`: a cargo change no one
  measured) discards the sections the station's last `IND_STATION_PORTS` named, every section when it
  named none, and the district window.
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

- Rust tells Java which movers to count at which stores, per compile (`IND_COUNT_SET`, a command
  without an op id, as `IND_PROBE_SET` 0x8406 is, choice GB5). Java answers in the sweep with
  cumulative amounts per (mover, store, good, direction) (`IND_COUNT_SAMPLE`, not counted in the
  sweep's records, as `IND_PROBE_SAMPLE` 0x004D is, choice GB3). The mover's position names its node,
  so the section is known with no face mapping.
- The count points are Create's own movers, where they move real goods:
  - `InvManipulationBehaviour.insert` and `extract` (funnels and every other user of the behaviour),
    skipping simulated calls (`CapManipulationBehaviourBase.simulateNext`);
  - the chute's transfers (`ChuteBlockEntity.handleInput` :346, `handleDownwardOutput` :367,
    `handleUpwardOutput` :423, through the handler `grabCapability` :503 returns);
  - the arm's `ArmInteractionPoint.insert` :116 and `extract` :123 and :130, which carry the
    `ArmBlockEntity`;
  - the basin's outputs into a neighbouring inventory (`BasinBlockEntity.tryClearingSpoutputOverflow`,
    lines 416, 426 and 448 to 449; `acceptOutputsInner`, lines 546 to 549, 597 and 606);
  - the frogport's push into and pull from neighbouring inventories
    (`FrogportBlockEntity.tryPushingToAdjacentInventories`, lines 262 to 282; the pull, lines 302 to
    308);
  - the packager's extraction from its target (`PackagerBlockEntity.attemptToSend`, lines 440 and 455)
    and its unpacking into it (`DefaultUnpackingHandler.unpack`, lines 41 to 74);
  - fluids at `FluidNetwork.tick` :66, where a pipe network fills and drains its endpoints
    (UNVERIFIED: which position names the mover, the pump or the pipe end at the store).
- Not counted in these slices: vanilla hoppers and droppers (they use the `Container` directly), the
  drill's cobblestone shortcut into a hopper below (`DrillBlockEntity`, lines 76 to 79), and other mods'
  movers. A shared store that one of them reaches makes a judging group (§4.3).
- A section's window at a counted store starts at the first sweep that carries `IND_COUNT_SAMPLE` for
  it after the count set changed.
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
  sections are judged apart;
- **for train cargo** that maps to several sections or to a shared store (§4.4).

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
- **Train cargo** is the whole train's change at departure, with no inventory (`TrainWatch.java:316` to
  `:328`). Portable storage interfaces look up no neighbour: they expose the train's storage, and the
  movers beside them pull and push. `IND_STATION_PORTS` (`TrainWatch.java:394`) names the district
  inventories the docked interfaces reach through conveyance. The cargo goes to the sections of those
  inventories: to one section as its port; to several sections, or to a shared store, as a judging
  group for the cargo's goods with that store interior. Cargo that names no inventory stays in
  `ports_d`. Java journals `TRAIN_CARGO` before the same departure's `IND_STATION_PORTS`
  (`TrainWatch.java:327`, `:331`), so the engine maps a cargo port after it has read the whole step.
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
- The page magic becomes `IGL2` (`glass/page.rs:32`), in slice S2. A restored page whose stored
  fingerprints do not match the sections recomputed from its compile (a changed tunable such as
  `sectionsMax`, or a changed fold), or one with the old magic `IGL1`, drops **that district's** glass
  state, and the district learns again; the restore logs one line with the number of districts
  dropped. Only an undecodable page fails the whole restore, as G2-82 says.
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
  districts, dissolving them as detection reaches them (`IndustryData.java:177` to `:180`). New: Java
  drops them, and the format change itself resets the industry engine through the host's fresh-start
  path (the one G2-82 uses), so Rust keeps no district Java no longer has, whether or not a glass page
  exists. Districts re-derive from the seed attachments at the first detection. Slice S4 brings this
  in with format 3 (package stores); slice S6's format 4 (join pairs) goes through the same path.
- **A glass page with magic `IGL1`** (slice S2, before any format change) drops its district's glass
  state, as §5 says; after a format reset there is none to restore.
- The operator sees one line at start: "rtsim industry: the save format changed (SEC-1); learned
  factory state was reset and districts are detected again." Nothing is lost silently, and only the
  sim's own knowledge is lost (UR-1). The drawn keys of §2.5 name the old engine's epoch, so the
  packages they hold are released.
- No live world runs rtsim (DR-0). GameTest worlds and rtsim-view's fixtures are regenerated after
  landing, as G2-85 did.

---

## 7. Cost on a weak PC

### 7.1 Detection (DS-11)

- J1 adds a class test per touch. J3 adds one pair per store whose controller lies in another chunk,
  from a canonical position the walk already computes.
- J2 adds the arm's points and the ejector's target for the arms and ejectors the walk already visits.
- J4 adds, for kinetic block entities within one block of a border only, the 4 diagonal positions
  Create lists and an `isConnected` test, read through `getChunkNow` (never loading a chunk). These
  reads count as looked-at block entities in `rescanBlockEntitiesPerTick`.
- The commit's union becomes cheaper: over the recorded joins instead of nine lookups per member chunk.
- Worst case: a district made of border cogs. Each costs at most its 4 diagonal neighbours plus its
  face neighbours, so the budget still bounds a tick to `rescanBlockEntitiesPerTick` plus one chunk.
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
  at its next compile, restore or save, until both hold. Merging all of a district's sections reaches
  no extra rows, so the caps can always be met. A merged section starts learning (§3.2): coarser
  sections, and the merged sections' learning is lost.
- **Saves.** Every glass page is written whole at every save today (G2-83); the extra rows add to that
  until For Ben 15 writes only changed pages. Package store pages are small: a pool per destination
  and the held packages.
- **Clusters.** Every district on one shared network or package store replays in one cluster. The
  work is the sum of the members' replays, but a cluster settles as one model, and its catch-up costs
  its breakpoints times its diffusers (`cluster.rs`, the module comment: one multiply per diffuser per
  settle). Coupling n districts of similar size costs about n times more per breakpoint than replaying
  them apart. `industry.clusterDistrictsMax` (TUNABLE, 16) bounds it: a shared object whose districts
  exceed it is not coupled. Its districts then treat a shared network as S1 does (each bounds its
  members on it and learns their rates, as unstable domains today), and a package store moves nothing
  while any of its ends is away. Each such object is logged once and shown by `/rtsim district info`,
  so the lost coupling is never silent (Q2, Q4).
- The replay costs what it costs today inside a district: parts are formed as today, over the
  replaying nodes. A fallback's rebuild repeats at most one replay step per frozen section.

### 7.3 Counters (Java)

- One hash test per real move at a count point (is this mover counted), and one counter update when it
  is. Counted movers are only those at shared stores, which the fold keeps few. A cap
  `glass.countedStoresMax` (TUNABLE, 256 per dimension) turns the rest into judging groups.
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
| `twoFactoriesSideBySideStayTwo` | two press lines in neighbouring chunks, nothing between them | two districts (today one) |
| `aVaultAcrossABorderJoinsItsChunks` | a packager in one chunk under an item vault whose controller is in the next | one district (J3) |
| `diagonalCogsAcrossABorderJoin` | a large cog meshing diagonally with a small cog across a border | one district (J4) |
| `anArmReachingAcrossABorderJoins` | a mechanical arm taking from a depot in the next chunk | one district (J2); a detection before the arm's first tick keeps the pair |
| `aChainInsideOneDistrictStaysEdges` | two frogports of one factory on a chain inside it | `CHAIN` edges, no package store, no `CHAIN_*` record |
| `aChainBetweenTwoFactoriesLeavesThemTwo` (exists as `aChainBetweenTwoDistrictsLeavesThemTwo`) | two factories on one chain, frogports at both ends | two districts; one package store; every package a `CHAIN_OUT` at one end and a `CHAIN_IN` at the other; both stay glass; both windows balance |
| `twoFrogportsOfOneDistrictOnASharedChain` | a third frogport of the first factory on the same chain | its packages to the first frogport are counted once each way, with no `CHAIN` edge |
| `anUnloadedMemberKeepsItsJoins` | a district with one member chunk unloaded before a detection | the same district after it (saved pairs) |
| `aStationBetweenTwoFactories` | a station touching two factories, its interfaces serving the one whose chunk it does not share | two districts; the station's chunk and its cargo with the one its interfaces serve, also after a restart before the next docking |
| `aBoilerOnAChainDrivesTheNextDistrict` | a boiler and engines in one factory, a chain carrying rotation to a press line | both compiles report parts of one shared network; away, the press line stops when the boiler starves |
| `aPackageIntoAChunkThatDoesNotTick` | a chain from a ticking chunk into one that does not tick, with a reversal | first records what Create does (route expiry, the reversal at line 175); with S1's guards the package waits on the last ticking conveyor and the next chunk is not loaded |
| `aRouteToAnAwayFrogportIsKept` | the same, the far frogport's district away, then a restart | with the frontier, packages for the far frogport reach the boundary conveyor and are held there, before and after the restart; one `CHAIN_HELD` each |
| `aHeldPackageDrawnIsRemovedOnce` | the far district drew a held package | it is never handed on; after the far reconcile it is gone from the chain; after an engine reset before the reconcile its key is stripped and it lands as a normal `CHAIN_IN` |
| `aPlayerTakesAPackageOffTheChain` | a player takes a package off a store's chain and hangs another | the next census sets the pools; no pool goes below zero |
| `aNetworkWhoseSourceChunkUnloads` | a boiler network across two districts, the source's chunk unloaded | records that the loaded half keeps its speed with S1's guard on (§2.6) |
| `AdoptGameTests`, `RescanBudgetGameTests`, `DistrictSeedGameTests`, `CreateSeedGameTests` | existing | still pass: adopt equals a full detection, the budget holds |

### 9.2 GameTests, sections (Level 2)

New `CorpusFactory` fixtures next to `PackageLoopFactory` and `BoilerOverstressFactory`:

| Fixture | What it is | Checks |
|---|---|---|
| `VaultBetweenMixersFactory` | a vault between two brass mixing lines (one fills it by funnel, one draws by funnel) | two sections, the vault shared and counted at its funnels; each passes its own window; break and replace the second mixer: the first keeps its passed window and replays while away |
| `HopperAtASharedVaultFactory` | the same with a vanilla hopper also feeding the vault | the hopper is modelled and not counted, so the compile makes the two sections a judging group with the vault interior; the group's row balances and passes |
| `ResidueAtASharedVaultFactory` | `VaultBetweenMixersFactory` with the test inserting into the vault through its capability for one window (another mod's mover) | that window makes a judging group that fails on the inserted good; the next window judges the two sections apart again |
| `BasinIntoASharedVaultFactory` | a mixer's basin putting its output into a vault another line draws from | the basin's outputs are counted (§4.2); two sections, each passes |
| `BeltIntoPressFactory` | a belt straight from a press into a saw | one section (SEC-1's "cannot be split") |
| `SharedBoilerTwoLinesFactory` | one boiler network driving a crushing line and a press line that share nothing else | three sections: the boiler's, and two lines on one domain (Q1 a); an edit to the press line's belt stales only it; adding presses until the stress passes the least capacity the crushing line's window ran at stales the crushing line too |
| `StationPressLineFactory` | a station's portable storage interface emptied by a funnel into a vault feeding a press line (`StationCargoGameTests`' kit) | the cargo maps through `IND_STATION_PORTS` to the press line's section |
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
- Shared objects: a shared network's state from parts in three districts, two with boilers (the
  fastest source's speed, every source's capacity); a package store's split and merge; routing per
  exporter with a frogport on the way, `*` last and ties by position; room per route; the census and
  the floor at zero; an end in the black box neither books nor draws; held packages, drawn keys,
  epochs and what is owed; `clusterDistrictsMax` and its fallback.
- The page: `IGL2` round trip; a page whose fingerprints disagree with its compile, or an `IGL1` page,
  drops only its district's glass state; the row caps count extra rows only, and merging meets them.

### 9.4 The reference base, offline

The dump of §8.1 and `rtsim-cli glass --graph` let the reference base's numbers in the local notes be
repeated on the laptop copy after each slice. That check stays local; nothing from it is committed.

---

## 10. Build plan

### 10.1 Slices

| Slice | What | Tests | Depends on |
|---|---|---|---|
| **S1** | No chunk loads (§2.8): every compile read through `getChunkNow`, members filtered; mixins on Create's chain lookups (lines 161, 175, 248, 296 to 297 of `ChainConveyorBlockEntity.tick`; `canAcceptPackagesFor` :110) and network recount (`calculateCapacity`, `calculateStress`, `sync`, `remove`); built: also rotation propagation and the stale re-sync, the frogport's far end and the exports to ports (PATCHES.md rows 50 to 59). The driver: a network whose members lie in more than one district, or in chunks of no district, is marked in every district on it as today's unstable domains are (`CreateCompile.java:487`, `:641`: its members are bounded, their rates learned), the boiler's district included, so no district replays a driver or a load it cannot see. `/rtsim district graph` (compile and members); `rtsim-cli glass` | `aCompileLoadsNoChunk`, `anAwayDistrictOnAChainStaysAway`, `anAwayDistrictOnASharedNetworkStaysAway`, `aNetworkRecountKeepsAnUnloadedSource`, `aPackageIntoAChunkThatDoesNotTick` (its first half), `aDemotedChunkComesBackInStep`, `aBoilerOnAChainDrivesTheNextDistrict` (driver part), the CLI on a GameTest world | nothing |
| **S2** | Sections in the engine: cut, residual groups (Java), fold, state, fingerprints, per-section windows, liveness and discards, the verdict rule, judging groups from the compile, staleness by box, edits between districts and the domain check, per-section freeze with the rebuild at the fallback's tick, `IGL2` with the per-district drop; train cargo through `IND_STATION_PORTS` | §9.3, `BeltIntoPressFactory`, `SharedBoilerTwoLinesFactory`, `FrozenNeighbourFactory`, `StationPressLineFactory`, `HalfLoadedFactory`, `HopperAtASharedVaultFactory` | nothing: sections inside today's districts already pay off |
| **S3** | Counting at the movers (Q3): `IND_COUNT_SET`, `IND_COUNT_SAMPLE`, the mover mixins, judging groups by residue | `VaultBetweenMixersFactory`, `ResidueAtASharedVaultFactory`, `BasinIntoASharedVaultFactory` | S2 |
| **S4** | Package stores (Q4): `IndustryData` format 3 with the drop and the engine reset (§6), the store record and identity, `CHAIN_OUT` and `CHAIN_IN` at the frogports of package stores, no `CHAIN` edges on them, pools per destination with routing, room and the census, and the both-away replay on the first cluster that couples districts (a pool shared between them); then the frontier (Q5): routes kept, the hold, `CHAIN_HELD`, drawn keys and their removal | `aChainInsideOneDistrictStaysEdges`, `aChainBetweenTwoFactoriesLeavesThemTwo`, `twoFrogportsOfOneDistrictOnASharedChain`, `aPlayerTakesAPackageOffTheChain`, then `aRouteToAnAwayFrogportIsKept`, `aHeldPackageDrawnIsRemovedOnce`, the store's engine tests | S1. Useful before S6: chains between districts that are apart today are counted nowhere since DS-10 |
| **S5** | Shared kinetic networks (Q2): parts per compile, the network's state, coupling on S4's cross-district cluster | `aNetworkWhoseSourceChunkUnloads` first, then `aBoilerOnAChainDrivesTheNextDistrict` (away part), the shared network's engine tests | S1, S2 (the domain check), S4 (the cluster) |
| **S6** | Level 1: joins J1 to J6, no bare adjacency, saved join pairs, the station rule with `TrainWatch.ports` resolving any district, `IndustryData` format 4 through S4's reset; the stale comment at `CreatePlugin.java:169` to `:171` | §9.1's join and station rows, and the existing detection tests | S1, S4, S5: no district is split before its packages and its network have somewhere to go |
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
7. **The boiler.** If Ana starves the boiler of water while both buildings are away, its engines lose
   capacity, the shared network overstresses, and the press hall's replay stops with the brass works'.
   While a building is loaded, Create's own network does the same.

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
| SC4 | Arms and ejectors are plugin links, like fans; an arm that has not ticked keeps its pair | they move goods across borders without touching; `armInputs` is null before the first tick |
| SC5 | Join pairs saved per district as a set, replaced only between chunks looked at, dropped with their chunk's membership; format 4 | unloaded members must keep their joins (DS-11) |
| SC6 | A station never joins by touch; its chunk joins exactly one district, the one its interfaces serve, found by chunk in any district and saved as a pair; its cargo goes there | a station between two factories must not glue them; cargo belongs where it moves; a restart must not forget it |
| SC7 | Package store frogports send `IND_PACKAGE` with `CHAIN_OUT` and `CHAIN_IN` on a new engine path; a chain inside one district keeps its `CHAIN` edges | the mail path forces the black box; one count per crossing |
| SC8 | Package store identity, split and merge; pools per destination resolved per exporter with capture on the way and ties by position; room of 20 per conveyor on the route (ESTIMATE); the census sets the pools, floor zero; an end in the black box or frozen neither books nor draws | Create's own rules (§2.5), and the moves no one counts |
| SC9 | The frontier keeps far routes alive on the boundary conveyor, holds packages by refusing the hand-off, keys them in `minecraft:custom_data` with the store, the engine's epoch and a serial, and removes only keys of the current epoch that the far reconcile lists; nothing is removed at a landing | Create forgets routes to a chunk that does not tick; UR-1 after a reset; a mod component may not survive an uninstall |
| SC10 | No chunk loads: every compile read through `getChunkNow`, Create's chain lookups and network recount guarded, all in S1 | each load pulls an away district into a partial load |
| SC11 | Until S5, a network whose members lie in more than one district is an unstable domain in every district on it, the boiler's included | defined behaviour that exists today, the same everywhere |
| SC12 | A shared network has no source district: its state is the tuple of boiler levels, its speed the fastest source's, its capacity every source's at its own speed; members in no district reported by the root's district | Create's rules; several districts can hold boilers |
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
| SC37 | The stale re-sync (PATCHES.md rows 56 and 58): at a block entity's first attach, a source in another loaded chunk that gives it another speed or network makes it wait one tick, then Create's `propagateMissingSource` on it and `propagateNewSource` from the source; a chunk that comes back FULL without loading gets the same at its promotion (SC45) | Create breaks the block otherwise; the tick lets the loaded chunk's block entities join their network first; it also mends the break Create does at a shaft across a chunk border |
| SC38 | `/rtsim district graph <id>` lives in its own class (`GlassGraphCommand`), writes JSON to the server's `debug/` folder, and reads only the last compile and the index | the dump of §1 as an operator command; nothing is compiled or loaded to answer it |
| SC39 | `rtsim-cli glass` takes a district id, not a name | ids are what the paged state holds; names can repeat |
| SC40 | The compile reads a FULL chunk that does not tick as it is, and stops at one that is not FULL: an item dropped or thrown into a chunk that is not loaded goes to the sink, a crafter grid that reaches one is not modelled, and every stop is counted (`unloadedStops`) | a ticking district's eight neighbours are FULL, so stops happen only on long walks; a stop never invents a spill |
| SC41 | The frogport's far-end guard (PATCHES.md row 52) is checked by the launch only | a test would need a port in a ticking chunk whose conveyor reverses while the far end is away; the guard is one null answer Create already handles |
| SC42 | S1's GameTests build the away state with no forced chunk and a radius-1 ticket on A (A ticks, its ring is FULL, B two chunks away is at level 34 and never FULL); a creative motor stands in for boiler levels except in one boiler test, which fills its boiler through the tank's fluid capability; one batch per test | the away state of a district two chunks from the loaded one; a motor's speed changes on demand; the chunk load counts and the re-sync counter are server-wide, so no two of these tests run at once |
| SC43 | A pipe walk that meets a chunk that is not loaded stops there and bounds its pump | the far ends are not known, and a stop must never read as a spill |
| SC44 | A network is crossing when a listed source or member lies outside the district, or a listed member inside links out of it (a face across the district's border, a chain connection, a large cog's diagonal) to a chunk that is not loaded or to a kinetic block entity of the same network; with its root unlisted its ratios are taken from the lowest loaded listed member that turns | structural: Create's own count of members it has not seen (`unloadedMembers`) is wrong both ways (it is a saved number, decremented with a clamp, and reset by whichever member's save starts the network), and a link into a chunk that is not loaded may reach members that never loaded this session; the root's own speed is not held. `aCompileLoadsNoChunk` takes the away part out of the network, as a restart leaves it |
| SC45 | A chunk reported below FULL is noted until it unloads; when it is reported FULL or more again it is queued and re-synced (`StaleKinetics.chunkFull`) at the next server tick's start or end, not inside the promotion. Chunks that never left FULL and chunks loaded anew are not queued | the promotion runs inside the chunk system's own task queue, and a propagation there could re-enter it; a promotion by several levels at once reports only its last status. Left open: the chunk is readable and ticks from its promotion, and the report of it runs later, from the main thread's task queue, so for up to one tick its block entities run before the re-sync; a source that changes in that tick, or Create's periodic validation falling in it, meets the stale part, and Create's own rule applies to it. A promotion reported for a chunk that a demotion has taken below FULL since leaves it noted |
| SC46 | `Loaded.capability` answers null for a Create multiblock (a vault, a tank) with any chunk of its box not loaded, the box taken from its controller (the least corner: `getHeight` along the main axis, `getWidth` across) | a vault's controller builds its handler from every member through `Level.getBlockEntity`, which loads; a tank's does not, so for a tank this is stricter than needed, and a tank across an away chunk reads as away |
| SC47 | S2 is built in four sub-slices: S2a the sections themselves (the cut, the fold, fingerprints, judging groups, `Compiled.sections`, the `IGL2` page with the per-district drop, `rtsim-cli glass` output, Java's residual groups), changing no window or replay; S2b per-section windows and verdicts, liveness, discards, staleness by box, recompile matching by fingerprint and the row caps; S2c per-section replay, the freeze and rebuild at the fallback's tick, the black box only when no section replays, the domain check; S2d edits between districts, train cargo through `IND_STATION_PORTS`, §9.3's GameTest fixtures and a residual groups GameTest | each lands with its own review and tests; S2a moves no replay, so a fault in the cut shows in its tests and its output before any window reads it |
| SC48 | The void sink (`SINK_VOID`) is a cut, as a store is, and lies in no box; a piece with an edge into it is working | one void sink per district would glue every line that drops into it; dropping goods into the void is work a window measures |
| SC49 | Only goods edges make a store a section's: an `OBSERVE` edge claims no store, and `HEAT` and `CONTROL` edges join the nodes they link, as any edge between two nodes that are not cuts does | a controller reading a vault another line fills moves nothing there; a heater and its machine, a switch and what it drives, run as one |
| SC50 | A section's box holds its nodes, the stores it touches (interior and shared) and each residual group's extent, grown by one block | staleness by box (S2b) must see an edit beside a section's store and inside an unknown machine's blocks |
| SC51 | The fold's ties: a transfer piece takes the section of a piece it reaches through one store, downstream (a store it gives into) first, then upstream, the lowest position first; each round's moves apply together at its end. Sections are ordered by (lowest position, fingerprint, first node) | §3.2's "never Java's node order": the result does not depend on the order pieces are visited in |
| SC52 | A section's fingerprint is FNV-1a 64 over its records, each encoded alone with its length and the encodings sorted: each node by codec kind, model, position (a virtual node by kind only), flags, rule, capacity, whether it has a domain, params with state slots zeroed and its rates (a level by its selector and speed when the node has a domain, a recipe by its contents); its stores; the edges with an end in it; per domain the `STRESS_DISABLED` flag, its levels and the section's own members, never the driver | the same factory in any node order gives the same print; a shared domain's other members are other sections', and a driver can change while the section does not |
| SC53 | A glass page of format `IGL1`, or whose sections are not those its compile cuts into now, is read as absent: the district is restored with no glass state (as `rtsim-view`'s inspect restores one alone), its key is marked so the next save deletes the page, the host logs it, and the district learns again. A page that does not decode still fails the whole restore | §5's per-district drop: every other district's glass state is kept |
| SC54 | Until S2c the sections are in no `Report` and not in the bridge's `to_f64s`; `rtsim-cli glass` shows them (count, merges by the cap, judging groups, shared stores, each section's nodes, stores, box and fingerprint, and each node's section with `--graph`), cut with the default `glass.sectionsMax` | the bridge's report header is fixed, and Java needs the sections only once they replay |
| SC55 | Residual groups: the residual positions sorted and joined by their faces and by any plugin's `links` (not only Ender Transmission's) when both ends are residual, numbered by their lowest position; a group's node sits at its lowest corner with params `dx, dy, dz` (its extent) and the FNV-1a hash of its sorted positions, made when a plugin's edge first reaches it; the compile is a black box one when any group node was made | SC14's groups; the hook is the district detection's own, so a mod's wires join here as they join there; the hash tells two groups of one box apart |

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
  demotions late too, and this path is untested there. A part that comes back with its source's chunk not loaded is left to
  Create, which is how it was before S1. The re-sync calls Create's private propagation steps
  (PATCHES.md row 56), which a Create update can move: the mixin then fails the launch.
- **The kept routes reach into Create's routing table** (an accessor for a private method, a public
  record). A Create update can break them; the contract test catches it, and Q5 (b) is the fallback.
- **A loaded half of a shared network runs on Create's last view of an away source** (§2.6): if the
  away boiler starves in the sim, the loaded machines keep Create's speed. Writing the sim's boiler
  state into a loaded network is not part of this design.
- **Fluid counting** (§4.2): which position names a pipe network's mover is UNVERIFIED. Until it is, a
  store shared by two sections through pipes is a judging group.
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
  one district's compile (§2.6); an edit there now asks for a recompile (§3.6), but a change that is
  not an edit (a source's chunk unloading) is not seen until that district compiles again.
- **Section counts on other bases** may differ from the reference base: `sectionsMax` and the row caps
  bound the cost either way, at the price of coarser sections.
- **Stores shared by many sections** are few after the fold, measured on the reference base, see the
  local notes; the design handles them (counting at movers, groups) but they are untested at scale.
