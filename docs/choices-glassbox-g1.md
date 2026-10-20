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

## Integration and exit test, GB80 onward

| # | Choice | Why |
|---|---|---|

## For Ben

Nothing yet.
