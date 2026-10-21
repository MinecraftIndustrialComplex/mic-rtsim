# The oil gate

**Status:** Ben approved OG-1 to OG-8 and OG-10 to OG-13 as recommended (2026-10-21). The dragon
half is built (2026-10-21): the engine side in `crates/rtsim-ecology/src/oil.rs` (with the drinking
raid and the roof gate in `crates/rtsim-industry`), the plugin side in `neoforge/iceandfire`, the
records in docs/boundary.md §6 and §6.8, the choices in `docs/choices-oilgate.md` (OGC#). Field
flights ship switched off (`fieldFlights` false) until §4.8's measurement passes; In Control is
untouched. §4.8 was first measured on 2026-10-21 (two runs by day, OGC35): at site 1 the gate's field
flights matched In Control's damage per minute but hit 40 to 50% later and brought a third to a half
as many dragons, so the gate does not pass yet (For Ben 1 in the choices file). The underground half
(OG-9, §4.6) is not built; its seam is in `docs/choices-oilgate.md`. The design below is as approved;
where the build chose a number or a detail, the choices file says so.

**Decided, and binding here:**
- **P13:** the desert dragon spawner is a progression gate: oil sits behind guns, through hostile
  ground. Deep Time will place oil deposits anywhere, so the gate follows deposits, not biomes:
  dragons use oil to breathe fire, so they must regularly return to oil and nest near it; dragons sense
  players starting industrial activity near an oil deposit, and target them; an underground threat near
  deposits stops players cheesing the gate by drilling to the oil from below. **In Control stays
  untouched until the sim version is measured to be at least as dangerous.** Ben: "This needs a careful
  touch, the idea behind this system was. A) Restrict oil to desert biomes. B) Make desert biomes
  extremely hostile to players s.t. they would need guns or a similar level of progression in the tech
  tree in order to soft-lock oil behind guns." and "Danger follow deposits, though potentially make it a
  bit more interesting."
- **P12:** dragons prey on caravans, village livestock, wild herds, and the farms of players they hold a
  grudge against. **P26:** a player's home is the region where they spent the most time recently.
  **P28:** watched dragon fights place bounded sim damage too; Ice and Fire griefing stays off.
  **M1:** pairs breed in their lair; stealing an egg is a theft the parents remember. **I14:** Destroy
  pollution from unloaded factories is attributed by position.
- **Also in force:** C31 (the topology layer), A15 (booked legs), I19 to I24 (threat sources, natural
  pressure, defences, resolution, damage, trains), C24 (threats get bored), D1, D3 (no bespoke blocks,
  items or entities), D4, D7.

**Sources.** Create Diesel Generators 1.3.15, Destroy 0.4.1, Ice and Fire CE 2.1-beta.1, In Control
10.3.0, Born in Chaos 1.7.6, Mowzie's Mobs 1.8.2, L_Ender's Cataclysm 3.33, Mutant Monsters 21.1.1,
Bosses of Mass Destruction 1.3.3, L2 Hostility 3.0.18, TaCZ, all in the Prism instance `mods/` folder
and matched against `pack/mods/*.pw.toml`, read with javap and cfr. Pack data: `pack/config`,
`pack/defaultconfigs`, `pack/kubejs`. Vanilla: the decompiled 1.21.1 sources in the Gradle cache. Deep
Time: `deep-time-latitude/neoforge/.../OilChunksSavedDataMixin.java` and `compat/cdg/CdgBridge.java`.
The engine: `crates/rtsim-ecology` (dragons), `crates/rtsim-industry` (districts, threats, deposits).
Nothing here was run in a game: "verified" means read in code or data.

**Numbers:** every number is a verified fact, arithmetic from one, or a labelled TUNABLE or
PLACEHOLDER. Anything unverified says UNVERIFIED.

---

## 0. What the player sees

Oil is found with the Diesel Generators oil scanner, as now. The ground over oil is dragon country:
fire dragons come to drink there, nest near it, and arrive within a minute or two of anyone standing
on it, as In Control's desert dragons do today. Start a base near the oil and the dragons that drink
there take it as an intrusion: once you drill, store crude or foul the air, they come for your works
and for you, watched or not. Dig down to the oil instead and you find the deep layer under every oil
field grown over with sculk: shriekers that call wardens when you make a noise. A player with iron gear
and a bow can visit an oil field and run; holding one takes guns.

## 1. Reuse table

| What happens | The real mechanic | Mod (verified id or class) |
|---|---|---|
| Where oil is | `OilChunksSavedData.getBaseOilAmount(ServerLevel, ChunkPos)`; pack: oil only in `#c:is_desert` (`oil_biomes` replaced), normal multiplier 0.0, high 2.0, threshold 4,000,000 mB, infinite above 10,000,000 | createdieselgenerators, pack kubejs and defaultconfigs |
| Where oil is, under Deep Time | Deep Time's mixin at the head of `getBaseOilAmount`: the share of a petroleum trap's charge under the chunk, else 0 (`CdgBridge`); sites from `LevelSites` | deeptime |
| Finding it | `createdieselgenerators:oil_scanner`: none, low, high or bottomless for the player's chunk | createdieselgenerators |
| Drilling it | the pumpjack hole scans down through pipes to a block in `createdieselgenerators:oil_deposit` (= `minecraft:bedrock`), every 20 ticks, and depletes the chunk unless it is infinite | createdieselgenerators |
| The engine hearing of it | `IND_DEPOSIT_SAMPLE` (0x0042), plugin 2 (Diesel oil), from member chunks with a pumpjack hole | mic_rtsim_dieselgenerators |
| Dragons | Ice and Fire fire dragons, sim-owned (rtsim-ecology: lairs, hunger, flights, grudges, raids through the industry threat interface) | iceandfire, mic_rtsim_iceandfire |
| Fire breath | `DragonBaseEntity.breathAttack` posts the cancellable `DragonFireEvent` first; cancelled, no breath, no particles, no damage | iceandfire |
| Lairs | `iceandfire:fire_dragon_roost` (surface) and `fire_dragon_cave` (underground, centre about y -24 to 5) in the fire biome tag, which includes desert | iceandfire |
| A lair placed near oil | vanilla `/place structure`'s code path (`Structure.generate` with every biome allowed, then `placeInChunk`) | minecraft, iceandfire |
| Sensing | districts (I2), deposit samples, boundary goods, Destroy pollution by position (I14) | mic_rtsim industry, destroy |
| Being raided | `Infrastructure::raid`, class Flying, Lanchester against the district's defence vector, theft and destroyed blocks capped (I22, I23) | mic_rtsim |
| Dragons drinking from tanks | theft of boundary goods (I23): crude oil (`destroy:crude_oil`, what the pack's Diesel pumpjack pumps), diesel, gasoline | mic_rtsim industry |
| Grudges | rtsim-knowledge reports and sentiment; grudge raids at the player's home (P26) | mic_rtsim |
| The underground threat | vanilla sculk: the `minecraft:sculk_patch_ancient_city` configured feature (catalysts, sensors, and shriekers that can summon), `WardenSpawnTracker`, the warden (500 health, 30 attack, sonic boom 10 through walls) | minecraft |
| Today's desert danger, the yardstick | `pack/config/incontrol/spawner.json` and `spawn.json` | incontrol |
| Guns | `tacz:gun_smith_table`, made by Create mechanical crafting in the pack; the earliest guns need Create Big Cannons autocannon parts | tacz, createbigcannons, pack kubejs |
| Measuring | the mic-rtsim GameTest harness with mock players, `LivingIncomingDamageEvent` | neoforge |

## 2. Custom-roll ledger

1. **The oil field index** in the engine: oil chunks and fields, from Deep Time's sites or CDG's own
   amount (§4.1).
2. **Field flights** (§4.2): the floor that keeps In Control's danger where oil is.
3. **The oil reserve** on each sim dragon: two saved fields, a closed form, a `DragonFireEvent` handler
   (§4.3), and a `Refuel` purpose.
4. **Lair choice weighted by oil**, and lair placement where a field has none (§4.4).
5. **One report kind** (`Encroachment`) and a `District` target for dragons (§4.5).
6. **One placed-feature file** in the mic_rtsim datapack and one boundary record to place it (§4.6).
7. **R18's exemption** for In Control's dragons until the gate passes (§3.3).
8. **The measurement harness** (§4.8).
9. **Roof cover in `resolve()`**, already designed in INDUSTRY.md §13.6 and not built (§4.5).

No new blocks, items or entities.

## 3. What happens today

### 3.1 Oil

- **Desert only, now.** `pack/kubejs/data/createdieselgenerators/tags/worldgen/biome/oil_biomes.json` is
  `{"replace": true, "values": ["#c:is_desert"]}`, and `createdieselgenerators-server.toml` sets the
  normal-chunk multiplier to 0.0. So Ben's A ("restrict oil to desert biomes") is already in the pack.
  Terralith's deserts are in `c:is_desert`.
- **CDG's formula:** the chunk is in an oil biome if any biome sampled at y 60 to 109 is in the tag. Then
  `((Perlin(world seed, octaves -2 to 1) at (cx, cz) + 1) / 1.6)^2 × 7,000,000 × 2.0`, zero below
  4,000,000, `Integer.MAX_VALUE` (bottomless) above 10,000,000. The biome lookups go through
  `ServerLevel.getBiome`, which reads the biome source for a chunk not yet generated, so the amount of
  any chunk can be computed without generating it (verified in vanilla's `LevelReader.getNoiseBiome`).
  The loop samples every block column at 50 heights: 12,800 lookups per chunk.
- **Destroy's pumpjack is unreachable, but Destroy's crude oil is what comes out.**
  `kubejs/startup_scripts/voided_content.js` voids `destroy:pumpjack` and `destroy:seismometer`, and
  `oilyunification.js` removes their recipes. The Diesel pumpjack is the only way to oil, and it pumps the
  first entry of the fluid tag `#createdieselgenerators:pumpjack_output`. The pack overrides that tag
  (`kubejs/data/createdieselgenerators/tags/fluid/pumpjack_output.json`, `replace: true`) to
  `destroy:crude_oil`, so the fluid is Destroy's, not CDG's `createdieselgenerators:crude_oil`. The override
  sat in a `tags/fluids` folder (plural) until 2026-10-21 and never loaded; a 1.21 tag folder is singular.
  (Destroy's own field, `ChunkCrudeOil`, has no biome gate.)
- **Deep Time** replaces the biome rule in its levels: oil sits only over petroleum traps, `round(charge
  × cdg_scale × the chunk's share of the trap)`, with CDG's infinite threshold applied. Traps are few per
  planet and known when the world is made.
- **What the engine learns today** is `IND_DEPOSIT_SAMPLE` for chunks that hold a pumpjack hole and
  belong to a district. An undrilled field is invisible to it.
- **Drilling reaches bedrock.** The pumpjack hole's pipe must reach bedrock, so every well, wherever its
  hole is, passes through the deepest layer of its chunk.

### 3.2 In Control's desert

Verified in the jar and the pack's files:
- `spawner.json` spawns `iceandfire:fire_dragon` (persecond 0.1, amount 1) and `minecraft:blaze`
  (persecond 1, amount 2 to 5) in the overworld, 25 to 100 blocks from a player, y 15 to 200,
  `norestrictions`, `maxthis` 200 each.
- `spawn.json` lets those joins through in `c:is_desert` and denies them elsewhere (`incontrol: true`
  means "during In Control's own spawn", so other spawns of those mobs are untouched).
- `SpawnerSystem.checkRules` runs every 20 ticks per dimension; `persecond` is a chance per run; each
  run picks one random player of the dimension, then tries 20 positions. `maxthis` counts the whole
  dimension. The amount is `min + nextInt(max - min)`, so blazes come 2 to 4.
- So **one player alone in a desert faces up to one fire dragon attempt every 10 seconds** (0.1 a
  second), until 200 are alive in the dimension; with N players in the dimension, a given player is
  picked one run in N. Blazes come in groups of 2 to 4, up to one attempt a second.
- Each dragon gets Ice and Fire's own spawn rolls (`DragonBaseEntity.finalizeSpawn`): age `1 +
  nextInt(80)` days, so stages 1 to 4: 30% stage 1, 31% stage 2, 31% stage 3, 7.5% stage 4. Max health
  grows from 20 at day 0 to 500 at day 125 (4% of the pack's `dragon.maxHealth` 500 upward), so these
  dragons have 24 to 327.
- With griefing off, they break nothing. **The desert gate is a danger to players, not to machines.**
- **Below y 15 nothing spawns.** A player who reaches the oil underground meets none of it.

### 3.3 A conflict that is live now

R18 (docs/overnight-choices-dragons.md) discarded natural and spawner dragons when the Ice and Fire
plugin ran (`SpawnKind::Natural | SpawnKind::Spawner`, `rtsim-ecology/src/engine.rs`). Ice and Fire
has no natural dragon spawns of its own (its dragons come with lairs), so in this pack every natural
fire dragon is In Control's. **With mic_rtsim_iceandfire installed, the desert gate is gone**, against
P13's "In Control stays untouched until measured". OG-1: until the gate passes, the plugin leaves
natural fire dragons alone (not adopted, not discarded, not given the oil rules), and the ledger counts
them as "In Control's". Landed 2026-10-20: the engine tunable `adopt_natural_dragons` (default false)
leaves them alone; true restores R18's discard (R18a).

## 4. The design

### 4.1 The oil field index

- **Oil chunks:** a chunk with base oil above zero. Stored as a bit per chunk in region bitmaps, plus the
  amount class (finite or bottomless).
- **Where the bits come from (OG-2):**
  - **Deep Time levels:** every trap from `LevelSites` at world load, with the chunks it covers
    (`SiteChunks.covering`). Exact and complete, before anything is generated.
  - **Other levels:** CDG's own `getBaseOilAmount` for each chunk when it first generates, on the
    server thread under a per-tick budget (the 12,800 biome lookups are cheap on a generated chunk,
    which holds its biomes; their cost is UNVERIFIED until the bench). Lairs also ask for the chunks
    within their reach (§4.4), sampled ahead of generation under the same budget.
- **A field** is a connected group (8-neighbour) of oil chunks. Its record: the canonical chunk set
  (TP4), its centre, finite oil known (from deposit samples once drilled), bottomless or not, and which
  dragons drink there.
- **"Near a field"** means within M blocks of one of its chunks, M PLACEHOLDER 100 (In Control's
  `maxdist`), through the topology (TP3, TP8).

### 4.2 Field flights: the danger floor

The gate must stay at least as dangerous as In Control where the oil is. A finite population of
dragons cannot match a spawner that adds one every 10 seconds, so the sim runs the same arrival process,
keyed to fields instead of the desert biome:

- **Each second, per dimension,** with chance 0.1 (In Control's `persecond`, PLACEHOLDER until the
  measurement), pick one random player of the dimension. If that player is near a field and at or above
  y 15 (In Control's `minheight`), draw one fire dragon.
- **The drawn dragon** is a sim dragon with Ice and Fire's own age roll (1 + nextInt(80) days), its own
  colour and sex rolls, and a full oil reserve. It arrives as a flight: it materializes at the edge of the
  player's entity-ticking area (PLAN.md §10.3) heading in, as R18's respawn draws do ("from 3,000 blocks
  away, as a flight").
- **Caps:** 200 drawn dragons alive per dimension (In Control's `maxthis`), and a per-field cap
  (TUNABLE, PLACEHOLDER 8).
- **What they become:** visitors. They drink at the field (§4.3), hunt like any dragon, and when they
  get bored (C24) they leave: a visitor carrying nothing leaves the sim; one carrying valuables claims a
  vacant lair within reach or stays a wanderer (R18's existing kind).
- **Arrival is later than In Control's** (its dragon appears 25 to 100 blocks away; a flight comes from
  the edge of view). §4.8 measures time to first contact for that reason.
- **Effect:** in a desert with oil, about the same stream as today. In a desert without oil, nothing
  (danger follows deposits). Over a Deep Time trap in a forest, the same stream as a desert has today.

### 4.3 Dragons need oil

- **The reserve:** two saved fields on `Dragon`, `oil` and `oil_at`, like `fullness` and `fed_at`. It
  falls by one point every `oil_tick_rate` ticks (TUNABLE, PLACEHOLDER 3,000, the pack's
  `hungerTickRate`, so oil and food keep one rhythm) and by one point per breath (below). It is a closed
  form: `oil_at(t) = max(0, oil - (t - oil_at) / oil_tick_rate - breaths since)`. Capacity TUNABLE,
  PLACEHOLDER 100.
- **A breath while embodied:** the plugin counts `DragonFireEvent`s for each sim dragon. Each event
  costs one point; at zero the event is cancelled, so the dragon fights with bite and claws only. How
  many events one fire stream posts is UNVERIFIED; a GameTest counts them and sets the capacity so a
  full reserve lasts a stated number of fights (TUNABLE, PLACEHOLDER 3).
- **A breath while abstract:** each raid pass on a district (`Threat` count) costs one point per pass.
  A raid with no oil is resolved with the dragon's strength reduced (TUNABLE share, PLACEHOLDER half),
  and burns nothing (its theft and destroyed blocks are the melee share, I23).
- **Refuelling:** a new purpose, `Refuel(field)`. It weighs like hunger: its utility rises as the reserve
  falls, minus the flight's distance cost (`distance_cost_per_kblock`). The dragon flies to its field,
  rests on an oil chunk for a dwell (TUNABLE, PLACEHOLDER 1 hour of game time) and leaves full. Watched,
  that is a fire dragon sitting on the sand over the oil: the sight that tells a player where oil is.
- **Or from your tanks (OG-5):** a district near a field, or anywhere on the dragon's way, holding crude
  oil, diesel or gasoline in its boundary goods is also a refuel target. The raid's theft takes oil up to
  the reserve's room (1 point per 1,000 mB, PLACEHOLDER) before anything else. Oil stock is what draws a
  dry dragon to a base.
- **Oil in the ground is not drawn down (OG-6):** a dragon resting on a field takes nothing from CDG's
  chunk amounts. Oil in the ground is in nobody's books, so conservation is untouched.

### 4.4 Lairs near oil

- **Choosing a lair:** every lair claim (a young dragon leaving its parents, a respawn draw, a starved
  dragon migrating, a visitor settling) already picks among roosts. The choice now subtracts the
  distance from the lair to its nearest field, at the same `distance_cost_per_kblock`. A lair with no
  field within its stage's reach is chosen only when nothing else is free.
- **Moving closer:** a dragon whose refuel legs cost more than a threshold (TUNABLE) migrates to a vacant
  lair nearer oil when one exists.
- **Today's pack already helps:** fire caves and roosts generate in the fire biome tag, which includes
  desert; the cave and roost sets are spread 18 and 15 chunks apart (separation 12 and 6), and Structurify
  does not disable them. So deserts, where all the oil is now, have lairs.
- **Where a field has no lair (OG-7):** under Deep Time, a trap in a temperate forest may have none
  within reach. The plugin then places one `iceandfire:fire_dragon_roost` near the field through
  vanilla's `/place structure` code path (every biome allowed), on the first load of a free site within
  M of the field, at most one per field (PLACEHOLDER). Whether Ice and Fire's roost start generates
  outside world generation, and whether it spawns its own dragon there, is UNVERIFIED; a GameTest decides.
  The roost's dragon, if it has one, is adopted (R18).
- **Nests:** M1's breeding happens at the lair, so the young of oil-field pairs hatch near oil and, by
  the lair rule above, settle near it.

### 4.5 Sensing industry near oil

Dragons that drink at a field notice what changes there. Each signal is an event the industry engine
already produces; each becomes an `Encroachment` report (rtsim-knowledge) witnessed by the field's
drinking dragons, against the district's players (those who placed its machines: I2 detects them with
`BlockEvent.EntityPlaceEvent`, which names the player).

| Signal | Source | Severity (PLACEHOLDER) |
|---|---|---|
| A district forms, or grows, near a field | the I2 census | 1 unit, once per district per day |
| A pumpjack hole in a district samples the field | `IND_DEPOSIT_SAMPLE` plugin 2 | 2 units, once per district per day |
| Crude oil, diesel or gasoline stock in the district | its boundary goods | 1 unit per day while above one tank (1,000 mB) |
| Pollution attributed to the district | Destroy's `changePollution` by position (I14) | 1 unit per day while above zero |

- **What a report does:** sentiment falls (rtsim-knowledge's scale), and below `grudge_below` the dragon
  holds a grudge (existing). Two targets follow:
  - **The works:** a new `Target::District`, the district itself, raided through `Infrastructure::raid`
    (class Flying, source Dragon). It weighs like a grudge, under the same cooldown.
  - **The player:** the existing grudge raid at the player's home (P26), unchanged.
- **Watched,** the dragon flies to the district and materializes there; the fight is real and P28's
  bounded damage applies.
- **Unwatched,** the raid resolves abstractly: Lanchester against the defence vector, theft first of oil
  (§4.3), destroyed blocks from the exposed set, roofless first, capped (I23). Turrets count (I21).
- **Roofs:** INDUSTRY.md §13.6 says roofs shield machines from flyers. `threat.rs` picks roofless
  blocks first when it chooses what a flyer destroys, but `resolve()` does not gate a flying attack by
  roof cover, as walls gate ground attackers. OG-11 asks to build the gate as designed. With it, a well built under a roof,
  or underground, is safe from dragons' blocks; its oil stock and its players are not.
- **Beginning, not only drilling:** the first row fires before any oil is pumped, which is what Ben
  asked for ("when players are beginning to do industrial activity near an oil deposit").

### 4.6 Underground: the deep listens

The cheese is reaching oil below y 15, where In Control spawns nothing and dragons do not fit. The
threat there is vanilla's own, placed where the oil is:

- **What is placed:** on the first load of each oil chunk (generated or not before), the plugin places
  one placed feature, `mic_rtsim:oil_field_sculk`, a data file in the mic_rtsim datapack: the vanilla
  configured feature `minecraft:sculk_patch_ancient_city` (10 charges of 32, 1 to 3 extra shriekers,
  catalyst chance 0.5) with vanilla placement modifiers: a count (PLACEHOLDER 2 per chunk), spread in the
  chunk, height uniform from the bottom of the world to y 15, an environment scan down to a solid floor.
  The plugin calls the same `place` that vanilla's `/place feature` calls, seeded from the world seed and
  the chunk. Once per chunk, ever.
- **Why it works:** a world-generation spreader places shriekers that can summon (`CAN_SUMMON` true for
  the patch's extra growths, and for 1 in 11 of its spread growths). Sensors and shriekers hear
  vibrations within 8 blocks; a vibration a player causes, heard by a shrieker, warns that player
  (`WardenSpawnTracker.tryWarn`): at most one warning every 200 ticks, one warning forgotten every
  12,000 ticks, and at the fourth a warden is summoned within 5 blocks of the shrieker (20 tries),
  unless one is already within 24 blocks. It emerges from the ground (`MobSpawnType.TRIGGERED`).
- **Why it gates:** a warden has 500 health and 30 attack, and its sonic boom does 10 through walls
  within 15 blocks across and 20 up or down, so a one-wide tunnel does not hide anyone. It digs back down
  after 1,200 ticks with nothing to chase. Every well's pipe reaches bedrock (§3.1), so every well, from
  above or below, passes through the layer.
- **Counterplay is vanilla's:** sneaking, wool (which blocks vibrations), breaking shriekers, killing
  wardens. A player who clears a field's sculk has beaten that part of the gate, honestly.
- **Machines are not its targets:** wardens break no blocks, watched or not, so the unwatched sim adds
  nothing here. Natural mob pressure underground (I20) applies as it does anywhere dark.
- **The pack does not change it:** no pack file mentions `doWardenSpawning` or wardens; the gamerule is
  vanilla's default (on). L2 Hostility levels apply to wardens as to any mob.
- **What else was surveyed and why not:**
  - Myrmex: not in Ice and Fire CE (no classes or data in the jar; CE's own docs say so).
  - Death worms (`iceandfire:deathworm`): world generation only, vanilla `minecraft:desert` only (not
    Terralith's), beyond 1,000 blocks from spawn, chance 0.0333; the pack gives them 10 health and 3
    attack, and they swim only through sand (`BlockTags.SAND`). Too weak, and they cannot follow into
    stone.
  - Born in Chaos: spawns in the dark everywhere in the overworld, gated by gamerules and light; not
    tied to anything, and C27 (its balance) is deferred.
  - Mowzie's grottol: below y 16, but not a threat (hostility UNVERIFIED). Cataclysm's koboleton: desert
    surface and desert structures. Mutant Monsters: rare variants where the vanilla mob spawns. Bosses
    of Mass Destruction: structures only. Darkness (True Darkness): client-side lighting only.
  - Alex's Caves: excluded by Ben (P8).

### 4.7 What "guns" means in this pack

- `tacz:gun_smith_table` is made by Create mechanical crafting in the pack (`tacz_workbenches.js`): 6
  steel plates, 2 steel blocks, 2 anvils, 1 precision mechanism, 1 brass plate, 4 planks.
- `tacz_new_recipes.js` defines 54 gun recipes. The earliest need one precision mechanism (`taurus943`,
  `hk_mk23`, `springfield1873`), and all need a Create Big Cannons autocannon barrel and breech.
- **UNVERIFIED and important:** pack/modDocs/gun-crafting-plan.md says these recipes failed to load under
  TaCZ r2; whether they load on r6 is not checked. If they do not, the gate has nothing behind it but the
  spawn bonus chest's Model 943 (only when that world option is on). Whether any gun input itself needs
  oil is also UNVERIFIED. Both are checked before the gate's tuning is trusted.

### 4.8 Measuring against In Control, and the switch-over

"At least as dangerous" is measured, not argued.
- **The probe:** a mock player (the GameTest harness's `makeMockServerPlayerInLevel`, which In Control's
  `level.players()` sees) held at a site, invulnerable, healed each tick; a `LivingIncomingDamageEvent`
  tally of damage and attackers. Passive on purpose: it measures exposure, the same for both arms.
- **The sites:** (1) desert surface over oil; (2) desert over oil, below y 15; (3) desert without oil;
  (4) a Deep Time trap outside a desert (once Deep Time is in the pack); each by day and by night, alone
  and with 3 other probes in the dimension.
- **The arms:** A: the pack as shipped (In Control on), sim dragons off or OG-1 on. B: a test copy of
  `spawn.json` without the fire dragon rule, the sim gate on. The pack's own file is not edited.
- **The measures:** damage per minute, time to first hostile contact, distinct dragons per hour.
- **The gate passes** when B is at least A at sites 1 and 2 on every measure (medians over the runs,
  count TUNABLE), and B is above zero at site 4. Site 3 is expected to fall: danger follows deposits.
- **Then, and only then,** Ben removes the fire dragon rule from In Control's two files and OG-1's
  exemption ends. The blaze rule is a separate choice (OG-12).
- **Measured** 2026-10-21 (sites 1 to 3, two runs by day, `OilDangerGameTests` with `-Pog10`): not yet
  passed. The table and the harness are OGC35 in `docs/choices-oilgate.md`.

## 5. Walkthroughs

**A. Today's pack, a desert with oil.**
1. Mara scans the dunes east of Brindle with the oil scanner: "high".
2. She stands there. Each second the field flight rolls; within the first minute a stage-2 fire dragon
   flies in from the edge of view, then another. She runs, as she would from In Control's dragons today.
3. Cinderjaw, an adult whose roost is 1,100 blocks away, drinks at this field: its reserve fell below
   its refuel level yesterday. Players who visit see it resting on the sand.
4. A week later Mara returns with a Model 943 and builds a small base 150 blocks from the field: a
   district forms. Cinderjaw gets one `Encroachment` report.
5. She sinks a pumpjack hole and pipes down. At y -40 her shaft breaks into a cave grown over with sculk.
   A sensor clicks, a shrieker shrieks: first warning. She keeps digging; at the fourth a warden rises
   out of the floor. She retreats up the shaft, sonic booms hitting her through the rock.
6. She comes back at night with wool, sneaks, breaks the shriekers by her shaft and finishes the well.
   The district's deposit sample arrives: two more units. Crude fills a tank: one more a day.
7. Cinderjaw's sentiment is below `grudge_below` now. She logs off. Unwatched, Cinderjaw raids the
   district: its pumpjack is roofless, its turret untested. Lanchester; the tank loses 4,000 mB to the
   dragon's reserve, and two exposed blocks are booked destroyed, applied when she next loads it.
8. Mara roofs the well and adds a second turret. The next raid takes oil from the tank but breaks no
   blocks under the roof; the turret, seen in a real fight since, counts at full weight.

**B. Deep Time, a trap under a forest.**
1. The world's sites list a petroleum trap under a temperate forest 3,000 blocks from any desert. In
   Control's desert rule never reached it.
2. The field index has it at world load. No fire lair lies within reach, so on the first load of a free
   site near it the plugin places a fire dragon roost; its dragon is adopted.
3. A player who stands on the trap meets the field flights, as a desert player does today.
4. The deep layer under the trap has its sculk. The gate is the same as in a desert.

## 6. Cost model

| Mechanic | When it costs | Cost |
|---|---|---|
| Field index, Deep Time | world load | O(traps), plus their chunks once |
| Field index, CDG | per chunk generated | one `getBaseOilAmount` (12,800 biome lookups on a generated chunk, time UNVERIFIED), under a per-tick budget; 1 bit stored per chunk |
| Field flights | per dimension per second, only while a player is near a field | one roll, one random player, one field lookup; a draw is one dragon and one flight leg |
| Oil reserve | per read | O(1) closed form; zero per tick |
| Breath | per `DragonFireEvent` of a sim dragon | one map lookup |
| Refuel | per refuel | one flight leg booked; no per-tick work |
| Lair weighting | per lair claim | O(lairs considered), with one field lookup each |
| Lair placement | per field without a lair | once, one structure placement |
| Sensing | per district event (formation, sample, daily stock or pollution check) | one field lookup (`fold_rect`), one report |
| Raids | per raid | as INDUSTRY.md §15 |
| Sculk layer | per oil chunk | one feature placement on first load, under the block quota; then vanilla's own cost, which is zero while nobody is near |
| Nobody near any field, nothing changing | | zero |

## 7. Tests before it ships

1. `DragonFireEvent` count per fire stream and per charge shot, for the reserve's capacity.
2. `/place structure iceandfire:fire_dragon_roost` outside world generation: does it place, and does it
   spawn a dragon.
3. The placed feature at y below 15 in a desert chunk: shriekers that can summon, a warden on the fourth
   warning.
4. CDG's `getBaseOilAmount` time on generated and ungenerated chunks (bench).
5. TaCZ r6: do the pack's gun recipes load.
6. The measurement of §4.8.

## 8. Proposals for Ben

| Id | Question | Recommended | Alternatives |
|---|---|---|---|
| OG-1 | In Control's dragons while the gate is unproven (R18 discards them now) | the plugin leaves natural fire dragons alone until the gate passes | (b) adopt them as sim wanderers, so they get the oil rules; (c) keep discarding them (the desert gate is gone while the sim runs) |
| OG-2 | How the sim learns where oil is | Deep Time's sites at world load; elsewhere CDG's own amount per chunk as it generates, plus a budgeted look ahead around lairs | (b) only chunks already generated; (c) only drilled chunks (deposit samples, today) |
| OG-3 | How danger reaches In Control's level over oil | field flights: In Control's arrival process keyed to fields, drawing sim dragons as flights | (b) the dragon population only, with no draws (fewer arrivals; likely fails §4.8); (c) keep In Control and re-key its rule to fields through its own `area` or `kubejs` conditions (both exist in 10.3.0; how they behave is UNVERIFIED) |
| OG-4 | How the oil reserve drains | with time at the hunger rate, and one point per breath; no oil, no fire | (b) per breath only (an idle dragon never needs oil); (c) with time only |
| OG-5 | Where dragons refuel | resting on a field, and from players' oil stocks by theft | (b) fields only; (c) stocks only |
| OG-6 | Do dragons draw down oil in the ground | no: oil in the ground is in nobody's books | (b) yes, finite chunks lose a share through `IND_DEPOSIT_DELTA` |
| OG-7 | Lairs near oil | lair choice weighs distance to oil, and one fire roost is placed near a field that has none within reach | (b) weighing only (Deep Time fields far from fire biomes stay empty); (c) placement only |
| OG-8 | What dragons sense and what they do | district formation, drilling samples, oil stock and pollution near a field become reports; grudges send raids on the works and at the player's home | (b) drilling samples only (nothing until oil flows); (c) presence only, no industry signal |
| OG-9 | The underground threat | vanilla sculk with summoning shriekers below y 15 in every oil chunk; wardens | (b) the sim warns and summons wardens itself from block events (invisible, no counterplay); (c) death worms with raised health (sand only); (d) natural pressure only (I20) |
| OG-10 | How "at least as dangerous" is measured | passive mock-player probes at four sites, A/B, damage per minute and time to first contact | (b) arithmetic from the two arrival rates; (c) play-testing by Ben |
| OG-11 | Roof cover against flyers | build INDUSTRY.md §13.6's roof gate in `resolve()` | (b) leave it out: roofed and underground districts take flying raids' block damage |
| OG-12 | In Control's desert blazes when the dragon rule goes | keep the blaze rule as it is (desert-keyed, untouched) | (b) re-key it to fields with OG-3 (c); (c) remove it |
| OG-13 | A dragon with no oil | fights with bite and claws only, at a reduced strength while abstract | (b) it breaks off and flies to refuel; (c) it still breathes, weakly |
