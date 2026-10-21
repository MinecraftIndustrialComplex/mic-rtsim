# The industry map (viewer redesign, 2026-10-21)

Ben's verdict on the first viewer: nothing on the map says what it is, the industry is invisible,
train lines are straight lines between stations, and the page looks generated. This note is the
design the redesign builds: what is drawn at which zoom, how it looks, and what a click says.

The test of it: open Alvedon cold and answer in seconds where the rail runs and which trains are on
it, where the factories are and what they make, how goods move between them and how much, and what
is idle or broken.

## 1. Sources

| Source | Read by | What it gives |
|---|---|---|
| `data/create_tracks.dat` (Create 6 `RailGraphs`, `Trains`) | mic-viewer-create `rail` | track as Create draws it (straight edges, bezier turns from each turn's `Starts`/`Axes`), stations with names and postbox addresses, signals with state, trains with every carriage's bogey positions, speed, schedule, navigation target and cargo |
| `region/r.X.Z.mca` block entities | mic-viewer-create `blocks` | chain conveyors and their connections and packages, frogports and their addresses, packagers, postboxes, belts (controller, length, index), vaults and tanks with contents, processing machines, and every kinetic block's network (stress, capacity, speed) |
| rtsim's save (industry pages) | rtsim-view | districts with their chunks, state and inspection; the sim's train lines (trips, cargo, travel time) and booked legs |
| Surveyor summaries, HABITAT | existing | the base map |

Region files are read **only where something asks**: the chunks of rtsim's districts and the chunks
under Create stations when the view opens (in the background, at SCHED_IDLE like the rest of the
viewer), and the chunks of the viewport whenever the view is close enough to draw machines (below 8
blocks a pixel), nearest the centre first, a bounded number per look. A chunk is read once and its
block entities kept as small records. Nothing reads a whole world up front, nothing holds a file open
between reads, and nothing writes. A file a live server may be writing is copied before it is read.

## 2. Layers per zoom

Zoom is blocks per CSS pixel (bpp). "Region" is the whole map of a city, "site" is one factory.

| Layer | Group | Over 16 (region) | 2 to 16 (town) | 0.5 to 2 (site) | Under 0.5 (block) |
|---|---|---|---|---|---|
| Track (`rail.track`) | Rail | network lines by graph | lines with a pale casing | lines, wider | two rails on sleepers |
| Stations (`rail.stations`) | Rail | named stations as rings | capsules along the track, names | same, all names | same |
| Signals (`rail.signals`) | Rail | | | bar across the track, state colour | same |
| Trains (`rail.trains`) | Rail | chevrons pointing the way they go | carriages as boxes, the leading one with a nose | same, names | same |
| Train traffic (`industry.lines`) | Rail | the sim's lines routed along the track, width by trips a day | same, with labels | | |
| Districts (`industry.districts`) | Factories | outlines of their chunks | outlines, names | outlines | outlines |
| Machines (`fact.machines`) | Factories | | | symbols by kind, state | same |
| Storage (`fact.storage`) | Factories | | | vaults and tanks, fill | same |
| Chain conveyors (`logi.chains`) | Logistics | | routes | routes, pulleys, packages on them | same |
| Belts (`logi.belts`) | Logistics | | | belts with direction | same |
| Package addresses (`logi.ports`) | Logistics | | | frogports, packagers, postboxes, addresses | same |

The sim's own station and train layers (`industry.stations`, `industry.trains`) stay, off by default
when Create's rail is in the save, since Create's are the same stations and the real trains.

## 3. Symbology

Cartographic and restrained, in the spirit of OpenRailwayMap and a transit diagram:

- **Base map muted.** Terrain is desaturated and lightened (light theme) or darkened (dark theme),
  so the industry reads over it. No translucent boxes over the map.
- **Ink, not colour, for structure.** Labels, stations, trains and machines are drawn in the map's
  ink (near black on light, warm off-white on dark) with a halo of the paper colour.
- **Colour carries one meaning per layer.** Track by graph (a fixed eight-colour cartographic
  palette by network size, small yards grey); state in red (overstressed, derailed, broken), amber
  (learning, waiting, packages) and grey (stopped, idle, suspended). Logistics is amber; districts
  teal.
- **Track** follows Create's own curves: each turn is sampled from the same cubic bezier Create
  builds (`BezierConnection.determineHandles`). Close up it becomes two rails on sleepers.
- **Stations** are white capsules with an ink border, turned along the track; far out, white rings
  with names (a station still called "Track Station" is not labelled until close).
- **Trains** are their carriages: a box per carriage from bogey to bogey, the leading carriage with a
  pointed nose in the direction of travel. A stopped train is grey, a derailed one red. Far out, a
  chevron.
- **Chain conveyors** are chain lines (short dashes) between pulleys, with the packages on them as
  amber dots. **Belts** are dark bands with chevrons the way they run.
- **Machines**: circle for processing, square for storage, triangle for power, diamond for movers;
  hollow when stopped, a red ring when the network is overstressed.
- **Districts**: the outline of their chunks, never a bounding box; solid when running, dashed when
  learning, dotted grey when suspended or out of season, red when damaged. Named in italics.
- **Traffic**: a soft band under the track along the route the trains take, its width by trips a day,
  labelled with trips a day and the main cargo.
- **Labels** in sentence case, never spaced capitals; collisions resolved by priority.

The page draws a key on the map naming every symbol on screen at the current zoom, built from the
same drawing code as the map.

## 4. Cards

Every card says real things, never ids alone:

- **Station**: name, network, postbox addresses, trains standing there, trains whose schedule calls
  there, traffic to each other station (trips a day, cargo), its district.
- **Train**: name, what it is doing (moving at N blocks a second towards a station, waiting at a
  station, derailed), carriages and length, schedule with the current stop marked, cargo by item
  and fluid.
- **Track**: the network's length, stations and trains.
- **Signal**: state and type.
- **District**: rtsim's inspection (state and why, rates, replay) plus what the census found in its
  chunks: machines by kind, storage and contents, belts, chain length, package addresses, kinetic
  networks and their stress.
- **Chain conveyor, belt, frogport, packager, machine**: what it is, its address or contents, speed,
  its network's stress and capacity.

## 5. API additions (mic-viewer)

All optional and additive (docs/api.md in mic-viewer):

- `point`: `a` (heading, radians, for turned glyphs), `st` (state: `bad`, `warn`, `off`); glyphs
  `station`, `signal`, `port`.
- `path`: `st` (line style: `rail`, `chain`, `belt`, `flow`, `dash`), `l` (label), `arrow`, `n`
  (things on it, drawn along it).
- `area`: `rings` (outline polygons) and `st`.
- a new kind `train`: `cars` (each `[x0, z0, x1, z1]`, front first), `l`, `st`.
- `style.line` and `style.label_bpp` on a layer; `line` on a legend entry.
- paths are simplified to the asked zoom on the server.
- `Backend::look` (the view asked for, so a backend can read what it shows) and `/api/find`
  (names to places, for the search box).

## 6. The glass box (G2)

G2 draws a compiled district's graph (VIEWER.md §7, row kinds 16 and 17). It needs nothing new from
the page: its nodes are `point` marks with the machine glyphs and states above, its edges `path`
marks with `st: "flow"`, `arrow`, a width from the rate and a label with the rate and its unit
(items a minute). rtsim-view's `glass` module takes nodes and edges and draws them; G2 feeds it.
