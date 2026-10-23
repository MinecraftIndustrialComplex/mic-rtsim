# Equilibrium and renewal: worlds that settle, and kinds that come back

**Status:** design for Ben's review, 2026-10-21. Nothing is built and no engine code changes here.
Ben approves this design before code. Ben answered EQ-1 to EQ-6 on 2026-10-21 (§5.1): EQ-1 to EQ-3 and EQ-6
as recommended, EQ-4 (b), and EQ-5 rejected in favour of `CONFLICT-LIFE.md`. Branch `plan/equilibrium`, based on main `f20234e`.

**Ben's request (2026-10-21):** "Ensuring worlds actually reach a nash equilibrium between tribes /
animals, etc. It would also be nice if we had abilities for mobs to spawn back in. I.E. If the player
kills off all of the Yeti's that currently exist, having a way for new ones to spawn. Same with
dragons, villages, etc."

**Binding here:** D1 (judge only by late-game runtime cost on a weak PC: closed forms, scheduled
events, per-site bookkeeping, no per-tick scans, no cost that grows with time or dead history), D3 (no
bespoke entities, blocks or items), D4 (plugins per mod), P3 (the engine owns the truth, puppets are
never saved; Ben reopened it for roosting dragons with EQ-2 (a), as MOWZIE §4.9 does for dens and groves),
P13 (the oil gate stays dangerous), A10 (a creature a player takes leaves the sim), C2 (no villager bodies for now), C22
(villages join at structure generation), M1 (dragon pairs breed in their lair), P12 (what dragons
prey on), R18a and OG-1 (natural dragons left alone), OG-9e (the oil nest with a Death Worm body),
UR-1 (rtsim can be removed and reinstalled at any time, and no backward compatibility before a
release).

**Inputs:** `research/population-inventory-2026-10-21.md` (how every population is born, dies and
comes back on main today, with citations); `MOWZIE.md` on `plan/mowzie` at `be38b77` (read, not
edited); `OILGATE.md` on main (the oil gate's field flights and lairs near oil); the pack's jars as read
for the inventory and for this design (§2.4).

**Evidence:** a coupled model of every kind, run on many seeds for 60 game years with shocks, and for
2,739 game years unattended: `research/equilibrium-2026-10-21/` (`model.py`, `game.py`, `run.py`,
`results/results.json`, `results/summary.md`). Section 3 has the maths and the numbers.

Calendar used throughout: a game day is 24,000 ticks (20 real minutes); a game year is 96 days
(Serene Seasons); ten real years of uptime is 262,980 game days, about 2,739 game years.

---

## 1. What changes in play

### 1.1 In one page

- **Nothing that lives at a place is gone for good.** Every lair, den, grove, oil nest, camp, haven,
  village and herd range has a way back, from a source the player could see: a hatchling or a dragon
  flying in from beyond the explored edge, settlers walking over from the next village, a herd
  wandering in from the next range, warriors drifting into a camp. Today herds, villagers, Ribbit
  residents, pirate fleets, Frostmaw and Death Worms do not come back.
- **Farming slows itself down.** Each site remembers how hard players have hit it lately: a pressure
  that rises with every kill and halves every game year. The wait for the next arrival doubles with
  each unit of pressure, up to a cap. A den farmed without pause yields about 1.8 Frostmaw a year (Ben
  chose MOWZIE's 6-day base, EQ-4 (b); the flat 6-day draft gives 12 to 16); a lair farmed without
  pause about one dragon a year (4.8 today). Leave the site alone and its pressure halves every game
  year; three quiet years bring a lair's wait back to within a fifth of its first. A kill by the player's pet, trap or turret counts as his, and luring a
  creature away from home costs its site the same as killing it (§2.2, §2.3).
- **Herds settle.** Births slow as a herd fills its range (a Beverton-Holt term), so herds sit at a
  steady share of their range whatever the forage tuning, with no winner-take-all and no extinctions
  in 60 years of any scenario. Today a 41-year run swings between 40,000 and 117,000 animals while one
  herd takes over and others die out.
- **Dragons follow food, and never finish a herd off.** Dragons switch to whatever prey is common, and
  cannot take a herd below a tenth of its range's capacity (a refuge: the animals in rough ground).
  A dragon whose hunting ground is eaten out leaves; a newcomer settles only where there is food.
- **Villages come back, and the economy sees the real number of villagers.** A village emptied of
  villagers gets a pair of settlers 32 game days later (longer after repeated killing) and grows back
  toward its beds over about five game years. Today a dead village stays dead while the economy believes it is full.
- **When factions fight is CONFLICT-LIFE.md's.** This design proposed a gate (a war party goes out only
  when it is worth sending against what the faction believes of the target, §2.5). Ben rejected it on
  2026-10-21 with the other two EQ-5 options ("All of these systems feel a bit too formulaic"), and
  `CONFLICT-LIFE.md` replaces §2.5. What stays from here: today's grievance ladder sends raid after
  losing raid at a fortress (13 to 14 wars a year in the model) and drives an arms race against a
  player who keeps building walls, and the renewal and pressure rules for camps (§2.4.5) hold whatever
  decides the fights.
- **Pirates no longer die out.** Havens hire crews up to a cap and hunt only routes they can win.
  Today a haven's crew pool refills only from captures, so a fleet that loses its ships is gone.
- **A server left alone for ten real years keeps every kind alive at a steady share,** and the number
  of renewal events per day stays flat from the second game century on (§3.6). Today a third of the herd ranges die out over that
  time with no player at all, and every pirate fleet is gone within the first game century.
- **Nothing pops in front of the player.** An arrival is placed when its site loads, or walks in from
  its source while a player watches; it is never conjured inside a site a player is looking at.
- **Nothing new in the world.** Every body, structure and item is the pack's (D3).

### 1.2 Walkthroughs

**Killing every Frostmaw in a region.** Ben clears the four dens of a snowy range in a week. Each den's
pressure goes to 1 (§2.2), so each will receive a migrant 12 days after its kill (`6 x 2^1`; the base
is MOWZIE's 6 days, EQ-4 (b), Ben 2026-10-21). Each
Frostmaw's notable dies and its heir takes the grudges (MOWZIE §4.2), so the range's anger at the rail
line does not end with the bodies. On day 48 or later, the first time each den loads with no player
inside 128 blocks, a Frostmaw lies asleep at the den's spot; MOWZIE's crystal question (MZ-4) decides
whether it carries one. Ben kills all four again on day 14: pressure is now `1 + 2^(-14/96) = 1.90`,
so the next migrants come after 22 days. A third round lifts it to about 2.6 and the wait to 36 days,
a fourth to 3.0 and 47; held there, the wait settles near 52 days and each den yields about 1.8
Frostmaw a year (§3.3: 1.79 a year closed form, the model 1.83). If he then leaves the range alone,
the pressure halves each game year: a kill after one quiet year brings a migrant in about 35 days,
after two in 21, after three in 16; 12 days is the floor a first kill gets. What he sees in the census (`rtsim-cli sim census` or the viewer): the
dens marked empty with their due days, and the range's heirs.

**Farming dragons at one lair.** Ben camps at a fire roost and kills each dragon as it lands. Today the
lair draws a new 30-day-old dragon from 3,000 blocks away 20 days after each death, with no limit:
4.8 dragons a year. Under this design the first newcomer comes 40 days after his first kill
(`20 x 2^1`; at a lair no player has hit, a dragon that dies of age is replaced after 20), the next
about 67 days after the second kill, then 83, and the waits settle near 87 days (89 from kill to kill,
with his two-day rounds): about 1.1 dragons a year (§3.3: 1.08 closed form, 1.12 in the model).
Taming one counts the same as a kill (A10: it leaves the sim; the lair's pressure rises), and so does a
kill by his tamed dragon, his wolves or a trap he built (§2.2). If the lair is near an oil field, the oil
gate's visitor dragons keep coming to the field as its danger floor (OILGATE §4.2, P13), but none of
them may settle in his lair before its wait ends (§2.4.1).
Each newcomer flies in from the nearest unexplored direction, so Ben sees it come; if a pair at a lair
within reach has raised a hatchling of dispersal age (M1), that hatchling takes the lair instead. If
Ben also eats out the hunting ground (kills the herds in the nine regions around the lair), the lair
stays empty until the herds or a village's livestock can feed a dragon again (§2.4.1).

**Wiping out a village.** Ben kills all twelve villagers of a plains village. The next census on load
counts zero, and the sim's population follows it down (today a zero census is ignored), so the
village stops consuming and its market goes quiet. The village's pressure is 1, so 32 days later two
settlers come: a family from beyond the explored edge (no other village loses anyone, §2.4.3). If Ben
is nearby they walk in from the nearest edge of the explored world; otherwise they are in the village
at its next load. From
two, vanilla breeding refills the village while a player is near (beds and food, vanilla's rules), and
the sim grows it toward its beds while it is unloaded (§2.4.3). Unvisited, it is back to 90% in about
480 game days in the model (438 to 534 across seeds): five game years, under seven real days of uptime. If Ben keeps killing every pair, the pairs keep coming,
more slowly: about three pairs a year. Zombie sieges, raids and dragons do not add pressure; only
player kills do. This walkthrough is EQ-1 (a), Ben's answer.

**A server left alone for ten real years.** Nobody logs in from the day the last player leaves. No
chunk loads, so everything runs abstract. Dragons age out at 400 days and their lairs stand empty 20
days before a newcomer arrives: 95% of lairs are held at any time. Herds hold about 60% of their
ranges' capacity, every range alive. Villages, dens, groves, nests and camps stay full. Pirate havens
keep 83% to 94% of their ship slots manned (by century): they lose ships to merchants and refit. The number of
renewal events per day is flat from the second game century to the 28th (§3.6). When a player
finally logs in, the first chunks to load place what the sim says is there: a Frostmaw in each den,
the grove's warriors, the villagers.

---

## 2. The rules

### 2.1 Two kinds of equilibrium

**(a) Population equilibrium.** For every kind and every coupled pair:
1. **Bounded:** each site holds at most its capacity K, and sites are structures the world generated
   (or habitat the world model sampled), so totals are bounded by the explored world. No explosion.
2. **No absorbing zero:** every site below K has a source and a finite expected wait, whatever killed
   it. No extinction spiral.
3. **Stable:** with no player, every kind's share of capacity returns to one band after any shock, and
   the band does not drift over 2,739 game years. Coupled pairs (dragons and herds, factions and
   factions, pirates and merchants) have an interior steady state: neither side is driven to zero, and
   no camp, herd or faction takes over its neighbours.
4. **Throttled under farming:** a site killed out as fast as it refills settles at a closed-form
   yield (§3.3), not at its fastest refill.

**(b) Strategic equilibrium.** Where a kind makes choices, each choice is a best response to what it
believes of the player and of its neighbours, and the player's best response to the kind's rule is
stationary: play converges instead of ratcheting. Concretely, a faction attacks only on positive
expected value, offers terms in the bargaining range first, and otherwise holds and probes (§2.5).

| Kind | (a) | (b) | Why |
|---|---|---|---|
| Dragons | yes | prey and lair choice only (ideal free distribution); tribute stays SINK.md's | they choose where to hunt and where to live, not whom to fight |
| Herds | yes | dispersal only (ideal free distribution) | animals choose ranges, not fights |
| Human villages, Ribbit villages | yes | no | their choices are prices, which the economy already settles (`sim quiet`) |
| Conflict factions (goblin, bandit, raider) | yes | yes | they choose raids, terms and targets |
| Frostmaw, Umvuthana | yes | yes, through the conflict engine (MOWZIE) | they are conflict cultures |
| Pirates (and ships) | yes | route choice | a pirate chooses which lane to hunt |
| Death Worms and oil nests | yes | no | a nest defends; it does not choose |
| Notables | records only | no | no population of their own |
| Industry districts | no | no | only players make them (§2.4.10) |

Among animals, a Nash equilibrium of where to live is the ideal free distribution: no animal can do
better by moving. Herds' spring dispersal (from full ranges to thin ones) and dragons' leave-when-
starved, settle-only-where-fed rules move toward it; §3 measures the bands, not a proof of it.

### 2.2 The shared rule

Every population that lives at a place is a **site**: a lair, a den, a grove, an oil nest, a camp, a
haven, a village, a Ribbit village, a herd range. A site has a capacity K, a count n of members at or
belonging to it, and a **pressure** P:

- **Pressure.** `P(t) = P0 x 2^(-(t - t0) / H)`. Each member a player kills or takes adds `1/K` (1 for a
  den or a lair), at the time of the loss. Losses with no player (age, starvation, another faction, a
  predator, a zombie siege) add nothing, so the world does not learn to empty itself. `H` is 96 days,
  one game year (TUNABLE). Stored as two numbers; read in closed form.
- **A player's loss** (`LossCause::Player`) is any of:
  1. a death vanilla credits to a player: the killer is a player, or an entity a player owns (a tamed
     dragon, a wolf: `OwnableEntity` with an owner), or a player hurt the victim in the last 100 ticks
     (`lastHurtByPlayer`, the credit vanilla's player-only loot and experience use);
  2. every A10 removal: taming, a lead, Carry On, a capture item;
  3. for the kinds whose bodies carry boss loot (dragons, Frostmaw, the Umvuthi, Death Worms), any
     death in the world that no wild mob is credited with: a trap, a contraption, a turret, lava, a fake
     player or an unknown source counts as the player's;
  4. a home body gone missing while rtsim was running (§2.3).

  Today the dragons' bridge looks only at `source.getEntity()` (`DragonBridge.java:758-766`), so a
  pet's or a trap's kill reads as a mob or nothing, and it reports a taming with no attacker
  (`:1000-1016`). Both change (§2.4.1). Without this, a player who farms a lair with a tamed dragon or a
  trap would never raise its pressure.
- **Delay.** When a site is short (n < K) the next member arrives after `min(cap, base x 2^P)` days,
  with P read at the moment the wait starts. Members arrive one at a time; when one arrives and the
  site is still short, the next wait starts. One scheduled event per arrival. A wait already running is
  not pushed out by more losses; the pressure they add is read when the next wait starts. A pool that
  refills at a rate (a camp's recruits) divides the rate by `2^P` instead: the same rule for a count
  kept as a number. Herds renew by births and dispersal, not by this delay (§2.4.2). Growth from inside
  a site (villagers breeding, a village's growth while unloaded, the Ribbit refill) and a haven's hires
  are not slowed by pressure: they are bounded by beds, houses and the crew cap, and what a player
  could farm from them carries no boss loot (§5.2).
- **Source.** Where the member comes from, in world terms:
  - **Heir:** from inside the site (a hatchling of the lair's pair, a grove warrior taking up the
    Umvuthi's mask, a village's own children by vanilla breeding).
  - **Neighbour:** from a site of the same kind in reach that has more than it needs, which loses the
    member (a herd's surplus walking to a thin range, a dragon's hatchling dispersing to a vacant
    lair). Conserved: a move, not a mint.
  - **Frontier:** from beyond the explored edge, when no neighbour can give (a dragon from 3,000
    blocks away, a Frostmaw migrant from the range's unexplored side, a small founding herd, a family
    of settlers).
  - **Regrow:** the site's own supply (a camp's recruits from the countryside, a haven's hires, an oil
    nest's worms while it is drilled).
- **Arrival.** How it appears:
  - **OnLoad:** placed when the site's chunk next loads, only if no player is within 128 blocks of the
    site (TUNABLE). The chunk loads at view distance, so the body is simply there when it renders.
  - **WalkIn:** a puppet walks or flies in from the source's direction while a player is near.
  - **Abstract:** the count changes while the site is unloaded; the next load's census reconciles.
  - **Never** into a watched site: an OnLoad arrival due while a player is within 128 blocks waits for
    the next unwatched load, or comes as a WalkIn if the kind can walk.
  - **A site that never unloads** (spawn chunks, a forced chunk, a chunk loader such as the MIC grid
    loader) would never see a next load. So when a wait ends with the site already loaded, the plugin
    runs the census there and then and places the arrival if no player is within 128 blocks; if one is,
    it tries again a game day later (one wheel entry).
  - **A war party on the march is not an arrival** (MZ-15, MOWZIE.md §4.16). An arrival adds a member
    the site did not have; a party's members are its camp's own, out on a route that BAND_MARCH holds
    and the far view draws from about a kilometre away. So the 128-block rule does not apply to it: its
    bodies are placed where its march puts them as they come inside the entity-ticking area. For a
    simulation distance of s chunks its edge is 16s to 16s + 16 blocks off along an axis and up to about
    1.4 times that on a diagonal (`DistanceManager.java:252-253`): 160 to 176 blocks at 10, vanilla's
    default, and 128 to 144 at 8. The column fills in stack by stack (a row of two warriors, a raptor
    pack, the chief alone; MZ-17, provisional (a)), each stack the tick its last member comes inside,
    so a raptor pack's nearest body can be a few blocks inside the edge. No warrior or raptor is drawn
    where it appears, at either distance: they walk into sight. The Umvuthi, the one body drawn beyond
    128 blocks (132), goes aloft at the rear of the column; at 8 it can be seen to appear 128 to
    132 blocks off along an axis, never nearer, and at 10 it appears beyond its draw distance. Nothing
    appears that was not already walking toward the player. A party that reaches a watched target with
    no bodies yet (a player who logs in or teleports there as it arrives) comes as a WalkIn along its
    own route: placed at the farthest ticking point on that route back from the target, it walks the
    rest, instead of today's waves on the arc 112 blocks out. That point is at least 160 blocks off
    along an axis at a simulation distance of 10, and at least 128 at 8.
  - **A joint attack** (CONFLICT-LIFE.md §2.16) is two such parties, each its own camp's members on its
    own route with its own BAND_MARCH; the rule holds for each contingent, and the two share one
    spawn quota of 16 members (CL-D49).
- **Bounds.** n never exceeds K. The sim never creates a site: sites are the structures the world
  generated (C22), or the herd ranges the world model sampled.
- **The oil gate** (OILGATE.md on main) adds two dragon sources near oil fields, and they are its
  decisions, not this design's: **field flights** (§4.2), a stream of visitor dragons drawn while a
  player is near a field, capped at 200 a dimension and 8 a field, which is the danger floor P13 asks
  for; and one Ice and Fire roost placed near a field that has none (§4.4, `PLACE_LAIR` on the unmerged
  branch), bounded by the fields. Killing visitors is fighting the gate's stream, as killing In
  Control's desert dragons is today, and pressure does not touch it. What this design adds: **a visitor
  that settles takes a lair like any newcomer,** so it may claim only an empty lair whose wait has ended
  (§2.4.1). The flights therefore cannot refill a farmed lair early, and the gate's danger does not
  depend on any lair's pressure.

**Why this rule.** Under constant farming (each arrival killed within c days) the pressure settles at
`P* = (1/K) / (1 - 2^(-gap/H))` with `gap = min(cap, base x 2^P*) + c`, where `P*` is the pressure
just after a kill. The base is the wait after a death no
player caused; a player's first kill at a one-member site doubles it; H sets the farmed yield (§3.3).
A site left alone forgets in a few years. Every quantity is a closed form: no tick, no scan.

**Shunning** (a site that refuses arrivals for good after enough kills) is unnecessary: a farmed K=1
site settles near P = 2, about one arrival a year. I leave it out (§5.2).

#### The interface species plans use

`rtsim-kernel::renewal` owns the maths, so every engine, the model and the tests agree. The fields
live in each engine's own site rows (the dragons' `Lair`, the conflict `Faction`, the herds' range, the
village `Settlement`, the ships' `Haven`), never in a shared table.

```rust
// crates/rtsim-kernel/src/renewal.rs (to be written; signatures, not code)

/// Recent player-caused losses at one site: P(t) = p * 2^(-(t - at) / H). 8 bytes. `at_day` is the
/// engine's own day; elapsed time is `day.saturating_sub(at_day)`, so a clock that restarted after a
/// reset can never read as a huge gap (the ledger hands over relative values, §2.3).
#[derive(Clone, Copy, Default)]
pub struct Pressure { p: f32, at_day: u32 }

impl Pressure {
    pub fn now(self, day: u32, half_life_days: f32) -> f32;
    /// `amount` is members lost / K; only `LossCause::Player` losses call this.
    pub fn bump(&mut self, day: u32, amount: f32, half_life_days: f32);
    /// A site found empty with no ledger row (§2.3): as if farmed to the cap.
    pub fn at_cap(rule: &RenewalRule, day: u32) -> Self;
}

/// One kind's renewal timing. Every field TUNABLE, read from the module's tunables.
pub struct RenewalRule { pub base_days: f32, pub cap_days: f32, pub half_life_days: f32 }

impl RenewalRule {
    /// min(cap, base * 2^P), in ticks.
    pub fn delay_ticks(&self, p: Pressure, day: u32) -> u64;
    /// The farmed steady state for a site of K killed within `check_days` of each arrival:
    /// (P*, days per arrival). Tests and `sim census` compare against it.
    pub fn farmed(&self, k: u16, check_days: f32) -> (f32, f32);
}

pub enum Source { Heir, Neighbour, Frontier, Regrow }
pub enum Arrival { OnLoad, WalkIn, Abstract }
/// Player: §2.2's four cases (vanilla's kill credit, A10 removals, an unclaimed death of a boss kind,
/// a home body gone missing while rtsim ran). Only Player bumps pressure.
pub enum LossCause { Player, Age, Starved, Faction, Predator, Other }
```

**The event contract.** Each plugin keeps its own opcodes; these are the fields they must carry.

| Contract | What it carries | Existing events it extends |
|---|---|---|
| SITE_FOUND | site key, kind, K, and the site ledger row if one exists, as relative values: pressure now, ticks until due, members missing or left behind (§2.3) | LAIR_FOUND, FACTION_FOUND, VILLAGE_ADOPTED, HAVEN_FOUND, the nest's (OG-9e) |
| SITE_LOSS | site, count, `LossCause`, actor (the credited player, when a player) | DRAGON_DIED (taming now names the player), RESIDENT_KILLED, HERD_KILLED and HERD_TAKEN, BAND_CASUALTIES, SHIP_SUNK and SHIP_CAPTURED, MOWZIE's deaths |
| SITE_CENSUS | site, members seen at home, and for boss kinds their UUIDs; whether rtsim ran through the time since the last census | VILLAGE_CENSUS, MOWZIE's HOME_CENSUS, a new lair census (§2.3) |
| RENEW | site, `Source`, `Arrival`, the source's direction for a WalkIn | the dragons' Respawn, MOWZIE's HOME_SPAWN, Ribbits' SpawnResident, a settlers' op |
| SITE_STATE | full, short (with due day), empty, broken; P | archive and viewer only, and the site ledger write |

### 2.3 Bodies, resets and the site ledger (UR-1)

Two requirements: **renewal must not leave the world depending on sim state** (remove rtsim and every
site still looks as the world left it), and **a reset must not mint free boss loot** (deleting
`world/rtsim/` must not shorten a wait or bring back a member that died).

**Body policy.** A kind that lives at its site keeps a vanilla body there, saved by vanilla, and
becomes a puppet only while away. This is MOWZIE §4.9's rule, generalised. A kind that is always on
the move stays a puppet.

| Kind | At home | Away | After an uninstall | After a reset |
|---|---|---|---|---|
| Dragons | **vanilla, roosting in its lair** (today: a puppet always; EQ-2, which reopens P3) | puppet (hunting, raiding, flying) | the roosting dragons stay, as Ice and Fire left them | lair census on LAIR_FOUND; the ledger's pressure |
| Herds | puppets near players (unchanged) | puppets | the sim's herds go; vanilla's own animals stay | re-founded from the world model at founding size, as today: a reset refills a hunted-out range, which I accept because animals are not boss loot |
| Villagers and settlers | vanilla (C2: the sim owns none) | n/a | stay | VILLAGE_CENSUS |
| Ribbit residents | vanilla (`ResidentSpawner`) | touring bands are puppets | stay | census |
| Frostmaw, Umvuthana, Umvuthi | vanilla (MOWZIE §4.9) | puppet in parties | stay | HOME_CENSUS; the ledger |
| Death Worms at an oil nest | vanilla (Ice and Fire's worm is persistent) | never leaves | stays | nest census; the ledger |
| Goblin, bandit, raider | no home bodies in the pack (§2.4.5) | puppet parties | n/a | the ledger's pool |
| Pirates | no plugin yet | puppets | n/a | the ledger's crew pool |

**The site ledger.** One small row per site in world SavedData, `mic_rtsim_sites.dat`, beside the
existing `mic_rtsim_villages.dat` and outside `world/rtsim/`, so a reset does not delete it: site key,
kind, K, the sim's last count, pressure (P0, t0), the due time, the members left behind (members that
left an unloaded home and died away, whose saved vanilla body the next load's census must remove), and
for boss kinds the UUIDs of the home bodies and of members missing (at most K each).
- **Clock.** The ledger's times are the world's game time (`ServerLevel.getGameTime()`, saved in
  `level.dat`, monotonic, untouched by `/time set` and by an rtsim reset). SITE_FOUND hands the engine
  relative values (pressure now, ticks until due), never an absolute time, so the engine's own clock may
  restart at zero after a reset; every elapsed time on either side is a saturating subtraction.
- **Size.** About 32 bytes a row, plus 16 bytes per tracked body (a den 1, a lair 2, a grove up to 10,
  a nest 2). One row per generated site, so it grows with exploration, never with time: a world with
  5,000 sites is under 400 KB. The plugin updates a row when the engine sends SITE_STATE and marks the
  file dirty; vanilla writes the whole file at its own world saves (SavedData's rule), not per event.
- **Format:** none promised before a release (UR-1); a row that does not parse is dropped.

**Missing members.** A census that finds fewer home bodies than members at home, with no death or A10
report for the others, does not book them as dead (MOWZIE §4.9's "died unseen"). For the boss kinds the
ledger knows each home body's UUID, and a body not found is **missing**:
- It is a player's loss (`P += 1/K`) when rtsim ran through the whole time since the last census: a
  home body can move only while its chunk is loaded, and it leaves home only when lured, led, carried
  or chased. At the first census after rtsim detects a run without it (UR-1's check), it is `Other`.
- Its replacement waits `min(cap, base x 2^P)` from that census for a player's loss, the cap wait for
  `Other`.
- If the body turns up again (`EntityJoinLevelEvent` with a UUID the ledger holds as missing) while its
  site is still short, it is home again and the replacement is cancelled. After the site has refilled it
  is a **stray**: still a vanilla body, member of no site, and its death later adds `1/K` to its old
  site like a player's kill. The ledger keeps at most K missing UUIDs a site, the oldest dropped first.

So luring a Frostmaw out of its den costs the den what a kill would, killing the stray later costs it
again, and the den's crystals stay at the farmed yield of §3.3. A body that wandered off on its own,
with rtsim absent, refills at most once per cap wait. Villagers and Ribbits are counted, not tracked by
UUID: they carry no boss loot, and a missing villager is `Other`.

**Why it stops the mint.** After a reset the engine rediscovers each site on load. With the ledger it
resumes the pressure, the due time and the left-behind and missing members, so a farmed den is still farmed and a
Frostmaw killed away from its unloaded den does not reappear. A site found empty with **no** ledger row
starts at the cap wait (`Pressure::at_cap`): an emptied site whose history is lost is treated as
farmed, never as fresh. Today a reset re-derives a lair as "found with no dragon" and gives it a dragon
after 20 days.

**What an uninstall can still do,** as UR-1 allows: a member that died away while its home was
unloaded still has its saved vanilla body at home, so a player who uninstalls before that home loads
can kill it again for its loot, once per such member. Members away at the moment of an uninstall are
lost (puppets); the ledger marks them away, and the reinstall's census books them `Other` with the cap
wait, so a reinstall never speeds a refill and no loot came of them. Nothing else.

### 2.4 Per kind

Each kind below gives: the site and K; what counts as a loss and what raises pressure; renewal (rule,
source, arrival); bounds; rates (every number is TUNABLE; the values given are the starting ones); what the player
sees; body; cost. Rates are the model's (§3); the engine's names are given where they exist.

#### 2.4.1 Dragons (Ice and Fire)

- **Site:** the lair (roost or cave), K = 1 occupant. M1's mated pair stays as built: at most a pair
  per lair, so dragons are at most two per lair, and lairs are Ice and Fire's structures.
- **Losses:** age (400 days, `lifespan_days`); a player's loss (§2.2: his kill, his pet's, a trap's or
  any death no wild mob is credited with; taming or any A10 removal; a roosting body gone missing), each
  `P += 1`. The bridge changes to match: `attackerKind` reads vanilla's kill credit
  (`getKillCredit()`, then the killer's owner), and `onTame` reports the taming player instead of
  `ATTACKER_NONE` (`DragonBridge.java:758-766`, `:1000-1016`); **starve-leave:** a dragon older than 60 days whose lair's fed share (a 30-day
  moving average of how much of its need it catches) falls below 0.35 leaves; it settles in a vacant lair
  in reach whose fed share is at least 0.5, else it flies off the map and leaves the sim.
- **Renewal:** when the lair stands empty, after `min(192, 20 x 2^P)` days (base 20 is today's
  `respawn_delay_days`) the lair is ready. A newcomer settles only if the lair's hunting ground would
  feed it (fed share at least 0.5), else the lair checks again every 24 days. **Source:** a hatchling
  of dispersal age from a lair within reach (M1, Neighbour) if one exists, else today's 30-day-old
  newcomer from 3,000 blocks (Frontier), now flying in from the nearest unexplored direction instead
  of a random one. Near an oil field, an oil gate visitor ready to settle (OILGATE §4.2) is a Frontier
  source too, and obeys the same wait: it may claim only an empty lair whose wait has ended.
- **Predation (P12):** a dragon picks among herds with weight `(N - R)^2 / K` (type III: it switches
  to what is common and leaves what is scarce), where `R = max(8, 0.1 K)` is the herd's **refuge**,
  the share it cannot reach. `predate` never takes a herd below R. Village livestock and caravans stay
  on the menu (weight 300 near a village, 30 elsewhere, TUNABLE), so a dragon in an eaten-out valley
  turns to villages and roads, as P12 wants. Its catch per day is `W / (W + 200)` of its need.
- **Bounds:** at most one occupant (or one pair) per lair; lairs are found, never made.
- **Rates:** base 20 days, cap 192, H 96; starve-leave 0.35, arrive-food 0.5, refuge 10% (min 8).
- **What the player sees:** a lair stands empty 20 days after its dragon dies of age (at a lair no
  player has hit), 40 days after a player's first kill, about 87 days under steady farming; a lair whose valley is eaten out stands empty until the herds come back, and
  its dragon is seen leaving.
- **Body:** vanilla while roosting at its lair, puppet when it leaves (EQ-2, which reopens P3). Today a
  puppet always.
- **Cost:** per lair 8 bytes of pressure, a 4-byte fed share updated at each hunt (an event that
  exists), one wheel entry while empty. The prey weight replaces today's utility term; no new pass.

#### 2.4.2 Herds and wild animals

- **Site:** the 512-block herd range (one herd per region today). K = the range's carrying capacity:
  today's founding size, `density x habitat cells`.
- **Births:** once a year, as today, with a density term: `Binomial(A x ceil(f), f x condition x
  1/(1 + A/M) / ceil(f))`, where M is set per species so that the herd's steady state is K (§3.2).
  Below K a herd grows; above it, it shrinks; it never overshoots. One multiply per herd per year.
- **Losses:** today's (age, background mortality, starvation, dragons, player kills and takes). No
  pressure: a herd's renewal is its own births, not a delay.
- **Renewal:** (1) **spring dispersal (Neighbour):** each spring, 5% of the adults of a herd above
  0.4 K walk to the neighbouring ranges below 0.25 K, split evenly, and found a range that is empty
  (they carry their species). Both thresholds sit below the steady state herds reach under dragon
  predation (about 0.6 K, §3.6); a donor threshold above it (the first model run used 0.8 K) almost
  never fires. (2) **frontier founding:** an empty range with habitat is re-founded by `max(12,
  0.05 K)` animals at a rate of 0.25 per range-year (one draw per empty range, scheduled), arriving
  from the direction of the nearest live herd or the explored edge. The tombstone, the permanent
  region mark and the name guard go: an extinct herd's row is reused.
- **Bounds:** K per range, ranges fixed by the world model.
- **Rates:** dispersal 5%, thresholds 0.4 K and 0.25 K, founders `max(12, 0.05 K)`, frontier 0.25
  per range-year, refuge 10% (min 8).
- **What the player sees:** herds that hold a steady size; a valley the player hunted out sees
  stragglers from the next ranges the next spring and is half full again in about 12 game years
  (§3.6). Births are once a year, so no rule here can make that fast without minting animals (§5.2).
- **Body:** puppets near players, as today.
- **Cost:** the yearly decision already visits each herd; dispersal reads four neighbours there. The
  frontier draw is one wheel entry per empty range.

Vanilla's own animals (chunk-generation animals and vanilla spawns near players) stay outside the sim,
as today (Y20).

#### 2.4.3 Human villages and villagers

- **Site:** the village; K = its beds (the census already walks the village's POIs).
- **Loaded:** vanilla rules (breeding needs food and a free bed within 48 blocks; zombie sieges; golems
  from 5 villagers). The census reports what is there, and the sim's `pop` **follows it both ways**:
  today it only rises after the first census and a census of zero is ignored (`phase1.rs:1071-1073`).
- **Losses:** a villager a player kills or takes (vanilla's credit, §2.2 cases 1 and 2) adds `1/K`.
  Zombies, raids and dragons add nothing.
- **Renewal (EQ-1):** (1) **settlers:** a village below 2 villagers (vanilla breeding needs two) gets
  a pair after `min(192, 16 x 2^P)` days: a family from beyond the explored edge (Frontier), walking in
  from the nearest edge of the explored world when a player is near (WalkIn), OnLoad otherwise. No
  other village loses anyone: taking two from a donor would mean removing vanilla villagers the player
  may have traded with, cured or named, which C2 and A10 never allowed. (2) **unloaded growth:** while unloaded, the sim grows the village toward its beds as a logistic in closed
  form, `dN/dt = g N (1 - N/K)` with g 0.01 per villager-day, read at the next load; the deficit is
  placed as vanilla villagers at the next unwatched load (they take professions from workstations as
  vanilla's do). Growth is not slowed by pressure (§2.2): villagers carry no loot, and loaded
  villages already breed by vanilla's rules with no limit but beds.
- **Bounds:** pop at most K (beds); villages are worldgen structures (C22).
- **What the player sees:** an emptied village gets two settlers 32 game days later (about 11 real
  hours) and is back to 90% in about five game years (480 days in the model, under seven real days of uptime); a murder-farmed village keeps getting pairs,
  about three a year.
- **Body:** vanilla. Settlers and grown villagers are ordinary villagers once placed; the sim owns
  none of them (C2 holds in that sense; Ben's EQ-1 (a) accepts this reading of C2).
- **Cost:** one wheel entry per village below 2; the logistic is evaluated at load. The village pop
  changes on census, a rare event, so `sim quiet` stays quiet (tested, §4).

#### 2.4.4 Ribbit villages

- **Site:** the Ribbit village; K = its houses (the adoption count plus houses grown, at most 8 more).
  Ribbits do not breed (an AgeableMob with no breeding), so the sim is their only source.
- **Renewal:** (1) **refill before build:** on each prosperous growth clock (10 days, today's
  `growth_after_days`), a village with fewer residents than houses gets a resident into an existing
  house (today's `SpawnResident` path) instead of a new house; not slowed by pressure, as a
  village's growth is not (§2.2). (2) **settlers:** below 2 residents, a pair after
  `min(192, 16 x 2^P)` days from beyond the explored edge (Frontier), as humans; no Ribbit village
  loses anyone. EQ-1 covers Ribbit settlers too.
- **Fix:** book `pop` when the resident exists (the census), not when the house is ordered
  (`ribbits.rs:356-359`).
- **Bounds:** residents at most houses, houses at most the adoption count plus 8.
- **What the player sees:** killed ribbits are replaced one per 10 prosperous days; an emptied village
  is back to 90% in about two game years (216 days in the model, three real days).
- **Body:** vanilla, as today. **Cost:** unchanged clock; one wheel entry while below 2.

#### 2.4.5 Conflict factions (goblin, bandit, raider)

- **Site:** the camp; K = the culture's `pool_cap` (goblin 60, bandit 40, raider 50).
- **Losses:** warriors killed by a player, at home or in a party, add `killed/K`; war-party losses to
  another faction or a village add nothing. In a joint attack (CONFLICT-LIFE.md §2.16) each camp books
  only its own contingent's dead, against its own K.
- **Renewal (Regrow):** recruits at `recruits_per_day / 2^P` (goblin 2, bandit 1.5, raider 2): word
  gets around that the camp is a death trap. Closed form between events: P is read at the last event
  that touched the pool (a muster, a loss, a census), so the pool stays linear in time.
- **Break and refound:** a camp whose pool is below 10% of its cap while P is at least 2 breaks: its
  survivors scatter and its node goes quiet. After `min(384, 48 x 2^P)` days a new band takes the empty
  camp with a quarter of the cap (Frontier). A faction can never break another; only players do.
- **(b):** when war parties and plunders go out is `CONFLICT-LIFE.md`'s (EQ-5: Ben rejected the gate
  of §2.5); the pool they draw on is this section's.
- **Bounds:** pool at most its cap; factions at most the camps found.
- **What the player sees:** a camp he keeps emptying recruits more and more slowly, then breaks and
  stands empty for two to four game years. How camps feud with each other is `CONFLICT-LIFE.md`'s.
- **Body:** there is no camp mod in the pack (Goblin's Tyranny is absent; the camp structure ids in
  the conflict culture files are unverified), so only parties have bodies (puppets). No Java exists
  yet for conflict; these rules bind the engine now and the plugin when it is written.
- **Cost:** one more float per faction (pressure); break and refound are one event each.

#### 2.4.6 Pirates and ships

- **Site:** the haven; K = its ship slots (one per medium today) and a crew pool capped at 18 (three
  sea crews). Today the pool has no cap and no source but captures.
- **Founding:** as today: every slot manned and one spare crew in the pool (`first_crew`).
- **Renewal:** (1) **hires (Regrow):** the pool grows by 0.25 per day toward the cap, logistic
  (`r (1 - pool/cap)`): drifters at the haven's market. Not slowed by pressure: the refit carries it.
  Captures add as today, up to the cap. (2) **refit:** an empty slot launches after
  `min(96, 4 x 2^P)` days (base 4 is today's `haven_rebuild_ticks`) once the pool holds a crew. A ship a
  player sinks or captures adds `1/slots`.
- **(b):** a pirate hunts only a lane whose expected value is positive (win chance times loot, less
  loss chance times its crew's value: with the model's 3 and 6, a win chance above 2/3); a lane
  escorted beyond that is left alone, and with no such lane it rests in port. In the model about 23% of
  havens have no lane worth hunting; §3.6 reports the others separately, since a haven that never sails
  never loses a ship.
- **Merchant and navy ships:** unchanged (back in their slot 2 days after a loss, unconditional). They
  are the economy's carriers, not a population.
- **What the player sees:** sinking a haven's ships quiets its sea for a few game months, not for ever: in the model half its slots are back in 48 days
  and 90% in 72 (96 against the same seed left alone).
- **Body:** no ships plugin exists; puppets when one does. **Cost:** one float per haven.
- **Built** (S8, 2026-10-22; choices T32 to T39 in docs/overnight-choices-ships.md): as above, plus a
  refit in port (a pirate mends its hull and makes up its crew from the pool before each decision,
  the empty slots' crews kept back). On the synthetic coast, seeds 1 to 30: after a kill-all half the
  slots are back in 23.5 days and all in 52.5 (medians; the model's 48 and 72 on its 24-day grid), and
  unattended havens keep every slot manned for a thousand years (1.00, above the band: with nobody
  watching, pirates never fight a ship, T37).

#### 2.4.7 Frostmaw and Umvuthana (MOWZIE)

The MOWZIE plan models both as conflict cultures; this design sets their renewal (MOWZIE §4.10 asks
for it). §2.6 lists exactly what MOWZIE.md must change to match.

- **Frostmaw den:** K = 1. A Frostmaw lost to a player (§2.2: killed at home or away, by him, his pet
  or his trap, or lured away and missing, §2.3) adds 1. After `min(192, 6 x 2^P)` days a migrant from
  the range's unexplored side (Frontier) is placed asleep at the den's spot at the next unwatched load
  (OnLoad; MOWZIE's HOME_SPAWN). The chief notable succeeds at once (records only), as MOWZIE has it.
  The base is MOWZIE's 6 days (EQ-4 (b), Ben, 2026-10-21).
- **The crystal:** whether a migrant holds one is MOWZIE's MZ-4. Under MZ-4 (a), crystals come at the
  den's yield: about 1.8 a year from a den farmed without pause. A robbed Frostmaw stays in its den
  (MOWZIE keeps it persistent) and is still a member: theft is not a loss, so the den refills only
  after its Frostmaw dies or goes missing.
- **Umvuthana grove:** K = the census count at first load (5 to 9). Warriors refill one at a time,
  each after `min(96, 6 x 2^P)` days, with `P += killed/K`; from the grove's edge farthest from any
  player when loaded (WalkIn), or at the next load (OnLoad).
- **The Umvuthi (Heir):** after the later of MOWZIE's cowed time and `min(192, 6 x 2^P_chief)` days
  (MZ-14 (a), Ben, 2026-10-21: the Frostmaw's base of 6, EQ-4 (b)), one warrior takes up the mask: at
  the next unwatched load the plugin removes one warrior body and places the Umvuthi on its throne, 18
  blocks from the fire pit (`UmvuthanaGrovePieces.java:276-288`), at the throne spot and direction the
  census recorded, as MOWZIE §4.6 already places a new chief (finalized as a structure spawn,
  restricted to the throne spot). The grove's count falls by one. A grove with no warrior waits for
  one first. The heir comes 12 days after a first kill, and a grove farmed without pause gives about
  1.8 Sol Visages a game year, the den's rate (MOWZIE.md, "Loot supply").
- **Raptors:** MOWZIE's raptor packs take a district's vanilla animals (owed hunts, MZ-8) and a
  village's livestock goods (plunder), never the sim's herds. Vanilla animals are outside the sim (Y20)
  and goods are the market's, so the herd balance needs nothing from them. If a later plan lets parties
  hunt the sim's herds on the march, they go through `predate` with the refuge.
- **What the player sees:** a cleared grove is back to 90% in about a game year (96 days in the
  model); a farmed grove yields about 6 warriors a year, not 12; the dens as in §1.2.
- **Body:** vanilla at home, puppets in parties (MOWZIE §4.9). **Cost:** as conflict factions.

#### 2.4.8 Death Worms and oil nests (OG-9e)

- **Site:** the oil nest; K = 2 worms (TUNABLE; OG-9e's nest stage may raise it).
- **Renewal (Regrow):** each worm after `min(96, 12 x 2^P)` days, **only while the nest is awake** (a
  pumpjack drilling, as OG-9e has it); an asleep nest holds what it has. A worm lost to a player (§2.2,
  a boss kind: traps count) adds `1/K`. Placed underground in the nest at the next unwatched load.
- **Natural desert Death Worms** (Ice and Fire worldgen, chance 0.0333 per placement) stay outside the
  sim, as natural dragons do (R18a, OG-1).
- **What the player sees:** a cleared nest under a working pumpjack is back within a game year (72
  days in the model); stop drilling and it stays as it is.
- **Body:** vanilla Ice and Fire worms at the nest, counted on load. **Cost:** one row per nest.

#### 2.4.9 Notables

Records with heirs; no bodies, no population of their own. Nothing changes: a notable is removed at
death (`knowledge.rs:484-524`) and its id lives in a generational arena (`rtsim-kernel/src/handle.rs`)
that reuses slots, so dead notables do not accumulate. The arena retires a slot after 65,535 reuses;
that is one slot per 65,535 deaths, negligible over ten years. The stale `Caravan.notable` id
(inventory §6) reads as "nobody known" through the generation check.

#### 2.4.10 Industry districts: made only by players

A district is the sim's mirror of a player's own machines (block entities the player placed). If the
sim made districts it would be placing machines (against D3), minting the player's kind of property
and its output (against UR-1's "never the only copy"), and deciding where industry is (the player's
choice). So districts have no renewal and no pressure; they leave when their seeds are gone, as today.
Threats are what a district faces, and they come from the kinds above.

#### Caravans, messengers, player bases

Not populations in this sense: caravans and messengers are flows on edges and cannot go extinct while
a road exists; a base is a pop-0 node. Unchanged.

### 2.5 The strategy gate (b)

**Rejected (EQ-5, Ben, 2026-10-21):** "All of these systems feel a bit too formulaic, come up with
something that feels more life like." `CONFLICT-LIFE.md` replaces this section; it is kept as the
record of option (a) and as one of the two baselines CONFLICT-LIFE.md measures itself against.

Today a faction's grievance against a player climbs a six-rung ladder (notice, envoy, scouts, raid,
invasion, siege; goblin thresholds 10, 30, 50, 70, 140, 260); each threshold crossed sends the next
rung once the pool holds half its party, war rungs repeat while grievance stays up, and every raider
the player kills adds grievance (`rtsim-conflict/src/engine.rs` 1536-1760). Nothing asks whether the
party can win. In the model (§3.5) that means 13 to 14 losing wars a year at a fortress, and an arms
race against a player who adds defence after each loss (his defence climbs from 68 to 145 over 50 years
and the raids never stop).

**The gate** keeps grievance and its thresholds as the motive (a faction must be angry enough to
consider a rung) and adds a best response:

1. **Belief.** The faction keeps `D̂`, its belief of the target's defence: updated halfway toward the
   defence it met after every fight, and decaying back to a prior (15) with a half-life of 96 days
   when nothing is learnt.
2. **Expected value of a war rung** against `D̂`: `EV = p x min(carry, stock) - warrior_value x
   E[warriors lost] + revenge`, with `p = A^2 / (A^2 + D̂^2)`, the gate's own belief of its chance (not
   the engine's: the abstract fight draws nothing, the stronger side wins, by `village_outcome`'s
   linear law, `engine.rs:4343-4365`, or `threat.rs`'s square law where turrets dominate,
   `rtsim-industry threat.rs:403-427`; CONFLICT-LIFE.md §2.1), `warrior_value` 3 loot items, and **revenge bounded:** `10 x g / (g + 100)` for
   grievance g, so hate alone never pays for a suicide raid.
3. **Terms first.** If the best rung has `EV > 0` and grievance has reached the envoy rung, the faction
   sends an envoy demanding `T = (EV + X) / 2`, the midpoint of the bargaining range between what the
   attack is worth to it (EV) and what it would cost the player (X, the expected loot lost): any `T`
   in that range leaves both better off than a war (the Nash bargaining solution with equal weights).
   If the player pays, today's tribute relief and respite apply. If he refuses, the attack follows after
   3 days, and the faction sends him no new envoy for 24 days (it attacks whenever a rung is worth it).
4. **Otherwise hold, and probe.** With no rung worth sending, the faction holds. Every 48 days at the
   scouts rung it sends scouts, which refresh its belief.
5. **Faction against faction:** the same EV rule, with the target camp's pool as the defence. A party
   of 12 against a camp at strength (40 to 60) has a p of one in 12 to 27, and in the engine it never wins (the fight is certain), so camps at strength never
   attack each other: the background feuds of today (2 to 20 a world-year in the model) stop in a world
   with no player, and a camp falls on a neighbour only after a player, a dragon or a break has thinned
   it (§3.6). Ben rejected this with the rest of EQ-5 (§5.1).

The gate is a best response to a belief, so its outcomes are stationary for each kind of player (§3.5):
a fortress gets scouts and no raids, a weak base pays tribute and is not raided, a player who adapts
settles at a defence where the terms are affordable, and a weak player who refuses everything is
raided at a steady rate (the faction's best response to him).

Pirates use the same rule for route choice (§2.4.6). Dragons do not bargain: tribute stays as SINK.md
§4.6 defines it.

### 2.6 What MOWZIE.md must match

`plan/mowzie` is not edited here. At `be38b77`, MOWZIE.md must change these to match this design.
MOWZIE.md on main has since taken them up through MZ-15 (`c0d6563`, the Invasion marches visibly),
which is Ben's later and explicit choice: where the two differ, MZ-15 wins, and items 8 and 11 now
follow it.

1. **§4.2 and §4.10, the Frostmaw body:** `recruits_per_day` 1/6 (a new Frostmaw 6 days after a death)
   becomes the shared rule: `min(192, 6 x 2^P)` days, P per den with H 96, `P += 1` for each Frostmaw
   lost to a player (§2.2: at home or away, by him, his pet or his trap, or missing). Not age (MOWZIE's
   `chief_lifespan_days` 1,000,000 stays). The base is 6 (EQ-4 (b), Ben, 2026-10-21).
2. **§4.5 and §4.10, grove warriors:** the refill at `recruits_per_day` 2 (a grove of 7 whole in 3.5
   days) becomes one warrior at a time, each after `min(96, 6 x 2^P)` days, `P += killed/K`. War-party
   losses refill by the same rule (it is one pool).
3. **§4.5, §4.6 and §4.10, the Umvuthi:** "6 days, scaled by boldness, after the cowed time" becomes an
   Heir: after the later of the cowed time and `min(192, 6 x 2^P_chief)` days (MZ-14 (a), Ben,
   2026-10-21), a warrior of the grove takes up the mask. §4.6's placement stays (HOME_SPAWN with the
   chief flag at the first unwatched load, on its throne at the throne spot, 18 blocks from the fire
   pit, finalized as a structure spawn) and gains one thing: at that load the plugin removes one
   warrior body, and the grove's count falls by one. None while the grove has no warrior.
4. **Every place that states the six-day refill or the fractional pool:** ledger 6 (the pool refilling
   while pool plus away is under the cap), §4.2's body bullet ("ready six days later"), walkthrough A
   step 8 ("Six days later a new Frostmaw", "Killing buys a week"), walkthrough D (the pool at 0.4 on
   day 12, reaching 1 on day 15.5, "the six empty days"), §7 sim runs 1 and 6 ("the body six days
   later", "a body after six days"), the "Robbed Frostmaw" reason ("a crystal every six days") and
   MZ-4 (a) ("the body after 6 days"). Under EQ-4 (b) each reads 12 days after a first kill, the
   wait doubling with pressure, about 1.8 a year farmed.
5. **MZ-4 and EQ-4:** MZ-4 (a)'s delay is EQ-4's answer (6 days, doubling with pressure); MZ-4 keeps the crystal, the drops, masks and
   the steal. With MZ-4 (a), crystals renew at the den's farmed yield (§2.4.7). A robbed Frostmaw stays
   a member: the steal is not a loss and starts no wait.
6. **§4.9, a den found empty:** "starts with an empty pool" gains "at the cap wait (`Pressure::at_cap`)
   when the site ledger has no row for it".
7. **§4.9, resets:** FACTION_FOUND carries the site ledger row (pressure now, ticks until due, members
   left behind and missing), so a reset neither shortens a wait nor brings back a member that died
   away (§2.3). The ledger's times are world game time (§2.3).
8. **§4.9 and §4.10, arrivals:** HOME_SPAWN never places into a camp with a player within 128 blocks;
   a den's Frostmaw is OnLoad only (asleep), a grove's warriors may WalkIn from the far edge. A camp
   that never unloads gets its census and arrival when the wait ends (§2.2). A war party on the march
   is not an arrival (§2.2, MZ-15): its bodies come where BAND_MARCH puts them as they enter the
   ticking area, at whatever distance that is, and one that reaches a watched target with no bodies
   walks in along its route from the ticking edge instead of appearing on the 112-block arc.
9. **Events and kill credit:** every death report carries `LossCause` and the credited player
   (§2.2's four cases; CAMP_DEATH's killer is vanilla's kill credit or the killer's owner, and an
   unclaimed death of the Frostmaw or the Umvuthi is the player's); HOME_CENSUS is the SITE_CENSUS and
   carries the home bodies' UUIDs; HOME_SPAWN carries `Source` and `Arrival` (§2.2).
10. **§4.10 item 4, predator and prey:** answered with nothing to add: raptor packs take a district's
    vanilla animals and a village's livestock goods, both outside the herds' balance (§2.4.7).
11. **§4.9, the census:** step 3's "the missing ones died unseen, and the pool loses them" becomes
    **missing** (§2.3): a player's loss unless rtsim was absent since the last census, its UUID kept,
    and a missing body that reappears is home again or a stray whose death raises the den's or grove's
    pressure. The removals of step 1 take every body a party on the road left behind, its member alive
    or dead, and the census never ends the party (MZ-15, MOWZIE.md §4.9 and §4.16): the march goes on,
    a living member away is counted with its party and not recalled, and a party ends only at its own
    home or at its last death. Removals are clamped to the bodies the census finds (a world that ran
    without rtsim may have lost them already). The cost is UR-1's: once its camp has come in, a member
    is away for the rest of its march, and an uninstall then loses it (§2.3). A joint attack
    (CONFLICT-LIFE.md §2.16) is two parties: the rule holds for each contingent, and a camp that comes
    in has only its own contingent's bodies taken.
12. **§4.10 item 5 and §7, tests:** `sim mowzie` reports in the `sim census` format (§4, slice S1), and
    the plan adds `kill_all_dens`, `kill_all_groves`, `farmed_den_yields_closed_form` (60 years, about
    1.8 a year: §3.3's 1.79 closed form, the figure MOWZIE.md's "Loot supply" states for a den and,
    under MZ-14 (a), for a grove's Sol Visages), and the GameTests `resetDoesNotMintAFrostmaw`, `aLuredFrostmawIsAPlayerLoss`,
    `aStrayFrostmawsDeathRaisesItsDensPressure`, `aPetOrTrapKillAtADenIsAPlayerLoss` and
    `anUmvuthiHeirTakesAWarrior`.
13. **§4.10's "what this plan needs" list** is answered: caps (1 per den, the census per grove, no
    world-level ceiling beyond the structures), rates against pressure (items 1 to 3), the empty camp
    rule (§2.2, shared with lairs), predator and prey (item 10), the census (item 12).
14. **§4.14, the tunables table:** the "Recruits" row (Frostmaw 1/6 a day, tribe 2 a day) becomes each
    culture's `RenewalRule` (base, cap, half-life) as in items 1 to 3, measured by the census of item 12.

### 2.7 Reuse and custom ledger

**Reused** (nothing new enters the world):

| What | From | Used for |
|---|---|---|
| Lairs, dragons, Death Worms | Ice and Fire CE 2.1-beta.1 | sites and bodies; the worm's persistence |
| Dens, groves, Frostmaw, Umvuthana, Umvuthi | Mowzie's Mobs 1.8.2 | sites and bodies (MOWZIE) |
| Villages, beds, villagers, breeding, sieges, golems | vanilla | K, settlers' bodies, growth while loaded |
| Ribbit villages, houses, ribbits | Ribbits 4.1.6 and the existing `HouseBuilder`, `ResidentSpawner` | K and bodies |
| Animals | vanilla, through `HerdBridge` | herd puppets |
| The year | Serene Seasons (96 days) | H, births, dispersal |
| Engine paths | dragons' Respawn and lair finder, conflict `pool_at`, ships' haven rebuild, herds' yearly decision, the village census | every renewal rule extends one of these |
| Kernel | TimingWheel, LazyTable, Philox streams, generational handles | scheduling, catch-up, draws, notables |
| SavedData precedent | `mic_rtsim_villages.dat` | the site ledger's home |

**Custom** (code only; sizes are estimates):

| # | What | Where | Size |
|---|---|---|---|
| 1 | `renewal`: Pressure, RenewalRule, closed forms | `rtsim-kernel` | S |
| 2 | `sim census`: the who-is-alive census for every module | `rtsim-cli`, each engine's census hook | M |
| 3 | Herd density term, dispersal, frontier founding, refuge in `predate`, tombstone removal | `rtsim-ecology` herds | M |
| 4 | Lair pressure, fed share, starve-leave, arrival gate, newcomer direction | `rtsim-ecology` dragons | M |
| 5 | Site ledger SavedData (world game time, relative hand-over) and FOUND events carrying it | core Java, each plugin | M |
| 6 | Kill credit in every bridge (§2.2's four cases), taming reported with its player | core Java, `neoforge/iceandfire`, the Mowzie and village bridges | S |
| 7 | Missing members and strays: home-body UUIDs, `EntityJoinLevelEvent` reconcile, the no-unload census | core Java, each plugin with home bodies | M |
| 8 | Dragon home bodies (EQ-2 a, decided): vanilla roosting body, puppet away, lair census | `neoforge/iceandfire` | L |
| 9 | Village census both ways, frontier settlers, unloaded growth (EQ-1 a, decided) | `rtsim-world`, `rtsim-jni` phase 1, core `village/` | M |
| 10 | Ribbit refill before build, pop on spawn, settlers | `rtsim-world` ribbits | S |
| 11 | Conflict pressure, break and refound, the gate | `rtsim-conflict` | M |
| 12 | Haven hires and cap, refit pressure, EV lanes | `rtsim-ships` | S |
| 13 | Long-horizon, kill-all and soak tests | each crate, `rtsim-host` | M |

### 2.8 Cost (D1)

| Kind | State per site | Work per renewal | Scans |
|---|---|---|---|
| Every site | Pressure 8 B, a due tick 8 B, counts | one wheel event, one closed form | none |
| Dragons | + fed share 4 B | the fed share updates at hunts, which exist | none |
| Herds | + nothing (M per species is a constant) | yearly decision (exists) reads 4 neighbours | none; frontier is one wheel entry per empty range |
| Villages | + logistic read at load | one census (exists) | none |
| Conflict | + pressure | pool closed form with P at the last event | none |
| Site ledger | about 32 B per site plus 16 B per tracked boss body, Java | a row update on SITE_STATE; the file is written at vanilla's world saves | none |
| Missing members | the UUIDs above | one lookup per `EntityJoinLevelEvent` of a tracked kind, and per death of a stray | none |

Nothing grows with time: site rows are bounded by the explored world, an extinct herd's row is reused,
notables use a generational arena, the ledger has one row per site, and each row keeps at most K
missing UUIDs. The model's renewal events per
day are flat over 2,739 game years (§3.6).

---

## 3. Maths and evidence

### 3.1 The model

`research/equilibrium-2026-10-21/model.py` is a coupled stochastic model, stepped daily (every 4 days
for the unattended run), of every kind above on a 12 x 12 grid of 512-block regions (6,144 blocks
square), vectorised over seeds. It has two modes: `baseline` mirrors main at `f20234e` as the
inventory describes it (no density term in births, permanent herd extinction, a 20-day lair respawn
with no pressure, villages that never refill while the sim's belief only rises, Ribbit villages that
only grow by houses, crew pools fed only by captures, no renewal for bosses or nests since no code
exists); `proposed` applies §2. It models the rules' shape (fixed points, bounds, recovery, knife
edges), not the engine's exact numbers.

What is in it: forage per region with logistic regrowth and shared grazing (the engine's claim
penalty); five species with the engine's fertility and mortality; one herd per region, founded when
the region becomes known; up to two lairs per region (Ice and Fire's set density and chance), none
within about 1,000 blocks of spawn; dragons hunting the 3 x 3 regions around their lair; villages
(beds 6 to 20), Ribbit villages (houses 4 to 12), dens, groves with their Umvuthi, nests, camps of
the three cultures with feuds, and pirate havens with two slots and three merchant lanes each.

Players are scripted: **none** (unattended), **homebody** (loads the 3 x 3 regions around spawn and
shoots dragons within two regions), **roamer** (explores outward to radius 6 and kills what it finds:
dragons, Frostmaw, groves, nests, camps, ships), **farmer** (kills every arrival at one site every 2
days for 60 years). Shocks: kill all of a kind in the known world at year 20, then the player stops
killing. 24 seeds for the steady runs, 16 for the rest, 60 years; 8 seeds for 262,980 days unattended.

`game.py` is the strategy model: one faction against one player base for 50 years over 64 seeds,
`ladder` mirroring today's conflict engine and `gate` applying §2.5, against four players: fortress
(defence 80), weak (6, pays when the demand is below its expected loss), stubborn (6, never pays) and
adaptive (starts at 12, adds 6 after each lost defence, sheds 1 per 24 quiet days).

`run.py` runs everything (`nice -n 10 python3 run.py --jobs 4`, about 5 minutes) and `summarize.py`
writes `results/summary.md`. The model's `den_base` default is still 24; the base-6 numbers for EQ-4 (b)
come from the same runners with `{"den_base": 6.0}` (`run.farm(("proposed", "den", 16, 60, 31,
{"den_base": 6.0}))` and `run.killall(("proposed", "dens", 16, 60, 21, 20, {"den_base": 6.0}))`, run on
2026-10-21 after Ben's answer).

### 3.2 Herd births: Beverton-Holt

Yearly map for adults A (last year's young join at the birth day): `A' = S_a A + S_y f A / (1 + A/M)`,
with `S_y = e^(-m_young)`, `S_a = e^(-m_adult_and_old)`. The fixed point is
`A* = M (S_y f / (1 - S_a) - 1)`, and with the young of the year the post-calving total is
`K = A* (1 + (1 - S_a)/S_y)`, so M is set per species as
`M = K / ((S_y f/(1 - S_a) - 1)(1 + (1 - S_a)/S_y))`. The map's slope at A* is
`S_a + (1 - S_a) / (1 + A*/M)`, which lies in (0, 1): the map is monotone and A* is globally stable,
with no overshoot and no cycle. Growth from low density is `λ = S_a + S_y f` a year: sheep 1.33, cow
1.26, pig 1.72, horse 1.2, goat 1.41. From a founding group of a twentieth of K to half of K takes about
5 years for pigs and 15 for horses with no predator, longer with dragons: **herd recovery is set by
biology** (one calving a year), not by any delay in this design.

Without the density term (today), births grow without bound until forage starvation stops them, which
is a delayed, overshooting feedback: the engine's 41-year run swings between 40,000 and 117,000 animals.

### 3.3 Pressure under farming

A site of K members, each arrival killed within c days: each kill adds `1/K`, so at the steady state
`P* = (1/K) / (1 - 2^(-gap/H))` with `gap = min(cap, base x 2^P*) + c`, one kill per gap. The right
side falls as P rises, so there is one root (`run.py: closed_form`, bisection). With c = 2 days and H 96:

| Site | base, cap (days), K | P* after a kill | days per arrival | arrivals a year | model, 60 years farmed |
|---|---|---|---|---|---|
| Lair (today's base) | 20, 192, 1 | 2.12 | 89 | **1.08** | 1.12 (today: 4.80) |
| Frostmaw den (EQ-4 (b), Ben 2026-10-21: MOWZIE's base 6 with pressure) | 6, 192, 1 | 3.11 | 54 | **1.79** | 1.83 |
| Frostmaw den, base 24 (EQ-4 (a), not chosen) | 24, 192, 1 | 1.99 | 97 | 0.99 | 1.02 |
| Frostmaw den, MOWZIE's draft (6 days flat, EQ-4 (c)) | 6, 6, 1 | none | 6 to 8 | 12 to 16 | 16.0 |
| Umvuthana grove, per warrior | 6, 96, 7 | 1.27 | 16.5 | **5.8** warriors | 6.05 (flat 6 days: 12.1) |
| Village settlers, pairs | 16, 192, 12 (a pair adds 2/12) | 0.84 | 30.6 | **3.1** pairs | 6.6 villagers |
| Den with H 48 (base 24) | 24, 192, 1 | 1.55 | 72 | 1.33 | 1.35 |
| Den with H 192 (base 24) | 24, 192, 1 | 2.52 | 140 | 0.69 | 0.72 |

The model's farmer checks every 2 days, so its kill comes 0 to 2 days after an arrival; that is why the
6-day flat den gives 16 a year in the model and 12 in the closed form with c = 2.

The base sets the waits a site sees when players leave it alone; under steady farming the yield is
set mostly by H: a den with base 6 and
pressure yields 1.8 a year, with base 24 one a year, against 12 a year for 6 days flat.

**A site of K 2 or more farmed to empty chains its waits** (found building S10, 2026-10-22). §2.2
starts the next wait when a member arrives and the site is still short, so a farmer who kills every
member keeps the site short, and its waits run back to back from arrivals; the kill's c days overlap
the next wait instead of adding to it. The gap is then `min(cap, base x 2^(P* x 2^(-(gap - c)/H)))`
with the same P*, shorter than the table's. For an oil nest (base 12, cap 96, K 2, c 2) the table's
form gives 44.4 days a worm and the chained one 37.6; the engine's farmed test matches the chained
form within 5% (`a_farmed_nest_renews_at_the_chained_rate`). A K 1 site, or one farmed a member at a
time, still reads the table; the table's rows with K above 1 (the grove, the villages) are worth
checking against a farm-to-empty run before their numbers are relied on.

### 3.4 Predator and prey

Prey switching with a refuge: dragon d hunting herd i takes in proportion to `w_i = (N_i - R_i)^2 / K_i`
plus the alternative food `a` (village livestock and caravans), with catch per day
`eat x W/(W + 200)`, `W = a + sum w_i`. Two properties hold by construction:

- **No extinction by predation:** `predate` never takes a herd below `R = max(8, 0.1 K)`, and a herd at
  R has `w = 0`, so dragons stop hunting it before it gets there; births from R grow it at λ.
- **Dragons do not starve where villages are:** with `a = 300` the catch is at least 0.6 of need near a
  village, above the arrive-food gate 0.5, so P12's fallback to villages and roads holds them.

Type III switching alone (no refuge) stops most extinctions at the engine's appetite but not at 16
times it (§3.6); the refuge is what makes extinction impossible.

### 3.5 The gate

Ben rejected the gate (EQ-5, 2026-10-21). This section stays as the measurement of options (a) and
(c) that `CONFLICT-LIFE.md` compares its own model against.

`game.py` runs one goblin faction (pool 60, recruits 2 a day, today's rung thresholds, dwells and
party sizes) against one player base for 50 game years over 64 seeds. σ is the grievance the base's
industry adds each day; with today's 6-day grievance half-life, σ 6, 9 and 12 hold grievance near 55
(the scouts rung), 83 (raid) and 110 (raid). Each goblin the player kills adds 8, as today.

Per year over the last decade, σ 9:

| Player | Ladder (today) | Gate |
|---|---|---|
| Fortress: defence 80, never pays | 13.8 wars; 192 goblins killed; 53 loot lost | 0 wars; 4 scouts killed; nothing lost |
| Weak: defence 6, pays when the demand is below its expected loss | 21.6 wars; 1,135 loot lost; no tribute (the envoy rung is passed once, on the way up) | 0 wars; 293 tribute |
| Stubborn: defence 6, never pays | 21.6 wars; 1,134 lost | 19.4 wars; 1,015 lost |
| Adaptive: starts at 12, adds 6 after each lost fight, sheds 1 per 24 quiet days | 13.7 wars; 192 killed; defence climbs from 68 to 145 | 0.8 wars; 144 tribute and 25 lost; defence 14 to 19 |

At σ 12 the picture is the same (ladder: 10.8 wars a year at the fortress, the adaptive player's
defence climbing to 160; gate: none at the fortress, the adaptive player flat at 19). What it shows:

- **No suicide raids.** The ladder throws about 190 goblins a year at a base it cannot beat. The gate
  sends scouts every 48 days, learns the base is strong, and stays home.
- **No ratchet.** Against the adaptive player the ladder starts an arms race: his defence doubles over
  40 years and the wars never stop. Under the gate his defence and the faction's behaviour settle after the
  first decade (defence 17.6 to 18.6, tribute 143 to 159 a year from the second decade to the fifth):
  a stationary outcome in which he pays terms he can afford.
- **No empty threats.** At σ 6 the ladder's envoys take 270 a year from a player who would pay,
  although the faction never reaches a war rung; the gate demands only when a war is worth sending.
- **A weak player who refuses everything is raided almost as often as today:** that is the faction's
  best response to him, and his own best response is to pay or to build.
- **The gate is not cheaper for every player.** The adaptive player pays about 170 a year under the
  gate against 15 in loot under the ladder; the ladder's figure leaves out a defence that doubled and
  fourteen fights a year. The gate's terms are, by construction, below his expected loss from a war.

### 3.6 Results

Every number here is in `results/summary.md` (`run.py`; the committed run shared a loaded laptop with other
sessions and took 2.4 hours, `results/run-full.log`). A ratio is
population over capacity, averaged over the last 30 of 60 game years, written median [5th, 95th
percentile] over seeds. "Today" is the model's `baseline` mode.

**Steady state** (24 seeds; the homebody stays by spawn and shoots dragons within two regions, the
roamer explores and kills everything it meets):

| | Today: no player | Today: homebody | Today: roamer | Proposed: no player | Proposed: homebody | Proposed: roamer |
|---|---|---|---|---|---|---|
| Herds, N/K | 0.67 [0.45, 0.84] | 0.76 [0.53, 0.96] | 0.37 [0.31, 0.46] | 0.59 [0.48, 0.70] | 0.77 [0.56, 0.87] | 0.59 [0.52, 0.63] |
| Herd ranges alive | 0.90 | 0.85 | 0.91 | 1.00 | 1.00 | 1.00 |
| Herd ranges extinct per world, 60 years | 6.0 | 3.6 | 15.3 | 0 | 0 | 0 |
| Lairs held | 0.95 | 0.83 | 0.93 | 0.95 | 0.61 | 0.89 |
| Villagers over beds | 0.82 | 0.85 | 0.85 | 1.00 | 0.94 | 0.98 |
| Frostmaw dens held (proposed at base 24; base 6 holds more) | 1.00 | 1.00 | 0.00 | 1.00 | 1.00 | 0.93 |
| Grove warriors over cap | 1.00 | 1.00 | 0.00 | 1.00 | 1.00 | 0.94 |
| Nest worms over cap | 1.00 | 1.00 | 0.00 | 1.00 | 1.00 | 0.94 |
| Camp pools over cap | 1.00 | 1.00 | 0.97 | 1.00 | 1.00 | 0.97 |
| Camp-on-camp feuds per world-year | 6.3 | 2.3 | 20.2 | 0 | 0 | 0.34 |
| Pirate ship slots manned | 0.29 [0.00, 0.92] | 0.00 [0.00, 0.78] | 0.00 [0.00, 0.35] | 0.93 | 0.93 | 0.89 |
| The same, havens with a lane worth hunting | 0.31 | 0.00 | 0.00 | 0.92 | 0.93 | 0.87 |
| Renewal events per day, whole world | 0.45 | 0.27 | 1.41 | 0.51 | 0.38 | 1.83 |

- **Herds sit near 0.6 K, not at K, because dragons eat:** about 40% of a range's yearly production
  goes to dragons at today's appetite. The homebody who shoots the dragons near spawn lifts his herds
  to 0.77 K and holds those lairs empty (0.61 of lairs held, the pressure he builds there).
- **No range dies in any proposed run.** Today 4 to 15 ranges per world die in 60 years, even with no
  player, and the dead stay dead.
- **No winner-take-all:** the largest herd holds 4% to 14% of all animals in the proposed runs (medians
  by player; at most 19%); today, at four times the dragons' appetite, it holds 22% (median) and up to 49%.
- **Today's villagers over beds stays near 0.82** because villages generate with fewer villagers than
  beds and nothing grows an unloaded village. Where a player loads villages and sieges take villagers,
  the sim's own count runs 6% (homebody) to 15% (roamer) above the real one, since a census can only
  raise it.
- **Factions stop feuding by habit:** under the gate (rejected, EQ-5) a camp falls on a neighbour only when the
  neighbour is weak (0.34 feuds a world-year with a roamer thinning camps, none without).
- **Pirates survive:** today, with no player, the median world still has 0.29 of its slots manned in
  years 30 to 60 (0 to 0.92 across seeds) and none after the first century (below); with a player
  the median world has none left by year 30. Under the proposal the havens that hunt keep 0.87 to
  0.93 of their slots manned (all havens 0.89 to 0.93), so the figure is not carried by fleets that
  never sail.
- **Cost:** under two renewal events a day for a whole world, with a player killing everything.

**Kill all of one kind** at year 20 across the known world, then a peaceful player (16 seeds):

| Kind | Today | Proposed: days to 50% | Days to 90% | End over the control |
|---|---|---|---|---|
| Dragons | 24 days (the 20-day respawn) | 48 | 48 [48, 72] | 1.01 |
| Frostmaw dens (base 6, EQ-4 (b); base 24 gave 72 and 72 [72, 78]) | never | 24 | 24 [24, 24] | 1.00 |
| Umvuthana groves | never | 48 | 96 [90, 96] | 1.00 |
| Oil nests | never | 48 | 72 [48, 72] | 1.00 |
| Camps | 48 days (the linear refill) | 48 | 48 [48, 72] | 1.00 |
| Pirate fleets | never | 48 | 72 [48, 150] | 0.99 |
| Ribbit villages | never | 120 | 216 [192, 240] | 1.00 |
| Human villages | never | 228 | 480 [438, 534] | 1.00 |
| Herds, local wipe (the 3 x 3 regions at spawn) | never | 1,128 [792, 1,608] | 90% of the control: 2,724 [2,390, 3,596], 88% of seeds within 40 years | 0.94 [0.85, 1.02] |
| Herds, every herd in the known world | never | 1,368 [1,248, 1,656] | 90% of the control: 3,192 [2,873, 3,538], 94% of seeds | 0.92 [0.88, 0.94] |

Times are on the model's 24-day census grid. The control is the same seed with the same peaceful
turn and no kill, so "end over the control" compares like with like while a kind is still settling
from its founding (herds founded at K are still sinking toward 0.6 K at year 20). Every proposed kind
comes back to its control's level; herds take 12 (local) to 14 (world) game years to half, because
they grow by one calving a year (§3.2). Every herd range is alive at the end, 40 years after the
wipe, in every seed of both herd wipes; today a world wipe leaves none.

**Farming one site** for 60 years: §3.3's table (the model matches the closed forms within 5%).

**Unattended for ten real years** (262,980 game days, 8 seeds, by game century):

| Century | Herds N/K, today | proposed | Herd ranges alive, today | proposed | Lairs held, today | proposed | Pirate slots, today | proposed | Events per day, today | proposed |
|---|---|---|---|---|---|---|---|---|---|---|
| 1st | 0.48 | 0.63 | 0.90 | 1.00 | 0.94 | 0.94 | 0.08 | 0.93 | 0.38 | 0.46 |
| 2nd | 0.40 | 0.60 | 0.76 | 1.00 | 0.96 | 0.96 | 0.00 | 0.90 | 0.37 | 0.51 |
| 11th | 0.40 | 0.60 | 0.69 | 1.00 | 0.95 | 0.95 | 0.00 | 0.87 | 0.34 | 0.51 |
| 28th | 0.43 | 0.60 | 0.67 | 1.00 | 0.95 | 0.95 | 0.00 | 0.88 | 0.33 | 0.51 |

In the proposed run every kind holds the same band from the second century to the 28th: villages,
Ribbit villages, dens, groves, nests and camps at 1.00, pirates between 0.83 and 0.94 by century,
and renewal events per day between 0.505 and 0.508. Today a third of the herd ranges die out and
every pirate fleet is gone in the first century. Each run takes two to four minutes.

### 3.7 Knife edges

Each sweep changes one rule or number and runs 16 seeds for 60 years (`results/summary.md`, "knife-edge
sweeps").

| What changes | Result | Reading |
|---|---|---|
| Dragon appetite x1, x4, x16, x64, with the refuge | herds 0.57, 0.28, 0.19, 0.15 K; **no extinctions**; lairs held 0.89 to 0.80 | dragons can thin herds but not finish them; the refuge holds at any appetite |
| The same, refuge off (switching alone) | herds 0.53, 0.16, 0.06, 0.02 K; extinctions per world 0, 1, 11, 88 | the refuge is the rule that matters; switching alone fails from x4 |
| The same, today | herds 0.38, 0.19, 0.02, 0.02 K; extinctions 16, 99, 113, 113; at x16 the median world's largest herd holds half of all animals | today's balance is a knife edge on the dragons' appetite |
| Density term off (births as today, everything else proposed) | herds 0.83 [0.55, 1.01] K, above K in some seeds; 0.8 extinctions per world; the largest herd holds 19% | Beverton-Holt is what holds herds without extinctions |
| Forage headroom 1, 1.5, 4 | proposed: 0.70 K in all three; today: 0.33, 0.47, 1.08 K | today's herd level is set by forage tuning; the proposed one is not |
| Starve-leave at 0.2, 0.35, 0.5 (appetite x16) | no change in any band (0.06 extinctions per world at 0.5) | the threshold is safe to tune for feel |
| Frontier founding rate 0, 0.05, 0.25, 1 per range-year (world-wide herd wipe) | never back; 50% back in 2,520, 1,464 and 1,176 days; at 0.05 a range is still empty 40 years on in 81% of seeds | some frontier founding is required; past 0.25 it buys little, since births set the pace |
| Frontier group 12 animals, 0.05 K, 0.2 K (world-wide wipe) | 50% back in 2,040, 1,464 and 888 days | a larger group is faster and places more animals no birth explains (§5.2) |
| Dispersal donor and taker thresholds 0.8 K and 0.5 K vs 0.4 K and 0.25 K; 2% or 10% (local wipe) | with 0.8 K: 50% back in 1,656 (2%) and 1,464 (10%) days; with 0.4 K: 1,272 and 984 | a donor threshold above the predated steady state (0.6 K) slows a local refill by a third to a half; below it the rate matters more |
| Settlers' base delay 4, 16, 64 days (village wipe) | 90% back in 456, 468 and 576 days | village recovery is set by growth, not by the settlers' delay |
| Half-life H 48, 96, 192 days (farmed den) | 1.35, 1.02, 0.72 a year | H is the knob for farmed yield |
| Grievance input σ 6, 9, 12 (strategy) | ladder at a fortress: 0, 13.8, 10.8 wars a year; gate: 0, 0, 0 | the ladder flips from peace to constant losing wars where steady grievance crosses the raid rung (σ about 7.6); the gate does not |

### 3.8 What the model does not show

- **The engine's numbers.** The model has the rules' shape; the engine's bands come from the tests in
  §4, which check the engine against the same bands.
- **The economy.** No prices, caravans or treasuries: village pop is a count, alternative dragon food
  is a constant weight, plunder value is a constant.
- **Space below a region.** One herd per 512-block range, lairs on a grid, the player's view as whole
  regions. Watched-site deferral is not modelled (it only delays arrivals).
- **Seasons** beyond the birth and dispersal days.
- **Many players, and players who learn.** Players are scripted; the strategy model has one faction
  and one base, and the player types are fixed rules (the adaptive one is the only learner).
- **Raptors** (MOWZIE's predators) are not modelled: they take vanilla animals and goods, not herds.
- **Bodies.** No body is modelled, so kill credit (§2.2), missing members, strays and resets (§2.3)
  are settled by the GameTests of §4, not by the model.
- **Rules the model simplifies:**
  - oil nests renew whether or not they are drilled (the rule renews only while drilled);
  - a dragon that starves out of its lair leaves the sim (the rule lets it settle in a vacant lair
    that would feed it first);
  - no M1 breeding, so every newcomer is a Frontier one and no hatchling takes a lair;
  - no oil gate visitors (OILGATE §4.2);
  - camps read the pressure every day for their recruit rate (the rule reads it at the last event: a
    coarser form of the same curve);
  - camps see each other's true pools when they weigh a feud (no belief; `game.py` has the belief for
    faction against player).

---

## 4. Build plan

Ordered slices. Each lands with its tests; later slices measure themselves with S1's census. Every
long-horizon test runs a release build (`cargo test --release`), under `nice -n 10`, never under the
GameTest lock.

| Slice | What | Tests it adds (names are the build's) | Waits for |
|---|---|---|---|
| S0 | `rtsim-kernel::renewal` (§2.2) | `pressure_halves_each_half_life`, `delay_is_capped`, `farmed_steady_state_matches_closed_form` (simulate kill-on-arrival for 60 years against `RenewalRule::farmed`, within 5%) | nothing |
| S1 | `rtsim-cli sim census`: one line per kind every N days (sites, members, capacity, short, broken, mean P, renewals) for every module the scenario runs; the same hook in the host soak. **Built 2026-10-22** (`crates/rtsim-kernel/src/census.rs`, `crates/rtsim-cli/src/sim/census.rs`, `Engine::census`; docs/overnight-choices-equilibrium.md EQC1 to EQC8, which add an `empty` column) | `census_counts_every_kind`, `census_is_flat_with_no_player` (`crates/rtsim-cli/src/sim/census.rs`) | S0 |
| S2 | Herds: density term, refuge in `predate`, spring dispersal, frontier founding, tombstone removal. **Built 2026-10-22** (`crates/rtsim-ecology/src/herds/engine.rs`, `herds/synth.rs`'s ranges grid; docs/overnight-choices-equilibrium.md EQC9 to EQC21, and its S2 findings: both wipes and the forage sweep inside the model's bands on seeds 1 to 30) | `herds_settle_without_forage_tuning` (forage headroom 1 to 4 gives one band), `predation_never_takes_a_herd_below_refuge`, `kill_all_herds` (world wipe: half back within 25 years, all ranges alive by 40), `local_wipe_refills_from_neighbours`, the property tests extended (founding is a booked source) (`crates/rtsim-cli/src/sim/equilibrium.rs`, the 30-seed versions `#[ignore]`d; `crates/rtsim-ecology/tests/herds.rs`: `founding_and_dispersal_are_booked_sources`, `a_restored_ranges_engine_continues_exactly`) | S0, S1 |
| S3 | Site ledger (world game time, relative hand-over) and SITE_FOUND carrying it; kill credit in every bridge; missing members and strays; the no-unload census | GameTests `resetKeepsAFarmedSiteFarmed`, `siteFoundEmptyWithoutLedgerWaitsTheCap`, `reinstallKeepsPressure` (the engine's clock restarts, the wait does not shorten), `uninstallLeavesHomeBodies`, `petKillRaisesPressure`, `trapKillAtABossSiteIsAPlayerLoss`, `tameIsAPlayerLoss`, `aLuredBodyIsAPlayerLossAndAStraysDeathCounts`, `aForceLoadedSiteRefillsWhenUnwatched` | S0 |
| S4 | Dragons: lair pressure, fed share, starve-leave, arrival gate, newcomer direction, oil visitors only into due lairs. **Built 2026-10-22, but for the GameTest** (`crates/rtsim-ecology/src/engine.rs`, `dragon.rs`, `oil.rs`, `tunables.rs`, `persist.rs`; docs/overnight-choices-equilibrium.md EQC22 to EQC33, and its S4 findings: lairs held about 0.06 below the model's, the kill-all's 90% at 72 days at the median; S3 is not built, so a late-found lair starts at P 0 and `resetDoesNotMintADragon` waits for S3, EQC31) | `kill_all_dragons` (90% back within 72 days), `farmed_lair_yields_closed_form` (60 years, at most 1.2 a year), `starving_dragon_leaves_and_returns_when_fed`, `visitor_claims_only_a_due_lair`, `long_horizon_dragons_and_herds_with_kills` (60 years, roamer script, the §3.6 bands); GameTest `resetDoesNotMintADragon` (`crates/rtsim-ecology/tests/lair_renewal.rs`; `oil.rs`; `crates/rtsim-cli/src/sim/equilibrium.rs`, the 30-seed versions `#[ignore]`d) | S2, S3 |
| S5 | Dragon home bodies (EQ-2 a): vanilla while roosting, puppet away, lair census | GameTests `roostingDragonIsSavedByVanilla`, `awayDragonIsAPuppetAndTheCensusReconciles`, `uninstallKeepsRoostingDragons`, `aDragonLuredFromItsLairIsMissingNotDead` | S4 (EQ-2 a, decided) |
| S6 | Villages and Ribbits (EQ-1): census both ways, settlers, unloaded growth, Ribbit refill before build, pop on spawn | `kill_all_villagers` (settlers in 32 days, 90% within 600), `village_pop_follows_census_down`, `kill_all_ribbits` (90% within 300), `sim_quiet_stays_quiet_with_settlers`; GameTests `settlersWalkInFromTheExploredEdge`, `noArrivalIntoAWatchedVillage` | S1 (EQ-1 a, decided) |
| S7 | Conflict: pressure on recruits, break and refound; when parties go out is `CONFLICT-LIFE.md`'s build plan (EQ-5 rejected the gate) | `kill_all_camps` (90% within 96 days), `farmed_camp_breaks_and_refounds` | S0 |
| S8 | Ships: hires and cap, refit pressure, EV lanes. **Built 2026-10-22** (`crates/rtsim-ships/src/engine.rs`, docs/overnight-choices-ships.md T32 to T39) | `kill_all_pirates` (90% within 180 days), `crew_pool_is_bounded`, `a_haven_with_no_lane_worth_hunting_rests`, `pirates_survive_unattended_1000_years` (hunting havens inside their §3.6 band; the engine holds the floor and sits at 1.00, T37) (`crates/rtsim-ships/tests/equilibrium.rs`) | S0 |
| S9 | MOWZIE on the shared rule (in its plan, §2.6) | `kill_all_dens`, `kill_all_groves`, `farmed_den_yields_closed_form`, GameTests `resetDoesNotMintAFrostmaw`, `aLuredFrostmawIsAPlayerLoss`, `aStrayFrostmawsDeathRaisesItsDensPressure`, `aPetOrTrapKillAtADenIsAPlayerLoss`, `anUmvuthiHeirTakesAWarrior` | S0, S3, MOWZIE |
| S10 | Oil nests on the shared rule (with OG-9e's build). **Built 2026-10-22** (`crates/rtsim-ecology/src/nest.rs`, docs/choices-oilgate.md OGC70 onward) | `kill_all_nests` (regrow only while drilled), `asleep_nest_holds` (`crates/rtsim-ecology/tests/nests.rs`) | S0, OG-9e |
| S11 | The long-horizon suite | `long_horizon_stability_with_kills` (every module, 60 game years, the roamer script, every kind inside its §3.6 band), `kill_all_<kind>` for every kind above in one place, `who_is_alive_soak` (262,980 game days in the host at max: every kind's census flat by century, state bytes flat, renewal events per day flat; nightly; its length is the engine's speed, about 45 minutes if the host runs 100 game days a second) | all |

The pass bands in the test names are the model's 95th percentiles with margin; the build replaces them
with the engine's own bands from S1's census once S2 to S8 land.

---

## 5. Decisions

### 5.1 Questions for Ben

Each question lists its options with the recommended one first. Everything in §5.2 is decided and can be
undone; these cannot, or are Ben's (decisions of his this reopens, body ownership, a world-save format,
boss loot, how fights feel, an interface another plan must follow).

**Decided (Ben, 2026-10-21).** The options as they were put to him are kept below the table.

| Id | Question | Decided (Ben, 2026-10-21) | What follows |
|---|---|---|---|
| EQ-1 | How does an emptied village come back? | **(a) Settlers plus growth while unloaded.** | §2.4.3 and §2.4.4 as written: a family of two from beyond the explored edge 32 days after a player empties a village, growth toward its beds while unloaded, the same pair for Ribbit villages. C2 and M3/M4 are reopened to that extent. Slice S6 is unblocked. |
| EQ-2 | Where does a dragon's body live? | **(a) A vanilla body while it roosts at its lair, a puppet while away.** | P3 is reopened for roosting dragons (and, with MOWZIE §4.9, for dens and groves). An uninstall leaves the roosting dragons; a reset finds them by census. Slice S5 is unblocked. |
| EQ-3 | Where does the site ledger live? | **(a) World SavedData `mic_rtsim_sites.dat`, beside `mic_rtsim_villages.dat`.** | §2.3 as written: one file, about 32 bytes a site, outside `world/rtsim/`. |
| EQ-4 | How fast does a killed Frostmaw come back? | **(b) MOWZIE's 6 days, doubling with pressure** (not the recommended 24). | `min(192, 6 x 2^P)`: 12 days after a first kill, about 1.8 a year farmed (§3.3: 1.79 closed form, the model 1.83), a world-wide wipe back within one 24-day census (§3.6). §1.1, §1.2, §2.4.7, §2.6 and §3.3 carry the base-6 numbers; MOWZIE.md is being changed to match by its own author. |
| EQ-5 | When do factions fight? | **None of (a), (b) or (c).** "All of these systems feel a bit too formulaic, come up with something that feels more life like." Scope: only faction fighting; renewal and pressure stay as designed here. | `CONFLICT-LIFE.md` is the redesign and replaces §2.5; §3.5 stays as the measurement of (a) and (c) it compares against. Camps keep §2.4.5's pressure, break and refound. |
| EQ-6 | One renewal rule for every kind, in `rtsim-kernel`? | **(a) Yes: one shared rule in `rtsim-kernel::renewal`.** | §2.2 as written: `rtsim-kernel::renewal` owns pressure, delay and the closed forms; every engine and species plan (MOWZIE first, §2.6) uses it with its own numbers. |

**The options as put to Ben (EQ-1 to EQ-6).**

| Id | Question | Options | Why it is Ben's |
|---|---|---|---|
| EQ-1 | **How does an emptied village come back?** (reopens C2 "no villager bodies for now" and M3/M4 "growth is ribbit houses only"; covers Ribbit villages too, whose ribbits cannot breed) | (a) **Settlers plus growth while unloaded (Recommended):** a family of two from beyond the explored edge, 32 days after a player empties the village (16 if nothing a player did emptied it, longer after repeated killing); then the sim grows the village toward its beds while unloaded and places the difference as ordinary vanilla villagers at the next unwatched load. Back to 90% in about five game years unvisited (480 days in the model). Ribbit villages get the same pair, then refill through their houses. (b) Settlers only: a pair arrives, and only vanilla breeding while a player is near grows it; an unvisited village stays at two (a Ribbit village still refills through its houses). (c) Neither: only the census fix, so the economy sees the real count, and a dead village stays dead. | The sim would place villagers and ribbits, which C2 and M3/M4 set aside for later. None of the options founds new villages (that stays with C22 and P22), and none takes villagers from another village (§2.4.3). |
| EQ-2 | **Where does a dragon's body live?** (reopens P3, "Minecraft entities are puppets that are never saved by vanilla", for roosting dragons; MOWZIE §4.9 takes the same rule for dens and groves, so one answer can cover both) | (a) **A vanilla body while it roosts at its lair, a puppet while away (Recommended):** MOWZIE §4.9's rule; the engine still owns the counts, the pressure and every body away from home; an uninstall leaves the roosting dragons, and a reset finds them by census. (b) A puppet always, as today, plus the site ledger: a reset cannot mint, but an uninstall leaves every lair empty. (c) A puppet always and no ledger (today): a reset gives every empty lair a dragon 20 days later. | Body ownership is architecture, and (a) changes what an uninstall leaves in the world. |
| EQ-3 | **Where does the site ledger live?** | (a) **World SavedData `mic_rtsim_sites.dat`, beside `mic_rtsim_villages.dat` (Recommended):** one file, about 32 bytes a site, outside `world/rtsim/`, so a reset keeps it and an uninstall leaves one inert file. (b) A NeoForge data attachment on each site's chunk: travels with the chunk, but is read only when the chunk loads, and vanilla strips unknown attachments while rtsim is out (the reconcile-marker problem UR-1 describes). (c) No ledger: a site found empty after a reset waits the cap (192 days); a member that died away from its unloaded home comes back after a reset. | It is a format in the world save, and it decides what a reset can mint. |
| EQ-4 | **How fast does a killed Frostmaw come back?** (the base of the den's delay; MOWZIE's MZ-4 (a) states 6 days and defers to this, §2.6 item 5) | (a) **24 days, doubling with pressure (Recommended):** 48 days after the first kill, about one a year farmed. (b) MOWZIE's 6 days, doubling with pressure: 12 days after the first kill, about 1.8 a year farmed. (c) MOWZIE's draft, 6 days flat: 12 to 16 a year farmed. | The Frostmaw is a boss whose drops are loot; this sets their supply. |
| EQ-5 | **When do factions fight?** | (a) **The gate (Recommended):** a war party goes out only when it is worth sending against what the faction believes of the target; terms first; scouts every 48 days otherwise. A fortress sees scouts, never raids. Camps at strength also stop fighting each other: background feuds fall from 2 to 20 a world-year to 0 to 0.34 (§3.6), and a camp falls on a neighbour only once something has thinned it. (b) The gate plus one glory raid a season whenever grievance is at the raid rung, worth or not, and one raid a season on a hated neighbour camp, so a strong base still gets fights to win and the background wars go on; each costs the camp warriors it then recruits back. (c) Today's ladder: every rung in turn, with no check that a party can win. | It changes what a player who builds a strong base experiences ((a) gives him quiet, (b) a fight a season), and whether the world has wars of its own when no player is near. |
| EQ-6 | **One renewal rule for every kind, in `rtsim-kernel`?** | (a) **Yes (Recommended):** `rtsim-kernel::renewal` (§2.2) owns pressure, delay and the closed forms; every engine and species plan (MOWZIE first, §2.6) uses it, with its own numbers. (b) Each engine keeps its own rule, and only the event contract (§2.2) is shared. (c) Pressure only for boss sites (lairs, dens, the Umvuthi); everything else keeps a flat refill. | It is an interface that binds MOWZIE.md and every later species plan. |

### 5.2 Decided here (each can be undone)

| Choice | Why |
|---|---|
| Only player-caused losses raise pressure | So the world does not learn to empty itself: an unattended server stays full, and a dragon eating a herd or a camp losing a feud does not slow its own renewal. |
| Half-life 96 days (one game year), counted per site | A farmed one-member site settles at one or two arrivals a year (a lair 1.1, a den 1.8 on its 6-day base, §3.3); a lair left alone three years is back within a fifth of its first wait (a den within a third: 16 days against 12). H is the knob that sets the farmed yield. |
| A player's loss is vanilla's kill credit (his hit in the last 100 ticks, his pet), every A10 removal, and for boss kinds any death no wild mob is credited with | A10: a creature a player takes leaves the sim, a loss to its site. Pets, traps and turrets are how a player farms without swinging a sword; for the boss kinds an unclaimed death in a loaded chunk is almost always his doing, while for villagers and herds zombies and falls are common and stay unpressured. |
| A home body missing at a census is a player's loss while rtsim ran throughout; it stays tracked, and a stray's death counts | A body can leave home only while its chunk is loaded, which is a player's doing; this makes luring cost what killing costs, and keeps one body from being counted twice. |
| A running wait is not pushed out by more losses | One wheel entry per wait and no rescheduling; the pressure from the extra losses sets the next wait. |
| A site that never unloads gets its census and arrival when its wait ends | Otherwise a den or lair under a chunk loader would never refill. |
| No shunning (a site that refuses arrivals for good) | Pressure alone holds a farmed site to one or two a year, and a permanent "never again" is the extinction this design removes. |
| Beverton-Holt births for herds | A stable fixed point at K with no overshoot, whatever the forage tuning (§3.6: forage headroom 1 to 4 gives the same band). |
| Prey switching plus a refuge of max(8, 10% of K) | The refuge makes extinction by dragons impossible at any appetite tried (up to 64 times today's); switching alone does not (§3.7). |
| Starve-leave below 0.35 and arrival only at 0.5 of need | No effect on the bands in the sweeps (§3.7), and they keep a dragon from sitting in an eaten-out valley. |
| M1 kept; a dispersing hatchling takes a vacant lair before a frontier newcomer | Ben's M1; the hatchling is a source the player could see and is conserved, not minted. |
| The newcomer flies in from the nearest unexplored direction | A source in world terms: the player can watch it come. |
| Herd dispersal: 5% of the adults of herds above 0.4 K to neighbours below 0.25 K, each spring | Both thresholds sit below the predated steady state (0.6 K); with the first run's 0.8 K and 0.5 K dispersal seldom fired, and a local wipe took 15 to 17 years to half instead of 10 to 13 (§3.7). |
| Frontier herds: `max(12, 0.05 K)` animals at 0.25 per range-year; the tombstone, region mark and name guard go | A world-wide wipe is half back in 14 to 15 years (21 with 12 founders); with no frontier at all it never comes back (§3.7). |
| Herd refill speed stays biology's (one calving a year) | Faster means placing animals that no birth explains; a larger frontier group (a fifth of K) cuts a world-wipe's time to half by about 40% (888 days against 1,464, §3.7) and can be turned on later. |
| Village pop follows the census both ways | Today the economy believes in villagers that are dead; this is a fix as much as a choice, and it holds whatever EQ-1 had decided (Ben chose (a)). |
| Settlers are a family from beyond the explored edge; no village gives any | Taking two from a donor would mean removing vanilla villagers the player may have traded with, cured or named (C2, A10); villagers carry no loot, so a frontier family mints nothing that matters. |
| Growth inside a site (villagers breeding, a village's growth while unloaded, the Ribbit refill) and haven hires are not slowed by pressure | They are bounded by beds, houses and the crew cap and carry no boss loot; pressure throttles what comes from outside (settlers, migrants, newcomers, refits). |
| Ribbit residents booked when they exist; refill before building | Today pop is booked before the house exists, and houses never empty; growth stays Ribbit houses only (M3/M4). |
| A site is watched when a player is within 128 blocks | A chunk normally loads at the server's view distance, past 128 blocks, so a body placed as it loads is there before it can be seen; a player within 128 blocks at a load (a login or a teleport) could see it appear, so the arrival waits. A war party on the march is not an arrival (MZ-15): its bodies come where its march puts them, stack by stack at the entity-ticking edge (128 to 144 blocks off along an axis at a simulation distance of 8, 160 to 176 at 10), having walked toward the player from about a kilometre in the far view, and only the Umvuthi, at the rear, can be seen to appear (128 to 132 blocks off at 8); one that reaches a watched target bodiless walks in along its route from the ticking edge. |
| Camps break at pressure 2 with the pool under 10%, and refound after `min(384, 48 x 2^P)` days with a quarter of the cap | A camp the player keeps emptying goes quiet for months instead of feeding him warriors for ever; only players break camps. |
| Pirate crew pools capped at 18 and fed by hires at 0.25 a day; refit after `min(96, 4 x 2^P)`; a haven with no lane worth hunting rests | Today the pool has no source but captures, so fleets die out; the cap keeps a haven from hoarding crews; a haven that rests is the best response to strong lanes, and a lane's odds move with escorts and navy. |
| Merchant and navy ships unchanged | They are the economy's carriers, not a population; their 2-day refill already works. |
| ~~The gate's numbers~~ (gone with EQ-5's answer; CONFLICT-LIFE.md sets its own) | Ben rejected the gate, 2026-10-21. |
| Natural Death Worms and natural dragons stay outside the sim | R18a and OG-1: the sim adopts only what it places or finds at sites. |
| Notables unchanged | Already a generational arena; dead notables cost nothing. |
| Districts are made only by players | §2.4.10: making them would place machines (D3) and mint the player's kind of property (UR-1). |
| A ledger row that does not parse is dropped; left-behind counts are clamped to what the census finds | UR-1: no format promises; a world that ran without rtsim may have lost the bodies already. |
