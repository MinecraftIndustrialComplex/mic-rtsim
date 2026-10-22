# Conflict-life model: results

8 seeds per scenario and mode, 20 game years each; run took 10.4 s. `ladder` is today's engine (EQ-5 (c)), `gate` is EQUILIBRIUM 2.5 (EQ-5 (a)), `life` is CONFLICT-LIFE.md.

## Fights per world-year, losses, stability

| Scenario | Mode | Fights a year: mean [p5, median, p95] | Years with none | Camp-on-camp a year | Lost share | Lost a year | Endless war | Surge | Extinct | Decade with no camp fight |
|---|---|---|---|---|---|---|---|---|---|---|
| alone | ladder | 29.2 [23, 29, 36] | 0.00 | 9.94 | 0.46 | 13.56 | 1.00 | 0.00 | 0.00 | 0.00 |
| alone | gate | 11.6 [9, 11, 15] | 0.00 | 0.00 | 0.17 | 1.99 | 1.00 | 0.00 | 0.00 | 1.00 |
| alone | life | 13.8 [9, 14, 19] | 0.00 | 0.97 | 0.11 | 1.51 | 1.00 | 0.00 | 0.00 | 0.00 |
| duel_adaptive | ladder | 0.0 [0, 0, 0] | 1.00 | 0.00 | n/a | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 |
| duel_adaptive | gate | 0.0 [0, 0, 0] | 1.00 | 0.00 | n/a | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 |
| duel_adaptive | life | 1.2 [0, 1, 2] | 0.18 | 0.00 | 0.37 | 0.43 | 0.00 | 0.00 | 0.00 | 1.00 |
| duel_fortress | ladder | 7.7 [4, 9, 10] | 0.00 | 0.00 | 0.98 | 7.56 | 1.00 | 0.00 | 0.88 | 1.00 |
| duel_fortress | gate | 0.0 [0, 0, 0] | 1.00 | 0.00 | n/a | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 |
| duel_fortress | life | 0.2 [0, 0, 1] | 0.78 | 0.00 | 0.97 | 0.21 | 0.00 | 0.00 | 0.00 | 1.00 |
| duel_stubborn | ladder | 21.2 [19, 21, 23] | 0.00 | 0.00 | 0.23 | 4.81 | 1.00 | 0.00 | 0.00 | 1.00 |
| duel_stubborn | gate | 12.5 [11, 12, 14] | 0.00 | 0.00 | 0.22 | 2.77 | 1.00 | 0.00 | 0.00 | 1.00 |
| duel_stubborn | life | 4.1 [3, 4, 6] | 0.00 | 0.00 | 0.04 | 0.17 | 1.00 | 0.00 | 0.00 | 1.00 |
| duel_weak | ladder | 0.0 [0, 0, 0] | 1.00 | 0.00 | n/a | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 |
| duel_weak | gate | 0.0 [0, 0, 0] | 1.00 | 0.00 | n/a | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 |
| duel_weak | life | 3.8 [2, 4, 5] | 0.00 | 0.00 | 0.04 | 0.16 | 1.00 | 0.00 | 0.00 | 1.00 |
| farmer | ladder | 29.7 [22, 30, 38] | 0.00 | 9.72 | 0.44 | 13.18 | 1.00 | 0.00 | 0.00 | 0.00 |
| farmer | gate | 11.6 [9, 12, 15] | 0.00 | 0.00 | 0.18 | 2.09 | 1.00 | 0.00 | 0.00 | 1.00 |
| farmer | life | 15.5 [10, 15, 21] | 0.00 | 1.09 | 0.13 | 2.06 | 1.00 | 0.00 | 0.12 | 0.00 |
| fortress | ladder | 30.2 [24, 30, 36] | 0.00 | 7.34 | 0.45 | 13.49 | 1.00 | 0.00 | 0.12 | 0.00 |
| fortress | gate | 11.7 [9, 11, 15] | 0.00 | 0.00 | 0.17 | 1.99 | 1.00 | 0.00 | 0.00 | 1.00 |
| fortress | life | 14.3 [9, 14, 21] | 0.00 | 0.99 | 0.12 | 1.66 | 1.00 | 0.00 | 0.00 | 0.00 |
| multi | ladder | 33.7 [27, 34, 39] | 0.00 | 7.51 | 0.56 | 18.77 | 1.00 | 0.00 | 0.12 | 0.00 |
| multi | gate | 16.7 [12, 17, 21] | 0.00 | 0.00 | 0.36 | 5.96 | 1.00 | 0.00 | 0.00 | 1.00 |
| multi | life | 16.1 [11, 16, 22] | 0.00 | 0.66 | 0.10 | 1.63 | 1.00 | 0.00 | 0.00 | 0.00 |
| multi_nospill | ladder | 33.7 [27, 34, 39] | 0.00 | 7.51 | 0.56 | 18.77 | 1.00 | 0.00 | 0.12 | 0.00 |
| multi_nospill | gate | 16.7 [12, 17, 21] | 0.00 | 0.00 | 0.36 | 5.96 | 1.00 | 0.00 | 0.00 | 1.00 |
| multi_nospill | life | 16.5 [11, 17, 22] | 0.00 | 0.87 | 0.12 | 1.91 | 1.00 | 0.00 | 0.00 | 0.00 |
| railway | ladder | 30.1 [22, 29, 38] | 0.00 | 10.07 | 0.45 | 13.39 | 1.00 | 0.00 | 0.00 | 0.00 |
| railway | gate | 33.5 [30, 34, 37] | 0.00 | 0.00 | 0.18 | 6.11 | 1.00 | 0.00 | 0.00 | 1.00 |
| railway | life | 16.7 [11, 16, 23] | 0.00 | 1.09 | 0.14 | 2.33 | 1.00 | 0.00 | 0.00 | 0.00 |

## How formulaic

| Scenario | Mode | Interval CV median [p10, p90] | Next act called to the day | Culture spread (JS) | Spread of a camp's fights a year (CV over seeds) | Story overlap of two seeds |
|---|---|---|---|---|---|---|
| alone | ladder | 0.81 [0.74, 0.94] | 0.18 | 0.000 | 0.05 | 0.50 |
| alone | gate | 0.73 [0.65, 0.91] | 0.13 | 0.000 | 0.05 | 0.42 |
| alone | life | 0.78 [0.55, 0.99] | 0.03 | 0.025 | 0.15 | 0.09 |
| duel_adaptive | ladder | n/a [n/a, n/a] | 1.00 | 0.000 | n/a | 1.00 |
| duel_adaptive | gate | n/a [n/a, n/a] | 0.34 | 0.000 | n/a | 0.39 |
| duel_adaptive | life | 0.46 [0.43, 0.59] | 0.08 | 0.010 | 0.07 | 0.26 |
| duel_fortress | ladder | 0.37 [0.20, 0.45] | 0.92 | 0.171 | 0.20 | 0.33 |
| duel_fortress | gate | n/a [n/a, n/a] | 1.00 | 0.000 | n/a | 1.00 |
| duel_fortress | life | 0.36 [0.30, 0.47] | 0.11 | 0.008 | 0.34 | 0.34 |
| duel_stubborn | ladder | 0.23 [0.21, 0.26] | 0.64 | 0.000 | 0.01 | 0.95 |
| duel_stubborn | gate | 0.34 [0.33, 0.35] | 0.11 | 0.000 | 0.01 | 0.61 |
| duel_stubborn | life | 0.47 [0.45, 0.58] | 0.05 | 0.013 | 0.03 | 0.21 |
| duel_weak | ladder | n/a [n/a, n/a] | 1.00 | 0.000 | n/a | 1.00 |
| duel_weak | gate | n/a [n/a, n/a] | 0.34 | 0.000 | n/a | 0.45 |
| duel_weak | life | 0.47 [0.44, 0.55] | 0.05 | 0.013 | 0.06 | 0.19 |
| farmer | ladder | 0.83 [0.74, 0.94] | 0.17 | 0.006 | 0.06 | 0.47 |
| farmer | gate | 0.76 [0.64, 0.91] | 0.12 | 0.002 | 0.10 | 0.43 |
| farmer | life | 0.86 [0.70, 1.17] | 0.02 | 0.017 | 0.20 | 0.09 |
| fortress | ladder | 0.76 [0.50, 0.88] | 0.15 | 0.001 | 0.04 | 0.49 |
| fortress | gate | 0.74 [0.65, 0.90] | 0.26 | 0.000 | 0.09 | 0.54 |
| fortress | life | 0.77 [0.63, 0.97] | 0.02 | 0.019 | 0.17 | 0.10 |
| multi | ladder | 0.64 [0.51, 0.80] | 0.16 | 0.004 | 0.04 | 0.41 |
| multi | gate | 0.70 [0.35, 0.96] | 0.09 | 0.004 | 0.07 | 0.31 |
| multi | life | 0.78 [0.61, 1.06] | 0.02 | 0.019 | 0.17 | 0.10 |
| multi_nospill | ladder | 0.64 [0.51, 0.80] | 0.16 | 0.004 | 0.04 | 0.41 |
| multi_nospill | gate | 0.70 [0.35, 0.96] | 0.09 | 0.004 | 0.07 | 0.31 |
| multi_nospill | life | 0.78 [0.64, 1.11] | 0.02 | 0.022 | 0.17 | 0.10 |
| railway | ladder | 0.83 [0.75, 0.92] | 0.18 | 0.000 | 0.07 | 0.51 |
| railway | gate | 0.67 [0.18, 0.82] | 0.51 | 0.000 | 0.05 | 0.78 |
| railway | life | 0.85 [0.68, 1.00] | 0.04 | 0.025 | 0.16 | 0.13 |

## Duels (game.py's setting: one goblin camp, one base, grievance 9 a day)

| Player | Mode | Wars a year, years 1-5 | Wars a year, last decade | Raiders killed a year | Tribute a year | Defence year 10 | Defence at the end |
|---|---|---|---|---|---|---|---|
| adaptive | ladder | 0.0 | 0.0 | 0 | 406 | 12 | 12 |
| adaptive | gate | 0.0 | 0.0 | 0 | 194 | 12 | 12 |
| adaptive | life | 1.5 | 1.0 | 12 | 0 | 31 | 30 |
| fortress | ladder | 6.0 | 8.9 | 65 | 0 | 80 | 80 |
| fortress | gate | 0.0 | 0.0 | 0 | 0 | 80 | 80 |
| fortress | life | 0.3 | 0.1 | 3 | 0 | 80 | 80 |
| stubborn | ladder | 20.7 | 21.4 | 61 | 0 | 6 | 6 |
| stubborn | gate | 11.8 | 12.8 | 37 | 0 | 6 | 6 |
| stubborn | life | 4.3 | 4.1 | 4 | 0 | 6 | 6 |
| weak | ladder | 0.0 | 0.0 | 0 | 406 | 6 | 6 |
| weak | gate | 0.0 | 0.0 | 0 | 361 | 6 | 6 |
| weak | life | 4.2 | 3.6 | 4 | 14 | 6 | 6 |

## What started the life-mode fights (share of fights)

| Scenario | Motive | Occasion |
|---|---|---|
| alone | loot 0.84, pride 0.08, glory 0.04, hunger 0.03 | caravan 0.66, restless 0.27, refused 0.07, news 0.00 |
| duel_adaptive | loot 1.00 | restless 0.99, refused 0.01 |
| duel_fortress | loot 1.00 | restless 1.00 |
| duel_stubborn | loot 1.00 | restless 0.98, refused 0.02 |
| duel_weak | loot 1.00 | restless 1.00 |
| farmer | loot 0.76, pride 0.16, hunger 0.05, glory 0.03 | caravan 0.58, restless 0.29, provoked 0.06, refused 0.05, news 0.01 |
| fortress | loot 0.86, pride 0.07, hunger 0.04, glory 0.03 | caravan 0.64, restless 0.29, refused 0.06, news 0.01, provoked 0.00 |
| multi | loot 0.87, pride 0.09, glory 0.02, hunger 0.01 | caravan 0.50, restless 0.37, refused 0.06, provoked 0.04, news 0.02 |
| multi_nospill | loot 0.86, pride 0.07, hunger 0.05, glory 0.01 | caravan 0.50, restless 0.42, refused 0.05, news 0.02, provoked 0.01 |
| railway | loot 0.87, pride 0.06, hunger 0.04, glory 0.03 | caravan 0.48, restless 0.26, train 0.21, refused 0.05, news 0.00, provoked 0.00 |

## Fights by target (a world-year)

| Scenario | Mode | Camps | Villages | Players | Trains and caravans |
|---|---|---|---|---|---|
| alone | ladder | 9.94 | 19.25 | 0.00 | 0.00 |
| alone | gate | 0.00 | 11.64 | 0.00 | 0.00 |
| alone | life | 0.97 | 3.79 | 0.00 | 9.07 |
| duel_adaptive | ladder | 0.00 | 0.00 | 0.00 | 0.00 |
| duel_adaptive | gate | 0.00 | 0.00 | 0.00 | 0.00 |
| duel_adaptive | life | 0.00 | 0.00 | 1.16 | 0.00 |
| duel_fortress | ladder | 0.00 | 0.00 | 7.71 | 0.00 |
| duel_fortress | gate | 0.00 | 0.00 | 0.00 | 0.00 |
| duel_fortress | life | 0.00 | 0.00 | 0.22 | 0.00 |
| duel_stubborn | ladder | 0.00 | 0.00 | 21.21 | 0.00 |
| duel_stubborn | gate | 0.00 | 0.00 | 12.46 | 0.00 |
| duel_stubborn | life | 0.00 | 0.00 | 4.11 | 0.00 |
| duel_weak | ladder | 0.00 | 0.00 | 0.00 | 0.00 |
| duel_weak | gate | 0.00 | 0.00 | 0.00 | 0.00 |
| duel_weak | life | 0.00 | 0.00 | 3.75 | 0.00 |
| farmer | ladder | 9.72 | 19.84 | 0.13 | 0.00 |
| farmer | gate | 0.00 | 11.47 | 0.11 | 0.00 |
| farmer | life | 1.09 | 3.74 | 1.62 | 9.03 |
| fortress | ladder | 7.34 | 19.74 | 3.09 | 0.00 |
| fortress | gate | 0.00 | 11.71 | 0.00 | 0.00 |
| fortress | life | 0.99 | 3.68 | 0.44 | 9.16 |
| multi | ladder | 7.51 | 17.16 | 9.06 | 0.00 |
| multi | gate | 0.00 | 11.75 | 4.92 | 0.00 |
| multi | life | 0.66 | 3.29 | 4.04 | 8.07 |
| multi_nospill | ladder | 7.51 | 17.16 | 9.06 | 0.00 |
| multi_nospill | gate | 0.00 | 11.75 | 4.92 | 0.00 |
| multi_nospill | life | 0.87 | 3.17 | 4.16 | 8.29 |
| railway | ladder | 10.07 | 20.01 | 0.00 | 0.00 |
| railway | gate | 0.00 | 11.54 | 0.00 | 22.01 |
| railway | life | 1.09 | 3.74 | 0.32 | 11.51 |

## Unattended for centuries (life, alone)

| Seed | Fights a year by century | Camp-on-camp a year by century |
|---|---|---|
| 7000 | 13.5, 13.3 | 0.83, 1.00 |
| 7001 | 13.2, 13.2 | 0.90, 0.95 |
| 7002 | 13.2, 13.7 | 1.28, 1.10 |
| 7003 | 13.9, 14.0 | 1.00, 1.18 |
