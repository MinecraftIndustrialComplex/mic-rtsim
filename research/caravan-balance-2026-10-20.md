# Caravan robbery balance, 2026-10-20

For Ben (DECISIONS.md, Review P3). Code at `d1e0808`. Numbers come from `rtsim-cli sim phase1`, rerun today (seeds 42, 7 and 1234 give the same cargo; `--no-script` as a control), which matches the committed `scenario/results/phase1-seed42-120d.json`. Vanilla facts were checked against the 1.21.1 jar. Every world number is still a PLACEHOLDER (V5, P9, P18).

## Verdict

Today, robbing a caravan is not the best way to get gear: the richest caravan holds 1 iron ingot and 1 or 2 emeralds. That comes from small placeholder flows, not from design. The consequences are weaker than PLAN.md walkthrough D promises, several loops cost almost nothing, and only small flows cap the loot.

## 1. What a robbed caravan yields

- **Capacity.** Two strength-1 llamas with 3 slots each (`LLAMA_SLOTS`), 6 stacks in all. A caravan leaves when full or after 48 hours (`dock_wait_quanta`). Only whole items go into chests (`stacks()`); fractions stay in the sim.
- **Measured.** No road moves 3 slots a day, so every caravan leaves on the 48-hour timer with two days of its road's flow. Of 2,215 daily traveller samples, 1,634 were empty scouts. Cargo caravans: median 0.022 slots, p90 0.13, max 0.70; 49% held no whole item. About 13% of travellers a player meets carry anything in a chest.

| Caravan | In the sim | In the chests |
|---|---|---|
| Day 20, Oakford to Brindle (robbed) | leather 1.97, iron 1.97, emerald 2.76 | 1 leather, 1 iron, 2 emeralds |
| Day 36, ribbit, Mirewater to Reedholm (robbed) | leather 0.67, amethyst 1.98 | 1 amethyst |
| Day 120 richest, Oakford to Brindle | iron 1.97, emerald 1.63, leather 0.43 | 1 iron, 1 emerald |
| Day 120 bulkiest, Brindle to Oakford | wheat 39.3, emerald 1.54 | 39 wheat, 1 emerald |

- **Peaks over 120 days**, one good on one caravan: iron 1.97, emerald 4.46, leather 3.94, amethyst 22, wheat 71. The nine-village world moves 0.98 iron a day, on one road.
- **Money.** Payments ride with the goods (`Dir::pay`): 0.5 to 4.5 emeralds owed for goods going the other way. The purse from player trades also goes into the last llama's chest (`spawn_caravan`).
- **Native catalogue** (`catalog.rs`; UNVERIFIED, not run): the richest iron village (snowy) makes 4 iron a day and a desert mints 2 emeralds a day, so one caravan holds at most about 8 iron or 4 new emeralds.
- **How often.** Each road direction sends one cargo caravan every 48 hours or an empty scout every 24. Camping one road: about one cargo caravan per game day (20 real minutes). At a hub like Brindle (8 roads), about eight.

## 2. What it is worth in MIC

- `remove_metals_from_chests.js` strips ingots, nuggets, gems, raw materials, ores and storage blocks from village, shipwreck and buried-treasure chests. Caravan chests put `iron_ingot` and `emerald` back as loot: the category you removed.
- **Iron.** Overgeared needs 3 ingots for a pickaxe head, 2 for a sword, 24 plates for armour, plus anvil, hammer and heat. The only iron road gives 1 ingot every 2 game days: a pickaxe in 6 days, armour in 48 (16 real hours). One cave vein beats it. It skips the caves' danger, not the forge.
- **Emeralds.** `disable_villager_trading.js` removed the armorer, toolsmith, weaponsmith and librarian; no pack recipe uses emeralds. What is left (fletcher bows, cleric pearls and XP bottles, leatherworker) skips no Overgeared or Create step.
- **Amethyst** buys Ribbits' decorative blocks, fish and an enchanted fishing rod (`RibbitTradeModule`).
- **The rest** (leather, wool, wheat, bread, clay, slime) is food and basics.

Nothing in today's catalogue skips a progression step. The risk is the catalogue P9 and P18 will write: the ceiling is the smaller of 6 stacks and two days of a road's flow, so one caravan could carry 384 of a tool part or Create component.

## 3. The real cost

| Act | Report | Sentiment (seen / heard) | Vanilla major negative (seen / heard) | Report lives |
|---|---|---|---|---|
| Kill a llama | Killing | -125 / -62 | 25 / 12 | 15 days |
| Lead a llama away | Theft | -50 / -25 | 25 / 12 | 1.5 days |
| Kill the trader | Killing; grudge passes straight to its home (`Knowledge::die`) | -125 at home | none at home | 15 days |

"Seen" means the caravan, its trader, or a village within 32 blocks.

- **Vanilla villages.** The sim's quote stays fair there (`World::markup`); only gossip moves prices. Golems need reputation -100 or lower, so major negative 20 or more: only a robbery inside a village, or two heard reports in a day, and for one day (decay 10 a day; `GossipWriter.aged` ages it for unloaded villagers too). Heard news (reputation -60) is gone within two days. Walkthrough D ("later you visit Oakford") fails if later is more than two days.
- **Caravans from the victim's home** charge x1.49 at -125 (mean half-life about a year), x1.24 at -62 (gone in about 56 days), x1.20 at -50 (31 days). On one-item cargoes this costs little.
- **Ribbit villages** apply the same markup to their own trades and withhold sorcerer buffs at -25 or below. Measured: Mirewater went from -50 to -46 in nine days.
- **Access** is never lost: no sentiment refuses a trade.
- **Reach.** The human robbery reached 3 of the 10 other nodes, the ribbit one 6, within 1.4 days; then both expired. Brindle, the robbed caravan's destination, never heard. Oakford was back to 0 by day 120. P18's agreed longer lifetimes are not in `rtsim_knowledge::Tunables` yet. A killing's reach was not run (UNVERIFIED).
- **Vanilla baseline.** A wandering trader spawns with a 2.5 to 7.5% chance a day; killing it drops two leads, its llamas 0 to 2 leather each, no chests. Sim caravans add a chest per loaded llama and their cargo, and pass far more often.

## 4. Exploit loops

1. **Kill without a killer.** `BridgeEvents.onDeath` reports `getSource().getEntity()`. A llama killed by lava, a fall, a cactus or fire has no killer: `lose_member` books no report, and the chest still drops. A tamed wolf's kill is booked against the wolf. Verified in code, UNVERIFIED in game.
2. **Laundering.** Every trade, of any size, adds +2 (`World::trade`, `trade_exact`). 25 trades erase a robbery, 63 a killing; the scenario's wheat dump made 14 in one visit.
3. **Trade, then kill.** `PuppetOffers` puts every payment into the cargo. Buy, kill the llama, take the coin back; or sell, kill, take the goods back.
4. **Far from home.** Robbery and theft reports live 1.5 days, one to three hops. Three hops out, no home village hears.
5. **Kill the whole caravan.** Inheritance writes no vanilla gossip, so the home's golems and villagers never react.
6. **Farm a road.** Roads ignore hazard (`price_lane_hazard` covers ships only); the robbed destination receives less and keeps asking. Heard gossip adds 12 per kill and decays 20 between caravans, so it never builds up.
7. **Spare the trader.** Caravan offers use the home's sentiment (`caravan_markup`), not the caravan's own: an attacked caravan trades fair until it gets home.

## 5. Recommendations

**Top three**

1. **Coin and gated goods stay abstract.** Purse and payments never go into chests (`spawn_caravan`: drop the purse `stacks` call; `stacks()` skips `GoodKind::Currency`); a robbery takes a share of the coin in the books. Physical cargo follows `remove_metals_from_chests.js`: no ingots, nuggets or gems in chests (a `physical` flag on `GoodDef`, data in `catalog.rs`).
2. **Every death has a killer.** Use `getKillCredit()` in `BridgeEvents.onDeath` (the last player to hurt it, through projectiles and tamed animals too). Book an unattributed death with a player within 32 blocks as a suspicion report (`lose_member`).
3. **Consequences scale with value and cannot be laundered.** Severity from what was taken against the home treasury (`rob`, `kill_member`, `take_member`; now `SEVERITY_UNIT`). A trade's +2 scaled by value and capped per village per day (`Tunables::delta`, `World::trade`). Inheritance writes the home's vanilla gossip (`Knowledge::die`). Robbery lifetimes lengthened as P18 agreed (`Tunables::lifetime`). A refusal threshold (a new `WorldConfig::refuse_at_or_below` beside `markup_worst`).

**Others**

4. **Cargo by tier.** A tier per good, and what materializes capped by the player's progression (`GoodDef`, `spawn_caravan`): early roads carry food, wool and clay.
5. **Cap per caravan.** At most N whole items of any good in chests (`stacks()`), so 6 x 64 is never reached.
6. **Loot that degrades.** Sealed crates that rot to junk if not delivered within days (a new item with a timestamp component; pack data).
7. **Worthless off the network.** Bills of lading or promissory notes naming the home village, cashed only by its market, or fenced at a haven at a discount (C24).
8. **Defences later (C12).** Guards sized by cargo value; extend `price_lane_hazard` to roads, so a farmed road sends fewer, guarded or rerouted caravans.
9. **Caravan memory.** `caravan_markup` also reads the caravan's own sentiment.
10. **Rerun this report** when P9 and P18 replace the V5 catalogue.
