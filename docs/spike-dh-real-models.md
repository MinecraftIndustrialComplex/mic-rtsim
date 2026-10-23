# Spike: the far view draws the real carriages and the real dragons

Branch `spike/dh-real-models`, 2026-10-22 and 23. Ben's complaints: "Trains don't match the trains players built
they are just black boxes" and "Dragons look like a blocky mess and not like dragons." Ben approved the plan
("Looks good, go do some tests and implement if they look good"): trains drawn as the real carriages, dragons as
pictures far away and Ice and Fire's real model for the nearest few, inside Distant Horizons' own pass.

**Verdict: pass, for both.** The full build goes ahead on `feat/dh-real-models`. One change from the plan: far
dragons are not pictures (impostors) but Ice and Fire's real model baked once into a short flight loop of meshes
(below, "Dragons"). The pictures were tried and looked worse than the baked model, which costs less to draw than
the live model by a factor of about 30.

The spike module (`neoforge/dhspike`, built only with `-PdhSpike`) stays on branch `spike/dh-real-models` and is
not merged: it has its own test carriages, its own shape payload and a director for screenshots. The full build
ports what worked into the core, mic_rtsim_create, mic_rtsim_iceandfire and mic_rtsim_dh
(docs/choices-dh-feed.md DHT60 on).

## Media

All under `/home/greencheetah/mic-media/dh-spike-2026-10-22/`:

- `shots/compare/`: before and after side by side (31 PNGs). `cNN_*` are trains (today's DH boxes on the left,
  the real carriages on the right), `gN_*` and `g_overview_*` are dragons (boxes, then the baked poses, and the
  `_live_vs_baked` files put the live Ice and Fire model beside the baked one), `h1_*` is a dragon behind a ridge,
  `f1_*` the fog at 1,000, 1,600 and 2,200 blocks.
- `raw/grab/`: every screenshot, as taken.
- `raw/ctl/`: the bench, bake and size tables (`spike-bench.csv`, `spike-bench2.csv`, `spike-bake*.csv`,
  `spike-sizes.csv`) and the impostor atlases that were rejected.

Names say what each shot is: `t2_diag_d700_zoom10` is test train 2 on diagonal track, 700 blocks away, at a zoom
of 10 degrees field of view (a spyglass look; the player's own 70 degree view is in the `fov70` shots).

## How it was tested

A dev client (`:dh:runClient`) in a private headless compositor, a locally generated test world, Create 6.0.10,
Ice and Fire CE 2.1-beta.1 and DH 3.3.3. Session 1 used vanilla's renderer, session 2 added the pack's Sodium
0.8.13. Four test carriage designs (a locomotive of 123 blocks, a wooden coach, a copper one and a glass one)
on straight track, broadside diagonals, 45 degree diagonals and curves, at 300 to 1,000 blocks. Twenty dragons
of all three species, four colours and stages 4 and 5, flying a ring at 300 to 1,000 blocks, then behind a
ridge and in the fog. The bench draws 600 frames per row.

## Trains

What was drawn: the carriage's blocks, sent once as a compact shape keyed by a 64-bit content hash, baked on the
client into one static mesh per render type (solid, cutout, cutout mipped, translucent) by Minecraft's own block
renderer, plus Create's bogeys from Create's own bogey renderer. The mesh is drawn inside DH's pass at the
carriage's true yaw and pitch, with DH's projection, DH's depth test and DH's fog.

- **Looks**: at 300, 600 and 1,000 blocks the carriages are plainly the player's build, on straight track,
  diagonals both ways and curves (c01 to c11). The boxes beside them are grey slabs.
- **Hills and buildings**: a hill hides the whole train in both modes (c10); a house hides part of one (c09).
- **Fog**: matches DH's terrain at 1,000, 1,600 and 2,200 blocks (f1), the same as the boxes.
- **Flicker**: none seen in any shot or while the camera moved.
- **Wire**: 85 to 120 bytes per carriage design deflated (91 to 123 blocks), once per design per player. Create's
  own entity NBT for the same carriages is 1,073 to 1,247 bytes gzipped.

| Draw (session 1, vanilla renderer) | carriages | CPU ms/frame | GPU ms/frame | vertices |
|---|---|---|---|---|
| boxes | 20 | 0.074 | (in DH's pass) | |
| real carriages | 20 | 0.062 | 0.040 | 154,496 |
| boxes | 50 | 0.109 | (in DH's pass) | |
| real carriages | 50 | 0.084 | 0.082 | 386,240 |

Bake (session 2, with Sodium), spread over frames on a 2 ms budget per frame: a carriage takes 3.7 to 8.8 ms of
tessellation over 3 to 5 frames, 0.2 to 0.3 ms of upload, and 1.6 to 2.1 ms for its bogeys. The **first** bogey of
a session costs about 189 ms in one frame (Create and Sodium warming up), a visible hitch. The spike's fix bakes
every bogey style and size once at login (`ShapeCache.prewarmBogeys`); it is built but not yet measured in a client.

## Dragons

Three ways were tried:

1. **Live stand-ins**: a client-only Ice and Fire dragon entity, never added to a level, posed and drawn through
   Ice and Fire's own renderer. Right in every way, but 12 ms of CPU a frame for 20 dragons (session 2).
2. **Impostors** (the plan's pictures): the live model rendered into an atlas of 16 headings by 3 elevations.
   0.12 ms for 20, but at a spyglass zoom they were flat and fuzzy and the wingbeat was lost. Rejected
   (`raw/ctl/impostor-atlas-*.png`).
3. **Baked poses**: the live model rendered once per species, colour and stage into 12 frames of Ice and Fire's
   flight loop, each frame a static mesh, then played as a flipbook. Visually the same as the live model
   (`*_live_vs_baked`), right species and colour, wings beating.

| Draw (session 2, Sodium) | dragons | CPU ms/frame | GPU ms/frame | vertices |
|---|---|---|---|---|
| boxes | 20 | (DH's pass, 1.6 ms) | 0.017 | |
| live stand-ins | 20 | 12.0 | 0.094 | 96,487 |
| baked poses | 19.5 | 0.36 | 0.078 | 96,215 |

Bake: 7 to 67 ms per pose set in total, spread over frames on a 2 ms budget; about 2 MB of vertex buffers per set
(57,000 to 62,000 vertices).

- **Ridge**: a dragon behind a ridge at 1,600 blocks is hidden by it (h1).
- **Fog**: the same as the boxes at every distance (f1 to f4).
- **The switch**: the plan's "real model for the nearest few" stays: the nearest 2 within 300 blocks are drawn
  live, everything else from the baked poses. In the bench all 20 dragons were beyond 300 blocks.

The stand-in entity is client-only and never added to a level (D3): it exists to be posed and rendered, for the
bake and for the nearest two.

## Shaders

The pack has no Iris, Oculus or shader packs. Sodium works (session 2). A shader pack would need its own test.

## What the full build needs on the wire

- In the far view's INTRO: a carriage's shape hash (0 for none), and a dragon's age (for its size between stages).
- A new optional payload carrying a shape (its hash and the deflated bytes), sent once per hash per player before
  the message that first names it.

## Caveats found

- **Block entity renderers** are not drawn, except Create's bogeys. A carriage built of chests, signs or banners
  shows those blocks' plain models, or nothing for blocks with no model.
- **Light**: everything far is lit at full sky light with no block light, as DH's boxes are.
- **Flipbook**: 12 frames for a 3 second wingbeat; still to be judged in a moving clip.
- **Mip fringes**: cutout mipped leaves (none in the test carriages) may thin out at 1,000 blocks.
- **Eviction**: the spike dropped a pose set 600 frames after its last use, which re-baked sets that came back into
  view a little later. The full build evicts by time.

## Traps

- After a camera jump of 700 blocks the test server lags several seconds, so a screenshot taken when the client's
  queue empties can catch the old mode. Wait for the server's echo of each command.
- A fixed wait after a bench command races a low frame rate.
- `opt fov` clamps at 30; the spike's zoom sets the field of view through `ViewportEvent.ComputeFov`.
- Create's first bogey draw costs about 180 ms with Sodium.
