# The habitat sampler's cost

The core's habitat sampler (`neoforge/core`, package `habitat`; docs/boundary.md §10; choices X2-# in
docs/overnight-choices-worldmodel.md) reads one point per chunk, 16 per 64-block cell, on the server
thread at the end of a tick, under a budget of 8 cells and 0.25 ms a tick (PLACEHOLDERS).

## 2026-10-20, the GameTest bench (`habitatSamplerCost`, `./gametest.sh core -PrtsimBench`)

**Conditions.** Branch `overnight/worldmodel` at `6069fa9` plus the bench test, the native engine, a
headless NeoForge 21.1.249 GameTest server (Java 21, `-Xmx3G`) at nice 10 on the laptop (AMD Ryzen AI
MAX PRO 390, 24 logical CPUs), shared with other sessions (1-minute load about 5). The GameTest world
is superflat plains (the FLAT preset): every point reads one biome, so the biome facts are cached
after the first; a real world has more biomes per cell, each read once per session (the cold figure).

**Method.** A 32 x 32 chunk area far out (64 cells, a quarter of them painted water at their points)
generated and loaded (forced); every cell read 50 times exactly as the sampler reads it (16
`getChunkNow`, the `MOTION_BLOCKING_NO_LEAVES` top and its block state, `OCEAN_FLOOR` under water, the
noise biome, the biome facts, the aggregate and the fingerprint), warm, nothing written; then one
read with the biome facts forgotten. The chunk-load hook's figure is the sampler's own counter over
the whole GameTest run.

| Measure | Result |
|---|---:|
| Cell reads | 3,200 |
| Per cell, mean | 8.50 µs |
| Per cell, p50 / p90 / p99 | 5.89 / 13.03 / 56.13 µs |
| Per cell, worst | 133.24 µs |
| **Per chunk (a cell's mean / 16)** | **0.53 µs** |
| A cell with the biome facts cold (31 tag checks and the spawn list, once per biome per session) | 229.41 µs |
| The chunk-load hook, per chunk load (1,897 loads) | 1.32 µs |

For scale: generating and loading those 1,024 flat chunks took Minecraft 30.8 s on the same thread
(30 ms a chunk); the sampler adds about 0.5 µs a chunk to read it, 1.3 µs a chunk for the load hook,
and nothing at all for a chunk whose cell is not complete yet.

**In the suite** (`habitatIsSentForAGeneratedArea`, the first cells of a fresh session, cold JIT and
cold biome facts): 11 to 29 cells sampled at 128 to 178 µs each on average, worst 0.96 to 1.44 ms; the
chunk-load hook 2.1 to 3.5 µs a load. The worst case is the first cell after a session start (the
biome facts and the JIT); the budget starts no new cell past 0.25 ms in a tick, so one tick spends at
most 0.25 ms plus one cell.

**What crosses the boundary.** A HABITAT is 80 bytes plus 8 per herd species the cell supports (112
for a plains cell, 80 for open sea); a cell crosses once, then only when its fingerprint changes.
HABITAT_KNOWN at a session start is 32 bytes plus 8 per known cell (at most 544 a region).

## The engine side

Measured with `cargo test` on the same laptop (release for the decision cost, at nice 10):

| Measure | Result |
|---|---:|
| A cell slot in memory (`Option<Habitat>`) | 72 bytes |
| A fully sampled plains region in memory (64 cells, their creature entries) | 6,208 bytes |
| Its page | 4,757 bytes |
| The meta page (one biome, four bodies) | 110 bytes |
| A herd's decision on the world model, before the cache (`decision_cost_on_the_world_model`: 16 sheep herds, the strait world, two sim years) | 1,450 µs |
| The same with the forage-cell cache (each cell read once per model revision) | 507 µs |
| The same on the uniform climate (Y3) | 853 µs |

So a region explored in full costs about 6 KB in the engine: 1,000 regions (a 16 km square) about
6 MB, paged per region on save (only the regions changed since the last save are written). A herd's
decision first cost 1.6 times what it did on the uniform climate, from reading the world model (four
habitat cells under the model's lock) for every forage cell its flow field touches, several times
each; caching each forage cell's reading until the model changes brought it to 0.6 times (the sea and
the unknown ground also close cells the uniform climate let the flow field explore).
