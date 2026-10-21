# Choices: the leg index purge and the paging leftovers (LP)

Reversible choices made on 2026-10-21 by the leg purge agent (branch `fix/leg-purge`), for Ben's
review (D10). Each one can be undone. Two tasks:

1. **The leg index never purged** (VIEWER.md §12.4): the phase 1 world's meta page (table 0x5001)
   and the heap grew without bound, and `a_thousand_days_at_max_keeps_the_state_bounded` was
   ignored for it.
2. **The paging leftovers** of docs/choices-paging-followups.md ("Not done here"), in D1 order:
   the world's meta page written whole, a failed commit that stopped the engine, P29, and
   industry's deletions over the whole store index.

Ben's decisions are at the end ("For Ben"); none blocks the work. **No page format changed**:
the leg index, the wheels and the market rows encode as they did, so GameTest worlds need no
deletion for this merge.

## What was measured

`a_thousand_days_at_max_keeps_the_state_bounded` (rtsim-host `tests/host.rs`): the host test's
populated world (nine villages, six ports, a haven, a fire dragon flying, a goblin camp) at max
speed, 1,000 game days, a save every 10 game days, release, nice 10, on the laptop under other
sessions' load. Run alone (`--exact`): the heap is `mic_native_rt::memory::in_use()`, process
wide, so tests running beside it inflate it. Before is origin/main at 6fe9c90, built from a
`git archive` copy.

| | Day 10 | Day 100 | Day 1,000 |
|---|---|---|---|
| **Before**: table 0x5001 (the phase 1 world's meta page) | 62,265 B | 130,310 B | 706,496 B |
| Before: all pages | 75,815 B | 141,521 B | 717,837 B |
| Before: heap | 4,633 KiB | 5,295 KiB | 14,641 KiB |
| **Leg purge only** (LP1): table 0x5001 | 30,850 B | 47,386 B | 124,129 B |
| Leg purge only: the meta page's market table section (raw) | 18,106 B | 29,139 B | 109,480 B |
| Leg purge only: market wakes on the wheel | 1,661 | 2,622 | 9,162 |
| **After** (LP1, LP7, LP8): table 0x5001 | 25,307 B | 22,483 B | 19,327 B |
| After: all pages | 38,858 B | 33,797 B | 30,681 B |
| After: heap | 4,329 KiB | 4,335 KiB | 4,660 KiB |
| After: market wakes on the wheel (11 markets) | 11 | 5 | 4 |
| After: leg bookings, cells (the dragon's one leg) | 170, 170 | 170, 170 | 0, 0 |

- Before, the test failed: `the pages do not grow: (150121, 717837)`. It passes now.
- **5,000 game days** (`RTSIM_HOST_SOAK_DAYS=5000`), after: all pages 26 to 35 KB the whole way
  (30,740 B on day 5,000), table 0x5001 19,355 B on day 5,000, heap 4,285 to 4,481 KiB (4,468 KiB on
  day 5,000). Flat.
- **Speed** is unchanged within the noise: 113 game days a second before, 108 to 113 after
  (1,000 days, the same load); 172 on the 5,000-day run under less load.
- **What the leg purge alone left growing** was the market table's wheel: every market event
  rebooks the market's next breakpoint, and the old wake stayed on the wheel until it popped stale
  (LP7). Its 9,162 wakes on day 1,000 were the 109 KB.

## The leg index

| # | Choice | Why |
|---|---|---|
| LP1 | **The purge drops only stale bookings, and exactly** (`LegIndex::purge_stale(before)`, rtsim-travel `legs.rs`): in every window that ended at or before `before`, the bookings of legs replanned or removed since go; live bookings stay, however old. It replaces `LegIndex::purge`, which dropped whole windows, live bookings included, and had no caller. | Every read (`sharing`, `in_region`) already keeps only live bookings, so no read can tell a purged index from an unpurged one: the purge never changes what a live mover or a pending meeting reads, and may run at any time. Old live bookings are read: a leg booked with a start in the past (a caravan or a herd booked when a player comes near, a module catching up after a switch-off, U10) must still find every live leg it crosses in those windows. Dropping whole windows would make that read depend on when the purge ran. The past windows then hold at most the live legs' own bookings: 170 for the dragon's one leg in flight, 0 once it lands. |
| LP2 | **Once a game day, at fixed game-time boundaries** (`LegIndex::purge_due(now)`, `PURGE_EVERY` = 24,000 ticks, PLACEHOLDER), called at the end of `World::run_until`, which owns the index. A purge runs when `now` passes a multiple of `PURGE_EVERY` since the last; a stride purges once, at its end, for the latest boundary it passed. `set_purge_every(0)` turns it off (tests). | Boundaries of game time, not of the caller's steps, so the cost a game day is about the same at any stride: a 20-tick run purges in the step that passes each boundary, a 2-day stride once for both. Since the purge is exact (LP1) when it runs sets only the cost, never the answers, so every stride agrees. Once a day bounds the stale bookings to about a day's replans, and on a live server (a game day is 20 minutes) a purge is rare next to its steps. Only the World owns a leg index in the engine (herds, dragons, ships and the conflict book into the world's); the others are tests and the herds bench. |
| LP3 | **The purge's cost is the cells of the past windows**, live bookings kept from earlier purges included, and it shrinks a cell's vector when it drops from it. | The past windows hold only live legs' bookings after the first purge, so a purge costs the live legs' past bookings plus a day's replans. A cursor that skipped windows purged before would miss bookings made into the past after that purge (LP1's catch-up case). Shrinking keeps the heap honest: a cell that held a day of replans keeps its capacity otherwise. |
| LP4 | **No page-format change.** The purge state (`purge_every`, `purged_through`) is session state, not saved; a load sets `PURGE_EVERY` and 0, so the first `purge_due` after a load (the first `World::run_until`) purges at once, for the latest boundary before its clock, a full pass over the past windows as any purge. | The index on disk is smaller after a purge, encoded as before. A purge after a load at another time than in an unbroken run changes nothing a read sees (LP1), so it needs no saving, and the restore tests' hashes (which exclude the leg cells, as `World::state_hash` always did) stay equal. |
| LP8 | **The purge also removes cells the reads emptied.** A read drops the stale bookings it meets but keeps the cell; a purge removes every past cell left empty, by itself or by a read. | Without it the index kept an empty cell (a map entry and an empty vector) for every region a replanned leg had crossed and a read had cleaned. `a_purge_drops_only_the_past_windows_stale_bookings` checks that a cell `in_region` emptied goes at the next purge. |
| LP9 | **The stride test is a dragons test** (`strides_across_purge_boundaries_keep_one_state`, rtsim-ecology `tests/dragons.rs`): the populated dragon world with a player standing on the dragons' ground for 6 game days, stepped at 20 ticks, 1,200, 7,000 (strides ending mid-day), 2 game days, in one call, and at 20 ticks with the purge off. Every run must give the same dragon stats, dragon and world state hashes, booked cells, live bookings and commands; the purge-off run must hold more bookings. | Dragons are the module that books and replans the most legs (flights, retargets, patrols) and the player's interest area reads the index all the time, so it is where a purge that changed a read would show. The purge-off run proves the purge did something and that it is invisible. |
| LP5 | **The soak test is un-ignored and prints what grows**: the phase 1 meta page by section (raw bytes, `World::meta_sections`, a diagnostic that encodes the meta page with marks), legs, leg bookings and cells, market rows and market wakes, on day 10, day 100 and the last day. Its bounds are as they were (pages at most twice the early value plus 64 KiB; heap twice plus 16 MiB). | The section rows found LP7 in one run. The heap bound is loose because the heap is process wide and the test binary runs the other soaks beside it; the pages bound is the one that failed before. |

## Market wakes

| # | Choice | Why |
|---|---|---|
| LP7 | **A rebooking takes the wake it replaces off the wheel.** `TimingWheel::cancel(at, is)` (rtsim-kernel `wheel.rs`) removes the newest pending entry for time `at` that `is` picks: its slot follows from `at` and `now` alone, since a slot cascades the moment `now` enters its range, and one scheduled at or before `now` waits in `ready`. `LazyTable` and `PagedLazyTable` (`lazy.rs`) cancel a row's old wake in `reschedule` and `remove`, and forget it once it pops. `stale_wakes` now counts cancels too; `pending_wakes` is one per region with a breakpoint booked. | Every market event rebooks the market's next breakpoint, so each market left a trail of stale wakes, one per event, until each one's time came: 9,162 on day 1,000 for 11 markets, 109 KB of the meta page (above) and growing with the trading. The cost of a cancel is one slot at worst, searched from the newest entry, so a recent booking is found at once; no back-pointers, and the entries still move freely. Pop order is unchanged (a slot keeps schedule order and a cancel keeps the others' order), so `World::state_hash` and every module's decisions are the same as on main. Saved bytes are not: the wheels hold no stale wakes and `stale_wakes` counts cancels, so a hash over page or snapshot bytes (the host's industry hash) differs from main's, while strides still agree with each other. Tests: `cancelling_matches_a_sorted_reference` (proptest: any pending or popped entry cancelled against a sorted reference, across cascades) and `rebooking_leaves_one_wake_a_region` (`tests/lazy_stagger.rs`: 64 regions rebooked over 50 quanta, staggered and held, keep exactly one wake each). **Cost**, measured A/B against main (`rtsim-cli bench scale-one`, 4,096 paged markets, 10 days of warmup and 40 measured, release, nice 10, side by side on cores 4 and 6): sim CPU 285 against 294 ms a game day, engine heap 24.8 against 27.9 MB, page loads 4,695 against 4,937 a day (a stale wake popping loads its market's page to check its stamp), save CPU 346 against 380 ms a day, the same events and state hash; `bench scale --quick --only markets,markets-paged` the same within noise. The worst case is a slot crowded with far wakes rebooked in turn: the search walks the slot and `Vec::remove` shifts its tail, 2.6 µs an event at 4,096 wakes in one slot and 9.6 µs at 16,384 (the review's probe). At 7,000 to 9,000 market events a game day for 4,096 markets (the run above) that is under 25 ms a day even then. If a bigger world shows it, the fallback is counted compaction ("Still open"). |

## The paging leftovers

| # | Choice | Why |
|---|---|---|
| LP6 | **Industry's deletions read only the industry tables' index blocks** (`industry::deletions` through `stored_keys`, `PageStore::table_ids` over its three tables), as the paged modules' and phase 1's do; `Link::load` reads its keys the same way, and an index read error is now a save failure or a refused start instead of a panic in `PageStore::keys`. | The whole store index at every save grew with every module's pages (herds, dragons, the markets). Now the cost is industry's own pages, which the same save writes whole anyway, so the deletions cost no more than the snapshot. Proportional to what is deleted would need a log of removed districts and clusters in industry's state; not worth it while its snapshot is whole. `table_ids` also passes the store's overlay (pages changed since the last checkpoint) once a table, a kernel cost shared with the other modules' deletions. |
| LP10 | **A poisoned store reopens in place, and the engine runs on.** `PageStore::recover` (rtsim-kernel `page.rs`): when a write failed partway and poisoned the store, it closes every handle and runs `PageStore::open` on the same directory and durability (as a restart after a crash: the log replays, a torn record is cut, strays go, a pending generation stays pending), keeps the index cache's size and replaces the store. The engine (`recover_store`, rtsim-jni `engine.rs`) calls it under the store's lock right after a failed commit, so no reader sees the store poisoned, and again before the next commit if that reopen failed. The save that failed counts as failed whether or not its log record made it: `durable` stays, Java keeps its journal, and the modules write their pages again at the next save (PF14). The world model's own commit does the same. **A record whose write or sync failed is cut** before the reopen (`Fault::SyncFails` in the tests): after a failed fsync the kernel may drop the record's pages while the file still reads whole, so a reopen would build on a record that a later crash turns into a hole under every later record (`PageError::Crc` at the next open). Cutting it makes the failed save not durable, as the engine already counts it. | A failed commit poisoned the store until the engine was reopened, so the next page miss on the step thread panicked and stopped the engine: one disk error (a full disk for a moment, an IO error) cost the whole session. The files always hold a consistent store (that is what crash safety means), so reopening them is the same recovery a restart does, without the restart. Treating a save whose record did reach the disk as failed costs one extra write of its pages and keeps one rule: only an acknowledged save moves the journal. **A double fault still stops the engine**: if the reopen fails too, the store stays poisoned until the next save tries again, and a page miss in between panics as before. Tests: `a_poisoned_store_recovers_in_place` (a torn log record, a log record whose sync failed, a failure before the cleanup, in the middle of a checkpoint and after one; a pending generation stays pending) and `a_save_failing_partway_costs_that_save_not_the_engine` (rtsim-jni `tests/protocol.rs`: a torn record and a failure before the cleanup at a real save; no acknowledgement, durable stays, the next save is acknowledged, and a restart from the files opens at it). Without the recovery the protocol test fails. |
| LP11 | **The world's meta page is left whole; written up for Ben** ("For Ben" 1). | Splitting its nodes and edges into paged tables changes the page format and the world's save code, and it pays only on large worlds (below). It was the first item in D1 order, but it is large and a format decision. |
| LP12 | **P29's confirm on WORLD_SAVED is not wired; written up for Ben** ("For Ben" 2). | Ben chose P29's design (DECISIONS.md P29, "Keep previous generation") and the kernel has it (K12), but wiring it changes when the engine acknowledges a save, which is the bridge protocol's contract with Java's journal, touches about 23 protocol tests that assume a save is durable at once, and meets the pressure saves (PF15) in a way that needs a choice. Large, and partly an interface decision. |

**The meta page, measured.** A probe (a temporary test, not committed) built `synth::grid(n, 42)`
worlds with `Genesis::Share(500)`, ran each 10 game days and encoded the meta page with section
marks (raw bytes, before LZ4):

| Nodes (edges) | Meta page, raw | Encode time | Nodes section | Edges section |
|---|---|---|---|---|
| 256 (564) | 389 KB | 0.87 ms | 245 KB (63%) | 66 KB (17%) |
| 1,024 (2,217) | 1.54 MB | 4.9 ms | 966 KB (63%) | 262 KB (17%) |
| 4,096 (8,837) | 6.06 MB | 29 ms | 3.82 MB (63%) | 1.03 MB (17%) |

The rest is the settlements, the knowledge, the world wheel, packets, wakes, roads and bands, each 1 to 5%. The
encode runs on the step thread while it builds a save (`phase1::snapshot` under the state lock),
at every save that takes a phase 1 snapshot, which is every save while herds, dragons, ships or the
conflict run. In the host test's populated world (nine villages) table 0x5001 holds 19 KB.

## For Ben

1. **The phase 1 world's meta page (LP11).** It is written whole at every snapshot: 389 KB raw
   and 0.9 ms of the step thread at 256 nodes, 1.5 MB and 4.9 ms at 1,024, 6 MB and 29 ms at
   4,096, linear in the nodes. Options:
   - (a) **Leave it whole until a world reaches about 1,000 nodes** (recommended). At the node
     counts MIC worlds have (villages and ports a dimension, tens to a few hundred) a save costs
     under a millisecond of encoding and a few hundred KB before LZ4, once every autosave. The
     real risk is the 29 ms step at 4,096 nodes, which D1 would not accept, but no MIC world is
     near it.
   - (b) **Page the nodes and edges now**: each node and its edges in a paged table under the
     PF12 `(table, 0)` convention, written when changed, with the meta page keeping the rest.
     That is about 80% of the bytes and the encode time. It changes the page format (GameTest
     worlds deleted after the merge, no compat before release), and every node change (a market's
     stock, a pipeline's state) must mark its page, which is the larger part of the work.
   - (c) **Encode on the IO lane**: copy the world's resident state into a snapshot structure on
     the step thread and encode it on the IO thread. Takes the encode off the step thread with no
     format change, but the copy is still linear and the bytes written are the same.
2. **P29: wiring the confirm on WORLD_SAVED (LP12).** The kernel already has two generations
   (K12): `commit_pending`, `confirm_through`, `select_for_journal`. Wiring it means: an ordinary
   save commits pending; WORLD_SAVED (`durable` obs seq) confirms the generation at or below it
   (`confirm_through`) on the IO lane; `start` already rolls back what is pending (PF15) and would
   select by the journal instead (`select_for_journal(worldNextObsSeq - 1)`); the acknowledgement
   (`durable`, `saves_done`) moves at the confirm, not the commit, so Java never trims its journal
   past a generation a rollback may cut (with JB9's `min(ack, writtenThrough)` on the Java side);
   `close(save = true)` must leave a confirmed generation, since no WORLD_SAVED reaches a closed
   engine: it confirms at once, as today. The viewer and the fork already read the confirmed generation
   (`view_confirmed`), so they would lag one save. The choice inside it is the pressure saves
   (PF15): the store keeps only the current and the last confirmed generation, and only the
   current can be confirmed, so a pressure save between `requestSave` and its WORLD_SAVED would
   leave the ordinary save unconfirmable. Options:
   - (a) **Hold pressure saves while an ordinary save waits for its WORLD_SAVED** (recommended):
     the window is one Minecraft world save (seconds), and the rows already stay over budget
     while a save is in flight (PF15). No kernel change.
   - (b) **Let a later confirm cover a pressure save**: WORLD_SAVED confirms the current
     generation when its obs seq is at or below the world's. A pressure save's obs seq is the
     step's, ahead of the world save, so under steady pressure nothing would be confirmed and the
     journal would grow until a quiet save.
   - (c) **Keep three generations in the kernel** (confirmed, the ordinary save, a pressure save
     on top): a store manifest change, for a window of seconds.

## Still open

- **The world model's load** (`worldmodel::load`) still reads the whole store index (`store.keys()`)
  twice, at start only, and panics on an index read error there. `table_ids(TABLE_REGIONS)` would
  do; out of this work's scope (it is not industry).
- **A double fault** (LP10): a failed commit whose reopen also fails leaves the store poisoned
  until the next save, and a page miss in between stops the engine.
- **A page miss on the step thread still waits for the store's mutex** through a commit and its
  fsync (docs/choices-paging-followups.md), and now also through a reopen after a failed one.
- **The cancel's worst case** (LP7): a rebooking searches one wheel slot, linear in the wakes it
  holds. If a world with tens of thousands of markets shows it, replace the eager cancel with
  counted compaction: count the superseded wakes and, once they outnumber the live ones, drop
  them all with an order-keeping pass over the slots (amortized constant, at most twice the live
  wakes). Never `swap_remove`: it reorders wakes of one time. The paged table would need its
  liveness from the wheel itself (the newest stamp of each region), since its rows may be out.
- **`PURGE_EVERY` is a placeholder** (LP2). Once a day is cheap at the numbers above; a world with
  many more movers replanning would show it in the purge's cost, not in what it keeps.
