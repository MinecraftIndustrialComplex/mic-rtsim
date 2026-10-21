package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.industry.Districts;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.glass.CreateGlass;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import com.simibubi.create.content.redstone.diodes.BrassDiodeBlock;
import com.simibubi.create.foundation.blockEntity.behaviour.BlockEntityBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.scrollValue.ScrollValueBehaviour;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestInfo;
import net.minecraft.gametest.framework.GameTestListener;
import net.minecraft.gametest.framework.GameTestRunner;
import net.minecraft.server.level.FullChunkStatus;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.thread.ProcessorMailbox;
import net.minecraft.util.thread.StrictQueue;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import it.unimi.dsi.fastutil.longs.LongSet;

import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.TreeSet;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.*;

/**
 * The exit test harness's own checks ({@link GlassReplayGameTests}): the far copy's chunk readiness, the
 * warm-up verdict and the fingerprint of a clock, each a bug that made the G2 exit test flaky.
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class GlassReplayHarnessGameTests {
    private GlassReplayHarnessGameTests() {}

    /** The held chunk's distance from the yard along x, in chunks: past every far copy (4,000 + 16 per slot). */
    static final int HOLD_CHUNKS = 6000;
    /** Ticks the entity IO is held once the chunk is block-ticking with its neighbours FULL. */
    static final int HOLD_TICKS = 40;
    /**
     * A backstop only (G2-78): the blocker lets go when the test releases it, when the test is done and when
     * the server stops, so it never ends on its own while the test runs. The chunk takes 17 to 44 s to be
     * ready on a loaded laptop; the old 30 s limit ended the hold before that once.
     */
    static final long HOLD_SECONDS = 600;
    /** How often the blocker looks whether the test or the server is done. */
    static final long HOLD_POLL_MS = 100;

    /**
     * A hold on the level's entity IOWorker ({@link #holdEntityIo}). {@code ended} is null while the blocker
     * holds, or waits its turn on the mailbox (every entity load queued after it waits either way), and says
     * why it let go once it has.
     */
    static final class IoHold {
        final CountDownLatch release = new CountDownLatch(1);
        final long postedNanos = System.nanoTime();
        volatile long engagedNanos = -1;
        volatile String ended;

        String state() {
            long now = System.nanoTime();
            return ended != null ? "let go: " + ended
                    : engagedNanos < 0 ? "queued for " + (now - postedNanos) / 1_000_000 + " ms"
                    : "on for " + (now - engagedNanos) / 1_000_000 + " ms";
        }
    }

    /**
     * Holds the level's entity IOWorker: a task at its foreground priority that waits for the hold's release.
     * Every entity load queued after it (a chunk's entities, read when the chunk reaches FULL) waits too,
     * as one does behind a slow queue on a loaded GameTest server. It also lets go once the test is done or
     * the server stops: {@code IOWorker.close} waits on the mailbox.
     */
    @SuppressWarnings("unchecked")
    static IoHold holdEntityIo(ServerLevel level, GameTestInfo info) {
        IoHold hold = new IoHold();
        try {
            Object manager = field(ServerLevel.class, "entityManager").get(level);
            Object storage = field(manager.getClass(), "permanentStorage").get(manager);
            Object region = field(storage.getClass(), "simpleRegionStorage").get(storage);
            Object worker = field(region.getClass(), "worker").get(region);
            ProcessorMailbox<StrictQueue.IntRunnable> mailbox = (ProcessorMailbox<StrictQueue.IntRunnable>) field(worker.getClass(), "mailbox").get(worker);
            mailbox.tell(new StrictQueue.IntRunnable(0, () -> {
                hold.engagedNanos = System.nanoTime();
                long limit = hold.engagedNanos + TimeUnit.SECONDS.toNanos(HOLD_SECONDS);
                String why = null;
                try {
                    while (why == null) {
                        if (hold.release.await(HOLD_POLL_MS, TimeUnit.MILLISECONDS))
                            why = "released";
                        else if (info.isDone())
                            why = "the test is done";
                        else if (!level.getServer().isRunning())
                            why = "the server stops";
                        else if (System.nanoTime() > limit)
                            why = "the backstop, " + HOLD_SECONDS + " s";
                    }
                } catch (InterruptedException e) {
                    why = "interrupted";
                    Thread.currentThread().interrupt();
                }
                hold.ended = why;
            }));
        } catch (ReflectiveOperationException e) {
            throw new GameTestAssertException("cannot reach the entity IOWorker: " + e);
        }
        return hold;
    }

    /**
     * Whether the level's {@code EntityStorage} already knows the chunk has no entities: then its entity
     * load completes at once and never queues on the IOWorker, so the hold cannot hold it.
     */
    static boolean knownEmpty(ServerLevel level, long chunk) {
        try {
            Object manager = field(ServerLevel.class, "entityManager").get(level);
            Object storage = field(manager.getClass(), "permanentStorage").get(manager);
            return ((LongSet) field(storage.getClass(), "emptyChunks").get(storage)).contains(chunk);
        } catch (ReflectiveOperationException e) {
            throw new GameTestAssertException("cannot reach the entity storage: " + e);
        }
    }

    static Field field(Class<?> c, String name) throws NoSuchFieldException {
        for (Class<?> k = c; k != null; k = k.getSuperclass())
            try {
                Field f = k.getDeclaredField(name);
                f.setAccessible(true);
                return f;
            } catch (NoSuchFieldException e) {
                // the superclass may declare it
            }
        throw new NoSuchFieldException(c.getName() + "." + name);
    }

    static KineticBlockEntity kinetic(ServerLevel level, BlockPos p) {
        BlockEntity be = level.getBlockEntity(p);
        check("a kinetic block entity at " + p.toShortString() + " (found " + be + ")", be instanceof KineticBlockEntity);
        return (KineticBlockEntity) be;
    }

    /**
     * The dead far copy (G2-68): a forced chunk whose entities have not loaded is block-ticking by its
     * ticket ({@link Districts#blockTicking}, the harness's old readiness) with its neighbours FULL, and
     * ticks no block entity: a shaft placed there keeps {@code needsSpeedUpdate} (cleared at its first
     * tick). The harness's readiness ({@link GlassReplayGameTests#notTicking}) must say it does not tick;
     * once the entities load it must say it does, and the shaft ticks. The hold must hold until the test
     * lets it go (G2-78): each held tick checks it first, so a hold that let go says so.
     */
    @GameTest(template = YARD, batch = "glass_harness_01", timeoutTicks = 12000)
    public static void aChunkWithItsEntitiesUnloadedTicksNoBlockEntity(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        ServerLevel level = helper.getLevel();
        BlockPos at = helper.absolutePos(new BlockPos(8, 2, 8)).offset(16 * HOLD_CHUNKS, 0, 0);
        ChunkPos c = new ChunkPos(at);
        long k = c.toLong();
        check("the held chunk " + c + " is not loaded before the test forces it", level.getChunkSource().getChunkNow(c.x, c.z) == null);
        check("the held chunk " + c + " is not known empty (its entity load would skip the IOWorker)", !knownEmpty(level, k));
        IoHold hold = holdEntityIo(level, helper.testInfo);
        boolean[] done = {false};
        Runnable release = () -> {
            hold.release.countDown();
            if (!done[0]) {
                done[0] = true;
                level.setChunkForced(c.x, c.z, false);
            }
        };
        helper.testInfo.addListener(new GameTestListener() {
            @Override
            public void testStructureLoaded(GameTestInfo info) {}

            @Override
            public void testPassed(GameTestInfo info, GameTestRunner runner) {
                release.run();
            }

            @Override
            public void testFailed(GameTestInfo info, GameTestRunner runner) {
                release.run();
            }

            @Override
            public void testAddedForRerun(GameTestInfo oldTest, GameTestInfo newTest, GameTestRunner runner) {}
        });
        long forcing = System.nanoTime();
        level.setChunkForced(c.x, c.z, true);
        long forced = System.nanoTime();
        String test = "glass harness entities";
        List<String> reasons = new ArrayList<>();
        helper.startSequence()
                .thenWaitUntil(() -> {
                    LevelChunk ch = level.getChunkSource().getChunkNow(c.x, c.z);
                    boolean ready = ch != null && Districts.blockTicking(level, k) && level.getChunkSource().isPositionTicking(k)
                            && ch.getFullStatus().isOrAfter(FullChunkStatus.BLOCK_TICKING);
                    if (!ready)
                        GlassReplayGameTests.ioWait();
                    check("the held chunk " + c + " loads, block-ticking by its ticket with its neighbours FULL (its ticking future done)", ready);
                })
                .thenExecute(() -> {
                    set(helper, at, AllBlocks.SHAFT.getDefaultState());
                    long now = System.nanoTime();
                    record(test, "held", "chunk " + c + " block-ticking with its neighbours FULL, entities loaded " + level.areEntitiesLoaded(k)
                            + "; forced in " + (forced - forcing) / 1_000_000 + " ms, ready " + (now - forced) / 1_000_000
                            + " ms later; the hold " + hold.state() + "; a shaft at " + at.toShortString());
                })
                .thenExecuteFor(HOLD_TICKS, () -> {
                    check("the entity IO hold holds (" + hold.state() + ")", hold.ended == null);
                    check("the held chunk's entities are not loaded although the hold holds: a load skipped the entity IOWorker",
                            !level.areEntitiesLoaded(k));
                    check("the held chunk is still block-ticking by its ticket", Districts.blockTicking(level, k));
                    check("the shaft in the held chunk did not tick (needsSpeedUpdate)", kinetic(level, at).needsSpeedUpdate());
                    String why = GlassReplayGameTests.notTicking(level, c);
                    check("the harness's readiness says the held chunk ticks block entities, which it does not", why != null);
                    if (reasons.isEmpty())
                        reasons.add(why);
                })
                .thenExecute(() -> {
                    record(test, "while held", "the readiness said: " + reasons + "; the hold " + hold.state());
                    hold.release.countDown();
                })
                .thenWaitUntil(() -> {
                    String why = GlassReplayGameTests.notTicking(level, c);
                    if (why != null)
                        GlassReplayGameTests.ioWait();
                    check("the readiness says the chunk ticks once its entities load (" + why + ")", why == null);
                    check("the shaft ticks once the chunk's entities load", !kinetic(level, at).needsSpeedUpdate());
                })
                .thenExecute(() -> record(test, "released", "the chunk ticks and the shaft ran its first tick"))
                .thenSucceed();
    }

    /** One good for {@link #theWarmUpCatchesACopyThatNeverMoved}, its tolerance built as the warm-up check builds it. */
    static GlassReplayGameTests.WarmGood good(String g, long control, long far, double inFlight, double noise) {
        return new GlassReplayGameTests.WarmGood(g, control, far, 64, inFlight, noise, GlassReplayGameTests.REL_TOL * Math.abs(control));
    }

    /**
     * The warm-up verdict (G2-70), on the numbers of the native run where press_sheets' far copy slipped
     * through: its sheets and ingots never moved, within a tolerance of 124 in flight (a press's batch)
     * against the control's 126 sheets and 128 ingots. Dead. A copy that lags inside the tolerance lives; so
     * does one beside a control that moved less than a stack, and a control that did not move at all.
     */
    @GameTest(template = YARD, batch = "glass_harness_02", timeoutTicks = 100)
    public static void theWarmUpCatchesACopyThatNeverMoved(GameTestHelper helper) {
        String test = "glass harness warm-up";
        List<GlassReplayGameTests.WarmGood> press = List.of(good("create:iron_sheet", 126, 0, 123.8, 0.7),
                good("minecraft:iron_ingot", -128, 0, 123.8, 31.8));
        String dead = GlassReplayGameTests.warmVerdict(press);
        record(test, "press_sheets (native d20e6bd)", dead == null ? "alive" : "dead: " + dead);
        check("a far copy whose stores never moved beside press_sheets' control is dead (tolerances "
                + press.get(0).tol() + ", " + press.get(1).tol() + ")", dead != null);
        String lags = GlassReplayGameTests.warmVerdict(List.of(good("create:iron_sheet", 126, 70, 123.8, 0.7),
                good("minecraft:iron_ingot", -128, -64, 123.8, 31.8)));
        check("a far copy that lags within the tolerance lives (" + lags + ")", lags == null);
        String small = GlassReplayGameTests.warmVerdict(List.of(good("minecraft:white_wool", 32, 0, 64, 40.9)));
        check("a far copy beside a control that moved less than a stack lives (" + small + ")", small == null);
        String still = GlassReplayGameTests.warmVerdict(List.of(good("minecraft:clay", 0, 0, 64, 0), good("minecraft:clay_ball", 0, 0, 64, 0)));
        check("a far copy beside a control that did not move lives (" + still + ")", still == null);
        String off = GlassReplayGameTests.warmVerdict(List.of(good("create:wheat_flour", 1283, 0, 64, 68.6)));
        check("a far copy off by more than its tolerance is dead", off != null);
        helper.succeed();
    }

    /**
     * A pulse timer's fingerprint (G2-72): its POWERING flips for 2 ticks in every period, which is the
     * clock's state and not the build. Hashed, every fingerprint pass that lands in a pulse mismatches the
     * compile, and package_loop recompiled every 20 ticks ("district none"). Over one period it powers at
     * some tick, and every tick hashes the same.
     */
    @GameTest(template = YARD, batch = "glass_harness_03", timeoutTicks = 400)
    public static void aPulseTimersPulseIsNotStructure(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        ServerLevel level = helper.getLevel();
        BlockPos timer = helper.absolutePos(new BlockPos(5, 2, 5));
        set(helper, timer, AllBlocks.PULSE_TIMER.getDefaultState().setValue(BlockStateProperties.HORIZONTAL_FACING, Direction.SOUTH));
        ScrollValueBehaviour period = BlockEntityBehaviour.get(level, timer, ScrollValueBehaviour.TYPE);
        check("the pulse timer has its period setting", period != null);
        period.setValue(PackageLoopFactory.CLOCK);
        Set<Long> hashes = new TreeSet<>();
        int[] seen = {0, 0};
        helper.startSequence()
                .thenIdle(2)
                .thenExecuteFor(PackageLoopFactory.CLOCK + 2, () -> {
                    BlockEntity be = level.getBlockEntity(timer);
                    check("the pulse timer stands", be != null);
                    seen[be.getBlockState().getValue(BrassDiodeBlock.POWERING) ? 1 : 0]++;
                    hashes.add(CreateGlass.INSTANCE.fingerprint(be));
                })
                .thenExecute(() -> {
                    record("glass harness pulse timer", "over " + (seen[0] + seen[1]) + " ticks", seen[1] + " powering, " + seen[0] + " not; "
                            + hashes.size() + " fingerprints " + hashes);
                    check("the pulse timer powered at some tick of its period", seen[1] > 0);
                    check("the pulse timer rested at some tick of its period", seen[0] > 0);
                    check("the pulse timer's fingerprint is one value over its period, not " + hashes.size(), hashes.size() == 1);
                })
                .thenSucceed();
    }
}
