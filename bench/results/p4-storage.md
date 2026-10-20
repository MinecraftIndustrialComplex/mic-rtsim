# P4 storage bench: our columns against bevy_ecs

P4 (DECISIONS.md, PLAN.md §17) asks whether agents live in `bevy_ecs` behind a facade with our own
executor, or in our own columns, and says the phase 0 storage bench settles it with the numbers
brought to Ben. This is that bench. It measures storage only. It does not recommend a switch: P4 is
Ben's decision. The kernel was not changed.

## What ran

- **Ours:** `rtsim_kernel::store::ColumnTable` through the `RowStore` facade, as it is on `main`
  (one `Vec` per component, dense rows, swap-remove, generation-checked sparse index, 6-byte `RowId`).
  The results files call it `cols`.
- **bevy_ecs 0.20.0**, the newest release (its rust-version is 1.95; it builds on 1.98.1), with
  `default-features = false, features = ["std"]`: no executor (`multi_threaded` and `async_executor`
  off), no reflection, no backtraces. Only `World`, `QueryState`, `Entity` and components. No
  `Schedule`, no `System`. Every `QueryState` is built once and reused, as a facade would hold it.
  Each bevy step first calls `World::increment_change_tick`, as a system run would. bevy writes change
  ticks as it always does, except in the lines marked "bypass".
- **Same data, same operations.** Both backends store the same component types. Rows come from a
  fixed SplitMix64 seed. Handles, churn positions and fresh rows are drawn in the same order. Before
  timing, every pattern runs 7 steps on a 20,000-row world in every variant, and the bench aborts
  unless all variants end in the same state (sums over every field, tagged counts and tagged sums).
  All variants agreed in every run.
- **Rows.** Agent: `Pos {x, z: i32}`, `Vel {dx, dz: i16}`, `Hunger(u16)`, `Owner([u8; 6])`,
  `Flags(u8)`: 21 bytes of payload. Notable (SCALE.md §9, 352 bytes): `Wealth(i64)` 8, the rest of the
  96-byte core 88, 16 memory slots 128, 16 sentiment entries 128, as four components.
- **Sizes:** 100,000 and 1,000,000 rows.
- **Timing:** `std::time::Instant` around batches of steps, `std::hint::black_box` on the stores.
  Each variant is calibrated by doubling (this is also the warm-up) to at least 200 ms per run, then
  measured 7 times; the median is reported. The variants of one pattern and size run interleaved
  (run 1 of each, then run 2 of each, and so on), so drift in clock speed or load hits all of them.
  One thread.
- **Build:** the cargo bench profile, which inherits release (opt-level 3, thin LTO, panic unwind,
  line tables), rustc 1.98.1, the default x86-64 target (SSE2; no AVX), plus loop headers aligned to
  64 bytes for every crate (`-C llvm-args=-align-loops=64`, see "Code layout" below).

### Variants

| | Pattern | Ours | bevy |
|---|---|---|---|
| a | `hunger += 1`, all rows | `column_mut::<Hunger>()` | `iter_mut().for_each` (the usual way); `contiguous_iter_mut` (0.20's table slices, ticks filled per slice); contiguous with `bypass_change_detection` (no tick writes) |
| b | `pos += vel`, all rows | **copy:** `RowStore` lends one column at a time, so `Vel` is copied to a scratch `Vec`, then `column_mut::<Pos>()`; **split (unsafe):** both columns at once through a raw pointer, standing in for a two-column borrow the API does not have | as in a, over `(&mut Pos, &Vel)` |
| c | 10% of rows by stored handle, fixed random order: read `Pos`, write `Hunger` | `get::<Pos>(id)` then `get_mut::<Hunger>(id)`: two lookups, as `RowStore` has no multi-component get | `Query::get_mut(entity)` on one `query_mut` per pass (one lookup; the faster bevy way, used as bevy's headline); `World::get` + `World::get_mut` (two lookups) |
| d | remove 1% and insert 1% per step, then a; d0 is the churn alone | `RowStore::remove` (swap-remove in every column) and `insert` | `World::despawn` and `World::spawn`; stream as bevy iter |
| d2 | tag churn (not in the brief): per step 0.5% of rows lose a tag and 0.5% gain it; the tagged count stays near 1% | **flag bit:** `get_mut::<Flags>(id)`, set or clear bit 0; **side table:** insert or remove a `(TagOf(RowId),)` row in a second `ColumnTable` | **table marker:** `insert(Tagged)` / `remove::<Tagged>()`, which moves the row to another table; **sparse marker:** the same with a `SparseSet` marker, which changes archetype but not table |
| e | `hunger += 1` on the ~1% of rows with the tag | **flag scan:** scan `Flags`, collect positions, then `column_mut::<Hunger>()` (the safe way today); **side table:** walk the handles, `get_mut::<Hunger>` each; **flag split (unsafe):** one branchless pass over `Flags` and `Hunger` together | `QueryState<&mut Hunger, With<Tagged>>` (tagged rows are their own table); the same `With<TaggedSparse>` |
| f | `wealth += 1` on 352-byte notables | `column_mut::<Wealth>()` | as in a; plus an array-of-structs reference, `Vec<352-byte struct>` |
| g | live heap per row | counting global allocator, and `ColumnTable::heap_bytes` | counting global allocator: live heap after building minus before `World::new` |
| h | build n rows one at a time (h: agents, h352: notables) | `ColumnTable::new`, `insert` per row | `World::new`, `spawn` per row; `spawn_batch` (reserves up front); for notables also a plain `Vec::push` reference |

The slice loops of a, b, f and the e flag split are shared `#[inline(never)]` kernels, so ours, bevy
contiguous and bevy bypass run the same machine code at the same address and differ only in how they
produce the slices.

## Conditions

| Run | File (`bench/results/`) | CPUs | Nice | 1-min load before / after | Use |
|---|---|---|---|---|---|
| full | `p4-storage-2026-10-20-0945-full` | all 24 | 10 | 5.39 / 5.15 | primary |
| pinned | `p4-storage-2026-10-20-0948-pinned2` | 20, 22 | 10 | 5.78 / 6.22 | primary |
| pinned repeat | `p4-storage-2026-10-20-0952-pinned2` | 20, 22 | 10 | 13.12 / 6.43 | agrees with 0948 |
| full repeat | `p4-storage-2026-10-20-0950-full` | all 24 | 10 | 6.45 / 13.91 | disturbed by outside load; kept, not used below |
| before loop alignment | `p4-storage-2026-10-20-0936-full`, `-0938-pinned2` | all 24; 20, 22 | 10 | 5.54 / 7.85; 8.37 / 6.08 | superseded build `17fe4be`, kept as evidence for "Code layout" |

All four primary and repeat runs are commit `6eebd58`, clean tree, no debug assertions, under the
shared lock `/tmp/mic-rtsim-kernel2-heavy.lock`. Each run took 126 to 130 s.

**Machine:** AMD Ryzen AI MAX PRO 390, 12 cores and 24 logical CPUs in two groups of 6 cores (CPUs
0-11 and 12-23), each group with its own 32 MiB L3; 1 MiB L2 per core; 50,268,049,408 bytes of RAM;
Linux 7.2.10. The machine was shared with other sessions (agents, a VM): the 1-minute load average
was 5.2 to 6.2 during the primary runs, and about 11 GB were in swap.

**Pinned profile (DECISIONS.md A11):** `taskset -c 20,22`, nice 10, the bench binary run directly.
From sysfs, CPU 20 is `core_id` 12 (SMT siblings 20-21) and CPU 22 is `core_id` 13 (siblings 22-23):
two different physical cores, no SMT pair, sharing one L3 (CPUs 12-23), like a 2-core part. `lscpu -e`
numbers the same cores 10 and 11. They were among the least busy CPUs in a 3-second sample. Their SMT
siblings 21 and 23 were not reserved, so other processes could still run there.

## Results at 1,000,000 rows

Medians in ns per operation. "x" is the ratio to the block's first line (ours) in the same run.

| | Variant | Unit | Full | x | Pinned | x | Pinned repeat |
|---|---|---|---:|---:|---:|---:|---:|
| a | ours | ns/row | 0.022 | 1.00 | 0.023 | 1.00 | 0.022 |
| | bevy iter | ns/row | 0.055 | 2.50 | 0.057 | 2.49 | 0.055 |
| | bevy contig | ns/row | 0.058 | 2.63 | 0.060 | 2.65 | 0.059 |
| | bevy contig bypass | ns/row | 0.022 | 1.00 | 0.022 | 0.98 | 0.022 |
| b | ours copy | ns/row | 0.211 | 1.00 | 0.199 | 1.00 | 0.207 |
| | ours split (unsafe) | ns/row | 0.141 | 0.67 | 0.142 | 0.71 | 0.141 |
| | bevy iter | ns/row | 0.175 | 0.83 | 0.173 | 0.87 | 0.174 |
| | bevy contig | ns/row | 0.181 | 0.86 | 0.185 | 0.93 | 0.178 |
| | bevy contig bypass | ns/row | 0.141 | 0.67 | 0.140 | 0.70 | 0.139 |
| c | ours get + get_mut | ns/lookup | 15.20 | 1.00 | 14.12 | 1.00 | 13.56 |
| | bevy query.get_mut | ns/lookup | 14.66 | 0.96 | 11.36 | 0.80 | 10.17 |
| | bevy world.get + get_mut | ns/lookup | 29.12 | 1.92 | 18.19 | 1.29 | 20.53 |
| d | ours | ns/row | 1.629 | 1.00 | 1.447 | 1.00 | 1.774 |
| | bevy | ns/row | 8.029 | 4.93 | 7.521 | 5.20 | 7.527 |
| d0 | ours | ns/row | 1.577 | 1.00 | 1.384 | 1.00 | 1.694 |
| | bevy | ns/row | 7.606 | 4.82 | 7.629 | 5.51 | 7.606 |
| d2 | ours flag bit | ns/change | 15.54 | 1.00 | 14.57 | 1.00 | 13.69 |
| | ours side table | ns/change | 9.652 | 0.62 | 9.513 | 0.65 | 9.357 |
| | bevy table marker | ns/change | 468.3 | 30.13 | 465.7 | 31.96 | 459.1 |
| | bevy sparse marker | ns/change | 179.3 | 11.54 | 153.9 | 10.56 | 155.8 |
| e | ours flag scan | ns/tagged | 33.41 | 1.00 | 34.44 | 1.00 | 34.74 |
| | ours side table | ns/tagged | 5.197 | 0.16 | 5.547 | 0.16 | 5.512 |
| | ours flag split (unsafe) | ns/tagged | 5.497 | 0.16 | 5.898 | 0.17 | 5.842 |
| | bevy table marker | ns/tagged | 0.045 | 0.001 | 0.047 | 0.001 | 0.046 |
| | bevy sparse marker | ns/tagged | 1.203 | 0.04 | 1.354 | 0.04 | 1.254 |
| f | ours | ns/row | 0.089 | 1.00 | 0.092 | 1.00 | 0.094 |
| | bevy iter | ns/row | 0.120 | 1.35 | 0.120 | 1.30 | 0.112 |
| | bevy contig | ns/row | 0.119 | 1.34 | 0.140 | 1.51 | 0.135 |
| | bevy contig bypass | ns/row | 0.085 | 0.96 | 0.097 | 1.05 | 0.096 |
| | aos `Vec<352 B struct>` | ns/row | 1.809 | 20.28 | 1.609 | 17.42 | 1.949 |
| h | ours insert | ns/row | 16.76 | 1.00 | 17.22 | 1.00 | 18.59 |
| | bevy spawn | ns/row | 50.88 | 3.04 | 52.04 | 3.02 | 53.96 |
| | bevy spawn_batch | ns/row | 45.32 | 2.70 | 32.46 | 1.88 | 41.03 |
| h352 | ours insert | ns/row | 129.7 | 1.00 | 138.6 | 1.00 | 129.2 |
| | bevy spawn | ns/row | 163.3 | 1.26 | 163.9 | 1.18 | 166.3 |
| | bevy spawn_batch | ns/row | 132.8 | 1.02 | 129.5 | 0.93 | 125.5 |
| | aos `Vec::push` | ns/row | 95.04 | 0.73 | 105.6 | 0.76 | 98.23 |

Tagged rows: 9,887 of 1,000,000 (e, d2). In c, 100,000 lookups per pass. In d, 10,000 removals and
10,000 insertions per step. In d2, 5,000 untags and 5,000 tags per step.

## Results at 100,000 rows

| | Variant | Unit | Full | x | Pinned | x | Pinned repeat |
|---|---|---|---:|---:|---:|---:|---:|
| a | ours | ns/row | 0.021 | 1.00 | 0.022 | 1.00 | 0.022 |
| | bevy iter | ns/row | 0.043 | 2.06 | 0.044 | 2.02 | 0.043 |
| | bevy contig | ns/row | 0.049 | 2.35 | 0.051 | 2.37 | 0.050 |
| | bevy contig bypass | ns/row | 0.021 | 1.00 | 0.022 | 1.00 | 0.022 |
| b | ours copy | ns/row | 0.188 | 1.00 | 0.192 | 1.00 | 0.201 |
| | ours split (unsafe) | ns/row | 0.141 | 0.75 | 0.145 | 0.76 | 0.139 |
| | bevy iter | ns/row | 0.168 | 0.89 | 0.172 | 0.90 | 0.173 |
| | bevy contig | ns/row | 0.167 | 0.89 | 0.175 | 0.91 | 0.175 |
| | bevy contig bypass | ns/row | 0.141 | 0.75 | 0.144 | 0.75 | 0.147 |
| c | ours get + get_mut | ns/lookup | 9.812 | 1.00 | 9.617 | 1.00 | 9.276 |
| | bevy query.get_mut | ns/lookup | 6.152 | 0.63 | 6.195 | 0.64 | 6.388 |
| | bevy world.get + get_mut | ns/lookup | 10.11 | 1.03 | 10.36 | 1.08 | 10.38 |
| d | ours | ns/row | 0.388 | 1.00 | 0.381 | 1.00 | 0.389 |
| | bevy | ns/row | 1.502 | 3.87 | 1.412 | 3.71 | 1.514 |
| d0 | ours | ns/row | 0.373 | 1.00 | 0.369 | 1.00 | 0.369 |
| | bevy | ns/row | 1.450 | 3.89 | 1.326 | 3.60 | 1.608 |
| d2 | ours flag bit | ns/change | 9.029 | 1.00 | 9.237 | 1.00 | 9.509 |
| | ours side table | ns/change | 7.587 | 0.84 | 7.563 | 0.82 | 7.706 |
| | bevy table marker | ns/change | 92.95 | 10.29 | 87.23 | 9.44 | 88.80 |
| | bevy sparse marker | ns/change | 40.21 | 4.45 | 40.00 | 4.33 | 40.49 |
| e | ours flag scan | ns/tagged | 29.44 | 1.00 | 30.26 | 1.00 | 30.17 |
| | ours side table | ns/tagged | 3.375 | 0.11 | 3.407 | 0.11 | 3.357 |
| | ours flag split (unsafe) | ns/tagged | 5.247 | 0.18 | 5.532 | 0.18 | 5.525 |
| | bevy table marker | ns/tagged | 0.046 | 0.002 | 0.051 | 0.002 | 0.048 |
| | bevy sparse marker | ns/tagged | 0.632 | 0.02 | 0.651 | 0.02 | 0.643 |
| f | ours | ns/row | 0.084 | 1.00 | 0.089 | 1.00 | 0.089 |
| | bevy iter | ns/row | 0.103 | 1.22 | 0.101 | 1.13 | 0.102 |
| | bevy contig | ns/row | 0.117 | 1.38 | 0.121 | 1.35 | 0.118 |
| | bevy contig bypass | ns/row | 0.088 | 1.04 | 0.088 | 0.98 | 0.085 |
| | aos `Vec<352 B struct>` | ns/row | 0.582 | 6.89 | 0.617 | 6.90 | 0.622 |
| h | ours insert | ns/row | 15.90 | 1.00 | 16.54 | 1.00 | 16.69 |
| | bevy spawn | ns/row | 29.21 | 1.84 | 30.78 | 1.86 | 30.90 |
| | bevy spawn_batch | ns/row | 29.98 | 1.89 | 18.75 | 1.13 | 19.31 |
| h352 | ours insert | ns/row | 113.8 | 1.00 | 100.7 | 1.00 | 101.9 |
| | bevy spawn | ns/row | 148.0 | 1.30 | 141.2 | 1.40 | 134.5 |
| | bevy spawn_batch | ns/row | 99.28 | 0.87 | 99.94 | 0.99 | 95.17 |
| | aos `Vec::push` | ns/row | 83.77 | 0.74 | 78.31 | 0.78 | 75.91 |

Tagged rows: 1,014 of 100,000.

## Memory (g)

Live heap after building one store of n rows one at a time, minus live heap before creating it. The
numbers were identical in every run.

| Rows | Store | n | Payload B/row | Heap bytes | B/row |
|---|---|---:|---:|---:|---:|
| agent | ours | 1,000,000 | 21 | 36,700,440 | 36.7 |
| agent | bevy spawn | 1,000,000 | 21 | 114,309,777 | 114.3 |
| agent | bevy spawn_batch | 1,000,000 | 21 | 110,180,817 | 110.2 |
| notable | ours | 1,000,000 | 352 | 383,779,040 | 383.8 |
| notable | bevy spawn | 1,000,000 | 352 | 452,999,587 | 453.0 |
| notable | bevy spawn_batch | 1,000,000 | 352 | 433,180,579 | 433.2 |
| agent | ours | 100,000 | 21 | 4,587,800 | 45.9 |
| agent | bevy spawn | 100,000 | 21 | 14,301,841 | 143.0 |
| agent | bevy spawn_batch | 100,000 | 21 | 11,660,721 | 116.6 |
| notable | ours | 100,000 | 352 | 47,972,576 | 479.7 |
| notable | bevy spawn | 100,000 | 352 | 56,637,859 | 566.4 |
| notable | bevy spawn_batch | 100,000 | 352 | 43,960,483 | 439.6 |

An empty `World` holds 13,591 bytes; an empty `ColumnTable` 224 to 280. `ColumnTable::heap_bytes`
agreed with the allocator to within those 280 bytes.

## What the numbers say

Numbers below are from the primary full and pinned runs and the pinned repeat, at 1,000,000 rows
unless a size is given. Where a figure is derived (per-operation costs, break-even), the arithmetic
is on these measured medians.

**a. One field, `hunger += 1`.** Ours streams at 0.022 to 0.023 ns per row (22 µs per pass over a
million rows). bevy's usual per-row iterator takes 0.055 to 0.057: 2.5x ours, and 2.0x at 100,000
rows. The difference is change detection: for each 2-byte write, bevy also writes a 4-byte changed
tick. With change detection bypassed, bevy matches ours exactly (same kernel, same bytes). bevy's
contiguous slices with change detection on are a little slower than its iterator (2.6x ours),
because they fill the tick slice in a separate pass.

**b. Two fields, `pos += vel`.** `RowStore::column_mut` borrows the whole table, so a system cannot
hold `Pos` mutably and read `Vel` at the same time. The safe workaround copies `Vel` each pass: 0.199
to 0.211 ns per row, which is 1.15 to 1.2x slower than bevy's iterator (0.173 to 0.175). With both
columns at once (the unsafe split, standing in for a two-column borrow), ours takes 0.141 to 0.142:
1.2x faster than bevy's iterator and equal to bevy with ticks bypassed (same kernel). So today's
facade loses this pattern; one API addition would win it.

**c. Random access by handle.** At 100,000 rows, where everything fits in cache, bevy's
`Query::get_mut` takes 6.2 to 6.4 ns per lookup and ours 9.3 to 9.8: bevy is 1.45 to 1.6x faster.
Ours pays for two lookups (`get` then `get_mut`, each a sparse-index read plus a search of the
table's column list by `TypeId` and a downcast), because `RowStore` cannot fetch two components of
one row at once. At 1,000,000 rows the lookups miss cache and the gap shrinks: bevy 10.2 to 14.7 ns,
ours 13.6 to 15.2, bevy 1.04 to 1.33x faster, with run spreads up to ±47%. bevy's other way,
`World::get` plus `World::get_mut`, was 1.0 to 1.1x ours at 100,000 rows and 1.3 to 1.9x slower than
ours at 1,000,000; bevy's headline uses `Query::get_mut`.

**d. Churn.** Churn alone (d0) costs ours 1.38 to 1.69 ns per row of the table and bevy 7.6: bevy is
4.5 to 5.5x slower. Per removal or insertion that is about 69 to 85 ns for ours against about 380 ns
for bevy at 1,000,000 rows, and about 19 against 66 to 80 ns at 100,000. With the stream after it
(d), bevy is 4.2 to 5.2x slower. Both backends swap-remove; spawn and despawn move no rows between
tables. bevy also keeps entity metadata and an archetype row list up to date, and its despawn path
checks every component of the row for hooks and observers. The columns have none of that.

**d2. Tag churn.** This is where bevy moves rows. Per tag change, ours costs 9.4 to 9.7 ns with a
side table and 13.7 to 15.5 ns with a flag bit (a random `get_mut` into the big table). bevy costs
459 to 468 ns with a table marker (the row's five components and their ticks move to another table),
about 49x our side table, and 154 to 179 ns with a sparse-set marker, 16 to 19x. At 100,000 rows the
ratios are 11.5 to 12x and 5.3x.

**e. Sparse filter.** bevy with a table marker iterates the tagged rows' own table densely: 0.045 to
0.047 ns per tagged row, 0.4 µs per pass over 9,887 tagged rows. Ours has no equivalent. A side table
of handles costs 5.2 to 5.5 ns per tagged row (51 to 55 µs per pass, one random lookup each), about
115 to 120x bevy. The branchless pass over `Flags` and `Hunger` together (needs the two-column
borrow) costs 5.5 to 5.9 ns per tagged row, which is 0.055 to 0.059 ns per row of the whole table. The
safe flag scan possible today costs 33 to 35 ns per tagged row (0.33 ms per pass). bevy's sparse-set
marker costs 1.2 to 1.35 ns per tagged row. Ours could iterate like bevy only by keeping tagged rows
in a table of their own, which means moving rows when tags change, as in d2; that was not measured.

**d2 and e together (derived).** At 1,000,000 rows, bevy's table marker saves about 51 to 54 µs per
pass over the tagged rows and costs about 456 to 459 ns more per tag change than our side table. It
comes out ahead while fewer than about 110 to 120 tags change per pass: a bit over 1% of the tagged
rows. At 100,000 rows the break-even is about 40 changes per pass, 4% of the tagged rows. Which side
of that line real kinds fall on depends on the systems, which do not exist yet.

**f. Wide rows.** Both backends store columns, so touching an 8-byte field of a 352-byte notable
streams 8 bytes per row in both. Ours takes 0.089 to 0.094 ns per row; bevy's iterator 0.112 to 0.120
(1.2 to 1.35x, change ticks again); bevy with ticks bypassed matches ours. The array-of-structs
reference, one 352-byte struct per row, takes 1.6 to 1.9 ns per row: 17 to 21x slower than either
column layout (7x at 100,000 rows). This is the structure-of-arrays benefit SCALE.md §9 relies on,
and both backends have it.

**g. Memory.** Ours costs 36.7 bytes per agent row: 35 bytes per slot (21 payload, a 6-byte id, an
8-byte sparse slot) times the `Vec` growth slack to 2^20 slots. bevy costs 110.2 bytes (spawn_batch)
to 114.3 (spawn): 3.0 to 3.1x ours. Per notable, ours 383.8 bytes and bevy 433.2 to 453.0, 1.13 to
1.18x. Both row kinds fit one model of bevy's cost: the payload, plus 8 bytes of change ticks per
component, plus about 49 bytes per entity (entity metadata, the table's entity list, the archetype's
row list): 21 + 5 x 8 + 49.2 = 110.2 and 352 + 4 x 8 + 49.2 = 433.2. So bevy's overhead is about 89
bytes per agent against our 14, and it matters most for small rows. At 100,000 rows ours shows more
growth slack (45.9 bytes per agent, capacity 131,072 for 100,000 rows) and bevy 116.6 to 143.0.

**h. Insert.** Building agent rows one at a time costs ours 16 to 19 ns per row at both sizes. bevy's
`spawn` costs 29 to 31 ns at 100,000 rows (1.8x) and 51 to 54 at 1,000,000 (3.0x). `spawn_batch`,
which reserves up front, costs 19 ns at 100,000 rows in three of four runs (30 in the other) and 32 to
45 at 1,000,000. For 352-byte notables the memory writes dominate: ours 101 to 139, bevy `spawn` 134
to 166, `spawn_batch` 95 to 133, a plain `Vec::push` 76 to 106 ns per row; ours and `spawn_batch` are
level. Insert numbers moved more between runs than any others: they allocate fresh memory, on a
machine with 11 GB in swap.

### In short

Measured here, our columns win on single-field writes (2 to 2.5x), churn (3.6 to 5.5x), tag changes
(11.5 to 49x against a table marker), insert (1.8 to 3x against `spawn`) and memory (3x for small
rows, 1.13 to 1.18x for notables). bevy wins on random access by handle (1.45 to 1.6x in cache, 1.04
to 1.33x out of cache), on iterating a rare tagged subset (70 to 120x), and on two-field streams given
today's `RowStore` API (1.1 to 1.2x), which a two-column borrow would turn into a 1.2x win for ours.
Most of bevy's extra cost comes from change ticks (8 bytes per component per row, one write per
touched row) and entity bookkeeping (about 49 bytes per entity, and work on every spawn and despawn).
The numbers lean towards our own columns for storage. The decision, which also weighs the executor
and everything below, is Ben's.

### Pinned against the full machine

The pinned run matches the full-machine run: 61 of 74 lines agree within 10% and 69 within 15%. The
two pinned runs agree on 67 of 74 lines within 10%. The bench uses one thread, so two cores give it
all it uses. The lines that differ most are the noisy ones: random access at 1,000,000 rows (up to
38%), `spawn_batch` (28 to 37%), bevy contig in f and the sparse marker in d2 at 1,000,000 rows (14 to
17%). Pinning will matter for the parallel step, which does not exist yet.

## What the facade showed

Three gaps in `RowStore` showed up as costs. They are API gaps, not storage gaps, and fixing them
would not change the layout. The kernel was not changed here.

- **No two-column borrow.** `column_mut` borrows the whole table, so streaming one column while
  reading another needs a copy (b: 1.3 to 1.5x the cost of the split) or `unsafe`. The flag scan in e
  needs the same workaround.
- **No multi-component get.** Reading one component and writing another of the same row takes two
  lookups (c).
- **Per-call column lookup.** Each `get`, `get_mut`, `column` or `column_mut` searches the column list
  by `TypeId` and downcasts through `dyn Any`. Negligible for streams; part of the per-lookup cost in c.

## Measurement notes

**Code layout.** The first build inlined each stream loop into its caller. In pattern a, ours ran
0.027 to 0.029 ns per row and bevy with ticks bypassed 0.022 to 0.023 over several runs, although
`objdump` showed the same instructions (two 16-byte loads, `paddusw`, two stores, 16 rows per pass).
Building our table after the bevy worlds instead of before, or adding one unrelated line of setup,
made ours 0.022. Forcing large allocations through mmap (a fixed glibc mmap threshold) put the
1,000,000-row buffers at the same page offset and did not change it, so it was code layout, not data
placement. The second build (`17fe4be`) put every slice loop in one shared out-of-line kernel. a and f
then agreed, but b's shared kernel ran at 0.14 to 0.16 ns per row in a test build and at 0.22 to 0.23
in the committed build, which differed from it only in a comment and a note string (runs 0936 and
0938). The third build (`6eebd58`) aligns every loop header to 64 bytes
(`bench/p4-storage/.cargo/config.toml`). A deliberately perturbed build (one extra function) then
agreed with the unperturbed one within about 10% on a, b and f, where the same kind of change had
moved b by 50% before. The primary runs use this build. Production builds will not carry this flag,
and in either backend tight loops can move by 25 to 50% from layout alone. Within one build, variants
that share a kernel are exactly comparable.

**Repeat and noise.** The pinned repeat agrees with the first pinned run within about 10% on most
lines. The full repeat (0950) ran while outside load rose to a 1-minute load of 13.9. Against the
primary full run, its lines were 27% slower at the median (half of them 19 to 37% slower), and 2.4 to
3.9x slower on random access at 1,000,000 rows, where bevy then led by 1.7x. The ratio between
backends moved by more than 20% on only six of 74 lines, all in c, d and d2. It is kept but not used
above. Run spreads were largest in c at
1,000,000 rows (up to ±47%) and in the inserts.

**Allocator.** A counting global allocator is installed for g; every variant pays its relaxed atomic
add per allocation, in every pattern.

## What these numbers do not settle

- **The executor.** No bevy `Schedule`, no multi-threaded executor, no executor of ours. Both P4
  options use our own executor, so this compares storage only.
- **Threads.** One thread. The potato profile matters for the parallel step, which is not built.
  The pinned cores also keep this laptop's clock speed and caches, so they do not model a slow CPU.
- **Real systems.** The rows and patterns are synthetic: one agent kind plus a tag, one notable kind,
  no mix of kinds in one pass.
- **Iteration order and determinism (PLAN.md §4.7).** Our dense order depends on the history of
  swap-removes; bevy iterates table by table. Neither order was checked against the determinism rules.
- **Saves.** Pages and the journal (PLAN.md §4.9) need columns written and read back; not measured.
- **The value of change detection.** bevy's tick cost buys "changed since the last run" queries. The
  bypass lines show its cost without them; whether the sim wants them is open.
- **Build cost.** bevy_ecs with only `std` adds 51 crates to the host build (68 packages against 17
  without it, from `cargo tree`). Compile time and binary size were not measured.
- **Scope of the setup.** One bevy version (0.20.0), one machine, the default x86-64 target (SSE2; a
  wider target such as AVX2 would speed up the streams of both), no Windows run (the other half of
  A11), and none of bevy's relations, hierarchies, observers or events.

## Reproduce

```sh
cd bench/p4-storage
flock /tmp/mic-rtsim-kernel2-heavy.lock nice -n 10 env CARGO_BUILD_JOBS=8 \
  nix develop -c cargo bench --bench p4_storage
# pinned: the binary cargo bench built, on two physical cores
flock /tmp/mic-rtsim-kernel2-heavy.lock nice -n 10 taskset -c 20,22 \
  env P4_PROFILE=pinned2 target/release/deps/p4_storage-<hash>
```

Run it from `bench/p4-storage`: cargo reads `.cargo/config.toml` (the loop alignment) only from the
working directory. The bench prints a warning, and the results record the rustflags, when the flag is
missing. Options, from the environment: `P4_TARGET_MS` (default 200), `P4_RUNS` (7), `P4_SIZES`
(`100000,1000000`), `P4_PROFILE` (`full`), `P4_WRITE=0` (print only), `P4_ONLY` (pattern ids such as
`a,b,g`). The crate is its own workspace, so the main workspace's `cargo test --workspace` never
builds bevy_ecs.
