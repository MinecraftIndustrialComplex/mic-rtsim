# Overnight choices: phase 1 engine modules

Reversible choices made on the night of 2026-10-20 while building the phase 1 engine modules
(`rtsim-settle`, `rtsim-trade`, `rtsim-travel`, `rtsim-knowledge`, `rtsim-world` and `rtsim-cli sim
phase1`), each with the reason. Ben reviews them; none is final, and each can be undone. They are
numbered E1, E2 and so on, apart from DECISIONS.md, which is not edited tonight. E29 to E42 were made
by the sub-agent that built `rtsim-knowledge` (its K1 to K14). E43 onward are round 2's (branch
`overnight/phase1b`): quiescence, caravan batching, the remaining phase 1 features and memory.

Tunables are PLACEHOLDERS unless a decision fixes them. Where PLAN.md §17 recommends a value, the
recommendation is taken and named here.

## Structure

| # | Choice | Why |
|---|---|---|
| E1 | A fifth crate, `rtsim-world`, composes settle, trade, travel and knowledge, owns the step loop and the calls the bridge will make (trade, offers, travellers, promote, rob, rescue, join). | PLAN.md §5.4's module registry and scheduler do not exist yet; something has to wire the modules, and a library is what `rtsim-jni` can call. Delete or slim it when the registry lands. |
| E2 | Edges: PLAN.md §17's P18 recommendation, Veloren's rule (each settlement links to its 4 nearest within 400 chunks, the union of both ends' choices). Path length is the straight line times a PLACEHOLDER factor of 1.0. A village that joins later (C22) adds its own nearest and becomes a neighbour of those that now count it among their nearest; existing edges are never dropped. | P18 is open and this is its recommendation. Dropping edges on a join would cut flows already on a road because a player explored. |

## Economy (C13, PLAN.md §8.4)

| # | Choice | Why |
|---|---|---|
| E3 | Conserved amounts are integers in thousandths of an item or coin; prices are millionths of a coin per item. Players and the bridge move whole items, which are 1,000 amounts. | A village eating 20 loaves a day eats 833 thousandths an hour, and a flow worth 2.5 emeralds a day is exact. Whole items alone would make most flows round to zero. |
| E4 | Stocks move only at quantum boundaries, every PLACEHOLDER 1,000 ticks (an in-game hour), with all rates whole thousandths per quantum. Events change rates for the boundaries after them. | Every market trajectory is then exact integer arithmetic with a closed form, so catch-up over any span is O(goods) and composes bit for bit (tested). A day quantum would make every road at least a day long. |
| E5 | `T`'s lower bound `2 × max d` is taken per market, over its own edges, not over the whole world. | The bound exists so a market's band between target and twice the target absorbs its own news lag; a far road elsewhere does not lag its news. |
| E6 | Money follows the same per-quantum law as goods, with `Mbar` as the target and the mint as capacity: below target it mints at capacity, at target it mints exactly what it pays out, above it mints nothing. Foreign currency is an exchange line with no capacity. | One law, one closed form. "Mines while M < Mbar" read literally (on, off) chatters at `Mbar`; matching at the target does not. |
| E7 | Sentiment never makes a quote better than fair: a disliked player pays more and is paid less, a liked one gets the fair price. Only ribbit villages apply the sim's markup; vanilla villages leave reputation to vanilla gossip (C20). Worst markup PLACEHOLDER 1/2 at sentiment -128. | A discount for liked players could be farmed into a money pump (trade fairly to be liked, then buy cheap and sell dear). Vanilla already reprices for reputation, so the sim would count it twice. |
| E8 | Exportable rate: nothing below the target; at it, capacity plus inflow minus consumption; above it, plus a drain of the surplus over `T`. Advertised rate (news): signed, consumption plus outflow minus capacity plus `(target - stock) / T`, minus inflow; rounded toward zero to the news resolution; above twice the target, or with nothing to pay with, it asks for all its inflow to stop. An exporter sends what it was landing there when the news was written plus the advertised change. | The research says "up to j's advertised absorbable rate (need minus inbound)" without saying what need is. A signed rate lets a glutted destination ask for less; the two-market test found that a rate clamped at zero locks into a limit cycle. |
| E9 | A culture with no mint anywhere in reach uses its mint recipe's notional cost (labour over yield, ignoring the resource) as `L_mint`. | Prices must stay defined in a world that happens to have no emerald ore; the notional cost is still labour, not an invented number. |
| E14 | A margin of exactly zero keeps a flow, after a rounding slack of 2 millionths (three integer prices, each rounded once; not a tunable). | At steady state an importer that cannot make a good sits at its target with an anchor of exactly the exporter's plus transport. A strict test stops that flow and restarts it forever. |
| E15 | A village joining later (C22) gets its costs from its own recipes plus its new neighbours' cached costs and transport. Existing markets keep their anchors and their `T`. | Repricing the whole world on every join would make one exploration step move every anchor. Known gap: a longer new edge than a neighbour had does not raise that neighbour's `T` bound until anchors are recomputed. |
| E16 | `L_mint` is the culture's cheapest mint cost, the same in every market of that culture. | An emerald is the same labour wherever it is, so prices in emeralds compare across markets and a steady flow's margin is exactly zero. With a per-market `L_mint` the same emerald would be worth different labour in neighbouring villages. |
| E17 | News resolution: the advertised rate is rounded to a step of the market's consumption over 8 (PLACEHOLDER), at least one item a day. | Without a dead band every refill step is a new advertisement, and a closed world never goes quiet (measured: 1,600 events a day in an 11-market world before, about 100 after). |
| E18 | Flows are gated on the destination's bid: its price, but never below its anchor. Above its target a destination cuts supply through its negative advertised rate, not by a margin cliff. | The two-market test measured a bang-bang limit cycle: a hair above target the price dipped below the anchor, the margin went negative, the flow stopped dead, the destination drained, and it restarted. With the floor, the first overshoot decays and the world goes quiet (43 events in 80 days). |
| E19 | Payment: the importer pays at arrival, at the quote the exporter set when the goods left (the destination's news price), in the exporter's currency if it holds some, else in its own. | Gold points form: a border market that piles up foreign coin spends it on imports from that culture, and its exchange price falls within the transport band until it does. |
| E20 | Coverage: after every market event, any line whose next step would take out more than it holds has its outflow (exports and payments alike) cut to the sustainable rate (inflow plus capacity minus consumption). A line one step from that books a breakpoint before it. | No stock or treasury ever goes negative (no debt), and pipelines always carry what the market really sent, which conservation rests on. |
| E21 | Barter in flows: a market that refuses the cash it would be paid in (above twice its target), or a buyer that has no cash, settles at anchor ratios in goods instead. The payee takes any good the payer has spare that it can still store, preferring goods it needs, then the one it values most. Barter at anchor ratios is also offered to players. | PLAN.md §8.4 says a cash-refusing market barters. Measured in the scenario: without barter in flows, a village with a structural surplus (Brindle) crossed twice its cash target by day 60 and its neighbours starved; with it, most villages stay fed through 120 days. |
| E22 | Recipe coupling: a good's consumption includes what this market's own recipes take; an input with an empty store limits its recipe to what flows in; inputs are fed before households. A change of production regime is a breakpoint for lines another recipe uses. | Keeps every line's rates constant between events, as the closed form needs, while inputs and outputs stay consistent. |
| E23 | Player trades are positive Trade reports the market witnesses (PLAN.md §8.7: "a fair trade raises sentiment"). | It is the table's "fair trade" row. Vanilla already writes its own trading gossip, so the knowledge crate skips first-hand trades at vanilla villages (E36). |

## Travel and caravans (SCALE.md §3, PLAN.md §8.6)

| # | Choice | Why |
|---|---|---|
| E10 | Caravan period: chest slots per caravan over the direction's volume in slots, at least 1 and at most the travel time; any flow at all, payments included, counts as active. An idle road has scouts every PLACEHOLDER day that carry only news. | Caravans per day = goods rate over capacity (PLAN.md §8.4). The cap at the travel time keeps every caravan's window on the road while it walks, so robbing it cuts exactly its consignments. Scouts make news cross an idle road at the speed of an occasional traveller, not instantly and not never. Known cost: a small flow makes small caravans (see incomplete work). |
| E11 | A market sends news when its snapshot differs in kind (any good's level or advertised rate, whether it can pay, whether it refuses cash), at its breakpoints, and after a player trade. Price alone does not trigger news. | "Each breakpoint emits one delayed news event per incident edge" (PLAN.md §8.4), plus the changes allocation reads. A price moves with every unit of stock; sending it every time would be the live price board Gintis warns against. |
| E12 | Groups (C25): a human caravan is one wandering trader and two trader llamas; a ribbit caravan is one ribbit merchant (count PLACEHOLDER, P18) that carries the cargo itself (P31's recommendation). Llamas fill chest by chest at 3 slots each (strength 1, PLAN.md §8.4); overflow rides with the last carrier. | C21 puts cargo in the llama chests; P31 is open and this is its recommendation. |
| E13 | The leg index's time window is one quantum (PLACEHOLDER); regions are 512 blocks (PLAN.md §5.2). | Matches the accrual grid; fine enough that a crossing booked in a window is near its real time. |
| E24 | A news packet waits for the next caravan on its road and merges with a packet already waiting for it; it carries the market's snapshot, up to 8 relayed snapshots it received since the last packet on that road (PLACEHOLDER), and the gossip reports its board gained. A market keeps the freshest snapshot of up to 16 markets (PLACEHOLDER), never evicting its neighbours. | Prices and gossip both move only with travellers, and a burst of changes before a caravan leaves costs one packet. |
| E25 | Something happening to a sampled caravan makes it an individual row: its window's consignments still on the road are cut from every pipeline of its direction and become its cargo; its trader becomes a notable whose heir is its home market. After an attack a trader with boldness of 128 or more (the middle of the range) goes on, else it turns back home (C25). On arrival an individual caravan deposits the payments it carried, sells its goods at the quote it left with as far as the treasury pays, and walks home with the proceeds and whatever did not sell. A group left with no leader breaks up, and what it carried leaves the sim with its animals. | Totals stay exact (tested under random scripts), the robbery report travels with the trader, and C24's "a notable trader killed on the road leaves its grudge with its home market" holds (tested). |
| E26 | Loot from a robbed caravan, a stolen llama's chest, and a killed member's dropped cargo are booked as "player out": they leave the sim's totals for Minecraft. | They become ordinary items and entities in the world (A10, C21), outside the sim's ledger. |
| E27 | Predator havens (C24): a raided road sends a share of every consignment (PLACEHOLDER 1/8 in the scenario) to the haven's market, walking at caravan speed; the haven is a storehouse market of its culture with no vanilla gossip, and pays nothing for its haul. | Stolen goods stay in the world's totals and are sold where the raiders live, as C24 says. |
| E28 | Player bases (C26) are market nodes with no storehouse: no stock, treasury or production; their news carries the last price players traded each good at there. | PLAN.md §8.2: "a base is not a storehouse market". What counts as a base is still P30. |

## Knowledge (made by the knowledge sub-agent; its K1 to K14)

| # | Choice | Why |
|---|---|---|
| E29 | "Already heard" is decided by the report table's knower list (6 bytes per knower), not by the noticeboard. A full board drops its oldest entry, and the holder never relearns that report as news. The knower list is also how expiry finds whose counters to bump and whose boards to clean. | A capped board alone would let an old report come back as fresh news. |
| E30 | A report evicted at the cap folds into counters exactly as an expired one does; a new report less important than everything resident is dropped. | Eviction is by importance (SCALE.md §6), never the strongest first. |
| E31 | Sentiment decay waits are exponential with mean `base × (1 + f × (m/127)²)`, PLACEHOLDER base 2,400 ticks and f 99: about half a day at magnitude 5 and about 18 days at 40. One anchor per holder; draws keyed (world seed, `knowledge.sentiment`, holder) with the slot mixing target and anchor; a projection costs at most 127 draws per entry. | Slower when strong, as Veloren's, but lazy and read-order independent (tested). |
| E32 | Lifetimes: killings 15 days and theft and robbery 1.5 (P18's recommendation, Veloren's); assault and rescue 5, trade 1.5 (PLACEHOLDER). Deltas on vanilla's gossip scale: assault -25, killing -125, robbery and theft -50, trade +2, rescue +25 (PLACEHOLDER). | P18. |
| E33 | Severity scales a delta: `base × severity / 100`. | One knob per report instead of one per kind and severity. |
| E34 | Hearsay counts 50% of first-hand, for sentiment, vanilla gossip amounts and memory salience (PLACEHOLDER). | News from a traveller should weigh less than what a village saw. |
| E35 | An heir inheriting a feeling of the same sign keeps the stronger; opposite signs add. Inherited memories go on the heir's board without a second delta. | A home market that already heard of its trader's robbery does not count it twice. |
| E36 | Vanilla gossip mapping (PLACEHOLDER, P18): assault minor negative 25; killing, robbery and theft major negative 25; trade trading 2; rescue minor positive 25. First-hand resident assault, resident killing and trade at a vanilla village are skipped (vanilla writes those itself); the same news from another village is written. | C20 and PLAN.md §8.7: write only what vanilla cannot know, and "news from another village" is one of those. |
| E37 | Notables remember what they learn, salience = severity (halved for hearsay); a full memory drops the least salient, oldest first among equals. | SCALE.md §9's memory slots. |
| E38 | The counter table is capped and drops the smallest count first. | SCALE.md §6: every table has a cap. |
| E39 | A packet holds at most 64 reports (PLACEHOLDER); the cursor moves only past what was taken, so the rest goes with the next traveller. | Bounded packets; nothing is lost. |
| E40 | Caps (PLACEHOLDER): sentiment entries 64 for villages, 16 for bases, caravans and notables (SCALE.md §9 for notables); board entries 256 for villages, 64 for the rest; report table and counters 2^20 each; heirs inherit 4 memories and 4 feelings; witness radius 32 blocks (Veloren's). | SCALE.md §6, §9; P18. |
| E41 | A notable's actor is registered with its seed as the external id (seeds must be unique) and removed at death. | Identity is derived from the seed (PLAN.md §4.4). |
| E42 | Each temperament trait is a byte from one Philox block keyed by the seed; `bend` moves a utility by up to plus or minus a percent (PLACEHOLDER 50). | C25: temperament costs no memory. |

## Round 2: quiescence (C13, P9; economy research failure table "quiescence")

Round 1's closed world never went quiet: 36 market events per market per day in the 11-node world,
15 to 20 in the bench's grids. A trace of every market event (`rtsim-cli sim phase1 --trace D`, `sim
quiet --trace D`, `World::enable_trace`) showed no single cause but a family of knife edges: a
decision that flips when a quantity crosses a point, and a feedback that pushes the quantity back
across it. Each choice below removes one; the toggles live in `rtsim_trade::Rules` so every
alternative can be rerun (`--rules round1`, `--rules up=1,barter=1` and so on). `r`, `T` and `k` are
untouched: they stay P9.

| # | Choice | Why |
|---|---|---|
| E43 | **Sinks in the line law.** Above a ceiling a line loses up to a slack per quantum, the mirror of production below the target: `sink = clamp(pre - ceiling, 0, slack)`, ledgered as `sunk` (conservation stays exact, tested by the closed-form proptest and every world test). **Money:** the ceiling is twice the treasury target and the slack unlimited: coin beyond it is spent on upkeep and leaves the world (foreign coin above twice its holding target likewise). **Goods:** the ceiling is the target and the slack two news steps (PLACEHOLDER): a village with plenty eats a little more and lets a little spoil, so the residue of rounding sits at the target instead of creeping away. | Nothing but players removed money, so surplus villages piled coin until they refused cash, and barter loops followed. The sink is the smallest change at the design's own threshold: where PLAN.md §8.4 has a market refuse cash above `2 Mbar`, it now spends the excess instead. Without it (`money=0`) the 11-node world sat at 2.1 events per market per day forever; without the goods slack (`slack=0`) at 1.1. |
| E44 | **Levels are bands.** `At` is the target plus or minus an eighth of it (PLACEHOLDER, `BAND_DIV`), `Empty` is up to a quarter of that band. Breakpoints and news happen at band edges. A recipe is held back by an input below its band (by what flows in, rates only), not by an input at exactly zero. | `At` was a single point, so any oversupply made a line `Above` and any shortfall `Below`, a breakpoint and news each time; a starved store flickered between zero and a few thousandths. |
| E45 | **An exporter in its band.** It exports from the bottom of its band, sending less by `(target - stock) / T` below the target so it refills; its ask is its price but never above its anchor (the mirror of E18's bid floor); flows into a destination within or above its band are quoted at the destination's anchor; what it can spare is rounded to its news step; the news step comes from the target (`target / T / 8`, at least an item a day), not from the consumption of the moment. | Exports stopped dead a thousandth below the target and restarted at it; a bakery's wheat step moved with its bread exports, so its advertisement flickered between two steps; every news re-quoted every flow by a few millionths. |
| E46 | **Suppliers share an advertisement.** An increase is split evenly among the neighbours that could supply (counted in the snapshot); a cut falls on the current suppliers in proportion to what each sends. | Three suppliers each answering the whole of "send 41 more" sent 123. Off (`split=0`) the 11-node world settled later and the grids worse. |
| E47 | **What is news.** An advertisement never asks to cut more than arrives (a producer that imports nothing advertises zero, however its exports move); for a currency line only "holds any" and "refuses cash" are news, and its breakpoints are only twice its target and the bottom of its band. | Producers' negative advertisements and treasuries drifting between levels sent news nobody acts on, and every news is a delivery event at every neighbour. |
| E48 | **No barter in flows (E21 off by default); stickiness off; net changes only.** A payer short of cash pays what it can and the coverage rule (E20) cuts the rest. A small change of a rate (`sticky=8`: under an eighth) can be held, but by default is not. Within one event a pipeline may be set twice (allocation, then coverage); only the net change wakes its destination. | With the money sink no market refuses cash from accumulation, which was E21's reason. Barter payments drained goods below target and flipped between currencies and goods: turning it off took the 11-node world from 12.9 to 4.8 events per market per day at the time it was tested. Stickiness helped one world and hurt others (measured: the 16-village grid with `sticky=8` 7.5 against 7.3, the 64-village grid 9.2 against 10.6); off is simpler. |
| E49 | **Rationing.** When a market cannot spare what its destinations want, it shares what it can spare in proportion to their wants (exactly: the rounding remainder goes to the biggest want), instead of best margin first. Negative margins still get nothing. | Two importers short of one exporter's goods took everything in turn: the shorter by the last news won all, filled, lost it to the other. With `ration=0` the 11-node world sat at 3.4 events per market per day. Departs from PLAN.md §8.4's "best first" when short. |
| E50 | **A treasury below its band pays out only what comes in.** Exactly (the remainder of the proportional cut goes to the biggest payment), and it stays cut until it is back at its target. | A broke market's treasury drained to zero, was cut, refilled by a thousandth of rounding, paid in full again: a cash ping-pong between broke neighbours that never ended. |
| E51 | **Hysteresis.** What a market asks to receive in all (its need) holds its last advertised total while it moves by less than a news step; what an exporter can spare holds its current outflow while it moves by less than a step; a recipe held back by a short input stays held back until the input is back at its target; a supplier already sending keeps its margin while the destination is anywhere in its band (its bid as if it stood at the band's bottom). | Each removed a two-state limit cycle the trace showed: an advertisement flipping between two steps as a stock crossed a rounding edge (and, held on the change instead of the total, a standing "send 41 less" read twice), a bakery's capacity flipping 282/283 under a quantized export, a dearer second supplier called in by every shortage and dropped by its end. |
| E52 | **Budget-limited demand: built, off.** A market whose treasury is below its band and owes more than it takes in asks for nothing more (`budget=1`). | The economically right answer to broke importers, but it made the grids worse (64 villages, seed 7: 1.60 against 0.01 events per market per day over days 400 to 800). Left for P9's soak. |
| E62 | **Exports in recipe order** (a round 1 bug): a market allocates a good before the inputs of its recipe, so an input's consumption already counts what the outputs' exports make the recipe use. | In id order a bakery exported wheat as if it baked only for itself, then baked for its bread exports too, fell below target and stopped: a two-quantum cycle in every bakery of the grids. |

**Result.** Events per market per day (closed worlds, no player; `rtsim-cli sim quiet`, windows of
100 days; the scripted scenario's last 10 days):

| World | Round 1 | Round 2 |
|---|---|---|
| Scripted 120-day scenario, days 110 to 120 | 34.3 (377 a day) | 0.65 (7 a day) |
| Closed 11-node world | 36.5 at days 230 to 240, flat | 2.70, 0.66, 0.44, 0.10, then 0.00 from day 400 to 1,000 (0.01 in two windows: a house booked) |
| The same world grown by joins (C22) | not measured | 3.15, then 0.00 from day 100 |
| Grid of 16 villages | 14 to 17 (round 1 rules) | 1.19, then 0.02 to 0.06 |
| Grid of 64 | 17 to 19 | 1.79, falling to 0.48 by days 700 to 800 |
| Grid of 256 | not measured | 1.23, falling to 0.11 |
| Grid of 1,024 | not measured | 1.29, falling to 0.09 |

Conservation held after every window and, in the tests, after every event. Tests:
`rtsim-world/tests/quiet.rs` (the scenario world is silent over days 500 to 700; round 1's rules stay
above 10; a grid falls below a tenth of its early rate), `tests/join.rs` (a grown world), and the
closed-form proptest with sinks in `rtsim-trade/src/line.rs`.

**Alternatives tried** (single toggles on the round 2 rules as they stood when tried; the system is
sensitive, so one run per world is a sample, not a law):
- No money sink: never quiet (2.1 to 2.7 events per market per day, steady).
- A holding target with budget-limited demand (E52) instead of nothing: worse on grids.
- Advertisements rounded up instead of toward zero (meant to leave only oversupply for the slack):
  10 events per market per day; a step bigger than a small good's consumption kept flipping 0 and 41.
- Stickiness at an eighth or a sixteenth: better on some worlds, worse on others; off.
- Barter in flows: 9 to 20 events per market per day.
- Best-first allocation when short: 3.4.

**Residuals.** The 64-village grid still has a slow churn (0.5 events per market per day at day 800,
falling); the trace there shows payments among broke ribbit villages and secondary suppliers. Growth
(P17) and bands (P16) perturb a settled world by design: a house booked is a population step, so a
world quiet for 300 days has an event when a village grows.

## Round 2: caravans (PLAN.md §8.4, §8.6; SCALE.md §3)

| # | Choice | Why |
|---|---|---|
| E53 | **Dock wait.** A caravan leaves when the dock has filled it (its slots over the road's volume, rounded up) or when its oldest load has waited the longest dock wait, PLACEHOLDER two days (`Tunables::dock_wait_quanta`, 48 quanta). Every flow's delay is that wait plus the walk (`Edge::lag`), so a caravan's whole window is in transit from departure to arrival and its load never shrinks on the road; what the pipelines count in transit is exactly the origin's dock, the caravans on the road and the destination's dock (tested: `tests/caravans.rs`). `T`'s lower bound is `2 × (walk + dock wait)`. Human capacity is 2 llamas × 3 × strength slots (6 at strength 1); a ribbit caravan is its merchant carrying the cargo itself (P31's recommendation), PLACEHOLDER 6 slots (`ribbit_caravan_slots`). Scouts on idle roads never wait longer than the dock wait. | Round 1 capped the caravan period at the travel time, so a small flow made caravans of a few quanta of flow, and a caravan's early consignments "arrived" while it was still walking. Batching only makes bigger caravans if the flow waits for them; making the wait part of the delay keeps the flow model and the sampled travellers the same thing. |

**Caravan sizes** (every traveller with cargo on any road, sampled daily after day 20 of the scripted
120-day scenario; slots are items over stack size, coin included):

| | Round 1 rule (period at most the walk) | Dock wait |
|---|---|---|
| Median load | 0.001 slots | 0.020 slots |
| 90th percentile | 0.021 | 0.49 |
| Largest | 0.12 | 0.83 |
| Carrying nothing over one item | 88% | 50% |
| Median of its biggest good | 0.04 items | 0.93 items |
| The robbed caravan of walkthrough D | wheat 0.53, leather 0.12, emerald 0.01 | leather 1.97, iron 1.97, emerald 2.46 |

On a busy road (`busy_roads_send_full_caravans`: 1,500 people fed by one farm) every caravan leaves
full, at 6.6 slots. The scenario's own flows are small (most roads carry a few items a day), so its
caravans stay under one slot: caravan size is bounded by flow times the dock wait, and a longer wait
raises `T`'s floor. **Known cost:** news rides caravans, so on a quiet road it waits up to the dock
wait; with robbery reports living 1.5 days (P18, Veloren's), a robbery now reaches 3 to 7 villages of
the scenario before it expires instead of 8. Review with P18's lifetimes.

## Round 2: the remaining phase 1 features

| # | Choice | Why |
|---|---|---|
| E54 | A village that joins later (C22) and makes an existing market a border market opens that market's exchange line for its currency, at mint parity within the gold points of the new edge. `World::join` is tested one village at a time, in any order (proptest), with conservation after every event. | Without it the existing market's line for the newcomer's coin had target zero, so with E43's sink every coin it was paid in vanished at once. E15's other gap (an existing market's `T` bound) stays. |
| E55 | **Sorcerers** (PLAN.md §8.8): `World::sentiment_snapshot(node)` is the SENTIMENT command of the Java side's boundary (`docs/boundary.md` §4.2 on `overnight/phase1-mc`): the village, `dislikeAtOrBelow` PLACEHOLDER -25 (an assault, or a robbery heard as news), `friendAtOrAbove` PLACEHOLDER 25, and every player it feels anything about, by UUID (`World::player_uuid(most, least)`). A ribbit village whose feelings may have changed (a delivery, a witness, a trade, an heir) gets a `Command::Sentiment` with the next commands; `sorcerer_includes` and `sorcerer_filter` answer for one player or a list. A village that witnesses a report now tells the next travellers at once (it used to wait for its next news). | P18 thresholds; the boundary record already exists on the Java side. Without the immediate gossip a robbery seen from a quiet village never left it before expiring. |
| E56 | **Growth** (P17's recommendation): a ribbit village whose treasury and the stocks its people need are at or above their bands (a needed good it has no way to get, none in store, made or coming, does not hold growth back) for 10 days (PLACEHOLDER) books one house: a `PlaceHouse` command (one of the 14 templates by seed, an engine op id, its meeting point to search from), and its population steps up by one (PLACEHOLDER) at that moment: households, stock targets and `Mbar` follow (`build::repopulate`), anchors and `T` do not. HOUSE_PLACED (`World::house_placed`) queues a `SpawnResident`. At most 8 houses per village (PLACEHOLDER). | P17's recommendation as written. Ribbit villages never have iron, so "every needed good" would mean no ribbit village ever grows. |
| E57 | **Touring bands** (P16's recommendation): every ribbit village of 8 or more (PLACEHOLDER) with a ribbit neighbour has one band of 3 (PLACEHOLDER: a master and two musicians, one puppet group, C25). Its circuit goes from home to the nearest ribbit village not yet on it, up to 3 stops (PLACEHOLDER), and back; it stays 6 quanta at each stop (P18's stay window, PLACEHOLDER) and walks at caravan speed. Where it is is a closed form of time (`band_view`, `bands_at` for "play now"); it carries news like a caravan, as packets that wait for its next departure from a stop (`Via::Band`), so it costs an event only when it carries something. | P16 as written. A band that is a flow and only an event when it has news keeps a quiet world quiet. |
| E59 | **Bases** (P30's recommendation): BASE MARKED makes a base a node the first time (`World::mark_base`) and a stop on every road within 256 blocks (PLACEHOLDER) of it; travellers on those roads list it (`Traveller::stops`); UNMARKED by every player retires it as a stop, the node stays. A caravan trades only through its own offers (`caravan_offers`, the MERCHANT_OFFERS shape): it sells its cargo at its destination's price as of the news it carries, converted to its own coin, and buys what its home lacks at its home's price, up to the coin it carries; a disliked player pays more (E7). A trade (`caravan_trade`) promotes the traveller, moves goods and coin to and from the player (ledgered outside), is a fair-trade report its home will hear, and at a base sets the base's price for that good (in emeralds at mint parity, so bases compare), which is the base's news. | P30 as recommended. The scenario's farm moved to (300, 150) so a road passes within the radius (scaffolding positions). |
| E60 | **What players see** (A15): the bridge reports positions (`set_player_pos`); each player's interest area (radius 160 blocks, PLACEHOLDER: vanilla's simulation distance) is booked as a leg in the shared leg index for the current window, replanned on each report. `visible(pos, radius)` finds flows through a static region index over roads (`rtsim_travel::RoadIndex`, built once per edge) and samples their travellers, finds individual caravans through the leg index, and adds bands; `encounters(player)` lists the individual caravans whose legs meet the player's booked area in its window. | Roads are permanent, so booking them in every time window of the leg index would cost per window; the leg index stays for things that move on legs. |
| E61 | The scenario script gains: a ribbit caravan robbed at day 35 (walkthrough F), the farm marked a base at day 30 and a caravan trade there (J), the sorcerers' answers at day 45, bands, houses and sentiment snapshots in the report. The scenario JSON is schema 2 (caravan sizes and these features added). | Shows each feature end to end. |

## Round 2: memory (SCALE.md §9)

| # | Choice | Why |
|---|---|---|
| E58 | A pipeline with nothing on the road and nothing coming is forgotten when trimmed (84% of them in a settled grid); a direction's pipelines are a sorted vector (`rtsim_travel::VecMap`), not a B-tree that allocates an 11-entry node for one pipeline; market books are `i64` (`Count`, 9.2 × 10^15 items), not `i128`; the node and edge tables are shrunk once built. `World::memory` estimates heap by component; `rtsim-cli sim memory` prints it beside the counting allocator. | Measured where it went first (below). Cheap changes only: no hot/cold split of the market rows yet. |

**Memory per market** (grid of 1,024 villages after 20 days, counting allocator; the bench's 4,096
point in brackets): round 1 30 KB [120.6 MiB]; round 2 before slimming 20.5 KB [81.8 MiB]; after
15.8 KB [62.9 MiB, 15.7 KB]. What is left, by the estimate: the market table's timing wheel 3.3 KB
(about 14 stale wakes per market and retained slot buffers, inside `rtsim-kernel`), the pipelines'
vectors 2.0 KB, snapshots 1.9 KB, the world's own wheel 1.7 KB (same cause), market lines 1.4 KB,
edge rows 1.0 KB, goods' curves and recipes 0.8 KB, the rest under 0.7 KB each.

**Rules kept as toggles** (E63): every quiescence rule is a field of `rtsim_trade::Rules` with the
chosen default, and `--rules` on `sim phase1` and `sim quiet` turns any off, so each comparison above
can be rerun.

## Round 2: where this departs from the design

- **Money sink** (E43): PLAN.md §8.4 has a market above `2 Mbar` refuse cash and barter. Now it
  spends the excess (upkeep) and barter in flows is off; cash refusal happens only for a moment after
  a player's purchase. P9 should decide whether this is the money rule.
- **Goods slack** (E43): spoilage and plenty at the target are new; the economy research has none.
- **Levels are bands** (E44); **rationing** replaces best-first when short (E49); a treasury's level
  changes are no longer news or breakpoints except at twice its target and the bottom of its band
  (E47).
- **Dock wait** (E53): flows are delayed by the wait plus the walk, `T`'s floor is `2 × (walk + wait)`,
  and news on quiet roads waits for the next caravan.
- **Growth** (E56) ignores needed goods a village has no way to get.

## Round 2: incomplete

- **Save and load** of the phase 1 world through the kernel's page store waits for the kernel
  follow-up (paging, incremental saves) to merge; it was not on `origin/main` when round 2 finished.
  What a save needs: every `Market` (its `Encode` covers lines, books, curves; `owed`, `cut` and
  `rules` are now in it), each edge's pipelines, lattices, cursors and pending packets, the packets in
  flight, bands' cursors and pending packets, growth clocks and pending houses, players, bases, and
  the world's wheel (whose entries the kernel follow-up's `TimingWheel::iter` can list).
- **The wheels' memory** (3.3 + 1.7 KB per market) belongs to `rtsim-kernel`: stale lazy wakes and
  retained slot buffers.
- **Residual churn** in some grids (E43 to E52, "Residuals"), and a grown world's anchors still come
  from whoever was there when each village joined (E15).
