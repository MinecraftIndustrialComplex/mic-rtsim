# Aeronautics ship mods, 2026-10-20

Question: which NeoForge 1.21.1 mods put pirate, naturally spawning or crewed ships on Create Aeronautics 1.3.2 and Sable 2.0.5, without adding much world structure? Sources: Modrinth API, CurseForge search pages, the latest jars (listed with `unzip -l`, JSON read with `unzip -p`, nothing run), and the authors' GitHub docs. The pack has Create 6.0.10, Sable 2.0.5, bundled Aeronautics 1.3.2 (carries Simulated), Lootr, Create Big Cannons 5.11.7, Structurify and GeckoLib 4.9.3.

## Mods that spawn or crew ships

| Mod (slug) | Latest, date | Licence | Deps (all in pack unless noted) | World structure | Ships | Crew |
|---|---|---|---|---|---|---|
| Create: Hostile Skies (`create-hostile-skies`) | 0.2.6, 2026-09-23 | MIT code, All Rights Reserved assets | Create, Sable, Aeronautics/Simulated; Lootr optional | None (no worldgen files) | Real Sable sub-levels from structure NBT; 6 ships, balloons with rudder only | Vanilla pillagers, captain with weapon; mob-like |
| Create Aeronautics: AMBUSH (`ambush`) | 1.1.6, 2026-09-22 (CurseForge marks it beta; stable is 1.1.5, Aug 22) | Custom, modpack and addon use permitted | All optional: Create, Aeronautics, Sable, CBC 5.11.7 (in pack); Lootr; FTB Quests and Guard Villagers (not in pack) | None; 188 encounter definitions, 78 NBT templates | Real Sable sub-levels (`sable_structure` action); airships, boats, cars, fleets | Vanilla pillagers, skeletons, piglins, villagers; they fire CBC cannons; mob-like |
| Create Aeronautics: Discovery (`create-aeronautics-discovery`) | 2.2.0, 2026-09-17 | All Rights Reserved | Create, Aeronautics (in pack) | Light: one balloon set (spacing 32, separation 16) plus balloons injected into villages | Real Sable sub-levels: assembled when a player is in range; planes fly autopilot goals | Soaring Trader (wandering-trader variant, peaceful), vanilla pillager aviators |
| Create Aeronautics Structures (`creat-aeronautics-structures`) | 1.1.0-Beta-3.1, 2026-07-09 | MIT | Create, Aeronautics, Sable, GeckoLib (in pack) | Heavy: 12 structures, sets at spacing 31/12 and 50/25 (denser than villages) | Static blocks, no Sable code | Engineer trader (GeckoLib villager) only |
| Aeronautics Structures (`aeronautics-structures`; CurseForge "New Structures") | 1.2, 2026-08-13 | All Rights Reserved | Create, Aeronautics, Sable | Medium: 2 balloon structures in the jar (airships advertised, not added), spacing 55 to 65 | Static, "not assembled" | None |
| Sable Encounters (`sable-encounters`) | 1.9.9, 2026-07-23 | CC0 on Modrinth, CC BY-NC in jar | Sable, Photomancy and LDLib2 (neither in pack) | Heavy: 4 sea sets at spacing 32/2 | Sub-levels pasted by invisible entities at worldgen | Boss-bar encounters; MCreator-built, 549 downloads |
| Create: Hangars (`create-hangars`) | 1.0.0, 2026-04-27 | CC-BY-4.0 | Create, Aeronautics | Heavy: 9 hangars, spacing 32/18 | None (loot buildings) | None |
| Physical_structures API (`physical_structures`) | 1.0.6, 2026-07-01 | MIT on Modrinth, All Rights Reserved in jar | Sable, `cbc_autotarget` (not in pack) | One gun feature | Placer API with events; not ships | None |

Dismissed (no Aeronautics ship spawns): Aeronautics Ship Structures (two static ocean ships, needs Villager Warship), Aeronautical Explorations, Aeronautics Military Structures, Aero Islands (sky-island worldgen), Pirates of Heaven Core (server ownership framework), Avarii Airship (static pirate structure), Warship Arise and Valkyrien Pirates (Valkyrien Skies), Pillager Pirates (Fabric), Skeleton Ghost Ship and Illager Warship (plain structures). Useful helpers: Sable Pathfinder (mobs path across sub-levels, `sable-pathfinder` 1.4.0, depends on Sable only) and Sable Photomancy (schematic authoring, MIT).

## How a simulation mod could drive them

None has a route-following API. MIC's plan (PLAN.md section 7) already spawns sub-levels itself through Sable's `allocateNewSubLevel` recipe, so the mods supply crews and templates, not movement.

- **Hostile Skies:** set `enableRaidSpawns = false` in `hostile_skies-common`. Ships are datapack files (`data/<ns>/hostile_skies/ships/*.json` plus a structure NBT, `/reload`, see `CUSTOM_SHIPS.md`). `/hostileskies spawnraid` and the static `RaidSpawnSystem.spawnShipAt(level, pos, target, template, badOmen)` place one. Autopilot uses the wheel and levers; the raid manager despawns after 14 minutes.
- **Ambush:** `/ambush admin spawning` switches natural spawns off; `/ambush disable <pack>`, `/ambush rate` and per-definition toggles refine it. `/ambush _<ns:id> [player]` starts one. Our own ship profiles and NBT count as Independent Content under its licence. Ships orbit or track a player; no point-to-point travel found.
- **Discovery:** best autopilot design (data-driven `straight`, `altitude`, `orbit` goals), `/spawnprefab`, `/discovery flyover toggle`; disable the balloon set with Structurify.

Licensing: Hostile Skies ship NBT and Ambush templates are protected, so the sim should ship its own templates.

## Ranked recommendation

1. **Hostile Skies.** Zero structures, real sub-levels, vanilla pillager crews, simple datapack format, MIT code to read or port. Cautions: early access, only balloon ships with a rudder, needs simulation distance 10.
2. **Ambush**, only if Ben wants naval combat and fleets. Pack versions match its ranges (including CBC). It is large (188 definitions, including many ground ambushes), its cannons fire explosive shells, and 1.1.6 is a beta. Restrict it to ship content with the disable commands.
3. **Discovery**, as a design reference or with structures off. Its peaceful flyover traders overlap the sim's merchant ships.
4. Reject Sable Encounters and every structure-only mod: Ben's no-heavy-structure rule, and they add no moving ships.

## Choices made

Counted Modrinth dates over CurseForge. Judged "heavy" by structure-set spacing against the village default (34/8).
