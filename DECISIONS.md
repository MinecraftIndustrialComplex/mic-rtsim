# mic-rtsim decisions

Every decision about mic-rtsim, newest at the bottom.

- **Who decided:**
  - **Ben:** his words are quoted exactly.
  - **Agreed:** settled with another session (e.g. Deep Time), with the reference.
  - **Proposal:** Claude's suggestion, not yet decided. Ben has asked to be asked about every choice, so
    every proposal goes to him before work depends on it.
- **Superseded:** an entry stays in the log; the entry that replaces it says so.

## How Ben wants this built

| # | Date | Decision | Who | Ben's words |
|---|---|---|---|---|
| D1 | 2026-10-19 | Judge every engine and architecture choice only on late-stage runtime performance on a weak PC. Ease of writing and ease of distribution do not count. | Ben | "Place no concern in distribution ease or initial writing ease and only on pure late stage mod performance, I want all of this stuff to be buttery smooth even on a potato PC." |
| D2 | 2026-10-19 | Ben is asked about every non-trivial design or content choice, even reversible ones. | Ben | "please keep me as in the loop about the development of this as possible to avoid it becoming slop"; then chose "Ask on every choice" |
| D3 | 2026-10-19 | The rtsim is backend infrastructure for emergent gameplay. It adds no bespoke entities, blocks or items unless absolutely necessary. Bodies, structures and content come from other mods. | Ben | "I ideally (unless absolutely necessary) want to avoid this mod having it's own bespoke entitites, when possible look into other mods that could provide blocks / in game content. I want this mod to primarly serve as backend infrastructure for emergent gameplay." |
| D4 | 2026-10-19 | Nothing in the core depends on a pack mod. A plugin may be built around one mod. | Ben | "In general, don't gate stuff behind mods in this pack. (Especially cobblemon, I honestly think I'm going to be removing cobblemon from the pack sooner rather than later.)" and "Tho, individual plugins can be based around a mod. I.E. The dragon raids mod can & probably should be based around Ice & Fire." |
| D5 | 2026-10-19 | Ribbits gets extra high priority. | Ben | "Also, give the ribbits mod extra high priority for this (It's one of my favourite mods.)" |
| D6 | 2026-10-19 | Planning for late-game load in huge worlds with millions of entities or more starts first. Algorithms and data abstraction get special attention. | Ben | "we need plans extremely early on for how to actualyl handle late late game load for absolutely fucking massive worlds with millions or more entities." and "What is the current plan to deal with this in terms of algorithms, you also need to be paying special attention to data abstractino, etc." |
| D7 | 2026-10-19 | NPC bodies should look and feel like Minecraft mobs, not players. Player-skinned humanoid NPC mods are not the default body source; each phase picks mob-like bodies. | Ben | "In general, really not a big fan of NPC's that look / feel like actual minecraft players instead of minecraft mobs." |
| D8 | 2026-10-20 | Design depth is not a risk to guard against. mcrtsim failed because of the weak models of its time, not because of its ambition; Deep Time shows a design this big can be built. Phase 0 is not to be rushed as a hedge against over-design; when it starts is still open. | Ben | "mcrtsim got killed by being done by shitty models, not this. If you need proof that this works now, look at Deeptime. (Ludicroius ambition, and a shit ton of code.)" |
| D9 | 2026-10-20 | Overnight: build as much as possible with no limit; build general scaffolding once the planned work is done; things that later get deleted are fine. | Ben | "As much as you can, don't set a limitation, if you build everything you want to start planning and building generalized scaffolding, don't be afraid to build things that will get deleted. I will re-visit in the morning." |
| D10 | 2026-10-20 | While Ben is away, reversible choices take Claude's recommendation and are logged for his review. Irreversible or outward-facing choices wait. | Ben | chose "Take my rec, log it" |
| D11 | 2026-10-20 | Agents may use the machine's resources freely. The only other session that should be busy is Deep Time, which may run GPU tests, and it messages before it does. | Ben | "feel free to use any of my system resources you want to. The only other agent that should be doing anything on them might be deeptime (It wants to run some GPU tests at some point.)" |

## Architecture

| # | Date | Decision | Who | Ben's words |
|---|---|---|---|---|
| A1 | 2026-10-19 | A core library plus content mods, not a monolith. One monorepo of Gradle subprojects; each mod is its own jar and mod id. | Ben | "I'm ideally like to create a library that can have other mods layer into it for adding future features instead of a monolithic mod"; chose "Core lib + content mods" |
| A2 | 2026-10-19 | The rtsim is licensed AGPL-3.0. Anything shared with Deep Time goes into a common library that we negotiate. Deep Time may later run on the engine. | Ben | "AGPL :D, it's a mic mod. If you need some stuff from deeptime, then go negotiate creating a common shared lib. I wouldn't be opposed to deeptime becoming a mod written ontop of a core sim engine." |
| A3 | 2026-10-19 | The world is a finite map. History (nations, evolution) is to be integrated with Deep Time. | Ben | "Finite map, we might want to integrate into deeptime for adding histories of worlds. (Nations, evolution, etc.)" |
| A4 | 2026-10-19 | The engine core is written in Rust. | Ben | chose "Rust core" after the performance-only comparison (PLAN.md §2) |
| A5 | 2026-10-19 | The rtsim must fit the pack-wide, agent-driven optimization effort. | Ben | "I am thinking about using agents to optimize the shit out of all of the mods in the pack. (Potentially merging mods, creating more shared libraries, re-writing mods / parts of minecraft in rust to be more performant.) In order to allow for more ambitious stuff in the pack, make sure that what we are doing here can play nicely with it." |
| A6 | 2026-10-19 | `mic_native` is the one native host for every native mod in the pack, including the optimization effort's rewrites. | Ben | chose "Yes, pack-wide" |
| A7 | 2026-10-19 | The host and the shared crates are licensed MPL-2.0. | Ben | chose "MPL-2.0" |
| A8 | 2026-10-19 | Deep Time may require `mic_native`; it does not need to stay fully standalone. | Ben, relayed by the Deep Time session | "Deeptime doesn't need to be 100% standalone mic_native + deeptime is completely fine to ship." |
| A9 | 2026-10-19 | The sim and Minecraft stay consistent through a journal in Minecraft's own save. Cross-store transfers carry operation ids, so a replay never applies twice. | Ben | chose "Journal in world save" |
| A10 | 2026-10-19 | A creature a player takes (lead, taming, Carry On pickup, capture) leaves the sim and becomes an ordinary entity. | Ben | chose "It leaves the sim" |
| A11 | 2026-10-19 | The potato reference is the laptop pinned to 2 physical cores, plus runs on a Windows PC. | Ben | chose "Laptop + a Windows run" |
| A12 | 2026-10-19 | The code lives in a private GitHub repo, `MinecraftIndustrialComplex/mic-rtsim`. | Ben | chose "Private GitHub repo" |
| A13 | 2026-10-19 | Unobserved traffic (caravans, ships, migrations) is simulated as flows on edges. Individual travellers are sampled only where a player can see, and become rows only when something happens to them. | Ben | chose "Flows" |
| A14 | 2026-10-19 | At galactic scale, an unvisited planet runs a cheap coarse Deep Time history that is authoritative. A visit runs only the refinement on the stored state (option B in SCALE.md §12). Deep Time does not plan for it yet, but no architecture may sabotage it: history and refinement stay separable, the history grid resolution stays a preset parameter, and a computed planet is stored data, never regenerated from its seed. | Ben | chose "B: coarse history"; to the Deep Time session: "Option B, Deep time will need a lot of work for this, don't plan around it for now. (It's months out.), though don't make architectural decisions that will sabotage it." (Deep Time decisions log, master 2885b5d9) |
| A15 | 2026-10-19 | Regions nobody reads are lazy. They are not stepped, and they catch up in closed form when read or at breakpoints. Travellers still move on scheduled legs: a leg is booked into a region and time-window index at its start, and a player entering a region looks up the legs booked there, so crossings become scheduled events with no scans. | Ben | chose "Lazy, catch up on read" after asking how a world-spanning dragon migration would still cross his house |
| A16 | 2026-10-19 | Resident memory is a configured budget, with the default set from phase 0 measurements. Everything else lives in region pages on disk, loaded on read with prefetch. | Ben | chose "Budget + paging" |
| A17 | 2026-10-19 | New planets are baked in the background only when free RAM allows (3.2 to 4.2 GB peak). Otherwise the planet stays "still forming" and cannot be landed on until it is baked. | Ben | chose "RAM check" |
| A18 | 2026-10-20 | The native host lives in a new private repo, MinecraftIndustrialComplex/mic-native (MPL-2.0), started from a copy of Deep Time's native-loading code. NOTICE.md records the copy and the relicensing by the copyright holder. | Ben | chose "New private repo" |
| A19 | 2026-10-20 | Names (P1): repo mic-rtsim; core mod id mic_rtsim; plugins mic_rtsim_<mod>; native host mod id mic_native. | Ben | chose "Use these names" |

## Content

| # | Date | Decision | Who | Ben's words |
|---|---|---|---|---|
| C1 | 2026-10-19 | The first slice was "slights and memory". Superseded by C6. | Ben | chose "Slights and memory" |
| C2 | 2026-10-19 | No villager or civilian bodies for now. | Ben | "tbh, don't need villagers rn, that is something to consider later on." |
| C3 | 2026-10-19 | Dragons remember slights. They are built on Ice and Fire and become phase 2 under C6. | Ben | chose "Dragons" |
| C4 | 2026-10-19 | Wanted: wandering traders and trader caravans. | Ben | "Wandering traders, trader caravans, etc. (Best entry point imho, but I might be wrong.)" |
| C5 | 2026-10-19 | Wanted: sky ships between villages, sky pirates, and sea ships between villages with ports. | Ben | "Skyships going between villages, sky pirates, seaships between villages with ports, etc." |
| C6 | 2026-10-19 | Phase 1 is caravans between vanilla and Ribbits villages, with memory of slights and news carried by caravans. Dragons follow in phase 2. | Ben | chose "Caravans first" |
| C7 | 2026-10-19 | The sim drives four Ribbits behaviours: merchant caravans, touring bands, sorcerers who remember, and village growth from YUNG's own house templates. | Ben | chose all four |
| C8 | 2026-10-19 | The sim owns all wandering traders. Vanilla's random trader spawning is turned off. | Ben | chose "Sim owns them" |
| C9 | 2026-10-19 | Each culture has its own currency. Prices follow local resource availability. Trade between villages is driven by a real economy simulation; the model is chosen at the phase 1 design gate. | Ben | "Per Culture + Local Resource Availability, ideally also be trading between villages based upon real economy sims." |
| C10 | 2026-10-19 | Later: kaiju that start spawning after players gain access to nukes, as late-game bosses. | Ben | "eventually I want to start adding in Kaiju to the pack. (Monsters that start spawning in after players have gotten access to nukes, that serve as late game bosses." |
| C11 | 2026-10-19 | Phase 1 caravans use vanilla and Ribbits bodies only: wandering traders, trader llamas and ribbit merchants. No mods are added to the pack for phase 1. Better Caravans, Goblin Traders and Easy NPC were offered and declined for now. | Ben | chose "None for now" |
| C12 | 2026-10-19 | Phase 1 caravans have no guards. | Ben | chose "No guards yet" |
| C13 | 2026-10-19 | The phase 1 economy is cost-anchored storehouse markets joined by news-driven flows (research/economy-2026-10-19.md). Price is a pure function of stock, anchored to labour cost from recipes and local resources. Transfers are integer and two-sided, markets are piecewise-linear between breakpoints, and prices travel only as news. | Ben | chose "Storehouse markets" |
| C14 | 2026-10-19 | Where the sim runs a market, vanilla villager trades draw from and pay into the village treasury and stock. A poor village cannot pay much, and prices follow its market. | Ben | chose "Through the treasury" |
| C15 | 2026-10-19 | A parallel workstream: simulate distant player factories and supply lines across Create and every addon in the pack, without relying on chunk loaders. Chunk loaders can still be used alongside. Design in INDUSTRY.md. | Ben | "Our pack is in desperate need of a way to simulate distant factories, etc, across create & all of the addons we are using other than chunk loaders. (Though chunk loaders can be used in tandem.)" |
| C16 | 2026-10-19 | Bandits, dragons, natural mobs and bosses attack player-made supply lines and factories, even while the player is AFK. The goal is to make players build infrastructure that secures their bases, and infrastructure that rebuilds lost ones. This replaces the old plan's proposal that raids on player bases wait until the owner is online. | Ben | "having bandits, dragons, natural mobs, bosses, etc, attacking player made create supply lines / factories, etc. Even when they are AFK to encourage players to actually build infra to secure their bases, and infra to re-build bases that might have been lost." |
| C17 | 2026-10-19 | On dedicated servers, attacks also hit bases whose owners are logged off. Threats follow the world, not the owner, and the defences built decide the outcome. | Ben | chose "Yes, offline too" |
| C18 | 2026-10-19 | Attacks on unwatched infrastructure are resolved abstractly. Real changes are applied the next time the chunks load: stock stolen from boundary inventories, and machines and blocks destroyed, bounded per attack. | Ben | chose "Real, applied on load" |
| C19 | 2026-10-19 | Distant factories are a black box by default: items, fluids and energy crossing the district boundary are measured while loaded, replayed while unloaded, and reconciled into boundary inventories on load. Per-mod plugins (Create factory gauges, PowerGrid) add precision. | Ben | chose "Black box + plugins" |
| C20 | 2026-10-20 | The sim writes slights and fair trades into vanilla villager gossip, so vanilla's own price changes and iron golems (at reputation -100 or lower) carry the consequence (P19). | Ben | chose "Write into gossip" |
| C21 | 2026-10-20 | Caravan cargo rides physically in the trader llamas' chests. Killing a llama drops it, and the loot is a journaled transfer (P20). | Ben | chose "In the chests" |
| C22 | 2026-10-20 | For now a village joins the sim when its structure start generates. Later, the existence of villages and much of structure generation will probably move into the sim (P22). | Ben | "For now when generated, eventually we will probably move the existence of villages, and much of structure generation into our system." |
| C23 | 2026-10-20 | Ribbit merchants' trades and restocks go through the ribbit village's treasury and stock, as villagers' do under C14 (P23). | Ben | chose "Through the treasury" |
| C24 | 2026-10-20 | Adopted from mcrtsim (research/mcrtsim-mining-2026-10-19.md): predator havens (stolen goods flow to the raider's home market and are sold there, so totals stay conserved); heirs inherit grudges (notables age and die on one scheduled event, and an heir or home inherits their strongest memories); threats get bored (a scheduled departure, brought forward by damage or hunger); rescues count (saving a caravan gives a positive report). | Ben | chose all four |
| C25 | 2026-10-20 | Also adopted from mcrtsim: temperament derived from each notable's seed weights its choices; a server config switch per module (turning off trade or travel restores vanilla trader spawning); groups (trader and llamas, rider and mount, ship and crew, band and master) materialize, capture and despawn as one unit. | Ben | chose all three |
| C26 | 2026-10-20 | Every player base is a market node that caravans visit, buy and sell at, and carry prices from as news. | Ben | chose "Every base" |

## Agreed with the Deep Time session

| # | Date | Agreement | Reference |
|---|---|---|---|
| X1 | 2026-10-19 | Deep Time's world-creation and first-start bakes run on the host's foreground pool. Play-time terrain columns and Distant Horizons sections stay as direct calls on the calling thread. | Deep Time session messages, 2026-10-19 |
| X2 | 2026-10-19 | In the event log, `t` is years relative to world creation. Negative means before creation and positive means after; the sign never says which writer. Pre-play civ history uses negative years, written as writer 2 (`civ:history`). | Deep Time master `c2476ea4` |
| X3 | 2026-10-19 | NativeLoader, `cpuprobe` and `tools/natives` move into `mic_native`, with no copy left in Deep Time, after Deep Time's P1-19 gate closes and its PGO and allocator work lands. The host needs per-job enter and exit hooks, and must replace or feed Deep Time's own memory cap. | Deep Time session messages, 2026-10-19 (following A8) |

## Open proposals (Claude's, waiting for Ben)

These are in PLAN.md but not decided. Each one goes to Ben before work depends on it.

| # | Proposal | Needed by |
|---|---|---|
| P1 | Decided as A19 (names). Still open: whether plugins carry only Java and data, with all rules in generic modules (PLAN §1.1). | phase 0 |
| P2 | The sim calendar is Serene Seasons' calendar, or mic-climate's `YearClock` in Deep Time worlds. Phase 1 does not need it. | herds, growth |
| P3 | The engine owns the truth. Minecraft entities are puppets that are never saved by vanilla. | phase 0 |
| P4 | Agent storage: `bevy_ecs` behind a facade, or our own columns. To be settled by the phase 0 benchmark, with the numbers brought to Ben. | phase 0 |
| P5 | Unit of abstraction for factories: one Create 6 logistics network; remote cargo moves as packages. | industry |
| P6 | History is generated at world creation, after Deep Time's bake when one is present. | history |
| P7 | A patch ledger plus contract tests for every reach into another mod. | phase 0 |
| P8 | Which mods supply bodies, structures and ports for later phases. Survey candidates (2026-10-19): Small Ships and Create Aeronautics Structures (ships), the Alex's Caves port (kaiju), Goblin's Tyranny (goblins), Illager Invasion (raiders), Naturalist (herds). D7 rules out player-like NPCs. | each later phase |
| P9 | Economy details left open by C13: the tunables r, T and k; a Numismatics mint rule (later). | phase 1 design gate |
| I1 | How Create schematics and the Schematicannon fit rebuilding: whether the sim records a schematic snapshot per district. Ben chose "Decide later". | industry threats |
| P10 | The fixes from the 2026-10-19 adversarial review that change the design, listed in PLAN.md. | phase 0 |

## Overnight choices (Claude, for Ben's review)

Reversible choices made while Ben was asleep (2026-10-20), each with the reason. None is final until
Ben reviews it; each can be undone.

| # | Date | Choice | Why |
|---|---|---|---|
| O1 | 2026-10-20 | Toolchain copied from Deep Time: `flake.nix` with rust-overlay (Rust 1.98.1, JDK 21), a matching `rust-toolchain.toml`, a `nix flake check` that fails when they drift, and `flake.lock` started as a copy of Deep Time's lock. No cross targets yet. | One nixpkgs and one toolchain in the store for both repos; the natives' cross-build belongs to `mic_native`. |
| O2 | 2026-10-20 | The counter RNG is Philox4x64-10 (Salmon et al., SC '11): key (world seed, system), counter (entity, slot, block, 0). System ids are the FNV-1a hash of a name. | Stateless, so any slot's draws come straight from their coordinates and thread count never matters; known-answer vectors checked against numpy 2.4.4. Hashed names mean adding a system never shifts another's numbers. |
| O3 | 2026-10-20 | Handle generation 0 is never live (six zero bytes mean "nobody"), and a slot retires at generation 65,535 instead of wrapping. | A zeroed row in a page must not name a real row; a wrapped generation would let a long-stale handle validate again. |
| O4 | 2026-10-20 | A lazy region's state re-anchors only at slot boundaries, events and breakpoints. Reads project without committing, and never reschedule a breakpoint. | This is what makes catch_up(t0 → t2) equal catch_up(t1 → t2) ∘ catch_up(t0 → t1) bit for bit, floats included, and keeps a region's trajectory independent of who reads it when (property-tested). |
| O5 | 2026-10-20 | Implemented SCALE.md §4 as written (random processes commit once per slot, so catch-up costs one commit per elapsed slot) and declared that cost in the bench (slope up to 1, bounded by eager stepping) instead of SCALE.md §10's 0. SCALE.md is unchanged. | §4 and §10 conflict. Measured (bench/results, 08:30 run; the 08:59 run agrees): a herd region read after n days makes n commits (0.505 µs at 1 day, 3,148 µs at 10,000 days), and in a 100,000-region world commits per day grow with world age toward one per region per day (10,424 at age 1 day, 99,809 at 1,000 days), because every region touched late pays for all the slots it skipped. Closed-form processes are flat. Ben's options: (a) accept the eager bound and use long slots (cost = regions / slot length); (b) give state-independent randomness O(log horizon) catch-up by splitting counts down a fixed dyadic tree, which composes exactly (prototyped as O17); (c) restructure state-dependent processes (cohorts by birth slot) to use (b). Recommendation: (a) now, (b) and (c) where the bench shows a process matters. |
| O6 | 2026-10-20 | Breakpoint wakes and other bookings are cancelled lazily: each region carries a stamp, a wake booked with an older stamp is dropped when it fires, and the timing wheel has no cancel operation. | PLAN.md §4.3 already books meetings with generation stamps; no back-pointers means entries can move between wheel levels freely. |
| O7 | 2026-10-20 | The timing wheel has 11 levels of 64 slots (the whole u64 tick range). Equal times pop in schedule order, which the cascade order guarantees without sorting. A time in the past is due now. | O(1) schedule and amortized O(1) pop with a deterministic order; property-tested against a sorted map. |
| O8 | 2026-10-20 | Rates that produce whole units are 32.32 fixed point (`Diffuser`); a float rate is converted once, at the edge. | SCALE.md §9 allows float rates, but a float remainder makes emit(a) + emit(b) differ from emit(a + b); integer floor division composes exactly, which lazy catch-up needs. |
| O9 | 2026-10-20 | The page store writes one segment file per save (every dirty page, LZ4 + CRC32 each) and then a binary, LZ4-compressed, CRC-checked manifest, in Deep Time's order (tmp, fsync, rename, fsync folder; manifest last; old files deleted after). Segments under half live are compacted, and more than 8 small segments (under 1 MiB) are merged; both thresholds are PLACEHOLDERS. Page keys are a 16-bit table and a 48-bit id. | An incremental save cannot overwrite a page file the durable manifest still names, and a file per page would cost an fsync per page. Two fsyncs per save whatever the page count. A JSON manifest would be megabytes at 100,000 pages. Known limit: every save still rewrites the whole manifest, so save cost has an O(all pages) part; an incremental manifest (deltas plus checkpoints) is the fix when the bench shows it matters. |
| O10 | 2026-10-20 | Operation ids carry their origin in the top bit: set for ids made on the Java side, clear for the engine's, matching the bridge's `Obs.JAVA_OP_BIT` (the kernel first had it the other way round; flipped once the bridge's wire format was seen). Observation replay drops sequences the pages include and operations already applied; operation ids are forgotten only when the bridge says no world save can resend them. `Replay::reconcile` detects the engine-ahead case (P29) but chooses no policy. | Two allocators never collide without coordinating; P29 is still Ben's. |
| O11 | 2026-10-20 | Agent storage is a `RowStore` trait implemented by `ColumnTable` (one `Vec` per component, dense rows, swap-remove, generation-checked sparse index). `bevy_ecs` is not a dependency. | P4 stays open: a `bevy_ecs` implementation of the same trait can be benched against it later without touching systems. |
| O12 | 2026-10-20 | Synthetic kinds (herd, sheep, field, stock, region) live in `rtsim_kernel::synth`, shared by the kernel's tests and the bench. | Scaffolding, as Ben allowed; delete it once real modules can stand in. |
| O13 | 2026-10-20 | The bench runs every configuration in its own child process, measures thread CPU time, a counting allocator's live heap and `/proc` RSS, repeats each point three times and keeps the median. Baseline counts and slope tolerances are PLACEHOLDERS. | Clean memory figures per configuration; CPU time bends less than wall time on a shared machine. Targets are P24. |
| O14 | 2026-10-20 | Binomial draws use inversion below n·p = 10 and Hörmann's BTRS above (expected cost independent of n). Float math in distributions is deterministic per platform and build; committed results are stored, never recomputed. | A herd of 10^12 must cost what a herd of 10 costs (the latent sweep measures it); the determinism gate is per machine (PLAN.md §4.7). |
| O15 | 2026-10-20 | No GitHub Actions workflow. The CI-style check is `tools/ci/scale-check.sh` (quick sweeps plus `--check`). | Actions on a private repo spend paid minutes: outward-facing, Ben's call. |
| O16 | 2026-10-20 | Build profiles as Deep Time's: workspace crates at -O1 and dependencies at -O2 in dev and test builds; release with thin LTO and `panic = "unwind"`. | Property tests stay fast with debug assertions on; the JNI crate will catch panics at every export (PLAN.md §2). |
| O17 | 2026-10-20 | Added `rtsim_kernel::dyadic`: a total split down a fixed dyadic tree over absolute slots, one binomial per node keyed by the node, so the count over any interval is a pure function of the seed and costs at most 2 × levels draws. Used by a bench-only `DyadicRegion` (cohort deaths at deterministic hazards, Poisson immigration) over 2^24 day slots. It is a prototype for O5's option (b), not a decision to use it. | It gives random processes O(log horizon) catch-up that still obeys the composition rule exactly, for points independent given the total. The bench measures it against the slot model (bench/results/README.md). State-dependent processes (births in proportion to adults) do not fit without restructuring, so SCALE.md §4's slots stay the default. |
| O18 | 2026-10-20 | A `LazyTable` caller must drain `next_due` up to `t` before reading or applying an event at `t` (debug builds assert it), and each region's booked wake time is saved with it and restored as is, never recomputed on load. | Found by an adversarial review: an event at a breakpoint's own tick superseded the breakpoint so its crossing was never delivered, a read could commit past a breakpoint that then fired against the later state, and a recomputed wake would depend on which reads moved the anchor before a save. |
| O19 | 2026-10-20 | The scale check also fails when any run breaks an invariant (conservation, a model's own invariants, slot commits above the eager bound), and flat contracts carry a max/min bound of 4 across the sweep (PLACEHOLDER) besides their slope. | A slope tolerance of 0.1 alone allows a 6.3x rise over eight decades, and a printed-only conservation result could fail silently. |

## Answers to the open questions (Ben, morning review 2026-10-20)

Each row answers the open question with the same id in OPEN-QUESTIONS.md.

| Id | Decision | Ben's words or choice |
|---|---|---|
| P1 | Three tiers for custom simulation features, because outside modders are expected:<br>1. a data and declarative rule engine for most content;<br>2. a native plugin SDK: plugins are their own cdylibs loaded through mic_native, written against an SDK crate that enforces the contracts (declared cadence and access, catch-up and breakpoints, integer ledgers, kernel RNG, no threads), with a test harness and per-plugin budgets and a kill switch;<br>3. Java and KubeJS event hooks.<br>Our own modules move onto the SDK. No stability promise before release. | "Leaning towards plugins own rust crates & / or having a config engine for mods to create custom sim logic since I'm fully expecting people un-affiliated with me wanting to make plugins that have custmo sim features. LMK if this is a trap."; then chose "Three tiers" |
| M7 | A third-party API is a goal; see P1. The module API stays free to change until a public release. | as P1 |
| O5 | Keep slots, and use dyadic counts for randomness that doesn't depend on state. | chose "Slots + dyadic" |
| P3 | The engine owns the truth; Minecraft entities are puppets vanilla never saves. | chose "Engine owns truth" |
| P4 | Agent storage stays our own columns, adding a two-column borrow and an index for rare tags. | chose "Our columns" |
| P7 | Every MIC mod keeps PATCHES.md plus contract GameTests for every reach into another mod. | chose "Every MIC mod" |
| P10 | Every design-changing review fix of 2026-10-19 is approved, item by item (all 15 listed in PLAN §17). | ticked every item |
| P24 | Performance targets keep their placeholders; final numbers come after real potato and Windows runs. | chose "Keep, set from data" |
