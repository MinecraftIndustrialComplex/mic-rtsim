# Mowzie's Mobs: factions and boss encounters

**Status:** design for Ben's review, 2026-10-21. Nothing is built. Ben approves this design before code.

**Decided, and binding here:**
- **Ben's request:** "go ahead and plan out some support for Mowzie's mobs, creating factions for the
  mobs in them, using bosses from them for encounters / raids. (I.E. Yeti's which can spawn in snow
  biomes not being big fans of train networks going through their territory.)" The "yeti" is the
  Frostmaw: the only Mowzie's mob whose home is a snow biome.
- **The pack's own TODO** (`pack/README.md:17-26`): "Boss raid system", "Natural Boss Spawning",
  "Remove items & progression from boss mods: ... mowzies mobs", "Mowzies mobs: Natty spawns". The
  third conflicts with parts of this plan (MZ-7).
- **C16, C17, C18:** bandits, dragons, natural mobs and **bosses** attack supply lines and factories,
  even while the owner is away or offline; unwatched attacks resolve abstractly and apply on load.
- **I21 to I25:** the defence survey, hazards and Lanchester, bounded theft and damage, trains lose
  cargo but never track, caps and cooldowns. **C24, C25:** havens, heirs who inherit grudges, bands
  that get bored, temperament, groups as one unit. **P14 (changed):** a signal with no direct actor
  counts against the player with the most time in its chunks. **P28:** watched fights place the same
  bounded sim damage as unwatched ones (decided for dragons; MZ-5 extends it).
- **Also in force:** D1 (judge only by late-game runtime cost on a weak PC), D3 (no bespoke blocks,
  items or entities), D4 (the core depends on no pack mod), D7 (mob-like bodies), A10 (a creature a
  player takes leaves the sim), A15 (booked legs), C31 (Deep Time's wrapped world), P8 (bodies from
  other mods), P15 (body kinds; the Java side registers them in code today, `BodyKinds.java:41`), U10.

**Sources.** Mowzie's Mobs 1.8.2: `mowziesmobs-1.21.1-1.8.2.jar`, its sha512 (`ab10d2f7...`) matched
against `pack/mods/mowzies-mobs.pw.toml`, decompiled with CFR. Jar paths below are relative to
`com/bobmowzie/mowziesmobs/server/` and **line numbers are CFR's**, not the mod's source. Data files
are the jar's `data/mowziesmobs/`. TaCZ 1.1.8-hotfix-r6 and Create 6.0.10 jars were matched against
their `.pw.toml` hashes and read with javap. Pack data: `pack/config`, `pack/defaultconfigs`,
`pack/kubejs`; no pack file overrides Mowzie's config, so its defaults hold. Vanilla: the decompiled
1.21.1 sources in the Gradle cache. mic-rtsim: this repository at `22686e8`; `core/` means
`neoforge/core/src/main/java/com/minecraftindustrialcomplex/mic_rtsim/`. Nothing here was run in a
game: "verified" means read in code or data.

**Numbers:** every number is a verified fact, arithmetic from one, or a labelled TUNABLE or
PLACEHOLDER. Every PLACEHOLDER says what it tunes and how to measure it (§4.11). Anything
unverified says UNVERIFIED.

---

## 0. What the player sees

The snowy peaks and slopes hide Frostmaw dens: an ice ruin with a huge white ape asleep in it,
holding a blue crystal in its fist. Today it never leaves. With this plan, it minds its range. Run a
Create railway through a Frostmaw's range and, the more trains you run, the angrier it gets. First
you hear it roar as you ride through. Then it leaves the den and waits by your line. Trains that pass
it while you are away lose cargo; trains you ride get ice breath and ice balls. Push it further and it
camps at your nearest station. You can kill it, but arrows do nothing to it, so you bring guns. You
can sneak up while it sleeps, invisible, and steal its crystal: then it has no ice attacks, and your
walls hold it. You can move your line out of its range. Or you run your trains and pay in cargo. Kill
it and the range is quiet for a while, until another Frostmaw moves into the empty den, carrying its
grudge.

In the savannas, Umvuthana groves are villages of masked warriors around a fire pit, ruled by the
Umvuthi, a sun-priest boss. Today they attack anyone unmasked who comes close. With this plan, the
grove is a tribe. Log its acacias, mine its hills, kill its people, and it notices: a gong sounds in
the distance. Then a masked envoy walks to your base and asks for gold. Pay, and the gold ends up in
the grove's chests. Refuse, and raptor-masked hunters come for your herds and the villages' herds,
then a war band of masked warriors. Wear an Umvuthana mask and the tribe leaves you
alone, as it does today. Kill the Umvuthi at its fire pit (its healers make it untouchable until you
kill them) and the tribe breaks: its raiders flee and it stays quiet for days.

## 1. Reuse table

| What happens | The real mechanic | Mod (verified id or class) |
|---|---|---|
| A Frostmaw's den | `mowziesmobs:frostmaw_spawn`, structure set `frostmaw_spawns`: random spread, spacing 25 chunks, separation 8, at least 7 chunks from `minecraft:villages`; the template holds a sleeping Frostmaw with its crystal | mowziesmobs |
| The Frostmaw | `mowziesmobs:frostmaw` (`EntityFrostmaw`): 250 health, 10 attack, knockback resistance 1, swipe, slam, ice breath, ice ball | mowziesmobs |
| Its prize | `mowziesmobs:ice_crystal`: dropped when a player kills it while it holds the crystal, or stolen while it sleeps | mowziesmobs |
| Its warning | sound `mowziesmobs:frostmaw.roar` | mowziesmobs |
| A tribe's home | `mowziesmobs:umvuthana_grove`, structure set `umvuthana_groves` (spacing 25, separation 8, 7 chunks from villages): the Umvuthi at the fire pit, 5 to 9 Umvuthana, chests with `mowziesmobs:chests/umvuthana_grove_chest` loot | mowziesmobs |
| Warriors and traders | `mowziesmobs:umvuthana` (`EntityUmvuthanaMinion`): 8 health, 3 attack, a mask, trades gold nuggets with masked players | mowziesmobs |
| The chief's healers | `mowziesmobs:umvuthana_crane`: the Umvuthi summons them each time it loses 45 health; they heal only the Umvuthi and give it Sunblock, under which it takes no damage | mowziesmobs |
| Hunters | `mowziesmobs:umvuthana_raptor` and `umvuthana_follower_raptor`: a raptor leads 4 or 5 followers; the raptor hunts animals and its followers take its target | mowziesmobs |
| The chief | `mowziesmobs:umvuthi` (`EntityUmvuthi`): 150 health, sun abilities, summons Umvuthana; takes 7 gold blocks once from a masked player, then grants Sun's Blessing | mowziesmobs |
| Peace with the tribe | an Umvuthana mask on the head (`mowziesmobs:umvuthana_mask_*`): Umvuthana, raptors and the Umvuthi do not target the wearer | mowziesmobs |
| The tribe's warning | sound `mowziesmobs:block.gong` | mowziesmobs |
| Factions, grievance, the ladder, envoys, raids, routs, heirs | rtsim-conflict (`crates/rtsim-conflict`) | mic_rtsim |
| Train lines and their traffic | industry line edges from Create train arrivals (`crates/rtsim-industry/src/engine.rs:109-119`, `2076-2087`) | mic_rtsim, mic_rtsim_create |
| Cargo lost from a train | I24: `attack_train` (`crates/rtsim-industry/src/engine.rs:2914`) and `IND_CARRIAGE_DELTA` on `Carriage.storage` | mic_rtsim_create |
| Defending a station or base | the I21 survey: walls 2 blocks or more, fences and walls; Create Radars fire control with Create Big Cannons; Create saws, drills, crushing wheels; iron golems, which attack every Mowzie body in this plan (`INDUSTRY.md:574-585`) | create, createbigcannons, create radars, minecraft |
| Weapons that hurt a Frostmaw | TaCZ guns: `EntityKineticBullet` extends `Projectile`, not `AbstractArrow`, so the Frostmaw's arrow immunity does not apply | tacz |
| A train running a Frostmaw over | Create run-over damage (`trainsCauseDamage = true`, the default) | create |
| Stealing the crystal | vanilla invisibility (`minecraft:potion` of invisibility) | minecraft |
| Puppets: bodies the sim owns | `BodyAdapter` (`core/body/BodyAdapter.java:16`) and `BodyKinds.registerBody` (`core/body/BodyKinds.java:41`), as dragons and herds use them | mic_rtsim |
| Camps found when they load | the lair finder pattern: a structure start seen on `ChunkEvent.Load` (`neoforge/iceandfire/.../Lairs.java:44-60`, `114-125`) | mic_rtsim_iceandfire (pattern) |
| Tribute put in a real chest | `TRANSFER_TO_CONTAINER` 0x8005 (`docs/boundary.md:286`), as the dragons' `LAIR_CHEST` uses it (`docs/boundary.md:850`, `880-908`) | mic_rtsim |

## 2. Custom-roll ledger

Sizes are estimates by analogy with code that exists; they are not measured.

**Engine (Rust, rtsim-conflict and the composition), about 900 lines plus tests:**
1. **A Rail signal** (`SignalKind::Rail` = 6, appended) and a line feed in the composition next to
   `feed_districts` (`crates/rtsim-jni/src/engine/conflict.rs:596-686`): industry line edges crossing a
   territory become continuous sources. A continuous source with no district also sets a position
   focus (today `refresh_rates` keeps a focus only with a district, `engine.rs:1513-1517`) (§4.3).
   About 180 lines.
2. **Empty parties** are allowed and their rungs skipped (today `Culture::resolve` refuses an empty
   party, `crates/rtsim-conflict/src/culture.rs:378-382`). About 40.
3. **Chiefs that stay home** (a culture field: which party kinds the chief rides with; today any war
   party, `engine.rs:2265-2271`), and **a chief killed at the camp** (today `chief_killed` needs a
   party, `engine.rs:4130-4140`). About 120.
4. **A culture of one:** the pool counts members away from home, so a one-Frostmaw den never fields a
   second Frostmaw while the first is out. About 20.
5. **Lying in wait** at a position or station: a war party that finds no district there waits out its
   patience and ambushes the train legs through its region (today it "finds nothing to raid",
   `engine.rs:2869-2878`, and ambushes only the first train it crosses on the way out,
   `engine.rs:2518-2521`). About 140.
6. **Threat source and class from the culture:** ambushes and district raids use the culture's source
   (`Boss`) and each unit's class and carry (today both hard-code `Bandit`, and ambushes `GroundMelee`,
   `engine.rs:2663-2664`, `2947`). A chief state bit picks between two unit rows (the Frostmaw with or
   without its crystal). About 70.
7. **Loot that is destroyed, not carried home:** a culture flag; the goods leave the books as the
   dragons' eaten food does. About 40.
8. **A gold good** (`gold_nugget`) in rtsim-world's catalogue and the phase 1 item map
   (`crates/rtsim-jni/src/engine/phase1/catalog.rs:28-38`), if Ben takes MZ-6 (a). About 20.
9. **Hunted animals:** a raptor raid that beats a district's defences owes the district dead animals,
   applied when it next loads, under a cap (like I23's destroyed blocks); plunder of a village takes
   its livestock goods, eaten (MZ-8). About 120.
10. **Reports the engine has no record for:** a grove's population at discovery (FACTION_FOUND's
    reserved word, `docs/boundary.md:1363`), a chief killed at the camp, a chief's state (health,
    crystal), trades at a haven into `Conflict::traded` (`engine.rs:2045`, no caller today), and a
    haven's chest (latest-wins, as `LAIR_CHEST`). About 150 with the decoding.
11. **Two cultures** as data: `data/cultures/frostmaw.json` and `umvuthana.json`, added to `builtin()`
    (`culture.rs:430-432`, `449`). About 120 lines of JSON.

**Java, about 2,000 lines plus about 1,000 of GameTests and kit:**
12. **ConflictBridge** in the core (MZ-1): the conflict module's Java half, which does not exist yet
    (`crates/rtsim-jni/src/engine/conflict.rs:5`): territories, the territory signals from vanilla
    block and death events, the envoy and its payment, the raid director (waves on an arc), steering,
    recalls, casualties. Vanilla only (D4). About 1,000, by analogy with `core/herd/HerdBridge.java`
    (940).
13. **The Mowzie plugin** `neoforge/mowziesmobs` (mod id `mic_rtsim_mowziesmobs`): body adapters for
    the Frostmaw, the Umvuthana warrior, raptor pack and envoy; camp finding for the
    two structures; adoption of the den's Frostmaw and the grove's Umvuthana; a mixin on
    `EntityUmvuthi.rememberTrade` and an event on minion trades; the notice sounds. About 900 to 1,100,
    by analogy with the Ice and Fire plugin (`DragonBridge.java` 1,029, `Lairs.java` 409).
14. **A strength hook for observed fights:** `IND_FIGHT` counts one per attacker today
    (`core/industry/Threats.java:384-385`); plugins supply a per-entity strength (§4.10). About 40.
15. **GameTests and a kit** (`MowzieGameTests`, `MowzieKit`), about 700 and 300, by analogy with
    `DragonGameTests.java` (688) and `DragonKit` (329); and **`rtsim-cli sim mowzie`**, about 450, by
    analogy with `sim/conflict.rs` (423).

No new blocks, items or entities.

## 3. What happens today

### 3.1 Mowzie's Mobs in the pack

Mowzie's Mobs 1.8.2 runs with its default config. The pack touches it in one place: Structurify
disables the Wroughtnaut's `wrought_chamber` (`pack/config/structurify.json:405-411`, pack commit
`893bd9f`, "Remove structure causeing crash from mowzie's"). What the jar places and spawns:

| Mob | Where it lives (verified) | What it does today (verified) |
|---|---|---|
| Frostmaw | `frostmaw_spawn` dens in snowy biomes that are not ocean, river, beach, forest or taiga, height 50 to 100, away from villages and outposts (`ConfigHandler.java:443`). In this pack's biome set: 15 biomes, including `frozen_peaks`, `snowy_slopes`, `snowy_plains`, `ice_spikes` and Terralith's `emerald_peaks`, `glacial_chasm`, `rocky_mountains`. | Sleeps until a player comes within 8 blocks while it has a target (`EntityFrostmaw.java:590-606`); fights; goes back to sleep after 1,200 ticks without a target (`580-589`). Targets players only (`284`). Persistent while it holds its crystal (`977`). Wanders inside its restriction (`208-209`). |
| Umvuthi | `umvuthana_grove` in the 8 savanna biomes, height 50 to 100 (`ConfigHandler.java:466`) | Placed at the fire pit, finalized as a structure spawn and restricted there (`UmvuthanaGrovePieces.java:276-288`). Persistent (`EntityUmvuthi.java:393`). Targets unmasked players, iron golems, zombies, skeletons, Elokosa (`318-345`). In a fight, summons Umvuthana while fewer than 4 are within 50 blocks (`488`), and cranes each time it has lost 45 health (`469-471`; `spawn_healers_threshold`, `ConfigHandler.java:473`). A crane heals only the Umvuthi (`EntityUmvuthanaCrane.java:174`) and gives it Sunblock, under which it takes no damage (`EntityUmvuthi.java:628-632`). |
| Umvuthana | 5 to 9 in each grove, restricted to 25 blocks of its centre (`UmvuthanaGrovePieces.java:494-507`) | Persistent (`EntityUmvuthanaMinion.java:253`). Target unmasked players, zombies, skeletons, Elokosa (`133-151`). Trade with masked players (`107`, `236-240`). |
| Umvuthana raptor | natural spawns in savanna, weight 5, groups of 1, away from villages, outposts and groves (`ConfigHandler.java:539`) | Leads 4 or 5 followers (`EntityUmvuthanaRaptor.java:191-199`), who take its target (`EntityUmvuthanaFollowerToRaptor.java:47`) and hunt on their own once leaderless (`94-97`). The raptor hunts animals with attack damage under 3 and a volume between 0.1 and 6 (`EntityUmvuthana.java:338-342`, `EntityUmvuthanaRaptor.java:88-90`): cows, sheep, pigs, chickens. |
| Naga | natural spawns where a biome is a beach and a mountain or hill, and on `minecraft:stony_shore`, up to y 68 (`ConfigHandler.java:526`); in this pack, `stony_shore` only | Stays within 50 blocks of home (`EntityNaga` `MAX_DIST_FROM_HOME`, `175`, `582`). |
| Sculptor | the `monastery` on mountain peaks, y 120 to 200 (`ConfigHandler.java:420`) | A stationary trial: it only targets what hurts it (`EntitySculptor.java:355`). |
| Wroughtnaut | the `wrought_chamber`, disabled in this pack | Immune from the front (`EntityWroughtnaut.java:202-231`). |
| Elokosa, Foliaath, Lantern, Grottol, Bluff | jungle (`ConfigHandler.java:389`), jungle (`551`), magical forests that are not snowy (`514`), stone below y 16 outside mushroom biomes (`502`), none (the Bluff has no natural spawn: empty biome list, `401`) | Ordinary mobs; the Grottol only seeks vanilla minecarts (`EntityAIGrottolFindMinecart`). |

None of them breaks blocks: no Mowzie's mob checks `mobGriefing`, and the Frostmaw's ice breath turns
water to ice for 140 ticks only (`EntityIceBreath.java:210`).

### 3.2 Pack notes that disagree with the jar

These are pack-side corrections, outside this repository; listed so the plan rests on the jar.
- `pack/modDocs/bosses.md:51-65` says the other bosses "spawn naturally in their biomes"; the
  Frostmaw, Umvuthi and Sculptor are structure spawns. Its health figures differ from the jar's
  attributes: Frostmaw 100 (jar 250), Umvuthana 30 (jar 8), Naga 60 (jar 30), Bluff 40 (jar 45),
  Sculptor 100 (jar 140); it lists a Wroughtnaut at 150 and a ferrous one at 80 (the jar registers only
  `ferrous_wroughtnaut`, 40: `EntityHandler.java:90`, `EntityWroughtnaut.java:198`). Its biomes are
  wrong for the Naga (stony shore, not swamp or jungle), the Bluff (no natural spawn) and the Sculptor (mountain-peak monastery, not jungle underground).
- `pack/kubejs/server_scripts/tacz_new_recipes.js:883`, `934`, `994` still use `naga_fang`,
  `bluff_rod` and `ice_crystal`; `pack/modDocs/gun-crafting-plan.md:294-295` says boss-drop materials
  (it names naga fang) are dropped from gun recipes.

### 3.3 The engine today

The conflict engine (rtsim-conflict) exists and runs in Rust tests and `rtsim-cli sim conflict`
(`crates/rtsim-cli/src/main.rs:81-85`), but **no Java plugin exists** (`crates/rtsim-jni/src/engine/conflict.rs:5`;
`docs/boundary.md` §9 "a future conflict plugin"). Its cultures are data (`goblin.json`,
`bandit.json`, `raider.json`). What it does, all in `crates/rtsim-conflict/src/engine.rs`:
- **Grievance** per faction and actor is a closed form: dG/dt = r - λG between events
  (`grievance.rs:1-18`). Impulses jump it (`signal`, `1226-1262`); continuous sources set a rate over a
  tolerance (`refresh_rates`, `1480-1517`: `eff = (raw - tolerance_per_day / TICKS_PER_DAY).max(0) *
  weight`, `1506`).
- **The ladder:** None, Notice, Envoy, Scouts, Raid, Invasion, Siege (`culture.rs:84`); one party per
  rung, sent when the grievance crosses the rung's threshold after the dwell on the rung below
  (`plan_feud`, `1536-1577`; `escalate`, `1660-1745`). A war party draws on the pool and needs at
  least half its make-up (`2237-2238`).
- **Targets:** the focus district, else the focus position, else the camp (`target_of`, `1796-1806`).
  At a position with no unwatched district within 64 blocks a band "finds nothing to raid"
  (`resolve`, `2869-2878`).
- **Trains:** a war band on its way out ambushes the first train leg it crosses, on a roll under
  `ambush_milli` 500 in 1,000 (`2496-2534`; `tunables.rs:103`), through industry's `raid_train`.
- **Rout and heir:** the war chief's death routs the band, cows the faction for `cowed_days`, drops the
  feud 2 rungs (`chief_rout_steps`, `tunables.rs:104`), and the heir succeeds (`4124-4196`).
- **Tribute:** paid to an envoy or at the camp; with no demand open it counts as a gift
  (`pay_tribute`, `1977-2021`).

## 4. The design

### 4.1 Two factions

| Faction | Camp | Chief | Members | Hates |
|---|---|---|---|---|
| `frostmaw` | each `mowziesmobs:frostmaw_spawn` | the Frostmaw | none besides it: a culture of one | nobody: it plunders nothing |
| `umvuthana` | each `mowziesmobs:umvuthana_grove` | the Umvuthi, who stays home (MZ-3) | the grove's Umvuthana warriors and raptor packs | `human` (village herds) |

Every value the jar does not give and this section does not set is goblin.json's, labelled
PLACEHOLDER (§4.11). The two cultures share goblin's thresholds `[10, 30, 50, 70, 140, 260]`, dwell
days `[0, 0.5, 0.5, 1, 4, 6, 8]`, half-life 6 days, patience 2 days, muster 1 day, cowed 6 days,
march 0.02 blocks a tick (`crates/rtsim-conflict/data/cultures/goblin.json`).

**Territory radius 200 blocks** for both: half of their structure sets' spacing (25 chunks × 16 / 2).
Each set places at most one start per 400 × 400-block cell, so each has at most 6.25 per
1,000 × 1,000 blocks (fewer where the biome is wrong). `reach_blocks` is 0 for both (PLACEHOLDER;
goblin's is 300): the engine matches a source to a faction by the source's position, out to the
radius plus the reach (`engine.rs:909-931`), so with no reach only what happens inside the circle
counts.

**Camps are found** as the Ice and Fire plugin finds lairs: when a chunk holding the structure's start
loads, the plugin reports FACTION_FOUND (flag LATE when found after generation) with the structure from
dictionary 4. A camp in a chunk nobody has generated does not exist yet: a line laid through a range
nobody has explored meets no Frostmaw until that den generates. That is a limit of the design.

### 4.2 The Frostmaw: a culture of one

**Who it is.** The den's Frostmaw becomes the faction's chief and only body. On the den's first load
the plugin adopts it (a tagged puppet the sim owns and vanilla never saves, as `DRAGON_ADOPT` does,
`docs/boundary.md:898`). From then on the engine keeps its state: health, whether it holds its
crystal, and where it is. When the den loads with the Frostmaw at home, the plugin materializes it
there asleep (`active` false), with its health and `has_crystal` (NBT keys verified,
`EntityFrostmaw.java:960-975`). Asleep it heals 0.3 a tick (`heals_out_of_battle`, on by default, `ConfigHandler.java:446`;
`EntityFrostmaw.java:594-596`), full
from nothing in about 830 ticks, so the engine counts a Frostmaw back home as healed: its health only
matters while it is away.

**Its pool is one.** `pool_cap` 1 (the den holds one Frostmaw, `frostmaw_spawn.nbt`). Its raid party is
the chief alone: one `frostmaw` unit, flagged as the chief (`BAND_MATERIALIZE` member stacks carry a
chief flag, `docs/boundary.md:1400`). Its Envoy and Scouts parties are empty, so it skips those rungs
(ledger 2): a beast makes no demands.

**What angers it.** Rail (§4.3), with killing and building inside its range from goblin. It ignores
logging and mining (weight 0): nothing in the jar ties it to trees or ore.

**The ladder.**

| Rung | What happens |
|---|---|
| Notice (10) | The plugin plays `mowziesmobs:frostmaw.roar` to the player the feud is against, from 32 blocks (PLACEHOLDER) toward the den, if they are in the range now, else the next time they enter it this session. Nothing else changes. |
| Envoy, Scouts | Skipped (empty parties). |
| Raid (70) | It leaves the den and walks to the focus: the point of the line nearest the den. There it lies in wait for its patience (2 days, PLACEHOLDER), ambushing train legs that pass (§4.4). Then it goes home, bored (C24). |
| Invasion (140) | It camps at the line's station nearer the den: every leg from or to that station passes it. |
| Siege (260) | As Invasion, repeated every `siege_interval_days` (1.0, PLACEHOLDER). |

**Its class follows its crystal.** With the crystal it throws ice balls at targets 14.5 blocks away
or more and breathes ice at those within 13.5; both need the crystal (`EntityFrostmaw.java:559-578`).
A wall does not stop that, so it counts as `ground_ranged`. Without the crystal it has only its swipe
(6.5 blocks) and, below 60% health, its slam (8.5): `ground_melee`, which walls stop (I22,
`INDUSTRY.md:595-597`). Ledger 6 picks the unit row by the chief's crystal bit.

**When it dies.** Killed anywhere, it is the chief's death: the band routs, the faction is cowed for
`cowed_days`, the feud drops two rungs. Its pool refills at `recruits_per_day` = 1 / `cowed_days`
(PLACEHOLDER, 1/6), so six days later an heir Frostmaw arrives at the den, with its crystal (the den's
template has `has_crystal` 1), holding half the old grudge (`heir_grudge_factor` 0.5,
`crates/rtsim-conflict/src/tunables.rs:107`). This answers the pack's "Natural Boss Spawning"
(MZ-4). While the pool is empty the den is empty.

### 4.3 Rail: how a Frostmaw feels a train line

The industry engine already learns every line: on each train arrival it updates a line edge for the
pair of stations, with trips, cargo and the first and last arrival (`crates/rtsim-industry/src/engine.rs:109-119`,
`2076-2087`, read through `Engine::lines`, `479`). The plan reads those edges.

- **Geometry.** A line is the straight segment between its two stations' block positions: the same
  segment the leg index books trains on (A15, `crates/rtsim-industry/src/threat.rs:580`). The
  segment crosses a territory if it passes within the radius (plus `reach_blocks`, 0 here) of the
  camp, measured the short way on Deep Time's wrapped worlds, as `book_on` does (C31). Whether to use Create's real track instead is MZ-12.
- **Rate.** Legs per day on the station pair, both directions summed: trips ÷ the days between first
  and last arrival. A line goes quiet (rate 0) when no train has arrived for three times its mean gap
  between trips (PLACEHOLDER: how fast a rerouted line is forgotten as a source; measure in `sim
  mowzie`'s reroute run).
- **Source.** One continuous source per (station pair, Rail), keyed like a district's
  (`set_emission_in`, `engine.rs:1286-1312`). Its position is the point of the segment nearest the den;
  its chunks are the two stations' chunks, so it counts against the player who has spent the most time
  at those stations (P14), and against the settlers while nobody has.
- **Weight and tolerance** (PLACEHOLDER): weight 1.0 per leg, tolerance 2 legs a day (one round trip).
  They set how much traffic a Frostmaw stands. With the 6-day half-life (λ = ln 2 / 6 = 0.1155 a day)
  the grievance settles at 8.66 × (legs a day - 2). Ignoring dwell and temperament:

| Legs a day | Settles at | Notice (10) | Raid (70) | Invasion (140) | Siege (260) |
|---|---|---|---|---|---|
| 4 | 17.3 | day 7.5 | never | never | never |
| 12 | 86.6 | day 1.1 | day 14.3 | never | never |
| 24 | 190.4 | day 0.5 | day 4.0 | day 11.5 | never |
| 50 | 415.5 | day 0.2 | day 1.6 | day 3.6 | day 8.5 |

- **Cost.** The crossing test runs once per new line edge (and when a station moves), against the
  factions in the 1,024-block cells the segment passes (`cell_blocks`, `tunables.rs:116`); the per-step
  feed compares each crossing line's rate with the rate the engine keeps, as `feed_districts` does
  (`conflict.rs:645-660`), so a steady line costs no event.

The tribe gives Rail weight 0 (PLACEHOLDER, my choice): nothing in the jar ties Umvuthana to tracks.

### 4.4 Lying in wait: the Frostmaw against trains

**Unwatched.** A Frostmaw at its focus waits out its patience. For each leg-index window (one day,
`LEG_WINDOW_TICKS`, `crates/rtsim-industry/src/threat.rs:553`) it asks once for the train legs through its 512-block region
(`trains_crossed`, `crates/rtsim-ecology/src/infra.rs:65`), and each leg rolls `ambush_milli` (500 in
1,000). A hit is an I24 attack on that train: source `Boss`, its class, strength 41.7 (§4.10). The
industry engine paces it: one attack per train per class per day (`cooldown_ticks` 24,000,
`crates/rtsim-industry/src/tunables.rs:184`). The cargo lost is at most the Frostmaw's carry,
PLACEHOLDER 64 (industry's own siege carry, `tunables.rs:180`), and at most 10% of the leg's cargo
(`theft_cap_ppm` 100,000, `181`). It is destroyed, not carried off (ledger 7; MZ-5). The loss reaches
the real carriage as `IND_CARRIAGE_DELTA` wherever the train is (`docs/industry-boundary.md:216`,
`263-265`).

So two trains passing a waiting Frostmaw lose at most 64 items each per day: at most 256 items over a
two-day wait. Track is never cut (I24).

**Watched.** A player within view (`view_radius` 160, `tunables.rs:91`) of a waiting Frostmaw sees the
real one. The plugin materializes it at its place with `alwaysActive` set, so it stays awake
(`EntityFrostmaw.java:361-363`, `580`), restricted to its spot so its `MoveTowardsRestrictionGoal`
walks it there (`208`), its target the player.
- It targets players only (`284`), and chases its target (`520-558`).
- A train at top speed (28 blocks a second, Create's `trainTopSpeed`, 1.4 blocks a tick) passes the
  27 blocks of track inside its 13.5-block breath range in about a second. Its breath waits 260 ticks
  between uses and its ice ball 200 (`559-578`), so riders on a moving train take one attack at most;
  a stopped train, or a station, is where it hurts.
- A train that runs into it does real damage: Create's run-over damage is the relative speed in blocks a
  tick × 16, doubled for the `MONSTER` category, above 0.35 blocks a tick (`ContraptionCollider.handleDamageFromTrain`,
  javap), with `trainsCauseDamage` on (the default; no pack override). The Frostmaw is `MONSTER`
  (`EntityHandler.java:98`), so a train at 1.4 blocks a tick deals 44 a hit: six hits kill it. The
  damage source's entity is the carriage, not a living entity (`CreateDamageSources.runOver(level,
  contraption)`), so it wakes the Frostmaw but gives it no target (`EntityFrostmaw.java:886-903`).
- With MZ-5 (a), a watched ambush books the same cargo loss as an unwatched one (P28, extended), so
  watching does not change what a line loses.

**Its den while it is out.** Empty. A player who visits finds the ruin and no Frostmaw.

### 4.5 The Umvuthana: a tribe

**Who it is.** The grove's Umvuthana are the tribe's warriors (MZ-11). On the grove's first load the
plugin counts and adopts them, and the engine's pool starts at that count, which is also the faction's
`pool_cap` (5 to 9). The Umvuthi stays a vanilla entity at its fire pit (persistent, `EntityUmvuthi.java:393`),
so it keeps its own memory of who traded with it (`TRADED_PLAYERS`, NBT `"players"`, `779-792`).
Umvuthana the Umvuthi summons in a fight are adopted into the pool up to its cap; above it they stay
vanilla. Its cranes stay vanilla: a crane heals and shields only the Umvuthi
(`EntityUmvuthanaCrane.java:174`), so it has no place in a band away from it. The pool is a count of
fighters, and a raptor pack counts as one.

**Units** (strength by §4.10):

| Unit | Body | Class | Strength | Carry, breaks |
|---|---|---|---|---|
| `warrior` | `mowziesmobs:umvuthana` | ground_melee | 0.4 | 0, 0 |
| `raptor` | a pack: `mowziesmobs:umvuthana_raptor` and 4 `umvuthana_follower_raptor` (the smaller of the jar's 4 or 5), one unit (C25 groups) | ground_melee | 2.6 | 0, 0 |
| `envoy` | `mowziesmobs:umvuthana` with its target goals removed | ground_melee | 0.4 | 0, 0 |

Carry and breaks are 0 because no Umvuthana takes items from containers or breaks blocks in the jar.

**Parties** (PLACEHOLDER sizes, set so the smallest grove can field each: a war party is sent when the
pool holds at least half its make-up, `engine.rs:2237-2238`, and a grove starts with 5 to 9):
- envoy: 1 envoy and 2 warriors (goblin's pattern);
- scouts: 1 raptor pack;
- raid: 3 warriors and 1 raptor pack (4 units, strength 3.8);
- invasion: 6 warriors and 2 raptor packs (8 units, strength 7.6; a grove of 5 sends 5);
- siege: empty, so Invasion is the top rung: the tribe breaks no blocks, so it cannot besiege;
- plunder: 1 raptor pack, against village herds (MZ-8).

**What angers it** (goblin's weights, PLACEHOLDER): killing its kin inside the territory (any
`mowziesmobs:umvuthana*` entity, natural raptor packs included), logging, mining, building, smog.

**The ladder.**

| Rung | What happens |
|---|---|
| Notice (10) | `mowziesmobs:block.gong` plays to the player, as the Frostmaw's roar does. |
| Envoy (30) | An envoy walks to the player's base (P26). Its custom name shows the demand: at the Envoy threshold, 0.5 × 30 = 15 gold nuggets within 3 days (goblin's `tribute_per_grievance` and `demand_days`, middling temperament). Right-clicking it with the gold in hand pays (TRIBUTE_PAID); hitting it is an insult (ENVOY_ANSWER 3). A masked player's right-click opens the minion's own trade screen (`EntityUmvuthanaMinion.java:229-240`), so the bridge takes the payment first. |
| Scouts (50) | A raptor pack watches the base from the edge for `linger_days`. |
| Raid (70) | A war band comes for the player's base. |
| Invasion (140) | The pool's whole strength comes. |

**Tribute** is gold (MZ-6): `gold_nugget`, the tribe's trading currency on screen (a masked player buys
clawed logs, cooked meat, feathers, sticks and campfires from the grove's warriors with gold nuggets,
and sells them melon, raw meat, beetroot, fish, acacia saplings and bones for nuggets, one offer at a
time, `EntityUmvuthanaMinion.java:107`; the player's item goes in, `InventoryUmvuthana.java:97-105`).
Paid tribute is stocked at the grove's haven (C24) and the plugin puts it into the grove's real chests
when the grove loads (a haven chest, ledger 10, as `LAIR_CHEST`): a player who later raids the grove
finds the gold they paid. Paying the Umvuthi its 7 gold blocks (`ConfigHandler.java:470-471`) is
reported as tribute at the camp (`TRIBUTE_PAID` with agent 0): it settles the payer's demand, or with
none open counts as a gift worth 567 nuggets. Trading with the grove's minions while masked softens
the trader's feud at a tenth of a gift's rate (`Conflict::traded`, `engine.rs:2045-2052`).

**Masks are honoured** (MZ-10): the jar's targeting skips a player wearing an Umvuthana mask
(`EntityUmvuthana.java:347-356`; the Umvuthi and minions also target the player who misbehaved,
`EntityUmvuthi.java:322-330`, `EntityUmvuthanaMinion.java:133-139`). A masked player at a watched raid is
not attacked; the band still fights the iron golems, which attack it. Masks drop from Umvuthana
(one in three, for example `loot_table/entities/umvuthana_rage.json`) and lie in grove chests.

**What a tribe raid does.**
- **Watched:** the real band arrives in waves on the edge of view (`spawn_edge_blocks` 112, quota 16,
  `tunables.rs:92-94`). Warriors attack unmasked players; raptor packs hunt the base's animals
  (jar-true), which flee Umvuthana within 6 blocks (`ServerEventHandler.java:239-244`). Villagers flee
  them too (`246-250`).
- **Unwatched:** Lanchester against the district's defences, class `ground_melee`: walls stop it
  (I22). A raid that wins steals nothing and breaks nothing (carry and breaks 0), but its raptor packs
  owe the district dead animals: when the district next loads, up to 2 animals per raptor pack
  (PLACEHOLDER; measure in a GameTest: animals a raptor pack kills in a watched raid of the same
  length) matching the jar's hunt rule are killed where they stand, so their drops fall (MZ-8, ledger 9).
- **Plunder of a village:** a raptor pack takes livestock goods (`leather`, `wool`) from the village's
  stock, up to `loot_share_milli` (150 in 1,000, goblin's), eaten: the village has less to sell. Breaking a plundering
  pack makes nearby villages think better of the player (rescue reports within `rescue_news_blocks`
  2,000, `tunables.rs:108`).

### 4.6 Killing a boss at home

The Umvuthi dies at its fire pit, or the Frostmaw in its den. The plugin sees the death
(`LivingDeathEvent`) and reports a chief killed at the camp with the killer (ledger 3, 10). The engine
runs what `chief_killed` runs for a party's chief: the heir succeeds, the faction is cowed for
`cowed_days` (6, scaled by the chief's boldness), every feud drops two rungs, and any party out routs
home. Loot is the jar's own: the Umvuthi drops `sol_visage` when a player kills it
(`loot_table/entities/umvuthi.json`); the Frostmaw drops `ice_crystal` if a player kills it while it
holds the crystal (`loot_table/entities/frostmaw.json`).

The Umvuthi is not adopted, so a new chief needs a body: when the cowed time ends, the plugin places a
new Umvuthi at the fire pit the next time the grove loads (finalized as a structure spawn and restricted
to the pit, as `UmvuthanaGrovePieces.java:276-288` does).

### 4.7 On screen: real entities with the engine's state

| Body | Made by | State the engine gives it | State it gives back |
|---|---|---|---|
| Frostmaw | its constructor; NBT `has_crystal`, `active`, `alwaysActive` set before its first tick | health (0 full), crystal, asleep at home or awake away | health, crystal (on dematerialize and as a latest-wins chief state), death with killer |
| Umvuthana warrior, envoy | its constructor, then `finalizeSpawn` so it gets its trade table (`EntityUmvuthanaMinion.java:243-250`) | mask (NBT `"mask"`), weapon (NBT `"weapon"`, `EntityUmvuthana.java:682-690`) | casualties with killer |
| Raptor pack | the raptor's constructor, then 4 followers through the public `(type, level, leader)` constructor (`EntityUmvuthanaFollowerToRaptor.java:33`), not `finalizeSpawn` (which adds 4 or 5 of its own, `EntityUmvuthanaRaptor.java:191-199`) | none | casualties |
| Umvuthi (a new chief) | its constructor, `finalizeSpawn` as a structure spawn, `restrictTo` the fire pit (`UmvuthanaGrovePieces.java:276-288`) | none: it stays vanilla | death with killer |

Puppets are tagged and never saved by vanilla; a copy that reloads from an old chunk is discarded, as
dragons do (`docs/boundary.md` §6.5). Steering moves the restriction centre for the bodies that walk
to it, the Frostmaw (`EntityFrostmaw.java:208`) and the warrior (`EntityUmvuthanaMinion.java:127`); the raptor has no such
goal, so the bridge gives it navigation orders and its followers keep to it. Body kinds are registered
in code (P15 as built).

### 4.8 Effects through modules that exist

| On | Frostmaw | Umvuthana |
|---|---|---|
| Trains | Ambush while lying in wait: cargo destroyed, bounded (§4.4). Riders attacked when watched. | None: bands on the march may cross a leg, but carry 0 takes nothing. |
| Stations | Invasion and Siege camp at the nearer station: every leg from or to it rolls. | None beyond the district. |
| Districts | A Raid that finds a district within 64 blocks of the focus raids it: Lanchester, class by crystal; no blocks broken, nothing stolen beyond the cargo rule. | Lanchester; walls stop it; raptors owe dead animals on load. |
| Villages | None: dens are 7 chunks or more from villages, and it plunders nothing. Villagers flee it at 10 blocks (`ServerEventHandler.java:250`). | Raptor plunder of livestock goods; villagers flee Umvuthana at 6 blocks (`247`). |
| Ships | None: its biomes exclude ocean, river and beach. A watched fight by a river port ices water for 140 ticks. | None. |
| Players | Watched: attacks players only. Unwatched raids target the player's home district (P26) only through the ladder's focus. | Unmasked players, at their base (P26). |

### 4.9 Relations

- **With each other:** none. Snow and savanna do not meet; neither culture hates the other.
- **Iron golems** attack every body in this plan: vanilla's golem targets any `Enemy` but a creeper
  (`IronGolem.java:78`), and the Frostmaw (`EntityFrostmaw.java:152`), the Umvuthi (`EntityUmvuthi.java:209`),
  the warrior and crane (`EntityUmvuthanaMinion.java:105`), the raptor (`EntityUmvuthanaRaptor.java:71`) and
  its followers (`EntityUmvuthanaFollowerToRaptor.java:28`) all implement it. The Umvuthi targets golems
  back (`EntityUmvuthi.java:318-345`). Golems are I21 guards against both factions.
- **Zombies and skeletons** fight Umvuthana both ways (`ServerEventHandler.java:227-236`; the
  warrior's target goals, `EntityUmvuthanaMinion.java:131-151`).
- **Farm animals** flee Umvuthana and the Umvuthi within 6 blocks and the Frostmaw within 10
  (`ServerEventHandler.java:239-244`).
- **Elokosa** (jungle) are the tribe's enemies on screen; not a faction (§4.12).
- **Carry On** (in the pack, `pack/mods/carry-on.pw.toml`): an Umvuthana picked up leaves the sim (A10).

### 4.10 Strength

The engine's strength unit is one ordinary mob: an observed fight counts one per attacker today
(`core/industry/Threats.java:384-385`). The plan's rule (TUNABLE): health × attack ÷ a zombie's (20 × 3).

| Body | Health × attack | Strength |
|---|---|---|
| Frostmaw | 250 × 10 (`EntityFrostmaw.java:291`) | 41.7 |
| Umvuthi | 150 × 2 (`EntityUmvuthi.java:361`) | 5.0 (understated: its damage is its sun abilities) |
| Umvuthana warrior, crane, follower | 8 × 3 (`EntityUmvuthana.java:451-452`; one attribute set, `EntityHandler.java:244`, `247-248`) | 0.4 |
| Raptor | 10 × 6 (`EntityUmvuthanaRaptor.java:97-98`) | 1.0; a pack of 1 and 4 followers 2.6 |

Measured in GameTests (§7) against reference bases through `IND_FIGHT`, with the strength hook (ledger
14). L2 Hostility may scale these mobs' health; UNVERIFIED.

### 4.11 Tunables and placeholders

| Value | Here | Source | What it tunes | How to measure |
|---|---|---|---|---|
| Territory radius | 200 | half the structure spacing | how far a camp's range reaches | GameTest census of den spacing in a generated world |
| Rail weight, tolerance | 1.0, 2 legs a day | PLACEHOLDER | how much traffic a Frostmaw stands | `sim mowzie` sweep of legs a day against days to the first raid; Ben's call on the target |
| Line stale window | 3 mean gaps | PLACEHOLDER | how soon a rerouted line stops counting | `sim mowzie` reroute run |
| Frostmaw carry | 64 | industry's siege carry | cargo lost per ambush | Ben's call (MZ-5) |
| Recruits | Frostmaw 1/6 a day; tribe 2 a day | 1 / cowed days; goblin's | heir arrival; how fast a beaten grove refills | `sim mowzie`: days from an emptied pool to a full one; Ben's call on the target |
| Thresholds, dwell, half-life, patience, muster, cowed, tribute per grievance (0.5) | goblin's | PLACEHOLDER | pace of the ladder | `sim mowzie` |
| Party sizes | §4.5 | PLACEHOLDER: a grove of 5 can field each | raid strength | GameTest fights |
| Strength rule | health × attack ÷ 60 | TUNABLE | Lanchester inputs | GameTest fights through `IND_FIGHT` |
| Animals owed per raptor pack | 2 | PLACEHOLDER | herd loss to a won raid | GameTest: animals killed in a watched raid |
| Notice distance | 32 blocks | PLACEHOLDER | where the roar or gong comes from | listen in game |

### 4.12 Left out, and why

| Mob | Why | What would bring it in |
|---|---|---|
| Ferrous Wroughtnaut | Its chamber is disabled in this pack (`structurify.json:405-411`, after a crash). Immune from the front. | MZ-9: re-enable the chamber once its crash is understood; then a stationary home boss like the Umvuthi. |
| Sculptor | A trial, not a ruler: it only fights what hurts it (`EntitySculptor.java:355`). | Nothing planned. |
| Naga | Lives only on `stony_shore` in this pack, within 50 blocks of where it spawned (`EntityNaga.java:175`, `582`), and has no camp. | A coastal hazard, with the ships work. |
| Elokosa | No camp; one at a time in jungles. | A jungle faction if one is ever wanted. |
| Foliaath, Lantern, Grottol | Ordinary mobs; the Grottol only seeks vanilla minecarts. | Nothing planned. |
| Bluff | No natural spawn by default: its biome list is empty (`ConfigHandler.java:401`), and no structure places it (only its spawn egg makes one). | Nothing planned. |

## 5. Walkthroughs

**A. A rail line through a Frostmaw's range.**
1. Iva mines copper in the snowy slopes. She lays Create track 1,800 blocks south to her smelter town,
   with a Create station at each end. The straight line between the stations passes 150 blocks from a
   Frostmaw den she has never seen. The den's chunk loaded once when she scouted the route, so the
   plugin found it and adopted its sleeping Frostmaw.
2. She runs one train shuttling between the two stations: a leg is about a minute at top speed
   (1,800 ÷ 28 = 64 seconds) plus loading, and she schedules 12 legs a Minecraft day. The first
   arrival creates the line edge; the feed finds the line crossing the den's circle and sets a Rail
   source against Iva, who spends the most time at those stations.
3. On day 2, riding through, she hears a roar from the north: the notice (the grievance passed 10 at
   about day 1.1, §4.3). Nothing else happens.
4. She adds a second train: 24 legs a day. Within four days the grievance crosses 70. The Frostmaw
   walks out of its den (150 blocks at the culture's march of 0.02 blocks a tick: 7,500 ticks, a third
   of a day) and stands on the line at the point nearest its den.
5. She is at her smelter, 1,500 blocks away. Each leg past the waiting Frostmaw rolls; a train loses at
   most 64 ingots a day, and at most 10% of what it carries, destroyed. Over the two-day wait her two
   trains lose at most 256 ingots. No track is cut.
6. She rides the next train north. The Frostmaw is standing on the track. The train hits it at speed:
   44 damage, and it turns, with no one to blame. As the carriages pass it gets one ice ball off at
   her; a second later she is past its range.
7. Her choices:
   - **Kill it.** She brings a TaCZ rifle; her bow does nothing to it. It falls after 250 damage;
     she gets an ice crystal (it still had its crystal and a player killed it). The faction is cowed for
     6 days, the feud drops two rungs. Six days later an heir moves into the den, holding half its predecessor's grudge against
     her.
   - **Steal its crystal first.** She drinks a potion of invisibility and walks into the den while it
     sleeps (a sleeping Frostmaw drops a target that has invisibility, `EntityFrostmaw.java:597-599`). She stands by the crystal in its fist: it is hers, and the Frostmaw wakes without it.
     From now on it has no breath or ice balls, it counts as melee, and a wall at her station stops it
     while she is away.
   - **Reroute.** She builds a halt station west of the range and adds it to the schedule, so neither
     leg's straight line enters the circle. The old pair goes quiet after three of its gaps between
     trips; the grievance halves every six days.
   - **Defend the station.** It will come to the nearer station when the grievance reaches 140. A
     Create Big Cannons autocannon on a Create Radars controller with auto-target and auto-fire counts
     against every class (I21); a 2-block wall counts only once the crystal is gone; an iron golem
     counts as a guard.
   - **Pay.** The Frostmaw takes no tribute. Running the trains and accepting the losses is the
     price.

**B. A savanna base near a grove.**
1. Tomas builds a windmill and a cattle ranch 150 blocks from an Umvuthana grove, inside its
   200-block range. The grove loaded when he arrived: the plugin adopted its 7 warriors, so the
   tribe's pool is 7.
2. He clears acacia for his ranch (logging) and kills four masked warriors who chased him (killing
   weighs 8 each: 32). That evening he hears a gong.
3. Half a day later an envoy with two guards walks into his ranch. Its name reads "16 gold nuggets
   within 3 days" (0.5 a point of grievance). He right-clicks it with the nuggets: the demand is
   settled, 70% of the grievance goes (`tribute_relief`), and the tribe cannot climb for 6 days
   (`respite_days`). When he next visits the grove, his nuggets are in its chests.
4. He keeps logging. Weeks later the grievance passes 70 and a war band comes: three warriors and a
   raptor pack. He is home, so the band arrives in waves at the edge of view. The raptors go for his
   cattle, the warriors for him, and his iron golem for all of them.
5. He goes on a trip. The next raid comes while he is away. His ranch has a fence all the way round,
   so the ground band is stopped at it (I22): nothing happens. Had the fence a gap and the band won,
   the raptors would owe him dead cows, and he would find them dead when he got back.
6. He finds a Fury mask on a dead warrior and wears it. The tribe's bodies no longer target him; only
   a warrior he has provoked still does. He sells bones to the grove's warriors for nuggets: the
   tribe's grudge softens a little with each trade.
7. Or he ends it: he walks into the grove unmasked and fights the Umvuthi at its fire pit. It summons
   warriors while fewer than 4 are near it, and cranes each time it loses 45 health; while a crane
   shields it with Sunblock his shots do nothing, so he shoots the cranes first. When it dies the tribe
   is cowed for six days and its bands run home. He takes the Sol Visage. When the cowed time is over,
   a new Umvuthi sits at the pit the next time the grove loads.

**C. A village herd.**
1. A plains village 900 blocks from the grove trades wool and leather. The tribe plunders on its own
   schedule (`plunder_mean_days` 12, goblin's): a raptor pack takes the village's leather and wool.
2. The village has less wool and leather to sell. Tomas, passing through, sees the pack and breaks
   it: villages within 2,000 blocks hear of the rescue and think better of him.

## 6. Cost model

| Mechanic | When it costs | Cost |
|---|---|---|
| Idle factions | per tick | zero: grievance is closed form, one booked event per feud |
| A faction | per faction-day while its page is read | 52.3 µs at 1,024 factions, 57.3 µs at 4,096 (`docs/choices-paging-modules.md:292`, PM-C12) |
| A faction | resident | about 580 bytes (PM-C13, `:293`), plus about 125 bytes of indexes (PM-C4, `:284`) |
| Rail crossing test | per new line edge or moved station | the factions in the 1,024-block cells the segment passes; cached per line |
| Rail feed | per step | one comparison per crossing line, from the resident source index; no page read |
| Lying in wait | per waiting party per leg window (one day) | one `trains_crossed` lookup for its region, then one roll per leg found |
| Train ambush | per hit | one `attack_train`, as today's bandit ambushes |
| Camp finding | per chunk load | one check of the chunk's structure starts (`Lairs.java:114-125`) |
| Watched boss | while seen | one Frostmaw, or a band in waves of at most 16 |
| Animals owed | per district load with a debt | one entity scan of the district's loaded chunks, capped |
| Density | per world | at most 6.25 dens and 6.25 groves per 1,000 × 1,000 blocks; 4,096 factions (the paged sweep's top, `docs/choices-paging-modules.md:23`) of one kind at that density cover 655 million square blocks, a square about 25,600 blocks on a side |
| Nobody near any camp, no line crossing, nothing changing | | zero |

## 7. Tests before it ships

**GameTests** (`neoforge/mowziesmobs/.../gametest/MowzieGameTests.java`; names in the dragons' style,
`DragonGameTests.java`, 688 lines). A real structure start cannot be placed in a GameTest (`/place`
registers no start, `docs/overnight-choices-iceandfire.md:140-143`), so tests declare their camps.
1. `aFrostmawMaterializesAsleepWithItsCrystalAndHealth`, and its state captures back.
2. `anArrowWakesAFrostmawWithoutDamageAndABulletHurtsIt` (TaCZ's bullet in the test, if TaCZ is in
   the test run; otherwise a `Projectile` stand-in).
3. `anInvisiblePlayerStealsTheCrystalFromASleepingFrostmaw` (a mock player).
4. `runOverDamageWakesAFrostmawWithoutATarget`: Create's `create:run_over` damage type with a
   non-living source entity. A moving train is not tested: no GameTest runs an assembled train on a
   schedule (`neoforge/README.md:262`).
5. `aLurkingFrostmawStaysAwakeAndWalksToItsSpot`.
6. `aMaskedPlayerIsNotTargetedByTheBand`.
7. `theUmvuthisTradeIsReportedAsTributeAtTheCamp`.
8. `anEnvoyIsPeacefulTakesPaymentAndCountsAHitAsAnInsult`.
9. `aChiefKilledAtTheCampIsReportedWithTheKiller`, for both bosses.
10. `aGroveIsAdoptedWithItsCountAndASummonedWarriorJoinsThePool`.
11. `tributeLandsInAGroveChestWhenTheGroveLoads`.
12. `raptorsHuntTheBasesAnimalsAndAnOwedHuntIsAppliedOnLoad`.
13. Strength fights: each body against reference bases through `IND_FIGHT`, for §4.10.

**`rtsim-cli sim mowzie`** (a scenario beside `sim conflict`, `main.rs:81-85`), with runs:
1. A line through a range: notice, raid, ambushes bounded per train per day, a kill, cowed, the heir.
2. Reroute: the old pair goes quiet and the grievance decays.
3. The tribe: envoy paid; envoy refused, then a raid and an invasion; plunder of a village.
4. Save and restore in the middle of a lurk and of a cowed time, with identical results.

**Bench:** `rtsim-cli bench scale --only factions,factions-paged` (`crates/rtsim-cli/src/main.rs:25`)
before and after.

## 8. Proposals for Ben

| Id | Question | Recommended | Alternatives |
|---|---|---|---|
| MZ-1 | Where the conflict module's Java half lives (none exists) | a vanilla `ConflictBridge` in the core owns the conflict records; the Mowzie plugin registers bodies, finds camps, adopts | (b) the Mowzie plugin owns the conflict range itself (the first plugin to register owns a kind, `SessionPlugins.java:23-29`, so a later goblin plugin would collide) |
| MZ-2 | What angers a Frostmaw | Rail from the industry's line edges, plus killing and building in its range | (b) killing and building only: trains through its range never matter; (c) Rail for every faction |
| MZ-3 | Which bosses leave home | the Frostmaw only; the Umvuthi is fought at its grove | (b) the Umvuthi also leads the Invasion in person (it then needs adopting, and its trade memory moves into the engine); (c) neither |
| MZ-4 | After a boss dies | an heir arrives when the pool refills (6 days, PLACEHOLDER) | (b) never: a cleared range stays clear; (c) at once |
| MZ-5 | What a Frostmaw ambush costs a train | cargo destroyed, at most 64 a train a day and 10% of a leg; watched or not | (b) riders only: the jar's Frostmaw never touches cargo, so unwatched trains lose nothing; (c) a delay |
| MZ-6 | The tribe's tribute | gold nuggets, a new phase 1 good; the Umvuthi's 7 gold blocks count as 567 | (b) emeralds, as goblins (no new good); (c) a gold-nugget market for the grove (a third phase 1 culture) |
| MZ-7 | The README's "remove items and progression from boss mods" | keep Mowzie's drops and its masks, trade and crystal: this plan's answers rest on them | (b) remove them: masks and the Umvuthi's trade go, tribute is the only peace |
| MZ-8 | Raptor hunts | won raids owe a district dead animals on load; village plunder takes livestock goods, eaten | (b) none: tribe raids do nothing unwatched |
| MZ-9 | The Wroughtnaut | left out while its chamber is disabled | (b) find the crash, re-enable, add it as a home boss |
| MZ-10 | Masks | honoured, as in the jar | (b) a tribe at war targets masked players too |
| MZ-11 | The grove's people | adopted: the pool is the real warriors, so killing them at the grove shrinks raids | (b) left vanilla: the pool is separate and the grove looks the same whatever happens |
| MZ-12 | Line geometry | the straight segment between stations, as legs are booked (A15); a halt station reroutes | (b) Create's track graph (the Create plugin reads `trackNetworks` already, `neoforge/create/.../WritePackages.java:162`), so curves count: more exact, more work, and the leg index stays straight |
