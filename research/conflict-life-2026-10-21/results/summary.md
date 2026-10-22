# Conflict-life model: results

64 seeds per scenario and mode, 50 game years each; run took 322.8 s. `ladder` is today's engine (EQ-5 (c)), `gate` is EQUILIBRIUM 2.5 (EQ-5 (a)), `life` is CONFLICT-LIFE.md.

## Fights per world-year, losses, stability

| Scenario | Mode | Fights a year: mean [p5, median, p95] | Years with none | Camp-on-camp a year | Lost share | Lost a year | Endless war | Surge | Extinct | Decade with no camp fight | No camp fight at all |
|---|---|---|---|---|---|---|---|---|---|---|---|
| alone | ladder | 29.9 [22, 30, 37] | 0.00 | 10.11 | 0.45 | 13.45 | 1.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| alone | gate | 11.5 [9, 12, 15] | 0.00 | 0.00 | 0.17 | 2.00 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| alone | life | 10.2 [5, 10, 17] | 0.00 | 2.26 | 0.18 | 1.83 | 0.06 | 0.00 | 0.00 | 0.00 | 0.00 |
| duel_adaptive | ladder | 0.0 [0, 0, 0] | 1.00 | 0.00 | n/a | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_adaptive | gate | 0.0 [0, 0, 0] | 1.00 | 0.00 | n/a | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_adaptive | life | 0.0 [0, 0, 0] | 0.97 | 0.00 | 0.14 | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_fortress | ladder | 9.0 [4, 10, 10] | 0.00 | 0.00 | 0.99 | 8.88 | 0.00 | 0.05 | 0.00 | 1.00 | 1.00 |
| duel_fortress | gate | 0.0 [0, 0, 0] | 1.00 | 0.00 | n/a | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_fortress | life | 0.1 [0, 0, 1] | 0.89 | 0.00 | 0.84 | 0.09 | 0.00 | 0.09 | 0.00 | 1.00 | 1.00 |
| duel_stubborn | ladder | 21.4 [19, 21, 23] | 0.00 | 0.00 | 0.22 | 4.75 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_stubborn | gate | 12.5 [10, 12, 15] | 0.00 | 0.00 | 0.22 | 2.74 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_stubborn | life | 3.2 [2, 3, 5] | 0.00 | 0.00 | 0.05 | 0.15 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_weak | ladder | 0.0 [0, 0, 0] | 1.00 | 0.00 | n/a | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_weak | gate | 0.0 [0, 0, 0] | 1.00 | 0.00 | n/a | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_weak | life | 0.1 [0, 0, 1] | 0.91 | 0.00 | 0.04 | 0.00 | 0.00 | 0.06 | 0.00 | 1.00 | 1.00 |
| farmer | ladder | 30.1 [22, 30, 38] | 0.00 | 10.09 | 0.45 | 13.52 | 1.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| farmer | gate | 11.7 [9, 12, 15] | 0.00 | 0.00 | 0.18 | 2.08 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| farmer | life | 10.9 [6, 10, 18] | 0.00 | 2.20 | 0.19 | 2.13 | 0.08 | 0.00 | 0.00 | 0.00 | 0.00 |
| fortress | ladder | 30.1 [23, 30, 37] | 0.00 | 7.26 | 0.44 | 13.33 | 1.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| fortress | gate | 11.6 [9, 12, 15] | 0.00 | 0.00 | 0.18 | 2.06 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| fortress | life | 9.7 [5, 9, 17] | 0.00 | 1.97 | 0.20 | 1.96 | 0.05 | 0.00 | 0.00 | 0.00 | 0.00 |
| multi | ladder | 33.4 [27, 33, 40] | 0.00 | 7.41 | 0.55 | 18.47 | 1.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| multi | gate | 16.2 [12, 16, 20] | 0.00 | 0.00 | 0.36 | 5.81 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| multi | life | 8.0 [4, 8, 13] | 0.00 | 1.60 | 0.20 | 1.59 | 0.06 | 0.00 | 0.00 | 0.00 | 0.00 |
| multi_nospill | ladder | 33.4 [27, 33, 40] | 0.00 | 7.41 | 0.55 | 18.47 | 1.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| multi_nospill | gate | 16.2 [12, 16, 20] | 0.00 | 0.00 | 0.36 | 5.81 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| multi_nospill | life | 8.0 [4, 8, 13] | 0.00 | 1.59 | 0.19 | 1.55 | 0.08 | 0.00 | 0.00 | 0.00 | 0.00 |
| railway | ladder | 30.0 [22, 30, 38] | 0.00 | 10.08 | 0.45 | 13.47 | 1.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| railway | gate | 33.6 [30, 34, 37] | 0.00 | 0.00 | 0.18 | 5.92 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| railway | life | 12.0 [7, 12, 19] | 0.00 | 1.91 | 0.20 | 2.36 | 0.05 | 0.00 | 0.00 | 0.00 | 0.00 |

## How formulaic

| Scenario | Mode | Interval CV median [p10, p90] | Pair CV median [p10, p90] | Next act called to the day | Culture spread (JS) | Spread of a camp's fights a year (CV over seeds) | Story overlap of two seeds |
|---|---|---|---|---|---|---|---|
| alone | ladder | 0.83 [0.77, 0.92] | 0.83 [0.77, 0.92] | 0.18 | 0.000 | 0.04 | 0.50 |
| alone | gate | 0.76 [0.68, 0.90] | 0.76 [0.68, 0.90] | 0.15 | 0.000 | 0.05 | 0.44 |
| alone | life | 0.88 [0.70, 1.06] | 1.04 [0.76, 1.72] | 0.02 | 0.017 | 0.15 | 0.07 |
| duel_adaptive | ladder | n/a [n/a, n/a] | n/a [n/a, n/a] | 1.00 | 0.000 | n/a | 1.00 |
| duel_adaptive | gate | n/a [n/a, n/a] | n/a [n/a, n/a] | 0.33 | 0.000 | n/a | 0.54 |
| duel_adaptive | life | 0.59 [0.44, 0.86] | 0.59 [0.44, 0.86] | 0.04 | 0.003 | n/a | 0.18 |
| duel_fortress | ladder | 0.27 [0.17, 0.38] | 0.27 [0.17, 0.38] | 0.97 | 0.017 | 0.07 | 0.33 |
| duel_fortress | gate | n/a [n/a, n/a] | n/a [n/a, n/a] | 1.00 | 0.000 | n/a | 1.00 |
| duel_fortress | life | 0.87 [0.62, 1.18] | 0.87 [0.62, 1.18] | 0.07 | 0.005 | n/a | 0.22 |
| duel_stubborn | ladder | 0.22 [0.20, 0.24] | 0.22 [0.20, 0.24] | 0.65 | 0.000 | 0.01 | 0.96 |
| duel_stubborn | gate | 0.34 [0.33, 0.35] | 0.34 [0.33, 0.35] | 0.11 | 0.000 | 0.01 | 0.64 |
| duel_stubborn | life | 0.56 [0.53, 0.60] | 0.56 [0.53, 0.60] | 0.01 | 0.020 | 0.06 | 0.16 |
| duel_weak | ladder | n/a [n/a, n/a] | n/a [n/a, n/a] | 1.00 | 0.000 | n/a | 1.00 |
| duel_weak | gate | n/a [n/a, n/a] | n/a [n/a, n/a] | 0.33 | 0.000 | n/a | 0.52 |
| duel_weak | life | 0.81 [0.45, 1.19] | 0.81 [0.45, 1.19] | 0.04 | 0.003 | n/a | 0.17 |
| farmer | ladder | 0.83 [0.77, 0.94] | 0.83 [0.77, 0.94] | 0.17 | 0.001 | 0.04 | 0.48 |
| farmer | gate | 0.78 [0.67, 1.07] | 0.78 [0.67, 1.07] | 0.14 | 0.003 | 0.05 | 0.42 |
| farmer | life | 0.89 [0.74, 1.08] | 1.01 [0.77, 1.70] | 0.02 | 0.011 | 0.16 | 0.07 |
| fortress | ladder | 0.79 [0.49, 0.90] | 0.79 [0.49, 0.90] | 0.16 | 0.000 | 0.03 | 0.47 |
| fortress | gate | 0.75 [0.67, 0.87] | 0.75 [0.67, 0.87] | 0.27 | 0.010 | 0.06 | 0.52 |
| fortress | life | 0.85 [0.70, 1.02] | 0.99 [0.72, 1.62] | 0.02 | 0.014 | 0.15 | 0.07 |
| multi | ladder | 0.67 [0.50, 0.81] | 0.83 [0.59, 1.31] | 0.16 | 0.001 | 0.03 | 0.44 |
| multi | gate | 0.74 [0.30, 0.96] | 0.74 [0.30, 1.00] | 0.09 | 0.002 | 0.07 | 0.38 |
| multi | life | 0.74 [0.58, 0.94] | 0.91 [0.61, 1.55] | 0.02 | 0.013 | 0.13 | 0.08 |
| multi_nospill | ladder | 0.67 [0.50, 0.81] | 0.83 [0.59, 1.31] | 0.16 | 0.001 | 0.03 | 0.44 |
| multi_nospill | gate | 0.74 [0.30, 0.96] | 0.74 [0.30, 1.00] | 0.09 | 0.002 | 0.07 | 0.38 |
| multi_nospill | life | 0.74 [0.58, 0.95] | 0.89 [0.62, 1.55] | 0.02 | 0.012 | 0.15 | 0.08 |
| railway | ladder | 0.83 [0.78, 0.91] | 0.83 [0.78, 0.91] | 0.18 | 0.000 | 0.03 | 0.51 |
| railway | gate | 0.70 [0.17, 0.88] | 0.70 [0.17, 0.88] | 0.52 | 0.000 | 0.04 | 0.78 |
| railway | life | 0.89 [0.67, 1.09] | 0.98 [0.69, 1.59] | 0.04 | 0.013 | 0.18 | 0.11 |

## Duels (game.py's setting: one goblin camp, one base, grievance 9 a day)

| Player | Mode | Wars a year, years 1-5 | Wars a year, last decade | Raiders killed a year | Tribute a year | Defence year 10 | Defence at the end |
|---|---|---|---|---|---|---|---|
| adaptive | ladder | 0.0 | 0.0 | 0 | 406 | 12 | 12 |
| adaptive | gate | 0.0 | 0.0 | 0 | 195 | 12 | 12 |
| adaptive | life | 0.1 | 0.0 | 0 | 228 | 12 | 12 |
| fortress | ladder | 6.0 | 9.6 | 65 | 0 | 80 | 80 |
| fortress | gate | 0.0 | 0.0 | 0 | 0 | 80 | 80 |
| fortress | life | 0.1 | 0.1 | 3 | 0 | 80 | 80 |
| stubborn | ladder | 21.0 | 21.3 | 60 | 0 | 6 | 6 |
| stubborn | gate | 12.2 | 12.5 | 35 | 0 | 6 | 6 |
| stubborn | life | 3.5 | 3.1 | 4 | 0 | 6 | 6 |
| weak | ladder | 0.0 | 0.0 | 0 | 406 | 6 | 6 |
| weak | gate | 0.0 | 0.0 | 0 | 362 | 6 | 6 |
| weak | life | 0.1 | 0.1 | 0 | 255 | 6 | 6 |

## What started the life-mode fights (share of fights)

A driver is any motive that was at least a quarter of the push toward the fight; a fight can have several. Loot only: greed for loot with no hunger, grudge, pride, glory or fear behind it.

| Scenario | Top motive | Drivers | Loot only | Occasion |
|---|---|---|---|---|
| alone | hunger 0.45, loot 0.33, glory 0.12, pride 0.09, preempt 0.00, revenge 0.00 | loot 0.51, hunger 0.49, glory 0.24, pride 0.11, revenge 0.01, preempt 0.01 | 0.19 | restless 0.80, caravan 0.16, provoked 0.04, news 0.01 |
| duel_adaptive | loot 0.68, hunger 0.24, revenge 0.05, glory 0.02 | loot 0.92, hunger 0.42, glory 0.35, revenge 0.13 | 0.12 | restless 1.00 |
| duel_fortress | hunger 0.96, loot 0.03, revenge 0.01 | hunger 0.96, loot 0.03, revenge 0.03 | 0.01 | restless 0.83, refused 0.17 |
| duel_stubborn | loot 0.74, revenge 0.12, pride 0.10, glory 0.03, hunger 0.00 | loot 0.94, revenge 0.53, pride 0.19, glory 0.13, hunger 0.00 | 0.27 | restless 0.79, refused 0.21 |
| duel_weak | loot 0.76, glory 0.12, hunger 0.09, revenge 0.02 | loot 0.91, glory 0.72, hunger 0.14, revenge 0.09 | 0.09 | restless 1.00 |
| farmer | hunger 0.40, loot 0.32, pride 0.17, glory 0.11, preempt 0.00, revenge 0.00 | loot 0.48, hunger 0.44, glory 0.22, pride 0.19, revenge 0.01, preempt 0.00 | 0.18 | restless 0.76, caravan 0.14, provoked 0.08, news 0.01, refused 0.00 |
| fortress | hunger 0.40, loot 0.37, glory 0.13, pride 0.10, preempt 0.00, revenge 0.00 | loot 0.56, hunger 0.44, glory 0.26, pride 0.11, revenge 0.01, preempt 0.01 | 0.21 | restless 0.78, caravan 0.16, provoked 0.03, refused 0.02, news 0.01 |
| multi | loot 0.41, hunger 0.26, glory 0.19, pride 0.13, revenge 0.00, preempt 0.00 | loot 0.59, glory 0.34, hunger 0.29, pride 0.15, revenge 0.02, preempt 0.00 | 0.24 | restless 0.77, caravan 0.15, provoked 0.04, refused 0.03, news 0.01 |
| multi_nospill | loot 0.41, hunger 0.27, glory 0.19, pride 0.13, revenge 0.00, preempt 0.00 | loot 0.59, glory 0.34, hunger 0.29, pride 0.15, revenge 0.02, preempt 0.00 | 0.24 | restless 0.78, caravan 0.15, provoked 0.04, refused 0.02, news 0.01 |
| railway | loot 0.54, hunger 0.28, glory 0.10, pride 0.07, revenge 0.02, preempt 0.00 | loot 0.71, hunger 0.31, glory 0.21, revenge 0.13, pride 0.09, preempt 0.00 | 0.32 | restless 0.56, train 0.29, caravan 0.11, provoked 0.02, refused 0.01, news 0.01 |

## Fights by target (a world-year; in brackets, the share the attacker lost)

| Scenario | Mode | Camps | Villages | Players | Trains and caravans |
|---|---|---|---|---|---|
| alone | ladder | 10.11 (0.98) | 19.80 (0.18) | 0.00 | 0.00 |
| alone | gate | 0.00 | 11.54 (0.17) | 0.00 | 0.00 |
| alone | life | 2.26 (0.52) | 6.31 (0.09) | 0.00 | 1.59 (0.05) |
| duel_adaptive | ladder | 0.00 | 0.00 | 0.00 | 0.00 |
| duel_adaptive | gate | 0.00 | 0.00 | 0.00 | 0.00 |
| duel_adaptive | life | 0.00 | 0.00 | 0.03 (0.14) | 0.00 |
| duel_fortress | ladder | 0.00 | 0.00 | 8.96 (0.99) | 0.00 |
| duel_fortress | gate | 0.00 | 0.00 | 0.00 | 0.00 |
| duel_fortress | life | 0.00 | 0.00 | 0.11 (0.84) | 0.00 |
| duel_stubborn | ladder | 0.00 | 0.00 | 21.38 (0.22) | 0.00 |
| duel_stubborn | gate | 0.00 | 0.00 | 12.45 (0.22) | 0.00 |
| duel_stubborn | life | 0.00 | 0.00 | 3.19 (0.05) | 0.00 |
| duel_weak | ladder | 0.00 | 0.00 | 0.00 | 0.00 |
| duel_weak | gate | 0.00 | 0.00 | 0.00 | 0.00 |
| duel_weak | life | 0.00 | 0.00 | 0.09 (0.04) | 0.00 |
| farmer | ladder | 10.09 (0.98) | 19.89 (0.18) | 0.10 (0.26) | 0.00 |
| farmer | gate | 0.00 | 11.57 (0.18) | 0.15 (0.32) | 0.00 |
| farmer | life | 2.20 (0.54) | 6.19 (0.09) | 0.94 (0.30) | 1.58 (0.05) |
| fortress | ladder | 7.26 (0.98) | 19.85 (0.18) | 2.97 (0.89) | 0.00 |
| fortress | gate | 0.00 | 11.52 (0.17) | 0.07 (0.72) | 0.00 |
| fortress | life | 1.97 (0.55) | 5.86 (0.10) | 0.34 (0.66) | 1.56 (0.05) |
| multi | ladder | 7.41 (0.99) | 17.10 (0.20) | 8.85 (0.88) | 0.00 |
| multi | gate | 0.00 | 11.56 (0.17) | 4.67 (0.81) | 0.00 |
| multi | life | 1.60 (0.50) | 4.73 (0.10) | 0.50 (0.44) | 1.21 (0.07) |
| multi_nospill | ladder | 7.41 (0.99) | 17.10 (0.20) | 8.85 (0.88) | 0.00 |
| multi_nospill | gate | 0.00 | 11.56 (0.17) | 4.67 (0.81) | 0.00 |
| multi_nospill | life | 1.59 (0.50) | 4.72 (0.10) | 0.49 (0.44) | 1.18 (0.06) |
| railway | ladder | 10.08 (0.98) | 19.91 (0.18) | 0.00 | 0.00 |
| railway | gate | 0.00 | 11.54 (0.17) | 0.00 | 22.05 (0.18) |
| railway | life | 1.91 (0.54) | 5.10 (0.09) | 0.16 (0.32) | 4.80 (0.17) |

## Each player's share (a year)

| Scenario | Mode | Player | Fights on him | Attackers killed there | Tribute paid |
|---|---|---|---|---|---|
| farmer | ladder | P:Ben | 0.10 | 0.0 | 0 |
| farmer | gate | P:Ben | 0.15 | 0.0 | 0 |
| farmer | life | P:Ben | 0.94 | 0.3 | 5 |
| fortress | ladder | P:Ben | 2.97 | 39.3 | 0 |
| fortress | gate | P:Ben | 0.07 | 1.2 | 0 |
| fortress | life | P:Ben | 0.34 | 5.4 | 0 |
| multi | ladder | P:Ash | 8.85 | 83.8 | 0 |
| multi | ladder | P:Bea | 0.00 | 0.0 | 76 |
| multi | gate | P:Ash | 4.67 | 50.6 | 0 |
| multi | gate | P:Bea | 0.00 | 0.0 | 0 |
| multi | life | P:Ash | 0.41 | 5.0 | 0 |
| multi | life | P:Bea | 0.09 | 0.1 | 369 |
| multi_nospill | ladder | P:Ash | 8.85 | 83.8 | 0 |
| multi_nospill | ladder | P:Bea | 0.00 | 0.0 | 76 |
| multi_nospill | gate | P:Ash | 4.67 | 50.6 | 0 |
| multi_nospill | gate | P:Bea | 0.00 | 0.0 | 0 |
| multi_nospill | life | P:Ash | 0.42 | 5.1 | 0 |
| multi_nospill | life | P:Bea | 0.07 | 0.1 | 363 |
| railway | ladder | P:rail | 0.00 | 0.0 | 0 |
| railway | gate | P:rail | 0.00 | 0.0 | 0 |
| railway | life | P:rail | 0.16 | 1.5 | 0 |

## Work (life): what the engine does a faction-year

| Scenario | Council sittings a faction-year | Relations weighed a sitting | Acts a faction-year |
|---|---|---|---|
| alone | 9.5 | 3.9 | 5.1 |
| duel_adaptive | 16.4 | 1.0 | 8.0 |
| duel_fortress | 18.1 | 1.0 | 8.3 |
| duel_stubborn | 11.3 | 1.0 | 6.3 |
| duel_weak | 16.2 | 1.0 | 7.9 |
| farmer | 9.6 | 4.2 | 5.5 |
| fortress | 10.2 | 4.4 | 5.6 |
| multi | 10.5 | 5.0 | 5.8 |
| multi_nospill | 10.6 | 5.0 | 5.9 |
| railway | 14.2 | 4.2 | 6.3 |

## Unattended for centuries (life, alone)

| Seed | Fights a year by century | Camp-on-camp a year by century |
|---|---|---|
| 7000 | 9.1, 10.1, 9.2, 9.0, 11.1 | 1.88, 1.99, 1.62, 1.44, 2.45 |
| 7001 | 8.7, 10.4, 10.4, 10.2, 10.0 | 1.54, 2.05, 2.19, 2.11, 2.19 |
| 7002 | 10.5, 10.4, 9.8, 10.6, 10.0 | 3.09, 3.09, 2.65, 3.00, 3.06 |
| 7003 | 10.9, 11.0, 10.7, 10.3, 10.4 | 3.14, 2.53, 2.59, 2.81, 2.66 |
| 7004 | 10.3, 11.3, 10.1, 10.9, 10.6 | 2.32, 2.77, 2.50, 2.37, 2.71 |
| 7005 | 10.3, 9.8, 11.2, 9.6, 10.3 | 2.08, 2.32, 2.94, 1.72, 2.11 |
| 7006 | 10.4, 9.4, 9.8, 9.3, 10.3 | 2.16, 1.93, 2.01, 2.19, 2.31 |
| 7007 | 10.5, 10.6, 10.3, 10.6, 10.8 | 2.64, 2.65, 2.65, 2.10, 2.33 |
