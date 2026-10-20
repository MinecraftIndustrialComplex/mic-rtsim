# mic-rtsim: the NeoForge side

The Java half of mic-rtsim (PLAN.md §1, §6, §8): the core bridge mod **`mic_rtsim`** and one project
per plugin (`mic_rtsim_<mod>`, PLAN.md §1.3). Minecraft 1.21.1, NeoForge 21.1.249, ModDevGradle
2.0.74, Parchment 2024.11.17, Java 21: the same pins as `mods/mic-climate`.

| Project | Mod id | Built around | What |
|---|---|---|---|
| `core` | `mic_rtsim` | NeoForge and mic_native, MIC's native host (no pack mod, D4) | the bridge, and phase 1's vanilla side: villages, markets, caravans, gossip, damage, rescues, robbery, player bases, growth; and the vanilla herd adapter (sheep, cows, pigs, horses and goats, docs/boundary.md §8) |
| `ribbits` | `mic_rtsim_ribbits` | Ribbits 4.1.6 (with YUNG's API 5.1.9, GeckoLib 4.9.3) | ribbit villages and residents, merchant caravans, touring bands, sorcerers who remember, growth from YUNG's houses (C7, C23) |
| `carryon` | `mic_rtsim_carryon` | Carry On 2.2.6.13 | a creature picked up leaves the sim (A10) |
| `create` | `mic_rtsim_create` | Create 6.0.10 (its jar-in-jar Ponder, Flywheel and Registrate for compiling) | the industry plugin (INDUSTRY.md §12 layer 2): kinetic, belt, pipe and chain merging, vault and tank controllers, packager targets, postboxes counted by package contents, station names, train arrivals and departures, carriage deltas through `TrainCargoManager`, the "District ledger" display source; and every industry GameTest |
| `iceandfire` | `mic_rtsim_iceandfire` | Ice and Fire CE 2.1-beta.1 (with Uranus 2.4.1, Jupiter 2.3.7, GeckoLib 4.9.3; Architectury 13.0.11 at runtime) | phase 2's dragons (PLAN.md §9, docs/boundary.md §6): the dragon body adapter (materialize with the engine's state, steer, depart, capture), wounds, deaths, meals, lairs and robbed hoards, and every other dragon reported untagged for the engine to adopt or discard (P13); no mixin. Choices Z1 to Z21 in `docs/overnight-choices-iceandfire.md` |

The plugins compile against the pack's own jars, read from `packMods` in `gradle.properties` (the MIC
smoke server's mods folder next to this repository; `-PpackMods=...` to point elsewhere), and their
GameTest servers load those jars. Every reach into another mod is in `../PATCHES.md`.

The core depends on no pack mod (D4): only NeoForge and `mic_native`, MIC's own native host, whose
plugin the engine is. The engine is `NativeEngine`, the Rust engine (crates/rtsim-jni) through JNI,
picked by the server config `engine.implementation = "native"`. `"stub"` picks `StubEngine`, a
pure-Java stand-in that speaks the boundary protocol exactly and simulates nothing, for tests. If the
native engine does not load there is no session (no fallback, A8): the log and `/rtsim perf` say why.

## Build and test

All heavy jobs on the shared laptop go through one lock, niced, with 4 workers:

```sh
cd neoforge
# the jar: core/build/libs/mic_rtsim-1.21.1-0.1.0.jar
flock /tmp/mic-rtsim-bridge-heavy.lock nice -n 10 nix shell nixpkgs#jdk21 -c ./gradlew --max-workers=4 :core:build

# unit tests only (pure Java, seconds)
flock /tmp/mic-rtsim-bridge-heavy.lock nice -n 10 nix shell nixpkgs#jdk21 -c ./gradlew --max-workers=4 :core:test

# unit tests, then the GameTests on a headless server; exits non-zero on any failure
./gametest.sh                                  # the server's engine: native (builds rtsim-jni with cargo)
./gametest.sh -PskipJni -PrtsimEngine=stub     # the same tests on the stub, no Rust needed
```

- **mic_native** comes from its own jar, by default the sibling checkout's
  `../../mic-native/neoforge/build/libs/mic_native-1.21.1-0.1.0.jar` (`-PmicNativeDir`,
  `-PmicNativeJar` to point elsewhere): compiled against, and a mod in the runs (`localRuntime`).
  `buildMicNative` runs mic-native's own build when the jar is missing.
- **The engine's library** is packed as a mic_native plugin: `cargoBuildJni` (cargo, so the
  repository flake's dev shell) and `jniNatives` stage `natives/mic_rtsim/natives.json` and
  `natives/mic_rtsim/<platform>/baseline/librtsim_jni.so`. `-PskipJni` packs none, so the Java side
  builds before the Rust crate does (the native engine then cannot load); `-Prelease` packs
  `../target/natives-dist-jni/` (every platform) instead.
- The phase 1 GameTests that script commands (markets, offers, gossip writes, houses, residents,
  sentiment) order them through `ScriptedEngine`, so they run on both engines
  (`./gametest.sh -PrtsimEngine=stub|native`). `RtsimTests.adoptArena` adopts a scripted village
  (`ScriptedEngine.scriptVillage`: the engine decides nothing for it); `RtsimTests.adoptArenaLive`
  adopts one the engine drives itself.
- `-PrtsimEngine=stub|native` picks the GameTest server's engine (`-Dmic_rtsim.engine`, honoured
  only with `-Dmic_rtsim.gametest=true`). Tests that run their own sessions get an engine of the same
  kind (`RtsimTests.freshEngine`); the native one keeps its pages under `run-gametest/mic_rtsim-test/`.

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
| `engine` | `NativeEngine` (the JNI natives of rtsim-jni, loaded through `MicNative.loadPlugin`), `StubEngine` (the four buffers, a worker thread, pages that survive a simulated crash), and `ScriptedEngine`, the test and debug surface both implement: orders, the received log, inspection, `crashAndReopen`. `NativeCodec` encodes the orders and decodes the received log. |
| `bridge` | the server session (`RtsimSession`): puppet pass, spawn, capture, taking, observations; `CommandDispatcher`; the NeoForge hooks (`BridgeEvents`); the `doTraderSpawning` switch (`TraderSpawning`). |
| `puppet` | the `mic_rtsim:agent` attachment (engine id, epoch), groups and the live registry. |
| `body` | the adapter interface (spawn, capture, steer), group kinds, and the vanilla adapters for `minecraft:wandering_trader` and `minecraft:trader_llama`, plus the human caravan group. |
| `config` | the server config: one switch per module, PLACEHOLDER budgets. |
| `mixin` | `EntitySaveMixin` (`shouldBeSaved`, `saveAsPassenger`), `LlamaInvoker` (`setStrength`), two accessors for the save GameTests; phase 1: `StructureManagerMixin` (adoption), `MerchantResultSlotMixin` (trades), `VillagerRestockMixin` (restock and demand), `VillagerReputationMixin` (reputation trim), `MerchantOfferAccessor`. All in `../PATCHES.md`. |
| `api` | `RtsimApi` for plugins: register bodies and groups, `takeFromSim` (Carry On's hook); phase 1: cultures, residents, resident factories, a village and its sentiment, the restock and demand hooks. |
| `command` | `/rtsim perf`, `/rtsim stub caravan` (orders either engine to spawn a caravan) and phase 1's demos. |
| `village` | phase 1: the village registry (`data/mic_rtsim_villages.dat`), adoption (C22), cultures, residents, the census, gossip writes (C20). |
| `market` | phase 1: the price curve and market book (a mirror of rtsim-trade), resident repricing through the treasury (C14), puppet merchant offers and cargo (C21), robbery detection. |
| `knowledge` | phase 1: sentiment snapshots, HURT coalescing, rescues (C24). |
| `base` | phase 1: player bases as market nodes (C26, P30's recommendation; `data/mic_rtsim_bases.dat`). |
| `growth` | phase 1: player-placed block marks, house placement from templates, resident spawns (C7). |
| `bridge/Phase1` | wires phase 1 to the tick, the events and the commands. |
| `industry` | the industry module (C15 to C19, INDUSTRY.md, docs/industry-boundary.md): district detection and the index (`Districts`, `data/mic_rtsim_industry.dat`), the boundary stock sampler (`Sampler`), player ports (`PlayerPorts`), reconciles and damage (`Reconciler`, the `mic_rtsim:reconcile` chunk attachment), the read-only ledger (`Ledgers`), GOOD names (`Goods`), `/rtsim district` (`IndustryCommands`), its own server config (`mic_rtsim-industry-server.toml`) and the plugin API (`IndustryPlugin`, `IndustryApi`). Mod-agnostic: capabilities and NeoForge events only. Choices H1 to H29 in `docs/overnight-choices-industrymc.md`. |
| `herd` | the vanilla herd adapter (docs/boundary.md §8.5, choices Y8 to Y21 in `docs/overnight-choices-herdsmc.md`): a `SessionPlugin` the core registers itself (`HerdBridge`, commands 0x8700 to 0x87FF, agents 0x48) that materializes the engine's herd groups as vanilla animals with the engine's look, age, shearing and identity seed (`HerdBodies`), steers them toward the herd's waypoints with vanilla AI otherwise (`HerdFollowGoal`), captures them with each member's state, reports kills, shearing, takings and obstacles (`HerdRecords`, `HerdEvents`). |
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
| `afailedenginestopsthebridgeforthesession` | an engine that fails (a panic in a native step) hands back empty buffers flagged FAILED, and the bridge stops for the session instead of waiting on it |
| `handovercostpertick` | the hand-over bench: submit to publish on the native engine (with and without a running phase 1 world) and the stub, burst and 50 ms cadence (only with `-PrtsimBench`; results in `bench/results/handover.md`) |
| `rtsimcommandsrun` | `/rtsim perf` and `/rtsim stub caravan` through the server's command dispatcher |
| `dotraderspawningfollowsthetradeandtravelswitches` | the gamerule follows the trade and travel switches, and the engine hears the module mask |
| `acaravansofferscomefromitschestcargo` | phase 1: the engine's offers replace the trader's; uses are what the llamas carry; a sale takes the goods out of a chest and the emerald in; sold out, the offer shows crossed out; TRADE per sale with an op id |
| `sellingdrainsthetreasuryandtheoffercrossesout` | walkthrough B: a farmer's wheat offer repriced from the market (21, 22, 23 wheat per emerald as the stock grows), the treasury drained by three sales, crossed out; vanilla's `restock()` leaves it crossed out with demand 0; a stale snapshot does not refund; a refill reopens it |
| `arobberythenatradearereportedapart` | 5 wheat taken from a chest and a sale of 10 in the same tick: one CARGO_CHANGED of -5 and one TRADE of 10 |
| `twoscreensshareonetreasury` | two players at two farmers of one village: two sales at one re-cap the other's open offer to the one emerald left |
| `arealcaravantradesisrobbedandthenewstravels` | phase 1 decided by the native engine: three villages far away, a caravan off its flows materialized at a player (`P1_SPAWN_NEAR`) with its real chest cargo and offers, a trade and a robbery booked once, and the robbery written into two other villages' gossip days later (`P1_ADVANCE`); skipped on the stub |
| `killingallamadropsitscargoexactlyonceacrossacrash` | DIED with an op id; the dropped cargo's chunk saved and read back through vanilla's entity storage after a "crash"; the DIED arrives again under the same op id and the cargo is in the world once |
| `gossipiswrittenonce` | WRITE_GOSSIP adds 25 once; a resend is DUPLICATE; after a crash that rolled back the journal and the village, the villager's own record refuses it; VANILLA_KNOWS; a villager that was away gets the news at the census |
| `moregossipthantheopringisstillwrittenonce` | seventy entries (more than the 64-op ring): two censuses write none of them again; a villager that arrives later takes each once |
| `ahouseisplacedonlyonafreeplot` | not over a player's block, a block entity or next to another house; placed from a template with its jigsaw replaced; HOUSE_PLACED names the slot; a resend is DUPLICATE |
| `arescueisreported` | a player kills a zombie targeting the trader: RESCUE |
| `damagetoacaravanisreportedonceperwindow` | three hits by one player are one HURT |
| `cargotakenfromachestisreported` | emeralds that leave a chest outside a trade: CARGO_CHANGED -12 |
| `aplayerbaseismarked` | a long stay marks a base: BASE MARKED; trades there name it |
| `avillagejoinswhenitsstructurestartgenerates` | a real `village_plains` start through `setStartForStructure`: VILLAGE_ADOPTED, culture human, its pieces |
| `aherdgroupmaterializeswiththeengineslookageandseeds` | herds (scripted, either engine): sheep with the engine's colours, shearing and a lamb's `Age`, horses with the engine's coat and the stats their seeds roll, a screaming goat; tagged, persistence-required, never saved, on the floor; HERD_MATERIALIZED, HERD_GROUP_STATE; HERD_DESPAWN captures them (the sheared one captured sheared) |
| `killingaherdanimalreportsitandthegroupshrinks` | a player's kill is HERD_KILLED naming the player, with a Java op id, its drops carrying it; a death with no killer is by none; the group goes on smaller |
| `aleadtamingandapickuptakeanimalsoutandshearsmarkone` | a lead, taming and the core's pickup hook each take an animal out (HERD_TAKEN with the player; the animal ordinary, carrying its record); shears are HERD_SHEARED with the player, the sheep stays in its group |
| `acapturekeepseachmembersstateandtheseedbringsbackthesamehorse` | a capture keeps a horse's wound and position; materialized again from its seed, each horse has the same max health, speed and jump |
| `aherdwalkstowarditswaypointandturnsonsteer` | sheep (their goal) and goats (their brain's walk target) walk toward the waypoint and turn back on HERD_STEER |
| `asaveandreloadduplicatesnothing` | vanilla's entity-chunk save leaves the herd's puppets out and keeps a taken sheep; copies of a puppet never join |
| `afenceproducesanobstaclereport` | sheep steered past a fence stall at it; HERD_OBSTACLE once, from before the fence, toward the waypoint |
| `theenginesownherdmaterializesnearaplayerandremembersitssheep` | native engine only: a herd founded with HERD_FOUND far out, a player beside it; the engine's group with the engine's colours; a kill drops the engine's count; the player leaves and comes back: the same sheep, never the dead one |

The plugins' GameTests (`mic_rtsim_ribbits`, `mic_rtsim_carryon` namespaces):

| Test | What it shows |
|---|---|
| `asorcererskipsadislikedplayer` | walkthrough F: a friend is buffed, a disliked player left out; alone with the disliked player the sorcerer does not cast; forgiven, it does |
| `aribbithousegoesonafreeplotandaresidentmovesin` | walkthrough H: `ribbits:houses/small_house_red_1` with `ribbits:main` on the first free plot of a search, not over a player's block; a nitwit spawns at its `ribbits:ribbits` slot, homed there |
| `aribbitmerchantsellsoutofitspackanddropsitwhenkilled` | a ribbit merchant caravan sells out of its pack; killed, the pack drops once with the DIED |
| `atouringbandplaysarounditsownmaster` | walkthrough G: four musicians play around their own master next to a village band that is already playing |
| `ribbitmerchantstradeandrestockthroughthetreasury` | C23: a ribbit merchant repriced from its village market; a disliked player brings twice the slime for a shard; a restock keeps the treasury's uses |
| `acarryonpickuptakesthecreatureoutofthesim` | Carry On's `EntityPickupEvent`: TAKEN(PICKUP) with the cargo; `Entity.save` works on the llama afterwards |
| `acarryonpickuptakesaherdanimaloutofthesim` | the same event on a herd sheep: HERD_TAKEN(PICKUP) with the player and a Java op id, through the same hook (choice Y8); the rest of the group stays |

The Ice and Fire GameTests (`mic_rtsim_iceandfire` namespace; Ice and Fire CE 2.1-beta.1's own jar and the
pack's `iaf-common.json`; either engine: the tests script the dragons' commands, on the native engine as
DRAGON_COMMAND orders, and the native engine answers untagged dragons itself; each in its own batch):

| Test | What it shows |
|---|---|
| `adragonmaterializeswiththeenginesstateandsteerstoitstarget` | the engine's AgeTicks (aging disabled), Hunger, Gender, Variant (`bronze`), health and home on a tagged, persistence-required dragon; it flies to its first target and turns on DRAGON_STEER; DRAGON_STATE; DRAGON_DEPART captures it with health, hunger, age and position |
| `aplayershitishurtandakillisdiedwiththekiller` | two hits in a tick are one DRAGON_HURT with the player; a kill is DRAGON_DIED naming the killer, after the lethal blow's HURT; the corpse stays tagged until the engine's save has the death |
| `untaggeddragonsareadoptedordiscarded` | a spawn-egg dragon and a NATURAL one (In Control's spawn type) reported DRAGON_UNTAGGED; the first adopted, the second discarded |
| `alairsdragonisadoptedanditshoardrobbed` | a dragon with no spawn type in a lair's box is that lair's STRUCTURE dragon (after LAIR_FOUND), adopted with the lair's home; a gold pile broken and ingots taken from a lair chest are one HOARD_ROBBED |
| `apuppetdragonisneversavedandacopyneverjoins` | vanilla's entity-chunk save leaves the dragon out; copies built from its NBT (new UUID, older epoch) never join |
| `atamedorfarawaypuppetleavesthesim` | a tamed puppet leaves the sim as a DRAGON_DIED with no killer (no TAKEN for dragons) and stays the player's; one carried out of the ticking area is captured and removed |
| `adragonsmealinavillageisreported` | a kill of a village's cow is DRAGON_ATE for the village; a cow carried at departure is eaten with the capture (C25) |

The industry GameTests (`mic_rtsim_create` namespace; Create 6.0.10's own jar; always the stub engine,
which scripts the engine's industry commands; each in its own batch):

| Test | What it shows |
|---|---|
| `adistrictisdetectedacrossshaftsandnamed` | I2: a player's chest and Create shafts across two chunk borders make one three-chunk district; it is named by its coordinates; a repeated detection keeps its id and revision (and is timed); `/rtsim district rename` and `split` (IND_EDIT 4); a shaft across the split makes both not learnable (rotation) |
| `adistrictisnamedafteritsstation` | a real `create:track_station` on a real track graph names its district; IND_STATION carries id, district and name |
| `asweepreportsboundarystockandtheliveflag` | I3, F5: a chest, a vault and a postbox in two chunks; a live sweep reports them all, the package by its contents, and IND_SWEEP_END's record count matches; with an unloaded member chunk the sweep is partial and IND_LOAD_STATE says so; IND_WATCHED is sent (no player: not watched) |
| `atrainarrivalisobserved` | the `Train.arriveAt` mixin: IND_TRAIN_ARRIVED with the carriage's 32 ingots, not simulated; ten unloaded at the stop are an IND_PORT (+10) at `leaveStation`; IND_TRAIN_DEPARTED follows |
| `asimulatedstationexchangesthroughthecargomanager` | I15: at an unloaded district's station the arrival is simulated; IND_CARRIAGE_DELTA applies through `TrainCargoManager` (removals first), IND_CARRIAGE_RESULT; the same op id again is DUPLICATE |
| `areconcileappliesonceacrossasimulatedcrash` | I8: +20 iron and epoch 1 in the chunk; the op resent and a new op for epoch 1 are both DUPLICATE; a chunk restored to before the reconcile takes it once; epoch 2 on top; a player's block is one IND_EDIT, a block set otherwise none |
| `removalsgofirstandashortfallisreported` | a full chest: +64 iron fits only because the removals go first; -50 diamonds of 10 is reported as -10 applied |
| `abrokenblockdropsitscontents` | C18, I23: a destroy entry with the expected state breaks the chest with drops (12 emeralds on the ground), one with a changed state is skipped, no fire; the result names the block |
| `aheldreconcileapplieswhenitschunkloads` | §4.1: a far chunk's reconcile is held while it is unloaded, applied at its load after IND_CHUNK_LOADED; after a real unload and reload the epoch comes back from the chunk's NBT |
| `theledgershowsonadisplay` | I18: a Display Link on a Stock Link shows the district's ledger (name, state, +120/h, "would have made 500") on a sign; IND_LEDGER_REQUEST |
| `mailatanunloadeddistrictisapackagerecord` | F26, H30: a package dropped into an unloaded district's postbox buffer is IND_PACKAGE in (address, 40 iron and 3 coal), one fetched is IND_PACKAGE out |
| `playerportsarecounted` | §3.3, H14: a fake player's container session on a district chest (+5 iron, with the chest's index), a toss (+3) and a pickup (-2) through NeoForge's own events; none is an edit |
| `adoublechestisoneinventory` | H8: two chest halves are one 54-slot inventory at the lower half; the canonical lookup never loads a chunk |
| `sweepcostonlargevaults` | INDUSTRY.md §19 risk 2: a 3 x 3 x 9 vault is one inventory; read cost per inventory and per slot, logged (H10) |

## Not done yet

- **A simulating engine.** The native engine's world (rtsim-jni `world.rs`) keeps the books of the
  records above and writes the commands tests and `/rtsim` order; it decides nothing yet. Phase 1's
  modules replace it.
- **Natives in dev builds** are this machine's baseline build only. The release set (eight platforms
  plus x86-64-v3) comes from `tools/natives/build-natives.sh` in mic-native's dev shell
  (`target/natives-dist-jni/`, packed by `-Prelease`); it built and passed every GameTest on
  2026-10-20, but only linux-x86_64 has been run.
- **Observations** not yet sent: chunk loads and tickets, recipes (PLAN.md §6.3). Phase 1's are in
  docs/boundary.md §4.
- **Phase 1 open ends** (docs/overnight-choices-phase1mc.md, docs/boundary.md §4.3): a crash between a
  new house's chunk save and the next world save can let a resent PLACE_HOUSE build twice, and
  HOUSE_PLACED is not carried by the chunk; a house stopped part way leaves its placed layers; a crash
  that rolls back a village's gossip sequence but not a villager makes that villager skip news up to its
  old mark; the reputation trim (G31) leaves one bundle's spread; robbery is attributed by proximity; puppet offers cannot yet be repriced from a ribbit village's
  sentiment (only residents are); border-market currency exchange (P21) is not offered.
- **Item components** in captured cargo (B23).
- **Picked-up drops**: an item picked up before the engine acknowledges its DIED takes the record
  into a player's inventory, where nothing carries it (docs/boundary.md §3.1).
- **A failed journal write** is still reported as saved (WORLD_SAVED), because NeoForge's SavedData
  write logs the IOException and the chain goes on.
- **Body kinds as data** (P15): group layouts are code.
- **A GameTest for a server at simulation distance 10** (risk R1): it needs real players.
- **The real engine's own phase 1 decisions** are GameTested only by `arealcaravantradesisrobbedandthenewstravels`
  (a caravan off the engine's flows, its trade, robbery and news); markets, sentiment, houses and
  residents the engine decides are covered by rtsim-jni's protocol test and rtsim-world's tests, and
  the GameTests that check how the bridge applies them script them (on either engine).
- **Industry** (docs/overnight-choices-industrymc.md): IND_PACKAGE for loaded districts (H30), IND_WRITE_PACKAGE,
  IND_CHANNEL_DELTA and IND_DEPOSIT_DELTA (answered FAILED: no plugin applies them yet), deposits in
  reconciles, IND_CONDITIONS (seasons), the threat records (IND_SPAWN_SAMPLE, IND_FIGHT, IND_SURVEY),
  the I16 gauge mixins, the PowerGrid, Destroy, Ore Excavation and Ender Transmission plugins; IND_WATCHED
  with a real player near and the simulation-distance rule are not covered by a GameTest (no real
  player); a real assembled train on a schedule is not tested (the station hooks run on synthetic
  trains).
- **Ice and Fire** (docs/overnight-choices-iceandfire.md): the pack's In Control edits P13 needs are a
  proposal in `iceandfire/pack-proposal/` (not applied); a real lair structure, settling, and the
  not-ticking, dimension-change and session-end captures are not covered by a GameTest.
- **Herds** (docs/overnight-choices-herdsmc.md): nothing in the game founds a herd yet (only the test
  order HERD_FOUND, Y4), and the engine's climate is uniform grassland (Y3); vanilla's own animals are
  neither adopted nor suppressed (H2-28, Y20); HERD_GROUP_STATE is not read by the engine. Steering
  over a long walk, a dispenser's shearing, and the dimension-change, session-end and
  switch-off captures are not covered by a GameTest.
