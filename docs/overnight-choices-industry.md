# Overnight choices: the industry engine

**2026-10-20, overnight agent 6** (the engine side of C15 to C19, `crates/rtsim-industry`). Ben said
to take the recommended option for reversible choices and log it. Every I-proposal below is still
open in INDUSTRY.md §20; this file records which recommended option the code relies on, and every
smaller choice made to build it. INDUSTRY.md and DECISIONS.md are unchanged; discrepancies with
them are at the end.

**Numbers.** Every default in `src/tunables.rs` is a PLACEHOLDER chosen so the synthetic sims run,
not a measurement. Where a default follows from a verified fact (Create's 100-tick gauge timer, its
18-slot offline buffer, the pack's dragon `stealItems` false), the comment cites it.

## Proposals relied on (recommended options)

| Proposal | Option the code takes |
|---|---|
| I2 | Districts are detected on the Java side; the engine takes membership as records (F3). |
| I3 | Boundary stock is the inventories Java declares (tagged storage, packager targets, postboxes, plugin buffers); machines are never sampled. |
| I4 | Game ticks only: the engine has no wall clock. |
| I5 | Windowed steady test; unlearned, stale and not-learnable districts freeze (F11, F32, F34). |
| I6 | One all-or-nothing box per district (F8 to F10, F13, F23). |
| I7 | A district suspends while any chunk ticks; deltas for every chunk are computed at the first, and the rest wait (F27). |
| I8 | Epochs per chunk, removals first, shortfalls trim outputs (F14, F16, F27, F31). |
| I11 | Deposit plugins: Ore Excavation, Diesel oil, Destroy crude (F15). |
| I12 | Per-season models; a boundary is a breakpoint (F28). |
| I13 | Channels as shared stock; PowerGrid lines couple running states (F8, F21). |
| I15 | The observed station exchange is emulated (F19). |
| I18 | The ledger as a Display Source line (F30). |
| I19 to I25 | Threats as the recommended options, with F17, F18, F20, F22 and F29. |
| I26 | I-A read-only first; the engine has a read-only mode (F25). |

## Choices

**Protocol** (docs/industry-boundary.md)
- **F1 (I3, I8; A9).** Industry records use their own kind ranges: 0x0040 to 0x005F latest-wins,
  0x0400 to 0x04FF journaled, 0x8400 to 0x84FF commands. The bridge must reserve them.
- **F2 (I3).** Three dictionaries: GOOD (0x10), NAME (0x11) and BLOCK_STATE (0x12).
  - A GOOD name is `<class>/<max stack>/<id>[/<component hash>]`, so the engine learns the class and
    stack size with no extra record.
  - Commands use the engine's dictionary, but its GOOD entries mirror Java's numbers, so the ids
    coincide.
- **F3 (I2).** District, station, train and channel ids are u64s allocated by Java and stable for
  the world. Inventories are u32 indices per district revision.
- **F4 (A9, I8).** Industry op ids are the kernel's engine ids with namespace 0x49 in bits 56 to 62,
  so no other engine allocator collides with them. One op id is allocated when a change is decided
  and is resent unchanged.
- **F5 (I5, INDUSTRY.md §3.2).** Stock samples are latest-wins and ports are journaled.
  - A sweep ends with IND_SWEEP_END and its record count.
  - A sweep missing a record is discarded whole, so a dropped record never corrupts a total.
  - Only sweeps with every chunk ticking are learning points.
- **F6 (I5).** A session start discards open learning windows: their unpaged samples are gone, while
  journaled ports would replay into a window with a hole.
- **F27 (I7, I8).** Chunk reconciles are sent when a chunk reports loaded (IND_CHUNK_LOADED with its
  applied epoch), or at once for chunks already reported loaded. Java holds a command for a chunk
  that is not loaded. The engine keeps every pending delta until a result says it is applied.
- **F31 (I8).** Applied reconcile amounts (from IND_RECONCILE_RESULT) count as ports, so learning
  never reads them as production. Java sends no separate IND_PORT for them.

**Learning**
- **F7 (I5, I6).** Rates are least-squares slopes in `f64`, converted once to the kernel's 32.32 fixed
  point when a model is adopted, and stored (the kernel's O8 and O14).
- **F11 (I5).** The steady test runs on the trailing window of the needed length.
  - Without this, a start-up transient (a smelter waiting for its first train) failed every later
    test: the smelter never learned when all districts loaded together.
  - A failure is counted once per maximum window length.
- **F13 (I6).** Targets are confirmed by two stops within `target_tolerance` with every input in
  stock and output space free.
  - The lowest stop level and the lowest restart level seen are used.
  - The restart level defaults to stop minus tolerance.
  - Learned only while a model exists. Unit-tested only: the iron line never hits its threshold.
- **F32 (I5, I7).** Unloading discards the open window: an unloaded gap inside a window would dilute
  the slope.
- **F34 (I5).** Every IND_EDIT, a configuration change included, makes a learned district stale.
  INDUSTRY.md §3.3 counts only player `BlockEvent`s, and those are all IND_EDIT carries.

**The replay**
- **F8 (I6, I13).** Districts that share a channel or a power line are one kernel `LazyModel`, a
  *cluster* of integer pools and processes. Most clusters hold one district.
  - **Box processes** mint and burn.
  - **Link processes** (transmitters, receivers, packagers) are transfers with one diffuser.
  - **Constraints:** pools stay within [0, capacity]; targets; shared item space; power requirements.
- **F9 (I6).** Breakpoints are exact for a constraint with one term, solved from the diffuser's
  carry. With several terms, a safe-side linear envelope may stop a process a few units early,
  never late.
  - Events re-anchor the envelope, so an event can move a multi-term stop. Events are inputs, and
    reads still change nothing.
- **F10 (I6).** A stopped process restarts only when it could then run `restart_min_run_ticks`.
  Without it, a consumer fed by a continuous trickle would cycle every few ticks.
- **F12 (I6, I8).** Item space is counted in 1/4096 slot units, with weights rounded up. Unknown
  goods count as 64-stack items.
  - The space is that of the inventories that received outputs while learning.
  - Every item good held there counts, so imports fill it and consumption frees it.
  - A margin of one slot per produced good and inventory covers partial stacks.
- **F23 (I6).** Each box learns its batch cycle: the longest gap between output increases in the
  steady window, at sampler resolution. A restart after a stop waits that long.
  - Without it, the continuous replay restarted mid-cycle and beat the real line by 0.5% in the
    starved regime.
  - A first start at unload is immediate.

**Reconciliation**
- **F14 (I8).**
  - **Removals** come from holders in proportion, largest remainder first, ties by index. Postboxes
    come last, because taking from inside a package is the awkward case.
  - **Additions** go to receivers in index order, then holders, then any inventory, up to slot-model
    room. What fits nowhere stays owed to the next load.
- **F16 (I8).** A short removal of s units of input c trims each output o by
  `max over c of floor(s × r_o / |r_c|)`.
  - The trim is capped by what the replay minted in the span, not by this round's additions.
  - It comes first from additions not yet applied, then from holders in a new epoch.
  - Outputs nobody holds any more are booked as untracked mints.
- **F20 (I23).** A doomed inventory (an attack broke its block while unloaded) still takes its
  reconcile deltas. Then everything in it drops when the block breaks.
  - The replay restarts without its room.
  - An output whose receivers are all doomed jams the line.
  - Before this, the replay's virtual contents of a destroyed vault were lost.

**Links, flows and deposits**
- **F15 (I11).** A deposit is a good of class `d`, read in every sweep, so learning sees it as a
  consumed input and exhaustion is an ordinary input-empty breakpoint.
  - Write-back happens at the reconcile.
  - Ore Excavation and Destroy crude write back inside the chunk's command; Diesel immediately.
  - Infinite veins are left out of the stock.
- **F19 (I15).** A station's exchange is learned from loaded calls, as arrival cargo minus
  departure cargo per good.
  - At a simulated station, unload goods go into the receivers' room less what the replay has
    already added; load goods go up to the carriage's room and the stock.
  - Java's IND_CARRIAGE_RESULT corrects the district's stock.
- **F21 (I13).** A sink's box requires its source's box. A live source is assumed running; an
  unloaded, unsimulated source means no power. The requirement follows the source's replay starting
  and stopping.
- **F26 (§8.2).** Packages leaving a loaded district's postboxes are learned as mail link rates, with
  the observed package contents as the batch.
  - The replay writes IND_WRITE_PACKAGE at each batch, so cost grows with packages.
  - Packages arriving at or leaving an unloaded district's offline buffer move what the engine
    knows and the replay's base together, so they are never applied twice.
- **F28 (I12).** Seasons come from IND_CONDITIONS. A season boundary restarts each member with that
  season's model, or freezes it.
- **F29 (I24, A15).** Legs are booked along the straight line between stations, by 512-block region
  and day window, and swept once a day.
  - An ambush loots carriage cargo with the same caps as district theft.
  - Track is never touched.

**Threats**
- **F17 (I20).** Natural pressure draws the next attack night from one geometric draw keyed by
  (district, class, day), with a zero-truncated Poisson count.
  - This has the same distribution as one draw per night slot, at one draw per attack.
  - PLACEHOLDERS: attacks land at tick 18,000 of the day; a natural mob has strength 1.
- **F18 (I21, I22, I23).** The outcome is the attackers' surviving share.
  - **Observed fights:** reduced to that share; the floor is the lowest comparable one.
  - **Walls:** let through `gaps × gap_passage_per_column` of a ground attack.
  - **Lanchester law:** the square law when turrets outweigh the other defenders, the linear law
    otherwise.
  - **Untested classes:** all defenders count at the untested weight.
  - **Theft:** survivors × carry, split in proportion to each good's cap.
  - **Destroyed blocks:** perimeter first for walkers, roofless first for flyers, ties by a hash of
    the attack id. Natural mobs break blocks only with `mobGriefing` on.
- **F22 (§13.1, C18).** "Watched" is a per-district flag the sampler sends (IND_WATCHED, latest-wins);
  unloading clears it.
  - Watched districts fight for real.
  - Unwatched ones resolve abstractly: at once, as a reconcile round, if loaded; at the next load if
    not.
  - Natural pressure applies to unwatched districts, loaded or not, since vanilla spawns nothing
    there either.
- **F35 (I19).** Stolen goods leave the world's books as `stolen`; the bandit camp market is the
  conflict module's.

**Phases and the ledger**
- **F25 (I-A, I26).** Read-only mode runs the replay and emulates station exchanges virtually, but
  emits no command except ledger lines.
  - On load it reports "would have made" and drops the replay's stock.
  - Its estimate leaves the trains' real cargo alone, so it can count goods twice across districts.
- **F30 (I18).** Ledger rates are per real hour (72,000 ticks at 20 per second). State codes are in
  docs/industry-boundary.md §4.3.

**Persistence**
- **F36 (A16, INDUSTRY.md §15).** The engine saves into kernel pages:
  - one page of globals (table 0x4900);
  - one page per district (0x4901, id = district id);
  - one page per replay cluster (0x4902) with its booked wake, restored as is (O18).

  The table numbers are PLACEHOLDERS until the module registry exists.
  - Everything is saved, open learning windows included, so a restore is exact; a real restart's
    `session_start` then discards the windows (F6).
  - District and cluster ids must fit 48 bits (the kernel's page key).
  - No dirty tracking yet: every page is written on every save.
- **F37 (A15).** `Engine::encounters` returns the trains whose booked legs lie in the regions a
  threat leg crosses during a window. Turning a meeting into an attack is the threat module's
  call.

**Scaffolding**
- **F24.** `fake.rs` (a Java stand-in through the wire codec), `scenario.rs` (the iron line),
  `Engine::adopt_model`, `Engine::inject` and `SyntheticThreats` are scaffolding, to delete once the
  Java side, its GameTests and real threat modules exist.

## Design discrepancies and findings for Ben

1. **Op-id top bit: resolved.** When this work began, the bridge branch put the top bit on the
   engine's ids, against the kernel's O10. Main now has the bridge on O10 (Java's ids set it), which
   this crate follows.
2. **The stale-tick race (INDUSTRY.md §19 risk 3) leaks across districts.** Commands land at least
   one tick after a load, so a machine runs on stock the replay already consumed.
   - The trim keeps a district's own books exact.
   - But in the iron line, the power plant's boiler burns already-replayed coal and powers the
     smelter for one cycle: +40 ingots of 9,296 (0.4%), which no single-district trim sees.
   - The race-free replay meets I-B: 99.71% of the control, never above it.
   - Options:
     - (a) accept a tolerance on both sides;
     - (b) the bridge holds a reloaded chunk's machines until its reconcile lands (feasibility
       UNVERIFIED);
     - (c) trims follow power links;
     - (d) the engine sends a district's reconcile ahead, when a player heads its way (the read-ahead
       of SCALE.md §8). The bridge would then apply it at `ChunkEvent.Load`, before the first tick.
       This removes the race at its cause, but needs the bridge's player positions.
3. **I20 says one draw per (district, night slot).** The geometric-gap draw (F17) has the same
   distribution and costs per attack instead of per night.
4. **"Cost does not grow with elapsed time" has one exception.** A consumer bottlenecked on a
   continuous trickle (a channel feeding it slower than it runs) stops and restarts once per
   `restart_min_run_ticks` (F10), so the cost grows as elapsed time / H. Throttling instead of
   all-or-nothing (I6 alternative (c)) would remove it.
5. **I6's all-or-nothing box over-produces where the real line stalls in ways learning never saw.**
   The learned cycle latency (F23) removes the systematic part. The steady rate is still the
   average over the window: a window without stalls learns no stalls.
6. **The sweep record (IND_SWEEP_END) is not in INDUSTRY.md.** §3.2 has round-robin sampling only;
   the record count is what makes latest-wins samples safe (F5).
7. **Package writing by unloaded districts (§8.2)** is built (F26). An engine-level test checks it:
   written packages, a train fetching some, and the vault giving exactly what was packed. Its
   first version took the packed goods twice; the fix is in. The fake world still has no postbox
   mail, so no scenario runs it with a real train.
8. **Memory.** About 8.6 KB resident per replaying district, measured: 86 MB at 10,000.
   - The district record, its learner and its cluster are not paged (A16 is not wired for industry
     state).
   - Paging and a slimmer district row are the obvious next steps.
9. **A merged cluster costs O(processes × constraints) per event.** Many districts on one channel
   pay quadratically per event. Clusters are formed only by channels and power lines, so this
   is rare.

## Adversarial review (Opus, 2026-10-20) and what changed

An independent review of the crate found fifteen problems. All are fixed, each with a test where it
can be shown:
- **Lost channel transfers (1):** a channel pool leaving its cluster now pushes its unpushed
  changes.
- **Stale pool indices (2):** `remove_member` matched pools by indices that shift; it now matches
  by key and keeps departed transfers in the books.
- **Trims on sent commands (3):** trims never edit a command already sent; they make a new epoch.
- **Jams that did not stop (4):** a jammed line is disabled outright.
- **Theft (5):** only raiders steal, and only items: not fluids, energy or deposits.
- **Ammunition (6):** fired ammunition is booked and clamped.
- **Destroyed blocks (7):** the reconcile result names the blocks broken; the wire format changed,
  so `IND_RECONCILE_RESULT` now carries Pos16 entries.
- **Train loot (8):** loot is reconciled against the carriage result; trains have cooldowns; legs
  are indexed by train.
- **Over-trimming (9):** trims accumulate per epoch.
- **Mail addresses (10):** mail pools carry their address, and the buffer's room counts what is
  already in it.
- **Power sinks (11):** sinks follow their source whether or not it replayed.
- **Ledger reads (12):** the ledger never advances time.
- **Reconcile races (13):** sample times are per record; an epoch applied without a result is taken
  as applied.
- **Double latency (14):** the restart latency was waited twice.
- **Attacker counts (15):** a large pressure mean drew 10,000 attackers.
- **Minor (16):** untested now means no comparable fight; read-only mode books nothing; no
  per-call clones.

Still open:
- **Channel double-spend (suspected by the review):** a live member's real channel withdrawal can
  meet virtual draws the replay has not pushed yet. The pool clamps the real withdrawal, and
  IND_CHANNEL_DELTA has no result to correct it.
- **Shared postbox buffers:** several mail pools on one postbox each count the whole 18-slot buffer
  as theirs, and IND_WRITE_PACKAGE has no result for an overflow.
- **Negative base:** `member_spec` clamps a negative base level to 0, which forgives a negative
  owed silently.
