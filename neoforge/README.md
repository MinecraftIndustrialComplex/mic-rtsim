# mic-rtsim: the NeoForge side

The Java half of mic-rtsim (PLAN.md §1, §6): the core bridge mod **`mic_rtsim`**, and later one
project per plugin (`mic_rtsim_<mod>`, PLAN.md §1.3). Minecraft 1.21.1, NeoForge 21.1.249,
ModDevGradle 2.0.74, Parchment 2024.11.17, Java 21: the same pins as `mods/mic-climate`.

The core depends on NeoForge only (D4). Until rtsim-jni exists, its engine is `StubEngine`, a
pure-Java stand-in that speaks the boundary protocol exactly and simulates nothing.

## Build and test

All heavy jobs on the shared laptop go through one lock, niced, with 4 workers:

```sh
cd neoforge
# the jar: core/build/libs/mic_rtsim-1.21.1-0.1.0.jar
flock /tmp/mic-rtsim-bridge-heavy.lock nice -n 10 nix shell nixpkgs#jdk21 -c ./gradlew --max-workers=4 :core:build

# unit tests only (pure Java, seconds)
flock /tmp/mic-rtsim-bridge-heavy.lock nice -n 10 nix shell nixpkgs#jdk21 -c ./gradlew --max-workers=4 :core:test

# unit tests, then the GameTests on a headless server; exits non-zero on any failure
./gametest.sh
```

`gametest.sh` runs `:core:test` and `:core:runGameTestServer` under the same lock, using the
repository flake's dev shell when there is one (`../flake.nix`) and `nixpkgs#jdk21` otherwise. It
leaves the full log in `core/build/gametest.log` and prints every `[gametest]` line, where each test
logs what it compared. About 40 seconds on the laptop, nearly all of it server start.

- A mixin that no longer matches its target fails the launch (`mic_rtsim.mixins.json` is
  `defaultRequire 1`): look for `InjectionError` or `MixinTransformerError` in the log.
- The GameTest world lives in `core/run-gametest/` and is reused between runs, so each run starts by
  replaying the journal the last one left (choice B37). Delete the directory for a clean world.
- `-PgametestDebugLog` turns on DEBUG logging, which shows the mixins being applied.

## What is here

| Package | What |
|---|---|
| `boundary` | the engine interface and the binary layout (`Layout`, `Obs`, `Cmd`), the hand-over state machine (`Handover`), one session of it (`BoundarySession`). Pure Java. Described for the Rust side in `docs/boundary.md`. |
| `journal` | the journal (`Journal`, pure Java) and its SavedData `mic_rtsim:journal` (`data/mic_rtsim_journal.dat`). |
| `engine` | `StubEngine`: the four buffers, a worker thread, pages that survive a simulated crash, a script API for tests. |
| `bridge` | the server session (`RtsimSession`): puppet pass, spawn, capture, taking, observations; `CommandDispatcher`; the NeoForge hooks (`BridgeEvents`); the `doTraderSpawning` switch (`TraderSpawning`). |
| `puppet` | the `mic_rtsim:agent` attachment (engine id, epoch), groups and the live registry. |
| `body` | the adapter interface (spawn, capture, steer), group kinds, and the vanilla adapters for `minecraft:wandering_trader` and `minecraft:trader_llama`, plus the human caravan group. |
| `config` | the server config: one switch per module, PLACEHOLDER budgets. |
| `mixin` | `EntitySaveMixin` (`shouldBeSaved`, `saveAsPassenger`), `LlamaInvoker` (`setStrength`), two accessors for the save GameTests. |
| `api` | `RtsimApi` for plugins: register bodies and groups, `takeFromSim` (the hook point for Carry On's `EntityPickupEvent`). |
| `command` | `/rtsim perf`, `/rtsim stub caravan`. |
| `gametest` | the GameTests, in the `mic_rtsim` namespace. |

## The GameTests

| Test | What it shows |
|---|---|
| `puppetisnotsavedacrosssaveandreload` | vanilla's own entity-chunk save (`autoSave`) and a read back from storage: an ordinary trader on the same block reloads, the puppet does not |
| `puppetridingaboatisnotsaved` | a boat saves its passengers through `saveAsPassenger`; the puppet is not among them, a pig in a second boat is |
| `staleepochcopyisdiscardedonjoin` | copies built from the puppet's own NBT, made stale four ways (old epoch fresh, old epoch from disk, live epoch under another UUID, an agent not embodied), never join |
| `traderandllamascaptureasonegroupandnollamadespawns` | the despawnDelay trap is closed; one llama in a loaded but frozen chunk captures the whole caravan in one tick; no lead drops; cargo, variant and strength are captured |
| `amemberleavingtrackingcapturesthegroup` | a member whose chunk stops being accessible captures the whole caravan |
| `aspawnwhereentitiesdonottickisrejected` | a spawn in a chunk that is not entity-ticking is answered SPAWN_REJECTED(not ticking) and creates nothing |
| `spawnquotaneversplitsagroup` | three caravans, quota 8 bodies a tick: two in one tick, the third whole in the next |
| `journalreplayappliesexactlyonceafteracrash` | two simulated crashes; transfers in both directions applied exactly once, one op id sent twice |
| `tamingtakesacreatureoutofthesim` | taming (vanilla's tame line) takes a llama out of the sim; TAKEN with its cargo; the rest stays one group |
| `aleadtakesacreatureoutofthesim` | a lead held by a player takes a llama on the next pass |
| `amemberremovedbyanothermodleavestherestasonegroup` | risk 11: a member another mod removes leaves as TAKEN(removed by other); the rest stays one unit |
| `deathsarejournaledandthereststaysonegroup` | DIED after each member's state at death (cargo included); the llamas of a dead trader stay one group and are not "taken" by the dead trader's leash |
| `rtsimcommandsrun` | `/rtsim perf` and `/rtsim stub caravan` through the server's command dispatcher |
| `dotraderspawningfollowsthetradeandtravelswitches` | the gamerule follows the trade and travel switches, and the engine hears the module mask |

## Not done yet

- **The engine.** rtsim-jni and the Rust step do not exist; `StubEngine` stands in.
- **Observations** beyond players, puppets, taking, deaths and transfers: chunk loads and tickets,
  damage, rescues, trades, villages, census, recipes (PLAN.md §6.3).
- **Commands** beyond spawn, despawn, steer and container transfers: trade offers, gossip, structure
  placement (PLAN.md §6.4).
- **Item components** in captured cargo (B23).
- **Body kinds as data** (P15): group layouts are code.
- **Carry On**: the plugin is not written; `RtsimApi.takeFromSim` is its hook point.
- **A GameTest for a server at simulation distance 10** (risk R1): it needs real players.
- **PATCHES.md** (P7) does not exist yet; every hook is listed in `BridgeEvents` and the mixins.
