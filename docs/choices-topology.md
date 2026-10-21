# Choices: the world topology layer (DECISIONS.md C31)

Ben, 2026-10-20: "make sure your sim works properly w/ the deeptime world wrapping system". Before
this layer every module assumed a flat, unbounded plane. A Deep Time world is a Petroff–Guyou
rectangle, x in [−b, b), z in [−b/2, b/2], whose east and west edges slide onto each other
(Toroidal World, period 2b) and whose north and south edges are each turned half a turn onto
themselves about (0, ∓b/2) (Deep Time's wrap engine). b is 6,656 at 16k and 13,056 at 32k.

The layer is `rtsim_space::topology` (Rust) and `com.minecraftindustrialcomplex.mic_rtsim.topology`
(Java). Every choice below can be undone; the record format (docs/boundary.md §11) is the only thing
other code depends on.

## The core

**TP1. Flat is the identity, bit for bit.** A frame nobody described is flat
(`Topologies::of`), and every module keeps its old formula on a flat frame (each helper branches on
`is_flat()`). So vanilla MIC worlds behave exactly as before, and every existing test kept its
results.

**TP2. Exact integer geometry.** Positions are block cells (i64); a cell stands for its centre, and
the doubled coordinates of `Iso::half` (a cell's centre is 2x + 1) make the turns and the shortest
displacement exact: `dist_sq` is a whole number of square blocks on every machine. Floats appear only
where the modules already used them: meeting times, closest approach, crossing fractions.

**TP3. The shortest displacement.** The cover's tiles within the domain's diameter are found once by
a breadth-first walk over the edges (which also checks the gluing: two ways round to one tile must
give one motion), sorted by their gap from the domain. The search stops at the first tile whose gap
exceeds the best distance found. Ties go to the first tile in that order, so every reader agrees.
Property tests: symmetry, the triangle inequality, never longer than the straight line, invariant
under every edge motion, a local isometry through every seam (a short step across any edge measures
its own length), and Deep Time's own `PetroffGuyou::fold_cell` agreeing cell for cell.

**TP4. Canonical at the door; keys canonical.** Every position the engine keeps is canonical: what
Java reports (players, puppets, dragons, camps, signals, habitat cells) is folded on entry, so a body
the frameworks have not folded yet is where it is on the world. Every key (region, province, grid
cell) is taken of a canonical position, so one place has one key (property test: canonical keys are
unique, and two places share one only at distance 0).

**TP5. Grids.** A grid whose cells fold whole onto cells ("aligned": the domain and every edge motion
are whole cells, as Deep Time's 64- and 128-block grids are, since b is a multiple of 256) keys and
neighbours exactly through `Grid::fold`, which also gives the quarter turns a heading takes. An
unaligned grid (512-block regions: b/2 = 3,328 is 6.5 regions) is keyed by where a cell's centre
block folds; index grids never rely on that, they fold rectangles (TP8).

**TP6. Session state, shared by pointer.** The topologies are not saved in the pages: Java sends a
TOPOLOGY record for every level at every session start, and a world's layout is fixed, so the value
is the one the pages were made under. The record is a sticky preamble record (written after the
dictionary, before any journal entry the first hand-over replays), so nothing replayed meets a flat
default. Modules hold one `Arc<Topologies>`, replaced whole when a record changes something; a step
costs a pointer comparison per module (`engine/topology.rs::sync`), and a module a load or a switch
replaced gets the value again at the next step.

**TP7. Legs are chart walks.** A leg starts at a canonical position and ends at the image of its
destination nearest the start (`Topology::toward`), which can lie past an edge. Its position at any
time is the straight line's point folded, a closed form per piece between seams
(`Topology::pieces`). `Path` keeps its fields; `x1, z1` may now lie past an edge. Herd routes,
marches and ship lanes are chart polylines the same way, restarted at the canonical position at each
decision so coordinates stay near the domain.

**TP8. Queries fold their rectangles.** A spatial query (an interest area, a prey or district lookup,
a lair search, a leg's booking per time window) folds its chart rectangle into canonical rectangles,
one per tile it overlaps (`Topology::fold_rect`), and looks up the cells of each. So everything near a
player across a seam is found, and a candidate's distance is then taken through the topology.

**TP9. Meetings and closest approach through seams.** Two canonical points closer than the domain's
narrow side are nearest through one of the images of the domain that touch it (nine for Guyou, three
for an x wrap, one for the plane). A meeting test cuts both walks at seams and compares each pair of
canonical pieces under those images (`Topology::first_within`); closest approach and road crossings
do the same (`closest_to`, `crossing`).

**TP10. Deep Time's layout is linked, not mirrored.** `deeptime-format` (LGPL-3.0-or-later, linked
into this AGPL crate) is a git dependency pinned at Deep Time 3bf532d1, as mic-native is pinned. Its
`ProjectionSpec` is the layout block of a world's `manifest.json` (FORMAT.md "The world's layout",
stable from format 0.5); `layout::from_projection` reads it into a `Topology`. Deep Time's edge
motions are data only (no `CellIsometry` type in Rust, and its `fold_cell` hard-codes the Guyou
gluing), so the group, folds and images are this crate's, general enough for Peirce, a torus and an
x wrap (all property-tested).

## Bodies

**TP11. The image on the player's side, and every target as its nearest image.** A dragon, a herd
group, a ship, pirates and a band seen on the march materialize at the image of their canonical
position nearest the player. A herd group keeps the motion from its route's chart to the bodies'
frame (`Embodied::chart`, saved), so its waypoints stay on that side; a ship's waypoints are moved by
the same motion. Toroidal World folds an entity that crosses its seam back into the domain at the end
of its tick, and Deep Time's wrap engine does the same, without an event either side would see (Deep
Time's coordinator, 2026-10-20). So the bridge takes every target it steers a body to as the target's
image nearest the body, each time: a group leader is re-steered when that image moves
(`RtsimSession.resteer`), a herd member's slot, lag and progress are taken through seams, a dragon's
target every tick, a ribbit band's stop. A position that jumps across a seam is a short step, never a
teleport or a duplicate: no Java check reads the raw jump, and the engine canonicalizes it.

**TP12. Edge_stop.** Until Deep Time's wrap engine installs the topology on a level (never, in any
shipped build), players, their vehicles and Sable ships stop one block inside the north and south
edges. Those seams still exist for distance: abstract travel crosses them (herds over a pole, legs,
lanes). Bodies never do (`Topology::embodied` is the world with them as walls):
- nothing materializes across one (`embodied_toward` is `None`: dragons count `seen_across_stop`,
  herds and ships note it and stay abstract);
- targets are routed round them (`embodied_route`) and kept `STOP_MARGIN` (PLACEHOLDER 4 blocks)
  inside them;
- a body is dematerialized before it would cross one: a herd group is despawned (reason 3) when its
  walk reaches the border within its hysteresis plus four margins, a ship departs before a lane that
  crosses one and is never materialized on such a lane, a watched band is recalled at the start of a
  segment that would cross one.
The Java side's nearest images for bodies never go through a stopped seam either.

**TP13. Cone points.** At the long edges' midpoints and the corners half a turn of planet is drawn in
a whole turn of world: walking round one takes half the distance and reverses a heading, and a nearby
point's nearest image can be two. Nothing is spawned or steered within `CONE_CLEARANCE` (PLACEHOLDER
48 blocks, three chunks: Deep Time says no heading or metric is consistent within a few chunks) of
one: flight ends are pushed radially out to it (`clear_of_cones`), herd members' spawn spots avoid it,
herd route cells and sea mask cells near one are not entered. Deep Time's bake keeps cone points in
open sea at least 7° from land, so this mostly concerns ships.

## Modules

**TP14. Herds.** The flow field is a chart grid around the herd's canonical position; each chart
cell reads the ground, forage, obstacles and claims of its canonical cell, so a route runs straight
over a seam and a pole (the heading flips there). Grazing takes each canonical cell of the footprint
once. Regions, prey queries and predation see across seams. Obstacles the plugin reports in the
bodies' frame are mapped back through the group's chart motion.

**TP15. Ship lanes.** The docks of one medium and frame are triangulated together with their images
in the tiles that touch the domain (a periodic Delaunay triangulation); an edge from a dock to an
image of another is a lane through the seam, its water path searched over a mask read at canonical
cells. A pair of docks keeps its shortest lane. Timetable fixes are canonical, with the lane chart's
motion. Lanes are built under the topology the network sees when it is built; a later change (only
edge_stop ending, which is not abstract geometry) does not rebuild them.

**TP16. Districts are local.** Nothing in rtsim-industry measures between districts. The one place
that assumed flat coordinates is the dragons' district index, which averaged a district's chunks for
its centre: it now averages them in the chart of the district's first chunk, so a district on a seam
has its centre on the seam, not on the far side of the world. Its queries fold their squares.

**TP17. HABITAT cells.** A cell reported past a slid edge (Toroidal World) is re-keyed to its
canonical cell. A cell past a turned edge is the folded copy Deep Time generates beyond a north or
south edge; it is a copy of a cell the sampler reports on its own, and is dropped
(`worldmodel.seam_copies`) rather than re-keyed with its sixteen points permuted.

**TP20. Settlements.** A village's position is keyed canonically when it joins. The nearest
search folds each ring of chart cells onto canonical cells and measures through seams, so a village
just across an edge is a neighbour and the edge rule links it by the short road. The table holds the
topology (not saved). A flat frame searches exactly as before (the property test against brute force
still holds).

**TP21. Roads, caravans, travellers and bands.** A road from `a` to `b` runs the short way
(`World::road_point`), so a caravan, a traveller sampled from a flow and a touring band are on it
where it runs, across a seam if it crosses one; caravan legs end at the nearest image; the road
index books a road the short way; `visible` and `encounters` look up folded regions; base stops,
haven delays, band tours and witnesses measure through seams. Phase 1's puppets: positions are
canonical on entry, a caravan or band appears at the image on the player's side and is steered to
its target's nearest image, and is despawned rather than steered across an edge_stop seam. The test
`a_caravan_road_crosses_the_x_seam_and_the_world_trades_as_on_the_plane` moves the synthetic
villages across the seam and finds the same roads, the same lengths and the same trade as on the
plane.

## The bridge

**TP18. Detection.** For every level: the first registered `TopologySource` that describes it
(`RtsimApi.registerTopology`; the core registers Deep Time's when Deep Time is loaded), else Toroidal
World's shape read by reflection (`ToroidalWorldApi.shapeOf`, x alone: an x wrap; x and z: a torus),
else flat inside the world border. Looked at again every 200 ticks (PLACEHOLDER) and at once after
`RtsimApi.topologyChanged()`; only a changed topology is sent again.

**TP19. Deep Time's layout through its internal route.** Deep Time has no public layout API (adding
one is a public interface, Ben's call, so Deep Time was not changed). `DeepTimeLayoutSource` reads by
reflection `GeologicChunkGenerator.terrain()` (a `WorldDataTerrain`), its `data().layout()` (a
`WorldLayout` record; null on a Lambert world, which Toroidal World's x wrap then describes), every
edge's `[quarterTurns, tx, tz]` and the border's `stops(name)`. Edge_stop holds while
`EdgeBorder.stopZ(level)` is a number. A reflective failure is logged once and the level falls back
to Toroidal World's x wrap.

## Tests

- `rtsim-space/tests/topology.rs`: property tests on a small Guyou world, the 16k one, an x wrap, a
  torus, Peirce's square and the plane: symmetry, the triangle inequality, invariance under every
  edge motion, a local isometry through every seam, unique canonical keys, transport through a seam
  and back is the identity, nearest images carry their motion, rectangles fold onto exactly their
  cells, legs are continuous through seams, Deep Time's `fold_cell` cell for cell; the layout block
  parsed from Deep Time's own type; edge_stop; cone points; grids; meetings through a seam.
- Scenarios on a 16k Guyou world: a herd crosses the x seam, and another goes north over the pole
  and comes out heading south, animals conserved (`rtsim-ecology/tests/topology.rs`); a caravan road
  crosses the x seam (`rtsim-world/tests/topology.rs`); sea lanes cross the x seam and the north seam
  (`rtsim-ships/tests/topology.rs`); a dragon leg crossing the seam enters a player's view exactly
  once, at the image on the player's side; a player beside the seam sees across it; a glued world's
  herds give the same hash for the same inputs, and after a save and restore; the leg index books a
  leg on both sides of a seam and meets it once (`rtsim-travel`); villages across a seam are
  neighbours (`rtsim-settle`); TOPOLOGY records through the real boundary (`rtsim-jni`).
- Java: `TopologyRecordsTest` (the record's bytes pinned against the Rust encoder, folds, nearest
  images, edge_stop, Toroidal World shapes, Deep Time layout parts); GameTests
  `TopologyGameTests` (a level's topology reaches the engine when it changes; a leader folded
  across a seam is re-steered, never captured or taken for a stale copy) on the native engine and
  the stub, with a fake Toroidal World probe: Toroidal World does not shape a GameTest server's
  superflat level.

## What waits on Deep Time

None of these is built in Deep Time, so nothing here waits on them to work; each is where this layer
would hook in:
- **A public layout API.** TP19 reads internals; a public accessor would replace the reflection.
- **A production topology install** (the wrap engine joining north and south outside the dev flag)
  and an event when it happens. The tracker then sends the layout without edge_stop within 200 ticks;
  an event would call `RtsimApi.topologyChanged()`.
- **Crossing events.** Toroidal World fires none; the wrap engine's `WrapCrossingEvent` fires before
  a north or south carry under the dev flag only. TP11 does not need them; a listener could re-steer
  at once instead of at the next pass.
- **Sable ships crossing north and south** in the engine (not built): ships keep to TP12.
