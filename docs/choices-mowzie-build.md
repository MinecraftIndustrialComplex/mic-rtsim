# Mowzie build: choices

Choices made while building MOWZIE.md §9's slices, where the plan left something open. Ids are
MZB-#, and each slice takes ids from its own range so slices built side by side never collide:
M1 1 to 49, M2 50 to 99, M3 100 to 149, M4 150 to 199, M5 200 to 249, M6 250 to 299, M7 300 to 349,
M8 350 to 399, M9 400 to 449, M10 450 to 499, M11 500 to 549, M12 550 to 599. Each slice writes only
under its own heading. MZB-589 is lent from M12's range to M6 (the BAND_CASUALTIES op id, which rides with
M6's branch); M12 skips it.

## M1 Records

- **MZB-1** (The layouts are this slice's): MOWZIE.md §2 names what each kind carries and fixes
  only BAND_MARCH's size (72 + 8k + 16w); every offset in `docs/boundary.md` §9.3 and §9.4 is this
  slice's, in §9's own pattern: u64 ids first, UUIDs as (most, least) pairs, one small field in aux
  (a count, the flags or the killer kind), variable parts last, reserved fields zero, sizes a
  multiple of 8. `boundary.rs` is the reference codec, with a round trip and a size test per kind.
- **MZB-2** (The planned kind ids): 0x0091 to 0x0093, 0x080C to 0x0814 and 0x8807 to 0x880B, as
  MOWZIE.md §2 proposed them; `git log --all -S` found each only in this branch's commits. Amended
  (review): DIFFICULTY moved to the fresh journaled id 0x0815 (MZB-17), and 0x0093 is retired.
- **MZB-3** (FACTION_FOUND's census): the reserved word at 52 is a u16 count of home bodies at 52
  and a u8 census at 54 (1 chief present, 2 crystal held, as CAMP_CENSUS's flags), 55 reserved; the
  size stays 56. A camp has tens of bodies, so 16 bits is plenty. Nothing reads it before slice M4.
- **MZB-4** (Camp epochs are 64-bit): MOWZIE.md §4.9 makes a camp's epoch a fresh Java op id, so
  CAMP_CENSUS, CAMP_OUT, CAMP_BODIES and CAMP_RECONCILE carry a u64, and CHIEF_BODY's epoch is a u64
  that holds the camp's epoch at home and a party's u32 epoch away.
- **MZB-5** (Strays and the carried-off killer): a stray carries its kind's stray flag
  (`DEATH_STRAY`, `BODY_STRAY`, `PAID_STRAY`) with slot and generation 0, so no slot number is
  reserved to mean "none". CAMP_DEATH's killer kinds are CASUALTIES' 0 to 3 plus 4 taken (Carry On).
- **MZB-6** (One trade memory per census): CAMP_CENSUS carries one traded set, after its bodies,
  tied to one body by its slot and generation (80, 84): the Umvuthi on the throne, the only body
  whose memory the engine keeps (MOWZIE.md §4.5). Every census body carries its UUID, a slot body's
  too, as the plugin keys deaths on the UUID (MOWZIE.md §4.9).
- **MZB-7** (CAMP_BODIES lines by index): each answer line carries the index of the CAMP_RECONCILE
  line it answers, as well as its slot and generation, so two lines on one slot (a take and a spawn)
  are told apart.
- **MZB-8** (CAMP_RECONCILE's line): 32 bytes: slot, generation, body (0 for none), u8 action (1
  spawn, 2 adopt, 3 take, 4 set out, 5 hand back, 6 remove, 7 seat the heir), u8 flags (chief,
  crystal), then two u64s that hold a stray's UUID (adopt, remove) or a party's agent and epoch (set
  out, hand back). The throne spot and direction are in the header, under a flag.
- **MZB-9** (HAVEN_CHEST's free slots): a u32 at 32, as LAIR_CHEST reports them.
- **MZB-10** (NOTICE_SHOWN names its notice's serial): MOWZIE.md lists only faction and player.
  Amended (review): every kept notice has a serial, a u32 at FACTION_NOTICE's 60 (the reserved word;
  the size stays 64, aux stays the rung), and NOTICE_SHOWN's aux echoes it in place of the rung. A
  kept notice is cleared only when faction, player and serial match, so an answer to an older notice
  cannot clear a newer one even on the same rung (the rung alone could not tell a notice from the one
  after a feud fell and climbed back).
- **MZB-11** (BAND_MARCH's small fields): kind uses BAND_MATERIALIZE's purposes (1 envoy to 6
  plunder); phases are numbered from 1 (1 mustering to 5 home), so 0 is never a phase; flags 1 the
  chief leads, 2 ended, 4 joint; the partner agent at 48 and the joint flag are written as zero; the
  unit kinds' count sits at 64; ticks are game time, as ENVOY_MATERIALIZE's deadline. AMBUSH_SPOT's
  reach is an f32 in blocks.
- **MZB-12** (Conflict cultures in dictionary kind 5): BAND_MARCH's culture is named
  `mic_rtsim:conflict/<culture>` after the culture data's `name` (ids from 1 in the data's order), as
  its bodies are named, so it never meets phase 1's village cultures (`human`, `ribbit`) in the one
  engine dictionary. A fork (reported): the plan does not say how the culture is named.
- **MZB-13** (Ids translated for the later kinds now): rtsim-jni translates CAMP_DEATH's body,
  the census bodies and CHIEF_PAID's items on the way in, and AMBUSH_SPOT's dimension, CAMP_RECONCILE's
  line bodies and BAND_MARCH's dimension, culture and units on the way out, and moves BAND_MARCH's
  ticks by the test offset as the envoy's deadline; `rtsim-host`'s fake Java does the same on its
  side. The later slices then need no wire work. Corrected (review): only rtsim-jni's `write_queue`
  shifts BAND_MARCH's ticks; the fake Java leaves them as they come.
- **MZB-14** (Peaceful holds the war rungs): on Peaceful `plan_feud` books no climb to a war rung
  (raid, invasion, siege), and the feud wake guards the same way. Notices, envoys and scouts are not
  war and go on, so a player on Peaceful still learns of a feud; grievance keeps its curve. Amended
  (MZB-132, review item 13): the envoy is held too, since its stand-in cannot exist on Peaceful;
  notices and scouts still go on.
- **MZB-15** (Peaceful stops what has left): a war party (plunder included) still mustering stands
  down at its muster's end: its bodies, if any, are recalled, its warriors go back to the pool, and
  the feud it served steps down one rung so that it climbs back once Peaceful ends. Amended
  (review): when Peaceful begins every war party already out stops at once, in handle order. One on
  the road (Out) or waiting at its target (Waiting) goes home, a watched fight (Fighting) ends; each
  in Minecraft is recalled with RECALL_SETTLED. `seen()` spawns no war body and `wave()` sends no wave
  while it is Peaceful, so an Out party passing a player stays abstract and an ambush booked before
  goes stale with the party's plan. The plunder drive draws again later. Counted in
  `ConflictStats::peace_held`.
- **MZB-16** (No burst on leaving Peaceful): amended (review). Leaving Peaceful at once booked every
  held feud's climb on the same tick. Now the resident meta keeps the flag and the span's start
  (`peaceful_since`); leaving it, every feud held below a war rung (no party, no demand) has its
  dwell clock paused over the span (`since` moves on by the time it spent Peaceful) and then shifted
  by a jitter in [0, dwell) drawn from the seed, the faction's key and the actor
  (`StreamKey(seed, SYS_PEACE, key).slot(actor)`), and no climb comes before its full dwell. Every
  feud is then planned again at the engine's present (page by page, in the budget).
- **MZB-17** (DIFFICULTY is journaled): amended (review). It was latest-wins at 0x0093, applied
  outside the shared clock, so a crash replay and the conflict-only sim could apply it at another
  point among the other records. It is now the journaled kind 0x0815 (u32 at 24, size 32) and
  0x0093 is retired. Java journals it at each session start and on each change; rtsim-jni and
  `Conflict::ingest` both apply it at its game time in journal order through `set_difficulty`, and a
  value equal to the engine's changes nothing. `ConflictLinkStats::difficulty` is gone.
- **MZB-18** (The kept notice lives in the feud's row): an `Option<Box<Notice>>` in `Feud`, paged
  with the faction's row. A resident map was tried first and failed the paged test that forbids
  resident state growing with factions (2,877 bytes at 64 factions, 8,730 at 256).
- **MZB-19** (A notice is kept until played): until a matching NOTICE_SHOWN or until its feud is
  forgotten, whatever rung the feud is on since; a later notice replaces it. A fork (reported): a
  notice can still play after its feud has climbed past Notice, since the plan says a notice is never
  lost and does not say it expires.
- **MZB-20** (Resent after the territory): at each session start the engine sends, faction by
  faction, the TERRITORY and then the kept notices of that faction's feuds, keeping to the page
  budget, so the plugin knows the territory before a notice that names it.
- **MZB-21** (NOTICE_SHOWN's actor): (0, 0) is the settlers; a player the engine has never seen or an
  unknown camp is ignored; each cleared notice counts in `ConflictStats::notices_shown`.
- **MZB-22** (When a demand is withdrawn): only on the casualty path, when the last member of an
  envoy party falls and no player killed any member or its chief over the party's life (`breaker`);
  a BAND_DEMATERIALIZED with no survivors withdraws nothing. The demand is settled through the
  outbox (DEMAND_SETTLED 5) and counted in `ConflictStats::withdrawn`. Amended (review): the old rule
  (Envoy with its dwell begun again) let a feud past the Scouts threshold climb at the dwell's end,
  sooner than the demand's deadline, so a mob killing the envoy brought the war forward. Now the
  feud holds on Envoy with no climb until the withdrawn demand's deadline (`wait_until`, and
  `Feud::envoy_again`, saved in the row), then sends a fresh envoy with a new demand, which runs as
  any demand.
- **MZB-23** (Deferred kinds are counted and dropped): a kind whose engine half comes later is
  decoded and translated, then dropped before the shared clock runs for it (in rtsim-jni before
  `catch_up`) and counted in `ConflictLinkStats::deferred`. Corrected (review): the earlier text had
  `Conflict::ingest` drop it before the op check and left rtsim-jni's op id spent only by `Replay`,
  so the two paths differed. Both now hand it to `Conflict::ingest`, which spends its op id first and
  then drops it, so a record sent twice is counted once in either path and the engine's op set is
  the same (`Conflict::op_applied`). `Conflict::later` lists the kinds, and each later slice takes
  its own off the list.
- **MZB-24** (A reconcile in flight ends with the session): `session_start` drops any queued
  CAMP_RECONCILE with the territories and notices it rebuilds, as MOWZIE.md §4.9 voids an epoch's
  lines; nothing sends one yet.
- **MZB-25** (The fake plugin): `FakeConflict` plays every notice and answers NOTICE_SHOWN a tick
  later (`play_notices`, on by default), and sends DIFFICULTY (`difficulty`, Normal) at an explicit
  `session_start`. Java's fake does not call it at open, so the rtsim-jni tests choose; the scenario
  calls it with Normal, so `rtsim-cli sim conflict` runs as before.
- **MZB-26** (The page format): the conflict's pages are RTC4 with meta version 3 (the kept notice in
  each feud with its serial and the Envoy hold, the Peaceful flag, its start, the last serial and
  three counters in the meta), with no migration. The rtsim-view
  fixtures are fresh core and create GameTest saves from the merged head, as before.
- **MZB-27** (The viewer hashes under the guard): the fresh core save put the herds' forage pages in a
  segment of their own, and `unreadable.rs` then panicked: rtsim-view's `load_world` took each paged
  module's state hash (which reads every page) outside its `guarded` load. The hash of phase 1,
  dragons, herds, ships and conflict is now taken inside the guard, so a page that cannot be read
  makes that module unreadable instead of the viewer panicking.
- **MZB-28** (Serials are engine-wide): the review asked for a serial per kept notice. It is one
  counter for the engine (`Conflict::notice_serial`, from 1, wrapping past 0), saved in the meta, not
  one per feud: a feud forgotten and made again would restart a per-feud count at 1, and a stale
  NOTICE_SHOWN for its first life could clear its new notice. A fork (reported).
- **MZB-29** (Which stopped parties step back): a war party stopped on the road (Out) never did its
  war, so its feud steps back a rung, as a mustering one does, if the feud is still on that party's
  rung and served by it. One stopped Waiting or Fighting reached its target, so the feud keeps its
  step. A fork (reported): the review did not say.
- **MZB-30** (DIFFICULTY while the module is off): ignored and counted, as every journaled record is
  (U10), not kept for later; the next session start sends it again. A fork (reported): the old
  latest-wins rule kept it, which a journaled kind cannot do without a store of its own.
- **MZB-31** (Kept notices after the replay): `session_start` (TERRITORY first) runs at
  SESSION_START, but the kept notices go out only at the first record of this session or at the
  first simulate point, after the replay has applied the NOTICE_SHOWN the pages missed. Sending them
  first drops any FACTION_NOTICE the replay queued again, so each goes once.
- **MZB-32** (No recall for nobody): a withdrawal that leaves no member spawned clears the party's
  embodied flag and sends no BAND_RECALL; one with members still in Minecraft recalls them with
  RECALL_SETTLED, not RECALL_DONE.
- **MZB-33** (A test hook): `Engine::with_conflict_mut` lends the conflict and the world to a test
  and then takes and sends the commands it queued, so the rtsim-jni tests can order a party
  (`Conflict::order_party`) through the native engine.
- **MZB-34** (The ambush under Peaceful at the engine): rtsim-jni has no rig with a train on a road a
  band lies by, so the ambush case is an engine test with a fake `Infrastructure` (rails under the
  raid's road): an ambush booked before Peaceful never falls. The Out, Waiting and Fighting cases are
  rtsim-jni tests. A fork (reported).
- **MZB-35** (List caps): every list of a variable-length record holds at most 1,024 entries
  (`boundary::MAX_ENTRIES`); encoders write the first 1,024 and decoders refuse more
  (`CodecError::TooMany`). At the cap CAMP_CENSUS, the largest, is 49,240 bytes, so the u16 size
  always fits, and `finish` asserts it does. 1,024 is past any camp, party or route this slice can
  make (PLACEHOLDER).
- **MZB-36** (The conflict's reports on the severity unit; the TALK review's finding): the conflict
  records its Robbery (a camp plunders a village), Rescue (a player routs a band: the real witnesses,
  CL-D61, the village the band marched on, each village that hates the camp within the view
  radius of where it broke, and each player base within that radius, plus the base a band that
  was fighting at its target marched on) and Killing (the war chief's killer, seen by the chief
  and the heir) at `SEVERITY_UNIT`, the kind's base delta, not at the rung codes 2, 2 and 4. The
  knowledge layer reads severity as a share of the unit, so a robbery moved the village's feeling for
  the camp by -1 instead of -50, a rescue by 0 instead of +25 (and wrote no vanilla gossip, which
  K2-25 promises), and the heir's feeling for the chief's killer by -5 instead of -125. The unit, not
  the rungs times 100: every other module records these kinds at the unit (rtsim-world, ships,
  ecology, which grows a robbery's severity with what it took), and rungs times 100 would make one
  robbery count twice a caravan's and one killing reach the clamp. The news kinds (10 and up) keep
  their rung codes (`ReportKind::magnitude`). TALK's `severity_term` still reads any severity under
  25 as a rung (TK33); no module records one now, so that rule only guards. Measured on `sim conflict`,
  seeds 1 to 24, 24 and 96 days, base against fix: escalations, parties, chiefs killed, casualties,
  goods stolen and every check identical; the heir's grudge adds 9.8 more killing grievance at day 24
  (65.1 to 74.9, the ignore run), none by day 96 (inherited raid grudges already hold the heir's
  feeling near the clamp). Villages' feeling for the camp at day 24 goes from 0 to -15.0 on average,
  -52.3 for the most resentful village (0 to 3.7 of 9 villages resent it), -40.6 and -113.6 at day
  96; the player's best feeling at a village from 0 to +88 at day 24 and +116 at day 96 (each rout is
  first-hand news to every hating village in range). The conflict census (16 seeds, 960 days, nobody
  watching) is identical: no player means no rescue or killing report, and nothing in the conflict
  reads a village's feeling for a camp. `sim conflict`'s results gain a `feelings` block (each
  village's feeling for the camp and the players at the end).

## M2 Cultures

- **MZB-50** (empty rungs, both ways): `Culture::has_rung`, `next_up` and `next_down` (ledger 2).
  A climb goes to the next rung up that has a party, or to Notice, at the threshold of the rung it
  goes to and after the dwell of the rung it leaves. Every step down (the calm in `feud_wake`, each
  of a fallen chief's two steps, an unmet demand's fall) goes on down past empty rungs, floored at
  Notice, so from Siege with Invasion and Raid empty a chief's fall lands on Envoy and a calm on
  Scouts. M1's Peaceful rules (merged in) take the same helpers: `step_back` (a war party stopped
  by Peaceful) steps its feud down by `next_down`, and the climb `set_difficulty` holds back is the
  one `next_up` names. An unmet demand climbs to `next_up(Envoy)` (Scouts for every culture so far).
  `send_party` sends nothing for an empty make-up (and see MZB-64). `Culture::resolve` lets any
  party be empty, refuses a unit counted 0 in one, and wants a plunder party only from a culture
  with `plunder_mean_days`. Reason: the lead's rule, and
  one helper pair keeps every site the same.
- **MZB-51** (a repeat counts from the arrival home): `repeat_days` (ledger 3), finite, above 0 and
  at most 3,650 (`resolve`). A feud on Raid or Invasion whose rung has a party, with no party out
  and no demand, books one event at the latest of the rung's threshold crossing, `repeat_days`
  after its band is home, its cowed time and its pool wait; the earliest of climb, repeat and fall
  is booked, and at the wake the climb is tried first, so a tie climbs. No new Feud field: a band
  walking home keeps `feud = Some(actor)`, so `repeat_after` takes the later of the feud's `since`
  and the last of its times (the arrival) over this faction's `Phase::Home` war parties for the
  feud. A Frostmaw sent to a station far outside its 200 blocks is home long after it turns, and
  the pool refills on the way, so counting from the turn (this choice's first version) could field
  a second Frostmaw while the first walked back; `sim mowzie` run 1 and the cultures tests check
  that never two are about. When a band comes home on its feud's own rung (no party out, no
  demand), `home` also sets the feud's `since` to the arrival and books the feud again, for a
  culture with `repeat_days` only, so the next wake is from the arrival. Side effect, the
  conservative one: the dwell of that rung counts from the arrival too, so a Frostmaw climbs to
  Invasion no sooner than Raid's 4-day dwell after its last band is home. Cultures without
  `repeat_days` (goblins, bandits, raiders, the Umvuthana) are untouched. The repeat sends the same
  rung's party through `escalate(.., again)`: the raid grudge and `raids` as a climb, but counted in
  `stats.repeats` (saved last in the stats), noted "repeat", and with no CONFLICT_STEP (its
  CONFLICT_PARTY records it). Tick sums here saturate (`later`), so no day count can overflow one.
  Without `repeat_days` nothing is booked and the wake is today's. With M1 merged: on Peaceful
  `repeat_after` books nothing (no war party leaves, M1's MZB-15), and leaving Peaceful counts a
  feud that would repeat as held, as one whose next rung up is a war rung, so it gets M1's pause
  and jitter and its repeat comes within its dwell plus `repeat_days`; a test pins none on Peaceful
  and one after it, with never two Frostmaws about.
- **MZB-52** (a pool wait for whole warriors): `escalate`'s wait is `ceil(want / 2)`, the whole
  warriors `send_party` needs. Without it a party of odd size (the Frostmaw's 1, the raiders'
  Invasion of 23) waited for half a warrior, failed to muster and woke its feud again at once. A
  test pins it: a Frostmaw's pool set to 0.6 at its climb to Raid wakes its feud fewer than 10
  times in the next 3 days (about 72,000 without the `ceil`). The wait's tick sum saturates. M1's
  `envoy_again` (merged in) waits for the same `ceil` of its Envoy's size. M2
  takes this one line; M4 builds the rest of ledger 6.
- **MZB-53** (threat source and class from the culture): `source` (`bandit` by default, `boss`)
  is what ambushes and district raids tell the industry engine (ledger 9). Both send one threat per
  class in the party, each with that class's count, strength and carry (ledger 9 says each unit's
  class and carry); an ambush's threat id hashes the class in. What changes for today's cultures:
  their ambushes sent one ground-melee threat with the whole party's strength and carry. A goblin
  raid (8 warriors, 4 archers) now sends melee 8.0 carrying 64 and ranged 3.2 carrying 16, an
  invasion adds its brutes as siege; a bandit raid sends thugs as melee and crossbowmen as ranged;
  a raider raid sends vindicators as melee and pillagers as ranged, an invasion adds its ravager as
  siege. Each threat meets the train under the industry engine's cooldown for its own class, and
  takes loot by its own strength and carry, so a mixed party can take more from one train than
  before, and a damaged party keeps the classes it has left rather than flipping wholly to its
  strongest. The ambush counts once in `stats.ambushes` and notes once, whatever it sent. The
  Umvuthana (all melee) and the Frostmaw (one unit) send one threat, as before. This supersedes
  C2-1's "a train ambush carries the band's whole carry" (docs/overnight-choices-conflictjni.md).
- **MZB-54** (the crystal row): `UnitDef.without_crystal` names the row a unit fights as while its
  chief lacks its crystal, and `Faction.crystal` (true until a report says otherwise, saved) picks
  it for strength, class, carry and losses. A body is always the unit's own row's, so the plugin
  sees one Frostmaw body either way. Nothing clears the bit yet: CHIEF_STATE is M6's.
- **MZB-55** (eaten loot leaves the books): `loot_destroyed` (ledger 10). A village's stock is taken
  by `World::take_stocks`, booked as destroyed in the market's line as a dragon's prey is, and
  counted in `ConflictLedger::destroyed`; goods taken from players (district raids, ambushes,
  watched looting) are counted in `destroyed_from_players` and never carried. `check` balances
  taken against landed, held or carried, and destroyed. Both maps are saved with the ledger.
  `take_stocks` takes a raid's whole want in one pass: one release, one market pass for every good,
  one market event (news) if anything was taken; `take_stock` is the one-good call of it. One pass
  per good made a news event per good for one raid.
- **MZB-56** (a lair must destroy its loot): `resolve` refuses `haven: false` without
  `loot_destroyed`. Reason: a lair has no storehouse, so carried loot could never land.
- **MZB-57** (tribute by value): the demand is
  `ceil(tribute_per_grievance × grievance × bend / 1000 × per_coin)` (ledger 12). `per_coin` is 1
  for money (a currency good, as gifts count it), else one coin's worth at the haven market's price
  (`PRICE_ONE / price`) held to at most `TRIBUTE_PRICE_CAP` (4) times the fallback, else
  `tribute_per_coin_fallback` (1 when absent; the Umvuthana's 9) while that market has no price. The
  factor 4: a nugget priced near nothing (a thousandth of a coin) would ask 1,000 a coin; 4 times
  the fallback (36 for the Umvuthana) is the most a grove asks, and a market price up to that is
  followed exactly. The tribute is clamped to `i32::MAX` before the wire's `i32`, as is the unpaid
  rest an envoy carries. `resolve` wants the fallback finite, above 0 and at most 1,000. The
  emerald cultures' demands are unchanged. Tests: tribute at 4, 9, 20 and (capped) 1,000 nuggets a
  coin, a priced tribute paid in full, an envoy refused.
- **MZB-58** (the `gold_nugget` good): added to the synthetic catalogue last (GoodId 9, so no other
  id moves), with no recipe and no need, and mapped to `minecraft:gold_nugget` in the phase 1
  catalogue. No market prices it yet, so the fallback carries every Umvuthana demand.
- **MZB-59** (the lair): `NodeKind::Lair`, a unit variant saved as tag 3, with no culture and no
  storehouse. `World::join` gives it a base market as it does a player base, so the ~6.6 KB a
  market-less node would save was not taken: every market read would have to handle a node with
  none. A raid on a lair meets haven defence (`haven_defence_per_pop`), plunder never picks one,
  ribbit bands never visit, its holder kind is a haven's, and the layers draw it as category 4. A
  lair joining makes no SETTLE_JOINED, as a player base makes none: only the tape (the viewer's
  news) and the archive tests read that kind, and a den is not a settlement founded.
- **MZB-60** (`frostmaw.json`): goblin's thresholds, dwell, hysteresis, half-life, demand and relief
  values, march, muster, patience, linger, siege interval and cowed time; `human` market only to
  resolve, with `haven: false`; `chief_lifespan_days` 1,000,000; territory 200 and reach 0. Killing
  8.0 and building 0.05 from goblin, logging and mining 0, no smog or radiation (Rail is M7).
  Units `frostmaw` (ground ranged) and `frostmaw_bare` (ground melee), both strength 41.7, carry 64,
  breaks 0, body `mic_rtsim:conflict/frostmaw/frostmaw`. Raid and Invasion are one `frostmaw`;
  Envoy, Scouts, Siege and plunder are empty; it hates nobody and never plunders. `repeat_days` 4,
  source `boss`, `loot_destroyed`. Fork: `pool_cap` 1 with `recruits_per_day` 1/6, a PLACEHOLDER
  for the den's 6-day first wait until M4 and M6 book arrivals; until then the pool regrows by
  itself.
- **MZB-61** (`umvuthana.json`): goblin's values as above, territory 200 and reach 0, lifespan
  1,000,000; goblin's smog, logging, mining, killing and building weights, no radiation. Units
  `warrior`, `raptor` (a pack, 2.6) and `envoy`, all ground melee with carry and breaks 0, bodies
  `mic_rtsim:conflict/umvuthana/<unit>`. Parties as MOWZIE.md §4.5, siege empty. `pool_cap` 9, the
  top of a grove's 5 to 9 until ledger 7 counts each grove; recruits 2 a day. Hates `human`;
  plunders every 12 days within 1,500 blocks with one raptor pack, whose take is eaten
  (`loot_destroyed`, MZ-8) and is nothing until raptors carry. Tribute in `gold_nugget` at a
  fallback of 9 an emerald; no repeat; source `boss`, as ledger 9 says and as the Frostmaw. Boss
  changes nothing either side can see but text: the industry engine special-cases only `Natural`,
  and the Java side has only the THREAT_BOSS constant, so it shows in log lines and threat records.
- **MZB-62** (the `sim mowzie` scaffold): `rtsim-cli sim mowzie [list | run N ...]`, runs numbered
  as MOWZIE.md §7. M2 builds run 1's den ladder, with kills in the range standing in for the rail
  meter: held between Raid and Invasion until Raid's dwell is over, then at 1.5 times Invasion's
  threshold for 40 days, then nothing, it checks Notice, Raid, Invasion, Raid, Notice, None, at
  least 2 Invasion repeats (the top rung), every repeat 4 days after the last band is home, never
  two Frostmaws about, and a bounded count of feud wakes. Run 5's grove ladder checks its demand at
  the fallback price, and two more groves whose havens price gold: a demand at 4 nuggets a coin
  paid in full (part of it first settles nothing), one at 9 refused at the envoy, which climbs to
  Scouts. Every other run, and the rest of these two, prints the slice that builds it. JSON goes
  to `scenario/results/sim-mowzie.json`, kept in git as the other sims' results are.
- **MZB-63** (what M2 changes in `sim conflict`): measured on the M2 base (09374322) against the
  head, a goblin camp and a raider camp in the `sim conflict` scenario (scenario.rs takes the
  culture from an env var in a throwaway copy only), seeds 7, 8 and 9, all three variants, 24 days:
  18 runs. In every run the conflict's tape rows, its log, the rungs, the attacks, the blocks
  broken, the world's stats and every node's stock and price of every old good are identical. What
  differs, all intended: `ConflictStats` has `repeats` (0 here; MZB-51); the conflict's state hash
  (its save holds the new stats field, the ledger's `destroyed` maps (MZB-55) and the culture
  fingerprint, which covers the new culture fields and the two new cultures); the world's state
  hash (the catalogue's `gold_nugget`, MZB-58, is in every market's encoding). The pool wait's
  `ceil` (MZB-52) and the per-class ambush (MZB-53) do not show here: neither culture reaches
  Invasion in 24 days (Raid 3 times at most), and no war band crosses a train in the refinery
  scenario, so their tests carry them.
- **MZB-64** (no wake loop on an empty rung; a chief's fall replans): `escalate` to a rung whose
  party is empty is a `debug_assert` and, in release, returns before it books anything, and
  `repeat_after` books no repeat of a rung without a party, so neither can book a wake every tick.
  Only a culture edited in place gets there: `Conflict::load` refuses a save made with other
  culture data. `resolve` refuses a party unit counted 0, so an empty make-up is an empty list.
  `chief_killed` changes the step and then plans every feud of the faction again, including one
  whose band is walking home (neither `go_home` nor `end_fight` runs then, reachable from M6); a
  test pins the new stamp and the band's arrival on time.
- **MZB-65** (a lair in TALK is nobody yet): main's TALK describes a settlement node by its kind,
  and `NodeKind::Lair` (MZB-59) is new here. Its node describes as UNKNOWN: TALK's LAIR class is a
  dragon lair by the ecology's key and resolves back through the dragons engine, so naming a
  Mowzie lair with it would send the plugin a key it cannot find. The lair's Java side, and its
  class if it gets one, wait for M5 and M6. A test in `talk.rs` pins it.
- **MZB-66** (raids under CL-D58 with M2's cultures): merging main's CL-D58 (a raid takes only the
  culture's share of each line's surplus over its target, and a band that could not carry off one
  whole item does not fight) into M2. A culture whose loot is destroyed (MZB-55) destroys the same
  capped shares that another culture would carry off, so destroying never reaches below a line's
  target. `carry_milli`, which CL-D58 added for the cap and the take, reads each unit as its
  chief's crystal has it fight now (`unit_row`, MOWZIE.md ledger 9), as M2's raid did before the
  merge. A lair has no stock over a target, so a band never fights at one for loot (MZB-59 still
  sets its defence).
- **MZB-67** (the blockade test's parent world hashes, re-pinned): `blockade_off_changes_nothing`
  (`crates/rtsim-ships/tests/blockade.rs`) pins the ships' and the world's save hashes of three runs
  with closures off to the parent's. M2's catalogue adds `gold_nugget` (MZB-58) to every market's
  encoding, so the three world hashes move. On M2 merged with main 4b207a37 the ships' hashes are
  the parent's, and with `gold_nugget` left out of the catalogue the world's are too (the test
  passes unchanged), so the new good is the whole difference. The world hashes are re-pinned to
  the merged head's, and the test's doc says why.

## M3 ConflictBridge

- **MZB-100** (the module switched off mid-session): with `conflict` off the bridge journals its waiting
  territory signals, captures every party as at a session's end (BAND_DEMATERIALIZED, SESSION_END) and
  drops its territories, kept notices and signal batches; `reportCamp` still remembers the camp (its kin
  test) but sends no FACTION_FOUND. D1 says the bridge is inert with no faction; this keeps it inert with
  the switch off too, and a later switch on starts clean from the engine's next TERRITORY.
- **MZB-101** (no words, news a no-op): Ben's decision of 2026-10-22: the bridge sends no chat, action bar
  or title. A notice is a sound; news reaches players only through the gossip system. `ConflictNews.post`
  is the one hook, and does nothing until branch cl/news-in-world gives it a sink. CL-8 (no player pacts)
  holds; NOTICE_SHOWN stays. The envoy is named "<Culture> Envoy" while its demand is open (TALK's
  speaker keys, `mic_rtsim.talk.speaker.envoy` with `mic_rtsim.talk.culture.<culture>`, "Envoy" for a
  plugin body with no culture) and its name never holds the demand: a nametag shows through walls to
  everyone near, and CL-1 tells a demand to that player only, so the demand is said only in the line a
  click or a talk gets (MZB-110, MZB-122). The first build's name ("3 Emerald, or stop killing") broke
  that. This is the conservative reading of Ben's in-person rule (review of 2026-10-23); the name goes at
  DEMAND_SETTLED.
- **MZB-102** (stand-ins): with no plugin's bodies, a culture's units stand in as vanilla illagers by unit
  name (warrior and thug to the vindicator; archer, crossbowman, emissary, herald and messenger to the
  pillager; brute to the ravager), through `BodyKinds.standIn`, which a plugin may override per body or
  per unit name. An unknown name falls back to `minecraft:pillager`. Bodies are made as vanilla makes a
  mob for an event (`finalizeSpawn` EVENT, called directly) and kept out of vanilla raids and patrols.
  A body is persistence-required while it is the party's; a body that leaves as an ordinary mob (a
  player's take, Carry On) gets back the persistence it had before the bridge held it (a body the bridge
  made: none), so it despawns as any other mob would (UR-1). `Mob` has no setter that clears the flag, so
  a mixin accessor (`MobAccessor`) writes it: the core has no access transformer and reaches every other
  private member through accessors too (the lead allowed an AT).
- **MZB-103** (forced chunks stand in for the simulation distance): the GameTests' fake players make no
  chunk tickets, so tests that need a place to tick entities, or to stop, force and release chunks at a
  far base (x below -3,000,000, z above 3,000,000) no earlier run used.
- **MZB-104** (flat distances): territories, notices and a member's distance from its place are flat
  x, z distances; a territory across a world seam counts on its own side only. The engine owns seams
  (TP2); a seam-crossing territory is rare and its far side only misses signals.
- **MZB-105** (a machine's signals are the settlers'): a block broken or placed, or a kin killed, by a
  NeoForge `FakePlayer` (a machine) counts for the settlers (no player in the TERRITORY_SIGNAL), never for
  the player who owns the machine. The engine already keeps a settlers' actor for unowned sources.
- **MZB-106** (stack formation): a stack walks two abreast, 1.5 blocks a row and 1.2 a file, behind its
  anchor; each member has a follow goal that takes it back to its place when it strays past the leash,
  and leaves it alone while it fights. Neighbouring stacks placed at one point stand 4 blocks apart
  across the heading.
- **MZB-107** (the envoy's guard): an envoy party's members have their target goals taken away and every
  target change by or onto them from outside the party is cancelled; every hit on them is blocked but
  those that bypass invulnerability (`/kill`). A hit by a real player (not a FakePlayer, not a
  spectator), directly or by a projectile they own, lifts the guard for the whole party, gives back its
  target goals and is answered once (ENVOY_ANSWER 3). The guard lasts until then, not for a time.
- **MZB-108** (who did it, when nothing did): a member killed with no killer (`/kill`, the void) is
  BAND_CASUALTIES kind 0 (none); a member discarded by anything but the bridge (a command, another mod)
  is kind 3 (other) with no killer. A Peaceful removal of a hostile body is neither: the stack goes back
  to pending.
- **MZB-109** (the war chief is an extra body): a band with the chief flag embodies the chief as one more
  body of its first stack's kind, at the rear of that stack, named; CHIEF_KILLED is sent for it (with a
  Java op id) and no BAND_CASUALTIES.
  Amended at review C (M6): the meaning stands, and it is the engine that adapts for a culture whose chief is
  its one unit (`renewal.boss`, the Frostmaw): `spawn_wave` sends that chief stack with `count` one less than
  its units, so a one-Frostmaw raid is a stack of count 0 with MEMBER_CHIEF, which is MZB-127's chief alone
  and one body (see MZB-295). Before this a raid sent count 1 with the chief flag, and Java made two
  Frostmaws: the unit slot took the den's body and the extra chief slot a fresh one with its crystal.
- **MZB-110** (clicking an envoy; no click refuses): the main session's ruling of 2026-10-23, over the first
  build's plain-click refusal. The demand's player pays by clicking the envoy with the demanded good in
  either hand, crouching or not: what is held, up to what is owed (TRIBUTE_PAID with a Java op id each
  time). The main hand pays when it holds the good; the off hand pays when the main hand is empty, and
  through vanilla's off-hand click when the main hand holds something else; the off-hand click after an
  answered main-hand click does nothing. After a payment the envoy says what is still owed (its demand
  line, to that player only, TALK's limits applying), so a payer hears the count without a name to read.
  A plain click with empty hands shows the demand: the same line as talk, to that player only (crouching
  is TALK's own gesture, MZB-122). Anything else in the main hand is an ordinary click. A settlers' demand
  (no player) takes nothing from any player: a click only shows it. A demand is refused only by striking
  or killing the envoy or a guard (ENVOY_ANSWER 3, MZB-107), or ends at its deadline (DEMAND_SETTLED 3):
  the bridge never sends ENVOY_ANSWER 2, which the codec keeps. Once its deadline has passed, or a player
  struck the party, or the tribute is all paid, or DEMAND_SETTLED came, a click with the good takes
  nothing and the envoy says why (`envoy.settled.expired`, `.struck`, `.paid`, or the settlement's
  outcome; `struck` is a new line). The good is kept after settlement so that click is still answered.
- **MZB-111** (a machine is no killer): a member killed by a NeoForge FakePlayer is kind 3 (other) with no
  killer, as MZB-105 makes a machine's signals the settlers'.
- **MZB-112** (M1's records wait for M1): DIFFICULTY and NOTICE_SHOWN are written against M1's layouts but
  sent only while `ConflictBridge.m1Records` is true, false until M1's engine half is on main (the engine
  on main does not know their kinds). The GameTests check NOTICE_SHOWN only then. Amended after M1
  landed (36b019d6): `m1Records` is true; DIFFICULTY is the journaled 0x0815 (a u32 at 24, size 32,
  at session start and on each change); FACTION_NOTICE's serial is read at 60 and NOTICE_SHOWN carries
  it in its aux. Test 19 checks the serial and a Peaceful GameTest checks DIFFICULTY.
- **MZB-113** (a withdrawn demand): DEMAND_SETTLED outcome 5 (withdrawn) is handled as every other
  outcome: the envoy's name goes and the news hook hears it. What withdrawal means is the engine's.
- **MZB-114** (test 18's foliaath): Mowzie's Mobs is not on the core's GameTest server, so a datapack
  registered only there adds `minecraft:endermite` to `mic_rtsim:threat/ignored` as the foliaath's
  stand-in. The shipped tag names `mowziesmobs:foliaath`, not required, and test 18 reads the shipped
  file from the core's jar to check it.
- **MZB-115** (scripted tests use an unclaimed camp): GameTests that script the engine's side report their
  camps as `mic_rtsim:conflict/test_unclaimed_camp`, which no culture claims, so the native engine founds
  no faction of its own there and sends no TERRITORY or feud of its own into the test.
- **MZB-116** (a guaranteed drop in the raid test): the raid test sets one member's main-hand drop chance
  above 1 so its crossbow always drops, to check that a dying member's drops carry its pending record.
- **MZB-117** (the feud test on the engine's clock): the native "feud driven to a raid" GameTest moves the
  phase 1 clock (P1_ADVANCE, which the conflict runs on) 1,200 ticks a step, under the conflict's
  `player_stale_ticks` (2,400), with the fake player reporting where it stands before each step; the camp
  is 40 blocks from the player so every party comes into view at once, and any stack still pending has
  its chunks forced (MZB-103). It stops at the raid's first materialization (seen setting out); the
  director's arc 112 blocks out is the JNI protocol test's. The player kills 32 of the camp's kin
  (grievance 256): the chief's temper is random and bends each threshold up to 1/0.6 of the culture's,
  so 16 kills (128) decayed below a meek chief's raid threshold (117) on the three-day climb.
- **MZB-118** (where a notice plays): a kept notice plays its sound to its player once they stand inside
  the faction's territory or within 256 blocks of the notice's place, tested every 20 ticks. A settlers'
  notice (no player) plays to every player in range then (not a spectator, not a FakePlayer), not only
  the first, and is forgotten after that tick; a player who comes in range later does not hear it.
- **MZB-119** (pinning bodies in the far stack test): the stack test turns off its bodies' AI so no stroll
  crosses a chunk edge, and lays a stone floor under them, since a mob without AI does not fall.
- **MZB-120** (the conflict talker, registered as bodies appear): one core talker (`ConflictTalker`) is
  registered as a built-in (asked after any plugin's talker for the type), not always talking, for each
  entity type the first time the bridge resolves a body kind to it. Stand-ins and plugin bodies are only
  known then, and the client never cancels a click on those types: the server decides per entity. It
  talks only for a live member of a party the bridge holds; beasts (an `Animal`, a ravager) never talk
  (TALK.md §4). Holder: the faction (3, the party's faction id). No small talk, no credit (TK6).
- **MZB-121** (the envoy's one line): voices follow TALK.md §4 (`envoy.<culture>`, parent `envoy`; a band
  member `<culture>`, culture read from `mic_rtsim:conflict/<culture>/<unit>`), not the lead's suggested
  `conflict.<culture>.envoy`, whose parent under TALK's drop-the-last-part rule would be
  `conflict.<culture>`, not `conflict.envoy`. Topics follow TALK.md §5.5, split by form: while open,
  `envoy.demand.tribute_or_stop`, `.tribute` or `.stop`, with the reason as its first argument (the most
  specific level's `envoy.reason.<signal>` line 0, else `envoy.reason`), then what is owed, the good, the
  signal word and the deadline; once settled, `envoy.settled.<outcome>` (paid, cut, expired, refused,
  withdrawn, other). Each has one line at the `envoy` level only, so it is the same line every time (TK31
  would mix a culture level's lines with the parent's). It speaks only to the player the demand is to
  (anyone for the settlers'); the speaker is "<Culture> Envoy".
- **MZB-122** (the empty hand talks and shows): TALK's crouch-click and the first build's refusal were one
  gesture, and the bridge's HIGHEST handler is registered before TALK's, so it would have swallowed the
  talk. With no click refusing (MZB-110), both empty-hand clicks say the envoy's line: crouching is TALK's
  (the bridge leaves it), and a plain click is the bridge's, which speaks through `Talk.speak` with the
  conflict talker, so TALK's rate limits and line picking hold for both. A crouch-click holding the good
  pays. Known gap: TALK's private off-hand guard is not told when the bridge speaks, so after a plain
  empty-main-hand click the client's off-hand item use (food, a shield) still reaches the server; the
  bridge cancels only the off-hand entity click. TalkEvents is TALK slice 2b's file and was left alone.
- **MZB-123** (the deadline in words): by the last of dawn, noon, dusk or midnight (day time 0, 6,000,
  12,000, 18,000) at or before the deadline, never after it: "by dusk tomorrow", "by noon, 3 days from
  now", "at once" once that mark is past. Deadlines are game time, the marks the level's day time.
- **MZB-124** (band members warn or insult): a member of a raid (purpose 3 and up, the hostile flag, or
  attacking) or of a routed band insults (`hostile.insult`); any other (scouts, an envoy's guards) warns,
  with the reason when the faction has one for that player (`hostile.warn.<signal>`: an open demand of its
  envoy to them, else a notice kept for them), else `hostile.warn`. Hostile to that player in a raid or
  with a reason, else wary. Lines for goblin, raider and bandit; the reasoned warnings and a fallback for a
  plugin's culture under `any`.
- **MZB-125** (a placed stack keeps to the area): the first build took a stack back to pending whenever a
  member stood where entities do not tick, and made it again at once, so a member that wandered over a
  chunk edge flickered the whole stack out and back healed and re-geared. Now a member out of the area
  whose place still ticks is moved back to its place (teleported, the same body; counted as a stray); a
  member whose body went (another dimension, unloaded) is made again at its place once
  `REQUEUE_COOLDOWN` (200 ticks, PLACEHOLDER) has passed; a member fighting away from its place in a
  ticking chunk is left alone (MZB-106's follow goal brings it back). The stack goes back to pending only
  when such a member's place stops ticking too, or Peaceful took a hostile body, and a requeued stack is
  placed again no sooner than `REQUEUE_COOLDOWN` ticks on. A pending stack's chunk set is recomputed only
  when a member's place crosses a chunk (D1: no per-tick sets).
- **MZB-126** (GameTests clean up however they end): a test that forces chunks in the far area
  (MZB-103) first releases every chunk forced there (a failed run skipped its own release, and forced
  chunks persist in the reused world), and releases its own through a GameTest listener on pass or fail
  (`ConflictKit.onEnd`); test 18 resets FIGHT_GAP at its start and in such a listener; tests remove their
  fake players the same way. The feud test counts P1_ADVANCE orders, which queue when the engine lags: it
  allows 80 for the envoy, as it allows 160 for the raid. Amended: the high counts (25, 40, then 80 not
  enough) were mostly a reused world's clock, not load. The phase 1 world restores its tick from the
  pages, which ran ahead of game time by the last run's asks, while the P1_ADVANCE offset lives only for a
  session; no ask moved the clock until game time plus the new offset passed it. The test now asks the
  gap once, before the camp exists, and waits for the clock to move. A fresh world needs 10 asks.
- **MZB-127** (a war chief with no units left): a chief-flagged stack whose count is 0 is a stack of the
  chief alone, one body of the stack's kind; the first build skipped it, so the chief got no body. Stacks
  keep the record's order otherwise (an empty stack without the chief is still skipped).
  Amended at review C (M6): this is how a Frostmaw raid is embodied (MZB-109, MZB-295): the engine sends it as
  count 0 with the chief flag.
- **MZB-128** (test 14's withdrawn demand, scripted): test 14's parties are scripted commands the
  engine does not know, so its DEMAND_SETTLED 5 cannot come from the engine; the test sends it as M1's
  engine does (the engine's own rule is M1's Rust tests). The engine sends it when its count of the
  party reaches nothing, and MZB-32 lets a member still stand in Minecraft then; the test stands one up
  (the settlers' party is an envoy and a guard, the guard killed by `/kill` first), so the name going
  and the envoy's `settled.withdrawn` line can be seen. Then the envoy is killed by `/kill` too: no
  ENVOY_ANSWER from the whole party, and its recall is RECALL_SETTLED (MZB-32).
- **MZB-129** (test 19 across a logout): the logout is real (`PlayerList.remove`: the logged-out event
  fires and the entity leaves the level) and the login is a new entity with the same id; no logged-in
  event fires (the fake players never had one, and their connection is a stand-in). A restart is not in
  the GameTest: the server has one bridge session, and ending it ends the engine for every later test.
  The restart half (the engine resends a kept notice at session start) is M1's rtsim-jni test, and the
  Java side of a resent notice is the FACTION_NOTICE this test sends. NOTICE_SHOWN's aux is checked
  against the notice's serial (FACTION_NOTICE offset 60).
- **MZB-130** (a member that is not hostile chats; TALK slice 4): `ConflictTalker.chats` is true for a
  live member that is not the envoy and not hostile to this player, and hostile is what makes its
  attitude HOSTILE: no raid or rout (an envoy party is neither), the player is not its target, and the
  camp has no reason against him (`reasonFor < 0`: no open demand of the faction's to him or to anyone,
  no kept notice). The core then adds its fluff, mood and news; a hostile member and the envoy keep only
  this talker's lines (warn with the reason, insult, the envoy's one line). Fork: a chatting member keeps
  the plain `hostile.warn` among its candidates at weight 1, so a stranger still hears "keep your
  distance" now and then, rather than dropping the camp's own line; the voice stays `<culture>`. Test 14
  checks it with a bystander after the settlers' demand (to anyone) is withdrawn: the guard chats with
  him, is WARY, and says no reason line and no insult; it does not chat with the player it has a reason
  against, and the envoy chats with no one.
- **MZB-131** (a band seen again while an old epoch walks off; review item 12): a band recalled with
  RECALL_PASSING and seen again at the next epoch (a player ahead of it on its road) has its old
  epoch's BAND_DEMATERIALIZED still to come. The engine matched a despawn to the party's current epoch
  only, so the old one was dropped and `recalled` never cleared: the members who died on the walk came
  back, and the next recall, adding its bodies to the stale count, took the whole band's alive count
  with it. Now `Party::recalled` is `(epoch, stacks)` per epoch recalled (`recall` merges the bodies it
  takes out of view into its epoch's entry); a despawn names its party by agent and is read three ways:
  the current epoch in view is the plugin despawning it (as before), an epoch with an entry has the
  entry removed and its dead (`recalled - survivors`) taken off the alive counts, with the bodies in
  view now clamped to who is left, and any other is stale and ignored. Fork: the live branch no longer
  clears `recalled` (older epochs' walk-offs are still to come); a session start used to clear it,
  which review found wrong (MZB-138: it keeps them for the journal's replay). The conflict
  pages are RTC5 with meta version 4 (the party row's `recalled` is a count of epochs, each a var epoch
  and its stacks), with no migration; M2's unbumped changes (crystal, destroyed loot, repeats) ride on
  that bump. `FakeConflict::walk_off_deaths` takes that many off a recalled band's survivors, first
  stacks first. The rtsim-jni test fails on the old engine (alive 12, not 10, once the old despawn
  is past) and its second recall (the player gone stale) leaves the count alone.
- **MZB-132** (envoys on Peaceful; review item 13, amends MZB-14): no envoy leaves on Peaceful
  either, because the envoy's body is a Mowzie pillager stand-in that cannot exist on Peaceful. One
  predicate (`peace_holds`: a war rung or the Envoy) now holds the climb to a rung in `plan_feud`
  and the feud wake, the dwell pause and jitter of MZB-16 on leaving, and the repeat of a war rung
  as before; a culture that skips the Envoy goes from notice to scouts as ever. Scouts are left
  untouched (fork: they are not war and carry no stand-in). Notices go on, so the feud holds at its
  notice with its grievance kept. Entering Peaceful: an envoy in view walks off with RECALL_PASSING
  toward its next route point (never held in view for a deadline the engine has put off) and stays
  abstract wherever it is: on its road it still arrives and waits, `seen()` shows no envoy, and the
  end of its muster or its wait is held (counted in `peace_held`, nothing booked). An open
  demand's deadline is not booked while Peaceful, and `feud_wake` ignores a deadline passed in it, so
  nothing expires, settles or climbs (a refusal in flight is recorded but its climb waits too,
  MZB-134). Leaving Peaceful (fork: the demand's clock is paused over the
  whole span, as MZB-16 pauses a dwell): every open demand's deadline d0 moves to d1 = d0 + the span
  it lived through (`at - max(made, peaceful_since)`); a withdrawn demand's fresh envoy (MZB-22,
  `wait_until`) moves by the span too, so none goes in a burst at the end; each held envoy is booked
  again after the feuds are planned (a muster at its old end or now, a waiting one to d1 with its
  patience moved by the part of the span it spent waiting, MZB-136), and an envoy a watching player
  has in view comes back at once,
  carrying d1 in BAND_ENVOY. Not a page format change (`peaceful_since` and the deadlines were
  already saved). Tests: rtsim-conflict `mowzie_records` (the two M1 Peaceful tests now hold at the
  notice, the burst test runs at the notice and, from the scouts, at the raid; an envoy held on its
  road, waiting, mustering and as a withdrawn demand) and the rtsim-jni test of an envoy in view
  that walks off, whose deadline moves by the span and comes due not before it; M1's Peaceful jni
  test now sees no envoy until Normal.
- **MZB-133** (an insult answer points at the envoy, not the camp; review item 1): ENVOY_ANSWER
  ATTACKED added its insult (`envoy_insult_kills` killings) at the faction's own camp with no
  district. `signal` replaces a districtless focus with the signal that outweighs it, so a feud whose
  focus had no district was retargeted at the camp itself, and the scouts the refusal then sent
  went there. The insult is now a killing where the envoy stood (`party_pos`), against its target's
  district, as `casualties` makes one for a killed member. Tests: rtsim-conflict `mowzie_records`
  (an attack answer keeps the scouts aimed at the demand's district; a districtless focus does not
  move to the camp; the attack adds exactly the insult to a plain refusal). The camp variant fails
  on the old rule.
- **MZB-134** (a refusal on Peaceful; review item 3): `recall_envoys` does not bump the epoch, so an
  ENVOY_ANSWER sent before Java saw Peaceful (or a dialogue still open) is accepted on Peaceful and
  `refuse` then `demand_unmet` climbed to the rung above, a war rung when the culture skips the
  scouts. `demand_unmet` now filters that rung by `!(peaceful && peace_holds(rung))`. Fork
  (conservative): the refusal is recorded (DEMAND_SETTLED outcome 3, the grievance, the insult) and
  the feud stays on Envoy, its `since` reset; the climb waits for Normal, when `plan_feud` takes it
  after its dwell. The scouts are not held (MZB-132), so a refusal on Peaceful still climbs to them.
  Tests: a no-scouts culture's refusal on Peaceful holds at Envoy through four days of killing, no
  band mustered, and climbs to the raid after Normal; the scouts climb still happens. Fails on the
  old rule.
- **MZB-135** (a Peaceful of no time; review item 4): turning Peaceful on and off on the same tick
  still jittered every held feud's dwell. Leaving Peaceful now pauses and jitters only when the span
  is more than 0 ticks (the walk-off of an envoy in view on entering stays, and costs nothing).
  Tests: a held feud's `since` is unchanged by a same-tick toggle and does move over a 1,000-tick
  span (not vacuous); the envoy in view still walks off with RECALL_PASSING and its deadline stays.
  Fails on the old rule.
- **MZB-136** (patience over Peaceful; review item 5, refines MZB-132): `resume_envoys` added the
  whole span to a waiting envoy's `bored_at`, also for one that arrived during Peaceful, whose
  patience clock started at its arrival. It now adds the time since the later of the span's start
  and its arrival (`times.last()`); an envoy waiting when Peaceful began waited all of it. Tests: one
  held while waiting moves by the whole span, one that arrived on Peaceful by the part after its
  arrival. The second fails on the old rule.
- **MZB-137** (a payment to an envoy settles only its own demand; review item 6): `envoy_home` and
  `go_home` clear the envoy's `feud`, so a TRIBUTE_PAID naming a finished envoy (late, after the
  deadline or the settlement) fell through the chain "its feud, the payer's, the settlers'" to a
  demand open for the settlers, and paid it. Confirmed real by a test that fails on the old chain.
  Now a payment naming an envoy settles the demand that envoy serves (`demand.envoy == Some(p)`) and
  no other, else it is a gift (softening the payer's grievance, as a late tribute always did); only
  a payment at the camp (no envoy) takes the payer's demand then the settlers'.
- **MZB-138** (a restart replays the walk-off and the fight; review item 2, older than item 12): a
  session start cleared `recalled`, `spawned` and `embodied` before the journal replayed, so a
  BAND_DEMATERIALIZED journaled after the last page save found no entry for its epoch and was
  dropped (the walk-off's dead came back), and the BAND_CASUALTIES of a band in view at the save
  found it not embodied and were dropped too. Now `session_start` folds the bodies the pages held
  in Minecraft (`spawned`) into a `recalled` entry of the current epoch (merging with the one it
  has, `fold_spawned_into_recalled`, also used by `recall`), keeps the existing entries, and sets
  `Conflict::replaying`. While it is set a casualty for an epoch with an entry is counted off that
  entry's stack (units and stack both go down, so a later despawn's `entry - survivors` adds only
  the dead nobody reported: no double count), and a despawn finds its entry as in MZB-131.
  `replay_done` (called first thing in `resend_notices`, the point the host has caught the replay up
  at) clears `replaying` and drops whatever entries remain: the bodies are gone with the session,
  nothing will report them, their members count as alive as before. No rtsim-jni change was needed.
  Not state, not a page format change. Fork (limit): records for an epoch newer than the page save
  (a band that was recalled or seen again after it) cannot be matched and are dropped, as they
  always were; the walk-off and the fight of the epoch the pages hold are the ones counted. A
  replayed casualty never routs the band and never brings its boredom forward: `session_start`
  turns a Fighting band into Waiting before the replay, and both branches of `casualties` need
  Fighting, so the replay counts the dead and their signals but not the rout the session itself
  may have had (the band is then handled as a waiting one, which is what the restart made of its
  fight; re-review P3-5). Tests:
  the reviewer's replayed-despawn case (alive 12 drops to 10 after the restart), the entries dropped
  at `resend_notices` (a later despawn is stale), a band embodied at the crash with a casualty and
  a despawn replayed (counted once), the multi-epoch `recalled` round trip through `save` and
  `save_paged`, and two rtsim-jni crash-and-replay tests (a recall with two walk-off deaths; a
  fight's casualties with no despawn journaled), which fail on the old engine (12, not 10 and 6).
  Also tested, not a new rule: a restart in the middle of Peaceful goes on exactly as the run that
  kept going (the same deadline, expiry tick and commands; a jni crash-and-leave test), and two
  Peaceful spans move a deadline by both. A correction to the commit message of 11eb3e62, which
  says each of its tests was checked to fail on the old rule (re-review P3-6): the jni test
  `a_crash_in_the_middle_of_peaceful_then_leaving_it_moves_the_deadline_by_the_whole_span` passes on
  106bda63 too. It is a regression guard for the Peaceful rule of MZB-132 across a restart, not a
  failing-first test; the two crash-and-replay tests above and the rtsim-conflict tests are the
  ones that failed first.
- **MZB-139** (the party's chief flag goes stale; TALK review): when a chief dies of age while out
  with a band, the lazy succession (`ensure_chief` on a feud wake or a plunder) cleared only the
  faction's `chief_out`; the band kept `chief == true`, so a later CHIEF_KILLED for it killed the new
  chief, who is at home. `succeed` now clears `chief` on every party of the faction (at most one
  has it: `chief_out` allows one band at a time). Readers of the flag, all conservative: the war
  chief's extra body (MZB-109) and BAND_CHIEF are written from the flag when a band materializes, so
  a band materialized afterwards (a new wave, a new epoch) has neither; `chief_killed` returns at
  once for a band without the flag, so a CHIEF_KILLED for it counts nothing (no `chiefs_killed`, no
  rout, no second succession) and the new chief lives; the band-home path cleared `chief_out` for a
  stale flag, which could free the new chief's place while his own band was out, and no longer
  does; `ensure_chief` reads the faction's chief, not the flag. So the band carries on without a
  chief. What Java sees: nothing new is sent at the succession. A chief body already spawned in
  Minecraft stays until the band despawns; killing it sends a CHIEF_KILLED the engine drops (it is
  no longer a chief body to the engine). Later BAND_MATERIALIZE commands of that band have no
  BAND_CHIEF and no MEMBER_CHIEF stack. Tests: rtsim-conflict `mowzie_records` (the old chief dies
  of age out with the band, then CHIEF_KILLED, the new chief at home lives; the control, a living
  chief, still falls; a band materializing after the death has no chief body where the control has
  one; the old band coming home leaves `chief_out` while the new chief's band is out); the first,
  third and fourth fail on the old rule.
- **MZB-140** (the heir's text, killed or aged): `succeed` told a death of age from a killing by the
  killer alone, so a chief killed by a mob (BY_MOB, BY_OTHER) was logged as dying of age. It takes
  `killed: bool` as its last parameter (the TALK branch adds the same parameter in the same place:
  `chief_killed` passes true, `ensure_chief` false). The text is "dies of age" for false, "is
  killed; the heir takes over, holding the grudge" for a player's kill, and "is killed, by no
  player; the heir takes over" for a kill with none. Test: the three texts. Fails on the old text.
- **MZB-141** (a replay over several hand-overs; re-review P2-1): the bodies a session start keeps
  for the replay (MZB-138) were dropped by `replay_done`, which `resend_notices` calls, and
  `rtsim-jni`'s conflict `simulate` called `resend_notices_if_due` at the end of every step, so the
  first hand-over of a restart ended the replay. Java hands its journal over while it fits (about a
  thousand 64-byte BAND_CASUALTIES in a minimum-size buffer, about 4,000 in 256 KiB), so a long
  journal spans hand-overs, and a despawn or a casualty in the second found nothing kept (the
  reviewer's run ended with 12 alive, not 10, after 1,500 filler records). `simulate` now resends
  the notices (and so ends the replay) only once the engine's replay has caught up with the
  session's first live observation (`applied_through + 1 >= session_first`); the other trigger, the
  first live conflict record in `journaled`, is unchanged. Cost: a restart whose journal tail is
  lost (cannot happen with a world save, whose journal and `next_obs` are saved together) would
  wait for the first live record instead of the first step. Test (rtsim-jni): the walk-off crash test
  with 1,500 filler records between the recall and the despawn on a `MIN_CAPACITY` engine; fails on
  the old `simulate` (12, not 10). The jni test helper `reopen_with` takes the capacity and hands
  the journal over until Java has sent it all.
- **MZB-142** (a walk-off death counts live as in the replay; re-review P2-2): MZB-138 counted a
  BAND_CASUALTIES for a band that is not embodied off its recalled stack only while `replaying`.
  Live, the same record after a recall (the player left, the band walks off, a mob or the player
  kills a member before the despawn) was dropped at the `embodied` check, so a restart's replay
  counted a death, a killing signal and a breaker the session itself never did (casualties 0 vs 1,
  grievance 15.999 vs 23.999, breaker None vs Some, in the reviewer's run). The `replaying` term is
  gone: a casualty for a band not embodied counts when it holds a recalled stack of the record's
  epoch, replaying or not. The despawn removes the stack, so a straggler after it is still no
  death, and the despawn counts only the dead nobody reported (units and stack went down together,
  no double count). The decision is to count it in both (the member did die, and a player's kill is
  an act against the faction wherever it happens). The rout and boredom branches of `casualties` need
  Phase::Fighting, which the player-left recall turns into Waiting at once, so a walk-off death
  never routs a band live either (as in a replay, MZB-138). `replaying` now only says that the
  stacks kept by a session start are dropped at `replay_done`. Test: the same player's killing of a
  recalled band ingested live and by a restart (casualties, alive, breaker, grievance equal and
  not zero), the despawn after it and a straggler after that; fails on the old rule (casualties 0).
- **MZB-143** (a tribute to a bored envoy on its way home; re-review P2-3, a regression of
  MZB-137): MZB-137 found the envoy's demand through `parties[p].feud`, but `go_home` clears an
  envoy's feud (it serves no feud while it walks home), so a payment to an envoy that got bored and
  left with its demand still open was a gift: the demand ran out and the feud climbed, though the
  player had paid. `pay_tribute` now finds the demand that names the envoy (`demand.envoy ==
  Some(p)`) among the faction's feuds, so it needs no feud on the party; a payment to an envoy whose
  demand is over (settled, expired, withdrawn) still names none and stays a gift (MZB-137's rule,
  tested as before). The scan is over one faction's feuds, once per payment. Test: an impatient
  culture (patience 1 day), the envoy leaves bored with the demand open, 6,600 ticks before its
  deadline, and a full payment to it settles PAID as a tribute, not a gift; on the MZB-137 lookup it
  was a gift and nothing settled.
- **MZB-144** (ENVOY_ANSWER is an envoy's; re-review P3-1): an answer for a party that is not an
  envoy (a stale or misrouted record, never sent by the bridge) used the party's feud as the demand's
  target, so for a war band it booked the insult and refused the feud's demand. The ENVOY_ANSWER
  arm now returns at once for a party that is not an `Envoy`, for both answers (ANSWER_REFUSED is
  never sent by the bridge either). Test: ATTACKED for a raid band that serves the player's feud
  changes no grievance and settles nothing; it booked the insult on the old engine.
- **MZB-145** (a blow from another player; re-review P3-2): the ATTACKED arm used the envoy's feud
  as the actor to refuse and the striker as the insult's actor, so a second player striking an envoy
  sent to the first refused the first player's demand and pointed the striker's feud at the first
  player's district. Ruling: the blow is the striker's insult. If the striker is the demand's target
  (the feud's actor, found as in MZB-143 for an envoy going home) nothing changes: the Killing
  signal of `envoy_insult_kills` at the envoy's place against its target's district, then the
  refusal. Otherwise the same signal is the striker's alone, on the striker's own feud, at the
  envoy's place, against no district (the target's is not the striker's), and the demand is not
  refused: it stands to its target, who can still pay it. The settlers' envoy has no player for a
  target, so any player's blow on it is a stranger's: the insult is booked, the settlers' demand runs
  to its deadline (it used to be refused at once); a fork, since the old reading made a blow on the
  villages' envoy a refusal. An answer for an envoy with no feud and no open demand naming it (its
  demand is over and it is going home) still does nothing. The Java side agreed to the same rule
  (`ConflictBridge.struck`): any player's blow lifts the guard, because the guards fight whoever
  strikes; a blow from the demand's player (or from any player when the demand is to none) closes
  the demand as before (`insulted`: the envoy takes nothing more and says "struck"), answered once;
  another player's blow is answered once per player (`struckBy`) and leaves the demand open to its
  player, so the envoy still takes that player's tribute and the engine, which has not refused it,
  agrees. Before, the first blow of any player closed the demand on the Java side only, and a later
  blow by the target was never sent. Not run: the GameTests (the lead's order); the Java change
  compiles. Tests (rtsim-conflict): a stranger's blow leaves the demand open, adds nothing to the
  target's grievance, gives the stranger an insult with a focus of no district away from the camp,
  and the target's own blow then refuses it as ever; fails on the old arm (the demand is refused).
- **MZB-146** (the lazy succession before a CHIEF_KILLED; re-review P3-3): `chief_killed` read the
  band's flag before anything had asked the world's knowledge whether the faction's chief was still
  alive. A chief who died of age out with the band, with no feud wake or plunder since to run the
  lazy succession (`ensure_chief`), still had the flag on his band, and a CHIEF_KILLED for his body
  ran as a killing of a dead man: a count, a rout, an heir taking over holding a grudge for a
  killing. `chief_killed` now runs `ensure_chief` for the band's faction first (a chief alive in the
  world's knowledge changes nothing), so the succession is the one for a death of age, `succeed`
  clears the flag, and the record is dropped as in MZB-139. Test: the chief dies of age out with
  the band after the engine has run past its due events (so no wake asks for the chief), then a
  CHIEF_KILLED: one succession, no chief fell, no rout, the flag gone, the heir lives. It fails
  without the new call (chiefs_killed 1). Replay is unaffected: the order of the journal decides, as
  it did.
- **MZB-147** (what Java does for a chief body killed after the succession; re-review P3-4):
  checked `ConflictBridge.journalLoss`, which sends CHIEF_KILLED (with an op id) for any slot marked
  `chief`, so for the old chief's body after the lazy succession (MZB-139, MZB-146). Nothing visible
  comes of it on the Java side: the body dies and drops its loot as any mob does (the drops carry the
  record for Carry On, a journaling matter), `chiefsKilled` is a statistic, and
  `ConflictNews.post(CHIEF_KILLED, ...)` is a no-op counter until the in-world news lands. The rout
  and the heir are the engine's answer to the record, and the engine now drops it, so the band
  carries on and the new chief lives, as MZB-139 says. The record is harmless and nothing in Java
  changes. A trap for the news slice: it must not post "the chief was killed" from `journalLoss`
  alone, since the engine may have dropped the record; post it from what the engine did (the
  heir's succession by a killing, `succeed(..., killed: true)`), or accept a stale line.

## M4 Camp reconciler, engine

- **MZB-150** (the five camp records got their engine half): CAMP_CENSUS (0x080F), CAMP_OUT
  (0x0813), CAMP_RECONCILE (0x8808), CAMP_BODIES (0x0812) and CAMP_DEATH (0x080E) are ingested,
  booked and (the reconcile) sent by `crates/rtsim-conflict/src/engine/camp.rs`, and are off
  `Conflict::later`; each later slice takes its own kinds off that list (MZB-23). No record's layout
  changed, so `boundary.rs`, `docs/boundary.md` and rtsim-jni's decoder are as M1 left them.
  `tests/mowzie_records.rs`'s deferred-record test spends an op id on CHIEF_PAID instead, and cites
  this choice. A fork (reported): the lead's brief said to change a layout only if the plan
  demanded it, and nothing did.
- **MZB-151** (`camp_bodies`, a culture flag, gates the ledger): the plan keeps a ledger for a camp
  "whose bodies a census counts"; a culture says so with `camp_bodies: true` in its JSON, which
  `frostmaw.json` and `umvuthana.json` set. Every other culture has no ledger and reads no camp
  record, but the pool's room (MZB-171) applies to it too, which moved `sim conflict`'s state
  hashes. Amended (review of 408d4a07): M4 first said every other culture was untouched. A fork
  (reported): the plan does not say how a culture opts in.
- **MZB-152** (where a ledger starts): at FACTION_FOUND (`camp_found`: the pool is the census's
  home bodies bounded by the culture's `pool_cap`, the camp's cap that count, one at least, bounded
  by the same, every slot owed for the first census's strays to fill), or lazily (`camp_create`, from
  the books as they are) for a faction found some other way. A den found empty gets cap 1 and pool 0
  and refills at the culture's rate (1 a 6 days) until M6's arrival renewal; a grove found with 4 of
  its 9 bodies has cap 4, so a grove cannot recruit past what was found. So a grove found empty or
  part-cleared, as every grove of an existing world may be, has the cap of the bodies it was found
  with (one at least) and raids only when its room fits a party (MZB-171): an empty one, cap 1, does
  not raid until M6's and M9's arrival renewal grows it. A fork (reported): the plan gives the cap as
  the culture's and says nothing of a camp found part-empty; keeping the cap at the found count is the
  choice that cannot field a body the world never had, and M6 and M9 own the arrival that grows it.
- **MZB-153** (slots, generations, ops): a slot takes the lowest free number; its generation is
  `derive_gen(key, slot, tick, last retired)`, never 0 (0 names a stray) and never the generation the
  slot last retired. The engine's allocator byte for a camp's ops is 0x43; a CAMP_RECONCILE's op is
  `derive_op(key, epoch, seq)`, so a decision made again after a restart has the same op. An answer
  line is booked once by (op, line) (MZB-169) and a CAMP_DEATH by its slot and generation (MZB-170);
  neither op goes into `applied_ops`, which first held them and grew with every death of a farmed
  grove.
- **MZB-154** (the pool stays closed-form, and slots follow it on a read): `pool_at` stays a closed
  form, so a feud, the bench and a save cost what they did, but its refill now stops at the cap less
  the members away for every culture (MZB-171; M4 first said it was unchanged). A change of the
  slots (`camp_sync`, called by every ledger entry point that changes them, and by `camp_bodies`
  after its epoch gate, MZB-172) first adds one owed slot for each whole member the pool gained
  since the last, each with a generation from the tick it became whole (`whole_at`). A camp that is
  in books `Ev::Refill` (wheel tag 7) with a stamp for its next whole member; one that is out books
  none, and its next census spawns what it gained. A state hash must sync first; `camp_audit` is
  `&self` and adds nothing, so it reads the books as they are, and the slots at home may be fewer
  than the whole warriors the pool holds (MZB-172). Reason: a per-tick or per-refill loop over every
  camp would break D1, and the closed form already says when a member is whole.
- **MZB-155** (waiting for whole bodies counts the slots a draw may take): M2's quorum for a
  party that must muster whole (`ceil(size / 2)`, MZB-52) is unchanged, but it is now measured in
  the slots a draw may take (home, no line in flight). A feud with fewer waits on `BLOCKED`
  (`u64::MAX / 2`), which books no event; `unblock` (an answer, a void, a CAMP_OUT, a return to the
  pool) wakes the waiting feuds and plans a refill, so a busy line wakes a feud a handful of times,
  never once a tick (`sim mowzie` run 10 asserts `feud_wakes`). Reason: a member whose body is
  being spawned or taken cannot be drawn, and the old closed-form wait did not know it.
- **MZB-156** (the draw order and `misses`): found bodies (misses 0) first, then missed, then owed,
  by slot number within each, never a slot with a line in flight, and never more than the smaller of
  `floor(pool)` and the slots it may take (the chaos run checks every draw). A slot drawn keeps its
  `misses` and `missed_at`: a body missed at a census and drawn is I3-unverified until its set-out
  answer or the next census, and a member that comes home is written off on the clock it was on.
- **MZB-157** (the write-off): `WRITE_OFF_MISSES` 3 censuses that miss a home body, the third at
  least `WRITE_OFF_DAYS` 2 days after the first (both PLACEHOLDER, as §8's row says); the slot is
  freed, the pool loses the member, and its generation is retired as "not died", so a body found
  later is a stray and is adopted or removed like any other. A generation retired at a death is
  retired as died: a body of it found later is removed.
- **MZB-158** (the census's table): Body found keeps its slot; Body missed counts a miss; Owed found
  becomes Body; Left found with its party out is taken, with its party mustering stays; Left absent
  becomes Gone; Gone found becomes Left (and is taken or kept by its party's phase); Dead found is
  removed, Dead absent is freed. Strays and tagged bodies no slot holds fill owed slots, then new
  slots while the pool and the members away are under the cap, and are removed above it; a body of
  a generation retired at a death is removed. A census never ends a party (§4.16).
- **MZB-159** (a replayed census): a census ingested while `Conflict::replaying` is set (M3's window,
  from `session_start` to `replay_done`, MZB-138 and MZB-141; before the merge rtsim-jni set it around
  each record whose `OBS_SEQ` was before the session's first, MZB-178) only marks its camp out:
  the session that took it is over, its epoch is not the plugin's, and it orders nothing. The
  records that follow it (answers, deaths) are booked as usual (an answer by the action it carries,
  MZB-168), and the census still names its epoch as the camp's latest (MZB-169). Not a fork: this is
  the plan's own wording (MOWZIE.md §4.9: "A CAMP_CENSUS replayed from an earlier session only marks
  its camp out"). The reason, for the record: a census has no state to skip by op id, and booking one
  against a camp that is out would adopt strays on a dead epoch.
- **MZB-160** (lines are void, never resent): at CAMP_OUT, and at a session start for every camp, a
  camp's lines in flight are voided and nothing is sent again; at most one line is in flight per
  slot (`send_lines` guards it as well as its callers, which makes the guard defence only); orders
  go only to a camp that is in; a CAMP_OUT for an epoch that is not the camp's ends nothing.
- **MZB-161** (an answer with no line in flight): replaced by MZB-168. M4 first booked it by the
  action the slot's state implies (Owed a spawn, Left or Dead a take, Body and Gone nothing), which
  was a guess; the review of 408d4a07 showed it lose members.
- **MZB-162** (a death books the member wherever the books have it): a CAMP_DEATH for a slot and
  generation the books hold frees it: at home the pool loses a member (the generation retired as
  died), away the party loses that unit in any phase (`member_died`) and a party left with none ends
  through its own `home` (`go_home`, or `end_fight` when fighting), so the pool takes back its
  survivors and `away` falls in one place; dead, left, the slot is freed. A stray's death books
  nothing, and a death for a generation the slot no longer holds books nothing. A fork (reported):
  the plan's table gives each row; an emptied party's end is the choice, `casualties`' own.
- **MZB-163** (the hand-back): `camp_home` turns Left into Body and Gone into Owed; an embodied party
  with puppets at a camp that is in gets a hand-back line for each Gone slot, else spawn lines. No M4
  flow reaches it (the embodied walk home is M10's), so one test forges an embodied party and the
  scenario's run 10 names the case "forged state". A fork (reported).
- **MZB-164** (what M4 leaves for the slices that own it): a chief killed at the camp and the den's
  arrival (M6), the Umvuthi's slot, heir, seat, Rule U and throne lines (M9 to M11), BAND_MARCH and
  the walk home in bodies (M10), AMBUSH_SPOT's removed flag (M8). A held line (Rule U, the
  Frostmaw's spawn wait) is Java's (M5) and is not emulated: the fake plugin applies every line in
  the tick it arrives, and answers it after `answer_ticks`. One more is M10's: a CAMP_DEATH for a
  slot left behind (its body still in the camp while the party musters or is out) whose party is
  embodied makes `member_died` take one unit off the party's books, but the puppet of that unit
  stays in the world, since the body that died is the camp's and not the puppet; the books and the
  world then disagree by one puppet until the party is recalled. No `debug_assert` fits: the chaos
  run reaches it (counted at this commit: 15 times over its 48 camps, 14 for a left slot and 1 for a
  gone one, with the player watching a fight), so it is a known state and not a bug in M4's flows. M10, which owns the embodied walk, ends that puppet with its party's recall.
- **MZB-165** (pages): the conflict page magic is RTC6 and the meta page's version is 5, after the
  merge with M3 (M3 took RTC5 and meta 4 for the recalled stacks kept per epoch, MZB-131; this
  branch's own format change comes after it): a camp's ledger (slots, retired generations, throne,
  refill stamp) is saved with its faction, and the stats grew six camp counters; a camp's page also
  holds the latest epoch a census named and the (op, line) answers booked in it, each with its
  record's journal sequence (MZB-169, MZB-177). No migration (no backward compatibility before
  release). This changes a page format, so the rtsim-view fixtures and hashes tests and rtsim-host's
  `gametest_worlds_open_in_the_host_with_their_saved_state` fail until the fixtures are regenerated
  from fresh GameTest worlds of the merged build, which the landing does.
- **MZB-166** (the emulated plugin and runs 8 and 10): `synth::FakeConflict`'s camp worlds apply a
  reconcile in the tick it arrives if the epoch is the camp's, else answer it stale; answers can be
  late, lost (`lose_answers`, one per order), replayed, dropped at a restart (`drop_queued`) or split
  over several records (`split_answers`, that many lines to a record, as holders in several chunks
  answer). Holders are emulated as MOWZIE.md §4.9 has them: a line applied and done, and a death,
  leaves its record on a holder (`holders`, `death_holders`) until the engine has it, and with
  `holders_again` the restart and each camp-in, before its census, journal those records again
  (`journal_holders`, MZB-175), which the engine books by the action each carries and skips by
  (op, line). `CampRun` drives an engine against one,
  with `decide` and `census_held` for crash tests, `track` and `settle` for the ghosts (MZB-167),
  and `check`: I1 and I2 by the engine's own audit, I4 by counting each (op, line) the plugin
  applied, I3 and I5 against the world; I6 is by construction, the engine never sees the world. Run
  10 is the scripted cases on worlds of fixed seeds (they check exact slot counts, which a raid's
  follow-up party changes from world to world) and chaos over 64 camps from `--seed`, whose
  plugin follows the seed (a lost record stays lost; holders journal it again; holders journal it
  again one line to a record), settled at the end and asserting that the engine's write-offs are
  exactly the ghosts, and none where holders journal again (MZB-167, MZB-175); run 8's part saves and restarts with lines in flight three ways.
  Both are printing checks that must pass (`sim mowzie` exits 1 otherwise, and a test runs it).
  Where run 10 still falls short of MOWZIE.md §7: no line is held (Rule U's holds, the Frostmaw's
  spawn wait: Java's, deferred by MZB-164, so the fake plugin applies every line in the tick it
  arrives) and no heir is seated, no Umvuthi marches (M10), and a chief's or the Umvuthi's death
  waits for M6 and M9; what it does reach is camps in and out with a player at the edge, musters,
  homecomings with hand-backs, refills, deaths at home, left behind and of strays, answers lost,
  late, replayed and stale, restarts and saves, with the invariants checked after every event.

- **MZB-167** (what a lost answer costs: the ghosts, counted exactly): an applied take or set-out
  whose answer never reaches the engine (lost, or in the journal's tail at a restart) once left a
  slot Left though the body was gone; if the party then came home the slot became Body, three
  censuses missed it and it was written off. M4 first called that a loss of the emulation and not
  of the plugin, because the real plugin journals the answer again from the marker the body left
  (§4.9). That premise was wrong: the marker's record, booked by the state its slot has by then
  (Body), booked nothing, so the real plugin lost the member too. MZB-168 fixes it: the record
  carries its action and is booked by it, so Body with a set-out done is owed and spawned. With the
  holders' records journaled again at the restart (the real plugin's), the four members of a raid
  whose set-out answer was in the journal's tail are owed, spawned at the next census, and none is
  lost; with the record lost for good (`holders_again` off), a party that gets home before the next
  census leaves four ghosts, and four members are written off. A ghost is a slot that is a body at
  home with no body in the world and no record on its way to the engine; three censuses over two
  days write it off, and a draw first rescues it (its set-out finds nothing to do and books the body
  gone). A record only a holder has is cover where the plugin journals holders again
  (`holders_again`, MZB-175): the emulated holders journal at a session start and at each
  camp-in, before the census, as the real ones do when their chunks load and as MZB-174 makes the
  census wait for. A footprint holder's record that arrives at camp-in, before the new census,
  still carries the latest epoch and books (MZB-169); one that arrives after a census of a later
  epoch is of an older epoch and books nothing, and for it the table is what counts. An answer
  lost with no restart after it, which the chaos run does, is repaired at the next camp-in by the
  holders. The chaos run ends with `settle` and asserts that the
  engine's write-offs are exactly the ghosts the run made and that none is left (`CampRun::track`
  finds them by effect after each step of a run), and (MZB-175) that nothing is written off where
  holders journal again; `lost_members`, which the assertion used, was an upper bound that any
  loss passed, and is gone. Tests:
  `a_lost_set_out_answer_costs_the_ghosts_it_made_and_holders_make_none` (four against none, each
  equal to its ghosts), `a_body_whose_death_is_on_its_way_is_no_ghost`,
  `answers_split_into_records_leave_the_books_as_whole_ones_do` and
  `the_check_names_an_order_applied_twice`.

- **MZB-168** (CAMP_BODIES' lines carry their action; replaces MZB-161): a line's answer is
  `u32 line, u32 slot, u32 generation, u8 action, u8 outcome, u16 reserved`, 16 bytes as before (so
  the record's size is unchanged), with the action of the CAMP_RECONCILE line it answers
  (`ACTION_*`), and the engine books the world fact by that action: not by the slot's state, which
  was MZB-161's guess, and not by the line in flight, which a CAMP_OUT or a session start voids
  while Java still answers it. The reviewer's case: a grove of 9 raids, Java applies the four
  set-outs, the answer waits in a journal tail past CAMP_OUT, the party gets home abstractly (Left
  becomes Body), and the answer then books nothing for a Body slot; four censuses miss four bodies
  and write four members off. By its action the answer says "set out, done": the Body slot is owed
  and the next census spawns it. The outcome constants are `u8` now. Layout changed in
  `boundary.rs` (encode, decode, fixture, generator), docs/boundary.md's row 0x0812 and
  MOWZIE.md's record table; rtsim-jni uses the same codec, so its decoder follows. A fork
  (reported): the lead's decision after the review, not the plan's wording; the main session
  accepted it on 2026-10-23. MOWZIE.md's §4.9 prose,
  ledger 7 and crash table rows 2 to 4 say so now (MZB-174).
- **MZB-169** (an answer line is booked once by (op, line), and only in the latest census's
  epoch): one op has several CAMP_BODIES records, since a holder's record is journaled again at load
  and held lines are answered later, so the skip is per line, not per op (a four-line set-out
  answered as two records of two lines lost the second under the op-only skip). A camp keeps the
  latest epoch a census named (`seen`, replayed censuses too) and the (op, line) answers booked in
  it (`answered`, sorted, saved with the ledger), emptied at each census of a new epoch; an answer
  of any other epoch books nothing, since a later census read the world after Java applied the
  lines, and the fact the answer reports is older than the books. That is the prune by epoch: the
  set holds one epoch's lines (a few dozen a day at a raided camp) and cannot grow without bound.
  Answers booked by action need the skip: a copy of a set-out answer, delivered after the party is
  home and the bodies are back, would turn a living body's slot owed and spawn a second body.
- **MZB-170** (camp records keep no op id): CAMP_DEATH is idempotent by its slot and generation
  (the first frees the slot, a copy finds none), and CAMP_BODIES by (op, line), so neither goes into
  `applied_ops`. `Obs::op()` no longer returns the death's op and rtsim-jni's `conflict::op_of` no
  longer gives it to `Replay`, which kept it for the session. The op stays in the record, as
  Java's name for it. The engine acknowledges by sequence (`ackedObsSeq`, docs/boundary.md §1), not
  by op id, so Java's dropping of a marker's record is unaffected. A fork (reported): the lead's
  decision; a death replayed after a slot was freed and made again under a new generation is
  absorbed by the generation, which `derive_gen` never repeats for the slot's last life.

- **MZB-171** (the pool's room is every culture's): MOWZIE.md ledger 6 says `pool_at`'s refill
  stops at the cap less the members away, "at every camp", and the code does so for every culture,
  ledger or not: a camp's cap is its culture's `pool_cap` (set at `add_faction`, bounded by a census
  that finds fewer) and `away` is kept for all. `pool_at` is the refilled pool bounded by that room
  and never below the smaller of the stored pool and the room, so a pool above the room is clamped
  to it (the code comment said it stayed; fixed); `pool_add` clamps to the room, and `pool_ready`
  is `BLOCKED` for a party larger than the room, which only a return can end (`unblock`). MZB-151
  and MZB-154 said other cultures were untouched and `pool_at` unchanged; both were wrong. What
  moved: M2's refill went to `pool_cap` from the moment a party left, so a goblin camp of 60 that
  sent a raid of 12 had the pool back to 60 within 6 days while the band was out (72 members
  fielded), and the survivors were clamped off the pool when they came home. Now the pool and the
  members away never pass the cap, and survivors come home to room. A second war party waits for a
  first one's return only when the room is short of it, which no default culture's numbers do but
  the two with a ledger (a den's cap is 1; a grove found with 4 of 9 bodies has cap 4); for a
  goblin the difference is the pool's value. In `sim conflict`'s JSON only the state hashes
  moved (the three variants and the restore line): the checks, logs, event counts, wire bytes and
  archive rows are M2's. The page format moved them too (MZB-165), so the room alone was measured,
  as M2's refill on M4's format against M4's: ignore e5d0d1d99fbe97d5 against e271b3b7af114664,
  tribute e7a65dc23ea5e53e against 788cce5224a8bb3f, converter 699a30ab49314569 against
  8f4a3f1dafeb5a7c. At day 24 the goblin camp's pool is 57.92 in the tribute and converter runs
  (60 less the two warriors lost) where M2's refill left 60.00, and 48.65 against 51.96 in the
  ignore run, where a plunder party of 8 is still out. Test:
  `every_cultures_pool_refills_only_to_its_cap_less_the_members_away` (a goblin camp of cap 20 with
  raids of 12 out for 6 days: pool plus away never passes 20 in 60 days, where M2's formula reached
  8.08 + 12 on the first day out). A fork (reported): the lead's decision to keep the room for every
  culture, as the plan's ledger 6 words it, over a flag that would keep M2's pool for a culture
  with no ledger; the main session accepted it on 2026-10-23.

- **MZB-172** (the review's smaller fixes): four things the review of 408d4a07 found, each with a
  test that fails without it. (1) An adoption answered "nothing to do" (the stray died before the
  order was applied) leaves the slot owed with no body there; a camp that is in now sends it a
  spawn line, as it does for a body found gone, where it used to wait for the next census (test:
  `an_adoption_with_nothing_to_adopt_is_followed_by_a_spawn`). (2) Every change of a ledger's slots
  follows a sync: `camp_bodies` syncs first (it freed a dead slot with the pool's gained members
  not yet added, so the lowest free number a later sync takes depended on whether anything had
  synced in between), and `camp_audit` is `&self` and adds nothing (the slots at home must be no
  more than the pool's whole warriors; a gain not yet added is the books as they should be). An
  audited run is now the run not audited: the chaos run's state hash is the same with its audit
  after every event and without (`auditing_after_every_event_changes_nothing`, a guard: it did not
  tell the old syncing audit from this one in 32 seeds, so the teeth are in
  `the_audit_leaves_the_slots_the_pool_gained_unadded` and
  `a_refill_while_the_camp_is_out_is_spawned_at_the_next_census`; the answer's sync is
  `an_answer_syncs_the_books_before_it_frees_a_slot`). Two tests that read the slots after an audit
  read the pool instead, since the audit no longer adds them. (3) Two guards no flow reaches, a
  slot left or gone whose party is not in the books (a party leaves the books through `home`,
  which settles its slots first), have a `debug_assert!` at the census and at the answer; the
  release arms stay as they were (two `should_panic` tests, debug builds only). (4) A war party is
  never drawn larger than the slots at home with no line in flight: every unit kind keeps one at
  least, so a make-up of two kinds from a camp with one warrior at home was two units on one slot
  and `away` one too many; the last kinds now give theirs up, so a party can have fewer kinds than
  its make-up when the camp is that short, and `send_party` sends nothing when `have` is 0
  (`a_party_is_never_drawn_larger_than_the_slots_at_home`). The member-died puppet is noted in
  MZB-164. A fork (reported) for (4): the plan says a party takes at least half its make-up and
  does not say which kinds go first.

- **MZB-173** (what M4 costs `bench scale`'s factions sweeps): measured once, M2's head (4b07ea84)
  against M4 as first built (408d4a07's engine; the review's fixes came after and were not measured
  again: they add a clamp to `pool_at` and an (op, line) lookup per camp answer, which the bench's
  cultures never send),
  `bench scale --full`, one run of each in turn. The machine was loaded (load average 32.7, 31.3 and
  28.7 on 24 threads, other sessions' builds and runs), so a few percent either way is noise, and
  every cpu figure of one run is inflated together.

  | sweep | factions | cpu ms/day M2 | cpu ms/day M4 | save KiB M2 | save KiB M4 | heap MiB M2 | heap MiB M4 |
  |---|---|---|---|---|---|---|---|
  | closed | 16 | 0.12 | 0.16 | 8.6 | 8.6 | 0.5 | 0.5 |
  | closed | 64 | 0.81 | 0.86 | 31.3 | 31.5 | 1.7 | 1.7 |
  | closed | 256 | 5.10 | 5.25 | 125.7 | 126.4 | 8.1 | 8.1 |
  | closed | 1,000 | 19.82 | 20.06 | 511.7 | 514.7 | 35.2 | 35.2 |
  | paged | 1,000 | 35.11 | 36.93 | 511.7 | 514.7 | 34.3 | 34.2 |
  | paged | 2,000 | 112.74 | 117.64 | 1027.2 | 1033.2 | 69.9 | 69.9 |
  | paged | 4,000 | 145.54 | 151.38 | 2058.6 | 2070.7 | 141.6 | 141.6 |

  The cpu is 1 to 6% above M2's from 64 factions up (the 16-faction row is 0.04 ms a day, below what
  the load moves), the save at most 0.6% larger (the cause was not traced), the heap the same. Two
  verdict lines differ and neither is M4's: `factions-paged resident` fails its slope on both
  (0.920 on M2, 0.921 on M4), and `factions closed cpuperunit` fails on M2 (0.234 against a declared
  0.20) and passes on M4 (0.178), which is the load, not a speed-up. Slots a day and conservation
  are as before (0, ok). The bench builds goblins, bandits and raiders (`synth::many`), none with
  `camp_bodies`, so this is the cost of the pool's room and the page's new fields, not of a ledger.

- **MZB-174** (the contract that makes the epoch gate safe, an M5 obligation): MZB-169's gate books
  an answer line only while its epoch is the latest a census of the camp named, so a holder's record
  that reaches the engine after a census of a later epoch books nothing. The review (P2-1) found
  that gate drops a late holder record, and the decision is a contract and no engine change:
  **Java sends a camp's census only after every holder in its footprint has journaled its record
  again.** A footprint holder loads with the camp's chunks, so M5's census (sent when every
  footprint chunk is FULL with its entities in) waits for those records, and they reach the engine
  carrying the epoch that is still the latest, before the new census; the books take them by the
  action each carries. MOWZIE.md §4.9 (the CAMP_BODIES bullet, the journal-again sentence, crash
  table rows 2 to 4) and ledger 7 say so, in the reviewer's words. The cost that remains and is
  known: a holder outside the footprint whose record arrives after a census of a later epoch is
  not booked. That census books the missing body by its table (a body left becomes gone), and a
  body at home is missed and written off at the third miss over two days, so one member is lost.
  This happens only after a crash, and the camp refills the member at its rate (`pool_at`). The
  engine cannot tell that holder's record from one of a superseded census, which is why it books
  nothing rather than guess (MZB-169). For M5: wait for the holders in the census's footprint;
  a holder outside it is not waited for, and is the cost above.

- **MZB-175** (run 10's exact ghost count had no teeth; the fake plugin follows MZB-174): the review
  (P2-2) found that putting the engine back to booking by state (MZB-168) or skipping by op
  (MZB-169) left `sim mowzie` at exit 0: holders journaled only at a restart, so in the chaos run a
  lost answer was a ghost the exact count simply counted, whatever the engine did with the records.
  Four changes. (1) With `holders_again` the emulated footprint holders journal their records again
  at each camp-in, before the census (`journal_holders_of`, at the census's own tick, the records
  queued first), as MZB-174 says the census waits for; the restart still journals them all. (2)
  `CampRun::track` counts a record a holder still has (a line, or a death) as cover where holders
  journal again: the member is no ghost, since the next camp-in or restart books it. (3) The chaos
  run, in the camp tests and in run 10, asserts `camp_written_off == 0` where `holders_again` is on,
  and keeps the equality with the ghosts gone for the seeds where it is off. (4) Run 10 gains three
  scripted cases with exact outcomes, the grove of nine whose raid's four set-out answers are lost:
  the records land after the party is home (whole, and one line to a record), and the records are
  journaled again at the next camp-in with no restart; each ends with no member written off. Tests:
  `holders_journal_their_records_as_the_camp_comes_in_before_its_census` and
  `a_death_only_a_marker_carries_is_no_ghost_where_holders_journal_again`. What each guard catches,
  measured by putting it back (one run of the camp tests and of `sim mowzie` each): booking by state
  (408d4a07's rule): 5 camp tests (the chaos test at seed 7: 3 written off, the late set-out answer,
  the camp-in case, a superseded epoch, one `should_panic`) and `sim mowzie` exit 1 on the three
  scripted cases (4 written off each); skipping by op (a record of an op already seen books
  nothing): 3 camp tests (both split-answer tests and the chaos test) and `sim mowzie` exit 1 on the
  one-line-to-a-record case and on the chaos (6 camps, the first three with 3, 3 and 1 members
  written off); the fake without camp-in journaling: 3 camp tests and `sim mowzie` exit 1 on the
  camp-in case (4 written off); the fake without the cover rule: one camp test, and `sim mowzie`
  exit 0, since its chaos seeds meet no death only a marker carries. The sim's chaos camps alone (64
  from `--seed`) do not meet the by-state case, which the reviewer's own 44 holders-on runs did (9
  written off by state, 17 by op, 0 with the fix): the scripted cases are what make `sim mowzie`
  fail for it. A fork (reported): none; this is the reviewer's plan.

- **MZB-176** (the audit's I2 has its lower bound back): MZB-172 made `camp_audit` read-only, which
  left I2 with only its upper bound (the slots at home are no more than the pool's whole warriors
  now). The lower bound is the pool's stored value: it is what the pool held when it was last moved,
  and every move of the pool follows a sync and moves the slots with it (`pool_return`, a draw's
  `pool_shift`, a death), so at least `floor(pool)` slots are at home, the refill since being the
  gain the next sync adds. A slot taken from home with the pool left as it was is now named (`I2:
  ... the pool held N when it was last moved`). It holds over every camp test, the chaos runs and
  `sim mowzie` (its chaos checks every event). Tests:
  `the_audit_names_a_pool_the_slots_at_home_fall_short_of` (a home slot made Dead by hand; and a
  pool that refilled for 30 days with the slots unadded, which is no shortfall). A fork (reported):
  the review said "at the camp's last sync"; the pool's stamp (`pool_at`) is that moment, since a
  sync comes before every move and the books hold no other mark of it.

- **MZB-177** (a camp forgets the answers a durable world save covers): the (op, line) answers
  booked in a camp's latest epoch (`Camp::answered`, MZB-169) grew with every answer until the next
  census, and a grove can stay in one epoch for months. Each answer is now kept with the journal
  sequence of the latest record that carried it (`Answered { op, line, seq }`; a holder's copy that
  is skipped raises it, so a copy keeps the answer until the copy is durable too). The host reports
  the highest durable sequence at WORLD_SAVED (`Conflict::durable_obs_seq`, set where
  `Replay::forget_ops_through` is called, which is the rule docs/boundary.md §2.3 gives for op ids)
  and `Conflict::obs_seq` carries the record being ingested. A camp forgets the answers at or below
  the durable sequence when it next books an answer, so only the camp that grows is touched: no scan
  of the faction table (its pages would load and dirty), and the set stays at what the last save has
  not covered. A host with no sequences (the simulations, whose records carry 0) keeps its answers
  to the end of the epoch. The camp page gains each answer's sequence (a varint); the layout change
  goes with group E's RTC6 bump (unreleased, no migration, MZB-165's rule). Cost, accepted as
  TAKEN's is (docs/boundary.md §2.3: by state after the op id is forgotten): a holder's copy
  journaled after the save but before Java has processed the engine's acknowledgement and dropped
  the record (an unloaded and reloaded holder chunk in that gap) is booked a second time, by its
  slot's state. A spawn answer or an answer for a slot the action cannot move changes nothing; a
  take answer for a slot spawned again since turns it owed once more, and the next order or census
  sets it right (Java's spawn does nothing where a body with that slot and generation is loaded).
  Tests: `a_booked_answer_is_forgotten_once_its_record_is_durable` (the sequence kept and raised by
  a copy, the bound exactly at the save, the cost, and the set bounded over 300 answers), the page
  round trip in `rich_ledger`, and rtsim-jni's
  `a_durable_world_save_lets_the_camp_forget_the_answers_it_covers` (both host hooks). A fork
  (reported): the trigger is WORLD_SAVED, the signal `Replay` already uses, not the engine's own
  `ackedObsSeq`, which the IO lane writes after the pages are on disk and which would need the camp
  ledger touched from that thread.

- **MZB-178** (the merge with M3, group E): the conflict pages are RTC6 with meta version 5
  (MZB-165). M3's `Conflict::replaying` window (set by `session_start`, ended by `replay_done` from
  `resend_notices`, which rtsim-jni calls once the journal has caught up, MZB-141) is the one replay
  window; the lines rtsim-jni set and cleared around each record are gone, and it only sets
  `obs_seq`. A camp's census inside the window only marks its camp out (MZB-159). `CampRun::restart`
  ends the replay at once (`replay_done`), and `restart_to_replay` leaves it open for the camp tests
  that replay the last session's journal, whose `replay` helper closes it. Fixed in the merge:
  `party_lost` stays in the despawn's tail (the dead the stack less its survivors; M3's
  `recalled.clear()` there is gone, an entry being removed per epoch) and in `casualties`, which M3
  now counts off a recalled stack live (MZB-142). A death is counted once by whichever record finds
  it, since a casualty takes the member off the stack the despawn compares against:
  `a_member_lost_on_a_walk_off_books_once_in_the_camp` (a casualty and a despawn death on a raid's
  walk-off: two slots freed as died, none written off, the camp refilled to its cap), and it fails
  without either `party_lost`.

## M5 Camp reconciler, Java

- **MZB-200** (the plugin): `neoforge/mowziesmobs`, mod id `mic_rtsim_mowziesmobs`, built against Mowzie's
  Mobs 1.8.2 for 1.21.1 (`mowziesmobs-1.21.1-1.8.2.jar`) and GeckoLib 4.9.3, ordered AFTER `mic_rtsim` and
  `mowziesmobs`. Its GameTest server copies the pack's `config/mowziesmobs-common.toml` into
  `run-gametest/config` (task `packMowzieConfig`), so health multipliers and the Frostmaw's
  `healsOutOfBattle` are the pack's.
- **MZB-201** (holders before the census, the rule of MZB-174): a census first calls
  `RtsimSession.ensureReasserted` on every entity carrying `mic_rtsim:pending` in the footprint chunks' whole
  columns, and only then writes CAMP_CENSUS. The session keeps a weak identity set of the holders it has
  journaled (a load's reassert or a carry), so one holder object is journaled at most once a session and a
  holder that loads again, a new object, is journaled again. The loaded-entity drain at the start of
  BridgeEvents' post tick usually gets there first; the census's own call covers a holder the drain has not
  reached.
- **MZB-202** (TRADED): the census writes TRADED 0 and no trader list. The Umvuthi's trade memory is M9's.
- **MZB-203** (in and out from the tick's last levels): `ChunkTicketLevelUpdatedEvent` fires twice for one
  rise, so the event only stores the footprint chunk's level and queues the camp; the camp is looked at once,
  at the end of the tick. Out: a footprint chunk above FULL (33). Waiting: every chunk at FULL or better but
  some chunk's entities not in (`ServerLevel.areEntitiesLoaded`), looked at again each tick. In: all at FULL
  with entities in, after being out.
- **MZB-204** (census cap): a census lists at most 1,024 bodies (`Camps.CENSUS_CAP`); the rest wait for a
  later census.
- **MZB-205** (den footprint; replaced by MZB-230): was the Frostmaw piece's template box, grown 1 block in x
  and z and 2 in y. The review found the box misses a sleeping Frostmaw two times in three.
- **MZB-206** (FACTION_FOUND): the reconciler sends it at a camp's first census of each session, with the
  bodies at home (u16 at 52) and the census flags (u8 at 54: chief present, crystal held). Its block is the
  camp's position clamped into the start chunk, because the start chunk keys the faction
  (`Obs.villageId(dim, start.x, start.z)`). FLAG_LATE when the start chunk was not new.
- **MZB-207** (SET_OUT hand-off): a SET_OUT line waits for the end of the tick. A BAND_MATERIALIZE in the same
  hand-over whose stack is placed in that tick claims it (same agent, party epoch and body name): the puppet
  is made where the camp body stands, with the kind's `copy`, and the camp body goes, its record on a marker.
  The CAMP_RECONCILE must come before the BAND_MATERIALIZE in the hand-over, since a materialize places its
  stacks while it is applied.
- **MZB-208** (another camp's body): a census skips a body tagged by another camp that stands in its
  footprint. Its own camp counts it, as ELSEWHERE.
- **MZB-209** (M9's lines): SEAT_HEIR, and a LINE_CHIEF line on a kind whose `chiefLines()` is false (the
  grove), are answered STALE and change nothing.
- **MZB-210** (adoption keeps the jar's persistence): ADOPT sets the tag and the restriction only, never
  `setPersistenceRequired`. A body the reconciler makes (SPAWN, HAND_BACK) is persistence-required.
- **MZB-211** (homecoming): after a HAND_BACK line the rest of that party is recalled toward the camp
  (`ConflictBridge.homeAt`, RECALL_DONE) and goes as a recalled band does. The engine has ended the party,
  so its BAND_DEMATERIALIZED books nothing.
- **MZB-212** (pending records dropped by identity): on an acknowledgement a holder drops the records it
  carried when that record was journaled, by identity, so a record added later under a newer sequence stays.
  A record marker left with none is discarded.
- **MZB-213** (a hand-back with no puppet): played as a SPAWN line, held and placed like one.
- **MZB-214** (where a grove warrior spawns): 20 blocks from the centre on the side away from the nearest
  non-spectator player within 256 blocks; with none, 2 to 11 blocks out at a random angle, as the fire pit
  places its own. On the MOTION_BLOCKING_NO_LEAVES surface, at the block's centre.
- **MZB-215** (Peaceful; reworded by the review and again by the re-review, the rule is MZB-234): on Peaceful
  no new body appears, so a SPAWN is answered STALE and the next census orders it again. The first reason
  given here, that the engine would otherwise keep a body Peaceful despawns, and the sentence that
  `shouldDespawnInPeaceful()` is false for every Mowzie entity, were wrong; Peaceful is per kind of
  entity (from the re-review of d5b45b22, P3-5, not re-read in the jar by the builder). Bluff, Naga,
  Foliaath, ElokosaHowler, the umvuthana raptor and its followers discard themselves on Peaceful, and the
  Elokosa despawns. The Frostmaw deactivates and stays, and the umvuthana warrior only stops targeting. M5's
  two bodies are the last two, so the rule below (a SPAWN is STALE, a HAND_BACK proceeds, bodies standing
  stay) is right for them. **For M9:** a tagged raptor would discard itself on Peaceful and be booked
  BY_NONE; M9 decides Peaceful for raptors itself (MZB-594).
  Checked at M6 in the 1.8.2 jar (cfr): `shouldDespawnInPeaceful()` is `Mob`'s false for the Frostmaw, the
  grove's warriors and envoys and the Umvuthi (`MowzieEntity.checkDespawn`, `295`, asks it); only
  `EntityElokosa` overrides it, to true. The Frostmaw never calls `discard()` on Peaceful: it deactivates and
  cannot be woken (`EntityFrostmaw.java:582`, `600`, and `616` skips the wake animation on a steal). The
  raptor, its followers, the bluff, the naga, the foliaath and the elokosa howler and its followers discard
  themselves in `tick`.
- **MZB-216** (stand-ins): `conflict/frostmaw/frostmaw` is `mowziesmobs:frostmaw`; `conflict/umvuthana/warrior`
  and `conflict/umvuthana/envoy` are `mowziesmobs:umvuthana` (the grove warrior, `EntityUmvuthanaMinion`);
  `conflict/umvuthana/raptor` was `mowziesmobs:umvuthana_raptor` and is, until M9, `mowziesmobs:umvuthana`
  too (MZB-594).
- **MZB-217** (den spot): the template box's floor centre, ((minX + maxX + 1) / 2, minY, (minZ + maxZ + 1) / 2).
  The template stands its Frostmaw at (3, 0, 3.5) of the unrotated [6, 5, 7] template, so under every
  rotation it is within about a block of the spot. A home Frostmaw is restricted round the spot's block.
- **MZB-218** (grove centre; its Y is replaced by MZB-242): the fire pit piece's (minX + 4, maxZ - 4), the Y
  of MZB-242 and no longer the height map. That is the jar's `findGround(4, 4)` under
  TemplateStructurePiece's NORTH orientation, the block its warriors are restricted to. The footprint is the
  25-block circle round it: every chunk whose nearest point is within 25 blocks. A grove start with no fire
  pit piece is no grove.
- **MZB-219** (made as the jar makes them): `make` calls `finalizeSpawn(STRUCTURE)` directly, as the jar's
  worldgen does, so no FinalizeSpawnEvent fires. The den's Frostmaw then gets the line's crystal and is put
  to sleep (`setActive(false)`).
- **MZB-220** (the Umvuthi is no census body): `GroveKind.isBody` is the exact type `mowziesmobs:umvuthana`.
  The Umvuthi is reported only in the census's chief fields (CHIEF, THRONE, health, position, direction).
  Followers, cranes and raptors share the warrior's base class and are not counted.
- **MZB-221** (a discard is a death): a tagged body removed as DISCARDED by anything but the reconciler
  (which takes the tag off first) is a CAMP_DEATH with killer kind none (BY_NONE).
- **MZB-222** (an unclaimed SET_OUT is a take): the body goes, its record on a marker, answered DONE; the
  party gets fresh puppets when it is seen later. With no body: NOTHING. Camp out or a newer epoch: STALE.
- **MZB-223** (restrictions): a home Frostmaw is restricted 8 blocks round its spot (PLACEHOLDER), set on every
  join, because vanilla does not save a mob's restriction and the jar never restricts a Frostmaw. A grove
  warrior is restricted 25 round the centre on join and on adoption (the jar's own also save theirs as
  `HomePos`). The den's footprint is the restriction plus 3 (MZB-230), so a Frostmaw kept to its restriction
  rests inside it.
- **MZB-224** (held spawns): a Frostmaw's SPAWN waits while a non-spectator player is within 160 blocks of the
  spot (its tracking range), for at most 6,000 ticks (`Camps.HOLD_MAX`, PLACEHOLDER). Any SPAWN also waits for
  the session's spawn budget. A waiting line is answered STALE when its camp goes out or recounts.
- **MZB-225** (the entity-chunk barrier; C2ME answered by the review round): the barrier takes the `IOWorker`
  out of `EntityStorage`'s `SimpleRegionStorage` and picks a strategy by that worker's class
  (`ChunkWriteBarrier.entityWritesFor`): exactly `IOWorker` is vanilla's (foreground writes ahead of the
  queue, JB2), a C2ME worker goes to `C2meWrites` (poll the write cache, then fsync the region file, JB3), any
  other `IOWorker` subclass waits for `synchronize(true)` (warned), and a storage that is not an
  `EntityStorage` is not waited for (warned once; its camp records ride without the barrier).
  **How C2ME stores entity chunks** (read from the pack's jar, `c2me-neoforge-mc1.21.1-0.4.0-alpha.0.122`,
  module `c2me-rewrites-chunkio`): `MixinChunkPosKeyedStorage` targets `SimpleRegionStorage` and builds a
  `C2MEStorageVanillaInterface` (an `IOWorker` subclass over one `C2MEStorageThread`) as the worker of every
  `SimpleRegionStorage`. `EntityStorage` holds one, so entity chunks use the same storage thread, write cache
  and asynchronous stores as block chunks: a store goes into the thread's `cache` and is compressed on C2ME's
  pool, and the write's last step removes the position from `cache` and writes the region file in one task.
  So the barrier's entity path reaches C2ME's worker through the accessor chain it already uses and
  `C2meWrites.of` covers it exactly as it covers the chunk path; nothing is missing. If a later C2ME changes
  those fields, `C2meWrites.of` returns null, the barrier warns and falls back to `synchronize(true)`, which is
  right but slow (a full flush at each autosave). What would fix a miss: nothing new in the barrier; the
  reflection in `C2meWrites` gets the new field names. Not run against a real C2ME world: the test
  `theEntityStorageGetsTheStrategyOfItsOwnWorker` asserts the strategy and `-Pc2me` runs it with the pack's
  jar; it is for the round after the re-review.
- **MZB-226** (the finder's queue; replaced by MZB-237): a chunk holding structure starts is queued when it
  loads, at most 4,096 (PLACEHOLDER), and drained at the end of the server tick once a session is live; a
  dropped chunk's camp is found when it next loads. A camp is declared once a server run and kept across
  sessions; at a session start every camp is out and looked at again. The queue held the `LevelChunk`.
- **MZB-227** (test camps): GameTests declare camps under structure names no culture claims
  (`mic_rtsim_mowziesmobs:test_den`, `mic_rtsim_mowziesmobs:test_grove`), through
  `MowzieCamps.declareDen/declareGrove(level, start, late, structure)` or `RtsimApi.declareCamp`. The engine
  (M3 and M4 alike) ignores FACTION_FOUND for a structure no culture claims, so a test camp gets only the
  test's scripted CAMP_RECONCILE lines, on the stub or the native engine.
- **MZB-228** (test 3's reset half, a gap; searched again in the review round): a GameTest cannot restart the
  session (`Camps.sessionStarted` is package-private and the session belongs to the server), and no test
  of the three modules reopens one: the GameTest server has one bridge session for its whole run, and the
  Rust engine's restart half is rtsim-jni's (M1, M4). Test 3 therefore checks the recount by taking the
  grove out and in (`aRemoveByUuidTakesAStrayAndAnElsewhereBodyIsCountedApart` does the same for a body
  that returns from disk). The reset itself (every camp out, `reported` clear, held lines dropped, the
  index emptied at a session start) has no GameTest, and no test-only hook was added to production code
  for it. The gap stays open until a GameTest can stop and start the session.
- **MZB-229** (Carry On): picking up a body a camp holds is a CAMP_DEATH with killer kind taken (BY_TAKEN)
  and that player; the tag goes before Carry On saves the entity, so it is put down a stray.
- **MZB-230** (a den's footprint is a cylinder; replaces MZB-205, qualifies MZB-223): the cylinder round the
  den's spot of radius `DenKind.FOOTPRINT_RADIUS` = `RESTRICT` + `FOOTPRINT_SLACK` = 8 + 3 = 11, and of
  half-height 11 (the same). The census lists the bodies inside it. The footprint's chunk set (levels,
  attention, the barrier) is every chunk the cylinder's disc touches: 5 for a spot in the middle of its
  chunk (its own and the four neighbours), more near a corner. The grove keeps its own circle of 25. Why: an
  awake Frostmaw with no target strolls inside its restriction (`WaterAvoidingRandomStrollGoal`, priority 7)
  for up to 1,200 ticks, then deactivates where it stands (`EntityFrostmaw.java:606-616`) and stays put
  asleep; against a template box grown by 1 it came to rest outside two times in three. The slack is a
  constant derived from the restriction constant so the two cannot drift apart. The GameTests force the
  3 by 3 chunks round a den, a superset of the 5.
- **MZB-231** (the index keeps its bodies through a hidden chunk; its premise corrected by MZB-590):
  `onLeave` keeps a tagged body's index entry when the removal reason is null (the entity is still alive, its
  chunk only hidden) and drops it on a real removal (discarded, changed dimension). The first wording said
  the entry was dropped when the body unloaded with its chunk; that was false: vanilla posts the leave once,
  when the chunk goes HIDDEN, with a null reason, and the unload that follows (`setRemoved(UNLOADED_TO_CHUNK)`
  in `PersistentEntitySectionManager`) posts no leave at all, so the entry stays as a removed object (MZB-590); the census adds every tagged body of its faction found inside, idempotently;
  iterating `tagged()` prunes removed entries. Why: a footprint chunk can go TRACKED, HIDDEN and TRACKED
  within one tick, and the second promotion posts no join. A TAKE by tag after such a flicker is DONE.
- **MZB-232** (the chief reads the index; strays within 64 blocks): `DenKind.chief` takes the footprint's
  Frostmaws and the faction's tagged bodies loaded elsewhere, so a Frostmaw that is the den's but stands
  outside the cylinder (indexed, ELSEWHERE) still answers as the chief, with CHIEF, CRYSTAL and its health.
  A late den (found in a world from before the plugin) looks, at its first census of the session, for
  untagged Frostmaws of the exact type within `DenKind.STRAY_RADIUS` = 64 blocks of the spot among the loaded
  entities, nearest first, as many as the den has free slots (`Camps.wideStrays`). Each is counted home and
  listed in the census as a stray, and the engine's ADOPT tags and restricts it, so no SPAWN follows. An
  untagged Frostmaw that joins a level later within 64 blocks of a den with a free slot makes that den look
  again at its next census (`strayWatch`; a den that is in recounts), and is listed the same way; a den
  with no free slot leaves it alone, since it is the jar's and outside the books. The 64 is the lead's
  figure (PLACEHOLDER). **The remaining gap, recorded and not closed:** only a Frostmaw beyond 64 blocks of
  a late den is not found, so the engine can SPAWN a second Frostmaw for that den. One that is not loaded at
  the den's first census is no gap within 64 blocks: `strayJoined` catches it when it loads later (the den
  looks again at its next census), as long as the den has a free slot. (Corrected from "or one not loaded at
  the den's first census", which the stray watch already covered.)
- **MZB-233** (a line is played only on a camp that is playable now): the session's order stays
  (`RtsimSession.endOfTick`, then the hand-over's commands, then `Camps.tick`), since other modules share
  it. `reconcile`, `resolveSetOuts` and `playHeld` each first check that every footprint chunk is still at
  a FULL level or better and that `areEntitiesLoaded` holds for it; if not, the camp is treated as out and
  the line is STALE, as a line for an out camp is, so nothing is made, taken or set out in the tick a
  footprint went down. The test queues its line in the tick after the release, because a step's scripted
  command is applied in that tick's `ServerTickEvent.Post`.
- **MZB-234** (the Peaceful rule; rewords MZB-215): on Peaceful no new body appears, so a SPAWN is STALE
  and makes no body; a HAND_BACK proceeds, since the body is the one that returns (it is added, then its
  puppet goes); bodies already standing stay. The "as the jar's do" this entry gave was too general (see
  MZB-215): it holds for the Frostmaw and the grove warrior, M5's two bodies, which stay on Peaceful; a body
  of a kind that discards itself on Peaceful (the raptor, M9) is not covered, and M9 decides it. Both halves
  are tested.
- **MZB-235** (a holder the barrier cannot find; a departure from the lead's "skipped"; reworded by MZB-597):
  at the arm, a holder noted by UUID that `level.getEntity` does not return is noted all the same, at the
  chunk the entity object last stood in while a weak reference to it is alive (`holdersRemembered`), and only
  when that reference is gone at the chunk it was noted in at commit time (`holdersUnresolved`). The lead's
  text skipped it as unloaded and so already written; noting it is the conservative option, since a holder in
  a chunk that is only being unloaded may not be written yet, and one more noted chunk costs one more write.
  The first version always fell back to the commit-time chunk, which a body that had walked on before its
  chunk was hidden had already left (MZB-597).
- **MZB-236** (a hand-back adds the body first): `Camps.handBack` makes the body, `copy`s the puppet's state
  into it and adds it; only after the add succeeds does `bridge.handBack` despawn the puppet. One tick with
  both present is acceptable. If the add fails (another mod cancels the join), the puppet stays, nothing is
  taken off the party, no recall starts, and the line is STALE. The body already home gives NOTHING and the
  puppet only goes.
- **MZB-237** (the finder's queue holds small records; replaces MZB-226): a loaded chunk with a den or
  grove start is queued as a record of the server (weak), the dimension, the late flag and, per start, the
  start chunk and a copy of the one bounding box the finder reads (the den's template piece, the grove's fire
  pit). No chunk or structure start is held. The bound stays 4,096 (PLACEHOLDER); a record past it is dropped
  and counted (`MowzieCamps.dropped`), and its camp is found at its chunk's next load, with the late flag
  taken from that load (not a new chunk).
- **MZB-238** (the barrier notes a camp record's holder by UUID): `RtsimSession.carryOnCampHolder` notes the
  holder's level and UUID, not its chunk; when the barrier arms, each UUID is resolved to its entity's
  current chunk and that chunk's entity storage is the one ordered (MZB-235 for a holder not found).
  Counters: `holdersResolved`, `holdersMoved` (it was in another chunk than at commit), `holdersRemembered`
  and `holdersUnresolved` (MZB-597).
  Why: a holder that walks into the next chunk between the answer and the save carries the record with it.
- **MZB-239** (reassert before the record is written): `Camps.answer` calls `ensureReasserted(holder)` before
  `bodiesRecord` commits the new CAMP_BODIES, so what the holder carried from disk reaches the engine under
  an earlier sequence. `carry` keeps its own call (idempotent) for its other callers.
- **MZB-240** (a body that changes dimension leaves its faction's index): the leave posted with the removal
  reason CHANGED_DIMENSION drops the entry, and a join in another dimension than the camp's is not
  indexed. A known quirk, not fixed: the copy in the other dimension keeps its camp tag, and discarding it
  still books a CAMP_DEATH for the camp, so the test strips the tag first.
- **MZB-241** (`dropPendingWhenAcked` sees only visible entities): it finds a holder with
  `level.getEntity`, so one that is hidden when the acknowledgement comes keeps its records until its next
  reassert, which drops them at the next acknowledgement. Harmless: the engine treats a repeated record as a
  no-op by op id. Noted as a comment in `RtsimSession`.
- **MZB-242** (the grove centre's Y comes from the structure start): `MowzieCamps.groveCentre` is the fire pit
  box's (minX + 4, minY, maxZ - 4); the height map is never read, so the Y depends neither on what is loaded
  nor on what players build. The jar's own warriors are restricted to the surface block there, which
  differs only by the ground's rise over the pit; a restriction is a sphere of 25. Replaces the Y of
  MZB-218.
- **MZB-243** (a grove spawn tries 20 places): `GroveKind.spawnAt` tries `SPAWN_TRIES` = 20 places as the
  jar's fire pit does, the first at the edge away from the nearest player and the rest jittered round it
  (or, with no player in 256 blocks, at 2 to 11 blocks out); each must be loaded ground, free of collisions
  (`level.noCollision` on the warrior's box) and of fluid. If none passes, the line is STALE.
- **MZB-244** (joins cancelled by another mod): a SPAWN, ADOPT or HAND_BACK whose body another mod cancels at
  its join (In Control and the like) is answered STALE. The next census shows the slot empty and the engine
  orders it again, so the line repeats at every census while the other mod keeps cancelling. Not fixed:
  nothing here can know the other mod's rule; `staleLines` shows it.
- **MZB-245** (an adopted Frostmaw without a crystal can despawn): the review found that an adopted Frostmaw
  with no crystal can despawn at random, and ADOPT never sets persistence (MZB-210). Its removal is booked as
  a CAMP_DEATH with no killer (BY_NONE), next to MZB-221, which also counts despawns and conversions as
  deaths. The engine then refills the slot. Not fixed here.
- **MZB-246** (rtsim not installed): without rtsim, every load of a body that carries the camp tag logs a
  NeoForge error about the unknown attachment. Harmless for UR-1 (uninstall without harm): it is a log line
  and the body is the jar's own.
- **MZB-247** (how the GameTests are built): one batch per test, since `Camps`'s counters are global; each
  test its own far base (a camp stays declared until the server stops). A den is forced as 3 by 3 chunks, a
  grove as 5 by 5. A fake player (`FakePlayers.add`) counts for `Camps.playerNear` and holds no ticket. Test
  10 (MZB-174) releases the den fully instead of the recipe's level-35 trick (force a chunk 4 away, release
  the den's), a fork taken at the implementer's judgment: the trick keeps the chunks in memory, so the
  entities leave with no storage round trip, while a full release sends the holders through the real entity
  storage, which is what the barrier orders and what a restart does, at the cost of waiting for it.
  Test 12 moves its body 80 blocks, not 60: the chunk 4 east of the grove's centre is held at 33 by the
  grove's own forced column, so releasing it would not unload the body; the chunk 5 east is at 34. Test 6 asserts that the camp
  was seen waiting for its entities, which can flake if the entity load completes inside one tick; the
  assertion that no census came before every footprint chunk had its entities is the one that matters.
  The Carry On test uses a stub camp kind (the Carry On module does not depend on the plugin) and posts
  Carry On's `EntityPickupEvent`, since its `PickupHandler` needs a real player's input; what Carry On does
  before posting it is not covered.
- **MZB-248** (not tested): the reassert-before-record order of `Camps.answer` (MZB-239), which test 10
  covers only through the census order; the session restart (MZB-228); and the poll on a real C2ME (MZB-249,
  MZB-290): the GameTest server runs without C2ME unless `-Pc2me` (MZB-292), and the poll is tested alone
  by silencing the event. (The grove spawn retry is tested, MZB-291.)
- **MZB-249** (C2ME never posts `ChunkTicketLevelUpdatedEvent`; found in the review round, answered by
  MZB-290): read in the pack's c2me-neoforge-mc1.21.1-0.4.0-alpha.0.122. Its `rewrites-chunk-system` module
  has `com.ishland.c2me.rewrites.chunksystem.mixin.MixinThreadedAnvilChunkStorage`, which `@Overwrite`s
  `ChunkMap.updateChunkScheduling(long, int, ChunkHolder, int)` with a call into
  `TheChunkSystem.vanillaIf$setLevel(long, int)`, and NeoForge posts the event from inside the vanilla
  method, so under C2ME it is never posted. `Camps.onTicket` was blind there: the levels read at `declare`
  were never updated, so after it a camp neither went out when its chunks unloaded nor came in when they
  loaded. The same class `@Overwrite`s `getUpdatingChunkIfPresent(long)` (the holder from C2ME's
  `TheChunkSystem.getHolder`, its user data, null when the holder is not open), and the holder it returns is
  C2ME's `NewChunkHolderVanillaInterface`, a `ChunkHolder` subclass whose `getTicketLevel()` is overridden to
  `newHolder.getTargetStatus().toVanillaLevel()`. So reading a level through `ChunkMapInvoker` works on both
  chunk systems (C2ME's value is its target status, which leads the loaded state; `areEntitiesLoaded` still
  gates the census). Options were: (1) a poll of each camp's footprint levels; (2) follow `ChunkEvent.Load`
  and `Unload` (C2ME is expected to post them; not verified; no FULL threshold); (3) a mixin on C2ME's
  `vanillaIf$setLevel`, rejected: C2ME is an alpha and a mixin on it fails the launch on any update (the
  reason JB3 reads it by reflection). The lead chose (1).
- **MZB-290** (the level poll is the authority; the event is a hint): `Camps.tick` reads the footprint
  levels of one twentieth of the camps each tick (`POLL_PERIOD` = 20; a camp's slot is its declaration order
  mod 20, kept as 20 lists so a tick touches only its own slot) through `ChunkMapInvoker.getUpdatingChunkIfPresent`,
  null being `NO_HOLDER`, and a camp whose levels moved is looked at at the end of that tick, which is the
  same `look` the event feeds. A camp costs 5 to 21 lookups every 20 ticks. The event listener stays as an
  early hint where it fires (vanilla, the GameTests). The first wording said levels from both were clamped
  to `NO_HOLDER` so that the two agree at an unload; they did not: the event posts 45 (`UNLOADED_LEVEL`,
  MAX_LEVEL + 1) and the poll read a missing holder as 46, so the poll would have reported the unload
  as a change after the event had already moved the camp, and C2ME's NEW status is 45 as well. Now both read
  through `Camps.norm`, every level above `ChunkLevel.MAX_LEVEL` being `NO_HOLDER` (MZB-596). The poll is
  correct with the listener off. Two more consequences: an out camp is noticed at most 20 ticks late
  under C2ME, and a line is not played on a stale level: `playable` (MZB-233) reads the levels afresh from the
  chunk map before it decides, since a line is played at once and cannot wait for the next poll (a camp it
  finds moved is also looked at at the end of the tick). Counters `polls` (camps polled) and `pollChanges`
  (footprint chunks whose level the poll or that check found moved) are public for the tests. The test hook
  is `Camps.ticketEventsSilentUntil`, a game time before which `onTicket` returns at once; a time, not a
  switch, so a failed test leaves the event off for a bounded while, and `Camps.clear` resets it. It is
  production code a test needs, asked for by the lead. Test:
  `aCampFollowsItsChunksThroughThePollWhenTheTicketEventIsSilent` (batch `mowzie_poll`): the event silenced, a
  den's chunks released, CAMP_OUT within `POLL_PERIOD + 3` ticks with `pollChanges` moved (it would not
  move if the event had updated the levels first), the chunks unloaded and forced again, census 2 within
  `POLL_PERIOD + 3` ticks of the chunks being ready, a new epoch, one CAMP_OUT in all.
- **MZB-291** (the grove spawn retry is tested): `aGroveWithNoFreePlaceAnswersASpawnStaleAndMakesNoBody`
  (batch `mowzie_no_room`) puts a persistent leaf block at the standing height over every column within 24
  blocks of the grove's centre: leaves are no part of `MOTION_BLOCKING_NO_LEAVES`, which `spawnAt` reads, so
  each of the 20 places is a spot inside a leaf block and the collision check rejects it (a solid block would
  raise the height map and leave the place free above it). The kind returns no place, and a SPAWN is STALE
  with no spawn counted, no body in slot 1 and no warrior in the area, with no player near (the 2 to 11 ring)
  and with one 60 blocks east (the edge away from it). With the leaves cleared the same line is DONE and the
  warrior stands in the open. Replaces the "not tested" of the spawn retry in MZB-248.
- **MZB-292** (C2ME in the GameTest servers): core's GameTest server has loaded the pack's C2ME jar under
  `-Pc2me` since JB3; `mowziesmobs/build.gradle` now does the same (`runtimeOnly files("${packMods}/${c2me_jar}")`
  under `-Pc2me`), so `./gametest.sh mowziesmobs -Pc2me` runs the Mowzie suite on C2ME's chunk system and
  `./gametest.sh core -Pc2me` the journal barrier's (its `theEntityStorageGetsTheStrategyOfItsOwnWorker` pins
  the C2ME entity-storage path). Not run: whether Mowzie's Mobs and C2ME start together on the GameTest
  server, and whether every test of the suite holds under the poll's up to 20 ticks of delay (tests that
  wait on a CAMP_OUT or a census do; a test that assumes the camp goes out in the tick after a release
  does not), are for the first run to show. The other plugins' projects have no such line.

- **MZB-590** (the index is pruned of removed entities; from the re-review of d5b45b22, P1-A): a body whose
  chunk unloads stayed in its faction's identity set as a removed object, so those objects piled up (a D1
  leak) and `declare` called `kind.joined` on them. Now every reader of the index skips and drops removed
  entries (`Camps.live(faction)`: `declare`, `freeSlots`, the census's loop over the camp's bodies, and
  `tagged()` as before), `poll` prunes the polled camp's set (so a camp's set is clean within one poll
  period of an unload), and `indexAdd` prunes the faction's set when it adds, which covers a faction no camp
  declared. `Camps.held(faction)`, the GameTests' view, stays raw so the test can see what the poll pruned.
  Test: `anUnloadedBodyLeavesTheIndexAtTheNextPollAndALoadIndexesItAgain` (batch `mowzie_index_unload`): an
  adopted Frostmaw, the den released and unloaded, `POLL_PERIOD + 3` ticks, the raw index holds no removed
  entity; forced again, it holds the Frostmaw the load made (another object, the same UUID), tagged. Which
  tick the poll prunes in is not asserted (it is 1 in 20 that it prunes in the removal tick itself); the
  log line records how many ticks the removed body stayed.

- **MZB-591** (three tests had no player in view of the band; re-review P2-A): a DONE HAND_BACK starts the
  party's recall, and `ConflictBridge.recallStep` despawns at once every placed puppet with no
  non-spectator player within `OUT_OF_VIEW` (64) blocks of a living member. The homecoming, handback_cancelled
  and peaceful tests had no player, so the band's other puppet was gone by the time they looked at it. Each now
  adds a `FakePlayers` watcher 50 blocks east of the grove's centre (`MowzieGameTests.watcher`: inside the 64,
  outside a warrior's sight, no ticket, removed when the test ends), before the band is made.

- **MZB-592** (play_time is triggered on the holder's level; re-review P2-B): the test waited until the
  camp's cached footprint level was above FULL, which under C2ME moves only in the poll inside `Camps.tick`,
  where `look` takes the camp out in that same tick, so the camp was no longer "in" when the line landed.
  The step now waits on the footprint holders' own levels, read through `ChunkMapInvoker.mic_rtsim$updating`
  (a null holder is `NO_HOLDER`). The line then lands before `Camps.tick`, and `playable` reads the levels
  afresh (MZB-290), so it is STALE under both chunk systems. A residual race remains under C2ME: C2ME's
  holder level can change on its own threads, and a change that lands in the middle of a tick's Post, after
  the step and before the poll, still lets the poll take the camp out first (a small window; the round will
  show whether it ever bites).
- **MZB-593** (cold's "seen waiting" is asserted only with the event live; re-review P2-C): the camp is seen
  waiting for its entities when its levels move ahead of them, which the ticket event does (the level rises
  in the level tick, the entities arrive later); under C2ME the poll reads levels that already carry the load,
  so the camp may never be seen waiting. `MowzieKit.watchTicketEvents` counts the
  `ChunkTicketLevelUpdatedEvent`s of the den's chunks from the release on; `sawWaiting` is asserted only when
  the count is above zero, else the test records that it was not. "No census before every footprint chunk had
  its entities" is asserted always.

- **MZB-594** (the raptor stand-in is the grove warrior until M9; re-review P2-D; owner M9): the stand-in
  for `conflict/umvuthana/raptor` was `mowziesmobs:umvuthana_raptor`, and `ConflictBodies.create` calls
  `finalizeSpawn(EVENT)`, whose raptor adds 4 or 5 `EntityUmvuthanaFollowerToRaptor` through
  `addFreshEntity`. Every umvuthana party that materialized with a raptor unit would have brought untagged
  hostile followers the engine does not know. Until M9 the raptor maps to `mowziesmobs:umvuthana`, a
  placeholder. **M9 owns the raptor maker**: a raptor with exactly 4 followers, each tagged as the party's
  member (or its followers made by the party as bodies of their own). Test:
  `anUmvuthanaPartyWithARaptorUnitBringsNoUntaggedFollowers` (batch `mowzie_raptor_standin`): two raptor units
  embodied, both the grove warrior, no `EntityUmvuthanaRaptor` and no `EntityUmvuthanaFollowerToRaptor` in the
  area after 40 ticks.

- **MZB-595** (a puppet is no stray, and only the stray-watching camps are walked; re-review P3-1):
  `strayJoined` ran for every join of a Mob, including the engine's own puppets, and walked every camp.
  A SET_OUT puppet of the den's Frostmaw joining beside the den, the chief's body handed to a puppet, would
  have made the den look again. Now a tagged entity (`RtsimAttachments.isTagged`: a puppet carries the
  bridge's agent tag, a body the camp tag) returns first, and `Camps` keeps, per dimension, the camps whose
  kind has a stray radius above zero (`strayers`, filled in `declare`, cleared in `clear`), so a join walks
  only those. Test: `aSetOutPuppetOfTheDensFrostmawJoiningNearTheDenCausesNoRecount` (batch
  `mowzie_puppet_stray`): a SET_OUT of an adopted Frostmaw's slot with a one-unit band materialized in the
  same hand-over, DONE, the puppet standing, and 40 ticks later the stray watch has not moved, no second
  census and no new epoch.
- **MZB-596** (the event and the poll agree at an unload; re-review P3-2; corrects MZB-290): the event
  posts `UNLOADED_LEVEL` (45) when the last ticket goes, the poll read a chunk with no holder as
  `NO_HOLDER` (46), and C2ME's holder reads 45 for a chunk of status NEW. `Camps.norm` now maps every level
  above `ChunkLevel.MAX_LEVEL` to `NO_HOLDER` in `declare`, `onTicket` and `refreshLevels`, so a chunk that
  is gone is one value however it is reached and the poll reports only real corrections. Test:
  `withTheTicketEventLiveAnUnloadLeavesThePollNothingToCorrect` (batch `mowzie_poll_agrees`): a den counted
  and released, CAMP_OUT, then the chunks unloaded and a poll period passed: the poll has run on the den
  (`polls` moved), `pollChanges` has not (only where the ticket event fired for the den's chunks, since
  without it the poll is the one that finds the change, and the test records that), and every footprint level
  reads `NO_HOLDER`. With the silent-event test (MZB-290) the two sides of the assertion discriminate:
  event off, the poll moves `pollChanges`; event on, it does not.

- **MZB-597** (the barrier notes a holder it cannot find where the object last stood; re-review P3-3;
  rewords MZB-235): `ChunkWriteBarrier.dependsOnHolder` keeps, with the commit-time chunk, a
  `WeakReference<Entity>` for each holder. At the arm a holder `level.getEntity` finds is noted where it stands
  (`holdersResolved`); one it does not find (its chunk hidden, or the entity removed by the unload, which leaves
  the Entity object with the position it had) is noted at the chunk of the object's own `chunkPosition()`
  (`holdersRemembered`); the commit-time chunk is the fallback only when the reference is cleared
  (`holdersUnresolved`). `holdersMoved` counts a holder noted in a chunk other than its commit-time one by
  either path. The reference is weak, so it keeps no entity alive and no chunk loaded (D1). Why the earlier chunk
  need not be noted (the lead's reading): a holder that leaves its record's chunk and is then unloaded is
  saved with the chunk it unloads from, and its record lives on the entity, so the chunk it stood in at the
  commit does not matter; a holder that is killed books through its death (CAMP_DEATH), not through the
  record. A public
  `forgetHolderReferences` clears every reference, for the GameTests only (the state a collected entity
  leaves, which a test cannot reach by waiting for the collector). Tests (core): the existing
  `aRecordsHolderThatWalksIntoTheNextChunkIsNotedWhereItStandsAtTheSave` now clears B's reference after
  discarding it (found 2, moved 1, remembered 0, lost 1) and the new `aHolderRemovedAfterWalkingIsNotedWhereItWasLastSeen`
  (batch `mic_rtsim_journal_barrier_remembered`) walks a holder into the second chunk and removes it
  (found 0, moved 1, remembered 1, lost 0; the second chunk noted, the first not).

- **MZB-598** (an adopted wide stray stays outside the footprint; a known gap; re-review P3-4; no code):
  a stray found wide (MZB-232, beyond the cylinder, within 64 blocks) is adopted where it stands: it is
  tagged and restricted to the spot but not moved, and it is the holder of the CAMP_BODIES record that
  answers its ADOPT (it is the body there). So it stays outside the footprint, counted by later censuses only
  through the index (BODY_ELSEWHERE) while it is loaded. A census journals first the records of the holders
  in the footprint's columns (MZB-201) and not of one outside them. Once a crash has happened before that
  record is acknowledged, the record reasserts when the stray's chunk loads, which can be after the next
  session's census, so the engine can see the census before it sees the old ADOPT's record. Chosen: not
  closed. It needs a crash inside the window between an ADOPT and its acknowledgement, with the adopted
  stray left outside the footprint, and the effect is a repeated or late record the engine treats by op id and
  epoch (the builder did not trace the engine's side again; the lead's re-review named the gap). If it
  matters, the fix is for a census to also journal the holders of the camp's indexed bodies that stand
  elsewhere, which is one loop over `live(faction)` before the census.
- **MZB-599** (the minor items of the re-review; P3-6):
  - `playHeld` checks Peaceful before it spends the line's budget (MZB-234), so a line held on a camp that
    cannot make a body no longer burns its attempts.
  - `DenKind.chief` excludes a Frostmaw tagged by another den (it reads the faction's index and the
    footprint's Frostmaws, and a Frostmaw that walked in from another den is that den's chief, MZB-208's rule
    for the census extended to the chief).
  - `MowzieCamps.dropped` is an `AtomicInteger` (`dropped.get()` in the GameTest): `onChunkLoad` increments it
    where the queue beside it, already concurrent, is filled, and a chunk load is not promised to be on the
    server thread under every chunk system; the other counters are the drain's, on the server thread.
  - The first GameTest round (`target/m5-gt1.log`) crashed the Mowzie server in the
    fifth test: `aGroveIsCountedAndAResetRecountsTheWarriorsStandingInIt` read `Grove.area()` before the grove
    was declared, and `area()` read the not yet set `spot` (a NullPointerException). Vanilla's sequence catches
    only `GameTestAssertException` from a step, so any other exception reaches the tick loop and takes down
    the whole GameTest server, and 21 tests went unrun. Two fixes in the kit: `Grove.area()` reads the chunk
    centre (the same point the spot is declared at) and no longer needs `spot`; and `MowzieKit.seq(helper)`
    replaces `helper.startSequence()` in every Mowzie test, guarding each step so that an exception that is no
    assertion is logged (its stack once per site) and fails that test as `unexpected <exception>`, the way a
    wait's last assertion is reported when it times out. A GameTest-only helper in the plugin (D4); the other
    projects' tests are untouched.
  - The second round (`target/m5-gt1-mowzie.log`) ran all 27 Mowzie tests: 20 passed and 7 failed, every one
    read as a fault of the test and none of the product code (the Nether and the reload cases are read from the
    code and the log, and the next round confirms them). (1) The fire pit test: the jar's `FirepitPiece.postProcess`
    makes no warrior on ground at y 0 or below (`bPos.getY() <= 0`) and the GameTest world's ground is at y -61,
    so the real piece made none; the test lays its floor at y 63 (`MowzieKit.platform` and `Grove.lay` take a
    minimum height). (2) and (6) Two tests waited for a record marker or a holder's pending records to be
    acknowledged, which only a saved engine page does, and a GameTest server never reaches the periodic save:
    `MowzieKit.acknowledge()` asks the engine to save at its next step, as core's pending-record tests do.
    (3) and (6) A chunk released and forced again was taken to have been reloaded from disk once its
    entity was gone from `level.getEntity`, but a chunk whose ticket dropped is only hidden then, and a hidden
    chunk forced again is promoted with no join: the tests now wait for the chunk itself to be unloaded and the
    old object removed (`MowzieKit.unloadedWithout`). (4) The Nether chunk's load runs on other threads and the
    wait was not an `io` wait, so on a busy machine it timed out in about two seconds of real time; it is an
    `io` wait now and names what it saw. (5) The check that the Frostmaw "stands" after a line went stale in
    the tick its footprint went down read the entity after the chunks had unloaded; it now checks that the
    Frostmaw was not taken (not discarded) and is still tagged. (7) `Den.at` and `Grove.at` read `spot` before
    the camp was declared; they use the chunk's middle until then.
  - The third round (`target/m5-gt2-mowzie.log`): 24 of 27 passed. Two failures were `Far.ready()` timing out on a
    chunk's load with its entities (the load flake of the other projects' rounds; the message now says which of
    chunk, entities and ticking was missing), and the hold-max test, whose 7,000 ticks left 1,000 for the
    setup around a 6,000 tick hold (`Camps.HOLD_MAX`): its timeout is 9,000.
  - A body hidden and tracked again (read from NeoForge 21.1.249's patched `ServerLevel.EntityCallbacks`:
    `onTrackingEnd` posts `EntityLeaveLevelEvent` with no removal reason, `onTrackingStart` posts no
    `EntityJoinLevelEvent`; only `addEntity` does, on a load or a spawn) stays in the books: `Camps.onLeave`
    returns at once for a body with no removal reason, so it keeps its index entry and is never written off; a
    footprint body's chunk going down is CAMP_OUT and coming back is a census that lists it again, and an
    elsewhere body is listed by every census as long as its object is alive; one that then unloads leaves the
    index at the next prune and comes back with a join from disk and a recount (the one gap is a record
    carrier hidden when its acknowledgement comes, which keeps its records until the next session, MZB-241).
  - The fourth round (`target/m5-gt4.log`, Carry On's first run): 3 of 4 passed. The camp pickup test failed
    at its first census (it listed no body, expected the one stray): the test's vindicator stands in a far
    chunk with no player within 128 blocks, and a non-persistent monster there is discarded by
    `checkDespawn` on its first tick, before the census. A fault of the test: it now calls
    `setPersistenceRequired()` on the stray (`Camps` sets it on every body it makes; an adopted stray keeps the
    jar's own persistence, MZB-210, so a real stray far from every player is the jar's concern). No product change.
  - Not tested: the Peaceful check ordering in `playHeld` (the Peaceful tests answer a SPAWN that is not held)
    and the chief's exclusion (it needs a second den's Frostmaw standing in the first's footprint).
  - Under C2ME the poll sees a camp's levels at most `POLL_PERIOD` (20) ticks late (MZB-290), so an unload
    and a reload of the same chunks inside one period is never seen: the levels read the same before and
    after, no CAMP_OUT and no new epoch. This is benign: the bodies the unload hid or removed are indexed
    again by their own join events (MZB-590), a body that left with its chunk comes back with it, and the
    census that would have followed has nothing a later one does not carry. Not tested (a test would race the
    period).

- **MZB-650** (main's a95bd2a2 merged into M5: the merge and the formats):
  - One conflict, `RtsimSession`, both sides kept: this branch's `asserted` set and `ensureReasserted` /
    `reasserted(holder)` beside main's `stalePending` and chunk `reassert`. The one change main's `PendingRecords`
    `{life, records}` forced elsewhere: `dropPendingWhenAcked` keeps what is left with `p.with(...)` (the holder's
    life stays) where it built a one-argument record, and the two Mowzie tests that hand-made a holder's records
    stamp the session's `engineLife()`.
  - Formats. Main did not touch the conflict crate's pages: its fixtures say RTC5 with meta version 4. M4's
    pages stay RTC6 with meta version 5, above them (the camp ledger is in meta 5). Main's own new numbers are
    IGL5 (the glass page), IDS4, `IndustryData` format 3, the package stores' IST2 in table 0x4906 and
    `rtsim/bridge-life.txt`; its new record kinds (0x004F, 0x0051, 0x0418, 0x0419, 0x8408, 0x8409) do not meet
    the camp records (0x080F and its neighbours). The rtsim-view fixtures were taken as main has them for the
    merge and are regenerated once at the end (the lead's order).
- **MZB-651** (RG13 and RG14 against the camp ledger; docs/choices-reinstall-guard.md):
  - RG13 (a chunk's applied reconcile epoch carries the engine life) has nothing to carry here: a camp's epoch
    is a journal op id kept in memory (`Site.epoch`), no chunk or entity stores one. The journal outlives a sim
    reset (RG5), so op ids never repeat across lives, and a CAMP_RECONCILE of an earlier life matches no live
    `Site.epoch` and is answered STALE (the stale-line test). The old engine's own pending orders sit in its pages,
    which the reset moved aside (RG6), so none is replayed.
    What the world does store is a body's `CampMember` (faction, slot, generation), engine identity. A new life's
    slots take generations from `derive_gen` (camp.rs), which hashes the camp, the slot and the tick the member was
    recruited or adopted, so they do not repeat an old life's and a body tagged by the old engine is not mistaken
    for a member of the new engine's slot (a one in 2^32 collision aside).
  - RG14 (pending records carry the engine life): every holder M5 adds takes part. A body or marker takes records
    only through `RtsimSession.carry` and `carryOnCampHolder`, which stamp `engineLife` with `PendingRecords.plus`;
    `Camps.takeRecords` and `answer` call `ensureReasserted` first, so records of another life are dropped
    there (`stalePending`), never put on the marker a body leaves. A holder of another life in a camp's footprint
    loses its records at the census, not journaled: `aHolderJournalsItsRecordAgainBeforeTheCampsNextCensus` has a third
    holder now, a marker whose record is the adopt's bytes under another op id, in the previous engine life: after the
    census the stale count is up by one and a CAMP_BODIES of that op id never reached the engine.
  - Engine side: the old engine's unacknowledged journal tail still reaches the new engine (RG14 "not
    covered"). The engine looks every camp record's faction up in its own books (`by_key`) and an answer's epoch in
    its camp's seen epochs, so a record of a camp it never founded books nothing; the new Rust test
    `camp_records_of_a_camp_this_engine_never_founded_book_nothing` feeds a census, an answer, a death and an out
    for a stranger and checks the stats, the camp's ledger and the orders unchanged. A replayed census only marks its camp out
    (`a_replayed_census_only_marks_its_camp_out`) and a superseded epoch's answer books nothing, as before.
  - Left as it is: a record marker whose records were all of another life stays in the world, empty (a vanilla
    `Marker` tagged `mic_rtsim.record`). A discard would be safe where it would be made: `reassert(Entity)` never runs
    inside the join event (`BridgeEvents` queues loaded holders and drains them at `ServerTickEvent.Post`). The marker
    is kept because it does not tick and is never sent to a client, it saves small and nothing reads it, and its
    number is bounded by the record markers alive at a reset. A later discard would be
    `dropPendingWhenAcked(holder, 0)` after `removeData` in the stale branch of `reassert(Entity)`; not taken. An
    admin can clear them with `/kill @e[type=minecraft:marker,tag=mic_rtsim.record]`. The GameTest discards its own.

- **MZB-652** (main's f361a6f1 merged into M5: SEC-1 S4p, carried records as copies judged by generation;
  docs/boundary.md section 3.2, DECISIONS.md SEC-1 S4p):
  - The merge had no textual conflict. What S4p's `PendingRecords` forced: it is `{life, entries}` now, an entry
    being a record with the generation that journaled it. `with` is gone (`filter` keeps what is left, so
    MZB-650's `p.with(...)` in `dropPendingWhenAcked` is `p.filter(...)`, still by identity), `plus` takes the
    generation (`plus(life, gen, records)`, so MZB-651's stamp is that), and `records()` is derived. The Mowzie
    tests that hand-make a holder's records build entries with the session's generation. The formats: S4p added
    JOURNAL_GEN 0x0130 and REASSERT 0x0131, the jni engine's meta version 2 and its page table 2 (the closed
    generations, magic RTJG). The conflict crate is untouched by it: RTC6 with meta version 5 stays above main's
    RTC5 and 4, and the camp records (0x080F and its neighbours) do not meet the new kinds.
  - How a camp record carried on a holder is judged. It is carried with the generation of the session that
    journaled it. When the holder loads (`BridgeEvents`' drain) or its camp's census reaches it first
    (`ensureReasserted`, MZB-174), the session journals a copy of each, REASSERT(gen, record whole), once per
    holder and session. The engine admits the REASSERT by its own sequence like any record, then judges the copy
    by (gen, the record's own sequence): dropped when the generation admitted that sequence, which is the usual
    case (the pages hold the record, or it was this session's own); applied once, as the record itself under the
    copy's sequence, when a crash lost it or no page holds its generation. An applied copy then meets the camp
    engine's own rules as the record would have: a camp it never founded books nothing, an answer for an epoch
    it has not seen books nothing, an engine op id already heard is a no-op (MZB-651). A REASSERT journaled in an
    earlier session and replayed now is a replay record (its sequence is below the session's first); a live one
    after the replay end is not; `conflict::journaled` reads the copy under the REASSERT's sequence, so the
    notices' "replay is over" test sees it on the right side. The order
    that MZB-174 gave is unchanged: the copy is journaled before the census, so an answer that is new still
    reaches the engine before the census whose epoch would make it stale, and one the engine has is dropped
    without effect. The kept JOURNAL_GEN that goes first (`Journal.pinnedGen`) and the generation blocks are the
    journal's and the engine's, nothing camp specific.
  - A fault the check found, in this branch's code: a marker that takes over a body's records (`removeWithMarker`,
    for the TAKE and REMOVE lines and the SET_OUT paths, and `death`) put them through `carry`, which stamps the current session's
    generation on every record it is given, the body's earlier ones included. A copy is judged by (gen, s), s the
    record's own sequence: a record of an earlier session under this session's generation has s below that
    generation's last sequence (the open generation takes a copy as new only past it, a closed one the same), so
    the copy is dropped even when the earlier session's pages lost the record, and the record is lost for good,
    the loss S4p exists to prevent. Fixed: the records travel as `PendingRecords.Entry` (generation and bytes).
    `Camps.takeRecords` returns entries, `Moved` and `answer`'s `before` hold them, `carryOnCampHolder` takes them
    and `RtsimSession.carryEntries` keeps each one's generation (`PendingRecords.plusEntries`); only a record
    journaled now gets this session's (`boundary().gen()`). `carry` for the other callers (a record journaled in
    this session) is unchanged in effect. A test: `aCampBodysDeathIsBookedOnAMarkerAndAStraysIsNot` gives the
    Frostmaw a record of a generation no session has, kills it, and reads the marker's saved pending: three
    entries, generations now, earlier, now.
  - Readers look through REASSERT: `ConflictKit.campBodies` lists the copies (applied or dropped) with the
    originals, by sequence, as it did the re-journaled records before; `aHolderJournalsItsRecordAgainBeforeTheCampsNextCensus`
    now also checks that both copies were dropped (this session's generation admitted the record), and its stale
    marker check reads the copies too, so a record of another life that was journaled would show.
  - No choice was needed beyond those: a record too large to wrap is journaled plainly (SC129) and a CAMP_BODIES
    of a few dozen lines is far below it; the inert marker of MZB-651 stands.
  - SEC-1 S4c1 (origin/main 428a3311) was merged after the carryon and C2ME rounds ran at a7e315d1, and those two
    results stand: S4c1 touches IndRecords, Industry, the create module (Frontier, PackageStores), the journal
    barrier's industry side and the industry wire, nothing in conflict, camps or the carry path, and the
    count of MZB-655 in `carryEntries` is on a path no test reaches today (an engine life that changed under a
    holder that was then taken over). The merge was textually clean (`git status` had no UU); core, create and
    mowziesmobs ran again on the merged head, since S4c1 voids create's result and the barrier is shared.
  - The first run of MZB-292 (`./gametest.sh mowziesmobs -Pc2me`, at a7e315d1): Mowzie's Mobs and the pack's C2ME
    start together on the GameTest server, and 23 of the 27 tests pass. Four fail on what the suite assumed of
    vanilla's chunk system (the poll is the authority under C2ME, MZB-249 and MZB-290, and the tests were written on
    vanilla): `aRemoveByUuidTakesAStrayAndAnElsewhereBodyIsCountedApart` ("the old w0 object is removed"),
    `aCampFollowsItsChunksThroughThePollWhenTheTicketEventIsSilent` ("out within a poll period of the release (53
    ticks)", the limit being `POLL_PERIOD` + 3), `aHeldFrostmawIsPlacedAfterHoldMaxWithAPlayerStillNear` (sent at
    2941, placed at 9081: 6140 ticks, the limit HOLD_MAX + 40), and
    `withTheTicketEventLiveAnUnloadLeavesThePollNothingToCorrect` (it logs "no ticket event fired for the den's
    chunks (C2ME?): the premise does not hold", then "every footprint level reads as no holder (34)", C2ME's
    holder staying with its target status). Nothing was changed for them: whether the Mowzie suite is meant to
    hold on C2ME, or only core's barrier tests are, is for a decision.

- **MZB-655** (the S4p merge review's fixes: tests and docs, no engine change; MZB-653 and MZB-654 are reserved for
  the merge-review fixes carried into M6, so this record leaves that gap):
  - Two GameTests did not pin MZB-652's generation fix, and now do. `aHolderJournalsItsRecordAgainBeforeTheCampsNextCensus`
    ends with a camp taking a holder over: the Frostmaw carries a record under `EARLIER_GEN` (a generation no
    session has, as a body saved by an earlier session would), a REMOVE order takes the body, the marker that takes
    its records is released with its chunk and loaded again, and the copies read from `ConflictKit.bodiesCopies` must
    be exactly one naming `EARLIER_GEN`, the adopt's answer, applied by the engine, every other the session's own and
    dropped. `aCampBodysDeathIsBookedOnAMarkerAndAStraysIsNot` unloads and loads the chief's marker the same way after
    it read the marker's saved data, with the same checks, a dropped CAMP_DEATH copy and one death booked by the den.
  - Shown to fail on the pre-fix `carry`: in a scratch copy, `RtsimSession.carryOnCampHolder` restamped every entry with
    `boundary().gen()` behind an environment switch (never committed), and the mowziesmobs project ran on the real
    engine with and without it. Without the fix the holder test fails at "one copy names the earlier generation": the
    copies were `[175ada57bab92e3a dropped, 175ada57bab92e3a dropped]`, the session's generation on both. The death test
    fails earlier, at its saved-data check that was already there ("each with the generation it was journaled in, the
    earlier one kept"), so its reload leg did not run in that revert; it is the same stamping seen one step later. With
    the fix the copies are `[123456789abcdef applied, f29f38b92320cd49 dropped]` (holder test) and
    `[f29f38b92320cd49 dropped, 123456789abcdef applied]` (death test); the generation that is the session's changes
    per run.
  - `ConflictKit.campDeaths` looks through `engine.copies(CAMP_DEATH)` as `campBodies` does (`Copy`, `copiesOf` and
    `bodiesCopies` give the copies with their generations). `carryEntries` counts the entries of another engine life
    that `PendingRecords.plusEntries` leaves out in `stalePending` (counted, not asserted: a crash in a save path is
    worse than a counted drop). The death test discards its markers at its end, passed or failed
    (`MowzieKit.discardRecordMarkers`), since they hold records of a generation a later run's engine never heard.
    MOWZIE.md's holder paragraph and its (5)-(6) crash row say a holder's copy is judged by its generation and only an
    applied copy reaches the (op, line) skip.
  - One jni test, `a_camp_record_a_holder_journals_again_is_judged_by_its_generation`, with three cases: a copy of a
    record the engine applied is dropped and books nothing even where the camp has forgotten the line (a plain resend of
    it would book again); a copy of a generation it never heard is booked once, under the copy's own sequence, and a
    second copy is dropped; a copy journaled in the last session and replayed is applied inside the replay window,
    so the replayed census after it still orders nothing and its camp is out. The CAMP_DEATH test's message no
    longer says "by its op id" (M5 removed CAMP_DEATH from `conflict::op_of`: a death is idempotent by the slot's
    generation).

- **MZB-656** (M5's tests wait on far chunks under load; a stopgap before fix/gametest-flakes lands ChunkWait):
  - What failed: with the machine's load average near 50 the merged round's mowziesmobs project failed two of
    M5's own tests on a wait that ran out, `aCampFollowsItsChunksThroughThePollWhenTheTicketEventIsSilent`
    (`Far.ready()`: "chunk true, entities false, ticking false") and `aBodyThatChangesDimensionLeavesItsCampsIndex`
    ("No sequences finished"). With the `Far.ready()` failure of an earlier round that is three different M5 tests
    losing a wait on a far chunk's entities, so it is not a flake to accept.
  - Why: a GameTest's `timeoutTicks` counts ticks, and the server ticks unthrottled, so what bounds a wait in
    wall time is the sleep of core's `RtsimTests.io` (5 ms while a check fails). A test of 1500 ticks has at
    most 7.5 s for a far chunk to load, whatever the load.
  - What was done: only the `timeoutTicks` of the tests that wait on far chunks (all of M5's but
    `aFloodOfChunkLoadsKeepsTheFindersQueueBoundedAndADroppedDenIsFoundAtItsNextLoad`, which builds its chunks
    itself and waits on none, left at 400) are 12000, which is 60 s at 5 ms a failing tick. The hold test keeps
    its 6000 ticks of deliberate hold (`Camps.HOLD_MAX`, quick ticks that sleep nothing) and gets the 12000 on
    top, 18000. `MowzieKit.Far.ready()` and `unloaded()` and core's `io()` are untouched: fix/gametest-flakes
    lands right after M5 with ChunkWait (wall-clock chunk waits) and a converter that rewrites `Far` and the
    tests' waits, and it takes each test's OLD timeout as the bound of its logic waits. The old and new values:
    - `aDensFrostmawStaysVanillaAndTheCensusReadsItsCrystalAndHealth`: 1500 to 12000
    - `aGroveIsCountedAndAResetRecountsTheWarriorsStandingInIt`: 2000 to 12000
    - `aFrostmawAsleepAwayFromItsSpotIsTheDensBodyAndAnIndexedOneOutsideStillAnswersAsChief`: 1500 to 12000
    - `aLateDensFrostmawThatWanderedOffIsFoundAndAdoptedAtTheFirstCensus`: 1500 to 12000
    - `aFrostmawJoiningNearADenWithAFreeSlotAsksForARecountAndOneJoiningAtAFullDenDoesNot`: 1500 to 12000
    - `aBodyKeepsItsIndexEntryThroughAHiddenAndTrackedFlickerAndIsStillTaken`: 1500 to 12000
    - `aLineArrivingInTheTickACampGoesOutIsAnsweredStale`: 1500 to 12000
    - `aHandBackWhoseJoinAnotherModCancelsKeepsThePuppetAndIsStale`: 2000 to 12000
    - `onPeacefulASpawnIsStaleAndAHandBackStillBringsTheBodyHome`: 2000 to 12000
    - `aBodyThatChangesDimensionLeavesItsCampsIndex`: 1500 to 12000
    - `aMemberComingHomeGetsASavedVanillaBodyAndAnAwayPuppetIsNotSaved`: 2000 to 12000
    - `aRefilledPoolIsSpawnedAtTheNextCensus`: 2500 to 12000
    - `aHeldFrostmawIsPlacedAfterHoldMaxWithAPlayerStillNear`: 9000 to 18000
    - `theCensusWaitsForEntitiesOnACampsColdLoad`: 3000 to 12000
    - `aSecondCensusStalesTheFirstsLinesAndARepeatIsIdempotent`: 3000 to 12000
    - `aHeldSpawnIsAnsweredStaleWhenTheDenGoesOut`: 2500 to 12000
    - `aCampBodysDeathIsBookedOnAMarkerAndAStraysIsNot`: 4000 to 12000
    - `aHolderJournalsItsRecordAgainBeforeTheCampsNextCensus`: 5000 to 12000
    - `aSetOutHandsTheCampBodyToItsPuppetAndAnUnclaimedSetOutIsATake`: 2500 to 12000
    - `aRemoveByUuidTakesAStrayAndAnElsewhereBodyIsCountedApart`: 3500 to 12000
    - `aCampFollowsItsChunksThroughThePollWhenTheTicketEventIsSilent`: 2500 to 12000
    - `aGroveWithNoFreePlaceAnswersASpawnStaleAndMakesNoBody`: 2000 to 12000
    - `anUnloadedBodyLeavesTheIndexAtTheNextPollAndALoadIndexesItAgain`: 2500 to 12000
    - `anUmvuthanaPartyWithARaptorUnitBringsNoUntaggedFollowers`: 1500 to 12000
    - `aSetOutPuppetOfTheDensFrostmawJoiningNearTheDenCausesNoRecount`: 2000 to 12000
    - `withTheTicketEventLiveAnUnloadLeavesThePollNothingToCorrect`: 2500 to 12000
  - The dimension test's stall, from its log: the log has no step timestamps, so it cannot name one step for
    certain. It shows (a) no tick at all for 24 s after the test began (the Far constructor logged at 13:23:14, the
    next line at 13:23:38), the tick that ran the first step, which forces the Nether chunk and the den's nine
    chunks, so the stall is most likely a far-chunk load on the server thread, and it costs one tick of the
    timeout whatever the timeout is; (b) the 1500 ticks used up in 9 s after that, about 6 ms a tick, which is
    the 5 ms sleep of failing `io` checks, so most ticks were spent in the two far waits (`far.ready()` and the
    Nether chunk wait), not in the engine round trips (those check without sleeping, so their ticks are quick);
    (c) every step ran, the last `record` line was logged on the final tick, and only the `thenSucceed` one
    tick later did not. So the step it stalled on was a far wait, and the test missed by one tick.
  - The death test's reload leg guards a different mutation than its saved-data check (see the comment in the
    test), so no extra revert run was made for it.
  - The round at 37a34e40 (`./gametest.sh core create mowziesmobs`, one invocation, the three projects' worlds
    deleted first, load average 40 to 60): mowziesmobs passed 27 of 27, the two tests that ran out of ticks
    and the two generation tests of MZB-655 among them. core passed
    116 of 117, and its llama crash test passed. create passed 207 of 208. The two failures are load flakes of
    main's tests, accepted and not rerun: core `aBigDetectionsLongestTickIsBelowTheOneCallTime` ("spread over
    several ticks, got 2"; its one call took 4.0 to 5.8 ms where docs/stress-worlds-results.md gives 1.3 to 1.5
    on a quiet machine) and create `aSweepReportsBoundaryStockAndTheLiveFlag` (timeoutTicks 600, its last wait
    "a sweep with an unloaded chunk is not live" ran out; neither the test nor the sweep was touched by S4c1, and
    it passed in the chain's round at a7e315d1).

### M5 design for the implementer

**Where it stands.** Branch `feat/mowzie-m5`, from `feat/mowzie-m3` at 0a5a4c2c; the code is in 5a87527d,
3ec0ff78, 5f973106, 7f1d4d16 and deeb63d1. Every production class below is written and compiles, and so are
ConflictKit's camp helpers. Not written: `MowzieKit` and `MowzieGameTests`. Nothing of M5 has run yet.
- Compile: `cd neoforge && nice -n 10 nix develop ..# -c ./gradlew --max-workers=4 --console=plain :core:compileJava :mowziesmobs:compileJava -PskipJni`.
- Run: `neoforge/gametest.sh mowziesmobs` (add `core` to rerun the core suite); the log is
  `neoforge/mowziesmobs/build/gametest.log`, grep `'[g]ametest'`. `-PrtsimEngine=stub` runs on the stub.
- When the lead says M4 has landed: merge it, delete `neoforge/*/run-gametest/world`, run the native
  GameTests. M4's decoder was read at b7ace993: CAMP_CENSUS, CAMP_BODIES (action u8 at +12, outcome u8 at
  +13 of each 16-byte line), CAMP_OUT and CAMP_DEATH match `ConflictRecords` byte for byte.

**Jar facts** (Mowzie's Mobs 1.8.2, read with cfr in `target/m5-mowzie-cfr/src`):
- `EntityHandler` holds DeferredHolders `FROSTMAW`, `UMVUTHANA_MINION` (id `mowziesmobs:umvuthana`),
  `UMVUTHANA_RAPTOR`, `UMVUTHI`. Structures: `mowziesmobs:frostmaw_spawn`, `mowziesmobs:umvuthana_grove`.
- `EntityFrostmaw extends MowzieLLibraryEntity`. `getHasCrystal/setHasCrystal` (NBT `has_crystal`),
  `getActive/setActive` (NBT `active`), `alwaysActive`. `requiresCustomPersistence()` is `getHasCrystal()`.
  `finalizeSpawn` sets the crystal. Max health 250 times the config's `healthMultiplier`. While inactive it
  heals 0.3 a tick when `healsOutOfBattle` is on. It wakes when an attackable entity is within 8 blocks and
  it has a target. A creative or invisible player within 1.8 blocks of the crystal takes it when
  `stealableIceCrystal` is on.
- `EntityUmvuthi extends MowzieGeckoEntity`, not `EntityUmvuthana`. Direction is synced data,
  `getDirectionData()` and `setDirection(int)`, default 0. Worldgen makes it at the throne piece's `umvuthi`
  data marker: `setDirection(rotation.rotate(3, 4))`, `finalizeSpawn(STRUCTURE)`, then
  `restrictTo(marker + (0, 0, -18) rotated, on the surface, -1)`.
- `EntityUmvuthanaMinion` and `EntityUmvuthanaRaptor` extend the abstract `EntityUmvuthana`:
  `getMaskType/setMask`, `getWeapon/setWeapon`. The minion's `requiresCustomPersistence()` is true and it
  saves `HomePos`; it restricts itself in `finalizeSpawn` only for COMMAND spawns.
- `FrostmawPieces.FrostmawPiece(StructureTemplateManager, ResourceLocation, BlockPos, Rotation)`: template
  `mowziesmobs:frostmaw_spawn`, size [6, 5, 7], OFFSET (0, 1, 0), no data markers. The Frostmaw is an entity
  in the template, placed without `finalizeSpawn` (the template's own crystal and health).
- `UmvuthanaGrovePieces.FirepitPiece(StructureTemplateManager, Rotation, BlockPos)` extends `Piece`. The
  constructor moves a THRONE or FIREPIT box up 1, but `TemplateStructurePiece.postProcess` recomputes the box.
  The orientation is NORTH, so `findGround(x, z)` is (minX + x, maxZ - z) on MOTION_BLOCKING_NO_LEAVES.
  `postProcess` makes 5 to 9 minions, each tried at up to 20 places 2 to 11 blocks from (4, 4), kept if
  `checkSpawnRules(STRUCTURE)` and `noCollision` pass, then `finalizeSpawn(STRUCTURE)` and
  `restrictTo(findGround(4, 4), 25)`.
- No Mowzie class overrides `shouldBeSaved`, so the core's `EntitySaveMixin` keeps every puppet out of saves.

**Layout.**
- Core, `com.minecraftindustrialcomplex.mic_rtsim`:
  - `conflict/Camps`: the reconciler (sites, the index, held spawns, set-outs, deaths, every record). It is
    owned by `ConflictBridge` (`reconciler()`), which calls `tick(now)` from `endOfTick` after the
    placements, and `claim/claimed/unclaim/handedOff` from `spawn`.
  - `conflict/CampKind`: what a plugin tells the core: `isBody`, `bodyName`, `isChief`, `chief` (census chief
    fields), `make`, `spawnAt`, `holdSpawn`, `joined`, `copy`, `chiefLines`, `kin`, `sound`.
  - `conflict/CampMember`: the attachment's payload. `ConflictRecords`: the layouts of CAMP_CENSUS, CAMP_BODIES,
    CAMP_OUT, CAMP_DEATH and the CAMP_RECONCILE command. `ConflictEvents`: ticket, join and leave listeners at
    LOWEST, kept whatever the module's state.
  - `bridge/RtsimSession`: `ensureReasserted`, `reasserted`, `carryOnCampHolder`, `RECORD_MARKER_TAG`, drop by
    identity. `journal/ChunkWriteBarrier.dependsOnEntities` and `Journal.newOpId` (a camp's epoch).
  - Mixins: `ChunkMapInvoker.mic_rtsim$updating`, `EntitySectionManagerAccessor`, `EntityStorageAccessor`,
    `SimpleRegionStorageAccessor`.
  - `api/RtsimApi`: `registerCampKind`, `declareCamp`, `isCampBody`, `takeCampBody`. The Carry On plugin calls
    the last two.
- Plugin, `com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs`:
  - `MicRtsimMowziesMobs`: stand-ins, kinds, listeners (ChunkEvent.Load, ServerTickEvent.Post, ServerStoppedEvent).
  - `MowzieCamps`: the finder. `DenKind`, `GroveKind`: the two kinds, stateless singletons.
  - To write: `gametest/MowzieKit` and `gametest/MowzieGameTests` (`@GameTestHolder(MicRtsimMowziesMobs.MODID)`,
    `@PrefixGameTestTemplate(false)`, template `arena`, already at
    `src/main/resources/data/mic_rtsim_mowziesmobs/structure/arena.nbt`).

**Finding a camp and its footprint.** `ChunkEvent.Load` queues a small record of a server chunk that has
structure starts (MZB-237; first written to queue the `LevelChunk`, MZB-226). The post-tick drain matches each valid start's registry key: a den uses its `FrostmawPiece` box
(else the start's), a grove its `FirepitPiece`. It calls `RtsimApi.declareCamp(level, startChunk, structure,
kind, pos, spot, box, radius, late)`. `Camps.declare` keys the camp by the start chunk's village id, computes
its footprint chunks (the den's cylinder, MZB-230, or the grove's circle) and reads each chunk's ticket level once
from `ChunkMap.getUpdatingChunkIfPresent` (46 when there is no holder). It restricts the camp's tagged bodies
already loaded and queues the camp for a look. After that only `ChunkTicketLevelUpdatedEvent` moves the
levels: no per-tick scan (D1).

**In, out and the census** (at the end of the tick, `Camps.tick`): settle deaths, resolve set-outs, play held
spawns, then look at each queued camp (MZB-203). Out: CAMP_OUT with the ending epoch, held lines STALE, epoch 0.
In: `census()`:
1. Held lines of the old epoch are answered STALE, and the new epoch is a fresh Java op id.
2. All entities in the footprint chunks' columns are listed once. A body is in the footprint if it is in the
   den's cylinder (MZB-230) or within the grove's radius.
3. Bodies: the kind's bodies inside, alive, not puppets, not another camp's (MZB-208). A stray is flagged
   BODY_STRAY, the chief BODY_CHIEF. Then the camp's tagged bodies loaded elsewhere (from the index), flagged
   BODY_ELSEWHERE.
4. The chief fields come from `kind.chief`. FACTION_FOUND goes if this is the session's first census
   (MZB-206).
5. Every holder in the columns is journaled (MZB-201). Then CAMP_CENSUS is written. That order is the rule of
   MZB-174.

A tagged body loaded from disk outside the footprint of a camp that is in sets `recount`: a new census without
an out.

**Reconcile actions** (`Camps.reconcile`, applied with the hand-over's commands). A camp not in at the command's
epoch answers every line STALE and changes nothing. Lines answered with no holder (NOTHING, STALE) share one
CAMP_BODIES. A line with a holder gets its own CAMP_BODIES, carried on the holder.

| Action | Den (Frostmaw) | Grove (warriors) |
|---|---|---|
| SPAWN | NOTHING if the slot's tagged body is loaded. Held while a player is within 160 (MZB-224) or the spawn budget is spent. Otherwise made at the spot with the line's crystal, asleep; tagged, persistence-required, added; DONE on the body. | The same with no player hold, made at the place of MZB-214. |
| ADOPT | The stray named by UUID (a, b): alive, the kind's body, not a puppet, not another camp's. Tagged (slot, generation), indexed, `joined` restricts it; DONE on it. Otherwise NOTHING. | The same. |
| TAKE, REMOVE | By UUID when a or b is nonzero (a stray or the camp's own), else by tag. Removed with a marker carrying its records and the answer; DONE on the marker. None: NOTHING. | The same. |
| SET_OUT | Waits for the end of the tick: handed to a materialize's puppet (MZB-207), or taken (MZB-222). | The same. |
| HAND_BACK | Party a at epoch b: its live puppet of that body becomes a tagged body where it stands (`make`, `copy`, persistence). The puppet despawns first, then the body is added; DONE on the body. Body already home: the puppet only goes, NOTHING. No puppet: as SPAWN (MZB-213). The party then goes home (MZB-211). | The same. |
| SEAT_HEIR, chief lines | STALE (MZB-209). | STALE. |

**`camp_member` and its index.** The attachment `mic_rtsim:camp_member` holds `CampMember(long faction, int slot,
int generation)`. It is saved with the body and never stored for NONE (faction 0);
`RtsimAttachments.campMember` returns null for an untagged entity. The index, `Camps.index`, maps a faction to an
identity set of its loaded, tagged, non-puppet bodies. EntityJoinLevelEvent and EntityLeaveLevelEvent keep it,
so nothing scans entities. A join restricts the body (`kind.joined`). A leave as DISCARDED while the session is
live is a death (MZB-221).

**Markers.** A `minecraft:marker` tagged `mic_rtsim.record` (vanilla `Tags`) is left where a body was removed or
died. It carries that body's pending records plus the new answer or death in `mic_rtsim:pending`.
`carryOnCampHolder` makes the journal's next file wait for the marker's entity chunk (`dependsOnEntities`). On
acknowledgement the records drop and the marker is discarded (MZB-212). When the marker loads again unacknowledged,
its records are journaled again before any census of its camp.

**UR-1.**
- Without the plugin: no camp is declared. Tagged bodies are indexed but play no lines, and their deaths still
  go to the engine as CAMP_DEATH.
- Without mic_rtsim: NeoForge logs and skips the unknown attachments, so the bodies load as the jar's own.
  Markers are inert: a marker never ticks and is never sent to a client.
- Puppets are never saved. No block, registry entry or entity type is added (D3).

**GameTests to write** (one batch each, every check logged on a `[gametest]` line). Setup recipe for each test:
1. `ConflictKit.releaseFarChunks`, then `ConflictKit.farBase()`. Force the footprint chunks, released in
   `ConflictKit.onEnd`.
2. Wait (`RtsimTests.io`) until every chunk ticks entities (`RtsimSession.entitiesTickAt`) and
   `areEntitiesLoaded`.
3. Lay a stone platform (copy `ConflictGameTests.platform`). Place the bodies, then declare the camp under a
   test name (MZB-227).
4. Read records with `ConflictKit.censuses/campBodies/outcomes/campOuts/campDeaths/censusBodies/factionsFound`.
   Script lines with `ConflictKit.reconcile(faction, epoch, ConflictKit.newOp(), lines)`; the epoch is
   CAMP_CENSUS's u64 at 32. Use `ConflictKit.together` to put a reconcile and a materialize in one hand-over.
   A marker's NBT has to be caught by an `EventPriority.LOWEST` `ServerTickEvent.Post` hook, because it is
   discarded a tick or two after its acknowledgement.

The tests:
1. `aDensFrostmawStaysVanillaAndTheCensusReadsItsCrystalAndHealth` (§7 test 1):
   - Four real `FrostmawPiece`s, one per rotation, each at local (8, 8) of its own chunk; place each with
     `postProcess(level, level.structureManager(), generator, level.random, piece box, chunk, pos)`.
   - The template's Frostmaw is within 1.5 blocks of `denSpot` (MZB-217).
   - Set crystal and health per den, then declare through `MowzieCamps.declareDen(..., TEST_DEN)` with
     `new StructureStart(structure, chunk, 0, new PiecesContainer(List.of(piece)))`.
   - Per den: FACTION_FOUND once, home 1, census flags CHIEF plus CRYSTAL as set. The census has one body,
     BODY_STRAY plus BODY_CHIEF, its UUID, and health at least what was set and at most its health now (it
     heals). The Frostmaw is no puppet and untagged.
   - ADOPT one: DONE. It is tagged (faction, 0, 1) and restricted 8 round the spot, still no puppet,
     `shouldBeSaved`, its persistence flag unchanged (MZB-210), and its NBT has `mic_rtsim:camp_member` under
     `neoforge:attachments`.
2. `aGroveIsCountedAndAResetRecountsTheWarriorsStandingInIt` (§7 test 3):
   - A real `FirepitPiece`, rotation at random. Every warrior it makes is restricted 25 round
     `MowzieCamps.groveCentre` (MZB-218).
   - Add a warrior inside, a raptor inside, a no-AI warrior 27 blocks out, and a no-AI Umvuthi with direction 2.
   - Census: exactly the minions inside, all strays. No raptor, no outside warrior, no Umvuthi body. Chief
     flags CHIEF plus THRONE at the Umvuthi's position, direction 2. FACTION_FOUND home n, CENSUS_CHIEF.
   - ADOPT all in one reconcile: all DONE, tagged, restricted.
   - Release and re-force the footprint: CAMP_OUT, then census 2 lists every adopted UUID with its slot,
     flags 0 or ELSEWHERE. FACTION_FOUND is still once. The reset half is MZB-228.
3. `aMemberComingHomeGetsASavedVanillaBodyAndAnAwayPuppetIsNotSaved` (§7 test 4):
   - A test grove and a band of the grove's faction, two WARRIOR, placed. A puppet is a
     `mowziesmobs:umvuthana`, a puppet, and `!shouldBeSaved`.
   - Set puppet 0's health, then HAND_BACK slot 0 for it. DONE. One held body, no puppet, tagged (faction, 0, 1),
     persistence-required, `shouldBeSaved`, at the puppet's last position with its health, mask and weapon.
     Puppet 0 is removed. Puppet 1 is still unsaved and its party is recalling or gone.
   - Then REMOVE slot 0: DONE. The marker's NBT, caught by the hook, has `Tags` with `mic_rtsim.record` and
     `mic_rtsim:pending`. The marker is discarded after its acknowledgement.
4. `aRefilledPoolIsSpawnedAtTheNextCensus` (§7 test 5):
   - A test den and a test grove with one stray warrior. A fake player stands 40 blocks from the den spot, so
     it is the grove's nearest player within 256.
   - Den SPAWN with LINE_CHIEF plus LINE_CRYSTAL: held (`heldLines`), no answer, no Frostmaw after 40 ticks.
   - Grove reconcile with ADOPT and SPAWN: both DONE. The spawned warrior is 20 (plus or minus 1.5) from the
     centre on the side away from the player, restricted 25, persistence-required.
   - A second SPAWN for that slot gives NOTHING.
   - Remove the player: the den line is DONE. The Frostmaw stands at the spot with the crystal, asleep,
     restricted 8, tagged (faction, 0, 1).
5. `aHeldFrostmawIsPlacedAfterHoldMaxWithAPlayerStillNear` (§7 test 5, timeout 7,000): a player stays within 160.
   The line is DONE no sooner than `Camps.HOLD_MAX` ticks after it was sent, and within 40 ticks after that.
6. `theCensusWaitsForEntitiesOnACampsColdLoad` (§7 test 21):
   - A test den with a crystal Frostmaw; census 1.
   - Release the chunk until it is unloaded (`!level.hasChunk`), then force it again. The camp is seen
     waiting, and no census comes before the entities are in. The next census has the Frostmaw's UUID and
     CRYSTAL.
   - A test grove centred mid-chunk, footprint columns c - 2 to c + 2, forcing only c - 2 and c - 1: column
     c + 2 sits at 34 and the camp stays out, with no census after 40 ticks. Force the rest: a census.
7. `aSecondCensusStalesTheFirstsLinesAndARepeatIsIdempotent` (§7 test 22):
   - A test grove with two strays; census e1. Out and in: census e2.
   - Lines at e1 are STALE and tag nothing. ADOPT at e2 is DONE; the same op again leaves the same tags and
     the same two bodies.
   - SPAWN at e2 is DONE; the same op again is NOTHING, with one body.
8. `aHeldSpawnIsAnsweredStaleWhenTheDenGoesOut` (§7 test 22):
   - A Frostmaw SPAWN held by a player, then the den released: CAMP_OUT and the line STALE.
   - Re-forced with no player: census 2, `heldLines` 0, and no Frostmaw.
9. `aCampBodysDeathIsBookedOnAMarkerAndAStraysIsNot` (§7 test 22):
   - A test grove with four strays, three adopted, and a test den with an adopted Frostmaw.
   - Warrior 0 killed by a fake player: CAMP_DEATH aux BY_PLAYER, killer UUID, slot 0, generation 1, an op id,
     and a marker. A stray killed: no CAMP_DEATH. Warrior 1 discarded: BY_NONE.
   - `RtsimApi.takeCampBody(warrior 2, player)`: BY_TAKEN with the player, and the tag is gone.
   - The Frostmaw killed: FLAG_CHIEF and the FROSTMAW body id.
10. `aHolderJournalsItsRecordAgainBeforeTheCampsNextCensus` (§7 test 22, MZB-174):
    - A test den in chunk (k, z0), Frostmaw adopted. Give it, and a hand-made `mic_rtsim.record` marker in the
      box, a `PendingRecords` copy of the adopt's CAMP_BODIES bytes, set directly so no acknowledgement drops
      them.
    - Force (k + 4, z0) and release (k, z0): the chunk stays in memory at level 35 and its entities unload.
      CAMP_OUT comes.
    - Force (k, z0) again. The re-journaled CAMP_BODIES (same op, highest obsSeq) comes after the CAMP_OUT's
      obsSeq and before the new CAMP_CENSUS's obsSeq. `reasserted(newFrostmaw)` is true.
    - Census 2 lists it tagged (CHIEF, slot 0, generation 1), and it is restricted again on rejoin. The marker
      is discarded after its acknowledgement.
11. `aSetOutHandsTheCampBodyToItsPuppetAndAnUnclaimedSetOutIsATake` (§7 test 22):
    - A test grove with three adopted warriors.
    - `together`: SET_OUT slots 0 and 1 for agent A, then BAND_MATERIALIZE A with one WARRIOR at the grove.
      Slot 0 is handed off: the puppet stands where body 0 stood, with its health and mask, and
      `handedOff` goes up 1. Slot 1 is taken (`taken` goes up 1). Both are DONE, and the camp holds only
      body 2.
12. `aRemoveByUuidTakesAStrayAndAnElsewhereBodyIsCountedApart` (§7 test 22):
    - Three strays. ADOPT w0, REMOVE w1 by UUID: DONE, with a marker.
    - Move w0, no AI, 60 blocks out into a forced chunk. Out and in: census lists w0 BODY_ELSEWHERE (slot 0,
      generation 1).
    - Release and re-force w0's chunk while the camp is in: `recounts` goes up 1 and a census comes with no
      CAMP_OUT.

**Traps.**
- A record marker is gone a tick or two after its acknowledgement: catch its NBT in a LOWEST post-tick hook.
- A Frostmaw heals while asleep: compare census health within a range, never exactly.
- Entities unload when their chunk goes above 33, even while the chunk stays in memory below 45. Forcing a
  chunk 4 away holds one at 35. A forced chunk's neighbours are FULL (32 and 33), so a footprint chunk is out
  only 3 or more chunks from every forced chunk.
- `FakePlayers.add` players count for `Camps.playerNear`, but they hold no ticket and make no natural spawns.
  A kill by one is BY_PLAYER, but a kill by NeoForge's `FakePlayer` is BY_OTHER.
- A BAND_MATERIALIZE places its stacks while it is applied, so a SET_OUT's CAMP_RECONCILE must come first in
  the hand-over. On the native engine, `together` sends two orders in a row, which normally land in one step.
- `ConflictKit.outcomes` keeps a line's first answer. A line answered again (a holder journaled again) is a
  further CAMP_BODIES with the same op; take the highest obsSeq for the latest.
- The GameTest world is reused: each test gets its own far base. Test camps stay declared until the server
  stops, so never reuse a start chunk within a run.

## M6 Frostmaw

- **MZB-250** (the base): `feat/mowzie-m6` starts from `feat/mowzie-m5` with the review fixes (MZB-230 to
  249 and 290 to 292; read at d5b45b22) and merges `feat/mowzie-m4` (2b239fe2) into it. M5 branched from M3,
  so it has none of M4's engine; the two merge without a conflict (`git merge-tree` at d5b45b22 and
  2b239fe2). After the merge delete `neoforge/*/run-gametest/world` (the page format moves, MZB-271). This
  design was written on `design/mowzie-m6`, fast-forwarded to d5b45b22.
- **MZB-251** (a culture's renewal): `CultureDef` gains `#[serde(default)] renewal: Option<RenewalDef>`,
  with `RenewalDef { base_days: f32, cap_days: f32, half_life_days: f32, #[serde(default)] boss: bool }`.
  `Culture::resolve` turns it into `Culture.renewal: Option<Renewal { rule: RenewalRule, boss: bool }>`
  (`rtsim-kernel::renewal::RenewalRule`). Validation, each its own error: every number finite,
  `base_days > 0`, `cap_days >= base_days`, `half_life_days > 0`, and a culture with a renewal must have
  `recruits_per_day == 0` and `camp_bodies` true. A culture without one refills by its rate as M4 built.
- **MZB-252** (the Frostmaw's numbers): `frostmaw.json` gets
  `"renewal": {"base_days": 6.0, "cap_days": 192.0, "half_life_days": 96.0, "boss": true}` and
  `recruits_per_day` 0.0 (EQ-4 (b), MOWZIE.md "The body"). `umvuthana.json` is unchanged: the grove's
  renewal is M9's.
- **MZB-253** (the boss flag, not the camp source): an unclaimed death bumps a culture's pressure only when
  its renewal says `boss`. `CampSource::Boss` cannot carry this, because both Mowzie cultures have it and
  the grove's warriors are not bosses (EQUILIBRIUM.md §2.2: "an unclaimed death of a boss kind").
- **MZB-254** (the den's wait in its camp): `Camp` gains `pressure: Pressure` (8 bytes), `arrival: u32` (a
  stamp, as `refill` is) and `arrival_at: Option<Tick>`. Kept in the camp, the engine's own site row
  (the kernel's rule: "the fields live in each engine's own site rows"). A renewal faction with no camp
  ledger (only engine tests make one) never refills.
- **MZB-255** (when a wait is booked): `plan_arrival(f, at)`, for a renewal culture only, runs at the end
  of `unblock` (after `plan_refill`) and at the end of `camp_found`. The den is short when
  `floor(pool_at) + away < cap`. Short with no wait booked: book `Ev::Arrival { faction, stamp }` at
  `at + rule.delay_ticks(pressure, day).max(1)`, `day = at / TICKS_PER_DAY`. Not short with a wait booked:
  cancel it (stamp + 1, `arrival_at` None). Booked whether the camp is in or out. A running wait is never
  read again, so later losses do not push it out (the kernel's rule). `plan_refill` books nothing for a
  den, since a rate of 0 makes `pool_ready` BLOCKED.
- **MZB-256** (the arrival): `camp_arrival(f, stamp, at)`: a stale stamp counts `stats.stale` and ends;
  else `arrival_at` None, `pool_add(f, 1.0, at)`, then `camp_sync` (one Owed slot), crystal true (MZ-4 (a):
  a new Frostmaw holds its crystal), `stats.arrivals` + 1 and a note; if the camp is in, `spawn_lines` and
  `send_lines` at once (EQUILIBRIUM item 8: a camp that never unloads gets its arrival when the wait
  ends); last `unblock`. An out den gets its SPAWN from its next census, as any Owed slot does.
  Amended at review: the crystal is set true only at a chief camp (`chief_camp(f)`: the home unit has a row
  without the crystal, `bare[u] != u`); a renewal culture without such a row leaves `crystal` as the books
  have it (test `an_arrival_gives_a_crystal_only_to_a_chief_camp`; it fails with the gate removed).
- **MZB-257** (a den found empty waits the cap): `camp_found(f, bodies, census, at)` takes FACTION_FOUND's
  census byte. With `CENSUS_CHIEF` it sets the faction's crystal from `CENSUS_CRYSTAL`. For a renewal
  culture the pressure starts at `Pressure::at_cap(rule, day)` when `bodies == 0` (EQUILIBRIUM item 6:
  "at the cap wait when the site ledger has no row for it"; there is no site ledger before S9, M12) and at
  zero otherwise. Then `plan_arrival`. So a den found empty gets its migrant 192 days later, and one found
  with its Frostmaw (strays within 64 included, MZB-232) books nothing.
- **MZB-258** (which losses are a player's): `renewal_cause(f, by) -> LossCause`: BY_PLAYER and BY_TAKEN
  are `Player`; BY_NONE and BY_OTHER are `Player` for a boss and `Other` otherwise; BY_MOB is
  `Predator`. Only `Player` bumps (kernel). `renewal_bump(f, n, at)` adds `n / cap` on the faction's
  camp's pressure, with `cap` the faction's cap (1 for a den), at the loss's day.
- **MZB-259** (where a loss bumps): `camp_death` (any slot but Dead, before the state match); the away
  path of a chief kill (`chief_killed_by`, below); the census's write-off of missing home bodies (counted in
  the loop, bumped on the camp once it is put back, before its `send_lines` and `unblock`; EQUILIBRIUM item
  11: missing is a player's loss); and the units lost in `raid_district`, a player's district, whose
  defences are the player's. Not `raid_node` (a village's or a rival haven's residents: `Faction`), nor
  `party_lost`, BAND_DEMATERIALIZED or BAND_CASUALTIES: in M6 only the Frostmaw renews, and its one unit is
  its chief, whose death comes as CHIEF_KILLED or CAMP_DEATH.
- **MZB-260** (what a write-off cannot tell): EQUILIBRIUM item 11 exempts a body missing while rtsim was
  absent. M6 cannot see an absence (no site ledger), so every write-off bumps. The cost is one longer wait
  after an uninstall gap; the absence rule is S9's (M12).
- **MZB-261** (kill credit in Java): `ConflictBridge.credit(LivingEntity dead, DamageSource src)` returns
  the killer kind and player. It starts from `byKind(src.getEntity())`; when that is not BY_PLAYER and
  `dead.getKillCredit()` is a `Player` that is not a NeoForge `FakePlayer`, the kill is BY_PLAYER by that
  player. Vanilla's `getKillCredit` is `lastHurtByPlayer` (a player's hit, or a tame `TamableAnimal`'s
  owner, for 100 ticks) or else `lastHurtByMob`, so a pet's kill, and a death within 5 s of a player's hit,
  are the player's (EQUILIBRIUM item 9: "vanilla's kill credit or the killer's owner"). Used by
  `Camps.onDeath` and by `ConflictBridge.onDeath` for a chief puppet (CHIEF_KILLED). BAND_CASUALTIES keep
  M3's `byKind`, since nothing of M6 reads them. Built as a package-private `record Credit(int by, UUID
  killer)`; the killer is null (a zero UUID in the record) unless the credit is a player. A machine
  (`FakePlayer`) that deals the blow is BY_OTHER by `byKind` (MZB-111), and the vanilla fallback then still
  names a real player whose hit came within the 100 ticks: the credit goes to the player, not to the machine.
  `Camps.onDeath` takes the credit when the death event fires, so a hit after that moment changes nothing.
  Corrected at review C: the claim above about a machine is wrong. In vanilla's `LivingEntity.hurt`, the
  killing blow's `FakePlayer` replaces `lastHurtByPlayer` (a `Player` attacker is stored as the last hurt
  by player whether or not it is a `FakePlayer`), so after a machine's blow `getKillCredit` is the machine
  itself, which `credit` refuses as a player. The result of a machine's kill is `Credit(BY_OTHER, null)`,
  and the credit does not fall back to the earlier real player. EQUILIBRIUM item 9 still holds for what it
  names (a pet's kill, a death within 5 s of a player's hit with no machine in the way). For a boss BY_OTHER
  is still a player's loss (`renewal_cause`: BY_NONE and BY_OTHER count for a boss's camp, EQUILIBRIUM.md
  §2.2), so a machine's kill of a Frostmaw still bumps the pressure. No code changed.
- **MZB-262** (one fall for every chief death): M4's `chief_killed` is split. `chief_falls(w, f, killer,
  fall: At, at)` does what both paths share: `stats.chiefs_killed`, the Killing report witnessed by the old
  chief and the heir when there is a killer, `die`, `succeed(killer, true)`, the cowed time and the tape
  event. `step_down(f, a, at)` steps a feud down `chief_rout_steps` rungs (each `next_down`) and floors it at
  Notice, but only when it stands above Notice; a feud at None or Notice is left as it is, and so is its
  `since`. M4 set `to.max(Notice)` unconditionally, so a chief kill raised a None feud to Notice.
- **MZB-263** (a chief killed away): `chief_killed_by(w, p, killer, by, at)` keeps M4's away path through
  `chief_falls` and steps the party's feud, or the killer's when the party has none, and adds
  `renewal_bump(f, 1, at)` when `renewal_cause` is Player. The public `chief_killed(w, p, killer, at)`
  stays for its callers and passes BY_PLAYER with a killer, BY_NONE without; the CHIEF_KILLED ingest
  passes `ck.by`.
  Amended at review: `a_chief_killed_away_bumps_the_pressure_unless_a_mob` also checks the wait that is
  booked (12 days after a player's kill, 6 after a mob's) and that the unit leaves the band (the band gone or
  empty, `away` 0, the slot counts `[0, 0, 0, 0, 1]`). The unit's removal is MZB-295's.
- **MZB-264** (a chief killed at the camp): `chief_killed_at_camp(w, f, killer, at)`: `ensure_chief`; the
  parties carrying the chief flag are collected first (`succeed` clears the flag, MZB-139); the fall at the
  faction's position; only the killer's own feud steps down (MOWZIE.md "Kills": a third player's kill at the
  camp steps no one else's); each collected party routs (Fighting: `end_fight` ROUTED; otherwise
  `stats.routs` + 1 and `go_home` ROUTED), with no breaker and no rescue news, since no band broke; a
  note; every feud replans, as the away path does. The camp's pressure is bumped by `camp_death` (MZB-259).
- **MZB-265** (CAMP_DEATH of a chief): with `DEATH_CHIEF`, a Body, Owed, Left or Gone slot runs
  `chief_killed_at_camp` after the slot is freed (for Left and Gone, before `member_died`, so the party
  that loses its chief unit routs as a chief's band). The killer is `self.player(w, d.killer)` for BY_PLAYER
  and BY_TAKEN, else none. A Dead slot's body is no second fall.
- **MZB-266** (a boss wiped out abstractly loses its chief): `lose_units` gains `w`. When the party carries
  the chief, its culture's renewal is a boss's, the loss was more than 0 and it leaves no unit alive,
  `chief_falls` runs with no killer and the party's feud steps down, before `party_lost`. M4 kept the
  chief alive through an abstract wipe-out, so a Frostmaw killed by a district's defence came back as the
  same chief.
- **MZB-267** (what does not fall a chief): a write-off (the missing Frostmaw is gone, but its chief actor
  stays for the migrant, MZB-259 bumps the pressure); a reset (the pressure is lost with the engine's
  state, S9, M12); a Frostmaw lured off or unloaded away from its den (M12).
- **MZB-268** (CHIEF_STATE in the engine): removed from `Conflict::later` and from the no-op arm of
  `ingest`; `Obs::ChiefState(c) => self.chief_state(c)` sets the faction's crystal from `CHIEF_CRYSTAL`
  and notes it. The health is not stored (nothing of M6 reads it; the census carries it). A record for an
  unknown faction is ignored.
- **MZB-269** (the census corrects the crystal): `camp_census`, when it is not a replay and has
  `CENSUS_CHIEF`, sets the crystal from `CENSUS_CRYSTAL` before `let flags = self.line_flags(f)`, so the
  lines of that census already ask for the right body. This also mends a CHIEF_STATE lost in a crash: it is
  latest-wins, not journaled.
  Amended at review: the byte is applied after the slots are matched, and only when the census holds a body
  flagged `BODY_CHIEF` that the books keep (a matched Body or Left slot, or an adopted stray). Java's
  `DenKind.chief()` takes any chief-kind body in the footprint, including the leftover body of a Dead slot,
  and M6's first form let that body (a robbed Frostmaw that set out while its den was out and died away)
  flip a new Frostmaw's crystal off before the SPAWN went out, so it was made robbed for good. With no kept
  chief body in the census the crystal stays as the books have it; the `LINE_CHIEF` lines of that census
  (generation not 0) are given the flags again once it is known. The synth's census bodies carry
  `BODY_CHIEF` for a chief body, as Java's `isChief` does. Test
  `a_new_frostmaw_keeps_its_crystal_when_a_dead_slots_robbed_body_still_stands`: the scenario above, ending
  with one REMOVE for the leftover and one SPAWN with `LINE_CHIEF | LINE_CRYSTAL` (it fails with the old
  rule); it is also the census-after-an-arrival-with-a-Dead-body-standing test. Residual: with two chief
  bodies standing at once (a leftover and the fresh Frostmaw, for the one census that removes the leftover)
  the byte is the one Java found first, and the engine cannot tell which body it came from.
  Amended at review C: the residue is closed. A census body of the chief kind carries its own crystal in a
  new bit, `BODY_CRYSTAL` (8, in the body flags field: no size change), set from `DenKind.hasCrystal(body)`
  (the core's `CampKind` gains that hook, false by default; the Frostmaw's is `getHasCrystal()`). The
  engine's `camp_census` sets the crystal from the chief bodies it keeps (a matched Body or Left slot, or an
  adopted stray) and the bit of those, never from the camp byte's `CENSUS_CRYSTAL`; a leftover body no slot
  keeps says nothing, whichever body Java found first. `CENSUS_CHIEF` still says whether a chief is present
  and `CENSUS_CRYSTAL` is still sent, as the byte FACTION_FOUND carries. The synth's census bodies carry the
  bit. Test `a_leftover_body_nearest_the_spot_does_not_flip_the_crystal` has the leftover robbed body listed
  before the fresh Frostmaw with the camp byte saying robbed, and the SPAWN still carries `LINE_CRYSTAL` (it
  fails with the old byte read). FACTION_FOUND's census byte has no kept-chief concept (the engine has no
  slots at discovery), so it still comes from the first chief body found: a residue, MZB-289. The GameTests
  that compared census body flags exactly (`MowzieGameTests`, and test 1 here) now include `BODY_CRYSTAL`.
- **MZB-270** (CHIEF_STATE in rtsim-jni): `observe_state` decodes `Obs::ChiefState`, counts
  `ConflictLinkStats::chief_states`, and gives it to the conflict's ingest at the conflict's own time
  (`c.now()`, so no module catches up for it), through the same `with_infra` hand-over as the journaled
  path. The protocol test's deferred count for it becomes the live count.
- **MZB-271** (the page format): `put_camp` and `get_camp` append the pressure (`Pressure::parts`, f32 and
  u32), `arrival` (var) and `arrival_at` (optional u64). The wheel's tag 8 is `Ev::Arrival` (7 is Refill).
  The conflict's page tag RTC6 becomes RTC7 and META 5 becomes 6. No migration (no backward compatibility
  before release): the fixtures are made again at landing (rtsim-view's data and hashes, rtsim-host's
  `gametest_worlds`).
  Amended at review: each camp slot also writes its chief (`Slot.chief`, 8 bytes after the slot's line, the
  slots' `len_prefix` 25 to 33), MZB-296. Still RTC7 and META 6, since M6's bump has not landed.
- **MZB-272** (the synth, the emulated plugin): the synth embodies a band as Java does (MZB-109, MZB-127):
  a stack's `count` units go into `Body.members`, and the chief stack makes one body more, `Body.chief`
  (the body id), outside the units, so a count-0 chief stack is the chief alone. `kill_chief` clears that
  extra body and takes no unit off any culture, a boss's included (a boss's raid carries no unit besides the
  chief, MZB-295); a recalled band's DEMATERIALIZED names the units in `members` and never the chief, as
  Java's survivors do. A band's `spawned` is its units plus the chief. (The first form took one unit off
  the first stack for every culture's chief kill, which hid a mismatch for a boss and cost a goblin raid a
  survivor Java would count.) A count-0 stack's chief ignores a fight's `kill_share`; a band wiped out
  stays in `parties` with no one alive. `camp_kill`
  sets `DEATH_CHIEF` for a body made from a `LINE_CHIEF` line; `CampBody` gains `chief` and `crystal`; a
  killer variant of the camp kill; a `steal` that pushes CHIEF_STATE with flags 0 and clears the body's
  crystal. Run 10's den timings move with MZB-252 (expected).
- **MZB-273** (CHIEF_STATE in Java): `ConflictRecords.CHIEF_STATE = 0x0091` and `ConflictRecords.ChiefState`
  with `FACTION = 8`, `HEALTH = 16`, `SIZE = 24`, `CRYSTAL = 1` (aux), matching `boundary.md`'s row.
- **MZB-274** (the plugin reports a steal): `RtsimApi.chiefState(LivingEntity body, int flags)` calls
  `Camps.chiefState`, which finds the faction of a camp body by its `CampMember` and of a chief puppet by
  `bridge.live(e)` (its stack's `chief`, then its party's faction). Anything else is ignored. The record
  goes into a queue keyed by faction (latest wins: flags and health), flushed at the start of `Camps.tick`
  before `poll`, so it precedes any CAMP_DEATH settled in the same tick. Each is written with
  `boundary().direct(CHIEF_STATE, 24, flags, now)`, the faction at 8 and the health at 16, kept for the
  next tick while `direct` returns less than 0, and dropped with the session (`Camps.clear`). A Peaceful
  world sends it too: the steal happens there. Built with three differences: the queue is also flushed in
  `sessionStopping` and in `switchedOff` (before `off = true`, while the camps still count as live), so a
  state queued in the last tick of a session is not lost; it is cleared at `sessionStarted`,
  `sessionFailed` and `clear`; and the camp-body check is `kind.isChief(body)` on the site's kind (or the
  kind that claims the body when the site is gone), so a camp's non-chief body reports nothing.
  `ConflictBridge.chiefPuppetFaction(Entity)` answers for a chief puppet. The counters are `chiefStatesQueued`
  (every call that queued, a replaced entry counted again) and `chiefStates` (records written), both
  public for the GameTests; `ConflictKit.chiefStates(faction)` reads the records by faction.
- **MZB-275** (a chief's camp death is news): `Camps.death` posts `ConflictNews` CHIEF_KILLED for
  FLAG_CHIEF, as `ConflictBridge.journalLoss` does for a puppet. No chat, action-bar or title line comes of
  it; NOTICE_SHOWN keeps only the roar and the gong.
- **MZB-276** (the steal mixin): `EntityFrostmawMixin` in the plugin injects into `EntityFrostmaw.tick()V`
  after its one call of `setHasCrystal(Z)V` (the steal, `EntityFrostmaw.java:615`), not on the setter,
  which loading NBT also calls (`readAdditionalSaveData`, 963). Server side only. A camp body
  (`RtsimApi.isCampBody`) gets `setPersistenceRequired()`, and any body or puppet of a camp reports
  CHIEF_STATE with flags 0 (MZB-274). Why the persistence: the crystal is the jar's only reason to keep a
  Frostmaw (`requiresCustomPersistence()` is `getHasCrystal()`), so a robbed one despawns, and a robbed den
  Frostmaw must stay disarmed in its den (MOWZIE.md "Robbed Frostmaw": stealing then walking off would
  farm a crystal at every refill). The steal is not a loss and starts no wait (EQUILIBRIUM item 5).
- **MZB-277** (what the mixin leaves alone): an untagged robbed Frostmaw stays the jar's (no persistence,
  no record); a robbed puppet gets no persistence (puppets are not saved, M5); a Frostmaw loaded from NBT
  passes through the setter only. A crystal-less Frostmaw adopted already robbed keeps MZB-210 and MZB-245
  (it can despawn, booked BY_NONE, and as a boss that bumps, MZB-258).
  Amended at review C (P2-a, a robbed Frostmaw does not get its crystal back as a raid puppet): any raid
  puppet not made from the den's body is a new Frostmaw, and the jar's `finalizeSpawn` gives every new
  Frostmaw a crystal, so a robbed den's raid came back armed. The build lead's preference was a bare body id
  (`frostmaw_bare` mapped to the Frostmaw with a post-create step). Built instead as a stack flag,
  MEMBER_BARE (2) in BAND_MATERIALIZE's stack flags (no size change), set by `spawn_wave` when the unit has
  a bare row (`c.bare[u] != u`) and the faction's crystal is false. The reason: a bare body id would make
  body identity depend on the crystal's mutable state in `claim`, in the HAND_BACK, in `back_of`, in
  casualties and in `member_died`, which all go by body id, so a body's identity would follow a state that
  changes while the raid is out. The flag costs one bit of an existing field
  and is read once, at materialize. The core stays generic: `ConflictRecords.Materialize.STACK_BARE`;
  `ConflictBridge` marks each slot of such a stack `bare`; after it creates a bare slot's body, and before
  `kind.copy`, it calls `BodyKinds.makeBare(body, mob)`, so a body handed off from its den, which `copy`
  fills in afterwards, keeps its own crystal state; the plugin owns the Frostmaw knowledge
  (`BodyKinds.registerBare(FROSTMAW, f -> f.setHasCrystal(false))`, like `registerStandIn`). A refilled
  slot is made bare again. GameTest `anUnwatchedRaidOfARobbedDenPlacesAFrostmawWithoutItsCrystal` (batch
  `frostmaw_raid_bare`; compiled, not run): two fresh count-0 chief stacks, one flagged bare, one not; the
  first has no crystal and the second has one. Rust `a_robbed_frostmaws_raid_is_materialized_bare` checks the
  flag on a robbed den and its absence on an armed one.
- **MZB-278** (UR-1 and D1 for the mixin): after an uninstall a robbed den Frostmaw keeps
  `PersistenceRequired`, which the jar reads as any persistent mob; it stays in its den, harmless. Without
  the plugin (rtsim alone) a robbed den body despawns and is booked BY_NONE. The mixin runs once per steal,
  a call that happens a few times a world; the queue holds one small entry per robbed faction.
- **MZB-279** (the footprint and the restriction stay as M5 has them): the cylinder of 11 (MZB-230) and
  `restrictTo(spot, 8)` on each join (MZB-223) fit M6; M6 needs neither changed. A jar fact the M5
  review did not use: the restriction also gates targeting. `TargetGoal.canAttack` accepts only a target
  whose block is inside the restriction (a sphere, `distSqr < 64`), so a den Frostmaw notices a player
  near its spot, not one near itself 5 to 8 blocks off; a chase is not bound (`canContinueToUse` does not
  check it), and with no target `MoveTowardsRestrictionGoal` (priority 5) walks it home before it sleeps.
  A den Frostmaw is thus a little harder to wake than a wild one; struck, it wakes and targets as the
  jar's does. Recorded, not changed: the 8 is a PLACEHOLDER (MZB-223).
- **MZB-280** (the arrival's spawn is M5's held spawn): the arrival's SPAWN waits while a non-spectator
  player is within 160 blocks of the spot, for at most `Camps.HOLD_MAX` = 6,000 ticks (MZB-224), as MOWZIE.md
  "The body" says ("once no player is within 160 blocks or after 6,000 ticks", EQ-4 (b), Ben). That
  supersedes EQUILIBRIUM item 8's 128 blocks and the kernel's `Arrival::OnLoad` doc, which say never into
  a watched site. On Peaceful the SPAWN is STALE (MZB-234) and the next census orders it again. A SPAWN
  without `LINE_CRYSTAL` is made persistent by the core (MZB-210).
- **MZB-281** (the thief in a GameTest): the steal reads `getPlayersNearby`, which is
  `level.getEntities(this, box)`: the entity manager, not `level.players()`. A `FakePlayers.add` player is
  only in `players()`, so the jar never sees it. The thief is `helper.makeMockPlayer(GameType)` (a plain
  `Player`, not a `ServerPlayer`) added with `level.addFreshEntity`: it holds no chunk ticket, is never
  a target (`NearestAttackableTargetGoal` reads `players()`), ticks its effects, and the jar handles a
  non-server player (`if (!(player instanceof ServerPlayer)) break`). It is made invisible by the
  invisibility effect, since a ticking entity's first `tickEffects` clears a bare `setInvisible(true)`
  (`effectsDirty` starts true). UNVERIFIED: that a mock `Player` joins and ticks in a server level without
  trouble. The fallback is `FakePlayers.addSeen(level, at, name)`, built as `add` but joined with
  `level.addNewPlayer(p)`, given the effect and `setInvisible(true)` (its effects never tick), removed with
  `removePlayerImmediately` before any release (it loads chunks round itself). The implementer records
  which one ran. Built (M6 step 10, compiled and not run: the lead keeps the GameTest flock for another
  round): the mock `Player`, as `MowzieKit.thief(helper, frostmaw, mode, invisible, fullInventory)`; no
  `addSeen` exists. I read the paths a plain `Player` takes on a server: `Player.tick` posts
  `PlayerTickEvent.Pre/Post` (Mowzie's handler only uses attachments and the inventory, with no cast), the
  rtsim join hooks ignore anything that is not a `Mob` or an `ItemEntity`, the jar's steal breaks out
  before its `ServerPlayer` advancement when the thief is none, and the mixin's injection point (right
  after `setHasCrystal(false)`) comes before that break. Still UNVERIFIED on a running server: whether a mock `Player` ticks in the entity
  list without trouble. The thief moves to the crystal's point (`MowzieKit.crystalPoint`) at the end of
  every tick until the Frostmaw has lost its crystal; the spectator thief is also invisible, so that only
  the jar's no-spectator filter can explain a robbery that does not happen; the full-inventory thief holds
  one dirt in each of the 36 main slots. Test 3 puts its five Frostmaws 48 blocks apart, not 40, so that
  each has its own 3 x 3 forced chunks.
  Amended at review C: the mock `Player` is replaced by `FakePlayers.addSeen`. A mock added with
  `addFreshEntity` is in the entity manager but not in `level.players()`, which `NearestAttackableTargetGoal`
  reads, so every "the Frostmaw sleeps on" of test 3 held for a reason a real player does not share.
  `addSeen` joins a `ServerPlayer` with `level.addNewPlayer`, so it is in both; `MowzieKit.thief` now makes
  that, sets its game mode and keeps it alive with Resistance V and a health top-up each tick (a target the
  Frostmaw may strike without ending the test; not invulnerable, because vanilla's `canBeSeenAsEnemy` is false
  for an invulnerable entity, so no targeting goal would take it and the jar's wake, which needs a target,
  could never fire: so the M6 re-review found), gives it the invisibility effect and `setInvisible(true)` when asked, and removes it with
  `FakePlayers.removeSeen` at the end (it loads chunks round itself, so the robbed-Frostmaw test removes it
  before it releases the den). The jar's own rules (the 1.8.2 sources): a visible target within 8 blocks wakes a
  sleeper, an invisible one is dropped from the target each asleep tick, a spectator is no target
  (`NO_SPECTATORS`). So test 3 asserts that the spectator's Frostmaw sleeps on, and it records, without
  asserting, whether the visible survival player's and the full-inventory invisible thief's Frostmaws woke.
  The old per-Frostmaw "and sleeps" check on the three thieves that steal nothing is replaced by the
  spectator's alone, and each of those three Frostmaws is also asserted to keep its crystal. UNVERIFIED: `addSeen`
  compiled and not run; it may log or fail where a login would not. If it does, the thief reverts to the mock
  `Player`, and the clause is written down as not proven.
- **MZB-282** (test 6 without TaCZ): the Mowzie GameTest run holds only rtsim, Mowzie's Mobs and GeckoLib
  (D4), so the bullet is a Snowball, a `Projectile` that is not an `AbstractArrow`; TaCZ's
  `EntityKineticBullet extends Projectile` takes the same branch (cfr). The jar's arrow branch catches
  `source.getDirectEntity() instanceof AbstractArrow`, which a `ThrownTrident` is.
  Built with the player 80 blocks from the Frostmaw, beyond its follow range of 50: a nearer survival player
  in `level.players()` is picked as the target by `NearestAttackableTargetGoal` while it sleeps, which would
  leave the arrow's `setTarget` (taken only when it has none) with nothing to show.
- **MZB-283** (test 15's Umvuthi half is M9's): M6 tests the Frostmaw half only.
- **MZB-284** (sim run 6, the den): checks and tolerances as in the design below. The farmed waits are
  compared with the kernel's own `delay_days` on the run's days (exact), and with the whole-day closed form
  within 3%; the 60-year rate within 5% of `96 / farmed(1, 2.0).1`.
- **MZB-285** (sim run 7, the kills): the seven cases of the design below, run 1's tail moved here.
- **MZB-286** (M4's rate tests keep a rate): engine tests that need the Frostmaw's refill by rate
  (`a_pool_short_of_a_whole_warrior_waits_for_it` and any the suite shows failing) run on
  `edited(FROSTMAW_JSON, |d| { d.renewal = None; d.recruits_per_day = 1.0 / 6.0; })`, so they keep testing
  the refill path, which the grove still uses.
- **MZB-287** (counters): engine `stats.arrivals` and `stats.renewal_losses` (bumps); the wheel's pending
  name "arrival"; rtsim-jni `chief_states`; Java `Camps.chiefStates` (written) and `chiefStatesQueued`.
- **MZB-288** (boundary.md): the Mowzie paragraph of §9 (CHIEF_STATE's engine half built in M6), the
  CHIEF_STATE bullet (the steal sends flags 0; the census corrects a lost one), the "Deferred kinds" bullet
  without CHIEF_STATE, and one line on the kill credit (MZB-261) under CAMP_DEATH and CHIEF_KILLED.
  Built (step 11, with review C): those four, and the review C changes: BAND_MATERIALIZE's stack flag 2
  (the stack's units are made without a crystal), CAMP_CENSUS's body flag 8 (the crystal is on this chief
  body), the paragraph on a boss's chief being its band's one unit (count 0 with the chief flag, the living
  chief back into the survivors), the paragraph on how the engine reads the census's crystal, the observation
  line's list of kinds the engine acts on (CHIEF_STATE added), and MZB-261's machine case corrected.
- **MZB-289** (gaps left open): the pressure and a running wait are lost at a reset, and FACTION_FOUND
  carries no site ledger row (S9, M12); a CHIEF_STATE lost in a crash leaves the engine's crystal wrong
  until the den's next census (MZB-269), which matters only to a raid drawn from it in between; a Frostmaw
  lured from or unloaded away from its den is M12's.
- **MZB-293** (where the rate check sits, built): `RenewalDef::rule(culture, what)` holds the numbers of
  MZB-251 (finite, `base_days > 0`, `cap_days >= base_days`, `half_life_days > 0`) and returns the kernel's
  `RenewalRule`, so a second renewal (M9's `chief_renewal`) calls it with its own `what` and none of the
  culture-level checks. The checks that `recruits_per_day == 0` and `camp_bodies` is true belong to the
  `renewal` field alone, in `Culture::resolve`: a culture whose chief renews while its warriors still refill
  by rate is legal, and the Frostmaw's rule does not have to be edited for it. The `rate_den_cultures`
  helper of `synth` (MZB-286) and `CampRun::with_cultures` serve the engine tests that keep a rate.
- **MZB-294** (the chief paths, built; MZB-262 to 266): `chief_falls` carries a `what` that opens the tape's
  text, so the away path's tape line is the one M4 wrote ("the band routs; cowed for N days") and the den's
  and the abstract wipe-out's are their own ("the chief's body falls", "the band is wiped out"). A body
  taken (BY_TAKEN) names a killer only when the record's `killer` is not the null UUID: the emulated
  plugin sends none for a carried-off body, and `player(w, (0, 0))` would mint an actor for it. Such a
  chief falls with no killer, steps no feud down and still bumps the pressure (MZB-258). The killer's
  actor is made only when the death is a chief's, so no other camp death changes the knowledge's actors.
  This amends MZB-265.
- **MZB-295** (the pressure's order in the away path, built): CHIEF_KILLED is what Java sends for a chief
  puppet, never BAND_CASUALTIES, so `chief_killed_by` bumps the pressure right after `chief_falls` and the
  loss reaches the books once. The DEMATERIALIZED that follows, with the chief missing from its survivors,
  runs `party_lost`, whose `unblock` plans the wait from a pressure that already holds the bump. An
  abstract wipe-out (`lose_units`) bumps before `party_lost` for the same reason; `camp_death` bumps
  before its slot is freed. A boss whose chief fell away is not stepped down twice: `step_down` leaves a
  feud at or below Notice alone.
  Amended at review: CHIEF_KILLED also takes the unit off the band. `chief_killed_by` gained
  `epoch: Option<u32>` (the record's epoch; `None` for the public `chief_killed`) and calls
  `chief_unit_lost(p, epoch, at)` after `renewal_bump`. For a boss band only (`renewal.boss`; another
  culture's chief is an extra body outside the units, MZB-109, so its unit count is not the chief's) it takes
  the first unit with a count off `units`, off the `recalled` stack of that epoch when the band is not
  embodied and holds it, else off `spawned`, then `stats.casualties` + 1 and `party_lost(p, 1, at)`, with no
  second bump. The DEMATERIALIZED that follows cannot take the chief off twice: `units`, `spawned` and
  `recalled` are all down, and one for a band already removed is ignored. Before this a crash between
  CHIEF_KILLED and DEMATERIALIZED had `replay_done` count the dead body as never reported alive and bring the
  Frostmaw home with no arrival booked (it dates from M4). Test
  `a_chief_killed_away_stays_dead_across_a_crash_before_the_band_is_counted_out`: after `restart()` the band
  has no Frostmaw, `away` is 0, the pressure is 1 and the arrival stands at the death plus 12 days.
  Amended at review C (P1, one Frostmaw per raid): the boss's chief is its one unit, but `spawn_wave` sent it
  as a stack of count 1 with MEMBER_CHIEF, which Java embodies as one unit slot plus MZB-109's extra chief
  slot: two Frostmaws (the unit slot took the den's body, the chief slot a fresh one with a crystal), and
  killing the den's body first counted as a casualty, not a fall. Fixed in the engine, Java keeping MZB-109's
  meaning: (1) `spawn_wave` takes `boss` (the same `renewal.boss` test as `chief_unit_lost`) and sends
  `t - 1` on the chief stack for a boss culture (`if chief && boss { t - 1 } else { t }`), so one Frostmaw is
  count 0 with MEMBER_CHIEF, MZB-127's chief alone; (2) in the Dematerialized arm, on the live path and on the
  recalled path, the living chief is added back into the first unit's `backs`, since Java's survivors leave
  the chief out. Tests in `tests/frostmaw.rs`: `a_frostmaw_raid_is_materialized_as_the_chief_alone` (count 0,
  MEMBER_CHIEF), `a_recalled_frostmaw_band_keeps_its_unit_while_the_chief_lives`,
  `a_chief_killed_then_counted_out_takes_exactly_one_unit`. GameTest
  `aFrostmawRaidMadeFromTheDensBodyEmbodiesOneFrostmaw` (batch `frostmaw_raid_one`; compiled, not run): a
  SET_OUT of the den's body and `StackSpec(FROSTMAW, 0, chief)` place exactly one Frostmaw within 64 blocks
  of the den, the den's own, with its crystal. Why the tests missed it: the synth took the chief for a unit,
  so the mismatch with Java's slots could not show (MZB-272, reworded).
- **MZB-296** (a chief the books already buried, built): `chief_killed_at_camp` compares the faction's chief
  before and after `ensure_chief`; a succession `ensure_chief` ran means the body that died was the old
  chief's (dead of age), so the kill is stale and nothing falls, as the away path's stale rule has it
  (MZB-146). The camp's pressure is still `camp_death`'s, since the body was a member lost. `camp_death`
  runs the fall for a Body or Owed slot after it is freed, for a Left or Gone slot before `member_died`,
  and never for a Dead slot.
  Amended at review: the before/after comparison was order-dependent (when something else ran `ensure_chief`
  first, the late kill of the old body felled the new chief). `Slot.chief` now holds the faction's chief
  when `camp_alloc` made the slot, saved in RTC7 (MZB-271), `camp_death` reads it from the slot, and
  `chief_killed_at_camp(w, f, body_of, killer, at)` runs `ensure_chief` and falls the chief only if
  `factions[f].chief == body_of`; otherwise the body is only an ordinary member lost. Test
  `a_late_kill_of_the_old_chiefs_body_leaves_the_new_chief_standing`: an age death and a succession by
  another path, then the late kill of the old body, and the new chief stands (it fails with the stamp check
  removed). Only the camp path needed this: the away path's stale rule is the party's `chief`
  flag, which `succeed` clears (MZB-139).
- **MZB-297** (the synth marks the chief, built; MZB-272): `CampRun::new` marks the first body of a camp
  whose home unit has a row without its crystal (`bare[u] != u`) as the chief with its crystal on it, and
  gives the FACTION_FOUND record `CENSUS_CHIEF | CENSUS_CRYSTAL` for it; a body the plugin spawns takes
  `chief` and `crystal` from its line's flags. Only the Frostmaw has such a unit today, so the umvuthana
  tests (M4) see no change; M9's `without_crystal` rows will mark theirs the same way. `CampRun.infra`
  (none by default) lets a test put walls under a raid.
- **MZB-298** (CHIEF_STATE, built; MZB-268 to 270): the engine's `chief_state` ignores a record for an unknown
  faction or a culture with no camp bodies, and notes only a change of the crystal. The link counts every
  CHIEF_STATE it decodes in `chief_states` whether or not the engine acts on it, and the protocol test of the
  later slices' records now counts it apart from `deferred`. The synth's census carries the chief byte from
  the world (a chief body in the camp, its crystal flag), `FakeConflict::steal` clears the body's crystal and
  sends the record, and `CampRun::robbed` finds a den with its chief already robbed.
- **MZB-299** (the heir's grudge is about 62.5, not 2.5; run 7 (b) and (c), built): `succeed()` hands the
  heir grievance of -sentiment times `heir_grudge_factor` (0.5), and a witnessed Killing report is -125
  (MZB-36), so the heir's grudge against a player who killed the chief is about 62.5, four times the
  Frostmaw's Notice threshold (15.6), not the "about 2.5, under Notice" that MOWZIE.md and run 7 (b) of the
  design said (the 2.5 dates from before MZB-36, when the report was -5). What keeps B's feud under Notice at
  first is that the faction is cowed after the fall (`cowed_until`), not the size of the grudge. Ruled at
  review: the engine's value is right. Run 7 (b) asserts it (62.5 within 2, above Notice's threshold, the
  feud still at no step, the faction cowed) and then runs past `cowed_until` an hour at a time: B's feud
  reaches Notice and no higher (one Notice climbed, none from Envoy up), and decays away about 6 days
  after the window. Run 7 (c) does the same for C, who also gets the kill's report (Notice and no higher, gone
  about 9 days after the window). MOWZIE.md is corrected at both places (the third player's kill, and the
  walkthrough's "plus about 62.5"); CONFLICT-LIFE.md is left alone. The regenerated `sim-conflict.json` also
  moved (survivors 20 to 19 and 8 to 7) because of MZB-272's synth fix: the dead chief is no longer counted a
  survivor.
  Amended at review C: that was the synth's mistake, not the engine's (a goblin chief is an extra body outside
  the units, MZB-109, so killing it loses no unit). With MZB-272 reworded the survivors are back at 20 and 8,
  and the conflict's state hash is again `cccb9a4f55839f1f`, the value it had before M6's step 7; the
  regenerated `sim-conflict.json` moves only in `wall_ms`, the hash and those two lines are the same as at
  the merge of M5 (`0d6eee77`). `sim-mowzie.json` did not change.
- **MZB-589** (BAND_CASUALTIES carries an op id, so a copy counts once; lent from M12's range; found by the build
  lead in the reassert path): `RtsimSession.reassert` commits the PENDING records of an unloaded chunk or entity
  holder again under a new OBS_SEQ with the same bytes, and the ack is the engine's saved-pages sequence, which
  lands about one autosave later. rtsim-jni's replay forgets its ops at WORLD_SAVED, so a BAND_CASUALTIES
  (journaled on the dead body's entity, with no op id) was applied a second time: another member off the
  band, another Killing signal, an early rout. The fix:
  - **Wire.** BAND_CASUALTIES gets `OP_ID` at 64 and size 72 (`boundary.md` §9). Java's `journalLoss` commits
    it with `commitWithOpId`, so each death has its own Java op (top bit set) and a copy has the same one.
    `BandCasualties.op` in the reference codec; `Obs::op()` still does not list it.
  - **Dedupe is per party** (`Party.casualty_ops: Vec<u64>`, saved with the party), not in the world's
    `applied_ops` and not in `Obs::op()`: `applied_ops` never shrinks and is saved at every save, so one entry
    per member death of the world's life would be the D1 leak. The list goes with the party, and is counted in
    the party's memory estimate. `casualties()` returns at once for an op in the list, live or off a recalled
    stack alike, and pushes the op only after the record took a unit (`t > 0`), so the list holds at most
    `start_units` ops. Two records for one body with two ops are two deaths.
  - **Format.** The party record gains a length-prefixed list of u64s at its end (RTC7 as it stands, M6's own
    unreleased format: no version bump and no migration). The conflict's
    state hash therefore moved, `cccb9a4f55839f1f` to `06fb447b6cb6d991` (the empty list's length prefix is in
    every party's bytes), and the `ignore` run's wire bytes went 3080 to 3104 (three casualty records, 8 bytes
    each). Survivors (20 and 8), the checks and every other line of `sim-conflict.json` are unchanged;
    `sim-mowzie.json` moved only in the state hash of run 8's muster set-outs detail line.
  - **The synth** gives each casualty its own deterministic op (`FakeConflict::op()`), so the sims stay
    deterministic.
  - **A copy after the party ended** finds no party: the copy carries the old agent id, `party_of` checks the
    handle's generation and the epoch, and `handle.rs` retires a slot before its u16 generation would wrap, so
    an old agent id never names a later party in the same slot (a reused slot's party starts at epoch 0, as an
    old one's first epoch does: only the generation tells them apart). A copy after the despawn took the epoch's
    stack is no death, as before (MZB-138, 142).
  - **A crash** is the journal's job, not the list's: the pages that survive predate the casualty, so the list
    does not hold its op, the replay applies the record once, and the reasserted copy is skipped. The crash test
    `a_walk_offs_dead_are_counted_again_by_the_replay_after_a_crash` is unchanged and green.
  - **Tests.** In `tests/mowzie_records.rs`: `a_casualty_copy_in_one_epoch_counts_once` (one unit, one count,
    one grudge move and breaker, equal to an engine that saw it once), `a_casualty_copy_does_not_rout_the_band_early`
    (a loss of four of twelve, twice would be the half that routs), `two_casualties_of_one_body_with_two_ops_both_count`
    (and the memory estimate grows 8 bytes an op), `the_list_keeps_only_the_casualties_that_took_a_unit`,
    `a_casualty_copy_after_a_save_and_reopen_counts_once` (the same session, and a restart's replay),
    `a_casualty_copy_on_a_recalled_stack_counts_once`, `a_casualty_copy_after_the_recall_settled_changes_nothing`,
    `a_casualty_copy_of_an_ended_party_does_nothing_to_the_party_in_its_slot` (live, after a reopen and a
    restart). rtsim-jni's `a_casualty_committed_again_after_a_world_save_counts_once` runs the real reassert
    path end to end (the same record committed twice with a WORLD_SAVED between). Java's `ConflictGameTests`
    asserts each casualty carries a distinct Java op id (compiled only). Without the early return, five of the
    eight rtsim-conflict tests fail; the recall-settled test passes either way (the straggler rule already
    drops it, so it pins that rule), and the reused-slot test fails only when `party_of` ignores the
    generation.
  - **Same tick.** These tests ingest the copy at the first's tick: the feud's grievance decays by the
    clock, so a copy a tick later would differ in the sixth digit with nothing counted twice.

### M6 design for the implementer

**Where it stands.** Design only, on `design/mowzie-m6` (from d5b45b22). Nothing of M6 is written. The open
questions for Ben: none. The two candidates are settled by Ben's own answers: the arrival's hold (160 blocks
or 6,000 ticks, MOWZIE.md "The body", EQ-4 (b), MZB-280) and a den found empty (the cap wait, EQUILIBRIUM
item 6, MZB-257).

**Jar facts** (Mowzie's Mobs 1.8.2 with cfr in `target/m6-mowzie-cfr/src`; vanilla from the NeoForm sources
jar in `target/m6-vanilla`):
- The steal (`EntityFrostmaw.tick`, 608-623) is in the asleep branch (590-625, after `setActive(false)` at
  580-588), so only a sleeping Frostmaw is robbed, and only when `stealableIceCrystal` (the pack's
  `mowziesmobs-common.toml`: true), `getHasCrystal()`, `tickCount > 20` and `getAnimation() == NO_ANIMATION`.
  The crystal is at `new Vec3(1.6, 0.4, 1.8).yRot(toRadians(-getYRot() - 90)).add(position())`. Each
  player of `getPlayersNearby(8, 8, 8, 8)` is tried: it must be within 1.8 blocks of that point, creative or
  invisible (`isCreative() || isInvisible()`), and not `isInventoryFull`, which is true when all 36 main
  slots hold something, whatever room the stacks have. It gets `ItemHandler.ICE_CRYSTAL` by
  `ItemHandlerHelper.giveItemToPlayer`, then `setHasCrystal(false)` (bytecode offset 3327, the method's only
  call of it), then, off Peaceful, `ACTIVATE_NO_CRYSTAL_ANIMATION` and `setActive(true)`; a `ServerPlayer`
  also gets the advancement. There is no client-side guard: the mixin returns on the client itself.
- `MowzieEntity.getPlayersNearby` is `level().getEntities(this, getBoundingBox().inflate(...))` filtered to
  players within `radius + width / 2`. `EntityGetter.getEntities(Entity, AABB)` filters with
  `EntitySelector.NO_SPECTATORS`, so a spectator never steals. A creative player is no attackable entity
  for the wake check (`getAttackableEntityLivingBaseNearby`), but the steal wakes the Frostmaw anyway.
- The wake check (in the same branch, before the steal) needs an attackable entity within 8 and a target.
  The target comes from `NearestAttackableTargetGoal(Player, 0, true, false)` (priority 1, reads
  `level.players()`), and a target with the invisibility effect is dropped each asleep tick.
- Goals: `FloatGoal` 0, two animation goals 2, `MoveTowardsRestrictionGoal(1.0)` 5,
  `WaterAvoidingRandomStrollGoal` 7, look goals 8. Max health 250 (times the config's multiplier), follow
  range 50.
- `hurt(DamageSource, float)` (855-905): fall damage is refused; a direct `AbstractArrow` (arrows,
  tridents) plays the "undamaged" sound, takes the attacker as target when it has none (not a creative
  player, not a Frostmaw), wakes it off Peaceful and returns false; everything else goes to `super.hurt`,
  then the same target and wake.
- Death: `DIE_ANIMATION` runs 94 ticks and the body is removed 20 before its end, 74 ticks after death.
- `requiresCustomPersistence()` is `getHasCrystal()`; `readAdditionalSaveData` calls `setHasCrystal` (963).
- TaCZ's `EntityKineticBullet extends Projectile`.
- Vanilla: `LivingEntity.getKillCredit()` (1814) is `lastHurtByPlayer` else `lastHurtByMob`;
  `lastHurtByPlayer` is set in `hurt` (1210-1227) for a player's hit and for a tame `TamableAnimal`'s owner
  (`getOwner()` reads `level.getPlayerByUUID`, so a `FakePlayers.add` owner counts), for 100 ticks.
  `TargetGoal.canAttack` (101) returns false for a target outside `mob.isWithinRestriction` (1304: no
  restriction, or `distSqr < r * r`). `Mob.canBeLeashed()` (1405) is false for an `Enemy`, so a Frostmaw
  cannot be leashed. `DamageSources.thrown(Entity causingEntity, Entity directEntity)` has its parameter
  names swapped: `Snowball` calls `thrown(this, getOwner())`, so pass the projectile first.
- `ServerLevel.addFreshEntity` posts the join and adds to the entity manager; `addNewPlayer` does the same
  for a `ServerPlayer` and its tracking start adds it to `players()` and to the chunk map (a player ticket).

**Layout.**
- Engine: `crates/rtsim-conflict/src/culture.rs` (MZB-251), `data/cultures/frostmaw.json` (MZB-252),
  `src/engine/camp.rs` (fields, `plan_arrival`, `camp_arrival`, `camp_found`, `camp_death`, `camp_census`,
  `renewal_cause`, `renewal_bump`), `src/engine.rs` (`Ev::Arrival` and its dispatch, `chief_falls`,
  `step_down`, `chief_killed_by`, `chief_killed_at_camp`, `lose_units`, `raid_district`, `chief_state`,
  `later`, `ingest`, stats), `src/persist.rs` (MZB-271), `crates/rtsim-jni/src/engine/conflict.rs`
  (MZB-270) and its protocol test, `crates/rtsim-cli/src/sim/mowzie.rs` and its synth (MZB-272, runs 6
  and 7), `crates/rtsim-conflict/tests/frostmaw.rs` (new).
- Core Java (`neoforge/core/.../mic_rtsim/`): `conflict/ConflictRecords.java`, `conflict/Camps.java`,
  `conflict/ConflictBridge.java`, `RtsimApi.java`, `gametest/ConflictKit.java` (`chiefStates(faction)`),
  `gametest/FakePlayers.java` (only for the fallback of MZB-281).
- Plugin (`neoforge/mowziesmobs/`): `mixin/EntityFrostmawMixin.java`, `mic_rtsim_mowziesmobs.mixins.json` in
  `resources`, the `[[mixins]] config = "${mod_id}.mixins.json"` line in
  `templates/META-INF/neoforge.mods.toml` (as ribbits),
  `gametest/FrostmawGameTests.java` (new), `gametest/MowzieKit.java` (`thief`).
- Docs: `docs/boundary.md` (MZB-288).

**Engine.**
- `Ev::Arrival { faction: u32, stamp: u32 }` dispatches to `camp_arrival`. Its wheel tag is 8.
- `plan_arrival(f, at)`:
  ```rust
  let Some(r) = self.culture(f).renewal else { return };
  let (away, cap) = (self.factions[f as usize].away, self.factions[f as usize].cap);
  let short = self.pool_at(f, at).floor() as u32 + away < cap;
  let day = (at.0 / TICKS_PER_DAY) as u32;
  let Some(camp) = self.camp_mut(f) else { return }; // no ledger: never refills
  match (short, camp.arrival_at) {
      (true, None) => {
          let t = at + r.rule.delay_ticks(camp.pressure, day).max(1);
          camp.arrival = camp.arrival.wrapping_add(1);
          camp.arrival_at = Some(t);
          let stamp = camp.arrival;
          self.schedule(t, Ev::Arrival { faction: f, stamp });
      }
      (false, Some(_)) => {
          camp.arrival = camp.arrival.wrapping_add(1);
          camp.arrival_at = None;
      }
      _ => {}
  }
  ```
- `camp_arrival` as MZB-256. `pool_add` keeps I2 because `camp_sync` follows it at the same tick.
- `renewal_bump(f, n, at)`: `camp.pressure.bump(day(at), n as f32 / cap as f32, r.rule.half_life_days)` and
  `stats.renewal_losses += n`. Called with the `LossCause` of MZB-258; the callers are MZB-259's.
- `camp_death`: compute `cause = renewal_cause(f, d.by)` once; unless the slot is Dead, `renewal_bump` for
  Player; then the state match as M4 has it; with `DEATH_CHIEF`, `chief_killed_at_camp` as MZB-265; then
  `unblock` (which runs `plan_arrival`).
- Chief paths: `chief_falls`, `step_down`, `chief_killed_by`, `chief_killed_at_camp` as MZB-262 to 264.
  `succeed` is unchanged. The away path still sets the breaker and the rescue news.
- `lose_units(w, ...)`: MZB-266. `raid_district` bumps the camp by the units its `lose_units` took
  (MZB-259); `raid_node` does not.
- CHIEF_STATE and the census: MZB-268 and 269. `faction_found` passes `ff.census` to `camp_found`.
- Persistence: MZB-271. `Pressure::from_parts` on load.

**Java.**
- `Camps.onDeath` uses `ConflictBridge.credit(dead, source)` for `by` and the killer's UUID (MZB-261).
  `ConflictBridge.onDeath` uses it for a chief puppet's CHIEF_KILLED only.
- `Camps.death` posts CHIEF_KILLED news for FLAG_CHIEF (MZB-275).
- `Camps.chiefState(LivingEntity, int)`, the queue and its flush (MZB-274); `RtsimApi.chiefState` calls it.
- The mixin (default priority 1000; no other mod targets this call in the pack):
  ```java
  @Mixin(EntityFrostmaw.class)
  abstract class EntityFrostmawMixin {
      @Inject(method = "tick()V", at = @At(value = "INVOKE",
              target = "Lcom/bobmowzie/mowziesmobs/server/entity/frostmaw/EntityFrostmaw;setHasCrystal(Z)V",
              shift = At.Shift.AFTER), allow = 1)
      private void mic_rtsim_mowziesmobs$stolen(CallbackInfo ci) {
          EntityFrostmaw self = (EntityFrostmaw) (Object) this;
          if (self.level().isClientSide()) return;
          if (RtsimApi.isCampBody(self)) self.setPersistenceRequired();
          RtsimApi.chiefState(self, 0);
      }
  }
  ```
  The config: `required` true, `minVersion` "0.8", package
  `com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.mixin`, `JAVA_21`, `injectors.defaultRequire` 1, no
  refmap (Mojang names at run time), as `mic_rtsim_ribbits.mixins.json`.
- Restriction and the held spawn: no change (MZB-279, MZB-280). UR-1 holds (MZB-278); D1 holds (one
  injection on a rare call, a queue keyed by faction, no per-tick work).

**GameTests to write** (`FrostmawGameTests`, `@GameTestHolder(MicRtsimMowziesMobs.MODID)`,
`@PrefixGameTestTemplate(false)`, the arena template, one batch each, each its own far base, as MZB-247;
stub engine first, then native):
1. `aRobbedFrostmawIsKeptAndAResetDoesNotGiveItsCrystalBack` (§7 test 2, batch `frostmaw_robbed`):
   - A test den, its crystal Frostmaw adopted (persistence false). A thief (MZB-281, survival, invisible)
     kept at the crystal's point each tick once the Frostmaw's `tickCount > 20`.
   - The thief holds an ice crystal, `getHasCrystal()` is false, `isPersistenceRequired()` is true, one
     CHIEF_STATE (aux 0, the den's faction, health within 1 of the Frostmaw's), the Frostmaw active. Remove
     the thief.
   - Release the den until its chunk is unloaded, then force it again: the Frostmaw that loads (same UUID,
     tagged, slot 0, generation 1) is persistent and crystal-less, and the CHIEF_STATE count is still 1.
     Census 2 has CHIEF without CRYSTAL, and no line restores a crystal.
   - A second den: a scripted SPAWN with `LINE_CHIEF` only is DONE with a crystal-less persistent Frostmaw.
     Amended at review C: that den is at `far.k + 6`, not `far.k + 4`, so that the first den's east column
     is not held loaded by the second den's forced chunk and its release unloads. The test also checks that
     census 1 lists the Frostmaw as `BODY_STRAY | BODY_CHIEF | BODY_CRYSTAL` and census 2 as `BODY_CHIEF`
     alone (the per-body crystal bit).
2. `anArrowAndATridentDoNothingAndABulletHurtsAFrostmaw` (§7 test 6, batch `frostmaw_arrows`): an untagged
   sleeping Frostmaw and a `FakePlayers.add` player. In this order (a hit that lands makes it invulnerable
   for 10 ticks): `hurt(arrow(new Arrow(level, player, new ItemStack(ARROW), null), player), 10)` is false,
   health unchanged, the target the player, active; `hurt(trident(new ThrownTrident(level, player,
   new ItemStack(TRIDENT)), player), 10)` is false, health unchanged; last
   `hurt(thrown(new Snowball(level, player), player), 10)` is true and the health falls (MZB-282).
3. `anInvisiblePlayerStealsTheCrystalFromASleepingFrostmaw` (§7 test 7, batch `frostmaw_thieves`): five
   untagged sleeping Frostmaws 40 blocks apart in forced chunks, a thief (MZB-281) at each crystal point:
   - invisible survival: crystal in its inventory, the Frostmaw crystal-less and active, not persistent
     (untagged, MZB-277), no CHIEF_STATE;
   - visible creative: steals;
   - invisible with all 36 main slots holding one dirt each: nothing, after 100 ticks;
   - visible survival: nothing;
   - spectator: nothing.
4. `aChiefKilledAtTheCampIsReportedWithTheKiller` (§7 test 15, Frostmaw half, batch `frostmaw_kill`):
   five dens with adopted Frostmaws and a `FakePlayers.hearing` player within 64 blocks of the first.
   - `MowzieKit.kill(f, playerAttack(p))`: CAMP_DEATH with aux BY_PLAYER, p's UUID, FLAG_CHIEF, slot 0,
     generation 1, the FROSTMAW body and an op id; a marker; the tag gone by the next tick; the body removed
     70 to 80 ticks after the death; `ConflictNews.posted()` up by 1; the Hearer's `raw` and `overlay` hold
     nothing from it.
   - A wolf (`setTame(true, true)`, owner p's UUID) kills: BY_PLAYER with p.
   - p hits for 10, and 40 ticks later `genericKill`: BY_PLAYER with p.
   - A zombie's `mobAttack`: BY_MOB, no killer.
   - `genericKill` with no hit before: BY_NONE.

**Rust tests** (`crates/rtsim-conflict/tests/frostmaw.rs`, on the shipped cultures unless named):
`a_den_killed_by_a_mob_comes_back_in_six_days`, `a_first_player_kill_makes_the_next_wait_twelve_days`,
`a_farmed_den_follows_the_closed_form`, `a_den_found_empty_waits_the_cap`,
`no_wait_while_the_frostmaw_is_out`, `an_adopted_stray_cancels_the_wait`,
`the_arrival_is_owed_and_spawned_at_the_next_census`, `the_arrival_at_a_den_that_is_in_sends_its_spawn`,
`chief_state_sets_the_crystal_and_the_census_corrects_it`, `faction_found_reads_the_crystal`,
`a_chief_killed_at_the_camp_succeeds_cows_and_routs_its_party`,
`only_the_killers_feud_steps_down_at_the_camp`,
`a_chief_body_left_at_home_killed_routs_its_party`, `a_dead_slots_body_is_no_second_chief`,
`a_chief_killed_away_bumps_the_pressure_unless_a_mob`, `a_boss_wiped_out_abstractly_loses_its_chief`,
`a_write_off_bumps_the_pressure`, `the_den_saves_its_pressure_and_wait` (blob and paged, the arrival on the
same tick after a round trip), `a_stolen_crystal_starts_no_wait`, `the_step_down_never_raises_a_feud` (a
None feud stays None). In `culture.rs`: the renewal parses and each validation error of MZB-251 is its own.
rtsim-jni's protocol test: CHIEF_STATE is counted live, not deferred, and a new `den(name)` helper drives a
CHIEF_STATE that flips the crystal and a census that corrects it.

**Sim runs** (`rtsim-cli sim mowzie`):
- **Run 6, the den** (RUNS row 6: built "M6, the den", rest "the grove M9"):
  - Killed BY_MOB: the next wait is 6.00 days. Killed by a player for the first time: 12.00.
  - Farmed (each arrival killed by a player 2 days after it comes): every wait equals the kernel's
    `delay_days` on the run's days, and the first five are within 3% of 12.0, 22.45, 36.4, 47.2 and 50.6
    (the whole-day closed form; P settles at 3.109, a wait of 51.76 and a kill every 53.76 days).
  - 60 game years farmed: arrivals per 96 days within 5% of 1.786 (about 108).
  - A den found empty: its migrant comes after 192.00 days.
  - A stolen crystal starts no wait, and the next arrival's line has `LINE_CRYSTAL`.
  - BY_TAKEN and BY_NONE bump; BY_MOB does not.
  - `stats.feud_wakes` grows by fewer than 10 over a wait with a busy line.
  - The camp audit holds after every event; a save and load mid-wait arrives on the same tick.
- **Run 7, the kills** (dispatched from `main`):
  - (a) The feud's player kills the chief on a raid: Raid steps down to Notice.
  - (b) A third player B kills it on A's raid: A's feud steps to Notice; B gets the Killing report, the heir
    a witness, and B's feud stays under Notice while the faction is cowed, then gets one Notice and decays
    (MOWZIE.md said about 2.5; built as about 62.5, MZB-299).
  - (c) A player C with no feud kills it at the camp: A unchanged, C not raised by the step-down, one Notice
    after the cowed window.
  - (d) The feud's player kills it at the camp: A steps down.
  - (e) A chief body Left at home is killed there: its party routs and the heir succeeds.
  - (f) An abstract wipe-out on a district: the chief falls with no killer, the party's feud steps down and
    the pressure is bumped.
  - (g) Run 1's tail: cowed, the heir, the body 12 days after a first kill, `feud_wakes`. RUNS row 1's rest
    becomes "the meter M7, ambushes M8" and its kill and body are run 7's.

**Work split** (in order; each step its tests green before the next):
0. Branch `feat/mowzie-m6` from the latest `feat/mowzie-m5`, merge `feat/mowzie-m4`, delete the
   run-gametest worlds (MZB-250).
1. `culture.rs`: `RenewalDef`, `Renewal`, resolve, validation, tests; `frostmaw.json`; the rate tests'
   culture (MZB-286).
2. `Camp` fields, `put_camp` and `get_camp`, RTC7 / META 6, `Ev::Arrival` and wheel tag 8 (MZB-271).
3. `plan_arrival`, `camp_arrival`, `camp_found` with the census byte; the Rust tests from
   `a_den_killed_by_a_mob_comes_back_in_six_days` to `the_arrival_at_a_den_that_is_in_sends_its_spawn`.
4. `renewal_cause`, `renewal_bump` and their callers (MZB-258, 259).
5. `chief_falls`, `step_down`, `chief_killed_by`, `chief_killed_at_camp`, `camp_death`'s chief arm,
   `lose_units`. **Opus review A**: the engine, steps 1 to 5.
6. CHIEF_STATE: `later`, `ingest`, `chief_state`, the census correction, rtsim-jni and its protocol test.
7. The synth fixes, runs 6 and 7, run 10's new den timings. **Opus review B**: the runs against MOWZIE.md's
   numbers.
8. Java core: `ConflictRecords`, `credit`, `Camps.onDeath`, `Camps.death`'s news, `Camps.chiefState` and its
   queue, `RtsimApi.chiefState`, `ConflictKit.chiefStates`.
9. The plugin's mixin, its config and the toml line; compile.
10. `FrostmawGameTests` 1 to 4 and `MowzieKit.thief`, on the stub, then native.
11. `boundary.md` (MZB-288) and, at landing, the fixtures (MZB-271). **Opus review C** before the GameTest
    round.

**What M6 changes in M5.** `Camps.onDeath` (credit), `Camps.death` (news), `Camps.tick` (the flush first), a
new `Camps.chiefState` and its queue, `ConflictBridge.credit` and `onDeath`'s chief arm,
`RtsimApi.chiefState`, `ConflictRecords.CHIEF_STATE`, `ConflictKit.chiefStates`; the plugin's mixin, config
and toml line. Not changed: the footprint, the restriction, the hold, `DenKind` (MZB-279, 280).

**Where the jar or the engine differs from MOWZIE.md.**
- §4.7 and test 7 speak of an invisible thief. A creative player steals as well, a spectator never, a thief
  with a full main inventory gets nothing, and only a sleeping Frostmaw outside an animation is robbed (so a
  puppet is robbed only asleep).
- The restriction also gates targeting (MZB-279), which §4.9 does not say.
- The Frostmaw's body goes 74 ticks after its death.
- M4's engine: `step_down` raised a None feud to Notice (MZB-262); an abstract boss wipe-out kept its chief
  (MZB-266); the synth returned a killed chief as a survivor (MZB-272); a den found empty would have waited
  6 days, not the cap (MZB-257).
- CHIEF_STATE is latest-wins, so a crash can lose it; the census corrects it (MZB-269).

**Traps.**
- `feat/mowzie-m5` has no M4 engine: build M6 on the merge.
- `git log -S'MZB-25'` matches MZB-25 itself as well as MZB-250 to 259; check this range with
  `-G'MZB-2[5-8][0-9]'`.
- A rate of 0 makes a ledger-less renewal faction never refill: M4's engine tests that build one need the
  rate culture of MZB-286.
- `DamageSources.thrown` takes the projectile first, despite its parameter names.
- A `FakePlayers.add` player is invisible to `level.getEntities`, so the jar's `getPlayersNearby` misses it;
  a `setInvisible(true)` on a ticking entity is undone at its first tick (MZB-281).
- `CampSource::Boss` is shared by both Mowzie cultures: use the renewal's `boss` (MZB-253).
- Pressure decays by whole days (`day.saturating_sub(at_day)`): compare waits with the whole-day form.
- Test 6 must strike with the snowball last: a hit that lands makes the Frostmaw invulnerable for 10 ticks.
- `isInventoryFull` ignores stack room: fill the 36 slots with one item each.
- (Review C) A count-0 chief stack ignores a synth fight's `kill_share`: the only body is the chief. A band
  wiped out in the synth stays in `parties` with no one alive, so assert `is_none_or`, not `is_none`.
- (Review C) A bare body id would couple body identity to the crystal's state in every place that goes by
  body id (MZB-277, amended).
- (Review C) Java's census byte comes from the chief body nearest the spot, which can be the leftover of a
  Dead slot; the per-body crystal bit closes that for the camp census (MZB-269), not for FACTION_FOUND.
- (Review C) A `FakePlayers.addSeen` player loads chunks round itself: remove it with `removeSeen` before
  releasing chunks. It is compiled only.

### The M5 merge review and the M5 C2ME diagnosis, fixed on M6

(MZB-653 and MZB-654 are the two reserved gaps in M5's range, MZB-655 above; MZB-660 to MZB-664 are M6's own.
Not run: no GameTest was run for these records; the Rust tests are, and the Java compiles.)

- **MZB-653** (M5 merge review, fixed on M6; P2): after a sim reset (RG5, RG6) an old life's tagged camp bodies
  loaded outside the footprint were listed ELSEWHERE by the census but never counted in `home`, which FACTION_FOUND
  sends and the engine sets the cap from (`camp_found`): a grove of 6 with 4 away came out at a cap of 2 for good,
  and the 4 were REMOVEd when they loaded (`camp_census` removes every tagged body no slot holds once the camp is at
  its cap). `Camps.census` now counts each tagged body loaded elsewhere in `home` (`home++` in the elsewhere loop),
  so the cap is the grove's size and they are adopted. No format change.
  - Tests: `a_grove_found_after_a_reset_with_bodies_away_adopts_them_all` (found 6, 2 in and 4 away, all tagged with
    generations no slot holds: cap 6, 6 ADOPT, no REMOVE, every body retagged) and
    `a_grove_found_short_of_its_bodies_away_removes_them_above_the_cap` (found 2 with the same 6: 2 ADOPT and
    4 REMOVE, the fault as it was, kept to show what the home count decides). GameTest
    `aGroveFoundAfterAResetCountsItsTaggedBodiesAwayAndAdoptsThemAll` (two tagged bodies in the footprint and one
    away in a chunk forced alone, made tagged before they join, in the step that declares the grove: FACTION_FOUND
    home 3, the census lists the away body ELSEWHERE, three ADOPTs retag them). Not run, so not shown failing; the
    check "home counts the body away: 3" reads 2 on the unfixed Camps.
  - What proves what: the Rust test founds the grove at 6 itself (the `home` the emulator is told), so it does not
    exercise the Java `home++`; its failure before the fix (12 ADOPT attempts for 6, none landing, the bodies
    keeping their old tags) is MZB-654's emulator change (ADOPT of an already-tagged body), not MZB-653's. The Java
    `home++` in `Camps.census` is proven only by the GameTest (home 3 against 2 on the unfixed Camps).
  - Not taken (a format change): an engine life on `CampMember`, so a tag from another life reads as no tag. The
    conservative count above needs no format and fixes the case reviewed; the life field would also make the
    census able to tell a reset's leftovers from a live camp's, and is for a release that has a format to keep.
- **MZB-654** (M5 merge review, fixed on M6; P3 x3):
  - `Camps.takeRecords` returns no records when the holder's life is not this engine's (`old.life() !=
    engineLife`): `carry` stamps the current life, and it was safe only because `ensureReasserted` ran first.
  - The synthetic plugin's ADOPT (`FakeConflict::apply_line`) matches Java's `Camps.adopt`: it takes any body of the
    camp by UUID, not only an untagged one, and retags it (Java unhooks the old tag and sets the slot's). A Rust
    test can now model a body tagged with a generation no slot holds. Sim hashes did not move:
    `scenario/results/sim-mowzie.json` regenerated at this commit is identical to the checked-in file (run 10's chaos
    and all), and the workspace tests with state hashes pass.
  - The census loop that makes each footprint holder journal its records (`ensureReasserted` before the census) was
    untested because the end-of-tick drain meets every holder that joined with records first. It is reached by a
    holder given its records after it joined: GameTest `theCensusDropsTheRecordsOfAnotherEngineLifeAFootprintBodyCarries`
    gives a fresh Frostmaw one record of the next engine life before the camp's first census and checks that the
    census drops it, counts it in `stalePending` once and journals nothing.
- **MZB-660** (M5 C2ME diagnosis, fixed on M6; P1, latent, not reproduced): under C2ME a footprint chunk can be
  HIDDEN while its entities are LOADED (C2ME's `processUnloads` drops an entity unload that fails, with no retry
  until a save; the pending entity load lands; the level is FULL again before C2ME's async upgrade makes the chunk
  TRACKED). `look` and `playable` accepted that on `areEntitiesLoaded` alone, the census `getEntities` saw no
  hidden entity, the engine ordered SPAWNs for bodies that were there, and the originals re-tracked with no join:
  a doubled body; a held Frostmaw at HOLD_MAX doubled the same way. Both now go through `Camps.entitiesIn`:
  entities loaded and the chunk's visibility in the entity manager `isAccessible()` (TRACKED or TICKING; not
  entity-ticking, which fails at level 33). `EntitySectionManagerAccessor` gains `@Accessor("chunkVisibility")`.
  - The state is forced in a GameTest without C2ME by `hide`: the entity manager's own `updateChunkStatus(pos,
    INACCESSIBLE)` leaves the chunk's entities loaded and out of every level query (the ticket level is
    unchanged), and the chunk is then taken out of the manager's `chunksToUnload` (new
    `@Accessor("chunksToUnload")`). The status change alone also queues the chunk, and vanilla's next
    `processUnloads` stores it (HIDDEN and LOADED), so the first version of the two tests collapsed one tick later
    and answered alike on fixed and unfixed builds; C2ME's real state differs from the hidden one only in not being
    queued. Each test checks `areEntitiesLoaded` at its answer step, so a periodic save (every 6,000 ticks) landing in
    the window fails with its own message, and its `onEnd` discards the body, so no reloaded body is left unless an autosave landed in the hide window (that
    run fails at the loaded check first, and the saved body can come back with the chunk).
    `aLineIsStaleWhileAFootprintChunkIsHiddenWithItsEntitiesLoaded` hides the den's chunk and sends a TAKE by tag:
    STALE, the Frostmaw stays; tracked again, the same line is DONE. The `playable` gate is what that test shows.
    Not run, so not shown failing; on the unfixed `playable` the TAKE is answered DONE through the index.
  - The stray radius (P2-b): `wideStrays` lists through `getEntities`, which sees visible sections only. A chunk
    inside the stray radius but outside the footprint can be hidden with its entities loaded; the stray's join set
    `strayWatch`, the census missed it and cleared the watch, nothing fired when the chunk showed, and the engine
    SPAWNed a second Frostmaw beside it. `look` now holds the census (`Camps.strayChunkHidden`) while the census
    would look wide (`strayWatch`, or a late camp's first) and any chunk of the radius has its entities loaded but is
    not accessible: a camp that is in stays in meanwhile, a camp not yet in waits as for its footprint. The hold
    ends when the chunk shows or unloads (a periodic save at the latest). A census that is held also delays a late
    camp's FACTION_FOUND until then, which is the safe side (a founding with the stray missing is the doubled
    Frostmaw). GameTest `aStrayInAHiddenChunkInsideTheRadiusIsListedOnceItsChunkShows`
    (a Frostmaw joins in the den's east chunk, 32 blocks out, hidden: no census for 30 ticks; shown, census 2 lists
    it as a stray chief). Not run; on the unfixed Camps the census 2 is made at once and lists no one.
  - The SPAWNs of the held census (re-check P2): the engine orders a SPAWN to a camp that is in without a census
    (`camp_arrival`, `camp_refill`), and a held SPAWN also plays past 160 blocks or at HOLD_MAX. With a recount
    waiting (a stray joined, or the census is held for a hidden chunk) the slot a SPAWN fills is the one the
    visible stray is about to fill: two Frostmaws. So while `c.recount` is set every SPAWN is held, past HOLD_MAX too
    (`spawnLine` and `playHeld`), and the census that ends the hold stales it (`staleHeld`); the engine orders
    again from the census. GameTest `aSpawnWaitsWhileACensusIsHeldForAHiddenChunkAndIsStaleAfterIt` (a den in, a
    visible stray in the footprint, the east chunk hidden, a SPAWN line: no second Frostmaw and no answer for 30
    ticks; the chunk shown, the SPAWN is STALE and census 2 lists the stray). Not run; on the unheld `spawnLine` the
    SPAWN plays at once and the den has two Frostmaws. Any recount, not only a held one, now holds SPAWNs until the
    census at the end of that tick.
  - The hold's bound (re-check P3): never a census past a hidden stray (a double body is worse than a late den), but
    `strayHold` keeps the age of the hold, and once it is HOLD_MAX ticks old runs the 81-chunk scan once every 100
    ticks instead of each tick. With saving off (`/save-off`: vanilla skips the entity manager's `autoSave`, the only
    thing that clears a C2ME-dropped unload) a late den waits for saving to resume.
  - The hold's reach (re-check P3): `strayChunkHidden` skips a chunk of the square of chunks round the radius whose nearest point to the
    spot is beyond the radius (the radius is a circle, the square its bounding box). Not taken: skipping the hold when
    the footprint's own strays already fill the free slots (`wideStrays` counts `freeSlots - strayInside`), since it
    needs the footprint scan on every held tick for a case that only delays the census.
- **MZB-661** (M5 C2ME diagnosis; P2): `byUuid` (REMOVE, TAKE and ADOPT by UUID) was `level.getEntity`, which
  misses a body whose chunk is hidden: NOTHING was answered and the stray lived. When `getEntity` misses it now
  falls back to the camp's own index (`live(faction)`) by UUID, with the level checked. GameTest
  `aRemoveByUuidFindsAnElsewhereBodyInAHiddenChunk` (an adopted warrior moved 80 blocks out, its chunk hidden by the
  entity manager and taken out of the unload queue, as in MZB-660: REMOVE is DONE and the body goes; the unfixed
  answer is NOTHING). Not run.
  - The untagged stray (P3): a REMOVE or ADOPT by UUID of a stray whose chunk went hidden after its census also
    answered NOTHING and left it alive (the engine's books were right). Each `Site` now keeps the strays its last
    census listed by UUID (`strayBodies`, rebuilt by each census, emptied when the camp goes out and when a session starts
    (`sessionStarted` sets the epoch to 0 without `out()`, so it clears the map itself), at most the census cap) and `byUuid` falls back to it (alive, same level). The same GameTest as the radius check ends with that
    REMOVE: DONE, the stray goes. Not run.
- **MZB-662** (P2, test): `aRemoveByUuidTakesAStrayAndAnElsewhereBodyIsCountedApart` waits on `w[0].isRemoved()`
  instead of `unloadedWithout` (`!hasChunk` plus `getEntity == null`, which under C2ME both flip before
  `setRemoved`). The failure message carries alive, the removal reason and whether the chunk is loaded, and says that
  alive in an unloaded chunk is C2ME's dropped entity unload, a case of its own. The ChunkWait conversion (step 3)
  replaces this wait with `ChunkWait.inMemory`. For that conversion: the converted wait (the `w[0]` wait near line
  2115) must still wait on the object's own `isRemoved()`, not only on `ChunkWait.inMemory`, or the C2ME dropped-unload
  case (the object alive in a chunk that is no longer loaded) comes back as a pass of the wrong condition.
  Amended after the slice C merge, for the two `mowziesmobs -Pc2me` failures on fd8eff01
  (`aHolderJournalsItsRecordAgainBeforeTheCampsNextCensus`, `aRobbedFrostmawIsKeptAndAResetDoesNotGiveItsCrystalBack`;
  plain `mowziesmobs` passed 39). Both trusted `getEntity == null` or `!hasChunk` as "saved and unloaded", and under
  C2ME a re-force inside the hide window tracks the same object again with no join. No product bug. The fix is one
  helper for all seven such waits:
  - `MowzieKit.removed(level, Supplier<Entity>...)` waits on each object's `isRemoved()`. `MowzieKit.unloadedWithout`
    (which keeps its `!hasChunk`) and both tests' `gone()` delegate to it. The callers keep the objects: the warriors
    and the Frostmaw as made, and the markers resolved with `getEntity` before the release.
  - The objects go as suppliers, read at each poll. A sequence's arguments are evaluated when it is built, before
    its `thenExecute` steps fill the arrays. The old waits got the UUIDs then, null, and `getEntity(null)` is null,
    so their entity checks passed at once.
  - A dropped unload (MZB-664) would make `isRemoved` wait for the next autosave, up to 6,000 ticks, past the robbed
    Frostmaw test's 3,000. When an object is still alive `REQUEUE_AFTER` (40) ticks into the wait, its chunk reads
    HIDDEN and the chunk is not in `chunksToUnload`, the helper adds it there through MZB-660's accessor (vanilla's
    retry: a chunk whose entities could not be stored yet stays queued) and logs a `[gametest]` warning naming the
    entity and chunk.
  - The robbed Frostmaw test's check after the reload is split in two: "the Frostmaw is back" (not null) and "a new
    object" (the old one was saved, not tracked again). `aHolderJournals...`'s stale marker check also asks for a new
    object.
  - Trap: under C2ME any test meaning "saved and loaded again" waits on the object's own `isRemoved()`, never on
    `getEntity` or `hasChunk`, and hands the wait the object through a supplier, not a value read when the sequence
    is built.
- **MZB-663** (P3, tests): the timings measure from what the camp sees. The poll test starts its out-clock at the
  first tick a footprint holder's level reads above FULL (or has no holder), not at the test's release; the held
  Frostmaw test bounds "placed" against the first tick `camps().heldLines() > 0`, not the test's send; the
  ticket-event-live test asserts NO_HOLDER only when a ticket event fired, else that every level reads above FULL and
  equals the holder's own current level (through `getUpdatingChunkIfPresent`, after the `norm` clamp), so a stale
  stored level fails. The comparison runs in a `thenWaitUntil` of its own, so a level still changing under C2ME is
  waited out instead of racing the poll.
  `gametest.sh mowziesmobs -Pc2me` runs once after the main round and its result is recorded there.
- **MZB-664** (P3, doc): `MowzieKit.acknowledge()`'s javadoc said a GameTest server never reaches the periodic
  save. It does, every 6,000 ticks (`MinecraftServer.tickServer`, `saveEverything`; the entity manager's
  autoSave then unloads every HIDDEN chunk, so C2ME's dropped unloads clear there), so the wording now says a run
  under 6,000 ticks never reaches it.

## M7 Rail

M7 is the rail meter, `set_emission_for`, a district-free focus and the train sampler (MOWZIE.md §9: ledgers 1
and 17; tests 10 and 11; runs 1 without its ambushes, 2, 3, 4 and 9; it needs M6). Engine line numbers below
are at 761f5d0f; jar facts are in "M7 design for the implementer".

- **MZB-300** (the branch): `feat/mowzie-m7` from the latest `feat/mowzie-m6`, once M6's engine and Java
  are both in it. After the page format change (MZB-302), delete `neoforge/*/run-gametest/world` before
  the first GameTest run.
- **MZB-301** (the signal kind): `SignalKind::Rail = 6`, appended. `COUNT` becomes 7, `ALL` gains it,
  `name` gives "rail" and `from_name("rail")` reads it. `grievance.rs`'s `K` follows `COUNT`.
  - `frostmaw.json` gains `"rail": {"weight": 1.0, "tolerance_per_day": 2.0}` (PLACEHOLDER, §4.3).
  - `umvuthana.json` gets no rail entry, so `Culture::resolve` gives it weight 0.
  - The TERRITORY mask sets a bit for each kind of weight above 0 (`territory`, engine.rs:1148), so bit 6
    is set for Frostmaw dens only and Java samples only for them.
- **MZB-302** (the page format): RTC8 and META 7 (`persist.rs` `MAGIC` and `META_VERSION`). If M9 bumped
  first (its MZB-437 shares RTC7 with M6), take one past whatever the base holds at landing. The fixtures
  are regenerated; there is no migration.
- **MZB-303** (where the meter lives): in rtsim-conflict, a new `src/engine/rail.rs` (an `impl Conflict`
  block, as `camp.rs`), not in the composition as ledger 1 says. The meter needs the conflict's wheel, its
  pages, `presence` and the replay of journaled records; the composition has none of them.
- **MZB-304** (the meter): `FactionRow` gains `rail: Vec<RailMeter>`, sorted by actor, one per actor.
  - `RailMeter { actor: ActorId, n: f64, t_ref: Tick, pushed: f64, pushed_at: Option<Tick>, stamp: u32,
    last: Option<(Pos, i32)>, lines: Vec<LineCount> }`. `last` is the latest pass's point and y, the
    source's position while the meter has no line.
  - τ is 2 days (48,000 ticks, PLACEHOLDER, §4.3). The count decays as `n(t) = n e^(-(t - t_ref)/τ)`;
    the rate is `n(t) × 24,000 / τ` legs a day.
  - Adds are exact. A pass whose exit t is at or after `t_ref` sets `n = n(t) + 1` and `t_ref = t`. An
    earlier one adds `e^(-(t_ref - t)/τ)` to n. A late pass therefore counts as if it had come on time.
- **MZB-305** (the push gate): tol is the culture's rail tolerance (2 legs a day), r the rate now and p the
  rate last pushed (`pushed`).
  - No push when `r < tol && p < tol`.
  - Otherwise push when `|r - p| >= max(0.1p, 1.0)`, or when a day has passed since `pushed_at` (the daily
    resync, ledger 1).
  - A push calls `set_emission_for` (MZB-313) with r and sets `pushed = r`, `pushed_at = at`.
    `stats.rail_pushes`.
  - The gate runs after each counted pass and at each wake.
- **MZB-306** (the wake): one `Ev::Rail { faction: u32, actor: ActorId, stamp: u32 }` per meter, on wheel
  tag 10 (M6's Arrival is 8 and M9's CampHeir 9). Booking bumps `stamp`, so a wake with another stamp is
  stale (`stats.stale`).
  - The wake is the earlier of the band's lower edge and `pushed_at + 1 day`. The lower edge is
    `t_ref + ceil(τ ln(n / n_lo))`, with `n_lo` the count at rate `p - max(0.1p, 1)`.
  - The upper edge needs no wake: only a pass raises the rate, and the gate runs on each pass.
  - When `r < tol && p < tol`, the wake is retirement instead: the tick at which r falls to 0.01 legs a day.
    Retirement pushes 0 if p is above 0 and drops the meter with its lines.
- **MZB-307** (lines): `LineCount { a: u64, b: u64, n: f64, t_ref: Tick, point: Pos, y: i32, near:
  Option<(Pos, DistrictId)> }`, with `a < b` the two station ids. They are kept per meter, at most 16
  (PLACEHOLDER); a 17th line evicts the one with the smallest decayed count.
  - The decay and the exact adds are MZB-304's.
  - `point` and `y` are the latest credited pass's point, so a reroute that still crosses the circle moves
    the focus with it (run 3).
  - `near` is whichever of the two stations is nearer the point and has a district. It is none when
    neither has one.
  - The busiest line (largest decayed count) gives the source's position and pin. With no line, `last`
    gives the position and the pin has no station.
- **MZB-308** (held passes): a pass waits for its train's next arrival to name its line.
  - `Conflict` gains resident `held: BTreeMap<u64, Vec<Held>>` by train and `held_oldest:
    BTreeSet<(Tick, u64)>`, with `Held { faction: u32, actor: Option<ActorId>, exit: Tick, point: Pos, y:
    i32 }`.
  - The caps are 64 per train (the train's oldest pass goes first) and 4,096 in all (the oldest pass
    overall goes first).
  - `held` is written at the end of `put_resident` and read back in that order; `held_oldest` is rebuilt
    at load.
  - A pass held for 1 day (PLACEHOLDER, ledger 1) is let go. This is lazy: `let_go_due(at)` runs first in
    `rail_pass`, `rail_call` and `rail_wake`. If none of them runs again, the pass is never let go, which
    can only leave an ownerless pass uncounted.
- **MZB-309** (letting go): a held pass with an owner was counted at ingest, so letting it go only drops it.
  - A pass with no owner is counted then, at its exit, against the top of `presence.top_of` its point's
    chunk.
  - If nobody is present there, it counts against nobody. It never counts against the settlers: a train
    nobody owns, through land nobody holds, is no one's grievance.
  - `stats.rail_let_go` counts both.
- **MZB-310** (an ownerless pass credited to a line): its actor is the presence top over both stations'
  chunks together, else nobody. `owner_of`'s settlers fallback (engine.rs:1522) is not used for rail.
- **MZB-311** (the call): `pub fn rail_call(&mut self, w: &mut World, call: &RailCall, at: Tick)`, with
  `RailCall { train: u64, line: Option<(StationEnd, StationEnd)> }` and `StationEnd { id: u64, dim: u32,
  x: i32, z: i32, district: Option<u64> }`.
  - With a line, each held pass of the train is credited. An ownerless pass gets its actor (MZB-310) and is
    counted. The meter's line `(min id, max id)` gets the add, the point, the y and `near`. Then the gate
    runs.
  - `line: None` lets the train's passes go now. It is sent for an unwatched stop, an arrival with no
    departure booked, and a self-edge (both ends the same station).
  - `pub fn holds_train(&self, train: u64) -> bool` lets rtsim-jni skip the call. `stats.rail_calls`.
- **MZB-312** (rtsim-jni): in `engine/industry.rs`'s `journaled`, for TrainArrived and TrainUnwatched:
  - Before `l.ingest`, capture the departure, since the arrival clears `last_departure`. This uses a new
    `IndustryEngine::last_departure_of(TrainId) -> Option<StationId>`, plus the two `StationDecl`s from a
    new `station_decl(StationId) -> Option<&StationDecl>`.
  - After it, when the conflict is enabled and running and `holds_train` is true, call a new
    `conflict::rail_call(st, rec, call)`. It does `shared::catch_up`, `phase1::world_tick`, `with_infra`,
    `c.rail_call` and then `take_commands`, as `run_engine` does (conflict.rs:633).
  - TrainUnwatched always sends `line: None`. With the industry off no call ever comes, and every pass is
    let go after its day.
- **MZB-313** (`set_emission_for`): `pub fn set_emission_for(&mut self, w: &mut World, f: u32, actor:
  ActorId, per_day: f64, pos: Pos, pin: Pin, at: Tick)`.
  - The key is `fnv1a64("rail" ++ faction key ++ actor bits)` and the kind is Rail. Ledger 1's `key` and
    `kind` parameters are dropped, since the faction and actor fix both.
  - `Source` gains `pin: Option<Box<Pin>>`, with `Pin { y: i32, station: Option<(Pos, DistrictId)> }`. It
    is boxed so every other source carries only one pointer.
  - The source has `affects = [(f, 1.0)]`, `district: None`, a fixed `actor`, no chunks and no places. So
    `reattribute` (1641) never moves it, and `add_faction`'s pickup (1085-1123) skips a source with a pin.
  - It is its own function beside `set_emission_inner` (1541), not a branch of it. It inserts or updates
    the source, calls `add_reach(Rail, id)` on f's row the first time, and calls `refresh_rates(f, actor,
    Rail, at)`. A rate of 0 removes the source and its reach.
  - **Amended by the lead for MZB-331's gate** (MZB-347): a world with no rail pays what base paid.
    The Rail kind keeps no slot in the per-feud arrays. `Grievance.level` and `rate` and `Feud.raw` are
    six f64s again (`KB = K - 1`, the kinds before Rail); the Rail kind's level, rate and raw rate are
    one `Option<Box<RailTail>>` on the grievance, which a feud has only while one of the three is
    non-zero (`level_of`, `rate_of`, `Feud::raw_of`, `Feud::set_raw` read and write either side, and
    `project`, `commit`, `add`, `scale`, `relieve`, `set_rate` treat the tail as one more kind, so every
    number is bit-identical to the seven-array form). `FactionRow.rail` is `Option<Box<Vec<RailMeter>>>`,
    `None` while there is no meter (read it through `rail()`; clippy's `box_collection` is allowed there
    on purpose: the box makes it one pointer, not a Vec's three words). `Source.pin` stays boxed.
  - Encoding (RTC8 and META 7 stay, since they are M7's own): the feud's last byte is a flags byte (bit 0
    the envoy hold, bit 1 the tail follows with its level, rate and raw rate), a source's district tag is
    a flags byte (bit 0 a district, bit 1 a pin that ends the source), a faction row ends with its meter
    count (one byte when none). A feud, a focus and a source with no rail encode to 176, 197, 32 and 40
    bytes, the same as at f6321fa5 (a test pins them).
- **MZB-314** (the district-free focus): `Focus` gains `track: Option<TrackFocus>`, with `TrackFocus { y:
  i32, station: Option<(Pos, DistrictId)> }`. It is Copy, so `Focus` stays Copy.
  - `refresh_rates` (1683-1720) sets no focus from a kind whose weight is 0 for the faction.
  - Otherwise a best source sets the focus when its class is at least the current focus's. The classes,
    highest first, are a district, a track (a source with a pin) and a bare position.
  - So a district focus is never displaced by a pass through the wild, as ledger 1 says, and a track focus
    is not displaced by a kill site, which would raid nothing.
  - Within one class the latest refresh wins, as districts do today. A rail source's focus copies its pin
    into `track`. The signal path's focus (1453-1461) gets `track: None`.
  - **Amended by the lead for MZB-331's gate** (MZB-347): `Focus.track` is `Option<Box<TrackFocus>>`,
    so `Focus` is 48 bytes, base's 40 plus one pointer (an inline `Option<Pin>` made it 80), and `Focus`
    is not `Copy`. What losing `Copy` cost: `escalate` and `envoy_again` clone the focus (one allocation
    only when it has a track; `target_of` takes `Option<&Focus>`), `refresh_rates` builds the focus once
    after its loop from a tuple of the best source's fields (it allocates the box only for the focus it
    keeps), and a few readers (`rtsim-layers`, tests) use `as_ref()`. The class rule is unchanged. The
    encoded form is the feud's focus tag: 0 none, 1 a focus with no track, 2 a focus with its pin written
    last, so a world with no rail pays no byte for the track.
- **MZB-315** (the target): `Target::Track { track: Pos, spot: Pos, y: i32 }`, persisted as tag 3
  (`put_target`, persist.rs:595).
  - `target_of` gains the party kind: `fn target_of(&self, w: &World, f: u32, kind: PartyKind, focus:
    Option<Focus>, at: Tick) -> Target`. Its two callers (`escalate` 1961, `envoy_again` 2135) pass the
    kind they send.
  - A focus with a track gives a Raid a Track target (spot from MZB-316). An Invasion gets
    `District(station's district, station pos)` when the pin names a station, else Track. Any other kind
    gets `Pos(focus.pos)`. The other arms are unchanged.
  - `target_pos(Track)` is the spot. The matches over `Target` in `resolve` and its `what` (3406-3429) and
    in `rescue_news` (4259) gain a Track arm, taken as `Pos` is (bases near the aim).
- **MZB-316** (the spot): RAIL_PASS gives one point and no bearing.
  - So the spot is 5 blocks (PLACEHOLDER, §4.4) from the track point toward the den (the faction's pos),
    rounded to whole blocks and put through `canon`.
  - The point is the pass's sample nearest the den. On a straight track the den direction is
    perpendicular to the track up to the sampling error. Samples 20 ticks apart at 2 blocks a tick are 40
    blocks apart, so the nearest sample is at most 20 blocks along from the perpendicular's foot: about
    7.6 degrees off at 150 blocks.
  - A den within 1 block of the point puts the spot +x.
- **MZB-317** (arriving at a Track): `arrive` (3256) runs its peaceful arm, then for a Track target waits
  until `bored_at` (`wait`, 3328). There is no watcher fight and no `resolve`.
  - The on-the-way ambush roll (3003-3041, `trains_crossed`) is skipped for a party whose target is Track:
    its trains come from sampling (ledger 8).
  - A forced `resolve(Track)` notes "raids nothing" and never looks for a district.
  - In M7 a Raid at a track waits and goes home; M8 makes it lie in wait.
- **MZB-318** (counters): `rail_passes`, `rail_pushes`, `rail_wakes`, `rail_let_go`, `rail_unknown` and
  `rail_calls`, appended to `stats_fields` and the stats page.
- **MZB-319** (RAIL_PASS ingest): RAIL_PASS leaves `Conflict::later` (4911) for `rail_pass(w, rp, at)`.
  - A pass for an unknown faction, or one whose rail weight is 0 or less, counts in `rail_unknown` and is
    dropped.
  - pos is `canon(dim, floor x, floor z)` and y is `floor y`. The owner maps through `player()`; a zero
    UUID means no owner. An exit after `at` is clamped to `at`.
  - An owned pass is counted at once (MZB-304) and held (MZB-308). An ownerless pass is only held.
    `stats.rail_passes`.
  - rtsim-jni's `translate` (conflict.rs:500) converts entry and exit from game time with
    `phase1::world_tick`, as for the other timed records.
- **MZB-320** (TrainSampler): `neoforge/create/.../TrainSampler.java`, registered in `MicRtsimCreate` on
  `ServerTickEvent.Post` whether or not the industry is on, since the meter is the conflict's. It returns
  at once unless the conflict is up and `Territories.anyMinding(6)` is true (MZB-324).
- **MZB-321** (the cadence): once a second (game time % 20 == 0) it walks `Create.RAILWAYS.trains` and puts
  each train in bucket `CreateIds.of(id) mod 20`. Each tick it samples one bucket. So each train is sampled
  every 20 ticks: 10 trains a tick at 200.
- **MZB-322** (a sample):
  - The point is `Vec3 at = train.carriages.get(0).getLeadingPoint().getPosition(train.graph)`. The
    dimension is that point's `node1.getLocation().getDimension()`.
  - The train is skipped when `graph == null`, the point's `edge == null`, `derailed`, `invalid`, or
    `edge.isInterDimensional()`.
  - A segment whose dimension changed, or one longer than 128 blocks (a portal, a wrapped world's seam, a
    teleport), is not tested; the sample starts a fresh segment. `Territories` has no wrap handling, and
    this keeps a seam out of it.
- **MZB-323** (passes): the segment from the train's last sample to this one is tested with
  `Territories.crossing(dim, x0, z0, x1, z1, 6, out)` (MZB-324).
  - A territory the segment meets opens a pass if the train has none open there. Its entry is the game
    time of the segment's first sample.
  - An open pass keeps the sample nearest the circle's centre, and its y. Only sample points are kept, so
    the point is on the track. For a segment that clips the circle, the nearer of its two ends is kept.
  - After each segment, every open pass whose circle does not contain the new sample is closed with exit
    = now and sent.
  - A train gone from `trains` closes its passes at its last sample.
- **MZB-324** (Territories, core): `crossing(ResourceKey<Level> dim, double x0, double z0, double x1,
  double z1, int signal, List<Territory> out)` gives each territory whose circle the segment meets and
  whose mask has the signal, once. It looks in the cells of the segment's box (`CELL` 512, so at most 4)
  and in the wide list. `anyMinding(int signal)` reads a count per bit, kept by `put`, `remove` and `clear`.
- **MZB-325** (RAIL_PASS out):
  - `RtsimApi.railPass(long faction, long train, UUID owner, ResourceKey<Level> dimension, double x,
    double y, double z, long entry, long exit)` calls `ConflictBridge.railPass`. That writes the record
    with `beginJournaled`, as `flushSignals` (2036) does, gated by `m1Records`.
  - The train id is `CreateIds.of(train.id)` (H19). The owner is `train.owner`, or the zero UUID when it is
    null.
  - The sampler's state (the last sample and open passes per train) is dropped at session end and at
    server stop. A pass open then is lost.
- **MZB-326** (UR-1): the sampler and the meter keep nothing in the world: no saved data, attachments or
  tags. Without the plugin no RAIL_PASS comes, and every meter decays, pushes down and retires through its
  own wakes.
- **MZB-327** (test 11's curves): code-built track of radius 100: a 90 degree curve, and an S-bend of two.
  The fixture's 90 degree curve (radius 9.5, `RailFixtureGameTests`) is 15 blocks of track. That is shorter
  than one sample's travel at 1.4 blocks a tick (28 blocks), so it cannot tell the track from the chord.
- **MZB-328** (test 10): a real assembled train, since contraption collision needs the carriage entity. A
  zombie on the rail is the control that is pushed.
- **MZB-329** (the build): the create project gains `-PwithMowzie`, as dh's `-PfarIceAndFire`
  (dh/build.gradle:12-16, 126-130, 143-144).
  - With it, the project does `evaluationDependsOn(':mowziesmobs')`, and the GameTest run gets a `mic_rtsim_mowziesmobs` mods entry (`project(':mowziesmobs')`'s main
    source set), `runtimeOnly project(':mowziesmobs')`, the Mowzie jars at run time and the Mowzie config
    copy.
  - The create project's tests name Mowzie only by registry id (`mowziesmobs:frostmaw`) and NBT, so the
    project compiles without the jar.
  - The Frostmaw tests' class (`RailFrostmawGameTests`) has no `@GameTestHolder`. It is registered from
    `RegisterGameTestsEvent` only when `ModList.get().isLoaded("mowziesmobs")`.
- **MZB-330** (sim tolerances): 8 seeds, each train's headway jittered uniformly by ±0.4 of itself, judged
  on medians.
  - A settled grievance is within 2%, a band crossing within ±0.6 day, and the pushes in 20 days within ±4.
  - The runs use a chief seed `MIDDLING` whose `Temperament` is (128, 128, 128). It is found by a search
    over seeds up to 2^28, and its own test asserts it. If none exists there, take traits within ±1 (a bend
    within 0.4%) and widen the crossing tolerance by 0.1 day.
- **MZB-331** (bench): `rtsim-cli bench scale --only factions,factions-paged`, an A/A pair first, then base
  and M7 side by side. A regression above 3% beyond the A/A spread blocks landing.
- **MZB-332** (no bearing in RAIL_PASS): RAIL_PASS's reserved u32 at 60 could carry the track's bearing at
  the point, which would let the spot stand exactly perpendicular. It stays reserved: MZB-316's den
  direction is within 8 degrees, the record is M1's, and a format change is Ben's to make. This is the one
  data-format candidate, settled conservatively, so M7 has no open question for Ben.
- **MZB-333** (boundary.md): RAIL_PASS's row says its entry and exit are game time, that it is journaled,
  and that its point is a sample (on the track). The "come later" list (boundary.md:1752) loses RAIL_PASS.

### M7 design for the implementer

**Where it stands.** Design only, on `design/mowzie-m7` (from 761f5d0f). Nothing of M7 is written. Open
questions for Ben: none. The one candidate, a bearing in RAIL_PASS, is settled by MZB-332.

**Jar facts** (Create 6.0.10 with cfr in `target/m7-cfr/create` and `target/m7-cfr/create-extra`; Mowzie's
Mobs 1.8.2 in `target/m7-cfr/mowzie`, `EntityMagicEffect` in `target/m7-cfr-magic/out`; vanilla from the
NeoForm sources jar in `target/m7-vanilla`):
- `GlobalRailwayManager`: `public Map<UUID, Train> trains` (57). `tick(Level level)` (208) returns unless
  `level.dimension() == Level.OVERWORLD`, so every train moves once a server tick, in the overworld's
  tick. `ServerTickEvent.Post` sees them after it.
- `Train`: `public UUID id` (157), `public TrackGraph graph` (160), `public boolean invalid` (167), `public
  List<Carriage> carriages` (173), `public boolean derailed` (182), `public UUID owner` (159), `public void
  arriveAt(GlobalStation station)` (871), `public void leaveStation()` (863).
- `Carriage.getLeadingPoint()` (384) and `getTrailingPoint()` (388) return `TravellingPoint`.
  `TravellingPoint` has `public TrackNode node1` (46), `public TrackEdge edge` (48), `public double
  position` (49), and `public Vec3 getPosition(@Nullable TrackGraph trackGraph)` (302). The last is
  `edge.getPosition(graph, t)` plus the edge's normal, so it reads the real curve.
- `TrackEdge`: `public boolean isInterDimensional()` (55), `public Vec3 getPosition(@Nullable TrackGraph
  trackGraph, double t)` (97), `TrackEdge(TrackNode node1, TrackNode node2, BezierConnection turn,
  TrackMaterial trackMaterial)` (38). `TrackNode.getLocation()` (23) gives a `TrackNodeLocation`, whose
  `getDimension()` (73) is a `ResourceKey<Level>`.
- `BezierConnection(Couple<BlockPos> positions, Couple<Vec3> starts, Couple<Vec3> axes, Couple<Vec3>
  normals, boolean primary, boolean girder, TrackMaterial material)` (create-extra, 103) builds test 11's
  curves in code.
- `GlobalStation` inherits `getBlockEntityPos()` and `getBlockEntityDimension()`; `TrainWatch` reads both
  (149, 229).
- Collision: `AbstractContraptionEntity.canCollideWith(Entity e)` (822-852) refuses spectators,
  `noPhysics`, hanging entities, glue, seats, projectiles and passengers, and ends `return
  e.getPistonPushReaction() == PushReaction.NORMAL` (851). `ContraptionCollider` (142) collides only
  entities it accepts. `EntityFrostmaw.getPistonPushReaction()` returns `PushReaction.IGNORE` (332), so a
  train passes through a Frostmaw (§3.1), and test 10 guards it.
- `AbstractContraptionEntity.isPickable()` returns false (798) and `hurt(DamageSource, float)` returns
  false (802). It is no `LivingEntity` (this matters to M8).

**Layout.**
- Engine:
  - `crates/rtsim-conflict/src/culture.rs` (MZB-301) and `data/cultures/frostmaw.json`.
  - `src/engine/rail.rs` (new: `RailMeter`, `LineCount`, `Held`, `Pin`, `TrackFocus`, `RailCall`,
    `StationEnd`, `rail_pass`, `rail_call`, `holds_train`, `let_go_due`, `rail_gate`, `rail_wake`,
    `set_emission_for`, `spot_toward`).
  - `src/engine.rs` (`Source.pin`, `Focus.track`, `Target::Track`, `Ev::Rail` and its dispatch,
    `refresh_rates`, `add_faction`'s pickup, `target_of`, `target_pos`, `arrive`, the on-the-way roll,
    `resolve`, `rescue_news`, `later`, `ingest`, stats).
  - `src/tables.rs` (`FactionRow.rail`) and `src/persist.rs` (MZB-302).
  - `crates/rtsim-industry/src/engine.rs` (`last_departure_of`, `station_decl`).
  - `crates/rtsim-jni/src/engine/industry.rs` and `engine/conflict.rs` (MZB-312, 319) and its protocol test.
  - `crates/rtsim-cli/src/sim/mowzie.rs` and its synth (the rail lines, runs 1 to 4 and 9).
  - `crates/rtsim-conflict/tests/rail.rs` (new).
- Core Java (`neoforge/core/.../mic_rtsim/`): `conflict/Territories.java` (`crossing`, `anyMinding`),
  `conflict/ConflictBridge.java` (`railPass`), `conflict/ConflictRecords.java` (RAIL_PASS offsets if
  missing), `api/RtsimApi.java` (`railPass`), `gametest/ConflictKit.java` (`railPasses(faction)`).
- Create plugin (`neoforge/create/`):
  - `TrainSampler.java` (new) and `MicRtsimCreate.java` (its registration).
  - `bench/SamplerBench.java` (new).
  - `gametest/TrainSamplerGameTests.java` (new, test 11) and `gametest/RailFrostmawGameTests.java` (new,
    test 10, then M8's 8 and 9).
  - `build.gradle` (MZB-329).
- Docs: `docs/boundary.md` (MZB-333).

**Engine.**
- The meter (`rail.rs`):
  ```rust
  pub(crate) const RAIL_TAU: f64 = 48_000.0; // ticks (PLACEHOLDER, §4.3)
  const RETIRE_PER_DAY: f64 = 0.01;

  impl RailMeter {
      fn n_at(&self, t: Tick) -> f64 {
          self.n * (-(t.0.saturating_sub(self.t_ref.0) as f64) / RAIL_TAU).exp()
      }
      fn add(&mut self, t: Tick) {
          if t >= self.t_ref {
              self.n = self.n_at(t) + 1.0;
              self.t_ref = t;
          } else {
              self.n += (-((self.t_ref.0 - t.0) as f64) / RAIL_TAU).exp();
          }
      }
      fn per_day(&self, t: Tick) -> f64 {
          self.n_at(t) * TICKS_PER_DAY as f64 / RAIL_TAU
      }
      /// The first tick at which the rate is at or below `r` legs a day.
      fn falls_to(&self, r: f64) -> Tick {
          let n_lo = r * RAIL_TAU / TICKS_PER_DAY as f64;
          if self.n <= n_lo {
              return self.t_ref;
          }
          self.t_ref + (RAIL_TAU * (self.n / n_lo).ln()).ceil() as u64
      }
  }
  ```
  `LineCount` has the same `n_at`, `add` and `per_day`.
- The gate, after an add or at a wake:
  ```rust
  fn rail_gate(&mut self, w: &mut World, f: u32, actor: ActorId, at: Tick) {
      let tol = self.culture(f).signals[SignalKind::Rail.index()].tolerance_per_day;
      let Some(m) = self.factions.row_mut(f).rail_meter_mut(actor) else { return };
      let (r, p) = (m.per_day(at), m.pushed);
      let quiet = r < tol && p < tol;
      if quiet && r <= RETIRE_PER_DAY {
          // retire: push 0 if p > 0, then remove the meter (and its lines)
          return;
      }
      let day_due = m.pushed_at.is_none_or(|t| at >= t + TICKS_PER_DAY);
      if !quiet && ((r - p).abs() >= (0.1 * p).max(1.0) || day_due) {
          // push r: set_emission_for(w, f, actor, r, pos, pin, at) from the busiest line, or `last`
      }
      // book: quiet -> falls_to(RETIRE_PER_DAY); else the earlier of
      // falls_to(p - max(0.1p, 1)) (only when that is above 0) and pushed_at + 1 day
      // (stamp += 1; schedule Ev::Rail { faction: f, actor, stamp })
  }
  ```
  Borrowing: read what the push needs into locals, drop the row borrow, then call `set_emission_for`.
- `rail_pass(w, rp, at)`: `let_go_due(at)`. Then the faction by key and the weight check (MZB-319). Find
  or insert the meter for an owned pass, `add(exit)`, set `last`, and run the gate. Then hold it (MZB-308).
- `rail_call(w, call, at)`: `let_go_due(at)`. Take the train's holds. With a line, credit each (MZB-311),
  then run the gate once per touched meter in actor order. With `None`, run MZB-309 on each.
- `rail_wake(w, f, actor, stamp, at)`: `let_go_due(at)`. A stale stamp counts in `stats.stale`; otherwise
  `stats.rail_wakes` and the gate.
- `set_emission_for` as MZB-313. `Source.rate` is units per tick, so it stores `per_day / TICKS_PER_DAY`.
  `refresh_rates` subtracts the tolerance once, from this one rate per actor, as for any kind.
- `refresh_rates`'s last lines become:
  ```rust
  if let Some(b) = best
      && sd.weight > 0.0
      && class(&b) >= feud.focus.as_ref().map_or(0, class)
  {
      feud.focus = Some(b);
  }
  // class: 2 for a district, 1 for a track, 0 for a bare position.
  ```
  The best's `track` is `s.pin.as_ref().map(|p| TrackFocus { y: p.y, station: p.station })`.
- `target_of`:
  ```rust
  fn target_of(&self, w: &World, f: u32, kind: PartyKind, focus: Option<Focus>, at: Tick) -> Target {
      match focus {
          Some(Focus { district: Some(d), pos, .. }) => Target::District(d, pos),
          Some(Focus { track: Some(t), pos, .. }) => match (kind, t.station) {
              (PartyKind::Invasion, Some((sp, d))) => Target::District(d, sp),
              (PartyKind::Raid | PartyKind::Invasion, _) => Target::Track {
                  track: pos,
                  spot: self.spot_toward(w, pos, self.factions[f as usize].pos),
                  y: t.y,
              },
              _ => Target::Pos(pos),
          },
          Some(fo) => Target::Pos(fo.pos),
          None => Target::Pos(self.factions[f as usize].pos),
      }
  }
  ```
  `spot_toward` works in the frame's chart (the topology's unwrapped offset, as `party_pos` routes), so a
  den across a seam still gives the right side, then `canon`s the result.
- Persistence (MZB-302):
  - `put_focus` adds a flag byte, then y and the station, for `track`.
  - `put_target` tag 3 writes track, spot and y.
  - `put_source` adds a flag byte, then the pin.
  - The FactionRow page writes `rail` (meters in actor order, each with its lines).
  - `put_ev` tag 10 is Rail.
  - `put_resident` ends with `held` (train ids in order, each with its holds).
  - The blob and paged paths share these, as M6's `put_camp` does.

**Java.**
- `TrainSampler` (create plugin), state per train UUID: `Sample { dim, x, y, z, tick }` and `List<Open>`,
  with `Open { territory, nearest (x, y, z, d2), entry }`. Per tick:
  ```java
  public static void tick(ServerTickEvent.Post e) {
      if (!RtsimApi.conflictUp() || !RtsimApi.territories().anyMinding(SIGNAL_RAIL)) return;
      long now = e.getServer().overworld().getGameTime();
      if (now % 20 == 0) rebucket();            // Create.RAILWAYS.trains, CreateIds.of(id) mod 20
      for (Train t : buckets[(int) (now % 20)]) sample(t, now);
  }
  ```
  `rebucket` also closes the passes of trains no longer in `trains` and drops their state. `RtsimApi`
  gains `conflictUp()` if it has no such test (the bridge's session is up and `m1Records` is true).
- `Territories.crossing` and `anyMinding` as MZB-324. The segment-circle test is the distance from the
  centre to the segment's nearest point, at most the radius.
- `RtsimApi.railPass(...)` and `ConflictBridge.railPass(...)` as MZB-325. Offsets are from boundary.md:1511:
  faction 24, train 32, owner 40 and 48, dimension 56 (the bridge's `EngineNames` id), reserved 60, x y z
  64 to 80, entry 88, exit 96. The size is 104.
- `SamplerBench` (`-Dmic_rtsim.samplerbench=true`, with `-Dmic_rtsim.farbench=true` for its 200 trains):
  it scripts 64 territories along the bench's routes and logs, every 1,200 ticks, `[samplerbench]` with
  trains sampled and ns per sample (the median and the 99th percentile over the window). The budget is 2
  µs a sample (PLACEHOLDER), or 20 µs a tick at 200 trains.
- D1: one bucket a tick, at most 4 cell lookups and a short list per sample, no allocation in steady state
  (reused lists). The fast exit (`anyMinding`) makes a world without Frostmaw dens cost one array read a
  tick. D4: the sampler is Create code in the create plugin; it reaches the core only through `RtsimApi`,
  and the core knows no Create type. D3: no entities. UR-1: MZB-326.

**GameTests to write** (synthetic trains run far away, out of entity-ticking chunks, since a synthetic train
in an entity-ticking chunk is dropped; each test at its own far base; scripted engine first, then native):
1. `theSamplerReportsPassesOnTheRealTrack` (§7 test 11, `TrainSamplerGameTests`, batch `rail_sampler`):
   - Setup: a code-built 90 degree curve of radius 100 and an S-bend (two such curves, opposite hands),
     each between two stations, with a synthetic train held at 1.4 blocks a tick. A second run on the curve
     holds it at 2.0.
   - Territories (ConflictKit's scripted TERRITORY), mask bit 6:
     - (a) radius 15, centred 10 blocks outside the arc's midpoint: the track enters it and the chord
       between the stations does not;
     - (b) radius 15, centred on the chord's midpoint, 29 blocks inside the arc;
     - (c) and (d), the S-bend's equivalents;
     - (e) as (a) with a mask without bit 6.
   - Assertions: (a) and (c) get exactly one RAIL_PASS per run; (b), (d) and (e) get none.
   - Each pass: its point is within 2.5 blocks of the curve (horizontal), and its y within 1 of the
     track's; entry is before exit; the train id is `CreateIds.of(id)`; the owner is the train's owner (one
     run with a null owner gives (0, 0)).
2. `aTrainPassesThroughAFrostmawWithoutTouchingIt` (§7 test 10, `RailFrostmawGameTests`, batch
   `rail_frostmaw_pass`, `-PwithMowzie`):
   - Setup: a 64-block straight with a station, and a train assembled at it as `RailFixtureGameTests`
     does (`StationBlockEntity.assemble`), held at 1.0 blocks a tick. A Frostmaw (by registry id, NoAI)
     stands on the rail 32 blocks ahead.
   - Assertions: the carriage entity's box overlapped the Frostmaw's on some tick. The Frostmaw's health is
     unchanged, its `hurtTime` is 0 throughout, and it moved under 0.1 blocks. The train is not derailed,
     and its distance over the run is within 1% of speed × ticks.
   - Control, the same run with a zombie on the rail: the zombie is moved at least 0.5 blocks.

**Rust tests** (`crates/rtsim-conflict/tests/rail.rs`, on the shipped cultures unless named):
- The meter's arithmetic: `a_late_pass_counts_as_if_on_time`, `the_meter_settles_at_n_over_tau`,
  `falls_to_is_the_closed_form`.
- The gate and its wakes: `the_gate_pushes_only_on_the_band_and_the_day`,
  `the_wake_is_the_band_edge_or_the_day`, `a_quiet_meter_retires_and_pushes_zero`,
  `a_stale_rail_wake_is_counted`.
- Owners and held passes: `an_owned_pass_counts_at_once_and_an_ownerless_one_on_credit`,
  `an_ownerless_pass_let_go_counts_against_the_chunks_top_or_nobody`,
  `held_passes_are_capped_per_train_and_in_all`, `a_self_edge_or_no_departure_lets_go`.
- Lines: `a_line_credit_moves_the_point`, `the_seventeenth_line_evicts_the_smallest`.
- Sources and the focus: `set_emission_for_reaches_only_its_faction` (two dens, one pinned source;
  `reattribute` and a new faction's pickup leave it alone),
  `a_district_focus_is_not_displaced_by_a_track_nor_a_track_by_a_bare_pos`,
  `a_zero_weight_kind_sets_no_focus`.
- Targets: `a_raid_on_a_track_focus_targets_the_track_and_an_invasion_the_station`,
  `the_spot_is_five_blocks_toward_the_den` (the +x fallback, and a den across a wrapped seam),
  `a_raid_at_a_track_waits_and_goes_home_raiding_nothing` (no on-the-way roll, no resolve).
- Ingest: `rail_pass_for_an_unknown_faction_or_a_zero_weight_culture_is_dropped`,
  `umvuthana_minds_no_rail` (its TERRITORY mask lacks bit 6).
- Persistence and the seed: `the_meter_round_trips_blob_and_paged` (meters, lines, holds, the pin,
  `Focus.track`, `Target::Track`, a booked `Ev::Rail` that fires on the same tick after a round trip),
  `the_middling_seed_is_middling`.
- In `culture.rs`: rail parses, and a missing rail entry is weight 0.
- rtsim-jni's protocol test: RAIL_PASS is live, not deferred, and its times are translated. A TrainArrived
  after a departure calls `rail_call` with the line; a TrainUnwatched lets go.

**Sim runs** (`rtsim-cli sim mowzie`; MZB-330's seeds, jitter and tolerances; a middling chief; the raid model
of §4.3: a 1-day muster, 150 blocks each way at 0.02 blocks a tick, a 2-day lurk, a repeat 4 days after it
is home; dwell 0.5 day on Notice and 4 on Raid):
- The synth's rail: a line is a polyline between two district stations with a headway and an owner. Each
  train's leg emits RAIL_PASS for each den circle it crosses (the sample nearest the den, in the polyline's
  own 20-tick samples), then a TrainArrived that runs `rail_call` as rtsim-jni does.
- **Run 1, the meter** (RUNS row 1's "the meter M7"): one line at 4, 12, 24 and 50 legs a day, and the
  trunk of 14 lines at 1.7, each against §4.3's first table:

  | Traffic | Settles at | Notice | Raid | 140 | Invasion | Pushes in 20 days |
  |---|---|---|---|---|---|---|
  | 4 legs a day | 17.3 | 11.8 | never | never | never | 19 |
  | 12 | 86.6 | 3.0 | 17.1 | never | never | 24 |
  | 24 | 190.4 | 1.8 | 6.4 | 14.1 | 17.6 | 30 |
  | trunk of 14 at 1.7 (23.8) | 188.7 | 1.8 | 6.3 | 14.2 | 17.5 | 33 |
  | 50 | 415.5 | 1.1 | 3.5 | 5.8 | 7.5 | 37 |

  Each Raid's target is Track; each Invasion's is the station district nearest the busiest line's point.
  `stats.rail_wakes` stays under 3 a day per meter.
- **Run 2, cut and stopped service** (§4.3's second table): 12 legs a day, 24 from day 2, a peak of about
  122 on day 13. From day 12:

  | From day 12 | Under 70 | Under 42 | Settles at |
  |---|---|---|---|
  | cut to 4 legs a day | 21.4 | 28.1 | 17.3 |
  | stopped | 19.2 | 23.6 | 0 (Notice left on day 40.5) |

  The repeat raid due on day 15.1 still comes. No raid sets out once the grievance is under 70.
- **Run 3, reroute**:
  - (a) From day 12 the line's track moves off the circle. Its passes stop, and the grievance follows the
    stopped row.
  - (b) The track moves within the circle. The line's point follows the new passes, and the next Raid's
    Track target is at the new point.
- **Run 4, two dens on one line**: both circles get passes and both feuds follow the 12-a-day row. A den
  found on day 10 gets its first pass from the next train and its own meter from there.
- **Run 9, the trunk**:
  - The 14 lines at 1.7 match the one line of 23.8 within MZB-330's tolerances (the trunk row).
  - With one line at 6 and the others at 1.2, the focus is the busy line's point.
  - Two owners' trains on one line make two meters and two feuds, each at its own share.
- **Bench** (MZB-331), and the sampler bench at 200 trains on the laptop (SamplerBench).

**Work split** (in order; each step's tests green and committed before the next):
0. Branch (MZB-300).
1. `culture.rs` (`Rail`, `COUNT`, names), `grievance.rs`'s `K`, `frostmaw.json`; the culture tests and
   `umvuthana_minds_no_rail`.
2. `rail.rs`: `RailMeter` and `LineCount` (`n_at`, `add`, `per_day`, `falls_to`) with the arithmetic tests.
3. `Source.pin`, `set_emission_for`, `add_faction`'s skip; `Focus.track` and `refresh_rates`'s class
   rule; their tests.
4. `Target::Track`, `target_of` with the kind and its two callers, `spot_toward`, `target_pos`, `arrive`'s
   Track arm, the on-the-way skip, `resolve`, `what` and `rescue_news`; their tests.
5. `Held`, `rail_pass`, `rail_call`, `holds_train`, `let_go_due`, `rail_gate`, `rail_wake`, `Ev::Rail` and
   its dispatch, the counters, `later` and `ingest`; the remaining engine tests. **Opus review A**: the
   engine, steps 1 to 5, against §4.3 and ledger 1.
6. Persistence (MZB-302) and `the_meter_round_trips_blob_and_paged`.
7. rtsim-industry's `last_departure_of` and `station_decl`; rtsim-jni's capture, `conflict::rail_call`,
   `translate`, and the protocol test.
8. The synth's rail, `MIDDLING` and its test, runs 1 to 4 and 9. **Opus review B**: the runs against §4.3's
   two tables.
9. The bench pair (MZB-331).
10. Core Java: `Territories.crossing` and `anyMinding`, `RtsimApi.railPass` (and `conflictUp`),
    `ConflictBridge.railPass`, `ConflictRecords`, `ConflictKit.railPasses`; compile.
11. Create: `TrainSampler`, its registration, `SamplerBench`; compile; the sampler bench.
12. `build.gradle` (MZB-329), `TrainSamplerGameTests` and `RailFrostmawGameTests` test 10. **Opus review
    C** before the GameTest round, then the GameTests on the scripted engine and on native.
13. `boundary.md` (MZB-333) and, at landing, the fixtures (MZB-302).

**What M7 changes in M6.** The engine's `Focus`, `Source`, `Target`, `refresh_rates`, `target_of`,
`arrive`, the on-the-way roll, `resolve` and `rescue_news`, and the page format (RTC8 / META 7). Not
changed: the camp, the renewal, the chief paths, CHIEF_STATE, any Mowzie plugin code, and `TrainMixin`.

**Where the jar or the engine differs from MOWZIE.md.**
- Ledger 1 puts the meter in the composition. It is in rtsim-conflict, which owns the wheel, the pages and
  the replay (MZB-303).
- Ledger 1's `set_emission_for(faction, key, kind, rate, pos, actor)` takes no key or kind; both follow
  from the faction and actor (MZB-313).
- §4.4 puts the spot perpendicular to the track between two samples. RAIL_PASS carries one point, so the
  spot is toward the den, within 8 degrees of perpendicular on a straight (MZB-316, 332).
- A pass is credited to a line only between two district stations. `TrainWatch.arrived` returns early for
  a station with no district (TrainWatch.java:224), and an unwatched stop ends a leg; other passes are let
  go (MZB-311).
- The industry's clock is game time and the conflict's is world time, so RAIL_PASS's times are translated
  (MZB-319).
- The fixture's 90 degree curve (radius 9.5) is shorter than one sample's travel, so it cannot serve test
  11 (MZB-327).
- MOWZIE.md's engine line numbers are stale. At 761f5d0f, `set_emission_inner` is 1541-1626 (not
  1337-1424), `refresh_rates`'s focus is 1715-1719 (not 1513-1517) and `resolve`'s Pos arm is 3409-3421
  (not 2869-2878).
- §3.1's no-collision is confirmed at the jar: IGNORE against Create's NORMAL-only `canCollideWith`.

**Traps.**
- zsh eats `"$b:crates/..."` as a history modifier: write `"${b}:crates/..."`.
- `tail -3` fails in these shells: write `tail -n 3`.
- A synthetic train in an entity-ticking chunk is dropped: run test 11's trains far away.
- An unwatched lurk's segments are 28 to 40 blocks long. On a curve of radius under about 24 blocks, a
  40-block sample's sagitta exceeds the reach of 8, so a pass can miss the track point (M8). Test 11 uses
  radius 100, where it is 2.
- `git log -G'MZB-3..'` shows M9's old 5d747e31, which used MZB-300 to 343 before 39db13e2 moved M9 to 400
  to 443. Check this range with `-G'MZB-3[0-4][0-9]'` and ignore 5d747e31.
- `refresh_rates` keeps a focus per kind, the latest refresh winning. Without MZB-314's class rule, a kill
  site would replace the track focus and the next Raid would raid nothing.
- The arrival clears the industry's `last_departure`: capture it before `l.ingest` (MZB-312).

### M7 build choices (engine half, steps 1 to 9; Java half, steps 10 to 13)

Where the design left a gap the build took the conservative option; each is undoable. Code is in
`crates/rtsim-conflict/src/engine/rail.rs` unless it says otherwise.

M7 took MZB-348, MZB-349 and MZB-375 to MZB-391 (assigned at the Opus engine review of 0fb08ef5; 392 to
399 stay spare, and are M7's too if the Java half needs them). The review's fixes are MZB-348, 349 and
375 to 384, after MZB-347 below; MZB-385 is the lead's decision on the wakes check; 386 to 390 are the
Java half's (after MZB-384 below); MZB-391 is Opus review C's engine fix (after MZB-390).

- **MZB-334** (a late pass and the line's point): a line's point, y and nearer station are set by a pass
  only if its exit is not before the line's reference time. A pass reported late still counts (adds are
  exact in any order: `a_late_pass_counts_as_if_on_time`) but leaves the point where the later pass put
  it, so the Raid's track point is the freshest one.
- **MZB-335** (held passes): a pass's age is counted from its exit, not from when it was held, and it is
  let go after a day. Eviction by the caps (64 a train, 4,096 in all) is a let-go like any other
  (`rail_let_go`). An ownerless pass let go counts against the presence top over its own chunk, read at
  the present (the tick of the call that lets it go), not at its exit, so a player who arrived since
  counts; a pass nobody stands near counts against nobody.
- **MZB-336** (unknown, self-edge): a pass for a faction the engine does not know, or whose culture
  weights no rail, counts `rail_unknown` and makes no meter; `rail_passes` counts only accepted passes.
  An arrival whose two stations are one (a self-edge) is treated as no line (the held passes are let
  go), as rtsim-jni sends it.
- **MZB-337** (the band's edge): the push gate's band test has a tolerance of 1e-9, so a wake booked at
  the first tick the closed form crosses the band's edge is not booked again a tick later by a float a
  hair on the near side.
- **MZB-338** (retirement and stale wakes): a push that takes the pushed rate to 0.01 a day or less
  retires the meter on the spot (the source is cleared first). A wake already booked for a retired meter
  finds none and counts stale. If the meter is made again its stamp restarts at 0, so a stale wake of
  the old one can fire early on the new one; that costs one extra gate evaluation and nothing else, so
  it is accepted rather than carried in the page.
- **MZB-339** (MIDDLING): the chief seed of the sim's runs is 20,376,233, the smallest of 13 seeds below
  2^28 whose `Temperament` is (128, 128, 128). `the_middling_seed_is_middling` asserts it
  (`crates/rtsim-conflict/tests/rail.rs`, `synth::MIDDLING`).
- **MZB-340** (sim geometry): the synthetic line is a straight north-south track 150 blocks east of the
  den's centre (radius 200), trains run at 1.4 blocks a tick (Create's top speed), the sampler sees them
  every 20 ticks, a pass is the sample nearest the den reported when the train leaves the circle, and
  the arrival lands at the leg's end. The clock moves an hour at a time with every pass and arrival at
  its own tick (`synth::rail_events`, `rtsim-cli/src/sim/mowzie/rail.rs`).
- **MZB-341** (the Invasion's day): the design's table has the Invasion a Raid dwell (4 days) after the
  Raid rung's start (17.6, 17.5, 7.5 days for 24 a day, the trunk and 50 a day). The engine restarts the
  rung's dwell when a band comes home (`feud.since`, MZB-51: it anchors the repeat), so the Invasion comes
  a dwell after the last Raid's return, or when the grievance reaches the rung's threshold if that is
  later: 21.6, 21.6 and 11.2 days. The ladder is M2's and the repeat rule is tested (`a band comes again
  4 days after the last one is home`), so it is not changed. The sim asserts the engine's rule for every
  seed (the Invasion's day is the 140 crossing's or a Raid's return day plus 4.0, within 0.25) and
  prints the design's day beside it. The disagreement is in the design's model, not the rail meter: the
  Notice, Raid and 140 columns and the settled levels all match. Fixing it would need a field decoupling
  the repeat's anchor from the climb's dwell; that is a ladder change and is left to Ben.
  **The lead decided: MZB-341 stands.** The design table's Invasion days (17.6, 17.5, 7.5) are superseded by
  the engine's (21.6, 21.6, 11.2): the ladder's dwell rule is M2's, tested and left alone, and the sim's
  assertion is the engine's rule.
- **MZB-342** (sim windows): runs 2 and 3 run 90 days, not 60, because the grievance is still coming down
  from its peak of about 122 (day 13) at day 60. A cut to 4 a day settles at 17.3 on a window of days 60
  to 90 (one seed's own reading wanders by about 1.5, so a long window); a stopped service reads
  under 0.5 over days 75 to 90 (it is 0.56 on days 60 to 90). Every other row reads days 50 to 70.
- **MZB-343** (an owned pass's meter): an owned pass is counted at ingest and credited to its line at the
  arrival only if the meter still exists then (a meter retired in between has no line to credit). The
  held table keeps `held_n` (a derived counter, not saved) and `held_oldest`, one entry a train.
- **MZB-344** (formats): RTC8 and META 7 are the next after this branch's RTC7 and META 6 (M6). A merge
  that finds another slice has taken them moves M7's one past (MZB-302). The held passes' entry time is
  not used by the engine and is not saved.
- **MZB-345** (rtsim-jni): `rail_call` does not run the wheel (the jni `catch_up` does, so a wake due
  before the arrival has fired first). The line comes from the industry's `last_departure_of`, read
  before `l.ingest` (MZB-312); an unwatched train, an arrival with no departure and a self-edge give
  `line: None`. The call is made only if the conflict is running and holds the train. A pass's entry and
  exit are translated with `phase1::world_tick`. The tests ran at offset 0, so the translation was
  not checked by equality; MZB-380 adds a protocol test with a three-day offset that reads the
  meter's decay reference (the engine reads only the exit; the entry has no reader to assert on).
- **MZB-346** (bench, MZB-331): `rtsim-cli bench scale --only factions,factions-paged`, full, release,
  base (f6321fa5, built from `git archive` in a scratch target) against M7 (165f1b58) side by side at
  nice 10, machine load 15 to 55. The resident `factions` sweep shows no cost: at 1,024 factions the M7
  over base ratio of CPU per day is 0.964 and 1.041 in two pairs (the A/A pair's ratios are 0.92 to 1.00
  at 16 to 1,024). The paged `factions-paged` sweep costs more: M7 over base is 1.031, 1.067, 1.041
  (pair 1) and 1.052, 1.025, 1.062 (pair 2) at 1,024, 2,048 and 4,096 factions, mean 1.047, against an
  A/A pair of 0.986, 0.998, 0.987. That is about 3.3 points beyond the A/A spread, over MZB-331's gate of
  3 by a hair. The cause is bytes, not work: the Rail signal kind makes `Feud.raw` seven f64s instead of
  six, a focus keeps an optional pin, a source an optional boxed pin, and a faction row a meter list, so
  an encoded faction is 27 bytes larger (save_full 527,088 to 554,743 bytes at 1,024, +5.2%) and the
  resident estimate of the paged rows is 13.8, 15.4 and 15.9% larger. A paged step is a page load plus
  its events, and with the budget fixed fewer rows stay resident. Events (28,375, 56,748, 113,227) and
  conservation are the same as base. Mitigations if the gate is held: pack `Focus.track` (an
  `Option<Pin>` is 40 bytes and doubles `Focus` from 40 to 80), or carry the Rail rate outside
  `Feud.raw`. Neither was done then, since both change a row layout the design fixes (MZB-313, MZB-314);
  the lead held the gate and ordered both, and MZB-347 is the result.
- **MZB-347** (the layout amendment and the bench again; amends MZB-313 and MZB-314 "for MZB-331's
  gate"): a world with no rail pays base's bytes but for pointers. What changed is written under
  MZB-313 and MZB-314. The sizes, with `size_of` pinned by `a_world_with_no_rail_keeps_base_layout_but_
  for_pointers`: `Focus` 40 to 48 (an inline pin made it 80), `Feud` 296 to 312 (the grievance's tail
  pointer and the focus's track pointer), `FactionRow` 200 to 208 (the meter list's pointer), a source
  unchanged from M7's (its pin was already boxed). The encoded rows: a feud, a focus and a source with no
  rail are 176, 197, 32 and 40 bytes, equal to base's (measured at f6321fa5 and pinned by
  `a_world_with_no_rail_encodes_what_it_did_before_m7`); a faction row adds one byte (its meter count).
  Every number the engine computes is bit-identical, since a tail adds a trailing term to the sums
  (`the_rail_kind_projects_as_any_other_kind_does`), so `sim mowzie` reads the same Notice, Raid, 140,
  Invasion, push and settled values and only two state hashes moved; `sim conflict` moved only its state
  hashes. The tests: the workspace's 1,411 pass, and the same five fail as before (rtsim-host `gametest_worlds_open_in_the_host_with_their_saved_state`,
  rtsim-view fixtures twice and hashes twice, which need GameTest saves).
  - **A focus from a source with no district** (MZB-314, `refresh_rates`): on main only a source with a
    district sets a feud's focus. With the class rule, a continuous source with no district (a bare
    position, class 0) sets one too, when the feud has none or a class-0 one; a track (class 1) and a
    district (class 2) are never displaced by a lower class. `sim conflict` reads the same numbers as
    before M7 (the committed and the regenerated outputs differ only in its state hashes and the new
    `rail_stale` field).
  - **Bench** (as MZB-346: release, `--only factions,factions-paged`, nice 10, side by side, machine load
    10 to 25). A/A pair (base against itself): paged CPU per day 0.991, 0.996, 0.984 at 1,024, 2,048
    and 4,096 factions, so the A/A spread is 1.6 points. New head (35ca0370) over base f6321fa5, pair 1:
    1.024, 1.024, 1.038 (mean 1.028); pair 2: 1.013, 1.033, 0.954 (mean 1.000); all six mean 1.014, which
    is below the A/A spread, against MZB-346's 1.047. The resident `factions` sweep at 1,024: 1.023 and
    0.998. Gate (3% beyond the A/A spread): met. Events (28,375, 56,748, 113,227) and conservation are
    base's; the state hashes differ from base's, as the encoded bytes do by the meter-count byte.
  - **Bytes**: `save_full` at 1,024 factions is 528,119 against base's 527,088 (+1,031, +0.2% =
    1,024 + 6 + 1: one byte a faction row for its meter count, six for M7's six stats counters, each a
    one-byte 0, and one for the count of the held passes' table, empty in this world, which the blob's
    resident tail ends with (`put_usize(w, self.held.len())` in `persist.rs`)), against 554,743 before
    the amendment. Since MZB-349 the stats carry a seventh counter (`rail_stale`), so the same figure
    at a later head should be +1,032 (derived from a zero varint being one byte, not measured);
    at 2,048 1,060,076 against 1,058,021; at 4,096 2,124,451 against 2,120,348. The paged resident
    estimate at 1,024 is 975,888 against base's 943,656 (+3.4%; it was 1,074,208, +13.8%), at 2,048
    1,828,672 against 1,755,120 (+4.2%), and at 4,096 3,528,688 against 3,384,128 (+4.3%): the pointers
    only (the tail, the track and the meter list, 8 bytes each, on a feud, a feud's focus and a row).

#### M7 review fixes (the Opus engine review of 0fb08ef5)

No P1; two P2s and some P3s. Commits on `feat/mowzie-m7` after 0fb08ef5, in this order.

- **MZB-348** (P2-1, a dead track keeps the focus): when a meter's source is removed (the 0 push of
  `retire_meter`), its actor's feud keeps the focus position and loses `track`, so the class rule of
  MZB-314 lets a class-0 focus (a wild kill) replace it and the next Raid goes to the kill site, not to
  a track nobody runs. The review's probe is folded into `tests/rail.rs` (trains, the meter retires,
  a wild kill, the Raid's target).
- **MZB-349** (P2-2 and P3-7, the sim's checks): the Invasion rule expects the larger of the last Raid
  return at or before it plus the dwell, and the 140 crossing, from the last return only (an earlier
  return no longer excuses it); it holds on every seed. "A meter wakes under 3 times a day" counts every
  `Ev::Rail` pop, live and stale (new stat `rail_stale`, persisted: the stats blob has seven rail
  counters). Run 4 asserts the third den's meter `n` is 1.0 right after its first pass. Runs 2 and 3
  read the projected grievance over the window whether the feud is forgotten or not (a forgotten feud
  is 0, not an empty window printed as "-0.0"), and the rail runs print the first seed's state hash.
  Before the stale-wake fix the wakes check failed at 8.85, 24.80, 48.75 and 100.56 pops a day at 4,
  12, 24 and 50 legs a day; it was not loosened (see MZB-382 for where it stands).
- **MZB-375** (the replay-end trigger before `rail_call`): rtsim-jni's `conflict::rail_call` runs
  `resend_notices_if_due` first when the record is this session's first live one
  (`OBS_SEQ >= session_first`), as `journaled` does before any conflict record. The branch that will own
  this trigger is `fix/replay-push` (the replay-gate redesign); when it lands, M7's call follows
  whatever trigger it leaves in `journaled`. The test
  (`a_session_whose_first_record_is_an_arrival_keeps_its_notice`: 60 passes held across a restart, the
  arrival first, one kept notice sent once after the territory) passes with and without the call on this
  branch: the first live conflict record or `simulate` fires the trigger anyway, and `resend_notices`
  re-adds the kept notices from `feud.notice`, so there is no observable loss when the feud is still
  there. **Failing first (Opus review C, P3-6):** the case where it is not is a catch-up that climbs the
  feud to the Notice and lets it fall away again, which the first arrival's catch-up does if it is far
  enough on. `a_notice_the_first_arrivals_catch_up_makes_is_sent_though_its_feud_ends_there` (60 passes
  held across a restart, the first record of the next session an arrival 60 days on; the feud ends at
  about day 41 to 45) passes with the call, the plugin sent the one Notice after the territory and no
  feud left. With the call removed it fails: the catch-up queued the Notice, the replay-end resend
  stripped it from the queue and the feud, gone, kept none to send again, so the plugin was sent 0
  notices, not 1 (`assertion left == right failed: the notice the catch-up made was sent once: []`,
  left 0, right 1). The call is restored. Days 30 and 40 pass without the call (the feud is still
  there to be re-sent), day 45 fails.
  **Since the M7 merge** (RP-102): the call is gone with the other MZB-141 triggers (RP-2). The
  replay end runs at the top of rtsim-jni engine.rs `journaled()`, before industry's journaled entry
  and so before `rail_call`, and both tests pass on the merged head with nothing in `rail_call`.
- **MZB-376** (P3-3, the booked wake): the meter keeps `wake`, the tick of the wake booked under its
  `stamp` (zero when none waits). A new wake is booked only when none waits or the new one is earlier;
  a later one leaves the booked wake, which fires, finds the rate in its band (or pushes) and books the
  next. The tick is saved with the meter (RTC8 / META 7, M7's own, no bump): in paged mode it cannot be
  rebuilt at load, since the wheel entry is not tied to a row. A stale pop reads the faction row
  (`row`, not `row_mut`), so it dirties no page. Before, every leg moved the wake and left the old entry
  stale: about two stale pops a leg.
- **MZB-377** (an owned credit skips the gate): the arrival's credit of an owned pass to its line
  (`credit_only`) leaves the rate as it was, which the gate saw at ingest, so it runs no gate. A meter
  retired in between has nothing to credit (MZB-343).
- **MZB-378** (P3-4, the rail bench): `rtsim-cli bench scale --only rail-paged`: dens (each with a line
  and an owner) at 50 legs a day through the paged engine, 256, 512 and 1,024 dens (256 and 512 under
  `--quick`; 64 dens fit in the 256 KiB budget). The rows report legs, wakes, stale pops, pushes, page
  loads and the step time. D1 evidence for MZB-331, 346 and 347, which benched no trains; not a gate.
  Before (MZB-376 not yet in) against after, `--quick` (3 days after 2 of warmup), release, nice 10, the
  two binaries run side by side at machine load 24 to 28, so only the ratios mean anything:

  | dens | legs/day | wakes/day | stale pops/day | pushes/day | step ms/day | page loads/day | save KiB | state hash |
  |------|----------|-----------|----------------|------------|-------------|----------------|----------|------------|
  | 256 before | 12,788 | 73 | 26,321 | 368.3 | 408.9 | 22,929 | 305.5 | 0a2a9dd3 |
  | 256 after | 12,788 | 1,004 | 164 | 368.3 | 318.9 (0.78) | 15,287 (0.67) | 205.9 | ce5e9ae4 |
  | 512 before | 25,599 | 145 | 52,652 | 739.0 | 1,381.7 | 68,141 | 619.3 | 16ae6612 |
  | 512 after | 25,599 | 2,004 | 315 | 739.0 | 1,069.0 (0.77) | 44,591 (0.65) | 416.2 | 94b07803 |

  The pops a den a day fall from 103 to 4.6 and the pushes do not move. The stale pops (26,321 a day,
  two a leg) become live wakes (1,004): see MZB-382. The saved wheel is a third smaller (the stale
  entries were saved too). The page loads fall by a third, not by the stale pops' share, because each
  of the 12,788 legs a day still loads its den's row page. A single 1,024-den point of the after binary
  (3 days, quiet machine): 51,193 legs, 4,008 wakes, 614 stale pops and 1,484 pushes a day, 100,566 page
  loads a day, 1,391 ms a day, a resident peak of 324,120 bytes (the budget is 262,144). The full sweep
  (10 days) took 729 s for its 256-den point under load 20 to 28 and was not waited for. The same
  binary gives the same state hashes on every run (ce5e9ae4, 94b07803). The sweep's resident peak is not flat per den: every den takes a leg every 480
  ticks, so a step dirties nearly every row page, and dirty pages are pinned until the save (the peak is
  the budget plus a step's changes, which grows with the dens). Measured slopes 0.119 (256 to 512), 0.097
  (512 to 1,024) and 0.107 (256 to 1,024); the declared slope is 0.15 (`RAIL_RESIDENT_SLOPE`) because every den's
  leg dirties row pages that stay pinned until the save (measured 0.097 to 0.119), as the dragons' and herds'
  sweeps declare theirs. Both contracts pass.
- **MZB-379** (P3-5, tolerance 0): with a tolerance of 0 nothing is quiet (`r < 0` is never true), so no
  meter retired: it woke once a day for ever and pushed a vanishing rate. A meter now retires when its
  rate has fallen to `RETIRE_PER_DAY` and it is quiet or its pushed rate is as low; a dead meter that
  still holds a pushed rate pushes it down and retires (as at tolerance 2; MZB-391 makes that one push,
  not two); with no lower band edge the wake is the earlier of the day and the retirement. Frostmaw's
  tolerance is 2, so `sim mowzie` and `sim conflict` are unchanged by it.
- **MZB-380** (P3-9, rtsim-jni tests): `a_pass_is_counted_on_the_worlds_clock` (a three-day phase 1 offset;
  it fails with 61,458 found for 133,458 expected when the exit is not translated) and
  `a_held_pass_crosses_a_restart_to_the_replayed_arrival` (RAIL_PASS before the save, TRAIN_ARRIVED
  journaled after it and replayed on the reopen: the rail called once, nothing held, the meter equal to
  the one before the crash). The entry is translated with the exit and not asserted: nothing reads it.
- **MZB-381** (results and hashes): `scenario/results/sim-mowzie.json` and `sim-conflict.json` are
  regenerated. Every state hash moved, for two reasons: the stats blob has a seventh rail counter
  (MZB-349, a byte), and a meter carries its booked wake (MZB-376, a varint a meter; only the rail runs
  have meters). Nothing else in `sim conflict` moved. In `sim mowzie` every settled level, rung day,
  push count and peak is as before; the "wakes a day" figure is not comparable with the committed one,
  which counted live wakes only (0.93 to 0.97 a day), where it now counts live and stale pops.
  `RTC8` fixtures regenerate at landing, as the brief said.
- **MZB-382** (finding: the wakes check failed): after MZB-376 the pops are 2.09, 5.46, 5.24 and
  5.22 a day at 4, 12, 24 and 50 legs a day and 5.79 on the trunk (were 8.85, 24.80, 48.75, 100.56 and
  48.34), so "under 3 a day" passed at 4 legs a day and failed from 12. The remaining pops are live
  wakes, not stale ones: the wake is the tick the rate would fall to the band's lower edge if no leg
  came (MZB-306), and in a steady flow a leg always comes, so each wake finds the rate in its band and
  books the next, `tau ln(1/0.9)` = 0.21 days later at a 10% band: 4.76 a day whatever the rate, once
  the band is relative (above about 10 legs a day, where `max(0.1 p, 1)` is `0.1 p`). Before the fix
  every leg moved the wake later, so the live wakes were rare (0.9 a day) and the stale ones were the
  cost; now the stale ones are rare (a leg in 80) and the live ones are the cost, at 5 row reads a day
  a meter instead of 100. The check was not loosened at the time; the lead decided it in MZB-385.
- **MZB-385** (the lead's decision on the wakes check; step 0 of the Java half): the engine stays as
  it is. A later idle wake would move push times (run 2's cut-service timings are asserted to 0.2 days),
  and the pops are the meter doing its job: a service that stops is seen within the band, at a bounded
  cost that does not grow with the rate. The design's "a meter wakes under 3 times a day" counted the
  live wakes before the booked wake (MZB-376) existed, when a leg moved the wake later and so most
  wakes were stale; it was never a bound on every pop. The check is now what D1 needs: at most 6.5
  `Ev::Rail` pops a day (live and stale) per meter at every rate and on the trunk, and the 50-leg figure
  at most 1.1 times the 24-leg figure (no growth with the rate). Pops a day, median of the seeds:

  | legs a day | 4 | 12 | 24 | 50 | trunk (14 at 1.7) |
  |------------|---|----|----|----|-------------------|
  | before MZB-376 | 8.85 | 24.80 | 48.75 | 100.56 | 48.34 |
  | after MZB-376 | 2.09 | 5.46 | 5.24 | 5.22 | 5.79 |

  The 50-leg figure over the 24-leg one is 1.00. `sim mowzie` passes every check; only its
  check names and details changed.
  - **Opus review C, P3-5** (the check missed a slow burst and a cut service): the 6.5 a day mean is now
    checked on run 2 as well (the cut to 4 legs a day: 2.75 pops a day; stopped: 0.81, over its 90 days),
    and a second check bounds the busiest single day: **at most 20 `Ev::Rail` pops (live and stale) in
    one whole day, of any of the 8 seeds, for one meter**. A mean over 70 days hides a burst of a day or
    two. Basis: the busiest whole day of the worst seed was 4, 10, 7 and 9 at 4, 12, 24 and 50 legs a day,
    14 on the trunk, and 8 and 8 on run 2's cut and stopped services; 20 is 1.4 times the highest. The
    tally is the engine's `rail_wakes + rail_stale` counter read at each whole-day hour in `drive`; day 0
    is included. Runs with several meters (run 4's two dens, run 9's two owners) are not bounded by it,
    since the counter is the sum of their meters; run 4's worst day is 20 for two meters. The check is
    `sim mowzie` run 1 and 2's; results regenerated, the new checks added and nothing else changed.
- **MZB-383** (note for Java review C, the sampler): a station inside the circle, or within the margin
  (MZB-390) and a sample past its edge, makes TRAIN_ARRIVED come before that leg's RAIL_PASS (the pass is
  reported when the train leaves the circle plus margin, the arrival when it stops), so the pass is held
  past the arrival it belongs to and credited to the next leg's line. Two things this understated, which
  Opus review C found: a Frostmaw's `territory_radius` is 200 (`cultures/frostmaw.json`), a circle a
  camp's own station can sit in, so it is not a corner case; and a terminus inside the circle makes one
  pass per round trip, not one per leg (the train is inside from the outbound arrival to the return), as
  well as crediting the wrong line. The sampler's GameTest should cover a station that close, or the
  choice should accept the misattribution. **Resolved by MZB-387: accepted, not covered.**
- **MZB-384** (the factions bench pair): not rerun. MZB-376 changes the encoding of a meter (a varint)
  and the in-memory `RailMeter` (8 bytes), and MZB-349 adds a stats byte; no `FactionRow`, `Feud`,
  `Focus` or source layout moved (the `size_of` pins of MZB-347 pass), and the `factions` sweeps have no
  meters. The +1,031 of MZB-347 should be +1,032 at this head by the one stats byte (derived, not measured).

- **MZB-386** (the Java half's core, step 10): `Territories`, `ConflictBridge` and `RtsimApi` carry what
  MZB-324 and MZB-325 asked for, with these particulars.
  - `Territories.anyMinding(signal)` reads a per-bit count (32 ints) kept by `put`, `remove` and `clear`, so
    a tick with no rail territory costs one array read. `crossing(dim, x0, z0, x1, z1, signal, out)` meets a
    circle when the segment's nearest point to its centre is within the radius (`Territory.meets`), reads
    the cells of the segment's box and the wide list (circles over more than 1,024 cells, a bad radius)
    once each, and allocates nothing.
  - `ConflictBridge.up()` is true for a live session, not failed, current, with the conflict module on and
    M1's records taken; `railPass(...)` journals RAIL_PASS at once (as `flushSignals` does a batch) and
    counts `railPasses`. `RtsimApi` has `territories()`, `conflictUp()` and `railPass(...)`; the Create side
    reaches the core through them only (D4).
  - The sampler asks `anyMinding(6)` before `conflictUp()`, the reverse of MZB-320's wording: the first is
    one array read and the second reads the session, and the result is the same.
  - The geometry is tested by `TerritoriesGameTests` (six GameTests), not JUnit: the core's unit tests have
    no Minecraft class on their classpath, and `Territory` holds a `ResourceKey<Level>`.
- **MZB-387** (the sampler's particulars, step 11, and MZB-383's answer):
  - Each sample runs in a `try` that catches `RuntimeException`: a train in a state Create's own tick would
    also trip on loses its segment and open passes, and the sampler warns once per server run and goes on.
  - The state is dropped (open passes lost, not sent) when no territory minds the rail or the engine side is
    not up, and when the server stops (`ServerStoppedEvent`, with the counters). A pass open then is a
    pass the engine never hears of, which is the cheap side of UR-1 (MZB-326): nothing is saved. A train
    still inside the circle when the engine comes back (or the server starts) gets a fresh pass at its
    first sample, with `entry` the time of that sample and not the time it came in: a cut-short entry, never
    a doubled pass, because the dropped one was never sent. The exit is the true one.
  - A segment that clips a circle keeps the nearer of its two ends as the pass's point, and a pass is kept
    until a sample is beyond the radius plus the margin (MZB-390, which replaced "while the new sample is
    inside the circle"): a train through a circle narrower than 128 blocks that is sampled on each side
    yields one pass whose point is the nearer sample, on the track.
  - A train that left Create's list closes its passes at its last sample's time, at the next rebucket.
  - The sampler's `samples` and `passes` are public counters, and `tracked()` and `clear()` are for the
    bench and the GameTests.
  - **MZB-383 is accepted, not covered.** A station inside the circle, or within the margin (MZB-390) and a
    sample past its edge, sends TRAIN_ARRIVED before that leg's RAIL_PASS, and the pass is credited to the
    next leg's line. A Frostmaw's `territory_radius` is 200, so a terminus inside the circle is common:
    such a train makes one pass per round trip (it is inside from the outbound arrival to the return, and
    the pass opens on entry and ends beyond the margin on the way out), not one per leg, as well as
    crediting the wrong line. Both are accepted with it. The
    synthetic graphs of test 11 hold no station, and test 10's real train runs with no territory. M8's
    `TrainSampler.arrived` (MZB-366 below) is where a flush of the train's open passes at its stop would
    go, if the misattribution is judged to matter: it knows the train and the moment it stops. Not built
    here (it adds a hook to every arrival for a pass that only a station inside a camp's range makes).
- **MZB-388** (the bench, step 11): `SamplerBench` (with `SamplerBenchGameTests`, `stage.nbt` in its own
  namespace `mic_rtsim_samplerbench`, and `./gradlew :create:runSamplerBench -PskipJni`) runs
  `FarTrainBench`'s synthetic trains with 64 territories of radius 40 (half on each of the bench's two
  rings, minding the rail, under faction keys the engine does not know and so drops) and logs
  `[samplerbench]` every 1,200 ticks: samples, passes sent, ns per sample (median, p99, max) and ns per tick
  (mean, p99, max) of the sampler's handler, for 6 windows, then the server stops (see the run length
  below). It runs only with
  `-Dmic_rtsim.samplerbench=true`, which keeps a dev client's `/test runall` from starting it. The ordinary
  GameTest server enables `mic_rtsim_create` and the bench's structure is in another namespace, so the
  bench is not among its tests. Budget (PLACEHOLDER): 2 microseconds a sample, 20 a tick at 200 trains.
  - The first run measured nothing: `FarTrainBench` lays 100 trains on each ring (19 and 31 blocks apart)
    and its default four carriages are 38 blocks long, so Create's train collision crashed all 200 in the
    first 200 ticks, and the sampler skips a derailed train (a 20 ns return). The run now sets
    `mic_rtsim.farbench.carriages` to 1 (`-PsamplerCarriages=<n>`; the sampler reads only the first carriage,
    so the count does not change its cost), and every window logs how many trains are derailed and warns
    when any is. A bench of derailed trains is the trap; the `far bench: tick N: ... derailed` line shows it too.
  - Result (`./gradlew :create:runSamplerBench -PskipJni` at the head of the commit that added this line; 200
    trains of one carriage on the two rings, 64 territories of radius 40, the stub engine, the laptop at
    load 1.8; 0 trains derailed; about 1,970 passes sent in each window, so the journal writes are in the
    figures). The first window includes the JIT warming up; ns:

    | window | samples | median a sample | p99 a sample | max a sample | mean a tick | p99 a tick | max a tick |
    |--------|---------|-----------------|--------------|--------------|-------------|------------|------------|
    | 1 | 11,827 | 381 | 7,113 | 141,395 | 11,228 | 69,641 | 612,879 |
    | 2 | 12,000 | 300 | 1,754 | 79,209 | 6,772 | 43,862 | 178,655 |
    | 3 | 12,000 | 251 | 1,433 | 25,097 | 4,887 | 22,162 | 85,199 |
    | 4 | 12,000 | 250 | 701 | 24,636 | 3,793 | 12,664 | 28,473 |
    | 5 | 12,000 | 281 | 771 | 16,301 | 4,202 | 15,199 | 28,554 |
    | 6 | 12,000 | 311 | 792 | 8,095 | 4,401 | 15,439 | 30,748 |

    Warm (windows 4 to 6): a sample takes about 0.25 to 0.3 microseconds at the median and 0.8 at the
    99th percentile, against the budget's 2; a tick takes 4 microseconds on average and 15 at the 99th
    percentile, against 20. The worst warm tick (30 microseconds) is over the 20 only as a maximum, which the
    budget does not bound. Both budgets are PLACEHOLDERS, and these figures meet them. The numbers are
    wall time in a GameTest server that ticks unthrottled, with a JIT that has seen 72,000 samples; a
    dedicated server with the pack's mods will see more cache misses, which this bench does not model.
  - **The run length** (Opus review C, P3-8; compiled, not run): the circles are placed when the conflict
    bridge is up, which may be after the GameTest's start, and the windows count from the placement. The
    test used to succeed at a fixed `windows * 1200 + 40` ticks from its own start, so a bridge more than 40
    ticks late dropped the last window (and `windows` 0, "for ever", ran one window, by `max(1, windows)`).
    It now succeeds when the bench has reported its last window (`SamplerBench.done()`, polled with
    `succeedWhen`), so a late bridge delays the end and drops nothing. The timeout is `windows * 1200 +
    12,000` ticks (`PLACE_WITHIN`: ten minutes for the bridge, a placeholder; no bridge in that time fails
    the test with "N of M windows, the circles never placed" rather than ending quietly with a short run).
    With `-PsamplerWindows=0` the test never succeeds and its timeout is `Integer.MAX_VALUE` (a GameTest's
    timeout is a tick count compared as a long, so it is never reached): the server runs until it is
    stopped.
- **MZB-389** (tests 10 and 11, step 12): written and compiled, not run (the GameTest round is the lead's).
  - Test 11 (`TrainSamplerGameTests`, batch `rail_sampler`, one test with four runs on separate graphs 250
    blocks apart at a far base). Bends of radius 100, circles of radius 15 placed 10 blocks outside the
    first bend's midpoint (sagitta about 29: the rail enters the circle and the chord does not) and 29
    blocks inside the arc on the chord (the chord crosses and the rail does not). Each run verifies the
    geometry itself before the sampler is trusted (`Run.verify`). The checks are MZB-327's, with the
    point within 2.5 blocks of the rail.
  - Test 10 (`RailFrostmawGameTests`, batch `rail_frostmaw_pass` for the Frostmaw and `rail_frostmaw_control`
    for the zombie: each calls `releaseFarChunks`, which would release the other's chunks if they shared a
    batch): the real train is built as `RailFixtureGameTests` builds its own. The track is 150 blocks along
    x at a far base, on a stone floor with the air above cleared (a Frostmaw is 4 by 4 blocks and the
    terrain is whatever the world made). The train is driven by `manualTick` and `speed` 1.0 each tick; its
    direction is found by running it three ticks. The distance check counts observations, not game time,
    so it does not depend on whether this test or Create's tick runs first in a server tick, and expects
    `SPEED * (observations - 1)`: observation 1 is in the tick that took the start, before Create moves the
    train. The entity's start is taken three observations after it
    is placed (it settles on the rail while the train is 30 blocks off).
    - What is restored, on a pass and on a failure alike, once: the train and its carriage entities are
      removed; the track and the station are taken out as blocks (so Create takes their graph); the chest is
      emptied (it drops nothing); the glue entities in the box are discarded; and every block of the box
      (the track's 150 blocks and 3 past each end, 4 each side, from one below the rail to 7 above it) is
      set back to the state it had before the test built in it, with no neighbour or shape updates. The
      bogeys, controls, chest and glue of the cars are inside the box, so a failure before assembly leaves
      them nowhere. Block entities of the original terrain are not restored (a freshly generated far chunk
      has none the test could reach). The forced chunks are then released.
    The zombie has fire resistance
    (a NoAI zombie would burn in daylight and flinch) and ends the run when moved half a block. Hurt,
    health and drift are recorded each tick and judged at the end; only a derailed or lost train, or a
    dead entity, ends the test at once (`testInfo.fail`).
  - Test 10 has no `@GameTestHolder`: `MicRtsimCreate` registers the class from `RegisterGameTestsEvent`
    on the mod bus when `mowziesmobs` is loaded, so a run without `-PwithMowzie` does not see it and needs no
    Mowzie jar. It names the Frostmaw by `mowziesmobs:frostmaw` and no Mowzie class. Run it with
    `./gradlew :create:runGameTestServer -PwithMowzie` (MZB-329; the build compiles and lists its tasks
    with the property, verified offline).
- **MZB-390** (Opus review C, P3-3: hysteresis at the circle's edge): a pass opens where a sample's segment
  meets the radius, as before, and ends at the first sample beyond the radius plus
  `TrainSampler.MARGIN` = 40 blocks, or when the circle is gone, no longer minds the rail or is in another
  dimension. Before, a pass ended at the first sample outside the radius, so a track along the edge, or a
  train reversing on it, sent a pass for every swap in and out (each one a further count in the rail meter).
  - **The margin**: Create's default powered top speed is 40 blocks a second (`poweredTrainTopSpeed`, 2 blocks a
    tick) and the sample period is 20 ticks, so a train covers at most 40 blocks between two samples. A
    train that has just left the radius therefore cannot be beyond radius plus 40 by its next sample (and a
    train inside cannot reach it in one), so the band is always seen once before a pass ends: no train
    crosses the band between two samples unseen. A larger margin would keep a leaving train's pass open for
    longer for nothing; a smaller one lets a fast train that stepped out and back in be two passes. It is a
    constant, not the config: the config is not readable before the server's config loads, a pack may raise
    the top speed, and the margin is only a de-duplication. A pack above 40 blocks a second keeps correct passes
    and loses some of the de-duplication at full speed; the sampler already starts afresh above 128 blocks
    between samples (MZB-322).
  - **What a pass is now**: its `entry` is still the sample before the first segment that meets the radius;
    its `exit` is the first sample beyond the margin, up to about two sample periods later than before (a
    train at 1.4 blocks a tick, on a straight through the circle, is seen to leave 28 to 56 blocks after
    the radius). The point is still the sample nearest the centre among all the pass's samples (a sample in the
    band is a candidate, and is never nearer than one inside). The engine counts a pass at its exit
    (MZB-311), so the count lands up to about two seconds later than before.
  - **Test 11's length check** (`TrainSamplerGameTests`) was "at most two samples long"; it is now
    `ceil(2 * (CIRCLE + MARGIN) / (speed * PERIOD)) + 1` samples (5 at 1.4, 4 at 2.0), which the rail's
    104 blocks inside the margin circle and a sample's slack at either end stay within (at most 4 and 3).
  - **The edge test** (`aTrainAtACirclesEdgeIsOnePass`, batch `rail_sampler_edge`, written and compiled, not
    run): a point walked through the pass rule (`TrainSampler.Walk`, the same `advance` a train's sample
    calls; the sampler's state for a train needs a Create `Train` only to read its position) on a circle
    of radius 100. The samples are at 150, 95, 108, 97, 112, 99, 130, 101, 96, 139, 99.5, 141, 150, 98 and
    145 blocks from the centre, 0.2 radians apart (24 to 57 blocks between samples, one a tick), and the
    test checks the passes open after each (0, then 1 through sample 10, 0 at 11 and 12, 1 at 13, 0 at 14),
    that the engine received two passes, with entry and exit at samples 0 and 11, and 12 and 14, and the
    nearest sample for each point. The old rule, replayed outside the game on the same samples, sends six
    passes (samples 0 to 2, 2 to 4, 4 to 6, 7 to 9, 9 to 11 and 12 to 14); this one two.
  - `TrainSampler` keeps `owner` in its state, set from the train at each rebucket, so a walk has no train;
    `Territories.Territory.within(x, z, margin)` is the new test.
- **MZB-391** (Opus review C, P3-7: a dead line's gate pushes once): `rail_gate` on a meter whose rate has
  fallen to `RETIRE_PER_DAY` now retires it at once, quiet or not, and its one push is the 0 from
  `retire_meter`. Before, a meter that was not quiet (a tolerance of 0, MZB-379) pushed the dying rate
  first (a source of about 0.0098 a day made, the feud planned, and in the same tick the source unmade and
  the feud planned again) and counted that as a push; the retirement then pushed 0. The retirement's 0 is
  not a counted push, as a quiet meter's never was, so `stats.rail_pushes` is one lower for each such
  retirement. The test `a_dead_lines_gate_pushes_once` (a tolerance-0 Frostmaw den, four legs, run to the
  retirement tick) counts the emissions by the feud's stamp (each plans its feud once): left 2, right 1
  before the fix, 1 after; the push count is unchanged across the retirement. `a_meter_retires_at_tolerance_zero`
  shares the new test's den (`zero_tolerance_den`).
  - **Numbers**: `sim mowzie` regenerated gives `scenario/results/sim-mowzie.json` byte for byte (Frostmaw's
    tolerance is 2, and a falling rate is pushed down at each band edge, so by the time a meter is dead its
    pushed rate is already under 2 and it is quiet: the same reason MZB-379 left `sim conflict` alone). The rail bench pair
    (`rtsim-cli bench scale --quick --only rail-paged`, the head before against the head after, release,
    nice 10, side by side at load about 19): the same legs, passes, wakes, stale pops, pushes and loads a day
    (12,788 / 12,792 / 1,004 / 164 / 368.3 / 15,287 at 256 dens; 25,599 / 25,604 / 2,004 / 315 / 739.0 /
    44,591 at 512), the same state hashes (ce5e9ae4, 94b07803), the same slopes (resident 0.119); step ms a
    day 141.06 before and 139.43 after at 256, 585.30 and 586.29 at 512 (noise). The bench's lines never die
    (50 legs a day each), so the path is not on it.

## M8 Lying in wait

M8 is AMBUSH_SPOT, TRAIN_PASSED, TRAIN_ROBBED, the swipe and the roar (MOWZIE.md §9: ledger 8; tests 8 and
9; run 1's ambushes and run 8). It needs M7. Engine line numbers are at 761f5d0f, before M7 moves them.

- **MZB-350** (the branch): `feat/mowzie-m8` from `feat/mowzie-m7`. M8 changes no page format (MZB-357).
- **MZB-351** (lying in wait): in `arrive`'s Track arm (MZB-317), before the wait, `lurk_start(p, at)`
  sends AMBUSH_SPOT. Its fields:
  - `agent: Party::agent(p)`, the party's `epoch`, flags 0;
  - `dimension` the party's frame, `faction` the faction's key;
  - `track` (x + 0.5, y, z + 0.5) of the Track's point, and `spot` (x + 0.5, y, z + 0.5) of its spot at
    the track's y (Java finds the floor, MZB-360);
  - `reach` 8.0 (PLACEHOLDER, §4.4: the swipe's 6.5 plus half a carriage).

  It also counts `stats.lurks` and notes "lies in wait by the track at (x, z)". `fn lurking(&self, p:
  Handle<Party>) -> bool` is true for a war party (`kind.is_war()`) with a Track target in `Phase::Waiting`
  or `Phase::Fighting`. A player who comes on the lurk starts a watched fight there (`start_fight`, 3928,
  sets Fighting), and the lurk goes on through it: a train may pass mid-fight.
  - Amendment (built, step 1): no engine path reaches Fighting with a Track target. `arrive`'s Track arm
    comes before the arm that calls `start_fight`, and `wait_over` fights only a Siege. A player who comes on
    a lurk is `seen` (the band is embodied and Waiting, its epoch bumped), not in a fight. `lurking` still
    covers Fighting, as the design says, because it costs nothing and a later path to a fight there would
    otherwise end no lurk; `tests/lurk.rs` forces the phase to test it. The conservative choice, taken
    without asking.
  - Amendment (built): the old arrive note "waits at the track by ..." is `lurk_start`'s note, "the raid
    lies in wait by the track at (x, z)" (the party kind's name), so a Track arrival notes once.
- **MZB-352** (ending a lurk): the same record with `AMBUSH_REMOVED`, sent first in `go_home` (3710) when
  `lurking(p)`, before it changes the phase or the target. Every end of a lurk passes through `go_home`, so
  that is the one place:
  - patience out (`wait_over`, 3375);
  - a watched fight's end, routed, bored or withdrawn (`end_fight`, 4172), the chief's fall among them;
  - wiped out unwatched (5285);
  - Peaceful (`stop_war_parties`: Waiting goes to `go_home`, Fighting to `end_fight`).
  - A recall alone (`recall`, 3753; `player_left`, 5736) does not end the lurk. Each turns Fighting back
    to Waiting, the puppet goes, and the lurk goes on unwatched (ledger 8).
  - `stats.lurks_ended`.
- **MZB-353** (resend): `resend_lurks()`, called from `resend_notices` (4625) after `replay_done`. It sends
  one AMBUSH_SPOT (flags 0) for each lurking party, in handle order. `session_start` drops agent commands,
  and Java's lurk table lives for one session, so a restart sends each lurk exactly once more.
  - Amendment (built, step 1): `resend_lurks` first drops any AMBUSH_SPOT still queued (one a replayed
    record made again), as `resend_notices` drops queued notices, so what is kept is what goes and a lurk
    is never sent twice in a session. `lurk_start` and `lurk_end` are the only other pushers.
  - Amendment (review A, P3): the host never reached that drop. `resend_notices_if_due` calls
    `take_commands` first (rtsim-jni `conflict.rs`), which moves a replay-made AMBUSH_SPOT from the
    engine's outbox into the link's queue, and the `retain` that followed dropped only Notices, so the
    spot went out twice (harmless: same epoch, the later wins). The `retain` drops a queued
    `Cmd::AmbushSpot` too (one that carries test names is a test's order and stays). The protocol test
    is step 3's. `fix/replay-push`'s gate will also cover this (it holds the engine's commands until the
    replay has caught up), but its `Outbox` has no `extend`, so when that branch merges `resend_lurks`
    needs a push loop where it now calls `self.outbox.extend(lurks)`.
- **MZB-354** (home): `home()`'s kept-row branch (about 3842) rewrites a Track target to `Pos(track)`, so a
  row kept after its party is home never reads as a lurk.
  - Amendment (built, step 1): the removal went at `go_home`, so the kept row says nothing more. It keeps
    phase Waiting with a bare-position target, and `lurking` is false for it.
- **MZB-355** (TRAIN_PASSED): TrainPassed leaves `Conflict::later` for `train_passed(w, infra, tp, at)`.
  - The party is `Party::of_agent(tp.agent)`. It must exist, be `lurking`, and have `tp.epoch <=
    party.epoch`; otherwise `stats.stale`.
  - `stats.passes` counts each pass, and `stats.passes_watched` those with `PASSED_WATCHED`. Watched or not,
    the roll is the same, so a watched line loses what an unwatched one would (P28).
  - The roll is `StreamKey::new(seed, SYS_AMBUSH, p.to_bits()).slot(tp.train ^ at.0.rotate_left(32))
    .below(1000) < ambush_milli` (500).
  - A hit calls `ambush` for `tp.train`. That is I24's attack (source Boss, strength 41.7, carry 64, cap
    10%, the industry's cooldown of 24,000 ticks per train per class, the loot destroyed).
  - When it took more than 0 items (of any good: see MZB-356's review amendment), the engine sends
    `Cmd::TrainRobbed { faction: key, train }`.
  - Amendment (built, step 2): `ingest` passes `train_passed` the record's time as `at.max(now)`, and the
    roll's stream slot takes that tick. The stale test is written as the design's: stale when the party
    is gone, is not lurking, or `tp.epoch > party.epoch` (a watch bumps the party's epoch past the one
    the spot carried, so an earlier epoch still counts). A Peaceful world never reaches the roll (its
    bands are home, so the pass is stale), and `ambush`'s own `peace_held` arm stays as it was for the
    on-the-way caller.
  - Amendment (review B): the rtsim-jni protocol test
    `a_pass_that_hits_robs_the_train_with_an_infrastructure_and_nothing_without_one` has the "with an
    infrastructure" half back. A lurk is passed by 24 booked trains (1,280 of a declared good each) in a
    session with the industry module on: 10 robberies, each of the party's carry (80, under a tenth of
    1,280), 800 items in the industry's books, the engine's count and its TRAIN_ROBBED commands, and the
    industry's cooldown on exactly the 10 trains robbed. The same passes in a session with the module off
    are counted and rob nothing. With `with_infra` made to always hand `NoInfrastructure` the test fails at
    "the roll hit on some of 24 passes, not all: 0". The good must be declared in Java's dictionary (an
    undeclared one stalls the industry command queue, and the conflict queue behind it, so no
    TRAIN_ROBBED goes out: found while writing the test).
- **MZB-356** (ambush): `ambush` (3150) returns the items it took and takes the note's wording: "on the way"
  for the on-the-way caller, "by the track" for a lurk. Its Peaceful arm (`peace_held`) is unchanged.
  - Amendment (built, step 2): `ambush` is `pub(crate)` and takes the wording as `how: &str`; it returns 0
    for Peaceful and for no hit, so only a robbery of something sends TRAIN_ROBBED.
  - Amendment (review A, P2): it returns the items of any good the cargo held, not those of the world's
    goods alone. Cargo that maps to no good of the world's still goes to `ledger.unmapped` as before, and
    still leaves the note's count and `items_from_districts` at the world's goods, but the train is short
    and the roar goes (MOWZIE.md's cue is the cargo gone). `take_industry_loot` returns `(mapped, all)`;
    the district raid reads `mapped`. A hit that found an empty cargo (0 items) sends none. The tests
    are `a_robbery_of_nothing_sends_no_train_robbed` (skipped; empty cargo) and
    `a_robbery_of_unmapped_goods_still_sends_train_robbed`.
- **MZB-357** (persistence): nothing new. A lurk is derived from the party's phase, kind and target, and
  Track has been persisted since M7. The pages stay at M7's format.
  - Amendment (built, step 2; review A adds the fifth): the new stats (`lurks`, `lurks_ended`, `passes`,
    `passes_watched`, and `lurk_robberies`, the TRAIN_ROBBED sent) are appended to the saved stats list. That list is length-prefixed and read with `unwrap_or(0)`, so a page
    written before them reads zeros and no reader breaks; the page layout is M7's, and a save is four
    varints longer. Every state hash moves for that reason alone (stats are hashed with the state), so
    `sim conflict` and `sim mowzie` regenerate with only their state hashes and the new stat fields
    different.
  - Amendment (built, steps 1 and 2): the Rust tests are `crates/rtsim-conflict/tests/lurk.rs`. The
    design's `a_recall_does_not_end_a_lurk` and `a_lurk_goes_on_through_a_watched_fight` are one test
    (`a_fight_at_the_track_is_still_a_lurk_and_a_session_turns_it_back_to_waiting`, the phase forced:
    MZB-351's amendment). The cooldown test runs the real industry engine through `IndustryLink` with two
    booked legs, so the 24,000 ticks per train per class are the industry's own.
  - Amendment (review A, P3): three more lurk-end tests. `a_player_leaving_a_watched_lurk_recalls_it_
    without_ending_it` (ledger 8: `player_left` on an embodied lurk, Waiting and forced Fighting, sends
    one Recall and no removal). `a_seen_lurk_takes_passes_at_the_epoch_its_spot_carried` (a real
    `Ev::Seen` from a player's area; passes at the spot's epoch and the band's count, one of a later epoch
    is stale; the removal goes at the band's new epoch). `a_band_whose_bodies_die_at_the_camp_takes_its_
    lurk_down_once` (the camp harness: a band at a track with every body left behind, killed by
    CAMP_DEATH, ends through `member_died`'s `go_home`).
- **MZB-358** (news): an ambush makes no news. An unwatched one has no witness (CL-D61). A watched one is
  seen in the world by its watcher, and the engine books no first-hand report for it, as it books none for
  a puppet's blows. The owner learns from the short train and the roar (MZB-366). A rout of a lurking party
  goes through `rescue_news` as any rout, with M7's Track arm.
- **MZB-359** (the lurk table, core): `ConflictBridge` keeps `Long2ObjectMap<Lurk>` by agent, with
  `Lurk(long agent, int epoch, long faction, ResourceKey<Level> dimension, Vec3 track, Vec3 spot, float
  reach)`.
  - A new AMBUSH_SPOT replaces the agent's lurk (latest wins). REMOVED drops it and releases a live puppet
    (MZB-361).
  - The table is cleared at session end.
  - The create plugin reads it through `RtsimApi.lurks()` (an unmodifiable view) and
    `RtsimApi.lurkBody(long agent)` (the live puppet, or null).
  - Amendment (built, step 5): the table is a `LinkedHashMap<Long, Lurk>` by agent (iteration in begin
    order), read through an unmodifiable view. It is dropped by agent, never by epoch (a `seen` lurk's passes
    carry E against the party's E+1, MZB-371's list). It is cleared by `sessionStarted`, `sessionStopping`,
    `sessionFailed`, `serverStopped` and `switchedOff`, and `idle()` counts it and the pending roars, so
    the bridge does not sleep with either. The lurk's dimension is read from the names dictionary without
    the level, so a spot for a level not loaded yet is kept. `lurkBody` answers a live puppet of a party that
    is not recalling.
  - Amendment (review C, item 4; compiled, not run): the bridge's `held` flag, which a switch-off reads to
    decide whether there is anything to clear, was `!byAgent.isEmpty() || ...` without the lurks and the
    pending roars. It is now `held = !idle()`, and `idle()` counts both (as the amendment above says it did), so
    a module switched off with only a lurk or a pending roar left still runs `switchedOff`, which captures the
    parties and calls `clearLurks()`. `trainCalled` returns at once when the conflict module is off (a module
    that is off plays no roar for a train arriving). The `floors` map (a remembered floor y by agent) is
    deleted with its three removals: nothing ever put into it or read it.
- **MZB-360** (placement): a stack of a lurking agent's BAND_MATERIALIZE is placed at the spot, not on the
  arc.
  - Its y comes from a scan from track y + 4 down to track y - 8 for a block with a sturdy top face
    (`isFaceSturdy(level, pos, Direction.UP)`) and the body's box free above it
    (`level.noCollision(type.getSpawnAABB(x, y, z))`). If none, `getHeight(MOTION_BLOCKING_NO_LEAVES, x,
    z)`.
  - Then `restrictTo(spot, 2)` and the hold (MZB-361). A puppet already live when AMBUSH_SPOT comes is held
    at once and walks to the spot.
  - Amendment (built, step 5): a lurking agent's stacks are anchored at the spot in `materialize` (the
    anchor on the spot, offsets zeroed, the heading toward the track), not walked there. BAND_STEER is
    ignored while the agent has a lurk. A new AMBUSH_SPOT for a live party re-anchors its stacks, and
    releases and re-holds its puppets only if the spot moved.
  - Amendment (the re-check's P2, the spot contract covered one geometry only; compiled and the JUnit run, the
    GameTests not run): the engine's spot (`spot_toward`: 5 whole blocks along the line from the track point
    to the den) is 5 from the rail and square to it only for a rail along an axis and a den square to it.
    For a rail on a diagonal a due-north den puts it 3.5 off the rail; a den off the line by 4 puts it 4 off;
    a den within a block of the line gives 5 along +x, which is on the rail for an east-west one. The numbers
    of MZB-365's contract are about the body's distance to the rail's centre line, so Java now places the
    body and the engine's spot only says which side of the rail:
    - Core `RailAxes` (`Vec3 at(ServerLevel, Vec3 point)`: the rail's horizontal direction at the point, any
      length or sign, or null), registered with `RtsimApi.registerRailAxes` (kept in `LurkSpots`). The core
      cannot name Create; the create plugin registers `RailDirections`. Nothing registered, or null: the
      engine's spot with the floor scan above, as before.
    - `LurkGeometry.candidates` (pure doubles, so JUnit runs it: `LurkSpotsTest`) returns block centres, best
      first. A body's centre can rest only on a block's centre (the navigation ends a wide body's path there,
      the wide-body fix in MZB-361's amendment), so the point is the block centre whose distance to the rail's
      centre line is within `LATERAL_SLACK` (0.4) of `SPOT_BLOCKS` and whose offset along the rail from the
      track point is at most `ALONG_MAX` (1.5; the strike does not care where along the rail), nearest to both
      first, ties by x then z. On the den's side first (the sign of the spot's offset from the line along the
      normal `(-az, ax)`), then the other. A spot within `ON_LINE` (0.5) of the line takes the left-hand
      normal first. Why 0.4: the block centres of a diagonal's side lie on lines `1/sqrt 2` = 0.707 apart,
      so the nearest is up to 0.354 off for a line that misses the lattice (a JUnit test sweeps the track
      point's fraction). Create's own rails run through block centres, so the real figures are exact: 5.0
      for XO and ZO, 4.95 (7/sqrt 2) for PD and ND.
    - `ConflictBridge.holdPoint` takes the first candidate that is standable (`standAt`: the column's chunk
      ticks entities, a block with a sturdy top face between the track's y + 4 and y - 8 with the body's box
      free above it, no heightmap fallback) plus the slot's offset in its stack. It is used at spawn
      (`spawn` makes the body there: no walk) and by `holdLurks` for a body that has no hold point yet.
      `Slot.holdAt` is set at spawn; `Slot.place` and `Slot.lag` use it, so `ConflictFollowGoal` (LEASH
      6.0) does not walk a member back to the engine's spot. `ConflictKit.lurkHold(agent)` reads it (GameTests only; it was on `RtsimApi` until the second re-check).
    - Refused: neither side has a standable candidate. At spawn `spawn` returns false, the stack is tried
      again after `RETRY_FAILED` (100) ticks, and `lurksRefused` counts each; at `holdLurks` the body is
      despawned and the stack waits `REQUEUE_COOLDOWN` (200). There was no refusal before: the choice is
      new (a lurk with no floor beside the rail costs the party its ambush, as a stack with no floor costs
      it its placing).
    - Create's `RailDirections` scans the 3 x 3 x 3 blocks about the track point for `ITrackBlock`
      pieces (`TrackShape.getAxes()`: XO, ZO, PD, ND, the slopes and the portal shapes have one axis, the
      `CR_*` crossings two) and takes the line through a block's centre nearest the point, its horizontal
      direction; null when the nearest line is farther than `ON_RAIL` (0.75), when a crossing is within it,
      when two lines of different directions are as near as each other (`TIE` 0.05), or the chunk is not
      loaded. Known gap: a bezier curve's middle has no block of its own, so a lurk on a curve keeps the
      engine's spot (the contract holds there only as far as the engine's spot happens to be square to
      the rail); a lurk at a curve's end piece uses that piece's axis.
    - Tests (written and compiled, not run): `LurkSpotsTest` (JUnit, run: nine cases: square, oblique, on
      the line, both diagonals over a sweep of fractions, order, no axis), `FrostmawGameTests` (an oblique
      den, a den on the line, the den's side without a floor, neither side with a floor then the south laid
      again: four tests with a stub `RailAxes` for an east-west rail), `RailDirectionsGameTests` (real
      track blocks of each shape), and test 9's three variants in `RailFrostmawGameTests` (MZB-370).
    - Forks: the hold point is the nearest block centre within 0.4 of 5.0, not exactly 5.0 (a body cannot
      rest between blocks); "refused" is as above; the diagonal is covered by the JUnit geometry and the
      provider's GameTest, not by a pass of test 9 on a diagonal rail (a diagonal fixture is a larger
      (the contract is for the first body).
  - Amendment (the second re-check's P3, embankments and cuttings, a curve's middle, a diagonal's corner;
    compiled and the JUnit run, the GameTests not run):
    - `standAt` takes a floor within `STAND_RISE` (2.0) of the rail's y, not the search of y + 4 down to y - 8:
      a floor whose top is 3 or more above or below the rail is the rim of a cutting or the crown of an
      embankment, a long walk from the rail and out of the strike's reach in the vertical (the swipe is 3D, and
      the gate's 6.0 horizontal absorbs |dy| up to 2.5 only). Such a column is no candidate, so the other side
      is tried, and with neither the lurk is refused as before. The no-axis path keeps `floorAt` (the heightmap
      fallback of M5's placing), since there is no rail to be level with.
    - With no axis known (a curve's middle, a crossing, no create plugin), `holdPoint` asks `RailAxes.trackNear(level,
      point, radius)` (a default method, false) about the engine's spot at the track's height, with the radius the body's
      half-width plus `LurkGeometry.RAIL_CLEARANCE` (1.5): 3.5 for the 4-wide Frostmaw, the least at which its box
      could reach the rail's clearance. A rail within that is refused (the same refusal as a side with no floor);
      none, and the engine's spot stands, so the no-plugin case works as before. Create's `RailDirections.trackNear`
      scans the blocks within the radius (centre to point, horizontal) and 3 blocks of height for an `ITrackBlock`
      or a `FakeTrackBlock`, the block Create fills a curve's span with (a bezier piece has an `ITrackBlock` only
      at its two ends). The refusal is a fork: the alternative, placing a body near a curve by the nearest track
      segment's direction, needs the bezier's tangent per segment (not read here), and a refused lurk is retried
      (`RETRY_FAILED`) and only costs the party its ambush at that point of the line.
    - A diagonal rail's centre line meets the corner of a square box, not its side, so the Frostmaw's 2.0 half-width
      is 2.83 against the rail. The candidate window is asymmetric for a rail not along an axis: far slack 0.4
      (unchanged), near slack `OBLIQUE_NEAR_SLACK` (0.17): 5.0 - 0.17 - 0.5 - 2.83 = 1.50, the clearance, by
      construction rather than by the 4.95 the block lattice happens to give (1.61 clear). `LurkGeometry.nearSlack(axis)`
      is the pick; an axis rail keeps the 0.4 both sides. Off Create's lattice (the track point not on the rail's
      line) the den's side may have no block centre in the narrower window; the other side, the mirror lattice,
      always has one (a JUnit sweep checks it), so the body goes there and the lurk is not refused for it.
    - Tests (written and compiled, not run): `LurkSpotsTest` (the sweep above, three diagonal cases, `nearSlack`),
      `FrostmawGameTests` (a cutting on the den's side stands the body on the other; no axis and a rail near is
      refused with the radius and point asserted, then the engine's spot once none is near), `RailDirectionsGameTests`
      (`trackNear` at a crossing, the corner of two lines, a fake track block, 3 blocks of height and bare ground).
  - Amendment (the second re-check's P3, a refusal after the claim; compiled, not run): `spawn` computed the hold point
    only in the branch with no camp body to hand off, and after `reconciler.claim`. A refusal there left the SET_OUT
    it had claimed (the current slot's, when the claimed body was not a `Mob`) marked done for the tick, and the
    hand-off branch never asked for a hold point at all: a camp Frostmaw a player can see was handed to a puppet,
    which `holdLurks` then found no point for and discarded (its camp body already removed with its records on a
    marker). Now `spawn` asks `holdPoint` first for every slot of a lurking party, and refuses the whole stack
    (`lurksRefused`, the made bodies discarded, the earlier slots' claims released, `RETRY_FAILED`) before it claims a
    camp body. The SET_OUT is then unclaimed, and the tick's end resolves it as the take it is for a line no
    materialize claimed (§4.9: the engine gets fresh puppets when the party is seen later). The camp body a player
    can see is still removed: taken, with its records on a marker, not handed to a puppet that is then discarded
    unseen. (The third check found the first wording of this amendment, "nothing discarded", wrong.) The residual is
    what a take loses against a hand-off: the body's health and crystal state (the fresh puppet starts whole); the
    engine takes any found body of a `Left` slot whose party is not mustering (camp.rs, the census reconcile), so
    only an engine check of the hold point before the set-out would leave the body in its camp. That is backlog; M8
    makes no engine change. A hand-off with a lurk
    sets the hold point at spawn (the puppet starts where the camp body stood and the hold walks it to the point).
    `holdLurks` keeps its own refusal for a body that stood before its lurk was known (or whose spot moved), a body
    a player may see as well; it despawns and requeues as before (the lurk arriving after a placed party is not a
    case the engine makes: AMBUSH_SPOT precedes BAND_MATERIALIZE). Fork: the take instead of the hand-off is the
    only outcome a refused hold can have, since the camp body cannot both stay in the camp and be handed over; the
    alternative, keeping the SET_OUT claimed and answering it later, would hold the camp's count open for the
    retry. Test (written and compiled, not run): `MowzieGameTests.aLurkRefusedAtItsPlacingClaimsNoCampBodyAndTheSetOutIsATake`
    (no hand-off made, one taken, the camp holds nothing, no puppet stands, the refusal counted).
  - Amendment (the third check's P3, `trackNear` refused falsely; compiled and the JUnit run, the GameTests not run):
    `RailAxes.trackNear` took the point at the track's height and looked 3 blocks above and 3 below it, so a body that
    stands on an embankment or a bank 2 or 3 above a lower line (or beside a lower line on a cutting's far side) was
    refused for a rail it could not touch. It is now `trackNear(level, point, radius, height)`: `point` is the body's
    feet, as `holdPoint` asks it (the floor `floorAt` finds at the engine's spot, no longer the track's y), and the
    blocks that count are those from the feet's block up to `height` above it (the body's height: 4.0 for a Frostmaw);
    a rail below the feet or above the head is not near. Radius unchanged (half the width and the rail clearance, 3.5).
    `RailDirections.NEAR_HEIGHT` is gone. For the flat ground the tests lay the floor's top is the track's y, so the
    point is the same one as before. Tests (written and compiled, not run): `RailDirectionsGameTests` (a rail at the
    head's block counts, one above the head, one below the feet, and the fixture's own track 3 blocks under the feet,
    do not), `FrostmawGameTests` (the point and the height asked).
- **MZB-361** (Lurker): a new core interface, registered per entity type id with
  `RtsimApi.registerLurker(String entityType, Lurker l)` (an id string, as `BodyKinds.registerStandIn`
  takes, so it can be called in the plugin's constructor before the registries freeze). The core looks
  it up by `BuiltInRegistries.ENTITY_TYPE.getKey(mob.getType())`:
  ```java
  public interface Lurker {
      void hold(Mob mob, Vec3 spot);      // each server tick while it lurks
      boolean strike(Mob mob, Vec3 at, Vec3 motion); // start a strike at a carriage point that moves by motion a tick; false if busy
      void release(Mob mob);              // the lurk ended or the body left it
      void roar(Mob mob);                 // a train it robbed is gone
  }
  ```
  - The core calls `hold` on `ServerTickEvent.Post`, after every level ticked, for each live lurk puppet,
    and `release` when the lurk ends or the puppet is recalled. A type with no Lurker gets the core's
    default: the restriction and the path rule below, no strike, no roar.
  - The Mowzie plugin's `FrostmawLurker`, `hold`:
    - `setAlwaysActive(true)` and the restriction once;
    - beyond 1.5 blocks of the spot and in `NO_ANIMATION`, `getNavigation().moveTo(spot.x, spot.y, spot.z,
      1.0)`; within 1.5, `getNavigation().stop()`;
    - while a strike's swing runs (`getAnimation() == SWIPE_ANIMATION` and `getAnimationTick() <= 9`), it
      sets `yBodyRot`, `yRot` and `yHeadRot` to the strike's bearing.
  - `strike`: only in `NO_ANIMATION`. It records the bearing, turns the body, and calls
    `AnimationHandler.INSTANCE.sendAnimationMessage(frostmaw, EntityFrostmaw.SWIPE_ANIMATION)`. The bearing
    is toward where the point will be when the swing lands (MZB-365's second amendment), so it takes the
    point's motion.
  - `release`: `setAlwaysActive(false)` and `clearRestriction()`.
  - `roar`: in `NO_ANIMATION`, `sendAnimationMessage(frostmaw, EntityFrostmaw.ROAR_ANIMATION)`. The jar
    plays its own roar sound in that animation.
  - Calling the jar's own animation is what MOWZIE.md §4.4 asks; the hold is new because the restriction
    does not hold an awake Frostmaw with a target (see the jar facts).
  - Amendment (built, steps 5 and 7): the hold runs in `ConflictBridge.endOfTick` after the placings. A
    slot's hold spot (`Slot.holdAt`) is set at its first hold from the placed x and z and the floor scan
    (MZB-360). The core's `Lurkers.DEFAULT` restricts to the spot once (radius 2) and walks back beyond 4
    blocks, strikes nothing and roars nothing. `FrostmawLurker` also sets `yBodyRotO` with the bearing and
    keeps the strike's bearing in a `WeakHashMap`; it stops its navigation in any animation, so a swipe or a
    roar is not walked out of. Registered in the plugin's constructor as
    `RtsimApi.registerLurker("mowziesmobs:frostmaw", new FrostmawLurker())`.
  - Amendment (review C, item 3; the GameTest is written and compiled, not run): the strike's bearing was
    removed only in `release`, so it outlived its swing. `hold` pins the body to it during any plain swipe up
    to tick 9, which includes the Frostmaw's own swipes at a player (the jar's `AnimationAreaAttackAI.tick`
    turns the body to its target by up to 30 degrees a tick until the hit frame, and its area attack reads
    `yBodyRot`): after one strike by a train, every later plain swipe at a player was held to the old
    carriage's bearing and its 135 degree arc missed the player. Now `hold` removes the bearing as soon as the
    animation is not a swipe, or is past `SWIPE_HIT_TICK`. A swipe-twice is not pinned and never was.
    `aStrikesBearingDoesNotTurnTheFrostmawsOwnLaterSwipes` (mowziesmobs, batch `frostmaw_lurk_bearing`): a held
    puppet strikes at a point east of it (yaw -90), the swing ends, a player (1000 health) stands 4 blocks west,
    and at tick 8 or 9 of the first plain swipe at the player the body's `yBodyRot` is more than 90 degrees from
    the strike's. Expected on the unfixed code: it stays at -90, and the test fails with "the swipe at the player
    turned away from the strike's bearing -90.0 (yBodyRot -90.0, 0.0 degrees off it)" (pinned at the hold, or 30
    degrees on at the worst before it). The 90 degree line is a guess at how far the jar's lookAt gets the body
    in eight ticks: the 180 degree geometry makes the unfixed code fail by 60 degrees or more, and a fixed body
    short of it would fail the test falsely; if the round shows that, lower it.
  - Amendment (review C, item 4, D1; compiled, not run): the walk-back called `getNavigation().moveTo(spot)`
    at every hold while the body was beyond `STAND`, a path search a tick for a body that was already on its
    way. Now `hold` calls `moveTo`
    only when the navigation is done, or its `getTargetPos()` is more than a block (squared distance 2.0) from
    the spot's block, so one search serves the whole walk. A private `stop(Mob)` stops the navigation in the
    swipe, other-animation and within-`STAND` branches as before (behaviour unchanged) and counts a live path it
    drops (`nav.isDone()` false before the stop). `FrostmawLurker` keeps two counters, `searches` and `dropped`, and the core `Lurker` gains two
    default methods, `pathSearches()` and `pathsDropped()` (zero for a Lurker that keeps no count), so the create
    project's test reads them through `Lurkers.of(mob)` without naming a Mowzie class. The counters are
    plugin-wide, not per body; the one test that reads them differences them over its hold. Test 9 logs the
    held puppet's path searches and dropped paths per second (MZB-370's item 4 amendment). The body's own
    goals search besides, and the jar's navigation (`MMPathNavigateGround`) may restart a path of its own: the
    counters measure what the hold asked for, not the pathfinder's total. No threshold: a count, not a check.
  - Amendment (the build lead's spot-distance choice, with a wide-body fix found writing it; compiled, not run):
    - `STAND` is 0.5 (it was 1.5): the contract in MZB-365's fourth amendment. `Lurker.stand()` returns it.
    - The walk-back aimed the navigation at the spot, and a wide body does not end a path on its target. A path's
      last node is the box's corner and vanilla puts the body's centre `(int) (width + 1) * 0.5` past it
      (`Path.getEntityPosAtNode`): 2.5 blocks for the 4-wide Frostmaw, against a spot at a block's centre. So a
      walk back to the spot ended 2.0 blocks east and 2.0 south of it (toward the rail where the spot lies north
      of the rail, as in test 9, and 2.8 blocks from the spot in any case). `hold` now aims at `spot - nodeOffset` on both axes (`(int) (width + 1) * 0.5 - 0.5`: 2.0 for the
      Frostmaw, 0 for a body under a block wide), with accuracy 0 (`moveTo(x, y, z, 0, speed)`, the node itself,
      not one short). The D1 test (no search while the navigation heads there) compares the navigation's target
      block with that aimed block.
    - A body that cannot get within `STAND` would search every tick. By my reading of the sources, not measured:
      the Mowzie navigation finishes a path when the body is within 0.5 on each axis of the node's position (up to
      0.71 off in a corner), its move control turns 7 degrees a tick (`new MMEntityMoveHelper(this, 7.0f)`) at a
      speed attribute of 0.3, so it does not stop on a point and 0.5 is below what a walk leaves it at. A walk-back
      is therefore searched at most once in `RESEARCH_TICKS` (10) ticks of one body (a game-time stamp by body, in
      a weak map, dropped in `release`), and the figure test 9 logs (searches per second) shows the rate.
    - Test 9 asserts where the body rests, not that it rests within `STAND` (MZB-370's amendment).
  - Amendment (the re-check's P1, the hold lost to the jar's chase; compiled, not run): the core's `aim` gives a
    held puppet the nearest player within 50 blocks, and the jar's `EntityFrostmaw.tick` then walks it to that
    target every tick it is more than 5.5 blocks off (`getNavigation().moveTo(target, 1.0)`, 0.95 in the slam),
    replacing the hold's path, while the hold looked again once in `RESEARCH_TICKS` (10): the puppet followed a
    player to the rail. Inside the stand the jar's `getNavigation().stop()` each tick cleared the hold's path
    and a fresh A* ran each tick. Fix: `EntityFrostmawMixin` wraps the three `PathNavigation.moveTo(Entity, double)`
    and the three `PathNavigation.stop()` calls in `tick()` (counted in the jar by javap and cfr, `require = 3,
    allow = 3`) with `@WrapOperation`; for a body that `FrostmawLurker.holds(f)` the chase is not made and the
    stop is not made (`moveTo` returns false, as the jar reads a failed path). `holds` is a game-time stamp the
    hold sets each tick it runs, good for 3 ticks (`HELD_FRESH`) and dropped in `release`, so a hold that lapses
    without a release (a plugin or lurk table reset) frees the jar's chase again within three ticks, and a body
    that is not held is never touched. While held the Frostmaw's `RandomStrollGoal` and `MoveTowardsRestrictionGoal`
    are taken out of its goal selector (kept with their priority in the body's `Lurker.Memo`, put back by `release`; the second re-check's P2 below moved them from a weak map): the
    stroll walks a body up to its restriction radius (2) from the spot's block, which is past `STAND` and toward
    the rail. Not taken out: the jar's attack animations and their turn toward the target (a swipe at a player in
    reach is the jar's own; the strike's bearing is MZB-361's amendment above). The 10 tick throttle stays: with
    the chase gated it only limits repeated tiny searches when the body rests just beyond `STAND` (the navigation
    ends a path within 0.5 on each axis, so a rest at 0.55 to 0.7 is possible; the round decides whether a rest
    within `STAND` is reliable, and the steady-stretch check allows `STEADY_PATHS` = 2 new paths).
  - Amendment (the re-check's P2; see MZB-360's): the `spot` the hold is given is the hold point, not the
    engine's spot (`Slot.holdAt`, set at spawn), so the restriction, the walk-back and the leash are about the
    place Java chose beside the rail.
    `aHeldPuppetWalksBackToItsSpotAndDoesNotChaseItsTarget` (mowziesmobs, batch `frostmaw_lurk_chase`): a held
    puppet with no crystal is moved 2 blocks north of its spot and set on a hovering 1000 health player 8
    blocks west (past the jar's 5.5 and 6.5); it must be within `STAND` of the spot within 200 ticks, never
    nearer the player than the spot is less `STAND`, and then for 100 ticks stay within `STAND` and be given at
    most 2 new paths (a new non-null `getPath()` instance, polled each tick). Expected on the unfixed code: the
    jar's chase runs nine ticks in ten and the puppet walks west: "back within 0.5 blocks of its spot in 200
    ticks (... the closest it came to the spot ...)" fails first, or if it stands the later checks (nearest
    approach, new paths) fail. The figures are recorded (`back`, `steady`).
  - Amendment (the second re-check's P2, the dodge leap; compiled, not run): a player within the jar's 4-block
    dodge range dealing 16 damage (the jar sums `damage`, a sum that loses 1 every 7 ticks), or any lava damage,
    starts `DODGE_ANIMATION`, and at its tick 6 the jar does `setDeltaMovement(m.x, 0.6, m.z)` with `m` the body's
    motion plus 1.7 blocks a tick at `dodgeYaw`: a 10 to 14 block leap off the spot, which the six chase calls did not
    cover. Choice (a fork the lead may flip): wrap the call, do not block the dodge. `EntityFrostmawMixin` wraps the
    `EntityFrostmaw.setDeltaMovement(DDD)V` calls of `tick()` (`require = 2, allow = 2`: the dodge's, cfr line 430, and
    the asleep branch's, line 592, which a held body never runs), and for a body `FrostmawLurker.holds` it passes the
    body's own current x and z and the call's y (0.6): the body hops in place and lands where it was, and the jar's
    animation state stays what the jar has it (the dodge runs its 15 ticks and ends on its timer, its
    `wantsToIceBreathAfterDodging` is still consumed, the dodge cooldown is set). Blocking the dodge instead would
    need a wrap of `hurt` to keep the jar's `shouldDodgeMeasure` and `wantsToIceBreathAfterDodging` from being set
    on some of its paths and not others, and a lava hurt starts the dodge before the measure is read. Two tests
    (mowziesmobs, batches `frostmaw_lurk_dodge_player` and `frostmaw_lurk_dodge_lava`): a held puppet without a crystal is hurt once
    (20 damage by a player 3 blocks west whom it targets; 1 damage of lava), the dodge is seen to pass tick 7, and for
    40 ticks it is never more than `STAND` from its hold point. Expected on the unfixed code: "the puppet never left
    its stand while it dodged: strayed 11.9 of 0.5" (or more).
  - Amendment (the second re-check's P2, `taken` leaks and pins the server; compiled, not run): the plugin kept the
    walking goals it took out of a held body in a `WeakHashMap<Mob, List<...>>`, and each goal holds its body (the
    goal's `mob` field), so the value reached the key and the entry never cleared: a puppet that left the world by any
    way but a release (`despawn` at a requeue, a recall or `capture`, or a death settled after the body was removed)
    was held with its level until the server stopped. Fix (the core `Slot` keeps it, as `stripped` is kept):
    `Lurker.hold(mob, spot, memo)` and `Lurker.release(mob, memo)` get a `Lurker.Memo`, a one-object holder the bridge
    creates at a slot's first hold and keeps in `Slot.lurkMemo`; `FrostmawLurker` keeps its `Taken(goals)` record in
    it and empties it in `release`. The bridge empties the memo in `releaseLurk`, which now asks the Lurker for every
    body it holds, removed from the world or not (it skipped removed ones), and `despawn` calls it first, so a
    discarded body's stamp, bearing and search time (weak maps whose values do not reach the body, which stay) go too.
    The interface change touches only `FrostmawLurker` and `Lurkers.DEFAULT` (nothing else implements `Lurker`).
    `FrostmawLurker.tracked(mob)` (a stamp, a bearing or a search time) is the GameTests' view. Tests: the module-off
    test also asserts `!tracked(body)` after the discard, and `aKilledHeldPuppetLeavesNothingInTheLurker` (batch
    `frostmaw_lurk_killed`) kills a held puppet, waits for the casualty and the removal, and asserts that nothing is
    tracked and the bridge has no member for it. A session stop proper is not made in a GameTest (the session is
    shared); the module switched off runs the same `capture`.
    - Correction (the third check's P3): the killed test does not fail on 86f97f99: `settleDeaths` released the dying
      body while it was still in the world (`isDeadOrDying`, not yet removed), so the old release ran. It stays as a
      guard. The leaks are driven by `aHeldPuppetRemovedBySomethingElseLeavesNothingInTheLurker` (batch
      `frostmaw_lurk_discarded`: a held body discarded from outside, `left` with reason DISCARDED, `gone`, `forget`, whose
      release was skipped for a removed body: fails on 86f97f99 by reading, the plugin still tracking the body) and by
      the module-off test (a session stop's `capture`, whose `despawn` never asked the Lurker on 86f97f99: fails there
      by reading). A requeue (`holdLurks` refusing, `despawn`) takes the same `despawn`. None has run; `tracked` is new
      in this range, so the old build does not compile them and "fails before" is a reading of its code.
  - Amendment (the third check's P3, the second `noLeap` wrap and lava; compiled, not run): the wrap on
    `EntityFrostmaw.setDeltaMovement(DDD)V` matched both calls of `tick()` (`require = 2, allow = 2`); the second, cfr
    line 592 (bytecode offset 2954), is the asleep and waking branch's `setDeltaMovement(0, y, 0)`, which stills the body
    and which a hold wants too, and a held body that is waking (the ACTIVATE animations) does run it. Wrapping it kept
    the body's own horizontal motion through a stilling. Now the wrap is pinned to `ordinal = 0` (the dodge's, offset
    1017, the first of the two by javap of the 1.8.2 jar), `require = 1, allow = 1`. Fork: this gives up the earlier
    "a jar with any other count fails to load" (a third call, or the dodge's call moving behind another, is no longer
    caught); the alternative, two wraps with an ordinal each, would keep a check on the second at the cost of a
    second hook that does nothing, and the pin is the tighter match. And a held body in lava (`isInLava()`) is let leap:
    the original velocity goes through whole, so the dodge carries it out of the lava instead of hopping it in place
    until it burns. The lava dodge test hurts the body with a lava damage source, not in lava, so it still dodges in
    place; a body standing in lava has no test.
- **MZB-362** (the target): the restriction gates targeting (M6's MZB-279), so a restricted Frostmaw never
  picks a player up itself. Every 20 ticks of the hold, a puppet with no target gets
  `level.getNearestPlayer(TargetingConditions.forCombat().range(50), mob)` (its follow range).
  - Amendment (built, step 5): before taking the nearest player, the rule drops a target that is dead, in
    another level, or beyond `OUT_OF_VIEW` (64 blocks), so a held puppet is not left on a player who left.
- **MZB-363** (TRAIN_PASSED from the sampler): at each sample, for each lurk in the train's dimension, the
  segment's distance to the track point at or within `reach` sends TRAIN_PASSED.
  - It is sent once per approach, and armed again when the train is beyond 64 blocks of the point.
  - `RtsimApi.trainPassed(long agent, int epoch, boolean watched, int carriage, long train)` writes it with
    `beginJournaled`, carrying the table's epoch.
  - Amendment (built, step 6): the distance is three-dimensional, from the track point to the sample's
    segment (the rig's `LurkTable` is horizontal), so a rail above or below a lurk is not beside it. A
    train's approach to a lurk is `Approach{lurk, armed, struck}` in a map by agent, made when a pass or a
    strike first needs one. It is valid only while its `lurk` is the table's instance (identity), so a lurk
    begun again by its agent (a replay resend, a moved spot) starts armed; entries of lurks that are gone or
    replaced are pruned at each 20-tick rebucket, and an entry is dropped when the train is beyond 64
    blocks. The sampler now runs when a lurk waits even if no territory minds the rail
    (`anyMinding(SIGNAL) || !lurks().isEmpty()`), and clears its near set with its states. The sampler sends a
    pass the moment a sample meets the lurk and keeps no pending pass, so REMOVED (the table entry gone) stops
    the next sample's pass at once, and the approach left over is pruned within a second (MZB-371's list: the
    rig's drop of a pending pass). `RtsimApi.trainPassed` needs the
    session up and returns whether it wrote.
- **MZB-364** (watched): `PASSED_WATCHED` when `lurkBody(agent)` is live. `carriage` is the index of the
  carriage whose leading point is nearest the track point, computed only when sending.
- **MZB-365** (near a puppet): a train whose last sample is within 64 blocks of a live lurk puppet joins a
  near set, rebuilt at each of its 20-tick samples, and is read every tick.
  - Each carriage's leading and trailing points are read. A point within 6.5 blocks (horizontal) of the
    puppet calls `RtsimApi.strike(agent, point)` once per carriage per approach.
  - The swipe moves no cargo: the cargo comes from the TRAIN_PASSED roll. It is real for riders, since the
    jar's area attack strikes living entities in its arc.
  - Amendment (built, step 6): the near set holds a train when the segment of its sample is within 64
    blocks of the lurk's track point and the lurk has a live puppet (the puppet stands within the restriction
    of the spot, a few blocks from the track point); the design's wording was "of the puppet". A
    refused strike (a busy puppet) does not mark the carriage struck: it is tried again each tick while the
    point stays in reach, so a carriage is struck at most once and a refusal loses nothing (the design
    marked it on the call). `RtsimApi.strike` picks the nearest held puppet of the agent. The near set drops a
    train that is invalid, derailed or without a graph.
  - Second amendment (found writing test 9, step 8; the design's geometry could not land a blow). The swing
    lands at animation tick 9, so 9 or 10 ticks after the call, and a carriage point at 1.0 blocks a tick has
    moved 9 or 10 blocks by then. The jar's area attack (`AnimationAreaAttackAI.hitEntities`) takes living
    entities whose 3D `distanceTo` (feet to feet) is within range + half their width (6.5 + 0.3 for a zombie),
    then keeps those whose horizontal distance less half their width is within 6.5 and whose bearing is within
    67.5 degrees of `yBodyRot`. With the design's spot 5 blocks off the rail and a strike at the first tick
    the point is 6.5 blocks away (along the rail 4.15 from the closest approach), the point is 4.85 past the
    closest approach when the swing lands: 6.96 blocks horizontally, and 84 degrees off the bearing the body
    was turned to. Both miss. Three changes:
    - `Lurker.strike(mob, at, motion)` and `RtsimApi.strike(agent, at, motion)` (and `ConflictBridge.strike`)
      take the point's displacement a tick, and `FrostmawLurker` turns to `at + motion * 9`
      (`SWIPE_HIT_TICK`), where the point will be at the landing. `TrainSampler.motionOf` is the track's
      direction at the point (a 0.25-block step along its edge, never across a node) times `train.speed`, zero
      for a stopped train.
    - The test's spot is 4 blocks off the rail (`LATERAL`), so the point is within the swing's range when it
      lands: 5.6 to 6.3 blocks horizontally, from a strike at the first tick within 6.5.
    - The test's rider sits on a seat directly on the first bogey (rail + 2), the least a seat can be, which
      keeps the 3D distance short (about 6.1 to 6.7 against the limit of 6.8 at worst). The figures are
      predictions: nothing has run. If the round shows the landing a tick off, `SWIPE_HIT_TICK` and
      `LATERAL` are the two numbers to move.
    A real player on a train passing a lurk at speed is struck in the same way; a train at a standstill in
    reach is struck at once, with a zero motion.
    (The last sentence was false as built; see the third amendment: the gate looked at where the carriage was,
    not where it would be.)
  - Third amendment (review C, item 2; written and compiled, not run): the gate and the aim at speed. As built,
    the strike fired when a bogey point was within 6.5 blocks (horizontal) of the puppet now, and aimed at that
    point plus its motion. At 1.0 blocks a tick the swing lands 9 ticks later with the carriage 9 blocks on. The
    engine's spot is `SPOT_BLOCKS` (5) off the rail (rail.rs: whole blocks toward the den, written as block +
    0.5), so a point first in reach at 6.5 is 4.15 along from the closest approach and lands 4.85 past it: 6.96
    blocks horizontally from a puppet 5 off, out of the swipe's 6.5. The test passed only because it laid the
    spot 4 off (`LATERAL`), which the engine never does. A real player at speed was not struck. Now:
    - The gate is on the landing point. `TrainSampler.landingOf` is where a bogey point will be `SWING_TICKS`
      (9) ticks on at the train's present speed: Create's own `TravellingPoint.getPositionWithOffset(graph,
      speed * 9, false)` while the point stays on its edge (curves included), and straight on from the edge
      end's tangent (`TrackEdge.getDirectionAt`) past an edge end. A carriage is struck when that landing is
      within `STRIKE_REACH` = 6.0 blocks (horizontal) of the puppet: the swipe's 6.5 less what its 3D
      feet-to-feet filter (6.5 + half the width, so 6.8 for a zombie) takes for a rider seated about 2 above the
      puppet's floor (6.0 horizontal is about 6.3 in 3D). A cheap first test (`STRIKE_REACH + 9 * |speed| + 4`
      plus the carriage's `bogeySpacing`, from a bogey point now) skips the carriages that cannot be in reach, so
      the landing is computed only for the few that can.
    - The aim is at a living rider if the carriage carries one (`CarriageContraptionEntity.getPassengers()`, the
      first that will be in reach; it is taken to move as the bogey nearer to it does), since the swipe hits
      living entities and a bogey point misses a rider at the middle or the rear of a long carriage. A carriage
      with a living rider that will not be in reach is not struck. A carriage with no living rider is struck at
      its leading bogey's point, else its trailing one's. The motion passed is the landing less the position,
      over 9, so the plugin's `at + motion * 9` is the landing.
    - The prediction follows the track within one edge. Past a node it goes straight on, and a bend beyond the
      node makes the aim err by about the part of the 9 ticks past the node squared over twice the bend's radius
      (a train at 1.4 blocks a tick covers 12.6 blocks in the swing; a tight Create curve is a few blocks of
      error). That is the recorded bound, not a fix: the swipe's 67.5 degree arc and its 6.5 reach absorb part of
      it. A train that changes speed during the swing (a station stop, a signal) errs by the change. Neither is
      tested.
    - Test 9 runs at the engine's own spot, 5 blocks off (`LATERAL` 5), at 0.5, 1.0 and 1.4 blocks a tick
      (`rail_frostmaw_lurk_slow`, `rail_frostmaw_lurk`, `rail_frostmaw_lurk_fast`: the same body with the speed
      passed in; the slow and the fast skip the bait, the hold's checks and the hearer, which the 1.0 run
      covers), each asserting that the rider is hit. Expected on the unfixed code: the 0.5 run probably hits
      (the swing lands 4.5 on, 0.35 past the closest approach: 5.0 horizontal), while the 1.0 and 1.4 runs miss
      (landing 6.96 and about 9.8 horizontally) and fail with "the zombie was hurt at <speed> (health 1000.0
      of 1000.0)". Nothing has run.
  - Fourth amendment (the build lead's choice after review C; written and compiled, not run): the spot-distance
    contract. The gate's 6.0 left a puppet resting 1.5 off its spot (the old `STAND`) on the far side from the
    rail 6.5 from the track: inside the jar's reach (a target is hit when its horizontal centre distance less its
    half-width is at most 6.5, so a player at about 6.8 from the Frostmaw's centre) and outside the gate, so such
    a puppet never struck. The choice, reversible: keep `STRIKE_REACH` at 6.0, which leaves about 0.5 for the
    prediction's error (see the correction below), and shrink the hold's `STAND` to 0.5 (MZB-361's amendment). The rule is one check in two
    inequalities, `TrainSampler.spotContract(stand, halfWidth)`:
    - `SPOT_BLOCKS + stand <= STRIKE_REACH`: 5.0 + 0.5 = 5.5 <= 6.0, a margin of 0.5.
    - `SPOT_BLOCKS - stand - halfWidth >= RAIL_CLEARANCE`: 5.0 - 0.5 - 2.0 = 2.5 >= 1.5, a margin of 1.0.
    `TrainSampler.SPOT_BLOCKS` (5.0) mirrors rail.rs's constant beside `STRIKE_REACH`, with a comment naming
    rail.rs, and `RAIL_CLEARANCE` (1.5) is the clearance test 9 already asserted. The Rust side pins its own
    number: `the_spots_distance_is_the_one_the_java_plugin_mirrors` (`crates/rtsim-conflict/tests/lurk.rs`;
    compiled and run, it passes), whose message names what else moves with it. `Lurker.stand()` (default
    `Lurkers.STRAY`, 4.0, a body that never strikes) exposes the number so create's test reads it without naming
    a Mowzie class. Test 9 in every variant records the figures and fails if the rule does not hold for the
    Lurker's stand and the live body's half-width (`getBbWidth() / 2`); `LATERAL` is `(int)
    TrainSampler.SPOT_BLOCKS`, so the test lays the engine's spot by construction. A fourth variant pins the
    puppet at the far side of its stand and asserts the strike at 1.4 blocks a tick (MZB-370's amendment).
    A Lurker of another body kind registers its own `stand()`, and its own gate if its reach differs.
  - Correction (the re-check's P3): the margin the 6.0 gate leaves is about 0.5, not 0.8. The jar's area attack
    (`AnimationAreaAttackAI.hitEntities`) has two cuts on a target: the horizontal centre distance less its
    half-width at most 6.5 (6.8 for a 0.6-wide target), and the 3D feet-to-feet distance at most 6.5 plus its
    half-width (6.8). A rider is about 2 above the puppet's feet (a seat on the bogey, or a floor one block above
    it), so the 3D cut allows sqrt(6.8^2 - 2^2) = 6.5 horizontally, and the binding cut is the 3D one. The gate's
    6.0 leaves 0.5 for the prediction's error. The
    contract's own inequalities are unchanged; it is the room the gate has for the prediction that is smaller
    than quoted.
  - Amendment (the re-check's P2; compiled, not run): the contract is about the hold point's distance to the rail's
    centre line, which Java now sets (MZB-360's amendment), within `LurkSpots.LATERAL_SLACK` (0.4) of
    `SPOT_BLOCKS`. `spotContract` counts the slack: `SPOT_BLOCKS + slack + stand <= STRIKE_REACH` (5.0 + 0.4 + 0.5
    = 5.9 <= 6.0) and `SPOT_BLOCKS - slack - stand - halfWidth >= RAIL_CLEARANCE` (5.0 - 0.4 - 0.5 - 2.0 = 2.1 >=
    1.5). The slack is the worst case; Create's rails put the body at exactly 5.0 (XO, ZO) or 4.95 (PD, ND), so
    the real far side is 5.5 (5.45). A lurk on a curve keeps the engine's spot (no axis there) and so only the
    engine's own geometry.
  - Amendment (the re-check's P2, a standing rider was ignored; compiled, the GameTests not run): `aimAt` took
    only the carriage's seated passengers (`getPassengers()`) as riders, so a living entity standing at the middle
    or the rear of a long carriage was no rider: with no seated one the carriage was struck at its leading bogey's
    point, up to a carriage length ahead of the entity, and the swing missed it. Now `TrainSampler.ridersOf(entity,
    body)` returns, in order, the seated passengers; the living entities in the carriage's `collidingEntities`
    (Create's server collision registers mobs and the like that stand on it, each for three ticks); and the living
    entities found by a search about the carriage whose feet stand on a block of it, by Create's own test (the
    position 0.2 below the feet, in the carriage's frame, is a key of `getContraption().getBlocks()`;
    `ContraptionCollider.worldToLocalPos`, which its client mixin uses). `aimAt` tries them in that order and takes
    the first that will be in reach when the swing lands; a carriage with any rider that will not be is still not
    struck, and one with none is struck at a bogey point as before.
    - Why a search and not `collidingEntities` alone (the brief's mechanism): a real player standing on a carriage
      is never in it on the server. `ContraptionCollider.collideEntities` reaches `registerColliding` only after
      `if (playerType == PlayerType.SERVER) continue;`, and the only other caller is a mixin in
      `foundation/mixin/client`. Read in Create's sources, not run; the riders GameTest records whether the player
      is in the set. Mobs are in it, so they are covered either way.
    - The search box is a circle about the contraption's anchor (`Contraption.bounds` is in the carriage's frame;
      the entity's own bounding box is not turned with it, so it is not used) of the bounds' largest horizontal
      reach plus `RIDER_MARGIN` (3.0), and as high as its largest vertical reach plus 3.0. Who counts is Create's
      `AbstractContraptionEntity.canCollideWith` (no spectator, no `noPhysics`, nothing already riding, no hanging
      entity, seat or projectile). The aim moves as the bogey nearer to the rider does, as for a seated one.
    - Cost: one entity search per carriage per tick while it is in the cheap range of a lurk's puppet and
      unstruck (about twenty ticks a pass at one block a tick), against a box a few blocks across. Not measured.
    - Known gap: a player standing at the edge of a platform beside a carriage is not a rider (his feet are over no
      block of it); one standing on a carriage that Create has not yet told the server of is, as the server
      sees his position. A bystander mob that stands on the carriage (a villager in it) is a rider and the carriage
      waits for it.
  - Amendment (the second re-check's P3, an armor stand was a rider; compiled, not run): `ridersOf` took any
    living entity, and an `ArmorStand` is one that collides with a contraption (it is no seat, projectile or hanging
    entity), so one standing on a carriage drew a strike off the bogey point and a carriage with one was not struck at
    its bogey when the stand was out of reach. A rider is now a living entity that is alive and a `Mob` or a `Player`
    (`TrainSampler.rides`), in all three of its sources. Nothing else in vanilla is a living non-mob except the
    stand; a mod's decoration that extends `Mob` is a mob by every other rule here.
  - Amendment (the second re-check's P3, a rider on the next carriage, a rider's height, a diagonal's corner;
    compiled, not run):
    - `strikeAt` looks at the riders of every unstruck carriage in the cheap range before it aims at any bogey. A
      first pass takes each carriage's riders (once, kept) and strikes the first whose landing is in reach; a second
      pass strikes a carriage that has no rider, at its bogey, only when the first struck nothing. Before, the
      loop went carriage by carriage, so a riderless carriage in front took the swing at its bogey (the puppet then
      busy for the swing) while a rider on the next carriage, in reach a tick later, was refused. The loop returns
      at the first strike that starts: the puppet is busy, so a second would be refused. A carriage with a rider that
      is not in reach is still not struck.
    - A rider's reach is the swipe's own, in 3D: `TrainSampler.reachesRider` takes the landing point's distance to the
      body's feet in three dimensions against `SWIPE_RANGE` (6.5) plus half the rider's width (6.8 for a player),
      the jar's 3D cut. The horizontal `STRIKE_REACH` (6.0) stays for a bogey point, which has no height of its own.
      So a rider on a carriage floor 2 above the body, or the body 2 below on a bank, is struck when 3D-near, and
      one the horizontal gate would have passed at 6.0 and the 3D cut refuses (|dy| above 3.2) is not.
    - `spotContract(stand, halfWidth, diagonal)`: for a diagonal rail the box's corner meets the rail, so the reach is
      `halfWidth * sqrt(2)` and the near slack `OBLIQUE_NEAR_SLACK` (MZB-360's amendment); the rule holds by
      itself, 5.0 - 0.17 - 0.5 - 2.83 = 1.50 >= 1.5 (it rested on the 4.95 the lattice gives before). Test 9 asserts
      it for an axis rail and for a diagonal.
    - The ordering of the two passes had no test: it needs a train of two carriages, one with a rider, in front of a
      puppet, which the riders test's one-carriage line does not lay (the third check's P3 adds one: the next
      amendment).
  - Amendment (the third check's P3, the bogey pass, the gate and the test of the order; compiled, not run):
    - `strikeAt` skips its second pass (the bare bogeys) whenever any carriage in the cheap range has a rider, struck
      or not in reach: a riderless carriage 0's bogey must not spend the swing (`SWING_TICKS`, the puppet busy) while
      carriage 1's rider is still outside `riderAim`'s window and will be inside it a tick or two on. A train none of
      whose gated carriages has a rider is struck at its bogeys as before.
    - The gate is built from the longest reach: `far = SWIPE_RANGE + 1.0 + SWING_TICKS * |speed| + BODY_SLACK`, not
      from the horizontal `STRIKE_REACH`, so a rider at a long carriage's overhang (up to 6.8 in 3D, a block or so
      more than the bogey's 6.0) is not gated out before `riderAim` is asked.
    - Test (written and compiled, not run): `RailFrostmawGameTests.theSwingGoesToARiderOnTheNextCarriageBeforeABareBogeyInFront`
      (batch `rail_frostmaw_riders_first`). A stopped train of a short carriage and a long one with a plank floor
      (`mixedCars`); the puppet two blocks past carriage 0's bogey, 5 off the rail. A zombie (AI on, goals out, feet
      on the plank) first on the far plank of carriage 1, out of the swipe's 3D reach with the carriage gated in: no
      strike in the 40 ticks after the sampler has seen the train (fails with the bogey pass left running). Then on the
      near plank, in reach: the puppet's yaw at the first strike is within 20 degrees of the bearing to the zombie
      and more than 20 off the bearing to the bogey, the two being 35 or more apart (fails on the carriage-by-carriage
      order of the second re-check, where the bogey of carriage 0 is struck first). The premises (the bogey in reach,
      the planks' 3D distances against the range, the bearings apart) are checked in the test.
- **MZB-366** (the roar at a station):
  - TRAIN_ROBBED adds `(train, faction)` to a per-session pending map in `ConflictBridge`, capped at 256
    with the oldest dropped.
  - `TrainMixin`'s existing TAIL injection on `arriveAt` (`mic_rtsim$arrived`) also calls
    `TrainSampler.arrived(train, station)`, after `TrainWatch.arrived`, which returns early without an
    industry or district. No new injection.
  - `TrainSampler.arrived` calls `RtsimApi.trainCalled(long train, ServerLevel level, BlockPos station)`,
    which returns at once for a train not pending.
  - For a pending train, every player within 64 blocks (PLACEHOLDER) of the station hears the camp's sound,
    placed as `play`'s notices: from 32 blocks toward the den (`NOTICE_DISTANCE`), at `NOTICE_VOLUME`.
    `play` is split so both use one `playToward(faction, dimension, x, z, player)`.
  - The entry is dropped only once a player heard it, so an arrival with nobody near keeps it for the next.
  - No chat, action-bar or title line.
  - Amendment (built, steps 5 and 6): `trainCalled` is public on `RtsimApi` and plays to non-spectator
    players that are not `FakePlayer`s within 64 blocks (three-dimensional) of the station. The level comes
    from `ServerLifecycleHooks.getCurrentServer()` and the station's dimension. A camp with no sound still
    gets the raid horn (`playToward`'s fallback), so the roar needs a declared camp with a sound
    (MZB-367).
- **MZB-367** (the den's sound): `DenKind.sound()` returns `MMSounds.ENTITY_FROSTMAW_ROAR`. So a den's
  notices roar too, instead of the raid horn: both are in-world sounds.
- **MZB-368** (the puppet roars): on TRAIN_ROBBED, a live lurk puppet of that faction is told `roar`.
  - Amendment (built, step 5): only held puppets (a live slot with a hold spot) of lurks of that faction
    are told; a puppet not yet placed hears nothing, and the pending entry stays for the station.
- **MZB-369** (UR-1 and D1): puppets are never saved (M3), and the lurk table and the pending roars are per
  session, so nothing stays in the world. The sampler's lurk work is a list scan per sample over the lurks
  (at most one per lurking party). The every-tick reads are for trains within 64 blocks of a live puppet
  only, and the hold is one call per live puppet a tick.
  - Amendment (review C, item 1; the GameTest is written and compiled, not run): "nothing stays in the world"
    was false for a take. `gone(keepBody=true)` (a player's Carry On pickup, or anything else's take) left the
    body in the world as an ordinary mob, and `forget` cleared the slot's hold spot without asking the
    `Lurker`, so the body kept `restrictTo(spot, 2)` and its `alwaysActive` true for good (it is in the
    Frostmaw's NBT), which breaches UR-1. `forget` now calls `releaseLurk(sl)`, which asks the Lurker to undo
    the hold and clears the spot, and `take` returns before Carry On saves the body, so the saved NBT is the
    released one. The other paths that clear a hold or drop a slot were read and need nothing: `despawn`
    (a requeue, a recall, `capture` at a session's end or the module's switching off, a hand-back) discards
    the body, so no hold outlives it (the second re-check found the Lurker's own state did: `despawn` now calls
    `releaseLurk` first, MZB-361's last amendment); a death or a removal by something else leaves a dead or removed body
    (`forget` now releases a dying one too, which is harmless); a recall or a moved or ended lurk already
    called `releaseLurk`. (Corrected by the re-check's P3; the first draft said a body unloaded with its chunk
    is saved held and the join check discards it as a stale copy. A held body is never saved: it carries the
    core's `mic_rtsim:agent` tag, and `EntitySaveMixin` makes `shouldBeSaved` false for a tagged entity, which is
    the filter every chunk save applies, and `saveAsPassenger` false, so a body riding something or saved by
    `Entity.save` is left out too. Its chunk is written without it.)
    Three GameTests in the mowziesmobs project (`FrostmawGameTests`, written and compiled, not run), sharing one
    fixture (`Held`: a one-unit Frostmaw band lurking at a spot, held), because the brief made this a UR-1 break
    and so its test must uninstall and reinstall:
    - `aLurkPuppetTakenByAPlayerIsLetGoLikeAnyOrdinaryFrostmaw` (batch `frostmaw_lurk_taken`, the mock player's
      `PICKUP`) and `aLurkPuppetTakenByNoPlayerIsLetGoLikeAnyOrdinaryFrostmaw` (batch `frostmaw_lurk_taken_other`,
      `TakeReason.PLUGIN`, no player). At once after the take: no puppet, no restriction, not always-active,
      `alwaysActive` false in the saved NBT, and the same of a body made from that NBT. Then the engine's
      BAND_CASUALTIES (by player, or by other) is waited for, `requestSaveAtNextStep` has it acknowledged, and the
      body's `mic_rtsim:pending` record drops (it is the one thing of mic_rtsim on the body until then, so the
      first save is checked for the hold only). The uninstall is the module switch the other GameTests use
      (`RtsimConfig.Test.setModule(Module.CONFLICT, false)` and `BridgeEvents.moduleSwitchesChanged`, as
      `IndustryGameTests` and `ModuleSwitchGameTests` do; no GameTest removes a mod, and none is named for an
      uninstall. The uninstall emulation is the NBT round trip below, not the switch: a taken body has left the
      bridge, so the module going off discards nothing of it, and the saved-NBT checks stand for the mod's
      removal (the re-check's P3)):
      the saved NBT of the acknowledged body has no `neoforge:attachments` key of `mic_rtsim:`, no
      `NeoForgeData` key and no tag with `mic_rtsim`, no word of `mic_rtsim` anywhere in its string form, and
      `alwaysActive` false, which is what a world without mic_rtsim and the plugin reads (NeoForge skips an
      unknown attachment; there is none to skip). The body stays after 40 ticks with the module off, alive and
      ordinary (no restriction, not always-active, not the bridge's). The reinstall is the module on again and a
      copy of the saved NBT, with a new UUID, added as a chunk would add it: after 60 ticks both bodies are
      ordinary Frostmaws, nothing is held by the lurk (`lurkBody` null), and the lurk is ended and leaves the
      table. The mock player (`PICKUP`) is not a `FakePlayer`, so the casualty is by a player.
    - Expected on the unfixed code, both: the first check to trip is the saved NBT's, "the taken body, before the
      engine has the casualty,: alwaysActive false in its saved NBT" (it is true there). This replaces the first
      draft's "the taken body has no restriction (true)", which was the first to trip on the unfixed code and so
      stood in front of the UR-1 check (the re-check's condition 2; see the amendment below).
    - `aLurkPuppetHeldWhenTheModuleSwitchesOffLeavesNothingInTheWorld` (batch `frostmaw_lurk_off`) is the other end
      of the uninstall, a puppet still held when the module goes off: the body is discarded, the lurk has no body,
      and 60 ticks after the module is on again no Frostmaw stands within 24 blocks of the spot. It passes on the
      unfixed code too (a guard that the release does not change what the switch-off does, which discards the
      body). The test restores the switch when it ends, passed or failed.
  - Known gap, outside M8 (the re-check's P3; recorded, not fixed): a held puppet is kept out of every chunk save,
    but `EntitySaveMixin` leaves `saveWithoutId` alone on purpose, because a dimension change copies an entity
    through it and the copy must keep its tag so the join check knows it. So a third party that captures a
    mob by calling `saveWithoutId` on it directly (a capture item that does not go through `Entity.save`, which
    the mixin does cover) would write the held body's NBT, `alwaysActive` true among it, and the released body
    would come out of that item holding. Carry On's pickup does not: `take` releases the hold first (above). Left
    open for the dimension copies; closing it means telling a dimension copy from a capture in `saveWithoutId`,
    which nothing in M8 needs. The same goes for any other plugin's puppet kind.
  - Amendment (the re-check's condition 2; the test is rewritten and compiled, not run): the take test's checks
    run in the order of the claim they make. A body made from the saved NBT is what a world without the mod
    reads, so the saved NBT is saved and read first (`h.saveOrdinary`, then the copy's `alwaysActive`), then
    the live body's (no restriction, not always-active). The copy's "has no restriction" check is dropped: a
    restriction is never in NBT, so it was true on any code and checked nothing. The expected failing value on
    the code before the fix is the saved NBT's `alwaysActive` ("the taken body, before the engine has the
    casualty,: alwaysActive false in its saved NBT"). The fail-first run is a worktree at 875ac2bd (the head before
    review C) with only the test file changed, on the throwaway branch `test/m8-ur1-failfirst`, for the lead to run
    with `gametest.sh mowziesmobs`; the two take tests are the ones expected to fail there, the
    `..._LeavesNothingInTheWorld` one to pass.
- **MZB-370** (test choices):
  - Test 8's rider is `helper.makeMockPlayer(GameType.SURVIVAL)` added with `addFreshEntity`, as M6's thief
    (MZB-281), so the ice ball's `getEntitiesOfClass` sees it. It is seated with `addSittingPassenger`.
    It is never a goal target (`players()` only), so the test sets the Frostmaw's target. The fallback is
    M6's `FakePlayers.addSeen`.
  - Test 9's struck rider is a seated zombie: living, in the entity manager, seated as a player is.
  - Both tests script the engine side: AMBUSH_SPOT, BAND_MATERIALIZE, TRAIN_ROBBED and IND_CARRIAGE_DELTA.
    The roll and the cooldown are Rust-tested.
  - `FakePlayers.Hearer` gains `sounds` (each `ClientboundSoundPacket`) and `titles` (title, subtitle and
    action-bar packets). `hearing` builds its `ServerGamePacketListenerImpl` as a subclass whose
    `send(Packet<?>)` records them.
  - Amendment (built, step 5): `ConflictKit.trainsPassed(agent)` reads the TRAIN_PASSED records the
    scripted engine received (the bridge keeps no per-agent counter), and `lurkStrikes()` the bridge's
    strike counter.
  - Amendment (built, step 8; nothing run): `RailFrostmawGameTests` has one fixture, `Line` (the far chunks,
    the cleared box and floor, a 150-block track, the stations, the train, and its restoring), which test 10
    runs on unchanged in behaviour, and four tests in four batches.
    - Test 8 is two tests. `anIceBallUnseatsARiderFromACarriage` (batch `rail_frostmaw_ice`) is the static
      form. `theIceBallHitRatesAgainstAMovingTrain` (batch `rail_frostmaw_ice_rates`, 16,000 ticks) is the
      moving form, split off because its 16 passes and their cooldowns take thousands of ticks and the
      static form should not wait on them, and because it is a measurement: it logs
      `[gametest] ice ball v=<v>: <hits>/4` and fails only if the rider is on two vehicles. The design's "within
      60 ticks of the freeze ending the rider has no vehicle" is a wait for no vehicle, with the freeze itself
      failing the test if it holds the rider 200 ticks (the jar's is 50).
    - The riders are health 1000, so a swipe or a freeze cannot kill one. The mock player is a plain `Player`
      (`GameTestHelper.makeMockPlayer`: not a `ServerPlayer`, `isLocalPlayer` true), added with
      `addFreshEntity`; `FakePlayers` players (the bait and the hearer) are in `level.players()` only. The
      Frostmaw's target is set again at every poll (the restriction gates targeting, MZB-362, and a target
      once dropped is not found again), in the hold phase too.
    - Test 9 lays its lurk 40 blocks ahead of the train on the 150-block line (not a 96-block straight with
      the lurk at its middle), its second station 60 blocks along from the first on the side the train goes,
      the spot 4 blocks off (MZB-365's second amendment), and the camp's sound is the registry holder
      `mowziesmobs:frostmaw.roar` (the plugin's `DenKind.sound()` is a `DeferredHolder`; the create project
      names no Mowzie class). The hold's checks are: within 2 blocks of the spot, the near edge at least 0.5
      off the rail's centre line (the spot's is 2.0; the Frostmaw stands within 1.5 of it, and away from the
      rail while it has the bait), `alwaysActive` true, a target at some poll. The bait walks 0.05 blocks a
      tick away from the rail, 30 blocks off to begin with.
    - The yBodyRot check is the puppet's `yBodyRot` against the bearing from the puppet to the zombie at the
      first poll that finds the zombie's health down (the swing landed in the tick before, the zombie moved
      at most a block: 14 degrees at 4 blocks off, inside the 30 allowed).
    - The roar part is in the other order than the design's: TRAIN_ROBBED, then `Train.arriveAt` the second
      station with nobody within 64 blocks (nothing plays; the entry stays), then `arriveAt` the first with
      the hearer 20 blocks off (one sound packet, the roar's, and nothing in `raw`, `overlay` or `titles`;
      the entry gone). The entry goes once a player has heard it, so the arrival with nobody near has to be
      first. `Train.arriveAt(GlobalStation)` is public and safe to repeat, and is what `TrainMixin` hooks, so
      the test drives the real path without moving the train. The hearer stands 20 blocks behind the first
      station, away from the lurk, so it is about 61 blocks from the puppet (not a target, 50; out of the
      puppet's own sounds) and 80 from the second station.
    - Amendment (review C, item 2; written and compiled, not run): test 9 is three tests, one body at three
      speeds (MZB-365's third amendment). Its spot is 5 blocks off the rail, the engine's own `SPOT_BLOCKS`, not
      the 4 above (which the engine never lays), and the train runs at 0.5, 1.0 and 1.4 blocks a tick with
      `manualTick` and a set `speed`, so it moves exactly that a tick. Each asserts that the rider is hurt. The
      1.0 run (`rail_frostmaw_lurk`) keeps the whole test; the other two (`_slow`, `_fast`) skip the bait, the
      hold's checks and the hearer and end after the TRAIN_PASSED check. The rider is still the seated zombie, so
      the 3D filter is the one MZB-365's third amendment gates on.
    - Amendment (review C, item 4; written and compiled, not run):
      - Test 9's clearance gate is a near edge at least 1.5 blocks off the rail's centre line (it was 0.5). The
        puppet's edge is its distance to the centre line less 2; the spot is `LATERAL` = 5 off, so the edge is 3.0
        at the spot and 1.5 at the nearest the hold lets it stand (within `STAND` = 1.5 of the spot). The margin at
        that nearest legal stand is nil, so a puppet pushed to the rail side of its stand fails exactly on the
        line; it normally drifts away from the rail with the bait as its target, and the wall of the test is the
        one place a failure would show it.
      - Test 9's hold phase logs the hold's path searches and dropped paths per second
        (`Lurkers.of(puppet).pathSearches()` and `pathsDropped()`, differenced over the hold; MZB-361's D1
        amendment). A record, not a check; the first run's figure is the baseline.
      - Test 8's moving half (`theIceBallHitRatesAgainstAMovingTrain`) now fails when no pass at the slowest
        speed (0.5 blocks a tick) froze the rider, with "at least one pass at 0.5 blocks a tick froze the rider
        (0/4, balls thrown at it: n)". It stays a measurement for the other speeds. This is a new gate on a
        test that had none, so there is no earlier failing value to quote; the first run decides whether one pass
        in four at the slowest speed is a fair floor.
    - Amendment (the build lead's spot-distance choice; written and compiled, not run), superseding the 1.5 stand
      in the item 4 amendment above:
      - With `STAND` at 0.5 the near edge is 2.5 at the nearest the hold lets the body rest (5.0 - 0.5 - 2.0),
        against the 1.5 clearance: a margin of 1.0, where the 1.5 stand had none. The clearance check reads the
        body's width (`getBbWidth() / 2`) and `TrainSampler.RAIL_CLEARANCE`.
      - Every variant records the spot, stand, half-width, reach and clearance, and fails if
        `TrainSampler.spotContract(Lurkers.of(puppet).stand(), halfWidth)` is not null, or if `LATERAL` is not the
        sampler's `SPOT_BLOCKS`.
      - The full test also records where the puppet rests 60 ticks after the bait goes, and fails if that is more
        than `STRIKE_REACH - SPOT_BLOCKS` (1.0) from the spot: the strike gate's margin, not `STAND`, because the
        body may rest beyond its stand (MZB-361's amendment). That check can fail on the first run; if it does,
        the figure it prints is what the hold really leaves, and the contract's numbers move to it.
        (Superseded by the re-check's P3: the check is that the body rests within `Lurkers.of(puppet).stand()` of
        its hold point, the rest point of the `Den`, which is what the contract counts on and what `STAND` 0.5
        makes the hold keep; the 1.0 bound let a body rest 0.5 beyond the stand and still pass, and the
        contract's far side is the stand's. The figure is still recorded, with the strike gate's margin beside it.)
      - `aLurkingFrostmawAtTheFarSideOfItsStandStillSwipesAtOnePointFourBlocksATick` (batch
        `rail_frostmaw_lurk_far`, 2400 ticks): the 1.4 pass without the bait, the hold's checks or the hearer, with
        `LurkRun.pin()` setting the puppet at `stand - 0.001` from the spot on the line away from the rail, with no
        horizontal motion, at the rider's setup and every poll until the zombie's health first drops. It asserts
        what the other passes do: a strike, the zombie hurt, the bearing within 30 degrees, no carriage box in
        the puppet's, the block counts unchanged, one TRAIN_PASSED. With the old stand (1.5) the contract check
        fails first ("the far side of the stand is 6.5 blocks from the track point, past the strike reach (6.0)"),
        and without it the pinned puppet, 6.5 from the rail, would see the landing no nearer than 6.5 and never
        strike ("the lurk struck at 1.4 (0)"). With the stand at 0.5 the landing comes to 5.5 at the nearest,
        inside the gate.
    - Amendment (the re-check's P2; written and compiled, not run): test 9 takes a `Den` (where the den lies, so
      the engine's spot) and a rest point (where the hold must stand the body): `SQUARE` is the earlier test
      (spot 5 north, rest the spot); `OBLIQUE`, the den 4 east and 4 north (spot +4, -4; rest 5 north of the
      track point); `ON_LINE`, the den on the rail (spot 5 along +x; rest 5 south, the left-hand side). Every
      run checks that `ConflictKit.lurkHold(agent)` is the rest point and the body was made within its stand of it.
      The hold phase's stray, clearance, pin and bait use the rest point (the bait is placed on the body's side
      of the rail, `away()`). Two new tests for the oblique den (`rail_frostmaw_lurk_oblique`, speed 1.0, and
      `_oblique_far`, 1.4 with the body pinned at the far side of its stand) and one for the den on the line
      (`_online`, 1.0); they skip the bait, hold and hearer as the other short runs do.
    - Amendment (the re-check's P2, a standing rider; written and compiled, not run): test 9 takes a `Rider`
      (`SEATED`, the earlier tests; `MIDDLE` and `REAR`), and `Line` a `longCar` flag. A long carriage
      (`longCars`) is one carriage of two small bogeys `LONG_SPAN` (8) blocks apart on the station's track, a
      floor of seven oak planks on the blocks between them, controls on the first bogey, all glued, and no
      second carriage: a second one behind it would be struck at its own bogey point, which the old code could
      also land on a rider at the rear (it passes 5 blocks behind the zombie), and the test would not tell the
      old code from the new. Create's assembler needs the first bogey at the first block past the station, bogeys
      at least 3 apart, and the second in the first's glue group; read from `StationBlockEntity.assemble`, not run.
      - `Line.stand` puts a zombie on the plank in the middle (`MIDDLE`) or the one next to the rear bogey (`REAR`),
        planks ordered along the way the train goes, at the plank's top (plus 0.01 before the second re-check), and checks that Create's own
        standing test finds a block of the carriage under it. The zombie is a mob because Create's server
        collision carries mobs and does not carry a player. The drive ramps up to the speed over ten ticks from
        the tick the zombie is put on, so a jolt from rest does not leave it behind; if it still slips, the first
        failure is the hit check, not a setup check, which is a defect of the test.
      - `aLurkingFrostmawSwipesARiderStandingAtTheMiddleOfALongCarriage` (`rail_frostmaw_lurk_standing_middle`)
        and `...AtTheRearOfALongCarriage` (`rail_frostmaw_lurk_standing_rear`), speed 1.0, the engine's own spot,
        pass only (no bait, hold or hearer): the same assertions as the seated pass (a strike, the zombie hurt,
        the bearing within 30 degrees, no carriage box touching the puppet's, the block counts unchanged, one
        TRAIN_PASSED). On the code before the fix the strike is aimed at the leading bogey's point, 5 (middle) or
        8 (rear) blocks ahead of the zombie, so the zombie's landing is about 9.7 or 12.4 blocks from the
        puppet when the gate opens (the point's is 6.0): out of the swipe's 6.5, and the carriage is marked struck, so the expected failure is "the zombie was hurt
        at 1.0 (health 1000.0 of 1000.0)".
      - `aStandingPlayerAndAStandingMobAreRidersOfACarriageAndABystanderIsNot` (`rail_frostmaw_riders`): a mock
        player (a plain `Player`, the kind Create's server skips) and a zombie stand on the stopped long
        carriage's floor, a second mock player 3 blocks beside it; `TrainSampler.ridersOf` returns the first two
        and not the third. A player is not carried by Create, so the train does not move in this test; the
        moving tests use a mob.
    - Amendment (the second re-check's P2 and P3s in the riders and standing tests; compiled, not run):
      - The riders test's mock players fell: a mock player is a plain `Player`, which Create's server collision does
        not hold up, so it fell through the carriage in the five idle ticks before `ridersOf` was read, and the
        bystander too. `Line.stand` now gives a player `setNoGravity(true)` and zero motion (the bystander likewise).
      - A zombie standing on a carriage (the riders test and the `MIDDLE` and `REAR` passes) was `noAi`, which has no
        travel and so no gravity, and hovered 0.01 above the plank where Create may never have registered it as
        colliding. `Line.stand` now leaves a mob's AI on with every goal and target goal removed (it walks nowhere
        and attacks nothing), and puts its feet exactly on the plank's top, so gravity presses it onto the floor as
        it does a real rider and Create carries it. The seated zombie stays `noAi` (it is a passenger).
      - The bystander stands level with the floor 1.6 blocks beside the player on it (it was 3.0), 1.1 past the
        plank's edge, and an armor stand stands on the floor with the player: `ridersOf` returns neither.
      - The second re-check's P3 tests of placement are listed in MZB-360's amendment (a cutting, a curve's middle,
        `trackNear`); the spot contract is checked for a diagonal as well as an axis rail (MZB-365's amendment).
      - The chase test (`aHeldPuppetWalksBackToItsSpotAndDoesNotChaseItsTarget`) fails when the puppet is back later than
        `BACK_TICKS` (200): the wait ended whenever it was back, within the test's own longer timeout, so the budget
        was named in the message and never enforced.
    - Cleanup: a recall (RECALL_DONE) and a wait for BAND_DEMATERIALIZED on a pass, a removal and a recall
      on a failure; the train, its glue, the track, the stations and the box are put back as they were
      (`TrainWatch.forget` too).
  - Amendment (the third check's P3; written and compiled, not run): three test changes. (1)
    `RailFrostmawGameTests.theSwingGoesToARiderOnTheNextCarriageBeforeABareBogeyInFront` (batch
    `rail_frostmaw_riders_first`; MZB-365's amendment) on a new fixture, `Line.mixed` / `mixedCars` (a short
    single-bogey carriage in front of a long one with a plank floor), `Line.planks` and `Line.standOn` (the riders
    test's `stand` is now `planks` and `standOn`). It reads the strike's aim off the puppet's yaw, which the Lurker sets
    at the strike, and the absence of a strike off `lurkStrikes`; the sampler's sight of the train off TRAIN_PASSED.
    (2) `FrostmawGameTests.aHeldPuppetRemovedBySomethingElseLeavesNothingInTheLurker` (batch
    `frostmaw_lurk_discarded`) drives the leak the killed-puppet test does not reach (MZB-361's third-check
    correction): a held body discarded without the death path, the memo gone from the Lurker after. The killed-puppet
    test passes on 86f97f99 (`settleDeaths` released first) and is kept as a guard, its "fails before" claim dropped;
    the discard test and the module-off test fail there by reading (`FrostmawLurker.tracked` is new). (3) The
    `trackNear` assertions of `RailDirectionsGameTests` and the curve test of `FrostmawGameTests` take the body's
    height and the floor-up count (MZB-360's amendment).
- **MZB-371** (run 1's ambush half):
  - The synth's rail lines also run as booked legs in a FakeWorld industry through `IndustryLink`, so
    `raid_train` finds the leg and its cargo.
  - Its Java emulation keeps the lurk table from AMBUSH_SPOT. It sends TRAIN_PASSED when a train of a line
    passes within 8 blocks of a lurk's track point, at the tick its timetable puts it there.
  - Amended at step 4 (the build, `rtsim-cli/src/sim/mowzie/lurk.rs` and `rtsim-conflict/src/synth.rs`):
    - **The legs.** `synth::rail_legs(lines, seed, jitter, to)` is the list `rail_events` is now written
      over (the events are byte-identical for a line with `trains: 0`). `RailLine.trains` (0 = a fresh train
      id per leg, as in M7; n = n trains that take the legs in turn, train `k` going again `DWELL_TICKS`
      (600) after it arrives, so a train never overlaps its own leg). A leg runs out on the line's path and
      back on the next lap of its train, and carries its `RailLeg { train, depart, duration }`.
    - **The table's lag.** Java hears each command `LAG` (100) ticks after the engine says it (one command
      drain, a placeholder). A pass in the few ticks between a lurk's end and Java hearing of it is the one
      stale TRAIN_PASSED the engine may count, so "no pass is stale except just after a lurk ended" is checked
      against the lurk's end, not against the table: a stale pass must fall in `[end, end + LAG]`. (With only
      the lower bound, a table that kept a removed lurk's passes, by not dropping them on REMOVED, still
      passed with 4953 stale passes at 12 legs a day and 12238 at 24; with the upper bound it fails with
      those numbers.)
    - **`synth::pass_tick(lines, leg, spot)`** is the first sampler tick (every 20 ticks, at the trains'
      1.4 blocks a tick) at or after the polyline's nearest approach to the spot's track point, when that
      is within the spot's reach.
    - **The cargo.** Each line's two trains carry 1280 and 320 of one good a leg, so the Frostmaw's 64-item
      carry binds on the first (a tenth of 1280 is 128) and the industry's 10% theft cap on the second
      (32); two trains over a 2-day wait lose at most 64 + 32 each time they are robbed, 192 over the wait,
      under §4.4's 256. (At 640 and 320 the first train's tenth was 64, the carry itself, so a carry of 1000
      or no cap at all gave the same theft.) The run checks that every robbery takes `min(64, cargo / 10)`
      of its train, and that each train's robberies all take the one value. Mutated, the carry at 1000
      fails that check (59 of 119 robberies off at 12 legs a day, 48 of 96 at 24; the 1280-item train loses
      128), the day and wait limits too (128 over 64; a wait of 320 over 256); with no cap (`theft_cap_ppm`
      at 1,000,000) only that check fails (60 of 119, 48 of 96; the 320-item train loses 64).
    - **The share.** `ambush_milli` rolls on every pass, but the industry's per-train cooldown
      (`cooldown_ticks` 24,000) turns a hit into `attacks_skipped` when the train was robbed in the last
      day. So the share is judged on the first pass of a train in each day of a wait that the cooldown
      leaves open (not skipped, and the industry's own cooldown on the train over when the pass came: the
      rig reads it with `Engine::train_cooldown_until(train)`, the latest of the train's per-class
      cooldowns, so a robbery on a band's march, 16 of the 112 ambushes of the 24-a-day line, closes it
      too), pooled over the two trains and both lines and 8 seeds; the roll's own rate over every counted
      pass is checked against the same band as a second figure, and reported per line.
    - **The roll's independence.** The rate of the roll on the counted pass that follows a miss of the same
      lurk and train, and on the one that follows a hit, are each held to the same band (12 legs a day: 53%
      of 325 after a miss, 51% of 363 after a hit; 24: 49% of 437, 53% of 450). Mutated to a roll keyed by
      the train alone (`slot(train)` for `slot(train ^ at.rotate_left(32))` in the engine's roll), it fails
      at 0% of 254 after a miss and 100% of 434 after a hit at 12 legs a day, 0% of 297 and 100% of 590 at
      24; the share check fails too then (64%), but only by chance before.
    - **Legs a day.** The detail line says what the trains made: 11.9 legs a day on the 12-a-day line and
      23.9 on the 24-a-day line, so the two trains with a 600-tick dwell do keep the named rate (a
      line's lap is well under 1,800 ticks in the rig's scene).
    - **The three counts of a robbery.** The engine's `stats.lurk_robberies`, the TRAIN_ROBBED commands the
      rig heard (the train and the tick of each) and the industry's own `books.stolen` delta for each pass
      must agree pass for pass: the same trains at the same ticks, and the items the engine counted for a
      pass equal the ones the industry booked. (The check was the engine's counter against the rig's count
      of the engine's own command, one site: it could not disagree.)
    - **What the emulation does not model (review B; documented, no engine outcome moves).** Each
      divergence from MZB-363's sampler is a place where Java's pass can differ from the rig's by a few
      ticks or by one pass, never by a rule:
      - *Sample times.* `pass_tick` counts its 20-tick samples from the leg's departure; Java's sampler
        samples a train every 20 ticks of its own phase (the server tick), not of the departure. A pass
        can come up to 19 ticks earlier or later than the rig's.
      - *One pass per leg.* The rig sends one TRAIN_PASSED for a leg, at the polyline's nearest approach.
        Java re-arms an approach once the train is more than 64 blocks from the track point, so a leg that
        runs by the point, away beyond 64 blocks and back within reach would send two. A rig leg is a
        straight run and passes a lurk's point once.
      - *The dimension* is ignored: the rig has one, and Java skips a lurk in another dimension from the
        train's.
      - *No TrainArrived reaches the industry.* The legs are booked as departed and never arrive there, so
        a leg's cargo is not delivered and no stock moves except what a lurk takes. Cooldowns, theft caps
        and the books are the industry's own all the same.
      - *Curves.* The rig takes the polyline's nearest approach to the point against the spot's reach;
        Java tests the chord of each 20-tick sample against it. On a curve a chord cuts across the bend,
        so Java can read a nearer or a farther distance than the polyline's, and a point just inside or
        outside the reach can differ. The rig's lines are one straight run each (`straight(x)`), where the
        chord and the polyline agree, so no number here depends on it.
    - **What Java must match when it lands (steps 5 and 6; these do move engine outcomes).** The rig's
      table (`synth::LurkTable`, `hear` in `lurk.rs`) is the specification, and run 1's stale-pass check
      (a stale pass only in `[end, end + LAG]`) fails for a Java that does not:
      - *Drop by agent.* AMBUSH_REMOVED removes the lurk by agent, not by epoch. A lurk the engine saw
        (`seen`) has its passes carry epoch E against the party's E+1, and its REMOVED goes out at E+1.
      - *Latest wins.* A second AMBUSH_SPOT for an agent (a session start's resend, a watch's new epoch)
        replaces the first; the table holds one lurk for each agent.
      - *Clear at session end.* The table is per session and is empty at a new one; the engine sends each
        lurk again once (MZB-353).
      - *Drop the sampler's pending work on REMOVED.* The rig drops the agent's scheduled passes
        (`self.due.retain(|_, tp| tp.agent != s.agent)`, `lurk.rs`) as it hears the removal, and again on a
        new spot of the agent. In Java that is the sampler's approach state for the agent (MZB-363's
        `approaches`) and any pass the same sample was about to send: a removed lurk sends nothing from
        the tick Java hears it, and a lurk begun again by the agent starts armed.
- **MZB-372** (run 8): save and restore, blob and paged, at four points: mid-lurk, in a cowed time, in a
  meter's decay (a booked `Ev::Rail` and held passes), and with a reconcile in flight.
  - Each is compared with an uninterrupted control: the outbox (by bytes, in order), the stats, and each
    feud's grievance by `to_bits` must be identical.
  - A session restart at each point sends exactly one more AMBUSH_SPOT per lurking party, and the next
    census orders the reconcile's lines again (M4).
  - Amended at step 4 (the three lurk points are `lurk::trips`; the reconcile point is M4's
    `reconcile_in_flight`, unchanged), each run on seeds 0 and 1 of the 12-a-day line:
    - **Mid-lurk**: two dens' bands (the second den at (1200, 1000)) lie in wait at once, each with a
      pass behind it; if the second never lurks while the first does, one band with two passes will do.
    - **Cowed**: two dens lurk on the line; once the first den's band has a pass behind it, its chief is
      killed (`chief_killed`, as run 7 does). A day later, inside the 6-day cowed time, a Killing signal of
      twice the Raid rung's threshold provokes the first den's feud again, and the save or restart comes
      right after it (in that order, the signal and then the trip). The control must be cowed at the trip,
      and the second den must begin a lurk after it (the tail that is compared). The first den stays empty
      for the rest of the run, and that is the rig's limit, not the engine's: the rig founds its dens with
      `add_faction`, which makes no camp ledger (FACTION_FOUND is what makes it in the pack, `camp_found`),
      and `plan_arrival` (a renewal culture's next member: the Frostmaw has no recruits,
      `recruits_per_day` 0, so its next body is the renewal wait's, 6 days a first kill, 12 for a player's)
      returns early for a faction with no ledger, so no arrival is ever booked and the feud climbs, waits
      out the cowed time and blocks on the empty pool. Founding the dens by FACTION_FOUND would give them
      ledgers, but a ledger camp draws its bodies from the camp world (`bodies_ready` is BLOCKED while
      `drawable()` is short), so the rig would need the plugin's answers (run 6's `CampRun` has them);
      that is not a small change to this rig. (Without the provocation the first den's feud sits at Notice
      for the 56 days after, flat at its grievance: the meter's pressure alone does not climb back to
      Raid.)
    - **Decay**: the line runs 24 legs a day and is cut to 4 on day 7; the point is the first tick from
      then with a band lurking, a held pass and a booked rail event (about day 9.3 in seeds 0 and 1, with
      one lurk). So the meter is decaying and a lurk is in wait, its table and its passes among what is
      saved. No lurk begins after the trip (the meter no longer climbs), which is why the lurk is compared
      by the restart arms (one spot said again) and not by a new start. (The 12-a-day line cut on day 10
      had no lurk at its point: the claim then covered the meter and the held passes only.)
    - **The tapes.** Each arm leaves the same tape: the commands in wire order, the stats, the hourly
      grievance by bits, the lurk table's own commands (`lurk_wire`) and the state hash. Blob and Paged
      must equal the control's tape whole. The restart arm is compared by its lurk stream, its grievances and
      the lurk and pass stats (the command stream differs only by the one resent spot per lurking party,
      which is checked as `resent == lurking`). A second restart arm, `Trip::PagedRestart`, opens the engine
      from its pages and then starts the session, as the host does after a crash or a clean shutdown (a
      few lines on the same `restart` step); it is held to the same comparison and to no load drift.
- **MZB-373** (what M8 needs):
  - From M7: `Target::Track` and the arrival's Track arm, the sampler with its per-train state, and
    RAIL_PASS live.
  - From M6: `frostmaw.json`, the chief paths (a lurking chief can fall), and the plugin's mixin config.
  - From M3 and M5: the puppet machinery (BAND_MATERIALIZE, `Party`, the puppets never saved),
    `CampKind.sound()`, `DenKind`, and `FakePlayers.hearing`.
  - M6 adds no `alwaysActive` and no lurk restriction, so M8 adds `Lurker`.
- **MZB-374** (boundary.md): TRAIN_PASSED is live (the "come later" list, boundary.md:1752). AMBUSH_SPOT's
  spot y is the track's, with Java finding the floor. TRAIN_ROBBED's roar is a sound only.
  - Amendment (review C, item 4): boundary.md's AMBUSH_SPOT section now states the spot-distance contract:
    rail.rs `SPOT_BLOCKS` (5.0) against `AMBUSH_REACH` (8.0, the TRAIN_PASSED radius only), the sampler's
    `STRIKE_REACH` (6.0, horizontal, on the landing point `SWING_TICKS` = 9 ticks on) and the Lurker's stand
    (1.5 at the time; now 0.5, below) and restriction radius (2); the margin is 1.0 at the spot and was
    nil-to-negative at the far side of a stand of 1.5, so the numbers move together. Its TRAIN_ROBBED section says who hears the station roar: a non-spectator
    player in range who is not a NeoForge `FakePlayer` (`ConflictBridge.trainCalled` skips those; the
    GameTests' `FakePlayers` hearer is a plain `ServerPlayer`, so it hears).
  - Amendment (the build lead's spot-distance choice): boundary.md's contract paragraph is rewritten as the
    checked rule of MZB-365's fourth amendment: `SPOT_BLOCKS + stand <= STRIKE_REACH` and `SPOT_BLOCKS - stand -
    halfWidth >= RAIL_CLEARANCE`, with the stand 0.5, `SPOT_BLOCKS` mirrored in the sampler and pinned in Rust,
    and the body's real rest looked at by the GameTest rather than assumed.
  - Amendment (the re-check's P2): boundary.md's AMBUSH_SPOT section says where Java stands the body (the rail's
    normal through `track`, `SPOT_BLOCKS` off the rail's centre line, the spot giving the side; refused when
    neither side is standable; the engine's spot when no rail direction is known), and the contract's two
    inequalities count `LATERAL_SLACK` (MZB-360's and MZB-365's amendments). The wire is unchanged: `spot` is
    still the engine's point.
  - Amendment (the second re-check's P3): boundary.md's contract block takes the diagonal's corner (`halfWidth *
    sqrt 2` and the near slack 0.17), the floor within 2 of the rail's y, and the refusal when no direction is known
    and a rail is within the body's half-width plus the rail clearance of the engine's spot (MZB-360's and MZB-365's
    amendments). The wire is unchanged.

### M8 design for the implementer

**Where it stands.** Design only, on `design/mowzie-m7` with M7's (from 761f5d0f). Nothing of M8 is
written. Open questions for Ben: none.

**Jar facts** (Mowzie's Mobs 1.8.2 with cfr in `target/m7-cfr/mowzie`, `EntityMagicEffect` in
`target/m7-cfr-magic/out`; Create 6.0.10 in `target/m7-cfr/create`; vanilla from the NeoForm sources jar in
`target/m7-vanilla`):
- The swipe:
  - `EntityFrostmaw` registers `new AnimationAreaAttackAI<EntityFrostmaw>(this, this, SWIPE_ANIMATION, null,
    null, 2.0f, 6.5f, 6.0f, 135.0f, 1.0f, 9)` (212): range 6.5, height 6, arc 135 degrees, damage frame 9.
  - `AnimationAreaAttackAI.hitEntities()` (57) reads `this.entity.getEntityLivingBaseNearby(this.range,
    this.height, this.range, this.range)`, a `List<LivingEntity>` (`MowzieEntity.getEntityLivingBaseNearby(double
    distanceX, double distanceY, double distanceZ, double radius)`, 441). It keeps those within the arc
    about `yBodyRot` and the range less half their width.
  - So the swipe can never strike a carriage: a contraption entity is no `LivingEntity`, `isPickable()` is
    false and `hurt` returns false (AbstractContraptionEntity.java:798, 802). Riders in the arc are struck.
  - Before frame 9, `AnimationAreaAttackAI.tick()` calls `entity.lookAt(entityTarget, 30, 30)`, where
    `AnimationAttackAI.start()` set `entityTarget = this.entity.getTarget()` (50). So the swing turns toward
    its target, not toward a carriage, unless the hold keeps the bearing.
- Animations: `SWIPE_ANIMATION = Animation.create(28)` (156) and `ROAR_ANIMATION = Animation.create(76)`
  (155). The jar calls `public <T extends Entity> void sendAnimationMessage(T entity, Animation animation)`
  (llibrary `AnimationHandler`, 27) itself (567, 314). The roar animation plays `MMSounds.ENTITY_FROSTMAW_ROAR`
  at volume 4 (377-379), and that sound is a `DeferredHolder<SoundEvent, SoundEvent>` (MMSounds.java:107).
- alwaysActive: `public void setAlwaysActive(boolean isAlwaysActive)` (950); while it is set, `tick()` calls
  `setActive(true)` (361-362). It is saved as "alwaysActive" (974) and read back (966).
- The restriction does not hold an awake Frostmaw with a target:
  - `tick()` (350) runs `super.tick()` (356) first, so the goals run there.
  - Then, with a target beyond 5.5 blocks (unless an ice breath runs within 7.5), it calls
    `this.getNavigation().moveTo((Entity)this.getTarget(), 1.0)` every tick (550-552).
  - `MoveTowardsRestrictionGoal` sets its path only when it starts, so the Frostmaw's own path replaces it
    the same tick.
  - Vanilla `Mob.serverAiStep()` (773) runs the selectors (782-792), then `navigation.tick()` (797) and
    `moveControl.tick()` (804). So a path set after the entity's tick, on `ServerTickEvent.Post`, is the one
    followed the next tick.
- The ice ball:
  - It is thrown at `targetDistance >= 14.5`, in `NO_ANIMATION`, with `iceBallCooldown` at 0 (then set to
    200), the crystal, and the Frostmaw on the ground (575-577). The breath is within 13.5 (569-573,
    cooldown 260).
  - `EntityIceBall.tick()` hits `getEntitiesNearby(2.0)` (60). That is `level().getEntitiesOfClass(Entity.class,
    box, ...)` (EntityMagicEffect, 149-155), so it reads the entity manager. A hit takes
    `hurt(freeze, 3 × multiplier)` and, for a living entity, `addFreezeProgress(1.0f)` (62-63). The ball
    bursts on `!level().noCollision(this, box.inflate(0.15))` (66).
  - `EntityMagicEffect.getPistonPushReaction()` returns IGNORE (83), so Create's `canCollideWith` refuses
    the ball. A contraption entity does not override `canBeCollidedWith`, so it is no collision shape for
    `noCollision`. The ball should fly through a carriage's blocks; that is read from the jar, and test 8
    confirms it.
- The freeze:
  - `FrozenData.onFreeze(LivingEntity entity)` (185-221) adds a new `EntityFrozenController`
    (`mowziesmobs:frozen_controller`, EntityHandler.java:118) at the entity and calls
    `entity.startRiding((Entity)this.frozenController, true)` (199). For a Mob it also calls
    `setNoAi(true)`.
  - `onUnfreeze` (223-235) calls `stopRiding()`, then `teleportTo` the old position, and discards the
    controller.
- The dismount:
  - Vanilla `Entity.startRiding(Entity vehicle, boolean force)` (2102) asks NeoForge's
    `EventHooks.canMountEntity(this, vehicle, true)` (2114), then runs `if (this.isPassenger())
    this.stopRiding()` (2116-2118).
  - Create's only mount listener is `CouplingHandler.preventEntitiesFromMoutingOccupiedCart(EntityMountEvent)`
    (51), which is for minecart couplings. So no Create code vetoes leaving a carriage seat. Another pack
    mod could; test 8 settles it in the running game.
- Seats: `AbstractContraptionEntity.addSittingPassenger(Entity passenger, int seatIndex)` (207);
  `Contraption.getSeatMapping()` (1234) is a `Map<UUID, Integer>`.
- `Mob.restrictTo(BlockPos, int)` (1308), `clearRestriction()` (1321), `isWithinRestriction()` (1300);
  `GameTestHelper.makeMockPlayer(GameType)` (269).

**Layout.**
- Engine:
  - `crates/rtsim-conflict/src/engine/rail.rs` (`lurk_start`, `lurk_end`, `lurking`, `resend_lurks`,
    `train_passed`).
  - `src/engine.rs` (`arrive`'s Track arm, `go_home`, the wiped-out path, `home`'s kept row, `ambush`,
    `resend_notices`, `later`, `ingest`, stats).
  - `crates/rtsim-cli/src/sim/mowzie.rs` and its synth (MZB-371, 372).
  - `crates/rtsim-conflict/tests/lurk.rs` (new).
  - rtsim-jni's protocol test.
- Core Java (`neoforge/core/.../mic_rtsim/`):
  - `conflict/Lurker.java` (new), `conflict/ConflictBridge.java` (the lurk table, placement, the hold
    tick, strikes, pending roars, `playToward`, `trainCalled`, `trainPassed`), `conflict/ConflictRecords.java`.
  - `api/RtsimApi.java` (`registerLurker`, `lurks`, `lurkBody`, `strike`, `trainPassed`, `trainCalled`).
  - `gametest/ConflictKit.java` (`ambushSpot`, `ambushRemoved`, `trainRobbed`, `trainsPassed(agent)`,
    `lurkStrikes()`) and `gametest/FakePlayers.java` (MZB-370).
- Create plugin: `TrainSampler.java` (lurks, near set, strikes, `arrived`), `mixin/TrainMixin.java` (one
  call added to `mic_rtsim$arrived`), `gametest/RailFrostmawGameTests.java` (tests 8 and 9).
- Mowzie plugin: `FrostmawLurker.java` (new), `MicRtsimMowziesMobs.java` (its registration), `DenKind.java`
  (`sound()`).
- Docs: `docs/boundary.md` (MZB-374).

**Engine.**
- `train_passed`:
  ```rust
  pub fn train_passed(
      &mut self,
      w: &mut World,
      infra: &mut dyn Infrastructure,
      tp: &TrainPassed,
      at: Tick,
  ) {
      let Some(p) = Party::of_agent(tp.agent).filter(|p| self.parties.contains(*p)) else {
          self.stats.stale += 1;
          return;
      };
      if !self.lurking(p) || tp.epoch > self.parties[p].epoch {
          self.stats.stale += 1;
          return;
      }
      self.stats.passes += 1;
      if tp.flags & boundary::PASSED_WATCHED != 0 {
          self.stats.passes_watched += 1;
      }
      let roll = StreamKey::new(self.seed, SYS_AMBUSH, p.to_bits())
          .slot(tp.train ^ at.0.rotate_left(32))
          .below(1000);
      if (roll as u32) >= self.tun.ambush_milli {
          return;
      }
      let got = self.ambush(infra, p, tp.train, at, w, "by the track");
      if got > 0 {
          let faction = self.factions[self.parties[p].faction as usize].key;
          self.outbox.push(Cmd::TrainRobbed(TrainRobbed { faction, train: tp.train }));
      }
  }
  ```
- `lurk_start(p, at)` and `lurk_end(p)` build the one AMBUSH_SPOT (`lurk_end` with `AMBUSH_REMOVED`) from the
  party's Track target. `go_home` calls `lurk_end` before it changes the phase or target. `resend_lurks`
  walks the parties in handle order.
- The engine's `ingest` (4923) already has the infrastructure: its match calls `train_passed(w, infra, tp,
  at)`. In rtsim-jni, a record that leaves `Conflict::later` no longer takes the deferred path
  (conflict.rs:561-570, which counts `deferred` and ingests at the engine's present without a catch-up). It
  goes through `shared::catch_up` and `ingest` (592), which passes the infrastructure through `with_infra`
  (596, 606). No rtsim-jni code changes beyond the protocol test.
- Stats appended: `lurks`, `lurks_ended`, `passes`, `passes_watched`.

**Java.**
- `ConflictBridge.accept` (641, its switch at 651) gains AMBUSH_SPOT (the table, and the hold of a live puppet or its
  release) and TRAIN_ROBBED (the pending map, and the roar of a live puppet).
- The hold tick on `ServerTickEvent.Post`: for each lurk with a live puppet, call `lurker.hold(mob, spot)`,
  and every 20 ticks the target rule (MZB-362). A puppet that died or was recalled is released and dropped
  from the hold list (the lurk stays).
- `RtsimApi.strike(long agent, Vec3 at)`: the live puppet's Lurker `strike`. A true answer counts in the
  bridge's `lurkStrikes` (read by test 9).
- The sampler's lurk pass, after the territory test of each sample (MZB-363 to 365):
  ```java
  for (Lurk l : RtsimApi.lurks().values()) {
      if (!l.dimension().equals(dim)) continue;
      double d = segmentDistance(l.track(), x0, z0, x1, z1);
      Approach a = approaches.get(trainId, l.agent());
      if (d <= l.reach() && a.armed) { a.armed = false; send(l, t, RtsimApi.lurkBody(l.agent()) != null); }
      else if (d > 64) a.armed = true;
      if (RtsimApi.lurkBody(l.agent()) != null && d <= 64) near.add(t);
  }
  ```
- `TrainMixin` keeps its three injections. `mic_rtsim$arrived` becomes `TrainWatch.arrived(...);
  TrainSampler.arrived(...)`.
- `FrostmawLurker` (Mowzie plugin) as MZB-361, registered in the plugin's constructor with
  `RtsimApi.registerLurker("mowziesmobs:frostmaw", new FrostmawLurker())`, beside the stand-ins and camp
  kinds (MicRtsimMowziesMobs.java:43-48).
- D4: `Lurker` and every call that reaches it are vanilla types in core. The Frostmaw code is only in the
  Mowzie plugin, and the Create code only in the create plugin. D3: no new entity; the swipe and the roar
  are the jar's own animations. D1 and UR-1: MZB-369.

**GameTests to write** (`RailFrostmawGameTests`, create project with `-PwithMowzie`, registered as MZB-329,
each at its own far base, scripted engine):
1. `anIceBallUnseatsARiderFromACarriage` (§7 test 8, batch `rail_frostmaw_ice`):
   - Static form: a train assembled at a station and stopped. The rider (MZB-370) is seated on its seat
     block. A Frostmaw stands 20 blocks off: awake, with its crystal, `restrictTo` its place with radius 2,
     and its target set to the rider. The hold is not used.
   - Within 400 ticks, a `mowziesmobs:ice_ball` was in the level, and the rider's vehicle is
     `mowziesmobs:frozen_controller`. The carriage's seat mapping no longer holds the rider's UUID, and the
     carriage entity's passengers do not include it.
   - Within 60 ticks of the freeze ending, the rider has no vehicle.
   - The test fails if the rider is ever on two vehicles.
   - Moving form: the train held at 0.5, 1.0, 1.4 and 2.0 blocks a tick, with 4 passes each, the Frostmaw
     18 blocks off the track. Log `[gametest] ice ball v=<v>: <hits>/4`. The rates are a measurement, not a
     gate; the test fails only on a double vehicle.
2. `aLurkingFrostmawStandsOffTheRailAndSwipesAPassingCarriage` (§7 test 9, batch `rail_frostmaw_lurk`):
   - Setup: a 96-block straight with two stations and an assembled train, a seated zombie in it. A scripted
     AMBUSH_SPOT for a new agent: the track point at the straight's middle, the spot 5 blocks off. Then a
     scripted BAND_MATERIALIZE of one FROSTMAW stack for that agent, and a `FakePlayers.add` player 30
     blocks from the spot. The player has a target and walks away: that tests the hold.
   - For 200 ticks, the puppet stays within 2 blocks (horizontal) of the spot, and its near edge (its
     distance to the rail less 2) stays at least 1.5 blocks clear.
   - `saveWithoutId(new CompoundTag()).getBoolean("alwaysActive")` is true.
   - The train passes at 1.0 blocks a tick:
     - exactly one TRAIN_PASSED for the agent, with `PASSED_WATCHED` and a carriage index in range;
     - `lurkStrikes()` at 1 or more;
     - at the swing's frame 9, `yBodyRot` within 30 degrees of the bearing to the struck point;
     - the zombie hurt (its health down);
     - the contraption's block count unchanged and the train not derailed.
   - A scripted IND_CARRIAGE_DELTA for the train lands in `Carriage.storage` (`IndustryKit.received`).
   - A scripted TRAIN_ROBBED, then the train's next arrival at a station with a Hearer within 64 blocks:
     the Hearer's `sounds` gains one packet with the roar's sound. Its `raw`, `overlay` and `titles` gain
     nothing. A second arrival with nobody near plays nothing, and the entry stays (`pendingRoars()`).
   - A scripted REMOVED: `alwaysActive` is false, `hasRestriction()` is false, and the agent is gone from
     `RtsimApi.lurks()`.
   - Built at step 8, not run: the spot is 4 blocks off, the line 150 blocks and test 8 two tests (MZB-365's
     second amendment, MZB-370's last).

**Rust tests** (`crates/rtsim-conflict/tests/lurk.rs`):
- The lurk's life: `a_raid_on_a_track_sends_its_ambush_spot`,
  `every_way_a_lurk_ends_sends_removed` (bored, a watched fight routed or bored, wiped out, the chief's
  fall, Peaceful while Waiting and while Fighting), `a_recall_does_not_end_a_lurk`,
  `a_lurk_goes_on_through_a_watched_fight`, `a_session_restart_resends_each_lurk_once`,
  `a_kept_home_row_has_no_track_target`.
- Passes and robberies: `a_train_passed_rolls_and_robs` (a hit raids the train and sends TRAIN_ROBBED; a
  miss sends nothing), `a_robbery_of_nothing_sends_no_train_robbed`, `a_stale_train_passed_is_counted`
  (no party, not lurking, a later epoch), `a_watched_pass_rolls_as_an_unwatched_one`,
  `the_cooldown_bounds_a_train_to_one_robbery_a_day` (a FakeWorld industry).
- News and persistence: `an_ambush_makes_no_news`, `a_lurk_survives_a_save_mid_wait` (blob and paged),
  `the_on_the_way_note_keeps_its_wording`.
- rtsim-jni's protocol tests (`crates/rtsim-jni/tests/protocol.rs`, module `conflict`):
  `a_train_passed_is_live_and_meets_the_lurk_of_its_own_time` (TRAIN_PASSED is live, `deferred` does not
  count it; a pass stamped inside the wait meets a live lurk although the hand-over is past the patience, one
  stamped after the end is stale; the lurk's start and removal reach the plugin once each, in Java's
  dictionary) and `a_lurk_goes_to_the_plugin_once_after_each_session_start` (a lurk a replayed record made
  goes once after a crash restart, not twice; the clean restart resends it once; fails with two spots when
  the `retain` of MZB-353's amendment does not drop the queued AMBUSH_SPOT). The host's own catch-up
  (`shared::catch_up`) is for the other modules: `Conflict::ingest` runs the conflict to the record's time
  itself, so mutating the catch-up out changes neither test.

**Sim runs** (`rtsim-cli sim mowzie`, MZB-330's seeds and tolerances):
- **Run 1, the ambushes** (built at step 4; MZB-371): the 12- and 24-a-day lines, each with two trains,
  over 8 seeds and 45 days.
  - Every Raid on a track sends AMBUSH_SPOT at its arrival and REMOVED when it ends.
  - At most 64 items are robbed per train per day, and at most 256 over a middling 2-day wait with two
    trains.
  - Each robbery takes `min(64, cargo / 10)` of its train (cargo 1280 and 320: 64 and 32).
  - The share of each train's first open pass of a day of the wait (the industry's cooldown on the train
    clear) that robs is between 35% and 65% (`ambush_milli` 500), pooled over the trains and seeds; the
    roll's rate over every counted pass is held to the same band, and so is its rate on the pass after a
    miss and the one after a hit (independence).
  - The robbed goods are in the ledger's destroyed share (ledger 10), and the industry's `books.stolen`
    equals them. The engine's `stats.lurk_robberies`, the TRAIN_ROBBED commands heard and the industry's
    books per pass agree (review A, P3: `stats.ambushes` also counts the on-the-way ambushes, which send
    no roar, so it is the larger).
  - No TRAIN_PASSED is stale except in the `LAG` ticks after a lurk ended.
- **Run 8, save and restore** (built at step 4; MZB-372): the three lurk points (the Decay point with a
  band lurking), each blob and paged, against the control; then the session restarts at each, plain and
  after a load from pages. The fourth, a reconcile in flight, is M4's.

**Work split** (in order; each step's tests green and committed before the next):
0. Branch (MZB-350).
1. `lurking`, `lurk_start`, `lurk_end` and their calls (`arrive`, `go_home`, the wiped-out path),
   `resend_lurks`, `home`'s kept row; the lurk-life tests.
2. `ambush`'s return and wording, `train_passed`, `later` and `ingest`, the stats; the pass tests.
   **Opus review A**: the engine against §4.4 and ledger 8 (every end sends REMOVED; nothing ends a lurk
   twice).
3. rtsim-jni: the protocol test only (TRAIN_PASSED live: it reaches `ingest` after the catch-up, and
   `deferred` does not count it).
4. The synth's industry and lurk emulation, run 1's ambush half, run 8. **Opus review B**: the runs against
   §4.4's bounds and the control comparison.
5. Core Java: `Lurker`, `registerLurker`, the lurk table, placement, the hold tick and target rule,
   `strike`, `trainPassed`, the pending roars, `playToward`, `trainCalled`; `ConflictKit` and `FakePlayers`
   (MZB-370); compile.
6. Create: the sampler's lurk pass, the near set and strikes, `TrainSampler.arrived` and the mixin's added
   call; compile.
7. Mowzie plugin: `FrostmawLurker`, its registration, `DenKind.sound()`; compile.
8. `RailFrostmawGameTests` 8 and 9. **Opus review C** before the GameTest round, then the GameTests (the
   create project with `-PwithMowzie`).
9. `boundary.md` (MZB-374).

**What M8 changes in M7.** The engine's `arrive` Track arm (it now sends AMBUSH_SPOT), `go_home`, `home`,
`ambush` and `resend_notices`; the sampler (the lurk pass and the near set); `TrainMixin`'s arrival body;
`RailFrostmawGameTests`. Not changed: the meter, the focus, the target rules, the page format.

**Where the jar or the engine differs from MOWZIE.md.**
- §4.4 leaves open which entities the swipe strikes (test 9). The jar settles it: only living entities in
  the arc, never a carriage, which is no `LivingEntity` and refuses `hurt`. The cargo comes only from the
  roll, as §4.4 says.
- §4.4 has the restriction hold the Frostmaw at its spot. An awake Frostmaw with a target re-paths to it
  every tick after its goals run, so the hold re-paths it after the entity ticks (MZB-361).
- The restriction gates targeting (M6's MZB-279), so the core sets the target (MZB-362).
- The swing turns toward the Frostmaw's target before it lands, so the hold keeps the strike's bearing
  through it.
- §4.4's inferred steps, as far as the jar goes: no Create code vetoes the dismount (only
  `CouplingHandler`'s minecart veto exists), and the ice ball ignores the carriage (IGNORE push reaction,
  no collision shape). Test 8 settles both in the game, and the moving form gives the hit rates.
- §4.4's "within view, a puppet with alwaysActive": the jar has no restriction-free way to keep it at the
  spot, which is why `Lurker` exists.
- MOWZIE.md's `engine.rs:2869-2878` ("finds nothing to raid") is 3409-3421 at 761f5d0f.

**Traps.**
- A `FakePlayers.add` player is invisible to the ice ball (`getEntitiesOfClass`) and to the swipe
  (`getEntityLivingBaseNearby`): use the mock player or a mob as the struck rider (MZB-370).
- The swipe cannot hit a contraption: a test that waits for carriage damage waits forever.
- `session_start` drops agent commands: AMBUSH_SPOT must be resent after the replay (MZB-353), or a
  restart leaves Java with no lurks.
- An unwatched lurk's 20-tick segments can miss the reach of 8 on curves under about 24 blocks of radius
  (M7's traps): place test 9's lurk on a straight.
- A restricted Frostmaw never targets on its own: a test that expects it to turn on a player must set the
  target, or run the hold's target rule.
- `TrainWatch.arrived` returns early without an industry or district: call `TrainSampler.arrived` beside
  it, not inside it.
- A lurking party can be in `Phase::Fighting` (a player came on it). A `lurking` test on Waiting alone
  loses the REMOVED of every lurk a watched fight ends, and Java keeps a lurk for a party gone home.
- `start_fight` and `seen` bump the party's epoch, so the lurk table's epoch is older than the party's
  after a watch: TRAIN_PASSED is stale only on a later epoch (`<=`), never on an unequal one
  (`party_of`'s filter).
- A SEEN lurk's passes carry epoch E (the spot's, which is what Java's table holds) against the party's
  E+1, and they pass; but the REMOVED goes out at E+1. Java must drop a lurk by agent, never by an epoch
  match (a table keyed or checked by epoch keeps the lurk of a band that was watched). Review C checks
  this on the Java side (MZB-359).
- `resend_lurks` says a lurk once per drain of the queue; once per SESSION is the host's `notices_due`
  flag. A host that calls `resend_notices` twice in a session sends it twice.

## M9 Umvuthana

(none yet)

## M10 March

- A party whose surviving members are all Left (none stands to be handed back) gets no HAND_BACK line and so
  no recall toward its camp: the homecoming of MZB-211 starts from a HAND_BACK. M10 owns this case (found in
  the M5 review, P2-7); until then such a party stays where the engine left it.
  M10 closes it: `home` recalls such a party toward its camp (MZB-463), and Java sends home every band a
  HAND_BACK names, whatever the line's outcome (MZB-464).
- **MZB-450** (base, reads and ids):
  - The implementation branch `feat/mowzie-m10` starts from the finished `feat/mowzie-m9` (M9a, M9b and
    M9c, each reviewed), with `feat/mowzie-m6` merged in. M10 needs two things from M6: `FakePlayers.addSeen`
    (a mock player the chunk map sees, so its chunks tick) and the party-scoped op ids of MZB-589.
  - Merge `main` too if M7 or another slice has landed there. After the merges, delete every project's
    `run-gametest/world`: pages from a newer build stop older builds starting.
  - This design read these heads; line numbers below are those copies':
    - `feat/mowzie-m9` at 7521375d to 6125fcce. It is still moving, so re-check every line number;
    - `feat/mowzie-m6` at c803fc14;
    - `main` at 65f90ee9.
  - Ids: MZB-450 to MZB-499, of which this slice uses MZB-450 to MZB-484.
- **MZB-451** (BAND_MARCH goes when what it says changes; ledger 23):
  - A new `Conflict::march_changed(p)` sets the party's saved `march_due` flag. If the flag was clear, it
    also pushes `p` onto `Conflict.march_due: Vec<Handle<Party>>`, which is unsaved and rebuilt from the
    flags at load.
  - Each caller marks where the state the record reads has changed:
    - `send_party`: a new party, Mustering;
    - `party_event`'s Mustering arm: the muster ends, Out;
    - `wait`, `start_fight` and `go_home`: the phase and the waypoints;
    - `home`, when it keeps the row (Waiting with loot);
    - `party_lost` (camp.rs), whenever `lost > 0`. The mark goes before its `is_war` return, so every unit
      loss of every kind marks: `lose_units`, BAND_DEMATERIALIZED, `casualties` and `chief_unit_lost` all end
      there;
    - `member_died` (camp.rs): a unit left behind dies at the camp;
    - `succeed`'s loop over the parties the chief led, and `chief_killed_by`: `chief` goes false;
    - `player_left`, when it turns Fighting to Waiting;
    - `session_start`, for every party. Java's map is per session (MOWZIE.md §4.16);
    - `start_segment`, only for a long route (MZB-455).
  - A segment's advance sends nothing. The waypoints carry their times, so Java's position needs no new
    record (MZB-454).
  - `take_commands` builds the records:
    1. It sorts `march_due` by handle.
    2. For each party still live with its flag set, it clears the flag, adds 1 to `march_rev` and pushes
       `Cmd::March(self.march_record(p, false))`.
    3. It calls `enforce_budget` after every `FACTIONS_PER_PAGE` records, as `session_start` pages territory.
    4. The rest of the outbox follows unchanged. So the march records come first.
  - A record carries the state as of the take, so several changes in one step make one record.
  - A `home` that removes the row builds its record itself (MZB-463). It pushes `march_record(p, true)` onto
    the outbox after `camp_home`'s lines and any recall, clears the flag, then calls `parties.remove(p)`. The
    outbox keeps its order, so Java has the hand-back lines before the end.
- **MZB-452** (the record's fields, built by `march_record(p, ended)`):
  - `agent` is `Party::agent(p)`; `revision` is the party's `march_rev`; `dimension` is `u32::from(frame)`;
    `faction` is the faction's key.
  - `culture` is the faction's culture index plus 1 (rtsim-jni's culture dictionary id). `kind` is
    `kind as u8`, as BAND_MATERIALIZE's purpose is.
  - `phase` is set by a match, not a cast: Mustering 1, Out 2, Waiting 3, Fighting 4, Home 5.
  - `flags`:
    - MARCH_CHIEF when `chief`;
    - MARCH_ENDED when `ended`, or when `alive() == 0 && !chief`;
    - MARCH_JOINT never, since no joint march exists. `partner` is 0.
  - `muster_end` is `until` while Mustering, else 0.
  - `units`: alive counts by body id (`body_id` of each unit's body). Unit rows that share a body are summed,
    zero counts are left out, and the list is sorted by body id.
  - `waypoints` are (x, z, tick) in the route's chart:
    - Mustering: the whole route with its times;
    - Out and Home: `route[seg..]` with `times[seg..]`;
    - Waiting and Fighting: one waypoint, `(route.last(), until)`.
  - The encoder cuts lists past MAX_ENTRIES (1024), as today.
- **MZB-453** (saved state and the format):
  - `Party` gains three fields: `march_rev: u32`, `march_due: bool` and `unseen_due: Tick` (MZB-458). They are
    appended at the end of `put_party` and `get_party` (persist.rs).
  - The outbox is saved (persist.rs 1296), and so is `march_due`. A mark made before a save therefore
    survives a crash after it.
  - The save is the next RTC and META after M9's. Fixtures and their hashes are regenerated, with no
    migration (nothing is released).
- **MZB-454** (one position rule on both sides; test 34): a new `BandMarch::chart_pos(&self, t: i64) ->
  Option<(i32, i32)>` in boundary.rs, with its Java twin `MarchPath.chartAt(long t)`:
  - none with no waypoint; phase 1, the first; phases 3 and 4, the last.
  - Phases 2 and 5: take `s`, the largest index at most `w - 2` whose tick is at most `t` (0 when there is
    none). Then apply `party_chart_pos`'s arithmetic to segment `s`:
    - `a` when `t1 <= t0` or `t <= t0`;
    - `b` when `t >= t1`;
    - else `fr = (t - t0) as f64 / (t1 - t0) as f64`, and per axis
      `(f64::from(a.0) + (f64::from(b.0) - f64::from(a.0)) * fr) as i32`. This truncates toward zero, as
      Java's `(int)` does.
  - For every tick the engine evaluates, this equals `party_chart_pos`: the engine's `seg` is the segment
    whose start has passed. `party_pos` is its canonical form.
  - Java's `MarchPath.at(long t, LevelTopology topo)` folds it as Rust's `canonical_i32` folds a cell: the
    block centre, `topo.fold(x + 0.5, z + 0.5)`, floored. Folding the corner would land on the neighbouring
    block under a half-turn. `RtsimSession.topology(level)` gives the level's topology.
- **MZB-455** (a route longer than a record holds): stateless. `start_segment` marks the party when
  `route.len() - seg >= MAX_ENTRIES`. So while the rest of the route does not fit, each segment start sends
  a new window of 1,024 waypoints, and the last window sent holds the whole rest. No route is that long
  today; the rule keeps the record honest if one is.
- **MZB-456** (the walk home in bodies; ledger 23):
  - `go_home` takes `here = party_chart_pos(p, at)`, not `party_pos`. The route is in the chart, and a
    canonical `here` among chart waypoints would send the walk across the world after a seam.
  - It walks when all of these hold:
    - the party is `embodied`;
    - the reason is RECALL_DONE, RECALL_BORED or RECALL_ROUTED;
    - `spawned` holds at least one member;
    - the party is no envoy, and the world is not Peaceful.
  - Otherwise it recalls toward the camp as today: SETTLED, PASSING and PLAYER_LEFT, an envoy, and Peaceful,
    where no war body stands.
  - A walking band keeps `epoch`, `spawned` and `seen_by`. After the new route, the mark, `book_leg` and the
    schedule, `go_home` calls `steer_segment(w, p, at)`.
  - `steer_segment` is factored out of `start_segment` unchanged:
    - a segment that crosses an edge_stop seam is recalled (RECALL_PASSING toward the segment's end);
    - else BAND_STEER to the segment's end, with STEER_ATTACK only on a war band's last segment Out, then
      `book_unseen`.

    `start_segment` calls it, so each later segment home steers too.
- **MZB-457** (met on the way home, and the column's heading):
  - `seen` no longer skips Home. It still skips an embodied party, Mustering, Fighting, and Peaceful's war
    bands and envoys.
  - After the wave, it steers for both Out and Home (today, Out only): BAND_STEER to `route[s + 1]`, arriving
    at `times[s + 1]`, flags 0.
  - `spawn_wave` with no arc sends the march's heading as the bearing; today it sends 0. A new
    `march_bearing(p, centre)` computes it, where `centre` is the image that `spawn_wave` already places the
    band at:
    - Out and Home, with `s = seg.min(len - 2)`: from `centre` to `toward(centre, route[s + 1])`. When that
      is zero, from `toward(centre, route[s])` to `centre`;
    - Waiting: from `toward(centre, route[len - 2])` to `centre`, the way it came;
    - otherwise, or for a zero vector: 0.
  - Degrees are taken as `start_fight` takes them, `(dz as f64).atan2(dx as f64).to_degrees() as f32`, from
    +x toward +z (boundary.md's BAND_MATERIALIZE).
- **MZB-458** (re-binding when the watcher walks off; ledger 23, test 29's re-bind clauses):
  - `inside_now(p, c, r, at) -> Option<f64>` gives the squared distance from the party's chart position to
    the image of `c` nearest it, when that is within `r`. `exit_time` uses it for its first test, so the two
    never disagree.
  - `rebind_to(w, p, at) -> Option<ActorId>` looks at the players in the party's frame, reported within
    `player_stale_ticks` and inside `view_radius + party_radius + 16` (192, `book_unseen`'s circle). It
    returns the least by (squared distance, ActorId).
  - `unseen(w, p, epoch, at)` gains the world. It acts only for an embodied party at that epoch, Out or Home,
    with `at == unseen_due`:
    - if `rebind_to` finds a player, `seen_by` becomes that player (the same one when it followed) and
      `book_unseen(w, p, at, at + 1)` runs;
    - if not, RECALL_PASSING toward `route[seg + 1]`, as today.
  - `book_unseen(w, p, at, not_before)` covers Out and Home. It books at `t.max(not_before)` and sets
    `unseen_due` to that tick. Its existing callers pass `at`. Only the latest booking acts; earlier ones find
    `unseen_due` moved.
- **MZB-459** (the watcher logs out; the private `player_left`): for each party that is embodied and seen by
  that player:
  - a siege is recalled with RECALL_PLAYER_LEFT, and its fight turns to waiting, as today;
  - any other party turns Fighting to Waiting (marked). Then `rebind_to`, at `self.now`, finds a player to
    take it over. If nobody is found, it is recalled as today.
- **MZB-460** (arriving watched from afar): this is `arrive`'s war arm when no watcher is at the target. An
  embodied party that is no siege, and for which `rebind_to` finds a player, is re-bound and not recalled.
  It resolves abstractly, with its losses falling through `lose_units` (MZB-462), and `go_home(RECALL_DONE)`
  walks it home. A siege is recalled as today.
- **MZB-461** (a band that walks into its fight): `start_fight` on an embodied party keeps `epoch`, `spawned`
  and `wave` and sets `seen_by`. It pushes BAND_STEER with STEER_ATTACK toward `route.last()`, with `arrive`
  0 (at its speed). The arc's `spawn_wave` then sends only the members not yet in Minecraft, since it already
  counts them. A party that is not embodied starts as today.
- **MZB-462** (`lose_units` keeps `spawned`; ledger 23): while the party is embodied, after the loss each
  `spawned` entry is clamped to its unit's count.
  - `recalled` is not clamped. A recalled stack is an older epoch's bodies, counted by their own
    BAND_DEMATERIALIZED (MZB-131), and clamping it would hide deaths in Minecraft.
  - The puppets the books no longer hold are Java's to trim (MZB-473).
- **MZB-463** (the end at home; closes the M5 review's P2-7): `camp_home` returns whether it handled the
  band's bodies: true when it sent a HAND_BACK line or recalled.
  - `home` recalls an embodied party with RECALL_DONE toward the camp when nothing handled it. That covers a
    party all Left, a faction with no camp ledger, and a `camp_home` that sent no hand-back line.
  - Then it sends the ENDED record (MZB-451).
- **MZB-464** (Java's homecoming on every HAND_BACK; amends MZB-211): `Camps.reconcile` puts a party in
  `homecoming` for each HAND_BACK line whose party the bridge holds at the line's epoch, whatever the line's
  outcome. That includes a camp that is not in, a STALE line and a failed `addFreshEntity`.
  - Each such party gets `homeAt`, toward the site's spot, or toward its own front stack when the bridge has
    no site.
  - Today a failed hand-back leaves the band's puppets standing for good, since the engine has ended the
    party and sends nothing more.
- **MZB-465** (a raptor pack comes home as a spawn): a camp slot's body is the camp's home body, the warrior.
  So a HAND_BACK line for a raptor unit finds no warrior puppet and plays as a SPAWN at the far edge
  (MZB-213, MZB-214), while the pack walks off through `homeAt`. `camp_home` is unchanged (test 32's raptor
  clause).
- **MZB-466** (the race at home; decided by the coordinator on 2026-10-24, not Ben, since it is reversible): a puppet may be killed
  in the tick or two between the engine's `home` and its HAND_BACK line.
  - Its BAND_CASUALTIES is for an ended party, so it books nothing and sends no Killing signal.
  - Its slot's line then finds no puppet, and the body spawns at the edge.
  - Accept it. Keeping the party until Java answers each line would change how every party ends.
  - The coordinator added two conditions:
    - **A counter:** `stats.unbooked_home_deaths`, plus a debug log line naming the party and the slot. It counts
      each BAND_CASUALTIES (and CHIEF_KILLED) that names a party already ended by `home` before its HAND_BACK line
      is answered, so the rate shows in GameTests and soaks. Java logs the matching case on its side too: a death
      of a puppet whose party has a HAND_BACK in flight.
    - **A test:** the unbooked death costs at most that one body and does not cascade. Afterwards the faction's
      party count, its pool, the camp's slots and the audit sums agree. No other party, slot or later band is
      touched. A second copy of the same casualty changes nothing.
  - If the counter shows the race is common, revisit it by keeping a party until every line is answered.
- **MZB-467** (idempotency): M10 adds or changes no holder-carried record and no op id.
  - BAND_MARCH is an engine command, latest-wins by revision within a session, and the engine sends every
    party again at a session start. Java sends nothing back for it.
  - The trim, the walk-in and the clamp journal nothing.
  - A trimmed member is no survivor in a later BAND_DEMATERIALIZED, whose format and keys (agent and epoch)
    are unchanged.
  - Nothing enters `applied_ops`.
- **MZB-468** (Java's march map):
  - New: `ConflictRecords.BAND_MARCH = 0x880B` and `ConflictRecords.March` (layout in the design below).
    `conflict/MarchPath` is an immutable decoded record, with its bodies resolved through the names at
    accept.
  - `ConflictBridge` keeps a `Long2ObjectOpenHashMap<MarchPath>` and a `LongOpenHashSet` of ended agents:
    - a record replaces the held path only with a higher revision;
    - ENDED removes the path, adds the agent to the ended set and trims nothing;
    - an ended agent's later records are dropped;
    - both clear in `sessionStarted` and `switchedOff`.
  - `public MarchPath march(long agent)` reads it, for ConflictKit and for M11. M10 adds no far-view source.
- **MZB-469** (the column; ledger 25, test 34):
  - Every materialize splits an entry into stacks of one row each: up to two members, at `lateral(g,
    rowSize)`, back 0. These are ledger 25's groups: a row of two, a pack, the chief alone.
  - A body whose maker says `pack()` (the raptor) gets one stack per member.
  - The chief (MZB-109's extra body) is a flagged stack of its own, at the rear of its entry's rows.
  - `Stack` gains `back` and `side`. Its offset from the head is `-h * back + r * side`, with
    `r = (-hz, hx)`.
  - At radius 0, met on the march:
    - the heading comes from the bearing (MZB-457), and the head is at the arc centre;
    - rows stand one behind the other, `ROW` (1.5) apart, in the record's order;
    - packs alternate at side `±PACK_SIDE` (3.0, PLACEHOLDER), `PACK_ROW` (6.0, PLACEHOLDER) back per pair;
    - the chief comes last.
  - On an arc (a fight), each entry keeps its arc point as today. Its rows stand back from that point, ROW
    apart, and its side keeps the entry's spread, `(i - (m - 1) / 2) * STACK_SPACING`.
- **MZB-470** (turns): `steer` turns the column.
  - The head is the front non-GONE stack's anchor less its offset.
  - The new heading runs from the head to the steer point.
  - Each stack's offset is recomputed from its `back` and `side`, and it walks to the point plus its offset.
  - A recall's walk keeps the offsets it has.
- **MZB-471** (pending stacks hold at their column place; ledger 25): unchanged from M3. A pending stack's
  place is its walk's anchor plus its offsets, so it moves with its column. It is tested only when a chunk it
  waits on changes or its place moves into another chunk.
- **MZB-472** (the stop: walk-in and clamp; ledger 25, CL-D35):
  - A band is stopped while the bridge holds a march for it in phase 3 or 4.
  - The square: a player's entity-ticking chunks lie within Chebyshev distance `s` of its chunk, where `s` is
    the server's simulation distance. One chunk inside the edge, that is chunks `pcx - s + 1` to
    `pcx + s - 1`: blocks `16 * (pcx - s + 1)` to `16 * (pcx + s) - 1`, and the same in z.
  - The player is the nearest non-spectator player in the band's level to the stack's place, within 256
    blocks of the head. With none, neither the walk-in nor the clamp runs.
  - The head is `MarchPath.chartAt(now)`, in the frame the bridge's anchors are in (MZB-479). On a flat world
    it equals `at`. When a TP slice maps images in the bridge, the head becomes `at`.
  - Walk-in, in M3's pass over pending stacks, on the stop's beat: the tick a record puts the band in the
    stop, then every 20 ticks (PLACEHOLDER, the clamp's). A pending stack of a stopped band whose place lies
    outside the square:
    1. On the segment from its place to the head, the first point inside the square is its new anchor. If
       the segment never enters the square, the anchor is the head clamped into it.
    2. The stack is re-anchored there (`ax`, `az`, `t0 = now`) and walks on toward its old end, clamped into
       the square.
    3. It is marked dirty (its place moved) and counted in `walkIns`.
    4. It is placed only if `entitiesTickAt` says every chunk it needs ticks, as for any stack: the square
       only picks the point.
  - Clamp, every 20 ticks (PLACEHOLDER): a placed stack of a stopped band whose walk end lies outside its
    square has the end clamped into the square, and walks on from where it stands. So it holds one chunk
    inside the edge while it walks toward the fight.
  - Both end when a record leaves the stop. The next BAND_STEER, the walk home's, then puts every stack back
    in its column.
- **MZB-473** (the trim; ledger 23; it ends MZB-164's left-behind puppet):
  - When a record with a higher revision arrives for a band the bridge holds, and it is not ENDED, the band
    is marked `trimDue`. The trim runs at the end of that tick, and every 20 ticks with the clamp while still
    due.
  - Per body:
    - `living` counts the band's slots that are alive, not the chief and not removing, over its non-GONE
      stacks, placed and pending;
    - `want` is the record's count for that body (0 when absent).
  - While `living > want`, it cuts one slot: first the rearmost pending stack's last slot; then, among placed
    stacks, the rearmost slot with no player within OUT_OF_VIEW (64) of its entity.
  - The chief's slot is cut the same way when the record lacks MARCH_CHIEF.
  - A cut is `despawn(sl)`, with its extras (MZB-428), then `sl.alive = false`. It is counted in `trimmed`
    and journals nothing.
  - When every slot left to cut is in sight, the trim waits with `trimDue` still set, so no puppet vanishes
    in front of a player.
  - Why: the books lose members no player saw go (an ambush; a raid resolved while watched from afar,
    MZB-460; a unit left behind that dies at the camp, MZB-164).
  - Until the trim, a killed puppet may be one the books have already lost. Its casualty is capped at the
    unit's count (`casualties`, engine.rs 5573), the approximation MZB-164 records.
- **MZB-474** (no hooks): M10 adds no mixin and no event hook.
  - D4: the column, the stop and the trim live in the core's ConflictBridge, and the plugin only says which
    bodies are packs.
  - D3: no new entity type. The column is made of the puppets the makers already make.
  - UR-1: nothing is saved in the world. The march map is memory, cleared each session. Trimmed and
    walked-in puppets carry no new data.
- **MZB-475** (D1, with about 20 bands loaded):
  - A record costs one decode and one map put: about 400 bytes for 20 waypoints and one body, and a few dozen
    records over a band's life.
  - Pending tests are unchanged from M3 (per changed chunk). Rows make more stacks, though: a band of 9 is
    about 6 stacks instead of 2, so there are more chunk sets, each still read once a tick per chunk, not
    per member.
  - The stop runs every 20 ticks, per stopped band, over its stacks times the level's players for the
    nearest: hundreds of distance tests a second at 20 bands.
  - The trim runs on a new record, then every 20 ticks only while due.
  - Nothing runs per member per tick.
- **MZB-476** (FakeConflict, synth.rs):
  - It keeps the latest march per agent by revision, drops it on ENDED, and clamps the held body's members to
    the record's counts (the trim).
  - It plays MZB-464: a HAND_BACK line whose party it holds at the line's epoch ends that body. The body is
    removed, and a BAND_DEMATERIALIZED (reason 1, its survivors) is pushed after `leave_ticks`; it books
    nothing for an ended party.
  - Steers stay ignored, since it models no positions.
- **MZB-477** (test 34's vectors), each phase Out, checked in Rust against `party_pos` and in Java against
  the same numbers (the folded results were run through `canonical_i32` for this design):
  - flat: route (0, 0), (100, 37), (100, -50) at ticks 1000, 1100, 1200. At 999, 1050, 1100, 1150 and 1300
    the positions are (0, 0), (50, 18), (100, 37), (100, -6) and (100, -50). The -6 is truncation toward
    zero, not floor.
  - x-wrap, x0 0, period 64: route (50, 0), (90, 10) at ticks 0, 40. At 20 the chart point (70, 5) folds to
    (6, 5); at 40, (90, 10) folds to (26, 10).
  - Guyou, `guyou(64, false)` (the seams glued, not stopped): route (10, 20), (10, 50) at ticks 0, 30, across
    the south seam, which turns the band. At 10 it is (10, 30). At 20 the chart point (10, 40) folds to
    (-11, 23), and at 30 (10, 50) folds to (-11, 13). The point (-10, 24) would be the corner's fold, the
    trap MZB-454 names.
- **MZB-478** (where each test's half lives):
  - The engine's decisions are tested in Rust.
  - GameTests run on scripted commands (ConflictKit), as M3's do.
  - Test 28 is in two halves:
    - an rtsim-jni protocol test: `order_party`, no BAND_MARCH from the takes, the Invasion reaching its
      target, and pool plus away within the cap;
    - a plugin GameTest: a camp TAKE leaves the column's puppets alone, and the throne take is held under
      M9's Rule U.
- **MZB-479** (the seam gap; recorded for a TP slice): ConflictBridge has no TP11 image mapping. BAND_STEER and
  BAND_MATERIALIZE points reach it in the engine's chart, and it walks to them as they are. Test 34's
  positions are checked across seams as numbers, but its moving form runs on a flat world.
- **MZB-480** (the M11 seam):
  - `chief_stack` stays false for the grove. The walk test counts `spawned`.
  - The column puts a flagged chief stack at the rear, ready for M11's Umvuthi.
  - `MarchPath` exposes MARCH_CHIEF, and `march(agent)` is the read M11's aloft will use.
  - M10 designs nothing of the aloft, CHIEF_BODY's travel or the chief's hand-back.
- **MZB-481** (M3's stack test becomes test 33's core): with one stack per row, the counts in
  `aBandAppearsStackByStackAndAPendingStackIsTestedOnlyOnChange` change. It is rewritten as test 33's core
  clauses, not kept beside it.
- **MZB-482** (boundary.md amended):
  - §9.5: bands on their way home are materialized.
  - BAND_MARCH's `musterEndTick` is 0 after the band sets out.
  - The revision is per session.
  - BAND_MATERIALIZE's bearing at radius 0 is the march's heading.
  - The note at the BAND_MARCH layout says when the record goes (MZB-451).
- **MZB-483** (the M3 interim rule ends): M3's "until M10, a stack is placed at BAND_MATERIALIZE's position,
  not in a column" is replaced by MZB-469 to MZB-472.
- **MZB-484** (no M10 push while the session's replay catches up): under rtsim-jni, `take_commands` moves the
  whole outbox into the JNI queue after every ingest, so the engine's `outbox.retain` filters never see a
  command pushed while `replaying`. Each M10 push is therefore gated where it is made:
  - **BAND_MARCH.** `take_commands` builds no march record while `self.replaying`. It leaves `march_due` and
    the flags as they are. `home` pushes its ENDED record only when not replaying, though the flag and the
    row go either way.
    - `session_start` marked every party, so the first take after `replay_done` sends each one once.
    - Java's map (cleared at the session start) never holds a party's mid-replay state, and it never hears of
      a party that ended during the replay.
  - **Embodied branches.** Each M10 branch that acts on an embodied party adds `!self.replaying` to its test,
    and while replaying falls back to today's path:
    - go_home's walk (MZB-456), and so steer_segment's steer home;
    - start_fight's walk-in (MZB-461);
    - the re-binds in `unseen`, `player_left` and `arrive` (MZB-458 to MZB-460).
  - **seen.** `seen` skips a Home party while replaying, as it skips every Home party today (MZB-457).
  - **Why the gates are needed for now.** While replaying, a party is embodied only through main's `seen()`,
    which has no `replaying` gate (an audit found it; the coordinator is fixing it, and M10 does not). The
    gates keep M10 from adding pushes to that hole, and they do nothing once it is fixed.
  - **Ungated, by design.**
    - `march_bearing` changes a field of BAND_MATERIALIZE's existing push, so it adds no push.
    - `lose_units`' clamp (MZB-462) pushes nothing, and it keeps the books true whatever made the party
      embodied.
    - Java's walk-in, clamp, trim and pending stacks send nothing to the engine.
  - **Tests.** M10's tests keep every live player out of a band's view until `replay_done`, so they do not
    hit main's `seen()`.
  - **boundary.md.** Its BAND_MARCH note says that none goes until the session's replay has caught up.

### M10 design for the implementer

**Where it stands.** Designed only; no code. Branch `design/mowzie-m10` (this doc). The implementation
branch `feat/mowzie-m10` starts as MZB-450 says. Compile as M9 does, in this worktree's own target dir. The
Rust is `nice -n 10 nix develop .# -c cargo test -p rtsim-conflict -p rtsim-jni -p rtsim-cli` from the repo
root.

Line numbers below are `feat/mowzie-m9` at 6125fcce, where engine.rs, camp.rs, boundary.rs, persist.rs and
synth.rs are unchanged since 65e780ee. Re-check them after the merges.

**Jar facts.** Mowzie's Mobs 1.21.1-1.8.2 was read with cfr in `target/m10-mowzie-cfr`. Vanilla 1.21.1 was
read from NeoForm's decompile output (`~/.gradle/caches/neoformruntime/intermediate_results/decompile_*`),
extracted to `target/m10-vanilla`.

*Mowzie*
- `EntityHandler` (130 to 134 of the cfr output):
  - `UMVUTHANA_MINION` is sized from `MaskType.FEAR`, `UMVUTHANA_RAPTOR` from `MaskType.FURY`, and
    `UMVUTHANA_FOLLOWER_TO_RAPTOR` from `MaskType.FEAR`. Each has `clientTrackingRange(8)` (128 blocks) and
    `setUpdateInterval(1)`.
  - `UMVUTHI` is `sized(1.5f, 3.2f).clientTrackingRange(10)`; `FROSTMAW` is `sized(4.0f, 4.0f)
    .clientTrackingRange(10)`.
- So a column's warriors and raptors reach every client within 128 blocks, and the trim's 64 (OUT_OF_VIEW) is
  inside that.

*Vanilla: the ticking square (MZB-472)*
- `ChunkLevel` 10 to 12: FULL 33, BLOCK_TICKING 32, ENTITY_TICKING 31 (`byStatus`, 55).
- The view distance loads chunks. `DistanceManager.PLAYER_TICKET_LEVEL` (40) is ENTITY_TICKING, and
  `PlayerTicketTracker` (464 to 550) puts that ticket on each chunk within the view distance.
- The simulation distance ticks them:
  - `DistanceManager.addPlayer` (230 to 237) adds a PLAYER ticket to `tickingTicketsTracker` at
    `getPlayerTicketLevel()`, which is `max(0, 31 - simulationDistance)` (252 to 253).
  - `inEntityTickingRange` (256) holds when that tracker's level is at most 31.
  - `ChunkTracker` (44 to 52, 71 to 72) adds one level for each step to any of the 8 neighbours.
  - So a player's entity-ticking chunks lie within Chebyshev distance `s` of its chunk, while `s` is no more
    than the view distance.
- The simulation distance is server-wide:
  - `PlayerList.getSimulationDistance()` (775);
  - `PlayerList.setSimulationDistance` (888) sets every level, through `ServerChunkCache.setSimulationDistance`
    (490) and `DistanceManager.updateSimulationDistance` (273).
- A player holds these tickets only once the chunk map tracks it. The chain:
  1. `ServerLevel.addNewPlayer`, then `addPlayer`, then `entityManager.addNewEntity` (890 to 904);
  2. `onTrackingStart` (1698 to 1701), then `getChunkSource().addEntity`;
  3. `ChunkMap.addEntity` (1110 to 1123), then `updatePlayerStatus(player, true)` (996 to 1003);
  4. `distanceManager.addPlayer`, unless `skipPlayer` (a spectator, when spectators do not generate chunks).
- Consequences:
  - M6's `FakePlayers.addSeen` player makes chunks tick; a `FakePlayers.add` player does not.
  - UNVERIFIED: whether `onTrackingStart` runs at once for a player whose chunk is not loaded. Test 33 forces
    the player's chunk first.
- `Entity.shouldRenderAtSqrDistance` (1692 to 1700): a client draws an entity out to `bbSize × 64 ×
  viewScale` blocks.

**Contradictions with MOWZIE.md and other slices:**
1. **Line numbers only.** MOWZIE.md cites `EntityHandler` 94 and 97 for the tracking ranges; in this cfr
   output they are 130 to 134. The values agree.
2. **Groups.** Ledger 25 wants a row of two, a pack and the chief alone. M3 makes one stack per entry
   (MZB-469).
3. **The bearing.** `spawn_wave` sends bearing 0 at radius 0, so a band met on the march faces +x (MZB-457).
4. **A failed hand-back.** MZB-211's homecoming starts only from a HAND_BACK that is done. A STALE or failed
   one leaves the puppets standing for good (MZB-464).
5. **§9.5.** boundary.md says bands on their way home are not materialized. M10 materializes them (MZB-482).
6. **The seam.** boundary.md gives BAND_STEER and BAND_MATERIALIZE points in the chart, and ConflictBridge
   maps no images (MZB-479).
7. **Test 29.** MZB-440 gives test 29's re-bind clauses to M11. Their engine half (`rebind_to`, `unseen`,
   `player_left`, `arrive`) is M10's, in Rust tests and run 11. The GameTest clauses stay with M11.
8. **Replay.** Main's `seen()` pushes while `replaying`. M10 neither fixes it nor depends on the fix
   (MZB-484).

**What earlier slices must provide** (keep these as they are):
- M6: `FakePlayers.addSeen` and `removeSeen`, and MZB-589's party-scoped op ids.
- M9:
  - M9c's `ConflictBodies.registerMaker`, `Maker`, `Slot.extras` and `RaptorMaker` (MZB-428, MZB-429). They
    are not built at 6125fcce.
  - M9b's Rule U holds (`holdRemoval`), which test 28's throne take uses.
  - CHIEF_BODY and the `chief_stack` gate.
- M3 and M5:
  - `Stack` with its pending watch (`entitiesTickAt`, the chunk sets, `dirty`);
  - `homeAt` and `startRecall`;
  - `Camps.reconcile`'s homecoming map;
  - the cap in `casualties`.

**Engine** (rtsim-conflict; rtsim-jni needs no change):
- `engine.rs`, the state:
  - `Party` gains `march_rev: u32`, `march_due: bool` and `unseen_due: Tick` (MZB-453).
  - `Conflict` gains `march_due: Vec<Handle<Party>>`. It is not saved; `load` rebuilds it from the flags, in
    handle order.
  - New: `march_changed(&mut self, p)` (MZB-451) and `march_record(&self, p, ended: bool) ->
    boundary::BandMarch` (MZB-452).
- `engine.rs`, the records:
  - `take_commands` (970): the march records come first, built only when not `replaying` (MZB-451, MZB-484).
    It calls `enforce_budget` after every `FACTIONS_PER_PAGE` of them.
  - The marks:
    - `send_party` (2755); `party_event`'s Mustering arm (2964); `wait` (3439);
    - `start_fight` (4203); `go_home` (3985); `home` (4046), when it keeps the row;
    - `succeed`'s loop (1342) and `chief_killed_by` (5691);
    - the private `player_left` (6180);
    - `session_start` (4932), for every party;
    - `start_segment` (3041), for a long route only (MZB-455).
  - `home` pushes the ENDED record after its lines and any recall and before `parties.remove`, only when not
    `replaying`.
- `engine.rs`, the walk home:
  - `go_home` (3985): `here` is `party_chart_pos` (901). The walk rule gains `!replaying` (MZB-456,
    MZB-484). Then it calls `steer_segment`.
  - `steer_segment(&mut self, w: &World, p, at)` is factored out of `start_segment` (3041) unchanged, and
    `start_segment` calls it.
  - `seen` (4720): Home, unless `replaying`; the steer covers both Out and Home (MZB-457).
  - New: `march_bearing(&self, p, centre: (i32, i32)) -> f32`, called by `spawn_wave` (4262) when there is
    no arc.
- `engine.rs`, re-binding (each gated by `!replaying`, MZB-484):
  - New: `inside_now(&self, p, c, r, at) -> Option<f64>`. `exit_time` (3137) uses it for its first test.
  - New: `rebind_to(&self, w: &World, p, at) -> Option<ActorId>`.
  - `unseen(&mut self, w, p, epoch, at)` (4817). Its wheel arm (4877) passes `w`.
  - `book_unseen(&mut self, w, p, at, not_before)` (3174) also books Home. Its callers pass `at`.
  - The private `player_left` (6180) (MZB-459) and `arrive`'s war arm (3367) (MZB-460).
  - `start_fight` (4203) on an embodied party (MZB-461).
- `engine.rs`, the books:
  - `lose_units` (3814) clamps `spawned` (MZB-462).
  - `home` (4046) falls back to a recall when `camp_home` returns false (MZB-463).
- `engine/camp.rs`:
  - `party_lost` (1158) marks before its `is_war` return;
  - `member_died` (2426) marks;
  - `camp_home` (1193) returns `bool`.
- `boundary.rs`: `BandMarch::chart_pos(&self, t: i64) -> Option<(i32, i32)>` (the struct is at 767), with a
  unit test of MZB-477's flat vector.
- `persist.rs`:
  - the three fields at the end of `put_party` (735) and `get_party` (775);
  - `MAGIC` and `META_VERSION` move to the next after M9's;
  - fixtures and hashes are regenerated, with no migration.
- `synth.rs` (`FakeConflict`, 322), MZB-476:
  - The `Cmd::March` arm (742) keeps the latest march per agent by revision, drops it on ENDED, and clamps the
    held body's members (keyed by body id, as `Materialize` keys them) to the record's counts.
  - `reconcile` (1027) ends a held body for each HAND_BACK line at the body's epoch (`l.a`, `l.b`), whatever
    the line's outcome, with BAND_DEMATERIALIZED (reason 1) after `leave_ticks`.
- rtsim-jni: its `Cmd::March` arms (engine/conflict.rs 952, 1031, 1087) already map the dimension, culture and
  bodies, and shift the ticks.

**Java, core** (`neoforge/core/.../mic_rtsim/`):
- `ConflictRecords`:
  - `BAND_MARCH = 0x880B`. Its `minSize` case is `March.UNIT_LIST`.
  - `March`: `AGENT = 16, REVISION = 24, DIM = 28, FACTION = 32, CULTURE = 40, KIND = 44, PHASE = 45,
    FLAGS = 46, PARTNER = 48, MUSTER_END = 56, UNITS = 64, UNIT_LIST = 72`.
  - Each unit is `UNIT = 8` bytes (`U_BODY 0`, `U_COUNT 4`); each waypoint is `WAYPOINT = 16` (`W_X 0`,
    `W_Z 4`, `W_TICK 8`).
  - `FLAG_CHIEF 1`, `FLAG_ENDED 2`, `FLAG_JOINT 4`; phases `MUSTERING 1` to `HOME 5`.
  - `static int size(int k, int w)` returns `UNIT_LIST + UNIT * k + WAYPOINT * w`.
- `conflict/MarchPath` (new, final, immutable):
  - fields: agent, revision, dimension, faction, culture (a name or null), kind, phase, flags, `muster_end`,
    the units as body name to count (an `Object2IntOpenHashMap<String>`), and the waypoints as `int[] xs,
    zs; long[] ticks`;
  - `chief()`, `ended()`, `stopped()` (phase 3 or 4);
  - `int[] chartAt(long t)` (MZB-454; null with no waypoint);
  - `int[] at(long t, LevelTopology topo)`: `topo.fold(x + 0.5, z + 0.5)`, each part floored.
- `ConflictBridge`, the map (MZB-468):
  - Fields: `Long2ObjectOpenHashMap<MarchPath> marches` and `LongOpenHashSet ended`. Counters:
    `marchRecords`, `trimmed`, `walkIns` and `clamps`.
  - `accept` (641): `case ConflictRecords.BAND_MARCH -> march(b, at, size, aux, names, now);`.
  - `private void march(ByteBuffer b, int at, int size, int w, EngineNames names, long now)`:
    1. When `March.size(k, w) > size`, it counts `malformed` and stops.
    2. It drops the record when the agent is in `ended`, or when the held path's revision is not lower.
    3. ENDED removes the path and adds the agent to `ended`.
    4. Otherwise it decodes the record (bodies through `names`) and puts it. When `byAgent` holds the band:
       `p.trimDue = p.trimNow = true`, and `p.stopBeat = true` if the new path is stopped and the old one was
       not.
  - `public MarchPath march(long agent)`.
  - `sessionStarted` (504) and `switchedOff` (1380) clear both.
- `ConflictBridge`, the column (MZB-469), in `materialize` (838). It replaces the per-entry loop (918 to 945).
  - `Party` gains `hdx, hdz`, the column's heading, plus `trimDue`, `trimNow` and `stopBeat`. `Stack` gains
    `back`, `side` and `chief`. `Slot.back` stays the slot's own place within its stack.
  - The heading: at radius 0 it is `(cos, sin)` of the record's bearing. On an arc it runs from the centroid to
    the target, as today.
  - Pseudo-code:
    ```
    cursor = 0
    for each entry i (record order):
      base  = radius == 0 ? (cx, cz) : at0[i]
      side0 = radius == 0 ? 0 : (i - (m - 1) / 2.0) * STACK_SPACING
      c     = radius == 0 ? cursor : 0
      if maker(body).pack():
        member j: a stack of 1, back c + PACK_ROW * (j / 2), side side0 + (j even ? PACK_SIDE : -PACK_SIDE)
        c += PACK_ROW * ceil(n / 2)
      else:
        row r: a stack of min(2, n - 2r) slots at lateral(g, rowSize), slot back 0; stack back c, side side0
        c += ROW per row
      if the entry has the chief: a stack of the chief slot alone, chief = true, back c, side side0; c += ROW
      if radius == 0: cursor = c
    each stack: off = -h * back + r * side, with r = (-hdz, hdx)
      anchor = radius == 0 ? base + off : base - h * back
      walk(target + off)
    ```
  - `PACK_SIDE = 3.0` and `PACK_ROW = 6.0` are both PLACEHOLDER.
- `ConflictBridge`, turns (MZB-470), in `steer` (980):
  1. The front stack is the non-GONE one with the least `back`, ties to the lower index.
  2. The head is the front stack's `anchor(now)` less its offset.
  3. `(hdx, hdz)` becomes the unit vector from the head to `(x, z)`; it is kept when the length is under
     1e-6.
  4. Each non-GONE stack's `offX, offZ` is recomputed from its `back` and `side`, and it walks to `(x + offX,
     z + offZ)`.
  - `startRecall` (1024) is unchanged: a recall's walk keeps its offsets.
- `ConflictBridge`, the stop (MZB-472), in `endOfTick` (1332):
  - The beat is `now % STOP_BEAT == 0` (20, PLACEHOLDER), or `p.stopBeat`.
  - In the pending loop, before `tryPlace`: on the beat, `walkIn(p, st, level, now)` for a stopped band.
  - After the loop, on the beat: `clamp(p, level, now)` for each stopped band. Then `stopBeat` is cleared.
  - `square(ServerPlayer pl)` returns blocks `16 * (pcx - s + 1)` to `16 * (pcx + s) - 1` in x, and the same
    in z, with `s = server.getPlayerList().getSimulationDistance()`.
  - `nearest(level, x, z, head)` is the nearest non-spectator player to `(x, z)` that is within 256 blocks of
    the head.
  - `walkIn`:
    1. The entry point is the first point inside the square on the segment from the stack's place to the
       head (clipped as Liang-Barsky does), or else the head clamped into the square.
    2. It sets `ax, az` there and `t0 = now`, then calls `walk` toward the old end clamped into the square.
    3. It sets `dirty = true` and adds 1 to `walkIns`.
  - `clamp`: a PLACED stack whose `(ex, ez)` lies outside its player's square walks from where it stands to the
    end clamped to `[min + 0.5, max + 0.5]`. It adds 1 to `clamps`.
- `ConflictBridge`, the trim (MZB-473), in `endOfTick` after the stop, for each band with `trimNow`, or with
  `trimDue` on the beat:
  1. For each body name among the band's live non-chief slots:
     - `living` counts slots that are `alive && !chief && !removing`, over the non-GONE stacks;
     - `want` is `path.units.getInt(name)`.
  2. While `living > want`, it cuts the slot `pick(p, name)` returns:
     - first, the last live slot of the rearmost PENDING stack;
     - else, the rearmost live slot of a PLACED stack (by stack `back`, then stack index, then slot index,
       each highest first) with no non-spectator player within OUT_OF_VIEW of its entity;
     - else null, which ends the loop and leaves the band blocked.
  3. Without `FLAG_CHIEF`, the chief's slot is cut by the same rule.
  4. `cut(sl)`:
     - `despawn(sl)` (1202), with its extras; then `sl.alive = false`; adds 1 to `trimmed`;
     - a stack left with no live slot is unwatched and set GONE.
  5. `trimDue` is set to `blocked`, and `trimNow` to false.
  6. It journals nothing (MZB-467).
- `Camps.reconcile` (941 to 1009), MZB-464:
  - Before the dispatch, each HAND_BACK line whose `bridge.party(ln.a())` is at epoch `(int) ln.b()` puts that
    party in `homecoming`, whether the camp is in or not.
  - The loop at 1008 calls `homeAt(p, spot)`. With no site spot it uses the band's front stack's anchor.
- `gametest/ConflictKit`:
  - New: `march(MarchSpec)`. `MarchSpec` holds agent, revision, dimension, faction, kind, phase, flags,
    `muster_end`, units as name to count, and waypoints as `long[] {x, z, tick}`.
  - New: `trimmed()`, `walkIns()` and `clamps()` reads.
  - `Band.bearing` exists already.

**Java, plugin** (`neoforge/mowziesmobs/.../mic_rtsim_mowziesmobs/`):
- `ConflictBodies.Maker` (core, M9c) gains `default boolean pack() { return false; }`. It stays a functional
  interface. `RaptorMaker` overrides it to return true.
- No mixin and no event hook (MZB-474).
- D3: no new entity type. D4: the column, stop and trim are core's; the plugin only says which body is a pack.
- UR-1: nothing new is saved in the world or on an entity. The march map is memory, cleared each session.
- D1 (MZB-475):
  - A record costs one decode and one map put.
  - The stop runs on the 20-tick beat, per stopped band, over its stacks and the level's players.
  - The trim runs on a record, then on the beat only while due.
  - Pending tests are as M3's, per changed chunk.
  - Nothing runs per member per tick.

**GameTests** (one batch per test, each on its own far base, MZB-247; scripted through ConflictKit, MZB-478):

22. The march cases of `theCampReconcilerKeepsItsInvariants`: `aBandSetsOutAndComesHomeInPlainView`
    (MowzieGameTests). It is the "in plain view" clause in its moving form; the reload clause's "its party
    marches on" is test 28's.
    - Setup: a test grove that is in, with 4 adopted warriors. A `FakePlayers.add` player stands 150 blocks
      from the grove's spot.
    - `together`: SET_OUT for slots 0 to 3 to agent A, then BAND_MATERIALIZE A with 3 warriors and 1 raptor
      (the maker).
    - Assert: three puppets stand where bodies stood, with health, mask and weapon kept. The only body to go
      is the warrior the raptor stands for, the one farthest from the player.
    - BAND_STEER A 40 blocks out, then back to within 6 blocks of the spot (the walk home: no BAND_RECALL is
      sent). When the head is there, HAND_BACK lines for the 4 slots go.
    - Assert, with a per-tick record of the band's and camp's entities within 128 blocks of the player:
      - no entity appears or vanishes there, except a puppet swapped for its body at the same place in the
        same tick;
      - each warrior is a tagged body where its puppet stood (within 0.01), restricted to the centre;
      - the raptor slot spawns at the far edge, more than 128 blocks from the player (MZB-465);
      - the pack walks off through `homeAt` and despawns out of view, with BAND_DEMATERIALIZED reason 1.
28. `aGroveThatComesInTakesWhatItsMarchLeft`, the GameTest half (MowzieGameTests):
    - Setup: a test grove with 3 adopted warriors and an adopted Umvuthi chief.
    - An Invasion, agent A, is materialized 40 blocks from the grove (2 warriors, `FLAG_CHIEF`, the chief's
      own stack), with its `march` (phase OUT).
    - The grove goes out and comes in, with a `FakePlayers.add` player 20 blocks from the throne. TAKE lines
      for the 3 warriors and the throne body (LINE_CHIEF) follow.
    - Assert, with the player there:
      - the warriors are taken at once, and the throne take is held (Rule U);
      - A's puppets keep their count, and nothing is journaled for A.
    - Remove the player: the throne take is DONE the next tick.
    - Again with no player: every take is DONE in one tick, and A is untouched.
32. `aBandWalksHomeInViewAndIsHandedBack` (MowzieGameTests):
    - Setup: a test grove that is in, and a band (3 warriors, 1 raptor) 60 blocks out at epoch e, with a
      player 30 blocks from it. BAND_STEER toward the spot; HAND_BACK lines as its head arrives.
    - Assert:
      - each warrior is a tagged body at its puppet's place, with mask and weapon, restricted to the centre;
      - the raptor slot is a SPAWN at the far edge while the pack walks off;
      - no BAND_RECALL was needed.
    - The grove out: BAND_RECALL; the puppets walk off and despawn out of view. Then a census and SPAWN lines
      spawn them.
    - MZB-464: the grove released before the HAND_BACK lines, so the lines are STALE. The band still walks
      toward the spot and despawns out of view, with BAND_DEMATERIALIZED reason 1. A line at epoch e + 1
      leaves the band alone.
    - The Umvuthi-on-its-throne clause is M11's (MZB-480).
33. `aColumnAppearsInsideTheTickingArea` (ConflictGameTests, core, batch `conflict_column`, alone because
    the simulation distance is server-wide). It replaces M3's stack test (MZB-481).
    - Setup:
      - Save `getSimulationDistance()` and `getViewDistance()`, and restore them in `ConflictKit.onEnd`.
      - Set the simulation distance to 8, and the view distance to at least 9 (UNVERIFIED that 8 suffices).
      - Force the player's chunk, then `FakePlayers.addSeen` at a far base. Remove it with `removeSeen` before
        releasing.
    - The player stands on its chunk's west column, so at 8 the ticking edge is 143 blocks east of it and
      the square's edge is 127.
    - A band of 3 warrior rows and the chief comes in at radius 0 and bearing 180 (heading -x, toward the
      player). Its head starts 170 blocks east of the player, on the player's z, and it walks at 0.1 blocks a
      tick to a target 60 blocks east.
    - Assert:
      - the stacks appear one by one, front first, each in the tick its last member is inside;
      - BAND_MATERIALIZED is never NOT_TICKING;
      - the placed stacks stay placed.
    - Each stack's distance from the player as it appears is logged against 128 to 144.
    - Steer the band back to 200 blocks east: its stacks return to pending as they leave. Steer it in again:
      they come again, each with `placings` 2.
    - The pending-test count: `tests` rises only when a watched chunk changes or a place moves chunks
      (`moves`).
    - A recall: BAND_DEMATERIALIZED lists every living member, placed or pending.
    - Again at simulation distance 10 (view distance 11): logged against 160 to 176.
    - The stop, at 8:
      - A band of 6 rows and the chief has its target 140 blocks east, inside the ticking area and outside the
        square. It arrives from the east, so its rear stacks' places (to about 149) lie beyond 143. A `march`
        puts it in phase WAITING.
      - Those stacks walk in (`walkIns`) and are placed once. They hold within the square while they walk
        toward the target (`clamps`).
      - None is taken back and placed again while the player stands still. One can be killed, which gives
        BAND_CASUALTIES.
      - A phase HOME `march` and a BAND_STEER home: each stack walks back to its column place.
    - With the target 150 blocks east (outside the ticking area at 8), the stacks hold within the square.
    - A trim: a `march` that lowers the warriors by 2 cuts the rearmost 2 in the same tick (`trimmed` 2) and
      journals nothing. The band stands about 130 blocks from the player, out of view.
      - Again with a second `FakePlayers.add` player 10 blocks from the rear stacks: nothing is cut while it
        stands there.
      - Remove it: the cut follows on the next beat.
    - An envoy (ENVOY_MATERIALIZE) appears as one group.
    - Carry On and `discard()` stay in M3's tests, which are re-run.
34. `theMarchRecordMatchesTheBodies` (ConflictGameTests and a JUnit `MarchPathTest`):
    - JUnit, under `neoforge/core/src/test/java/...` beside `TopologyRecordsTest`: MZB-477's three vectors
      through `chartAt` and `at`, with `LevelTopology.xWrap(0, 64, ...)` and `guyou(64, false, ...)`.
    - GameTest:
      - The layout: a `march` with 2 units and 3 waypoints round-trips into `bridge().march(agent)`.
      - A lower revision is dropped; ENDED drops the path and later records.
      - A band met on the march (BAND_MATERIALIZE at radius 0 with the march's bearing) stands as its column:
        rows two abreast, 1.5 blocks apart, the chief at the rear, and the front stack's head within 0.5 of
        `march.at(now)`.
      - The moving form: on the flat GameTest world, at 0.02 blocks a tick, in short walks to a steered
        restriction centre. Log the column's head against `march.at(now)` every 20 ticks (UNVERIFIED how it
        reads).
- Re-run, after the row split: `aRaidPartyIsSteeredLosesMembersAndIsRecalled` and
  `aChiefWhoseStackHasNoUnitsLeftStillComes` (ConflictGameTests), and Carry On's
  `CarryOnConflictGameTests`. Fix stack counts, never the rule.

**Rust tests:**
- `rtsim-conflict`: a new `tests/mowzie_march.rs`, with `mowzie_records.rs`'s `Run` harness.
  - Records:
    - each mark makes one record per take, with `march_rev` rising by 1 and several changes in one step
      making one record;
    - the march records come before the rest of the outbox;
    - a session start sends every party, but only after `replay_done`. `take_commands` while replaying builds
      none, and keeps the flags;
    - `home` sends the hand-back lines, then ENDED; while replaying it sends no ENDED.
  - Positions:
    - MZB-477's flat vector through `chart_pos` and `party_pos`;
    - the x-wrap and Guyou vectors through `set_topologies` and `party_pos`.
  - The walk:
    - routed, bored and done while embodied: BAND_STEER and no BAND_RECALL;
    - settled, Peaceful, an envoy, `spawned` empty and `replaying`: a recall, as today;
    - each later segment home steers;
    - an edge_stop seam is recalled.
  - Met on the way home: a Home party is materialized with the march's bearing; while replaying it is not.
  - Re-binding:
    - the watcher walks off with a second player 180 blocks off: re-bound, not recalled;
    - the watcher follows at 180: the same player;
    - nobody: RECALL_PASSING;
    - an `Unseen` superseded by a later booking does nothing (`unseen_due`);
    - a booking at the same tick lands at `at + 1`.
  - `player_left`: a fight turns to waiting, with a second player 100 blocks off, and keeps its bodies; a
    siege is recalled.
  - `arrive` with the player 170 blocks from the target: re-bound, resolved abstractly, walked home; with
    the player 150 blocks off, a fight.
  - `start_fight` on an embodied band: STEER_ATTACK, the same epoch, and a wave of only the members not yet
    spawned.
  - `lose_units`: `spawned` clamped and `recalled` not. 6 warriors with 3 lost leave `spawned` 3.
  - `home`'s fallback: a party all Left, a faction with no camp ledger, and a `camp_home` with no hand-back
    line are each recalled (P2-7).
  - Save and load keep `march_rev`, `march_due` and `unseen_due`, and rebuild the list.
- `rtsim-jni` protocol tests, in `tests/protocol.rs`'s conflict module beside
  `casualties_journaled_after_a_save_of_a_band_in_view_are_counted_by_the_replay` (5711). Each keeps live
  players out of the band's view until the replay has caught up (MZB-484).
  - A session start sends BAND_MARCH for every party once the replay has caught up, with the counts the
    replayed casualties left.
  - A restart between a walk-home casualty and its record: the replay counts the casualty once, and the next
    record carries it.
  - `lose_units` across a restart: 6 warriors with 3 killed while resolved abstractly leave 3 after the
    recall, not 0.
  - Test 28's engine half: `order_party` an Invasion; the grove out at its muster's end comes in; the takes
    send no BAND_MARCH (the revision is unchanged); the Invasion reaches its target; a raid and a refill
    meanwhile leave pool plus away at or under the cap.
  - No crash or reassert copy is needed, since M10 adds no holder-carried record (MZB-467).

**Sim runs** (`rtsim-cli sim mowzie`; `RUNS` and `main`'s dispatch in `crates/rtsim-cli/src/sim/mowzie.rs`):
- **Run 11** (`invasion_marches`; `RUNS` keeps "M10, M11", so M11 can add the chief's travel). Players are
  moved through `World::set_player_pos` (rtsim-world interest.rs 121). It must show:
  - an Invasion with its chief from a grove that is out and from one that is in, at 12.6;
  - killed away: the rout, the feud on Scouts, the heir at 12 days;
  - a grove that comes in while it is out takes its throne body, and the Invasion goes on;
  - the held throne body and the seat at the wait's end, and the seat waiting for a fight's end;
  - the re-binds (walked off, logged out, followed), with no recall;
  - the arrival with the only player 170 blocks off: abstract, then walked home;
  - a fight that loses its watcher with a second player near keeps its bodies;
  - the abstract dead counted once at the recall.
- **Run 13** (`march_record`):
  - It steps runs 1, 5, 10 and 11 by 20 ticks. For each party unchanged across a step (plan and
    `march_rev`), it checks `chart_pos` against `party_chart_pos` at every tick of the step. For the rest, it
    checks at the step's end.
  - A record follows each change of route, phase or count.
  - No census ends a party.
  - On an x-wrapped world, a band that crosses the seam and then routs gets a `go_home` route starting at
    `party_chart_pos`: no lap at its front, and a first segment as long as its walk.
  - On a glued world, `canonical_point(x + 0.5, z + 0.5)` floored equals `party_pos` at every tick across the
    seam.

**Work split** (each sub-slice is committed and tested on its own):
- **M10a, engine:**
  1. The merges and the world delete (MZB-450).
  2. `boundary.rs` `chart_pos` and its test.
  3. The `Party` fields, `march_changed`, `march_record` and the marks.
  4. `take_commands` and `home`'s ENDED, with the replay gate.
  5. `persist.rs`, then the regenerated fixtures.
  6. `party_chart_pos` in `go_home`, `steer_segment` and the walk rule.
  7. `seen` and `march_bearing`.
  8. `inside_now`, `rebind_to`, `unseen_due`, `book_unseen`, `unseen`, `player_left` and `arrive`.
  9. `start_fight` on an embodied band.
  10. `lose_units`' clamp, then `camp_home -> bool` and `home`'s fallback.
  11. `FakeConflict`.
  12. `mowzie_march.rs` and the protocol tests, including MZB-466's counter and its no-cascade test.
  13. Runs 11 and 13.
  14. The boundary.md amendments (MZB-482, MZB-484).

  **Opus review here**, focused on:
  - the books: `spawned`, `recalled` and the casualties' cap across a restart;
  - the re-bind loops: no `Unseen` storm, and no recall lost;
  - the replay gates.
- **M10b, Java:**
  1. `ConflictRecords.March` and `MarchPath`, with `MarchPathTest`.
  2. The march map and `march`.
  3. The column in `materialize`, and `Maker.pack()` with RaptorMaker's override.
  4. The turns in `steer`.
  5. The stop: `square`, `nearest`, `walkIn` and `clamp`.
  6. The trim.
  7. `Camps.reconcile`'s homecoming.
  8. The ConflictKit helpers.

  **Opus review here**, focused on:
  - D1: no per-member work per tick;
  - placement: no stack placed outside ticking chunks, and none flickering at the edge;
  - the trim never cutting in sight.
- **M10c, GameTests:**
  1. 33, then the re-runs of M3's tests.
  2. 34.
  3. 32.
  4. 28's half.
  5. 22's march cases.

  **Opus review at the end**, of M10 whole against MOWZIE.md §4.16, ledgers 23 and 25, and tests 22, 28,
  32 to 34.

**No open question.** The coordinator decided MZB-466 (the race at home) on 2026-10-24: it is accepted, with a
counter and a no-cascade test.

**Traps.**
- **The cell fold.** Java folds the block centre, `(x + 0.5, z + 0.5)`. Folding the corner lands on the
  neighbouring block under a half-turn: (-10, 24) instead of (-11, 23).
- **Truncation.** `party_chart_pos` truncates toward zero: (100, -6), not (100, -7).
- **Replay.** Under rtsim-jni every push while `replaying` reaches Java; the outbox filters never see it. Gate
  at the push (MZB-484).
- **The simulation distance is server-wide.** Test 33 runs in its own batch and restores it, and the view
  distance, in `onEnd`.
- **Ticket players.** A `FakePlayers.add` player ticks nothing; use `addSeen`, with its chunk forced first.
- **A tick floor.** `exit_time` can return the current tick, so an `Unseen` re-booked at `at` would loop. The
  booking uses `not_before = at + 1`.
- **Unseen storms.** Without `unseen_due`, each re-bind adds an event and the old ones all fire.
- **The saved outbox.** The outbox is saved, so `march_due` must be too. An unsaved flag loses a mark made
  before a crash.
- **ENDED trims nothing.** The hand-back or the recall ends the band. A trim on ENDED would cut puppets that
  are about to be handed back.
- **The seam gap.** The bridge's points are in the chart (MZB-479). Use `chartAt` for the head, and keep test
  34's moving form on a flat world.
- **The base moves.** `feat/mowzie-m9` is still moving, and M9c's maker is not built yet. Re-check every line
  number above.
- **MOWZIE.md** is at the repo root, not in docs/.
- **The command count.** `take_commands` returns more commands now. Tests that count them, or index into
  them, need the march records filtered out.

## M11 Aloft

(none yet)

## M12 S9

(none yet)

## Backlog: conflict news gaps from TALK

TALK left these five gaps in conflict news; they are the conflict owner's (the coordinator,
2026-10-23), low priority, after M4 and M5 unless one is trivial while that code is open. TALK.md §9's
Feud, Raid and Fell paragraphs describe them.
1. `fell.you` is never told: a Fell's Who is the camp, so the killer hears only through the chief's
   Killing.
2. A fight that ends early (the watching player leaves, the session restarts) and then goes home bored or
   done records nothing.
3. After a restart, a fight broken at the rout share records nothing; only a full wipe-out is recorded.
4. A Fell names the fallen chief as a dead notable ("someone important"), not as the chief: its actor is
   gone when the lines are told.
5. A siege routed after a round tells two reports, the round's and the rout's.

## Hand-off

(none yet)

## Replay pushes (fix/replay-push)

The design is docs/replay-gate.md; each choice names its section there. Every choice is the
conservative option at its fork.

- **RP-1** (one flag; §1.1, §3.1): rtsim-jni's `State.replaying` holds SEC-1's predicate,
  `applied_through + 1 < session_first`, set at SESSION_START and cleared once a session by the replay
  end. Each engine holds its own copy in its outbox, set by `begin_replay` and cleared by
  `replay_done`. Not a second definition: the same predicate, latched so that every module reads one
  answer and the end runs once. Not saved. Rejected: a parameter on every engine call (dozens of
  signatures, and a test can pass the wrong value).
  Amended (SEC-1's meeting-point review, item 5): "the same predicate, latched" overstated it. The
  flag is set at SESSION_START and cleared by the first of the three triggers (§1.1, §2.2): the top
  of `journaled()` on a record's sequence at or past `session_first`, the step's top on
  `applied_through + 1 >= session_first`, and the arm's end with RP-29's journal check, which can
  clear it while `applied_through + 1 < session_first`.
- **RP-2** (one replay end for all four modules; §2.2, §3.2): `shared::replay_end` runs at the top of
  `journaled()` for the first record at or past `session_first`, at the step's top when the replay's
  last record was the hand-over's last, and at the SESSION_START arm's end when there is nothing to
  replay. It replaces conflict's two MZB-141 triggers and `notices_due`. Rejected: one end per module
  (four places that can disagree on when a session's live part starts).
  Amended (the coordinator, on §10's second question): `replay_end` counts the hand-overs the replay
  took and the records it replayed, in a stat and in one info log line at the end, so a slow replay
  shows up.
  Amended (design review): the hand-over count rises at the step's entry whenever `st.replaying` is
  true there, so a short replay counts 1. The trigger at the top of `journaled()` covers every
  module's journaled entry, industry's included, so M7's `rail_call` needs no trigger of its own;
  whichever of M7 and fix/replay-push lands second deletes M7's conflict-only one (replay-gate.md
  §4.4's note).
- The three §10 trade-offs (lose rather than repeat; nothing near players until the replay ends; a
  copied band death shrinks the band by one, the extra body clearing at the next despawn) were decided
  by the coordinator on 2026-10-24, as recommended, not by Ben, who can overrule.
- **RP-3** (the gate at each engine's push; §4.1): an `Outbox` type per engine with a private `Vec`,
  whose only entry is a gated `push`, and one exhaustive `gate` match with no `_` arm giving each kind
  a queued verdict (keep or drop at the session start) and a replaying verdict (send, drop or never).
  At the engine, not at rtsim-jni's `take_commands`: rtsim-host, rtsim-cli and crate tests use engines
  without rtsim-jni, and the crate's privacy stops a push around it. The queued column also stops the
  restored outboxes of dragons, herds and ships reaching a new session unfiltered, as they do today.
- **RP-4** (a "never" kind; §4.1): the decisions that embody return early while replaying, so a
  "never" kind cannot be pushed; if one is, release skips it, counts it and logs it loudly, and tests
  `debug_assert`. No release panic. The skip heals at the replay end, which makes every body abstract.
- **RP-5** (the session start keeps the bodies; §5.1): `begin_replay` changes no body; what the pages
  hold embodied stays so, at its epoch, until the replay end, where today's session start runs
  (moved, §5.6). Rejected: abstracting at the start and patching each replayed record kind to find its
  body (the audit's five LEAKs are that patching, failing).
  Amended by RP-26: the arm's players leave without any module's `player_left`.
- **RP-6** (a replayed *_MATERIALIZED is honoured; §5.2): one dispatched while replaying, and not a
  copy, at an epoch above the agent's, embodies the agent as the last session had it. A live one is
  handled as today. Conflict counts its casualties by body kind against all alive units, since the
  record names how many were placed, not which.
  Amended by RP-22 (conflict's fill), RP-23 (a held party's fight) and RP-31 (only an engine that
  started from pages honours).
- **RP-7** (epochs never reused; §5.3): an agent's next epoch is above every epoch seen for it, in the
  pages, a replayed *_MATERIALIZED and a copy. Each module with epochs (conflict, dragons, herds, ships)
  gets its own test.
  Amended by RP-30: the floor is held apart from the epoch until the replay end.
- **RP-8** (copies never match a body; §2.3, §5.7): a REASSERT's content is an earlier session's, so a
  copied loss applies by identity (party and body kind, member key) and a copied *_MATERIALIZED only
  raises the epoch floor. rtsim-jni's `st.applying_copy`, set around S4p's `reasserted()` dispatch,
  tells the modules; a field, so S4p's signatures stay.
  Amended (design review): the SESSION_START arm clears `applying_copy` first, since a step that
  panicked inside a copy's dispatch (caught) leaves it `Some`. Amended again (SEC-1's meeting-point
  review, item 4): that cannot happen, since a panicked step fails the engine for good and no later
  step reads a record; the reset stays as a defence.
- **RP-9** (a copied BAND_CASUALTIES; §2.3, §5.7): `Conflict::ingest_copy` counts it against the
  party's alive units of that body kind, by party alone, touching no stack. Today, on main and on S4p,
  such a death is dropped by `party_of`'s exact epoch and the member lives on. Slice C also tests
  MZB-131's live walk-off death of an old epoch while the band is embodied at a new one, first, and
  fixes it the same way if it fails. Safe because a named generation's new copy was never applied (SEC-1); the
  gap case (pages older than the journal, boundary.md §3.2's "Known limit") can count a death twice
  and stays that limit's case.
  Amended by RP-25 (the stacks capped after a copied death) and RP-27 (the MZB-131 fix, by epoch).
- **RP-10** (MZB-138's fold goes; §5.4): the fold of embodied bodies into recalled stacks at the
  session start existed so replayed records found them; RP-5 keeps them embodied instead. The replay
  end drops every recalled stack, as `replay_done` does today.
- **RP-11** (sightings wait; §6): `shared::simulate` returns at its top while replaying, keeping the
  latest position per player, and every decision that embodies returns early. The kept positions are
  routed in the step the replay ends in, or the first one after. Costs a bool test per step; loses
  dragons' home counts and conflict presence for the replay's hand-overs. Phase 1 is untouched (the
  function serves the four modules only).
- **RP-12** (conflict re-derived news; §7.3): NOTICE, DEMAND_SETTLED and TRAIN_ROBBED are dropped while
  replaying. Kept notices go at the replay end (MZB-31, unchanged); a settled demand's news and a
  roar decided after the last save are lost, never shown twice (Questions for Ben 1).
- **RP-13** (dragons ADOPT and DISCARD; §4.5): dropped while replaying. DragonBridge reports an untagged
  dragon once per entity per session at join and never saves the tag, so the live report after the
  replay end decides again (adopt_again at a new epoch).
- **RP-14** (NEST_RENEW; §7.1): a renewal serial per nest, carried by the command, applied by Java
  only above the last it applied. Not an absolute owed count, which races Java's own placing and can
  double; not a plain drop, which loses a renewal. An old Nests file loads owed 0 and serial 0: never
  doubled. Slice D drops NEST_RENEW while replaying until slice N adds the serial.
- **RP-15** (NEST_SORTIE; §7.2): dropped while replaying. Loses the visible worms of a raid on a
  watched district decided again; the raid's effect on the district stands.
- **RP-16** (the DEPART trap; §5.8): a DRAGON_DEMATERIALIZED of reason NOT_EMBODIED carries only the
  agent and the epoch, and the engine took its zeros (health 0, hunger 0, a rest at (0, 0, 0)). It
  takes no position, health or hunger and resumes the dragon where the engine had it, as
  SPAWN_REJECTED does. Slice D, with a test.
- **RP-17** (landing order; §2): neither fix/replay-push nor feat/sec1-s4p must land first; whichever
  lands second does §2.7's merge and writes §2.5's four tests.
- **RP-18** (the API; §3.4): each engine's `session_start` is removed and replaced by `begin_replay`
  and `replay_done`, so that every caller fails to compile and chooses. Rejected: keeping
  `session_start` as both in one (callers would silently get the old behaviour).
- **RP-19** (a new engine life; §2.6, §7.1): Java keeps the RG13 engine life beside each nest's last
  serial and starts a nest's serial afresh under a new life; the owed count stays.
  Amended (design review): so slice N goes after D and after RG lands; RG13's engine life is only on
  feat/reinstall-guard (35e9b082 and later).
- **RP-20** (where the end is checked; §2.2): at the top of `journaled()`, after its `save_through`
  block, before S4p's branches and `admitted()`: it sees every kind and reads only the sequence, on
  both branches.
- **RP-21** (a copy of generation 0; §2.3, SEC-1's review): S4p judges a copy of nonce 0 new every
  time and records none, so repeats each apply unless an op id stops them. BAND_CASUALTIES has no op
  id: a copied one of generation 0 is refused and counted, never applied. Loses a death that was
  really lost (a member lives on); never kills one twice. CHIEF_KILLED (op id) and herds' losses
  (member key and op id) apply as RP-8 says.
- **RP-22** (conflict's fill for an honoured BAND_MATERIALIZED; §5.2; amends RP-6; the design
  review's item 1): `spawned` is rebuilt by `spawn_wave`'s own fill, one function for both, one fill
  per wave from the party's `wave` to the record's: the units in order, each up to its alive members
  not placed, within `spawn_quota` less the bodies placed and less the epoch's deaths since the
  party's previous honoured record. Never above what Java was commanded; a count below it loses
  nothing journaled. An honoured envoy record fills nothing, as `materialize_envoy` records nothing
  live (decided by the coordinator, 2026-10-24; the review asked for every unit, which would part the
  replay from the live path). Rejected: every alive unit
  (RP-6 as first written: an invasion of 26 with 16 placed despawns to 16); the record's `spawned`
  (it omits the pending members Java reports at the despawn).
- **RP-23** (a war party's arrival waits; §6.1; the review's item 2): while replaying, the branches
  of `arrive()` and `wait_over()` that ask `watcher()` hold the party (in memory, its arrival tick)
  and poll with its own Ev::Party every `REPLAY_HOLD_TICKS` (PLACEHOLDER 20), so pages saved
  mid-replay keep an event and no format changes. A replayed, honoured BAND_MATERIALIZED of a held
  party takes `start_fight`'s state without commands; otherwise the poll at the hold's slack
  (RP-48), or the first after the replay end if that comes first, decides as today, normally
  abstractly. Loses rather than doubles: a crash before the fight's BAND_MATERIALIZED was journaled,
  and a sighting inside the slack read as a fight (amended by RP-48: a sighting on the way home or
  while a siege waited was read as a fight however late it came). Slices D and S check dragon raids
  and pirate coast raids for the same pattern. Rejected: resolving at
  `replay_done` directly (no event in pages saved mid-replay; a restart from them would strand the
  party).
- **RP-24** (the clock follows the records; §3.6; the review's item 3): while replaying, with any of
  the four modules running, every read of the hand-over's time that runs the clock is held at
  `st.replay_clock`, the latest replayed record's world tick: the modules' `simulate`s, phase 1's
  world, ships' `build`, herds' `found`, the non-journaled records applied at the hand-over's time;
  worldmodel founds nothing. The world is held with the modules, since the five share one clock and
  the world never steps back; the clamps to `phase1.world.now()` stay (the review suggested dropping
  them; with the world held they carry nothing forward, and they still guard the world's past).
  Industry is not held (its own clock and replay rules); a held module's train lookups and raids
  then meet industry's later state, a known limit (decided by the coordinator, 2026-10-24). A
  phase 1 world alone is not held. Rejected: holding the four alone (the world's run and the next
  clamp carry them forward).
- **RP-25** (the copy cap; §5.7; amends RP-9; the review's item 4): after a copied BAND_CASUALTIES,
  `spawned` and every recalled stack of that unit are capped at its alive units, as the recalled
  despawn already caps `spawned`. Without it the next despawn undoes the death (6 placed, a copy,
  6 survivors: 6 again). §10's third answer ("the extra body clears at the next despawn") holds
  only with this cap.
- **RP-26** (the arm forgets players without `player_left`; §2.1, §5.6; amends RP-5; the review's
  item 5): `shared::session_start` calls `World::forget_player_pos` for each player with an area and
  no viewer's `player_left`, which would recall conflict's watched kept bands and despawn herds'
  groups. Each module's `replay_done` does the player work instead (conflict's presence spans,
  dragons' and ships' interest legs, herds' interest and `pos`). Slice C leaves only conflict out
  of the arm's `player_left`; slices D, H and S each leave their own out with their `replay_done`.
- **RP-27** (a casualty's stack by epoch; §5.9; the review's item 6; MZB-131): `casualties()` finds
  the party by agent, then takes from `spawned` when embodied at the record's epoch, else from the
  recalled stack of that epoch, else counts nothing (a straggler). Rejected: by agent alone (an old
  epoch's death would come off the new stack and count again at the old despawn).
  Amended (the re-review's item 7): a walk-off death also takes RP-25's cap, `spawned` and every
  recalled stack of that unit capped at its alive units. Without it `spawned` stayed above the
  alive, and a plugin that despawned the newer epoch first, holding every body it placed, undid the
  death (12 placed, a walk-off death, 11 alive; the despawn of 12 gave 12). ConflictBridge ends the
  old epoch before it materializes the new one, and that despawn clamps `spawned` itself, so the
  bridge was safe; the cap makes the order not matter.
- **RP-28** (backstops; §6): `start_fight` and `wave` return at their top while replaying, `wave`
  without scheduling the next; the gate stays the last backstop (RP-4).
- **RP-29** (an empty replay ends at the arm; §2.1): at the SESSION_START arm's end the replay also
  ends when Java's journal starts at or past `session_first` (`max(applied_through + 1, jf) >=
  session_first`), so main without S4p does not wait for the first live record. A pinned JOURNAL_GEN
  below `jf` after this end is harmless (never dispatched to a module); the S4p merger checks.
- **RP-30** (the epoch floor apart from the epoch; §5.3; amends RP-7): an in-memory map per engine
  of the highest epoch seen that the agent's `epoch` does not hold; the next epoch is
  `max(epoch, floor) + 1`; `replay_done` folds the floors into the epochs (everything is abstract
  then) and clears the map. Rejected: raising `epoch` itself (an embodied party's records at its
  current epoch would stop matching). Lost at a restart, harmless by RP-8.
- **RP-31** (only an engine that started from pages honours; §2.6, §5.2): `begin_replay(from_pages)`;
  a fresh engine (first start, reinstall, a module built without saved state) honours no replayed
  *_MATERIALIZED and only raises floors, so an old life's record on a reused handle embodies
  nothing. Loses: a fresh engine's own records journaled before its first save are not honoured
  after a crash before that save (today's behaviour). RG13's engine life could later tell the two
  apart.

- **RP-32** (the hand-over count; §2.2): the hand-over that carries SESSION_START enters the step
  with the last session's `false`, so the arm sets the count to 1 when the replay is still on at
  its end; each later hand-over counts at the step's entry. A replay that ends at the arm counts 0,
  one that ends inside the session start's hand-over 1. The counts are JNI `Stats` atomics
  (`replays_ended`, `replay_handovers`, `replay_records`), not Java's stats array.
- **RP-33** (the clock's reads; §3.6, RP-24): one function, `shared::hand_over_target` (the
  hand-over's world tick through `clock_target`), at every read that runs the clock: the four
  modules' `simulate`, dragons' and herds' `reset_if_due` (conflict's and ships' read no time),
  phase 1's `simulate` and `input_now` (its latest-state inputs), the STATE kinds applied at the
  hand-over's time (dragons', ships'), the rejects (conflict's, ships', herds', dragons'
  `write_queue`), worldmodel's founding. Ships' journaled path raises the replay clock too
  (`catch_up_to`). Worldmodel's guard is `st.replaying` itself: nothing is founded while replaying,
  the regions stay pending. Not held: industry (RP-24).
  Amended (the fix round's item 3): the test orders that run the clock to the hand-over's time
  (the works grants and raids, `phase1::directive`; TRIBUTE_START; HERD_FOUND) are queued until the
  replay end, not held at the replay clock. Taken at once, one runs the world to Java's present, and
  the next replayed record's clamp (`max(record, conflict.now, phase1.world.now)`) carries the
  conflict there: RP-24's fault. Held at the replay clock instead, each would take effect in the
  replayed past (a grant spent, a tribute begun, a herd founded before Java gave the order). Queued,
  a unit that holds one, and every unit after it so that units keep their order, is taken at the
  first step that starts after the replay end, at the hand-over's time as any order is: after every
  replayed record, and no earlier than Java gave it. In memory (`held_units`), lost with a crash
  before the end as any untaken order is. The other orders run no clock: commands are queued for
  Java, SaveAtNextStep waits anyway (RP-42), and ScriptVillage, Phase1SpawnNear, NestDay and
  TalkReport move no time. Phase1Advance (a test's fast-forward of the world-tick mapping) is not
  held; a test gives it outside a replay. Failing first, a kept fight's casualties replayed
  over several hand-overs with the order taken in the second: a WorksGrant and, apart, a
  HERD_FOUND (herds on) each undid all 3 (12 out): the camp's pool's closed form 26.92, live 23.92.
  Amended (the re-review's item 5): a SESSION_START drops the held units (they were the last
  session's), with a warning and the pending count corrected; a works raid and a TRIBUTE_START
  (dragons on, its village given works and a purse and its lair found before the save) each have
  their test, and each fails with its kind taken at once: 26.92 against 23.92 again.
- **RP-34** (the arm's leave; §2.1, RP-26): `shared::session_start` leaves each player's area
  through `with_viewers_of(st, false, ..)`, every viewer but the conflict; the conflict's
  `replay_done` ends every presence span at once (`Presence::every_player_left`).
- **RP-35** (where a replay's state lives; §3.3, §4.3): the Outbox holds `replaying`
  (`Conflict::replaying()` reads it) and the gate's counts, cumulative since the engine was built or
  loaded, so `ConflictStats` (saved) gains no field; `defer()` is the early return and counts it;
  `last_never` keeps the kind and name for the log. Only conflict's Link has `replay_end_due`;
  dragons', herds' and ships' `replay_end` are empty until their slices.
- **RP-36** (`resend_notices` ends the replay; §5.6): it calls `replay_done` first (a no-op unless
  replaying), so a caller that only resends cannot leave the gate on. Its retain of queued notices
  stays; rtsim-jni's replay end asserts instead that none is queued (the gate drops them).
- **RP-37** (the floors' bookkeeping; §5.3, RP-30): keyed by agent; raised only when the party does
  not hold that epoch; spent (removed) by `next_epoch`.
- **RP-38** (the honour's edges; §5.2): the epoch's deaths since the last honoured record are taken
  once per record; a record of the same epoch when the party is not embodied changes nothing (its
  body is gone); a held arrival takes the fight's state only on a newer epoch (`start_fight`
  always takes a new one).
- **RP-39** (copies; §2.3, §5.7, RP-21, RP-25): the generation-0 refusal sits after `translate`
  and before the `Conflict::later` check and the catch-up, so a refused copy moves no clock; the
  cap applies to copies only (a live casualty takes from its stack). The rtsim-jni test of a
  generation-0 copy waits for S4p: nothing on this base sets `applying_copy`.
- **RP-40** (the held arrivals' details; §6.1, RP-23): `held` maps a party to (since, plan); a
  poll is answered only while its plan matches, else the entry goes. After a restart from pages
  saved mid-replay the map is empty, so `held_since` infers the hold from the pages (out, a war
  party, on its last segment, its arrival due; or a waiting siege whose round is due), and a poll
  that arrives again keeps the route's last time as its arrival and its patience from there.
  A siege's hold is from `until.min(at)` and changes neither `seg` nor `bored_at`. `fight_held`
  leaves `seg` at the route's end, as a live arrival does, ends no earlier than the next tick,
  counts `raids_watched` and schedules no wave (`wave` is held anyway, RP-28). Since RP-42 the
  host takes no page while replaying, so the inference from the pages is reached only from
  engine-level tests that save mid-replay (test (b)'s restarts); kept as a defence (the
  re-review's item 4).
- **RP-41** (the tests): (b) is engine-level, in rtsim-conflict's mowzie_records, where an
  Infrastructure that records every `raid` shows an abstract resolve; (c) lowers the camp's pool so
  that it shows the band coming home, and compares both the pool's closed form at a fixed time and
  its raw value and time (the closed form alone does not show when the band came home). The crate
  tests that called `session_start` call `begin_replay(true)`, and `replay_done` where they wanted
  the old reset's effects; `boundary::tests::cmds` is `pub(crate)` for the gate's table test.
- **RP-42** (no save while replaying; §2.2, §3.2; the fix round's item 1): `journaled()` ends the
  replay before its `save_through` block, so a save due at the first live record is taken after
  `replay_done`. Beyond the brief's two paths, every path that takes pages waits while replaying:
  the step end's `SaveAtNextStep`, a `save_through` already due, and `close(save = true)`, which
  keeps the pages as they were (they were taken mid-replay before: durable 1025, want 2). A request
  made while replaying is taken at the first journaled record or step end after the replay end. The
  budget save waits too, so the resident rows may pass the budget by what the replay's hand-overs
  change: 56,792 B at the peak against 4,096 B over 5 hand-overs, 2,680 B after the end's budget save.
  Real Java asks for a save through `nextObsSeq - 1`, which is at or past its session's JOURNAL_GEN,
  so a bridge that names its generation never asks below `session_first`; the ordering case needs a
  bridge that names none (a level save before the session's first record) or a direct request, which
  the tests use. A held raid is not read as a fight on either code: a band at home is never seen, and
  the pages' held state (out, `seg` one short, the poll pending; `held` is not saved) is the same
  whether they were taken before or after the replay end, since `replay_done` leaves `seg` and the
  poll as they are; the test is kept as the control.
- **RP-43** (§2.5's tests, written by the merger at 96ca2219): tests 1, 2, 3 and 5 sit in rtsim-jni
  tests/protocol.rs's `conflict` module, not tests/reassert.rs, because the camp, raid and band
  helpers they need live there; test 4 (pinnedGen, no conflict state) sits in tests/reassert.rs
  beside S4p's. Without the copy bit (`applying_copy`) five fail: the live materialized copy embodies
  ([1], want [2]), the unknown-generation copy takes 0 of 6, the replayed and live casualty copies
  leave 12 alive (want 6), and generation 0 refuses 0 (want 4). The replayed materialized variant
  holds either way, since a replayed BAND_MATERIALIZED raises the epoch floor too; it is kept as the
  control. Test 4 cannot tell an end right before the JOURNAL_GEN from one right after it (a
  JOURNAL_GEN is never dispatched, so nothing reads the difference); it does fail with no end at the
  record, when the step's top ends it after the session's live transfer, counted as replayed
  ((1, 1, 4), want (1, 1, 3)).
- **RP-44** (phase 1 shows nothing while replaying; §6; the fix round's item 4): `phase1::simulate`
  skips `interest()`, `markets()` and the queued P1_SPAWN_NEARs while `st.replaying`, whether or not
  a module holds the world: the replayed records still to come change what a player would see
  either way, and the brief names `st.replaying`. The whole of `interest()` waits, not only its
  materializing half: phase 1's session start leaves no puppet embodied, so nothing is there to
  steer or capture while nothing is materialized. `last_interest` and `last_market` are left as
  they were, so both run in the first step after the replay end. The world's own commands
  (`take_world_commands`: gossip, works) still go as before, outside this item. Failing first, three
  villages and a goblin camp far off, a player in village A with a caravan near it at the save,
  4,000 fillers, and a replay at MIN_CAPACITY: (markets, caravans) sent while replaying (1, 2),
  want (0, 0). The control, the same replay in one hand-over, sends both in its first hand-over
  on either code.
- **RP-45** (a held siege's patience; §6.1; the fix round's item 5): `arrive()` takes a war party's
  arrival as its route's last time, capped at `at`, for every war arrival, not only in the hold:
  after the replay end `replay_done` clears `held`, so the poll reaches `arrive()` again at the
  poll's tick, and `bored_at` was reset from there. Live, a war party's last segment is scheduled
  at its route's last time and never late (the Out chain runs from the muster's end, and Peaceful
  stands a mustering war party down), so live arrivals are unchanged. Envoys and scouts keep `at`:
  an envoy held through Peaceful while mustering (MZB-132) can arrive after its route's last time,
  and its patience runs from its real arrival as before. Failing first, a siege on the refinery
  with nobody near, the pages saved 3,000 ticks before its arrival, the replay 600 past it: the
  replayed siege's (`bored_at`, turn for home) (123570, 123570), want the session that went on's
  (122950, 122950), 620 ticks late (the replay's 600 and one 20-tick poll).
- **RP-46** (honour as live at a newer epoch; §5.2; the fix round's item 6): `honour` at a newer
  epoch clears `spawned` without folding it into a recalled stack, as live `start_fight` (and
  `seen`, which needs the party not embodied) takes a new epoch. A live recall still folds
  (MZB-131). With the real bridge the fold seldom had anything to fold: `ConflictBridge.materialize`
  ends an older epoch's party (BAND_DEMATERIALIZED, reason COMMAND, its survivors) before it
  journals the newer one's BAND_MATERIALIZED, and that despawn, replayed while the party is
  embodied at the older epoch, takes `dematerialized`'s live branch first. The fake plugin drops
  the older bodies silently, so a record of the older epoch after the newer one's materialize
  shows the difference. Failing first, the watched raid of RP-23's test (b), seen on its road at
  one epoch and fought at the refinery at the next, with one older-epoch body's death after the
  fight's BAND_MATERIALIZED: live counts it against no stack; replayed, (casualties, pool on day 9)
  (4, 35.998), want live's (3, 36.998). The engine test of §5.2's honour now asserts no recalled
  stack at the newer epoch.
- **RP-47** (a held arrival across two crashes; §6.1; the fix round's item 8): the test sits in
  rtsim-jni tests/protocol.rs, not at engine level, because "no double" rests on the host: a
  session that crashes before its replay end takes no page (RP-42) and the journal keeps its
  records, so the next session replays from the same pages. A resolution is counted by the
  engine's log note ("finds nothing to raid", a raid on a place with no district), since
  ConflictStats takes no new field. Unwatched only: a watched arrival's honoured fight is read
  from the same records in each session, as in the one-crash tests. S2 replays S0's 4,000 records
  and S1's 20, not S1's JOURNAL_GEN (never dispatched). With the arrival's hold off, S1 resolves
  it while replaying (Home, want Out). A second case ends S1 with a clean close (`close(true)`,
  the path RP-42 changed; the re-review's item 3): with `close`'s guard off, S1 saves mid-replay
  and S2 opens on pages durable through observation 1,026, not S0's save at 3, and replays 2,997
  records, not 4,020. The outcome holds even then (resolved once, S0's books), through
  `held_since`'s inference from pages saved mid-replay (RP-40), so the test checks the pages and
  the replay's records too.
- **RP-48** (a held decision's slack; §6.1; amends RP-23; the re-review's item 1): while
  replaying, a held arrival or siege round waits for its fight's BAND_MATERIALIZED only
  `REPLAY_HOLD_SLACK_TICKS` (600 world ticks, 30 s; the shortest siege interval is a day). Amended
  by the RP-48/RP-49 check (fixes4 item 1): no hard bound exists on the delay from a watched
  decision to its record; it is normally 1 to 2 ticks, and a spawn group held by Java's spawn quota
  (`DEFAULT_SPAWN_QUOTA` 8, which holds every later command), a full command buffer or a lagging
  engine stretches it. 600 covers 30 s of stall and is no longer a placeholder; a fight recorded
  past it was a double, and since RP-56 is lost instead (RP-51 measures the delay). Its last poll is scheduled
  at `since` + the slack; there the Ev::Party arm drops the hold and `party_event` runs `arrive()`
  or `wait_over()`, which, still replaying and past the slack, take the unwatched branch at once.
  The replay clock follows the records (§3.6), so that poll runs after every record before the
  slack and before any at or past it. `held_since` answers only inside the slack, so a later
  sighting finds no hold and embodies the band as `seen()` did live. The decision is taken as of
  the hold's tick: `wait_over` reads the round as `until.min(at)`, and both it and `arrive()` check
  patience at that tick and set a siege's next round from it, so a later round falls where the
  session that went on had it (live the tick is the event's own). Residual, closed by RP-56 (the
  record's free bytes 44 to 47 carry the command's echoed flags): a sighting inside the slack was
  still read as a fight. No rule without one exists here: the record carries the agent,
  epoch, dimension, wave and units spawned, a fight's first wave and a sighting's are both wave 0
  at the next epoch, and Java journals both as it applies the command; telling them apart needs a
  field, a format change. Time: an unwatched decision while replaying resolves up to the slack
  late. Three tests that resolved at the replay end with a journal past the slack now resolve at
  the slack (an unwatched arrival, a siege round past the slack) or end the replay inside it
  (RP-45's siege). Failing first, a siege of patient goblins on a refinery, its round at R1
  unwatched, the pages from before R1, replayed against the session that went on as (phase,
  embodied, epoch, `until`, `raids_watched`, raids): a sighting while it waits, (Fighting, true, 1,
  85270, 1, []) against (Waiting, true, 1, 103270, 0, [79270]); a fight at R2, (Home, true, 2,
  102673, 1, []) against (Fighting, true, 2, 109270, 1, [79270]); a held arrival then a sighting,
  (Fighting, true, 1, 61270, 1, []) against (Waiting, true, 1, 79270, 0, [55270]). Rejected:
  telling the records apart by position (a waiting siege is seen at its target).
- **RP-49** (the replay's resident peak; §3.2; RP-42; the re-review's item 2): no save path while
  replaying (the reviewer's pending-generation save is a design change, not this slice's). The
  bound instead: the replay opens on the last durable save, the last one Java asked for, since the
  budget's saves are pending generations that a crash rolls back (PF15); taking no page until its
  end, it holds every row the replayed window touches. So its resident rows are bounded by the rows
  the crashed session touched between its last durable save and the crash, plus, for each session
  that crashed before its replay end (RP-47), the rows its own records touched. Java's save cadence
  limits that window, not the budget. The live session's budget saves kept it near its budget and
  a step, so the replay passes the live peak by the window's rows less the budget, which grows
  with the window. Measured in rtsim-jni's
  `a_replays_resident_peak_is_the_rows_touched_since_the_last_durable_save` (20 camps a hand-over
  under 4,096 B, no save asked for): the live peak 5,776 B both times; the replay 28,528 B after
  100 camps and 56,792 B after 200, against the 28,616 B and 56,880 B the same sessions touched
  with a budget they never reached. The test asserts the bound and the growth. Reported to the build
  lead as a finding (an excess that is no constant); no fix built. Amended by the RP-48/RP-49 check
  (fixes4 items 6 and 7): the window is normally empty or one vanilla autosave interval (Java asks
  for a save on every overworld `LevelEvent.Save`, BridgeEvents.java:175-186, every 6,000 ticks,
  scaled by `/tick sprint`; save-off stops the journal write as well); the worst case is every paged
  row of every module, the old unpaged footprint, and for the camps above only with item 6's bug or
  the causes RP-55 logs. Item 6's bug: `request_save` replaced a pending request, so an engine that
  lagged the journal by more than one save interval had its target pushed past it at every world
  save and took no save at all; fixed in 63d1b4fa, which keeps the oldest pending request. Whose
  code: 0c66e75c (2026-10-20, "rtsim-jni: the native link crate", an agent session co-authored by
  Claude Opus 5.5 (1M context)), on main through the overnight/jni merges 3e5a7e0c and 3d47d4e6;
  d1edb812 only reflowed it. The herds, the largest paged module, do not fit "normally small",
  measured in `a_replays_herd_rows_are_the_rows_touched_since_the_last_durable_save` (128 herds, a 4
  KiB row budget, 50 transfers a step of 1,200 ticks, no save asked for, a crash; the replayed
  engine outside TEST mode, RP-55): the replay held 95,540 B after 2 steps (2,400 ticks, less than
  one autosave) against 186,606 B unpaged, and 191,730 B against 203,970 B after 80 steps; the live
  peak under the budget was 1,232 B or 95,540 B (2 steps) and 1,400 B or 103,406 B (80), by when
  the IO lane's saves came back, so it is printed, not asserted. A herd page holds 32 herds and one
  herd's event dirties it, and no page is taken while replaying (RP-42), so the herd replay's rows
  grow fast with the window: about half the unpaged footprint after 2 steps, nearly all after 80.
  Corrected by RP-55 (fixes5 item 3): the 177,178 B first recorded here for 2 steps, and "every
  start loads every herd page", were TEST mode's read-back hash, main's, not the replay's.
- **RP-50** (a copy whose op id was heard; §2.3; SEC-1's meeting-point review, item 2): in
  `reasserted()` the copy bit is set and `copies_applied` counted only on `admit_copy`'s Apply. A
  new copy whose op id the engine already heard (generation 0 is new each time) is dispatched as
  DuplicateOp without the bit and counted in `copies_dropped`, so every admitted copy counts once,
  applied or dropped. `dispatch` debug-asserts the bit only on Apply. No module read the bit on that
  path, so only the count moves. Failing first, rtsim-jni's
  `a_copy_whose_op_id_was_heard_is_dropped_not_applied` and its replayed twin: (applied, dropped)
  (1, 0) against (0, 1); with the assertion and not the fix, both stop at "the copy bit on a
  DuplicateOp". Rejected: leaving a duplicate-op copy out of both counts.
- **RP-51** (the slack from data; §6.1; RP-48; fixes4 item 1): `ConflictStats::fight_record_gap_max`,
  the largest gap from a live `start_fight` to its fight's first BAND_MATERIALIZED (same epoch),
  measured at the record's ingest tick, the one `held_since` compares with the slack. `start_fight`
  notes (epoch, tick) per party in an in-memory map, `fights_unanswered`; the first record of that
  epoch takes it out and raises the stat; a sighting's record has no entry, and a replay none (the
  map is per process, and `start_fight` does nothing while replaying). An entry whose record never
  comes (a rejected spawn, a crash) waits for the party's next fight or its removal. Not saved: a
  field in the saved stats vector would change the page, so the stat is this process's, and
  rtsim-jni logs each new largest (info; a warning at or past the slack), which a server's log
  keeps across restarts. Failing first, mowzie_records'
  `a_watched_fights_record_gap_is_measured_from_its_decision` (the band seen on its road, then
  fought at its arrival; the plugin answers a tick later): 0 against 1. The sims' JSON gains the
  field.
- **RP-52** (a held round's walk home, labelled by the round; §6.1; RP-48; fixes4 item 3):
  `wait_over` labels the walk home bored when the round (`until.min(at)`) is at or past `bored_at`,
  not the call's tick. Live they are the same tick; a held siege round resolved at the slack's
  poll is up to the slack later, so a round that kills the last of the band within the slack
  before its boredom went home bored in the replay and done in the session that went on. Failing
  first, mowzie_records' `a_held_round_that_kills_the_band_before_its_boredom_sends_it_home_done`
  (every raider held at the walls, `abstract_loss_share` 1, `bored_at` the round + 300): the
  replay's bored count 1 against the live 0. The sims are unchanged.
- **RP-53** (the inference from the pages gates on the round; §6.1; RP-48; fixes4 item 4):
  `held_since`'s inference for a siege waiting past its round now asks `until < bored_at` (the
  round before its boredom, as `wait_over` holds it) instead of the record's tick before it. The
  map answers by the hold's plan and slack alone, so a fight record past the boredom but inside the
  slack was honoured with the map and refused after a restart that lost it. Failing first,
  mowzie_records' `a_held_rounds_fight_past_its_boredom_is_honoured_after_a_restart` (`bored_at`
  the round + 110, pages at + 105, the record at + 115, before the poll at + 120): after the
  restart (Home, epoch 1, until R + 601, watched 0, raids [R + 600]) against the map's (Home,
  epoch 1, until R + 116, watched 1, no raid). The sims are unchanged.
- **RP-54** (a held fight that Java refused; §6.1; RP-23; fixes4 item 5): a replayed
  BAND_SPAWN_REJECTED of wave 0 at an epoch newer than a held party's, inside the slack, on an
  engine from its pages, ends the hold as the fight: `held` loses the party, which takes the
  record's epoch, not embodied, nothing placed, `wave` 1 (as `spawn_wave` left it), then
  `fight_held`. Live the refusal leaves the band fighting unembodied to the fight's end; the replay
  read it against the older epoch (`party_of` found nothing), kept the hold and resolved the
  arrival unwatched at the slack. Failing first, mowzie_records'
  `a_held_arrivals_refused_fight_is_fought_when_replayed` (`Run::refuse` turns the plugin's
  BAND_MATERIALIZED of the band's fight into a refusal, reason 1): replayed (Home, false, epoch 1,
  until 24020, watched 0, raids [55870]) against live (Home, false, epoch 2, until 61270, watched
  1, no raid). Corrected (fixes5 item 2): BAND_SPAWN_REJECTED has no free bytes (24 u64 agent, 32
  u32 epoch, 36 u32 wave, 40 u64 cmdSeq, 48 bytes), so RP-56's echoed flag cannot go there, and no
  layout changes on this branch. The residual, accepted by the coordinator: a sighting refused
  inside a held fight's slack is read as the fight's refusal, so the hold ends early, at the
  refusal, and the fight's own BAND_MATERIALIZED then finds no hold and is lost as RP-56 says (its
  casualties and loot count against nothing). The books hold (one resolution); only live parity is
  lost, and only for a sighting refused inside a held fight's slack. The option not taken, pending
  the coordinator, who wants no wave-field change now: the refusal's `wave` is a u32 and
  BAND_MATERIALIZE's a u16, so its high half (bytes 38 and 39) could carry the echoed flags; noted
  in boundary.md's BAND_SPAWN_REJECTED row and §6.1 for whoever next changes that layout. A
  replayed refusal of a newer epoch for a party not held
  raised no epoch floor, so the next decision after the replay could take the refused epoch again;
  closed by RP-57. The sims are unchanged.
- **RP-55** (the replay window's growth, logged; §3.2; RP-49; fixes4 item 7): the engine logs a
  warning, and counts it in `Stats::window_grew` (a test hook, not in Java's array), each time the
  next start's replay window grows past one save interval: a save Java asked for that is not made
  (the IO lane refuses it, the poisoned store does not reopen, a stale save built before a failed
  one, the page index unread, the commit fails); an engine that stops simulating (a step's error or
  panic: no save until the next start); and a replayed JOURNAL_GEN from before this session's first
  observation that admits as new (an earlier session started past the pages and ended with no save:
  a crash, or a close, before its replay end or its first save, §6.1's "More than one crash"). The
  line names the cause and the last durable observation. A budget's save that fails is not logged:
  it is a pending generation, which a crash rolls back anyway. A close while replaying (RP-42) logs
  nothing itself; the next start's JOURNAL_GEN line covers it. The tests read the counter, since
  `log_warn` goes to the host's ring, which drops lines when full. Failing first, the counter's
  increment cut: `an_engine_failure_publishes_empty_buffers_flagged_failed` 0 against 1,
  `a_save_failing_partway_costs_that_save_not_the_engine` (TornLog) 0 against 1 (one failure and no
  stale refusal that run), both `a_held_arrival_across_a_second_*` tests 0 against 1 in S2 (S1
  asserts 0).
  Finding 4 (fixes5 item 3), the herd replay near the unpaged footprint: main loads every herd page
  at each start, with or without a replay, in TEST mode only (the GameTests and rtsim-jni's tests),
  never in production. The load is rtsim-jni `engine/herds.rs:187`, `saved.map(|_|
  engine.state_hash())`, TEST mode's read-back check against the hash its save wrote: `state_hash`
  (rtsim-ecology `herds/engine.rs:2667`) encodes `Herds::save()` (`herds/persist.rs:710-722`), whose
  `PagedRows::for_each` (rtsim-kernel `rows.rs:799-822`) loads every herd and forage page. Every
  TEST-mode save does the same (rtsim-jni `engine/paging.rs:158-162`). Both are on origin/main
  (4a3392f9d and 9d521ea6; main's `paging.rs:161`), and the dragons, the conflict and the ships
  check the same way at a start (`dragons.rs:193`, `conflict.rs:328`, `ships.rs:199`). A production
  start reads the meta page only (`Herds::open_paged`, `herds/persist.rs:764-786`) and
  `session_start` touches the active set (`herds/engine.rs:2110-2128`). Measured in RP-49's 2-step
  run: 163,314 B resident right after the start in TEST mode, 0 outside it; still resident when the
  replay's one step returned (177,178 B, the figure RP-49 first gave as the replay's; why that step
  did not evict them is not traced), gone a step later (95,540 B), and back after each TEST-mode
  pressure save (175,858 B and 179,438 B, against 46,278 B and 49,466 B outside TEST mode). What the
  replay itself causes: RP-42's gate (rtsim-jni `engine.rs:1431`, `!st.replaying`, 63b0bd902, not on
  main, which has no gate at `engine.rs:1307`) takes no pressure save while replaying, so every page
  the window dirtied stays to the replay end: 95,540 B after 2 steps, 191,730 B after 80, the same
  in and outside TEST mode. The RP-49 herds test now runs its replayed engine outside TEST mode and
  asserts that the 2-step replay holds less than the unpaged footprint and the 80-step one more than
  the 2-step (it asserted the 2-step replay held more than half of it).
- **RP-56** (a fight's record, told from a sighting's; §6.1; RP-48; RP-23; fixes4 item 2): the
  command flag `BAND_FIGHT` (8) marks a BAND_MATERIALIZE of a fight's wave (`spawn_wave` with an
  arc: `start_fight`'s and a siege round's waves; a sighting's passes none), and Java echoes the
  command's flags into BAND_MATERIALIZED's bytes 44 to 47 (padding Java never wrote, zero-filled by
  `Journal.begin`, so no layout change: an old record reads as a sighting). Built as option 1,
  decided by the lead: on an engine from its pages, a newer epoch's sighting inside a held party's
  slack only embodies it (§5.2) and keeps the hold; a fight record ends the hold in `fight_held` as
  before; a fight record that finds no hold (at `since` + the slack exactly, as past it) raises the
  epoch floor only, so its casualties and loot count against nothing (the `Looted` arm skips that
  epoch's loot while replaying: it counts every party's loot whatever the epoch), and the slack's
  poll books the round as the abstract raid: lost, never doubled. `ConflictStats::fights_unheld`
  counts it once per epoch, and rtsim-jni warns of each. Case A's cost, named by the lead (the books
  matter more than its live parity): for an arrival, the slack's poll is `arrive()`'s unwatched
  branch, which recalls the band the sighting embodied, where the session that went on kept it
  embodied; a siege's round recalls nothing. Checked against Rule U before building: BAND_RECALL
  drops while replaying (§4.4's table), so no body vanishes in a player's sight; the crashed
  session's bodies are gone; the casualties of that epoch still count through the recalled stack;
  and the replay end makes every band abstract, so the difference is the engine's `embodied` until
  the replay end (`a_siege_held_then` asserts that no recall leaves). Tests, mowzie_records:
  `a_sighting_inside_a_held_decisions_slack_is_no_fight` (a siege's round and its arrival, the
  sighting 300 ticks after the decision),
  `a_held_arrivals_fight_recorded_after_the_slack_is_no_double` (the fight's records shifted 1,200
  ticks past the arrival), `a_held_arrivals_fight_recorded_exactly_at_the_slack_finds_no_hold` (one
  tick inside is honoured as live; at the slack, no hold) and case B,
  `a_sighting_journaled_with_a_held_arrivals_fight_is_no_double` (a sighting at T-1 and the watched
  arrival at T journaled in one step: casualties and loot counted once). Failing first: the sighting
  in a round's slack replayed (Fighting, true, 1, until 85270, watched 1, raids []) against the
  session that went on (Waiting, true, 1, 103270, 0, [79270]); the fight at and past the slack
  (raids, casualties, watched, loot, unheld) ([55870], 3, 0, 5000, 0) against ([55870], 0, 0, 0, 1),
  live ([], 3, 1, 5000). Case B and the control one tick inside passed before the fix (they guard
  against reading the flag the other way). The sims are unchanged but for the new stat. Not done: a
  refused sighting inside the slack (RP-54's residual, accepted).
- **RP-57** (every record that names an epoch raises the floor; §5.3; RP-7; RP-30; RP-37; fixes5
  item 1): while replaying, `Conflict::ingest` folds the record's party epoch (`Obs::party_epoch`)
  into the agent's floor first, before the op-id dedupe, the later kinds' return and any check that
  drops the record; `ingest_copy` folds every copy's, live or replayed, before its match; rtsim-jni
  folds the generation-0 casualty copy it refuses (RP-21). `replay_done` then lifts each party's
  epoch past its floor, as before. Before, only a BAND_MATERIALIZED (fresh engine, or a fight that
  found no hold) and a copied BAND_CASUALTIES or BAND_MATERIALIZED raised it, so a replayed refusal
  of a newer epoch for a party not held left the next decision to take that epoch again. Failing
  first, mowzie_records'
  `a_replayed_refusal_for_a_party_not_held_raises_its_epoch_floor` (a raid band Out at epoch 0, on
  an engine from its pages, a replayed BAND_SPAWN_REJECTED of epoch 1, then the replay end): the
  next BAND_MATERIALIZE took epoch 1 against 2. The sweep,
  `every_record_naming_an_epoch_raises_its_floor`, applies each kind below at epoch 4 to a band at
  epoch 0 on three paths (an engine from its pages, a fresh engine, a copy) and reads the epoch
  after the replay end: 29 of 36 cases stayed at 0 before (every kind but BAND_MATERIALIZED on the
  pages and fresh paths; every kind but BAND_MATERIALIZED and BAND_CASUALTIES as copies).
  `a_copied_casualty_counts_against_an_abstract_band_once`'s control changed with it: it asserted
  epoch 0 after a replayed straggler casualty, which encoded the bug; it now asserts the record's
  3 on both paths. The sims are unchanged. Every kind that carries an epoch:

| Kind | Epoch | Reaches the floor |
| --- | --- | --- |
| 0x0090 BAND_STATE | the party's | Latest-wins, never journaled, so never replayed or copied; rtsim-jni counts it and the engine reads none of it (rtsim-jni `conflict.rs` `observe_state`). `party_epoch` names it, so a slice that reads it folds it through `ingest` |
| 0x0803 BAND_MATERIALIZED | the party's | `ingest`'s fold (a sighting, a held fight, a fight that finds no hold, a stale epoch, a fresh engine); a copy, `ingest_copy`'s |
| 0x0804 BAND_SPAWN_REJECTED | the party's | `ingest`'s fold, held or not, any wave; a copy, `ingest_copy`'s |
| 0x0805 BAND_DEMATERIALIZED | the party's | `ingest`'s fold, a stale epoch too; a copy, `ingest_copy`'s |
| 0x0806 BAND_CASUALTIES | the party's | `ingest`'s fold; a copy, `ingest_copy`'s; a generation-0 copy refused in rtsim-jni, `fold_epoch` there. Since the M6 merge it carries an op (MZB-589), skipped by the party's list inside `casualties`, after both folds: a casualty the party counted already still raises the floor (the sweep's first case) |
| 0x0807 CHIEF_KILLED | the party's | `ingest`'s fold, before the op-id dedupe; a copy, `ingest_copy`'s. Since the M6 merge it goes through M6's `chief_killed_by` with the record's epoch, after the fold |
| 0x0808 ENVOY_ANSWER | the envoy's | `ingest`'s fold; a copy, `ingest_copy`'s |
| 0x0809 TRIBUTE_PAID | the envoy's (agent 0: at the camp, no party) | `ingest`'s fold, before the op-id dedupe; agent 0 names no party, so nothing to fold |
| 0x080B BAND_LOOTED | the party's | `ingest`'s fold, before the op-id dedupe and the unheld fight's skip; a copy, `ingest_copy`'s |
| 0x080D TRAIN_PASSED | the party's | A later kind: `ingest`'s fold comes before the `later` return; a copy goes to `ingest_copy` from rtsim-jni's later-kind branch (changed here: it called `ingest`) |
| 0x080F CAMP_CENSUS, 0x0812 CAMP_BODIES, 0x0813 CAMP_OUT | a camp's (u64) | Java's op id for the census, never an epoch the engine allocates, so it cannot name one the engine would issue again; `party_epoch` is None. M6's census body bit BODY_CRYSTAL names none either |
| Every other conflict kind (FACTION_FOUND, TERRITORY_SIGNAL, POLLUTION, GIFT_GIVEN, CHIEF_STATE, HAVEN_CHEST, DIFFICULTY, RAIL_PASS, CAMP_DEATH, TRADED, NOTICE_SHOWN, CHIEF_PAID) | none | Nothing to fold. `party_epoch` matches every `Obs` variant by name, so a new kind does not compile until it says. After the M6 merge: CHIEF_STATE is read and names a faction, held while replaying (RP-100); CAMP_DEATH's DEATH_CHIEF names a slot and generation, no epoch. After the M7 merge: RAIL_PASS is read (`rail_pass`, no longer a later kind) and names a faction, a train and an owner, no epoch; the industry's TRAIN_ARRIVED and TRAIN_UNWATCHED reach the conflict through `rail_call` with a train and a line only, so M7 adds nothing to fold (RP-102) |
| A conflict record that does not decode | unknown | Skipped with a warning before any module code (rtsim-jni `journaled`), as before: its epoch cannot be read |
| Any conflict record while the conflict is switched off (U10) | | Ignored whole, without its fold, as every journaled record is (MZB-30; rtsim-jni `engine/conflict.rs:565`). A residual, not built (the coordinator's ruling, fixes6 item 6; docs/boundary.md, the conflict's switch): a session crashes with the conflict on; the next start has it off and saves past the crashed window (the journal is trimmed); a later start turns it on, and that session can issue the crashed window's epochs again, from pages that never saw them. Not RG5 and RG6's old life (Java's, the whole directory, drawn only when the pages include no observation). The option not taken, no format change: the jni conflict module detects the gap at switch-on (its pages' `now` older than the journal's start) and raises every party's epoch past the window before it ingests |
| Herds (HERD_*) | the herds' own | Built by slice H: RP-81 (every kind that names an epoch folds, `Obs::group_epoch`), RP-84 (the session's handle bound). |
| Dragons (DRAGON_*), ships (SHIP_*) | the module's own | No floor yet in either (until slices D and S land): their `replay_end` is empty (rtsim-jni `dragons.rs:280`, `ships.rs:275`) and a copy is applied as a live record. Their epochs come from rtsim-ecology `engine.rs:3011`, `:4364` and `:4402`, rtsim-ships `engine.rs:2802` and `:3983`. Slices D and S build them (§9.2, §9.5), to this rule: every kind of theirs that names an epoch, not only the *_MATERIALIZED. Reported, not built here (slice C is the conflict's) |

  A live copy's floor stays in memory until the party's next body (`next_epoch`), as RP-30's did,
  and a save before it does not write it. Not a gap: the session that issued that epoch moved the
  party's epoch to it at once, so its own save holds it, or, crashed first, the next start's replay
  of what Java journaled for it does (this rule), short of the switched-off case above. The
  rtsim-jni generation-0 fold has no jni-level test of a newer epoch: the fixture's copies are of
  the pages' own epoch; the engine half is the sweep's Copy path.
- **RP-58** (a fighting band is embodied again by its next commanded wave; fixes6 item 5; live on
  main since 283b1254): when Java refused a watched fight's first wave, the `BAND_SPAWN_REJECTED`
  arm left the band unembodied with nothing placed, and nothing set `embodied` again, as
  `spawn_wave` never sets it. The later waves Java placed were in Minecraft by the books
  (`spawned`) but their `BAND_CASUALTIES` found no band in view (`casualties` asks `embodied` and
  the epoch) and counted nothing, and the fight's end recalled nothing (`go_home` recalls only an
  embodied band). The fix, my call: `wave` sets `embodied` when it commands a wave (the wave
  counter moved). Not at the wave's `BAND_MATERIALIZED`: the live engine reads none
  (`materialized` acts only while replaying), and embodiment is set at command time everywhere
  else (`start_fight`, `seen`), so a record-time rule would need a new live path. Failing first,
  live, mowzie_records' `a_fight_whose_first_wave_was_refused_counts_its_later_waves_deaths` (a
  raid band's fight with only its wave 0 refused, the plugin killing a quarter of each later wave
  at the base): the plugin reported 3 deaths of epoch 2, the engine counted 0 and the band's alive
  did not move; now 3, alive down by 3, and the fight's end recalls epoch 2. One commit, d092dafd,
  code and test only, no dependency on the replay gate: it cherry-picks cleanly onto origin/main
  (428a3311), where the test fails without the fix with the same values and rtsim-conflict and
  rtsim-jni pass with it. The sims are unchanged.
- **RP-59** (a record that moves a held party ends its hold first; §6.1; RP-23; RP-48; RP-56;
  fixes6 item 1): a held decision's abstract raid was lost when its band was sent home inside the
  slack. The hold kept the decision while a sighting at `since` + 300 embodied the band (RP-56);
  then a wipe-out (`casualties`), its chief's death (`chief_killed`) or Peaceful
  (`stop_war_parties`) sent it home through `go_home`, whose `bump` left the hold's poll stale at
  the Ev::Party arm, so the decision never resolved. Those three are every path that moves a held
  war party: the plan's other bumps are `start_segment`, `wait`, `fight_held` and `start_fight`
  (the hold's own ends), `resume_envoys` and the envoy branches of `casualties` and `envoy_home`
  (envoys are never held), and `end_fight` (Fighting, not held). The fix: `Conflict::ingest` calls
  `release_before` after the op-id dedupe and before it applies the record, and `ingest_copy`
  before a copied BAND_CASUALTIES. A BAND_CASUALTIES or CHIEF_KILLED for a held party at its epoch
  or an older one (a copy's at any epoch), or a DIFFICULTY that makes the world Peaceful (every
  held party), ends the hold: `end_hold` runs `party_event` at the record's tick with
  `hold_ending` set, which `arrive` and `wait_over` read to decide unwatched inside the slack too;
  then the record applies. Any count of casualties ends it, not only a wipe-out, so as not to
  repeat `casualties`' reckoning of one; the decision only comes sooner, as of fewer records. A
  newer epoch's record is the held fight's (honoured or refused, §6.1). A BAND_DEMATERIALIZED ends
  nothing: `ConflictBridge.materialize` sends one for the band's older epoch just before the
  fight's BAND_MATERIALIZED, which would end a fight's hold the moment before it is honoured. Its
  residual, added to §6.1's losses: a death of a band embodied before the decision, journaled in
  the tick or two between the fight's command and its BAND_MATERIALIZED, ends the hold unwatched
  and the fight is lost as RP-56 says (one resolution, never two). Failing first,
  mowzie_records' `a_held_decision_whose_band_is_sent_home_inside_the_slack_keeps_its_raid` (the
  checker's scratch case, renamed and widened: a siege's round and its arrival, each wiped out,
  its chief killed and Peaceful, 50 ticks after the sighting): replayed raids [] against the
  session that went on's [79270] (round) and [55270] (arrival) in all six, the band Home with
  `until` 79270 and 24020 against 103270 and 79270 (the next round's tick, booked by the decision's
  `wait` before the band went home). Now as live, and each raid sent 34 members as live did. The
  sims are unchanged.
- **RP-60** (a held decision resolves with the members it had at its tick; §6.1; RP-48; RP-59;
  fixes6 item 2): the slack's poll resolved the held decision with the members left at the poll,
  fewer than the session that went on had at the decision when a record replayed inside the hold
  took some. RP-59's release covers a death: a BAND_CASUALTIES ends the hold before it applies, so
  the decision resolves first, with every member. It does not cover a BAND_DEMATERIALIZED, which
  ends no hold (RP-59 says why) and whose survivors can be fewer than were placed (deaths Java
  never reported). The fix: `Held` keeps the party's members at the hold, and `end_hold` (at the
  slack's poll and at RP-59's release) swaps them in for `party_event`, then takes what the
  decision cost (each unit's members before less after, the raid's dead) off the members left.
  Only the map's holds: a restart from pages saved mid-replay (`held_since`'s inference, a
  defence since RP-42) and a hold the replay end clears decide with the members left, as before.
  Failing first, mowzie_records' `a_held_decision_raids_with_the_members_it_had_at_its_tick` (as
  RP-59's case, the band's bodies despawned 50 ticks after the sighting with 2 of the first placed
  stack's members gone, and, as the control RP-59 already passed, 1 member killed instead): the
  despawn's raid sent 32 members against live's 34, for a siege's round and its arrival; the death
  already sent 34 with RP-59 alone. Now 34 in all four. The sims are unchanged.
- **RP-61** (item 5's replay half: a refused first wave's later waves are honoured; §5.2; RP-54;
  RP-58; fixes6): with RP-58 live, a replay of the same fight still counted none of its later
  waves' deaths. RP-54 ends a held arrival's hold at the refusal of its first wave, the band
  Fighting and not embodied, and `honour` honoured a record of the party's own epoch only when it
  was embodied ("not embodied: the body is gone"), so the later waves' BAND_MATERIALIZED placed
  nothing and their BAND_CASUALTIES found no band in view. The fix: `honour` takes a record of the
  same epoch, at or above the party's `wave`, for a fighting band that is not embodied as the
  fight's later wave: it embodies the band again and fills it, as live `wave` does at the command
  (RP-58). Kept apart from d092dafd, which goes to main alone and has no replay gate to change.
  A known edge, rarer than the case fixed: Java answering a wave commanded just before a despawn
  of the same epoch (chunks unloaded) after the despawn, which live leaves unembodied, is
  embodied by the replay; its bodies are in Minecraft, so the replay's books are the truer.
  Failing first, mowzie_records'
  `a_held_arrival_whose_first_wave_alone_is_refused_counts_its_later_waves` (the checker's scratch
  case, renamed: a watched raid's arrival whose wave 0 alone is refused, the plugin killing a
  quarter of each later wave at the base, replayed from the pages saved 3,000 ticks before):
  replayed (phase, epoch, watched, deaths, alive, raids) = (Home, 2, 1, 0, 12, []) against the
  session that went on's (Home, 2, 1, 6, 6, []). Now as live. The sims are unchanged.
- **RP-62** (the gap stat at a refused first wave; §6.1; RP-51; RP-54; fixes6 item 3): RP-51's
  `fight_record_gap_max` took a fight's entry in `fights_unanswered` at its first
  BAND_MATERIALIZED only, so when Java refused the first wave (RP-51's "a rejected spawn" entry
  that waits) it measured the next wave's record, a wave interval later, past the slack: a gap the
  hold never waits for, since a replay's hold reads the refusal as the fight (RP-54). The fix:
  `fight_answered` takes the entry and measures it at either record, a BAND_MATERIALIZED of that
  epoch or a BAND_SPAWN_REJECTED of its wave 0; RP-51's text on the rejected spawn holds now only
  for a crash. Failing first, mowzie_records' `a_refused_first_wave_is_its_fights_first_record`
  (the checker's case live: a watched raid's arrival whose wave 0 alone is refused): 1201 against
  a slack of 600; now 1, the plugin answering a tick after the command. The sims are unchanged.
- **RP-63** (a late refused first wave marks its epoch unheld; §6.1; RP-54; RP-56; fixes6 item
  4): a replayed BAND_SPAWN_REJECTED of a newer epoch's wave 0 that found no hold (at or past the
  slack, which resolved the arrival unwatched) did nothing, so it was not counted in
  `fights_unheld`, and nothing kept that epoch's later records from counting. The fix: that arm
  marks the epoch in `unheld_fights` and counts it once, as `honour` does for a fight's
  BAND_MATERIALIZED that finds no hold (RP-56). The checker's case (a later accepted wave of that
  epoch loots) did not double before the fix: every wave of a fight carries BAND_FIGHT, so its
  BAND_MATERIALIZED marked the epoch at `honour`. The refusal is the fight's only record when Java
  refused every wave: then the stat missed it. The record carries no flags, so a refused
  sighting's wave 0 of a newer epoch counts too; a sighting always takes a new epoch, so that one
  has no bodies, and only the stat moves (the option not taken: a set of the parties whose hold
  the slack resolved, to tell them apart). Failing first, mowzie_records'
  `a_held_arrivals_fight_refused_whole_and_recorded_after_the_slack_counts_as_unheld` (every wave
  refused, the refusal at the slack + 600): `unheld` 0 against 1; the case with the later waves
  placed, `a_held_arrivals_refused_first_wave_recorded_after_the_slack_is_no_double`, passed
  before and after (one abstract raid, no loot, counted once). The sims are unchanged.
- **RP-64** (the M5 merge; replay-gate §2.7's M5 paragraph, §4.4; RP-2; RP-10; RP-42; MZB-177):
  main's `Conflict::replaying` field (MZB-141's flag) is deleted and its readers use the outbox's
  `replaying()`. M5's `session_start` goes with it: its camp loop runs in `begin_replay` (git's
  merge), its command filter is the gate's queued column, its MZB-138 fold stays retired. M5's
  `send_lines` pushes each CAMP_RECONCILE through the gate instead of extending the queue around
  it; "never" holds by construction, every camp being out from `begin_replay` to `replay_done`.
  Choices: synth's `CampRun::restart_to_replay` opens the window with `begin_replay(true)` (the
  engine holds what its pages would), and `CampRun::round_trip` asserts the window is closed
  rather than carrying the flag across a load: the window's state (floors, holds, the outbox's
  flag) is in memory, and the host takes no page while replaying (RP-42); every caller round-trips
  outside it. tests/camp.rs's `a_replayed_census_only_marks_its_camp_out` opens the window with
  `begin_replay` and takes its snapshot after it (the window's start already makes the camp out and
  voids its lines), so it still tests the census alone. No M5 `Cmd` or `Obs` kind is new, so no
  gate or `party_epoch` arm is added: camp epochs are Java's. `obs_seq` and `durable_obs_seq` stay
  as main sets them.
- **RP-65** (the M5 merge check's P2; RP-64): tests/camp.rs's
  `a_replayed_census_only_marks_its_camp_out` could not catch a census arm that skipped `see`,
  since `begin_replay` puts the camp out before the census comes. It now feeds an answer of the
  replayed epoch twice (a holder's copy) and asserts it is booked once. With `see(epoch)` dropped
  from the replaying arm it fails: `camp_answers` 9 against 10. The arm's `epoch = 0` is
  equivalent after the merge (dropped, the 64 camp tests pass): `camp_session_start` has made
  every camp out at the window's start, and only a live census brings one in. It stays as a
  defence. The option not taken: a window opened without `camp_session_start`, which no host
  opens.
- **RP-66** (a test's conflict command waits for the replay end; §4.4; §3.6; RP-33; the M5 merge
  check's item 2): rtsim-jni's `order_command` puts a TEST order's command straight into the
  host's queue, around the gate, so a test could send a BAND_MATERIALIZE (a "never" kind) while
  the replay runs. The conservative option, as the lead asked: such a unit is held with RP-33's
  (`waits_for_the_replay_end`: the four that run the clock, and `Order::ConflictCommand`). It
  runs no clock, but holding it lets it take the units after it too, so order is kept. The option
  not taken: list it in §4.4 as a route with its own verdict per kind. Failing first, protocol's
  `a_conflict_command_ordered_while_replaying_waits_for_the_replay_end` (eight hand-overs of
  fillers): "not written while the replay runs". `Order::HerdCommand` is the same kind of route
  for the herds; it is not this slice's.
- **RP-67** (`obs_seq` before the `later` path; the M5 merge check's item 3): rtsim-jni's
  `journaled` set the engine's `obs_seq` only on its last path, so a `later` record (and the
  refused copy of a casualty) was ingested with the previous record's. It is now set right after
  the record decodes, on every path. No test can fail on it today: the only reader is
  `camp_bodies` (an answer is kept with it, MZB-177), and CAMP_BODIES is neither `later` nor a
  casualty. The records the host makes itself (`reject`'s BAND_SPAWN_REJECTED) carry no
  sequence and keep the last one set; they read none.
- **RP-68** (a world save while replaying forgets no answer; §2.7; MZB-177; the M5 merge check's
  item 4): a WORLD_SAVED between a long replay's hand-overs raised `durable_obs_seq` at once, so
  the camp's next booking forgot the answers of replayed records, and a holder's copy of one,
  journaled again later in the replay, was booked twice. `Conflict::world_saved` now keeps the
  highest sequence reported while the outbox replays in `durable_held` (not state) and raises
  `durable_obs_seq` with it at `replay_done`; live, it raises it at once. rtsim-jni's WORLD_SAVED
  arm calls it. `Replay`'s own op-id forgetting on the same line is unchanged (not this
  check's). Failing first, tests/camp.rs's
  `a_world_save_while_replaying_forgets_no_answer_until_the_replay_end` with the save applied at
  once: `camp_answers` 12 against 11.
- **RP-69** (the M5 merge check's missing tests; §2.7 item 7; item 5): all five are written, and
  every one passed when written (the paths were right; the tests keep them so).
  - rtsim-jni: the replayed census test reasserts each answer live for the replayed epoch, with
    no second booking, and asserts the gate's "never" count stays 0.
  - rtsim-jni: `a_replay_begun_by_a_replayed_faction_only_marks_its_census_out` (`grove_with`,
    the pages saved before FACTION_FOUND).
  - tests/camp.rs: `a_replayed_homecoming_at_an_out_camp_sends_nothing` and
    `a_replayed_death_of_an_embodied_partys_member_costs_its_unit_and_sends_nothing`. Each marks
    the party embodied by hand, as the homecoming test does, since no flow of this slice leaves
    one embodied across a restart.
- **RP-100** (a CHIEF_STATE while replaying waits for the replay end; the M6 merge; replay-gate
  §2.7's M6 paragraph, §5.6; RP-68; MZB-268, MZB-269): M6's CHIEF_STATE is latest-wins and never
  journaled, and rtsim-jni's `observe_state` gives it to the engine at once, also while the outbox
  replays. So it is newer than every record the replay brings back, and one of those undid it: a
  replayed FACTION_FOUND's census byte set the crystal as the den was found, and a CHIEF_STATE for
  a faction that only the replay founds was dropped as unknown. `Conflict::chief_state` now keeps
  one that comes while replaying in `chief_states_held` (the latest for each faction key, not
  state) and `replay_done` applies them after the floors, as RP-68 does with a world save. Live,
  it applies at once, as before. Failing first, frostmaw's
  `a_chief_state_while_replaying_waits_for_the_replay_end` (three live CHIEF_STATEs, the last
  with the crystal off, then the den's FACTION_FOUND with chief and crystal, then the replay end):
  the crystal read true. The sims are unchanged by it (they replay no CHIEF_STATE).
  The residual, not built: this session's own census of the den, taken before the steal, is
  journaled and so dispatched after the replay end, behind a CHIEF_STATE written after it. Its
  crystal mend then puts the crystal back until the den's next census. M6's own window had the
  same order. The option not taken: carry the record's game time into the engine and keep the
  crystal's time in state, so that a mend older than the last CHIEF_STATE is skipped. That is a
  format change, which this merge does not make.
- **RP-101** (M6's `chief_unit_lost` and RP-27; the M6 merge; a residual, not built): the boss's
  unit lost with its chief takes from a stack by M6's own rule (`!embodied` and a recalled stack
  of the epoch), not by `casualties`' (RP-27: `spawned` only when embodied at the record's epoch,
  else that epoch's recalled stack), and it takes from a stack for a copy of CHIEF_KILLED too, which
  RP-9 and RP-25 forbid. Nothing can show it today: a boss band is its one unit (the Frostmaw's
  raid and invasion are one `frostmaw`), so the loss ends the band and its stacks with it. A boss
  culture with a band of more than one unit must pick the stack as `casualties` does.
- **RP-102** (M7's decisions while replaying are main's, gated; a band at a track is never held; the
  M7 merge; replay-gate §2.7's M7 paragraph, §4.4, §6.1; MZB-308, MZB-312, MZB-315, MZB-375). M7
  adds no `Cmd` kind and no push site. Its routes reach main's pushes through `plan_feud`: a
  RAIL_PASS (`rail_pass`), a train's call through industry's TRAIN_ARRIVED and TRAIN_UNWATCHED
  (`rail_call`), a held pass let go after a day (`let_go_due`) and a meter's wake (`Ev::Rail`) count
  legs and run the push gate, whose push (`set_emission_for`) recomputes the feud's rate and replans
  it. While replaying all of it is re-derived from the same records as live, so nothing is dropped
  or held of the rail's own: the meter, its source and the feud's climb are state; the climb's
  FACTION_NOTICE drops at the gate and goes again from the feud at the replay end (MZB-31); a war
  party sets out abstract and a sighting of it waits (§6). A RAIL_PASS has no op id and no copy
  (Java journals each once, at once, from `ConflictBridge.railPass`), so the pages and a contiguous
  replay apply each once. A raid or invasion bound for a track asks no watcher when it arrives:
  `arrive`'s track arm comes before the arm that holds (§6.1) and only waits out the band's patience
  at the spot, and `resolve` raids nothing there. So it is never held and decides while replaying as
  live; the residual §6.1 listed for a track Java has removed since does not arise, since Java is
  never asked about a track (BAND_MATERIALIZE and BAND_STEER carry the spot as a plain position).
  M7's own replay-end trigger in `rail_call` (MZB-375) goes with the other MZB-141 triggers (RP-2):
  the end at the top of engine.rs `journaled()` runs before industry's entry, so before the call.
  Tests, written at the merge to pin what is built (both pass on the merged head; the second fails
  when the track arm lets a replaying arrival fall through to the hold, checked and undone):
  tests/rail.rs `a_replayed_rail_climb_decides_as_live_and_its_notice_waits_for_the_replay_end` (60
  passes into an engine whose replay is open and one live, a day run: the same meter, pushes, step
  and kept notice; no FACTION_NOTICE replaying, the gate's "never" 0, the notice sent once by the
  replay end) and `a_track_raids_arrival_while_replaying_is_not_held_and_waits_as_live` (the replay
  opened while the track raid is out: it arrives Waiting at the spot until its patience, as live,
  with nothing deferred, raided or looked for). M7's two MZB-375 protocol tests pass without the
  call.
- **RP-103** (an ownerless pass's owner while replaying; the M7 merge; a residual, not built): an
  ownerless pass counts, when its line is named or a day on, against the player with the most time
  near it (`presence.top_of`, MZB-309, MZB-310). Presence is credited only from player positions,
  which are latest-wins and never journaled, and none is routed while replaying (§6): the replay
  reads the pages' stays, decayed, without the time the last session's players spent there after
  the save. So a replayed call or let-go can count the pass against an older stayer, or against
  nobody, where live counted it against someone else. One count at most either way: lose, never
  double. P14's `owner_of` (a source with no direct actor) reads presence while replaying the same
  way, and slice C left it, so the rail follows it. Not held to the replay end: the positions after
  it are the new session's, not the window's, so a hold would not recover the live answer. The
  option not taken: Java names the owner it sees at the call (a format change to the industry's
  TRAIN_ARRIVED, or a conflict record beside it).
- **RP-104** (a copied RAIL_PASS; the M7 check, P3): `ingest_copy` sent every kind it did not
  name to `ingest`, so a RAIL_PASS copy would run `rail_pass` again and count its leg twice. Java
  never sends one (no op id, no holder reasserts it, §4.4), so nothing counted twice; the conflict no
  longer trusts that: `ingest_copy` drops a RAIL_PASS. Test: tests/rail.rs
  `a_copy_of_a_rail_pass_counts_nothing` (the copy counts and holds nothing; the control, the same
  pass live, is counted and held; without the arm the copy counts 1).

### Slice H (herds)

Branch fix/replay-herds, from f2cf3d69 (slice C merged with main 095be6e8). The design is
docs/replay-gate.md; this slice is §9.4 with §4.6, §5.2, §5.3, §5.6 and §5.7's herds lines. The code
is rtsim-ecology `herds/{outbox,engine,boundary,herd,persist,mod}.rs` and rtsim-jni
`engine/herds.rs`, `engine/shared.rs` and `engine.rs`. RP ids 80 to 89; each was checked free with
`git log --all --oneline -i -S'RP-8x' -- docs crates` before use. No page, journal or wire format
changes (RP-89).

- **RP-80** (the outbox and the gate; §4.6; RP-3, RP-4): `herds/outbox.rs` is slice C's pattern for
  the herds' `Cmd`. `Herds::outbox` is an `Outbox` over a private `Vec<Cmd>`; the only ways in are
  `push` (gated), `restore` (pages, before any session) and `begin_replay`'s removal; `gate(&Cmd)` is
  an exhaustive `match` with no `_` arm. The engine has one push site per kind (`seen`, `steer`,
  `despawn`), all through `push`. As built:

  | Kind | Queued at the session start | While replaying | Why |
  | --- | --- | --- | --- |
  | 0x8700 HERD_MATERIALIZE | drop | never | embodies; `seen` returns before it (RP-83) |
  | 0x8701 HERD_STEER | drop | drop | a body's order; the group is the last session's |
  | 0x8702 HERD_DESPAWN | drop | drop | the same |

  The queued column is the old `session_start` filter exactly: it took the whole queue. `session_start(legs,
  at)` is gone; `begin_replay(from_pages)` sets the flag and applies the queued verdicts, and
  `replay_done(legs, at)` (RP-86) ends it. `Replaying::Send` exists for a future query kind and is
  unused. A "never" kind reaching the gate is counted, its kind and name kept (`GateCounts`), not
  queued, and a `debug_assert!`; no release panic, since the replay end makes every group abstract.
  *The books, per dropped kind.* A Steer decided while replaying names a group the new session's
  Java does not have: the decision still moves the group's `steer` and counts in `stats.steers`
  (the engine's view of what it ordered), and nothing else follows a steer (no pool, slot, puppet
  or owed amount), so dropping it leaves the books as a live run's. A Despawn marks the group
  `leaving`, counts in `stats.despawns`, and frees nothing by itself: a group ends by its
  HERD_DEMATERIALIZED (a replayed one is applied as recorded, the capture landing on the members)
  or, with none, by the replay end, which makes the group abstract with its members seen at the
  replay clock. Dropping it therefore leaves no member, puppet or check booked that a live run
  would have settled otherwise. The Materialize is never decided: nothing to drop. Unit tests:
  `every_command_kind_has_its_replay_verdicts` (one value of each kind against the table, the
  queued column, the counts, the deferral), `a_never_kind_decided_while_replaying_is_counted_and_not_queued`
  (panics in a debug build by `should_panic`; counts and queues nothing in release, the
  assertion checked under `cargo test --release`). The engine test
  `a_steer_decided_while_replaying_is_dropped_and_counted` has its control (the same obstacle with
  no replay queues the Steer). Option not taken: the conflict crate's `Outbox` shared with the
  herds; its kinds and verdicts are the conflict's, and the crates do not depend on each other.
- **RP-81** (every kind that names an epoch raises the floor; §5.3; RP-7, RP-30, RP-37, RP-57; this
  supersedes the herds row of slice C's note that herds, dragons and ships had "no floor yet"):
  `Obs::group_epoch()` is an exhaustive `match` over all eight kinds (agent 0, a record of no
  herd, gives none), so a ninth kind does not compile until it says. `Herds::floors` maps an
  agent to the highest epoch a replayed record or a copy named that its herd's own `epoch` does not
  hold (`raise_floor`). `ingest` folds first while replaying, before any check that drops the
  record; `ingest_copy` always; `fold_epoch` is public for the record rtsim-jni sees before the
  engine can: rtsim-jni `engine/herds.rs` `journaled` decodes the record and folds every one that
  decodes before its `enabled` check, so a session with the herds switched off (U10) still raises
  the floor, and a switch-on later in the session or after a crash never decides an epoch a record
  used. It folds live records too, and when the herds are on with no herd: such an engine does
  not run (`running` needs a herd), so its replay begins at its first founding and its own fold
  in `ingest` (only while its replay runs) never saw the journal's records. A live record names
  an epoch its herd holds, which raises nothing. A record that does not decode (bad framing, or a kind the boundary cannot read) names no
  epoch the engine can read: it raises no floor, switched on or off, and is skipped with a
  warning as before. A herd's next group takes `x.epoch.max(floor).wrapping_add(1).max(1)` in
  `seen`, with `floors.remove` there: the floor sits under the wrap rule, so a floor above the
  herd's epoch survives it and a wrap to 0 still lands on 1. At `replay_done` each floor folds into
  its herd's `epoch` (every group is abstract then); the floor of an agent with no herd yet is kept
  for that agent's first group (the conservative option; the map is in memory only). Each kind:

  | Kind | Epoch | Reaches the floor |
  | --- | --- | --- |
  | 0x0080 HERD_GROUP_STATE | the group's | `ingest` and `ingest_copy` fold it; the engine reads none of it (latest-wins, never journaled), so only a slice that reads it matters |
  | 0x0700 HERD_MATERIALIZED | the group's | folded first; then honoured (RP-82) or, as a copy or on a fresh engine, nothing more |
  | 0x0701 HERD_SPAWN_REJECTED | the group's | folded first, before any check that drops it |
  | 0x0702 HERD_DEMATERIALIZED | the group's | folded first, before any check that drops it |
  | 0x0703 HERD_KILLED | the group's | folded first; the kill then goes by member key and op id |
  | 0x0704 HERD_SHEARED | the group's | the same |
  | 0x0705 HERD_TAKEN | the group's | the same |
  | 0x0706 HERD_OBSTACLE | the group's | folded first, before any check that drops it |

  `an_epoch_replayed_with_no_herd_yet_is_not_decided_again_for_the_first_herd` (rtsim-jni: the old
  session's herd never saved, a group of epoch 1 journaled, the next session on with no herd; the
  herd founded after the replay end, on the same handle, decides above 1; control
  `an_epoch_replayed_with_the_herd_in_the_pages_is_not_decided_again`): with the fold only in the
  switched-off branch it fails, "epoch 1 after 1".
  `a_record_replayed_while_the_herds_are_off_still_raises_their_epoch_floor` (rtsim-jni: the herds
  off for a session that replays a journaled HERD_MATERIALIZED of epoch 9, a crash, the herds on
  again, the next group's epoch above 9; control
  `a_record_replayed_while_the_herds_are_on_raises_their_epoch_floor`): with the fold taken out of
  `journaled` it fails, "epoch 1 after a replayed 9", and the control passes.
  `every_record_naming_an_epoch_raises_its_floor` applies each kind at epoch 4 on three paths (an
  engine from its pages, a fresh engine, a copy) and reads the next group's epoch: 5 on every
  path. It does not compile on the base (the floor, `begin_replay` and `Obs::group_epoch` are new);
  the rtsim-jni pair below shows the same on the base at the wire.
  `the_floor_is_above_the_herds_own_epoch_and_below_the_wrap` (a record of the herd's own epoch
  raises nothing; a record of the highest epoch gives the highest, not a wrapped one) and
  `a_floor_of_an_agent_with_no_herd_stays_past_the_replay_end` have their controls inside.
- **RP-82** (a replayed HERD_MATERIALIZED is honoured; §5.2; RP-6, RP-22, RP-31): on an engine
  that came back from its pages (`begin_replay(true)`), while replaying, a record whose epoch is
  above the group's embodies the herd at that epoch with `min(spawned, alive)` members, picked by
  the engine's own `members::reconcile` and `pick` (the herd as it is now: seen members first, then
  fresh); no `Ev::Check` is booked, the chart is the identity, and the player slot is
  `NO_PLAYER` (`u32::MAX`), which no `players[]` entry carries, so no puppet count rises. It sends
  nothing, and the group lives to the replay end (`active` is updated so `replay_done` finds it).
  An older embodied group of the pages whose end was never journaled is released first (members
  stay seen). An epoch at or below the group's changes nothing; a newer one replaces. The record's
  epoch must also be above the herd's own `epoch` (§5.2: "above the agent's"): the pages' state
  need not be a prefix of the journal, and a herd abstract at epoch 7 has used 5, so a replayed
  record of 5 is history, not a group to honour (the group-epoch check alone would honour it when
  no group is embodied). A live record,
  a fresh engine's (RP-31) and a copy's (RP-8) are not honoured. Choice: the member set is the
  engine's own pick rather than a record's list (the record carries none); the same pick the
  decision made, because the herd the pages and the replay leave is the one the decision saw. The
  members a later replayed capture, kill or shearing names are found by key in `seen`, as live.
  Option not taken: honouring with the record's `spawned` alone and every member alive; it would
  count a killed sheep into the group the player sees. A group honoured has no chart (its
  player is not known), so on a topology that folds (`!topo.is_flat()`) a later obstacle of it
  counts as late instead of being put back on the route's cell; the replay end makes it abstract
  anyway.
  `NO_PLAYER` is transient: no page is taken while replaying (RP-42), so it is never saved.
  Test: `a_replayed_materialization_is_honoured_only_on_an_engine_from_its_pages` (the four paths;
  the capture lands on the honoured members and is late elsewhere) and
  `a_replayed_materialization_of_an_older_epoch_changes_nothing_and_a_newer_one_replaces`;
  `a_replayed_materialization_at_or_below_the_herds_own_epoch_is_not_honoured` (a herd raised to
  epoch 7, then records of 5 and 7 are not honoured and one of 8 is, with no group embodied): with
  the herd-epoch check taken out it fails, "a record of 5" honoured.
- **RP-83** (nothing embodies while replaying; §6; RP-3): `seen` returns first thing while the
  outbox replays (`Outbox::defer`, counted in `deferred`). The `Ev::Seen` events the pages' wheel
  holds are a sighting of the last session's world, the player is not known again until the
  replay end, and a group decided here would be a "never" Materialize. The herd stays abstract and
  consistent; the sighting the replay end routes books its own meeting and the decision follows.
  `a_meeting_from_the_pages_embodies_nothing_while_replaying` (control: with no replay the same
  meeting embodies a group). The other events (`Decide`, `Check`, `Year`) do not embody.
- **RP-84** (copies; §5.7; RP-8, RP-9): `Herds::ingest_copy` replaces `ingest` for a copy. It folds
  the floor first, then applies HERD_KILLED, HERD_SHEARED and HERD_TAKEN (they go by member key
  and op id and need no body); every other kind (HERD_GROUP_STATE, HERD_MATERIALIZED,
  HERD_SPAWN_REJECTED, HERD_DEMATERIALIZED, HERD_OBSTACLE) counts in `stats.late_records`, since
  each matches a body and a copy's body is gone. On the base the herds had no copy path: rtsim-jni
  applied a copy as the record itself. This slice added the host half too: rtsim-jni
  `engine/herds.rs` `ingest` reads `st.applying_copy` and calls `ingest_copy` for a copy (as
  conflict's does), without which RP-82's honour would take a copy for a record. First written
  without that half, the engine's `ingest_copy` was unreachable from the host; the
  rtsim-jni pair below found it. Failing with the routing undone:
  `a_replayed_copy_of_a_materialization_is_not_honoured_but_its_epoch_is_counted` (the old
  session's HERD_MATERIALIZED replaced by a REASSERT of it from an earlier generation, in the
  journal) gives (groups honoured, copies applied) = (1, 1) against (0, 1); it does not compile on
  the base (no honour). Control `a_replayed_materialization_the_journal_holds_is_honoured`: the
  record itself, (1, 0). Both then see a group epoch above the old one after the replay end.
  *Accepted residual.* A copied HERD_KILLED or HERD_TAKEN goes by agent and member key with no
  epoch check (the key names the herd's own member, not a group's), so a copy for epoch 4 can
  remove a member of a live epoch-5 group that picked it before the copy was heard: the herd
  loses the member for good, which is right (the sheep is dead), but the group's count is one
  fewer than its bodies until the group's next despawn. This is conflict's accepted §10 item 3
  (a copied band death) in the herds, and the same recommendation: accept. Option not taken: an
  epoch check on the copy, which would keep a dead sheep alive in the herd.
  *Handles allocated in the replayed window (the slice D review's P1: a dragons arena hands out
  indices in order and never reuses them, so a replay that allocates in another order puts
  another dragon under a record's index).* Can a herd handle differ here? Not in the window: a
  herd is allocated only by `add_herd`, whose callers are the world model's founding
  (`worldmodel::simulate` returns while `st.replaying`) and a test's `Order::HerdFound` (in
  `runs_the_clock`, held to the replay end, RP-33), besides the restore from the pages. A group
  is no handle (it is the herd's handle and an epoch; no split makes a herd), and a member is
  named by key within its herd, so no arena order is involved. After the replay end it can: the
  pages lack the herds the last session founded after its last save, and the world model founds
  in the order the world is explored, so handle k after a crash can be another herd than handle
  k before it. Two things reach it. The epoch floor, keyed by agent and kept for an agent with
  no herd, only ever raises an epoch (the new herd takes a higher one, never an equal one;
  `a_floor_of_an_agent_with_no_herd_stays_past_the_replay_end`). A copy that arrives after the
  founding is guarded: `Herds::replay_bound` is the herd arena's next index at the session's
  start, `honour` and the copy's KILLED, SHEARED and TAKEN act only on a herd below it, and any
  other copy counts in `late_records` (its floor still counts). *The bound is the session's,
  not the replay's.* The first cut took it at `begin_replay`, and the re-check found that wrong
  twice. The herds' replay does not begin at the session start: it begins when they first run
  (`reset_if_due` needs a herd, `running` being switched on with a herd), so with no herd at the
  start it begins at the first founding, after that herd's `add_herd`, and a `reset_if_due`
  placed before `add_herd` never ran (the first cut had one, now removed); and a switch-on in
  mid-session begins it again, reading the bound with the herds founded since. Now rtsim-jni's
  herds `session_start` calls `Herds::begin_session` whether or not the herds run, which sets
  the bound, and `begin_replay` only lowers it (`min`). This also closes the earlier residual of
  a test's HerdFound while the herds are off: it allocates after `begin_session`, so it is not
  known. A process that runs a second session takes a new bound at its start, with the herds
  founded in the first among those it can name.
  Tests: rtsim-ecology `a_copy_for_a_herd_founded_after_the_session_began_lands_on_nothing`
  (KILLED and TAKEN; the herd founded after the replay end, before the replay begins, and before
  a switch-on's second `begin_replay`; the flock's copy lands),
  `an_engine_with_no_herd_at_the_session_start_takes_no_copy_for_its_first_herd` (control: a
  herd held at the session start takes it) and `a_herd_founded_in_the_replay_window_is_not_honoured`
  (the flock's record is). rtsim-jni
  `a_copy_for_a_herd_the_pages_lacked_does_not_land_on_the_herd_founded_in_its_place` (a second
  herd founded after the replay, a REASSERT of a KILLED naming its agent; control
  `a_copy_for_a_herd_the_pages_held_lands_on_it_after_a_founding`),
  `a_switch_on_after_a_founding_does_not_make_the_new_herd_one_the_last_session_had` (MODULES off
  and on after the founding; control inside) and
  `a_copy_for_a_herd_the_session_began_without_does_not_land_on_its_first_herd` (the old session
  never saved its herd, so the next starts with none and founds one on handle 0; control
  `a_copy_for_a_herd_the_session_began_with_lands_on_it`). With the bound taken at `begin_replay`
  (commit 9128fc3d) the engine tests fail and so do the wire tests: "it lands on nothing" 1
  against 0 for the no-herd start, and (0, 1) against (0, 0) after the switch-on. They do not
  compile on the base (`begin_replay`, `replay_honoured`).
- **RP-85** (a test's herd command waits for the replay end; §4.6; RP-33, RP-66): `Order::HerdCommand`
  (TEST mode; `herds::order_command`) puts a command of any kind straight into the link's queue,
  around the gate, so a test could send a HERD_MATERIALIZE while the replay runs. It is held as
  RP-66 held `Order::ConflictCommand`: `waits_for_the_replay_end` is `runs_the_clock(o) ||
  matches!(o, ConflictCommand | HerdCommand)`. Tests (rtsim-jni, `mod herds`): the order made
  while a long replay runs (8,000 filler records, so several hand-overs) is not written until the
  replay end, `a_herd_command_ordered_while_replaying_waits_for_the_replay_end`; control
  `a_herd_command_ordered_without_a_replay_is_written_at_once`. *The audit of every other route that
  hands the herds engine a command outside the gate:*
  1. `Order::HerdCommand` / `herds::order_command`: the bypass, held (this record).
  2. `Order::HerdFound` / `herds::order_found`: founds a herd and runs the clock; already in
     `runs_the_clock` (RP-33), so held.
  3. The world model's founding (`worldmodel.rs` into `herds::found`): returns at the top while
     `st.replaying`.
  4. `herds::reject` (a synthetic HERD_SPAWN_REJECTED at `hand_over_target`): only after a
     Materialize write failed in `take_commands`; the gate never queues a Materialize while
     replaying, so none exists to fail.
  5. `shared::introduce_herds`: routes the areas of `shared.present`, which is empty while
     replaying (players are reported after the replay end, RP-11).
  6. The dragons' prey path (`dragons::run_engine` into the herds): goes through the herds'
     events and so through the gate.
  7. `Order::DragonCommand` (dragons.rs `order_command`) and the ships' equivalent are the same
     kind of bypass for slices D and S; not closed here.
  8. The generic world orders (`SpawnGroup`, `DespawnGroup`, `SteerGroup`) write records for
     Java and never reach the herds engine.
  Option not taken: a verdict per kind for a held test command. The order is a test's, not the
  engine's; holding it takes the units after it too, so order is kept.
- **RP-86** (the replay end; §5.6; RP-26, RP-5, RP-31): `begin_replay` changes no body (the pages'
  groups stay embodied, so the journal's records of them find them). `replay_done(legs, at)` runs
  only when a replay runs: every active group is abstract (clips' legs cancelled; the members of
  an embodied group have `seen.last = at`), every player's interest leg is cancelled and set to
  None, `puppets` 0, `pos` None, the floors folded (RP-81), then pushes queue as usual. `at` is the
  replay clock, `st.replay_clock.max(engine.now())`; the immediate end of a session with nothing to
  replay (`!st.replaying` at `reset_if_due`) uses `hand_over_target`. `active` is a superset set, so
  an honoured group (RP-82) is in it. Test:
  `the_puppet_counts_and_the_players_areas_end_at_the_replay_end` (control: a group in view at the
  start still has its puppets then). The world's shared area leg: the arm's `leave_area` cancels
  it, `replay_done` cancels the same handle again (the `LegIndex` handles are generational, so a
  stale cancel is a no-op), and sets it None.
- **RP-87** (the arm leaves the herds out of `player_left`; §2.1, §5.1, §5.6; RP-26): rtsim-jni's
  `shared::with_viewers_of` takes `Hear { herds, conflict }` instead of a `conflict` flag;
  `Hear::SESSION_START` leaves both out, `Hear::ALL` is every other call. The herds' `player_left`
  would despawn the groups the pages hold in view before the journal's records of them are
  applied, which is the design's alternative to gating in `Viewer::player_left` (not taken: the
  doc's design is the host's, and it keeps the engine's `Viewer` impl the same live and replayed).
  The herds' player work is the replay end's (RP-86). `with_viewers_of`, `Hear` and `session_start`
  in `shared.rs` are the lines slices D and S edit too (they move the dragons' and the ships'
  player work into their replay ends the same way): the three branches meet there.
- **RP-88** (an ending epoch is never reused; §5.3; common rule 5): a group whose
  HERD_MATERIALIZED and capture were both replayed (it ended in the replayed window, on an engine
  whose pages did not have it) is not decided again at its epoch after the replay end: the floor
  has its epoch, and the next group takes one above it. Tests: rtsim-ecology
  `an_epoch_that_ended_in_the_replayed_window_is_not_taken_again` (control: a group the pages held,
  ended in the window, takes the next epoch too); rtsim-jni
  `an_epoch_that_ended_in_the_replayed_window_is_not_decided_again` (control
  `an_epoch_that_ended_after_the_pages_held_it_is_not_decided_again`): the old session despawns the
  group, the crash, the next session's first group has an epoch above the ended one.
- **RP-89** (the log, the counts, what changes and what does not): the replay end's log line gains
  the herds' counts (commands dropped, "never" kinds, decisions deferred, groups honoured), under a
  wording that says they are totals since each engine started (rtsim-jni `shared::replay_end`,
  `Herds::replay_gate_counts`, `GateCounts`). The counts are lifetime totals, not reset by
  `begin_replay`, as conflict's are (the reset option is not taken: one reading for both modules,
  and the totals show a "never" count of an earlier session too); not saved (`replay_honoured`
  likewise). A rise in the "never" count is logged at once, not at the replay end: rtsim-jni
  `herds::take_commands` compares it with the link's `never_logged` and calls `log_error` naming
  the last kind (§4.3), as conflict's does; this replaces the replay-end `log_warn` the first cut
  had. No page, journal or wire format changes: the floors and the gate
  state are in memory only, `Embodied.player` is `NO_PLAYER` only transiently, and the
  rtsim-view fixtures need no regeneration for this slice. Forks, all conservative:
  floors keyed by agent in memory and kept for an agent with no herd; a copy of a body-matched
  kind raises the floor and counts late (RP-8); `Despawn` and `Steer` dropped while replaying
  with their decisions still counted in `stats`; the replay end's `at` the replay clock; a floor
  of `u32::MAX` wraps to 1 by the wrap rule (unreachable in practice).
  *Tests, rtsim-jni `mod herds` (base f2cf3d69, then after):*
  `a_long_replay_with_a_player_at_the_herd_never_sends_a_killed_sheep` and its control
  `a_one_hand_over_replay_...`: pass on both. Slice C's RP-11 hold keeps the player's sighting out of
  the replay on the base, so the scratch audit's members [0, 9, 10] are not sent again there; the
  pair stays as the gate's regression guard and now also checks the group's size and a zero
  "never" count. `a_group_decided_after_the_pages_does_not_have_its_epoch_decided_again_after_a_crash`:
  base fails, "the next session's is above it" (1 against 2: the epoch decided again after the
  crash); passes. Its control `a_group_in_the_pages_...`: passes on both.
  `a_replayed_capture_of_a_group_decided_after_the_save_lands_on_its_members`: base fails, the
  capture counted late (0 against 1); passes. Its control
  `a_replayed_capture_of_a_group_the_pages_held_lands_on_its_members`: base fails too, the same way,
  because the base's session start made the pages' group abstract before the replay (RP-5 closes
  both); passes. `an_epoch_that_ended_in_the_replayed_window_is_not_decided_again`: base fails,
  "epoch 1 after 1 ended"; passes; control `an_epoch_that_ended_after_the_pages_held_it_is_...`: passes
  on both. `a_herd_command_ordered_while_replaying_waits_for_the_replay_end`: base fails, "not
  written while the replay runs"; passes; control `a_herd_command_ordered_without_a_replay_...`:
  passes on both. `a_replayed_copy_of_a_materialization_is_not_honoured_but_its_epoch_is_counted`
  and its control `a_replayed_materialization_the_journal_holds_is_honoured` (RP-84): no honour on
  the base, so they do not compile there; with the copy routing undone the first gives (1, 1)
  against (0, 1). The rtsim-ecology tests need `begin_replay`, the floors and `group_epoch`, which
  the base lacks, so they do not compile there.
  *Traps:* (a) `cargo fmt` or rustfmt on rtsim-jni `engine.rs` reformats `engine/dragons.rs`,
  which the base has dirty; format the other files singly. (b) The sweep's pages path folds the
  floors into the herd's own epoch at `replay_done`, so only the copy and the ghost (no herd) cases
  exercise the floor's place under the wrap rule through the engine's `seen`. (c) The "deferred" count is
  zero in the jni long-replay test (no meeting of the pages fell due in that replay), so the
  `seen` early return is tested at the engine, not at the wire. (d) 3,000 filler records were too
  few for a multi-hand-over replay; 8,000 obstacle reports of a herd the engine never had give
  a dozen minimum-size hand-overs. (e) A mutation check run while a workspace `cargo test` is
  still working through its binaries overwrites the one it has not reached yet (the same file
  name): the first workspace run here reported the mutated copy test as failed; the rerun with
  nothing touched passed. (f) The first cut of RP-84 had `Herds::ingest_copy` with no caller in the
  host; only a test through the wire (a REASSERT in the journal) finds that.
  *Checks:* `cargo test --workspace --no-fail-fast`: 1,600 tests, 25 ignored, after the review fixes
  (a run at a load average of about 100 had four fail on timing, none in the herds: the 100,000
  district card test of rtsim-cli, the leased-run lockstep test of rtsim-host, two protocol tests
  of the conflict resident peak and the session start's close; each passed alone, the card one
  flipping from run to run);
  `cargo clippy -p rtsim-ecology -p rtsim-jni --all-targets -- -D warnings`: clean; the "never" unit
  test also under `cargo test --release -p rtsim-ecology a_never_kind`: passes. The sims
  (`rtsim-cli sim herds` and `sim dragons`, the two the herds engine feeds, run to files in /tmp
  and compared with `scenario/results` at the base, wall and timing fields left out): no
  difference.
