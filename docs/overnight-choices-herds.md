# Overnight choices: migrating herds (2026-10-20)

Reversible choices made while Ben was asleep, each with its reason, numbered H2-# so they never
clash with the H choices that already exist. None is final until Ben reviews it. The work is
`crates/rtsim-ecology/src/herds` (a new module next to the dragons), the herd prey interface in
the dragon engine, `rtsim-cli sim herds`, the herd and members sweeps of `bench scale`, and
`docs/boundary.md` §8. DECISIONS.md, PLAN.md and OPEN-QUESTIONS.md are unchanged.

## Vanilla facts checked in the 1.21.1 sources (2026-10-20)

Checked in NeoForm's decompiled 1.21.1 sources (`sourcesAndCompiledWithNeoForge` in the Gradle
cache): `EntityType`, `Sheep`, `Cow`, `Pig`, `Horse`, `AbstractHorse`, `Variant`, `Markings`, `Goat`,
`AgeableMob`, `EatBlockGoal`, `DyeColor`. Everything else in the herds is TUNABLE or a scenario
choice.

- Entity types `minecraft:sheep` (box 0.9 × 1.3), `minecraft:cow` (0.9 × 1.4), `minecraft:pig`
  (0.9 × 0.9), `minecraft:horse` (1.3964844 × 1.6), `minecraft:goat` (0.9 × 1.3); all
  `MobCategory.CREATURE` with `clientTrackingRange(10)`.
- With Ice and Fire's food-point rule (`round(width × height × 10)`, verified with the dragons),
  one animal is worth 12 fullness (sheep, goat), 13 (cow), 8 (pig), 22 (horse).
- A natural sheep's colour, `Sheep.getRandomSheepColor`: `nextInt(100)` below 5 black, below 10 gray,
  below 15 light gray, below 18 brown, else white, but one in 500 of those pink. NBT `Color` (byte,
  the `DyeColor` id: white 0, pink 6, gray 7, light gray 8, brown 12, black 15) and `Sheared`.
- Wool comes back when a sheep eats grass (`Sheep.ate`); an adult's `EatBlockGoal` fires one tick
  in 1,000, a baby's one in 50.
- A horse's `Variant` int is `variant | markings << 8`; a spawned horse draws both uniformly over 7
  colours and 5 markings (`Horse.finalizeSpawn`, `Util.getRandom`) and its health, speed and jump
  from its own random.
- A goat screams with chance 0.02 at spawn (`IsScreamingGoat`); `HasLeftHorn`, `HasRightHorn`.
- Cows and pigs have no variants in 1.21.1.
- `AgeableMob`: `Age` (int) is negative for a baby and starts at −24,000; `ForcedAge`.
- Movement speed attributes (not used by the sim's migration speeds, which are its own): sheep
  0.23, cow 0.2, pig 0.25, goat 0.2, horse random.

## Choices

| # | Choice | Why |
|---|---|---|
| H2-1 | Herds are their own module, `crates/rtsim-ecology/src/herds`, with their own engine (`Herds`), timing wheel, boundary codec and save. The dragon engine gains only additive pieces: `Target::Herd`, `PreyKind::Herd` (season weights herd 110, 130, 110, 70 for spring to winter), `DragonTunables::herd_risk`, `DragonStats::herd_hunts`, and `Dragons::run_until_with_herds`. Nothing of the dragons' interest code is touched. | The integration agent edits rtsim-jni, rtsim-world's interest code and the ecology's interest legs tonight; the task asks for herd code in its own module, coordinating through additive APIs. |
| H2-2 | **O5's recommendation, taken:** deaths are the kernel's dyadic counts (`Split::Hazard`), one tree per (year-class, anchor generation, cohort). What depends on state (births from the adults alive, starvation from the shortfall, kills, takings, predation) happens only at events and re-anchors the classes it touches. | Deaths at a fixed hazard are exactly the randomness dyadic counts carry; a read after 10,000 days walks at most three trees per class (bench: catch-up 0.02 to 0.04 µs per herd read whatever the gap). O5 stays Ben's open question; this is its recommended answer, logged. |
| H2-3 | **Year-classes** instead of three cohort rows: every birth event makes one class (`[first, first + born)` of member indices, its birth day, its count at its anchor). Young, adult and old are sums by age (maturity, old age, lifespan per species). | A member's age is its class's, so its cohort is a function of time: the same ewe is a lamb, a ewe, then old, never young again, which three cohort rows with binomial ageing cannot promise. At most lifespan-in-years classes per herd. |
| H2-4 | Deaths count in day slots (deaths of day d show from day d + 1), except death of age, which takes every survivor on the day the lifespan is reached. A re-anchor at any tick of day D commits the days before D. | A herd's count changing at dawn is invisible at this scale; day slots keep trees short (L = log2 of a cohort's days). |
| H2-5 | Births once a year at the species' first breeding sub-season: Binomial(adults × ceil(fertility), fertility × adult condition / ceil(fertility)). Both sexes count; members have no sex. | Vanilla animals have no sex; one draw a year at an event is O(1) and composition-safe. Fertility above 1 (pigs, 1.2) needs more trials than adults. |
| H2-6 | Species data (diet, comfort band, herd size range, speed, breeding sub-seasons, maturity, old age, lifespan, annual mortality per cohort, fertility, ground per animal, starvation hazard, predation weights) is `HerdSpeciesData`, every number a labelled PLACEHOLDER in sim years of Serene Seasons' 96 days. Bodies are the five vanilla animals (D4). | The task; Naturalist bodies are a later plugin (P8). |
| H2-7 | The forage field: 128-block cells; logistic growth with decay, F' = gF − hF², which is linear in u = 1/F, so a sub-season is an affine map, a year one composition, n years its closed-form power. Only grazed cells have rows; a cell without one sits on the climate's periodic steady state; a yearly event folds rows back within 1% of it; grazing never goes below 3% of a cell's best capacity. | Catch-up of any cell is O(sub-seasons a year) whatever the elapsed time (tested against an ODE stepper and for composition); memory follows recent grazing, not the map. |
| H2-8 | Climate through a provider trait (`herds::climate::Climate`: forage rates, temperature and terrain per cell and sub-season). It is assumed **periodic in the year**. The synthetic `Valley` serves the tests, the CLI and the bench. | Serene Seasons and Deep Time come later as plugins filling the same trait. A provider whose years differ would re-anchor rows at each year. |
| H2-9 | Sub-seasons are Serene Seasons' twelve, over the ecology's `Calendar` (P2 undecided). | Same shape as the dragons' calendar (R8). |
| H2-10 | A decision per herd per sub-season, staggered by a hash of the herd over a quarter day. At it the herd is charged for its grazing since the last one at the place it stayed (travel grazing ignored), up to 80% of the forage above the floor in its footprint; condition moves half way toward what was met; below half of demand met, it starves at the species' hazard scaled by the shortfall (one binomial per class). | Events, not elapsed time; staggering keeps decisions from spiking (PLAN.md §4.3). |
| H2-11 | **The flow field** is a Dijkstra over a dense local grid within a reach cap (40 cells) and a horizon of 2.5 sub-seasons of walking; cells are scored by forage on arrival (what meets demand, capped at 1, plus richness relative to the best in reach), °C outside the comfort band (0.15 a degree), route cost (0.08 per 1,000 block-equivalents) and other herds' claims; the herd walks one sub-season's share (60% of it) of the path to the best cell and decides again next time. Cells beyond this sub-season's walk are scored on a stride of two. No corner cutting past a closed or obstructed cell. | Looking only one sub-season ahead left herds stuck behind a ridge they could not cross in one go; the horizon lets them plan the long way round. The dense grid halved the decision's cost. |
| H2-12 | The comfort band is a soft penalty, not a wall. | A herd caught outside its band moves toward it instead of finding no legal cell. |
| H2-13 | A herd avoids cells within two of another herd's target this sub-season (−0.4). | Herds spread instead of piling on the same pasture. |
| H2-14 | **Obstacles raise the cost of entering a cell**, not of one edge: the cell a cell ahead of where the group is stuck, toward its steer target; +100 per report at full failure (the share of the group that failed, at least a quarter), capped at 400, halving every two sim years; the herd replans from where it is stuck at once. | A fence across a pass closes every edge into the cell it blocks; 100 makes a fenced pass a cell wide dearer than a detour of several kilometres. |
| H2-15 | Herds index their routes by region themselves (derived, rebuilt on load from each route and its saved footprint radius) and book legs into the **shared leg index only for players near** (A15, lazily): one leg per (herd, player), the route's chord over the player's interest window (about two hours), booked with the footprint radius less a depth of 24 blocks, so the one meeting fires when the footprint reaches into the area. | The task's "booked lazily, only where a player could see them"; a herd leg over a sub-season would book hundreds of cells nobody reads. |
| H2-16 | Herds book their own player interest areas (owner bit 60 with bit 59 and a player slot), as the dragons do theirs (R31); herd legs are bit 60 with the herd's handle (players 63, dragons 62, ships 61). | One interest area per player across modules is the integration agent's work tonight; this keeps the herds out of its way and ignored by every other module's owner decoding. |
| H2-17 | A group's size: alive × the share of the footprint inside the player's area at closest approach over the next two hours (a lens area), at least 1, at most 32 a group and 48 a player (PLACEHOLDERS); one group per herd at a time, for the first player near it. | The task's "count × the loaded fraction of its footprint, capped per player". The loaded area is taken as the interest area until the bridge reports loaded chunks per herd. |
| H2-18 | Members are placed deterministically inside the footprint and inside the player's area (its inner edge), from the member's seed and the epoch; a lens midpoint when no sample lands. | The bridge rejects a spawn outside the entity-ticking area (§2.4); members appear "at the far edge, all moving the same way". |
| H2-19 | **Which members are alive is a convention over counts:** in a class, the seen ones are alive, and of the unseen ones not dead, the lowest indices are. Anonymous deaths fall on the highest unseen indices; seen members die only when a class's count falls below them (old age), highest index first, unembodied first, into the dead set. A herd remembers 1,024 seen members at most, forgetting the least recently seen. | PLAN.md §4.4's dead-index set and "cohort deaths from unseen indices", with no row per member. |
| H2-20 | Sampling: seen members first, most recently seen first; then fresh members by largest remainder over the classes' unseen counts, lowest unseen index first. | The group you met comes back as it was, topped up in cohort proportion. |
| H2-21 | A10: a killed or taken member leaves its class (one re-anchor, minus one) and joins the dead set; a player's kill is booked as killed, other deaths as lost, a taking as taken. A shearing marks the member sheared until 2,400 ticks pass (PLACEHOLDER from vanilla's regrowth), folded back at the next capture. Op ids apply once. | The task; the dead set keeps a killed animal from ever reappearing. |
| H2-22 | An embodied group is looked at every 600 ticks: steered when its heading changes, despawned once the footprint is 64 blocks beyond the area or the player is gone. | An observed instance is allowed cost; hysteresis so edge-walking does not thrash (PLAN.md §4.2). One materialization per approach is tested. |
| H2-23 | **Dragons prey on herds** through `Herds::prey_near` (the region index, no scan) and `Herds::predate`: the dragon weighs where the herd will be when it arrives; a hunt takes latent animals only, at most 1% of the herd, weighted young 3, adult 1, old 2, and the herd books them as eaten. The dragons' own ledger is unchanged: animals are not goods. Village livestock stays a prey source (extended, not replaced). | P12 said a minimal wild-herd model could come in phase 2; an embodied group's animals are the plugin's, lost only in a real fight. |
| H2-24 | **Dragons follow herds:** a herd remembers up to 4 predators that ate from it, for 24 days each; such a predator sees it within three times its usual range. The memory lives in the herds. | The scenario's "dragons that follow a herd", with no change to the dragons' saved rows beyond the new target. |
| H2-25 | `Dragons::run_until_with_herds` lends the herds to the dragon engine for the call (swapped with an empty engine) and interleaves the two wheels: a herd event at the same tick as a dragon's runs first. `run_until` is unchanged. | Dragons read herd positions and take animals mid-run; the herds must be caught up to each dragon event. |
| H2-26 | Conservation per herd, in integers: the classes' counts at their anchors equal founded + born − died − starved − killed − eaten − taken − lost, and the next index equals founded + born. Checked after every step in the CLI and every test. | SCALE.md §9; the task's "animal counts are integers in the ledger". |
| H2-27 | Save: one page, magic `RTHD` version 1, everything but the tunables, the climate and the derived indexes; the state hash is FNV of the page. | The dragons' persist pattern (S8); restore gives an identical hash (property-tested with a random script, the leg index saved beside). |
| H2-28 | Vanilla's own passive animals are left alone: the sim's herds are extra bodies. | Suppressing or adopting them (as the dragons adopt lair dragons) is the vanilla adapter's question, open. |
| H2-29 | Kind ranges 0x0080 to 0x008F, 0x0700 to 0x07FF, 0x8700 to 0x87FF; agent namespace byte 0x48. | Next to the ships' (0x0070, 0x0600, 0x8600), clear of every range reserved in docs/boundary.md and docs/industry-boundary.md, and of the industry plugins' branch. |
| H2-30 | The bench times the herds engine alone (no world: herds need only the leg index): CPU per herd-day flat in herd count, CPU per sim day and catch-up flat in members per herd, heap linear in herds; the members sweep's heap contract is 0.3, not flat, because grazed forage rows grow with a footprint up to its cap. | Members themselves cost zero bytes; the grazed area is the honest residue. |
| H2-31 | `sim herds` timeline (seed 7, 200 days from early spring of year 0): the player meets the Northdown flock on day 26 (kills six, shears three, leads one away), walks 2 km away for two hours and comes back; fences the pass at the start of autumn and waits there until early winter; a restart of world, dragons and herds at the midpoint; two lairs (Highmoor Roost in the northern summer pastures, Ashpeak south of the ridge). | A reproducible script tuned to this seed; the checks verify the beats, not exact days. |
| H2-32 | The synthetic valley: 0.5 °C a cell colder to the north, ±10 °C over the year; forage stands where it is wet (the north's rains peak in late spring, the south's in late autumn, two sub-seasons ahead of the warmth), capacity 4,000 sheep-days a cell times noise times 0.1 + 0.9 × wet²; an east-west ridge two cells deep and 61 long with one pass a cell wide. Six herds of about 33,000 animals. | A world where following forage means crossing the ridge twice a year, and fencing the pass forces a detour of several kilometres. |
| H2-33 | A footprint's radius is sqrt(count × ground per animal / π), clamped to 16 to 1,536 blocks. | The cap bounds a decision's grazing work and the forage rows a herd writes. |
| H2-34 | Founders come with a stable age pyramid at the species' mortalities, ages whole years plus the time since its last birth season. | A herd found in a world should look like one that has lived there. |
| H2-35 | A member embodied right now never dies off-screen: anonymous deaths and old age skip it, and a year-class holding one stays until the group is captured; it dies of age while away, at the next materialization. A kill reported for a member whose class has already reached zero is booked as that natural death, not as a kill. | Found by the restore property test: a founders' class reached its lifespan while a cow of it stood in front of the player. |

## Results

- **Tests:** `crates/rtsim-ecology/tests/herds.rs` (12, three of them property tests): founding,
  seasonal migration, identity after kills and a season of deaths, flat catch-up, one
  materialization per approach, composition (one run equals many, reads in between change
  nothing), conservation under any script, restore under any script, a truncated page, an
  obstacle and its replan, dragons eating from and following a herd, a new session. Unit tests in the module
  (18): vanilla food points and colour odds, cohorts, the forage closed form against an ODE
  stepper and its composition, grazing's floor and fold, year-class reads and re-anchors, catch-up
  work, the route through a gap and the fenced gap, route closed forms, the codec. Workspace: 389
  passed, 0 failed; clippy `-D warnings` and fmt clean.
- **`sim herds`** and **the sweeps**: see the reply that merged this branch and
  `bench/results/README.md` (2026-10-20, the herd sweeps).

## What is not done

- **The native engine does not route herd records** (no herds module in rtsim-jni), and **no vanilla
  adapter exists**: `herds::synth::FakeHerds` is the executable reference.
- **One interest area per player**: herds book their own (H2-16) until the integration work lands.
- **Climate providers**: only the synthetic valley; Serene Seasons and Deep Time plugins later.
- **Other predators**: wolves and players' own hunting outside a materialized group are not
  modeled; a real dragon eating embodied herd animals comes back as HERD_KILLED (lost), not through
  DRAGON_ATE.
- **Grazing while walking** is ignored; a herd eats only where it stays.
- **Decision cost** is about 2 ms (0.2 ms a herd-day), mostly forage reads of the flow field; a
  per-cell cache of the steady state would cut it.
