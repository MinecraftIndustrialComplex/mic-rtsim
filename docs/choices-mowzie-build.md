# Mowzie build: choices

Choices made while building MOWZIE.md §9's slices, where the plan left something open. Ids are
MZB-#, and each slice takes ids from its own range so slices built side by side never collide:
M1 1 to 49, M2 50 to 99, M3 100 to 149, M4 150 to 199, M5 200 to 249, M6 250 to 299, M7 300 to 349,
M8 350 to 399, M9 400 to 449, M10 450 to 499, M11 500 to 549, M12 550 to 599. Each slice writes only
under its own heading.

## M1 Records

(none yet)

## M2 Cultures

- **MZB-50** (empty rungs, both ways): `Culture::has_rung`, `next_up` and `next_down` (ledger 2).
  A climb goes to the next rung up that has a party, or to Notice, at the threshold of the rung it
  goes to and after the dwell of the rung it leaves. Every step down (the calm in `feud_wake`, each
  of a fallen chief's two steps, an unmet demand's fall) goes on down past empty rungs, floored at
  Notice, so from Siege with Invasion and Raid empty a chief's fall lands on Envoy and a calm on
  Scouts. An unmet demand climbs to `next_up(Envoy)` (Scouts for every culture so far).
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
  Without `repeat_days` nothing is booked and the wake is today's.
- **MZB-52** (a pool wait for whole warriors): `escalate`'s wait is `ceil(want / 2)`, the whole
  warriors `send_party` needs. Without it a party of odd size (the Frostmaw's 1, the raiders'
  Invasion of 23) waited for half a warrior, failed to muster and woke its feud again at once. A
  test pins it: a Frostmaw's pool set to 0.6 at its climb to Raid wakes its feud fewer than 10
  times in the next 3 days (about 72,000 without the `ceil`). The wait's tick sum saturates. M2
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

## M3 ConflictBridge

(none yet)

## M4 Camp reconciler, engine

(none yet)

## M5 Camp reconciler, Java

(none yet)

## M6 Frostmaw

(none yet)

## M7 Rail

(none yet)

## M8 Lying in wait

(none yet)

## M9 Umvuthana

(none yet)

## M10 March

(none yet)

## M11 Aloft

(none yet)

## M12 S9

(none yet)

## Hand-off

(none yet)
