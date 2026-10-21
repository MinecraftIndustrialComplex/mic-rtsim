# Choices: the kernel review bugs, and the modules on paging (PM)

Reversible choices made on 2026-10-20 by the paging-modules agent (branch `feat/paging-modules`),
for Ben's review (D10). Each one can be undone.

## Summary

Both kernel review bugs are fixed with a failing test first (PM1, PM2). Herds, dragons, conflict,
ships and the phase 1 world's markets now keep their rows on the page store: a meta page holds
what stays resident, and row pages are written only when they change. Industry did not move
because the glass box has not merged. rtsim-jni does not page any module yet.

Every module gives the same state hash paged and resident at every point. A save reopened from
its pages continues exactly like the run that kept going (each module's paged tests).

Each module's own memory as its world grows: rows all resident, against rows paged under a fixed
budget. The rest of the process heap is the phase 1 world under the module.

| Module | Grows | Rows resident: module memory | Paged: module memory | Paged: rows in memory at peak (budget) | Peak slope |
|---|---|---|---|---|---|
| Herds | 256 to 16,384 herds | 1.5 to 95.1 MiB (live heap) | 1.2 to 10.9 MiB (live heap) | 1.17 to 2.04 MB (1 MiB) | 0.137 |
| Dragons | 512 to 32,768 dragons | 0.71 to 34.5 MB | 0.60 to 12.1 MB | 271 to 695 KB (256 KiB) | 0.23 |
| Conflict | 1,024 to 4,096 factions | 2.91 MB at 1,024 | 1.44 to 3.37 MB | 313 KB at both (256 KiB) | -0.001 |
| Ships | 372 to 51,774 ships | 0.54 to 20.6 MiB | 0.50 to 7.8 MiB | 82 to 90 KB (64 KiB) | 0.015 |
| Phase 1 markets | 4,096 villages | 49.1 MiB (live heap) | 42.7 MiB (live heap) | 15.8 MB (4 MiB): not bounded | |

What still grows with the world in a paged run:
- the meta page (the wheel and the position indexes, written whole by every save);
- the extra saves for the budget when every entity acts in the same window;
- the phase 1 world's other per-village parts (news, pipelines, the world wheel, knowledge
  holders).

"Not done here", at the end, lists them.

The scale bench before and after, on the full machine and the potato profile, is in
`bench/results/README.md` (2026-10-21 06:11 UTC). No contract fails after the change that did not
fail before. Live heap falls at every point; for example, 100,000 events a day fall from 50.8 to
10.1 MiB and 4,096 markets from 67.4 to 45.8 MiB. CPU stays within noise of before.

## The two open kernel review bugs

| # | Choice | Why |
|---|---|---|
| PM1 | **Two saves in flight.** A page now records the number of the latest snapshot that holds it (`Page::saving`, 0 for none), and each `SaveTicket` carries its snapshot's number. `saved` unpins a page only when its ticket is that latest snapshot; `save_failed` always marks its pages dirty again and unpins only its own. Test: `paging::tests::two_saves_in_flight_keep_the_later_change` (failed before the fix: the first ticket back unpinned a page the second snapshot still held, the budget evicted it, and the second save's failure lost its change). | The review's proposed fix. A later snapshot holds the newer bytes, so only its ticket says the page's state is on disk. Marking dirty on any failure is conservative: at worst a page is written twice. |
| PM2 | **An IO error inside a parallel step.** The step now pops its wakes and mail, then loads every page they touch, and only then groups and runs them. If a load fails, `TimingWheel::rewind` puts the popped wakes and mail back into both wheels at the step's start, in pop order (schedule order included), `PagedTable::due` is restored, and the step returns the error having changed nothing but which pages are resident. The retried step is the step that never failed. Tests: `bsp::tests::an_io_error_in_a_step_loses_nothing` (a read fault every third step, after 0 to 2 good reads, at 1 and 4 workers and step lengths of 200 and 4,800 ticks: same state hash, mail hash, wakes and messages as a clean run; it failed before the fix with 2,975 grain lost in dropped mail) and the property test `wheel::tests::a_rewound_wheel_pops_as_if_never_popped`. A test-only read fault, `page::Fault::Read { after }`, makes the store's `get` fail like a disk error. | The review offered "re-book what was popped, or load first". Re-booking alone cannot work: the wheels' clocks have moved to the window's end, so a re-booked wake would fire at the next window's start, not its own tick. Loading first without popping would need a second pass over every due entry in every step; the rewind costs nothing unless a load fails, and then O(pending wakes), which an IO error can afford (D1). |

## Measured first: what grows with the world today

`rtsim-cli bench scale --quick --only markets,dragons,ships,herds,members,factions` at commit
1c43097 (release, nice 10, load average 11 to 20 from other sessions; the run is in
`bench/results/scale-*-quick-before-quick.json` and its table in this section's numbers). Heap is
the counting allocator's live heap at the end of each point; "engine" is the module's own
`heap_bytes` estimate. Component splits come from `World::memory()` and the modules'
`heap_bytes` at the same points (throwaway probes, not committed).

| Sweep (what grows) | Points | Live heap | Module's own estimate | Where it goes |
|---|---|---|---|---|
| markets (villages, phase 1 world) | 16, 256, 4,096 | 0.3, 4.0, 65.5 MiB (slope 0.96) | 0.18, 2.2, 36.3 MiB | 16.8 KB per village at 4,096: market rows 6.6 KB (40%), world wheel and wakes 2.3 KB, pipelines 2.8 KB, news 2.3 KB, edges 1.1 KB, nodes 0.7 KB |
| factions (camps, one village each) | 16, 256, 1,024 | 1.0, 25.2, 126 MiB (slope 1.16, over its own 1.1 contract) | conflict engine 0.04, 0.62, 2.5 MiB | the phase 1 world under the camps: 104.5 MiB at 2,048 nodes, of which knowledge 38.5 MiB (superlinear: 5.6 MiB at 512 nodes), market rows 24.8, leg index 14.3, world wheel 7.9 |
| dragons (on a fixed 256-village world) | 8, 128, 512 | 4.6, 7.6, 12.3 MiB | dragon engine 0.04, 0.17, 0.65 MiB | 1.2 KB per dragon in the engine; the world grows 6.3 MiB, mostly knowledge holders for the dragons (0 to 4.2 MiB), the market table and the leg index |
| herds (5,000 members each) | 4, 64, 256 | 0.11, 0.67, 1.35 MiB | 0.10, 0.60, 1.26 MiB | at 256: herd rows 343 KB (1.3 KB per herd), forage cells 432 KB (the bench valley is fixed, so forage saturates), region index, wheel and the rest 460 KB |
| ships (per service, 100 ports) | 1, 4, 16 | 3.4, 3.0, 3.8 MiB | 0.13, 0.18, 0.38 MiB | about 0.25 KB per ship; the rest is the fixed port network and its world |
| members (per herd, 16 herds) | 100 to 1e6 | 0.08 to 0.59 MiB | | latent: grazed forage cells grow with footprints up to their cap |

**What grows with world size, biggest first:**
1. **The phase 1 world** (rtsim-world over settle, trade, travel, knowledge): about 17 KB per
   village in a plain grid and about 51 KB per node in the conflict world, where raids and envoys
   make reports. It is the largest term in every sweep that has villages. Knowledge holders
   (noticeboards and sentiments per village, dragon, faction and notable) are the one superlinear
   part until their caps fill.
2. **Herds**: about 3 KB per herd (1.3 KB row, the rest its region index entries and wheel), and
   forage cells per grazed cell.
3. **Conflict**: about 2.5 KB per faction in the engine (its villages and havens are phase 1's).
4. **Dragons**: about 1.2 KB per dragon in the engine, plus about 8 KB of phase 1 knowledge per
   dragon actor.
5. **Ships**: about 0.25 KB per ship, and the port network.

Nothing in any module pages today: every module saves one blob (`save()`), rtsim-jni writes it
as one page per module (0x4400 dragons, 0x4800 herds, 0x4300 conflict, 0x5300 ships), and phase 1
as snapshot pieces plus an input log (0x5000, 0x5001).

## Design: module state on the page store

### The kernel pieces

- **Lazy regions** stay on `paging::PagedTable` (K1 to K7): a model with catch-up and
  breakpoints, its wake wheel resident.
- **Entity rows** go on the new `rows::PagedRows<T>` (keyed by a stable `u64`) and
  `rows::PagedArena<T>` (the `Arena` interface; handles are keys and are never reused, so no free
  list or generation table stays resident). A row is read through `&self` and loads its page on
  a miss; changes take `&mut self` and dirty the page; `enforce_budget` at step boundaries evicts
  clean pages in least-recently-used order. Without a page source the table is resident, as an
  arena; with one it pages. Both give the same page bytes and the same `state_hash` (property
  test `rows::tests::paged_equals_resident`).
- **The store is shared, not passed.** `rows::PageSource` is implemented for `PageStore`,
  `RwLock<PageStore>`, `Mutex<PageStore>` and rtsim-jni's `Mutex<Option<PageStore>>`. A module
  keeps an `Arc<dyn PageSource>`; the save path commits through the same store. Module methods
  keep their signatures.

### Per module: what is paged and what stays resident

| Module | Paged (keyed by) | Resident (why) |
|---|---|---|
| phase 1 world | market rows (the settlement key; they are lazy regions); news rings and knowledge holders (the settlement or actor key) | the flow graph (nodes, edges and their pipelines: SCALE.md §8 keeps the flow graph resident), the world wheel, packets and caravans in flight, the leg index (activity), settlement and actor indexes |
| herds | herd rows (herd handle index), forage cells (packed cell), the region index's entries (packed region) | the wheel (about one pending decision per herd), players, claims of the current sub-season, ops, obstacles (reports, capped by activity) |
| dragons | dragon rows (handle index), lairs (lair index) | the wheel, players, homes, the prey ledger, the cell indexes of lairs (an index from position to id) |
| conflict | factions (index), feuds (faction, actor), sources (key, kind) | the wheel, parties in flight, presence (capped), players, the cell indexes |
| ships | ship rows (handle index), per-lane threats | the wheel, the port network (docks, lanes, services: the flow graph), havens, navies, booked voyages (observed only) |
| industry | not moved here (task rule: only after the glass-box branch merges); see the end | |

**Resident memory, read honestly.** After the move a module's resident state is the budget
(rows) plus what SCALE.md §8 keeps resident: indexes from id to place, the flow graph, and the
wheel of pending events. The wheel is a few dozen bytes per entity that has an event booked, so a
module whose every entity books an event a day still grows by that much per entity; that is the
"regions with imminent events" term of SCALE.md §8's working set, not a page. The benches below
report the paged tables' resident bytes (bounded by the budget) and the whole heap (whose slope
is the wheel and indexes) separately.

### Lazy catch-up meets a paged-out page

- **Lazy regions** (markets): unchanged from K3 and K4. Catch-up is a pure function of the
  stored state and the time (O4), so a page changed only by catch-up may be dropped unsaved and
  recomputed; a wake is a wheel entry `(key, booked tick)` that is live only if the page, once
  loaded, holds the same booked tick. Reads never reschedule.
- **Entity rows** (herds, dragons, factions, ships): their between-event state is already a
  closed form of the row and the clock (a herd's position along its route, cohorts as dyadic
  counts, a ship on its timetable, a feud's grievance curve). A paged-out row needs nothing until
  an event or a read names it. Every wheel entry carries the row's key and the row's plan stamp
  (`plan`, `stamp`, `generation`), so firing it loads the page and drops the event if the stamp is
  stale, as now. Reads of entity rows never write, so a read-only page stays clean and can be
  evicted again. Nothing ever catches up on load or on eviction.
- **Prefetch**: a module's wheel names the next events' keys, so their pages can be read ahead as
  K6 does for regions. Not built for rows yet: rows load on demand on the step thread.

### How saves pick it up

- **Unpaged** (no store, or rtsim-jni before it wires paging): the module's `save()` blob as today,
  rows written by `encode_all` in key order. The format version of each module page changes once,
  because `PagedArena`'s blob is not `Arena`'s.
- **Paged**: one commit holds, for each module, its **meta page** (the resident state above, plus
  each row table's count and next index) and each row table's **dirty pages**
  (`PagedRows::save_dirty`), and the tickets come back after the commit is durable (`saved`, PM1).
  Opening reads the meta page and opens each row table with nothing resident.
- **State hash**: `fnv1a64(save())` as today, with rows in key order: identical paged and
  unpaged, and it costs the whole module (tests and tools only, never per save in the game).
- **Two generations** (P29, K12) need nothing new: row pages are pages.

### Page tables

Row tables take the module's table plus 0x10 and up, so rtsim-jni's per-table deletion
(`deletions`, which deletes the module's own table's pages a save no longer has) never touches
them: herds 0x4810 (rows), 0x4811 (forage cells), 0x4812 (region index); dragons 0x4410 (rows),
0x4411 (lairs); conflict 0x4310 (factions), 0x4311 (feuds), 0x4312 (sources); ships 0x5310 (rows),
0x5311 (lane threats); phase 1 0x5010 (markets), 0x5011 (knowledge holders).

## Herds (done first of the modules: the second-largest growth, and the one whose rows the dragons read)

| # | Choice | Why |
|---|---|---|
| PM3 | **Herd rows, forage cells and the region index are row tables**: herds on a `PagedArena` (table 0x4810, 32 herds a page), forage cells on `PagedRows` keyed by `cell_key` (0x4811, 8 x 8 cells a page), the region index on `PagedRows` keyed by `cell_key(frame, rx, rz)` (0x4812, 4 x 4 regions a page, each row the sorted herd indices). The meta page (0x4813, id 0) holds the rest: the wheel, players, claims, obstacles, ops, the active set, the predator map and each table's counts. | The largest per-herd terms are the row (1.3 KB) and its grazed forage; the region index is per herd too. |
| PM4 | **Each herd keeps its own region list in its row** (`Herd::regions`), replacing the engine's resident `herd_regions` map. The region index is rebuilt from those lists on a blob load and kept as pages when paged; only a new topology recomputes every herd's regions from its route. | A paged engine must never recompute anything from every herd. Bonus: a loaded engine's index is now exactly the saved engine's (the old rebuild dropped extinct herds' entries, so a loaded engine could differ from one that never stopped). |
| PM5 | **An active set** (herds that may hold player legs or an embodied group; a superset pruned when walked) replaces the whole-table walks in `player_left` and `session_start`, and is saved with the predator map (both used to be rebuilt by walking every herd on load). | Those walks loaded every page on every logout. |
| PM6 | **Herds decide by page.** The decision stagger (a quarter day after each sub-season begins) is now a hash of the herd's row page, page-mates on consecutive ticks, instead of a hash of the herd. | Every herd decides within the same quarter day, between two autosaves; staggered by herd, a burst dirtied every page in the first few ticks, so the dirty set (which cannot be evicted before a save) was the whole table. By page, a tick dirties one or two herd pages. Behaviour: the same spread, different herds early. |
| PM7 | **The host saves when the rows are over budget with every page dirty** (`Herds::needs_save`), an extra pending save like any other (P29: a crash rolls it back like an unconfirmed autosave). The bench does this; rtsim-jni does not page modules yet (see the end). | Dirty pages cannot leave memory before a save, and the decision burst changes more than any budget holds. An extra save writes what the next autosave would have written anyway (a page dirtied twice is written twice). |
| PM8 | **The timing wheel frees a burst's vectors**: emptied slot vectors over 256 entries are dropped instead of kept as spares (at most 64 spares), and the ready queue shrinks to 1,024 once drained. | After a herd burst the wheel held 410 bytes per pending decision in idle capacity (1.7 MB at 4,096 herds); now 147 (0.6 MB). Allocation only: pop order is untouched. |
| PM9 | **Budget split** over the herds' tables: half to herd rows, 35% to forage cells, the rest to the region index. PLACEHOLDER shares. | Measured working sets at 4,096 herds: 0.5 MB herd rows, 0.46 forage, 0.18 index. |

**Measured** (quick sweeps, release, nice 10, load 30 to 40; `herds-wide` grows the valley with
the herds at one density, resident; `herds-paged` the same paged under a 1 MiB budget, stepped tick
by tick, autosaved every 6,000 ticks; one sub-season, so one burst of decisions per herd). Both
give the same state hash at every point.

| Herds | resident: heap | paged: heap | paged: rows resident at peak | paged: saves a day (of them for the budget) | bytes per save |
|---|---|---|---|---|---|
| 256 | 1.5 MiB | 1.2 MiB | 1.17 MB | 4.4 (0.4) | 35 KB |
| 1,024 | 5.8 MiB | 1.5 MiB | 1.41 MB | 6.5 (2.5) | 145 KB |
| 4,096 | 22.8 MiB | 2.5 MiB | 1.71 MB | 14.8 (10.8) | 440 KB |
| 16,384 (one-off pair, before PM8) | 95.1 MiB | 10.9 MiB | 2.04 MB | 48.6 (44.6) | 800 KB |

Slopes: resident heap 0.98; paged rows resident 0.137 (declared 0.15), paged heap 0.27 (declared
0.4: the wheel's pending decision per herd and the sub-season's claims stay resident by design,
about 190 bytes a herd); CPU per herd-day flat in both (0.06), the paged run within noise of the
resident one (save CPU excluded and reported apart).

**Not flat, and why.** A herd changes once a sub-season and every herd changes in the same quarter
day, so the bytes a save must write grow with the herds, and so do the saves the budget asks for
(44 a day at 16,384 herds, each under 1 MB). Spreading decisions over the whole sub-season would
cut that burst 32 times; it changes when herds decide, so it is Ben's call (a tunable,
`stagger_ticks`).

## Dragons

Branch `feat/paging-dragons` (rtsim-ecology `engine.rs`, `dragon.rs`, `persist.rs`; the bench in
rtsim-cli `bench/dragons.rs`). Dragons are few in a real game (Ice and Fire spaces its lairs
about two a region), so the point here is that nothing a dragon does costs a pass over every
dragon, and that the rows stay under a budget however many there are (D6).

| # | Choice | Why |
|---|---|---|
| PM-D1 | **Dragons on a `PagedArena<Dragon>` (table 0x4410), lairs on `PagedRows<Lair>` (0x4411) behind a `Lairs` newtype** keyed by the lair index. Lairs are append-only, so the keys are exactly `0..len`, a `LairId` is its key, and `Lairs` keeps `Index` and `IndexMut` (a missing lair panics, as the `Vec` did). Dragon handles are never reused (generation 1), so a dead dragon's id finds nothing. `Dragon` and `Lair` keep their encodings (`put_dragon`, `put_lair`) as `Encode` and `Decode`; `Row::heap_bytes` counts names, vectors and maps. | The task's tables. No id map stays resident for either: the lair index is dense, and the arena's handle is its key. |
| PM-D2 | **Two save forms, one meta.** The meta (magic `RTDM`) is everything resident: seed, clock, the lair indexes, the wheel, homes, the prey ledger, stats, the outbox, players, the prey index, the body maps, the embodied and vacant sets, and each table's row count and next index. `save()` stays one blob: `RTDG`, the meta, then both tables' `encode_all` in key order; `load()` reads it back and checks the lair keys are exactly `0..len`, no trailing bytes, then `check_resident` and `check_rows`. Paged: `save_paged(batch, meta_key)` puts the meta page and each table's dirty pages, `saved` and `save_failed` take the tickets, `open_paged` reads the meta page only and runs `check_resident` (indexes, the vacant set and wheel entries against the lair count, embodied handles against the next index), which reads no row. The meta key is the host's (the tests and bench use (0x4400, 0), the page rtsim-jni writes the blob to; it must not be in a row table). | `PagedArena`'s blob is not `Arena`'s, so the 0x4400 page's format changed once: rtsim-jni's dragons page `VERSION` is now 2 (no compatibility before release). `check_rows` is a whole pass, so only the blob load runs it. |
| PM-D3 | **One row a page for both tables** (`DRAGONS_PER_PAGE`, `Lairs::PER_PAGE`, PLACEHOLDER 1). Measured at 4,096 dragons, a save every 6,000 ticks, 256 KiB budget, before PM-D5's extra saves: resident rows peaked at 1.56 MB at 16 a page, 794 KB at 4, 322 KB at 1. | Most of a dragon's events change its row, and a changed page is pinned until the next save, so a page of 16 dragons was pinned whenever any of them acted. The cost of 1 is an entry a row in the store's index (2.76 MB at 32,768 dragons, measured). |
| PM-D4 | **Budget split**: three quarters to dragon rows, a quarter to lair rows. PLACEHOLDER shares. | Every event reads its dragon; fewer read a lair. Resident, a dragon row is about 410 bytes and a lair row about 180 (measured below). |
| PM-D5 | **The host enforces the budget and saves when it asks.** `enforce_budget` is the host's call at its step boundaries (the bench every 20 ticks); `run_until` calls it only while resident, to keep `heap_bytes` measured. `needs_save` (as herds' PM7) says the rows are over the budget with nothing left to evict, and the host saves: an extra pending save like any other (P29). | Calling it per event would pay the check (a pass over the resident pages when over budget) once an event. Without the extra save, rows changed between autosaves are pinned and grow with dragons times the autosave interval: the peak was 1.87 MB at 32,768 dragons (slope 0.47); with it, 695 KB (slope 0.23). |
| PM-D6 | **An embodied index** (`embodied`, the dragons with a body in the world, kept by `track` after every state change and saved in the meta) replaces `session_start`'s pass over every dragon. | A login loaded every dragon page. Bounded by activity: bodies exist only near players. |
| PM-D7 | **A vacant-lair index** (`vacant`, lairs with no occupant and a `vacant_since`, kept by `track_lair` and saved in the meta). `vacant_lair` (a roost or migration search over the cells near a dragon) skips lairs not in it before reading their rows. | The search read every lair row in the regions within the dragon's grudge range, almost all occupied. Resident cost about 6 bytes a vacant lair; at most the lairs. |
| PM-D8 | **`lair_by_key` and `lair_cells` stay resident** and are saved whole in the meta: about 60 bytes a lair resident (B-tree entries at two-thirds fill and each cell's vector) and about 20 bytes a lair in every save. Not built: page `lair_cells` by `rows::cell_key`, so a save writes only the cells that changed. | The task allows it, and lairs change rarely. It is the part of the meta that grows with lairs; the wheel is the larger part (PM-D13). |
| PM-D9 | **Reads before writes**: `release_claim` and `robbed` read before they write, `feed` returns before borrowing when there is no food and no victim, `cancel_leg` when there is no leg; the no-op `get_mut` in `disperse` is gone. | A mutable borrow dirties the page whether or not anything changes, and a dirty page is pinned. Same behaviour: paged and resident runs give the same state hash at every bench point. |
| PM-D10 | **The kill's heir log** looks through the victim's children and mate, not every dragon. **`remove`** also drops `adopted` entries naming the dragon. | The first was a whole-table pass on a player action. The second keeps a removed dragon's handle out of the resident maps, so none names a missing row. |
| PM-D11 | **Whole passes left, for tools and tests only**: `by_name`, `check`, `state_hash` and `save()` read every row (on a paged engine, every page). | rtsim-jni's `encode_page` writes `save()` and `state_hash()` (a second `save()`) at every save, and restore hashes again; fine resident, and both go when paging is wired into rtsim-jni. |
| PM-D12 | **`Dragons::memory()`**: resident bytes by part (dragon rows, lair rows, lair indexes with the embodied and vacant sets, wheel, homes, prey index, log and players); `heap_bytes` is its sum and reads no row. | The bench and the notes report the parts; the old `heap_bytes` walked every row. |
| PM-D13 | **The bench** (`dragons_paged`, `dragons_resident`): a world that grows with the dragons (two dragons a village, `grown_villages`), a 256 KiB budget, an autosave every 6,000 ticks (fsynced after the first), the budget every 20 ticks and an extra save when it asks (PM-D5), the dragons' own CPU per dragon-day, and the same points resident beside it. The fixed-grid `dragons` sweep stays resident, for comparison with the runs before this branch. | A fixed world saturates its prey at 512 dragons; growing it keeps a dragon's life the same at every point, so the slopes are the engine's, not the prey's. |
| PM-D14 | **`synth::FakeIaf` derives `Clone`** (the test plugin). | The restore test runs the restored engine beside the original in lockstep, each with its own copy of the plugin's state at the save. |
| PM-D15 | **Kernel: a row page reserves room for its own rows only** (`PagedRows::try_insert` doubles a page's vector up to `rows_per_page` and stops; test `rows::tests::a_page_reserves_room_for_its_own_rows_only`). | A `Vec`'s first push reserves four, so a page of one row held room for four: a resident dragon row cost 1,534 bytes instead of 479 and a lair row 672 instead of 264 (pages loaded from the store were exact already). Memory only: page bytes, state and every hash are unchanged. It touches every module's row pages; herds' 32-row pages fill their doubling exactly, so only their partial pages shrink. |
| PM-D16 | **The resident contract is declared at 0.3, not 0.1** (`DRAGONS_RESIDENT_SLOPE`; SCALE.md's 0 kept beside it). After every save the rows are at or under the budget (the bench fails otherwise); the declared slope is for the peak between saves. | Measured 0.23. The peak is the budget plus the rows one 20-tick step changes, and the events in a step grow with the dragons; nothing can leave memory before the save that ends the step. |

**Measured** (quick sweeps, release, nice 10, load 31 to 38 from other sessions, commit a5b73a8;
`bench/results/scale-2026-10-21-0122-quick-dragons-after.json`). The world grows with the
dragons (two dragons a village); one lair a dragon; two days of warm-up, then the window. Paged:
256 KiB budget, autosave every 6,000 ticks, the budget every 20 ticks, an extra save when it asks.
Paged and resident give the same state hash and the same events at every point.

| Dragons (villages) | resident: heap | resident: engine | paged: heap | paged: engine | paged: rows at peak | rows after the last save | saves a day (of them the budget's) | bytes a save | save CPU a day | CPU a dragon-day, resident / paged |
|---|---|---|---|---|---|---|---|---|---|---|
| 512 (256) | 10.4 MiB | 0.71 MB | 10.4 MiB | 0.60 MB | 271 KB | 262 KB | 4.0 (0) | 37 KB | 7.6 ms | 31 / 41 us |
| 4,096 (2,048) | 90.3 MiB | 4.68 MB | 88.7 MiB | 2.10 MB | 322 KB | 262 KB | 4.3 (0.3) | 282 KB | 21 ms | 46 / 54 us |
| 32,768 (16,384) | 753.7 MiB | 34.5 MB | 735.2 MiB | 12.1 MB | 695 KB | 262 KB | 50 (46) | 1.95 MB | 1.36 s | 52 / 72 us |

Slopes: paged rows at peak 0.23 (declared 0.3, PM-D16; max/min 2.56); paged heap 1.02 and
resident heap 1.03, both the phase 1 world under the dragons (about 23 KB a dragon at two dragons
a village); the dragons' own engine heap 0.72 paged, 0.93 resident. Page loads: 112, 1,708 and
39,345 a day (about 1.4 an event); the store's index 111 KB, 799 KB and 2.76 MB (PM-D3).

**Where a resident dragon's bytes go** (`Dragons::memory()` at 3 days, resident, one lair a
dragon): dragon row 479, lair row 264, lair indexes with the embodied and vacant sets 60, prey index
31, wheel 497 at 512 dragons falling to 213 at 32,768 (three events booked a dragon). Paged, the
two rows leave memory and the rest stays (304 bytes a dragon at 32,768 by these parts; the paged
engine measured 369). The meta a save
writes is 63 to 65 bytes a dragon (2.1 MB at 32,768): the wheel and the lair indexes.

**Before and after.**

| Run | Points | Rows at peak | Engine heap | Heap |
|---|---|---|---|---|
| Before this branch (`Arena`, fixed 256-village grid, resident; side by side with the after run below, three runs each interleaved) | 8, 128, 512 | | 43, 175, 647 KB | 4.6, 7.6, 12.3 MiB |
| After, same sweep | 8, 128, 512 | | 46, 216, 711 KB | 3.7, 5.7, 10.4 MiB |
| This branch before PM-D5 and PM-D15 (grown world) | 512, 4,096, 32,768 | paged 270 KB, 322 KB, 1.87 MB | resident 1.54, 12.5, 100.3 MB; paged 0.69, 3.92, 29.9 MB | resident 13.1, 113.5, 938 MiB |
| After (grown world, the table above) | 512, 4,096, 32,768 | paged 271 KB, 322 KB, 695 KB | resident 0.71, 4.68, 34.5 MB; paged 0.60, 2.10, 12.1 MB | resident 10.4, 90.3, 754 MiB |

On the fixed grid the dragons' CPU a dragon-day is the same before and after (medians of three
interleaved runs: 22.6, 16.5, 19.2 us before; 21.6, 16.9, 19.7 after) and so are the events
(16, 245, 1,402). The engine estimate is 10% larger resident (124 bytes a dragon at 512, of which a
row page's header and its index entry are about 90); the smaller heap after is the phase 1 world's changes
merged from `feat/paging-modules`, not the dragons. CPU on this loaded machine reads about twice
the 10 us of the quiet run in `scale-2026-10-20-1224-dragons.json`, so only side-by-side figures
compare.

**Not flat, and why.**
- **Saves grow with the dragons.** The meta is rewritten whole by every save (the wheel and the
  lair indexes, 65 bytes a dragon), and the extra saves the budget asks for grow with the dragons
  too: 50 saves a day at 32,768, about 97 MB written and 1.36 s of CPU a sim day, against 2.35 s
  of the dragons' own. At a few hundred dragons (a real game) it is 4 saves a day of under 40 KB.
  Fix, not built: the wheel and `lair_cells` on pages of their own, so the meta stops growing.
- **The wheel is the largest resident term** (213 to 497 bytes a dragon). It is SCALE.md §8's
  "imminent events" working set; paging it is a kernel change for every module.
- **Paged CPU is 17 to 37% over resident** at these points (page loads and decodes, and the budget
  every 20 ticks), from one loaded run; worth a quiet pair before anyone tunes it.
- **Whole passes stay for tools** (PM-D11), and rtsim-jni does not page the dragons yet (it saves
  the version 2 blob).

## Conflict

Branch `feat/paging-conflict`. The conflict's row tables page; the meta page holds the rest.
Tests: `crates/rtsim-conflict/tests/paged.rs` (`many_factions_paged_equal_the_resident_run`,
`a_paged_save_reopened_in_the_middle_goes_on_as_the_run_that_kept_going`,
`the_walkthrough_paged_equals_the_resident_one_and_restores_from_pages`) and the unit tests in
`tables.rs`. Bench: `rtsim-cli bench scale --only factions-paged`.

| # | Choice | Why |
|---|---|---|
| PM-C1 | **Three row tables** (`crates/rtsim-conflict/src/tables.rs`): factions on `PagedRows` keyed by faction index (0x4310), sources keyed by a source id handed out in order and never reused (0x4312, 32 a page), and a place index from 16 x 16-chunk regions to the ids of the sources with chunks there (0x4313, 4 x 4 regions a page, `tables::region_key`). The meta page (`save_paged(batch, meta_key)`; the bench and tests use 0x4300 id 0, rtsim-jni's conflict key) holds everything else and each table's count. | The faction rows (about 450 bytes encoded with two feuds) and the sources are what grow with the world. |
| PM-C2 | **Feuds fold into their faction's row** (`FactionRow::feuds`, sorted by actor; table 0x4311 stays reserved, unused). The engine keeps only a feud count. | Every feud event reads and changes its faction anyway; a separate table would cost a second page per feud event. Two feuds a faction in the bench. |
| PM-C3 | **Each faction row keeps its reach**: the ids of the sources, by signal kind, that affect it (`FactionRow::reach`), replacing the resident `source_index`. `refresh_rates` walks the faction's own reach. | The old index was resident and grew with factions times sources. |
| PM-C4 | **Resident indexes**: factions by key (`by_key`, 26 bytes a faction), by cell (`cells`, 34), and sources by (key, kind) to id and rate (64 a source). Measured B-tree costs per entry, counting allocator, 100,000 entries. | Lookups by key from Java, and the cell queries of signals, plunder targets and routes, must not load pages. The rate is resident so rtsim-jni's district feed (`sources.rate`) reads no page. About 125 bytes a faction with its one source. |
| PM-C5 | **The place index replaces `source_chunks` and the scan of every source in `add_faction`**: a new faction finds the sources near it through the regions its reach covers, and `reattribute(chunk)` reads one region row. `set_emission` diffs a source's reach and places, so a changed rate touches only the factions and regions that changed. | `add_faction` walked every source, and the chunk map was resident per source chunk. |
| PM-C6 | **The node index grows incrementally**, and `rescue_news` reads its cells around the camp. | A node joining rebuilt the whole index, and `rescue_news` walked every node. |
| PM-C7 | **Budget split**: factions half, sources a quarter, places the rest. PLACEHOLDER shares. | Factions are the working set. |
| PM-C8 | **Step boundaries**: `run_until` enforces the budget after each event when paged; `session_start` (a TERRITORY for every faction, once a session) after each page of factions. | One step may touch many pages. |
| PM-C9 | **A budget check that found nothing to evict skips until something changes**: after a check ends over budget with every page dirty, the next checks return at once until a load or a save. | Otherwise each event walked every resident dirty page (19,746 futile checks in one 12-day run). |
| PM-C10 | **The host saves when the rows are over budget with every page dirty** (`Conflict::needs_save`), as herds PM7. The bench, the paged tests and the walkthrough do; rtsim-jni does not page modules yet. | Dirty pages cannot leave before a save, and conflict events land on factions all over the table: without it the resident peak between autosaves was every faction page (858 KB at 1,024 factions, 3.04 MB at 4,096: slope 0.91). With it, 314 KB at both. |
| PM-C11 | **Formats**: the blob's magic is `RTC2` (the resident part, then the three tables by `encode_all` in key order) and rtsim-jni's conflict page VERSION goes from 1 to 2; the meta page is `RTCM` version 1. No migration (no compatibility work before a release). | `PagedRows`' blob is not the old maps'. |
| PM-C12 | **4 factions a page** (PLACEHOLDER; was 16). | An event pins its faction's whole page until a save, so smaller pages pin fewer bytes per event: paired runs at 256 KiB, 16 against 4 a page, 1,024 factions: step CPU 75.4 against 52.3 us per faction-day, saves a day 44 against 13, save CPU 125 against 37 ms a day; 4,096 factions: 82.4 against 57.3 us, 162 against 47 saves, 801 against 276 ms. Same state hash. A load costs about 5 us of store read (two syscalls, CRC, LZ4) plus about 0.45 us of decode per faction. |
| PM-C13 | **What stays resident by design**, at 4,096 factions with every page evicted (`Conflict::memory`): parties in flight 441 bytes a faction (0.43 parties a faction, about 1 KB each), the wheel 244 (2.6 pending events a faction), the indexes 124 (PM-C4), the node index 30, the row tables' page indexes 15. The allocator's measure of the paged engine grows 580 bytes a faction (1.39 MB at 1,024, 3.17 MB at 4,096). | The task let parties stay resident; the wheel is SCALE.md §8's events-booked term. |

**Measured** (quick sweeps, release, nice 10, load 33 to 49; CPU is thread CPU of the conflict
step with the world stepped apart; saves reported apart; budget 256 KiB, autosave every 6,000
ticks plus PM-C10's saves; 12 days after 2 of warm-up; the same state hash paged and resident at
every point).

Before (branch point, resident): the engine's estimate 0.04, 0.62 and 2.5 MiB at 16, 256 and
1,024 factions; 23.6 us per faction-day at 1,024 (paired with the after build's 25.3: the place
and node indexes cost 7% resident).

| Factions | resident: engine estimate | resident: engine live | resident: us per faction-day | paged: rows resident after a save | paged: rows resident at peak | paged: engine estimate | paged: engine live | paged: us per faction-day | paged: loads a day | paged: saves a day (for the budget) | paged: bytes a save | paged: save CPU ms a day |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 16 | 0.06 MB | 0.08 MB | 19.5 | | | | | | | | | |
| 256 | 0.78 MB | 0.77 MB | 29.9 | | | | | | | | | |
| 1,024 | 2.91 MB | 2.87 MB | 29.5 | 257,776 B | 313,568 B | 1.44 MB | 1.39 MB | 46.2 | 1,431 | 13.1 (9.1) | 133 KB | 34 |
| 4,096 | | | | 257,496 B | 313,192 B | 3.37 MB | 3.17 MB | 53.1 | 6,790 | 46.9 (42.9) | 400 KB | 224 |

Slopes: paged rows resident at peak -0.001 (declared 0.1), events per faction flat, paged CPU per
faction-day 0.10 (declared 0.2, as the resident sweep's). The resident sweep's process heap slope
is 1.12 against its 1.1 contract, as before this work: it is the phase 1 world under the camps
(see "Measured first"), not the conflict engine. The page store's own index was 285 KB at 1,024
and 618 KB at 4,096.

**Not flat, and why.** Two things still grow with the factions in a paged run:

1. **Saves a day.** Events scale with factions (about one a faction-day) and each pins a page, so
   under a fixed budget PM-C10's saves grow with the factions: 13 a day at 1,024, 47 at 4,096.
2. **The meta page, written by every save**, holds the wheel and the PM-C4 indexes: 115 bytes a
   faction raw (474 KB at 4,096), about 60 compressed (246 KB written by a save with nothing
   dirty). With (1), the bytes saved a day grow with the square of the factions. Fixing it means
   paging the indexes (they change only when factions or sources come and go) and giving the
   wheel a paged form (entries bucketed by due time), a kernel change shared with herds, whose
   meta holds its wheel too. Not done here.

Other open items: `applied_ops` (the op set) grows with every applied op and is never pruned (as
before this work); `session_start` is one pass over every faction a session, page by page;
rows per page and the budget shares are placeholders.

## Ships (branch `feat/paging-ships`)

| # | Choice | Why |
|---|---|---|
| PM-S1 | **Ship rows are a `PagedArena<Ship>`** (table 0x5310, 16 rows a page). Handles keep generation 1 and their index, so agent ids and every stored `ShipId` are unchanged. | A row is about 260 bytes resident (248 inline plus its small maps), so a page is about 4 KB, the size of the other modules' pages. A service's ships are consecutive keys (PM-S3), so a watched timetable loads one or two pages. |
| PM-S2 | **Lane threats are paged rows keyed by lane** (0x5311, 64 a page). Their two small sets (`hunters`, `escorts`) are sorted Vecs instead of a `BTreeSet` and a `BTreeMap`. | They grow with the lanes: a lane gets a row the first time it is threatened, and the row is never removed. A `BTreeMap` with one entry allocates a node for eleven (about 100 to 200 bytes per non-empty set); a Vec holds 8 or 16. |
| PM-S3 | **`service_ships` is a key range per service** (`ShipRange { first, n }`): 8 bytes per service and nothing per ship, instead of a Vec of ids. | `man_service` inserts a service's ships one after another, and handles are never reused, so the keys are consecutive. This is debug-asserted when ships are manned, and `check()` and every load check the ranges. |
| PM-S4 | **The embodied index is kept and saved** (`embodied: BTreeSet<ShipId>`, maintained by `set_embodied` everywhere the flag changes, written in the meta page). `session_start` walks the index instead of every ship. | A whole-table pass at every session start would load every ship page. Rebuilding the index on open would be that same pass, so it is saved instead. Its size is bounded by what players embody. |
| PM-S5 | **Hunted lanes come from the havens' active pirates** (`hunted_lanes()`, the pirates in Hunting), not from a pass over the threat rows. `lane_threats()` (every row) stays as a tool for `sim ships` and the tests. | The scripted player and the tests asked which lanes are hunted by walking every lane threat. |
| PM-S6 | **The port network has its own page** (0x5312 id 0: docks, lanes, services and their key ranges, the lints, berth overflows). Only the save after the network changes writes it. `add_port`, `build`, `connect` and `add_navy` mark it, and a failed save that wrote the page marks it again. The meta page holds the rest of the resident state. | The network stays resident (the flow graph, SCALE.md §8) and grows with the world, at about 1.2 KB per port. Writing it in every autosave would make every save grow with the world. |
| PM-S7 | **The budget is split 7/8 to ship rows and 1/8 to lane threats.** This is a PLACEHOLDER share. | Ships are almost all of the row bytes. A threat row is under 100 bytes, and only the lanes near a player or a hunt are touched. |
| PM-S8 | **`enforce_budget` runs at the end of every `run_until`.** `needs_save()` reports when the rows are over budget and every resident page has changed since the last save, as herds do (PM7). The bench then saves; rtsim-jni does not page modules yet. | Dirty pages cannot leave memory before a save. Without the extra save, the resident peak grew with whatever a player dirtied between two autosaves. |
| PM-S9 | **The one-blob `save()` stays** (rows by `encode_all` in key order), and its format is now **RTS2**. rtsim-jni's ships page VERSION is now 2. `state_hash` is still `fnv1a64(save())`, the same paged or not. | The brief asked for this. There is no backward compatibility before release, so old saves are not read. |
| PM-S10 | **`check()` also checks the indexes** (`check_indexes`): embodied against the rows, each ship inside its service's range, and each hunting pirate against its lane's hunters, both ways. It is a whole pass, run only by tests and tools. | The indexes replace whole-table passes, so they must be proven to agree with the rows. |
| PM-S11 | **`sim ships`'s scripted player moved to `synth::Script`**, so the paged tests run the same script. `sim ships` output is unchanged except the restart hash (the format) and wall time. | The tests can then compare paged and unpaged runs on the scenario the sim already checks. |
| PM-S12 | **`set_page_layout(ships_per_page, threats_per_page)`** works only before paging starts, and the layout is saved in the meta page. Tests use 4 and 4. | With tiny pages a small test world pages hard (thousands of loads and evictions in 12 days). |
| PM-S13 | **The caller picks the meta page's key** (`save_paged(batch, meta_key)`, `open_paged(source, budget, meta_key, tun)`). The key must not be in 0x5310 to 0x5312. | This is the brief's interface. rtsim-jni will most likely use 0x5300 id 0, the module's page today. Herds keep their meta page at a fixed key (0x4813), so one convention for every module is the lead's call when wiring. |
| PM-S14 | **The per-ship `BTreeMap`s (cargo, quotes, a pirate's loot) are left as they are**, and so is the inline `Pirate`/`Promoted`. | Rows page, so this heap is resident only while the row's page is. Boxing `Pirate` and `Promoted` would make rows about half the size (fewer pages, smaller blob); it is optional and not done. |
| PM-S15 | **The bench** (`ships-grid` resident, `ships-paged` paged) grows the ocean grid from 49 to 6,400 ports at 16 ships a service (372 to 51,774 ships, 139 times). One player tours the 7 x 7 docks around the haven, one dock an hour, and the fake plugin answers. The paged sweep uses a 64 KiB budget, autosaves every 6,000 ticks and adds budget saves. `ShipsConfig::roam` sends the player to every dock instead (one-off runs). | Each size gets the same play, so a slope measures the world rather than how much of it the player has seen. A roaming player's costs grow until it stops coming back within a save interval, then stay flat (measured below). |

**What pages and what stays resident.** Paged: ship rows (0x5310) and lane threats (0x5311).
Resident, in the meta page that every save writes (2 to 6 KB in the bench): the wheel, havens,
navies, players, booked voyages, applied ops, `seen_by`, the embodied index, the fight
observations, the ledger, the stats and the outbox. Every one of these is bounded by activity
(players, pirates, what is watched), not by the ship count. Also resident, but in the network page
(0x5312) that only a change rewrites: the network itself. At the top grid point (6,400 ports,
12,720 lanes, 3,462 services) the network costs 7.74 MB by the module's estimate (6.97 MB measured
by the allocator on a copy):

| Part | Bytes | Per item |
|---|---|---|
| docks | 505 KB | 79 B per dock |
| lanes | 4.92 MB | 387 B per lane |
| services (with their key ranges) | 2.32 MB | 671 B per service |
| all | 7.74 MB | 1.21 KB per port, 609 B per lane |

The page store's own index is also resident and grows with the pages: 259 KB at the top point,
for about 3,450 pages (75 B per page).

**Measured.** The before numbers come from the table at the top of this file. The `ships` sweep
at commit 1c43097 had per service 1, 4 and 16 ships on 100 ports. Its live heap was 3.4, 3.0 and
3.8 MiB, and the engine's own estimate was 0.13, 0.18 and 0.38 MiB: about 0.25 KB per ship, all
of it resident. The same quick sweep at 659b927 (resident, since rtsim-jni does not page yet)
gives a live heap of 2.4, 2.2 and 2.4 MiB and an engine estimate of 0.15, 0.19 and 0.33 MiB:
still 259 B per ship. The heap is lower because of the wheel's freed burst vectors (PM8), and CPU
is flat (slope 0.013).

The grid sweeps (`bench scale --only ships-grid,ships-paged`, full, at 659b927) ran in release,
nice 10, at a load average of 27 to 40 from other sessions. Each point is the median of 3 runs
of 10 sim days. CPU is thread CPU time: the ships engine's own calls, with the world, the fake
plugin and the saves stepped apart (save CPU is its own column).

| Ports | Ships | Lanes | Live heap: resident, paged | Engine: resident, paged | Network | Rows resident: end, peak | Loads a day | Saves a day (for the budget) | Bytes a save | CPU ms a sim day: resident, paged | Save CPU ms a day | Full blob |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 49 | 372 | 92 | 5.9, 5.9 MiB | 0.54, 0.50 MiB | 0.06 MiB | 55 KB, 82 KB | 130 | 7.7 (3.7) | 12.5 KB | 4.3, 8.5 | 9.0 | 25 KB |
| 196 | 1,584 | 377 | 8.3, 8.0 MiB | 0.96, 0.61 MiB | 0.22 MiB | 56 KB, 89 KB | 192 | 8.4 (4.4) | 13.8 KB | 5.0, 8.2 | 12.4 | 88 KB |
| 784 | 6,200 | 1,538 | 19.8, 18.4 MiB | 2.82, 1.33 MiB | 0.90 MiB | 56 KB, 87 KB | 181 | 8.0 (4.0) | 12.9 KB | 6.4, 11.5 | 13.3 | 345 KB |
| 3,136 | 25,746 | 6,221 | 65.1, 58.9 MiB | 10.44, 4.09 MiB | 3.66 MiB | 56 KB, 90 KB | 204 | 9.1 (5.1) | 14.7 KB | 9.1, 11.9 | 13.5 | 1.43 MB |
| 6,400 | 51,774 | 12,720 | 129.4, 116.9 MiB | 20.64, 7.83 MiB | 7.39 MiB | 56 KB, 90 KB | 193 | 8.5 (4.5) | 13.9 KB | 8.5, 11.2 | 13.4 | 2.92 MB |

| Slopes | Measured | Declared |
|---|---|---|
| paged: row pages resident at peak (`Metric::Resident`) | 0.015 (max/min 1.10) | 0.1 |
| paged: bytes a save | 0.023 (max/min 1.17) | 0.15, from 0.35 after this run |
| paged: CPU a sim day | 0.077 | 0.2 |
| resident: CPU a sim day | 0.160 | 0.2 |
| live heap, both | 0.65, 0.63 | 1.1 |
| resident: full blob | 0.98 | 1.1 |

Under a 64 KiB budget the row pages stay at 56 KB at the end of each run and peak at about
90 KB, whether there are 372 ships or 51,774. The peak is the budget plus what one step loads or
changes before the step's `enforce_budget` or its budget save. At the top point the engine falls
from 20.6 to 7.8 MiB, and the network is 7.4 MiB of what remains. A paged save writes about
14 KB. Before, the one-blob save wrote 2.9 MB, which grows with the ships. The live heap is the
phase 1 world under the grid (17 KB a village) plus the network, both resident by design. Paged
CPU is 3 to 4 ms a sim day above resident: about 190 page loads a day, each a read and a decode
of 16 rows.

**A roaming player** (`roam: true`, one run of 3 days each, paged) gets a fresh dock most hours.
At 1,584, 6,200 and 51,774 ships: the peak is 88.7, 88.5 and 88.5 KB; loads are 228, 236 and
258 a day; there are 4.7, 4.0 and 6.0 budget saves a day; bytes a save are 16, 31 and 17 KB; CPU
is 10.5, 12.2 and 15.7 ms a sim day. These costs are as flat as the fixed tour's once the grid is
bigger than what the player covers between two saves. The bytes a save go up and down because the
store compacts the first full save's segment when half of it is stale: 991 pages were compacted
in the 6,200-ship window and 680 in the 51,774-ship one. An earlier run with this roaming tour and
no budget saves had peaks of 57 to 136 KB, growing with the grid (resident slope 0.175). That is
why PM-S8 adds budget saves.

**Not done.** rtsim-jni does not page ships yet (the lead wires every module at once). Ships do
not prefetch pages: a row loads on demand on the step thread. Rows are not shrunk by boxing
`Pirate` and `Promoted` (PM-S14).

## Phase 1 world: the market rows (moved, not bounded yet)

Bounded since 2026-10-21 by staggered wakes: docs/choices-paging-followups.md PF1 to PF4.

| # | Choice | Why |
|---|---|---|
| PM10 | **`lazy::PagedLazyTable`**: a `LazyTable` whose regions live in a `PagedArena` (the same calls, handles in insertion order and never reused, the same wakes in the same order); its wake wheel stays resident. A `ModelCodec` says how rows are written, because a model's own `Encode` may be a hashing form (the market's is). | The markets are lazy regions; the kernel's `PagedTable` pages lazy regions too but takes the store in every call, and the world reaches its markets from twenty places. This keeps every call site. |
| PM11 | **The world's markets are a `PagedLazyTable`** (table 0x5010, 16 markets a page; meta page 0x5012 id 0). `World::save` writes the market rows last, in key order (snapshot magic RTW2); `save_paged`, `open_paged`, `saved`, `save_failed`, `enforce_budget` and `needs_save` as the herds'. rtsim-jni's phase 1 meta page goes to version 3, so older pages are dropped (S5). | Market rows were the largest single per-village term in the market sweep. |

**Tested:** `a_paged_world_runs_and_restores_exactly` (rtsim-world `tests/persist.rs`): a world
paging its markets under a 12 KB budget, saved after every op of a random script, gives the
resident world's state hash after every op and its exact snapshot at the end, and a world opened
from its pages at any point continues exactly. Resident (no store), the world's state hash is
unchanged from before (A/B at 1,024 markets against the binary at 1c43097: same hash, heap 12.5
against 17.0 MiB, the wheel's freed vectors (PM8); CPU per day +5 to 8%, the row table's index
lookups).

**Measured, and why it is not done** (one pair side by side at 4,096 markets, 4 MiB budget, 3
days after 10): heap 42.7 MiB paged against 49.1 resident, the same state hash; but the resident
market rows peaked at 15.8 MB, four times the budget, CPU per day rose from 633 to 982 ms (4,310
page loads a day) and saves took 2.5 s of CPU a day (24 saves, 20 of them for the budget, 4.8 MB
each). Two reasons, both in the model, not the pages:

1. **Every market is hot.** About five events per market per day, so between two autosaves every
   market page changes, and a changed page cannot leave before a save.
2. **One tick can change every market.** Market breakpoints fall on trade-quantum boundaries
   (`Market::next_breakpoint` returns `quantum.at(..)`), so many markets wake in the same tick,
   and no save can come between them.

The `markets-paged` sweep declares this honestly (resident slope 1.2, "not bounded yet"; the quick
run after the reboot measured 1.107 from 256 to 4,096 markets: every row, plus the pages' own
overhead). What it needs, in order: market wakes staggered by page within a quantum, as the herds'
decisions are (PM6; a behaviour change, so Ben's call); then the other three quarters of a
village's heap, which stay resident today: news rings (2.3 KB a village), pipelines (2.8 KB, the
flow graph, resident by SCALE.md §8), the world wheel (2.3 KB) and knowledge holders (superlinear
in the conflict world, 38.5 MiB of 104.5 at 2,048 nodes). Knowledge holders (planned table 0x5011)
are the next module to page.

## Industry: not moved (the glass box has not merged)

`git log origin/main` on 2026-10-20 has no merge of `feat/glassbox-g1`, so rtsim-industry is
left alone (task rule). What it will need, from its `Engine` today:

- **Districts** (`districts: BTreeMap<DistrictId, District>`): a `PagedRows` keyed by the
  district id, with `cluster_of` folded into the row (as the herds' regions, PM4).
- **Clusters** (`clusters: LazyTable<Cluster>`): a `PagedLazyTable<Cluster, ClusterCodec>`
  (PM10), the same change the markets took; wakes staggered by page if clusters share quantum
  boundaries as markets do.
- **Stations and lines** (`stations`, `lines`): the rail flow graph, resident (SCALE.md §8).
- **Per train** (`last_departure`, `arrivals`, `train_cooldown`): resident while a train runs;
  rows keyed by train id if train counts grow into the hundreds of thousands.
- **Wakes and pressure** (`wakes`, `pressure`): resident, as every module's wheel.
- The glass box's compiled machine graphs (G1) are per district: they belong in the district
  row, or in their own row table keyed by district if they are large.

## Not done here

- **rtsim-jni does not page any module yet.** Each module still saves its `save()` blob as one
  page (herds 0x4800 version 2, dragons 0x4400 version 2, conflict 0x4300 version 2, ships
  0x5300 version 2, phase 1 meta version 3). Wiring paging
  in means: the engine's store shared as the modules' `PageSource` (it is a
  `Mutex<Option<PageStore>>`, which implements it), the save path calling each module's
  `save_paged` into the save's batch, the IO lane handing the tickets back once the commit is
  durable (`saved`, or `save_failed`), `enforce_budget` after each step, an extra pending save
  when any module `needs_save` (PM7), and `open_paged` on load. The per-module deletion lists
  (`deletions`) must keep away from the row tables (they do: different table ids).
- **One convention for meta pages.** Herds and the phase 1 world keep theirs at a fixed key of
  their own (0x4813 and 0x5012, id 0); dragons, conflict and ships take the key from the caller
  (`save_paged(batch, meta_key)`). Pick one when wiring rtsim-jni.
- **The meta page grows with the entities in every module** (the wheel, about one pending event
  per entity, and the position indexes), and every save writes it whole: with the budget's extra
  saves, bytes written a day grow with the square of the entity count (dragons: about 97 MB a sim
  day at 32,768). The fix is a paged wheel (pending events in pages by time bucket) and paged
  position indexes: a kernel change shared by every module.
- **Prefetch for row tables** (rows load on demand on the step thread).
