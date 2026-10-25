package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Districts;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Goods;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.TickGate;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassConfig;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassReport;
import com.minecraftindustrialcomplex.mic_rtsim_create.CreateIds;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.TrainWatch;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.GlassExitKit;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.contraptions.actors.psi.PortableStorageInterfaceBlock;
import com.simibubi.create.content.logistics.vault.ItemVaultBlock;
import com.simibubi.create.content.trains.bogey.AbstractBogeyBlock;
import com.simibubi.create.content.trains.entity.Carriage;
import com.simibubi.create.content.trains.entity.CarriageBogey;
import com.simibubi.create.content.trains.entity.Train;
import com.simibubi.create.content.trains.station.GlobalStation;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestGenerator;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestInfo;
import net.minecraft.gametest.framework.GameTestListener;
import net.minecraft.gametest.framework.GameTestRunner;
import net.minecraft.gametest.framework.TestFunction;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.Rotation;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.items.IItemHandler;
import net.neoforged.neoforge.items.ItemHandlerHelper;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.TreeSet;
import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.at;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.block;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.fill;

/**
 * A station's call while its district is away (docs/stress-worlds-results.md 10.5 and 10.8, choices SG32
 * to SG35): it books against the station's own port inventories, never the factories' input chests, and
 * loads at most what a real call moved.
 *
 * <p>A saw shafts factory (corpus 5: its input chest feeds the saw through a smart chute filtered to
 * alloy) is built far from the yard, as {@link GlassReplayGameTests} builds its far copy, with copper and
 * gold ingots lying in its input chest beside the alloy. Next to it stands a station port as the stress
 * world builds one ({@code StressWorld.port}): sink vault, smart chute pulling gold, a Portable Storage
 * Interface, smart chute pushing copper, supply vault of copper. A synthetic train makes one loaded call:
 * it brings {@value #LEARNED} gold, the test moves them into the sink vault and {@value #LEARNED} copper
 * from the supply vault into the carriage (the interface's and the chutes' work), records the interface
 * as docked ({@link TrainWatch#dockedForTest}: a synthetic train has no carriage entity) and the train
 * leaves. That call is the station's exchange (unload gold, load copper, {@value #LEARNED} each), and
 * its departure journals the station's ports, which must be the two vaults and nothing else.
 *
 * <p>Once the factory's replay is eligible its chunks unload. The train comes back with
 * {@value #LEARNED} gold and arrives simulated; the chunks load again and the reconcile lands. Then:
 * <ul>
 *   <li>the factory's input chest holds exactly the copper and gold it held at the unload;</li>
 *   <li>the supply vault lost copper, at least one and at most the learned {@value #LEARNED}; the sink
 *       vault gained gold, at least one and at most {@value #LEARNED}.</li>
 * </ul>
 * On the native engine the live engine decides, and its IND_CARRIAGE_DELTA must reach the carriage. On
 * the stub with the natives present, the reconcile is the offline engine's over the same records (as in
 * the replay tests); without natives only the records are checked. {@code MIC_GLASS_SUITE=station} runs
 * this test without the glass suites.
 */
@GameTestHolder(MicRtsimCreate.MODID)
public final class StationCargoGameTests {
    private StationCargoGameTests() {}

    /** What the loaded call moves each way: the station's per-call amount. */
    static final int LEARNED = 30;
    static final int CHEST_COPPER = 256, CHEST_GOLD = 128, SUPPLY_COPPER = 512;
    /** The far site, in whole chunks along x: clear of the replay suite's far copies (4,000 + 16 per slot). */
    static final int FAR_CHUNKS = 6400;
    /** Ticks the far site stays unloaded at least. */
    static final int AWAY = 1200;
    /** Ticks after the engine saw the unload before the train arrives. */
    static final int ARRIVE_AFTER = 200;
    /** Ticks the native engine gets to answer the simulated call with IND_CARRIAGE_DELTA. */
    static final int RESULT_WAIT = 600;
    /** The district must keep its revision this long before the loaded call (the vaults form on their first tick). */
    static final int STABLE = 100;

    @GameTestGenerator
    public static Collection<TestFunction> stationCargo() {
        String suite = System.getenv("MIC_GLASS_SUITE");
        if (suite != null && !suite.isBlank() && !suite.trim().equals("station"))
            return List.of();
        CorpusFactory f = new SawShaftsFactory();
        int maxTicks = f.warmupTicks() + 2 * (GlassReplayGameTests.WINDOW + GlassExitGameTests.SLACK) + AWAY + RESULT_WAIT
                + 3 * GlassReplayGameTests.IO_TICKS + 4 * GlassReplayGameTests.SETTLE + 4000;
        return List.of(new TestFunction("stationcargo", "stationcargo.saw_shafts", MicRtsimCreate.MODID + ":" + IndustryKit.YARD,
                Rotation.NONE, maxTicks, 0L, true, false, 1, 1, true, h -> start(h, f)));
    }

    /** CONTROL: a fixture's own steps after its verdict (SEC-1's {@code StationPressLineGameTests}). */
    enum Stage { LOAD, DISTRICT, RUN, UNLOAD, AWAY, RELOAD, SETTLE, CONTROL, DONE }

    static final class Run {
        final GameTestHelper helper;
        final GlassReplayGameTests.Far far;
        final CorpusFactory f;
        final String test;
        /**
         * The fixture's own verdict at the end of the settle (null: this test's). SEC-1's {@code StationPressLineGameTests}.
         * It may go on to a stage of its own; else the run is done.
         */
        java.util.function.Consumer<Run> verdict;
        /** What the fixture keeps between ticks. */
        final Map<String, Object> mine = new java.util.HashMap<>();
        /** What the trains carry: the partner's cargo on the simulated call. */
        Item cargo = Items.GOLD_INGOT;
        final GlassExitKit.Recorder rec;
        final boolean judge;
        final boolean stub;
        final List<ChunkPos> farChunks = new ArrayList<>();
        final BlockPos farMin, farMax;
        Stage stage = Stage.LOAD;
        int ticks, stageTicks;
        CorpusFactory.Built copy;
        long builtAt = -1;
        BlockPos input, psi, sinkVault, supplyVault, stationPos;
        GlobalStation station, partner;
        Train train;
        long fd = -1;
        int revision = -1, supplyIdx = -1, sinkIdx = -1;
        long stableSince = -1;
        boolean filled, called, portsSeen, arrivalSeen;
        long t1 = -1, unloadedAt = -1, awayAt = -1, arrivedAt = -1, leftAt = -1, forcedAt = -1, loadedAt = -1, quietSince = -1;
        int heldBase;
        Map<String, Long> chest1, supply1, sink1;
        Map<String, Long> carriageMoved;
        boolean offlineDone;
        long nextOfflineAt = -1;
        String eligibleBy = "";
        String last = "starting";
        final List<String> failures = new ArrayList<>();

        Run(GameTestHelper helper, CorpusFactory f) {
            this(helper, f, "station cargo", FAR_CHUNKS);
        }

        Run(GameTestHelper helper, CorpusFactory f, String test, int farChunksAlongX) {
            this.helper = helper;
            this.f = f;
            this.test = test;
            this.far = new GlassReplayGameTests.Far(helper, new BlockPos(16 * farChunksAlongX, 0, 0));
            this.rec = GlassExitKit.start(helper.getLevel());
            this.judge = GlassExitKit.canJudge();
            this.stub = !IndustryKit.nativeEngine();
            farMin = far.absolutePos(BlockPos.ZERO);
            farMax = far.absolutePos(new BlockPos(33, GlassReplayGameTests.TOP, 33));
            for (int cx = farMin.getX() >> 4; cx <= farMax.getX() >> 4; cx++)
                for (int cz = farMin.getZ() >> 4; cz <= farMax.getZ() >> 4; cz++)
                    farChunks.add(new ChunkPos(cx, cz));
        }

        ServerLevel level() {
            return helper.getLevel();
        }

        long now() {
            return level().getGameTime();
        }

        void stage(Stage s) {
            IndustryKit.record(test, "stage", s + " at " + now() + " (test tick " + ticks + ")");
            stage = s;
            stageTicks = 0;
        }
    }

    static void start(GameTestHelper helper, CorpusFactory f) {
        GlassConfig.Test.set(GlassConfig.CHECKSUM_WINDOW_TICKS, GlassReplayGameTests.WINDOW);
        IndustryKit.fastConfig();
        Run run = new Run(helper, f);
        IndustryKit.record(run.test, "far site", "offset " + run.far.offset.toShortString() + ", chunks " + run.farChunks + ", engine "
                + IndustryKit.engine().describe() + (run.judge ? "" : ", NOT judged (no natives)"));
        for (ChunkPos c : run.farChunks)
            run.level().setChunkForced(c.x, c.z, true);
        helper.testInfo.addListener(new GameTestListener() {
            @Override
            public void testStructureLoaded(GameTestInfo info) {}

            @Override
            public void testPassed(GameTestInfo info, GameTestRunner runner) {
                release(run);
            }

            @Override
            public void testFailed(GameTestInfo info, GameTestRunner runner) {
                release(run);
            }

            @Override
            public void testAddedForRerun(GameTestInfo oldTest, GameTestInfo newTest, GameTestRunner runner) {}
        });
        helper.onEachTick(() -> {
            try {
                tick(run);
            } catch (RuntimeException e) {
                if (run.failures.isEmpty())
                    MicRtsim.LOGGER.error("[gametest] {}: the harness threw", run.test, e);
                run.failures.add("the harness threw at stage " + run.stage + ": " + e);
                run.stage = Stage.DONE;
            }
        });
        helper.succeedWhen(() -> {
            if (run.stage != Stage.DONE)
                throw new GameTestAssertException("waiting for " + run.test + " at stage " + run.stage + " (" + run.last + ")");
            if (!run.failures.isEmpty())
                throw new GameTestAssertException(String.join("; ", run.failures));
        });
    }

    static void release(Run run) {
        if (run.train != null)
            TrainWatch.forget(run.train);
        for (ChunkPos c : run.farChunks)
            run.level().setChunkForced(c.x, c.z, false);
    }

    static void tick(Run run) {
        if (run.stage == Stage.DONE)
            return;
        run.ticks++;
        run.stageTicks++;
        if (run.ticks % GlassReplayGameTests.POLL == 0)
            run.rec.poll();
        switch (run.stage) {
            case LOAD -> load(run);
            case DISTRICT -> district(run);
            case RUN -> runLoaded(run);
            case UNLOAD -> unload(run);
            case AWAY -> away(run);
            case RELOAD -> reload(run);
            case SETTLE -> settle(run);
            default -> {}
        }
    }

    static void fail(Run run, String why) {
        run.failures.add(why);
        IndustryKit.record(run.test, "FAIL", why);
        run.stage(Stage.DONE);
    }

    /** True once every chunk would tick block entities; fails the run after IO_TICKS of the stage. */
    static boolean ticking(Run run, Collection<ChunkPos> cs, String what) {
        String why = GlassReplayGameTests.firstNotTicking(run.level(), cs);
        if (why == null)
            return true;
        run.last = what + ": " + why;
        if (run.stageTicks > GlassReplayGameTests.IO_TICKS)
            fail(run, "the chunks never ticked (" + run.last + ")");
        else
            GlassReplayGameTests.ioWait();
        return false;
    }

    // ---- stages -------------------------------------------------------------------------------

    /** Builds the factory and the port once the far chunks and the yard's would tick. */
    static void load(Run run) {
        List<ChunkPos> cs = new ArrayList<>(run.farChunks);
        cs.addAll(GlassReplayGameTests.yardChunks(run.helper));
        if (!ticking(run, cs, "far chunks"))
            return;
        ServerLevel level = run.level();
        IndustryKit.forget(run.helper);
        ChunkPos lo = run.farChunks.get(0), hi = run.farChunks.get(run.farChunks.size() - 1);
        IndustryKit.ind().districts().forgetArea(level, lo.x - 1, lo.z - 1, hi.x + 1, hi.z + 1);
        GlassReplayGameTests.clear(level, run.farMin, run.farMax, run.farMin);
        GlassReplayGameTests.Far h = run.far;
        run.copy = run.f.build(h);
        run.input = run.copy.inputs().get(0);
        fill(h, run.input, Items.COPPER_INGOT, CHEST_COPPER);
        fill(h, run.input, Items.GOLD_INGOT, CHEST_GOLD);
        GlassReplayGameTests.seed(h, run.copy);
        // The port, as StressWorld.port builds it: sink vault (y 1 to 2), smart chute pulling gold out of the
        // interface, the interface facing south, smart chute pushing copper in, supply vault (y 6 to 7).
        run.psi = at(h, 12, 4, 8);
        run.sinkVault = run.psi.below(3);
        run.supplyVault = run.psi.above(2);
        run.stationPos = run.psi.below(4);
        vault(level, run.sinkVault);
        SteamHelpers.smartChute(h, run.psi.below(), Items.GOLD_INGOT, 64);
        level.setBlock(run.psi, AllBlocks.PORTABLE_STORAGE_INTERFACE.getDefaultState().setValue(PortableStorageInterfaceBlock.FACING, Direction.SOUTH), 3);
        SteamHelpers.smartChute(h, run.psi.above(), Items.COPPER_INGOT, 1);
        vault(level, run.supplyVault);
        for (BlockPos p : List.of(run.sinkVault, run.psi.below(), run.psi, run.psi.above(), run.supplyVault))
            IndustryKit.playerPlaced(h, p);
        int joined = GlassCorpus.join(h); // the factory and its port: one district wherever the chunk borders fall
        run.builtAt = run.now();
        GlassCorpusKit.log(run.f.name(), "station cargo built", run.copy.notes() + "; port at " + run.psi.toShortString()
                + "; chunk borders joined: " + joined);
        run.stage(Stage.DISTRICT);
    }

    static void vault(ServerLevel level, BlockPos corner) {
        BlockState v = AllBlocks.ITEM_VAULT.getDefaultState().setValue(ItemVaultBlock.HORIZONTAL_AXIS, Direction.Axis.X);
        for (int dx = 0; dx < 2; dx++)
            for (int dy = 0; dy < 2; dy++)
                for (int dz = 0; dz < 2; dz++)
                    level.setBlock(corner.offset(dx, dy, -dz), v, 3);
    }

    /**
     * Fills the supply vault once it has formed, waits for one district holding the factory, both vaults
     * and the station, at a revision that holds {@value #STABLE} ticks, then makes the loaded call.
     */
    static void district(Run run) {
        ServerLevel level = run.level();
        if (!run.filled) {
            if (run.stageTicks < 10)
                return;
            fill(run.far, run.supplyVault, Items.COPPER_INGOT, SUPPLY_COPPER);
            run.filled = true;
        }
        if (run.stageTicks % 20 != 0)
            return;
        District d = IndustryKit.districtAt(run.helper, run.copy.anchor());
        District atStation = IndustryKit.districtAt(run.helper, run.stationPos);
        District.Inventory supply = d == null ? null : d.inventoryAt(Districts.canonicalInventory(level, run.supplyVault));
        District.Inventory sink = d == null ? null : d.inventoryAt(Districts.canonicalInventory(level, run.sinkVault));
        if (d == null || atStation != d || supply == null || sink == null || d.inventoryAt(run.input) == null) {
            run.last = "district: factory " + d + ", station " + atStation + ", supply vault " + supply + ", sink vault " + sink;
            run.stableSince = -1;
            if (run.stageTicks > GlassReplayGameTests.IO_TICKS)
                fail(run, "no one district holds the factory, the port and the station (" + run.last + ")");
            return;
        }
        if (run.stableSince < 0 || d.revision != run.revision) {
            run.revision = d.revision;
            run.stableSince = run.now();
            return;
        }
        if (run.now() - run.stableSince < STABLE)
            return;
        run.fd = d.id;
        run.supplyIdx = supply.index();
        run.sinkIdx = sink.index();
        IndustryKit.record(run.test, "district", d + " (" + d.chunks.size() + " chunks, " + d.inventories.size() + " inventories, revision "
                + d.revision + "), supply vault inventory " + run.supplyIdx + ", sink vault inventory " + run.sinkIdx);
        loadedCall(run);
        run.stage(Stage.RUN);
    }

    /** The loaded call, in one tick: arrive, the interface's work, leave. */
    static void loadedCall(Run run) {
        ServerLevel level = run.level();
        run.station = syntheticStation(level, run.stationPos, "Cargo Halt");
        run.partner = syntheticStation(level, run.stationPos.offset(400, 0, 0), "Partner Halt");
        run.train = syntheticTrain(level, run.helper.absolutePos(new BlockPos(5, 1, 5)));
        IItemHandler car = run.train.carriages.get(0).storage.getAllItems();
        ItemHandlerHelper.insertItemStacked(car, new ItemStack(Items.GOLD_INGOT, LEARNED), false);
        run.train.arriveAt(run.station);
        TrainWatch.dockedForTest(run.train, run.station, level.dimension(), run.psi);
        int gold = move(car, items(level, run.sinkVault), Items.GOLD_INGOT, LEARNED);
        int copper = move(items(level, run.supplyVault), car, Items.COPPER_INGOT, LEARNED);
        run.train.leaveStation();
        // Create sets the next destination after leaveStation (the schedule's next instruction).
        run.train.navigation.destination = run.partner;
        run.train.navigation.distanceToDestination = 400;
        run.called = true;
        IndustryKit.record(run.test, "loaded call", "at " + run.now() + ": " + gold + " gold into the sink vault, " + copper
                + " copper out of the supply vault; train " + CreateIds.of(run.train.id) + ", station " + CreateIds.of(run.station.id));
        if (gold != LEARNED || copper != LEARNED)
            fail(run, "the loaded call moved " + gold + " gold and " + copper + " copper, not " + LEARNED + " each");
    }

    /** Checks the station's ports, then runs until the factory's replay is eligible and unloads it. */
    static void runLoaded(Run run) {
        if (run.stageTicks % GlassReplayGameTests.POLL != 0)
            return;
        if (!run.portsSeen) {
            long station = CreateIds.of(run.station.id);
            List<ByteBuffer> ports = IndustryKit.received(IndRecords.IND_STATION_PORTS, IndRecords.StationPorts.STATION, station);
            List<ByteBuffer> dep = IndustryKit.received(IndRecords.IND_TRAIN_DEPARTED, IndRecords.Train.TRAIN, CreateIds.of(run.train.id));
            List<ByteBuffer> arr = IndustryKit.received(IndRecords.IND_TRAIN_ARRIVED, IndRecords.Train.TRAIN, CreateIds.of(run.train.id));
            if (ports.isEmpty() || dep.isEmpty() || arr.isEmpty()) {
                run.last = "loaded call records: " + arr.size() + " arrived, " + ports.size() + " station ports, " + dep.size() + " departed";
                if (run.stageTicks > 1200)
                    fail(run, "the loaded call's records never came (" + run.last + ")");
                return;
            }
            if ((arr.get(0).getInt(IndRecords.Train.ARRIVED_FLAGS) & IndRecords.Train.FLAG_SIMULATED) != 0) {
                fail(run, "the loaded call arrived simulated");
                return;
            }
            ByteBuffer p = ports.get(ports.size() - 1);
            TreeSet<Integer> got = new TreeSet<>();
            for (int i = 0; i < IndustryKit.aux(p); i++)
                got.add(p.getInt(IndRecords.StationPorts.INVENTORIES + 4 * i));
            TreeSet<Integer> want = new TreeSet<>(List.of(run.supplyIdx, run.sinkIdx));
            IndustryKit.record(run.test, "station ports", got + " (want " + want + "), district " + p.getLong(IndRecords.StationPorts.DISTRICT));
            if (!got.equals(want) || p.getLong(IndRecords.StationPorts.DISTRICT) != run.fd) {
                fail(run, "the station's ports are " + got + " in district " + p.getLong(IndRecords.StationPorts.DISTRICT) + ", not the two vaults "
                        + want + " in " + run.fd);
                return;
            }
            run.portsSeen = true;
        }
        long since = run.now() - run.builtAt;
        int budget = run.f.warmupTicks() + 2 * (GlassReplayGameTests.WINDOW + GlassExitGameTests.SLACK);
        if (!run.judge) {
            if (since < run.f.warmupTicks() + GlassReplayGameTests.WINDOW + 1000)
                return;
            run.eligibleBy = "not judged: warm-up and one window";
        } else {
            if (run.stub && run.stageTicks % GlassReplayGameTests.STUB_REPORT_POLL != 0 && since < budget)
                return;
            GlassReport fr = GlassExitKit.report(run.fd, run.rec);
            run.last = fr == null ? "far site: no complete compile" : "far site: " + fr.summary() + "; " + fr.replay().summary();
            boolean passed = fr != null && fr.windowsCompleted() > 0 && fr.verdict() == GlassReport.PASS;
            if (fr != null && fr.replay().eligible())
                run.eligibleBy = "the report says eligible" + (passed ? "" : " (last verdict not a pass)");
            else if (passed)
                run.eligibleBy = "a completed passing window; the report does NOT say eligible";
            else if (since < budget)
                return;
            else {
                fail(run, "the factory never became eligible (" + run.last + ")");
                return;
            }
        }
        ServerLevel level = run.level();
        run.t1 = run.now();
        run.chest1 = stock(level, run.input);
        run.supply1 = stock(level, run.supplyVault);
        run.sink1 = stock(level, run.sinkVault);
        run.heldBase = IndustryKit.ind().reconciler().held();
        IndustryKit.record(run.test, "eligible", run.eligibleBy + " at " + run.t1 + ", " + since + " ticks after the build; input chest "
                + run.chest1 + ", supply vault " + run.supply1 + ", sink vault " + run.sink1);
        for (ChunkPos c : run.farChunks)
            level.setChunkForced(c.x, c.z, false);
        run.stage(Stage.UNLOAD);
    }

    static void unload(Run run) {
        ServerLevel level = run.level();
        District d = IndustryKit.ind().district(run.fd);
        for (ChunkPos c : run.farChunks)
            if (level.getChunkSource().getChunkNow(c.x, c.z) != null) {
                run.last = "far chunk " + c + " unloading";
                GlassReplayGameTests.ioWait();
                failSlow(run);
                return;
            }
        if (d == null || IndustryKit.ind().districts().loadedChunks(d) != 0 || d.loadState() != IndRecords.LoadState.UNLOADED) {
            run.last = "far district " + (d == null ? "gone" : "load state " + d.loadState());
            GlassReplayGameTests.ioWait();
            failSlow(run);
            return;
        }
        if (run.unloadedAt < 0)
            run.unloadedAt = run.now();
        if (run.stageTicks % 10 != 0)
            return;
        boolean seen = false;
        for (ByteBuffer r : IndustryKit.received(IndRecords.IND_LOAD_STATE, IndRecords.LoadState.DISTRICT, run.fd))
            seen |= r.getLong(Obs.GAME_TIME) >= run.t1 && IndustryKit.aux(r) == IndRecords.LoadState.UNLOADED;
        if (!seen) {
            run.last = "the engine has not seen the far district unloaded";
            failSlow(run);
            return;
        }
        IndustryKit.record(run.test, "unloaded", "at " + run.unloadedAt + ", the engine saw it by " + run.now());
        run.awayAt = run.now();
        run.stage(Stage.AWAY);
    }

    static void failSlow(Run run) {
        if (run.stageTicks > GlassReplayGameTests.IO_TICKS)
            fail(run, "stage " + run.stage + " never finished (" + run.last + ")");
    }

    /** The train comes back with {@value #LEARNED} gold and arrives simulated; then it leaves and the site loads. */
    static void away(Run run) {
        ServerLevel level = run.level();
        long train = CreateIds.of(run.train.id);
        if (run.arrivedAt < 0) {
            if (run.stageTicks < ARRIVE_AFTER)
                return;
            // What the partner station did: the copper went off, the gold came on.
            IItemHandler car = run.train.carriages.get(0).storage.getAllItems();
            for (int s = 0; s < car.getSlots(); s++)
                car.extractItem(s, car.getStackInSlot(s).getCount(), false);
            ItemHandlerHelper.insertItemStacked(car, new ItemStack(run.cargo, LEARNED), false);
            run.arrivedAt = run.now();
            run.train.arriveAt(run.station);
            return;
        }
        if (!run.arrivalSeen) {
            // The engine sees the journal once the tick's records are sent: wait for the record.
            ByteBuffer a = null;
            for (ByteBuffer r : IndustryKit.received(IndRecords.IND_TRAIN_ARRIVED, IndRecords.Train.TRAIN, train))
                if (r.getLong(Obs.GAME_TIME) >= run.arrivedAt)
                    a = r;
            if (a == null) {
                run.last = "waiting for the second call's IND_TRAIN_ARRIVED";
                if (run.now() - run.arrivedAt > 200)
                    fail(run, "no IND_TRAIN_ARRIVED for the second call");
                return;
            }
            run.arrivalSeen = true;
            int flags = a.getInt(IndRecords.Train.ARRIVED_FLAGS);
            IndustryKit.record(run.test, "simulated call", "at " + run.arrivedAt + ", flags " + flags);
            if ((flags & IndRecords.Train.FLAG_SIMULATED) == 0)
                fail(run, "the second call did not arrive simulated (flags " + flags + ")");
            return;
        }
        if (run.leftAt < 0) {
            boolean answered = !run.stub && !IndustryKit.received(IndRecords.IND_CARRIAGE_RESULT, IndRecords.CarriageResult.TRAIN, train).isEmpty();
            if (!answered && (run.stub || run.now() - run.arrivedAt < RESULT_WAIT)) {
                if (!run.stub || run.now() - run.arrivedAt < 20) {
                    run.last = "waiting for the engine's station exchange";
                    return;
                }
            }
            run.carriageMoved = new TreeMap<>(stock(run.train.carriages.get(0).storage.getAllItems()));
            IndustryKit.record(run.test, "carriage after the call", run.carriageMoved + (run.stub ? " (stub: nothing applies to it)"
                    : answered ? "" : " (NO IND_CARRIAGE_DELTA answered in " + RESULT_WAIT + " ticks)"));
            if (!run.stub && !answered) {
                fail(run, "the engine made no station exchange for the simulated call (" + notes(run) + ")");
                return;
            }
            run.train.leaveStation();
            run.leftAt = run.now();
        }
        if (run.now() < run.unloadedAt + AWAY)
            return;
        run.forcedAt = run.now();
        for (ChunkPos c : run.farChunks)
            level.setChunkForced(c.x, c.z, true);
        run.stage(Stage.RELOAD);
    }

    static void reload(Run run) {
        if (!ticking(run, run.farChunks, "far chunks again"))
            return;
        District d = IndustryKit.ind().district(run.fd);
        if (d == null || d.loadState() != IndRecords.LoadState.LIVE) {
            run.last = "far district " + (d == null ? "gone" : "load state " + d.loadState());
            failSlow(run);
            return;
        }
        run.loadedAt = run.now();
        IndustryKit.record(run.test, "reloaded", "at " + run.loadedAt + ", " + (run.loadedAt - run.forcedAt) + " ticks after the tickets");
        run.stage(Stage.SETTLE);
    }

    static void settle(Run run) {
        ServerLevel level = run.level();
        if (run.stub && run.judge && !run.offlineDone) {
            if (run.now() < run.nextOfflineAt)
                return;
            run.nextOfflineAt = run.now() + 10;
            if (!applyOffline(run)) {
                run.last = "stub: no offline reconcile yet";
                if (run.now() - run.loadedAt < GlassReplayGameTests.OFFLINE_WAIT)
                    return;
                IndustryKit.record(run.test, "offline reconcile", "none for the far district");
            }
            run.offlineDone = true;
        }
        boolean busy = IndustryKit.ind().reconciler().held() > run.heldBase;
        for (ChunkPos c : run.farChunks) {
            LevelChunk ch = level.getChunkSource().getChunkNow(c.x, c.z);
            busy |= ch == null || TickGate.holds(ch);
        }
        if (busy)
            run.quietSince = -1;
        else if (run.quietSince < 0)
            run.quietSince = run.now();
        int settle = GlassReplayGameTests.SETTLE;
        if (run.now() - run.loadedAt < settle || run.quietSince < 0 || run.now() - run.quietSince < settle / 2) {
            run.last = "settling: held " + IndustryKit.ind().reconciler().held() + (busy ? ", busy" : ", quiet since " + run.quietSince);
            if (run.stageTicks > 4 * settle + 2000)
                fail(run, "the far site never settled (" + run.last + ")");
            return;
        }
        if (run.verdict != null) {
            run.verdict.accept(run);
            if (run.stage == Stage.SETTLE)
                run.stage(Stage.DONE);
            return;
        }
        judge(run);
        run.stage(Stage.DONE);
    }

    /** The stub with natives: the offline engine's reconcile of the far district, applied as the reconciler would. */
    static boolean applyOffline(Run run) {
        run.rec.poll();
        long[] t = NativeEngine.glassReplayOffline(run.rec.stream(), run.fd);
        if (t.length < 3)
            return false;
        ServerLevel level = run.level();
        District d = IndustryKit.ind().district(run.fd);
        List<String> lines = new ArrayList<>();
        for (int pass = 0; pass < 2; pass++)
            for (int i = 0; i + 2 < t.length; i += 3) {
                long delta = t[i + 2];
                if ((pass == 0) != (delta < 0))
                    continue;
                int inventory = (int) t[i];
                District.Inventory inv = d != null && inventory >= 0 && inventory < d.inventories.size() ? d.inventories.get(inventory) : null;
                Goods.Name name = Goods.parse(IndustryKit.ind().goodName((int) t[i + 1]));
                long got = inv == null || name == null ? 0 : GlassReplayGameTests.access(level, inv).apply(name, delta);
                lines.add("inv " + inventory + " " + GlassReplayGameTests.key((int) t[i + 1]) + " " + delta + " (applied " + got + ")");
            }
        IndustryKit.record(run.test, "offline reconcile applied", lines);
        return true;
    }

    static void judge(Run run) {
        ServerLevel level = run.level();
        Map<String, Long> chest2 = stock(level, run.input), supply2 = stock(level, run.supplyVault), sink2 = stock(level, run.sinkVault);
        String cu = id(Items.COPPER_INGOT), au = id(Items.GOLD_INGOT);
        long chestCopper = get(chest2, cu) - get(run.chest1, cu), chestGold = get(chest2, au) - get(run.chest1, au);
        long took = get(run.supply1, cu) - get(supply2, cu), gave = get(sink2, au) - get(run.sink1, au);
        IndustryKit.record(run.test, "input chest", run.chest1 + " -> " + chest2 + ": copper " + chestCopper + ", gold " + chestGold);
        IndustryKit.record(run.test, "supply vault", run.supply1 + " -> " + supply2 + ": copper taken " + took);
        IndustryKit.record(run.test, "sink vault", run.sink1 + " -> " + sink2 + ": gold given " + gave);
        IndustryKit.record(run.test, "notes", notes(run));
        if (!run.judge) {
            IndustryKit.record(run.test, "NOT judged", "no natives: nothing reconciled the far site");
            return;
        }
        if (chestCopper != 0 || chestGold != 0)
            run.failures.add("the factory's input chest changed by " + chestCopper + " copper and " + chestGold + " gold");
        if (took <= 0 || took > LEARNED)
            run.failures.add("the supply vault lost " + took + " copper, not 1 to " + LEARNED);
        if (gave <= 0 || gave > LEARNED)
            run.failures.add("the sink vault gained " + gave + " gold, not 1 to " + LEARNED);
        if (!run.stub && run.carriageMoved != null && get(run.carriageMoved, cu) != took)
            run.failures.add("the carriage took " + get(run.carriageMoved, cu) + " copper, the supply vault lost " + took);
        IndustryKit.record(run.test, run.failures.isEmpty() ? "PASS" : "FAIL", run.failures.isEmpty() ? "the station booked on its ports only" : run.failures);
    }

    // ---- helpers ------------------------------------------------------------------------------

    static String notes(Run run) {
        List<String> n = GlassExitKit.notes(run.fd, run.t1 < 0 ? 0 : run.t1, run.rec);
        List<String> station = new ArrayList<>();
        for (String s : n)
            if (s.contains("station exchange") || s.contains("replay"))
                station.add(s);
        return station.size() > 12 ? station.subList(station.size() - 12, station.size()).toString() : station.toString();
    }

    static Train syntheticTrain(ServerLevel level, BlockPos vaultPos) {
        level.setBlock(vaultPos, block("create:item_vault").defaultBlockState(), 3);
        AbstractBogeyBlock<?> bogeyBlock = (AbstractBogeyBlock<?>) block("create:small_bogey");
        Carriage carriage = new Carriage(new CarriageBogey(bogeyBlock, false, new CompoundTag()),
                new CarriageBogey(bogeyBlock, false, new CompoundTag()), 2);
        carriage.storage.addBlock(level, level.getBlockState(vaultPos), BlockPos.ZERO, vaultPos, level.getBlockEntity(vaultPos));
        carriage.storage.initialize();
        level.setBlock(vaultPos, Blocks.AIR.defaultBlockState(), 3);
        return new Train(UUID.randomUUID(), null, null, new ArrayList<>(List.of(carriage)), new ArrayList<>(), false, 0);
    }

    static GlobalStation syntheticStation(ServerLevel level, BlockPos pos, String name) {
        GlobalStation st = new GlobalStation();
        st.id = UUID.randomUUID();
        st.name = name;
        st.blockEntityPos = pos;
        st.blockEntityDimension = level.dimension();
        return st;
    }

    static IItemHandler items(ServerLevel level, BlockPos pos) {
        IItemHandler h = level.getCapability(Capabilities.ItemHandler.BLOCK, pos, null);
        if (h == null)
            throw new GameTestAssertException("no item handler at " + pos.toShortString());
        return h;
    }

    /** Moves up to {@code n} of {@code item} from one handler to the other; returns how many moved. */
    static int move(IItemHandler from, IItemHandler to, Item item, int n) {
        int moved = 0;
        for (int s = 0; s < from.getSlots() && moved < n; s++) {
            if (!from.getStackInSlot(s).is(item))
                continue;
            ItemStack got = from.extractItem(s, n - moved, false);
            ItemStack rest = ItemHandlerHelper.insertItemStacked(to, got, false);
            moved += got.getCount() - rest.getCount();
            if (!rest.isEmpty()) {
                from.insertItem(s, rest, false);
                break;
            }
        }
        return moved;
    }

    static Map<String, Long> stock(ServerLevel level, BlockPos pos) {
        return stock(items(level, pos));
    }

    static Map<String, Long> stock(IItemHandler h) {
        Map<String, Long> out = new TreeMap<>();
        for (int s = 0; s < h.getSlots(); s++) {
            ItemStack st = h.getStackInSlot(s);
            if (!st.isEmpty())
                out.merge(id(st.getItem()), (long) st.getCount(), Long::sum);
        }
        return out;
    }

    static String id(Item item) {
        return GlassCorpusKit.id(item);
    }

    static long get(Map<String, Long> m, String k) {
        return m == null ? 0 : m.getOrDefault(k, 0L);
    }
}
