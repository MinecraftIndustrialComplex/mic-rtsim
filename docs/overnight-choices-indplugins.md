# Overnight choices: the industry plugins, threat sampling and the level-store commands

**2026-10-20, overnight agent 14** (the remaining Java side of industry: the plugins for INDUSTRY.md §6's
hard cases, the threat sampling of §13.4 and §13.5, and the commands the Java side answered FAILED).
Ben said to take the recommended option for reversible choices and log it. Every I-proposal is still
open in INDUSTRY.md §20; this file records which recommended option the code relies on, and every smaller
choice made to build it. INDUSTRY.md, DECISIONS.md, PLAN.md and OPEN-QUESTIONS.md are unchanged. The
engine's choices are F# (`docs/overnight-choices-industry.md`), the earlier Java side's H#
(`docs/overnight-choices-industrymc.md`), the native link's W# (`docs/overnight-choices-industryjni.md`).

**Numbers.** Every default named PLACEHOLDER is one; nothing was tuned. Measured numbers say where they
come from (a GameTest on this laptop, AMD Ryzen AI MAX PRO 390, on the stub engine unless it says
otherwise).

## Proposals relied on (recommended options)

| Proposal | What the code does |
|---|---|
| I11 | Ore Excavation veins, Diesel chunk oil and Destroy crude oil are deposits (goods of class `d`), read each sweep, written back with the chunk's reconcile (Ore Excavation, Destroy) or at once by IND_DEPOSIT_DELTA (Diesel). |
| I12 | IND_CONDITIONS from Serene Seasons' calendar (the one mic-climate's YearClock reads) when the climate plugin is present, else vanilla's "no seasons". |
| I13 (as corrected on 2026-10-20) | An Ender Transmission channel's endpoints are one district (Ben: a stop-gap for tightly linked machinery, not long-distance transmission); PowerGrid lines between districts are measured power links (IND_POWER_LINK). |
| I14 | Destroy pollution attributed by the position of each `changePollution`; written back at the district's reconcile (see Q9 for "level types live"). |
| I16 | Warn first: a logistics network spanning districts is a ledger warning; no mixin (Q8). |
| I20 | Natural spawns per dark spawnable block, measured while a player is in the district at night. |
| I21 | The defence survey: perimeter, dark area, roofs, radar auto-turrets with ammunition, kill machines, guards; real fights observed. |

## Choices

**Where things live**
- **Q1.** Each plugin is a Gradle subproject under `neoforge/` sharing `neoforge/plugin.gradle` (the pack's
  jars compile-only and on the GameTest server, Create's jar-in-jar libraries for compiling). A mod's
  version range in `neoforge.mods.toml` follows the mod's own version string (Diesel Generators calls
  itself `1.21.1-1.3.15`, so its range starts there).
- **Q2.** Mod ids (A19): `mic_rtsim_oreexcavation`, `mic_rtsim_dieselgenerators`,
  `mic_rtsim_endertransmission`, `mic_rtsim_powergrid`, `mic_rtsim_destroy` after their mods;
  `mic_rtsim_climate` (built around `mic_climate`, so not `mic_rtsim_mic_climate`) and `mic_rtsim_radar`
  (built around Create Radars and Create Big Cannons together).
- **Q23.** The plugins share a test kit in the core's GameTest package (`IndustryTestKit`: the session's
  engine, received records, and industry commands scripted on either engine, W17's way). The core's
  threat tests run in the `mic_rtsim` namespace on a copy of the Create plugin's yard
  (`industry_yard`). Create's own `IndustryKit` is unchanged.
- **Q24.** The threat sampler's and the conditions' settings are a config file of their own,
  `mic_rtsim-threats-server.toml` (all PLACEHOLDER), so the industry config keeps its keys.
- **Q25.** `Ledgers.warn(district, key, text)`: plugins put warnings on a district's ledger; the display
  source shows them after the goods when there is room, the chat ledger always.

**Deposits (I11, F15)**
- **Q3.** A deposit good is `d/0/<kind id>/<place hash>`, the hash 16 hex digits of FNV-1a 64 over
  `dimension|chunkX|chunkZ` (`Deposits`). The engine keys a district's deposits by good, so two chunks'
  veins must be two goods; `Goods.parse` reads the hash as the component hash, and a plugin checks it
  against the chunk a command names.
- **Q22.** IND_DEPOSIT_DELTA is routed by the core to whichever plugin owns the good
  (`IndustryPlugin.applyDeposit`), not to one registered handler: the command's plugin code says which
  mod, the good says which deposit. The journal's applied set is the dedup (level SavedData and the journal
  reach disk at the same save, as H17). APPLIED when a plugin owns the good, FAILED otherwise. A reconcile's
  deposit delta (inventory 0xFFFFFFFF) goes the same way, in the chunk's own tick, with its epoch.
- **Q4.** Ore Excavation: a vein is read only in chunks with a drilling machine or extractor
  (`OreDataAttachment.getData` generates a vein on first access, so the sampler never touches other
  chunks). Remaining is in extraction cycles (`getResourcesRemaining`); Ore Excavation's 0 is an infinite
  vein (sent with the infinite flag) and -1 an exhausted one (sent as 0). Written with `OreData.extract(n)`,
  clamped so a vein never goes below empty.
- **Q5.** Diesel: chunk oil in millibuckets, read in chunks with a pumpjack hole (which pumps its own
  chunk). A chunk nothing has written is recomputed by Diesel on every read (a 12,800-sample biome scan),
  so the plugin reads Diesel's map through an accessor and computes an unwritten chunk once per session.
  `Integer.MAX_VALUE` is Diesel's infinite chunk: never stock. Writes clamp at 0.

**Mail and packages**
- **Q6.** IND_PACKAGE now goes out for loaded districts too (reversing H30's cost): the engine learns a
  loaded district's mail rates (F26). The train's cargo port at departure subtracts what the station's mail
  moved between the train and every postbox of the station (`MailWatch` tells `TrainWatch`), so mail is
  counted once, by the postbox's own district. This also fixes a double count H30 left: packages for an
  unloaded district's postbox were in the loaded station district's cargo port as well.
- **Q21.** IND_WRITE_PACKAGE: into the postbox's inventory when its position is loaded and holds a postbox
  (as Create's own mail would), with the COMMAND_RESULT carried in the chunk until acknowledged (the
  pending-record pattern); else into the station port's offline buffer (marked primed, so the postbox takes
  it back at its load) with the journal as the dedup. A package whose contents need more than nine stacks
  is several boxes; each package is written whole or not at all; a partial write is APPLIED and the rest
  logged and counted (the engine keeps its books as written, W10), none at all is FAILED.

**Channels (I13)**
- **Q7.** (Reworked after Ben corrected I13: Ender Transmission in this pack is a stop-gap for tightly
  linked machinery, such as a turret on a Sable vehicle powered by an engine in its body, never long-distance
  transmission.) Every transmitter links, through the plugin `links` hook, to the other live transmitters on
  its channel (the same kind: items, fluids or rotation; the same channel number and password) in the same
  level, so the district detection joins their chunks into one district however far apart they are. The
  channel's buffer is machinery inside the district, like a pipe's contents: no channel port, no
  IND_CHANNEL_SAMPLE, and IND_CHANNEL_DELTA is not applied (the core answers it FAILED). Ender
  Transmission keeps no list of its item and fluid endpoints, so a mixin at the end of the transmitter
  block entity's constructor registers each in a weak set; rotation transmitters are read from the mod's
  own `EnergyNetwork.ENERGY` lists without changing them. An endpoint in another level cannot join one
  district: the districts on such a channel get a ledger warning (checked every 200 ticks, PLACEHOLDER),
  and what crosses it is learned as the districts' own production and use. A link is seen only while
  both endpoints' chunks are loaded (as chain conveyors). The settings screen is no block edit, so the
  200-tick check also compares each transmitter's channel and password with the last check and marks its
  chunk for detection when they changed. A transmitter on a Sable vehicle is UNVERIFIED (no Sable in the test
  servers).

**Gauge networks (I16)**
- **Q8.** Warn only. A Create 6 logistics network whose stock links are in two or more districts puts a
  warning on each one's ledger naming the others, checked every 200 ticks (PLACEHOLDER). The mixins I16's
  second step names are not built: Create's summary (`getSummaryOfNetwork`) reads only loaded links'
  inventories, so counting a simulated district's links as present would let gauges order from stock
  nobody can read, and the packagers that would fulfil the order are in the unloaded district and do not
  run. Fulfilling into offline buffers needs the engine's mail model behind it first.

**Pollution (I14)**
- **Q9.** Each district's attributed pollution is a deposit good it produces: the cumulative change of each
  pollution type at the district's positions (`PollutionLedger`, saved in
  `data/mic_rtsim_destroy_pollution.dat`), reported each sweep at the district's anchor chunk with Destroy's
  plugin code, so the engine learns it as an output of the box with no pollution concept of its own. A
  reconcile's delta writes it back at the anchor chunk: level types (greenhouse, ozone depletion, acid rain)
  to the level, chunk types (smog, radioactivity) to that chunk. **Not built: "level types live".** The
  engine writes a plugin's deposits back with the chunk or at once by plugin, not by good, so level types
  wait for the next load; the Java side already applies an IND_DEPOSIT_DELTA for a pollution good if the
  engine sends one. Measured in `pollutionIsAttributedAndCrudeOilIsADeposit`: +40 greenhouse and +12 smog
  attributed, +9 smog outside the district not.

**Power (I13, §7)**
- **Q10.** Batteries are plugin stock: `e/0/powergrid:joule`, whole joules (Power Grid keeps a double in
  joules), energy capacity the battery's, written with `setEnergy` clamped to [0, capacity]
  (`IndustryPlugin.describe` gives a plugin store its shape). Power links are measured: every 20 ticks, for
  each line whose two ends are block terminals in two districts, both loaded, the current from one district
  to the other times the voltage at the sender's end, summed over the lines between them (over a closed
  circuit that sum is the power crossing the cut whatever the voltage reference). Every 200 ticks the
  window's mean decides: more than 1 W (PLACEHOLDER) and the sender is the source; a sink keeps the source
  that delivers most (the engine's link has one). A link ends when a window saw both districts live and no
  line between them. Sent links persist in `data/mic_rtsim_powergrid_links.dat`. Lines ending at a free
  junction are not followed. Measured in `aLineBetweenDistrictsIsAPowerLink`: a 5 V creative source over
  two wires, 51.5 A, 257 W leaving the source's district (2.65 W reach the resistor: the wires take the
  rest).

**Threats (I20, I21)**
- **Q11.** Threat classes: the entity type tags `#mic_rtsim:threat/ground_melee`, `ground_ranged`,
  `flying` and `siege` first (a pack can classify modded mobs), else vanilla: creepers siege, flying mobs
  flying, skeletons and ranged attackers ground ranged, the rest ground melee. Guards: player-built iron
  golems and owned animals (`OwnableEntity` with an owner: wolves, Ice and Fire's tamed dragons); each
  fights walkers and sieges, only flying or ranged guards also flyers.
- **Q12.** A real fight is an episode per (district, class), from the first engagement (a hostile mob in the
  district targets a player or a guard, deals or takes damage there, or explodes there) to
  `fightGapTicks` (200, PLACEHOLDER) without one. Attackers are distinct mobs, killed ones those that died in
  the fight, guards lost those a hostile killed, blocks destroyed the non-air blocks an explosion by a
  hostile took in the district, machines destroyed those of them with an industrial non-stock block entity.
  Strength is 1 per attacker, the engine's natural-mob unit (F17, itself a PLACEHOLDER). Items stolen: 0
  (vanilla mobs take nothing from containers; a modded thief is not observed).
- **Q13.** Spawn samples: a hostile mob's natural spawn (`FinalizeSpawnEvent`, NATURAL, not
  spawn-cancelled) counts for the district whose chunk or ring (1 chunk, PLACEHOLDER) holds it, while that
  district is observed: a player (not a spectator) in one of its chunks while `isNight()`, checked every 20
  ticks. A sample goes out after 2,400 observed ticks (PLACEHOLDER) or when the observation ends, with the
  dark spawnable blocks of the district and its ring from the last survey (a sample waits for a survey).
  The rate is biased low for a small district: vanilla spawns nothing within 24 blocks of a player.
- **Q14.** The survey's walk surface is at the district's ground level: the median height of its
  industrial blocks. Each column's standing height is the one nearest that level within 4 blocks
  (PLACEHOLDER) with a solid top below and room for a two-high mob, so a roof does not hide the floor under
  it. A walker steps into a neighbour at most 1 higher, drops any distance, and never enters a fence, wall
  or fence gate; a machine can be attacked from any open neighbour.
- **Q15.** `perimeterGaps` is the smallest number of open columns that cut every machine off from the
  outside (a minimum vertex cut, Edmonds-Karp on unit capacities); `perimeterColumns` the same cut with
  every step allowed (the wall a district would need at least). The graph is the machines' bounding box
  plus 8 columns (PLACEHOLDER), at most 40,000 columns (PLACEHOLDER; past that both are sent as 0 and a log
  line says so). Kill machines at gaps: saws, drills and crushing wheel controllers in a column a walker
  reaches, or next to one. Armed deployers are UNVERIFIED (whether one hurts depends on its held item and
  mode) and not counted. Measured in `theDefenceSurveyMeasuresWallsRoofsAndDark`: a furnace in the open has
  4 gaps of a 4-column perimeter, a closed ring 0, a one-column doorway 1.
- **Q16.** Dark spawnable: positions from a column's top surface down 16 blocks (PLACEHOLDER) where a zombie
  could spawn (`SpawnPlacements.isSpawnPositionOk`) at block light at most the dimension's
  `monster_spawn_block_light_limit`; the district's own (IND_SURVEY) and its ring's (the spawn samples).
  Roofed machines: the block above cannot see the sky (`canSeeSky`), so a glass roof is no roof.
- **Q17.** A turret is a Create Big Cannons mount in the district that a Create Radars network drives (one
  of its weapon mounts, or the block a pitch controller of its faces), whose targeting has mob,
  auto-target and auto-fire, whose radar is loaded and running, with an assembled cannon, and that cannon
  an autocannon with rounds (input buffer plus magazine). Each counts 1.0 against every class: the unit is
  "one turret", not calibrated (the engine's untested weight and observed fights calibrate it). The
  ammunition good is the magazine's main round. `ammoPerEngagement` is not measured (0, which the engine
  reads as always fed). Big cannons' loaded state is UNVERIFIED and they are left out; CBC Modern Warfare's
  compact mount is not driven by Create Radars at all.
- **Q18.** A survey runs when a district's revision changes, after a player places or breaks a block in it,
  and every 24,000 ticks (PLACEHOLDER) while all its chunks are loaded, at most every 200 ticks
  (PLACEHOLDER); one at a time, 2 chunks a tick (PLACEHOLDER); a district chunk that unloads aborts it, and a
  request for the same district restarts it. The walk from outside and the two minimum cuts run on Minecraft's background
  executor once the chunks are read (they use only the survey's own maps); the records go out on the
  server thread, and a district that changed meanwhile drops the result. IND_SURVEY_BLOCKS lists wall faces first, then machines, then
  boundary stock, at most 2,048 (PLACEHOLDER).

**Conditions (I12)**
- **Q19.** Vanilla sends season 0 of 1, length 0 (the engine reads that as no season boundaries). The
  climate plugin sends Serene Seasons' four seasons, each its season duration, season 0 begun its cycle ticks
  ago in the overworld. Checked every 100 ticks (PLACEHOLDER); sent again when the season changes or the
  origin moves more than 1,200 ticks (PLACEHOLDER: sleeping skips the cycle ahead; a paused cycle moves it
  a little at each check).
- **Q20.** Temperature: the plugin hook and `Conditions.celsius` give mic-climate's ambient temperature in
  degrees Celsius (`Climate.celsius`), but nothing sends it: IND_CONDITIONS has no temperature field, and
  temperature is per position. A per-district latest-wins record is the proposal (see the end).

## Findings for Ben

1. **Ore Excavation's default config makes veins infinite** (`defaultInfinite` true in its server config).
   Infinite veins are never stock, so deposits only matter if the pack makes veins finite.
2. **Create Radars never checks ammunition** (`isCannonReadyToFire` is true for any mount), and CBC Modern
   Warfare's compact mount cannot be driven by it.
3. **Ender Transmission's channel buffers are JVM statics** that are never cleared, and looking up a fluid
   transmitter's capability creates its channel entry. Since I13's correction the engine's channel stock
   (F8: IND_CHANNEL_SAMPLE, IND_CHANNEL_DELTA, IND_PORT kind 7) has no Java producer; its owner can retire it.
4. **The GameTest yard's floor is the layer the industry tests place blocks in** (y 1 of the structure):
   a block "on the floor" replaces it. The survey test builds its walls one block higher to stand two above
   the floor.
5. **A sub-agent of this run (the radar and Create logistics lookup) took about seven hours** under the
   account's rate limit; the rest of the work went on without it.
6. **The create GameTests' "goods' names do not resolve" on the native engine** (`totals {null=86}`, four
   tests failing on main since the herds and the world model added records) was the test log, not the
   bridge: `NativeEngine` kept only the last 10,000 received records, and a busy suite dropped the
   session's first DICT entries with them. It now keeps every DICT name apart (`javaDictName`); create
   passes 18 of 18 on the native engine again.
7. **A GameTest world saved before kernel2's page format 2** (`run-gametest/world/rtsim`) stops the native
   engine at start ("format 1, this build reads 2"), and every test of that project fails. Delete the
   project's `run-gametest/world` after merging the kernel change.

## Proposal: IND_CLIMATE (not built)

A latest-wins record per loaded district, for I12's "conditions seen while learning": `8 i64 gameTime,
16 u64 district, 24 f32 celsius (NaN: no climate mod), 28 f32 biome temperature (vanilla's unitless
value)`, sent once per sweep. It needs `wire.rs`, the engine's model conditioning and a fixture; the Java
side has the values (`Conditions.celsius`, `Conditions.biomeTemperature`).
