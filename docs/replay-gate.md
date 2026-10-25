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
  `applied_through + 1 < session_first`. RP keeps one bool, `st.replaying`, that every module reads:
  set at SESSION_START and cleared by the first of the three triggers (§2.2), which runs the replay
  end once (RP-1):
  1. the top of `journaled()`, for a record whose own sequence is at or past `session_first`
     (`replay_end_before`, which tests the record's `seq`, not `applied_through`);
  2. the step's top, once `applied_through + 1 >= session_first` (`replay_end_if_due`);
  3. the SESSION_START arm's end, once `max(applied_through + 1, jf) >= session_first`, Java's
     journal start counted (`replay_end_at_session_start`, RP-29).

  So the flag is not the predicate latched: with RP-29 it can be false while
  `applied_through + 1 < session_first` (a journal that starts at or past `session_first` sends
  nothing below it), and trigger 1 ends it on a record's sequence (after a gap, the first record
  that comes).

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
     and ends a replay that has not started. The `applying_copy` reset is a defence only (RP-8):
     `reasserted()` clears the field after every dispatch, and a step that panics inside one sets
     the engine failed for good (the step's `catch_unwind`), so no later step reads a record, this
     arm included. It keeps a field that should already be `None` from marking a record of this
     session a copy. The replay clock starts at the world's present (§3.6).
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
  path: a copy judged not new goes to `answered_again` and is never dispatched, and a new one whose
  op id was already heard (`admit_copy`'s DuplicateOp) is dispatched as that verdict, answered again,
  without the bit; `dispatch` debug-asserts the bit only on Apply. Both count in `copies_dropped`,
  never `copies_applied` (RP-50; the merge set the bit and counted the copy applied before the
  verdict, SEC-1's meeting-point review). A copy of the open
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
- **A known non-case (SEC-1's meeting-point review):** a session of a bridge that names no
  generation (SECTIONS.md SC128's unnamed generation) replayed in a later session is moot before
  release, since every bridge now journals its JOURNAL_GEN at open and nothing unreleased keeps
  backward compatibility.
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
6. **SEC-1's meeting-point review, item 7:** a second live copy after a replayed copy counts nothing
   (`a_second_live_copy_after_a_replayed_copy_counts_nothing`); a save due at a REASSERT takes the
   pages before the copy, which then counts once, live and in the next session's replay
   (`a_save_due_at_a_reassert_takes_the_pages_before_the_copy`). On main a session's first live
   record is its JOURNAL_GEN, so that REASSERT is the first record past the save Java asks for, not
   the first live record (§2.4's no-gen bridge is the only other way). The third, a held arrival's
   replayed fight, is §6.1's.

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

**The M5 merge** (this branch merging main 4ac4801a: M5 at 4b926efb, SEC-1 S4c1 at 428a3311, the
M10 doc; RP-64). The one textual conflict was rtsim-conflict engine.rs: `ConflictStats` (both
sides' stats kept) and the `Conflict` fields and `Conflict::new` (both sides' kept but M5's
`replaying`). The rest is semantic:

1. **`replaying` goes.** M5's `pub replaying: bool` was MZB-141's flag, set by `session_start`
   and cleared by `replay_done`, which RP-2 and RP-10 had retired for the outbox's (§3.3). Its
   readers use `replaying()`: the CAMP_CENSUS arm (engine/camp.rs), synth's `CampRun` (its
   restart opens the window with `begin_replay(true)`, the host's session start; its save and load
   asserts the window is closed, since the host takes no page while replaying, RP-42), and
   tests/camp.rs (the window through `begin_replay` and `replay_done`). M5's `session_start` is
   gone with it: git carried its `camp_session_start` loop into `begin_replay`, its command filter
   is the gate's queued column, and its MZB-138 fold stays retired (RP-10, §5.4). Nothing of M5
   runs at an MZB-141 trigger: rtsim-jni has none since RP-2, and main added none.
2. **`obs_seq` and `durable_obs_seq` stay** (MZB-177). rtsim-jni sets them as main does:
   `obs_seq` in engine/conflict.rs `journaled` from each record's OBS_SEQ, `durable_obs_seq` at
   WORLD_SAVED (engine.rs, never lowered). A replayed record's `obs_seq` is below `session_first`
   (every record dispatched while replaying is an earlier session's, §2.2), a replayed copy's too,
   its OBS_SEQ being the REASSERT's own (§2.3); a live record's, a live copy's among them, is at or
   past it. A CAMP_BODIES answer is kept by that sequence until a WORLD_SAVED makes it durable.
   The merge check's follow-ups: `obs_seq` is set before `journaled`'s early paths (RP-67), and a
   WORLD_SAVED while the outbox replays waits for `replay_done` (`Conflict::world_saved`, RP-68),
   so no answer booked from a replayed record is forgotten while the replay can still bring its
   holder's copy.
3. **The gate**: no new kind (§4.4).
4. **The floors** (RP-57): no new `Obs` kind either. The camp kinds (CAMP_CENSUS, CAMP_BODIES,
   CAMP_OUT) carry a camp's epoch, a u64 Java draws (`Journal.newOpId`, M5), never one the engine
   issues, and CAMP_DEATH and CHIEF_PAID none: `party_epoch` stays None for them, as RP-57's table
   says.
5. **S4c1 and the meeting points.** S4c1 changes rtsim-industry and Create's Java (the frontier,
   package stores) and none of rtsim-jni's session start arm, `journaled()` or `reasserted()`;
   main's rtsim-jni engine.rs gains only the WORLD_SAVED line above. S4p's `seq < session_first`
   test, REASSERT's judgement and the pinned JOURNAL_GEN hold as §2.1 to §2.3 say. On the Java side
   M5's camp holders carry the generation each record was journaled in (RtsimSession, MZB-652) and
   reassert a holder's carried records before its new ones: a copy is still judged by S4p's
   `copy_is_new`, by that generation, so §2.3's two cases stand.
6. **Fixtures**: main regenerated the rtsim-view fixtures at cc0c8c34 (M5's head); no converter yet
   (no ChunkWait on main).
7. **The merge check's tests** (RP-65, RP-69), every one the check listed:
   - rtsim-jni: `a_replayed_census_orders_nothing_and_its_answers_are_booked_by_state` now also
     reasserts each replayed answer live for the census's epoch and books nothing again, and
     asserts the gate's "never" count stays 0;
   - rtsim-jni: `a_replay_begun_by_a_replayed_faction_only_marks_its_census_out`, the window
     opened late by `reset_if_due` (the pages predate the camp);
   - tests/camp.rs: `a_replayed_homecoming_at_an_out_camp_sends_nothing` (`camp_home`'s recall,
     dropped by the gate);
   - tests/camp.rs: `a_replayed_death_of_an_embodied_partys_member_costs_its_unit_and_sends_nothing`;
   - tests/camp.rs: `a_replayed_census_only_marks_its_camp_out`, which now answers the replayed
     epoch (RP-65).
   None is left unwritten.

**The M6 merge** (feat/mowzie-m6 at fd8eff01 merging main f66cb736, this slice; RP-100, RP-101).
Six files conflicted:

1. **engine.rs `spawn_wave`:** main's body (the §6 early return, `wave_fill(p, 0)` and `place`,
   BAND_FIGHT in the flags) with M6's two stack fields: a boss's chief stack is one short of its
   count (`renewal.boss`), and a stack made without its crystal carries MEMBER_BARE (review C).
2. **engine.rs `casualties`:** main's body (the party by agent, the stack by epoch, RP-27; a copy
   from no stack, RP-9, with RP-25's cap; `fill_deaths`) with M6's MZB-589 dedupe first: a record
   whose op the party has counted (`Party::casualty_ops`) is dropped. The op is kept only when the
   record took a unit (git's merge).
3. **synth.rs:** the emulated BAND_MATERIALIZED carries M6's `spawned` (the chief counted) and
   main's `flags`.
4. **tests/mowzie_records.rs:** both sides' tests kept. Main's six BAND_CASUALTIES literals take
   `op: next_op()`, each its own: the record is 72 bytes on M6, its op at 64 (MZB-589). M6's
   `a_casualty_copy_after_a_save_and_reopen_counts_once` expected the restart to fold the embodied
   band into a recalled stack, as M6's `session_start` did; here the bodies stay embodied at their
   epoch for the replay (§5.1, RP-5), so its restart leg asserts that instead, and the replay's copy
   still counts nothing.
5. **rtsim-jni tests/protocol.rs:** main's `grove_with(name, strays, pages_first)` (RP-69) over
   M6's `camp_of(name, structure, body, strays, census)`, which gains `pages_first`; M6's `den`
   passes false. Slice C's `fillers` take M6's `next_casualty_op()`, and two comments there and in
   M6's filler loop now say 72 bytes. At 72 bytes 4,000 fillers no longer fit one 256 KiB
   hand-over, so the two replays that count one hand-over for the whole tail (RP-42's
   `a_close_while_replaying_keeps_the_pages` and the held arrival across a second session, both
   cases) take `ONE_HAND_OVER`, 1 MiB.
6. **docs/boundary.md** (the 0x8803 row) and **ConflictRecords.java:** main's flag 8 (FLAG_FIGHT)
   and M6's stack flag 2 (STACK_BARE), both.

What the merge checked:

1. **The gate:** M6 adds no `Cmd` kind and no push site. The ten pushes in engine.rs and
   camp.rs's `send_lines` are main's, so every command of M6's goes through `push`. M6's new routes
   all reach them:
   - `camp_arrival` (a Frostmaw's wait ran out) orders its SPAWN only at a camp that is in;
   - the census's crystal mend asks the lines again from inside a live `camp_census`;
   - `camp_refill` needs `is_in`;
   - a stray reaches the engine only as a census line;
   - the chief's fall, at the camp and away (`chief_killed_by`, `chief_killed_at_camp`), sends
     Recall through `go_home` and `end_fight`, which drop while replaying, and replans through
     `plan_feud`, whose embodies return early (§6).
   Reconcile's "never" holds by construction, as at the M5 merge: every camp is out from
   `begin_replay` to `replay_done`, and only a live census, which comes after the replay end, brings
   one in. §4.4 lists the sites.
2. **The camp hold:** Java's `Camps.sessionStarted` clears every camp's `recount`, `strayBodies`,
   `holdSince` and held lines, and looks at each again. The census it then takes is live (its
   sequence is at or past `session_first`, §2.2), so the engine reads it after the replay end. M6's
   recount hold (`c.recount` holds every SPAWN, `strayHold` up to HOLD_MAX with a rescan every 100
   ticks) holds lines of that live epoch only. The replayed CAMP_* records keep §5's rules:
   - CAMP_CENSUS only marks its camp out (RP-64, RP-65): its bodies, M6's strays and BODY_CRYSTAL
     among them, are dropped;
   - CAMP_BODIES is booked by state (MZB-177, RP-65);
   - CAMP_OUT is honoured;
   - CAMP_DEATH is honoured by its slot and generation. With DEATH_CHIEF it runs the chief's fall,
     whose pushes are dropped (item 1) and whose pressure bump is state only;
   - an arrival that falls due meanwhile adds its owed slot, which the next live census fills.
3. **The floors** (RP-57): M6 changes BAND_CASUALTIES, which now carries an op. `ingest` and
   `ingest_copy` fold the epoch before `casualties` runs its dedupe, so a casualty the party has
   counted already still raises the floor. The sweep has the case: with the dedupe ahead of the
   fold, its Pages and Fresh paths stay at epoch 0. CHIEF_KILLED goes through M6's `chief_killed_by`
   with the record's epoch, after the fold. CHIEF_STATE, now read, CAMP_CENSUS's BODY_CRYSTAL and
   CAMP_DEATH's DEATH_CHIEF name no party epoch. RP-57's table says so.
4. **CHIEF_STATE** (RP-100): latest-wins, never journaled, given to the engine at once (rtsim-jni
   `observe_state`), so it is newer than every replayed record. While the outbox replays it now
   waits for `replay_done`, the latest for each faction, as RP-68's world save does. Before, a
   replayed FACTION_FOUND's census byte undid it, or it found its faction unknown and was dropped.
   It is held while the host replays too (`st.replaying`), since the engine's own replay begins only once
   the conflict is in the world, after a replayed FACTION_FOUND.
5. **RP-66:** M6 adds no `Order::ConflictCommand` test in Rust. Its GameTests order reconciles
   and materializes through `ConflictKit` in the middle of a session, after no restart. One ordered
   while a server's start replays waits for the replay end.
6. **Formats:** M6's RTC7 and META 6 are kept, with no migration (nothing is released). The
   rtsim-view fixtures hold conflict meta page 5, and the five tests that read them fail until they
   are regenerated: rtsim-view's `the_core_gametest_save_draws_every_layer_and_writes_nothing`,
   `the_create_gametest_save_draws_every_layer_and_writes_nothing`,
   `every_gametest_save_carries_and_matches_each_modules_hash` and `a_hash_that_differs_is_flagged`,
   and rtsim-host's `gametest_worlds_open_in_the_host_with_their_saved_state` (the conflict's pages
   dropped). The worktree's run-gametest worlds are deleted.
7. **Residual (RP-101):** M6's `chief_unit_lost` picks its stack by M6's own rule, not RP-27's,
   and takes from a stack for a copy too. It cannot matter today, because a boss band is its one
   unit.

**The M7 merge** (feat/mowzie-m7 at 2a80b4f0 merging M6 at 6d6c6658, then main; each its own merge
commit; RP-102, RP-103). M7 branched from M6 at 58a5551a, before M6 merged main, so the first merge
brings slice C, RP's gate and SEC-1's sections with M6's tail. Seven files conflicted in it:

1. **engine.rs:** slice C's held arrivals and M7's held passes took the same names (`held`,
   `Held`, `hold`). M7's are renamed `rail_held`, `rail_held_oldest`, `rail_held_n`, `HeldPass`
   and `hold_pass`, in engine/rail.rs too, which merged clean. Both sides' fields, stats and
   imports are kept. In `arrive`, M7's track arm (the band waits at the spot until its patience
   runs out, noted "track") comes after the envoy, scout and peaceful arms and before the arm that
   holds (§6.1).
2. **persist.rs:** M7's RTC8 and META 7 (MZB-344) are kept, and M6's casualty ops (MZB-589) end
   the party record, as the RTC7 line of the format history now says. The outbox's `iter` and
   `restore` replace the commands vector; the held passes go under their new names.
3. **rtsim-jni engine/conflict.rs:** `rail_call` without M7's `resend_notices_if_due` (MZB-375),
   one of the MZB-141 triggers RP-2 removed. The replay ends at the top of engine.rs `journaled()`,
   before industry's journaled entry and so before the call (RP-102).
4. **rtsim-jni tests/protocol.rs:** both sides' imports; M7's rail tests read the outbox's
   `replaying()`, not M5's field.
5. **docs/boundary.md:** M6's CHIEF_STATE text and M7's RAIL_PASS bullet, both.
6. **scenario/results** sim-conflict.json and sim-mowzie.json: regenerated on the merged head.

What the merge checked:

1. **The gate:** M7 adds no `Cmd` kind and no push site; engine/rail.rs pushes nothing. Its routes
   (`rail_pass`, `rail_call`, `let_go_due` and the meter's `Ev::Rail` wake) reach main's pushes
   only through `rail_gate`, `set_emission_for`, `refresh_rates` and `plan_feud`: a climb's
   FACTION_NOTICE drops while replaying and goes again from the feud at the replay end, and a war
   party's embody returns early (§6). §4.4 lists it.
2. **The floors** (RP-57): RAIL_PASS names a faction, a train and an owner, no epoch, and
   `party_epoch` is None for it. The industry's TRAIN_ARRIVED and TRAIN_UNWATCHED reach the
   conflict through `rail_call` with a train only. RP-57's table says so.
3. **Decisions while replaying** (RP-102): all of M7's are re-derived from the same records as
   live, so the meter, its source and the feud's climb come out as they did. A band bound for a
   track asks no watcher at its arrival and is never held. The one residual is the owner of an
   ownerless pass, read from presence, which holds no positions from the window (RP-103).
4. **RP-66:** M7 adds no `Order::ConflictCommand` test in Rust. Its GameTests
   (RailFrostmawGameTests, TrainSamplerGameTests, SamplerBenchGameTests and TerritoriesGameTests)
   drive the engine through `ConflictKit` in the middle of a session, after no restart.
5. **Formats:** RTC8 and META 7 are kept, with no migration (nothing is released). M9 took the same
   numbers, so whichever of the two lands second bumps them again. The rtsim-view fixtures, main's
   (core from c67da61, create from fe87a92 at the second merge), hold conflict meta page 5 (main's
   RTC6), so the five tests that read the conflict's pages fail until they are regenerated:
   rtsim-view's `the_core_gametest_save_draws_every_layer_and_writes_nothing` and
   `the_create_gametest_save_draws_every_layer_and_writes_nothing` ("meta page version 5, this
   build reads 7"), `every_gametest_save_carries_and_matches_each_modules_hash` and
   `a_hash_that_differs_is_flagged` (no conflict hash), and rtsim-host's
   `gametest_worlds_open_in_the_host_with_their_saved_state` ("conflict: pages dropped"). The
   worktree had no run-gametest worlds left to delete.
6. **C2ME** (MZB-662): none of M7's four GameTest classes waits on `getEntity == null` or
   `!hasChunk` as "saved and unloaded". RailFrostmawGameTests forces its chunks and loads them with
   `getChunk`, SamplerBenchGameTests waits on the bench's own state, and the other two wait on
   nothing of the kind. Nothing to convert.

The second merge, main at 8bfd5742: SEC-1 S5-2 (shared kinetic networks, passive heat, the glass
solve's network inputs, fixtures regenerated from c67da61 and fe87a92) and slice H (the herds'
replay gate, RP-80 to RP-89). One file conflicted, docs/choices-mowzie-build.md, where both sides
added RP records after RP-69: the M6 and M7 merges' RP-100 to RP-103 stay in slice C's list, and
slice H's section follows. Main changed no file of rtsim-conflict, rtsim-kernel or rtsim-host. Its
rtsim-industry engine.rs (the network registry and `drain_networks`) and M7's one function there
(`last_departure_of`, MZB-312) are apart. Slice H's rtsim-jni changes are the herds' (the arm
leaves the herds out of `player_left`, RP-87; a herd command waits for the replay end, RP-85; the
herds' gate counts in the replay end's log line) and leave the conflict's replay end where §2.2 and
RP-102 put it, at the top of `journaled()`. Its filler records are the herds' own (56 bytes), not
BAND_CASUALTIES, so MZB-589's 72 bytes change none of its hand-over counts. Nothing in this merge
reaches a conflict gate, epoch or replayed decision, and the two sims are the same as after the
first merge under the filter.

**The M9 merge** (feat/mowzie-m9 at 2f3941d0, M9 on M6's 58a5551a, merging M6 at 1290d0b7, then M7
at 39c819f1, each its own merge commit, 788820c9 and 56c55d8d; RP-130 to RP-132, MZB-670 to
MZB-672). Main at e75265e7 came in with M6, which had merged it (1290d0b7), so the third merge had
nothing to bring. The first merge brings slice C, SEC-1 S5-2, slices H and S and M6's review fixes;
thirteen files conflicted in it, and slice C's outbox.rs, new to M9, needed M9's row:

1. **engine.rs:** `Conflict` keeps M9's `placing` beside slice C's replay fields; M9's `replaying`
   field goes and M9 reads `outbox.replaying()`. `spawn_wave` takes M6's `wave_fill`, with M9's
   `chief_stack` gating MEMBER_CHIEF and BAND_CHIEF beside M6's BAND_FIGHT. `begin_replay`
   replaces `session_start`. In `resend_notices` the hunts held go with the kept notices, pushed one
   by one through the gate (RP-131).
2. **outbox.rs** (no conflict): OWED_HUNT's verdict row in the exhaustive `gate`; M9's two
   TEMPORARY guards in `owe_hunt` and `hunt_applied` are deleted (§7.4).
3. **engine/camp.rs:** M6's crystal off the kept chief body's BODY_CRYSTAL bit, with M9's
   `chief_camp` guard and per-slot line flags; `chief_short`'s CHIEF_BODY through the gate.
4. **boundary.rs:** HUNT_APPLIED is in `party_epoch`'s list of kinds that name none (RP-57).
5. **persist.rs:** M9's numbers, with M6's casualty-op note on the party record; the second merge
   bumps them.
6. **rtsim-jni engine.rs and engine/conflict.rs:** the shared `answered_again` calls the
   conflict's too; a CHIEF_STATE is held while the host replays (RP-100), else `give_latest`;
   `journaled` sets `obs_seq` first, then counts M9's HAVEN_CHEST.
7. **Camps.java:** M6's stray index, census hold and recount holds and its `PendingRecords.Entry`
   records, on M9's heir pair and `removeBody`; a body loaded elsewhere counts in FACTION_FOUND's
   `home` only when pooled (MZB-670).
8. **CampKind.java, ConflictBridge.java, MicRtsimMowziesMobs.java, docs/boundary.md:** both
   sides.
9. **rtsim-jni tests/protocol.rs:** the filler BAND_CASUALTIES of `replayed_hunt` carry their op
   (MZB-589; RP-132).
10. **scenario/results/sim-mowzie.json:** regenerated on the merged head (below).

The second merge brings M7's rail (RTC8 / META 7, RP-102 to RP-104). Eight files conflicted:
engine.rs (both re-exports, wheel events `CampHeir` 9 and `Rail` 10, both sides' stats and their
names; `later` is TRAIN_PASSED alone; `ingest` runs M7's `rail_pass` and M9's `haven_chest`,
`trade_booked` and `chief_paid`), persist.rs (both imports, wheel tags, stats, resident tails),
tables.rs (the faction row's heap counts M9's hunts and M7's meters), rtsim-jni engine/conflict.rs
(the module doc names both; `translate` keeps M7's RAIL_PASS ticks and M9's `tribute_items`),
protocol.rs (the later-kinds test uses a TRAIN_PASSED, since RAIL_PASS and CHIEF_PAID are both
live), docs/boundary.md and docs/choices-mowzie-build.md (both sides) and sim-mowzie.json.

- **Formats:** the merged M9 is RTC9 and META 8 (MZB-448, persist.rs's format history): M7 took
  RTC8 and META 7 first and lands first. Wheel tags 0 to 10 each once (M9's 9, M7's 10); no page
  table on either side; the faction row is M9's hunts and camp in `put_faction`, then M7's rail
  meters; the resident state ends with M7's held passes, then M9's placing; the stats are M9's
  three, then M7's seven. Save and load read the same fields in the same order. No migration.
- **The eight points:** the gate covers every M9 push (§4.4's M9 paragraph; RP-131), RP-57's table
  has M9's kinds (none names an epoch) and `ingest_copy` drops a HAVEN_CHEST copy (RP-130); M9's
  decisions while replaying, RP-66, MZB-589's fillers, the arrive order and the chief kill paths
  need no change (RP-132). M7's resident size pin takes M9's hunts list in the base (MZB-672).
- **GameTests:** M9's four unload waits (two built a `List.of` of null UUIDs and threw at the
  sequence's build, two passed at once on null ids) wait on the objects' own `isRemoved()` through
  `MowzieKit.removed` (MZB-671), in a commit of their own.
- **Sims** (wall_ms, fights_unheld and fight_record_gap_max filtered): sim-conflict is M7's but for
  the conflict state hashes (the state carries M7's and M9's fields) and M9's three stats at 0;
  against M9 it adds M7's seven rail stats at 0 and M6's own changes since 58a5551a (an invasion
  home with 20 left and a raid with 8, from slice C through M6's merge of main, and wire bytes 3,104
  for 3,080, MZB-589's op on three BAND_CASUALTIES); against main, M6's stats too. The world hash
  is d450929438383fb2 in all five. sim-mowzie has M9's runs and checks (all ok) and M7's rail runs
  and checks (all ok) together; against M7 its one changed line is M9's (the plunder at node 4
  finds 0 items spare, not 1: plunder takes leather and wool only), against M9 nothing but M7's
  runs, hashes, stats at 0 and slice labels; against M6 and main both sides' runs, and against main
  M6's changes as on M6. No check of any parent changed its outcome.
- **Fixtures:** main's, conflict meta page 5, so the five tests that read the conflict's pages fail
  on META 8 as on M7's merge: rtsim-view's `the_core_gametest_save_draws_every_layer_and_writes_nothing`
  and `the_create_gametest_save_draws_every_layer_and_writes_nothing` ("meta page version 5, this
  build reads 8"), `every_gametest_save_carries_and_matches_each_modules_hash` and
  `a_hash_that_differs_is_flagged` (no conflict hash), and rtsim-host's
  `gametest_worlds_open_in_the_host_with_their_saved_state` ("conflict: pages dropped").
- **The check's fixes** (RP-133, RP-134, MZB-672 amended, MZB-673). A raid held while replaying
  owes its hunt under the op the session that went on sent, and a replayed HUNT_APPLIED for its
  district ends the hold first, so the hunt kills once (RP-133, §6.1, §7.4). A tribute stack
  answered after the pages' save is booked from its replayed answer, so the census does not put
  it down again (RP-134, §4.4). The faction row boxes M9's hunts (216 bytes, MZB-672). No format
  change: RTC9 / META 8.

**The M8 merge** (feat/mowzie-m8 at 959ada91, M8 on M7's pre-merge 2a80b4f0, merging M7 at 39c819f1,
then M9 at 7153b7fe, then main at 5a45b9e6, each its own merge commit, a64599d6, 74dd20ae and
1885aa7d; RP-150 to RP-153, MZB-690 to MZB-692). Main at e75265e7 came in with M7 and M9, which had
both merged it; main moved on while the merge ran, and the third merge brings SEC-1 S6 (station
joins, IndustryData format 4, Java only: no Rust, sim or conflict file), with no conflict. M8 adds no
`Cmd` kind: slice C's outbox, new to M8, had AMBUSH_SPOT's and
TRAIN_ROBBED's rows already, and §4.4's M8 paragraph has the push sites.

The first merge brings slice C, M6 at 6d6c6658 and main (M7's merge, RP-102 to RP-104). Five files
conflicted:

1. **rtsim-jni engine/conflict.rs:** slice C's `replay_end` replaces M8's `resend_notices_if_due`.
   M8's queue filter, which dropped AMBUSH_SPOT beside NOTICE at the replay end, becomes the second
   kind of slice C's debug assertion: the gate drops both while replaying, so none is queued there.
2. **docs/boundary.md:** the deferred kinds, both sides.
3. **FrostmawGameTests.java:** both sides' imports and tests, with M7's `RtsimTests.io`.
4. **sim-conflict.json and sim-mowzie.json:** M7's, regenerated at the end (below).

Three more did not compile on slice C and needed the merge's hand: engine/rail.rs `resend_lurks`
pushes each AMBUSH_SPOT through the gate (the outbox has no `extend`); tests/lurk.rs's
BAND_CASUALTIES carries an op (`next_op`, MZB-589) and its restarts call `begin_replay(true)` (the
fight test then `replay_done`); rtsim-cli sim/mowzie/lurk.rs asserts a trip runs after the replay
(slice C has no `replaying` to set and restore), and its restart is `begin_replay(true)` then
`resend_notices`.

The second merge brings M9 (RTC9 / META 8, RP-130 to RP-132, MZB-670 to MZB-672). Five files
conflicted:

1. **engine.rs:** `raid_district` adds M9's survivor hunts and takes the loot as M9 does. `later` is
   empty (M8 built TRAIN_PASSED, M9 the grove's kinds), kept for a later slice (MZB-691), and
   `ingest` runs `rail_pass`, `train_passed`, `haven_chest`, `trade_booked` and `chief_paid`.
2. **ConflictBridge.java:** M9's owed hunts beside M8's lurks, both cleared at a session's start and
   failure; `spawn` keeps M8's hold points and M9's extras, and a lurk refused at its placing
   discards the extras made with its bodies too (MZB-692); the tick runs the owed hunts, then holds
   on `idle()`, which counts M8's lurks and roars.
3. **ConflictRecords.java and MicRtsimMowziesMobs.java:** both sides (the record sizes; the
   Frostmaw's lurker beside the grove's chests and strength).
4. **docs/boundary.md:** AMBUSH_SPOT's row M8's, CAMP_RECONCILE's M9's (the mask action); the
   resend paragraph both; no kind deferred.

Beyond the conflicts: persist.rs takes the formats (below); rtsim-jni engine/conflict.rs's module
doc says no kind is later; protocol.rs's `later_slices_records_are_counted_and_dropped` becomes
`no_record_is_deferred_and_each_reaches_the_engine` (deferred 0, and its TRAIN_PASSED, for no
party, reaches `train_passed` and counts stale).

- **Formats:** the merged M8 is RTC10 and META 9 (MZB-690, persist.rs's format history): M9 took
  RTC9 and META 8 and lands before M8. The magic is `RTCA` (four bytes; A for ten). M8 persists only
  its five stats (a lurk is derived from the party's phase, kind and target, MZB-357): the stats are
  M9's three, then M7's seven, then M8's five (lurks, lurks_ended, passes, passes_watched,
  lurk_robberies). Save and load read the same fields in the same order. No migration.
- **The eight points:** the gate covers every M8 push (§4.4's M8 paragraph); M8 adds no `Cmd` kind,
  so the verdict test's sample already has both of its kinds. RP-57's table has TRAIN_PASSED read
  now, and `ingest_copy` drops a TRAIN_PASSED copy after its fold (RP-150). A replayed pass is counted
  and rolls nothing (RP-151, RP-154); after two crashes every pass of the second session up to
  its world save can be stale while replaying (RP-152, a residual). The lurk's camp claim meets no camp hold (RP-153). RP-66, MZB-589's fillers,
  the arrive order, M7's `rail_held` and the FactionRow pin need no change (RP-153).
- **GameTests:** M8's (FrostmawGameTests from `aLurkPuppetTakenByAPlayerIsLetGoLikeAnyOrdinaryFrostmaw` on,
  RailFrostmawGameTests, RailDirectionsGameTests and MowzieGameTests'
  `aLurkRefusedAtItsPlacingClaimsNoCampBodyAndTheSetOutIsATake`) have no wait of either kind: every
  `List.of` is built inside a `thenExecute` or a poll, every wait is a lambda (`io`, `paced`), and
  the one `getEntity` check (FrostmawGameTests "the copy joined") is a `thenExecute` check right
  after `addLegacyChunkEntities`, no unload wait. Nothing to convert.
- **Sims** (wall_ms, fights_unheld and fight_record_gap_max filtered): sim-conflict against M8 has
  M6's changes since M8's base (an invasion home with 20 left and a raid with 8; wire bytes 3,104
  for 3,080, MZB-589's op on three BAND_CASUALTIES), M9's three stats at 0 and the state hashes;
  against M9 M8's five stats at 0 and the hashes; against M7 both; against main M6's and M7's stats
  too. The world hash is d450929438383fb2 in all five. sim-mowzie has M8's lurk checks (17 on run 1,
  6 on run 8) and M9's runs and checks (runs 5, 6 and 12 built, the grove chaos with heirs on run
  10), all ok. Against M8 its one changed line is M9's plunder at node 4 (0 items spare, not 1);
  against M9 nothing but M8's checks, stats at 0, slice labels and hashes; against M7 both; against
  main M6's changes too (blocked on 9 of 16 days, the camp chaos counts), as on every parent. No
  check of any parent changed its outcome. Main at 5a45b9e6 has e75265e7's sims (S6 changes no
  scenario file) and no Rust of S6's, so the sims were not run again after the third merge.
- **Fixtures:** main's, conflict meta page 5, so the five tests that read the conflict's pages fail
  on META 9 as on the M7 and M9 merges: rtsim-view's
  `the_core_gametest_save_draws_every_layer_and_writes_nothing` and
  `the_create_gametest_save_draws_every_layer_and_writes_nothing` ("meta page version 5, this build
  reads 9"), `every_gametest_save_carries_and_matches_each_modules_hash` and
  `a_hash_that_differs_is_flagged` (no conflict hash), and rtsim-host's
  `gametest_worlds_open_in_the_host_with_their_saved_state` ("conflict: pages dropped").

**The M8 fixes and the second-pass merges** (feat/mowzie-m8 from 14c896ee; RP-154 to RP-156).
The merge check's P2: a replayed TRAIN_PASSED is counted and rolls nothing (RP-154, §4.4), so the
replay never robs a train the live session may have robbed; RP-151, RP-152 (every pass of the
second session up to its world save, not one train's roll) and RP-57's 0x080D row say so, and
`lurk_robberies` counts only robberies told to Java. Then M9's merge-check fixes at 42e8fcc3 (RP-133,
RP-134, MZB-672 amended, MZB-673; two doc conflicts; the FactionRow pin is M9's 216 again with
hunts boxed, RP-155) and main at a11311aa (M6 landed with its P3s, its fixtures at RTC7 / META 6,
slice S's ships; no conflict, RP-156). No format change: RTC10 / META 9. The sims regenerated on the
merged head equal the committed ones but for wall_ms. The fixture tests that read the conflict's
pages fail on META 9 against main's META 6 fixtures ("meta page version 6, this build reads 9"), the
same five as on the earlier merges (listed under Fixtures above); the next fixture regeneration
(the core and create GameTest saves) brings them to RTC10 / META 9.

## 3. The flag

### 3.1 Where it is set and cleared (rtsim-jni)

- `State.replaying: bool`, not saved.
- **Set** true at the top of the SESSION_START arm (§2.1). Before the first SESSION_START (`start`
  to the first hand-over) it is false: no module has reset yet, and the first SESSION_START sets it.
- **Cleared** only by the replay end, `shared::replay_end` (§3.2), once a session, by the first of
  the three triggers (§1.1, §2.2): the top of `journaled()` for a record whose sequence is at or past
  `session_first`; the step's top once `applied_through + 1 >= session_first`; the SESSION_START
  arm's end once `max(applied_through + 1, jf) >= session_first` (RP-29). Not the predicate latched:
  RP-29's journal check can clear it while `applied_through + 1 < session_first`.
- A module switched on in the middle of a session (MODULES, U10) resets as today
  (`reset_due`, `reset_if_due`); its replay end runs at once when `st.replaying` is false, else at the
  session's replay end.

### 3.2 The replay end (rtsim-jni engine/shared.rs)

- `shared::replay_end_if_due(st)`: when `st.replaying` and `applied_through + 1 >= session_first`,
  run `replay_end(st)`. Called at the step's top and at the SESSION_START arm's end.
- `shared::replay_end_before(st, seq)`: when `st.replaying` and `seq >= session_first`, run
  `replay_end(st)`. Called at the top of `journaled()`.
- `replay_end(st)`: `st.replaying = false`; one info log line (the hand-overs the replay took,
  §2.2, the records applied while replaying, and each module's commands dropped and skipped while
  replaying, §4.3), the same counts in the JNI stats; then each module's `replay_end(st)` in the step's order
  (dragons, ships, herds, conflict). Each module's `replay_done` runs at the replay clock (§3.6); the
  held part of the hand-over's time runs after it, at the next run of the clock.
- **The records applied while replaying** are those `dispatch` takes as Apply before the replay
  end. Not counted: the JOURNAL_GENs (never dispatched), the stale records the pages hold
  (AlreadyApplied, a REASSERT's included), the copies dropped as not new (`copy_is_new`), and the
  op-id duplicates (DuplicateOp, a copy's included). Counted: a copy of generation 0 that conflict
  refuses (RP-21), since it is dispatched as Apply and refused in the module.
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
- **The bound** (RP-49). The replay starts from the last durable save, the last one Java asked for:
  the budget's own saves are pending generations, which a crash rolls back (PF15). Taking no page,
  the replay keeps every row its records and the window's own events touch, so its resident rows
  are bounded by the rows the crashed session touched between its last durable save and the crash,
  plus, after a session that crashed before its replay end (§6.1's "More than one crash"), the rows
  that session's own records touched. The window is Java's save cadence, not the budget. The live
  session's budget saves held it near the budget and a step, so the replay can pass the live peak
  by the window's rows less the budget, which grows with the window: not a constant. Measured in
  `a_replays_resident_peak_is_the_rows_touched_since_the_last_durable_save` (20 camps found a
  hand-over under a 4,096 B budget, no save asked for): live 5,776 B at the peak in both; replayed
  28,528 B after 100 camps (the session touched 28,616 B), 56,792 B after 200 (56,880 B). Normally
  the window is empty or one vanilla autosave interval: Java asks for a save on every overworld
  `LevelEvent.Save` (BridgeEvents.java:175-186; 6,000 ticks, scaled by `/tick sprint`), and save-off
  stops the journal write too. The worst case is every paged row of every module, the old unpaged
  footprint: for most modules only after a lost save request (fixed in 63d1b4fa: the oldest pending
  request is kept) or one of the causes the engine logs (RP-55): a save Java asked for that is not
  made, an engine that stops simulating, an earlier session past the pages that ended with no save.
  The herds hold the most: 32 herds a page and one herd's event dirties it, so their replay's rows
  grow fast with the window
  (`a_replays_herd_rows_are_the_rows_touched_since_the_last_durable_save`, outside TEST mode: 95,540
  B against 186,606 B unpaged after 2,400 ticks, 191,730 B against 203,970 B after 96,000). TEST
  mode reads every page of each paged module back for its state hash at a start and at each save
  (main's, RP-55), so a test that measures a replay's rows runs the replayed engine outside it.

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
  queued, with every unit after it, and taken at the first step after the end. A conflict command
  waits the same way, as it goes around the gate (§4.4, RP-66). The rest run no clock.

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
- the replay end's log line gives each module's dropped and skipped totals, counted since its
  engine started (not reset by a session's replay, for conflict and herds alike).

The counts live in the module's JNI stats, not saved.

### 4.4 Conflict (rtsim-conflict boundary.rs `Cmd`, main 3391a5d9; with M5, main 4ac4801a)

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
| Reconcile | drop | never | answers a CAMP_CENSUS of this session with spawn, adopt and take lines (MOWZIE.md §4.9); M5's `send_lines` orders only a camp that is in (`is_in`), false all through a replay: `begin_replay`'s `camp_session_start` makes every camp out and a census replayed only marks its camp out (RP-64) |
| TrainRobbed | keep | drop | a roar (RP-12) |
| ChiefBody | keep | never | goes only ahead of a stack, a spawn line or a hand-back, none of which is made while replaying |
| March | drop | send | latest wins per agent (MOWZIE.md §4.16): the abstract party's march |
| OwedHunt (M9) | drop | drop | the faction's hunts are state: a raid replayed owes its hunt again, which the plugin had; every hunt still held goes at the replay end (`resend_notices`), after a journaled HUNT_APPLIED cleared its own (§7.4, MZB-449, RP-131) |

The queued column is the old `session_start` filter exactly (agent kinds, Territory, Notice and
Reconcile dropped); M5's `session_start` had the same filter, which the merge left to the gate
(RP-64). OwedHunt is the one kind that filter would keep (it names no agent): M9's
`session_start` kept it, and the gate drops it, since the replay end sends every hunt held
(RP-131). After the M5 merge, Territory, Notice, Settled, Envoy, Materialize, Steer, Recall and
Reconcile have push sites (engine.rs about 1152, 2137, 2368, 3081, 4059, 4375, 4830, 4880, 5065,
5127; engine/camp.rs about 629, `send_lines`, which M5 wrote as a `Vec` extend around the gate and
the merge routes through `push`). M5 adds no `Cmd` kind: CAMP_RECONCILE and CHIEF_BODY were in the
table already, and the engine pushes no CHIEF_BODY yet. AmbushSpot, TrainRobbed, ChiefBody and
March have no push site; M7, M9 and M10 bring them, and their merges meet the exhaustive match.
M7 (the M7 merge, §2.7) brings none of them and no other push site (below); M8 (the M8 merge)
brings AmbushSpot's and TrainRobbed's (below).
M6 (the M6 merge, §2.7) adds no `Cmd` kind and no push site either. Its routes reach main's:
- `camp_arrival`, `camp_refill` and the census's crystal mend order lines through `send_lines`,
  which orders only a camp that is in;
- the chief's fall (`chief_killed_by`, `chief_killed_at_camp`) sends Recall through `go_home` and
  `end_fight`, and replans through `plan_feud`.
So the Reconcile row's "never" covers M6's SPAWN lines (an arrival, a refill, a recount), and
the Recall row covers the chief's.

One route goes around the gate, and it is a test's: rtsim-jni engine/conflict.rs `order_command`
(`Order::ConflictCommand`, TEST mode only) puts a command of any kind straight into the host's
queue. Its verdict is "wait": a unit holding one, and every unit after it, is held to the replay end
with RP-33's orders that run the clock (`waits_for_the_replay_end`, RP-66), so no test's command
goes while the replay runs, a "never" kind included.

**M7's rail (the M7 merge, §2.7; RP-102, RP-103).** M7 adds no `Cmd` kind and no push site: rail.rs
pushes nothing itself, and its routes reach main's pushes through `plan_feud`. As the note written
before the merge asked:

- **The caller.** M7's rtsim-jni engine/industry.rs `journaled` calls `conflict::rail_call` on
  TRAIN_ARRIVED and TRAIN_UNWATCHED for a train the conflict holds passes of. M7's own replay-end
  trigger in `rail_call` (MZB-375) is deleted with the other MZB-141 triggers (RP-2): the end at the
  top of engine.rs `journaled()` runs before `industry::journaled` is dispatched, so before the
  call. M7's two MZB-375 protocol tests pass with nothing in `rail_call`.
- **What it reaches.** A RAIL_PASS (`rail_pass`), a train's call (`rail_call`), a held pass let go
  after a day (`let_go_due`) and a meter's wake (`Ev::Rail`, rebuilt from saved state, fired in the
  replayed window under the replay clock, §3.6) count legs and run the push gate (`rail_gate`), whose
  push (`set_emission_for`) recomputes the feud's rate and replans it. While replaying all of it is
  state re-derived from the same records as live: the climb's FACTION_NOTICE drops at the gate and
  goes again from the feud at the replay end, a war party sets out abstract, and a sighting of it
  waits (§6). No "never" kind is decided.
- **RAIL_PASS** has no op id and no copy: Java journals each once, at once
  (`ConflictBridge.railPass`, no holder reasserts it), and the pages and a contiguous replay apply
  each record once. It names no epoch (RP-57's table). A copy, were one ever sent, is dropped by
  `ingest_copy` (RP-104).
- **Track targets.** A raid or invasion bound for a track (`Target::Track`) asks no watcher at its
  arrival: `arrive`'s track arm comes before the arm that holds (§6.1) and only waits out the band's
  patience at the spot, and `resolve` raids nothing there. So it is never held, and decides while
  replaying exactly as live. Java is never asked about a track: BAND_MATERIALIZE and BAND_STEER carry
  the spot as a plain position (y 64, BAND_SURFACE), so a track Java has removed since changes
  nothing.
- **Presence** (RP-103, a residual): an ownerless pass counts against the player with the most time
  near it (`presence.top_of`), which while replaying holds no position of the window; P14's
  `owner_of` reads it the same way.

**M9's camp, hunts and tribute (the M9 merge, §2.7; RP-130 to RP-132).** M9 adds one `Cmd` kind,
OwedHunt, with its row above, and new push sites for two kinds that had rows:

- **OWED_HUNT** goes from `owe_hunt` (a raid on an unwatched district, in `resolve`) and from
  `hunt_applied` (the rest of a hunt whose replaced op was answered). Both push through the gate,
  which drops them while replaying; the books change as live. M9's two TEMPORARY guards (no push
  while `replaying`, MZB-449 R1) are deleted with the merge, as §7.4 asked, and `resend_notices`
  sends each hunt still held from the faction's state once the replay has caught up.
- **CAMP_RECONCILE** gains M9's lines, the heir's pair among them (MASK of a warrior at home and
  SEAT_HEIR, one order, `camp_heir`). engine/camp.rs builds a CAMP_RECONCILE in one place,
  `send_lines`, which orders only a camp that is in, so the Reconcile row's "never" covers every
  line M9 adds; `camp_heir` asks `is_in` itself too.
- **CHIEF_BODY** goes ahead of a chief SPAWN line in `send_lines` and from a census that found the
  chief short (`chief_body_cmd`, which asks `is_in`). Never while replaying: the ChiefBody row's
  "never" holds.
- **Tribute stacks** are no conflict `Cmd`: `place_tribute` books a TRANSFER_TO_CONTAINER that
  rtsim-jni writes through the core outbox (`placings`, `push_with_op`) under a deterministic op
  (`tribute_op`, of the camp, the stack's index, the good and the amount), only for a camp that is
  in. So none is decided while replaying; the census that brings the camp in after the replay
  places the next stack. A stack the session that went on decided and had answered after the
  pages' save is booked from its replayed COMMAND_RESULT: no placing holds it, so `tribute_placed`
  (or `tribute_failed`) looks the op up among the camps that owe tribute in the replay, at the
  indexes from the camp's count, and books it as that session did (RP-134). The census then
  decides the next index, not that stack again from a stock that has moved. A stack in flight at
  the crash, whose answer the journal does not hold, is decided again at its index: the same op
  when the stock and what is owed have not moved, which Java answers DUPLICATE_OP if it put it
  down (MZB-446); another op when they have, which Java puts down, a gift of at most that one
  stack per camp (RP-134's residual).
- **Raptors and the envoy party** use main's BAND_MATERIALIZE and ENVOY_MATERIALIZE sites, with
  §6's early returns; M9 adds no site of either.
- **Observations, not commands:** HAVEN_CHEST, TRADED, CHIEF_PAID and HUNT_APPLIED come from Java.
  TRADED and CHIEF_PAID carry Java op ids (`Obs::op`, read by the engine's `first_time` and by
  rtsim-jni `op_of`), so a replayed or copied one is skipped by the op-id dedupe. HUNT_APPLIED's op is the engine's own and is booked by the state of
  the faction's hunts: a second answer finds no hunt (MZB-449). HAVEN_CHEST has no op id and no
  holder carries one (GroveChests journals it at once); it is the chest's state, latest wins, so
  `ingest_copy` drops a copy, as it drops RAIL_PASS (RP-130).

**M8's lurk (the M8 merge, §2.7; RP-150 to RP-153).** M8 adds no `Cmd` kind: AMBUSH_SPOT and
TRAIN_ROBBED had their rows above, and the verdict test's sample has both. Its push sites, all
through `push` and the gate:

- **AMBUSH_SPOT** goes from `lurk_start` (`arrive`'s Track arm, before the wait), from `lurk_end`
  (`go_home`, the one way a lurk ends, the removal) and from `resend_lurks` (`resend_notices`, after
  `replay_done`). Queued "drop" and "drop" while replaying: Java's lurk table lives for one session,
  so a lurk begun or ended by a replayed record is state, and the replay end says every lurk still
  standing once, at the party's epoch after the floors are lifted. `resend_lurks` drops any
  AMBUSH_SPOT still in the outbox first, as `resend_notices` drops a NOTICE; with the gate's drops
  there is none (rtsim-jni's `replay_end` asserts the same of its queue).
- **TRAIN_ROBBED** goes from `train_passed` when the robbery took anything, which a live session
  alone makes (a replayed pass rolls nothing, RP-154). Queued "keep" and "drop" while replaying: it
  is the station's roar (RP-12), and none is decided then.
- **A robbery's goods** are no `Cmd`: `train_passed` calls `ambush`, whose `infra.raid_train` takes
  them from industry at industry's present, as M7's `Ev::Ambush` does (industry is not held). A
  replayed pass is counted and never calls it (RP-154): the live session may have robbed the
  train, and the replay's attack would be another op that Java applies again.
- **The swipe and the roar** are Java's: the swipe is a held lurk puppet's strike when a carriage
  comes in its reach (`ConflictBridge`'s strike through the `Lurker`), and the roar plays at the
  station of a train TRAIN_ROBBED names. Neither sends a record the engine acts on.
- **The hand-off** is main's: a lurking party seen is embodied by BAND_MATERIALIZE through §6's early
  returns ("never" while replaying), and its puppets stand at the hold point Java finds from the
  AMBUSH_SPOT. A camp body is handed to a puppet only from this hand-over's SET_OUT lines, which no
  lurk meets (RP-153).
- **TRAIN_PASSED** is an observation with no op id and no holder (ConflictBridge journals it at
  once). `ingest` folds its epoch while replaying (RP-57), then `train_passed` counts it and, while
  replaying, rolls nothing (RP-154); a copy is folded and dropped (RP-150).

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

As built (slice H, RP-80, RP-85): rtsim-ecology `herds/outbox.rs`, `gate` and `Outbox`, the engine's
`outbox` field; `engine.rs` has one push site per kind (the Materialize in `seen`, `steer`,
`despawn`), each through `Outbox::push`. The queued column is the old `session_start` filter exactly
(everything went). The "never" row cannot be reached: `seen` returns before it decides while
replaying (`Outbox::defer`, §6, RP-83). One route goes around the gate, and it is a test's:
rtsim-jni `engine/herds.rs` `order_command` (`Order::HerdCommand`, TEST mode only) puts a command of
any kind into the link's queue. Its verdict is "wait", as for conflict (§4.4): it is held to the
replay end with the orders that run the clock (`waits_for_the_replay_end`, RP-85). The other ways a
command could reach the herds' queue (`Order::HerdFound`, the world-model founding, `reject`,
`introduce_herds`, the dragons' prey path) are audited in RP-85: each is gated or cannot run while
replaying.

The books, per dropped kind (RP-80): a Steer decided while replaying names a group of the last
session, which Java's new session does not have; the group's `steer` field moves and `stats.steers`
counts the decision, as live, and nothing else is touched (no pool, slot, puppet or owed amount
follows a steer). A Despawn marks the group `leaving` and counts in `stats.despawns`; it frees
nothing by itself, since the group's end is its HERD_DEMATERIALIZED (a replayed one applies as
recorded) or, with none, the replay end, which makes every group abstract with its members seen at
the replay clock. Neither leaves a count that a live run would have settled differently.

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

**As built (slice S, RP-90).** rtsim-ships `outbox.rs`: `gate()` is an exhaustive match on `Cmd`
with no `_` arm, and the commands are a private `Vec`, so a push around the gate does not compile.
The table above is the code: the only "never" kind is Materialize (the early returns of §6 make it
impossible by construction; a bug is counted, kept for the log, not queued, and a `debug_assert!`
with no release panic). Every other kind drops while replaying, and every kind drops from the
queue at `begin_replay`. The books stay right for each dropped kind (RP-90): a Timetable, Steer,
Depart and Despawn are orders to a body of this session, and no body of this session exists
while replaying (the bodies of the last one are gone, §5.1), so the only reader of a dropped order
is a body that is not there; a SetCargo sets a hold on a body, and the hold the books keep is
`cargo_of(&World, ..)`, pure of any command, so dropping it changes no number (the engine's own
holds and the world's flows are untouched); the Materialize is never decided, so there is none to
drop. Other routes: `Order::ShipCommand` is held (RP-96, below); a copy goes to `ingest_copy`
(RP-97).

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
  - **A newer epoch:** `epoch` = the record's, `embodied` = true, `spawned` cleared, `wave` 0, and
    the record's waves are filled. An older embodied epoch's bodies leave the books unfolded, as
    live `start_fight` drops them (RP-46). The bridge journals the older epoch's
    BAND_DEMATERIALIZED before the newer one's BAND_MATERIALIZED (`ConflictBridge.materialize`
    ends the old party first), so a replay normally finds the party no longer embodied at the older
    epoch; when it still is, a later record of that epoch counts against nothing, as it did live.
    (As first written those bodies folded into a recalled stack of that epoch, MZB-131's walk-off,
    which counted such a record where live did not. A live recall folds as before.)
  - **The same epoch, at or above the party's `wave`:** the waves from the party's `wave` to the
    record's are filled (normally one). Below it: nothing; the pages already hold that wave's fill.
    Not embodied: nothing, the body is gone; but a fighting band not embodied is one whose first
    wave Java refused (RP-54), and its later wave's record embodies it again and is filled, as
    live `wave` embodies the band when it commands that wave (RP-58, RP-61).
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
- **Herds** (HERD_MATERIALIZED: agent, epoch, spawned; RP-82): the herd is embodied at the record's
  epoch, with the members the decision picked: the engine's own `members::pick` of `min(spawned,
  alive)` from the herd as it is now (seen members first, then fresh), `since` the time the record
  is applied, no checks booked (no `Ev::Check`), the chart the identity, and no player: the slot is
  `NO_PLAYER`, which no puppet count carries (the counts end at the replay end anyway, §5.6). Not
  every alive member: the record's `spawned` is what Java placed. An older group of the pages' whose
  end was not journaled is released first, its members staying seen. An epoch at or below the
  group's, or at or below the herd's own epoch, changes nothing; a newer one replaces it. A group
  honoured on a world whose frame is not flat counts a later obstacle of it as late (the chart is
  not known). Only while replaying, only from the pages, never a copy's.
- **Ships** (SHIP_MATERIALIZED: agent, epoch, entity): `epoch` = the record's; `set_embodied(id, true)`.

An epoch at or below the agent's changes nothing. A live *_MATERIALIZED is handled as today. A
copied one embodies nothing (RP-8). One not honoured (a copy, a fresh engine's, RP-31) raises the
agent's epoch floor (§5.3).

### 5.3 Epochs are never reused (RP-7, RP-30)

An agent's next epoch is above every epoch the engine has seen for it: the pages', and those of
every replayed record and every copy that names one, whatever its kind and whatever the record does
after (RP-57): a *_MATERIALIZED (§5.2), and as much a refusal, a despawn, a loss, an answer or a
loot, for an agent held or not, embodied or not, in the pages or not yet. Conflict folds them at the
top of `ingest` while replaying and of `ingest_copy` always (`Obs::party_epoch` lists every kind),
and rtsim-jni folds a copy it refuses (RP-21). The rule is the same for dragons, herds and ships,
whose slices (§9.2, §9.4, §9.5) build their floors: every kind of theirs that names an epoch, not
only the *_MATERIALIZED. Each module's
`epoch + 1` becomes `max(epoch, floor) + 1` (conflict: `seen` and `start_fight`; dragons, herds and
ships at their materialize). The floor is an in-memory map per engine, agent to the highest epoch
seen that the agent's own `epoch` does not hold (RP-30). Not the epoch itself: raising a party's
`epoch` while it is embodied at an older one would make `party_of` and the despawn miss that body's
records. At `replay_done`, when every agent is abstract, each floor folds into its agent's epoch and
the map is cleared, so the pages carry it. A floor raised live (a copy after the replay end) waits
for the agent's next materialize. A restart loses the map; harmless, since no record of one body
is ever applied to another (RP-8). Each module that has epochs has its own test: an epoch decided
after the pages and answered in the journal is never decided again after a crash.

**Herds (RP-81).** `Herds::floors` is a map from agent to floor. `ingest` folds the record's epoch
(`Obs::group_epoch`, an exhaustive match over all eight kinds) first while replaying, and
`ingest_copy` always. A herd's next group takes `x.epoch.max(floor).wrapping_add(1).max(1)`: the
floor sits under the wrap rule, so a floor above the herd's own epoch survives it and the wrap to 0
still lands on 1. `seen` spends the floor (`floors.remove`) when it decides. At `replay_done`, with
every group abstract, each floor folds into its herd's epoch; the floor of an agent with no herd yet
stays in the map for that agent's first group (the conservative reading; the map is in memory only).
rtsim-jni folds every record that decodes before its switched-off check, so the herds off for a session,
or on with no herd yet (their replay begins at the first one), still raise the floor; a record that
does not decode names no epoch and raises none.

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
  to run (§6.1); the CHIEF_STATE records held meanwhile applied, the latest for each faction
  (RP-100); then the kept notices go (`resend_notices`).
- **Dragons:** every embodied dragon resumed where it was last seen (`resume(.., left: true)`),
  `by_entity` and `chests` cleared, every player's interest leg cancelled.
- **Herds** (RP-86, RP-87): every embodied group abstract (clips' legs cancelled, members'
  `seen.last` set to the replay clock), every player's interest and `pos` forgotten, `puppets` 0, the
  epoch floors folded (§5.3). The herds are left out of the arm's `player_left` (rtsim-jni
  `shared::session_start` hears only the modules that say so, `Hear`), so these players' work is the
  replay end's and the arm calls nothing of the herds. A herds player's interest is the world's area
  leg, which the arm's `leave_area` cancels (`LegIndex` handles are generational, so a stale cancel
  is safe): `replay_done` cancels it again for the same reason and sets it to None. Not run unless a
  replay runs (`replay_done` is a no-op otherwise); rtsim-jni calls it from the replay end's three
  triggers, and at once when the session starts with nothing to replay.
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
  (by party). BAND_MATERIALIZED: the floor only. Every other kind: as `ingest`. Every copy that
  names an epoch raises the floor to it first, live or replayed (RP-57).
- **Herds** (RP-84): `Herds::ingest_copy`, called by rtsim-jni when `st.applying_copy` is `Some`.
  Every copy raises the floor first. HERD_KILLED, HERD_SHEARED and HERD_TAKEN go by member key and op
  id already, so they apply as `ingest`; every other kind (HERD_GROUP_STATE, HERD_MATERIALIZED,
  HERD_SPAWN_REJECTED, HERD_DEMATERIALIZED, HERD_OBSTACLE) raises the floor and counts as late: each
  matches a body, and a copy's body is gone (RP-8). A copied KILLED or TAKEN has no epoch check, so
  a copy for epoch 4 can remove a member of a live epoch-5 group: accepted as §10 item 3, the group's
  count one fewer than its bodies until its next despawn. Those three kinds, and the honour of
  §5.2, act only on a herd that existed when the session began (`Herds::replay_bound`, the herd
  arena's next index at the herds' `session_start`, whether or not they run; `begin_replay` only
  lowers it, RP-84): nothing founds a herd while replaying, and a herd founded after, on a handle
  the last session used for another herd (the pages lack what it founded after its last save), is
  not the one a copy names (such a copy counts late; its floor still counts).
- **Dragons:** nothing is carried; a copied DRAGON_MATERIALIZED would raise the floor only.
- **Ships:** nothing is carried, and every copy, whatever its kind, raises the floor only
  (`Ships::ingest_copy`, RP-97): the handle a copy names may be another ship than the earlier
  session's, so a copy of a sinking or of loot is lost rather than booked to a stranger (§10).

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

A walk-off death takes from its own epoch's stack only, so it then caps `spawned` and every recalled
stack of that unit at the alive units, as a copy does (§5.7, RP-25; the re-review's item 7). Without
the cap a despawn of the newer epoch first, with every body it placed, counts the survivors above the
alive and undoes the death. The bridge despawns the old epoch first, which clamps `spawned` itself;
the cap makes the order not matter.

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

- **The hold.** The party goes into an in-memory map `held`, party to its arrival tick, its poll's
  plan and its members then (`Held`, RP-60). The first hold sets `bored_at` as `arrive()` does
  today; a later one keeps it.
- **The poll.** Its Ev::Party is scheduled again with the same plan, `REPLAY_HOLD_TICKS` later
  (PLACEHOLDER 20 world ticks), so that pages saved in the middle of the replay hold an event for
  the party and a restart from them arrives again, and once more at the hold's slack (below), where
  the hold ends. The poll must not move the route: `party_event`'s Out arm increments `seg` before
  it calls `arrive()` (about L2826), and the wheel's events are saved, so no new `Ev` kind (a page
  format change). Recommended: the held check in the Ev::Party arm answers a held party's poll
  before `party_event`, and the first hold puts `seg` back by one, so that a poll that does reach
  `party_event` (at the slack, after the replay end, or after a restart that lost the map) lands on
  the last segment again and arrives. The slice checks `party_pos` and `book_leg` with `seg` one
  short and tests the restart. Since RP-42 the host takes no page while replaying, so only
  engine-level tests save mid-replay and restart from those pages; the restart case stays as a
  defence (the re-review's item 4).
- **Honoured.** A replayed, honoured BAND_MATERIALIZED of a held party (§5.2) is the last session's
  watched fight. The party takes the state `start_fight` leaves, without its commands: Fighting;
  `until` = `min(arrival + watched_fight_ticks, bored_at)` from the held arrival tick;
  `fight_strength`; its leg booked from the record's time to `until`; `seg` on the last segment; an
  Ev::Party at `until` with a bumped plan, which voids the poll; the epoch and the fill as §5.2
  says; `seen_by` None (`wave` returns early while replaying, and the replay end clears it). It
  leaves `held`. Only a fight's record (BAND_FIGHT in its flags, echoed from the command: RP-56)
  inside the hold's slack is honoured so (RP-48). Its replayed
  casualties count and rout as live ones, and the fight ends at `until` inside the window (§3.6) or
  becomes Waiting at the replay end, as every Fighting party does
  (`a_held_arrivals_replayed_fight_counts_its_casualties_once`, rtsim-jni tests/protocol.rs).
- **A sighting** (RP-56). A held party's BAND_MATERIALIZED without BAND_FIGHT, a player seeing the
  band after the decision resolved unwatched live, only embodies it (§5.2) and keeps the hold, so
  the slack's poll resolves the decision unwatched as of the hold. For an arrival that poll is
  `arrive()`'s unwatched branch, which recalls the band the sighting embodied, where the session
  that went on kept it embodied (case A's cost, decided by the lead): the recall reaches no one
  while replaying (BAND_RECALL drops, §4.4), the crashed session's bodies are gone, the casualties
  of that epoch still count through the recalled stack, and the replay end makes every band
  abstract anyway. A siege's round (`wait_over()`) recalls nothing. A sighting and a fight
  journaled in one step (a sighting at T-1, the watched arrival at T) are read each by its flag.
- **Refused.** A replayed BAND_SPAWN_REJECTED of a held party's first wave at a newer epoch, inside
  the slack, is that fight too: live `start_fight` took the epoch and the refusal left the band
  fighting unembodied. The party takes the honoured state with the record's epoch, not embodied,
  nothing placed, `wave` 1 (RP-54).
- **Not honoured: the slack** (RP-48). A held party with no such record keeps its poll until the
  hold's slack, `REPLAY_HOLD_SLACK_TICKS` (600 world ticks, 30 s) after its arrival or round. No
  hard bound exists on the delay from a watched decision to its BAND_MATERIALIZED (Java journals it
  as it applies the command): normally 1 to 2 ticks, but a spawn group held by Java's spawn quota
  (`DEFAULT_SPAWN_QUOTA` 8 a tick, which holds every later command), a full command buffer or a
  lagging engine stretches it; 600 covers 30 s of stall. The live engine measures it
  (`ConflictStats::fight_record_gap_max`, and rtsim-jni logs each new largest, warning at or past
  the slack: RP-51), so the value can be set from data: to the fight's first record, its first
  wave's BAND_SPAWN_REJECTED when Java refused it, which the hold reads as the fight (RP-62). A
  hold that no record answered by then is
  read as not fought: the poll at the slack ends the hold and runs `arrive()` or `wait_over()`,
  which, still replaying and past the slack, take the unwatched branch at once (recall, `resolve`,
  then wait or go home), as the session that went on did. The decision runs with the members the
  hold kept (RP-60): a record replayed inside the hold that took some (a despawn's survivors fewer
  than were placed; a death, though that one ends the hold first, below) came after the decision
  in the session that went on. What the decision cost (its raid's dead) then comes off the members
  left. The replay clock follows the records
  (§3.6), and the events up to a record's time run before it, so that poll runs before any record
  at or past the slack and after every record before it. A later sighting of the party finds no
  hold and embodies the band as `seen()` did live. A later fight record (BAND_FIGHT, a newer
  epoch) finds no hold either, at the slack exactly as past it: it only raises the epoch floor
  (§5.3), so its casualties and loot count against nothing, as the abstract raid's were booked at
  the poll. So does a later BAND_SPAWN_REJECTED of a newer epoch's wave 0 (RP-63), the fight's
  only record when Java refused every wave; the record carries no flags, so a refused sighting's
  is read the same (its epoch has no bodies: only the stat moves). It is counted once per epoch in
  `ConflictStats::fights_unheld`, and rtsim-jni warns of each (RP-56).
- **Moved inside the slack** (RP-59). A replayed record that would move a held party ends its hold
  first, before it is applied: a BAND_CASUALTIES or CHIEF_KILLED for it at its epoch or an older
  one (a wipe-out or a rout sends it home), a copy of a BAND_CASUALTIES at any epoch (it counts at
  any), a DIFFICULTY that makes the world Peaceful, for every held party (`stop_war_parties`), and
  a HUNT_APPLIED for a district a held party of its faction raids (its target, or a district within
  64 blocks of a position target), oldest hold first (RP-133): the held raid owes its hunt before
  the answer that spends it.
  The hold's end runs `party_event` at the record's tick with the hold bypassed inside the slack
  too (`hold_ending`), so the decision resolves unwatched as the session that went on resolved it
  at the hold's tick, and the record then applies to the band as it did there. Before, the record
  sent the band home first, which bumped the plan, left the hold's poll stale and lost the
  decision's abstract raid. A record of a newer epoch is the held decision's own fight, answered
  as above. A BAND_DEMATERIALIZED ends no hold: Java ends a band's older epoch with one just before
  its fight's BAND_MATERIALIZED (`ConflictBridge.materialize`), so it would end the hold of a fight
  about to be honoured.
- **Not honoured: the replay end.** If the replay ends inside the slack, `replay_done` clears
  `held`, and the poll's next firing runs `arrive()` or `wait_over()` as today. Positions are routed
  only after the modules run in a step (§6 above), so that first decision normally finds no watcher
  and resolves abstractly: the reviewer's rule (resolve only when no replayed BAND_MATERIALIZED
  embodied the party), once.
- **From the arrival or round.** Either way the decision is taken as of the hold's tick: an arrival
  keeps its arrival tick, its route's last time, and its patience runs from there (RP-45); a siege's
  next round is that tick plus its interval, and its patience is checked at that tick (RP-48): by
  the walk home's label when the round ends the band (RP-52), and by `held_since`'s inference from
  the pages after a restart, which takes a fight record past the boredom but inside the slack as
  the map does (RP-53). So a later round, and a fight at it, falls where the session that went on
  had it. Live the hold's tick
  is the event's own, and nothing changes.

**Losses accepted** (lose, never double):

- A crash right after a watched arrival, before Java journaled its BAND_MATERIALIZED: the fight Java
  may have shown is resolved abstractly at the replay end.
- A fight recorded at or past the slack (RP-48, RP-51, RP-56). The replay resolves the decision
  unwatched at the slack, and the late record raises the floor only: the watched fight is booked
  as the abstract raid, its own casualties and loot are dropped. Until RP-56 it embodied the band
  as a sighting and counted them again.
- A refused sighting inside the slack (RP-54's residual, accepted by the coordinator).
  BAND_SPAWN_REJECTED carries no flags and has no free bytes (24 u64 agent, 32 u32 epoch, 36 u32
  wave, 40 u64 cmdSeq, 48 bytes), so a replayed refusal of a newer epoch's first wave inside a held
  fight's slack is read as the fight's refusal: the hold ends early, at the refusal, and the band
  fights its round unembodied; the fight's own BAND_MATERIALIZED then finds no hold and is lost as
  RP-56 says (its casualties and loot count against nothing). The books hold, one resolution; only
  live parity is lost, and only for a sighting refused inside a held fight's slack. Not taken: the
  refusal's wave is a u32 and the command's a u16, so its high half (bytes 38 and 39) could carry
  the echoed flags as BAND_MATERIALIZED's 44 does (boundary.md's BAND_SPAWN_REJECTED row); the
  coordinator wants no change to that field now.
- A record that moves a held party before its fight's own record (RP-59): a death or a chief's
  of a band embodied before the decision, journaled between the fight's command and its
  BAND_MATERIALIZED (normally 1 to 2 ticks), ends the hold as unwatched, and the fight's record
  then finds no hold and is lost as RP-56 says. The books hold, one resolution.
- Time: an unwatched decision while replaying is resolved at its slack, up to
  `REPLAY_HOLD_SLACK_TICKS` after its tick, so the band starts home that much later than in the
  session that went on (a siege's next round does not move).
- None from M7's track-target parties, which §4.4's note listed before the M7 merge: a band bound
  for a track asks no watcher at its arrival and is never held (RP-102).

**More than one crash** (the fix round's item 8). A held arrival stays held until its slack or the
replay end of the first session that reaches one, whichever comes first. A session that crashes or
closes before its replay end takes no page (RP-42, §3.2), so the next opens on the same pages and
replays the last live session's records and then the crashed session's own: the arrival comes again
in the replayed window, is held again, and is decided once, at its slack or after that session's
replay end (`a_held_arrival_across_a_second_crash_before_the_replay_end_is_resolved_once` and its
clean close case, rtsim-jni tests/protocol.rs: one resolution in S1 and S2 together, the band's
books as in S0). A decision a crashed session made at the slack goes with it (no page, its commands
deferred) and is made again from the same records. What is lost is time: an arrival resolved after
the replay end of the session that completes one is resolved later than its tick in the session that
went on, so an unwatched band walks home later. The crashed session's in-memory replay state
(`held`, the floors and the fill's deaths, §5.3) goes with it and is rebuilt from the same records.

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

As built (the M9 merge, RP-131): both guards are deleted and both pushes go through the gate; the
re-send in `resend_notices` is kept, behind main's NOTICE filter only. M9 had deleted its own two
OwedHunt filters at its re-check (MZB-449 R1), so none was left to delete. The gate's queued "drop"
differs from M9's `session_start`, which kept a queued OWED_HUNT: the replay end sends every hunt
held, so a queued one would only have gone twice.

A raid held while replaying (§6.1) owes its hunt when its hold ends, at the slack, before a record
or at the replay end, later than the session that went on decided it. The hunt's op hashes the
decision's tick, so `owe_hunt` takes the hold's tick (`hold_ending`), and a replayed HUNT_APPLIED for
a held raid's district ends that hold first (RP-133): the hunt is owed under the op that session
sent and its replayed answer spends it, where before the answer found no hunt and the hunt went
again under a new op at the replay end, killing twice. Live ops are M9's.

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
  window, which resolves abstractly once, at the hold's slack (RP-48; after the replay end if that
  comes first).
- **(c) RP-24:** a watched fight kept in the pages (Fighting at the save), whose casualties are
  delivered in the replay's third hand-over (fillers before them), and whose end, walk home and
  arrival (`home()`, which pools the party) all fall after those casualties' game time but inside
  the window, on a short route home. Today the first replayed hand-over's step runs the clock past
  the arrival, the party is pooled, and the casualties drop. After: they count, and the faction's
  pool after the replay is the no-crash run's. The event inside a later hand-over's window is what
  the test is for; the audit's 3,000-block march had none. Control: no crash. A second case, the
  one-hand-over replay, is no control for RP-24: before df53fe38 it failed too, on RP-26's anchor.
- **(d) RP-25:** a raid of 6 all placed, a copied BAND_CASUALTIES, then Java's despawn with 6
  survivors: 5 alive (6 without the cap). Control: a live BAND_CASUALTIES and a despawn with 5
  survivors: 5.

And one for each of the other two review items, with its control:
- **RP-26:** a watched, kept band across the arm: still embodied after SESSION_START, no RECALL in
  any buffer, and its replayed casualties count off its kept stack. Control: no crash, the same
  counts. Written after RP-5 and before RP-26, where it fails.
- **RP-27 (MZB-131):** a band recalled at epoch e and embodied again at e + 1; a casualty of epoch e
  takes from the recalled stack and caps `spawned` at the alive (amended by the re-review's item 7;
  it left `spawned` whole); one of e + 1 takes from `spawned`. Control: casualties of e + 1 only.
  Written first; fails on main (the e casualty is dropped). With the cap, a second test: e's
  walk-off death, then e + 1 despawned first with every body placed, keeps the death (failed before
  the cap: 12 alive, not 11); control, e despawned first.

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

Built (RP-80 to RP-89, docs/choices-mowzie-build.md "Slice H (herds)"): every step above, plus
`Order::HerdCommand` held to the replay end (RP-85), and a floor for all eight observation kinds
(RP-81, which supersedes the herds row of RP-57's "no floor yet" note).

Tests (rtsim-jni tests/protocol.rs `mod herds`, with the engine's in rtsim-ecology tests/herds.rs):
1. `a_long_replay_with_a_player_at_the_herd_never_sends_a_killed_sheep` (scratch :3533: members
   [0, 9, 10] sent again today). Control: the one-hand-over replay. On the base slice C's hold already
   keeps a player's sighting out of the replay, so the pair passes there too; they stay as the
   regression guard of the gate and add the group's size and the gate's "never" count.
2. The epoch test (today: 1 live, and 1 again after the restart):
   `a_group_decided_after_the_pages_does_not_have_its_epoch_decided_again_after_a_crash`. Control:
   `a_group_in_the_pages_does_not_have_its_epoch_decided_again_after_a_crash`.
3. A replayed HERD_DEMATERIALIZED of a group materialized after the save captures its members'
   positions: `a_replayed_capture_of_a_group_decided_after_the_save_lands_on_its_members`. Control:
   `a_replayed_capture_of_a_group_the_pages_held_lands_on_its_members`.
4. The gate tests: rtsim-ecology `every_command_kind_has_its_replay_verdicts`,
   `a_never_kind_decided_while_replaying_is_counted_and_not_queued`; the engine's tests for the
   floors, the honour, the early return, the replay end and the copies.
5. The ending epoch: `an_epoch_that_ended_in_the_replayed_window_is_not_decided_again`. Control:
   `an_epoch_that_ended_after_the_pages_held_it_is_not_decided_again`.
6. The bypass: `a_herd_command_ordered_while_replaying_waits_for_the_replay_end`. Control:
   `a_herd_command_ordered_without_a_replay_is_written_at_once`.
7. Copies: `a_replayed_copy_of_a_materialization_is_not_honoured_but_its_epoch_is_counted`. Control:
   `a_replayed_materialization_the_journal_holds_is_honoured`.
8. Floors when the herds are off: `a_record_replayed_while_the_herds_are_off_still_raises_their_epoch_floor`.
   Control: `a_record_replayed_while_the_herds_are_on_raises_their_epoch_floor`.
9. The herd's own epoch: `a_replayed_materialization_at_or_below_the_herds_own_epoch_is_not_honoured`
   (rtsim-ecology, controls inside).
10. Handles: `a_copy_for_a_herd_the_pages_lacked_does_not_land_on_the_herd_founded_in_its_place`
    (rtsim-jni). Control: `a_copy_for_a_herd_the_pages_held_lands_on_it_after_a_founding`. Also
    after a switch-on (`a_switch_on_after_a_founding_does_not_make_the_new_herd_one_the_last_session_had`)
    and with no herd at the session start
    (`a_copy_for_a_herd_the_session_began_without_does_not_land_on_its_first_herd`, control
    `a_copy_for_a_herd_the_session_began_with_lands_on_it`). The engine's:
    `a_copy_for_a_herd_founded_after_the_session_began_lands_on_nothing`,
    `an_engine_with_no_herd_at_the_session_start_takes_no_copy_for_its_first_herd`,
    `a_herd_founded_in_the_replay_window_is_not_honoured`.
11. Floors with no herd: `an_epoch_replayed_with_no_herd_yet_is_not_decided_again_for_the_first_herd`.
    Control: `an_epoch_replayed_with_the_herd_in_the_pages_is_not_decided_again`.

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

**As built (slice S, RP-90 to RP-99 and RP-120 to RP-122).** The records are in docs/choices-mowzie-build.md, "Slice S
(ships)".

- **The gate** (§4.7, RP-90): rtsim-ships `outbox.rs`; one "never" kind (Materialize), the rest drop.
- **The replay's start and end** (§5.1, §5.6, RP-91): `begin_replay(from_pages)` changes no body;
  `replay_done(&mut World)` makes every ship abstract, cancels every player's interest leg, folds
  the epoch floors into the ships' epochs, then lays up a ship that was kept sailing only for its
  view (`lay_up_if_waiting`, §2.4.6a rule 7).
- **Floors** (§5.3, RP-57, RP-92, RP-99): nine ships kinds name an epoch (State, Materialized,
  SpawnRejected, Dematerialized, Damaged, Sunk, Boarded, Captured, CargoLooted); Fight, PortFound
  and HavenFound name none. `Obs::ship_epoch` is an exhaustive match, so a new kind does not
  compile until it says. A floor for a ship the engine does not have yet is kept past the replay end
  (a port or haven that joins after, or a ship the replay allocated later). The fold comes before
  every drop (RP-122): a module switched off for the session, ships with no dock yet and an
  engine that has not begun its replay all fold, keyed on the session's replay.
- **The honour and every by-agent lookup** (§5.2, RP-93, RP-120): only on a ship that existed when
  the session began (`replay_next`, taken once by `Ships::begin_session` from `ships::session_start`;
  `begin_replay` keeps the smaller bound and the first call's `from_pages`). The bound is not the
  honour's alone: `ship_of`, `ship_of_op`, both ships of a Fight, the capturing ship and the doer
  all go through `named_ship`, because the arena's handles are allocated in order and the replay
  allocates in its own (the network built, ports and havens joined, inside the replayed window).
  A record about a handle at or above the bound only raises the floor.
- **Early returns** (§6, RP-94): `seen`, the `SeenPirate` event, `materialize_timetabled`,
  `materialize_pirate`.
- **Pirate coast raids** (§6.1, RP-95): `coast_raid` is skipped whole while replaying (not held:
  nothing retries it); the answer, its reasons and the residual are in the record.
- **The blind watcher** (§6.1, RP-95): no player is routed to the ships until the replay end, so
  `point_in_view` and `relaunch_in_view` answer true while replaying (every dock is watched). A
  relaunch then waits for the slot's next stay, a shut haven's refound reschedules at
  + `base_days` and bumps the haven's epoch, a navy's assault waits for its next period, and a
  direct `port_raid` is not made; none is held to the replay end, each looks again at its own next
  time and the first look after the end is judged by the players routed by then.
- **Test orders** (§3.6, §4.4, RP-96): `Order::ShipCommand` waits for the replay end.
- **Copies** (§5.7, RP-97): rtsim-jni routes a REASSERT to `Ships::ingest_copy`, which folds the
  epoch and does nothing more for every kind; a copied HAVEN_FOUND joins no haven node to the
  world either.
- **The link** (§3.2, RP-98): `replay_end` logs the counts; the ships' pages, the outbox and
  every other format are unchanged.

The paths that hand the ships engine a command or a decision, each with its verdict (RP-96):

| Path | Verdict |
|---|---|
| The engine's own events (`run_until`, from `shared::run_to` and from each ingest) | commands go through the gate; decisions that embody return early (RP-94) |
| `Ships::ingest` of a journaled record (rtsim-jni `journaled`, `observe_state`) | commands through the gate; the epoch is folded first (RP-57), before the module's switch is asked (RP-122) |
| A copy (REASSERT) | `ingest_copy`: the floor only (RP-97) |
| `ships::simulate`'s network build, `add_port`, `add_haven`, `add_navy`, `connect` (from journaled PORT_FOUND and HAVEN_FOUND, and from the step) | run while replaying, as live; they decide no body; the handle-allocation limit is RP-93 and RP-120 |
| `reject` (the host's own SPAWN_REJECTED for a command it cannot write) | only after a queued Materialize, which the gate never lets through while replaying |
| `Order::ShipCommand` | held to the replay end with the units after it (RP-96) |
| `Order::Phase1Advance` and the other clock orders | held by slice D (RP-74), not in S's `matches!`; the merge takes the union |
| `lift_closures` (a switch-off) | unchanged: it queues no command |
| `with_ships_mut` | a test accessor, not a route in the game |
| The `Viewer` impl's `player_left` | not overridden for ships, so the arm's leave is a no-op for them (U8) |
| Sightings (`shared::simulate`) | wait for the replay end (§6) |

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
   at the end naming the count and the records applied while replaying, §3.2), so a slow replay shows
   up in GameTests and soaks (RP-2).
3. **A copied band death after a crash** (a dying member's drops carried its record) shrinks the band
   by one even when it is in view again; its bodies in the world may then be one more than its count
   until the next despawn. *Recommended: accept.* The other way leaves a dead member alive for good. **Decided:
   accept**; the extra body clears at the next despawn.
   That holds only with RP-25 (§5.7): the copied death also caps the band's count of placed members
   at its alive ones, and without the cap the next despawn would undo the death instead.
