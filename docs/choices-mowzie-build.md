# Mowzie build: choices

Choices made while building MOWZIE.md §9's slices, where the plan left something open. Ids are
MZB-#, and each slice takes ids from its own range so slices built side by side never collide:
M1 1 to 49, M2 50 to 99, M3 100 to 149, M4 150 to 199, M5 200 to 249, M6 250 to 299, M7 300 to 349,
M8 350 to 399, M9 400 to 449, M10 450 to 499, M11 500 to 549, M12 550 to 599. Each slice writes only
under its own heading.

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

(none yet)

## M7 Rail

(none yet)

## M8 Lying in wait

(none yet)

## M9 Umvuthana

(none yet)

## M10 March

- A party whose surviving members are all Left (none stands to be handed back) gets no HAND_BACK line and so
  no recall toward its camp: the homecoming of MZB-211 starts from a HAND_BACK. M10 owns this case (found in
  the M5 review, P2-7); until then such a party stays where the engine left it.

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
