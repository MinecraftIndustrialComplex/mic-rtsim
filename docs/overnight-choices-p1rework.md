# Choices: phase 1 after Ben's morning review (2026-10-20)

Reversible choices made while implementing the answers in DECISIONS.md, "Answers to the open
questions (Ben, morning review 2026-10-20)", that change phase 1: G31, Prices, Review P3, P16, P21,
News, P18, U10 and Pages. Numbered PR1, PR2 and so on. Numbers marked PLACEHOLDER or TUNABLE are not
measured; TUNABLE ones are to be set from a soak (P9, P18). Branch `rework/phase1-morning`.

Not done here, by Ben's instruction: the money sink (E-sink, "surplus coin buys something real")
needs a design Ben approves first, so the round 2 sink stands with a TODO pointer
(`rtsim_trade::Rules::money_sink`); the caravan balance fixes of
research/caravan-balance-2026-10-20.md ("Not now"); industry, dragons, herds and ships.

## Prices and reputation (G31, "Prices")

| # | Choice | Why |
|---|---|---|
| PR1 | **Reputation picks a price level inside the engine's quote.** MARKET carries a fair level and four markup levels at standings -25, -50, -75 and -100 (TUNABLE, PLACEHOLDER; `phase1::PRICE_LEVELS`), each `markup_for(threshold, worst)`. A player's standing is vanilla's own reputation with that villager (`Villager.getPlayerReputation`: its gossip, which the sim also writes, C20) or, for ribbits, the village's SENTIMENT; the bridge looks up the deepest level whose threshold the standing is at or below. Vanilla's `updateSpecialPrices` discount is taken off again on sim-priced offers (`VillagerReputationMixin` now folds instead of trimming). Positive reputation and Hero of the Village give the fair price, never better (E7). Supersedes G31's trim and G20's "vanilla villagers get no markup". | Ben chose "Inside the sim quote": one formula. A discount on both sides of a market that buys and sells the same good is a money pump (G31's review), so a good standing earns the fair price, a bad one pays the markup. Vanilla's scale is the sim's (P18: "vanilla's gossip scale"), so the thresholds are on both; -100 is where vanilla's golems turn. |
| PR2 | **The engine sends ladders of bundles.** Per good and side, per level, the bundles of up to 8 successive trades (PLACEHOLDER, `phase1::LADDER_DEPTH`), each priced at the market the trades before left (`rtsim_trade::market::ladder`; `Market::offer` now calls the same `bundle_at`). The bridge walks a ladder by its own trades since the MARKET and shows the step there; a stock the ladder does not reach (a sale and a purchase of one good mixed) shows the offer crossed out until the next MARKET, which the engine sends right after a trade. About 4.7 KiB per MARKET for the catalogue's seven goods. An offer's uses are now at most the ladder's depth (8) until the next MARKET, where they used to follow the whole stock. Scripted MARKET orders still carry curves: the native engine prices them with rtsim-trade, the stub with the Java copy, so the same GameTest checks both. `PriceCurve` and the new `PriceOracle` (the old `MarketBook.offer` math) stay in the core only as the test oracle and the stub's pricing. Supersedes G4. | Ben chose "Engine sends bundles". One bundle alone would let a shift-click trade at a stale price, or trade once and wait for the engine; a ladder covers a run of trades exactly, and its steps are the engine's, so nothing drifts. |

## Caravans (Review P3, P21)

| # | Choice | Why |
|---|---|---|
| PR3 | **A lost member books what it held, in this order:** its own planned cargo, then the purse, then the other members' cargo, never more than the row holds (a chest holding what the sim never put there is not the sim's). What it was planned to carry and did not hold moves to the next carrier (with nobody left, it leaves with the group). Items the sim does not trade are ignored; no MEMBER_STATE before the TAKEN or DIED books nothing out. The same for a member lost with nobody to blame (`lose_member_holding`). Logged as a new input (`MemberHeld`, tag 14). | Ben: "Book what was really there." The purse rides in the last carrier's chest (V12) and coin from player trades sits in it, so the purse comes before the other llama's goods. |
| PR4 | A capture at despawn (V13) is still compared and logged, not reconciled. | P3 decided TAKEN and DIED; every change in game should already have come as TRADE or CARGO_CHANGED. |
| PR11 | **Coin exchange at border markets** (P21): a trader or ribbit merchant at a border market (a village with a road to a village of the other culture) that is one end of its leg offers the foreign coin for the market's own and buys it back, at that market's exchange line price, each bundle rounded against the player and with the caravan's markup. A puppet is "at" the market within 96 blocks of it (PLACEHOLDER); its MERCHANT_OFFERS are sent again when it walks into or out of that reach. Uses follow what it carries (cargo and purse). Resident ribbit merchants get no exchange offer: the sim never adds a resident's offers (PLAN.md §8.5). | Ben chose "At border markets". Rounding both ways against the player keeps a round trip from making coin. |

## News (News, P18, P16)

| # | Choice | Why |
|---|---|---|
| PR6 | **Messengers are flows.** Every road has messengers leaving each end every 6 quanta (TUNABLE, PLACEHOLDER: four a day), each end on its own phase from the edge's seed, walking at 1.5 times a caravan's pace (TUNABLE, PLACEHOLDER). A news packet leaves with the next messenger and merges with one waiting for it. Messengers have no bodies (D3) and cost an event only when they carry something. Ships' lanes keep sending news with their ships. Empty scouts (E10) still walk idle roads for players to meet, but carry no news. The `T` bound (`2 × (walk + dock wait)`) is unchanged: it is about goods. | Ben chose "News travels separately". Flows keep a quiet world quiet; the dock wait no longer delays a robbery report by up to two days. |
| PR7 | **Report lifetimes lengthened** (TUNABLE): killings 30 days (Veloren's 15 doubled), robbery and theft 6 (Veloren's 1.5 four times), assaults and rescues 10, trades 3. | P18: "Lifetimes are lengthened now that news travels separately." A hop takes about half a day now; 1.5 days reached only the nearest villages. |
| PR8 | **Bands' circuits** (P16): from each stop, the nearest neighbour the band may visit: a ribbit village, a human village with a ribbit neighbour (the border), or an active player base (P30) whose every marking player stands at or above 0 (TUNABLE, `band_standing_at_or_above`) with the band's home. The circuit is charted again at each stay at home (a new circuit starts there; each kept stop keeps what it knew of the band); one that would shrink to home alone keeps its stops. | Ben chose all three plus "player settlements (assuming the player is in good standing)". Charting at home needs no event of its own and never moves a band mid-walk. |
| PR9 | **Bands carry gossip, never prices**, and a band's delivery is not a market event. | With prices on bands, the border circuits changed the news paths and a grid of 16 villages fell into a payment churn: 0.65 market events per market per day at days 400 to 600, against 0.004 with gossip only (the quiet test's bound is 0.2). Markets must not hang on a circuit that follows players' standing. |
| PR10 | **Performances** (P16): at each stay away from home the band plays and the host's feeling toward the band's home village rises by 5 (TUNABLE, `band_performance_delta`): a direct change, not a report, so it is not news. One world event per stay (`BandPlays`); a base whose owners fell out of good standing since the circuit was charted gets no performance. | "Raise the host's opinion of the band's home village." A report would travel and wake markets at every stay; the opinion is the host's own. |

## Switch and pages (U10, Pages)

| # | Choice | Why |
|---|---|---|
| PR13 | **Phase 1 runs only with all four of its modules on** (knowledge, settlements, trade, travel; before, any one kept it on). Switched off (U10) it captures every puppet back (one last write), drops what was queued for the session, ignores censuses (latest state), runs nothing, decides nothing and materializes nothing; on again its world runs to now at once. **Amended by RP-300: the switch gates only new decisions; every record of what already happened is heard; a record applied while off goes in at its own tick, never at the frozen clock.** The journaled records (trades, cargo, taken and killed members, hurt, rescue, talked, bases, adoptions, HOUSE_PLACED, a work's COMMAND_RESULT) are no longer ignored while off (the engine acknowledged them and Java will not send them again): their inputs are kept at their own ticks in the meta page and applied in time order at the switch-on, at the next step, or before a viewer module runs the shared clock past them. Commands the bridge answers by op id that were decided before the switch and not yet written wait, unsequenced, until it is on again; resends of ones already written still go out with their sequences. The dragons, ships, herds and conflict live in its world and run its clock while they run. Industry's W8 is untouched (its own rework). | Ben chose "Ignore everything". Trade or travel off restores vanilla's traders (C25), which caravans must not double. A sequence is taken only when a command is written, so holding an unwritten one is safe; holding a resend past newer sequences would make Java skip it (Java skips anything at or below the last sequence it applied), and dropping either would lose a decision made while on. A puppet left in the world while its records are ignored could be robbed for free. |
| PR14 | **Pages: confirmed, unchanged.** Phase 1 pages a build cannot read are dropped loudly and the world starts fresh (`when_pages_unreadable`, S5; test `unreadable_phase1_pages_are_dropped_and_replaced`). This rework changes the snapshot and log formats (edges, bands, a world event, a log input), so pages written before it are dropped that way. | Ben chose "Drop and start fresh"; no compatibility work before a release. |

## Money sink

| # | Choice | Why |
|---|---|---|
| PR12 | The round 2 money sink (E43) stands, with a TODO pointer to E-sink on `Rules::money_sink`. | Ben: surplus coin is to buy something real (village upgrades, buildings, tribute), which needs a design he approves first. |

## Results

- **Rust:** `cargo test --workspace` passes (58 test binaries, no failure; new: rtsim-world
  `tests/morning.rs` 7 tests, rtsim-trade `a_ladder_is_the_offers_of_successive_trades`, rtsim-travel
  `messengers_leave_on_their_own_schedule`, rtsim-jni `a_switched_off_phase1_ignores_everything_and_catches_up`,
  `a_killed_member_books_what_its_member_state_held`, `a_caravan_at_a_border_market_offers_coin_exchange`,
  `market_ladders_are_rtsim_trades`, and the log codec's `MemberHeld`); `cargo clippy --workspace
  --all-targets -- -D warnings` and `cargo fmt --all --check` clean. The quiet tests hold: the
  scenario world silent over days 500 to 700, the grid of 16 at 0.004 events per market per day.
- **GameTests,** stub and native engine, each on a fresh GameTest world: core 48 of 48, ribbits 5 of
  5, carryon 2 of 2. The scripted MARKET prices the same on both engines (the stub's Java oracle and
  rtsim-trade): wheat per emerald 21, 22, 23, then 18 after the refill; slime per shard 11 fair and
  18 for a player the ribbits hate. The live caravan's robbery reached two villages' gossip after 10
  asks on the native engine.
- **`sim phase1`** (120 days, seed 42): conservation ok, 0.02 market events per market per day over
  the last 10 days, state hash 6d41ee745999da50 (scenario/results/phase1-seed42-120d.json rewritten).
  Bands now tour Brindle and Fernwick (the border) and played 436 times. `--days 60 --check`:
  conservation ok after every event.
