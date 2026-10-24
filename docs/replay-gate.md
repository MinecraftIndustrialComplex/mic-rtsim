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

S4p: feat/sec1-s4p (its code at 3bff09e5; the branch head 1c1b674f has main 3391a5d9 merged).
RG: feat/reinstall-guard (RG13, RG14). Neither S4p nor RP must land first; whichever lands second does
the merge in §2.7 (RP-17). They meet in four places: the session start, the replay end, the copies
(REASSERT) and the epochs. RG14 meets RP nowhere in code.

### 2.1 Session start

- **Function:** the SESSION_START arm of the observation reader (rtsim-jni engine.rs, the arm that
  calls every module's `session_start`, then `Replay::reconcile`, then sets `session_first`).
- **S4p** adds `st.session_gen_seen = false;` right after `st.session_first = ...`.
- **RP** adds three things to the same arm:
  1. `st.replaying = true;` first, before any module's `session_start`, so that a module reset inside
     the arm never sees the last session's `false` and ends a replay that has not started.
  2. Right after the `session_first` line: `st.replaying = applied_through + 1 < session_first;`.
  3. At the arm's end: `shared::replay_end_if_due(st)`. With nothing to replay, the replay ends here.
- **Overlap:** adjacent added lines after `session_first`; no shared state. Keep both.
- **SEC-1 fact 4 (pinnedGen):** Java sends its newest acknowledged JOURNAL_GEN first at each session
  start, and pages that include it skip it (`stale_skipped` + 1 a session). RP's predicate reads
  `applied_through` alone: a skipped record never moves it, so it never ends or prolongs the replay.
  Nothing in RP reads the first replayed record's content or assumes it carries data.

### 2.2 The replay end (MZB-141)

- **Today** it is conflict's alone, two triggers in rtsim-jni engine/conflict.rs: `journaled` on the
  first conflict record at or past `session_first` (about L530), and `simulate` once
  `applied_through + 1 >= session_first` (about L641). S4p does not touch that file. On S4p alone both
  still work: JOURNAL_GEN is never dispatched to conflict, so the first trigger fires on the first
  live conflict record, and the second at the step.
- **RP** moves both into one transition for every module, `shared::replay_end_if_due(st)`, called at
  three points (RP-2):
  1. **`journaled()`** (engine.rs), first thing after its `save_through` block, for a record whose
     OBS_SEQ is at or past `session_first`. That is before S4p's JOURNAL_GEN branch, its unnamed
     generation branch, its REASSERT branch and `admitted()`. On S4p the first live record is the
     session's JOURNAL_GEN, so the replay ends just before that record is admitted. On main it is the
     first live record of any kind.
  2. **The step's top** (`step_inner`, after the observations are read and before
     `industry::simulate`), when `applied_through + 1 >= session_first`: a replay whose last record
     was the hand-over's last.
  3. **The SESSION_START arm's end** (§2.1).
- **Why the top of `journaled()`, not inside S4p's `admitted()`** (RP-20): the end must run after the
  last replayed record is dispatched and before the first live one is admitted. The top of
  `journaled()` sees every kind (JOURNAL_GEN, REASSERT, the rest), reads only the record's sequence,
  and exists on both branches. Inside `admitted()` it would also be correct but would need the
  sequence passed in and three call sites checked.
- **A gap** (a journal trimmed past the pages, `Verdict::Gap` and `skip_to` on both branches): the
  records below `session_first` that never come cannot hold the replay open. The first record that
  does come, at or past `session_first`, ends it; one that comes below it is replayed as usual.

### 2.3 Copies (REASSERT)

- **Function:** S4p's `reasserted()`. A copy is admitted by its own sequence, judged by
  `copy_is_new(gen, of)`, and when new is dispatched as its inner record, with OBS_SEQ rewritten to
  the copy's sequence.
- **SEC-1 fact 3's two cases, under RP:**
  1. A REASSERT admitted at or past `session_first` (a chunk or entity loaded in this session) is not
     a replay record. The replay end has run before it (§2.2 trigger 1 fires on the REASSERT itself
     when it is the session's first live record).
  2. A REASSERT journaled in an earlier session after its pages, and replayed now (below
     `session_first`), is a replay record, dispatched while replaying.
- **Copies never match a body (RP-8).** In both cases the content is an earlier session's (S4p drops
  copies of the open generation), so its body is gone. A copied *_MATERIALIZED is never honoured: it
  only raises the agent's epoch floor (§5.3). A copied loss is applied by identity (the party and the
  body kind, the herd member's key, the resident), never against an embodied epoch or a recalled
  stack.
- **The copy bit:** RP adds `st.applying_copy: bool` to rtsim-jni's State, set by `reasserted()` just
  before its `self.dispatch(st, &r, seq, verdict)` and cleared just after, and read by the modules'
  `journaled`. A field, not a parameter, so `dispatch()` and every module's `journaled()` keep S4p's
  signatures (RP-8). Before S4p lands nothing sets it; RP's slices add the field and the engine side.
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
  alive units of that body, found by the party alone (any epoch), and touches no stack. S4p's
  judgement is what makes this safe: a copy is new only when its original was never applied, and then
  nothing journaled after the original was applied either (Java's journal survives a crash as a
  prefix), so no despawn of that body counted its survivors.
- **Found on the way:** the same `party_of` also drops a live walk-off death of a band's old epoch
  while the band is embodied again at a newer one (MZB-131's own case). Slice C tests it first and,
  if it fails, finds a live casualty by the party and the epoch being live or recalled (RP-9).

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

Written by whichever lands second (§2.7), in rtsim-jni tests/reassert.rs, each with its control:

1. **Fact 3, case 1:** a live REASSERT of a BAND_CASUALTIES after the replay end (its original lost
   with the journal tail) counts once; a second copy of the same original is dropped
   (`copies_dropped` + 1) and counts nothing. Control: the original in the replayed tail and its copy
   later: counted once, by the original (the copy is not new).
2. **Fact 3, case 2:** a REASSERT journaled in session 1 after the pages and replayed in session 2 is
   counted while replaying, before the replay end; the party's units fall once.
3. A REASSERT holding a BAND_MATERIALIZED, live and replayed: it embodies nothing, and the party's
   next epoch is above it.
4. **pinnedGen:** two restarts in a row; `stale_skipped` rises by one a session, and the replay end
   comes right before each session's JOURNAL_GEN.

### 2.6 RG14

- RG14 drops old-life pending records in Java before they are journaled (SEC-1 fact 5), so the
  engine never sees them. RG touches Java and docs only; RP touches Java only in slice N (Nests and
  DragonBridge's NEST_RENEW). No shared function.
- Old-life records still in Java's journal (the old engine's unacknowledged tail, RG5's open case)
  reach a fresh engine below `session_first`, so RP treats them as replay: nothing decided from them
  reaches Java; a *_MATERIALIZED for an agent the fresh engine does not have is ignored, and one for
  an agent it has (a reused handle) only raises that agent's epoch. RP neither closes nor widens RG5.
- **Slice N meets RG13's engine life:** NEST_RENEW's renewal serial (RP-14) starts again with a fresh
  engine, so Java keeps the engine life beside each nest's last serial and starts a nest's serial
  afresh under a new life (RP-19).

### 2.7 The merge, whichever lands second

1. **engine.rs, SESSION_START arm:** keep S4p's `session_gen_seen = false` and RP's lines (§2.1).
2. **engine.rs, `journaled()`:** S4p restructured its body (JOURNAL_GEN, unnamed generation and
   REASSERT branches, `admitted()`, `dispatch()`, `answered_again()`); RP adds two lines at its top. A
   textual conflict: take S4p's body and put RP's check right after the `save_through` block.
3. **`reasserted()`:** set and clear `st.applying_copy` around its dispatch, one line each side. If
   RP lands first, S4p's merge adds them; if S4p lands first, slice C does.
4. **tests/protocol.rs:** S4p shifts the journal's sequences by one (the JOURNAL_GEN) and
   `stale_skipped` by one a session. RP's jni tests count records against `session_first`, never by
   absolute sequence, so they hold on both; the merger reruns them.
5. **§2.5's tests:** written by the merger.
6. **docs/boundary.md:** S4p edits §3.2; RP edits §2.3 and §5 (items 4 and 6). Different sections.
7. Nothing else: S4p touches no module file (engine/conflict.rs, dragons.rs, herds.rs, ships.rs,
   shared.rs), and RP touches no kernel journal code.
