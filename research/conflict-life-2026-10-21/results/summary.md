# Conflict-life model: results

64 seeds per scenario and mode, 50 game years each; run took 698.2 s. `ladder` is today's engine (EQ-5 (c)), `gate` is EQUILIBRIUM 2.5 (EQ-5 (a)), `life` is CONFLICT-LIFE.md.

## Fights per world-year, losses, stability

| Scenario | Mode | Fights a year: mean [p5, median, p95] | Years with none | Camp-on-camp a year | Lost share | Lost a year | Endless war | Surge | Extinct | Decade with no camp fight | No camp fight at all |
|---|---|---|---|---|---|---|---|---|---|---|---|
| alone | ladder | 30.1 [23, 30, 38] | 0.00 | 10.16 | 0.34 | 10.12 | 1.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| alone | gate | 12.6 [9, 12, 18] | 0.00 | 2.07 | 0.12 | 1.55 | 0.00 | 0.00 | 0.00 | 0.02 | 0.00 |
| alone | life | 7.4 [3, 7, 14] | 0.00 | 1.16 | 0.02 | 0.16 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| alone_noally | life | 7.3 [3, 7, 14] | 0.00 | 1.16 | 0.02 | 0.17 | 0.00 | 0.00 | 0.00 | 0.02 | 0.00 |
| duel_adaptive | ladder | 9.1 [4, 10, 10] | 0.00 | 0.00 | 0.99 | 8.96 | 0.00 | 0.03 | 0.00 | 1.00 | 1.00 |
| duel_adaptive | gate | 2.3 [0, 3, 4] | 0.23 | 0.00 | 0.76 | 1.76 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_adaptive | life | 0.1 [0, 0, 1] | 0.95 | 0.00 | 0.05 | 0.00 | 0.00 | 0.02 | 0.00 | 1.00 | 1.00 |
| duel_fortress | ladder | 8.8 [4, 10, 10] | 0.00 | 0.00 | 1.00 | 8.83 | 0.00 | 0.06 | 0.00 | 1.00 | 1.00 |
| duel_fortress | gate | 0.0 [0, 0, 0] | 1.00 | 0.00 | n/a | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_fortress | life | 0.0 [0, 0, 0] | 0.99 | 0.00 | 1.00 | 0.01 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_stubborn | ladder | 23.6 [21, 24, 24] | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_stubborn | gate | 10.4 [10, 10, 11] | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_stubborn | life | 3.4 [2, 3, 5] | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_weak | ladder | 13.5 [0, 22, 24] | 0.42 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_weak | gate | 0.0 [0, 0, 0] | 1.00 | 0.00 | n/a | 0.00 | 0.00 | 0.00 | 0.00 | 1.00 | 1.00 |
| duel_weak | life | 0.2 [0, 0, 1] | 0.87 | 0.00 | 0.00 | 0.00 | 0.00 | 0.09 | 0.00 | 1.00 | 1.00 |
| farmer | ladder | 30.1 [23, 30, 38] | 0.00 | 10.07 | 0.33 | 10.02 | 1.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| farmer | gate | 12.9 [9, 12, 19] | 0.00 | 2.12 | 0.12 | 1.60 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| farmer | life | 9.3 [4, 9, 16] | 0.00 | 1.17 | 0.02 | 0.19 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| farmer_noally | life | 9.3 [4, 9, 16] | 0.00 | 1.19 | 0.02 | 0.18 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| fortress | ladder | 29.7 [23, 30, 37] | 0.00 | 7.18 | 0.33 | 9.91 | 1.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| fortress | gate | 13.0 [9, 13, 18] | 0.00 | 1.64 | 0.16 | 2.06 | 0.00 | 0.00 | 0.00 | 0.03 | 0.00 |
| fortress | life | 7.2 [3, 7, 14] | 0.00 | 1.09 | 0.04 | 0.30 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| fortress_noally | life | 7.3 [3, 7, 14] | 0.00 | 1.11 | 0.04 | 0.30 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| multi | ladder | 32.0 [26, 32, 39] | 0.00 | 7.24 | 0.46 | 14.71 | 1.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| multi | gate | 14.6 [11, 15, 18] | 0.00 | 0.64 | 0.26 | 3.78 | 0.00 | 0.00 | 0.00 | 0.12 | 0.00 |
| multi | life | 4.9 [2, 5, 9] | 0.00 | 0.58 | 0.06 | 0.28 | 0.00 | 0.00 | 0.00 | 0.05 | 0.00 |
| multi_noally | life | 4.9 [2, 5, 9] | 0.00 | 0.58 | 0.06 | 0.29 | 0.00 | 0.00 | 0.00 | 0.05 | 0.00 |
| multi_nospill | ladder | 32.0 [26, 32, 39] | 0.00 | 7.24 | 0.46 | 14.71 | 1.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| multi_nospill | gate | 14.6 [11, 15, 18] | 0.00 | 0.64 | 0.26 | 3.78 | 0.00 | 0.00 | 0.00 | 0.12 | 0.00 |
| multi_nospill | life | 4.9 [2, 5, 9] | 0.00 | 0.60 | 0.06 | 0.27 | 0.00 | 0.00 | 0.00 | 0.08 | 0.00 |
| railway | ladder | 29.9 [22, 30, 38] | 0.00 | 10.03 | 0.33 | 10.00 | 1.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| railway | gate | 12.8 [9, 12, 18] | 0.00 | 2.13 | 0.13 | 1.61 | 0.00 | 0.00 | 0.00 | 0.00 | 0.00 |
| railway | life | 8.4 [4, 8, 14] | 0.00 | 0.78 | 0.06 | 0.52 | 0.00 | 0.00 | 0.00 | 0.03 | 0.00 |
| railway_noally | life | 8.3 [4, 8, 14] | 0.00 | 0.75 | 0.06 | 0.52 | 0.00 | 0.00 | 0.00 | 0.02 | 0.00 |

## How formulaic

| Scenario | Mode | Interval CV median [p10, p90] | Pair CV median [p10, p90] | Next act called to the day | Culture spread (JS) | Spread of a camp's fights a year (CV over seeds) | Story overlap of two seeds (chance) | Excess dispersion of a camp's years median [p10, p90] |
|---|---|---|---|---|---|---|---|---|
| alone | ladder | 0.83 [0.77, 0.91] | 0.83 [0.77, 0.91] | 0.18 | 0.000 | 0.04 | 0.52 (0.49) | 0.00 [0.00, 0.00] |
| alone | gate | 0.83 [0.67, 1.94] | 1.30 [0.68, 2.21] | 0.17 | 0.004 | 0.10 | 0.38 (0.36) | 0.00 [0.00, 1.06] |
| alone | life | 0.85 [0.70, 1.06] | 0.77 [0.55, 1.11] | 0.02 | 0.012 | 0.16 | 0.09 (0.08) | 0.32 [0.00, 0.61] |
| alone_noally | life | 0.84 [0.71, 1.06] | 0.78 [0.55, 1.09] | 0.02 | 0.011 | 0.15 | 0.09 (0.07) | 0.31 [0.00, 0.61] |
| duel_adaptive | ladder | 0.24 [0.12, 0.39] | 0.24 [0.12, 0.39] | 0.98 | 0.014 | 0.06 | 0.53 (0.34) | 0.00 [0.00, 0.00] |
| duel_adaptive | gate | 1.29 [1.27, 1.30] | 1.29 [1.27, 1.30] | 0.11 | 0.000 | 0.02 | 0.61 (0.14) | 0.48 [0.41, 0.53] |
| duel_adaptive | life | 0.79 [0.41, 1.23] | 0.79 [0.41, 1.23] | 0.04 | 0.002 | n/a | 0.18 (0.16) | n/a [n/a, n/a] |
| duel_fortress | ladder | 0.26 [0.19, 0.40] | 0.26 [0.19, 0.40] | 0.98 | 0.014 | 0.07 | 0.45 (0.28) | 0.00 [0.00, 0.00] |
| duel_fortress | gate | n/a [n/a, n/a] | n/a [n/a, n/a] | 1.00 | 0.000 | n/a | 1.00 (0.95) | n/a [n/a, n/a] |
| duel_fortress | life | n/a [n/a, n/a] | n/a [n/a, n/a] | 0.08 | 0.002 | n/a | 0.23 (0.23) | n/a [n/a, n/a] |
| duel_stubborn | ladder | 0.19 [0.15, 0.21] | 0.19 [0.15, 0.21] | 0.98 | 0.000 | 0.00 | 0.99 (0.93) | 0.00 [0.00, 0.00] |
| duel_stubborn | gate | 0.25 [0.24, 0.26] | 0.25 [0.24, 0.26] | 0.14 | 0.000 | 0.00 | 0.65 (0.53) | 0.00 [0.00, 0.00] |
| duel_stubborn | life | 0.54 [0.48, 0.58] | 0.54 [0.48, 0.58] | 0.02 | 0.027 | 0.06 | 0.15 (0.13) | 0.00 [0.00, 0.00] |
| duel_weak | ladder | 0.19 [0.16, 0.22] | 0.19 [0.16, 0.22] | 0.99 | 0.392 | 0.78 | 0.57 (0.53) | 0.07 [0.00, 0.54] |
| duel_weak | gate | n/a [n/a, n/a] | n/a [n/a, n/a] | 0.33 | 0.000 | n/a | 0.54 (0.35) | n/a [n/a, n/a] |
| duel_weak | life | 0.93 [0.59, 1.50] | 0.93 [0.59, 1.50] | 0.04 | 0.004 | n/a | 0.16 (0.15) | n/a [n/a, n/a] |
| farmer | ladder | 0.84 [0.76, 0.96] | 0.84 [0.76, 0.96] | 0.17 | 0.001 | 0.03 | 0.49 (0.45) | 0.00 [0.00, 0.00] |
| farmer | gate | 0.86 [0.67, 1.89] | 1.18 [0.68, 2.32] | 0.16 | 0.005 | 0.10 | 0.35 (0.33) | 0.00 [0.00, 1.08] |
| farmer | life | 0.84 [0.66, 1.02] | 0.77 [0.54, 1.07] | 0.02 | 0.010 | 0.14 | 0.09 (0.08) | 0.22 [0.00, 0.54] |
| farmer_noally | life | 0.84 [0.67, 1.01] | 0.77 [0.55, 1.08] | 0.02 | 0.010 | 0.14 | 0.09 (0.07) | 0.22 [0.00, 0.55] |
| fortress | ladder | 0.79 [0.48, 0.92] | 0.81 [0.44, 0.89] | 0.16 | 0.000 | 0.03 | 0.49 (0.44) | 0.00 [0.00, 0.00] |
| fortress | gate | 0.84 [0.68, 1.77] | 1.34 [0.68, 2.54] | 0.17 | 0.016 | 0.08 | 0.40 (0.38) | 0.00 [0.00, 0.81] |
| fortress | life | 0.84 [0.69, 1.03] | 0.76 [0.56, 1.10] | 0.02 | 0.012 | 0.16 | 0.08 (0.07) | 0.29 [0.00, 0.60] |
| fortress_noally | life | 0.84 [0.70, 1.02] | 0.76 [0.54, 1.08] | 0.02 | 0.010 | 0.15 | 0.08 (0.07) | 0.34 [0.00, 0.60] |
| multi | ladder | 0.66 [0.48, 0.81] | 0.83 [0.46, 1.36] | 0.18 | 0.001 | 0.03 | 0.44 (0.39) | 0.00 [0.00, 0.00] |
| multi | gate | 0.74 [0.57, 0.98] | 0.92 [0.66, 1.69] | 0.09 | 0.005 | 0.05 | 0.36 (0.34) | 0.00 [0.00, 0.00] |
| multi | life | 0.72 [0.56, 0.90] | 0.74 [0.53, 1.07] | 0.02 | 0.009 | 0.13 | 0.08 (0.07) | 0.00 [0.00, 0.47] |
| multi_noally | life | 0.71 [0.57, 0.89] | 0.74 [0.51, 1.07] | 0.02 | 0.010 | 0.15 | 0.09 (0.07) | 0.00 [0.00, 0.51] |
| multi_nospill | ladder | 0.66 [0.48, 0.81] | 0.83 [0.46, 1.36] | 0.18 | 0.001 | 0.03 | 0.44 (0.39) | 0.00 [0.00, 0.00] |
| multi_nospill | gate | 0.74 [0.57, 0.98] | 0.92 [0.66, 1.69] | 0.09 | 0.005 | 0.05 | 0.36 (0.34) | 0.00 [0.00, 0.00] |
| multi_nospill | life | 0.70 [0.57, 0.93] | 0.74 [0.52, 1.06] | 0.02 | 0.010 | 0.16 | 0.09 (0.08) | 0.00 [0.00, 0.48] |
| railway | ladder | 0.83 [0.78, 0.91] | 0.83 [0.78, 0.91] | 0.18 | 0.000 | 0.04 | 0.51 (0.48) | 0.00 [0.00, 0.00] |
| railway | gate | 0.83 [0.68, 1.93] | 1.27 [0.69, 2.31] | 0.17 | 0.004 | 0.09 | 0.38 (0.35) | 0.00 [0.00, 1.04] |
| railway | life | 0.81 [0.63, 0.94] | 0.76 [0.55, 1.09] | 0.04 | 0.009 | 0.17 | 0.13 (0.11) | 0.00 [0.00, 0.48] |
| railway_noally | life | 0.79 [0.61, 0.97] | 0.75 [0.52, 1.03] | 0.04 | 0.010 | 0.17 | 0.13 (0.11) | 0.00 [0.00, 0.52] |

## Refusals and guards: can a watcher call the day?

Refusal lag: days from a refused envoy to that camp's next fight on the target (within a year). Commonest lag: the share of lags within a day of the commonest one. Guards lag: days from the railway taking its guards off to the next train robbed.

| Scenario | Mode | Refusals a year | Followed by a fight | Lag median [p10, p90] | Commonest lag (share within 1) | Fight within a day | Guards lag median [p10, p90] (n, CV) |
|---|---|---|---|---|---|---|---|
| duel_adaptive | ladder | 0.02 | 1.00 | 10 [10, 10] | 10 (1.00) | 0.00 | n/a [n/a, n/a] (0, n/a) |
| duel_adaptive | gate | 0.89 | 1.00 | 3 [3, 3] | 3 (1.00) | 0.00 | n/a [n/a, n/a] (0, n/a) |
| duel_adaptive | life | 0.00 | 1.00 | 3 [2, 4] | 2 (0.50) | 0.00 | n/a [n/a, n/a] (0, n/a) |
| duel_fortress | ladder | 0.02 | 1.00 | 10 [10, 10] | 10 (1.00) | 0.00 | n/a [n/a, n/a] (0, n/a) |
| duel_fortress | life | 0.01 | 0.00 | n/a [n/a, n/a] | n/a (n/a) | n/a | n/a [n/a, n/a] (0, n/a) |
| duel_stubborn | ladder | 0.02 | 1.00 | 10 [10, 10] | 10 (1.00) | 0.00 | n/a [n/a, n/a] (0, n/a) |
| duel_stubborn | gate | 3.10 | 1.00 | 3 [3, 3] | 3 (1.00) | 0.00 | n/a [n/a, n/a] (0, n/a) |
| duel_stubborn | life | 0.49 | 1.00 | 4 [1, 14] | 3 (0.40) | 0.11 | n/a [n/a, n/a] (0, n/a) |
| duel_weak | ladder | 0.01 | 1.00 | 10 [10, 10] | 10 (0.97) | 0.00 | n/a [n/a, n/a] (0, n/a) |
| farmer | life | 0.14 | 0.19 | 4 [1, 54] | 3 (0.44) | 0.11 | n/a [n/a, n/a] (0, n/a) |
| farmer_noally | life | 0.15 | 0.15 | 3 [1, 37] | 3 (0.55) | 0.18 | n/a [n/a, n/a] (0, n/a) |
| fortress | ladder | 0.02 | 1.00 | 9 [9, 10] | 9 (1.00) | 0.00 | n/a [n/a, n/a] (0, n/a) |
| fortress | gate | 0.67 | 0.99 | 3 [3, 3] | 3 (0.99) | 0.00 | n/a [n/a, n/a] (0, n/a) |
| fortress | life | 0.37 | 0.28 | 5 [1, 63] | 3 (0.36) | 0.14 | n/a [n/a, n/a] (0, n/a) |
| fortress_noally | life | 0.37 | 0.30 | 4 [1, 65] | 3 (0.41) | 0.11 | n/a [n/a, n/a] (0, n/a) |
| multi | ladder | 0.06 | 0.64 | 9 [2, 37] | 9 (0.55) | 0.00 | n/a [n/a, n/a] (0, n/a) |
| multi | gate | 1.80 | 0.99 | 3 [3, 3] | 3 (0.98) | 0.00 | n/a [n/a, n/a] (0, n/a) |
| multi | life | 0.64 | 0.44 | 8 [2, 63] | 3 (0.26) | 0.07 | n/a [n/a, n/a] (0, n/a) |
| multi_noally | life | 0.64 | 0.46 | 8 [2, 67] | 3 (0.26) | 0.08 | n/a [n/a, n/a] (0, n/a) |
| multi_nospill | ladder | 0.06 | 0.64 | 9 [2, 37] | 9 (0.55) | 0.00 | n/a [n/a, n/a] (0, n/a) |
| multi_nospill | gate | 1.80 | 0.99 | 3 [3, 3] | 3 (0.98) | 0.00 | n/a [n/a, n/a] (0, n/a) |
| multi_nospill | life | 0.65 | 0.47 | 8 [2, 65] | 3 (0.26) | 0.08 | n/a [n/a, n/a] (0, n/a) |
| railway | life | 0.62 | 0.29 | 11 [2, 75] | 3 (0.25) | 0.05 | 28 [5, 74] (1371, 0.79) |
| railway_noally | life | 0.62 | 0.31 | 11 [2, 78] | 3 (0.24) | 0.07 | 27 [5, 74] (1354, 0.81) |

## Duels (game.py's setting: one goblin camp, one base, grievance 9 a day)

| Player | Mode | Wars a year, years 1-5 | Wars a year, last decade | Raiders killed a year | Tribute a year | Defence year 10 | Defence at the end |
|---|---|---|---|---|---|---|---|
| adaptive | ladder | 6.8 | 9.6 | 65 | 0 | 26 | 26 |
| adaptive | gate | 3.6 | 2.2 | 23 | 1 | 24 | 23 |
| adaptive | life | 0.1 | 0.0 | 0 | 158 | 12 | 12 |
| fortress | ladder | 5.5 | 9.6 | 65 | 0 | 80 | 80 |
| fortress | gate | 0.0 | 0.0 | 0 | 0 | 80 | 80 |
| fortress | life | 0.1 | 0.0 | 0 | 0 | 80 | 80 |
| stubborn | ladder | 23.2 | 23.7 | 45 | 0 | 6 | 6 |
| stubborn | gate | 10.2 | 10.4 | 20 | 0 | 6 | 6 |
| stubborn | life | 3.7 | 3.4 | 3 | 0 | 6 | 6 |
| weak | ladder | 6.0 | 14.7 | 28 | 138 | 6 | 6 |
| weak | gate | 0.0 | 0.0 | 0 | 329 | 6 | 6 |
| weak | life | 0.6 | 0.1 | 0 | 177 | 6 | 6 |

## What started the life-mode fights (share of fights)

A driver is any motive that was at least a quarter of the push toward the fight; a fight can have several. Loot only: greed for loot with no hunger, grudge, pride, glory or fear behind it.

| Scenario | Top motive | Drivers | Loot only | Occasion |
|---|---|---|---|---|
| alone | hunger 0.56, loot 0.41, glory 0.03, pride 0.01 | loot 0.63, hunger 0.63, glory 0.13, pride 0.02, preempt 0.00 | 0.23 | restless 0.83, caravan 0.14, provoked 0.02, news 0.01, allied 0.00 |
| alone_noally | hunger 0.55, loot 0.41, glory 0.03, pride 0.01 | loot 0.64, hunger 0.62, glory 0.13, pride 0.02, preempt 0.00 | 0.24 | restless 0.83, caravan 0.14, provoked 0.02, news 0.01 |
| duel_adaptive | loot 0.69, hunger 0.21, glory 0.06, revenge 0.03 | loot 0.93, glory 0.47, hunger 0.30, revenge 0.13 | 0.13 | restless 0.99, refused 0.01 |
| duel_fortress | loot 0.62, revenge 0.38 | loot 0.75, revenge 0.75, glory 0.06 | 0.25 | restless 1.00 |
| duel_stubborn | loot 0.77, revenge 0.19, pride 0.04, glory 0.00, hunger 0.00 | loot 0.95, revenge 0.69, pride 0.08, glory 0.04, hunger 0.00 | 0.23 | restless 0.95, refused 0.05 |
| duel_weak | loot 0.66, glory 0.20, revenge 0.08, hunger 0.06 | loot 0.85, glory 0.54, revenge 0.28, hunger 0.09 | 0.11 | restless 1.00 |
| farmer | hunger 0.44, loot 0.32, pride 0.21, glory 0.02 | loot 0.51, hunger 0.50, pride 0.22, glory 0.10, preempt 0.00, revenge 0.00 | 0.18 | restless 0.78, caravan 0.11, provoked 0.08, news 0.02, allied 0.00, refused 0.00 |
| farmer_noally | hunger 0.45, loot 0.32, pride 0.21, glory 0.02 | loot 0.51, hunger 0.50, pride 0.22, glory 0.10, preempt 0.00, revenge 0.00 | 0.19 | restless 0.79, caravan 0.11, provoked 0.08, news 0.02, refused 0.00 |
| fortress | hunger 0.53, loot 0.42, glory 0.03, pride 0.02, revenge 0.00 | loot 0.66, hunger 0.61, glory 0.13, pride 0.03, revenge 0.00, preempt 0.00 | 0.24 | restless 0.82, caravan 0.14, provoked 0.02, news 0.01, refused 0.00, allied 0.00 |
| fortress_noally | hunger 0.54, loot 0.42, glory 0.03, pride 0.02, revenge 0.00 | loot 0.66, hunger 0.61, glory 0.13, pride 0.02, revenge 0.00, preempt 0.00 | 0.24 | restless 0.82, caravan 0.14, provoked 0.02, news 0.01, refused 0.00 |
| multi | loot 0.53, hunger 0.33, glory 0.09, pride 0.05, revenge 0.00 | loot 0.73, hunger 0.39, glory 0.26, pride 0.07, revenge 0.01, preempt 0.00 | 0.29 | restless 0.81, caravan 0.13, provoked 0.02, news 0.02, refused 0.01, allied 0.00 |
| multi_noally | loot 0.53, hunger 0.32, glory 0.09, pride 0.06, revenge 0.00 | loot 0.74, hunger 0.38, glory 0.26, pride 0.08, revenge 0.01, preempt 0.00 | 0.29 | restless 0.80, caravan 0.13, news 0.03, provoked 0.03, refused 0.01 |
| multi_nospill | loot 0.51, hunger 0.34, glory 0.09, pride 0.06, revenge 0.00 | loot 0.73, hunger 0.40, glory 0.25, pride 0.07, revenge 0.01, preempt 0.00 | 0.29 | restless 0.81, caravan 0.13, provoked 0.02, news 0.02, refused 0.01, allied 0.00 |
| railway | loot 0.62, hunger 0.32, glory 0.04, revenge 0.01, pride 0.01 | loot 0.82, hunger 0.38, glory 0.15, revenge 0.07, pride 0.02, preempt 0.00 | 0.41 | restless 0.54, train 0.35, caravan 0.09, provoked 0.01, news 0.01, refused 0.00, allied 0.00 |
| railway_noally | loot 0.63, hunger 0.30, glory 0.05, revenge 0.01, pride 0.01, preempt 0.00 | loot 0.83, hunger 0.37, glory 0.16, revenge 0.07, pride 0.02, preempt 0.00 | 0.42 | restless 0.54, train 0.35, caravan 0.08, provoked 0.01, news 0.01, refused 0.01 |

## Fights by target (a world-year; in brackets, the share the attacker lost)

| Scenario | Mode | Camps | Villages | Players | Trains and caravans |
|---|---|---|---|---|---|
| alone | ladder | 10.16 (1.00) | 19.93 (0.00) | 0.00 | 0.00 |
| alone | gate | 2.07 (0.75) | 10.57 (0.00) | 0.00 | 0.00 |
| alone | life | 1.16 (0.12) | 5.19 (0.00) | 0.00 | 1.02 (0.00) |
| alone_noally | life | 1.16 (0.12) | 5.13 (0.01) | 0.00 | 1.03 (0.00) |
| duel_adaptive | ladder | 0.00 | 0.00 | 9.07 (0.99) | 0.00 |
| duel_adaptive | gate | 0.00 | 0.00 | 2.32 (0.76) | 0.00 |
| duel_adaptive | life | 0.00 | 0.00 | 0.06 (0.05) | 0.00 |
| duel_fortress | ladder | 0.00 | 0.00 | 8.83 (1.00) | 0.00 |
| duel_fortress | gate | 0.00 | 0.00 | 0.00 | 0.00 |
| duel_fortress | life | 0.00 | 0.00 | 0.01 (1.00) | 0.00 |
| duel_stubborn | ladder | 0.00 | 0.00 | 23.58 (0.00) | 0.00 |
| duel_stubborn | gate | 0.00 | 0.00 | 10.37 (0.00) | 0.00 |
| duel_stubborn | life | 0.00 | 0.00 | 3.40 (0.00) | 0.00 |
| duel_weak | ladder | 0.00 | 0.00 | 13.49 (0.00) | 0.00 |
| duel_weak | gate | 0.00 | 0.00 | 0.00 | 0.00 |
| duel_weak | life | 0.00 | 0.00 | 0.17 (0.00) | 0.00 |
| farmer | ladder | 10.07 (1.00) | 19.91 (0.00) | 0.13 (0.00) | 0.00 |
| farmer | gate | 2.12 (0.75) | 10.64 (0.00) | 0.18 (0.00) | 0.00 |
| farmer | life | 1.17 (0.12) | 5.18 (0.00) | 1.93 (0.01) | 1.05 (0.00) |
| farmer_noally | life | 1.19 (0.11) | 5.15 (0.01) | 1.92 (0.01) | 1.03 (0.00) |
| fortress | ladder | 7.18 (1.00) | 19.78 (0.00) | 2.74 (1.00) | 0.00 |
| fortress | gate | 1.64 (0.78) | 10.59 (0.00) | 0.79 (0.99) | 0.00 |
| fortress | life | 1.09 (0.14) | 4.99 (0.01) | 0.12 (0.96) | 1.00 (0.00) |
| fortress_noally | life | 1.11 (0.13) | 5.06 (0.01) | 0.13 (1.00) | 1.00 (0.00) |
| multi | ladder | 7.24 (1.00) | 17.31 (0.01) | 7.47 (0.98) | 0.00 |
| multi | gate | 0.64 (0.87) | 10.61 (0.00) | 3.35 (0.96) | 0.00 |
| multi | life | 0.58 (0.21) | 3.19 (0.01) | 0.47 (0.28) | 0.64 (0.00) |
| multi_noally | life | 0.58 (0.22) | 3.20 (0.01) | 0.47 (0.28) | 0.65 (0.00) |
| multi_nospill | ladder | 7.24 (1.00) | 17.31 (0.01) | 7.47 (0.98) | 0.00 |
| multi_nospill | gate | 0.64 (0.87) | 10.61 (0.00) | 3.35 (0.96) | 0.00 |
| multi_nospill | life | 0.60 (0.20) | 3.23 (0.01) | 0.43 (0.31) | 0.66 (0.00) |
| railway | ladder | 10.03 (1.00) | 19.84 (0.00) | 0.00 | 0.00 |
| railway | gate | 2.13 (0.75) | 10.64 (0.00) | 0.00 | 0.00 |
| railway | life | 0.78 (0.15) | 3.83 (0.01) | 0.16 (0.00) | 3.63 (0.11) |
| railway_noally | life | 0.75 (0.15) | 3.76 (0.01) | 0.17 (0.00) | 3.58 (0.11) |

## Each player's share (a year)

| Scenario | Mode | Player | Fights on him | Attackers killed there | Loot taken | Tribute paid | Loot and tribute as a share of his income |
|---|---|---|---|---|---|---|---|
| farmer | ladder | P:Ben | 0.13 | 0.0 | 6 | 0 | 0.01 |
| farmer | gate | P:Ben | 0.18 | 0.0 | 9 | 0 | 0.02 |
| farmer | life | P:Ben | 1.93 | 0.1 | 93 | 0 | 0.19 |
| farmer_noally | life | P:Ben | 1.92 | 0.1 | 92 | 0 | 0.19 |
| fortress | ladder | P:Ben | 2.74 | 39.4 | 0 | 0 | 0.00 |
| fortress | gate | P:Ben | 0.79 | 16.0 | 0 | 0 | 0.00 |
| fortress | life | P:Ben | 0.12 | 2.8 | 1 | 0 | 0.00 |
| fortress_noally | life | P:Ben | 0.13 | 2.9 | 0 | 0 | 0.00 |
| multi | ladder | P:Ash | 7.47 | 79.7 | 10 | 0 | 0.02 |
| multi | ladder | P:Bea | 0.00 | 0.0 | 0 | 1 | 0.00 |
| multi | gate | P:Ash | 3.34 | 48.0 | 15 | 0 | 0.03 |
| multi | gate | P:Bea | 0.00 | 0.0 | 0 | 0 | 0.00 |
| multi | life | P:Ash | 0.32 | 4.5 | 24 | 0 | 0.05 |
| multi | life | P:Bea | 0.14 | 0.1 | 25 | 271 | 0.62 |
| multi_noally | life | P:Ash | 0.33 | 4.6 | 25 | 0 | 0.05 |
| multi_noally | life | P:Bea | 0.14 | 0.1 | 25 | 272 | 0.62 |
| multi_nospill | ladder | P:Ash | 7.47 | 79.7 | 10 | 0 | 0.02 |
| multi_nospill | ladder | P:Bea | 0.00 | 0.0 | 0 | 1 | 0.00 |
| multi_nospill | gate | P:Ash | 3.34 | 48.0 | 15 | 0 | 0.03 |
| multi_nospill | gate | P:Bea | 0.00 | 0.0 | 0 | 0 | 0.00 |
| multi_nospill | life | P:Ash | 0.34 | 4.7 | 26 | 0 | 0.05 |
| multi_nospill | life | P:Bea | 0.09 | 0.1 | 16 | 267 | 0.59 |
| railway | ladder | P:rail | 0.00 | 0.0 | 0 | 0 | 0.00 |
| railway | gate | P:rail | 0.00 | 0.0 | 0 | 0 | 0.00 |
| railway | life | P:rail | 0.16 | 1.6 | 19 | 0 | 0.04 |
| railway_noally | life | P:rail | 0.17 | 1.6 | 20 | 0 | 0.04 |

## Alliances (life; section 2.16)

Each `*_noally` scenario is the same seeds with alliances off. Calls: the shares a sworn ally joined, refused or could not send (busy). Joint fights lost: the share the attackers lost. Ends: a year, by why the pact ended.

| Scenario | Offers a year | Sworn a year | Worlds with a pact | Pact days median (mean) | Calls a year (joined, refused, busy) | Joint fights a year (lost) | On players (lost) | On camps | Joint demands a year | Betrayals a year | Ends a year | Pairs a year |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| alone | 0.229 | 0.146 | 1.00 | 105 (148) | 0.077 (0.47, 0.48, 0.05) | 0.036 (0.01) | 0.000 (0.00) | 0.036 | 0.000 | 0.026 | lapsed 0.115, betrayed 0.026, refused 0.002 | G1-G2 vs R1 0.146 |
| farmer | 0.241 | 0.150 | 1.00 | 98 (134) | 0.057 (0.52, 0.45, 0.02) | 0.030 (0.01) | 0.000 (0.00) | 0.030 | 0.000 | 0.022 | lapsed 0.121, betrayed 0.022, refused 0.002 | G1-G2 vs R1 0.150 |
| fortress | 0.516 | 0.162 | 1.00 | 102 (148) | 0.085 (0.51, 0.47, 0.02) | 0.043 (0.05) | 0.007 (0.27) | 0.037 | 0.029 | 0.027 | lapsed 0.127, betrayed 0.027, refused 0.003, strained 0.001 | G1-G2 vs R1 0.156, G2-R1 vs P:Ben 0.004, G2-U1 vs P:Ben 0.002 |
| multi | 0.232 | 0.089 | 1.00 | 93 (140) | 0.041 (0.49, 0.49, 0.02) | 0.020 (0.08) | 0.004 (0.31) | 0.016 | 0.009 | 0.013 | lapsed 0.072, betrayed 0.013, refused 0.002, strained 0.001 | G1-G2 vs R1 0.087, G1-R1 vs P:Ash 0.002 |
| multi_nospill | 0.262 | 0.081 | 1.00 | 85 (137) | 0.033 (0.42, 0.56, 0.02) | 0.014 (0.00) | 0.002 (0.00) | 0.013 | 0.003 | 0.012 | lapsed 0.064, betrayed 0.012, refused 0.002 | G1-G2 vs R1 0.080, G1-R1 vs P:Ash 0.001 |
| railway | 0.183 | 0.121 | 1.00 | 104 (152) | 0.036 (0.32, 0.54, 0.14) | 0.012 (0.00) | 0.000 (0.00) | 0.012 | 0.000 | 0.021 | lapsed 0.094, betrayed 0.021, refused 0.001, strained 0.000 | G1-G2 vs R1 0.121 |

## Work (life): what the engine does a faction-year

| Scenario | Council sittings a faction-year | Sittings a year by culture | Relations weighed a sitting | Acts a faction-year | Rows touched a faction-day |
|---|---|---|---|---|---|
| alone | 9.5 | bandit 11.6, frostmaw 3.9, goblin 13.9, raider 10.0, umvuthana 9.4 | 3.9 | 4.9 | 0.113 |
| alone_noally | 9.4 | bandit 11.5, frostmaw 3.9, goblin 13.7, raider 9.9, umvuthana 9.5 | 3.9 | 4.9 | 0.112 |
| duel_adaptive | 16.3 | goblin 16.3 | 1.0 | 7.8 | 0.170 |
| duel_fortress | 18.4 | goblin 18.4 | 1.0 | 8.2 | 0.192 |
| duel_stubborn | 10.5 | goblin 10.5 | 1.0 | 5.8 | 0.145 |
| duel_weak | 15.9 | goblin 15.9 | 1.0 | 7.6 | 0.167 |
| farmer | 9.5 | bandit 11.6, frostmaw 3.5, goblin 13.8, raider 9.8, umvuthana 10.6 | 4.2 | 5.5 | 0.117 |
| farmer_noally | 9.5 | bandit 11.7, frostmaw 3.5, goblin 13.7, raider 9.9, umvuthana 10.6 | 4.2 | 5.5 | 0.116 |
| fortress | 10.1 | bandit 11.6, frostmaw 3.9, goblin 14.5, raider 11.3, umvuthana 10.8 | 4.5 | 5.5 | 0.119 |
| fortress_noally | 10.1 | bandit 11.6, frostmaw 3.9, goblin 14.5, raider 11.3, umvuthana 10.9 | 4.5 | 5.5 | 0.118 |
| multi | 10.5 | bandit 11.1, frostmaw 4.0, goblin 16.8, raider 11.8, umvuthana 9.2 | 5.0 | 5.7 | 0.119 |
| multi_noally | 10.5 | bandit 11.1, frostmaw 3.9, goblin 16.7, raider 11.8, umvuthana 9.4 | 5.0 | 5.7 | 0.118 |
| multi_nospill | 10.6 | bandit 11.0, frostmaw 3.9, goblin 17.1, raider 11.7, umvuthana 9.4 | 5.0 | 5.7 | 0.120 |
| railway | 14.2 | bandit 11.1, frostmaw 13.1, goblin 21.7, raider 9.5, umvuthana 9.4 | 4.2 | 6.1 | 0.163 |
| railway_noally | 14.2 | bandit 11.2, frostmaw 13.0, goblin 21.6, raider 9.4, umvuthana 9.5 | 4.2 | 6.1 | 0.162 |

## Unattended for centuries (life, alone)

| Seed | Fights a year by century | Camp-on-camp a year by century |
|---|---|---|
| 7000 | 7.4, 8.4, 6.9, 7.5, 7.0 | 1.13, 1.43, 1.15, 1.24, 1.12 |
| 7001 | 6.8, 7.7, 7.7, 7.8, 7.5 | 0.98, 1.21, 1.31, 1.31, 1.24 |
| 7002 | 6.9, 8.6, 7.7, 7.4, 7.1 | 1.05, 1.35, 1.30, 0.94, 1.03 |
| 7003 | 7.0, 6.4, 6.8, 6.8, 8.1 | 1.04, 0.88, 0.95, 1.03, 1.26 |
| 7004 | 6.5, 7.5, 7.9, 8.7, 7.0 | 0.90, 1.21, 1.28, 1.55, 1.09 |
| 7005 | 7.5, 6.7, 8.1, 8.4, 7.6 | 1.16, 1.02, 1.40, 1.27, 1.20 |
| 7006 | 7.0, 6.8, 7.7, 7.3, 8.3 | 1.15, 0.98, 1.12, 1.04, 1.37 |
| 7007 | 7.8, 7.5, 7.1, 8.1, 7.6 | 1.31, 1.10, 1.24, 1.30, 1.36 |
