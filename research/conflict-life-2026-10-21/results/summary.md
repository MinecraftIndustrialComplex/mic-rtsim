# Conflict-life model: results

8 seeds per scenario and mode, 20 game years each; run took 14.2 s. `ladder` is today's engine (EQ-5 (c)), `gate` is EQUILIBRIUM 2.5 (EQ-5 (a)), `life` is CONFLICT-LIFE.md.

## Fights per world-year, losses, stability

| Scenario | Mode | Fights a year: mean [p5, median, p95] | Years with none | Camp-on-camp a year | Lost share | Lost a year | Endless war | Surge | Extinct | Decade with no camp fight | No camp fight at all |
|---|---|---|---|---|---|---|---|---|---|---|---|
| alone | ladder | 30.5 [23, 30, 38] | 0.00 | 10.51 | 0.45 | 13.75 | 1.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| alone | gate | 11.2 [9, 11, 14] | 0.00 | 0.00 | 0.17 | 1.93 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| alone | life | 9.4 [5, 9, 15] | 0.00 | 2.26 | 0.19 | 1.82 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| duel_adaptive | ladder | 0.0 [0, 0, 0] | 1.00 | 0.00 | n/a | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_adaptive | gate | 0.0 [0, 0, 0] | 1.00 | 0.00 | n/a | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_adaptive | life | 0.5 [0, 0, 1] | 0.51 | 0.00 | 0.16 | 0.09 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_fortress | ladder | 8.3 [4, 9, 10] | 0.00 | 0.00 | 0.98 | 8.15 | 0.00 | 0.00 | 0.88 | 1.00 | 1.00 |
| duel_fortress | gate | 0.0 [0, 0, 0] | 1.00 | 0.00 | n/a | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_fortress | life | 0.2 [0, 0, 1] | 0.81 | 0.00 | 0.85 | 0.18 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_stubborn | ladder | 21.1 [19, 21, 23] | 0.00 | 0.00 | 0.24 | 5.01 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_stubborn | gate | 12.4 [10, 12, 15] | 0.00 | 0.00 | 0.22 | 2.74 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_stubborn | life | 3.2 [2, 3, 5] | 0.00 | 0.00 | 0.03 | 0.09 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_weak | ladder | 0.0 [0, 0, 0] | 1.00 | 0.00 | n/a | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_weak | gate | 0.0 [0, 0, 0] | 1.00 | 0.00 | n/a | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_weak | life | 1.2 [0, 1, 2] | 0.09 | 0.00 | 0.04 | 0.05 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| farmer | ladder | 29.9 [23, 30, 38] | 0.00 | 9.97 | 0.45 | 13.46 | 1.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| farmer | gate | 11.5 [8, 12, 14] | 0.00 | 0.00 | 0.16 | 1.87 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| farmer | life | 11.7 [6, 11, 20] | 0.00 | 2.55 | 0.23 | 2.75 | 0.12 | 0.00 | 0.00 | 0.00 | 0.00 |
| fortress | ladder | 29.9 [24, 30, 36] | 0.00 | 7.25 | 0.44 | 13.08 | 1.00 | 0.00 | 0.38 | 0.00 | 0.00 |
| fortress | gate | 12.2 [9, 12, 17] | 0.00 | 0.00 | 0.21 | 2.51 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| fortress | life | 10.6 [5, 10, 18] | 0.00 | 2.62 | 0.23 | 2.38 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| multi | ladder | 33.8 [26, 34, 41] | 0.00 | 7.54 | 0.56 | 18.79 | 1.00 | 0.00 | 0.38 | 0.00 | 0.00 |
| multi | gate | 16.0 [11, 16, 20] | 0.00 | 0.00 | 0.35 | 5.66 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| multi | life | 8.7 [5, 8, 14] | 0.00 | 1.84 | 0.22 | 1.88 | 0.12 | 0.00 | 0.00 | 0.00 | 0.00 |
| multi_nospill | ladder | 33.8 [26, 34, 41] | 0.00 | 7.54 | 0.56 | 18.79 | 1.00 | 0.00 | 0.38 | 0.00 | 0.00 |
| multi_nospill | gate | 16.0 [11, 16, 20] | 0.00 | 0.00 | 0.35 | 5.66 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| multi_nospill | life | 8.4 [5, 8, 13] | 0.00 | 1.82 | 0.23 | 1.95 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| railway | ladder | 29.6 [22, 30, 37] | 0.00 | 9.97 | 0.45 | 13.34 | 1.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| railway | gate | 33.9 [30, 34, 37] | 0.00 | 0.00 | 0.17 | 5.92 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| railway | life | 11.5 [7, 11, 18] | 0.00 | 2.02 | 0.21 | 2.46 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 |

## How formulaic

| Scenario | Mode | Interval CV median [p10, p90] | Pair CV median [p10, p90] | Next act called to the day | Culture spread (JS) | Spread of a camp's fights a year (CV over seeds) | Story overlap of two seeds |
|---|---|---|---|---|---|---|---|
| alone | ladder | 0.80 [0.78, 0.93] | 0.80 [0.78, 0.93] | 0.19 | 0.000 | 0.06 | 0.52 |
| alone | gate | 0.82 [0.68, 0.87] | 0.82 [0.68, 0.87] | 0.15 | 0.000 | 0.05 | 0.41 |
| alone | life | 0.81 [0.67, 1.00] | 0.91 [0.56, 1.69] | 0.03 | 0.040 | 0.21 | 0.07 |
| duel_adaptive | ladder | n/a [n/a, n/a] | n/a [n/a, n/a] | 1.00 | 0.000 | n/a | 1.00 |
| duel_adaptive | gate | n/a [n/a, n/a] | n/a [n/a, n/a] | 0.33 | 0.000 | n/a | 0.47 |
| duel_adaptive | life | 0.59 [0.41, 0.72] | 0.59 [0.41, 0.72] | 0.05 | 0.025 | 0.13 | 0.17 |
| duel_fortress | ladder | 0.29 [0.13, 0.42] | 0.29 [0.13, 0.42] | 0.93 | 0.145 | 0.19 | 0.41 |
| duel_fortress | gate | n/a [n/a, n/a] | n/a [n/a, n/a] | 1.00 | 0.000 | n/a | 1.00 |
| duel_fortress | life | 0.74 [0.56, 1.08] | 0.74 [0.56, 1.08] | 0.07 | 0.052 | 0.70 | 0.30 |
| duel_stubborn | ladder | 0.22 [0.19, 0.25] | 0.22 [0.19, 0.25] | 0.63 | 0.000 | 0.01 | 0.95 |
| duel_stubborn | gate | 0.34 [0.32, 0.34] | 0.34 [0.32, 0.34] | 0.11 | 0.000 | 0.02 | 0.62 |
| duel_stubborn | life | 0.56 [0.51, 0.61] | 0.56 [0.51, 0.61] | 0.03 | 0.048 | 0.03 | 0.18 |
| duel_weak | ladder | n/a [n/a, n/a] | n/a [n/a, n/a] | 1.00 | 0.000 | n/a | 1.00 |
| duel_weak | gate | n/a [n/a, n/a] | n/a [n/a, n/a] | 0.34 | 0.000 | n/a | 0.51 |
| duel_weak | life | 0.43 [0.33, 0.55] | 0.43 [0.33, 0.55] | 0.04 | 0.038 | 0.12 | 0.13 |
| farmer | ladder | 0.85 [0.76, 0.93] | 0.85 [0.76, 0.93] | 0.17 | 0.004 | 0.04 | 0.48 |
| farmer | gate | 0.76 [0.64, 1.03] | 0.76 [0.64, 1.03] | 0.14 | 0.002 | 0.05 | 0.40 |
| farmer | life | 0.85 [0.64, 1.16] | 0.96 [0.62, 1.45] | 0.02 | 0.027 | 0.26 | 0.07 |
| fortress | ladder | 0.76 [0.51, 0.93] | 0.78 [0.51, 0.92] | 0.16 | 0.000 | 0.05 | 0.49 |
| fortress | gate | 0.73 [0.57, 0.88] | 0.73 [0.57, 0.88] | 0.23 | 0.060 | 0.50 | 0.52 |
| fortress | life | 0.90 [0.62, 1.15] | 0.87 [0.65, 1.42] | 0.02 | 0.027 | 0.22 | 0.07 |
| multi | ladder | 0.69 [0.50, 0.80] | 0.81 [0.55, 1.33] | 0.16 | 0.003 | 0.04 | 0.44 |
| multi | gate | 0.71 [0.36, 0.87] | 0.70 [0.36, 0.85] | 0.09 | 0.005 | 0.09 | 0.32 |
| multi | life | 0.70 [0.49, 0.99] | 0.77 [0.56, 1.27] | 0.02 | 0.035 | 0.19 | 0.08 |
| multi_nospill | ladder | 0.69 [0.50, 0.80] | 0.81 [0.55, 1.33] | 0.16 | 0.003 | 0.04 | 0.44 |
| multi_nospill | gate | 0.71 [0.36, 0.87] | 0.70 [0.36, 0.85] | 0.09 | 0.005 | 0.09 | 0.32 |
| multi_nospill | life | 0.73 [0.56, 0.96] | 0.84 [0.54, 1.48] | 0.02 | 0.025 | 0.18 | 0.09 |
| railway | ladder | 0.84 [0.74, 0.93] | 0.84 [0.74, 0.93] | 0.19 | 0.000 | 0.06 | 0.52 |
| railway | gate | 0.70 [0.17, 0.88] | 0.70 [0.17, 0.88] | 0.52 | 0.000 | 0.05 | 0.78 |
| railway | life | 0.85 [0.63, 1.11] | 0.89 [0.52, 1.40] | 0.04 | 0.028 | 0.19 | 0.12 |

## Duels (game.py's setting: one goblin camp, one base, grievance 9 a day)

| Player | Mode | Wars a year, years 1-5 | Wars a year, last decade | Raiders killed a year | Tribute a year | Defence year 10 | Defence at the end |
|---|---|---|---|---|---|---|---|
| adaptive | ladder | 0.0 | 0.0 | 0 | 406 | 12 | 12 |
| adaptive | gate | 0.0 | 0.0 | 0 | 195 | 12 | 12 |
| adaptive | life | 0.6 | 0.5 | 3 | 197 | 17 | 17 |
| fortress | ladder | 6.8 | 9.0 | 65 | 0 | 80 | 80 |
| fortress | gate | 0.0 | 0.0 | 0 | 0 | 80 | 80 |
| fortress | life | 0.3 | 0.2 | 4 | 0 | 80 | 80 |
| stubborn | ladder | 20.9 | 21.2 | 62 | 0 | 6 | 6 |
| stubborn | gate | 12.0 | 12.7 | 37 | 0 | 6 | 6 |
| stubborn | life | 3.5 | 3.1 | 3 | 0 | 6 | 6 |
| weak | ladder | 0.0 | 0.0 | 0 | 406 | 6 | 6 |
| weak | gate | 0.0 | 0.0 | 0 | 361 | 6 | 6 |
| weak | life | 1.4 | 1.2 | 1 | 202 | 6 | 6 |

## What started the life-mode fights (share of fights)

A driver is any motive that was at least a quarter of the push toward the fight; a fight can have several. Loot only: greed for loot with no hunger, grudge, pride, glory or fear behind it.

| Scenario | Top motive | Drivers | Loot only | Occasion |
|---|---|---|---|---|
| alone | loot 0.40, hunger 0.34, glory 0.14, pride 0.11, revenge 0.00, preempt 0.00 | loot 0.59, hunger 0.37, glory 0.27, pride 0.13, revenge 0.02, preempt 0.01 | 0.25 | restless 0.77, caravan 0.18, provoked 0.04, news 0.01 |
| duel_adaptive | loot 0.59, glory 0.38, revenge 0.01, hunger 0.01 | loot 0.83, glory 0.79, revenge 0.22, hunger 0.03 | 0.15 | restless 1.00 |
| duel_fortress | hunger 0.76, loot 0.21, pride 0.03 | hunger 0.76, loot 0.32, pride 0.09, glory 0.06, revenge 0.03 | 0.06 | restless 0.62, refused 0.38 |
| duel_stubborn | loot 0.74, revenge 0.16, pride 0.07, glory 0.02, hunger 0.00 | loot 0.97, revenge 0.56, pride 0.18, glory 0.13, hunger 0.00 | 0.24 | restless 0.82, refused 0.18 |
| duel_weak | loot 0.75, glory 0.19, revenge 0.06 | loot 0.98, glory 0.54, revenge 0.43, hunger 0.01 | 0.31 | restless 1.00 |
| farmer | hunger 0.39, loot 0.29, pride 0.22, glory 0.10, preempt 0.00, revenge 0.00 | loot 0.44, hunger 0.42, pride 0.24, glory 0.20, revenge 0.01, preempt 0.00 | 0.17 | restless 0.74, provoked 0.12, caravan 0.11, refused 0.02, news 0.01 |
| fortress | hunger 0.39, loot 0.36, pride 0.13, glory 0.11, preempt 0.01, revenge 0.00 | loot 0.52, hunger 0.43, glory 0.22, pride 0.16, revenge 0.02, preempt 0.02 | 0.20 | restless 0.78, caravan 0.14, provoked 0.04, refused 0.03, news 0.01 |
| multi | loot 0.41, hunger 0.27, glory 0.16, pride 0.15, revenge 0.01, preempt 0.00 | loot 0.55, glory 0.30, hunger 0.30, pride 0.18, revenge 0.03, preempt 0.00 | 0.24 | restless 0.76, caravan 0.13, refused 0.05, provoked 0.05, news 0.01 |
| multi_nospill | loot 0.40, hunger 0.23, pride 0.18, glory 0.18, revenge 0.00, preempt 0.00 | loot 0.57, glory 0.33, hunger 0.26, pride 0.20, revenge 0.02, preempt 0.01 | 0.23 | restless 0.75, caravan 0.14, provoked 0.05, refused 0.04, news 0.01 |
| railway | loot 0.59, hunger 0.21, glory 0.09, pride 0.09, revenge 0.02, preempt 0.00 | loot 0.75, hunger 0.24, glory 0.21, revenge 0.13, pride 0.11, preempt 0.00 | 0.35 | restless 0.56, train 0.30, caravan 0.10, provoked 0.03, refused 0.01, news 0.00 |

## Fights by target (a world-year)

| Scenario | Mode | Camps | Villages | Players | Trains and caravans |
|---|---|---|---|---|---|
| alone | ladder | 10.51 | 20.03 | 0.00 | 0.00 |
| alone | gate | 0.00 | 11.23 | 0.00 | 0.00 |
| alone | life | 2.26 | 5.47 | 0.00 | 1.65 |
| duel_adaptive | ladder | 0.00 | 0.00 | 0.00 | 0.00 |
| duel_adaptive | gate | 0.00 | 0.00 | 0.00 | 0.00 |
| duel_adaptive | life | 0.00 | 0.00 | 0.54 | 0.00 |
| duel_fortress | ladder | 0.00 | 0.00 | 8.29 | 0.00 |
| duel_fortress | gate | 0.00 | 0.00 | 0.00 | 0.00 |
| duel_fortress | life | 0.00 | 0.00 | 0.21 | 0.00 |
| duel_stubborn | ladder | 0.00 | 0.00 | 21.15 | 0.00 |
| duel_stubborn | gate | 0.00 | 0.00 | 12.41 | 0.00 |
| duel_stubborn | life | 0.00 | 0.00 | 3.17 | 0.00 |
| duel_weak | ladder | 0.00 | 0.00 | 0.00 | 0.00 |
| duel_weak | gate | 0.00 | 0.00 | 0.00 | 0.00 |
| duel_weak | life | 0.00 | 0.00 | 1.22 | 0.00 |
| farmer | ladder | 9.97 | 19.89 | 0.08 | 0.00 |
| farmer | gate | 0.00 | 11.32 | 0.14 | 0.00 |
| farmer | life | 2.55 | 6.21 | 1.64 | 1.32 |
| fortress | ladder | 7.25 | 19.66 | 3.04 | 0.00 |
| fortress | gate | 0.00 | 11.57 | 0.64 | 0.00 |
| fortress | life | 2.62 | 5.90 | 0.56 | 1.49 |
| multi | ladder | 7.54 | 17.34 | 8.95 | 0.00 |
| multi | gate | 0.00 | 11.46 | 4.58 | 0.00 |
| multi | life | 1.84 | 4.60 | 1.17 | 1.12 |
| multi_nospill | ladder | 7.54 | 17.34 | 8.95 | 0.00 |
| multi_nospill | gate | 0.00 | 11.46 | 4.58 | 0.00 |
| multi_nospill | life | 1.82 | 4.42 | 0.94 | 1.20 |
| railway | ladder | 9.97 | 19.61 | 0.00 | 0.00 |
| railway | gate | 0.00 | 11.79 | 0.00 | 22.12 |
| railway | life | 2.02 | 4.69 | 0.23 | 4.56 |

## Each player's share (a year)

| Scenario | Mode | Player | Fights on him | Attackers killed there | Tribute paid |
|---|---|---|---|---|---|
| farmer | ladder | P:Ben | 0.08 | 0.0 | 0 |
| farmer | gate | P:Ben | 0.14 | 0.0 | 0 |
| farmer | life | P:Ben | 1.64 | 2.5 | 3 |
| fortress | ladder | P:Ben | 3.04 | 40.6 | 0 |
| fortress | gate | P:Ben | 0.64 | 10.1 | 0 |
| fortress | life | P:Ben | 0.56 | 8.8 | 0 |
| multi | ladder | P:Ash | 8.95 | 85.0 | 0 |
| multi | ladder | P:Bea | 0.00 | 0.0 | 76 |
| multi | gate | P:Ash | 4.58 | 49.1 | 0 |
| multi | gate | P:Bea | 0.00 | 0.0 | 0 |
| multi | life | P:Ash | 0.61 | 7.7 | 0 |
| multi | life | P:Bea | 0.56 | 0.8 | 343 |
| multi_nospill | ladder | P:Ash | 8.95 | 85.0 | 0 |
| multi_nospill | ladder | P:Bea | 0.00 | 0.0 | 76 |
| multi_nospill | gate | P:Ash | 4.58 | 49.1 | 0 |
| multi_nospill | gate | P:Bea | 0.00 | 0.0 | 0 |
| multi_nospill | life | P:Ash | 0.66 | 8.4 | 0 |
| multi_nospill | life | P:Bea | 0.29 | 0.5 | 340 |
| railway | ladder | P:rail | 0.00 | 0.0 | 0 |
| railway | gate | P:rail | 0.00 | 0.0 | 0 |
| railway | life | P:rail | 0.23 | 2.8 | 0 |

## Work (life): what the engine does a faction-year

| Scenario | Council sittings a faction-year | Relations weighed a sitting | Acts a faction-year |
|---|---|---|---|
| alone | 9.2 | 3.9 | 4.9 |
| duel_adaptive | 14.0 | 1.0 | 7.0 |
| duel_fortress | 17.7 | 1.0 | 8.6 |
| duel_stubborn | 11.0 | 1.0 | 6.2 |
| duel_weak | 12.3 | 1.0 | 6.5 |
| farmer | 9.6 | 4.2 | 5.5 |
| fortress | 10.0 | 4.4 | 5.7 |
| multi | 10.2 | 4.9 | 5.7 |
| multi_nospill | 10.5 | 4.9 | 5.9 |
| railway | 14.3 | 4.2 | 6.5 |

## Unattended for centuries (life, alone)

| Seed | Fights a year by century | Camp-on-camp a year by century |
|---|---|---|
| 7000 | 9.7, 10.7 | 2.00, 2.52 |
| 7001 | 9.7, 11.2 | 2.60, 2.85 |
| 7002 | 10.3, 10.3 | 2.55, 3.04 |
| 7003 | 11.0, 9.8 | 2.83, 2.01 |
