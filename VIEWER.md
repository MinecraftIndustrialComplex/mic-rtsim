# Viewer: a god view of the world, and the headless engine host

**Status:** a design for Ben to approve before any code is written (2026-10-20). It answers VW1 to VW4
in DECISIONS.md. Every choice that is Ben's is in §17, "Proposals for Ben", recommendation first.
Deep Time's coordinator reviews §3.2 and §14, which list what Deep Time would change and expose.

**Numbers.** A measured value names its source ("overnight" is docs/overnight-2026-10-20.md, measured
on the laptop while other sessions shared it). A derived value shows its arithmetic. Anything else is
**TUNABLE** (a setting with a starting value), **ESTIMATE** (a reasoned guess) or **UNVERIFIED** (a
fact not yet checked).

**In brief.**
- One shared viewer, **mic-viewer** (MPL-2.0): Deep Time's canvas slippy map and its HTTP server
  pattern, generalized so a page is a projection plus tile sources plus layer sources. No npm, no build
  step, nothing from the network.
- For rtsim it runs as a **sidecar process** (`rtsim-cli view <world>`) reached by an SSH port forward.
  It reads the engine's saved pages, a compact **view tap** the engine writes only while someone is
  watching and only in the engine's slack, and an **event archive**. The server thread never does
  viewer work; with nobody watching, the engine's only viewer work is a lease check every 10 s, plus
  the archive's recording (§5.5, §10).
- Every module draws through **layer descriptors** and self-describing parts made of a closed set of
  generic mark kinds, so a module or plugin adds a layer without the core, the sidecar's reader or the
  page knowing it (D4). The server aggregates by zoom
  on the detail ladder, so a browser draws at most a budget of marks whatever the world holds.
- **The event archive** records raids, wars, migrations and trade shifts in 32-byte rows with coarse
  positions, compacted in three tiers under a disk budget, so an op can scrub back to last week.
- **The headless host** (`rtsim-cli run`) loads a copy of a world's rtsim pages and runs the engine
  at any speed with the same viewer attached. Without Minecraft it has no chunk sampling, no puppets
  and no live Create, so only what the engine already learned or compiled replays (§12).

---

## 1. Walkthrough: what an op does

### 1.1 A live server that is not a Deep Time world

Ben's MIC server is a normal generated world (SRV1 rehearses rtsim on a copy of it first). The server
has rtsim installed and is running. Ben is on his laptop.

1. **Forward a port.** `ssh -L 8740:127.0.0.1:8740 ben@<server>`. The viewer listens on the server's
   loopback only; the forward is the only way in, as Deep Time's Azure test VM is reached (Ben, of that
   VM: "not putting it on the tailnet").
   Port 8740 is TUNABLE; Deep Time's workbench keeps 8642.
2. **Start the sidecar** in that SSH session: `rtsim-cli view --live /path/to/world` (a binary built
   from the mic-rtsim flake at the same commit as the mod on the server, copied there once). It prints
   what it found, for example:
   ```
   world: overworld flat (no wrap); also the_nether, the_end
   pages: generation <g>, saved at game day <d> <hh:mm>, <m> min ago
   live: the view tap runs while a page is open (lease: rtsim/view.lease)
   base map: Surveyor (<n> regions), HABITAT underneath (<k> cells)
   archive: <a> MiB of 256 MiB, game days <d0> to <d>
   serving http://127.0.0.1:8740/ (loopback only; Ctrl-C to stop)
   ```
   The sidecar runs at idle priority, below the engine's own nice-19 pool (on Linux `SCHED_IDLE` and the
   idle IO class), and writes nothing into the world folder except, with `--live`, one lease file
   (§5.2). Without `--live` it is the save source and writes nothing at all.
3. **Open `http://localhost:8740/`.** The page shows the overworld as far as it has been explored:
   Surveyor's per-block terrain where Surveyor recorded it, HABITAT's 64-block cells elsewhere. The top
   bar shows the game day and hour, "live", how far behind the tap is (what is on screen within a tap
   period, 2 s, TUNABLE; the rest within the measured cycle, §5.2), and a dimension switch. The left
   panel groups the layers by module: Settlements, Trade, Travel, Dragons, Herds, Ships, Conflict,
   Industry, Players.
4. **Zoomed out** (a whole continent on screen): villages as dots sized by population and coloured by
   culture; roads as lines whose width is the goods in transit, tinted red where the raid hazard is
   high; districts as squares coloured by their state (running, learning, stale, damaged); lairs;
   herds as soft discs labelled with their head count; faction territories as circles. Where more than
   the budget would land on screen (2,000 marks, TUNABLE), the server returns clusters with counts
   instead.
5. **Zoom in on a road.** Caravans appear and move. Most are the flow's sampled travellers (A13),
   derived purely from the flow (`World::travellers`), the same ones a player standing there would
   meet; the few rows (a robbed caravan, a promoted one) carry a ring. Live, the samples come from the
   regions the open pages name in the lease (the focus, §5.2), so they appear a tap period or two
   after zooming in. Messengers and touring bands are
   drawn the same way.
6. **Click a caravan.** A card opens: home village, outbound or returning, the leg's start and end and
   how far along it is, members (trader, llamas), cargo by good, purse, the news it carries and how
   old that news is. A "follow" button keeps it centred.
7. **Scrub back to last week's goblin raid.** The timeline strip at the bottom shows event counts per
   game day, with the wall-clock date under the game days (the server is not always running, so the
   two differ). Filtering to Conflict shows a spike labelled Raid. Clicking it moves the map to that
   moment: the war band's route from the camp to the "North Smelter" district (its legs, from the
   archive), the raid's row (units, loot, blocks broken, applied when the district next loaded), and
   the feud's escalation steps before it (Notice, Envoy, Scouts, Raid). Positions in the past are as
   coarse as the archive keeps them (§10).
8. **Back to Live; open the district.** The district is drawn as the box of its chunks with its
   stations and postboxes; train lines run to other districts with trips, cargo per trip and travel
   time. Its card is the engine's own inspection of the district (`Engine::inspect`, what
   `/rtsim district ledger` reports to an op): state and why, learned rates per hour, what the replay
   made and used since it unloaded, whether it is running or what stopped it, the last reconcile, the
   attacks. Once the
   glass box exists (GLASSBOX.md G1), a deep zoom draws its compiled graph: stores, machines and movers
   at their block positions, edges with rates.
9. **Close the tab.** The page's event stream closes (a tab left hidden closes it after 60 s, §11),
   the sidecar stops renewing the lease, and the engine's view tap stops within 40 s (§5.2). Ctrl-C
   ends the sidecar. Nothing of the viewer is left running on the server.

SRV1's rehearsal is the same with the server on the laptop: no SSH, `rtsim-cli view` on the local
copy. A singleplayer world works the same way while the game runs, on the client's save folder.

### 1.2 A Deep Time world

The same commands. The sidecar finds the world's Deep Time data folder (the one Deep Time's data
reference names inside the world, holding `manifest.json`; `WorldDataBinder` resolves it). The base
map is the bake's satellite, elevation and water tiles, rendered by Deep Time's own renderer through
its plug-in (§3.2), and the readout under the cursor shows latitude and longitude. The map wraps east
and west. A caravan whose road crosses the north edge is drawn as two pieces, one on each side of the
half-turn seam (§8). The map is the same core as Deep Time's workbench, so the look and the controls
are the workbench's.

### 1.3 The headless host on the laptop

A copy of the world sits on the laptop (SRV1: `rtsim-cli export` on the server writes a consistent
copy of the rtsim pages, §12.1, which is then copied over SSH and kept read only).

1. `rtsim-cli run ~/mic-copies/live-2026-10-20/world --fork ~/rtsim-runs/live-copy --speed max
   --until +3650d --view 127.0.0.1:8741`
2. It copies the world's `rtsim/` folder into the fork and never writes into the world. It starts
   mic_native in its own process, loads the pages, and runs the engine as a server with nobody online
   would, as fast as the CPU allows, printing game days per second.
3. `http://localhost:8741/` shows the same viewer, live, as ten game years pass: markets settle or
   swing, dragons hunt, herds migrate, factions escalate against districts that pollute, districts
   replay their learned models.
4. Ctrl-C stops it. The fork holds the pages and the archive of the run, which `rtsim-cli view` can
   open later. A fork is never loaded back into Minecraft (its journal no longer matches, A9).

### 1.4 What players never see

The viewer is a god view for ops and developers (VW1). I18 holds for players: nothing the viewer knows
reaches the game. There is no in-game command that opens it, no chat link, no item, no map marker, and
nothing flows from the viewer into the engine. Only someone with a shell on the server can reach it.

---

## 2. Reuse table and custom-roll ledger

### 2.1 Reuse

| Need | From | What is reused | Status |
|---|---|---|---|
| Slippy map | Deep Time `crates/deeptime-viewer/assets/worldmap.js` (597 lines) | canvas 2D over 256 px XYZ tiles; pan, wheel zoom anchored at the cursor, pinch, double-click, keys; east-west wrap; tile LRU of 420 with 8 in flight and ancestor fallback up to 5 levels; deep links | built, moves (VW3) |
| HTTP server | Deep Time `crates/deeptime-cli/src/watch/http.rs` (777 lines) | std `TcpListener`, one accept thread, a thread per connection (64 at most), 16 KiB headers, 1 MiB bodies, keep-alive with timeouts, SSE (`stream_events`: `retry: 2000`, a ping every 15 s), `tailnet_or_loopback`, `post_refusal` | built, pattern moves (VW3) |
| Tile disk cache | Deep Time `watch/tiles.rs` (617 lines) | versioned folders, LRU cap (384 MiB per run), atomic tmp-and-rename writes, immutable cache headers | built, cache part moves |
| PNG encoding | Deep Time `deeptime-viewer` `encode_png` | `png` crate at Fast compression | built |
| Projection | Deep Time `projection.js` (459 lines), `DTProj.world(block, C)` | `PetroffGuyou` and `LambertWorld` objects: `zRange`, `wrapX`, `canonical`, `latLon`, `blockOf`; `fold` on `PetroffGuyou` only | built, stays in Deep Time as the plug-in |
| Deep Time base tiles | Deep Time `watch/worldmap.rs` `BakeMap::open`, `render`, `sample` (1,924 lines) | satellite, elevation, water and overlay layers; the sample readout | built, stays in Deep Time as the plug-in |
| Deep Time history | `deeptime-format` `EventLog::load`, `between` (EVENTS.md) | pre-play history (writer 2, `civ:history`, X2) for the timeline on a Deep Time world | built; no `civ:history` writer yet |
| Seams | rtsim-space `topology` (branch feat/topology) | `canonical`, `fold_rect` (a viewport folds into canonical rectangles), `pieces` (a leg cut at seams), `cones` | being built now |
| Publishing state | mic_native `SnapshotCell` ("the one way a service shares changing data"), already used by rtsim-jni's `Inspect` | lock-free publish of an immutable snapshot | built |
| Pages and codec | rtsim-kernel `page.rs`, `codec.rs` | segments, log, manifest, LZ4 + CRC32, incremental commits; the view tap and the archive reuse them | built; a read-only open is new (§5.3) |
| Pure reads | rtsim-kernel `LazyModel::view`, `Lazy::view_without_catch_up`; `World::market` | projection without committing | built |
| Pure positions | `Path::at`, `World::traveller_pos`, `World::caravan_place`, `World::band_view`, `Dragon::position`, `Herds::position`, `footprint`, `count`, `Ships::position` (`Service::fix`), `Conflict::party_pos`, `Conflict::feud_view` | where everything is at time t, as a closed form | built |
| Sampling without promoting | `World::travellers(edge, dir)`, herds' `members::member` | the travellers on a road, the animals of a herd, derived and never taken | built |
| Event rows | rtsim-knowledge `ReportRow` (32 bytes: who, whom, kind, severity, flags, 48-bit tick, frame, 24-bit x and z, y) | the archive's row layout | built |
| Module loaders | `World::load`, `Dragons::load`, `Herds::load`, `Ships::load`, `Conflict::load`, industry and space `persist.rs` | decode a save without the JVM | built; the glue in rtsim-jni is private (§12) |
| Fake Java | rtsim-jni `tests/common/mod.rs` (1,268 lines) with `mic_native_host::start` and `register_in_process` | drives the real engine on the real host in one process | built, for tests |
| Base map, non Deep Time | Surveyor 1.2.4 (in the pack, both sides): `<world>/data/surveyor/c.<rx>.<rz>.dat`; rtsim HABITAT (`WorldModel::area`, `habitat_at`) | per-block terrain summaries with colour palettes; 64-block cells | built (Surveyor's layout read from its jar and checked against a world's region files, §9) |
| Low priority | mic_native `platform::lower_current_thread` (nice 19, `THREAD_PRIORITY_LOWEST`) | the pattern; the sidecar goes lower (§5.5) | built |
| JSON | `serde_json` (already in rtsim-cli and Deep Time) | API bodies | built |

### 2.2 Custom-roll ledger

Sizes compare with the code that exists: worldmap.js is 597 lines, http.rs 777, tiles.rs 617, page.rs
2,697. Line counts are ESTIMATES.

| Piece | Repo | Size | Notes |
|---|---|---|---|
| Map core: worldmap.js made generic (projection, tile and host interfaces; flat and x-wrap projections; a square tile pyramid beside Deep Time's 2:1 one) | mic-viewer | M (about 250 lines changed) | §3.1 |
| Mark renderer: point, leg, path, edge, area, cell, cluster, label; hit testing; east-west copies; leg animation | mic-viewer | L (about 800) | §4.3 |
| Part format: generic rows (a mark, its region, card fields), the writer and the one reader | mic-viewer | M (about 500) | §4.3 |
| Timeline strip: histogram, scrubber, filters, live and past | mic-viewer | M (about 400) | §10.6 |
| Card renderer: fields, small series, links to other things | mic-viewer | S (about 200) | §4.4 |
| Page shell, layer panel, styles | mic-viewer | S (about 300) | |
| Server crate: routes as a trait, Host check on every request, loopback default, SSE writer, static assets, CSP, limits | mic-viewer | M (about 900, mostly moved) | §13 |
| Tile cache and `TileSource` trait | mic-viewer | S (moved) | |
| API types: world, layer descriptor, marks, card, events, histogram | mic-viewer | S (about 400) | §4 |
| Checks: node syntax and pure-function tests of the map core; a fixture page screenshot | mic-viewer | S | §16 |
| `PageStore::open_read_only` with retry | rtsim-kernel | S (about 150) | §5.3 |
| Peeks: `LazyTable::peek`, `PagedTable::peek`, industry `inspect_peek` and `stock_peek` on `&self` | rtsim | S to M | §5.3 |
| `rtsim-view` crate: `ViewSource` and `DetailSource` traits, part keys (static by region, movers by id), region counts, the evicted-page count cache, binning and the mark budget | rtsim | M (about 900) | §4, §6 |
| Layers and cards for phase 1, dragons, herds, ships, conflict, industry, HABITAT, players | rtsim | L (about 2,400 in all) | §7 |
| Messenger positions (`messengers_on(edge, at)`); a district's box and centre kept at detection; public readers for industry's stations and ships' `pirate_pos` (both private today) | rtsim | S | §7 |
| View tap: lease and focus, slack gate, encoding under a budget into a latest-wins mailbox, one commit per period on the IO lane, epochs, resident set | rtsim | M (about 600) | §5.2 |
| Event archive: writer, flush with saves, rollback cut, tiers, compaction, reader, histogram index | rtsim | L (about 1,500) | §10 |
| Archive hooks at existing event points in each module | rtsim | M (about 400) | §10.1 |
| Save loading as a library (`load_world(dir)`), out of rtsim-jni's private glue, decoding a page at a time | rtsim | M (about 450) | §4.1 |
| `rtsim-cli view` (save, live and host sources; lease tied to open pages, renewed from a normal-priority thread; idle CPU and IO priority) | rtsim | M (about 550) | §4.1 |
| `rtsim-cli export` (a consistent copy of the pages through the read-only open) | rtsim | S (about 100) | §12.1 |
| `rtsim-cli run`, the headless host (fork, driver, session preamble, command answers, strides, save interval, pacing) | rtsim | L (about 1,200) | §12 |
| Base map from Surveyor (gzip NBT reader, palettes, shading) | rtsim or mic-viewer | M (about 700) | §9 |
| Base map from HABITAT cells | rtsim | S (about 300) | §9 |
| Benches and tests: archive rates, tap cost, no-write and determinism checks | rtsim | M | §16 |
| Glass-box graph view (after GLASSBOX G1) | rtsim | M | §7 |

---

## 3. The pieces and where they run

```
 laptop                         server machine
 ------                         -------------------------------------------------------------
 browser  <-- ssh -L 8740 -->   rtsim-cli view  (sidecar, idle priority)    Minecraft server JVM
 (plain JS from                   mic-viewer core: HTTP, SSE, tiles,          bridge (Java): no viewer code
  mic-viewer)                     static assets                               rtsim-jni engine (mic_native):
                                  rtsim-view: layers, cards, aggregation        step on background pool
                                  readers (read only):                          step tail: view tap (only
                                    <world>/rtsim/      sim pages                 while leased, in slack)
                                    <world>/rtsim/view/ view tap pages  <------  IO lane writes view pages
                                    <world>/rtsim/archive/              <------  IO lane flushes archive
                                    <world>/data/surveyor/  base map               with each save
                                    Deep Time bake (plug-in)
                                  writes: <world>/rtsim/view.lease only ------> engine stats it every 10 s
                                          (with --live; nothing without it)
```

### 3.1 The shared repo: mic-viewer (MPL-2.0)

A new private repo `MinecraftIndustrialComplex/mic-viewer` (name: VWD-2), MPL-2.0 as the host and the
shared crates are (A7). Deep Time's code is LGPL-3.0-or-later, so the moved files need Ben's
relicensing as their copyright holder, recorded in a NOTICE.md as mic-native's was (A18).

- **`web/`**, plain ES modules served from the binary, no build step:
  - `map.js`: worldmap.js made generic. It works in block coordinates (as worldmap.js already does:
    `setView(cx, cz, bpp)`) and takes a projection object, tile sources, layer sources and a host
    object (§14 lists what replaces `window.DTWB`).
  - `flat.js`: the projections the core owns: flat (vanilla), and x-wrap (Toroidal World alone).
  - `marks.js`, `timeline.js`, `card.js`, `viewer.html`, `viewer.css`.
- **`crates/mic-viewer`**, one Rust library:
  - `http`: the server, from http.rs, with routes registered as a trait instead of a hard-coded
    `get()`; `tiles::route` and `sections::route` already have that shape (functions of the path and
    query that return `Option<Answer>` and `Option<Reply>`).
  - `sse`: the event-stream writer from `stream_events`.
  - `tiles`: the disk cache and a `TileSource` trait.
  - `api`: the read-only API's types and JSON (§4), and the part format: generic rows and their one
    reader (§4.3).
  - `assets`: the web files through `include_str!`, as deeptime-viewer does now.
- **Consumers** pin it by git commit, as mic-native and deeptime-format are pinned. Each project builds
  its own binary from the core plus its own sources: Deep Time's `deeptime watch`, rtsim's
  `rtsim-cli view` and `run`. The core knows neither project.

### 3.2 Deep Time's plug-in (stays in Deep Time)

Deep Time keeps `projection.js` with its Rust and Java twins and `tools/viewer/projection-check.js`,
so the three twins stay beside their parity test (VW3). It supplies:
- the projection object: `DTProj.world(block, C)` already returns one (`PetroffGuyou` or
  `LambertWorld`);
- its tile pyramid (2:1, `2^(z+1) × 2^z` tiles, 256 px, zoom 0 to `max_zoom`) and its tile source
  (`BakeMap::render`);
- its own layers (deposits) and its host actions: the sample readout and "3D preview here".

Its pages' tests stay in Deep Time (projection-check.js, deposits-check.js, phase1_viewer_check.py,
shot.py). What Deep Time must change to do this is §14.

### 3.3 rtsim's side (AGPL, in mic-rtsim)

- **`crates/rtsim-view`**: the `ViewSource` and `DetailSource` traits each module implements (§4.3),
  part keys, aggregation by zoom (§6), the view tap (§5.2) and the archive (§10). (Not to be confused
  with `rtsim_world::interest::Viewer`, the trait for player interest areas.)
- **rtsim-jni** links the tap and the archive writer into the engine step.
- **`rtsim-cli view`** and **`rtsim-cli run`**: the sidecar and the headless host, one binary (PLAN.md
  §4.11 already names `run` as the fast-forward command).
- **No Java.** The bridge does no viewer work. The lease is a file the engine checks itself (§5.2).

### 3.4 Where the HTTP server sits (VWD-1)

Deep Time's coordinator leans towards a sidecar. Ben has not been asked. The honest comparison:

| | **A. Sidecar + view tap** (recommended) | **B. In-process mic_native service** | **C. Sidecar on saves only** |
|---|---|---|---|
| Nobody watching | the sidecar is not running; the engine makes one integer comparison per step and stats one file every 10 s (§5.2) | no listener, no thread if started on demand; an atomic flag per step | nothing at all |
| Someone watching | the step tail encodes parts only in the engine's slack, at most 250 µs a step (TUNABLE, §5.2); the IO lane commits them once per tap period; the sidecar's own process does all HTTP, aggregation and tiles at idle priority | the step tail publishes parts into a `SnapshotCell`; HTTP, aggregation and tile rendering run inside the server process | the sidecar alone |
| Freshness | what open pages look at, within a tap period (2 s, TUNABLE); the rest within the measured cycle; moving things animate between publications from their legs | sub-second | the last Minecraft save: autosave every 6,000 ticks, 300 s at 20 TPS (`MinecraftServer.AUTOSAVE_INTERVAL`, 1.21.1 sources) |
| Cards of heavy things (a district's inspection) | summary live; the full inspection from the last save's pages, projected to now in the sidecar. Industry saves one page per district and one per replay cluster (persist.rs), so a card decodes three pages (the district's, its cluster's, the globals) without the whole engine; the projection over them (`inspect_peek`, §5.3) is new code | on demand from engine state, answered in the step tail | as of the last save |
| Base maps | Surveyor, HABITAT, Deep Time tiles, all in the sidecar | HABITAT cheaply; Surveyor parsing and Deep Time tiles (about 60 MiB per open map at 16k, at most two maps: tiles.rs) would sit in the server process | as A |
| Plugins' layers (P1: separate cdylibs) | live: none of their code needed, since parts are self-describing (§4.3); saves and the host: the plugin's cdylib loaded by an in-process mic_native host (UNVERIFIED; until then, live only) | in the server, where the plugins already are | only with the plugin's cdylib loaded, as A |
| Threads | none in the server | a listener thread in the server process: **mic_native's contract says a plugin owns no threads**, so the host would need a new facility (a change to mic-native) | none in the server |
| Isolation | a crash, a leak or a slow request cannot touch the server | panics are caught at the boundary; an abort, an out-of-memory or a stuck socket thread is the server's | as A |
| Deploying | a second binary on the server (built from the repo by its flake) | nothing extra: it ships in the mod jar | as A |
| One API for live, saves and the headless host (VW2) | yes: three sources behind one API | live only; saves and the host need a second server anyway | saves only |
| Deep Time | the same shape as Deep Time's workbench, a process of its own (`deeptime watch`) | Deep Time's workbench is its own process, so it would not share the server; in-game bakes run on the host's foreground pool (X1) with no viewer attached | as A |
| Build skew | the sidecar must be built from the engine's commit (pages carry no format version before release); the tap's header names the engine's build, and a mismatch is refused, module by module | none: one build | as A |

**Recommendation: A.** It is the only option where a stuck viewer can never cost the server anything,
it needs no change to mic-native's no-threads rule, and the same program serves saves, live servers
and the headless host. B's real advantages are freshness below 2 s, nothing to deploy, and plugins'
layers with no question of loading their code elsewhere; none is worth a socket inside the game server
for an ops tool.

---

## 4. One read-only API over three sources (VW2)

### 4.1 Sources

| Source | When | Sim state from | Live view from | Archive |
|---|---|---|---|---|
| **save** | `rtsim-cli view <world>` (no `--live`): a copy, or a world with no running engine. Writes nothing | `<world>/rtsim/` opened read only (§5.3), decoded a page at a time | none: the sidecar writes parts itself from the decoded state | `<world>/rtsim/archive/` |
| **live** | `rtsim-cli view --live <world>`, once the engine answers the lease (a new epoch's header appears) | the last save's pages, for cards of heavy things and evicted regions | `<world>/rtsim/view/<epoch>/` (the tap's pages) | same, up to the last save, plus the tap's pending-rows part |
| **host** | `rtsim-cli run` | the engine in the same process | parts published into a `SnapshotCell` by the host's own step tail | the fork's archive |

All three produce the same thing, **parts** (§4.3), and everything above them is one code path.

- **Loading a save without the JVM.** Every module already has a pure-Rust loader that takes bytes
  (`World::load`, `Dragons::load`, `Herds::load`, `Ships::load`, `Conflict::load`, industry's and
  space's `persist.rs`), but the glue that reads them from pages is private to rtsim-jni and tied to
  its link types. It moves into a library function, `load_world(dir)`, which also rebuilds what the
  pages do not hold, as rtsim-jni does at start: phase 1's config, goods and recipe book (its catalog
  and `WORLD_SEED`), the conflict's cultures (`Cultures::from_json`), the herds' climate, each
  module's tunables.
- **Build skew.** Pages carry no format version before release (each module checks its own magic and
  version and drops what it cannot read). The sidecar never drops anything: a module whose pages it
  cannot read is reported and left out, and the tap's header names the engine's build so the sidecar
  can say "built from a different commit" before it tries.
- **Memory.** The sidecar never holds a whole world decoded. Where a module saves a page per thing
  (industry: one per district and one per replay cluster, persist.rs), it decodes a page, writes its
  rows, and drops it: at the 100,000 districts the benches ran, decoding them at once would take about
  860 MB (derived: 8.6 KB a replaying district, docs/overnight-choices-industry.md), beside the
  server's JVM. Where a module saves one page for everything (phase 1's world: about 65 MB at 4,096
  markets, derived from 15.8 KB a market), that page is held while the sidecar runs. Pages decoded for
  cards are kept under a cap (256 MiB, TUNABLE) and dropped least recently used first. Once the
  modules move onto paging (not yet: overnight), the kernel's `PagedTable` does this with its own
  budget.

### 4.2 Routes

Every route is a GET; anything else is 405. Nothing a page sends changes a simulation (Deep Time's
rule, and here no route has a body at all).

| Route | Answer |
|---|---|
| `/` and the static files | the page and its scripts |
| `/api/world` | frames (dimensions), each frame's topology (kind, domain, wrap period, edges, cone points), the projection id, the source (save, live, host), game time, wall time, ticks per second measured, the tap's age and cycle time |
| `/api/layers` | layer descriptors (§4.3) |
| `/api/marks?frame&x0&z0&x1&z1&bpp&t&layers` | marks for the viewport, within the budget, with a version per layer |
| `/api/thing/<layer>/<id>?t` | a card (§4.4) |
| `/api/events?frame&from&to&x0&z0&x1&z1&kinds&limit` | archive rows in the window |
| `/api/events/histogram?from&to&buckets&kinds` | counts per bucket, from the archive's chunk headers (§10.3) |
| `/api/stream` | SSE: `hello`, `frame` (a new publication: game time and each layer's version), `events` (archive rows newer than the stream's start), `ping` every 15 s |
| `/tiles/<source>/<z>/<x>/<y>.png` | base map tiles |

`t` is a game tick. Without `t` the answer is "now"; with a past `t` the layers answer from the archive
(§10.6), and layers that have no past say so.

### 4.3 Layers are data (D4)

A module or plugin adds a layer with its own Rust, registered with the view registry (the engine's and
the sidecar's). The core and the page never learn about it: the page draws any layer from its
descriptor with a closed set of mark kinds. **Parts are self-describing**: a module writes generic
rows (a mark of a known kind, the region it falls in, and the card fields its descriptor names), and
one reader in the core reads every part. So the sidecar's live source runs no module code at all,
which matters because plugins are separate cdylibs from other authors (P1) that a sidecar built from
mic-rtsim cannot contain.

```rust
/// Where the state is (the engine's step tail, the headless host, or the sidecar over a save).
pub trait ViewSource {
    /// Layer descriptors (id, group, name, mark kinds, styles, legend, card fields, zoom range per
    /// ladder level, whether it has cards and a past).
    fn layers(&self) -> Vec<LayerDesc>;
    /// The parts this source writes. Keys are stable: things that stay put are bucketed by
    /// canonical region (TP4); movers by id, so a mover is in one part whichever region it crosses.
    fn parts(&self) -> Vec<PartKey>;
    /// Writes one part as of `now` as generic rows, with a count per kind per 512-block region.
    /// `&self` only: a source cannot change state.
    fn write_part(&self, key: PartKey, now: Tick, out: &mut PartWriter);
    /// The Individual level for one region (the travellers on its roads, a herd's animals there):
    /// pure samples, never taken.
    fn write_samples(&self, region: RegionKey, now: Tick, out: &mut PartWriter);
}

/// In the sidecar, over the saved sim pages, read only: cards of heavy things.
pub trait DetailSource {
    fn card(&self, pages: &SavedPages, layer: LayerId, id: u64, at: Tick) -> Option<Card>;
}
```

`ViewSource` runs where state is. `DetailSource` (a district's inspection, a herd's ledger) and the
save source's decoding are module code that runs in the sidecar: rtsim's own modules are linked into
`rtsim-cli`; a plugin's would be its cdylib loaded by an in-process mic_native host, as the headless
host loads the engine (whether the host can load a third-party plugin outside the JVM is UNVERIFIED).
Until it can, a plugin's layers are live and host only, and its cards are its rows' fields
(VWD-17).

**Mark kinds** (the page knows only these):

| Kind | Fields | Used for |
|---|---|---|
| `point` | x, z, glyph, colour, size, id | villages, dragons, ships, docks, stations, players |
| `leg` | pieces of (x0, z0, x1, z1), t0, t1, glyph, id | caravans, dragons in flight, trains, bands walking: the page animates the position |
| `path` | points with times, colour | herd routes, war band routes, ship lanes |
| `edge` | from and to positions, width, colour | roads and flows, train lines, feuds |
| `area` | centre and radius, or a box | herd footprints, faction reach, district boxes |
| `cell` | a grid origin, cell size and values | density fields, HABITAT, heat |
| `cluster` | x, z, count per kind | anything over the budget |
| `label` | x, z, text | names at deep zoom |

A plugin built around one mod (D4: allowed) adds its layers the same way. The native plugin SDK (P1,
tier 2) gets the `ViewSource` and `DetailSource` hooks.

### 4.4 Cards

A card is generic JSON: a title, a list of fields (label, value, unit), small series (a price over
the archive's days, a grievance over its steps) and links to other things (the caravan's home village,
the raid's district). The page renders every card with one renderer.

---

## 5. Reading without writing

### 5.1 The rule

Viewing never advances or alters the simulation, and it never makes the engine do work that changes
what it writes:
- **No `&mut`.** Every `ViewSource` method takes `&self`. Where a module's reader takes `&mut self`
  today, the viewer gets a `&self` twin. `LazyTable::read` commits elapsed slots and moves `touched`
  (the eviction order). Industry's `inspect` and `sim_stock` take `&mut self`; they commit no slots
  (the cluster model has no slot period, cluster.rs), but they move `touched` and the statistics.
- **Never past now.** A read is clamped to the engine's time and never fires what is due (industry's
  `inspect`: "it projects the replay without committing anything (the kernel's read rule) and never
  fires what is due").
- **Never a page load the sim did not ask for.** In the live source the tap encodes only resident
  state; what the engine has evicted, the sidecar reads from the sim pages on disk (§5.2, item 6).
- **Never an observer.** A viewer is not an interest area: looking at a road samples its travellers
  purely and never materializes a caravan, never promotes a traveller to a row, never books a leg.
- **Never an input to the sim.** Nothing a page sends reaches the sim. The engine reads two things
  from the viewer, both in the lease: its modification time, which switches the tap, and the focus
  (§5.2), which orders what the tap encodes. Neither reaches a module.
- **Only the engine's slack.** The tap encodes only while the next hand-over has not arrived and
  stands down when the engine is behind (§5.2), and its time is not counted as step time by the
  governor, whose stretch decisions follow wall-clock load (PLAN.md §4.6, §4.7). So the guarantee is
  exact while the engine keeps up. When it does not, steps already run late and "a decision that ran
  late under load" can differ (PLAN.md §4.7); the tap then adds nothing, except at most one part's encode at
  the moment a hand-over arrives, and the sidecar competes for CPU only at the idle weight (§5.5).

### 5.2 The view tap: published like the hand-over

The bridge's hand-over (docs/boundary.md §1.2) is four buffers whose `published` and `consumed`
words move with release stores and acquire loads, so a reader only ever sees a whole hand-over and
never waits on the writer. The tap follows the same discipline:

1. **The lease.** With `--live`, the sidecar writes `<world>/rtsim/view.lease`, outside the view
   store so that deleting the store can never lose a renewal. It holds the sidecar's process id and
   the **focus**: the canonical region keys that open pages show at the Group level or deeper (at
   most 64, TUNABLE; nothing secret). A thread at normal priority renews it every 10 s while at least
   one page has its event stream open, and rewrites it when the focus changes (at most once a second,
   TUNABLE), so a starved idle-priority sidecar never loses its lease. The engine compares the
   hand-over number once per step and every 200 steps (10 s at 20 TPS, TUNABLE; every 40 steps, one
   tap period, while leased) `stat`s the lease on the IO lane, reading it (a few hundred bytes) only
   when its time changed. A lease older than 30 s (TUNABLE) is over.
2. **Leased: encoding in the slack.** At the step's tail, after `C[h & 1].published = h`, the tap
   encodes parts only while hand-over h + 1 has not been submitted (`O[(h + 1) & 1].published`, an
   acquire load, checked between parts) and for at most 250 µs a step (TUNABLE). A part is never
   split, so a step whose input is ready waits for at most one part's encode; parts are capped in rows
   so that stays far under the budget (the encode time per part is UNVERIFIED; V4 measures it). The
   tap stands down for the whole step when hand-over h held more than one TICK record (the engine was
   behind) or step h took more than half a tick (25 ms, TUNABLE). Each cycle goes: the header and
   the global parts, then the focus regions' parts and samples (`write_samples`), then the other
   parts round robin. Encoded parts go into a latest-wins mailbox with one slot per part, so nothing
   piles up.
3. **Commits.** Once per tap period (2 s, TUNABLE) the IO lane takes the mailbox, drops the parts
   whose CRC32 matches the last written, and commits the rest as one commit into the view store
   `<world>/rtsim/view/<epoch>/`, without fsync (`Durability::NoSync`), with a header page: game
   time, wall time, ticks per second, the engine's build, a version per layer, the cycle time, the
   epoch and the **resident set** (the regions whose state is in memory). At most one commit is
   outstanding; if the IO lane has other work queued when the period comes (`pool_info`), the commit
   waits for the next period, so saves and prefetches always go first. One commit a period is one
   segment a period.
4. **Not leased.** Nothing is encoded or committed. When a lease has been over for 60 s (TUNABLE) the
   IO lane deletes `<world>/rtsim/view/`; the next lease starts a new epoch in a new folder and deletes
   any older one. A delete that fails (Windows refuses to delete a file another process holds open) is
   retried at the next start. Epochs are separate folders, so a reader holding an old file can never
   read it as part of a new store.
5. **The reader** opens the view store read only (§5.3). A page-store commit happens at one moment,
   the append of its log record (page.rs), so the reader sees a whole publication or none: the log
   record plays the part of the hand-over's `published` word.
6. **Regions the engine has evicted** are not encoded (that would load pages the sim did not ask
   for), and the resident set tells the sidecar which regions those are, as distinct from regions not
   yet encoded this cycle. The paging layer evicts only pages whose state is already on disk, and a
   change made only by catch-up is a pure function of that state and the time (paging.rs, O4), so the
   sidecar reads an evicted region from the sim pages and peeks it to now: what the engine would
   compute, up to the peek cap, and as of the anchor beyond it (§5.3). For zoomed-out views it counts
   each evicted page once per version (a page moves in the store whenever the engine rewrites it) and
   caches the counts, so evicted land costs a scan of its pages once, not per request.

**Parts are small and stable.** A part is encoded whole, so every table is bucketed until each part
holds at most 2,000 rows (TUNABLE), the 4,096 villages included. Things that stay put (villages,
lairs, districts, docks) are bucketed by region. Movers (caravans, bands, dragons, ships, parties,
trains, herds) are bucketed by id and carry their legs, and the sidecar places them by the leg, so a
mover encoded at one step and a region encoded at another can never show it twice or lose it.

**Why round robin and no change tracking.** A part is re-encoded whole and dropped when unchanged.
That costs work only while leased, and needs no instrumentation in any module (no dirty flags on
every state change). Moving things are published as legs, which change only at events, so most parts
are unchanged most of the time and cost an encode but no write.

**Its cost and its freshness.** With 4,096 villages, 512 dragons, 814 ships and 100,000 district
summaries (the largest counts the benches ran), plus 10,000 rows of caravans, bands and parties
(ESTIMATE: no bench has run that count), at about 40 bytes a row (ESTIMATE), a full cycle is about
4.6 MB (derived: 115,422 rows × 40 bytes). The tap has at most 5 ms of encoding a second (derived:
250 µs × 20 steps), so a full cycle takes 4.6 MB divided by 5 ms a second times the encode rate: about
0.9 s at 1 GB/s, about 9 s at 100 MB/s (two illustrations; the encode rate is UNVERIFIED and V4
measures it), and longer when the engine has little slack. That is why the focus goes first: what an
op is looking at is fresh within a tap period or two, the rest within the cycle, whose measured
length the header carries and the page shows per layer.

### 5.3 Peeks, and opening a store read only

- **`LazyTable::peek(&self, h, at)`** and **`PagedTable::peek`**: when no slot boundary lies between
  the anchor and `at`, `LazyModel::view` (already pure). Otherwise the cell is cloned and the clone
  caught up, so the cost is the elapsed slots (O5); above a cap (64 slots, TUNABLE) the peek answers
  as of the anchor and the card says "as of game day d". Phase 1 markets already read this way
  (`World::market` uses `view_without_catch_up`).
- **Industry** gets `inspect_peek(&self, d, at)` and `stock_peek(&self, d)`, the current `inspect`
  and `sim_stock` (on main since the industry rework) without their `&mut` effects.
- **`PageStore::open_read_only(dir)`**: today's `open` creates the folder, truncates a torn log tail
  and deletes files the index does not name (`remove_strays`). A viewer must do none of that while the
  engine is writing. The read-only open reads the root and the log up to the last whole record and
  ignores the rest. Compaction and log folds delete files at any time (page.rs), so the reader holds
  no file open between reads, checks every record's CRC32, and on a missing file or a bad record
  reopens from the root and retries, three times (TUNABLE), then reports the store as busy. On
  Windows its files are opened with `FILE_SHARE_DELETE`, so the engine can still rename and delete
  them; that this keeps every save working while a sidecar reads is UNVERIFIED, and V1 and V4 test it
  on Windows.

### 5.4 The tests that prove it

- **Lockstep.** Two runs of the same world for a fixed number of game days, one with a viewer leased,
  its focus moving, and requesting every route as fast as it can, one with nothing. The state hashes
  (`World::state_hash`, `Dragons::state_hash`, the herds', ships' and conflict's) must agree every
  game day, and the bytes of the sim pages must be identical at the end (the view store and the lease
  are outside them).
- **Under load.** The same pair with the engine held to fewer CPUs than it needs, so steps run late.
  A loaded run can differ from itself (PLAN.md §4.7), so the test measures the tap instead: it stands
  down whenever the engine is behind, and no step whose input was ready waits longer than one part's
  encode.
- **Saves.** Without `--live` the sidecar writes nothing: the world folder's bytes are identical
  before and after a session, which a read-only copy (SRV1) also enforces.

### 5.5 D1, point by point

| D1 asks | How |
|---|---|
| Nothing while nobody watches | no sidecar process; in the engine, one integer comparison per step and one `stat` every 10 s. The archive is recording, not viewing, and is VWD-8. |
| Bounded while someone watches | the tap only in the engine's slack and at most 250 µs a step (TUNABLE), one commit per tap period, waiting behind any other IO-lane work; the sidecar in its own process at idle priority (on Linux `SCHED_IDLE`, whose scheduler weight is 3 against nice 19's 15, so against a busy engine thread it gets about a sixth of a CPU, derived; and the idle IO class), its decoding and rendering on one worker thread (TUNABLE), under its own memory cap (§4.1); 2,000 marks a request; 64 connections |
| Never on the server thread | the bridge has no viewer code at all; the tap runs at the tail of the engine step on mic_native's background pool, its writes on the IO lane |
| Reads published state | the tap publishes whole commits, as the hand-over publishes a whole buffer; the host source publishes through `SnapshotCell`; peeks never commit (§5.3) |

---

## 6. Aggregation by zoom, on the detail ladder

The viewport's zoom is blocks per screen pixel (bpp). Each layer's descriptor says which ladder level
(SCALE.md §2) answers at which bpp (TUNABLE per layer):

| bpp | Ladder level | What is drawn | Answered from |
|---|---|---|---|
| 256 and up | Strategic, Aggregate | totals per region, faction reach, trade volume per road | the per-region counts every part carries; evicted regions' from the sidecar's count cache (§5.2, item 6) |
| 64 to 256 | Field, Flow | density cells (herd footprints spread over 64-block cells), roads as edges with rates | region counts and edge rows |
| 4 to 64 | Group | herds, caravans on legs, war bands, ships, dragons, districts | rows |
| under 4 | Individual | sampled travellers on a road, a herd's sampled animals, the glass box's nodes | pure samples (`travellers`, `members::member`), never taken: live, from the tap's focus samples (`write_samples`, §5.2); from a save or the host, for any region |

**The budget.** A request returns at most 2,000 marks (TUNABLE), shared among the visible layers by
priority. A layer with more rows in view than its share is binned on a 32 px screen grid (TUNABLE) and
returned as clusters with counts per kind. **Cost per request:** the regions the viewport covers
(each part keeps its rows grouped by region, with counts) plus the marks returned; never the rows
outside the view, and never more marks than the budget. The one cost that grows with the world is the
first zoomed-out look at evicted land, a scan of its pages, which the count cache pays once. Millions
of latent animals are one herd area; a road with a thousand caravans a day is one edge (A13).

---

## 7. Layers per module

What each module stores (read from the code on 2026-10-20) and what it draws. "Rows" is the largest
count a bench has run, where one exists.

| Module | What it stores | Layers and marks | Card | Rows |
|---|---|---|---|---|
| **Settlements** (rtsim-settle) | `Settlement{key, kind, pos, pop, joined}`; `NodeKind` Village(Human or Ribbit), PlayerBase, Haven | villages and bases as points sized by population | kind, culture, population, joined, neighbours | 4,096 villages (restore bench) |
| **Markets** (rtsim-trade) | `Market{lines, goods, treasury...}`, no position of its own (its node's); price `p(S) = p_star * (1 + r * (1 - S / S_bar))` on `[0, 2 S_bar]` (`PriceCurve::price_at`) | villages coloured by one chosen good's price against its anchor (`Market::price`, `anchor`) | per good: price, anchor, stock, target, level; treasury, currency; the news it holds and its age (`NewsRing`) | 15.8 KB per market in memory (overnight) |
| **Roads and flows** (rtsim-travel) | `Edge{a, b, medium, msg_period...}`, `Dir::in_transit`, `World::lane_hazard` | edges between node positions, width by goods in transit, colour by hazard | rates both ways, raid and predation hazard, messenger period | |
| **Caravans** (rtsim-travel, rtsim-world) | rows: `Caravan{home, members, stage, leg, purse}`; travellers derived from the flow, never stored | legs; travellers only below 4 bpp | home, stage, leg times and progress, members, cargo, purse, news carried | |
| **Messengers** | derived from `Edge.msg_period` and `msg_walk`; no position function yet | legs below 64 bpp | edge, departure, the packet's age | |
| **Touring bands** (rtsim-world `ribbits.rs`) | `Band{home, route, ...}`; `World::band_view(b, at)` | a point staying at a node, or a leg walking | home, route, stop, members | |
| **Knowledge** (rtsim-knowledge) | reports (32-byte rows with time and place), sentiment per holder (`sentiment_snapshot`), notables | reports become archive events (§10); sentiment is a card section | a village's feelings toward players (ops only) | report table capped at 2^20 |
| **Dragons** (rtsim-ecology) | `Lair{pos, occupants, hoard}`, `Dragon{state, leg, rest, fullness, health, grudges}` | lairs; dragons as points with their flight leg; a line to a hunt's target | name, species, age, `fullness_at`, `health_at`, state and target, grudges | 512 dragons (bench) |
| **Herds** (rtsim-ecology) | `Herd{species, classes, route, ...}`; latent members cost nothing | an area (`footprint`) with `count`; the route as a path; sampled animals below 4 bpp | species, count, cohorts, condition, ledger (born, died, eaten, taken) | 1,000,000 animals per herd (bench) |
| **Ships** (rtsim-ships) | `Dock`, `Lane{path}`, `Service` timetables, `Ship{role, cargo, hull}`, `Haven`, `Navy`, `LaneThreat` | docks; lanes as paths tinted by threat; ships as points (`Service::fix`; pirates by `pirate_pos`, private today, so a public reader is new) | role, service, phase, cargo, crew, hull | 814 merchant ships (bench) |
| **Conflict** (rtsim-conflict) | `Faction{pos, radius, pool}`, `Feud{grievance, step, demand, party}`, `Party{kind, route, times, phase, loot}` | faction areas; feuds as edges coloured by step (Notice to Siege); parties as legs and paths | faction, warriors now (`pool_at`), feuds with grievance by kind (`feud_view`), demands, parties | |
| **Industry** (rtsim-industry) | `District{chunks, dim, presence, phase, ports, reports, attacks, ...}`; stations in the engine's private `stations` map (a public reader is new); `lines()` (`LineEdge`: trips, cargo, travel ticks); train `legs()` | districts as boxes of their chunks (a box and centre kept at detection: new, small); stations; train lines as edges; trains as legs | the inspection (`inspect_peek`, `Inspection`): state and reason, rates per hour, made since, running or stopped, learning ticks to go, last reconcile, attacks; later the glass-box graph | 100,000 districts (bench) |
| **World model** (rtsim-space) | HABITAT cells (64 blocks: water, ice, biome, tags, heights, temperatures, creatures) | the base map under everything (§9) | biome, heights, year-round degrees, creatures | about 6 KB per sampled region (overnight) |
| **Players** (shared interest areas) | `PlayerState{uuid, pos}`, live only | points (ops only; live source only) | UUID; the name from the server's `usercache.json` when the sidecar is given the server folder | |

The **glass box** (GLASSBOX.md, next to be built) adds one layer later: a compiled district's nodes
and edges, below 1 bpp, from its cold graph pages (§9.2 of GLASSBOX.md) read in the sidecar, and its
hot regime summary from the tap.

---

## 8. Seams: topology on the map

Every position the engine keeps is canonical (TP4), and the topology layer (C31, branch feat/topology)
supplies everything the viewer needs:
- **Viewport queries** fold their rectangle with `Topology::fold_rect`, one canonical rectangle per
  tile of the cover it overlaps, so things just across a seam are found.
- **Legs and paths** arrive already cut by `Topology::pieces`: each piece is a straight move in
  canonical coordinates. A caravan crossing a north or south half-turn is two pieces, drawn apart;
  the page never draws a line across the map.
- **East and west** wrap in the page, as worldmap.js wraps tiles today: a mark is drawn again at
  x ± period when the viewport spans the seam.
- **Cone points** (`Topology::cones`) can be shown as a debugging layer.
- **Flat worlds** (Ben's server) are the identity, bit for bit (TP1).

---

## 9. Base maps

**Deep Time worlds:** the bake's tiles through Deep Time's plug-in (satellite, elevation, water,
overlay; at 16k, cold tiles take 3 to 26 ms on average by layer, 48 ms at worst, and warm ones 0.3 to
1.4 ms: Deep Time's STATUS.md, measured on the Mac under load).

**Other worlds** (Ben's server):

| Source | Resolution | Coverage | Cost | Notes |
|---|---|---|---|---|
| **Surveyor** (recommended, optional) | 1 block | every chunk Surveyor recorded (on load, unload and tick, server side) | gzip NBT per 512-block region, about 125 KB a region (measured: 39 region files, 4.88 MB, in the MICTest save); parsed and rendered in the sidecar, cached as tiles | `<world>/data/surveyor/c.<rx>.<rz>.dat`, read from Surveyor 1.2.4's `ChunkSummary`, `LayerSummary` and `uints` classes and checked against the region files of a copy of the OmegaSrv world (V5 reader spike, 2026-10-21): region palettes `blocks` and `biomes` (ids) with `blockColors`, `biomeWater`, `biomeGrass` and `biomeFoliage` (colours); `chunks` keyed by absolute chunk coordinates `"cx,cz"`, each with `layers` keyed by the top of a height band (0, 61, 256, 319 in both saves; a band runs from its key down to the next key), each with `found` (a 256-bit column mask as a LongArray, trailing zero words dropped; column i is x = i / 16, z = i % 16 in the chunk) and arrays holding one entry per found column in mask order: `depth`, `biome`, `block`, `light`, `water` (water blocks above the floor) and `glint` (the light at the water's top). Each array takes the narrowest form that fits: a byte array (unsigned bytes; two entries a byte, high nibble first, when it is half the count long, rounded up), an int array (ints; two unsigned shorts an int, high half first, when it is shorter than the count), a single byte, short or int when every column agrees, and absent when every column is 0. The floor is at y = band key minus `depth` (MEASURED: 30,951 of 31,038 floors, 99.7 %, name the block the region file holds there; the other 87 changed after Surveyor saw them, melted ice and a felled tree), and the water's top block is the floor plus `water` (12,765 of 12,781). The floor is the first block, from the band's top down, that has a map colour and at least two air blocks above it (water between does not count against it); a plant, carpet or snow layer with a map colour standing on it is recorded in its place, with its own height. |
| **HABITAT** (always there) | 64 blocks | every cell rtsim sampled (all 16 of its chunks loaded) | nothing extra: the engine already holds it (about 6 KB a region) | water, ice, biome, heights; coarse but always present with rtsim |
| Region files | 1 block | every generated chunk | the whole `region/` folder (182 MB, 174 MiB, in the MICTest save) parsed with chunk NBT; colours need a table exported from the game, since region files hold block states, not colours | heavy; worth it only if Surveyor leaves the pack |

**Tile cache location:** Deep Time's tiles.rs caches under the run's own folder
(`<run>/tiles/v<RENDER_VERSION>/...`). On a play world the bake sits inside the world folder
(`planet/deeptime`), so a sidecar caching there would put tile PNGs into the server's world and its
backups. The sidecar keeps its cache outside the world folder, keyed by the bake's identity (and, on
other worlds, the Surveyor and HABITAT inputs' identity).

D4: Surveyor is a pack mod, so it is an optional base-map source: the core falls back to HABITAT when
`data/surveyor/` is absent. Antique Atlas stores nothing on the server (it reads Surveyor's client
data). Distant Horizons' server database holds block states, not colours, in an internal format; it
is not proposed.

---

## 10. The event archive (VW4)

The engine keeps almost no history today: dragons', herds', ships' and conflict's logs are in memory,
off by default and not saved, and industry's log is capped at 20,000 entries and not saved. The
persisted histories are the knowledge reports (lifetimes of 3 to 30 game days) and each district's
reconcile and attack queues. So the timeline needs a recorder that runs while the engine runs.

### 10.1 What is recorded

At points where each module already does the thing (no new scans):

| Module | Events | Position |
|---|---|---|
| Knowledge | every report (assault, killing, robbery, theft, trade, rescue, resident assault and killing), as they are made | the report's own place |
| Phase 1 | village joined; base marked and unmarked; house placed; caravan promoted to a row, robbed, arrived; **trade shift**: at a market's own events (breakpoint, delivery, arrival), its price index has moved more than 10% (TUNABLE) since its last recorded value. No daily scan: a quiet market writes nothing, and a slow drift is recorded at its next event. | the node |
| Dragons | flight legs (start, end, purpose); hunts (`HuntRecord`); raids on districts and trains; lair claimed; departure (boredom); breeding; death | leg ends, target |
| Herds | founded; route legs at each route decision (**migrations**); predation; starvation | route points |
| Ships | pirate raids and their results; ship lost and back; haven changes (timetabled departures are derived, never recorded) | the lane point |
| Conflict | escalation step changes per feud (**wars**: Invasion and Siege); demands, tribute paid or refused; parties launched (their route legs); raid outcomes (units, loot, damage); a party's end | the party, the target |
| Industry | district detected; phase changes (learned, stale, damaged); reconcile reports; attack reports; train line first seen | the district's centre |
| Players (ops) | none by default (VWD-9) | |

### 10.2 Rows

Two 32-byte layouts, after `ReportRow`:

| Event row | Bytes | | Leg row | Bytes |
|---|---|---|---|---|
| tick (48 bits) | 6 | | t0 (48 bits) | 6 |
| kind | 2 | | kind | 2 |
| frame | 1 | | frame | 1 |
| severity, flags | 1 | | flags | 1 |
| x, z (24 bits each, blocks) | 6 | | x0, z0, x1, z1 (24 bits each, blocks) | 12 |
| subject id (48 bits) | 6 | | subject id (48 bits) | 6 |
| object id (48 bits) | 6 | | duration in ticks (32 bits) | 4 |
| magnitude (i32: goods, units, damage) | 4 | | | |

- **Kinds** are names a module registers (`"conflict.raid"`), given ids in the archive's own
  dictionary file, so a new module adds kinds without a format change.
- **Frames** are a byte, an index into the same dictionary (`ReportRow` spends two): 255 dimensions
  with archived events. A further dimension's rows are dropped and counted, never mislabelled.
- **Ids** are 48-bit archive handles: the entity's own id when it fits in 48 bits, otherwise (a
  player's UUID, a 64-bit id with high bits set) an entry in the archive's id dictionary, marked by a
  flag. Nothing is truncated.
- **Coordinates** are 24-bit signed blocks (±8,388,607), as in reports; a Deep Time world at 32k
  spans 26,112 × 13,056 blocks (deeptime-format's FORMAT.md), well inside. Positions further out are
  clamped and flagged.
- **Positions sampled coarsely (VW4) are leg rows.** Everything that moves in the sim moves on legs
  or routes of straight pieces (caravans, bands, dragons, herds, parties, trains), so a position is
  recorded once per decision, and the leg gives the position at any moment between. That is coarser
  in time than any fixed sampling rate a moving thing would need, and exact. Age thins it further:
  the notable tier keeps only named movers' legs, the count tier only counts per region per day.
  Nothing is sampled on a clock, so a quiet world writes nothing.
- **Payloads** (a raid's loot by good) go in a side block of the chunk, pointed to by a flag; only
  rows with severity at or above "notable" may have one.

### 10.3 Files and the flush

- `<world>/rtsim/archive/`: chunk files of up to 4,096 rows (TUNABLE) sorted by time, LZ4 and CRC32
  with the kernel's codec, and an index of chunk headers: first and last tick, the wall-clock time at
  the flush, the page save sequence, the bounding box, and a count per kind. The histogram route reads
  only the headers.
- **In the step**, an archived event is a push of 32 bytes into a pending buffer. The buffer is capped
  (1 MiB, 32,768 rows, TUNABLE); past the cap the lowest severity rows are dropped and counted, and the
  step never waits.
- **At each save**, at the snapshot (`save_now`, on the step's thread), the pending buffer is swapped
  for an empty one, so the swapped rows are exactly the events up to the state being saved. The IO
  lane writes them as chunks before the page commit, with no fsync of their own: the archive is a
  record, not the truth. Each chunk is tagged with that save's `obs_seq` (the observations the
  generation includes, `Generation::obs_seq` in page.rs), not its save sequence: a save that fails
  leaves its sequence to the next one (`last_seq + 1`), so a sequence can name two different
  snapshots, while the observation sequence moves on with every tick's TICK record (UNVERIFIED that
  `obs_seq` counts TICK records; if not, the tag is the hand-over number at the snapshot).
- **Between saves**, while leased, the tap publishes the pending rows as one more part, so a live
  page sees an event within a tap period rather than at the next save (up to 300 s later).

### 10.4 Tiers, budget, compaction (VWD-4)

| Tier | Holds | Default horizon | Share of the budget |
|---|---|---|---|
| **Full** | every row | the newest 504 game days (TUNABLE: 7 real days of 24-hour play at 72 game days a day) | 50% |
| **Notable** | rows at or above "notable" severity, and leg rows of named movers (dragons, war bands, named ships) | up to 7,200 game days (TUNABLE: 100 real days of 24-hour play) | 35% |
| **Counts** | per kind, per 512-block region, per game day: a count and a magnitude sum (16 bytes) | until the budget is full, then per 4,096-block square per 7 days, then the oldest go | 15% |

- **Budget:** 256 MiB (TUNABLE) in all. It is a hard cap: when a tier passes its share, its oldest
  chunks are demoted into the next tier (filtered, then aggregated, then coarsened, then dropped).
- **What the cap buys** (derived, before compression): the full tier's 128 MiB holds 4,194,304 rows.
  A world that archives R rows a game day keeps 4,194,304 / R game days at full detail: 4,194 at
  R = 1,000, 42 at R = 100,000. R is UNVERIFIED; §16's first archive test measures it per scenario.
  A busier world keeps fewer days at full detail. Where rows arrive faster than 4 MiB of compaction a
  save can move them, the flush drops the lowest-severity rows and counts them, so the cap holds at
  the cost of detail, and the page says how many were dropped.
- **Compaction** runs on the IO lane after a save, at most 4 MiB read and written per save
  (TUNABLE, as the page store's `COMPACT_MAX_BYTES`), and never touches a chunk newer than the
  confirmed generation, which a rollback may still cut (P29).

### 10.5 Rollback (P29)

Each chunk carries the `obs_seq` of the save it was flushed with (§10.3). On open, before anything
new is written, the engine cuts the chunks whose tag is above the `obs_seq` of the generation it
selected (`select_for_journal`), so after a crash the archive never describes events the rolled-back
sim will make differently, and the journal's replay records them once. A failed save's chunk stays
and is consistent: the next save's generation includes everything up to its own snapshot. The sidecar
never cuts anything: it ignores chunks above the generation it reads. Chunks lost to a power failure
are lost: the archive misses those minutes.

### 10.6 The timeline in the page

- The strip shows the histogram (from chunk headers) for the chosen span, game days on top and wall
  dates under them.
- Scrubbing to a past tick draws: archive rows within a window around it (one game hour, TUNABLE) as
  markers; leg rows that cover that tick as movers at their exact position on the leg; the count tier
  as heat where only counts remain. Layers with no past (markets' full ladders, district internals)
  grey out, except for the series the archive keeps (a market's price index over its trade-shift rows).
- **The past is the archive's, not a replay.** Old pages are not kept, so the viewer cannot rebuild
  the full state of last week; it shows what was recorded.
- On a Deep Time world, Deep Time's event log (pre-play history, negative years, X2) can be a second
  event source on its own scale; play-time rows are in game ticks.

---

## 11. Live updates (VWD-5)

**Recommended: SSE, reused from Deep Time's `stream_events`.** One stream per open page carries
`frame` notices (a new tap header: game time, each layer's version) and new archive rows (the tap's
pending-rows part, §10.3). The page
fetches marks only for layers whose version changed, only for its viewport, and animates legs between
notices at the measured ticks per second. It stops extrapolating after two tap periods without a
notice and shows the view as stale. The open streams are also the lease (§5.2): when the last tab
closes, the sidecar stops renewing it. A tab hidden for 60 s (TUNABLE, `visibilitychange`) closes its
stream and reopens it when shown, so a forgotten background tab does not keep the tap running, and
the browser's six HTTP/1.1 connections per host bind only with six visible tabs. Cost: a thread per
open page in the sidecar (the http.rs model, at most 64), a ping every 15 s.

The alternative is polling `/api/world` every 2 s while the tab is visible, with a version to answer
"unchanged" cheaply; the lease would then follow the last request time.

---

## 12. The headless host (VW2)

### 12.1 What it is

Ben asked whether running the engine without a server is worth it for development. Yes: it is the only
way to soak a real world's state for years in minutes (P9's tunables are set "from a soak"), to
rehearse on a copy of his world (SRV1), to reproduce a bug from a save, and to develop the viewer with
no server running. Most of it exists already as rtsim-jni's test harness.

`rtsim-cli run <world> --fork <dir> [--speed 1x|10x|max] [--until <ticks or days>] [--view addr]`.
It copies `<world>/rtsim/` (pages and archive, not the view store) into `<dir>` through the read-only
open (§5.3), so it gets one whole generation even from a live world, and never writes into the world.
`rtsim-cli export <world> <dir>` does only that copy, for SRV1: a plain `cp` or `rsync` of a running
server's `rtsim/` can catch compaction halfway and copy a torn store (without `export`, copy with
saves paused by `save-off`).

It starts mic_native's host in its own process (as rtsim-jni's tests do: `mic_native_host::start`,
`register_in_process`), opens the engine on the fork's pages (the newest committed generation; there
is no Minecraft save to match), and plays the Java side of a server where nobody is online. The engine
is rtsim-jni itself, which is already built as an rlib for its tests; the host reuses the fake Java of
those tests (1,268 lines), lifted into a library.

**Strides.** Each step is a hand-over whose TICK advances game time by a stride. Cost follows events
and elapsed slots more than steps, because of the lazy regions and the timing wheel, but each step has
a fixed cost too, and at 20 ticks a step a game day is 1,200 steps. So the stride is 20 ticks at 1x
and, at higher speeds, the longest stride whose state hashes match a 20-tick run (that any longer
stride does is UNVERIFIED; V2's exit test compares them).

**Commands are answered.** Java's commands get the answers Java gives when it cannot act:
`COMMAND_RESULT` status 3, failed, and `SPAWN_REJECTED` reason 1, not ticking (boundary.md); and
`lastAppliedCmdSeq` moves on in every hand-over, so the engine's outbox drains through a ten-year soak
instead of growing.

**Saves.** The host saves the fork every 10 game days (TUNABLE), and sooner when the archive's pending
buffer passes half full, so the archive flushes (§10.3) at any speed.

**What it needs from the world besides the pages:**
- **The topology:** a Deep Time world's `manifest.json` layout block through deeptime-format (as the
  topology layer reads it, TP10); flat otherwise.
- **The session preamble Java would send:** dictionaries (dimension names and the rest) and the
  TOPOLOGY record. Whether every module's pages name what they hold without Java's dictionaries is
  UNVERIFIED (phase 1 names its nine goods itself; dragons save their dictionary); the host's exit
  test is that a GameTest world's save opens with the same state hash.
- **Seasons:** Serene Seasons' calendar is Java's. The host takes `--season-days <n>`; without it,
  every district stays in the season its pages were saved in.

### 12.2 What it can do

- Run phase 1's markets, flows, caravans on legs, messengers, bands, sentiment and news; dragons'
  hunger, flights, hunts and raids; herds' migration over the HABITAT cells already sampled; ships'
  timetables, pirates and navies; conflict's grievance, escalation, parties and raids; industry's
  learned and compiled districts replaying between breakpoints.
- Write its own archive in the fork, so a ten-year soak leaves a timeline.
- Serve the viewer live from its own step (the host source, §4.1).
- Serve P9's soak (the economy tunables r, T and k are set "from a soak"), SRV1's rehearsal on a copy
  of Ben's world, bug reproduction from a save, and the viewer's own development.

### 12.3 What it cannot do without Minecraft

- **No chunk sampling.** HABITAT stays as saved, so herds never found or wander onto ground nobody
  sampled (unknown ground is closed, X2-5), and ships keep the water they know.
- **No puppets.** Nothing materializes: no trades or robbery by players, no rescues, no watched fights,
  so caravans stay flows and the calibrations from observed fights never move.
- **No live Create.** Nothing is learned, compiled or recompiled; districts that were learning or
  stale stay frozen (I5); damage from offline raids is queued for a load that never comes. Only models
  already learned or compiled replay.
- **No Java effects.** Gossip writes, sentiment displays, house placement and transfers into containers
  are commands nobody applies. The host answers them as failed, honestly unapplied; a house booked by
  prosperity is never built, so ribbit villages stop growing.
- **No players.** No interest areas, so everything is unobserved and lazy, as on a server where nobody
  is online. Grudge raids still aim at the homes the pages remember.
- **No bootstrap.** It needs pages: the world must have run rtsim in Minecraft at least once. Importing
  villages and habitat from a save that never ran rtsim (Surveyor's structure and terrain summaries)
  is possible later, not in this design (VWD-13).

### 12.4 Speed

A whole world's speed is UNVERIFIED; V2's exit test measures it and writes it here. What is measured:
- **The `rtsim-cli sim` scenarios** (scenario/results, on the laptop), one module each with its small
  synthetic world and setup included: dragons 109 ms for 48 game days (2.3 ms a day), ships 57 ms
  for 12 (4.7 ms a day), herds 809 ms for 200 (4.0 ms a day; a sim year is 96 game days, species.rs),
  conflict 259 to 417 ms a run for 24 (11 to 17 ms a day). These are not per-module costs to add up.
- **The overnight benches at scale** (laptop under shared load): dragons 10 to 14 µs per dragon per
  game day (512 dragons: 5.1 to 7.2 ms a day, derived); ships 17 to 19 µs per game day for up to 814
  ships; quiet industry 13 to 14 µs per game day up to 100,000 districts.
- **Steps:** the fixed cost of a step is unmeasured; at 1,200 steps a game day, 10 µs a step (an
  illustration) would be 12 ms a game day by itself, which is why the host lengthens its stride.

An ESTIMATE for a world the size of the benches: dragons at 512 (5.1 to 7.2 ms), herds as in their
scenario (4.0 ms) and conflict as in its scenario (11 to 17 ms) come to about 20 to 28 ms a game
day, so tens of game days a second, about 35 to 50 (derived), before phase 1, whose per-day cost is
unmeasured. At 72 game days per real day, 40 game days a second is a real day of 24-hour play in
about 1.8 s, and ten game years (3,650 days) in about 90 s.

### 12.5 Controls

Speed, pause and stop are the terminal's (flags, and keys on stdin), not the page's: the page stays
strictly read only, as Deep Time's does (VWD-14).

---

## 13. Security

- **Loopback by default.** `--bind` accepts loopback addresses; tailnet peers (100.64.0.0/10 and
  fd7a:115c:a1e0::/48, Deep Time's `tailnet_or_loopback`) only with `--allow-tailnet`; anything else
  is refused at start.
- **Host header on every request.** Deep Time's server checks Host on POSTs only; a page on another
  site that rebinds its DNS to 127.0.0.1 could then read GET answers, which here are the whole world's
  state. The shared server checks Host on every request: the name must be `localhost`, `127.0.0.1` or
  `[::1]` (tailnet names only with the flag), on any port, so `ssh -L 9000:127.0.0.1:8740` works;
  otherwise 403. Rebinding is stopped by the name alone.
- **Other users on the server.** A loopback port is open to every local account. On a server where
  others have shells, `--socket <path>` listens on a Unix socket in a directory only the op can
  enter, which OpenSSH forwards the same way (`ssh -L 8740:/path/to/socket`); there is no token to
  leak (VWD-7).
- **Read only.** rtsim's API has no POST, PUT or DELETE (405). Deep Time keeps its POSTs with its
  same-origin JSON rule.
- **Headers:** a Content Security Policy of `default-src 'self'` (the page loads nothing from the
  network, and the browser enforces it), `nosniff`, no CORS.
- **Limits:** Deep Time's (64 connections, 16 KiB of headers, timeouts), plus the mark budget.
- **No secrets in pages:** no tokens exist (access is the SSH login); no filesystem paths beyond the
  world's name; no player addresses. Player UUIDs and names are shown: ops only.
- **Files:** the sidecar opens the world's files read only; with `--live` its one write is the lease,
  and without it, nothing. Its tile cache lives in its own cache folder, outside the world.

---

## 14. What Deep Time changes and exposes

For Deep Time's coordinator to review.

1. **worldmap.js moves** to mic-viewer as `map.js`. The `window.DTWB` coupling becomes a host object
   passed at creation: `esc`, `fmtInt`, `api` (its fetch helper) and a `store` (its `wvLayer`
   setting); `latText` and `lonText` become the projection's readout; `DT.preview` becomes a host
   action ("3D preview here", with the box and its corners); the sample readout stays Deep Time's,
   driven by a `hover(x, z)` event from the map; `DT.focusedRun()` stays in workbench.js, which recreates the map on a run change. The DOM ids
   (`stage`, `worldview`, `worldbtn`, `modal`) become parameters.
2. **projection.js** is unchanged as a twin. The map core uses what both its objects already have
   (`zRange`, `wrapX`, `canonical`, `latLon`); `fold` is `PetroffGuyou`'s only, and the core needs
   only `canonical`. `drawGrid`'s Lambert-only formulas (an
   inference from the code) become the projection's optional `grid` function.
3. **Tiles:** `BakeMap` (in `deeptime-cli/src/watch/worldmap.rs`, most of its 1,924 lines) imports
   `deeptime_core` (`climate::{glaciers, koppen, snapshot}`, `satellite`, `world::Field`) as well as
   `deeptime_format`, so it is not a light move into deeptime-viewer, which depends today only on
   deeptime-format, png, serde and serde_json. Two shapes (Deep Time's review, 2026-10-20):
   - **(a, default) a small new crate, e.g. `deeptime-tiles`,** holding `BakeMap` behind mic-viewer's
     `TileSource` and depending on deeptime-core and deeptime-format, so deeptime-viewer stays light.
     rtsim's sidecar then compiles and links all of deeptime-core to draw a Deep Time world's tiles.
   - **(b) no link at all:** the sidecar asks `deeptime watch` for tiles over loopback, and Deep Time
     ships no Rust API to rtsim.
   `BakeMap`'s Rust tests (5 in `watch/worldmap.rs`, 3 in `tiles.rs`) move with it. The disk cache
   part of tiles.rs moves to mic-viewer. `sections::route` also takes `&Workbench`, so a registered
   route gets a state handle, not only the path and query. Pointing `BakeMap` at a play world's bake
   (`planet/deeptime` inside the world folder) has not been tried: UNVERIFIED.
4. **http.rs** gives way to mic-viewer's server: Deep Time registers its routes; `tiles::route` and
   `sections::route` already fit. Two behaviour changes come with it: the Host check on GETs (by name,
   any port), and loopback only unless `--allow-tailnet` (the `tailscale serve` notes in http.rs,
   `tools/viewer/mac-watch.sh` and STATUS.md, and the m6 tailnet URL in CLAUDE.md, would follow).
5. **SSE:** `stream_events` moves as a generic writer; `live.rs` stays Deep Time's.
6. **Tests stay:** projection-check.js, deposits-check.js, phase1_viewer_check.py and shot.py, run
   against Deep Time's binary serving the shared map. worldmap.js is tested today by
   `tools/viewer/deposits_shots.py`, which opens the Final world view by clicking `#worldbtn`, checks
   `#worldview` shows and closes it; Deep Time's page keeps those ids when they become parameters, or
   that shot test changes with the move. mic-viewer adds its own checks.
7. **A dependency:** mic-viewer pinned by git commit.
8. **Optional:** an `EventSource` over its event log for the shared timeline.
9. **P1-19 is untouched:** nothing new runs during a bake. The shared server is the code Deep Time
   runs today, moved.
10. **Licensing:** the moved files leave LGPL for MPL-2.0 by Ben's relicensing, recorded in NOTICE.md.

---

## 15. Cost model

| Piece | Where | When | Cost | Grows with | Never with |
|---|---|---|---|---|---|
| Lease check | step thread, then IO lane | every step; a `stat` every 200 steps (10 s, TUNABLE), every 40 while leased | one integer comparison per step; one `stat` per 10 s; reading a few hundred bytes when the lease changed | | world size, watchers |
| View tap | step tail (after commands are published), then IO lane | only while leased, and only while the next hand-over has not arrived | at most 250 µs a step (TUNABLE) encoding, plus at most one part's encode when a hand-over arrives (UNVERIFIED, measured in V4); none when the engine is behind; one commit of changed parts per tap period, waiting behind other IO-lane work | rows, cycle length | anything while not leased |
| Archive append | step thread | at archived events, always | a 32-byte push | events | entities, time |
| Archive flush and compaction | IO lane | at each save | chunk writes; compaction at most 4 MiB a save (TUNABLE) | events | world size |
| Archive disk | `<world>/rtsim/archive/` | always | at most 256 MiB (TUNABLE) | | time (capped) |
| Sidecar CPU | sidecar, idle priority (`SCHED_IDLE`, idle IO class) | while running | one worker thread for decoding and rendering (TUNABLE) plus a thread per open stream; about a sixth of a CPU against a busy engine thread (derived from the scheduler weights, §5.5) | requests | |
| Sidecar HTTP | sidecar | per request | regions in view plus marks returned; at most 2,000 marks | viewport, zoom | entities outside the view |
| Count cache | sidecar | the first zoomed-out look at evicted land | one read of each evicted page in view, once per page version | evicted pages viewed | requests |
| Sidecar memory, live | sidecar | while running | the view store's parts it reads (about 4.6 MB at the bench maxima, ESTIMATE) plus pages decoded for cards, under a cap (256 MiB, TUNABLE) | rows | |
| Sidecar memory, save | sidecar | while running | page-per-thing modules decoded a page at a time (industry at 100,000 replaying districts would be about 860 MB at once, derived); one-page modules held: phase 1 at 4,096 markets about 65 MB (derived from 15.8 KB a market, overnight); card pages under the same cap | the largest one-page module | the number of districts |
| Base map tiles | sidecar | per tile request | Surveyor: one region parse per 512 blocks, then cached; Deep Time: 3 to 26 ms cold on average (48 at worst), 0.3 to 1.4 ms warm (16k) | tiles requested | |
| Browser | the op's laptop | while open | at most the budget of marks per layer redraw | | entities |
| Server thread | Minecraft | always | **nothing**: the bridge has no viewer code; the hand-over stays at its measured 11 to 20 µs a tick (overnight) | | |

---

## 16. Phases and exit tests

| Phase | Builds | Exit test |
|---|---|---|
| **V0 Shared core** (with Deep Time) | mic-viewer: map core, marks, server crate, tiles cache, API types; Deep Time's workbench moved onto it | Deep Time's four page tests pass unchanged; its workbench works over the SSH forward; the Host check refuses a forged Host on a GET |
| **V1 Save viewer** | `open_read_only`, peeks, `load_world`, `rtsim-view` with every module's layers and cards, the part format, HABITAT base map, `rtsim-cli view` and `export` | on a GameTest world's save: every layer draws; the folder's bytes are identical after a session; at 100,000 synthetic districts a full-world request returns within the budget and the sidecar stays under its memory cap; on Windows, a world saves while the sidecar reads it |
| **V2 Headless host** | `rtsim-cli run`: fork, driver, preamble, command answers, strides, saves, pacing, host source | a GameTest world's pages open with the same state hash; a 20-tick and a long-stride run agree on state hashes; 1,000 game days run at max speed with the viewer attached, with the engine's outbox bounded; the measured game days per second and the fixed cost per step are written into this document |
| **V3 Archive and timeline** | the archive writer, tiers, compaction, rollback cut; the timeline strip | R (rows per game day) measured for every `rtsim-cli sim` scenario and written here; a 10,000 game day soak stays under the budget; a crash between page commits cuts the right chunks |
| **V4 Live tap** | lease and focus, tap, view store, live source, SSE notices | the tests of §5.4 over 100 game days (lockstep, and under load: the tap stands down); with no lease, step time unchanged within noise; with a lease, the tap's time at or under its budget plus one part, and the encode time per part measured; the server thread's hand-over unchanged; on Windows, saves succeed while the sidecar reads |
| **V5 Surveyor base map** | reader spike (done 2026-10-21, against a copy of the OmegaSrv world: §9), then the renderer | the MICTest save's explored area draws at 1 block per pixel; a world without Surveyor falls back to HABITAT |
| **V6 Glass-box view** | after GLASSBOX G1: the compiled graph layer | the 12-factory corpus draws with rates matching the district's inspection |

**V1 as built** (feat/viewer, 2026-10-21), where it differs from the sections above:
- **No parts yet.** The save source draws mic-viewer scenes straight from the decoded save, once at
  open (`rtsim-view` `layers.rs`); a request at any `t` gets the save's scene. The part format
  (§4.3's `ViewSource`, `PartWriter`) comes with the live tap (V4), when a second producer first exists.
- **`rtsim-cli export` moved to V2**, with `rtsim-cli run`.
- **Flat frames.** The pages hold no topology, so every frame is flat with no wrap, a Deep Time
  world's too, until the sidecar reads its `manifest.json` (§9).
- **Names** come from Java's journal (`<world>/data/mic_rtsim_journal.dat`, its dictionary):
  dimensions and district names. Without it, `dim<id>`.
- **Messengers** are the news packets on roads (a packet with a band travels as the band); hunts are
  their own layer, a line from the dragon to its target.
- **Memory cap**: 1,024 MiB by default (`--mem-cap`, TUNABLE). A save whose decoded state is over it
  is refused at open, and district cards are refused once the heap reaches it. Cards are cached
  under 256 MiB, tiles under 64 MiB.
- **`/api/status`** (an extra route): each module's load status and state hash, each layer's mark
  count or why it is empty, the caches and the heap. An empty layer's reason is also on its
  description.
- **Measured** (the laptop, `rtsim-cli/tests/view.rs`): at 100,000 synthetic districts (the industry
  bench's, 100,001 pages, 20.3 MiB raw), the sidecar starts in 0.8 s, a full-world request returns
  250 marks at 40 bpp and 2,000 at 0.25 bpp in 11 to 19 ms, a district card takes 3 to 4 ms, and the
  process peaks at 52 MiB resident (heap 32 MiB). The Windows half of the exit test is not run yet.

---

## 17. Proposals for Ben

Each row: the question, the recommended option first, then the alternatives.

| # | Question | Recommended | Alternatives |
|---|---|---|---|
| VWD-1 | Where the HTTP server sits | **A sidecar process** (`rtsim-cli view`) reading the sim pages, a view tap the engine writes only while leased, and the archive (§3.4) | (b) in-process, a mic_native service in the server JVM: freshest, nothing to deploy, and plugins' layers with their code already loaded, but a listener thread breaks mic_native's no-threads rule and the server process carries sockets, tiles and parsing; (c) a sidecar on saves only: no viewer work in the engine at all, up to 300 s stale |
| VWD-2 | The shared repo's name | **mic-viewer** (`MinecraftIndustrialComplex/mic-viewer`, private, MPL-2.0), next to mic-native | (b) mic-map; (c) mic-worldview. (mic-atlas would be confused with Antique Atlas.) |
| VWD-3 | The base map on worlds that are not Deep Time's | **Surveyor's server-side terrain summaries** as an optional source, HABITAT underneath and as the fallback (§9) | (b) HABITAT only: no parser, 64-block pixels, sampled ground only; (c) render region files: every generated chunk, but heavy, and colours need a table exported from the game |
| VWD-4 | The event archive's retention and budget | **256 MiB hard cap in three tiers**: every row for 504 game days, notable rows and named movers' legs up to 7,200 game days, then counts per region per day, coarsened and dropped oldest first (§10.4) | (b) one tier, a ring of the newest rows within the cap; (c) a larger cap (1 GiB) with the same tiers; (d) no cap, compaction only |
| VWD-5 | The live update transport | **SSE** from Deep Time's `stream_events`: version notices, then fetches of changed layers; open streams are the lease (§11) | (b) polling every 2 s while the tab is visible; (c) WebSocket: nothing to reuse, more code |
| VWD-6 | The headless host's name and scope | **`rtsim-cli run`** (PLAN.md §4.11's name), beside `rtsim-cli view`; scope as §12: a fork of the pages, nobody online, Java's commands answered as failed, terminal controls; plus `rtsim-cli export` for consistent copies | (b) a separate binary, `rtsim-host`; (c) the same scope plus a "pretend Java" policy that acknowledges commands as applied (houses appear, transfers succeed), clearly a fiction |
| VWD-7 | Default exposure | **Loopback only on port 8740, reached by `ssh -L`**; tailnet only with `--allow-tailnet`; `--socket` (a Unix socket only the op can reach) on a server where others have shells (§13) | (b) loopback and tailnet by default, as Deep Time's code allows today; (c) the Unix socket always |
| VWD-8 | Whether the archive records with nobody watching | **Always on**, with its own config switch: history is the point of VW4, and its cost is a 32-byte push per archived event plus writes at saves | (b) only while a viewer is leased: free when unwatched, but no history from before someone looked; (c) off by default |
| VWD-9 | Players in the viewer | **Live positions only** (ops), no player tracks in the archive; events involving players (reports) are archived as they are | (b) also a coarse hourly sample of each online player's position in the archive; (c) no players at all |
| VWD-10 | The timeline's axis | **Game ticks**, with the wall-clock date under them (from each chunk's flush time) | (b) wall-clock time, with gaps where the server was stopped |
| VWD-11 | How read-only viewing is enforced | **`&self`-only view API plus the determinism test** (§5.4), and process isolation from VWD-1 | (b) process isolation only |
| VWD-12 | Build order | **V0 core with Deep Time, V1 save viewer, V2 headless host, V3 archive, V4 live tap, V5 Surveyor, V6 glass box** (§16); the host comes early for SRV1 and P9 | (b) the live tap before the host; (c) the archive first, so history starts accruing as soon as rtsim reaches Ben's server |
| VWD-13 | Bootstrapping the host from a world that never ran rtsim | **Not now**: the host needs pages from one run in Minecraft (§12.3) | (b) import villages and habitat from Surveyor's structure and terrain summaries |
| VWD-14 | Controls for the headless host | **Terminal only**; the page stays read only | (b) pause and speed buttons on the page, for the host source only |
| VWD-15 | The glass-box graph in the viewer | **After GLASSBOX G1**, as a deep-zoom layer (V6) | (b) a separate page per district |
| VWD-16 | Whether the lease may steer the tap | **Yes, a focus**: the regions open pages show at the Group level or deeper go first each cycle, and get Individual-level samples; it orders the tap's work and never reaches a module (§5.1, §5.2) | (b) no steering: round robin only, so freshness is the whole cycle (seconds or more at the bench maxima) and live mode has no Individual level (save and host only) |
| VWD-17 | Plugins' layers outside the server | **Self-describing parts** (live needs no plugin code), and for saves and the host, the plugin's cdylib loaded by an in-process mic_native host; until that works, plugin layers are live and host only (§4.3) | (b) plugin layers live only, always |
| VWD-18 | How rtsim's sidecar gets a Deep Time world's tiles (§14 item 3) | **A small `deeptime-tiles` crate** holding `BakeMap` behind `TileSource`; the sidecar links deeptime-core (Deep Time's default) | (b) tiles over loopback from `deeptime watch`: no compile-time dependency, but a second process on the server |
