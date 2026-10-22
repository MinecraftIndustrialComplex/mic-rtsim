# The oil gate: a cheese audit

**Status:** audit and plan only, 2026-10-21, of the gate as built on main at 02c2096 (OILGATE.md,
docs/choices-oilgate.md). None of the fixes here is built and no test was written for them. Neither
mic-rtsim nor Deep Time is in `pack/mods` yet, so this reads the gate as it will be once both ship, with
OILGATE.md's defaults and Ben's answers of 2026-10-21 (DECISIONS.md OGB-1 to OGB-5: reserve 600, refuel
below 240, both built with this audit; drinking free of the theft cap).

**Ben's bar** (2026-10-21, in place of For Ben 1's options): "As long as it is inpheasible for a player
to build out an actual oil pipeline & difficult for a semi-casual player to cheese. (Probably need to
do some checks to make sure players can't do bullshit like run in, place a minecart contraption ->
turn it into a real world object, and run away while placing pipes along the way or some shit.)"

So the gate passes when both hold:
1. **No standing oil works before guns.** A well, a pipeline or a link home that keeps running is
   found, hurt and stopped, whoever or whatever built it.
2. **No cheap grab.** The obvious tricks a semi-casual player tries (§2) cost real danger or real
   losses.

**In short.**
- **The gate does not meet the bar today.** Presence danger cannot: neither In Control nor field
  flights breaks a block or drinks a tank. The infrastructure half, the only part that acts on a
  standing well, has three gaps:
  - It never sees a well that no real player placed by hand. Ben's minecart trick works as he feared
    (A1), and so do a Schematicannon, a Deployer and a ship (A2 to A4).
  - It senses a well only near the spot where its dragons happen to drink, and only when they run
    low. A well elsewhere on the field, or one linked home, goes unseen (B1, B2). A new well waits up
    to 45 days with the new reserve (B4).
  - It cannot hurt a well that is roofed, underground, guarded or partly loaded (C1 to C5).
- **In Control's fire dragon is no longer needed** once the first three fixes land (§3). Recommended:
  field flights on, In Control's fire dragon off, blazes kept. Ben decides.
- **The smallest fix set (§4):**
  - F1: a pumpjack seeds its own district, whoever placed it.
  - F2: the field watches its own ground, instead of waiting for refuel visits.
  - F3: sense by chunks, not by the district's centre.
  - F4: a roof delays a flyer instead of stopping it (Ben).
  - F5: survey what is loaded.
  - F6: build OG-9e's deep layer.
  - F9: watched raids do damage too, as P28 decided.
  - After those: F7 (name the builders) and PF1 (ethanol, Ben).
- **None of it is built.** Not one row of §2 has an integration test; §4 lists the unit tests each fix
  would bring.

## 1. How the gate sees a well, link by link

Every cheese in §2 breaks one of these links. The numbers are the defaults (OilTunables in
`crates/rtsim-ecology/src/oil.rs:65-143`) with Ben's reserve.

| Link | What has to happen | Where |
|---|---|---|
| L1 Seen | A district exists. A district starts only when a real player places a seed block: `onPlace` drops any placer that is not a `Player` or is a fake player, and `playerPlaced` records the seed. A seed whose block entity is gone is dropped at the next detection. The only other way in is the operator's `/rtsim` adopt. | `IndustryEvents.java:110-111`, `Districts.java:249-262`, `:913-925`, `:296-322` |
| L2 Known as a well | `IND_DEPOSIT_SAMPLE` for a loaded district chunk that holds a Diesel Generators pumpjack hole. | `neoforge/dieselgenerators` `DieselOilPlugin` |
| L3 Sensed | Only when a dragon arrives to drink: `refuel_arrive` calls `sense`. A dragon flies to drink only when its reserve is at or below 240, to the oil chunk nearest wherever it is then. It senses each district whose centre (the mean of its chunk centres) lies within `sense_reach` 400 of that chunk, once a day each. The centre index is rebuilt only when the number of districts changes. | `oil.rs:837-867`, `:873-885`, `:990-1072`; `infra.rs:157-185` |
| L4 Grudged | Severity is 100 × (1 + 2 for an oil sample + 1 for 1,000 mB of fuel + 1 for pollution), at most 255. The sentiment change is -25 × severity / 100, and a grudge is -40 or lower. So a bare district (-25) is no grudge; fuel alone (-50) or a drilling well (-63) is. Players are named only when their P26 home lies within 512 blocks of the district. | `rtsim-knowledge/src/tunables.rs:75`, `:113`; `rtsim-ecology/src/tunables.rs:177`; `engine.rs:95` |
| L5 Raided | A grudged district is a raid target within `grudge_range` 6,000 blocks: at most once every 3 days per dragon, never while it starves, up to 2 districts a raid. Unwatched, the raid is resolved by the industry engine; watched, the dragon appears for real. | `species.rs:148`; `rtsim-ecology/src/tunables.rs:179-180`; `engine.rs:1740-1800` |
| L6 Hurt | Theft: a dragon with room drinks fuel stock, free of the 10% cap (OGB-3), up to its room at 1,000 mB a point. Blocks: one per surviving pass (a fire dragon has 1 to 4 passes by stage), at most 8. A flyer breaks only roofless blocks, scaled by the unroofed share of machines, and only blocks in the last survey. A survey runs only when every chunk of the district is loaded. | `rtsim-industry/src/threat.rs:355-363`, `:462-545`; `rtsim-industry/src/tunables.rs:180-184`; `species.rs:159-160`; `Threats.java:441` |
| L7 Danger to the player | In Control's desert dragons: desert only, y 15 to 200 (OILGATE.md §3.2). Field flights are off (`fieldFlights` false), and when on they roll only for players at y 15 or higher. Lair dragons, and watched raids. | `oil.rs:534-571`, `:642-694` |

Two numbers matter most:
- **Time to the first sense.** A dragon at rest loses a point every 3,000 ticks, so it drinks every
  (600 - 240) × 3,000 = 1,080,000 ticks: 45 game days, about 15 hours of play. That is the longest wait
  between a well starting and its first sense, per dragon. Breath and raids drain it faster: a stream
  of fire is 35 to 41 points (choices For Ben 2). Before Ben's answer (100 and 40) it was 7.5 days.
- **How long a grudge lasts.** Sentiment fades one step at a time, at a mean of 2,400 × (1 + 99 ×
  (m / 127)²) ticks a step at magnitude m (`rtsim-knowledge/src/tunables.rs:24-28`, `:88-89`). From -63
  back up past -40 is 24 steps, about 1,010,000 ticks or 42 days on average. From -50 it is 11 steps,
  about 356,000 ticks or 15 days. So a drilling well, once sensed, stays a target for about as long as
  a resting dragon takes to come back and sense it again; a district with only fuel in its tanks
  drops off the list for most of that time.

## 2. The cheeses

Twenty-six things a semi-casual player in this pack would try for oil before guns, each with the
real mechanic, what the gate does today, and how likely such a player is to think of it. **Open**:
the gate does nothing. **Slowed**: it acts, but late or weakly. **Stopped**: the trick fails. Pack
facts were read in the jars of the Prism instance "Minecraft Industiral Complex-2.1.3" and in `pack/`
(config, defaultconfigs, kubejs); nothing was run in a game.

**Pack facts every row leans on.**
- **Oil.** The only crude source is the Diesel Generators pumpjack on an oil chunk. The pack makes
  `oil_biomes` `#c:is_desert` and sets the normal chunk multiplier to 0.0 (high chunks 2.0), so only
  high-oil desert chunks hold oil (`pack/kubejs/data/createdieselgenerators/tags/worldgen/biome/oil_biomes.json`,
  `pack/defaultconfigs/createdieselgenerators-server.toml`). Deep Time replaces that with petroleum
  traps anywhere.
- **The pumpjack.** It pumps `min(oil, 1000 × head/bearing distance ratio)` mB a crank turn, doubled
  with a large crank, into the hole's 8,000 mB tank. It runs only while its chunk is loaded. In an
  rtsim world a district's boundary flows are also replayed while it is unloaded and reconciled into
  its tanks on load (INDUSTRY.md C19), so an unwatched well keeps paying.

### A. The well is never seen (L1)

| # | The cheese | Today | Why | Likely |
|---|---|---|---|---|
| A1 | **Ben's minecart trick.** Build the pumpjack and a tank on a minecart or train contraption at home, ride it to the oil, disassemble it there, hand-place the pipe down to bedrock and leave. | **Open** | Disassembly is `Contraption.addBlocksToWorld`, a plain `setBlock` with no place event, so no seed and no district: no sample, no sense, no raid. Hand-placed pipes are members, never seeds (`CreatePlugin.seed`). Whether a pumpjack survives assembly is UNVERIFIED; no movement blacklist was found in Diesel Generators. | High: it is the first idea of a Create player who has heard about the gate. |
| A2 | **Print it.** A Schematicannon builds the well from a schematic. | **Open** | `LaunchedItem.place` calls `BlockHelper.placeSchematicBlock`: no event. | Medium: the cannon is mid-Create and needs gunpowder. |
| A3 | **Deploy it.** A Deployer, or a deployer on a contraption, places the hole. | **Open** | Deployers post the event with a `DeployerFakePlayer`, which `onPlace` drops (`IndustryEvents.java:111`). | Low to medium. |
| A4 | **Fly it in.** Build the well on a Sable ship, fly it out, disassemble it over the oil. | **Open** | Sable and Simulated move blocks with `chunk.setBlockState`, no event. Tanks keep their fluid across assembly. While aboard, ship blocks live in plot chunks near chunk (10,000, 10,000) that the district scan never reads. | Medium: Aeronautics is a headline mod, and crude is a Propulsion thruster fuel (`pack/kubejs/data/createpropulsion/thruster_fuels/destroy_crude_oil.json`). |
| A5 | **Carry it in.** Pick up a working machine or a full tank with Carry On and set it down at the oil. | **Stopped** | Carry On forbids `create:*` and `create*:*` (Diesel Generators included) by default, and the pack ships no Carry On config. Where it does place, it posts the place event with the real player, so the seed is recorded. | Medium (and it fails). |

### B. The well is seen but not sensed (L3)

| # | The cheese | Today | Why | Likely |
|---|---|---|---|---|
| B1 | **Drill away from the dragons' spot.** Put the well on the end of the field nearest home, not where the dragons land. | **Open** | A low dragon drinks at the oil chunk nearest wherever it is, usually by its lair, and senses only districts whose centre is within 400 blocks of that chunk (`oil.rs:837-867`, `:990-1001`). The rest of the field is never sensed. | High, often by accident. |
| B2 | **Link it to the base.** Join the well to the base by pipe, belt, shaft, or an Ender Transmission fluid link on one channel. | **Open** | Joined chunks are one district whose centre is the mean of all its chunks, so a big base pulls it far from the field (`infra.rs:157-185`). The index is rebuilt only when the number of districts changes, so a district that grows keeps its old centre (`infra.rs:158-161`). Ender Transmission has no range, no power cost and no dimension limit in the jar; two transmitters cost 14 obsidian, 6 ender pearls, 2 precision mechanisms, an eye and 2 pumps each. | High: it is the obvious way to bring oil home. |
| B3 | **Link it across dimensions.** The same link with the base in the Nether or another level. | **Stopped** | rtsim cannot join a channel across levels (`ChannelLinks`), so the well stays its own small district by the oil and is sensed as one. What crosses the link is not simulated, which is a sim gap, not a hole in the gate. | Low. |
| B4 | **Be quick.** Drill, fill tanks for a week or two, pack up before the dragons notice. | **Slowed**, barely | The first sense waits for a dragon's refuel visit: up to 45 days a resting dragon with Ben's reserve (§1). A short-lived well is likely never sensed. | High: it is the natural plan for a first grab. |
| B5 | **Deal with the field's dragon.** Lure it away, or kill it in its sleep. | **Slowed** | Fire dragons sleep when it is not day and wake when a player comes within about three times their size (`IafDragonLogic`). Killing one takes weapons nobody has early: every gun needs Create Big Cannons steel parts (`pack/kubejs/server_scripts/tacz_new_recipes.js`). An emptied roost refills after 20 days from 3,000 blocks (`rtsim-ecology/src/tunables.rs:185-187`). | Low early. |

### C. The well is sensed but not hurt (L6)

| # | The cheese | Today | Why | Likely |
|---|---|---|---|---|
| C1 | **Roof it.** Build a shed over the pumpjack. | **Open** for blocks | A flyer breaks only roofless blocks, fewer as more machines are roofed (`threat.rs:355-363`, `:520-537`): OG-11 as approved. A fully roofed well loses nothing. Only theft reaches it. | High once a pumpjack has burned once. |
| C2 | **Keep nothing there.** Pump straight into the link or a pipe home. | **Open** with C1 | Theft at the field drinks only the fuel stock of the districts it senses (`oil.rs:886-897`). With C1 and an empty tank a raid takes nothing and breaks nothing. | Medium. |
| C3 | **Be bigger than the loaded area.** A long pipeline, or a well joined to a far base (B2) while the chunks between stay unloaded. | **Open** | A survey runs only when every chunk of the district is loaded (`Threats.java:441`). With no survey there is no exposed block set, so a raid breaks nothing (`threat.rs:528`). | Medium; it comes free with B2. |
| C4 | **Go underground.** Reach the oil from a cave or shaft below y 15. | **Open** | Machines underground are roofed (C1). In Control spawns nothing below y 15 (OILGATE.md §3.2), field flights roll only at y 15 or higher (`oil.rs:539-541`), and the deep layer (OG-9e) is not built. | High: players mine anyway, and the pipe goes to bedrock. |
| C5 | **Stand guard.** Stay at the well; when a dragon comes, wait it out. | **Open** for blocks | A raid on a watched district brings the dragon for real, and a watched dragon breaks no blocks (`infra.rs:6-8`, R19). Ice and Fire griefing is off, which also stops its `digWhenStuck` (`pack/config/iceandfire/iaf-common.json`). This contradicts P28 as Ben decided it ("Buildings take bounded sim damage, watched or not", PLAN.md §9). | Medium. |
| C6 | **Live far away.** Keep the home base more than 512 blocks from the well. | **Open** for the player | `sense` names only players whose P26 home is within 512 blocks of the district (`oil.rs:1024-1037`, `engine.rs:95`). The well is still raided; its builder is never hunted. | High, by geography. |

### D. Oil or fuel without a well

| # | The cheese | Today | Why | Likely |
|---|---|---|---|---|
| D1 | **Find crude elsewhere:** loot, trades, Ore Excavation veins, Destroy's pumpjack. | **Stopped** | No loot table, trade or feature grants crude. Ore Excavation has only a water vein. Destroy's pumpjack and seismometer are voided (`voided_content.js`), and `oilyunification.js` removes the distillation routes. | Medium (and it fails). |
| D2 | **Burn ethanol instead.** Ferment sugar, apples, potatoes or sugar cane with bone meal in a lidded basin (200 mB a batch), or buy moonshine from Destroy's innkeeper and distil it. | **Open, on purpose** (OGB-8): ethanol is the pre-oil fuel, rebalanced in the pack to its real properties so it is poor in diesel engines | `createdieselgenerators:basin_fermenting/fermentable` probably survives the pack's removal: `oilyunification.js` removes recipes by fluid output, which KubeJS 7 matches only on recipe types it has a schema for, and Diesel Generators registers none (UNVERIFIED until a server log shows it). Ethanol is a Diesel Generators engine fuel, and its Destroy distillate a Propulsion thruster fuel. Biodiesel (plant oil plus ethanol) is a Create mixing recipe and probably is removed. | High: JEI shows it. |

### E. Taking the oil home

These carry oil the gate never sees, by design: the gate acts on the well. Each is harmless once the
well is seen and hurt (§4), and open while it is not.

| # | The cheese | Today | Why | Likely |
|---|---|---|---|---|
| E1 | **Backpack tank.** A leather backpack with a tank upgrade holds 12,000 mB at copper-tier Create, more with stack upgrades (an iron one with a first stack upgrade, 48,000 mB). | **Open** | `TankUpgradeItem.getTankCapacity`, `pack/config/sophisticatedbackpacks-server.toml` (4,000 mB a row), `pack/kubejs/server_scripts/sophisticated_backpacks.js`. | High. |
| E2 | **Canisters.** Diesel Generators canisters hold 4,000 mB each (1,000 more per Capacity level), fill from a spout, cost 2 andesite alloy, a barrel and 3 iron plates. | **Open** | `CanisterBlockItem`; "Canister can be filled by spouts" true in the pack's server config. | High. |
| E3 | **Tanker cart or train.** Tanks on a contraption keep their fluid; Create Connected vessels hold 8,000 mB a block. | **Slowed** for scheduled trains, **open** for ridden carts | A hungry dragon whose flight crosses a booked train leg raids it (I24, `engine.rs:1346-1465`); a ridden cart is not booked. A broken Create tank drops empty, so tanks must travel whole. | Medium. |
| E4 | **Tanker ship.** Tanks aboard a Sable ship, with Propulsion platinum tanks at 16,000 mB a block; the ship burns crude. | **Open** | Ship blocks live in plot chunks, outside every district. | Medium. |

### F. The player's own danger at the oil (L7)

| # | The cheese | Today | Why | Likely |
|---|---|---|---|---|
| F1 | **Oil outside a desert.** On a Deep Time world, use a trap under a forest. | **Open** | In Control's fire dragon is allowed only in `c:is_desert` (`pack/config/incontrol/spawn.json`); field flights are off. Lairs still form near the field (OG-7). | Certain on Deep Time worlds. |
| F2 | **Go at night.** | UNVERIFIED | Fire dragons sleep at night and wake when a player comes close (`IafDragonLogic`). Whether In Control's or the sim's dragons doze off at night near a player is not measured; OGC35 measured by day only, though §4.8 asks for night runs. | Medium: players notice sleeping dragons. |
| F3 | **Fly in and out.** On a Cobblemon flyer: Skarmory spawns in deserts and badlands by day, and the pack's only Poke Ball recipe (`create_encapsulated:poke_balls/poke_ball`) is cheap. Jetpacks and elytra are End-gated. | **Slowed** | A dragon targets any player within 128 blocks (`targetSearchLength`), mounted or not. Ride stamina is finite (`infiniteRideStamina` false). Whether a flyer outruns a fire dragon is UNVERIFIED. | Medium for Cobblemon players. |
| F4 | **Hide in a box.** A sealed hut at the well. | **Open** | With griefing off a wild dragon breaks no block and does not dig, so a closed room stops it (UNVERIFIED in play that no breath reaches inside). With C5 the well also stays whole while the player hides. | High: it is the first instinct when a dragon appears. |

**Open:** A1 to A4, B1, B2, C1 to C6, D2, E1, E2, E4, F1, F4. **Slowed:** B4 (barely), B5, E3, F3.
**Stopped:** A5, B3, D1. **Unknown:** F2.

The gate's real failures are the twelve open rows in A to C, with B4. E's rows close by themselves
once those do, and D2 is open on purpose: Ben made ethanol the pre-oil fuel (OGB-8), rebalanced in
the pack so it is poor in diesel engines.

## 3. Is In Control's fire dragon still needed?

What each source of danger does against Ben's two halves:

| Source | Bar 1: standing works | Bar 2: a quick grab | Where it misses |
|---|---|---|---|
| In Control's fire dragon (on today) | Nothing: Ice and Fire griefing is off, so it breaks no block and takes no oil (OILGATE.md §3.2). | Real danger at the oil, in deserts. | Below y 15; outside `c:is_desert`, where Deep Time puts most oil; anywhere a player cannot be reached (§2, F rows). Its dragons are not sim dragons (OG-1): they never drink, sense or hold a grudge. |
| Field flights (built, off) | Nothing, for the same reason. | The same arrival process keyed to oil chunks, wherever they are, at y 15 or higher (`oil.rs:534-571`, `:642-694`). Measured at site 1: the same damage per minute as In Control, first contact 40 to 50% later (OGC35). | Below y 15 (on purpose: the deep layer's job). |
| Lairs, sensing, grudges, the roof gate (on) | The only part that acts on a well, a pipeline or a link home. Today it fails here: see §2's open rows on L1, L3 and L6. | Slow: the first sense waits for a refuel visit (up to 45 days a dragon, §1). | Wells it never sees (L1), sees late (L3) or cannot hurt (L6). |

**Judgement.** Under Ben's bar, presence danger (In Control or field flights) does nothing for bar 1:
neither breaks a block or drinks a tank. Bar 1 rests entirely on the infrastructure half, which is
open today. Presence danger matters only for bar 2's window, the time between a player arriving at
the oil and the first dragon coming for the works. With F2 below, that window shrinks from weeks to
the field dragon's next decision: a dragon at rest decides every `rest_ticks` 12,000 ticks
(`rtsim-ecology/src/tunables.rs:170`). Field flights close what is left of it, from the first minute
and wherever the oil is. In Control does the same only in deserts, and only above y 15. So In
Control's fire dragon is not needed once F1 to F3 land, and matching it (OG-10's pass condition) is
no longer the test. The test is §2: every row stopped or slowed.

**Recommendation (needs Ben, it changes the pack's spawns):**
- **(a) Recommended.** When rtsim ships with F1 to F3: turn field flights on at the built draw
  (`fieldFlights` true, `flight_chance_milli` 100, `flight_cap_field` 8, both PLACEHOLDER), take the
  fire dragon out of In Control's `spawner.json` and both `spawn.json` rules (the files are ready in
  `neoforge/iceandfire/pack-proposal/`), keep the blazes (OG-12 (a)), and retire OG-10's "at least as
  dangerous as In Control" pass condition. Keep OGC35's harness for tuning the draw. Danger then
  follows deposits, which P13 asked for, from one system whose dragons drink, sense and remember.
- (b) Keep In Control as it is and field flights off (today). Desert oil keeps its danger; Deep Time
  oil outside deserts has no presence danger at all, and nothing changes for bar 1.
- (c) Neither: no field flights and no In Control dragon. Only lairs and watched raids bring dragons
  to the oil. With F2 a working well meets a real dragon within about half a day; a player who only
  scouts or grabs meets none. This is the cheapest, and it leaves bar 2's first half-day open.

Until F1 to F3 land, (b) is what the pack has, and switching off In Control first would only remove
danger.

## 4. Fixes: the smallest set

Rule D3 holds for every fix: no new entity, block or item. Each fix names the rows of §2 it closes.
"Ben" marks the ones that change something he approved or something players see in the pack; the
rest are reversible code choices inside rtsim, mine to make when the build starts.

**F1. A pumpjack seeds its own district, whoever placed it** (A1 to A4). The pumpjack hole is the only
way to pump crude (§2's pack facts, D1), so seeing every working hole is enough to see every well. The
dieselgenerators plugin, which already finds `PumpjackHoleBlockEntity` in district chunks for the
deposit sample, records any hole it finds in a loaded chunk as a seed, through a small core call
beside `playerPlaced` (record the seed, mark the chunk dirty). It finds holes on chunk load and from a
hook on the hole's own first server tick (a mixin on Diesel Generators' block entity, not a new
block), so a hole set down by a contraption, a ship, a Schematicannon or a Deployer is seen within a
tick. The rest of a dropped-off base joins the district by connection, as any machine does. Not Ben:
it is a pumpjack-only exception to DS-10. The wider forms would amend DS-10 itself and are Ben's:
any seed-shaped block entity that appears without a player within `near_blocks` of an oil chunk, or
any machine anywhere whoever placed it.
**Built** (docs/choices-oilgate.md OGC60), with its GameTest.

**F2. The field watches its own ground** (B1, B4; it also makes L3 independent of the reserve). Today
a district is sensed only when a dragon happens to drink within 400 blocks of it. Instead:
- at a district's first deposit sample (it is drilling), at once, and
- once a game day for every district that has a chunk within `sense_reach` of any chunk of a field,

a fire dragon of the nearest fire lair within `lair_reach` of that field senses it, with today's
severity and its once-a-day rule (`sense`, unchanged). No flight is needed: it is the same report a
visit would have filed. A field with no fire lair in reach has none to send, which OG-7 fixes the
first time a player comes near. Cost: one event a field a day and a chunk lookup. Refuelling, theft
at the field and visits stay as they are. Not Ben: it keeps OG-8's rule (dragons sense industry near
oil) and changes only when it fires.

**F3. Sense by chunks, not by the centre** (B2, and C3's joined districts). `Infrastructure` gains a chunk test (the
districts with any chunk within r of a point; the industry engine holds every district's chunk set),
used by `sense` and by F2. `DistrictIndex::refresh` rebuilds whenever any district changes (a change
counter in the industry engine), not only when their number does. A well joined to a base by pipe,
belt, shaft or Ender Transmission is then sensed as the well it is, and its raids choose from the
whole district's exposed blocks: linking a well to the base brings the dragons to the base. Not Ben.

**F4. A roof delays a flyer; it does not stop one** (C1, C2). Needs Ben: it changes OG-11 as he
approved it.
- **(a) Recommended.** For each roofed machine, the survey also lists the top block of its column
  when that block lies within `roof_reach` (PLACEHOLDER 8) blocks above the machine, flagged ROOFLESS
  (it sees the sky). A flyer breaks it like any roofless block; the next survey finds the machine
  open. A shed costs the dragons a raid or two; more than 8 blocks of cover still shields, and the
  deep layer (F6) covers that.
- (b) Keep OG-11 as approved: a roof shields every block, and only theft and the danger to players
  reach a roofed well. A roofed well that sends its oil away at once is then beyond every unwatched
  raid.
- (c) Stage 4 fire dragons ignore the roof gate; younger ones keep it.

Ben chose (a) as OGB-7. **Built** (docs/choices-oilgate.md OGC64, OGC65), with its tests.

**F5. Survey what is loaded** (C3). A survey runs only when every chunk of the district is loaded
(`Threats.java:441`), so a district larger than the loaded area never gets an exposed set and no raid
on it breaks anything (`threat.rs:528`). Survey the loaded chunks of a partly loaded district for its
exposed blocks, and keep each chunk's last rows until that chunk is surveyed again (the
`IND_SURVEY_BLOCKS` parts then carry their chunk); keep the all-loaded rule for the perimeter cut,
which needs the whole area. Not Ben.
**Built** (docs/choices-oilgate.md OGC61 to OGC63), with its tests.

**F6. The underground half, as decided** (C4). OG-9e's Death Worm oil nest,
asleep until a pumpjack drills, is already Ben's decision and its seams exist
(`docs/choices-oilgate.md`, the seam section). Build it. Nothing new to decide.

**F7. Name the builders** (C6). `sense` names the players who placed or broke blocks in the district
in the last `grudge_window` days (PLACEHOLDER 10; the industry engine journals every `IND_EDIT` with
the player), as well as those whose home is within 512 blocks. A remote well's builder is then
hunted at home and on sight, wherever they live. Not Ben (it is what OGC16 meant P26's homes to stand
in for). It needs one addition: the industry engine reads `IND_EDIT` but drops its player
(`rtsim-industry/src/engine.rs:782`), so each district would keep its last few editors and their
times, saved with it.

**F8. Danger where the oil is** (F1, and bar 2's first window). §3's recommendation (a). Ben. Before
trusting field flights or In Control at night, run OGC35's probe by night, as §4.8 already asks (F2).

**F9. A watched raid does the bounded damage too, as P28 decided** (C5, F4). Ben decided P28 as
"bounded damage too"; the build followed R19, the earlier recommendation, so a watched raid on a
district only brings the real dragon (`infra.rs:6-8`). A raid on a watched district then also
resolves through the threat interface as an unwatched one does, blocks and theft, after the real
fight, so watched and unwatched raids agree (P28's own words). Standing guard in a box keeps the
player alive; it no longer keeps the well. Not Ben: it builds what he decided.

**PF1. Ethanol** (D2). Ben chose a third way (OGB-8): ethanol stays as the pre-oil fuel and the pack
rebalances it to its real properties, so it is poor in diesel engines. Not part of the gate's build.
The options as asked:
- **(a) Recommended.** Keep ethanol behind oil, as `oilyunification.js` already means to: remove
  `createdieselgenerators:basin_fermenting/fermentable`, `createdieselgenerators:bulk_fermenting/fermentable`
  and `destroy:distillation/ethanol_from_moonshine` by recipe id, check mic_petrochem's own ethanol
  uses first, and confirm the removal in a server log.
- (b) Make ethanol the pre-oil fuel on purpose: leave the recipes, and the oil gate gates crude's own
  products (diesel, gasoline, plastics, asphalt, ANFO) rather than engine fuel as such.

**Not fixed, on purpose.** Carrying oil home (E rows) is not chased: once F1 to F3 make every well
seen and hurt, what a player carries home is what they paid for at the well. Killing the field's
dragons with real weapons (B5) is the gate working: an empty roost stands 20 days before a respawn
comes from 3,000 blocks (`rtsim-ecology/src/tunables.rs:185-187`).

**Tests, when it is built.** None written now. F2, F3 and F7 are engine code with unit tests in
`oil.rs` and `infra.rs` against the fake infrastructure: a drilling district 1,000 blocks from the
drink spot but on the field is sensed within a day; a well joined to a base 3,000 blocks away is
sensed; the index sees a district that grew. F4 and F5 are unit tests in `threat.rs`: a raid on a
roofed survey takes roof rows before machine rows; a partial survey keeps the other chunks' rows. F1
is Java: its own GameTest sets a hole down with `level.setBlock` and no player, the fix's test, not
a test of the cheese. F9 is an ecology unit test: a raid on a watched district books the same
bounded damage as an unwatched one.

## 5. For Ben

Recommended option first in each.

1. **In Control's fire dragon (§3).**
   - (a) When rtsim ships with F1 to F3: field flights on at the built draw, In Control's fire dragon
     rule out, blazes kept, and OG-10's "at least as dangerous as In Control" pass condition retired.
   - (b) Keep In Control as it is, with field flights off.
   - (c) Neither: lairs and watched raids only.
2. **Roofs (F4).**
   - (a) A roof within 8 blocks above a machine is itself a block a flyer can break, so a shed buys a
     raid or two.
   - (b) Keep OG-11: a roof shields every block.
   - (c) Stage 4 dragons ignore roofs.
Ben answered on 2026-10-21 (DECISIONS.md OGB-6 to OGB-8): 1 (a), 2 (a), and for 3 ethanol stays
pre-oil, rebalanced to be poor in diesel engines.

3. **Ethanol (PF1).**
   - (a) Keep it behind oil, as `oilyunification.js` intends: remove the fermentation and moonshine
     recipes by id.
   - (b) Make it the pre-oil fuel on purpose.

Decided here as reversible choices, not for Ben:
- F1 is a pumpjack-only exception to DS-10. Its wider forms would amend DS-10 and would be his.
- F2's sensing dragon comes from the nearest fire lair within `lair_reach`.
- F4's `roof_reach` is 8 and F7's window is 10 days, both PLACEHOLDER.
