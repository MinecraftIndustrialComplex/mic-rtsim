# Choices: the kernel review bugs, and the modules on paging (PM)

Reversible choices made on 2026-10-20 by the paging-modules agent (branch `feat/paging-modules`),
for Ben's review (D10). Each one can be undone.

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

## Phase 1 world: the market rows (moved, not bounded yet)

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

The `markets-paged` sweep declares this honestly (resident slope 1.1, "not bounded yet"). What
it needs, in order: market wakes staggered by page within a quantum, as the herds' decisions are
(PM6; a behaviour change, so Ben's call); then the other three quarters of a village's heap,
which stay resident today: news rings (2.3 KB a village), pipelines (2.8 KB, the flow graph,
resident by SCALE.md §8), the world wheel (2.3 KB) and knowledge holders (superlinear in the
conflict world, 38.5 MiB of 104.5 at 2,048 nodes). Knowledge holders (planned table 0x5011) are
the next module to page.

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
  page (herds 0x4800 version 2, dragons 0x4400 version 2, phase 1 meta version 3). Wiring paging
  in means: the engine's store shared as the modules' `PageSource` (it is a
  `Mutex<Option<PageStore>>`, which implements it), the save path calling each module's
  `save_paged` into the save's batch, the IO lane handing the tickets back once the commit is
  durable (`saved`, or `save_failed`), `enforce_budget` after each step, an extra pending save
  when any module `needs_save` (PM7), and `open_paged` on load. The per-module deletion lists
  (`deletions`) must keep away from the row tables (they do: different table ids).
- **Prefetch for row tables** (rows load on demand on the step thread).
