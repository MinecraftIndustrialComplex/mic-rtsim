# Choices: the paging follow-ups (PF)

Reversible choices made on 2026-10-21 by the paging follow-ups agent (branch
`feat/paging-followups`), for Ben's review (D10). Each one can be undone. The work follows
`docs/choices-paging-modules.md` ("Not done here"): markets held to their budget, a paged wheel
and paged indexes in the kernel, and paging wired into rtsim-jni.

## Markets held to their budget

Before: every market's breakpoints fall on trade-quantum boundaries (`Market::next_breakpoint`
returns `quantum.at(..)`), so one tick wakes every market, and a market page changed since the
last save cannot leave. At 4,096 markets under a 4 MiB budget the resident market rows peaked at
15.8 MB. Ben's decision (DECISIONS.md PF-M, "Stagger by page", 2026-10-21): stagger market
wakes by page, keeping each market's trade cadence and shifting only its phase.

| # | Choice | Why |
|---|---|---|
| PF1 | **Market wakes are staggered by page over the trade quantum.** `PagedLazyTable::staggered(period)` gives every region a phase, `stagger_phase(page, period)`: the page number times the golden ratio's 64-bit Weyl constant, scaled to `[0, period)`. A breakpoint at `bp` wakes the region at `bp + phase`. The world staggers its markets over one quantum (1,000 ticks). What the market decides is unchanged: its settle depends only on the boundaries crossed (`market_event` takes `nb = index(at) + 1`, so any tick in the quantum gives the same decision). The world's own events that land on a quantum boundary and target a market are shifted by the same phase: a pipeline's `Wake`, road and band `Deliver`, the caravans' and lanes' re-read wakes. `Arrive`, `Grow` and `BandPlays` are not (they are not on boundaries). A breakpoint is "held" from `bp` until its phased wake; any other event that reaches a held market first delivers the breakpoint (`World::release`, `PagedLazyTable::take_held`), so the market never sees an event past a breakpoint it has not crossed, and the wake that fires later is stale. The phase is saved in the table's meta page. Tests: `lazy_stagger.rs` (kernel: the wakes of one boundary spread a page at a time over the period; a held breakpoint is delivered first and only once; the stagger survives a save) and `a_paged_grid_stepped_tick_by_tick_stays_within_its_budget` (rtsim-world `tests/persist.rs`: 128 markets under a quarter of their rows, stepped tick by tick for a day with autosaves and pressure saves; it failed before the change, 289 KB at peak against a 94 KB budget). | A phase per page is the herds' way (PM6): a tick then changes the markets of one page or two, and the next save frees them. A Weyl sequence spreads any number of pages evenly over the period without a table. Phasing the boundary events with their market keeps each market's own order of events as it was (a wake and a delivery for the same boundary still meet the market after its breakpoint). Every economy test still holds: the closed scenario and the overshoot worlds still go quiet (`tests/quiet.rs`), determinism and restore give identical hashes, and every paged world matches its resident twin. |
| PF2 | **The budget test turns off the conservation check after every event** (`check_every_event`, on by default in debug builds). | The check reads every market, a whole-table pass that loads every page; it would measure the check, not the world. The test checks conservation once at the end instead. |
| PF3 | **One market per page** (`MARKETS_PER_PAGE`, was 16). PLACEHOLDER. | A market row is 1 to 7 KB resident (its goods), about a page already. With 16 a page, any of 16 markets touched dirtied the page: at 4,096 markets the stagger alone took the budget's pressure saves from 20 to 84 a day. Measured at 4,096 markets (quick, side by side, then cores swapped): 1, 4 and 16 markets a page all peak at 4.21 MB; saves a day 17.7, 51 and 88; save CPU 1.27 to 1.31, 3.13 to 3.15 and 5.49 s a sim day; sim CPU 1.04 to 1.09, 1.01 and 1.15 s a sim day (smaller pages load more often: 14,340 loads a day against 13,530 at 4). |
| PF4 | **The `markets-paged` sweep starts above the budget** (2,048, 4,096 and 8,192 markets; quick 2,048 and 4,096; was 256, 1,024 and 4,096) and its resident contract is flat (0.1 with the max/min bound), not 1.2. | At 256 and 1,024 markets every row fits in the 4 MiB budget, so their peak is the whole table and the slope measures the table, not the bound. The other paged sweeps put every point but the first over their budget for the same reason. |

**Measured** at 4,096 markets under a 4 MiB budget (`bench scale --quick --only markets-paged`,
release, nice 10, A and B side by side on cores 4 and 6, load average 15 to 23 from other
sessions; resident peak, saves and bytes are deterministic, CPU is noisy):

| | Before (b100817) | PF1, 16 a page | PF1 and PF3, 1 a page |
|---|---|---|---|
| Market rows resident at peak (budget 4.19 MB) | 15.79 MB | 4.21 MB | 4.21 MB |
| Sim CPU, saves apart | 1.47 s a day | 1.15 s | 1.04 to 1.09 s |
| Saves a day (of them for the budget) | 24 (20) | 88 (84) | 17.7 (13.7) |
| Bytes a save | 4.78 MB | 4.41 MB | 4.85 MB |
| Save CPU | 2.72 s a day | 5.49 s | 1.27 to 1.31 s |
| Heap at peak | 96.8 MB | 84.9 MB | 85.3 MB |
| 256 markets (under the budget) | 51 ms a day, 4 saves | 51 ms, 4 saves | 19 to 22 ms, 4 saves |

What a save still writes is mostly the world's meta page: nodes, edges, pipelines, packets,
bands and the world wheel, all resident and written whole by every save (see the paged wheel
below and "Not done here").

## The paged wheel and paged indexes

Before: every paged module's save wrote its whole meta page, and the meta page held the module's
timing wheel and its position indexes, all resident. So a save cost grew with the entity count in
every module even when nothing changed: measured per save, the dragons' meta page was 32.6 KB at
512 dragons and 259 KB at 4,096 (the wheel 167 KB, the lair index 49 KB, the road, lair and
village cell indexes 42 KB), the factions' 115 KB at 1,024 and 464 KB at 4,096 (the parties 205
KB, the wheel 115 KB, the source keys 78 KB, the faction keys and cells 65 KB), the herds' 6.4 KB
at 256 and 25.6 KB at 1,024 (the claims and the wheel). The tests first: each module's
`a_save_with_nothing_changed_is_flat_in_the_*` saves a world with nothing changed at one size and
at four times it, and asks that the second save write at most half as much again plus 1 KB. All
three failed (the bytes grew four times).

| # | Choice | Why |
|---|---|---|
| PF5 | **The wheel is saved as a log** (`rtsim_kernel::meta_pages::PagedWheel`): it stays a resident `TimingWheel` with the same pop order, every entry gets a sequence number when scheduled, and a save appends the entries scheduled since the last save to an open tail page. A full tail seals into pages of `WHEEL_PAGE_ENTRIES` (PLACEHOLDER 1,024). The key of the last entry popped, (due tick, sequence), is a watermark saved in the header: an entry is pending exactly when its key is above it, so a sealed page whose largest key is at or below it is deleted, and reopening keeps only the entries above it. Once the popped entries left in live pages outweigh the pending ones plus `WHEEL_SLACK` (PLACEHOLDER 4,096), the save writes the pending entries into fresh pages and deletes the old ones. A failed save rewrites every pending entry next time and deletes every page it knew. | Pops come in (time, sequence) order, so one watermark says which entries are gone without writing anything per pop. A save then costs the entries scheduled since the last one, plus a rewrite amortized over at least as many pops as it writes. The rewrite on failure needs no record of what the failed batch held. The slack keeps a small wheel from being rewritten every few saves. |
| PF6 | **An index is a map in key-range pages** (`PagedMap`): resident as a `BTreeMap`, a change marks the page its key falls in, a save writes the marked pages, a page grown past twice `MAP_PAGE_ENTRIES` (PLACEHOLDER 512) splits, and a page left empty is deleted (its range joins the page before it). Small pages are not merged. A failed save marks every page dirty and deletes again what it would have deleted. | Range pages keep neighbours together (a lair's cells, a faction's places), so a change touches one page. Not merging keeps a page's range fixed between saves; an empty page goes, so the page count follows the live keys. Marking everything on a failure is what the row tables do (PM5). |
| PF7 | **The factions' parties are a `ResidentArena`**: an `Arena` saved in pages of `ARENA_PAGE_SLOTS` (PLACEHOLDER 64) slots, a change marking its slot's page, and the free list saved as a stack of pages (from page id 2^40) of which a save writes those from the lowest depth reached since the last save. They stay resident, not moved to `rows::PagedArena`. | The parties are handles the whole engine holds; a resident arena keeps the handles, the generation checks and the memory budget as they were and costs only the save. Moving them to rows would make them evictable, a larger change the bench does not ask for (parties are about 50 bytes each). |
| PF8 | **Herds: tables 0x4814 (wheel) and 0x4815 (claims), meta version 3.** The meta page's wheel and claims are written as page headers in a paged save and as blobs in a resident save (`save`, used by the resident tests and the JNI's blob path). | The module's tables sit at its base plus 0x10 and up, clear of the JNI's own keys (0x4800), so the JNI's `deletions` never touches them. |
| PF9 | **Dragons: tables 0x4412 (wheel), 0x4413 (lairs by key), 0x4414 (lair cells), 0x4415 (village cells), 0x4416 (road cells); magic `RTD2` and meta magic `RTN2`.** `PreyIndex::reset` clears the village and road indexes when the topologies change (`set_topologies`), so their pages are dropped by the next save rather than kept as stale cells. | As PF8. The magic changes because the meta layout did (no backward compatibility before release). |
| PF10 | **Conflict: tables 0x4314 (wheel), 0x4315 (parties), 0x4316 (faction keys), 0x4317 (faction cells), 0x4318 (source keys); magic `RTC3`, meta version 2.** The structures (a few per faction, written whole) stay a blob in the meta page. | As PF8. The rest of the meta page, the structures with it, was about 1 KB of the 464 KB at 4,096 factions. |
| PF11 | **The kernel's own tests use small constants** (`cfg!(test)`: 64 entries a wheel page, slack 128, 32 a map page, 8 slots an arena page). Property tests (`a_reopened_wheel_pops_as_a_resident_one`, `a_reopened_map_is_the_saved_one`, `a_reopened_arena_is_the_saved_one`) run random histories of schedules, pops, saves, failed saves and reopens against the resident structure. | With the real sizes a short history never seals a page, splits or rewrites. The modules' tests run the real sizes. |

**Measured** by the three tests (raw bytes a save writes when nothing changed since the last one,
deterministic):

| | Before | After |
|---|---|---|
| Herds, 64 herds, then 256 | 2,252 B, then 9,024 B | 103 B, then 108 B |
| Dragons, 96 dragons in 48 lairs, then 384 in 192 | 5,472 B, then 22,171 B | 245 B, then 267 B |
| Factions, 64, then 256 | 9,047 B, then 33,252 B | 1,096 B, then 1,125 B |

**Measured** by the scale sweeps (`bench scale --quick`, two pairs side by side, the second with
the cores swapped; bench/results/README.md, 2026-10-21 08:24 and 09:02 UTC). Bytes a save, and
save CPU a sim day from the swapped pair, before and after:

| | Bytes a save | Save CPU a day |
|---|---|---|
| Dragons, 512, 4,096, 32,768 | 37.2, 277 and 2,077 KB, then 29.1, 83.9 and 199 KB | 8.3, 21.7 and 1,096 ms, then 7.7, 12.5 and 199 ms |
| Herds, 256, 1,024, 4,096 | 32.8, 159 and 455 KB, then 29.7, 150 and 409 KB | 0.9, 5.5 and 28.7 ms, then 0.9, 4.8 and 25.3 ms |
| Factions, 1,024, 4,096 | 132 and 397 KB, then 91.7 and 200 KB | 30.9 and 215 ms, then 25.4 and 145 ms |
| Ships, 49 to 6,400 ports | 12 to 13 KB, unchanged | unchanged |

Saves a day and row pages resident at peak are the same before and after (the budget sweeps'
peaks: dragons 271 to 695 KB, herds 1.17 to 1.70 MB, factions 314 KB, against 262 KB, 1.05 MB
and 262 KB budgets; the peak is the budget plus what one step changes). Live heap rises up to 3%.
Sim CPU is within the noise. In the sweeps the herds still write about 100 B per herd a save:
a herd's row changes with nearly every event, so its page is dirty at the next save either way.

## Paging wired into rtsim-jni

Before: rtsim-jni held every module resident and wrote each one's `save()` blob as one page at
every save (herds 0x4800, dragons 0x4400, conflict 0x4300, ships 0x5300), and the phase 1 world's
whole `World::save` in snapshot pieces; while herds, dragons, ships or the conflict are in the
world, the phase 1 link snapshots at every save. So every save wrote every module and the whole
world, and nothing paged. The test first: `a_save_with_nothing_changed_is_flat_in_the_herd_count`
(rtsim-jni `tests/protocol.rs`) founds 32 herds and then 128, saves twice, and asks that the
second save at 128 write at most 4 KB more than at 32. It failed: 6,126 B and 23,750 B. Industry
is untouched (the glass box G2 work is on it).

| # | Choice | Why |
|---|---|---|
| PF12 | **One meta-page convention: the caller passes the key, and the engine passes the module's own page `(table, 0)`**: herds 0x4800, dragons 0x4400, conflict 0x4300, ships 0x5300, the phase 1 world 0x5001 (its snapshot table; the link's own meta page stays at 0x5000 id 0, its snapshot pieces move to 0x5001 id 1 and up). Herds and the world gave up their fixed keys (0x4813 and 0x5012): `Herds::save_paged(batch, meta_key)`, `Herds::open_paged(source, budget, meta_key, ..)`, `World::save_paged(batch, meta_key)`, `World::open_paged(source, budget, meta_key, ..)`, as dragons, conflict and ships already were. Every module's row and index tables stay at its table plus 0x10 and up. | The engine owns the key space and every module's tables sit next to the page the engine already had for it. Keeping a module's own pages off its row tables lets the engine's per-table deletions (PF21) never touch the module's pages. A fixed key inside a module would be a second rule for two modules out of five. |
| PF13 | **A save is built on the step thread and committed on the IO lane, and the tickets come back through a channel.** At the step boundary (`take_save`) each module's `save_paged` puts its meta page and its changed pages into the save's batch; the batch goes to the IO lane with the core's and industry's pages (serial, as before). The modules' tickets travel in a `paging::Job` that sends its outcome when the commit is made, or when the job is dropped for any reason (a failed commit, a refused job, a closed engine). Each step drains the channel at its start, before each save and before the budget check, and hands each ticket to `saved` or `save_failed`. | Building needs `&mut` on the modules, which only the step thread has; committing is IO, which must stay off it (D1). A failure reported on drop means no path can leave a ticket unanswered, so a module never waits on pages it will not get back. |
| PF14 | **A save built before the step saw an earlier save fail is not committed** (`Job::stale`: the job was built when the failure count was lower than it is at commit time). It counts in `save_failures`, its tickets come back failed, and the next save writes the pages again. | A paged module writes only what changed since its last save. If that save failed, the next batch built before the failure was drained lacks those pages: committing it would make a generation that reads back rows that never reached the disk. |
| PF15 | **Pressure saves are pending generations.** After every step the engine runs each module's `enforce_budget`; when a module (switched on, or with a save due) is still over its budget with every resident page changed since the last save (`needs_save`) and no save is in flight, it takes a save of its own at that boundary, committed with `commit_pending` (P29). It counts in `stats.pressure_saves`, not `saves_done`, and does not move the acknowledgement (`ackedObsSeq`), so Java keeps the journal behind it. `start` rolls back every pending generation right after opening the store, and the journal replays from the last save Java asked for. The next ordinary save's `commit` confirms the pending generations with it. | A save Java did not ask for must not trim the journal: the world save it pairs with is Java's. Rolling back at start keeps one rule for the restart: the store is exactly the last acknowledged save. One in flight at a time keeps a slow disk from stacking saves; meanwhile the rows stay resident over the budget. |
| PF16 | **State hashes only in TEST mode**, in a page of their own beside the meta page, `(table, 1)`, checked on load (a mismatch is logged and reported by the `*_restore_check` calls). The phase 1 meta page's hash is written as 0 outside TEST and checked only when TEST and nonzero. | A state hash hashes the module's whole save: it reads every row, so in production it would page in everything at every save, which is what paging is for avoiding. The tests that check restores run in TEST mode. |
| PF17 | **The phase 1 world pages through the store** (markets at 0x5010). A snapshot is now the world's `save_paged` (its meta page and the pages changed since the last snapshot) plus the link's own part (the village index and the censused villages) in pieces. The world's pages are written only with a snapshot, and a snapshot is also taken when the world `needs_save` (over its budget with every resident market page changed). Phase 1 meta version 4 (older pages are dropped, S5). | The tail of the input log replays on top of the snapshot, so the world on disk must be the snapshot's world: a world page written between snapshots would be ahead of the inputs replayed onto it. The world's own needs-save trigger keeps its rows within the budget between snapshots. |
| PF18 | **Dropped pages are deleted at start.** A module whose pages are dropped (unreadable, pages without a meta page, or pages without the phase 1 world they were saved with) is flagged `purge`; `start` deletes all its tables (`table..=table + 0x1f`) in one synchronous commit before anything runs. | A fresh module's row ids start again, so a fresh table would read the old rows by key as its own. Deleting them first is the only order in which that cannot happen. |
| PF19 | **`Config.row_budget`, 4 MiB of rows per paged module by default** (`DEFAULT_ROW_BUDGET`, PLACEHOLDER): herds, dragons, conflict, ships and the phase 1 world's markets, so 20 MiB of rows at most beside the resident indexes and wheels. | The markets sweep's budget (PF1 to PF4). One number per module keeps the config small until the potato box says how to split it. |
| PF20 | **A switched-on module writes its meta page at every save**, even with nothing changed (a quiet save is about 450 B in all). **Switched off, it writes once** (`save_due`, set when the switch goes off) and then keeps its pages as they are until it is on again (U10). A failed save sets `save_due` too, so the module writes again even switched off. | The meta pages are small now (PF5 to PF10): writing one every save costs a few hundred bytes and needs no record of whether it changed. |
| PF21 | **Deletions read only the module's own table** (`PageStore::table_ids` per table), not the whole store index (`keys()`). | A save's deletion pass then costs the module's own pages, not every page in the store (the row tables can hold millions). Industry's `deletions` still reads `keys()`: it was out of bounds here. |

**Measured** (rtsim-jni tests, raw bytes, deterministic):

| | Before (the link at f38a384) | After |
|---|---|---|
| A save with nothing changed, 32 herds | 6,126 B | 444 B |
| The same, 128 herds | 23,750 B | 449 B |

`herds_over_their_row_budget_page_out_and_read_back_after_a_crash` runs 128 herds under a 4 KiB
row budget for four sim days without a save asked for: the budget took 3 saves of its own, the
rows end at 1,400 B, and they peak at 96,110 B in a step where every herd decided (the budget
plus one step's changes, as in the sweeps). After a crash the start rolls the pending generations
back, the journal replays, and the herds read back at the last save's state hash; a second crash
then finds nothing pending. There is no JNI CPU bench: the sweeps measure
the modules themselves.

## Not done here

- **The phase 1 world's meta page is written whole at every snapshot** (its nodes, edges,
  pipelines, packets, bands and wheel, all resident), and while herds, dragons, ships or the
  conflict run there is a snapshot at every save. At 4,096 markets that is most of the 4.8 MB a
  save writes. `PagedWheel` and `PagedMap` (PF5, PF6) are the pieces for it. Written up with
  measurements: docs/choices-leg-purge.md LP11 and its "For Ben" 1.
- **Industry's deletions read the whole store index at every save** (`store.keys()`). Industry was
  out of bounds for this work. Done: docs/choices-leg-purge.md LP6, then no index read at all
  (docs/choices-industry-dirty-pages.md ID2).
- **A page miss on the step thread waits for the store's mutex**, which the IO lane holds through
  a commit and its fsync, so a step that misses during a commit stalls for it. The fix needs
  page reads that do not wait on a commit.
- **A failed commit poisons the store** until the engine is reopened (the kernel's rule), so the
  next page miss panics in the step and the engine fails. That is the right end for a store that
  may be half written, but it means one disk error stops the engine rather than retrying. Done: the
  store reopens in place (docs/choices-leg-purge.md LP10).
- **Ordinary saves still confirm at once** (`commit`); P29's confirm on WORLD_SAVED (a pending
  generation confirmed when Java's world save lands) is not done. Written up: docs/choices-leg-purge.md
  LP12.
- **The world model's link** (`worldmodel`) still writes its own pages whole.
- **The row budget is a placeholder**, and a step's changes come on top of it: a long catch-up
  step that touches every row holds them all until the save after it.
