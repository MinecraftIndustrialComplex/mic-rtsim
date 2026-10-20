# mic-rtsim: the NeoForge side

The Java half of mic-rtsim (PLAN.md §1, §6, §8): the core bridge mod **`mic_rtsim`** and one project
per plugin (`mic_rtsim_<mod>`, PLAN.md §1.3). Minecraft 1.21.1, NeoForge 21.1.249, ModDevGradle
2.0.74, Parchment 2024.11.17, Java 21: the same pins as `mods/mic-climate`.

| Project | Mod id | Built around | What |
|---|---|---|---|
| `core` | `mic_rtsim` | NeoForge only (D4) | the bridge, and phase 1's vanilla side: villages, markets, caravans, gossip, damage, rescues, robbery, player bases, growth |
| `ribbits` | `mic_rtsim_ribbits` | Ribbits 4.1.6 (with YUNG's API 5.1.9, GeckoLib 4.9.3) | ribbit villages and residents, merchant caravans, touring bands, sorcerers who remember, growth from YUNG's houses (C7, C23) |
| `carryon` | `mic_rtsim_carryon` | Carry On 2.2.6.13 | a creature picked up leaves the sim (A10) |

The plugins compile against the pack's own jars, read from `packMods` in `gradle.properties` (the MIC
smoke server's mods folder next to this repository; `-PpackMods=...` to point elsewhere), and their
GameTest servers load those jars. Every reach into another mod is in `../PATCHES.md`.

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

`gametest.sh` runs `:core:test` and `:core:runGameTestServer`, then each plugin's
`runGameTestServer`, under the same lock, using the repository flake's dev shell when there is one
(`../flake.nix`) and `nixpkgs#jdk21` otherwise (`./gametest.sh ribbits` runs one project;
`MIC_RTSIM_LOCK` and `MIC_RTSIM_WORKERS` override the lock and worker count). It leaves each full log
in `<project>/build/gametest.log` and prints every `[gametest]` line, where each test logs what it
compared. About two minutes for all three on the laptop, most of it server starts and stops.

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
| `mixin` | `EntitySaveMixin` (`shouldBeSaved`, `saveAsPassenger`), `LlamaInvoker` (`setStrength`), two accessors for the save GameTests; phase 1: `StructureManagerMixin` (adoption), `MerchantResultSlotMixin` (trades), `VillagerRestockMixin` (restock and demand), `MerchantOfferAccessor`. All in `../PATCHES.md`. |
| `api` | `RtsimApi` for plugins: register bodies and groups, `takeFromSim` (Carry On's hook); phase 1: cultures, residents, resident factories, a village and its sentiment, the restock and demand hooks. |
| `command` | `/rtsim perf`, `/rtsim stub caravan`. |
| `village` | phase 1: the village registry (`data/mic_rtsim_villages.dat`), adoption (C22), cultures, residents, the census, gossip writes (C20). |
| `market` | phase 1: the price curve and market book (a mirror of rtsim-trade), resident repricing through the treasury (C14), puppet merchant offers and cargo (C21), robbery detection. |
| `knowledge` | phase 1: sentiment snapshots, HURT coalescing, rescues (C24). |
| `base` | phase 1: player bases as market nodes (C26, P30's recommendation; `data/mic_rtsim_bases.dat`). |
| `build` | phase 1: player-placed block marks, house placement from templates, resident spawns (C7). |
| `bridge/Phase1` | wires phase 1 to the tick, the events and the commands. |
| `gametest` | the GameTests, in the `mic_rtsim` namespace, and helpers the plugins' tests share (`RtsimTests`, `FakePlayers`). |

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
| `despawnofanagentthatisnotembodiedisanswered` | DESPAWN_GROUP for an agent with no puppet gets DEMATERIALIZED(not embodied), so the engine never waits on it |
| `acapturedcopycomesbackonceasanordinaryentity` | a copy a capture mod serialized while the member was a puppet joins once, untagged; a second copy is stale |
| `acancelleddeathkeepsthemember` | a death another mod cancels after the bridge saw it journals nothing and keeps the member |
| `atakenllamadoesnotdespawn` | a taken llama lets go of its trader and its despawn timer is out of reach |
| `achunksavedaheadofthejournalstopsaresenttransfer` | a chest's chunk saved after an engine transfer, the journal not: the resent transfer is refused by the chunk's own record, which is dropped once the engine acknowledges |
| `atakencreaturecarriesitstakenuntiltheenginehasit` | a tamed llama carries its TAKEN; a copy read from disk journals it again with the same op id; both drop it on the acknowledgement |
| `adeadmembersdropscarryitsdied` | the items a dying llama drops carry its MEMBER_STATE and DIED |
| `rtsimcommandsrun` | `/rtsim perf` and `/rtsim stub caravan` through the server's command dispatcher |
| `dotraderspawningfollowsthetradeandtravelswitches` | the gamerule follows the trade and travel switches, and the engine hears the module mask |
| `acaravansofferscomefromitschestcargo` | phase 1: the engine's offers replace the trader's; uses are what the llamas carry; a sale takes the goods out of a chest and the emerald in; sold out, the offer shows crossed out; TRADE per sale with an op id |
| `sellingdrainsthetreasuryandtheoffercrossesout` | walkthrough B: a farmer's wheat offer repriced from the market (21, 22, 23 wheat per emerald as the stock grows), the treasury drained by three sales, crossed out; a stale snapshot does not refund; a refill reopens it |
| `killingallamadropsitscargoexactlyonceacrossacrash` | DIED with an op id; the dropped cargo's chunk saved and read back through vanilla's entity storage after a "crash"; the DIED arrives again under the same op id and the cargo is in the world once |
| `gossipiswrittenonce` | WRITE_GOSSIP adds 25 once; a resend is DUPLICATE; after a crash that rolled back the journal and the village, the villager's own record refuses it; VANILLA_KNOWS; a villager that was away gets the news at the census |
| `ahouseisplacedonlyonafreeplot` | not over a player's block, a block entity or next to another house; placed from a template with its jigsaw replaced; HOUSE_PLACED names the slot; a resend is DUPLICATE |
| `arescueisreported` | a player kills a zombie targeting the trader: RESCUE |
| `damagetoacaravanisreportedonceperwindow` | three hits by one player are one HURT |
| `cargotakenfromachestisreported` | emeralds that leave a chest outside a trade: CARGO_CHANGED -12 |
| `aplayerbaseismarked` | a long stay marks a base: BASE MARKED; trades there name it |
| `avillagejoinswhenitsstructurestartgenerates` | a real `village_plains` start through `setStartForStructure`: VILLAGE_ADOPTED, culture human, its pieces |

The plugins' GameTests (`mic_rtsim_ribbits`, `mic_rtsim_carryon` namespaces):

| Test | What it shows |
|---|---|
| `asorcererskipsadislikedplayer` | walkthrough F: a friend is buffed, a disliked player left out; alone with the disliked player the sorcerer does not cast; forgiven, it does |
| `aribbithousegoesonafreeplotandaresidentmovesin` | walkthrough H: `ribbits:houses/small_house_red_1` with `ribbits:main` on the first free plot of a search, not over a player's block; a nitwit spawns at its `ribbits:ribbits` slot, homed there |
| `aribbitmerchantsellsoutofitspackanddropsitwhenkilled` | a ribbit merchant caravan sells out of its pack; killed, the pack drops once with the DIED |
| `atouringbandplaysarounditsownmaster` | walkthrough G: four musicians play around their own master next to a village band that is already playing |
| `ribbitmerchantstradeandrestockthroughthetreasury` | C23: a ribbit merchant repriced from its village market; a disliked player brings twice the slime for a shard; a restock keeps the treasury's uses |
| `acarryonpickuptakesthecreatureoutofthesim` | Carry On's `EntityPickupEvent`: TAKEN(PICKUP) with the cargo; `Entity.save` works on the llama afterwards |

## Not done yet

- **The engine.** rtsim-jni and the Rust step do not exist; `StubEngine` stands in.
- **Observations** not yet sent: chunk loads and tickets, recipes (PLAN.md §6.3). Phase 1's are in
  docs/boundary.md §4.
- **Phase 1 open ends** (docs/overnight-choices-phase1mc.md): a crash between a new house's chunk save
  and the next world save can let a resent PLACE_HOUSE build twice; villager gossip ops are capped at 64;
  robbery is attributed by proximity; puppet offers cannot yet be repriced from a ribbit village's
  sentiment (only residents are); border-market currency exchange (P21) is not offered.
- **Item components** in captured cargo (B23).
- **Picked-up drops**: an item picked up before the engine acknowledges its DIED takes the record
  into a player's inventory, where nothing carries it (docs/boundary.md §3.1).
- **A failed journal write** is still reported as saved (WORLD_SAVED), because NeoForge's SavedData
  write logs the IOException and the chain goes on.
- **Body kinds as data** (P15): group layouts are code.
- **A GameTest for a server at simulation distance 10** (risk R1): it needs real players.
- **The real engine**: every phase 1 test scripts the engine side through the stub.
