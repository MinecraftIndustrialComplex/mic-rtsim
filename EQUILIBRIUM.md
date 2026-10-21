# Equilibrium and renewal: worlds that settle, and kinds that come back

**Status:** design for Ben's review, 2026-10-21. Nothing is built and no engine code changes here.
Ben approves this design before code. Branch `plan/equilibrium`, based on main `f20234e`.

**Ben's request (2026-10-21):** "Ensuring worlds actually reach a nash equilibrium between tribes /
animals, etc. It would also be nice if we had abilities for mobs to spawn back in. I.E. If the player
kills off all of the Yeti's that currently exist, having a way for new ones to spawn. Same with
dragons, villages, etc."

**Binding here:** D1 (judge only by late-game runtime cost on a weak PC: closed forms, scheduled
events, per-site bookkeeping, no per-tick scans, no cost that grows with time or dead history), D3 (no
bespoke entities, blocks or items), D4 (plugins per mod), P3 (the engine owns the truth, puppets are
never saved), A10 (a creature a player takes leaves the sim), C2 (no villager bodies for now), C22
(villages join at structure generation), M1 (dragon pairs breed in their lair), P12 (what dragons
prey on), R18a and OG-1 (natural dragons left alone), OG-9e (the oil nest with a Death Worm body),
UR-1 (rtsim can be removed and reinstalled at any time, and no backward compatibility before a
release).

**Inputs:** `research/population-inventory-2026-10-21.md` (how every population is born, dies and
comes back on main today, with citations); `MOWZIE.md` on `plan/mowzie` at `be38b77` (read, not
edited); the pack's jars as read for the inventory and for this design (§2.4).

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
  each unit of pressure, up to a cap. A den farmed without pause yields about one Frostmaw a year (the
  MOWZIE draft gives 12 to 16); a lair farmed without pause about one dragon a year (4.8 today). Leave
  the site alone for a year or two and it is back to its first wait.
- **Herds settle.** Births slow as a herd fills its range (a Beverton-Holt term), so herds sit at a
  steady share of their range whatever the forage tuning, with no winner-take-all and no extinctions
  in 60 years of any scenario. Today a 41-year run swings between 40,000 and 117,000 animals while one
  herd takes over and others die out.
- **Dragons follow food, and never finish a herd off.** Dragons switch to whatever prey is common, and
  cannot take a herd below a tenth of its range's capacity (a refuge: the animals in rough ground).
  A dragon whose hunting ground is eaten out leaves; a newcomer settles only where there is food.
- **Villages come back, and the economy sees the real number of villagers.** A village emptied of
  villagers gets a pair of settlers after 16 days (longer after repeated killing) and grows back
  toward its beds. Today a dead village stays dead while the economy believes it is full.
- **Factions pick fights they expect to win.** A war party goes out only when the faction expects to
  gain more than it loses against what it believes of the target's defence. A strong base sees scouts
  and no raids; a weak one is first offered terms it can afford; a player who refuses and stays weak
  is raided at a steady rate. Today's grievance ladder sends raid after losing raid at a fortress (13
  to 14 wars a year in the model) and drives an arms race against a player who keeps building walls.
- **Pirates no longer die out.** Havens hire crews up to a cap and hunt only routes they can win.
  Today a haven's crew pool refills only from captures, so a fleet that loses its ships is gone.
- **A server left alone for ten real years keeps every kind alive at a steady share,** and the number
  of renewal events per day stays flat (§3.6). Today a fifth of the herd ranges die out over that
  time with no player at all, and every pirate fleet is gone within the first game century.
- **Nothing pops in front of the player.** An arrival is placed when its site loads, or walks in from
  its source while a player watches; it is never conjured inside a site a player is looking at.
- **Nothing new in the world.** Every body, structure and item is the pack's (D3).

### 1.2 Walkthroughs

**Killing every Frostmaw in a region.** Ben clears the four dens of a snowy range in a week. Each den's
pressure goes to 1 (§2.2), so each will receive a migrant 48 days after its kill (`24 x 2^1`). Each
Frostmaw's notable dies and its heir takes the grudges (MOWZIE §4.2), so the range's anger at the rail
line does not end with the bodies. On day 48 or later, the first time each den loads with no player
inside 128 blocks, a Frostmaw lies asleep at the den's spot; MOWZIE's crystal question (MZ-4) decides
whether it carries one. Ben kills all four again on day 50: pressure is now `1 + 2^(-50/96) = 1.70`,
so the next migrants come after 78 days. A third round lifts it to about 2 and the wait to about 95
days; held there, each den yields about one Frostmaw a year (§3.3: 0.99 a year, the model 1.02). If he
leaves the range alone for a year the pressure halves and the next kill brings a migrant in 68 days;
after two quiet years, 48 days. What he sees in the census (`rtsim-cli sim census` or the viewer): the
dens marked empty with their due days, and the range's heirs.

**Farming dragons at one lair.** Ben camps at a fire roost and kills each dragon as it lands. Today the
lair draws a new 30-day-old dragon from 3,000 blocks away 20 days after each death, with no limit:
4.8 dragons a year. Under this design the first newcomer still comes after 20 days, the second after
about 40, and the waits settle near 89 days: about 1.1 dragons a year (§3.3: 1.08 closed form, 1.12 in
the model). Taming one counts the same as a kill (A10: it leaves the sim; the lair's pressure rises).
Each newcomer flies in from the nearest unexplored direction, so Ben sees it come; if a pair at a lair
within reach has raised a hatchling of dispersal age (M1), that hatchling takes the lair instead. If
Ben also eats out the hunting ground (kills the herds in the nine regions around the lair), the lair
stays empty until the herds or a village's livestock can feed a dragon again (§2.4.1).

**Wiping out a village.** Ben kills all twelve villagers of a plains village. The next census on load
counts zero, and the sim's population follows it down (today a zero census is ignored), so the
village stops consuming and its market goes quiet. The village's pressure is 1, so 32 days later two
settlers come: from the nearest village within 1,536 blocks that has at least four villagers and is
more than 60% full (it loses those two), or, if none, a family from beyond the explored edge. If Ben
is nearby they walk in from that direction; otherwise they are in the village at its next load. From
two, vanilla breeding refills the village while a player is near (beds and food, vanilla's rules), and
the sim grows it toward its beds while it is unloaded (§2.4.3). Unvisited, it is back to 90% in about
15 months of game time (456 days in the model). If Ben keeps killing every pair, the pairs keep coming,
more slowly: about three pairs a year. Zombie sieges, raids and dragons do not add pressure; only
player kills do. This walkthrough depends on question EQ-1.

**A server left alone for ten real years.** Nobody logs in from the day the last player leaves. No
chunk loads, so everything runs abstract. Dragons age out at 400 days and their lairs stand empty 20
days before a newcomer arrives: 95% of lairs are held at any time. Herds hold about 60% of their
ranges' capacity, every range alive. Villages, dens, groves, nests and camps stay full. Pirate havens
keep 83% to 96% of their ship slots manned: they lose ships to merchants and refit. The number of
renewal events per day is the same in the 27th game century as in the first (§3.6). When a player
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
- **Delay.** When a site is short (n < K) the next member arrives after `min(cap, base x 2^P)` days,
  with P read at the moment the wait starts. Members arrive one at a time; when one arrives and the
  site is still short, the next wait starts. One scheduled event per arrival.
- **Source.** Where the member comes from, in world terms:
  - **Heir:** from inside the site (a hatchling of the lair's pair, a grove warrior taking up the
    Umvuthi's mask, a village's own children by vanilla breeding).
  - **Neighbour:** from a site of the same kind in reach that has more than it needs, which loses the
    member (settlers from a full village, a herd's surplus walking to a thin range, a dragon's
    hatchling dispersing to a vacant lair). Conserved: a move, not a mint.
  - **Frontier:** from beyond the explored edge, when no neighbour can give (a dragon from 3,000
    blocks away, a Frostmaw migrant from the range's unexplored side, a herd of 12 founders, a family
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
- **Bounds.** n never exceeds K. The sim never creates a site: sites are the structures the world
  generated (C22), or the herd ranges the world model sampled. (The unmerged oil gate branch's
  `PLACE_LAIR` places Ice and Fire's own roost near oil fields; it is bounded by the oil fields players
  visit, and it is that branch's decision.)

**Why this rule.** Under constant farming (each arrival killed within c days) the pressure settles at
`P* = (1/K) / (1 - 2^(-gap/H))` with `gap = min(cap, base x 2^P*) + c`. The base sets the first wait;
H sets the farmed yield (§3.3). A site left alone forgets in a few years. Every quantity is a closed
form: no tick, no scan.

**Shunning** (a site that refuses arrivals for good after enough kills) is unnecessary: a farmed K=1
site settles near P = 2, about one arrival a year. I leave it out (§5.2).

#### The interface species plans use

`rtsim-kernel::renewal` owns the maths, so every engine, the model and the tests agree. The fields
live in each engine's own site rows (the dragons' `Lair`, the conflict `Faction`, the herds' range, the
village `Settlement`, the ships' `Haven`), never in a shared table.

```rust
// crates/rtsim-kernel/src/renewal.rs (to be written; signatures, not code)

/// Recent player-caused losses at one site: P(t) = p * 2^(-(t - at) / H). 8 bytes.
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
pub enum LossCause { Player, Age, Starved, Faction, Predator, Other }
```

**The event contract.** Each plugin keeps its own opcodes; these are the fields they must carry.

| Contract | What it carries | Existing events it extends |
|---|---|---|
| SITE_FOUND | site key, kind, K, and the site ledger row if one exists (§2.3) | LAIR_FOUND, FACTION_FOUND, VILLAGE_ADOPTED, HAVEN_FOUND, the nest's (OG-9e) |
| SITE_LOSS | site, count, `LossCause`, actor (the player, when a player) | DRAGON_DIED, RESIDENT_KILLED, HERD_KILLED and HERD_TAKEN, BAND_CASUALTIES, SHIP_SUNK and SHIP_CAPTURED, MOWZIE's deaths |
| SITE_CENSUS | site, members seen at home on load | VILLAGE_CENSUS, MOWZIE's HOME_CENSUS, a new lair census (§2.3) |
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
| Dragons | **vanilla, roosting in its lair** (today: a puppet always; EQ-2) | puppet (hunting, raiding, flying) | the roosting dragons stay, as Ice and Fire left them | lair census on LAIR_FOUND; the ledger's pressure |
| Herds | puppets near players (unchanged) | puppets | the sim's herds go; vanilla's own animals stay | re-founded from the world model, as today |
| Villagers and settlers | vanilla (C2: the sim owns none) | n/a | stay | VILLAGE_CENSUS |
| Ribbit residents | vanilla (`ResidentSpawner`) | touring bands are puppets | stay | census |
| Frostmaw, Umvuthana, Umvuthi | vanilla (MOWZIE §4.9) | puppet in parties | stay | HOME_CENSUS; the ledger |
| Death Worms at an oil nest | vanilla (Ice and Fire's worm is persistent) | never leaves | stays | nest census; the ledger |
| Goblin, bandit, raider | no home bodies in the pack (§2.4.5) | puppet parties | n/a | the ledger's pool |
| Pirates | no plugin yet | puppets | n/a | the ledger's crew pool |

**The site ledger.** One small row per site in world SavedData, `mic_rtsim_sites.dat`, beside the
existing `mic_rtsim_villages.dat` and outside `world/rtsim/`, so a reset does not delete it: site key,
kind, K, the sim's last count, pressure (P0, t0), the due day, and the members left behind (members that
left an unloaded home and died away, whose saved vanilla body the next load's census must remove).
About 32 bytes a row; one row per generated site, so it grows with exploration, never with time. The
Java plugin writes a row when the engine sends SITE_STATE, and SITE_FOUND carries it back after a
reset. Format: none promised before a release (UR-1); a row that does not parse is dropped.

**Why it stops the mint.** After a reset the engine rediscovers each site on load. With the ledger it
resumes the pressure, the due day and the left-behind count, so a farmed den is still farmed and a
Frostmaw killed away from its unloaded den does not reappear. A site found empty with **no** ledger row
starts at the cap wait (`Pressure::at_cap`): an emptied site whose history is lost is treated as
farmed, never as fresh. Today a reset re-derives a lair as "found with no dragon" and gives it a dragon
after 20 days.

**What an uninstall can still do,** as UR-1 allows: a member that died away while its home was
unloaded comes back if the player kills its saved body before reinstalling. Members away at the moment
of an uninstall are lost (puppets). Nothing else.

### 2.4 Per kind

Each kind below gives: the site and K; what counts as a loss and what raises pressure; renewal (rule,
source, arrival); bounds; rates (every number TUNABLE, starting values in brackets); what the player
sees; body; cost. Rates are the model's (§3); the engine's names are given where they exist.

#### 2.4.1 Dragons (Ice and Fire)

- **Site:** the lair (roost or cave), K = 1 occupant. M1's mated pair stays as built: at most a pair
  per lair, so dragons are at most two per lair, and lairs are Ice and Fire's structures.
- **Losses:** age (400 days, `lifespan_days`); a player kill (DRAGON_DIED with a killer) or taming
  (A10), each `P += 1`; **starve-leave:** a dragon older than 60 days whose lair's fed share (a 30-day
  moving average of how much of its need it catches) falls below 0.35 leaves; it settles in a vacant lair
  in reach whose fed share is at least 0.5, else it flies off the map and leaves the sim.
- **Renewal:** when the lair stands empty, after `min(192, 20 x 2^P)` days (base 20 is today's
  `respawn_delay_days`) the lair is ready. A newcomer settles only if the lair's hunting ground would
  feed it (fed share at least 0.5), else the lair checks again every 24 days. **Source:** a hatchling
  of dispersal age from a lair within reach (M1, Neighbour) if one exists, else today's 30-day-old
  newcomer from 3,000 blocks (Frontier), now flying in from the nearest unexplored direction instead
  of a random one.
- **Predation (P12):** a dragon picks among herds with weight `(N - R)^2 / K` (type III: it switches
  to what is common and leaves what is scarce), where `R = max(8, 0.1 K)` is the herd's **refuge**,
  the share it cannot reach. `predate` never takes a herd below R. Village livestock and caravans stay
  on the menu (weight 300 near a village, 30 elsewhere, TUNABLE), so a dragon in an eaten-out valley
  turns to villages and roads, as P12 wants. Its catch per day is `W / (W + 200)` of its need.
- **Bounds:** at most one occupant (or one pair) per lair; lairs are found, never made.
- **Rates:** base 20 days, cap 192, H 96; starve-leave 0.35, arrive-food 0.5, refuge 10% (min 8).
- **What the player sees:** a killed dragon's lair stands empty 20 days the first time, about 89 days
  under steady farming; a lair whose valley is eaten out stands empty until the herds come back, and
  its dragon is seen leaving.
- **Body:** vanilla while roosting at its lair, puppet when it leaves (EQ-2). Today a puppet always.
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
- **Renewal:** (1) **spring dispersal (Neighbour):** each spring, 2% of the adults of a herd above
  0.8 K walk to a neighbouring range below 0.5 K, and found it if it is empty (they carry their
  species); (2) **frontier founding:** an empty range with habitat is re-founded by 12 animals at a
  rate of 0.25 per range-year (one draw per empty range, scheduled), from the direction of the
  nearest live herd or the explored edge. The tombstone, the permanent region mark and the name guard
  go: an extinct herd's row is reused.
- **Bounds:** K per range, ranges fixed by the world model.
- **Rates:** dispersal 2%, thresholds 0.8 K and 0.5 K, founders 12, frontier 0.25 per range-year
  (EQ-6), refuge 10% (min 8).
- **What the player sees:** herds that hold a steady size; a valley the player hunted out sees
  stragglers arrive the next spring and fills over years, not days (§3.6: births are once a year).
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
- **Losses:** a villager a player kills or takes adds `1/K`. Zombies, raids and dragons add nothing.
- **Renewal (EQ-1):** (1) **settlers:** a village below 2 villagers (vanilla breeding needs two) gets
  a pair after `min(192, 16 x 2^P)` days, from the nearest village within 1,536 blocks that has at least
  4 villagers and is above 60% of its beds (Neighbour: it loses the two), else a family from beyond the
  explored edge (Frontier); WalkIn from that direction when a player is near, OnLoad otherwise. (2)
  **unloaded growth:** while unloaded, the sim grows the village toward its beds as a logistic in closed
  form, `dN/dt = g N (1 - N/K)` with g 0.01 per villager-day, read at the next load; the deficit is
  placed as vanilla villagers at the next unwatched load (they take professions from workstations as
  vanilla's do).
- **Bounds:** pop at most K (beds); villages are worldgen structures (C22).
- **What the player sees:** an emptied village gets two settlers after a month and grows back over a
  year; a murder-farmed village keeps getting pairs, about three a year.
- **Body:** vanilla. Settlers and grown villagers are ordinary villagers once placed; the sim owns
  none of them (C2 holds in that sense; EQ-1 asks Ben whether that is what C2 meant).
- **Cost:** one wheel entry per village below 2; the logistic is evaluated at load. The village pop
  changes on census, a rare event, so `sim quiet` stays quiet (tested, §4).

#### 2.4.4 Ribbit villages

- **Site:** the Ribbit village; K = its houses (the adoption count plus houses grown, at most 8 more).
  Ribbits do not breed (an AgeableMob with no breeding), so the sim is their only source.
- **Renewal:** (1) **refill before build:** on each prosperous growth clock (10 days, today's
  `growth_after_days`), a village with fewer residents than houses gets a resident into an existing
  house (today's `SpawnResident` path) instead of a new house. (2) **settlers:** below 2 residents, a
  pair after `min(192, 16 x 2^P)` days, from a Ribbit village in reach, else the frontier, as humans.
- **Fix:** book `pop` when the resident exists (the census), not when the house is ordered
  (`ribbits.rs:389-391`).
- **Bounds:** residents at most houses, houses at most the adoption count plus 8.
- **What the player sees:** killed ribbits are replaced one per 10 prosperous days; an emptied village
  is back to 90% in about 7 months (216 days in the model).
- **Body:** vanilla, as today. **Cost:** unchanged clock; one wheel entry while below 2.

#### 2.4.5 Conflict factions (goblin, bandit, raider)

- **Site:** the camp; K = the culture's `pool_cap` (goblin 60, bandit 40, raider 50).
- **Losses:** warriors killed by a player, at home or in a party, add `killed/K`; war-party losses to
  another faction or a village add nothing.
- **Renewal (Regrow):** recruits at `recruits_per_day / 2^P` (goblin 2, bandit 1.5, raider 2): word
  gets around that the camp is a death trap. Closed form between events: P is read at the last event
  that touched the pool (a muster, a loss, a census), so the pool stays linear in time.
- **Break and refound:** a camp whose pool is below 10% of its cap while P is at least 2 breaks: its
  survivors scatter and its node goes quiet. After `min(384, 48 x 2^P)` days a new band takes the empty
  camp with a quarter of the cap (Frontier). A faction can never break another; only players do.
- **(b):** war parties and plunders go through the gate (§2.5).
- **Bounds:** pool at most its cap; factions at most the camps found.
- **What the player sees:** a camp he keeps emptying recruits more and more slowly, then breaks and
  stands empty for months; feuds between camps become opportunistic (a camp falls on a neighbour the
  player or a dragon has just weakened).
- **Body:** there is no camp mod in the pack (Goblin's Tyranny is absent; the camp structure ids in
  the conflict culture files are unverified), so only parties have bodies (puppets). No Java exists
  yet for conflict; these rules bind the engine now and the plugin when it is written.
- **Cost:** one more float per faction (pressure); break and refound are one event each.

#### 2.4.6 Pirates and ships

- **Site:** the haven; K = its ship slots (one per medium today) and a crew pool capped at 18 (three
  sea crews). Today the pool has no cap and no source but captures.
- **Renewal:** (1) **hires (Regrow):** the pool grows by 0.25 per day toward the cap, logistic
  (`r (1 - pool/cap)`): drifters at the haven's market. Captures add as today, up to the cap. (2)
  **refit:** an empty slot launches after `min(96, 4 x 2^P)` days (base 4 is today's
  `haven_rebuild_ticks`) once the pool holds a crew. A ship a player sinks or captures adds `1/slots`.
- **(b):** a pirate hunts only a lane whose expected value is positive (win chance times loot, less
  loss chance times its crew's value); a lane escorted beyond that is left alone, and with no such lane
  it rests in port.
- **Merchant and navy ships:** unchanged (back in their slot 2 days after a loss, unconditional). They
  are the economy's carriers, not a population.
- **What the player sees:** sinking a haven's ships quiets its sea for a season, not for ever.
- **Body:** no ships plugin exists; puppets when one does. **Cost:** one float per haven.

#### 2.4.7 Frostmaw and Umvuthana (MOWZIE)

The MOWZIE plan models both as conflict cultures; this design sets their renewal (MOWZIE §4.10 asks
for it). §2.6 lists exactly what MOWZIE.md must change to match.

- **Frostmaw den:** K = 1. A Frostmaw a player kills (at home or away) adds 1. After
  `min(192, 24 x 2^P)` days a migrant from the range's unexplored side (Frontier) is placed asleep at
  the den's spot at the next unwatched load (OnLoad; MOWZIE's HOME_SPAWN). The chief notable succeeds
  at once (records only), as MOWZIE has it. Base 24 against MOWZIE's 6 is EQ-4.
- **Umvuthana grove:** K = the census count at first load (5 to 9). Warriors refill one at a time,
  each after `min(96, 6 x 2^P)` days, with `P += killed/K`; from the grove's edge farthest from any
  player when loaded (WalkIn), or at the next load (OnLoad).
- **The Umvuthi (Heir):** after the later of MOWZIE's cowed time and `min(192, 24 x 2^P_chief)` days,
  one warrior takes up the mask: the grove loses a member and the Umvuthi is at the fire pit at the
  next unwatched load. A grove with no warrior waits for one first.
- **Raptors:** raptor hunts go through the herds' same `predate` with the refuge (MOWZIE §4.10 item 4).
- **What the player sees:** a cleared grove takes about three months to refill (96 days in the model,
  42 days at no pressure); a farmed grove yields about 6 warriors a year, not 12; the dens as in §1.2.
- **Body:** vanilla at home, puppets in parties (MOWZIE §4.9). **Cost:** as conflict factions.

#### 2.4.8 Death Worms and oil nests (OG-9e)

- **Site:** the oil nest; K = 2 worms (TUNABLE; OG-9e's nest stage may raise it).
- **Renewal (Regrow):** each worm after `min(96, 12 x 2^P)` days, **only while the nest is awake** (a
  pumpjack drilling, as OG-9e has it); an asleep nest holds what it has. A worm a player kills adds
  `1/K`. Placed underground in the nest at the next unwatched load.
- **Natural desert Death Worms** (Ice and Fire worldgen, chance 0.0333 per placement) stay outside the
  sim, as natural dragons do (R18a, OG-1).
- **What the player sees:** a cleared nest under a working pumpjack is back in two to three months
  (72 days in the model); stop drilling and it stays as it is.
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
   E[warriors lost] + revenge`, with `p = A^2 / (A^2 + D̂^2)` (Lanchester's square law, as `threat.rs`
   already uses), `warrior_value` 3 loot items, and **revenge bounded:** `10 x g / (g + 100)` for
   grievance g, so hate alone never pays for a suicide raid.
3. **Terms first.** If the best rung has `EV > 0` and grievance has reached the envoy rung, the faction
   sends an envoy demanding `T = (EV + X) / 2`, the midpoint of the bargaining range between what the
   attack is worth to it (EV) and what it would cost the player (X, the expected loot lost): any `T`
   in that range leaves both better off than a war (the Nash bargaining solution with equal weights).
   If the player pays, today's tribute relief and respite apply. If he refuses, the attack follows after
   3 days.
4. **Otherwise hold, and probe.** With no rung worth sending, the faction holds. Every 48 days at the
   scouts rung it sends scouts, which refresh its belief.
5. **Faction against faction:** the same EV rule, with the target camp's pool as the defence.

The gate is a best response to a belief, so its outcomes are stationary for each kind of player (§3.5):
a fortress gets scouts and no raids, a weak base pays tribute and is not raided, a player who adapts
settles at a defence where the terms are affordable, and a weak player who refuses everything is
raided at a steady rate (the faction's best response to him).

Pirates use the same rule for route choice (§2.4.6). Dragons do not bargain: tribute stays as SINK.md
§4.6 defines it.

### 2.6 What MOWZIE.md must match

`plan/mowzie` is not edited here. At `be38b77`, MOWZIE.md must change these to match this design:

1. **§4.2 and §4.10, the Frostmaw body:** `recruits_per_day` 1/6 (a new Frostmaw 6 days after a death)
   becomes the shared rule: `min(192, 24 x 2^P)` days, P per den with H 96, `P += 1` for each Frostmaw
   a player kills, at home or away. Not age (MOWZIE's `chief_lifespan_days` 1,000,000 stays). Base 24
   or 6 is EQ-4.
2. **§4.5 and §4.10, grove warriors:** the refill at `recruits_per_day` 2 (a grove of 7 whole in 3.5
   days) becomes one warrior at a time, each after `min(96, 6 x 2^P)` days, `P += killed/K`. War-party
   losses refill by the same rule (it is one pool).
3. **§4.5 and §4.10, the Umvuthi:** "6 days, scaled by boldness, after the cowed time" becomes an Heir:
   after the later of the cowed time and `min(192, 24 x 2^P_chief)` days, a warrior of the grove takes
   up the mask (the grove's count falls by one); none while the grove has no warrior.
4. **§4.9, a den found empty:** "starts with an empty pool" gains "at the cap wait (`Pressure::at_cap`)
   when the site ledger has no row for it".
5. **§4.9, resets:** FACTION_FOUND carries the site ledger row (pressure, due day, members left
   behind), so a reset neither shortens a wait nor brings back a member that died away (§2.3).
6. **§4.9 and §4.10, arrivals:** HOME_SPAWN never places into a camp with a player within 128 blocks;
   a den's Frostmaw is OnLoad only (asleep), a grove's warriors may WalkIn from the far edge.
7. **Events:** every death report carries `LossCause` and the actor; HOME_CENSUS is the SITE_CENSUS;
   HOME_SPAWN carries `Source` and `Arrival` (§2.2).
8. **§4.10 item 4:** raptor hunts take herd animals through `predate` with the refuge.
9. **§4.10 item 5 and §7, tests:** `sim mowzie` reports in the `sim census` format (§4, slice S1), and
   the plan adds `kill_all_dens`, `kill_all_groves`, `farmed_den_yields_closed_form` (60 years, at most
   1.1 a year), and the GameTest `resetDoesNotMintAFrostmaw`.
10. **§4.10's "what this plan needs" list** is answered: caps (1 per den, the census per grove, no
    world-level ceiling beyond the structures), rates against pressure (above), the empty camp rule
    (§2.2, shared with lairs), predator and prey (item 8), the census (item 9).

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
| 5 | Site ledger SavedData and FOUND events carrying it | core Java, each plugin | M |
| 6 | Dragon home bodies (if EQ-2 a): vanilla roosting body, puppet away, lair census | `neoforge/iceandfire` | L |
| 7 | Village census both ways, settlers, unloaded growth (if EQ-1 a or b) | `rtsim-world`, `rtsim-jni` phase 1, core `village/` | M |
| 8 | Ribbit refill before build, pop on spawn, settlers | `rtsim-world` ribbits | S |
| 9 | Conflict pressure, break and refound, the gate | `rtsim-conflict` | M |
| 10 | Haven hires and cap, refit pressure, EV lanes | `rtsim-ships` | S |
| 11 | Long-horizon, kill-all and soak tests | each crate, `rtsim-host` | M |

### 2.8 Cost (D1)

| Kind | State per site | Work per renewal | Scans |
|---|---|---|---|
| Every site | Pressure 8 B, a due tick 8 B, counts | one wheel event, one closed form | none |
| Dragons | + fed share 4 B | the fed share updates at hunts, which exist | none |
| Herds | + nothing (M per species is a constant) | yearly decision (exists) reads 4 neighbours | none; frontier is one wheel entry per empty range |
| Villages | + logistic read at load | one census (exists) | none |
| Conflict | + pressure | pool closed form with P at the last event | none |
| Site ledger | about 32 B per site, Java | a write on SITE_STATE | none |

Nothing grows with time: site rows are bounded by the explored world, an extinct herd's row is reused,
notables use a generational arena, and the ledger has one row per site. The model's renewal events per
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
writes `results/summary.md`.

### 3.2 Herd births: Beverton-Holt

Yearly map for adults A (last year's young join at the birth day): `A' = S_a A + S_y f A / (1 + A/M)`,
with `S_y = e^(-m_young)`, `S_a = e^(-m_adult_and_old)`. The fixed point is
`A* = M (S_y f / (1 - S_a) - 1)`, and with the young of the year the post-calving total is
`K = A* (1 + (1 - S_a)/S_y)`, so M is set per species as
`M = K / ((S_y f/(1 - S_a) - 1)(1 + (1 - S_a)/S_y))`. The map's slope at A* is
`S_a + (1 - S_a) / (1 + A*/M)`, which lies in (0, 1): the map is monotone and A* is globally stable,
with no overshoot and no cycle. Growth from low density is `λ = S_a + S_y f` a year: sheep 1.33, cow
1.26, pig 1.72, horse 1.2, goat 1.41. From 12 founders to half of a range of 2,000 takes 10 to 28 years:
**herd recovery is set by biology** (one calving a year), not by any rate in this design.

Without the density term (today), births grow without bound until forage starvation stops them, which
is a delayed, overshooting feedback: the engine's 41-year run swings between 40,000 and 117,000 animals.

### 3.3 Pressure under farming

A site of K members, each arrival killed within c days: each kill adds `1/K`, so at the steady state
`P* = (1/K) / (1 - 2^(-gap/H))` with `gap = min(cap, base x 2^P*) + c`, one kill per gap. The right
side falls as P rises, so there is one root (`run.py: closed_form`, bisection). With c = 2 days and H 96:

PLACEHOLDER_CLOSED_FORM

The base sets the first wait; under steady farming the yield is set mostly by H: a den with base 6 and
pressure yields 1.8 a year, with base 24 one a year, against 12 a year for 6 days flat.

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

PLACEHOLDER_GATE

### 3.6 Results

PLACEHOLDER_RESULTS

### 3.7 Knife edges

PLACEHOLDER_KNIFE

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
- **Raptors** (MOWZIE's predators) are not modelled separately; they share the refuge rule.
- **Herd dispersal at predation-depressed density** (§3.7).

---

## 4. Build plan

Ordered slices. Each lands with its tests; later slices measure themselves with S1's census. Every
long-horizon test runs a release build (`cargo test --release`), under `nice -n 10`, never under the
GameTest lock.

| Slice | What | Tests it adds (names are the build's) | Waits for |
|---|---|---|---|
| S0 | `rtsim-kernel::renewal` (§2.2) | `pressure_halves_each_half_life`, `delay_is_capped`, `farmed_steady_state_matches_closed_form` (simulate kill-on-arrival for 60 years against `RenewalRule::farmed`, within 5%) | nothing |
| S1 | `rtsim-cli sim census`: one line per kind every N days (sites, members, capacity, short, broken, mean P, renewals) for every module the scenario runs; the same hook in the host soak | `census_counts_every_kind`, `census_is_flat_with_no_player` | S0 |
| S2 | Herds: density term, refuge in `predate`, spring dispersal, frontier founding, tombstone removal | `herds_settle_without_forage_tuning` (forage headroom 1 to 4 gives one band), `predation_never_takes_a_herd_below_refuge`, `kill_all_herds` (world wipe: half back within 25 years, all ranges alive by 40), `local_wipe_refills_from_neighbours`, the property tests extended (founding is a booked source) | S0, S1 |
| S3 | Dragons: lair pressure, fed share, starve-leave, arrival gate, newcomer direction, taming bump | `kill_all_dragons` (90% back within 72 days), `farmed_lair_yields_closed_form` (60 years, at most 1.2 a year), `starving_dragon_leaves_and_returns_when_fed`, `long_horizon_dragons_and_herds_with_kills` (60 years, roamer script, the §3.6 bands) | S2 |
| S4 | Site ledger and SITE_FOUND carrying it (Java) | GameTests `resetDoesNotMintADragon`, `resetKeepsAFarmedSiteFarmed`, `siteFoundEmptyWithoutLedgerWaitsTheCap`, `uninstallLeavesHomeBodies` | S3 |
| S5 | Dragon home bodies (EQ-2 a): vanilla while roosting, puppet away, lair census | GameTests `roostingDragonIsSavedByVanilla`, `awayDragonIsAPuppetAndTheCensusReconciles`, `uninstallKeepsRoostingDragons` | S4, EQ-2 |
| S6 | Villages and Ribbits (EQ-1): census both ways, settlers, unloaded growth, Ribbit refill before build, pop on spawn | `kill_all_villagers` (settlers in 32 days, 90% within 600), `village_pop_follows_census_down`, `kill_all_ribbits` (90% within 300), `sim_quiet_stays_quiet_with_settlers`; GameTests `settlersWalkInFromTheDonor`, `noArrivalIntoAWatchedVillage` | S1, EQ-1 |
| S7 | Conflict: pressure on recruits, break and refound, the gate (EQ-5) | `kill_all_camps` (90% within 96 days), `farmed_camp_breaks_and_refounds`, `fortress_gets_probes_not_raids`, `weak_base_is_offered_terms_first`, `no_ratchet_over_50_years` (the adaptive base's defence and wars flat across decades) | S0, EQ-5 |
| S8 | Ships: hires and cap, refit pressure, EV lanes | `kill_all_pirates` (90% within 150 days), `crew_pool_is_bounded`, `pirates_survive_unattended_1000_years` | S0 |
| S9 | MOWZIE on the shared rule (in its plan, §2.6) | `kill_all_dens`, `kill_all_groves`, `farmed_den_yields_closed_form`, GameTest `resetDoesNotMintAFrostmaw` | S0, S4, MOWZIE |
| S10 | Oil nests on the shared rule (with OG-9e's build) | `kill_all_nests` (regrow only while drilled), `asleep_nest_holds` | S0, OG-9e |
| S11 | The long-horizon suite | `long_horizon_stability_with_kills` (every module, 60 game years, the roamer script, every kind inside its §3.6 band), `kill_all_<kind>` for every kind above in one place, `who_is_alive_soak` (262,980 game days in the host at max: every kind's census flat by century, state bytes flat, renewal events per day flat; nightly, about 45 minutes at 100 game days a second) | all |

The pass bands in the test names are the model's 95th percentiles with margin; the build replaces them
with the engine's own bands from S1's census once S2 to S8 land.

---

## 5. Decisions

### 5.1 Questions for Ben

PLACEHOLDER_QUESTIONS

### 5.2 Decided here (each can be undone)

PLACEHOLDER_CHOICES
