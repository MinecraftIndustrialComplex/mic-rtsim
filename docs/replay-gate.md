# The replay gate (fix/replay-push)

The design for one rule across conflict, dragons, herds and ships: a session's journal replay decides
nothing that reaches Java's bodies, and every replayed record of a body counts against that body as
the last session had it. The audit behind it (five LEAKs, each a failing scratch test) is summarised
in §9's test lists. Choices: RP-1 onward, in docs/choices-mowzie-build.md, section "Replay pushes
(fix/replay-push)". Slices: §9.

## 1. The rule

From SESSION_START until the replay end (§3.2):

1. **Nothing embodies.** No module decides a MATERIALIZE, an envoy, an adoption or any other command
   that makes or moves a body in Java. Sightings wait for the replay end (§6).
2. **Every command goes through one gate per engine** (§4): an exhaustive match with no `_` arm says,
   for each kind, whether it is sent, dropped, or never decided while replaying (a bug: skipped and
   counted in release, a `debug_assert` in tests).
3. **Replayed records find their bodies** (§5). The session start no longer makes anything abstract:
   the embodied state of the pages is kept, a replayed *_MATERIALIZED is honoured, and so the deaths,
   damage and despawns the last session journaled after its pages count against the bodies they
   name. At the replay end, what is still embodied becomes abstract (today's session start, moved).
4. **No epoch is used twice** (§5.3).
5. **The clock follows the records** (§3.6, RP-24). While replaying, the shared clock (the four
   modules and the phase 1 world) runs only as far as the latest replayed record; the rest of the
   hand-over's time runs after the replay end, so no event of the window fires before a record of it.
6. **A war party's arrival waits** (§6.1, RP-23). A raid, invasion or siege that reaches its target
   while replaying, or a siege whose wait ends, neither fights nor resolves until the replay end,
   unless a replayed BAND_MATERIALIZED shows that the last session embodied it.

### 1.1 Definitions (SEC-1's, binding)

- `session_first = max(world_next, pages + 1)`, set at SESSION_START (rtsim-jni engine.rs). With
  S4p, the session's own JOURNAL_GEN sits at `session_first`.
- A record is **replayed** when its own OBS_SEQ is below `session_first`. For a copy (S4p's
  REASSERT) that is the copy's own sequence, which S4p's `reasserted()` writes into the inner record
  before dispatching it; the original's sequence plays no part.
- The engine is **replaying** while the next record it admits is below `session_first`, that is
  `applied_through + 1 < session_first`. RP keeps one bool, `st.replaying`, holding this predicate's
  answer wherever a module reads it, and runs the replay end once, when the answer turns false. It is
  the same definition, latched, not a second one (RP-1).

A record dispatched while `st.replaying` is true is a replayed record, and a record dispatched after
the replay end is not: the replay end runs before the first record at or past `session_first` is
admitted (§3.2). So an engine can read "this record is replayed" from its own `replaying` flag.

## 2. Where this meets S4p and RG14

S4p: feat/sec1-s4p (its code at 3bff09e5 and a003bfe6; 1c1b674f merges S3, S4b and main 3391a5d9).
SEC-1 reviewed this section (its reply is folded in below).
RG: feat/reinstall-guard (RG13, RG14). Neither S4p nor RP had to land first; whichever landed second
did the merge in §2.7 (RP-17). They meet in four places: the session start, the replay end, the copies
(REASSERT) and the epochs. RG14 meets RP nowhere in code.

**What landed.** RG and S4p are both on main (S4p's merged head f361a6f1), so RP lands second. This
branch merged main at 9b942f61 and wrote §2.5's tests at 96ca2219. §2.1 to §2.3, §2.5 to §2.7 are
checked against what landed and amended where main differs from what they assumed; the amendments say
"on main".

### 2.1 Session start

- **Function:** the SESSION_START arm of the observation reader (rtsim-jni engine.rs, the arm that
  calls every module's `session_start`, then `Replay::reconcile`, then sets `session_first`).
- **S4p** adds `st.session_gen_seen = false;` right after `st.session_first = ...`.
- **RP** adds three things to the same arm:
  1. `st.replaying = true;` and `st.applying_copy = None;` first, before any module's
     `session_start`, so that a module reset inside the arm never sees the last session's `false`
     and ends a replay that has not started, and a step that panicked inside a copy's dispatch
     (caught, so the field stayed `Some`) marks no record of this session a copy (RP-8). The replay
     clock starts at the world's present (§3.6).
  2. `shared::session_start` forgets every player's position and area (`World::forget_player_pos`)
     and calls no module's `player_left` (RP-26). Today it calls `leave`, whose `player_left`s recall
     conflict's watched bands and despawn herds' groups: bodies that RP-5 keeps until the replay end.
     Each module's `replay_done` does that work instead (§5.6).
  3. At the arm's end, after the `session_first` line: `shared::replay_end_at_session_start(st, jf)`,
     which reads the predicate with the new `session_first`. With nothing to replay, the replay ends
     here. At the arm only, "nothing to replay" also counts Java's journal: the replay ends when
     `max(applied_through + 1, jf) >= session_first`, where `jf` is the arm's `journal_first` (0
     read as `world_next`, as the arm already does) (RP-29). Before S4p a journal trimmed to start at
     or past `session_first` sent nothing below it, and the replay would otherwise stay on until the
     first live record, however long that takes. With S4p the session's JOURNAL_GEN ends it at once
     anyway; the pinned generation (fact 4 below) can sit below `jf` and come after this end, which is
     harmless, since a JOURNAL_GEN is never dispatched to a module.
- **Overlap:** RP's last line and S4p's come after the same `session_first` line; no shared state.
  Keep both.
- **On main (checked at the merge):** the arm sets `session_first`, then S4p's
  `session_gen_seen = false`, then RP's session-start end, then (still replaying) the hand-over
  count's 1 (§2.2). The pinned JOURNAL_GEN sits below `session_first`, so it is never the session's
  own: pages that include it skip it (`stale_skipped` + 1), older pages admit it through a gap, and
  neither dispatches it or touches `replaying`. RP-29's end at the arm, before it comes, is harmless:
  `the_replay_ends_right_before_each_sessions_journal_gen` (tests/reassert.rs) runs three sessions
  and counts one end, one stale skip and exactly the replayed tail in each.
- **SEC-1 fact 4 (pinnedGen):** S4p keeps the newest acknowledged JOURNAL_GEN outside the trimmed
  entries (SC130: `Journal.pinnedGen` in Java, `pinned_gen` in rtsim-host) and sends it first at each
  session start. Pages that include it skip it (`stale_skipped` + 1 a session) and `applied_through`
  does not move. Pages older than it admit it as a gap (`skip_to`, then `admit_gen`), which moves
  `applied_through`; its sequence is below `session_first` either way, so the replay stays on. With
  the engine ahead of the journal (S4p's EngineAhead), the predicate is false at the arm's end and the
  replay ends there, which is right. Nothing in RP reads the first replayed record's content or
  assumes it carries data.

### 2.2 The replay end (MZB-141)

- **Today** it is conflict's alone, two triggers in rtsim-jni engine/conflict.rs: `journaled` on the
  first conflict record at or past `session_first` (about L530), and `simulate` once
  `applied_through + 1 >= session_first` (about L641). S4p does not touch that file. On S4p alone both
  still work: JOURNAL_GEN is never dispatched to conflict, so the first trigger fires on the first
  live conflict record, and the second at the step.
- **RP** moves both into one transition for every module, `shared::replay_end(st)`, reached at
  three points (RP-2), each through its own guard of the one predicate (`replay_end_before`,
  `replay_end_if_due`, `replay_end_at_session_start`):
  1. **`journaled()`** (engine.rs), first thing, before its `save_through` block (RP-42), for a
     record whose OBS_SEQ is at or past `session_first`. So a save due at that record is taken after
     the replay end (§3.2), and the end is before S4p's JOURNAL_GEN branch, its unnamed generation
     branch, its REASSERT branch and `admitted()`. On S4p the first live record is the
     session's JOURNAL_GEN, so the replay ends just before that record is admitted. On main it is the
     first live record of any kind. Every module's journaled entry is reached through this function,
     industry's included, so the end also runs before an industry record that calls into another
     engine (M7's TRAIN_ARRIVED and TRAIN_UNWATCHED, §4.4).
  2. **The step's top** (`step_inner`, after the observations are read and before
     `industry::simulate`), when `applied_through + 1 >= session_first`: a replay whose last record
     was the hand-over's last.
  3. **The SESSION_START arm's end** (§2.1), with RP-29's journal check.
- **The hand-over count** (RP-2's amendment) rises at the step's entry, before trigger 2, whenever
  `st.replaying` is true there, so a replay that ends inside the first hand-over after SESSION_START
  counts 1, and one that ends at the arm counts 0.
- **Why the top of `journaled()`, not inside S4p's `admitted()`** (RP-20): the end must run after the
  last replayed record is dispatched and before the first live one is admitted. The top of
  `journaled()` sees every kind (JOURNAL_GEN, REASSERT, the rest), reads only the record's sequence,
  and exists on both branches. Inside `admitted()` it would also be correct but would need the
  sequence passed in and three call sites checked.
- **A gap** (a journal trimmed past the pages, `Verdict::Gap` and `skip_to` on both branches): the
  records below `session_first` that never come cannot hold the replay open. The first record that
  does come, at or past `session_first`, ends it; one that comes below it is replayed as usual.
- **On main: one record, three switches.** S4p's own session test, `seq >= session_first` (in
  `journal_gen()` and in the unnamed generation branch), is the predicate `replay_end_before` reads.
  So the replay end, S4p's session switch and the generation switch all happen at one record, in
  that order: the session's JOURNAL_GEN (its sequence is `session_first`) ends the replay, then
  `admit_gen` closes the replayed generation at its last sequence and opens the session's. With a
  bridge that names no generation, the first live record of any kind ends the replay, then opens
  `UNNAMED_GEN | session_first`, then is admitted. Every record dispatched while replaying therefore
  belongs to a generation of an earlier session (the pages' open one or one a replayed JOURNAL_GEN
  opened), and every record dispatched after the end to this session's.
- **On main: the replay clock agrees.** While replaying the clock runs no further than the replay
  clock (§3.6), which each replayed record raises to its GAME_TIME. A copy is dispatched as its inner
  record, whose GAME_TIME is the original's (Java journals a REASSERT with the inner record's time),
  so a replayed copy raises the clock no further than its original, never past it: the clock can
  only lag, which holds an event back and never fires one early. The JOURNAL_GEN is never dispatched
  to a module, so it raises nothing; the end it triggers lets the next run of the clock reach the
  hand-over's time, after every replayed record and at or before the first live one's.

### 2.3 Copies (REASSERT)

- **Function:** S4p's `reasserted()`. A copy is admitted by its own sequence, judged by
  `copy_is_new(gen, of)`, and when new is dispatched as its inner record, with OBS_SEQ rewritten to
  the copy's sequence.
- **SEC-1 fact 3's two cases, under RP:**
  1. A REASSERT admitted at or past `session_first` (a chunk or entity loaded in this session) is not
     a replay record. The replay end has run before it: on main at the session's JOURNAL_GEN, which
     comes first; with a bridge that names no generation, on the REASSERT itself when it is the
     session's first live record (§2.2 trigger 1).
  2. A REASSERT journaled in an earlier session after its pages, and replayed now (below
     `session_first`), is a replay record, dispatched while replaying.
- **Told apart by generation (on main).** A replayed REASSERT is admitted while an earlier
  session's generation is open; its original is of that generation or an older one, judged against
  `open.last` or the closed generation's `last_of`. A live REASSERT is admitted after this session's
  generation opened (§2.2, one record, three switches), so the open generation is this session's
  and the copy's original, carried from disk, is of a closed or unknown generation, or of this
  session's own (a holder unloaded and loaded again), which is dropped. The one `copy_is_new` judges
  both; `st.replaying` already says which case it is, and the generation says only whether the copy
  is new. A copy of the open generation's own record is dropped in both cases.
  A copy journaled after the pages and replayed counts once, in the replay
  (`a_copy_journaled_after_the_pages_is_counted_by_the_replay`); a live copy of an original the
  replay applied counts nothing (`a_copy_of_casualties_the_replay_counted_counts_nothing`).
- **Copies never match a body (RP-8).** In both cases the content is an earlier session's (S4p drops
  copies of the open generation), so its body is gone. A copied *_MATERIALIZED is never honoured: it
  only raises the agent's epoch floor (§5.3). A copied loss is applied by identity (the party and the
  body kind, the herd member's key, the resident), never against an embodied epoch or a recalled
  stack.
- **The copy bit:** RP adds `st.applying_copy` to rtsim-jni's State, set by `reasserted()` just
  before its `self.dispatch(st, &r, seq, verdict)` and cleared just after, and read by the modules'
  `journaled`. A field, not a parameter, so `dispatch()` and every module's `journaled()` keep S4p's
  signatures (RP-8). It holds the copy's generation nonce, `Option<u64>`, `Some` only on the apply
  path: a copy judged not new goes to `answered_again` and is never dispatched. A copy of the open
  generation's own record is always dropped while replaying (`reasserted()` admits the REASSERT's own
  sequence first, which moves `open.last` past the copy's `of`). On main the merge (9b942f61) sets
  it around `reasserted()`'s dispatch; without it five of §2.5's tests fail (RP-43).
- **Carried records the rule meets:** conflict BAND_CASUALTIES and CHIEF_KILLED (ConflictBridge
  `settleDeaths`, a dying member's drops; `gone`, a taken body); herds HERD_KILLED and HERD_TAKEN
  (HerdBridge); phase 1 TAKEN, DIED and MEMBER_STATE (RtsimSession; not an RP module); COMMAND_RESULT
  (by op id). Dragons and ships carry none.
- **The conflict copy gap, plainly.** S4p assumes a new copy applies as its record. For conflict's
  losses it does not, on main or on S4p. `casualties()` (rtsim-conflict engine.rs about L5161) finds
  the party with `party_of`, which wants the party's current epoch exactly, and then drops a casualty
  of a party that is not embodied and holds no recalled stack of that epoch (MZB-138's straggler
  rule). A new copy names the epoch of a body of an earlier session; when it comes, the party is
  abstract with its stacks dropped, or embodied at a newer epoch. Either way the death is dropped and
  the member lives on. CHIEF_KILLED goes by the party and is applied; herds' losses go by member key
  and op id and are applied. RP-9 closes the gap: a copied BAND_CASUALTIES counts against the party's
  alive units of that body, found by the party alone (any epoch), and takes from no stack; each stack
  of that body is then capped at the units left alive (RP-25, §5.7). S4p's
  judgement is what makes this safe: for a named generation, with the engine's state a prefix (pages
  plus a contiguous replay), a copy is new only when its original was never applied, and then nothing
  journaled after the original was applied either (Java's journal survives a crash as a prefix), so
  no despawn of that body counted its survivors. SEC-1 names two exceptions:
  1. **Generation 0.** `copy_is_new` answers new at once for nonce 0 and `admit_copy` records no
     `(0, of)`, so repeated copies of one record each apply unless an op id stops them. Java writes
     generation 0 for an EpochEntry without one and for any record kept from before generations.
     BAND_CASUALTIES has no op id, so a copied BAND_CASUALTIES of generation 0 is refused and counted,
     never applied (RP-21): a member may live on; none dies twice. CHIEF_KILLED has an op id; herds'
     losses go by member key and op id.
  2. **A gap** (pages older than the journal's first entry, such as a restored backup): the state is
     not a prefix, a copy from the trimmed stretch can be judged new while later sessions' records (a
     despawn that counted survivors) were applied from the replay, and the death may count twice.
     That is docs/boundary.md §3.2's "Known limit", accepted as such; S4p's
     `copies_against_pages_restored_from_an_older_backup` (tests/reassert.rs) shows the verdicts.
- **Found on the way:** the same `party_of` also drops a live walk-off death of a band's old epoch
  while the band is embodied again at a newer one (MZB-131's own case). Slice C tests it first and
  fixes it as RP-27 says (§5.9): the party by agent, then the stack by the record's epoch. Finding by
  agent alone, with no epoch, would take an old epoch's death off the new epoch's stack and leave the
  old epoch's recalled stack whole, so the old epoch's despawn would count the same death again.

### 2.4 Generations and epochs

- S4p's generations judge copies; RP's epochs judge bodies. Neither reads the other. UNNAMED_GEN and
  the generation table change nothing in RP.
- **RP's epoch rule (RP-7):** an agent's next epoch is above every epoch the engine has seen for it:
  in its pages, in a replayed *_MATERIALIZED, and in a copy. Today a replayed MATERIALIZED is ignored
  (conflict `ingest` about L4910, herds about L2158), so an epoch decided after the pages comes again
  after a crash; the herds audit test shows it (epoch 1 live, and 1 again after the restart).
- A lost tail (Java's journal not saved before a crash) can still hide an epoch from the engine. Only
  a copy can name it afterwards, and the copy never matches the new body (RP-8). So after RP no record
  of one body is applied to another.

### 2.5 Tests that need both

Written by whichever lands second (§2.7), each with its control. On main (96ca2219, RP-43): 1, 2, 3
and 5 in rtsim-jni tests/protocol.rs's `conflict` module, where the camp and band helpers live; 4 in
tests/reassert.rs beside S4p's.

1. **Fact 3, case 1:** a live REASSERT of a BAND_CASUALTIES after the replay end (its original lost
   with the journal tail) counts once; a second copy of the same original is dropped
   (`copies_dropped` + 1) and counts nothing. Control: the original in the replayed tail and its copy
   later: counted once, by the original (the copy is not new).
   `a_live_copy_of_casualties_a_crash_lost_counts_once`,
   `a_copy_of_casualties_the_replay_counted_counts_nothing`.
2. **Fact 3, case 2:** a REASSERT journaled in session 1 after the pages and replayed in session 2 is
   counted while replaying, before the replay end; the party's units fall once.
   `a_copy_journaled_after_the_pages_is_counted_by_the_replay`.
3. A REASSERT holding a BAND_MATERIALIZED, live and replayed: it embodies nothing, and the party's
   next epoch is above it. `a_live_copy_of_a_band_materialized_embodies_nothing_and_raises_the_epoch`
   and its replayed twin.
4. **pinnedGen:** two restarts in a row; `stale_skipped` rises by one a session, and the replay end
   comes right before each session's JOURNAL_GEN.
   `the_replay_ends_right_before_each_sessions_journal_gen`. It shows the end at that record or the
   step's top's end, not which side of the JOURNAL_GEN: a JOURNAL_GEN is never dispatched, so an end
   just after it reads the same (RP-43).
5. **Generation 0 (RP-21):** two REASSERTs of one BAND_CASUALTIES with nonce 0, live: neither counts,
   and the refusal count rises by two. Control: the same copies with a named, unknown generation:
   the first counts once, the second is dropped. `copies_of_generation_0_casualties_are_refused`,
   `copies_of_casualties_of_an_unknown_generation_count_once`.

### 2.6 RG14

- RG14 drops old-life pending records in Java before they are journaled (SEC-1 fact 5), so the
  engine never sees them. RG touches Java and docs only; RP touches Java only in slice N (Nests and
  DragonBridge's NEST_RENEW). No shared function.
- Old-life records still in Java's journal (the old engine's unacknowledged tail, RG5's open case)
  reach a fresh engine below `session_first`, so RP treats them as replay: nothing decided from them
  reaches Java; a *_MATERIALIZED for an agent the fresh engine does not have is ignored, and one for
  an agent it has (a reused handle) only raises that agent's epoch floor. RP neither closes nor widens
  RG5.
- **Against §5.2 (RP-31):** §5.2 honours a replayed *_MATERIALIZED above the agent's epoch, and a
  reused handle's old-life record is one, so the two rules meet. The conservative answer: an engine
  that did not start from pages (a first start, a reinstall, a module whose engine was built fresh)
  honours none; `begin_replay(from_pages: bool)` tells it, and every replayed *_MATERIALIZED only
  raises the floor (§5.3). Loses: a fresh engine's own records journaled before its first save are
  not honoured after a crash before that save, which is today's behaviour for every engine. Once RG13
  lands, the engine life could tell the two apart; RP does not wait for it.
- **Slice N meets RG13's engine life:** NEST_RENEW's renewal serial (RP-14) starts again with a fresh
  engine, so Java keeps the engine life beside each nest's last serial and starts a nest's serial
  afresh under a new life (RP-19).
- **On main (checked at the merge):** RG13's engine life is Java's alone (`ReinstallRules.engineLife`,
  kept by RtsimSession beside the page directory) and never reaches the engine, so
  `begin_replay(from_pages)` stays the engine's own answer and RP-31 stands as written. RG14 drops a
  holder's pending records of another life unjournaled (RtsimSession, `stalePending`), as assumed
  above: the engine sees neither them nor a copy of them.

### 2.7 The merge, whichever lands second

Done by this branch at 9b942f61 (RP landed second). The one textual conflict was engine.rs (items 1
and 2, and both sides' fields in Stats, State and `State::new`); the rest merged clean. Item 3 went in
with the merge; RP's replayed-record count moved into `dispatch()`, so a copy applied while replaying
counts as a replayed record. Item 5 is 96ca2219. Item 8: main's fixtures were taken at the merge;
this branch regenerates them, and deletes the worktree's run-gametest worlds, before it lands.

1. **engine.rs, SESSION_START arm:** keep S4p's `session_gen_seen = false` and RP's lines (§2.1).
2. **engine.rs, `journaled()`:** S4p restructured its body (JOURNAL_GEN, unnamed generation and
   REASSERT branches, `admitted()`, `dispatch()`, `answered_again()`); RP adds two lines at its top. A
   textual conflict: take S4p's body and put RP's check right after the `save_through` block. (The
   fix round moved it above that block, RP-42.)
3. **`reasserted()`:** set `st.applying_copy = Some(nonce)` before its dispatch and `None` after,
   one line each side. If RP lands first, S4p's merge adds them; if S4p lands first, slice C does.
4. **tests/protocol.rs:** S4p shifts the journal's sequences by one (the JOURNAL_GEN) and
   `stale_skipped` by one a session. RP's jni tests count records against `session_first`, never by
   absolute sequence, so they hold on both; the merger reruns them.
5. **§2.5's tests:** written by the merger.
6. **docs/boundary.md:** S4p edits §3.2; RP edits §2.3 and §5 (items 4 and 6). Different sections.
7. **S4p's other files**, which RP does not touch: neoforge core Journal.java, JournalSavedData.java
   and engine/StubEngine.java (the pin, the op-id multiset, magic RTS2, the unnamed generation);
   RtsimSession.java, PendingRecords.java, BoundarySession.java, Obs.java, ScriptedEngine.java,
   RtsimCommands.java, Industry.java, IndustryAttachments.java, Reconciler.java and the GameTests
   PendingRecordGameTests, RtsimTests, WorldGameTests and create's ReconcileAheadGameTests;
   BoundaryProtocolTest.java; rtsim-host java.rs, run.rs and save.rs (`pinned_gen`, NBT
   "pinnedGen"); rtsim-kernel journal.rs; rtsim-jni wire.rs; docs/boundary.md §3 and §3.2.
   tests/reassert.rs has S4p's 9 tests; §2.5's go beside them. The branch at a003bfe6 also carries
   S3 and S4b (merged at 1c1b674f), which bring rtsim-jni engine/industry.rs and tests/industry.rs
   and the rtsim-industry crate; RP touches none of those either.
8. **Fixtures:** S4p changes the page format (META v2, table 2 RTJG), so whoever lands second
   regenerates the rtsim-view fixtures after the merge (and deletes the worktree's run-gametest
   worlds), even if RP's own slice changed no format.
9. Nothing else: S4p touches no module file (engine/conflict.rs, dragons.rs, herds.rs, ships.rs,
   shared.rs, phase1.rs, worldmodel.rs), and RP touches no kernel journal code and no industry file.
   Both touch engine.rs (items 1 and 2) and tests/protocol.rs (item 4).

## 3. The flag

### 3.1 Where it is set and cleared (rtsim-jni)

- `State.replaying: bool`, not saved.
- **Set** true at the top of the SESSION_START arm (§2.1). Before the first SESSION_START (`start`
  to the first hand-over) it is false: no module has reset yet, and the first SESSION_START sets it.
- **Cleared** only by the replay end, `shared::replay_end` (§3.2), which runs once a session: at the
  arm's own end when there is nothing to replay, else at the first trigger that finds
  `applied_through + 1 >= session_first`.
- A module switched on in the middle of a session (MODULES, U10) resets as today
  (`reset_due`, `reset_if_due`); its replay end runs at once when `st.replaying` is false, else at the
  session's replay end.

### 3.2 The replay end (rtsim-jni engine/shared.rs)

- `shared::replay_end_if_due(st)`: when `st.replaying` and `applied_through + 1 >= session_first`,
  run `replay_end(st)`. Called at the step's top and at the SESSION_START arm's end.
- `shared::replay_end_before(st, seq)`: when `st.replaying` and `seq >= session_first`, run
  `replay_end(st)`. Called at the top of `journaled()`.
- `replay_end(st)`: `st.replaying = false`; one info log line (the observations replayed, the
  hand-overs the replay took, §2.2, and each module's commands dropped and skipped while replaying,
  §4.3), the same counts in the JNI stats; then each module's `replay_end(st)` in the step's order
  (dragons, ships, herds, conflict). Each module's `replay_done` runs at the replay clock (§3.6); the
  held part of the hand-over's time runs after it, at the next run of the clock.
- A module's `replay_end(st)`: when its `replay_end_due` is set and it runs (`running(st)`): clear
  the flag, call its engine's `replay_done`, take its commands. Conflict then re-sends its kept
  notices (today's `resend_notices_if_due` body). A module that is switched off keeps the flag; its
  reset on switch-on runs both.
- A module's reset (`reset_if_due`) calls its engine's `begin_replay(from_pages)` (RP-31: whether
  that module's engine was restored from pages) and sets `replay_end_due`.
- The two MZB-141 triggers in engine/conflict.rs and `notices_due` go (RP-2).
- **No page is taken while replaying** (RP-42). Pages saved mid-replay would hold what only the
  replay end undoes: bodies still embodied, kept fights still Fighting, held parties' `seg` one
  short, floors and `fill_deaths` unfolded. So every path that saves waits while `st.replaying`:
  - `journaled()`'s `save_through` block, which runs after trigger 1, so a save due at the first
    live record is taken after the end; a request still due while replaying is taken at the first
    journaled record or step end after it;
  - the step end's (`SaveAtNextStep`, and a `save_through` that `applied_through` has reached);
  - the budget's (`paging::enforce_budget`);
  - `close(save = true)`, which leaves the pages as they were; the next start replays what they
    lack from Java's journal.
- **The cost** is the budget: the resident rows may pass it by what the replay's hand-overs change,
  until the first budget save after the end. Measured in rtsim-jni's
  `a_long_replay_takes_no_budget_save_before_its_end` (200 factions and 4,000 filler records
  replayed in 5 hand-overs under a 4,096 B row budget): 56,792 B at the peak while replaying, 2,680 B
  after the end's budget save.

### 3.3 How engines see it

A field, not a parameter (RP-3). Each engine's outbox (§4.1) holds `replaying`; `begin_replay()` sets
it and `replay_done()` clears it, so the gate and the engine's own early returns (§6) read one place.
Not saved: a session sets it.

### 3.4 rtsim-host, rtsim-cli and crate tests

- **rtsim-host** drives rtsim-jni's `Engine` through its fake Java's SESSION_START, so it gets the
  flag as the game does. Nothing to add.
- **rtsim-cli** (`sim conflict`, `sim mowzie`, benches) drives engines directly and has no sessions:
  an engine is built with `replaying` false and never gates.
- **Each engine's `session_start` goes** (RP-18), replaced by `begin_replay` and `replay_done`, so
  that every caller fails to compile and chooses: a test that restarts an engine calls
  `begin_replay(true)`, feeds whatever replayed records it wants, then `replay_done()`. The callers
  today: rtsim-conflict tests/mowzie_records.rs; rtsim-ecology tests/herds.rs, persist.rs and
  tribute.rs; rtsim-ships tests/blockade.rs, lifecycle.rs, paged.rs and port_raids.rs; rtsim-jni's
  module resets. (`FakeConflict::session_start` in rtsim-conflict synth.rs is the emulated Java, not
  the engine.)

### 3.5 Cost (D1)

When not replaying: one bool test per journaled record and one per step, and one per place that
reads the clock's target (§3.6). While replaying: less work than today (no routing, no sightings, the
clock held), plus one `min` per clock target and one poll event per held war party every
`REPLAY_HOLD_TICKS` of replayed time (§6.1). At the replay end: one pass over what is embodied, which
is today's session start, moved, and the held window's events, which today run in the first
hand-over instead.

### 3.6 The clock while replaying (RP-24)

**The fault.** A step runs the shared clock to the hand-over's game time, and after a crash that is
Java's present, at or past the last replayed record. So in the first replayed hand-over every event of
the whole window fires (a fight's end, a march's next leg, a party home and pooled by `home()`) before
the later hand-overs' records of that window arrive. Their records then meet a world that moved on:
a casualty finds its party pooled and is dropped. The audit's long-replay test missed it because its
3,000-block march held no event inside the window. The clamps `max(record, conflict.now,
phase1.world.now)` (rtsim-jni engine/conflict.rs about L575 and L619, and dragons', herds' and ships'
twins) then carry every later record forward to that present.

**The coupling check (the reviewer's question).** What ties the four modules to the clock that runs
ahead:

- **One clock for five.** The four modules live in the phase 1 world and run its clock (phase1.rs's
  `simulate` comment). Each module calls `w.run_until(at)` before each event, and the world never
  steps back: ships read a time below `w.now()` as missed (`world_at`, `missed`; PC103 found a market
  stepped before its anchor), and conflict, dragons and herds clamp to it. So holding the four alone
  is not possible: `phase1::simulate` runs the world itself to the hand-over's time
  (`world.run_until(wt(game_time))`), and the next record's clamp would carry every module there.
  Dropping `phase1.world.now()` from the clamps, as the reviewer suggested, would let an event run in
  the world's past. The world is held instead, with the four.
- **`shared::run_to`** runs all four modules in rounds; a call from any one module's `simulate`
  (dragons' runs first) runs conflict too. So the hold belongs in one place below them all.
- **worldmodel** founds herds with `herds::found(st, n, world_tick(game_time))`, which runs the
  clock to the hand-over's time. It founds nothing while replaying: the regions stay pending (in
  memory, as today between steps) and are founded after the replay end.
- **Industry** keeps its own clock (`industry::simulate` runs first in the step,
  `advance_to(tick(game_time))`) and its own replay rules; it is not held. The modules meet it in two
  ways. Explicit-time lookups, `trains_crossed(frame, regions, from, to)` over industry's legs:
  conflict's ambush at a segment's start (rtsim-conflict engine.rs about L2943), dragons'
  `train_scan` (rtsim-ecology engine.rs about L1596) and ships' `coast_raid` (rtsim-ships engine.rs
  about L3493). While held, a module asks about a window that industry has already passed, and a leg
  industry closed since may be missing, so a replayed ambush or scan can miss a train the live one
  found (lose, never double). And raids, which industry applies at its own present
  (`threat_with` takes `t.at.max(self.now)`), so a held module's raid books on industry's later
  state. Conflict's district raids come from `resolve`, which §6.1 holds; its train ambushes
  (`Ev::Ambush`, `raid_train`) still run in the replayed window, as today, and are abstract (no
  body). Dragons' and pirates' raids are slices D's and S's check. **Decided by the coordinator,
  2026-10-24:** industry is not held during the replay, a known limit (holding it would put RP into
  industry's own replay); the long-replay tests run with industry on and off.
- **Rail and trains:** M7's TRAIN_ARRIVED and TRAIN_UNWATCHED reach conflict through
  `industry::journaled` after `shared::catch_up` to the record's time, so they follow the records
  like every other journaled record (§4.4's note).
- **Orders** (`take_unit`, from tests and the console) run at the hand-over's time as today. The
  four that run the clock there (the works grants and raids, TRIBUTE_START, HERD_FOUND) wait for
  the replay end (RP-33, the fix round's item 3): a unit given while replaying that holds one is
  queued, with every unit after it, and taken at the first step after the end. The rest run no
  clock.

**The rule.** rtsim-jni keeps `st.replay_clock: Tick` (not saved): set at the SESSION_START arm to
the world's present, and raised by `shared::catch_up` to each replayed record's world tick before it
runs the clock. While `st.replaying` and any of the four modules runs (`any_viewer`), every place
that turns the hand-over's game time into a time that runs the clock takes
`min(that, st.replay_clock)` from one function (`shared::clock_target`): the four modules'
`simulate` and `reset_if_due`, `phase1::simulate`'s target, ships' `build`, a non-journaled record
applied at the hand-over's time (ships' STATE kinds, ships' `reject`), herds' `found`. There are
about seventeen such reads of `world_tick(st, st.game_time)` and `phase1.wt(st.game_time)` on main;
the slice lists each in its report. With none of the four running, nothing is held: a phase 1 world
alone keeps today's clock and session rule (§6's last point). The clamps stay; with the world held
they no longer carry a record past its own time, and they still guard the world's past.

**At the replay end** the clock is at the replay clock. Each module's `replay_done` runs there; the
next run of the clock (the live record's `catch_up`, or the step's `simulate`) runs the held window
after every replayed record, and before any live one.

## 4. The gate

### 4.1 Where it lives (RP-3)

At the engine's push, one choke point per engine, not at rtsim-jni's `take_commands`: the engine is
where the decision is made, rtsim-host, rtsim-cli and crate tests use engines without rtsim-jni, and
the type system can guard it in the engine's crate.

- Each engine crate (rtsim-conflict, rtsim-ecology for dragons and nests, rtsim-ecology's herds,
  rtsim-ships) gets a private module `outbox` with `pub(crate) struct Outbox`, holding the commands
  (a private `Vec<Cmd>`), `replaying`, and in-memory counts (dropped, skipped, the last skipped kind).
- The only way in is `Outbox::push(cmd)`, which applies `gate(&cmd)`. Besides: `restore` (pages, at
  load, before any session), `take`, `retain` (removal only), and `len` and `iter` for the budgets.
- The engine's `outbox` field becomes an `Outbox`. A push around the gate does not compile.
- `fn gate(c: &Cmd) -> Gate`, in the same module: one arm per `Cmd` variant, no `_` arm. A new kind
  does not compile until it is given both verdicts:
  - `queued`: what `begin_replay` does to a command still queued (restored from the pages, or left
    from before): **keep** or **drop**;
  - `replaying`: what `push` does while replaying: **send** (queued as usual), **drop** (counted), or
    **never** (a bug, RP-4: counted, the kind kept for the log, not queued, and
    `debug_assert!(false, "{kind:#06x} decided while replaying")`).
- No release panic (the lead's condition 2): §6's early returns make a "never" kind impossible by
  construction; the gate is the backstop. Should one get through in release, the engine has changed
  its state as if the body existed; the replay end makes it abstract, so the skip heals.

### 4.2 Tests that hold it

- Compile time: the private `Vec` and the exhaustive match.
- One unit test per engine, `every_command_kind_has_its_replay_verdicts`: one value of each kind
  (boundary.rs's codec tests already build one of each) against the tables below.
- One test per engine for the "never" path:
  `#[cfg_attr(debug_assertions, should_panic(expected = "while replaying"))]`; in release it checks
  the count and the empty queue. The slice runs it once with `cargo test --release -p <crate> <name>`.

### 4.3 The log and the counters

rtsim-jni's `take_commands` for each module reads the outbox's counts after each take:

- a rise in "skipped" logs a warning at once, naming the module and the kind, e.g. "conflict decided a
  BAND_MATERIALIZE (0x....) while replaying: skipped, a bug (docs/replay-gate.md §4)";
- the replay end's log line gives each module's dropped and skipped totals for the replay.

The counts live in the module's JNI stats, not saved.

### 4.4 Conflict (rtsim-conflict boundary.rs `Cmd`, main 3391a5d9)

| Kind | Queued at the session start | While replaying | Why |
|---|---|---|---|
| Territory | drop (pushed afresh) | send | latest wins; the session start pushes every faction's |
| Notice | drop | drop | the kept notices go at the replay end (MZB-31, RP-12) |
| Envoy | drop | never | embodies |
| Materialize | drop | never | embodies |
| Steer | drop | drop | a body's order; no body of this session yet |
| Recall | drop | drop | the same |
| Settled | keep | drop | news only; the envoy that would name it is gone (RP-12) |
| AmbushSpot | drop | drop | a body's lurk |
| Reconcile | drop | never | answers a CAMP_CENSUS of this session with spawn, adopt and take lines (MOWZIE.md §4.9); M9 gates it on `is_in`, false all through a replay |
| TrainRobbed | keep | drop | a roar (RP-12) |
| ChiefBody | keep | never | goes only ahead of a stack, a spawn line or a hand-back, none of which is made while replaying |
| March | drop | send | latest wins per agent (MOWZIE.md §4.16): the abstract party's march |
| OwedHunt (M9, not on main) | drop | drop | held hunts go again at the replay end (§7.4) |

The queued column is today's `session_start` filter exactly (agent kinds, Territory, Notice and
Reconcile dropped). On main only Territory, Notice, Settled, Envoy, Materialize, Steer and Recall
have push sites (engine.rs about 1018, 1986, 2217, 2909, 3669, 3973, 4381, 4427); the others come
with M5, M7 and M9, whose merges meet the exhaustive match.

**M7's rail caller (feat/mowzie-m7, not on main): a note for whichever of M7 and fix/replay-push
lands second.**

- **The caller.** M7's rtsim-jni engine/industry.rs (about L570 to L577) calls `conflict::rail_call`
  (M7's engine/conflict.rs about L600) on TRAIN_ARRIVED and TRAIN_UNWATCHED. Without RP that path
  never reaches conflict's MZB-141 triggers, so `replaying` can stay true across it; M7's own fix
  runs conflict's replay-end trigger before `rail_call`.
- **The rule:** the replay end runs at the top of every module's journaled entry that can reach
  another engine, industry's included. RP-2's trigger at the top of engine.rs `journaled()` sits
  above every module's entry (`industry::journaled` is dispatched from there), so it already covers
  this caller. The merge deletes M7's conflict-only trigger with the other MZB-141 triggers (RP-2).
- **What it reaches.** Through `plan_feud`, `rail_call` pushes a NOTICE and a raid's or invasion's
  set-out and embody commands. They join §6's list: the embody returns early while replaying, and
  the gate drops the NOTICE (the embody is a "never").
- **RAIL_PASS** is ingested with no op id; the replay applies each once (pages plus a contiguous
  replay), and no copy carries one.
- **`Ev::Rail` wakes**, rebuilt from saved state, fire in the replayed window under the replay clock
  (§3.6) and embody nothing.
- **Track targets.** A track-target party sends its set-out and embody commands with a spot 5 blocks
  toward the den. One held at its target while replaying (§6.1) decides at the replay end against the
  engine's track, which Java may have removed since. The merger checks what Java answers for a
  removed track and that the answer leaves the party abstract and consistent.

### 4.5 Dragons and nests (rtsim-ecology boundary.rs `Cmd`)

| Kind | Queued | While replaying | Why |
|---|---|---|---|
| Materialize | drop | never | embodies |
| Steer | drop | drop | a body's order (no push site today) |
| Depart | drop | drop | a body's order; Java answers one for no body with NOT_EMBODIED (§5.8) |
| Adopt | drop | drop | decided from a replayed UNTAGGED; the engine keeps the adoption, and Java reports the entity untagged again this session (RP-13) |
| Discard | keep | drop | the same: the entity reports again and the live record decides (RP-13) |
| PlaceLair | keep | send | Java places once per field (LairPlacer's saved `fields`) |
| OilSurvey | keep | send | a query, answered with OIL_CHUNKS |
| PlaceNest | keep | send | idempotent (boundary.rs) |
| NestRenew | keep | drop in slice D; send from slice N | not idempotent until it carries its serial (§7.1) |
| NestSortie | drop | drop | a watched district's fight, re-derived (RP-15) |

### 4.6 Herds (rtsim-ecology herds/boundary.rs `Cmd`)

| Kind | Queued | While replaying | Why |
|---|---|---|---|
| Materialize | drop | never | embodies |
| Steer | drop | drop | a body's order |
| Despawn | drop | drop | the same |

### 4.7 Ships (rtsim-ships boundary.rs `Cmd`)

| Kind | Queued | While replaying | Why |
|---|---|---|---|
| Materialize | drop | never | embodies |
| Timetable | drop | drop | a body's legs |
| Steer | drop | drop | a body's order |
| Depart | drop | drop | the same |
| SetCargo | drop | drop | a body's hold, by epoch op |
| Despawn | drop | drop | a body's order |

Today dragons, herds and ships filter nothing at the session start, so a restored body command would
reach the new session's Java. The engines' outboxes are nearly always empty at a save under
rtsim-jni (commands are taken after each ingest and step), so this was rare; the queued column
closes it.

## 5. The observation side

### 5.1 The session start keeps the bodies (RP-5)

`begin_replay` changes no body: what the pages hold embodied stays embodied, at its epoch, with its
stacks. It sets `replaying` and applies the queued verdicts; conflict also pushes every faction's
TERRITORY, as today. Everything today's `session_start` does moves to `replay_done` (§5.6). Nothing
else at the arm changes a body either: `shared::session_start` forgets the players' positions and
calls no module's `player_left` (RP-26, §2.1).

### 5.2 A replayed *_MATERIALIZED is honoured (RP-6, RP-22, RP-31)

A *_MATERIALIZED dispatched while replaying, and not a copy, is Java's answer to a decision of the
last session. When the engine started from pages (RP-31, §2.6) and the epoch is above the agent's,
the agent is embodied as the last session had it:

- **Conflict** (BAND_MATERIALIZED: agent, epoch, dimension, wave, spawned), as RP-22 amends RP-6:
  - **A newer epoch:** if the party is embodied with bodies at an older epoch, those fold into a
    recalled stack of that epoch (MZB-131's walk-off, which the older epoch's records may still
    need). Then `epoch` = the record's, `embodied` = true, `spawned` cleared, `wave` 0, and the
    record's waves are filled.
  - **The same epoch, at or above the party's `wave`:** the waves from the party's `wave` to the
    record's are filled (normally one). Below it: nothing; the pages already hold that wave's fill.
  - **The fill** is `spawn_wave`'s own, without the command, from one function both call so that they
    cannot part: the room is `spawn_quota` (16) less the bodies in Minecraft by the books; the units
    in order, each up to its alive members not yet placed; then `wave` = the record's + 1. One
    difference keeps the count at or under what Java was commanded: the room also counts as placed
    every body of that epoch that died since the party's previous honoured record (an in-memory count
    while replaying; restarts: §5.3). The live engine decides a wave `wave_interval_ticks` (1,200) after the last,
    long after it read Java's answer to the last and the deaths before it, but a death journaled
    between its decision and Java's answer was not read; freeing that room could place more than
    Java had. A count below Java's loses nothing that is journaled: a despawn counts the members it
    did not know as placed as alive, and each death is a BAND_CASUALTIES of its own.
  - **Not every alive unit** (RP-6 as first written): Java's DEMATERIALIZED counts only the members
    it was commanded, so an invasion of 26 seen with 16 placed would despawn as 26 - 26 + 16 = 16, ten
    members lost.
  - **Not the record's `spawned`:** it counts the members placed at once; the pending rest are also
    Java's, and its despawn reports them as survivors (ConflictBridge.java about L815).
  - **An envoy** (decided by the coordinator, 2026-10-24): an honoured envoy record fills nothing,
    as `materialize_envoy` does, so `spawned` stays empty, as live. The reviewer asked for every
    unit; the live engine records none for an envoy and its despawn then counts nothing, so a fill
    would part the replayed path from the live one.
  - **A war party held at its target** (§6.1) whose record this is takes the fight's state too.
- **Dragons** (DRAGON_MATERIALIZED: agent, epoch, entity): `epoch` = the record's; the dragon's
  state becomes Embodied (since: the record's time, depart_at never, purpose home, no harass), its
  abstract leg cancelled, `by_entity` and the embodied index updated, as `materialize` does
  without the command.
- **Herds** (HERD_MATERIALIZED: agent, epoch, spawned): `epoch` = the record's; `embodied` with every
  alive member and no player (the puppets count is reset at the replay end anyway).
- **Ships** (SHIP_MATERIALIZED: agent, epoch, entity): `epoch` = the record's; `set_embodied(id, true)`.

An epoch at or below the agent's changes nothing. A live *_MATERIALIZED is handled as today. A
copied one embodies nothing (RP-8). One not honoured (a copy, a fresh engine's, RP-31) raises the
agent's epoch floor (§5.3).

### 5.3 Epochs are never reused (RP-7, RP-30)

An agent's next epoch is above every epoch the engine has seen for it: the pages', a replayed
*_MATERIALIZED's (§5.2), and a copy's (conflict's casualty copies raise it, §5.7). Each module's
`epoch + 1` becomes `max(epoch, floor) + 1` (conflict: `seen` and `start_fight`; dragons, herds and
ships at their materialize). The floor is an in-memory map per engine, agent to the highest epoch
seen that the agent's own `epoch` does not hold (RP-30). Not the epoch itself: raising a party's
`epoch` while it is embodied at an older one would make `party_of` and the despawn miss that body's
records. At `replay_done`, when every agent is abstract, each floor folds into its agent's epoch and
the map is cleared, so the pages carry it. A floor raised live (a copy after the replay end) waits
for the agent's next materialize. A restart loses the map; harmless, since no record of one body
is ever applied to another (RP-8). Each module that has epochs has its own test: an epoch decided
after the pages and answered in the journal is never decided again after a crash.

**Restarts (the fix round's item 2).** The floors and the fill's deaths (§5.2) are in memory only.
Outside a replay both are empty, but for a floor raised live: `replay_done` folds every floor into
its agent's epoch and clears the deaths, and no page is taken while replaying (RP-42). So pages
never hold an agent that needs either, and a restart rebuilds both from the records it replays.
Before RP-42 a save at the session's first live record could take pages mid-replay, and a restart
from them lost both: the next honoured wave of that epoch counted fewer bodies as placed, so its
fill went above what Java had placed and the next despawn (`alive - spawned + survivors`) lost the
difference; and an epoch could be decided again (harmless, as above). What remains is a floor raised
live, by a copy after the replay end: it stays in memory until the agent's next materialize, so
pages taken in between lack it, and a restart from them may decide that epoch again. A known limit,
and harmless by RP-8: a copy is an earlier session's record, and Java's later sessions hold no body
of it (§5.5), so no record of that body comes again to match the new one. Nothing that remains
counts a death or a member twice.

### 5.4 The MZB-138 fold goes (RP-10)

The fold at the session start existed so that replayed records found the last session's bodies.
RP keeps them embodied instead, so there is nothing to fold. At the replay end every party's
recalled stacks are dropped, as `replay_done` does today: nothing of an ended session reports
again except copies, which need no stack (RP-9). The live walk-off's recalled stacks within a
session (MZB-131, MZB-142) are unchanged.

### 5.5 A record of a body Java's new session no longer has

Every replayed record of such a body is history: it is applied (a death kills, damage counts, a
despawn counts its survivors). Whatever the engine decides for that body while replaying is dropped
by the gate. At the replay end the body becomes abstract with what the replay left. Java is told
nothing: it never had the body in this session.

### 5.6 The replay end per module (`replay_done`)

Today's `session_start`, moved, plus conflict's `replay_done`, plus what each module's `player_left`
did for every player at the arm before RP-26 (§2.1), without its body moves, since everything is
made abstract here anyway. Each runs at the replay clock (§3.6).

- **Conflict:** every party not embodied, no `seen_by`, `spawned` cleared, Fighting becomes Waiting,
  every recalled stack dropped, `replaying` false; every player's presence span ended
  (`presence.left`, P14's counter: the replay's gap is no time spent); the epoch floors folded
  (§5.3) and the fill's death counts (§5.2) cleared; the held set cleared, its parties' polls left
  to run (§6.1); then the kept notices go (`resend_notices`).
- **Dragons:** every embodied dragon resumed where it was last seen (`resume(.., left: true)`),
  `by_entity` and `chests` cleared, every player's interest leg cancelled.
- **Herds:** every embodied group abstract (clips' legs cancelled, members' `seen.last` set), every
  player's interest and `pos` forgotten, `puppets` 0. A herds player's interest is the world's area
  leg, which `forget_player_pos` already cancelled at the arm: set to None, not cancelled again (the
  Viewer impl's rule, rtsim-ecology herds/engine.rs about L2806).
- **Ships:** every embodied ship abstract, `seen_by` cleared, `lay_up_if_waiting`, every player's
  interest leg cancelled.

### 5.7 Copies, per module (RP-8, RP-9)

- **Conflict:** `Conflict::ingest_copy(w, obs, at)`, called by rtsim-jni when `st.applying_copy` is
  `Some`. A BAND_CASUALTIES copy of generation 0 is refused in rtsim-jni and counted (RP-21).
  BAND_CASUALTIES: the party by agent alone, the units of that body kind fall (capped at those
  alive), no stack is taken from, the killer's signal and the rout follow as for a live one, and the
  epoch floor rises to the record's. Then `spawned` and every recalled stack of that unit are capped
  at its alive units, as the recalled-entry despawn already caps `spawned` (rtsim-conflict engine.rs
  about L5052 to L5058) (RP-25). Without the cap the death is undone at the next despawn: a raid of
  6 all placed, a copied death (units 5, `spawned` still 6), then Java's 6 survivors give
  `5.saturating_sub(6) + min(6, 6)` = 6. With it, `5 - 5 + min(6, 5)` = 5. CHIEF_KILLED: as `ingest`
  (by party). BAND_MATERIALIZED: the floor only. Every other kind: as `ingest`.
- **Herds:** HERD_KILLED and HERD_TAKEN go by member key and op id already; a copied
  HERD_MATERIALIZED raises the floor only.
- **Dragons, ships:** nothing is carried; a copied *_MATERIALIZED would raise the floor only.

### 5.8 The DEPART trap (RP-16)

DragonBridge answers a DRAGON_DEPART for a dragon it does not hold with a DRAGON_DEMATERIALIZED of
reason NOT_EMBODIED that carries only the agent and the epoch. The engine's Dematerialized arm
(rtsim-ecology engine.rs about L3274) reads no reason and takes the zeros: health 0, hunger 0, a rest at
(0, 0, 0) in dimension 0. Fix in slice D: reason NOT_EMBODIED takes no position, health or hunger; the
dragon resumes where the engine had it, as a SPAWN_REJECTED does.

### 5.9 A casualty finds its stack by epoch (RP-27)

`casualties()` (live or replayed, not a copy) finds the party by agent (`Party::of_agent`), as the
despawn does, and then the stack by the record's epoch: `spawned` when the party is embodied at that
epoch; else the recalled stack of that epoch; else none, and the record is a straggler after its
despawn and counts nothing (MZB-138's rule, unchanged). The unit and the stack fall together, as
today. Today's `party_of` wants the current epoch exactly, so an old epoch's walk-off death while the
band is embodied again at a newer one is dropped (MZB-131's own case); finding by agent alone, with no
epoch, would take it from the newer epoch's stack and leave the old stack whole for its despawn to
count again.

## 6. Sightings wait for the replay end (RP-11)

- **rtsim-jni:** `shared::simulate` returns at its top while replaying, before it takes
  `st.shared.pending`. Player positions keep arriving (latest per player). Dragons' `player_at`
  (engine/shared.rs about L224), conflict's `observe_presence`, `observe_area`, `route_area` and
  the stale players' leave all wait. `present` was cleared at the session start, so the
  mid-session `introduce_herds` and `introduce_conflict` route nothing while replaying.
- **Engines** return early while replaying, before any state changes, from every decision that
  embodies (counted as deferred in the outbox's counts):
  - conflict `seen`, `spawn_wave` (a kept band's next wave, a timeline event), `materialize_envoy`,
    and two backstops at the top of `start_fight` and `wave` (RP-28): `start_fight` is reached only
    from the branches §6.1 holds, and `wave` returns without scheduling the next, since the kept
    band's fight becomes Waiting at the replay end anyway;
  - dragons `materialize` (callers: `seen`, the field flights in oil.rs, engine.rs about 1380 and
    2695);
  - herds `seen` (also fired by Ev::Seen events from the pages' wheel);
  - ships `materialize_timetabled` and `materialize_pirate` (from `seen`, `seen_ships`, `raid`).
- **At the replay end:** `shared::simulate` runs after the modules in every step, so the kept
  positions are routed in the same step when the replay ends inside a hand-over or at the step's
  top, and in the first step when it ends at SESSION_START. Every player who reported during the
  replay is routed at once, from the latest position; one who did not is routed at its next report.
  So yes: the first hand-over after the replay end.
- **Cost (D1):** a bool test per step when not replaying. Nothing is queued for later: the kept
  positions are the map that exists today.
- **Phase 1 keeps its own rule.** `shared::simulate` serves the four modules only: with none
  running (`any_viewer`) it returns before reading a position, and phase 1's caravans read no
  interest area (only the modules are routed one). So the early return changes nothing for a phase 1
  world, and phase 1's session start (every agent abstract, docs/boundary.md §2.3) is unchanged.
  Slice C still reruns the phase 1 jni tests, since the step's order is touched.
- **Phase 1 shows nothing while replaying** (RP-44, the fix round's item 4). `phase1::simulate`
  reads its own player positions (PLAYER records), not the shared areas, so the early return above
  did not stop it: in a replay of several hand-overs it materialized caravans and sent MARKET
  snapshots from a world in the replayed past, held at the replay clock (§3.6), before the records
  still to come had changed it. While `st.replaying` it now runs neither `interest()` (materialize,
  steer, capture) nor `markets()`, nor a test's P1_SPAWN_NEAR; their cadences are not touched, so
  the first step after the replay end runs them all. The world still runs and its own commands
  (`take_world_commands`) still go. A replay of one hand-over ends before phase 1 runs (§2.2), so
  it sees no change.

### 6.1 A war party's arrival waits (RP-23)

**The fault.** `watcher()` answers None for the whole replay: positions were forgotten at the arm,
and none is routed until the replay end. So a raid, invasion or siege that reaches its target in the
replayed window takes `arrive()`'s unwatched branch (rtsim-conflict engine.rs about L3228): it
recalls, `resolve()`s abstractly (the district's damage, the loot), then waits or goes home. If the
last session watched that arrival, it fought instead, and its replayed BAND_MATERIALIZED at e + 1,
casualties and BAND_LOOTED then apply on top of the abstract resolution: the raid is resolved twice.
Or `home()` has pooled the party and they drop. A siege's `wait_over()` (about L3259) has the same
branch.

**The rule.** While replaying, the branches of `arrive()` and `wait_over()` that would call
`watcher()` (a war party when the world is not Peaceful; a siege's next round) neither fight nor
resolve. The envoy, scout and Peaceful branches ask no watcher and are unchanged.

- **The hold.** The party goes into an in-memory map `held`, party to its arrival tick. The first
  hold sets `bored_at` as `arrive()` does today; a later one keeps it.
- **The poll.** Its Ev::Party is scheduled again with the same plan, `REPLAY_HOLD_TICKS` later
  (PLACEHOLDER 20 world ticks), so that pages saved in the middle of the replay hold an event for the
  party and a restart from them arrives again. The poll must not move the route: `party_event`'s Out
  arm increments `seg` before it calls `arrive()` (about L2826), and the wheel's events are saved, so
  no new `Ev` kind (a page format change). Recommended: the held check in the Ev::Party arm answers
  a held party's poll before `party_event`, and the first hold puts `seg` back by one, so that a
  poll that does reach `party_event` (after the replay end, or after a restart that lost the map)
  lands on the last segment again and arrives. The slice checks `party_pos` and `book_leg` with `seg`
  one short and tests the restart.
- **Honoured.** A replayed, honoured BAND_MATERIALIZED of a held party (§5.2) is the last session's
  watched fight. The party takes the state `start_fight` leaves, without its commands: Fighting;
  `until` = `min(arrival + watched_fight_ticks, bored_at)` from the held arrival tick;
  `fight_strength`; its leg booked from the record's time to `until`; `seg` on the last segment; an
  Ev::Party at `until` with a bumped plan, which voids the poll; the epoch and the fill as §5.2 says;
  `seen_by` None (`wave` returns early while replaying, and the replay end clears it). It leaves
  `held`. Its replayed casualties count and rout as live ones, and the fight ends at `until` inside
  the window (§3.6) or becomes Waiting at the replay end, as every Fighting party does.
- **Not honoured.** A held party with no such record keeps its poll. `replay_done` clears `held`,
  and the poll's next firing, after the replay end, runs `arrive()` or `wait_over()` as today.
  An arrival that arrives again so keeps its arrival tick, its route's last time, and its patience
  runs from there, as in the session that went on (RP-45).
  Positions are routed only after the modules run in a step (§6 above), so that first decision
  normally finds no watcher and resolves abstractly: the reviewer's rule (resolve only when no
  replayed BAND_MATERIALIZED embodied the party), once.

**Losses accepted** (lose, never double):

- A crash right after a watched arrival, before Java journaled its BAND_MATERIALIZED: the fight
  Java may have shown is resolved abstractly at the replay end.
- A held party's BAND_MATERIALIZED that was a sighting, not a fight. A party that resolved unwatched
  and was then seen on its way home answers with a BAND_MATERIALIZED too, and the record does not
  say which. It is read as a fight at the target: the abstract resolution the live engine made is
  lost, and the band walks home later.
- An M7 track-target party held at a track Java has removed since (§4.4's note).

**Slices D and S** check dragon raids and pirate coast raids for the same pattern: a decision in the
replayed window that asks whether a player watches, which while replaying always hears no.

## 7. Re-derived events in (S, W]

While replaying, `catch_up` runs the engine's own timeline over the replayed window, so its events
are decided again. Java may already have applied them before the crash.

### 7.1 NEST_RENEW (RP-14)

- Today Java adds: `n.owed += count` (Nests.java about L428), so a renewal decided again is owed twice
  (the audit's nest test: engine 1 worm, world 2).
- **Options:** (a) drop NEST_RENEW while replaying, which loses a renewal Java's save lost; (b) an
  absolute owed count, which races Java's own placing (a worm placed between the engine's last census
  and the command's arrival is owed again: a double); (c) a renewal serial. **Chosen: (c), the
  serial,** because it never doubles by construction and loses nothing.
- **The serial:** each nest row gains `renewals: u32`, incremented by `renew`; NEST_RENEW carries
  it. The catch-up re-derives the same renewals in the same order, so a renewal decided again carries
  the same serial. Java keeps per nest the last serial applied and the engine life it came under
  (RG13), and applies a NEST_RENEW only when its serial is above the last under the same life.
- **Fail safe on an old file:** a Nests SavedData without the serial loads with every nest's owed at
  0 and its last serial at 0: whatever was owed is dropped, never doubled (the lead's condition 4).
- **A new engine life** (a reinstall, RG13): the serial starts afresh for that nest; the owed count
  stays (the new life's renewals are its own decisions) (RP-19).
- Until slice N lands, slice D drops NEST_RENEW while replaying (option a's loss, for one slice).

### 7.2 NEST_SORTIE (RP-15)

Dropped while replaying. Loses: the visible worms of a raid on a watched district decided again in
the replayed window. The raid's effect on the district is the engine's and stands.

### 7.3 Conflict NOTICE, DEMAND_SETTLED, TRAIN_ROBBED (RP-12)

- NOTICE: dropped while replaying. Loses nothing: a notice is kept state, and every kept notice goes
  at the replay end (MZB-31's `resend_notices`, unchanged).
- DEMAND_SETTLED: dropped. Loses the "demand settled" news when the last session decided it after
  its last save and Java never showed it. The envoy it would rename is gone with the session.
- TRAIN_ROBBED: dropped. Loses a roar.

### 7.4 OWED_HUNT (M9)

mz9f's re-check (R1) adds two temporary guards: no OWED_HUNT is pushed while `replaying`, in
`owe_hunt` and in `hunt_applied`'s re-send, and `resend_notices` re-sends every hunt still held. The
gate gives OwedHunt "drop" while replaying and "drop" queued. When feat/mowzie-m9 merges main after
slice C, the merger deletes both guards (each carries a comment naming fix/replay-push), keeps the
re-send, and deletes the `outbox.retain` filters for OwedHunt, which `Outbox` replaces.

## 8. Formats

| What | Change | Slice |
|---|---|---|
| Conflict, herds, ships pages | none (a held party's poll is an ordinary Ev::Party) | C, H, S |
| Dragons pages: nest rows | `renewals: u32` | N |
| Dragons pages: the outbox | NestRenew carries its serial | N |
| Wire: NEST_RENEW | a u32 serial (docs/boundary.md §6.4, DragonRecords) | N |
| SavedData: Nests | per nest the last serial and its engine life | N |
| META | none (META_VERSION 2 is S4p's) | |
| Not saved | `replaying`, `applying_copy`, `replay_end_due`, the gate's counts, `replay_clock`, the hand-over count; conflict's `held` map and the fill's death counts; each engine's epoch floors | C to S |

Nothing is released, so no compat work. After slice N: delete every `neoforge/*/run-gametest/world`
in the worktree before the next GameTest run, and rerun crates/rtsim-view/tests/fixtures.rs; if it
fails on the dragons pages, regenerate the core and create fixtures as 52639c6b did.

## 9. Slices

The Opus review of this design (the lead's condition 5) is folded in (RP-22 to RP-31). Slice C goes
to an Opus implementer, then the slice's Opus review, with no separate re-check. Slices D, H and S
may go to Sonnet implementers, each copying slice C's pattern (one line per slice below says what to
copy), with an Opus review each. Order: C first; after C lands, D, H and S may run side by side (each
adds one line to shared.rs's list of replay ends); N after D, and after RG (feat/reinstall-guard,
RG13's engine life, at 35e9b082 and later) is on main, since N's serial keeps that life (§9.3).

Every slice: `nice -n 10 cargo test --workspace --no-fail-fast`, `cargo clippy -p <touched crates>
--all-targets -- -D warnings` (glass.rs:175 needs `-A clippy::needless_borrows_for_generic_args`),
rustfmt only on files whose base is clean (protocol.rs is not: hand-format). The fake Java sends
player positions only in the first hand-over at each game time. A long replay needs enough fillers
to span three or more hand-overs on a `MIN_CAPACITY` engine (`MAX_RECORD + 128`, about 64 KB):
4,000 small records, not 1,500.

### 9.1 Slice C: the flag, the replay end, the clock, conflict

Steps:
1. rtsim-jni State: `replaying`, `applying_copy: Option<u64>` (None until S4p's wiring),
   `replay_clock`, the hand-over count, each module's `replay_end_due`.
2. SESSION_START arm, `journaled()` top, `step_inner` top: §2.1 (with RP-26's forgetting and RP-29's
   journal check), §2.2 (with the hand-over count at step entry). Until slices D, H and S move their
   player work into `replay_done`, dragons, herds and ships still hear `player_left` at the arm
   (their bodies still end there, today's `session_start`); only conflict's is left out, and each
   later slice leaves its own out.
3. engine/shared.rs: `replay_end_if_due`, `replay_end_before`, `replay_end` (§3.2); the early
   return in `simulate` (§6); `clock_target` and the raise in `catch_up` (§3.6).
4. The clock (§3.6, RP-24): every read of the hand-over's time that runs the clock goes through
   `clock_target`, in rtsim-jni's conflict.rs, dragons.rs, herds.rs, ships.rs, phase1.rs and
   worldmodel.rs (no founding while replaying). This is the one place slice C touches the other
   modules' files beyond their `replay_end` stubs: the clock is shared, so it cannot be held for
   conflict alone.
5. engine/conflict.rs: reset calls `begin_replay(from_pages)` and sets `replay_end_due`;
   `replay_end` calls `replay_done` then the notices' re-send; the MZB-141 triggers and
   `notices_due` go; journaled calls `ingest_copy` when `st.applying_copy` is `Some`, and refuses a
   BAND_CASUALTIES copy of generation 0, counted (RP-21); take_commands logs the gate's counts
   (§4.3).
6. rtsim-conflict: `outbox` module with `Outbox` and `gate` (§4.1, §4.4); every push, the persist
   restore, the session filter and `resend_notices`' retain go through it.
7. rtsim-conflict: `session_start` becomes `begin_replay` (§5.1); `replay_done` per §5.6; the
   honoured BAND_MATERIALIZED with RP-22's fill (§5.2); the epoch floor (§5.3, RP-30); `ingest_copy`
   with RP-25's cap (§5.7); `casualties()` by epoch (§5.9, RP-27); early returns in `seen`,
   `spawn_wave`, `materialize_envoy`, and the backstops in `start_fight` and `wave` (§6, RP-28); the
   held arrivals (§6.1, RP-23).
8. The callers of `session_start` (§3.4) choose `begin_replay` and `replay_done`.
9. If S4p is on main by then: §2.7's merge, the copy wiring in `reasserted()`, §2.5's tests.

Tests (each with its control; the scratch tests from the audit become real ones):
1. `a_band_first_seen_after_the_save_is_counted_after_a_crash` (scratch audit_replay_push.rs :5601:
   12 alive today, 6 expected). Control: the same run without the crash, 6.
2. `a_long_replay_counts_every_casualty_against_its_band` (scratch :5586, fails today). Control: the
   one-hand-over replay (scratch :5592, passes today and after).
3. `no_band_materializes_while_a_long_replay_runs_with_a_player_near`: no BAND_MATERIALIZE,
   ENVOY_MATERIALIZE or FACTION_NOTICE in any command buffer before the replay end; then the band
   materializes from the live position, at an epoch above every replayed one. Control: no crash, it
   materializes on the first hand-over.
4. `a_band_epoch_decided_after_the_save_is_never_decided_again` (RP-7). Control: without the crash,
   epochs E+1 then E+2.
5. `every_command_kind_has_its_replay_verdicts`, and the "never" test (§4.2).
6. `a_copied_casualty_counts_against_an_abstract_band_once` (engine level, `ingest_copy`): an
   abstract party, and one embodied at a newer epoch whose stack is untouched. Control: the same
   record through `ingest` is a straggler and counts nothing. In rtsim-jni: a copy of generation 0
   counts nothing, twice over (RP-21), once S4p's `reasserted()` sets the field.
7. A module switched on mid-session ends its replay at once (U10); a session with nothing to replay
   ends at SESSION_START, and so does one whose journal starts at or past `session_first` (RP-29);
   the hand-over count is 0 then, 1 for a short replay and at least 3 for the long one.
8. The MZB-138, MZB-141 and MZB-142 tests pass. Where an assertion read a recalled stack the fold
   made, it reads the kept embodied state instead; no count is weakened.

The review's four amendments, failing first. Each is written before its fix and run on the code
without it; the commit message records the failure (the values seen). (b) and (c) fail on main as
well; (a) and (d) can only fail once their base exists (RP-6's honour, RP-9's `ingest_copy`), so the
base goes in first in its plain form, as first designed, and the amendment after.
- **(a) RP-22:** an invasion of 26 seen after the save, 16 placed, despawned with 16 survivors: 26
  alive after the crash (16 with `spawned` = every alive unit). The same with 3 killed before the
  despawn and 13 survivors: 23 (26 on main, 13 with the plain honour). Control: no crash, 26 and 23.
- **(b) RP-23:** a raid reaching a watched target in the replayed window, its BAND_MATERIALIZED and
  casualties replayed: resolved once (no abstract `resolve`, the district's damage and the loot as
  the no-crash run's), its casualties counted. Control: no crash; and an unwatched arrival in the
  window, which resolves abstractly once, after the replay end.
- **(c) RP-24:** a watched fight kept in the pages (Fighting at the save), whose casualties are
  delivered in the replay's third hand-over (fillers before them), and whose end, walk home and
  arrival (`home()`, which pools the party) all fall after those casualties' game time but inside
  the window, on a short route home. Today the first replayed hand-over's step runs the clock past
  the arrival, the party is pooled, and the casualties drop. After: they count, and the faction's
  pool after the replay is the no-crash run's. The event inside a later hand-over's window is what
  the test is for; the audit's 3,000-block march had none. Control: no crash; and the
  one-hand-over replay.
- **(d) RP-25:** a raid of 6 all placed, a copied BAND_CASUALTIES, then Java's despawn with 6
  survivors: 5 alive (6 without the cap). Control: a live BAND_CASUALTIES and a despawn with 5
  survivors: 5.

And one for each of the other two review items, with its control:
- **RP-26:** a watched, kept band across the arm: still embodied after SESSION_START, no RECALL in
  any buffer, and its replayed casualties count off its kept stack. Control: no crash, the same
  counts. Written after RP-5 and before RP-26, where it fails.
- **RP-27 (MZB-131):** a band recalled at epoch e and embodied again at e + 1; a casualty of epoch e
  takes from the recalled stack and leaves `spawned` whole; one of e + 1 takes from `spawned`.
  Control: casualties of e + 1 only. Written first; fails on main (the e casualty is dropped).

GameTests: `neoforge/gametest.sh core carryon` (ConflictGameTests, CarryOnConflictGameTests; core
also runs the herd and phase 1 GameTests that the routing and clock changes touch).

Opus review checks:
1. No `_` arm in `gate`; nothing outside the outbox module touches its `Vec`.
2. Every path to an Envoy, Materialize, Reconcile or ChiefBody push returns early while replaying
   (list the callers).
3. `begin_replay` changes no body, and nothing else at the arm does (RP-26); `replay_done` does what
   the old `session_start` and `replay_done` did together, plus §5.6's player work.
4. The replay end runs once a session, after the last replayed record and before the first live
   one, at each of its three triggers, and with a journal gap.
5. A replayed BAND_MATERIALIZED raises the epoch and embodies; a live, a copied or a fresh engine's
   one does not.
6. D1: nothing new per step when not replaying beyond the bool tests and the clock target's test.
7. The phase 1 jni tests pass with the routing deferred and the clock held.
8. The four amendments as explicit items: RP-22's fill never above what Java was commanded; RP-23's
   hold resolves a watched arrival once, survives a restart from pages saved mid-replay and never
   moves `seg` past the route; RP-24 holds every read of the hand-over's time that runs the clock
   (the slice's list against a grep); RP-25's cap. Tests (a) to (d) checked against them, and each
   one's recorded failure.

### 9.2 Slice D: dragons

Pattern: copy slice C's conflict work (its `outbox` module and `gate`, `begin_replay(from_pages)` and
`replay_done`, the honour and the floor, the early returns, rtsim-jni's `replay_end` stub made real,
dragons left out of the arm's `player_left`, their interest legs cancelled in `replay_done`); the
flag, the replay end and the clock are slice C's already.

Steps:
1. rtsim-ecology: `outbox` module for the dragons engine (nest.rs and oil.rs push through it), §4.5
   with NestRenew dropped while replaying.
2. `session_start(w, at)` becomes `begin_replay(from_pages)` and `replay_done(w, at)` (§5.6).
3. The early return in `materialize` (§6); the honoured DRAGON_MATERIALIZED (§5.2); the epoch floor.
4. The DEPART trap (§5.8).
5. rtsim-jni engine/dragons.rs: reset, `replay_end`, take_commands' counts; shared.rs's list.
6. Dragon raids (§6.1's last point): find every decision in the replayed window that asks whether a
   player watches (a raid on a district, a field flight, a nest sortie) and hold or gate it as §6.1
   does for conflict's arrivals; list each with its answer.
7. Read, do not change: DragonBridge reports an untagged dragon once per entity per session at join
   (`reportUntagged`, Z9), and the tag is never saved, so a dragon adopted in the replayed window
   reports untagged again (RP-13). Say so in the slice's notes, with the lines.

Tests:
1. `a_dragon_killed_after_a_save_stays_dead_after_a_crash` (scratch :2256: 1 alive today, 0
   expected). Control: without the crash.
2. The epoch test (RP-7).
3. `a_depart_answered_not_embodied_keeps_the_dragon_where_it_was`: health and position as the engine
   had them (today health 0 at (0, 0, 0)). Control: a DEMATERIALIZED of a held dragon takes its
   health and position.
4. `a_replayed_untagged_dragon_is_adopted_again_by_its_live_report`: no ADOPT while replaying; the
   live DRAGON_UNTAGGED after the replay end re-adopts it at a new epoch. Control: a live UNTAGGED
   adopts at once.
5. `no_dragon_materializes_while_a_long_replay_runs_with_a_player_near`, with its control.
6. The gate tests (§4.2).

GameTests: `neoforge/gametest.sh iceandfire`.

Opus review checks: as slice C's 1 to 6, for dragons; the field flights' early return leaves the
flight abstract and consistent; the DEPART fix touches only NOT_EMBODIED; step 6's list.

### 9.3 Slice N: nest renewals

N goes after D, and after RG lands: its serial keeps RG13's engine life (RP-19), which exists only on
feat/reinstall-guard (35e9b082 and later), not on main.

Steps:
1. rtsim-ecology nest.rs: `renewals` on the nest row, carried by NEST_RENEW; persist.rs and
   boundary.rs (layout, codec); the gate's NestRenew becomes "send" while replaying.
2. Java: DragonRecords' NEST_RENEW layout; Nests.java's renew applies a serial above the last under
   the same engine life; the SavedData keeps both; an old file loads owed 0, serial 0 (§7.1).
3. docs/boundary.md §6.4.

Tests:
1. `a_renewal_after_the_save_is_not_owed_twice_after_a_restore` (scratch
   rtsim-ecology/tests/audit_nest_renew.rs :785: engine 1 worm, world 2 today): at engine level, the
   restored engine's renewal carries the serial the first run's did. Control: two renewals carry two
   serials.
2. Java unit tests: a repeated serial adds nothing; a higher one adds its count; an old file loads
   owed 0; a new engine life restarts the serial.
3. The rtsim-view fixtures (§8).

GameTests: `neoforge/gametest.sh iceandfire`, after deleting the worktree's run-gametest worlds.

Opus review checks: never doubled on any path (a restore, a repeat, an old file, a new life); the
serial is the catch-up's own, not a session's; the wire size matches on both sides.

### 9.4 Slice H: herds

Pattern: copy slice C's conflict work (outbox and gate, `begin_replay(from_pages)` and
`replay_done`, the honour and the floor, the early return, herds left out of the arm's
`player_left`); §5.6's herds line includes the player work RP-26 moved from `player_left`.

Steps: the outbox and §4.6; `session_start(legs, at)` becomes `begin_replay(from_pages)` and
`replay_done(legs, at)`; the early return in `seen`; the honoured HERD_MATERIALIZED (§5.2); the epoch
floor under `wrapping_add(1).max(1)`; rtsim-jni engine/herds.rs and shared.rs's list.

Tests:
1. `a_long_replay_with_a_player_at_the_herd_never_sends_a_killed_sheep` (scratch :3533: members
   [0, 9, 10] sent again today). Control: the one-hand-over replay.
2. The epoch test (today: 1 live, and 1 again after the restart).
3. A replayed HERD_DEMATERIALIZED of a group materialized after the save captures its members'
   positions. Control: one the pages held.
4. The gate tests.

GameTests: `neoforge/gametest.sh core` (HerdGameTests).

Opus review checks: as slice C's; Ev::Seen events restored from the pages do nothing while
replaying; the players' puppet counts end at the replay end.

### 9.5 Slice S: ships

Pattern: copy slice C's conflict work (outbox and gate, `begin_replay(from_pages)` and
`replay_done`, the honour and the floor, the early returns, ships left out of the arm's
`player_left`, their interest legs cancelled in `replay_done`), plus §6.1's check for pirate coast
raids.

Steps: the outbox and §4.7; `session_start()` becomes `begin_replay(from_pages)` and
`replay_done()`; early returns in `materialize_timetabled` and `materialize_pirate`; the honoured
SHIP_MATERIALIZED; the epoch floor; rtsim-jni engine/ships.rs and shared.rs's list; rtsim-ships
tests choose (§3.4); pirate coast raids (`coast_raid`, rtsim-ships engine.rs about L3431) checked
for §6.1's pattern, held or gated, with the answer in the slice's notes.

Tests:
1. `damage_to_a_ship_seen_after_a_save_is_counted_after_a_crash` (scratch :2625: live (3, 800, 1),
   after the crash (4, 1000, 0) today). Control: without the crash.
2. The epoch test.
3. The gate tests.

GameTests: none (no ships plugin in Java); rtsim-host's ships runs.

Opus review checks: as slice C's; a kept ship's NextLeg events while replaying drop their commands
and keep the voyage's times; `lay_up_if_waiting` runs at the replay end.

## 10. Questions for Ben

Gameplay trade-offs at the forks, recommendation first. The coordinator decided all three on
2026-10-24, as recommended (crash-only, small and reversible), and lists them for Ben, who can
overrule.

1. **After a crash, lose rather than repeat.** Things the last session decided after its last save
   (a "demand settled" message, a robbed-train roar, a worm sortie at a watched base) are not shown
   again, since Java may have shown them already. *Recommended: lose them.* The other way shows some
   twice. **Decided: lose them.**
2. **Nothing appears near players until the replay ends.** At a restart the journal's replay takes
   the first hand-overs (a few more for a long journal); bands, dragons, herds and ships near a player
   appear right after it. *Recommended: accept.* Players normally join after it. **Decided: accept**, with a
   counter: `shared::replay_end` records how many hand-overs the replay took (a stat, and an info log line
   at the end naming the count and the records replayed), so a slow replay shows up in GameTests and
   soaks (RP-2).
3. **A copied band death after a crash** (a dying member's drops carried its record) shrinks the band
   by one even when it is in view again; its bodies in the world may then be one more than its count
   until the next despawn. *Recommended: accept.* The other way leaves a dead member alive for good. **Decided:
   accept**; the extra body clears at the next despawn.
   That holds only with RP-25 (§5.7): the copied death also caps the band's count of placed members
   at its alive ones, and without the cap the next despawn would undo the death instead.
