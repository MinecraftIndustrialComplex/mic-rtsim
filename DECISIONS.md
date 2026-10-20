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
| P1 | The working name `mic-rtsim`. Mod ids are `mic_rtsim` for the core and `mic_rtsim_<plugin>` for plugins. | phase 0 |
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
