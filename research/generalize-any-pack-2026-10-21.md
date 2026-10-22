# Generalizing rtsim to any pack: any tech mod, any size (2026-10-21)

Ben asked (2026-10-21): "Also, how easy would this system be to generalize s.t. it can work for any
arbitraliy sized pack?", then clarified: "By arbitrarily sized pack I also meant any pack with
arbitrary mods for it's factories not necessarily create." This note answers both. The main question
is whether rtsim can simulate factories built from any tech mod with no Create at all. The second is
whether it holds from single player with a few mods up to a 500-mod pack on a 1,000-player SMP.

This is a read-only assessment of main at 03022e8. It builds on `docs/pages/create-addon/index.html` and
`research/population-inventory-2026-10-21.md` and does not repeat them. Mod facts in §12 were checked
against each mod's 1.21.1 source on GitHub.

## 0. Verdict

1. **Yes, mostly already.** rtsim's industry never models machines. It measures stock at a district's
   edge, then learns and replays the net rates (`crates/rtsim-industry/src/learn.rs:1-10`, INDUSTRY.md
   §4.1). Any block entity with a NeoForge item, fluid or FE capability is seen
   (`Districts.java:969-996`). So a Mekanism, Immersive Engineering, Modern Industrialization or
   GregTech factory with no Create is grouped, learned and replayed unloaded today, with no plugin.
2. **What breaks on a non-Create pack is generic plumbing, not machines.** Four things, each S:
   districts seed only from blocks a player places; only chests and barrels count as stock; opening any
   machine GUI in a district discards the learning window; and pipes that expose no capability (Pipez)
   are invisible to the district closure.
3. **The real per-mod work is storage networks and wireless links.** AE2, Refined Storage 2, Mekanism
   QIO, Entangloporters and EnderStorage hold goods where no NeoForge capability shows them. Each needs
   a plugin (M for AE2 and RS2, S for the rest), plus one shared-store link in Rust (M).
4. **Exact replay need not stay Create-only, but it should come last.** A learner constrained by the
   vanilla RecipeManager (M) lifts every mod at once. Exact per-mod models then cost S for Mekanism,
   whose tick formula is public, and L for GregTech.
5. **Mod count costs nothing per tick; world size does.** On a 2M-chunk SMP on a weak PC the walls are
   resident districts (M), one-tick detection (S) and an unbounded goods dictionary (S). A non-MIC pack
   also needs an industry-only profile with optional plugins (S, plus M for packaging).

**Sizes** used throughout: **S** is up to about 500 lines or one focused agent-day with tests. **M**
is 500 to 3,000 lines, a few days to two weeks. **L** is more than that, or it changes persisted
formats across modules.

---

## 1. What "any pack" means here

- **Any mod set.** Take a vanilla + Mekanism pack, or a kitchen-sink pack like ATM10 (504 mods,
  1.21.1 NeoForge). The 8,107-chunk ATM10 S2 sample
  (`research/stress-worlds-2026-10-21-data/audits/atm10-s2-sample-8k-chunks.json`) holds 26,505 block
  entities:
  - AE2 family 2,695 (ae2 1,762, extendedae 562, advanced_ae 371)
  - Create 1,554
  - Mekanism 745
  - Extreme Reactors 561
  - Immersive Engineering 384
  - Functional Storage 350
  - Pipez 255
  - Super Factory Manager 189
  - Sophisticated Storage 127

  So a kitchen-sink factory is mostly AE2 plus Mekanism, with drawers and Pipez in between.
- **Any size.** This runs from single player on a weak PC (D1, `DECISIONS.md:16`) to an SMP on the
  scale of TBSMP S2: 2,097,750 chunks, 3.45M block entities and 1,041 players
  (`research/stress-worlds-smp-2026-10-21.md` §4.1).
- **Thermal is out of reach.** Thermal Expansion has no 1.21.x release, and no CoFH repo has a branch
  newer than 1.20.1 (§12). rtsim is 1.21.1 NeoForge, so Thermal would need a backport of all of
  rtsim.

---

## 2. A factory with no Create, today

Take a Mekanism ore line: a player places an enrichment chamber, a crusher and an energized smelter,
joins them with logistical transporters, feeds them from a chest, and they output into a second chest.
This is what each stage of rtsim does with it, read from main.

| Stage | What happens | Where | Works for any mod? |
|---|---|---|---|
| **Seen** | A block entity is industrial if a plugin says so, or if it has an item, fluid or energy capability, unsided or on any of 6 sides (up to 15 lookups) | `Districts.java:969-996` | **Yes.** Mekanism, IE, MI, GTCEu and Industrial Foregoing machines all expose these capabilities. |
| **Seeded** | Only a block a non-FakePlayer places (`EntityPlaceEvent`) can seed | `IndustryEvents.java:110-117`, `Districts.java:227-245` | **Partly.** Machines built before rtsim was installed never seed. The only fix is the operator command `/rtsim district adopt [radius]` (`Districts.java:275`, `IndustryCommands.java:230-239`). DS-10 on `fix/district-seeds` (not merged) narrows seeds to "machines": a non-vanilla ticking block entity with a capability, minus `#mic_rtsim:district_not_seed`. It keeps the player-placed rule (`Districts.java:248` on that branch). |
| **Grouped** | Industrial chunks join by 8-neighbour adjacency. A member's block entity touching one across a chunk border extends the district. Plugin `links()` and operator merges also join chunks | `Districts.java:633`, `:916`, `:936` | **Yes, if pipes have a capability.** Mekanism transporters and cables do. Pipez item and fluid pipes return null or a 0-slot dummy handler (§12), so a Pipez line through empty chunks is invisible. |
| **Stock** | Only blocks in `#mic_rtsim:district_stock` are boundary stock. Core lists `#c:chests`, `#c:barrels`, chest, trapped chest and barrel | `Districts.java:60-62`, `:900`; `core/.../tags/block/district_stock.json` | **No for most storage.** Mekanism bins and energy cubes, IE crates, Functional Storage drawers, Sophisticated Storage and AE2 or RS2 storage count only if tagged or declared by a plugin. |
| **Learned** | The net production of each good is stock change minus port flow, `N_g = S_g - S_g(t0) - P_g`. A least-squares slope is fit over a steady window of up to 72,000 ticks with 512 points. Coupling is learned from starvation, up to 48 goods | `learn.rs:1-10`, `learn.rs:178-181`, `tunables.rs:25-30` | **Yes.** No recipe, cycle time or power draw is needed. |
| **Discarded** | Opening any menu on a district block that is not boundary stock discards the window. So does opening any menu while standing in a district without a click on stock, such as a backpack or a wireless terminal | `PlayerPorts.java:111-128`, INDUSTRY.md §3.4 | **No, for GUI-heavy mods.** Every look at a Mekanism, IE or MI machine GUI throws away the learning window. Create machines have few GUIs, which is why this never showed. |
| **Replayed** | One all-or-nothing process per district, or several split by learned coupling. Breakpoints come when an input runs out or a learned target is reached. On load the result is reconciled into the stores through plain `IItemHandler`, `IFluidHandler` and `IEnergyStorage` | INDUSTRY.md §4.1, `StockAccess.java:118-170` | **Yes.** |
| **Energy** | FE is a good, `e/0/fe`, but only in stock-tagged blocks | `Goods.java:37` | **Partly.** A Mekanism energy cube is stock only once tagged. Generators inside the district are simply part of the box. |

**What a player sees on a pure Mekanism pack today:** with the stock tag extended and the district
adopted, the ore line learns and runs while unloaded. If the player keeps opening machine GUIs while
loaded, it never learns and stays frozen, which is today's vanilla behaviour, never worse. On an
AE2-centred base the machines pull from and push into the ME network, so no chest changes. The
district then learns "nothing happens" and replays nothing, which is again only today's behaviour.

---

## 3. Machine models: what each gets right and wrong

Five candidates, from what exists to what would be new. They stack, and none replaces the black box.

### 3.1 Observe and replay (the black box, built)

Measure the boundary stock while loaded, fit rates, and replay them while unloaded.

- **Gets right:**
  - **Any machine.** Machines are never seen, so an unknown mod is not a special case.
  - **Multiblocks.** Only their ports matter.
  - **Random outputs.** IE crusher secondaries and GT chanced outputs come out at their measured
    average.
  - **Internal buffers.** Their error is bounded by the buffer size.
  - **Loops.** A machine that feeds back into the same chest nets out.
- **Gets wrong or misses:**
  - **Stale rates.** Rates change unseen while unloaded. That does not happen, because nothing runs
    unloaded except the replay.
  - **Partial jams.** One all-or-nothing process stops the whole district when any input runs out.
  - **Big districts.** More than 48 goods means no coupling at all (`learn.rs:181`, `:349`). A
    GregTech line is easily past 48.
  - **Day-dependent power.** Solar power makes the half-window slopes disagree, so the district never
    turns steady. Only a plugin forces the one-day window (INDUSTRY.md §3.4).
- **Tier upgrades:**
  - **Through a GUI.** Today that is a discard and then a relearn, which is the right outcome by
    accident.
  - **Mekanism tier installers.** They change the block entity in place without an `EntityPlaceEvent`,
    so it is not an edit. The steady test still catches the change and a newer window replaces the
    model.

### 3.2 Sections (SEC-1, designed, not built)

Split a district at countable links and at stores, and learn each section on its own
(`DECISIONS.md:269`).

- **Gets right:**
  - **Partial jams.** One section starving no longer stops the rest.
  - **The 48-good cliff.** Big districts are split below it.
- **Mod-agnostic?** Only if the cut points are found generically. Stores are generic once the stock
  tag is right. Links are Create-shaped today (packages, frogports, stations).
- **For other mods:** a store between two machine groups is the natural cut. AE2 pattern providers and
  interfaces are the cut points of an ME base, and that needs the AE2 plugin.

### 3.3 Recipes from the RecipeManager (new, M)

GTCEu (`GTRecipeType implements RecipeType`), Modern Industrialization (`MachineRecipeType`, registered
through `Registries.RECIPE_TYPE`) and vanilla all keep their recipes in the vanilla `RecipeManager`
(§12). Mekanism and IE register recipe types the same way. That last point was not re-checked in this
pass (U).

- **What it gives:**
  - the possible transformation vectors (inputs and outputs per craft) for every machine recipe in
    the pack, read once at server start and on reload.
- **What it does not give:**
  - **Durations.** These are per mod, and per tier and upgrade.
  - **Which machine runs which recipe type.** JEI catalysts and EMI workstations are registered only
    on the client, so a dedicated server never sees them (§12).
  - **Fluids, chemicals and chances,** through the vanilla API. `Recipe.getIngredients()` and
    `getResultItem()` cover items only, and not every mod fills them.
- **Best use: recipe-constrained learning.** Fit the learned net-rate vector as a non-negative
  combination of recipe vectors (NNLS):
  - **Structure for free.** The fit says which outputs need which inputs. That replaces the
    quadratic coupling partition and its 48-good cap with recipe structure.
  - **Noise rejection.** A fit with a small residual is a confident model. A large residual means "not
    explained by known recipes": a mob farm, a world-mutating machine, or a mod reading recipes
    in code.
  - **No machine mapping needed.** It matches by goods, not by machine, so neither catalysts nor
    plugins are needed.
- **Per-mod adapters for vectors** (S each, 100 to 300 lines):
  - Mekanism: chemicals and chance outputs
  - MI: fluids and EU
  - IE: fluids and chances
  - GTCEu: its content map with chanced and tiered outputs, so S to M
- **Gets wrong:**
  - recipes computed in code rather than stored;
  - outputs that carry data components;
  - machines with no recipes at all (Digital Miner, laser drills, quarries).

  These fall back to plain observe-and-replay.

### 3.4 Energy as an input (partly built)

- **What exists:**
  - FE is a good, `e/0/fe` (`Goods.java:37`).
  - Stock-tagged energy blocks are read and written through `IEnergyStorage`.
  - `Obs::PowerLink` couples two districts' running states over a power line (`obs.rs:461`).
- **Generic gap:** energy storage is stock only when tagged. One shipped tag list fixes that (S).
- **Power starvation:**
  - **Stored energy feeds the district.** When the energy store runs dry the process stops at that
    breakpoint. That is right.
  - **Live generators feed it** (solar, a reactor inside the box). They are invisible, and starvation
    appears only as a lower measured rate. That is right while the generator is steady.
  - **Day and night swing.** The district never turns steady, and §3.5 fixes it.
- **Gets wrong:** power that crosses a district boundary over a non-adjacent wire. IE wire connectors
  move FE between connectors up to the wire's length apart (§12). Each such mod needs a `PowerLink`
  plugin (S).

### 3.5 Machine activity from block states (new, S)

Mekanism machines have a boolean `active` block-state property (`AttributeStateActive`, §12). Vanilla
furnaces have `lit`. Many mods follow the same pattern (U per mod).

- **The mechanism:** at each sweep, count the district's block entities whose state has a boolean
  `active`, `lit`, `working` or `powered` property set.
- **What it gives, with no plugin:**
  - a duty cycle per machine type;
  - a generic starvation signal ("machines idle while inputs are stocked" means power or output is
    the limit);
  - a day-cycle detector (activity periodic at 24,000 ticks forces the one-day window, the generic
    form of what INDUSTRY.md §3.4 does only by plugin).
- **Gets wrong:** mods without such a property, and multiblocks whose controller has it but whose
  parallel count is hidden.

### 3.6 Speed and upgrade modifiers (per mod, only for exact replay)

- **What it would take:** reading upgrade slots means a plugin per mod. Mekanism's formula is public
  (§12):
  - `ticks = base * M^(-speed/8)`
  - `FE per tick = ceil(base * M^(2*speed/8 - energy/8))`
  - `M` is `maxUpgradeMultiplier`, default 10.
- **When it is needed:** only for the glass box's exact replay (§6). The black box needs none of it,
  because it measures the upgraded rate.

### Summary

| Model | Tier upgrades | Multiblocks | Random outputs | Power starvation | Cost |
|---|---|---|---|---|---|
| Observe and replay (built) | relearn after the steady test or a discard | yes (ports only) | average | stored: exact; live: average; day cycle: never steady | built |
| Sections (SEC-1) | per section | yes | average | per section | M, after the generic cut rules |
| Recipe-constrained fit | relearn, structure unchanged | yes | expected value via adapter | as observe | M + S per adapter |
| Activity sampling | duty cycle shows it | controller only | n/a | detects it | S |
| Upgrade formulas | exact | per mod | per mod | per mod | S to L per mod |

---

## 4. Plugins versus generic

**A per-mod plugin is unavoidable where goods or links exist outside NeoForge's three capabilities.**
That covers:

1. **storage networks** whose contents sit in cells or frequencies;
2. **non-physical links:** wireless and frequency-keyed transport;
3. **non-standard resources:** Mekanism chemicals;
4. **finite deposits** that a mod keeps in its own SavedData;
5. **exact replay.**

Everything else is generic, or is data (tags).

| Mod (1.21.1) | Generic gets | Plugin adds | Plugin size |
|---|---|---|---|
| **Mekanism** 10.7 | Machines, transporters and cables (they have capabilities). Bins and energy cubes once tagged | Chemicals as goods: own `Capabilities.CHEMICAL`, one chemical per tank, maps onto the fluid class. QIO and Entangloporter frequencies as shared stores (no distance or dimension check, §12). Digital Miner as Out (progressive) | **M** (chemicals S, QIO S to M, Entangloporter S) |
| **AE2** (with ExtendedAE, Advanced AE) | Interfaces and pattern providers are industrial. Nothing shows network contents: the Drive's item handler is its cell slots, and the Interface's handlers adapt its stocking slots only (§12) | The grid as one store: `IGrid.getStorageService().getCachedInventory()` for stock, `IStackWatcher` for changes. Terminals as ports. P2P, quantum bridges and wireless connectors as links | **M** |
| **Refined Storage 2** (2.0.x) | The same: the disk drive exposes disk items only (§12) | `StorageNetworkComponent` (a `RootStorage` with `getAll()` and listeners) as one store | **M** |
| **Immersive Engineering** | Machines, multiblock ports, conveyors (block entities with capabilities) | Wire `PowerLink`s (`EnergyTransferHandler` paths). Excavator veins as finite deposits: `MineralVein.depletion` in `IESaveData`, default yield 38,400 | **S** (two small parts) |
| **Modern Industrialization** | Everything (machines, multiblock hatches, pipes) | Only exact replay | none for the black box |
| **GregTech CEu Modern** (7.0.x for 1.21.1) | Everything at the black-box level. Big lines pass the 48-good cap | Recipe adapter (§3.3). Bedrock fluid and ore veins as deposits (U) | **S to M**, plus **L** for exact replay |
| **Industrial Foregoing** | Machines. Laser drills are production from nothing (fine) | Mob crushers are spawn-fed farms, Out under I9 | none |
| **Pipez** | Energy pipes (real `PipeEnergyStorage`) | Item and fluid pipes need a closure rule (§5) | **S** (a tag, no Java) |
| **EnderStorage** | Nothing (ender chests share one object per frequency, §12) | Frequency as a shared store | **S** |
| **Flux Networks** | Plugs and points have FE capabilities | Network as a `PowerLink` | **S** |
| **Functional Storage, Sophisticated Storage, IE crates, Mekanism bins** | Stock once tagged | none (data) | **S total**, one tag file |
| **Thermal** | n/a: no 1.21.1 release (§12) | n/a | n/a |

**Generic fallback** when a plugin is missing:

- **Storage networks:** the district learns "nothing moves" and freezes in effect. That is no worse
  than vanilla.
- **Wireless links:** the two ends learn as independent producer and consumer. Replay is consistent
  while both are unloaded, and drifts when only one end is loaded.
- **Chemicals:** stay inside the box, invisible. They only matter where they cross a boundary.
- **Deposits:** are treated as infinite, which is the exploit case I11 names. The fallback should
  instead mark these districts "finite deposit unknown", as INDUSTRY.md §6 case 4 already does.

---

## 5. Transport

**Physical pipes and conveyors** join districts through chunk adjacency and border touches
(`Districts.java:633`, `:916`), provided the pipe block entities have a capability.

| Carrier | Capability on the pipe? | Today |
|---|---|---|
| Mekanism transporters, cables, pipes, tubes | yes | joins (works) |
| IE conveyors | yes (block entities, §12) | joins |
| Pipez item and fluid | no (null, or a 0-slot dummy on extracting sides, §12) | **breaks the closure** |
| Pipez energy | yes | joins |
| AE2 cables | a bare cable bus is not a handler (U) | breaks unless parts expose one |
| Super Factory Manager cables | no (U) | breaks |
| Create belts, pipes, chain conveyors | yes, and plugin `links()` for chains (`CreatePlugin.java:76-81`) | joins |

**Generic fix (S):** a data tag, `#mic_rtsim:district_conduit`, of block-entity blocks that join
whatever they touch even without a capability. Ship it with `required:false` entries for Pipez, AE2
cable buses and SFM cables. Closure then follows conduits chunk by chunk, as it follows capabilities.

**Non-physical networks** are the real gap:

- **The one generic Java hook:** `IndustryPlugin.links(be, out)` (`IndustryPlugin.java:34`). It
  merges the linked districts into one. Ender Transmission uses it (`ChannelPlugin.java:23`).
- **What merging does to a big network:** one AE2 network, QIO frequency or EnderStorage frequency
  spanning a base and three outposts becomes one district over far-apart chunks. It is mostly
  PARTIAL-loaded (`IND_CHUNK_UNLOADED`, `Districts.java:186-200`) and easily past 48 goods.
- **The Rust side:** it has no generic link between districts, only `Obs::PowerLink` (`obs.rs:461`),
  `LinkKey::Mail` (`learn.rs:44`) and the train leg index.
- **Create 6 logistics are already the same problem.** `GaugeNetworks.java:18-50` only warns when a
  gauge network spans districts (INDUSTRY.md §8.4).

**The general mechanism: a shared-store link (M).** It is a store owned by no district:

1. **What it stands for:** an ME grid, an RS2 network, a QIO frequency, an EnderStorage frequency, an
   Entangloporter frequency, an Ender Transmission channel, or a Create stock network.
2. **How each district relates to it:** the district's flows to and from the store are ports, learned
   like train cargo.
3. **How replay works:** replay keeps the store's balance, and a district starves when the store
   empties.
4. **Where it comes from:** it generalizes `PowerLink` from FE to any good. Plugins only have to name
   the store and report its stock and its ports. The district closure stops at the link instead of
   merging.

This is the same split SEC-1 wants for Create's packages, so the work serves MIC and the Create addon
too.

**Power networks:**

- **Adjacent cables** (Mekanism universal cables, Pipez energy) are physical and join districts.
- **Non-adjacent power** (IE wires, Flux Networks, PowerGrid wires through its plugin) needs a
  `PowerLink` plugin (S each).
- **Create stress** stays inside one district by construction (INDUSTRY.md §6 case 1).

---

## 6. The glass box

**Status:** G1, a read-only compile and checksum, is on main. G2, where the glass box drives the
replay, is on `feat/glassbox-g2` (`GLASSBOX.md:3`).

**What is already general:**

- **The graph records.** There are 12 node kinds and 12 edge kinds
  (`crates/rtsim-industry/src/glass/records.rs:33-91`), recipes are `{good, num, den}` terms
  (`records.rs:338-350`), and the solver reads kinds, flags and tables, not model codes. Its only
  model-code use is the store codes at `check.rs:933`, and unknown codes are kept (`records.rs:144`).
- **The fallback.** Block entities no extractor claims become one residual `BLACK_BOX` node per
  district (`GraphBuilder.java:30` on G2, `records.rs:56`, `:437`). A district that does not compile
  falls back to the black box (`glass/mod.rs:130-136`). So a mixed Create and Mekanism district works
  today: Create exactly, Mekanism as a learned node.
- **The model tiers** are mod-neutral (`GLASSBOX.md:624-634`):
  - tier 1, JSON templates such as `timed` (fixed ticks per item);
  - tier 2, a Rust `MachineModel`;
  - tier 3, a Java `GlassModelProvider`.

**What is Create-only:**

- **The compiler.** `create/.../glass/CreateCompile.java`, `Recipes`, `Cycles` and `KineticWalk` are
  about 7,700 lines in the Create plugin.
- **Kinetic domains** with speed levels and overstress (`solve.rs:657-675`).
- **The code tables.** The 23 recipe-type codes (`records.rs:213-239`) and about 40 model codes
  (`records.rs:146-189`) form one global, Create-flavoured table.

**Does exact replay have to stay Create-only? No.** Most non-Create machines are simpler than Create's,
because they run a fixed duration per recipe from a power buffer, with no shared kinetic network:

- **Mekanism:** a `timed` template whose period comes from `MekanismUtils.getTicks` with the installed
  upgrades. FE per tick is a recipe term. Pipes are edges with no capacity (transporters outrun the
  machines). **S to M.**
- **Modern Industrialization:** the recipe holds duration and EU. Multiblock hatches are stores.
  **S to M.**
- **IE:** fixed-time machines, plus multiblocks with process queues. **M.**
- **GregTech:** overclocking tiers, parallels, recipe logic, hatches and covers. **L.**
- **AE2 autocrafting:** crafting CPUs schedule jobs on demand. Keep it a black box over the network
  store.

**A generic middle tier, "recipe glass" (M after §3.3).** When the recipe-constrained fit says a
district is explained by known recipes, the glass box can claim each machine. It matches a block
entity's id to the recipe types its goods fit, gives it a `timed` node with the measured period, and
treats pipes as unbounded edges.

- **What it buys:** per-machine replay with no per-mod Java.
- **What it gets wrong:** periods are measured, not computed, so an upgrade still needs a relearn.

**Recommendation:** keep exact replay Create-first through G2 to G4, as planned (`GLASSBOX.md:891`,
G-B18). Add recipe glass only after the generic slices, and per-mod templates only when a pack
that uses them ships.

---

## 7. The other MIC-specific parts

| Part | Where it is MIC-specific | Size to generalize | Cheapest general mechanism |
|---|---|---|---|
| **Create knowledge in the industry model** | Train, package, postbox and station ports and records in the black box: `obs.rs:76-78`, `:147`, `:185-230`, `district.rs:38`, about 120 train references in `engine.rs`; `IndRecords.java` | **none needed** | Dormant on a pack with no Create: the ports never fire. Leave it. |
| **Glass box** | §6 | see §6 | Residual black-box node, recipe glass |
| **Other tech mods' machines** | Nothing: the black box handles them (§2) | **S** (§9 G1 to G3) | Seeds, stock tag, GUI ports, conduit tag |
| **Non-physical transport** | Only Ender Transmission and Create chain conveyors via `links()`. Create 6 gauge networks are warned about, not modelled | **M**, then **S to M** per mod | Shared-store link (§5) |
| **Energy networks** | FE as a good (generic). PowerGrid's joule good and `PowerLink`s (plugin). Create stress inside glass only | **S** per wire mod | Generalized `PowerLink` |
| **Chunk-loading mods** | None. Load state comes from the real ticking state (`Districts.blockTicking`, `level.shouldTickBlocksAt`). FTB Chunks uses a NeoForge `TicketController` with ticking tickets, online-only by default (§12) | **none for correctness**, **S** for cost | A district that stays LIVE is never replayed and needs no learning. Skip sweeps on districts whose chunks have held a ticket for a long time (§8). |
| **Data-driven vs hard-coded content** | See the rows below | | |
| **Cultures** | Settle `Culture` {Human, Ribbit} (`crates/rtsim-settle/src/lib.rs:24-45`), currencies per culture | **M** | Culture ids from data, as conflict cultures already are in shape |
| **Factions** | Conflict cultures are JSON, but compiled in with `include_str!` (`rtsim-conflict/src/culture.rs:430-432`). `SignalKind` has smog and radiation from Destroy (`culture.rs:25-58`) | **S** | Load the JSON from datapacks |
| **Species** | Dragons: 3 Ice and Fire species (`rtsim-ecology/src/species.rs:11-37`). Herds: 5 vanilla species (`herds/species.rs:32-65`, `HerdBodies.java:43`). Prey table of 15 | **S to M** | Herds from an entity tag plus spawn data. Dragons stay a plugin module |
| **Tags** | `district_stock`, threat class tags (`ThreatClasses.java:19-22`), biome bits from `c:` tags (any mod's biomes classify, `rtsim-space/src/habitat.rs:66-98`) | **S** | Already data. Ship more entries |
| **Recipes (phase 1 economy)** | 9 goods, 9 recipes, 7 resources (`rtsim-world/src/synth.rs:19-110`, `rtsim-jni/.../phase1/catalog.rs:30-58`), checked in persisted formats (`rtsim-world/src/persist.rs:986`) | **L** | Not needed for factories. Only a pack that wants rtsim's villages and trade needs it |
| **Module selection** | Runtime bitmask of nine (`rtsim-host/src/save.rs:29`), `[modules]` booleans, all on by default (`RtsimConfig.java:49-51`). Phase 1 needs settlements, trade, travel and knowledge together (`rtsim-jni/src/engine/phase1.rs:117-121`) | **S** | A profile key, `profile = "industry"`, that sets the mask. The industry-only default is used when no MIC plugin is present |
| **Trader veto and other vanilla overrides** | `WanderingTraderSpawnerMixin` cancels only while trade and travel are on (C8a, `DECISIONS.md:268`), so an industry-only profile restores vanilla traders. 17 core mixins, all `defaultRequire 1` | **S** to check | Off with the modules. One risk to test on big packs: `LevelChunkTickGateMixin` holds every mod's block entity tickers for up to 20 ticks while a reconcile lands. AE2 grids and Mekanism transmitter networks form on early ticks (U) |
| **Config surface** | Server TOMLs for modules, budgets, industry, glass and threats (all PLACEHOLDER defaults). Every Rust tunable is a compile-time default, and Java feeds none (`rtsim-jni/src/engine/industry.rs:175`, `:231`, `:251`). No per-plugin switch | **S** | Pass the industry tunables (window, rate floor, coupling cap) from the TOML |
| **Plugin loading** | Each plugin declares its target mod `type = "required"` with a version range. Create is pinned `[6.0.10,6.1)`, while TBSMP runs 6.0.6. Plugin mixins are `defaultRequire 1`. No ModList-gated registration | **S to M** | `optional` dependencies plus ModList checks, or keep separate jars (works today: do not install a plugin whose mod is absent) |
| **Native platforms** | The cdylib loads only as a `mic_native` plugin (`rtsim-jni/src/jni_api.rs:81-86`). Platform keys: Linux gnu and musl, Windows, macOS, each x86_64 and aarch64; only the baseline variant is packaged (`core/build.gradle`). Exercised on Linux and on Windows under Wine (VIEWER.md V4). `mic-native`, `mic-viewer` and `deeptime-format` are private SSH git dependencies (`Cargo.toml:41`, `:55-70`) | **M** to publish | Ben's call (REL-1, `DECISIONS.md:263`). `deeptime-format` is used only by `rtsim-space`'s layout parsing (`rtsim-space/src/topology/layout.rs:11-90`), so it can be feature-gated (S) |

---

## 8. Scale: pack size and world size

**Pack size (mod count) costs nothing per tick.** Each item below is from code:

- **No iteration of registries, recipes or tags per tick.** Registry use is point lookups
  (`Goods.java:71-90`), and structures are walked once at start and on reload.
- **The goods dictionary is lazy.** It grows with distinct goods seen in boundary stock, not with
  registry size (`Journal.java:214-224`).
- **Recipe data stays per district.** The glass box has no global recipe table, only recipes
  referenced by a compiled district.

**Where a big pack does cost:**

1. **Goods dictionary growth from data components (S).**
   - **What happens:** every distinct component patch is a new good, and its patch is persisted
     forever (`Goods.java:94-110`: `patches.put(h, patch); data.setDirty()`).
   - **Which items churn:** energy-holding items, filled storage cells, damaged tools and backpacks
     change their patch constantly. A chest of AE2 cells or charging Mekanism tablets mints a new good
     each sweep.
   - **Cost in Rust:** `GoodVec` assumes "tens of goods, not thousands" (`ids.rs:165-172`).
   - **Fix:**
     - fold items whose patch carries an item capability (energy, items, fluids) or an
       unstackable-with-data component to the bare item id for learning;
     - drop patches no store has held for a save cycle.
2. **Goods per district.**
   - **The learner:** it keeps up to 512 points per good per learning district (`tunables.rs:28`,
     `:77`). A 200-good GregTech district is about 1.6 MB while learning (ESTIMATE, 16 bytes a point).
   - **Coupling:** the 48-good cap drops coupling.
   - **Fix:** the recipe-constrained fit (§3.3) and sections (SEC-1).
3. **Capability lookups at detection.**
   - **Cost:** up to 15 per block entity in member chunks (`Districts.java:969-996`), half of a
     rescan's time in the stress profiles.
   - **Packs with many decorative block entities** (FramedBlocks, Domum Ornamentum) add lookups that
     always fail. ATM10's sample is 26% FramedBlocks and Domum.
   - **Fix:** cache "never industrial" per block-entity type.

**World size is where the walls are.** Measured so far, on four Create worlds:

- **Run time:** rtsim's own work is 0.2% to 0.5% of the server thread, about 0.1 to 0.25 ms per tick
  (`docs/stress-worlds-results.md:10-14`).
- **Detection:** one full pass over 39 districts took 89 to 160 ms in one tick (`:328-334`).

| Wall | Today | At 2M chunks, 1,000 players, weak PC | Fix | Size |
|---|---|---|---|---|
| **Detection in one tick** | A rescan walks every member chunk of the districts in question in one tick; the full pass runs every 6,000 ticks | Thousands of loaded districts after a mass login or a restart give multi-second single ticks | Block-entity budget per tick over several ticks (stress results §7 (a), the glass fingerprint pass's 256 per tick) | **S** |
| **Resident districts** | Every `District` is in a `BTreeMap`; only clusters page (`engine.rs:243-248`); industry is not under the page budget | 8.6 KB per black-box district measured (`GLASSBOX.md:786-789`). 10k districts is 86 MB, 100k is 860 MB, past a potato's budget (D1) | Districts in a `LazyTable` under the kernel's page budget, as clusters are. The timing wheel already says which wake next (SCALE.md §8) | **M** |
| **Learner memory** | Goods × 512 points per learning district | Bounded by loaded districts, so fine unless many 200-good districts are loaded together | Reservoir-sample down to 128 points past the steady test | **S** |
| **Sampler budget** | 64 inventories and 20,000 slots per tick (`IndustryConfig.java:46-52`) | Bounded per tick, so a sweep stretches. Learning slows but stays correct | Skip sweeps on districts held LIVE by a ticket for longer than a learning window: they never replay | **S** |
| **Dictionary** | Unbounded with components | Grows with every churning item on a big server | Item 1 above | **S** |
| **Catch-up** | Event-driven timing wheel, O(breakpoints + events) per catch-up, independent of elapsed time (`cluster.rs:30-36`) | Fine | none | |
| **Hard caps** | `u64` district ids, `u32` goods, 2^20 glass records per kind, 3,000 LP variables, a 65,528-byte record size | A district bigger than the glass caps falls back to the black box | none | |

**The other modules** (herds, dragons, conflict, ships, phase 1 markets) already page under a 4 MiB
row budget each (`rtsim-jni/src/engine/paging.rs:45-47`, kernel default 64 MiB). SCALE.md §0's rule (cost
grows with observation, events and aggregates, never with how many things exist) holds for them. They
are MIC content, though, and would be off in an industry-only profile.

---

## 9. The generalization plan

The slices are ordered so a non-Create pack becomes useful as early as possible.

- **[MIC]** marks a slice that also improves MIC itself.
- **[Addon]** marks a slice the standalone Create addon (`docs/pages/create-addon/index.html`) needs
  too.

| # | Slice | What it unlocks | Size | Marks |
|---|---|---|---|---|
| **G0** | **Industry-only profile.** A `profile` key, and industry only when no MIC plugin is loaded. Industry tunables from the TOML | Any pack can run rtsim's factories without villages, trade or the trader veto | **S** | [Addon] |
| **G1** | **Seeds and stock for any mod.** Merge DS-10. Adopt machine seeds automatically on a chunk's first load after install (once per chunk, behind a world switch). Ship `district_stock` entries (`required:false`) for Mekanism bins and energy cubes, IE crates and capacitors, Functional Storage, Sophisticated Storage and Storage Drawers, plus a `district_not_seed` list of their cables | A Mekanism, IE, MI or GTCEu base, old or new, becomes districts with real stock | **S** | [MIC] (LiveSrv's existing bases), [Addon] |
| **G2** | **GUI sessions as ports.** A session with no change to the player's inventory is not a discard. A menu whose slots all belong to the clicked block entity's own containers is a port, its diff attributed as stock sessions are. Only crafting-style menus (result or crafting containers) stay discards | GUI-heavy mods learn while players use them | **S** | [MIC], [Addon] |
| **G3** | **Conduit tag.** `#mic_rtsim:district_conduit` joins chunks through capability-less pipes (Pipez, SFM, AE2 cable buses) | Pipez and SFM bases close properly | **S** | |
| *useful point* | *After G0 to G3, a Mekanism, IE, MI or GTCEu pack with chests, drawers and pipes works at black-box fidelity. That is about one agent-week.* | | | |
| **G4** | **Shared-store link in Rust.** A store owned by no district, with district ports into it. Plugins name the store and report its stock. Closure stops at links instead of merging | The base for every storage network and wireless link | **M** | [MIC] (Ender Transmission, Create 6 stock and gauge networks, INDUSTRY.md §8.4), [Addon] |
| **G5** | **Storage-network plugins.** AE2 (grid store via `IStorageService` and `IStackWatcher`; terminals as ports; P2P, quantum and wireless as links), RS2 (`StorageNetworkComponent`), Mekanism (chemicals, QIO, Entangloporter), EnderStorage | ATM10-style AE2 and Mekanism bases | AE2 **M**, RS2 **M**, Mekanism **M**, EnderStorage **S** | |
| *useful point* | *After G4 and G5, a kitchen-sink pack's main factories work.* | | | |
| **G6** | **Scale walls.** Budgeted detection, districts under the page budget, dictionary folding and patch GC, sweep skip for long-ticketed districts, a "never industrial" cache per block-entity type | Big SMPs and weak PCs | **M** (paging) + **S** each | [MIC], [Addon] |
| **G7** | **Recipe-constrained learning.** A recipe index from the RecipeManager at reload, the NNLS fit, and coupling from recipe structure. Adapters for MI, Mekanism, IE and GTCEu | Big GregTech lines past 48 goods. Confidence from residuals. Fewer frozen districts | **M** + **S** per adapter | [MIC] (Create addon machines are one learned node until glass G4), [Addon] |
| **G8** | **Activity sampling.** `active` and `lit` duty cycles, a generic starvation signal, a day-cycle window | Solar-powered and power-starved districts learn without a plugin | **S** | [MIC] |
| **G9** | **Power and deposit plugins.** IE wires and excavator veins, Flux Networks, GTCEu veins | Cross-district power and finite ores in those mods | **S** each | |
| **G10** | **Sections, generic cut rules** (SEC-1 with stores from the stock tag and links from G4) | Partial jams in big districts | **M** | [MIC] (SEC-1 is MIC's own plan), [Addon] |
| **G11** | **Exact replay beyond Create.** Recipe glass (§6), then Mekanism and MI templates | Per-machine replay for those mods | **M**, then **S to M** per mod, GTCEu **L** | |
| **G12** | **Packaging.** Optional plugin dependencies, per-plugin switches, public or vendored natives, a feature gate on `deeptime-format` | Installable outside MIC | **M** | [Addon] (needs Ben, REL-1) |
| **G13** | **Data-driven world content** (conflict cultures from datapacks S, herds from tags S to M, settle cultures M, phase 1 economy L) | rtsim's villages, trade and wildlife on other packs | **L** in all | |

**What helps MIC most, and soonest:**

- **G1:** DS-10 plus adoption for LiveSrv's existing bases.
- **G2:** Destroy, Vintage and PowerGrid have GUIs.
- **G4:** Create 6 stock networks and Ender Transmission without merging.
- **G6:** LiveSrv at scale.
- **G7:** Create addon machines that the glass box does not cover yet.

None of these is wasted on MIC, so the generic path costs MIC nothing.

---

## 10. Decisions for Ben

Each has the recommended option first. Everything else in this note is undoable and was chosen in
the text.

**Answered 2026-10-21:** Ben took (a) on all four (DECISIONS.md GA-1 to GA-4).

1. **Is "any pack" a goal?**
   - **(a) Recommended. A by-product.** Build only the slices that also serve MIC or the Create addon:
     G0, G1, G2, G4, G6, G7, G8, G10. Leave per-mod plugins (AE2, RS2, Mekanism, IE) until a non-MIC
     release is on the table.
   - (b) An active goal: also build G3, G5 and G9 now, about four more agent-weeks.
   - (c) No: keep rtsim MIC-only and skip G0 and G3.
2. **How storage networks and wireless links are modelled.**
   - **(a) Recommended. A shared store in Rust (G4).** Districts stay small, and the network is one
     store with ports.
   - (b) Merge every district on one network into one, as Ender Transmission does today. That costs
     nothing more, but giant PARTIAL districts follow, and the 48-good cap.
   - (c) Freeze any district attached to a network.
3. **Machine model beyond the black box for non-Create mods.**
   - **(a) Recommended. Recipe-constrained learning first (G7), recipe glass later (G11).** No per-mod
     Java until a pack needs exactness.
   - (b) Per-mod exact models now, starting with Mekanism.
   - (c) Black box only, forever, for non-Create mods.
4. **Release shape for a non-MIC pack** (outward-facing, REL-1).
   - **(a) Recommended. Nothing public until you say so.** Build G0 and G12's in-tree parts so a
     release is a switch, not a project.
   - (b) Plan a public "rtsim industry" mod with `mic-native` and `deeptime-format` made public or
     vendored.
   - (c) Ship rtsim industry inside the Create addon only.

---

## 11. What the TBSMP run will settle

The parallel TBSMP stress run had not published `docs/stress-tbsmp-results.md` when this was written.
Its worktree is compiling the Create and Carry On plugins against TBSMP's Create 6.0.6
(`feat/tbsmp-compat`, d7c68a1, "Create and Carry On ranges as properties"). It will settle:

1. **Version spread.**
   - **The question:** does the Create plugin work on a Create older than its `[6.0.10,6.1)` pin?
     That is the first real test of plugin loading on someone else's pack (§7, plugin loading).
2. **Detection at scale.**
   - **The world:** 633k overworld block entities, 112k of them Create, and 1,041 players' worth of
     builds.
   - **What it shows:** the one-tick rescan and `adopt` cost on a real megabase. That decides whether
     G6's budgeted detection is urgent.
3. **District count and resident memory.**
   - **The question:** how many districts a real SMP makes, and so whether paging districts (G6) is
     needed now or later.
4. **Dictionary growth.**
   - **The question:** how many goods and persisted patches a 144-mod world mints. Sophisticated
     Backpacks and Storage with Create integration are in it.
5. **Not settled by it:**
   - **Non-Create factories.** TBSMP is a Create pack. The non-Create questions need a Mekanism or AE2
     world, and the ATM10 S2 sample (§1) is the candidate fixture.

---

## 12. Mod facts checked on the web (1.21.1 source), and what is unverified

All facts below were checked against each mod's 1.21.1 branch on GitHub, read by a Sonnet sub-agent on
2026-10-21:

| Mod | Fact | Source |
|---|---|---|
| AE2 (`1.21.1`) | Only the generic `AEBaseInvBlockEntity` item handler, which is the Drive's cell slots. The ME Chest's handlers cover its own cell. On the Interface, `ME_STORAGE` is the whole grid when no slots are configured, but its item and fluid handlers adapt the stocking slots only. No block's `ItemHandler` shows network contents. `IStorageService.getInventory()` and `getCachedInventory()`; watchers via `IStorageWatcherNode` and `IStackWatcher` | `appeng/init/InitCapabilityProviders.java`, `appeng/helpers/InterfaceLogic.java`, `appeng/api/networking/storage/IStorageService.java` |
| Refined Storage 2 (`support/2.x`) | The disk drive's handler wraps disk items only. Network storage is `StorageNetworkComponent` (`RootStorage`: `getAll`, listeners) | `refinedstorage-neoforge/.../ModInitializer.java`, `.../root/RootStorage.java` |
| Mekanism 10.7.19 (`1.21.x`) | Chemicals use `Capabilities.CHEMICAL` (`mekanism:chemical_handler`). The upgrade formulas are in §3.6. Machines have the `active` block-state property. Entangloporter and QIO frequencies have no distance or dimension check | `MekanismUtils.java`, `common/capabilities/Capabilities.java`, `AttributeStateActive.java`, `InventoryFrequency.java` |
| Immersive Engineering (`1.21.1`) | Wires carry FE between non-adjacent connectors (`EnergyTransferHandler`, per-wire loss). Excavator veins deplete (`MineralVein.depletion`, `excavator_yield` 38,400, kept in `IESaveData`). Conveyors are block entities | `api/wires/localhandlers/EnergyTransferHandler.java`, `api/excavator/MineralVein.java`, `common/IESaveData.java` |
| JEI (`1.21.1`), EMI (`1.21`) | Catalysts and workstations register only on the client | `JustEnoughItemsClient.java`, `EmiReloadManager.java` |
| Pipez (`1.21.1`) | Item and fluid pipes return null, or `DummyItemHandler` (0 slots) on extracting sides. Energy pipes expose a real `PipeEnergyStorage` | `PipeLogicTileEntity.java`, `DummyItemHandler.java` |
| FTB Chunks (`1.21.1/main`) | A NeoForge `TicketController` with ticking tickets keyed by team. `force_load_mode` is `NEVER`, `ALWAYS` or `DEFAULT`. `DEFAULT` loads offline only with permission `ftbchunks.chunk_load_offline`, false by default | `neoforge/ForceLoading.java`, `FTBChunksWorldConfig.java`, `ChunkTeamDataImpl.java` |
| GTCEu Modern (`1.21`), MI (`1.21.x`) | Both keep recipes in the vanilla `RecipeManager`. GTCEu 7.0.x is released for 1.21.1 (Modrinth marks it beta) | `GTRecipeType.java`, `RecipeManagerLateMixin.java`, `MachineRecipeType.java`, `MIRegistries.java` |
| EnderStorage (`1.21.1`) | Exists (2.13.0). Chests on one frequency share one storage object | `EnderStorageManager.java`, `TileEnderChest.java` |
| Flux Networks (`1.21`) | Exists for 1.21.1 (8.0.0). The transfer code was not read | `gradle.properties`, CurseForge file 6089446 |
| Thermal Expansion | No 1.21.x release or branch | teamcofh.com downloads, CoFH GitHub branches |

**Unverified (U) in this note:**

- that Mekanism and IE register their recipe types in the vanilla registry;
- that bare AE2 cable buses and SFM cables expose no handler;
- which other mods carry an `active`- or `lit`-style property;
- GTCEu's vein storage;
- how `LevelChunkTickGateMixin` interacts with AE2 and Mekanism network formation.

None of these changes the verdict. Each changes only the size of one slice.
