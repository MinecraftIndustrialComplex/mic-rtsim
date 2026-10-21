# Scale bench results

Each `scale-<UTC date>-<time>-<mode>.json` is one `rtsim-cli bench scale` run: every configuration's
measurements, the fitted slopes and contracts, the machine, the git commit and the conditions. The
`.txt` beside it is the table the run printed.

The hand-over bench (Java submit to engine publish, through JNI and mic_native's background pool)
has its own page: [handover.md](handover.md), with its `handover-<date>-<time>-gametest.json` runs.

The restore bench (the phase 1 world's snapshot against replaying its whole input log) has its own
page: [restore.md](restore.md), with its `restore-<date>-<time>-<N>m-<D>d.json` runs.

The habitat sampler's cost (the core's world-model sampler, per cell and per chunk, and the engine
side's memory and herd decisions on the world model) has its own page:
[habitat-sampler.md](habitat-sampler.md).

## How to read them

- **Sweeps** vary one count across orders of magnitude and hold the rest at the baseline in the JSON
  (`baseline`): 10,000 regions, 10,000,000 latent individuals, 1,000 caravans a day, 1 observer, 64
  regions per page, 1 warm-up day, 10 measured days. The baseline is a PLACEHOLDER chosen to run in
  seconds, not a target world (P24). The age sweeps use 100,000 regions (see their notes).
- **Models:** `herd` regions hold a grain stock (deterministic) and a herd whose births, ageing and
  deaths commit once per day slot (random, SCALE.md §4). `closed` regions hold only the stock, so they
  catch up in closed form. `dyadic` regions (from the 08:38 run on) hold the stock and a herd whose
  randomness comes from dyadic counts (DECISIONS.md O17): cohort deaths at fixed hazards and Poisson
  immigration, no ageing between cohorts, nothing to commit.
- **Metrics:** `cpu ms/day` is thread CPU time per sim day over the measured days. `catchup us` is CPU
  per read of a region left unread for the elapsed time. `heap` is live heap from a counting
  allocator; `rss` and `peak` come from `/proc/self/status`. `save` is the bytes on disk after a full
  save; `incr` is what an incremental save wrote one sim day later. `slots/day` is slot commits per
  day. `conserved` checks grain and head counts against the ledger.
- **Slopes** are least-squares fits of log10(metric) on log10(count): 0 is flat, 1 is linear.
  `declared` is what the code claims and `--check` enforces; `SCALE.md` is what SCALE.md §10 asks
  for. Where they differ, DECISIONS.md overnight choice O5 says why.

## 2026-10-20 08:30 UTC, full (`scale-2026-10-20-0830-full`)

**Conditions.** Commit `7a2eb1b`, clean tree, release build (thin LTO), rustc 1.98.1. AMD Ryzen AI
MAX PRO 390 (24 logical CPUs), 50,268,049,408 bytes of RAM, Linux 7.2.10, at nice 10, one
configuration at a time, three repeats each keeping the median. The machine was shared with other
sessions: 1-minute load 6.23 before and 7.29 after. Not the potato profile (A11): nothing was pinned
to two physical cores. Every configuration conserved grain and head counts. Every slope kept its
declared contract.

**What it shows** (numbers from the run's table):
- **CPU does not grow with the latent count, but it is not constant.** From 10,000 to 10^12 latent
  sheep in 10,000 regions, CPU ran between 1.11 and 3.49 ms per sim day (slope 0.039): it rises
  between 10^4 and 10^8, which matches the binomial sampler moving from inversion (cheap while n·p
  is small) to BTRS, and does not rise past 10^8. Live heap stayed at 2.7 MiB (slope 0.000), and a
  full save grew from 295.9 to 454.4 KiB (slope 0.024, wider varints).
- **Closed-form regions cost nothing until touched.** From 100 to 1,000,000 regions, CPU went from
  0.41 to 0.97 ms per sim day (slope 0.092). A read after 1 day or after 10,000 days cost the same,
  0.003 to 0.004 µs. World age from 1 to 1,000 days left CPU between 0.54 and 0.66 ms per day.
- **Slot-committed random processes pay for elapsed time.** A herd region read after n days made n
  commits and cost 0.505 µs at 1 day, 40.389 µs at 100 days and 3,148.206 µs at 10,000 days (slope
  0.950). In a world of 100,000 herd regions, commits per day grew with the world's age, from 10,424
  at 1 day to 99,809 at 1,000 days, approaching one per region per day (eager stepping), and CPU from
  2.58 to 15.31 ms per sim day. This is SCALE.md §4's slot rule meeting SCALE.md §10's contract; see
  DECISIONS.md O5.
- **Events cost what they cost.** From 10 to 100,000 caravans a day, CPU went from 0.46 to 13.61 ms
  per sim day (slope 0.377: fixed per-tick and observer costs dominate the low end).
- **Memory is linear in regions, because nothing pages out yet (A16).** At 1,000,000 regions, live
  heap was 94.1 MiB (closed) and 150.1 MiB (herd); a full save was 8,389.6 KiB and 19,052.4 KiB.

## 2026-10-20 08:38 UTC, full, with the dyadic model (`scale-2026-10-20-0838-full`)

**Conditions.** Code at commit `860e351`. The record says `git_dirty: true` because DECISIONS.md and
README.md had uncommitted edits during the run; no code differed from the commit. Same machine,
build and settings as above; 1-minute load 6.29 before and 5.83 after. Every configuration conserved
grain and head counts, and every slope kept its declared contract. The herd and closed rows repeat
the 08:30 run within noise (for example, a herd read after 10,000 days cost 2,939.616 µs here and
3,148.206 µs there).

**What the dyadic model adds** (numbers from the run's table):
- **Reads cost the same after any gap.** A dyadic region read after 1 day cost 7.184 µs, after 100
  days 6.850 µs, after 10,000 days 2.707 µs (slope -0.098), with no slot commits. The herd model
  cost 0.438 µs, 35.581 µs and 2,939.616 µs at those gaps, so the dyadic read is dearer than the
  slot model only for gaps under about 20 days.
- **No debt with age.** In the 100,000-region world, CPU per sim day stayed between 0.92 and 1.03 ms
  from age 1 to 1,000 days (slope -0.012). The herd model went from 2.23 to 13.25 ms.
- **Flat in regions and latent count.** From 100 to 1,000,000 regions, CPU went from 0.59 to 1.39 ms
  per sim day (slope 0.085); from 10,000 to 10^12 latent individuals it stayed between 0.70 and 1.06
  ms (slope 0.009).
- **Memory costs more per region:** 222.1 MiB of live heap at 1,000,000 regions against 150.1 MiB for
  the herd model, for the stream keys, root totals and the one-day memo each herd carries.

## 2026-10-20 08:59 UTC, full, after the review fixes (`scale-2026-10-20-0859-full`)

**This is the current reference.** Code at commit `9dcb4d7`, clean tree, run with `--check`: it
passed, every slope and every max/min bound held, and no run broke an invariant (conservation, the
dyadic model's bounds, the eager bound on slot commits). Same machine and settings as above;
1-minute load 7.83 before. Changes since 08:38 that move numbers: regions keep their booked wake
(16 bytes more per region, and in every page), the dyadic trees have 24 levels instead of 32, and
the BTRS bound is computed differently (same distribution where it was already exact).

**Numbers** (from the run's table; slopes and max/min in the second table):
- Latent count, herd model: 1.05 to 3.47 ms per sim day (slope 0.036, max/min 3.29); dyadic model:
  0.58 to 0.91 ms (slope 0.019, max/min 1.56).
- Regions, 100 to 1,000,000: closed 0.45 to 0.95 ms per day (slope 0.082); dyadic 0.57 to 1.33 ms
  (slope 0.088); herd 0.54 to 2.48 ms (slope 0.172, a 10-day run). Live heap at 1,000,000 regions:
  102.1 MiB closed, 158.1 MiB herd, 230.1 MiB dyadic; full save 8,956.9, 19,635.7 and 17,764.1 KiB.
- Elapsed time before a read, 1 to 10,000 days: closed 0.003 to 0.004 µs; dyadic 6.394 to 1.902 µs
  (slope -0.118); herd 0.492 to 3,010.510 µs (slope 0.946, one commit per day).
- World age, 1 to 1,000 days, 100,000 regions: closed 0.78 to 0.48 ms per day; dyadic 1.01 to 0.95
  ms; herd 2.45 to 13.61 ms, with slot commits per day rising from 10,424 to 99,809.
- Events, 10 to 100,000 a day (herd): 0.50 to 13.82 ms per day (slope 0.368).

## 2026-10-20 10:13 UTC, the phase 1 market sweeps (`scale-2026-10-20-1013-markets-edges`)

Only the two sweeps added with the phase 1 engine modules (`--only markets,edges`, three repeats,
median kept), on branch `overnight/phase1`. Each point builds a grid of human and ribbit villages
(`rtsim_world::synth::grid`, every number a PLACEHOLDER), trades for 10 days, then measures 10 days.
Every run conserved every good and currency. Not a `--check` run of the whole bench: the kernel
sweeps were not rerun here.

**Numbers** (from the run's table):
- Markets, 16 to 4,096 (edge rule: 4 nearest): 1.45 to 984 ms per sim day, slope 1.165 (declared
  1.3); live heap 0.5 to 120.6 MiB, slope 0.978. Events per market per day are flat (15 to 20, from
  `bench scale-one`), so the work is linear in markets; the cost per event rises from about 7 µs to
  about 15 µs as the working set leaves the caches. Heap is about 30 KB per market against the
  economy research's 1 KB ESTIMATE: market books are 16-byte integers per counter, snapshots are
  dense over goods, and every pipeline is a B-tree entry. A hot/cold layout is the fix when it
  matters.
- Edges, nearest-k 1 to 16 at 512 markets: 13.0 to 719.7 ms per sim day, slope 1.431 against k
  (declared 2.1: a market event costs O(degree × goods), and news reaches every neighbour, so
  events also grow with degree).
- A world at rest costs nothing: the two-market test in `rtsim-world` reaches zero events.

## 2026-10-20 12:03 UTC, the phase 1 market sweeps after round 2 (`scale-2026-10-20-1203-markets-edges`)

The same two sweeps (`--only markets,edges`, three repeats, median kept) on branch
`overnight/phase1b`, after the quiescence rules (choices E43 to E52 and the review fixes in
`docs/overnight-choices-phase1.md`), caravan batching (E53) and the memory work (E58). Same world
generator and windows as the 10:13 run (trade 10 days, measure days 10 to 20, which is still the
transient: a closed world settles over a few hundred days, see `rtsim-cli sim quiet`). Every run
conserved every good and currency. Laptop, other agents' builds running alongside (load about 6).

**Numbers** (10:13 run in brackets):
- Markets, 16 to 4,096: 0.20 to 157 ms per sim day [1.45 to 984], slope 1.186 [1.165]; live heap 0.3
  to 63.1 MiB [0.5 to 120.6], slope 0.965. At 4,096 markets that is 15.8 KB per market [30 KB].
  The CPU drop is fewer events: the same grids make about a sixth of the events in the measured
  window, and a tenth or less once settled (`sim quiet --markets 1024`: 1.31 events per market per
  day in days 0 to 100, 0.12 from day 300 on).
- Edges, nearest-k 1 to 16 at 512 markets: 0.98 to 275.6 ms per sim day [13.0 to 719.7], slope 1.992
  [1.431]: the small-degree points fell the most, since a sparse grid settles fastest.
- Where the memory goes (`rtsim-cli sim memory --markets 1024 --days 20`, counting allocator 15.8 KB
  per market, estimate 15.0 KB): the market table's wheel 3.3 KB (retained slot capacity and about
  14 stale wakes per market: a kernel matter), pipelines in their sorted vectors 2.0 KB, snapshots
  1.9 KB, the world's own wheel 1.7 KB, market lines 1.4 KB, edge rows 1.0 KB, the rest under 1 KB
  each.

## 2026-10-20 12:24 UTC, the dragon sweep (`scale-2026-10-20-1224-dragons`)

`--only dragons --check`, three repeats, median kept, on branch `overnight/dragons` (phase 2's
`rtsim-ecology`, choices R1 onward in `docs/overnight-choices-dragons.md`). A fixed grid of 256
villages (`rtsim_ecology::synth::grid`, livestock as market stock), one dragon per lair spread over
it, nobody watching; 2 days of warm-up, then 10 measured. The figure the contract reads is the
dragons' own CPU per dragon-day: the bench steps the world to each dragon event on its own and
times only the dragons' handlers (their decisions, legs, raids, and the market events the raids
cause). Every run conserved every good with predation (`Dragons::check`), and no dragon
materialized. Laptop, other agents' builds running alongside (load about 6).

**Numbers:**
- Dragons, 8 to 512: 10 to 14 us per dragon-day (slope 0.009, max/min 1.05, contract flat): about
  1.1 events per dragon a day at about 10 us each. The world's own trade (9.6 to 25 ms per sim day
  in the table) is the rest.
- Live heap 4.1 to 10.8 MiB (slope 0.225): the 256-village world dominates; the dragons' own rows,
  wheel and indexes are 45 KB at 8 and 0.7 MB at 512 (`engine_heap_bytes`).
- Beyond the sweep, by hand (`bench scale-one`): at 2,048 dragons on the same world (seven lairs a
  region, far denser than Ice and Fire's own spacing) prey runs out, each dragon makes 2.4 events a
  day instead of 1.1, and the cost per event stays about 10 us. Growing the map with the dragons
  (two villages each, 8 to 1,024) holds events per dragon at 1.1 but the cost per event rises from 5
  to 16 us as the world's working set leaves the caches, as the market sweep's note says; switching
  rtsim-knowledge's next-death query to the wheel's lower bound changed nothing, so it is not an
  algorithmic term.

## 2026-10-20 17:04 UTC, the ship sweep (`scale-2026-10-20-1704-ships`)

`--only ships --check`, three repeats, median kept, on branch `overnight/ships` (`rtsim-ships`,
choices T1 onward in `docs/overnight-choices-ships.md`). A fixed ocean grid of 100 ports
(`rtsim_ships::synth::ocean_grid`: sea docks, Delaunay lanes as world edges, loops sailed both ways)
with one haven and one sea pirate, nobody watching; the swept value is the cap on merchant ships per
service (headways as short as platform spacing allows). 2 days of warm-up, then 50 measured. The
figure the contract reads is the ships engine's own CPU per sim day: the bench steps the world to
each ship event on its own and times only the ships' handlers (the pirates' decisions, legs and
hazard changes, and the market events those cause). Every run conserved every good with the ships'
holds (`Ships::check`), no voyage was booked and no ship materialized, and only pirate events were
pending. Laptop, other agents' builds running alongside (load about 6).

**Numbers:**
- Merchant ships 56 to 814 (1 to 16 a service): 16.9 to 19.0 us per sim day (slope 0.032, max/min
  1.12, contract flat), the same 32 events in 50 days at every point: the pirate's. Per merchant ship
  that is 302 ns a day at 56 ships and 23 ns at 814, falling as one over the count: an unobserved
  merchant ship has no cost of its own.
- Live heap 2.6 to 4.0 MiB (slope 0.152): the 100-port world dominates; the ships engine's own rows,
  services, lanes and indexes are 129 KB at 56 ships and 381 KB at 814 (`engine_heap_bytes`), about
  310 bytes a ship.

## 2026-10-20 17:56 UTC, the herd sweeps (`scale-2026-10-20-1756-herds`)

`--only herds,members --check`, three repeats, median kept, on branch `overnight/herds` (the
`herds` module of `rtsim-ecology`, choices H2-1 onward in `docs/overnight-choices-herds.md`). Herds
spread over the synthetic valley (`rtsim_ecology::herds::synth::bench_herds`, the five species in
turn), nobody watching; 8 days of warm-up (one decision each), then 80 measured days (ten
sub-seasons). The herds engine is timed alone: herds need only the leg index, no world. After the
run every herd's cohorts are read 10,000 days later (pure dyadic reads) to time catch-up. Every run
conserved every herd's animals as integers (`Herds::check`), and nothing materialized or booked a
leg. Laptop, other agents' builds running alongside (load about 4). The file was written as
`-full` by `bench scale` and renamed `-herds`, as the dragon and ship runs were.

**Numbers:**
- Herds, 4 to 256 at 5,000 members each: 250 to 175 µs per herd-day (slope -0.085, max/min 1.43,
  contract flat), one event (a decision) per herd per sub-season: the flow field of a decision
  costs about 1.5 to 2 ms, most of it forage reads of the cells in reach. Live heap 0.2 to 1.3 MiB
  (slope 0.447): herd rows plus the forage rows of grazed cells; the engine's own structures are
  178 KB at 4 herds and 1.27 MB at 256 (`engine_heap_bytes`).
- Members per herd, 100 to 1,000,000 (16 herds): 4.20, 3.97, 3.44, 3.69, 3.82 ms per sim day (slope
  -0.012, max/min 1.22, contract flat): a herd of a million costs what a herd of a hundred does.
  Catch-up after 10,000 unread days: 0.02 to 0.04 µs per herd read (slope -0.003). Live heap 0.1 to
  0.6 MiB (slope 0.204, declared 0.3): members cost zero bytes, but a bigger herd's footprint
  grazes more forage cells, each a row until the yearly fold, up to the footprint's cap (1,536
  blocks, reached near 50,000 sheep); from 100,000 members on the heap is flat (529 KB and 556 KB
  of engine structures).
## 2026-10-20 18:47 UTC, the faction sweep (`scale-2026-10-20-1847-factions`)

`--only factions`, three repeats, median kept, on branch `overnight/conflict` (`rtsim-conflict`,
choices K2-1 onward in `docs/overnight-choices-conflict.md`). `rtsim_conflict::synth::many`: one
village per camp (the grid grows with the swept count, so density is fixed), goblins, bandits and
raiders in turn, each with a settlers' smog source inside its territory and a player's logging in
it, so every faction carries two feuds and plunders the villages and rival havens it hates; nobody
watching. 2 days of warm-up, then 40 measured. The figures the contracts read are the conflict
engine's own events and CPU per faction-day: the bench steps the world to each conflict event on its
own and times only the conflict's handlers (its decisions, legs, raids, and the market transfers
those cause). Every run conserved every good (`Conflict::check`) and nothing materialized. A rerun
with `--check` passed. Laptop, other agents' builds running alongside.

**Numbers:**
- Factions 16 to 1,024: 1.04 events per faction-day at every point (slope 0.006, max/min 1.04,
  contract flat), at 5.4 to 9.1 us of CPU per faction-day (slope 0.125 here, 0.089 in the `--check`
  rerun; declared 0.2): the cost per event rises as the world grows 64 times (80 MiB of heap at
  1,024 camps and villages), the cache effect the dragon sweep's note describes, not a scan.
- Live heap 1.3 to 80 MiB (slope 0.994): the world's markets (one village and one haven per
  faction) dominate; the conflict engine's own rows, wheel and indexes are 43 KB at 16 factions
  and 2.8 MB at 1,024 (`engine_heap_bytes`, about 2.7 KB a faction with its feuds and parties).
- Two quadratic terms were found and removed on the way: a new faction re-applied every source in
  the world (each replanning every faction it reached: about a million stale wakes at 1,024
  factions; now only the sources within the newcomer's reach, through a (faction, kind) source
  index), and routes and plunder targets scanned every node (now rings of cells, and A* over the
  roads). At a fixed 256-village world the same build makes 1.0, 1.5 and 2.1 events per faction-day
  at 16, 256 and 1,024 camps: denser camps feel each other's smog and plunder each other more
  (behaviour), at a flat cost per event.

## 2026-10-20 10:15 to 10:44 UTC, the second kernel run: paging, the parallel step, O5, the potato profile

**Files.** `scale-2026-10-20-1015-full`, `bsp-2026-10-20-1027-full`, `o5-2026-10-20-1028-full`, and
the potato profile beside them: `scale-2026-10-20-1034-full-potato`, `bsp-2026-10-20-1043-full-potato`,
`o5-2026-10-20-1044-full-potato` (`.txt` is the printed table).

**Conditions.** Code at commit `241d78b` (release, thin LTO, rustc 1.98.1, baseline x86-64, no
`target-cpu`). The records say `git_dirty: true` because docs and later kernel edits (streamed
compaction and folds, review fixes) were uncommitted while they ran; the binary was built at
`241d78b` before those edits. Nice 10, three repeats, median kept. Full runs on all 24 logical
CPUs; potato runs pinned with `taskset -c 2,4`: two different physical cores (cores 1 and 2; their
SMT siblings 3 and 5 unused), as A11 asks. 1-minute load 6.7 to 10.9 throughout: other sessions
were running (the phase 1 agent was profiling its own bench). **This laptop is a Ryzen AI MAX PRO
390 at up to 5.1 GHz with a large L3; two of its cores are far faster than a potato's two. A11's
Windows run was not possible tonight.** Choices K16 to K19 in `docs/overnight-choices-kernel2.md`.

**Paging holds resident memory flat (A16).** The bench world on paged regions, 4 MiB budget
(PLACEHOLDER), a save every 6,000 ticks, from the `paged` rows of `scale-2026-10-20-1015-full`:

| model | regions | resident pages, peak | live heap | store index | loads/day | prefetch / used | step CPU ms/day | save CPU ms/day | bytes/save |
|---|---|---|---|---|---|---|---|---|---|
| closed | 100,000 | 4.0 MiB | 5.0 MiB | 133 KiB | 349 | 321 / 321 | 7.29 | 13.1 | 497 KiB |
| closed | 1,000,000 | 4.0 MiB | 6.5 MiB | 1,536 KiB | 932 | 844 / 844 | 11.56 | 12.7 | 832 KiB |
| closed | 10,000,000 | 4.0 MiB | 7.7 MiB | 2,717 KiB | 994 | 897 / 897 | 18.16 | 17.5 | 682 KiB |
| closed | 100,000,000 | 4.0 MiB | 7.9 MiB | 2,879 KiB | 1,000 | 903 / 903 | 25.86 | 27.7 | 675 KiB |
| herd | 100,000,000 | 4.0 MiB | 7.8 MiB | 2,878 KiB | 1,000 | 904 / 903 | 37.72 | 34.4 | 1,287 KiB |
| dyadic | 10,000,000 | 6.2 MiB | 7.6 MiB | 2,717 KiB | 2,016 | 902 / 600 | 30.32 | 19.8 | 1,177 KiB |

- Resident page bytes sit at the budget at every size (slope 0.000 for closed and herd). The live
  heap grows 5.0 to 7.9 MiB over three orders of magnitude (slope 0.068) and stops: what grows is the
  store's index block cache and overlay filling to their caps (1.5 MiB and 16,384 entries, K15), and
  from 10 million to 100 million regions the heap moved 0.2 MiB. In the first run, before the index
  was paged, the store's index was 925 KiB at 1 million regions and growing by about 33 bytes per page.
- The dyadic model runs over budget (peak 6.2 MiB): its pages are bigger and dirty pages wait for
  the next save (K3); `over_budget_checks` in the JSON counts it.
- RSS after `malloc_trim` was 11 to 31 MiB. The heap peak inside the window reached 80 MiB at 100
  million regions: a fold held every rewritten index block in memory, and compaction read whole
  segment files. Both are streamed and capped since (`COMPACT_MAX_BYTES`, K20); these runs predate it.
- Saves: the log record per save stays 15 to 28 KB and bytes per save 0.5 to 1.5 MiB at every size
  (slopes 0.03 to 0.07): a save writes what changed, not the world. CPU per day rises with the miss
  rate (more events miss the cache as the world grows, and index blocks start missing too) until
  every event misses, then holds; slopes 0.09 to 0.19 against the declared 0.35.
- Prefetch: caravan destinations booked from their departure, and read-ahead along the observers'
  path, are used nearly every time; at the hot end of the LRU list they survive their lead time
  (the first run put them at the cold end and the budget check evicted every one unused).

**The parallel step and the determinism gate** (`bsp-2026-10-20-1027-full`): 200,000 trading posts,
6 days, saves every 6,000 ticks. Every run of each world ended in the same state hash and mail hash
at 1, 2, 4, 8 and 16 workers, resident and paged (a quarter of the pages fit): gates for all four
worlds passed, and grain was conserved in every run. Speed of the step itself, against 1 worker:

| world | 2 | 4 | 8 | 16 | 16, paged |
|---|---|---|---|---|---|
| 1 s steps, bare engine | 1.08x | 1.13x | 0.99x | 0.52x | 0.60x |
| 30 s steps, bare engine | 1.27x | 1.29x | 0.63x | 0.48x | 0.61x |
| 1 s steps, 1,500 rounds per event (11.6 us measured) | 1.50x | 2.16x | 2.96x | 3.32x | 3.29x |
| 30 s steps, 1,500 rounds per event | 1.85x | 3.57x | 6.07x | 9.23x | 7.19x |

- The bare engine's own work per event (catch-up of a day's slot, a transfer) is under a
  microsecond, so the serial gather and merge cost as much as the parallel part and threads do not
  pay. With per-event work of the size module logic will have, the step scales: 9.2x on 16 workers
  with 30-second steps (6,095 regions per step), 3.3x with 1-second steps (206 per step).
- `MIN_JOB_WEIGHT` keeps small steps on the caller; it was added after the first run, where 8 and
  16 workers ran the bare world 1.5 to 2 times slower than 1.

**O5: where slot commits cost** (`o5-2026-10-20-1028-full`): 1,000 regions read once every k days,
CPU per region per sim day after k days of warm-up:

| read every k days | 1 | 4 | 16 | 32 | 64 | 256 | 1,024 |
|---|---|---|---|---|---|---|---|
| slot model, ns | 337.6 | 358.5 | 388.7 | 338.2 | 362.7 | 359.8 | 321.2 |
| dyadic model, ns | 5,109.8 | 1,327.6 | 408.0 | 211.5 | 89.0 | 22.5 | 4.8 |

The slot model costs about 320 to 390 ns per region-day whatever the cadence: it is eager stepping,
paid late. The dyadic model costs about 5 us per read, so it is dearer for regions read more often
than every 16 days and cheaper past that (67x at 1,024 days). Regions that players and caravans
visit daily do not need dyadic counts; regions nobody visits for months do.

**The potato profile** (two physical cores, nice 10) beside the full machine:

| | full machine | potato |
|---|---|---|
| paged closed, 100 million regions: step CPU ms/day, live heap | 25.86, 7.9 MiB | 22.72, 7.9 MiB |
| paged herd, 100 million regions | 37.72, 7.8 MiB | 33.70, 7.8 MiB |
| resident herd, 1 million regions, CPU ms/day | 2.19 | 2.23 |
| herd read after 10,000 days | 3,564.4 us | 3,004.1 us |
| step, 30 s steps, 1,500 rounds per event, 2 workers | 1.85x | 1.99x |
| step, 1 s steps, 1,500 rounds per event, 2 workers | 1.50x | 1.47x |
| O5 crossover | about 16 days | about 16 days |

Single-threaded numbers match within noise: one fast core is one fast core. The parallel step gets
close to 2x on two cores when there is work per event. The potato scale check failed one bound: the
latent sweep's CPU max/min was 4.11 against the PLACEHOLDER 4.0 (the binomial sampler's switch from
inversion to BTRS, see the 08:30 notes); every slope and every invariant held.

## 2026-10-21 06:11 to 06:50 UTC, the modules on paging: before and after, full machine and potato

**Runs.**
- `scale-2026-10-21-0639-full-full-before` and `scale-2026-10-21-0640-full-full-after`: the full
  machine.
- `scale-2026-10-21-0641-full-potato-before` and `scale-2026-10-21-0640-full-potato-after`: the
  potato profile.
- `scale-2026-10-21-0646-full-potato-swap-*`: the potato pair again, with the cores swapped.
- `scale-2026-10-21-0611-quick-paged-after`: the new paged sweeps.
- `scale-2026-10-21-0625-quick-markets-paged-recheck`: markets-paged after its contract changed.

Each `.txt` is the printed table.

**Conditions.**
- **Builds.** Before is commit 1c43097: both kernel review bugs fixed, every module resident. It
  ran from outside the repository, so its JSON has no commit. After is a804324:
  `feat/paging-modules` with origin/main 3ba14e5 merged. Its JSON reads 5c02276 because two
  doc-only commits landed while it ran. Both are release builds at nice 10.
- **Machine.** The laptop had rebooted after a power loss. The 1-minute load average was 23 at the
  start and 50 to 59 at the end, from other sessions.
- **Pairing.** Before and after ran side by side on the 13 sweeps both binaries have, three
  repeats each, keeping the median:
  - full machine: before pinned to CPUs 12-17, after to 18-23 (each scale point is one thread);
  - potato: before on CPUs 2,4, after on 6,8 (two physical cores each).
- **New sweeps.** The paged sweeps ran once, in quick mode, on CPUs 10,11.
- **Void run.** An earlier attempt (01:55 to 05:00 UTC) ran at a load of 100 to 130 and was cut
  off by the power loss. Its numbers are void.

**Checks.** No contract fails after the change that did not fail before:

| Run | Before | After |
|---|---|---|
| full | factions heap 1.188 (1.1) | factions heap 1.147 (1.1) |
| potato | paged cpu 0.391 (0.35); markets cpu 1.305 (1.30); factions heap 1.188 | paged cpu 0.390; elapsed dyadic catch-up max/min 4.04 (bound 4.0; before 3.64, code untouched); factions heap 1.147 |
| paged sweeps | | markets-paged resident 1.107 (1.1): redeclared at 1.2 as "not bounded yet", recheck passed |

The factions heap slope is the phase 1 world under the camps, before and after.

**Live heap** (the same on both machines), before and after:

| Point | Before | After |
|---|---|---|
| events, 100,000 a day | 50.8 MiB | 10.1 MiB (the wheel's freed burst vectors, PM8) |
| markets, 4,096 | 67.4 MiB | 45.8 MiB |
| edges, 16 | 24.7 MiB | 13.5 MiB |
| factions, 1,024 | 200.9 MiB | 156.1 MiB |
| dragons, 512 | 23.1 MiB | 19.5 MiB |
| ships, 16 a service | 4.0 MiB | 2.2 MiB |
| herds, 256 | 1.3 MiB | 1.0 MiB |
| paged regions, 10^8 | 7.8 MiB | 7.6 MiB |

**CPU per sim day** (after over before):
- **Full pair.** Most points are within 10%. Exceptions:
  - markets below 1,024 villages: 0.73 to 1.46 (all under 25 ms a day); 1.17 at 1,024 and 0.97
    at 4,096;
  - dragons: 0.97 to 1.18;
  - factions: 0.57 to 1.09;
  - edges: 0.79 to 0.99.
- **Potato pair.**
  - The first pair read dragons and ships at 2x after. With the cores swapped (before on 6,8,
    after on 2,4) they read 0.83 to 1.04 and 0.90 to 1.15, and markets 0.97 to 1.11. So the 2x
    was core placement on a loaded machine, not the change.
  - Everything else was within 13% on the first pair.

**State hashes.**
- **The same before and after:** age, dragons, edges, events, latent, markets, paged and regions.
- **Different:** factions, herds, members and ships. Their save blobs changed format (conflict
  RTC2, herds version 2, ships RTS2), and herds also decide by page now (PM6).
- Paged against resident within the after build is the same at every point (each module's tests,
  and the paged sweeps).

**The new paged sweeps** (quick, after only). Row pages resident at peak, as a slope over the
world's size:

| Sweep | Slope | Contract |
|---|---|---|
| herds-paged | 0.137 | 0.15 |
| dragons_paged | 0.226 | 0.3 |
| ships-paged | 0.019 | 0.1 |
| factions-paged | -0.001 | 0.1 |
| markets-paged | 1.107 | 1.2, not bounded yet |

Live heap at the top point, resident against paged:

| Point | Resident | Paged | What dominates |
|---|---|---|---|
| herds, 4,096 | 22.8 MiB | 2.5 MiB | |
| ships, 6,400 ports | 139.7 MiB | 127.2 MiB | the phase 1 world and the port network, resident by design |
| dragons, 32,768 | 753.7 MiB | 735.2 MiB | the phase 1 world under the dragons |

docs/choices-paging-modules.md has the per-module tables.
