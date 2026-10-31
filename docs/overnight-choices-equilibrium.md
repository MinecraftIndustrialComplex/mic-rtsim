# Overnight choices: EQUILIBRIUM.md slices S1, S2 and S4 (2026-10-22), S5 and S6 (designed and built 2026-10-29), S7 (designed 2026-10-30)

Reversible choices made while Ben was asleep, each with its reason, numbered EQC#. None is final
until Ben reviews it. The work is EQUILIBRIUM.md §4's slices S1 (the who-is-alive census), S2
(herds: density term, refuge, dispersal, frontier founding, tombstone removal) and S4 (dragons: lair
pressure, fed share, starve-leave, arrival gate, newcomer direction, oil visitors only into due
lairs). DECISIONS.md is unchanged; EQUILIBRIUM.md marks the slices built. Branches `feat/eq-s1-s2`
and `feat/eq-s4`. The last two sections are S5 and S6. S5 is the design of the dragon home bodies (EQ-2 (a); records EQC100 to
EQC144) and, after the design, what the build found (EQC145 to EQC149; branch `feat/eq-s5-dragon-bodies`). S6 (villages and
Ribbits; EQC150 to EQC193) is designed and built on `feat/eq-s6-villages`. S7 (conflict pressure and the outpost reporter; EQC200 to EQC299) is designed on
`feat/eq-s7-conflict` and not built.

## S1: the census

| # | Choice | Why |
|---|---|---|
| EQC1 | (2026-10-22, slice S1) **One census type for every engine, `rtsim_kernel::census::KindCensus`, with one row per kind of site**, and each engine a `census()` read at its own now: the world's `village.human` and `village.ribbit`, the dragons' `dragon.lair` and `oil.nest`, the herds' `herd.range`, the ships' `ship.haven`, the conflict's `conflict.camp`. The native engine's `Engine::census` joins the modules its session runs, in that order. | The plan asks for "one line per kind ... for every module", and later slices measure themselves with it; one type next to `renewal` keeps the counting rule (which state a site is in) in one place. |
| EQC2 | **An `empty` column beside the plan's list** (sites, members, capacity, short, empty, broken, mean P, renewals). A site is broken, else empty (no members), else short (fewer than K), else full; full is `sites - short - empty - broken` and is not printed. | SITE_STATE (§2.2) has four states; without `empty` a wiped kind and a thinned kind read the same, and the kill-all tests of S2 to S11 count empty sites. |
| EQC3 | **Renewals are printed per line, as the count since the kind's previous line**, from each engine's cumulative counters (`CensusLog` keeps the last total). | The plan's "renewal events per day flat" is a rate; a running total would grow forever and never diff flat. |
| EQC4 | **What a renewal is, per kind:** a lair's claims plus hatchings; a nest's first broods plus worms renewed; a herd's yearly calvings (S2 adds foundings and recolonisations, EQC20); a haven's refits; a ribbit village's houses booked. Human villages and camps book none yet (S6, S7). | Each is the event that adds a member to a site, the model's `*_events` (`research/equilibrium-2026-10-21/model.py`), from counters the engines already keep. |
| EQC5 | **K where the engine has none yet:** villages print capacity 0 and count full or empty (beds come with S6); a camp's K is its culture's pool cap and its members the whole warriors in its pool; a herd's K is its founding count until S2 stores one. Mean P reads 0 for kinds with no pressure yet (lairs until S4, camps until S7, herds and villages always). | The census reports what the engines hold; inventing a K for villages now would be a second, unreviewed rule. |
| EQC6 | **An oil nest whose cell was given up (no room for the chamber) is not a site.** A broken heart is. | A given-up cell never renews and holds nothing; counting it would show a broken site forever where §2.4.8 says there is none. |
| EQC7 | **`rtsim-cli sim census` runs three scenarios with nobody watching**, each with its world's villages: `ecology` (the phase 1 dragon world's lairs, oil under two nest cells with an endless well beside each, the herd valley's six herds hunted by the dragons), `ships` (the synthetic coast's haven), `conflict` (the goblin camp). Defaults: seed 7, 960 days, a line every 24 days (the model's census grid), plus day 0. Text only: a fixed-width header and one line per kind. | A nest wakes and renews only while drilled (S10), so an undrilled scenario would show empty nests forever. Tests read the Rust API (`sim::census::run`, `Scenario`), so the text has no consumer that needs JSON yet. |
| EQC8 | **The host soak samples `Engine::census` every 10 game days** and prints it on its table's days; `a_thousand_days_at_max_keeps_the_state_bounded` asserts every module's kinds are on every sample. The flatness bands wait for S11's `who_is_alive_soak`. | The populated host world has one lair, one haven, one camp and no herds; it shows the hook works end to end, and S11 is the slice that judges bands there. |

## S2: herds

| # | Choice | Why |
|---|---|---|
| EQC9 | (2026-10-22, slice S2) **A herd row is its range, kept for good.** A row now stores its range's K (the founding count), its range (the 512-block region of its canonical founding position), its home (that position) and, when empty, the tick it emptied (`Herd::k`, `range`, `home`, `vacant`). An extinct herd's row is not removed: it waits, vacant, for a frontier founding or dispersers, and is reused with its id and name. The engine indexes rows by range (`Herds::range_herds`). The world model's region mark stays and now means "judged" (the region was founded or found barren once); the name guard that kept the world model from founding a second herd is the range index (`rtsim-jni` `herds::has_range`). | §2.4.2: "an extinct herd's row is reused". One row per range keeps the world model's one-herd-per-region rule true for good, so nothing else has to change when a range empties; the plugin's agent ids, the archive's subjects and the viewer's cards keep pointing at the same herd. |
| EQC10 | **Beverton-Holt's M comes from the engine's own life table** (`HerdSpeciesData::bh_m_per_k`): with `l_n` the expected years alive per newborn and `l_a` those as an adult (summed a year of age at a time over the species' cohorts and annual mortality), `M = K l_a / (l_n (f l_a - 1))`. The factor `1/(1 + A/M)`, A the herd's adults, multiplies each litter's chance in `breed`. A species that cannot replace itself has no term. `HerdTunables::density_term` turns it off (the tests' "births as before"). | §3.2's closed form has two stages (young, adult and old); the engine has three cohorts by age, and old animals do not breed. M set from the closed form would put the steady state somewhere other than K. The life table gives the same fixed point the engine's own ages produce. |
| EQC11 | **The refuge binds predators only:** `predate` takes no more than `alive - R` and `prey_near` offers no herd at or below R, `R = max(refuge_min 8, round(refuge_share 0.1 x K))`. Deaths of age, hunger and player kills can still take a herd below R. The model's switching weight (type III, predators favour common prey) is not built: dragons choose prey by food, and that is S4's. | §2.4.2 and §3.4: the refuge is "the rule that matters"; §3.7 shows switching alone fails from four times the appetite. A player can still hunt a herd out, which is what the frontier and dispersal answer. |
| EQC12 | **Spring dispersal runs at a herd's yearly breeding decision, after its births:** a herd above `donor_above` (0.4) of its K sends `Binomial(latent adults, disperse_share 0.05)` (one draw keyed by the herd and year; animals a player has seen stay) to its four neighbouring ranges (region centres plus or minus 512 blocks, canonical, so across a seam) that are below `taker_below` (0.25) of their K and are empty or of its species. The split is exact: each taker gets `sent / n`, the first `sent % n` one more. Dispersers keep their birth days, as classes of the taker; both ledgers book them (`left`, `arrived`), and the world's arrivals equal its departures. An empty taker takes the donor's species and condition, counts as bred this year, and is scheduled to decide. | §2.4.2's rule. Species: a thin herd of another species is not a home for the donor's animals (the model adds them anyway, a simplification of a grid of counts). Keeping birth days keeps every member's age, look and identity seed real. |
| EQC13 | **Dispersers that reach a live, thin taker before its own breeding decision breed again there that year** (at most once more, and only into a range below 0.25 K). | Ordering every range's breeding before every range's dispersal would need a second yearly pass; the extra births are bounded by 5% of a donor's adults going only to thin ranges, which is where the plan wants growth. |
| EQC14 | **The frontier founding is one exponential wait from the moment a range empties** (`frontier_per_year` 0.25 a range-year; one draw keyed by the herd and that tick, one wheel entry), not the model's yearly Bernoulli at spring. When it comes, `max(frontier_min 12, round(frontier_share 0.05 x K))` young adults of the range's last species arrive, 192 blocks (3/8 of a region) from its home toward the nearest live herd within `frontier_search` (3) rings of ranges, if that cell is passable, else at the home. If a player stands within view radius, hysteresis and the group's footprint of that point, it is put off to the next sub-season. No "explored edge" direction, and no `loaded_regrow_rate` (the model's vanilla spawns near players). | §2.4.2 asks for "one draw per empty range, scheduled"; an exponential wait is that draw and costs nothing while it waits. The herds engine does not know which ground is explored, so the nearest live herd is the direction it can know; with none near, the animals start at home. Vanilla's own spawns stay outside the sim (Y20). |
| EQC15 | **`Herds::cull` kills every latent animal of a herd now, booked as killed** (a test and tool hook for the wipes; nothing in the game calls it). The range empties at its next decision. | The kill-all tests need a world wipe that books like any death; an embodied group is the plugin's, so it is left. |
| EQC16 | **The herds' save format is VERSION 4** (each row's K, range, home and vacancy; the ledgers' `arrived` and `left`; six new counters: vacated, frontier foundings, frontier postponed, dispersals, dispersed, recolonised), with no migration. rtsim-view's GameTest fixtures are regenerated. | No backward compatibility before release (Ben's rule). |
| EQC17 | **A frontier founding and a range founded again by dispersers write the archive's existing `herds.founded` row** (at the arrival point, with the animals); `herds.extinct` still marks the emptying. No new archive kind. | The timeline already reads a founding as a founding; a new kind would need the viewer and the archive dictionary to learn it for no new meaning. |
| EQC18 | **The test world is a grid of ranges** (`rtsim_ecology::herds::synth::ranges`; `rtsim-cli sim census --scenario ranges`): 8 x 7 regions of the valley (x -3 to 4, z -3 to 3), one herd per region at its centre, its species drawn per region from the seed, its K the species' founding density times 32 cells (half a region's 64 world-model cells), under the phase 1 dragon world's five lairs, with nobody watching. The wipes come after 10 game years (the model's after 20): the herds settle from their founding count within about 4 years, and a 40-year run of 56 ranges takes about 100 seconds in release. | The valley's six herds are not neighbours, so dispersal could never fire there. The grid founds herds as the world model does (one per region, density times habitat). |
| EQC19 | **Forage headroom in the engine is every species' diet divided by x** (`sim::equilibrium::with_headroom`). The probe (seed 7, years 10 to 20 of the grid): with the density term N/K is 0.17, 0.41, 0.78, 0.83, 0.85 at x 0.25, 0.5, 1, 1.5, 4; without it 0.30, 1.1, 3.0, 5.5 and 14.6 (still growing). So today's diets are about the model's headroom 1: forage starts to bind the proposed herds there and sets their level below it. The sweep tests x 1, 1.5 and 4, as §3.7. | The model defines headroom by its own forage field; the engine's valley has no single number to match, so the knob is the one that scales forage against herds, and the probe shows where it starts to bind. |
| EQC20 | **A herd range's renewals in the census are its calvings, frontier foundings and ranges founded again by dispersers** (EQC4 said "S2 adds" them). The census K is the stored K. | The census counts what adds a member to a site. |
| EQC21 | **The long checks are `#[ignore]`d, with short always-run versions** on the 3 x 3 ranges at spawn and one seed (`crates/rtsim-cli/src/sim/equilibrium.rs`; run the long ones with `nice -n 10 cargo test --release -p rtsim-cli sim::equilibrium -- --ignored`). The long ones judge seeds 1 to 30 against the plan's row: half of the year before back within 25 years, every range alive 40 years on (both wipes); for the forage sweep, the three headrooms' medians within 0.1 K and each seed's spread within 0.15 K. `predation_never_takes_a_herd_below_refuge` and the ranges' property tests are in `crates/rtsim-ecology/tests/herds.rs`. | The task's rule for long-horizon tests. The 30-seed runs of all three take tens of minutes on the shared laptop. |

## What the census found (not edited)

- **Herds grow past their founding count with nobody watching** (seed 7, `sim census --scenario
  ecology`): 33,000 animals at day 0, 26,449 at day 96 (the founding pyramid's first deaths), then
  41,575 at day 480 and 49,780 at day 960, 1.5 times the founding count. This is the missing density
  term §3.2 describes; S2 adds it.
- **A camp's pool dips below its cap with nobody watching** (52 of 60 on day 480 in the conflict
  scenario, 53 of 60 on day 20 of the host soak): its parties go out and come back. `short` counts it.

## What the S2 checks found (not edited)

The 30-seed checks of EQC21 on the 8 x 7 ranges grid, release build, seeds 1 to 30, median [5th,
95th] over seeds, against §3.6's model (whose wipes come at year 20 and whose numbers are 16 seeds):

| Check | Engine | Model |
|---|---|---|
| N/K, years 10 to 20, headroom 1 | 0.78 [0.76, 0.80] | 0.59 [0.48, 0.70] (no player) |
| N/K, headroom 1.5 | 0.83 [0.81, 0.84] | |
| N/K, headroom 4 | 0.83 [0.82, 0.85] | |
| World wipe: days to half the year before | 1,560 [1,368, 1,752] | 1,368 [1,248, 1,656] |
| World wipe: days to 90% of the control | 3,096 [2,808, 3,408], every seed | 3,192 [2,873, 3,538], 94% of seeds |
| World wipe: last year over the control | 0.95 [0.92, 0.98] | 0.92 [0.88, 0.94] |
| World wipe: days to every range alive | 1,272 [936, 1,752] | (alive at the end in every seed) |
| Local wipe (3 x 3 at spawn): days to half | 1,176 [792, 1,464] | 1,128 [792, 1,608] |
| Local wipe: days to 90% of the control | 2,688 [1,944, 3,360], 26 of 30 seeds | 2,724 [2,390, 3,596], 88% of seeds |
| Local wipe: last year over the control | 0.98 [0.84, 1.13] | 0.94 [0.85, 1.02] |
| Local wipe: days to every range alive | 288 [120, 792] | |

- **Every range is alive 40 years after both wipes in all 30 seeds,** and every seed is back to half
  within 25 years (the longest 1,752 days, 18 years). A world wipe refills by the frontier (about 53
  of 56 ranges per seed) and the rest by dispersal; a local wipe refills by dispersal (228 of the 270
  wiped ranges over the 30 seeds) and the frontier (29).
- **One band across forage headroom 1.5 to 4, and 0.05 K below it at 1:** the medians are 0.78, 0.83
  and 0.83, within the 0.1 the check asks, and no seed spreads more than 0.15 K across the three.
  Headroom 1 is where today's diets start to bind (EQC19).
- **The engine's herds sit higher than the model's 0.59 K** because the grid has the phase 1 world's
  five lairs over 56 ranges, fewer dragons per range than the model's world, so less of each range's
  yearly production is eaten (§3.6: about 40% goes to dragons at the model's density). The wipes'
  times match the model's because they are set by one calving a year (§3.2), not by the level.

## S4: dragons

| # | Choice | Why |
|---|---|---|
| EQC22 | (2026-10-22, slice S4) **Each lair keeps its pressure and its wait:** `Lair` gains `pressure` (the shared `Pressure`, P and its day), `due` (when its wait ends), and the fed share with its tick (EQC24). Whenever a lair empties (age, starve-leave, a move, any death, a claimant lost on the way), its wait starts: `min(192, 20 x 2^P)` days from P then (`DragonTunables::renewal()`: `respawn_delay_days` 20, `respawn_cap_days` 192, `respawn_half_life_days` 96). The respawn event carries the lair's plan number, so a superseded wait is dropped. A lair found or placed starts at P 0 and the base wait, as today (EQC31). | §2.4.1 on S0's rule. The lair is the site (K = 1 occupant), so P lives on it, not on a dragon. |
| EQC23 | **A player's loss is every dragon death the bridge reports with a killer kind other than `Mob`** (a player, `None`: today's taming and odd deaths, `Other`: traps and the rest), and each such dragon adds 1 to its lair's P, or to the P of the lair it was flying to claim. A pair killed together adds 2 (an 80-day wait). Dragons killed by a wild mob, of age, or starving away add nothing. `Dragons::slay` is the scripted players' and tests' hook. | §2.2: "any death no wild mob is credited with", "each `P += 1`". The pair rule follows "each" literally; one dragon of a pair killed costs the lair the same as a lone one. |
| EQC24 | **The fed share is the lair's, a time-weighted moving average:** a new reading `x` dt days after the last moves it by `(x - fed)(1 - e^(-dt/30))` (`fed_tau_days` 30). Readings: each hunt's catch over the hunter's need then (`min(full - fullness, appetite)`), and 0 for each daily decision that found nothing in reach. It starts at 1 when a dragon moves into an empty lair. Starting at 1 with nothing to eat, it falls below 0.35 in about 32 days. | The plan's "30-day moving average of how much of its need it catches". The engine has no daily ration; its hunts and its empty days are the readings, and weighting by elapsed time keeps a lair that hunts often from forgetting faster. |
| EQC25 | **Starve-leave replaces the old fruitless move:** after each hunt and each empty daily decision, a dragon older than `starve_leave_age_days` 60 whose lair's fed share is below `starve_leave` 0.35 leaves it: for the nearest vacant lair of its species within its `grudge_range` (6,000 blocks) whose wait has ended and whose ground passes the gate (EQC26), else off the map (a `DRAGONS_DEPART` tape event, the knowledge's `die`) and out of the sim (`starved_away`). Embodied dragons and oil visitors never starve-leave. `fruitless_before_move` and the move after three fruitless hunts are gone; the fruitless widening of the hunting range stays. | §2.4.1. The old move ignored how well the dragon had eaten before; the fed share is the plan's measure. `grudge_range` is the engine's existing "in reach" for lairs (the oil visitors' and dispersers' search). |
| EQC26 | **The arrival gate reads the lair's hunting ground, the nine regions around it:** when a wait ends, the lair's ground share is `W / (W + gate_half 200)`, W = `gate_village` 300 with a village in those regions (`gate_wild` 30 without) plus `(N - R)^2 / K` for each herd in them (its animals above the refuge, from the herds' `prey_near`). Below `arrive_food` 0.5 the check is put off `arrive_recheck_days` 24 (`arrivals_gated`). The same gate guards every move into a lair: newcomers, dispersing hatchlings, starve-leavers and oil visitors. With no herds lent to the dragons (a plain `run_until`, a session without the herds module) the gate is open. | §2.4.1 and §3.4's weights; the nine regions are §1's "the herds in the nine regions around the lair" and the model's 3 x 3. A first try with the newcomer's own territory (30 days old: 600 blocks) shut three of the census scenario's five lairs for good once their first dragons died of age, though those dragons had caught 0.87 to 0.95 of their need (an adult hunts 1,500 to 2,500 blocks out). |
| EQC27 | **Predation is not changed:** dragons still choose prey by today's utility; the type III weight `(N - R)^2 / K` is used only by the gate. | S4's row does not list it, and EQC11 left it open. It would change every hunt and the `sim dragons` story; the refuge (S2) already makes extinction by predation impossible, and the S4 checks pass without it. A later slice, if Ben wants it. |
| EQC28 | **Sources, in order:** a Neighbour first, the nearest hatchling of the lair's species at least `disperse_days` (50) old, roosting and not embodied, whose parent shares its lair, from a lair within `grudge_range` holding two or more (`heirs_sent`); else a Frontier newcomer as today (30 days old, 3,000 blocks out), flying in from the side away from the centroid of the lairs and villages within 3,000 blocks of the lair. With none, or the centroid within 150 blocks of the lair, the bearing is random as before. The random bearing is always drawn, so the stream does not depend on which is used. | §2.4.1 and M1. The engine does not know which ground a player has explored; "away from the known sites around it" is the nearest unexplored direction it can know. |
| EQC29 | **Oil visitors claim only a due lair:** `visitor_done` uses the same search as starve-leave (wait ended, gate passed); with none in reach the visitor leaves as today. | §2.4.1 and OILGATE §4.2. |
| EQC30 | **The dragons' pages are RTD7 and their meta RTN7** (each lair's P, its day, due, fed share and its tick; four counters: `player_losses`, `starved_away`, `arrivals_gated`, `heirs_sent`), with no migration. rtsim-view's GameTest fixtures are regenerated. | No backward compatibility before release (Ben's rule). |
| EQC31 | **S3 is not built, so S4 takes the conservative path:** (a) a lair found late (rediscovered after a reset, or placed by `PLACE_LAIR`) starts at P 0 and waits the base 20 days, as today, not at the cap that §2.3 gives a site found empty with no ledger row, since telling a lair found at world generation from one rediscovered after a reset needs the ledger (EQ-3); (b) the bridges' kill credit (`getKillCredit()`, a pet's owner, `onTame` naming the tamer) stays S3's, so a pet's kill still reads `Mob` and costs the lair nothing; (c) the GameTest `resetDoesNotMintADragon` waits for S3, because without the ledger a reset still gives an empty lair a dragon 20 days later, which is what it forbids. No Java changes. | The task: S4 waits for S3; take the conservative path and record it. |
| EQC32 | **The world checks are in `sim::equilibrium`, on the 8 x 7 ranges grid and its five lairs** (`dragon_run`, `Roamer`, `dragon_kill_all`). The roamer is the model's, on dragons only: every second day a step to one of the nine regions around it, refused beyond `min(6, 2 + day / 48)` regions of spawn; each day, with chance 0.5, it kills every dragon at home in the lairs of its region. It kills no herd (nor does the model's). Lairs held counts lairs with a dragon at home, on the 24-day census. The kill-all runs the roamer to year 20, kills every dragon in the world as its player, then turns peaceful; the control is the same seed turning peaceful with no kill (the model's `killall`). The 60-year check runs each seed with the roamer and with nobody, read over the last 30 years. The always-run versions use seed 1 on the whole grid (no lair lies within the 3 x 3 at spawn): a kill-all in year 1 and four years of the roamer. | §3.6's scripts and measures, as close as the engine's world allows. |
| EQC33 | **Three tests turn starve-leave off** (`starve_leave = 0`): the tribute test and the archive test's hatching (their pairs live on roads and a tributary village and catch about a third of their need, so they would leave before what the tests watch), and the grudge test reads the new wait (80 days after two player kills). The farmed-lair and starving-dragon tests are in `crates/rtsim-ecology/tests/lair_renewal.rs` (the starving one on uniform grassland, so the herds that bring the newcomer stay by the lair); `visitor_claims_only_a_due_lair` is in `oil.rs`. | The tests watch other rules; the synthetic worlds feed their dragons less than a real one would. |

## What the S4 checks found (not edited)

The 30-seed checks of EQC32 on the 8 x 7 ranges grid and its five lairs, release build, seeds 1 to
30, median [5th, 95th] over seeds, against §3.6's model:

| Check | Engine | Model |
|---|---|---|
| Lairs held, years 30 to 60, the roamer | 0.83 [0.79, 0.88] | 0.89 |
| Lairs held, years 30 to 60, nobody | 0.89 [0.86, 0.93] | 0.95 |
| Herds N/K, the roamer | 0.78 [0.76, 0.80] | 0.59 [0.52, 0.63] |
| Herds N/K, nobody | 0.78 [0.75, 0.79] | |
| Fewest ranges alive, either | 1.00 in every seed | 1.00 |
| Kill-all: days to half the year before | 48 [48, 72] | 48 |
| Kill-all: days to 90% of the control | 72 [48, 96], every seed (the longest 144) | 48 [48, 72] |
| Kill-all: second half of the 6 years over the control | 1.02 [0.73, 1.22] | 1.01 |
| Farmed lair (one player killing every newcomer within 2 days), years 30 to 60 | 1.10 a year, P after a kill 2.13 | 1.083, P* 2.115 |

- **The lairs are held about 0.06 below the model's, with the roamer and without.** With nobody,
  each seed's five lairs lose about 68 dragons to age in 60 years, each followed by the 20-day base
  wait, and the gate puts off about 66 arrivals for 24 days each: together about 11% of the lairs'
  time. The roamer adds about 35 kills a seed (each a player's loss, every one in `player_losses`);
  about one dragon a seed starves away in 60 years, with the roamer or without. The roamer still sits below nobody in every band, as in
  the model.
- **The herds are where S2 left them, and the roamer does not move them:** 0.78 with it and without.
  The grid's five lairs over 56 ranges eat too little to show (the S2 finding); the model's 0.59 is a
  world with a lair per few ranges.
- **The kill-all's 90% is 24 days slower than the model's at the median, and the plan's 72 at the
  median holds.** Five lairs make 90% of the control every lair the control holds, so one lair still
  slow from the roamer's kills before the shock holds the seed back (seed 17: P near 2.9 after the
  kill, a wait near 144 days); the model reads many lairs, where one slow lair is under the 10%. The
  test asserts the median within 72 and every seed back within the rule's cap read on the census
  (216 days), not the 95th within 72. The end level is the control's at the median; the spread
  (0.73 to 1.22) is five lairs on a 24-day census.
- **The census scenario holds three or four of its five lairs from day 360 on** (`sim census
  --scenario ecology`, seed 7): Cinder Hollow and Old Scorch have neither a village nor a herd in
  their nine regions most of the year, so once their first dragons die of age the gate keeps them
  empty until a migrating herd passes. That is the gate doing what §2.4.1 says; the scenario's lairs
  were placed before there was a gate.

## S5: dragon home bodies (designed and built 2026-10-29)

EQUILIBRIUM.md §4's S5 row on Ben's EQ-2 (a), which reopens P3: **a dragon's body is vanilla while it roosts at its
lair, and a puppet while it is away.** Records EQC100 to EQC149 (S5's range). The design (EQC100 to EQC144) was written
first and reviewed; the build followed it and its findings are EQC145 to EQC149 at the end. `boundary.md`,
`replay-gate.md` and EQUILIBRIUM.md changed as EQC125 lists. Branch `feat/eq-s5-dragon-bodies`, based on `feat/mowzie-m12a-s3` at 43062c0b
(main d6ac9341 plus M12a: S3's site ledger, kill credit and MZB-1000 to MZB-1047).

**Words used.** A *home body* is a lair dragon's vanilla entity: saved by vanilla, carrying the plugin's `home`
attachment. A *puppet* is a body the plugin tags with the core's `agent` tag: never saved. A lair is *in* while every
chunk of its footprint is at FULL with its entities loaded (the camps' rule, MOWZIE §4.9), else *out*. A *census* is
the plugin's report of what stands in a lair that has come in. A dragon is *owed* a body when it roosts at its lair
and none is believed to stand. A body is *missing* when it was believed standing and a census finds it gone, *left
behind* when its dragon is away or dead and it may still stand, and a *stray* when it stands and the engine's books
do not hold it.

### What S5 changes, by place

| Place | Change |
|---|---|
| Engine (`rtsim-ecology`) | `Dragon.body` (the home body the engine believes stands); `Lair` gains `left_behind`, `missing`, `expect`; a census reconcile; strays adopted as occupants; missing and left-behind bodies; owed bodies spawned; recall of a watched lair's away dragon; hand-back; the lair's SITE_STATE carries home, left-behind and missing UUIDs (row K 2). No new event on the wheel. |
| Protocol (`boundary.md` §6) | New: LAIR_CENSUS 0x0510, LAIR_OUT 0x0511, DRAGON_LEFT_HOME 0x0512 (Java to engine, journaled); DRAGON_HAND_BACK 0x850A (engine to Java). Extended: MATERIALIZE flag 32, ADOPT flag 2 (and three sync fields), MATERIALIZED aux 1, DEMATERIALIZED reason 8, DISCARD reasons 3 and 4, DIED (the body's UUID and its lair). Retired: the lair field of DRAGON_UNTAGGED. |
| Formats | Dragon pages RTD9 to RTD10, meta RTN9 to RTN10, no migration. The home attachment on the entity. The site ledger file is unchanged (version 1; a lair row now has K 2 and UUIDs). |
| Plugin (`neoforge/iceandfire`) | The `home` attachment; `HomeBodies` (index of loaded home bodies, census, leash scan, the tag swaps); `DragonBridge` registers home bodies as puppets with a `home` flag and keeps them on capture; in-box dragons leave the untagged path for the census; the session stop keeps home bodies. |
| Core | One new class, `sites/ChunkWatch`: the chunk-level poll, `areEntitiesLoaded` and epoch, copied from the camps' proven logic and used first by this plugin. `Camps` is not touched. |

### The records

| # | Choice | Why |
|---|---|---|
| EQC100 | (2026-10-29, slice S5 design) **Scope.** S5 builds EQ-2 (a) for lair dragons only: at a lair a dragon's body is vanilla, away it is a puppet, the engine still owns counts, pressure, every body away from a lair and every abstract hour. It adds the lair census the S5 row names and the four GameTests of its row (one renamed, EQC109 and EQC139). It does not touch herds, villages, camps, nests or the oil gate's visitors, and it changes no S4 band (EQC141 tests that). Out of scope: the camps' refactor onto `ChunkWatch`, records riding on entities (F4), occupants beyond a pair. | The row, EQ-2 (a), and the rule that S4's bands stay: a body is how the engine's dragon shows, not a second population. |
| EQC101 | **What a lair's dragon is today, read in the code.** (1) A world-generated lair dragon joins untagged; `DragonBridge.reportUntagged` names its lair (`DragonBridge.java:1301-1351`); the engine's `untagged()` (`engine.rs:4775`) makes it the lair's occupant and orders ADOPT; the plugin tags it as a puppet and the next `pass()` removes it when its chunk is not entity-ticking (`DragonBridge.java:852-855`, Z14). So the roost's own dragon is gone from the world after its first load, never saved (`EntitySaveMixin`), and exists only in the engine until a player's area covers the lair (`Ev::SeenAtLair`, `engine.rs:1559`), when `materialize` makes a sleeping puppet. (2) The lair's ledger row is K 1, count = occupants, away 0, no bodies (`lair_site_state`, `engine.rs:732-758`); a row with dragons home waits the cap, "dragons are puppets until EQ-2" (F12, MZB-1031, `engine.rs:934-947`). (3) Reusable as built: the engine's `adopted` map and `adopt_again` (`engine.rs:439`, `4876`), the replay gate's honouring of an embodied dragon, `press`, `kill`, `remove`, `leave_lair`, `occupy` (`engine.rs:976`, `4198`, `4310`, `4367`, `4390`), the plugin's puppet registry and its hooks for damage, death, meals and state, core's `SiteLedger`, `FoundRow` bodies lists and `KillCredit`. | S5 starts from this, and the first line is a gap S5 closes beyond UR-1: today even a lair nobody has uninstalled loses its roosting dragon from the saved world. |
| EQC102 | **What the Ice and Fire CE 2.1-beta.1 jar says, class and line** (CFR 0.152 decompile of `iceandfire-ce-o6K8vx54.jar`: `nix run nixpkgs#cfr -- <jar> --outputdir <dir> --jarfilter 'com\.iafenvoy\.iceandfire\.(entity\|world\|registry\|config\|util\|loot\|event)(\..*)?'`; `DBE` is `entity/DragonBaseEntity`, `Logic` is `entity/util/dragon/IafDragonLogic`). **No hard leash:** `restrictTo` is never called for a dragon; `getRestrictCenter`, `getRestrictRadius` (config `wanderFromHomeDistance`, 40) and `hasRestriction` are overridden (`DBE` 363-377) but vanilla's `Mob.isWithinRestriction` reads the private `restrictRadius`, -1 for a dragon (`Mob.java` 1304). Ground wander hops 10 blocks (`DragonAIWanderGoal`), flying wander picks points within 40 of home, with an unbounded radial fallback (`DragonUtils.getBlockInView` 68-96); hunting follows prey up to `targetSearchLength`, 128 (`DBE` 353-356). **Night:** a dragon with a home more than about 5 blocks away flies back to it to sleep (`Logic` 36-46, `DragonAIReturnToRoostGoal`); a lightning dragon sleeps by day (`LightningDragonEntity` 101); a dragon with no home sleeps where it stands. **Cave dragons have no home after any save:** `createDragon` sets `homePos` but never `hasHomePosition` (`DragonCaveStructure` 251-267) and the NBT writes the home only with it (`DBE` 806-810); roost dragons have both (`DragonRoostStructure` 288-300), the home being the start's pivot, whose Y is the box's lowest, not the surface (`StructureStart.java` 86-88). **Generation:** one dragon per start, created in `postProcess` in the pivot's chunk and written straight into the proto chunk's entity NBT (`DragonRoostStructure` 98-107, `DragonCaveStructure` 124-139, `ProtoChunk.java` 190-195); no "spawned" flag exists in piece NBT or chunk data. Boxes: roost 19 to 29 wide and 4 tall, cave 24 by 24 and 24 tall. **Entity type:** all three `sized(0.78, 1.2).clientTrackingRange(256)`, update interval 3, only fire is fire immune (`IafEntities` 85-87, 129-143); the view distance caps tracking (`ChunkMap.java:1327-1335`: the player's view distance times 16, capped by the server's), 160 blocks at 10. **Persistence:** `isPersistenceRequired` and `requiresCustomPersistence` are true (`DBE` 1345, 1349); nothing in the jar removes a living dragon by distance, difficulty or mode (`DBE` 603-607, 640, 1278, 2405). **Save:** every `addAdditionalSaveData` calls `removeParts()` (`DBE` 817-818) and the next tick makes new parts; orphan parts discard themselves (`MultipartPartEntity` 136-172); `Flying` and `Hovering` are saved, the flight target is not (`DBE` 801, 803, 835-836), and a reloaded flyer picks one on its first tick. **Death:** `dropFromLootTable` is empty (`DBE` 2338); the loot is the corpse's harvest (`mobInteract` 1263-1300), the corpse stays until `DeathStage` reaches age/5 days (a dragon under 5 days is removed at once, `tickDeath` 635-650) and is saved (`DBE` 798-800, 830-833); `isAlive()` is true for a corpse (`DBE` 1082-1087). **Taming:** only the Creative Dragon Meal tames a wild dragon (`DBE` 1111-1124); a hatchling is born tame with its owner (`DragonEggEntity` 160-190). **Hurt and waking:** a sleeping dragon wakes and, untamed, targets a player who hurts it (`DBE` 1623-1646) or comes within about 7 to 9 blocks (`DBE` 2223-2230); a hoard pile broken or a chest opened within 30 blocks wakes dragons (`ServerEvents` 355-399). **Age and hunger:** `AgeTicks` +1 a tick unless `AgingDisabled`, capped at 128 days (`Logic` 197-202); `Hunger` -1 per 3,000 ticks while the entity ticks (`Logic` 204), hunting below 90. No other per-dragon store (`DragonPosWorldData` is client-triggered and empty on a server, `DBE` 2703). | The design depends on each: the leash exists only if the plugin makes it, a cave needs a home written, a live body is synced with setters (`IafDragons.write` calls `addAdditionalSaveData`, which discards the parts), corpses are persistent and loot-bearing, and a loaded lair's dragon hunts on its own. |
| EQC103 | **What the precedent gives and what S5 does not copy** (MOWZIE §4.9, S3's camps). **Taken as built:** a saved attachment names the body (`CAMP_MEMBER`); in and out follow chunk ticket levels with a poll as the authority and `areEntitiesLoaded` (`Camps.java` 60-69, 90, 101, 1138-1150); the census matches a body by UUID whatever its tags say and retags it (MZB-1014); a missing body is a player's loss when rtsim ran through and Other after a gap (EQUILIBRIUM §2.3, MZB-1024); three misses over two days before a write-off; a stray fills a free slot or is removed; orders apply only while the site is in and each is safe to apply twice; the engine books only what Java reports (I6). **Simplified:** no slot ledger (a dragon has one `body`, a lair at most two), no CAMP_RECONCILE-shaped order (existing ADOPT, MATERIALIZE and DISCARD carry the lines, plus one HAND_BACK), no Rule U (a boss body is never removed in a fight: here a body a player is within 128 blocks of is recalled, not removed, EQC115), no parties, no chief. **Left out:** records riding on entities and markers and the barrier learning entity chunks (F4, EQC143). | A lair is K 1 (or a pair), not a pool: the slot ledger and the reconcile order are for N members and parties, and cost the most to build. What a crash can still do is written down (EQC143). |
| EQC104 | **Ids checked free** on this head and every local branch (`git grep`): dragon kinds 0x0510 to 0x0512 (journaled observations) and 0x850A (command); the dragon ranges 0x0500 to 0x05FF and 0x8500 to 0x85FF are reserved (`boundary.md` §6.1), and the last used are 0x050F and 0x8509. Core's 0x8006 (SITE_STATE), 0x0132 (MODULE_SWITCH) and the conflict engine's 0x880B (BAND_MARCH) are other ranges and stay. MATERIALIZE flags used are 1, 2, 4, 8, 16: 32 is free. ADOPT flags: 1 is used, 2 is free. DEMATERIALIZED reasons: 1 to 6 are used by the shared table (`boundary.md` §2.3, and by the conflict and herd records); 7 and 8 are free in every module, dragons take 8 and leave 7 to the shared table's next code. DISCARD reasons 1 and 2 are used. MATERIALIZED aux is 0 today. The EQC range 100 to 149 is S5's. | The task's check, and the rule that a number is never reused. |
| EQC105 | **Two tags, one entity, swapped in place.** A *puppet* carries `mic_rtsim:agent` (core; `EntitySaveMixin` stops vanilla saving it). A *home body* carries the plugin's `mic_rtsim_iceandfire:home` attachment {lair key (long), agent (long)}, serialised with the entity, with `setPersistenceRequired` and `AgingDisabled`, and no agent tag, so vanilla saves it. A body is never both. The plugin swaps them on the same entity, never replacing it: home to puppet at the leash (EQC109), puppet to home at a hand-back (EQC110). Invariants: **H1** a dragon has at most one body. **H2** a lair holds at most two home bodies. **H3** the engine books only what Java reports it did or found (I6). **H4** no order is applied twice (a line names a target state). **H5** a body that stands and that the engine does not believe is a stray, and is adopted or removed by EQC113 and EQC115, never left to tick beside the dragon's other body. **H6** a puppet is never saved, so a body that leaves its lair can never be left standing elsewhere by vanilla. | One entity and two tags is the smallest change that gives EQ-2 (a): the saved body is the same body the player sees, and H6 is what removes the camps' stray-by-luring problem (EQC109). |
| EQC106 | **Who has a home body: the first two occupants of a lair by age rank** (the older first, ties by handle), the row's K being 2 (EQC120). A third occupant (a pair's young, M1) has none while two stand: `SeenAtLair` materializes it as a puppet, as today, so H2 holds. | A pair is the most a lair holds in practice: a pair forms only when a dragon claims an occupied roost, and newcomers, dispersers, starve-leavers and visitors take empty ones (`claim`, `respawn`, `vacant_lair`), so lairs with three occupants arise from breeding alone. The ledger's lists are bounded by K. Fork F7. |
| EQC107 | **The engine's belief is one field.** `Dragon.body: Option<Uuid>`: the dragon's home body, believed standing in the world (loaded or saved). With the state: **Roosting, Some** = home, standing (or loaded and Embodied); **Roosting, None** = owed; **Flying or Hunting, Some** = away with its body left behind at the lair; **Embodied, Some** = a loaded home body in control; **Embodied, None** = a puppet. Set from Java's answers only: MATERIALIZED with aux 1 (a spawn, an adoption, a hand-back), DEMATERIALIZED reason 8 keeps it, any other capture, DRAGON_LEFT_HOME, a death or a removal clears it. Never inferred. | The existing `adopted` map names a body by UUID for a dragon the engine met untagged; for a lair's dragons the engine needs the reverse, dragon to body, for the census and for rows, and a lair has at most two. |
| EQC108 | **Who is authoritative for a value.** At every ADOPT of a home body (a census finding it, a hand-back) the engine's closed forms go onto the body: age (`age(at)`, the engine owns age and the body has `AgingDisabled`), hunger (`fullness_at`), health (`health_at`), the lair's home position with `hasHomePosition` set (a cave dragon has none, EQC102), sleeping from I&F's own rule. A *live* body is written with setters (`setAgeInTicks`, `setHunger`, `setHealth`, the `homePos` fields), never `IafDragons.write` (it calls `addAdditionalSaveData`, which discards the parts, EQC102). While loaded the body is authoritative and the engine hears it as it hears a puppet: DRAGON_STATE every 100 ticks, HURT, ATE, DIED. A home body's engine row is `Embodied`, so no hunt, no depart and no leg is decided for it. | A dragon's hours unloaded are the engine's, and the body shows them when it loads: hunger it fell to, health it regained. While loaded I&F's AI is the dragon, as for a puppet today. |
| EQC109 | **The leash: a home body that goes 48 blocks from its lair becomes a puppet.** Fork F1, **FOR BEN** (1). Java checks each loaded home body every 20 ticks (EQC130); beyond `homeLeash` (48 blocks horizontally, or another dimension) it swaps the tags in place and journals DRAGON_LEFT_HOME. The engine clears `body`, the dragon stays `Embodied` at its epoch, and from then on it is a puppet in every rule: captured when out of range (`pass`, `capture`), returned home abstractly (`resume`), owed a body at the lair afterwards. **Taken:** the dragon stays the lair's occupant and nothing is lost: luring a dragon away costs the lair nothing by itself (a kill, a tame or any other loss while it is away costs pressure as always, `press` reads the dragon's lair). Options: (a) this; (b) this plus `P += 1` at the leash; (c) EQUILIBRIUM §2.3 literally: leave a vanilla stray where it unloads and book a missing body at the lair (the den rule, `aLuredBodyIsAPlayerLossAndAStraysDeathCounts`). | (a) follows EQ-2 (a), "a puppet when it leaves", and removes the stray: a puppet is never saved (H6), so no lured body is ever left to loot while the lair refills; a dragon that is still the occupant cannot be farmed by luring. It differs from EQUILIBRIUM.md §1.1 and §2.3's sentence that luring costs the site what a kill does, which is written for bodies vanilla would save away from home. |
| EQC110 | **Hand-back: a settled puppet at its lair becomes a home body.** Fork F6. The engine reads each Embodied puppet's DRAGON_STATE: when the dragon has a lair that is in (EQC111), the position is within `hand_back_blocks` (24) of the lair's home horizontally and 16 vertically, and the state is not flying, it orders DRAGON_HAND_BACK (agent, epoch), at most one in flight per dragon. Java swaps the tags in place and answers MATERIALIZED with aux 1; the engine sets `body`. Options: (a) engine-ordered from STATE, as taken; (b) none: a puppet is captured when the player leaves and its body is spawned at an unwatched census; (c) the plugin decides. | Without it a lair a player lives beside, whose chunk never unloads and is never "unwatched", keeps a dragon that arrived in view as a puppet for ever, which an uninstall loses (UR-1's exposure). It is one command and one reused answer. |
| EQC111 | **The engine's rows.** `Dragon` gains `body: Option<Uuid>` and `miss: (u8, Tick)` (the misses of EQC114). `Lair` gains `left_behind: Vec<Uuid>` (at most 4, the oldest dropped), `missing: Vec<Uuid>` (at most 2, the oldest dropped, S3's rule), `expect: Vec<Uuid>` (at most 2, EQC119), `miss: (u8, Tick)` (for `expect`), `gap: bool` (the found tail's GAP), and, not saved, `in_epoch: u64` (0 while out), set by LAIR_CENSUS and cleared by LAIR_OUT and every session start. `DragonStats` gains counters (EQC137). `DragonTunables` gains `home_bodies_max` 2, `miss_writeoff` 3, `miss_span_days` 2, `hand_back_blocks` 24 and `watch_blocks` 128 (the plugin's `watchBlocks` is the larger of that and the tracking distance, EQC128). Every change of `body`, `left_behind` or `missing` marks the lair's ledger row (`track_lair`). | The lair is the site: its books follow its bodies, as a camp's did (MZB-1014). |
| EQC112 | **The census reconcile** (table below the records). LAIR_CENSUS arrives with the lair's epoch; the engine voids lines of an earlier epoch, marks the lair in, and compares each of the lair's dragons with the bodies listed. Lines are ordered removals first (DISCARD), then adoptions (ADOPT), then spawns (MATERIALIZE), so a removed body and a spawned one never coexist. No line is sent for a lair that is out or for a replayed census (EQC123). | The §4.9 reconcile with five rows instead of slots, and its ordering rule for the same reason (H1). |
| EQC113 | **Strays and a lair's first dragon.** A body listed with no engine dragon (an untagged I&F dragon in the lair's box, the roost's own from worldgen; a body whose agent the engine has no dragon for, after a reset or a stripped attachment; a UUID in `expect`) becomes an occupant while the lair holds fewer than two: `add_dragon` with the body's species, age, hunger, gender and variant as `untagged()` does today, `occupy`, then ADOPT with flag 2 (EQC108), and a lair found late with no row (`found_unknown`) that stands empty has its pressure reset to 0 as `untagged()` does (MZB-1012: its history was lost, not its dragon). Above two, or of another lair's box, DISCARD reason 2 as today. The engine's lair branch of `untagged()` is deleted: the plugin reports in-box dragons through the census only (EQC127), DRAGON_UNTAGGED's lair field is always 0, and its tests move to the census (EQC140). | The census is the one place that knows a lair's bodies are all in. A dragon outside every lair box (a spawn egg's, an In Control one the engine adopts) keeps the untagged path and stays a puppet. |
| EQC114 | **Missing.** A dragon Roosting with `body Some(u)` that the census does not list has missed; `miss` counts it. At the third miss at least two game days after the first (the camps' numbers: 3 misses and 2 days, F1 in docs/replay-gate.md) the body is written off: `body` clears, `u` goes to the lair's `missing`, the dragon is lost to the sim (`w.knowledge.die`, `remove`), and **a player's loss** (`pressure += 1`, `player_losses`) when rtsim ran through, **Other** (no pressure, the wait is the cap's, `cap_next`) when the lair's `gap` is set or the plugin's census says rtsim was absent. The lair's wait starts from there (`leave_lair`). A census that lists the body resets the misses. A UUID in `missing` listed again while the lair is short is home again: a dragon is made from it as a stray (EQC113), which cancels the replacement wait (`occupy`); after the lair has refilled it is a stray left standing (not removed, S3's F3, MZB-1022), and its death adds `1/K` to its old lair whatever killed it (S3's F4, MZB-1023, via DRAGON_DIED's UUID, EQC121). | EQUILIBRIUM §2.3 as built for camps. With the leash a body can vanish only by something other than play (another mod, an admin, a corrupted chunk, a crash window, a body moved while rtsim was absent), which is when a census should say so. |
| EQC115 | **A dragon away with a body standing.** The lair's census lists the body of a dragon that is Flying, Hunting or an Embodied puppet (its trip began while the lair was out, or the lair was unloaded). **Unwatched** (no player within `watchBlocks` of the lair's home, the census flag): DISCARD reason 3, `body` clears, nothing is seen. **Watched:** the engine **recalls** the dragon: its leg is cancelled and it roosts at the lair (`cancel_leg`, `roost`), then ADOPT: a dragon its roost is watched at has been home all along. Its trip's events are stale by `plan`; a hunt already resolved stands. Options (F5): (a) recall, as taken; (b) hold the removal and retry at the next census, so the dragon is away while its body is seen sitting there; (c) remove it in view. A puppet that is away and watched elsewhere is not recalled: its body is removed. | A body must not vanish in front of a player, and must not coexist with the same dragon flying elsewhere (H1). A recall happens when the lair comes in with a player within `watchBlocks`: a login, a teleport, or an approach along an axis (it comes in at 112 to 144 blocks). A diagonal approach (in at up to about 190 blocks) finds the roost empty and the dragon flies in later (`Seen`). It happens only while the dragon is on a hunt, and it undoes nothing the sim booked. Not Ben's. |
| EQC116 | **Owed bodies: where and when.** Two paths, one rule. (1) `Ev::SeenAtLair` (a player's area covers the roost): a Roosting dragon with `body None` materializes **as a home body**, MATERIALIZE flag 32 (the old puppet path with the new flag), at the edge of the player's area as today; a Roosting dragon with `body Some` is **ignored** (its body is its embodiment, the census finds it). (2) A census with the lair in and **no player within `watchBlocks`** of its home (flag clear) spawns the same for every owed dragon of the lair: a lair that never unloads (a chunk loader, spawn chunks) gets its body at its next census, at most `census_period` later. Both spawn at the lair's home position, sleeping or sitting as I&F rules, through the spawn budget; the plugin answers SPAWN_REJECTED ALREADY_EMBODIED if a loaded body already carries the dragon's agent (H1). A rejected spawn leaves the dragon owed. Fork F13. | EQUILIBRIUM §2.2: an arrival is never placed into a watched site, and this is not an arrival: the dragon was at its roost, and path (1) is the edge-of-area materialization that exists today. Path (2) is the "site that never unloads" rule applied to lairs. |
| EQC117 | **When an embodiment ends, and what the engine never tells a home body.** A home body's embodiment ends at (a) its lair going out (the plugin sends DEMATERIALIZED reason 8, HOME_SAVED, per loaded body, then LAIR_OUT), (b) the body leaving the level by unload with no LAIR_OUT yet (the same record), (c) the leash (EQC109), (d) its death. Reason 8 means "the body stays and vanilla saves it": the engine takes health and hunger as for any capture, keeps `body`, and calls `resume` (a dragon within 32 blocks of its lair roosts, else flies home). The engine never orders a home body to depart: `Obs::State`'s hunger rule and the wound rule apply to puppets only (`body None`), and `remove` (a dead-of-age or starved-away dragon) sends DEPART reason 5, which on a home body **discards** it. A home body in a chunk that stops entity-ticking is not captured: it stays Embodied, frozen, until the lair goes out. | I&F's AI is the dragon while the body is loaded, the engine's hunts are for the hours it is not. Nothing has to be flown away that vanilla will save. |
| EQC118 | **Deaths, losses and left-behind bookkeeping.** A home body's death is today's DRAGON_DIED path (the body is registered as a puppet with `home`): core's kill credit, the player's loss through `LossCause::from_credit(.., boss)`, the killing report and grudge for a player's kill. The corpse is **not** left behind: it is the loot, saved by vanilla, its attachment removed when the engine acknowledges the death (`untagCorpse`, Z6), and a corpse that loads still carrying the attachment (a crash before the acknowledgement) loses it locally and reports nothing. A dragon that leaves the lair other than by death (`starve_leave`, `disperse`, `respawn`'s heir, `relocate`, a migration) or dies of age, or is removed while away, pushes its `body` to the old lair's `left_behind` (`leave_lair`, `remove`) and clears it. A tame (A10) is DRAGON_DIED kind taken as today and the plugin removes the attachment: the body is the player's. | The kill credit and the corpse are S3's and Z6's, unchanged. A body is left behind only when the sim no longer counts it, so its removal costs nothing and nobody sees a loot corpse vanish. |
| EQC119 | **Reset and reinstall.** A reset makes the engine start fresh; the ledger keeps the lair's row (S3), and now the row's `bodies.home` names the home bodies. `found_lair`'s Row arm: `expect = row.bodies.home`, `gap = row.gap()`, the dragons are not recreated, the wait is the cap's with the row's pressure (F12 stands as the safety wait), and **the lair's census adopts each expected body as an occupant (EQC113), which `occupy` turns into no wait**. An expected UUID not listed counts as a miss against `expect.miss` and is dropped at the third over two days; the lair's wait runs on to the cap, no pressure added (Other). A found tail's `left` and `missing` UUIDs come back into `left_behind` and `missing`. A reinstall with no reset keeps the pages: `body` is believed, the attachment was stripped by NeoForge while the plugin was away (it logs and skips it), the census matches by UUID and the plugin retags (ADOPT). **After either, no dragon is minted, none is duplicated, and a roosting dragon is where the world left it** (`uninstallKeepsRoostingDragons`, `resetFindsTheRoostingDragonByCensus`). Away or abstract dragons are lost at a reset or an uninstall, and book Other at the cap, as S3 does. | §2.3's reset and uninstall paragraphs for lairs: "the roosting dragons stay, as Ice and Fire left them". |
| EQC120 | **The lair's ledger row.** `lair_site_state` (`engine.rs:732`) sends K **2** (a pair, EQC106), count = occupants, away = occupants with `body None`, `bodies.home` = their bodies, `bodies.left_behind` and `bodies.missing` = the lair's lists (each bounded to K by the encoder). The census (`KindCensus`, `dragon.lair`) keeps K 1 for pressure and fill: the row's K is the body bound, not the site's capacity. `found_lair` reads the tail's lists back (EQC119). No change to the ledger file's version or the Java `SiteLedger`: it already stores K and the three lists (MZB-1001). | The ledger's lists are bounded by K and a pair is the most a lair holds; setting K 2 changes no rule of S4. |
| EQC121 | **The records** (layouts below the table; offsets include the 24-byte journaled prefix for observations). **LAIR_CENSUS** 0x0510, journaled: the lair, the epoch, flags (1 WATCHED), the bodies found as 80-byte entries (UUID, agent from the home attachment or 0, species, flags, age, hunger, health, gender, variant, position), at most 16. **LAIR_OUT** 0x0511: lair, epoch. **DRAGON_LEFT_HOME** 0x0512: agent, epoch, dimension, the body's UUID, position. **DRAGON_HAND_BACK** 0x850A (engine to Java): agent, epoch. **DRAGON_DIED** grows by the body's UUID and its lair key (80 to 104 bytes), as CAMP_DEATH grew by a UUID (MZB-1030). **DRAGON_ADOPT** grows by age, hunger and health (64 to 80). | One census, one end-of-epoch, one swap each way, and the two growths a stray's death and a synced adoption need. |
| EQC122 | **Flags, reasons, aux.** MATERIALIZE flag 32 HOME_BODY: the entity gets the home attachment instead of the agent tag and is persistent as a body, sleeping or sitting by flag 4. ADOPT flag 2 HOME_BODY: tag as a home body and sync (EQC108). **DRAGON_MATERIALIZED aux bit 1 HOME_BODY**: the answer says the body it made, adopted or handed back is a home body (the engine sets `body`). **DEMATERIALIZED reason 8 HOME_SAVED.** **DISCARD reason 3** (its dragon is away, EQC115), **4** (a second body of one dragon, EQC112). DRAGON_UNTAGGED's lair field: always 0. | Each is a bit or a code the record already has room for. |
| EQC123 | **Journal and replay.** LAIR_CENSUS and LAIR_OUT are journaled and ingested live; while a session's replay runs a census only marks the lair in or out (no line, as a replayed CAMP_CENSUS only marks its camp out, RP-64), and at a session start every lair is out and its lines void. DRAGON_LEFT_HOME names an agent and epoch, so `Obs::agent_epoch` folds it (the match is exhaustive: the build must add all three arms). The new command's gate verdicts (`outbox.rs`, exhaustive, no `_` arm): HAND_BACK queued Drop, replaying Drop (it orders a body's tag swap: a body of this session). Materialize stays Q::Drop, R::Never, Adopt Drop and Drop, Discard Keep and Drop. A replayed DEMATERIALIZED reason 8 is applied as any capture is (`resume`). | The replay gate (docs/replay-gate.md §4.5): a replayed record is the last session's answer to its own decision; the engine decides again after the end. |
| EQC124 | **Epochs and idempotence.** The census's epoch is a Java op id minted when the lair comes in. A line names its target state: ADOPT does nothing if the body already carries the home attachment of that agent; MATERIALIZE does nothing if a loaded body carries the dragon's agent; DISCARD does nothing if no body has that UUID; HAND_BACK does nothing if the entity is not an agent-tagged puppet of that agent and epoch. Commands are per session as every dragon command (S11). An answer (MATERIALIZED, DEMATERIALIZED, COMMAND_RESULT, SPAWN_REJECTED) is booked by the dragon's own epoch check as today (`embodied(agent, epoch)`), so a stale answer books nothing. A line in flight when the lair goes out (LAIR_OUT) is applied or rejected by the plugin's own checks (a spawn needs `entitiesTickAt`; a DISCARD of a body in an unloaded chunk finds none, is answered applied, and the body is dropped from `left_behind` only at the next census that does not list it). | H4, with the camps' epoch and none of their lines' state tracking. |
| EQC125 | **Documents the build changes.** `docs/boundary.md` §6.1 (ranges are unchanged), §6.3 (LAIR_CENSUS, LAIR_OUT, DRAGON_LEFT_HOME rows; DIED and MATERIALIZED notes; LAIR_FOUND's hand-over for bodies), §6.4 (HAND_BACK; MATERIALIZE flag 32, ADOPT flag 2 and fields, DISCARD reasons), §6.5 (how Java applies each; the tag swaps), §6.6 (item 2 becomes "in-box dragons come through the census"; item 3's adopt rule), §6.7 (the engine side); `docs/replay-gate.md` §4.5's table; EQUILIBRIUM.md §2.4.1's body bullet, §2.3's dragon row (K 2) and §4's S5 row (built); `docs/overnight-choices-iceandfire.md` Z14 and Z7 (superseded for home bodies); MZB-1031's text; this section marks each record built. MOWZIE.md is not edited. | The two boundary documents change together with the codecs, as `boundary.md` §6 says of itself. |
| EQC126 | **No CAMP_RECONCILE-shaped order.** Fork F9. Options: (a) existing ADOPT, MATERIALIZE and DISCARD as the lines, plus HAND_BACK, each with its existing answer (taken); (b) a `LAIR_RECONCILE` command with typed lines and a CAMP_BODIES-shaped answer. | (a) needs no per-line state tracking and reuses the replay gate's verdicts; the lines are one to four a lair, and (b) is the form to take if a lair ever holds a pool. |
| EQC127 | **The plugin's classes.** New `HomeBodies` (the index of loaded home-tagged bodies by UUID and agent, the census, the leash scan, the swaps, the tag helpers), new `home` attachment registered beside `SPAWN_TYPE` in `MicRtsimIceAndFire` (a `DeferredRegister` entry, codec {lair, agent}, serialised). `DragonBridge`: `Puppet` gains `home`; `capture` for a home body reports and keeps it (never `discard`) unless the reason is a DEPART 5; `pass` skips the not-ticking capture for a home body; `onDeath`, `onDamage`, `state`, `ate` need nothing but the registration; `reportUntagged` skips a dragon in a lair box (its census lists it) and a dragon with the home attachment; `admit` and `left` treat a home body as live. `Lairs`: each lair registers its footprint with `ChunkWatch` on report. `MicRtsimIceAndFire.onJoin` and a new `EntityLeaveLevelEvent` branch maintain the index. `IafDragons`: `syncLive` (setters) and `setHome` for a cave. | Nothing outside the Ice and Fire plugin and the one core class changes: D4. |
| EQC128 | **Footprint, cadence, thrash, watched.** The footprint is the chunks the disc of `lairFootprint` (48 blocks) round the lair's home touches, at most 7 by 7. Only **active** lairs are polled: those whose home chunk is loaded (a `ChunkEvent.Load` and `Unload` pair, which the plugin already follows for lair finding), polled every 20 ticks in 20 slots as the camps are (`POLL_PERIOD`, MZB-290), with `ChunkTicketLevelUpdatedEvent` as a hint (C2ME replaces its poster, MZB-249). In means every footprint chunk at level 33 or lower with `areEntitiesLoaded`; the first tick it holds mints the epoch and sends the census; any chunk above 33, or a body leaving the level by hide or unload, is out at once (reason 8 per body, then LAIR_OUT). A lair that went out waits `censusGap` (200 ticks) before it can come in again, so a player on the boundary does not make a record storm. While in, the census repeats once per game day (`censusPeriod` 24,000 ticks: the daily recount of EQC114 and the force-loaded lair's arrival), and a home-tagged body read from disk outside the footprint of a lair that is in asks for one next tick. WATCHED is any non-spectator player within `watchBlocks` of the lair's home through the seams (`inAnyPlayersRange`'s topology image); `watchBlocks` is the larger of 128 (EQUILIBRIUM §2.2's rule for an arrival) and the distance a client tracks an entity at, the player's view distance times 16 capped by the server's, 160 at 10 (`ChunkMap.java:1327-1335`, `getPlayerViewDistance`), so whatever a player can see counts as watched. Fork F11 (48 against 32 or 64). | The camps' machinery for the same questions; 48 is the 40 blocks I&F flies and wanders from home (EQC102) plus a margin, and the disc keeps a census under 49 chunks. At view 10 a lair comes in when the player's chunk is within about 8 chunks of its home chunk (112 to 144 blocks on an axis, up to about 190 on a diagonal), so on an axis approach it comes in inside the 160 blocks a client tracks an entity at, and on a diagonal beyond them, in both cases before the player can melee it. |
| EQC129 | **Applying the lines.** **ADOPT flag 2:** the entity by UUID (`find`), not tame, not model-dead: attach `home` {lair, agent}, remove any agent tag, `setPersistenceRequired`, sync (EQC108), register as a puppet with `home` at the engine's epoch, answer MATERIALIZED aux 1; refused as ADOPT is (SPAWN_REJECTED 4 or 5). **MATERIALIZE flag 32:** `type.create`, `IafDragons.write` (a body not yet in a level) and the home position, attach `home`, `addFreshEntity`, journal MATERIALIZED aux 1; the existing rejections, spawn budget and `entitiesTickAt` check. **DISCARD 3 and 4:** as today (no drops, passengers set down), refused for a tame body or a puppet, applied if gone. **HAND_BACK:** swap the tags of the named puppet if it is still tagged at that epoch, set persistence, sync nothing (the puppet's state is the dragon's), answer MATERIALIZED aux 1. A removal of a body a player is within `watchBlocks` of is **not sent**: the engine's recall (EQC115) is what the census asked for in that case, so no DISCARD is sent for it. | EQC124's idempotence, in Java. |
| EQC130 | **The leash scan and the swap.** Every `leashScan` (20 ticks) the plugin walks its registered home bodies (a handful) and checks the horizontal distance from the lair's home through the seams, and the dimension. On exit it journals DRAGON_LEFT_HOME first, then in one tick removes the `home` attachment, sets the agent tag at the same epoch (`AgentTag(agent, epoch)`), keeps persistence, and leaves it a puppet in the registry (the `home` flag clears). The swap is a tag change on a live entity: vanilla's save filter (`shouldBeSaved`) reads it at the next save, so the body is not written from then on. A crash after the swap and before the chunk is rewritten leaves an old home body on disk, which the next census finds by UUID and removes or adopts by EQC112, never two. | Journal first, so a crash can only leave a stale saved body, which a census reconciles (EQC103's "never a second body"). |
| EQC131 | **Home bodies the census has not listed yet.** Between a body loading and its lair coming in (the band where it is within tracking range, 160 blocks at view 10, and the lair is not yet in) the plugin registers nothing but still must not lose what happens to it: a death of a body carrying the home attachment sends DRAGON_DIED with its UUID and the lair key (agent from the attachment, epoch 0); the engine honours a DIED whose epoch it does not hold when the UUID is a dragon's `body` or is in the lair's `missing` (EQC114), by the same kill-credit rule. A hurt is not reported. A body that carries the attachment of a lair that is not known this session is reported by the lair finder first (`Lairs.at`), as an untagged dragon's lair is. A body that changes dimension is treated as leaving the leash. | The gap between the tracking range and "in" is real (EQC128): a bow shot from 80 blocks can kill a body before its lair has been counted. |
| EQC132 | **Stop, fail, switch off.** `sessionStopping`: puppets are captured as today; **home bodies are not removed**: each gets DEMATERIALIZED reason 8 (journaled with the stop's save), so the saved world holds them and the engine's pages say so. `sessionFailed`: the same, best effort, and the registry is cleared; a home body that was not reported is found by the next session's census by UUID. `Module.ECOLOGY` switched off (`RtsimConfig.on`): the plugin stops the leash scan and the census, and every home body stays a plain vanilla entity with its attachment; switched on, each lair that is in comes in with a new epoch. | A body never becomes a loss because the engine went away: that is EQ-2 (a)'s promise. |
| EQC133 | **Uninstall and reinstall, what the player sees.** Uninstalled mid-world: the saved chunks hold every home body that was loaded or saved as one, as Ice and Fire left it (UR-1's "the roosting dragons stay"); NeoForge skips the unknown attachment with a log line (`AttachmentHolder.java:162`) and the entity is the jar's own. Lost: dragons that were puppets or abstract (away, owed), and any lair whose dragon had no body. Reinstalled: the next census adopts each body by UUID, or by the ledger's row after a reset, retagging it; a dragon that was lost comes back as a newcomer after the wait the row holds. | The loss is the camps' (MOWZIE §4.9, "what an uninstall or reset loses") narrowed to dragons. |
| EQC134 | **Existing plugin pieces, one line each.** `LairPlacer` (OG-7): a placed lair is reported PLACED and late; its roost dragon, if Ice and Fire spawns one, is a stray at the first census and is adopted (the OILGATE §4.4 line stays true). `Nests`: unchanged (worms are the nests', not lairs'). The tribute chest (`Lairs.scanChests`): unchanged. The far feed (`DragonFeedSource`): home bodies are in `forEachBody` like puppets, so DH draws them; a saved-but-unloaded body is not drawn, as an abstract dragon is not. The oil gate's visitors: puppets, never home bodies until they settle (a settled visitor's body is owed). `HoardRobbed`: unchanged. In Control's NATURAL dragons: left alone as before, except one standing in a lair box with no spawn type, which the lair's box rule already makes a structure dragon (`spawnKind`). | Where the change could have reached, and does not. |
| EQC135 | **Engine pages: RTD10 and RTN10, no migration.** Per dragon, after `sensed`: `body` (a tag byte, 16 bytes), `miss` (u8, `var_u64`). Per lair, after `found_unknown`: a flags byte (bit 0 `gap`), `left_behind`, `missing`, `expect` (each a length and 16-byte UUIDs), `miss` (u8, `var_u64`). The meta page (RTN10): `DragonStats` gains the counters of EQC137, the state hash covers every new field. `in_epoch` is not saved. A save of an older version is refused as any other (UR-1: no compatibility before a release). `rtsim-view`'s GameTest fixtures are regenerated, as S4 did (EQC30). | The same shape as EQC30. |
| EQC136 | **The attachment and the ledger on disk.** The home attachment's codec is {`lair`: long, `agent`: long}, written with `serialize(CODEC, h -> h.agent() != 0)`; it travels with the entity in its region file and is visible to an admin's `/data get entity` under `neoforge:attachments` and to nobody else. The site ledger's file (`mic_rtsim_sites.dat`, version 1) is unchanged: a lair row is K 2 and carries up to two home UUIDs, two left-behind and two missing (MZB-1001: 48 bytes plus 16 a UUID, at most 128 more). | Same visibility and same file as S3's `camp_member` and ledger. |
| EQC137 | **Stats and the S1 census.** `DragonStats` gains `lair_censuses`, `home_adopts`, `home_spawns`, `home_removed`, `home_recalled`, `home_saved`, `home_left`, `home_hand_backs`, `home_missing`, `strays_adopted`, `stray_deaths` (RTN10). `Dragons::census` keeps `dragon.lair` as is (K 1, members the occupants); `sim census` gains no kind: a body is how a member shows, not a member. The viewer and `rtsim perf` read the stats. | The census counts populations, and S5 adds none. |

### The census reconcile (EQC112)

Lair L, epoch E, bodies B listed with UUID u and agent a (0 if untagged). "Away" is Flying, Hunting or an Embodied puppet.

| Dragon of L | A body in B for it (by `body` = u, else by agent a) | None listed |
|---|---|---|
| Roosting, `body Some(u)` | ADOPT flag 2 (sync); `miss` resets; Embodied | a miss (EQC114); written off at the third over two days |
| Roosting, `body None` (owed) | a body carrying its agent stands: ADOPT it | unwatched: MATERIALIZE flag 32 (EQC116); watched: nothing (`SeenAtLair` covers it) |
| Away | unwatched: DISCARD 3, `body` clears; watched: recall (EQC115) then ADOPT | the left-behind body is gone: `body` clears, no loss (the dragon is away) |
| Embodied home body of this epoch | a second body of one dragon in B: DISCARD 4 on the one farther from the lair, the other ADOPTed | it left or was lost since: the answers of EQC117 book it |
| **No dragon** | a stray (EQC113): adopt as an occupant below two, else DISCARD 2 | n/a |
| UUID in `left_behind` | DISCARD 3, dropped from the list | dropped |
| UUID in `missing` | the lair is short: a dragon is made from it (EQC113), the replacement wait cancelled; else it stays standing | stays in the list (the oldest dropped past two) |
| UUID in `expect` (after a reset) | adopted as an occupant (EQC113) | a miss against `expect.miss` (EQC119) |

### Byte layouts (EQC121, EQC122)

All offsets as `boundary.md` §6.3 and §6.4 give them (observations after the 24-byte journaled prefix, commands after the 16-byte header).

| Kind | aux | Fields (offset type name) | Size |
|---|---|---|---|
| 0x0510 LAIR_CENSUS | n (bodies) | 24 u64 lair, 32 u64 epoch, 40 u32 dimension, 44 u32 flags (1 WATCHED), 48 u32 n, 52 reserved, then n bodies of 80: 0 u64 uuidMost, 8 u64 uuidLeast, 16 u64 agent (0: no home attachment), 24 u32 species (dictionary 3), 28 u32 flags (1 sleeping, 2 flying, 4 has a home position), 32 i32 ageTicks, 36 i32 hunger, 40 f32 health, 44 u32 gender, 48 u32 variant, 52 reserved, 56 f64 x, 64 f64 y, 72 f64 z | 56 + 80n, n at most 16 |
| 0x0511 LAIR_OUT | 0 | 24 u64 lair, 32 u64 epoch | 40 |
| 0x0512 DRAGON_LEFT_HOME | 0 | 24 u64 agent, 32 u32 epoch, 36 u32 dimension, 40 u64 entityMost, 48 u64 entityLeast, 56 f64 x, 64 f64 y, 72 f64 z | 80 |
| 0x850A DRAGON_HAND_BACK | 0 | 16 u64 agent, 24 u32 epoch, 28 reserved | 32 |
| 0x0504 DRAGON_DIED (grown) | killer kind | as §6.3, then 80 u64 entityMost, 88 u64 entityLeast, 96 u64 lair (0: unknown) | 104 |
| 0x8503 DRAGON_ADOPT (grown) | 0 | as §6.4, then 64 i32 ageTicks, 68 i32 hunger, 72 f32 health, 76 reserved; flags 1 has a home, 2 home body | 80 |

### Forks taken (each can be undone)

| Id | Question | Options | Taken | Record |
|---|---|---|---|---|
| F1 | What becomes of a home body that leaves its lair | (a) a puppet, nothing lost; (b) a puppet and `P += 1`; (c) a vanilla stray and a missing-body loss (the den rule) | **(a)**, FOR BEN (1) | EQC109 |
| F2 | What embodies a home body | (a) the lair census; (b) a per-body join report | **(a)**: one record, one place that knows a lair is in | EQC112, EQC128 |
| F3 | Where the in and out tracker lives | (a) a new core class `ChunkWatch`, `Camps` untouched; (b) a copy inside the plugin; (c) refactor `Camps` onto it now | **(a)**: D4, and a refactor of M12's `Camps` is not S5's risk | EQC103, EQC127 |
| F4 | How crash-safe the lines are | (a) a census that heals and a three-miss write-off; (b) the camps' records on entities, markers and the write barrier | **(a)**: a crash can lose a body (a dragon, a booked loss) and never duplicate one; (b) is the upgrade if the soak shows the window | EQC143 |
| F5 | A watched lair whose dragon is away | (a) recall; (b) hold the removal; (c) remove in view | **(a)** | EQC115 |
| F6 | Puppet at its lair, lair in | (a) engine-ordered hand-back from STATE; (b) none; (c) the plugin decides | **(a)** | EQC110 |
| F7 | Bodies per lair | (a) the first two by age; (b) every occupant; (c) one | **(a)** | EQC106 |
| F8 | The untagged path for in-box dragons | (a) retire into the census; (b) keep both | **(a)** | EQC113 |
| F9 | The order's shape | (a) existing ADOPT, MATERIALIZE, DISCARD plus HAND_BACK; (b) a LAIR_RECONCILE with typed lines | **(a)** | EQC126 |
| F10 | What marks a home body | (a) a plugin attachment; (b) a core attachment; (c) persistent data | **(a)**: the camps' precedent, plugin-owned (D4); uninstall skips it with a log line | EQC105, EQC136 |
| F11 | Footprint and leash radius | (a) 48 for both; (b) 32; (c) 64 | **(a)** | EQC128 |
| F12 | How a saved body is reported | (a) DEMATERIALIZED reason 8; (b) a new record | **(a)** | EQC122 |
| F13 | Where an owed body is placed | (a) at `SeenAtLair` and at an unwatched census; (b) the census only | **(a)** | EQC116 |

### FOR BEN

| Id | Question | Options | Why it is Ben's |
|---|---|---|---|
| FB-1 | **DECIDED by Ben, 2026-10-29: (a).** **A dragon lured more than 48 blocks from its lair: does the lair pay for it?** | (a) **No (Recommended):** the home body becomes a puppet and stays the lair's occupant; it flies home when it is out of sight; a kill, a tame or any loss while it is away costs the lair pressure as always. (b) The lair pays `P += 1` at the leash, kept alive. (c) The den's rule: a vanilla stray stays where it unloads and the lair books a missing body (a player's loss) and refills. | It changes a sentence of EQUILIBRIUM.md §1.1 and §2.3 he approved ("luring a creature away from home costs its site the same as killing it"), and how dragon loot is supplied. (a) cannot be farmed: the dragon is still the occupant, one kill is one loss. (c) is the literal rule, and the only one that leaves a loot body unsaved-by-rtsim somewhere away from home. **Ben chose (a): a dragon lured away becomes a puppet and stays the lair's occupant, no pressure step, no vanilla stray. EQUILIBRIUM.md §1.1 and §2.3 now say that for dragons a lure is no loss (EQ-2 (a), FB-1 (a), 2026-10-29) and keep the den rule as it is for the others.** |

Told, not asked (all inside EQ-2 (a), EQ-3 and the rules above): the ledger row's K is 2 for a lair (the census K stays 1); a lair under a chunk loader now has a live dragon (EQ-2 (a)'s consequence, see the risks); a recall can happen to a watched roost (EQC115); the plan row's fourth test is renamed (EQC139).

### Walked through: one roost, one hunt, one kill

| When | What happens | `body` and state | Records |
|---|---|---|---|
| Day 0, worldgen | the roost's dragon is created and written into the chunk's entity NBT; no engine, no census | none | none |
| Day 0, a player comes within about 8 chunks | the lair comes in; the dragon is an untagged I&F dragon in the box | none | LAIR_FOUND (existing), then LAIR_CENSUS (1 body, agent 0) |
| Same tick | the engine adopts it as the occupant and syncs it; the plugin tags it, answers aux 1 | Some(u), Embodied | ADOPT flag 2, MATERIALIZED aux 1 |
| Day 0.3, the player kills it from 60 blocks | I&F's corpse; DIED with the player's credit; the lair's `P` 1 | cleared | DRAGON_DIED (UUID, lair); the corpse's attachment removed at the acknowledgement |
| (instead) Day 0.3, the player leaves | the lair goes out: the body is saved by vanilla | Some(u), Roosting | DEMATERIALIZED reason 8, LAIR_OUT |
| Day 5, nobody there | the dragon is hungry: an abstract hunt; its body stands saved | Some(u), Flying | none |
| Day 5.5, the player returns, 220 blocks out | the lair is not in; nothing is told; the leg meets the player's area: `Seen` materializes a puppet over the prey | Some(u), Embodied puppet away | MATERIALIZE (puppet) |
| Day 5.6, the player is 180 blocks out on a diagonal | the lair comes in, unwatched; the dragon is away: DISCARD 3 removes the standing body, unseen | None, Embodied | LAIR_CENSUS, DISCARD reason 3 |
| Day 5.9, the puppet is out of sight | captured; flies home abstractly; roosts | None, Roosting (owed) | DEMATERIALIZED reason 1 |
| Day 6, the dragon settles at its roost with the player's area (160 blocks) over it | `visitors_at_lair` fires `SeenAtLair`: a home body at the area's edge | Some(u'), Embodied | MATERIALIZE flag 32, MATERIALIZED aux 1 |
| Day 6.2, the dragon chases the player 70 blocks out | the leash: tags swapped, DRAGON_LEFT_HOME; a puppet from here | None, Embodied | DRAGON_LEFT_HOME |
| Day 40, the mod is removed and the world reloaded | the saved roost holds whatever home bodies were saved; the rest were puppets | n/a | none |

### Tests (EQC138 to EQC141), each change with its failing-first test

**Rust (EQC138)**, new file `crates/rtsim-ecology/tests/home_bodies.rs` unless named, against a `FakeIaf` that gains home bodies (it applies flag 32 and 2, keeps the body on reason 8, and builds the census from its own bodies and a flag for WATCHED):

| Change | Test (names are the build's) | Failing first because |
|---|---|---|
| The census and its adoption (EQC112, EQC113) | `a_lairs_own_dragon_found_by_the_census_becomes_its_occupant_with_a_home_body`; `a_home_body_found_is_adopted_and_synced_from_the_engine`; `a_second_body_of_one_dragon_is_discarded_the_farther`; `a_stray_above_two_is_discarded`; `a_late_lair_with_no_row_resets_its_pressure_when_its_own_dragon_is_found` | the main head has no census; the old lair path adopts a puppet |
| Away dragons and recall (EQC115) | `an_away_dragons_standing_body_is_discarded_unwatched`; `a_watched_lair_recalls_its_away_dragon`; `a_recall_cancels_the_trip_and_books_no_hunt` | no such rule |
| Owed bodies (EQC116) | `an_owed_dragon_gets_its_body_at_an_unwatched_census`; `a_watched_census_spawns_nothing`; `seen_at_lair_materializes_an_owed_dragon_as_a_home_body`; `seen_at_lair_ignores_a_dragon_with_a_standing_body`; `a_spawn_for_a_dragon_with_a_loaded_body_is_rejected` | the old `SeenAtLair` makes a puppet |
| Missing (EQC114) | `a_missing_body_is_written_off_at_the_third_miss_over_two_days` (a player's loss, and Other under GAP); `a_missing_body_found_again_while_short_is_home_again`; `after_a_refill_a_missing_body_found_is_a_stray_and_its_death_counts`; `a_census_that_lists_the_body_resets_the_misses` | F12 books a body missing as nothing |
| Leaving and losses (EQC117, EQC118) | `a_dragon_that_leaves_its_lair_leaves_its_body_to_remove`; `a_dragon_dead_of_age_away_leaves_its_body_to_remove`; `a_killed_home_body_is_a_players_loss_and_its_corpse_is_not_left_behind`; `a_home_bodys_hunger_never_sends_a_depart`; `remove_discards_a_home_body` | home bodies do not exist |
| The swaps (EQC109, EQC110) | `a_home_body_left_home_is_a_puppet_and_the_engine_forgets_its_body`; `hand_back_is_ordered_only_for_a_settled_puppet_at_a_lair_that_is_in`; `reason_8_keeps_the_body_and_resumes_roosting` | no records |
| Reset and rows (EQC119, EQC120) | `a_reset_lair_expects_its_rows_bodies_and_the_census_adopts_them`; `a_body_not_found_after_a_reset_is_other_and_waits_the_cap`; `a_lairs_row_carries_home_left_and_missing_bodies_bounded_to_two`; extends `a_restart_that_loses_the_marks_changes_no_decision` | the row has no bodies |
| Formats and the gate (EQC121 to EQC123, EQC135) | the codec `all_obs` and `all_cmds` samples; `pages_round_trip_rtd10`; `the_state_hash_covers_the_bodies`; `a_replayed_census_marks_in_and_sends_nothing`; `hand_back_is_dropped_while_replaying` | new kinds |
| Invariants (H1, H2) | a property test, `no_lair_ever_holds_two_bodies_for_one_dragon`, over random sequences of census, loss, leave and swap, run with `Dragons::check` extended (no two dragons share a body, at most two per lair, `left_behind` and `missing` disjoint from standing bodies) | nothing checks bodies |

**rtsim-jni (EQC138):** `tests/protocol.rs::dragons::a_census_is_answered_with_adopt_discard_and_spawn_lines` and `a_reset_hands_a_lairs_bodies_to_the_new_engine` over the fake Java (`tests/common`, which gains `lair_census` and the answers), the host's fake Java (`rtsim-host/src/java.rs`) for the new kinds.

**GameTests (EQC139)** (`neoforge/iceandfire`, native engine only, each its own batch on its own far base as S3's, with `SiteTestHooks` for stop, gap and reset; `HomeBodies.forceCensus(lair)` and `forceOut(lair)` are the test hooks):

| Test | What it asserts | Failing first on 43062c0b because |
|---|---|---|
| `roostingDragonIsSavedByVanilla` | a lair declared over the arena with its own dragon (as Ice and Fire's roost piece makes it): the census adopts it; it carries the home attachment and no agent tag; vanilla's own entity-chunk save and read-back returns it, same UUID, same attachment; the lair's row names its UUID | the dragon is an agent-tagged puppet, removed when not ticking, never saved |
| `awayDragonIsAPuppetAndTheCensusReconciles` | the engine's dragon is sent away (a test hunt, or the body is walked past the leash): DRAGON_LEFT_HOME, the tags swap, vanilla's save does not return it; its captured state; the lair out and in again with the old body saved: the census finds the body, the dragon is away, DISCARD reason 3 removes it, and no second body exists; the dragon returns and roosts owed, and an unwatched census spawns its body | no census, no swap |
| `uninstallKeepsRoostingDragons` | with the plugin stopped (`SiteTestHooks.stop`) and the entity chunk read back without the attachment type: the dragon stands, untagged, same UUID; restarted, the census adopts it by UUID with no second body; the engine's lair is occupied, no newcomer is minted | the saved world has no dragon |
| `aDragonLuredFromItsLairIsAwayNotMissing` (the row's `aDragonLuredFromItsLairIsMissingNotDead` renamed for FB-1 (a)) | a home body walked 50 blocks off is a puppet and still the occupant; no pressure; unloaded it is not saved; it flies home abstractly | no leash |
| `aVanishedHomeBodyIsMissingNotDead` | the body is removed with no death by something other than the plugin; the census misses it three times over two days (the test's clock); a player's loss (pressure 1, the wait from it), then a replacement after the wait; with a GAP the same is Other at the cap; the UUID in the row's `missing`; the same body put back while the lair is short is home again, and after the refill is a stray whose death raises the pressure | a missing body books nothing today |
| `aHomeBodysKillCreditIsTheLairsLoss` | a player's kill, a tamed wolf's kill, a trap's kill (lava, no wild credit) each press the lair once and leave a corpse with no attachment | home bodies do not exist |
| `resetFindsTheRoostingDragonByCensus` | a plain reset (S3's hook): the new engine's lair expects the row's body; the census adopts it as the occupant; no wait is left; pressure kept | F12 gives such a lair the cap wait |
| `aWatchedRoostRecallsItsAwayDragon` | the lair comes in with a player at 60 blocks and the dragon away: no discard, the dragon roosts, its body adopted | no rule |
| `aForceLoadedLairSpawnsItsOwedBodyWhenUnwatched` | a lair under a ticket never goes out; its owed dragon gets its body at the next census with no player within `watchBlocks`, and not while one is | no census |
| `aSettledPuppetAtAnInLairBecomesAHomeBody` | a puppet landed at its lair, the lair in: HAND_BACK, tags swapped, vanilla's save returns it | no hand-back |

**Tests that change (EQC140):** the iceandfire `DragonGameTests` `alairsdragonisadoptedanditshoardrobbed` (the dragon is now adopted by the census, as a home body); `SiteLedgerGameTests` `occupied()` (its dragon is adopted by the census: it waits for MATERIALIZED aux 1 and the registry's `agentOf` still works for a home body); `tests/lair_renewal.rs` `lairs_own_dragon` (it feeds DRAGON_UNTAGGED with a lair key; it moves to LAIR_CENSUS); S4's `lair_renewal.rs` tests that adopt through UNTAGGED; the core's `DragonFeed` GameTests if they place a lair dragon by UNTAGGED.

**Long horizon (EQC141):** S4's own tests must pass unchanged with `FakeIaf` bodies on: `kill_all_dragons` (90% within 72 days at the median), `farmed_lair_yields_closed_form`, `long_horizon_dragons_and_herds_with_kills`, plus `bodies_do_not_move_the_s4_bands`, a 60-year run on the 8 by 7 ranges grid and its five lairs (seeds 1 to 30, `#[ignore]`d, the short seed 1 version always run) whose lairs-held, herds-N/K and kill-all medians stay inside S4's own bands (EQC32's table) with every lair census answered. The host soak's flatness (`who_is_alive_soak`, S11) must see the body counters flat by century.

### EQC142: cost (D1)

Engine: per dragon 17 bytes and per lair about 10 plus 16 a UUID; one pass over a lair's two occupants per census; no wheel event, no per-tick work. A census is one record per lair per epoch and one a game day while in; an out is one record per body; the leash is one record per excursion (a body becomes a puppet once, then is a puppet until captured). Plugin: the leash scan walks the registered home bodies (a handful) every 20 ticks; the in and out poll reads at most 49 chunk holders per active lair every 20 ticks in 20 slots, and only for lairs whose home chunk is loaded (a few dozen, never the thousands of lairs a session has found). Home bodies tick whenever their chunk entity-ticks (see the risks).

### EQC143: what it leaves

- **Records riding on entities, markers and the write barrier** are not built (F4). A crash between an answer reaching the journal and its entity chunk being saved can lose a spawned body (booked missing after three misses, a player's loss with the pressure that costs) and never makes a second one, since a census finds a body by UUID whatever the books say. The upgrade, if the soak shows the window: the camps' protocol, `RtsimSession.ensureReasserted` and the barrier's entity-chunk learning, which are core.
- **A third occupant has no body** (EQC106). **Hatchlings** of a pair are abstract.
- **Away and abstract dragons are lost by an uninstall or a reset** (booked Other at the cap), and so is a lair no census has reached since its dragon arrived.
- **A lair that never unloads and always has a player within 128 blocks** gets its body by `SeenAtLair` or a hand-back only (EQC116, EQC110), never by the unwatched census.
- **`Camps` is not moved onto `ChunkWatch`** (F3), so the two keep a copy of one idea until M12b or a later slice merges them.
- **Predation, hunting range and I&F's own hunting by a loaded body are unchanged**: a loaded home body hunts on its own inside the leash and is a puppet beyond it, as an embodied dragon is today; the engine's weights (EQC27) stay.
- **Oil visitors** stay puppets. **Natural and In Control dragons** stay outside the sim (R18a). **Dimensions other than the overworld** are untested.
- **Cave dragons gain a home** (the engine's lair position, set at ADOPT): today's puppets already get one by ADOPT flag 1; after a save Ice and Fire would otherwise let them roam unleashed (EQC102).

### EQC144: work split, for a builder

Each step lands its tests; `cargo test` and clippy `-D warnings` after each Rust step; the Java steps wait for the GameTest rules below. Steps 1 to 5 are Rust and need no Minecraft.

1. **Records.** `crates/rtsim-ecology/src/boundary.rs`: the three observations and HAND_BACK, flags 32 and 2, `Materialized.home`, reason 8, DISCARD reasons, the grown `Died` and `Adopt`; `Obs::agent_epoch` arms; the gate's verdicts in `outbox.rs`; round trips in the existing `all_obs` and `all_cmds` samples. Failing first: a round trip with each new field.
2. **Rows and pages.** `dragon.rs` (`Dragon.body`, `miss`; `Lair` fields), `persist.rs` (RTD10, RTN10, hash, `heap_bytes`, `check`), `DragonStats`. Failing first: `pages_round_trip_rtd10`.
3. **Engine logic.** `engine.rs`: the census reconcile (EQC112 to EQC116), swaps and hand-back (EQC109, EQC110), embodiment end (EQC117), losses (EQC118), reset (EQC119), rows (EQC120), `SeenAtLair`, `State`, `remove`, `leave_lair`, `Ev::Death`, `untagged()` (the lair branch removed), tunables. One sub-step per row of the table, each with its failing test from the list above.
4. **The fake plugin and the properties.** `synth.rs` `FakeIaf` bodies, `tests/home_bodies.rs`, the invariants test, the S4 bands with bodies on, `lair_renewal.rs` moved to the census, the replay tests.
5. **The native link.** `crates/rtsim-jni/src/engine/dragons.rs` (`journaled_kind` already covers 0x0500 to 0x05FF; the decoder arms, the command writer for HAND_BACK, the `Kept` replay of the new records), `orders.rs` (a `LAIR_CENSUS` test order beside DRAGON_COMMAND), `tests/common` fake Java, the host's `java.rs`, the protocol tests; regenerate `rtsim-view`'s fixtures.
6. **Core.** `sites/ChunkWatch` (mod-agnostic: a set of chunks, the poll in slots, the event hint, `areEntitiesLoaded`, `in(epoch)` and `out(epoch)` callbacks), a unit test over a fake chunk source; nothing else in core.
7. **The plugin.** `DragonRecords` (layouts), the attachment, `HomeBodies`, `DragonBridge` (`Puppet.home`, capture, pass, stop), `Lairs` (footprint), `MicRtsimIceAndFire` (join, leave, chunk unload), `IafDragons.syncLive`, config keys (`homeLeash`, `lairFootprint`, `censusPeriod`, `censusGap`, `watchBlocks`, `leashScan`). `javac` first; no GameTest yet.
8. **GameTests.** The ten above and the changed ones, `DragonKit` helpers, `HomeBodies` test hooks. First run each on 43062c0b to see it fail for the reason in its column, then green.
9. **Documents.** EQC125's list; mark the records built; EQUILIBRIUM.md's S5 row.
10. **Rounds.** `cargo test --workspace`, clippy, then the GameTests of iceandfire (all), core, mowziesmobs and `create -PwithMowzie` (S3's ledger tests share the clock and the ledger), each from a cleared world (`run-gametest` moved aside), one project at a time. Before any build: `cat /sys/class/power_supply/AC/online` must print 1 and `ps -eo args | grep -c '[m]odFolders=mic_rtsim_mowziesmobs'` must print 0.

### What the reading found (not edited)

- **Today even an uninstalled-never world loses the roost's own dragon.** The lair dragon is adopted as a puppet at its first load and removed when its chunk does not tick (EQC101), so a saved roost with no player nearby holds no dragon at all. S5 fixes that for every lair, not only after an uninstall.
- **Cave dragons have no home after any save** (EQC102): in the jar, not in the plan. S5's ADOPT writes one.
- **A roost's `homePos` Y is the box's lowest Y** (EQC102), not the surface the dragon stands on; the plugin's lair home (`Lairs.home`, Z5) is the surface, and S5 writes that.
- **A lair can hold more than a pair**: breeding has no cap (`breed`, `engine.rs:4470`) while the plan says "at most a pair per lair" (§2.4.1, M1). Rare in practice (EQC106), but the ledger's K had to become 2 (EQC120), and EQUILIBRIUM.md §2.4.1's "Bounds" line is not what the code does.
- **EQUILIBRIUM.md §1.1 and §2.3 say luring a creature away costs its site what a kill does.** For dragons that is not what a leash gives (FB-1).
- **The row's fourth test name** (`aDragonLuredFromItsLairIsMissingNotDead`) presumes a lured body is saved away from home; under the leash a lured dragon is away, not missing, and the missing case is a body that vanishes (EQC139).
- **MZB-1031 (F12) and Z14 and Z7 are superseded for lairs:** "dragons are puppets until EQ-2" ends here (EQC125).
- **The leash needs a record of its own** (DRAGON_LEFT_HOME): without it the engine's belief that a body stands outlives the body, and a crash then books a loss no one caused (EQC130).

### Risks

| Risk | What it would do | Mitigation |
|---|---|---|
| A chunk loader (the MIC grid loader) keeps a lair loaded | its home bodies tick for ever: two dragons with ten multipart entities each, hunting on their own inside the leash, with no player near | EQ-2 (a)'s consequence and vanilla's own behaviour for a loaded lair; the engine's census books it (`dragon.lair`), the unwatched census gives the body its place. If it costs too much, a later rule can freeze an unwatched home body (`setNoAi`) |
| The band between tracking range and "in" (an axis approach: the body is tracked at 160 blocks, the lair comes in at 112 to 144) | a body is visible and shot at before its lair has been counted | EQC131: a home-tagged body's death is reported by attachment; EQC128 puts "in" at about 8 chunks at view 10 |
| In and out thrash at a chunk boundary | a record per out and per in | `censusGap` 200 ticks; the poll is the authority, the event a hint |
| C2ME replaces the ticket event and hides entities (MZB-249, MZB-660) | an in with a body hidden, or a missed out | the camps' lessons are copied: the poll is the authority, `areEntitiesLoaded` is checked, a body leaving by hide counts as an out |
| A crash in the journal-ahead window (F4) | a spawned body lost, booked missing, a player's loss at the cap of one lair | EQC143; the upgrade is named |
| The leash cuts an I&F excursion into puppet time | a dragon that flies 50 blocks to a target becomes a puppet and is captured when the player leaves, and its saved body is replaced by a new one at a later census | one swap per excursion; the new body carries the engine's state (EQC108); no equipment or custom name exists on a wild dragon |
| S4's bands move | lairs held or kill-all times change if bodies change what the engine decides | EQC141: the bands are tested with bodies on; the engine decides nothing differently for an Embodied home body than for an Embodied puppet |
| `docs/boundary.md` and the codecs drift | a plugin and an engine that disagree | the fake plugin is the executable reference, as §6 says; the build changes both in one step (EQC144 step 1) |

### UNVERIFIED, for the build to check first

1. Whether `EntityLeaveLevelEvent` for a hidden body carries no removal reason on this NeoForge (the camps found it so, MZB-1015); the plugin's out relies on it as a second signal.
2. That a live body's `setAgeInTicks`, `setHunger` and `setHealth` refresh its attributes as the NBT read does (EQC108); if not, the sync calls `refreshDirtyAttributes` and `growDragon(0)`.
3. That `setData` and `removeData` on a live dragon, and the save filter reading them at the next write, behave as for a puppet's untag (`untagCorpse`).
4. That a cave dragon given `hasHomePosition` keeps it through a vanilla save and read-back (the NBT path reads the home only when every coordinate is non-zero, EQC102: a home at coordinate 0 is dropped).
5. The footprint size against real roost and cave boxes on a Deep Time world (EQC128's "at most 7 by 7" is arithmetic, not measured).

### EQC145 to EQC149

The build's findings (S4 left its findings after its records; S5 does the same). Built 2026-10-29 on `feat/eq-s5-dragon-bodies`, one commit per step of EQC144 (`8404239d` records, `aaeea166` rows and pages, `b6022c90` engine, `1060050b` and `508f0d4f` fake plugin and host tests, `0233ff9e` jni protocol tests, `0cf42a7f` core `ChunkWatch`, `ebdae2da` the plugin, `7852cc58` GameTests, then the fixes named below).

#### EQC145: failing first, and where it was not

Rust steps were written test first and run red before the code; the texts, for the record:

| Step | The failing text before the code |
|---|---|
| 1, records | the gate test `outbox::tests::every_command_kind_has_its_replay_verdicts`: `the drops are counted`, left `dropped: 6`, right `dropped: 5` (HAND_BACK in the sample, no verdict yet); the other new fields failed to compile (`missing field home in initializer of Materialized`, `missing fields entity and lair in initializer of Died`, `missing fields age_ticks, health and hunger in Adopt`) |
| 2, pages | `pages_round_trip_rtd10` was written beside the page code and seen green only: **no red was recorded for it** |
| 3, engine | `tests/home_bodies.rs`: 26 of 29 failed against the old engine, e.g. `one adoption: []  left: 0 right: 1` (a lair's own dragon found by the census), `a_stray_above_two_is_discarded  left: 0 right: 2`, `left: [] right: [(33, 7)]`; the three that passed red assert a no-op. After the code two failed once and were fixed in the step: `an_away_dragons_standing_body_is_discarded_unwatched` (home_bodies.rs:336) and `a_home_body_found_is_adopted_and_synced_from_the_engine` (`hunger from the engine's closed form  left: 87 right: 50`) |
| 4, fake plugin | `tests/home_bodies_long.rs` and the invariants property were written with the fake plugin and seen green only: no red recorded |
| 4b, host and jni tests that fed a lair key to UNTAGGED | red because the engine's lair branch was gone, which is the contract moving: `the lair is lived in  left: Some(Tick(1000)) right: None`; `home, no pressure  left (0, 11, 3.2630346) right (1, 0, 0.0)`; `left (0,0,0) right (3000,100,3000)`; the host: `timed out waiting for a materialization after the switch`. Moved to `LAIR_CENSUS` helpers. `gametest_worlds_open_in_the_host_with_their_saved_state` (host.rs:465) stayed red until the fixtures were refreshed |
| 6, `ChunkWatch` | first run: 19 compile errors (no Minecraft classes on `:core:test`'s classpath), then `NoClassDefFoundError: it/unimi/dsi/fastutil/longs/Long2ObjectOpenHashMap` (7 of 8 failed); fixed by making the tracker Minecraft-free (`VanillaChunks` holds the game source): 8 pass |
| 8, GameTests | **not red against 43062c0b.** The twelve GameTests and the jni tests `a_census_is_answered_with_adopt_discard_and_spawn_lines` and `a_reset_hands_a_lairs_bodies_to_the_new_engine` call classes that exist only on the branch (`HomeBodies`, the census records), so on 43062c0b they do not compile; they were written after the code and their first runs were red for the reasons below |

The first full iceandfire round failed 7 tests: `awatchedroost...`, `adragonluredfromitslairisawaynotmissing`, `asettledpuppetataninlairbecomesahomebody`, `awaydragonisapuppetandthecensusreconciles` (all four: `DRAGON_LEFT_HOME reached the engine`, because the leash and the watched rule subtracted the wrong point of `nearestImage`, a distance of 0: nothing was ever leashed and every player watched every lair; fixed in `6cc4b2bb`), `alairsdragonisadoptedanditshoardrobbed` (the test asserted a sleeping flag; the dragon is awake at the census, flags 0), and two save-wait tests that ran out at 2400 logic ticks (`resetfindstheroostingdragonbycensus`, `uninstallkeepsroostingdragons`: budget 9000, and a gap of 1200 ticks where `gapRestart(2 * DAY)` had jumped the game time past `ChunkWait`'s budget). The third round failed 3 (below, EQC147).

#### EQC146: the UNVERIFIED facts, settled by GameTests

1. **A hidden body's `EntityLeaveLevelEvent` carries removal reason `null`** (the plugin log reads `removal reason null`); a real chunk unload carries `UNLOADED_TO_CHUNK` (`aHeldLairComesInAndGoesOutByItself`). The plugin treats both as the body leaving: the lair's out is the authority, the event the second signal, as the plan said.
2. **The setters alone do not refresh the attributes.** `aLiveBodysAgeSyncRefreshesItsAttributes`: max health at 30 days 135.0; after `setAgeInTicks(110 days)` and `setHunger` alone 135.0; after `refreshDirtyAttributes()` 442.0. `IafDragons.sync` calls the refresh (`growDragon(0)` is not needed).
3. **`setData` and `removeData` on a live dragon take effect at the next vanilla write.** `roostingDragonIsSavedByVanilla`: the entity chunk read back from storage carries the dragon and `HomeTag[lair, agent]`; a puppet (agent tag, no home attachment) is not written (`awayDragonIsAPuppetAndTheCensusReconciles`: `the puppet was not saved`); a hand-back writes it again (`aSettledPuppetAtAnInLairBecomesAHomeBody`).
4. **A home survives Ice and Fire's own NBT write and read unless a coordinate is exactly 0.** `aHomeSurvivesVanillasSaveUnlessACoordinateIsZero`: `-8231361, -58, 7551959` read back equal; `0, 70, 5` read back as no home (`hasHomePosition` flag still true). The leash is the plugin's (`leashScan`), not Ice and Fire's, so a lair at a zero coordinate only loses the dragon's own roaming limit after a save.
5. **The footprint against a real roost.** A placed fire roost's box is 25 x 25 x 25 (12 blocks either side of its pivot), well inside the 96-block disc of the footprint (7 by 7 chunks at most); a held 48-block footprint of `ENTITY_TICKING` chunks came in 2 ticks after the declare (a poll every 20) and went out 2 ticks after the tickets were let go. **Not measured:** cave boxes, and any Deep Time world.

#### EQC147: where the build differs from the plan

- **The magics are `RTDA` and `RTNA`**, not the plan's `RTD10` and `RTN10`: a page magic is four bytes and those are five characters (the tenth format, `A` for ten, to keep the magic four bytes). No migration, as agreed.
- **The engine has no `watch_blocks`.** The plugin decides watched (the larger of `watchBlocks` and the distance a client tracks an entity at) and says so in the census's `CENSUS_WATCHED` flag; the engine only reads the flag. One rule in one place, one tunable fewer on the engine.
- **The test hooks are `HomeBodies.testIn(lair, watched, now)` and `testOut(lair, now)`**, not `forceCensus` and `forceOut`: a test names the lair and the plugin does the rest.
- **Core's `ChunkWatch` is split from the game source** (`VanillaChunks`), so its unit test runs in `:core:test` without Minecraft.
- **A scripted `DEPART` captures a home body**, as it does a puppet: existing tests that depart the occupant still do.
- **`DRAGON_UNTAGGED` carries lair 0 always**: a lair's dragons come by the census.
- **The recall of an away dragon at a watched roost is the Rust tests' alone** (`a_watched_lair_recalls_its_away_dragon`, `a_recall_cancels_the_trip_and_books_no_hunt`): a GameTest cannot stage a hunt with no prey. The vanished-body test has no gap variant for the same reason of time.
- **A home body's corpse is ordinary at once.** Vanilla saves a home body, so the journal's `DRAGON_DIED` (which names the body and the lair) is enough and the attachment is removed; a puppet's corpse stays tagged until the engine's pages hold the death (Z6).
- **Who keeps a body when a lair holds more than two dragons (H2):** the dragons that hold a body or are being given one first, then the oldest, then the lowest handle, at most `home_bodies_max` (2); a third dragon's standing body is discarded (DISCARD reason 2). Of one dragon's bodies (H1) the one in control stays, else the closest to the lair, and the rest go with reason 4.
- **`SiteTestHooks.requestSave` now also saves the world** (`saveEverything(true, false, true)`). The journal is trimmed only to what its file on disk holds (JB9) and a GameTest world does not save itself, so a test waiting on `savedThrough` passed only when a world save happened to fall in its window: S3's `resetDoesNotMintADragon` (4000 ticks) passed in the first iceandfire round and failed in the third (`an engine save covers the journal through 298`), right after my reset test; the budget of 9000 that my own save-wait tests took spans the 6000-tick autosave. The diagnosis is from the code (`BoundarySession.applyCommands` clamps the trim to `writtenThrough`) and the fix confirms it: in the fourth round the test passed with the handover 39 ticks after the save, where an earlier pass had waited 640. Not shown by a red run of the old hook in isolation. The hook is the tests', and every test that uses it (iceandfire S3 and S5, mowziesmobs S3) gets the save it asked for.
- **`homeBodies` in the GameTests counts one lair's bodies**, not any dragon with the attachment within 80 blocks: a failed neighbouring test's leftover body made `awayDragonIsAPuppetAndTheCensusReconciles` fail with `no second body: no home body stands`. `aSettledPuppetAtAnInLairBecomesAHomeBody` read only the chunk of the lair's centre, and the dragon had landed 3 blocks over a chunk border (`vanilla's save returns it`): it reads the dragon's own chunk too.

#### EQC148: results

Final code is `f25df8f0` plus the fixture and documents commits (no code changes after it).

| Check | Result |
|---|---|
| `cargo test --workspace --no-fail-fast -j 8` | 2309 passed, 0 failed, 28 ignored (143 test binaries); includes the viewer fixtures' host test and S5's `tests/home_bodies.rs` (29), `tests/home_bodies_long.rs`, the two jni protocol tests |
| `cargo clippy -j 8 --workspace --all-targets -- -D warnings` | clean |
| `:core:test` | `ChunkWatchTest`, 8 tests, all pass (the only unit tests in `:core:test`); run on the final code |
| GameTest round `core` (cleared world) | All 168 required tests passed. The first run failed `aColumnFormsAgainAfterTheModuleIsSwitchedOffAndOn` once (`the band appeared again`) and passed on the rerun of the same code: **a flake**, a test with a flake history (MZB-1040) |
| `create -Psec1fx` | All 266 required tests passed |
| `create -PwithMowzie` | All 271 required tests passed |
| `mowziesmobs` | All 112 required tests passed, before and after `f25df8f0` (the hook change) |
| `iceandfire` | round 1: 7 failed (EQC145); round 2: the server crashed in a GameTest (`aWatchedRoostSpawnsNothing...` called `bridge().body(agent).discard()` after `testOut` had forgotten the body: a NullPointerException in a `thenExecute`, which a GameTest server does not survive; the test now takes the entity first); round 3: 3 failed (EQC147); round 4, on the final code: **All 54 required tests passed** |

The core and create rounds ran before `f25df8f0`, which changes `SiteTestHooks.requestSave` (used by no core or create test) and iceandfire's GameTests only; they were not run again. `origin/main` had not moved from `43062c0b` at the last fetch, so there was nothing to merge.

#### EQC149: what is left

- **Footprints on caves and Deep Time worlds** (UNVERIFIED 5): measure the real cave boxes and a Deep Time lair before the 48-block disc is final.
- **The upgrade EQC143 names** (records riding on entities, markers and the write barrier) for the crash window between an answer and its entity chunk.
- **The 30-seed, 20-year soak** (`bodies_do_not_move_the_s4_bands_30_seeds`, `#[ignore]`, release) was not run; the three-year run at seed 1 is in the suite.
- **No play-drive** of a MIC pack world with a real roost and a player (the pack's loader, a Deep Time world); the GameTests and the fake plugin are the evidence.
- **The hand-back and the recall have no GameTest of a real hunt**: the hand-back is staged by moving the puppet home, the recall by the Rust tests alone.

## S6: villages and Ribbits

**Designed 2026-10-29, built on the same day** (the build's records and findings are EQC184 onward). Branch `feat/eq-s6-villages`, cut from
`feat/mowzie-m12a-s3` at 43062c0b (main d6ac9341 plus slice M12a: S3's site ledger and kill credit,
MZB-1000 to MZB-1047). The work is EQUILIBRIUM.md §4's S6 row under Ben's decision EQ-1 (a) (§5.1,
walkthrough "Wiping out a village" in §1.2): the census both ways, settlers, unloaded growth, Ribbit
refill before build, pop on spawn. Ids EQC150 to EQC199 are S6's; the records below use EQC150 to
EQC183 and EQC184 to EQC199 are kept for the build's own findings. Choices that are Ben's are marked
**FOR BEN**, each with its options and a recommendation, and are gathered after the records.

### The shape

1. Each adopted village gets one renewal row in the phase 1 world: `members` (what the sim believes
   is there, 0 allowed), `k`, `owed` (bookings Java has not yet answered), a pressure and a
   settlers' wait. The market keeps its floor-1 `pop`; `members` is the true count (EQC155, EQC157).
2. The census runs both ways. Java marks a village census COMPLETE when every chunk of the village's
   reach is loaded with its entities; the engine lowers its count only on a complete census read
   twice in a row, and still only raises it on an incomplete one (EQC150 to EQC152).
3. Pressure is what a player's kills add: 1/K for each resident killed with a player's credit, the
   kind-8 village report that already exists. Zombies, raids, dragons and takes add nothing (EQC156,
   EQC158).
4. Settlers: a village the engine has seen alive, holding fewer than 2 with at least 2 beds, waits
   `min(192, 16 x 2^P)` days (32 after a player kills all K), then a pair arrives (EQC159 to EQC162).
5. Unloaded growth: a human village below its beds grows by exponential steps at `0.01 x N x (1 -
   N/K)` a day, as bookings; Java places the bodies at the next unwatched load, never past the free
   beds (EQC163, EQC164, EQC179).
6. Ribbits: the growth clock fills an empty house first (a resident at the meeting point) and builds
   only when every house is full; pop is booked when the resident exists; below 2 a settlers' pair
   comes as for humans (EQC165 to EQC167).
7. Arrivals: a village nobody is within 128 blocks of gets its bodies at the meeting point at the next
   load; a watched village's settlers walk in from a point 96 to 176 blocks out, and growth and refill
   wait. Nothing appears in front of a player (EQC174 to EQC178).
8. Wire: no new record kinds. Flags only (VILLAGE_CENSUS flag 2, SPAWN_RESIDENT flags 4 and 8), two
   input tags, two wheel tags, three archive kinds (EQC168 to EQC173).
9. Formats: the phase 1 world snapshot RTW4 to RTW5, the phase 1 meta RTP1 5 to 6, and
   `mic_rtsim_villages.dat` gains a walkers list (additive). No ledger row (EQC158, EQC171, EQC172).
10. Answered 2026-10-29 (Ben, FB-1, FB-2, FB-4; the coordinator, FB-3): whether villages found empty at first sight (including zombie villages) are settled,
    whether every bed counts toward K, the walkers list with the core arrival API, and what a refilled
    Ribbit is.

### S6 base and reads

Read for this design, all at 43062c0b: EQUILIBRIUM.md (§1, §2.2 to §2.4.4, §2.6, §2.8, §3.2, §3.3,
§3.6 and §3.7's villages rows, §4's S1 and S6 rows, §5.1's EQ-1 and its walkthrough, §5.2);
`docs/choices-mowzie-build.md` M12a (MZB-1000 to MZB-1047, above all MZB-1026: villages and Ribbits
get no ledger row; MZB-1006 to MZB-1010: the kill credit codes on death reports, and MZB-1009's
resident bullet); the phase 1 engine (`crates/rtsim-world`: `world.rs`, `ribbits.rs`, `inputs.rs`,
`persist.rs`, `config.rs`; `crates/rtsim-jni/src/engine/phase1.rs` and `phase1/catalog.rs`; the host's
fake Java in `crates/rtsim-host/src/java.rs`); the core plugin (`village/Census.java`,
`Residents.java`, `Village.java`, `VillageRegistry.java`, `VillageAdoption.java`,
`growth/ResidentSpawner.java`, `knowledge/HurtObserver.java`, the GameTests
`LivePhase1GameTests` and `WorldGameTests`); the Ribbits plugin (`neoforge/ribbits`: the mod
`mic_rtsim_ribbits`, which registers the Ribbit culture, body and resident factory);
`docs/boundary.md` §4 and the kind ranges; `docs/overnight-choices-phase1*.md` (V9, V14 to V17, G11,
G17, G22 to G25, G32, G37, E56); `research/equilibrium-2026-10-21/model.py` (`villages`, `ribbits`)
and `results/summary.md`; `research/population-inventory-2026-10-21.md` §3 and §4. S5's dragon
design was not read (it is being drafted in parallel); EQC150 to EQC199 and the ids in "Ids S6 takes"
are the whole of S6's claim.

**What is true in the repo (file and line at 43062c0b):**

| # | Fact | Where |
|---|---|---|
| R1 | A village's population only ratchets up. The first VILLAGE_CENSUS sets it; later ones only raise it (growth books a resident before it exists, a part-loaded village counts low: V9); a census of 0 residents is ignored; the golem count is the one latest-wins field. | `phase1.rs:1309-1342`, `docs/overnight-choices-phase1jni.md:22` |
| R2 | The census input is `Input::Population`, applied as `set_population(i, pop.max(1))`: a floor of 1, and no market event afterwards. Ribbit growth does both (`set_population` and `market_event(.., Cause::Growth)`). | `inputs.rs:203-206`, `ribbits.rs:355-364` |
| R3 | A ribbit village books a house and its resident's pop at the growth clock (10 prosperous days, `growth_after_days`), before the house exists; the resident is ordered at HOUSE_PLACED, one per house; at most 8 houses (`growth_max_houses`); `houses` never falls. | `ribbits.rs:287-366`, `:392-409`, `config.rs` |
| R4 | The S1 census prints a village's members as `Settlement.pop` with K 0 (EQC5); a ribbit village's renewals are its houses. | `world.rs:735-749` |
| R5 | Adoption guesses a village's population from its structure pieces: a third (human, 4 to 30) or a half (ribbit, 4 to 20). A reset re-adopts every registry village flagged LATE, so the new engine starts every village at that guess. | `phase1.rs:1708-1745`, `catalog.rs:148-153`, `VillageAdoption.announceAgain` |
| R6 | COMMAND_RESULT removes an op from the outbox, and books `Input::WorkResult` for a work op only. A resident op's result changes nothing in the world. | `phase1.rs:2087-2108` |
| R7 | The engine's SPAWN_RESIDENT carries AT_HOUSE and the Ribbit body only; AT_VILLAGE (the meeting point, `standNear`) is used by bought works' golems. | `phase1.rs:2371-2392`, `wire.rs:747-762` |
| R8 | Java's census visits one loaded village every 100 ticks round robin; it counts untagged `Residents.kind` entities in the village box widened by 16 and attributed by home or meeting point, the beds as HOME POI records in loaded chunks inside the box, and golems. It reports no completeness. | `Census.java:37-121, 123`, `Obs.java:542-563` |
| R9 | A resident order waits in `VillageRegistry.residentOrders` (NBT in `mic_rtsim_villages.dat`) for its chunk to be loaded and entity-ticking and its entities loaded; exactly-once rests on the op id, `residentId(op)` and the carried COMMAND_RESULT. There is no villager factory (only the Ribbit's and `minecraft:iron_golem`), and no check for a watching player. | `ResidentSpawner.java`, `Residents.java:45-60`, `VillageRegistry.java:92-141` |
| R10 | A resident killed with a player's credit is RESIDENT_KILLED, and only an attacker kind PLAYER becomes `Input::VillageReport` kind 8 (a pet's kill and a death after a player's hit are the player's since MZB-1009; a FakePlayer's is not). | `HurtObserver.java:105-112`, `phase1.rs:1833-1869` |
| R11 | Villages and Ribbit villages get no ledger row (MZB-1026), because they have no pressure on main. | `docs/choices-mowzie-build.md` MZB-1026 |
| R12 | The kernel has `Pressure`, `RenewalRule` and `LossCause`; S2 and S4 booked their renewals with them; S1's `KindCensus` has `short`, `empty`, `broken`. | `rtsim-kernel/src/renewal.rs`, `census.rs` |
| R13 | `Settlement.pop` is read by the economy (consumption, treasury ceilings, the works purse's golem cap `pop / 5`) and by the conflict engine's village defence (`pop x 0.25`). | `rtsim-trade/src/build.rs`, `works.rs`, `rtsim-conflict/src/engine.rs` |

**What is true in vanilla 1.21.1 and the pack** (decompiled sources in
`~/.gradle/caches/neoformruntime`, jar `sourcesAndCompiledWithNeoForge_0c46573b...`, and the vanilla
data jar `stripServer_b0b72e7b..._resourcesOutput.jar`; paths under `net/minecraft/world/entity` unless
shown):

| # | Fact | Where |
|---|---|---|
| V1 | A villager can breed only with food: `foodLevel + inventory food >= 12` (bread 4, potato, carrot and beetroot 1 each), awake, age 0. | `npc/Villager.java:701-703`, `:100` |
| V2 | The breeding behaviour needs both villagers able to breed, takes a free HOME POI (a bed) within 48 blocks of the parent, and gives no birth without one; 275 to 324 ticks; parents age 6000 (5 minutes), the child -24000 (one day). | `ai/behavior/VillagerMakeLove.java:41-48, 65-79, 85-98, 105-120` |
| V3 | A pair is chosen among villagers within 8 blocks in the idle activity only; the schedule is IDLE from tick 10, WORK 2000, MEET 9000, IDLE 11000, REST 12000. | `ai/behavior/VillagerGoalPackages.java:165, 192-195`, `schedule/Schedule.java:24-30` |
| V4 | A bed's POI is HOME with 1 ticket (head blockstates only); the bell is MEETING, 32 tickets, range 6. | `ai/village/poi/PoiTypes.java:36-37, 43-63, 122-123` |
| V5 | A villager's death releases its HOME, JOB_SITE, POTENTIAL_JOB_SITE and MEETING_POINT tickets, so a killed village has all its beds and workstations free. | `npc/Villager.java:653-668` |
| V6 | `finalizeSpawn` sets a villager's type from the biome only for COMMAND, SPAWN_EGG, spawner and DISPENSER spawns (STRUCTURE sets `assignProfessionWhenSpawned`); a factory that creates one must set the type itself. Breeding picks the biome type 50% of the time. | `npc/Villager.java:785-798, 803-815` |
| V7 | Villagers never despawn (`removeWhenFarAway` false) and never spawn naturally: their category is MISC, which `NaturalSpawner` leaves out. | `npc/Villager.java:567`, `EntityType.java:710-712`, `world/level/NaturalSpawner.java:55-57` |
| V8 | A zombie converts a villager it kills at Normal 50%, Hard 100%, never at Easy or Peaceful; the death credit is the zombie's. A siege is a 1 in 10 chance each midnight with a player in a village. | `monster/Zombie.java:421-444`, `ai/village/VillageSiege.java:32-35, 74` |
| V9 | A walk target the pathfinder cannot reach is followed by a random step of up to 10 blocks toward it, so a far target is walked in hops. | `ai/behavior/MoveToTargetSink.java:116-141` |
| V10 | Entities of a chunk at FULL or better are accessible (TRACKED, or TICKING from entity-ticking); `getChunkNow` returns only a FULL chunk; `areEntitiesLoaded` is the entity manager's own flag. | `world/level/entity/Visibility.java:26-32`, `EntitySectionStorage.java:57`, `server/level/ServerChunkCache.java:173`, `ServerLevel.java:1702-1707` |
| V11 | About 1 village in 50 generates abandoned: every biome's `village/<biome>/town_centers` pool carries zombie variants at 2% of its weight (plains 4 of 204, desert 5 of 250, savanna 9 of 459, snowy 6 of 306, taiga 2 of 100); the villagers there are zombie villagers, so a census counts 0. | `data/minecraft/worldgen/template_pool/village/<biome>/town_centers.json` |
| V12 | Ribbits 4.1.6: `RibbitEntity.getBreedOffspring` returns null (they cannot breed) and `removeWhenFarAway` returns false; the AI is goal based on an `AgeableMob`; a new ribbit's home is where it was spawned (phase1mc, G16). | the jar's bytecode (`javap -c`), `docs/overnight-choices-phase1mc.md` |

### S6 records: the census both ways

| # | Choice | Why |
|---|---|---|
| EQC150 | (2026-10-29, slice S6, design) **The census becomes `Input::Census { key, residents, beds, complete }`** (input tag 22; `Input::Population`, tag 2, stays for scripted tests). The world applies it per village row. The first complete census of a row sets `members` to `residents` whichever way that moves it, `k` from `beds` (EQC154), `seen_r` and `peak`. A later complete census sets `members` to `residents` plus the bookings Java has not answered (EQC162) and reads the new `members` and `k` against the wait (EQC159). An incomplete census only raises: `members` goes up when `residents` plus the owed bookings (for a Ribbit village `residents` alone, EQC162) exceed it, and nothing else changes (V9 kept for the incomplete case). Every census sets `loaded_until` (EQC164). Every change of `members` goes through `set_members` (EQC155). | §2.4.3: "the sim's pop follows it both ways". Today the census of a part-loaded village counts low, which is why it could never lower (R1); the completeness flag (EQC151) is what makes lowering safe. |
| EQC151 | **Java marks a census COMPLETE** (VILLAGE_CENSUS flags bit 2, `Obs.VillageCensus.FLAG_COMPLETE`) when (a) every chunk that meets the village's reach is FULL or better (`getChunkNow` is not null) and has its entities loaded (`areEntitiesLoaded`), and (b) every walker the registry tracks for the village (EQC178) was found in the level. The reach is the village box widened by `CENSUS_REACH` 48 blocks, for the entity query only: a resident is still attributed to its village by its home or meeting point, the beds are still the HOME POIs inside the box, and a villager or Ribbit standing 40 blocks out at its job or at the water is counted. A census that is not complete is still sent. Residents are counted as today (R8: untagged `Residents.kind` entities, babies included) plus the walkers. | V10: a FULL chunk's entities are accessible, so the existing "loaded chunk" test reads all of them, and `areEntitiesLoaded` is the flag that the sections are in. Without the flag a village at the edge of the loaded area counts low and every wipe would be believed. The wider reach is a cheap guard against the margin of 16 (G25) cutting off residents who wander (V3). At most about 150 chunk lookups per census, one census per 100 ticks. |
| EQC152 | **The link filters before it logs** (`phase1.rs` `census()`, transient `Link::census_memo`, not saved): the first complete census of a village is logged at once; a complete census equal to the world's last confirmed reading (`seen_r`) is logged only as a heartbeat, at most once per 6,000 ticks per village; a complete census that differs is held as pending and logged only when the next complete census of that village reads the same number (a different number replaces the pending one); an incomplete census is logged when it would raise, and otherwise only as a heartbeat under the same throttle (a village at the edge of the loaded area still reads loaded). The lowering rule is **two equal complete readings in a row**. Fork F14: one reading (rejected: a villager standing across the reach edge flaps the count and every flap is a market event and a log line), two readings (taken), a tolerance band (rejected: a wipe of one or two villagers would never register). | The inputs are the pages' replay log (V6 of the phase 1 choices): a census each 100 ticks per loaded village would fill the 1,024-input tail every few days. A restart loses the memo and costs at most one more census interval. A drop of one needs two censuses, a wipe two as well: one census interval later than a single reading (100 ticks times the number of loaded villages, under a minute). |
| EQC153 | (Ben, 2026-10-29: (b), as written.) **A village the engine has never seen alive is never settled.** `seen` becomes true the first time a complete census counts one or more residents; settlers (EQC159), growth (EQC163) and refill (EQC165) all need `seen`. A village found empty at its first complete census (an abandoned village, V11: about 1 in 50; a village a player emptied before the engine looked or while rtsim was not installed; every village that was wiped before a reset, because a reset re-adopts them all at the adoption guess, R5) keeps `members` 0 and starts no wait until a census sees a villager in it (a cured zombie villager, a villager a player brings). The first census still sets `members` to 0, so the economy stops believing in a full village. Options, with the recommendation, are under **FOR BEN** below. | §2.3's "a site found empty with no ledger row starts at the cap wait" is written for boss kinds; MZB-1026 gave villages no row. Settling every empty village changes what existing worlds look like (dead villages come back, zombie villages heal) and that is not EQ-1's case, which is a village a player emptied while the engine watched. |
| EQC154 | **K is the village's beds for a human village, clamped to 32** (`village_k_max`): the HOME POIs the latest complete census counted inside the village box, claimed or not (V4, V5, R8); before a complete census it is the adoption guess. A village with `k` below 2 has no settlers and no growth. A Ribbit village's K is EQC166. (Ben, 2026-10-29, fork F9: every bed counts) for what a bed is: every bed in the box counts, including beds a player built there; the options are below. | §2.4.3: "K = its beds (the census already walks the village's POIs)". Vanilla's own breeding rule is any free bed within 48 blocks of the parent (V2), so a bed counted is a bed vanilla would use. The clamp keeps a bed hall from turning the sim's growth into a villager farm; 32 is the adoption guess's own ceiling (30) with room. |
| EQC155 | **`World::set_members(node, members, at)` is the one place a village's count changes.** It does nothing if the count is unchanged; otherwise it stores `members`, calls `set_population(node, members.max(1), at)` (the market keeps its floor of 1: a market with pop 0 has no targets, `inputs.rs:203`), then `market_event(node, at, Cause::Population)`: a new `Cause` with `forces_news` true, like `Growth`. A wiped village's market therefore stays alive at a near-zero target. Readers that want the true count read the row (`World::village_members`): S1's census, the link, the tests. | R2: the census path calls `set_population` with no market event while `grow` calls both, and a pop change is exactly when the flows must be read again. One function keeps the two paths the same. R13: the ceiling and the golem cap follow `pop`, so the conservation tests (EQC183) run with a wipe. |
| EQC156 | **Kills do not move `members`; a confirmed census does.** A resident a player kills adds pressure (EQC158) and nothing else at once, so twelve kills are one market event, not twelve. A villager a player takes (Carry On, a minecart or boat to a trading hall: §2.2's second case) is **not** booked as pressure: the census sees one fewer and the loss is `Other`, no pressure (fork F4: booking takes needs a Carry On hook for residents, which the Carry On plugin does not have, and mints nothing because villagers carry no loot). A zombie's conversion is the zombie's kill (V8) and adds nothing. | §2.4.3: "Zombies, raids and dragons add nothing; only player kills do." One market event per wipe is the quiet the quiescence tests want (EQC183). |

### S6 records: the row, pressure and settlers

| # | Choice | Why |
|---|---|---|
| EQC157 | **Every village has one renewal row**, `World::villages: BTreeMap<usize, VillageRow>` beside `growth`, made at `join` for `NodeKind::Village` only (bases, havens and lairs have none; a test's scripted village is skipped by the link as for every other kind). Fields: `members: u32` (starts at the adoption guess), `k: u16` (the guess), `k0: u16` (the guess, kept: a Ribbit village's K), `owed: u16` and `owed_at: u64` (bookings Java has not answered, and the tick of the oldest), `seen: bool`, `seen_r: u32` (`u32::MAX` none), `peak: u16`, `pending_houses: u16` (a Ribbit village's houses booked and not yet lived in), `pressure: Pressure`, `wait_due: u64` and `wait_stamp: u32` (the settlers' wait), `breed_stamp: u32`, `loaded_until: u64`, `serial: u32` (arrival batches so far: op identity and the growth draws). About 90 bytes a village. It is in `state_hash` and in the snapshot (EQC171). `World::resident_ops: BTreeMap<u64, usize>` holds the ops in flight. | R4, R11: the plan puts a village's state in its engine's site rows, never in a shared table (§2.2, "The interface species plans use"). One row per village is bounded by the explored world, grows with nothing, and nothing in it is a history. |
| EQC158 | **Pressure is the kernel's `Pressure` on the row, half-life 96 days** (`village_rule: RenewalRule { base_days 16, cap_days 192, half_life_days 96 }`, one rule for human and Ribbit villages). Each `Input::VillageReport` of kind 8 (a resident killed with a player's credit: R10) adds `1 / k.max(1)` at the report's day; kind 7 (a resident hurt) and every other loss add nothing. Fork F7: **no ledger row** (MZB-1026 stands); the alternative, count-only rows in `mic_rtsim_sites.dat`, was rejected for MZB-1026's own reason (it touches `Phase1` and its meta) and because a village's pressure guards no boss loot. A reset forgets a village's pressure and wait: its settlers then come after the base 16 days, and what a reset mints is two villagers. A restart keeps both (EQC171). | §2.2: only a player's loss raises pressure, so the world does not learn to empty itself. §2.3: "Villagers and Ribbits are counted, not tracked by UUID: they carry no boss loot". |
| EQC159 | **The settlers' wait.** A village is short for settlers when `seen`, `k >= 2` and its effective count is below 2 (`members` for a human village, `members + owed` for a Ribbit village, whose bookings are not yet members: EQC165). Whenever a confirmed census, a lapse or `set_members` makes it short with no wait running, one `Ev::Settlers { node, stamp }` is booked `village_rule.delay_ticks(pressure, day)` later, that is `min(192, 16 x 2^P)` days with P read now, and `settle.emptied` is archived. A running wait is not pushed out by later kills (§2.2). Whenever the village stops being short (a census of 2 or more: vanilla bred, a zombie villager was cured, a player brought some; or `k` fell below 2) the stamp is bumped and `wait_due` cleared. Examples at K 12: all twelve killed by a player, P is 1.00 (kills within minutes of each other), wait 32 days (EQ-1 (a)); emptied by a siege, P 0, wait 16 days; a village killed out again and again settles at the closed form of §3.3 (P* 0.84, 3.1 pairs a year, with a pair counted as two kills, so `RenewalRule::farmed` is read at K/2). | §2.4.3 renewal (1). The wait starts when the engine sees the village short, so it lags the last death by the census's two readings (EQC152), which is minutes against a wait of days. |
| EQC160 | **The pair.** At `Ev::Settlers`, if the village is still short, `n = min(2, k - effective count)` arrive (`Source::Frontier`; the effective count is EQC159's); `serial += 1`; each body gets op `identity_op(StreamKey(seed, SYS_VILLAGE, key).block(serial, 0)[0], slot + 1)` (`SYS_VILLAGE = SystemId::named("world.village")`), as a grown house's resident does (V14 of the phase 1 choices): the same decision after a crash replay gets the same id. A human village books them **abstractly**: `members += n`, `owed += n` (§2.2's Abstract arrival: the count changes while the site is unloaded); a Ribbit village books `owed += n` only (EQC165). One `Command::SpawnResident` per body goes out with `arrive: Settler { x, z }` (EQC161, EQC169) and the op is kept in `resident_ops`. `stats.settlers += n`, the kind's renewals rise, `settle.settlers` is archived. Nothing is taken from another village (§2.4.3), and a family from beyond the edge is a villager the player could breed anyway: no loot is minted. | §2.4.3 renewal (1); §5.2: "Settlers are a family from beyond the explored edge; no village gives any". The clamp to `k` keeps the booking under the beds (EQC179 holds the Java side). |
| EQC161 | **The family comes from the side away from the other villages.** `World::settlers_from(node)`: the unit bearing away from the centroid of the village nodes within 3,000 blocks (S4's rule for a dragon newcomer, EQC28, read on this world's nodes); with none, or with the centroid within 150 blocks, a bearing from the Philox stream `block(serial, 2)`, which is always drawn so the stream does not depend on which is used. The source is the point 192 blocks out along it, in the village's frame, and goes out as the order's x and z. | §1.2: "a family from beyond the explored edge, walking in from the nearest edge of the explored world". The engine knows the villages, not the explored edge; away from the known villages is the nearest unexplored direction it can know, as in S4. |
| EQC162 | **Bookings and results.** `Input::ResidentResult { op, placed }` (input tag 23) is booked from a COMMAND_RESULT whose op the world holds in `resident_ops` or in `houses_pending` (a house the world booked whose PLACE_HOUSE was answered NOT_FREE or FAILED), as `WorkResult` is for a work op (R6; heard on or off, never dropped by the cap, RP-300). **Placed** (APPLIED or DUPLICATE_OP): `owed -= 1`, and a Ribbit village's `members += 1` (EQC165). **Not placed** (FAILED, NOT_FREE): `owed -= 1`, a human village's `members -= 1` (the abstract booking lapses, `stats.lapsed`), a failed house gives back its `k` and `pending_houses` (the `houses` count stays booked). A complete census never undoes a booking Java has not answered: it sets a human village to `residents + owed`, a Ribbit village to `residents`. Java answers an order in the tick it places the body, so no census read after the spawn precedes the result and nothing is counted twice. An `owed` older than `owed_max_days` (192) is released at the next confirmed census: a lost order must not hold a count up for ever. Fork F2: a latest-wins target that Java tops up (rejected: the placement still needs the owed number to survive a census, and R9's op machinery already gives exactly-once across a crash). | §2.2 RENEW contract: "a settlers' op". V16 of the phase 1 choices: ops are kept until their result and resent. |

### S6 records: growth, refill and the Ribbits

| # | Choice | Why |
|---|---|---|
| EQC163 | **Unloaded growth is a run of scheduled steps, not one closed form read at load** (fork F1). While a human village is `seen`, `2 <= members < k` and no step is pending, `Ev::Breed { node, stamp }` is booked one exponential wait ahead: `-ln(u) / (g x members x (1 - members / k))` days, `g = village_growth_per_day` 0.01, `u` from `block(serial, 1)`. At the event, if the village reads loaded (EQC164) the step is dropped and a new wait drawn; otherwise `members += 1`, `owed += 1`, one order with `arrive: Growth` (ARRIVAL only, never a walk-in: EQC169), `stats.grown += 1`, renewals, `settle.grew`. Any change of `members` or `k` bumps `breed_stamp` and books a fresh wait (the exponential is memoryless, so a redraw at the new N is exact). Growth is not slowed by pressure (§2.2). At 2 villagers and 12 beds the logistic takes about 400 days to 11 villagers (`ln 55 / 0.01`), so with the 32 days of the settlers 90% comes near day 430, inside the plan's 480 (§3.6; model `grow_unloaded 0.01`). | §2.4.3 renewal (2) says "evaluated at load"; the S6 row's `kill_all_villagers` runs with nobody loading anything, and S1's census and the economy must see the growth then (§2.2: "Abstract: the count changes while the site is unloaded"). One wheel entry per village below K, none when it is full: nothing grows with time (D1). |
| EQC164 | **A village reads loaded for 12,000 ticks after each logged census** (`village_loaded_ticks`; `loaded_until = at + 12,000`). The world cannot see chunks, so the link's heartbeat (EQC152: a logged census at least every 6,000 ticks while the village stays loaded) is the signal. Java's round robin gives a loaded village a census every 100 ticks times the number of loaded villages, so the signal holds for up to 120 loaded villages. Vanilla's own breeding governs a loaded village (V1, V2), and the sim's step is dropped. Fork F11: a separate Java record for loaded and unloaded (rejected: a new kind and a Java hook for what the census already says), no gate (rejected: the sim would grow a village a player stands in, on top of vanilla). | §2.4.3: "Loaded: vanilla rules ... The census reports what is there." |
| EQC165 | **Ribbit refill before build, and pop on spawn** (forks F5, F6). When the growth clock fires prosperous (`grow`, R3), a village with `members >= 1` and `members + owed < k` gets **a resident, not a house**: one `Command::SpawnResident` with `arrive: Refill` (AT_VILLAGE and ARRIVAL, so at the meeting point as a bought work's golem is), `owed += 1`, `stats.refills += 1`, the clock restarted as after a house; no PLACE_HOUSE, `houses` unchanged. The clock's early return at `growth_max_houses` (`ribbits.rs:293-295`) is lifted for the refill: the clock runs whenever the village has room to refill or a house left to build, so a village at the ceiling of 8 houses refills after a wipe (R3: today it never regrows). Only when every house is lived in does it build as today: the house is booked (`houses += 1`, `pending_houses += 1`, `owed += 1`), but **`set_population` is no longer called at the order** (R3), and the resident is ordered at HOUSE_PLACED as now (its op, `identity_op(house op, slot)` as today, is entered in `resident_ops` when the order is queued). `members += 1` when the resident's APPLIED result arrives (EQC162): pop on spawn. A Ribbit village's census sets `members` to `residents`, so a house that never got a plot leaves no booking behind. Fork F5: abstract booking as for humans (rejected: P17's "the population steps up then", §2.4.4's fix, and Ribbits have no beds to clamp against). Fork F6: the refilled Ribbit stands in a house's resident slot (rejected for now: Java does not know which generated house is empty); at the meeting point it is homed there (V12). | §2.4.4 renewal (1) and fix. The model's refill needs a living resident (`prosper` needs `r_pop >= 1`), kept as `members >= 1`. |
| EQC166 | **A Ribbit village's K** is its adoption count `k0` plus the houses it has booked until a complete census has seen it alive; from then it is the largest complete reading (`peak`, which only rises) plus the houses booked and not yet lived in (`pending_houses`). A wiped village keeps its `peak`, so refill restores its old size and no more; a failed house takes its 1 back. The row's `k` is recomputed whenever `peak`, `pending_houses` or the village's booked houses change. | §2.4.4: "K = its houses (the adoption count plus houses grown, at most 8 more)". The adoption count is a guess from the pieces (R5); the census is what says how many ribbits a generated village really holds. |
| EQC167 | **Ribbit settlers** are EQC159 and EQC160 for a Ribbit village (`k >= 2`, the pair `Source::Frontier`, `owed += n` and `members` rises at the results). Refill needs `members >= 1`, so an emptied Ribbit village is first settled and then refilled through its houses (§2.4.4). The refill rides the prosperity clock as today (R3): if the S6 checks find `kill_all_ribbits` slower than 300 days because the economy rarely sits at target after each +1, the knob is `ribbit_refill_needs_prosperity` (default true), and the choice to flip it is recorded in EQC184 onward. | §2.4.4 renewal (2): "no Ribbit village loses anyone". |

### S6 records: wire and formats

| # | Choice | Why |
|---|---|---|
| EQC168 | **No new record kind.** The census and the orders already exist. The wire changes are flags and meanings: VILLAGE_CENSUS flags (offset 20) gain bit 2 COMPLETE (EQC151); SPAWN_RESIDENT flags (offset 44) gain bit 4 ARRIVAL and bit 8 WALK_IN (EQC169). `docs/boundary.md` §4.1's VILLAGE_CENSUS row and its "first census sets the population, later ones only raise it" sentence (§5 item 10), and §4.2's SPAWN_RESIDENT row and §4.3's text, are rewritten by the build. | No new kind is needed, and a new kind is a number to keep unique against S5's draft and the later slices (the ranges are in `docs/boundary.md` §6.1 to §13.1); flags take none (EQC170 lists every id S6 does take). |
| EQC169 | **SPAWN_RESIDENT's arrival fields** (layout and 80-byte size unchanged). ARRIVAL (4): the order is a renewal arrival; Java holds it while a player is within 128 blocks of the village's footprint (EQC174), places it at the meeting point when none is (EQC175), and clamps a human body to the free beds (EQC179). WALK_IN (8, with ARRIVAL): when the village is watched the order may come as a walk-in (EQC177); x and z are the point the family comes from (EQC161), y is 0. An order with neither flag behaves as today (a grown house's resident, a bought work's golem). A human body is the dictionary name `minecraft:villager` (DICT_BODY, by name, no fixed id), profession 0. Fork F12: a new command kind (rejected: R9's keep, resend, exactly-once and UUID machinery is all keyed on this one). | The order already waits for its chunk and survives a crash; the arrival rules are three flags on top. |
| EQC170 | **Ids S6 takes** (each checked free in `docs/boundary.md`, `wire.rs`, `Obs.java`, `Cmd.java`, `inputs.rs`, `persist.rs`, `kinds.rs` at 43062c0b and in the S5 worktree as I found it, clean at the same commit): input tags 22 and 23 (used: 1 to 21); wheel tags 6 and 7 (used: 0 to 5); `Body` persist tag 6 (used: 0 to 5); `SystemId::named("world.village")`; archive kinds `settle.emptied`, `settle.settlers`, `settle.grew`; phase 1 stats indexes 20 to 24 (`STATS` is 20 now, 25 after); the flags of EQC168. **No record kind, no `SiteKind` (MZB-1000 has 1 to 3), no `ReportKind`, no dictionary kind, no JNI export, no test order.** A full table is under "Ids S6 takes" below. | So the numbers can be checked against S5's draft. |
| EQC171 | **Formats.** The phase 1 world snapshot goes **RTW4 to RTW5** (the village rows in node order after the "growth, bases and works" section, `resident_ops`, the new `Stats` fields; `Ev` tags 6 and 7 and the changed `SpawnResident` and `Body` encodings; `check_magic` names RTW4 as an older format). The phase 1 meta goes **RTP1 5 to 6**: the kept inputs and the tail carry tags 22 and 23, and the link's snapshot part drops `censused` (the row's `seen_r` replaces it). A v5 page is dropped by `when_pages_unreadable`, no migration. rtsim-view's fixtures are regenerated. Nothing else is versioned: the dragons', herds', conflict's and ships' pages are untouched. | No backward compatibility before a release (Ben's rule; EQC16, EQC30). |
| EQC172 | (The coordinator, 2026-10-29, fork F13: as written.) **`mic_rtsim_villages.dat` gains a `walkers` list** (UUID, village, source x and z, spawn tick, deadline). The file has no version tag; an absent list reads empty, so an older file loads. `ResidentOrder`'s saved fields already hold flags and x, y, z. `mic_rtsim_sites.dat` is untouched (EQC158). An uninstall leaves the settlers as ordinary villagers and the list inert. Options are under **FOR BEN**. | The walk-in outlives the player who watched it (EQC178); something must remember the walkers across an unload and a restart. |
| EQC173 | **Archive, stats and S1.** Three kinds appended after `SETTLE_HOUSE` in `kinds!` (ids are positions, no compat): `SETTLE_EMPTIED = "settle.emptied", "Village emptied", Notable`; `SETTLE_SETTLERS = "settle.settlers", "Settlers arrive", Notable`; `SETTLE_GREW = "settle.grew", "Village grows", Routine`. `rtsim_world::Stats` gains `settlers`, `grown`, `refills`, `emptied`, `lapsed` and `kills`; the link's `nPhase1Stats` indexes 20 to 24 (SETTLERS, GROWN, REFILLS, EMPTIED, LAPSED) and Java's `NativeEngine.P1_*` mirror them. **S1's census for villages changes:** `village.human` and `village.ribbit` print members (the sum of `members`), capacity (the sum of `k`, a never-seen village at its guess), short (members below k), empty (members 0), mean P (the rows' pressure now) and renewals (arrivals booked: settlers and growth for humans, refills, settlers and houses for Ribbits). EQC4's "human villages book none yet" and EQC5's "capacity 0" are superseded. | The viewer's timeline needs the three events; the stats are what the GameTests read through the existing array. |

### S6 records: the Java side

| # | Choice | Why |
|---|---|---|
| EQC174 | **The watched rule** (`village/Arrivals.watched(level, village)`): true when any non-spectator player is within `ARRIVAL_WATCH_BLOCKS` 128 of the village's **footprint** (horizontal distance to the box, not to the bell: a house at the far edge is as visible as the centre). A SPAWN_RESIDENT with ARRIVAL and no WALK_IN is held while watched, on the order's existing `nextTry` (40 ticks, `RETRY_TICKS`) and its existing persistence in `residentOrders`; it is checked at every try. Java drops an ARRIVAL order that has waited 192 game days (4,608,000 ticks, the engine's `owed_max_days`) and answers it FAILED, so a lapse is also what an order the watched rule held for ever ends in; an order Java places after the engine released its booking is one unbooked villager the next complete census counts, bounded by the free beds. A village that never unloads (a spawn chunk, a chunk loader) is served the same way: its order is retried until the village is unwatched, which replaces §2.2's "tries again a game day later" with a cheap pass over the players every 40 ticks. | §2.2: "Never into a watched site" with a 128-block radius (TUNABLE); `Camps.playerNear` is the same loop for the conflict plugin. |
| EQC175 | **OnLoad placement keeps `ResidentSpawner.step` as it is**: the chunk loaded and entity-ticking, the entities around the spawn point loaded, `standNear` the meeting point, the persistence flag, the per-tick spawn quota, `finish` answering COMMAND_RESULT in the placing tick. ARRIVAL adds only the watch hold (EQC174) before the placement, and a human body adds the bed clamp (EQC179) and the factory (EQC176). | R9: the exactly-once path (op, `residentId`, the carried result) is the part that must not change. |
| EQC176 | **A villager factory**: `Residents.registerFactory("minecraft:villager", ...)` beside the golem's in core's static block. It creates `EntityType.VILLAGER`, moves it to the spot, sets its type from the biome at the spot (`VillagerType.byBiome`; `finalizeSpawn` does not for an event spawn, V6), profession NONE, adult, and calls `finalizeSpawn` with `MobSpawnType.EVENT` (not STRUCTURE, which sets `assignProfessionWhenSpawned`). Idle villagers take workstations from the free job sites (V5) as vanilla's do, and a farmer restarts V1's food chain; the sim owns nothing of them (C2 as EQ-1 (a) reads it): an untagged, vanilla-saved villager from its first tick. `Residents` already lists `EntityType.VILLAGER` as a resident kind with vanilla gossip, so the census counts them and, by G11 and G32 (an entry above a villager's mark is written at the census, and a new villager's mark is 0), the village's pending gossip is written into them at the next census: a village keeps what it heard about a player (UNVERIFIED on the running game; `settlersAreOrdinaryVillagers` reads their gossip to check it). | §2.4.3 body: "ordinary villagers once placed (they take professions from workstations as vanilla's do)". |
| EQC177 | **The walk-in** (ARRIVAL and WALK_IN, village watched, meeting chunk entity-ticking). The spawn point is the first of 176, 160, 144, 128, 112 and 96 blocks out from the meeting point along the ray toward the order's (x, z) whose column is entity-ticking (`isPositionEntityTicking`), has a standable, liquid-free, collision-free surface (the `MOTION_BLOCKING_NO_LEAVES` height, a sturdy block below) and is at least 128 blocks from every non-spectator player; with none, the order waits and tries again in 40 ticks. Each order picks its point afresh, so a pair usually spawns together, 2 to 3 blocks apart; each is persistent and is entered in the registry's walkers (EQC178). Steering every `WALK_STEP_TICKS` 40: a villager's brain gets `WALK_TARGET` set to the point 24 blocks toward the meeting point, and vanilla's `MoveToTargetSink` walks it, in 10-block random hops where no path reaches (V9); a Ribbit's `PathNavigation.moveTo` gets the same point (its goals are not brain activities; that it holds is for the GameTest to show). Steering ends within 12 blocks of the meeting point, where the schedule, beds and job sites take over. COMMAND_RESULT is APPLIED at the spawn: the booking was the sim's, the walk is only how it looks. Fork F10: wait for an unwatched load only (rejected: a village a player lives beside would stay dead, which EQ-1's walkthrough says it does not). | §1.2: "If Ben is nearby they walk in from the nearest edge of the explored world"; §2.2: the arrival "comes as a WalkIn if the kind can walk". 128 blocks from every player puts the spawn past a villager's default render range (a box of average edge 1.05 renders to 64 x 1.05 = 67 blocks at the default entity distance, `Entity.java:1692-1699`; a raised entity-distance setting reaches further, so the 128 is the watched rule's TUNABLE and not a guarantee), and inside the entity-ticking area (160 to 176 blocks at simulation distance 10). |
| EQC178 | **Walkers** (`VillageRegistry.walkers`, EQC172). A walker is counted by its village's census whether or not it stands in the reach, and one that cannot be found (its chunk is not loaded) makes the census incomplete (EQC151). It leaves the list when within 12 blocks of the meeting point, when it dies, or at its deadline (`WALK_TIMEOUT_TICKS` 6,000 after the spawn). A walker still walking at its deadline, or with no player within 128 blocks of it and of the village, is moved to the meeting point (`standNear`) and removed: the sim said they arrived, and nobody sees them arrive. A settler a mob kills on the way is an ordinary death, no pressure; a player's kill is a kind-8 report like any resident's. The list holds at most 2 per settlers' event. | Without this a settler whose player left would freeze 150 blocks out, be missed by every census and bring a second pair after the next wait. |
| EQC179 | **A human arrival never takes more than the free beds.** At placement Java takes the two counts the census takes, fresh: `free = beds - residents` (walkers counted). One body is placed if `free > 0`; otherwise the order is answered NOT_FREE and the booking lapses (EQC162). A settlers' pair with one free bed places one and lapses one. Ribbit orders are not clamped: the engine's `k` bounds them and a Ribbit village has no bed POIs. | Vanilla breeds only into free beds (V2): the sim must not put a villager where vanilla would not. It also caps the harm of a census that undercounts (a bed hall, a walker lost): at worst the beds fill. |
| EQC180 | **Ribbits in Java.** `Ribbits.create` is used as it is (home where spawned, V12). The profession of an arriving Ribbit is decided at placement: **merchant when the village has none living** (Java scans the village's Ribbits as the census does), else one of gardener, sorcerer, fisherman, merchant, nitwit by the op's hash. Today a grown house's resident is a nitwit (`RibbitProfessionModule.NITWIT` is the factory's default for an empty profession), which would leave a killed-out village with no merchant for ever (C23's resident merchants are how players trade there). The walk-in steers with `PathNavigation` (EQC177). A Ribbit village has no beds, so no clamp. (Ben, 2026-10-29: as written.) | §2.4.4 does not say what the replacement ribbit is; the village's trade does depend on it. |
| EQC181 | **No new kill hook.** The credit codes of MZB-1006 to MZB-1010 stand: a player's kill (his hit, his pet's, a death after his hit) is the kind-8 report (R10) and the engine's pressure bump (EQC158); a FakePlayer's is `Other` and adds nothing; a Carry On pickup or a minecart's carriage adds nothing (F4); a zombie's conversion adds nothing. | R10 and MZB-1009 already do what §2.2's first case asks for villagers. |
| EQC182 | (The coordinator, 2026-10-29: as written.) **The arrival helpers are core API**, `mic_rtsim.api.Arrivals`: `watched(level, footprint-or-point, radius)`, the standable-spot finder and the walker registry, with `ARRIVAL_WATCH_BLOCKS` 128 as the one place §2.2's TUNABLE lives. S5 (roosting dragons) and S9 (a grove's WalkIn, MOWZIE.md §4.16) can call it instead of writing a second one. Options are under **FOR BEN**. | An interface another plan follows. |
| EQC183 | **Tunables** (every one TUNABLE, PLACEHOLDER until the S6 checks). World (`WorldConfig`): `renewal` true (false is today's world, for the "births as before" tests), `village_rule` 16, 192, 96, `village_growth_per_day` 0.01, `village_k_max` 32, `settlers_pair` 2, `village_loaded_ticks` 12,000, `owed_max_days` 192, `ribbit_refill` true, `ribbit_refill_needs_prosperity` true (**false since the build, EQC184**). Link: heartbeat 6,000 ticks, readings to lower 2. Java: `ARRIVAL_WATCH_BLOCKS` 128, `CENSUS_REACH` 48, walk spawn distances 176 down to 96, `WALK_STEP_TICKS` 40, hop 24 blocks, done within 12 blocks, `WALK_TIMEOUT_TICKS` 6,000, retry 40 ticks. | One list so the build and the tests agree. |

### S6 forks

Each is in the record named; the option taken is the conservative one where I was unsure (fewest
changes to what exists, nothing minted, nothing a player sees that EQ-1 did not promise).

| Fork | Options | Taken | Record |
|---|---|---|---|
| F1 growth while unloaded | scheduled exponential steps; one closed form read at load | steps: the economy and S1 see growth with nobody loading | EQC163 |
| F2 bodies owed to Java | an `owed` count with `members = residents + owed`; a latest-wins target Java tops up | `owed` and ops: exactly-once is already there | EQC162 |
| F3 when a kill lowers the count | at the kill report; at the confirmed census | census: one market event per wipe | EQC156 |
| F4 a villager a player takes (Carry On, a vehicle) | pressure through a new Carry On hook; `Other`, no pressure | `Other` | EQC156 |
| F5 a Ribbit's pop | at the order (as humans); when the resident exists | on spawn (P17, §2.4.4) | EQC165 |
| F6 where a refilled Ribbit stands | a house's resident slot; the meeting point | the meeting point | EQC165 |
| F7 where a village's pressure lives | the engine's row; count-only ledger rows | the row (MZB-1026 stands) | EQC158 |
| F8 villages found empty, zombie villages | settle after the cap wait; settle after the base wait; never settle a village not seen alive | never (Ben, 2026-10-29) | EQC153 |
| F9 what a bed is | every bed in the box, cap 32; beds that existed at adoption; the adoption guess | every bed, cap 32 (Ben, 2026-10-29) | EQC154 |
| F10 a watched village's settlers | walk in; wait for an unwatched load | walk in (EQ-1's walkthrough) | EQC177 |
| F11 the loaded signal | the link's census heartbeat; a Java loaded record; none | heartbeat | EQC164 |
| F12 the order | SPAWN_RESIDENT with flags; a new command kind | flags | EQC169 |
| F13 remembering walkers | a registry list; an entity attachment; none | registry list (the coordinator, 2026-10-29) | EQC172 |
| F14 when the count may fall | two equal complete readings; one; a tolerance band | two readings | EQC152 |
| F15 an arriving Ribbit's profession | nitwit as a grown house's; merchant first then by the op; random by the op | merchant first (Ben, 2026-10-29) | EQC180 |

### FOR BEN

Everything else in S6 can be undone and is decided here. These are what a player sees in his
villages or a format or interface another plan follows. **Answered 2026-10-29: FB-1 (b), FB-2 (a),
FB-4 (b), by Ben; FB-3 (a) by the coordinator, my recommendation (an internal interface, reversible
before a release).** Each question is kept below with its options.

**FB-1 (EQC153, F8), DECIDED by Ben 2026-10-29: (b). What happens to a village the engine finds with no villager at first sight?**
This covers about 1 village in 50 (abandoned villages, V11), villages players emptied before the engine
looked, and every wiped village after a reset or reinstall.
- (a) **Settle it like any emptied village** after the cap wait (192 days): the plan's §2.3 rule for a
  site found empty. Every dead village in an existing world, including villages players emptied
  on purpose for trading halls, comes back to life 192 days after the build first loads it, and
  zombie villages heal.
- (b) **Never settle a village the engine has not seen alive (Recommended).** Dead and zombie villages
  stay as they are; a village is renewed only after a census has counted a villager in it. The cost: a
  village wiped before a reset stays dead (a corner), and a player who cures one zombie villager
  starts that village's life.
- (c) Settle it after the base wait (16 days): as (a), faster.
Why it is Ben's: it changes what existing worlds look like; EQ-1 is about a village a player empties
while the engine watches.

**FB-2 (EQC154, F9), DECIDED by Ben 2026-10-29: (a), every bed in the box counts, capped at 32. Does a player's bed count?** K is the beds the census finds inside the village
box.
- (a) **Every bed in the box counts, capped at 32 (Recommended):** vanilla's own breeding uses any
  free bed within 48 blocks (V2). A bed hall a player built inside a village would fill, slowly, with
  villagers while the village is unloaded (the logistic takes about 400 days from 2 to 11 villagers at 12 beds, and about 490 days from 2 to 90% of 32 beds).
- (b) Only the beds the village had at adoption: the engine would need a bed count at first sight, which
  it does not get (adoption happens at structure generation, before the chunks exist).
- (c) K is the adoption guess (a third of the pieces, 4 to 30): no bed counting at all.
Why it is Ben's: players build bed rooms in villages for trading halls, and (a) makes them fill.

**FB-3 (EQC172, EQC182, F13), DECIDED by the coordinator 2026-10-29: (a), my recommendation; an internal interface, reversible before a release. The walkers list and the arrival API.** A walk-in must outlive the player
who watched it.
- (a) **A `walkers` list in `mic_rtsim_villages.dat` (additive, no version, an older file loads) and
  `mic_rtsim.api.Arrivals` as the shared helper (Recommended).**
- (b) A data attachment on each settler: travels with the entity, but vanilla strips attachments
  while the mod is absent and the entity must be loaded to be found (EQ-3 (b)'s problem).
- (c) No list: a settler whose watcher left is simply placed at the village when unseen, and is not
  counted while it walks, so a complete census during the walk reads the village short and can start a
  second wait: the pair walking in would be followed by another pair. It works with less code and one
  more edge case.
Why it is Ben's: a world-save format (small, additive) and an interface S5 and S9 would follow.

**FB-4 (EQC180, F15), DECIDED by Ben 2026-10-29: (b), a merchant when the village has none, otherwise a profession picked by the op's hash. What is a refilled Ribbit?** Today a grown house's resident is a nitwit.
- (a) A nitwit, as grown houses already do: no change; a killed-out village loses its merchant for good.
- (b) **A merchant when the village has none, else a profession by the op's hash (Recommended):** the
  village keeps someone to trade with.
- (c) A profession by the op's hash only.
Why it is Ben's: it is what players meet in a Ribbit village after they empty it.

### Ids S6 takes

Checked against `docs/boundary.md` (§2.3, §4, and the ranges of §6.1, §7.1, §8.1, §9.1, §10.1, §11.1,
§12.1, §13.1), `crates/rtsim-jni/src/wire.rs`, `Obs.java` and `Cmd.java`, `rtsim-world`'s `inputs.rs` and
`persist.rs`, `rtsim-travel`'s `persist.rs` and `rtsim-archive`'s `kinds.rs`, at 43062c0b, the S5 worktree
(clean, at that same commit, when I read it) and origin/main. Nothing S6 takes is in a dragon range; S5
should check its own against this list.

| Space | Taken by S6 | Used today | Free left nearby |
|---|---|---|---|
| Observation record kinds | none | phase 1: 0x0020 to 0x0022, 0x0120 to 0x0126; core 0x0001 to 0x0005, 0x0010, 0x0011, 0x0100 to 0x0111, 0x0130 to 0x0132 | 0x0023 to 0x002F, 0x0127 to 0x012F |
| Command record kinds | none | phase 1: 0x8010 to 0x8015; core 0x8001 to 0x8006 | 0x8016 to 0x801F |
| VILLAGE_CENSUS flags (offset 20) | bit 2 COMPLETE | bit 1 MEETING_POI | bits 4 and up |
| VILLAGE_ADOPTED aux | none | bit 1 LATE | |
| SPAWN_RESIDENT flags (offset 44) | bit 4 ARRIVAL, bit 8 WALK_IN | 1 AT_HOUSE, 2 AT_VILLAGE | bits 16 and up |
| `Input` log tags (rtsim-world) | 22 Census, 23 ResidentResult | 1 to 21 | 24 and up |
| `Ev` wheel tags (rtsim-world) | 6 Settlers, 7 Breed | 0 to 5 | 8 and up |
| `Body` persist tag (rtsim-travel) | 6 VillagerResident | 0 to 5 | 7 and up |
| RNG system | `world.village` | `world.op`, `ribbits.growth`, `ribbits.band`, `world.works` | |
| Archive kind names | `settle.emptied`, `settle.settlers`, `settle.grew` | the `settle.*` four | |
| Phase 1 stats indexes | 20 to 25 (25 counts logged censuses, EQC192) | 0 to 19 | 26 and up |
| Formats | RTW4 to RTW5; RTP1 5 to 6 | | |
| `SiteKind`, `ReportKind`, dictionary kinds, JNI exports, test orders | none | | |
| EQC ids | EQC150 to EQC183 | S5's own range is the lead's | EQC184 to EQC199 kept |

### S6 tests

Test names are the build's. Rust tests go in `crates/rtsim-world/tests/villages.rs` (new) unless a
crate is named; the long ones are `#[ignore]`d with short always-run versions, as EQC21 did, and run
`nice -n 10 cargo test --release`. A world-level harness (`rtsim_world::synth::bodies`, new) keeps a count of
real bodies per village, answers every `SpawnResident` with an applied result (at once, or only when the
script loads the village), and feeds a village a census only while the script holds it loaded: a
village left unloaded gets none and reads unloaded, so the sim's growth runs. It is what lets the engine
tests run with nobody loading anything.

| Records | Rust | GameTests |
|---|---|---|
| EQC150 to EQC152, EQC155 (the census both ways) | `a_complete_census_sets_and_lowers_members`, `an_incomplete_census_only_raises`, `a_wiped_village_keeps_its_market_at_pop_one`; in `rtsim-jni` `tests/villages.rs`: **`village_pop_follows_census_down`** (VILLAGE_CENSUS records in: one differing complete reading changes nothing, two lower the count, an incomplete one never lowers, heartbeats are throttled to one per 6,000 ticks, the world after a restart equals the one that kept running), `census_flapping_never_reaches_the_world` | core `aCompleteCensusLowersTheCountAndAnIncompleteOneDoesNot` (a village box that reaches past the loaded chunks reads incomplete) |
| EQC153, EQC154 (found empty, K) | `a_village_never_seen_alive_gets_no_settlers`, `a_seen_village_emptied_gets_settlers`, `k_follows_beds_and_is_capped_at_32`, `a_village_with_fewer_than_two_beds_has_no_settlers` | core `aVillageNeverSeenAliveIsNotSettled` (an adopted village with no villagers, 40 days advanced) |
| EQC156, EQC158 (kills and pressure) | `a_player_kill_adds_one_over_k`, `other_losses_add_nothing`, `kill_reports_do_not_move_members` | core `aPlayerKillAddsPressureAndTheWaitFollowsIt` (a fake player kills the arena's villagers, `P1_ADVANCE`: the settlers' order is not out at day 31 and is at day 32), `aMobKillAddsNoPressure` (a zombie credit: 16 days) |
| EQC159 to EQC162 (the wait, the pair, the bookings) | `settlers_come_after_the_wait` (32 days after a full player wipe at K 12, 16 after a siege, cancelled by a census of 2), `a_running_wait_is_not_pushed_out`, `a_pair_is_clamped_to_k`, `the_bearing_points_away_from_the_neighbours`, `a_failed_arrival_lapses_the_booking`, `a_census_never_undoes_a_booking_java_has_not_answered`, `owed_expires_after_two_years`, `a_replayed_engine_gives_the_same_ops`; `farmed_village_yields_closed_form` (60 years, a player killing every pair within 2 days: within 5% of §3.3's 3.1 pairs a year at K 12); in `rtsim-cli` `sim::equilibrium`: **`kill_all_villagers`** (every village a player wiped gets its pair 32 days after, and 90% of the control's villagers are back within 600 days: the median over 30 seeds on the `#[ignore]`d version, one seed always run) | |
| EQC163, EQC164 (growth, loaded) | `human_growth_follows_the_logistic` (30 seeds: the mean time from 2 to 11 villagers at K 12 within 10% of 400 days), `growth_stops_at_k`, `a_loaded_village_does_not_grow_in_the_sim`, `growth_is_not_slowed_by_pressure` | core `arrivalsNeverFillMoreBedsThanThereAre` |
| EQC165 to EQC167 (the Ribbits) | `ribbit_refill_fills_before_it_builds`, `a_full_ribbit_village_still_builds`, `pop_is_booked_on_spawn_not_on_order`, `a_failed_house_gives_its_booking_back`, `ribbit_settlers_below_two`, `kill_all_ribbits` in `rtsim-cli`: 90% of the control within 300 days | Ribbits `aKilledRibbitIsRefilledIntoTheVillageNotANewHouse`, `aRefilledVillageKeepsAMerchant`, `ribbitSettlersWalkIn` |
| EQC168 to EQC173 (wire, formats, archive, S1) | `village_rows_round_trip` (the persist proptest, rows and `resident_ops` in the hash), `an_older_world_format_is_named_not_read` (RTW4), `renewal_off_is_todays_world` (with `renewal` false every existing `rtsim-world` test passes unchanged), the codec round trip for tags 22 and 23 and `Ev` 6 and 7, `spawn_resident_flags_round_trip` (jni protocol), `kinds_are_unique` (archive), `census_counts_villages_by_k` and `census_is_flat_with_no_player` extended with villages (`rtsim-cli` `sim/census.rs`), `sim_quiet_stays_quiet_with_settlers` (`tests/quiet.rs`: a grid of villages, a tenth wiped at day 200, market events per market per day back to 0 by the end, `check_conserved` after every window) | |
| EQC174 to EQC179 (arrivals) | in `rtsim-host`: `villages_renew_end_to_end` (the native engine through the fake Java across a crash between a settlers' order and its result: one body, the same op) | core `settlersWalkInFromTheExploredEdge` (**the row's**: a player stands in the village, the pair spawns 96 to 176 blocks out and at least 128 from him, walks to within 12 blocks of the bell inside the timeout, is two untagged vanilla villagers, the engine's `owed` is 0), `noArrivalIntoAWatchedVillage` (**the row's**: a growth order and a settlers' order, a player within 128 blocks: no villager is placed inside the footprint; he leaves 200 blocks and the growth order places at the bell), `anUnwatchedVillageGetsItsSettlersAtTheMeetingPoint`, `aWalkerThatLosesItsWatcherIsPlacedAtTheVillage`, `settlersAreOrdinaryVillagers` (type by biome, no profession, saved by vanilla, takes a workstation within a day, trades) |

### S6 walkthrough: Ben kills all twelve villagers

A plains village, 12 beds, 12 villagers, a few other villages loaded.

1. Ben kills the twelve. Twelve kind-8 reports reach the engine; each adds 1/12: P is 1.00. `members` has
   not moved.
2. The next complete census reads 0, and the one after reads 0 again (a minute or less with a few
   loaded villages). `set_members` takes the count to 0, the market keeps pop 1 and re-reads its flows,
   `settle.emptied` is archived, and `Ev::Settlers` is booked `16 x 2^1 = 32` days ahead.
3. Thirty-two game days later (768,000 ticks, 10 hours 40 minutes of real time) the pair is booked:
   `members` 2, `owed` 2, two SPAWN_RESIDENT orders (ARRIVAL and WALK_IN, from a point 192 blocks out on
   the side facing away from the neighbouring villages).
4. If nobody is within 128 blocks of the footprint and the village is loaded, the pair is placed at the
   bell in that tick, each answered APPLIED, `owed` 0. If it is unloaded the orders wait in Java's
   registry and the pair stands at the bell at the next load. If Ben is within 128 blocks, the pair spawns
   at the first point on the ray toward the source (96 to 176 blocks out) that is inside the ticking
   area and at least 128 blocks from him, and walks in; he sees two villagers come up the road. If he
   stands where no such point exists, they wait until he moves.
5. From two, vanilla breeds only with food and a free bed (V1, V2): a farmer has to find a composter and
   crops, and the village's beds are free (V5), so Ben can help. While the village is unloaded the sim
   adds a villager every 20 to 60 days on average, as bookings Java places at the next unwatched load,
   never past the free beds; it reaches 11 of its 12 beds about 430 days after the kills. While it is
   loaded the sim adds nothing, and what the village does is vanilla's.
6. If Ben kills the new pair at once, P is 1/6 on top of what is left of the first, and the next pair
   comes after about 30 days; kept up, about 3 pairs a year (§3.3). Zombies add nothing.

### S6 what it leaves

- Villages are still never founded (C22, P22); a village with no beds stays as it is.
- A village a player takes villagers from by Carry On or by vehicle loses them without pressure (F4).
- Settlers and emptied villages are archived but are not news a villager can tell (TALK's lines, CL-9);
  a later talk slice can add them.
- A refilled Ribbit stands at the meeting point and is homed there (F6); moving it into a house's slot
  needs Java to know which generated house is empty.
- The viewer has no card for a village row; the timeline shows the three new kinds.
- A village the engine can never census completely (always at the edge of the loaded area) keeps its
  count and never settles; the guess stays.
- Iron golems and bought works are untouched; a village's golem cap still follows `pop` (R13).
- A reset forgets a village's pressure and `seen` (F7, F8): villages emptied before it are not settled
  unless FB-1 (a) is chosen.
- Babies count as members; the wait ignores whether the two are adults with food and a bed, which is
  vanilla's to settle once they exist.
- Wandering traders and caravans are phase 1's and unchanged.
- Switching phase 1 off freezes the settlers' waits and the growth steps with the world (U10);
  `ResidentResult` is heard on or off like `WorkResult` (RP-300) and the census input is ignored while off, as
  the census is today. An uninstall leaves the settlers as ordinary villagers; orders not yet placed are
  lost with the sim, and the villagers they stood for were only ever the sim's belief.

### S6 risks

1. **The walk-in is the least certain part.** A villager's walk target is a brain memory that other
   behaviours also set, and a Ribbit's goals are not brain activities. The fallback (EQC178: placed at
   the village when unseen, or at the deadline) means a failed walk costs only the look, but the
   GameTests `settlersWalkInFromTheExploredEdge` and `ribbitSettlersWalkIn` are what show it works; if the Ribbit
   one cannot be made to hold, Ribbit settlers use the fallback alone and that is recorded.
2. **Completeness is conservative, so some villages never settle.** A village at the edge of the loaded
   area is rarely complete; its count keeps the adoption guess. The reach of 48 is a guess too: a
   resident who wanders further than that is missed, and the sim then believes the village shorter
   than it is. Humans are clamped by free beds and Ribbits can overshoot K by the share that is
   outside the reach.
3. **A first complete census lowers many villages at once** in an existing world (the guesses were
   always high or low). The census is one village per 100 ticks, so the market events are spread, but
   the first hour of a deployed server is a burst; the `sim quiet` windows must show it ends.
4. **A falling `pop` lowers the treasury ceiling,** and coin above the ceiling sinks into the works purse
   (SINK.md): a wiped village may buy golems with its own treasury before the settlers come, and the
   conservation checks (`check_conserved`) must hold across a wipe. The test `sim_quiet_stays_quiet_with_settlers`
   runs it.
5. **`kill_all_ribbits` rides the prosperity clock.** Each +1 raises the targets, so the village may need
   days to be prosperous again; if the 300-day band fails the knob is `ribbit_refill_needs_prosperity`.
6. **The log grows by heartbeats:** about four inputs a day per loaded village, until the day's
   snapshot (`TAIL_MAX_TICKS` is one world day). A server with 50 loaded villages adds 200 inputs a
   day to a tail of 1,024; measured in step 4.
7. **The model is a model.** It has no census flapping, no beds, no walkers; its 480 days (438 to 534)
   is for the model's K 6 to 20. The engine's own bands come from the S6 checks and go in EQC184
   onward, with the numbers of S2's and S4's "what the checks found".

### S6 work split

Each step is one commit on `feat/eq-s6-villages` (commit early: an uncommitted agent worktree is
removed after an interruption), builds and tests under `nice -n 10`, and stops with the tests of its
row green. Before any Gradle or GameTest step, check the laptop is on AC and no GameTest run holds
the lock.

1. **The world's rows and census input** (`rtsim-world`). `VillageRow`, `World::villages`,
   `set_members`, `Cause::Population`, `Input::Census` and `Input::ResidentResult` (tags 22, 23),
   the `Input` codec and `apply`, `WorldConfig` fields (EQC183), `World::census` with K and members
   (EQC173), persist RTW5 (rows, `resident_ops`, stats), `Body::VillagerResident` (rtsim-travel).
   Tests: the census-both-ways row, `village_rows_round_trip`, `an_older_world_format_is_named_not_read`,
   `renewal_off_is_todays_world`. Done when the whole `rtsim-world` suite passes with `renewal` false
   unchanged and true on the new tests.
2. **Pressure, the wait and the pair** (`rtsim-world`). Pressure bump on kind 8, `Ev::Settlers`, the
   pair, `identity_op`, `settlers_from`, `resident_ops`, the results and lapses, `owed_max_days`,
   archive kinds. Tests: the wait, pair and bookings rows, `farmed_village_yields_closed_form`.
3. **Growth and the Ribbits** (`rtsim-world`). `Ev::Breed`, `loaded_until`, the refill and pop on spawn
   in `ribbits.rs`, Ribbit K, the failed-house path. Tests: the growth and Ribbit rows,
   `sim_quiet_stays_quiet_with_settlers`.
4. **The link** (`rtsim-jni`). VILLAGE_CENSUS's flag, `census()` with the memo and heartbeat,
   `Input::Census` booking, COMMAND_RESULT routing to `ResidentResult`, SpawnResident flags and the
   human body (`catalog::HUMAN_BODY`), the stats, META v6 and the dropped `censused`. Tests:
   `village_pop_follows_census_down`, `spawn_resident_flags_round_trip`, `census_flapping_never_reaches_the_world`.
5. **The fake Java and the host** (`rtsim-host`): a `Java::census` helper, village bodies, an applied
   answer for arrival orders. Test: `villages_renew_end_to_end` with the crash.
6. **The checks** (`rtsim-cli`): the `villages` scenario of `sim census`, `sim::equilibrium`'s
   `kill_all_villagers` and `kill_all_ribbits`, the census tests extended. Run the 30-seed versions,
   record the bands in "What the S6 checks found" (EQC184 onward), and set the knobs if a band fails
   (EQC167).
7. **Java core.** `Census` (complete flag, reach, walkers), `Arrivals` (watched, spot, walkers),
   `ResidentSpawner` (ARRIVAL, WALK_IN, the bed clamp, the NOT_FREE lapse), the villager factory,
   `VillageRegistry.walkers`, `Obs` and `Cmd` constants, `NativeEngine.P1_*`. GameTests: the core
   rows of the table.
8. **Java Ribbits.** Profession choice, `PathNavigation` walk-in, refill at the meeting point.
   GameTests: the Ribbits rows.
9. **Docs and the fixtures.** `docs/boundary.md` (§4.1, §4.2, §4.3, §5 item 10), EQUILIBRIUM.md §4's
   S6 row marked built and §5.2's village rows, this file's findings, rtsim-view's fixtures, the
   archive dictionary in the viewer.
10. **Land.** Merge `origin/main`, check `git diff --name-only --diff-filter=U`, run the tests the
    branch changes, then the pull request and review as CLAUDE.md says.


## What the S6 build found (not edited)

The 30-seed checks of EQC174 to EQC178 on the synthetic world's six human and three Ribbit villages
(`rtsim-cli`, `sim::equilibrium`, release build, seeds 1 to 30, every village of the culture killed by a
player on day 100, the same seed's control with no wipe), median [5th, 95th] over seeds:

| Check | Engine | Target |
|---|---|---|
| Kill all villagers: day of the first and the last settlers' order, after the wipe | 32 and 32, in every village of every seed | 32 (P 1.00, 16 x 2) |
| Kill all villagers: days to 90% of the control | 525 [431, 668]; four seeds above 600 (6, 14, 27, 28) | the model 480 [438, 534]; the test: the median within 600 |
| Kill all Ribbits: day of the first and the last settlers' order | 32 and 32, in every village of every seed | 32 |
| Kill all Ribbits: days to 90% of the control | 122 [112, 122], the longest 132 (seed 20) | within 300 |
| Human growth from 2 to 11 villagers at K 12, mean of 30 seeds | within 10% of 400 (the discrete process's closed form is 385.7) | 400 |

| # | Finding | What it decided |
|---|---|---|
| EQC184 | **The Ribbit refill must not wait for prosperity.** With the refill riding the prosperity clock (EQC167's default), `kill_all_ribbits` never reached 90% of the control in 700 days: the wiped world ended at 0.44 to 0.79 of the control with 9 to 19 refills, three seeds. Taken off the refill it reached 90% on day 122 in every one of three seeds, 28 refills, 1.000 at the end. After a wipe the markets rarely sit at target for the three days the clock needs, and every +1 moves the target again. | `ribbit_refill_needs_prosperity` is **false** by default. A house still needs prosperity, as before S6. |
| EQC185 | **Kill-all for villagers is slower than the model's, inside the test.** Median 525 days against the model's 480, 95th percentile 668. The engine's K runs 10 to 20 beds (the model's 6 to 20), the settlers' wait is the whole 32 days, and the logistic is discrete with a last step of about 105 days from K - 1 to K. | The test stays at the median within 600 days. No knob moved. |
| EQC186 | **The grid's late market rate is chaotic, with or without S6.** The same four villages' populations set to 1 for a month and set back, with no renewal at all, took seed 42 from 1.72 to 0.24 market events per market per day over days 1,500 to 1,800 and seed 7 from 0.15 to 3.44. `sim_quiet_stays_quiet_with_settlers` therefore does not assert silent markets (it did in its first form, and failed the day the refill's default changed). | It asserts what S6 owns: in the last 300 days no settler, growth step, refill or house is booked, no village has a booking or a wait running, every row equals what stands there (the harness's count), no market event has the cause `Population` or `Growth`, and conservation held after every 100 days. The chaotic rate is the economy's, not S6's; a later slice that wants a silent grid needs its own look at the Wake and Delivery events. |
| EQC187 | **A village nobody lives in builds nothing.** A Ribbit village at 0 members with a prosperous market ordered a house for nobody: the refill needs `members >= 1` and the house path did not look. Found by the refill test. | `World::deserted` (seen alive, 0 members) stops the growth clock; settlers come first, then the refill, then houses. |
| EQC188 | **Pop on spawn keeps a growing village prosperous.** Reedholm, the one Ribbit village of the synthetic world that grows a house at all in 200 days (Mirewater and Bogsend are never prosperous three days running), reads prosperous 194 days of 198 with pop booked on spawn and 9 days of 198 with P17's pop step at the order: the step lifted the targets over the stocks. | A side effect of EQC165, wanted. The Ribbit tests use Reedholm. |
| EQC189 | **A growth step inside a census's loaded window is dropped and drawn again.** The half day after a logged census (`village_loaded_ticks`, 12,000) holds the village loaded; a step due then is dropped and a new wait drawn (the exponential is memoryless, so the loss is at most that half day per village). The 30-seed logistic test meets it on some first steps. | The tests read a step with no order as a dropped one (`loaded_until > due`). |
| EQC190 | **The settlers' bearing reads the nearest image of each neighbour** (`Topology::toward`) and the source point is canonicalised, so a village near a seam of a toroidal frame points away from the villages on its other side. EQC161 said the centroid of the neighbours; on a flat frame it is the same. | Nothing to decide; recorded because the design's text names no seam. |
| EQC191 | **Test files.** Rust: `rtsim-world/tests/villages.rs` (step 1, the census and formats), `settlers.rs` (step 2, which sets `village_growth_per_day` to 0 so a growth order never sits between a pair and its census), `village_growth.rs` (step 3), `quiet.rs`; `rtsim-jni/tests/villages.rs` (steps 4 and 5, including `villages_renew_end_to_end` and the crash between an order and its result, which live there and not in `rtsim-host` because the fake Java they drive is the host's, in `rtsim-host/src/java.rs`); `rtsim-cli/src/sim/census.rs` (the `villages` scenario, `Villages`) and `equilibrium.rs`. Java: `gametest/VillageRenewalGameTests.java` (core) and `gametest/RibbitRenewalGameTests.java` (Ribbits). | The names in "S6 tests" are the tests' names. |
| EQC192 | **A TEST-mode view of a village's row** (`Engine::phase1_village`, `nPhase1Village`, `NativeEngine.phase1Village`) and a census counter (stat 25) were added for the tests: the engine's belief about one village was otherwise readable only through its markets. | Stats are 0 to 25; Java's `PHASE1_STATS` is 26. |
| EQC193 | **The renewal GameTests wait on far chunks, and the server ticks unthrottled.** Three rounds on code that differed only in the tests failed three different waits: the first core round at the walk-in's "walking: 0", a second at "the reach is loaded" of `aVillageNeverSeenAliveIsNotSettled`, the first Ribbits round at the Ribbit's walk, and on the merged head the core walk-in again, at "both walked to within 12 blocks". Two causes. (1) A wait's timeout in ticks passes in less wall time than the chunks need on the IO threads, so a failing `check` in both classes now sleeps 5 ms (as `RtsimTests.io` does for the waits that use it). (2) A walker stops in a chunk that does not tick; the walk tests forced the census reach and the ring round each spawn point but not what lies between, and which bearing the engine draws decides whether a gap sits on it. Both walk tests now force the whole corridor, 16 blocks apart from the bell to the edge, and carry a trace of the walk (every 500 ticks, where each settler is and what holds it) in their failure message. | Test-only. In the game a settler that cannot go on is placed at the village when the 6,000-tick deadline passes (`Walkers`), so a gap costs a walk and never a body. The runs: `:core:test` and core 178 of 178, Ribbits 15 of 15, `create -Psec1fx` 266 of 266 on the merged head (the rounds that failed first are the ones named above). |

## S7: conflict pressure

**Designed 2026-10-30, not built.** EQUILIBRIUM.md §4's S7 row: pressure on the recruits of a camp whose members have no
bodies at home, and its break and refound. When a war party goes out is `CONFLICT-LIFE.md`'s build plan (EQ-5 rejected the
gate), and S7 stays out of it (EQC226). Branch `feat/eq-s7-conflict`, cut from main 9389b4e5 (S5's dragon home bodies and
S6's villages have both landed). Ids EQC200 to EQC299 are S7's (extended from EQC249 on 2026-10-30, nothing else uses EQC250 to EQC299): EQC200 to EQC236 are the
engine side, EQC237 to EQC269 the outpost reporter Ben added (FB-5 (b)), and EQC270 to EQC299 are kept for the build's own findings. Choices that are Ben's are marked **FOR BEN**, each with its options and a recommendation,
and are gathered after the records. Nothing is built and no engine code changes here; the only other file is the model the
numbers come from, `research/equilibrium-s7-camps/camps.py`.

**Words used.** A *pool camp* is a conflict culture whose members have no body at home: its warriors are a number, and its
parties are the only bodies it ever has. Today that is goblin, bandit and raider (`camp_bodies` absent, `renewal` absent,
`recruits_per_day` 2, 1.5 and 2, caps 60, 40 and 50). A *body camp* is a culture with `camp_bodies` (the Frostmaw den and the
Umvuthana grove, MOWZIE.md §4.9); they are S9's, and S7 changes nothing at a den or a grove. The *anchor* is the faction's
`pool` and `pool_at`: the pool is a line from the anchor, and every event that moves the pool moves the anchor. A camp is
*thin* when its members (pool and members away) are under a tenth of its cap, *idle* when it has no party in any phase and no
feud with an open demand, *broken* between its break and its refound. A *new band* is a refounded camp's fresh chief and heir.

### The shape

1. A pool camp gets a **strain**: one boxed record on its faction row holding its pressure, its break, and the stamp of its one
   wheel event (EQC201). A culture opts in with a new `pool_renewal` block in its JSON (EQC202); the body cultures have none, so
   their code paths and numbers do not move.
2. The recruit rate is `max(r x 2^-P, 1/96)` a day with P read at the anchor, in the three places that read
   `recruits_per_day` today. The pool stays a line between events, so `pool_ready` is the exact inverse of `pool_at` (EQC203).
3. A player's loss is a credited kill of a party member (`BAND_CASUALTIES`, which the engine does not book for anyone today), a
   war chief killed by a player, and a party lost at a player's district; at a district every death that no wild mob dealt is
   the player's, as the abstract fight books it (EQC204, EQC205). Raids on villages and camps, and mobs, add nothing.
4. A thin, idle camp at P 2 breaks; one with a party out or a demand open waits, looked at again each game day (EQC207, EQC208).
5. A broken camp is inert both ways: nothing leaves it and nothing is sent at it, its grievance keeps its closed form, its
   node and market are untouched (EQC209, EQC210).
6. `min(384, 48 x 2^P)` days after the break a **new band** takes it with a quarter of the cap, new chief and heir, no old
   fights; the pressure it carries is what is left of the old one (EQC211, EQC212).
7. **The finding that moves a number.** At the engine's recruit rates a player who kills every whole warrior every two days
   holds a goblin camp at P 1.58, a bandit camp at 1.67 and a raider camp at 1.72, so the break at P 2 never fires and the
   row's `farmed_camp_breaks_and_refounds` could not pass. Taken: a half-life of 192 days for pool camps (the farmed P is then
   2.13 to 2.29). **FOR BEN** FB-1 (EQC203).
8. The ledger row, the found tail, the census and the archive carry it with no new record kind: a broken pool camp's row is
   `EMPTY | BROKEN | WAITING` with the refound wait left (EQC214, EQC216, EQC218). Formats: the conflict pages RTCD to
   RTCE, meta 12 to 13 (EQC217).
9. **Nothing in the shipped pack reported a pool camp before S7** (`RtsimApi.reportCamp` had no caller outside the tests; the one
   `declareCamp` caller is the Mowzie plugin, for dens and groves). Ben decided FB-5 (b) on 2026-10-30: **S7 includes a reporter**,
   one core class that declares vanilla pillager outposts as raider camps when their chunks load (EQC237 to EQC269). Raiders and
   their raid ladder therefore run in real worlds, with S7's pressure and break on top; new choices that are Ben's are FB-6 to FB-9.
10. Tests: `kill_all_camps` (90% of the control within 96 days; the model's rule reads 45 to 54), `farmed_camp_breaks_and_refounds`, and failing-first tests for every behaviour that changes (EQC230 to EQC232).

### S7 base and reads

Read for this design, all at 9389b4e5: EQUILIBRIUM.md (§1, §2.2 and §2.3, §2.4.5 and §2.4.7, §2.6, §2.7, §2.8, §3.3 with its note on K
above 1, §3.6 to §3.8, §4's S7, S9 and S11 rows, §5.1's EQ-5 and EQ-6, §5.2's camp rows); `CONFLICT-LIFE.md` (0, 1, 2.1 to 2.3, 2.9 to
2.11 with the build order of 2.11, and the decisions CL-D19 to CL-D34 of §7, for CL-D23, CL-D26 and CL-D31 to CL-D33); `MOWZIE.md` §4.10
(the recruits row, the loot supply, the five things it asks of the shared rule), with §4.9's rules as EQUILIBRIUM.md §2.6 and the code
state them; `docs/choices-mowzie-build.md` M12a (the summary above MZB-1000 and MZB-1016's note on the grove's recruits), with the
M6, M9 and M12a records the engine's comments cite (MZB-251 to MZB-259, MZB-826, MZB-1006 to MZB-1008, MZB-1014) read in the code; this
file's S1, S2, S4, S5 and S6 sections; the conflict engine (`crates/rtsim-conflict`: `engine.rs` Faction 112, Ev 506, `census` 1289,
`camp_site_state` 1327, `add_faction` 1647, `pool_at` 1874, `pool_add` 1886, `pool_ready` 1900, `signal` 2087, `plan_feud` 2458,
`send_party` 3379, `lose_units` 4973, `home` 5241, `plunder` 5377, `handle` 6305, `casualties` 7365, `chief_killed_by` 7556, `check`
7842; `engine/camp.rs` `camp_found` 681, `camp_restore` 750, `whole_at` 905, `pool_return` 932, `renewal_bump` 1046, `plan_arrival`
1071, `party_lost` 1445; `culture.rs`, `persist.rs`, `tunables.rs`, `data/cultures/*.json`, `synth.rs`); `crates/rtsim-kernel`
`renewal.rs`, `sites.rs`, `census.rs`; `crates/rtsim-jni/src/orders.rs`; the core Java that declares camps and reports deaths
(`ConflictBridge`, `Camps`, `RtsimApi`, `NativeCodec`, `SiteTestHooks`) and the Mowzie plugin's `MowzieCamps`; `docs/boundary.md`
(the SITE_STATE and found-tail text); `crates/rtsim-archive/src/kinds.rs`; `research/equilibrium-2026-10-21/model.py` (`camps`,
`kill_camp`) and its summary. Not read: MOWZIE.md §4.5 and §4.9 in full, CONFLICT-LIFE.md §2.4 to §2.8, §2.12 to §2.17 and §4 to §6, and
the M9 and M11 records, beyond what is cited. S5 and S6 are on main, so no id below is drafted against an unlanded branch; no branch
other than this one touches `crates/rtsim-conflict`.

**What is true in the repo (file and line at 9389b4e5):**

| # | Fact | Where |
|---|---|---|
| R1 | The pool is a line: `pool + recruits_per_day x (t - pool_at)`, clamped to the cap less the members away. Three places read `recruits_per_day`: `pool_at`, `pool_ready` and the camp ledger's `whole_at`. Every writer of the anchor goes through `pool_add`, `pool_return` or `pool_shift`. | `engine.rs:1874-1910`, `camp.rs:905, 932-952` |
| R2 | Pressure and a renewal exist only for a body culture: a `renewal` needs `camp_bodies` and `recruits_per_day` 0; the pressure lives on the `Camp` ledger, which a pool camp does not have; `renewal_bump` returns at once for a culture with no `renewal`. | `culture.rs:585-607`, `camp.rs:232, 1046-1049` |
| R3 | The five cultures: goblin 60 at 2 a day, bandit 40 at 1.5, raider 50 at 2 (no `renewal`, no bodies); Frostmaw cap 1 with a boss `renewal` (6, 192, 96); Umvuthana cap 9 at 2 a day with `camp_bodies` and only a `chief_renewal`. The grove's warriors therefore still regrow by recruits (the M12a note on `uninstallLeavesHomeBodies`, MZB-1016), which is S9's. | `data/cultures/*.json` |
| R4 | A loss is booked (`renewal_bump`, `chief_lost`) in `lose_units` (cause `Player` for a party lost at a player's district, `Faction` for a raid on a village or a rival camp), `chief_killed_by`, `camp_death`, a stray's death, a write-off and the chief's deaths. **`casualties()`, which handles BAND_CASUALTIES, books none**: it takes the units off the party and calls `party_lost`. The record carries `by` and the killer, and Java fills them from the full kill credit (`ConflictBridge.onDeath` takes `Camps.killer` for a member, MZB-826); the doc comments at `ConflictBridge.java:2519` and `Camps.java:2144` ("BAND_CASUALTIES keep byKind, nothing reads them") are stale. | `engine.rs:4744, 5027, 5102, 7365-7470, 7582`, `camp.rs:2147, 2934, 2972, 2990, 3173` |
| R5 | A party ends in one place, `home()`, after a wiped or routed band has walked home; a band that lost every unit is still a party until then. The district resolve loses `missing x abstract_loss_share` (0.5) of the units a fortress did not leave standing, and the survivors straggle home. | `engine.rs:5241-5352, 4744` |
| R6 | The Peaceful world is the engine's one precedent for an inert camp: `plan_feud` books no step, `plunder` reschedules itself and sends nothing, `send_party` and the muster refuse, and `set_difficulty` plans every feud again when it ends. | `engine.rs:2418, 2487, 3607, 4141, 5378, 6571-6600` |
| R7 | `send_party` is the one place a party is made; `plunder` picks its target among nodes the culture hates, rival camps included, by the node's pop and `haven_defence_per_pop`, which is fixed at join (`pool_cap / 4`) and does not follow the pool. | `engine.rs:3379, 1669, 5377-5420, 5084` |
| R8 | Every faction already sends a SITE_STATE row (kind Camp, K the cap, the floor of the pool as count, SHORT and EMPTY), but `p` comes from the `Camp` ledger only, so a pool camp's row says P 0 and never WAITING. `camp_found` for a camp with no bodies restores only the row's count ("the gap is not backdated", fork F14). The found tail's flags are ROW 1, GAP 2, WAITING 4, EXISTED 8; `state_flags::BROKEN` (4) exists but does not reach the tail. | `engine.rs:1327-1380`, `camp.rs:681-700`, `sites.rs:56-96` |
| R9 | `Conflict::census` reports `conflict.camp` with members the floor of the pool, K the cap, **P 0, broken false and no renewals**, with a comment that names this slice. | `engine.rs:1286-1296` |
| R10 | The conflict pages are RTCD (`RTC13`), meta 12; the wheel's `Ev` has tags 0 to 11 (`CampRecount` is 11); the faction row ends with its optional camp. The archive's conflict kinds end at `conflict.party_home`; no kind or name has "broken" or "refound" in it. | `persist.rs:89-95, 342-380, 1040-1140`, `kinds.rs:71-85` |
| R11 | **Before S7** no shipped plugin reported a pool camp (Ben's FB-5 (b) adds the reporter, EQC237): `RtsimApi.reportCamp` (whose Javadoc names `minecraft:pillager_outpost` and `goblinstyranny:goblin_camp`) is called by tests and by nothing in production, and the one production caller of `declareCamp` is the Mowzie plugin, for dens and groves. The `mic_rtsim:conflict/*` structure ids are test stand-ins. | `RtsimApi.java:185-226`, `MowzieCamps.java:177, 211`, `ConflictGameTests` |
| R12 | A faction's chief and heir are made at `add_faction` (seeds from `StreamKey(seed, SYS_CHIEF, key)` and the `chiefs` counter) and replaced by `succeed`; `observed` (real fights seen, newest last) is the floor `village_outcome` uses against the faction. | `engine.rs:1647-1750, 2010-2060` |
| R13 | An envoy is not drawn from the pool (`kind.is_war()` is false), so `envoy_insult_kills` (2) is a grievance count, not a loss. | `engine.rs:3396-3455` |

**What the numbers say** (`research/equilibrium-s7-camps/camps.py`, a day-stepped model with the engine's own recruit rates):

| Check | goblin 60 at 2 | bandit 40 at 1.5 | raider 50 at 2 |
|---|---|---|---|
| Recruits a day as a share of the cap (the model of §3.1 used 8%) | 3.3% | 3.75% | 4.0% |
| Farmed to the cap by a player who kills every whole warrior every 2 days: P reaches, half-life 96 | 1.58 | 1.67 | 1.72 |
| The same, half-life 192 | 2.13 | 2.24 | 2.29 |
| Breaks a year and broken share, farmed, half-life 96, break at 2 | 0 | 0 | 0 |
| The same, half-life 192, break at 2: breaks a year, broken share, day of the first break | 0.23, 0.47, 218 | 0.28, 0.57, 162 | 0.30, 0.60, 144 |
| The same, half-life 96, break at 1.25 | 0.57, 0.68, 38 | 0.60, 0.72, 30 | 0.63, 0.74, 28 |
| Share of its recruits a player must kill, steadily, to reach the break: half-life 192, P 2 / half-life 96, P 1.25 | 87% / 64% | 77% / 57% | 72% / 54% |
| One kill-all (P 1.0): days to 50% and 90% of the cap, P read at the last event | 30 and 54 | 27 and 48 | 25 and 45 |
| The same, P read every step | 29 and 51 | 26 and 45 | 24 and 43 |

The farmed P is the continuum `P = (r H / (cap ln 2)) 2^-P`; it does not depend on how often the player kills, only on the
share of the recruits he takes. Reading P at the last event instead of every step moves a kill-all's 90% by 3 to 6 days and
changes no break (rows C and D of the script).

### What S7 changes, by place

| Place | Change |
|---|---|
| Culture data (`rtsim-conflict`) | A `pool_renewal` block (`PoolRenewalDef`, resolved to `PoolRenewal`) on goblin, bandit and raider; refused with `renewal` or `camp_bodies`. The Frostmaw and Umvuthana files are untouched. |
| Engine (`rtsim-conflict`) | `Faction.strain: Option<Box<Strain>>`; `recruit_rate` in `pool_at`, `pool_ready` and `whole_at`; `pool_bump` (re-anchor, then bump) behind `renewal_bump`; a bump in `casualties` for pool cultures; `strain_check`, `break_camp`, `refound`, `reband`; `Ev::Strain` (wheel tag 12); the `camp_active` gates; `slay`; census, ledger row and found restore; stats. |
| Boundary | **No record kind, no flag, no aux bit.** SITE_STATE and the found tail carry a broken pool camp as `EMPTY | BROKEN | WAITING` and count 0, with the refound wait as the wait left (EQC214). |
| Formats | The conflict pages RTCD to RTCE and the meta 12 to 13; the site ledger file is unchanged (version 1). rtsim-view's fixtures are regenerated. |
| Archive | Two kinds appended: `conflict.broken`, `conflict.refounded`. |
| Java | Production: none. Test only: test order 41 `CONFLICT_SLAY` and two GameTests in core; the stale comments at R4 are corrected. |
| `rtsim-cli` | The `camps` scenario of `sim census`; `sim::equilibrium` gains the camps harness, `kill_all_camps`, `farmed_camp_breaks_and_refounds`, the long-horizon run. |
| Documents | EQUILIBRIUM.md §2.4.5, §3.3, §4's S7 row, §5.2's camp row; `docs/boundary.md` (the pool camp's row); this section's findings. |
| Kernel | Nothing: `Pressure`, `RenewalRule`, `LossCause` and `FoundRow` are used as they are. |

### S7 records: pressure and the recruit rate

| # | Choice | Why |
|---|---|---|
| EQC200 | (2026-10-30, slice S7 design) **Scope.** S7 gives a pool camp pressure on its recruits, a break and a refound (EQUILIBRIUM.md §2.4.5). It does not decide when a party goes out (CONFLICT-LIFE.md), does not touch a den or a grove (S9), a village, a dragon or the economy, adds no body, no block, no item and no record kind (D3), and changes no number a body culture reads: the Mowzie suites must pass unchanged, and a test pins the Frostmaw and the grove (EQC230). | The row's words, EQ-5's answer, and the rule that S9 owns the grove's warriors (§2.6 item 2 turns the grove into arrivals, not into a slower rate). |
| EQC201 | **Where pressure lives: `Faction.strain: Option<Box<Strain>>`, `Some` only for a culture with `pool_renewal`.** `Strain { pressure: Pressure, broken: Option<Broken>, stamp: u32, check_at: Option<Tick> }` with `Broken { since: Tick, due: Tick }`: about 56 bytes for each pool camp and a null pointer for every other faction. Fork F1: on the `Camp` ledger (rejected: it exists only for bodies, and a ledger for a pool camp would be a slot table with no slots); a bare `Pressure` field on every faction (rejected: 8 bytes for the Frostmaw and the grove that nothing reads, and a second pressure beside the `Camp`'s). | The plan puts a site's state in its engine's site row (§2.2). One box keeps a body culture's row as it is. |
| EQC202 | **Culture data: a `pool_renewal` block** beside `renewal` (`PoolRenewalDef`, resolved with `RenewalDef::rule`'s checks): `half_life_days` 192 (FB-1), `recruit_cap_days` 96, `break_share` 0.1, `break_pressure` 2.0, `refound_base_days` 48, `refound_cap_days` 384, `refound_share` 0.25; goblin, bandit and raider carry the same block. `Culture::resolve` refuses it with a `renewal` (a culture's members come back by one rule), with `camp_bodies` (a body camp's pool is a slot table that follows a line the same way, but S9 decides the grove, and a rate that moves under a ledger of owed bodies is not S7's risk), with `recruits_per_day` 0 (there must be a rate to slow), and with any number not finite, `break_share` outside (0, 1), `refound_share` outside (0, 1], `refound_cap_days` below `refound_base_days`. Fork F13: reuse `renewal` with a flag (rejected: its meaning, an arrival after a wait, is the opposite of a rate). | One block, one validator, the Frostmaw and Umvuthana files untouched. A culture JSON is data, so S9 can opt the grove in or out with no engine change. |
| EQC203 | **The recruit rate: `max(r x 2^-P, 1 / recruit_cap_days)` a day, which is the kernel's own rule read as a rate (`1 / RenewalRule { base_days: 1/r, cap_days: 96, half_life_days: H }.delay_days(P)`), with P read at the anchor and a half-life of 192 days for pool camps. FOR BEN (FB-1).** One helper, `Conflict::recruit_rate(f)` (per tick; 0 while the camp is broken; `r` for a culture with no strain), replaces the three reads of `recruits_per_day`. P at the anchor is the strain's pressure at `pool_at`'s game day. **Every pressure bump re-anchors the pool first**: `pool_bump` calls `pool_add(f, 0, at)` before it changes the pressure, so the gap before a loss is integrated at the pressure it had (today `lose_units` bumps before `party_lost` re-anchors, so the order is fixed inside one function and tested, EQC230). Forks: **F2** how P is read: at the last event (taken); every step (no closed form, and `pool_ready` would need the root of an exponential integral); or a booked re-read every few days (a wheel entry for each pressured camp, and an answer from `pool_ready` that moves). The script shows the first costs 3 to 6 days on a kill-all's 90% and nothing on the break, and the plan itself reads P "at the last event" (§3.8). **F3** a floor of one recruit in 96 days (taken; the plan has none): P would have to pass 7 for it to bind, and it keeps a corrupt ledger row (a huge `p`) from making an absorbing zero (§2.1, no absorbing zero). **F12** the half-life, below. | §2.4.5: "recruits at `recruits_per_day / 2^P`", with "P read at the last event that touched the pool", so the pool stays a line and `pool_ready` (the inverse) is never early. **The finding.** The model of §3.1 recruits 8% of a camp a day; the engine's camps recruit 3.3% to 4%. Recruiting less, a camp is slower to refill, so the same killing leaves a player's pressure lower than the model's: a player who kills every whole warrior every 2 days holds P at 1.58, 1.67 and 1.72 on the plan's half-life of 96 (the table above), because he can only take what the camp has recruited. §2.4.5's break at P 2 therefore never fires, and the row's `farmed_camp_breaks_and_refounds` could not pass. **Taken: half-life 192 for pool camps.** The same farmer reaches 2.13, 2.24 and 2.29; the first break comes on day 144 to 218; 0.23 to 0.30 breaks a year when he farms to the limit; the wait at P 2 to 2.3 is 192 to 236 days, the plan's "two to four game years" at its low end. The break stays at 2, the 10% and the 48 and 384 days of §2.4.5. A pool camp's memory is then two game years, level with the years its refound takes. The options are under FB-1. |
| EQC204 | **What is a player's loss at a pool camp** (the credit codes are MZB-1006's: a credited kill, a pet's, a death after the player's hit within 100 ticks, and every A10 removal are `Player`; a FakePlayer's, a trap's and an unknown source's are `Other`; a mob's is `Predator`; for a body that carries no boss loot an `Other` adds nothing, §2.2). By source: (1) **BAND_CASUALTIES** `by` PLAYER: `Player`; MOB: `Predator`; NONE and OTHER: `Other`, **except when the party's target is a player's district** (FB-2): there NONE and OTHER are `Player`, because the district's defences are the player's (turrets, traps, walls, lava) and the abstract fight books exactly that (R5); (2) the **abstract resolve at a district** (`lose_units`, `LossCause::Player`, already): the units that fall; (3) a **war chief** killed by a player (`chief_killed_by`, already calls `renewal_bump`): `1/cap`; (4) a **raid on a village or a rival camp** (`LossCause::Faction`): nothing, so the world does not learn to empty itself (§2.2); (5) an **envoy or scouts** killed: nothing (R13: they are no pool draw); (6) a death by age: none exists for a pool member. Each loss adds `n / cap`, cap the faction's `cap` (the culture's `pool_cap` for a pool camp). Fork F4: credited kills only everywhere (rejected for districts: a watched fight at a trap-defended base would book less pressure than the same raid resolved unwatched, a difference the engine's two ways of resolving one raid must not have). | §2.2's four cases and §5.2's "a player's loss is vanilla's kill credit". The district rule is §2.2's own third case ("any death no wild mob is credited with") applied where the player is the only one who defends, and it keeps the watched and unwatched fights equal. |
| EQC205 | **`casualties()` books the loss for a pool culture.** After `party_lost(p, t, at)` and only when `t > 0` and the culture has `pool_renewal`, call `renewal_bump(f, t, cause, at)` with the cause of EQC204; the bump is once per counted op, because the function has already returned for a `casualty_ops` repeat and for a copy it resolved unwatched, so a reassert, a replay and a copy book it once (`a_replayed_casualty_bumps_once`). It does not run for a body culture: the Frostmaw's chief dies through CHIEF_KILLED, and the grove's warriors killed on a march book no pressure today (finding for S9: when the grove gets a member `renewal`, `casualties()` is where an away warrior's death must bump, and CAMP_DEATH's "a member that died away was lost then" must not bump twice). `renewal_bump` itself grows a second arm: the body culture's as it is, then `pool_bump` for a culture with a strain. | R4: the loss that a player inflicts on a pool camp is mostly this one, and nothing books it. Gating on `pool_renewal` and not on "any renewal" keeps S9's choice free and Mowzie's behaviour bit for bit. |
| EQC206 | **A test hook: `Conflict::slay(w, f, n, at) -> u32`** (pub, as `Dragons::slay` and `Herds::cull` are): the player kills up to `n` of the camp's warriors, the pool first (`pool_bump` with cause `Player`), then the units of its parties through `lose_units(.., Player)`; returns how many fell. `n = u32::MAX` kills all. It is the scripted players' and the tests' only way to kill a pool camp's members "at home", where they have no bodies, and the JNI test order 41 (EQC221) calls it. | The kill-all and farm tests need a wipe that books like any death; the real game reaches the same bump through parties (EQC204). |

### S7 records: break, refound and the inert camp

| # | Choice | Why |
|---|---|---|
| EQC207 | **The break rule.** A camp with a strain breaks when all three hold: it is **thin** (`floor(pool_at) + away < break_share x cap`, strict: under 6 of a goblin camp's 60, 4 of 40, 5 of 50), its pressure now is at least `break_pressure`, and it is **idle** (no `Party` of the faction in any phase, and no feud of it with an open `demand`). Fork F6: break at once and recall or end its parties and withdraw its demands (rejected: three recall paths, a withdrawn envoy in front of a player, and a march record to end, all to save a few days); break counting only the pool (rejected: a party of eight out with an empty pool is a camp that has not lost its people yet). Only a player's pressure can make P reach 2 (§5.2: only players break camps), but the thinning that completes the break may be a feud's or a village's, as in the model (`camps()`: `pool < 0.1 cap` and `P >= 2`). | §2.4.5 and the model. Waiting for idle costs days (a wiped band walks home at about 480 blocks a day) and removes every path that would act on a camp that is leaving. |
| EQC208 | **When it is looked at: `strain_check(f, at)` and one wheel event, `Ev::Strain { faction, stamp }` (wheel tag 12).** `strain_check` runs after a pool camp's pressure bump, at the end of `home()` (a party ended), and from the event. Not thin or P under the break: it clears any booked check (bumps the stamp). Thin, pressured and idle: `break_camp` now. Thin, pressured and not idle: it books the event `strain_recheck_ticks` (one game day) ahead if none is booked (`check_at`), and the event looks again; a Siege band that lingers for its patience costs at most a few dozen events, one at a time. A broken camp's event is its refound, with the same stamp rule: a stale stamp is dropped and counted in `stats.stale`. Fork F7: a daily poll of every pool camp (rejected: D1; a camp that is not thin and pressured is never looked at). | Every way a camp becomes breakable is an event the engine already runs; the one wait is for parties and demands to end, which have no hook of their own that is worth adding. |
| EQC209 | **What a break does** (`break_camp`). `strain.broken = Some { since: at, due: at + min(refound_cap, refound_base x 2^P) days }` with P read now; the pool becomes 0 (`pool_add` to 0) and the recruit rate 0 until the refound; the faction's `chief_out` and `cowed_until` are cleared; every booked `Ev::Feud` of the faction is voided (each feud's stamp bumped, O(its feuds) once) and `plunder_stamp` is bumped; `Ev::Strain` is booked at `due`; the row is marked (EQC214); `stats.breaks`, the tape event `conflict.broken` (EQC218) and a `note`. The camp's **node, market, stock and haven pop are untouched**, and no report, notice or news is made (FB-4). Fork F8: pop to the floor while broken (rejected for S7: a Haven's pop is the economy's, a change of it is two market events and a conservation case, and a broken camp that other camps do not raid (EQC210) needs no weaker defence). | "Its survivors scatter and its node goes quiet" (§2.4.5): quiet in the conflict engine's sense, nothing that acts or is acted on. The economy stays S7's reading, not its business. |
| EQC210 | **The inert camp: one predicate, `Conflict::camp_active(f)`, true unless the faction is broken, and every place that makes a camp act asks it.** (1) `plan_feud` returns before it books (as Peaceful does); (2) the `Ev::Feud` handler drops a wake for a broken faction; (3) `plunder` returns without rescheduling (a refound books the next); (4) `send_party` returns `None`; (5) the plunder target filter skips a broken rival camp (a node whose faction is broken), and so does any other target search of a camp the build finds by grep (the list is closed by the `Conflict::check` invariant below); (6) `pool_ready` answers BLOCKED while the rate is 0. **Not gated:** `signal` and `set_emission` still reach a broken camp's feuds, so grievance keeps its closed form and a player who logs the ruin's forest for a year is hated by whoever comes (decay is the forgetting; the plan's half-lives are days, a continuous source is steady, and both are the world's physics). The refound re-arms every item above: `plan_feud` for each feud and `schedule_plunder` (the way `set_difficulty` ends Peaceful). The invariant `check()` gains: a broken faction has no party, `away` 0, pool 0, no live `Ev::Feud` stamp and no live `Ev::Plunder` stamp; a faction with no strain is never broken. | The Peaceful gate is the proven shape (R6). One predicate is what CONFLICT-LIFE.md's council reads before it sits (EQC226), so its `Ev::Council` is the first thing it gates. |
| EQC211 | **The refound** (`Ev::Strain` at `due`, camp still broken). `refound(f, at)`: `broken = None`; pool `floor(refound_share x cap)` (15, 10 and 12 of 60, 40 and 50) anchored at `at` with the rate of the pressure then; `away` 0; `reband` (below); `schedule_plunder`; `plan_feud` for each feud; `stats.refounds`; tape `conflict.refounded`; the row marked. **`reband(w, f, at)`: a new chief and heir** (two `create_notable` calls from the next `StreamKey(seed, SYS_CHIEF, key)` blocks of `chiefs`, `chiefs += 2`, the code `add_faction` runs, factored out and called from both), `observed` cleared, `cowed_until` zero, `crystal` true. The old chief and heir are not killed and make no news: they are left to age out as notables do (a goblin chief lives 120 days, its heir 240), so the new band holds none of their grudges (C24's heir takes grudges at a succession; a refound is not one). | §2.4.5: "a new band takes the empty camp with a quarter of the cap". A band that was scattered is not the one that comes back, and CONFLICT-LIFE.md's council (temperaments drawn from the chief's seeds) gets a different camp, not the same one waking. |
| EQC212 | **Pressure survives the break and decays on its own clock.** The strain keeps its `Pressure`; at the refound it has had `min(384, 48 x 2^P)` days of half-life 192 (P 2.1 at the break is about 1.0 at the refound). The refounded camp therefore recruits at about half its rate, climbs from a quarter of its cap, and cannot break again at once: its pool is a quarter of the cap (not thin) and the next player's loss is a new bump (`a_refounded_camp_does_not_break_again_at_once`). Fork F10: reset P at the refound (rejected: the place is still a death trap to the next band, and §2.2 makes pressure the site's). | §2.2: pressure belongs to the site, and a site left alone forgets in a few years. |

### S7 records: the ledger, the census and the formats

| # | Choice | Why |
|---|---|---|
| EQC213 | **Counts.** A pool camp's `members` for the break is the floor of the pool plus `away`; for the census it stays the floor of the pool (EQC5), so a camp with parties out reads short, as it did. K is the cap. | S1's rule, unchanged; the S7 bands read members and away together where they need to (EQC233). |
| EQC214 | **The ledger row, and no wire change.** `camp_site_state` already sends every faction's row; for a pool camp `p` becomes the strain's pressure now, and a broken camp's row is `EMPTY | BROKEN | WAITING`, count 0, away 0, `wait_left` the ticks to the refound. The found tail has no BROKEN bit and S7 does not add one (fork F11: a new `tail_flags::BROKEN` bit 16, rejected: a change to core's `SiteLedger` and every plugin's tail reader for a state that `WAITING` with count 0 already says; a pool camp never waits for an arrival, so WAITING means "refound due"). `camp_found`'s no-bodies branch, with a row, now restores: the pool to the row's count at `now`, the pressure by `Pressure::handed_over(row.p, row.elapsed, day, half_life)`, and a `waiting()` row with count 0 as a **broken camp** with `due = now + wait_now_ticks()` and its `Ev::Strain` booked; it voids the plunder `add_faction` booked. A row with count above 0 and WAITING is read as not broken (a row from another build). Without a row the camp is found full at P 0, as today. | R8. The tail is core's and every plugin's; this uses the fields that exist, and the build's protocol test pins the convention (`a_pool_camps_found_tail_restores_its_pressure_and_break`). |
| EQC215 | **Reset, reinstall, uninstall (UR-1).** S7 adds no body and no world change, so there is nothing to leave behind. A reset with the ledger keeps a farmed pool camp farmed and a broken one broken (the row, as above). A reset **with no row** (the plugin never reported the camp before, or the file was deleted) finds it full and unpressured: a pool camp is never "found empty" (it has no census), so §2.3's cap wait has nothing to attach to, and what the reset mints is warriors that carry no boss loot. Parties away at an uninstall are lost as before and booked `Other` (no pressure) at a reinstall. | §2.3 for kinds with no bodies; the plan's item 6 ("a site found empty with no ledger row") is about boss sites. |
| EQC216 | **The S1 census for `conflict.camp`.** `Conflict::census` gives each camp its pressure now (a body culture's from its `Camp`, a pool camp's from its strain), `broken` from the strain, and the kind's `renewals` as `stats.arrivals + stats.refounds`: a recruit is a rate, not an event, so the flat-events requirement of S11 reads arrivals and refounds, and `stats.breaks` beside them for the model's `events` (which counts breaks and refounds both, `camps()` in `model.py`). EQC4's "camps book none yet" and EQC5's "mean P reads 0 until S7" are superseded here. `rtsim-cli sim census` gains a `camps` scenario: `synth::many(villages, factions, seed)`, a grid of villages with goblin, bandit and raider camps in turn, nobody watching. | The numbers the long runs and S11 compare against. |
| EQC217 | **Formats.** Conflict pages **RTCD to RTCE** (`RTC14`) and the meta **12 to 13**: the faction row ends with `put_opt(strain)` after the camp (pressure `f32` and day `u32`, the optional break `since` and `due` as `u64`, `stamp` and `check_at`), the wheel's tag 12 is `Ev::Strain { faction, stamp }`, and the stats end with `breaks`, `refounds`, `break_deferrals` and `slain`. The state hash covers the strain. An older page is refused as any other (UR-1: no compatibility before a release); rtsim-view's conflict fixtures are regenerated. The site ledger's file (`mic_rtsim_sites.dat`, version 1) is unchanged. | The M12a precedent: the camp ledger is one more optional record on the faction row (MZB-1014), and so is this. |
| EQC218 | **Archive and tape.** Two kinds are appended to `kinds!` (ids are positions, no compatibility): `CONFLICT_BROKEN = "conflict.broken", "Camp breaks", Notable` and `CONFLICT_REFOUNDED = "conflict.refounded", "Camp refounded", Notable`; the tape events carry the faction and, for a break, P and the wait. Archive names and the viewer's dictionary are checked by `kinds_are_unique` and a grep of `viewer/`. | Both are events a timeline should show; neither is news (FB-4). |
| EQC219 | **Tunables** (each in the culture's `pool_renewal` unless named, every one TUNABLE, PLACEHOLDER until the S7 checks): `half_life_days` 192, `recruit_cap_days` 96, `break_share` 0.1, `break_pressure` 2.0, `refound_base_days` 48, `refound_cap_days` 384, `refound_share` 0.25; engine (`ConflictTunables`): `strain_recheck_ticks` 24,000, `district_unclaimed_is_players` true (the FB-2 switch). | One list for the build and the tests. |
| EQC220 | **Cost (D1).** Per pool camp about 56 bytes; `pool_at` gains one `exp2` (called per query, none per tick); a loss is O(1); the break check scans the faction's parties and feuds (tens) at most once per loss, party end and event, and only for a camp that is thin and pressured; the wheel holds at most one `Ev::Strain` per pool camp, and only while it is thin, pressured and busy, or broken. Nothing grows with time: a camp's strain is one row, a broken camp one event, and the model's break and refound events per day are flat. | EQUILIBRIUM.md §2.8's "conflict: one more float per faction, break and refound are one event each". |
| EQC221 | **Java: the production change is the outpost reporter (EQC237 to EQC269; Ben's FB-5 (b), 2026-10-30), and nothing else.** No `Camps` or `ConflictBridge` logic changes for the engine side. The test-only changes are: (1) the JNI test order **41 `CONFLICT_SLAY`** (`u64 faction key, u32 n, f32 pressure`: `slay` of `n`, `u32::MAX` all, then the pressure added as if that many kills had happened, so a GameTest need not run a hundred game days to reach a break; TEST mode only; 33 and 35 to 39 stay free for the peers' codes as `NativeCodec` leaves them) with its encoder in `NativeCodec` and a `RtsimTests.slayCamp`; (2) three core GameTests (EQC231); (3) the stale comments at `ConflictBridge.java:2519` and `Camps.java:2144` are corrected, because the engine now reads BAND_CASUALTIES' credit and Java already fills it fully (R4; checked by `aPetsKillOfAPartyMemberIsThePlayersLoss`, EQC231). | The row's tests are Rust; the ledger path through a real reset is the one thing a Rust test cannot show. |

### S7 records: Mowzie, Peaceful, replay and CONFLICT-LIFE.md

| # | Choice | Why |
|---|---|---|
| EQC222 | **What stays exactly as it is for the body cultures.** The Frostmaw's `renewal` (6, 192, 96, boss), its arrivals and its pressure on the `Camp` ledger; the Umvuthana grove's `recruits_per_day` 2 and its `chief_renewal`; `renewal_cause` with its boss rule; `camp_death`, `chief_killed_by` and the written-off bump; `plan_refill` and `plan_arrival`. `pool_at`, `pool_ready` and `whole_at` return what they returned for a culture with no strain (`recruit_rate` is `r`), so the whole M4 to M12a conflict suite (`tests/camp.rs`, `grove.rs`, `frostmaw.rs`, `mowzie_*.rs`, `cultures.rs`) and the Mowzie GameTests must pass with no edit. **For S9, found on the way:** the grove's warriors killed on a march by a player book no pressure (R4), and the grove's refill is a rate; S9's `renewal` on the grove is a data change plus a bump in `casualties()` for body cultures, and it must not double the bump `camp_death` makes for a body killed at the camp. | EQC200 and the S9 row; stated so that S9's author does not have to rediscover R4. |
| EQC223 | **Peaceful.** A broken camp is not Peaceful's business and Peaceful is not its: `set_difficulty` ending Peaceful plans every feud again, and `plan_feud` returns for a broken faction, so the two inert states compose. A camp that breaks while the world is Peaceful breaks as usual (parties are already held; idle is the normal case). | The only other inert gate the engine has (R6). |
| EQC224 | **Replay and the journal.** S7 adds no record, so no verdict in `outbox.rs` and nothing in `docs/replay-gate.md`. The new bump rides a journaled BAND_CASUALTIES that the engine already counts once per op (`casualty_ops`), so a replayed or reasserted record books pressure once; the break, the refound and the recruit rate are functions of the pages and the wheel, so a restored engine continues exactly (`a_restart_changes_no_decision`, EQC230). `slay` and test order 41 are TEST-mode and not journaled: a GameTest that uses them saves through (`SiteTestHooks.requestSave`) before it resets, as S3's do. | The replay gate is about records; S7 introduces none. |
| EQC225 | **Rows that exist only when a culture opts in.** `pool_renewal` is checked at `Culture::resolve`, so a malformed block stops the engine at start (as `renewal` does) and a world with the three cultures edited out of `pool_renewal` is today's world (the "renewal off" test: every existing conflict test passes with the blocks removed). | Every earlier slice kept the "as before" switch testable (S6's `renewal_off_is_todays_world`). |
| EQC226 | **How S7 stays out of CONFLICT-LIFE.md's build plan.** CL's order (2.11): (1) the council on the harness, (2) persistence of its fields, (3) the envoy's reason and the news kinds, (4) the lurk, (5) the stolen crystal, (6) BAND_MARCH and the walk-in, (7) caravan ambushes, (8) alliances. S7 touches none: no council, no sitting, no occasion, no party kind, no march, no record. S7 and CL meet at **four code points**, and each is an addition or a read, never a decision about when: (a) the gates of EQC210 are where CL's `Ev::Council` and `Ev::Clash` handlers begin; (b) `pool_at`, `pool_ready` and `bodies_ready` keep their signatures and meanings, so a muster reads them as today (a pressured camp's party is later or smaller, which is the point); (c) `reband` is the one function CL adds its per-camp state to (EQC227); (d) the faction row and the pages: CL adds its fields and S7 adds its strain, and **whichever lands second takes the next page letter** and regenerates the viewer fixtures, with a merge of the row's tail by hand. CL-D23 ("renewal, pressure, break and refound unchanged") is what S7 builds, and CL-D26's guard share reads `pool_at`, so a thinned camp defends less once both are in. Defender losses CL adds (a camp that holds pays for it) are a rival camp's or a village's work: cause `Faction`, no pressure. | The coordinator's instruction and EQC200: "when parties go out is CL's". The four points are listed so the second author finds them. |
| EQC227 | **What S7 asks of CONFLICT-LIFE.md (FB-3).** (1) Every act of a camp is gated by `camp_active(f)`; a council does not sit for a broken camp and a neighbour does not weigh one (preemption, plunder reach, a pact's partner: a pact with a camp that breaks ends as a lapse). (2) `reband(w, f, at)` is the one place a new band is made: CL initialises its per-camp state there as `add_faction` does (stores at a season's start, fear 0, standings 1.0, 0.7 and 0.6, sittings 0, temperaments from the new seeds) and clears the camp's own feud memories, beliefs, trust, pride and truces; other camps' memories of it stay and fade. (3) A pool camp's `pool_at` is the members at home, and S7 makes it follow pressure; CL may not keep a second count. (4) A broken or refounded camp is told to nobody by S7 (FB-4). | An interface another plan follows (FB-3). |
| EQC228 | **Documents the build changes.** EQUILIBRIUM.md §2.4.5 (the half-life, the finding, the break and refound as built, the stale "No Java exists yet for conflict" line, "what the player sees"), §3.3 (a pool camp's row, with the rate form), §2.8's conflict row, §4's S7 row (built), §5.2's camp row; `docs/boundary.md` (§9: a pool camp's SITE_STATE and tail convention, EQC214); this file's findings (EQC270 onward). `docs/replay-gate.md`, `docs/choices-mowzie-build.md` and MOWZIE.md are not edited. | |

### S7 records: the outpost reporter (Ben's FB-5 (b), 2026-10-30)

Ben decided that S7 includes a reporter that declares vanilla pillager outposts as raider camps, so the raid ladder runs in real
worlds. These records are the design of that reporter. What is read from the 1.21.1 data is marked; the choices that are Ben's are
FB-6 to FB-9 (after FB-5 below). The engine side of S7 (EQC200 to EQC236) is unchanged by them: an outpost's camp is the pool camp
those records describe.

| # | Choice | Why |
|---|---|---|
| EQC237 | (2026-10-30, the reporter's design) **Where it lives: one new core class, `mic_rtsim.conflict.Outposts`** (the finder, the queue and the reporter), registered beside the other core hooks in `MicRtsim` and drained from `ConflictBridge.endOfTick`. It reports through `RtsimApi.reportCamp` (R11: the API that exists for a camp with no `CampKind`, no reconciler and no bodies), so no `Camps` or `ConflictBridge` logic changes. **D4 (one plugin per mod):** a vanilla structure is core's, as villages (`VillageAdoption`) and herds are; there is no new mod, plugin or jar, and Mowzie's plugin is not touched. The structure ids are the raider culture's `structures`: `minecraft:pillager_outpost` and `illagerinvasion:illager_fort` (Illager Invasion is not in the pack, so only the first is ever reported; an id the registry lacks is skipped at start). The list is a Java constant mirrored from `raider.json`, pinned by `the_raider_claims_the_outposts` (Rust) and by the GameTest that has the engine found a camp for each id (EQC252). Fork F23: a plugin in its own module (rejected: it would add a jar for one class with no mod to depend on). | PLAN's rule that vanilla is core's; `RtsimApi.reportCamp`'s own Javadoc names `minecraft:pillager_outpost`. |
| EQC238 | **What a vanilla outpost is** (read from the server data jar and the neoform sources, not assumed). `minecraft:pillager_outpost` is a jigsaw structure (start pool `base_plates`, size 7, within 80 blocks of its centre, `WORLD_SURFACE_WG`), placed by `random_spread` (spacing 32 chunks, separation 8, frequency 0.2, `exclusion_zone` 10 chunks from any village). **Its pillagers are not placed with it:** none of its eleven templates carries a pillager (the only entities in them are one caged iron golem and two caged allays; counted from the NBT, jigsaw names excluded). They are the structure's **spawn override** (`monster`, `minecraft:pillager`, weight 1, count 1, `bounding_box: full`), so vanilla's natural spawner makes and re-makes them while a player is near, and `PatrolSpawner` and village `Raid`s are separate vanilla systems. **An outpost has no standing population to own, and nothing saved at home to count.** | The question the coordinator asked (who owns a pillager at the outpost) has this answer first, and it removes the census, the slot ledger and the home-body machinery of S3 and S5 from the reporter. |
| EQC239 | **Who owns what** (table below the records). The sim owns no body at an outpost. The vanilla pillagers there stay vanilla's: not counted in the pool, not tagged, not adopted, not removed, not recalled. The camp's 50 warriors are a number in the engine. A war party is puppets of vanilla raider entities while a player is near it, and nothing otherwise. FB-7 asks whether Ben wants the outposts' own pillagers to become the camp's bodies instead; the recommendation is no. | MOWZIE.md §4.9 and S5 give a den, a grove and a lair a vanilla home body because those sites hold a saved population; an outpost holds none (EQC238). |
| EQC240 | **How an outpost is found: `ChunkEvent.Load`, the finder pattern of `MowzieCamps` and `Lairs` (MOWZIE.md §4.9).** For a loaded `LevelChunk`, `getAllStarts()`; each valid start whose structure id is a raider structure becomes a small record (dimension, start chunk position, `late = !isNewChunk`, a copy of the start's box), bounded at 4,096 (`QUEUE_CAP`; a dropped record is found at its chunk's next load); the queue is drained at the end of the server tick once a session is live, and holds nothing from a chunk. No mixin. Fork F17: the worldgen hook that adopts villages at generation (rejected: a village is an economy node that must exist when its structure does, C22; a camp matters when a player is near, and the load path finds a generated outpost and an old one with one code path, as Mowzie's dens and groves are). **Consequence, and a bound (D1):** only outposts a player's chunks have loaded become camps, so the number of raider camps is the number of outposts explored. A player who has never come within view distance of an outpost has no raiders from it. | The load event is where every other camp is found; its cost for a chunk with no start is one empty map read. |
| EQC241 | **Key, position, once.** The faction key is `Obs.villageId(dim, start chunk)`, the start chunk of the structure, as every camp's (MZB-206); the block given to `reportCamp` is (the start chunk's middle x, the start's `minY`, the middle z), a block in the start chunk, because the key is taken from `pos`'s chunk and the engine's camp position is that block (its parties muster and come home there; the territory is centred on it). The exclusion zone of EQC238 keeps a village start 10 chunks from an outpost, so the key space it shares with villages does not collide. The reporter keeps a `known` map for the server run (key to site) and reports each key once per session (EQC246); the engine is idempotent by key and the core's `camps.put` replaces. | The key must be the start's, or one structure would be two camps after a reload. |
| EQC242 | **The camp it founds: the raider culture's, full.** Cap 50 (`pool_cap`), pool 50, P 0, `homeBodies` 0, census 0; a reset with the ledger row resumes pressure and a broken camp (EQC214, EQC215). LATE changes nothing for a pool camp. Fork F18: a smaller cap for an outpost, or a second raider culture for outposts (rejected: the 50 are the country's people the camp can call, not the one to five mobs the structure spawns; the cap sets party sizes through the muster rule; a second culture is a data fork nobody asked for). | The data is Ben-approved and the engine's tests use it. |
| EQC243 | **Territory, signals and the killing signal. FOR BEN (FB-6).** The territory is the culture's (radius 300, pollution reach +200) and every signal works as it does for any camp: smog 0.5, radiation 1.0, logging 0.05, mining 0.05, building 0.5, killing 12 (grievance per unit; half-life 15 days; rungs at 6, 18, 30, 45, 100, 200). The reporter supplies `kin`, the predicate that says which deaths are "members killed": **a mob in the vanilla `minecraft:raiders` entity tag (minus the witch), not a sim puppet, that dies inside the outpost's structure box widened by `outpostKinMargin` 24 blocks.** The existing death hook then raises Killing for a *direct* kill by a player (`ConflictBridge.onDeath`: the killer entity is a player; an arrow's owner counts; a pet's or a trap's kill does not, which is the signal's rule today and is not touched, while pressure uses the full credit, FB-2). So one kill is 12 (the notice, nothing visible), two are 24 (a herald and two pillagers come to demand emeralds), four are 48 (a raid of 6 vindicators and 6 pillagers); a player who clears or farms the outpost's own pillagers is hated by its camp, and one who kills a village raid's pillagers or a patrol elsewhere in its territory is not. | The `kin` predicate is the one place the reporter decides who the camp's "members" are, and it is how a survival player first meets the ladder. |
| EQC244 | **What a raid party is in the world.** Puppets of **vanilla entities**: raider units stand in as `Vindicator` (vindicator), `Pillager` (pillager and herald) and `Ravager` (ravager) (`BodyKinds` stand-ins; D3: no new entity, block or item). They carry the core's agent tag, are never saved by vanilla (`EntitySaveMixin`), are materialized only near a player by the machinery that exists (the far view draws the column from about a kilometre, the bodies appear stack by stack at the entity-ticking edge, MZ-15; an envoy walks up and speaks), are steered by the engine, and are captured and counted back when the player leaves. They are not members of a vanilla `Raid`, are not patrol leaders and carry no ominous banner. Makeups are the culture's: envoy a herald and 2 pillagers; scouts 2 pillagers; raid 6 vindicators and 6 pillagers; invasion 12, 10 and 1 ravager; siege 16, 12 and 3; plunder 4 and 4. | Nothing here is new: it is the goblin precedent (MZB-102) and the M10 march, with the raider culture's data. |
| EQC245 | **Vanilla patrols, raids and the outpost's own mobs are left alone** (Y20: vanilla's own mobs stay outside the sim). `PatrolSpawner`, village `Raid`s, bad omen, the outpost's natural pillagers and its caged golem and allays run as vanilla. The sim does not count, tag, adopt, steer or remove any of them, a vanilla raider is never a sim member, and a sim puppet is never a vanilla raid wave. A sim party and a vanilla patrol can stand in one place; both are raider-team mobs that target players, villagers and golems, not each other. | The inventory's rule for vanilla spawns; the cost of adopting them is FB-7. |
| EQC246 | **The module switch (U-e), the session and the config.** `RtsimConfig.on(Module.CONFLICT)` is read each drain. Off: the queue and `known` are kept (bounded) and nothing is reported (`reportCamp` journals nothing when the module is off, EQC241's idempotence covers the rest). **On, at the switch-on edge or a session start: every known outpost is reported again once, before any new one.** The engine keeps its camps across the switch (frozen, not forgotten) and its parties are captured by the existing `switchedOff`. Two config keys, flat as the file's other names: `outpostsJoin` (default true; false makes the finder inert) and `outpostsJoinExisting` (default true; false reports only outposts in chunks generated after install, `late` false; FB-9). A world on Peaceful holds every war party and envoy and vanilla spawns no pillagers either. | The U-e precedent for every conflict thing; the session list because `camps` is cleared with the session (ConflictBridge `camps.clear()`). |
| EQC247 | **Uninstall and reset safety (UR-1).** The reporter creates no entity, block, item or chunk data, and the finder only reads starts. A puppet is never saved; the camp is pages under `world/rtsim/` and a row in `mic_rtsim_sites.dat`. Remove the mod: every outpost stands as vanilla made it, its structure start untouched, spawning as vanilla (`uninstallLeavesTheOutpostAsVanilla`, EQC252). Reinstall: the chunks load again and the outposts are re-found; the ledger hands the pressure back. A reset (no `world/rtsim/`) re-finds them full and unpressured (EQC215). | The requirement the coordinator named. |
| EQC248 | **What players see change.** (1) *At an outpost:* nothing. No block, chest, mob or sound is added or taken away. (2) *Provoking it:* killing its pillagers (inside its box) or polluting, logging, mining or building within about 300 blocks (500 for pollution) raises its camp's grievance; first a notice, then a herald with two pillagers walks to the player to demand emeralds (paying ends it, refusing climbs), then raids of vindicators and pillagers, then invasions with a ravager and sieges. The march is visible from about a kilometre in the far view; the bodies appear at the ticking edge and walk in. (3) *Plunder:* the camp's war bands go for the nearest village or Ribbit village within 2,500 blocks (FB-8 sets how often); a watched one is physical. (4) *Wearing one down:* a camp whose parties a player keeps destroying recruits slower, fields smaller and later parties, and in the extreme goes quiet for about two game years and comes back as a new band under a new chief (EQC209 to EQC212). (5) *Peaceful:* nothing leaves, and vanilla spawns no pillagers either. (6) *Unchanged:* vanilla patrols, bad-omen raids, the outpost's loot and natural pillagers. | The coordinator asked for it as a list. |
| EQC249 | **Raid frequency in survival: the plunder clock. FOR BEN (FB-8).** `raider.json` has `plunder_mean_days` 8: each found camp sends a plunder band (4 vindicators and 4 pillagers) at the nearest village or Ribbit village within 2,500 blocks on a Poisson clock of mean 8 game days. A village near three explored outposts would be plundered about every 3 days. Taken (and recommended): the mean becomes **48 days** (about twice a game year for a found camp) until CONFLICT-LIFE.md decides when parties go out (its council replaces this clock), a one-number data change; the tests that depend on 8 days pass an explicit culture (`conflict_with`). **Ben's answer (2026-10-30) made the 48 an average of a wait that depends on the camp: EQC285 to EQC290.** The ladder against *players* is grievance-driven and unchanged. | The plan keeps the clock as "today's"; with the reporter it meets real villages (FB-8). |
| EQC250 | **Existing worlds (FB-9).** An outpost in a chunk generated before the mod is reported when the chunk loads, flagged LATE, with a full pool and P 0, and its plunder clock starts then. `outpostsJoinExisting` false restricts the reporter to outposts in new chunks. The count of camps grows only as the player explores; a deployed server turns on the raiders of every outpost its players load. | Ben's question named it. |
| EQC251 | **Cost (D1).** A chunk load with no start is one empty map read; the queue holds records, not chunks; an outpost found costs a faction row (about 400 bytes with its strain), a node, a ledger row (48 bytes) and one `known` entry; a party exists only while it marches. Bounded by the outposts a player has loaded (about one in 1,024 chunks of explored land). | EQUILIBRIUM.md §2.8. |

| EQC252 | **The reporter's tests.** **GameTests** (core, native engine only, batch `mic_rtsim_outposts`, each on its own far base as S3's; a **real** vanilla outpost: the test generates a `minecraft:pillager_outpost` start with `Structure.generate` at a chunk 8 chunks from the arena (the way `aVillageJoinsWhenItsStructureStartGenerates` makes a village), sets it on the chunk and feeds the finder the chunk through its public load hook, as Mowzie's tests do): `anOutpostIsFoundOnceWhenItsChunkLoads` (three loads of one chunk and a second session after `plainReset` give one FACTION_FOUND per session for the key, the engine's TERRITORY comes back with radius 300, the ledger row says K 50, count 50, P 0; the raider culture claims `minecraft:pillager_outpost`); `anOutpostFollowsTheConflictSwitch` (module off: nothing journaled, the queue kept; on: reported once; off and on again: once more); `existingWorldsOutpostsJoinLate` (a chunk with `isNewChunk` false: FACTION_FOUND flag LATE; `outpostsJoinExisting` false: none; `outpostsJoin` false: none); `clearingAnOutpostIsGrievanceAndAVillageRaidIsNot` (a real server player kills two vanilla pillagers standing in the box: the engine's Killing signal arrives, 24; the same kills 100 blocks outside the box: none; a pet's kill: none); `aProvokedOutpostSendsItsEnvoy` (**the ladder sends a party**: two kills, then the engine's envoy rung after its dwell, a BAND_MARCH of a raider envoy from the outpost to the player, its bodies placed near him: one Pillager standing in for the herald and two more, each carrying the agent tag, none saved by a vanilla write of the chunk, and a `Raid` object created for none); `aFarmedOutpostBreaksAndTheNewBandRefounds` (order 41 wipes the camp with pressure 2.5: its row reads `EMPTY | BROKEN | WAITING` with the wait left, and no party leaves it for a game day of ticks; `gapRestart` by 250 days: the new engine finds the row, the wait has run out, the camp is refounded at once with count 12, not broken, P decayed and non-zero, and a kill of two pillagers sends an envoy again); `aVanillaPatrolAndTheOutpostsOwnPillagersAreLeftAlone` (a command-spawned patrol and a pillager the spawn override made are untagged and uncounted after 2,000 ticks with a party out, and are neither adopted nor removed); `uninstallLeavesTheOutpostAsVanilla` (a party materialized, then `SiteTestHooks.stop` and a read-back of the chunk's entities and starts: the start is intact and equal to what was generated, no entity carries the agent tag, nothing of the sim is in the chunk; the ledger file holds only its rows). **Failing first:** none of the reporter exists on 9389b4e5, so the first run of each is red by `no FACTION_FOUND reached the engine` (the first three), `the engine got no Killing signal` (the fourth), `no band was sent` and the following (the rest); they are written after the finder compiles, as S5's were, and the record says so. **Rust:** `the_raider_claims_the_outposts` (`Cultures::builtin()` claims `minecraft:pillager_outpost` and `illagerinvasion:illager_fort` for the raider culture and no other), `the_raider_plunders_at_the_slower_mean` (EQC249); both fail first on the old data. | The coordinator's list: declared once, the ladder sends a party, a farmed outpost breaks and refounds with the new band, uninstall; plus the switch, the late flag, the kin rule and the vanilla-left-alone rule. |
| EQC253 | **Clocks in these tests.** The ladder test waits for real game ticks (the envoy rung's dwell is 12,000 ticks and the march from 128 blocks at 0.03 blocks a tick is 4,300; the server ticks unthrottled, EQC193, and the test has its own batch and a timeout of 40,000 ticks). The refound does not wait: the camp's wait is at least `48 x 2^2` = 192 game days, so the test jumps the clock with `SiteTestHooks.gapRestart`, whose found tail ages the pressure and the wait by the gap (EQC214; UNVERIFIED 5 is exactly this). No conflict-advance order is added (fork F24: a test order that runs the conflict engine ahead of game time, as `P1_ADVANCE` does phase 1, rejected: the ledger's clock is game time and an engine ahead of it would write rows from the future). | Fewer ids and no skewed clock. |
| EQC254 | **Documents the reporter changes.** EQUILIBRIUM.md §2.4.5's "Body" paragraph (there is a reporter; an outpost's pillagers are vanilla's, EQC238 and EQC239) and §1.1's camp line; `docs/boundary.md` (§9: nothing new on the wire; the reporter and its keys); the conflict section of `docs/choices-mowzie-build.md` is not edited; this section. | |

**Who owns which body (EQC239):**

| Body | At the outpost | Away | After an uninstall |
|---|---|---|---|
| The outpost's natural pillagers (structure spawn override) | vanilla's: spawned, saved and despawned by vanilla; untouched by the sim | not applicable (vanilla's patrol AI) | stay as vanilla left them |
| The caged iron golem and allays | vanilla's | | stay |
| The camp's 50 warriors | a number in the engine; no body, no census, no CAMP_RECONCILE | | nothing exists |
| A war party's members | none (they are not at the outpost) | puppets (core agent tag, never saved) of `Vindicator`, `Pillager`, `Ravager`, only near a player; counted back as survivors | never saved, so none; booked `Other` at a reinstall |
| An envoy (herald and 2 pillagers) | none | puppets, as above | as above |

### S7 forks

Each is in the record named; the option taken is the conservative one where I was unsure (fewest changes to what exists, nothing
a player sees that Ben has not approved, nothing minted).

| Fork | Options | Taken | Record |
|---|---|---|---|
| F1 where a pool camp's pressure lives | a box on the faction; the `Camp` ledger; a field on every faction | the box, `Some` only with `pool_renewal` | EQC201 |
| F2 how P is read for the rate | at the last event; every step; a booked re-read | at the last event (the plan's reading; 3 to 6 days on a kill-all's 90%, none on the break) | EQC203 |
| F3 a floor on the rate | none; one recruit in 96 days | the floor | EQC203 |
| F4 which deaths are the player's | credited kills only; plus every non-mob death at a player's district | the second, so a watched and an unwatched raid book the same (FB-2) | EQC204 |
| F5 where the new bump runs | for every culture; for pool cultures only | pool cultures only; S9 decides the grove | EQC205 |
| F6 the break with a party out | break at once and end its parties; wait until idle | wait until idle | EQC207 |
| F7 how breakability is noticed | a daily poll; events at the points that change it plus one daily recheck while busy | events and the recheck | EQC208 |
| F8 a broken camp's node | untouched; pop to the floor | untouched | EQC209 |
| F9 who comes back | the old chief and heir; a new band | a new band | EQC211 |
| F10 pressure at the refound | kept and decaying; reset | kept | EQC212 |
| F11 the broken state across a reset | a new BROKEN tail bit; WAITING with count 0 | WAITING with count 0, no wire change | EQC214 |
| F12 the half-life and the break (FB-1) | 192 and break at 2; 96 and break at 1.25; faster recruits; the plan's numbers | 192 and break at 2 | EQC203 |
| F13 where the numbers live | `pool_renewal`; reuse `renewal`; engine tunables | `pool_renewal` in the culture | EQC202 |
| F14 news of a break or a refound | none; gossip kinds | none (FB-4) | EQC209 |
| F15 how a test kills a pool camp | `slay`; drive parties through a fake Java only | both: `slay` for the long runs, the fake Java for the credit paths | EQC206 |
| F16 what a census calls a renewal | every recruit; arrivals and refounds | arrivals and refounds | EQC216 |
| F17 how an outpost is found | `ChunkEvent.Load`; the worldgen hook villages use; both | chunk load only | EQC240 |
| F18 an outpost's camp size | the culture's 50; a smaller cap; a second culture | the culture's | EQC242 |
| F19 which deaths are the camp's members killed (FB-6) | raider mobs inside the box and 24 blocks; anywhere in the territory; none | inside the box | EQC243 |
| F20 the outpost's own pillagers (FB-7) | vanilla's; the camp's home bodies | vanilla's | EQC239 |
| F21 the plunder clock (FB-8) | mean 8 days; 48; off | 48 days | EQC249 |
| F22 existing worlds (FB-9) | join with a switch; new chunks only; inert by default | join, with the switch | EQC250 |
| F23 where the reporter lives | core; a plugin of its own | core (D4: vanilla is core's) | EQC237 |
| F24 how a test reaches a refound | `gapRestart`; a conflict-advance order | `gapRestart` | EQC253 |

### FOR BEN

Everything else in S7 can be undone and is decided here. These are what a player sees at a camp, a number of the plan Ben
approved, or an interface another plan follows. Each is built as recommended; a different answer is a data change or a few
lines, as noted.

**FB-1 (EQC203, F12), DECIDED by Ben 2026-10-30: (a), half-life 192 for pool camps, break at P 2. The break does not fire at the engine's recruit rates. Which number moves?** EQUILIBRIUM.md §2.4.5 breaks a
camp at P 2 with the pool under 10%, on the shared half-life of 96 days. The model that chose those numbers recruits 8% of a
camp a day; the engine's goblin, bandit and raider camps recruit 3.3%, 3.75% and 4%. A player who kills every whole warrior
every two days holds P at 1.58, 1.67 and 1.72 and never reaches 2 (`camps.py`, section A and B), so a pool camp can never be
broken by any player, and the row's test would have to inject its pressure.
- (a) **Half-life 192 days for pool camps, break at 2 (Recommended).** The same farmer reaches 2.13, 2.24 and 2.29; the first
  break comes on day 144 to 218, then 0.23 to 0.30 breaks a year with the camp empty about half the time; a wait of 192 to
  236 days (two game years and a little, the plan's "two to four"). A player must take 72% to 87% of what the camp recruits,
  for months. Breaks are rare and deliberate, which is the conservative reading of "only players break camps".
- (b) Half-life 96, break at P 1.25. A player taking 54% to 64% of what it recruits breaks it; the first break on day 28 to 38,
  0.57 to 0.63 a year, empty 70% of the time, a wait of 114 days at the break (a little over a game year). Camps go quiet
  readily, so the world has fewer raids from a harried camp.
- (c) Raise the three cultures' recruit rates to the model's 8% a day. Changes the size of every party a camp can field.
- (d) Keep the plan's numbers. A pool camp never breaks; the refound exists and is tested by an injected pressure.
Why it is Ben's: a number of a plan he approved that changes how a camp he is hunting behaves. Switching is two JSON numbers.

**FB-2 (EQC204, F4), DECIDED by Ben 2026-10-30: (a), trap, turret and unclaimed kills of raiders at a player's district count as that player's loss. Do a trap's, a turret's and an unclaimed kill at a player's district count as his loss?** A pool camp's
members die in a player's fights in two ways: an unwatched raid is resolved by the engine against the district's defences, and
every unit that falls is the player's loss; a watched raid is fought in the world, and Java credits each death.
- (a) **At a player's district every death no wild mob dealt is his (Recommended).** Lava, a Create contraption, a turret, a fake
  player: the base is his defence. The watched and the unwatched raid book the same pressure, and a trap-defended base thins
  the camp as a sword-defended one does. On the road and at a village only a credited kill counts.
- (b) Credited kills only, everywhere. A base defended by machines never thins its attackers' camp, and the same raid books
  pressure unwatched and none watched.
Why it is Ben's: how fights feel at a base he builds. (b) is the literal §2.2 for a body that carries no boss loot.

**FB-3 (EQC226, EQC227, F9), DECIDED by Ben 2026-10-30: (a), S7 defines `camp_active`, `reband` and the stable pool API, and whichever of S7 or CONFLICT-LIFE.md lands second takes the next page letter. The interface CONFLICT-LIFE.md must follow.** `camp_active(f)` gates every act (a council does not
sit for a broken camp, a broken camp is nobody's neighbour); `reband` is the one place a new band is made and where CL's
per-camp state starts afresh; a pool camp's `pool_at` is the one count of members at home; the page letter goes to whoever
lands second.
- (a) **S7 defines them and CL's build follows (Recommended).**
- (b) Defer the break and the refound until the council is built, so the gates are written once. S7 is then pressure on recruits
  alone, and `farmed_camp_breaks_and_refounds` waits for CL.
Why it is Ben's: an interface another plan follows, and it touches CL's scope. The gates are one-line reads either way.

**FB-4 (EQC209, F14), DECIDED by the coordinator 2026-10-30: (a), my recommendation (reversible): no news of a break or a refound in S7. Is a break or a refound news?** CL-D56 makes all faction news in-world gossip, carried by traders and
villagers (CONFLICT-LIFE.md 2.17, TALK). A camp that breaks, or a new band that takes a ruined camp, is the kind of thing a
villager would say.
- (a) **No news in S7 (Recommended).** The archive's timeline and the census show it; no `ReportKind`, no talk line.
- (b) Two report kinds (`CampBroken`, `CampRefounded`) witnessed by the nearest villages and told as gossip, with their lines.
Why it is Ben's: it touches CL's news design and what players hear. (b) is a TALK slice's work and does not block S7.

**FB-5 (EQC221, EQC237, R11), DECIDED by Ben 2026-10-30: (b), NOT my recommendation. Nothing in the shipped pack reports a pool camp. Does S7 get a reporter?**
`RtsimApi.reportCamp` is for a plugin to call, and none called it for goblin, bandit or raider camps; the Mowzie plugin declares groves
and dens. Raider camps could be the vanilla pillager outposts and the Illager Invasion forts (the culture's `structures` already name
them). Reporting them turns on today's ladder (EQUILIBRIUM.md §2.5: 13 to 14 raids a year lost at a fortress, an arms race with a
player who keeps building walls) in real worlds, with S7's pressure and break on top.
- (a) No reporter now; S7 is engine groundwork (my recommendation, not taken).
- (b) **A raider reporter for vanilla outposts in S7's scope (Ben's choice), so the pressure and break are met in the pack.**
Taken: (b). The design is EQC237 to EQC254 (EQC255 to EQC269 kept); the choices it leaves to Ben are FB-6 to FB-9. S7 changes nothing at a den or a grove.


**FB-6 (EQC243, F19), DECIDED by Ben 2026-10-30: (a). What counts as killing an outpost's members?** The camp's grievance rises 12 for each member a player kills in
its territory, and the reporter's `kin` predicate says which mobs are members (the signal itself counts a direct kill by a player; a
pet's or a trap's kill does not, as for every camp). Two kills bring a herald and two pillagers to demand emeralds, four a raid of
twelve, ten or so a siege. Clearing or farming an outpost is among the first things a survival player does with one, and a
vanilla pillager farm at an outpost is a common build.
- (a) **Raider-tag mobs (not the witch) dying inside the outpost's box and 24 blocks round it (Recommended):** clearing or farming
  the outpost angers its camp; killing a village raid's pillagers or a patrol elsewhere in its territory does not.
- (b) Raider-tag mobs anywhere in the camp's 300-block territory: a village raid defended by the player angers every outpost within
  300 blocks.
- (c) None: the camp is angered only by pollution, logging, mining and building (and by killing its own sim warriors). Outposts
  can be cleared for free.
Why it is Ben's: how fights feel, and the first thing a player does at an outpost. Switching is the predicate in `Outposts` and one
config number (the margin).

**FB-7 (EQC239, EQC238, F20), DECIDED by Ben 2026-10-30: (a), no, the camp is a number. Do an outpost's own pillagers become the camp's bodies?** They are the structure's natural spawns
(none is placed with it), so there is no saved population at an outpost; the camp is a pool of 50 that the sim keeps as a number and
sends out as puppets, as the goblin culture does.
- (a) **No (Recommended):** the pillagers there are vanilla's and the camp's warriors are a number. Nothing is adopted, counted,
  removed or recalled; uninstall leaves nothing.
- (b) Yes, as home bodies of a body camp (a census, a slot ledger, adopt and hand back, as the dens and S5's lairs): the raider
  culture gains `camp_bodies`; the cap would be the one to five pillagers standing, vanilla's spawner would keep making more that
  the census must adopt or remove, a player's kill of one would book pressure, and S9's machinery for the grove's warriors is the
  prerequisite.
Why it is Ben's: body ownership (MOWZIE.md §4.9, EQ-2) and what an uninstall leaves. (b) is a plan of its own.

**FB-8 (EQC249, F21), ANSWERED by Ben 2026-10-30, and not as a number: "should probably depend on the raiders, w/ some RNG and dependence upon historical data for the raiders comprising the camp, their experiences, temperaments, etc. Though ~48 days is a good avg." Built as the plunder mood (EQC285 to EQC289 below): the mean stays 48 days, bent by the chief's temperament and the camp's recent raids. How often do raiders raid villages in survival?** Each found camp sends a plunder band at the nearest village
or Ribbit village within 2,500 blocks on a clock; the culture's data says a mean of 8 game days. Targets are the culture's
`hates` (humans and Ribbits; goblins and bandits do not exist), nearest first, so one village bears them; a plunder a player
watches is physical (pillagers at the village).
- (a) The data as it is: mean 8 days. A village near three explored outposts is plundered about every 3 days.
- (b) **Rarer (Recommended): mean 48 days,** about twice a game year for a found camp, until CONFLICT-LIFE.md decides when parties
  go out.
- (c) Off: the camp acts only on grievance, against players.
Why it is Ben's: whether and how often villages in his survival worlds are raided. The ladder against players is FB-6's.

**FB-9 (EQC250, F22), DECIDED by Ben 2026-10-30: (a), yes, any outpost whose chunk loads joins, flagged LATE, with the `outpostsJoinExisting` switch. Do existing worlds' outposts join?** The finder reports an outpost whenever its chunk loads, so a world made
before the mod gets raider camps from every outpost its players load, with a full pool and no grievance (they matter once provoked
or on the plunder clock).
- (a) **Yes, with a switch (Recommended):** every outpost whose chunk loads joins (flag LATE); `outpostsJoinExisting` false limits
  it to new chunks.
- (b) Only outposts in chunks generated after the mod: existing worlds see no raiders from old outposts.
- (c) Ship inert (`outpostsJoin` false) and turn it on per world.
Why it is Ben's: it decides what a deployed server (LiveSrv) looks like after the update.

**FB-10 (EQC288, F26). Which cultures plunder on their history?** The mood is a block of six numbers in a culture's JSON. It is on the
raider only: the goblin and bandit camps (plunder means of 12 and 6 days, and no reporter in the pack) and the grove (12 days, a body
camp that is S9's) keep their clocks as they were, bent by their chiefs' aggression alone.
- (a) **The raider only (Recommended; built):** the raiders are the camps Ben's worlds will have (the outposts); the others change
  nothing today.
- (b) Also the goblin and the bandit: one block each. Their mean waits would be exactly the data's 12 and 6 days (the mood centres the
  temperament), 9% shorter than today's, which average 1.10 times the data's; and a camp that loses members raids less, one that wins more.
- (c) Every plundering culture, the grove included: S9's to decide (its warriors are bodies and its losses go through CAMP_DEATH).
Why it is Ben's: it changes how often villages and players meet the camps of cultures a pack author may add. A change is one JSON
block, no code.

**Told, not asked** (all inside EQ-5, EQ-6 and the rules above): the conflict pages RTCD to RTCE (the engine's own save, no
migration); the reporter is core's and has no mod of its own (D4: villages and herds are core's too); a refounded camp has a new chief and heir; a broken camp's market is untouched; a camp with a party out breaks
only when it is idle; the three cultures carry `pool_renewal` and the body cultures do not; the stale comments about
BAND_CASUALTIES are corrected.

### Ids S7 takes

Checked against `docs/boundary.md` (§2.3, §9.1 and the ranges of §6.1 to §13.1), `crates/rtsim-conflict/src/boundary.rs` and
`persist.rs`, `crates/rtsim-jni/src/orders.rs` and `wire.rs`, `NativeCodec.java`, `ConflictRecords.java`, `Obs.java` and
`Cmd.java`, `rtsim-kernel/src/sites.rs` and `renewal.rs`, `rtsim-archive/src/kinds.rs`, and this file's S5 and S6 tables, at
9389b4e5 and on every local branch (`git grep`; no branch other than this one touches the conflict crate).

| Space | Taken by S7 | Used today | Free left nearby |
|---|---|---|---|
| Observation record kinds | none | conflict 0x0090, 0x0091, 0x0800 to 0x0817; S5's 0x0510 to 0x0512; core 0x0001 to 0x0005, 0x0010, 0x0011, 0x0100 to 0x0111, 0x0130 to 0x0132; M12a's SITE_STATE is a command | 0x0818 and up in the conflict range |
| Command record kinds | none | conflict 0x8800 to 0x880C (BAND_MARCH 0x880B); S5's 0x850A; core 0x8001 to 0x8006 | 0x880D and up |
| Flags, aux bits, dictionary kinds, `SiteKind`, `ReportKind`, JNI exports | none | SITE_STATE flags 1, 2, 4, 8; found tail flags 1, 2, 4, 8; `SiteKind` 1 to 3 | tail flag 16 (not taken, F11) |
| Conflict `Ev` wheel tag | 12 `Strain` | 0 to 11 | 13 and up |
| Conflict page magic and meta | `RTCE`, meta 13 | `RTCD`, meta 12 | `RTCF` (whoever lands second) |
| Test orders (`orders.rs`, `NativeCodec`) | 41 `CONFLICT_SLAY` | 1 to 9, 16 to 32, 34, 40 | 33 and 35 to 39 (the codec leaves them for peers), 42 and up |
| Archive kind names | `conflict.broken`, `conflict.refounded` | the fifteen `conflict.*` kinds; no other name with `broken` or `refound` | |
| Config keys (`RtsimConfig`, flat names) | `outpostsJoin`, `outpostsJoinExisting`, `outpostKinMargin` | `spawnQuotaPerTick`, `puppetPassInterval`, `puppetStateInterval`, `playerInterval`, `implementation`, the module keys, S5's `homeLeash` and the rest | |
| Core Java classes | `conflict/Outposts` (the reporter) | | |
| Culture JSON | `pool_renewal`; `raider.json` `plunder_mean_days` 8 to 48 (FB-8) | `renewal`, `chief_renewal`, `camp_bodies`, `chief_body`, `chief_leads`, and the rest | |
| RNG systems | none (`reband` uses `SYS_CHIEF`'s next blocks) | | |
| Formats | conflict pages RTCD to RTCE, meta 12 to 13 | the site ledger file is untouched (version 1) | |
| EQC ids | EQC200 to EQC269 | S6 used EQC150 to EQC193 | EQC270 to EQC299 kept for the build |

### S7 tests

Test names are the build's. Rust tests go in `crates/rtsim-conflict/tests/strain.rs` (new) unless a crate is named; every long
run is `#[ignore]`d with a short always-run version, as EQC21 did, and runs `nice -n 10 cargo test --release`. The camps
harness is `synth::many(villages, factions, seed)` (EQC216), with a Roamer (the model's: every second day a step among the nine
regions around it, refused beyond `min(6, 2 + day / 48)` regions of spawn, and with chance 0.2 a `slay` of the camps in its
region) and a Farmer (the model's: a `slay` of every whole warrior of one camp every second day).

#### EQC230: Rust, each change with its failing-first test

| Change | Test (names are the build's) | Failing first because |
|---|---|---|
| Culture data (EQC202, EQC225) | `pool_renewal_parses_and_validates` (the three cultures carry it, the Frostmaw and Umvuthana do not; refused with a `renewal`, with `camp_bodies`, with `recruits_per_day` 0, with a number not finite, a share outside its range, a cap under its base); `with_the_blocks_removed_every_existing_test_passes` (run as `renewal_off_is_todays_conflict`) | the field does not exist |
| The rate (EQC203) | `recruits_slow_by_two_to_the_p` (a kill-all at P 1.0: 1 a day for a goblin camp, not 2); `the_rate_floor_holds_at_absurd_pressure` (a row with p 40 recruits one in 96 days); `a_bump_integrates_the_gap_at_the_old_pressure` (a long quiet gap then a loss: the pool before the loss is the unpressured line); `pool_ready_agrees_with_pool_at` (property over random sequences of slay, muster, return and gaps: `pool_at(pool_ready(n)) >= n` and never earlier by a tick) | `pool_at` reads `recruits_per_day` alone; `renewal_bump` bumps before `party_lost` re-anchors |
| Losses (EQC204, EQC205) | `a_credited_kill_of_a_party_member_adds_one_over_cap` and `a_mob_kill_adds_nothing` (BAND_CASUALTIES through the fake Java); `an_unclaimed_death_at_a_players_district_is_his` and `an_unclaimed_death_on_the_road_is_not` (FB-2 (a)); `a_raid_lost_at_a_players_walls_adds_the_lost_share` (the abstract resolve, 0.5 of the missing); `a_raid_lost_to_a_village_adds_nothing`; `a_war_chief_killed_by_a_player_adds_one_over_cap`; `a_killed_envoy_adds_nothing`; `a_replayed_casualty_bumps_once` (a copy, a reassert and a replay of one op); `the_body_cultures_book_what_they_booked` (the Frostmaw den's and the grove's pressure, counters and stats unchanged over the M9 and M12a scripts, and a BAND_CASUALTIES of a grove warrior books none) | `casualties()` books no pressure for anyone; `renewal_bump` returns for a culture with no `renewal` |
| Break (EQC207, EQC208) | `a_camp_breaks_when_thin_at_p_two`; `a_camp_does_not_break_under_p_two_or_when_not_thin` (59 of 60 at P 3; 0 of 60 at P 1.9); `a_camp_with_a_party_out_waits_and_breaks_when_it_ends` (rechecked each game day, one event at a time, breaks at the party's `home`); `a_demand_open_defers_the_break`; `a_pool_that_recovers_clears_the_check` (the stamp is bumped, no stale event acts) | no break exists |
| The inert camp (EQC209, EQC210, EQC223) | `a_broken_camp_sends_nothing` (a plunder, a feud step, an envoy, scouts, a muster, with grievance high and signals arriving); `a_broken_camp_is_not_a_plunder_target`; `a_broken_camps_grievance_keeps_its_closed_form`; `a_broken_camp_in_a_peaceful_world_wakes_correctly` (set_difficulty twice); property `no_broken_camp_ever_acts` over random slay, ticks, signals, difficulty and restarts, with `Conflict::check` extended: a broken faction has no party, `away` 0, pool 0, no live feud or plunder stamp; a faction with no strain is never broken | no gate |
| Refound (EQC211, EQC212) | `a_camp_refounds_after_the_rule_with_a_quarter_pool` (`min(384, 48 x 2^P)` days, 15, 10 and 12); `a_refounded_camp_has_a_new_chief_and_heir_and_no_old_fights` (handles differ, `observed` empty, no feud keeps a booked step); `a_refounded_camp_keeps_its_decayed_pressure_and_does_not_break_at_once`; `a_refounded_camp_plans_its_plunder_and_its_feuds` | no refound |
| Ledger and found (EQC214, EQC215) | `a_pool_camps_row_carries_pressure_and_the_refound_wait` (flags `EMPTY | BROKEN | WAITING` when broken, `SHORT` when thin, `p` now); `a_reset_resumes_a_pool_camps_pressure_and_its_break` (`FoundRow::from_state` of a row, aged by `elapsed`, into a new engine); `a_camp_found_with_no_row_is_full_and_unpressured`; `a_row_with_count_and_waiting_is_not_broken` | the row has `p` 0 and `camp_found` keeps only the count |
| Formats (EQC217) | `pages_round_trip_rtce` (a strained, a broken and a plain camp, paged and whole); `an_older_conflict_format_is_named_not_read` (RTCD); `the_state_hash_covers_the_strain`; `a_restart_changes_no_decision` (`CampRun::round_trip` mid-wait, mid-break and across a refound) | the codec has no strain |
| The census (EQC216) | in `rtsim-cli` `sim/census.rs`: `census_reports_pressure_broken_and_renewals` and `census_counts_every_kind` extended with the `camps` scenario | `Conflict::census` reads P 0, broken false and no renewals |
| The row's tests | in `rtsim-cli` `sim/equilibrium.rs`, **`kill_all_camps`**: every camp of the `camps` world killed by a player (`slay`) in year 10, the control the same seed with no kill; members (pool and away) over the control, median over seeds; passes when the median reaches 90% of the control within **96 days** (the rule reads 45 to 54) and every seed within 120, and no camp breaks (a kill-all is P 1.0). **`farmed_camp_breaks_and_refounds`**: the Farmer on one camp for 60 years; asserts the first break at day 150 to 260 for a goblin camp (the script: 218), the break's wait equal to `min(384, 48 x 2^P)` read at the break within a day, the refound pool a quarter of the cap, breaks a year within 25% of the script's (goblin 0.23, bandit 0.28, raider 0.30) and the broken share within 0.1 of it; a second assertion with the half-life 96 and break 2: **no break in 60 years** (the finding, pinned) | the first needs P on the recruit rate; the second needs the break |

#### EQC231: protocol and GameTests

**rtsim-jni** (`tests/protocol.rs`, conflict): `a_pool_camps_found_tail_restores_its_pressure_and_break` (SITE_STATE out, a reset, FACTION_FOUND in with the tail the host's fake ledger hands back), `a_test_order_slays_a_camp` (order 41 round trip, TEST mode only, refused otherwise), `a_credited_band_casualty_bumps_the_pool_camp` (the record as Java writes it, through the native engine).

**GameTests** (`neoforge/core`, native engine only, each its own batch on its own far base as S3's, skipping recorded on the stub; `ConflictKit`'s goblin camp declared by `reportCamp`, `SiteTestHooks` for the save and the reset, order 41 for the kill):

| Test | What it asserts | Failing first on 9389b4e5 because |
|---|---|---|
| `aPoolCampKeepsItsPressureAndItsBreakAcrossAReset` | a camp is wiped with order 41 and given pressure 2.5, so it breaks; its ledger row names P and WAITING with the wait left; a plain reset (`plainReset`): the new engine's camp is broken with that wait left and the same P; after the refound the row says count 12 and a P that has decayed, not zero | the row says P 0 and never WAITING for a pool camp, and `camp_found` restores only the count |
| `aBrokenPoolCampSendsNoParty` | a broken camp with grievance high and a player in its territory sends nothing for 30 game days (`P1_ADVANCE`); the refound sends one | no break |
| `aPetsKillOfAPartyMemberIsThePlayersLoss` | a tamed wolf's kill of a party member of a pool camp raises its pressure by 1/cap (the credit chain Java to engine) | the engine reads no BAND_CASUALTIES credit |

#### EQC232: tests that change

None change meaning. `tests/cultures.rs` lines 325 to 393 (the pool's closed form with `pool_at` set by hand) hold at P 0; `rtsim-cli` `sim/census.rs` `conflict` scenario gains the camps' P column (the goblin camp's P is 0 there); the viewer fixtures that hold conflict pages are regenerated (EQC217). Any test that kills a pool camp's party at a district and then reads `pool_at` over a long gap now sees the slower line: the build greps `LossCause::Player` and `abstract` in `tests/` and lists them (UNVERIFIED 4).

### S7 bands (EQC233)

The pass bands in the test names are the model's, and the build replaces them with the engine's own, recorded in EQC270 onward as S2, S4 and S6 did. Provisional, from `camps.py` and §3.6:

| Check | Band | Basis |
|---|---|---|
| No player, 60 years: no camp breaks, pool and away over cap | 0 breaks; within 0.05 of the same engine with `pool_renewal` removed | pressure is zero without a player: pools are today's |
| `kill_all_camps` | median 90% of the control within 96 days; every seed within 120; no break | §4's row; the rule reads 45 to 54 |
| `farmed_camp_breaks_and_refounds` | first break day 150 to 260 (goblin); 0.23 to 0.30 a year; broken share 0.47 to 0.60; wait `min(384, 48 x 2^P)` | `camps.py` B and E |
| The roamer, 60 years, years 30 to 60 | pool and away over the control's: median at least 0.9; camps broken at any time at most 5%; the model's 0.97 is the target, the engine's lower recruit rate the reason for the margin | §3.6's "camp pools over cap, roamer 0.97" |
| Unattended, 2,739 game years (S11's `who_is_alive_soak`) | `conflict.camp` flat by century: 0 breaks, 0 refounds, mean P 0, members near cap less the parties out | §3.6 (camps 1.00, events flat) |
| Cost | breaks and refounds a day in the roamer run at most 0.01 per camp; wheel entries for strain at most 2 per camp ever pending | EQC220 |

### S7 walkthrough: Ben wipes a raider outpost every forty days

A raider camp (K 50, 2 recruits a day, the pool block of EQC202), found full. Ben is hunting it: each time he stands where its
parties arrive he kills whatever the camp has sent, which in the engine's terms is all of its whole warriors. (With the reporter
of EQC237 the camp is a vanilla pillager outpost that a player's chunk load has found; the numbers are `camps.py` section E.)

| When | What happens | P after | Pool |
|---|---|---|---|
| Day 0 | 50 killed; the camp is thin, P is under 2, nothing breaks. Recruits fall from 2 to 1 a day. | 1.00 | 0, then 40 by day 40 |
| Day 40 | 40 killed. | 1.67 | 0, rate 0.63 a day |
| Day 80 | 25 killed. | 1.94 | 0, rate 0.52 |
| Day 120 | 21 killed. P is 2.10 and the pool is under 5: the camp is thin and pressured. It is idle (no party, no demand), so it breaks that tick: `conflict.broken`, pool 0, the plunder clock and every feud's step voided. The refound is `48 x 2^2.10 = 206` days away. | 2.10 | 0 |
| Days 120 to 326 | Nothing leaves the camp and no rival camp raids it. Ben's walls are quiet. His grievance with it, if he had any, decays by the closed form. | 1.0 by the end | 0 |
| Day 326 | A new band takes the camp: `conflict.refounded`, 12 warriors, a new chief and heir, no old fights. It recruits at about 1 a day and plans its plunder. | 1.0 | 12 |
| Day 326 on | If Ben leaves it alone P halves every 192 days and the camp is back to 90% in about 35 days. If he wipes its twelve at once P is about 1.25: the camp is thin, but under 2, so it does not break. | | |

A mob's kills, a raid lost at a village and a Peaceful world add nothing at any step.

### S7 what it leaves (EQC234)

- **Only vanilla pillager outposts are reported** (the Illager Invasion fort id is in the culture and is reported if its mod is ever present); no goblin or bandit camp has a reporter.
- **No news** of a break or a refound (FB-4); the timeline and the census show them.
- **The grove and the den** are S9's; the grove's warriors still regrow by `recruits_per_day` 2 and a grove warrior killed on a
  march still books no pressure (R4, EQC222).
- **A broken camp's market** is untouched and still trades and holds its stock (EQC209, F8).
- **Home kills.** A pool camp has no body at home, so a player can lower its pool only through its parties: one who stands where a
  party arrives and kills it does the work, and one who never meets a party never raises its pressure.
- **A camp found by a plugin with no ledger row after a reset** comes back full and unpressured (EQC215).
- **Recruiting while a party is out** is the existing room rule: a band's survivors always have their slots (ledger 6).
- **The viewer** has no camp card for P or broken; the timeline shows the two kinds and `sim census` the numbers.
- **The model's camp feuds** (§3.6: camps at strength do not attack each other) are not S7's: they were the rejected gate's, and
  camp against camp is CL's.

### S7 risks

1. **The finding is a number of Ben's plan.** If FB-1 goes another way, `pool_renewal` is two JSON numbers (a), (b) and (d) or
   three cultures' recruit rates (c) plus all the tests that read their pool; the farm test pins the finding so a later change of
   the recruit rates re-opens it.
2. **The re-anchor invariant.** The rate is read from the pressure at the anchor's day, so a bump that does not re-anchor first
   changes the pool backwards in time. `pool_bump` does it inside one function and `a_bump_integrates_the_gap_at_the_old_pressure`
   plus the closed-form property are the guard; any later code that sets `pool_at` by hand (tests do, `cultures.rs`) is fine at
   P 0.
3. **A gate missed.** A broken camp that sends one party is a camp that acts while it is scattered. The list of EQC210 is closed
   by `check()` and by the property test over random operations; a new party-making path added by CL must go through
   `send_party` or ask `camp_active`.
4. **A camp that never goes idle.** A Siege band that lingers or a demand that stays open defers the break, a day at a time;
   each recheck is one event and ends with the party. A camp whose parties are always out is not thin (its members count).
5. **Pressure thins the parties a camp can field** (a pressured camp's muster takes `min(want, pool)` and none below half): a
   player who has just wiped a camp sees smaller, later raids from it. That is the intent, and CL's council reads the same pool.
6. **The BAND_CASUALTIES credit** now matters. Java fills it from the full kill credit (R4); a regression there (a pet's kill
   read as a mob's) would under-count the pressure silently. `aPetsKillOfAPartyMemberIsThePlayersLoss` pins it.
7. **The second landing of CL or S7 takes the page letter** (EQC226 (d)): a merge of two optional tails on the faction row, by
   hand, and a fixture regeneration.
8. **The raid ladder now runs in real worlds** (Ben's FB-5 (b)). The first thing a survival player does at an outpost, killing its
   pillagers, is grievance at 12 each (FB-6): two kills bring an envoy, four a raid, and a pillager farm at an outpost keeps the
   ladder climbing to invasions and sieges. The camp's pressure and break (EQC203 to EQC212) are what answer a player who wins
   those fights. If the first worlds find the ladder too eager, the knobs are the `kin` margin and the raider thresholds
   (`raider.json`), not code.
9. **One village can bear every plunder** (the nearest hated node, EQC249): FB-8 (b)'s 48 days is the guard until CL replaces the
   clock; a village next to several explored outposts is still the target of each.
10. **A found camp is only as found as its chunk.** After a server restart an outpost's Java `Camp` (its `kin` and sound) is back when
    its chunk next loads; its engine faction and territory are back at once. A notice for a camp whose chunk is unloaded plays with the
    default horn, and a kill inside its box needs its chunk, so the order is safe (EQC246).
11. **Key collisions.** The camp key is `Obs.villageId` of the start chunk; the vanilla exclusion zone keeps a village start away,
    and the dens and groves already share the space; a modded village structure inside 10 chunks of an outpost is the case the
    GameTest cannot cover and `add_faction` would join as the village's node.
12. **The witch.** The `raiders` tag includes the witch; the predicate removes it (a swamp hut is not an outpost's member).
13. **The model is a model.** Its camps recruit 8% a day, have no parties, no districts and no demands; the engine's bands come
   from the S7 checks and go in EQC270 onward.

### UNVERIFIED, for the build to check first

1. That `Pressure::now` at the anchor's whole day gives the same line `pool_ready` inverts across a day boundary (the rate changes
   at midnight only when a bump or the decay of `p` crosses a day: P is read at `pool_at`'s day and held until the next event;
   EQC203's property test is the check).
2. That every party-making path goes through `send_party` or `plan_feud` or `plunder` (EQC210's list is from grep at 9389b4e5:
   `send_party`, `plunder`, the feud steps, the lurk and the ambush, `Ev::Ambush`'s train legs). The invariant test is what closes it.
3. That an `Ev::Feud` already on the wheel at a break is voided by bumping the feud's stamp and nothing else holds a copy (a
   `Party`'s `plan` is separate and no party exists at a break).
4. Which existing tests kill a pool camp's party at a district and then read the pool over a long gap (EQC232), and whether any
   asserts the old unslowed line.
5. That `FoundRow.wait_now_ticks()` and `Pressure::handed_over` behave for a pool camp as for a body camp (they are the same
   code, `camp_restore`), including a reset with `elapsed` longer than the wait: the camp is refounded at once.
6. That `synth::many` camps stay quiet enough to measure (their ladder raids villages and loses parties to them, cause
   `Faction`, no pressure): the control is the same seed with no kill, and the Roamer's and Farmer's kills are the only difference.
7. The Illager Invasion fort id in `raider.json` is not checked (the mod is not in the pack; the reporter skips an id the registry does
   not have), and the `mic_rtsim:conflict/*` ids (EQUILIBRIUM.md §2.4.5: "unverified") are test stand-ins and no reporter uses them.
8. That no outpost template carries a pillager: counted from the NBT of the eleven templates by string (jigsaw block names excluded).
   The first reporter step parses the template entity lists, or a GameTest generates an outpost and counts pillagers in its box right
   after generation at difficulty Normal, before any spawn cycle.
9. That `ChunkEvent.Load` gives `LevelChunk.getAllStarts()` the start for a jigsaw outpost as it does for Mowzie's pieces, and that
   `isNewChunk` is true for a chunk generated this session and false for one read from disk (Mowzie's `late` relies on it).
10. That the raider culture's makeups materialize with the stand-in bodies in the real arena (the goblin precedent used the same
    stand-ins) and that `Vindicator` and `Pillager` puppets do not join a vanilla `Raid` or become patrol leaders on their own.
11. That a game day of unthrottled GameTest ticks with one outpost in the arena fits the ladder test's timeout (EQC253).

### S7 work split

Each step is one commit on `feat/eq-s7-conflict` (commit early: an uncommitted agent worktree is removed after an interruption),
builds and tests under `nice -n 10`, and stops with the tests of its row green and the whole conflict suite unchanged. Before any
Gradle or GameTest step, check the laptop is on AC (`cat /sys/class/power_supply/AC/online` prints 1), that `ps -eo args | grep -c
'[m]odFolders=mic_rtsim_mowziesmobs'` prints 0, and that no GameTest run holds the lock. Steps 1 to 7 are Rust and need no Minecraft.

1. **Culture data** (`culture.rs`, the three JSONs). `PoolRenewalDef`, `PoolRenewal`, `Culture::pool_renewal`, the validation.
   Tests: `pool_renewal_parses_and_validates`, `renewal_off_is_todays_conflict`.
2. **The strain and the rate** (`engine.rs`, `camp.rs`). `Strain`, `Faction.strain` made in `add_faction`, `recruit_rate` in
   `pool_at`, `pool_ready`, `whole_at`, `pool_bump`, `pressure_now`. Tests: the rate rows of EQC230.
3. **Losses** (`engine.rs`, `camp.rs`). `renewal_bump`'s second arm, the bump in `casualties()`, the district rule and
   `district_unclaimed_is_players`, `slay`. Tests: the loss rows, `the_body_cultures_book_what_they_booked` (green before and after: it pins what must not move).
4. **Break and the inert camp** (`engine.rs`). `strain_check`, `break_camp`, `Ev::Strain`, `camp_active` and its gates, the
   `check()` extension, the archive kinds. Tests: the break and inert rows, the property.
5. **Refound** (`engine.rs`). `refound`, `reband` (factored out of `add_faction`). Tests: the refound rows.
6. **Ledger, census and formats** (`engine.rs`, `camp.rs`, `persist.rs`). The pool camp's row, the found restore, the census, RTCE
   and meta 13, stats. Tests: the ledger, format and census rows; regenerate rtsim-view's conflict fixtures.
7. **The checks** (`rtsim-cli`, `rtsim-jni`). The `camps` scenario, `sim::equilibrium`'s harness, `kill_all_camps`,
   `farmed_camp_breaks_and_refounds`, the long run (run the 30-seed versions, record the bands in EQC270 onward, set the knobs if
   a band fails); the jni order 41 and the three protocol tests.
8. **Java core** (test only). `NativeCodec.CONFLICT_SLAY`, `RtsimTests.slayCamp`, the three GameTests, the two stale comments.
9. **Documents** (EQC228).
10. **The reporter** (core Java, EQC237 to EQC254). `conflict/Outposts` (the finder, the queue, `known`, the re-report at a session
    start and the switch-on edge, the `kin` predicate), its hooks in `MicRtsim` and `ConflictBridge.endOfTick`, the config keys,
    `raider.json`'s plunder mean. `javac` first. Rust: `the_raider_claims_the_outposts`, `the_raider_plunders_at_the_slower_mean`,
    failing first.
11. **The reporter's GameTests** (EQC252). Write each, run it red for the reason in its record, then green, one project at a time.
12. **Fixtures and rounds.** Refresh rtsim-view's fixtures from fresh `core` and `create -Psec1fx` saves as S6 did (keeping the
    `fixtures.rs` header history), merge `origin/main`, then the final checks: `cargo test --workspace`, clippy `-D warnings`,
    `:core:test`, and the rounds `core`, `create -Psec1fx`, `create -PwithMowzie`, `mowziesmobs`, `iceandfire` and `ribbits` from
    cleared worlds.
13. **Land.** Merge `origin/main`, check `git diff --name-only --diff-filter=U`, run the tests the branch changes (the whole
    conflict suite, `rtsim-jni`, `rtsim-cli`, then `:core` GameTests and `create -PwithMowzie` and `mowziesmobs`, which share the
    clock and the ledger), then the pull request and review as CLAUDE.md says.

## What the S7 build found (2026-10-30)

Built on `feat/eq-s7-conflict` in the steps of "S7 work split" (each a commit, the engine side failing-first). Ben's FB-6 to FB-9 had
not been answered when the reporter was built; it is built on the recommendations, each a one-line change (below). The numbers are
the engine's own, from `cargo test --release -p rtsim-cli --bins -- camp wanderer wipe_every --include-ignored` (nine tests, 4 s) and the
GameTest rounds recorded in the final report.

### S7 bands (EQC270)

These replace the provisional table of EQC233. Every row is a test that fails outside its band.

| Check | Engine | Band kept | Model |
|---|---|---|---|
| No player, 60 years (`no_player_no_pressure_for_60_years`) | 0 breaks, 0 refounds, 0 broken days, P 0 for all three cultures; the same warriors every day as with `pool_renewal` removed | exactly the pre-S7 world | the same |
| `kill_all_camps`, seed 1 (a full camp wiped on day 100, 148 killed) | half back in 30 days, 90% in 54; no break | 90% within 120, no break | 45 to 54 |
| `kill_all_camps_30_seeds` | half: median 28 days [5th 28, 95th 32]; 90%: **median 51 [48, 69]**, worst 83 (seed 5, end 0.987 of the control); 0 breaks in 30 | median at most 96, each within 120 | 48 |
| `farmed_camp_breaks_and_refounds` (a farmer kills every whole warrior every second day, 60 years) | first break goblin day 218, bandit 162, raider 144; **0.233, 0.283 and 0.300 breaks a year**; P at the break 2.00 to 2.01; the wait 192.0 to 193.0 days (`min(384, 48 x 2^P)`); refound pools 15, 10 and 12 (a quarter of 60, 40, 50); broken **0.547** of the time | first break day 100 to 300, 0.15 to 0.40 a year, broken 0.35 to 0.70 | 150 to 260, 0.23 to 0.30, 0.47 to 0.60 |
| The same farmer at the plan's half-life of 96 (`farmed_camps_do_not_break_at_a_half_life_of_96`) | **no break in 60 years**; max P 1.58, 1.67, 1.72 | the finding that decided FB-1 | 1.58, 1.67, 1.72 |
| A wanderer, a wipe of what is home in one camp in three every 800 days (`a_wanderer_leaves_the_camps_near_their_caps`) | 0.971 of the control's warriors over years 30 to 60; max P 1.45 to 1.61; **no break** | at least 0.90, broken at most 5% | 0.97 |
| The wanderer every 400 days | **0.806**; 5 breaks in 60 years (raider 4, bandit 1); broken 0.062 of camp-days | at least 0.75, broken at most 10% | not run |
| The wanderer every 180 days (`a_wipe_every_half_year_breaks_the_camps_in_the_end`) | 0.526; 21 breaks; broken 0.255 | breaks (share above 5%), below 0.9 | not run |

What this says to Ben: a hunter who comes back to a camp once in about two game years never breaks it; one who comes every 400 days
breaks one now and then; one who comes every half year (a farmer) breaks them and finds each camp empty about a quarter of the time.
EQC233's roamer band (0.9 and 5%) holds at 800 days, not at 400: the provisional band assumed the model's roamer, which visits camps
it has not wiped lately, and a wanderer who wipes what is home takes P to 1.0 per visit, so two visits inside about a half-life
(192 days) put P over 2. The knob is `pool_renewal.break_pressure` (a number in the three JSONs).

### S7 deviations from the design (EQC271)

| # | What | Why |
|---|---|---|
| EQC271 | **`strain_check` runs at the end of `party_lost` too** (EQC208 named `pool_bump`, `home()`, the event and the found restore). A member killed in a party leaves `away` stale at `pool_bump`'s check, so a camp whose last warriors died on the march would not break until the next day's recheck; the break test with parties out found it. | `a_camp_with_a_party_out_waits_and_breaks_when_it_ends`. |
| EQC272 | **Three new stats, not four.** `breaks`, `refounds` and `break_deferrals` (EQC217 added `slain`). `slay` returns how many fell and keeps no counter; a counter of a test hook would be page bytes for nothing. | |
| EQC273 | **The order's pressure is added by `pool_press`, not by kills.** Order 41 kills `n` warriors as a player's (each adding `1/K`) and then adds `pressure` as a number, so a GameTest sets P exactly (P 2.0 for a break) without 100 kills. `pool_press` is a pub method of the engine that adds a number of pressure the way a loss does (re-anchoring the pool first, then checking for a break). | The tests need a precise P. |
| EQC274 | **`camp_found` for a camp with no bodies** restores through `strain_found`: the pressure by `Pressure::handed_over`, a `waiting()` row with count 0 as a broken camp whose `due = at + wait_now_ticks().max(1)` (a gap longer than the wait refounds it at once, one tick on), `Ev::Strain` booked and the plunder stamp bumped; any other row goes to `strain_check`. | EQC214, as designed. |
| EQC275 | **The ledger row for a broken camp has no new flag** (F11 stands): `SHORT | EMPTY | BROKEN | WAITING`, count 0, away 0. `a_row_with_count_and_waiting_is_not_broken` pins the one ambiguity (a row with a count is never a break). | |
| EQC276 | **`rail.rs`'s row-size assertion grew by one pointer (8 bytes)**: the only existing test that changed. | `Faction.strain` is an `Option<Box<Strain>>`. |
| EQC277 | **`synth::camps` exists** (a quiet world of camps and villages with no player), instead of finding out whether `synth::many` is quiet enough (UNVERIFIED 6): the design said its ladder raids villages and loses parties to them with no pressure, which would make every long run need its own control; a world built for the purpose does not. | Every S7 long run uses `synth::camps` and `camp_run`'s paired control (`Presser::None`). |
| EQC278 | **The archive kinds are appended at the end of `kinds!`**, after the other crates' kinds, because a kind's saved id is its position. | |
| EQC279 | **The stale kill-credit comments** in `ConflictBridge.onDeath` and `Camps` (they said a pet's kill was no loss; R4 made it one) are corrected; no code changed. | |

### UNVERIFIED, settled (EQC280)

| # | Result | Where |
|---|---|---|
| 1 | The line `pool_ready` inverts is the line `pool_at` runs: both read P at the anchor's day through `recruit_rate`; the engine agrees within a last bit of the float (the test's "two ticks before" check allows 1e-9) | `pool_ready_agrees_with_pool_at`, `a_bump_integrates_the_gap_at_the_old_pressure` |
| 2 | The gates are in `plan_feud`, `send_party`, `plunder`, `schedule_plunder` and the plunder target filter. `Conflict::check` now also verifies (`check_strains`) that a broken faction has pool 0, nothing away, no chief out, no party, no live feud wake and no live plunder wake, and that a faction has a strain exactly when its culture has a rule; the property test over random histories holds | `no_broken_camp_ever_acts`, `a_broken_camp_sends_nothing` |
| 3 | An `Ev::Feud` on the wheel at a break is voided by bumping the feud's stamp and nothing else holds a copy | `a_broken_camp_sends_nothing`, `a_refounded_camp_plans_its_plunder_and_its_feuds` |
| 4 | No existing test read a pool camp's pool over a long gap in a way the slower line changed: with P 0 the rate is bit-for-bit the old one (`a_culture_with_no_strain_recruits_at_its_rate`, the f64 path), and the whole conflict suite passed unchanged but for `rail.rs` (EQC276) | the suites |
| 5 | `FoundRow.wait_now_ticks()` and `Pressure::handed_over` behave for a pool camp as for a body camp, including a gap longer than the wait (the camp refounds at once) | `a_reset_resumes_a_pool_camps_break`, `a_camp_found_with_no_row_is_full_and_unpressured`; GameTests `aPoolCampKeepsItsPressureAndItsBreakAcrossAReset`, `aFarmedOutpostBreaksAndTheNewBandRefounds` |
| 6 | Wrong: `synth::many`'s camps are not quiet (EQC277) | |
| 7 | The `illagerinvasion:illager_fort` id is still unchecked (the mod is not in the pack); the reporter reads the registry and skips an id it lacks | |
| 8 | Not tested: a GameTest that places the generated pieces and counts pillagers was not written. The reporter does not depend on it: it neither counts nor adopts vanilla's pillagers either way (EQC239). `aVanillaPatrolAndTheOutpostsOwnPillagersAreLeftAlone` pins that two pillagers standing in a found outpost's box stay untagged and unremoved | |
| 9 | `ChunkEvent.Load` gives `getAllStarts()` the start of a generated jigsaw outpost (the test generates one with `Structure.generate`, sets it as `ChunkGenerator.tryGenerateStructure` does, and posts the load as the game does); `isNewChunk` false gives FACTION_FOUND flag LATE (aux 1), true gives a plain one | `anOutpostIsFoundOnceWhenItsChunkLoads`, `existingWorldsOutpostsJoinLate` |
| 10 | The raider culture's makeups materialize with the stand-in bodies in the real arena: the envoy (a herald and two pillagers) came after 36 asks of 1,200 ticks and was placed as `Pillager`s. A `Raid` object was not looked for | `aProvokedOutpostSendsItsEnvoy` |
| 11 | The ladder test fits: 36 asks of 1,200 ticks (about 1.8 game days) in under twenty seconds of real time at unthrottled ticks | |

### S7 reporter findings (EQC281)

| # | What |
|---|---|
| EQC281 | **A reinstall's clock jump ends a test's logic waits** (`SiteTestHooks.gapRestart` moves the game time on by the gap, and a `ChunkWait` logic wait counts from the last chunk wait): `aFarmedOutpostBreaksAndTheNewBandRefounds` failed at once with "not within 8000 ticks of the last chunk wait" until it took `SiteLedgerGameTests.clockMoved`'s steps (wait a tick, then a chunk wait on a forced chunk). The reporter itself was right: the outposts re-report to the new session by `Outposts.tick`, with no help. |
| EQC282 | **The raiders' territory radius is 300 blocks** (pollution 500, reach +200), not 400: my first test had it wrong, the culture file and EQC243 had it right. |
| EQC283 | **FB-6 to FB-9, as built (Ben answered on 2026-10-30: FB-6 (a), FB-7 (a), FB-9 (a), and FB-8 as the plunder mood, EQC285 to EQC290):** FB-6 (a) `kin` is a `minecraft:raiders` mob but the witch inside the box widened by `outpostKinMargin` 24 (a config key; 0 means the box itself); FB-7 (a) the outposts' pillagers are vanilla's; FB-8 (b) `raider.json`'s `plunder_mean_days` 48 (one number); FB-9 (a) `outpostsJoinExisting` true. |
| EQC284 | **Test overrides.** `RtsimConfig.Test.setOutpostsJoin` and `setOutpostsJoinExisting` (GameTest runs only, the property `mic_rtsim.gametest`), read before the config. |

### The plunder mood (Ben's FB-8 answer, 2026-10-30)

Ben, on how often raiders raid villages: *"Should probably depend on the raiders, w/ some RNG and dependence upon historical data for
the raiders comprising the camp, their experiences, temperaments, etc. Though ~48 days is a good avg."* So the wait to a camp's next
plunder is not one number. The records below say what exists, what was added, why, and what CONFLICT-LIFE.md's council takes over.
Built in the engine (`crates/rtsim-conflict/src/engine/mood.rs`, the `plunder_mood` block of `raider.json`), tested in
`crates/rtsim-conflict/tests/mood.rs` (twelve tests); nothing on the wire or in Java changes.

| # | Choice | Why |
|---|---|---|
| EQC285 | **What already models a band's mind (read before adding anything).** (1) *Temperament*: `Temperament::from_seed(chief_seed)` (bold, sociable, aggressive; `rtsim-knowledge`), recomputed from the seed, costing no bytes; the plunder clock already read it: the mean was `plunder_mean_days x 1000 / bend(Aggressive, 1000, 50)`, 0.67 to 2.0 times the mean, and a new band (`reband`, the refound) draws a new chief and so a new temperament, as does the heir who succeeds a fallen or aged chief. (2) *Fights seen*: `Faction.observed` (strength and surviving share of the camp's last 16 watched fights), read by `village_outcome` as a floor under what a raid costs the attackers; it holds watched fights only, is cleared at a refound, and nothing reads it for timing. (3) *Grievance and memory*: per feud (`Grievance`, the news the knowledge layer keeps, a chief's and heir's reports); they point at players and villages, not at the camp's own raids, and they decay by their own half-lives. (4) *Pressure*: `Strain.pressure`, a player's losses over the recruiting rate and the break (EQC200). (5) *CONFLICT-LIFE.md* plans a council (chief, heir and elder, each with a temperament drawn from the culture's mean, pride, grievance, memory slots with `lost at`, `raided us` and `paid` kinds, §2.1 to §2.4) that decides when a camp goes: not built. The camp's past raids and their outcomes were remembered by nothing. | Ben asked for reuse; (1) is reused as it is, (5) is what the new part is shaped for, and the one thing missing is (2)'s unwatched counterpart as a memory: how the camp's own raids went. |
| EQC286 | **The mean, and the data.** `mean(f, at) = plunder_mean_days x (1000 / bend(Aggressive, 1000, 50)) / temper_mean x factor(f, at)`, in game days, where `temper_mean` (about 1.10, computed once from the 256 aggressions when the culture resolves) centres the temperament so that the camps' mean is the culture's own 48 days, and `factor = clamp(exp(loss_gain x grief - spoils_gain x spoils), min_factor, max_factor)`. The block `plunder_mood` in `raider.json`: `half_life_days` 96 (a game year, the plan's pressure half-life), `loss_gain` 2.8, `spoils_gain` 0.15, `chief_loss_share` 0.1, `min_factor` 0.5, `max_factor` 3.0. The draw is the exponential one it always was, from `SYS_PLUNDER`, the faction key and the draw's number: the mood moves the mean, never the stream, so A/B runs and replays hold. A culture with no block keeps `mean x 1000 / bend` exactly (the goblin, bandit and grove clocks are bit for bit as before). | Ben: ~48 days average, some RNG, temperament, history. The gains are set so that a typical camp's losses and loot balance (EQC288). |
| EQC287 | **The history: two memories of the kernel's `Pressure` shape, saved in the faction row.** `Faction.mood: Option<Box<Mood>>`, `Some` only for a culture with the block; `Mood { grief, spoils }`, each an f32 and the day it was last bumped, halving in `half_life_days` (the kernel's closed form, no per-tick work). **Grief** takes `lost / cap` for every unit of a war party lost to anyone (village defences, a rival camp, a player, a trap: `party_lost`, the one function every loss goes through, once per counted casualty op) and `chief_loss_share` for a war chief's fall (`chief_falls`). **Spoils** takes 1 for each plunder that brought loot home (`home`, items landed at the haven). A refound clears both (`reband`): a new band has not raided. The mood is not in the site ledger's row, so a reset (no pages) gives a camp a neutral history, as it gives a feud none; the pages and the journal keep it across a crash and a restart. Saved at the end of the faction row after the strain (`put_opt`: a tag byte, and 16 bytes when present): the pages stay `RTCE`, meta 13 (this branch's format, not yet released, EQC217). `Conflict::check` pins that a faction has a mood exactly when its culture has the block. | The coordinator's sketch, reduced to the two numbers that answer it: bold or successful goes sooner (spoils, the chief's aggression), a band that lost members or its chief waits longer (grief), the player's kills of a party included because they are losses like any other. `observed` is not reused: it holds watched fights only, 16 of them, and a pair is not a decayed sum. |
| EQC288 | **Measured (the engine's own; all in `tests/mood.rs`).** (1) With no history, 3,200 camps on 16 seeds: the mean of `plunder_mean_days` is 48.0 (the test's band 47.5 to 48.5), the lowest about 29 days and the highest about 87 (0.6 to 1.8 of 48). (2) A quiet world (24 camps, no village with anything to take, 960 game days, 6 seeds): the camps draw a plunder every **48.1** days (band 45 to 51), 56 raids, grief 0.001. (3) A rich world (every village refilled to 20 items over its targets every ten days, the same days and seeds): a plunder every **47.1** days (band 42 to 54), 2,934 raids and 7,572 members lost, mean grief 0.146 (a seventh of the cap lost lately) and spoils 2.95, the camps' means ending between 17.6 and 129.2 days: the history spreads the camps well past the temperament's 29 to 87. At the first gains tried (`loss_gain` 3.5) the rich world's mean was 53.9 days: a typical camp's grief outweighed its spoils; 2.8 balances them. (4) One raid with loot lowers the next mean by 14%; losing 4 of 50 raises it by 25%; a chief's fall by 32%; both halve in 96 days (a quarter in two game years, nothing in the test's 2,000 days); the bounds hold at 100 times the loss and 100 times the loot (`the_factor_is_clamped`). | The coordinator asked for a test over many camps and seeds with the band. |
| EQC289 | **How CONFLICT-LIFE.md takes it over.** The plunder clock reads one function, `Conflict::plunder_mean_days(f, at)`; the council replaces its body with a sitting (its voices' temperaments in place of the chief's aggression, their memories in place of `Mood`: `lost at` is grief, `raided them` and `paid` are spoils) and keeps the exponential draw and `schedule_plunder`. `Mood` and its three writers (`grieve_members`, `grieve_chief`, `gloat`) are the only places that know a camp's raiding history; deleting them removes the mood and nothing else reads them. The block's name in the JSON would go with them. | Ben: "shape it so the council can take it over". |
| EQC290 | **Not done on purpose.** (1) The mood is not on a wire record, the ledger or the viewer: no player sees a temperament or a score (the raids come sooner or later, that is all). (2) `observed` and the pool's pressure are not folded in (a player's kills reach grief through the parties they kill). (3) The goblin, bandit and grove clocks are unchanged (FB-10). (4) A reset forgets the history (EQC287). (5) A pillager farm at an outpost: its kills of vanilla spawns are grievance (FB-6), not grief (they are not the camp's warriors). | Each can be added; none is needed for the answer Ben gave. |

## S11: the long-horizon suite (designed 2026-10-30)

**Designed 2026-10-30, not built.** EQUILIBRIUM.md §4's S11 row: `long_horizon_stability_with_kills` (every module, 60 game
years, the roamer script, every kind inside its §3.6 band), `kill_all_<kind>` for every kind in one place, and
`who_is_alive_soak` (262,980 game days in the host at max: every kind's census flat by century, state bytes flat, renewal
events per day flat; nightly). Branch `design/s11`, cut from origin/main a4717c1f, designed against main plus S9
(`feat/eq-s9-mowzie` at 795748fc, itself based on a4717c1f; S9's tests and helpers are named where the suite calls them). The
review-fix branches `fix/s5-review`, `fix/s6-review`, `fix/s7-review`, `fix/m11-review` and `fix/m12a-review` are in flight;
the work split says what waits for them. Ids EQC400 to EQC499 are S11's: EQC400 to EQC469 are the design, EQC470 to EQC499
are kept for the build's own findings. Choices that are Ben's are marked **FOR BEN**, each with its options and a
recommendation, and are gathered after the records. Nothing is built and no code changes here. Five light probes were run
for this design (EQC453): two host runs, the always-run equilibrium tests timed once, and one run of the research model in a
scratch copy outside the repository.

**Words used.** A *kind* is one census row (`dragon.lair`, `herd.range`, and so on). The *every-kind world* is the world
S11 builds in the shipped engine with sites of every kind on one map (EQC402). A kind's check is *alone* when it runs in its
own harness world (the existing kill-alls) and *together* when it runs in the every-kind world. A *judged century* is one of
centuries 2 to 27 of the soak, 9,600 game days each. A series' *drift* is its Theil-Sen slope times the judged span; it is
*material* when it exceeds the series' threshold δ and *significant* when a Mann-Kendall test gives p < 0.001 (EQC438). The
*calibration seeds* are the eight soak seeds the build runs once to set and check every threshold (EQC442). A *red night* is
a nightly soak whose verdict is FAIL.

### The shape

1. **Every module runs together only in the shipped engine.** `rtsim-cli`'s harnesses each build their own world (only
   herds and dragons share one); the jni `Engine` is the one place every module shares one clock (`shared::run_rounds`). The
   host soak runs it, but on a world with one lair, one haven slot, one goblin camp, nine human villages and no herds, Ribbit
   villages, nests, dens, groves or outposts: three kinds have no site at all and the rest have one each (the probe's census,
   EQC453). S11 adds one world builder, `rtsim_host::every`, that founds every kind on one map through the fake Java's own
   records, and it is shared by the three tests (EQC401 to EQC404).
2. **Kills reach the engine as a player's** through real records where the fake Java already has them (villagers, Ribbits,
   nest worms, pirate ships, dens, groves and the Umvuthi through the fake conflict plugin) and through TEST-mode orders where
   it has none: 41 `CONFLICT_SLAY` (pool camps and outposts, exists), and two new ones, 43 `DRAGON_SLAY` and 44 `HERD_CULL`
   (EQC405, EQC406).
3. **The roamer reaches every kind.** Today's roamer kills dragons only, and the model's never touches herds, villagers or
   Ribbits. S11's roamer walks as the model's and kills what it meets of every kind; each kind's booked player losses must equal
   its kills, so "with kills" has teeth (EQC410 to EQC413).
4. **Bands.** Every kind gets a band for each test, from (in order) the engine's own measured harness band, the model where it
   counts the kind (including a model run made for this design for the Umvuthi and the base-6 den, EQC416), or the build's
   first 30-seed run with its reason recorded; never looser than the rules' own caps (EQC415 to EQC420).
5. **`long_horizon_stability_with_kills`**: the every-kind world, 60 game years, the roamer and nobody on the same 30 seeds,
   years 30 to 60 against the band table; a four-year seed-1 cut runs by default (EQC417, EQC463).
6. **`kill_all_<kind>` in one place**: one table, `rtsim-cli`'s `sim::killall`, with a row for every kind, its alone harness
   and its band; the nest and pirate harnesses move from their crates' `tests/` into their `synth` modules so the table can call
   them; two rows are new (the Umvuthi, outposts); the same table runs together in the every-kind world on eight seeds
   (EQC425 to EQC430).
7. **`who_is_alive_soak`**: the every-kind world with nobody, 262,980 game days in the host at max, the census every 10 days,
   bytes, heap and queues at each century's end. It passes only if every kind is present and alive on every sample, every
   judged century sits in its kind's band, and no series has a drift that is both material and significant (EQC433 to EQC447).
8. **The engine's speed** (measured tonight on today's host world): 50.3 game days a second in the soak test under a load of
   about 55, 82.3 through `rtsim-cli run` under about 40, and 112 recorded under about 10 (VIEWER.md §12.4). So 262,980 days
   take 87, 53 and 39 minutes. The every-kind world costs more per day; derived, about 18 to 21 game days a second under
   tonight's load and roughly twice that on a quiet laptop, so plan for 2 to 4 hours, to be measured first (EQC453).
9. **Where it runs**: there is no CI. A script, `tools/nightly/soak.sh`, builds under the cargo heavy lock and runs the soak
   at nice 19 outside it, writing a local report; whether a timer runs it each night, and how a red night reaches a session,
   are **FOR BEN** S11-1 and S11-2 (EQC450 to EQC452).
10. **The census** gains two rows, `conflict.umvuthi` and `ship.port`; outposts stay inside `conflict.camp` (EQC457 to EQC459).
11. **The S11 row** was checked against the code and the probes and is unchanged (EQC460).

### S11 base and reads

Read for this design: EQUILIBRIUM.md (§1.1 and §1.2's unattended walkthrough, §2.1, §2.2, §2.3, §2.4.1 to §2.4.8, §2.8, §3.1,
§3.3, §3.6, §3.8, §4's table and its rule on long-horizon tests), and S9's diff of it; this file's S1, S2, S4, S5, S6 and S7
sections with the S7 build's findings; `docs/choices-mowzie-build.md` on S9 (MZB-1219, MZB-1259, MZB-1262, MZB-1266, MZB-1267,
MZB-1281); `rtsim-cli`'s `sim/equilibrium.rs` and `sim/census.rs` at main and at S9; `crates/rtsim-host` (`tests/host.rs`,
`tests/tap.rs`, `src/run.rs`, `src/java.rs`, `src/save.rs`); `crates/rtsim-jni/src/engine.rs` (`census`) and
`engine/shared.rs` (`run_rounds`), `orders.rs`; `crates/rtsim-kernel/src/census.rs`; the census of every engine
(`rtsim-world/src/world.rs`, `rtsim-ecology/src/engine.rs` and `herds/engine.rs`, `rtsim-ships/src/engine.rs`,
`rtsim-conflict/src/engine.rs` at S9); `crates/rtsim-ships/tests/equilibrium.rs`; `crates/rtsim-ecology/tests/nests.rs`,
`home_bodies_long.rs`, `lair_renewal.rs`; S9's `crates/rtsim-conflict/src/synth.rs` (`FakeConflict`, `CampRun`,
`mowzie_world_on`); VIEWER.md §12.4; `docs/choices-leg-purge.md`; `README.md`'s commands, `tools/ci/*.sh`,
`neoforge/gametest.sh`, `flake.nix`; `research/equilibrium-2026-10-21/` (`model.py`'s `Player`, `run.py`'s `steady`,
`results/results.json`). Outside the repository: the laptop's user timers and `pack/tools/gc/README.md`, and GitHub's API
for the repository. Three sub-agents read for the catalogue of tests, the host and engine composition, and the bands; every
fact below was checked in the code or the docs named.

**What is true in the repo** (main a4717c1f unless S9 795748fc is named; `EQ` is `crates/rtsim-cli/src/sim/equilibrium.rs`):

| # | Fact | Where |
|---|---|---|
| R1 | `rtsim-cli` is a binary crate only, so its `sim::equilibrium` tests are the binary's unit tests: no other crate can call them, and rtsim-cli cannot call anything in another crate's `tests/`. | `crates/rtsim-cli/Cargo.toml:11-13` |
| R2 | `sim::equilibrium` is five harnesses on five worlds: `wipe_run` and `dragon_run` on `census::Ecology` (herds and dragons on one world, the herds lent to the dragons' loop), `village_wipe` on `census::Villages`, `camp_run` on `rtsim_conflict::synth::camps`, and S9's `mowzie_run` (one camp on `CampRun`, the emulated plugin) and `unattended_grove` (one grove among six villages). | EQ `:182`, `:425`, `:653`, `:760`; S9 EQ `:986`, `:1077`; `census.rs:406-504` |
| R3 | There is one `Roamer`, and it kills dragons only ("on the dragons alone (the other kinds' slices bring theirs)"). It starts at region (0, 0), steps every second day to one of the nine regions around it within `min(6, 2 + day / 48)` regions of spawn, and each day with chance 0.5 slays every occupant of the lairs of its region. The other scripted players belong to one harness each. | EQ `:326-378`; `Presser` `:713`; S9 `MowziePresser` `:902`; `census.rs:303`; `rtsim-ships/src/synth.rs:907`; `rtsim-ships/tests/equilibrium.rs:122`; `rtsim-ecology/tests/nests.rs:216` |
| R4 | The model's roamer kills, each day in its region with these chances, the dragons (0.5), dens (0.5), groves and their Umvuthi (0.3), nests (0.5), camps (0.2) and ships (0.3). It never kills herds, villagers or Ribbits. The model counts the Umvuthi and prints no ratio for it. | `research/equilibrium-2026-10-21/model.py:753-762`, `:682`; `run.py:25-40` |
| R5 | `sim census` has seven scenarios, each on its own world; only `ecology` and `ranges` run two engines together. | `census.rs:54`, `:135`; S9 adds `mowzie` |
| R6 | `Engine::census` joins villages, dragons and nests, herds, ships and conflict. `shared::run_rounds` takes the earliest `next_due` over dragons, ships, herds and conflict and runs each to it: the one place every module shares one clock. | `crates/rtsim-jni/src/engine.rs:1016-1033`; `engine/shared.rs:555-586` |
| R7 | The host soak's world: nine human villages, six sea ports, one haven (one slot), one fire lair with its body, one goblin camp; a player in the first village for a day, then nobody. The soak runs at `Speed::Max` with real fsyncs, a census and the saved page bytes every 10 game days. `a_thousand_days_at_max_keeps_the_state_bounded` asserts pages late at most twice early plus 64 KiB, heap late at most twice early plus 16 MiB, and the same kind names on every sample; no count is checked. | `crates/rtsim-host/tests/host.rs:65-146`, `:209-224`, `:1547-1625`, `:1689-1719` |
| R8 | The fake Java plays two plugins, `FakeHerds` and `FakeConflict`, and only records dragon and ship commands. Its records: `adopt`, `village_census`, `resident_killed`, `habitat`, `herd_obs`, `dragon_obs`, `ship_obs`, `conflict_obs`, `conflict_flush`. | `crates/rtsim-host/src/java.rs:220`, `:226`, `:232`, `:239`, `:1623`, `:1733`, `:1841`, `:1879`, `:1921`, `:1955`, `:2018`, `:2051`, `:2143` |
| R9 | The TEST-mode orders are 1 to 9, 16 to 32, 34, 40, 41 `CONFLICT_SLAY` (a player kills a pool camp's warriors) and S9's 42 `CONFLICT_SEND`. No order kills a dragon, a herd or a ship. | `crates/rtsim-jni/src/orders.rs:48-88` |
| R10 | Kill-alls: herds (world and local), dragons, villagers, Ribbits and pool camps in EQ, S9's dens and groves there too, each with an always-run cut and an `#[ignore]`d 30-seed twin; `kill_all_nests` (one seed, no twin) and `kill_all_pirates` (30 seeds, always run) in other crates' `tests/`. None for the Umvuthi, outposts or ports. | EQ `:1024`, `:1066`, `:1137`, `:1291`, `:1324`, `:1403`; S9 EQ `:1967`, `:2011`; `rtsim-ecology/tests/nests.rs:361`; `rtsim-ships/tests/equilibrium.rs:198` |
| R11 | Long runs: `long_horizon_dragons_and_herds_with_kills` (seed 1, 4 years; the twin 30 seeds by 60 years, the roamer and nobody); `bodies_do_not_move_the_s4_bands` (3 years; its 20-year 30-seed twin was never run); `pirates_survive_unattended_1000_years` (ten centuries: each century manned at least 0.83, hunts above 0, the save within the first's plus 10%); S9's `long_horizon_mowzie_with_kills` (dens and groves on separate worlds; halves within 0.08, renewals within 25%, the save within twice plus 4 KiB). | EQ `:1201`, `:1217`; `rtsim-ecology/tests/home_bodies_long.rs:143`, `:182`; this file EQC149's note at line 442; `rtsim-ships/tests/equilibrium.rs:555-605`; S9 EQ `:2200-2273` |
| R12 | S9's long run found the rich-village grove's save growing "about 7 bytes a game day ... not diagnosed". Over 262,980 days that is about 1.8 MB. | S9 EQ `:2252-2255` |
| R13 | The census row is `KindCensus` (sites, members, capacity, short, empty, broken, the summed P, cumulative renewals). The kinds are `village.human`, `village.ribbit`, `dragon.lair`, `oil.nest`, `herd.range`, `ship.haven` and `conflict.camp`, and at S9 `conflict.den` and `conflict.grove`. There is no row for the Umvuthi, outposts (they are raider camps inside `conflict.camp`) or ports. `conflict.camp`'s members are the pool only, not members away. | `crates/rtsim-kernel/src/census.rs:21-58`; `rtsim-world/src/world.rs:762-793` (S9); `rtsim-ecology/src/engine.rs:696-704`; `herds/engine.rs:661`; `rtsim-ships/src/engine.rs:995-1012`; S9 `rtsim-conflict/src/engine.rs:1377-1396` |
| R14 | A kind's renewals: human villages settlers and growth; Ribbits settlers, refills and houses; havens refits and refounds; at S9 pool camps refounds, dens arrivals, groves grove arrivals; lairs claims and hatchings; nests broods and worms renewed; herds calvings, foundings and recolonisations. | the same lines; EQC4, EQC20 |
| R15 | No CI: no `.github`, no workflow and no self-hosted runner (GitHub's API, 2026-10-30: the repository is private, 0 workflows, 0 runners); `flake.nix` has no apps; `tools/ci/determinism-check.sh` and `scale-check.sh` are run by hand under `/tmp/mic-rtsim-heavy.lock`, the lock `README.md`'s cargo commands take; GameTests take `/tmp/mic-rtsim-bridge-heavy.lock`. The laptop's one MIC timer is `pack-gc` (plain unit files in `~/.config/systemd/user`, 04:30, Nice 19, idle IO, written by its script's `install`; the durable home is Ben's Nix config). | `README.md:103-152`; `tools/ci/*.sh:7-9`; `neoforge/gametest.sh:53`; `pack/tools/gc/README.md:11-15` |
| R16 | The host at max: strides up to two game days give the same hashes as 20 ticks; the stride is 2,400 ticks and a save every 10 game days; the populated world ran 112 game days a second with fsyncs under about 10 runnable, and 108 to 113 and 172 in later runs. | `crates/rtsim-host/src/run.rs:33-40`; VIEWER.md:937-943; `docs/choices-leg-purge.md:44` |
| R17 | The fake conflict plugin answers a camp's load, census, spawns and kills (`camp_in`, `camp_kill`, `camp_out`), and S9's `mowzie_visit` drives a den's or a grove's player through it. | S9 `rtsim-conflict/src/synth.rs:1035-1124`, `:1419`; S9 EQ `:960-982` |
| R18 | A ship sunk by a player is the record `Obs::Sunk { by: Player, agent, epoch, .. }`; a nest's worms killed are `NestLoss` with cause Player. | `rtsim-ships/tests/equilibrium.rs:122-146`; `rtsim-ecology/src/synth.rs:1052` |
| R19 | The native engine's wells come from the industry module's districts: an oil deposit (`DieselOil` or `DestroyCrude`) of the district's current sweep, finite with some left or infinite. A nest wakes and renews only while drilled. | `rtsim-ecology/src/infra.rs:488-500`; EQUILIBRIUM.md §4's S10 row |
