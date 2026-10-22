# Overnight choices: EQUILIBRIUM.md slices S1 and S2 (2026-10-22)

Reversible choices made while Ben was asleep, each with its reason, numbered EQC#. None is final
until Ben reviews it. The work is EQUILIBRIUM.md §4's slices S1 (the who-is-alive census) and S2
(herds: density term, refuge, dispersal, frontier founding, tombstone removal). DECISIONS.md is
unchanged; EQUILIBRIUM.md marks the slices built. Branch `feat/eq-s1-s2`.

## S1: the census

| # | Choice | Why |
|---|---|---|
| EQC1 | (2026-10-22, slice S1) **One census type for every engine, `rtsim_kernel::census::KindCensus`, with one row per kind of site**, and each engine a `census()` read at its own now: the world's `village.human` and `village.ribbit`, the dragons' `dragon.lair` and `oil.nest`, the herds' `herd.range`, the ships' `ship.haven`, the conflict's `conflict.camp`. The native engine's `Engine::census` joins the modules its session runs, in that order. | The plan asks for "one line per kind ... for every module", and later slices measure themselves with it; one type next to `renewal` keeps the counting rule (which state a site is in) in one place. |
| EQC2 | **An `empty` column beside the plan's list** (sites, members, capacity, short, empty, broken, mean P, renewals). A site is broken, else empty (no members), else short (fewer than K), else full; full is `sites - short - empty - broken` and is not printed. | SITE_STATE (§2.2) has four states; without `empty` a wiped kind and a thinned kind read the same, and the kill-all tests of S2 to S11 count empty sites. |
| EQC3 | **Renewals are printed per line, as the count since the kind's previous line**, from each engine's cumulative counters (`CensusLog` keeps the last total). | The plan's "renewal events per day flat" is a rate; a running total would grow forever and never diff flat. |
| EQC4 | **What a renewal is, per kind:** a lair's claims plus hatchings; a nest's first broods plus worms renewed; a herd's yearly calvings (S2 adds foundings and recolonisations); a haven's refits; a ribbit village's houses booked. Human villages and camps book none yet (S6, S7). | Each is the event that adds a member to a site, the model's `*_events` (`research/equilibrium-2026-10-21/model.py`), from counters the engines already keep. |
| EQC5 | **K where the engine has none yet:** villages print capacity 0 and count full or empty (beds come with S6); a camp's K is its culture's pool cap and its members the whole warriors in its pool; a herd's K is its founding count until S2 stores one. Mean P reads 0 for kinds with no pressure yet (lairs until S4, camps until S7, herds and villages always). | The census reports what the engines hold; inventing a K for villages now would be a second, unreviewed rule. |
| EQC6 | **An oil nest whose cell was given up (no room for the chamber) is not a site.** A broken heart is. | A given-up cell never renews and holds nothing; counting it would show a broken site forever where §2.4.8 says there is none. |
| EQC7 | **`rtsim-cli sim census` runs three scenarios with nobody watching**, each with its world's villages: `ecology` (the phase 1 dragon world's lairs, oil under two nest cells with an endless well beside each, the herd valley's six herds hunted by the dragons), `ships` (the synthetic coast's haven), `conflict` (the goblin camp). Defaults: seed 7, 960 days, a line every 24 days (the model's census grid), plus day 0. Text only: a fixed-width header and one line per kind. | A nest wakes and renews only while drilled (S10), so an undrilled scenario would show empty nests forever. Tests read the Rust API (`sim::census::run`, `Scenario`), so the text has no consumer that needs JSON yet. |
| EQC8 | **The host soak samples `Engine::census` every 10 game days** and prints it on its table's days; `a_thousand_days_at_max_keeps_the_state_bounded` asserts every module's kinds are on every sample. The flatness bands wait for S11's `who_is_alive_soak`. | The populated host world has one lair, one haven, one camp and no herds; it shows the hook works end to end, and S11 is the slice that judges bands there. |

## What the census found (not edited)

- **Herds grow past their founding count with nobody watching** (seed 7, `sim census --scenario
  ecology`): 33,000 animals at day 0, 26,449 at day 96 (the founding pyramid's first deaths), then
  41,575 at day 480 and 49,780 at day 960, 1.5 times the founding count. This is the missing density
  term §3.2 describes; S2 adds it.
- **A camp's pool dips below its cap with nobody watching** (52 of 60 on day 480 in the conflict
  scenario, 53 of 60 on day 20 of the host soak): its parties go out and come back. `short` counts it.
