# Restore bench: the phase 1 world's snapshot against its whole input log

`rtsim-cli bench restore` (crates/rtsim-cli/src/bench/restore.rs). Each `restore-<UTC date>-<time>-<N>m-<D>d.json`
is one run. Choices S1 to S7 in `docs/overnight-choices-persist.md`.

**What it measures.** The scale bench's grid of N villages (`synth::grid`, seed 42, genesis at half
the targets), joined one at a time at tick 0 as the bridge's adoptions join them (C22), then run D
world days. Then, on one thread:
- **replay**: an empty world, every join, then every day: what each start did before (choice V6,
  the whole input log replayed);
- **save**: `World::save`, then LZ4 and CRC32 of its 1 MiB pieces (`EncodedPage::encode`);
- **restore**: the pieces decoded and `World::load` (median of 5 repeats, with min and max), and
  the same through a page store on disk (commit, reopen, read every piece, load; no fsync);
- **one world day** run after the restore: the longest a tail spans before the next snapshot (S3).

Every restore is checked: the restored world's state hash equals the original's, and again after
both run one more day.

## 2026-10-20 14:55 UTC (`restore-2026-10-20-1455-1024m-100d`, `-4096m-100d`)

**Conditions.** Commit `2f9417148b43`, clean tree, release build (thin LTO), rustc 1.98.1. AMD Ryzen
AI MAX PRO 390 (24 logical CPUs), Linux 7.2.10, at nice 10, under `flock` with no other heavy job of
this session. The machine was shared with other sessions: 1-minute load 5.58 before and 6.02 after
the 1,024 run.

| | 1,024 villages, 100 days | 4,096 villages, 100 days |
|---|---:|---:|
| edges | 4,116 | 16,425 |
| news packets on the road | 297 | 1,249 |
| **replay of the whole log** (joins + days) | **4,251 ms** (200 + 4,051) | **23,160 ms** (1,004 + 22,156) |
| snapshot, raw | 2,561,825 B (2.5 KB per village) | 10,073,062 B (2.5 KB per village) |
| snapshot, stored (LZ4) | 1,366,055 B in 3 pieces | 5,460,813 B in 10 pieces |
| save: encode / LZ4 and CRC32 | 4.5 ms / 3.7 ms | 33.1 ms / 14.1 ms |
| **restore** (decompress + load), median | **8.2 ms** (1.0 + 7.2) | **35.3 ms** (4.6 + 31.0) |
| restore, min and max of 5 | 6.9 to 10.7 ms | 30.0 to 43.6 ms |
| page store on disk: commit / reopen, read and load | 0.4 ms / 7.5 ms | 1.5 ms / 29.9 ms |
| one world day after the restore | 10.2 ms | 47.3 ms |
| live heap: the world as run / as restored | 26.8 MB / 14.9 MB | 107.9 MB / 59.4 MB |
| state hash after the restore, and a day later | matches, matches | matches, matches |

**What it shows.**
- A start with the snapshot costs about 8 ms at 1,024 villages instead of 4.3 s, and does not grow
  with the world's age: the replay grows with every day since the world began (4,051 ms of the 4,251
  were the 100 days), the restore only with the world's size (about 4x for 4x the villages).
- A tail costs at most one world day of simulation (S3): 10 ms at 1,024 villages, about what the
  restore itself costs.
- The snapshot is about 2.5 KB per village raw and 1.3 KB stored, so a save that takes one writes
  1.4 MB at 1,024 villages; a save that only adds to the tail writes the meta page and a log chunk.
- The restored world holds less heap than the world that ran (14.9 against 26.8 MB): what is left
  behind by running (spare slot buffers in the timing wheels, vector capacity) is not restored.

**Limits.** One machine, shared; no fsync (the store's own crash tests cover durability); the grid
world has no players, so no caravan rows; the replay column is the replay of joins and days only
(a real log also held every trade, report and promotion, which made it longer).
