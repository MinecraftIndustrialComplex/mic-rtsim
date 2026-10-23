# Mowzie's Mobs: factions and boss encounters

**Status:** decided; the build starts 2026-10-22. Revised 2026-10-21 after two adversarial reviews that were
checked against the jars, then reworked the same day for Ben's answers (§8), and again on 2026-10-22 for
his rejection of the horse: the Umvuthi travels aloft, MZ-16's second round (§4.5, §8), fixed the same
day for a third review against the jar and vanilla's sources. Touched again on 2026-10-22 for alliances
(CONFLICT-LIFE.md §2.16, Ben's CL-5 (b)): §0, §4.5, §4.11, §4.12 and §4.16's BAND_MARCH. Ben answered
the last open questions the same day (MZ-16's second round, MZ-17, CHIEF_PAID and the UR-1 exposure of
marches, §8), so nothing is left open. §9 is the build order.

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
- **Ben's answers (2026-10-21, §8):** MZ-1, MZ-4, MZ-5, MZ-6, MZ-11 and MZ-13 chose (a), as recommended;
  **MZ-3 chose (b)**, not the recommendation: the Umvuthi also leads the Invasion in person (§4.5), and
  **MZ-15** amends it: whoever the grove sends walks the route to the target and back where a player can
  see it (§4.5, §4.16); and EQUILIBRIUM.md's EQ-4 chose (b): a Frostmaw's body comes back after
  `min(192, 6 × 2^P)` days, P being its den's pressure (§4.2, §4.10).
- **Ben's answer (2026-10-22, §8):** MZ-16's horse is rejected, and every other animal mount with it:
  "Given the current models for umuthi (sunbirds), them riding horses / most other animals doesn't
  make too much sense." The Umvuthi floats seated above the rear of its column instead, borne by its
  sun: (a) of MZ-16's second round, which Ben chose the same day (§4.5).
- **Ben's last answers (2026-10-22, §8):** MZ-16's second round, (a) aloft; MZ-17, (a) capture per
  stack, which amends C25; CHIEF_PAID (0x0814) accepted; and the UR-1 exposure of marches accepted, as
  §4.9 and §4.16 describe.
- **The pack's own TODO** (`pack/README.md:17-26`): "Boss raid system", "Natural Boss Spawning",
  "Remove items & progression from boss mods: ... mowzies mobs", "Mowzies mobs: Natty spawns". Ben
  chose to keep Mowzie's items, against the third (MZ-4 (a), §8).
- **UR-1** (`DECISIONS.md:278`): rtsim can be removed and reinstalled at any time; a reset may lose
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
leaves without you. Push it further and it comes back every few days, then camps by the station
nearest its den. Run fewer trains and it calms down over days; lay your track around its range and it
forgets the line.

You can kill it. Arrows and tridents bounce off it; swords and axes, fire, TaCZ bullets and Create Big
Cannons shells all hurt it, and a cannon shell has no attacker it can turn on. You can sneak up while
it sleeps, invisible, and steal its crystal: then it has no ice attacks and no ice balls to throw at
your riders, and your walls hold it. Kill it and the range is quiet for a while, until another
Frostmaw moves into the empty den, carrying its predecessor's grudge. On Peaceful, nothing comes.

In the savannas, Umvuthana groves are villages of masked warriors around a fire pit, ruled by the
Umvuthi, a sun-priest boss that never takes a step. Today they attack anyone unmasked who comes close.
With this plan the grove is a tribe. Log its acacias, mine its hills, kill its people, and it notices: a
gong sounds in the distance. Then a masked envoy and two guards walk to your base, ignoring your iron
golem and you, and ask for gold. Pay, and the gold ends up in the grove's chests. Refuse, or strike
them, and raptor-masked hunters come for your herds and the villages' herds, then a war band of masked
warriors. When the whole tribe marches, the Umvuthi comes with it, seated in the air two blocks above
the ground at the rear of the column, its legs hanging as they hang over its throne's edge and the sun
on its head alight: from a hilltop you watch it lift from its throne and follow its warriors out of the
grove and across the savanna toward you, and with Distant Horizons you see the column coming a kilometre
off. At your base it sinks to the ground and summons more. Wear an Umvuthana mask and the tribe leaves
you alone, as it does today. Kill the Umvuthi, on its throne or out on the invasion (its healers make it
untouchable until you kill them), and every masked warrior near it falls with it, its raiders flee, and
the tribe stays quiet until a warrior takes up its mask. A grove you have thinned fills back up over a
few days, with warriors walking in from its edge. Rarely, a grove and a goblin camp that both hate you
swear against you: the envoy names both, and the masked war band and the goblins march on you from their
two camps and arrive together, never fighting each other (CONFLICT-LIFE.md §2.16).

## 1. Reuse table

| What happens | The real mechanic | Mod (verified id or class) |
|---|---|---|
| A Frostmaw's den | `mowziesmobs:frostmaw_spawn`, structure set `frostmaw_spawns`: random spread, spacing 25 chunks, separation 8, at least 7 chunks from `minecraft:villages`; the template (`structure/frostmaw_spawn.nbt`) holds one `mowziesmobs:frostmaw` with `has_crystal` 1 and no `PersistenceRequired` | mowziesmobs |
| The Frostmaw | `mowziesmobs:frostmaw` (`EntityFrostmaw`): 250 health, 10 attack, knockback resistance 1, 4 × 4 blocks (`EntityHandler.java:98`), swipe, slam, ice breath, ice ball | mowziesmobs |
| Its swipe at a passing carriage | the jar's `SWIPE_ANIMATION`, an `AnimationAreaAttackAI` of range 6.5 and arc 135 degrees (`EntityFrostmaw.java:212`), started by `AnimationHandler.INSTANCE.sendAnimationMessage(this, SWIPE_ANIMATION)` as the jar's own AI does (`567`) | mowziesmobs (llibrary `AnimationHandler`) |
| The ice ball that unseats a rider | `EntityIceBall`: 1.6 blocks a tick, aimed ahead of its target from the target's position one tick earlier (`EntityFrostmaw.java:484-501`); hits every living entity within 2 blocks of it for 3 freeze damage and 1.0 freeze progress (`entity/effects/EntityIceBall.java:60-64`); at 1.0, `FrozenData` applies `FROZEN` for 50 ticks (`capability/FrozenData.java:260-261`) and `onFreeze` seats the entity on a new `EntityFrozenController` with `startRiding(controller, true)` (`FrozenData.java:185-199`, called from `ServerEventHandler.java:330`); vanilla `startRiding` calls `stopRiding()` first, which takes the rider off the carriage | mowziesmobs, minecraft |
| Its prize | `mowziesmobs:ice_crystal`: dropped when a player kills it while it holds the crystal, or stolen while it sleeps | mowziesmobs |
| Its warning | sound `mowziesmobs:frostmaw.roar` | mowziesmobs |
| A tribe's home | `mowziesmobs:umvuthana_grove`, structure set `umvuthana_groves` (spacing 25, separation 8, 7 chunks from villages): the Umvuthi on its throne 18 blocks from the fire pit (`UmvuthanaGrovePieces.java:276-288`), 5 to 9 Umvuthana, chests with `mowziesmobs:chests/umvuthana_grove_chest` loot | mowziesmobs |
| Warriors and traders | `mowziesmobs:umvuthana` (`EntityUmvuthanaMinion`): 8 health, 3 attack, a mask, always persistent (`253`), trades gold nuggets with masked players | mowziesmobs |
| The chief's healers | `mowziesmobs:umvuthana_crane`: the Umvuthi summons them each time it loses 45 health; they heal only the Umvuthi and give it Sunblock, under which it takes no damage | mowziesmobs |
| Hunters | `mowziesmobs:umvuthana_raptor` and `umvuthana_follower_raptor`: a raptor leads 4 or 5 followers; the raptor hunts animals and its followers take its target | mowziesmobs |
| The chief | `mowziesmobs:umvuthi` (`EntityUmvuthi`): 150 health, sun abilities, summons Umvuthana; never walks (no movement goal, `318-358`, `tick` zeroes its horizontal motion, `444`, and it has no walk animation); pushes every living entity within 1.2 blocks away each tick (`449-450`); takes 7 gold blocks once from a masked player, then grants Sun's Blessing, and remembers who paid (`TRADED_PLAYERS`, `729-777`); when it dies, every devoted Umvuthana within 30 blocks dies with it (`die`, `859-871`) | mowziesmobs |
| The Umvuthi's travel on an Invasion (MZ-16 (a), Ben, 2026-10-22) | its own body, floated: vanilla `setNoGravity` (a synced flag, `Entity.java:1165-1166`) and `move(MoverType.SELF, …)` (`Entity.java:619`), which collides with blocks, called by the plugin once a tick after the body's own tick, as its `tick` zeroes its horizontal motion before it moves (`EntityUmvuthi.java:444-445`); its height from vanilla's `MOTION_BLOCKING` heightmap, which counts leaves and fluids (`Heightmap.java:144`); with nothing by its feet its legs hang, as on its throne (`checkBlocksByFeet`, `509-516`, `644-671`); its facing turned by a re-pair: a fresh add bundle from a temporary `ServerEntity` (public constructor, `ServerEntity.java:71`; `sendPairingData`, `247-303`) sent with `ServerChunkCache.broadcast` (`482`) to the clients already tracking it, whose `addEntity` drops their old body of that id first (`ClientLevel.java:351-356`), since the jar reads its synced direction only at its first tick (`446-447`), on each client; its motion zeroed and its fall distance reset every tick aloft, as nothing else cancels an upward push with no gravity (`LivingEntity.java:2330-2341`). At the target it settles, and `restrictTo` where it sits as `finalizeSpawn` does for a spawn that is not a structure's (`960-962`); its summons keep to 25 blocks of that centre (`SpawnFollowersAbility`, `997-1060`, `1046`); its boss bar shows to a tracking player once it has that player in line of sight (`MMBossInfoServer.java:40-58`), which vanilla gives only within 128 blocks | minecraft, mowziesmobs |
| The Umvuthi's prize | `mowziesmobs:sol_visage`, dropped when a player kills it (`loot_table/entities/umvuthi.json`, `killed_by_player`) | mowziesmobs |
| Peace with the tribe | an Umvuthana mask on the head (`mowziesmobs:umvuthana_mask_*`): Umvuthana, raptors and the Umvuthi do not target the wearer | mowziesmobs |
| The tribe's warning | sound `mowziesmobs:block.gong` | mowziesmobs |
| Factions, grievance, the ladder, envoys, raids, routs, heirs | rtsim-conflict (`crates/rtsim-conflict`) | mic_rtsim |
| Where trains really run | Create simulates every train wherever it is, loaded or not (`GlobalRailwayManager.tick`, `trains` map, lines 59 and 210); a train's position is `Carriage.getLeadingPoint().getPosition(graph)` (`Carriage.java:385`, `TravellingPoint.java:302`), its dimension the leading point's node location (`TrackNodeLocation.dimension`, `:41`) | create |
| Whose train it is | `Train.owner`, a UUID (`Train.java:162`); who sets it in the hash-matched jar is UNVERIFIED (MZ-13 (a) uses it, P14 the fallback) | create |
| Cargo lost from a train | I24: `attack_train` (`crates/rtsim-industry/src/engine.rs:2962`), which finds the train's current booked leg by its id (`LegIndex::leg_of`, `threat.rs:722`), and `IND_CARRIAGE_DELTA` on `Carriage.storage` | mic_rtsim_create |
| Train arrivals and lines | industry line edges from Create train arrivals (`TrainWatch.java`; `crates/rtsim-industry/src/engine.rs:109-119`, `2100-2111`) | mic_rtsim_create |
| Defending a station or base | the I21 survey: walls 2 blocks or more, fences and walls; Create Radars fire control with Create Big Cannons (Radars class every `Enemy` as hostile, `TrackCategory`: the Frostmaw, every Umvuthana, envoys included, and the Umvuthi); Create saws, drills, crushing wheels; iron golems, which attack every Mowzie body in this plan (`INDUSTRY.md:574-585`) | create, createbigcannons, create radars, minecraft |
| Weapons that hurt a Frostmaw | melee, fire, TaCZ guns (`EntityKineticBullet` extends `Projectile`, not `AbstractArrow`), Create Big Cannons shells (also `Projectile`, with no attacker entity); a thrown trident is an `AbstractArrow` (`ThrownTrident`), so it bounces off like an arrow | tacz, createbigcannons, minecraft |
| Stealing the crystal | vanilla invisibility (`minecraft:potion` of invisibility) | minecraft |
| Keeping a robbed Frostmaw | vanilla `Mob.setPersistenceRequired()`, saved as `PersistenceRequired` | minecraft |
| Puppets: bodies the sim owns while they are away | `BodyAdapter` (`core/body/BodyAdapter.java:16`) and `BodyKinds.registerBody` (`core/body/BodyKinds.java:41`), as dragons and herds use them | mic_rtsim |
| Keeping golems off an envoy | NeoForge `LivingChangeTargetEvent`, which the core already listens to (`core/industry/ThreatEvents.java:32`, `56`) | neoforge |
| Keeping turret shells and stray blows off an envoy | NeoForge `LivingIncomingDamageEvent` (cancelable; not yet used in the core) and `DamageSource.getEntity()`, the attacker or a projectile's owner | neoforge, minecraft |
| Knowing when a camp's bodies are all in | NeoForge `ChunkTicketLevelUpdatedEvent` for each footprint chunk's level (`ChunkMap.java:425`), then `ServerLevel.areEntitiesLoaded(long)` (`ServerLevel.java:1702`): entities load after their chunk, asynchronously (`PersistentEntitySectionManager.java:215-222`, `243-250`), and unload when it drops below FULL, which can happen with no chunk unload and end with no new `ChunkEvent.Load` (§4.9) | neoforge, minecraft |
| Camps found when they first load | the lair finder pattern: a structure start seen on `ChunkEvent.Load` (`neoforge/iceandfire/.../Lairs.java:44-60`, `114-125`) | mic_rtsim_iceandfire (pattern) |
| Telling a camp's bodies apart | an entity attachment, `mic_rtsim:camp_member`, saved with the entity as `mic_rtsim:gossip_ops` is on villagers (`core/puppet/RtsimAttachments.java:36-38`) | mic_rtsim |
| Records that ride on bodies | `mic_rtsim:pending` (`core/puppet/RtsimAttachments.java:30-31`) on a camp body or a `minecraft:marker` (no tick, never sent to a client, `Marker.java:21`, `39`), journaled again at load as `docs/boundary.md` §3.1 does for chests | mic_rtsim, minecraft |
| The journal never ahead of a body's chunk | `ChunkWriteBarrier` (`core/journal/ChunkWriteBarrier.java:62`), extended to entity chunks (ledger 22) | mic_rtsim |
| Tribute put in a real chest | `TRANSFER_TO_CONTAINER` 0x8005 (`docs/boundary.md:286`), as the dragons' `LAIR_CHEST` uses it (`docs/boundary.md:850`, `892-923`) | mic_rtsim |
| A rail fixture for tests | `RailFixtureGameTests` (branch `test/create-rail-fixture`, not merged): real Create 6.0.10 track with a 90 degree curve and an S-bend, two stations, a signal and an assembled two-carriage train on a schedule | mic_rtsim_create |

## 2. Custom-roll ledger

Sizes are estimates by analogy with code that exists; they are not measured.

**Engine (Rust: rtsim-conflict, rtsim-settle and the composition), about 2,200 lines plus tests:**
1. **Rail from sampled trains** (§4.3). `SignalKind::Rail` = 6, appended. The composition decodes
   RAIL_PASS and keeps one decayed leg count per (faction, actor) over all of that actor's passes,
   with a hysteresis band and a daily resync, pushing a rate only on those and booking one wake for
   its next push; the engine subtracts the tolerance once from that one rate. Per-line counts are
   kept beside it only to pick the focus and the ambush line: a pass is credited to its line when
   the train's next arrival names it, and a held pass with no arrival in 1 day (PLACEHOLDER) is let
   go. A new `set_emission_for(faction, key, kind, rate, pos, actor)` pins a source to one faction
   (today `set_emission_inner` finds factions from the source's one position,
   `engine.rs:1337-1424`, so a source can reach only the dens around that point). A source with no
   district keeps a position focus (today `refresh_rates` keeps a focus only with a district,
   `engine.rs:1513-1517`). About 280 lines.
2. **Empty parties and skipped rungs.** A culture may leave a rung's party empty (today
   `Culture::resolve` refuses one, `culture.rs:378-382`). A climb skips empty rungs (Notice goes
   straight to Raid after Notice's dwell), and a step down that lands on an empty rung goes on to the
   next rung below that has a party, floored at Notice. About 60.
3. **Repeat:** a culture field `repeat_days`: a feud sitting on Raid or Invasion with no party out,
   and still at or above that rung's threshold, sends that rung's party again `repeat_days` after
   the last one came home (today only Siege repeats, every `siege_interval_days`,
   `engine.rs:2776`, `2814`). Absent means today's behaviour. About 40.
4. **The chief leads** (§4.5): a culture field `chief_leads` naming the party kinds the chief goes
   with (today any war party, `engine.rs:2265-2271`): the Frostmaw's Raid and Invasion, the Umvuthi's
   Invasion only; and `chief_strength`, its strength in its band's fights (the Umvuthi's 5.0, §4.13;
   none for the Frostmaw, whose chief is its one unit). A chief leads only if its slot is home, with a
   body or owed, and not while the heir waits for its body (§4.10); the Umvuthi leads whether its grove
   is in or out at the muster's end (§4.5). A leading Umvuthi is outside the party's units: it counts in
   no unit, quota or wave count, but `strength` and `strength_by_class` (`engine.rs:2603-2634`) add its
   strength while it leads, as it goes to the fight. Its own stack, with MEMBER_CHIEF
   (`boundary.rs:64`), follows CHIEF_BODY at every embodiment of the band: `seen` (`engine.rs:3602`),
   `start_fight` (`3307-3350`), a later wave (`3485`) and the walk home (ledger 23). How it travels is
   the Mowzie plugin's (ledger 24): the engine has no field and the records no flag for it. About 90.
5. **A chief killed at the camp** (today `chief_killed` takes a party, `engine.rs:4124-4140`): the same
   report, death, succession and cowing as a party chief's, every party out routs home, and the
   **killer's** feud steps down `chief_rout_steps`, floored at Notice; with no killer, or a killer who
   has no feud, no feud steps down (§4.2). A party whose chief is out when the chief's body left at
   home dies (§4.9) loses its chief flag and routs with the rest; a chief killed away keeps today's
   path, CHIEF_KILLED (`engine.rs:3975-3982`) into `chief_killed`. About 90.
6. **A culture of one, and waiting for whole bodies:** a den's body comes back by an arrival, not a
   rate: after a death, or for a den found empty, one booked event brings it after `min(192, 6 × 2^P)`
   days (EQ-4 (b); EQUILIBRIUM.md §2.2's shared rule, in `rtsim-kernel::renewal`: P per den, halving
   every 96 days, plus 1 for each Frostmaw lost to players, read when the wait starts). The wait starts
   only while the pool plus the members away is under the cap, so a den never fields a second Frostmaw
   while the first is out. Today `pool_at` (`engine.rs:1053-1057`) refills the pool from the moment a
   party leaves, up to the cap; the change is there: its refill stops at the cap less the members away.
   A grove's refill at its rate is held the same way, at every camp: a band's survivors always come
   home to their own slots, and none is clamped off (§4.9). A feud that cannot field its party waits
   for `need` = ceil(party size ÷ 2) whole bodies, and for a den, for the arrival. Today `escalate`
   waits for size ÷ 2 (`engine.rs:1671`), 0.5 of a body for the Frostmaw's party of 1, while
   `send_party` needs floor(pool) × 2 ≥ size (`2236-2238`): with the pool between 0.5 and 1 the wait is
   already over, the send fails again, and `plan_feud` books the next tick (`1569`), one wake a tick,
   about 72,000 for a feud over the 3 days a pool at 1/6 a day takes from 0.5 to 1. The Umvuthi's heir
   waits for the later of the cowed time and `min(192, 6 × 2^P_chief)` days (MZ-14 (a)), then for a
   warrior to take up the mask (§4.10). About 90.
7. **A pool cap per camp, and the camp reconciler's engine half** (§4.9). `pool_cap` is per culture
   today (`engine.rs:882`, `1062`); a camp's cap becomes the count of its bodies at FACTION_FOUND (a
   grove's 5 to 9), bounded by the culture's. Nothing at the boundary says a party has left or come
   home today (the commands are Territory, Notice, Envoy, Materialize, Steer, Recall and Settled,
   `boundary.rs:385-393`), and no party reaches its camp in a body (`seen` skips the walk home,
   `engine.rs:3608-3610`; `go_home` recalls, `3106-3110`; ledger 23 gives the walk home bodies where a
   player is). So each camp keeps a ledger: a slot per member, each with a generation and one of five
   states, following the pool under §4.9's invariants. `send_party` draws slots in §4.9's order (today
   it draws a count, `engine.rs:2236-2250`), never more than it may draw: `have` becomes the smaller of
   floor(pool) (`2236`) and the slots at home with no line in flight; and losses away fall on slots.
   The engine marks a camp in and out (CAMP_CENSUS, CAMP_OUT); reconciles each census by §4.9's table;
   orders a muster's end, a homecoming (a hand-back of a band that comes home in bodies, spawns for one
   that comes home with none) and a refill at a camp that is in, as CAMP_RECONCILE lines with derived
   op ids; books CAMP_BODIES and CAMP_DEATH by the world fact each reports; and keeps at most one line
   in flight per slot (draws and picks skip a slot with one), voiding them at an epoch's end and a
   session start. A census never ends a party (§4.16): a party ends through `home` (`engine.rs:3170`:
   survivors to the pool, loot stocked) only at its own last event (`2425`), at its camp, or when its
   last unit dies. The Umvuthi's slot is away only while it leads an Invasion (§4.5): a census
   takes its body left on the throne as any body left behind, under Rule U, and a homecoming at a grove
   that is in hands its puppet back to a body on the throne (§4.5). The ledger keeps the throne spot
   from the grove's first census, and an heir is one reconcile of two lines, a warrior's removal and
   the chief's spawn, seated in place of the old chief's body if that is still on the throne (§4.6):
   the removal frees the warrior's slot, the pool one less, and the spawn fills the chief's. Strays
   fill new slots only while the pool plus the members away is under the cap. About 600.
8. **Lying in wait** (§4.4): a war party at a position with no district waits out its patience (today
   it "finds nothing to raid", `engine.rs:2869-2878`), sends AMBUSH_SPOT, and rolls `ambush_milli` once
   per TRAIN_PASSED; a hit calls `attack_train` for that train id. The spot's passes come from
   sampling, never from `trains_crossed`'s 512-block regions (the real track can sit in a different
   region from the straight booked leg). Whenever a lurk ends (its patience out, the party killed,
   routed or otherwise sent home, or left with no units, §4.9), the engine sends AMBUSH_SPOT again with
   its removed flag; without it Java would go on sampling the spot and sending TRAIN_PASSED for an
   agent that is gone. A recall alone does not end it: `player_left` recalls an embodied party in any
   phase (`engine.rs:4448`), which only despawns the puppet, and the lurk goes on unseen, keeping its
   spot and rolling its ambushes. About 150.
9. **Threat source and class from the culture:** ambushes and district raids use the culture's source
   (`Boss`) and each unit's class and carry (today both hard-code `Bandit`, and ambushes
   `GroundMelee`, `engine.rs:2663-2664`, `2947`). A chief state bit picks between two unit rows (the
   Frostmaw with or without its crystal). About 70.
10. **Loot that is destroyed, not carried home:** a culture flag; the goods leave the books as the
    dragons' eaten food does. About 40.
11. **Hunted animals** (decided, §4.5): a raptor raid that beats a district's defences owes the
    district dead animals, applied when it next loads, under a cap (like I23's destroyed blocks);
    plunder of a village takes its livestock goods, eaten. About 120.
12. **Tribute priced by value**, a real engine change: today `escalate` demands
    `tribute_per_grievance` × grievance units of the tribute good itself (`engine.rs:1680-1683`).
    The rate stays in the market's money (emeralds, 0.5 a point, goblin's) and a demand converts it
    to the culture's tribute good at the haven market's price. The catalogue has no gold good and so
    no gold price (`crates/rtsim-world/src/synth.rs` has none), so a fixed fallback rate (§4.5) carries every
    demand until the grove's market has one. With MZ-6 (a), a `gold_nugget` good in rtsim-world's
    catalogue and the phase 1 item map (`crates/rtsim-jni/src/engine/phase1/catalog.rs:28-38`).
    About 70.
13. **A den without a market:** a culture flag `haven: false` joins the camp as a market-less world
    node (a new `NodeKind::Lair` in `crates/rtsim-settle/src/lib.rs:48`, no stock or market rows),
    because `add_faction` makes every camp a `NodeKind::Haven(market)` node today
    (`engine.rs:854-861`), and a Frostmaw trades nothing. About 60 with the match arms.
14. **Reports the engine has no record for** (§4.12): CAMP_DEATH (a camp body killed or carried off, at
    home or left behind, with its slot and generation, the killer and an op id, once per body UUID,
    §4.5), CHIEF_STATE (latest-wins: health and crystal), CHIEF_PAID (a first payment to an Umvuthi
    body, journaled, with that body's slot and generation: tribute at the camp, settling the payer's
    open demand or else a gift, and an entry in the grove's trade memory only when the slot and
    generation are the live chief's; the memory is kept per grove, 16 bytes a player, cleared at a
    succession, §4.5), CHIEF_BODY (the chief's health, crystal and trade memory, sent to Java ahead of
    a chief's stack or spawn), TRADED (a masked trade at the grove, into `Conflict::traded`,
    `engine.rs:2045`, which has no caller today), a haven's chest (latest-wins, as `LAIR_CHEST`),
    NOTICE_SHOWN (a notice is kept per feud until the plugin has played it, and resent at each session
    start), TRAIN_ROBBED (the train the robbed-train roar plays for), and the world's difficulty (no
    war party leaves on Peaceful). An envoy party wiped out with no player among its killers withdraws
    its demand: a new DEMAND_SETTLED outcome 5 (today 1 to 4, `docs/boundary.md:1461`) with no relief,
    no refusal and no climb, the feud staying on Envoy (today the demand would run to its deadline and
    expire as if refused, `demand_due`, `engine.rs:1809-1821`). About 360 with the decoding and the
    trade memory.
15. **Two cultures as data:** `data/cultures/frostmaw.json` and `umvuthana.json`, added to `builtin()`
    (`culture.rs:430-432`, `449`), each with `market_culture` (`Culture::resolve` errors without it,
    `culture.rs:383-387`), `chief_lifespan_days` 1,000,000 (§4.2) and the fields above. About 140
    lines of JSON.

**Java, about 3,400 lines plus about 1,950 of GameTests and kit:**
16. **ConflictBridge** in the core (MZ-1 (a)): the conflict module's Java half, which does not exist
    yet (`crates/rtsim-jni/src/engine/conflict.rs:5`): territories (kept in a spatial hash the train
    sampler also reads), the territory signals from vanilla block and death events, the envoy and its
    payment, the raid director (waves on an arc), the latest BAND_MARCH per agent and the column it
    lays out for a band met on the march (§4.16), steering, recalls, casualties, notices and their
    acknowledgement, the camp reconciler's plumbing (each footprint chunk's level, epochs and CAMP_OUT,
    applying CAMP_RECONCILE, records on bodies and markers, journaled again at load; §4.9) and
    CHIEF_BODY's routing (§4.5), the `mic_rtsim:camp_member` attachment and its index, the Peaceful
    flag. Vanilla only (D4). About 1,350, by analogy with `core/herd/HerdBridge.java` (940). In the
    Carry On plugin, the pickup hook (`MicRtsimCarryOn.java:39-43`) also takes tagged camp bodies:
    about 20.
17. **The train sampler** in the Create plugin (`neoforge/create`, `TrainSampler.java` beside
    `TrainWatch.java`): once a second it walks `GlobalRailwayManager.trains`, reads each train's
    leading point and tests the segment from its last sample against the territory hash; it reports
    RAIL_PASS and, against AMBUSH_SPOT, TRAIN_PASSED; near a lurking Frostmaw that a player watches it
    samples every tick; it plays the robbed-train roar (TRAIN_ROBBED). About 300.
18. **The Mowzie plugin** `neoforge/mowziesmobs` (mod id `mic_rtsim_mowziesmobs`): body adapters for
    the Frostmaw, the Umvuthana warrior, raptor pack and envoy; camp finding and the census for the two
    structures (the bodies by slot and the strays); a reconcile's lines for the two kinds: adopting and
    tagging strays, taking bodies, handing them to a party's puppets as it sets out, handing a
    homecoming band's puppets back to tagged vanilla bodies where they stand, spawning tagged vanilla
    bodies when a band comes home with none or the pool refills, a Frostmaw's spawn held out of sight,
    and removing (§4.9); restricting a tagged Frostmaw to its den at each join; persistence for a
    robbed Frostmaw (a mixin at the steal, §4.7); the swipe at passing carriages; the envoy party's
    targeting and damage (§4.5); a mixin on `EntityUmvuthi.rememberTrade` that sends CHIEF_PAID with
    the body's slot and generation, and an event on minion trades; the Umvuthi's summons tagged as the
    party's puppets before they join, by a mixin at the jar's `addFreshEntity` call
    (`EntityUmvuthi.java:1050`; §4.5; its travel is ledger 24); reading its trade memory
    (`getTradedPlayers`) and writing it only through `setTradedPlayersCompound`, never `rememberTrade`,
    whose mixin would journal each payer again; Rule U's holds on removing its bodies; an heir seated
    in place of a dead chief's throne body (§4.6); deaths keyed on the body's UUID; and, for a second
    body, its drops cancelled (`LivingDropsEvent`) and a mixin that gives `EntityUmvuthi.die`'s
    `getEntitiesNearby` call an empty list, so it takes no warriors with it (§4.5); the notice sounds.
    About 1,500 to 1,700, by analogy with the Ice and Fire plugin (`DragonBridge.java` 1,267,
    `Lairs.java` 419).
19. **A strength hook for observed fights:** `IND_FIGHT` counts one per attacker today
    (`core/industry/Threats.java:384-385`); plugins supply a per-entity strength (§4.13). About 40.
20. **One core exclusion tag**, `mic_rtsim:threat/ignored`, checked first in `ThreatClasses.hostile`
    (`core/industry/ThreatClasses.java:36-39`), with `mowziesmobs:foliaath` as an optional entry
    (§4.15). About 10 lines and one JSON file.
21. **GameTests and a kit** (`MowzieGameTests`, `MowzieKit`), about 1,600 and 350, by analogy with
    `DragonGameTests.java` (701) and `DragonKit` (329); the rider and swipe tests build on the rail
    fixture; and **`rtsim-cli sim mowzie`**, about 750, by analogy with `sim/conflict.rs` (423).
22. **Entity chunks in the journal barrier** (§4.9). The `ChunkWriteBarrier`
    (`core/journal/ChunkWriteBarrier.java:62`) notes level chunks today (`dependsOn(LevelChunk)`,
    `:170`) and puts them on disk through `ChunkMap.save`; a chunk's entities are stored apart, in
    their own region files through their own IOWorker (`EntityStorage.java:86-108`,
    `SimpleRegionStorage.java:17`, `24`). A camp record needs the entity chunk it speaks for noted the
    same way: saved, put on disk ahead of the queue, and waited for before the journal's file is
    written. How the barrier asks vanilla for one chunk's entities, and whether C2ME moves entity
    storage as it does chunk storage, are UNVERIFIED. About 150.

**Added for the visible march (MZ-15), about 310 lines of engine and 520 of Java:**
23. **The march in the engine** (§4.16): BAND_MARCH, built from the party's route from the segment it
    is on (`route[seg]` and `times[seg]` unchanged, in chart coordinates, as `party_chart_pos` reads
    them, `engine.rs:694-715`), sent at `send_party` (`2224`), at each change of route or phase, on a
    casualty, at the party's end and at each session start; `seen` and `book_unseen` (`3602`, `2576`)
    cover a band walking home; `go_home` (`3103`) steers an embodied band home instead of recalling it,
    and takes its start, `here` (`3104`), from `party_chart_pos`, not from `party_pos`, which folds
    (`2398-2403`): after a seam crossing the folded point would put a whole lap at the front of the
    route home, and `times_for` (`2207-2220`) would give that segment a lap's duration; `start_fight`
    (`3307-3350`) keeps an embodied band's puppets and sends waves only for members not yet in
    Minecraft; a band's survivors keep their slots, by `pool_at` (`1053-1057`), whose refill stops at
    the cap less the members away (ledger 6); and a census never ends a party (ledger 7). `unseen`
    (`3696-3705`) recalls an embodied band only with no player within the distance `book_unseen`
    watches, 192 blocks (`view_radius` plus `party_radius` plus 16): it counts every player at their
    current position, the one the band was seen by included, re-binds the band to the nearest and books
    its `Unseen` again from there. `book_unseen` books from where that player stood (`2576-2596`), and
    `seen` returns early for an embodied band (`3608`), so without that count a player following the
    column would see its Umvuthi recalled. `player_left` (`4435-4456`) re-binds the embodiment the same
    way, and a fighting band still turns to waiting, as today: at `wait_over` (`2797`) a siege fights
    or resolves abstractly, and any other band goes home with no resolve. A siege is recalled at
    `player_left` as today, with no re-bind: `wait_over` would fight or resolve it while embodied and
    then wait a whole `siege_interval_days`, and nothing would recall it, as `unseen` acts only on a
    band that is out (`3700`). `arrive` keeps its watcher within 160 blocks of the target (`watcher`,
    `2837-2850`), as CL-D33 has it: CONFLICT-LIFE.md's model runs every fight but the Frostmaw's
    abstractly (§4.4), and a wider watcher would start fights at targets the ticking area does not
    reach, where no wave comes in and no loss is booked. An embodied band with no watcher but a player
    within 192 blocks of it skips the recall at `2771`, resolves abstractly, the members it loses,
    which BAND_MARCH counts, leaving the column out of every player's sight, and `go_home` steers its
    bodies home; a siege is recalled at its arrival as today, and no chief leads one (ledger 4).
    `lose_units` (`3001-3024`), which `resolve` calls, lowers a party's `units` but not its `spawned`,
    while `casualties` lowers both (`4072-4076`). Today `resolve` runs only after a recall, so that
    never mattered; a band resolved while embodied would lose its abstract dead twice at its next
    recall (6 warriors, 3 killed abstractly: the recall reports 3 survivors of 6 and `3966-3970`
    subtracts the 3 again, leaving none), and a Dematerialized from the bridge (`3955-3963`) would end
    the same. So `lose_units` also lowers the matching `spawned` entry while the party is embodied, as
    `casualties` does. With these, a band with its chief is recalled only with no player within 192
    blocks of it (Rule U, §4.5). The edge is the bridge's (ledger 25). About 310.
24. **The Umvuthi aloft, and the march in Java** (§4.5, §4.16; MZ-16 (a), Ben): its puppet given
    `setNoGravity(true)` at its spawn and moved once a tick, in the bridge's server-tick pass after the
    level's entities have ticked, which skips a body that `isDeadOrDying()`: its motion set to zero and
    its fall distance reset (`resetFallDistance`), so no explosion, wind charge, shell or fan lifts it
    and it never lands as from a fall, then `move(MoverType.SELF, step)` toward its place at the
    column's rear at up to 0.1 blocks a tick, then `restrictTo` where it is, radius -1, so its summons
    keep to it; its target height from the `MOTION_BLOCKING` heightmap over its box's columns and 8
    blocks ahead, every 10 ticks, read only through `getChunkNow`; the stuck rule (a rise of up to 24
    blocks, then a `moveTo` its place once no player within 160 blocks has a clear line to it by the
    plugin's own block `clip`, or after 1,200 ticks); its direction set before its first tick at every
    spawn, and after that turned by a re-pair (an accessor mixin for the server's private `direction`
    field, `setDirection` and its yaw, body yaw and head yaw set, then a temporary `ServerEntity`'s
    pairing packets sent as one bundle with `ServerChunkCache.broadcast` to the clients already tracking
    it; ChunkMap untouched) when its way is more than 55 degrees off, with no active ability, at most
    once in 200 ticks; the trail's particles every 10 ticks, sent long-distance; settling within 32
    blocks of the target with the attack flag, or on a player target it has in sight, never onto a
    fluid, then `noGravity` cleared and `restrictTo` where it sits; rising once the attack flag has
    dropped and it has had no player target for 200 ticks; `noGravity` cleared at its first
    `LivingDeathEvent`, so a body killed aloft falls as it dies; a dying chief's loot dropped if its
    puppet is taken back; `EntityTravelToDimensionEvent` cancelled for every tagged puppet, so no body
    of a band leaves through a portal it drifts or walks into; laying out a band met on the march as a
    column along its route, the chief at its rear; the hand-off of a throne body to its puppet at a
    grove left in view, the puppet facing its route out and rising from the throne; the hand-back of a
    homecoming band's puppets to tagged vanilla bodies, the Umvuthi's at its throne once it has settled
    there. No swim, supernova or repel mixin, no damage cancels and no Carry On rules: a body that
    collides as it moves and is never saved needs none, and the pack's Carry On cannot lift it (ledger
    25). Aloft its box's foot is 2 blocks up, within a player's reach: a player who reaches the column
    can strike it, and a masked player can trade with it as at home, the payment booked as CHIEF_PAID
    (ledger 14) (§4.5). About 330.
25. **One group per stack, in the ConflictBridge** (MZ-17 (a), Ben, 2026-10-22: capture per stack,
    amends C25; §4.16). The ConflictBridge is a session plugin, as `HerdBridge` is, and keeps its
    own groups as HerdBridge does (`HerdBridge.java:70`, its `byAgent` map at `243`), not the core's
    registry, whose only caller is the core's SPAWN_GROUP for caravans (`CommandDispatcher.java:264`);
    the core's join check asks the plugin that owns a tagged agent (`RtsimSession.rejectOnJoin`,
    `975-999`). So the bridge spawns each stack of a band as its own group: a row of two warriors, a
    raptor pack, the chief alone; an envoy is one group, as it spawns at one point. A stack not wholly
    entity-ticking is held pending at its column position from BAND_MARCH, never reported `NOT_TICKING`.
    The ticking test is per stack, as HerdBridge's is per group (`HerdBridge.java:662-672`), and for a
    pending stack it runs only when its answer can change: once a tick the bridge reads the
    entity-ticking state of each chunk a pending stack waits on (`RtsimSession.entitiesTickAt`,
    `419-422`: two hash lookups a chunk, not a member), and tests a stack only when one of its chunks
    has changed or its place has moved into another chunk. Polling is the only way: no NeoForge event
    follows the entity-ticking state, and `ChunkTicketLevelUpdatedEvent` follows the load-level tickets,
    not the simulation distance's tracker (`DistanceManager.java:256-257`). A stack with a member that
    leaves the area goes back to pending with its living members instead of being reported
    Dematerialized. When the band stops to wait or fight, a stack still pending walks in from the
    ticking edge, as members past the spawn quota do (CL-D35, CONFLICT-LIFE.md §2.12): it is placed at
    the entity-ticking point nearest its place in the column, on the route where the route enters the
    area, and walks toward the fight with its steering clamped to one chunk inside the edge, so it holds
    there rather than leave the area; held at the edge itself, it would be taken back as soon as the
    player stepped a chunk away. The clamp ends when the band leaves its stop, so a held stack rejoins
    its column for the walk home and the hand-back at the throne. Without the walk-in, the engine, which
    counts a pending stack as in and sends it no wave, would keep a stack whose place lies outside the
    area out for the whole fight, members in its books that cannot die; without the clamp, a stack
    walking toward a target outside the area would leave it, be taken back and be placed again, a spawn,
    a take and a respawn each time (D1), and at s = 8 an Umvuthi would be seen to go and come back 128
    to 132 blocks off. A fight is watched with a player within 160 blocks of the target (`watcher`,
    `engine.rs:2837-2850`), and at a simulation distance of 8 the target itself ticks only within 128 to
    144 blocks of a player on an axis: with a player 145 to 160 blocks off, the stacks hold at the edge
    and the fight runs to its end with no loss booked unless the player goes to them. That gap is
    CL-D33's as it stands (a bodiless party at a watched target walks in from the ticking edge), not new
    here. The engine keeps one embodiment per party: at a recall one Dematerialized reports every living
    member, in Minecraft or pending, as a survivor, so `engine.rs:3933-3973` is unchanged once
    `lose_units` keeps `spawned` (ledger 23), and today's clearing of the embodiment at a rejected first
    wave (`3924-3931`) does not fire at the edge. The chief's summons join the party's groups, each a
    group of its own holding no unit (§4.5). A puppet that Carry On picks up (through the plugin's
    `take`, as HerdBridge's members are) or that anything but the bridge discards (HerdBridge's
    REMOVED_BY_OTHER, `HerdBridge.java:347-351`) counts as taken (A10), as a herd's member does: it
    leaves the sim, and the bridge books it as BAND_CASUALTIES, with no change to the record
    (`docs/boundary.md:1438`). A take by a known player (`takeFromSim` has the pickup's player,
    `MicRtsimCarryOn.java:42`) is killer kind 1 with that player, a direct act against the faction
    (`1459`), as a herd's TAKEN carries its player; one with no player, a discard by anything but the
    bridge, is kind 3, other, with no killing signal. The Umvuthi's puppet taken so is CHIEF_KILLED with
    the same kind: the chief is gone, its band routs and its heir waits (§4.10). Booked with no player
    when a player took it, `chief_killed` would cow the faction and step the feud down blaming nobody,
    and the heir's line would say its chief died of age (`engine.rs:1142`). Its body, once put down, is
    an ordinary Umvuthi, whose Sol Visage drops when it is killed. With the pack's Carry On settings no
    survival player can pick up a band member (hostile mobs are off, and the Umvuthi, 3.2 blocks tall,
    is over the 2.5-block limit); a creative player can. C25 has a band and its master capture and
    despawn as one unit (`DECISIONS.md:80`); capture by stack amends that (MZ-17 (a), Ben, 2026-10-22). No
    core change and no record change. About 190, in the bridge. The other options (§8, MZ-17): keeping
    C25 for bands, one group that comes in only once wholly ticking and goes whole, costs nothing but
    brings a column's head in up to its length inside the edge, within 128 blocks at s = 8; capture per
    member, also in the bridge, costs about the same and splits a pack or a row at the edge. No option
    touches the core's registry or the herds' and dragons' bridges, which keep their own groups.

No new blocks, items or entities.

**Boundary records**: 17 new kinds, plus a new DEMAND_SETTLED outcome (next to §9 of `docs/boundary.md`;
field layouts are written with the code, and the kinds are provisional: taken when the code lands, after
checking other branches for the same ids):

| Kind | Name | Direction | What it carries |
|---|---|---|---|
| 0x080C | RAIL_PASS | Java to engine | faction, train id, train owner, the sampled point nearest the camp, entry and exit ticks |
| 0x080D | TRAIN_PASSED | Java to engine | agent, epoch, train id, the nearest carriage's index, watched flag |
| 0x080E | CAMP_DEATH | Java to engine | faction, body kind, its slot and generation (none for a stray), chief flag, killer kind (a player, a mob, taken by Carry On, none) and UUID, op id; sent once per body UUID (§4.5); carried on a marker where the body was until acknowledged (§4.9) |
| 0x080F | CAMP_CENSUS | Java to engine | faction, a new epoch, the footprint's bodies of the camp's kinds, each by slot and generation or as a stray by UUID; the camp's tagged bodies loaded elsewhere; chief present, crystal held, health; for the Umvuthi, its position and direction (the throne spot, §4.6), and the players it has traded with, with that body's slot and generation (§4.5) |
| 0x0810 | TRADED | Java to engine | faction, player, gold nuggets that changed hands (the engine values them in coins at the grove market's price, as `Conflict::traded` takes coins), op id |
| 0x0811 | NOTICE_SHOWN | Java to engine | faction, player, the notice's serial (FACTION_NOTICE carries it) |
| 0x0812 | CAMP_BODIES | Java to engine | the answer to a CAMP_RECONCILE: faction, epoch, the reconcile's op id, and per line its slot, generation and outcome (done, nothing to do, stale); carried on the bodies and markers it changed until acknowledged (§4.9) |
| 0x0813 | CAMP_OUT | Java to engine | faction, the epoch that ended: a footprint chunk dropped below FULL (§4.9) |
| 0x0814 | CHIEF_PAID | Java to engine | faction, the paid Umvuthi body's slot and generation (none for a stray), player, op id, the items paid; sent by the `rememberTrade` mixin on a player's first payment to that body (§4.5) |
| 0x0815 | DIFFICULTY (journaled; 0x0093 retired, MZB-17) | Java to engine | the world's difficulty, sent at session start and on change |
| 0x0091 | CHIEF_STATE (latest-wins) | Java to engine | faction, health, crystal held |
| 0x0092 | HAVEN_CHEST (latest-wins) | Java to engine | faction, dimension, a grove chest's position, its free slots (as `LAIR_CHEST`) |
| 0x8807 | AMBUSH_SPOT | engine to Java | agent, epoch, faction, dimension, the track point, the body's spot, reach; flag removed (sent whenever the lurk ends, ledger 8) |
| 0x8808 | CAMP_RECONCILE | engine to Java | faction, epoch, an op id derived from the faction, epoch and decision, and lines: slot, generation, kind, and an action (spawn, adopt a stray by UUID, take, set out with the party's agent and epoch, hand back a party's puppet by its agent and epoch, remove a slot's body or a stray by UUID, seat the heir in place of a dead slot's throne body), chief flag, crystal; for an Umvuthi's spawn, the throne spot and direction (§4.6) (§4.9) |
| 0x8809 | TRAIN_ROBBED | engine to Java | faction, train id |
| 0x880A | CHIEF_BODY | engine to Java | faction, agent and epoch (0 and the camp's epoch at home, else the party's), health, crystal held, the traded players' UUIDs (written with `setTradedPlayersCompound`); sent ahead of a chief's stack, spawn line or hand-back, and to a loaded chief body whose traded set is short (§4.5) |
| 0x880B | BAND_MARCH | engine to Java | agent, revision, faction, culture, party kind, dimension, phase (mustering, out, waiting, fighting, home), flags (the chief leads, ended, joint), the partner contingent's agent for a joint attack (CONFLICT-LIFE.md §2.16), the muster's end tick, unit counts by body kind, waypoints with their ticks from the segment the party is on (`route[seg]` and `times[seg]` unchanged, chart coordinates); sent at each change of a party's march and at each session start (§4.16) |

FACTION_FOUND's reserved word (`docs/boundary.md:1421`) carries the census at discovery: the count
of home bodies, chief present, crystal held.

## 3. What happens today

### 3.1 Mowzie's Mobs in the pack

Mowzie's Mobs 1.8.2 runs with its default config. The pack touches it in one place: Structurify
disables the Wroughtnaut's `wrought_chamber` (`pack/config/structurify.json:405-411`, pack commit
`893bd9f`, "Remove structure causeing crash from mowzie's"). What the jar places and spawns:

| Mob | Where it lives (verified) | What it does today (verified) |
|---|---|---|
| Frostmaw | `frostmaw_spawn` dens in snowy biomes that are not ocean, river, beach, forest or taiga, height 50 to 100, away from villages and outposts (`ConfigHandler.java:443`). In this pack's biome set: 15 biomes, including `frozen_peaks`, `snowy_slopes`, `snowy_plains`, `ice_spikes` and Terralith's `emerald_peaks`, `glacial_chasm`, `rocky_mountains`. How often dens pass the 50 to 100 height check in this pack's terrain is UNVERIFIED. | Sleeps until a player comes within 8 blocks while it has a target (`EntityFrostmaw.java:590-606`); fights; goes back to sleep after 1,200 ticks without a target (`580-589`). Targets players only (`284`). Never wakes on Peaceful (`582`, `600`). Persistent while it holds its crystal (`977`); once robbed it despawns like any monster, as nothing in the den's template sets `PersistenceRequired` (`MowzieEntity.java:291-317`): at once with no player within 128 blocks, and beyond 32 blocks at 1 in 800 a tick once idle for 600 ticks, so within about a minute of the thief walking off. Wanders inside its restriction (`208-209`). |
| Umvuthi | `umvuthana_grove` in the 8 savanna biomes, height 50 to 100 (`ConfigHandler.java:466`) | Placed at the template's `umvuthi` marker, 18 blocks from the fire pit, finalized as a structure spawn, with its restriction centre set to the pit and radius -1 (`UmvuthanaGrovePieces.java:276-288`), which vanilla reads as no restriction (`Mob.java:1325-1326`) but which keeps its summons within 25 blocks of the pit (`EntityUmvuthi.java:1046`). Never walks: it has no movement goal (`318-358`), its `tick` zeroes its horizontal motion (`444`), and a push moves it only up or down (`974-980`). Persistent (`393`). Targets unmasked players, iron golems, zombies, skeletons, Elokosa (`318-345`). In a fight, summons Umvuthana while fewer than 4 are within 50 blocks (`488`), and cranes each time it has lost 45 health (`469-471`; `spawn_healers_threshold`, `ConfigHandler.java:473`). A crane heals only the Umvuthi (`EntityUmvuthanaCrane.java:174`) and gives it Sunblock, under which it takes no damage (`EntityUmvuthi.java:628-632`). Out of a fight it heals 0.3 a tick (`584-594`). When it dies, every devoted Umvuthana within 30 blocks dies within 20 ticks (`die`, `859-871`), by a magic hit with no attacker (`EntityUmvuthana.java:531-535`). |
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
  Killing report that the chief and the heir witness (at `SEVERITY_UNIT` since MZB-36; it was the
  rung code 4, which the knowledge layer read as 4% of a unit); the chief dies and `succeed` runs at
  once: the heir becomes chief and a new heir is named, and for each player the new chief resents,
  0.5 (`heir_grudge_factor`, `tunables.rs:107`) times that resentment is added as a Killing impulse,
  on top of a grievance that is otherwise unchanged (`1091-1134`). The faction is cowed for `cowed_days` × 1000 ÷ the chief's boldness bend. One feud steps down
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
| `umvuthana` | each `mowziesmobs:umvuthana_grove` | the Umvuthi, who leads the Invasion and goes with it (MZ-3 (b), MZ-15, §4.5) | the grove's Umvuthana warriors and raptor packs | `human` (village herds) |

Every value the jar does not give and this section does not set is goblin.json's, labelled
PLACEHOLDER (§4.14). The two cultures share goblin's thresholds `[10, 30, 50, 70, 140, 260]`, dwell
days `[0, 0.5, 0.5, 1, 4, 6, 8]`, hysteresis 0.6, half-life 6 days, patience 2 days, muster 1 day (war
parties only: envoys and scouts set out at once, `engine.rs:2259-2263`), cowed 6 days, march 0.02
blocks a tick (`crates/rtsim-conflict/data/cultures/goblin.json`).

**Territory radius 200 blocks** for both (TUNABLE): half of their structure sets' spacing (25 chunks
× 16 / 2). Each set places at most one start per 400 × 400-block cell, so each has at most 6.25 per
1,000 × 1,000 blocks (fewer where the biome is wrong). Separation 8 chunks lets two dens stand 128
blocks apart, so circles overlap and one line can cross two ranges. `reach_blocks` is 0 for both
(PLACEHOLDER; goblin's is 300): only what happens inside the circle counts.

**Camps are found** as the Ice and Fire plugin finds lairs: when a chunk holding the structure's start
loads and the camp's entities are in, the plugin reports FACTION_FOUND (flag LATE when found after
generation) with the structure from dictionary 4 and the census of the bodies there (§4.9). A camp in a chunk nobody has generated does
not exist yet: a line laid through a range nobody has explored meets no Frostmaw until that den
generates. That is a limit of the design.

**The two `market_culture` values:** `umvuthana` is `human` (its haven sells what its raptors take
to the human market's buyers, C24); `frostmaw` is `human` too, only to satisfy `Culture::resolve`,
with `haven: false` so the den joins as a market-less node (ledger 13).

### 4.2 The Frostmaw: a culture of one

**Who it is.** The den's Frostmaw is the faction's chief and only body. At home it stays the jar's own
entity, saved by vanilla with only the plugin's tag added (§4.9): asleep, holding its crystal, with its
own health. The engine keeps a copy of its state from the census and CHIEF_STATE (health, crystal) for
the times it is away. Asleep it heals 0.3 a tick (`heals_out_of_battle`, on by default,
`ConfigHandler.java:446`; `EntityFrostmaw.java:594-596`), full from nothing in about 830 ticks, so the
engine counts a Frostmaw that has come home as healed.

**Its pool is one.** `pool_cap` 1 (the den holds one Frostmaw, `frostmaw_spawn.nbt`). Its Raid and
Invasion parties are the chief alone: one `frostmaw` unit, flagged as the chief (`BAND_MATERIALIZE`
member stacks carry a chief flag, `docs/boundary.md:1458`). Its Envoy, Scouts and Siege parties are
empty (ledger 2): a beast makes no demands, sends no scouts and breaks no walls.

**What angers it** (decided, MZ-2): Rail (§4.3), with killing and building inside its range from
goblin. It ignores logging and mining (weight 0): nothing in the jar ties it to trees or ore.

**The ladder.**

| Rung | What happens |
|---|---|
| Notice (10) | The plugin plays `mowziesmobs:frostmaw.roar` to the player the feud is against, from 32 blocks (PLACEHOLDER) toward the den, when they are in the range. If they are not, the notice waits in the engine's save until their next entry, across logouts and restarts (NOTICE_SHOWN, ledger 14). |
| Envoy, Scouts | Skipped: a climb from Notice goes straight to Raid after Notice's half-day dwell. |
| Raid (70) | It leaves the den and walks to the focus: a spot beside the real track of the line that carries the most of that player's traffic through the range (§4.3, §4.4). There it lies in wait for its patience (2 days, PLACEHOLDER), ambushing trains that pass. Then it goes home, bored (C24). While the grievance stays at 70 or more it comes again `repeat_days` (4, PLACEHOLDER) after each return (ledger 3). |
| Invasion (140) | After Raid's 4-day dwell, and once no raid is out (a feud climbs only with no party out, `engine.rs:1550`): it waits beside the track at the line's station nearer the den, where every train from or to that station passes it and a district there is raided (§4.6). It repeats like the Raid. |
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
- **The body** (EQ-4 (b), Ben, 2026-10-21). The pool is empty while the den waits for a migrant
  (§4.10), and one booked arrival ends the wait after `min(192, 6 × 2^P)` days, P being the den's
  pressure: it halves every 96 days, a game year, and gains 1 for each Frostmaw lost to players (for a
  boss, any death no wild mob is credited with, EQUILIBRIUM.md §2.2). So a den whose Frostmaw died of
  no player's doing has a new one in 6 days, one killed by a player for the first time in 12, and a den
  farmed as fast as it refills settles near a 52-day wait, a kill every 54 days, about 1.8 Frostmaws a
  game year (§4.10). The plugin places it in the den, asleep and holding its crystal (MZ-4 (a)), the
  next time the den comes in while it is home, once no player is within 160 blocks or after 6,000
  ticks (§4.9).
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
- **The meter: one per den and actor.** Each pass counts against an actor: by MZ-13 (a), the train's
  owner, else P14 at the line's two stations. Each (faction, actor) keeps one decayed count of all that
  actor's passes through the range, whatever line they ran: on each pass n ← n × e^(-Δt/τ) + 1, and its
  rate is n ÷ τ legs a day, with τ = 2 days (PLACEHOLDER). A pass by an owned train counts the moment
  it is reported.
- **Pushing.** The composition pushes the meter's rate into the engine when it leaves a band around
  the last rate pushed (10% of it, and at least 1 leg a day, PLACEHOLDER), and when a day has passed
  since the last push (the resync, below); never while both are under the tolerance, where the
  engine would count nothing anyway. Between passes the rate falls as a closed form, so each meter
  books one wake: the earlier of the band's lower edge and the resync. A meter over the tolerance
  pushes about once a day while its traffic is steady.
- **The source.** One continuous source per (faction, actor), pinned to that faction
  (`set_emission_for`, ledger 1). The engine sums a feud's sources of one kind and subtracts the
  tolerance once from the sum (`refresh_rates`, `engine.rs:1480-1506`); with one source per
  (faction, actor), that sum is the meter, so the tolerance is applied once to all of a player's
  traffic through the range, however many station pairs it is split over.
- **The line: only for the focus.** Beside the meter the composition keeps a decayed count per
  (line, faction, actor), line meaning the unordered station pair. A pass is credited to its line
  when the train's next arrival at a station names the leg it was on (the industry engine already
  pairs a train's departure and arrival into a line edge, `engine.rs:2076-2087`). The line with the
  largest count is the focus: the source's position, sent with each push, is that line's sampled
  point nearest the den; the Raid lies in wait there (§4.4) and the Invasion goes to that line's
  station nearer the den. Line counts make no pushes and book no wakes; they are read when the meter
  pushes. A held pass whose train makes no arrival within 1 day (PLACEHOLDER: a train parked or
  removed mid-line) is let go: an owned one has already counted on its meter and misses only its
  line's count; an ownerless one, which needs the stations for P14, counts against P14 at its own
  sampled point, or against nobody. A den found after the line exists feels the line's next pass:
  the sampler reads the circles as they are now, so there is nothing to re-test.
- **Weight and tolerance** (PLACEHOLDER): weight 1.0 per leg, tolerance 2 legs a day (one round trip)
  per den and actor. With the 6-day half-life (λ = ln 2 / 6 = 0.1155 a day) the grievance settles at
  8.66 × (legs a day - 2).

**Why a daily resync** (decided here; it can be undone). With the band alone, the pushed rate froze
wherever it last entered the band, up to 1 leg a day off, and 1 leg a day is 8.66 grievance points,
the size of Notice: a 4-leg line settled at 11.3 from below and 17 from above. Of the three fixes the
review offered:
- a band floor of 0.25 leg sits inside the meter's own ripple (one pass moves n ÷ τ by 0.5, so the
  rate swings ±0.25 about the true one), and a steady line would push at nearly every pass;
- a band on the rate above the tolerance keeps the 1-leg floor where it does the most harm, at low
  rates (a 4-leg line is only 2 legs over);
- the resync clears the bias from both sides for one push a day per (den, actor), a cost that does
  not grow with the number of lines or trains. What remains is the ripple: the resync reads the
  meter at a random point between passes, at most ±0.25 leg, ±2.2 grievance points.

Days to each rung for traffic started on day 0 and run steadily, against the engine's arithmetic:
the meter, band and resync in the loop; the tolerance subtracted once from the one rate; the
grievance integrated in 1/600-day steps; the Frostmaw's climbs with goblin's dwell (half a day on
Notice before Raid, 4 days on Raid before Invasion) and none while a party is out (`plan_feud`,
`engine.rs:1550`); a raid out 3.6 days (a 1-day muster, 150 blocks each way, a 2-day lurk) and
repeated 4 days after it is home (ledger 3); middling temperament. Each figure is the median of 8
runs with every pass jittered by up to 40% of the headway; perfectly regular trains move a settled
grievance by under 1 point and a crossing by under 0.8 day, except the 4-leg Notice (day 10.0) and
the 24-leg Invasion (below).

| Traffic through the range | Settles at | Notice (10) | Raid (70) | Grievance 140 | Invasion | Pushes in the first 20 days |
|---|---|---|---|---|---|---|
| 4 legs a day | 17.3 | day 11.8 | never | never | never | 19 |
| 12 legs a day | 86.6 | day 3.0 | day 17.1 | never | never | 24 |
| 24 legs a day | 190.4 | day 1.8 | day 6.4 | day 14.1 | day 17.6 | 30 |
| 14 lines at 1.7 legs a day on one trunk (23.8) | 188.7 | day 1.8 | day 6.3 | day 14.2 | day 17.5 (13.8 to 13.9 in 4 runs of 32) | 33 |
| 50 legs a day | 415.5 | day 1.1 | day 3.5 | day 5.8 | day 7.5 (Raid's 4-day dwell) | 37 |

After the first 20 days every row pushes about once a day. At 24 legs a day the grievance passes 140
a few hours after the first repeat raid was sent (day 14.0), so the Invasion waits for that raid to
come home; with perfectly regular trains it passes 140 first, on day 13.9, and climbs then. The
trunk row is the common case of many routes sharing one track through a range: with a meter per line,
each line stayed under its own tolerance and its push gate, and the range felt nothing (grievance
0); with one meter per (den, actor) it feels the trunk's 23.8 legs a day as one line of 23.8 would.

The meter's lag (τ) adds 0.9 to 2.8 days to a crossing against a rate known at once, and 4.3 days to
the 4-leg Notice, where the rate first has to climb past the tolerance.

**Running fewer trains works, on a timescale of days.** A line run at 12 legs a day from day 0 and 24
from day 2 (walkthrough A) peaks at a grievance of about 122 on day 13 if traffic changes on day 12:

| From day 12 | Grievance under 70 (no more raids) | Under 42 (Raid rung left) | Settles at |
|---|---|---|---|
| cut to 4 legs a day | day 21.4 | day 28.1 | 17.3 (Notice: a roar now and then) |
| stopped | day 19.2 | day 23.6 | 0 (under 6, the Notice rung left, day 40.5) |

In both, the repeat raid due on day 15.1 still comes: the grievance is over 70 then.

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
swipe's 6.5 plus half a carriage), and the same record with its removed flag whenever the lurk ends,
for any reason (ledger 8).

**Unwatched.** While its party waits out its patience, the sampler reports each train whose sampled
segment passes within the reach of the track point (TRAIN_PASSED). Each pass rolls `ambush_milli` (500
in 1,000). A hit is an I24 attack on that train, found by its id (`attack_train`, which takes the
train's current booked leg, `threat.rs:722`): source `Boss`, its class, strength 41.7 (§4.13). The
industry engine paces it: one attack per train per class per day (`cooldown_ticks` 24,000,
`crates/rtsim-industry/src/tunables.rs:184`). The cargo lost is at most the Frostmaw's carry,
PLACEHOLDER 64 (industry's own siege carry, `tunables.rs:180`), and at most 10% of the leg's cargo
(`theft_cap_ppm` 100,000, `181`). It is destroyed, not carried off (ledger 10; MZ-5 (a)). The loss
reaches the real carriage as `IND_CARRIAGE_DELTA` wherever the train is
(`docs/industry-boundary.md:216`, `263-265`). A train with no booked leg (one not run between declared
stations) has no cargo on the books and loses nothing.

So two trains passing a waiting Frostmaw lose at most 64 items each per day: at most 256 items over a
two-day wait. Track is never cut (I24).

**The cue.** The engine sends TRAIN_ROBBED; when that train next arrives at a station with a player
within 64 blocks (PLACEHOLDER), the plugin plays the roar there, from 32 blocks toward the den. The
player who finds a short train has heard why.

**Watched.** A player within view (`view_radius` 160, `tunables.rs:91`) of a waiting Frostmaw sees the
real one at its spot, a puppet with `alwaysActive` set so it stays awake (`EntityFrostmaw.java:361-363`,
`580`), restricted to its spot so its `MoveTowardsRestrictionGoal` holds it there (`208`). It targets
players only (`284`).
- **The swipe.** While its puppet is in the world the sampler samples trains within 64 blocks of it
  every tick. When a carriage comes within 6.5 blocks, the plugin starts the jar's swipe
  (`AnimationHandler.INSTANCE.sendAnimationMessage(frostmaw, SWIPE_ANIMATION)`, as the jar's AI does,
  `567`), turned toward the carriage. It is the jar's own area attack, so a rider within its arc is
  struck too (which entities it hits is settled by test 9). The cargo comes from the same TRAIN_PASSED
  and roll as an unwatched pass, so a watched line loses what an unwatched one would (P28, MZ-5 (a)):
  the delta lands within a step of the swipe, whether or not the carriage is still in view.
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

**Its den while it is out.** If it set out while the den was in, its body went with it (§4.9): handed
to its puppet if a player saw it go, else taken. A player who visits finds the ruin and no Frostmaw. If
it set out while the den was out, its vanilla body is still in the saved den, and the den's next census
takes it, whoever is near (§4.9), while the Frostmaw goes on with its raid or its lurk: a census never
ends a march (MZ-15, §4.16). So a player who visits the den while its Frostmaw waits by her line finds
the den empty, and the lurk goes on, with its ambushes; a rider who comes along the line meets it
there, awake, wherever the den is. Reason: until MZ-15 a census ended the party through `home` unless a
player was nearer the party than the den, so a visit to the den cut a lurk short, and the Frostmaw on
the road, or in the far view, vanished. This has the uninstall exposure of any member away whose body
is gone, which §4.9 lists: an uninstall while the Frostmaw is out with its body taken leaves the den
empty for good.

### 4.5 The Umvuthana: a tribe

**Who it is.** The grove's Umvuthana are the tribe's warriors (MZ-11 (a), Ben, 2026-10-21). At home they
stay the jar's own entities, saved by vanilla with only the plugin's tag added (§4.9). At the grove's
first census the plugin counts them, and that count is both the pool and the grove's own `pool_cap` (5
to 9; ledger 7). At home the Umvuthi is a vanilla entity on its throne (persistent,
`EntityUmvuthi.java:393`), keeping its own memory of who traded with it (`TRADED_PLAYERS`, NBT
`"players"`, `779-793`), which the engine also keeps; it goes with its tribe's Invasion, borne aloft at
its rear (below). Umvuthana it summons in a fight at home are vanilla minions like the rest: strays,
which the census adopts only while the pool plus the members away is under the grove's cap (§4.9). Its
cranes at home stay vanilla and are never counted: a crane heals and shields only the Umvuthi
(`EntityUmvuthanaCrane.java:174`), so it has no place in a band away from it. The pool is a count of
fighters, and a raptor pack counts as one. At home every pool unit is a warrior's body: a war party's
raptor pack holds one warrior's slot, so setting out from a grove that is in it takes that warrior's
body, and coming home it is owed one again (§4.9). A scouting raptor pack draws on no pool: it takes no
warrior's body and turns back into none.

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
- invasion: 6 warriors and 2 raptor packs (8 units, strength 7.6), led by the Umvuthi going with it,
  outside the units, its strength of 5.0 added (12.6, below); a grove of 5 sends 4, 3 warriors and 1
  pack (`send_party` floors each unit's share, 5 ÷ 8 of each, `engine.rs:2240-2247`);
- siege: empty, so Invasion is the top rung: the tribe breaks no blocks, so it cannot besiege;
- plunder: 1 raptor pack, against village herds.

**What angers it** (goblin's weights, PLACEHOLDER): killing its kin inside the territory (any
`mowziesmobs:umvuthana*` entity, natural raptor packs included), logging, mining, building, smog.

**The envoy party** (decided). While a demand is open, the plugin strips the target goals of all
three bodies of the envoy party, cancels any golem's target change onto them
(`LivingChangeTargetEvent`, which the core already handles, `ThreatEvents.java:32`, `56`), and
cancels every hit on them (`LivingIncomingDamageEvent`) unless a player is the attacker or the
projectile's owner (`DamageSource.getEntity()`), or the damage bypasses invulnerability (the void,
`/kill`). So a Create Radars turret, which classes every Umvuthana as hostile and fires on it
(`TrackCategory`: any `Enemy`), a golem's blow, a zombie, a skeleton's arrow, lava and a fall do
nothing to them. If a player strikes the envoy or a guard, all three get their target goals back and
the hit is an insult (ENVOY_ANSWER 3). A party wiped out by anything else (only damage that
bypasses invulnerability can) is casualties with killer kind mob, neither an insult nor a refusal:
the demand is withdrawn (ledger 14) and the feud stays on Envoy. Reason: nearly every base has an
iron golem, and a late base has radar-guided cannons (§5 A recommends one), and every Umvuthana is
an `Enemy` that both attack (`IronGolem.java:78`); without this, the base's own defences would turn
every demand into a war the player never chose, and the guards would attack every unmasked player
at the base before a word was said.

**The ladder.**

| Rung | What happens |
|---|---|
| Notice (10) | `mowziesmobs:block.gong` plays to the player, as the Frostmaw's roar does, and waits for their next entry the same way. |
| Envoy (30) | An envoy party walks to the player's base (P26). The envoy's custom name shows the demand: at the Envoy threshold, 0.5 × 30 = 15 emeralds' worth, which is 135 gold nuggets at the fallback rate (below), within 3 days (goblin's `demand_days`, middling temperament). Right-clicking the envoy with the gold in hand pays (TRIBUTE_PAID); hitting it is an insult. A masked player's right-click opens the minion's own trade screen (`EntityUmvuthanaMinion.java:229-240`), so the bridge takes the payment first. |
| Scouts (50) | A raptor pack watches the base from the edge for `linger_days`. |
| Raid (70) | A war band comes for the player's base. |
| Invasion (140) | The pool's whole strength comes. |

**Tribute** is gold (MZ-6 (a), Ben, 2026-10-21), priced by value (ledger 12): the culture asks 0.5
emerald's worth per point of grievance (goblin's), converted at the grove market's price for gold
nuggets, or at a fallback of 9 nuggets an emerald (PLACEHOLDER: one gold ingot an emerald) until the
market has a price. The catalogue has no gold good today, so no market has one, and the fallback
carries every demand until the gold good and its price exist (ledger 12). Gold nuggets are the tribe's
trading currency on screen (a masked player buys clawed logs, cooked meat, feathers, sticks and
campfires from the grove's warriors with gold nuggets, and sells them melon, raw meat, beetroot, fish,
acacia saplings and bones for nuggets, one offer at a time, `EntityUmvuthanaMinion.java:107`; the
player's item goes in, `InventoryUmvuthana.java:97-105`). Paid tribute is stocked at the grove's haven
(C24) and the plugin puts it into the grove's real chests when the grove loads (a haven chest, ledger
14, as `LAIR_CHEST`): a player who later raids the grove finds the gold they paid. Paying the Umvuthi
its 7 gold blocks (`ConfigHandler.java:470-471`) is reported as tribute at the camp (CHIEF_PAID, ledger
14): it settles the payer's open demand, or with none open counts as a gift of 567 nuggets, 63
emeralds' worth at the fallback rate. Trading with the grove's minions while masked (TRADED) softens
the trader's feud at a tenth of a gift's rate (`Conflict::traded`, `engine.rs:2045-2052`).

**Masks are honoured** (MZ-4 (a)): the jar's targeting skips a player wearing an Umvuthana mask
(`EntityUmvuthana.java:347-356`; the Umvuthi and minions also target the player who misbehaved,
`EntityUmvuthi.java:322-330`, `EntityUmvuthanaMinion.java:133-139`). A masked player at a watched raid
is not attacked; the band still fights the iron golems, which attack it. Masks drop from Umvuthana (one
in three, for example `loot_table/entities/umvuthana_rage.json`) and lie in grove chests.

**What a tribe raid does.**
- **Watched:** the real band walks in as the column a player met on its march (§4.16); one that reaches
  the base with no bodies, as for a player who logs in or teleports there, arrives in waves on the edge
  of view (`spawn_edge_blocks` 112, quota 16, `tunables.rs:92-94`). Warriors attack unmasked players;
  raptor packs hunt the base's animals (jar-true), which flee Umvuthana within 6 blocks
  (`ServerEventHandler.java:239-244`). Villagers flee them too (`246-250`).
- **Unwatched:** Lanchester against the district's defences, class `ground_melee`: walls stop it
  (I22). A raid that wins steals nothing and breaks nothing (carry and breaks 0), but its raptor packs
  owe the district dead animals (decided, MZ-8; ledger 11): when the district next loads, up to 2
  animals per raptor pack (PLACEHOLDER; measure in a GameTest: animals a raptor pack kills in a
  watched raid of the same length) matching the jar's hunt rule are killed where they stand, so their
  drops fall.
- **Plunder of a village:** a raptor pack takes livestock goods (`leather`, `wool`) from the village's
  stock, up to `loot_share_milli` (150 in 1,000, goblin's), eaten: the village has less to sell. Breaking
  a plundering pack makes the village it marched on, and any village that hates the tribe within
  the view radius (160 blocks) of where the pack broke, think better of the player: a rescue report
  they witness first-hand, as does a player base within that radius (CONFLICT-LIFE.md §7, CL-D61).
  Every other village hears it from travellers, at hearsay weight; a base tells them.

**MZ-8, decided: raptor hunts are owed and plunder is eaten.** Reason: C16 to C18 already require
attacks to land while the owner is away; a tribe raid that did nothing unwatched would break them.

**The Umvuthi leads the Invasion, borne by its sun above its band** (MZ-3 (b), Ben, 2026-10-21; MZ-15,
Ben, 2026-10-21; how it travels is MZ-16's second round, (a), Ben, 2026-10-22, §8). Ben: "I
feel like it should be the grove / whoever they send 'walking' towards the player instead of it just
not working." So the Umvuthi marches with its warriors, from the grove to the target and back, where a
player can see it, and the far view can draw it the whole way (§4.16). It rides nothing: Ben rejected
the horse and every other animal mount (MZ-16, 2026-10-22): "Given the current models for umuthi
(sunbirds), them riding horses / most other animals doesn't make too much sense."
- **What its body can do** (the jar). It cannot walk: it has no movement goal
  (`EntityUmvuthi.java:318-358`: target goals, its abilities at priorities 1, 2 and 6, and looking
  about), its `tick` zeroes its horizontal motion before it moves (`444-445`) and turns it to one of
  four directions (`452-453`), and its animations have no walk, flight or leap
  (`umvuthi.animation.json`: belly drum, bless, blink, death, flare, hurt, idle, mask twitch, roar,
  solar beam, spawn strix, sun strike, supernova, talk); its model has no wings (`umvuthi.geo.json`, 118
  bones). On the ground nothing else moves it: it is not pushable (`974-976`), a push keeps only its
  vertical part (`978-980`), fluids do not push it (`970-972`), and it takes no fall damage (`966-968`).
  With no gravity a push upward would last: vanilla's `travel` only shrinks the vertical speed by 0.98 a
  tick (`LivingEntity.java:2330-2341`), and an explosion pushes through its own resistance attribute,
  which the jar leaves at 0 (`Explosion.java:294`, `303-304`), so a creeper below would lift it about 30
  blocks; wind charges, shells and Create's fans would too. Aloft the plugin zeroes its motion every
  tick (below). It is a seated figure that never stands. Every 20 ticks it checks the two blocks
  diagonally in front of its feet, one block down (`checkBlocksByFeet`, `644-671`, called at `509-511`):
  with either solid it lifts its legs, and with neither they hang (`legsUp`, `512-516`; the jar's client
  model, `com/bobmowzie/mowziesmobs/client/model/entity/ModelUmvuthi.java`, lifts and spreads the thighs
  by `legsUp`, `46-52`, and sways the legs only while they hang, `97-106`). On its throne they hang: in
  the jar's template (`structure/umvuthana/umvuthi_throne.nbt`, 19 × 17 × 25 blocks) the marker it is
  placed at stands on a platform of acacia slabs (template y 7) with air at both those blocks, the
  backrest behind it, and air above it to template y 13; above that the template is structure void,
  which keeps whatever the world had there. Its head carries its sun: on clients a glow every 10 ticks
  and a wisp of smoke (`454-467`). None of its abilities reads whether it stands on the ground:
  `EntityUmvuthi` and `MowzieEntity` read `onGround` only to keep `prevOnGround` and `willLandSoon`
  (`MowzieEntity.java:333`, `344`).
- **How it travels: aloft.** Its puppet floats seated 2 blocks (PLACEHOLDER) above the ground at its
  place at the rear of the column (§4.16), its legs hanging as on its throne and its head-sun lit. The
  plugin sets `setNoGravity(true)` on the puppet (a synced vanilla flag, `Entity.java:1165-1166`) and
  moves it once a tick, after the level's entities have ticked, with `move(MoverType.SELF, step)`
  (`Entity.java:619`), which collides with blocks: the jar's `tick` wipes any horizontal motion set
  before it (`444`). Before each step the pass sets the body's motion to zero, so no push lifts it
  (above), and resets its fall distance (`resetFallDistance`): `move` counts the distance it sinks
  (`Entity.java:619-671`), and a body that settled after a stuck rise of more than 3 blocks would land
  as from a fall, trampling farmland and turtle eggs and throwing landing particles (`1192-1208`;
  `LivingEntity.java:346-371`). After the step it calls `restrictTo` where the body is, radius -1:
  `finalizeSpawn` restricts it once, where it spawned (`EntityUmvuthi.java:960-962`), and its summons
  take their centre from it (`1046`), so without this they would keep to the spot where its band was
  met. The pass skips a body that `isDeadOrDying()`: a dying body is neither stepped nor re-paired (its
  death animation runs 115 ticks, `DIE_ABILITY`, `210`; `MowzieEntity.tickDeath`, `453-467`). A puppet
  is never saved (the AgentTag), so no saved Umvuthi ever floats: every body on a throne is the jar's,
  with gravity. The ConflictBridge cancels `EntityTravelToDimensionEvent` for every tagged puppet of a
  band, so a body that drifts or walks into a player's nether portal, which has no collision, stays in
  its dimension (UNVERIFIED that the event fires for a mob's portal use in 1.21.1, test 31); whether L2
  Hostility's traits that move a mob, Teleport among them, reach a puppet is UNVERIFIED (test 31). The
  plugin handles five things:
  - **Its height.** Its target height is 2 blocks above the highest `MOTION_BLOCKING` heightmap value
    (`Heightmap.java:144`: any block that blocks motion, leaves among them, or holds a fluid) over the 4
    to 9 columns under its box and over the 8 blocks (PLACEHOLDER) ahead along its way, recomputed every
    10 ticks (PLACEHOLDER) and read only from chunks already loaded (`ServerChunkCache.getChunkNow`; a
    column whose chunk is not at hand is skipped, never loaded). It rises at up to 0.1 blocks a tick and
    sinks at up to 0.05 (PLACEHOLDER). So it lifts over a tree's crown, a wall or a roof in its path,
    and floats 2 blocks over a river's surface, never in it: it needs no swim and no drowning cancelled.
  - **Its pace.** It moves toward its place at up to 0.1 blocks a tick (PLACEHOLDER), five times the
    march's 0.02, so it keeps to its place and catches up when held back. `Entity.move` clips each axis
    on its own, so a step that a block stops still makes its other axes: it slides along a wall while it
    rises over it, and under a canopy, whose leaves the heightmap counts, its box presses against the
    underside of the leaves as it moves on, and it rises once it is past them. Its box never enters a
    block, so it takes no suffocation damage and none is cancelled.
  - **Getting stuck.** If it has made no headway toward its place for 100 ticks while more than 8 blocks
    from it (PLACEHOLDER), its target height rises, by up to 24 blocks (PLACEHOLDER); if it is still
    held 100 ticks later, the plugin moves it to its place, at its height there, the first tick no
    player within 160 blocks has a clear line to it, by the plugin's own block clip from each player's
    eyes to the body (vanilla's `hasLineOfSight` gives up past 128 blocks, `LivingEntity.java:3039`, so
    a player 128 to 160 blocks off would see the jump), or after 1,200 ticks (PLACEHOLDER).
  - **Its facing: a re-pair.** Its `tick` copies its synced direction into its private `direction` field
    (`284`) only at `tickCount` 1 (`446-447`; `setDirection`, `701`, writes only the synced value), then
    sets its yaw and body yaw from that field every tick, on the server and on each client (`452-453`).
    So a new direction shows only on a client that makes its body after it is set: without help a
    floating Umvuthi keeps its first facing at every turn of its route, sideways or backwards, and at
    the fight its head, turned on its target, twists up to 140 degrees from its body. The plugin sets
    its direction before its first tick at every spawn, to the quarter facing its way (after
    `finalizeSpawn`, which may turn it toward a player holding its spawn egg, `946-958`). After that it
    turns it by a re-pair, in the pass after the entity tick. It sets the synced direction, through an
    accessor mixin the server's own field, and the body's yaw, body yaw and head yaw to the new quarter:
    the entity tick has just set them from the old field (`452-453`), and the add packet carries the
    server's rotation (`ClientboundAddEntityPacket.java:37-52`), so without this each client would show
    the old facing for a tick. It then builds a temporary `ServerEntity` for the body (a public
    constructor, `ServerEntity.java:71`), collects its pairing packets (`sendPairingData`, `247-303`:
    the add packet and the body's non-default synced data, the new direction among it; the jar writes no
    spawn data of its own, `MowzieEntity.java:371-372`) into one `ClientboundBundlePacket`, and sends it
    with `ServerChunkCache.broadcast` (`482`), which reaches only the players already tracking the body.
    Each of those clients' `addEntity` drops its old body of that id first (`ClientLevel.java:351-356`)
    and adds a fresh one, whose first tick reads the new direction. Nothing changes on the server:
    ChunkMap keeps its tracker, no start or stop tracking event fires (`ServerEntity.java:236`, `244`),
    the boss bar keeps its players, and the body keeps its id, UUID, health, effects, ability timers,
    target and kill credit; no join, leave or death event fires on the server, and nothing is saved or
    loaded. On each client NeoForge fires `EntityJoinLevelEvent` for the fresh body
    (`ClientLevel.java:352`). The boss music fades and is taken up again by the fresh client body at the
    next event 67 (`MowzieEntity.java:345-350`), which the server broadcasts each tick, without
    restarting the track. A fresh client body starts with its legs up (`blocksByFeet` true,
    `EntityUmvuthi.java:285`, and the `legsUp` timer at 0, `280`) until its first feet check at a
    `tickCount` that is a multiple of 20 (`509-516`): aloft, its legs lift and drop again within about
    30 ticks after each re-pair and at each first sight, a twitch test 31 records (hiding it would take
    a client mod). A re-pair runs when its way (along its route while it marches, toward its target
    while it fights) is more than 55 degrees from its facing (PLACEHOLDER: 10 degrees past the 45 at
    which the nearest quarter changes), with no active ability, at most once in 200 ticks (PLACEHOLDER).
    Whether a client draws the swap of one id's body in one tick without a flicker is UNVERIFIED (test
    31). Rejected: removing and re-adding its tracker (`ServerChunkCache.removeEntity` and `addEntity`,
    `470-476`), the first form of this design, which crashes the server when the body's section is not
    tracked at the re-pair and turns tracked later (`addEntity` makes a tracker that `onTrackingStart`
    then meets: "Entity is already tracked!", `ServerLevel.java:1766-1767`, `ChunkMap.java:1110`) and
    leaks a tracker the other way round; a `tick` mixin on clients, which would make mic-rtsim a client
    mod with one required payload channel (today the pack has no mic-rtsim on clients, and shipping
    `mic_native` to every client would be Ben's call); a fresh puppet at each turn (the first round's
    swap, which had to copy the jar's save, `healthLost`, the ability timers and the kill credit into it
    and remove the old body first); and the stale facing.
  - **A trail** (PLACEHOLDER: on). Every 10 ticks the server sends a few vanilla `minecraft:small_flame`
    particles under it to each player tracking it (`ServerLevel.sendParticles`, a vanilla packet, so no
    client mod), with long distance set: the plain call reaches only players within 32 blocks, the
    long-distance one 512 (`ServerLevel.java:1258-1300`), and the body is drawn to 132; so it reads as
    borne by its sun's fire. Test 31's screenshots, with it and without, settle whether it stays.

  Why aloft: the jar's body is a seated figure whose legs hang when nothing is by its feet, so lifted 2
  blocks it reads as it does on its throne, carried by its own sun, with no new entity, pose or
  animation (D3). The first round rejected a glide on the ground, a seated body sliding over the grass;
  aloft is the same motion where no one expects a seated body to be standing, and whether it reads as
  carried and not slid is test 31's screenshot (UNVERIFIED until it runs). Its move collides, so it
  needs none of the horse's guards: no swim, no mixin to keep its own supernova or its push off a mount,
  no suffocation or drowning to cancel, and no Carry On rules (the pack's Carry On cannot lift it,
  ledger 25). Aloft its box's foot is 2 blocks up, within a player's reach of 3: a player who reaches
  the column can strike it, and a masked player can trade with it as at home (`mobInteract`, `906-912`),
  the payment booked as CHIEF_PAID as any other is (below). It crosses a river, a wall or a wood that
  its warriors must go around or through.
- **The options** (MZ-16's second round, §8; each without an animal, D3 kept):
  - (a) **aloft**, above: recommended.
  - (b) **sun steps:** it blinks from one of its own sunstrikes to the next. `EntitySunstrike` needs
    open sky above its block or is discarded (`entity/effects/EntitySunstrike.java:194-195`), and is
    discarded too when the block under it changes (`208-209`); it strikes 35 ticks after it lands (`84`)
    and hits every entity within 3 blocks of it that is not an item twice, once as a mob's projectile
    and once as fire, each for 2 × the damage multiplier, and sets it alight for 3 seconds (`223-243`);
    only an entity that is `LeaderSunstrikeImmune`, the Umvuthi and its band, is spared. So it would not
    move under a canopy, would hit golems, villagers, animals, armour stands, carts and boats on its
    road unless a damage mixin stopped it, and would jump ahead of a column that walks; and a strike is
    a saved entity (`290-295`) that lingers up to 1,243 ticks (`86`, `194`), so one in a chunk that
    unloads outlives rtsim (UR-1).
  - (b') **particle sun steps:** the same blinks with no strike entity: the plugin moves the body from
    step to step and draws each strike with vanilla particles (`sendParticles`), so nothing is hit and
    nothing is saved, D3 kept with no client mod. It still blinks rather than travels, runs ahead of a
    column that walks, and a strike drawn without the jar's beam and sound reads as a copy of one.
  - (c) **bounds:** flung in arcs as its summons are (`EntityUmvuthi.java:1051`): a seated body thrown
    with no leap animation, which reads as a physics bug.
  - (d) **carried unseen:** placed at the fight in a sunstrike. Against MZ-15: it is not seen coming,
    and the far view has no chief to draw; it is the war seat again (MZ-3's first form: a puppet placed
    behind the band's first wave, DECISIONS.md).
  - (e) **it stays home** and a stand-in leads: MZ-3 (a), which Ben did not choose.

  Also rejected: a walk (a mixin that lets it walk: no walk animation, four facings, a slide); a litter
  or palanquin, or a Create contraption (a new entity or block, D3; a contraption also needs track or a
  structure the grove does not have); the cranes' teleport (`UmvuthanaTeleportAbility`,
  `EntityUmvuthana.java:984-1035`, belongs to `EntityUmvuthana`, which the Umvuthi is not); and any
  animal mount (MZ-16, Ben).
- **Who leads.** The Invasion only (`chief_leads`, ledger 4): an envoy, scouts, a raid and plunder go
  without it. It goes whether its grove is in or out when the muster ends, and not while its heir waits
  for a body (§4.10). It holds no unit and counts in no quota or wave; now that it goes to the fight,
  its strength (5.0, §4.13) counts in the band's, as the conflict-life design has it (CONFLICT-LIFE.md
  §2.7). An Invasion is 8 units (16 bodies: 6 warriors and 2 packs of 5), under the spawn quota of 16
  units (`tunables.rs:94`), so a Mowzie band is never cut down when it is met (§4.16). A joint attack
  shares one quota of 16 bodies for both contingents, a pack counting as its 5 (CONFLICT-LIFE.md §2.16,
  CL-D49): an invasion beside a goblin band shows its share at first, at least one stack, and walks the
  rest in from the ticking edge as the quota frees.
- **Leaving a grove that is in.** At the muster's end the set-out line covers its slot too (§4.9). If
  the band is seen as it sets out, its throne body is handed to its puppet where it sits, the puppet
  facing the quarter of its route out (a hand-off is a new body, so it turns as it is replaced), and
  it rises straight up out of the throne, past the backrest, to its height, and floats out over the
  grove behind its warriors to its place at the column's rear. In the template nothing is above the
  throne; a tree or a player's block over it is met by the stuck rule (above). A player at the grove
  sees the warriors file out and the Umvuthi lift from its throne and follow them through the air. If
  the band is not seen, the throne body is taken under Rule U, and the engine carries the Umvuthi
  until the band is met.
- **Leaving a grove that is out.** Its body stays on the throne in the saved grove (its slot away,
  left), and the grove's next census takes it (§4.9). Wherever the band is met, the Umvuthi is there,
  aloft at its rear.
- **When it has a body.** Wherever the band has bodies: met on the road (`seen`, `engine.rs:3602`), at
  a watched fight (`start_fight`, `3307-3350`), at a later wave (`3485`) and walking home (§4.16). Each
  time the engine sends CHIEF_BODY and then the chief's own stack, with MEMBER_CHIEF (`boundary.rs:64`),
  in the band's first BAND_MATERIALIZE. Its puppet is made as an heir is (§4.6), but with
  `finalizeSpawn` and `MobSpawnType.EVENT`, then its direction, `setNoGravity(true)`, its health and
  trade memory from CHIEF_BODY, and the AgentTag, and it is placed at its place in the column at its
  height; its health is nearly always full, as out of a fight it heals 0.3 a tick, from nothing in
  about 575 ticks (§3.1). How it travels is the Mowzie plugin's, for the one body kind it knows floats:
  the engine and the records carry no flag for it. A client tracks it from 160 blocks
  (`EntityHandler.java:97`, capped by the view distance, `ChunkMap.java:1331`), and its boss bar shows
  to a tracking player once it has that player in line of sight (`MowzieEntity.startSeenByPlayer`,
  `527-529`; `MMBossInfoServer.java:40-58`), which vanilla gives only within 128 blocks
  (`LivingEntity.java:3033-3041`).
- **At the fight.** When the band's steer carries its attack flag and the Umvuthi is within 32 blocks of
  the target (PLACEHOLDER), or when it has a player target in sight on the road, it settles: it sinks at
  0.05 blocks a tick, with collision, onto whatever is under it (the ground, a roof, a wall's top or a
  tree's crown, where its body sits as on any block), and on landing the plugin clears `noGravity` and
  calls `restrictTo` where it sits, radius -1, as `finalizeSpawn` does for a spawn that is not a
  structure's (`EntityUmvuthi.java:960-962`), so its summons keep to 25 blocks of it (`1046`). It never
  sinks onto a fluid: while the top block under its box holds one, it floats on toward its place and
  settles at the first ground; with the attack flag set and within 8 blocks of the target still over a
  fluid (a base built on water), it fights where it floats, restricted there. From there it fights as at
  home: it takes targets within its follow range of 40 blocks (`360-362`), summons warriors while fewer
  than 4 are within 50 blocks and cranes each time it has lost 45 health (`469-471`, `488-490`), and a
  masked player it does not target can trade with it (`mobInteract`, `906-912`). Any other target on the
  road (a golem, a zombie or a skeleton: its goals for those need no sight,
  `EntityUmvuthi.java:346-348`, and its player goal ignores sight, `334`, which is why the road rule
  asks for sight itself) it fights aloft as it floats on with its column: none of its abilities needs
  the ground (above), and its summons keep to it, its restriction moving with it. Settling for any
  target would set it down at every village and every night's mobs while its band walked on, and a golem
  that kept it targeted would hold it there. It rises again only once the band's steer has dropped the
  attack flag (the band leaving the target, routed or going home) and it has had no player target for
  200 ticks (PLACEHOLDER), a golem's blows notwithstanding: the plugin sets `noGravity` again, and it
  rises to its height and floats back to its place in the column. While the flag holds it stays down, so
  a target lost to a wall, to invisibility or to a player flying off does not start a loop of rising and
  settling again 32 blocks out.
- **Its summons.** Umvuthana and cranes it summons away from its throne get their restriction centre
  from it before they join (`1046`). A mixin at the jar's `addFreshEntity` call (`1050`) tags each one
  a chief puppet summons with the party's AgentTag, and the ConflictBridge adds it to the party's
  groups as one of its own holding no unit (ledger 25), before it joins: the core's join check runs at
  `EventPriority.HIGHEST` (`BridgeEvents.java:45`) and asks the plugin that owns a tagged agent, which
  discards an entity it does not know (`RtsimSession.rejectOnJoin`, `975-999`; `SessionPlugin.admit`).
  Vanilla never saves them, though the jar makes every minion persistent; they hold no unit, their
  deaths book nothing, and they despawn with the party. At home its summons stay strays (above).
- **Coming home.** A band walks home and, where a player is, has bodies (§4.16). At a grove that is in,
  the Umvuthi floats to its throne spot, turns to the throne's direction by a re-pair, and settles
  onto the throne, and a hand-back line (§4.9) replaces its puppet there with a tagged vanilla body:
  its constructor, `finalizeSpawn` as a structure spawn, `restrictTo` the fire pit with radius -1, its
  direction and health, and the engine's trade memory. A band that comes home with no bodies leaves its
  slot owed, and a spawn line places the Umvuthi on its throne under Rule U, at once if the grove is
  in, else at its next census.
- **Trade memory moves into the engine.** The jar keeps who paid the Umvuthi in its entity data
  (`TRADED_PLAYERS`, `729-743`) and saves it as NBT `"players"` (`779-793`). A puppet is never saved,
  so a payment to the Umvuthi away would be forgotten, and the payer asked again at home. Only a
  first payment is paid: `MessageUmvuthiTrade` (`59-69`) takes the gold and calls `rememberTrade`
  (`770-777`) for a player not yet remembered, and gives a remembered one Sun's Blessing free. A mixin
  on `rememberTrade` sends each first payment to any Umvuthi body, at home or away, as a journaled
  CHIEF_PAID (ledger 14) with that body's slot and generation. The engine books it as tribute at the
  camp (above), and adds the payer to the grove's memory only when the slot and generation are the live
  chief's: a payment to a dead chief's body still on the throne (§4.6) is tribute, and the heir does
  not remember it. The memory, per grove, is the payers so booked since the last succession, plus each
  census's set (`getTradedPlayers`, `735`) from a body of the live chief's slot and generation, so a
  dead Umvuthi's body read before its removal adds nothing. It costs 16 bytes a player and is cleared
  at a succession: a new Umvuthi has met nobody. CHIEF_BODY writes it into every body the engine places
  (a puppet, a hand-back or an heir) and into a loaded body whose set is short, through
  `setTradedPlayersCompound`, never `rememberTrade`, whose mixin would journal every payer again; that
  write has no record of its own, and if it is lost, the next census sends it again. CHIEF_BODY also
  carries the Frostmaw puppet's health and crystal, which §4.7 takes from the engine. A reset loses the
  payments the engine had not yet written into a body on the throne (§4.9).
- **The grove while it is away.** If the grove comes in while the Umvuthi is out, the census takes its
  throne body (§4.9), under Rule U: a visitor finds the throne empty and the warriors who stayed. If a
  player is within 160 blocks of the throne, the take waits, and that player sees it on its throne
  while it also marches with its band: two bodies (below). If the band comes home while that take still
  waits, Java answers the take stale once the puppet is within 32 blocks of the throne, and the
  hand-back replaces both bodies with one on the throne, with the puppet's health.
- **Rule U.** No Umvuthi body, on the throne or a puppet, is removed (by a take, a removal, a recall or
  a despawn) while a player is within 160 blocks of it or it has a player as its target, however long
  that lasts; a throne line still waiting when its epoch ends is answered stale, and the next census
  orders it again. An Umvuthi is spawned at its throne only once no player is within 160 blocks of the
  throne, or after 6,000 ticks (PLACEHOLDER), as a Frostmaw's spawn waits (§4.9). A hand-off and a
  hand-back replace a body where it stands and do not wait, and a re-pair (above) removes nothing; an
  heir seated in place of a dead chief's body waits only while that body is in a fight (§4.6), so a
  fight's boss bar is never refilled. The engine recalls a band with its chief only with no player
  within 192 blocks of it: `unseen` and `player_left` first re-bind it to any player that near, the one
  it was seen by included, and a band that arrives with such a player near but none within 160 blocks of
  its target resolves abstractly and walks home in its bodies (ledger 23). A recall's puppets despawn
  out of every player's sight or after `recall_timeout_ticks` (1,200, `engine.rs:3164`), whoever is
  near, so that timeout reaches an Umvuthi near a player only after an `edge_stop` recall or a
  homecoming with its grove out (below), or when a player comes within 160 blocks of a band recalled
  with nobody near (a teleport): a recall with no end would keep bodies for a band the engine has let
  go. Vanilla never removes an Umvuthi: it requires custom persistence (`EntityUmvuthi.java:393`), so
  `checkDespawn` (`MowzieEntity.java:291-297`) never despawns it for distance, and as a `PathfinderMob`
  it does not despawn on Peaceful. Reason for the rule: a grove's census can run with a player inside
  the throne's tracking range (it waits for the circle's far side, so a near-side body is 142 to 158
  blocks off when it runs, §4.9, and the throne, 18 blocks from the pit, can be nearer than 160); and a
  time limit on a removal would let the boss vanish while a player fights it. Four removals do not ask,
  and none can wait:
  - **The ticking edge.** The ConflictBridge takes back a stack the tick a member's chunk stops ticking
    entities (ledger 25): at a simulation distance of 8 an Umvuthi 128 to 132 blocks off on an axis,
    inside its draw distance (§4.16), can be seen to go. A frozen puppet held in a chunk that no
    longer ticks could be saved with that chunk when it unloads, against the puppets' never-saved rule.
  - **An `edge_stop` seam.** A band whose march would cross an `edge_stop` seam is recalled at the
    start of that segment, watched or not, and marches on abstract (`start_segment`,
    `engine.rs:2458-2475`, TP12); its Umvuthi goes after the recall's timeout. On MIC the north and
    south edges are `edge_stop` until the wrap engine installs (C31). A body walked on would cross an
    edge that does not join.
  - **A failed session.** The bridge's failure captures every puppet at once: the core's own
    (`captureAll`, `RtsimSession.java:308-317`) and each plugin's through `sessionFailed`, the
    ConflictBridge's among them. Nothing runs after it to hold a body.
  - **Home with the grove out.** A band that reaches home while its grove is out ends there and is
    recalled (BAND_RECALL reason 3, §4.16); its Umvuthi, at its throne, goes out of every player's
    sight or after the recall's timeout. The grove is out with a player within 160 blocks of the throne
    only while one of its footprint chunks lies beyond that player's loaded chunks. The party has
    ended, and once the grove comes in its census places the chief, so a body held past the band's end
    would be a second one.
- **Two bodies, while a removal waits.** While a take of its throne body is held, the Umvuthi has its
  puppet with its band and its body on the throne: the one exception to I5 (§4.9), bounded by Rule U's
  hold, which ends when the player leaves or the epoch ends. The plugin keys deaths on the body's UUID.
  `die` calls `super.die` twice (`EntityUmvuthi.java:858-871`), so NeoForge's `LivingDeathEvent` fires
  twice for one body (`LivingEntity.java:1408-1409`), and its drops come at the end of its death
  animation (`MowzieEntity.tickDeath`, `453-467`), after both. The first body to die is the member's
  death, with its loot, and a second event for the same UUID is ignored. The other body (Java knows it
  by the member's first death, or by a removal line for a dead slot), if it dies before it is removed,
  drops nothing (NeoForge's cancelable `LivingDropsEvent`), takes no warriors with it (the `die` mixin,
  ledger 18), books nothing, and is removed. Keyed on the member instead, the second event would cancel
  the real Sol Visage (test 27).
- **Killed away.** Its puppet's death is CHIEF_KILLED, as a party chief's is today
  (`engine.rs:3975-3982`), into today's `chief_killed` (`4123-4200`): the killer's Killing report; the
  heir succeeds at once, holding 0.5 of the old grudges (§3.3); the tribe is cowed for 6 × 1000 ÷ the
  chief's boldness bend days; the party's feud steps down 2 rungs, from Invasion to Scouts; the band
  routs and walks home (§4.16); other parties go on. Killed aloft, it is let fall: the plugin clears its
  `noGravity` at the first `LivingDeathEvent`, and the pass no longer steps it, so it falls at vanilla's
  gravity to the ground or a river's bed during its death animation (it takes no fall damage), and its
  loot drops where it lies. The jar's `die` kills every devoted Umvuthana within 30 blocks and 20 up
  (`EntityUmvuthi.java:859-871`, `MowzieEntity.java:449-451`): its summons, and the band's own within 30
  blocks, devoted as every Umvuthana is (`EntityUmvuthana.java:761`). On the road, with the Umvuthi at
  the rear of a column about 12 blocks long, that is the whole column: each a BAND_CASUALTIES whose
  killer is vanilla's credit (`getKillCredit`, `LivingEntity.java:1814-1820`: the player who hurt that
  body in the last 100 ticks, else none, §4.6). The player who killed it gets the Sol Visage. The
  grove's P_chief gains 1 (EQUILIBRIUM.md §2.2's boss rule), so the heir comes later (§4.10). Its death
  animation lasts 115 ticks (`DIE_ABILITY`, `EntityUmvuthi.java:210`), and its drops come only at its
  end (`MowzieEntity.tickDeath`, `453-467`): if its puppet is taken back in that window (a stop, the
  ticking edge, a recall's timeout), the plugin first does what `tickDeath` does at the end, the killer
  set from its kill data and `dropAllDeathLoot` with its kill cause, through an accessor mixin, so the
  Sol Visage is on the ground (test 36); a crash in that window loses it. If its body is still on the
  throne (dead, left), the grove's next census removes it under Rule U, or the heir is seated in its
  place (§4.6); if rtsim is removed before then, the Umvuthi is back on its throne: the jar's state
  wins, which UR-1 allows.
- **Uninstall (UR-1).** While its throne body is gone, an uninstall leaves the grove without its
  Umvuthi for good, as §4.9 lists for any member away whose body is gone: the whole of an Invasion that
  set out from a grove that was in, and the rest of one whose grove came in while it was out. That
  includes a chief killed away from home: from its death until its heir is seated (12 days at least,
  §4.10) the throne is empty, and an uninstall in that time leaves the grove without a chief. Payments
  the engine holds and has not yet written into a body are lost too, at an uninstall as at a reset. The
  jar creates an Umvuthi only when it generates a grove (`UmvuthanaGrovePieces.java:276-288`; checked
  in the jar), so nothing brings back a throne body handed off in view or taken. That is MZ-15's cost:
  a march that a visit to the grove cannot cut short. An Invasion that sets out from a grove that is
  out and comes home before the grove next comes in leaves the throne as it was. A write-back of the
  throne body at shutdown was rejected: Java would place a body the engine had not ordered, against I6,
  and a crash would skip it.

### 4.6 Killing a boss at home

The Umvuthi dies on its throne, or the Frostmaw in its den. Both bodies are vanilla there, so the
plugin sees the death (`LivingDeathEvent`) and reports CAMP_DEATH with the chief flag and the killer
(ledger 5, 14). The engine runs the camp form of `chief_killed`: the heir succeeds, the faction is
cowed for `cowed_days` (6, scaled by the chief's boldness), the killer's feud drops two rungs (floor
Notice; for the Frostmaw, Notice), and any party out routs home. Loot is the jar's own: the Umvuthi
drops `sol_visage` when a player kills it (`loot_table/entities/umvuthi.json`); the Frostmaw drops
`ice_crystal` if a player kills it while it holds the crystal (`loot_table/entities/frostmaw.json`).

A warrior killed at the grove is a CAMP_DEATH without the chief flag: the pool loses one, and a kill
by a player is also the killing signal (P14).

**The Umvuthi takes its warriors with it.** Its `die` finds every Umvuthana within 30 blocks and no
more than 20 above it (`EntityUmvuthi.java:859-871`; `MowzieEntity.getEntitiesNearby`, `449-451`) and
gives each devoted one a `timeUntilDeath` of under 20 ticks, after which it dies by a magic hit with no
attacker (`EntityUmvuthana.java:531-535`). Every Umvuthana is devoted (`EntityUmvuthana.java:761`) but
a follower bound to a player (`EntityUmvuthanaFollowerToPlayer.java:201`). The grove's warriors start 2
to 11 blocks from the pit and the Umvuthi 18 (`UmvuthanaGrovePieces.java:276-288`, `494-507`), so 7 to
29 blocks from it: most of the grove, often all of it, falls with its chief. Each is a CAMP_DEATH whose
killer is vanilla's credit (`getKillCredit`, `LivingEntity.java:1814-1820`), a player who hurt that
warrior in the last 100 ticks, else none: the plugin asks for it, as the magic hit's source has no
entity and `BridgeEvents.java:234` reads the source's entity today, so one kill of the chief is not a
Killing signal for every warrior. The heir's body then waits for a warrior to stand in the grove again
(§4.10).

**The new chief's body.** The Frostmaw's comes after its wait (§4.2). The Umvuthi's comes from a
warrior: once the heir's wait is over (§4.10) and a warrior stands in the grove, one reconcile, under
Rule U (§4.5), removes the warrior farthest from any player that has no line in flight, whose slot is
freed and the pool one less, and spawns the chief at the throne spot, filling the chief's slot: its
constructor, `setPos`, `setDirection` as the old one faced, `finalizeSpawn` as a structure spawn,
`restrictTo` the fire pit with radius -1 (as `UmvuthanaGrovePieces.java:276-288` does), an empty trade
memory, and the plugin's tag. If the old chief's body is still on the throne when the wait ends (it
died away, its slot dead, left, and its removal held under Rule U or not yet ordered), the reconcile
seats the heir in its place instead: Java answers a held removal of that body stale and, in one tick,
removes the body and spawns the heir where it stood, with the heir's full health and an empty trade
memory; the warrior's removal goes with it. That hand-off waits only while the old body is in a fight:
it has a player as its target, and a player has hurt it within the last 600 ticks (PLACEHOLDER), which
the plugin notes at each hit. A target alone would not end: the Umvuthi takes an unmasked player within
its follow range of 40 blocks as its target whether it sees them or not, with no time limit
(`EntityUmvuthi.java:322-336`), so a player working beside the throne would hold the heir off for good.
So the hand-off never refills a fight's boss bar, and its wait ends with the fight: a grove beside a
player's base, whose dead chief's body Rule U would hold as long as the player stays, still gets its
heir when the wait ends. Until then the old body can be paid: the payment is tribute, and enters no
trade memory (§4.5). The throne spot is the Umvuthi's position and direction at the grove's first
census (CAMP_CENSUS); a grove found without its Umvuthi uses the fire pit, direction 1 (PLACEHOLDER).

**At a station (Invasion).** A Frostmaw that finds a district within 64 blocks of its station spot
raids it as well: Lanchester, class by its crystal, nothing broken, nothing stolen beyond the cargo
rule.

### 4.7 On screen: real entities with the engine's state

| Body | At home | Away (a puppet) | State it gives back |
|---|---|---|---|
| Frostmaw | the jar's vanilla body, tagged by the plugin and restricted to its den (§4.9); `PersistenceRequired` set by the plugin at the steal, which also reports CHIEF_STATE: a mixin at the `setHasCrystal(false)` call in `EntityFrostmaw.tick` (`615`), not on the setter itself, which loading NBT also calls (`963`); without it the random despawn takes a robbed Frostmaw within about a minute (§3.1) | its constructor; NBT `has_crystal`, `active`, `alwaysActive` set before its first tick (`EntityFrostmaw.java:960-975`); health and crystal from CHIEF_BODY (§4.5) | health and crystal (CHIEF_STATE; census), death with killer |
| Umvuthana warrior, envoy | the jar's vanilla bodies, tagged (an envoy party's three are fresh puppets, never taken from home, §4.9) | its constructor, then `finalizeSpawn` so it gets its trade table (`EntityUmvuthanaMinion.java:243-250`); mask (NBT `"mask"`), weapon (NBT `"weapon"`, `EntityUmvuthana.java:682-690`) | casualties with killer |
| Raptor pack | not at home: a raptor pack exists only in a party | the raptor's constructor, then 4 followers through the public `(type, level, leader)` constructor (`EntityUmvuthanaFollowerToRaptor.java:33`), not `finalizeSpawn` (which adds 4 or 5 of its own, `EntityUmvuthanaRaptor.java:191-199`) | casualties |
| Umvuthi | vanilla and tagged, on its throne; an heir spawned at the throne spot, or seated in place of a dead chief's body, by a reconcile (§4.6): its constructor, `finalizeSpawn` as a structure spawn, `restrictTo` the fire pit, its direction | only on an Invasion, a puppet aloft at the rear of its band wherever the band has bodies: met on the road, at the fight and walking home (§4.5, §4.16): its constructor, `finalizeSpawn` with `EVENT`, `setDirection`, `setNoGravity(true)`, its health and trade memory from CHIEF_BODY; at the fight it settles and is restricted where it sits; at home a hand-back puts a vanilla body on its throne | its death with the killer (CAMP_DEATH at home, CHIEF_KILLED away); each first payment to one of its bodies (CHIEF_PAID, with that body's slot and generation); the players a body on the throne has traded with (census) |

Steering moves the restriction centre for the bodies that walk to it, the Frostmaw
(`EntityFrostmaw.java:208`) and the warrior (`EntityUmvuthanaMinion.java:127`); the raptor has no such
goal, so the bridge gives it navigation orders and its followers keep to it; the Umvuthi, which cannot
walk, is floated to its place in the column (§4.5). Body kinds are registered in code (P15 as built).

### 4.8 Effects through modules that exist

| On | Frostmaw | Umvuthana |
|---|---|---|
| Trains | Ambush while lying in wait: cargo destroyed, bounded (§4.4). Watched: swipes at carriages, ice balls unseat riders. The robbed-train roar. | None: bands on the march may cross a leg, but carry 0 takes nothing. |
| Stations | Invasion waits at the nearer station: every train from or to it passes. | None beyond the district. |
| Districts | An Invasion that finds a district within 64 blocks of its spot raids it: Lanchester, class by crystal; nothing broken or stolen. | Lanchester; walls stop it; raptors owe dead animals on load. |
| Villages | None: dens are 7 chunks or more from villages, and it plunders nothing. Villagers flee it at 10 blocks (`ServerEventHandler.java:250`). | Raptor plunder of livestock goods; villagers flee Umvuthana at 6 blocks (`247`). |
| Ships | None: its biomes exclude ocean, river and beach. A watched fight by a river port ices water for 140 ticks. | None. |
| Players | Watched: attacks players only. | Unmasked players, at their base (P26); on an Invasion, the Umvuthi, borne aloft with its band and fighting at the base (§4.5). |

### 4.9 The camp reconciler: bodies, saves and uninstall (UR-1)

UR-1 lets Ben remove rtsim at any time and lets a reset lose only the sim's own state. Puppets are
never saved by vanilla (the core's `EntitySaveMixin`, `core/puppet/AgentTag.java:10-12`), so a den
whose Frostmaw were a puppet would be empty after an uninstall, and a grove of puppets would lose its
warriors. So **bodies at home stay vanilla**: the jar's own entities, saved by vanilla with their own
health, crystal, mask and weapon. Remove rtsim and every den and grove looks as the jar left it, give
or take what players did.

This section is one mechanism, and the only place the plan moves a body at a camp. Each camp keeps a
ledger of its members' bodies under six invariants; a census runs every time the camp's entities have
all come in; the engine compares the ledger with the census and sends one order that is safe to apply
twice; and the record of what that order did rides on the entities it changed, so the next census
repairs a crash at any point.

**Who has a body at home.** Only war parties draw on the pool: raid, invasion, siege and plunder
(`PartyKind::is_war`, `culture.rs:216-221`; `send_party`, `engine.rs:2224-2251`). An envoy party and a
scouting raptor pack draw on no pool (`engine.rs:2222-2223`: "envoys and scouts always go"), so their
bodies are fresh puppets: they hold no slot, take no home body, and are gone when they end, wherever
they are. So a census never meets them, and an envoy's demand stays open while its party is out (a
census ends no party, §4.16).

**The ledger.** The engine keeps, per camp, one *slot* for each member of the camp's pool and of its
war parties away. A raptor pack is one slot: it is one pool unit, and one warrior's body at home
(§4.5). Each slot has a *generation*, derived from the slot and the tick its member was recruited or
adopted by the op id rule (`docs/boundary.md:153-163`), so a replay after a crash derives the same one.
A grove also has a slot for its Umvuthi, the chief, which is in no pool: it is away only while it leads
an Invasion (MZ-3 (b), MZ-15, §4.5), and after a death it is owed a body that waits for the heir
(§4.10); a den's Frostmaw is its chief and its pool's one member. Each slot is in one of five states:

| State | The member | Its body |
|---|---|---|
| home, body | in the pool | in the camp, tagged |
| home, owed | in the pool | none yet: to be spawned |
| away, left | in a war party | still in the camp, tagged: the camp was out when the party set out, or it is still mustering |
| away, gone | in a war party | none: taken, handed to the party as its puppet, or never had one (it was owed when it was drawn, and carries the debt) |
| dead, left | none: it died away | still in the camp, to be removed |

The invariants:
- **I1.** Every member has exactly one slot, and every slot is in exactly one state.
- **I2.** `floor(pool_at)` (`engine.rs:1053-1058`) is the number of the pool's slots at home, and the
  slots away are the war units out. The ledger follows the pool: when the pool gains a whole member a
  slot is added, home and owed, with its generation from the tick it became whole; every other change
  to the pool (a draw, a return, a death, a write-off, an adoption) moves a slot with it.
- **I3.** The camp's tagged bodies in the world, loaded or saved, are exactly its slots home with a
  body, away with it left, and dead with it left. So the bodies in the world plus the bodies owed equal
  the members at home plus the bodies left behind, and with none left behind, the members at home.
- **I4.** No order is applied twice (*Orders and answers*, below).
- **I5.** No member has two bodies: a spawn is ordered only for a member owed one, Java does nothing
  for a spawn whose body is already loaded, and if two loaded bodies ever carry one slot and
  generation, the next census removes the one farther from the camp. The one exception is an Umvuthi
  whose throne body's take is held under Rule U while its puppet goes with its band, until the player
  near the throne leaves or the epoch ends (§4.5).
- **I6.** The engine books only what Java reports it found or did (CAMP_CENSUS, CAMP_BODIES,
  CAMP_DEATH), never what it ordered.

A crash can break I3 only in the ways the crash table below lists, and the next census repairs each.

**Identity.** Every body a slot holds (at home, left behind, or dead and left) carries a
`mic_rtsim:camp_member` entity attachment: the faction (the camp's key), the slot and the generation.
Vanilla saves it with the entity, as `mic_rtsim:gossip_ops` rides on villagers
(`core/puppet/RtsimAttachments.java:36-38`); after an uninstall NeoForge skips it with a log line
(`AttachmentHolder.java:162`) and the jar's entity is as it was. A *stray* is a body of the camp's
kinds in its footprint that is untagged (the jar's own at a camp's first census, Umvuthana the Umvuthi
summoned, a body put down from Carry On) or tagged with a slot and generation the books do not hold
(from before a reset, or written off). The engine remembers the generation each slot last retired, and
whether at a death. The plugin keeps an index of the loaded tagged bodies by `EntityJoinLevelEvent` and
`EntityLeaveLevelEvent` (`ServerLevel.java:1820`).

**A home Frostmaw stays in its den.** The jar never calls `restrictTo` on a Frostmaw, so its
`MoveTowardsRestrictionGoal` (`EntityFrostmaw.java:208`) is inert, and one that chased a player falls
asleep wherever it stands 1,200 ticks after losing its target (`581-586`), outside its den. A
restriction is not saved (`Mob.java:132-133`, `1305-1326`), so the plugin restricts a tagged Frostmaw
to its den's spot (8 blocks, PLACEHOLDER) each time it joins the level. A warrior already keeps its
home: it saves `HomePosX/Y/Z` and `HomeDist` and restricts itself again on load
(`EntityUmvuthanaMinion.java:281-285`), 25 blocks round the grove's centre
(`UmvuthanaGrovePieces.java:507`). Whether a Frostmaw walks back before it sleeps is UNVERIFIED (test
22).

**When a camp is in.** A camp's *footprint* is the den's template box (6 × 5 × 7 blocks, 1 to 4 chunks)
or the grove's 25-block circle (at most 5 × 5 chunks). Its bodies are all there only while every
footprint chunk is at FULL and has its entities loaded. Entities load after their chunk, asynchronously
(`PersistentEntitySectionManager.java:215-222`, `243-250`), and unload as soon as the chunk drops below
FULL (`149-152`, `224-241`; below FULL is HIDDEN, `Visibility.java:26-31`). The chunk itself can stay
in memory up to level 44 (`ChunkLevel.java:15`) and come back to FULL with no new `ChunkEvent.Load`,
which fires only in a chunk's first FULL load task (`ChunkStatusTasks.java:215`): a held chunk keeps
its finished FULL result (`GenerationChunkHolder.java:161-165`), and one waiting to unload comes back
as it is (`ChunkMap.java:414`). So:
- The plugin follows each footprint chunk's ticket level through `ChunkTicketLevelUpdatedEvent`
  (`ChunkMap.java:425`), on the server thread, before the chunk's holder is promoted or demoted. FULL
  is level 33 or lower (`ChunkLevel.java:10`).
- When any footprint chunk's level rises above 33, the camp is **out**: Java stops applying its orders
  and sends CAMP_OUT with the epoch that ended.
- While every footprint chunk is at 33 or lower, the plugin checks `ServerLevel.areEntitiesLoaded`
  (`ServerLevel.java:1702`) for each at the end of every server tick. At the end of the first tick in
  which all hold, the camp is **in**: Java mints a new *epoch* (a fresh Java op id,
  `docs/boundary.md:154-155`) and takes the census.
- A body tagged for a camp that is in, read from disk outside the footprint (its chunk loading later),
  starts a new epoch and census the same way.
- `ChunkEvent.Load` only finds camps: a chunk holding a structure start (the lair finder pattern,
  `Lairs.java:44-60`) registers the camp's footprint; the plugin reads each footprint chunk's current
  level once, then follows the events. At a camp's discovery FACTION_FOUND (§4.1) goes first, and its
  slots start owed, for the census's strays to fill.

A player's tickets bring every chunk within the view distance, Chebyshev, to entity ticking
(`DistanceManager.java:40`, `549-551`); the level rises one per chunk step beyond
(`ChunkTracker.java:23-25`, `71-72`), and FULL is two steps further out (`ChunkLevel.java:10-12`). So a
camp comes in when a player's chunk comes within the view distance plus 2 chunks of every footprint
chunk: at the default 10, about 192 to 208 blocks on an axis and 271 to 294 on a diagonal. A player 3
to 13 chunks beyond the view distance (about 210 to 370 blocks at the default 10) holds the camp's
chunks in memory below FULL, and every time they return to FULL the camp comes in again, with a new
census. Reason, over counting `EntityJoinLevelEvent`s as `HabitatEvents.java:25` and
`DragonBridge.java:1167` do: a count has to know the last body is in, and join events alone cannot say
when that is.

**The census** (CAMP_CENSUS) carries the epoch; every body of the camp's kinds in the footprint, tagged
(its slot and generation) or a stray (its UUID); the camp's tagged bodies loaded elsewhere, from the
index; whether the chief is present, its crystal and health; and for the Umvuthi, its position and
direction (the throne spot, §4.6) and the players it has traded with, with that body's slot and
generation (§4.5). Who is near does not change what a census decides: a body left behind by a party on
the road is taken whoever is near (§4.16), so the census carries no player.

**The reconcile.** On CAMP_CENSUS the engine marks the camp in at that epoch, voids every line of an
earlier epoch still in flight, and compares each slot with the bodies found:

| Slot | Its body found | Not found |
|---|---|---|
| home, body | stays; its misses reset | a miss; at the third, at least 2 days after the first (PLACEHOLDER), the member died unseen: the pool loses it and its generation is retired |
| home, owed | home, body (a spawn whose answer was lost) | a stray fills it (an adopt line), else a spawn line |
| away, left | by its party's phase: mustering, nothing; out, waiting, fighting or walking home, a take line, the party marching on (a census never ends a party, §4.16) | away, gone (a take whose answer was lost) |
| away, gone | away, left, then as that row | stays |
| dead, left | a remove line | the slot is freed |

Then the strays left over: a body whose generation the books retired at a death is removed; the rest
fill new slots (adopt lines, the pool gaining each) while the pool plus the members away is under the
camp's cap, as a refill does (ledger 6), and are removed above it. Of two bodies with one slot and
generation, the one farther from the camp is removed.

**The Umvuthi's slot** follows the table like any slot, with three differences. It is drawn only by an
Invasion (§4.5): from a grove that is in, its body goes in the set-out line like a warrior's (handed to
its puppet if the band is seen as it sets out, else taken), and from a grove that is out it
leaves away, left, for the grove's next census to take. Every take, removal and despawn of its bodies
waits under Rule U (§4.5), and a spawn at its throne waits until no player is within 160 blocks of it,
or 6,000 ticks. And while it is owed after a death, nothing is spawned and no stray fills it until the
heir's wait is over and a warrior stands in the grove; then one reconcile removes that warrior and
spawns the chief (§4.6). After it dies away, its slot is dead, left while its body is still on the
throne, freed when a census removes that body under Rule U, and owed from then until the heir's wait
ends and a warrior stands in the grove. If the wait ends with that body still there, the heir is seated
in its place (§4.6), so a removal held under Rule U holds back the heir only while that body fights a
player.

**A party ends only through its own `home`** (`engine.rs:2425`, `3170`), at its camp at the end of its
walk home, or when its last unit dies; never at a census (§4.16). Its survivors are back in the pool
and its loot is stocked at the camp (a Frostmaw carries none, ledger 10). A survivor whose body is
still in the camp is home, body again. One whose body is gone is owed: at a camp that is in, a survivor
whose puppet is in the world is handed back where it stands (*A homecoming*, below), and the rest get
spawn lines in the same reconcile; at a camp that is out, its puppets, if any, are recalled as today
(BAND_RECALL, reason 3, done), and the camp's next census spawns them. A party left with no units ends
the same way, with AMBUSH_SPOT's removed flag if it was lurking (ledger 8). So no member is in two
places, outside the Umvuthi's one held exception (I5). And while a camp is in, a body is left behind
only while its party musters or a line for it is in flight.

**While a camp is in,** the engine orders each change as it happens, in the same form:
- **A muster's end** (`party_event`, `engine.rs:2413-2417`): a set-out line with the party's agent for
  its members' bodies left. When a BAND_MATERIALIZE for that agent comes in the same hand-over, Java
  hands them off: it removes each body and spawns the party's puppet in its place, with its health and
  crystal or its mask and weapon; otherwise it takes them. The party is materialized then when it is
  seen as it sets out: a player's area (`view_radius` 160) already covers the camp, so `book_leg` books
  their meeting at the leg's start (`engine.rs:2338`, `2378-2394`). A party not seen then gets fresh
  puppets when `seen` embodies it later (`engine.rs:3602-3610`), with the engine's health and crystal
  for a Frostmaw (CHIEF_STATE, census) a new mask and weapon for a warrior, and the Umvuthi's health
  and trade memory (CHIEF_BODY). A raptor pack's slot gives up the warrior body farthest from any
  player, taken, as a pack's puppets are a raptor and its followers. The Umvuthi's slot is in the
  set-out line too: if the band is seen as it sets out, its throne body is handed to its puppet where
  it sits, which rises from the throne and floats out behind its warriors; otherwise the body is taken,
  under Rule U (§4.5).
- **A homecoming** (`home`): a survivor whose puppet is in the world gets a hand-back line: Java
  replaces the puppet where it stands with the camp's tagged vanilla body (a warrior's with its mask
  and weapon, restricted to the grove's centre; a Frostmaw restricted to its den; the Umvuthi on its
  throne, once its puppet has settled there, §4.5), with `PersistenceRequired` set. A raptor pack's slot
  gets a spawn line at the grove's edge farthest from any player, while its pack walks off and despawns
  as a recalled puppet does. The survivors with no body and no puppet are owed again, and each gets a
  spawn line.
- **A refill:** each whole member the pool gains is a slot owed a body, and a spawn line. The pool
  refills only while it and the members away are under the camp's cap (ledger 6), so a band's survivors
  always have their slots.
- **An heir:** once the heir's wait is over (§4.10) and the Umvuthi's slot is owed, a removal line for
  the warrior farthest from any player, of those with no line in flight, and a spawn line for the
  chief, in one reconcile, both under Rule U (§4.5, §4.6); with no warrior home, nothing until one is.
  If the slot is still dead, left, its old body on the throne, the chief's line seats the heir in that
  body's place instead, a hand-off that does not wait, and a held removal of the old body is answered
  stale (§4.6). A grove that stays in, as one beside a player's base does, gets its heir as soon as the
  wait ends.

While a camp is out, nothing is ordered: its slots change only by events, and its next census
reconciles them.

**Draws and losses** (walkthrough A, "kill it"). `send_party` draws its units from the camp's slots at
home: first those whose bodies the last census found, then those whose bodies it missed, then those
owed, skipping any slot with a line in flight, whose body may be going or coming, and never more than
it may draw: the smaller of floor(pool) (`engine.rs:2236`) and those slots; a raptor pack's pick of a
warrior body and an heir's pick skip them too, so no slot is spent twice. A slot drawn with a body is
away, left; one drawn owed is away, gone, carrying its debt: no take and no spawn is ordered for it
while it is away, and at `home` it is owed again. Losses away fall first on slots whose bodies are
gone, so fewer bodies are removed later; a slot left behind that dies is dead, left.

**Deaths** (CAMP_DEATH, ledger 14) carry the body's slot and generation, and are journaled at the end
of the tick (`docs/boundary.md:253`). The death of a body whose slot and generation the books hold
kills that member wherever the books have it: at home, the pool loses one (the Umvuthi's slot is owed
again, its body waiting for the heir, §4.10); away, its party loses that unit in any phase (mustering,
setting out with its answer in flight, out with its body left); dead, nothing more. A stray's death
books nothing. The plugin sends one CAMP_DEATH per body UUID (the Umvuthi's `die` raises two
`LivingDeathEvent`s, §4.5), and a second chief death for a slot already dead does nothing
(`chief_killed` returns early, `engine.rs:4123-4200`). A party left with no units ends through `home`,
with AMBUSH_SPOT's removed flag if it lurked. The chief flag runs the camp form of `chief_killed`
(ledger 5), whose `go_home` routs every party out (`engine.rs:4183-4188`), so a Frostmaw whose
left-behind body is killed ends its own raid, and an Umvuthi's throne body killed while it is away (held
under Rule U, §4.5) ends its Invasion. A tagged body that Carry On picks up leaves the sim (A10)
through the existing hook (`MicRtsimCarryOn.java:39-43`, extended to tagged bodies): its tag goes and
it is a CAMP_DEATH with killer kind taken. One removed as DISCARDED (`Entity.java:3891-3895`) by
anything but the plugin (a despawn, another mod) is a CAMP_DEATH with killer kind none.

**Orders and answers.** The one order is CAMP_RECONCILE: faction, epoch, an op id derived from the
faction, epoch and decision (`docs/boundary.md:153-163`), and lines, each a slot and generation with
what to do. It goes through the outbox in the industry reconcile's form
(`docs/industry-boundary.md:356-362`, F27): it is not resent after a restart, but derived again at the
camp's next census.
- Java applies a CAMP_RECONCILE only while its camp is in that epoch, every line in the tick it arrives;
  otherwise it answers STALE and applies nothing. The waits are a Frostmaw's spawn and the Umvuthi's
  lines (Rule U, §4.5). A spawn of either boss is held until no player is within 160 blocks of the den's
  spot or the throne (their tracking ranges, `EntityHandler.java:97-98`) or for 6,000 ticks
  (PLACEHOLDER), whichever is first, so a player who lives beside a camp still gets its boss back. A
  take or removal of an Umvuthi body is held while a player is within 160 blocks of it or it has a
  player as its target, with no limit, so a boss never vanishes in a fight. A hand-back of the Umvuthi
  waits for its puppet to settle on the throne. An heir seated in place of a dead chief's body waits
  only while that body is in a fight (§4.6). A held line still waiting when its epoch ends is answered
  stale.
- Each line names a target state: a spawn does nothing if a body with that slot and generation is
  loaded; a take, set-out or removal does nothing if none is. With the camp in, every footprint body is
  loaded, so applying a line twice changes nothing.
- Java answers with CAMP_BODIES: per line, done, nothing to do, or stale. The engine books the world
  fact the answer reports, by state, as it books TAKEN and DIED (`docs/boundary.md:249-252`). *Body
  gone* (a take, set-out or removal, done or with nothing to do): home, body becomes owed; away, left
  becomes gone; dead, left is freed. *Body there* (a spawn, done or with nothing to do, or an adoption
  done): home, owed becomes home, body; away, gone becomes left, reconciled then by its party's phase
  as at a census. A line for a stray names it by UUID and books nothing for any slot, but an adoption
  is *body there* for the slot it fills. A replayed answer is then a no-op, and an answer or record for
  a generation its slot no longer holds books nothing.
- The engine keeps at most one line in flight per slot, and sends only to camps that are in. On
  CAMP_OUT it marks the camp out and voids that epoch's lines still in flight, which Java answers
  stale. Java applies an epoch's lines before that epoch's CAMP_OUT or the next census, so their
  answers reach the engine first, and a second census never meets the first's lines unapplied. At a
  session start every camp is out and every line in flight is void, as every conflict command is per
  session (`docs/boundary.md:1463-1464`). A CAMP_CENSUS replayed from an earlier session only marks its
  camp out.

**Records ride on the entities.** Spawns, takes and deaths change saved entity chunks, which vanilla
writes apart from the journal: in their own region files through their own IOWorker
(`SimpleRegionStorage.java:17`, `24`), each chunk's entities in one write
(`EntityStorage.java:86-108`), after `saveLevelData` has queued the journal's write
(`ServerLevel.java:817`, `824-826`). So the record of each line and each death rides on an entity in
the same chunk as the change, in `mic_rtsim:pending` (`core/puppet/RtsimAttachments.java:30-31`), as
`docs/boundary.md` §3.1 keeps transfers on chests and TAKEN on creatures. A spawned or adopted body
carries its own. A taken, handed-off, removed or killed body leaves a `minecraft:marker` where it
stood, holding the record: never sent to a client (`EntityType.java:496-498`, `Marker.java:39`) and
with no tick (`Marker.java:21`). The camp's chunk would not do: its record could be on disk while the
entity change was not, or the other way round. Whenever a holder loads with its record, Java journals
the record again with the same op id, which the engine skips if it has it; on the engine's
acknowledgement Java drops the record, at once or when the holder next loads, and discards the marker.
And the `ChunkWriteBarrier` (ledger 22) learns entity chunks: the journal's write waits until the
entity chunks that a camp record speaks for are on disk, or is held to the next world save, so the
journal is never ahead of them. Today "Entities that carry records are not covered" (`docs/boundary.md`
§3.1).

**Crashes.** One reconcile's steps: (1) the engine decides and writes CAMP_RECONCILE; (2) Java applies
it in one tick, leaves the records on their holders, and journals CAMP_BODIES, noting the holders'
entity chunks to the barrier; (3) those entity chunks are saved, at an unload or a world save; (4) the
journal's file is written, after (3) by the barrier; (5) the engine admits CAMP_BODIES and its pages
are saved; (6) the engine acknowledges and Java drops the records. A death starts at (2).

| A crash | What is on disk | What the next start does |
|---|---|---|
| before (2), or between (2) and (3) | nothing changed | the order is void; the camp's next census orders again whatever is still needed |
| between (3) and (4) | the entity change and its record; no answer in the journal | the holder's record is journaled again when its chunk loads, before the camp's census (a holder in the footprint loads with it), and booked by state; a holder outside the footprint is booked when it loads, and meanwhile the census books the missing body by its table |
| between (4) and (5) | the answer in the journal | replayed, and booked by state |
| between (5) and (6) | the answer in the pages too | skipped by op id; a holder's record journaled again is a no-op |
| the journal ahead of an entity chunk | nothing, while the barrier holds the journal's write (it gives up after 3 held saves in a row, logged, `GIVE_UP_ROUNDS`) | without it, or after it gives up: a spawn booked but not on disk is missed three times and written off (a member lost); a take booked whose body is on disk is found, so away, left, then as that row; a death booked whose body is on disk has a generation retired at a death, so the census removes it |
| the engine's pages ahead of the world save (P29, still open) | a line or death booked that the world lost | as the row above, without the barrier's help |
| an order in flight at a shutdown | | void; the next census orders it again, so it is never answered late or applied twice |

So a crash can lose a member, a spawn the world lost being written off after three misses, but never
make a second body of one, and a living member is booked dead only after three misses over two days. A
body that walks into another chunk after its line is answered can be saved there after the journal, as
the barrier noted only the chunk it was in: a crash then loses that body, written off after three
misses, one member at most per body that moved.

**In plain view.**
- **Setting out.** A party seen as it sets out leaves in its members' own bodies, handed off, as they
  stand; the Umvuthi rises from its throne and floats out behind them (§4.5). Otherwise no player
  is within 160 blocks of the camp (for a grove, of its centre, so at least 135 blocks from any of its
  bodies), and no client tracks the bodies the set-out line takes: a Frostmaw is tracked from 10 chunks
  and an Umvuthana from 8 (`EntityHandler.java:98`, `94`), capped by the view distance
  (`ChunkMap.java:1331`), assuming the server's `entity-broadcast-range-percentage` is 100, which
  nobody has verified. The warrior body a raptor pack stands for is the one that can be seen to go,
  when every body is within 128 blocks of a player.
- **On the road.** A band met on the march appears at its march position stack by stack, each as it
  comes inside the entity-ticking area (ledger 25): 160 to 176 blocks off on an axis at a simulation
  distance of 10 and 128 to 144 at 8, beyond the distance a client draws a warrior or a raptor (§4.16).
  It walks on; the far view draws it from BAND_MARCH until the client has its puppets.
- **Coming home.** A band walking home has bodies where a player is (§4.16): a player who follows it
  sees it walk into its camp and become the camp's own (hand-backs). A band that comes home with no
  bodies is owed: a Frostmaw's spawn waits until no player is within 160 blocks of the den's spot (or
  6,000 ticks), then places it asleep there; warriors spawn at the grove's edge farthest from any
  player and walk in, restricted to the centre as `UmvuthanaGrovePieces.java:494-507` does. Each
  spawned or handed-back body is tagged, with `PersistenceRequired` set.
- **A take at a census** comes a round trip after the camp's entities come in at the view distance plus
  2 chunks, beyond those ranges. The budget is tight on an axis approach. At a den the census waits for
  the farther chunk when the template straddles a chunk border, so a powered train at 40 blocks a
  second (2 a tick, §4.3) leaves 14 to 24 ticks before the Frostmaw shows at 160 blocks, and about 50
  or more on a diagonal (271 to 294 blocks). At a grove the census waits for the circle's far side, so
  a near-side warrior is 142 to 158 blocks off when it runs and shows at 128: 7 to 15 ticks. In those
  ticks the entities are read from disk (asynchronously), the census runs at the end of the tick they
  are all in, and one hand-over brings CAMP_RECONCILE back, at least a tick later and more while a step
  is still running (`docs/boundary.md:12-13`, §1.2 there). On a server that keeps up that is a few
  ticks; on one whose entity reads or engine fall behind, a rider at full speed on an axis can see the
  body for a moment before it goes. Test 22 measures the ticks. A player who logs in or teleports
  within tracking range of a camp with a body left behind can also see it go. Reason, over dropping the
  body at `EntityJoinLevelEvent` before any client tracks it: whether a tagged body is left behind or
  at home is the slot's state, which only the engine has.
- **The Umvuthi** is seen to go and come only as itself: it floats out and home with its band, aloft at
  its rear (§4.5); its body on the throne is taken or placed only under Rule U; and a hand-off or
  hand-back replaces a body where it stands.

**Reset and uninstall.**
- **A reset** recreates each faction from FACTION_FOUND, which carries the counts at the camp's first
  census: a robbed den comes back robbed (the body's `has_crystal` is 0), a grove comes back with the
  warriors standing in it and the Umvuthi on its throne, its trade memory read again from the body, a
  den found empty starts with an empty pool (§4.10). Its slots start owed, with new generations, and
  the bodies, strays to the new books, fill them.
- **What an uninstall or reset loses:** members away whose bodies are gone, members owed a body, the
  grudges, which are the sim's own, payments to the Umvuthi not yet written into its body on the throne
  (§4.5), and the throne itself from a chief's death away until its heir is seated; and, at a crash,
  the Sol Visage of a chief puppet in its 115-tick death animation (§4.5). The other way round, a
  member that set out while its camp was out and died away comes back with an uninstall if its camp has
  not come in since (its body is removed at that census), as its body was still in the saved camp: the
  jar's state wins, which UR-1 allows. Since a census takes every body left behind (§4.16), a member is
  exposed for the rest of its march once its camp has come in while it was out, and for the whole march
  when it set out from a camp that was in: so is the Umvuthi (MZ-15, §4.5). Markers not yet
  acknowledged stay as inert `minecraft:marker` entities (no tick, never sent to a client); a datapack
  that selects every marker would see them.
- **Untested so far for any module:** the deploy rehearsal's uninstall had no puppets to check, as no
  player joined (`docs/deploy-rehearsal-live.md:461-462`); dragons carry the same gap. Test 4
  covers this plan's bodies.

**Walked through** (walkthroughs A, B, F and G, §5; one row per change):

| When | What happens | The slots | Order, and its answer |
|---|---|---|---|
| A, day 0 | the den comes in as Iva scouts; FACTION_FOUND counts 1 | slot 0 owed, and the jar's Frostmaw, a stray, fills it | adopt; done: home, body |
| A, 7.5 | the raid musters; the den is out | slot 0 away, left | none |
| A, 8.5 | the muster ends; the den is out | as it was; the party marches abstractly | none |
| A, 9.5 | the den comes in as Iva rides north | slot 0 away, left, its party out | take; done: away, gone, a marker holding the record |
| A, 9.5, kill it | its puppet dies by the line | slot 0 freed, no body to remove; the heir succeeds; the pool is 0 | none |
| A, 21.5 | the wait ends, 12 days after a first kill (§4.2); the den is out | slot 0, a new generation, owed | none |
| A, 21.5 to 23 | her trains still run: the new Frostmaw musters and sets out | drawn owed: away, gone, with its debt | none: nothing to take, nothing to spawn while it is away |
| A, its return | `home` | owed | a spawn at the den's next census (at once if the den is in), held while a player is within 160 blocks; done: home, body |
| B, step 1 | the grove comes in; FACTION_FOUND counts 7 | 7 owed, filled by the 7 strays | 7 adopts; done |
| B, step 2 | Tomas kills four (CAMP_DEATH by slot) | 4 freed; the pool is 3 | none |
| B, step 3 | the pool refills while the grove is in | each whole warrior a slot owed | a spawn at the far edge; done |
| B, step 4 | a raid musters; the grove is in | 4 drawn with bodies: away, left, standing in the grove | none |
| B, step 4 | the muster ends; Tomas, 150 blocks off, sees it set out | 4 away, left | set out; done: 3 handed off, 1 taken for the raptor pack: away, gone |
| B, step 4 | a warrior and the pack die | 2 freed | none |
| B, step 4 | the two survivors walk home with Tomas following; `home` with the grove in | 2 away, gone, their puppets at the grove | 2 hand-backs; done: home, body |
| B, step 5 | a raid musters, sets out and comes home; the grove is out throughout | 4 away, left; at `home`, home, body | none |
| B, step 5 | Tomas comes back; the grove comes in | every body found | none |
| F, step 1 | the Invasion's muster ends; the grove is out | 6 warriors', 2 packs' and the Umvuthi's slots away, left | none |
| F, step 3 | Nadia meets the column on the road; the band is embodied where it stands | as they were | CHIEF_BODY, then the chief's stack, its puppet aloft; no reconcile, the grove is out |
| F, step 5 | Nadia kills it at her farm (CHIEF_KILLED) | the Umvuthi's slot dead, left; the heir succeeds; the warriors within 30 blocks die (BAND_CASUALTIES); the summons' deaths book nothing | none |
| F, step 6 | the survivors walk home and reach `home`; the grove is out | the survivors home, body; the slots of warriors killed at her farm dead, left | none |
| F, step 7 | a week later she rides to the grove; it comes in while she is about 200 blocks off | the dead, left slots' bodies found, the throne body among them | removes: the warriors' at once, the throne body's held under Rule U while she is within 160 blocks of it; done: freed, the Umvuthi's slot owed |
| F, step 8 | the heir's wait ends; the grove comes in later | the Umvuthi's slot owed | a warrior's removal and the chief's spawn at the throne spot, under Rule U; done: the warrior's slot freed, the chief home, body |
| F, step 8 | had she stayed by the grove, the old body's removal still held as the wait ends | the Umvuthi's slot dead, left | a warrior's removal and the heir seated in place of the old body, at once unless that body is fighting her, then once the fight ends (§4.6); the held removal answered stale; done: the warrior's slot freed, the chief home, body |
| F, step 9 | had she not killed it: Tomas is within 160 blocks of the throne as the grove comes in during the march | the Umvuthi's slot away, left | a take, held while he stays; done when he leaves: away, gone |
| F, step 9 | then the Invasion walks home with Tomas still there | the take still held | answered stale as the Umvuthi's puppet comes within 32 blocks of the throne; a hand-back puts one body on the throne; done: home, body |
| G, step 2 | the Invasion's muster ends with the grove in; Ines watches from a ridge 120 blocks off | 6 warriors', 2 packs' and the Umvuthi's slots away, left, standing in the grove | set out: 6 handed off, 2 taken for the packs, the throne body handed to the Umvuthi's puppet, which rises from the throne; done: away, gone |

### 4.10 Renewal: heirs, migrants and regrowth

Ben asked for populations that come back and settle into a balance. EQUILIBRIUM.md covers every species
with one shared rule (§2.2 there: a pressure P per camp, halving every 96 days and gaining 1 for each
loss to players, and a delay of `min(cap, base × 2^P)`); its input is
`research/population-inventory-2026-10-21.md`. These two factions plug into it with the rules below;
every rate it has not set is TUNABLE.

| What renews | How | Rate (TUNABLE) | On screen |
|---|---|---|---|
| A chief (the notable) | `succeed`, at once, holding the grudges (§3.3) | immediate | nothing until a body exists |
| A Frostmaw (the body) | one booked arrival after a death, or for a den found empty: a migrant moves into the empty den (ledger 6) | `min(192, 6 × 2^P)` days (EQ-4 (b), Ben, 2026-10-21): 6 with no player cause, 12 after a first kill, about 52 when farmed | asleep in the den, holding its crystal (MZ-4 (a)), at its next census while it is home, once no player is within 160 blocks (§4.9) |
| Grove warriors | the pool refills toward the grove's own cap | `recruits_per_day` 2 (goblin's): an emptied grove of 7 is whole in 3.5 days | walking in from the grove's edge: if the grove is in, as each whole body is ready, else at its next census (§4.9) |
| The Umvuthi (the body) | the heir takes up the mask: after the later of the cowed time and the wait, a warrior's body is removed and the chief spawned at the throne spot (§4.6); none while no warrior is home | `min(192, 6 × 2^P_chief)` days (MZ-14 (a), §8), P_chief gaining 1 for each Umvuthi lost to players; the cowed time is 6 days scaled by boldness | on its throne at the next census, under Rule U (§4.5) |
| Raptor packs in war parties | from the pool, as warriors are | as warriors | only in parties; a scouting pack is a fresh puppet, drawn from no pool and gone when it ends |

Natural Mowzie spawns (raptor packs, Naga, Foliaath, Elokosa, Lantern, Grottol) are the jar's own and
outside the sim; a grove's census never counts a natural raptor.

**Loot supply** (MZ-4 (a)). Each heir is one more ice crystal or Sol Visage: a Frostmaw comes back
holding its crystal, and an Umvuthi drops its Sol Visage when a player kills it. A den farmed as fast
as it refills, each Frostmaw killed 2 days after it arrives, settles at a pressure of 3.11 and a 52-day
wait, a kill every 54 days, about 1.8 crystals a game year (96 days): 1.79 by EQUILIBRIUM.md's closed
form and 1.83 in its model (§3.3 there). A grove gives Sol Visages at the same rate with MZ-14 (a), and
about 1.0 a year with (b). Stealing the crystal starts no wait: the robbed Frostmaw stays in its den
(§4.7), and only its death brings an heir, with a new crystal, so stealing then killing still yields
one crystal per heir. The stretch is felt at once: after a first kill the next Frostmaw is 12 days off;
killed again 2 days after it arrives, the one after is 22 days off, then about 36 and 47, settling
near 52.

**What this plan needs from the pan-species design:**
1. **Caps.** A per-camp cap (this plan sets 1 per den and the census count per grove) and whatever
   world-level ceiling it sets per species.
2. **Rates against pressure.** `recruits_per_day` for each culture tuned against how fast players kill
   them, so dens and groves neither vanish nor swamp a region; the Frostmaw's is now EQ-4's, and the
   tribe's 2 a day is still a guess (EQUILIBRIUM.md §2.6 item 2).
3. **The empty camp rule.** One rule for a camp found empty or emptied for good (a migrant after a
   delay, as dragons do), shared with the dragons' vacant lairs.
4. **Predator and prey.** Raptor hunts and village plunder take animals and livestock goods; the herds
   module's populations and the villages' stocks must account for them in the same balance.
5. **A census in `sim`.** `rtsim-cli sim mowzie` reports dens with a body, groves' pools and the
   animals taken per day, in the form the pan-species design measures every species.

### 4.11 Relations

- **With each other:** none. Snow and savanna do not meet; neither culture hates the other.
- **Alliances** (CONFLICT-LIFE.md §2.16, CL-5 (b)). The Frostmaw never swears: a council of one has no
  one to swear with. The grove may swear with a camp that also quarrels with a player, most often a
  goblin camp (both hate humans): in the measured fortress world a grove and Mudhollow swore against
  Ben about once in 270 world-years (12 pacts in 64 worlds of 50 years).
  Its raid can be called; its invasion goes only on the Umvuthi's own wounded pride (§4.5), which no
  ally counts on, and then with the Umvuthi aloft at the rear of its own column. A goblin band and the
  masked warriors of one joint attack do not turn on each other: the ConflictBridge cancels a change of
  target to a body of the same attack (`LivingChangeTargetEvent`; CONFLICT-LIFE.md CL-D50). The
  Mowzie bodies retarget through `setTarget`, which fires it (verified in the jar):
  `UmvuthanaHurtByTargetAI.canContinueToUse` ends in `this.mob.setTarget(livingentity)`, a follower
  raptor takes its leader's target with `setTarget` (`EntityUmvuthanaFollowerToRaptor:48`), the
  Umvuthi hands its target on with `setTarget` (`EntityUmvuthi:1093`), and none overrides
  `isAlliedTo`. The conflict-life GameTest 5 runs on them.
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
  losses with no fight possible would be unfair. Councils still sit, and a pact may be sworn (words
  between camps); no ally is called and no joint party leaves (CONFLICT-LIFE.md §2.16, CL-D53).
- **No notice carries a line of text** (CONFLICT-LIFE.md §2.17, CL-D56; Ben, 2026-10-22: "All faction
  news in-world"). The roar and the gong are cues in the world and stay, kept until shown as above.
  A pact against a player, and every other pact, a feud, a raid and a death, reach him as news that
  traders, caravans and villagers carry; ENVOY_MATERIALIZE carries the ally's actor (8 bytes) so a
  joint envoy can name it in person, and FACTION_NOTICE carries nothing new.

### 4.13 Strength

The engine's strength unit is one ordinary mob: an observed fight counts one per attacker today
(`core/industry/Threats.java:384-385`). The plan's rule (TUNABLE): health × attack ÷ a zombie's (20 × 3).

| Body | Health × attack | Strength |
|---|---|---|
| Frostmaw | 250 × 10 (`EntityFrostmaw.java:291`) | 41.7 |
| Umvuthi | 150 × 2 (`EntityUmvuthi.java:361`) | 5.0 (understated: its damage is its sun abilities); added to its band's strength while it leads (§4.5, ledger 4) |
| Umvuthana warrior, crane, follower | 8 × 3 (`EntityUmvuthana.java:451-452`; one attribute set, `EntityHandler.java:244`, `247-248`) | 0.4 |
| Raptor | 10 × 6 (`EntityUmvuthanaRaptor.java:97-98`) | 1.0; a pack of 1 and 4 followers 2.6 |

Measured in GameTests (§7) against reference bases through `IND_FIGHT`, with the strength hook (ledger
19). L2 Hostility may scale these mobs' health; UNVERIFIED.

### 4.14 Tunables and placeholders

| Value | Here | Source | What it tunes | How to measure |
|---|---|---|---|---|
| Territory radius | 200 | TUNABLE: half the structure spacing | how far a camp's range reaches | GameTest census of den spacing in a generated world; Ben's call |
| Rail weight, tolerance | 1.0, 2 legs a day per den and actor | PLACEHOLDER | how much traffic a Frostmaw stands | `sim mowzie` sweep of legs a day against days to the first raid; Ben's call on the target |
| Meter τ, band, resync; held pass | 2 days; 10% and at least 1 leg a day; a day; 1 day | PLACEHOLDER | how fast changed traffic is felt; pushes a meter costs; how long a pass waits for its line | `sim mowzie` cut, reroute and trunk runs; push counts |
| Sample period, hash cell | 20 ticks, 512 blocks | PLACEHOLDER | sampler cost and pass accuracy | bench at 200 trains; test 11 on the fixture's curves |
| Lurk offset, reach | 5 blocks off the rail, 8 blocks | PLACEHOLDER | where it stands; which passes count | test 9 |
| Repeat | 4 days after a return | PLACEHOLDER | how often an angry Frostmaw comes back | `sim mowzie` line run; Ben's call |
| Frostmaw carry | 64 | industry's siege carry; MZ-5 (a), Ben, 2026-10-21 | cargo lost per ambush | decided |
| Renewal | Frostmaw `min(192, 6 × 2^P)` days, H 96; Umvuthi heir `min(192, 6 × 2^P_chief)`; tribe 2 a day | Frostmaw: EQ-4 (b), Ben; Umvuthi: MZ-14 (a), Ben; tribe TUNABLE | heir and migrant arrival; regrowth | EQUILIBRIUM.md's model; `sim mowzie` renewal runs |
| Thresholds, dwell, half-life, patience, muster, cowed | goblin's | PLACEHOLDER | pace of the ladder | `sim mowzie` |
| Tribute | 0.5 emerald's worth a point; fallback 9 nuggets an emerald | goblin's; gold nuggets by MZ-6 (a), Ben; PLACEHOLDER rate | the price of peace | the grove market's gold price, once it exists (ledger 12) |
| Party sizes | §4.5 | PLACEHOLDER: a grove of 5 can field each | raid strength | GameTest fights |
| Strength rule | health × attack ÷ 60 | TUNABLE | Lanchester inputs | GameTest fights through `IND_FIGHT` |
| Animals owed per raptor pack | 2 | PLACEHOLDER | herd loss to a won raid | GameTest: animals killed in a watched raid |
| Notice and cue distance | 32 blocks toward the den; cue within 64 of a station | PLACEHOLDER | where the roar or gong comes from | listen in game |
| Aloft (MZ-16 (a), Ben) | 2 blocks over the highest `MOTION_BLOCKING` block under its box and 8 blocks ahead; rise up to 0.1 and sink up to 0.05 blocks a tick; pace up to 0.1 blocks a tick; settle within 32 blocks of the target with the attack flag, or on a player target in sight; rise once the flag has dropped and it has had no player target for 200 ticks; height read every 10 ticks; over a fluid, fight where it floats only within 8 blocks of the target; re-pair when 55 degrees off, at most once in 200 ticks; stuck after 100 ticks with no headway more than 8 blocks from its place, then up to 24 blocks higher, then moved out of sight or after 1,200 ticks; the trail every 10 ticks, on | PLACEHOLDER | how high and how closely it keeps to its band, where and when it settles and rises, how often it turns, and whether it reads as borne by its sun | tests 23, 31, 35 |
| The column | two abreast, 1.5 blocks a row, laid back from the march's position; raptor packs beside, the chief at the rear | PLACEHOLDER; the DH note's column | where a band met on the march stands, matching the far view | test 34 |
| Pending stacks | a stack not wholly entity-ticking is tried again at its column position when a chunk it waits on changes its entity-ticking state or its place moves into another chunk; at a stop it walks in from the edge and holds one chunk inside it | PLACEHOLDER; MZ-17 (a) | how soon a stack appears once it is inside the entity-ticking area | test 33 |
| March pace | `march_blocks_per_tick` 0.02 (`culture.rs:289`) | goblin's, PLACEHOLDER | how long a march takes, and how a column reads in sight | test 34; `sim mowzie` run 13 |
| Rule U's spawn wait | 6,000 ticks | PLACEHOLDER | how long an Umvuthi's spawn at its throne waits for a player to leave (its takes and removals wait with no limit) | test 29 |
| Throne fallback | the fire pit, direction 1 | PLACEHOLDER | an heir at a grove found without its Umvuthi | test 30 |

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

### 4.16 The march: seen coming (MZ-15)

Ben (2026-10-21): "I feel like it should be the grove / whoever they send 'walking' towards the player
instead of it just not working. (I.E. Possibly being able to see them coming via DH.)" So every war
party of both cultures, the Frostmaw on its raid and the tribe's bands with the Umvuthi aloft at their
rear, is one march with one position at every tick, from its camp to its target and back. Where a player
is, the march is bodies; beyond, the engine carries it, and a Distant Horizons far view
(`research/distant-horizons-2026-10-21.md` §4.6, the DH note) can draw it from the same record. This
plan exposes the position; it draws nothing. Ben's answers to the note bound that drawing: DH-1 sends
sim state only to clients that opt in, within their DH radius; DH-2 lets the far view show only what
eyes would see from where the player stands, so a column of walking figures and one seated figure
floating at its rear, never the record's counts, faction or names; DH-5 retires the note's hidden
Umvuthi seat for this march.

**What the engine already knows.** At departure every party has its whole route, waypoints on the road
graph with the tick at each (`Party`, `engine.rs:199`; `route`, `2131`), and `party_pos` gives its
position at any tick in closed form (`2400`). The route changes only at events: the muster's end,
arrival, lurking, a rout, the walk home. Java learns nothing until a player meets the band
(BAND_MATERIALIZE, `docs/boundary.md` 0x8803).

**The record: BAND_MARCH** (0x880B, engine to Java, provisional), one per change of a party's march. It
carries what the DH note's route record needs (its §4.6 and §6.3) and what the plugin needs to lay out
bodies:
- agent, a revision (each record replaces the agent's earlier ones), faction, culture, party kind,
  dimension;
- phase: mustering, out, waiting, fighting, home;
- flags: the chief leads, ended, joint;
- the partner's agent: for one contingent of a joint attack (CONFLICT-LIFE.md §2.16), the other
  contingent's party, else none;
- the muster's end tick (its departure);
- the unit counts by body kind (a raptor pack its own kind, of 5 bodies);
- the route from the segment the party is on to the route's end, as waypoints with their ticks: the
  first is `route[seg]` at `times[seg]`, unchanged and unrounded, in chart coordinates
  (`engine.rs:694-715`), not the party's position at the record's tick; a party that waits or fights
  has one waypoint and the tick it leaves.

It is 72 bytes plus 8 per body kind and 16 per waypoint: 408 for 20 waypoints and two body kinds, 416
with three. `route` (`engine.rs:2131-2205`) puts no cap on its waypoints, so a long route makes a
longer record, 16 bytes a waypoint. The engine sends it when a party is sent (mustering, `send_party`,
`engine.rs:2224`); at each change of route or phase; when a casualty changes its counts, at most one a
tick per party; with the ended flag when the party ends at `home` or loses its last unit; and again for
every live party at a session start, as conflict commands are per session
(`docs/boundary.md:1463-1464`). The ConflictBridge keeps the latest per agent and drops it at the ended
flag. The position at any tick is the waypoints interpolated by time with the engine's own formula
(`party_chart_pos`, `engine.rs:694-715`: in f64, truncated to whole blocks), then folded by the world's
topology, which Java has from TOPOLOGY (`docs/boundary.md` §11), as `party_pos` folds (`2398-2403`): as
a cell (`crates/rtsim-space/src/topology/mod.rs:239-248`). `LevelTopology.fold` (`115-150`) folds a
point, and on a glued seam, which turns, a cell's corner and its centre can land a block apart, so Java
folds the cell's centre and floors the result. That is `party_pos` to the block: the far view and the
bodies read one source.

**Where the bodies stand.** A band's position is the head of its column. Its bodies stand two abreast,
1.5 blocks a row (PLACEHOLDER; the DH note's column), laid back along the route from there, its raptor
packs beside the column, its chief, when it leads, at the rear. The plugin spawns a band met on the
march in that layout (`arcRadius` 0, with BAND_MATERIALIZE's bearing as the heading), so its bodies
appear where the far view drew the column. A column of 16 bodies is about 12 blocks long. The chief
goes last so that a column coming toward a player brings it in last and farthest: the Umvuthi is the
one body of a band that a client draws beyond 128 blocks (below).

**What changes in the engine** (ledger 23):
- **The walk home has bodies.** `seen` embodies a band walking home as it does one going out, steering
  it waypoint by waypoint toward its camp (today it skips the Home phase, `engine.rs:3608`), and
  `book_unseen` (`2576`) books where that walk leaves the player's area too. `go_home` (`3103`) steers
  an embodied band home with BAND_STEER instead of recalling it (`3106-3110`), its route home starting
  from `party_chart_pos`, not the folded `party_pos` (ledger 23), so a band seen at its target, or
  routed there, walks home in view. A mustering band is still skipped: its members are the camp's own
  bodies, standing in it (§4.9).
- **An embodied band that arrives keeps its bodies.** `start_fight` (`3307-3350`) bumps the epoch and
  spawns the band again on the arc 112 blocks out. For a band already embodied it keeps the puppets,
  turns them on the target (BAND_STEER flag 1, attack), and sends as waves only the members not yet in
  Minecraft.
- **Recalls only with nobody near** (ledger 23). `unseen` and `player_left` re-bind an embodied band to
  the nearest player within 192 blocks, counting every player at their current position, the one it was
  seen by included, before they recall it; a fighting band whose player leaves still turns to waiting,
  and a siege is recalled there as today. An embodied band that arrives with no player within 160
  blocks of its target but one within 192 of it is not recalled: it resolves abstractly and `go_home`
  steers it home (a siege is recalled at its arrival as today; no chief leads one). `lose_units`
  lowers `spawned` too while a band is embodied, so its abstract dead are not counted twice at its
  recall. The watched fight keeps CL-D33's 160 blocks. Rule U (§4.5) rests on this.
- **The ticking edge** (Java, ledger 25). Puppets live only where entities tick, within the simulation
  distance (`DistanceManager.java:252-253`; level 31 or lower, `ChunkLevel.java:10-12`). The core's
  registry rejects a group with any member outside it (`NOT_TICKING`, `RtsimSession.java:533-535`),
  takes back one with a member that leaves it (`350-354`) and refuses a second group for an agent
  (`528-530`); HerdBridge's own groups follow the same rule (`HerdBridge.java:447`, `662-672`); and
  today one rejection of a band's first wave clears its embodiment for good (`engine.rs:3924-3931`). A
  band held as one group could not come in while its column straddles the edge. So the ConflictBridge, a
  plugin that keeps its own groups as HerdBridge does, makes each stack its own group, held pending
  until it is wholly inside (MZ-17 (a), Ben, 2026-10-22: capture per stack, amends C25): a
  column fills in stack by stack as it walks inside, and one walking out empties the same way, while the
  engine sees one embodiment. When the band stops to wait or fight, a stack still pending walks in from
  the edge, as members past the spawn quota do (CL-D35), and holds one chunk inside the edge until the
  band leaves its stop, so none stays out for the whole fight, and none is placed twice unless the
  player moves more than a chunk away. A pending stack is tested only when a chunk it waits on changes
  or its place moves into another chunk (ledger 25). An envoy is one group: it spawns at one point.
- **The chief's stack** at every embodiment (ledger 4, §4.5).

**What a player sees, and the seams.**
- **The far view** draws the column from the record: about 1 km, which at 0.02 blocks a tick is about
  two Minecraft days of warning (the DH note's §4.6). Its proxy should hide only once the client has the
  puppets: the note's other test, the band's chunk loaded on the client, would hide it in the ring
  between the client's loaded chunks and the entity-ticking edge, where it has no bodies yet.
- **Meeting the band.** A band is met where its route comes within 160 blocks of a player
  (`view_radius`, `tunables.rs:91`). Entities tick in the chunks within the simulation distance s of the
  player's chunk, counted in chunks along each axis (`DistanceManager.java:252-253`), so the edge is 16s
  to 16s + 16 blocks off along an axis and up to about 1.4 times that on a diagonal: 160 to 176 blocks
  at s = 10, vanilla's default, and 128 to 144 at s = 8, as on the te-perf server. Each stack appears
  the tick its last member's block comes inside (ledger 25), so at the edge; a raptor pack's nearest
  body can be a few blocks inside it. At the default entity distance a client draws a body within its
  bounding box's size times 64 blocks (`Entity.shouldRenderAtSqrDistance`, `1692-1700`): a warrior
  within 72.5 blocks, a raptor within 81 and the Umvuthi within 132. So no warrior or raptor is drawn as
  it appears, at either distance: they walk into sight. The Umvuthi, last in the column, appears 160 to
  176 blocks off at s = 10, beyond its draw distance, and 128 to 144 at s = 8, where it can be seen to
  appear 128 to 132 blocks off on an axis, never nearer while chunks keep up (after a teleport, or under
  fast flight with Aeronautics or Sable, chunks can become entity-ticking late, and a stack then appears
  nearer); with the far view its box becomes its body where it stands. A client with its entity distance
  above 100% draws farther, up to each body's tracking range (warriors 128, `EntityHandler.java:94`; the
  Umvuthi 160, `97`): its warriors are still sent only within 128 blocks, so they walk into sight, and
  at s = 8 it can see the Umvuthi appear 128 to 144 blocks off.
- **A band that reaches a watched target with no bodies** (a player who logs in or teleports there as it
  arrives) comes as today's waves on the arc 112 blocks out, the Umvuthi aloft with the first: the one
  case where it appears in sight. The conflict-life design's walk-in (EQUILIBRIUM.md §2.2, `:224-240`)
  replaces the arc: placed at the farthest ticking point on its route back from the target, the band
  walks the rest, so it too appears at the edge, and holds inside the area as a pending stack does
  (ledger 25).
- **The spawn quota** (16 units, `tunables.rs:94`) is kept and never binds a Mowzie band (§4.5). A
  bigger band of another culture keeps its whole strength in the fight's books, and its members past
  the quota stay in the record and walk in along its route from the ticking edge as the quota frees
  (CL-D35, CONFLICT-LIFE.md §2.12), holding inside the area (ledger 25).
- **The pace.** 0.02 blocks a tick (goblin's, PLACEHOLDER): a band crosses the 160 blocks of a player's
  area in 8,000 ticks. Bodies walk to a steered restriction centre (§4.7), so in sight a band advances
  in short walks; how that reads at this pace is UNVERIFIED (test 34).

**The census never ends a march.** A camp that comes in while its party is on the road (out, waiting,
fighting or walking home) has the bodies that party left behind taken, and the party marches on (§4.9);
it ends only at its own `home`, or when its last unit dies. Reason: until MZ-15 a census ended a party
whose bodies it found unless a player was nearer the party than the camp, so a visit to the camp cut a
raid short, and the band on the road, or in the far view, vanished. The cost is UR-1 exposure (§4.5,
§4.9).

**The walk home.** At its camp a band ends through `home`. If the camp is in, its puppets are handed
back (§4.9): each becomes the camp's tagged vanilla body where it stands, the Umvuthi on its throne. If
the camp is out, its puppets are recalled as today (BAND_RECALL, reason 3, done) and the camp's next
census places its members.

**What the far view can draw.** The record carries the phase, so the far view can draw a band mustering
at its camp and one walking home as well as one going out; the DH note's §15.2 drew neither, because
neither had bodies. A mustering band's bodies are the camp's own, standing in it whenever the camp is
in, so its proxy should hide where the client has the camp's chunks (the DH note's chunk test), not
wait for puppets; a band walking home now has bodies where a player is, as one going out does.

**Agreement with the conflict-life design** (EQUILIBRIUM.md and CONFLICT-LIFE.md on main at `72b439a`,
read, not edited):
- CONFLICT-LIFE.md §2.7 decides only when the Umvuthi leads (wounded pride of 0.8 or more, CL-D17) and
  §2.12 takes its march from §4.5: its 5.0 is in the band's strength, 12.6 for a full invasion (7.6 and
  5.0), as here (§4.5, §4.13), and missing from the grove's defence until it is home.
- Its rule that nothing appears within 128 blocks of a player (EQUILIBRIUM.md:214-222, `1106`) holds
  for a band met on the march at a simulation distance of 8 or more on an axis approach, while chunks
  keep up, but for a raptor pack's nearest body a few blocks inside the edge, which no client draws
  there; the Umvuthi, the one body drawn beyond 128 blocks, goes last and appears at the edge (above).
  EQUILIBRIUM.md §2.2 (`:224-240`, `1106`) and CONFLICT-LIFE.md §2.12 and CL-D33 carry these numbers,
  with the nearest walk-in point at least 160 blocks off on an axis at 10 and at least 128 at 8.
- Frostmaw, Umvuthana and Umvuthi stay vanilla at home and are puppets in parties (EQUILIBRIUM.md:329),
  as here.
- Members away at an uninstall are lost and booked Other at a reinstall (EQUILIBRIUM.md:376-378), as
  here; MZ-15 lengthens that exposure (§4.5, §4.9).
- **The census and the march.** EQUILIBRIUM.md §2.6 item 11 (`:695-703`) now follows MZ-15 (CL-D42): a
  census takes the bodies a party on the road left behind, never recalls its living members, and a
  march ends only at its own home or at its last death, as here.
- Its model leaves out the march out, which adds days before a fight, and models the walk home:
  survivors are away 1 to 3 days, and until they are home the camp's defence lacks them and a grove the
  Umvuthi's 5.0 (CONFLICT-LIFE.md §2.9, §4.4). Here the march out has bodies too, and a camp visited
  while its band is out has the band's bodies taken, so it is that much emptier until the band comes
  home.

## 5. Walkthroughs

**A. A rail line through a Frostmaw's range.**
1. Iva mines copper in the snowy slopes. She lays Create track 1,800 blocks south to her smelter town,
   with a station at each end, "Copper Halt" and "Smelter". The track bends around a ridge and runs 150
   blocks from a Frostmaw den she has never seen. The den's chunk loaded once when she scouted the
   route: the plugin reported FACTION_FOUND with its census (one Frostmaw, holding its crystal), and
   tagged it. The Frostmaw sleeps on as the jar's own entity.
2. Day 0: one train shuttles between the stations, 12 legs a Minecraft day. Every leg's leading point
   enters the den's circle; the sampler reports a RAIL_PASS each time, and the pass counts at once
   on the den's meter for Iva, the train's owner (MZ-13 (a)); each arrival also credits the line
   {Copper Halt, Smelter}, which makes it the focus. The meter climbs, and its first pushes set the
   Rail source against Iva.
3. Day 2: she adds a second train: 24 legs a day.
4. Day 2.7: the grievance passes 10. She is at the smelter, outside the range, so the notice waits.
   The next morning she rides north and, as her train enters the range, hears a roar from the north.
5. Day 7.5: the grievance passes 70; the Frostmaw climbs from Notice straight to Raid. After its 1-day
   muster it leaves the den (the den is out, so nothing is ordered: its vanilla body stays in the saved
   chunk, its slot away with the body left, and the party marches abstractly), walks 150 blocks at 0.02
   blocks a tick (7,500 ticks, under a third of a day) and waits 5 blocks off the rail at the point of
   the real track nearest its den, from day 8.8 to day 10.8. It is home on day 11.1.
6. She is at her smelter, 1,500 blocks away. Each train passing within 8 blocks of the spot rolls; a
   hit takes up to 64 ingots and at most 10% of what the train carries, destroyed, at most once a
   train a day. Over the two days her two trains lose at most 256 ingots. When a robbed train pulls
   into Smelter, she hears the roar from the north. No track is cut.
7. Day 9.5: she rides north. Cresting the ridge, she sees the Frostmaw beside the line, awake (it
   appeared as she came within 160 blocks of its spot; whenever the den comes in, before or after that,
   its census takes the body left in it, and the lurk goes on, §4.4). It throws an ice ball well ahead
   of her train; it bursts beside her seat, she freezes, and she is pulled off the carriage. For 2.5
   seconds she stands frozen in the snow next to the Frostmaw while her train runs on to Copper Halt.
   As the second carriage passed, it swiped; the carriage's chest is short when it arrives (the same
   booked roll as an unwatched pass).
8. Her choices:
   - **Kill it.** Her bow and her trident do nothing; she brings a TaCZ rifle and fire. It falls after
     250 damage; she gets an ice crystal (it still had its crystal and a player killed it). Her feud
     drops to Notice, the faction is cowed for about 6 days, and the heir takes over at once, holding
     the old grievance plus about 2.5. The den's pressure goes from 0 to 1, so the new Frostmaw arrives
     6 × 2 = 12 days later, on day 21.5, holding its crystal (MZ-4 (a), EQ-4 (b)), and is placed asleep
     in the den when the den next comes in (walkthrough D). If her 24 legs a day still run, the
     grievance is still over 70 when it arrives, long after the cowed time, so it musters at once and
     is by the line about 1.3 days later. If the den has not come in since, it sets out still owed a
     body and carries the debt: nothing is taken, no one is booked dead, no second Frostmaw appears,
     and its body is placed in the den when it comes home (§4.9). Killing buys about 12 days, and each
     kill buys more: killed again 2 days after it arrives, the next is 22 days off, then about 36 and
     47 (§4.10).
   - **Steal its crystal first.** She drinks a potion of invisibility and walks into the den while it
     sleeps (a sleeping Frostmaw drops a target that has invisibility, `EntityFrostmaw.java:597-599`).
     She stands by the crystal in its fist: it is hers, and the Frostmaw wakes without it. The plugin
     sets it persistent, so it stays in its den instead of despawning when she leaves, and reports its
     state. From now on it has no breath or ice balls, so riders are safe from the dismount; it counts
     as melee, and a 2-block wall at her station stops it while she is away.
   - **Run fewer trains.** She cuts back to 4 legs a day on day 12. The grievance peaks at about 122
     on day 13; the raid due on day 15.1, 4 days after the Frostmaw came home, still comes; the
     grievance falls under 70 on day 21.4 (no more raids) and under 42 on day 28.1, when the feud
     drops back to Notice. Stopping the line entirely gets there by day 23.6.
   - **Reroute.** She lays new track west around the range. Her trains' passes stop, and the old
     line's meter decays as if she had stopped it. A halt station with no new track changes nothing:
     the range feels the track, not the stations.
   - **Defend the station.** If she changes nothing, the grievance passes 140 on day 15.2, just after
     the repeat raid was sent (day 15.1); a feud climbs only with no party out, so when that raid is
     home, on day 18.7, it comes to wait by Copper Halt, the station nearer its den. That order rests
     on a 0.1-day margin: in 1 or 2 of 32 jittered runs (by model), and in every run with perfectly
     regular trains (day 14.75 to 15.06 by the trains' phase), the grievance passes 140 before the
     repeat raid leaves, and
     the Invasion comes then, on about day 15, instead of the repeat raid (§4.3 gives the same caveat
     for its 24-leg row). A Create Big Cannons autocannon on a Create Radars controller with
     auto-target and auto-fire counts against every class (I21), and its shells hurt the Frostmaw
     without giving it anyone to turn on; a 2-block wall counts only once the crystal is gone; an iron
     golem counts as a guard.
   - **Pay.** The Frostmaw takes no tribute. Running the trains and accepting the losses is the price.

**B. A savanna base near a grove.**
1. Tomas builds a windmill and a cattle ranch 150 blocks from an Umvuthana grove, inside its 200-block
   range. The grove came in when he arrived: the census counted 7 warriors and tagged them, so the
   tribe's pool and cap are 7.
2. While he scouts, four masked warriors chase him and he kills them (killing weighs 8 each: 32).
   Each death is a CAMP_DEATH: the pool is 3. That evening he hears a gong.
3. Half a day later (Notice's dwell), his grievance has decayed to 30.2 and the tribe climbs to Envoy:
   the demand is made then, at 0.5 emerald's worth a point, 9 nuggets an emerald, rounded up: 136 gold
   nuggets, due 3 days later (goblin's `demand_days`, middling temperament). With no muster (an
   envoy sets out at once, `engine.rs:2259-2263`) and the 150-block walk (7,500 ticks, 0.31 day), about
   0.8 days after the kills, an envoy and two guards walk into his ranch. His iron golem watches them
   pass; the guards ignore him. The envoy's name shows "136 gold nuggets". He right-clicks it with 136
   nuggets (about 15 gold ingots): the demand is settled, 70% of the grievance goes (`tribute_relief`),
   and the tribe cannot climb for 6 days (`respite_days`).
   When he next visits the grove, his nuggets are in its chests, and new warriors have walked in from
   its edge (the pool refills at 2 a day).
4. He clears acacia for his ranch, day after day, and kills the warriors who come at him unmasked.
   Weeks later the grievance passes 70 and a war band musters: three warriors and a raptor pack, drawn
   from the warriors whose bodies stand in the grove. While he is home the grove is in (every grove
   chunk is within 11 chunks of his ranch's, inside the default view distance plus 2), and at 150
   blocks he is within 160 of it, so the band is seen as it sets out: three warriors walk out of the
   grove as its puppets in their own bodies, and the one warrior the raptor pack stands for is taken
   (CAMP_BODIES: done). The band comes at his ranch. The raptors go for his cattle, the warriors for
   him, and his iron golem for all of them. Two warriors live; when the fight ends they walk home, and
   he follows them the 150 blocks to the grove, a third of a day at their pace. At `home` each is
   handed back where it stands, a tagged vanilla warrior of the grove again (CAMP_BODIES: done), and
   nothing appears or vanishes in front of him. Had he left on a trip before they got home, their
   puppets would have despawned behind him once he was out of range, the grove would be out, and
   nothing would be ordered: the two would stay owed, and the census when he came back would place
   them, with no one booked dead.
5. He goes on a trip. The next raid musters while he is away and the grove is out, so nothing is
   ordered, and the warriors' bodies stay in the saved grove, their slots away with the bodies left.
   His ranch has a fence all the way round, so the ground band is stopped at it (I22): nothing happens,
   and it walks home, where its members are home again in the bodies they left, with nothing to spawn.
   Had the fence a gap and the band won, the raptors would owe him dead cows, and he would find them
   dead when he got back. When he comes back, the grove comes in as he nears the ranch, and the census
   finds every body its slots expect: nothing is ordered and no one is booked dead.
6. He finds a Fury mask on a dead warrior and wears it. The tribe's bodies no longer target him; only
   a warrior he has provoked still does. He sells bones to the grove's warriors for nuggets: the
   tribe's grudge softens a little with each trade.
7. Or he ends it: he walks into the grove unmasked and fights the Umvuthi on its throne, 18 blocks from
   the fire pit. It summons warriors while fewer than 4 are within 50 blocks of it, and cranes each time
   it loses 45 health; while a crane shields it with Sunblock his shots do nothing, so he shoots the
   cranes first. When it dies, every warrior within 30 blocks of it falls within a second, most of the
   grove: each is a CAMP_DEATH, his kill only if he hit that warrior in the last 100 ticks. His own feud
   drops two rungs, the tribe is cowed for about six days and its bands run home. He takes the Sol
   Visage. The grove's pressure is now 1, so the heir is due 12 days later (MZ-14 (a)); the grove is in
   while he is home, so the reconcile is ordered then, and applied once he is more than 160 blocks from
   the throne or after 6,000 ticks (Rule U, §4.5): a warrior's body goes, the grove having regrown by
   then at 2 a day, and a new Umvuthi sits on the throne. Had the tribe climbed to Invasion first, the
   Umvuthi would have come against him (§4.5): from 120 blocks off (his ranch, at 150, is past the 132
   to which it is drawn) he would see it rise from its throne and float out behind its warriors, as Ines
   does in G.

**C. A village herd.**
1. A plains village 900 blocks from the grove trades wool and leather. The tribe plunders on its own
   schedule (`plunder_mean_days` 12, goblin's): a raptor pack takes the village's leather and wool.
2. The village has less wool and leather to sell. Tomas, passing through, sees the pack and breaks
   it: the village the pack marched on saw it and thinks better of him at once; the other villages
   hear of the rescue from travellers, later, and think a little better of him (CL-D61). Had it
   been his base the pack broke on, the base would have told the travellers.

**D. Clearing a range, and what comes back.**
1. Iva kills the Frostmaw by her line on day 9.5, as in A. The den is empty.
2. She visits the den on day 12. It comes in, and she finds the ruin and no Frostmaw: its body was
   taken when the den came in while the Frostmaw was out by the line (§4.4), so the census has nothing
   to remove. Had the den stayed out until she killed it, the census would now remove the body left
   behind, its slot dead (§4.9). Its next Frostmaw is due on day 21.5 (12 days after the kill, §4.2),
   and until then the feud waits for the arrival without waking (ledger 6).
3. On day 21.5 the new Frostmaw arrives in the books, a slot owed a body. Nothing shows yet: the den
   is out.
4. On day 23 she flies over the den; it comes in, the census finds the den empty and the new Frostmaw's
   slot owed (§4.9), and the plugin places it asleep in the ruin once she has flown more than 160
   blocks on. If her trains still run, it is already angry: the heir took the old grievance, and it may
   have mustered and set out before she came (A, "kill it"); then its slot is away and still owed,
   nothing is placed, and its body is placed when it comes home.
5. Bram, another player, kills it at its den on day 26 while it sleeps (he has no feud with it), and
   takes its crystal. The faction is cowed again, the heir grudge falls on Bram, and Iva's feud does
   not step down. The den's pressure has decayed from 1 to 0.888 since day 9.5 and gains 1, so the next
   wait is 6 × 2^1.888 = 22.2 days, until about day 48: Iva gains those empty days.

**E. Removing rtsim.** Ben removes mic-rtsim on day 10, while the Frostmaw waits by Iva's line and Iva
has stayed at her smelter. The den's saved chunk still holds the vanilla body (the den has not come in
since it set out), so after the uninstall the Frostmaw is home asleep. Had Iva ridden north as in A,
step 7, the den would have come in, its body would have been taken, and the uninstall would leave the
den empty: the exposure §4.9 lists. The grove keeps its vanilla warriors and its Umvuthi, unless an
Invasion is out whose bodies are gone: one that set out with the grove in took its warriors and its
Umvuthi with it, and an uninstall then leaves the grove without them for good (MZ-15's cost, §4.5). Ben
reinstalls on day 30: FACTION_FOUND rebuilds both factions from their censuses (crystal held, 7
warriors), with no grudges, and the tagged bodies, strays to the new books, fill the new slots.

**F. The Umvuthi leads an Invasion.**
1. Nadia farms wheat and cattle 450 blocks from a grove of 9. For weeks she has logged acacia and
   killed warriors inside its 200-block range, and she struck its envoy. Her grievance passes 140 and
   the tribe climbs to Invasion: 6 warriors and 2 raptor packs, led by the Umvuthi (MZ-3 (b)), muster
   for a day and march on her farm, her home by P26. The grove is out when the muster ends (she is at
   her farm, far beyond the view distance plus 2 chunks), so nothing is ordered: every body stays in
   the saved grove, their slots away with the bodies left, the Umvuthi's too. BAND_MARCH gives Java the
   march: its route with the tick at each waypoint, 6 warriors and 2 packs, the chief leading.
2. The band marches 450 blocks at 0.02 blocks a tick: 22,500 ticks, under a day. A far view drawn from
   that record (§4.16) would show her a short column crossing the savanna toward her farm, a taller box
   at its rear.
3. She rides out to look and meets it on the road. The engine embodies it where it stands (`seen`):
   CHIEF_BODY, then the band's stacks, each appearing as it comes inside her entity-ticking area, 160 to
   176 blocks off at the default simulation distance of 10: the warriors two abreast, the raptor packs
   beside them, and at the rear the Umvuthi, seated in the air 2 blocks above the grass. None is drawn
   that far out; they walk into sight, the warriors from about 72 blocks and the Umvuthi from about 132,
   and its boss bar shows once it has her in line of sight within 128 blocks. She falls back to her
   farm, and the column walks on after her.
4. At her farm the band's steer turns on it, with the attack flag; the warriors come over the rise and
   the raptors go for her cattle. 32 blocks out the Umvuthi sinks to the ground, restricted where it
   sits. Within 40 blocks it takes her as its target and summons warriors around it, then cranes as it
   loses health. The summons are puppets: none is saved, and none counts in the band. In the fight's
   books its strength of 5.0 counts in the band's 12.6.
5. She shoots the cranes first and kills it. She gets the Sol Visage, and every warrior within 30
   blocks of it falls with it, its summons and the band's own nearby, each a BAND_CASUALTIES credited
   to her only if she hurt that warrior in the last 100 ticks. The engine runs `chief_killed`: her
   Killing report, the heir succeeding with half the grudges, the tribe cowed for about 6 days, her
   feud from Invasion to Scouts, the band routed. The grove's P_chief is 1. Its slot is dead, left: its
   vanilla body still sits on the throne in the saved grove.
6. The survivors walk home. She watches them go until they leave her area and their puppets despawn out
   of her sight; a far view would draw them on to the grove. They reach `home` about a day later, with
   the grove still out: their slots are home, with the bodies they left.
7. A week later she rides to the grove. It comes in while she is still about 200 blocks off; the census
   finds the throne body of a dead slot and the bodies of the warriors who died at her farm, and the
   reconcile removes them. The warriors' go at once. The throne body's removal waits under Rule U while
   she is within 160 blocks of it: on the grove's far side from her the throne is 149 to 201 blocks off
   as the census runs, so it usually goes at once and she finds the throne empty; on her side it waits,
   and she finds the Umvuthi still sitting there, the body of a chief already dead. Killed, that body
   drops nothing and takes no warriors with it, as the other of two bodies does (§4.5); once she
   leaves, the epoch ends and the next census removes it. Had rtsim been removed before then, the
   Umvuthi would be on its throne for good: the jar's state wins, which UR-1 allows.
8. Twelve days after the kill, the later of the cowed time and 6 × 2^1 days (MZ-14 (a)), the heir's
   wait is over. The grove is out, so nothing shows until it next comes in; then one reconcile removes
   the warrior farthest from any player and seats a new Umvuthi on the throne, its trade memory empty:
   whoever paid the old one pays again. Had she stayed by the grove all that while, the old body's
   removal would still be held when the wait ends; the heir is then seated in its place at once, or,
   were the old body fighting her, once that fight ends (§4.6), and a payment she made to the old body
   meanwhile counts as tribute but is not remembered by the heir.
9. Had she not killed it: had Tomas been within 160 blocks of the throne when the grove came in during
   the march, the census would have ordered the throne body taken, held while he stayed: the Umvuthi
   has two bodies, the one with its band and the one he sees. The first to die is the death that
   counts, with its Sol Visage; the other drops nothing, takes no warriors with it, and is removed. Had
   he still been there when the band came home, the held take would be answered stale as the puppet came
   within 32 blocks of the throne, and the hand-back would leave one Umvuthi on the throne, with the
   puppet's health. Had he left before then, the take applies as he goes, and the throne stands empty
   until the Umvuthi comes home.

**G. Seen coming from far off.**
1. Ines has a hilltop fort 900 blocks from a grove of 8 and plays with a far view (Distant Horizons,
   drawing bands from BAND_MARCH as the DH note's §4.6 proposes; this plan only exposes the march). She
   has been mining the grove's hills, and its grievance passes 140: the tribe musters its Invasion, 6
   warriors and 2 raptor packs, the whole pool, with the Umvuthi to lead it.
2. She rides to a ridge 120 blocks from the grove to watch. The grove is in, and she is within 160
   blocks of it, so at the muster's end the band is seen as it sets out: six warriors walk out of the
   grove in their own bodies, the two whose bodies the raptor packs stand for are taken on the far side
   from her, and the packs fall in beside the column. The Umvuthi's body is handed to its puppet where
   it sits, which lifts from its throne, rises past the backrest, and floats out over the grove after
   its warriors to the rear of the column, its legs hanging and its head-sun alight (CAMP_BODIES: done).
   The grove stands empty.
3. She rides home ahead of them. Once she is out of range the band's puppets despawn behind her, out of
   her sight, and the engine carries the march. From her tower the far view shows a short line on the
   savanna, a gold box at its rear, coming on at a walk: 900 blocks at 0.02 blocks a tick is 45,000
   ticks, nearly two Minecraft days of warning. She builds a wall and sets her golems on it.
4. On the second day the column's head comes within 160 blocks of her. At her simulation distance of 10
   the column's stacks appear where the far view drew them, each as it comes 160 to 176 blocks from her,
   and the box gives way to bodies as they walk into sight: warriors two abreast, raptor packs beside
   them, the Umvuthi aloft at the rear, its boss bar showing once it has her in line of sight within 128
   blocks. Had her server run a simulation distance of 8, the stacks would appear 128 to 144 blocks out;
   of them only the Umvuthi is drawn that far, and it could be seen to appear 128 to 132 blocks out,
   where the far view's box stood a moment before.
5. 32 blocks from her wall the Umvuthi sinks to the ground and summons; the warriors try the wall and
   are stopped at it (I22), and her golems break them. The survivors walk home, and the Umvuthi rises
   after them once the band has turned for home and it has had no player target for 200 ticks, her
   golems' blows notwithstanding; the far view draws them all the way. Had she followed them to the
   grove, she would have seen them walk in and become the grove's own again, the Umvuthi settling onto
   its throne (hand-backs, §4.9).

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
| Rail meters | per (faction, actor), plus a line count per (line, faction, actor) | a few dozen bytes and one booked wake per meter, a few bytes per line count (no wakes); about one push a day while over the tolerance, 19 to 37 in a new line's first 20 days (§4.3) |
| Lying in wait | per pass within reach of a spot | one roll; a hit is one `attack_train`, as today's bandit ambushes |
| Envoy guard | per damage event to a living entity, server-wide | one lookup of the target's UUID in the open envoy parties, usually an empty set |
| Camp finding | per chunk load | one check of the chunk's structure starts (`Lairs.java:114-125`), and for a camp found, one read of each footprint chunk's level |
| Camps in and out | per ticket level change of a footprint chunk | one lookup in the footprint index; while every footprint chunk is at FULL and the camp is not yet in, one `areEntitiesLoaded` lookup per footprint chunk a tick (at most 25: a grove's 51-block circle spans at most 5 × 5 chunks) |
| Census and reconcile | each time a camp comes in | one entity scan of the den's template or the grove's 25-block circle, and one CAMP_RECONCILE when anything differs; a player at the edge (the view distance plus 2 chunks) can bring a camp in again and again, one census each time |
| Camp body index | per entity join or leave, server-wide | one attachment check |
| Orders at a camp that is in | per war party and refill | a set-out line at a muster's end, hand-back and spawn lines at a homecoming, a grove's refill (at most 2 a day): a few takes, hand-offs or spawns, each with a record on the body or a marker until acknowledged; the barrier saves the 1 to 4 entity chunks of a den (up to 25 of a grove) ahead of the journal's next write |
| Notices | per feud | one pending bit, resent at session start |
| Watched boss | while seen | one Frostmaw, or a band in waves of at most 16, plus on an Invasion an Umvuthi aloft or settled and its summons (under 4 Umvuthana within 50 blocks and 3 cranes within 40 of it, `EntityUmvuthi.java:469`, `488`) |
| Aloft | per Umvuthi puppet, while its band has bodies and it is aloft | one `move` a tick, with vanilla's block collision for its box; its motion, fall distance and restriction set each tick; its target height from the `MOTION_BLOCKING` heightmap over the 4 to 9 columns under its box and 8 ahead, every 10 ticks, from loaded chunks only (a map lookup and an array read a column); a target check a tick for the settle; at most one re-pair in 200 ticks (one add bundle to each tracking player); block clips to players within 160 blocks only while it is stuck; one long-distance particle packet to each tracking player every 10 ticks. Settled, nothing |
| Chief's summons | per minion or crane a chief puppet summons | a tag and an entry in the ConflictBridge's groups before it joins (the mixin at `addFreshEntity`); nothing for any other join |
| Trade memory | resident, per grove | 16 bytes a player who has paid its Umvuthi; one journaled CHIEF_PAID per first payment to an Umvuthi body |
| Rule U holds | per held Umvuthi line, per tick | one nearest-player check against the body, and for a take or removal its target, for as long as a player stays; a spawn for at most 6,000 ticks |
| BAND_MARCH | per change of a party's march | 72 bytes plus 8 per body kind and 16 per waypoint (408 for 20 waypoints and two kinds): at a send, each change of route or phase, a casualty (at most one a tick per party), the end, and every live party at a session start; the ConflictBridge keeps one per party out |
| Pending stacks | per stack of a met band not yet wholly entity-ticking | two hash lookups a tick per chunk a pending stack waits on (no event follows the entity-ticking state), and a stack's test only when one of those chunks changes or its place moves into another chunk, in the ConflictBridge; at a stop, one walk-in from the edge per stack still pending, which then holds one chunk inside it until the band moves on; no record (MZ-17 (a)) |
| The walk home | per band walking home within a player's area | as one going out: its puppets and their steering, which today only bands going out cost |
| Animals owed | per district load with a debt | one entity scan of the district's loaded chunks, capped |
| Density | per world | at most 6.25 dens and 6.25 groves per 1,000 × 1,000 blocks; 4,096 factions (the paged sweep's top, `docs/choices-paging-modules.md:23`) of one kind at that density cover 655 million square blocks, a square about 25,600 blocks on a side |
| Nobody near any camp, no train in any range, nothing changing | | the sampler's per-second walk only |

## 7. Tests before it ships

**GameTests** (`neoforge/mowziesmobs/.../gametest/MowzieGameTests.java`; names in the dragons' style,
`DragonGameTests.java`, 701 lines). A real structure start cannot be placed in a GameTest (`/place`
registers no start, `docs/overnight-choices-iceandfire.md:140-143`), so tests declare their camps. The
rail tests build on `RailFixtureGameTests` (branch `test/create-rail-fixture`); whether its train runs
its schedule under GameTest's unthrottled ticks is UNVERIFIED (no GameTest has run an assembled train on
a schedule, `neoforge/README.md:262`), so each rail test has a stationary form that settles the
mechanism, and the moving form runs if the fixture's train moves.
1. `aDensFrostmawStaysVanillaAndTheCensusReadsItsCrystalAndHealth`.
2. `aRobbedFrostmawIsKeptAndAResetDoesNotGiveItsCrystalBack`: the steal sets `PersistenceRequired`
   and sends CHIEF_STATE; loading a robbed Frostmaw from NBT does neither.
3. `aGroveIsCountedAndAResetRecountsTheWarriorsStandingInIt`.
4. `aMemberComingHomeGetsASavedVanillaBodyAndAnAwayPuppetIsNotSaved`, and the NBT of a tagged body and
   of a marker holds nothing a world without rtsim cannot load: the attachment is skipped with its log
   line (`AttachmentHolder.java:162`) and the marker is inert. An Umvuthi left on its throne while it
   leads is saved there, without `NoGravity`; its puppet and its summons are not saved.
5. `aRefilledPoolIsSpawnedAtTheNextCensus`, for a den and a grove that are out; at a grove that is in,
   each whole warrior is spawned when it is ready (CAMP_BODIES: done); strays fill owed slots before
   any spawn; a den's Frostmaw spawn waits while a mock player is within 160 blocks, and is placed once
   it leaves or at 6,000 ticks; a member that set out from a camp that was out and came back to it is
   not spawned a second time.
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
14. `anEnvoyPartyIgnoresGolemsAndPlayersUntilStruckAndTakesPayment`, extended: while the demand is
    open, a projectile with no owner (a Create Big Cannons shell fired by a Create Radars controller
    if both are in the test run, else a stand-in), a golem's blow and a skeleton's arrow do no damage
    to the envoy or a guard; a player's arrow or bullet does and is an insult (ENVOY_ANSWER 3); a
    party killed by `/kill` sends no ENVOY_ANSWER and the demand is withdrawn (DEMAND_SETTLED 5).
15. `aChiefKilledAtTheCampIsReportedWithTheKiller`, for both bosses.
16. `tributeLandsInAGroveChestWhenTheGroveLoads`.
17. `raptorsHuntTheBasesAnimalsAndAnOwedHuntIsAppliedOnLoad`.
18. `aNaturalFoliaathIsNotCountedAsPressure` (the core tag).
19. `aNoticeWaitsForThePlayersNextEntryAcrossALogout`.
20. Strength fights: each body against reference bases through `IND_FIGHT`, for §4.13.
21. `theCensusWaitsForEntitiesOnACampsColdLoad`: the stationary form holds a den's chunk at FULL with
    its entities not yet in (`areEntitiesLoaded` false), and asserts no CAMP_CENSUS and no
    CAMP_RECONCILE; once they are in, the census reports 1 with the crystal. A grove with one footprint
    chunk below FULL stays out until that chunk is at FULL with its entities in.
22. `theCampReconcilerKeepsItsInvariants`, for a den and a grove, asserting I1 to I6 (§4.9) after every
    step, in the stationary form unless named:
- **bodies that reload without their chunk**: a mock player moved from inside the view distance to 3
      chunks beyond the view distance plus 2 and back holds the camp's chunks in memory below FULL, so
      no `ChunkEvent.Load` fires on its return; the camp goes out (CAMP_OUT) and comes in again with a
      new epoch and census, so a body left behind by a party that set out meanwhile is taken and its
      party marches on, an owed body is spawned, and a left-behind body killed in between is a
      CAMP_DEATH by slot that kills its party's unit;
- **a member drawn while owed** (walkthrough A, "kill it"): with the den out, the next Frostmaw arrives
      as an owed slot and musters and sets out; the den comes in, with a mock player near it and again
      with none: nothing is taken, nothing is spawned, no member is booked dead, and the slot stays
      away and gone; at `home` it is owed and spawned once; the books and the world each hold one
      Frostmaw and one crystal throughout;
- **the draw order**: a grove with three bodies found, one missed and two owed sends a party of four:
      the three found and the one missed are drawn, none owed; the set-out line's answer for the missed
      one is nothing to do, and it is booked away and gone; a slot with a line in flight is not drawn,
      nor picked for a raptor pack or an heir;
    - **crashes**, in the style of `ReconcileAheadGameTests`
      (`neoforge/create/.../gametest/ReconcileAheadGameTests.java:296`, `616`), which reads the journal
      and the chunk as they are on disk at each point: a take, a spawn, a set-out with its hand-off, a
      removal and a death, each stopped between every pair of the crash table's steps and started again
      from the files; each start does what its row says, a holder's record is journaled again with its
      op id and booked once, and the barrier holds the journal's write until the entity chunks are on
      disk; with the barrier off, the journal-ahead row's repairs follow;
    - **a second census before the first's lines**: CAMP_OUT and a new census arrive before an epoch's
      CAMP_RECONCILE, which Java answers stale and does not apply; the new census's reconcile removes,
      takes and spawns each body once; a den going out while a Frostmaw's spawn is held answers it
      stale, the next census orders it once, and no living Frostmaw is removed;
    - **deaths in every phase**: a body killed at the camp during a muster with the pool at 0, between
      a muster's end and its answer, and while its party is out with the body left: each kills that
      member's unit; a party left with no units ends through `home`, with AMBUSH_SPOT's removed flag if
      it lurked; the chief's kill routs every party out; a stray's death books nothing;
- **in plain view**: with a mock player 150 blocks from a grove that is in, a band that sets out walks
      out in its members' bodies (health, mask and weapon kept), and no body vanishes but the warrior
      the raptor pack stands for, the one farthest from the player; a band that comes home in its
      puppets to a grove that is in is handed back where it stands, and no body appears or vanishes
      within 128 blocks of the player;
    - **take-always**: with the Frostmaw lurking and its body left in the den, the den comes in with a
      mock player at the den, with one nearer the spot, and with none near: each time the body is
      taken, the party stays out with its lurk and its AMBUSH_SPOT, no BAND_RECALL is sent,
      TRAIN_PASSED still arrives, and the slot is away and gone; when it comes home to a den that is
      out, the next census spawns it;
    - a `player_left` recall during the lurk sends no removed flag, and TRAIN_PASSED still arrives;
    - a grove's envoy party out while the grove comes in is not touched and its demand stays open; an
      envoy or scouting party that ends at a grove that is in despawns, and the next census adopts
      nothing from it;
    - **strays and identity**: the jar's untagged bodies fill a new camp's owed slots; a body tagged
      with a generation the books retired at a death is removed, and one written off is adopted under
      the cap; of two loaded bodies with one tag, the farther is removed; a tagged Frostmaw is
      restricted to its den at each join, and one that chased a player and lost it walks back before it
      sleeps (UNVERIFIED until this runs);
    - **Carry On**, if it is in the test run: picking up a tagged warrior is a CAMP_DEATH with killer
      kind taken and clears its tag, and putting it down makes a stray;
- **the take's budget**: a mock player moved toward the den at 2 blocks a tick along an axis, and again
      on a diagonal, with the Frostmaw away, and toward a grove; the ticks from the camp coming in to
      the take are logged against 14 to 24 (den, axis), about 50 (den, diagonal) and 7 to 15 (grove)
      before the body is in tracking range (unthrottled GameTest ticks make the entity read span more
      ticks than on a live server, so the count errs high).
23. `anInvadingUmvuthiFloatsWithItsBand`: an Invasion met on the road (`seen`), at its fight
    (`start_fight`), at a later wave and walking home each sends CHIEF_BODY and then the chief's stack;
    the Umvuthi's puppet spawns at the rear of the column with `NoGravity`, its direction the quarter
    facing its way, and CHIEF_BODY's health and trade memory, and keeps 2 blocks over the highest
    motion-blocking block under its box and 8 blocks ahead, over flat grass, a 6-block wall across its
    route, an oak's crown and a roof, never moving more than 0.1 blocks in a tick and never inside a
    block; with the route ahead in an unloaded chunk it reads nothing there and loads no chunk
    (counted); within 32 blocks of the target with the attack flag, or on a mock player target in sight,
    it sinks onto what is under it, its `NoGravity` cleared and its restriction centre where it sits;
    with the attack flag still set and its target lost (out of sight, invisible, or the player flying
    off) it stays down past 200 ticks; once the flag drops and it has had no player target for 200 ticks
    it rises and rejoins the band; a golem that targets it on the road neither sets it down nor holds
    it, and it fights the golem aloft with its summons restricted within 25 blocks of it, not of its
    spawn point; a creeper exploding under it, a wind charge and a fan's push leave it within 0.5 blocks
    of its target height; after a stuck rise of 24 blocks it settles on farmland with no landing
    particles and the farmland stays; a puppet pushed into a nether portal stays in the overworld; under
    a ceiling 3 blocks up and 20 wide across its route it is held, rises by the stuck rule's steps, and
    with a mock player in line of sight waits up to 1,200 ticks before it is moved to its place, at once
    with none; killed aloft, it falls, drops one Sol Visage at its death animation's end, and no body is
    left floating; a saved chunk never holds an Umvuthi with `NoGravity`.
24. `theChiefsSummonsArePuppetsAndAreNotSaved`: the Umvuthi's puppet, settled and targeting a mock
    player, summons warriors and cranes; each is tagged and added to the ConflictBridge's groups before
    it joins and is admitted by the ConflictBridge at the core's join check, is in no unit or quota, and
    is absent from the chunk's saved entities; a summons at the throne stays a stray.
25. `theTradeMemoryRoundTripsThroughTheEngine`: a masked mock player pays the throne's Umvuthi, and a
    CHIEF_PAID with the body's slot and generation is journaled, settling the player's open demand, or
    with none booked as a gift; the puppet's `hasTradedWith` is true for that player; a payment to the
    puppet is remembered by the body handed back to the throne; a restart after a payment and before any
    CHIEF_BODY keeps it; a census of a dead slot's throne body, and a payment to it, add nothing to the
    heir's memory, the payment still booked as tribute; CHIEF_BODY's write journals nothing; a
    succession clears it.
26. `anUmvuthiKilledAwayDropsTheSolVisageAndRoutsTheInvasion`: a mock player's kill of the settled
    Umvuthi is CHIEF_KILLED with that player, the Sol Visage drops, the feud goes from Invasion to
    Scouts, the column's warriors within 30 blocks die, each a BAND_CASUALTIES with `getKillCredit`'s
    killer, and the rest walk home; while the player is its target no timeout removes it; the grove's
    next census removes the throne body under Rule U.
27. `killingTheUmvuthiKillsDevotedUmvuthanaWithin30Blocks`: the mock player kills the Umvuthi on its
    throne with tagged warriors 10, 25 and 35 blocks from it: one CAMP_DEATH for the chief though `die`
    raises two `LivingDeathEvent`s, and the Sol Visage drops; the first two warriors die within 20
    ticks, each a CAMP_DEATH with killer kind none unless the mock player hurt it in the last 100
    ticks; the third lives, as does a follower bound to a player.
28. `aGroveThatComesInTakesWhatItsMarchLeft`: with an Invasion out and the grove out at its muster's
    end, the grove comes in with a mock player near the throne and again with none: each time the
    warriors' bodies and the throne body are taken (the throne body's under Rule U), the party marches
    on with its BAND_MARCH unchanged, and the Invasion still reaches its target; a raid and a refill
    meanwhile leave pool plus away at or under the cap.
29. `theUmvuthisLinesWaitWhileAPlayerIsNear`: a take and a removal of a throne body wait while a mock
    player is within 160 blocks of it or is its target, past 6,000 ticks, and apply once neither holds;
    a spawn at the throne waits for the player to leave or 6,000 ticks; an epoch's end answers them
    stale; with a take held and the band's puppet killed first, the throne body killed after drops
    nothing, kills no warrior, books nothing, and is removed; with the take still held as the band comes
    home, it is answered stale as the puppet comes within 32 blocks of the throne, and one body ends on
    the throne, with the puppet's health; a band whose watcher walks off, or logs out, with a second
    mock player 180 blocks from the band, inside the 192 that `book_unseen` watches, is re-bound to that
    player and not recalled; a band whose watcher follows it, 180 blocks behind when its `Unseen` falls
    due, is re-bound to the same player; a fighting band whose watcher logs out with a second mock
    player 100 blocks off keeps its bodies, and its fight turns to waiting, while a fighting siege so
    left is recalled, as today; an embodied band that arrives with the mock player 170 blocks from its
    target is not recalled, resolves abstractly, loses its fallen out of the player's sight and walks
    home in its bodies, and once the player leaves, its recall keeps the band less those the raid
    killed, each lost once (6 warriors with 3 killed leave 3, not 0), while with the player 150 blocks
    off it starts a fight; and the four exceptions: at a simulation distance of 8, an aloft Umvuthi
    whose chunk stops ticking entities is taken back at once with a mock player 130 blocks off on an
    axis, and nothing of it is left floating or saved; a band whose route crosses an `edge_stop` seam is
    recalled at that segment with a mock player beside it; a failed session captures every puppet; and a
    band that reaches home with its grove out, a mock player 150 blocks from the throne at a view
    distance of 8, is recalled.
30. `anUmvuthiHeirTakesAWarrior`: after the wait, one reconcile removes the warrior farthest from the
    mock player, skipping one with a line in flight, and spawns the chief at the throne spot
    (`finalizeSpawn` as a structure spawn, restricted to the pit, the old direction, an empty trade
    memory); the warrior's slot is freed and the chief's filled; with no warrior home nothing is
    ordered until one is; a grove found without its Umvuthi uses the fire pit, direction 1; with the
    old chief's body still on the throne and its removal held by a mock player within 160 blocks, the
    heir is seated in its place in one tick when the wait ends, at full health with an empty trade
    memory, and the held removal is answered stale; with the old body targeting the mock player and
    hurt by it in the last 600 ticks, the seat waits; with the old body still targeting the mock
    player, which has not hit it for 600 ticks, the seat is done.
31. `anAloftUmvuthiReadsAndTurns`: screenshots of the Umvuthi aloft, with the trail and without, over
    grass and over water, settle how it looks (§4.5, UNVERIFIED until this runs: whether it reads as
    borne by its sun, not slid); its legs hang aloft and lift once it settles by a block; on a headless
    client that has tracked the body since its spawn, a 90 degree turn of its route brings a re-pair
    within 200 ticks, after which the client's body is a new client entity with the same id, its yaw
    within 45 degrees of its way in its first frame (no spin), and a screenshot in that tick shows no
    gap (UNVERIFIED until this runs); the lift and drop of its legs on the fresh client body is
    recorded, with its length in ticks; the server's body keeps its id, UUID, health, `healthLost` to
    the fraction, Sunblock, fire, ability timers, target and `lastHurtByPlayer`, ChunkMap holds one
    tracker for it, and no tracking, join, leave or death event fires on the server; with the body's
    section turned from hidden to tracked after a re-pair, nothing throws; a re-pair waits for an active
    ability to end, none comes within 200 ticks of the last, and none runs for a dying body; every
    player who had the boss bar keeps it through the re-pair, and the boss music is taken up again
    without restarting the track (logged); the trail shows to a client 100 blocks off; whether L2
    Hostility's moving traits reach a puppet is logged; set out from a grove that is in with a mock
    player 120 blocks off, the hand-off puppet faces its route out, rises from the throne past the
    backrest without touching a block and floats out after its warriors, and with a block placed over
    the throne it is met by the stuck rule; at home it floats to the throne spot, re-pairs to the
    throne's direction and settles on the throne before the hand-back.
32. `aBandWalksHomeInViewAndIsHandedBack`: a band seen at its target and routed walks home in its
    puppets (BAND_STEER, no BAND_RECALL); a band met walking home is embodied; at `home` with the grove
    in, each puppet becomes a tagged vanilla body where it stands (mask, weapon, restricted to the
    centre), the Umvuthi on its throne once its puppet has settled there, and a raptor pack's slot is
    spawned at the far edge while its pack walks off; with the grove out, the puppets are recalled and
    the next census spawns them.
33. `aColumnAppearsInsideTheTickingArea`: at simulation distances of 8 and 10, a band whose column
    straddles the entity-ticking edge appears stack by stack, each stack the tick its last member is
    inside, and is never reported `NOT_TICKING`; the stacks already in stay in; a stack that walks out
    goes back to pending and comes again; each stack's distance from the mock player as it appears is
    logged against 160 to 176 blocks (10) and 128 to 144 (8) on an axis, and the Umvuthi's is never
    under 128; a recall reports every living member, in Minecraft or pending, as a survivor; an embodied
    band that arrives keeps its puppets, and only members not yet in Minecraft come as waves; with the
    band stopped at its target and a stack's place in the column outside the area (a mock player 140
    blocks from the target at 8), that stack is placed once, holds one chunk inside the edge while it
    walks toward the fight, is never taken back and placed again while the player stands within a chunk
    of where it stood, can be killed, and rejoins its column when the band leaves; with the mock player
    150 blocks from the target, which does not tick at 8, the stacks hold at the edge and the fight runs
    to its end; a pending stack is tested only when a chunk it waits on changes or its place moves into
    another chunk (counted); a puppet picked up with Carry On by a creative mock player leaves the band
    as BAND_CASUALTIES with killer kind player and that player, and one removed with `discard()` as
    killer kind other, with no killing signal; the Umvuthi's puppet so picked up is CHIEF_KILLED with
    that player; an envoy appears as one group.
34. `theMarchRecordMatchesTheBodies`: BAND_MARCH arrives at each send, change, casualty, end and
    session start; a band met on the march stands as its column (two abreast, 1.5 blocks a row, the
    chief at the rear) with its head at the record's position; the position Java computes from
    BAND_MARCH equals `party_pos` to the block, on a flat world, across a seam of a wrapped one, and
    across a glued seam, which turns; in the moving form, the column's advance at 0.02 blocks a tick,
    in short walks to a steered restriction centre, is logged against the record (how it reads is
    UNVERIFIED until this runs).
35. `anAloftUmvuthiCrossesARiver`: a band's route crosses a river 3 blocks deep and 20 wide: the
    Umvuthi floats 2 blocks over the surface, never touches the water, and the band reaches its target;
    with the attack flag set while it is over the river more than 8 blocks from the target it floats on
    to the far bank and settles there; with the target a raft base on open water, it fights where it
    floats within 8 blocks of it, restricted there; killed over the river, its body falls into it and
    the Sol Visage drops once, at its death animation's end.
36. `aDyingChiefTakenBackDropsItsLoot`: a mock player kills the settled Umvuthi puppet, and during its
    115-tick death animation the server stops, and in a second run its stack is taken back at the
    ticking edge: each time the Sol Visage is on the ground where it died, CHIEF_KILLED names the
    player, and nothing drops twice.

**`rtsim-cli sim mowzie`** (a scenario beside `sim conflict`, `main.rs:81-85`), with runs:
1. A line through a range, with the meter: notice, raid, repeat, invasion, at §4.3's first table;
   ambushes bounded per train per day; a kill, cowed, the heir, the body 12 days after a first kill. It
   asserts on `stats.feud_wakes` (`engine.rs:1582`): a den waiting for its next Frostmaw with a busy
   line wakes its feud a handful of times, never once a tick.
2. Cut service and stopped service: the grievance follows §4.3's second table; no raid once it is
   under 70.
3. Reroute: the new track's passes miss the circle and the old line's meter decays.
4. Two dens on one line: both feel it; a den found after the line exists feels the next pass.
5. The tribe: envoy paid at the priced demand; envoy refused, then a raid and an invasion; plunder;
   raids that leave and come home with the grove in and out (CAMP_BODIES answered as the plugin would),
   after which no census books a living survivor dead; a grove that refills while a band is out comes
   back to its cap with every survivor's slot kept.
6. Renewal: an emptied grove refills to its own cap; a den's body comes back after `min(192, 6 × 2^P)`
   days: 6 with no player cause, 12 after a first kill, and about 1.8 a game year when farmed
   (EQUILIBRIUM.md §3.3); a den found empty gets a migrant.
7. Kills: by the feud's player on a raid, by a third player on a raid, at the camp by a player with no
   feud: which feud steps down in each.
8. Save and restore in the middle of a lurk, a cowed time, a meter's decay and a reconcile in flight,
   with identical results; a session restart there voids the lines, and the next census orders them
   again.
9. A trunk: 14 lines at 1.7 legs a day through one range reach §4.3's trunk row, as one line of 23.8
   does; the focus follows the busiest line; two owners' trains on one line make two meters.
10. The camp reconciler: camps coming in and out at random (a player at the edge), musters, homecomings
    with hand-backs, refills, the Umvuthi's marches, held lines and heirs, deaths in every phase,
    answers lost, late, replayed and stale, and session restarts at random points, with Java emulated as
    a world of tagged bodies; after every event I1 to I6 hold, and at the end no member has two bodies
    and no living member is booked dead unless its body was lost; no draw takes more than the smaller of
    floor(pool) and the slots it may draw, and no stray is adopted while the pool plus the members away
    is at the cap.
11. An Invasion with its chief: from a grove that is out and from one that is in, the Umvuthi goes
    with its band and its strength counts in the band's; killed away, the band routs, the feud lands on
    Scouts and the heir's body waits 12 days; a grove that comes in while it is out takes its throne
    body and the Invasion goes on; with a player beside the grove holding the dead chief's throne body
    under Rule U for the whole wait, the heir is seated in its place when the wait ends, and while that
    body is in a fight the seat waits for the fight's end; a band whose watcher walks off or leaves
    with a second player within 192 blocks is re-bound to that player, not recalled, as is one whose
    watcher follows it; an embodied band that arrives with its only player 170 blocks from the target
    resolves abstractly and walks home in its bodies, and one whose fight loses its watcher with a
    second player near keeps its bodies and goes on abstractly; a band resolved while still embodied
    has its abstract dead counted once at its recall.
12. The heir: an Umvuthi killed as fast as it comes back for 60 game years gives about 1.8 Sol Visages
    a year at base 6 (MZ-14 (a)) and about 1.0 at base 24 (b), matching EQUILIBRIUM.md's model.
13. The march record: for every party in runs 1, 5, 10 and 11, the position BAND_MARCH gives equals
    `party_pos` at every tick, a record follows each change of route, phase or count, and no census
    ends a party; on a wrapped world (Toroidal World's topology), a band that crosses a seam and then
    routs gets a route home from `go_home` that starts at `party_chart_pos`, with no lap at its front
    and a first segment as long as its walk; on a glued world (Deep Time's half-turned north and south
    seams), the cell Java's fold of a cell's centre gives equals `party_pos` at every tick across a
    seam.

**Bench:** `rtsim-cli bench scale --only factions,factions-paged` (`crates/rtsim-cli/src/main.rs:25`)
before and after, and a sampler bench at 200 trains on the laptop.

## 8. Decisions and questions for Ben

**Decided by Ben** (2026-10-21 and 2026-10-22, in conversation; moved here from the questions):

| Id | Ben's answer | Where it lands |
|---|---|---|
| MZ-1 | (a), as recommended (Ben, 2026-10-21): a vanilla `ConflictBridge` in the core owns the conflict records and the territory hash; the Mowzie plugin registers bodies, finds camps and takes censuses; the Create plugin samples trains | ledger 16 |
| MZ-3 | **(b)**, not the recommendation (Ben, 2026-10-21): the Umvuthi also leads the Invasion in person; amended by MZ-15 | §4.5's march, Rule U and trade memory; §4.6; §4.9; ledgers 4, 5, 7, 14, 18, 24 and 25; walkthroughs F and G; tests 23 to 36 |
| MZ-4 | (a), as recommended (Ben, 2026-10-21): heirs come back, the Frostmaw with its crystal, and Mowzie's drops, masks, trades and the steal are kept | §4.2, §4.10's loot supply |
| MZ-5 | (a), as recommended (Ben, 2026-10-21): an ambush destroys cargo, at most 64 a train a day and 10% of a leg, the same watched or not, plus the swipe and the ice balls | §4.4 |
| MZ-6 | (a), as recommended (Ben, 2026-10-21): gold nuggets, a new phase 1 good, priced by value | §4.5, ledger 12 |
| MZ-11 | (a), as recommended (Ben, 2026-10-21): the real warriors, saved as vanilla at home, with a census | §4.5, §4.9 |
| MZ-13 | (a), as recommended (Ben, 2026-10-21): `Train.owner`, with P14 at the line's stations as the fallback | §4.3 |
| EQ-4 | (b), against EQUILIBRIUM.md's recommended 24 (Ben, 2026-10-21): MOWZIE's base of 6 days, doubling with pressure, `min(192, 6 × 2^P)`: 12 days after a first kill, about 1.8 a game year farmed | §4.2, §4.10, walkthroughs A and D, sim runs 1 and 6 |
| MZ-14 | (a), as recommended (Ben, 2026-10-21): the Umvuthi heir waits on the same base of 6 as the Frostmaw, `min(192, 6 × 2^P_chief)`: 12 days after a first kill, about 1.8 Sol Visages a game year farmed | §4.6, §4.10, renewal and tunables rows; EQUILIBRIUM.md §2.4.7 and §2.6 items 3 and 12 follow |
| MZ-15 | Amends MZ-3 (b) (Ben, 2026-10-21): "I feel like it should be the grove / whoever they send 'walking' towards the player instead of it just not working. (I.E. Possibly being able to see them coming via DH.)" Whoever the grove sends, the Umvuthi included, walks the route to the target and back where a player can see it, and its position is known at every tick for a far view | §4.5, §4.9, §4.16; ledgers 4, 6, 7, 23, 24 and 25; BAND_MARCH; walkthroughs B, F and G; tests 23 to 36; sim runs 11 and 13 |
| MZ-16 | **Rejected** (Ben, 2026-10-22), first round: (a) a horse, and with it (b) a donkey or mule and any other animal mount: "Given the current models for umuthi (sunbirds), them riding horses / most other animals doesn't make too much sense." How the Umvuthi travels instead is MZ-16's second round, the next row | §4.5, §4.16; ledgers 4 and 24; walkthroughs B, F and G; tests 23, 31 and 35; the questions |
| MZ-16, second round | **(a), aloft** (Ben, 2026-10-22): on an Invasion the Umvuthi's own body floats seated 2 blocks over the ground at the rear of its column, its legs hanging as on its throne and its head-sun lit, with vanilla's no-gravity flag and one colliding `move` a tick, turned by a re-pair, sinking to fight and rising to go, with a trail of vanilla flame particles (on until test 31's screenshots say otherwise); no new entity, no client mod. The other options of the second round (sun steps, particle sun steps, bounds, carried unseen, staying home) are set aside, as §4.5 gives them | §4.5, §4.16; ledgers 4 and 24; the tunables' aloft row; walkthroughs B, F and G; tests 23, 31 and 35 |
| MZ-17 | **(a), capture per stack** (Ben, 2026-10-22), **amending C25**: each stack of a band (a row of two warriors, a raptor pack, the chief alone) is its own group in the ConflictBridge, held pending until it is wholly inside the entity-ticking area, walking in from the edge at a stop and holding one chunk inside it; the engine sees one embodiment; a taken puppet is booked as taken with its taker; no core or record change. C25's "a band and its master capture and despawn as one unit" now holds per stack for war bands | ledger 25; §4.16; DECISIONS.md C25 and MZ-17; tests 33 and 34 |
| CHIEF_PAID | **Accepted** (Ben, 2026-10-22): record 0x0814, Java to engine, journaled: a first payment to an Umvuthi body, at home or away, with that body's slot and generation, booked as tribute at the camp (the payer's open demand, else a gift) and entered in the grove's trade memory only for the live chief's slot and generation | ledger 14; §4.5's trade memory; the trade memory row below; test 25 |
| UR-1 exposure of marches | **Accepted** (Ben, 2026-10-22), as §4.9 and §4.16 describe: a census takes every body a party on the road left behind, so an uninstall loses the members away whose bodies are gone (for the rest of a march once its camp has come in while it was out, and for the whole march when it set out from a camp that was in), the Umvuthi among them, its throne empty from a death away until its heir is seated, and payments the engine holds that are not yet written into a body | §4.5's uninstall, §4.9's reset and uninstall, §4.16's census and the march; the bodies away row below |

**Decided here** (each can be undone; the reason is the plan's):

| Id | Decided | Reason |
|---|---|---|
| MZ-2 | A Frostmaw is angered by Rail, plus killing and building in its range | Ben's request names trains through yeti territory. Rail for other cultures (goblins, bandits) is a possible follow-up. |
| MZ-8 | Won raptor raids owe a district dead animals on load; village plunder takes livestock goods, eaten | C16 to C18 require attacks while the owner is away; "nothing unwatched" would break them. |
| MZ-9 | The Wroughtnaut stays out | Its crash is found (§4.15) and is in the jar's own placement check, which this plan cannot fix. |
| MZ-12 | Real track, by sampling trains once a second | Ben rejected straight lines between stations; chords put the Frostmaw where no train runs and anger the wrong dens. |
| Old age | `chief_lifespan_days` 1,000,000 for both cultures | §4.2. |
| Envoy party | While a demand is open: targeting stripped, golems kept off, every hit cancelled unless a player struck it; a party killed by anything else withdraws the demand | §4.5: golems and radar turrets would otherwise start a war the player never chose. |
| Rail meter | One meter per (den, actor), tolerance once; a band plus a daily resync; line counts only for the focus | §4.3: a meter per line hid a trunk of light lines, and the band alone froze a rate up to 1 leg off. |
| Camp reconciler | One ledger per camp (a slot per member, five states, six invariants). A census each time every footprint chunk is at FULL with its entities in, followed by ticket level, not only at a chunk's first load. One epoch-scoped order, CAMP_RECONCILE, whose lines name target states and whose answer is booked by the world fact it reports. Orders void at a session start, derived again by the next census. Records on the bodies and markers they change, journaled again at load, and the journal barrier extended to entity chunks | §4.9: each check found a new gap in the piecemeal rules (a census missed when bodies reloaded without their chunk, a member drawn while owed, orders lost or applied twice across a restart, a second census before the first's order, deaths outside a muster). One mechanism with invariants and a crash table replaces them. Voiding and deriving again, in the industry reconcile's form (F27), over resending with a duplicate answer: conflict commands are per session (`docs/boundary.md:1463`), and a census rebuilds what is needed from the world. Records ride on entities, not on the camp's chunk, because vanilla stores entities apart from chunks. The new and renamed kinds are internal to mic-rtsim and unreleased. |
| Bodies away | Only war parties take home bodies (envoys and scouts are fresh puppets). A census takes every body left behind by a party on the road, whoever is near, and never ends the party; a party ends only at its own `home` or its last death. An owed member drawn carries its debt away | §4.4, §4.9, §4.16 (MZ-15): a march that a visit to its camp could cut short would vanish from the road and the far view, and no member is in two places. The cost is UR-1 exposure for the rest of a march once its camp has come in. The jar places an Umvuthi only when it generates a grove (`UmvuthanaGrovePieces.java:276-288`, checked in the jar), so an uninstall also leaves without its chief every grove whose Invasion set out while it was in (its throne body handed off in view or taken), one whose grove came in during the march, and one whose chief died away, from its death until its heir is seated; and payments the engine holds that are not yet written into a body are lost at an uninstall as at a reset. |
| In plain view | A party seen as it sets out leaves in its members' own bodies, handed to its puppets; one that comes home in bodies is handed back where it stands. A spawn of a Frostmaw or an Umvuthi waits until no player is within 160 blocks of its den or throne, or 6,000 ticks. A take at a census can be seen by a rider at full speed on an axis approach when the server falls behind (14 to 24 ticks of budget at a den, 7 to 15 at a grove), or by a player who logs in or teleports within tracking range | §4.9: the hand-off, the hand-back and the held spawn remove the common pops; dropping a body at its join instead would need the slot's state, which only the engine has. |
| Unseen deaths and strays | A home body missing from three censuses, the third at least 2 days after the first (PLACEHOLDER), is written off. A stray fills an owed slot before any spawn | §4.9: one miss can be a Frostmaw that chased a player out of the footprint into an unloaded chunk; three over two days is a lost body. Adopting strays keeps a reset's bodies, the Umvuthi's summons and Carry On's put-downs in the books instead of spawning beside them. |
| Peaceful | No war parties leave | §4.12. |
| Kills | A chief killed at its camp steps down the killer's feud only; a killer with no feud steps none down; a third player's kill on a raid steps down the raided player's feud, as today | §4.2: a stranger's kill should not clear another player's grudge, but breaking a raid should end it. |
| Den node | A market-less world node (`NodeKind::Lair`), not a haven | Ledger 13: a Frostmaw trades nothing, and a haven costs about 17 KB. |
| Robbed Frostmaw | Kept persistent in its den, set at the steal | The steal should disarm it, not remove it; otherwise stealing and walking off farms a crystal at each refill without a fight. |
| Aloft (Ben's, MZ-16's second round, above) | On an Invasion the Umvuthi's puppet floats seated 2 blocks over the ground at the rear of its column wherever the band has bodies, its own body with vanilla's no-gravity flag and one colliding `move` a tick, its height from the `MOTION_BLOCKING` heightmap of loaded chunks only; its motion zeroed, its fall distance reset and its restriction moved with it every tick aloft; it settles within 32 blocks of the target with the attack flag, or on a player target in sight, never onto a fluid, fights any other target aloft, and rises once the attack flag has dropped and it has had no player target for 200 ticks; held by a ceiling it rises, then is moved to its place out of every player's sight or after 1,200 ticks; a trail of vanilla flame particles under it (on); killed aloft, it falls | §4.5 (MZ-15, MZ-16): the jar's Umvuthi cannot walk and has no walk animation, Ben rejected every animal mount, and its legs already hang when nothing is by its feet, as on its throne, so aloft it reads as it does at home, carried by its own sun; a hidden advance would be the war seat again (MZ-3's first form, a puppet placed behind the first wave); a move that collides needs no swim, drowning, suffocation, supernova or Carry On guard, and within reach aloft it can be struck, or traded with by a masked player, as at home. Ben chose it on 2026-10-22 (MZ-16's second round, above); the options he set aside are §4.5's. |
| Chief stack | The leading Umvuthi is outside the party's units (no unit, quota or wave count), but its strength, 5.0, adds to its band's | §4.5: it goes to the fight and fights there, as CONFLICT-LIFE.md §2.7 also counts it. |
| Seen coming | One record per change of a party's march, BAND_MARCH (the route with its ticks, phase, kind, counts, the chief's flags); its position is the route interpolated by time, which is `party_pos`; a band met on the march stands as a column from that position | §4.16 (MZ-15): the far view and the bodies read one source, so a box becomes bodies where it stands. |
| Walk home | A band walking home has bodies where a player is, as one going out does, and is handed back at a camp that is in; its route home starts at `party_chart_pos`; an embodied band that arrives keeps its puppets; a stack outside the ticking edge waits pending until it is inside (MZ-17) | §4.16 (MZ-15): a march that vanished on its way home, or was spawned again at the fight, would break the picture Ben asked for; the folded position would send a band that crossed a seam a lap the wrong way. |
| Rule U | No Umvuthi body is taken or removed while a player is within 160 blocks of it or it has a player as its target, with no limit; a spawn at its throne waits for 160 blocks or 6,000 ticks; a hand-off, a hand-back and a re-pair do not wait (a re-pair removes nothing), and an heir seated in place waits only while the old body is in a fight (a player its target, and a player's hit within 600 ticks). The engine recalls a band with its chief only with no player within 192 blocks, counting every player at their current position and re-binding it to the nearest; a band that arrives with a player that near but none within 160 blocks of its target resolves abstractly and walks home in its bodies. Four removals do not ask: the ConflictBridge's take-back of a stack that stops ticking, an `edge_stop` seam's recall, a failed session's capture, and the recall of a band home with its grove out; vanilla's despawn and Peaceful never remove an Umvuthi | §4.5: a grove's census can run with a player inside the throne's tracking range, and a limit on a removal would let the boss vanish mid-fight. Holding a frozen puppet could save it with its chunk, a body cannot walk across an edge that does not join, a failed session runs nothing more, and a band home with its grove out has ended, its chief placed by the grove's census. |
| Trade memory | Each first payment to an Umvuthi body is a journaled CHIEF_PAID with that body's slot and generation, booked as tribute (the payer's open demand, else a gift); the engine remembers the payers from the live chief's slot and generation since the last succession, plus each census's set from such a body; CHIEF_BODY writes it into each body it places, through `setTradedPlayersCompound`; cleared at a succession | §4.5: a puppet is never saved, CHIEF_STATE is latest-wins and can be dropped, and a dead chief's body must not hand its payers to the heir. A record of its own replaces a flag in TRIBUTE_PAID, whose layout has no room for a generation. |
| Chief's summons | Tagged as the party's puppets and added to the ConflictBridge's groups, each a group of its own holding no unit, by a mixin at the jar's `addFreshEntity`, before they join | §4.5: the jar makes every minion persistent, so they would otherwise be saved far from any grove; the core's join check runs first and asks the ConflictBridge, which refuses a tagged entity it does not know. |
| Devoted deaths | Warriors that fall with the Umvuthi are deaths with vanilla's credit (`getKillCredit`: a player who hurt them in the last 100 ticks, else none); on the road that is the whole column | §4.6: the jar's magic hit has no attacker; crediting the chief's killer with each would add a Killing signal per warrior for one act. Marching puppets stay devoted, as every Umvuthana is in the jar. |
| Throne spot | The Umvuthi's position and direction at the grove's first census; else the fire pit, direction 1 | §4.6: the jar places it 18 blocks from the pit, at a template marker the plugin can read only from the body. |
| Two bodies | Deaths keyed on the body's UUID; a member's first death is its death; the other body drops nothing, takes no warriors with it, books nothing, and is removed | §4.5: `die` raises two death events for one body, so keying on the member would cancel the real Sol Visage; the rule keeps a visitor from a second Sol Visage. |
| Heir in place | An heir whose old chief's body is still on the throne is seated in its place in one tick, a hand-off that waits only while that body is in a fight: a player its target, and a player's hit within the last 600 ticks (PLACEHOLDER) | §4.6: Rule U holds that body's removal while a player stays beside the grove, with no limit, and the heir would have waited on it for as long; a swap in a fight would refill its boss bar, and a target alone lasts while an unmasked player stays within 40 blocks. |
| Facing by a re-pair | The plugin sets the Umvuthi's direction before its first tick at every spawn, and after that turns it by a re-pair: the synced direction, the server's private field (an accessor mixin) and its yaw, body yaw and head yaw set, then a temporary `ServerEntity`'s pairing packets sent as one bundle with `ServerChunkCache.broadcast` to the clients already tracking it, each of which drops its old body of that id and makes a fresh one that reads the new direction at its first tick; the server's body and ChunkMap are untouched (same id, UUID and state; no tracking, join, leave, death or save event on the server; the boss bar keeps its players); only when its way is more than 55 degrees off, with no active ability, at most once in 200 ticks (PLACEHOLDER); no client mod | §4.5: the jar reads its direction only at a client body's first tick, so a client already tracking it would see it keep its first facing at every turn. Rejected: removing and re-adding its tracker (`ServerChunkCache.removeEntity` and `addEntity`), which throws "Entity is already tracked!" when its section turns tracked later, or leaks a tracker; mic-rtsim on every client with one required payload channel (Ben's call); a fresh puppet at each turn (the first round's swap, which had to copy the jar's save, its timers and its kill credit, and remove the old body first); and the stale facing. Whether a client draws the re-pair without a flicker, and the legs' lift and drop on each fresh client body, are test 31's. |
| Over water | The Umvuthi floats 2 blocks over a fluid's surface (the heightmap counts fluids), never settles onto one, and floats on to the first ground, but fights where it floats within 8 blocks of a target on water | §4.5: aloft it never enters water, so it needs no swim and no drowning cancelled. |
| A dying chief | A chief puppet taken back during its death animation drops its loot first | §4.5: the jar drops it only at the 115-tick animation's end, so a stop in that window would lose the Sol Visage. |

**Questions:** none open. Ben answered the last of them, MZ-16's second round and MZ-17, on 2026-10-22
(above).

## 9. Build order

Each slice lands on main by itself with its own tests and leaves main working. Ledger numbers are §2's,
GameTests are §7's by number, and "run N" is `rtsim-cli sim mowzie`'s run N. Every Java slice runs its
GameTests against the real engine. A slice that changes a page format regenerates rtsim-view's fixtures
from fresh GameTest worlds and adds no migration. Record ids are taken when a slice lands, after
`git log --all -S` finds no other branch using them. Choices made while building are recorded in
`docs/choices-mowzie-build.md`. An Opus adversarial review checks M1 to M5 and M10 before each lands.

Three things are left out of this order on purpose:
- **No council.** Mowzie runs on today's ladder (ledgers 2 and 3), so CONFLICT-LIFE.md's council is
  not built here.
- **Two CONFLICT-LIFE pieces are built here, as this plan names them:**
  - BAND_MARCH (ledger 23). Its joint flag and partner agent are written as zero until alliances
    land.
  - The walk-in from the ticking edge (CL-D35, ledger 25).
- **No chat notices, and no pact notices.** Conflict news stays out of chat and is told in the world
  (CL-9 widened, Ben, 2026-10-22: "All faction news in-world"; CONFLICT-LIFE.md §2.17). §4.12's told
  pact is revised, and no slice here builds a text notice: NOTICE_SHOWN keeps only the roar and the
  gong.

| Slice | What it builds | Ledger | Tests | Needs |
|---|---|---|---|---|
| M1 Records | The 17 record kinds and DEMAND_SETTLED's outcome 5, in `boundary.rs` and `docs/boundary.md` §9, decoded in rtsim-jni. A kind whose engine half comes later is counted and dropped until then. DIFFICULTY (no war party leaves on Peaceful), NOTICE_SHOWN (a notice kept until shown, resent at session start), a wiped envoy's withdrawn demand | 14 (in part) | Codec round trips for every kind; rtsim-jni fake-Java tests for Peaceful, the kept notice and outcome 5 | |
| M2 Cultures | `frostmaw.json` and `umvuthana.json`, empty rungs, repeat, `NodeKind::Lair`, threat source and class, destroyed loot, the `gold_nugget` good and tribute by value. No plugin finds these cultures yet, so today's game is unchanged | 2, 3, 9, 10, 12, 13, 15 | Culture and ladder unit tests; the `sim mowzie` scaffold | M1 |
| M3 ConflictBridge | The conflict module's Java half in core, played by vanilla bodies for today's cultures. It covers territories and their hash, territory signals, envoy and payment, the envoy guard, the raid director, steer, recall, casualties and notices. Each stack is its own group: until M10, a stack is placed at BAND_MATERIALIZE's position, not in a column. Also the Peaceful flag, the strength hook, the `threat/ignored` tag and Carry On takes of puppets | 16 (not the camp plumbing), 19, 20, 25 (not the column) | Core GameTests with the real engine: a goblin raid materialized per stack, steered, recalled and killed; 14 with stand-ins; 18; 19 | M1 |
| M4 Camp reconciler, engine | Slots, epochs, the per-camp cap, waiting for whole bodies, CAMP_CENSUS, CAMP_OUT, CAMP_RECONCILE, CAMP_BODIES and CAMP_DEATH | 6, 7 | Run 10 (Java emulated); run 8's reconcile in flight; the faction bench before and after | M2 |
| M5 Camp reconciler, Java | The plugin `neoforge/mowziesmobs`: adapters, camp finding, the census, and the reconcile lines for both kinds. In core: the `camp_member` attachment and its index, markers, footprint levels, and entity chunks in the barrier | 16 (camp plumbing), 18 (in part), 22 | 1, 3, 4, 5, 21, and 22 except its march and aloft cases | M3, M4 |
| M6 Frostmaw | A chief killed at the camp, the den's arrival on `rtsim-kernel::renewal`, CHIEF_STATE, the steal mixin, restriction and the held spawn | 5, 6 (den), 14 (CHIEF_STATE) | 2, 6, 7, 15 (Frostmaw); runs 6 (den) and 7 | M5 |
| M7 Rail | The rail meter, `set_emission_for`, a district-free focus, and the train sampler | 1, 17 | 10, 11; runs 1 (no ambushes), 2, 3, 4 and 9 | M6 |
| M8 Lying in wait | AMBUSH_SPOT, TRAIN_PASSED, TRAIN_ROBBED, the swipe and the roar | 8 | 8, 9; run 1's ambushes, run 8 | M7 |
| M9 Umvuthana | The chief leads; hunted animals; tribute at the grove's price; CHIEF_PAID, TRADED and HAVEN_CHEST; trade memory; the envoy party, raptors, the heir and Rule U | 4, 11, 12, 14 (the rest), 18 (the rest but travel) | 12, 13, 14, 15 (Umvuthi), 16, 17, 20, 25, 27, 30; runs 5, 6 (grove) and 12 | M5 |
| M10 March | BAND_MARCH, the walk home, re-binding, `lose_units` keeping `spawned`; the column, the pending stacks and the walk-in | 23, 25 (the column) | 28, 32, 33, 34, 22's march cases; run 11 without aloft, run 13 | M9 |
| M11 Aloft | The Umvuthi aloft and CHIEF_BODY | 24 | 23, 24, 26, 29, 31, 35, 36, 22's aloft cases; run 11 | M10 |
| M12 S9 | EQUILIBRIUM.md's S9 on the shared renewal rule. If S3 (site ledger, kill credit) is not on main by then, its pieces land first as their own slice, following EQUILIBRIUM.md §4 | | `kill_all_dens`, `kill_all_groves`, `farmed_den_yields_closed_form`, and S9's five GameTests; the sampler bench at 200 trains | M11, S3 |
