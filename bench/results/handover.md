# Hand-over cost per tick (phase 0, PLAN.md §4.5 and §14)

What one hand-over costs between the bridge (Java, the server thread) and the engine (Rust,
rtsim-jni on mic_native's background pool), measured by the GameTest
`HandoverBenchGameTests.handOverCostPerTick` (`neoforge/`, `./gametest.sh -PrtsimBench`). Each
`handover-<UTC date>-<time>-gametest.json` here is one run; the bench writes it into the GameTest
game directory (`core/run-gametest/mic_rtsim-bench/`) and it was copied here unchanged.

## What is measured

Each engine gets its own boundary session (`BoundarySession`, no Minecraft in the loop), driven by
the server thread:

- **burst**: 2,000 hand-overs back to back inside one server tick, after 200 not counted. The pool
  thread is warm.
- **cadence**: one hand-over after a 50 ms sleep, 100 times: the play case. The GameTest server ticks
  as fast as it can, so the sleep stands in for a 50 ms tick; the pool thread is parked between
  steps and every step pays its wake-up.
- **payloads** (PLACEHOLDERS, not a measured tick): the preamble only (a TICK record), 30
  PUPPET_STATE records (1.7 KB) and 1,000 (56 KB, a fifth of the 256 KiB default buffer).

Per hand-over, in nanoseconds (the JSON has min, p50, p90, p99, max and mean):

| Field | What |
|---|---|
| `handOver` | Java's whole `BoundarySession.handOver` call: preamble, journal drain, the header, the release store of `published` and the JNI `submit`. The cost the bridge adds to the server tick. |
| `submitCall` | the `Engine.submit` call alone (for the native engine: the JNI call and mic_native's `spawn` onto the background pool). |
| `submitToPublish` | from the start of the submit call to the server thread seeing the step's `published` word (acquire), spinning. The bridge itself never spins: it looks again on the next tick. |
| `step` | native only: the step's own time on the pool thread, from its start to its publish (its stats). |

The stub (`StubEngine`, pure Java, a single-thread executor) runs the same bench for comparison.

## 2026-10-20, three runs (11:01, 11:03, 11:04 UTC)

**Conditions.** Code of commit `18e6ea1` (the runs were made from the working tree that became it).
AMD Ryzen AI MAX PRO 390 (24 logical CPUs), Linux 7.2.10, OpenJDK 21.0.12.1, the GameTest server
under `nice -n 10` (gametest.sh), mic_native's background pool at its defaults (22 threads, nice
19), rtsim-jni `release` profile. The machine was shared with other sessions: 1-minute load 9.9 to
12.7 during the runs. Not the potato profile (A11): nothing was pinned to two physical cores, and
the 50 ms gap is a sleep, not a tick's work, so caches are less disturbed than in play.

**Per-tick cost to the server thread (cadence, the play case), p50 over the three runs:**

| Payload | native `handOver` | native `submitCall` | stub `handOver` | stub `submitCall` |
|---|---|---|---|---|
| preamble only | 15.9 to 17.2 µs | 15.1 to 16.1 µs | 10.0 to 12.1 µs | 7.5 to 9.5 µs |
| 30 records | 16.9 to 20.0 µs | 16.1 to 18.9 µs | 9.4 to 12.6 µs | 7.9 to 11.1 µs |
| 1,000 records | 16.7 to 19.0 µs | 15.8 to 18.0 µs | 7.9 to 12.7 µs | 6.8 to 11.2 µs |

p99 of the native `handOver` in cadence: 22 to 40 µs.

**Submit to publish (cadence), p50 and p99:**

| Payload | native p50 | native p99 | native `step` p50 | stub p50 | stub p99 |
|---|---|---|---|---|---|
| preamble only | 34.2 to 35.7 µs | 52.5 to 74.9 µs | 8.2 to 8.5 µs | 42.5 to 45.6 µs | 69 to 218 µs |
| 30 records | 36.0 to 43.2 µs | 77.6 to 621.6 µs | 8.9 to 10.2 µs | 40.3 to 49.7 µs | 96 to 268 µs |
| 1,000 records | 46.8 to 56.0 µs | 67.2 to 231.2 µs | 19.3 to 24.3 µs | 58.5 to 69.6 µs | 94 to 1,632 µs |

**Warm (burst), p50:** native `handOver` 0.71 to 2.52 µs, `submitCall` 0.62 to 1.79 µs; submit to
publish 2.5 to 5.1 µs (preamble only or 30 records) and 14.6 to 18.1 µs (1,000 records); `step` 0.7
to 1.2 µs and 12.2 to 15.2 µs. The stub: submit to publish 2.6 to 7.9 µs and 23.7 to 26.7 µs.

**What it shows:**
- **The hand-over costs the server thread about 16 to 20 µs a tick in play** (p50; p99 up to
  40 µs), 0.03 to 0.04% of a 50 ms tick. Nearly all of it is the JNI submit call, which costs 0.6
  to 1.8 µs while the pool is warm: the difference is the wake-up of a parked worker plus a server
  thread back from a 50 ms sleep (this bench does not separate the two). The stub's submit, an
  executor waking its thread, costs 7 to 11 µs in the same conditions.
- **Commands are ready 34 to 56 µs after the submit** (p50), so the bridge, which looks again one
  tick (50 ms) later, always finds the step finished at these sizes. The worst p99 seen was 622 µs.
- **The engine's own step is 8 to 10 µs cold for an empty or small hand-over** (under 1.2 µs warm)
  and 19 to 24 µs cold for 1,000 records (12 to 15 µs warm): 11 to 14 ns a record, warm. The minimal
  world simulates nothing yet, so this is the link's cost, not the sim's.
- **Native against stub:** the native submit call costs about twice the stub's in cadence, and its
  submit to publish is 5 to 14 µs shorter (p50). Whether a spinning worker or a dedicated waker
  is worth its CPU is mic_native's pool question (P24), not the link's.
- Spread between runs is wide (other sessions loaded the machine to 10 to 12); compare runs side by
  side on a quiet machine before reading differences of a few microseconds.

## 2026-10-20 11:34 UTC, after the review fixes (`handover-2026-10-20-1134-gametest.json`)

**Conditions.** Code of commit `9a6831f` (the review fixes of `c587876`: every `NativeEngine`
native call under its monitor, the outbox trimmed by COMMAND_RESULT, op ids, buffer holds), same
machine and settings, 1-minute load 5.2 (lower than the three runs above).

**Numbers (p50):** cadence, native: `handOver` 11.4 to 19.5 µs, `submitCall` 10.4 to 16.1 µs,
submit to publish 27.7 to 43.4 µs (p99 71 to 365 µs), `step` 7.7 to 17.9 µs; stub: `handOver`
6.9 to 10.1 µs, submit to publish 29.4 to 40.8 µs. Burst, native: submit to publish 2.9 to 4.0 µs
(preamble only or 30 records) and 11.6 µs (1,000 records). Within the spread of the runs above:
the fixes did not change the hand-over's cost.

## 2026-10-20 13:33 and 13:35 UTC, with phase 1 running (`handover-2026-10-20-1333-gametest.json`, `-1335-`)

**Conditions.** Code of commit `102ee01` (phase 1 on the native engine, branch
`overnight/phase1-jni`), same machine and settings as above, 1-minute load 4.7 to 6.6 (other
sessions). A third rig joins the bench, **`native-p1`**: the native engine with its phase 1 world
running. 64 villages are adopted (an 8 x 8 grid 500 blocks apart, five structures, 45 pieces each),
the hand-overs start ten days of game time on (so the first step runs the world through ten days of
flows, news and caravans, inside the warm-up), the module mask has phase 1's four modules on, and
every hand-over also carries four PLAYER records (players 30 blocks off four villages; the bridge
reports a player every 20 ticks, so this is more than play). Every step runs the phase 1 world to
the hand-over's game time; every 20 hand-overs the engine looks for caravans and bands near the
players, every 100 it refreshes the markets near them. At the end of the 1335 run its counters read:
64 villages, world tick 246,900, 8 caravans materialized (SPAWN_GROUP and MERCHANT_OFFERS sent; the
bench's command sink never answers, so they stay spawning), 28 MARKET snapshots, 72 inputs logged (64
joins, 8 promotions). The JSON's `step` field for `native-p1` is null in these two files (the writer
only kept it for `native`, fixed after); the step p50 below is from the run's `[gametest]` lines.

**Per-tick cost to the server thread (cadence, the play case), p50, the two runs:**

| Payload | native `handOver` | native-p1 `handOver` | native-p1 `submitCall` | stub `handOver` |
|---|---|---|---|---|
| preamble only (+ 4 PLAYER for p1) | 12.8, 17.8 µs | 11.2, 20.1 µs | 8.0, 15.5 µs | 10.6, 13.7 µs |
| 30 records | 10.3, 17.2 µs | 12.5, 18.5 µs | 9.7, 15.5 µs | 8.5, 12.5 µs |
| 1,000 records | 8.9, 14.5 µs | 16.9, 17.6 µs | 13.8, 14.6 µs | 5.9, 6.8 µs |

p99 of the native-p1 `handOver` in cadence: 23 to 51 µs (native 21 to 29 µs).

**Submit to publish and the step (cadence), p50:**

| Payload | native s2p | native step | native-p1 s2p | native-p1 step |
|---|---|---|---|---|
| preamble only | 36.1, 47.2 µs | 14.5, 18.5 µs | 32.7, 47.1 µs | 15.1, 19.8 µs |
| 30 records | 32.2, 43.1 µs | 14.4, 17.8 µs | 38.6, 46.2 µs | 18.4, 20.3 µs |
| 1,000 records | 40.3, 51.2 µs | 25.4, 28.6 µs | 58.0, 57.5 µs | 33.1, 32.1 µs |

native-p1 submit to publish p99: 71 to 492 µs.

**What it shows:**
- **With phase 1 running the hand-over still costs the server thread 11 to 20 µs a tick** (p50),
  the same band as the 11 to 20 µs measured without it: the phase 1 world runs on the pool thread,
  and what the server thread pays is the submit's wake-up of a parked worker plus the extra PLAYER
  records it writes. Only the 1,000-record payload sits a few microseconds higher with phase 1
  (16.9 and 17.6 µs against 8.9 and 14.5), within the spread between the two runs.
- **The step grows by 1 to 7 µs** (p50) with a 64-village world running to each hand-over, so a
  quiet event-driven world costs the pool thread almost nothing per tick; the first step's ten days
  (and the interest and market passes every 20 and 100 hand-overs) land in the p99.
- **TEST mode is not play.** A first run of the phase 1 rig opened the engine with `FLAG_TEST`
  (`handover-2026-10-20-1331-testmode-gametest.json`): its `handOver` read 27 to 29 µs, and 176 µs
  with 1,000 records, because `NativeEngine.submit` drains the received log (every record, through
  JNI) in TEST mode. GameTests pay that; a server does not. The rig now runs without it.
