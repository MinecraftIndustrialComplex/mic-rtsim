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

## The dragons' save and their route through the native engine

| # | Choice | Reason |
|---|---|---|
| S8 | **`Dragons::save` and `Dragons::load`** (`crates/rtsim-ecology/src/persist.rs`): every dragon, lair, event (in pop order), player view, home counter, ledger line, dictionary mapping and the commands not taken yet (with the reference codec and a fixed dictionary). Not written: the tunables (the caller's) and the CLI's diagnostics (`log`, `hunts`). The dragons' memories, grudges and flights live in the phase 1 world (its knowledge and leg index), which is saved beside them with `World::save`. | The task: health, memory and grudges survive a restart, with the state hash of an engine that never stopped (tested, `crates/rtsim-ecology/tests/persist.rs`). |
| S9 | **Dragon epochs come back as saved** (no stride after a restart, unlike phase 1's V26 and review 3's P6). | Their state hash includes every dragon's epoch, and a restore must equal an engine that never stopped. The journal's replay of a lost session reaches the engine in the first hand-overs, before any new materialization, so a replayed record cannot meet a reused epoch; a dragon record carried by a chunk into a later session (§3.1's case) is the plugin's to design: if it does, add a stride on load and compare hashes without epochs. |
| S10 | **Every adopted entity is remembered** (`Dragons::adopted`, saved; `by_entity` stays per session). An untagged body the engine adopted before is adopted again as the same dragon under a new epoch. | Review 3's open case: a spawn-egg dragon reported again after a crash (its DRAGON_ADOPT never reached a world save) was adopted a second time. |
| S11 | **Every dragon command is per session**; none is kept or resent. Sequences come from the core's counter and the commands are written after phase 1's. DISCARD has no op id (its COMMAND_RESULT carries op 0). | Puppets never outlive a session; an adoption or discard lost in a crash is decided again when the body rejoins the level and Java reports it untagged (`EntityJoinLevelEvent`). A kept DISCARD would add a wire change (an op id) for no case the re-report does not cover. |
| S12 | **Dragon pages this build cannot read, or without the world they were saved with** (the phase 1 pages dropped or absent), are dropped with a loud log and the dragons start afresh (`dragons::when_pages_unreadable`); the next save replaces them. | As S5 and W23. Dragons without their world would name actors and legs that do not exist. |
| S13 | **Order in a step:** the dragons run before the phase 1 world, and a phase 1 journaled input runs them to its time first (`dragons::catch_up`), so a dragon event never meets a world that ran past it. PLAYER positions feed the dragons only once there is a dragon or a lair; while there is one, the phase 1 link snapshots at every save (S4). | The ecology engine catches the world up to each of its events itself; it cannot go back. A pack without Ice and Fire's lairs keeps phase 1's cheap tail saves. |
| S14 | PLACEHOLDERS of the route: the dragons' pages are table 0x4400 (one page, with the state hash checked on load); they run under Java's ECOLOGY module bit (`1 << 4`); their seed is a constant (as phase 1's, V5); hoard item ids map to the phase 1 catalogue's goods by item name; DRAGON_ATE's prey bodies are not mapped to herd goods (the catalogue has no livestock good; P9 and P18's catalogue work); the industry engine is the dragons' threat interface only while the industry module runs. | Next to the industry and phase 1 tables; the rest follows what the bridge sends today. |

## Tests (dragons)

- `crates/rtsim-ecology/tests/persist.rs`: a wounded dragon's health, memories and grudge are the
  same after a restart, and it goes on hour by hour for 40 days (capture, hunts) with the state
  hashes and commands of one that never stopped; the scenario's dragons restored at a random hour
  of a month continue exactly with a player walking past their lairs (property test); a truncated
  page is an error; an untagged dragon reported again after a restart is adopted once. Two
  mutations (home weights, fed_at) fail them.
- `crates/rtsim-jni/tests/protocol.rs` `dragons::dragons_are_adopted_hurt_captured_materialized_and_replayed_across_a_crash`:
  through the native engine and a fake Ice and Fire plugin (`tests/common`): a lair and its
  structure dragon adopted; a player's wound is a grudge; the body captured with its health; a
  player at the lair materializes it again under a new epoch; a natural spawn left alone (its spawn-egg sibling adopted); a save; a hoard robbery and a spawn-egg dragon after it; a crash. The pages read back
  to their saved state hash (and the world's), and the journal's replay books the robbery once,
  adopts the egg dragon once, and leaves every dragon's health and grudge as before the crash; the
  same robbery's op id again is applied once.

## Results

- **Rust:** `cargo test --workspace` 331 passed, 0 failed (313 before this branch);
  `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --all --check` clean.
  The phase 1 restore property test also passed at 1,000 cases, the world-level one at 1,000.
- **GameTests, native engine** (nothing on the Java side changed; run because the native engine's
  saves did): core 37 of 37 on a fresh GameTest world, then 37 of 37 again on the same world, whose
  start read the first run's snapshot and replayed a tail of 1 input ("phase 1 world restored to
  world tick 102984: snapshot of 6075 bytes at tick 102528 ..., then 1 inputs replayed"); ribbits 5
  of 5, carryon 1 of 1. The stub engine has no Rust in it and was not rerun.
- **Restore time:** `bench/results/restore.md` (8.2 ms at 1,024 villages after 100 days against
  4,251 ms replaying the log; 35.3 ms against 23,160 ms at 4,096).

## Incomplete

- **The kernel follow-up** (`overnight/kernel2`: paging under a memory budget, two save
  generations for P29, an incremental manifest) had not merged into origin/main when this branch
  finished, so phase 1, the industry engine and the dragons still use the current `PageStore`.
  Moving them is a matter of the keys: phase 1's meta page and tail chunks (table 0x5000) and its
  snapshot pieces (0x5001), the dragons' page (0x4400) and the industry tables (0x4900 to 0x4902)
  are whole pages written by one commit, so they map onto kernel2's `commit_pending` and
  `confirm_through` as the core's pages do. `TimingWheel::iter` here is kernel2's, copied
  verbatim, so that hunk merges clean; `in_pop_order` and the codecs sit after it.
- **The Ice and Fire plugin** (`mic_rtsim_iceandfire`) is not written: nothing in Java sends the
  dragons' records yet. Its executable reference is the fake plugin in
  `crates/rtsim-jni/tests/common` (`dragon_obs`, `dragon_command`); docs/boundary.md §6.5 and §6.6
  say what it does in game.
- **Not measured:** a save's snapshot cost with dragons present at every save (S4: about 4.5 ms
  of encoding and 3.7 ms of LZ4 at 1,024 villages, from the restore bench, plus the dragons'
  page).
- **Review 3's other open items** (P3, bands' members, `World::visible` over bands, PreyIndex
  rebuilds) are untouched.
