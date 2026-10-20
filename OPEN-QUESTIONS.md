# mic-rtsim: open questions for Ben

**For the morning review, 2026-10-20.** Every open choice in one place, grouped by what it blocks. Each
gives the recommended option first, then the alternatives. Answering "P17: recommended" or "I6: b" is
enough.

**Also review: the overnight implementation choices.** Agents logged every reversible choice they made
while you slept: O1 to O19 in DECISIONS.md ("Overnight choices"), and the rest in
`docs/overnight-choices-*.md` (B for the bridge, E for the phase 1 engine, K for kernel follow-ups, F for
the industry engine) and in mic-native's DECISIONS.md (N). They need your review too.

**Where these come from:**
- **P:** PLAN.md §17. P1 to P10 are also in DECISIONS.md. P19, P20, P22 and P23 are gone: they became
  C20 to C23. P30 and P31 are new, raised while applying C26 and C21.
- **I:** INDUSTRY.md §20. I1 is the one you chose to decide later.
- **M:** the questions in research/mcrtsim-mining-2026-10-19.md §6, numbered as there. C26 answered M5.

| Blocks | Count |
|---|---|
| Phase 0 | 9 |
| Phase 1 | 8 |
| Phase 2 | 5 |
| Industry | 27 |
| Later | 11 |
| **All** | **60** |

Phase 0 has its own group because it comes before phase 1 and its questions are the first to block
code.

## Phase 0

- **P1. The plugin split** (names are decided, A19). Recommended: plugins carry Java and data only, and
  rules live in generic modules. Alternative: plugins with their own Rust rule crates. Blocks: the plugin
  layout.
- **O5. Random processes and elapsed time** (found by the kernel bench overnight). SCALE.md §4 commits
  random processes once per slot, so a region left unread pays one commit per skipped slot, while §10
  says no cost may depend on elapsed time. Measured: a random herd model went from 0.49 µs to 3,011 µs
  per read as the gap grew from 1 to 10,000 days. Recommended: keep slots, and use the kernel's new
  `dyadic` counts (catch-up cost flat in elapsed time) for processes whose randomness does not depend
  on state, such as arrivals and deaths at fixed rates. Alternatives: slots everywhere and accept the
  cost; require closed-form distributions for every random process. Blocks: herds and anything random
  in lazy regions.
- **P3. The engine owns the truth.** Recommended: yes; Minecraft entities are puppets vanilla never saves
  (PLAN §6.1, and A9 and A10 assume it). Alternative: vanilla saves entities and the engine reconciles.
  Blocks: the bridge and the puppet GameTests.
- **P4. Agent storage.** Recommended: let the phase 0 storage bench decide, `bevy_ecs` behind a facade
  against our own columns, with the numbers brought to you. Alternative: pick one now. Blocks: the
  kernel's storage facade.
- **P7. Patch ledger and contract tests.** Recommended: PATCHES.md plus GameTests for every reach into
  another mod, in every MIC mod. Alternative: the rtsim only. Blocks: the first adapter.
- **P10. The design-changing review fixes.** Recommended: approve the 2026-10-19 list as applied (PLAN
  §17, the P10 paragraph). Alternative: item by item. Blocks: the bridge design and its GameTests.
- **P24. Performance and scale targets.** Recommended: keep the placeholders (bridge ≤ 1 ms at p99,
  nothing on the heap beyond the buffers, background pool cores − 2 with a minimum of 1); set final
  numbers from phase 0 data. Alternatives: set them now; relative targets only. Blocks: every perf gate.
- **P25. Regions per province.** Recommended: pick from the phase 0 bench among 4 × 4, 8 × 8 and
  16 × 16. Alternative: you pick now. Blocks: the address space (PLAN §5.2).
- **P29. Engine pages ahead of the world after a crash.** Recommended: keep the previous page generation
  until Minecraft's save completes, and load the newest one not ahead of the journal. Alternatives: write
  pages only after Minecraft's save; accept rare loss. Blocks: the journal GameTests.

## Phase 1

- **Quiescence and money sinks** (found overnight by the phase 1 scenario). An 11-village world never
  went quiet: about 30 events per market per day, from barter loops and money piling up in surplus
  villages, because nothing but players removes money. This makes P9 concrete. **Round 2 chose (E43 to
  E68 in docs/overnight-choices-phase1.md):** a money sink (coin above twice the treasury target is spent
  and leaves the world, booked as "sunk"), replacing "refuse cash and barter"; plus spoilage at target,
  stock bands, hysteresis and rationing. A closed 11-village world now goes fully quiet (36.5 to 0.00
  events per market per day); large grids keep 0.1 to 0.6. Review the sink; r, T and k stay yours.
- **G31: vanilla's reputation discount on sim prices.** The plan says vanilla's discount still applies
  on top of sim prices. The phase 1 Java agent trims it at the market's current price, so a buy and a
  sale can't loop into money, but the trim leaves the price change across one bundle. Alternatives: no
  vanilla discount in adopted villages; apply it inside the sim's quote. See
  docs/overnight-choices-phase1mc.md.
- **One source of truth for prices.** The Java side copies the market price math from rtsim-trade
  (`PriceCurve`, `Market::offer`) so offers can be shown at once. Two copies can drift apart.
  Recommended: the engine sends ready-made bundles and Java only shows them; keep the Java copy only as
  a test oracle. Alternative: keep both, with a shared test vector file that both sides must pass.
- **P3 (review 3): coins duplicated through a llama's chest.** When a caravan llama dies or is taken,
  the sim books its planned cargo, not what its chest really held. A player who pays coins, kills the
  llama holding them and picks them up leaves the sim's caravan still owning the coins. Recommended:
  book exactly the items the bridge reports for that member. Alternative: keep V13 (log the difference,
  don't correct). See docs/review3-2026-10-20.md.
- **Phase 1 save pages a build can't read.** Today the whole engine refuses to start. Industry drops its
  pages and starts fresh instead (W23). Recommended before release: the same as industry (drop and
  start fresh, with a loud log), since nothing is released (your no-backcompat rule). Alternative:
  refuse to start, as now.
- **Caravan size.** Small flows made tiny caravans: a robbed one carried 0.53 wheat. Round 2 makes a
  caravan wait at the dock until full (6 slots) or two days. Busy roads now send full caravans; small
  flows still make caravans under one slot, and news now waits for caravans, so a robbery report with
  P18's 1.5-day lifetime reaches 3 to 7 villages instead of 8. Review the wait and the report lifetimes
  together.

- **P9. Economy tunables.** Recommended: set `r`, `T` (at least 2 × the longest edge time) and `k` from
  a soak at the design gate; a Numismatics mint rule later. Alternative: you set each by hand now.
  Blocks: the phase 1 design gate.
- **P15. Body-kind format.** Recommended: datapack JSON at `data/<ns>/mic_rtsim/body_kind/<kind>.json`,
  ordered candidates with vanilla last; group kinds (C25) in the same format. Alternatives: server config
  TOML; fixed in each plugin's code. Blocks: body kinds and groups.
- **P16. Touring bands.** Recommended: bands tour ribbit villages as a flow, carry news and play at each
  stop. Alternatives: also tour border human villages; also raise the host village's opinion of the
  band's home. Blocks: the band adapter.
- **P17. Ribbit village growth.** Recommended: stock and treasury above target for a TUNABLE time book
  one house, placed when its chunk next loads, never over player blocks; population steps up then.
  Alternatives: houses only, population fixed; defer growth. Blocks: growth (walkthrough H).
- **P18. Phase 1 starting values.** Recommended: vanilla's gossip scale, Veloren's lifetimes and edges;
  the rest TUNABLE from a soak, now with rescues, gossip types per report (C20) and heirs (C24).
  Alternatives: you set each by hand; plain placeholders. Blocks: the phase 1 design gate.
- **P21. Currency exchange for players.** Recommended: traders and ribbit merchants at border markets
  offer emerald and amethyst trades at that market's rate. Alternatives: no player exchange in phase 1;
  exchange through Numismatics later. Blocks: border market offers.
- **P30. What a player base is (new, from C26).** Recommended: where a player spends much time (P26's
  counter), or a district (I2); caravans trade only through their own offers, with players present.
  Alternatives: declared by command; auto-trade with district stock; Create table cloth shops.
  Blocks: C26.
- **P31. Ribbit caravan cargo (new, from C21).** Recommended: the merchant carries it, and killing it
  drops the cargo as a journaled transfer, like a llama's chest. Alternatives: a loot table scaled by
  cargo; nothing drops. Blocks: robbing ribbit caravans.

## Phase 2

- **P12. What dragons prey on, besides player infrastructure (C16).** Recommended: caravan flows, plus
  village livestock counted as market stock; wild herds wait for ecology. Alternatives: a minimal
  wild-herd model in phase 2; caravans only. Blocks: dragon hunting legs.
- **P13. The In Control fire-dragon rule.** Recommended: remove it, so the sim owns every dragon
  (INDUSTRY.md I19 assumes this). Alternatives: keep it and adopt each spawned dragon; keep it and leave
  untagged dragons alone. Blocks: the dragon population.
- **P26. A player's home, for targeting.** Recommended: the region where the player spent the most time
  recently, from a decaying counter. Alternatives: the respawn point; a claims mod. Blocks: dragon
  grudges, and P30 reuses it.
- **P28. Dragon damage in a watched fight.** C18 already makes unwatched attacks destroy bounded blocks.
  Recommended: a watched dragon breaks no blocks (IaF griefing stays off). Alternatives: bounded
  sim-placed damage in watched fights too, so both agree; turn IaF griefing on. Blocks: dragon fights.
- **M1. Dragon breeding and egg theft.** Recommended: a pair breeds in its lair on a scheduled event,
  giving C24's heirs; taking an IaF egg is a theft they remember (hooks UNVERIFIED). Alternatives: no
  breeding, the lair is the heir; breeding with no egg slight. Blocks: dragon heirs.

- **Dragons built overnight on recommendations** (docs/overnight-choices-dragons.md, R1 to R31). The
  engine took P12, P13, P26, P28 and **M1 (dragons breed in their lair)** at their recommendations.
  Review M1 especially. Not yet independently reviewed: the review hit the rate limit.
- **Pack change waiting for you (P13):** remove the fire-dragon entry from In Control's `spawner.json`
  and take the fire dragon out of `spawn.json`'s two rules, so the sim owns every dragon
  (docs/boundary.md §6.6). Nothing in the pack was changed overnight.
- **Plan facts the jar contradicts** (found by the dragons agent): `stealItems` is a pixie setting, not a
  dragon one (PLAN.md, INDUSTRY.md and the industry engine's zero carry for flyers cite it as a dragon
  setting); In Control's fire dragon is desert-only in effect through `spawn.json`; dragons do carry
  prey off from stage 2. The docs still need correcting.

## Industry

- **The stale-tick race** (found overnight by the industry engine). Commands land at least one tick
  after a chunk loads, so a machine can run once on stock the replay already counted: +40 ingots (0.4%)
  in the test. No trim inside one district can see it. Four options are in
  docs/overnight-choices-industry.md; the agent's cleanest is to send a district's reconcile ahead as a
  player approaches, using player positions from the bridge. Your call.

Phases are INDUSTRY.md §16 (I-A to I-H).

- **P5. Factory unit.** Recommended: retire it in favour of I2's district. Alternatives: one Create 6
  logistics network; per-machine or per-chunk units. Blocks: I-A, answered with I2.
- **I1. Rebuild schematics (you chose "decide later").** Recommended: repair schematics of the destroyed
  blocks only, sent to the district's postbox. Alternatives: full snapshots; none, the ledger lists the
  positions. Blocks: I-H.
- **I2. What a district is.** Recommended: automatic, closed under belts, pipes, shafts and wires, named
  after its station, with command edits. Alternatives: player-designated; one logistics network (P5); a
  loader's footprint. Blocks: I-A.
- **I3. Boundary stock.** Recommended: tagged storage, packager inventories, postboxes and plugin
  buffers; machine inventories never. Alternatives: count all inventories but write only tagged ones;
  stock links only. Blocks: I-A.
- **I4. When time passes.** Recommended: game ticks only; nothing while the server is stopped.
  Alternative: wall-clock catch-up with a cap. Blocks: I-B.
- **I5. Learning.** Recommended: a windowed steady test; unlearned, stale and not-learnable districts
  freeze. Alternatives: a stale district runs its old model; run at reduced confidence. Blocks: I-A.
- **I6. Box coupling.** Recommended: one all-or-nothing process; any empty input, full output or target
  stops it. Alternatives: one process per output; learned coupling. Blocks: I-B.
- **I7. Partial loads.** Recommended: suspend while any chunk ticks, with pending deltas per chunk.
  Alternatives: simulate the frozen remainder; force-load it. Blocks: I-B.
- **I8. Exactly-once reconcile.** Recommended: an epoch in a chunk attachment; removals first; a
  shortfall trims outputs. Alternatives: the global journal only; NBT rewrite at load. Blocks: I-B.
- **I9. Proximity production.** Recommended: replay what a chunk-loaded district with no player near
  makes: crops yes, spawn-fed farms no. Alternatives: everything; nothing from the world. Blocks: I-B,
  and it depends on open fact 2 below.
- **I10. Moving contraptions.** Recommended: cyclic ones are learnable; progressive ones freeze the
  district. Alternatives: replay anyway; plugins for known designs later. Blocks: I-B.
- **I11. Finite deposits.** Recommended: plugins for Ore Excavation, Diesel Generators and Destroy crude
  oil. Alternatives: deposits infinite; freeze drilling districts. Blocks: I-C.
- **I12. Seasons.** Recommended: per-season models; a season change is a breakpoint. Alternatives:
  ignore seasons; mod-data multipliers. Blocks: I-C. It reads the same calendar as P2.
- **I13. Links across districts.** Recommended: Ender Transmission channels as shared stock; PowerGrid
  lines couple running states. Alternatives: merge linked districts; not learnable. Blocks: I-C.
- **I14. Destroy pollution.** Recommended: attribute `changePollution` by position; level types while
  unloaded, chunk types on load. Alternatives: grievance only; level types only. Blocks: I-C.
- **I15. Station cargo.** Recommended: emulate the observed exchange through `TrainCargoManager`.
  Alternatives: packages only; no exchange. Blocks: I-D.
- **I16. Gauge networks across districts.** Recommended: warn first, then mixins count simulated links
  and fulfil into offline buffers. Alternatives: warn only; fulfilment only. Blocks: I-D and I-E.
- **I17. Player ships.** Recommended: frozen while unloaded, as today, shown on the ledger. Alternative:
  a Sable voyage plugin with the naval phase. Blocks: the ledger's ship lines.
- **I18. Visibility.** Recommended: a Create Display Source on stock links and stations, atlas markers,
  and a chat ledger. Alternatives: chat only; a custom screen. Blocks: I-A.
- **I19. Threat order.** Recommended: natural mobs, then dragons, then bandits, then bosses with kaiju.
  Alternatives: dragons first; bandits first. Blocks: I-F and I-G.
- **I20. Natural mob pressure.** Recommended: measured spawns per dark block, with vanilla's griefing
  rules. Alternatives: none; a TUNABLE hazard per biome. Blocks: I-F.
- **I21. Defence survey.** Recommended: perimeter, dark area, roof, radar turrets with ammunition, kill
  machines, guards and observed fights. Alternatives: observed fights only; declared defences. Blocks:
  I-F.
- **I22. Resolution.** Recommended: hazards for pressure, Lanchester for attacks, never better than
  observed. Alternatives: a lookup of observed fights; hazards for everything. Blocks: I-F.
- **I23. Damage.** Recommended: theft up to carry and a cap; exposed blocks up to a cap, with drops; a
  lost machine stops the district; never fire. Alternatives: proportional loss; theft only; no drops.
  Blocks: I-F.
- **I24. Trains attacked.** Recommended: loot carriage cargo; never cut track while unwatched.
  Alternatives: derail; delay only. Blocks: I-F.
- **I25. Pacing.** Recommended: per-attack caps and a cooldown per threat class, no shields.
  Alternatives: shields after heavy losses; tie attacks to the owner's online time. Blocks: I-F.
- **I26. Industry phase order.** Recommended: read-only ledger, replay, fluids and energy, trains, Create
  precision, threats, rebuild. Alternatives: threats first; trains before replay. Blocks: the start of
  industry work.

## Later

- **P2. Sim calendar.** Recommended: Serene Seasons' calendar, or mic-climate's `YearClock` in Deep Time
  worlds. Alternative: the vanilla day count only. Blocks: herds and growth; industry's I12 reads the
  same seasons.
- **P6. History timing.** Recommended: at world creation, after Deep Time's bake. Alternative: lazily,
  per region on first visit. Blocks: history. Under C22, history at world creation knows no villages yet
  (PLAN §10.7).
- **P8. Bodies, structures, ports and havens.** Recommended: survey Small Ships, Create Aeronautics
  Structures, Seven Seas, the Alex's Caves port, Goblin's Tyranny, Illager Invasion, Naturalist, and
  havens (C24). Alternatives: vanilla bodies only; abstract only. Blocks: later phases, industry I-G.
- **P11. Order of later phases.** Recommended: ships, herds, conflict, history, kaiju, space; civilians
  only when you ask. Industry is now parallel (C15). Alternative: herds first, then ships. Blocks: what
  follows phase 2.
- **P14. Who owns signals with no actor.** Recommended: direct acts per player; pollution and the like
  to one shared "settlers" actor. Alternatives: a claims or teams mod; each chunk to the player with the
  most time or blocks there. Blocks: conflict, and I14's grievance feed.
- **P27. Kaiju trigger.** Recommended: the first nuclear detonation observed in the world. Alternatives:
  the first device crafted; the first reactor reaching criticality (hooks UNVERIFIED). Blocks: kaiju.
- **M2. Patrols.** Recommended: yes, with conflict and ships: a patrol is a flow that lowers an edge's
  hazard, and crew conversion is a transfer of group counts. Alternatives: no patrols, so lines are safe
  only through district defences (I21); patrols as notable rows only. Blocks: conflict and ships.
- **M3. Villages advancing in technology.** Recommended: decide it with C22's future direction; until
  then growth is houses only (P17). Alternatives: professions shift with the market sooner; villages gain
  Create stations, which train trade needs. Blocks: later village content.
- **M4. Village decline and founding.** Recommended: decide it with C22's future direction too.
  Alternatives: decline as market collapse only, with no block changes; abandonment applied on load,
  like C18 (residents are vanilla, C2). Blocks: later village content.
- **M6. Hiring and escort.** Recommended: escort only, through rescues (C24); hiring waits for guards
  (C12), with no dialogue screen (D3). Alternatives: hire a caravan to carry your goods, through its
  offers; escort contracts as offers. Blocks: later caravan content.
- **M7. A third-party API.** Recommended: not a goal before a public release; the module API stays
  internal and may change freely. Alternative: design for outside modders now, with a stable module API
  and plugin Rust crates (against P1). Blocks: how stable the module API must be (PLAN §5.4).

## Open facts, settled by GameTest

These are facts, not choices. The sources disagree (PLAN §15.1).

1. **Do Create trains run in other dimensions?** INDUSTRY.md reads Create's source as yes; the old PLAN
   §10.4 and the industry inventory said overworld only. A Nether train loop with nobody there settles
   it. It decides whether the industry train hooks cover every dimension.
2. **Do random ticks happen under chunk loaders?** `pack/modDocs/create-power-loader.md` says crops need
   a nearby player; INDUSTRY.md reads NeoForge's `tickChunks` as random-ticking under a ticking ticket.
   Crops in a loader-held chunk with nobody within 128 blocks settle it. It decides I9.
