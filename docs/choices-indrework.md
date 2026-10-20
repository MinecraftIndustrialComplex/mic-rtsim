# Choices: the industry rework after Ben's morning answers (2026-10-20)

**Branch `rework/industry-morning`.** Ben answered the open questions on the morning of 2026-10-20
(DECISIONS.md, "Answers to the open questions"): I6, I7, I9, I13, I18, U10, the stale-tick row, P14,
C28 and C29 changed the industry and conflict engines. This branch is their Rust side, on the black-box
path, which C29 keeps as the glass box's fallback and checksum. Reversible choices made to build it are
numbered IR1 onward, one line of reason each. DECISIONS.md, PLAN.md, INDUSTRY.md and OPEN-QUESTIONS.md
are unchanged. Every number marked PLACEHOLDER is not measured.

## Learned coupling (I6)

| # | Choice | Why |
|---|---|---|
| IR1 | **Two goods share a process unless evidence separates them** (`Coupling::partition`): every consumed good feeds every produced good, unless it has `coupling_min_episodes` (PLACEHOLDER 2) valid starvation episodes and that output kept moving in every one; a learned target joins every output (targets are learned with every output quiet); outputs fed by no input (a mob farm) are one process; over 48 goods, one process. Union-find over the goods; groups sorted by their smallest good. | Ben's rule stays "never better than reality": a split is made only where a real episode showed the line going on without the input. An input that never ran out (a fuel nobody lets run dry) cannot be ruled out of any line, so until it starves the district stays all-or-nothing, the overnight default. |
| IR2 | **An episode** is an input below one sweep's largest use (a machine taking 4 a batch stops with 2 left), measured from `coupling_settle_ticks` (PLACEHOLDER 1,200) after it ran out to its last sweep still out, at least `coupling_min_span_ticks` (PLACEHOLDER 2,400) long. It counts only if the input was not used meanwhile (at most `coupling_stopped_share`, PLACEHOLDER 0.2, of its rate), and an output "moved" only if it made at least `coupling_moved_share` (PLACEHOLDER 0.8) of its rate and `coupling_min_batches` (PLACEHOLDER 3) of its own largest sweep-to-sweep steps. Evidence is kept across windows and unloads, reset by a player edit or a new revision; open episodes end unjudged when the window is discarded. | An input fed as fast as it is used reads 0 in stock but is no starvation; judging it would split a line from the input it needs. One batch already inside a machine finishes once when its input runs out, and must not pass for a line that goes on. The settle time is not derived from the learned latency (F23): that is the longest gap between rises, which the starvation gaps themselves stretch (found by the test: every episode was swallowed). |
| IR3 | **A good belongs to one process, at its full learned rate**: no rate is split between processes. | A shared input whose share per line is unknown keeps the lines together (IR1), so a split never needs one. Rates are window averages, so a line that starved while learning runs slower in the replay than it can (conservative). |
| IR4 | **A PowerGrid sink requires every box process of its source** (`requires: Option<DistrictId>`), not one numbered process. | The black box cannot tell which of a source's processes is its generator: any of them stopping cuts the power (the safe side). The coupling of running states (I13) is otherwise unchanged. |
| IR5 | **The district model is an interface** (`model.rs`: `DistrictModel::processes`; `BlackBox` implements it). `District::member_spec` does everything model-independent: pools, placement (I7), caps, item space, power, package links, jammed outputs. | C28's compiled glass box (GLASSBOX.md, not approved) can slot in as another `DistrictModel`, run by the same cluster with the same breakpoints, reconciles and threats. |

## Simulate the rest (I7)

| # | Choice | Why |
|---|---|---|
| IR6 | **Coverage is per chunk, by load, not by tick**: a chunk leaves the replay at IND_CHUNK_LOADED and rejoins at **IND_CHUNK_UNLOADED** (new, journaled, 0x040A, at `ChunkEvent.Unload`). A Java side that never sends it is read the old way: with no chunk ticking, the whole district is unloaded again. | A loaded chunk's vault is reachable by a ticking neighbour's machine, so the safe side takes it out of the replay at load. Loads and unloads are events, exact, with no 20-tick poll. The fallback keeps today's Java side and its GameTests working until it sends the record. |
| IR7 | **A process runs in a partial replay only if every good it touches lives in the unloaded part**: the live inventories that received or hold it are all unloaded (a deposit, by its chunk). One that reaches a loaded chunk runs nowhere in the replay; a good that lives nowhere known keeps its process waiting. | The black box sees no machines: a process with a home in a loaded chunk may be running for real there, and must never also run in the replay. With machines in a loaded chunk and stock in an unloaded one, nothing runs (Create cannot reach an unloaded chunk either). Never better than reality. |
| IR8 | **A partial round** plans the replay's change over the inventories it covered and sends only the leaving chunks' deltas, each chunk on its own epoch (the district's epoch counter; each chunk's attachment holds its own applied epoch). Staying inventories keep theirs as **`held`** (counted in their contents, so the replay that goes on starts from them, and sent when their chunk leaves); what fits nowhere stays `owed`. Coverage changes are brought up to date once a tick's records are in, so a district whose chunks all load in one tick reconciles in one round. The trim cap rides on with the virtual part. | A district-wide `owed` would misattribute the remainder when the district later loads whole; per inventory it lands where it was planned. Deferring to the end of the tick keeps today's single round for a whole-district load. |

## The stale-tick race: reconcile ahead of arrival

| # | Choice | Why |
|---|---|---|
| IR9 | **A closing IND_APPROACH** within `approach_distance_blocks` (PLACEHOLDER 256, the Java side's own radius) stops the district's replay and sends every chunk's reconcile at once (`Round::Ahead`), and every older unsent command. Each closing record extends a **hold** (PLACEHOLDER 100 ticks, five of Java's 20-tick reports); when it ends the replay goes on from the reconciled state. A record that is not closing never starts a hold. | Ben's choice. The district makes nothing while held, so it is never better than reality; a player standing still near a district does not freeze it. |
| IR10 | **Java applies a held reconcile at the chunk's `ChunkEvent.Load`**, before any block entity there ticks, and journals its IND_RECONCILE_RESULT before the chunk's IND_CHUNK_LOADED. The fake Java side does exactly that. | The engine half alone cannot close the race: the command must land before the first tick, which only the bridge can do. |

**The I-B check, re-measured** (the iron line, `the_iron_line_conserves_and_meets_ib`, 10 days, seed 11;
`rtsim-cli sim industry` prints the same): control **9,296** ingots; the protocol as built (machines tick on
stale stock, Java sends IND_APPROACH) **9,269**, that is -27 against the control, 99.71%, never above it;
without the approach **9,309** (+13 against the control, the +40 leak against the race-free run, as before,
measured on main's own code too); the ideal race-free Java side 9,269. The approach removes the leak
entirely: the power plant's coal reconcile lands before its boiler ticks.

## Replay everything (I9), level pollution live (I14), Ender Transmission (I13), no free visibility (I18)

| # | Choice | Why |
|---|---|---|
| IR11 | **I14: deposit write-back is routed by good first** (`deposit::writeback`): Destroy's level-wide types (`destroy:greenhouse`, `destroy:ozone_depletion`, `destroy:acid_rain`, as the plugins' attributed deposit goods, Q9) are `Writeback::Live`: what the replay made is pushed as IND_DEPOSIT_DELTA every `live_pollution_ticks` (PLACEHOLDER 1,200) while the district replays, and the rest at its reconcile. A push marks the pool pushed and moves the deposit's `remaining`, so the reconcile writes back only what was not pushed. Chunk types ride the chunk's reconcile. | Ben decided "level-wide types apply live while unloaded"; Java already applies such a delta to the level at once. **The one replay cost that grows with time away**: a command per polluting unloaded district per period, which "live" asks for. |
| IR12 | **I9: not-learnable code 6 (spawn-fed farm) is read as learnable.** | Ben chose "Everything". An older Java side may still send it. |
| IR13 | **I13: the channel machinery is gone**: `PoolKey::Channel`, `LinkKey::Channel`, channel clusters, IND_CHANNEL_SAMPLE (0x0043), IND_CHANNEL_DELTA (0x8402) and port kind 7, from the engine, wire.rs, the fixtures, IndRecords and the wire contract test (52 of 52 against the regenerated fixtures). Both kinds are read as unknown kinds. Clusters join districts only by PowerGrid lines. | Ben's correction: an Ender Transmission channel's endpoints belong to one district or vehicle, so it is internal plumbing, never stock shared by distant districts; the industry plugins merged their endpoints into one district on the Java side, and nothing sends the records. |
| IR14 | **I18: the engine shows players nothing.** It never writes IND_LEDGER and ignores IND_LEDGER_REQUEST; the ledger report and `Cluster::forecast` are gone; read-only mode (phase I-A, F25), which existed only to show what a district would have made, is removed with its `Mode`, and the native link reads its `nOpen` flag and ignores it with a warning. `Engine::inspect` stays, a read for tests, the CLI and an operator's log that never reaches the Java side. **Update (rework/i18-no-ledger):** the Java display source and chat ledger are gone, so the IND_LEDGER and IND_LEDGER_REQUEST codecs, their kinds (0x8405, 0x0044) and the `retired_records` counter are removed too; both kinds now read as unknown kinds. | Ben: players see only what their own built readouts measure. Removing the codec now would break the Java side, which still decodes it, before its agent removes the display source. |

## The switch (U10) and the conflict (P14)

| # | Choice | Why |
|---|---|---|
| IR15 | **U10: a switched-off industry ignores everything**: the native link applies no industry record, journaled ones and COMMAND_RESULTs included (counted, `IndustryStats::ignored`), runs nothing and writes nothing; its queue, outbox and pages wait as they are. Switched on in a session, the engine's own session start runs at that game time, so it catches up over the time it was off; every kept command goes again under a new sequence. Java announces its districts again then (the fake Java side's `FakeWorld::announce`); the engine keeps what it knows of an inventory declared again unchanged. | Ben's rule, the dragons' and ships'. An answer that came while off was ignored, so a kept command goes again and Java answers it by op id (DUPLICATE if applied). Unlike phase 1's PR13, COMMAND_RESULTs are not taken while off: a failed carriage delta's take-back advances the engine, which must stay frozen. The cost: an IND_CARRIAGE_RESULT journaled while off is lost, and that one exchange keeps its record without its correction. Without the announcement a district detected while off would be unknown to the engine for good. |
| IR16 | **P14: a per-chunk time-spent counter in rtsim-conflict** (`presence.rs`): each chunk keeps its 3 players (PLACEHOLDER) with the most decayed time (half-life PLACEHOLDER 216,000 ticks, three days), at most 65,536 chunks (PLACEHOLDER; the one touched longest ago goes), a gap between reports counting at most 1,200 ticks (PLACEHOLDER, as P26's `Homes::max_gap`). Every PLAYER report feeds it (rtsim-jni `shared.rs`, and `observe_player`); O(log chunks) a report, no scans. | P26's home counter is per player and per 512-block region, the wrong way round for "who owns this chunk"; a counter per chunk answers it directly and stays bounded. |
| IR17 | **A source carries its owner and its chunks**: an industry district's chunks (at most 64, PLACEHOLDER), or a POLLUTION record's one chunk; its owner is the player with the most time in them together, the settlers only while nobody has spent time there. When a chunk's leading player changes, its sources move to the new owner, grievance feed and all (`ConflictStats::reattributed`). | Ben's "per chunk by time spent"; summing over a district's chunks lets the player who works anywhere in a factory own its smog, not only one who stands at its centre. |
| IR18 | **`sim conflict` follows the new owner**: the refinery's smog now counts against the player who runs it, sharing a feud with that player's logging. The checks follow the smog's owner instead of "the settlers"; the converter run still meets the smog demand by cutting the signal with no band marching, and its feud now calms to notice rather than to nothing (the logging, a direct act, keeps it there). `scenario/results/sim-conflict.json` refreshed. | The scenario's narrative is unchanged; only who the tribe holds responsible is, as P14 now says. |

## Scaffolding and smaller choices

| # | Choice | Why |
|---|---|---|
| IR19 | **The fake world loads chunk by chunk** (`ChunkVisit`, on top of district `Visit`s): a district's chunks include its machines' and surveyed blocks'; a recipe runs only with all its chunks loaded; sweeps read only loaded inventories and are live only when all are; it sends IND_CHUNK_UNLOADED (`chunk_unload_records`), IND_APPROACH ahead of a scripted load (`approach_signal`, `approach_lead` PLACEHOLDER 60 ticks), applies held reconciles at a chunk's load, and books goods brought in from outside (`import`, `WorldBooks::imported`). `IronLine` gains `approach` and loses `mode`; `run_iron_line_until` resumes a cut run with the player's repair. | To test I7 and the approach the way the real Java side behaves. The persist test's cut at day 9.5 had skipped the scripted repair, which the old code happened to forgive. |
| IR20 | **New counters**: `Stats::reconciles_ahead`, `pollution_pushes`, `retired_records`; `IndustryStats::ignored`; `ConflictStats::reattributed`. | Tests and the CLI read them. |

**Not built:** IND_CLIMATE (the industry plugins' proposal for I12's conditions): it did not fall out of
this work, and the season models are unchanged.

## Findings for Ben

1. **The learned batch latency (F23) is stretched by starvation gaps**: it is the longest gap between an
   output's rises in the window, so a line that starved while learning waits longer before a restart in
   the replay than its machine really does. Conservative, left as is; it is why IR2 stopped using it.
2. **Goods that arrive after a district's last sweep before it unloads are not in its replay's start**:
   the engine never saw them. Conservative (the reconcile plans from what it knew); a sweep at unload
   would close it.
3. **A player who keeps closing in on a district without ever loading it keeps it held** (IR9): circling
   at the edge of the approach radius freezes a factory. A record that is not closing never starts a
   hold, so standing still does not.
4. **P14 merges a player's own acts and their factory's pollution into one feud** (IR18): a tribe that
   would have pressed the settlers over smog now presses the player who runs the refinery, and paying
   one demand meets that player's whole grievance.
5. **I7's partial replay is coarse by design**: a process with any home in a loaded chunk stops, and
   chunks between the view and simulation distances (loaded, not ticking) run nowhere, in the world or
   the replay.

## Java pointers (for the Java agents)

1. **I18, no free visibility** (done, branch `rework/i18-no-ledger`): the "District ledger" display source,
   the chat ledger, the IND_LEDGER_REQUEST sending, the IND_LEDGER handling and the ledger GameTest are
   gone, and IND_LEDGER_REQUEST (0x0044) and IND_LEDGER (0x8405) are removed from wire.rs, IndRecords,
   `neoforge/tools/industry-wire-fixtures` and the contract test. `/rtsim district info` and `debug` are
   operator output (permission 2) from what Java holds, with no engine round trip. Plugin warnings (Q25)
   stay, as `DistrictDiagnostics.warn`: server log and the operator output only.
2. **Reconcile ahead of arrival**: apply a held IND_CHUNK_RECONCILE at the chunk's `ChunkEvent.Load`,
   before any block entity ticks (today: at the next server tick), and journal its IND_RECONCILE_RESULT
   before the chunk's IND_CHUNK_LOADED. Send IND_APPROACH for partly loaded districts too (a player
   walking toward the unloaded part), and keep `closing` (aux 1) accurate: only closing records hold a
   district.
3. **I7** (done): IND_CHUNK_UNLOADED (0x040A, `IndRecords.ChunkUnloaded`) is journaled at `ChunkEvent.Unload`
   for every chunk of a district (`Districts.onChunkUnload`).
4. **U10** (done): with `Module.INDUSTRY` off Java sends nothing (`Industry.endOfTick` returns early and
   `accept` answers nothing); when it turns on, the next tick announces every district (IND_DISTRICT,
   IND_DISTRICT_CHUNKS, IND_INVENTORY, IND_STATION), the load states and the watched flags, every loaded
   chunk (IND_CHUNK_LOADED), the calendar (IND_CONDITIONS), the surveys (IND_SURVEY with its blocks) and,
   through `IndustryPlugin.announce`, the plugins' own (IND_POWER_LINK), as at a session start.
5. **I9** (done): Java never sends not-learnable code 6 (`IndRecords.NotLearnable.sent`); no subtraction of
   mob drops is needed.
6. **I14**: IND_DEPOSIT_DELTA for a level pollution good now comes every minute while its district is
   unloaded; the Destroy plugin already applies it to the level at once.
7. **Page formats changed** (industry and conflict): delete each project's `run-gametest/world` before
   the next GameTest run (W23 would drop the pages loudly anyway).

## Tests

- rtsim-industry: `learn` unit tests (the partition with and without evidence, a shared input, targets
  and input-less outputs, an episode judged from sweeps, an input fed as fast as used), `deposit`'s route
  by good, `cluster`'s coupling groups stopping apart; `tests/rework.rs` end to end: learned coupling
  keeps an independent line running and the default does not; the unloaded part of a district keeps
  running, each chunk on its own epoch; the old Java side without chunk unloads; a spawn-fed farm
  replays; a reconcile ahead of arrival leaves no stale tick; a player who turns away; level pollution
  live. The I-B test now runs the protocol as built and prints all four numbers.
- rtsim-jni: the mill through the native engine with an approach and a player who turns away; the
  switch ignoring everything; a switched-off industry catching up.
- rtsim-conflict: the presence counter's unit tests; pollution going to the player who spends the most
  time in its chunks, moving with them, across a district's chunks, and through a save.
- Java: the wire contract test, 52 of 52; the Ender Transmission and Create plugins compile.
