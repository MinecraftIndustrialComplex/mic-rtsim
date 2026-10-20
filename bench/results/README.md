# Scale bench results

Each `scale-<UTC date>-<time>-<mode>.json` is one `rtsim-cli bench scale` run: every configuration's
measurements, the fitted slopes and contracts, the machine, the git commit and the conditions. The
`.txt` beside it is the table the run printed.

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
