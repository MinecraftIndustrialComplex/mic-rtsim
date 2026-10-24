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
  - Not tested: the Peaceful check ordering in `playHeld` (the Peaceful tests answer a SPAWN that is not held)
    and the chief's exclusion (it needs a second den's Frostmaw standing in the first's footprint).
  - Under C2ME the poll sees a camp's levels at most `POLL_PERIOD` (20) ticks late (MZB-290), so an unload
    and a reload of the same chunks inside one period is never seen: the levels read the same before and
    after, no CAMP_OUT and no new epoch. This is benign: the bodies the unload hid or removed are indexed
    again by their own join events (MZB-590), a body that left with its chunk comes back with it, and the
    census that would have followed has nothing a later one does not carry. Not tested (a test would race the
    period).

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
- **MZB-272** (the synth, the emulated plugin): `kill_chief` takes one unit off the chief's stack in
  `Body.members` (the first stack's body), so the dead chief no longer comes back as a survivor; `camp_kill`
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
- **MZB-360** (placement): a stack of a lurking agent's BAND_MATERIALIZE is placed at the spot, not on the
  arc.
  - Its y comes from a scan from track y + 4 down to track y - 8 for a block with a sturdy top face
    (`isFaceSturdy(level, pos, Direction.UP)`) and the body's box free above it
    (`level.noCollision(type.getSpawnAABB(x, y, z))`). If none, `getHeight(MOTION_BLOCKING_NO_LEAVES, x,
    z)`.
  - Then `restrictTo(spot, 2)` and the hold (MZB-361). A puppet already live when AMBUSH_SPOT comes is held
    at once and walks to the spot.
- **MZB-361** (Lurker): a new core interface, registered per entity type id with
  `RtsimApi.registerLurker(String entityType, Lurker l)` (an id string, as `BodyKinds.registerStandIn`
  takes, so it can be called in the plugin's constructor before the registries freeze). The core looks
  it up by `BuiltInRegistries.ENTITY_TYPE.getKey(mob.getType())`:
  ```java
  public interface Lurker {
      void hold(Mob mob, Vec3 spot);      // each server tick while it lurks
      boolean strike(Mob mob, Vec3 at);   // start a strike at a carriage point; false if busy
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
    `AnimationHandler.INSTANCE.sendAnimationMessage(frostmaw, EntityFrostmaw.SWIPE_ANIMATION)`.
  - `release`: `setAlwaysActive(false)` and `clearRestriction()`.
  - `roar`: in `NO_ANIMATION`, `sendAnimationMessage(frostmaw, EntityFrostmaw.ROAR_ANIMATION)`. The jar
    plays its own roar sound in that animation.
  - Calling the jar's own animation is what MOWZIE.md §4.4 asks; the hold is new because the restriction
    does not hold an awake Frostmaw with a target (see the jar facts).
- **MZB-362** (the target): the restriction gates targeting (M6's MZB-279), so a restricted Frostmaw never
  picks a player up itself. Every 20 ticks of the hold, a puppet with no target gets
  `level.getNearestPlayer(TargetingConditions.forCombat().range(50), mob)` (its follow range).
- **MZB-363** (TRAIN_PASSED from the sampler): at each sample, for each lurk in the train's dimension, the
  segment's distance to the track point at or within `reach` sends TRAIN_PASSED.
  - It is sent once per approach, and armed again when the train is beyond 64 blocks of the point.
  - `RtsimApi.trainPassed(long agent, int epoch, boolean watched, int carriage, long train)` writes it with
    `beginJournaled`, carrying the table's epoch.
- **MZB-364** (watched): `PASSED_WATCHED` when `lurkBody(agent)` is live. `carriage` is the index of the
  carriage whose leading point is nearest the track point, computed only when sending.
- **MZB-365** (near a puppet): a train whose last sample is within 64 blocks of a live lurk puppet joins a
  near set, rebuilt at each of its 20-tick samples, and is read every tick.
  - Each carriage's leading and trailing points are read. A point within 6.5 blocks (horizontal) of the
    puppet calls `RtsimApi.strike(agent, point)` once per carriage per approach.
  - The swipe moves no cargo: the cargo comes from the TRAIN_PASSED roll. It is real for riders, since the
    jar's area attack strikes living entities in its arc.
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
- **MZB-367** (the den's sound): `DenKind.sound()` returns `MMSounds.ENTITY_FROSTMAW_ROAR`. So a den's
  notices roar too, instead of the raid horn: both are in-world sounds.
- **MZB-368** (the puppet roars): on TRAIN_ROBBED, a live lurk puppet of that faction is told `roar`.
- **MZB-369** (UR-1 and D1): puppets are never saved (M3), and the lurk table and the pending roars are per
  session, so nothing stays in the world. The sampler's lurk work is a list scan per sample over the lurks
  (at most one per lurking party). The every-tick reads are for trains within 64 blocks of a live puppet
  only, and the hold is one call per live puppet a tick.
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
    - **The legs.** The detail line says what the trains made: 11.9 legs a day on the 12-a-day line and
      23.9 on the 24-a-day line, so the two trains with a 600-tick dwell do keep the named rate (a
      line's lap is well under 1,800 ticks in the rig's scene).
    - **The three counts of a robbery.** The engine's `stats.lurk_robberies`, the TRAIN_ROBBED commands the
      rig heard (the train and the tick of each) and the industry's own `books.stolen` delta for each pass
      must agree pass for pass: the same trains at the same ticks, and the items the engine counted for a
      pass equal the ones the industry booked. (The check was the engine's counter against the rig's count
      of the engine's own command, one site: it could not disagree.)
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
      killed (`chief_killed`, as run 7 does). The save or restart is a day later, inside the 6-day cowed time,
      right after a Killing signal of twice the Raid rung's threshold provokes the first den's feud again.
      The control must be cowed at the trip, and the second den must begin a lurk after it (the tail that is
      compared). The first den stays empty for the rest of the run: a Frostmaw den has no recruits
      (`recruits_per_day` 0) and its next body is the renewal wait's (6 days a first kill, 12 for a
      player's), which runs on the camp plugin's ledger that this rig has no plugin for (run 6 has it). So
      the first den's feud climbs, waits out the cowed time and then blocks on the empty pool. That is a
      limit of the rig, not of the engine. (Without the provocation the first den's feud sits at Notice for
      the 56 days after, flat at its grievance: the meter's pressure alone does not climb back to Raid.)
    - **Decay**: the line is cut from 12 to 4 legs a day on day 10; the point is the first tick from day 11
      with a held pass and a booked rail event.
    - **The tapes.** Each arm leaves the same tape: the commands in wire order, the stats, the hourly
      grievance by bits, the lurk table's own commands (`lurk_wire`) and the state hash. Blob and Paged
      must equal the control's tape whole. The restart arm is compared by its lurk stream, its grievances and
      the lurk and pass stats (the command stream differs only by the one resent spot per lurking party,
      which is checked as `resent == lurking`).
- **MZB-373** (what M8 needs):
  - From M7: `Target::Track` and the arrival's Track arm, the sampler with its per-train state, and
    RAIL_PASS live.
  - From M6: `frostmaw.json`, the chief paths (a lurking chief can fall), and the plugin's mixin config.
  - From M3 and M5: the puppet machinery (BAND_MATERIALIZE, `Party`, the puppets never saved),
    `CampKind.sound()`, `DenKind`, and `FakePlayers.hearing`.
  - M6 adds no `alwaysActive` and no lurk restriction, so M8 adds `Lurker`.
- **MZB-374** (boundary.md): TRAIN_PASSED is live (the "come later" list, boundary.md:1752). AMBUSH_SPOT's
  spot y is the track's, with Java finding the floor. TRAIN_ROBBED's roar is a sound only.

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
- **Run 8, save and restore** (built at step 4; MZB-372): the three lurk points, each blob and paged,
  against the control; then the session restarts at each. The fourth, a reconcile in flight, is M4's.

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
