# Scale bench results

Each `scale-<UTC date>-<time>-<mode>.json` is one `rtsim-cli bench scale` run: every configuration's
measurements, the fitted slopes and contracts, the machine, the git commit and the conditions. The
`.txt` beside it is the table the run printed.

The hand-over bench (Java submit to engine publish, through JNI and mic_native's background pool)
has its own page: [handover.md](handover.md), with its `handover-<date>-<time>-gametest.json` runs.

The restore bench (the phase 1 world's snapshot against replaying its whole input log) has its own
page: [restore.md](restore.md), with its `restore-<date>-<time>-<N>m-<D>d.json` runs.

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
