# Overnight choices: kernel follow-ups (K)

Reversible choices made on 2026-10-20 while Ben was asleep, by the second kernel agent (branch
`overnight/kernel2`). Each one is undoable; none is final until Ben reviews it (D10). DECISIONS.md
is not edited tonight; the O series there is the first kernel agent's.

The work: paging under a memory budget (A16, SCALE.md §8), incremental and two-generation saves
(PLAN.md §4.9, P29), the bulk-synchronous parallel step (SCALE.md §5), the P4 storage bench, the
potato profile (A11), and a bench that shows where O5 matters.

## Paging (A16, SCALE.md §8)

| # | Choice | Why |
|---|---|---|
| K1 | Paging is a new type, `rtsim_kernel::paging::PagedTable`, beside `LazyTable`, with the same rules (catch-up on read, reads never reschedule, drain `next_due` first). `LazyTable` is unchanged. Regions are named by a stable `u64` key; a region's page is `key / regions_per_page`. `morton2` packs grid coordinates so a page is a square block. | Other agents build on `LazyTable` tonight, so the kernel API only grows. A key that computes its page needs no per-region index, so a region that is not resident costs zero bytes. |
| K2 | Defaults: budget 64 MiB, 64 regions per page, prefetch 100 ticks (5 s) ahead. All three are PLACEHOLDERS in the code. | A16 says the default comes from phase 0 measurements, and P24 holds the targets. 64 regions per page is the scale bench's existing value. |
| K3 | Only pages whose state is on disk are evicted. A page changed only by catch-up counts as on disk and may be dropped unsaved: a reload redoes the same slot commits. A page changed by an event, an insert, a removal or a breakpoint is dirty until a save writes it; it is never written out early (no spill), so the budget is soft against dirty pages, and the table counts the checks that end over budget. | O4 makes catch-up a pure function of the stored state and the time, so dropping it is exact. A spill would be a page generation written outside Minecraft's save, which is P29's question; dirty pages are bounded by what happened since the last save, not by world size. |
| K4 | A booked wake is a wheel entry `(region key, booked tick)`. It is live when the region's page holds the same booked tick; otherwise it is stale. Wakes due at the same tick fire in key order. | Evicting a page then never touches the wheel. Key order is canonical, so a table reopened from a save fires wakes in the same order as one that never stopped. Cost: a stale entry for an evicted region costs one page load. |
| K5 | The wheel of booked wakes and the clock are resident and saved whole into one meta page at each save (`save_meta`). Opening a table restores them with no page resident. | The wheel is what says which regions wake next (SCALE.md §8), so it stays resident. Its size is the number of pending wakes, not of regions; but it is rewritten whole at each save, so save cost has an O(pending wakes) part. Chunking it into dirty-tracked pages is the fix if the bench ever shows it. |
| K6 | Prefetch is split for the host's IO lane: `prefetch_due` returns pages with their disk location, `PageLocation::read` reads one without the store, and `install` drops the bytes if the page became resident or moved on disk meanwhile. `prefetch` does it all inline for tests and the bench. A prefetched page enters at the hot end of the LRU list, as if just used. Sources: every booked wake (lead ticks before it), `prefetch_at` for events the caller has scheduled (a caravan's arrival), and `read_ahead` for player movement (`read_ahead_grid` gives the cells ahead of a player on a square grid). | The engine owns no threads. The location check makes a late install safe without a version number per page. The first bench run put prefetched pages at the cold end, and the budget check after the step evicted every one of them before use (0 hits of 568 a day); at the hot end they survive their lead time. |
| K7 | The budget counts an estimate: each resident page's struct and slot vector, the heap its models report (`PagedModel::heap_bytes`, default 0), and 24 bytes per id-index entry. The bench reports the counting allocator's live heap beside it. | Exact accounting would need an allocator per table. The estimate is checked against the measured heap in the bench. |

## The parallel step (SCALE.md §5)

| # | Choice | Why |
|---|---|---|
| K8 | `rtsim_kernel::exec::Executor` has two methods: `workers()` (advice only) and `run(Vec<Job>)`, which runs every job and returns when all have finished, resuming a job's panic on the caller. `exec::scope(exec, \|s\| s.spawn(..))` is the scope-and-spawn form on top. Two implementations: `Inline` (the caller, in order) and `ThreadPool` (std threads the caller creates; the calling thread works too). The module docs show the mic-native-rt implementation over `rt::scope`. | Object-safe, so the engine takes `&dyn Executor`. `rt::scope` already has this contract (jobs borrow from the stack; returns when all finished; a worker helps while it waits). `ThreadPool` needs one `unsafe` lifetime erasure, argued in a SAFETY comment and tested with panicking jobs and concurrent callers. |
| K9 | `rtsim_kernel::bsp::Bsp` steps a `PagedTable` of `Actor` models over a window `(from, to]`. Each due region handles its work in time order: its own breakpoints (they fire at their own ticks, even several in one window) interleaved with its messages, sorted by (time, breakpoints first, sender, send tick, sequence). A message is never applied in the step that sent it. Pages load on the caller's thread before the parallel phase. Messages to regions that do not exist come back as dead letters. External inputs go in with `post`. | SCALE.md §5's order (region id, then message sequence) made precise. The result is identical at any worker count and any budget (tested), but not across step lengths: the step length is part of the world. |
| K10 | Messages in flight are saved whole, in canonical order, by `Bsp::save`. | Like K5: in-flight traffic is bounded by activity. |

## Saves (PLAN.md §4.9, P29)

| # | Choice | Why |
|---|---|---|
| K11 | The page store's index is a checkpoint (`manifest.rtm`, the whole index) plus a log (`mlog-<checkpoint>.rtl`, one CRC-framed record per save). A save appends one record of the pages it changed and fsyncs the log; that append is the moment the save happens. The log is folded into a new checkpoint when it reaches the checkpoint's size and at least `LOG_MIN_BYTES` (256 KiB, PLACEHOLDER), and at the first save. On open a torn last record is cut off; a bad record with more after it stops the open with an error. Format version 2; old stores are not read (no migration before release). | Save cost now follows the dirty pages (amortized: a checkpoint costs the whole index but comes after as many logged bytes). This is the fix O9 named. Treating a mid-log CRC failure as corruption, not a torn write, stops silent loss of later saves. |
| K12 | Two generations, as P29 recommends: `commit_pending` writes a save but keeps the last confirmed generation's pages until `confirm(save)`; `open_for_journal(dir, durability, world_obs_seq)` opens the newest generation whose observation sequence is not past the world's, rolling back otherwise; `rollback` is durable. `commit` stays a save confirmed at once, so existing callers see no change. Only the current and the last confirmed generations are kept, and only the current one can be confirmed. A rolled-back save number is never reused. **P29 itself stays Ben's: this builds its recommended option, it does not decide it.** | "Ahead of the journal" is read as: the generation includes observations the world's save does not have. Keeping just two generations is what P29 asks; an intermediate unconfirmed generation has no snapshot to return to. |
| K13 | Compaction finds a segment's current pages from the segment's own record headers instead of walking the whole index, and a page's location is packed into 24 bytes (segment numbers are 32-bit), so the resident index costs 32 bytes per page plus the hash map's overhead. | The old commit walked every page to find what to move, an O(all pages) part beside the manifest. The index is resident by design (SCALE.md §8), so its size per page matters for the budget. |
| K14 | The crash test (`tests/crash_kill.rs`) runs the test binary as a child that saves forever with fsync on, kills it with SIGKILL at 32 pseudo-random moments (16 directories, two lives each), and checks that the store opens at a complete save whose every page matches a model, that no temporary file or unnamed segment survives, and that a pending generation rolls back exactly (on a copy). Fault-injection tests stop a save at each of six points of the save order. | A real kill exercises the code paths between the injected points. It covers process crashes, not power loss: the page cache survives a kill. The fsync order is what covers power loss, and it is unchanged in kind from O9. |

## Notes for the engine from the bridge contract

- **Repeated events are no-ops.** TAKEN, DIED and COMMAND_RESULT may arrive more than once
  (docs/boundary.md, "Idempotent events", §3.1). The engine must apply each at most once: by op id
  while `Replay` still remembers it, and by state after that (a member already gone, an operation
  already done). Nothing in the kernel enforces the second half; the modules that handle these
  records must.
- **Java op ids are random.** `OpId::bridge` wraps 63 random bits chosen by the bridge, not the
  observation's sequence (a rolled-back journal reissues sequences). Its doc comment said
  otherwise and is corrected. An engine that re-derives a transfer after a crash must give it the
  same op id it had, or the bridge cannot recognise the duplicate.
