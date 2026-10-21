# Choices: the paging follow-ups (PF)

Reversible choices made on 2026-10-21 by the paging follow-ups agent (branch
`feat/paging-followups`), for Ben's review (D10). Each one can be undone. The work follows
`docs/choices-paging-modules.md` ("Not done here"): markets held to their budget, a paged wheel
and paged indexes in the kernel, and paging wired into rtsim-jni.

## Markets held to their budget

Before: every market's breakpoints fall on trade-quantum boundaries (`Market::next_breakpoint`
returns `quantum.at(..)`), so one tick wakes every market, and a market page changed since the
last save cannot leave. At 4,096 markets under a 4 MiB budget the resident market rows peaked at
15.8 MB. The lead's decision: stagger market wakes by page, keeping each market's trade cadence
and shifting only its phase.

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
