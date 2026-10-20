# Scale: the late game with millions of things and more

**Status:** design for Ben's review, 2026-10-19. This document answers decision D6: plan first for
late-game load in huge worlds, with special attention to algorithms and data abstraction. Every other
design must fit inside it. Choices still open are marked **(Ben)** and gathered in §13.

## 0. The one rule

**The sim's cost per unit of time grows only with three things:**
- what players can observe;
- how many things happen (events);
- how many coarse cells and aggregates exist.

It never grows with how many things exist. Most of the world is never looked at, so most of the world
must cost nothing.

**Every system states its cost as a formula in those terms, and CI checks the formula at scale (§10).**

## 1. What grows in a late game, and what holds it

| Grows | With | Held by |
|---|---|---|
| Wildlife and populations | world size, time | the detail ladder and latent individuals (§2), lazy regions (§4) |
| Travellers: caravans, ships, dragons, war bands | settlements × connections | flows (§3) |
| Settlements and factions | world size, history | lazy regions (§4), the strategic tier |
| Player industry: districts, trains, packages, planets | play time, with no ceiling | the industry ladder (§7) |
| Memory and history: reports, sentiment, chronicle | time | compaction and caps (§6) |
| Bodies (planets, moons) | late game | partitions (§5), paging (§8) |
| Players | few | interest areas |
| Real Minecraft entities in loaded chunks (items, contraptions, mobs, puppets) | what players build and visit | not the sim's cost: puppet caps, plus the pack-wide optimization effort (PLAN.md §1a) |

## 2. Data abstraction: the detail ladder

**This is the centre of the design.** Every simulated kind (a sheep, a merchant, a ship, a district, a
settlement) exists at one level of detail at a time, per instance. Each level is a sufficient
summary of the levels below it.

| Level | Holds | Sheep | Caravan traffic |
|---|---|---|---|
| Individual | one row with full state | the ewe a player sheared and named | the caravan a player robbed |
| Group | one row: counts per cohort and their distributions | a herd: lambs, adults, old, with body condition (mean and spread) | one caravan on one leg |
| Field | counts per kind per grid cell | sheep density per 64-block cell | |
| Flow | rates on a graph edge | | caravans per day and goods per day on the road from Brindle to Oakford |
| Aggregate | totals per region or province | sheep per province | trade volume per province |
| Strategic | totals per faction or body | | balance of trade between cultures |

**The five operations every kind implements:**
1. **`fold`** moves an instance down a level (individual into its group, a group into its field),
   preserving every conserved quantity.
2. **`sample`** draws an instance from a level, seeded by (parent seed, slot). The same slot gives the
   same individual every time, until the state changes. This is why the same ewe with the same
   markings comes back.
3. **`query`** answers questions from whatever level currently exists, for example "how many sheep
   within 2 km of here" or "how much grain is on this road". **Systems only ever call queries, never
   storage.** So when data moves between levels, no system's code changes. This is the abstraction
   that makes every other section possible.
4. **`catch_up(t0 → t1)`** advances the state at a cost that does not depend on the elapsed time
   (§4).
5. **`next_breakpoint()`** returns when the instance's outputs next change in kind: a stock runs out,
   a threshold is crossed, a leg ends (§4).

**Invariants**, all checked by property tests in CI:
- Conserved quantities (counts, goods, money) are integers.
- `fold` after `sample` changes no total.
- `catch_up` composes (§4).

**Identity is derived, not stored.**
- An individual with no history that matters costs zero bytes; its name and look come from its seed.
- Only *notable* individuals are rows: those with a history a player could observe.
- Promotion to notable runs under a budget. Demotion is by importance.

**Prior art:**
- In Victoria 3 a pop is a group by design: a size, plus culture, religion, profession and location.
- Dwarf Fortress keeps historical figures as individuals and everyone else as abstract populations.
- Stellaris reportedly replaced individual pops with pop groups for late-game performance in its 4.0
  update. UNVERIFIED; being checked.

## 3. Flows: traffic without travellers

**What a flow is.** Unobserved, unremarkable traffic between two places is a *flow*: a rate of
caravans per day and of goods per day on an edge.
- A flow changes only when supply or demand at either end changes. That change is an event. A single
  traveller departing or arriving is not.

**When a player is on or near the edge,** the sim samples the travellers on the stretch they can see:
- identity from (edge seed, departure slot);
- cargo from the flow's goods mix;
- position from departure time and speed.

Travellers enter and leave the player's view at the flow's own rate, so the road is neither suspiciously
empty nor suspiciously full.

**A sampled traveller becomes an individual row** when something happens to it that must persist:
robbed, escorted, attacked, or talked to with consequence. That departure slot is removed from the
flow, so the totals stay exact.

**Cost:** events grow with the number of edges whose rates change, plus the individual travellers.
They do not grow with traffic volume. A road carrying 1,000 caravans a day costs the same as a road
carrying one.

**Encounters between flows** (pirates on a sea lane, a dragon over a road) are hazard rates on the
edge: losses equal rate × hazard, integrated in closed form. Individual fights happen only when a
player observes one, or when one side is already an individual.

## 4. Lazy regions: do not step what nobody reads

**There are two kinds of dynamics:**
- **Local processes inside a region:** forage regrowth, births and deaths, settlement growth,
  production.
- **Interactions:**
  - a caravan or raid arrives;
  - a flow rate changes;
  - a player comes near.

  These are events delivered to the region.

**The rule:**
- A region that nobody observes and that receives no events is not stepped at all. It records the time
  it was last evaluated.
- Any read of it (a query, an event, an observation) first catches it up to now.
- The scheduler wakes a region only at its processes' breakpoints: the moments its outputs change in
  kind. One example is "grain surplus crosses the caravan threshold at day X". So a region's effects on
  the rest of the world are known without stepping it.

**What catch-up costs.** Every local process must catch up at a cost that does not grow with elapsed
time:
- **Closed form:** logistic growth, exponential decay, and piecewise-linear production between
  breakpoints.
- **Matrix powers:** cohort ageing as a Leslie matrix raised to the elapsed number of steps, by repeated
  squaring, so O(log t).
- **A single draw:** for example, deaths at a constant hazard over n days are one binomial draw with
  probability 1 − (1 − p)ⁿ.
- **A process with none of these** runs on a cadence instead, and CI flags it as a scale risk.

**Determinism.** The result must not depend on *when* the region happens to be read.
- **The composition rule:** catch_up(t0 → t2) must equal catch_up(t1 → t2) applied after
  catch_up(t0 → t1). It is property-tested.
- **Random processes commit only at fixed slot boundaries** (for example, once per sim month). Each
  slot uses one draw from the counter RNG, keyed by (region, process, slot). A read in the middle of a
  slot sees the deterministic expected value and commits nothing. Lazy and eager evaluation therefore
  draw identical numbers, and catch-up over k slots costs k cheap draws.

**Cost:** the work done depends on the number of regions that have events, observers or breakpoints.
It does not depend on how many regions exist.

## 5. Partitions: space, bodies and threads

- **The spatial hierarchy:** body → dimension → province → region (512 × 512 blocks, as in Minecraft's
  region files) → cell.
  - Queries are answered at the coarsest level that can answer them.
  - Each province keeps its aggregates up to date incrementally whenever one of its regions changes.
  - How many regions make a province is open **(Ben)**, decided from the phase 0 bench.
- **The parallel step:**
  - The regions due in a step are processed in parallel.
  - Effects that cross regions are messages, applied at the next step in a fixed order: by region id,
    then by message sequence.
  - Results are therefore identical at any thread count (bulk-synchronous parallelism).
- **Bodies (planets) are separate partitions.** They are linked only by strategic events whose travel
  delay is at least one step. So they run in parallel trivially, and a whole body can be paged out.

## 6. Memory that does not grow with time

- **Reports expire into counters.**
  - A report keeps its full detail for a window that depends on its kind.
  - After that it folds into its holder's sentiment and into a counter per (actor, place, kind).
- **Chronicle compaction.** Old history segments are summarized into era records that keep only the
  notable events, ranked by importance. The full log goes to disk (Deep Time's event log), not to
  memory.
- **Every table has a cap** and an eviction rule based on importance: player involvement, then
  recency, then magnitude. Caps are configuration, sized from the memory budget (§8).
- **Path caches** are LRU and bounded.

## 7. Player industry, the one with no ceiling

The same ladder applies, and every level is event-driven:

| Level | What runs it | Cost |
|---|---|---|
| Loaded district | Create itself | Minecraft's, not the sim's |
| Unloaded district | a process model with measured rates; it wakes at breakpoints (an input runs out, a target is reached) | grows with breakpoints |
| District network | districts linked by trains and packages, treated as flows. It is solved as a linear flow network only when some district's rate changes. | grows with rate changes |
| Strategic | megaprojects (a swarm eating Mercury) as a few rows with closed-form growth | constant |

So 10,000 districts running at steady rates cost nothing per tick. Cost appears only when something
changes.

## 8. Bounded memory: paging

- **What is resident:**
  - **Always in memory:** the strategic tables, the settlement and notable indexes, the flow graph,
    and an index from id to region.
  - **Paged:** everything else lives in region pages, loaded only when needed.
- **Eviction:** pages nobody has touched for a while are dropped from memory. They are already on disk,
  because saves are incremental per page.
- **Loading:** a page is loaded when it is read, which always happens through catch-up anyway.
  - **Read-ahead** follows player movement.
  - **The timing wheel** says which regions wake next, so their pages are prefetched on the host's IO
    lane before they are needed.
- **Resident memory is a budget, not a function of world size (Ben).** On a potato the working set is:
  - each player's neighbourhood;
  - the regions with imminent events;
  - the strategic tables.
- **Pages are self-contained.** References across regions use stable ids resolved through the resident
  index, so evicting a page never means fixing up pointers. Pages are LZ4 + CRC32, using the shared
  codec.

## 9. Numbers and layout

- **Conserved quantities are integers.** Floats are used only for fields and rates. When a float rate
  produces whole items, the fractional remainder is carried forward (error diffusion), so totals stay
  exact over any time span.
- **Handles inside the engine** are a 32-bit index plus a 16-bit generation (6 bytes). 64-bit ids exist
  only at the boundary with Minecraft.
- **Rows are packed:**
  - **A report** is 32 bytes:
    - who and whom, 6 bytes each;
    - kind 2, severity 1, flags 1;
    - time as a 48-bit tick, 6;
    - frame 2, x and z as 24-bit values (3 each), y 2.
  - **A sentiment entry** is 8 bytes: a 6-byte handle, an i8 value, and 1 byte spare.
  - **A memory slot** is 8 bytes: a 6-byte handle, a u8 salience, and 1 byte spare.
- **Columns per kind** (structure of arrays): a system that touches one field streams only that field.
- **Distributions** are either fixed buckets (histograms) or moments (mean and variance), chosen per
  kind.

## 10. Complexity contracts and the scale bench

- **Every system declares its cost** in terms of the things it touches:
  - observed instances, events and breakpoints;
  - cells and individual rows.

  Its cost must never depend on:
  - the latent count;
  - flow volume;
  - total region count;
  - elapsed time.
- **`rtsim-cli bench scale`** builds synthetic worlds that vary one count at a time across orders of
  magnitude, holding everything else fixed. The counts are:
  - latent individuals;
  - flow volume;
  - settlements;
  - regions;
  - districts;
  - players;
  - elapsed time between reads.

  It measures CPU per sim day, resident memory, save bytes and catch-up latency.
- **CI fails when a measured growth slope exceeds the declared one.** Example: if doubling the number of
  latent sheep raises CPU at all, the system fails.
- **This bench is a phase 0 deliverable,** written before any content. Its targets are open **(Ben)**,
  in §12.

## 11. What is still hard

1. **The composition rule constrains model design.** Random processes must commit on fixed slots. The
   economy model (being researched) must fit both this rule and flows.
2. **The edge of observation.** Travellers sampled from flows must enter and leave convincingly. This is
   tested in GameTests that watch a road while a player approaches and leaves.
3. **Catch-up on arrival.** A player arriving sees the result instantly, so every catch-up must fit in
   the materialization budget. That is why catch-up cost must not depend on elapsed time.
4. **Market clearing in lazy regions.** Clearing needs prices from neighbouring regions, so prices
   travel as news and flows, never as live reads across regions.
5. **Minecraft's own entity load in loaded areas** is outside the sim. The sim's part is to keep far
   things abstract and to cap puppets; the rest belongs to the pack-wide optimization effort.

## 12. Galactic scale: whole planets on the ladder

At galactic scale the ladder holds whole worlds as well as the things on them.

| Level | What a planet is | Cost |
|---|---|---|
| Latent | derived from the galaxy seed; no row | 0 |
| Strategic row | a body: its orbit, owner, population totals and output | about 128 B (budget) |
| Coarse world | a province grid, with history at province level | kilobytes to a few MB (Deep Time session) |
| Full world | a Deep Time bake, a Minecraft dimension and sim region pages | about 100 MB of terrain data per 16k planet with the timelapse dropped, or about 210 MB keeping one keyframe per 50 Myr; Minecraft region files come on top |

Every figure in this section comes from the Deep Time session's measurements of 2026-10-19 (master
`db37a097`, the play world and the test VM).

**Coarse and full must agree, and a low-resolution bake cannot provide that.**
- Deep Time's history is chaotic. Changing the sphere sample count, the L1 grid, the compute path (GPU or
  CPU) or the build changes the continents.
- The same seed, preset, build and compute path always gives an identical world. So the rule is: store
  what was computed, and never regenerate it from the seed.
- **Three ways to get consistency (Ben):**
  - **A. Run the real history for every planet,** and keep only a summary until someone visits.
    Consistent, but the history is most of a bake's cost, so thousands of planets are out of reach.
  - **B. Make a coarse history authoritative.**
    - Unvisited planets run a cheap coarse-grid history.
    - A visit runs only the refinement on the stored state: amplification, L2 and L3 detail, strata,
      rivers and deposit sites. Consistent by construction.
    - Deep Time's presets already work this way across sizes: quick 16k and quick 32k share one history
      grid.
    - Two things are unmeasured: how coarse the history grid can go, and how a run's time splits between
      history and refinement.
    - The Deep Time session recommends B once those are measured.
  - **C. The rtsim invents province-level planets,** and a visit runs Deep Time constrained to match
    them. Approximate at best; pinning a plate history to a given map is research.
- **Resource sites come out of the history,** so they stay consistent only under A or B.
- **Under B,** the sim's history of an unvisited planet runs on Deep Time's coarse grid, and the sim's
  provinces are that grid.

**Baking a planet before anyone arrives:**
- **How long a 16k planet takes over 1,000 Myr:**
  - 826 to 955 s on the laptop with 8 threads;
  - 1,290 s on the quiet VM;
  - Deep Time's target is 900 s on an average gaming rig at normal priority.
- **In the background,** beside a running game on a few threads at the lowest normal priority, expect
  two to three times longer: 30 to 45 minutes. Either the trip lasts that long, or the planet shows a
  "still forming" state on arrival. The bake checkpoints, so it can pause and resume.
- **Peak memory is 3.2 GB** (quick 16k, 4 threads) **to 4.2 GB** (full 16k, 8 threads), on top of the
  game. On a potato this is the binding limit. In-game bakes may have to be dedicated-server only, or
  wait until the player is not playing **(Ben)**.
- **The compute path is fixed per planet and stored with it.** CPU is the default for in-game bakes,
  because the game already loads the GPU and GPU and CPU runs give different worlds.
- **Several players heading to different planets** means a bake queue.

## 13. Questions for Ben

Asked in chat on 2026-10-19; answers go into DECISIONS.md.

1. **Scale targets** for the bench and the design.
2. **Memory policy** for the sim's resident memory.
3. **Lazy regions:** catch-up on read, or eager coarse stepping.
4. **Flows:** unobserved traffic as flows, or every traveller always an individual.
5. **Planet consistency at galactic scale:** option A, B or C in §12.
6. **In-game planet bakes on weak PCs:** dedicated-server only, run while the player is not playing, or
   allowed with a warning.
