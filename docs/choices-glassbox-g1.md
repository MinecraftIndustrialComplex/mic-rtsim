# Choices: glass box phase G1 (2026-10-20)

**Branch `feat/glassbox-g1`.** Ben approved GLASSBOX.md and chose "Start building" (DECISIONS.md
GLASSBOX, G-B1 to G-B19, C32). G1 is the read-only compile and solve (G-B18, GLASSBOX.md §11.3): Java
extracts each Create district into a graph, the engine solves it for the floor L and the ceiling U per
good and checks it against one window of measurement; the replay still runs the C19 black box.
Reversible choices made to build it are numbered GB1 onward, one line of reason each. Every number
marked PLACEHOLDER is not measured; UNVERIFIED marks what no test has confirmed.

## Wire (G-B19)

| # | Choice | Why |
|---|---|---|
| GB1 | **The glass records live in their own module and class**: `rtsim-industry/src/glass/` (one `Obs::Glass` variant, one `Cmd::ProbeSet`), `GlassRecords.java` in `industry/glass/`, their own fixture file and `GlassWireContractTest`. The industry codec's writer and reader are shared (`pub(crate)`). | The i18 rework edits `IndRecords` and the industry contract test at the same time; separate files keep the merges clean, and one variant keeps the engine's match arms to one line each. |
| GB2 | **A node carries its rate table inline** (Rate24: level, recipe, batch, ticks as a rational), recipes are compile-wide records referred to by index, and a domain carries its levels (root speed as a rational, capacity in SU) and its members (ratio as a rational, impact). Stress quantities are f64, rates and speeds rationals. | One record per node keeps the count check simple. Create sums stress in floating point (`KineticNetwork.calculateStress`), so the overstress test repeats Create's own arithmetic; rates are exact rationals as §3.7 says. Recipes are shared because two machines of one kind share them. |
| GB3 | **IND_PROBE_SAMPLE (0x004D)** carries a probed buffer's contents, and it is **not counted** in the sweep's records. | §5.3 names IND_PROBE_SET but not how probe readings come back. Not counting them leaves the black box's sweep accounting (F5) untouched; a lost probe sample loses one localization point only. |
| GB4 | **The engine does not page the graph in G1**; Java resends a district's last compile every `glass.resendTicks` (PLACEHOLDER 1,200) with the RESEND flag, and the engine ignores a resend of a compile it already holds. | A dropped latest-wins record must be redone (§1.4), and a resend needs no new request kind. G1 is read only, so nothing is lost when a restart drops the graph until the next resend. G2, which replays from the graph, pages it with its district (A16). |
| GB5 | **IND_CHUNK_TICKING is journaled at 0x040B; IND_PROBE_SET is a command at 0x8406 without an op id.** | Ticking changes are events (I7 needs every one). Applying a probe set twice is harmless, so it needs no COMMAND_RESULT. |
| GB6 | **One frame per compile**, in IND_GRAPH_END: 0 for the level, else the Sable sub-level's id (the low 64 bits of its UUID's hash). Positions are the blocks' own coordinates. | Sable keeps a ship's blocks as ordinary block entities at fixed plot-grid positions in the same `ServerLevel` (`ServerLevelPlot`, 2,048 × 2,048 blocks per plot from x, z = 20,480,000); moving the ship changes only `SubLevel.logicalPose()`. So the plot coordinates are the ship's own frame and the extraction needs no transform (C32). A district never spans a ship and the world: they cannot touch. |
| GB7 | **IND_GRAPH_END carries the checksum window** (28 u32 `window`, 0 the engine's `glass.checksumWindowMin`), from Java's `glass.checksumWindowTicks`. | The engine's tunables are fixed per build; GameTests need short windows, and an operator can lengthen one district's without a rebuild. |
| GB8 | **A node flag PASS_THROUGH (64)** marks an in-place machine on a line: goods none of its recipes takes pass unchanged. | A press over a belt or a fan zone sees every item on the belt; without the flag each unmatched good would need a transfer recipe. |
| GB9 | **The graph's meaning is written down once**, docs/industry-boundary.md §9.3, before either half is built. | The extractor and the solver are built in parallel; one text keeps them agreeing on units, rate entries, heat, boilers, controllers and what is measured. |

## Solver and checksum (Rust), GB10 to GB29

| # | Choice | Why |
|---|---|---|

## Extraction and cycle evaluators (Java), GB30 to GB59

| # | Choice | Why |
|---|---|---|

## Corpus (GameTests), GB60 to GB79

| # | Choice | Why |
|---|---|---|
| GB60 | **The smoke test measures a least-squares slope of each good's per-tick store total over the window**, and passes an exact good within max(5%, 3 sigma of its chance outputs, 0.1 a minute), a bounded good inside [L, U] widened by 5% of the larger end, and fails a produced good measured at 0. | The slope uses every sample, so a store that moves in steps (a chute's 16, a press's 64) is not judged by where the window's two ends fall; sigma comes from each recipe's binomial variance (x 1.2 for the slope of a random walk), so chance outputs get their own width and exact ones stay at 5%. |
| GB61 | **One generated GameTest per factory** (`GlassCorpusGameTests`, `@GameTestGenerator`), each in its own batch `glass_<name>`, on the shared `yard` template, built in code, sky access on, max ticks warm-up + 7,200 + 400. `MIC_GLASS_ONLY=<name,...>` limits the run to some factories. | Own batches keep one factory's blocks and districts from touching another's; building in code keeps every block, speed and filter next to the rate it explains (no NBT templates to keep in step); the filter makes a one-factory run take two minutes. |
| GB62 | **`GlassCorpus.prepare` seeds every store and the anchor as player placed**, and `district` fails unless they are all in one district. `build` holds no time-dependent assertions. | Each factory must be exactly one district (I2 seeds); the exit test calls the same `build` and judges its own window. |
| GB63 | **Chutes and smart chutes move items, not hoppers; feeds are smart chutes with exact counts (1, 2 or 9) where the count shapes the rate.** | A hopper holds 5 stacks and moves on an 8-tick cooldown, a hidden buffer the rate would have to explain; a count-1 smart chute makes the input store a fine staircase (a plain chute's 16 put a 2,400-tick slope 4.4% off on the saw). |
| GB64 | **Windows: 2,400 ticks by default, 7,200 where a store moves in steps of 64 (press) or a chance output needs the width (brass mixing, precision mechanisms); 3,600 for the crafter grid.** | A 64-step staircase every 1,216 ticks is up to 24% off over 2,400 ticks and 2.3% over 7,200 (simulated). |
| GB65 | **Crushing, washing and blasting are two lines** (raw iron washed; raw gold blasted), each a crusher fed exactly 2 at a time at 50 RPM dropping onto a 16 RPM belt through a 7-block fan zone stopped by a full block. | Blasting destroys every washing output of a crushed ore and washing does nothing to ingots, so one line cannot show both. A count of 1 makes the first crushing stage 20 a tick at any speed (log2(1) = 0); 2 at 50 RPM gives a 99-tick cycle, slow enough for 210 ticks in the zone. |
| GB66 | **The brass mixer's burner is placed with a blaze and topped up as a player would (regular fuel, 10,000 ticks)**, so it stays KINDLED through warm-up and window; copper and zinc come in on two belts. | Fed burners are corpus 8's subject; a constant heat level keeps this factory about the mixer. One belt for both deadlocks once the basin's copper slot is full. |
| GB67 | **The press works over a depot, not a belt.** | On a belt the remainder of a held stack is not pressed again (`BeltPressingCallbacks` skips the current segment), so a belt press of whole stacks makes one sheet per stack. |
| GB68 | **The saw's filter is set to shafts, and both sides of the saw have an output chute.** | Andesite alloy also matches stonecutting recipes and the saw rotates its recipe index each start; the output side follows the sign of the speed. |
| GB69 | **Brass tunnels: two lines with different goods (wheat on PREFER_NEAREST, cornflower on ROUND_ROBIN), each splitting a 171.43 feed between a side millstone and a chute straight ahead.** The wheat millstone (109.09) is between half and all of the feed, the cornflower one (150) above half. | Different goods keep the lines apart in the district's net per good; the mill capacities are chosen so each rule gives a split the other would not (prefer nearest fills the side, round robin gives exactly half). |
| GB70 | **Crafter grid: the vanilla iron block from nine ingots, 64 RPM, a smart chute taking exactly 9 into the connected input, rows pointing east and the east column down into a chest; crafters placed after the ones that feed them.** | Deterministic with one input good; the four hand-offs make a long chain; one tick of error is 1% of the 99-tick cycle; a plain chute's 16 would leave partial fills; the cycle depends on block entity tick order, which placement order fixes. |
| GB71 | **Precision mechanisms are limited by their feed (one golden sheet per 64 ticks from an andesite funnel onto an 8 RPM belt), not by the deployers; the five rare scrap outputs are bounded [0, mean + 4 sigma] at rate 0.** | A side-fed loop fed above its deployers' capacity jams (measured: every loop segment full, nothing out for the window), so a deployer-limited loop is not a state Create holds; each scrap expects under 3 per window, which the produced-good check would fail at 0. |
| GB72 | For Ben: **GLASSBOX.md §3.3 says items already on a belt pass before a side insertion; Create's `canInsertAtFromSide` only looks at earlier insertions from the same side, so side feeds merge into through traffic, and an over-fed sequenced-assembly loop jams instead of settling at capacity (§3.4).** Recommended: model the insertion rule as Create has it and treat an over-fed loop as stalled; alternative: keep §3.4's capacity rule and mark such loops bounded. | Measured in corpus 6 (feeder at a sheet per 9 ticks: all 28 loop segments full by the end of warm-up, nothing out). The model's rule decides what the glass box predicts for every side-fed loop. |
| GB73 | **Package loop: a 40-tick pulse timer drives a redstone requester that orders exactly 2 clay; two chain conveyors with no frogport bound to a connection; frogport B's packages unpack into a millstone on clay.** | The packager in redstone mode packs up to 576 items a package and a receiving station unpacks one per 22 ticks, so neither makes a clean rate; clay to clay balls has no chance outputs, so both goods are exact. |
| GB74 | **Sheep farm: 16 sheep, sheared at the start, in one 1 x 1 pen on a grass block under a deployer with shears; `mobGriefing` is set false for the test and restored when it passes or fails; the expectation is binomial-style (variance 41.2 a minute) over the 7,200-tick window.** | Without the game rule the first sheep to eat turns the only grass to dirt and nothing regrows; starting sheared avoids a burst of first shearings; 16 keeps shearing well under the deployer's limit. |
| GB75 | For Ben: **the package loop's rate is set by its clock, not by the packager; the packager only shows it keeps up.** Recommended: keep it (the corpus checks the logistics chain end to end at a known rate); alternative: a second design where the packager's own 21-tick cycle is the bottleneck. | The packager's cycle is only measurable when nothing upstream is slower. |
| GB76 | For Ben: **the sheep check is weak: its 3-sigma band is about plus or minus 49% (about 53 shearings in 7,200 ticks), and three runs all came in high (+7%, +15%, +17%).** Recommended: accept it for G1, since mob farms are learned sources (G-B17); alternative: raise this factory's window to about 43,000 ticks for a 20% band. | The brief caps windows at 7,200 ticks; a longer bench would also settle whether the hand model runs about 10% low (UNVERIFIED). |

## Integration and exit test, GB80 onward

| # | Choice | Why |
|---|---|---|
| GB80 | **The stub engine's runs are checked offline by the native code**: `NativeEngine.glassOffline(records, district)` feeds the stub's received records (whole, in arrival order) to a fresh Rust industry engine and returns the same report the native engine gives (`rtsim-jni` `glass_offline`). With `-PskipJni` there are no natives, and the exit test logs that it cannot judge. | The stub simulates nothing; a second, Java solver would be a second model to keep in step. The engine is deterministic, so one code judges both runs. |
| GB81 | **The report reaches Java as a flat `double[]`** (`Report::to_f64s`, decoded by `GlassReport`), through `nGlassReport(handle, district)`, between steps. | One array per call, like the other inspection natives; f64 holds every count exactly and every rate as the engine computed it. |

## For Ben

Nothing yet.
