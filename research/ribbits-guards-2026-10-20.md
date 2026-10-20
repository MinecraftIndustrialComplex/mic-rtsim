# Ribbits guard mods, 2026-10-20

For Ben (decision C12, caravan guards). Pack: Ribbits 4.1.6, NeoForge 1.21.1. Jars were read with unzip and javap in fresh /tmp dirs; nothing was run.

## Verdict

Yes: **Guard Ribbits** has a NeoForge 1.21.1 build. Its Modrinth page is stale (1.20.1 only, so the 1.21.1 facet search misses it); take it from CurseForge. It is a frog-sized mob body rtsim can spawn, but it does not actually populate villages. YUNG ships no addon: Ribbits 4.1.6 has one entity (`ribbits:ribbit`, with professions) and YUNG's Modrinth list has no Ribbits addon.

## 1. Guard Ribbits (`guard-ribbits`, author sunbatheproductions28)

- **Build.** CurseForge project 1148985, file 8768643, "[NeoForge] 2.0.0 for 1.21.1", 2026-08-30, 51.9 KB. Modrinth t7UhdlGh stops at 1.20.1 (2024-12-03). GitHub source is the 1.20.1 tree.
- **Licence.** Unclear: CurseForge says GPLv3, the repo says LGPL-3 code with assets All Rights Reserved, the jar says CC0-1.0.
- **Dependencies.** NeoForge 21.0.0+, YUNG's API 1.21.1-NeoForge-5.1.5+, GeckoLib 4.7.5.1+ (all already in the pack through Ribbits). Needs Ribbits at runtime: a mixin adds an alert goal to `RibbitEntity`, so hurt ribbits call guards (present, untested).
- **Entity.** One type, `guardribbits:guard_ribbit`, MobCategory CREATURE, box 0.5 x 0.75, GeckoLib model, extends AgeableMob (so PathfinderMob). 20 HP, speed 0.25, attack 3, follow range 16. Goals: float, melee, wander 0.8, open doors, look at player; flees polar bears and ravagers. Targets: anything that hurt it (alerting neighbours), raiders, zombies, and players only while angry (20 to 39 s). Takes 25% damage from a frontal cone. No equipment, no home or patrol goal.
- **Spawning.** Spawn egg and `/summon` only. No biome modifier, no spawn placement.
- **Villages.** Not populated. The jar overrides the `ribbits:ribbits` pool with a sixth piece `ribbits/guard_ribbit`, but that .nbt is byte-identical to `ribbit_nitwit.nbt` (cmp), so it places a nitwit ribbit. This matches the author's "not fixed yet" note.
- **Steerable by rtsim.** Yes: a registered mob with vanilla AI. Call `setPersistenceRequired()` on spawn (despawn not verified). Steer with navigation and `setTarget`.
- **Structures.** None of its own.

## 2. Ribbits: Croaks (`ribbits-croaks`, jonahanoj)

- **Build.** Modrinth VtNWzBdM, 2.0.0, NeoForge 1.21.1, 2026-07-28, 6.9 MB, the only 1.21.1 file. MCreator-made.
- **Licence and deps.** MIT on Modrinth, "Not specified" in the jar. NeoForge 21.1.65+, GeckoLib optional. Declares no Ribbits dependency.
- **Raiders, not guards.** Hostile Croak Soldier (15 HP), Axeman (20 HP, 5 damage), Arbalist (crossbow), Bomber (dynamite), Captain, Croak King (800 HP boss), Amethyst Croak, Jester Ribbit. All extend Monster. They target players and Ribbots.
- **Defenders.** Ribbot (five variants) and Barrel Bot: 10 HP, melee, retaliate only, player-summoned from Ribbot Crates. They do not spawn with villages.
- **Structures.** `croaks:outpost` (swamp, mangrove swamp, spacing 18, holds soldiers, captain, arbalist, jesters), `croaks:shed` (plains, 70), `croaks:purple_mushroom` (swamp, 5). A Boggy Bane potion triggers a Croak Raid on a village.
- **Steerable.** Plain entity types, but Monsters that hunt players.

## 3. Rejected

- **Wandering Ribbit** (`wandering-ribbit`, 4.1.0, 2026-10-08): a trader with a tent structure. No combat.
- **Useful Ribbits** (`useful-ribbits`): Forge 1.20.1 on Modrinth, helper ribbits such as a chef. All Rights Reserved.
- **SpaceFrog's Better Ribbits**: datapack for 1.21.4 and later, raises ribbit HP.
- **Guard Ribbits Updated**, **Ribbits Updated**, unofficial ports: Quilt, Fabric or 26.x builds.
- **Guard Ribbits Fabric 2.0.0**: needs Sinytra Connector, no reason to use it.

## Suggested use for rtsim

Use `guardribbits:guard_ribbit` as the garrison and caravan escort body, spawned by the sim; ignore its village hook. Croaks could supply a bandit faction. Both are one-maintainer mods, so pin versions.
