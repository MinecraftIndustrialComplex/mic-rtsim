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
  district, no migration), **G2-22** (replay parts), **G2-11** (partial-load cut), **G-B6** (Ben: flow
  across a loaded and unloaded edge is modelled, not zero).
- **GA-2** (storage networks and wireless links are one shared store in Rust with ports, G4; districts
  stay small), **I13** (PowerGrid lines couple running states; Ender Transmission is local only),
  **I16** (gauge networks across districts: warn, then fulfil), **I7** (the unloaded part of a partly
  loaded district keeps being simulated), **I5** (stale districts freeze while unloaded).
- **UR-1** (rtsim can be deleted and reinstalled at any time; no save-format promises; a reset loses
  only the sim's own knowledge), **SRV2**, **DR-0**, and the rule of no backward compatibility before
  release.
- **D1:** performance on a weak PC is the only judge.

**Sources.** Create facts are `javap` on the pack's `create-1.21.1-6.0.10.jar`, with line numbers from
its `LineNumberTable` (a method's first line, or the line of the call named). NeoForge facts are
`javap` on `neoforge-21.1.249-universal.jar`. Vanilla facts are the decompiled 1.21.1 sources in the
NeoForm cache. Addon facts are `javap` on the pack's jars (create_connected 1.3.3, Ender Transmission
2.1.1, PowerGrid 0.6.2). Repository facts name the file and line at `0bc2f02`. Anything not read from a
jar or the code is marked UNVERIFIED.

**Numbers:** every number is a verified fact, arithmetic from one, a labelled TUNABLE, or a labelled
ESTIMATE. Where a number from the reference base matters, the text says "measured on the reference
base, see the local notes": those notes stay outside the repository, with the dump and the scripts
that made them.

---

## 0. What changes and what stays

| Part | Today | With sections |
|---|---|---|
| What joins two chunks into one district | a face touch of industrial block entities across the border, a plugin link (fans, Ender channels), and **any 8-neighbour member chunk** (`Districts.commit`, the loop at `Districts.java:910` to `:915`) | a face touch, a plugin link, one inventory spanning the border, rotation across the border that is not a face touch, an arm or ejector reaching across; never bare adjacency (§2.1) |
| Chain conveyors | not a link since DS-10, but chunks still join by adjacency, so a short chain usually sits inside one district | never a join; the packages on a chain network that spans districts are a shared store with a port at every frogport (§2.5) |
| Packages at frogports | not counted (`IndRecords.Port.PACKAGE` is defined and never sent) | an `IND_PACKAGE` record at the frogport's inventory for every package that leaves onto a chain or lands off one, as `MailWatch` does for postboxes (§2.5) |
| A kinetic network shared by districts | each district's compile decides its driver from its own engines only (`CreateCompile.java:413` to `:418`, `:452`); districts disagree | the driver is decided over the network's sources; the other districts take the network as an input that runs while the driving district does (§2.6) |
| The unit that learns, replays and goes stale | the district | the **section**: a working line between stores; the district stays the unit of loading, ports, the black box and the checksum (§3) |
| An edit | discards the whole district's glass state (`engine.rs:823` to `:852`, `glass/mod.rs:210`) | stales the sections near the edit; the recompile keeps every section whose build did not change (§3.6) |
| A replay that cannot go on | the district falls back to the black box or freezes | the sections in that replay part freeze; the rest go on (§3.5) |
| Measurement | one window per district per good | one window per section per good, plus the district's own window as the checksum; flows into stores shared by sections are counted by store face (§4) |
| Saved state | `IndustryData` format 2; glass page `IGL1` | `IndustryData` format 3 (join pairs); glass page `IGL2` (sections inside the district's page); a loud reset on load (§5, §6) |

---

## 1. What the reference base showed

A copy of a large survival base built with the MIC pack was loaded on the laptop with the stub
engine and a local hook that dumped every district commit and every glass compile. The live server was never touched. The hook is not
committed; §8.1 makes the same dump an operator command. In general terms:

1. **Bare adjacency holds the big districts together.** With only physical joins, a big district
   splits into a few large pieces and some small ones, and no chunk is left without a seed. Measured
   on the reference base, see the local notes.
2. **The only goods edges the split cut were a packager under a tall item silo whose controller sits in
   the next chunk.** A multiblock inventory across a border is not a face touch of two block entities
   in the walk's sense, so "one inventory spanning chunks joins them" (J3) is required.
3. **One chain-carried, boiler-driven kinetic network spans most of the base's districts.** Chain
   conveyors carry rotation (`ChainConveyorBlockEntity.addPropagationLocations` :660,
   `propagateRotationTo` :667), and many chain conveyors stand in chunks that belong to no district.
   The districts on that network disagreed about its driver: the driver bug of §2.6.
4. **The frogports of one chain network end up in several districts after the split.** Most of their
   chain edges (frogport to frogport, one per ordered pair) then cross districts, so package networks
   across districts are the normal case, not an edge case (§2.5).
5. **If kinetic networks joined sections, the shared network would glue most machines of each big
   district into one section.** With networks as a shared input instead (Q1, recommended), and the fold
   of §3.2, sections are working lines plus storage systems, and stores shared by sections become
   rare. The edges at shared stores were face-adjacent (a funnel, a chute, a pipe), so counting by
   store face identifies them (§4.2). Measured on the reference base, see the local notes.
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
| J3 | **One inventory across chunks:** a store whose canonical position (`canonicalInventory`, `CreatePlugin.java:174`: an `IMultiBlockEntityContainer`'s controller, a double chest's lower half) lies in another chunk joins the two chunks; an item vault (`ItemVaultBlockEntity`), a fluid tank, a create_connected item silo (`ItemSiloBlockEntity implements IMultiBlockEntityContainer$Inventory`) or fluid vessel | the walk already canonicalizes every candidate (`Districts.java:1260`) | none extra |
| J4 | **Rotation across the border that is not a face touch:** diagonal cogs (`KineticBlockEntity.addPropagationLocations` :559 adds the diagonals when the cog can propagate diagonally) and any class that overrides `addPropagationLocations` (Ender Transmission's `EnergyTransmitterBlockEntity` :33), confirmed by Create's own rule (`RotationPropagator.isConnected`, public, :403); chain conveyors excluded (§2.6) | the walk, for kinetic block entities within one block of a border only, and for overriding classes (a per-class cache) | a few neighbour reads per border cog, counted in the DS-11 budget |
| J5 | **Merge links** (`/rtsim district merge`) and **pins** (`split`), unchanged | `IndustryData` | none |
| J6 | **A station's chunk** joins the district beside it when it has no seed of its own (DS-10's anchor rule, unchanged: 8-adjacent or linked by what touches it) | the walk's station roots | none |

Bare 8-neighbour adjacency is gone: the loop at `Districts.java:910` to `:915` is replaced by a union
over the joins the walk recorded. `grown()` (`:867`) still grows an incremental detection's frontier
by the 8-neighbour districts, because an edit in one chunk can create a join to the next.

### 2.2 The cut points, per mod

| Mod | Block (class) | Level 1: districts | Level 2: sections | Where it is counted |
|---|---|---|---|---|
| Create | chain conveyor (`ChainConveyorBlockEntity`) | **cut**: no touch join; its packages are a shared store (§2.5); its rotation is a rotation link (§2.6) | not a node | at the frogports |
| Create | frogport (`FrogportBlockEntity extends PackagePortBlockEntity`, an 18-slot package inventory) | stays in its district by the touches to its packager and inventory | its inventory is a store (`KIND_POSTBOX`, `CreatePlugin.java:192`): a cut | export onto the chain: `PackagePortTarget$ChainConveyorFrogportTarget.export` :114 (the conveyor's `addLoopingPackage` :122 or `addTravellingPackage` :123, on a call that is not a simulation); landing: `FrogportBlockEntity.tick` :155, the `insertItem` at :212 after the catch animation (`startAnimation` :223, started by `ChainConveyorBlockEntity.exportToPort` :361) |
| Create | packager (`PackagerBlockEntity`) | **not a cut**: it joins its target inventory by touch, and J3 when the target spans chunks | a mover between its target store and its frogport | its packages, at the frogport or postbox they go through |
| Create | train station (`StationBlockEntity`) | **cut** by touch; an anchor (J6) | its portable storage interfaces face stores: cuts | cargo at `Train.arriveAt` :954 and the PSI's dock (`PortableStorageInterfaceMixin`, `TrainWatch`), today an `IND_PORT` with no inventory (`TrainWatch.java:328`) |
| Create | postbox (`PostboxBlockEntity`) | a member of its district (an inventory) | a store: a cut | mail at `GlobalStation.runMailTransfer` :167 (`MailWatch`, `IND_PACKAGE` with the postbox's inventory), unchanged |
| Create | item vault, fluid tank (`ItemVaultBlockEntity`, `FluidTankBlockEntity`) | J3 across a border | a store: a cut, unless it is a boiler (a `BOILER` node joins its engines) | by face (§4.2) where two sections share it |
| Create | chest, barrel (stock tag) | J1 only with a non-store neighbour | a store: a cut | by face; a vanilla hopper bypasses capabilities (§4.3) |
| Create | belt, funnel, tunnel, chute, pipe, pump, depot, basin, spout, drain, press, mixer, saw, deployer, crafter, millstone, crushing wheel, fan, arm, burner, steam engine | J1 (and J2 for arms, ejectors, fans) | **never a cut**: a belt straight into the next machine is one section (SEC-1) | inside its section's window |
| Create | shaft, cog, gearbox, encased chain drive, clutch, gearshift, water wheel | J1, J4 | not nodes: members of a kinetic domain, a shared input (Q1) | not counted |
| Create | factory gauge, stock link, stock ticker, redstone requester (`LogisticallyLinkedBehaviour`) | **no join**: data coupling, I16 (`GaugeNetworks` warns) | a controller in the section it gates | not goods |
| Create | redstone link | **no join**: data coupling | not modelled across districts (§14) | not goods |
| create_connected | item silo, fluid vessel (`ItemSiloBlockEntity`, `FluidVesselBlockEntity`) | J3 | a store: a cut | by face |
| create_connected | inventory bridge, access port (`InventoryBridge`, `InventoryAccessPort`) | J1 | one inventory's nodes join (the proxied store is the store) | by face at the proxied store (UNVERIFIED: the proxy's own capability path) |
| create_connected | kinetic bridge (`KineticBridgeBlockEntity`, `KineticBridgeDestinationBlockEntity extends GeneratingKineticBlockEntity`) | J1 (the two halves are adjacent) | two networks already: the destination is a generator fed by the source's network, the same shape as §2.6's rotation link | not counted |
| Ender Transmission | item and fluid transmitters, energy transmitters (`EnergyTransmitterBlockEntity.isCustomConnection` :23, `addPropagationLocations` :33) | J2 (`ChannelLinks`), J4; local only (I13) | the channel buffer is a store: a cut | by face at the buffer |
| PowerGrid | wires (entities), batteries | no join; `IND_POWER_LINK` couples districts (I13, `PowerLinks`) | a battery is an energy store: a cut; a `LINE` edge is a cut | the link record, unchanged |
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
  districts in question, never with the walk's touches alone (a trap the dump met).
- `IndustryData.FORMAT` goes from 2 to 3 (`IndustryData.java:45`). The load path for an older format
  is §6's.
- `AdoptGameTests`' rule stays: an adopt gives the same districts a full detection gives.

### 2.4 Stations

Unchanged from DS-10: a station is an anchor, not a seed; its chunk joins the district beside it; train
cargo is attributed through the district at the station's chunk. A station is never a touch join (J1's
exception), so a station between two factories no longer glues them.

### 2.5 Package networks across districts

A chain network (the conveyors reachable by their `connections`) whose frogports lie in more than one
district is a **shared store**, GA-2's G4 shape: the packages in transit, owned by no district, with a
port at every frogport on it. This design is G4's first user.

- **Identity and room.** Java names the network by its lowest conveyor position and lists its
  frogports with district, inventory and address filter, and its conveyor count. Room is
  `chainConveyorCapacity` (20, `CLogistics:15`) packages per conveyor, times the conveyors.
- **Routing.** A frogport takes the packages whose address its filter matches (`PackagePortBlockEntity`
  address filter: deterministic). The store keeps goods per receiving port, not loose packages.
- **Loaded districts.** Every package that leaves a frogport onto the chain, and every one that lands in
  a frogport, is an `IND_PACKAGE` (`IndRecords.Package` :353: district, inventory, direction, address,
  contents) for the frogport's district: two mixins at the count points of §2.2. These are exact ports,
  as postbox mail is. The same records fix a gap that exists today: since DS-10, packages between two
  districts on one chain are counted nowhere.
- **Both ends away.** The exporter's replay books its frogport's exports into the store; the importer's
  replay draws what is addressed to it, no earlier than the route's travel time (`RPM / 360` blocks per
  tick along the chain, GLASSBOX.md §1) and only while there is room. The store is a pool of the
  existing cluster, as the I13 power link is.
- **One end loaded, the other away** (G-B6, Q4). Create's chain hands a package to the next conveyor
  with no loaded check (`ChainConveyorBlockEntity.tick` :135, the `getBlockEntity` calls at :161, :176,
  :248 and :296), and vanilla's `Level.getBlockEntity` loads a chunk on the server thread
  (decompiled `Level.java:707` to `:713`). So a real package leaves the loaded side and stops in a
  conveyor whose chunk does not tick (UNVERIFIED: what it does there is the first GameTest of slice
  S4). Recommended: a package handed into a conveyor that does not tick moves into the store (one
  mixin at `addTravellingPackage` :380, journaled as the loaded district's export), and a package the
  store owes a loaded frogport is written into its inventory as a real package (`PackageItem.containing`
  and `addAddress`, as INDUSTRY.md §8.2 writes mail), room permitting. This is G2-11's cut, moved to the
  chain.
- **The compile.** `CreateCompile.frogportEdges` (`:1849`) keeps `CHAIN` edges between frogports of
  the same district and makes a port of the frogport's chain side when the network reaches another
  district. `CreateCompile.chainNetwork` (`:1877`) walks conveyors through `Level.getBlockEntity`, which
  loads chunks: it must read through `getChunkNow` and stop at unloaded chunks (a trap that exists
  today).

### 2.6 Kinetic networks across districts

Chain conveyors carry rotation, so cutting districts at chains cuts kinetic networks between districts.
Inside a district nothing changes (Q1 is about sections).

- **The driver fix (a bug today).** The compile builds `enginesByNet` from the district's own engines
  (`CreateCompile.java:413` to `:418`) and asks `singleBoiler` (`:615`) about them (`:452`, `:453`), so
  two districts on one network see different drivers, or none. The driver is decided over the
  network's sources instead: `KineticNetwork.sources`, and for a powered shaft its public
  `PoweredShaftBlockEntity.enginePos`. A source in a chunk that is not loaded makes the driver
  "outside", never "none".
- **The rotation link** (Q2, recommended). The district that holds the driver (the most capacity, if
  several hold sources) is the network's source district; every other district on the network gets an
  `IND_ROTATION_LINK(sink, source, network)` record, the shape of `IND_POWER_LINK` (`IndRecords.java:76`,
  I13: the sink runs only while the source does). In the sink's graph the network is a domain whose
  level follows the source district's replay: running at its compiled speed while the source's driver
  runs, stopped when it stops or overstresses. A sink district never replays a network it does not
  drive as a fixed level when the source is away.
- **UNVERIFIED, tested first:** what Create's loaded half of a network does when the chunk holding its
  source unloads (keeps its last speed, or stops). The partly loaded case follows Create's answer.

### 2.7 Data coupling across districts

Factory gauges, stock links and redstone links never join districts. Gauge networks across districts are
I16's (warn, then fulfil); `GaugeNetworks` already warns. A redstone link from one district's switch to
another district's clutch is out of scope here (§14).

---

## 3. Level 2: sections cut at stores

### 3.1 The cut

Sections are computed in Rust from the district's compile (`glass::graph::Graph`), at every accepted
compile and on restore. Java sends nothing new for them.

- **Union** along every goods edge (adjacency, belt, chute, pipe, air zone, arm reach, drop), every
  heat edge, every control and observe edge, and between the nodes of one inventory (a machine's input
  and output buffers), **except** where either end is a `STORE` node.
- **Cut** at every `STORE` node (it joins nothing) and at `CHAIN` and `LINE` edges.
- **Kinetic domains do not join** (Q1, recommended): a domain shared by sections is an input to each.
  A boiler is a `BOILER` node, not a store, so it joins its engines, its heaters and its water supply.
- The residual `BLACKBOX` node (G2-8) and every node it touches form one section, which learns as the
  residual does today.

### 3.2 Folding

The raw cut makes many small pieces: lone funnels, a belt between two vaults, a hopper chain. They are
folded so each section is a working line:

1. A **working** section holds a `MACHINE`, `HEATER`, `BOILER`, `SINK`, `SOURCE` or `BLACKBOX`.
2. A **transfer** section (movers, junctions, buffers only) folds into the nearest working section it
   reaches through one store, downstream first, then upstream, ties to the lower section id.
3. Transfer sections that reach no working section merge with each other across the stores they share:
   a storage system (vaults, funnels and belts sorting goods between them) is one section.
4. An **idle** piece (no machine and no store) keeps no state; its nodes belong to no section and carry
   no flow in the solve.
5. If more than `glass.sectionsMax` (TUNABLE, 64) sections remain, the two smallest neighbours (by
   nodes, sharing a store) merge until it holds.

Measured on the reference base (see the local notes), this leaves sections the size of working lines
and storage systems, and few shared stores.

### 3.3 Section state

| Field | Per | Notes |
|---|---|---|
| `section_of` | node | `u16`, `NONE` for idle nodes; recomputed, never saved alone |
| `fingerprint` | section | a canonical hash of the section's build: its nodes by kind and position and their build records, its edges by end positions, the build of every store it touches, and for every domain it touches the members' speeds as compiled and the lowest driver level that carries the domain's stress (an integer for a boiler, one bit for fixed sources) |
| open window rows | (section, good) | `Acc` and `Cum` as today (`check.rs:47`, `:66`), in one sorted `Vec` keyed by (section, good), not a map per section |
| `passed`, `strikes`, `edited`, `fallback` | section | today's `Compiled.passed`, `strikes`, `edited` and `DistrictGlass.fallback`, moved per section |
| window clock | district | sections share the district's window start, length and sweeps; each gets its own verdict at the end |

The fingerprint extends G2-81's "same factory": state slots (a heater's burn left, a controller's
output, a domain's live level, stress and capacity) are not part of it, and every build field is. The
domain term is chosen so that adding a machine to a shared network stales the sections on it only when
the network's speed or the boiler level it needs changes.

### 3.4 Learning

- Every section has its own window rows and verdicts (§4.1). A section **passes** when its window
  passes by today's rules (G-B4, the check of `check.rs`), applied to its rows.
- The district's own window stays, unchanged: it is the checksum over everything (stores in no section,
  ports with no inventory, the black box), and today's verdict for districts that have one section.
- Sections in a **judging group** (§4.3) pass or fail together.
- The black-box learner (C19) stays per district: it is the fallback and the checksum on the model.

### 3.5 Replay

- A section **replays** while it has a passed window, is not edited, and has not fallen back.
- The replay is built over the replaying sections only: the other sections' nodes are gated off
  (`Regime.gate_off`, `solve.rs:84`, as controllers gate nodes today), so their stores become pools
  that only replaying neighbours touch. A gated-off node keeps its stress in its domain: Create's stress
  does not depend on work.
- G2-22's parts form over the replaying nodes as today (`replay.rs:541`): parts are solve units, not
  judging units, so two replaying sections on one domain or one two-sided store share a part.
- **A part that falls back freezes its sections, and the rest of the district goes on** (SEC-1's own
  words; SC18). A frozen section holding a network's driver stops that domain for every section on it
  (conservative: nothing runs on an engine the sim does not trust).
- A district whose sections all freeze is today's frozen district (I5).

### 3.6 Staleness

- `IND_EDIT` (`IndRecords.java:71`) already carries the position (`Obs::Edit`, `obs.rs:467`). An edit
  stales the sections whose node box, grown by one block, holds the position. Java adds the kinetic
  network id of the edited block when it is kinetic, and the edit also stales every section on that
  network. A command edit (aux 4) stales the whole district.
- Stale sections stop replaying at once (I5, I7). The others keep their windows and their replay.
- Java recompiles after edits (G-B3). The new compile's sections are matched to the held ones by
  fingerprint: a section whose fingerprint is unchanged keeps its passed window, strikes and replay
  standing, renumbered to the new node indices; a changed or new section starts learning. The open
  window restarts for every section, because the node indices changed.

### 3.7 What stays per district

Loading and presence, ports and their journal, reconcile, threats, the black box, the district window,
the compile and its regime cache, the replay's cluster process set, and G2-81's compile numbering.

---

## 4. Accounting: the flows across a cut

### 4.1 Per section, per good

For a section `s`, a good `g`, its interior stores `I(s)` (every automation edge is to `s`), and the
shared stores `H(s)` (edges to `s` and to another section):

- **Measured:** `Y(s,g) = Σ_{i∈I(s)} ΔS(i,g) + Σ_{h∈H(s)} (in(h,s,g) - out(h,s,g)) - ports(s,g)`, where
  `in` and `out` are what `s`'s own movers put into and took from `h`, counted (§4.2).
- **Predicted:** `P(s,g) = Σ_{i∈I(s)} pred(i,g) + Σ_{h∈H(s)} (Σ_{e: s→h} L(e,g) - Σ_{e: h→s} L(e,g))`,
  from the solve's per-store predictions (`Solution.per_store`, `solve.rs:167`) and its exact per-edge
  goods flows (`Solution.edge_goods`, `:178`). Bounds and variance follow by the same sums of `Cum`.
- **Identity, tested:** summed over sections, stores in no section and the ports with no inventory,
  this is the district's `Y` and `P`. A section is never judged on goods it does not touch.

### 4.2 Counting at shared stores (Q3)

Recommended: **count by store face**.

- Rust tells Java which stores to count, per compile (`IND_COUNT_SET`, a command without an op id, as
  `IND_PROBE_SET` 0x8406 is, choice GB5). Java answers in the sweep with cumulative amounts per (store,
  queried block, face, good, direction) (`IND_COUNT_SAMPLE`, not counted in the sweep's records, as
  `IND_PROBE_SAMPLE` 0x004D is, choice GB3). Rust maps the face's neighbour to the node there.
- Java counts in one place: every block capability lookup ends in NeoForge's
  `BlockCapability.getCapability(Level, BlockPos, BlockState, BlockEntity, C)` (:200; the `Level`
  defaults in `ILevelExtension` call it, and `BlockCapabilityCache.getCapability` :138 calls the level
  and caches the answer at :149). A mixin there wraps the item or fluid handler of a counted store in a
  counting handler that knows the face asked about, and counts only real moves (`simulate` false,
  `FluidAction.EXECUTE`).
- Create's movers cache their handler (`CapManipulationBehaviourBase.findNewCapability` :135, the
  lookups at :139 and :147), so making a store counted calls `level.invalidateCapabilities(pos)` once,
  which makes every cache look again.
- UNVERIFIED, checked in slice S7 before anything depends on it: that every Create mover the compile
  models goes through a capability (not a block entity's own field), and that no caller tests the
  handler's class (the wrapper hides it).

### 4.3 Judging groups

A shared store whose moves cannot be attributed (a vanilla hopper, which uses the `Container` directly;
a lookup with no face; an arm whose interaction point does not name a face, UNVERIFIED) joins the
sections at it into a **judging group**, judged on their summed rows. The group is smaller than the
district, and with counting by face it is rare (measured on the reference base, see the local notes).

### 4.4 Ports

- A port with an inventory (`PortTransfer.inventory`, `obs.rs:163`) belongs to the section whose
  interior store that is. A port into a shared store is in no section's `Y`, by the equation above:
  no section's movers moved it.
- A port with no inventory (train cargo today, `TrainWatch.java:328`; a player's toss or pickup) is in
  the district's window. Train cargo is moved by the station's portable storage interfaces, whose
  positions `TrainWatch` knows at docking: the cargo of one interface facing one inventory is sent with
  that inventory (slice S6); otherwise the sections whose stores face the docked interfaces form a
  judging group for the cargo's goods.
- Frogport packages (§2.5) and postbox mail carry their inventory, so they always belong to a section.

---

## 5. Paging

- Sections live **inside** the district's glass page (G2-80, `persist::GLASS_TABLE` 0x4903), not in a
  table of their own: the per-section fields of §3.3, and the open window rows as one sorted array.
  `section_of` is not saved: it is recomputed from the compile's records on restore, as `Graph::build`
  rebuilds the indices.
- The page magic becomes `IGL2` (`glass/page.rs:32`). A restored page whose stored fingerprints do not
  match the sections recomputed from its compile is refused, and the restore fails loudly as G2-82
  says (the host starts the industry engine afresh and says so).
- Rows are a `Vec` sorted by (section, good), never a map per section: a one-entry map node costs far
  more than its row.
- The page grows by the extra rows (each good counted once per section that touches it, instead of once
  per district). Writing only changed pages is For Ben 15 (G2-83), unchanged here.

---

## 6. Migration

No migration code (no backward compatibility before release; G2-85). On load:

- `IndustryData` format 2 with districts: the districts are dropped, a warning goes to the log
  (`IndustryData.java:178` already does this for older formats), and districts re-derive from the seed
  attachments at the first detection.
- A glass page with magic `IGL1`: the restore fails by name, and the host starts the industry engine
  afresh and says so (G2-82, W23).
- The operator sees one line at start: "rtsim industry: the save format changed (sections, SEC-1); learned
  factory state was reset and districts are detected again." Nothing is lost silently, and only the
  sim's own knowledge is lost (UR-1).
- No live world runs rtsim (DR-0). GameTest worlds and rtsim-view's fixtures are regenerated after
  landing, as G2-85 did.

---

## 7. Cost on a weak PC

### 7.1 Detection (DS-11)

- J1 and J3 add no block entity visits: a class test per touch, and a canonical position the walk
  already computes.
- J2 adds the arm's points and the ejector's target for the arms and ejectors the walk already visits.
- J4 adds, for kinetic block entities within one block of a border only, the diagonal positions Create
  lists and an `isConnected` test, read through `getChunkNow` (never loading a chunk). These reads count
  as looked-at block entities in `rescanBlockEntitiesPerTick`.
- The commit's union becomes cheaper: over the recorded joins instead of nine lookups per member chunk.
- Worst case: a district made of border cogs. Each costs at most the 12 diagonal neighbours plus its face
  neighbours, so the budget still bounds a tick to `rescanBlockEntitiesPerTick` plus one chunk.
- Join pairs: 16 bytes each; a district has at most its touches' worth.

### 7.2 Sections (engine)

- Sections are computed once per accepted compile: a union-find and a breadth-first fold, linear in
  nodes and edges, next to a compile that is itself linear and rare (edits only).
- Memory per section: a header of about 64 bytes (ESTIMATE), `section_of` 2 bytes per node, and per
  (section, good) row about 350 bytes (ESTIMATE: the open row's `Acc` and `Cum` and the passed
  window's `GoodResult`).
- The rows that are new are the goods of stores shared by sections, counted once per section. Measured
  on the reference base, the extra memory is small next to today's glass state (see the local notes).
- Worst case per district: `glass.sectionsMax` sections times the goods each touches. The fold bounds
  sections; the goods a section touches are at most the goods of its nodes. A TUNABLE
  `glass.sectionRowsMax` (4,096 rows, about 1.4 MB, ESTIMATE) merges the smallest neighbours further.
- The replay costs what it costs today: parts are formed as today, over the replaying nodes.

### 7.3 Counters (Java)

- One hash test per capability lookup on the server thread (is this position counted), and one counter
  update per real move into or out of a counted store. Counted stores are only the shared ones, which
  the fold keeps rare. A cap `glass.countedStoresMax` (TUNABLE, 256 per dimension) turns the rest into
  judging groups.

---

## 8. Reach

### 8.1 Commands

- `/rtsim district sections [id]`: per section, its nodes, stores, verdict (learning, passed, failed,
  stale, frozen, replaying), the last window's worst good, and the shared stores with their counting
  state. `/rtsim district info` gains one line: sections, and how many replay.
- `/rtsim district graph <id>`: writes the district's compile, sections, join pairs and package
  networks to a file in the server's debug folder, the operator command the dump of §1 needed. It reads
  only what the engine and the index already hold.

### 8.2 rtsim-cli

`rtsim-cli glass <world> [district] [--json | --graph]`: the paged glass state of a saved world, read
only through the read-only open, as `rtsim-cli lines` reads train lines (`crates/rtsim-cli/src/lines.rs`).
`--graph` writes the compile with `section_of` for offline checks (the reference base's check of §9.4).

### 8.3 Viewer

- rtsim-view's district card gains its sections (verdict, nodes, shared stores) and the district's
  join pairs; the map gains a sections layer below 1 block per pixel (VIEWER.md §7 already plans a glass
  layer) and package networks as edges between frogports.
- mic-viewer is a public repository: the layer is code only; no world data goes into it.

---

## 9. Tests

Fixtures are built by code on the GameTest yard templates (`IndustryKit.YARD` for the Create module,
`industry_yard` for core), as the glass corpus is (`CorpusFactory`, `GlassCorpusKit`), never from world
copies. A factory that must cross a chunk border places itself relative to the absolute chunk grid
inside the yard, as `CreateSeedGameTests`' chain helper does.

### 9.1 GameTests, districts (Level 1)

| Test | Fixture | Checks |
|---|---|---|
| `twoFactoriesSideBySideStayTwo` | two press lines in neighbouring chunks, nothing between them | two districts (today one) |
| `aVaultAcrossABorderJoinsItsChunks` | a packager in one chunk under an item vault whose controller is in the next | one district (J3) |
| `diagonalCogsAcrossABorderJoin` | a large cog meshing diagonally with a small cog across a border | one district (J4) |
| `anArmReachingAcrossABorderJoins` | a mechanical arm taking from a depot in the next chunk | one district (J2) |
| `aChainBetweenTwoFactoriesLeavesThemTwo` (exists as `aChainBetweenTwoDistrictsLeavesThemTwo`) | two factories on one chain, frogports at both ends | two districts; a package network with two ports; every package an `IND_PACKAGE` at both ends; both windows balance |
| `anUnloadedMemberKeepsItsJoins` | a district with one member chunk unloaded before a detection | the same district after it (saved pairs) |
| `aStationBetweenTwoFactories` | a station touching two factories | two districts; the station's chunk with one of them |
| `aBoilerOnAChainDrivesTheNextDistrict` | a boiler and engine in one factory, a chain carrying rotation to a press line | both compiles name the same driver; the press line gets an `IND_ROTATION_LINK` |
| `aPackageIntoAChunkThatDoesNotTick` | a chain from a ticking chunk into one that does not tick | records what Create does (the UNVERIFIED fact of §2.5) before slice S4 builds on it |
| `aNetworkWhoseSourceChunkUnloads` | a boiler network across two districts, the source's chunk unloaded | records Create's behaviour (§2.6) |
| `AdoptGameTests`, `RescanBudgetGameTests`, `DistrictSeedGameTests`, `CreateSeedGameTests` | existing | still pass: adopt equals a full detection, the budget holds |

### 9.2 GameTests, sections (Level 2)

New `CorpusFactory` fixtures next to `PackageLoopFactory` and `BoilerOverstressFactory`:

| Fixture | What it is | Checks |
|---|---|---|
| `VaultBetweenMixersFactory` | a vault between two brass mixing lines (one fills it, one draws) | two sections, the vault shared and counted by face; each passes its own window; break and replace the second mixer: the first keeps its passed window and replays while away |
| `BeltIntoPressFactory` | a belt straight from a press into a saw | one section (SEC-1's "cannot be split") |
| `SharedBoilerTwoLinesFactory` | one boiler network driving a crushing line and a press line that share nothing else | two sections on one domain (Q1 a); an edit to the press line's belt stales only it; adding a press that needs a higher boiler level stales both |
| `StationPressLineFactory` | a station's portable storage interface unloading into a vault feeding a press line (`StationCargoGameTests`' kit) | the cargo belongs to the press line's section |
| `FrozenNeighbourFactory` | two lines on a vault, the first made to oscillate | the first section freezes, the second goes on drawing from the vault until it is empty |

Each runs the corpus cycle: loaded for a window, away for a replay, back for a reconcile that matches.

### 9.3 Engine tests (`glass/tests.rs`)

- The cut and the fold on hand-built graphs: a vault between two mixers, a belt into a machine, a
  shared domain, a transfer chain, a storage system, idle movers, `sectionsMax`.
- Fingerprints: the same factory with other state slots keeps every section; one changed section
  restarts alone; a domain change stales every section on it only when the needed driver level moves.
- Accounting: the identity of §4.1 on random graphs; counted and uncounted shared stores; judging
  groups; ports by inventory.
- Replay: a gated-off section's stores are touched only by its neighbours; a frozen driver stops its
  domain; G2-84's paging tests extended to sections (a save and restore before each step, equal pages
  and commands).
- The page: `IGL2` round trip; a page whose fingerprints disagree with its compile is refused.

### 9.4 The reference base, offline

The dump of §8.1 and `rtsim-cli glass --graph` let the reference base's numbers in the local notes be
repeated on the laptop copy after each slice. That check stays local; nothing from it is committed.

---

## 10. Build plan

### 10.1 Slices

| Slice | What | Tests | Depends on |
|---|---|---|---|
| **S1** | The driver fix (§2.6, over network sources); `/rtsim district graph`; `rtsim-cli glass` | `aBoilerOnAChainDrivesTheNextDistrict` (driver part), the CLI on a GameTest world | nothing |
| **S2** | Frogport packages as `IND_PACKAGE` at both count points (two mixins), loaded districts only | `aChainBetweenTwoFactoriesLeavesThemTwo` with records | nothing (fixes a gap since DS-10) |
| **S3** | Level 1 joins J1 to J6, no bare adjacency, saved join pairs, `IndustryData` format 3, the loud reset | §9.1 rows 1 to 4, 6, 7, and the existing detection tests | S2 |
| **S4** | The package network store (G4's first user): both ends away; then the frontier for one end loaded (Q4); `chainNetwork` through `getChunkNow` | `aPackageIntoAChunkThatDoesNotTick` first, then the store's replay tests | S2, S3 |
| **S5** | The rotation link (Q2) | `aNetworkWhoseSourceChunkUnloads` first, then the link's replay tests | S1, S3 |
| **S6** | Sections in the engine: cut, fold, state, fingerprints, per-section windows with judging groups where nothing is counted, staleness by position and network, replay gating, `IGL2`; train cargo per interface | §9.3, `BeltIntoPressFactory`, `SharedBoilerTwoLinesFactory`, `FrozenNeighbourFactory`, `StationPressLineFactory` | S1 |
| **S7** | Counting by store face (Q3): `IND_COUNT_SET`, `IND_COUNT_SAMPLE`, the capability mixin | the UNVERIFIED checks of §4.2 first, then `VaultBetweenMixersFactory` | S6 |
| **S8** | Reach: `sections` command, info line, viewer layer and card | command GameTests, rtsim-view fixtures | S6 |

S6 does not need S3 to S5: sections inside today's districts already pay off. New mixins go in
PATCHES.md (P7) with contract tests.

### 10.2 Reuse

| Need | Reused | Where |
|---|---|---|
| The walk, touches, canonical inventories, plugin links, merge links, pins, budgets | district detection as built | `Districts.java` (DS-10 to DS-12) |
| Arm points | `CreateGlassAccess.armInputs`, `armOutputs` | the glass compile |
| A link found by a plugin | `FanLinks` | `CreatePlugin.links` |
| Rotation across a border | Create's own `RotationPropagator.isConnected` and `addPropagationLocations` | Create |
| Package records | `IND_PACKAGE` and its engine path | `MailWatch` |
| A link between districts that couples running states | `IND_POWER_LINK` | `PowerLinks` (I13) |
| A store owned by no district | G4's shared store (GA-2) | not built: SEC-1 builds its first form |
| Flow across a loaded and unloaded edge | G2-11's cut pools | `replay.rs` |
| Gating nodes off | `Regime.gate_off` and `gates` | `solve.rs`, `check.rs` |
| Replay parts | `partition` (G2-22) | `replay.rs:541` |
| The same factory | `same_factory` (G2-81) | `glass/mod.rs:109` |
| A command and a sample record for Java-side measurement | `IND_PROBE_SET`, `IND_PROBE_SAMPLE` (GB3, GB5) | `GlassRecords` |
| Paging | the glass page (G2-80) | `glass/page.rs` |
| A read-only CLI view of a saved world | `rtsim-cli lines` | `lines.rs` |
| Fixtures | `CorpusFactory`, `GlassCorpusKit`, `PackageLoopFactory`, `BoilerOverstressFactory`, `StationCargoGameTests` | the Create GameTests |

### 10.3 Custom (new code)

| Piece | Why nothing existing does it |
|---|---|
| Join pairs in `District`, format 3 | the 8-neighbour rule needed no memory |
| J4 for border cogs | the walk only tests faces |
| Two frogport mixins | Create has no event for a package leaving or landing |
| The package network record and its pool | G4 is not built |
| The frontier mixin (Q4 a) | Create hands packages into chunks that do not tick |
| `IND_ROTATION_LINK` and its coupling | a power link couples districts through electricity, not a shaft network |
| Sections in Rust (cut, fold, fingerprint, rows) | the window, the verdict and the staleness are per district today |
| The counting capability wrapper and its records | the sweep reads levels, not flows |
| `sections` and `graph` commands, the CLI view, the viewer layer | new state to show |

---

## 11. A player's walkthrough

Ana builds a brass works next to her rail line.

1. **The build.** A train station with a portable storage interface unloads crushed copper and zinc ore
   into item vault A. Funnels pull the ore onto a belt through a washing fan into vault B. Two mixers
   over blaze burners take from vault B by funnels and fill vault C with brass. A packager under vault C
   sends brass to a frogport on a chain conveyor. Forty blocks away on the same chain, a second frogport
   and packager fill vault D, and a belt from vault D runs through three mechanical presses into vault
   E. A boiler with four steam engines beside the brass works drives both buildings: its shaft runs
   along the chain.
2. **Districts.** Two districts, the brass works and the press hall: the chain is a link, not a join
   (§2.1). The packages on the chain are one package network with a port at each frogport (§2.5). The
   press hall's shaft network is the boiler's, so it gets a rotation link from the brass works (§2.6).
   Before sections, if the two buildings' chunks touched, they were one district.
3. **Sections.** The brass works has two working sections: the washing line (vault A to vault B, with
   the station's cargo port) and the mixing line (vault B to vault C, with the packager and frogport,
   folded in downstream). Vault B is shared, and its funnels are counted by face. The press hall has
   one section (vault D to vault E). `/rtsim district sections` lists them, each "passed" after a
   loaded window.
4. **An edit.** Ana breaks a mixer to put a third one in. The mixing line goes stale; the washing line
   does not. She flies off before a new window passes. While she is away the washing line replays and
   vault B fills; when it is full the fan line stops, as Create's would. The mixing line is frozen, so
   no brass leaves on the chain, and the press hall uses up vault D and stops. Before sections, the
   whole brass works froze at the edit.
5. **Back home.** Ana returns; the reconcile writes vault B's level and vault D's emptiness into the
   world. The mixing line passes a window with three mixers, and the next trip away replays all three
   sections.
6. **The boiler.** If Ana starves the boiler of water, its engines lose capacity, the network
   overstresses, and the press hall's rotation link stops it with the brass works, loaded or away.

---

## 12. Questions for Ben

1. **Kinetic networks inside a district.** One shaft network often runs every machine in a building.
   - **(a) Recommended. A shared input.** Sections cut through a network; each section's speed comes
     from the network, and every section on it goes stale only when the network's speed or the boiler
     level it needs changes. On the reference base this keeps sections the size of working lines.
   - (b) One section per network, as SEC-1's "a shared shaft network cannot be split" reads literally.
     On the reference base, one network then holds most machines of each big district in one section,
     and sections gain little there.
   - (c) Split only fixed-speed networks (water wheels, motors); a boiler network joins its sections.
2. **Rotation across a district cut.** A chain conveyor carries rotation as well as packages.
   - **(a) Recommended. Districts stay apart, with a rotation link:** the district on the far side runs
     only while the driving district's engines run (the I13 power link's shape).
   - (b) A chain that carries rotation joins districts. On the reference base this joins most districts
     back into one, the giant district GA-2 turned down.
   - (c) The far district takes the network at its compiled speed with no link: simpler, but it keeps
     running when the boiler stops, making more than Create would.
3. **How flows into a store shared by two sections are counted.**
   - **(a) Recommended. By store face**, through one mixin on NeoForge's capability lookup
     (`BlockCapability.getCapability`): every mod's mover is counted the same way, and the face says
     which section moved it.
   - (b) Mixins on each storage class's handler (vault, tank, silo, chest): no face, so a store with two
     sections on it still needs (c).
   - (c) No counting: the sections at a shared store are judged together as one group, and only stores
     with a single section on each side split cleanly.
4. **Packages on a chain while exactly one end is loaded.** G-B6 says flow across a loaded and unloaded
   edge is modelled.
   - **(a) Recommended. Modelled:** a package handed into a chunk that does not tick moves into the
     package network's store, and packages owed to a loaded frogport are written into it as real
     packages, as mail is written into postboxes.
   - (b) Zero while one end is loaded: packages wait on the chain as in Create, and the far factory
     starves or backs up while you stand at the near one. Simpler; both-away flow still works.

---

## 13. Choices (Claude's, reversible)

| # | Choice | Why |
|---|---|---|
| SC1 | Joins J1 to J6; bare 8-adjacency is gone | SEC-1; on the reference base, adjacency held big districts together where nothing physical joined them |
| SC2 | J3: a multiblock inventory across a border joins its chunks | the only goods edges the split cut on the reference base |
| SC3 | J4 uses Create's own `isConnected`, for border kinetic blocks and overriding classes only | Create's rule, at a cost bounded by the border |
| SC4 | Arms and ejectors are plugin links, like fans | they move goods across borders without touching |
| SC5 | Join pairs saved per district, replaced only between chunks looked at; format 3 | unloaded members must keep their joins (DS-11) |
| SC6 | Stations keep DS-10's anchor rule and never join by touch | a station between two factories must not glue them |
| SC7 | Frogport packages are `IND_PACKAGE` records with the frogport's inventory | the record and engine path exist (`MailWatch`) |
| SC8 | A packager is not a cut; it joins its target | it is a machine on a store, and its packages are counted at the port they use |
| SC9 | One package network store per chain network that spans districts, room 20 per conveyor, goods per receiving port | GA-2's shape; Create's capacity and routing |
| SC10 | The driver is decided over the network's sources | districts on one network disagreed about its driver |
| SC11 | Sections cut at `STORE` nodes and `CHAIN` and `LINE` edges, join along everything else and one inventory's nodes | SEC-1's stores; a belt into a machine stays whole |
| SC12 | The fold: transfer into working, downstream first; storage systems merge; idle pieces keep nothing | sections are working lines; shared stores become rare |
| SC13 | `glass.sectionsMax` 64, `glass.sectionRowsMax` 4,096, `glass.countedStoresMax` 256 (TUNABLE) | weak-PC bounds |
| SC14 | The fingerprint's fields (§3.3), the domain term by speed and needed driver level | an edit on a shared network stales only what it can change |
| SC15 | Rows in one sorted `Vec` per district | a map node per one-entry section costs more than its row |
| SC16 | A recompile keeps every section whose fingerprint did not change, in every recompile, not only after a restart | G2-81's rule, per section |
| SC17 | Edits stale by node box grown by one, plus the edited block's kinetic network; commands stale all | positions are in `IND_EDIT` already |
| SC18 | A part that falls back freezes its sections; the rest go on | SEC-1: each section replays and goes stale on its own |
| SC19 | The district window stays as the checksum and judges ports with no inventory | nothing is lost against today |
| SC20 | `IGL2`, sections recomputed on restore, a mismatch refused loudly | G2-80, G2-82 |
| SC21 | Migration is a loud reset | no backward compatibility before release; UR-1 |
| SC22 | The black-box learner stays per district | it is the fallback and the checksum |
| SC23 | A gated-off node keeps its stress; a frozen driver stops its domain | Create's stress does not depend on work; conservative |
| SC24 | Train cargo per portable storage interface when one faces one inventory, else a judging group | the interface positions are known at docking |

---

## 14. Risks and unverified points

- **What Create does with a package handed into a chunk that does not tick** (§2.5) and **what a
  network's loaded half does when its source chunk unloads** (§2.6) are UNVERIFIED. Each is the first
  GameTest of its slice, and the slice follows what it shows.
- **The counting wrapper** (§4.2): movers that bypass capabilities, and callers that test a handler's
  class, are UNVERIFIED. If either is common, Q3 falls back to (c) for those stores.
- **Arms** call capabilities from a distance: attributing an arm's moves by its interaction point's face
  is UNVERIFIED (§4.3); until it is, a store with an arm and another section on it is a judging group.
- **Redstone links between districts** (a switch in one, a clutch in the other) are data coupling the
  compile does not model across districts. Splitting districts makes this more common; until it is
  modelled, the gated machine takes its compile-time state.
- **`CreateCompile.chainNetwork` loads chunks** through `Level.getBlockEntity` today; with districts cut
  at chains it would load other districts' chunks during a compile. S4 fixes it; until then, S3 must
  not ship without it.
- **Section counts on other bases** may differ from the reference base: `sectionsMax` and
  `sectionRowsMax` bound the cost either way, at the price of coarser sections.
- **Stores shared by many sections** were not seen on the reference base after the fold; the design
  handles them (counting by face, groups) but they are untested at scale.
