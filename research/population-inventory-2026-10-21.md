# Population inventory: how every population is born, grows, shrinks, dies out and comes back

Date: 2026-10-21. Repo: `mic-rtsim`, main tip `aca3c3a`. Facts only; nothing here is a design or a recommendation.
Purpose: input for a design on long-run stable worlds and for repopulation (Ben: "a nash equilibrium between
tribes / animals", "if the player kills off all of the Yeti's ... a way for new ones to spawn. Same with dragons,
villages, etc.").

How this was made: four read-only code surveys (dragons; herds and threats; villages, caravans, notables;
ships, conflict, industry), a spot check of their claims against the source, and long-horizon runs of the cheap
`rtsim-cli sim` scenarios on a release build of main (section 16). Citations are `file:line`, relative to the repo
root. Java paths are abbreviated: `core/` is `neoforge/core/src/main/java/com/minecraftindustrialcomplex/mic_rtsim/`.
"PLACEHOLDER" means the source marks the number as a placeholder. Things I did not verify by running are marked
"(reading)".

## 0. The picture in one page

| Kind | Real thing in the world | Sim state | Comes back from zero? |
|---|---|---|---|
| Dragons (Ice and Fire) | Entities, puppeted | A knowledge notable per dragon, plus a permanent record per lair | Yes, from any known lair, 20 days after it stands empty, with no cap |
| Herds (sheep, cow, pig, horse, goat) | Vanilla animals, puppeted near players | Year-class counts per herd | No. Extinct is permanent per 512-block region |
| Human villages and villagers | Vanilla villages and villagers | One integer `pop` per village | No source of villagers. The integer never falls |
| Ribbit villages | Vanilla-saved ribbit entities and placed houses | `pop` plus `houses` (max 8) | No. The 8-house ceiling is permanent |
| Player bases | Player-built | A pop-0 node | n/a (not a population) |
| Havens | Not in the real game | Fixed-pop node | n/a |
| Caravans and messengers | Puppets for what a player can see | Derived from edge schedules | Cannot go extinct while a road exists |
| Notables | None | Records with heirs | Chiefs and heirs always refill; traders and captains refill lazily |
| Pirates | Not in the real game (no Java) | Rust only | Only while `crew_pool` lasts. Then extinct |
| Merchant and navy ships | Not in the real game | Rust only | Always, 2 days after loss |
| Conflict factions (goblin, bandit, raider) | Not in the real game (no Java) | A float pool that refills linearly to a cap | Yes (in Rust); factions themselves are immortal |
| Underground threats | Natural mobs only | A cumulative spawn ratio per district | Nothing to restore. Death Worm and oil nest have no code |
| Yeti (Frostmaw) and the rest of Mowzie | Not in the pack's rtsim | A plan on an unmerged branch | Plan: an heir arrives 6 days after a kill |
| Industry districts | Player-built | Mirror of player blocks | No. Only players make them |

Three structural facts run through every row:

1. Only three kinds have real bodies and a working Java plugin on main: dragons, herds, and the village side
   (villagers, ribbit houses and residents, caravans). Ships and conflict have no Java at all
   (`core/engine/NativeCodec.java:116,119`; nothing sends FACTION_FOUND, HAVEN_FOUND or PORT_FOUND), so they run only
   in Rust, in tests and in `rtsim-cli sim`.
2. Bodies are puppets. Materialised sim bodies are never saved by vanilla, so the sim's state is the truth and an
   uninstall leaves none behind (section 15).
3. Where something comes back, it is almost always because a one-way record (a lair, a region mark, a node) stays
   forever and a timer or pool refills against it. Where a record is spent (a region's herd mark, a ribbit village's
   house count, a haven's crew pool), the kind does not come back.

---

## 1. Dragons (Ice and Fire)

Engine: `Dragons` in `crates/rtsim-ecology/src/engine.rs`. A dragon is a knowledge notable (so it has an heir and
grudges). Java: `neoforge/iceandfire` (`DragonBridge` puppets, `Lairs` lair finder).

| Question | Finding | Citations |
|---|---|---|
| 1. Lives where, which plugin | Rust rows in `dragons` and `lairs`; state machine Roosting, Flying, Hunting, Embodied, Leaving (`dragon.rs`). Hunger is closed form, floored at 0. The JNI host builds it with `DragonTunables::default()` and runs it while any dragon OR any lair exists. Java plugin `neoforge/iceandfire` (`DragonBridge`, `Lairs`, `MicRtsimIceAndFire`). | `crates/rtsim-ecology/src/engine.rs:648-695`; `dragon.rs:297-300`; `crates/rtsim-jni/src/engine/dragons.rs:66,103-106,207-218`; `neoforge/iceandfire/.../MicRtsimIceAndFire.java:77-100` |
| 2. How new members appear | (a) Adoption of a lair's own worldgen dragon (spawn kind STRUCTURE inside a lair box, lair empty). (b) Respawn: `respawn_delay_days` (20) after a lair becomes vacant, or after a lair is discovered with no dragon, a 30-day-old dragon of the lair's species appears 3,000 blocks away in a random direction and flies in to claim it. (c) Breeding: a mated female in a lair hatches a 0-day-old child every 60 days (practically unreachable, see below). (d) Spawn-egg and `/summon` dragons are adopted as lair-less wanderers. Natural and spawner dragons (In Control) are NOT adopted by default. Hatched and tamed dragons are never adopted. | `engine.rs:597-640` (schedule at 637-638); `engine.rs:3571-3632` (respawn); `engine.rs:3640-3726` (untagged adoption); `engine.rs:3471-3537` (breed); `tunables.rs:117-123,182-184` |
| 3. How members leave | Age: each dragon books a death at 400 days (lifespan, a sim invention; IaF has none). Player kill of a materialised dragon: Java journals DRAGON_DIED with the killer, the engine records a Killing report, lair and kin inherit a grudge, memories pass to the heir. Taming: journaled as DRAGON_DIED with no killer. NOT a way out: starvation (no code path), abstract raids (wound floors at 10 percent health), dragon vs herd. | `engine.rs:652,689,1100-1122,2759-2769,3203-3289,3315-3368`; `neoforge/iceandfire/.../DragonBridge.java:811-858,1008-1024`; `species.rs:84-87` |
| 4. What happens at zero | Dragons come back from the lairs. A killed or tamed dragon vacates its lair, which books a Respawn at +20 days. Lairs are never removed (no removal code found), so every lair ever reported replaces its dragon each time it stands empty, with no cap, no cooldown growth, no check on how many dragons exist. A lair discovered with no dragon gets one anyway after 20 days. A respawned newcomer that dies on the way draws another after another full delay (regression test). Where there is no lair, nothing returns. | `engine.rs:3370-3393,3571-3632`; `crates/rtsim-ecology/tests/review3.rs:88-130`; `tests/dragons.rs:404-423`; `docs/overnight-choices-dragons.md:61` |
| 5. Interactions, feedback | Prey by utility: roads (caravans), villages, grudge targets (a player's home or farm), wild herds. Village raids take livestock out of the books. Herd hunts call `herds::predate` (1 percent of the herd per hunt, then the dragon follows the herd 24 days). Season weights: winter favours villages. Tribute (SINK.md 4.6): a village paying a quarter lowers its pull; a vacant lair loses its tribute. Grudges pass kin to heir to lair, and the lair keeps feelings for its next occupant. Loops: kill gives a grudge held by the lair, so the respawned occupant inherits it (positive); prey exhausted gives `fruitless` count, then migration to a vacant lair (negative on local pressure). | `engine.rs:1914-1961,3397-3421,1820-1842`; `herds/engine.rs:1957-1985`; `tunables.rs:149-154,173`; `tests/tribute.rs:196,256` |
| 6. Parameters | `respawn_delay_days` 20, `respawn_age_days` 30, `respawn_from_blocks` 3,000 (PLACEHOLDER); `hunt_below` 60; `rest_ticks` 12,000; `fruitless_before_move` 3; `view_radius` 160; `lair_feelings` 4; `tribute_share_milli` 250; `grudge_below` -40; `raid_wound_milli` 400; `adopt_natural_dragons` false (TUNABLE). Species: `lifespan_days` 400, `stage_days` [25,50,75,100], `breed_days` 60, `disperse_days` 50, `max_health` 500. **Caps: none** on dragon count, lairs, respawns or births per lair. The only bounds are one occupant per lair (vacancy test) and the discard of a second structure dragon. | `tunables.rs:55-144,163-191`; `species.rs:136-159`; `engine.rs:1847-1880,3660-3668` |
| 7. Tests and tools | `tests/dragons.rs`, `review3.rs`, `persist.rs`, `tribute.rs`, `dragons_paged.rs` (48 lairs, 8 days); `rtsim-cli sim dragons` (48 days default, scripted player); `bench dragons` (cost, not population). Java: 9 GameTests in `DragonGameTests.java`. No test or tool before this report ran dragons longer than about 48 days or counted them over years. | `crates/rtsim-cli/src/sim/dragons.rs:51-71,454-512`; `crates/rtsim-cli/src/bench/dragons.rs:1-10` |

Dragon specifics worth stating plainly:

* Breeding is real code that real worldgen almost never reaches. A lair only receives a second dragon when vacant;
  a worldgen second structure dragon is discarded (`engine.rs:3662-3665`); a respawn newcomer is stage 2 and cannot
  pair on arrival (pairing needs stage 3, `engine.rs:3448-3456`). Births are 0 in every run (section 16).
* Taming vacates the lair, so a tamed dragon is replaced by a wild one after 20 days (reading, untested;
  `DragonBridge.java:1008-1024`, `engine.rs:3370-3393`).
* Natural dragons (In Control's desert fire dragons): not adopted by default (R18a, OG-1, P13). The JNI host cannot
  flip the setting (`crates/rtsim-jni/src/engine/dragons.rs:103-106`). Reading gap: the spawn-type attachment is
  never saved (`MicRtsimIceAndFire.java:47-52`), so a natural dragon left alone may be reported untagged with kind
  OTHER or STRUCTURE after a restart and then take the adopt path (`DragonBridge.java:936-948,964-965`;
  `engine.rs:3654-3668,3678-3695`). Untested.
* "Dragon season" in the CLI help is a scenario span (autumn into winter) and the seasonal prey weights, not a
  breeding or arrival season.

### 1b. Lairs as a population of sites

| Question | Finding | Citations |
|---|---|---|
| Where | Rust `Lair` rows (key, species, Roost or Cave, pos, actor, occupants, hoard, `vacant_since`). Java `Lairs` finder. | `engine.rs:616-636`; `Lairs.java:59-70` |
| Appear | Only by discovery: LAIR_FOUND on a chunk load whose structure start is a lair, or an untagged dragon joining inside one. Ice and Fire generates roosts in new chunks; nothing in the sim creates lairs on main. | `Lairs.java:115-131,158-182`; `engine.rs:597-640` |
| Leave | Never. A lair whose structure a player destroys stays a record. | no removal in `crates/` or `neoforge/` |
| Zero | n/a. Lairs drive the respawn. | `engine.rs:3370-3393` |

### 1c. Unmerged: `feat/oilgate-dragons` (worktree `../mic-rtsim-oilgate`)

Not on main (`git branch --contains 8726ac0` lists only that branch). Included because it changes the answers.

* Field-flight visitors: dragons drawn from nothing near an oil field when a player at y >= 15 stands within 100
  blocks; caps 8 per field and 200 per frame (PLACEHOLDER). A visitor carrying valuables can claim a vacant lair or
  stay a wanderer, so it is a one-way source into the permanent population. Default OFF; the OG-10 A/B failed
  (B's first hit 40 to 50 percent later, a third to a half of A's distinct dragons) (`oil.rs:110-137,641-691,693-756,758-790`;
  `docs/choices-oilgate.md:92-95,298-306`).
* `PLACE_LAIR`: ON by default on that branch. Once per oil field with no fire lair within 1,500 blocks, Java
  `LairPlacer` generates a fire roost via vanilla's `/place structure` path. The placed roost's dragon is adopted and
  the lair becomes a permanent respawn source. Count is bounded only by the oil fields players visit
  (`oil.rs:592-636`; `neoforge/iceandfire/.../LairPlacer.java:1-60`).

---

## 2. Herds and vanilla wild animals

Engine: `Herds` in `crates/rtsim-ecology/src/herds`. Java: `core/herd/HerdBridge.java` (command range 0x8700),
`habitat/HabitatSampler.java`. Switch `Module.ECOLOGY`, default on (`core/config/RtsimConfig.java:51`).

| Question | Finding | Citations |
|---|---|---|
| 1. Lives where | Rust: one `Herd` row per herd (year-classes, route, `seen` and `dead` sets, followers, ledger of founded, born, died, starved, killed, eaten, taken, lost). Java `HerdBridge` materialises a group as vanilla animals near a player (`HerdBodies`, `HerdFollowGoal`). | `herds/engine.rs:223`; `herds/herd.rs:240-275`; `crates/rtsim-jni/src/engine/herds.rs:99-110`; `core/herd/HerdBridge.java:70` |
| 2a. Founding | One herd per 512-block region, judged once the region is at least 25 percent known. Species drawn by weight from the biomes' vanilla creature spawn shares plus chunk-generation animals. Size = `density[species] x habitat cells` (sheep 24, cow 10, pig 12, horse 6, goat 8), floored at 4 x the chunk-generation animals, clamped 8 to 4,000. Then the region is marked `mark::HERDS` and never judged again. Founding follows exploration, not world creation. | `herds/habitat.rs:254-265,291-379`; `crates/rtsim-jni/src/engine/worldmodel.rs:48,51,267-318`; `herds/engine.rs:611-713`; `docs/overnight-choices-worldmodel.md:74` |
| 2b. Births | Once a year per herd at the first of the species' breeding sub-seasons: `Binomial(adults x ceil(fertility), fertility x adult_condition / ceil(fertility))`. Only adults breed. **No density term.** Fertility per adult per year: sheep 0.55, cow 0.4, pig 1.2, horse 0.3, goat 0.7. A herd does not breed in its founding year. | `herds/engine.rs:758,1104-1150,677`; `herds/species.rs:172-230`; `docs/overnight-choices-herds.md:43` |
| 2c. Other sources | None. No splitting, merging, immigration or refounding (grep of `herds/*.rs`). Vanilla chunk-generation animals are only counted as founding weight; their bodies are left alone (Y20 "left alone"). | `neoforge/core/.../habitat/HabitatSampler.java:271-286`; `docs/overnight-choices-worldmodel.md:60,61`; `docs/overnight-choices-herdsmc.md:72` |
| 3. How members leave | Background mortality per cohort per year (sheep young 0.25, adult 0.06, old 0.25; cow 0.18/0.04/0.2; pig 0.35/0.08/0.3; horse 0.15/0.03/0.15; goat as sheep). Old age at `lifespan_days` (sheep 960). Starvation: below satisfaction 0.5 a binomial per class. Dragon predation (the only predator; wolves not modelled). Player kill: Java reports a killed puppet as HERD_KILLED (`HerdEvents` on `LivingDeathEvent`); taken (lead, tame, Carry On) is HERD_TAKEN. Killing a plain vanilla animal reports nothing. Shearing has no population effect. | `herds/species.rs:181-236,267-271`; `herds/engine.rs:1055-1099,1679-1730,1750-1810,1889-2110`; `HerdBridge.java:333-351,830-935`; `herds/tunables.rs:41,120-124` |
| 4. At zero | A herd goes extinct when a decision finds `alive == 0`: route becomes a stay, `next_decide = u64::MAX`, a `HERDS_EXTINCT` archive event. The row is never removed, the region mark stays, and a name guard blocks a second herd of that name. **Nothing re-founds it**: no recolonisation, no timer, no neighbour spill. Vanilla's own animals keep spawning by vanilla rules and are unrelated. A herd can also die out with no player (adults gone at breeding time gives no births). | `herds/engine.rs:784-802`; `herds/herd.rs:260`; `worldmodel.rs:292-296,316`; `tests/herds.rs:800-847` |
| 5. Interactions | Dragons eat herds and follow them. Players: kills, shears, leads, fences (obstacle reports raise a cell cost by 100, cap 400, half-life 192 days). Herds compete for forage and avoid another herd's claimed cells. No feedback from herd count to dragon count except through dragon fullness. | `herds/engine.rs:1812-1888`; `herds/tunables.rs:30,69-74`; `PLAN.md:1178-1186` |
| 6. Parameters | PLACEHOLDER throughout: `intake_per_day` 1.0 (cow 4, pig 1.5, horse 5, goat 1), `maturity_days` 96, `starve_hazard` 0.2, `graze_share` 0.8, `starve_below` 0.5, `predation_share_milli` 10, `follow_factor` 3, `followers_cap` 4, `group_cap` 32, `player_cap` 48. Founding: `min_count` 8, `max_count` 4,000, `min_known_share` 0.25, `FOUNDING_PER_STEP` 64. Forage: 128-block cells, capacity 2,000 per cell, growth 0.1/day. **No hard population cap and no carrying-capacity number**; capacity is emergent from forage. The only caps are on puppets. | `herds/species.rs:171-242`; `herds/tunables.rs:94-125`; `herds/habitat.rs:67-82,254-265`; `herds/forage.rs:30,33`; `herds/engine.rs:988-1100` |
| 7. Tests and tools | `tests/herds.rs` (13 tests, 3 property tests: conservation, restore equals hash, composition, kills do not resurrect, extinction event for a herd of 2); `tests/habitat.rs`; `sim herds` (200 days default); `bench scale --only herds,members`; Java `HerdGameTests` (8), `HabitatGameTests` (3). | `crates/rtsim-ecology/tests/herds.rs:90,127,238,533,735,800-847`; `crates/rtsim-cli/src/sim/herds.rs:1-45`; `bench/results/README.md:200-223` |

Notes: `FOUNDING_SEED` and `HERD_SEED` are constants, not the world seed (`worldmodel.rs:48`; `jni/engine/herds.rs:66`).
A materialised member does not die off-screen of age or anonymous causes (H2-35). The sim's latent herds and vanilla's
own animals of the same species never interact, so the same animals are represented twice at founding. Stale doc:
`docs/overnight-choices-herdsmc.md:100` ("No herd is ever founded in the game") is superseded by `worldmodel::simulate`.

---

## 3. Human villages and villagers

| Question | Finding | Citations |
|---|---|---|
| 1. Lives where | Rust: one `Node` per village with `Local { pop, resources }` and a `Settlement` row (`NodeKind::Village(Culture::Human)`). Java: villagers are plain vanilla entities saved by vanilla; `village/{Census,Residents,VillageRegistry,VillageAdoption}.java` track them. The sim never owns a villager (C2, PLAN 8.2, 10.8). | `crates/rtsim-settle/src/lib.rs:54-70,121-135`; `crates/rtsim-world/src/world.rs:546-640`; `core/village/*.java` |
| 2. How members appear | A village joins when its structure start generates (C22): VILLAGE_ADOPTED, then `Input::Join` with `pop = (pieces/3).clamp(4,30)` (PLACEHOLDER V5). The first VILLAGE_CENSUS replaces that guess (it may land above or below); later censuses only raise it. Villagers themselves are vanilla's (worldgen, breeding, zombie curing). The sim never spawns a human villager; `growth_check` is ribbit-only. | `crates/rtsim-jni/src/engine/phase1.rs:1288-1330,1059-1090`; `phase1/catalog.rs:145-152`; `docs/overnight-choices-phase1jni.md:22` (V9); `crates/rtsim-world/src/ribbits.rs:287-291` |
| 3. How members leave | Sim pop never decreases. A player-killed resident is journaled RESIDENT_KILLED and becomes a sentiment/news report only (`Input::VillageReport` kind 8). Deaths by zombies, dragons, raids are not reported as pop changes. Raids steal stock and kill raiders, never pop. | `HurtObserver.java:103`; `phase1.rs:1431-1465`; `crates/rtsim-world/src/inputs.rs:323-335`; `crates/rtsim-conflict/src/engine.rs:3050-3085` |
| 4. At zero | A village with all villagers dead keeps its stored pop: it keeps consuming, keeps its treasury target and keeps being served by caravans. A census reporting 0 residents is ignored (`if residents == 0 { return; }`). Nothing returns villagers. Vanilla may repopulate by its own rules, which the sim neither models nor calls. The node is never removed. No document describes a fully killed village. | `phase1.rs:1071-1073`; `PLAN.md:1274-1280` (10.8, 10.9: civilians and village founding are future direction) |
| 5. Interactions | Pop sets consumption (`household = pop x need`), treasury target (`Mbar = k_days x pop / L_mint`), craft capacity, golem cap (`pop/5`), raid defence (`pop x 0.25`). Works purse buys golems for a threatened village and decor. Killed residents and robbed caravans change sentiment, hence prices and golem gossip. | `crates/rtsim-trade/src/build.rs:313-317,391-420`; `labour.rs:92-103`; `works.rs:442-470,823-884`; `crates/rtsim-conflict/src/engine.rs:3037-3045` |
| 6. Parameters | V5 adoption guess, `Census::INTERVAL = 100` ticks, `VillageRegistry.MARGIN = 16`, `village_defence_per_pop = 0.25`, `WorkCap::PerResidents(5)`, `works_threat_days = 24`, `k_days` (P9), `money_sink = true`. All PLACEHOLDER. Cap on pop: 30 at adoption; the census can set higher with no cap found. | `catalog.rs:6,145-152`; `Census.java:36`; `VillageRegistry.java:34-35`; `tunables.rs:31-32`; `works.rs:872`; `config.rs:101,144` |
| 7. Tests and tools | `sim quiet`, `sim phase1`; `crates/rtsim-world/tests/quiet.rs:30,47,63`, `tests/join.rs`. No test or doc measures a village whose villagers die out. | `docs/overnight-choices-phase1.md:99-118`; `crates/rtsim-cli/src/sim/quiet.rs:1-47` |

* The village `pop` is a one-way ratchet. Only two code paths write it after adoption: the census and ribbit growth
  (`set_population`, `ribbits.rs:369`); it floors at 1 (`inputs.rs:201`).
* Census only counts loaded chunks (`Census.java:62`), so a never-loaded village keeps the guess forever.
* The only sim-made bodies at a human village are iron golems from the works purchase (`works.rs:811-884`).

---

## 4. Ribbit villages

| Question | Finding | Citations |
|---|---|---|
| 1. Lives where | Same node/market as humans, `NodeKind::Village(Culture::Ribbit)`. `World.growth: node -> {houses, since, stamp}`; `World.bands`. Java: `neoforge/ribbits` plugin; core `growth/{HouseBuilder,ResidentSpawner}.java` place houses and spawn residents. | `crates/rtsim-world/src/ribbits.rs:287-321,392-409,423-460`; `core/growth/ResidentSpawner.java:26-50` |
| 2. How members appear | Adoption guess `(pieces/2).clamp(4,20)`. Growth: when every needed good and the treasury are at band for `growth_after_days` (10), `grow` emits `PlaceHouse` (template = seed mod 14, radius 48) and **immediately** does `set_population(pop + 1)`. HOUSE_PLACED then queues one `SpawnResident`. Cap `growth_max_houses = 8`. Bands (3 members) are created only at build or at a `join`, and need pop >= 8 and a route of 2 or more. | `ribbits.rs:268-366,392-409,423-460`; `world.rs:392,634-636`; `flows.rs:879`; `catalog.rs:147-152` |
| 3. How members leave | Player kills are reported (RESIDENT_KILLED) with no pop change. Houses never removed. Bands have no leave path (`bands` is only pushed to). Band puppets despawn when no player is near while the closed-form position continues. | `phase1.rs:1431-1465`; `ribbits.rs:591-620`; `phase1.rs:2338-2370` |
| 4. At zero | A killed ribbit is not replaced. `houses` is never decremented, so a village at the ceiling never regrows even if all its ribbits are killed. A resident killed after spawn is not re-issued (op ids are the dedupe key, `ribbits.rs:333`). Bands are immortal (a puppet is respawned by Java each time a player sees one). No founding of ribbit villages. | `ribbits.rs:294-296,331-365,392-409`; `phase1.rs:2513` |
| 5. Interactions | Growth is positive feedback on the economy: more pop raises targets, so prosperity must be re-earned (the 10-day clock resets if a need drops). Lamps cap `pop/2`. Bands raise hosts' sentiment (+5) and carry news; they visit ribbit villages, border human villages and player bases in good standing, never havens. Sorcerer ribbits drop buffs for players the village dislikes. | `works.rs:880`; `ribbits.rs:462-500,558-590`; `DECISIONS.md:62,78,67` |
| 6. Parameters | `growth_after_days` 10, `growth_pop_per_house` 1, `growth_max_houses` 8, `house_templates` 14, `band_min_pop` 8, `band_stops` 3, `band_members` 3, `band_performance_delta` 5. `ResidentSpawner.RETRY_TICKS = 40`. All PLACEHOLDER. Resulting pop cap: adoption guess (4 to 20) + 8. | `config.rs:63-85,130-137`; `ResidentSpawner.java:32`; `HouseBuilder.java:45-72` |
| 7. Tests | `crates/rtsim-world/tests/features.rs:88` (growth, 60 days), `:155` (bands); `tests/morning.rs:142,215`; `sim quiet --growth --bands`; Java `RibbitGameTests` (house on a free plot, touring band, sorcerer, merchant). No long-run growth test with player kills. | `docs/overnight-choices-phase1.md:107,163-164` |

Pop is booked before the house or resident exists (`ribbits.rs:389-391`): if no plot is free or the resident dies,
pop stays up.

---

## 5. Player bases and havens

Bases (C26, P30, E59). Not a population: a pop-0 node (`MarketKind::Base`, no treasury target, no production).

| Question | Finding | Citations |
|---|---|---|
| Appear | Created when one player's dwell counter in a 128-block cell reaches `MARK = 1,800` samples; further players are added to `st.players`. | `crates/rtsim-world/src/bases.rs:85-112`; `core/base/BaseTracker.java:41-50` |
| Leave | `unmark_base` removes a player; with no marker the base leaves every edge's `stops` list. The node and its edges stay. Counters halve every 72,000 ticks; a marked base unmarks below `MARK/4`. | `bases.rs:123-153`; `BaseTracker.java:44-50,138-144` |
| Zero | An inert node; re-marking reuses it. Base nodes accumulate with player roaming (bounded per player by 16 counters, not in total). | `bases.rs:87-90` |
| Parameters | `base_stop_blocks` 256, `CELL` 128, `HALF_LIFE` 72,000, `CELLS` 16 per player (PLACEHOLDER). | `config.rs:89,140`; `BaseTracker.java:41-50` |
| Tests | `tests/features.rs:203`; `sim phase1` marks the farm a base at day 30. | `docs/overnight-choices-phase1.md:165,167` |

Havens (pirate and raider home markets). `NodeKind::Haven(Culture)` joined by ships (`HAVEN_POP = 4`) and conflict
(`pop = max(1, pool_cap/4)`). Joined once, pop fixed, no growth path (`growth_check` is ribbit-only), bands never visit.
Used as raid defence (`haven_defence_per_pop = 1.0`). Nothing sends HAVEN_FOUND in the real game.
(`crates/rtsim-jni/src/engine/ships.rs:66,392-405`; `crates/rtsim-conflict/src/engine.rs:855-862`;
`crates/rtsim-conflict/src/tunables.rs:33-35,98`.)

---

## 6. Caravans, traders and messengers

| Question | Caravans and traders | Messengers |
|---|---|---|
| 1. Lives where | No per-traveller storage: `rtsim-travel` keeps a departure `Lattice` per edge direction and `World::travellers` derives them. A stored `Caravan` row exists only after `promote(key)` (player nearby or a consequence). Java `Phase1`/`bridge/` materialise puppets; `TraderSpawning` cancels vanilla's wandering trader (C8a); `neoforge/ribbits` supplies merchants. | A schedule on every `Edge` (`msg_period`, `msg_walk`). No struct per messenger. |
| 2. Appear | Departures follow the flow (period = slots / volume, at most the travel time). An idle direction keeps scouts every `scout_days = 1`. Java materialises only what a player can see: within `SPAWN_RADIUS = 96`, `MAX_PER_PLAYER = 2`, `MAX_EMBODIED = 16`. A trader notable (seed = caravan seed, heir = home market, lifespan 120 days) is made lazily when a report is booked. | One leaves each end every `messenger_quanta = 6` quanta (four a day), set by `add_edge`, so also on edges added by later joins. |
| 3. Leave | The sim never kills a traveller. Player: `take_member_holding` (Theft), `kill_member_holding` (Killing; a dead leader's notable dies, grudge to the home market), `rob`, or `lose_member` (mob or other mod: no report, cargo leaves the sim). No leader means `dissolve`: the row is removed and holdings go to `outside.player_out`. | Cannot be killed (no bodies). |
| 4. Zero | Killing every caravan body costs goods only. The next lattice departure of every edge is a fresh traveller; idle roads have scouts. The kind cannot go extinct while roads exist. | Cannot reach zero; edges persist. |
| 5. Interactions | Robbing cuts its consignments out of the flow. Reports go to the home market's actor and travel as gossip. Havens and raiders (conflict) rob roads (in Rust). Bases are stops. No guards (C12). | Carry news (sentiment, prices, reports) independent of caravans. |
| 6. Parameters | `caravan_slots` 6, `ribbit_caravan_slots` 6, `scout_days` 1, `trader_lifespan_days` 120 (PLACEHOLDER P18); Java V8 block (PLACEHOLDER). | `messenger_quanta` 6, `messenger_speed_milli` 1,500. |
| 7. Tests | `tests/caravans.rs`, `morning.rs:232-345`, `features.rs:261`; `sim phase1` (scripted robbery at day 35). No test kills every body. | `morning.rs:26,62`. |
| Citations | `crates/rtsim-world/src/caravans.rs:38-90,149-249,307-322,377-442,444-475,555-584,620-634`; `crates/rtsim-travel/src/lattice.rs:1-20`; `crates/rtsim-world/src/config.rs:18-29,115-122`; `crates/rtsim-jni/src/engine/phase1.rs:131-143,1601-1660,2338-2480`; `core/bridge/TraderSpawning.java:5-36` | `crates/rtsim-travel/src/edge.rs:184-232`; `config.rs:25-29,116-117` |

Quirks: a dissolve books the whole load as `player_out` even if a mob caused it (`bridge.rs:291-301`); a dissolved
caravan leaves its notable alive until age death (`caravans.rs:620-633`); `Caravan.notable` keeps a stale id after
the notable dies (`caravans.rs:307-311`).

---

## 7. Notables (dragons, chiefs, traders, captains) and civilians

A notable is a seed, a home, an heir, born/dies ticks, 16 memory slots; temperament is derived from the seed
(`crates/rtsim-knowledge/src/knowledge.rs:424-440`, `notable.rs:1-12`).

| Question | Finding | Citations |
|---|---|---|
| Appear | `create_notable(seed, home, heir, born, lifespan)` books a death at `born + lifespan`. Creators: caravan traders (120 days, lazily); conflict chiefs and heirs (`succeed` always refills the pair; a new heir is conjured for each succession); dragons (species lifespan minus age); pirate captains (200 days). | `knowledge.rs:424-440`; `crates/rtsim-world/src/caravans.rs:307`; `crates/rtsim-conflict/src/engine.rs:894-898,1091-1125`; `crates/rtsim-ecology/src/engine.rs:657`; `crates/rtsim-ships/src/tunables.rs:215` |
| Leave | Age (`world.rs:807-811`), violence (leader kill, dragon kill, chief kill, captain lost). `die` hands the strongest 4 feelings and 4 memories to the heir, else the home. | `knowledge.rs:484-521`; `crates/rtsim-knowledge/src/tunables.rs:93-95` |
| Zero | Records only. Homes (nodes, factions) never die, so memories always have somewhere to go. | `knowledge.rs:486-490` |

Civilians have no head counts: no per-civilian row anywhere; PLAN 10.8 defers it ("Not now. Residents stay vanilla
entities.").

---

## 8. Pirates, ships and crews

All in `crates/rtsim-ships`. **No Java** (`core/engine/NativeCodec.java:116`): not in the real game today.

### 8a. Pirates

| Question | Finding | Citations |
|---|---|---|
| 1. Lives where | `Ships` engine; pirate rows are `Ship` rows with `Role::Pirate { haven }` and a `Pirate` sub-struct; `Haven` holds `crew_pool`. Reference plugin is `FakeShips` in `synth.rs`. | `engine.rs:92-195,449`; `crates/rtsim-jni/src/engine/ships.rs:16` |
| 2. Appear | HAVEN_FOUND calls `add_haven(.., pirates = 1, ..)`: exactly one pirate ship per medium (sea, sky) per haven, with `crew_pool = first_crew` = the largest `pirate_crew` of its media (sea 6, sky 5). Rebuild: `haven_rebuild_ticks` (4 days) after a loss, a replacement launches ONLY if `crew_pool >= pirate_crew`, which it then subtracts; otherwise it reschedules every 4 days forever. Pool growth: only captures (`ceil(standing x crew_conversion_milli/1000)`, or the plugin's `crew_taken`). New captains by `create_notable` at rebuild or on age death. | `engine.rs:832-870` (`:869` first crew), `:3948`, `:3576-3633`, `:3586-3593`, `:3237-3238`, `:3809`; `tunables.rs:131-132` |
| 3. Leave | Sunk only when `survived == 0` in a fight (`pcrew = ceil(crew x survived)`); sunk or captured by a player (SHIP_SUNK/SHIP_CAPTURED); `lose_pirate` destroys cargo and records the grudge. No starvation, age or hunting. **No repair, healing or resupply code** (grep of engine.rs): crews and hulls ratchet down between losses. | `engine.rs:3186-3260,3480-3572,3701-3750,3771-3792` |
| 4. Zero | The haven books Rebuild in 4 days. The pool starts with ONE crew, so without captures at most one rebuild per haven happens. A sea-only haven goes 6 to 0; a haven with both media can rebuild one. After that the fleet stays extinct, retrying silently every 4 days. Arithmetic from the code, **not tested**. With no pirates alive there is nothing to capture a merchant, so the pool cannot refill. | `engine.rs:869,3586-3593`; `tunables.rs:197,219` |
| 5. Interactions | Hunt weight `lane_volume x square_law_surv^exp x aggression / (1 + dist/2000)`; zero-flow lanes are skipped, so piracy depends on merchant traffic. Lane hazard takes a share of each consignment into the haven market. Navy and escorts lower hazard. Pirates raid unwatched player districts and booked train legs within 384 blocks of a lane. Captures convert merchant crews into the pool (positive feedback). A lane is hunted by one haven at a time. | `engine.rs:2530-2610,1406-1430`; `docs/overnight-choices-ships.md:52-66`; `tests/ships.rs:690` |
| 6. Parameters | `pirate_crew` sea 6 / sky 5; `pirate_rest_ticks` 24,000; `pirate_hunt_ticks` 72,000; `hunt_radius_blocks` 5,000; `loot_share_milli` 600; `capture_at` 0.5; `crew_conversion_milli` 500; `captain_lifespan_ticks` 200 d; `coast_raid_blocks` 384; `haven_rebuild_ticks` 96,000. All PLACEHOLDER. Caps: pirates per haven per medium = 1 (literal at `engine.rs:3948`); **`crew_pool` has no cap and no decay** (`u32`, `+=` only). | `tunables.rs:2-3,30-31,76-124,132,197-219` |
| 7. Tests | `tests/ships.rs:477,233,592,829,507,457`; `sim ships` (12 days default, scripted player sinks the first pirate after day 9); bench ship sweep (flat CPU, one pirate). Before this report: no run over months, none checking `crew_pool`. | `crates/rtsim-cli/src/sim/ships.rs:1-30,53`; `bench/results/README.md:178-199` |

### 8b. Merchant and navy ships

| Question | Finding | Citations |
|---|---|---|
| Appear | At network build: `ships per service = period / headway_ticks`, capped at `max_ships_per_service` 6. Late ports get a shuttle service; the network is never re-planned. | `docs/overnight-choices-ships.md:36` (T13); `tunables.rs:47-55,127-136` |
| Leave | Lost to pirates (`lose_merchant`: hold destroyed) or a player. No repair: a damaged crew stays reduced until the ship is lost. Never die of age. | `engine.rs:3393-3430,3186-3200,3701-3710,3734-3792` |
| Zero | Always back: `respawn_ticks` (2 days) after loss, at the slot's next dock stay, full crew, unconditional (no pool). Test `lost_ships_come_back_in_their_own_slot` (`tests/ships.rs:457`). | `engine.rs:3410,3431-3480`; `tunables.rs:127-129` |
| Parameters | `merchant_crew` 4 / 3, `navy_crew` 6, `merchant_strength` 0.5, `navy_strength` 1.5, `max_ships_per_service` 6, `respawn_ticks` 48,000 (PLACEHOLDER). | `tunables.rs:2-3,28-33,47-55,89-91,187-189,218-221` |

VOYAGE.md (player ships as sim voyages) is design only ("Nothing is built", line 3).

---

## 9. Conflict factions (goblin, bandit, raider) and their pools

Crate `crates/rtsim-conflict`. **No Java** (`NativeCodec.java:119`; only the `Module.CONFLICT` switch). Camp structure
ids (`goblinstyranny:goblin_camp`, `minecraft:pillager_outpost`, `illagerinvasion:illager_fort`) are UNVERIFIED.

| Question | Finding | Citations |
|---|---|---|
| 1. Lives where | Rust only: factions in paged row tables, composed with the phase 1 `World` (a camp is a Haven node). Cultures are JSON (`goblin.json`, `bandit.json`, `raider.json`), built in by `include_str!`. | `engine.rs:67-92`; `culture.rs:430-461`; `crates/rtsim-jni/src/engine/conflict.rs` |
| 2. Appear | Factions: `add_faction` only from FACTION_FOUND or a test; idempotent by key. Pool starts full (`pool_cap`). Refill: `pool_at(t) = min(cap, pool + recruits_per_day/TICKS_PER_DAY x (t - pool_at))`, closed form, linear, independent of food, territory, villages, players or kills. Survivors of a war band return to the pool. Chiefs and heirs: conjured at creation and at every `succeed`. Envoys and scouts are NOT drawn from the pool (only `kind.is_war()` takes it); their make-ups are created from nothing and never returned. | `engine.rs:840-900,1053-1066,1082-1121,2224-2252,3170-3210,3865-3875` |
| 3. Leave | Abstract loss in district and village raids (`abstract_loss_share` 0.5), plugin BAND_CASUALTIES (plugin does not exist), war chief killed (rout, heir takes over holding half the grudge, faction cowed `cowed_days`, killer's feud drops 2 rungs). Pool members are removed at muster, so the pool shrinks when a band is sent, not when it dies; the dead are simply not added back at `home`. **A faction itself is never removed** (no `remove_faction`; `TERRITORY_REMOVED` exists but is never emitted). No starvation, age, capture or taming. | `engine.rs:2972-3060,3170-3210,4052-4200,3496-3515`; `boundary.rs:55`; `engine.rs:982-984` |
| 4. Zero | A war party needs the pool to hold at least half its make-up, else the feud waits until the pool will (`pool_ready`). With goblin numbers (arithmetic): raid 12 needs 6, 3 days from empty at 2/day; invasion 26 needs 13, 6.5 days; siege 34 needs 17, 8.5 days; full refill to 60 takes 30 days. In the model the kind comes back with time whatever the player kills. In the real game nothing runs. A player destroying a camp is not reported (no event exists). No test sets the pool to zero. | `engine.rs:2235-2240,1660-1676,1069-1080`; `goblin.json:34-40`; `bandit.json:38-39`; `raider.json:39-40` |
| 5. Interactions | Grievance per (faction, actor) climbs a six-rung ladder (notice, envoy, scouts, raid, invasion, siege; goblin thresholds [10,30,50,70,140,260]). Killing members adds grievance (positive feedback). Tribute buys respite (`respite_days` 6, `tribute_relief` 0.7). Plunder against hated neighbours (goblins hate human and bandit; bandits hate human and ribbit; raiders hate human, ribbit, goblin, bandit), gaps exponential (goblin 12 days, bandit 6, raider 8). No code reduces any village's or haven's `pop`: raids move stock, not people. Raids on player districts go through the industry threat interface. Haven `pop` is `pool_cap/4` at join and not linked to the live pool. | `engine.rs:3248-3305,3031-3100,4052-4120`; `goblin.json:12,15-22,49-52`; `docs/overnight-choices-conflict.md` K2-10,11,14,24,25 |
| 6. Parameters | PLACEHOLDER (all culture JSON, `culture.rs:7-8`): goblin `pool_cap` 60, `recruits_per_day` 2.0, `chief_lifespan_days` 120; bandit 40, 1.5, 90; raider 50, 2.0, 150; `rout_share` 0.5; `territory_radius` 400. `ConflictTunables`: `spawn_quota` 16, `abstract_loss_share` 0.5, `heir_grudge_factor` 0.5, `presence_chunks` 65,536 (the one hard table cap). Pool capped by `pool_cap` (`.min(cap)` in `pool_at`, `.clamp(0, cap)` in `pool_add`; an overdraw is silently forgiven). **Cap on faction count: none.** | `data/cultures/*.json`; `tunables.rs:2-3,24-33,42-43,72-123`; `engine.rs:1057,1064` |
| 7. Tests | `tests/conflict.rs` (proptest `many_factions_conserve_every_good`, 36 villages, 2 to 9 factions, 40 days; chief kill; flat catch-up); `sim conflict` (24 days default, three runs plus restore); `bench scale --only factions` (0.09 to 0.13 against a 0.2 contract). No test or sim measures pool equilibrium, exhaustion or long-run faction behaviour before this report. | `crates/rtsim-conflict/tests/conflict.rs:198,296,477,523-535`; `crates/rtsim-cli/src/sim/conflict.rs:1-30` |

---

## 10. Underground threats, Death Worms, sandworms

On main there is only the natural-mob pressure model on unwatched districts, which has no population of mobs.

| Question | Finding | Citations |
|---|---|---|
| 1. Lives where | `crates/rtsim-industry/src/threat.rs` (`Pressure`, `Defence`, `resolve`), scheduling in `engine.rs`. Java `industry/Threats.java`, `ThreatEvents.java`. | `threat.rs:132-160,231-243`; `engine.rs:2436-2470,2613-2643,2679-2912`; `Threats.java:1-120` |
| 2. How threats arise | Java counts natural hostile spawns in a district and a 1-chunk ring while a player is there at night, per class (ground melee, ground ranged, flying, siege), flushes IND_SPAWN_SAMPLE every 2,400 observed ticks. `rate = spawns / block_ticks` (cumulative). For an unwatched district one geometric draw gives the next attack night, a zero-truncated Poisson gives the count, the attack fires at tick 18,000. A fully lit walled district has zero pressure. | `Threats.java:157-213,259-270`; `threat.rs:132-204`; `engine.rs:61,68,2436-2470,2613-2643` |
| 3. How threats leave | Not tracked. `resolve` computes the attackers' surviving share (walls, Lanchester laws, never better for the defender than the best observed fight) and survivors set the damage. Real fights are journaled as IND_FIGHT (last 16 per class kept). | `threat.rs:350-430,462-484,300-318`; `Threats.java:272-300,377-398` |
| 4. Zero | There is no zero. After an attack, `cooldown_until[class] = at + 24,000`, then the next attack is drawn again from the same rate, as long as the district stays unwatched and the rate is above 0. The rate is a **cumulative ratio that never resets or decays**; killing mobs does not change it. | `engine.rs:2436-2470,2613-2643,2910`; `threat.rs:140-160`; `district.rs:274,336` |
| 6. Parameters | `night_ticks_per_day` 10,000; `untested_weight` 0.5; `theft_cap_ppm` 100,000; `destroy_cap_blocks` 8; `blocks_per_attacker` [0,0,1,1]; `cooldown_ticks` 24,000; `gap_passage_per_column` 0.05; `NIGHT_ATTACK_TICK` 18,000. Natural ground attackers destroy 0 blocks by default. No cap on `count` other than the draw's own. | `crates/rtsim-industry/src/tunables.rs:136-190`; `ThreatConfig.java:36-46` |
| 7. Tests | `threat.rs:861-1011` unit tests; `crates/rtsim-industry/tests/end_to_end.rs:298,620,808-850`; Java `ThreatGameTests`. No long-run decay test. | |

Death Worm, oil nest, sandworms (the Ben-relevant part):

| Item | Status on main | Citations |
|---|---|---|
| `neoforge/sandworms`, `neoforge/terraentity` | Spikes only, outside the default build; they own no logic. Measured: a Sandworms worm does not survive a chunk unload; IaF worm segments are rebuilt next tick. | `neoforge/settings.gradle:25-26`; `research/worm-mods-2026-10-20.md:330-400` |
| Oil nest, fifth class `Burrowing`, Death Worm body | Research plus decision OG-9e (`DECISIONS.md:260`), no code. Proposal: a `Nest` record per oil field, stage rising with time since cleared and oil drained, a cleared nest regrows after `regrow_at`. | `research/underground-threats-2026-10-21.md:260-333` |
| IaF Death Worm today | Desert-only worldgen mob (`#iceandfire:entity_gen/deathworm`, chance 0.0333), never despawns, no rtsim record, no sim lifecycle, no code in this repo that spawns, grows, kills or restores it. | `research/worm-mods-round2-2026-10-21.md:17-30,126-170` |

---

## 11. The Yeti (Frostmaw) and Mowzie's Mobs: plan only

On main there is no handling: a case-insensitive grep for `yeti` over `.rs`, `.java` and `.md` returns nothing;
`Frostmaw` appears once, as the name of a synthetic Ice-dragon in test data (`crates/rtsim-ecology/src/synth.rs:216`).
Everything below is `MOWZIE.md` on branch `plan/mowzie` (`mic-rtsim-mowzie`), commit `f7f2fd6`, not an ancestor of
main. Status: "Nothing is built. Ben approves this design before code" (`MOWZIE.md:3`).

| Topic | The plan | Citations |
|---|---|---|
| Model | Two cultures as data on the conflict engine: `frostmaw` (a culture of one, pool 1) and `umvuthana` (a tribe). Camps are each `mowziesmobs:frostmaw_spawn` den and `umvuthana_grove`. Needs a Java ConflictBridge in the core (none exists): about 2,000 Java and 900 Rust lines. | `MOWZIE.md:125-138,208-213` |
| Discovery | Found when a chunk holding the structure start loads (the IaF lair-finder pattern), FACTION_FOUND flagged LATE if found after generation. "A line laid through a range nobody has explored meets no Frostmaw until that den generates." Density at most 6.25 dens and 6.25 groves per 1,000 x 1,000 blocks. | `MOWZIE.md:220-230,604` |
| Adoption | The den's Frostmaw is adopted on first load as a tagged puppet; the grove's 5 to 9 Umvuthana are adopted and the pool starts at that count; the Umvuthi and cranes stay vanilla. | `MOWZIE.md:234-238,349-356,652` |
| Return after a kill | A killed Frostmaw is the chief's death: band routs, faction cowed 6 days, pool refills at 1/6 per day, so six days later an heir Frostmaw arrives at the den holding half the old grudge. Offered as the answer to the pack's "Natural Boss Spawning". Alternatives listed: never; at once. A killed Umvuthi (not adopted) is replaced by the plugin at the fire pit on the next grove load after the cowed time. | `MOWZIE.md:99-105,268-273,432-434,645` |
| Stability checks proposed | `rtsim-cli sim mowzie`; bench `--only factions,factions-paged`; "days from an emptied pool to a full one" is a named measurement. | `MOWZIE.md:500,629-636` |

---

## 12. Industry districts

Player-built and mirrored by Java (`Districts.java`); the engine follows. `crates/rtsim-industry`.

| Question | Finding | Citations |
|---|---|---|
| Appear | A chunk is industrial when it holds a seed (a player-placed block entity with an item/fluid/energy capability) or is connected to a member chunk; a district is an 8-neighbour connected group plus `/rtsim district merge` links. The engine creates the row on the first IND_DISTRICT. Growth is the player adding blocks (a new `revision` clears chunks and discards the learner). Learning is the only maturation. No district is founded by the sim. | `Districts.java:44-57,305-312,526,831-847`; `engine.rs:637-690,885-887,1050-1062`; `INDUSTRY.md:82-91` |
| Leave | Only when the rescan finds no seeds left and no successor: IND_DISTRICT_REMOVED, then `stop_sim` and `districts.remove`. A raid lost machine sets `Phase::Damaged`. **No abandonment decay, expiry or horizon**: a Learned unloaded district replays indefinitely. | `Districts.java:784-795,831-860`; `engine.rs:739-757,1333-1365,2907-2909`; `district.rs:84-93` |
| Zero | Nothing recreates districts; only a player placing new seeds does. | `Districts.java:784-795` |
| Interactions | Threats (pirates, goblins, dragons, natural) land on districts through the threat interface. Learned pollution rates and drained deposits feed faction grievance (C2-7, C2-8), so districts feed faction grievance but not faction population. Spawn-fed mob farms replay unloaded (I9), so a district can mint goods from nothing, booked as `minted`. | `docs/overnight-choices-conflictjni.md:31-32`; `engine.rs:664-668,1342-1349` |
| Parameters, tests | No cap on district count found. 45 PLACEHOLDER marks in `tunables.rs`; offline buffer is Create's 18 packages (`district.rs:35`). `tests/end_to_end.rs:91-174,347,859`. No long-run district-count test; district removal with goods only has a Java GameTest helper. | `tunables.rs`; `crates/rtsim-industry/tests/end_to_end.rs`; `Districts.java:1120-1145` |

---

## 13. Cross-kind interactions and feedback loops

* Dragons and herds: a dragon takes 1 percent of a herd per hunt and follows it 24 days. Herds with no feedback to the
  dragon count. Dragons are unbounded in count (one per lair), herds are unbounded in growth. Herd extinction does not
  hurt dragons (they keep respawning from lairs and fall back to villages and roads).
* Dragons and villages and caravans: a hungry dragon raids villages (livestock out of the books) and roads (goods).
  Tribute lowers the village's pull. Dragon raids trigger the golem purchase (`village_raided`, golems first for
  24 days). Nothing in the real game reduces a village's `pop` or a caravan's existence.
* Kill and grudge loop (dragons, conflict, ships): killing members adds grievance or a grudge. Dragons inherit it through
  the lair, conflict heirs hold half of it (`heir_grudge_factor` 0.5), pirate havens hold a Killing grudge. Players who
  keep killing get bigger responses (positive feedback). Cowed time and tribute are the negative feedbacks.
* Factions and villages and rails: raids and plunders move stock, never pop. Goblins hate humans and bandits; raiders
  hate everyone listed. Train legs are ambushed in Rust (implemented, untested); Mowzie would add Frostmaw as a Rail
  signal consumer.
* Pirates and merchant lanes: pirates need lane volume (zero-flow lanes skipped); hunts price a lane as expected loss,
  which can shrink the traffic they feed on. Captures feed the crew pool.
* Economy and population: village `pop` sets consumption and treasury targets; ribbit growth raises it. Quiescence
  (`sim quiet`) depends on pop being constant, which is why the village pop only ratchets and does not respond to prices.
* Threat pressure and districts: lifetime-mean spawn ratio, not coupled to any mob population.

---

## 14. What the sim treats as "in the world" beyond explored chunks, and structures

* Herds: the world model samples habitat cells as chunks generate (`ChunkEvent.Load`, `isNewChunk`; the sampler counts
  chunk-generation animals, `HabitatSampler.java:271-286`). A region is judged once it is at least 25 percent known.
  So the sim's herds exist only where chunks have been generated and sampled. Founding follows exploration.
* Dragons: the lair finder hooks `ChunkEvent.Load` plus `ChunkAccess.getAllStarts` and flags LATE when the chunk came
  from disk. Ice and Fire generates the roosts; nothing on main creates lairs. A respawned newcomer is drawn 3,000 blocks
  away "regardless of what is loaded or explored" and flies abstractly until it crosses a player's interest area
  (`engine.rs:3578-3584,3631`).
* Villages: a mixin on `StructureManager.setStartForStructure` (PATCHES.md row 8) reports village starts the moment they
  generate (test `aVillageJoinsWhenItsStructureStartGenerates`). `PLAN.md` 10.9 (C22) says village existence and
  structure generation may "eventually" move into the sim; direction only.
* Mowzie dens and groves (plan): dens in ungenerated chunks do not exist until generated (`MOWZIE.md:227-230`).
* Can the sim place or reuse a structure today? Not on main. The only code that does is `LairPlacer.java` on the
  unmerged oilgate branch, which drives vanilla's `/place structure` path for an Ice and Fire roost, with Ice and Fire's
  own per-chunk chance and spawn-distance rule. D3 (`DECISIONS.md:18`) forbids bespoke entities, blocks and items, not
  other mods' content, so a placed roost or den from a pack mod is within D3.
* Open question carried by the plan docs: what the sim should do about regions already generated before the mod was
  installed (the LATE flag exists for exactly that), and what to do when all structures of a kind are already
  generated and used up (dens or roosts cleared by a player stay records; nothing places new ones except the unmerged
  branch).

---

## 15. UR-1 (delete and reinstall at any time) and what the sim writes into the world

UR-1 is `DECISIONS.md:272`; SRV2 is `DECISIONS.md:245`. A sim reset is "stop, delete `world/rtsim/`, start", with no
compat code before release.

What a reset loses (`docs/deploy-rehearsal-live.md` 6.6 and 6.7): the phase 1 world, markets, sentiments,
caravans, ships. What it re-derives: herds are founded again from the world model within a minute, as many as the
loaded chunks hold; dragons' lairs are found again each session (a dragon from a lair respawns, so a reset is
effectively a respawn of the lair's dragon). What stays in the world: `mic_rtsim_villages.dat` (village registry and
the houses the sim built), the district data, the bases data, the journal, and chunk attachments (`industry_seeds`,
`player_blocks`). Puppets are never saved, so an uninstall leaves none behind.

World-writing population mechanisms, and what each means for UR-1:

| Mechanism | What it writes | After a reset |
|---|---|---|
| Ribbit houses | `HouseBuilder.java` places blocks (`ChunkAccess.setBlockState`) | Houses stay; the sim forgets `houses`, so growth counts from zero again and may place up to 8 more houses on top of the old ones |
| Ribbit residents | `ResidentSpawner.java` spawns entities that vanilla saves | They stay as ordinary entities; the sim forgets them |
| Iron golems from the works purchase | Bodies | Same |
| Lair tribute | Stacks inserted into Ice and Fire lair chests (`Lairs.java`, LAIR_CHEST) | Stay in the chests |
| Taken herd animals | Leashed or tamed animals persist as vanilla entities | Stay, as the player's animals |
| Dragons | Never saved as puppets; adopted dragons live only in sim state until taken or tamed | Lost; the lair respawns one |
| `reconcile` markers | Vanilla strips the markers while rtsim is absent (UR-1 caveat) | |

Conclusion for UR-1: the population mechanisms that write into the world are ribbit growth (blocks and entities), the
golem purchase and lair tribute. Everything else is either puppets (not saved) or sim-only state that a reset loses.

---

## 16. Long-horizon runs on a release build of main (`aca3c3a`)

Run under `nice -n 10`, `cargo run --release -p rtsim-cli -- sim ...`, `--no-write`. Raw outputs were kept outside the
repo. Seed 7 unless stated.

| Run | Result |
|---|---|
| `sim dragons` 400, 1000, 2000, 4000 days | The dragon census stays at 5 (one per lair) at every horizon. Counters at 1000 / 2000 / 4000 days: deaths_age 11 / 26 / 51, respawns equal to the age deaths, claims 16 / 33 / 61, migrations 5 / 7 / 11, births 0. Nothing goes extinct and nothing grows. Respawn keeps the count steady ("Old Scorch Roost stood empty: a young dragon flies in to claim it"). |
| `sim dragons` default (48 days) at HEAD | **4 of 8 checks FAIL**: "a wound becomes a grudge", "a grudge raid on the unwatched farm", "the slayer and the heir", "the heir hunts the player". Same for seeds 1, 2, 3, 7, 11 (seed 11 also fails "threats get bored"). The committed `scenario/results/sim-dragons.json` shows all 8 passing. At HEAD the scripted meeting hits Emberling instead of Cinderjaw (script `crates/rtsim-cli/src/sim/dragons.rs:100-196`). Scenario drift; not diagnosed. **Diagnosed 2026-10-22:** the script's fixed days went stale with phase 1's news by messengers (ac36fbc); the meeting is now found (docs/overnight-choices-dragons.md R27a). |
| `sim herds` 800 days | Total 33,000 to 25,661 (year 1) to 24,526 (year 2) to 27,493, 32,743, 35,004, 40,108, 42,906, 48,997 (year 8), then 67,276 at the next calving, 67,054 at day 800. Births pulse at the year turn. |
| `sim herds` 4000 days (about 41 years) | Totals oscillate between about 40k and 117k at year starts (62,982 at year 10, 73,707 at 20, 105,143 at 30, 86,258 at 41). **Winner-take-all**: Northdown grew from 12,000 to about 62,000 while Riverbend, Windmane and Crag fell under 200 (192, 97, 144 at the end). |
| `sim herds` 4000 days, seed 1 | Windmane (2,500 at the start) hit 0 at about day 3,880 and stayed extinct. Seeds 2 and 3 show small herds crashing to 111 to 197. |
| `sim herds` 200 days (committed result) | 33,000 to 31,892. No extinction. Later scripted checks fail at long horizons because they are scripted for year 1. |
| `sim ships` 12 / 120 / 1000 / 3000 days | Exit 0, all checks pass. After about day 120 hunts stay at 30, rebuilds at 1 and sinkings at 1, while fruitless hunts rise from 127 to 5,887 by day 3,000. Pirates stop making successful hunts in the quiet synthetic economy; cause not diagnosed (probably zero lane volume). **Diagnosed 2026-10-22:** not lane volume. Fights the plugin played killed both crews to the last man and the pirates stayed active with no crew: no lane was worth hunting, and never lost, they were never rebuilt (fixed, docs/overnight-choices-ships.md T27a). Hunts still end once the one-crew pool is spent (row 4). |
| `sim conflict` default and 400 days | No FAIL. A repeating roughly 14-day cycle of scouts, a 12-warrior raid, the director fight, a chief kill with the heir taking over, and a rout. By my reading the pool never empties (2/day refill against 12 spent per cycle). The sim does not print the pool. |
| `sim phase1 --days 1000` | Conservation ok; events per market per day over the last 10 days: 0.0. |
| `sim quiet --days 1000 --every 100` | Windows 5.40, then 0.00 for every later window. |
| `sim industry --days 400` | PASSED (scripted bandit and dragon attacks). |
| Host soak `a_thousand_days_at_max_keeps_the_state_bounded` (`crates/rtsim-host/tests/host.rs:1581`, `--release --exact --nocapture`) | Passed in 10.35 s at 107.9 game days/s. The `agents` column stays at 4 (days 10 to 1000); pages 38,866 B to 27,354 B; heap 4,338 to 4,566 KiB. Measures state size, not population distributions. |

---

## 17. Tests and tools inventory

| Tool | What it measures | Recorded results |
|---|---|---|
| `rtsim-cli sim phase1` | The 11-node scripted world (six human villages, three ribbit, a haven, a base): prices, stocks, flows, treasuries, news. Pop fixed. | `scenario/results/`; `docs/overnight-choices-phase1.md` |
| `rtsim-cli sim quiet` | Market events per market per day with no player; conservation each window; `--growth`, `--bands`, `--works`, `--grow` | Closed 11-node world 2.96, 0.34, then 0.00 from day 200 to 1,000; grid of 1,024 villages 1.31 then 0.10 to 0.14 (`docs/overnight-choices-phase1.md:99-118`) |
| `rtsim-cli sim industry` | Districts under scripted attack | PASSED (section 16) |
| `rtsim-cli sim dragons` | 8 scripted checks over 48 days | Committed all-pass; 4 fail at HEAD (section 16) |
| `rtsim-cli sim herds` | Six synthetic herds and two dragons, 200 days default | 33,000 to 31,892 committed; long horizons in section 16 |
| `rtsim-cli sim ships` | Coast with one haven, 12 days default | Section 16 |
| `rtsim-cli sim conflict` | 24 days, ignore / tribute / converter runs plus restore | "Tribute pays four times in 24 days (17, 86, 96, 98 emeralds)", "two war chiefs killed, two heirs" (`docs/overnight-choices-conflict.md`) |
| `rtsim-cli bench scale` and `bench dragons` | CPU, heap, catch-up (flat-cost contracts) | `bench/results/README.md:178-223`; herds 100 to 1,000,000 members over 80 days; factions 0.09 to 0.13 against 0.2 |
| Host soak `a_thousand_days_at_max_keeps_the_state_bounded` | State size and outbox bounds at max | Section 16; `docs/choices-leg-purge.md` (state-growth fix, 5,000-day soak flat) |
| Archive soak `crates/rtsim-archive/tests/soak.rs` | Storage bounds over 10,000 days | Storage only |
| Property tests | Conservation of goods and herd integers (`Herds::check`), restore equals hash, composition, kills do not resurrect | `crates/rtsim-ecology/tests/herds.rs`; `crates/rtsim-conflict/tests/conflict.rs:523-535` |
| Java GameTests | `DragonGameTests` (9), `HerdGameTests` (8), `HabitatGameTests` (3), `RibbitGameTests`, `ThreatGameTests` | |
| Viewer | `herds.extinct` metric listed at `VIEWER.md:1243` | |

No tool anywhere census-counts populations over years with a player killing them.

---

## 18. Gaps

### Kinds with no source at zero

| Kind | What stays dead, and why |
|---|---|
| Herds | Extinct is permanent: the row is a tombstone (`herds/engine.rs:784-802`), the region mark stays (`worldmodel.rs:292-316`), a name guard blocks a second herd, and no immigration exists. A herd can die out with no player involved (section 16, seed 1). |
| Human villagers | The sim never spawns a human villager; `pop` never falls and a zero census is ignored, so a village emptied of villagers still looks full to the economy. |
| Ribbit residents | `houses` is never decremented and growth stops at 8, so a killed-out ribbit village never regrows. |
| Pirate fleets | `crew_pool` starts at one crew and refills only through captures by pirates that no longer exist: about one rebuild per haven, then extinct (arithmetic, untested). |
| Industry districts | Player-built only. |
| Dragons | Come back only through a known lair; none without a lair. Tamed dragons are replaced too (reading). |
| Conflict factions | Come back in Rust only (pool refill); the Java side is absent so nothing runs in the real game. A destroyed camp is not reported. |
| Yeti / Frostmaw, Mowzie tribes | Nothing on main. The plan has an heir after 6 days (not built). |
| Death Worm, oil nest | Nothing on main. IaF's own Death Worm is vanilla-rule worldgen. |

### Unbounded or ratchet state

| State | Why it is unbounded or one-way |
|---|---|
| Dragon count | No cap; a respawn at every lair, and lairs are never removed. On the unmerged oilgate branch `PLACE_LAIR` adds a lair near every oil field a player visits. |
| Herd totals | No carrying capacity and no density term in births (section 16: 40k to 117k oscillation, winner-take-all). |
| `Haven::crew_pool` | No cap, `+=` only. |
| Village `pop` | Only up after the first census; ribbit growth books pop before the house exists. |
| Base nodes, bands, haven and faction nodes | Never removed. |
| Threat pressure | Cumulative ratio, never decays. |
| Counters | Conflict `plunders` and `chiefs`; works `bought`; `HerdLedger` counters. |
| Region marks | `mark::HERDS` never cleared. |
| Notable ids | `Caravan.notable` can hold a stale id; a dissolved caravan leaves a live notable until age. |

### Missing tests and open diagnoses

* No long dragon or herd stability check with player kills in it. Before this report no run was longer than 200 days
  (herds) or 48 days (dragons).
* No test sets the conflict pool to zero, kills all pirates and waits past one rebuild, or kills all villagers.
* The restart-adopts-natural-dragons path and the taming-triggers-respawn path are untested.
* The dragon scenario checks fail at HEAD against a committed all-pass result: it needs a fix or a note (scenario
  drift, not diagnosed). Fixed 2026-10-22 (R27a).
* Pirate hunts stop after about day 120 in `sim ships` (not diagnosed). Diagnosed 2026-10-22: crewless pirates
  kept active (fixed, T27a); after that, the pool (row 4).
* The pool of a conflict faction is not printed by `sim conflict`, so equilibrium cannot be read from the output.
* No census tooling for populations inside the soak: the soak measures bytes and heap, not who is alive.
* Java is absent for ships and conflict, so none of the pirate and faction behaviour above has run in a real world.
