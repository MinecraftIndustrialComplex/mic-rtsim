package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Districts;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassReport;
import com.minecraftindustrialcomplex.mic_rtsim_create.CreateIds;
import com.minecraftindustrialcomplex.mic_rtsim_create.TrainWatch;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit;
import com.simibubi.create.content.trains.entity.Train;
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
 * <p>Once both units have learned, a synthetic train makes a loaded call while the press line's unit is a
 * fifth to three fifths into a window: it brings {@value StationCargoGameTests#LEARNED} iron ingots, the test
 * moves them into the vault (the interface's and the chute's work), and the station's ports, journaled at the
 * train's departure, must be that vault alone. The engine books the cargo on the port's inventory, which lies
 * in the press line's unit, so that window must go on (its ticks never fall back), complete and pass, with the
 * unit's goods bounded: the compile claims the interface with no node (choice SC95), and a black box there
 * would leave the unit unbounded, so that unbooked ingots could not fail it. The far site then unloads, the
 * train comes back with iron and arrives simulated, and the site loads again: no unit is discarded for the
 * cargo ({@code GlassReport.DISCARDS[8]}, "station cargo"), and the exchange takes iron out of the carriage.
 *
 * <p>The negative control, after the verdict: a chest placed beside the interface's chute is a district
 * inventory in no unit, so the station's ports name the vault and it; another loaded call then names a unit
 * and none, which must cost the press line's unit its window (a station cargo discard).
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
    /** Where in the press line's open window the loaded call comes: between these ticks into it. */
    static final int CALL_FROM = GlassReplayGameTests.WINDOW / 5, CALL_TO = 3 * GlassReplayGameTests.WINDOW / 5;
    /** The negative control's chest: beside the interface's chute, which only moves up and down. */
    static final BlockPos CONTROL_CHEST = StationPressLineFactory.PORT_CHUTE.east();
    /** Ticks the negative control gets for its recompile, and then for its discard. */
    static final int CONTROL_WAIT = 2400;

    @GameTestGenerator
    public static Collection<TestFunction> stationPressLine() {
        if (!SectionsFxGameTests.selected(NAME))
            return List.of();
        CorpusFactory f = new StationPressLineFactory();
        int maxTicks = f.warmupTicks() + 3 * (GlassReplayGameTests.WINDOW + GlassExitGameTests.SLACK) + StationCargoGameTests.AWAY
                + StationCargoGameTests.RESULT_WAIT + 3 * GlassReplayGameTests.IO_TICKS + 4 * GlassReplayGameTests.SETTLE + 2 * CONTROL_WAIT + 4000;
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
            case CONTROL -> control(run);
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
        int joined = GlassCorpus.join(h); // the line, its port and the bystander: one district wherever the chunk borders fall
        run.builtAt = run.now();
        GlassCorpusKit.log(run.f.name(), "station press line built", run.copy.notes() + "; port at " + run.psi.toShortString()
                + "; chunk borders joined: " + joined);
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
        run.stage(Stage.RUN);
    }

    /** The loaded call, in one tick: arrive, the interface's work, leave. */
    static void loadedCall(Run run) {
        ServerLevel level = run.level();
        run.station = StationCargoGameTests.syntheticStation(level, run.stationPos, "Press Halt");
        run.partner = StationCargoGameTests.syntheticStation(level, run.stationPos.offset(400, 0, 0), "Partner Halt");
        run.train = StationCargoGameTests.syntheticTrain(level, run.helper.absolutePos(new BlockPos(5, 1, 5)));
        call(run, run.train, "loaded call");
        run.called = true;
        run.mine.put("calledAt", run.now());
    }

    /** One loaded call of {@code train} at the station: {@value StationCargoGameTests#LEARNED} iron into the vault. */
    static void call(Run run, Train train, String what) {
        ServerLevel level = run.level();
        IItemHandler car = train.carriages.get(0).storage.getAllItems();
        ItemHandlerHelper.insertItemStacked(car, new ItemStack(run.cargo, LEARNED), false);
        train.arriveAt(run.station);
        TrainWatch.dockedForTest(train, run.station, level.dimension(), run.psi);
        int moved = StationCargoGameTests.move(car, StationCargoGameTests.items(level, run.sinkVault), run.cargo, LEARNED);
        train.leaveStation();
        train.navigation.destination = run.partner;
        train.navigation.distanceToDestination = 400;
        IndustryKit.record(run.test, what, "at " + run.now() + ": " + moved + " iron into the vault; train " + CreateIds.of(train.id)
                + ", station " + CreateIds.of(run.station.id));
        if (moved != LEARNED)
            StationCargoGameTests.fail(run, "the " + what + " moved " + moved + " iron, not " + LEARNED);
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

    /**
     * Runs until both units have learned and the press line's unit is in the middle of a window, makes the loaded
     * call, checks the station's ports, then runs until that window has passed and unloads the site.
     */
    static void runLoaded(Run run) {
        if (run.stageTicks % GlassReplayGameTests.POLL != 0)
            return;
        long since = run.now() - run.builtAt;
        int budget = run.f.warmupTicks() + 2 * (GlassReplayGameTests.WINDOW + GlassExitGameTests.SLACK) + 1200;
        if (!run.called) {
            if (!run.judge) {
                loadedCall(run);
                return;
            }
            if (run.stub && run.stageTicks % GlassReplayGameTests.STUB_REPORT_POLL != 0 && since < budget)
                return;
            GlassReport fr = GlassExitKit_report(run);
            run.last = fr == null ? "far site: no complete compile" : "far site: " + fr.summary() + "; " + fr.replay().summary();
            String gate = ready(run, fr);
            GlassReport.Unit line = gate == null ? SectionsFx.unitAt(fr, run.far.absolutePos(StationPressLineFactory.PRESS)) : null;
            if (gate == null && line == null)
                gate = "no one unit holds the press: " + SectionsFx.describe(fr);
            else if (gate == null && (line.windowTicks() < CALL_FROM || line.windowTicks() > CALL_TO))
                gate = "the press line's unit is " + line.windowTicks() + " ticks into its window, the call comes at " + CALL_FROM + " to " + CALL_TO;
            if (gate != null) {
                run.last = "before the call: " + gate;
                if (since < budget + GlassReplayGameTests.WINDOW)
                    return;
                StationCargoGameTests.fail(run, "the units never learned, or no window of the press line's came to the call (" + gate + ")");
                return;
            }
            run.mine.put("w0", line.windowsCompleted());
            run.mine.put("ticks", line.windowTicks());
            run.mine.put("discards0", fr.checksum().discards(STATION_CARGO));
            info(run, "the press line's unit at the call", line.summary());
            loadedCall(run);
            return;
        }
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
        if (!run.judge) {
            if (since < run.f.warmupTicks() + GlassReplayGameTests.WINDOW + 1000)
                return;
            run.eligibleBy = "not judged: warm-up and one window";
        } else {
            long calledAt = (Long) run.mine.get("calledAt");
            long after = run.now() - calledAt;
            int callBudget = GlassReplayGameTests.WINDOW + GlassExitGameTests.SLACK;
            if (run.stub && run.stageTicks % GlassReplayGameTests.STUB_REPORT_POLL != 0 && after < callBudget)
                return;
            GlassReport fr = GlassExitKit_report(run);
            run.last = fr == null ? "far site: no complete compile" : "far site: " + fr.summary() + "; " + fr.replay().summary();
            GlassReport.Unit line = fr == null ? null : SectionsFx.unitAt(fr, run.far.absolutePos(StationPressLineFactory.PRESS));
            int w0 = (Integer) run.mine.get("w0");
            long ticks = (Long) run.mine.get("ticks");
            if (line == null) {
                if (after < callBudget)
                    return;
                StationCargoGameTests.fail(run, "no one unit holds the press after the call (" + run.last + ")");
                return;
            }
            if (line.windowsCompleted() == w0) {
                // the window across the call goes on: its ticks never fall back (a discard would start one at 0,
                // less than the fifth of a window it had at the call, by the next read)
                if (!check(run, "the press line's window across the call goes on (not discarded)", line.windowTicks() >= ticks,
                        line.windowTicks() + " ticks into it, " + ticks + " at the last read; " + line.summary())) {
                    run.stage(Stage.DONE);
                    return;
                }
                run.mine.put("ticks", line.windowTicks());
                run.last = "the press line's window across the call: " + line.summary();
                if (after < callBudget)
                    return;
                StationCargoGameTests.fail(run, "the press line's window across the call never completed (" + line.summary() + ")");
                return;
            }
            String d = SectionsFx.describe(fr);
            info(run, "the press line's unit after its window across the call", line.summary() + ", " + after + " ticks after the call");
            check(run, "the press line's window across the call completes and passes (the cargo booked on the vault)",
                    line.windowsCompleted() == w0 + 1 && line.verdict() == GlassReport.PASS && line.passed(), line.summary());
            check(run, "the press line's unit is bounded: no black box at the interface (SC95)", !line.unbounded(), d);
            long discards = fr.checksum().discards(STATION_CARGO);
            check(run, "no unit was discarded for the loaded call's cargo", discards == (Long) run.mine.get("discards0"),
                    discards + " " + GlassReport.DISCARDS[STATION_CARGO] + " discards, " + run.mine.get("discards0") + " before the call");
            run.eligibleBy = "both units learned, and the press line's window across the call passed";
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
        // the negative control: a chest beside the interface's chute, a district inventory no unit holds
        run.mine.put("controlDiscards0", discards);
        run.mine.put("controlAt", run.now());
        BlockPos chest = run.far.absolutePos(CONTROL_CHEST);
        GlassCorpusKit.chest(run.far, chest);
        IndustryKit.playerPlaced(run.far, chest);
        info(run, "negative control", "a chest at " + chest.toShortString() + " beside the interface's chute");
        run.stage(Stage.CONTROL);
    }

    /**
     * The negative control: once the district has recompiled with the chest among its inventories, a loaded call
     * whose ports name the vault (the press line's unit) and the chest (no unit's) must take the STATION discard
     * path (its counter goes up). The chest's revision makes the units new, so they may still be warming up and
     * have no window to drop: the counter, not a dropped window, is what this proves.
     */
    static void control(Run run) {
        if (run.stageTicks % 20 != 0)
            return;
        ServerLevel level = run.level();
        District d = IndustryKit.ind().district(run.fd);
        if (!run.mine.containsKey("controlTrain")) {
            GlassReport fr = GlassExitKit_report(run);
            District.Inventory chest = d == null ? null : d.inventoryAt(Districts.canonicalInventory(level, run.far.absolutePos(CONTROL_CHEST)));
            boolean compiled = d != null && chest != null && fr != null && fr.checksum().revision() == d.revision && fr.units().size() >= 2;
            if (!compiled) {
                run.last = "control: waiting for the recompile with the chest (" + (fr == null ? "no report" : fr.checksum().summary())
                        + ", district revision " + (d == null ? -1 : d.revision) + ", chest " + chest + ")";
                if (run.stageTicks > CONTROL_WAIT)
                    controlFail(run, "the district never recompiled with the control's chest (" + run.last + ")");
                return;
            }
            run.mine.put("controlChest", chest.index());
            // the vault's index at this revision (inventories sort by position, so it is the same one here)
            District.Inventory vault = d.inventoryAt(Districts.canonicalInventory(level, run.sinkVault));
            run.mine.put("controlVault", vault == null ? -1 : vault.index());
            info(run, "control: recompiled", SectionsFx.describe(fr) + "; chest inventory " + chest.index());
            Train t = StationCargoGameTests.syntheticTrain(level, run.helper.absolutePos(new BlockPos(5, 1, 5)));
            run.mine.put("controlTrain", t);
            run.mine.put("controlCalledAt", run.now());
            call(run, t, "control call");
            return;
        }
        long calledAt = (Long) run.mine.get("controlCalledAt");
        long station = CreateIds.of(run.station.id);
        List<ByteBuffer> ports = IndustryKit.received(IndRecords.IND_STATION_PORTS, IndRecords.StationPorts.STATION, station);
        TreeSet<Integer> got = new TreeSet<>();
        if (!ports.isEmpty()) {
            ByteBuffer p = ports.get(ports.size() - 1);
            for (int i = 0; i < IndustryKit.aux(p); i++)
                got.add(p.getInt(IndRecords.StationPorts.INVENTORIES + 4 * i));
        }
        Set<Integer> want = Set.of((Integer) run.mine.get("controlVault"), (Integer) run.mine.get("controlChest"));
        GlassReport fr = GlassExitKit_report(run);
        long discards = fr == null ? -1 : fr.checksum().discards(STATION_CARGO);
        long before = (Long) run.mine.get("controlDiscards0");
        if (!got.equals(want) || discards <= before) {
            run.last = "control: ports " + got + " (want " + want + "), " + discards + " station cargo discards (" + before + " before)";
            if (run.now() - calledAt > CONTROL_WAIT)
                controlFail(run, "the control call's ports never named the vault and the chest, or its cargo discarded nothing (" + run.last + ")");
            return;
        }
        info(run, "control: ports", got + ", " + discards + " station cargo discards (" + before + " before)");
        check(run, "the negative control: cargo with ports naming a unit's inventory and one of no unit takes the STATION discard", true, run.last);
        IndustryKit.record(run.test, run.failures.isEmpty() ? "PASS" : "FAIL", run.failures.isEmpty()
                ? "the cargo booked on the press line's section, and the control discarded" : run.failures);
        run.stage(Stage.DONE);
    }

    private static void controlFail(Run run, String why) {
        check(run, "the negative control", false, why);
        run.stage(Stage.DONE);
    }
}
