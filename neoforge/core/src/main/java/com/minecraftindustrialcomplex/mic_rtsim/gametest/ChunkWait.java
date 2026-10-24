package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestInfo;
import net.minecraft.gametest.framework.GameTestListener;
import net.minecraft.gametest.framework.GameTestRunner;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.TicketType;
import net.minecraft.world.level.ChunkPos;

import java.util.concurrent.atomic.AtomicLong;

/**
 * The two clocks of a GameTest that waits for chunks: the one helper every module's tests use for
 * chunks loading, ticking or unloading, and for holding chunks outside their arena.
 *
 * <p>Two traps made the earlier waits flaky:
 * <ul>
 *   <li>The GameTest server ticks unthrottled (12,000 to 20,000 ticks a second when idle), and a
 *   ticket's lifespan and a test's {@code timeoutTicks} both count server ticks. A 30,000-tick ticket
 *   was gone in about 2 s, before generation at a fresh far position under load was done, so its
 *   chunks dropped below FULL and never came; a tick bound on a chunk wait runs out the same way. A
 *   {@link Hold} has no lifespan: it is released when its test passes or fails (a listener on the
 *   test), or by {@link Hold#release()}.</li>
 *   <li>{@code GameTestHelper.fail} inside {@code thenWaitUntil} throws the exception a failing check
 *   throws, and the sequence swallows both until the test's own tick timeout, so a bound written with it
 *   never ends a test. The bounds here fail the test itself ({@link #fail}).</li>
 * </ul>
 *
 * <p>A chunk wait ({@link #ready}, {@link #chunks}) is real work on other threads (generation, the
 * world's IO), so it is bounded by wall-clock time ({@link #WALL_NANOS} from its first poll) and counts
 * no ticks. A wait on in-game logic ({@link #logic}, {@link #engine} for the engine's answers) is bounded
 * by the test's tick budget, counted from the tick the last chunk wait ended (or the test's start). A
 * test using these sets GameTest's own tick timeout out of reach ({@link #NO_TICK_TIMEOUT}), because
 * that counts ticks from the test's start, chunk waits included, and wraps every wait in one of them,
 * so each is bounded.
 */
public final class ChunkWait {
    /** GameTest's own timeout for a test whose waits are all bounded here: out of reach. */
    public static final int NO_TICK_TIMEOUT = 10_000_000;
    /** A chunk wait's wall-clock bound. */
    public static final long WALL_NANOS = 120_000_000_000L;
    /** How often a slow chunk wait logs its progress. */
    private static final long LOG_EVERY_NANOS = 10_000_000_000L;

    /** What a held square needs. */
    public enum Need {
        /** Loaded as a level chunk: blocks and block entities read and written ({@code getChunkNow}). */
        FULL(0),
        /** Ticking entities, with their entities loaded: mobs walk and tick there. */
        ENTITY_TICKING(2);

        /** Ticket levels below FULL the square needs (two to entity ticking). */
        final int margin;

        Need(int margin) {
            this.margin = margin;
        }
    }

    /** Never times out; each hold's own value, so two holds never merge into one ticket. */
    private static final TicketType<Long> HOLD = TicketType.create("mic_rtsim_test_hold", Long::compare);
    private static final AtomicLong NEXT = new AtomicLong();

    private final GameTestHelper helper;
    private final int logicTicks;
    /** The tick the last chunk wait ended, or the test's start: logic waits count from it. */
    private long mark;

    private ChunkWait(GameTestHelper helper, int logicTicks) {
        this.helper = helper;
        this.logicTicks = logicTicks;
        this.mark = helper.getTick();
    }

    /**
     * A test's clocks: chunk waits bounded by {@link #WALL_NANOS}, and logic waits by {@code logicTicks}
     * from the last chunk wait's end.
     */
    public static ChunkWait of(GameTestHelper helper, int logicTicks) {
        return new ChunkWait(helper, logicTicks);
    }

    /** Holds every chunk within {@code blocks} of {@code at} loaded as level chunks until the test ends. */
    public Hold hold(BlockPos at, int blocks) {
        return hold(at, blocks, Need.FULL);
    }

    /** Holds every chunk within {@code blocks} of {@code at} at {@code need} until the test ends. */
    public Hold hold(BlockPos at, int blocks, Need need) {
        Hold h = new Hold(helper.getLevel(), at, blocks, need);
        onEnd(h::release);
        return h;
    }

    /**
     * Runs {@code r} when the test passes or fails, however it ends: for chunks a test forced (a forced
     * chunk is saved with the world, and a failed test's would stay loaded in every later run).
     */
    public void onEnd(Runnable r) {
        helper.testInfo.addListener(new GameTestListener() {
            @Override
            public void testStructureLoaded(GameTestInfo info) {}

            @Override
            public void testPassed(GameTestInfo info, GameTestRunner runner) {
                r.run();
            }

            @Override
            public void testFailed(GameTestInfo info, GameTestRunner runner) {
                r.run();
            }

            @Override
            public void testAddedForRerun(GameTestInfo old, GameTestInfo next, GameTestRunner runner) {}
        });
    }

    /**
     * The wait for a held square, for {@code thenWaitUntil}: a {@link #chunks} wait that says how many of
     * its chunks were ready, and, failed, what the centre chunk's holder held.
     */
    public Runnable ready(Hold h) {
        return chunks(h::check, h::holder);
    }

    /**
     * A wait on chunks loading, ticking or unloading, for {@code thenWaitUntil}: {@code check} fails (a
     * {@code GameTestAssertException}) until they have. Bounded by {@link #WALL_NANOS} of real time from
     * its first poll, counting no ticks; a slow wait logs the check's message every 10 s.
     */
    public Runnable chunks(Runnable check) {
        return chunks(check, null);
    }

    private Runnable chunks(Runnable check, java.util.function.Supplier<String> more) {
        long[] t = {-1, -1}; // the first poll, the last log
        return () -> {
            long now = System.nanoTime();
            if (t[0] < 0)
                t[0] = t[1] = now;
            try {
                check.run();
            } catch (GameTestAssertException e) {
                if (now - t[0] > WALL_NANOS)
                    fail(e.getMessage() + String.format(" (not within %.0f s)", WALL_NANOS / 1e9) + (more == null ? "" : "; " + more.get()));
                if (now - t[1] >= LOG_EVERY_NANOS) {
                    t[1] = now;
                    MicRtsim.LOGGER.info("[gametest] {}: waiting {} s, {}", name(), String.format("%.0f", (now - t[0]) / 1e9), e.getMessage());
                }
                throw e;
            }
            long tick = helper.getTick();
            MicRtsim.LOGGER.info("[gametest] {}: chunks ready after {} s ({} ticks)", name(), String.format("%.1f", (now - t[0]) / 1e9), tick - mark);
            mark = tick;
        };
    }

    /**
     * A wait on in-game logic, for {@code thenWaitUntil}: bounded by the test's tick budget counted from
     * the tick the last chunk wait ended (or the test's start). Past it the test fails with the wait's own
     * message.
     */
    public Runnable logic(Runnable wait) {
        return () -> {
            try {
                wait.run();
            } catch (GameTestAssertException e) {
                if (helper.getTick() - mark > logicTicks)
                    fail(e.getMessage() + " (not within " + logicTicks + " ticks of the last chunk wait)");
                throw e;
            }
        };
    }

    /**
     * A {@link #logic} wait on the engine's answer: the engine answers on its own thread and a tick never
     * waits for it, so a failing check is paced as {@link RtsimTests#io} paces one (the tick sleeps 5 ms,
     * and a budget of N ticks is at least N * 5 ms of real time).
     */
    public Runnable engine(Runnable wait) {
        return RtsimTests.io(logic(wait));
    }

    /** Fails the test now, past the sequence that would swallow a {@code helper.fail} until the timeout. */
    public void fail(String message) {
        GameTestAssertException e = new GameTestAssertException(message);
        helper.testInfo.fail(e);
        throw e;
    }

    private String name() {
        return helper.testInfo.getTestName();
    }

    /**
     * Chunks held loaded by a ticket: never saved with the world (a forced chunk is, and would stay
     * loaded next to other tests in the next run), no lifespan, released when the test ends.
     */
    public static final class Hold {
        private final ServerLevel level;
        private final BlockPos at;
        private final int blocks;
        private final Need need;
        private final ChunkPos centre;
        private final int radius;
        private final long id = NEXT.incrementAndGet();
        private boolean held = true;

        private Hold(ServerLevel level, BlockPos at, int blocks, Need need) {
            this.level = level;
            this.at = at.immutable();
            this.blocks = blocks;
            this.need = need;
            this.centre = new ChunkPos(at);
            // Every chunk of the square is within this many chunks of the centre's.
            this.radius = ((blocks + 15) >> 4) + need.margin;
            level.getChunkSource().addRegionTicket(HOLD, centre, radius, id);
        }

        /** Lets the chunks go (once; the test's end does it too). */
        public void release() {
            if (held) {
                held = false;
                level.getChunkSource().removeRegionTicket(HOLD, centre, radius, id);
            }
        }

        /** How many of the square's chunks are ready. */
        public int readyCount() {
            int n = 0;
            for (int cx = (at.getX() - blocks) >> 4; cx <= (at.getX() + blocks) >> 4; cx++)
                for (int cz = (at.getZ() - blocks) >> 4; cz <= (at.getZ() + blocks) >> 4; cz++)
                    if (ready(cx, cz))
                        n++;
            return n;
        }

        /** How many chunks the square has. */
        public int total() {
            return (((at.getX() + blocks) >> 4) - ((at.getX() - blocks) >> 4) + 1)
                    * (((at.getZ() + blocks) >> 4) - ((at.getZ() - blocks) >> 4) + 1);
        }

        private boolean ready(int cx, int cz) {
            if (level.getChunkSource().getChunkNow(cx, cz) == null)
                return false;
            return need != Need.ENTITY_TICKING
                    || level.isPositionEntityTicking(new ChunkPos(cx, cz).getWorldPosition()) && level.areEntitiesLoaded(ChunkPos.asLong(cx, cz));
        }

        void check() {
            int n = readyCount(), total = total();
            if (n < total)
                throw new GameTestAssertException(String.format("%d of %d chunks within %d blocks of %s %s", n, total, blocks,
                        at.toShortString(), need == Need.FULL ? "loaded" : "entity ticking"));
        }

        /** The centre chunk's holder: its ticket level and statuses, or "null" for no holder. */
        String holder() {
            return "the centre chunk's holder: " + level.getChunkSource().getChunkDebugData(centre).replaceAll("§.", "").replace('\n', ' ').trim();
        }
    }
}
