# Choices: the industry engine's pages written only when they changed (ID)

Reversible choices made on 2026-10-22 by the industry dirty pages agent (branch
`perf/industry-dirty-pages`), for Ben's review (D10). Each one can be undone. The work is G2-83's
option (a) (docs/choices-glassbox-g2.md, For Ben 15), decided by the coordinator while Ben was
away, provisional for Ben.

Before: every industry page was written whole at every save, pressure saves included (F36): the
globals page, every district, every replay cluster and, since G2-9, every district's glass state,
about twelve times a district page. 10,000 quiet replaying districts saved 41 MB and spent 0.4 to
0.7 s of the step thread a save. The deletions read the industry tables' index (LP6).

After: a save writes the globals page, the jni link page, and the district, cluster and glass
pages changed since the save before. A quiet district costs nothing per save. The deletions are
the changed keys whose entries are gone, with no index read. The page format is unchanged (the
same keys and the same bytes per page), so no fixture moves and no save is wiped. ID10 later moved the booked train
legs out of the globals page into a table of their own, which is a format change (fixtures regenerated, no
migration).

## The design

| # | Choice | Why |
|---|---|---|
| ID1 | **Tracked maps, marked by construction** (`rtsim-industry/src/tracked.rs`). The districts and the glass state are a `Tracked<DistrictId, _>`, a `BTreeMap` with the keys changed since the last save; the replay clusters are a `TrackedTable`, the kernel's `LazyTable` with the changed regions by key. Both read as the map they hold (`Deref`, and `get`, `iter`, `peek` on the table) and have no `DerefMut`: the only `&mut` to an entry comes from a method that marks it (`get_mut`, `get_or_insert_with`, `insert`, `remove`, `values_mut`; `apply`, `next_due`, `insert`, `remove` on the table). A mark is conservative: a `get_mut` that changes nothing still marks. | The bar is a page marked on every mutation. Marks by construction make a missed mark a compile error, not a review finding: the glass replay's state, window rollovers, the compile renumbering of G2-81, the solve cache and every other field change through one of these methods. Finer marks (only when bytes change) would need a hash or a diff per touch; the page check (ID5) measures what the conservative marks cost instead. |
| ID2 | **A save is the globals page, the changed pages and the deletions** (`Engine::save_changed`, returning `ChangedPages`). A changed key whose entry is still there is written; one whose entry is gone is deleted. The host (`rtsim-jni` `industry::save`) adds its link page. `Engine::snapshot` stays the whole state, for a fresh store, tests and state hashes. The fresh start path (the jni `Engine::start` with no restorable industry pages) still writes the whole snapshot and deletes what `industry::deletions` finds over the industry tables' index, the only index read left. | Deletions from the marks are proportional to what was removed, which LP6 noted would need a log of removed districts and clusters: the changed sets are that log. The fresh start is once per new engine; it keeps the index read because the store may hold another build's pages there (W23). |
| ID3 | **A save that did not reach the disk is written again by the next.** `save_changed` hands back a `SaveTicket` (the keys it took), which goes through `paging::Job` with the other modules' tickets. A failed or stale save (PF14) calls `Engine::save_failed`, which marks those keys again; a key marked since keeps its newer handle. | Taking the marks at save time and putting them back on failure is how the paged modules do it (PF13, PF14). Without it a failed save would lose the only record that the page changed. |
| ID4 | **A restart writes the touched pages once.** The session start (`Engine::session_start`) discards every district's open window, sweep and loaded chunks and sets every compiled glass page's `numbers_reset` (G2-81), through `get_mut` and `values_mut`, so the first save after a restart writes every district and glass page, as a save did before. From the second save on, only changed pages. | Those fields do change on most districts (a compiled district's `numbers_reset` always does), and finer marks here would save one full save per restart. |
| ID5 | **The page check.** On in debug builds, or with `RTSIM_VERIFY_PAGES` set and not `0` (`engine::page_check_on`): the engine keeps a hash of every page as of the last save (or as restored), and every `save_changed` hashes every page again and counts a page whose bytes changed without being written or deleted (`unmarked`; a debug build panics on one), the pages written, and those written unchanged (`unchanged`, what the conservative marks cost). `Engine::check_save` is a save whose pages go nowhere: the fake world (`FakeWorld::check_saves`, on by default) runs one every 1,000 ticks, and the glass replay tests' `feed` one after every record, so every industry test and every G2-9 test checks the marks. With the check off, `check_save` does nothing. | A missed mark is silent in production: the store keeps the older page and a restore goes back in time for that district. The check finds one where it happens. It was mutation-tested: `Tracked::get_mut` without its mark failed 38 tests in four test binaries (district and glass pages named in the panics); `TrackedTable::apply` and `next_due` without theirs failed 18 (cluster pages). |
| ID6 | **The globals page and the link page are written whole at every save.** The globals page holds the stations, lines, pressure, wakes, dirty set, ops, outbox and counters: 105 to 128 B in the benches and 112 B with the link page in the jni quiet save. (The booked train legs left it in ID10.) | It changes at almost every save (the clock and the counters are in it). Splitting it would pay only once its maps grow; pressure, wakes and the dirty set grow with the districts that are surveyed or due, which the benches here keep small (For Ben 1). |
| ID7 | **A cluster read marks only if it moved the region's anchor** (`TrackedTable::read`). A model without slots changes nothing on a read (its `touched` is not saved); one whose read committed a slot is marked. | Reads are how quiet districts are watched; marking them would write every watched cluster at every save. The page check covers the rule. |
| ID8 | **A restore refuses a district page stored at another district's key.** | A save writes and deletes a district's page at its own key only (ID2), so a page elsewhere would never be replaced or deleted. Before ID2 the whole snapshot overwrote every page and the case could not arise. |
| ID10 | **The booked train legs are pages of their own** (table `LEG_TABLE` 0x4904, `threat::LegIndex`, SG60). A leg page holds the legs whose ids fall in one run of 256 (`LEGS_PER_PAGE`; page id = leg id / 256), in id order, each with its dimension and the regions its line crosses; the page starts with the magic `ILEG`. The globals page keeps only the next leg id. `LegIndex` marks a page when a leg is booked into it, when cargo is looted off a leg in it and when an expiry takes a leg from it; a save writes the marked pages, and deletes a marked page that holds no leg. The cell and train lookups are rebuilt from the legs at a restore, in id order through the same insert a booking uses. A restore refuses a page whose legs are not its own ids, are out of order or reach the id counter. No migration: a save from before has no leg pages and a longer globals page, which fails to decode, so the industry engine starts afresh as for any format change (W23). | docs/stress-worlds-results.md 10.9: the globals page grew with the line legs (91, 383 and 636 KB raw on the syn200 rtsim arms) and was rewritten whole at every save, which dirty pages (ID1 to ID9) had left as the one page that did. A leg is booked on a train's departure, so the page being filled and, once a day, the pages the expiry takes legs from change; the rest of the legs sit in pages a save leaves alone. Legs are booked in id order and expire oldest first, so the changed pages are the newest and the oldest. The cells are stored with the leg, not in a lookup page, because the lookups are a function of the legs and bookings (a restored index equals the booked one, tested) and one copy cannot disagree with the other. 256 legs is a page of a few KB to a few tens of KB, in the range of a district page. |

## Tests

- `rtsim-industry` `tests/persist.rs`: `a_save_writes_only_what_changed` (a quiet engine's save is
  the globals page alone; a changed district writes exactly its district, cluster and glass pages;
  a removed one deletes exactly those), `a_run_restored_from_its_changed_pages_continues_exactly`
  (a run saved by changed pages into a store and restored from that store at many cuts equals the
  run that never stopped), `a_failed_save_is_written_again_by_the_next`.
- `glass/tests.rs` `replay_path::paging` (G2-84's runs): the pages now come from a store fed only
  by changed pages after every act, and every cut, including a restore before every step, restores
  from that store. The pages at the end, the commands and the replay model at each step are equal
  to the uninterrupted run's.
- `rtsim-jni` `tests/industry.rs`: `a_quiet_industry_save_is_flat_in_the_district_count` (1 and 16
  mills: the quiet save is the globals and link pages, 2 pages and 112 B, at both; the first save
  4 pages and 501 B against 34 pages and 6,541 B), `a_removed_district_leaves_the_store_with_the_next_save`
  (the store then reads only the industry tables: `INDUSTRY_TABLES` over `table_ids`),
  `a_failed_industry_save_is_written_again_by_the_next` (a torn log fault, then the store equals
  the engine's snapshot).
- `tracked.rs` has a unit test of the marks.
- `rtsim-industry` `tests/leg_pages.rs` (ID10): legs go to pages of 256 ids and the globals page does
  not grow with them; a save after a booking writes the page being filled and no other leg page; an
  expiry deletes the pages it empties and rewrites the one it thins, and a restore does not reuse an
  expired id; a failed save writes its leg pages again; a page at another key, an id counter behind
  the legs, another magic or any cut length is refused. `threat.rs` has unit tests that the marks
  follow a booking, a loot (only when cargo was taken) and an expiry, that the index rebuilt from the
  pages equals the booked one (cells, windows and trains), and that a line across a seam keeps its
  cells.
- The benches' glass sweeps were run with `RTSIM_VERIFY_PAGES=1` in a release build (quick
  mode): 0 unmarked and 0 unchanged pages in every sweep.

## Measured

ID9. `rtsim-cli bench industry --only <sweep>`, full mode, base 0bc2f02 against this branch, niced
under the project lock, on a loaded laptop (load 18 to 20 on 24 threads from other sessions): two
interleaved pairs (base then new, then new then base) of `glass_quiet`, `glass_regimes` and
`glass_nodes`, then three rounds of `glass_quiet` and five of `glass_regimes` with both builds
running at once, so they share the load. The ranges below are over all the runs of each build.

`glass_quiet` (BUSY = 10 export lines with a breakpoint or event every day, beside n quiet
replaying factory cells):

| n quiet | save bytes, base | save bytes, new | save CPU, base | save CPU, new | heap, base | heap, new |
|---|---|---|---|---|---|---|
| 10 | 58,400 | 17,490 (31 pages) | 61 to 239 us | 55 to 94 us | 0.51 MB | 0.51 MB |
| 100 | 426,734 | 17,491 | 0.73 to 2.1 ms | 52 to 99 us | 2.93 MB | 2.94 MB |
| 1,000 | 4,111,728 | 17,498 | 19 to 40 ms | 53 to 93 us | 27.04 MB | 27.11 MB |
| 10,000 | 40,977,628 | 17,500 | 396 to 527 ms | 97 to 161 us | 269.97 MB | 270.66 MB |

The new save is the globals page and the busy lines' 3 pages each (1 + 3 x 10 = 31), flat in n.
The base's matches G2-83 (4.11 MB and 41.0 MB). Save CPU is best of three saves; the base's
save CPU slope is 1.12 to 1.29 (declared 1, so the base exceeds it in every run), the new one's
-0.003 to 0.14 (declared 0, flat; the 0.14 run, whose 10,000 point was three times its 1,000
point, is flagged as exceeding it, the other four pass). The heap is
measured after the setup, before its first save, so the new build's extra 70 B a district is the
setup's marks waiting for that save (two key sets and a handle map); a save frees them. CPU per
sim day shows no difference outside the noise of the loaded machine: side by side, `glass_quiet`
averaged 5 to 16% more for this build at each n (three rounds) and `glass_regimes`, the same
export lines with no quiet districts, 5 to 7% less (five rounds). What the marks add to a step is
one set insert per mutable access to a district or glass state and one map insert per cluster
event.

`glass_regimes` (one to 1,000 export lines, all busy): the same bytes in both builds (1,847 to
1,864 B, 4 pages: globals, district, cluster, glass), since every page changes every day.

`glass_nodes` (one compile of n nodes): the first save after the compile writes the globals and
glass pages, 1,724 B at 10 nodes, 126,765 B at 1,000 and 1,281,277 B at 10,000 (base: the whole
snapshot, 1,860 B, 132,560 B and 1,342,560 B; the difference is the district page, unchanged by
the compile). Its CPU is the glass page's either way (1.6 to 3.2 ms at 10,000 nodes in both). The
save after that is 105 to 109 B in 1.1 to 2.5 us.

## For Ben

1. **G2-83 decided (a) by the coordinator, provisional for you.** Undoing it means going back to
   `snapshot` in `industry::save`; the format is the same either way. (b) of G2-83 (a slimmer glass
   page) is still open and now matters only for the first save after a compile or a restart.
2. **The globals page grows with surveyed and due districts** (pressure, wakes, the dirty set). It
   is 105 to 128 B in every bench here; a large server with many surveyed districts would write it
   whole at every save. Splitting it into pages is the next step if it shows up in a save's bytes.
