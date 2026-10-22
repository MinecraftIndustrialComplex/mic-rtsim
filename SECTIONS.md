# Sections: factories cut at links and stores

**Status:** design of 2026-10-22, for Ben's review. Nothing here is built. The questions in §12 come
first; every other choice is listed in §13 and can be undone.

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
| A kinetic network shared by districts | each district's compile decides its driver from its own engines only (`CreateCompile.java:413` to `:418`, `:452`); districts disagree | a **shared network** in Rust: every district reports its own part from its own compile, the level is decided over all parts, and the districts on it replay in one cluster (§2.6) |
| The unit that learns, replays and goes stale | the district | the **section**: a working line between stores; the district stays the unit of loading, ports, the black box and the checksum (§3) |
| An edit | drops the district's open window and stops its glass replay until a window passes or Java recompiles (`glass/mod.rs:313` to `:332`) | stales the sections whose node box holds the edit; the recompile keeps every section whose build did not change (§3.6) |
| A replay that cannot go on | the district falls back to the black box or freezes | the sections the failure names freeze, and the replay restarts over the rest (§3.5) |
| Measurement | one window per district per good | one window per section per good, plus the district's own window as the checksum; flows into stores shared by sections are counted at Create's movers (§4) |
| Chunk loading | the compile reads three places through `Level.getBlockEntity`, which loads chunks | the compile reads through `getChunkNow`; a package bound for a chunk that does not tick waits on the last ticking conveyor (§2.5, slice S1) |
| Saved state | `IndustryData` format 2; glass page `IGL1` | `IndustryData` format 3 (join pairs, package stores); glass page `IGL2` (sections inside the district's page); package store pages; a loud reset on load (§5, §6) |

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
   are shared by sections. The moves at shared stores were made by Create's movers (funnels, chutes,
   pipes), which can be counted where they act (§4.2).
6. **No tool dumps a live world's district graph.** §8.1 adds one.

---

## 2. Level 1: districts cut at links

### 2.1 What joins two chunks

A district is a connected group of member chunks holding at least one seed (DS-10). Two member chunks
are connected only by these joins:

| # | Join | Where it is found | Cost |
|---|---|---|---|
| J1 | **Face touch** of two industrial block entities across the border, horizontal faces, as today (`Districts.java:1275` to `:1296`), **except** where either side is a `ChainConveyorBlockEntity` or a `StationBlockEntity`; two stores side by side still do not join (DS-10) | the walk, unchanged | one class test per touch |
| J2 | **Plugin link**, as today (`Districts.java:1297` to `:1313`): a fan's stream (`FanLinks`), an Ender channel; new: a mechanical arm's interaction points (`ArmBlockEntity.inputs`, `outputs`, `ArmInteractionPoint.getPos()`, read through the existing `CreateGlassAccess.armInputs` and `armOutputs`) and a weighted ejector's target (`EjectorBlockEntity.getTargetPosition()` :589) when they lie in another chunk | `CreatePlugin.links` (`CreatePlugin.java:142`) | the arm's points, the ejector's one target |
| J3 | **One inventory across chunks:** a store whose canonical position (`canonicalInventory`, `CreatePlugin.java:174`: an `IMultiBlockEntityContainer`'s controller, a double chest's lower half) lies in another chunk joins the two chunks; an item vault (`ItemVaultBlockEntity`), a fluid tank, a create_connected item silo (`ItemSiloBlockEntity implements IMultiBlockEntityContainer$Inventory`) or fluid vessel | the walk canonicalizes every candidate today (`Districts.java:1260` to `:1268`) but records no join; new: it records the pair. `canonicalInventory` returns null when the controller's chunk is not loaded (`Districts.java:1518` to `:1519`); the saved pair then stands (§2.3) | one pair per such store |
| J4 | **Rotation across the border that is not a face touch:** diagonal cogs (`KineticBlockEntity.addPropagationLocations` :559 adds 4 diagonal positions when the cog can propagate diagonally, the lambda at its line 565) and any class that overrides `addPropagationLocations` (Ender Transmission's `EnergyTransmitterBlockEntity` :33), confirmed by Create's own rule (`RotationPropagator.isConnected`, public, :403); chain conveyors excluded (§2.6) | the walk, for kinetic block entities within one block of a border only, and for overriding classes (a per-class cache) | a few neighbour reads per border cog, counted in the DS-11 budget |
| J5 | **Merge links** (`/rtsim district merge`) and **pins** (`split`), unchanged | `IndustryData` | none |
| J6 | **A station's chunk** with no seed of its own joins **exactly one** district: the one holding the inventories its docked interfaces reached last (`IND_STATION_PORTS`, `TrainWatch.java:394`, kept per station in memory, so after a restart until the next docking the fallback applies), else the lowest district id among those 8-adjacent to it or linked by what touches it (DS-10's anchor rule, narrowed to one) | the walk's station roots (`Districts.java:843`) | none |

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
| Create | chest, barrel (stock tag) | J1 only with a non-store neighbour | a `STORE`: a cut | at the movers; a vanilla hopper uses the `Container` directly and is not counted (§4.3) |
| Create | belt, funnel, tunnel, chute, pipe, pump, depot, basin, spout, drain, press, mixer, saw, deployer, crafter, millstone, crushing wheel, fan, arm, burner, steam engine | J1 (and J2 for arms, ejectors, fans) | **never a cut**: a belt straight into the next machine is one section (SEC-1) | inside its section's window; at a shared store, funnels, chutes, arms and pipes count their own moves (§4.2) |
| Create | shaft, cog, gearbox, encased chain drive, clutch, gearshift, water wheel | J1, J4 | not nodes: members of a kinetic domain, a shared input (Q1) | not counted |
| Create | factory gauge, stock link, stock ticker, redstone requester (`LogisticallyLinkedBehaviour`) | **no join**: data coupling, I16 (`GaugeNetworks` warns) | a controller in the section it gates | not goods |
| Create | redstone link | **no join**: data coupling | not modelled across districts (§14) | not goods |
| create_connected | item silo, fluid vessel (`ItemSiloBlockEntity`, `FluidVesselBlockEntity`) | J3 | a `STORE`: a cut | at the movers |
| create_connected | inventory bridge, access port (`InventoryBridge`, `InventoryAccessPort`) | J1 | the proxied store is the store; the bridge's nodes join it (UNVERIFIED: the proxy's capability path) | at the movers on the proxy |
| create_connected | kinetic bridge (`KineticBridgeBlockEntity`, `KineticBridgeDestinationBlockEntity extends GeneratingKineticBlockEntity`) | J1 (the two halves are adjacent) | two networks already: the destination is a generator fed by the source's network | not counted |
| Ender Transmission | item, fluid and energy transmitters (`EnergyTransmitterBlockEntity.isCustomConnection` :23, `addPropagationLocations` :33) | J2 (`ChannelLinks`), J4; local only (I13) | the channel's buffer is global per channel and password and has no block position; the repository calls it "not stock, not a port, not sampled" (`ChannelLinks.java:31` to `:32`). It is no store and no cut; the compile models no transmitter, so transmitters fall into a residual group (§3.1) | not counted |
| PowerGrid | wires (entities), batteries | no join; `IND_POWER_LINK` couples districts (I13, `PowerLinks`), unchanged | no glass compiler models PowerGrid; its blocks are boundary inventories or residual, cut or joined by §3.1's rules like any other | the link record, unchanged |
| Sophisticated Storage | controller, chests, barrels, connectors | `#mic_rtsim:district_not_seed` | stores; a storage network is GA-2's shared store (G4), not this design | G4 |
| create_power_loader, Numismatics, create_ultimate_factory | station chunk loader; no package block entities; recipes only | nothing | nothing | nothing |

### 2.3 Saved joins and incremental detection

- **Each district saves its join pairs** (two chunk positions, 16 bytes a pair) next to its seed counts.
  A join needs both chunks loaded to be seen, so an unloaded member keeps the joins its district last
  saw: an incremental detection replaces a district's pairs only between chunks it looked at in that
  detection (`loadedAtTest`), and keeps the rest. Today the 8-neighbour rule needs no memory, which is
  why this is new.
- The walk records join pairs where it records touches today. A DS-12 incremental detection records
  only its own walk's joins, so the commit unions the recorded pairs with the saved pairs of the
  districts in question, never with the walk's touches alone (a trap met while making the dump).
- `IndustryData.FORMAT` goes from 2 to 3 (`IndustryData.java:45`). The load path for an older format
  is §6's.
- `AdoptGameTests`' rule stays: an adopt gives the same districts a full detection gives.

### 2.4 Stations

A station is an anchor, not a seed (DS-10), and never a touch join (J1's exception), so a station
between two factories no longer glues them. Its chunk joins exactly one district (J6): the one its
docked interfaces serve, which is where its cargo moves. Train cargo is attributed to that district,
not to the district at the station's chunk as `TrainWatch` does today (`TrainWatch.java:218` at
arrival, `:305` after a restart), and inside it to sections as §4.4 says.

### 2.5 Package networks across districts

A **chain network** is the conveyors reachable by their `connections`, walked through `getChunkNow`
(§2.8). A chain network wholly inside one district changes nothing: the compile keeps its `CHAIN`
edges between frogports (`CreateCompile.frogportEdges` :1849), and no record is sent. A chain network
whose frogports sit in more than one district is a **package store** (Q4): the packages in transit,
owned by no district, with a port at each of its frogports. This is GA-2's shared store (G4), built
here first.

- **Identity.** Java keeps each package store in `IndustryData` (format 3): a persisted id given at
  first sight, its frogports (district, inventory, address filter) and its conveyor count, sent to Rust
  as a new `IND_PACKAGE_STORE` record when it changes. A re-walk that finds the same frogports keeps
  the id. When a network splits, the piece holding the lowest frogport position keeps the id, the rest
  get new ids, and what the store owes each frogport goes with that frogport; when networks merge, the
  lowest id stays and the contents add. A walk that stops at an unloaded chunk keeps the frogports it
  last saw beyond it.
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
- **Routing.** Create sends a package to the frogport whose address matches at the least routing
  distance, `*` addresses last, ties in insertion order (`ChainConveyorRoutingTable` lines 53 to 60
  and 74 to 95); addresses match as globs (`PackageItem.matchAddress` lines 185 to 192); a package no
  frogport matches loops for ever. The store keeps one pool per address it has seen, in goods and in
  packages. At each `IND_PACKAGE_STORE`, Rust resolves each address to the frogport Create's rule
  picks, from the frogports' filters and the route lengths along the walk; a pool whose address
  matches nothing is drawn by no one.
- **Room.** Create holds a package back only at the hand-off from one conveyor to the next, when the
  next one's looping list holds `chainConveyorCapacity` packages (20, `CLogistics:15`; the test at
  `ChainConveyorBlockEntity` line 117). A frogport's real export has no capacity check (lines 118 to
  123 of the export), and the travelling packages are not bounded (the test at lines 102 to 103 counts
  connections, not packages). So the store's room is the looping capacity, 20 per conveyor times the
  conveyors: an upper bound. Create's real limits are the frogports' 18 slots and the packagers.
- **Both ends away.** The exporter's replay books its exports into the pool of their address while the
  store has room; the importer's replay draws from the pools its frogport takes, no earlier than the
  route's travel time (`RPM / 360` blocks per tick along the chain, GLASSBOX.md §1). The store is a pool
  shared by the districts of one replay cluster; today every pool belongs to one district
  (`cluster.rs`, the module comment), so this is new (§10.3).
- **One end loaded, the other away** (Q5). Create hands a package to the next conveyor with no loaded
  check (`ChainConveyorBlockEntity.tick` :135, the `getBlockEntity` at :296), and vanilla's
  `Level.getBlockEntity` loads a chunk on the server thread (decompiled `Level.java:707` to `:713`), so
  today a package leaves the loaded side and stops in a conveyor whose chunk does not tick. Recommended,
  the G2-11 way:
  - **The frontier.** One mixin at that hand-off tests the next conveyor's chunk with
    `Level.shouldTickBlocksAt` (the test vanilla's block entity loop uses, `Level.java:508`) before
    Create looks it up. When it does not tick, the package stays where it is, as before a full
    conveyor (UNVERIFIED: what a refused package does; the first GameTest of slice S4 records it), and
    the next chunk is not loaded for it. A held package stays in the world.
  - **The ledger.** Rust keeps only a ledger: the store counts held packages per address, and the far
    district's replay draws them as G2-11's simulated side draws a live supply. The frontier journals
    no port: a held package was already its exporter's `CHAIN_OUT`.
  - **Paying back.** What the far district drew is owed. When its chunks load and its reconcile applies
    what it owes, the held packages it drew are removed from the chain, each found by a key the
    frontier wrote into the vanilla `minecraft:custom_data` component (a component type rtsim
    registered would not decode once rtsim is removed: UNVERIFIED for 1.21.1, so the vanilla one is used
    either way). A drawn package is never handed on: the frontier keeps it, and the landing mixin drops
    one that lands anyway. If its conveyor's chunk is not loaded at the reconcile, it is removed at
    that chunk's next load. A reset before then leaves it in the world: a duplicate at worst, never a
    loss (UR-1).
  - **The other way.** Packages the far district's replay sends to a loaded frogport are written into
    its inventory as real packages, room permitting, as mail is written into postboxes (INDUSTRY.md
    §8.2): one `CHAIN_IN` each, which the landing mixin does not record again.
  - **The limit.** Held packages fill the last ticking conveyors' looping room; past that the near
    factory backs up, as before a full chain. A long absence moves at most that room toward the far side.

### 2.6 Kinetic networks across districts

Chain conveyors carry rotation, so cutting districts at chains cuts kinetic networks between districts.
Inside a district nothing changes here (Q1 is about sections).

- **The driver bug (today).** The compile builds `enginesByNet` from the district's own engines
  (`CreateCompile.java:413` to `:418`) and asks `singleBoiler` (`:615` to `:626`) about them (`:452`,
  `:453`), which returns null for engines on several tanks. Two districts on one network see different
  drivers, or none, and a district with no engine of its own takes the network as one fixed level
  (`:481` to `:485`), which keeps running when the boiler stops.
- **What Java can see.** `KineticNetwork.sources` and `members` list only loaded block entities. A
  generator in a chunk that is not loaded is visible only when it is the network's root (Create's
  network id is the root's position, `CreateCompile.java:426`). The compile's other reads through
  `Level.getBlockEntity` (`:426` itself) load chunks and move to `getChunkNow` (§2.8).
- **The shared network** (Q2). A network whose members lie in chunks of more than one district is a
  shared network in Rust, owned by no district. Each district's compile reports **its own part**: the
  members in its own chunks with their stress at compiled speed, and its own sources with their
  capacity at each driver level (its boilers' 0 to 18 levels as tabulated today, `:454` to `:473`; its
  fixed sources' capacity). Members and sources in chunks of no district (chain conveyors on the way)
  are reported by the district holding the network's root, else by the lowest district id on the
  network, as last loaded. Rust keeps every part as its district last compiled it, so an unloaded
  district's part is known. No district is picked as "the source": the network runs while the summed
  capacity at every boiler's current level carries the summed stress, and stops otherwise.
- **Replay.** The districts on one shared network replay in one cluster (`Cluster`, as I13's power
  links already make clusters), stepped in time order. A district holding a boiler writes its level
  changes into the shared network as breakpoints of the cluster, and every district reads the network's
  state from it. This is glass-to-glass coupling across districts, new code (§10.3). G2-16's rule (a
  power link sends its districts to the black box, `engine.rs:1594` to `:1595`, its doc at `:1577` to
  `:1580`) does not apply: both sides model the network. A black-box district on a shared network
  couples as I13's `requires` does: its part runs while the network runs, and a source it holds counts
  only while its box processes run (the safe side).
- **Loaded districts.** A loaded district's network is Create's. The shared network matters for away
  districts; a loaded district's boiler level reaches it from that district's sweeps.
- **UNVERIFIED, tested first:** what Create's loaded half of a network does when the chunk holding its
  source unloads (keeps its last speed, or stops). The partly loaded case follows Create's answer.

### 2.7 Data coupling across districts

Factory gauges, stock links and redstone links never join districts. Gauge networks across districts are
I16's (warn, then fulfil); `GaugeNetworks` already warns. A redstone link from one district's switch to
another district's clutch is out of scope here (§14).

### 2.8 Reads that load chunks

Three reads in the compile go through `Level.getBlockEntity`, which loads a chunk on the server thread:
`CreateCompile.chainNetwork` (`:1877`) walking conveyors, the network root (`:426`), and a frogport's
face neighbours (`:1859`). With districts cut at chains and shafts, each would load other districts'
chunks during a compile. All three read through `getChunkNow` and stop at a chunk that is not loaded,
in slice S1, before anything splits.

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
  residual node per face-connected group of unclaimed block entities (a Java change, §10.3). A group
  joins the non-store nodes it touches; stores stay cuts.
- **Virtual nodes** (the void sink) have no position: they are in no node box (§3.6) and enter a
  fingerprint by kind only (§3.3). A residual group's box and fingerprint use the positions it holds.

### 3.2 Folding

The raw cut makes many small pieces: lone funnels, a belt between two vaults, a hopper chain. They are
folded so each section is a working line:

1. A **working** section holds a `MACHINE`, `HEATER`, `BOILER`, `SINK`, `SOURCE` or `BLACKBOX`.
2. A **transfer** section (movers, junctions, buffers only) folds into the nearest working section it
   reaches through one store, downstream first, then upstream; ties go to the section holding the lowest
   node position (never Java's node order).
3. Transfer sections that reach no working section merge with each other across the stores they share:
   a storage system (vaults, funnels and belts sorting goods between them) is one section.
4. An **idle** piece (no machine and no store) keeps no state; its nodes belong to no section and carry
   no flow in the solve.
5. If more than `glass.sectionsMax` (TUNABLE, 64) sections remain, the two smallest sections (by
   nodes) that share a store merge, else the two smallest that share a domain, else the two smallest
   in the district, until it holds. The same merge serves the row caps of §7.2.

Measured on the reference base (see the local notes), this leaves sections the size of working lines
and storage systems, and few shared stores.

### 3.3 Section state

| Field | Per | Notes |
|---|---|---|
| `section_of` | node | `u16`, `NONE` for idle nodes; recomputed, never saved alone |
| `fingerprint` | section | a canonical hash of the section's build (below) |
| window start | section | the sweep its open window began at; window length and sweeps are the district's |
| open window rows | (section, good) | `Acc` and `Cum` as today (`check.rs:47`, `:66`), in one sorted `Vec` keyed by (section, good), not a map per section; per-store rows keyed by inventory index as today (`check.rs:97`) and remapped by store position at a recompile |
| `passed`, `strikes`, `edited`, `fallback` | section | today's `Compiled.passed`, `strikes`, `edited` and `DistrictGlass.fallback`, moved per section |
| level run at | (section, domain) | the driver level each domain the section touches ran at in its passed window, for §3.6's domain check |

**The fingerprint** is FNV-1a 64 (`rtsim_kernel::rng::fnv1a64`, the hash the solve's regime key already
uses, `solve.rs:106`) over a canonical encoding sorted by position:

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
  residue at shared stores, ports with no inventory, the black box), and today's verdict for districts
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
- **A fallback freezes the sections it names, and the replay restarts over the rest** (SEC-1: each
  section replays on its own). A replay falls back when the regime has no fixed point, oscillates, sticks
  at a breakpoint or meets a stop it does not model (`Fallback`, `replay.rs:68`). The replay counts, per
  section, the watches that fired, the processes that blocked and the flags that did not settle since
  the last outside event (new). The sections those name freeze, and the replay restarts from that event
  over the remaining sections, at most once per section per replay. A fallback that names no section,
  or every section of the part, freezes the part, as a fallback freezes the district today.
- A frozen section holding a network's boiler stops that domain for every section on it, and for every
  district on a shared network (conservative: nothing runs on an engine the sim does not trust).
- **Partly loaded districts** (I7, G2-11). Sections whose nodes all lie in ticking chunks are live and
  learn. The others replay if the verdict allows; G2-11's cut runs between live nodes and the simulated
  nodes of replaying sections, as it runs between live and simulated nodes today. A section that may
  not replay has its simulated nodes gated off.
- A district whose sections all freeze is today's frozen district (I5).

### 3.6 Staleness

- **By node box only.** `IND_EDIT` (`IndRecords.java:71`) already carries the position (`Obs::Edit`,
  `obs.rs:467`). An edit stales the sections whose node box, grown by one block, holds the position,
  and a judging group as a whole when any section of it goes stale. A command edit (aux 4) stales the
  whole district. No kinetic network id is sent: belts, mixers and fans are kinetic block entities
  (`BeltBlockEntity extends KineticBlockEntity`, `MechanicalMixerBlockEntity extends
  BasinOperatingBlockEntity`), so staling by network would stale every line on a shared shaft.
- Stale sections stop replaying at once (I5, I7). The others keep their windows and their replay.
- **The recompile** (G-B3). The new compile's sections are matched to the held ones by fingerprint. A
  section whose fingerprint is unchanged keeps its passed window, strikes, replay standing and open
  window rows (remapped to the new node indices and store positions). A changed or new section starts
  learning; only those restart their open window, so a player who keeps building elsewhere does not
  hold back the lines he left alone.
- **The domain check.** A kept section on a domain stays passed while the domain's capacity (on a shared
  network, the sum over every part, §2.6) still carries the domain's stress at the level the section's
  passed window ran at. When it no longer does, the section goes stale. An edit in another district on
  a shared network changes that district's part only; Rust runs the domain check for every district on
  the network at once, with no recompile of theirs.
- A block placed outside every box stales nothing at once; the recompile finds the new or changed
  section, and the domain check catches its load on a network.

### 3.7 What stays per district

Loading and presence, ports and their journal, reconcile, threats, the black box, the district window,
the compile and its regime cache, the replay's cluster process set, and G2-81's compile numbering.

---

## 4. Accounting: the flows across a cut

### 4.1 Per section, per good

For a section `s`, a good `g`, its interior stores `I(s)` (every automation edge is to `s`), and the
shared stores `H(s)` (edges to `s` and to another section):

- **Measured:** `Y(s,g) = Σ_{i∈I(s)} ΔS(i,g) + Σ_{h∈H(s)} (in(h,s,g) - out(h,s,g)) - ports(s,g)`, where
  `in` and `out` are what `s`'s own movers put into and took from `h`, counted (§4.2), and `ports` are
  the ports of §4.4 that belong to `s`.
- **Predicted:** `P(s,g) = Σ_{i∈I(s)} pred(i,g) + Σ_{h∈H(s)} (Σ_{e: s→h} L(e,g) - Σ_{e: h→s} L(e,g))`,
  from the solve's per-store predictions (`Solution.per_store`, `solve.rs:167`) and its exact per-edge
  goods flows (`Solution.edge_goods`, `:178`). Bounds and variance follow by the same sums of `Cum`.
  Sections cut only at stores, so no goods edge runs between two sections.
- **Identity, tested.** For a shared store `h`, its change is what the sections' movers moved, plus
  ports into it, plus a **residue**: moves no counted mover made (a vanilla hopper, a player, another
  mod's mover). Summing over sections then gives the district:
  `Y(d,g) = Σ_s Y(s,g) + Σ_{stores in no section} (ΔS - ports) + Σ_h residue(h,g) - ports with no
  inventory`. Ports into shared stores cancel. `P` sums the same way with a predicted residue of zero,
  so a residue shows in the district's window, never in a section's.
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
  - fluids at `FluidNetwork.tick` :66, where a pipe network fills and drains its endpoints
    (UNVERIFIED: which position names the mover, the pump or the pipe end at the store).
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

The sections at a shared store whose residue over a window is not zero (a vanilla hopper, which uses
the `Container` directly; a player; a mover of another mod) cannot be judged apart for the goods
involved: they form a **judging group** for that window, judged on their summed rows. So do the
sections at a store beyond `glass.countedStoresMax` (§7.3), and the sections that share a train's cargo
(§4.4). A group is smaller than the district, and with Create's movers counted it is rare (measured on
the reference base, see the local notes). A group goes stale as a whole (§3.6).

### 4.4 Ports

- A port with an inventory (`PortTransfer.inventory`, `obs.rs:163`) belongs to the section of the node
  that holds or claims that inventory: a store's section when the store is interior, the mover's
  section for a frogport or postbox (they claim their inventory, §2.2). A port into a shared store is
  in no section's `Y` and cancels in the identity of §4.1.
- **Train cargo** is the whole train's change at departure, with no inventory (`TrainWatch.java:316` to
  `:328`). Portable storage interfaces look up no neighbour: they expose the train's storage, and the
  movers beside them pull and push. `IND_STATION_PORTS` (`TrainWatch.java:394`) names the district
  inventories the docked interfaces reach through conveyance; the cargo goes to the sections of those
  inventories: to one section as its port, or to several as a judging group for the cargo's goods. A
  split per interface would need a mixin on the interface's transfer (§10.3, not in these slices).
- A player's toss or pickup has no inventory and stays in the district's window.
- Frogport packages (§2.5) and postbox mail carry their inventory, so they always belong to a section.

---

## 5. Paging

- Sections live **inside** the district's glass page (G2-80, `persist::GLASS_TABLE` 0x4903), not in a
  table of their own: the per-section fields of §3.3, and the open window rows as one sorted array.
  `section_of` is not saved: it is recomputed from the compile's records on restore, as `Graph::build`
  rebuilds the indices.
- The page magic becomes `IGL2` (`glass/page.rs:32`). A restored page whose stored fingerprints do not
  match the sections recomputed from its compile (a changed tunable such as `sectionsMax`, or a changed
  fold) drops **that district's** glass state with one log line, and the district learns again. Only an
  undecodable page fails the whole restore, as G2-82 says.
- **Package stores** each get one page in the same page store (a new table next to `GLASS_TABLE`, id
  given at build): the pools per address, the held ledger, and what is owed per frogport. Shared kinetic
  networks hold no state of their own: they are rebuilt from the districts' compiles and replays.
- Rows are a `Vec` sorted by (section, good), never a map per section: a one-entry map node costs far
  more than its row.
- The page grows by the extra rows (each good counted once per section that touches it, instead of once
  per district). Today every page is written whole at every save (G2-83); writing only changed pages is
  For Ben 15, unchanged here.

---

## 6. Migration

No migration code (no backward compatibility before release; G2-85). On load:

- **`IndustryData` format 2 with districts.** Today an older format only warns and loads the districts,
  dissolving them as detection reaches them (`IndustryData.java:177` to `:180`). New: Java drops them,
  and the format change itself resets the industry engine through the host's fresh-start path (the one
  G2-82 uses), so Rust keeps no district Java no longer has, whether or not a glass page exists.
  Districts re-derive from the seed attachments at the first detection.
- A glass page with magic `IGL1` is refused by name; after the reset above there is none to restore.
- The operator sees one line at start: "rtsim industry: the save format changed (sections, SEC-1); learned
  factory state was reset and districts are detected again." Nothing is lost silently, and only the
  sim's own knowledge is lost (UR-1).
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
- The rows that are new are the goods of stores shared by sections, counted once per section. Measured
  on the reference base, the extra memory is small next to today's glass state (see the local notes).
- **Bounds.** Per district, `glass.sectionRowsMax` (TUNABLE, 1,024 rows, about 350 KiB, ESTIMATE);
  per world, `glass.sectionRowsWorldMax` (TUNABLE, 16,384 rows, about 5.5 MiB, ESTIMATE). Over either,
  the district with the most rows merges sections by §3.2 step 5 at its next compile: coarser
  sections, never lost state. With one section per district, the rows are today's.
- **Saves.** Every glass page is written whole at every save today (G2-83); the extra rows add to that
  until For Ben 15 writes only changed pages. Package store pages are small: a pool per address.
- **Clusters.** Every district on one shared network or package store replays in one cluster. The
  work is the sum of the members' replays, but a cluster settles as one model, and its catch-up costs
  its breakpoints times its diffusers (`cluster.rs`, the module comment: one multiply per diffuser per
  settle). Coupling n districts of similar size costs about n times more per breakpoint than replaying
  them apart. `industry.clusterDistrictsMax` (TUNABLE, 16) bounds it: a shared object whose districts
  exceed it is not coupled. Its away districts then take a shared network as stopped, and a package
  store moves nothing while either end is away.
- The replay costs what it costs today inside a district: parts are formed as today, over the
  replaying nodes. A fallback's restart repeats at most one replay step per frozen section.

### 7.3 Counters (Java)

- One hash test per real move at a mover the compile models (is this mover counted), and one counter
  update when it is. Counted movers are only those at shared stores, which the fold keeps rare. A cap
  `glass.countedStoresMax` (TUNABLE, 256 per dimension) turns the rest into judging groups.
- The frontier adds one `shouldTickBlocksAt` test per hand-off between conveyors, and saves a chunk
  load for each package it holds.

---

## 8. Reach

### 8.1 Commands

- `/rtsim district sections [id]`: per section, its nodes, stores, verdict (learning, passed, failed,
  stale, frozen, replaying), the last window's worst good, and the shared stores with their counting
  state and residue. `/rtsim district info` gains one line: sections, and how many replay.
- `/rtsim district graph <id>`: writes the district's compile, sections, join pairs, package stores and
  shared networks to a file in the server's debug folder, the operator command the dump of §1 needed.
  It reads only what the engine and the index already hold.

### 8.2 rtsim-cli

`rtsim-cli glass <world> [district] [--json | --graph]`: the paged glass state of a saved world, read
only through the read-only open, as `rtsim-cli lines` reads train lines (`crates/rtsim-cli/src/lines.rs`).
`--graph` writes the compile with `section_of` for offline checks (§9.4). `rtsim-cli stores <world>`
lists package stores with their pools and held ledger.

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
| `twoFactoriesSideBySideStayTwo` | two press lines in neighbouring chunks, nothing between them | two districts (today one) |
| `aVaultAcrossABorderJoinsItsChunks` | a packager in one chunk under an item vault whose controller is in the next | one district (J3) |
| `diagonalCogsAcrossABorderJoin` | a large cog meshing diagonally with a small cog across a border | one district (J4) |
| `anArmReachingAcrossABorderJoins` | a mechanical arm taking from a depot in the next chunk | one district (J2) |
| `aChainInsideOneDistrictStaysEdges` | two frogports of one factory on a chain inside it | `CHAIN` edges, no package store, no `CHAIN_*` record |
| `aChainBetweenTwoFactoriesLeavesThemTwo` (exists as `aChainBetweenTwoDistrictsLeavesThemTwo`) | two factories on one chain, frogports at both ends | two districts; one package store; every package a `CHAIN_OUT` at one end and a `CHAIN_IN` at the other; both stay glass; both windows balance |
| `twoFrogportsOfOneDistrictOnASharedChain` | a third frogport of the first factory on the same chain | its packages to the first frogport are counted once each way, with no `CHAIN` edge |
| `anUnloadedMemberKeepsItsJoins` | a district with one member chunk unloaded before a detection | the same district after it (saved pairs) |
| `aStationBetweenTwoFactories` | a station touching two factories, its interfaces serving one | two districts; the station's chunk and its cargo with the one its interfaces serve |
| `aBoilerOnAChainDrivesTheNextDistrict` | a boiler and engines in one factory, a chain carrying rotation to a press line | both compiles report parts of one shared network; away, the press line stops when the boiler starves |
| `aPackageIntoAChunkThatDoesNotTick` | a chain from a ticking chunk into one that does not tick | first records what Create does (§2.5); then, with the frontier, the package waits on the last ticking conveyor and the next chunk is not loaded |
| `aHeldPackageDrawnIsRemovedOnce` | the far district drew a held package | after its reconcile the package is gone from the chain, and it never lands; with a reset before the reconcile it is still in the world |
| `aNetworkWhoseSourceChunkUnloads` | a boiler network across two districts, the source's chunk unloaded | records Create's behaviour (§2.6) |
| `AdoptGameTests`, `RescanBudgetGameTests`, `DistrictSeedGameTests`, `CreateSeedGameTests` | existing | still pass: adopt equals a full detection, the budget holds |

### 9.2 GameTests, sections (Level 2)

New `CorpusFactory` fixtures next to `PackageLoopFactory` and `BoilerOverstressFactory`:

| Fixture | What it is | Checks |
|---|---|---|
| `VaultBetweenMixersFactory` | a vault between two brass mixing lines (one fills it by funnel, one draws by funnel) | two sections, the vault shared and counted at its funnels; each passes its own window; break and replace the second mixer: the first keeps its passed window and replays while away |
| `HopperAtASharedVaultFactory` | the same with a vanilla hopper also feeding the vault | a residue, so the two sections form a judging group |
| `BeltIntoPressFactory` | a belt straight from a press into a saw | one section (SEC-1's "cannot be split") |
| `SharedBoilerTwoLinesFactory` | one boiler network driving a crushing line and a press line that share nothing else | two sections on one domain (Q1 a); an edit to the press line's belt stales only it; adding presses until the level the crushing line's window ran at no longer carries the stress stales the crushing line too |
| `StationPressLineFactory` | a station's portable storage interface emptied by a funnel into a vault feeding a press line (`StationCargoGameTests`' kit) | the cargo maps through `IND_STATION_PORTS` to the press line's section |
| `FrozenNeighbourFactory` | two lines on a vault, the first made to oscillate | the fallback names the first section, which freezes; the replay restarts, and the second goes on drawing from the vault until it is empty |
| `HalfLoadedFactory` | two lines in one district, one in chunks out of simulation distance | the ticking line learns and passes; the other replays if passed |

Each runs the corpus cycle: loaded for a window, away for a replay, back for a reconcile that matches.

### 9.3 Engine tests (`glass/tests.rs`, `cluster.rs` tests)

- The cut and the fold on hand-built graphs: a vault between two mixers, a belt into a machine, a
  shared domain, a transfer chain, a storage system, idle movers, residual groups, `sectionsMax` and
  the merge fallbacks.
- Fingerprints: the same factory with other state slots keeps every section; the same factory with its
  nodes in another order gives the same fingerprints; one changed section restarts alone; a machine
  added elsewhere on a domain changes no fingerprint; the domain check stales only past the level run at.
- Accounting: the identity of §4.1 on random graphs, with ports into shared stores and residues; counted
  and uncounted shared stores; judging groups; ports by inventory; the verdict rule of §3.4.
- Replay: a gated-off section's stores are touched only by its neighbours; a fallback freezes the
  sections it names and restarts once per section; a frozen boiler stops its domain; per-section
  liveness; G2-84's paging tests extended to sections (a save and restore before each step, equal pages
  and commands).
- Shared objects: a shared network's level from parts in three districts, two with boilers; a package
  store's split and merge; routing by the least distance with `*` last; room; the held ledger and what
  is owed; `clusterDistrictsMax`.
- The page: `IGL2` round trip; a page whose fingerprints disagree with its compile drops only its
  district's glass state.

### 9.4 The reference base, offline

The dump of §8.1 and `rtsim-cli glass --graph` let the reference base's numbers in the local notes be
repeated on the laptop copy after each slice. That check stays local; nothing from it is committed.

---

## 10. Build plan

### 10.1 Slices

| Slice | What | Tests | Depends on |
|---|---|---|---|
| **S1** | The three reads through `getChunkNow` (§2.8). The driver decided over the network's sources; a network with sources outside the district (in another district, or an unloaded root) is marked as today's unstable domains are (`CreateCompile.java:487`, `:641`: its members are bounded, their rates learned), the same in every district on it, so nothing replays a driver it cannot see. `/rtsim district graph`; `rtsim-cli glass` | `aCompileLoadsNoChunk`, `aBoilerOnAChainDrivesTheNextDistrict` (driver part), the CLI on a GameTest world | nothing |
| **S2** | Sections in the engine: cut, residual groups (Java), fold, state, fingerprints, per-section windows and liveness, the verdict rule, judging groups where nothing is counted, staleness by box and the domain check, per-section freeze and restart, `IGL2` with the per-district drop; train cargo through `IND_STATION_PORTS` | §9.3, `BeltIntoPressFactory`, `SharedBoilerTwoLinesFactory`, `FrozenNeighbourFactory`, `StationPressLineFactory`, `HalfLoadedFactory` | nothing: sections inside today's districts already pay off |
| **S3** | Counting at the movers (Q3): `IND_COUNT_SET`, `IND_COUNT_SAMPLE`, the mover mixins | `VaultBetweenMixersFactory`, `HopperAtASharedVaultFactory` | S2 |
| **S4** | Package stores (Q4): the store record and identity in Java, `CHAIN_OUT` and `CHAIN_IN` at the frogports of package stores, no `CHAIN` edges on them, pools per address with routing and room, the both-away replay (the first pool shared between districts in a cluster); then the frontier and the ledger (Q5) | `aPackageIntoAChunkThatDoesNotTick` first, then `aChainInsideOneDistrictStaysEdges`, `aChainBetweenTwoFactoriesLeavesThemTwo`, `twoFrogportsOfOneDistrictOnASharedChain`, `aHeldPackageDrawnIsRemovedOnce`, the store's engine tests | S1. Useful before S6: chains between districts that are apart today are counted nowhere since DS-10 |
| **S5** | Shared kinetic networks (Q2): parts per compile, the summed level, coupling in one cluster | `aNetworkWhoseSourceChunkUnloads` first, then `aBoilerOnAChainDrivesTheNextDistrict` (away part), the shared network's engine tests | S1, S2 (the domain check) |
| **S6** | Level 1: joins J1 to J6, no bare adjacency, saved join pairs, the station rule, `IndustryData` format 3, the drop and the engine reset; the stale comment at `CreatePlugin.java:169` to `:171` | §9.1's join and station rows, and the existing detection tests | S1, S4, S5: no district is split before its packages and its network have somewhere to go |
| **S7** | Reach: `sections` command, info line, `rtsim-cli stores`, viewer layer and card | command GameTests, rtsim-view fixtures | S2 |

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
| `getChunkNow` at the compile's three reads | they load chunks today |
| The package store: Java's record and identity, `CHAIN_OUT` and `CHAIN_IN` and their engine path, pools per address with Create's routing, a pool shared between districts | G4 is not built; every cluster pool belongs to one district; the compile models no addresses |
| Two frogport mixins | Create has no event for a package leaving or landing |
| The frontier mixin, the ledger, the custom data key and the removal at reconcile (Q5 a) | Create hands packages into chunks that do not tick |
| Shared kinetic networks: parts per compile, the summed level, glass-to-glass coupling across districts in a cluster | each compile sees only its own engines; G2-16 sends power-linked districts to the black box |
| Residual groups in `GraphBuilder` | one residual per district glues every line with an unknown block |
| Sections in Rust (cut, fold, fingerprint, rows, verdict rule, domain check) | the window, the verdict and the staleness are per district today |
| Per-section fallback naming and the restart | a fallback freezes the whole district today |
| The mover mixins and the count records | the sweep reads levels, not flows |
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
3. **Sections.** The brass works has two working sections: the washing line (vault A to vault B; the
   station's cargo maps to it through the funnel at the interface) and the mixing line (vault B to vault
   C, with the packager and frogport folded in downstream). Vault B is shared, and its funnels count
   their moves. The press hall has one section (vault D to vault E). `/rtsim district sections` lists
   them, each "passed" after a loaded window.
4. **An edit.** Ana breaks a mixer to put a third one in. The mixer's box is the mixing line's, so the
   mixing line goes stale; the washing line does not, and the lighter load on the shaft passes the
   domain check. She flies off before a new window passes. While she is away the washing line replays
   and vault B fills; when it is full the fan line stops, as Create's would. The mixing line is frozen,
   so no brass leaves on the chain, and the press hall uses up vault D and stops. Before sections, the
   whole brass works froze at the edit.
5. **Back home.** Ana returns; the reconcile writes vault B's level and vault D's emptiness into the
   world. The mixing line passes a window with three mixers, and the next trip away replays all three
   sections. If the third mixer needs a higher boiler level than the washing line ran at, the domain
   check stales the washing line too, and it learns again in the same window.
6. **The boiler.** If Ana starves the boiler of water while both buildings are away, its engines lose
   capacity, the shared network overstresses, and the press hall's replay stops with the brass works'.
   While a building is loaded, Create's own network does the same.

---

## 12. Questions for Ben

1. **Kinetic networks inside a district.** One shaft network often runs every machine in a building.
   - **(a) Recommended. A shared input.** Sections cut through a network; each takes its speed from
     it, and goes stale when the network can no longer carry its stress at the level its window ran at.
     Cost: a network is one solve unit, so the sections on it replay in one part; a failure freezes only
     the section it names, and the part restarts over the rest (§3.5). Measured on the reference base
     (see the local notes), this keeps sections the size of working lines.
   - (b) One section per network, as SEC-1's "a shared shaft network cannot be split" reads literally.
     A building run from one network is then one section, and sections gain little there (measured on
     the reference base, see the local notes).
   - (c) Split only fixed-speed networks (water wheels, motors); a boiler network joins its sections.
2. **Rotation across a district cut.** A chain conveyor carries rotation as well as packages.
   - **(a) Recommended. Districts stay apart on a shared network:** each district reports its own part,
     the level is decided over all parts, and the districts on the network replay together. Cost: new
     glass-to-glass coupling across districts (§10.3), and a cluster's catch-up costs its breakpoints
     times its diffusers, bounded by `industry.clusterDistrictsMax` (§7.2).
   - (b) A chain that carries rotation joins districts. A network that runs through a whole base then
     joins its districts back into one, the giant district GA-2 turned down (measured on the reference
     base, see the local notes).
   - (c) No coupling: a district without the boiler takes the network at one fixed level, as today. It
     keeps running when the boiler stops, making more than Create would.
3. **How flows into a store shared by two sections are counted.**
   - **(a) Recommended. At Create's movers:** mixins where funnels, chutes, arms and pipes move real
     goods; the mover's position names its section. Other mods' movers at a shared store are not
     counted, and their sections are judged together (a judging group).
   - (b) At the store, by a counting wrapper on NeoForge's capability lookup: counts every mod's movers,
     but it must subclass Create's `VersionedInventoryWrapper` or funnels and chutes rescan every tick,
     cached handlers take up to 64 ticks to see it, and arms cannot be told from a funnel on top
     (§4.2).
   - (c) No counting: the sections at a shared store are always judged together, and only stores with
     a single section on each side split cleanly.
4. **What holds packages between districts.**
   - **(a) Recommended. A package store owned by no district** (GA-2's shared store, built here
     first): its own saved data (an id, its frogports, pools per address, paged), Create's room and
     routing, and a pool the districts on it share in one replay cluster.
   - (b) No store: a `CHAIN_OUT` is booked as the importer's import after the travel time, routed at
     export. No new shared object, but packages for a full or away importer have nowhere to wait, so
     the exporter's replay stops at once, and a partly loaded chain gets no frontier.
5. **Packages on a chain while exactly one end is loaded.** G-B6 says flow across a loaded and unloaded
   edge is modelled.
   - **(a) Recommended. Modelled, the G2-11 way:** packages wait in the world on the last ticking
     conveyor, the far district's replay draws them through a ledger, and they are removed when that
     district's reconcile pays back what it drew; packages it sends to a loaded frogport are written in
     as real packages, as mail is. A reset leaves at worst a duplicate, never a loss. The flow toward
     the far side is bounded by the waiting room.
   - (b) Zero while one end is loaded: packages wait on the chain as in Create, and the far factory
     starves or backs up while you stand at the near one. Simpler; both-away flow still works.
   - (c) Rust takes the packages into the store: unbounded flow, but Rust then holds the only copy of
     a player's items until the far side loads, an exception to UR-1 only you can grant.
6. **Packagers at a district cut.** SEC-1's list of countable links names packagers.
   - **(a) Recommended. Cut at the chain, not the packager:** a packager joins the store it serves,
     and its packages are counted at the frogport or postbox they go through.
   - (b) A packager touching a block entity across a chunk border does not join: its chunk and its
     target's chunk are separate districts, and what it takes from the target crosses between them
     uncounted unless a new port is added at the packager (a mixin on its extraction).

---

## 13. Choices (Claude's, reversible)

| # | Choice | Why |
|---|---|---|
| SC1 | Joins J1 to J6; bare 8-adjacency is gone | SEC-1; measured on the reference base (see the local notes), adjacency held big districts together where nothing physical joined them |
| SC2 | J3: a multiblock inventory across a border joins its chunks, with a recorded pair | physical joins alone cut goods edges there (measured on the reference base, see the local notes) |
| SC3 | J4 uses Create's own `isConnected`, for border kinetic blocks and overriding classes only | Create's rule, at a cost bounded by the border |
| SC4 | Arms and ejectors are plugin links, like fans | they move goods across borders without touching |
| SC5 | Join pairs saved per district, replaced only between chunks looked at; format 3 | unloaded members must keep their joins (DS-11) |
| SC6 | A station never joins by touch, and its chunk joins exactly one district, the one its interfaces serve; its cargo goes there | a station between two factories must not glue them, and cargo belongs where it moves |
| SC7 | Package store frogports send `IND_PACKAGE` with `CHAIN_OUT` and `CHAIN_IN` on a new engine path; a chain inside one district keeps its `CHAIN` edges | the mail path forces the black box; one count per crossing |
| SC8 | Package store identity, split and merge rules; room from the looping capacity; routing by Create's least distance with `*` last | Create's own rules (§2.5) |
| SC9 | The frontier tests `shouldTickBlocksAt`, holds the package, keys it in `minecraft:custom_data` | Create loads the next chunk today; a mod component would not survive an uninstall |
| SC10 | The three compile reads through `getChunkNow`, in S1 | they load chunks today |
| SC11 | Until S5, a network with sources outside the district is an unstable domain in every district on it | defined behaviour that exists today, the same everywhere |
| SC12 | A shared network has no source district: capacity and stress summed over every district's part; members in no district reported by the root's district | several districts can hold boilers; picking one flips as levels change |
| SC13 | Sections cut only at `STORE` nodes; `CHAIN` and `LINE` edges join | SEC-1's stores; `LINE` is a depot's own chain |
| SC14 | Residual groups, one per face-connected group of unclaimed block entities; virtual nodes have no position | one residual per district glues every line |
| SC15 | The fold: transfer into working, downstream first, ties by lowest position; storage systems merge; idle pieces keep nothing; merges by store, then domain, then size | sections are working lines; shared stores become rare; the cap is always reachable |
| SC16 | `glass.sectionsMax` 64, `glass.sectionRowsMax` 1,024, `glass.sectionRowsWorldMax` 16,384, `glass.countedStoresMax` 256, `industry.clusterDistrictsMax` 16 (TUNABLE) | weak-PC bounds |
| SC17 | The fingerprint: FNV-1a 64 over every build field, recipe and edge, and only the section's own nodes on a domain | stable across node order; a machine elsewhere on a network changes nothing |
| SC18 | Rows in one sorted `Vec` per district, store rows remapped by position | a map node per one-entry section costs more than its row; inventory indices change by revision |
| SC19 | A recompile keeps every section whose fingerprint did not change, open window included; only changed sections restart | G2-81's rule, per section; a builder elsewhere does not hold lines back |
| SC20 | Edits stale by node box grown by one; judging groups stale whole; commands stale all; the domain check runs at recompile and across districts on a shared network | positions are in `IND_EDIT` already; every Create line is kinetic |
| SC21 | Liveness per section | a section learns while another part of its district is out of range |
| SC22 | The verdict rule: a section replays only while it passed and the district window did not fail on its goods | the district window stays the checksum |
| SC23 | A fallback freezes the sections it names and restarts the replay once per section; a part when it names none | SEC-1: each section replays on its own |
| SC24 | The district window stays as the checksum and judges residues and ports with no inventory | nothing is lost against today |
| SC25 | `IGL2`, sections recomputed on restore, a fingerprint mismatch drops only that district's glass state | a tunable change must not reset the world's learning |
| SC26 | Migration is a loud reset: Java drops old districts and the format change resets the engine | no backward compatibility before release; UR-1 |
| SC27 | The black-box learner stays per district | it is the fallback and the checksum |
| SC28 | A gated-off node keeps its stress; a frozen boiler stops its domain everywhere | Create's stress does not depend on work; conservative |
| SC29 | Train cargo to the sections `IND_STATION_PORTS` names; a judging group when several | interfaces expose the train's storage; movers do the moving |
| SC30 | Counting starts at the first sweep that carries samples after a count set changes | no window mixes counted and uncounted sweeps |

---

## 14. Risks and unverified points

- **What a refused package does at the frontier** (§2.5) and **what a network's loaded half does when
  its source chunk unloads** (§2.6) are UNVERIFIED. Each is the first GameTest of its slice, and the
  slice follows what it shows.
- **Create's chain conveyors load neighbour chunks today.** `ChainConveyorBlockEntity.tick` calls
  `Level.getBlockEntity` on connected conveyors with no loaded check: at :161 for routing adverts
  (about every 80 ticks, `ChainConveyorRoutingTable.shouldAdvertise`, its line 49) and at :296 for the
  hand-off. A chunk that "does not tick" is one Create itself loads for a moment, so the frontier tests
  block ticking, not presence. The frontier removes the hand-off's load for held packages; the adverts
  still load, a weak-PC cost that exists today. Stopping them changes Create's routing and is not part
  of this design.
- **Fluid counting** (§4.2): which position names a pipe network's mover is UNVERIFIED. Until it is, a
  store shared by two sections through pipes is a judging group.
- **Movers of other mods** at a shared store are not counted; their sections are a judging group
  (§4.3).
- **The custom data key** on a held package: that a mod-registered component would not decode after an
  uninstall is UNVERIFIED for 1.21.1; the design uses the vanilla component either way.
- **Redstone links between districts** (a switch in one, a clutch in the other) are data coupling the
  compile does not model across districts. Splitting districts makes this more common; until it is
  modelled, the gated machine takes its compile-time state.
- **Members and sources in chunks of no district** on a shared network are reported as last loaded by
  one district's compile (§2.6); a change there while that district is away is not seen until it
  compiles again.
- **Section counts on other bases** may differ from the reference base: `sectionsMax` and the row caps
  bound the cost either way, at the price of coarser sections.
- **Stores shared by many sections** are rare after the fold (measured on the reference base, see the
  local notes); the design handles them (counting at movers, groups) but they are untested at scale.
