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

(none yet)

## M8 Lying in wait

(none yet)

## M9 Umvuthana

- **MZB-400** (ids and base): M9 takes MZB-400 to 449, the header's range (the design was first written as
  300 to 343 and renumbered by the lead). M5 has also written MZB-290 to 292, inside M6's range; M6 uses
  250 to 289. The design
  reads M4 at `feat/mowzie-m4` 2b239fe2 (the engine, as `target/m4-src`) and M5 at `feat/mowzie-m5`
  d5b45b22 (the Java). They are separate branches, so M9's branch merges both before anything else.
- **MZB-401** (the chief's slot): a culture with `chief_body: Option<String>` keeps its chief in one slot of
  id `CHIEF_SLOT` = `u32::MAX` (-1 as a Java int). Umvuthana's is `mic_rtsim:conflict/umvuthana/umvuthi`, a
  new body dictionary entry with stand-in `mowziesmobs:umvuthi`. The id sorts last, so `Camp::index`'s binary
  search holds. The slot is outside the pool. These all skip it: `home_count`, `away_count`, `drawable`, the
  draw order, `spawn_lines`, `party_lost`, the units, the pool and its refill, the audit sums (I1 to I6)
  and the stray count. The den is untouched: its Frostmaw is its one unit (M6), and frostmaw's `chief_body`
  is None.
- **MZB-402** (the Umvuthi is a census body; replaces MZB-220):
  - `GroveKind.isBody` takes `EntityUmvuthi` as well as the exact minion type, and `isChief` is true for it.
  - The census lists it with BODY_CHIEF, its slot and generation, as well as in the chief fields (CHIEF,
    THRONE, health, position, direction).
  - An untagged Umvuthi in the footprint is a stray only while a grove found without its Umvuthi waits for
    its heir (MZB-405). The engine then ADOPTs it with LINE_CHIEF. After a chief's death, a stray does not
    cut the heir's wait.
  - Any other untagged Umvuthi is left alone, as the jar's own. A tagged one of a retired generation is
    REMOVEd under Rule U (MZB-433).
  - Followers, cranes and raptors stay uncounted.
- **MZB-403** (per-slot line flags; the mask in flight):
  - Each line's flags come from `line_flags(f, slot)`: LINE_CHIEF for CHIEF_SLOT, and the den's crystal
    flag as M4 sets it.
  - CAMP_BODIES' answer lines carry no flags, so the engine knows the chief by its slot id alone.
  - A MASK line names no slot (MZB-408), and `send_lines` marks a slot in flight only when its generation
    is not 0. So the engine keeps the MASK in flight as `Camp.mask: Option<(u64, u32)>` (op, line), and
    counts it in `in_flight`.
- **MZB-404** (the throne):
  - The engine keeps the throne from the grove's first census with CENSUS_THRONE (M4's `Camp.throne`).
  - Every CAMP_RECONCILE to a grove sets FLAG_THRONE, with THRONE_X, Y and Z and DIRECTION.
  - Java keeps the latest on the site (`Camps.Site.throne`, for that epoch).
  - With none known (a grove found without its Umvuthi), FLAG_THRONE is clear. Java then uses the fire pit's
    centre (MZB-218, MZB-242) and direction 1, as §7 test 30 says.
- **MZB-405** (a grove found without its Umvuthi):
  - The grove gets no chief slot, and `Camp.heir = Some(Heir { due, found_empty: true })` at
    `Pressure::at_cap`, so its heir comes after the cap's wait of 192 days.
  - MOWZIE.md states no wait. EQUILIBRIUM.md §2.3 takes a camp found empty as one farmed to the cap.
  - An untagged Umvuthi seen in the footprint before then cancels the wait and is ADOPTed into a new chief
    slot.
  - M6 uses the same rule for a den found empty (its migrant), and whichever slice lands first writes
    `Pressure::at_cap`.
- **MZB-406** (a chief killed at the grove): this is CAMP_DEATH on the chief slot, while Body, or while Left
  with its take still held (MOWZIE.md ledger 5). In order:
  1. M6's `chief_killed_at_camp(w, f, killer, at)` runs: `ensure_chief`, the fall, only the killer's own
     feud steps down, and every party not Home routs.
  2. `Camp.pressure` steps up by 1, unless the killer kind is BY_MOB (another mob's kill is no player's
     pressure).
  3. The slot is freed with died true, and the trade memory is cleared.
  4. `heir_due = max(cowed_until, at + wait)` is set, and `Ev::CampHeir { faction, stamp }` is booked for
     then. It is wheel tag 9: M4's Refill is 7 and M6's Arrival is 8.
- **MZB-407** (the heir's wait): a culture field `chief_renewal: Option<RenewalDef>`. Umvuthana's is
  `{ base_days: 6, cap_days: 192, half_life_days: 96, boss: true }` (MZ-14 (a)). The wait is
  `min(cap, base × 2^P)` days, with P decaying at the half-life, as in M6's den renewal.
  - It reuses M6's `RenewalDef` type and resolution, but as a field of its own: M6 validates its `renewal`
    to need `recruits_per_day == 0` (a culture of one), and the grove recruits 2 a day.
  - Nothing is ordered while no warrior is home (Body with no line). The heir then waits, and the camp is
    looked at again at each census.
- **MZB-408** (the heir takes a warrior): when the heir is due, one CAMP_RECONCILE carries two lines.
  - The first is a new `ACTION_MASK` = 8: slot 0, generation 0, the warrior body. Java takes the camp's
    warrior farthest from every non-spectator player that has no line in flight or held.
  - The second is SEAT_HEIR: slot CHIEF_SLOT, a new generation, the umvuthi body and LINE_CHIEF. L_A is
    the old generation when the chief slot is Dead, else 0.
  - Java plays the two together: it makes the heir first, then removes the warrior (its record on a marker,
    as a take). If either cannot be done, both are STALE and nothing changes.
  - MASK is answered with the taken warrior's slot and generation.
- **MZB-409** (booking the heir):
  - MASK DONE frees that slot (died false) and calls `pool_shift(-1)`, since the warrior becomes the chief.
  - SEAT_HEIR DONE makes the chief slot Body, and clears `Camp.heir` and the trade memory. NOTHING does the
    same: the body already stands, tagged.
  - STALE on either line orders the pair again at the next census.
  - While the pair is in flight `camp_drawable` is 0, so no muster takes the warrior about to be masked.
  - A MASK answer can name a slot whose own line is still in flight. The slot is freed all the same, and
    that line's late answer finds a retired generation and is ignored.
  - The SEAT_HEIR spawn waits under the chief's spawn hold (MZB-433), and MASK waits with it.
- **MZB-410** (seated in place): this is SEAT_HEIR with L_A not 0, while the old generation's body stands
  tagged in the footprint.
  - Java waits while that body is in a fight: it has a player target, and a player hit it within 600
    ticks (PLACEHOLDER). The plugin keeps the hits in a weak map.
  - Then, in one tick, Java answers the old body's held lines STALE, removes it with no CAMP_DEATH, and
    makes the heir where it stood: at full health, in its direction, with an empty trade memory.
  - With the old body gone, it is a plain SEAT_HEIR at the throne.
  - In M9 only scripted lines reach it (test 30), since a chief slot is Dead only after a death away
    (MZB-412).
- **MZB-411** (a chief written off): a chief slot missed 3 times over 2 days (camp.rs's write-off rule) is
  booked as a death at the grove with BY_NONE.
  - P steps up (EQUILIBRIUM.md §2.2 counts a missing home body), the memory clears and the heir waits.
  - `chief_killed_at_camp` is not called: no one saw it die, so there is no report, no feud steps down and
    no party routs.
- **MZB-412** (a chief killed away, the engine half):
  - A Left chief slot becomes Dead. The census REMOVEs its throne body under Rule U, or the heir is seated
    over it (MZB-410).
  - A Gone chief slot is freed with died true.
  - Both step P up, clear the memory and set the heir.
  - Nothing sends a chief's death away before M11 (BAND_CASUALTIES with the chief, ledger 24), so only Rust
    tests reach it. (Amended at review, P3-1: this no longer holds. With `chief_leads` an Umvuthi leads
    Invasions from M9 on, and M3's Java already sends CHIEF_KILLED for a chief puppet, so M9b reaches it.
    `chief_killed_by` calls `chief_died_away` when the faction's chief slot is away with that party, after
    `chief_falls` booked the cowing, so the heir is due after it. The fall, the cowing and the band's rout
    are booked once, by `chief_killed_by`; `chief_died_away` adds only the slot and the heir, and the
    chief's pressure step, for a player's kill. The member renewal's bump (`renewal_bump`) is a boss
    culture's and a grove has none, so the pressure steps once. The band comes home without the chief: its
    slot is Dead with no party. A crash between the CHIEF_KILLED and the DEMATERIALIZED finds the slot
    Dead in the save, so the replay has no chief to bring home.)
- **MZB-413** (who leads, MOWZIE.md ledger 4): two culture fields.
  - `chief_leads: Vec<PartyKind>`: the default is raid, invasion and siege (today's "any war party").
    Umvuthana's is `["invasion"]`; frostmaw's is M6's.
  - `chief_strength: f32`: umvuthana's is 5.0.

  `chief_can_lead` holds when the chief slot exists, is Body or Owed, has no line in flight, and no heir
  waits. A draw for a party the chief leads takes the chief too: Body becomes Left, Owed becomes Gone.
  While it leads, `strength` and `strength_by_class` add `chief_strength` in the first unit's class, so a
  full Invasion (6 warriors, 2 raptors) is 6 × 0.4 + 2 × 2.6 + 5.0 = 12.6.
- **MZB-414** (no chief stack before M11):
  - BAND_CHIEF and MEMBER_CHIEF are written only when `chief_stack(c)` holds, which is false for a culture
    with `chief_body`. So MZB-109's stand-in chief never appears with a grove's band.
  - The chief's SET_OUT line finds no puppet to claim it, so it is a take (MZB-222), held under Rule U
    (MZB-433). The Umvuthi stays on its throne until no player is near, then goes.
  - M11 turns `chief_stack` on, with the aloft puppet and CHIEF_BODY's travel.
- **MZB-415** (coming home): a party with the chief ends Home.
  - A Left chief slot becomes Body.
  - A Gone one becomes Owed. The engine sends CHIEF_BODY (MZB-421), then a chief SPAWN, held as MZB-433
    says.
  - M9 sends no chief HAND_BACK, since there is no puppet.
  - A held take can apply after the party is home. Its late DONE makes the slot Owed, and a SPAWN follows.
    This is accepted: both wait until no player is near.
- **MZB-416** (gifts at the grove's price, the rest of ledger 12):
  - When the market has no price for the tribute good, `gift_value` prices it at `1 / tribute_per_coin`.
    Today it uses `m.price(g) / PRICE_ONE`, which is 0 for an unpriced good, so every gift of gold was
    worth nothing.
  - So the Umvuthi's 7 gold blocks, 567 nuggets, are 63 coins.
- **MZB-417** (TRADED):
  - A masked trade with a grove's minion is worth `nuggets / tribute_per_coin` coins. It is booked through
    `Conflict::traded`, once by op id.
  - Java writes it only for a minion tagged by a grove or standing in a grove's footprint. The count is
    the gold nuggets on either side of the trade, and a trade with no gold side writes nothing.
  - A trade with a band's puppet writes nothing.
- **MZB-418** (CHIEF_PAID):
  - It calls `pay_tribute(w, faction, payer, None, goods, at)`. That settles the payer's open demand, else
    the settlers', else it books a gift (MZB-416).
  - The payer goes into the trade memory only when the slot and generation are the live chief's (Body).
  - PAID_STRAY (flags 1) is a payment to an untagged Umvuthi standing in a grove's footprint. It is booked
    as tribute, with no memory entry.
  - A payment to an Umvuthi outside every grove writes nothing.
- **MZB-419** (gold blocks and ingots):
  - The Umvuthi wants gold blocks (the config's gold_block × 7), but the catalogue has only `gold_nugget`.
  - So rtsim-jni's CHIEF_PAID translation maps items through `TRIBUTE_EQUIVALENTS` (gold_block 81 nuggets,
    gold_ingot 9) before `own_items`, merging entries. The engine sees 567 gold_nugget.
  - TRIBUTE_PAID (M3's) is untouched.
- **MZB-420** (trade memory): `Camp.traded: Vec<(u64, u64)>`, sorted, of at most 1,024 players
  (PLACEHOLDER, the census's own cap).
  - Past the cap, a payment is still tribute but is not remembered.
  - It is fed by CHIEF_PAID, and by the census's traded list when TRADED_SLOT and TRADED_GENERATION are the
    live chief's. The two are unioned, so a payment the engine has and the body has not yet written stays.
  - It is saved. It is cleared when the chief dies or is written off, and when an heir is seated.
- **MZB-421** (CHIEF_BODY at home):
  - It holds agent 0, the camp's epoch, health 0 (the body keeps its own), flags 0 and the memory.
  - It is sent before every chief SPAWN, and whenever a census's traded list is shorter than the engine's.
    It is never sent before SEAT_HEIR, since an heir's memory is empty.
  - Java core keeps the latest for each faction for that epoch, and drops it at CAMP_OUT.
  - Java writes it through a new `CampKind.chiefBody(Mob, ChiefBody)` (a default no-op): when the chief's
    body is made, and at once onto a standing chief body of that slot and generation.
  - GroveKind writes it with `setTradedPlayersCompound` (a ListTag of `NbtUtils.createUUID`), never with
    `rememberTrade`, so nothing is journaled.
  - Its puppet and travel use (agent not 0) are M11's.
- **MZB-422** (HAVEN_CHEST):
  - rtsim-jni decodes it in a new `observe_latest` arm, shared with M6's CHIEF_STATE. Today `observe_state`
    drops latest-wins kinds.
  - The engine keeps the chests by faction, unsaved, and forgets one at CAMP_OUT.
  - The plugin's `GroveChests`, after Ice and Fire's `Lairs.java`, looks at each grove that is in every 200
    ticks (PLACEHOLDER). It picks the chest or barrel (`ChestBlockEntity`, `BarrelBlockEntity`) in the
    footprint nearest the centre that has room or a loot table not yet rolled.
  - It sends HAVEN_CHEST when the choice or its free slots change, and one with 0 free slots when there is
    none.
- **MZB-423** (tribute into the chest):
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
- **MZB-424** (owed hunts, ledger 11): a unit field `hunts: u32` (raptor 2, PLACEHOLDER) and a culture field
  `hunt_cap: u32` (8, PLACEHOLDER).
  - `raid_district` adds `survivors × hunts` for each class it did not skip, capped at `hunt_cap` per
    district.
  - A watched raid adds nothing, since there the jar's raptors hunt for themselves.
  - The hunts are kept in `Faction.hunts: Vec<Hunt { district, count, op }>`, a small Vec, never a one-entry
    BTreeMap.
- **MZB-425** (the hunt records; the ids are provisional):
  - OWED_HUNT (0x880C, engine to Java, latest-wins for each faction and district): faction, district,
    dimension, centre x and z, radius 48 (PLACEHOLDER), count and op id.
  - HUNT_APPLIED (0x0816, journaled): faction, district, op id and killed.
  - They are the next free ids after today's DIFFICULTY 0x0815 and BAND_MARCH 0x880B. Another slice may
    take them first, so they are settled at landing.
  - One hunt is in flight for each district. It is resent at each session start and booked once by op id.
    HUNT_APPLIED clears it whatever `killed` is.
- **MZB-426** (applying a hunt): core `conflict/OwedHunts.java`, vanilla only.
  - It keeps the latest command for each faction and district, unsaved.
  - Once the circle's chunks tick entities, it kills up to `count` animals in it, nearest the centre first.
    They must pass the jar's raptor test, rewritten in vanilla calls: no ATTACK_DAMAGE or less than 3, and
    0.1 < width² × height < 6 (`EntityUmvuthana.registerHuntingTargetGoals`).
  - It spares tamed, named and leashed animals (MZB-442), and kills with `kill()`, so their drops fall.
  - It journals HUNT_APPLIED even when it killed none (the hunt is spent), with the circle's entity chunks
    in M5's barrier.
  - A crash between the kills and the save can apply one hunt twice, bounded by the cap.
- **MZB-427** (plunder takes livestock goods, the rest of ledger 11):
  - A culture field `plunder_goods: Vec<String>`. Umvuthana's is `["leather", "wool"]`; the catalogue has no
    meat good.
  - When it is not empty, `raid_node`'s plunder takes those goods only, and its cap counts those goods
    only. It ignores the units' `carry`: raptors carry 0, so today their plunder takes nothing.
  - The goods are eaten: taken out of the node's stock through `take_stocks` and booked to no one.
  - Only witnesses learn of it first-hand (CL-D61); the rest learn by gossip.
- **MZB-428** (body makers and extras):
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
- **MZB-429** (raptors on Peaceful; extends MZB-234):
  - A raptor and its followers discard themselves on Peaceful (their own `tick`), so the maker returns null
    on Peaceful and the stack stays pending.
  - A pack that discards itself is a despawn, not a casualty.
- **MZB-430** (the mixins): in the plugin's `mic_rtsim_mowziesmobs.mixins.json`, shared with M6 (see *Shared
  with M6* below).
  - `EntityUmvuthiMixin` at `rememberTrade` HEAD sends CHIEF_PAID:
    - the items from `getDesires()`, which still names what `fulfillDesire` just took;
    - the player from its game profile id;
    - the slot and generation from the body's CampMember;
    - PAID_STRAY when it is untagged in a footprint, and nothing outside a grove.
  - `EntityUmvuthiMixin` also gives `die`'s `getEntitiesNearby` call an empty list for a second body
    (MZB-431).
  - `ContainerUmvuthanaTradeSlotResultMixin` at `onTake`'s `ItemStack.shrink` sends TRADED, server side
    only.
- **MZB-431** (a second body): a chief body whose REMOVE line is held, or whose member already died, is a
  second body.
  - Its LivingDropsEvent is cancelled.
  - Its death journals nothing; `Camps.onDeath` skips it.
  - Its `die` takes no warriors with it (MZB-430), and it is removed.
- **MZB-432** (the killer, shared with M6): core's `Camps.killer(dead, source)` takes the first of:
  1. the source's entity, when it is a player;
  2. the player owner of an `OwnableEntity` source;
  3. `dead.getKillCredit()`, when it is a player (vanilla keeps one 100 ticks);
  4. any other source entity, as BY_MOB;
  5. else BY_NONE.

  It applies to Camps only. M11 adopts it for BAND_CASUALTIES.
- **MZB-433** (Rule U's holds):
  - The new `CampKind.holdRemoval(level, site, body)` defaults to false. GroveKind returns true for an
    Umvuthi with a non-spectator player within 160 blocks or a player target.
  - TAKE, REMOVE and SET_OUT-as-take lines on such a body are held with no time limit, and an epoch's end
    answers them STALE.
  - A chief SPAWN or SEAT_HEIR waits on `holdSpawn`: a player within 160 blocks of the throne, for at most
    `HOLD_MAX` 6,000 ticks.
- **MZB-434** (GroveKind's chief; supersedes MZB-209 and MZB-202):
  - `chiefLines()` is true.
  - `make` for the umvuthi body:
    1. the constructor and `moveTo`;
    2. `setDirection` before `addFreshEntity`, since the jar reads it only at tick 1;
    3. `finalizeSpawn(STRUCTURE)` and `restrictTo(fire pit, -1)`, as worldgen does;
    4. CHIEF_BODY's memory.
  - `spawnAt` uses the site's throne (MZB-404), else the fire pit.
  - `joined` restricts it again.
  - The census writes TRADED t, TRADED_SLOT and TRADED_GENERATION from the chief body's
    `getTradedPlayers()`.
  - `sound()` returns `mowziesmobs:block.gong`, the grove's notice sound.
- **MZB-435** (strength in observed fights, ledger 19, shared with M6): the plugin registers one
  IndustryPlugin whose `fightStrength` gives:

  | Body | Strength |
  |---|---|
  | Frostmaw (M6's) | 41.7 |
  | Umvuthi | 5.0 |
  | Any other `EntityUmvuthana` | 0.4 |
  | Raptor | 1.0 |

  So a pack of a raptor and 4 followers is 2.6.
- **MZB-436** (the envoy):
  - Its body is a grove minion. M3's guard is unchanged: it strips the jar's player and hurt-by goals while
    the demand is open, and lifts them at an insult.
  - The payment is taken at EntityInteract HIGHEST (MZB-110) and the event is cancelled, so the jar's trade
    screen does not open.
- **MZB-437** (the page format):
  - RTC7 and META 6, shared with M6. The second slice to land bumps again.
  - Camp gains `pressure` (M6's), `heir`, `traded`, `owed_tribute`, `placing` and `mask`.
  - Faction gains `hunts`, and the wheel gains `Ev::CampHeir`.
  - Fixtures are regenerated, with no migration.
- **MZB-438** (rtsim-jni):
  - CHIEF_PAID, TRADED and HAVEN_CHEST leave boundary.md §9.6's deferred list.
  - CHIEF_BODY and OWED_HUNT get writers, and HUNT_APPLIED gets a decoder.
  - The tribute's TRANSFER_TO_CONTAINER rides the existing `Outbox<TransferCmd>`, completed by
    COMMAND_RESULT.
- **MZB-439** (where the GameTests' camps come from):
  - Tests 13, 15, 16 and 25's payment half declare their grove under `mowziesmobs:umvuthana_grove` at a far
    base, so the native engine claims it as umvuthana's.
  - Tests 27 and 30, and test 29's throne clauses, use scripted test camps (MZB-227).
  - A GameTest cannot restart the session (MZB-228), so every restart case goes to the Rust protocol tests.
- **MZB-440** (test 29's throne clauses are built in M9): its first three clauses are built in M9:
  - the take and the removal held past 6,000 ticks;
  - the spawn held for the player or 6,000 ticks;
  - an epoch's end answering them STALE.

  M11 adds the rest (the puppet, re-binding and the four exceptions). Its second-body clause needs a
  puppet, so M11 builds it, on M9's MZB-431.
- **MZB-441** (test 25 in three parts):
  - a GameTest on a real grove for the payment;
  - a scripted test camp for CHIEF_BODY's write;
  - Rust protocol tests for the restart, the Invasion round trip, a dead slot's census and payment, and a
    succession.

  The puppet clauses are M11's.
- **MZB-442** (an owed hunt spares tamed, named and leashed animals; the question for Ben, decided on the
  recommendation): an owed hunt kills only stock that nobody has marked as theirs. Embodied raptors do what
  the jar's AI does: its damage test already spares wolves (ATTACK_DAMAGE 4), but not a named cow.
- **MZB-443** (the seams M9 leaves):
  - M10 owns BAND_MARCH, the column and the walk-in. M9 touches none of them: a grove's band, with or
    without its chief, marches as M10 makes any band march.
  - M11 owns the aloft movement, CHIEF_BODY's travel (agent not 0, ledger 24), `chief_stack` for the grove
    (its stack and puppet, MEMBER_CHIEF), the chief's HAND_BACK, the summons (a mixin at
    `EntityUmvuthi.java:1050` adding to MZB-428's extras), the chief's death away through BAND_CASUALTIES
    (MZB-412's engine half) and test 29's other clauses.
- **MZB-444** (the culture fields' checks, M9a; fills a gap in "Validation" of the design): `Culture::resolve`
  refuses each of these on its own.
  - `chief_body` empty, or equal to a unit's body: a chief's body kind must be one no unit has, so a census
    never reads one as the other. It needs `camp_bodies`.
  - `chief_renewal`, `chief_leads` or a non-zero `chief_strength` with no `chief_body`.
  - `chief_strength` not finite or below 0.
  - `chief_leads` naming an unknown party, a kind other than raid, invasion and siege (scouts, envoy and
    plunder have no chief), or one twice. An empty list is allowed: the chief leads nothing.
  - `plunder_goods` with an empty name or a name twice; a unit with `hunts` and a `hunt_cap` of 0.
  - Whether each `plunder_goods` name is in the catalogue cannot be known at resolve time. A test checks the
    built-in cultures against the default catalogue, and the engine skips a name the catalogue lacks.
  - `body_names` appends a culture's `chief_body` after its units, so every existing body id keeps its
    number (the Umvuthi's is the last).

- **MZB-445** (the heir's pair, M9a; design gaps):
  - The heir's generation is derived from the faction key, the chief slot and the tick the heir is due, and
    never the generation the slot last held (`Camp::heir_generation`), so a census can tell the heir from
    an answer to a lost reconcile made over an old chief's body.
  - The pair is tried at the heir's due event, at each census and at each `unblock` (`camp_heir`), never at a
    `camp_sync`, so a camp that is out or has no warrior home orders nothing and is looked at again.
    (Amended at review, P2-1: "at each census only" left a grove that stayed in with no heir for good when
    its due tick found no warrior home or its pair was answered stale. `unblock` ends with `camp_heir`,
    which returns at once unless the heir is due, the camp is in and no pair is in flight.) A pair answered
    so that no one is seated is not ordered again by that very answer, which would be one pair a tick while
    Java answers STALE: the answer books an `Ev::CampHeir` one game hour on (`HEIR_RETRY`, PLACEHOLDER)
    under the heir's stamp, and its `unblock` does not look at the heir.
  - A camp found without its Umvuthi whose wait an untagged one cancels resets its pressure to none: the
    cap's pressure was a stand-in, as if the camp had never been found empty.
  - A MASK answered for a warrior the books have `Left` is booked as a take: the slot goes `Gone` and keeps
    its place in the party.
  - An unanswered pair is void at CAMP_OUT and at a session start with the other lines.
  - `seated` seats only the heir that waits (amended at review, P3-1): an answer whose generation is not
    `heir_generation` while an heir waits changes nothing. A holder's copy of an old SEAT_HEIR, booked by
    state after its answer is forgotten, raised a chief that had died away (its Dead slot freed, its
    generation seated again, the waiting heir cancelled) and, reaching a live chief of a newer generation,
    cleared its trade memory and moved the heir's stamp. The generation of an heir whose death the books
    have is never the next heir's, which derives from a later due tick.
  - The heir killed before its seat is answered (found by run 10's grove chaos): a CAMP_DEATH on
    `CHIEF_SLOT` whose generation is the heir's, while the books hold no chief slot of that generation,
    seats the heir first (`seat_before_death`) and then books its death as any
    chief's: the pressure steps for a player's kill, the heir is due again, the memory is cleared. The answer
    that comes late, from a holder's record at the next camp-in, finds the generation retired at a death and
    seats no one (`seated`). Without it the death booked nothing and the late answer seated a chief with no
    body, written off after three misses with its fall never booked. A generation retired by a write-off is
    untouched. (Amended at review: the first version asked the SEAT_HEIR line to be in flight, which a
    CAMP_OUT or a session start voids, and run 10's chaos found the same ghost with the camp out when the
    heir was killed. A body tagged with the waiting heir's generation exists only if the world did the seat,
    whether or not the line is still held here.)
  - The emulator plays the pair whole (`synth`, a gap): `FakeConflict` does both lines of a reconcile that
    carries a MASK, or neither (`pair_possible`): a warrior that is tagged, not the chief, not elsewhere and
    not near a player, and no body of the seat's slot and generation standing already. If either cannot be
    done both are answered STALE, as MZB-408 has Java do. Before this the emulator could take a warrior and
    not seat the heir, which the engine's invariant (every MASK done with its SEAT_HEIR done) rightly called
    a bug of the emulator.
  - The sim's grove checks, where the design leaves the detail open. Run 5 (`grove_rest`): plunder (livestock
    only, eaten, no village defence; 8 raids ate 240 items), an Invasion led by the chief (fights of strength
    3.8 and 12.6), the grove in and out (6 rounds, its own plunder drive quiet), hunts owed to the cap (2, 4,
    6, 8, 8, 8, 8, 8) and the relief of a gift in nuggets equal to the same value in emeralds and to a trade
    at the tenth. Run 6 (`grove_renewal`): the grove's wait, pressure and farmed rate from the culture's own
    rule. Run 10 (`chief_chaos`): 32 groves of 220 events beside M4's 64-camp chaos, an Invasion ordered one
    time in eight, the pair, the pool counts, the one live chief body and the write-offs checked after every
    event; where holders journal again no member is written off. Run 12 (`heir_farmed`): the Umvuthi killed a
    few ticks after its seat for 60 game years (96 days each): base 6 gives 1.822 a year against the closed
    form's 1.810 and the doc's 1.8, base 24 gives 1.003 against 1.000, every wait the kernel's own
    `min(192, base x 2^P)` on its day.
- **MZB-446** (the tribute stack, M9a; settles MZB-423's open edges):
  - One op is one stack: at most 64 items, the least of what is owed, the stock and 64. Core's
    TRANSFER_TO_CONTAINER fails above an item's own stack size, so only the tribute good is owed (the gold
    nugget stacks to 64), never a gift's other goods.
  - `Placing` carries the chest (Java's dimension and position) as well as the good and amount, so a
    transfer rewritten after a restart knows where it goes. The stacks in flight are `Conflict.placing`, a
    resident list of (faction, `Placing`) saved in the meta page, never a field of a paged row.
  - The op id is `identity_op(faction key, TRIBUTE_SLOT | stacks)` over a counter saved in the camp, so the
    same decision after a crash has the same id.
  - Each stack takes one free slot off the chest's count until HAVEN_CHEST says otherwise; FAILED forgets
    the chest; CAMP_OUT and a session start clear it.
  - The engine books the goods as left to the players (`World::stock_to_players`, `player_out`) when it
    sends them, and back (`stock_from_players`) on FAILED.
  - Amended at review, P3-2: the stock is read after the world is caught up to the tick of the decision
    (`run_until`), as `stock_to_players` does, and a take short of what was read places the whole items it
    got and puts the rest back with `stock_from_players`. The grove's stock of the tribute good draws about
    two a day in the test world, so a stack read at the world's older clock was taken short and left the
    books with no stack to show for it.
- **MZB-447** (tribute pricing, M9a): `gift_value` prices only the faction's tribute good and an unpriced
  tribute good is worth `1 / tribute_per_coin` of a coin, so 567 nuggets are 63 coins at the fallback 9.0. TRADED's
  coins are its nuggets over `tribute_per_coin`. Any other unpriced good is still worth nothing.
- **MZB-448** (the page numbers, M9a): the M9 save is RTC8 and meta page 7, with wheel tag 9 for `CampHeir`,
  kept in `persist.rs`'s two constants, `MAGIC` and `META_VERSION`, so that a renumber (M7's rail may take
  RTC8 first) is a two-line change. There is no migration: the mods are unreleased.
- **MZB-449** (hunts, M9a; settles MZB-424 to MZB-427's open edges):
  - A hunt is `Hunt { district, pos, count, op }` in `Faction.hunts`. The op is the 0x43 allocator byte over
    `identity_op(key, HUNT_SLOT ^ hash(district, tick))`.
  - A raid on a district that already has a hunt adds its count to the old one, capped at `hunt_cap`, with a
    new op; the plugin keeps the latest command for the district, so the old one never applies after the
    new one is read. A late HUNT_APPLIED for a replaced op is spent (its op id is booked) and clears
    nothing.
  - Nothing is owed on Peaceful, by a culture with no `hunt_cap`, or by a raid the infrastructure skipped.
    A hunt already owed when Peaceful begins stays owed.
  - A replacement's op folds in the op it replaces (`owe_hunt` hashes district, tick and the held op), so
    a replacement in the very tick of the first gets a new op all the same.
  - The hunts go again with the notices, after the replay has caught up (`resend_notices`), and not in
    `session_start`: a HUNT_APPLIED journaled last session must clear its hunt before the plugin hears of it,
    or the animals die twice.
  - The hunt's centre is the party's target position (the district's centre) and its radius 48
    (PLACEHOLDER).
  - Plunder with `plunder_goods` takes those goods and no others, and what the survivors take is the whole of
    their reach (the goods' surplus over target times the culture's loot share), not their `carry`; no one
    is left alive, nothing is taken. The goods are eaten: `take_stocks`, `ledger.destroyed`, whatever the
    culture's `loot_destroyed`.

### M9 design for the implementer

**Where it stands.** Designed only; no code. Branch `design/mowzie-m9` (this doc). The implementation
branch starts from `feat/mowzie-m5` with `feat/mowzie-m4` merged in (MZB-400). Compile as M5 does, in this
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
   their followers discard themselves (MZB-429).
2. **The raptor stand-in.** MZB-216 makes it through `finalizeSpawn`, which adds 4 or 5 untagged followers
   per raptor, and they leak (MZB-428).
3. **Pack size.** §4.13's 2.6 for a pack fits 4 followers; the jar makes 4 or 5. The maker makes 4.
4. **The trade event.** Ledger 18 says "an event on minion trades". There is none, so a mixin is used at
   `SlotResult.onTake`.
5. **The set-out.** §4.5 hands the Umvuthi's slot at its set-out "to its puppet". There is no puppet before
   M11, so it is a held take (MZB-414).
6. **A grove found empty.** No wait is stated for a grove found without its Umvuthi. MZB-405 takes
   EQUILIBRIUM.md's cap.
7. **The census body.** MZB-220 kept the Umvuthi out of the census bodies, and MZB-402 replaces it.
   MZB-202 and MZB-209 are superseded by MZB-420 and MZB-434.
8. **The id range.** See MZB-400.

**What M5 must provide** (it is built; keep these as they are): `Camps.onDeath`'s `putIfAbsent` by UUID
(test 27 counts on it), `holdSpawn`, `HOLD_MAX`, `playerNear`, `playable` (MZB-233), the census writer's
chief fields and traded fields, and `MowzieCamps.groveCentre` (MZB-242). M9 changes `GroveKind` (MZB-434),
the line dispatch's `chiefLines` gate and the raptor stand-in (MZB-428).

**Shared with M6** (each is built once, by whichever slice lands first; the other uses it):
- `Camp.pressure: Pressure` with `Pressure::at_cap`, and the found-empty rule (MZB-405).
- The `RenewalDef` type and its resolution (MZB-407 adds its own field).
- `chief_killed_at_camp` (MZB-406).
- The mixin config `mic_rtsim_mowziesmobs.mixins.json`: required true, minVersion 0.8, JAVA_21,
  defaultRequire 1, no refmap. M6 adds `EntityFrostmawMixin`; M9 adds `EntityUmvuthiMixin`,
  `ContainerUmvuthanaTradeSlotResultMixin` and `ContainerUmvuthanaTradeAccessor`.
- rtsim-jni's `observe_latest` arm (CHIEF_STATE and HAVEN_CHEST).
- `Camps.killer` (MZB-432).
- The one `fightStrength` plugin (MZB-435).
- The page bump (MZB-437).
- The wheel tags: Arrival 8, CampHeir 9.

Two fixes for M6:
- its draft's Peaceful wording (item 1 above);
- its `renewal` validation, which must not be read as the grove's (MZB-407).

**Engine** (rtsim-conflict, rtsim-jni):
- `culture.rs`:
  - `CultureDef` and `Culture` gain `chief_body`, `chief_renewal`, `chief_leads`, `chief_strength`,
    `plunder_goods` and `hunt_cap`; `UnitDef` gains `hunts`.
  - Validation: `chief_body` names a dictionary body, `chief_renewal` needs `camp_bodies`, and every
    `plunder_goods` entry is a catalogue good.
  - `data/cultures/umvuthana.json`: chief_body umvuthi, chief_renewal {6, 192, 96, true}, chief_leads
    [invasion], chief_strength 5.0, plunder_goods [leather, wool], hunt_cap 8, raptor hunts 2.
- `engine/camp.rs`:
  - `CHIEF_SLOT`, the exclusions (MZB-401), `Camp` fields `heir: Option<Heir>`,
    `traded: Vec<(u64, u64)>`, `owed_tribute: Vec<(u32, i64)>` (good, milli), `placing: Option<Placing>`
    and `mask: Option<(u64, u32)>`.
  - `line_flags(f, slot)` (MZB-403).
  - The census: the chief stray (MZB-402, MZB-405), the traded union (MZB-420), CHIEF_BODY when short
    (MZB-421), and the chief's write-off (MZB-411).
  - `chief_can_lead`, and `camp_draw` taking the chief (MZB-413).
  - `camp_heir(w, f, at)`, which sends the pair (MZB-408) or waits. The booking of ACTION_MASK and
    SEAT_HEIR answers (MZB-409), and of a Dead chief slot (MZB-412).
- `engine.rs`:
  - `strength` and `strength_by_class` (3098, 3120) add the leading chief.
  - The `chief_stack(c)` gate at MEMBER_CHIEF (3976) and BAND_CHIEF (4022).
  - The party's end at Home for the chief (MZB-415).
  - CAMP_DEATH on CHIEF_SLOT runs MZB-406.
  - `gift_value` (2508), MZB-416.
  - TRADED to `traded` (2477), MZB-417; CHIEF_PAID to `pay_tribute` (2404), MZB-418.
  - `owed_tribute` and `place_tribute(w, f, at)`, MZB-423.
  - `raid_district` (3453) owes hunts and `raid_node` (3579) plunders, MZB-424 and MZB-427.
  - `send_hunts(at)` at session start and on a new hunt; `hunt_applied`.
- `rtsim-world`: `World::stock_to_players(i, goods, at)`, the mirror of `stock_from_players`.
- `boundary.rs`: `ACTION_MASK` = 8, OWED_HUNT and HUNT_APPLIED (MZB-425), and the CHIEF_BODY writer. The
  decoders of CHIEF_PAID, TRADED and HAVEN_CHEST are wired into `Conflict::observe`.
- `persist.rs`: RTC7 and META 6 (MZB-437), `Ev::CampHeir` on tag 9.
- rtsim-jni:
  - `TRIBUTE_EQUIVALENTS` in the CHIEF_PAID translation (MZB-419).
  - `observe_latest` (MZB-422).
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
  - FLAG_THRONE read into `Site.throne` (MZB-404).
  - The dispatch plays chief lines when `chiefLines()` holds.
  - ACTION_MASK with SEAT_HEIR as one unit, and the seating in place (MZB-408, MZB-410).
  - `holdRemoval` holds with no limit, answered STALE at the epoch's end (MZB-433).
  - The CHIEF_BODY store and its write (MZB-421).
  - `killer(dead, source)` (MZB-432), replacing `byKind(source.getEntity())` in `onDeath` (1203).
  - The second body: a weak set, a LivingDropsEvent cancel at LOWEST, and a skip in `onDeath` (MZB-431).
- `conflict/ConflictBodies`: `registerMaker` and `Maker`. `ConflictBridge`: `Slot.extras`, the maker at
  1145, and the extras' despawn on recall, dematerialize and failure (MZB-428).
- `conflict/OwedHunts` (MZB-426).

**Java, plugin** (`neoforge/mowziesmobs/.../mic_rtsim_mowziesmobs/`):
- `GroveKind` (MZB-402, MZB-433, MZB-434), plus a `FightWatch` weak map of the last player hit on an
  Umvuthi (LivingDamageEvent.Post, a player by `Camps.killer`'s rule) for MZB-410.
- `RaptorMaker` (MZB-428, MZB-429), registered in `MicRtsimMowziesMobs` for `conflict/umvuthana/raptor`.
- `GroveChests` (MZB-422).
- `MowzieStrength implements IndustryPlugin` (MZB-435).
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
25. `theTradeMemoryRoundTripsThroughTheEngine`, in three parts (MZB-441):
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
29. `theUmvuthisLinesWaitWhileAPlayerIsNear`, the first three clauses (MZB-440):
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
  7. GameTests 15 (the Umvuthi half), 25 (the Java parts), 27, 29 (MZB-440) and 30.

  **Opus review here**, focused on Rule U and the second body: no line applies in a player's sight, and no
  path journals a second body's death.
- **M9c, bands and the world:**
  1. `registerMaker`, `Slot.extras` and `RaptorMaker`, with Peaceful.
  2. `OwedHunts` and the hunt records.
  3. `GroveChests` and HAVEN_CHEST, with the tribute's transfer.
  4. `MowzieStrength`.
  5. GameTests 12, 13, 14, 16, 17 and 20.

  **Opus review at the end**, of M9 whole against MOWZIE.md §4.5 and §4.9.

**Question for Ben** (the design follows the recommendation, MZB-442):
1. May an owed hunt (a raptor raid on a base nobody watched) kill tamed, named or leashed animals?
   Recommended: no. It takes unmarked stock only, so a named pet never dies off-screen to a raid the player
   could not see. Embodied raptors still do what the jar's AI does.

**Traps.**
- **Two death events.** `EntityUmvuthi.die` posts LivingDeathEvent twice. Key a death on the UUID, never on
  the event.
- **Direction.** `setDirection` is read only at tick 1: set it before `addFreshEntity`, never after.
- **Gold gifts.** An unpriced good makes `gift_value` 0. Gold gifts were worth nothing until MZB-416.
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
