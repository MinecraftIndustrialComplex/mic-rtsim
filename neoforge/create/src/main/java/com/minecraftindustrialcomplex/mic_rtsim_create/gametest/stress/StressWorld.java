package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.stress;

import com.google.gson.GsonBuilder;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.BrassMixingFactory;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.CorpusFactory;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.MillstoneFlourFactory;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.PressSheetsFactory;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.SawShaftsFactory;
import com.mojang.logging.LogUtils;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.Create;
import com.simibubi.create.content.contraptions.actors.psi.PortableStorageInterfaceBlock;
import com.simibubi.create.content.contraptions.behaviour.MovementContext;
import com.simibubi.create.content.contraptions.glue.SuperGlueEntity;
import com.simibubi.create.content.logistics.vault.ItemVaultBlock;
import com.simibubi.create.content.processing.burner.BlazeBurnerBlock;
import com.simibubi.create.content.trains.entity.CarriageContraptionEntity;
import com.simibubi.create.content.trains.entity.Train;
import com.simibubi.create.content.trains.schedule.Schedule;
import com.simibubi.create.content.trains.schedule.ScheduleEntry;
import com.simibubi.create.content.trains.schedule.condition.ScheduledDelay;
import com.simibubi.create.content.trains.schedule.condition.TimedWaitCondition;
import com.simibubi.create.content.trains.schedule.destination.DestinationInstruction;
import com.simibubi.create.content.trains.station.GlobalStation;
import com.simibubi.create.content.trains.station.StationBlockEntity;
import com.simibubi.create.content.trains.track.TrackBlock;
import com.simibubi.create.content.trains.track.TrackShape;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Districts;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtUtils;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplate;
import net.minecraft.world.level.storage.LevelResource;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.items.IItemHandler;
import net.neoforged.neoforge.items.ItemHandlerHelper;
import org.apache.commons.lang3.tuple.MutablePair;
import org.slf4j.Logger;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Random;
import java.util.TreeMap;
import java.util.UUID;
import java.util.function.Supplier;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.fill;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.id;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.item;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.SteamHelpers.smartChute;

/**
 * Builds the synthetic stress world: N districts of real Create factories and M scheduled cargo trains
 * between neighbouring districts that load and unload through Portable Storage Interfaces, on the
 * GameTest server's flat world, then writes {@code stressgen.json} (what was built) and
 * {@code stressgen-scene.json} (an sw.py scene) into the world folder. The GameTest server saves the
 * world when the test passes.
 *
 * <p><b>Layout</b> (chunks; y {@value #Y} is the flat world's first air layer). District i sits in a grid
 * of C columns, filled row by row from chunk ({@value #CX0}, {@value #CZ0}):
 * <pre>
 *   one district, Hc = 2 chunk rows:  [W][M][M]...[M][E]  then one empty chunk column, one empty chunk row
 *   W  west station column: lane k's "D{i} W{k}" stations (the far end of the line from district i-1)
 *   M  machine columns, ceil(cells / 8) of them: four 8x8 slots per chunk, one factory per slot
 *   E  east station column: lane k's "D{i} E{k}" stations (the near end of the line to district i+1)
 * </pre>
 * The factories are the glass corpus's (millstone flour, press sheets, saw shafts, brass mixing), each a
 * running build with a creative motor and full input chests, placed by the corpus classes themselves
 * through a helper whose origin is the slot.
 *
 * <p><b>Lines</b>. Each pair of horizontally neighbouring districts (a, a+1) has up to four lanes, one
 * straight XO track each at z = row z + 5 + 8k, from "D{a} E{k}" (assembly east) to "D{a+1} W{k}"
 * (assembly west). Train j runs on pair j mod |P|, lane j div |P|: one carriage on a small bogey with
 * controls at both ends and a blaze burner conductor between them, a chest and a PSI facing north. Its
 * cyclic schedule is: to B, wait {@code dwell} s; to A, wait {@code dwell} s. Good X goes A to B and good
 * Y back.
 *
 * <p><b>Ports</b>. A station's port is a stack north of the track at the block where the carriage PSI's
 * active area stops (calibrated on the train's first stop there, not computed): sink vault (2x2x2),
 * smart chute pulling the incoming good out of the PSI, the PSI facing south, smart chute pushing the
 * outgoing good in (one per pull), supply vault (2x2x2) with {@value #SUPPLY} of the outgoing good.
 * A PSI only docks when its active area enters the block at 0.25 blocks per tick or less, which a train
 * does on its last block of braking; so the ports are built after the first visit and docking starts on
 * the second. The test passes when every line's two sinks hold the good the other end sent.
 */
final class StressWorld {
    private static final Logger LOG = LogUtils.getLogger();

    static final int MAX_TICKS = 1_000_000;
    static final int Y = -60;
    static final int CX0 = 64, CZ0 = 64;
    static final int HC = 2, LANES = 4;
    static final int SUPPLY = 8000;
    /** Ticks after an arrival before the PSI actor's position is read. */
    static final int CALIBRATE_AFTER = 10;
    /** Ticks the trains get, after assembly, to make their calibration stops and two loaded visits. */
    static final int VERIFY_TICKS = 40_000;

    static final List<String> GOODS = List.of("minecraft:iron_ingot", "minecraft:copper_ingot", "minecraft:gold_ingot",
            "minecraft:coal", "minecraft:cobblestone", "minecraft:gravel", "minecraft:sand", "minecraft:oak_log",
            "minecraft:wheat", "minecraft:redstone", "create:andesite_alloy", "create:zinc_ingot", "create:brass_ingot",
            "create:wheat_flour", "create:iron_sheet");

    static final Map<String, Supplier<CorpusFactory>> KINDS = new LinkedHashMap<>();

    static {
        KINDS.put("millstone", MillstoneFlourFactory::new);
        KINDS.put("press", PressSheetsFactory::new);
        KINDS.put("saw", SawShaftsFactory::new);
        KINDS.put("brass", BrassMixingFactory::new);
    }

    /** The generator's parameters, from MIC_STRESS_* environment variables (choice SG2). */
    record Params(String name, int districts, int cells, int trains, long seed, int dwell, Map<String, Integer> kinds) {
        static Params fromEnv() {
            Map<String, Integer> kinds = new LinkedHashMap<>();
            for (String part : env("MIC_STRESS_KINDS", "millstone:4,press:2,saw:2,brass:1").split(",")) {
                String[] kv = part.trim().split(":");
                if (!KINDS.containsKey(kv[0]))
                    throw new IllegalArgumentException("MIC_STRESS_KINDS: no factory kind " + kv[0] + " (have " + KINDS.keySet() + ")");
                kinds.put(kv[0], kv.length > 1 ? Integer.parseInt(kv[1]) : 1);
            }
            return new Params(env("MIC_STRESS_NAME", "synthetic"),
                    Integer.parseInt(env("MIC_STRESS_DISTRICTS", "40")),
                    Integer.parseInt(env("MIC_STRESS_CELLS", "32")),
                    Integer.parseInt(env("MIC_STRESS_TRAINS", "-1")),
                    Long.parseLong(env("MIC_STRESS_SEED", "1")),
                    Integer.parseInt(env("MIC_STRESS_DWELL", "10")),
                    kinds);
        }

        private static String env(String k, String def) {
            String v = System.getenv(k);
            return v == null || v.isBlank() ? def : v.trim();
        }
    }

    enum Phase { CHUNKS, FACTORIES, TRACKS, STATIONS, NAMES, CARS, ENTER, ASSEMBLE, CALIBRATE, ADOPT, PROFILE, UNFORCE, WRITE, DONE }

    final class Lane {
        final int j, a, b, k, zt, xsA, xsB;
        final String nameA, nameB, trainName;
        final Item out, back;
        final BlockPos stA, stB;
        Train train;
        int state;
        long t;
        BlockPos dockA, dockB;
        long assembledAt = -1, firstB = -1, firstA = -1, verifiedAt = -1;

        Lane(int j, int a, int b, int k, Random rng) {
            this.j = j;
            this.a = a;
            this.b = b;
            this.k = k;
            this.zt = z0(a) * 16 + 5 + 8 * k;
            this.xsA = (x0(a) + wc - 1) * 16 + 3;
            this.xsB = x0(b) * 16 + 12;
            this.nameA = "D" + a + " E" + k;
            this.nameB = "D" + b + " W" + k;
            this.trainName = "Stress " + j + " (D" + a + "-D" + b + ")";
            int x = rng.nextInt(GOODS.size()), y = rng.nextInt(GOODS.size() - 1);
            if (y >= x)
                y++;
            this.out = item(GOODS.get(x));
            this.back = item(GOODS.get(y));
            this.stA = new BlockPos(xsA, Y, zt + 1);
            this.stB = new BlockPos(xsB, Y, zt + 1);
        }

        StationBlockEntity sbeA() {
            return station(stA);
        }

        StationBlockEntity sbeB() {
            return station(stB);
        }

        String where() {
            Train tr = train;
            if (tr == null)
                return "no train";
            GlobalStation cur = tr.getCurrentStation();
            return "state " + state + ", at " + (cur == null ? "-" : cur.name) + ", destination "
                    + (tr.navigation.destination == null ? "-" : tr.navigation.destination.name) + ", schedule state " + tr.runtime.state
                    + " entry " + tr.runtime.currentEntry + ", derailed " + tr.derailed + ", sink A " + count(dockA == null ? null : dockA.below(3), back)
                    + " " + id(back) + ", sink B " + count(dockB == null ? null : dockB.below(3), out) + " " + id(out);
        }
    }

    final GameTestHelper h;
    final ServerLevel level;
    final Params p;
    final int mc, wc, pitchX, pitchZ, cols, rows;
    final List<long[]> chunks = new ArrayList<>();
    final List<int[]> pairs = new ArrayList<>();
    final List<Lane> lanes = new ArrayList<>();
    final Map<String, Integer> built = new TreeMap<>();
    final List<Map<String, Object>> districtInfo = new ArrayList<>();
    final Map<String, Object> phaseLog = new LinkedHashMap<>();
    final Random rng;
    final int kindTotal;

    Phase phase = Phase.CHUNKS;
    int cursor;
    long phaseTick, phaseNanos, startTick, startNanos;
    boolean failed;
    long gameTime;

    StressWorld(GameTestHelper h, Params p) {
        this.h = h;
        this.level = h.getLevel();
        this.p = p;
        if (p.districts < 2 || p.cells < 1)
            throw new IllegalArgumentException("MIC_STRESS_DISTRICTS must be at least 2 and MIC_STRESS_CELLS at least 1");
        this.mc = (p.cells + 7) / 8;
        this.wc = mc + 2;
        this.pitchX = wc + 1;
        this.pitchZ = HC + 1;
        this.cols = Math.max(2, (int) Math.ceil(Math.sqrt(p.districts * pitchZ / (double) pitchX)));
        this.rows = (p.districts + cols - 1) / cols;
        this.rng = new Random(p.seed);
        this.kindTotal = p.kinds.values().stream().mapToInt(Integer::intValue).sum();

        for (int cz = CZ0; cz < CZ0 + rows * pitchZ; cz++)
            for (int cx = CX0; cx < CX0 + cols * pitchX; cx++)
                chunks.add(new long[]{cx, cz});
        for (int r = 0; r < rows; r++)
            for (int c = 0; c + 1 < cols; c++) {
                int a = r * cols + c;
                if (a + 1 < p.districts)
                    pairs.add(new int[]{a, a + 1});
            }
        int m = p.trains < 0 ? 2 * pairs.size() : Math.min(p.trains, LANES * pairs.size());
        for (int j = 0; j < m; j++) {
            int[] pr = pairs.get(j % pairs.size());
            lanes.add(new Lane(j, pr[0], pr[1], j / pairs.size(), rng));
        }
    }

    int x0(int i) {
        return CX0 + (i % cols) * pitchX;
    }

    int z0(int i) {
        return CZ0 + (i / cols) * pitchZ;
    }

    void start() {
        LOG.info("[stressgen] {}: {} districts of {} factories ({} x {} grid, district {} x {} chunks), {} lines on {} pairs, seed {}, dwell {} s, kinds {}",
                p.name, p.districts, p.cells, cols, rows, wc, HC, lanes.size(), pairs.size(), p.seed, p.dwell, p.kinds);
        startTick = phaseTick = level.getGameTime();
        startNanos = phaseNanos = System.nanoTime();
        h.startSequence().thenWaitUntil(this::tick).thenSucceed();
    }

    private void tick() {
        if (failed)
            throw new GameTestAssertException("failed");
        gameTime = level.getGameTime();
        boolean done;
        try {
            done = step();
        } catch (GameTestAssertException e) {
            throw e;
        } catch (RuntimeException e) {
            LOG.error("[stressgen] {} failed", phase, e);
            fail(phase + ": " + e);
            return;
        }
        if (!done)
            throw new GameTestAssertException("building: " + phase);
    }

    private void fail(String why) {
        failed = true;
        LOG.error("[stressgen] FAILED: {}", why);
        h.testInfo.fail(new GameTestAssertException(why));
        throw new GameTestAssertException(why);
    }

    private void next(Phase to) {
        long ticks = gameTime - phaseTick;
        double s = (System.nanoTime() - phaseNanos) / 1e9;
        phaseLog.put(phase.name().toLowerCase(Locale.ROOT), Map.of("ticks", ticks, "seconds", Math.round(s * 10) / 10.0));
        LOG.info("[stressgen] {} done in {} ticks, {} s", phase, ticks, String.format(Locale.ROOT, "%.1f", s));
        phase = to;
        cursor = 0;
        phaseTick = gameTime;
        phaseNanos = System.nanoTime();
    }

    private long inPhase() {
        return gameTime - phaseTick;
    }

    /** One tick of the build; true when the world is finished. */
    private boolean step() {
        switch (phase) {
            case CHUNKS -> {
                for (int n = 0; n < 128 && cursor < chunks.size(); n++, cursor++) {
                    long[] c = chunks.get(cursor);
                    level.setChunkForced((int) c[0], (int) c[1], true);
                    level.getChunk((int) c[0], (int) c[1]);
                }
                if (cursor >= chunks.size())
                    next(Phase.FACTORIES);
            }
            case FACTORIES -> {
                if (cursor < p.districts)
                    factories(cursor++);
                else
                    next(Phase.TRACKS);
            }
            case TRACKS -> {
                if (cursor < lanes.size()) {
                    for (int n = 0; n < 32 && cursor < lanes.size(); n++)
                        track(lanes.get(cursor++));
                } else if (inPhase() >= 5) {
                    next(Phase.STATIONS);
                }
            }
            case STATIONS -> {
                if (cursor < lanes.size()) {
                    for (int n = 0; n < 64 && cursor < lanes.size(); n++) {
                        Lane l = lanes.get(cursor++);
                        point(l.stA, new BlockPos(0, 0, -1), true);
                        point(l.stB, new BlockPos(0, 0, -1), false);
                    }
                } else {
                    List<String> missing = new ArrayList<>();
                    for (Lane l : lanes) {
                        if (l.sbeA().getStation() == null)
                            missing.add(l.nameA);
                        if (l.sbeB().getStation() == null)
                            missing.add(l.nameB);
                    }
                    if (missing.isEmpty())
                        next(Phase.NAMES);
                    else if (inPhase() > 400)
                        fail(missing.size() + " stations never joined their track graph: " + head(missing));
                }
            }
            case NAMES -> {
                for (Lane l : lanes) {
                    StationBlockEntity a = l.sbeA(), b = l.sbeB();
                    a.updateName(l.nameA);
                    b.updateName(l.nameB);
                    if (a.getAssemblyDirection() != Direction.EAST || b.getAssemblyDirection() != Direction.WEST)
                        fail("line " + l.j + ": assembly directions " + a.getAssemblyDirection() + " and " + b.getAssemblyDirection()
                                + ", expected EAST at " + l.nameA + " and WEST at " + l.nameB);
                }
                next(Phase.CARS);
            }
            case CARS -> {
                if (cursor < lanes.size()) {
                    for (int n = 0; n < 64 && cursor < lanes.size(); n++)
                        car(lanes.get(cursor++));
                } else if (inPhase() >= 5) {
                    next(Phase.ENTER);
                }
            }
            case ENTER -> {
                if (cursor < lanes.size()) {
                    for (int n = 0; n < 64 && cursor < lanes.size(); n++) {
                        Lane l = lanes.get(cursor++);
                        if (!l.sbeA().enterAssemblyMode(null))
                            fail("line " + l.j + ": " + l.nameA + " would not enter assembly mode");
                    }
                } else if (inPhase() >= 5) {
                    next(Phase.ASSEMBLE);
                }
            }
            case ASSEMBLE -> {
                if (cursor < lanes.size()) {
                    for (int n = 0; n < 16 && cursor < lanes.size(); n++)
                        assemble(lanes.get(cursor++));
                } else {
                    Create.RAILWAYS.markTracksDirty();
                    next(Phase.CALIBRATE);
                }
            }
            case CALIBRATE -> {
                int done = 0;
                for (Lane l : lanes) {
                    lane(l);
                    if (l.state >= 6)
                        done++;
                }
                if (done == lanes.size()) {
                    next(profileReps > 0 ? Phase.ADOPT : Phase.UNFORCE);
                } else if (inPhase() > VERIFY_TICKS) {
                    List<String> stuck = new ArrayList<>();
                    for (Lane l : lanes)
                        if (l.state < 6)
                            stuck.add("line " + l.j + " " + l.nameA + " -> " + l.nameB + ": " + l.where());
                    fail((lanes.size() - done) + " of " + lanes.size() + " lines not verified after " + VERIFY_TICKS + " ticks: " + head(stuck));
                }
            }
            case ADOPT -> {
                // MIC_STRESS_PROFILE: the scene's adopts, as sw.py makes them, so there are districts to time (SG60).
                Industry ind = IndustryApi.current();
                if (ind == null)
                    fail("MIC_STRESS_PROFILE needs the industry module, and there is none in this run");
                List<List<Integer>> points = adoptPoints();
                for (int n = 0; n < 40 && cursor < points.size(); n++, cursor++) {
                    List<Integer> a = points.get(cursor);
                    ind.districts().adoptAround(level, new ChunkPos(new BlockPos(a.get(0), a.get(1), a.get(2))), a.get(3));
                }
                if (cursor >= points.size() && !ind.districts().rescanning())
                    next(Phase.PROFILE);
                else if (inPhase() > 6000)
                    fail("the adopts' detection did not finish in 6000 ticks");
            }
            case PROFILE -> {
                profile(IndustryApi.current());
                next(Phase.UNFORCE);
            }
            case UNFORCE -> {
                for (long[] c : chunks)
                    level.setChunkForced((int) c[0], (int) c[1], false);
                next(Phase.WRITE);
            }
            case WRITE -> {
                write();
                next(Phase.DONE);
                LOG.info("[stressgen] {} built in {} ticks, {} s: {} factories {}, {} lines verified",
                        p.name, gameTime - startTick, String.format(Locale.ROOT, "%.1f", (System.nanoTime() - startNanos) / 1e9),
                        built.values().stream().mapToInt(Integer::intValue).sum(), built, lanes.size());
                return true;
            }
            case DONE -> {
                return true;
            }
        }
        return false;
    }

    /** MIC_STRESS_PROFILE, the number of timed one-call detections to run, 0 for none. */
    final int profileReps = Integer.parseInt(System.getenv().getOrDefault("MIC_STRESS_PROFILE", "0").trim());

    /**
     * Times {@code profileReps} one-call detections ({@link Districts#rescanNow}) on the built world, then
     * the same number again under a stack sampler, and logs both with where the detection's time went. The
     * one call is the shape the operator commands have (SG60): one big tick.
     */
    private void profile(Industry ind) {
        Districts ds = ind.districts();
        LOG.info("[stressgen] profile: {} districts, {} chunks forced", ds.all().size(), chunks.size());
        for (int i = 0; i < 3; i++)
            ds.rescanNow(level);
        long[] ns = new long[profileReps];
        for (int i = 0; i < profileReps; i++) {
            long t0 = System.nanoTime();
            ds.rescanNow(level);
            ns[i] = System.nanoTime() - t0;
        }
        java.util.Arrays.sort(ns);
        LOG.info("[stressgen] profile: one-call detection, {} runs: min {} ms, median {} ms, max {} ms; {} block entities looked at; {}",
                profileReps, ns[0] / 1_000_000, ns[ns.length / 2] / 1_000_000, ns[ns.length - 1] / 1_000_000, ds.lastRescanLooked(), ds.lastRescanPhases());
        try (StackSampler s = new StackSampler(Thread.currentThread(), "rescanNow", 200_000)) {
            for (int i = 0; i < profileReps; i++)
                ds.rescanNow(level);
            s.close();
            for (String line : s.report(30))
                LOG.info("[stressgen] profile: {}", line);
        }
    }

    private static String head(List<String> l) {
        return l.size() <= 8 ? String.join("; ", l) : String.join("; ", l.subList(0, 8)) + "; and " + (l.size() - 8) + " more";
    }

    // ---- factories ------------------------------------------------------------------------------

    /** District i's factories: slot s in machine column s / 8, chunk row (s % 8) / 4, quarter s % 4. */
    private void factories(int i) {
        Map<String, Integer> here = new TreeMap<>();
        for (int s = 0; s < p.cells; s++) {
            int col = s / 8, r = s % 8, row = r / 4, q = r % 4;
            int cornerX = (x0(i) + 1 + col) * 16 + (q % 2) * 8, cornerZ = (z0(i) + row) * 16 + (q / 2) * 8;
            String kind = pickKind();
            // the corpus builds span yard x 5..9, z 6..9, y 1..5: shifted to the slot's x +1..5, z +1..4
            BlockPos origin = new BlockPos(cornerX - 4, Y - 1, cornerZ - 5);
            GameTestHelper at = new GameTestHelper(h.testInfo) {
                @Override
                public BlockPos absolutePos(BlockPos pos) {
                    return origin.offset(pos);
                }
            };
            CorpusFactory.Built b = KINDS.get(kind).get().build(at);
            for (BlockPos in : b.inputs())
                topUp(in);
            here.merge(kind, 1, Integer::sum);
            built.merge(kind, 1, Integer::sum);
        }
        Map<String, Object> d = new LinkedHashMap<>();
        d.put("index", i);
        d.put("chunks", List.of(x0(i), z0(i), x0(i) + wc - 1, z0(i) + HC - 1));
        d.put("factories", here);
        districtInfo.add(d);
    }

    private String pickKind() {
        int x = rng.nextInt(kindTotal);
        for (var e : p.kinds.entrySet()) {
            x -= e.getValue();
            if (x < 0)
                return e.getKey();
        }
        throw new IllegalStateException();
    }

    /** Fills an input store with more of the good it already holds, so the factory runs for hours. */
    private void topUp(BlockPos pos) {
        IItemHandler inv = level.getCapability(Capabilities.ItemHandler.BLOCK, pos, null);
        if (inv == null)
            return;
        Item good = null;
        for (int s = 0; s < inv.getSlots() && good == null; s++)
            if (!inv.getStackInSlot(s).isEmpty())
                good = inv.getStackInSlot(s).getItem();
        if (good == null)
            return;
        for (int n = 0; n < 256; n++)
            if (!ItemHandlerHelper.insertItemStacked(inv, new ItemStack(good, good.getDefaultMaxStackSize()), false).isEmpty())
                return;
    }

    // ---- rail -----------------------------------------------------------------------------------

    private void track(Lane l) {
        BlockState xo = AllBlocks.TRACK.getDefaultState().setValue(TrackBlock.SHAPE, TrackShape.XO);
        for (int x = l.xsA - 2; x <= l.xsB + 2; x++)
            level.setBlock(new BlockPos(x, Y, l.zt), xo, 3);
    }

    /**
     * A station targeting the track at {@code pos + target}, as RailFixtureGameTests places one, with the
     * edge point's direction: true makes an XO station's assembly direction east, false west.
     */
    private void point(BlockPos pos, BlockPos target, boolean direction) {
        level.setBlock(pos, AllBlocks.TRACK_STATION.getDefaultState(), 3);
        var be = level.getBlockEntity(pos);
        CompoundTag tag = be.saveWithFullMetadata(level.registryAccess());
        tag.put("TargetTrack", NbtUtils.writeBlockPos(target));
        tag.putBoolean("TargetDirection", direction);
        be.loadWithComponents(tag, level.registryAccess());
    }

    private StationBlockEntity station(BlockPos pos) {
        if (!(level.getBlockEntity(pos) instanceof StationBlockEntity s))
            throw new GameTestAssertException("no station at " + pos.toShortString());
        return s;
    }

    /**
     * The carriage on station A's track, east of the station block: small bogey at +1; controls facing
     * east at +1, a smouldering blaze burner (the conductor, CarriageContraption.inControl) at +2,
     * controls facing west at +3; a chest at +2 above the burner with 64 of good X, and a PSI facing north
     * beside the chest. One glue box over all of it.
     */
    private void car(Lane l) {
        BlockPos t = new BlockPos(l.xsA, Y, l.zt);
        BlockPos bogey = t.east(1).above(), chest = t.east(2).above(3);
        level.setBlock(bogey, AllBlocks.SMALL_BOGEY.getDefaultState().setValue(BlockStateProperties.HORIZONTAL_AXIS, Direction.Axis.X), 3);
        level.setBlock(t.east(1).above(2), AllBlocks.TRAIN_CONTROLS.getDefaultState().setValue(BlockStateProperties.HORIZONTAL_FACING, Direction.EAST), 3);
        level.setBlock(t.east(2).above(2), AllBlocks.BLAZE_BURNER.getDefaultState()
                .setValue(BlazeBurnerBlock.HEAT_LEVEL, BlazeBurnerBlock.HeatLevel.SMOULDERING), 3);
        level.setBlock(t.east(3).above(2), AllBlocks.TRAIN_CONTROLS.getDefaultState().setValue(BlockStateProperties.HORIZONTAL_FACING, Direction.WEST), 3);
        level.setBlock(chest, net.minecraft.world.level.block.Blocks.CHEST.defaultBlockState(), 3);
        fill(h, chest, l.out, 64);
        level.setBlock(chest.north(), AllBlocks.PORTABLE_STORAGE_INTERFACE.getDefaultState()
                .setValue(PortableStorageInterfaceBlock.FACING, Direction.NORTH), 3);
        level.addFreshEntity(new SuperGlueEntity(level, SuperGlueEntity.span(new BlockPos(l.xsA + 1, Y + 1, l.zt - 1), new BlockPos(l.xsA + 3, Y + 3, l.zt))));
    }

    private void assemble(Lane l) {
        StationBlockEntity sbe = l.sbeA();
        sbe.assemble(UUID.nameUUIDFromBytes(("mic-rtsim stressgen " + p.seed + " " + l.j).getBytes(StandardCharsets.UTF_8)));
        GlobalStation st = sbe.getStation();
        Train train = st == null ? null : st.getPresentTrain();
        if (train == null) {
            CompoundTag tag = sbe.saveWithFullMetadata(level.registryAccess());
            fail("line " + l.j + ": no train assembled at " + l.nameA + ": " + tag.get("LastException") + " " + tag.get("FailedCarriageIndex"));
            return;
        }
        train.name = Component.literal(l.trainName);
        Schedule schedule = new Schedule();
        schedule.entries.add(entry(l.nameB));
        schedule.entries.add(entry(l.nameA));
        train.runtime.setSchedule(schedule, false);
        sbe.exitAssemblyMode();
        l.train = train;
        l.assembledAt = gameTime;
    }

    private ScheduleEntry entry(String destination) {
        ScheduleEntry e = new ScheduleEntry();
        e.instruction = new DestinationInstruction();
        e.instruction.getData().putString("Text", destination);
        ScheduledDelay wait = new ScheduledDelay();
        wait.getData().putInt("Value", p.dwell);
        wait.getData().putInt("TimeUnit", TimedWaitCondition.TimeUnit.SECONDS.ordinal());
        e.conditions.add(new ArrayList<>(List.of(wait)));
        return e;
    }

    // ---- ports ----------------------------------------------------------------------------------

    /**
     * One line's calibration and check: 0 going to B, 1 stopped at B (read the PSI's active block, build
     * B's port), 2 going to A, 3 stopped at A (build A's port), 4 fill both supplies, 5 wait for both
     * sinks, 6 verified.
     */
    private void lane(Lane l) {
        GlobalStation cur = l.train.getCurrentStation();
        switch (l.state) {
            case 0 -> {
                if (cur != null && cur.name.equals(l.nameB)) {
                    l.state = 1;
                    l.t = gameTime;
                    l.firstB = gameTime;
                }
            }
            case 1 -> {
                if (gameTime - l.t >= CALIBRATE_AFTER) {
                    l.dockB = dock(l);
                    port(l.dockB, l.out, l.back);
                    l.state = 2;
                }
            }
            case 2 -> {
                if (cur != null && cur.name.equals(l.nameA)) {
                    l.state = 3;
                    l.t = gameTime;
                    l.firstA = gameTime;
                }
            }
            case 3 -> {
                if (gameTime - l.t >= CALIBRATE_AFTER) {
                    l.dockA = dock(l);
                    port(l.dockA, l.back, l.out);
                    l.state = 4;
                    l.t = gameTime;
                }
            }
            case 4 -> {
                if (gameTime - l.t >= 10) { // the new vault forms on its first tick; fill the whole multiblock
                    fill(h, l.dockA.above(2), l.out, SUPPLY);
                    fill(h, l.dockB.above(2), l.back, SUPPLY);
                    l.state = 5;
                }
            }
            case 5 -> {
                if (count(l.dockB.below(3), l.out) > 0 && count(l.dockA.below(3), l.back) > 0) {
                    l.state = 6;
                    l.verifiedAt = gameTime;
                }
            }
            default -> {
            }
        }
    }

    /**
     * The block the carriage PSI's active area is in now ({@code MovementContext.position} already holds
     * the actor's active-area offset, AbstractContraptionEntity.tickActors), checked against the layout:
     * y {@value #Y} + 3 and two blocks north of the carriage.
     */
    private BlockPos dock(Lane l) {
        CarriageContraptionEntity e = l.train.carriages.get(0).anyAvailableEntity();
        if (e == null)
            fail("line " + l.j + ": no carriage entity at " + l.train.getCurrentStation().name);
        for (MutablePair<StructureTemplate.StructureBlockInfo, MovementContext> actor : e.getContraption().getActors()) {
            if (!AllBlocks.PORTABLE_STORAGE_INTERFACE.has(actor.left.state()) || actor.right.position == null)
                continue;
            BlockPos g = BlockPos.containing(actor.right.position);
            if (g.getY() != Y + 3 || g.getZ() != l.zt - 3)
                fail("line " + l.j + ": the carriage PSI's active area is at " + actor.right.position + " (block " + g.toShortString()
                        + "), expected y " + (Y + 3) + ", z " + (l.zt - 3));
            return g;
        }
        fail("line " + l.j + ": the carriage has no PSI actor");
        return null;
    }

    /**
     * A port at {@code g}: sink vault (y -3..-2), smart chute pulling {@code in} from the PSI (y -1), the
     * PSI facing south (y 0), smart chute pushing {@code outgoing} into it (y +1), supply vault (y +2..+3).
     * The vaults run along x, two by two by two, so their item handlers are at g + (0, -3, 0) and g + (0, 2, 0).
     */
    private void port(BlockPos g, Item in, Item outgoing) {
        vault(g.below(3));
        smartChute(h, g.below(), in, 64);
        level.setBlock(g, AllBlocks.PORTABLE_STORAGE_INTERFACE.getDefaultState().setValue(PortableStorageInterfaceBlock.FACING, Direction.SOUTH), 3);
        smartChute(h, g.above(), outgoing, 1);
        vault(g.above(2));
    }

    private void vault(BlockPos corner) {
        BlockState v = AllBlocks.ITEM_VAULT.getDefaultState().setValue(ItemVaultBlock.HORIZONTAL_AXIS, Direction.Axis.X);
        for (int dx = 0; dx < 2; dx++)
            for (int dy = 0; dy < 2; dy++)
                for (int dz = 0; dz < 2; dz++)
                    level.setBlock(corner.offset(dx, dy, -dz), v, 3);
    }

    private int count(BlockPos pos, Item good) {
        if (pos == null)
            return -1;
        IItemHandler inv = level.getCapability(Capabilities.ItemHandler.BLOCK, pos, null);
        if (inv == null)
            return -1;
        int n = 0;
        for (int s = 0; s < inv.getSlots(); s++) {
            ItemStack st = inv.getStackInSlot(s);
            if (st.is(good))
                n += st.getCount();
        }
        return n;
    }

    // ---- output ---------------------------------------------------------------------------------

    /** stressgen.json and stressgen-scene.json in the world folder (format: choice SG8). */
    private void write() {
        Map<String, Object> grid = new LinkedHashMap<>();
        grid.put("columns", cols);
        grid.put("rows", rows);
        grid.put("origin_chunk", List.of(CX0, CZ0));
        grid.put("district_chunks", List.of(wc, HC));
        grid.put("pitch_chunks", List.of(pitchX, pitchZ));
        grid.put("y", Y);
        List<Map<String, Object>> trains = new ArrayList<>();
        for (Lane l : lanes) {
            Map<String, Object> t = new LinkedHashMap<>();
            t.put("index", l.j);
            t.put("name", l.trainName);
            t.put("id", l.train.id.toString());
            t.put("districts", List.of(l.a, l.b));
            t.put("lane", l.k);
            t.put("stations", List.of(l.nameA, l.nameB));
            t.put("station_blocks", List.of(xyz(l.stA), xyz(l.stB)));
            t.put("docks", List.of(xyz(l.dockA), xyz(l.dockB)));
            t.put("goods", List.of(id(l.out), id(l.back)));
            t.put("ticks", Map.of("assembled", l.assembledAt - startTick, "first_b", l.firstB - startTick,
                    "first_a", l.firstA - startTick, "verified", l.verifiedAt - startTick));
            trains.add(t);
        }
        Map<String, Object> params = new LinkedHashMap<>();
        params.put("name", p.name);
        params.put("districts", p.districts);
        params.put("cells", p.cells);
        params.put("trains", lanes.size());
        params.put("seed", p.seed);
        params.put("dwell_s", p.dwell);
        params.put("kinds", p.kinds);
        Map<String, Object> out = new LinkedHashMap<>();
        out.put("generator", StressWorldGameTests.NS);
        out.put("format", 1);
        out.put("params", params);
        out.put("grid", grid);
        out.put("factories", built);
        out.put("build", Map.of("ticks", gameTime - startTick, "seconds", Math.round((System.nanoTime() - startNanos) / 1e8) / 10.0, "phases", phaseLog));
        out.put("districts", districtInfo);
        out.put("trains", trains);

        Path dir = level.getServer().getWorldPath(LevelResource.ROOT);
        try {
            var gson = new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create();
            Files.writeString(dir.resolve("stressgen.json"), gson.toJson(out) + "\n");
            Files.writeString(dir.resolve("stressgen-scene.json"), gson.toJson(scene()) + "\n");
        } catch (IOException e) {
            throw new RuntimeException("writing stressgen.json", e);
        }
    }

    private static List<Integer> xyz(BlockPos p) {
        return p == null ? null : List.of(p.getX(), p.getY(), p.getZ());
    }

    /** The scene's adopts: x, y, z and radius of each, along every district's middle row. */
    private List<List<Integer>> adoptPoints() {
        List<List<Integer>> adopt = new ArrayList<>();
        for (int i = 0; i < p.districts; i++)
            for (int cx = x0(i) + 1; cx - 1 < x0(i) + wc; cx += 3)
                adopt.add(List.of(Math.min(cx, x0(i) + wc - 1) * 16 + 8, Y, z0(i) * 16 + 8, 1));
        return adopt;
    }

    /**
     * The sw.py scene: every district row force-loaded (boxes of at most 256 chunks, vanilla's limit), each
     * district adopted with radius-1 adopts every three chunks along its middle, and a busy window past
     * the 6,000-tick periodic detection pass (choice SG9).
     */
    private Map<String, Object> scene() {
        List<List<Integer>> boxes = new ArrayList<>();
        int width = cols * pitchX, perBox = Math.max(1, 256 / HC);
        for (int r = 0; r < rows; r++) {
            int cz = CZ0 + r * pitchZ;
            for (int cx = CX0; cx < CX0 + width; cx += perBox) {
                int cx2 = Math.min(CX0 + width, cx + perBox) - 1;
                boxes.add(List.of(cx * 16, cz * 16, cx2 * 16 + 15, (cz + HC) * 16 - 1));
            }
        }
        List<List<Integer>> adopt = adoptPoints();
        Map<String, Object> sc = new LinkedHashMap<>();
        sc.put("forceload", boxes);
        sc.put("adopt", adopt);
        sc.put("idle", 60);
        sc.put("settle", 120);
        sc.put("busy", 600);
        sc.put("sample", 5);
        sc.put("unload_reload", 90);
        sc.put("info_max", 12);
        sc.put("rescan", true);
        return sc;
    }
}
