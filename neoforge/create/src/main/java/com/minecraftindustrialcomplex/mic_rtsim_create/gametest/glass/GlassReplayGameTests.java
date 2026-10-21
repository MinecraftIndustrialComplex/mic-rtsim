package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Districts;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Goods;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.TickGate;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassConfig;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassReport;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.GlassExitKit;
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
 * The far copy's chunks are force-loaded exactly as the GameTest framework forces the yard's, so both tick
 * alike. Both run until the far copy's report has a completed passing window (G-B4: the replay is
 * eligible). Then, in one tick, both copies' stocks are snapshot and the far copy's chunks lose their
 * tickets: they stop ticking, unload for real (the engine sees IND_CHUNK_UNLOADED for every chunk) and stay
 * away at least {@link #SPAN} ticks from the snapshot while the control runs on. They are force-loaded again,
 * the reconcile lands (its result is journaled, no gate or held reconcile is left) and after {@link #SETTLE}
 * ticks both copies are snapshot again in one tick.
 *
 * <p><b>Judging</b>, per good, with {@code c} the control's change and {@code s} the far copy's: on the
 * native engine {@code s} is the far copy's own change (the engine's reconcile is in it); on the stub, which
 * simulates nothing, it is the far copy's change plus every IND_CHUNK_RECONCILE delta a fresh native
 * industry engine sends the far district over the same records ({@link NativeEngine#glassReplayOffline}).
 * <ul>
 *   <li><b>at or below</b>: {@code sign(c) x s <= |c| + slack}, and {@code |s| <= slack} for {@code c = 0}:
 *       the replay never makes more, or uses more, than the control did;</li>
 *   <li><b>within tolerance</b>: {@code |s - c| <= tol}, {@code tol = slack + latency x |c| / span + WIP + }
 *       {@link #REL_TOL}{@code  x |c|}.</li>
 * </ul>
 * {@code slack} is one batch or stack in flight (the larger of the good's stack size, {@link #FLUID_IN_FLIGHT}
 * for a fluid, and the control report's quantum times its window) plus the control's noise over the span
 * ({@link #CHECKSUM_Z} x the control report's sigma per tick x span). {@code latency} is the far report's
 * line latency plus the ticks neither copy ran: from the snapshot to the first record telling the engine
 * the district left LIVE, and from the first IND_CHUNK_LOADED to the tick every far chunk ticks again.
 * {@code WIP} is what the far copy held outside its stores at the snapshot (machines, belts, depots, item
 * entities), frozen in the saved chunks. UNBOUNDED goods are judged only at or below. Every comparison is a
 * {@code [gametest] glass replay <factory>: row} line: good, class, control, simulated, slack, tolerance,
 * verdict; a {@code terms} line says what the tolerance is made of.
 *
 * <p>The report while the far copy is away must say replay model GLASS: on the native engine any report
 * read while it is unloaded; on the stub the offline report of the records up to the reload. Its replay
 * summary (regimes, breakpoints, oscillations, cut edges) is logged.
 *
 * <p>The <b>partial</b> variant ({@link #PARTIAL}) keeps every far chunk loaded: the chunk of the factory's
 * anchor keeps ticking blocks (a level 32 ticket) and the others are FULL without ticking (level 33,
 * Java sends IND_CHUNK_TICKING), so the glass box simulates part of the factory with its cut edges (I7,
 * G-B6) and reconciles it when the chunks tick again. Judged the same way.
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
    /** The factories the partial variant runs (their district spans several chunks wherever the yard lands). */
    static final Set<String> PARTIAL = Set.of("brass_tunnels", "boiler_overstress");

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
                    helper -> start(helper, f, slot, false)));
            if (PARTIAL.contains(f.name())) {
                int pslot = GlassCorpus.ALL.size() + i;
                out.add(new TestFunction("glassreplay_partial_" + f.name(), "glassreplay.partial_" + f.name().toLowerCase(Locale.ROOT),
                        MicRtsimCreate.MODID + ":" + IndustryKit.YARD, Rotation.NONE, maxTicks, 0L, true, false, 1, 1, true,
                        helper -> start(helper, f, pslot, true)));
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

    /** One factory's run. */
    static final class Run {
        final GameTestHelper helper;
        final Far far;
        final CorpusFactory f;
        final boolean partial;
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
        String awaySummary = "no report while away";
        int awayReports;
        int prefix = -1;
        final List<String> failures = new ArrayList<>();
        String last = "starting";

        Run(GameTestHelper helper, CorpusFactory f, int slot, boolean partial) {
            this.helper = helper;
            this.f = f;
            this.partial = partial;
            this.far = new Far(helper, farOffset(slot));
            this.test = "glass replay " + (partial ? "partial " : "") + f.name();
            this.rec = GlassExitKit.start(helper.getLevel());
            this.judge = GlassExitKit.canJudge();
            this.stub = !IndustryKit.nativeEngine();
            farMin = far.absolutePos(BlockPos.ZERO);
            farMax = far.absolutePos(new BlockPos(33, TOP, 33));
            // The partial variant shifts the far copy by up to 15 blocks in x or z: its chunks cover that too.
            BlockPos reach = partial ? farMax.offset(15, 0, 15) : farMax;
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

    static void start(GameTestHelper helper, CorpusFactory f, int slot, boolean partial) {
        GlassConfig.Test.set(GlassConfig.CHECKSUM_WINDOW_TICKS, WINDOW);
        IndustryKit.fastConfig();
        Run run = new Run(helper, f, slot, partial);
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
        // Forget the industry of both places (the world is reused between runs), clear the far copy's place
        // as the framework clears the yard, and build both in this tick.
        IndustryKit.forget(run.helper);
        ChunkPos lo = run.farChunks.get(0), hi = run.farChunks.get(run.farChunks.size() - 1);
        IndustryKit.ind().districts().forgetArea(level, lo.x - 1, lo.z - 1, hi.x + 1, hi.z + 1);
        clear(level, run.farMin, run.partial ? run.farMax.offset(15, 0, 15) : run.farMax);
        run.control = run.f.build(run.helper);
        seed(run.helper, run.control);
        if (run.partial)
            split(run);
        run.copy = run.f.build(run.far);
        seed(run.far, run.copy);
        run.builtAt = run.now();
        GlassCorpusKit.log(run.f.name(), "replay built", run.copy.notes());
        run.stage(Stage.RUN);
    }

    /**
     * The partial variant: shifts the far copy (0 to 15 blocks, along the axis its stores and anchor spread
     * most) so that a chunk border runs between them, wherever the yard landed. The far district then has two
     * chunks or more, the anchor's ticking and the others not.
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
        BlockPos by = alongX ? new BlockPos(shift, 0, 0) : new BlockPos(0, 0, shift);
        run.far.offset = run.far.offset.offset(by);
        run.farMin = run.farMin.offset(by);
        run.farMax = run.farMax.offset(by);
        IndustryKit.record(run.test, "split", "stores and anchor span " + (alongX ? "x " : "z ") + min + " to " + max + " in the control; the far copy moves "
                + shift + " along " + (alongX ? "x" : "z") + " so a chunk border falls before control " + (alongX ? "x " : "z ") + border
                + (max == min ? " (they do not spread: no border between them)" : ""));
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
        run.fReport1 = fr;
        run.cReport1 = run.judge ? report(run, run.cd) : null;
        IndustryKit.record(run.test, "snapshot 1", "at " + run.t1 + ": control " + run.c1 + " over " + run.cPos.size() + " stores; far "
                + run.f1 + " over " + run.fPos.size() + " stores; far WIP " + run.wip1);
        if (run.cPos.size() != run.fPos.size())
            IndustryKit.record(run.test, "WARNING", "the copies' stores differ: control " + run.cPos.size() + ", far " + run.fPos.size());
        if (run.cReport1 != null)
            logReport(run, "control report 1", run.cReport1);
        if (fr != null)
            logReport(run, "far report 1", fr);
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
                run.sawGlass |= r.replay().model() == GlassReport.REPLAY_GLASS;
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
        if (!run.stub && results == 0)
            IndustryKit.record(test, "WARNING", "no IND_RECONCILE_RESULT for the far district after the snapshot");

        // The replay model while away.
        if (run.judge) {
            String model = run.away == null ? "no report" : run.away.replay().modelName();
            IndustryKit.record(test, "replay model", (run.sawGlass ? "GLASS" : "NOT glass (" + model + ")") + "; " + run.awaySummary
                    + (fReport2 == null ? "" : "; at the end " + fReport2.replay().summary()));
            if (!run.sawGlass)
                run.failures.add(run.f.name() + ": the report while away does not say replay model GLASS (" + run.awaySummary + ")");
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
        goods.addAll(reconciled.keySet());
        List<String> failed = new ArrayList<>();
        int judged = 0, moved = 0;
        List<String> weak = new ArrayList<>();
        for (String g : goods) {
            long c = run.c1.containsKey(g) || c2.containsKey(g) ? c2.getOrDefault(g, 0L) - run.c1.getOrDefault(g, 0L) : 0;
            long world = f2.getOrDefault(g, 0L) - run.f1.getOrDefault(g, 0L);
            long rec = offline.getOrDefault(g, 0L);
            long s = world + rec;
            GlassReport.Good cg = cGoods.get(g), fg = fGoods.get(g);
            int cls = fg != null ? fg.cls() : cg != null ? cg.cls() : -1;
            String clsName = fg != null ? fg.className() : cg != null ? cg.className() : "unreported";
            long stack = stackOf(g);
            double batch = cg != null && Double.isFinite(cg.q()) && cr != null ? cg.q() * cr.windowTicksTarget() : 0;
            double inFlight = Math.max(stack, batch);
            double noise = cg != null && Double.isFinite(cg.sigma()) ? CHECKSUM_Z * cg.sigma() * span : 0;
            double slack = inFlight + noise;
            double rate = span > 0 ? Math.abs(c) / (double) span : 0;
            double lat = latency * rate;
            long wip = run.wip1.getOrDefault(g, 0L);
            double rel = REL_TOL * Math.abs(c);
            double tol = slack + lat + wip + rel;
            boolean below = c == 0 ? Math.abs(s) <= slack : Math.signum(c) * s <= Math.abs(c) + slack;
            boolean within = Math.abs(s - c) <= tol;
            boolean unbounded = cls == GlassReport.UNBOUNDED;
            boolean pass = run.judge ? below && (unbounded || within) : true;
            String verdict = (!run.judge ? "not judged"
                    : (below ? "at or below pass" : "at or below FAIL") + ", " + (unbounded ? "within not judged (unbounded)" : within ? "within pass" : "within FAIL"))
                    + (c != 0 && slack >= Math.abs(c) ? " (WEAK: the slack covers the control's change)" : "");
            IndustryKit.record(test, "row", String.format(Locale.ROOT, "%s | %s | %s | control %+d | simulated %+d (world %+d, reconcile %+d) | slack %.1f | tolerance %.1f | %s",
                    run.f.name(), g, clsName, c, s, world, run.stub ? rec : reconciled.getOrDefault(g, 0L), slack, tol, verdict));
            IndustryKit.record(test, "terms", String.format(Locale.ROOT, "%s: in flight %.1f (stack %d, batch %.1f), noise %.1f (z %.1f x sigma %.6f/t x %d), latency %d x %.5f/t = %.1f, WIP %d, relative %.1f",
                    g, inFlight, stack, batch, noise, CHECKSUM_Z, cg == null ? 0 : cg.sigma(), span, latency, rate, lat, wip, rel));
            judged++;
            if (c != 0)
                moved++;
            // The slack alone covers the control's whole change: the row cannot tell a replay from none.
            if (c != 0 && slack >= Math.abs(c))
                weak.add(g);
            if (!pass)
                failed.add(g + " (" + clsName + ": control " + c + ", simulated " + s + ", tolerance " + String.format(Locale.ROOT, "%.0f", tol) + ")");
        }
        // A control that moved nothing makes every row pass whatever the replay did: the run proves nothing.
        if (moved == 0)
            run.failures.add(run.f.name() + ": the control changed no good over the span (stalled or empty): nothing to compare");
        MicRtsim.LOGGER.info(String.format(Locale.ROOT, "[gametest] %s: %d goods (%d moved in the control, %d weak %s) over %d ticks (%d away), engine %s, eligible by %s, replay %s, %s",
                test, judged, moved, weak.size(), weak, span, run.forcedAt - run.unloadedAt, run.stub ? "stub (offline replay)" : "native", run.eligibleBy, run.awaySummary,
                !run.judge ? "not judged" : failed.isEmpty() && run.failures.isEmpty() ? "pass"
                        : "FAIL " + failed + (run.failures.isEmpty() ? "" : " and " + run.failures)));
        if (!failed.isEmpty())
            run.failures.add(run.f.name() + ": " + failed);
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

    /** Clears the far copy's place as the framework clears the yard: a polished andesite floor at y 0, air above, no entities. */
    static void clear(ServerLevel level, BlockPos min, BlockPos max) {
        BlockState floor = Blocks.POLISHED_ANDESITE.defaultBlockState(), air = Blocks.AIR.defaultBlockState();
        int flags = Block.UPDATE_CLIENTS | Block.UPDATE_KNOWN_SHAPE;
        for (BlockPos p : BlockPos.betweenClosed(min, max)) {
            BlockState want = p.getY() == min.getY() ? floor : air;
            if (level.getBlockState(p) != want)
                level.setBlock(p, want, flags);
        }
        AABB box = new AABB(Vec3.atLowerCornerOf(min), Vec3.atLowerCornerOf(max).add(1, 1, 1)).inflate(2);
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
