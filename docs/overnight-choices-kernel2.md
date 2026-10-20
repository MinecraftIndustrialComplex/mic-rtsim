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
| K9 | `rtsim_kernel::bsp::Bsp` steps a `PagedTable` of `Actor` models over a window `(from, to]`. Each due region handles its work in time order: its own breakpoints (they fire at their own ticks, even several in one window) interleaved with its messages, sorted by (time, breakpoints first, sender, send tick, sequence). A message is never applied in the step that sent it. Pages load on the caller's thread before the parallel phase. Messages to regions that do not exist come back as dead letters. External inputs go in with `post`. | SCALE.md §5's order (region id, then message sequence) made precise. The result is identical at any worker count and any budget (tested), but not across step lengths: the step length is part of the world. A step whose work is under `MIN_JOB_WEIGHT` (64 items, PLACEHOLDER) per job runs on the caller: the first `bench bsp` run, with jobs of a few items, ran slower at 8 and 16 workers than at 1. |
| K10 | Messages in flight are saved whole, in canonical order, by `Bsp::save`. | Like K5: in-flight traffic is bounded by activity. |

## Saves (PLAN.md §4.9, P29)

| # | Choice | Why |
|---|---|---|
| K11 | The page store's index is written incrementally: a log (`mlog-<checkpoint>.rtl`, one CRC-framed record per save, listing each changed page with where it was and where it is now) over a root (`manifest.rtm`). A save appends one record and fsyncs the log; that append is the moment the save happens. The changes since the last checkpoint stay in memory as an overlay; when it reaches `OVERLAY_MAX_PAGES` (16,384, PLACEHOLDER) or the log `LOG_MAX_BYTES` (4 MiB, PLACEHOLDER), the next save folds them into the index blocks they touch (K15) and writes a new root. On open a torn last record is cut off; a bad record with more after it stops the open with an error. Format version 2; old stores are not read (no migration before release). | Save cost follows the dirty pages: a fold costs the blocks its overlay touched, never the whole index. This is the fix O9 named. Treating a mid-log CRC failure as corruption, not a torn write, stops silent loss of later saves. Logging old locations lets a replay rebuild reference counts without reading a block. |
| K12 | Two generations, as P29 recommends: `commit_pending` writes a save but keeps the last confirmed generation's pages until `confirm(save)`; `open_for_journal(dir, durability, world_obs_seq)` opens the newest generation whose observation sequence is not past the world's, rolling back otherwise; `rollback` is durable. `commit` stays a save confirmed at once, so existing callers see no change. Only the current and the last confirmed generations are kept, and only the current one can be confirmed. A rolled-back save number is never reused. How it meets the bridge (docs/boundary.md): `requestSave(throughObsSeq)` becomes `commit_pending(batch, through)`; WORLD_SAVED becomes `confirm_through(through)`; SESSION_START's `worldNextObsSeq` feeds `select_for_journal(worldNextObsSeq - 1)` on the store `start` opened. **P29 itself stays Ben's: this builds its recommended option, it does not decide it.** | "Ahead of the journal" is read as: the generation includes observations the world's save does not have. Keeping just two generations is what P29 asks; an intermediate unconfirmed generation has no snapshot to return to. |
| K13 | Compaction finds a segment's current pages from the segment's own record headers instead of walking the index, and a page's location is packed into 24 bytes (segment numbers are 32-bit). | The old commit walked every page to find what to move, an O(all pages) part beside the manifest. |
| K14 | The crash test (`tests/crash_kill.rs`) runs the test binary as a child that saves forever with fsync on, kills it with SIGKILL at 32 pseudo-random moments (16 directories, two lives each), and checks that the store opens at a complete save whose every page matches a model, that no temporary file or unnamed segment survives, and that a pending generation rolls back exactly (on a copy). Fault-injection tests stop a save at each of six points of the save order. | A real kill exercises the code paths between the injected points. It covers process crashes, not power loss: the page cache survives a kill. The fsync order is what covers power loss, and it is unchanged in kind from O9. |
| K15 | The location index is paged too: locations live in index blocks of 512 pages (12 KiB decoded, LZ4 and CRC32 on disk, in index segments), and only 128 decoded blocks stay in memory (1.5 MiB, PLACEHOLDER, `set_index_cache_blocks`). The root lists blocks, not pages. A fold rewrites only the blocks it touched, plus the live blocks of index segments under half used, and merges small index segments once there are more than eight (as for page segments). `PageStore` keeps its `Sync`: the block cache and file handles sit behind mutexes, so lookups work through `&self` (`locate`, `contains`). `keys()` now reads every block (whole-world passes only) and panics on an IO error; `try_keys()` returns it. | The first paged bench showed the store's resident index at about 33 bytes per page (a hash map entry per page): 9 MiB at 10 million regions, more than the 4 MiB budget, so resident memory still grew with the world. SCALE.md §8 keeps "an index from id to region" resident; for regions that index is the key itself (K1), and the page store's own location index is an implementation detail that must not grow either. |

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

## Benches

| # | Choice | Why |
|---|---|---|
| K16 | The scale bench gains `paged` sweeps: the bench world (same regions, caravans, observers) on a `PagedTable` over a store on disk, at 100,000, 1,000,000, 10,000,000 and (closed and herd models) 100,000,000 regions, with a 4 MiB budget (PLACEHOLDER: small enough that every point pages), a save every 6,000 ticks (Minecraft's autosave), caravan destinations prefetched from their departure and observers reading ahead along their path. The world is built page by page with periodic commits, so no point ever holds the whole world. Contracts: resident page bytes flat (slope 0.1, max/min 4), heap flat (0.1), CPU 0.35 and bytes per save 0.35 (both PLACEHOLDERS: they rise until every event misses the cache, then hold). Step CPU excludes save CPU, which is reported beside it. Before the measured window the bench hands freed memory back to the OS (`malloc_trim`) and resets the peak resident size (`/proc/self/clear_refs`), so RSS shows the window, not the build. | A16 asks for resident memory that does not grow with the world; the bench shows it at a fixed budget, as Ben asked. Without the trim, RSS kept what the build phase had passed through the allocator (11 to 23 MiB against 5 to 8 MiB live). The dyadic model stops at 10 million regions because its conservation check walks four trees per region. |
| K17 | `rtsim-cli bench bsp` runs 200,000 trading posts (`synth::TradePost`: a herd on day slots, grain, a caravan every one to four days carried as a message) for 6 days after 1 day of warm-up, at 1, 2, 4, 8 and 16 workers, with steps of 20 ticks (1 s) and 600 ticks (30 s), resident and paged (a quarter of the pages fit), and with 0 or 1,500 rounds of hashing per event (`TradePost::work`, PLACEHOLDER) standing for a module's work: the first run showed that the bare engine's work per event (under a microsecond) is too small to gain from threads, so the bench also measures a world whose events cost what module logic will. The determinism gate compares the state hash (every page caught up to the end) and the hash of the mail in flight across every run of a step length; `--check` fails when any differs or grain is not conserved. Speedup is measured on the time inside the step, not on wall time, which saves with fsync dominate. | PLAN.md §4.7's gate replays observations and the stretch log; there are none yet, so the gate runs a synthetic world. `TradePost` lives in `synth` with the other scaffolding (O12). |
| K18 | `rtsim-cli bench o5` reads 1,000 regions once every k days (k from 1 to 1,024, staggered, after k days that bring every region into the cycle) for 1,024 days, slot model against dyadic model, and reports CPU per region per sim day. It changes nothing: O5 is Ben's. | It shows the one number O5 turns on: a slot model costs about one commit per region-day at any cadence (eager stepping, paid late), a dyadic model one read per k days, so the cadence where they cross says which regions need dyadic counts. |
| K19 | The potato profile (A11) pins every bench to CPUs 2 and 4 with `taskset`, at nice 10. `lscpu -e` and `/sys/devices/system/cpu/cpu*/topology` show SMT siblings in adjacent pairs (0-1, 2-3, ...), so CPUs 2 and 4 are two different physical cores (cores 1 and 2, same L3); CPU 0 is avoided because it takes more interrupts. The parallel-step bench runs at 1 and 2 workers there. | A11 asks for two physical cores, not SMT siblings. **This is a Ryzen AI MAX PRO 390 at up to 5.1 GHz with a large L3, not a low-end PC: two of its cores are much faster than a potato's two.** A11's Windows run was not possible tonight. The build targets baseline x86-64 (no `-C target-cpu`), as A11 wants for natives. |

| K20 | Compaction streams a segment record by record and moves at most `COMPACT_MAX_BYTES` (4 MiB, PLACEHOLDER) per save; a fold writes its index segment block by block to the file. | The paged bench at 100 million regions showed a heap peak of 80 MiB inside the window: a fold held every rewritten block, and compaction read whole segment files. Neither may scale with the world. Measured before this change; not re-measured tonight. |

## Fixes from an adversarial review (same night)

An Opus review of the kernel diff found no critical bug and five major ones. Fixed:
- A zero-filled log tail (a power loss after the file grew, before its data landed) read as an
  empty record and made the store unopenable; empty records are now torn when only zeros follow.
- `select_for_journal` (and so `open_for_journal`) left a generation the world's save holds pending;
  it now confirms it, so a later crash cannot roll back past it.
- `set_durability(Fsync)` after a NoSync build now syncs what NoSync wrote.

Not fixed tonight (known, with the fix the review proposed):
- **Two saves in flight lose changes**: `saving` is a bool, so the first ticket back unpins a page a
  later snapshot still holds. Fix: a snapshot number per page; `saved` unpins only its own.
- **An IO error inside a step drops the step's popped wakes and mail.** Fix: re-book what was
  popped (wakes at their booked time, mail in the mail wheel) before returning, or load first.
- A dropped `SaveTicket` pins its pages forever; a bad page in a compaction candidate stops every
  later save; heap accounting ignores catch-up changes (latent: no model reports heap); mail that
  lags the table lands at an eviction-dependent time (clamp to the window start); a model whose
  breakpoint is not after its anchor loops forever in a step; a panicking step job loses its pages
  (a step panic is fatal: drop the table and reopen); keys past the 48-bit page range are not
  checked outside `insert`; the prefetch wheel grows if the host never calls `prefetch_due`;
  `Inline::run` stops at the first panic.
- **For the JNI wiring (K12):** open `PagedTable` and `Bsp` only after `select_for_journal`, and
  acknowledge observations through `confirmed().obs_seq`, never `obs_seq()`: the journal must still
  hold everything after the generation a crash can roll back to.
- Docs: a save is three fsyncs (segment, folder, log), not two; the store's block directory grows by
  one entry per 512 pages.

## Where the code and the design documents differ

- **SCALE.md §5's message order** is "by region id, then by message sequence". The step sorts a
  region's work by (time, breakpoints before messages, sender, send tick, sequence): messages carry
  a delivery time, and a region's own breakpoints inside the window interleave with them.
- **PLAN.md §4.9** has the engine snapshot dirty pages at a step boundary and the IO lane compress
  and write them. `PagedTable::snapshot_dirty` is that snapshot (raw bytes, pages pinned until the
  ticket returns), and `save_dirty` still compresses on the caller for tests and the bench. There is
  no copy-on-write: the snapshot copies each dirty page's bytes.
- **SCALE.md §8's "index from id to region"**: regions need none (the key names the page, K1). An
  index for other ids (agents, notables) is not built; nothing has such ids yet.
- **PLAN.md §4.7's determinism gate** replays observations and the stretch log at 1 and 8 workers.
  There is no bridge input or governor yet, so `bench bsp` checks a synthetic world (K17).
- **The budget is soft against dirty pages** (K3). A16 reads as a hard budget; a hard one needs spills,
  which need P29 to allow a generation outside Minecraft's save.

## Not done

- No IO lane: prefetch reads run inline in the bench (`PagedTable::prefetch`), so prefetch shows
  hits but no hidden latency. The split API for the host (`prefetch_due`, `PageLocation::read`,
  `install`) is tested, not wired.
- The wake index (K5) and the mail in flight (K10) are rewritten whole at each save.
- No agent-id index, no province aggregates (SCALE.md §5), no stretch log (PLAN.md §4.7).
- Spilling dirty pages early (K3), and therefore a hard budget.
- The Windows run of the potato profile (A11).
