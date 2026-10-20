# Overnight choices: phase 2 dragons, engine side (2026-10-20)

Reversible choices made while Ben was asleep (D9, D10), each with its reason, numbered R#. None is
final until Ben reviews it. The work is `crates/rtsim-ecology`, `rtsim-cli sim dragons`, the dragon
sweep of `bench scale`, and `docs/boundary.md` §6. DECISIONS.md, PLAN.md and OPEN-QUESTIONS.md are
unchanged; where this work found one of them wrong, the last section says so.

## Ice and Fire facts checked in the jar (2026-10-20)

Checked in `iceandfire-2.1-beta.1.jar` and the pack's configs (the smoke server copy is byte-identical
to `pack/config`). Everything else Ice and Fire in the code is marked UNVERIFIED or TUNABLE.

- Entities `iceandfire:fire_dragon`, `ice_dragon`, `lightning_dragon`; base class
  `com.iafenvoy.iceandfire.entity.DragonBaseEntity extends TamableAnimal`.
- NBT: `Hunger` int, `AgeTicks` int, `Gender` bool (true male), `Variant` string, `Sleeping`,
  `AgingDisabled`, `HasHomePosition` bools, `HomeAreaX/Y/Z` ints and `HomeDimension` string (written
  only with a home). A home coordinate of exactly 0 drops the home on read.
- Hunger is clamped 0 to 100, higher is fuller, falls by 1 every `hungerTickRate` ticks; spawned
  dragons start at 50. The target goal hunts below 90. Prey food points: `round(width × height × 10)`
  of the prey's box for an `AgeableMob`, 15 for a player.
- Stages by age in days: 25, 50, 75, 100 begin stages 2 to 5; `isMature` is stage 4 or more; a wild
  dragon's age in days stops at 128; no death of age.
- Max health from 4% of `maxHealth` at day 0 to all of it at day 125; the pack's `maxHealth` is 500.
- `public final IafDragonFlightManager flightManager`, `setFlightTarget(Vec3)`; overwritten while the
  dragon has a live attack target; y clamped to `maxFlight` (256).
- Lairs: structures `iceandfire:{fire,ice,lightning}_dragon_{roost,cave}`; structure sets spacing 15
  (roosts) and 18 (caves) chunks; hoard piles `gold_pile`, `silver_pile`, `copper_pile`. Lair
  dragons spawn with aging disabled.
- `dragon_targets` tag: cow, sheep, pig, chicken, llama, trader llama and more. Dragons of stage 2 and
  up carry small prey off (`startRiding`).
- Pack config: `dragon.griefing` false, `canDespawn` true, `targetSearchLength` 128,
  `wanderFromHomeDistance` 40, `hungerTickRate` 3000, `neutralToPlayer` false. **`stealItems` is a
  pixie key, not a dragon one.**
- In Control: `spawner.json` has a fire dragon entry (`persecond` 0.1, `maxthis` 200, overworld, no
  biome condition); `spawn.json` has two `onjoin` rules for blaze and fire dragon (allowed in
  `c:is_desert`, denied elsewhere when In Control caused the join).
- Eggs come only from female dragons' loot and from breeding, which needs a player feeding mature
  dragons; a hatched egg becomes a tamed dragon owned by the egg's owner.

## Choices

| # | Choice | Why |
|---|---|---|
| R1 | One crate, `crates/rtsim-ecology`, holds the dragon rules and their composition with `rtsim-world` (markets, flows, the leg index, knowledge) and `rtsim-industry` (threats). The generic parts (species, hunger, prey, homes, flights, the boundary codec) are their own modules; `engine.rs` composes them. | PLAN.md §5.4's module registry does not exist yet; `rtsim-world` composes phase 1 the same way (E1). The caller owns the world and passes it in, so nothing in phase 1 changes. |
| R2 | Dragons are notables in the world's own `Knowledge`, and lairs are knowledge holders (a group actor, holder kind Notable) that witness what happens to their dragons. | One knowledge store means caravans carry dragon news, villages' feelings move, and a lair can be the heir of last resort (C24, PLAN.md §9) with no new holder kind. |
| R3 | P12's recommendation: prey is caravan flows and village livestock counted as market stock; wild herds wait for ecology; player districts only through a grudge (C16). | Taken as recommended (D10). |
| R4 | The hazard on a flow is drawn per caravan: a road patrol falls on the caravans on the road when it ends, a crossing on those passing its point in its window, each caught with the species' chance (one counter-RNG draw keyed by dragon and caravan key). A caught caravan becomes a row (`World::promote`) and loses a share of its cargo. Decisions use the closed-form expectation: the walk times the flow's rate. | SCALE.md §3 makes a traveller an individual when something happens to it; cutting a share of a pipeline in place would need `flows.rs` changes in phase 1's code. The expectation does not depend on caravan batching, so the decision stays right after phase 1 round 2. |
| R5 | Conservation: what dragons take is destroyed in the world's books (`outside.destroyed` for cargo, the line's `book.destroyed` for stock); the dragons' ledger splits it into eaten and burnt (consumption) and hoarded (a holder, the lair's hoard), and hoards hold hoarded minus robbed. `Dragons::check` composes the world's own check, the split, the hoards and the industry engine's stolen books. | PLAN.md §9: dragons destroy and have no haven; the task: stolen or eaten goods leave only as consumption or go to the hoard. |
| R6 | Prey classes are data by item name: herd livestock (cattle 13, sheep 12, pig 8 fullness: Ice and Fire's food-point rule on vanilla's boxes, box sizes not checked in the jar), meat items, valuables (any currency, gold, emerald, diamond), and everything else burnt. | The prey table is data per pack; the numbers are placeholders except where the rule is Ice and Fire's. |
| R7 | Fullness is Ice and Fire's `Hunger` (verified semantics), a closed form anchored at the last meal. A roosting dragon hunts at 60 (TUNABLE; Ice and Fire's own goal hunts below 90), a bout chains at most 2 hunts, and it rests at least half a day between bouts. | Hunting every few days rather than every few hours; one event per hunt leg; a dragon that ate little does not loop. |
| R8 | Seasons come from a plain calendar shaped like Serene Seasons' (24-day seasons) until P2 is decided. Season weights per prey kind (TUNABLE) encode "herds penned near villages in winter, grazing wide in summer": winter roads 50% and villages 180%, autumn 130% and 60%. | The task's "winter shifts what dragons prey on"; P2 is Ben's. |
| R9 | Flights are straight legs at the species' speed (PLACEHOLDER 0.5 blocks a tick) in the shared leg index, at a forced cruise Y of 180 while unloaded. A roosting dragon books no leg: players find it through a static lair index. | A15 and the Veloren research §4. A days-long stay leg books a cell per hour per region and made each booking cost grow with crowding. |
| R10 | Encounters: `LegIndex::sharing` (new, additive) lists the live legs sharing a cell with a leg; the existing `meeting` gives the first moment two come within range. Player interest areas use the top owner bit (phase 1's `PLAYER_OWNER` convention), dragons the next. | One shared index, no scans; exactly one event per crossing pair, cancelled in cascade when either leg is replanned. |
| R11 | A live meeting materializes the dragon at the area's edge. It harasses when it hunts there, flies to prey near the player, holds a grudge against the player, or comes for the player's home; it then books a departure (visit PLACEHOLDER 6,000 ticks, bent up by Aggressive). Passing through, it departs when it would cross the view; at home, it stays until the player leaves. | C24: threats get bored, and only a visit to harass needs a bound. |
| R12 | Wounds shorten the rest of a visit by 1 − 3 × (damage / max health) (TUNABLE); a wound of a third of its health ends it at once. A starving dragon (fullness 20 or less, TUNABLE) only looks in and leaves to hunt. | C24: damage and hunger bring the departure forward. |
| R13 | A dragon's agent id is namespace byte 0x44 over its 48-bit row handle. | The native link fixes no agent namespace; this keeps dragons apart from caravans' ids. |
| R14 | The scenario sets the industry engine's carry for flying attackers to 32 items (TUNABLE); the industry default (0) is unchanged. | The default rests on a dragon `stealItems` key that the jar check shows is a pixie key. Without carry, a grudge raid destroys blocks but takes nothing to eat or hoard. Changing the industry default is the industry module's call. |
| R15 | An abstract raid never kills a dragon (it keeps a tenth of its health); defences that stop passes wound it by 40% of max health times the stopped share (TUNABLE). | A death is a player's kill or a real fight, with a report and an heir; an abstract kill would hide both. |
| R16 | P26's recommendation for a player's home: a decaying counter per region from position reports (half-life 3 days, 8 regions per player, PLACEHOLDERS). A grudge raid hits up to 2 districts within 512 blocks of the home; after a grudge a dragon hunts for food for 3 days before the grudge pulls again; a starving dragon eats first. | Taken as recommended; the cooldown stops an heir from circling a watched home forever. |
| R17 | M1's recommendation: a mated pair in one lair breeds on one scheduled event (every 60 days, TUNABLE, from maturity, Ice and Fire's stage 4); single dragons do not breed. The heir is the youngest living offspring, else the mate, else the lair. Egg theft as a slight is left (hooks UNVERIFIED). | Taken as recommended; heirs exist without Ice and Fire's player-driven breeding. |
| R18 | P13's recommendation: the sim owns every dragon. Untagged dragons: a lair's own from world generation is adopted as its occupant (a second is discarded), spawn-egg and command dragons are adopted as wanderers, natural and spawner dragons are discarded, tamed and hatched ones are left alone (A10). Respawn: a roost empty for 20 days (TUNABLE) draws a stage-2 dragon (30 days old) from 3,000 blocks away, as a flight. | Taken as recommended; adopting creative spawns keeps admins' dragons alive while keeping the population the sim's. |
| R19 | P28's recommendation: a watched dragon breaks no blocks (Ice and Fire griefing stays off). Its consequences go through stock: DRAGON_ATE takes a village's herd by the count the real dragon killed inside the village. | Taken as recommended; DRAGON_ATE keeps the market's herd honest with what happened in game. |
| R20 | Sim lifespan 400 days (TUNABLE). | Ice and Fire has no death of age (verified); C24 wants one booked event. |
| R21 | A player's kill is a killing report witnessed by the dragon's kin and lair; the last 4 settlements it preyed on (TUNABLE) witness a rescue report naming the slayer. | C24 (rescues count) and PLAN.md §9's walkthrough, step 6. |
| R22 | A robbed hoard gives up what the engine's hoard holds of each item, and the rest is the structure's own treasure; the robbery report's severity grows with the items taken (one unit plus one per 64 items). | The structure's gold piles were never in the engine's ledger. |
| R23 | Hoards stay abstract: valuables are not placed into lair chests. Placing them on load through TRANSFER_TO_CONTAINER is left for later. | Conservation holds either way; placing items needs the plugin. |
| R24 | A village raid takes at most 5% of each herd good's stock (TUNABLE), no more than the dragon can eat; the village witnesses an assault by the dragon. A village's defenders cost 15 fullness points of utility (TUNABLE), bent by boldness (C25). | Retuned after phase 1 round 2, whose villages keep much larger herds. |
| R25 | `rtsim-world` gets `Cause::Predator` for the market event a raid causes; it forces news like a player trade. | A raid moves stock and price; the news must go out. Additive: no exhaustive match on `Cause` exists. |
| R26 | The dragon sweep measures the dragons' own CPU per dragon-day (the world stepped apart) on a fixed 256-village world, 8 to 512 dragons; the contract is flat. | Subtracting a run without dragons buried small counts in noise; growing the map with the dragons measures the cache, not the dragons (bench/results/README.md). |
| R27 | `sim dragons` runs a fixed timetable: the player wounds Cinderjaw on a road, kills it at home, visits villages and a lair, and robs its hoard; reactions apply by dragon name when it materializes. The fake plugin captures a departing dragon 300 ticks later. | A reproducible script, tuned to this seed; the checks verify the beats, not exact days. |
| R28 | Additive changes to shared crates: `Arena` indexing (`rtsim-kernel`), `LegIndex::sharing` and `window_ticks` (`rtsim-travel`), `World::take_stock`, `prey_on_traveller`, `destroyed_total` and `Cause::Predator` (`rtsim-world`). | The task's rule: other crates through their public APIs, with only minimal additive changes. |
| R29 | Kind ranges reserved: 0x0060 to 0x006F, 0x0500 to 0x05FF, 0x8500 to 0x85FF. | Next to the industry ranges (`docs/industry-boundary.md` §1). |
| R30 | A hungry dragon's leg looks up the booked train legs in the regions it passes (the industry engine's leg index, one lookup per region) and falls on the first train it overlaps, in the middle of the window both are under way in; the industry engine loots the carriages within its caps (I24) and the dragons' ledger takes the loot as the difference in the engine's stolen books. At most one train a leg (PLACEHOLDER). | PLAN.md §9: "a dragon's hunt leg that crosses a district or a booked train leg schedules an encounter". The train leg index has no positions inside a leg, so the meeting time is the overlap's middle, a PLACEHOLDER until train legs carry their path. |
| R31 | `Dragons::observe_player` books its own interest area per player in the shared leg index, with the same owner convention as `World::set_player_pos` (phase 1 round 2). When the bridge drives both, a player has two interest legs and a crossing books two meetings; the second finds the dragon already embodied and does nothing. | The world keeps its interest leg's id private (`PlayerState::interest`); one interest leg per player is the integration step (expose the world's stamp, or let the dragons use the world's areas). |

## What the jar check contradicts in the plan documents (not edited)

- PLAN.md §9 and INDUSTRY.md §13.2 say the pack sets dragon `stealItems` false. There is no dragon
  `stealItems`; it is a pixie key. The industry engine's default carry for flying attackers (0)
  cites it (`rtsim-industry` `tunables.rs`).
- PLAN.md §9 says the In Control fire dragon spawner has no biome condition and calls the "desert
  spawner" reading wrong. The spawner entry has none, but `spawn.json`'s `onjoin` rules deny In
  Control's fire dragon joins outside `c:is_desert`, so in effect it spawns in deserts only.
- PLAN.md §9 says whether Ice and Fire dragons carry prey off is UNVERIFIED. They do, from stage 2,
  for prey under half their width.
- `pack/modDocs/iceandfire-ce.md` still names 2.0-beta.16; the pack pins 2.1-beta.1.
