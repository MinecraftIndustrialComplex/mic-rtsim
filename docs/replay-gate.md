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

S4p: feat/sec1-s4p (its code at 3bff09e5 and a003bfe6, the head; 1c1b674f merges S3, S4b and main
3391a5d9). SEC-1 reviewed this section (its reply is folded in below).
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
- **SEC-1 fact 4 (pinnedGen):** S4p keeps the newest acknowledged JOURNAL_GEN outside the trimmed
  entries (SC130: `Journal.pinnedGen` in Java, `pinned_gen` in rtsim-host) and sends it first at each
  session start. Pages that include it skip it (`stale_skipped` + 1 a session) and `applied_through`
  does not move. Pages older than it admit it as a gap (`skip_to`, then `admit_gen`), which moves
  `applied_through`; its sequence is below `session_first` either way, so the replay stays on. With
  the engine ahead of the journal (S4p's EngineAhead), `replaying` is false at the arm's end and the
  replay ends there, which is right. Nothing in RP reads the first replayed record's content or
  assumes it carries data.

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
- **The copy bit:** RP adds `st.applying_copy` to rtsim-jni's State, set by `reasserted()` just
  before its `self.dispatch(st, &r, seq, verdict)` and cleared just after, and read by the modules'
  `journaled`. A field, not a parameter, so `dispatch()` and every module's `journaled()` keep S4p's
  signatures (RP-8). It holds the copy's generation nonce, `Option<u64>`, `Some` only on the apply
  path: a copy judged not new goes to `answered_again` and is never dispatched. A copy of the open
  generation's own record is always dropped while replaying (`reasserted()` admits the REASSERT's own
  sequence first, which moves `open.last` past the copy's `of`). Before S4p lands nothing sets it;
  RP's slices add the field and the engine side.
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
5. **Generation 0 (RP-21):** two REASSERTs of one BAND_CASUALTIES with nonce 0, live: neither counts,
   and the refusal count rises by two. Control: the same copies with a named, unknown generation:
   the first counts once, the second is dropped.

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
3. **`reasserted()`:** set `st.applying_copy = Some(nonce)` before its dispatch and `None` after,
   one line each side. If RP lands first, S4p's merge adds them; if S4p lands first, slice C does.
4. **tests/protocol.rs:** S4p shifts the journal's sequences by one (the JOURNAL_GEN) and
   `stale_skipped` by one a session. RP's jni tests count records against `session_first`, never by
   absolute sequence, so they hold on both; the merger reruns them.
5. **§2.5's tests:** written by the merger.
6. **docs/boundary.md:** S4p edits §3.2; RP edits §2.3 and §5 (items 4 and 6). Different sections.
7. **S4p's other files**, which RP does not touch: neoforge core Journal.java, JournalSavedData.java
   and engine/StubEngine.java (the pin, the op-id multiset, magic RTS2, the unnamed generation);
   rtsim-host java.rs and save.rs (`pinned_gen`, NBT "pinnedGen"); rtsim-kernel journal.rs; rtsim-jni
   wire.rs; docs/boundary.md §3 and §3.2. tests/reassert.rs has S4p's 9 tests; §2.5's go beside them.
8. **Fixtures:** S4p changes the page format (META v2, table 2 RTJG), so whoever lands second
   regenerates the rtsim-view fixtures after the merge (and deletes the worktree's run-gametest
   worlds), even if RP's own slice changed no format.
9. Nothing else: S4p touches no module file (engine/conflict.rs, dragons.rs, herds.rs, ships.rs,
   shared.rs), and RP touches no kernel journal code.

## 3. The flag

### 3.1 Where it is set and cleared (rtsim-jni)

- `State.replaying: bool`, not saved.
- **Set** true at the top of the SESSION_START arm, then to `applied_through + 1 < session_first`
  once `session_first` is known (§2.1).
- **Cleared** only by the replay end, `shared::replay_end` (§3.2), which runs once a session.
- A module switched on in the middle of a session (MODULES, U10) resets as today
  (`reset_due`, `reset_if_due`); its replay end runs at once when `st.replaying` is false, else at the
  session's replay end.

### 3.2 The replay end (rtsim-jni engine/shared.rs)

- `shared::replay_end_if_due(st)`: when `st.replaying` and `applied_through + 1 >= session_first`,
  run `replay_end(st)`. Called at the step's top and at the SESSION_START arm's end.
- `shared::replay_end_before(st, seq)`: when `st.replaying` and `seq >= session_first`, run
  `replay_end(st)`. Called at the top of `journaled()`.
- `replay_end(st)`: `st.replaying = false`; one log line (the observations replayed, and each
  module's commands dropped and skipped while replaying, §4.3); then each module's `replay_end(st)`
  in the step's order (dragons, ships, herds, conflict).
- A module's `replay_end(st)`: when its `replay_end_due` is set and it runs (`running(st)`): clear
  the flag, call its engine's `replay_done`, take its commands. Conflict then re-sends its kept
  notices (today's `resend_notices_if_due` body). A module that is switched off keeps the flag; its
  reset on switch-on runs both.
- A module's reset (`reset_if_due`) calls its engine's `begin_replay` and sets `replay_end_due`.
- The two MZB-141 triggers in engine/conflict.rs and `notices_due` go (RP-2).

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
  `begin_replay()`, feeds whatever replayed records it wants, then `replay_done()`. The callers
  today: rtsim-conflict tests/mowzie_records.rs; rtsim-ecology tests/herds.rs, persist.rs and
  tribute.rs; rtsim-ships tests/blockade.rs, lifecycle.rs, paged.rs and port_raids.rs; rtsim-jni's
  module resets. (`FakeConflict::session_start` in rtsim-conflict synth.rs is the emulated Java, not
  the engine.)

### 3.5 Cost (D1)

When not replaying: one bool test per journaled record and one per step. While replaying: less work
than today (no routing, no sightings). At the replay end: one pass over what is embodied, which is
today's session start, moved.

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
TERRITORY, as today. Everything today's `session_start` does moves to `replay_done` (§5.6).

### 5.2 A replayed *_MATERIALIZED is honoured (RP-6)

A *_MATERIALIZED dispatched while replaying, and not a copy, is Java's answer to a decision of the
last session. When its epoch is above the agent's, the agent is embodied as the last session had it:

- **Conflict** (BAND_MATERIALIZED: agent, epoch, wave, spawned): if the party is embodied with bodies
  at an older epoch, those fold into a recalled stack of that epoch (MZB-131's walk-off, which the
  older epoch's records may still need); then `epoch` = the record's, `embodied` = true, `spawned` =
  every alive unit. The record names how many were placed, not which, so every alive unit may be in
  Minecraft: casualties count by body kind against the units and then the stack, as live ones do.
  At the same epoch with the party embodied (a later wave), nothing changes.
- **Dragons** (DRAGON_MATERIALIZED: agent, epoch, entity): `epoch` = the record's; the dragon's
  state becomes Embodied (since: the record's time, depart_at never, purpose home, no harass), its
  abstract leg cancelled, `by_entity` and the embodied index updated, as `materialize` does
  without the command.
- **Herds** (HERD_MATERIALIZED: agent, epoch, spawned): `epoch` = the record's; `embodied` with every
  alive member and no player (the puppets count is reset at the replay end anyway).
- **Ships** (SHIP_MATERIALIZED: agent, epoch, entity): `epoch` = the record's; `set_embodied(id, true)`.

An epoch at or below the agent's changes nothing. A live *_MATERIALIZED is handled as today. A
copied one embodies nothing (RP-8).

### 5.3 Epochs are never reused (RP-7)

An agent's next epoch is above every epoch the engine has seen for it: the pages', a replayed
*_MATERIALIZED's (§5.2), and a copy's (conflict's casualty copies raise it, §5.7). Each module's
`epoch + 1` stays; only the floor under it is new. Each module that has epochs has its own test:
an epoch decided after the pages and answered in the journal is never decided again after a crash.

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

Today's `session_start`, moved, plus conflict's `replay_done`:

- **Conflict:** every party not embodied, no `seen_by`, `spawned` cleared, Fighting becomes Waiting,
  every recalled stack dropped, `replaying` false; then the kept notices go (`resend_notices`).
- **Dragons:** every embodied dragon resumed where it was last seen (`resume(.., left: true)`),
  `by_entity` and `chests` cleared.
- **Herds:** every embodied group abstract (clips' legs cancelled, members' `seen.last` set), every
  player's interest leg cancelled, `puppets` 0, `pos` None.
- **Ships:** every embodied ship abstract, `seen_by` cleared, `lay_up_if_waiting`.

### 5.7 Copies, per module (RP-8, RP-9)

- **Conflict:** `Conflict::ingest_copy(w, obs, at)`, called by rtsim-jni when `st.applying_copy` is
  `Some`. A BAND_CASUALTIES copy of generation 0 is refused in rtsim-jni and counted (RP-21).
  BAND_CASUALTIES: the party by agent alone, the units of that body kind fall (capped at those
  alive), no stack is touched, the killer's signal and the rout follow as for a live one, and the
  epoch floor rises to the record's. CHIEF_KILLED: as `ingest` (by party). BAND_MATERIALIZED: the
  floor only. Every other kind: as `ingest`.
- **Herds:** HERD_KILLED and HERD_TAKEN go by member key and op id already; a copied
  HERD_MATERIALIZED raises the floor only.
- **Dragons, ships:** nothing is carried; a copied *_MATERIALIZED would raise the floor only.

### 5.8 The DEPART trap (RP-16)

DragonBridge answers a DRAGON_DEPART for a dragon it does not hold with a DRAGON_DEMATERIALIZED of
reason NOT_EMBODIED that carries only the agent and the epoch. The engine's Dematerialized arm
(rtsim-ecology engine.rs about L3274) reads no reason and takes the zeros: health 0, hunger 0, a rest at
(0, 0, 0) in dimension 0. Fix in slice D: reason NOT_EMBODIED takes no position, health or hunger; the
dragon resumes where the engine had it, as a SPAWN_REJECTED does.

## 6. Sightings wait for the replay end (RP-11)

- **rtsim-jni:** `shared::simulate` returns at its top while replaying, before it takes
  `st.shared.pending`. Player positions keep arriving (latest per player). Dragons' `player_at`
  (engine/shared.rs about L224), conflict's `observe_presence`, `observe_area`, `route_area` and
  the stale players' leave all wait. `present` was cleared at the session start, so the
  mid-session `introduce_herds` and `introduce_conflict` route nothing while replaying.
- **Engines** return early while replaying, before any state changes, from every decision that
  embodies (counted as deferred in the outbox's counts):
  - conflict `seen`, `spawn_wave` (a kept band's next wave, a timeline event), `materialize_envoy`;
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
| Conflict, herds, ships pages | none | C, H, S |
| Dragons pages: nest rows | `renewals: u32` | N |
| Dragons pages: the outbox | NestRenew carries its serial | N |
| Wire: NEST_RENEW | a u32 serial (docs/boundary.md §6.4, DragonRecords) | N |
| SavedData: Nests | per nest the last serial and its engine life | N |
| META | none (META_VERSION 2 is S4p's) | |
| Not saved | `replaying`, `applying_copy`, `replay_end_due`, the gate's counts | C to S |

Nothing is released, so no compat work. After slice N: delete every `neoforge/*/run-gametest/world`
in the worktree before the next GameTest run, and rerun crates/rtsim-view/tests/fixtures.rs; if it
fails on the dragons pages, regenerate the core and create fixtures as 52639c6b did.

## 9. Slices

Before any building: an Opus review of this design (the lead's condition 5). Then one Sonnet
implementer per slice and an Opus review per slice. Order: C first; after C lands, D, H and S may run
side by side (each adds one line to shared.rs's list of replay ends); N after D.

Every slice: `nice -n 10 cargo test --workspace --no-fail-fast`, `cargo clippy -p <touched crates>
--all-targets -- -D warnings` (glass.rs:175 needs `-A clippy::needless_borrows_for_generic_args`),
rustfmt only on files whose base is clean (protocol.rs is not: hand-format). The fake Java sends
player positions only in the first hand-over at each game time. A long replay needs enough fillers
to span three or more hand-overs on a `MIN_CAPACITY` engine (`MAX_RECORD + 128`, about 64 KB):
4,000 small records, not 1,500.

### 9.1 Slice C: the flag, the replay end, conflict

Steps:
1. rtsim-jni State: `replaying`, `applying_copy: Option<u64>` (None until S4p's wiring), each module's
   `replay_end_due`.
2. SESSION_START arm, `journaled()` top, `step_inner` top: §2.1, §2.2.
3. engine/shared.rs: `replay_end_if_due`, `replay_end_before`, `replay_end` (§3.2); the early
   return in `simulate` (§6).
4. engine/conflict.rs: reset calls `begin_replay` and sets `replay_end_due`; `replay_end` calls
   `replay_done` then the notices' re-send; the MZB-141 triggers and `notices_due` go; journaled
   calls `ingest_copy` when `st.applying_copy` is `Some`, and refuses a BAND_CASUALTIES copy of
   generation 0, counted (RP-21); take_commands logs the gate's counts (§4.3).
5. rtsim-conflict: `outbox` module with `Outbox` and `gate` (§4.1, §4.4); every push, the persist
   restore, the session filter and `resend_notices`' retain go through it.
6. rtsim-conflict: `session_start` becomes `begin_replay` (§5.1); `replay_done` per §5.6; the
   honoured BAND_MATERIALIZED (§5.2); the epoch floor (§5.3); `ingest_copy` (§5.7); early returns
   in `seen`, `spawn_wave`, `materialize_envoy` (§6).
7. The callers of `session_start` (§3.4) choose `begin_replay` and `replay_done`.
8. If S4p is on main by then: §2.7's merge, the copy wiring in `reasserted()`, §2.5's tests.

Tests (each scratch test from the audit becomes a real one, with its control):
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
7. The MZB-131 case (§2.3): a walk-off death of the old epoch while the band is embodied at the new
   one. Written first; if it fails today, fixed by finding the party by agent with the epoch live or
   recalled.
8. A module switched on mid-session ends its replay at once (U10), and a session with nothing to
   replay ends at SESSION_START: the existing tests, rerun.
9. The MZB-138, MZB-141 and MZB-142 tests pass. Where an assertion read a recalled stack the fold
   made, it reads the kept embodied state instead; no count is weakened.

GameTests: `neoforge/gametest.sh core carryon` (ConflictGameTests, CarryOnConflictGameTests; core
also runs the herd and phase 1 GameTests that the routing change touches).

Opus review checks:
1. No `_` arm in `gate`; nothing outside the outbox module touches its `Vec`.
2. Every path to an Envoy, Materialize, Reconcile or ChiefBody push returns early while replaying
   (list the callers).
3. `begin_replay` changes no body; `replay_done` does what the old `session_start` and
   `replay_done` did together.
4. The replay end runs once a session, after the last replayed record and before the first live
   one, at each of its three triggers, and with a journal gap.
5. A replayed BAND_MATERIALIZED raises the epoch and embodies; a live or a copied one does not.
6. D1: nothing new per step when not replaying beyond the bool tests.
7. The phase 1 jni tests pass with the routing deferred.

### 9.2 Slice D: dragons

Steps:
1. rtsim-ecology: `outbox` module for the dragons engine (nest.rs and oil.rs push through it), §4.5
   with NestRenew dropped while replaying.
2. `session_start(w, at)` becomes `begin_replay()` and `replay_done(w, at)` (§5.6).
3. The early return in `materialize` (§6); the honoured DRAGON_MATERIALIZED (§5.2); the epoch floor.
4. The DEPART trap (§5.8).
5. rtsim-jni engine/dragons.rs: reset, `replay_end`, take_commands' counts; shared.rs's list.
6. Read, do not change: DragonBridge reports an untagged dragon once per entity per session at join
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
flight abstract and consistent; the DEPART fix touches only NOT_EMBODIED.

### 9.3 Slice N: nest renewals

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

Steps: the outbox and §4.6; `session_start(legs, at)` becomes `begin_replay()` and
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

Steps: the outbox and §4.7; `session_start()` becomes `begin_replay()` and `replay_done()`; early
returns in `materialize_timetabled` and `materialize_pirate`; the honoured SHIP_MATERIALIZED; the epoch
floor; rtsim-jni engine/ships.rs and shared.rs's list; rtsim-ships tests choose (§3.4).

Tests:
1. `damage_to_a_ship_seen_after_a_save_is_counted_after_a_crash` (scratch :2625: live (3, 800, 1),
   after the crash (4, 1000, 0) today). Control: without the crash.
2. The epoch test.
3. The gate tests.

GameTests: none (no ships plugin in Java); rtsim-host's ships runs.

Opus review checks: as slice C's; a kept ship's NextLeg events while replaying drop their commands
and keep the voyage's times; `lay_up_if_waiting` runs at the replay end.

## 10. Questions for Ben

Gameplay trade-offs at the forks, recommendation first. None blocks building: the recommendation is
what the slices build.

1. **After a crash, lose rather than repeat.** Things the last session decided after its last save
   (a "demand settled" message, a robbed-train roar, a worm sortie at a watched base) are not shown
   again, since Java may have shown them already. *Recommended: lose them.* The other way shows some
   twice.
2. **Nothing appears near players until the replay ends.** At a restart the journal's replay takes
   the first hand-overs (a few more for a long journal); bands, dragons, herds and ships near a player
   appear right after it. *Recommended: accept.* Players normally join after it.
3. **A copied band death after a crash** (a dying member's drops carried its record) shrinks the band
   by one even when it is in view again; its bodies in the world may then be one more than its count
   until the next despawn. *Recommended: accept.* The other way leaves a dead member alive for good.
