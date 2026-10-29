# Overnight choices: EQUILIBRIUM.md slices S1, S2 and S4 (2026-10-22)

Reversible choices made while Ben was asleep, each with its reason, numbered EQC#. None is final
until Ben reviews it. The work is EQUILIBRIUM.md §4's slices S1 (the who-is-alive census), S2
(herds: density term, refuge, dispersal, frontier founding, tombstone removal) and S4 (dragons: lair
pressure, fed share, starve-leave, arrival gate, newcomer direction, oil visitors only into due
lairs). DECISIONS.md is unchanged; EQUILIBRIUM.md marks the slices built. Branches `feat/eq-s1-s2`
and `feat/eq-s4`. Slice S6 (villages and Ribbits) is a design only, on `feat/eq-s6-villages`, in the last
section.

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
| EQC183 | **Tunables** (every one TUNABLE, PLACEHOLDER until the S6 checks). World (`WorldConfig`): `renewal` true (false is today's world, for the "births as before" tests), `village_rule` 16, 192, 96, `village_growth_per_day` 0.01, `village_k_max` 32, `settlers_pair` 2, `village_loaded_ticks` 12,000, `owed_max_days` 192, `ribbit_refill` true, `ribbit_refill_needs_prosperity` true. Link: heartbeat 6,000 ticks, readings to lower 2. Java: `ARRIVAL_WATCH_BLOCKS` 128, `CENSUS_REACH` 48, walk spawn distances 176 down to 96, `WALK_STEP_TICKS` 40, hop 24 blocks, done within 12 blocks, `WALK_TIMEOUT_TICKS` 6,000, retry 40 ticks. | One list so the build and the tests agree. |

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
| Phase 1 stats indexes | 20 to 24 | 0 to 19 | 25 and up |
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

