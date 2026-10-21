# Glass box G2 choices (glass-box replay)

Every reversible choice made while building phase G2 (GLASSBOX.md §11.3), with ids G2-1,
G2-2, ... Anything that is Ben's call is in "For Ben", recommended option first. G1's choices
are in docs/choices-glassbox-g1.md (GB1 to GB111). Code comments cite these ids.

## The replay

| Id | Choice |
|---|---|
| G2-1 | **Report header 41.** The glass report (docs/industry-boundary.md §9) grows from 33 to 41 header slots: 33 replay model (0 none, 1 black box, 2 glass), 34 since (tick, -1 never), 35 regimes, 36 breakpoints, 37 oscillation fallbacks, 38 cut edges, 39 eligible (0/1), 40 learned sources. Good rows follow at 41. Java's `GlassReport.HEADER` matches. |
| G2-2 | **Eligibility (G-B4).** A compile replays on the glass model once one window of it has passed and no IND_EDIT came after (`Compiled::eligible`). Before that, and after an edit until the recompile passes, the district freezes (no replay; the books stay as the last reconcile left them). No compile, or one the compiler refused, keeps the black box. A derate or a demotion after a pass keeps eligibility; the replay solves with the compile's regime, so it uses the derated factors and the demoted kinds. |
| G2-3 | **Per-store pools.** A glass replay books into `PoolKey::Store(district, inventory, good)`, one pool per store and good (§4.1), read from each covered inventory's stock at the start. A store's change stays with that store and lands there at the reconcile; a change that cannot be placed in its own store goes to the district's owed (`reconcile::plan` spreads it as before). |
| G2-4 | **One Box process per part per regime.** Each part (G2-22) of a district's replay is one process: the regime's net rate per pool, as Box terms, all-or-nothing. A breakpoint stops it the tick before a flag changes; the replay re-reads the flags from the pool levels, re-solves (the regime cache answers most of these) and replaces the process in place. No slot period (C28). |
| G2-5 | **Held and room.** A store's good is flagged empty when its pool cannot run one tick of its drain; it clears when the pool has risen by the good's holding threshold above the flagged level, or (G2-19) holds stock while nothing arrives. Room is the store's free item slots, shared among the goods arriving in it (G2-26), and for fluids its capacity. A store some arriving good has no room in for a tick is flagged full and its in-edges close. |
| G2-6 | **Latency holds (§4.4).** A simulated node whose potential rises (its flow in the regime with nothing held) is held off for its downstream latency (the time its output takes to reach a store; a loop counts whole), then runs. What it makes arrives no earlier than in Create, and every flow stays a solve's, so goods are conserved. A fall is at once. |
| G2-7 | **Breakpoints and the oscillation limit.** A breakpoint is a pool at a bound (empty, full, a controller threshold, a dry cut good), a heater's phase timer, a hold coming due, or an outside event. More than `glass.breakpointsPerEventMax` (64) breakpoints since the last outside event or the last closed-form cycle is Oscillating. A hold coming due does not count (a node holds once per rise of its potential, and a rise needs another breakpoint), which is why the bench saw fallbacks at 99 to 110 breakpoints. A fallback (Oscillating, Unsettled, Stuck, Unexpected) moves the district to the black box until it is live again or a new compile arrives; with nothing learned it freezes. |
| G2-8 | **Residual node (G-B13, §5.1).** From the passing window, per unbounded good (a black box's): measured, shaded toward 0 by z sigma, minus predicted. Per bounded good, only a production above U (G2-57). Exact goods take none. It is split over the covered stores whose own window slope misses the same way (for a deficit with none, over the stores the model fills), floored to 32.32 per tick so it never rounds above, and booked as a source or sink at those stores while the stores it drains hold and those it fills have room (learned coupling, I6). A positive residual on a positive measured flow counts as a learned source (I9). |
| G2-9 | **Glass state is not paged.** The compile, its passed window and the replay's regime state live in memory only. After a restore, a district that was replaying is marked dirty and goes on by the black box (or freezes) from the next step, its stores' changes carried, until it is loaded and recompiled. No page format change in G2. |
| G2-10 | **Regime cache per part.** Solves are kept by regime key, `glass.regimeCache` (4) per part, least recently used out, so a controller cycling between two regimes solves each once. |
| G2-11 | **Partial-load cut (I7, G-B6).** Goods edges between a live and a simulated node are the cut. When goods cross from the live side, `PoolKey::Stock(d, g)` holds the live side's supply (what the live stores hold plus what the district owes) and the simulated side draws it there; its change from the start is owed. A cut good that runs dry stops the live node that would send it, until the supply is back at a holding threshold. A cut keeps the district one part. |
| G2-12 | **Time shift at a breakpoint.** The regime that just ended is shifted back in time until the pool that fired sits on its canonical carry (`shift_back`), so a breakpoint costs its regime under a tick and the next regime starts from a reproducible carry. A cycle's pieces record their fires and apply the same shift, so a closed-form period equals the stepped one. A pool nothing drains (an output store at 0) is not flagged empty, because no watch would restart its carry. |
| G2-13 | **Closed-form cycles (G-B9).** A regime sequence that repeats exactly (same flags, same watched levels, zero drift on every pool that decides a breakpoint) becomes one `Body::Cycle` over as many whole periods as provably change nothing else (128 marks kept, 4 candidate periods per mark, at most 2^40 ticks, 2 units of margin per pool). A controller loop costs one process per stretch, not two breakpoints per period. This is the periodic edge of §4.2 for clocks and controllers; trains, ships and packages are G3. |
| G2-14 | **Imports reach the machines.** An import (a train's unload, a give-back, an injection, a package) into a replaying district with no cut moves into the covered store that feeds the graph with that good (one with room first, then the lowest inventory index), so the machines see it; the store's change is written back once at the reconcile. A good no covered store feeds, and every import across a cut, waits in `Stock` for the reconcile. |
| G2-15 | **An edit freezes.** An IND_EDIT on a replaying district stops the glass replay (what it did is carried) and the district freezes until the recompile passes a window. |
| G2-16 | **What keeps the black box in G2.** A district with deposits, cross-district links, a PowerGrid source, a sink, or damage replays by the black box, as does one whose glass replay fell back. |
| G2-17 | **Chunk ticking.** A Java side that sends no IND_CHUNK_TICKING is read as "every loaded chunk ticks". |
| G2-18 | **Moving frames.** A compile on a moving frame (a Sable ship, frame not 0) has no world positions: every node is simulated. |
| G2-19 | **Empty flags clear on standing stock.** A store flagged empty that holds stock while nothing arrives is a source again: a chest a funnel kept nearly empty is not a source while goods pass through it, but what is left drains once they stop. Not for a hub (G2-22): its other parts drain it, which this part cannot see. |
| G2-20 | **The switch flips a tick early.** Each test looks one tick ahead (a bound the process could not run a tick under is a crossing now), so the replay's threshold switch flips a tick before Create's. It flips a little more often than Create (by its band over the band less two) and makes a little less. |
| G2-21 | **A machine's cycle is continuous.** The replay books a consumer's output at its rate, not at the end of each cycle, so at any read it may be one item ahead of Create's (the item in the cycle). The tests allow that one item. |
| G2-22 | **Parts.** The graph splits by union-find into pieces that share nothing but stores they only draw from (hubs). Each part is its own process with its own regimes, cache and cycles, so two loops of unequal periods each find their cycle. A hub is in every part it feeds; a cycle's steepest drain is its safe envelope there, a hub's empty flags are sticky, and its last units go to whichever part's watch fires first. A cut or a residual keeps the district one part. |
| G2-23 | **A heater's burn is not latency.** A heater's rate is its burn per fuel, not a transit: a fuel lights it at once, so the latency map gives heaters 0. |
| G2-24 | **The first solve round starts with heaters lit.** The duty loop starts from the gated heaters' assumed duty, so a heater fed by the line it heats is not read as out for good on the first round. L only comes down from there. |
| G2-26 | **One slot per arriving good first.** A store's free slots go one to each arriving good with no part-filled stack (heaviest first while slots last), then the rest split by rate. By rate alone a rare byproduct's share floors to no slot and the store reads full at once. |
| G2-27 | **An unconverged solve falls back.** A solve that did not settle books flows that do not balance at its nodes (a loop whose only exit is full grows round by round): goods would leave the stores and arrive nowhere. The replay falls back as Unsettled instead. |
| G2-28 | **Rounding one tick ahead.** A rate's reach is ceil of its 32.32 net per tick in units (the most one tick moves); a controller threshold uses reach + 1. Every flag test compares a level against the reach, so no process runs a tick past a bound. |
| G2-57 | **A bounded good inside its window takes no residual.** GLASSBOX.md §5.3: inside [L, U] the bounded rules are calibrated to the measurement; only a measured production above U is something unmodelled producing, and its excess over U is the learned source. A residual on one bounded good alone booked it from nothing when its recipe partners' windows were narrower (the exit run's package_loop made 1,012 clay balls from no clay). Calibrating bounded flows inside the solve is not built: inside [L, U] the replay runs L. |

## Belt side insertion and the over-fed loop (GB72, decided by Ben as recommended)

| Id | Choice |
|---|---|
| G2-29 | Through traffic is marked by a new edge flag, THROUGH = 16 (records.rs, GlassRecords.java, docs/industry-boundary.md), not inferred from edge kind or order. CreateCompile sets it on a belt's chain links and the taker split link; a belt's end edge is not THROUGH. |
| G2-30 | Side insertions at a belt segment share one proportional priority group (`inserted_i = s_i min(1, c / sum s)`); through traffic takes the remainder. From `BeltInventory.canInsertAtFromSide`: an insertion waits only for its own side's last insertion. |
| G2-31 | A merge is bounded (SHARED_CAPACITY) only when the segment binds with more than one input flowing; below capacity it is exact. |
| G2-32 | The merge rule applies only to belt movers with both a THROUGH in-edge and another in-edge. |
| G2-33 | The stall is decided once after L converges, then L is re-solved (up to another `solveRoundsMax` rounds). |
| G2-34 | The feed f is measured by a probe solve with the loop's nodes as sinks. |
| G2-35 | A loop stalls when its host is SEQUENCED, on a cycle and saturated, its goods loop holds no store, and f > mu for some base good; f = mu runs. |
| G2-36 | mu is recipe capacity times heat factor times base input amount at the converged domain level. |
| G2-37 | A stalled loop gets L = 0 on every goods edge whose head is in the loop (parts chutes and feed included); U is untouched and the class stays bounded (cycle). |
| G2-38 | Loops with P at most 2 passes are documented but not exempted: the stall applies for any f > mu (see For Ben). |
| G2-39 | Both rules sit behind pub consts (`BELT_INSERTIONS_FIRST`, `OVERFED_LOOPS_STALL`, both true), with `solve_with` to flip them per call. |
| G2-40 | The LP passes each good through a pass-through machine with no recipes (the loop's deployers), as L and the linear bound already did. |
| G2-41 | Corpus 6 is split into `PrecisionMechanismFactory.layout(h, feedRpm)` and `checkBelts(h)`, shared with the new `precision_overfed` factory (feeder at 64 RPM, warm-up 1000, window 2400). |
| G2-42 | `precision_overfed` expects EXACT 0 for sheet, cogwheel, large cogwheel, nugget, mechanism and andesite alloy. Its exit window is [0, U], so only its smoke test checks the jam itself. |
| G2-43 | GLASSBOX.md §2.3's "Belt insertion" row (1 block against 1.0625) is left as is, to keep the docs diff to §3.3 and §3.4. |

## Bench sweeps (§9.3, `rtsim-cli bench industry --only glass_`)

| Id | Choice |
|---|---|
| G2-44 | The bench marks a compile passed through its public fields (`passed` set, `edited` false) instead of driving a checksum window. |
| G2-45 | `glass_items` scales a switched line: speed is the scale clamped to 1..10 and the batch scale/speed, so the period stays 3,200 ticks at every scale. |
| G2-46 | `glass_elapsed` and `glass_quiet` use a factory cell of 10 nodes and 11 edges whose stores never fill or empty; `glass_periodic` uses the switched line (press 1 per 10 ticks, consumer 1 per 20, switch at 100/20). |
| G2-47 | The catch-up sweeps measure one read from the unload, so the start transient (7 breakpoints) is inside each point. |
| G2-48 | `glass_regimes` makes breakpoints with exports of one stack from a full 2-slot store, evenly spaced; an unmeasured warm-up day runs first there and in `glass_quiet`. |
| G2-49 | "Nothing happening" (`glass_quiet`) is steady factory cells plus 10 busy export lines (one export a day, staggered 100 ticks), as in the existing quiet sweep. |
| G2-50 | Heap is the bench's counting allocator, with `Engine::heap_bytes` recorded beside it; each point is the median by CPU of 3 runs (quick) or 5 (full). |
| G2-51 | A sweep with any point off the glass box fails as "NOT ON THE GLASS BOX" and prints the fallback reasons. A `--only <prefix>` flag selects sweeps. |
| G2-52 | Ranges: items 1x to 1000x over 10 days (quick) or 30 (full); elapsed 1 to 1,000 days (quick) or 10,000 (full); periodic 10 to 100,000 periods; regimes 1 to 1,000 a day over 3 or 10 days; quiet 10 to 1,000 districts (quick) or 10,000 (full). Districts 64 blocks apart, inventories 2 apart, big stores 2^24 slots; items, quiet and oscillation read the stock daily. |
| G2-53 | `glass_oscillation` reports 8 cases (narrow band, no band, two switches on one store, opposed switches, two loops same, two loops 2:1, two loops odd, five loops) without a slope. |
| G2-54 | The bench's findings live as tests in `crates/rtsim-industry/tests/glass_bench_findings.rs`; the export and quiet tests check breakpoints proportional to exports, not exact counts. Committed results carry a `-full-glass` suffix. |

## The exit test harness (GameTests, `GlassReplayGameTests`)

| Id | Choice |
|---|---|
| G2-25 | The harness credits an input's work in progress (goods inside machines at the end) as yield within tolerance only, never toward "at or below the control"; the stub engine applies the offline reconcile as the native one does. |
| G2-55 | Each corpus factory runs as two copies, one kept loaded (the control) and one far away and unloaded (simulated). The far place is laid out as vanilla's `StructureUtils` lays out the yard (relative y -2..-1 stone, y 0 air, y 1 polished andesite, air above, vanilla's clear margin), so blocks that need support (a diode on the floor, a boiler's channel dug into it) stand as in the control. Each copy draws its own package frequency at build. While both are loaded, the harness samples both copies' declared stores every 100 ticks; if any good's change over the last 2400 ticks differs by more than one stack or batch in flight, plus 3 control sigma over the window, plus a tenth of the control's change, every row of that factory reads INVALID COPY and fails. |
| G2-56 | `package_loop` is marked KNOWN GAP in the table and still counts as a failure (package routing by address is not modelled, see For Ben). |

## For Ben

Recommended option first in each.

1. **Package routing by address.** Today `CreateCompile.frogportEdges` flags chain edges BOUNDED with L = 0 ("addresses are not modelled"), so a package loop replays as making nothing (`package_loop`: control +1236 clay balls, simulated 0). GLASSBOX.md says address matching should be exact. (a) Model addresses in the compile (each packager's address and each frogport's filter make an exact edge) in G3 with the other periodic lines; (b) leave L = 0 (safe, never above the control, but the factory is dead while away).
2. **Glass state across a restart (G2-9).** (a) Page the compile and its passed window with the district, so a restart keeps the glass replay (a page format change); (b) keep it in memory, so after every restart replaying districts drop to the black box until a player loads them.
3. **Over-fed loops with P at most 2 (G2-38).** (a) Keep predicting a stall until a factory measures one (estimate: the queued feed falls to about half the first deployer's pass rate); (b) exempt them now.
4. **Time lost at each breakpoint (G2-12, G2-20).** A breakpoint costs its regime under a tick and the switch flips a tick early, so a switched line makes about one tick of output per flip less than Create. (a) Keep it (always at or below the control); (b) carry the fraction exactly (more state per breakpoint).
5. **The start hold (G2-6).** A district that unloads mid-run holds each rising node for its latency again, so a belt-fed line loses one latency per unload (the arm depots replica made about 87% of ideal in the unit test). (a) Keep it; (b) treat goods already on the belt at the unload as in transit, which needs the belt's contents at the unload.
6. **Parts sharing a hub (G2-22).** (a) Keep parts with a hub's steepest drain as its envelope, which lets two loops of unequal period stay on the glass box; (b) one process per district, which falls back as Oscillating whenever two loops have different periods.
7. **A fallback with nothing learned freezes (G2-7).** (a) Freeze until the district is live again or recompiles (never above the control); (b) replay a black box from the compile's predicted rates.
8. **A whole node is held on movers (G2-6).** (a) Keep holding the whole node (conservative); (b) hold per out-edge, closer to Create on a belt that splits.
9. **A dead factory is eligible.** A compile whose model predicts 0 for everything still passes and replays exactly (nothing). (a) Keep it eligible; (b) refuse zero models.
10. **Native recompile churn on package loops.** The native control recompiled the package loop repeatedly while loaded (184 compiles in one run; not a frequency collision between the copies), a D1 cost on the server thread. (a) Look at it with the package routing work in item 1; (b) rate-limit recompiles per district now.
11. **Exit test on native alone.** The stub engine reaches the same table except where its offline reconcile timing differs. (a) Count native as the exit verdict and the stub as a smoke check; (b) require both.

## UNVERIFIED

- Derating and demotion in the replay: the replay solves with the compile's regime (derate factors, demoted kinds), but no replay test covers a derated or demoted compile.
- The residual and learned coupling are covered by one unit test (`the_residual_replays_a_black_box_while_its_stores_hold`), not by a GameTest factory with a real residual.
- A partial-load cut is covered by one unit test (`a_partial_load_carries_the_cut_edge_from_the_live_store`) and by the partial brass_tunnels and boiler_overstress exit rows; cuts in other shapes are not tested.
- Trains, ships and packages as periodic edges are G3; G2's periodic edges are its closed-form controller and clock cycles.
- Whether Create's real switched half period is 1600 ticks or the 1578.5 the replay ran on feab04d (bench finding 2, before parts).
- From GB72: whether loops with P at most 2 jam or settle; the f = mu knife edge; parts-starved loops (not saturated, so not stalled); loops containing a store (excluded); which side wins the gaps when a merge is saturated; the deployer's time of about 6 ticks per application; whether a deployer at speed 0 lets items pass; that the jam forms within the warm-up (measured at 64 RPM only).
- Bench CPU times vary up to about 1.7x between runs under other agents' load; the slopes held across 2 to 3 runs.
