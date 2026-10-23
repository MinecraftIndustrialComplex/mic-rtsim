package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Districts;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassReport;
import com.minecraftindustrialcomplex.mic_rtsim_create.CreateIds;
import com.minecraftindustrialcomplex.mic_rtsim_create.TrainWatch;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestGenerator;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestInfo;
import net.minecraft.gametest.framework.GameTestListener;
import net.minecraft.gametest.framework.GameTestRunner;
import net.minecraft.gametest.framework.TestFunction;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Rotation;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;
import net.neoforged.neoforge.items.IItemHandler;
import net.neoforged.neoforge.items.ItemHandlerHelper;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Set;
import java.util.TreeSet;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.at;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.fill;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.StationCargoGameTests.LEARNED;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.StationCargoGameTests.Run;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.StationCargoGameTests.Stage;

/**
 * SEC-1 fixture (SECTIONS.md §9.2): a station's cargo reaches the press line's section through
 * {@code IND_STATION_PORTS}. {@link StationPressLineFactory} is a press line fed from a vault with a station
 * port on top of it, and a bystander line; this runs {@link StationCargoGameTests}' calls against it (its
 * stage machine and its helpers; only the port, the readiness and the verdict are this fixture's).
 *
 * <p>A synthetic train makes a loaded call: it brings {@value StationCargoGameTests#LEARNED} iron ingots, the
 * test moves them into the vault (the interface's and the chute's work), the station's ports are journaled at
 * the train's departure and must be that vault alone. The far site unloads once both units have learned, the
 * train comes back with iron and arrives simulated, and the site loads again. The engine books the cargo on
 * the port's inventory, which lies in the press line's unit: no unit is discarded for it
 * ({@code GlassReport.DISCARDS[8]}, "station cargo"), and the exchange takes iron out of the carriage.
 *
 * <p>{@code MIC_SEC1FX_ONLY=station_press_line} selects it among the fixtures.
 */
@GameTestHolder(SectionsFxGameTests.NAMESPACE)
@PrefixGameTestTemplate(false)
public final class StationPressLineGameTests {
    private StationPressLineGameTests() {}

    static final String NAME = "station_press_line";
    /** The far site, in whole chunks along x: clear of the station cargo test's (6,400) and the replay suites'. */
    static final int FAR_CHUNKS = StationCargoGameTests.FAR_CHUNKS + 32;
    /** A discard reason of {@code GlassReport.DISCARDS}. */
    static final int STATION_CARGO = 8;

    @GameTestGenerator
    public static Collection<TestFunction> stationPressLine() {
        if (!SectionsFxGameTests.selected(NAME))
            return List.of();
        CorpusFactory f = new StationPressLineFactory();
        int maxTicks = f.warmupTicks() + 2 * (GlassReplayGameTests.WINDOW + GlassExitGameTests.SLACK) + StationCargoGameTests.AWAY
                + StationCargoGameTests.RESULT_WAIT + 3 * GlassReplayGameTests.IO_TICKS + 4 * GlassReplayGameTests.SETTLE + 4000;
        return List.of(new TestFunction("sec1fx_" + NAME, "sec1fx." + NAME, SectionsFxGameTests.NAMESPACE + ":" + IndustryKit.YARD,
                Rotation.NONE, maxTicks, 0L, true, false, 1, 1, true, h -> start(h, f)));
    }

    private static String id(Run run) {
        return run.f.name();
    }

    static boolean check(Run run, String what, boolean ok, Object detail) {
        return SectionsFx.check(id(run), run.f.name(), run.test, run.failures, what, ok, detail);
    }

    static void info(Run run, String what, Object detail) {
        SectionsFx.info(id(run), run.test, what, detail);
    }

    static void start(GameTestHelper helper, CorpusFactory f) {
        com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassConfig.Test.set(
                com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassConfig.CHECKSUM_WINDOW_TICKS, GlassReplayGameTests.WINDOW);
        IndustryKit.fastConfig();
        Run run = new Run(helper, f, "sec1fx " + NAME, FAR_CHUNKS);
        run.cargo = Items.IRON_INGOT;
        run.verdict = StationPressLineGameTests::verdict;
        IndustryKit.record(run.test, "far site", "offset " + run.far.offset.toShortString() + ", chunks " + run.farChunks + ", engine "
                + IndustryKit.engine().describe() + (run.judge ? "" : ", NOT judged (no natives)"));
        for (ChunkPos c : run.farChunks)
            run.level().setChunkForced(c.x, c.z, true);
        helper.testInfo.addListener(new GameTestListener() {
            @Override
            public void testStructureLoaded(GameTestInfo info) {}

            @Override
            public void testPassed(GameTestInfo info, GameTestRunner runner) {
                StationCargoGameTests.release(run);
            }

            @Override
            public void testFailed(GameTestInfo info, GameTestRunner runner) {
                StationCargoGameTests.release(run);
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
            case UNLOAD -> StationCargoGameTests.unload(run);
            case AWAY -> StationCargoGameTests.away(run);
            case RELOAD -> StationCargoGameTests.reload(run);
            case SETTLE -> StationCargoGameTests.settle(run);
            default -> {}
        }
    }

    /** Builds the line, its bystander and the port once the far chunks and the yard's would tick. */
    static void load(Run run) {
        List<ChunkPos> cs = new ArrayList<>(run.farChunks);
        cs.addAll(GlassReplayGameTests.yardChunks(run.helper));
        if (!StationCargoGameTests.ticking(run, cs, "far chunks"))
            return;
        ServerLevel level = run.level();
        IndustryKit.forget(run.helper);
        ChunkPos lo = run.farChunks.get(0), hi = run.farChunks.get(run.farChunks.size() - 1);
        IndustryKit.ind().districts().forgetArea(level, lo.x - 1, lo.z - 1, hi.x + 1, hi.z + 1);
        GlassReplayGameTests.clear(level, run.farMin, run.farMax, run.farMin);
        GlassReplayGameTests.Far h = run.far;
        run.copy = run.f.build(h);
        GlassReplayGameTests.seed(h, run.copy);
        run.psi = StationPressLineFactory.abs(h, StationPressLineFactory.PSI);
        run.sinkVault = StationPressLineFactory.abs(h, StationPressLineFactory.VAULT);
        run.input = run.sinkVault;
        run.stationPos = StationPressLineFactory.abs(h, StationPressLineFactory.STATION);
        for (BlockPos p : List.of(run.sinkVault, StationPressLineFactory.abs(h, StationPressLineFactory.PORT_CHUTE), run.psi,
                StationPressLineFactory.abs(h, StationPressLineFactory.DRAW)))
            IndustryKit.playerPlaced(h, p);
        run.builtAt = run.now();
        GlassCorpusKit.log(run.f.name(), "station press line built", run.copy.notes() + "; port at " + run.psi.toShortString());
        run.stage(Stage.DISTRICT);
    }

    /** Fills the vault once it has formed, waits for one district holding the line, the vault, the bystander and the station. */
    static void district(Run run) {
        ServerLevel level = run.level();
        if (!run.filled) {
            if (run.stageTicks < 10)
                return;
            fill(run.far, run.sinkVault, Items.IRON_INGOT, StationPressLineFactory.VAULT_INGOTS);
            run.filled = true;
        }
        if (run.stageTicks % 20 != 0)
            return;
        BlockPos bystander = run.far.absolutePos(SectionsFx.BYSTANDER_SAW);
        District d = IndustryKit.districtAt(run.helper, run.copy.anchor());
        District atStation = IndustryKit.districtAt(run.helper, run.stationPos);
        District atBystander = IndustryKit.districtAt(run.helper, bystander);
        District.Inventory vault = d == null ? null : d.inventoryAt(Districts.canonicalInventory(level, run.sinkVault));
        if (d == null || atStation != d || atBystander != d || vault == null) {
            run.last = "district: line " + d + ", station " + atStation + ", bystander " + atBystander + ", vault " + vault;
            run.stableSince = -1;
            if (run.stageTicks > GlassReplayGameTests.IO_TICKS)
                StationCargoGameTests.fail(run, "no one district holds the line, the port, the station and the bystander (" + run.last + ")");
            return;
        }
        if (run.stableSince < 0 || d.revision != run.revision) {
            run.revision = d.revision;
            run.stableSince = run.now();
            return;
        }
        if (run.now() - run.stableSince < StationCargoGameTests.STABLE)
            return;
        run.fd = d.id;
        run.sinkIdx = vault.index();
        IndustryKit.record(run.test, "district", d + " (" + d.chunks.size() + " chunks, " + d.inventories.size() + " inventories, revision "
                + d.revision + "), vault inventory " + run.sinkIdx);
        loadedCall(run);
        run.stage(Stage.RUN);
    }

    /** The loaded call, in one tick: arrive, the interface's work, leave. */
    static void loadedCall(Run run) {
        ServerLevel level = run.level();
        run.station = StationCargoGameTests.syntheticStation(level, run.stationPos, "Press Halt");
        run.partner = StationCargoGameTests.syntheticStation(level, run.stationPos.offset(400, 0, 0), "Partner Halt");
        run.train = StationCargoGameTests.syntheticTrain(level, run.helper.absolutePos(new BlockPos(5, 1, 5)));
        IItemHandler car = run.train.carriages.get(0).storage.getAllItems();
        ItemHandlerHelper.insertItemStacked(car, new ItemStack(run.cargo, LEARNED), false);
        run.train.arriveAt(run.station);
        TrainWatch.dockedForTest(run.train, run.station, level.dimension(), run.psi);
        int moved = StationCargoGameTests.move(car, StationCargoGameTests.items(level, run.sinkVault), run.cargo, LEARNED);
        run.train.leaveStation();
        run.train.navigation.destination = run.partner;
        run.train.navigation.distanceToDestination = 400;
        run.called = true;
        IndustryKit.record(run.test, "loaded call", "at " + run.now() + ": " + moved + " iron into the vault; train " + CreateIds.of(run.train.id)
                + ", station " + CreateIds.of(run.station.id));
        if (moved != LEARNED)
            StationCargoGameTests.fail(run, "the loaded call moved " + moved + " iron, not " + LEARNED);
    }

    /** What the units must show before the far site leaves; the structure's checks run once. */
    static String ready(Run run, GlassReport fr) {
        if (fr == null)
            return "no complete compile";
        if (fr.units().size() < 2)
            return fr.units().size() + " units of 2";
        for (GlassReport.Unit u : fr.units())
            if (!SectionsFx.learned(u))
                return "a unit has not learned: " + SectionsFx.describe(fr);
        if (run.mine.put("structure", Boolean.TRUE) == null) {
            BlockPos vault = run.far.absolutePos(StationPressLineFactory.VAULT), press = run.far.absolutePos(StationPressLineFactory.PRESS);
            BlockPos belt = run.far.absolutePos(StationPressLineFactory.BELT_START);
            GlassReport.Unit line = SectionsFx.unitAt(fr, press), vaultUnit = SectionsFx.unitAt(fr, vault);
            GlassReport.Unit bystander = SectionsFx.unitAt(fr, run.far.absolutePos(SectionsFx.BYSTANDER_FEED));
            String d = SectionsFx.describe(fr);
            info(run, "far units", d);
            check(run, "the press line's unit (vault and belt in it) and the bystander's, apart", fr.units().size() >= 2 && line != null
                    && line == vaultUnit && line == SectionsFx.unitAt(fr, belt) && bystander != null && bystander != line, d);
            check(run, "both units learned before the far site leaves", SectionsFx.learned(line) && SectionsFx.learned(bystander), d);
        }
        return null;
    }

    /** Checks the station's ports, then runs until both units have learned and unloads the site. */
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
                    StationCargoGameTests.fail(run, "the loaded call's records never came (" + run.last + ")");
                return;
            }
            if ((arr.get(0).getInt(IndRecords.Train.ARRIVED_FLAGS) & IndRecords.Train.FLAG_SIMULATED) != 0) {
                StationCargoGameTests.fail(run, "the loaded call arrived simulated");
                return;
            }
            ByteBuffer p = ports.get(ports.size() - 1);
            TreeSet<Integer> got = new TreeSet<>();
            for (int i = 0; i < IndustryKit.aux(p); i++)
                got.add(p.getInt(IndRecords.StationPorts.INVENTORIES + 4 * i));
            Set<Integer> want = Set.of(run.sinkIdx);
            info(run, "station ports", got + " (want " + want + "), district " + p.getLong(IndRecords.StationPorts.DISTRICT));
            if (!check(run, "the station's ports are the press line's vault alone, in the far district",
                    got.equals(want) && p.getLong(IndRecords.StationPorts.DISTRICT) == run.fd, got + " in district " + p.getLong(IndRecords.StationPorts.DISTRICT)
                            + ", want " + want + " in " + run.fd)) {
                run.stage(Stage.DONE);
                return;
            }
            run.portsSeen = true;
        }
        long since = run.now() - run.builtAt;
        int budget = run.f.warmupTicks() + 2 * (GlassReplayGameTests.WINDOW + GlassExitGameTests.SLACK) + 1200;
        if (!run.judge) {
            if (since < run.f.warmupTicks() + GlassReplayGameTests.WINDOW + 1000)
                return;
            run.eligibleBy = "not judged: warm-up and one window";
        } else {
            if (run.stub && run.stageTicks % GlassReplayGameTests.STUB_REPORT_POLL != 0 && since < budget)
                return;
            GlassReport fr = GlassExitKit_report(run);
            run.last = fr == null ? "far site: no complete compile" : "far site: " + fr.summary() + "; " + fr.replay().summary();
            String gate = ready(run, fr);
            if (gate != null) {
                run.last = "scenario: " + gate;
                if (since < budget)
                    return;
                StationCargoGameTests.fail(run, "the units never learned (" + gate + ")");
                return;
            }
            run.eligibleBy = "both units learned";
        }
        ServerLevel level = run.level();
        run.t1 = run.now();
        run.heldBase = IndustryKit.ind().reconciler().held();
        IndustryKit.record(run.test, "eligible", run.eligibleBy + " at " + run.t1 + ", " + since + " ticks after the build");
        for (ChunkPos c : run.farChunks)
            level.setChunkForced(c.x, c.z, false);
        run.stage(Stage.UNLOAD);
    }

    private static GlassReport GlassExitKit_report(Run run) {
        return com.minecraftindustrialcomplex.mic_rtsim_create.gametest.GlassExitKit.report(run.fd, run.rec);
    }

    /** After the settle: the exchange took iron out of the carriage, and no unit was discarded for the cargo. */
    static void verdict(Run run) {
        info(run, "notes", StationCargoGameTests.notes(run));
        if (!run.judge) {
            info(run, "NOT judged", "no natives: nothing reconciled the far site");
            return;
        }
        GlassReport far = GlassExitKit_report(run);
        String d = SectionsFx.describe(far);
        info(run, "units after the reconcile", d);
        BlockPos press = run.far.absolutePos(StationPressLineFactory.PRESS), vault = run.far.absolutePos(StationPressLineFactory.VAULT);
        GlassReport.Unit line = SectionsFx.unitAt(far, press);
        check(run, "the vault is still in the press line's unit after the calls", line != null && line == SectionsFx.unitAt(far, vault), d);
        long discards = far == null ? -1 : far.checksum().discards(STATION_CARGO);
        check(run, "no unit was discarded for the station's cargo (it booked on the port's inventory)", discards == 0,
                discards + " " + GlassReport.DISCARDS[STATION_CARGO] + " discards; " + (far == null ? "no report" : far.checksum().summary()));
        if (!run.stub) {
            String iron = StationCargoGameTests.id(Items.IRON_INGOT);
            long left = StationCargoGameTests.get(run.carriageMoved, iron);
            check(run, "the exchange unloaded iron from the carriage (at least one, at most " + LEARNED + ")", left >= 0 && left < LEARNED,
                    "the carriage held " + left + " iron after the engine's answer, of " + LEARNED);
        }
        IndustryKit.record(run.test, run.failures.isEmpty() ? "PASS" : "FAIL", run.failures.isEmpty() ? "the cargo booked on the press line's section" : run.failures);
    }
}
