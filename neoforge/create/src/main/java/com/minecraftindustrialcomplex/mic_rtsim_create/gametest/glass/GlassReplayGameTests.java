package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Districts;
import com.simibubi.create.content.kinetics.base.GeneratingKineticBlockEntity;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import com.simibubi.create.content.kinetics.belt.BeltBlockEntity;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Goods;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryPlugin;
import com.minecraftindustrialcomplex.mic_rtsim.industry.StockAccess;
import com.minecraftindustrialcomplex.mic_rtsim.industry.TickGate;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassConfig;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassReport;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.GlassExitKit;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.GlassKit;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestGenerator;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestInfo;
import net.minecraft.gametest.framework.GameTestListener;
import net.minecraft.gametest.framework.GameTestRunner;
import net.minecraft.gametest.framework.TestFunction;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.TicketType;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.Rotation;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.fluids.FluidStack;
import net.neoforged.neoforge.fluids.capability.IFluidHandler;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;
import net.neoforged.neoforge.items.IItemHandler;

import java.nio.ByteBuffer;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.Comparator;
import java.util.IdentityHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.TreeSet;
import java.util.stream.Collectors;

/**
 * G2's exit test (GLASSBOX.md §11.3, phase G2): the glass box replays a factory while its chunks are away,
 * and what the reconcile hands back at the next load matches what the factory did in the world.
 *
 * <p>Each corpus factory is built twice in the same tick: the <b>control</b> in the GameTest yard and an
 * identical <b>far copy</b> {@link #farOffset} blocks away along x (far beyond any view or simulation
 * distance, a whole number of chunks so both copies meet the same chunk borders), each its own district.
 * The far copy's place is laid out as the framework lays out the yard ({@link #clear}: stone, an air row,
 * the template's floor at relative y 1) and its chunks are force-loaded exactly as the GameTest framework
 * forces the yard's, so both tick alike. Each copy is its own build (a factory draws fresh ids, such as a
 * logistics frequency, per build). Both run until the far copy's report has a completed passing window (G-B4: the replay is
 * eligible). Then, in one tick, both copies' stocks are snapshot and the far copy's chunks lose their
 * tickets: they stop ticking, unload for real (the engine sees IND_CHUNK_UNLOADED for every chunk) and stay
 * away at least {@link #SPAN} ticks from the snapshot while the control runs on. They are force-loaded again,
 * the reconcile lands (its result is journaled, no gate or held reconcile is left) and after {@link #SETTLE}
 * ticks both copies are snapshot again in one tick.
 *
 * <p><b>Judging</b>, per good, with {@code c} the control's change and {@code s} the far copy's: on the
 * native engine {@code s} is the far copy's own change (the engine's reconcile is in it); on the stub, which
 * simulates nothing, every IND_CHUNK_RECONCILE delta a fresh native industry engine sends the far district
 * over the same records ({@link NativeEngine#glassReplayOffline}) is applied to the far stores after the
 * reload, through the stock access the reconciler uses, takes before puts: {@code s} is again the far
 * copy's own change, and a replay that fills a store leaves no room for the settle to fill twice (a delta
 * the offline engine sends only over the later records is added on top, with a WARNING).
 * <ul>
 *   <li><b>at or below</b>: {@code sign(c) x s <= |c| + slack}, and {@code |s| <= slack} for {@code c = 0}:
 *       the replay never makes more, or uses more, than the control did;</li>
 *   <li><b>within tolerance</b>: {@code |s - c| <= tol}, {@code tol = slack + latency x |c| / span + WIP + }
 *       {@code input WIP yield + }{@link #REL_TOL}{@code  x |c|}.</li>
 * </ul>
 * {@code slack} is one batch or stack in flight (the larger of the good's stack size, {@link #FLUID_IN_FLIGHT}
 * for a fluid, and the control report's quantum times its window, capped at 2 x stack when a mover's
 * capacity and not a recipe batch makes it: {@link #batch}) plus the control's noise over the span
 * ({@link #CHECKSUM_Z} x the control report's sigma per tick x span). {@code latency} is the far report's
 * line latency plus the ticks neither copy ran: from the snapshot to the first record telling the engine
 * the district left LIVE, and from the first IND_CHUNK_LOADED to the tick every far chunk ticks again.
 * {@code WIP} is what the far copy held outside its stores at the snapshot (machines, belts, depots, item
 * entities), frozen in the saved chunks. {@code input WIP yield} (G2-25) is what the far machines' held inputs
 * at the snapshot make of the good at the compile's recipe yields ({@link #inputYield}): the replay leaves
 * machine WIP alone when the stores run dry, the control's machines finish it; within tolerance only, not
 * at or below. UNBOUNDED goods are judged only at or below. Every comparison is a
 * {@code [gametest] glass replay <factory>: row} line: good, class, control, simulated, slack, tolerance,
 * verdict; a {@code terms} line says what the tolerance is made of.
 *
 * <p>The <b>copy check</b> ({@link #copyCheck}): over the far copy's last {@link #WINDOW} ticks before the
 * unload, each good's change in its declared stores must be within one stack or batch in flight, the
 * control's checksum noise over that window and {@link #REL_TOL} of the control's change, of the control's.
 * So must each good's level in those stores at the snapshot (the noise over the ticks since the first
 * sample, the relative term of the control's change since then). Otherwise the far copy is not the factory
 * the control is, and every row reads INVALID COPY, a failure.
 * A factory in {@link #KNOWN_GAPS} is judged on the at or below half only: every row is marked KNOWN GAP
 * with what is missing, and a within miss prints without failing the test.
 *
 * <p>Once one {@link #WINDOW} of samples is in, the warm-up check runs the copy check's window test early:
 * a far copy that does not move its goods as the control does is dead, and both copies are built again
 * once (logged "far copy dead at warm-up, rebuilt", with both copies' kinetics). Dead again, INVALID COPY.
 *
 * <p>The report while the far copy is away must say replay model GLASS: on the native engine any report
 * read while it is unloaded; on the stub the offline report of the records up to the reload. Its replay
 * summary (regimes, breakpoints, oscillations, cut edges) is logged.
 *
 * <p>The <b>partial</b> variant ({@link #PARTIAL}) keeps every far chunk loaded: the chunk of the factory's
 * anchor keeps ticking blocks (a level 32 ticket) and the others are FULL without ticking (level 33,
 * Java sends IND_CHUNK_TICKING), so the glass box simulates part of the factory with its cut edges (I7,
 * G-B6) and reconciles it when the chunks tick again. Judged the same way. Its place against the chunk
 * grid is fixed, whatever the yard's: a border between the stores along one axis, the whole build inside
 * one chunk along the other, so the far district has two chunks.
 *
 * <p>The <b>straddle</b> smoke ({@link #STRADDLE}) is the partial variant across a chunk corner (three or
 * four district chunks, the ticking one with FULL-only neighbours; G2-63). It passes when every far report
 * read while away carries the snapshot's compile number and none after the first GLASS one says otherwise
 * (on the stub, the one offline report). Its rows are printed for information, not judged.
 *
 * <p>{@code MIC_GLASS_SUITE=replay} runs only this suite, {@code MIC_GLASS_ONLY} only the named factories.
 * Without natives ({@code -PskipJni}) the stub run cannot judge: it runs the cycle and logs.
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class GlassReplayGameTests {
    private GlassReplayGameTests() {}

    static final int WINDOW = GlassExitGameTests.WINDOW;
    /** Ticks between recorder polls (far inside the received log's 10,000 records) and native report polls. */
    static final int POLL = 100;
    /** Ticks between report polls on the stub, each of which replays the whole stream offline. */
    static final int STUB_REPORT_POLL = 400;
    /** Ticks from the snapshot to the reload, at least: 2.5 checksum windows away. */
    public static final int SPAN = 6000;
    /** Ticks the far copy stays unloaded at least, however long its chunks took to unload. */
    public static final int MIN_AWAY = 1200;
    /** Ticks from the far copy's reload to the second snapshot, with no gate and no held reconcile left at the end. */
    public static final int SETTLE = 200;
    /** {@code glass.checksumZ} in rtsim-industry's tunables.rs (not exposed to Java): the checksum's z. */
    public static final double CHECKSUM_Z = 3.0;
    /** The relative term of the tolerance: a tenth of the control's change. */
    public static final double REL_TOL = 0.10;
    /** A fluid's "stack" in flight: a bucket. */
    public static final long FLUID_IN_FLIGHT = 1000;
    /** The far copy's distance in chunks along x: past every other far-chunk test (3,000 to 3,700), one slot per factory. */
    static final int FAR_CHUNKS = 4000, FAR_STRIDE = 16;
    /** Yard-relative height cleared above the far copy's floor (the corpus builds up to y 14). */
    static final int TOP = 20;
    /** A wait on chunk IO, in ticks; each failed check sleeps 5 ms, so this is at least a minute. */
    static final int IO_TICKS = 12_000;
    /** Stub: ticks after the reload the offline reconcile may take to appear before the settle goes on without one. */
    static final int OFFLINE_WAIT = 60;
    /** The factories the partial variant runs (their far district spans two chunks, placed so, wherever the yard lands). */
    static final Set<String> PARTIAL = Set.of("brass_tunnels", "boiler_overstress");
    /**
     * The factories the straddle smoke runs: a partial copy across a chunk corner, so the ticking chunk has
     * FULL-only neighbours in the district (G2-63). It checks only that the far compile number holds and the
     * replay stays GLASS while away; its rows are printed for information, not judged.
     */
    static final Set<String> STRADDLE = Set.of("boiler_overstress");
    /** The yard template's width and depth ({@code yard.nbt}: 34 x 6 x 34, a polished andesite floor at its y 0). */
    static final int YARD_SIZE = 34;
    /**
     * Factories with a gap the lead knows of, by what is missing: judged on the at or below half only. Every
     * row says KNOWN GAP; a within miss prints, and does not fail the test.
     */
    static final Map<String, String> KNOWN_GAPS = Map.of("package_loop", "package addresses not modelled, For Ben 1");

    /** Keeps a chunk FULL (level 33, distance 0) and not block-ticking. */
    private static final TicketType<ChunkPos> FULL_ONLY =
            TicketType.create("mic_rtsim_create_replay_full", Comparator.comparingLong(ChunkPos::toLong));
    /** Keeps a chunk block-ticking (level 32, distance 1) with its neighbours FULL only. */
    private static final TicketType<ChunkPos> BLOCK_TICKING =
            TicketType.create("mic_rtsim_create_replay_ticking", Comparator.comparingLong(ChunkPos::toLong));

    @GameTestGenerator
    public static Collection<TestFunction> glassReplay() {
        String suite = System.getenv("MIC_GLASS_SUITE");
        if (suite != null && !suite.isBlank() && !suite.trim().equals("replay"))
            return List.of();
        String only = System.getenv("MIC_GLASS_ONLY");
        Set<String> filter = only == null || only.isBlank() ? Set.of()
                : Arrays.stream(only.split(",")).map(String::trim).filter(s -> !s.isEmpty()).collect(Collectors.toSet());
        List<TestFunction> out = new ArrayList<>();
        for (int i = 0; i < GlassCorpus.ALL.size(); i++) {
            CorpusFactory f = GlassCorpus.ALL.get(i);
            if (!filter.isEmpty() && !filter.contains(f.name()))
                continue;
            int slot = i;
            int maxTicks = f.warmupTicks() + 2 * (WINDOW + GlassExitGameTests.SLACK) + SPAN + 3 * IO_TICKS + SETTLE + 2000;
            out.add(new TestFunction("glassreplay_" + f.name(), "glassreplay." + f.name().toLowerCase(Locale.ROOT),
                    MicRtsimCreate.MODID + ":" + IndustryKit.YARD, Rotation.NONE, maxTicks, 0L, true, false, 1, 1, true,
                    helper -> start(helper, f, slot, false, false)));
            if (PARTIAL.contains(f.name())) {
                int pslot = GlassCorpus.ALL.size() + i;
                out.add(new TestFunction("glassreplay_partial_" + f.name(), "glassreplay.partial_" + f.name().toLowerCase(Locale.ROOT),
                        MicRtsimCreate.MODID + ":" + IndustryKit.YARD, Rotation.NONE, maxTicks, 0L, true, false, 1, 1, true,
                        helper -> start(helper, f, pslot, true, false)));
            }
            if (STRADDLE.contains(f.name())) {
                int sslot = 2 * GlassCorpus.ALL.size() + i;
                out.add(new TestFunction("glassreplay_straddle_" + f.name(), "glassreplay.straddle_" + f.name().toLowerCase(Locale.ROOT),
                        MicRtsimCreate.MODID + ":" + IndustryKit.YARD, Rotation.NONE, maxTicks, 0L, true, false, 1, 1, true,
                        helper -> start(helper, f, sslot, true, true)));
            }
        }
        return out;
    }

    /** The far copy's offset from the yard, in blocks: whole chunks along x, so chunk borders fall alike. */
    static BlockPos farOffset(int slot) {
        return new BlockPos(16 * (FAR_CHUNKS + FAR_STRIDE * slot), 0, 0);
    }

    /**
     * The yard's helper, moved by {@code offset}: a factory's {@code build} places the far copy through it
     * unchanged (positions through {@link #absolutePos}, entities through {@link #absoluteVec}).
     */
    static final class Far extends GameTestHelper {
        /** Whole chunks, plus (the partial variant) the shift that puts a chunk border through the factory. */
        BlockPos offset;

        Far(GameTestHelper yard, BlockPos offset) {
            super(yard.testInfo);
            this.offset = offset;
        }

        @Override
        public BlockPos absolutePos(BlockPos pos) {
            return super.absolutePos(pos).offset(offset);
        }

        @Override
        public BlockPos relativePos(BlockPos pos) {
            return super.relativePos(pos.subtract(offset));
        }

        @Override
        public Vec3 absoluteVec(Vec3 v) {
            return super.absoluteVec(v).add(offset.getX(), offset.getY(), offset.getZ());
        }

        @Override
        public Vec3 relativeVec(Vec3 v) {
            return super.relativeVec(v.subtract(offset.getX(), offset.getY(), offset.getZ()));
        }
    }

    enum Stage { LOAD_FAR, RUN, UNLOAD, AWAY, RELOAD, SETTLE, DONE }

    /** Both copies' declared stores at one tick, for the copy check. */
    record Sample(long t, Map<String, Long> control, Map<String, Long> far) {}

    /** One factory's run. */
    static final class Run {
        final GameTestHelper helper;
        final Far far;
        final CorpusFactory f;
        final boolean partial;
        /** The straddle smoke: a partial copy across a chunk corner, judged on its compile and model only. */
        final boolean straddle;
        final String test;
        final GlassExitKit.Recorder rec;
        final boolean judge;
        final boolean stub;
        /** The far copy's forced chunks: those its yard box intersects, as the framework forces the yard's. */
        final List<ChunkPos> farChunks = new ArrayList<>();
        /** The far copy's yard box (moved with the partial variant's shift once it is known). */
        BlockPos farMin, farMax;
        Stage stage = Stage.LOAD_FAR;
        int ticks, stageTicks;
        long builtAt = -1;
        CorpusFactory.Built control, copy;
        long cd = -1, fd = -1;
        /** Partial variant: the far chunk that keeps ticking. */
        ChunkPos ticking;
        long t1 = -1, unloadedAt = -1, forcedAt = -1, loadedAt = -1, quietSince = -1, t2 = -1;
        /** Reconciles Java held at the first snapshot (an earlier test's, for chunks that never came back). */
        int heldBase;
        boolean eligible;
        String eligibleBy = "";
        List<BlockPos> cPos, fPos;
        Map<String, Long> c1, f1, wip1;
        GlassReport cReport1, fReport1, away;
        boolean sawGlass;
        /** Reports read while away after the first GLASS one that no longer say GLASS, and the last of them. */
        int leftGlass;
        String leftGlassAs;
        /** The far compile numbers the reports while away carried, with how many reports each. */
        final Map<Integer, Integer> awayCompiles = new TreeMap<>();
        String awaySummary = "no report while away";
        /** Why the far copy replayed the way it did: the engine's notes and the report's model. */
        String why = "not read";
        int awayReports;
        int prefix = -1;
        /** The far report's completed windows when last polled before eligibility. */
        long windowsSeen = -1;
        /** The far compile at eligibility. */
        GlassKit.Graph graph;
        /** Per good: what the far machines' held inputs at the snapshot make at the recipe yields (G2-25). */
        final Map<String, Double> inputYield1 = new TreeMap<>();
        /** Per good: the held inputs {@link #inputYield1} counts, summed over the machines. */
        final Map<String, Map<String, Long>> inputFrom1 = new TreeMap<>();
        /** Stub: the offline reconcile applied after the reload, per good, as sent and as applied (null: not yet). */
        Map<String, Long> offlinePlanned, offlineApplied;
        long nextOfflineAt = -1;
        /** Both copies' declared stores every {@link #POLL} ticks while loaded, back to {@link #WINDOW} before now. */
        final ArrayDeque<Sample> samples = new ArrayDeque<>();
        /** The first of them, soon after the build, for the copy check's levels. */
        Sample firstSample;
        /** Why the far copy's last window before the unload does not match the control's (null: it does). */
        String invalidCopy;
        /** The box the far copy's place is laid out in (before the partial variant's shift, which it covers). */
        BlockPos areaMin, areaMax;
        /** The warm-up check ran (once per build), and how often both copies were built again after it. */
        boolean warmChecked;
        int rebuilds;
        final List<String> failures = new ArrayList<>();
        String last = "starting";

        Run(GameTestHelper helper, CorpusFactory f, int slot, boolean partial, boolean straddle) {
            this.helper = helper;
            this.f = f;
            this.partial = partial || straddle;
            this.straddle = straddle;
            this.far = new Far(helper, farOffset(slot));
            this.test = "glass replay " + (straddle ? "straddle " : partial ? "partial " : "") + f.name();
            this.rec = GlassExitKit.start(helper.getLevel());
            this.judge = GlassExitKit.canJudge();
            this.stub = !IndustryKit.nativeEngine();
            farMin = far.absolutePos(BlockPos.ZERO);
            farMax = far.absolutePos(new BlockPos(33, TOP, 33));
            // The partial variant shifts the far copy by up to 15 blocks in x and z: its chunks cover that too.
            BlockPos reach = this.partial ? farMax.offset(15, 0, 15) : farMax;
            for (int cx = farMin.getX() >> 4; cx <= reach.getX() >> 4; cx++)
                for (int cz = farMin.getZ() >> 4; cz <= reach.getZ() >> 4; cz++)
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

    static void start(GameTestHelper helper, CorpusFactory f, int slot, boolean partial, boolean straddle) {
        GlassConfig.Test.set(GlassConfig.CHECKSUM_WINDOW_TICKS, WINDOW);
        IndustryKit.fastConfig();
        Run run = new Run(helper, f, slot, partial, straddle);
        IndustryKit.record(run.test, "far copy", "offset " + run.far.offset.toShortString() + ", box " + run.farMin.toShortString() + " to "
                + run.farMax.toShortString() + ", chunks " + run.farChunks + ", engine " + IndustryKit.engine().describe()
                + (run.judge ? "" : ", NOT judged (no natives)"));
        for (ChunkPos c : run.farChunks)
            run.level().setChunkForced(c.x, c.z, true);
        // However the test ends, the far copy stops ticking: later batches must not run it.
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

    /** Drops every ticket the test gave the far copy's chunks. */
    static void release(Run run) {
        ServerLevel level = run.level();
        for (ChunkPos c : run.farChunks) {
            level.setChunkForced(c.x, c.z, false);
            level.getChunkSource().removeRegionTicket(FULL_ONLY, c, 0, c);
            level.getChunkSource().removeRegionTicket(BLOCK_TICKING, c, 1, c);
        }
    }

    /** A failed wait on chunk IO: the tick sleeps 5 ms so the chunk system gets time (ReconcileAheadGameTests.io). */
    static void ioWait() {
        try {
            Thread.sleep(5);
        } catch (InterruptedException ie) {
            Thread.currentThread().interrupt();
        }
    }

    static void tick(Run run) {
        if (run.stage == Stage.DONE)
            return;
        run.ticks++;
        run.stageTicks++;
        if (run.ticks % POLL == 0)
            run.rec.poll();
        switch (run.stage) {
            case LOAD_FAR -> loadFar(run);
            case RUN -> runLoaded(run);
            case UNLOAD -> unload(run);
            case AWAY -> away(run);
            case RELOAD -> reload(run);
            case SETTLE -> settle(run);
            default -> {}
        }
    }

    // ---- stages -------------------------------------------------------------------------------

    static void loadFar(Run run) {
        ServerLevel level = run.level();
        for (ChunkPos c : run.farChunks)
            if (level.getChunkSource().getChunkNow(c.x, c.z) == null || !Districts.blockTicking(level, c.toLong())) {
                run.last = "far chunk " + c + " loading";
                ioWait();
                return;
            }
        IndustryKit.record(run.test, "far chunks", "loaded and block-ticking after " + run.ticks + " ticks");
        run.areaMin = run.farMin;
        run.areaMax = run.partial ? run.farMax.offset(15, 0, 15) : run.farMax;
        build(run);
    }

    /**
     * Forgets the industry of both places (the world is reused between runs), lays the far copy's place out
     * as the framework lays out the yard, and builds both in this tick. Again after a dead warm-up: the
     * control's yard is laid out afresh too, so both copies start together.
     */
    static void build(Run run) {
        ServerLevel level = run.level();
        IndustryKit.forget(run.helper);
        ChunkPos lo = run.farChunks.get(0), hi = run.farChunks.get(run.farChunks.size() - 1);
        IndustryKit.ind().districts().forgetArea(level, lo.x - 1, lo.z - 1, hi.x + 1, hi.z + 1);
        if (run.rebuilds > 0)
            clearYard(run);
        clear(level, run.areaMin, run.areaMax, run.farMin);
        run.control = run.f.build(run.helper);
        seed(run.helper, run.control);
        if (run.partial && run.rebuilds == 0) {
            split(run);
            // The floor moves with the shift: lay the place out again around the copy's own origin.
            clear(level, run.areaMin, run.areaMax, run.farMin);
        }
        run.copy = run.f.build(run.far);
        seed(run.far, run.copy);
        run.builtAt = run.now();
        GlassCorpusKit.log(run.f.name(), run.rebuilds > 0 ? "replay rebuilt" : "replay built", "control: " + run.control.notes() + "; far: " + run.copy.notes());
        run.stage(Stage.RUN);
    }

    /** The control's yard as the framework left it: the floor, air above (its structure block, below, stays). */
    static void clearYard(Run run) {
        ServerLevel level = run.level();
        BlockPos o = run.helper.absolutePos(BlockPos.ZERO);
        BlockState floor = Blocks.POLISHED_ANDESITE.defaultBlockState(), air = Blocks.AIR.defaultBlockState();
        int flags = Block.UPDATE_CLIENTS | Block.UPDATE_KNOWN_SHAPE;
        BlockPos lo = o.offset(0, 1, 0), hi = o.offset(YARD_SIZE - 1, TOP, YARD_SIZE - 1);
        for (BlockPos p : BlockPos.betweenClosed(lo, hi)) {
            BlockState want = p.getY() == o.getY() + 1 ? floor : air;
            if (level.getBlockState(p) != want)
                level.setBlock(p, want, flags);
        }
        AABB box = new AABB(Vec3.atLowerCornerOf(lo), Vec3.atLowerCornerOf(hi).add(1, 1, 1)).inflate(2);
        for (Entity e : level.getEntities((Entity) null, box, e -> !(e instanceof Player)))
            e.discard();
    }

    /**
     * The warm-up check, once one {@link #WINDOW} of samples is in: the copy check's window test from the
     * first sample to now (the control's report so far for the noise and the batch, no compile cap). A far
     * copy that does not move its goods as the control does is dead (stub m5's press belt, stub m6's
     * precision mechanisms): both are built again once; dead again, the run fails as INVALID COPY.
     * Returns true when it rebuilt or failed (the caller stops for this poll).
     */
    static boolean warmUpCheck(Run run) {
        Sample from = run.firstSample;
        if (run.warmChecked || from == null || run.now() - from.t() < WINDOW)
            return false;
        run.warmChecked = true;
        ServerLevel level = run.level();
        long len = run.now() - from.t();
        GlassReport cr = report(run, run.cd);
        Map<String, Long> cNow = stock(level, run.control.stores()), fNow = stock(level, run.copy.stores());
        Map<String, GlassReport.Good> cGoods = byKey(cr);
        TreeSet<String> goods = new TreeSet<>(from.control().keySet());
        goods.addAll(from.far().keySet());
        goods.addAll(cNow.keySet());
        goods.addAll(fNow.keySet());
        List<String> off = new ArrayList<>();
        for (String g : goods) {
            long c = cNow.getOrDefault(g, 0L) - from.control().getOrDefault(g, 0L);
            long f = fNow.getOrDefault(g, 0L) - from.far().getOrDefault(g, 0L);
            GlassReport.Good cg = cGoods.get(g);
            long stack = stackOf(g);
            double inFlight = Math.max(stack, batch(null, g, cg, cr, stack).units());
            double sigma = cg != null && Double.isFinite(cg.sigma()) ? cg.sigma() : 0;
            double noise = CHECKSUM_Z * sigma * len, rel = REL_TOL * Math.abs(c);
            double tol = inFlight + noise + rel;
            boolean ok = Math.abs(f - c) <= tol;
            IndustryKit.record(run.test, "warm-up check good", String.format(Locale.ROOT,
                    "%s: control %+d, far %+d over %d ticks from %d; tolerance %.1f (in flight %.1f, noise %.1f, relative %.1f): %s",
                    g, c, f, len, from.t(), tol, inFlight, noise, rel, ok ? "match" : "OFF"));
            if (!ok)
                off.add(g + " (control " + c + ", far " + f + ")");
        }
        if (off.isEmpty()) {
            IndustryKit.record(run.test, "warm-up check", "match over " + len + " ticks from " + from.t() + " (" + goods.size() + " goods)"
                    + (run.rebuilds > 0 ? ", after " + run.rebuilds + " rebuild" : ""));
            return false;
        }
        String what = "over " + len + " ticks from " + from.t() + " the far copy's stores do not move as the control's: " + off
                + "; far kinetics " + kinetics(level, run.farMin, run.farMax) + "; control kinetics "
                + kinetics(level, run.helper.absolutePos(BlockPos.ZERO), run.helper.absolutePos(new BlockPos(YARD_SIZE - 1, TOP, YARD_SIZE - 1)));
        if (run.rebuilds > 0) {
            run.invalidCopy = "dead at warm-up after a rebuild: " + what;
            fail(run, "INVALID COPY: the far copy was dead at warm-up twice (built again once): " + what);
            return true;
        }
        IndustryKit.record(run.test, "far copy dead at warm-up, rebuilt", what);
        run.rebuilds++;
        run.cd = run.fd = -1;
        run.samples.clear();
        run.firstSample = null;
        run.warmChecked = false;
        run.windowsSeen = -1;
        build(run);
        return true;
    }

    /**
     * A copy's Create kinetics in a box, for a dead warm-up: kinetic block entities (still, with no network,
     * overstressed), belts (still), and each source with its speed and whether its chunk ticks blocks.
     */
    static String kinetics(ServerLevel level, BlockPos min, BlockPos max) {
        int n = 0, still = 0, noNet = 0, over = 0, belts = 0, beltsStill = 0;
        List<String> sources = new ArrayList<>();
        for (BlockPos p : BlockPos.betweenClosed(min, max)) {
            if (level.getChunkSource().getChunkNow(p.getX() >> 4, p.getZ() >> 4) == null
                    || !(level.getBlockEntity(p) instanceof KineticBlockEntity k))
                continue;
            n++;
            if (k.getSpeed() == 0)
                still++;
            if (k.network == null)
                noNet++;
            if (k.isOverStressed())
                over++;
            if (k instanceof BeltBlockEntity) {
                belts++;
                if (k.getSpeed() == 0)
                    beltsStill++;
            }
            if (k instanceof GeneratingKineticBlockEntity g)
                sources.add(p.toShortString() + " generated " + g.getGeneratedSpeed() + " speed " + g.getSpeed()
                        + (Districts.blockTicking(level, ChunkPos.asLong(p)) ? ", chunk ticking" : ", chunk NOT ticking"));
        }
        return String.format(Locale.ROOT, "{%d kinetic (%d still, %d no network, %d overstressed), %d belts (%d still), sources %s}",
                n, still, noNet, over, belts, beltsStill, sources);
    }

    /**
     * The partial variant: shifts the far copy (0 to 15 blocks in x and in z) so that its place against the
     * chunk grid does not depend on where the yard landed. Along the axis its stores and anchor spread most a
     * chunk border runs between them; along the other the control's whole build sits inside one chunk,
     * centred (so the far district has the two chunks m5 ran, the anchor's ticking and the other not). The
     * straddle smoke puts a border through the middle of the build along the other axis too: a chunk
     * corner, three or four chunks, the ticking one with FULL-only neighbours (G2-63).
     */
    static void split(Run run) {
        List<BlockPos> ps = new ArrayList<>(run.control.stores());
        ps.add(run.control.anchor());
        int minX = Integer.MAX_VALUE, maxX = Integer.MIN_VALUE, minZ = Integer.MAX_VALUE, maxZ = Integer.MIN_VALUE;
        for (BlockPos p : ps) {
            minX = Math.min(minX, p.getX());
            maxX = Math.max(maxX, p.getX());
            minZ = Math.min(minZ, p.getZ());
            maxZ = Math.max(maxZ, p.getZ());
        }
        boolean alongX = maxX - minX >= maxZ - minZ;
        int min = alongX ? minX : minZ, max = alongX ? maxX : maxZ;
        // The first column of the second chunk, in the control's coordinates: past the middle, never past max.
        int border = min + Math.max(1, (max - min + 1) / 2);
        int shift = Math.floorMod(-border, 16);
        // The other axis: the control's built blocks, not only its stores (a machine past them shares their chunk).
        int[] fp = footprint(run);
        int lo = alongX ? fp[2] : fp[0], hi = alongX ? fp[3] : fp[1], width = hi - lo + 1;
        int across;
        String acrossWhy;
        if (run.straddle) {
            int oBorder = lo + Math.max(1, width / 2);
            across = Math.floorMod(-oBorder, 16);
            acrossWhy = "a chunk border before control " + (alongX ? "z " : "x ") + oBorder + " too (a corner)";
        } else if (width <= 16) {
            across = Math.floorMod((16 - width) / 2 - lo, 16);
            acrossWhy = "the build inside one chunk, centred";
        } else {
            across = Math.floorMod(-lo, 16);
            acrossWhy = "the build from a chunk's first column (" + width + " wide: it cannot fit one chunk)";
        }
        BlockPos by = alongX ? new BlockPos(shift, 0, across) : new BlockPos(across, 0, shift);
        run.far.offset = run.far.offset.offset(by);
        run.farMin = run.farMin.offset(by);
        run.farMax = run.farMax.offset(by);
        // Where the far build lands on the chunk grid (the far offset is whole chunks plus this shift).
        BlockPos d = run.far.offset;
        int cx0 = (fp[0] + d.getX()) >> 4, cx1 = (fp[1] + d.getX()) >> 4, cz0 = (fp[2] + d.getZ()) >> 4, cz1 = (fp[3] + d.getZ()) >> 4;
        IndustryKit.record(run.test, "split", "stores and anchor span " + (alongX ? "x " : "z ") + min + " to " + max + " in the control; the far copy moves "
                + shift + " along " + (alongX ? "x" : "z") + " so a chunk border falls before control " + (alongX ? "x " : "z ") + border
                + (max == min ? " (they do not spread: no border between them)" : "") + ", and " + across + " along " + (alongX ? "z" : "x") + " for "
                + acrossWhy + "; the build spans x " + fp[0] + " to " + fp[1] + ", z " + fp[2] + " to " + fp[3] + " in the control, far chunks x "
                + cx0 + " to " + cx1 + ", z " + cz0 + " to " + cz1 + " (" + (cx1 - cx0 + 1) * (cz1 - cz0 + 1) + " in its box)");
    }

    /** The control's built blocks in its yard (above the floor, or in it where they replaced it): min x, max x, min z, max z. */
    static int[] footprint(Run run) {
        ServerLevel level = run.level();
        BlockPos o = run.helper.absolutePos(BlockPos.ZERO);
        int[] fp = {Integer.MAX_VALUE, Integer.MIN_VALUE, Integer.MAX_VALUE, Integer.MIN_VALUE};
        for (BlockPos p : BlockPos.betweenClosed(o.offset(0, 1, 0), o.offset(YARD_SIZE - 1, TOP, YARD_SIZE - 1))) {
            BlockState st = level.getBlockState(p);
            if (st.isAir() || (p.getY() == o.getY() + 1 && st.is(Blocks.POLISHED_ANDESITE)))
                continue;
            fp[0] = Math.min(fp[0], p.getX());
            fp[1] = Math.max(fp[1], p.getX());
            fp[2] = Math.min(fp[2], p.getZ());
            fp[3] = Math.max(fp[3], p.getZ());
        }
        if (fp[0] > fp[1]) {
            // Nothing built (cannot happen for a corpus factory): the stores' anchor alone.
            BlockPos a = run.control.anchor();
            fp = new int[] {a.getX(), a.getX(), a.getZ(), a.getZ()};
        }
        return fp;
    }

    static void seed(GameTestHelper h, CorpusFactory.Built b) {
        for (BlockPos p : b.stores())
            IndustryKit.playerPlaced(h, p);
        IndustryKit.playerPlaced(h, b.anchor());
    }

    /** Both copies run loaded until the far copy may be replayed (G-B4), then both are snapshot and the far copy leaves. */
    static void runLoaded(Run run) {
        if (run.stageTicks % POLL != 0)
            return;
        if (run.cd < 0 || run.fd < 0) {
            District c = IndustryKit.districtAt(run.helper, run.control.anchor());
            District d = IndustryKit.districtAt(run.helper, run.copy.anchor());
            if (c == null || d == null) {
                run.last = "districts: control " + c + ", far " + d;
                return;
            }
            if (c == d) {
                run.failures.add("the control and the far copy are one district " + c);
                run.stage = Stage.DONE;
                return;
            }
            run.cd = c.id;
            run.fd = d.id;
            IndustryKit.record(run.test, "districts", "control " + c + " (" + c.chunks.size() + " chunks, " + c.inventories.size()
                    + " inventories), far " + d + " (" + d.chunks.size() + " chunks, " + d.inventories.size() + " inventories)");
        }
        sample(run);
        if (run.judge && warmUpCheck(run))
            return;
        int budget = run.f.warmupTicks() + 2 * (WINDOW + GlassExitGameTests.SLACK);
        long since = run.now() - run.builtAt;
        GlassReport fr = null;
        if (!run.judge) {
            // -PskipJni: nothing can say when the replay is eligible; run the warm-up and a window.
            if (since < run.f.warmupTicks() + WINDOW + 1000)
                return;
            run.eligibleBy = "not judged: warm-up and one window";
        } else {
            if (run.stub && run.stageTicks % STUB_REPORT_POLL != 0 && since < budget)
                return;
            fr = report(run, run.fd);
            run.last = fr == null ? "far copy: no complete compile" : "far copy: " + fr.summary() + "; " + fr.replay().summary();
            if (fr != null && fr.windowsCompleted() != run.windowsSeen) {
                // A window that does not pass keeps the copy from eligibility: log what it measured, per good.
                run.windowsSeen = fr.windowsCompleted();
                if (fr.windowsCompleted() > 0 && fr.verdict() != GlassReport.PASS)
                    logReport(run, "far window " + fr.windowsCompleted() + " not a pass", fr);
            }
            if (fr != null && fr.checksum().acceptedAt() > run.now() + WINDOW) {
                // The GameTest server starts every run at game time 0 while the world's rtsim pages keep the
                // engine's clock: no window opens until the game catches up. Not the glass box's fault.
                fail(run, "the engine's clock is ahead of the game (compile accepted at " + fr.checksum().acceptedAt() + ", game time " + run.now()
                        + "): a reused GameTest world; delete run-gametest/world");
                return;
            }
            if (run.stageTicks % 1200 == 0)
                IndustryKit.record(run.test, "status", run.last);
            boolean passed = fr != null && fr.windowsCompleted() > 0 && fr.verdict() == GlassReport.PASS;
            if (fr != null && fr.replay().eligible()) {
                run.eligible = true;
                run.eligibleBy = "the report says eligible" + (passed ? "" : " (last verdict not a pass)");
            } else if (passed) {
                run.eligible = true;
                run.eligibleBy = "a completed passing window; the report does NOT say eligible";
            } else if (since < budget) {
                return;
            } else {
                run.eligibleBy = "NEVER eligible in " + since + " ticks";
                run.failures.add(run.f.name() + ": the far copy never became eligible (" + run.last + ")");
            }
        }
        IndustryKit.record(run.test, "eligible", run.eligibleBy + " at " + run.now() + ", " + since + " ticks after the build");
        dumpCompile(run);
        snapshot1(run, fr);
        leave(run);
    }

    static void snapshot1(Run run, GlassReport fr) {
        ServerLevel level = run.level();
        run.t1 = run.now();
        run.heldBase = IndustryKit.ind().reconciler().held();
        run.cPos = positions(run.control, IndustryKit.ind().district(run.cd));
        run.fPos = positions(run.copy, IndustryKit.ind().district(run.fd));
        run.c1 = stock(level, run.cPos);
        run.f1 = stock(level, run.fPos);
        run.wip1 = wip(level, run.farMin, run.farMax, run.fPos);
        inputYield(run, level);
        run.fReport1 = fr;
        run.cReport1 = run.judge ? report(run, run.cd) : null;
        IndustryKit.record(run.test, "snapshot 1", "at " + run.t1 + ": control " + run.c1 + " over " + run.cPos.size() + " stores; far "
                + run.f1 + " over " + run.fPos.size() + " stores; far WIP " + run.wip1);
        IndustryKit.record(run.test, "input WIP yield", run.inputYield1.isEmpty() ? "none: no far machine held a recipe's inputs"
                : run.inputYield1.entrySet().stream().map(e -> String.format(Locale.ROOT, "%s %.1f from %s", e.getKey(), e.getValue(),
                        run.inputFrom1.get(e.getKey()))).collect(Collectors.joining("; ")));
        if (run.cPos.size() != run.fPos.size())
            IndustryKit.record(run.test, "WARNING", "the copies' stores differ: control " + run.cPos.size() + ", far " + run.fPos.size());
        if (run.cReport1 != null)
            logReport(run, "control report 1", run.cReport1);
        if (fr != null)
            logReport(run, "far report 1", fr);
        copyCheck(run, level);
    }

    /** Both copies' declared stores now, keeping the latest sample at or before {@link #WINDOW} ticks ago. */
    static void sample(Run run) {
        ServerLevel level = run.level();
        run.samples.addLast(new Sample(run.now(), stock(level, run.control.stores()), stock(level, run.copy.stores())));
        if (run.firstSample == null)
            run.firstSample = run.samples.peekLast();
        while (run.samples.size() > 1 && run.samples.stream().skip(1).findFirst().get().t() <= run.now() - WINDOW)
            run.samples.removeFirst();
    }

    /**
     * The copy check: over the far copy's last window before the unload ({@link #WINDOW} ticks, from the
     * samples), each good's change in its declared stores is within tolerance of the control's: one stack or
     * batch in flight, the checksum noise over the window, and {@link #REL_TOL} of the control's change.
     * Otherwise the far copy is not the factory the control is (a block that did not survive, a store that
     * filled), and every row reads INVALID COPY. The same for each good's level in those stores at the
     * snapshot, with the noise over the ticks since the first sample and the relative term of the control's
     * change since then: a copy that stalled for a while and caught up holds what the control held earlier.
     */
    static void copyCheck(Run run, ServerLevel level) {
        Sample from = run.samples.peekFirst();
        if (from == null || from.t() >= run.t1) {
            run.invalidCopy = "no store sample before the snapshot";
            IndustryKit.record(run.test, "copy check", "INVALID COPY: " + run.invalidCopy);
            return;
        }
        long len = run.t1 - from.t();
        Map<String, Long> cNow = stock(level, run.control.stores()), fNow = stock(level, run.copy.stores());
        Map<String, GlassReport.Good> cGoods = byKey(run.cReport1);
        TreeSet<String> goods = new TreeSet<>(from.control().keySet());
        goods.addAll(from.far().keySet());
        goods.addAll(cNow.keySet());
        goods.addAll(fNow.keySet());
        Sample first = run.firstSample;
        long since = first == null ? 0 : run.t1 - first.t();
        List<String> off = new ArrayList<>(), apart = new ArrayList<>();
        for (String g : goods) {
            long c = cNow.getOrDefault(g, 0L) - from.control().getOrDefault(g, 0L);
            long f = fNow.getOrDefault(g, 0L) - from.far().getOrDefault(g, 0L);
            GlassReport.Good cg = cGoods.get(g);
            long stack = stackOf(g);
            double batch = batch(run.graph, g, cg, run.cReport1, stack).units();
            double inFlight = Math.max(stack, batch);
            double sigma = cg != null && Double.isFinite(cg.sigma()) ? cg.sigma() : 0;
            double noise = CHECKSUM_Z * sigma * len;
            double rel = REL_TOL * Math.abs(c);
            double tol = inFlight + noise + rel;
            boolean ok = Math.abs(f - c) <= tol;
            IndustryKit.record(run.test, "copy check good", String.format(Locale.ROOT,
                    "%s: control %+d, far %+d over %d ticks from %d; tolerance %.1f (in flight %.1f, noise %.1f, relative %.1f): %s",
                    g, c, f, len, from.t(), tol, inFlight, noise, rel, ok ? "match" : "OFF"));
            if (!ok)
                off.add(g + " (control " + c + ", far " + f + ")");
            // The level: a copy that stalled and caught up moves alike in the last window but holds what the
            // control held some time ago, so a store runs dry or fills at another point of the span.
            if (first != null) {
                long cl = cNow.getOrDefault(g, 0L), fl = fNow.getOrDefault(g, 0L);
                long cSince = cl - first.control().getOrDefault(g, 0L);
                double lNoise = CHECKSUM_Z * sigma * since, lRel = REL_TOL * Math.abs(cSince);
                double lTol = inFlight + lNoise + lRel;
                boolean lOk = Math.abs(fl - cl) <= lTol;
                IndustryKit.record(run.test, "copy check level", String.format(Locale.ROOT,
                        "%s: control %d, far %d at the snapshot (control %+d since %d); tolerance %.1f (in flight %.1f, noise %.1f, relative %.1f): %s",
                        g, cl, fl, cSince, first.t(), lTol, inFlight, lNoise, lRel, lOk ? "match" : "APART"));
                if (!lOk)
                    apart.add(g + " (control " + cl + ", far " + fl + ")");
            }
        }
        List<String> why = new ArrayList<>();
        if (!off.isEmpty())
            why.add("over the last " + len + " ticks before the unload the far copy's stores do not move as the control's: " + off);
        if (!apart.isEmpty())
            why.add("at the snapshot the far copy's stores hold other amounts than the control's: " + apart);
        if (!why.isEmpty())
            run.invalidCopy = String.join("; ", why);
        IndustryKit.record(run.test, "copy check", run.invalidCopy == null
                ? "match over the last " + len + " ticks before the unload (" + goods.size() + " goods)" + (len < WINDOW ? ", SHORT of " + WINDOW : "")
                : "INVALID COPY: " + run.invalidCopy);
    }

    /** The far copy's chunks lose their tickets (the partial variant: all but one stop ticking blocks). */
    static void leave(Run run) {
        ServerLevel level = run.level();
        if (run.partial) {
            run.ticking = new ChunkPos(run.copy.anchor());
            District d = IndustryKit.ind().district(run.fd);
            if (d == null || d.chunks.size() < 2) {
                // One chunk cannot be part loaded: nothing for this variant to judge where the yard landed.
                IndustryKit.record(run.test, "SKIPPED", "the far district has " + (d == null ? "no" : d.chunks.size()) + " chunk(s): no partial load");
                run.stage(Stage.DONE);
                return;
            }
            if (run.straddle && d.chunks.size() < 3) {
                // Two chunks are the partial variant's case: the smoke would pass without the corner it is for.
                fail(run, "the straddle copy's far district has " + d.chunks.size() + " chunks " + longs(d.chunks) + ": no chunk corner");
                return;
            }
            for (ChunkPos c : run.farChunks) {
                if (c.equals(run.ticking))
                    level.getChunkSource().addRegionTicket(BLOCK_TICKING, c, 1, c);
                else
                    level.getChunkSource().addRegionTicket(FULL_ONLY, c, 0, c);
                level.setChunkForced(c.x, c.z, false);
            }
            IndustryKit.record(run.test, "partial", "far chunk " + run.ticking + " keeps ticking blocks; " + (run.farChunks.size() - 1)
                    + " FULL only; far district chunks " + longs(IndustryKit.ind().district(run.fd).chunks));
        } else {
            for (ChunkPos c : run.farChunks)
                level.setChunkForced(c.x, c.z, false);
        }
        run.stage(Stage.UNLOAD);
    }

    static void unload(Run run) {
        ServerLevel level = run.level();
        District d = IndustryKit.ind().district(run.fd);
        if (run.partial) {
            boolean ok = d != null && d.loadState() == IndRecords.LoadState.PARTIAL;
            for (ChunkPos c : run.farChunks) {
                boolean loaded = level.getChunkSource().getChunkNow(c.x, c.z) != null;
                boolean ticks = Districts.blockTicking(level, c.toLong());
                ok &= loaded && ticks == c.equals(run.ticking);
            }
            if (!ok) {
                run.last = "far copy going partial: load state " + (d == null ? "no district" : d.loadState());
                if (run.stageTicks > IO_TICKS)
                    fail(run, "the far copy never went partial (" + run.last + ")");
                return;
            }
        } else {
            for (ChunkPos c : run.farChunks)
                if (level.getChunkSource().getChunkNow(c.x, c.z) != null) {
                    run.last = "far chunk " + c + " unloading";
                    ioWait();
                    return;
                }
            if (d == null || IndustryKit.ind().districts().loadedChunks(d) != 0) {
                run.last = "far district still has loaded chunks: " + (d == null ? "no district" : IndustryKit.ind().districts().loadedChunks(d));
                ioWait();
                return;
            }
        }
        if (run.unloadedAt < 0) {
            run.unloadedAt = run.now();
            IndustryKit.record(run.test, run.partial ? "partial (Java)" : "unloaded (Java)", "at " + run.unloadedAt + ", " + (run.unloadedAt - run.t1)
                    + " ticks after the snapshot; Java load state " + (d == null ? "?" : d.loadState()));
        }
        // The engine must have seen it: the district's load state left LIVE, and every district chunk's
        // IND_CHUNK_UNLOADED (the partial variant: IND_CHUNK_TICKING off for every chunk but the ticking one).
        if (run.stageTicks % 10 != 0)
            return;
        List<ScriptedEngine.Received> recs = run.rec.records();
        int want = d == null ? 0 : run.partial ? d.chunks.size() - (d.chunks.contains(run.ticking.toLong()) ? 1 : 0) : d.chunks.size();
        int seen = 0;
        if (run.partial) {
            for (ScriptedEngine.Received r : of(recs, GlassRecords.IND_CHUNK_TICKING, GlassRecords.ChunkTicking.DISTRICT, run.fd, run.t1))
                if (r.aux() == 0)
                    seen++;
        } else {
            seen = count(recs, IndRecords.IND_CHUNK_UNLOADED, IndRecords.ChunkUnloaded.DISTRICT, run.fd, run.t1);
        }
        int wantState = run.partial ? IndRecords.LoadState.PARTIAL : IndRecords.LoadState.UNLOADED;
        boolean state = false;
        for (ScriptedEngine.Received r : of(recs, IndRecords.IND_LOAD_STATE, IndRecords.LoadState.DISTRICT, run.fd, run.t1))
            state |= r.aux() == wantState;
        if (seen < want || !state) {
            run.last = "the engine has seen " + seen + " of " + want + (run.partial ? " IND_CHUNK_TICKING off" : " IND_CHUNK_UNLOADED") + ", load states "
                    + loadStates(recs, run);
            if (run.stageTicks > IO_TICKS)
                fail(run, "the engine never saw the far copy leave (" + run.last + ")");
            return;
        }
        IndustryKit.record(run.test, run.partial ? "partial" : "unloaded", "the engine saw it by " + run.now() + ": " + loadRecords(run, run.t1));
        run.stage(Stage.AWAY);
    }

    static void away(Run run) {
        long due = Math.max(run.t1 + SPAN, run.unloadedAt + MIN_AWAY);
        boolean last = run.now() >= due;
        if (run.judge && !run.stub && (run.stageTicks % POLL == 0 || last)) {
            GlassReport r = report(run, run.fd);
            if (r != null) {
                run.away = r;
                run.awayReports++;
                boolean glass = r.replay().model() == GlassReport.REPLAY_GLASS;
                if (run.sawGlass && !glass) {
                    run.leftGlass++;
                    run.leftGlassAs = r.replay().summary();
                }
                run.sawGlass |= glass;
                run.awayCompiles.merge(r.compile(), 1, Integer::sum);
                run.awaySummary = r.replay().summary();
                if (run.stageTicks % 1200 == 0)
                    IndustryKit.record(run.test, "away", r.replay().summary() + "; " + r.summary());
            }
        }
        if (!last)
            return;
        run.rec.poll();
        run.prefix = run.rec.size();
        if (run.judge && run.stub) {
            run.away = GlassReport.decode(NativeEngine.glassOffline(run.rec.stream(run.prefix), run.fd));
            run.awayReports = run.away == null ? 0 : 1;
            run.sawGlass = run.away != null && run.away.replay().model() == GlassReport.REPLAY_GLASS;
            run.awaySummary = run.away == null ? "no complete compile offline" : run.away.replay().summary();
            if (run.away != null)
                run.awayCompiles.merge(run.away.compile(), 1, Integer::sum);
        }
        IndustryKit.record(run.test, "replay while away", run.awaySummary + " (" + run.awayReports + " reports"
                + (run.stub ? ", offline over " + run.prefix + " records" : "") + ")");
        if (run.away != null)
            logReport(run, "far report away", run.away);
        ServerLevel level = run.level();
        run.forcedAt = run.now();
        for (ChunkPos c : run.farChunks) {
            level.setChunkForced(c.x, c.z, true);
            if (run.partial) {
                if (c.equals(run.ticking))
                    level.getChunkSource().removeRegionTicket(BLOCK_TICKING, c, 1, c);
                else
                    level.getChunkSource().removeRegionTicket(FULL_ONLY, c, 0, c);
            }
        }
        run.stage(Stage.RELOAD);
    }

    static void reload(Run run) {
        ServerLevel level = run.level();
        for (ChunkPos c : run.farChunks)
            if (level.getChunkSource().getChunkNow(c.x, c.z) == null || !Districts.blockTicking(level, c.toLong())) {
                run.last = "far chunk " + c + " reloading";
                ioWait();
                return;
            }
        District d = IndustryKit.ind().district(run.fd);
        if (d == null || d.loadState() != IndRecords.LoadState.LIVE) {
            run.last = "far district " + (d == null ? "gone" : "load state " + d.loadState());
            if (run.stageTicks > IO_TICKS)
                fail(run, "the far copy never came back live (" + run.last + ")");
            return;
        }
        run.loadedAt = run.now();
        IndustryKit.record(run.test, "reloaded", "at " + run.loadedAt + ", " + (run.loadedAt - run.forcedAt) + " ticks after the tickets");
        run.stage(Stage.SETTLE);
    }

    static void settle(Run run) {
        ServerLevel level = run.level();
        if (run.stub && run.judge && run.offlineApplied == null) {
            if (run.now() < run.nextOfflineAt)
                return;
            run.nextOfflineAt = run.now() + 10;
            if (!applyOffline(run)) {
                run.last = "stub: no offline reconcile yet, " + (run.now() - run.loadedAt) + " ticks after the reload";
                if (run.now() - run.loadedAt < OFFLINE_WAIT)
                    return;
                run.offlinePlanned = Map.of();
                run.offlineApplied = Map.of();
                IndustryKit.record(run.test, "offline reconcile applied", "none: the offline engine sent the far district no reconcile by "
                        + run.now() + ", " + (run.now() - run.loadedAt) + " ticks after the reload");
            }
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
        if (run.now() - run.loadedAt < SETTLE || run.quietSince < 0 || run.now() - run.quietSince < SETTLE / 2) {
            run.last = "settling: held " + IndustryKit.ind().reconciler().held() + (busy ? ", busy" : ", quiet since " + run.quietSince);
            if (run.stageTicks > 4 * SETTLE + 2000)
                fail(run, "the far copy never settled (" + run.last + ")");
            return;
        }
        run.rec.poll();
        run.t2 = run.now();
        judge(run);
        run.stage(Stage.DONE);
    }

    static void fail(Run run, String why) {
        run.failures.add(run.f.name() + ": " + why);
        IndustryKit.record(run.test, "FAIL", why);
        run.stage(Stage.DONE);
    }

    // ---- judging ------------------------------------------------------------------------------

    static void judge(Run run) {
        ServerLevel level = run.level();
        String test = run.test;
        long span = run.t2 - run.t1;
        Map<String, Long> c2 = stock(level, run.cPos), f2 = stock(level, run.fPos);
        GlassReport cReport2 = run.judge ? report(run, run.cd) : null;
        GlassReport fReport2 = run.judge ? report(run, run.fd) : null;
        IndustryKit.record(test, "snapshot 2", "at " + run.t2 + " (span " + span + "): control " + c2 + "; far " + f2);
        if (cReport2 != null)
            logReport(run, "control report 2", cReport2);
        if (fReport2 != null)
            logReport(run, "far report 2", fReport2);

        // What the engine saw and sent: loads, unloads, ticking, reconcile results.
        List<ScriptedEngine.Received> recs = run.rec.records();
        int unloads = count(recs, IndRecords.IND_CHUNK_UNLOADED, IndRecords.ChunkUnloaded.DISTRICT, run.fd, run.t1);
        int loads = count(recs, IndRecords.IND_CHUNK_LOADED, IndRecords.ChunkLoaded.DISTRICT, run.fd, run.forcedAt);
        int tickingOff = 0, tickingOn = 0;
        for (ScriptedEngine.Received r : of(recs, GlassRecords.IND_CHUNK_TICKING, GlassRecords.ChunkTicking.DISTRICT, run.fd, run.t1)) {
            if (r.aux() == 0)
                tickingOff++;
            else
                tickingOn++;
        }
        IndustryKit.record(test, "engine saw", unloads + " IND_CHUNK_UNLOADED, " + loads + " IND_CHUNK_LOADED (epochs "
                + epochs(recs, run) + "), IND_CHUNK_TICKING " + tickingOff + " off / " + tickingOn + " on, load states " + loadStates(recs, run)
                + "; recorder kept " + recs.size() + " records, dropped " + run.rec.dropped());
        District fd = IndustryKit.ind().district(run.fd);
        int districtChunks = fd == null ? -1 : fd.chunks.size();
        if (!run.partial && unloads < districtChunks)
            run.failures.add(run.f.name() + ": the engine saw " + unloads + " IND_CHUNK_UNLOADED of the far district's " + districtChunks + " chunks");
        if (run.partial && tickingOff == 0)
            run.failures.add(run.f.name() + ": the engine saw no IND_CHUNK_TICKING off for the far copy");

        // The reconcile: its results on the native engine, the offline engine's deltas on the stub.
        Map<String, Long> reconciled = new TreeMap<>();
        List<String> resultLines = new ArrayList<>();
        int results = 0;
        for (ScriptedEngine.Received r : of(recs, IndRecords.IND_RECONCILE_RESULT, IndRecords.ReconcileResult.DISTRICT, run.fd, run.t1)) {
            results++;
            ByteBuffer v = r.view();
            StringBuilder sb = new StringBuilder(String.format(Locale.ROOT, "op %x epoch %d chunk %d,%d at %d:", v.getLong(IndRecords.ReconcileResult.OP_ID),
                    v.getInt(IndRecords.ReconcileResult.EPOCH), v.getInt(IndRecords.ReconcileResult.CHUNK_X), v.getInt(IndRecords.ReconcileResult.CHUNK_Z),
                    v.getLong(Obs.GAME_TIME)));
            for (int i = 0; i < r.aux(); i++) {
                int e = IndRecords.ReconcileResult.ENTRIES + i * IndRecords.RESULT24;
                if (e + IndRecords.RESULT24 > v.capacity())
                    break;
                sb.append(String.format(Locale.ROOT, " inv %d %s %+d (applied %+d)", v.getInt(e), key(v.getInt(e + 4)), v.getLong(e + 8), v.getLong(e + 16)));
                if (!run.stub)
                    reconciled.merge(key(v.getInt(e + 4)), v.getLong(e + 16), Long::sum);
            }
            resultLines.add(sb.toString());
        }
        for (String l : resultLines)
            IndustryKit.record(test, "reconcile result", l);
        if (!run.stub)
            IndustryKit.record(test, "reconciled (applied, in the world change)", results + " results: " + reconciled);
        Map<String, Long> offline = new TreeMap<>();
        if (run.stub && run.judge) {
            long[] t = NativeEngine.glassReplayOffline(run.rec.stream(), run.fd);
            Map<String, Long> perInventory = new TreeMap<>();
            for (int i = 0; i + 2 < t.length; i += 3) {
                offline.merge(key((int) t[i + 1]), t[i + 2], Long::sum);
                perInventory.merge("inv " + t[i] + " " + key((int) t[i + 1]), t[i + 2], Long::sum);
            }
            IndustryKit.record(test, "offline reconcile", (t.length / 3) + " deltas: " + offline + "; per inventory " + perInventory);
        }
        // Stub: the offline reconcile went into the far stores after the reload (applyOffline); only a delta
        // the offline engine sent over the later records is still outside the world change.
        Map<String, Long> remainder = new TreeMap<>();
        Map<String, Long> applied = run.offlineApplied == null ? Map.of() : run.offlineApplied;
        if (run.stub && run.judge) {
            Map<String, Long> planned = run.offlinePlanned == null ? Map.of() : run.offlinePlanned;
            TreeSet<String> keys = new TreeSet<>(offline.keySet());
            keys.addAll(planned.keySet());
            for (String k : keys) {
                long r = offline.getOrDefault(k, 0L) - planned.getOrDefault(k, 0L);
                if (r != 0)
                    remainder.put(k, r);
            }
            if (!remainder.isEmpty())
                IndustryKit.record(test, "WARNING", "the offline reconcile over every record differs from the one applied after the reload by "
                        + remainder + ": counted on top of the world change");
        }
        if (!run.stub && results == 0)
            IndustryKit.record(test, "WARNING", "no IND_RECONCILE_RESULT for the far district after the snapshot");

        // The replay model while away, and the engine's own words for it.
        if (run.judge) {
            String model = run.away == null ? "no report" : run.away.replay().modelName();
            run.why = why(run, GlassExitKit.notes(run.fd, run.t1, run.rec));
            IndustryKit.record(test, "replay model", (run.sawGlass ? "GLASS" : "NOT glass (" + model + ")") + "; " + run.awaySummary
                    + (fReport2 == null ? "" : "; at the end " + fReport2.replay().summary()) + "; why: " + run.why);
            if (!run.sawGlass)
                run.failures.add(run.f.name() + ": the report while away does not say replay model GLASS (" + run.awaySummary + ")");
            if (run.straddle)
                straddleChecks(run);
        }

        // Ticks neither copy ran: snapshot to the first record of the district leaving LIVE; first load to ticking again.
        long left = firstLeft(recs, run);
        long unloadGap = left < 0 ? 0 : Math.max(0, left - run.t1);
        long firstLoad = firstTime(recs, IndRecords.IND_CHUNK_LOADED, IndRecords.ChunkLoaded.DISTRICT, run.fd, run.forcedAt);
        long loadGap = firstLoad < 0 ? 0 : Math.max(0, run.loadedAt - firstLoad);
        long lineLatency = run.fReport1 == null ? 0 : Math.max(0, run.fReport1.latency());
        long latency = lineLatency + unloadGap + loadGap;
        IndustryKit.record(test, "latency", latency + " ticks: line " + lineLatency + ", unload gap " + unloadGap + " (first record leaving LIVE at "
                + left + "), load gap " + loadGap + " (first IND_CHUNK_LOADED at " + firstLoad + ", ticking at " + run.loadedAt + ")");

        // The goods: every one in either copy's stores, either report or the reconcile.
        GlassReport cr = cReport2 != null ? cReport2 : run.cReport1;
        GlassReport fr = run.fReport1 != null ? run.fReport1 : fReport2;
        Map<String, GlassReport.Good> cGoods = byKey(cr), fGoods = byKey(fr);
        TreeSet<String> goods = new TreeSet<>();
        goods.addAll(run.c1.keySet());
        goods.addAll(c2.keySet());
        goods.addAll(run.f1.keySet());
        goods.addAll(f2.keySet());
        goods.addAll(cGoods.keySet());
        goods.addAll(fGoods.keySet());
        goods.addAll(offline.keySet());
        goods.addAll(applied.keySet());
        goods.addAll(reconciled.keySet());
        List<String> failed = new ArrayList<>();
        int judged = 0, moved = 0;
        List<String> weak = new ArrayList<>();
        for (String g : goods) {
            long c = run.c1.containsKey(g) || c2.containsKey(g) ? c2.getOrDefault(g, 0L) - run.c1.getOrDefault(g, 0L) : 0;
            long world = f2.getOrDefault(g, 0L) - run.f1.getOrDefault(g, 0L);
            long rec = remainder.getOrDefault(g, 0L);
            long s = world + rec;
            GlassReport.Good cg = cGoods.get(g), fg = fGoods.get(g);
            int cls = fg != null ? fg.cls() : cg != null ? cg.cls() : -1;
            String clsName = fg != null ? fg.className() : cg != null ? cg.className() : "unreported";
            long stack = stackOf(g);
            Batch b = batch(run.graph, g, cg, cr, stack);
            double batch = b.units();
            double inFlight = Math.max(stack, batch);
            double noise = cg != null && Double.isFinite(cg.sigma()) ? CHECKSUM_Z * cg.sigma() * span : 0;
            double slack = inFlight + noise;
            double rate = span > 0 ? Math.abs(c) / (double) span : 0;
            double lat = latency * rate;
            long wip = run.wip1.getOrDefault(g, 0L);
            double inputs = run.inputYield1.getOrDefault(g, 0.0);
            double rel = REL_TOL * Math.abs(c);
            double tol = slack + lat + wip + inputs + rel;
            boolean below = c == 0 ? Math.abs(s) <= slack : Math.signum(c) * s <= Math.abs(c) + slack;
            boolean within = Math.abs(s - c) <= tol;
            boolean unbounded = cls == GlassReport.UNBOUNDED;
            boolean pass = run.judge && !run.straddle ? below && (unbounded || within) : true;
            String verdict = (!run.judge ? "not judged" : run.straddle ? "information only (straddle smoke; would read "
                    + (below ? "at or below pass" : "at or below FAIL") + ", " + (unbounded ? "within not judged" : within ? "within pass" : "within FAIL") + ")"
                    : (below ? "at or below pass" : "at or below FAIL") + ", " + (unbounded ? "within not judged (unbounded)" : within ? "within pass" : "within FAIL"))
                    + (c != 0 && slack >= Math.abs(c) ? " (WEAK: the slack covers the control's change)" : "");
            // A far copy that did not run as the control did proves nothing either way: no row of it passes.
            if (run.invalidCopy != null && !run.straddle) {
                verdict = "INVALID COPY: " + verdict;
                pass = false;
            }
            // A known gap fails outright, whatever the slack: its rows never pass by accident (the lead's call, m5).
            String gap = KNOWN_GAPS.get(run.f.name());
            if (gap != null && run.judge && !run.straddle) {
                // The safety half only (the lead's call, m7): simulated above the control beyond the slack fails.
                verdict += "; KNOWN GAP (" + gap + ")" + (within || unbounded ? "" : ": the within miss does not fail the test");
                pass = run.invalidCopy == null && below;
            }
            IndustryKit.record(test, "row", String.format(Locale.ROOT, "%s | %s | %s | control %+d | simulated %+d (world %+d, reconcile %+d) | slack %.1f | tolerance %.1f | %s",
                    run.f.name(), g, clsName, c, s, world, run.stub ? applied.getOrDefault(g, 0L) + rec : reconciled.getOrDefault(g, 0L), slack, tol, verdict));
            IndustryKit.record(test, "terms", String.format(Locale.ROOT, "%s: in flight %.1f (stack %d, batch %.1f%s), noise %.1f (z %.1f x sigma %.6f/t x %d), latency %d x %.5f/t = %.1f, WIP %d, %s, relative %.1f",
                    g, inFlight, stack, batch, b.note(), noise, CHECKSUM_Z, cg == null ? 0 : cg.sigma(), span, latency, rate, lat, wip,
                    inputs > 0 ? String.format(Locale.ROOT, "input WIP yield %.1f ADDED (within only; G2-25) from far machine inputs %s", inputs, run.inputFrom1.get(g))
                            : "input WIP yield 0",
                    rel));
            judged++;
            if (c != 0)
                moved++;
            // The slack alone covers the control's whole change: the row cannot tell a replay from none.
            if (c != 0 && slack >= Math.abs(c))
                weak.add(g);
            if (!pass)
                failed.add(g + " (" + clsName + ": control " + c + ", simulated " + s + ", tolerance " + String.format(Locale.ROOT, "%.0f", tol) + ")");
        }
        // A control that moved nothing makes every row pass whatever the replay did: the run proves nothing,
        // unless the factory is built to stall (GB72's over-fed loop): then the rows check the replay made nothing.
        boolean stallExpected = !run.control.expect().isEmpty()
                && run.control.expect().stream().allMatch(e -> e.lo() == 0 && e.hi() == 0);
        if (moved == 0 && stallExpected)
            IndustryKit.record(test, "stall", "the control moved nothing, as the factory expects: every row checks |simulated| <= slack");
        else if (moved == 0 && !run.straddle)
            run.failures.add(run.f.name() + ": the control changed no good over the span (stalled or empty): nothing to compare");
        MicRtsim.LOGGER.info(String.format(Locale.ROOT, "[gametest] %s: %d goods (%d moved in the control, %d weak %s) over %d ticks (%d away), engine %s, eligible by %s, replay %s (why: %s), %s",
                test, judged, moved, weak.size(), weak, span, run.forcedAt - run.unloadedAt, run.stub ? "stub (offline replay)" : "native", run.eligibleBy, run.awaySummary,
                run.why,
                !run.judge ? "not judged" : failed.isEmpty() && run.failures.isEmpty() ? "pass"
                        : "FAIL " + (run.invalidCopy != null && !run.straddle ? "INVALID COPY " : "") + failed + (run.failures.isEmpty() ? "" : " and " + run.failures)));
        if (!failed.isEmpty())
            run.failures.add(run.f.name() + ": " + (run.invalidCopy != null && !run.straddle ? "INVALID COPY (" + run.invalidCopy + "): " : "")
                    + failed + (KNOWN_GAPS.containsKey(run.f.name()) ? "; KNOWN GAP (" + KNOWN_GAPS.get(run.f.name()) + ")" : ""));
    }

    /**
     * The straddle smoke's own checks (G2-63): every far report read while away carries the compile the far
     * copy had at the snapshot, and once one says GLASS none after it says otherwise.
     */
    static void straddleChecks(Run run) {
        int at = run.fReport1 == null ? -1 : run.fReport1.compile();
        boolean held = at >= 0 && !run.awayCompiles.isEmpty() && run.awayCompiles.keySet().stream().allMatch(c -> c == at);
        IndustryKit.record(run.test, "straddle compile", "far compile " + at + " at the snapshot; while away " + run.awayCompiles
                + " (compile: reports): " + (held ? "held" : "CHANGED"));
        if (!held)
            run.failures.add(run.f.name() + ": straddle: the far compile did not hold while away (compile " + at + " at the snapshot, while away "
                    + run.awayCompiles + ")");
        IndustryKit.record(run.test, "straddle model", run.leftGlass == 0 ? (run.sawGlass ? "GLASS throughout once it started" : "never GLASS")
                : run.leftGlass + " of " + run.awayReports + " reports while away left GLASS after it started, the last: " + run.leftGlassAs);
        if (run.leftGlass > 0)
            run.failures.add(run.f.name() + ": straddle: the replay left GLASS while away (" + run.leftGlass + " reports, the last: " + run.leftGlassAs + ")");
    }

    /** A good's batch in flight, in units, and how it was capped (empty: not capped). */
    record Batch(double units, String note) {}

    /**
     * A good's batch in flight: the control report's quantum over its window ({@code check.rs}: q is the
     * larger batch on each side of the good's edges, in and out added, over the window's ticks). A mover's
     * batch is its per-transfer amount ({@code graph.rs batch_units}: a rate with no recipe), which can be
     * far more than travels at once (a packager's 9 stacks for packages of 2): when the recipes' batches of
     * the good in the compile (in and out, twice the largest) do not reach the quantum, it comes from a
     * mover and is capped at 2 x stack, the recipes' part kept whole (the lead's call on G2 exit run m5).
     */
    static Batch batch(GlassKit.Graph g, String good, GlassReport.Good cg, GlassReport cr, long stack) {
        double q = cg != null && Double.isFinite(cg.q()) && cr != null ? cg.q() * cr.windowTicksTarget() : 0;
        if (g == null || q <= 2 * stack)
            return new Batch(q, "");
        Map<Integer, GlassKit.Recipe> recipes = new TreeMap<>();
        for (GlassKit.Recipe r : g.recipes())
            recipes.put(r.index(), r);
        double recipe = 1;
        for (GlassKit.Node n : g.nodes())
            for (GlassKit.Rate r : n.rates()) {
                GlassKit.Recipe rec = recipes.get(r.recipe());
                if (rec == null || (r.flags() & FIRST_CYCLE) != 0)
                    continue;
                for (List<GlassKit.Term> ts : List.of(rec.inputs(), rec.outputs()))
                    for (GlassKit.Term t : ts)
                        if (t.den() > 0 && key(t.good()).equals(good))
                            recipe = Math.max(recipe, Math.ceil(r.batch() * (double) t.num() / t.den()));
            }
        // Rounding: q is batch / window ticks in f64, back to units here.
        if (q <= 2 * recipe + 1e-6)
            return new Batch(q, "");
        double capped = Math.max(2.0 * stack, 2 * recipe);
        return new Batch(Math.min(q, capped), String.format(Locale.ROOT, ", capped from %.1f: a mover's capacity, the recipes' largest batch %.0f", q, recipe));
    }

    /** {@code rate_flags::FIRST_CYCLE} (records.rs): a rate for the first cycle only, not a batch in flight. */
    static final int FIRST_CYCLE = 2;

    /**
     * G2-25: per output good, what the far machines' held inputs at the first snapshot make at the compile's
     * recipe yields. A machine is a node with a rate on a recipe; it counts once per recipe it runs that makes
     * the good, its inputs read from the inventory at its own position (the limiting input over the recipe's
     * amount, times the good's expected yield net of what the recipe takes of it). Stores are not machines;
     * the partial variant's ticking chunk is skipped (its machines run in the world).
     */
    static void inputYield(Run run, ServerLevel level) {
        GlassKit.Graph g = run.graph;
        if (g == null)
            return;
        Map<Integer, GlassKit.Recipe> recipes = new TreeMap<>();
        for (GlassKit.Recipe r : g.recipes())
            recipes.put(r.index(), r);
        Set<Object> skip = Collections.newSetFromMap(new IdentityHashMap<>());
        for (BlockPos p : run.fPos) {
            IItemHandler inv = level.getCapability(Capabilities.ItemHandler.BLOCK, p, null);
            if (inv != null)
                skip.add(inv);
            IFluidHandler fl = level.getCapability(Capabilities.FluidHandler.BLOCK, p, null);
            if (fl != null)
                skip.add(fl);
        }
        Set<Object> seen = Collections.newSetFromMap(new IdentityHashMap<>());
        for (GlassKit.Node n : g.nodes()) {
            TreeSet<Integer> runs = new TreeSet<>();
            for (GlassKit.Rate r : n.rates())
                if (recipes.containsKey(r.recipe()))
                    runs.add(r.recipe());
            BlockPos pos = new BlockPos(n.x(), n.y(), n.z());
            if (runs.isEmpty() || run.fPos.contains(pos) || (run.partial && new ChunkPos(pos).equals(run.ticking)))
                continue;
            Map<String, Long> held = new TreeMap<>();
            add(held, level, pos, seen, skip);
            if (held.isEmpty())
                continue;
            for (int ri : runs) {
                GlassKit.Recipe r = recipes.get(ri);
                double lots = Double.POSITIVE_INFINITY;
                Map<String, Double> net = new TreeMap<>();
                Map<String, Long> used = new TreeMap<>();
                for (GlassKit.Term t : r.inputs()) {
                    if (t.num() <= 0 || t.den() <= 0)
                        continue;
                    String k = key(t.good());
                    double amount = t.num() / (double) t.den();
                    lots = Math.min(lots, held.getOrDefault(k, 0L) / amount);
                    net.merge(k, -amount, Double::sum);
                    used.put(k, held.getOrDefault(k, 0L));
                }
                if (used.isEmpty() || !(lots > 0) || Double.isInfinite(lots))
                    continue;
                for (GlassKit.Term t : r.outputs())
                    if (t.num() > 0 && t.den() > 0)
                        net.merge(key(t.good()), t.num() / (double) t.den(), Double::sum);
                for (Map.Entry<String, Double> e : net.entrySet()) {
                    if (e.getValue() <= 0)
                        continue;
                    run.inputYield1.merge(e.getKey(), lots * e.getValue(), Double::sum);
                    Map<String, Long> from = run.inputFrom1.computeIfAbsent(e.getKey(), k -> new TreeMap<>());
                    used.forEach((k, v) -> from.merge(k, v, Long::sum));
                }
            }
        }
    }

    /**
     * Stub: the reconcile the offline native engine sends the far district over the records so far, applied
     * to the far stores through the stock access the reconciler uses (a plugin's, else the capabilities),
     * takes before puts as in {@code Reconciler.apply}. False while it has sent none.
     */
    static boolean applyOffline(Run run) {
        run.rec.poll();
        long[] t = NativeEngine.glassReplayOffline(run.rec.stream(), run.fd);
        if (t.length < 3)
            return false;
        ServerLevel level = run.level();
        District d = IndustryKit.ind().district(run.fd);
        Map<String, Long> planned = new TreeMap<>(), applied = new TreeMap<>();
        List<String> lines = new ArrayList<>();
        for (int pass = 0; pass < 2; pass++) {
            for (int i = 0; i + 2 < t.length; i += 3) {
                long delta = t[i + 2];
                if ((pass == 0) != (delta < 0))
                    continue;
                int inventory = (int) t[i];
                int good = (int) t[i + 1];
                String k = key(good);
                planned.merge(k, delta, Long::sum);
                District.Inventory inv = d != null && inventory >= 0 && inventory < d.inventories.size() ? d.inventories.get(inventory) : null;
                Goods.Name name = Goods.parse(IndustryKit.ind().goodName(good));
                long got = inv == null || name == null ? 0 : access(level, inv).apply(name, delta);
                applied.merge(k, got, Long::sum);
                lines.add("inv " + inventory + (inv == null ? " (not in the district)" : " at " + inv.pos().toShortString()) + " " + k + " "
                        + delta + " (applied " + got + ")");
            }
        }
        run.offlinePlanned = planned;
        run.offlineApplied = applied;
        IndustryKit.record(run.test, "offline reconcile applied", "at " + run.now() + ", " + (run.now() - run.loadedAt) + " ticks after the reload: "
                + applied + " of " + planned + "; " + lines);
        return true;
    }

    /** The access the reconciler applies a delta through: a plugin's for its kinds, else the capabilities. */
    static StockAccess access(ServerLevel level, District.Inventory inv) {
        for (IndustryPlugin p : IndustryApi.plugins()) {
            StockAccess a = p.access(level, inv);
            if (a != null)
                return a;
        }
        return StockAccess.capabilities(IndustryKit.ind(), level, inv);
    }

    /** Engine notes logged per test at most; the rest are counted. */
    static final int NOTES = 400;

    /**
     * The far copy's compile as the engine got it, one {@code graph} line per node, edge, recipe and domain
     * ({@link GlassExitKit#dump}), and the edges on a cycle of the graph (each edge u to v where v reaches u):
     * the compile record does not say which edges the solver takes as back edges, but a back edge is one of
     * these.
     */
    static void dumpCompile(Run run) {
        GlassKit.Graph g = GlassKit.latest(run.fd, run.rec.records());
        run.graph = g;
        if (g == null) {
            IndustryKit.record(run.test, "graph", "no compile received");
            return;
        }
        for (String line : GlassExitKit.dump(g))
            IndustryKit.record(run.test, "graph", line);
        int n = g.nodes().size();
        List<List<Integer>> out = new ArrayList<>();
        for (int i = 0; i < n; i++)
            out.add(new ArrayList<>());
        for (GlassKit.Edge e : g.edges())
            if (e.from() >= 0 && e.from() < n && e.to() >= 0 && e.to() < n)
                out.get(e.from()).add(e.to());
        List<String> cyc = new ArrayList<>();
        for (GlassKit.Edge e : g.edges()) {
            if (e.from() < 0 || e.from() >= n || e.to() < 0 || e.to() >= n)
                continue;
            boolean[] seen = new boolean[n];
            java.util.ArrayDeque<Integer> q = new java.util.ArrayDeque<>();
            q.add(e.to());
            seen[e.to()] = true;
            boolean back = false;
            while (!q.isEmpty() && !back) {
                int u = q.poll();
                if (u == e.from())
                    back = true;
                for (int v : out.get(u))
                    if (!seen[v]) {
                        seen[v] = true;
                        q.add(v);
                    }
            }
            if (back)
                cyc.add("e" + e.index() + " n" + e.from() + "->n" + e.to());
        }
        IndustryKit.record(run.test, "graph cycles", cyc.isEmpty() ? "none: the graph is acyclic" : cyc.size() + " edges on a cycle: " + cyc);
    }

    /**
     * Logs the engine's notes about the far district since the first snapshot and says why it replayed
     * the way it did: the fallback's words when it fell back, else the model the report while away gave
     * (no note says why a district stays on the black box for deposits, links, power, sinks or damage, or
     * why it froze: the report's model and eligibility say that).
     */
    static String why(Run run, List<String> notes) {
        Map<String, Integer> kinds = new TreeMap<>();
        List<String> fallbacks = new ArrayList<>();
        List<String> starts = new ArrayList<>();
        for (int i = 0; i < notes.size(); i++) {
            String n = notes.get(i);
            if (i < NOTES)
                IndustryKit.record(run.test, "engine note", n);
            String words = n.replaceFirst("^\\d+ ", "");
            kinds.merge(words.replaceFirst("[:(].*$", "").trim(), 1, Integer::sum);
            if (words.startsWith("glass replay falls back"))
                fallbacks.add(n);
            if (words.startsWith("glass replay starts") || words.startsWith("replay starts"))
                starts.add(n);
        }
        IndustryKit.record(run.test, "engine notes", notes.size() + " since " + run.t1 + (notes.size() > NOTES ? " (first " + NOTES + " logged)" : "")
                + ": " + kinds);
        String model = run.away == null ? "no report while away" : run.away.replay().modelName()
                + (run.away.replay().eligible() ? ", eligible" : ", not eligible");
        String started = starts.isEmpty() ? "no replay start noted" : "started " + starts;
        if (!fallbacks.isEmpty())
            return "fell back " + fallbacks + "; " + started + "; report " + model;
        if (run.sawGlass)
            return "glass; " + started;
        if (run.away != null && run.away.replay().model() == GlassReport.REPLAY_BLACK_BOX)
            return "black box with no fallback noted (deposits, links, power, sinks or damage keep it; G2-16); " + started;
        if (run.away != null && !run.away.replay().eligible())
            return "frozen: the report says not eligible (G-B4); " + started;
        return "no replay: " + model + "; " + started;
    }

    // ---- reading the world ----------------------------------------------------------------------

    /** A copy's stores: those its build lists and every inventory of its district, each once. */
    static List<BlockPos> positions(CorpusFactory.Built b, District d) {
        LinkedHashSet<BlockPos> out = new LinkedHashSet<>(b.stores());
        if (d != null)
            for (District.Inventory inv : d.inventories)
                out.add(inv.pos());
        return new ArrayList<>(out);
    }

    /** Per good (items by id, fluids as {@code fluid:<id>} in mB), each handler once (a multiblock's blocks share one). */
    static Map<String, Long> stock(ServerLevel level, Collection<BlockPos> stores) {
        Map<String, Long> out = new TreeMap<>();
        Set<Object> seen = Collections.newSetFromMap(new IdentityHashMap<>());
        for (BlockPos p : stores)
            add(out, level, p, seen, null);
        return out;
    }

    private static void add(Map<String, Long> out, ServerLevel level, BlockPos p, Set<Object> seen, Set<Object> skip) {
        IItemHandler inv = level.getCapability(Capabilities.ItemHandler.BLOCK, p, null);
        if (inv != null && (skip == null || !skip.contains(inv)) && seen.add(inv))
            for (int i = 0; i < inv.getSlots(); i++) {
                ItemStack s = inv.getStackInSlot(i);
                if (!s.isEmpty())
                    out.merge(GlassCorpusKit.id(s.getItem()), (long) s.getCount(), Long::sum);
            }
        IFluidHandler fl = level.getCapability(Capabilities.FluidHandler.BLOCK, p, null);
        if (fl != null && (skip == null || !skip.contains(fl)) && seen.add(fl))
            for (int i = 0; i < fl.getTanks(); i++) {
                FluidStack f = fl.getFluidInTank(i);
                if (!f.isEmpty())
                    out.merge("fluid:" + BuiltInRegistries.FLUID.getKey(f.getFluid()), (long) f.getAmount(), Long::sum);
            }
    }

    /**
     * What the far copy holds outside its stores: every other block entity's item and fluid handlers in its
     * box (tunnels and packagers skipped: they show the inventory they sit on) and its item entities.
     */
    static Map<String, Long> wip(ServerLevel level, BlockPos min, BlockPos max, Collection<BlockPos> stores) {
        Set<Object> skip = Collections.newSetFromMap(new IdentityHashMap<>());
        for (BlockPos p : stores) {
            IItemHandler inv = level.getCapability(Capabilities.ItemHandler.BLOCK, p, null);
            if (inv != null)
                skip.add(inv);
            IFluidHandler fl = level.getCapability(Capabilities.FluidHandler.BLOCK, p, null);
            if (fl != null)
                skip.add(fl);
        }
        Set<BlockPos> storeSet = Set.copyOf(stores);
        Map<String, Long> out = new TreeMap<>();
        Set<Object> seen = Collections.newSetFromMap(new IdentityHashMap<>());
        for (BlockPos q : BlockPos.betweenClosed(min, max)) {
            BlockEntity be = level.getBlockEntity(q);
            if (be == null || storeSet.contains(q))
                continue;
            ResourceLocation type = BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(be.getType());
            String t = type == null ? "" : type.getPath();
            if (t.contains("tunnel") || t.contains("packager"))
                continue;
            add(out, level, q.immutable(), seen, skip);
        }
        for (ItemEntity e : level.getEntitiesOfClass(ItemEntity.class, new AABB(Vec3.atLowerCornerOf(min), Vec3.atLowerCornerOf(max).add(1, 1, 1))))
            out.merge(GlassCorpusKit.id(e.getItem().getItem()), (long) e.getItem().getCount(), Long::sum);
        return out;
    }

    /**
     * Lays the far copy's place out as the framework lays out the yard. {@code StructureUtils.prepareTestStructure}
     * puts the structure block at relative y 0 and the template one block above it, and
     * {@code clearSpaceForStructure} fills its box (the template's, x -2 to +3, z -3 to +3, y -3 to +20 past it)
     * with stone below the structure block's row and air from it up; the template then lays its 34 x 34 polished
     * andesite floor at relative y 1. So: relative y -2 and -1 stone, y 0 air, y 1 the floor over the copy's
     * {@link #YARD_SIZE} square from {@code origin} (its relative 0, 0, 0), air above, through {@code areaMin}
     * to {@code areaMax} (x and z: the copy's reach, every shift of the partial variant) and the margin around
     * it, in the far chunks only (no chunk loads); no entities. Factories lean on it: a diode at y 2 stands on
     * the floor, a chute at y 1 drops into the air under it.
     */
    static void clear(ServerLevel level, BlockPos areaMin, BlockPos areaMax, BlockPos origin) {
        BlockState stone = Blocks.STONE.defaultBlockState(), floor = Blocks.POLISHED_ANDESITE.defaultBlockState(), air = Blocks.AIR.defaultBlockState();
        int flags = Block.UPDATE_CLIENTS | Block.UPDATE_KNOWN_SHAPE;
        int y0 = origin.getY(), x0 = origin.getX(), z0 = origin.getZ();
        BlockPos lo = new BlockPos(areaMin.getX() - 2, y0 - 2, areaMin.getZ() - 3), hi = new BlockPos(areaMax.getX() + 3, areaMax.getY(), areaMax.getZ() + 3);
        for (BlockPos p : BlockPos.betweenClosed(lo, hi)) {
            if (level.getChunkSource().getChunkNow(p.getX() >> 4, p.getZ() >> 4) == null)
                continue;
            int x = p.getX() - x0, z = p.getZ() - z0;
            boolean onFloor = x >= 0 && x < YARD_SIZE && z >= 0 && z < YARD_SIZE;
            BlockState want = p.getY() < y0 ? stone : p.getY() == y0 + 1 && onFloor ? floor : air;
            if (level.getBlockState(p) != want)
                level.setBlock(p, want, flags);
        }
        AABB box = new AABB(Vec3.atLowerCornerOf(lo), Vec3.atLowerCornerOf(hi).add(1, 1, 1)).inflate(2);
        for (Entity e : level.getEntities((Entity) null, box, e -> !(e instanceof Player)))
            e.discard();
    }

    static long stackOf(String key) {
        if (key.startsWith("fluid:"))
            return FLUID_IN_FLIGHT;
        ResourceLocation id = ResourceLocation.tryParse(key);
        return id == null ? 64 : BuiltInRegistries.ITEM.get(id).getDefaultMaxStackSize();
    }

    /** A good's dictionary id as a stock key: the item id, or {@code fluid:<id>}. */
    static String key(int good) {
        return key(IndustryKit.ind().goodName(good), good);
    }

    static String key(String name, int good) {
        Goods.Name n = Goods.parse(name);
        if (n == null)
            return "good " + good + (name == null ? "" : " " + name);
        return n.cls() == 'f' ? "fluid:" + n.id() : n.id().toString();
    }

    static Map<String, GlassReport.Good> byKey(GlassReport r) {
        Map<String, GlassReport.Good> out = new TreeMap<>();
        if (r != null)
            for (GlassReport.Good g : r.goods())
                out.put(key(g.good()), g);
        return out;
    }

    static GlassReport report(Run run, long district) {
        return GlassExitKit.report(district, run.rec);
    }

    static void logReport(Run run, String what, GlassReport r) {
        IndustryKit.record(run.test, what, r.summary() + "; " + r.replay().summary() + "; " + r.checksum().summary());
        for (GlassReport.Good g : r.goods())
            IndustryKit.record(run.test, what + " good", g.describe(key(g.good())));
    }

    // ---- records --------------------------------------------------------------------------------

    /** The kept records of one kind for a district, at or after game time {@code from}. */
    static List<ScriptedEngine.Received> of(List<ScriptedEngine.Received> recs, int kind, int districtAt, long district, long from) {
        List<ScriptedEngine.Received> out = new ArrayList<>();
        for (ScriptedEngine.Received r : recs) {
            if (r.kind() != kind || r.bytes().length < districtAt + 8)
                continue;
            ByteBuffer v = r.view();
            if (v.getLong(districtAt) == district && v.getLong(Obs.GAME_TIME) >= from)
                out.add(r);
        }
        return out;
    }

    static int count(List<ScriptedEngine.Received> recs, int kind, int districtAt, long district, long from) {
        return of(recs, kind, districtAt, district, from).size();
    }

    static long firstTime(List<ScriptedEngine.Received> recs, int kind, int districtAt, long district, long from) {
        List<ScriptedEngine.Received> l = of(recs, kind, districtAt, district, from);
        return l.isEmpty() ? -1 : l.get(0).view().getLong(Obs.GAME_TIME);
    }

    /** The game time of the first record after the snapshot telling the engine the far district is not LIVE. */
    static long firstLeft(List<ScriptedEngine.Received> recs, Run run) {
        long best = -1;
        for (ScriptedEngine.Received r : of(recs, IndRecords.IND_LOAD_STATE, IndRecords.LoadState.DISTRICT, run.fd, run.t1))
            if (r.aux() != IndRecords.LoadState.LIVE) {
                best = r.view().getLong(Obs.GAME_TIME);
                break;
            }
        for (int kind : new int[] {IndRecords.IND_CHUNK_UNLOADED, GlassRecords.IND_CHUNK_TICKING}) {
            int at = kind == IndRecords.IND_CHUNK_UNLOADED ? IndRecords.ChunkUnloaded.DISTRICT : GlassRecords.ChunkTicking.DISTRICT;
            for (ScriptedEngine.Received r : of(recs, kind, at, run.fd, run.t1))
                if (kind == IndRecords.IND_CHUNK_UNLOADED || r.aux() == 0) {
                    long t = r.view().getLong(Obs.GAME_TIME);
                    if (best < 0 || t < best)
                        best = t;
                    break;
                }
        }
        return best;
    }

    static String loadRecords(Run run, long from) {
        List<ScriptedEngine.Received> recs = run.rec.records();
        return count(recs, IndRecords.IND_CHUNK_UNLOADED, IndRecords.ChunkUnloaded.DISTRICT, run.fd, from) + " IND_CHUNK_UNLOADED, "
                + count(recs, GlassRecords.IND_CHUNK_TICKING, GlassRecords.ChunkTicking.DISTRICT, run.fd, from) + " IND_CHUNK_TICKING, load states "
                + loadStates(recs, run);
    }

    static String loadStates(List<ScriptedEngine.Received> recs, Run run) {
        StringBuilder sb = new StringBuilder("[");
        for (ScriptedEngine.Received r : of(recs, IndRecords.IND_LOAD_STATE, IndRecords.LoadState.DISTRICT, run.fd, run.t1)) {
            ByteBuffer v = r.view();
            sb.append(sb.length() > 1 ? ", " : "").append(switch (r.aux()) {
                case IndRecords.LoadState.UNLOADED -> "unloaded";
                case IndRecords.LoadState.PARTIAL -> "partial";
                case IndRecords.LoadState.LIVE -> "live";
                default -> "state " + r.aux();
            }).append(' ').append(v.getInt(IndRecords.LoadState.TICKING_CHUNKS)).append('/').append(v.getInt(IndRecords.LoadState.TOTAL_CHUNKS))
                    .append(" at ").append(v.getLong(Obs.GAME_TIME));
        }
        return sb.append(']').toString();
    }

    static String epochs(List<ScriptedEngine.Received> recs, Run run) {
        List<Integer> out = new ArrayList<>();
        for (ScriptedEngine.Received r : of(recs, IndRecords.IND_CHUNK_LOADED, IndRecords.ChunkLoaded.DISTRICT, run.fd, run.forcedAt))
            out.add(r.view().getInt(IndRecords.ChunkLoaded.APPLIED_EPOCH));
        return out.toString();
    }

    static String longs(it.unimi.dsi.fastutil.longs.LongOpenHashSet chunks) {
        List<String> out = new ArrayList<>();
        for (long c : chunks)
            out.add(ChunkPos.getX(c) + "," + ChunkPos.getZ(c));
        return out.toString();
    }
}
