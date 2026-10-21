# Mowzie's Mobs: factions and boss encounters

**Status:** design for Ben's review, revised 2026-10-21 after an adversarial review that was checked
against the jars. Nothing is built. Ben approves this design before code.

**Decided, and binding here:**
- **Ben's request:** "go ahead and plan out some support for Mowzie's mobs, creating factions for the
  mobs in them, using bosses from them for encounters / raids. (I.E. Yeti's which can spawn in snow
  biomes not being big fans of train networks going through their territory.)" The "yeti" is the
  Frostmaw: the only Mowzie's mob whose home is a snow biome.
- **Ben on rail geometry (2026-10-21):** "Connections between train lines are just straight lines
  instead of looking like actual trains or following the map." So a range feels the real track
  the trains run on, never the straight chord between two stations (§4.3, MZ-12).
- **Ben on renewal (2026-10-21):** "If the player kills off all of the Yeti's that currently exist,
  having a way for new ones to spawn. Same with dragons, villages, etc.", and a "nash equilibrium
  between tribes / animals". §4.10 states how these two factions renew and what they need from the
  pan-species design.
- **The pack's own TODO** (`pack/README.md:17-26`): "Boss raid system", "Natural Boss Spawning",
  "Remove items & progression from boss mods: ... mowzies mobs", "Mowzies mobs: Natty spawns". The
  third conflicts with parts of this plan (MZ-4).
- **UR-1** (`DECISIONS.md:272`): rtsim can be removed and reinstalled at any time; a reset may lose
  only the sim's own state. §4.9 is how this plan keeps it.
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
are the jar's `data/mowziesmobs/`. Create 6.0.10 was decompiled with CFR (`Train`, `Carriage`,
`TravellingPoint`, `GlobalRailwayManager`, `AbstractContraptionEntity`, `ContraptionCollider`); TaCZ
1.1.8-hotfix-r6, Create Big Cannons and Create Radars were read for the review. Pack data:
`pack/config`, `pack/defaultconfigs`, `pack/kubejs`; no pack file overrides Mowzie's config, so its
defaults hold. Vanilla: the decompiled 1.21.1 sources in the Gradle cache. mic-rtsim: this
repository at main `0de20b6`; `core/` means
`neoforge/core/src/main/java/com/minecraftindustrialcomplex/mic_rtsim/`. Nothing here was run in a
game: "verified" means read in code or data.

**Numbers:** every number is a verified fact, arithmetic from one, or a labelled TUNABLE or
PLACEHOLDER. Every PLACEHOLDER says what it tunes and how to measure it (§4.14). Anything
unverified says UNVERIFIED, and the test that settles it is named in §7.

---

## 0. What the player sees

The snowy peaks and slopes hide Frostmaw dens: an ice ruin with a huge white ape asleep in it,
holding a blue crystal in its fist. Today it never leaves. With this plan it minds its range. Run a
Create railway through a Frostmaw's range and, the more trains you run, the angrier it gets. First
you hear it roar as you ride through. Then it leaves the den and waits beside your track, a few blocks
off the rail. Trains that pass it while you are away lose cargo, and the next time a robbed train
pulls into a station near you, you hear the roar again. Ride past it yourself and it swipes at the
carriages as they go by, and throws an ice ball ahead of your train: if it hits you, you freeze, you
are pulled off your seat, and you stand frozen in the snow beside an awake Frostmaw while your train
leaves without you. Push it further and it comes back every few days, then camps by your nearest
station. Run fewer trains and it calms down over days; lay your track around its range and it
forgets the line.

You can kill it. Arrows and tridents bounce off it; swords and axes, fire, TaCZ bullets and Create Big
Cannons shells all hurt it, and a cannon shell has no attacker it can turn on. You can sneak up while
it sleeps, invisible, and steal its crystal: then it has no ice attacks and no ice balls to throw at
your riders, and your walls hold it. Kill it and the range is quiet for a while, until another
Frostmaw moves into the empty den, carrying its predecessor's grudge. On Peaceful, nothing comes.

In the savannas, Umvuthana groves are villages of masked warriors around a fire pit, ruled by the
Umvuthi, a sun-priest boss. Today they attack anyone unmasked who comes close. With this plan the
grove is a tribe. Log its acacias, mine its hills, kill its people, and it notices: a gong sounds in
the distance. Then a masked envoy and two guards walk to your base, ignoring your iron golem and you,
and ask for gold. Pay, and the gold ends up in the grove's chests. Refuse, or strike them, and
raptor-masked hunters come for your herds and the villages' herds, then a war band of masked
warriors. Wear an Umvuthana mask and the tribe leaves you alone, as it does today. Kill the Umvuthi at
its fire pit (its healers make it untouchable until you kill them) and the tribe breaks: its raiders
flee and it stays quiet for days. A grove you have thinned fills back up over a few days, with
warriors walking in from its edge.

## 1. Reuse table

| What happens | The real mechanic | Mod (verified id or class) |
|---|---|---|
| A Frostmaw's den | `mowziesmobs:frostmaw_spawn`, structure set `frostmaw_spawns`: random spread, spacing 25 chunks, separation 8, at least 7 chunks from `minecraft:villages`; the template (`structure/frostmaw_spawn.nbt`) holds one `mowziesmobs:frostmaw` with `has_crystal` 1 and no `PersistenceRequired` | mowziesmobs |
| The Frostmaw | `mowziesmobs:frostmaw` (`EntityFrostmaw`): 250 health, 10 attack, knockback resistance 1, 4 × 4 blocks (`EntityHandler.java:98`), swipe, slam, ice breath, ice ball | mowziesmobs |
| Its swipe at a passing carriage | the jar's `SWIPE_ANIMATION`, an `AnimationAreaAttackAI` of range 6.5 and arc 135 degrees (`EntityFrostmaw.java:212`), started by `AnimationHandler.INSTANCE.sendAnimationMessage(this, SWIPE_ANIMATION)` as the jar's own AI does (`567`) | mowziesmobs (llibrary `AnimationHandler`) |
| The ice ball that unseats a rider | `EntityIceBall`: 1.6 blocks a tick, aimed ahead of its target from the target's position one tick earlier (`EntityFrostmaw.java:484-501`); hits every living entity within 2 blocks of it for 3 freeze damage and 1.0 freeze progress (`entity/effects/EntityIceBall.java:60-64`); at 1.0, `FrozenData` applies `FROZEN` for 50 ticks (`capability/FrozenData.java:260-261`) and `onFreeze` seats the entity on a new `EntityFrozenController` with `startRiding(controller, true)` (`FrozenData.java:185-199`, called from `ServerEventHandler.java:330`); vanilla `startRiding` calls `stopRiding()` first, which takes the rider off the carriage | mowziesmobs, minecraft |
| Its prize | `mowziesmobs:ice_crystal`: dropped when a player kills it while it holds the crystal, or stolen while it sleeps | mowziesmobs |
| Its warning | sound `mowziesmobs:frostmaw.roar` | mowziesmobs |
| A tribe's home | `mowziesmobs:umvuthana_grove`, structure set `umvuthana_groves` (spacing 25, separation 8, 7 chunks from villages): the Umvuthi at the fire pit, 5 to 9 Umvuthana, chests with `mowziesmobs:chests/umvuthana_grove_chest` loot | mowziesmobs |
| Warriors and traders | `mowziesmobs:umvuthana` (`EntityUmvuthanaMinion`): 8 health, 3 attack, a mask, always persistent (`253`), trades gold nuggets with masked players | mowziesmobs |
| The chief's healers | `mowziesmobs:umvuthana_crane`: the Umvuthi summons them each time it loses 45 health; they heal only the Umvuthi and give it Sunblock, under which it takes no damage | mowziesmobs |
| Hunters | `mowziesmobs:umvuthana_raptor` and `umvuthana_follower_raptor`: a raptor leads 4 or 5 followers; the raptor hunts animals and its followers take its target | mowziesmobs |
| The chief | `mowziesmobs:umvuthi` (`EntityUmvuthi`): 150 health, sun abilities, summons Umvuthana; takes 7 gold blocks once from a masked player, then grants Sun's Blessing | mowziesmobs |
| Peace with the tribe | an Umvuthana mask on the head (`mowziesmobs:umvuthana_mask_*`): Umvuthana, raptors and the Umvuthi do not target the wearer | mowziesmobs |
| The tribe's warning | sound `mowziesmobs:block.gong` | mowziesmobs |
| Factions, grievance, the ladder, envoys, raids, routs, heirs | rtsim-conflict (`crates/rtsim-conflict`) | mic_rtsim |
| Where trains really run | Create simulates every train wherever it is, loaded or not (`GlobalRailwayManager.tick`, `trains` map, lines 59 and 210); a train's position is `Carriage.getLeadingPoint().getPosition(graph)` (`Carriage.java:385`, `TravellingPoint.java:302`), its dimension the leading point's node location (`TrackNodeLocation.dimension`, `:41`) | create |
| Whose train it is | `Train.owner`, a UUID (`Train.java:162`); who sets it in the hash-matched jar is UNVERIFIED (MZ-13) | create |
| Cargo lost from a train | I24: `attack_train` (`crates/rtsim-industry/src/engine.rs:2914`), which finds the train's current booked leg by its id (`LegIndex::leg_of`, `threat.rs:689`), and `IND_CARRIAGE_DELTA` on `Carriage.storage` | mic_rtsim_create |
| Train arrivals and lines | industry line edges from Create train arrivals (`TrainWatch.java`; `crates/rtsim-industry/src/engine.rs:109-119`, `2076-2087`) | mic_rtsim_create |
| Defending a station or base | the I21 survey: walls 2 blocks or more, fences and walls; Create Radars fire control with Create Big Cannons (Radars class the Frostmaw, hostile Umvuthana and the Umvuthi as hostile); Create saws, drills, crushing wheels; iron golems, which attack every Mowzie body in this plan (`INDUSTRY.md:574-585`) | create, createbigcannons, create radars, minecraft |
| Weapons that hurt a Frostmaw | melee, fire, TaCZ guns (`EntityKineticBullet` extends `Projectile`, not `AbstractArrow`), Create Big Cannons shells (also `Projectile`, with no attacker entity); a thrown trident is an `AbstractArrow` (`ThrownTrident`), so it bounces off like an arrow | tacz, createbigcannons, minecraft |
| Stealing the crystal | vanilla invisibility (`minecraft:potion` of invisibility) | minecraft |
| Keeping a robbed Frostmaw | vanilla `Mob.setPersistenceRequired()`, saved as `PersistenceRequired` | minecraft |
| Puppets: bodies the sim owns while they are away | `BodyAdapter` (`core/body/BodyAdapter.java:16`) and `BodyKinds.registerBody` (`core/body/BodyKinds.java:41`), as dragons and herds use them | mic_rtsim |
| Keeping golems off an envoy | NeoForge `LivingChangeTargetEvent`, which the core already listens to (`core/industry/ThreatEvents.java:32`, `56`) | neoforge |
| Camps found when they load | the lair finder pattern: a structure start seen on `ChunkEvent.Load` (`neoforge/iceandfire/.../Lairs.java:44-60`, `114-125`) | mic_rtsim_iceandfire (pattern) |
| Tribute put in a real chest | `TRANSFER_TO_CONTAINER` 0x8005 (`docs/boundary.md:286`), as the dragons' `LAIR_CHEST` uses it (`docs/boundary.md:850`, `880-908`) | mic_rtsim |
| A rail fixture for tests | `RailFixtureGameTests` (branch `test/create-rail-fixture`, not merged): real Create 6.0.10 track with a 90 degree curve and an S-bend, two stations, a signal and an assembled two-carriage train on a schedule | mic_rtsim_create |

## 2. Custom-roll ledger

Sizes are estimates by analogy with code that exists; they are not measured.

**Engine (Rust: rtsim-conflict, rtsim-settle and the composition), about 1,500 lines plus tests:**
1. **Rail from sampled trains** (§4.3). `SignalKind::Rail` = 6, appended. The composition decodes
   RAIL_PASS, holds a pass until the train's next arrival names its line, and keeps a decayed leg
   count per (line, faction) with a hysteresis band, pushing a rate only when it leaves the band and
   booking one wake for its next downward push. A new `set_emission_for(faction, key, kind, rate,
   pos, actor)` pins a source to one faction (today `set_emission_inner` finds factions from the
   source's one position, `engine.rs:1337-1424`, so a source can reach only the dens around that
   point). A source with no district keeps a position focus (today `refresh_rates` keeps a focus
   only with a district, `engine.rs:1513-1517`). About 260 lines.
2. **Empty parties and skipped rungs.** A culture may leave a rung's party empty (today
   `Culture::resolve` refuses one, `culture.rs:378-382`). A climb skips empty rungs (Notice goes
   straight to Raid after Notice's dwell), and a step down that lands on an empty rung goes on to the
   next rung below that has a party, floored at Notice. About 60.
3. **Repeat:** a culture field `repeat_days`: a feud sitting on Raid or Invasion with no party out,
   and still at or above that rung's threshold, sends that rung's party again `repeat_days` after
   the last one came home (today only Siege repeats, every `siege_interval_days`,
   `engine.rs:2776`, `2814`). Absent means today's behaviour. About 40.
4. **Chiefs that stay home:** a culture field naming the party kinds the chief rides with (today any
   war party, `engine.rs:2265-2271`). About 40.
5. **A chief killed at the camp** (today `chief_killed` takes a party, `engine.rs:4124-4140`): the
   same report, death, succession and cowing as a party chief's, every party out routs home, and the
   **killer's** feud steps down `chief_rout_steps`, floored at Notice; with no killer, or a killer
   who has no feud, no feud steps down (§4.2). About 80.
6. **A culture of one:** the pool refills only while the pool plus the members away is under the
   cap, so a den never fields a second Frostmaw while the first is out (today the pool refills from
   the moment a party leaves, `engine.rs:1053-1066`). About 20.
7. **A pool cap per camp, and the census** (§4.9). `pool_cap` is per culture today (`engine.rs:882`,
   `1062`); a camp's cap becomes the count of its bodies at FACTION_FOUND (a grove's 5 to 9), bounded
   by the culture's. HOME_CENSUS on each camp load reconciles the engine's home count with the
   vanilla bodies there, and HOME_SPAWN sends the deficit. About 140.
8. **Lying in wait** (§4.4): a war party at a position with no district waits out its patience
   (today it "finds nothing to raid", `engine.rs:2869-2878`), sends AMBUSH_SPOT, and rolls
   `ambush_milli` once per TRAIN_PASSED; a hit calls `attack_train` for that train id. The spot's
   passes come from sampling, never from `trains_crossed`'s 512-block regions (the real track can sit
   in a different region from the straight booked leg). About 140.
9. **Threat source and class from the culture:** ambushes and district raids use the culture's source
   (`Boss`) and each unit's class and carry (today both hard-code `Bandit`, and ambushes
   `GroundMelee`, `engine.rs:2663-2664`, `2947`). A chief state bit picks between two unit rows (the
   Frostmaw with or without its crystal). About 70.
10. **Loot that is destroyed, not carried home:** a culture flag; the goods leave the books as the
    dragons' eaten food does. About 40.
11. **Hunted animals** (decided, §4.5): a raptor raid that beats a district's defences owes the
    district dead animals, applied when it next loads, under a cap (like I23's destroyed blocks);
    plunder of a village takes its livestock goods, eaten. About 120.
12. **Tribute priced by value:** `tribute_per_grievance` stays in the market's money (emeralds, 0.5
    a point, goblin's) and a demand converts it to the culture's tribute good at the haven market's
    price, with a fixed fallback rate until it has one (§4.5). With MZ-6 (a), a `gold_nugget` good in
    rtsim-world's catalogue and the phase 1 item map (`crates/rtsim-jni/src/engine/phase1/catalog.rs:28-38`).
    About 70.
13. **A den without a market:** a culture flag `haven: false` joins the camp as a market-less world
    node (a new `NodeKind::Lair` in `crates/rtsim-settle/src/lib.rs:48`, no stock or market rows),
    because `add_faction` makes every camp a `NodeKind::Haven(market)` node today
    (`engine.rs:854-861`), and a Frostmaw trades nothing. About 60 with the match arms.
14. **Reports the engine has no record for** (§4.12): CAMP_DEATH (a body or the chief killed at
    home, with the killer and an op id), CHIEF_STATE (latest-wins: health, crystal), TRADED (a
    masked trade at the grove, into `Conflict::traded`, `engine.rs:2045`, which has no caller today),
    a haven's chest (latest-wins, as `LAIR_CHEST`), NOTICE_SHOWN (a notice is kept per feud until the
    plugin has played it, and resent at each session start), TRAIN_ROBBED (the train the robbed-train
    roar plays for), and the world's difficulty (no war party leaves on Peaceful). About 240 with the
    decoding.
15. **Two cultures as data:** `data/cultures/frostmaw.json` and `umvuthana.json`, added to `builtin()`
    (`culture.rs:430-432`, `449`), each with `market_culture` (`Culture::resolve` errors without it,
    `culture.rs:383-387`), `chief_lifespan_days` 1,000,000 (§4.2) and the fields above. About 140
    lines of JSON.

**Java, about 2,700 lines plus about 1,300 of GameTests and kit:**
16. **ConflictBridge** in the core (MZ-1): the conflict module's Java half, which does not exist yet
    (`crates/rtsim-jni/src/engine/conflict.rs:5`): territories (kept in a spatial hash the train
    sampler also reads), the territory signals from vanilla block and death events, the envoy and its
    payment, the raid director (waves on an arc), steering, recalls, casualties, notices and their
    acknowledgement, the census plumbing, the Peaceful flag. Vanilla only (D4). About 1,100, by
    analogy with `core/herd/HerdBridge.java` (940).
17. **The train sampler** in the Create plugin (`neoforge/create`, `TrainSampler.java` beside
    `TrainWatch.java`): once a second it walks `GlobalRailwayManager.trains`, reads each train's
    leading point and tests the segment from its last sample against the territory hash; it reports
    RAIL_PASS and, against AMBUSH_SPOT, TRAIN_PASSED; near a lurking Frostmaw that a player watches it
    samples every tick; it plays the robbed-train roar (TRAIN_ROBBED). About 300.
18. **The Mowzie plugin** `neoforge/mowziesmobs` (mod id `mic_rtsim_mowziesmobs`): body adapters for
    the Frostmaw, the Umvuthana warrior, raptor pack and envoy; camp finding and the census for the two
    structures; turning a home body into a puppet when it leaves and back into a saved vanilla body
    when it returns (§4.9); persistence for a robbed Frostmaw; the swipe at passing carriages; the
    envoy party's targeting; a mixin on `EntityUmvuthi.rememberTrade` and an event on minion trades;
    the notice sounds. About 1,100 to 1,300, by analogy with the Ice and Fire plugin
    (`DragonBridge.java` 1,029, `Lairs.java` 409).
19. **A strength hook for observed fights:** `IND_FIGHT` counts one per attacker today
    (`core/industry/Threats.java:384-385`); plugins supply a per-entity strength (§4.13). About 40.
20. **One core exclusion tag**, `mic_rtsim:threat/ignored`, checked first in `ThreatClasses.hostile`
    (`core/industry/ThreatClasses.java:36-39`), with `mowziesmobs:foliaath` as an optional entry
    (§4.15). About 10 lines and one JSON file.
21. **GameTests and a kit** (`MowzieGameTests`, `MowzieKit`), about 1,000 and 300, by analogy with
    `DragonGameTests.java` (688) and `DragonKit` (329); the rider and swipe tests build on the rail
    fixture; and **`rtsim-cli sim mowzie`**, about 550, by analogy with `sim/conflict.rs` (423).

No new blocks, items or entities.

**Boundary records** (next to §9 of `docs/boundary.md`; field layouts are written with the code, and
the kinds are provisional: taken when the code lands, after checking other branches for the same ids):

| Kind | Name | Direction | What it carries |
|---|---|---|---|
| 0x080C | RAIL_PASS | Java to engine | faction, train id, train owner, the sampled point nearest the camp, entry and exit ticks |
| 0x080D | TRAIN_PASSED | Java to engine | agent, epoch, train id, the nearest carriage's index, watched flag |
| 0x080E | CAMP_DEATH | Java to engine | faction, body kind, chief flag, killer kind and UUID, op id |
| 0x080F | HOME_CENSUS | Java to engine | faction, bodies present by kind, chief present, crystal held |
| 0x0810 | TRADED | Java to engine | faction, player, gold nuggets that changed hands (the engine values them in coins at the grove market's price, as `Conflict::traded` takes coins), op id |
| 0x0811 | NOTICE_SHOWN | Java to engine | faction, player |
| 0x0091 | CHIEF_STATE (latest-wins) | Java to engine | faction, health, crystal held |
| 0x0092 | HAVEN_CHEST (latest-wins) | Java to engine | faction, dimension, a grove chest's position, its free slots (as `LAIR_CHEST`) |
| 0x0093 | DIFFICULTY (latest-wins) | Java to engine | the world's difficulty, sent at session start and on change |
| 0x8807 | AMBUSH_SPOT | engine to Java | agent, epoch, faction, dimension, the track point, the body's spot, reach; flag removed |
| 0x8808 | HOME_SPAWN | engine to Java | the answer to a census: faction, bodies to remove by kind (left behind, §4.9), bodies to spawn by kind, chief flag, crystal |
| 0x8809 | TRAIN_ROBBED | engine to Java | faction, train id |

FACTION_FOUND's reserved word (`docs/boundary.md:1363`) carries the census at discovery: the count
of home bodies, chief present, crystal held.

## 3. What happens today

### 3.1 Mowzie's Mobs in the pack

Mowzie's Mobs 1.8.2 runs with its default config. The pack touches it in one place: Structurify
disables the Wroughtnaut's `wrought_chamber` (`pack/config/structurify.json:405-411`, pack commit
`893bd9f`, "Remove structure causeing crash from mowzie's"). What the jar places and spawns:

| Mob | Where it lives (verified) | What it does today (verified) |
|---|---|---|
| Frostmaw | `frostmaw_spawn` dens in snowy biomes that are not ocean, river, beach, forest or taiga, height 50 to 100, away from villages and outposts (`ConfigHandler.java:443`). In this pack's biome set: 15 biomes, including `frozen_peaks`, `snowy_slopes`, `snowy_plains`, `ice_spikes` and Terralith's `emerald_peaks`, `glacial_chasm`, `rocky_mountains`. How often dens pass the 50 to 100 height check in this pack's terrain is UNVERIFIED. | Sleeps until a player comes within 8 blocks while it has a target (`EntityFrostmaw.java:590-606`); fights; goes back to sleep after 1,200 ticks without a target (`580-589`). Targets players only (`284`). Never wakes on Peaceful (`582`, `600`). Persistent while it holds its crystal (`977`); once robbed it despawns like any monster when no player is within 128 blocks, as nothing in the den's template sets `PersistenceRequired` (`MowzieEntity.java:291-317`). Wanders inside its restriction (`208-209`). |
| Umvuthi | `umvuthana_grove` in the 8 savanna biomes, height 50 to 100 (`ConfigHandler.java:466`) | Placed at the fire pit, finalized as a structure spawn and restricted there (`UmvuthanaGrovePieces.java:276-288`). Persistent (`EntityUmvuthi.java:393`). Targets unmasked players, iron golems, zombies, skeletons, Elokosa (`318-345`). In a fight, summons Umvuthana while fewer than 4 are within 50 blocks (`488`), and cranes each time it has lost 45 health (`469-471`; `spawn_healers_threshold`, `ConfigHandler.java:473`). A crane heals only the Umvuthi (`EntityUmvuthanaCrane.java:174`) and gives it Sunblock, under which it takes no damage (`EntityUmvuthi.java:628-632`). |
| Umvuthana | 5 to 9 in each grove, restricted to 25 blocks of its centre (`UmvuthanaGrovePieces.java:494-507`) | Persistent (`EntityUmvuthanaMinion.java:253`). Target unmasked players, zombies, skeletons, Elokosa (`133-151`). Trade with masked players (`107`, `236-240`). |
| Umvuthana raptor | natural spawns in savanna at y 60 or more, weight 5, groups of 1, away from villages, outposts and groves (`ConfigHandler.java:539`) | Leads 4 or 5 followers (`EntityUmvuthanaRaptor.java:191-199`), who take its target (`EntityUmvuthanaFollowerToRaptor.java:47`) and hunt on their own once leaderless (`94-97`). The raptor hunts animals with attack damage under 3 and a volume between 0.1 and 6 (`EntityUmvuthana.java:338-342`, `EntityUmvuthanaRaptor.java:88-90`): cows, sheep, pigs, chickens. |
| Naga | natural spawns where a biome is a beach and a mountain or hill, and on `minecraft:stony_shore`, at y 68 or more, in darkness under open sky (`ConfigHandler.java:526`, `MowzieEntity.java:223-227`); in this pack, `stony_shore` only | Stays within 50 blocks of home (`EntityNaga` `MAX_DIST_FROM_HOME`, `175`, `582`). Implements `FlyingAnimal` (`EntityNaga.java:155-159`). |
| Sculptor | the `monastery` on mountain peaks, y 120 to 200 (`ConfigHandler.java:420`) | A trial: it targets its attacker only once below 80% health (`EntitySculptor.java:199`, `321-323`), and disappears after granting its reward (`1447-1453`). |
| Bluff | the `monastery`: its `spawn_overrides` place Bluffs (weight 100, 1 to 2), and two of its spawner templates hold them; no natural spawn (empty biome list, `ConfigHandler.java:401`) | An ordinary mob. |
| Wroughtnaut | the `wrought_chamber`, disabled in this pack | Takes damage only while vulnerable, and then only from outside the 220 degrees in front of it (`EntityWroughtnaut.java:200-231`); a hit with no attacker entity does nothing unless it bypasses invulnerability. |
| Elokosa | jungle (`ConfigHandler.java:389`); a howler (`elokosa_howler`) spawns with 3 or 4 followers (`EntityElokosaHowler.finalizeSpawn`) | Ordinary mobs. |
| Foliaath | jungle (`ConfigHandler.java:551`) | Never moves; an `Enemy` (`EntityFoliaath.java:93-95`). |
| Lantern | `is_magical` biomes that are not snowy (`ConfigHandler.java:514`), which in this pack is `minecraft:dark_forest` only | Passive (`MobCategory.AMBIENT`, `EntityHandler.java:100`), 4 health. |
| Grottol | stone below y 16 outside mushroom biomes (`ConfigHandler.java:502`) | Only while fleeing, seeks a vanilla minecart on vanilla rails and drives it at 2.7 blocks a tick (`EntityAIGrottolFindMinecart`); 20 health. |

None of them breaks blocks: no Mowzie's mob checks `mobGriefing`, and the Frostmaw's ice breath turns
water to ice for 140 ticks only (`EntityIceBreath.java:210`).

**Trains and these mobs.** A Create train never touches a Frostmaw or the Umvuthi: both return
`PushReaction.IGNORE` from `getPistonPushReaction` (`EntityFrostmaw.java:332-334`,
`EntityUmvuthi.java:429-431`), and Create collides only with entities whose push reaction is `NORMAL`
(`AbstractContraptionEntity.canCollideWith`, `ContraptionCollider.collideEntities`). A train has no
check for entities in its way (`Train.tick`). A train driven at a Frostmaw passes through it.

### 3.2 Pack notes that disagree with the jar

These are pack-side corrections, outside this repository; listed so the plan rests on the jar.
- `pack/modDocs/bosses.md:51-65` says the other bosses "spawn naturally in their biomes"; the
  Frostmaw, Umvuthi and Sculptor are structure spawns. Its health figures differ from the jar's
  attributes: Frostmaw 100 (jar 250), Umvuthana 30 (jar 8), Naga 60 (jar 30), Bluff 40 (jar 45),
  Sculptor 100 (jar 140), Grottol 50 (jar 20), Foliaath 20 (jar 10), Lantern 30 (jar 4), Elokosa 60
  (jar 17; the howler 20); it lists a Wroughtnaut at 150 and a ferrous one at 80 (the jar registers
  only `ferrous_wroughtnaut`, 40: `EntityHandler.java:90`, `EntityWroughtnaut.java:198`). Its biomes
  are wrong for the Naga (stony shore, not swamp or jungle), the Bluff (the monastery) and the
  Sculptor (mountain-peak monastery, not jungle underground).
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
- **The ladder:** None, Notice, Envoy, Scouts, Raid, Invasion, Siege (`culture.rs:84`). A feud
  climbs one rung at a time when its grievance crosses the next rung's threshold, after the dwell on
  its current rung (`dwell_days`, indexed by the current rung) and outside any cowed time
  (`plan_feud`, `1536-1577`; `feud_wake`, `1580-1615`; `escalate`, `1660-1745`). Each climb sends that
  rung's party once; only Siege repeats, every `siege_interval_days` (`2776`, `2814`). A feud steps
  down one rung when its grievance falls under `hysteresis` (0.6) times its rung's threshold. A war
  party takes whole bodies from the pool and needs at least half its make-up (`2236-2238`).
- **Targets:** the focus district, else the focus position, else the camp (`target_of`, `1796-1806`).
  At a position with no unwatched district within 64 blocks a band "finds nothing to raid"
  (`resolve`, `2869-2878`).
- **Trains:** a war band on its way out ambushes the first train leg it crosses, on a roll under
  `ambush_milli` 500 in 1,000 (`2496-2534`; `tunables.rs:103`), through industry's `raid_train`.
- **A chief's death** (`chief_killed`, `4124-4196`, which takes a party): the killer gets a
  severity-4 Killing report that the chief and the heir witness; the chief dies and `succeed` runs at
  once: the heir becomes chief and a new heir is named, and for each player the new chief resents,
  0.5 (`heir_grudge_factor`, `tunables.rs:107`) times that resentment is added as a Killing impulse,
  on top of a grievance that is otherwise unchanged (`1091-1134`; one severity-4 Killing adds about
  2.5). The faction is cowed for `cowed_days` × 1000 ÷ the chief's boldness bend. One feud steps down
  `chief_rout_steps` (2) rungs, floored at Notice: the party's, else the killer's (`4168-4176`). The
  band routs. The body comes back only when the pool refills.
- **Old age:** `ensure_chief` runs `succeed` when the chief notable has died of age
  (`1082-1089`); goblin's chiefs live `chief_lifespan_days` 120.
- **Camps:** `add_faction` (`840-935`) joins every camp to the world as a `NodeKind::Haven(market)`
  node with population `(pool_cap / 4).max(1)`, starts the pool full at the culture's `pool_cap`, and
  picks up the sources already emitting within its reach.
- **Tribute:** paid to an envoy or at the camp; with no demand open it counts as a gift
  (`pay_tribute`, `1977-2021`).

## 4. The design

### 4.1 Two factions

| Faction | Camp | Chief | Members | Hates |
|---|---|---|---|---|
| `frostmaw` | each `mowziesmobs:frostmaw_spawn` | the Frostmaw | none besides it: a culture of one | nobody: it plunders nothing |
| `umvuthana` | each `mowziesmobs:umvuthana_grove` | the Umvuthi, who stays home (MZ-3) | the grove's Umvuthana warriors and raptor packs | `human` (village herds) |

Every value the jar does not give and this section does not set is goblin.json's, labelled
PLACEHOLDER (§4.14). The two cultures share goblin's thresholds `[10, 30, 50, 70, 140, 260]`, dwell
days `[0, 0.5, 0.5, 1, 4, 6, 8]`, hysteresis 0.6, half-life 6 days, patience 2 days, muster 1 day,
cowed 6 days, march 0.02 blocks a tick (`crates/rtsim-conflict/data/cultures/goblin.json`).

**Territory radius 200 blocks** for both (TUNABLE): half of their structure sets' spacing (25 chunks
× 16 / 2). Each set places at most one start per 400 × 400-block cell, so each has at most 6.25 per
1,000 × 1,000 blocks (fewer where the biome is wrong). Separation 8 chunks lets two dens stand 128
blocks apart, so circles overlap and one line can cross two ranges. `reach_blocks` is 0 for both
(PLACEHOLDER; goblin's is 300): only what happens inside the circle counts.

**Camps are found** as the Ice and Fire plugin finds lairs: when a chunk holding the structure's start
loads, the plugin reports FACTION_FOUND (flag LATE when found after generation) with the structure from
dictionary 4 and the census of the bodies there (§4.9). A camp in a chunk nobody has generated does
not exist yet: a line laid through a range nobody has explored meets no Frostmaw until that den
generates. That is a limit of the design.

**The two `market_culture` values:** `umvuthana` is `human` (its haven sells what its raptors take
to the human market's buyers, C24); `frostmaw` is `human` too, only to satisfy `Culture::resolve`,
with `haven: false` so the den joins as a market-less node (ledger 13).

### 4.2 The Frostmaw: a culture of one

**Who it is.** The den's Frostmaw is the faction's chief and only body. At home it stays the jar's
own entity, saved by vanilla and untagged (§4.9): asleep, holding its crystal, with its own health.
The engine keeps a copy of its state from the census and CHIEF_STATE (health, crystal) for the times
it is away. Asleep it heals 0.3 a tick (`heals_out_of_battle`, on by default, `ConfigHandler.java:446`;
`EntityFrostmaw.java:594-596`), full from nothing in about 830 ticks, so the engine counts a Frostmaw
that has come home as healed.

**Its pool is one.** `pool_cap` 1 (the den holds one Frostmaw, `frostmaw_spawn.nbt`). Its Raid and
Invasion parties are the chief alone: one `frostmaw` unit, flagged as the chief (`BAND_MATERIALIZE`
member stacks carry a chief flag, `docs/boundary.md:1400`). Its Envoy, Scouts and Siege parties are
empty (ledger 2): a beast makes no demands, sends no scouts and breaks no walls.

**What angers it** (decided, MZ-2): Rail (§4.3), with killing and building inside its range from
goblin. It ignores logging and mining (weight 0): nothing in the jar ties it to trees or ore.

**The ladder.**

| Rung | What happens |
|---|---|
| Notice (10) | The plugin plays `mowziesmobs:frostmaw.roar` to the player the feud is against, from 32 blocks (PLACEHOLDER) toward the den, when they are in the range. If they are not, the notice waits in the engine's save until their next entry, across logouts and restarts (NOTICE_SHOWN, ledger 14). |
| Envoy, Scouts | Skipped: a climb from Notice goes straight to Raid after Notice's half-day dwell. |
| Raid (70) | It leaves the den and walks to the focus: a spot beside the real track of the line that angers it most (§4.4). There it lies in wait for its patience (2 days, PLACEHOLDER), ambushing trains that pass. Then it goes home, bored (C24). While the grievance stays at 70 or more it comes again `repeat_days` (4, PLACEHOLDER) after each return (ledger 3). |
| Invasion (140) | After Raid's 4-day dwell: it waits beside the track at the line's station nearer the den, where every train from or to that station passes it and a district there is raided (§4.6). It repeats like the Raid. |
| Siege | Skipped: Invasion is its top rung. |

**Its class follows its crystal.** With the crystal it throws ice balls at targets 14.5 blocks away
or more and breathes ice at those within 13.5; both need the crystal (`EntityFrostmaw.java:559-578`).
A wall does not stop that, so it counts as `ground_ranged`. Without the crystal it has only its swipe
(6.5 blocks) and, below 60% health, its slam (8.5): `ground_melee`, which walls stop (I22,
`INDUSTRY.md:595-597`). Ledger 9 picks the unit row by the chief's crystal bit.

**When it dies,** wherever it dies, the chief is dead. The engine runs `chief_killed` (or its camp
form, ledger 5): the heir succeeds at once, holding the grudges §3.3 describes, the faction is cowed
for 6 days (scaled by the dead chief's boldness), one feud steps down two rungs, and any party out
routs. What this plan adds:
- **Skipped rungs.** Two rungs down from Raid is Envoy and from Invasion is Scouts; the Frostmaw has
  neither, so the feud lands on Notice (ledger 2). From Notice it climbs straight back to Raid once
  the cowed time is over and the grievance is still at 70 or more.
- **Killed at its den, with no party out.** The killer's feud steps down (ledger 5). If the killer
  has no feud with it, no feud steps down: every feud still waits out the cowed time and the empty
  den, but a stranger's kill does not clear another player's grudge.
- **A third player kills it on its raid.** The party's feud steps down, as the engine does today: a
  Frostmaw raiding Iva's line and killed by Bram drops Iva's feud to Notice. Bram gets the Killing
  report, so the heir holds a small grudge against him (about 2.5, under Notice) that decays.
- **The body.** The pool is empty while the den waits for a migrant (§4.10): at `recruits_per_day`
  1/6 (TUNABLE) a new Frostmaw is ready six days later, and the plugin places it in the den, asleep,
  the next time the den loads (HOME_SPAWN). Whether it arrives with its crystal is MZ-4.
- **Old age** (decided): `chief_lifespan_days` 1,000,000 (about 2,700 years) for both cultures, so
  the chief changes only when its body dies. Reason: an age death would make `ensure_chief` replace
  the chief with no body dying on screen, while the den's Frostmaw sleeps on unchanged; the renewal
  that matters comes from deaths players cause.

### 4.3 Rail: how a Frostmaw feels a train line (decided, MZ-12)

The range feels the trains themselves, on the track they really run.

- **Sampling.** Create simulates every train wherever it is, loaded or not (`GlobalRailwayManager`,
  ticked from the overworld). Once a second (20 ticks, PLACEHOLDER) the Create plugin's
  `TrainSampler` reads each overworld train's leading point (`Carriage.getLeadingPoint().getPosition(graph)`)
  and tests the segment from its previous sample against the territory circles in the core's spatial
  hash (512-block cells, PLACEHOLDER: a 200-block circle lies in at most 4). A train at top speed
  (28 blocks a second, Create's `trainTopSpeed`; 40 with fuel, `poweredTrainTopSpeed`,
  `CTrains.java:23-27`; no pack override) moves up to 40 blocks a sample, so the test is a segment
  against a circle, never a point. No chunk is loaded and no track is read. Nothing is keyed on the
  track graph: `TrackGraph.getChecksum` is a sum of node net ids (`TrackGraph.java:322-327`), which
  misses changes to edges alone and is reassigned at load, so a line is known only by its station
  pair.
- **A pass.** When a train's samples enter a circle, the sampler follows it until it leaves, keeps
  the sampled point nearest the camp, and reports one RAIL_PASS: the faction, the train's id and
  owner, that point, the entry and exit ticks. A train that stops inside a range reports its pass when
  it leaves.
- **The line.** The composition holds a pass until the train's next arrival at a station, which
  names the leg it was on (the industry engine already pairs a train's departure and arrival into a
  line edge, `engine.rs:2076-2087`). The pass then counts on that (line, faction): line meaning the
  unordered station pair. A den found after the line exists feels the line's next pass: the sampler
  reads the circles as they are now, so there is nothing to re-test.
- **The meter.** Each (line, faction) keeps a decayed count of its passes: on each pass n ← n ×
  e^(-Δt/τ) + 1, and its rate is n ÷ τ legs a day, with τ = 2 days (PLACEHOLDER). The composition
  pushes the rate into the engine's source only when it leaves a band around the last rate pushed
  (10% of it, and at least 1 leg a day, PLACEHOLDER), and not while both are under the tolerance.
  Between passes the rate falls as a closed form, so the composition books one wake for the moment
  it will cross the band's lower edge. A steady line makes no events: at 24 legs a day the rate
  ripples between 23.75 and 24.25; at 3 legs a day between 2.76 and 3.26, inside the 1-leg band.
- **The source.** One continuous source per (line, faction), pinned to that faction
  (`set_emission_for`, ledger 1). Its position is the line's sampled point nearest the den, kept as
  the feud's focus when this line's rate is its largest. Its actor is MZ-13's: recommended, the
  train's owner, else P14 at the line's two stations.
- **Weight and tolerance** (PLACEHOLDER): weight 1.0 per leg, tolerance 2 legs a day per line (one
  round trip). With the 6-day half-life (λ = ln 2 / 6 = 0.1155 a day) the grievance settles at 8.66 ×
  (legs a day - 2). Tolerance is per line, so a player who splits the same traffic over more station
  pairs through one range gets one more free round trip a day for each pair; `sim mowzie` measures
  whether that matters.

Days to each rung for a line started on day 0 and run steadily, with the meter and the band in the
loop (integrated in 1/1,200-day steps; dwell and temperament left out):

| Legs a day | Settles at | Notice (10) | Raid (70) | Invasion (140) | Siege (260) | Pushes |
|---|---|---|---|---|---|---|
| 4 | 17.3 | day 21 | never | never | never | 2 |
| 12 | 86.6 | day 3.0 | day 20 | never | never | 9 |
| 24 | 190.4 | day 1.8 | day 6.4 | day 16 | never | 15 |
| 50 | 415.5 | day 1.1 | day 3.5 | day 6.0 | day 11.5 (the Frostmaw has no Siege rung) | 22 |

The meter's lag (τ) adds about two days to each crossing against a rate known at once, and the band
holds the pushed rate up to 10% under the true one (at 24 legs a day the pushed rate sits at about
22.7, settling at 179 instead of 190).

**Running fewer trains works, on a timescale of days.** A line run at 12 legs a day from day 0 and 24
from day 2 peaks at a grievance of 120 on day 13 if traffic changes on day 12:

| From day 12 | Grievance under 70 (no more raids) | Under 42 (Raid rung left) | Settles at |
|---|---|---|---|
| cut to 4 legs a day | day 21.2 | day 27.9 | 17.3 (Notice: a roar now and then) |
| stopped | day 19.0 | day 23.5 | 0 (under 6, the Notice rung left, day 40.3) |

The old estimator (trips over a line's lifetime) would have held such a line near 14 legs a day for a
month after the cut; the meter forgets a stopped line in about two τ.

**The tribe** gives Rail weight 0 (PLACEHOLDER, my choice): nothing in the jar ties Umvuthana to
tracks.

### 4.4 Lying in wait: the Frostmaw against trains

**The spot.** The Raid's focus is a sampled track point. The Frostmaw's spot is 5 blocks (PLACEHOLDER)
off the track there, on the den's side, perpendicular to the track's direction between the two samples
around the point. The Frostmaw is 4 blocks wide, so its near edge stands 3 blocks clear of the rail
and no train renders through it; its swipe reaches 6.5 blocks, so it still reaches a passing carriage.
The engine sends AMBUSH_SPOT with the track point, the spot and a reach of 8 blocks (PLACEHOLDER: the
swipe's 6.5 plus half a carriage).

**Unwatched.** While its party waits out its patience, the sampler reports each train whose sampled
segment passes within the reach of the track point (TRAIN_PASSED). Each pass rolls `ambush_milli` (500
in 1,000). A hit is an I24 attack on that train, found by its id (`attack_train`, which takes the
train's current booked leg, `threat.rs:689`): source `Boss`, its class, strength 41.7 (§4.13). The
industry engine paces it: one attack per train per class per day (`cooldown_ticks` 24,000,
`crates/rtsim-industry/src/tunables.rs:184`). The cargo lost is at most the Frostmaw's carry,
PLACEHOLDER 64 (industry's own siege carry, `tunables.rs:180`), and at most 10% of the leg's cargo
(`theft_cap_ppm` 100,000, `181`). It is destroyed, not carried off (ledger 10; MZ-5). The loss reaches
the real carriage as `IND_CARRIAGE_DELTA` wherever the train is (`docs/industry-boundary.md:216`,
`263-265`). A train with no booked leg (one not run between declared stations) has no cargo on the
books and loses nothing.

So two trains passing a waiting Frostmaw lose at most 64 items each per day: at most 256 items over a
two-day wait. Track is never cut (I24).

**The cue.** The engine sends TRAIN_ROBBED; when that train next arrives at a station with a player
within 64 blocks (PLACEHOLDER), the plugin plays the roar there, from 32 blocks toward the den. The
player who finds a short train has heard why.

**Watched.** A player within view (`view_radius` 160, `tunables.rs:91`) of a waiting Frostmaw sees the
real one at its spot, a puppet with `alwaysActive` set so it stays awake (`EntityFrostmaw.java:361-363`,
`580`), restricted to its spot so its `MoveTowardsRestrictionGoal` holds it there (`208`). It targets
players only (`284`).
- **The swipe.** While it is watched the sampler samples trains within 64 blocks of it every tick.
  When a carriage comes within 6.5 blocks, the plugin starts the jar's swipe
  (`AnimationHandler.INSTANCE.sendAnimationMessage(frostmaw, SWIPE_ANIMATION)`, as the jar's AI does,
  `567`), turned toward the carriage. It is the jar's own area attack, so a rider within its arc is
  struck too (which entities it hits is settled by test 9). The cargo comes from the same TRAIN_PASSED
  and roll as an unwatched pass, so a watched line loses what an unwatched one would (P28, MZ-5): the
  delta lands within a step of the swipe, whether or not the carriage is still in view.
- **The ice ball: its move against riders.** A rider is a player, so the awake Frostmaw targets them
  as the train comes. At 14.5 blocks or more, with its crystal and the 200-tick cooldown spent, it
  throws an ice ball at 1.6 blocks a tick, aimed ahead of the rider from where they were one tick
  earlier (`EntityFrostmaw.java:484-501`). The ball hits every living entity within 2 blocks of it:
  3 freeze damage and 1.0 freeze progress (`EntityIceBall.java:60-64`). At 1.0 the rider is `FROZEN`
  for 50 ticks (`FrozenData.java:260-261`), and `onFreeze` seats them on a new frozen controller with
  `startRiding(controller, true)` (`FrozenData.java:185-199`), whose first step, in vanilla, is
  `stopRiding()`: the rider leaves the carriage seat. They stand frozen for 2.5 seconds beside an awake
  Frostmaw while the scheduled train runs on without them. Within 13.5 blocks the breath does the
  same in five hits (0.23 each, `EntityIceBreath.java:172`).
  - **Inferred, not read, and settled by test 8:** that no Create code vetoes the dismount (NeoForge's
    mount event can cancel `removeVehicle`; if Create cancels it, `startRiding` would seat the rider
    on two vehicles at once, and the plugin must not let that happen); that a carriage's blocks do not
    stop the ball (it bursts on world blocks, `EntityIceBall.java:65`, and a carriage's blocks are not
    world blocks, but whether vanilla's entity collision counts the contraption is unread); and how
    often a ball thrown at a train moving 1.4 blocks a tick (2 with fuel) lands within 2 blocks of the rider.
- **No collision.** A train passes through a Frostmaw (§3.1): the spot keeps it off the rail so it
  never looks that way.

**Its den while it is out.** Empty. A player who visits finds the ruin and no Frostmaw.

### 4.5 The Umvuthana: a tribe

**Who it is.** The grove's Umvuthana are the tribe's warriors (MZ-11). At home they stay the jar's
own entities, saved by vanilla and untagged (§4.9). On the grove's first load the plugin counts them,
and that count is both the pool and the grove's own `pool_cap` (5 to 9; ledger 7). The Umvuthi stays a
vanilla entity at its fire pit (persistent, `EntityUmvuthi.java:393`), so it keeps its own memory of
who traded with it (`TRADED_PLAYERS`, NBT `"players"`, `779-792`). Umvuthana it summons in a fight
are vanilla minions like the rest, and the census counts them up to the grove's cap. Its cranes stay
vanilla and are never counted: a crane heals and shields only the Umvuthi
(`EntityUmvuthanaCrane.java:174`), so it has no place in a band away from it. The pool is a count of
fighters, and a raptor pack counts as one.

**Units** (strength by §4.13):

| Unit | Body | Class | Strength | Carry, breaks |
|---|---|---|---|---|
| `warrior` | `mowziesmobs:umvuthana` | ground_melee | 0.4 | 0, 0 |
| `raptor` | a pack: `mowziesmobs:umvuthana_raptor` and 4 `umvuthana_follower_raptor` (the smaller of the jar's 4 or 5), one unit (C25 groups) | ground_melee | 2.6 | 0, 0 |
| `envoy` | `mowziesmobs:umvuthana` | ground_melee | 0.4 | 0, 0 |

Carry and breaks are 0 because no Umvuthana takes items from containers or breaks blocks in the jar.

**Parties** (PLACEHOLDER sizes, set so the smallest grove can field each: a war party is sent when the
pool holds at least half its make-up, `engine.rs:2236-2238`, and a grove starts with 5 to 9):
- envoy: 1 envoy and 2 warriors (goblin's pattern);
- scouts: 1 raptor pack;
- raid: 3 warriors and 1 raptor pack (4 units, strength 3.8);
- invasion: 6 warriors and 2 raptor packs (8 units, strength 7.6; a grove of 5 sends 5);
- siege: empty, so Invasion is the top rung: the tribe breaks no blocks, so it cannot besiege;
- plunder: 1 raptor pack, against village herds.

**What angers it** (goblin's weights, PLACEHOLDER): killing its kin inside the territory (any
`mowziesmobs:umvuthana*` entity, natural raptor packs included), logging, mining, building, smog.

**The envoy party** (decided). While a demand is open, the plugin strips the target goals of all
three bodies of the envoy party, and cancels any golem's target change onto them
(`LivingChangeTargetEvent`, which the core already handles, `ThreatEvents.java:32`, `56`). If a
player strikes the envoy or a guard, all three get their target goals back and the hit is an insult
(ENVOY_ANSWER 3). Killed by zombies or skeletons (which fight Umvuthana both ways), they are
casualties with killer kind mob, not an insult. Reason: nearly every base has an iron golem, and
every Umvuthana is an `Enemy` that golems attack (`IronGolem.java:78`); without this, golems would
turn every demand into a war the player never chose, and the guards would attack every unmasked
player at the base before a word was said.

**The ladder.**

| Rung | What happens |
|---|---|
| Notice (10) | `mowziesmobs:block.gong` plays to the player, as the Frostmaw's roar does, and waits for their next entry the same way. |
| Envoy (30) | An envoy party walks to the player's base (P26). The envoy's custom name shows the demand: at the Envoy threshold, 0.5 × 30 = 15 emeralds' worth, which is 135 gold nuggets at the fallback rate (below), within 3 days (goblin's `demand_days`, middling temperament). Right-clicking the envoy with the gold in hand pays (TRIBUTE_PAID); hitting it is an insult. A masked player's right-click opens the minion's own trade screen (`EntityUmvuthanaMinion.java:229-240`), so the bridge takes the payment first. |
| Scouts (50) | A raptor pack watches the base from the edge for `linger_days`. |
| Raid (70) | A war band comes for the player's base. |
| Invasion (140) | The pool's whole strength comes. |

**Tribute** is gold (MZ-6), priced by value (ledger 12): the culture asks 0.5 emerald's worth per point
of grievance (goblin's), converted at the grove market's price for gold nuggets, or at a fallback of
9 nuggets an emerald (PLACEHOLDER: one gold ingot an emerald) until the market has a price. Gold nuggets
are the tribe's trading currency on screen (a masked player buys clawed logs, cooked meat, feathers,
sticks and campfires from the grove's warriors with gold nuggets, and sells them melon, raw meat,
beetroot, fish, acacia saplings and bones for nuggets, one offer at a time, `EntityUmvuthanaMinion.java:107`;
the player's item goes in, `InventoryUmvuthana.java:97-105`). Paid tribute is stocked at the grove's
haven (C24) and the plugin puts it into the grove's real chests when the grove loads (a haven chest,
ledger 14, as `LAIR_CHEST`): a player who later raids the grove finds the gold they paid. Paying the
Umvuthi its 7 gold blocks (`ConfigHandler.java:470-471`) is reported as tribute at the camp
(`TRIBUTE_PAID` with agent 0): it settles the payer's demand, or with none open counts as a gift of 567
nuggets, 63 emeralds' worth at the fallback rate. Trading with the grove's minions while masked
(TRADED) softens the trader's feud at a tenth of a gift's rate (`Conflict::traded`,
`engine.rs:2045-2052`).

**Masks are honoured** (MZ-4): the jar's targeting skips a player wearing an Umvuthana mask
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
  owe the district dead animals (decided, MZ-8; ledger 11): when the district next loads, up to 2
  animals per raptor pack (PLACEHOLDER; measure in a GameTest: animals a raptor pack kills in a
  watched raid of the same length) matching the jar's hunt rule are killed where they stand, so their
  drops fall.
- **Plunder of a village:** a raptor pack takes livestock goods (`leather`, `wool`) from the village's
  stock, up to `loot_share_milli` (150 in 1,000, goblin's), eaten: the village has less to sell. Breaking
  a plundering pack makes nearby villages think better of the player (rescue reports within
  `rescue_news_blocks` 2,000, `tunables.rs:108`).

**MZ-8, decided: raptor hunts are owed and plunder is eaten.** Reason: C16 to C18 already require
attacks to land while the owner is away; a tribe raid that did nothing unwatched would break them.

### 4.6 Killing a boss at home

The Umvuthi dies at its fire pit, or the Frostmaw in its den. Both bodies are vanilla there, so the
plugin sees the death (`LivingDeathEvent`) and reports CAMP_DEATH with the chief flag and the killer
(ledger 5, 14). The engine runs the camp form of `chief_killed`: the heir succeeds, the faction is
cowed for `cowed_days` (6, scaled by the chief's boldness), the killer's feud drops two rungs (floor
Notice; for the Frostmaw, Notice), and any party out routs home. Loot is the jar's own: the Umvuthi
drops `sol_visage` when a player kills it (`loot_table/entities/umvuthi.json`); the Frostmaw drops
`ice_crystal` if a player kills it while it holds the crystal (`loot_table/entities/frostmaw.json`).

A warrior killed at the grove is a CAMP_DEATH without the chief flag: the pool loses one, and a kill
by a player is also the killing signal (P14).

The new chief's body: the Umvuthi is placed at the fire pit (HOME_SPAWN with the chief flag) the
first time the grove loads after the cowed time ends, finalized as a structure spawn and restricted
to the pit, as `UmvuthanaGrovePieces.java:276-288` does. The Frostmaw's comes with the pool (§4.2).

**At a station (Invasion).** A Frostmaw that finds a district within 64 blocks of its station spot
raids it as well: Lanchester, class by its crystal, nothing broken, nothing stolen beyond the cargo
rule.

### 4.7 On screen: real entities with the engine's state

| Body | At home | Away (a puppet) | State it gives back |
|---|---|---|---|
| Frostmaw | the jar's vanilla body, untagged; `PersistenceRequired` set by the plugin once it is robbed, so it does not despawn (§3.1) | its constructor; NBT `has_crystal`, `active`, `alwaysActive` set before its first tick (`EntityFrostmaw.java:960-975`); health from the engine | health and crystal (CHIEF_STATE; census), death with killer |
| Umvuthana warrior, envoy | the jar's vanilla bodies | its constructor, then `finalizeSpawn` so it gets its trade table (`EntityUmvuthanaMinion.java:243-250`); mask (NBT `"mask"`), weapon (NBT `"weapon"`, `EntityUmvuthana.java:682-690`) | casualties with killer |
| Raptor pack | not at home: a raptor pack exists only in a party | the raptor's constructor, then 4 followers through the public `(type, level, leader)` constructor (`EntityUmvuthanaFollowerToRaptor.java:33`), not `finalizeSpawn` (which adds 4 or 5 of its own, `EntityUmvuthanaRaptor.java:191-199`) | casualties |
| Umvuthi | vanilla, always; a new chief by HOME_SPAWN: its constructor, `finalizeSpawn` as a structure spawn, `restrictTo` the fire pit | never away (MZ-3) | death with killer |

Steering moves the restriction centre for the bodies that walk to it, the Frostmaw
(`EntityFrostmaw.java:208`) and the warrior (`EntityUmvuthanaMinion.java:127`); the raptor has no such
goal, so the bridge gives it navigation orders and its followers keep to it. Body kinds are registered
in code (P15 as built).

### 4.8 Effects through modules that exist

| On | Frostmaw | Umvuthana |
|---|---|---|
| Trains | Ambush while lying in wait: cargo destroyed, bounded (§4.4). Watched: swipes at carriages, ice balls unseat riders. The robbed-train roar. | None: bands on the march may cross a leg, but carry 0 takes nothing. |
| Stations | Invasion waits at the nearer station: every train from or to it passes. | None beyond the district. |
| Districts | An Invasion that finds a district within 64 blocks of its spot raids it: Lanchester, class by crystal; nothing broken or stolen. | Lanchester; walls stop it; raptors owe dead animals on load. |
| Villages | None: dens are 7 chunks or more from villages, and it plunders nothing. Villagers flee it at 10 blocks (`ServerEventHandler.java:250`). | Raptor plunder of livestock goods; villagers flee Umvuthana at 6 blocks (`247`). |
| Ships | None: its biomes exclude ocean, river and beach. A watched fight by a river port ices water for 140 ticks. | None. |
| Players | Watched: attacks players only. | Unmasked players, at their base (P26). |

### 4.9 Bodies, saves and uninstall (UR-1)

UR-1 lets Ben remove rtsim at any time and lets a reset lose only the sim's own state. Puppets are
never saved by vanilla (the core's `EntitySaveMixin`, `core/puppet/AgentTag.java:10-12`), so a den
whose Frostmaw were a puppet would be empty after an uninstall, and a grove of puppets would lose its
warriors. So:
- **Bodies at home stay vanilla:** saved by vanilla, untagged, with their own health, crystal, mask and
  weapon. Remove rtsim and every den and grove looks as the jar left it, give or take what players
  did.
- **A body is tagged only while it is away.** When a party leaves a loaded camp, the plugin takes the
  home body, removes it, and spawns the puppet with its state. When the camp is unloaded, the vanilla
  body stays in the saved chunk and the engine counts it as left behind; the next time the camp
  loads, the census first removes one vanilla body of that kind for each member left behind, whether
  that member is still away or has died since. So a Frostmaw killed by a line does not reappear in its
  den, and one still out is not in two places.
- **Coming home.** A party that reaches a loaded camp dematerializes into saved vanilla bodies there
  (the tag removed, `PersistenceRequired` set). One that reaches an unloaded camp returns to the pool,
  and the next load's census spawns the bodies.
- **The census** (HOME_CENSUS, every camp load; ledger 7), after those removals, compares the vanilla
  bodies at home with the engine's home count. Fewer than the engine expects and not ordered: the missing ones died unseen,
  and the pool loses them. Fewer because the pool refilled: the deficit is spawned (HOME_SPAWN), the
  Frostmaw asleep at the den's template spot, warriors at the grove's edge farthest from any player,
  restricted to its centre as `UmvuthanaGrovePieces.java:494-507` does. More (the Umvuthi summoned
  some): the pool takes them up to the grove's cap; the rest stay vanilla and uncounted.
- **A reset** recreates each faction from FACTION_FOUND, which carries the census: a robbed den comes
  back robbed (the body's `has_crystal` is 0), a grove comes back with the warriors standing in it, a
  den found empty starts with an empty pool (§4.10).
- **What an uninstall or reset loses:** members away from a loaded camp at that moment (puppets), and
  the grudges, which are the sim's own. The other way round, a member that left an unloaded camp and
  died away comes back with an uninstall, since its vanilla body was still in the saved den: the jar's
  state wins, which UR-1 allows.
- **Untested so far for any module:** the deploy rehearsal's uninstall had no puppets to check, as
  no player joined (`docs/deploy-rehearsal-omegasrv.md:461-462`); dragons carry the same gap. Test 4
  covers this plan's bodies.

### 4.10 Renewal: heirs, migrants and regrowth

Ben asked for populations that come back and settle into a balance. A separate design covers every
species (in progress on `plan/equilibrium`; its input is `research/population-inventory-2026-10-21.md`).
These two factions plug into it with the rules below; every rate is TUNABLE until that design sets it.

| What renews | How | Rate (TUNABLE) | On screen |
|---|---|---|---|
| A chief (the notable) | `succeed`, at once, holding the grudges (§3.3) | immediate | nothing until a body exists |
| A Frostmaw (the body) | the pool refills from 0 to 1 after a death, or for a den found empty; a migrant moves into the empty den | `recruits_per_day` 1/6 (6 days), as the dragons' vacant lairs refill on a delay (`respawn_delay_days` 20 in the inventory) | asleep in the den at its next load (HOME_SPAWN); with or without its crystal is MZ-4 |
| Grove warriors | the pool refills toward the grove's own cap | `recruits_per_day` 2 (goblin's): an emptied grove of 7 is whole in 3.5 days | walking in from the grove's edge, at the next load or, if the grove is loaded, as each whole body is ready |
| The Umvuthi (the body) | after the cowed time | 6 days, scaled by boldness | at the fire pit at the next load |
| Raptor packs in parties | from the pool, as warriors are | as warriors | only in parties |

Natural Mowzie spawns (raptor packs, Naga, Foliaath, Elokosa, Lantern, Grottol) are the jar's own and
outside the sim; a grove's census never counts a natural raptor.

**What this plan needs from the pan-species design:**
1. **Caps.** A per-camp cap (this plan sets 1 per den and the census count per grove) and whatever
   world-level ceiling it sets per species.
2. **Rates against pressure.** `recruits_per_day` for each culture tuned against how fast players kill
   them, so dens and groves neither vanish nor swamp a region; this plan's 1/6 and 2 are guesses.
3. **The empty camp rule.** One rule for a camp found empty or emptied for good (a migrant after a
   delay, as dragons do), shared with the dragons' vacant lairs.
4. **Predator and prey.** Raptor hunts and village plunder take animals and livestock goods; the herds
   module's populations and the villages' stocks must account for them in the same balance.
5. **A census in `sim`.** `rtsim-cli sim mowzie` reports dens with a body, groves' pools and the
   animals taken per day, in the form the pan-species design measures every species.

### 4.11 Relations

- **With each other:** none. Snow and savanna do not meet; neither culture hates the other.
- **Iron golems** attack every body in this plan but the envoy party (§4.5): vanilla's golem targets
  any `Enemy` but a creeper (`IronGolem.java:78`), and the Frostmaw (`EntityFrostmaw.java:152`), the
  Umvuthi (`EntityUmvuthi.java:209`), the warrior and crane (`EntityUmvuthanaMinion.java:105`), the
  raptor (`EntityUmvuthanaRaptor.java:71`) and its followers (`EntityUmvuthanaFollowerToRaptor.java:28`)
  all implement it. The Umvuthi targets golems back (`EntityUmvuthi.java:318-345`). Golems are I21
  guards against both factions.
- **Zombies and skeletons** fight Umvuthana both ways (`ServerEventHandler.java:227-236`; the
  warrior's target goals, `EntityUmvuthanaMinion.java:131-151`).
- **Farm animals** flee Umvuthana and the Umvuthi within 6 blocks and the Frostmaw within 10
  (`ServerEventHandler.java:239-244`).
- **Elokosa** (jungle) are the tribe's enemies on screen; not a faction (§4.15).
- **Carry On** (in the pack, `pack/mods/carry-on.pw.toml`): an Umvuthana picked up leaves the sim (A10).

### 4.12 Notices, feedback and Peaceful

- **A notice is never lost.** The engine keeps an unplayed notice per feud and resends it at every
  session start; the plugin plays it the next time the player is in the range and answers
  NOTICE_SHOWN.
- **Unwatched losses are heard.** The robbed-train roar (§4.4); and the industry's attack reports carry
  the loss for the viewer.
- **Peaceful** (decided): the plugin reports the world's difficulty; while it is Peaceful no war party
  of either culture leaves, ambushes and raids do nothing, and feuds keep their grievance and decay as
  usual. The jar's Frostmaw never wakes on Peaceful (`EntityFrostmaw.java:582`, `600`), so unwatched
  losses with no fight possible would be unfair.

### 4.13 Strength

The engine's strength unit is one ordinary mob: an observed fight counts one per attacker today
(`core/industry/Threats.java:384-385`). The plan's rule (TUNABLE): health × attack ÷ a zombie's (20 × 3).

| Body | Health × attack | Strength |
|---|---|---|
| Frostmaw | 250 × 10 (`EntityFrostmaw.java:291`) | 41.7 |
| Umvuthi | 150 × 2 (`EntityUmvuthi.java:361`) | 5.0 (understated: its damage is its sun abilities) |
| Umvuthana warrior, crane, follower | 8 × 3 (`EntityUmvuthana.java:451-452`; one attribute set, `EntityHandler.java:244`, `247-248`) | 0.4 |
| Raptor | 10 × 6 (`EntityUmvuthanaRaptor.java:97-98`) | 1.0; a pack of 1 and 4 followers 2.6 |

Measured in GameTests (§7) against reference bases through `IND_FIGHT`, with the strength hook (ledger
19). L2 Hostility may scale these mobs' health; UNVERIFIED.

### 4.14 Tunables and placeholders

| Value | Here | Source | What it tunes | How to measure |
|---|---|---|---|---|
| Territory radius | 200 | TUNABLE: half the structure spacing | how far a camp's range reaches | GameTest census of den spacing in a generated world; Ben's call |
| Rail weight, tolerance | 1.0, 2 legs a day per line | PLACEHOLDER | how much traffic a Frostmaw stands | `sim mowzie` sweep of legs a day against days to the first raid; Ben's call on the target |
| Meter τ, band | 2 days; 10% and at least 1 leg a day | PLACEHOLDER | how fast a changed line is felt; pushes a line costs | `sim mowzie` cut and reroute runs; push counts |
| Sample period, hash cell | 20 ticks, 512 blocks | PLACEHOLDER | sampler cost and pass accuracy | bench at 200 trains; test 11 on the fixture's curves |
| Lurk offset, reach | 5 blocks off the rail, 8 blocks | PLACEHOLDER | where it stands; which passes count | test 9 |
| Repeat | 4 days after a return | PLACEHOLDER | how often an angry Frostmaw comes back | `sim mowzie` line run; Ben's call |
| Frostmaw carry | 64 | industry's siege carry | cargo lost per ambush | Ben's call (MZ-5) |
| Recruits | Frostmaw 1/6 a day; tribe 2 a day | TUNABLE | heir and migrant arrival; regrowth | the pan-species design; `sim mowzie` renewal run |
| Thresholds, dwell, half-life, patience, muster, cowed | goblin's | PLACEHOLDER | pace of the ladder | `sim mowzie` |
| Tribute | 0.5 emerald's worth a point; fallback 9 nuggets an emerald | goblin's; PLACEHOLDER rate | the price of peace | Ben's call (MZ-6) |
| Party sizes | §4.5 | PLACEHOLDER: a grove of 5 can field each | raid strength | GameTest fights |
| Strength rule | health × attack ÷ 60 | TUNABLE | Lanchester inputs | GameTest fights through `IND_FIGHT` |
| Animals owed per raptor pack | 2 | PLACEHOLDER | herd loss to a won raid | GameTest: animals killed in a watched raid |
| Notice and cue distance | 32 blocks toward the den; cue within 64 of a station | PLACEHOLDER | where the roar or gong comes from | listen in game |

### 4.15 Left out, and why

| Mob | Why | What would bring it in |
|---|---|---|
| Ferrous Wroughtnaut | Its chamber is disabled in this pack (`structurify.json:405-411`). MZ-9, decided: left out. The crash is found: a local copy of the server's crash reports holds five watchdog reports in which a C2ME worker is stuck in `WroughtnautChamberStructure.tryWroughtChamber` calling `getBaseColumn`; the jar's placement check reads a noise column every 2 blocks over a 64 × 64-block square and more for walls (`WroughtnautChamberStructure.java:77-109`), up to 2,112 columns per candidate. | A fixed placement check in a later Mowzie release, then a stationary home boss like the Umvuthi. |
| Sculptor | A trial, not a ruler: it fights only once below 80% health and leaves after its reward. | Nothing planned. |
| Bluff | Lives at the monastery, beside the Sculptor; no camp of its own. | Nothing planned. |
| Naga | Lives only on `stony_shore` in this pack, within 50 blocks of where it spawned, and has no camp. It already counts as a flying threat to districts: it implements `FlyingAnimal`, which `ThreatClasses.classOf` reads (`ThreatClasses.java:55-56`), so it needs no tag. | Nothing planned. |
| Foliaath | Never moves, so it is no pressure on a district; today a natural Foliaath counts as ground-melee pressure (`ThreatEvents.java:45-53`). The core exclusion tag (ledger 20) stops that. | Nothing more planned. |
| Elokosa | No camp; howler packs in jungles. | A jungle faction if one is ever wanted. |
| Lantern, Grottol | Passive (Lantern), or a cart-riding fugitive (Grottol). | Nothing planned. |

## 5. Walkthroughs

**A. A rail line through a Frostmaw's range.**
1. Iva mines copper in the snowy slopes. She lays Create track 1,800 blocks south to her smelter town,
   with a station at each end, "Copper Halt" and "Smelter". The track bends around a ridge and runs
   150 blocks from a Frostmaw den she has never seen. The den's chunk loaded once when she scouted the
   route: the plugin reported FACTION_FOUND with its census (one Frostmaw, holding its crystal). The
   Frostmaw sleeps on as the jar's own entity.
2. Day 0: one train shuttles between the stations, 12 legs a Minecraft day. Every leg's leading point
   enters the den's circle; the sampler reports a RAIL_PASS each time, and on each arrival the pass
   counts on the line {Copper Halt, Smelter} for this den. The meter climbs; the first pushes set a
   Rail source against Iva, the train's owner (MZ-13 (a)).
3. Day 2: she adds a second train: 24 legs a day.
4. Day 2.8: the grievance passes 10. She is at the smelter, outside the range, so the notice waits.
   The next morning she rides north and, as her train enters the range, hears a roar from the north.
5. Day 7.6: the grievance passes 70; the Frostmaw climbs from Notice straight to Raid. After its
   1-day muster it leaves the den (the den is unloaded, so its vanilla body stays in the saved chunk
   and the party marches abstractly), walks 150 blocks at 0.02 blocks a tick (7,500 ticks, a third of
   a day) and waits 5 blocks off the rail at the point of the real track nearest its den, from day
   8.9 to day 10.9.
6. She is at her smelter, 1,500 blocks away. Each train passing within 8 blocks of the spot rolls; a
   hit takes up to 64 ingots and at most 10% of what the train carries, destroyed, at most once a
   train a day. Over the two days her two trains lose at most 256 ingots. When a robbed train pulls
   into Smelter, she hears the roar from the north. No track is cut.
7. Day 9.5: she rides north. Rounding the ridge, she sees the Frostmaw beside the line, awake. It
   throws an ice ball well ahead of her train; it bursts beside her seat, she freezes, and she is
   pulled off the carriage. For 2.5 seconds she stands frozen in the snow next to the Frostmaw while
   her train runs on to Copper Halt. As the second carriage passed, it swiped; the carriage's chest is
   short when it arrives (the same booked roll as an unwatched pass).
8. Her choices:
   - **Kill it.** Her bow and her trident do nothing; she brings a TaCZ rifle and fire. It falls after
     250 damage; she gets an ice crystal (it still had its crystal and a player killed it). Her feud
     drops to Notice, the faction is cowed for about 6 days, and the heir takes over at once, holding
     the old grievance plus about 2.5. Six days later a new Frostmaw sleeps in the den (MZ-4). If her
     24 legs a day still run, the grievance is still over 70 when the cowed time ends, and the new
     Frostmaw raids within a day or two. Killing buys a week.
   - **Steal its crystal first.** She drinks a potion of invisibility and walks into the den while it
     sleeps (a sleeping Frostmaw drops a target that has invisibility, `EntityFrostmaw.java:597-599`).
     She stands by the crystal in its fist: it is hers, and the Frostmaw wakes without it. The plugin
     sets it persistent, so it stays in its den instead of despawning when she leaves, and reports its
     state. From now on it has no breath or ice balls, so riders are safe from the dismount; it counts
     as melee, and a 2-block wall at her station stops it while she is away.
   - **Run fewer trains.** She cuts back to 4 legs a day on day 12. The grievance peaks at 120 on day
     13, falls under 70 on day 21 (no more raids) and under 42 on day 28, when the feud drops back to
     Notice. Stopping the line entirely gets there by day 23.5.
   - **Reroute.** She lays new track west around the range. Her trains' passes stop, and the old
     line's meter decays as if she had stopped it. A halt station with no new track changes nothing:
     the range feels the track, not the stations.
   - **Defend the station.** At 140 (about day 16 at 24 legs a day) it comes to wait by Copper Halt,
     the station nearer its den. A Create Big Cannons autocannon on a Create Radars controller with
     auto-target and auto-fire counts against every class (I21), and its shells hurt the Frostmaw
     without giving it anyone to turn on; a 2-block wall counts only once the crystal is gone; an iron
     golem counts as a guard.
   - **Pay.** The Frostmaw takes no tribute. Running the trains and accepting the losses is the price.

**B. A savanna base near a grove.**
1. Tomas builds a windmill and a cattle ranch 150 blocks from an Umvuthana grove, inside its
   200-block range. The grove loaded when he arrived: the census counted 7 warriors, so the tribe's
   pool and cap are 7.
2. While he scouts, four masked warriors chase him and he kills them (killing weighs 8 each: 32).
   Each death is a CAMP_DEATH: the pool is 3. That evening he hears a gong.
3. Half a day later (Notice's dwell), his grievance has decayed to 30.2 and the tribe climbs to Envoy:
   the demand is made then, at 0.5 emerald's worth a point, 9 nuggets an emerald, rounded up: 136 gold
   nuggets, due 3 days later (goblin's `demand_days`, middling temperament). After its one-day muster
   and the 150-block walk, about two days after the kills, an envoy and two guards walk into his
   ranch. His iron golem watches them pass; the guards ignore him. The envoy's name shows "136 gold
   nuggets". He right-clicks it with 136 nuggets (about 15 gold ingots): the demand is settled, 70%
   of the grievance goes (`tribute_relief`), and the tribe cannot climb for 6 days (`respite_days`).
   When he next visits the grove, his nuggets are in its chests, and new warriors have walked in from
   its edge (the pool refills at 2 a day).
4. He clears acacia for his ranch, day after day, and kills the warriors who come at him unmasked.
   Weeks later the grievance passes 70 and a war band comes: three warriors and a
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
   shields it with Sunblock his shots do nothing, so he shoots the cranes first. When it dies, his own
   feud drops two rungs, the tribe is cowed for about six days and its bands run home. He takes the
   Sol Visage. When the cowed time is over, a new Umvuthi sits at the pit the next time the grove
   loads.

**C. A village herd.**
1. A plains village 900 blocks from the grove trades wool and leather. The tribe plunders on its own
   schedule (`plunder_mean_days` 12, goblin's): a raptor pack takes the village's leather and wool.
2. The village has less wool and leather to sell. Tomas, passing through, sees the pack and breaks
   it: villages within 2,000 blocks hear of the rescue and think better of him.

**D. Clearing a range, and what comes back.**
1. Iva kills the Frostmaw by her line on day 9.5, as in A. The den is empty.
2. She visits the den on day 12. It loads, and the census removes the vanilla body left behind when
   the Frostmaw marched (§4.9): she finds the ruin and no Frostmaw. The pool is at 0.4.
3. On day 15.5 the pool reaches 1. Nothing shows yet: the den is unloaded.
4. On day 17 she flies over the den; it loads, the census finds no Frostmaw where the engine expects
   one, and the plugin places it asleep in the ruin. If her trains still run, it is already angry:
   the heir took the old grievance.
5. Bram, another player, kills it at its den on day 20 while it sleeps (he has no feud with it). The
   faction is cowed again, the heir grudge falls on Bram, and Iva's feud does not step down; she only
   gains the six empty days.

**E. Removing rtsim.**
Ben removes mic-rtsim on day 10, while the Frostmaw waits by Iva's line. The puppet by the line is not
saved and is gone; the den's saved chunk still holds the vanilla body (the den has not loaded since it
left), so after the uninstall the Frostmaw is home asleep. The grove keeps its vanilla warriors and its
Umvuthi. Ben reinstalls on day 30: FACTION_FOUND rebuilds both factions from their censuses (crystal
held, 7 warriors), with no grudges.

## 6. Cost model

| Mechanic | When it costs | Cost |
|---|---|---|
| Idle factions | per tick | zero: grievance is closed form, one booked event per feud |
| A faction | per faction-day while its page is read | 52.3 µs at 1,024 factions, 57.3 µs at 4,096 (`docs/choices-paging-modules.md:292`, PM-C12) |
| A faction | resident | about 580 bytes (PM-C13, `:293`), plus about 125 bytes of indexes (PM-C4, `:284`) |
| A grove's haven node | resident, in the phase 1 world | about 17 KB a node in a plain world, about 51 KB a node in the conflict sweep, where raids and envoys make reports (`docs/choices-paging-modules.md:58`, `66-67`); only groves players have found |
| A den's node | resident | a market-less node (ledger 13): no market rows (6.6 KB of the 17 KB above); its node, wheel and index share remain, to be measured |
| Train sampler | per second | one leading-point read and one hash lookup per overworld train; about 1 µs a train (inferred), about 0.2 ms a second at 200 trains; no chunk loads |
| Watched swipe timing | per tick, while a player watches a lurking Frostmaw | the trains within 64 blocks of it |
| RAIL_PASS | per train per range crossed | one observation |
| Rail meters | per (line, faction) | a few dozen bytes and one booked wake; pushes only while the rate moves: 2 to 22 while a new line ramps up (§4.3), none while it is steady |
| Lying in wait | per pass within reach of a spot | one roll; a hit is one `attack_train`, as today's bandit ambushes |
| Camp finding and census | per camp chunk load | one check of the chunk's structure starts (`Lairs.java:114-125`) and one entity scan of the den's template or the grove's 25-block circle |
| Notices | per feud | one pending bit, resent at session start |
| Watched boss | while seen | one Frostmaw, or a band in waves of at most 16 |
| Animals owed | per district load with a debt | one entity scan of the district's loaded chunks, capped |
| Density | per world | at most 6.25 dens and 6.25 groves per 1,000 × 1,000 blocks; 4,096 factions (the paged sweep's top, `docs/choices-paging-modules.md:23`) of one kind at that density cover 655 million square blocks, a square about 25,600 blocks on a side |
| Nobody near any camp, no train in any range, nothing changing | | the sampler's per-second walk only |

## 7. Tests before it ships

**GameTests** (`neoforge/mowziesmobs/.../gametest/MowzieGameTests.java`; names in the dragons' style,
`DragonGameTests.java`, 688 lines). A real structure start cannot be placed in a GameTest (`/place`
registers no start, `docs/overnight-choices-iceandfire.md:140-143`), so tests declare their camps. The
rail tests build on `RailFixtureGameTests` (branch `test/create-rail-fixture`); whether its train runs
its schedule under GameTest's unthrottled ticks is UNVERIFIED (no GameTest has run an assembled train on
a schedule, `neoforge/README.md:262`), so each rail test has a stationary form that settles the
mechanism, and the moving form runs if the fixture's train moves.
1. `aDensFrostmawStaysVanillaAndTheCensusReadsItsCrystalAndHealth`.
2. `aRobbedFrostmawIsKeptAndAResetDoesNotGiveItsCrystalBack`.
3. `aGroveIsCountedAndAResetRecountsTheWarriorsStandingInIt`.
4. `aBodyComingHomeBecomesASavedVanillaBodyAndAnAwayPuppetIsNotSaved`.
5. `aRefilledPoolIsSpawnedAtTheNextCampLoad`, for a den and a grove.
6. `anArrowAndATridentDoNothingAndABulletHurtsAFrostmaw` (TaCZ's bullet if TaCZ is in the test run;
   otherwise a `Projectile` stand-in).
7. `anInvisiblePlayerStealsTheCrystalFromASleepingFrostmaw` (a mock player).
8. `anIceBallUnseatsARiderFromACarriage`: a mock player seated on a fixture carriage, the Frostmaw
   targeting them. It settles the inferred steps of §4.4: no veto on the dismount (the rider ends on
   the frozen controller and, after thawing, on no vehicle), a ball thrown through the carriage's
   blocks, and, in the moving form, the hit rate at 0.5, 1.0, 1.4 and 2.0 blocks a tick.
9. `aLurkingFrostmawStandsOffTheRailAndSwipesAPassingCarriage`: the spot 5 blocks off the track, the
   swipe on a carriage within 6.5 blocks, the booked delta in `Carriage.storage`, and which entities
   the swipe strikes.
10. `aTrainPassesThroughAFrostmawWithoutTouchingIt`: guards §3.1's finding (speed unchanged, no damage).
11. `theSamplerReportsPassesOnTheRealTrack`: on the fixture's 90 degree curve and S-bend, a circle the
    track enters but the chord between the stations misses gets a pass, and one the chord crosses but
    the track avoids gets none.
12. `aMaskedPlayerIsNotTargetedByTheBand`.
13. `theUmvuthisTradeIsReportedAsTributeAtTheCamp`.
14. `anEnvoyPartyIgnoresGolemsAndPlayersUntilStruckAndTakesPayment`.
15. `aChiefKilledAtTheCampIsReportedWithTheKiller`, for both bosses.
16. `tributeLandsInAGroveChestWhenTheGroveLoads`.
17. `raptorsHuntTheBasesAnimalsAndAnOwedHuntIsAppliedOnLoad`.
18. `aNaturalFoliaathIsNotCountedAsPressure` (the core tag).
19. `aNoticeWaitsForThePlayersNextEntryAcrossALogout`.
20. Strength fights: each body against reference bases through `IND_FIGHT`, for §4.13.

**`rtsim-cli sim mowzie`** (a scenario beside `sim conflict`, `main.rs:81-85`), with runs:
1. A line through a range, with the meter: notice, raid, repeat, invasion; ambushes bounded per train
   per day; a kill, cowed, the heir, the body six days later.
2. Cut service and stopped service: the grievance follows §4.3's second table; no raid once it is
   under 70.
3. Reroute: the new track's passes miss the circle and the old line's meter decays.
4. Two dens on one line: both feel it; a den found after the line exists feels the next pass.
5. The tribe: envoy paid at the priced demand; envoy refused, then a raid and an invasion; plunder.
6. Renewal: an emptied grove refills to its own cap; an emptied den gets a body after six days; a den
   found empty gets a migrant.
7. Kills: by the feud's player on a raid, by a third player on a raid, at the camp by a player with no
   feud: which feud steps down in each.
8. Save and restore in the middle of a lurk, a cowed time and a meter's decay, with identical results.

**Bench:** `rtsim-cli bench scale --only factions,factions-paged` (`crates/rtsim-cli/src/main.rs:25`)
before and after, and a sampler bench at 200 trains on the laptop.

## 8. Decisions and questions for Ben

**Decided here** (each can be undone; the reason is the plan's):

| Id | Decided | Reason |
|---|---|---|
| MZ-2 | A Frostmaw is angered by Rail, plus killing and building in its range | Ben's request names trains through yeti territory. Rail for other cultures (goblins, bandits) is a possible follow-up. |
| MZ-8 | Won raptor raids owe a district dead animals on load; village plunder takes livestock goods, eaten | C16 to C18 require attacks while the owner is away; "nothing unwatched" would break them. |
| MZ-9 | The Wroughtnaut stays out | Its crash is found (§4.15) and is in the jar's own placement check, which this plan cannot fix. |
| MZ-12 | Real track, by sampling trains once a second | Ben rejected straight lines between stations; chords put the Frostmaw where no train runs and anger the wrong dens. |
| Old age | `chief_lifespan_days` 1,000,000 for both cultures | §4.2. |
| Envoy party | Targeting stripped and golems kept off while a demand is open | §4.5. |
| Peaceful | No war parties leave | §4.12. |
| Kills | A chief killed at its camp steps down the killer's feud only; a killer with no feud steps none down; a third player's kill on a raid steps down the raided player's feud, as today | §4.2: a stranger's kill should not clear another player's grudge, but breaking a raid should end it. |
| Den node | A market-less world node (`NodeKind::Lair`), not a haven | Ledger 13: a Frostmaw trades nothing, and a haven costs about 17 KB. |
| Robbed Frostmaw | Kept persistent in its den | The steal should disarm it, not remove it; otherwise stealing and leaving farms a crystal every six days without a fight. |

**Questions** (recommended option first):

| Id | Question | Recommended | Alternatives |
|---|---|---|---|
| MZ-1 | Where the conflict module's Java half lives (none exists; it blocks every conflict plugin) | a vanilla `ConflictBridge` in the core owns the conflict records and the territory hash; the Mowzie plugin registers bodies, finds camps, takes censuses; the Create plugin samples trains | (b) the Mowzie plugin owns the conflict range itself (the first plugin to register owns a kind, `SessionPlugins.java:23-29`, so a later goblin plugin would collide) |
| MZ-4 | After a boss dies, and what of Mowzie's items the pack keeps (this merges MZ-4, MZ-7 and MZ-10) | (a) heirs come back (the body after 6 days, TUNABLE), the Frostmaw with its crystal; keep Mowzie's drops, masks, trades and the steal. The ice crystal and Sol Visage become renewable at one per den or grove per heir, and `tacz_new_recipes.js:994` keeps a supply of ice crystal. A Frostmaw robbed of its crystal drops none, so stealing then killing still yields one crystal per heir. | (b) the heir arrives without its crystal: no crystal renewal, and after its first death a den is melee-only, so walls hold it and riders are safe; (c) heirs with crystals, but Mowzie's items removed as the README's TODO says: masks, trades and drops go, tribute is the only peace with the tribe, and stealing the crystal still disarms but gives nothing |
| MZ-5 | What a Frostmaw ambush does to a train (trains cannot hit it, §3.1) | (a) cargo destroyed, at most 64 a train a day and 10% of a leg, the same watched or not; watched, it swipes passing carriages from beside the track and its ice balls unseat riders | (b) a forced stop: the train is held at the spot for a time (a mixin on Create's train), so riders must face it and unwatched legs arrive late; (c) a delay only: legs arrive late, no cargo lost (INDUSTRY.md §13.8 lists delay as an alternative); (d) riders only, as in the jar: unwatched trains lose nothing |
| MZ-13 | Whom a line's grudge falls on | (a) the train's owner (Create's `Train.owner`; who sets it is checked in the hash-matched jar before code), with P14 at the line's stations when it has none | (b) P14 at the stations: whoever spends most time in their chunks, which in multiplayer can be a bystander |
| MZ-11 | The grove's people | (a) the real warriors, saved as vanilla at home, counted by a census on each load, tagged only while away (§4.9): killing them at the grove shrinks raids, and an uninstall leaves the grove as it was | (b) left vanilla, with a separate abstract pool: the grove looks the same whatever happens |
| MZ-3 | Which bosses leave home | the Frostmaw only; the Umvuthi is fought at its grove | (b) the Umvuthi also leads the Invasion in person (it then needs a puppet form, and its trade memory moves into the engine); (c) neither |
| MZ-6 | The tribe's tribute | (a) gold nuggets, a new phase 1 good, priced by value: 0.5 emerald's worth a point, 9 nuggets an emerald until the market has a price (135 nuggets at the Envoy threshold) | (b) emeralds, as goblins (no new good); (c) gold ingots, priced the same way (also a new phase 1 good: 15 ingots at the Envoy threshold, in coarser steps) |
