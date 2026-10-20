# Overnight choices: persistence and routing (2026-10-20)

Reversible choices made on the night of 2026-10-20 while giving the phase 1 world a snapshot, the
dragons a save, and the dragons' boundary records a route through the native engine. One line each
with the reason, numbered S#. None is in DECISIONS.md; each can be undone or put to Ben. Numbers
marked PLACEHOLDER are not measured. Branch `overnight/persist`.

## The phase 1 world's snapshot

| # | Choice | Reason |
|---|---|---|
| S1 | **A snapshot of the world plus the inputs since it**, instead of the whole input log (V6, V7). The snapshot is everything the world holds (`World::save`, `World::load` in `crates/rtsim-world/src/persist.rs`, with codecs for settle, trade, travel, knowledge and the kernel's arenas, wheels and lazy table in each crate's own `persist` module): every table, wheel and index, private fields included, so a restored world continues bit for bit. Not written: the configuration, the goods and recipes (the caller's, size-checked) and the market trace. | A start replayed every input since the world began (review 3: "the phase 1 input log only grows"); the bench measured 4.3 s at 1,024 villages after 100 days against 8 ms for the snapshot (`bench/results/restore.md`). Hand-written codecs as the kernel's (`codec.rs`: no serde in the kernel), each in its crate because the fields are private. |
| S2 | Pages: table 0x5000 keeps the meta page (version 2) and the tail's chunks of 1,024 inputs; the snapshot goes to table 0x5001 in pieces of 1 MiB (PLACEHOLDER). The snapshot also carries the link's village index and censused villages as of the snapshot; everything else the link keeps (scripted villages, the kept outbox, the queued kept commands of review 3 P1, the caravan identity map of P2, epochs, the test offset) is in the meta page, written at every save. Pages a save leaves behind (an older snapshot's pieces, an older tail) are deleted in the same commit. | Pieces keep each page small for the store; the meta page stays small and is always current. |
| S3 | **When a save takes a new snapshot**: when the tail holds 1,024 inputs (PLACEHOLDER, one chunk), when the snapshot is one world day old (PLACEHOLDER: a start runs the world's time since its snapshot, inputs or not), or when the snapshot the tail follows never reached the disk (a failed or unfinished commit: the next save snapshots again rather than write a tail the disk cannot rebuild). Otherwise a save writes the meta page and the tail chunks not yet on disk. | Saves stay small between snapshots, and a start replays at most a day: one world day after the restore cost 10 ms at 1,024 villages (bench). |
| S4 | **The world changed outside the log** (the dragons: they share the world's knowledge, legs and markets, and their effects are not inputs) marks the link tainted (`Link::taint`), and the next save takes a snapshot. | A tail replays inputs only; with the dragons in the world, a tail replay alone would rebuild a world without their effects. A save is then always a full picture of the instant, consistent with the dragons' own pages written in the same commit. |
| S5 | **Phase 1 pages this build cannot read** (another build's format, a meta page of another version, a damaged or missing page) are dropped with a loud error log and the phase 1 world starts afresh; the core and the rest of the engine start as usual, and the next save deletes them. One function decides it: `when_pages_unreadable` in `crates/rtsim-jni/src/engine/phase1.rs`; returning `Err` there makes such pages stop the engine instead. | Open on Ben's sheet with this recommendation, matching industry's W23. Before release there is no migration (Ben's rule): version 1 pages (the input log) are dropped too. |
| S6 | At a save, commands the world decided that the link had not taken yet (an input applied mid-step, e.g. a census, before the next take) are taken into the queue first, so kept ones are in the meta page and the snapshot's own outbox is empty. A start still discards what its tail replay re-decides. | Found by the property test: the snapshot carried untaken commands, which a start then discarded. In play these were lost after a crash too. |
| S7 | The bench (`rtsim-cli bench restore`) joins the grid's villages one at a time and runs the days, as the bridge's log did, and compares that replay with the snapshot's save, restore and one more day. | It is the cost a start paid before and pays now, on the scale bench's world. |

## Tests

- Kernel: `Arena`, `TimingWheel` and `LazyTable` saved at any point continue exactly (property tests).
- Each crate's codec reads back equal (settle, trade, travel).
- `crates/rtsim-world/tests/persist.rs`: a world saved at any point of a random script (trades,
  robberies, rescues, takings, joins, players walking, bases, reports, commands taken) continues with
  the same state hash after every op and the same snapshot bytes at the end; a busy world with a
  caravan row and news on the road reads back byte for byte and runs 20 days identically; a
  truncated or foreign snapshot is an error, never a panic. A deliberate mutation (caravan leg
  generations off by one on load) fails both.
- `crates/rtsim-jni` `engine::phase1::tests::a_restored_phase1_world_matches_one_that_never_stopped`:
  random input scripts through the link (joins, censuses, trades, promotions, caravan trades,
  robberies, members taken or killed, reports, bases, time, saves and saves whose commit failed),
  restarted from the pages at a random point: the same state hash after every op as an engine that
  never stopped, the same snapshot bytes at the end, and both read back their own pages again.
  Passed 1,000 cases.
