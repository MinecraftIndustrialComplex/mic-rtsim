# Surplus money buys real things

**Status:** design for Ben's review, 2026-10-20. Nothing is built. Ben approves this design before code.

**Decided, and binding here:**
- **E-sink:** surplus coin above a treasury's target is not destroyed. It buys something real and
  visible instead: village upgrades, buildings, tribute. This replaces round 2's "spent and leaves the
  world" sink, and conservation stays exact.
- **P9:** the tunables `r`, `T` and `k` come from a soak; the Numismatics mint rule comes later.
- **Prices:** the engine sends ready-made offer bundles; Java only shows them. **G31:** vanilla's
  reputation price change is folded into the sim's quote.
- **M3/M4 with C22:** villages advancing, declining or being founded are decided later, together with
  the sim taking over structure generation. Until then, growth is ribbit houses only (P17, E56).
- **Also in force:** C13 (storehouse markets, price a pure function of stock), C14 and C23 (trades go
  through the treasury), P12 (dragons prey on village livestock), R5 (what dragons take is destroyed in
  the world's books and hoarded in theirs), D1 (performance on a weak PC is the only judge), D3 (no
  bespoke blocks, items or entities), D4 (the core needs no pack mod).

**Sources.** The sink as built: `crates/rtsim-trade/src/line.rs` (the line law), `market.rs` (where the
money ceiling is set), `tunables.rs` (`Rules::money_sink`), `crates/rtsim-world/src/check.rs`
(conservation), docs/overnight-choices-phase1.md (E43 to E68, the quiescence measurements). House
building: `neoforge/core/.../growth/HouseBuilder.java`, `ResidentSpawner.java`,
`crates/rtsim-world/src/ribbits.rs` (E56). Vanilla facts: the 1.21.1 client jar's data and the
decompiled sources in the Gradle cache. Mod facts: the pack's jars, each named where used.

**Numbers:** every number is a verified fact, arithmetic from one, or a labelled TUNABLE or
PLACEHOLDER. Anything unverified says UNVERIFIED.

---

## 0. What the player sees

A village that trades well gets visibly richer. Brindle, a plains village that sells a lot of wheat and
buys little, stops piling up emeralds in a number nobody can see. Instead, lamp posts from the plains
village's own decor set go up along its streets, an extra iron golem walks its square, and, after the
fire dragon to the east took its cows twice, Brindle starts paying it: each season a few emeralds go into
the chest in the dragon's roost, and the dragon hunts elsewhere. A player who robs that roost finds
Brindle's emeralds there. A ribbit village does the same with amethyst and its own pieces.

Nothing new is added to the game: every piece is a template the village's own structure already uses,
the golem is vanilla's, and the hoard is Ice and Fire's.

---

## 1. Reuse table

| What happens | Real mechanic | From |
|---|---|---|
| Surplus coin leaves the treasury | E43's line law: above the ceiling a money line loses everything over it each quantum, in closed form | built: `rtsim-trade` `line.rs` |
| Where the coin goes instead | a per-market works purse, a new holder in the books | new (small) |
| A village builds something | `StructureTemplate.placeInWorld` on a free plot, a few layers a tick under a block quota, never over player blocks | built: `HouseBuilder` (PLACE_HOUSE, HOUSE_PLACED) |
| What it builds, human villages | the village type's own lamp and decor templates, from its `decor` pool (§4.2 lists them) | vanilla 1.21.1 client jar, `data/minecraft/structure/village/<type>/` |
| What it builds, ribbit villages | Ribbits' own `decor/lamp_post`, placed with Ribbits' processor list `ribbits:main` as its houses are | Ribbits 4.1.6, `data/ribbits/structure/decor/` |
| A village hires protection | an iron golem as vanilla's own village template holds it (`village/common/iron_golem`: `PlayerCreated` 0, `PersistenceRequired` 1, health 100): it defends the village, may attack players (a player-created golem never does), turns on a player at reputation -100 or lower (`DefendVillageTargetGoal`), and never despawns (`AbstractGolem.removeWhenFarAway` is false) | vanilla 1.21.1; spawned by the built SPAWN_RESIDENT |
| Golems matter to raiders | a village's defenders already cost a raiding dragon utility (`village_risk`) | built: `rtsim-ecology` |
| Tribute to a dragon | coin into the lair's hoard, a holder in the dragons' ledger; robbed hoards report HOARD_ROBBED | built: `rtsim-ecology` (R5, R22), `mic_rtsim_iceandfire` (Z16) |
| Tribute made lootable | the coin placed into a chest Ice and Fire generated in the lair, one stack per command; breaking a chest or pile within 30 blocks of a wild dragon wakes it and makes the player its target (`ServerEvents.onBreakBlock`, `goldSearchLength` 30 in the pack's `iaf-common.json`) | built: TRANSFER_TO_CONTAINER (B24, J11); Ice and Fire CE 2.1-beta.1 |
| The price of a work | the labour cost anchors `p*` of the work's blocks, from the server's recipes | built: `rtsim-trade` `labour.rs` |
| A work's identity | op id from (village, nth work), so a replay books the same work once | built: E56's identity op ids |

## 2. Custom-roll ledger

No new blocks, items or entities (D3).

| Piece | Where | Size | Notes |
|---|---|---|---|
| Works purse and its books | Rust, `rtsim-trade` + `rtsim-world` | S | redirect `sunk` for currency lines into the purse; new book terms `works`, `tribute`; the purse in conservation and pages |
| Works catalogue loader | Rust + datapack JSON | S | `data/<ns>/mic_rtsim/work/<id>.json`, like P15's body kinds |
| Template costing | Java + Rust | S | one TEMPLATE_BLOCKS record per template per session (item counts from the template's palette); priced with the market's anchors |
| Purchase scheduler | Rust, `rtsim-world` | S | one wheel entry per village with an affordable work; identity op ids |
| Decor placement | Java + Rust | S | PLACE_HOUSE with the decor template and its pool's processors; HouseBuilder unchanged; the engine queues no resident on a work's HOUSE_PLACED (E56 queues one for a house) |
| Golem spawn | Java | S | SPAWN_RESIDENT with body `minecraft:iron_golem`, at the meeting point |
| Tribute | Rust, `rtsim-ecology` | S | a tribute row per (village, lair), one event a season, a target filter in `weigh` |
| Tribute into the lair chest | Java, `mic_rtsim_iceandfire` | S | on the lair's load, TRANSFER_TO_CONTAINER into a chest inside the lair box (R23's deferred step) |
| `sim quiet` column and tests | Rust | S | works and tribute events per village per day beside market events |

---

## 3. What the sink does today, and why it exists

**The rule as built** (E43, `line.rs`). Every good at every market follows one law per quantum:

```text
pre  = S + inflow - outflow - consumption
prod = clamp(target - pre, 0, capacity)
sink = clamp(pre - ceiling, 0, slack)
S'   = pre + prod - sink
```

For a currency line the ceiling is twice the treasury target `Mbar` and the slack is unlimited
(`market.rs`, where `money_sink` is on), so a treasury never ends a quantum above `2 Mbar`: whatever is
over it is booked `book.sunk` and is gone. For a goods line the ceiling is the target and the slack two
news steps (PLACEHOLDER): a village with plenty eats a little more and lets a little spoil. Foreign coin
above twice its holding target sinks the same way. Conservation counts `sunk` as a burn
(`check.rs`).

**Why it was needed.** Round 1's closed world never went quiet. Nothing but players removed money, so
surplus villages piled coin until they refused cash, barter loops followed, and every refusal and
barter was a market event. Measured with `rtsim-cli sim quiet` and its trace (events per market per day,
closed worlds, no player):

| World | No money sink | Round 2 (sink on) |
|---|---|---|
| Closed 11-node world | 2.1 to 2.7, steady, never quiet | 0.00 from day 200 to 1,000 |
| Grid of 16 villages | not measured | 0.00 to 0.05 |
| Grid of 1,024 villages | not measured | 0.10 to 0.14 |

The lesson the trace gave (and the reason this design keeps the law untouched): every rule that flips at
a point needs a band or hysteresis, and an unbounded treasury keeps a world busy on its own. The sink is
what bounds the treasury.

**What E-sink changes.** Only where the coin goes. The law, the ceiling and the slack stay exactly as
they are, so every market sees the same trajectory, and round 2's quiet results hold by construction.

---

## 4. The design

### 4.1 The works purse: where coin leaves circulation

- **Each market gets a works purse per currency it sinks.** What E43 books as `sunk` on a currency
  line is credited to the purse instead.
- **The purse is out of circulation.** It never pays a flow, never pays a player, never counts toward the
  treasury's level, its news or its breakpoints. Nothing in the market reads it. So the coin has left
  circulation at the moment it crosses the ceiling, whether or not anything is ever bought with it, and
  the economy goes quiet exactly as it does today.
- **The purse is a closed form.** Within a regime the money line is one of the pieces `Line::plan`
  already lists; on the "sit at the ceiling, sinking" piece the sink per quantum is constant. So the
  purse at any time is its value at the last market event plus that constant times the quanta since:
  one multiply on read, nothing per tick.
- **Goods are unchanged.** E-sink is about coin. The goods slack (spoilage and plenty at the target)
  stays as E43 built it.

### 4.2 What the purse buys: the works catalogue

A **work** is one real, visible thing a village can pay for. Works are data, per culture, at
`data/<ns>/mic_rtsim/work/<id>.json` (the same pattern as P15's body kinds), so a pack or a plugin adds
works without code. Each names its kind, its culture, what it places, its cap, and its effect in the
sim. The first catalogue:

| Work | Culture | What appears in the world | Cap | Effect in the sim |
|---|---|---|---|---|
| Street lamp | each vanilla village type | its type's lamp from its `decor` pool: `plains_lamp_1` (3 × 4 × 3 blocks), `desert_lamp_1` (1 × 4 × 1), `savanna_lamp_post_01` (1 × 2 × 1), `snowy_lamp_post_01` to `_03` (3 × 4 × 1 to 3 × 4 × 3), `taiga_lamp_post_1` | PLACEHOLDER: one per 3 houses of the village | none: vanilla's own light rule lowers hostile spawns around it |
| Decor | taiga villages (the only type with decor templates besides lamps) | `taiga_decoration_1` to `_6` (from 1 × 1 × 1 to 3 × 2 × 6) | PLACEHOLDER: 4 per village | none |
| Ribbit lamp | ribbit | `ribbits:decor/lamp_post` (3 × 5 × 4), with processor list `ribbits:main` | PLACEHOLDER: one per 2 houses | none |
| Iron golem | vanilla villages | one `minecraft:iron_golem` at the meeting point, as vanilla's `village/common/iron_golem` template holds it | one per 5 villagers the census counts (vanilla's own gossip threshold, below), PLACEHOLDER | counts as a village defender against dragons (§4.6) |
| Tribute | any village a dragon raided | coin into the raiding dragon's lair chest | one tribute row per village | that dragon leaves the village's livestock alone while tribute is paid (§4.6) |

**What is deliberately not in the catalogue** (§5): houses, beds, workstations, bells, farms. Each of
those changes a vanilla village's population or what its villagers do, which is M3/M4's ground.

### 4.3 Prices: from the work's own blocks

- **A placed work's price** is the sum, over the template's blocks, of the anchor `p*` of each block's
  item at this market (labour cost relative to mining one coin, C13), plus placement labour per block
  (TUNABLE). Air, structure void and jigsaw blocks cost nothing. An item that is not one of the market's
  goods is costed through the same recipe graph `labour.rs` already solves from `RecipeManager`, so
  there are no price tables.
- **The template's blocks** come from Java once per template per session: a TEMPLATE_BLOCKS record of
  (item, count) pairs read from the template's first palette, count-checked like a sweep. A template's
  blocks never change within a session.
- **An iron golem's price** is the anchors of what vanilla builds one from: four iron blocks and a
  carved pumpkin (36 iron ingots and a pumpkin by their recipes), plus the same placement labour.
- **Tribute has no price.** It is a share of the purse each season (§4.6).
- **Why this rule.** A rich mining village pays little for stone decor and much for golems, a ribbit
  village near a geode pays in plentiful amethyst: local resources set the price, as C9 asks of every
  price.

### 4.4 When a work is bought, and when it appears

1. **Order.** Each village keeps one pointer into its catalogue: the next work it wants. The order is
   need first: tribute if a dragon raided it within the last season, then a golem if it was raided and
   is under its cap, then lamps and decor, cheapest first, ties by the village's seed. (TUNABLE; MS-6.)
2. **Booking.** When the next work becomes affordable, the engine computes the quantum at which the purse
   reaches its price (one division, §4.1) and books one wheel entry there. A market event that changes
   the sink rate rebooks it (lazy cancellation by stamp, O6).
3. **Buying.** At that moment the price leaves the purse and is booked as `works` (§4.5). The engine
   sends PLACE_HOUSE with the work's template, its pool's processors (none for vanilla decor, whose
   pools give an empty list; `ribbits:main` for Ribbits) and the SEARCH flag, as E56 sends a house (op id
   from the village and the work's number), or SPAWN_RESIDENT for a golem. The engine knows the op id is
   a work, so its HOUSE_PLACED queues no resident and steps no population.
4. **Appearing.** The bridge places it exactly as it places ribbit houses today: when every chunk of a
   free plot is loaded, a few layers a tick under the block quota (HouseBuilder's `BLOCK_QUOTA`,
   PLACEHOLDER 512 blocks a tick for all builds), never over a player's block or a block entity, and
   waiting while a mob or player stands where the next layers go. A player near the village watches it go
   up; a village nobody visits gets it the next time someone does.
5. **No plot.** HouseBuilder answers NOT_FREE after its ring search. The coin is already spent (the
   builders came and went); the engine marks that work kind full for the village (cap reached) and moves
   its pointer on. Nothing is refunded, so no event loops on a crowded village.

### 4.5 Where the coin ends up

The coin never comes back into any treasury. It ends in exactly one of four places, and the books say
which:

| End | When | Book term (per currency) | Physically |
|---|---|---|---|
| The purse | crossed the ceiling, not yet spent | `purse` (a holder, counted as seen) | nowhere: a number |
| Builders' wages and materials | a placed work or a golem is bought | `works` (a burn) | the work's blocks or the golem; never coin, so breaking a lamp post yields its own blocks, not emeralds |
| A dragon's hoard | tribute is paid | `tribute` in the world's books (a burn, as R5 books dragons' takings), `hoarded` in the dragons' ledger | the coin in a chest in the lair, once the lair loads (§4.6) |
| A player | a player robs the hoard | the dragons' ledger `robbed` (R22); the coin re-enters the economy only when the player trades it (`player_in`) | in the player's inventory |

Conservation per currency becomes: Σ treasuries + in transit + caravan cargo + purses = genesis + minted
+ player in − player out − destroyed − works − tribute. The dragons' own check (`Dragons::check`)
already composes hoards as hoarded minus robbed. Asserted after every event in debug, as today.

**Why not coin as blocks.** Placing the coin itself (emerald blocks in a village building) was
considered: it would make griefing a village a source of coin, which is the caravan robbery problem again
(research/caravan-balance-2026-10-20.md). It is MS-3's alternative.

### 4.6 Tribute and golems: the two works that change behaviour

**Tribute** (P12 dragons prey on village livestock; R24 a raid takes at most 5% of each herd good,
TUNABLE).
- **Who pays.** A village a dragon raided (a `Cause::Predator` event at that village) within the last
  season starts paying that dragon's lair. A season is 24 days (three Serene Seasons sub-seasons of 8
  days, the calendar P2 chose).
- **What.** Once a season, one event: the village pays a share of its purse (TUNABLE, PLACEHOLDER a
  quarter) into the lair's hoard, as the currency's item (emerald, or amethyst shard for ribbits;
  `rtsim-ecology`'s valuables already include any currency, R6).
- **What it buys.** While the tribute is current (paid within the last season), that lair's dragons
  give the village's livestock no utility in `weigh`, so they hunt elsewhere: caravans on roads, wild
  herds, other villages, the farms of players they hate. Paying shifts the danger onto the neighbours and
  the roads. That is the consequence a player sees: the caravans out of a tributary village get eaten
  instead.
- **Ends.** When the purse is empty at a payment, the tribute lapses and the village is prey again. When
  the dragon dies, its heir (C24) inherits the arrangement with the lair; a vacant lair takes no tribute.
- **Physical.** On the lair's next chunk load the plugin moves the paid coin into a chest inside the lair
  box with TRANSFER_TO_CONTAINER, one stack per command (B24), through the kernel's outbox so a crash
  never pays twice (J11). This is R23's deferred step, applied to tribute only. Caves have chests:
  `DragonCavePiece.setGoldPile` rolls each floor cell and places a chest on one roll in 99, with the
  cave's loot table (`chest/fire_dragon_male_cave` and so on). Whether roosts have any container is
  UNVERIFIED; a lair with none keeps its tribute abstract, as R23 keeps hoards today.
- **Robbing it** is already a theft the lair remembers (R22, Z16: HOARD_ROBBED counts what left a
  container inside the lair box).

**Iron golems.**
- Spawned by SPAWN_RESIDENT at the village's meeting point, with `isPlayerCreated` false, so vanilla
  treats it as the village's own: it attacks hostile mobs and any player whose reputation with a nearby
  villager is -100 or lower. Vanilla saves it like any resident (residents stay vanilla entities,
  PLAN.md §8.2). Vanilla's own golem rule keeps working beside it (`Villager.spawnGolemIfNeeded`: when
  5 villagers that slept in the last day gossip, or 3 panic, and none has seen a golem within 16 blocks in
  the last 599 ticks, one spawns). A bought golem also holds vanilla's off for as long as villagers see it.
- **In the sim:** the dragons' fear of a village's defenders (`village_risk`, PLACEHOLDER 15 fullness
  points, bent by boldness) grows with the golems the census counts (TUNABLE per golem). A rich village
  is a worse target, so a dragon goes for a poorer one.
- **Cap:** vanilla has none (the code above checks no count, beds or workstations). The work's cap
  borrows vanilla's gossip threshold: one bought golem per 5 villagers the census counts
  (PLACEHOLDER).

### 4.7 Foreign coin

E43 also sinks foreign coin above twice its holding target. That coin goes into the same market's purse,
in its own currency. Works are priced in home coin; foreign coin in the purse counts toward them at mint
parity (`L_mint(foreign) / L_mint(home)`, the anchor C13 already uses for exchange lines). Tribute is
paid in whichever coin the purse holds most of. No market event either way.

---

## 5. What this must not prejudge (M3, M4, C22, P17)

- **No population change.** No work adds a bed, a workstation or a villager. Vanilla villagers breed when
  a village has free beds and food, and take jobs at free workstations, so either would be growth by the
  back door. Bells are left out too: a second bell is a second meeting point for vanilla's villager AI.
  Golems are not villagers.
- **No change to the market.** Works never move targets, anchors, `T`, capacities or the culture. The
  economy of a village with ten lamp posts is the economy it had.
- **Ribbit houses stay P17's** (E56: prosperity for 10 days, PLACEHOLDER, books a house; at most 8,
  PLACEHOLDER). They are not bought from the purse, so this design does not change when ribbit villages
  grow (MS-4 asks whether they should be).
- **The purse is the hook M3/M4 can use.** A village whose catalogue is exhausted keeps filling its purse,
  out of circulation and at no cost (§4.1). When Ben decides how villages advance or found daughter
  villages, a large purse is a ready input: "this village can afford to advance" or "can afford to found
  one", without changing the sink.
- **C22:** nothing here depends on how a village came to exist. A village stays a market with a position
  and a culture; when the sim takes over structure generation, the works catalogue is one of the ways
  structures appear, not a separate system.

---

## 6. Walkthroughs

Names are for the reader; nothing in game shows them.

**A. A rich plains village lights its streets.**
1. You trade with Brindle every few days: you buy its bread and pay in emeralds you mined. Brindle exports
   wheat to its neighbours, who pay in emeralds too.
2. Brindle's treasury reaches twice its target. Before E-sink, everything over that line vanished. Now it
   goes into Brindle's purse, and its trades, prices and caravans behave exactly as before.
3. The purse reaches the price of a plains street lamp (its blocks costed at Brindle's prices, plus
   labour). Brindle books it.
4. You come back. As Brindle's chunks load, a lamp post from the plains decor set rises on a free spot
   near the square, a few layers a tick, never on your fences or your farm.
5. Over the next weeks more lamps appear, then decor pieces. Once its catalogue caps are reached Brindle
   stops building; the purse just grows, unseen, waiting for M3/M4.

**B. A raided village pays the dragon.**
1. Cinderjaw, a fire dragon roosting east of Oakford, raids Oakford in winter and takes cows (a market
   event: Oakford's leather and beef prices rise).
2. Oakford's next work is tribute. At the start of the next season it pays a quarter of its purse into
   Cinderjaw's hoard. Its cows are safe from Cinderjaw for a season.
3. Cinderjaw, still hungry, hunts the road between Oakford and Brindle instead. Caravans there lose
   cargo; Brindle's prices for Oakford's goods rise as fewer arrive.
4. You fly to the roost. Its chest holds Oakford's emeralds next to Ice and Fire's own treasure. You take
   them. Cinderjaw remembers the theft (R22). The emeralds are yours; they come back into the economy
   only when you trade them.
5. You kill Cinderjaw. Oakford's tribute lapses with no lair dragon to pay (the lair is vacant) and its
   purse fills again, and Oakford buys a golem.

**C. A ribbit village spends amethyst.**
1. Mirewater sits near an amethyst geode, so it mints plenty. Its amethyst line sinks into its purse.
2. Its purse buys Ribbits' own lamp posts (`decor/lamp_post`), placed with Ribbits' own processor list
   (mossy planks, pillars to the ground), as its houses are.
3. Its houses still come only from prosperity (P17); the purse never buys one.

---

## 7. Cost model

| Work | When | Cost | Grows with | Never with |
|---|---|---|---|---|
| Purse accrual | on read | one multiply from the line's plan | | elapsed time, coin amount |
| Purchase booking | at each market event that changes the sink rate | one division, one wheel entry (lazy cancel by stamp) | market events (already counted) | elapsed time |
| A purchase | once per work | one event, one PLACE_HOUSE or SPAWN_RESIDENT | works bought: at most the catalogue caps per village, ever | elapsed time |
| Tribute | once per paying village per season (24 days) | one event, one TRANSFER_TO_CONTAINER per stack when the lair loads | paying villages | coin amount |
| Template costing | once per template per session | one TEMPLATE_BLOCKS record; pricing is a sum over the template's distinct items | templates in the catalogue | villages |
| Placement (Java) | only while the work's chunks are loaded | HouseBuilder's: under `BLOCK_QUOTA` and `CANDIDATES_PER_TICK` (PLACEHOLDERS 512 blocks and 8 plot tests a tick, all builds together) | blocks placed | villages not visited |
| Memory | resident | per market: one purse value per currency it holds (8 bytes each), a catalogue pointer and a stamp | markets | works bought |

**Zero while unwatched:** an unvisited village's works are booked as engine events only; nothing is
placed and no chunk is loaded for it. A village whose catalogue is exhausted and pays no tribute costs
nothing at all: its purse is a closed form nobody reads.

**Events a settled world still has.** At most one event per work bought (bounded by the caps, so a
finite number per village over the world's life) and one per tributary village per season. Market
events are unchanged: these events touch only the purse and the works pointer, never a market line, so
they wake no market and send no news.

---

## 8. Tests before it ships

- **Quiet worlds unchanged:** `sim quiet` on the 11-node world at three geneses and the grids of 16 to
  1,024 (the memory's rule: judge on many worlds, days 400 to 800). Market events per market per day must
  be identical, event for event, to round 2's with the purse on, since nothing reads the purse.
- **A new `sim quiet` column:** works and tribute events per village per day, which must fall to the
  tribute rate once catalogues fill.
- **Conservation** with `purse`, `works` and `tribute` after every event (debug), and the closed-form
  proptest in `line.rs` extended: the purse equals the sum of stepped sinks.
- **GameTests:** a village whose purse reaches a lamp's price places it on a free plot and not on a
  player's block; a golem spawns at the meeting point and is not player-created; tribute coin appears in
  a chest inside a lair box on load, once across a crash.

---

## 9. Proposals for Ben

Each lists the recommended option first.

| # | Question | Recommended | Alternatives |
|---|---|---|---|
| MS-1 | Where coin leaves circulation | at E43's ceiling, twice the treasury target, into a works purse; the law is untouched, so round 2's quiet results hold | (b) at the top of the target's band (target plus an eighth): more coin to spend, but the treasury law changes and quiescence must be re-measured on every world; (c) at the target itself, same caveat and a treasury that never holds a buffer |
| MS-2 | The first catalogue | street lamps and taiga decor from each village type's own templates, Ribbits' lamp post, iron golems, and tribute to raiding dragons | (b) decor only; (c) also the decor pools' worldgen features (`pile_hay`, `pile_pumpkin`, `pile_melon`, `flower_plain`, `patch_berry_bush`, Ribbits' `veg_patch`), which need a small PLACE_FEATURE command; (d) also repair: re-place missing blocks of the village's own structure pieces, into air only, after a raid or a player breaks them |
| MS-3 | What the coin becomes | builders' wages and materials: it leaves the world as the work's blocks, never as coin | (b) part of each work is coin as blocks (emerald or amethyst blocks in the piece), lootable; (c) wages only, nothing physical but the work |
| MS-4 | Ribbit houses (P17) | unchanged: E56's prosperity trigger, not paid from the purse | (b) paid from the purse when E56 books one, so growth also waits for money (changes when villages grow) |
| MS-5 | Tribute | to the lair of a dragon that raided the village within the last season: a quarter of the purse (PLACEHOLDER) each season, and its dragons spare the village's livestock while it is current | (b) no tribute; (c) also to bandit camps and pirate havens once the conflict module has bodies, paid into their purse, never their treasury, so it stays out of circulation |
| MS-6 | Purchase order | need first (tribute, then a golem if raided), then lamps and decor cheapest first | (b) a seed-random order; (c) cheapest first across all kinds |
| MS-7 | Tribute made physical | placed into a chest inside the lair when the lair loads (R23's deferred step, tribute only) | (b) abstract, as R23 leaves hoards today: a robbed lair gives up only the structure's own treasure |
| MS-8 | Golems in the sim | each golem the census counts raises the dragons' `village_risk` for that village (TUNABLE per golem) | (b) flavour only: golems fight real dragons when watched and change nothing unwatched |
| MS-9 | An exhausted catalogue | the purse keeps growing, out of circulation, as the fund M3/M4 may spend later | (b) the excess is spent as wages and leaves the world (today's behaviour, with a different name); (c) raise the caps over time so there is always something to buy |
| MS-10 | Foreign coin in the purse | counts toward works at mint parity; tribute paid in the coin the purse holds most of | (b) foreign coin pays tribute only; (c) separate catalogues per currency |
| MS-11 | A work with no free plot | the coin stays spent and that work kind is marked full for the village | (b) refund the purse and retry after a season (an event per retry) |
