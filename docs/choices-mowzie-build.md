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

(none yet)

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
- **MZB-215** (Peaceful; reworded by the review, the rule is MZB-234): on Peaceful no new body appears, so a
  SPAWN is answered STALE and the next census orders it again. The first reason given here, that the engine
  would otherwise keep a body Peaceful despawns, was false: `shouldDespawnInPeaceful()` is false for every
  Mowzie entity, so the jar keeps its own mobs on Peaceful. A returning body still comes home (HAND_BACK
  proceeds) and the bodies already standing stay.
- **MZB-216** (stand-ins): `conflict/frostmaw/frostmaw` is `mowziesmobs:frostmaw`; `conflict/umvuthana/warrior`
  and `conflict/umvuthana/envoy` are `mowziesmobs:umvuthana` (the grove warrior, `EntityUmvuthanaMinion`);
  `conflict/umvuthana/raptor` is `mowziesmobs:umvuthana_raptor`.
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
- **MZB-231** (the index keeps its bodies through a hidden chunk): `onLeave` keeps a tagged body's index
  entry when the removal reason is null (the entity is still alive, its chunk only hidden or unloading) and
  drops it on a real removal; the census adds every tagged body of its faction found inside, idempotently;
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
  figure (PLACEHOLDER). **The remaining gap, recorded and not closed:** a Frostmaw beyond 64 blocks of a
  late den, or one not loaded at the den's first census, is not found, so the engine can SPAWN a second
  Frostmaw for that den.
- **MZB-233** (a line is played only on a camp that is playable now): the session's order stays
  (`RtsimSession.endOfTick`, then the hand-over's commands, then `Camps.tick`), since other modules share
  it. `reconcile`, `resolveSetOuts` and `playHeld` each first check that every footprint chunk is still at
  a FULL level or better and that `areEntitiesLoaded` holds for it; if not, the camp is treated as out and
  the line is STALE, as a line for an out camp is, so nothing is made, taken or set out in the tick a
  footprint went down. The test queues its line in the tick after the release, because a step's scripted
  command is applied in that tick's `ServerTickEvent.Post`.
- **MZB-234** (the Peaceful rule; rewords MZB-215): on Peaceful no new body appears, so a SPAWN is STALE
  and makes no body; a HAND_BACK proceeds, since the body is the one that returns (it is added, then its
  puppet goes); bodies already standing stay, as the jar's do. Both halves are tested.
- **MZB-235** (a holder the barrier cannot find; a departure from the lead's "skipped"): at the arm, a holder
  noted by UUID that `level.getEntity` does not return falls back to the chunk it was noted in at commit
  time, and `holdersUnresolved` counts it. The lead's text skipped it as unloaded and so already written;
  the fallback is the conservative option, since a holder in a chunk that is only being unloaded may not be
  written yet, and one more noted chunk costs one more write.
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
  Counters: `holdersResolved`, `holdersMoved` (it was in another chunk than at commit), `holdersUnresolved`.
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
  early hint where it fires (vanilla, the GameTests); levels from both are clamped to `NO_HOLDER`, so the
  event's value for an unloaded chunk and the poll's agree and the poll reports only real corrections. The
  poll is correct with the listener off. Two more consequences: an out camp is noticed at most 20 ticks late
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

- **MZB-300** (ids and base): M9 takes MZB-300 to 349, as its lead assigned. The header above gives M7 300 to
  349 and M9 400 to 449, so M7 takes 400 to 449 instead (the header is outside this heading and is not
  edited here). M5 has also written MZB-290 to 292, inside M6's range, so M6 starts after 292. The design
  reads M4 at `feat/mowzie-m4` 2b239fe2 (the engine, as `target/m4-src`) and M5 at `feat/mowzie-m5`
  d5b45b22 (the Java). They are separate branches, so M9's branch merges both before anything else.
- **MZB-301** (the chief's slot): a culture with `chief_body: Option<String>` keeps its chief in one slot of
  id `CHIEF_SLOT` = `u32::MAX` (-1 as a Java int). Umvuthana's is `mic_rtsim:conflict/umvuthana/umvuthi`, a
  new body dictionary entry with stand-in `mowziesmobs:umvuthi`. The id sorts last, so `Camp::index`'s binary
  search holds. The slot is outside the pool. These all skip it: `home_count`, `away_count`, `drawable`, the
  draw order, `spawn_lines`, `party_lost`, the units, the pool and its refill, the audit sums (I1 to I6)
  and the stray count. The den is untouched: its Frostmaw is its one unit (M6), and frostmaw's `chief_body`
  is None.
- **MZB-302** (the Umvuthi is a census body; replaces MZB-220):
  - `GroveKind.isBody` takes `EntityUmvuthi` as well as the exact minion type, and `isChief` is true for it.
  - The census lists it with BODY_CHIEF, its slot and generation, as well as in the chief fields (CHIEF,
    THRONE, health, position, direction).
  - An untagged Umvuthi in the footprint is a stray only while a grove found without its Umvuthi waits for
    its heir (MZB-305). The engine then ADOPTs it with LINE_CHIEF. After a chief's death, a stray does not
    cut the heir's wait.
  - Any other untagged Umvuthi is left alone, as the jar's own. A tagged one of a retired generation is
    REMOVEd under Rule U (MZB-333).
  - Followers, cranes and raptors stay uncounted.
- **MZB-303** (per-slot line flags; the mask in flight):
  - Each line's flags come from `line_flags(f, slot)`: LINE_CHIEF for CHIEF_SLOT, and the den's crystal
    flag as M4 sets it.
  - CAMP_BODIES' answer lines carry no flags, so the engine knows the chief by its slot id alone.
  - A MASK line names no slot (MZB-308), and `send_lines` marks a slot in flight only when its generation
    is not 0. So the engine keeps the MASK in flight as `Camp.mask: Option<(u64, u32)>` (op, line), and
    counts it in `in_flight`.
- **MZB-304** (the throne):
  - The engine keeps the throne from the grove's first census with CENSUS_THRONE (M4's `Camp.throne`).
  - Every CAMP_RECONCILE to a grove sets FLAG_THRONE, with THRONE_X, Y and Z and DIRECTION.
  - Java keeps the latest on the site (`Camps.Site.throne`, for that epoch).
  - With none known (a grove found without its Umvuthi), FLAG_THRONE is clear. Java then uses the fire pit's
    centre (MZB-218, MZB-242) and direction 1, as §7 test 30 says.
- **MZB-305** (a grove found without its Umvuthi):
  - The grove gets no chief slot, and `Camp.heir = Some(Heir { due, found_empty: true })` at
    `Pressure::at_cap`, so its heir comes after the cap's wait of 192 days.
  - MOWZIE.md states no wait. EQUILIBRIUM.md §2.3 takes a camp found empty as one farmed to the cap.
  - An untagged Umvuthi seen in the footprint before then cancels the wait and is ADOPTed into a new chief
    slot.
  - M6 uses the same rule for a den found empty (its migrant), and whichever slice lands first writes
    `Pressure::at_cap`.
- **MZB-306** (a chief killed at the grove): this is CAMP_DEATH on the chief slot, while Body, or while Left
  with its take still held (MOWZIE.md ledger 5). In order:
  1. M6's `chief_killed_at_camp(w, f, killer, at)` runs: `ensure_chief`, the fall, only the killer's own
     feud steps down, and every party not Home routs.
  2. `Camp.pressure` steps up by 1, unless the killer kind is BY_MOB (another mob's kill is no player's
     pressure).
  3. The slot is freed with died true, and the trade memory is cleared.
  4. `heir_due = max(cowed_until, at + wait)` is set, and `Ev::CampHeir { faction, stamp }` is booked for
     then. It is wheel tag 9: M4's Refill is 7 and M6's Arrival is 8.
- **MZB-307** (the heir's wait): a culture field `chief_renewal: Option<RenewalDef>`. Umvuthana's is
  `{ base_days: 6, cap_days: 192, half_life_days: 96, boss: true }` (MZ-14 (a)). The wait is
  `min(cap, base × 2^P)` days, with P decaying at the half-life, as in M6's den renewal.
  - It reuses M6's `RenewalDef` type and resolution, but as a field of its own: M6 validates its `renewal`
    to need `recruits_per_day == 0` (a culture of one), and the grove recruits 2 a day.
  - Nothing is ordered while no warrior is home (Body with no line). The heir then waits, and the camp is
    looked at again at each census.
- **MZB-308** (the heir takes a warrior): when the heir is due, one CAMP_RECONCILE carries two lines.
  - The first is a new `ACTION_MASK` = 8: slot 0, generation 0, the warrior body. Java takes the camp's
    warrior farthest from every non-spectator player that has no line in flight or held.
  - The second is SEAT_HEIR: slot CHIEF_SLOT, a new generation, the umvuthi body and LINE_CHIEF. L_A is
    the old generation when the chief slot is Dead, else 0.
  - Java plays the two together: it makes the heir first, then removes the warrior (its record on a marker,
    as a take). If either cannot be done, both are STALE and nothing changes.
  - MASK is answered with the taken warrior's slot and generation.
- **MZB-309** (booking the heir):
  - MASK DONE frees that slot (died false) and calls `pool_shift(-1)`, since the warrior becomes the chief.
  - SEAT_HEIR DONE makes the chief slot Body, and clears `Camp.heir` and the trade memory. NOTHING does the
    same: the body already stands, tagged.
  - STALE on either line orders the pair again at the next census.
  - While the pair is in flight `camp_drawable` is 0, so no muster takes the warrior about to be masked.
  - A MASK answer can name a slot whose own line is still in flight. The slot is freed all the same, and
    that line's late answer finds a retired generation and is ignored.
  - The SEAT_HEIR spawn waits under the chief's spawn hold (MZB-333), and MASK waits with it.
- **MZB-310** (seated in place): this is SEAT_HEIR with L_A not 0, while the old generation's body stands
  tagged in the footprint.
  - Java waits while that body is in a fight: it has a player target, and a player hit it within 600
    ticks (PLACEHOLDER). The plugin keeps the hits in a weak map.
  - Then, in one tick, Java answers the old body's held lines STALE, removes it with no CAMP_DEATH, and
    makes the heir where it stood: at full health, in its direction, with an empty trade memory.
  - With the old body gone, it is a plain SEAT_HEIR at the throne.
  - In M9 only scripted lines reach it (test 30), since a chief slot is Dead only after a death away
    (MZB-312).
- **MZB-311** (a chief written off): a chief slot missed 3 times over 2 days (camp.rs's write-off rule) is
  booked as a death at the grove with BY_NONE.
  - P steps up (EQUILIBRIUM.md §2.2 counts a missing home body), the memory clears and the heir waits.
  - `chief_killed_at_camp` is not called: no one saw it die, so there is no report, no feud steps down and
    no party routs.
- **MZB-312** (a chief killed away, the engine half):
  - A Left chief slot becomes Dead. The census REMOVEs its throne body under Rule U, or the heir is seated
    over it (MZB-310).
  - A Gone chief slot is freed with died true.
  - Both step P up, clear the memory and set the heir.
  - Nothing sends a chief's death away before M11 (BAND_CASUALTIES with the chief, ledger 24), so only Rust
    tests reach it.
- **MZB-313** (who leads, MOWZIE.md ledger 4): two culture fields.
  - `chief_leads: Vec<PartyKind>`: the default is raid, invasion and siege (today's "any war party").
    Umvuthana's is `["invasion"]`; frostmaw's is M6's.
  - `chief_strength: f32`: umvuthana's is 5.0.

  `chief_can_lead` holds when the chief slot exists, is Body or Owed, has no line in flight, and no heir
  waits. A draw for a party the chief leads takes the chief too: Body becomes Left, Owed becomes Gone.
  While it leads, `strength` and `strength_by_class` add `chief_strength` in the first unit's class, so a
  full Invasion (6 warriors, 2 raptors) is 6 × 0.4 + 2 × 2.6 + 5.0 = 12.6.
- **MZB-314** (no chief stack before M11):
  - BAND_CHIEF and MEMBER_CHIEF are written only when `chief_stack(c)` holds, which is false for a culture
    with `chief_body`. So MZB-109's stand-in chief never appears with a grove's band.
  - The chief's SET_OUT line finds no puppet to claim it, so it is a take (MZB-222), held under Rule U
    (MZB-333). The Umvuthi stays on its throne until no player is near, then goes.
  - M11 turns `chief_stack` on, with the aloft puppet and CHIEF_BODY's travel.
- **MZB-315** (coming home): a party with the chief ends Home.
  - A Left chief slot becomes Body.
  - A Gone one becomes Owed. The engine sends CHIEF_BODY (MZB-321), then a chief SPAWN, held as MZB-333
    says.
  - M9 sends no chief HAND_BACK, since there is no puppet.
  - A held take can apply after the party is home. Its late DONE makes the slot Owed, and a SPAWN follows.
    This is accepted: both wait until no player is near.
- **MZB-316** (gifts at the grove's price, the rest of ledger 12):
  - When the market has no price for the tribute good, `gift_value` prices it at `1 / tribute_per_coin`.
    Today it uses `m.price(g) / PRICE_ONE`, which is 0 for an unpriced good, so every gift of gold was
    worth nothing.
  - So the Umvuthi's 7 gold blocks, 567 nuggets, are 63 coins.
- **MZB-317** (TRADED):
  - A masked trade with a grove's minion is worth `nuggets / tribute_per_coin` coins. It is booked through
    `Conflict::traded`, once by op id.
  - Java writes it only for a minion tagged by a grove or standing in a grove's footprint. The count is
    the gold nuggets on either side of the trade, and a trade with no gold side writes nothing.
  - A trade with a band's puppet writes nothing.
- **MZB-318** (CHIEF_PAID):
  - It calls `pay_tribute(w, faction, payer, None, goods, at)`. That settles the payer's open demand, else
    the settlers', else it books a gift (MZB-316).
  - The payer goes into the trade memory only when the slot and generation are the live chief's (Body).
  - PAID_STRAY (flags 1) is a payment to an untagged Umvuthi standing in a grove's footprint. It is booked
    as tribute, with no memory entry.
  - A payment to an Umvuthi outside every grove writes nothing.
- **MZB-319** (gold blocks and ingots):
  - The Umvuthi wants gold blocks (the config's gold_block × 7), but the catalogue has only `gold_nugget`.
  - So rtsim-jni's CHIEF_PAID translation maps items through `TRIBUTE_EQUIVALENTS` (gold_block 81 nuggets,
    gold_ingot 9) before `own_items`, merging entries. The engine sees 567 gold_nugget.
  - TRIBUTE_PAID (M3's) is untouched.
- **MZB-320** (trade memory): `Camp.traded: Vec<(u64, u64)>`, sorted, of at most 1,024 players
  (PLACEHOLDER, the census's own cap).
  - Past the cap, a payment is still tribute but is not remembered.
  - It is fed by CHIEF_PAID, and by the census's traded list when TRADED_SLOT and TRADED_GENERATION are the
    live chief's. The two are unioned, so a payment the engine has and the body has not yet written stays.
  - It is saved. It is cleared when the chief dies or is written off, and when an heir is seated.
- **MZB-321** (CHIEF_BODY at home):
  - It holds agent 0, the camp's epoch, health 0 (the body keeps its own), flags 0 and the memory.
  - It is sent before every chief SPAWN, and whenever a census's traded list is shorter than the engine's.
    It is never sent before SEAT_HEIR, since an heir's memory is empty.
  - Java core keeps the latest for each faction for that epoch, and drops it at CAMP_OUT.
  - Java writes it through a new `CampKind.chiefBody(Mob, ChiefBody)` (a default no-op): when the chief's
    body is made, and at once onto a standing chief body of that slot and generation.
  - GroveKind writes it with `setTradedPlayersCompound` (a ListTag of `NbtUtils.createUUID`), never with
    `rememberTrade`, so nothing is journaled.
  - Its puppet and travel use (agent not 0) are M11's.
- **MZB-322** (HAVEN_CHEST):
  - rtsim-jni decodes it in a new `observe_latest` arm, shared with M6's CHIEF_STATE. Today `observe_state`
    drops latest-wins kinds.
  - The engine keeps the chests by faction, unsaved, and forgets one at CAMP_OUT.
  - The plugin's `GroveChests`, after Ice and Fire's `Lairs.java`, looks at each grove that is in every 200
    ticks (PLACEHOLDER). It picks the chest or barrel (`ChestBlockEntity`, `BarrelBlockEntity`) in the
    footprint nearest the centre that has room or a loot table not yet rolled.
  - It sends HAVEN_CHEST when the choice or its free slots change, and one with 0 free slots when there is
    none.
- **MZB-323** (tribute into the chest):
  - Each booked payment (CHIEF_PAID, the envoy's TRIBUTE_PAID, a gift) adds its goods to
    `Camp.owed_tribute`.
  - While the grove's chest is known with free slots and nothing is being placed, the engine takes one good
    out of the books: the least of what is owed, the faction's stock and free × 64. It uses a new
    `World::stock_to_players(i, goods, at)` (player_out).
  - It sends TRANSFER_TO_CONTAINER through rtsim-jni's outbox with an op id, as the dragons do, and keeps it
    as `Camp.placing: Option<Placing { op, good, amount }>` (saved).
  - APPLIED or DUPLICATE_OP lowers what is owed by the amount. FAILED returns the goods through
    `stock_from_players`.
  - A grove whose chest is gone keeps owing.
- **MZB-324** (owed hunts, ledger 11): a unit field `hunts: u32` (raptor 2, PLACEHOLDER) and a culture field
  `hunt_cap: u32` (8, PLACEHOLDER).
  - `raid_district` adds `survivors × hunts` for each class it did not skip, capped at `hunt_cap` per
    district.
  - A watched raid adds nothing, since there the jar's raptors hunt for themselves.
  - The hunts are kept in `Faction.hunts: Vec<Hunt { district, count, op }>`, a small Vec, never a one-entry
    BTreeMap.
- **MZB-325** (the hunt records; the ids are provisional):
  - OWED_HUNT (0x880C, engine to Java, latest-wins for each faction and district): faction, district,
    dimension, centre x and z, radius 48 (PLACEHOLDER), count and op id.
  - HUNT_APPLIED (0x0816, journaled): faction, district, op id and killed.
  - They are the next free ids after today's DIFFICULTY 0x0815 and BAND_MARCH 0x880B. Another slice may
    take them first, so they are settled at landing.
  - One hunt is in flight for each district. It is resent at each session start and booked once by op id.
    HUNT_APPLIED clears it whatever `killed` is.
- **MZB-326** (applying a hunt): core `conflict/OwedHunts.java`, vanilla only.
  - It keeps the latest command for each faction and district, unsaved.
  - Once the circle's chunks tick entities, it kills up to `count` animals in it, nearest the centre first.
    They must pass the jar's raptor test, rewritten in vanilla calls: no ATTACK_DAMAGE or less than 3, and
    0.1 < width² × height < 6 (`EntityUmvuthana.registerHuntingTargetGoals`).
  - It spares tamed, named and leashed animals (MZB-342), and kills with `kill()`, so their drops fall.
  - It journals HUNT_APPLIED even when it killed none (the hunt is spent), with the circle's entity chunks
    in M5's barrier.
  - A crash between the kills and the save can apply one hunt twice, bounded by the cap.
- **MZB-327** (plunder takes livestock goods, the rest of ledger 11):
  - A culture field `plunder_goods: Vec<String>`. Umvuthana's is `["leather", "wool"]`; the catalogue has no
    meat good.
  - When it is not empty, `raid_node`'s plunder takes those goods only, and its cap counts those goods
    only. It ignores the units' `carry`: raptors carry 0, so today their plunder takes nothing.
  - The goods are eaten: taken out of the node's stock through `take_stocks` and booked to no one.
  - Only witnesses learn of it first-hand (CL-D61); the rest learn by gossip.
- **MZB-328** (body makers and extras):
  - The new `ConflictBodies.registerMaker(String body, Maker)`, with
    `Mob make(ServerLevel level, double x, double y, double z, float yaw, List<Mob> extras)`. ConflictBridge's
    placing (`ConflictBodies.create` at ConflictBridge.java:1145) asks the maker first.
  - Extras get the AGENT tag before they join and are kept in `Slot.extras`. They despawn with their slot
    or stack, book nothing and are never saved.
  - The leader's death is the unit's casualty. A follower's death is nothing, and a follower left with no
    leader fights on until its stack despawns.
  - The raptor maker:
    1. the constructor, then `moveTo`;
    2. no `finalizeSpawn`, which would add 4 or 5 untagged followers (MZB-216's stand-in leaks them today);
    3. exactly 4 followers through `EntityUmvuthanaFollowerToRaptor(type, level, leader)`, on the jar's
       0.1-block circle with its weapon roll (`random.nextInt(3) == 0 ? 1 : 0`).

    With exactly 4 followers, a pack's strength is the unit's 2.6.
  - M11's summons reuse the extras.
- **MZB-329** (raptors on Peaceful; extends MZB-234):
  - A raptor and its followers discard themselves on Peaceful (their own `tick`), so the maker returns null
    on Peaceful and the stack stays pending.
  - A pack that discards itself is a despawn, not a casualty.
- **MZB-330** (the mixins): in the plugin's `mic_rtsim_mowziesmobs.mixins.json`, shared with M6 (see *Shared
  with M6* below).
  - `EntityUmvuthiMixin` at `rememberTrade` HEAD sends CHIEF_PAID:
    - the items from `getDesires()`, which still names what `fulfillDesire` just took;
    - the player from its game profile id;
    - the slot and generation from the body's CampMember;
    - PAID_STRAY when it is untagged in a footprint, and nothing outside a grove.
  - `EntityUmvuthiMixin` also gives `die`'s `getEntitiesNearby` call an empty list for a second body
    (MZB-331).
  - `ContainerUmvuthanaTradeSlotResultMixin` at `onTake`'s `ItemStack.shrink` sends TRADED, server side
    only.
- **MZB-331** (a second body): a chief body whose REMOVE line is held, or whose member already died, is a
  second body.
  - Its LivingDropsEvent is cancelled.
  - Its death journals nothing; `Camps.onDeath` skips it.
  - Its `die` takes no warriors with it (MZB-330), and it is removed.
- **MZB-332** (the killer, shared with M6): core's `Camps.killer(dead, source)` takes the first of:
  1. the source's entity, when it is a player;
  2. the player owner of an `OwnableEntity` source;
  3. `dead.getKillCredit()`, when it is a player (vanilla keeps one 100 ticks);
  4. any other source entity, as BY_MOB;
  5. else BY_NONE.

  It applies to Camps only. M11 adopts it for BAND_CASUALTIES.
- **MZB-333** (Rule U's holds):
  - The new `CampKind.holdRemoval(level, site, body)` defaults to false. GroveKind returns true for an
    Umvuthi with a non-spectator player within 160 blocks or a player target.
  - TAKE, REMOVE and SET_OUT-as-take lines on such a body are held with no time limit, and an epoch's end
    answers them STALE.
  - A chief SPAWN or SEAT_HEIR waits on `holdSpawn`: a player within 160 blocks of the throne, for at most
    `HOLD_MAX` 6,000 ticks.
- **MZB-334** (GroveKind's chief; supersedes MZB-209 and MZB-202):
  - `chiefLines()` is true.
  - `make` for the umvuthi body:
    1. the constructor and `moveTo`;
    2. `setDirection` before `addFreshEntity`, since the jar reads it only at tick 1;
    3. `finalizeSpawn(STRUCTURE)` and `restrictTo(fire pit, -1)`, as worldgen does;
    4. CHIEF_BODY's memory.
  - `spawnAt` uses the site's throne (MZB-304), else the fire pit.
  - `joined` restricts it again.
  - The census writes TRADED t, TRADED_SLOT and TRADED_GENERATION from the chief body's
    `getTradedPlayers()`.
  - `sound()` returns `mowziesmobs:block.gong`, the grove's notice sound.
- **MZB-335** (strength in observed fights, ledger 19, shared with M6): the plugin registers one
  IndustryPlugin whose `fightStrength` gives:

  | Body | Strength |
  |---|---|
  | Frostmaw (M6's) | 41.7 |
  | Umvuthi | 5.0 |
  | Any other `EntityUmvuthana` | 0.4 |
  | Raptor | 1.0 |

  So a pack of a raptor and 4 followers is 2.6.
- **MZB-336** (the envoy):
  - Its body is a grove minion. M3's guard is unchanged: it strips the jar's player and hurt-by goals while
    the demand is open, and lifts them at an insult.
  - The payment is taken at EntityInteract HIGHEST (MZB-110) and the event is cancelled, so the jar's trade
    screen does not open.
- **MZB-337** (the page format):
  - RTC7 and META 6, shared with M6. The second slice to land bumps again.
  - Camp gains `pressure` (M6's), `heir`, `traded`, `owed_tribute`, `placing` and `mask`.
  - Faction gains `hunts`, and the wheel gains `Ev::CampHeir`.
  - Fixtures are regenerated, with no migration.
- **MZB-338** (rtsim-jni):
  - CHIEF_PAID, TRADED and HAVEN_CHEST leave boundary.md §9.6's deferred list.
  - CHIEF_BODY and OWED_HUNT get writers, and HUNT_APPLIED gets a decoder.
  - The tribute's TRANSFER_TO_CONTAINER rides the existing `Outbox<TransferCmd>`, completed by
    COMMAND_RESULT.
- **MZB-339** (where the GameTests' camps come from):
  - Tests 13, 15, 16 and 25's payment half declare their grove under `mowziesmobs:umvuthana_grove` at a far
    base, so the native engine claims it as umvuthana's.
  - Tests 27 and 30, and test 29's throne clauses, use scripted test camps (MZB-227).
  - A GameTest cannot restart the session (MZB-228), so every restart case goes to the Rust protocol tests.
- **MZB-340** (test 29's throne clauses are built in M9): its first three clauses are built in M9:
  - the take and the removal held past 6,000 ticks;
  - the spawn held for the player or 6,000 ticks;
  - an epoch's end answering them STALE.

  M11 adds the rest (the puppet, re-binding and the four exceptions). Its second-body clause needs a
  puppet, so M11 builds it, on M9's MZB-331.
- **MZB-341** (test 25 in three parts):
  - a GameTest on a real grove for the payment;
  - a scripted test camp for CHIEF_BODY's write;
  - Rust protocol tests for the restart, the Invasion round trip, a dead slot's census and payment, and a
    succession.

  The puppet clauses are M11's.
- **MZB-342** (an owed hunt spares tamed, named and leashed animals; the question for Ben, decided on the
  recommendation): an owed hunt kills only stock that nobody has marked as theirs. Embodied raptors do what
  the jar's AI does: its damage test already spares wolves (ATTACK_DAMAGE 4), but not a named cow.
- **MZB-343** (the seams M9 leaves):
  - M10 owns BAND_MARCH, the column and the walk-in. M9 touches none of them: a grove's band, with or
    without its chief, marches as M10 makes any band march.
  - M11 owns the aloft movement, CHIEF_BODY's travel (agent not 0, ledger 24), `chief_stack` for the grove
    (its stack and puppet, MEMBER_CHIEF), the chief's HAND_BACK, the summons (a mixin at
    `EntityUmvuthi.java:1050` adding to MZB-328's extras), the chief's death away through BAND_CASUALTIES
    (MZB-312's engine half) and test 29's other clauses.

### M9 design for the implementer

**Where it stands.** Designed only; no code. Branch `design/mowzie-m9` (this doc). The implementation
branch starts from `feat/mowzie-m5` with `feat/mowzie-m4` merged in (MZB-300). Compile as M5 does, in this
worktree's own target dir; the Rust is `nice -n 10 nix develop .# -c cargo test -p rtsim-conflict -p
rtsim-jni` from the repo root.

**Jar facts** (Mowzie's Mobs 1.21.1-1.8.2, read with cfr in `target/m9-mowzie-cfr/src`, signatures checked
with javap):

*Registry and the Umvuthi*
- `EntityHandler`: `UMVUTHI` (id `mowziesmobs:umvuthi`), `UMVUTHANA_MINION` (`mowziesmobs:umvuthana`),
  `UMVUTHANA_RAPTOR` (`mowziesmobs:umvuthana_raptor`), `UMVUTHANA_FOLLOWER_TO_RAPTOR`
  (`mowziesmobs:umvuthana_follower_raptor`), `UMVUTHANA_CRANE`.
- `EntityUmvuthi`:
  - `public boolean requiresCustomPersistence()` (393) returns true.
  - `public void tick()` (438) reads `getDirectionData()` only when `this.tickCount == 1` (446), so
    `public void setDirection(int direction)` (701) takes only before the first tick.
  - `public ItemStack getDesires()` (725) is synched data, the config's `gold_block` × 7 (ConfigHandler
    470 to 471).
- Trade memory:
  - `public void setTradedPlayersCompound(ListTag players)` (729) makes a new compound, so it syncs.
  - `public Set<UUID> getTradedPlayers()` (735) reads list type 11 (int arrays).
  - `public boolean fulfillDesire(Slot input)` (757) does `input.remove(desires.getCount())`.
  - `public boolean hasTradedWith(Player player)` (766) keys on `player.getGameProfile().getId()`.
  - `public void rememberTrade(Player player)` (770) appends `NbtUtils.createUUID(profile id)` to "players"
    with no dedupe. Its only caller is `MessageUmvuthiTrade.handleServer` (61):
    `if (!hasTradedWith && umvuthi.fulfillDesire(trade.getSlot(0))) umvuthi.rememberTrade(player)`. It runs
    only when `umvuthi.getCustomer() == player` and `player.containerMenu instanceof ContainerUmvuthiTrade`;
    a player who has traded already gets Sun's Blessing instead.
- `public void die(DamageSource cause)` (859) calls `super.die(cause)` twice, so two LivingDeathEvents.
  Between them it calls `this.getEntitiesNearby(EntityUmvuthana.class, 30.0, 20.0, 30.0, 30.0)` (861) and
  sets `timeUntilDeath = random.nextInt(20)` on each that `isUmvuthiDevoted()`. That warrior then
  `hurt(indirectMagic(this, null), health + 1)` itself, a source with no causing entity.
  - The bytecode is invokespecial `MowzieGeckoEntity.die`, invokevirtual
    `getEntitiesNearby:(Ljava/lang/Class;DDDD)Ljava/util/List;`, `isUmvuthiDevoted`, invokespecial
    `MowzieGeckoEntity.die`.
  - The second `die` finds `dead` already set, so it drops nothing. M5's `Camps.onDeath` books one death,
    since its `dying.putIfAbsent` keys on the UUID.
- `public SpawnGroupData finalizeSpawn(ServerLevelAccessor world, DifficultyInstance difficulty,
  MobSpawnType reason, SpawnGroupData livingData)` (945).
- Worldgen (`UmvuthanaGrovePieces` 278 to 287) does `new EntityUmvuthi(UMVUTHI.get(), level)`,
  `setDirection(i)`, `finalizeSpawn(level, difficulty, STRUCTURE, null)`, then `restrictTo(firePitPos, -1)`.
  The loot (the Sol Visage) is `LootTableHandler.UMVUTHI`.

*Minion trades*
- `ContainerUmvuthanaTrade` holds `private final EntityUmvuthanaMinion umvuthanaMinion`.
- Its private inner `SlotResult extends Slot` has `public void onTake(Player player, ItemStack stack)` (106).
  It calls `ItemStack.shrink:(I)V` once (113), and only after `getOfferingTrade()` and `Trade.getInput()`
  match the input.
- `Trade` is `record Trade(ItemStack input, ItemStack output, int weight)`.
- The minion's default trades pay or take gold nuggets on one side, except flint for feathers and back.

*Targeting*
- `EntityUmvuthana.registerHuntingTargetGoals` (338 to 356):
  - animals that have no ATTACK_DAMAGE or less than 3.0, with 0.1 < width² × height < 6.0;
  - zombies, skeletons and zoglins;
  - it avoids creepers;
  - players, but never on Peaceful, and skipping one whose `armor.get(3)` (the head) is an `UmvuthanaMask`.
- The raptor (88 to 89) and a leaderless follower (`setLeaderUUID(ABSENT_LEADER)`, 96) use it. The minion
  does not hunt animals.
- `EntityUmvuthanaMinion.registerTargetGoals` (131 to 147): `UmvuthanaHurtByTargetAI`, and a player goal
  that skips masked players unless `target == getMisbehavedPlayer()`.

*Raptors*
- `EntityUmvuthanaRaptor.finalizeSpawn` (191 to 201):
  - `size = random.nextInt(2) + 3`, `for (i = 0; i <= size; ++i)`: 4 or 5 followers.
  - Each is `new EntityUmvuthanaFollowerToRaptor(UMVUTHANA_FOLLOWER_TO_RAPTOR.get(), level(), this)`, put
    at 0.1 × (cos, sin)(theta × i), with `setWeapon(random.nextInt(3) == 0 ? 1 : 0)` and
    `world.addFreshEntity`.
- The follower's constructor is
  `(EntityType<? extends EntityUmvuthanaFollowerToRaptor>, Level, EntityUmvuthanaRaptor leader)`.
- `public void addPackMember(EntityUmvuthanaFollowerToRaptor)` (142).
- The raptor's tick keeps its pack on a 3-block circle (it moves a follower more than 20 blocks off), and
  at 117 to 118 does `if (!isClientSide && difficulty == PEACEFUL) this.discard();`. The follower's tick
  (49 to 50) does the same.
- `remove` in the first tick kills the pack, and `die` sets them leaderless.
- ATTACK_DAMAGE 6 and MAX_HEALTH 10 (97 to 98).
- `checkSpawnRules` (221) refuses Peaceful.
- `ConfigHandler.SpawnConfig`'s constructor takes `heightMax` before `heightMin`, so the Umvuthana's
  `-65, 60` is a minimum of 60 with no maximum.

*Other*
- `sounds.json` has `block.gong`, so the sound is `mowziesmobs:block.gong`.
- The mixin target strings:
  - `Lcom/bobmowzie/mowziesmobs/server/entity/umvuthana/EntityUmvuthi;getEntitiesNearby(Ljava/lang/Class;DDDD)Ljava/util/List;`
  - `com.bobmowzie.mowziesmobs.server.inventory.ContainerUmvuthanaTrade$SlotResult`
- MixinExtras 0.5.3 is NeoForge's (`mixinextras-neoforge` in the Gradle cache), so `@WrapOperation` is
  there.

**Contradictions with MOWZIE.md and other slices:**
1. **Peaceful.** MZB-215, MZB-234 and M6's draft say the jar keeps its own mobs on Peaceful. Raptors and
   their followers discard themselves (MZB-329).
2. **The raptor stand-in.** MZB-216 makes it through `finalizeSpawn`, which adds 4 or 5 untagged followers
   per raptor, and they leak (MZB-328).
3. **Pack size.** §4.13's 2.6 for a pack fits 4 followers; the jar makes 4 or 5. The maker makes 4.
4. **The trade event.** Ledger 18 says "an event on minion trades". There is none, so a mixin is used at
   `SlotResult.onTake`.
5. **The set-out.** §4.5 hands the Umvuthi's slot at its set-out "to its puppet". There is no puppet before
   M11, so it is a held take (MZB-314).
6. **A grove found empty.** No wait is stated for a grove found without its Umvuthi. MZB-305 takes
   EQUILIBRIUM.md's cap.
7. **The census body.** MZB-220 kept the Umvuthi out of the census bodies, and MZB-302 replaces it.
   MZB-202 and MZB-209 are superseded by MZB-320 and MZB-334.
8. **The id range.** See MZB-300.

**What M5 must provide** (it is built; keep these as they are): `Camps.onDeath`'s `putIfAbsent` by UUID
(test 27 counts on it), `holdSpawn`, `HOLD_MAX`, `playerNear`, `playable` (MZB-233), the census writer's
chief fields and traded fields, and `MowzieCamps.groveCentre` (MZB-242). M9 changes `GroveKind` (MZB-334),
the line dispatch's `chiefLines` gate and the raptor stand-in (MZB-328).

**Shared with M6** (each is built once, by whichever slice lands first; the other uses it):
- `Camp.pressure: Pressure` with `Pressure::at_cap`, and the found-empty rule (MZB-305).
- The `RenewalDef` type and its resolution (MZB-307 adds its own field).
- `chief_killed_at_camp` (MZB-306).
- The mixin config `mic_rtsim_mowziesmobs.mixins.json`: required true, minVersion 0.8, JAVA_21,
  defaultRequire 1, no refmap. M6 adds `EntityFrostmawMixin`; M9 adds `EntityUmvuthiMixin`,
  `ContainerUmvuthanaTradeSlotResultMixin` and `ContainerUmvuthanaTradeAccessor`.
- rtsim-jni's `observe_latest` arm (CHIEF_STATE and HAVEN_CHEST).
- `Camps.killer` (MZB-332).
- The one `fightStrength` plugin (MZB-335).
- The page bump (MZB-337).
- The wheel tags: Arrival 8, CampHeir 9.

Two fixes for M6:
- its draft's Peaceful wording (item 1 above);
- its `renewal` validation, which must not be read as the grove's (MZB-307).

**Engine** (rtsim-conflict, rtsim-jni):
- `culture.rs`:
  - `CultureDef` and `Culture` gain `chief_body`, `chief_renewal`, `chief_leads`, `chief_strength`,
    `plunder_goods` and `hunt_cap`; `UnitDef` gains `hunts`.
  - Validation: `chief_body` names a dictionary body, `chief_renewal` needs `camp_bodies`, and every
    `plunder_goods` entry is a catalogue good.
  - `data/cultures/umvuthana.json`: chief_body umvuthi, chief_renewal {6, 192, 96, true}, chief_leads
    [invasion], chief_strength 5.0, plunder_goods [leather, wool], hunt_cap 8, raptor hunts 2.
- `engine/camp.rs`:
  - `CHIEF_SLOT`, the exclusions (MZB-301), `Camp` fields `heir: Option<Heir>`,
    `traded: Vec<(u64, u64)>`, `owed_tribute: Vec<(u32, i64)>` (good, milli), `placing: Option<Placing>`
    and `mask: Option<(u64, u32)>`.
  - `line_flags(f, slot)` (MZB-303).
  - The census: the chief stray (MZB-302, MZB-305), the traded union (MZB-320), CHIEF_BODY when short
    (MZB-321), and the chief's write-off (MZB-311).
  - `chief_can_lead`, and `camp_draw` taking the chief (MZB-313).
  - `camp_heir(w, f, at)`, which sends the pair (MZB-308) or waits. The booking of ACTION_MASK and
    SEAT_HEIR answers (MZB-309), and of a Dead chief slot (MZB-312).
- `engine.rs`:
  - `strength` and `strength_by_class` (3098, 3120) add the leading chief.
  - The `chief_stack(c)` gate at MEMBER_CHIEF (3976) and BAND_CHIEF (4022).
  - The party's end at Home for the chief (MZB-315).
  - CAMP_DEATH on CHIEF_SLOT runs MZB-306.
  - `gift_value` (2508), MZB-316.
  - TRADED to `traded` (2477), MZB-317; CHIEF_PAID to `pay_tribute` (2404), MZB-318.
  - `owed_tribute` and `place_tribute(w, f, at)`, MZB-323.
  - `raid_district` (3453) owes hunts and `raid_node` (3579) plunders, MZB-324 and MZB-327.
  - `send_hunts(at)` at session start and on a new hunt; `hunt_applied`.
- `rtsim-world`: `World::stock_to_players(i, goods, at)`, the mirror of `stock_from_players`.
- `boundary.rs`: `ACTION_MASK` = 8, OWED_HUNT and HUNT_APPLIED (MZB-325), and the CHIEF_BODY writer. The
  decoders of CHIEF_PAID, TRADED and HAVEN_CHEST are wired into `Conflict::observe`.
- `persist.rs`: RTC7 and META 6 (MZB-337), `Ev::CampHeir` on tag 9.
- rtsim-jni:
  - `TRIBUTE_EQUIVALENTS` in the CHIEF_PAID translation (MZB-319).
  - `observe_latest` (MZB-322).
  - Writers for CHIEF_BODY and OWED_HUNT, and the HUNT_APPLIED decoder.
  - `place_tribute`'s TRANSFER_TO_CONTAINER through `st.outbox.push_with_op`, with COMMAND_RESULT
    completing it (engine.rs 1563 to 1568) and its outcome handed to the conflict as `placed(op, outcome)`.
- `synth.rs` emulator, for run 10:
  - `CampBody` gains `chief: bool` and `traded: Vec<Uuid>`;
  - `CampWorld` gains `near: bool` (a player within 160 blocks);
  - `apply_line` plays ACTION_MASK and SEAT_HEIR as the plugin does: the pair together, the farthest
    warrior, the holds and seating in place.

**Java, core** (`neoforge/core/.../mic_rtsim/`):
- `ConflictRecords`:
  - `CampReconcile.ACTION_MASK` = 8.
  - Layouts for CHIEF_PAID (0x0814), TRADED (0x0810), HAVEN_CHEST (0x0092), CHIEF_BODY (0x880A),
    OWED_HUNT and HUNT_APPLIED, matching boundary.md byte for byte.
- `conflict/CampKind`: `default boolean holdRemoval(ServerLevel, Camps.Site, Mob body)` (false) and
  `default void chiefBody(Mob body, ChiefBody memory)` (no-op).
- `conflict/Camps`:
  - FLAG_THRONE read into `Site.throne` (MZB-304).
  - The dispatch plays chief lines when `chiefLines()` holds.
  - ACTION_MASK with SEAT_HEIR as one unit, and the seating in place (MZB-308, MZB-310).
  - `holdRemoval` holds with no limit, answered STALE at the epoch's end (MZB-333).
  - The CHIEF_BODY store and its write (MZB-321).
  - `killer(dead, source)` (MZB-332), replacing `byKind(source.getEntity())` in `onDeath` (1203).
  - The second body: a weak set, a LivingDropsEvent cancel at LOWEST, and a skip in `onDeath` (MZB-331).
- `conflict/ConflictBodies`: `registerMaker` and `Maker`. `ConflictBridge`: `Slot.extras`, the maker at
  1145, and the extras' despawn on recall, dematerialize and failure (MZB-328).
- `conflict/OwedHunts` (MZB-326).

**Java, plugin** (`neoforge/mowziesmobs/.../mic_rtsim_mowziesmobs/`):
- `GroveKind` (MZB-302, MZB-333, MZB-334), plus a `FightWatch` weak map of the last player hit on an
  Umvuthi (LivingDamageEvent.Post, a player by `Camps.killer`'s rule) for MZB-310.
- `RaptorMaker` (MZB-328, MZB-329), registered in `MicRtsimMowziesMobs` for `conflict/umvuthana/raptor`.
- `GroveChests` (MZB-322).
- `MowzieStrength implements IndustryPlugin` (MZB-335).
- Mixins in `mixin/`:
  - `@Mixin(EntityUmvuthi.class)`, with
    `@Inject(method = "rememberTrade", at = @At("HEAD")) private void mic_rtsim_mowziesmobs$paid(Player player, CallbackInfo ci)`.
    The same class has
    `@WrapOperation(method = "die", at = @At(value = "INVOKE", target = "<the getEntitiesNearby string above>")) private List<?> mic_rtsim_mowziesmobs$devoted(EntityUmvuthi self, Class<?> c, double a, double b, double cc, double r, Operation<List<?>> op)`,
    which returns `List.of()` for a second body and otherwise calls `op`.
  - `@Mixin(targets = "com.bobmowzie.mowziesmobs.server.inventory.ContainerUmvuthanaTrade$SlotResult")`,
    with `@Shadow @Final ContainerUmvuthanaTrade this$0;` and
    `@Inject(method = "onTake", at = @At(value = "INVOKE", target = "Lnet/minecraft/world/item/ItemStack;shrink(I)V")) private void mic_rtsim_mowziesmobs$traded(Player player, ItemStack stack, CallbackInfo ci)`.
  - `@Mixin(ContainerUmvuthanaTrade.class) interface ContainerUmvuthanaTradeAccessor` with
    `@Accessor("umvuthanaMinion") EntityUmvuthanaMinion mic_rtsim_mowziesmobs$minion();`.
  - All at the default priority 1000. Whether another pack mod mixes into these methods is UNVERIFIED;
    `defaultRequire 1` fails the boot loudly if one moves them.
- No bespoke entity anywhere (D3). Everything Mowzie is in the plugin (D4); core gets only generic hooks.
- UR-1: every body is the jar's own type, the memory is the jar's own field, and extras and owed hunts are
  never saved. The mixins only observe, or empty a list for a body the reconciler made and removes.
  Uninstalling leaves vanilla entities and items.
- D1: GroveChests looks only at groves that are in, every 200 ticks. OwedHunts runs only while it holds a
  command. The memory is at most 16 KiB a grove. No per-tick scan is added.

**GameTests** (`MowzieGameTests` unless noted; one batch per test, each on its own far base, MZB-247):

12. `aMaskedPlayerIsNotTargetedByTheBand`:
    - Setup: BAND_MATERIALIZE a raid (3 warriors, 1 raptor through the maker) 12 blocks from two fake
      players, one with an `UmvuthanaMask` on the head.
    - Assert, after 100 ticks: no body or follower targets the masked player, and some target the bare one.
    - The masked player strikes warrior 0. Assert that it targets that player within 20 ticks.
13. `theUmvuthisTradeIsReportedAsTributeAtTheCamp` (native engine):
    - Setup: a real grove; the census adopts its Umvuthi as the chief (ADOPT, LINE_CHIEF, CHIEF_SLOT).
    - A masked fake player opens its trade and pays 7 gold blocks through `MowzieKit.payUmvuthi`
      (`MessageUmvuthiTrade.handleServer`, with a test `IPayloadContext` that runs `enqueueWork` at once).
    - Assert one CHIEF_PAID: the chief's slot and generation, gold_block 7, flags 0. The engine books a
      63-coin gift (no demand is open).
    - A second payment: Sun's Blessing and no record.
    - A nugget trade with a grove minion: one TRADED with the trade's count.
    - An Umvuthi outside every grove: no record.
14. `anUmvuthanaEnvoyPartyIgnoresGolemsAndPlayersUntilStruckAndTakesPayment`: core's test 14 (M3) again,
    with the jar's minions as the envoy and its guards.
    - Assert that while the demand is open, no minion targets the unmasked fake player, whatever its own
      player goal says.
    - Paying opens no `ContainerUmvuthanaTrade`.
    - The rest is M3's extended clauses: ownerless projectile, golem and skeleton no damage, a player's
      arrow an insult, `/kill` withdraws.
15. `aChiefKilledAtTheCampIsReportedWithTheKiller`, the Umvuthi half (M6 does the Frostmaw's):
    - Setup: a real grove with its chief adopted.
    - A fake player kills it. Assert one CAMP_DEATH on CHIEF_SLOT with BY_PLAYER and that player. The
      engine books the Killing, `cowed_until`, P = 1 and the heir due at max(cowed, 12 days).
    - Again with a tamed wolf of the player as the killer: BY_PLAYER, the owner.
    - Again with the player's hit followed by a fall within 100 ticks: BY_PLAYER.
16. `tributeLandsInAGroveChestWhenTheGroveLoads` (native engine):
    - Setup: a real grove with no chest. Pay the Umvuthi as test 13 does. The engine owes the tribute.
    - Release the grove, place a chest in its footprint, and force it again.
    - Assert, in order: HAVEN_CHEST (27 free); TRANSFER_TO_CONTAINER; 567 gold nuggets in the chest;
      COMMAND_RESULT APPLIED; owed 0.
    - A full chest: nothing is sent until there is room.
17. `raptorsHuntTheBasesAnimalsAndAnOwedHuntIsAppliedOnLoad`:
    - Embodied: a raptor pack (the maker) beside cows and sheep at a test district kills them by the jar's
      AI.
    - Owed: a scripted OWED_HUNT (count 5, radius 48) on a far base that is not loaded, holding 4 cows, a
      sheep, a tamed wolf, a named cow and a leashed sheep. Force it. Assert:
      - Nothing dies before the chunks tick entities.
      - Then the 5 eligible animals die, nearest first, with their drops on the ground.
      - The tamed, named and leashed animals live.
      - HUNT_APPLIED (killed 5) is journaled with the op id.
      - The same op again kills nothing.
    - A second hunt of 10 with none left: HUNT_APPLIED, killed 0.
20. `eachMowzieBodysFightStrengthIsItsShare` (test 18's pattern, ConflictGameTests 986 to 1040):
    - Setup: each body in turn (Umvuthi, minion, raptor pack, and M6's Frostmaw) attacks the industry yard.
    - Assert that IND_FIGHT carries `Threats.strength` at 5.0, 0.4, 2.6 (summed over the pack) and 41.7,
      within 0.01.
25. `theTradeMemoryRoundTripsThroughTheEngine`, in three parts (MZB-341):
    - Real grove, native engine: the adopt, then a masked payment. Assert CHIEF_PAID with the chief's slot
      and generation, a settled demand or a gift, and no second record.
    - Scripted test camp: CHIEF_BODY with two players, then a chief SPAWN. Assert `hasTradedWith` is true
      for both, and that the write journals nothing.
    - Rust protocol tests: see below.
27. `killingTheUmvuthiKillsDevotedUmvuthanaWithin30Blocks`:
    - Setup: a test camp; ADOPT an Umvuthi as the chief, and warriors 10, 25 and 35 blocks off. A
      player-bound follower stands at 10 blocks.
    - The fake player kills the Umvuthi. Assert one CAMP_DEATH for the chief, and one Sol Visage on the
      ground.
    - The warriors at 10 and 25 die within 20 ticks, each a CAMP_DEATH with BY_NONE. The 25-block one is
      BY_PLAYER when the player hit it 40 ticks before.
    - The warrior at 35 and the follower live.
29. `theUmvuthisLinesWaitWhileAPlayerIsNear`, the first three clauses (MZB-340):
    - A TAKE and a REMOVE on a test camp's chief wait 6,100 ticks with the fake player 150 blocks off. With
      the player moved 170 blocks off and set as the Umvuthi's target each tick, they still wait. With
      neither, they apply the next tick.
    - A chief SPAWN waits for the player to leave, or for 6,000 ticks.
    - Out and in: every held line is STALE.
30. `anUmvuthiHeirTakesAWarrior`:
    - The scripted pair on a test camp with three warriors, one with a held TAKE. Assert the warrior removed
      is the farthest from the fake player and not the held one. The heir:
      - stands at the throne line's spot, in its direction;
      - was made by `finalizeSpawn(STRUCTURE)`;
      - is restricted to the pit;
      - has `getTradedPlayers()` empty.

      MASK is answered with the warrior's slot and generation, and SEAT_HEIR DONE.
    - With no warrior, both are STALE.
    - With FLAG_THRONE clear: the fire pit, direction 1.
    - Seated in place: L_A is the old generation, the old body is tagged, and its REMOVE is held by the
      player at 150 blocks. The heir appears where the old body stood in one tick, at full health with an
      empty memory, and the REMOVE is STALE.
    - The old body targets the player and was hit by it 100 ticks ago: the seat waits. 600 ticks after that
      hit, the seat is done.

**Rust tests:**
- `rtsim-conflict` unit tests:
  - The chief slot is never drawn or counted.
  - A grove found empty waits 192 days, and a stray cancels the wait.
  - A chief killed at the grove: P, the heir's due time, and no order while no warrior is home.
  - The pair's booking: DONE, NOTHING, STALE, a masked slot with a line in flight, and `drawable` 0.
  - Seating over a Dead slot.
  - The write-off.
  - The away death, Left and Gone.
  - The chief leads only an Invasion: 12.6, Body to Left, Owed to Gone, and no MEMBER_CHIEF.
  - Home: Left to Body; Gone to CHIEF_BODY, then SPAWN.
  - `gift_value`: 567 nuggets are 63.
  - TRADED is booked once by op.
  - CHIEF_PAID on a live slot, a dead slot and a stray.
  - The memory: its cap, its union with the census, and its clearing.
  - Hunts are owed by surviving raptors, capped, and nothing is owed for a watched raid.
  - Plunder of leather and wool only, eaten.
  - Placing: an amount, then APPLIED, DUPLICATE or FAILED.
  - Save and load with every new field.
- `rtsim-jni` protocol tests:
  - The CHIEF_PAID gold equivalents.
  - HAVEN_CHEST is latest-wins.
  - A restart after a payment and before any CHIEF_BODY keeps the memory (test 25).
  - The Invasion round trip (test 25): the chief Gone, home, CHIEF_BODY before SPAWN.
  - A dead slot's census and a payment to it add nothing, and the payment is still tribute (test 25).
  - A succession clears the memory (test 25).
  - An owed hunt is resent at the session start and booked once.
  - The tribute transfer through the outbox, across a restart.

**Sim runs** (`rtsim-cli sim mowzie`; `RUNS` and `main`'s dispatch in `crates/rtsim-cli/src/sim/mowzie.rs`):
- **Run 5, the rest** (`grove_ladder` grows), must show:
  - plunder takes only leather and wool, eaten;
  - an Invasion draws the chief and fights at 12.6;
  - the grove in and out through `CampWorld`: raids leave and come home, no census books a living survivor
    dead, and a refill while a band is out returns to the cap with each survivor's slot kept;
  - hunts are never over the cap;
  - gold gifts are worth nuggets / 9.
- **Run 6, the grove** (a new `grove_renewal`), must show:
  - an emptied grove refills to its cap of 9 at 2 a day, with the chief slot untouched;
  - a grove found without its Umvuthi seats its heir at 192 days, or at once when a stray turns up.
- **Run 10** (`camp_reconciler`) gains held lines and heirs in the emulator. After every event it checks
  that no grove holds two live chief bodies, that the chief is in no pool count, and that each MASK DONE
  has its SEAT_HEIR DONE in the same reconcile.
- **Run 12** (a new `heir_farmed`): an Umvuthi killed by a player as soon as its body stands, for 60 game
  years. It must show about 1.8 Sol Visages a game year at base 6 and about 1.0 at base 24, within 10%
  (PLACEHOLDER) of EQUILIBRIUM.md's model, with each wait `min(192, base × 2^P)`.

**Work split** (each sub-slice is committed and tested on its own):
- **M9a, engine:**
  1. Merge M4 into the M5 base.
  2. The culture fields.
  3. `CHIEF_SLOT` and its exclusions.
  4. `line_flags` and the throne.
  5. The census's chief stray and the found-empty heir.
  6. `chief_can_lead`, the draw, strength and the `chief_stack` gate.
  7. Home.
  8. The death at the grove, the write-off and the away half.
  9. `Ev::CampHeir`, `camp_heir` and the pair's booking.
  10. `gift_value`, TRADED, CHIEF_PAID, the memory and CHIEF_BODY.
  11. The chests, `stock_to_players` and `place_tribute`.
  12. Hunts and plunder.
  13. `boundary.rs` and `persist.rs` (RTC7), then the regenerated fixtures.
  14. The rtsim-jni arms.
  15. The Rust tests, and runs 5, 6, 10 and 12.

  **Opus review here**, focused on the books: the pool and audit sums with the chief slot, and the pair's
  booking under answers lost, late and replayed.
- **M9b, Java camp and chief:**
  1. The `ConflictRecords` layouts.
  2. The `CampKind` hooks.
  3. In `Camps`: the throne, the chief dispatch, the MASK and SEAT_HEIR pair, seating in place, the
     `holdRemoval` holds, `killer`, the second body, and the CHIEF_BODY store.
  4. In `GroveKind`: the chief body, `make`, `spawnAt`, `joined`, `holdRemoval`, `chiefBody`, the census's
     TRADED list, the gong, and `FightWatch`.
  5. The mixins and the accessor, in the shared config.
  6. `MowzieKit.payUmvuthi`.
  7. GameTests 15 (the Umvuthi half), 25 (the Java parts), 27, 29 (MZB-340) and 30.

  **Opus review here**, focused on Rule U and the second body: no line applies in a player's sight, and no
  path journals a second body's death.
- **M9c, bands and the world:**
  1. `registerMaker`, `Slot.extras` and `RaptorMaker`, with Peaceful.
  2. `OwedHunts` and the hunt records.
  3. `GroveChests` and HAVEN_CHEST, with the tribute's transfer.
  4. `MowzieStrength`.
  5. GameTests 12, 13, 14, 16, 17 and 20.

  **Opus review at the end**, of M9 whole against MOWZIE.md §4.5 and §4.9.

**Question for Ben** (the design follows the recommendation, MZB-342):
1. May an owed hunt (a raptor raid on a base nobody watched) kill tamed, named or leashed animals?
   Recommended: no. It takes unmarked stock only, so a named pet never dies off-screen to a raid the player
   could not see. Embodied raptors still do what the jar's AI does.

**Traps.**
- **Two death events.** `EntityUmvuthi.die` posts LivingDeathEvent twice. Key a death on the UUID, never on
  the event.
- **Direction.** `setDirection` is read only at tick 1: set it before `addFreshEntity`, never after.
- **Gold gifts.** An unpriced good makes `gift_value` 0. Gold gifts were worth nothing until MZB-316.
- **Latest-wins kinds.** `observe_state` drops them: HAVEN_CHEST and CHIEF_STATE need `observe_latest`.
- **Gold blocks.** gold_block and gold_ingot are not in the catalogue. Translate them in rtsim-jni, never
  in the engine.
- **The base.** M4 and M5 are separate branches, so merge M4 in first.
- **Answer flags.** CAMP_BODIES' answer lines carry no flags, and `send_lines` marks a slot in flight only
  when its generation is not 0. Know the chief by its slot id, and track MASK apart.
- **Raptor followers.** The raptor stand-in through `finalizeSpawn` leaks 4 or 5 untagged followers each.
  A raptor removed in its first tick kills its pack.
- **Raptor plunder.** With `carry` 0, plunder takes nothing unless `plunder_goods` is set.
- **Spawn heights.** `SpawnConfig` takes `heightMax` before `heightMin`: the Umvuthana's `-65, 60` is
  a minimum of 60.
- **Duplicate payers.** `rememberTrade` does not dedupe. A union with the census must dedupe.
- **The empty grove.** A grove found empty waits the full cap, 192 days, not 6.
- **Trade-memory keys.** The jar keys trade memory on the game profile id. A fake player's profile id is
  what counts, not its entity UUID.
- **M6's renewal.** M6's `renewal` validation needs `recruits_per_day == 0`, so the grove needs its own
  `chief_renewal` field.
- **Wheel tags.** M6's Arrival takes wheel tag 8; CampHeir is 9.
- **Peaceful.** M6's wording is false for raptors: they discard themselves.
- **Ids in M6's range.** M5 took MZB-290 to 292 from M6's range.
- **The minion's container.** `ContainerUmvuthanaTrade.umvuthanaMinion` is private, and `SlotResult` is a
  private inner class. Use an accessor and `this$0`. Keep the accessor out of a GameTest holder's
  signatures (nest it).

## M10 March

- A party whose surviving members are all Left (none stands to be handed back) gets no HAND_BACK line and so
  no recall toward its camp: the homecoming of MZB-211 starts from a HAND_BACK. M10 owns this case (found in
  the M5 review, P2-7); until then such a party stays where the engine left it.

## M11 Aloft

(none yet)

## M12 S9

(none yet)

## Hand-off

(none yet)
