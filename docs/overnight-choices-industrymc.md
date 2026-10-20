# Overnight choices: the industry module's Java side

**2026-10-20, overnight agent 10** (the Java side of C15 to C19: the district sampler in the core,
`neoforge/core/.../industry/`, and the Create plugin `mic_rtsim_create`, `neoforge/create/`). Ben said to
take the recommended option for reversible choices and log it. Every I-proposal is still open in
INDUSTRY.md §20; this file records which recommended option the Java side relies on, and every smaller
choice made to build it. INDUSTRY.md, DECISIONS.md, PLAN.md and OPEN-QUESTIONS.md are unchanged. The
engine's choices are F1 to F37 (`docs/overnight-choices-industry.md`); the Java side follows them.

**Numbers.** Every default in `IndustryConfig` is a PLACEHOLDER, except where a measurement is cited
(the slot budget, H10). Measurements are from `sweepCostOnLargeVaults` on this laptop (AMD Ryzen AI MAX
PRO 390), single-threaded, after warm-up; the potato reference (A11) is this laptop pinned to 2 cores,
so a single-threaded read costs about the same there.

## Proposals relied on (recommended options)

| Proposal | What the Java side does |
|---|---|
| I2 | Automatic detection from player-placed block entities, closed under touching industrial block entities across chunk borders and plugin links, 8-neighbour groups, named after the first station, edited by `/rtsim district` (H3 to H7). P5 is not used. |
| I3 | Boundary stock: `#mic_rtsim:district_stock` blocks, the inventory behind each Create packager, postboxes and frogports with package contents; machine inventories are never sampled (H8, H9). |
| I5 | Only player `BlockEvent`s are IND_EDIT; unattributed container sessions are IND_DISCARD (H14, H29). |
| I7, I8 | Epochs in the `mic_rtsim:reconcile` chunk attachment, held commands, removals first, the epoch in the same tick (H15, H16). |
| I15 | IND_CARRIAGE_DELTA through `TrainCargoManager`'s own handlers (H17, H18). |
| I18 | Superseded by Ben's I18 as decided (DECISIONS.md): no display source, no player output. `/rtsim district debug` (was `ledger`) is an operator diagnostic (H21, H22). |
| I23 | Destroy entries broken with `destroyBlock(pos, true)` when the state still matches; no fire (H15). |

## Choices

**Where things live**
- **H1.** The mod-agnostic part (districts, sampling, ports, reconciles, ledger) is the core package
  `industry` (D4: NeoForge only). Plugins add precision through `IndustryPlugin` and
  `IndustryApi.register` (layer 2 and 3 of INDUSTRY.md §12). The Create plugin is `mic_rtsim_create`
  (A19), compiled against the pack's own Create 6.0.10 jar; its jar-in-jar libraries (Ponder with catnip,
  Flywheel, Registrate) are extracted for compiling only.
- **H24.** The module observes the world only in the server's own session; GameTests that run private
  sessions next to it do not get a second industry module on the same data.
- **H26.** Its config is a file of its own, `mic_rtsim-industry-server.toml`, so the bridge's config
  keeps its keys.

**Goods (F2)**
- **H2.** A GOOD's component hash is FNV-1a 64 over a canonical text of the stack's `DataComponentPatch`
  (encoded with the registry-aware codec to NBT, compound keys sorted), 16 hex digits. The patch behind
  each hash is kept in `data/mic_rtsim_industry.dat`, so an engine command naming a good with
  components can be rebuilt after a restart. Known cost: one dictionary entry per distinct patch ever
  counted (named or damaged items), never trimmed.
- **H9.** Create packages are counted by their contents wherever goods are counted (stock in any
  inventory, carriages, a player's inventory, ports), not only in postboxes: a package in a vault or a
  carriage is never a good of its own (each package's contents make its components unique).

**Detection (I2)**
- **H3.** A *seed* is a block entity a player placed (`BlockEvent.EntityPlaceEvent`, multi-place
  included) that has an item, fluid or energy capability or that a plugin recognises; seeds live in
  their chunk (`mic_rtsim:industry_seeds`). Detection walks from the seed chunks through industrial
  block entities that touch across a chunk border (and plugin links: chain conveyors), then groups the
  member chunks 8-neighbour. Unloaded member chunks keep their membership. A loaded member chunk without
  seeds whose industrial block entity faces an unloaded member chunk is kept too (what it touches
  cannot be seen; dropping it would flap the district at every load).
- **H4.** Detection after edits recomputes only the districts an edit can change (those owning or next
  to an edited chunk, grown until nothing the walk reaches belongs to another), at most every
  `rescanIntervalTicks` (20); every `periodicRescanTicks` (6,000) and after a `/rtsim district` edit it
  recomputes the whole dimension (inventories that machines add). Ids: a pinned group keeps its pin; an automatic group the old district it overlaps
  most (each old id once, biggest group first); the rest get new ids, never reused (F3). Measured: a
  three-chunk district of 18 block entities (a chest and 17 shafts) detects in 64 and 60 us in two runs
  (best of 50, warm; `aDistrictIsDetectedAcrossShaftsAndNamed`), and a repeated detection changes neither its id nor
  its revision. A large base is unmeasured; `/rtsim district perf` shows the last detection's time.
- **H5.** `/rtsim district`: `list`, `info`, `debug` (was `ledger`), `rename <name> <new name>` (over the station's
  name), `merge <a> <b>` (a persistent link between their anchor chunks; a pinned district merges by
  moving its pins), `split [radius]` (the member chunks within the radius of yours are pinned to a new
  district), `exclude` and `include` (your chunk), `adopt [radius]` (industrial block entities around you
  become seeds: for worlds made before the mod, and structures placed by commands), `rescan`, `perf`.
  Every edit journals IND_EDIT (aux 4) for the districts it touches.
- **H6.** A chest a player places is a district by the rule as written: a storage-only district learns
  zero rates. Left as is; a "needs a machine" rule would miss vault-only stations.
- **H7.** Not learnable when industrial block entities touch across a district boundary: code 1 (rotation)
  for two kinetic ones, code 7 (a Java-side code, shown as "a belt, chute or pipe crosses the
  boundary") for belts, chutes and pipes. Other block entities that touch (two chests) never make a
  district not learnable.
- **H8.** `#mic_rtsim:district_stock`: the core adds `#c:chests` and `#c:barrels` (and the vanilla ids);
  the Create plugin adds `create:item_vault`, `create:fluid_tank` and, if present, Create Connected's
  item silo and fluid vessel. A multiblock is one inventory at its controller
  (`IMultiBlockEntityContainer.getController`); a vanilla double chest is one at its lower half. One
  function (`Districts.canonicalInventory`) does this for tagged storage, packager targets and clicks,
  and returns nothing rather than load a chunk. `sweepCostOnLargeVaults` checks that a 3 x 3 x 9 vault
  is one inventory, `aDoubleChestIsOneInventory` the chest.
- **H31.** The Create plugin's industrial block entities are a list, not every `SmartBlockEntity`:
  kinetics, pipes, chutes, funnels, tunnels, packagers, stock links and tickers, factory gauges,
  threshold switches, smart observers, stations, postboxes and frogports, depots, basins, crates and
  multiblock storage. Copycats, clipboards, signals, curved tracks and display boards are decoration and
  make no district.

**The sampler (I3, F5)**
- **H10.** K = 64 inventories a tick (PLACEHOLDER) and a slot budget of 20,000 item slots a tick, at least
  one inventory a tick whatever its size. The slot budget is measured, not guessed:
  - a full 1,620-slot vault (3 x 3 x 9, the largest): 37.7, 29.9 and 28.7 us per read in three runs
    (23.3, 18.5 and 17.7 ns per slot; the third with cargo builds running beside it);
  - one vault block (20 slots): 1.0, 0.5 and 0.7 us; a full chest (27 slots): 1.1, 1.0 and 2.8 us;
  - the first read of a new district, cold (JIT, new GOOD names): 6.7, 5.9 and 6.3 ms for the slowest
    inventory.
  So 20,000 slots is about 0.35 to 0.5 ms a tick, half the 1 ms bridge budget of INDUSTRY.md §3.2. The
  steady cost is (boundary slots of loaded districts) x about 20 ns / the 100-tick period: 20 full
  vaults cost about 6.5 us a tick on average.
- **H28.** No change-flag mixin on Create vaults (INDUSTRY.md §19 risk 2): the slot budget bounds the
  burst, and a vault in a working factory changes every sweep anyway, so a flag would save least where
  the cost is. It stays the next step if many idle full vaults show up in a profile.
- **H11.** `live` is set only when every chunk of the district is block-ticking at the sweep's first
  read and at every tick until its end: `FullChunkStatus.BLOCK_TICKING` *and* `shouldTickBlocksAt`
  (the simulation distance; player tickets give chunks within the view distance a ticking status, but
  their block entities tick only within the simulation distance). An
  inventory in an unloaded chunk is skipped and makes the sweep partial (not live). A revision change
  abandons the running sweep (inventory indices are per revision). Records are counted when attempted,
  so a record dropped for space makes the engine discard that sweep (F5).
- **H12.** Load states: a per-district count of loaded chunks kept by `ChunkEvent.Load` and `Unload`, a
  block-ticking check every 20 ticks (PLACEHOLDER) and at once after a chunk event; IND_LOAD_STATE when
  the state (0, 1, 2) changes, and once per district at each session start.
- **H13.** IND_WATCHED: a player in the dimension within the server's simulation distance (in chunks,
  Chebyshev) of one of the district's chunks. Sent on a change and every 100 ticks (PLACEHOLDER) while
  the district has loaded chunks, never for an unloaded one.

**Ports (§3.3)**
- **H14.** A container session belongs to the block the player right-clicked at most 2 ticks before the
  menu opened, and counts only on boundary stock: its port (kind 3) is the negated change of the player's
  own inventory and carried stack between Open and Close, with the inventory's index. A menu on any
  other district block (a crafting table, a furnace) or on nothing we can tell discards the window
  (IND_DISCARD 1): its effect cannot be told from production. A click on boundary stock that opens no
  menu (a bucket into a tank) is a port of the player's change over that tick. Tosses and pickups while
  a session is open are left to the session's diff. Only goods whose amount changed are named. Tosses count where the player stands, pickups where the item lies. A stock block a
  player breaks is a port of its contents (kind 6), and its drops are marked (`mic_rtsim:ported`, never
  saved) so picking them up is not counted again; breaking one block of a multiblock vault is no port
  (its contents stay in the rest).
- **H29.** IND_EDIT is every block a player places or breaks inside a district, not only industrial
  ones (a wall around the smelter changes it too). `configured` (3) is never sent: there is no generic
  hook for it.

**Reconciles (I8, F27)**
- **H15.** A command for a chunk that is not loaded is held in memory (the engine keeps every pending
  delta until a result says it is applied, F27, so a restart resends them) and applied in epoch order
  on the first tick the chunk is loaded. DUPLICATE when the chunk's epoch for the district is at or
  above the command's, or the journal holds the op id as applied. Deltas apply by position (the index is
  kept in the result); removals walk the slots from the last, additions insert stacked. Deposits
  (inventory 0xFFFFFFFF, class `d`) are reported unapplied: there is no deposit plugin yet. A destroy
  entry compares `BlockStateParser.serialize(state)` (the BLOCK_STATE form) with the expected state.
  The quota is 512 deltas and blocks a tick (PLACEHOLDER); a command bigger than the whole quota goes
  alone at the start of a tick, never split.
- **H16.** The chunk's reconcile attachment also carries the IND_RECONCILE_RESULT bytes until the engine
  acknowledges them, and they are journaled again at the chunk's next load in a later session (the
  pending-record pattern of `docs/boundary.md` §3.1). A crash that keeps the chunk and loses the journal
  still reports what was applied, not only the epoch (F13 would have taken it as fully applied).

**Trains (I15, §8.1)**
- **H18.** Three Create mixins: HEAD and TAIL of `Train.arriveAt(GlobalStation)` and HEAD of
  `Train.leaveStation()`. The cargo the train brought is taken at the head of `arriveAt`, because
  `arriveAt` runs Create's own mail transfer before it returns. IND_TRAIN_ARRIVED only at stations whose
  block is in a district. The cargo port (kind 1) goes out at departure from a loaded district: carriage
  contents at arrival minus at departure, packages by their contents. A departure with no arrival this
  session (a restart while the train stood there), or from a district that loaded while a simulated
  train stood there, discards the window instead (IND_DISCARD 5). IND_TRAIN_DEPARTED waits until `navigation.destination` is set (Create sets it after
  `leaveStation`), at most 40 ticks; ETA = `distanceToDestination / maxSpeed()` in ticks.
- **H17.** IND_CARRIAGE_DELTA is deduplicated by the journal's op index alone: train cargo
  (`RailwaySavedData`) and the journal are both SavedData written by the same save. Removals go first,
  as in a reconcile. A train that is not found still gets an IND_CARRIAGE_RESULT (everything applied 0),
  since the engine corrects stock from the result and not from COMMAND_RESULT; a carriage storage that
  is not initialized (Create throws) counts as empty.
- **H19.** Station and train ids on the wire: SplitMix64 of the UUID's halves, never 0.
- **H20.** A postbox's removals come out of its packages (rewriting `create:package_contents`; an
  emptied package is gone); additions go in as new packages addressed to the port's own filter, while
  it has a free slot.
- **H30.** Mail: IND_PACKAGE only for postboxes of *unloaded* districts, from two more mixins (HEAD and
  TAIL of `GlobalStation.runMailTransfer`) that diff the ports' `offlineBuffer`s: packages in or out,
  grouped by address, with their contents. For a loaded district the train-side cargo diff already
  carries what mail moved (packages by their contents), as the design says ("postbox inventories are
  stock; the train-side diff is the port"); an IND_PACKAGE as well would count it twice (the engine reads
  a loaded district's IND_PACKAGE as a port). The cost: the engine's mail-rate learning for loaded
  districts (F26) gets nothing from Java yet.

**The ledger (I18; H21 was superseded: Ben's I18 as decided removed the display source and every player-facing ledger, and `Ledgers` became `DistrictDiagnostics`, operator only)**
- **H21.** "District ledger" is a multi-line display source registered in Create's `display_source`
  registry and bound with `DisplaySource.BY_BLOCK.add` to `create:stock_link` and `create:track_station`.
  The district is the link's "District" option (a name or id; a text box in the link's screen), else the
  source block's own. It refreshes every 100 ticks. Lines: name and state (with the freeze reason), the
  top goods per real hour (F30) with "would have made" for simulated states, the next breakpoint. Good
  names are translatable components, so each client reads its own language.
- **H22.** (Superseded.) IND_LEDGER_REQUEST and IND_LEDGER are gone from the wire; `/rtsim district info`
  and `debug` print only what Java holds, with no engine round trip (I18 as decided, IR14).
- **H27.** Read-only mode (I-A) is the engine's (F25): Java applies what the engine sends and has no
  switch of its own.

**The stale-tick race (F choices, finding 2)**
- **H23.** Only the engine-agnostic hook: IND_APPROACH (0x0046, latest-wins, a Java-side proposal not
  yet in wire.rs): `8 i64 gameTime, 16 u64 district, 24 u32 distance (blocks to the district's box),
  28 u32 etaTicks (0xFFFFFFFF unknown)`, aux 1 when the player is closing in. Every 20 ticks per
  unloaded district within 256 blocks (PLACEHOLDER) of a player; `approachSignal` switches it off. The
  engine skips it as an unknown kind until it reads it. Nothing else: no early apply at
  `ChunkEvent.Load`, no holding of machines. That policy is Ben's (options a to d).

**Tests**
- **H32.** `IndustryWireContractTest` (core unit tests) rebuilds 19 observation kinds from IndRecords
  and compares them byte for byte with records wire.rs encoded, reads 4 command fixtures back at
  IndRecords offsets, and checks 11 code tables. The fixtures come from
  `neoforge/tools/industry-wire-fixtures` (its own Cargo workspace, path dependencies on the crates);
  regenerate them when wire.rs changes.
- **H25.** Every industry GameTest is in the create project and runs against the stub engine
  (`-Dmic_rtsim.engine=stub` whatever `-PrtsimEngine` says): the industry commands they script are raw
  orders only the stub's script API takes. Each test has its own batch (so none runs beside another),
  and each starts by forgetting every district of the level (a test helper). (Since W17,
  `docs/overnight-choices-industryjni.md`: the run takes the engine switch, native by default, and the
  scripted commands reach the native engine as INDUSTRY_COMMAND orders.)

## Adversarial review (Opus, 2026-10-20) and what changed

An independent review of the Java side against wire.rs, docs/industry-boundary.md and the engine found
no wire-format mismatch (a separate contract test, `IndustryWireContractTest`, checks every IndRecords
offset against fixtures encoded by wire.rs itself: 36 tests). It found these, all fixed:
- **Ticking (2):** chunks within the view distance but outside the simulation distance counted as
  block-ticking (H11).
- **Counting twice (3, 8, 15):** a double chest behind a packager; the drops of a block a reconcile
  broke (the engine already books the spill); a double-chest half broken by a player (H8, H14).
- **Ports (1, 4):** mail moved inside `arriveAt` was in no diff (H18); sessions on non-stock menus taught
  the engine that a district crafts; tosses and pickups inside a session counted twice (H14).
- **Crash safety (6, 7):** a throwing `onRemove` or an uninitialized carriage storage could stop the
  bridge mid-command; a missing train got no result (H15, H17).
- **Restarts (9):** a departure whose arrival was before the session (H18).
- **Cost (5, 10, 11, 12, 13):** chunk-load events queued while nothing drained them; a canonical lookup
  that could load a chunk; whole-dimension detection after every edit (H4); every Create smart block
  entity a seed (H31); every stack in a player's pockets named as a good (H14).
- **Smaller (14, 17):** a dropped IND_WATCHED change waited for the resend; a GOOD name whose registry
  path holds '/' was misread.
One finding (the stock tag files swapped) was wrong: the core's tag lists the vanilla chests and
barrels, the Create plugin's the vaults and tanks.

## Findings for Ben

1. **The GameTest server ran at about 600 ticks a second**, so tests with tick timeouts finish in
   seconds; nothing in the module depends on wall time, but anything timed in ticks in a test is not a
   measure of real-time cost.
2. **A chunk that drops below FULL and comes back before it unloads** fires no new `ChunkEvent.Load`,
   so no IND_CHUNK_LOADED: its attachment never left memory. The engine sends reconciles at once for a
   chunk it thinks loaded (F27) and Java holds them until `getChunkNow` sees it, so nothing is lost; the
   held test waits for the real unload (`ChunkEvent.Unload`, the save) before it reloads.
3. **Synthetic trains work for the station hooks.** The GameTests build a `Train` and `Carriage` with a
   mounted vault (`MountedStorageManager.addBlock`) and call `arriveAt` and `leaveStation`, so the mixins
   are exercised without assembling a train. A real assembled train on a schedule is not tested.
4. **A core GameTest is timing-sensitive under load**: `traderAndLlamasCaptureAsOneGroupAndNoLlamaDespawns`
   failed once ("the far chunk is entity-ticking") while cargo builds ran beside the suite, and passed
   on the rerun (36 of 36). It is the phase 0 caravan test, not industry code; the GameTest server's
   speed makes its tick timeout short in wall time.
5. **Real stations do work in a GameTest**: a straight `create:track` line plus a `create:track_station`
   whose block entity is given `TargetTrack` in NBT joins the graph in a few ticks, and the district takes
   its name.
