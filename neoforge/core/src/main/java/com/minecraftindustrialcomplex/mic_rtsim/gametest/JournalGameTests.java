package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.journal.JournalSavedData;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.Container;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The journal (A9, PLAN.md §4.9): replay after a crash applies every cross-store transfer exactly once,
 * in both directions, even when the same op id arrives twice.
 *
 * <p>The test runs its own bridge sessions next to the server's: each has its own engine (of the
 * server's kind, {@link RtsimTests#freshEngine}) and its own journal SavedData, and the test drives
 * their ticks itself. A "world save" is the journal through the same NBT encoding vanilla writes
 * ({@link JournalSavedData#roundTrip}); a "crash" is dropping the session, closing the engine without
 * saving ({@link ScriptedEngine#crashAndReopen}) and building a new session from the engine's last
 * pages and that saved journal. The chest is real: it is the Minecraft side of the engine-to-Minecraft
 * transfer.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class JournalGameTests {
    private JournalGameTests() {}

    private static final long STORE = 7;
    /** An engine-allocated op id: top bit clear. */
    private static final long ENGINE_OP = 4242;

    private static int diamonds(Container chest) {
        int n = 0;
        for (int i = 0; i < chest.getContainerSize(); i++)
            if (chest.getItem(i).is(Items.DIAMOND))
                n += chest.getItem(i).getCount();
        return n;
    }

    /** The test's own state machine; one {@link #tick} per server tick. */
    private static final class Run {
        final GameTestHelper helper;
        final BlockPos chestPos;
        int stage;
        int wait;
        long javaOp;
        RtsimSession session;
        ScriptedEngine engine;
        JournalSavedData saved;

        Run(GameTestHelper helper) {
            this.helper = helper;
            this.chestPos = helper.absolutePos(new BlockPos(5, 1, 5));
        }

        Container chest() {
            return (Container) helper.getLevel().getBlockEntity(chestPos);
        }

        RtsimSession open(ScriptedEngine e, JournalSavedData journal) {
            engine = e;
            session = new RtsimSession(helper.getLevel().getServer(), engine, journal);
            return session;
        }

        /** The server dies: the session is gone, and the engine comes back from its last pages. */
        ScriptedEngine crash() {
            session = null;
            return engine.crashAndReopen(); // no save: whatever the engine did not put in its pages is lost
        }

        JournalSavedData worldSave(RtsimSession s) {
            return JournalSavedData.roundTrip(s.journal(), helper.getLevel().registryAccess());
        }

        boolean tick() {
            if (session != null)
                session.endOfTick();
            if (++wait > 200)
                throw new GameTestAssertException("journal test stuck at stage " + stage);
            String dim = RtsimTests.dimension(helper);
            switch (stage) {
                case 0 -> {
                    // Session 1: one transfer each way.
                    open(RtsimTests.freshEngine(), JournalSavedData.fresh());
                    javaOp = session.journalTransfer(Obs.Transfer.GENERIC, 1, STORE, Items.EMERALD, 5);
                    engine.transferToContainer(ENGINE_OP, dim, chestPos.getX(), chestPos.getY(), chestPos.getZ(), "minecraft:diamond", 3);
                    next();
                }
                case 1 -> {
                    if (engine.ledger(STORE) == 5 && diamonds(chest()) == 3 && engine.opApplied(ENGINE_OP)) {
                        record("journal", "session 1", "engine ledger 5, chest 3 diamonds, engine knows op " + ENGINE_OP + " applied");
                        // The world saves its journal; then the server dies before the engine saved pages.
                        saved = worldSave(session);
                        check("the saved journal holds both unacknowledged observations", saved.journal().size() >= 2);
                        // Session 2: the engine is back at its last pages (empty). It re-derives the same
                        // engine op under a new command sequence, as an engine rolled back by a crash does.
                        open(crash(), saved);
                        engine.transferToContainer(ENGINE_OP, dim, chestPos.getX(), chestPos.getY(), chestPos.getZ(), "minecraft:diamond", 3);
                        next();
                    }
                }
                case 2 -> {
                    List<ScriptedEngine.Received> results = engine.received(Obs.COMMAND_RESULT);
                    boolean dupSeen = results.stream().anyMatch(r -> r.aux() == Obs.CommandResult.DUPLICATE_OP && !r.skipped());
                    if (engine.ledger(STORE) == 5 && dupSeen) {
                        check("the replayed Java transfer was applied once", engine.ledger(STORE) == 5);
                        check("the re-issued engine op was not applied again", diamonds(chest()) == 3);
                        record("journal", "session 2", "replay applied once; engine op " + ENGINE_OP + " answered DUPLICATE_OP");
                        // Let the step in flight finish without a save, so the next step is the one that
                        // saves: its acknowledgement is then read on the tick after the snapshot below.
                        // (Blocking on the engine is for this test only; the bridge never does.)
                        engine.awaitIdle();
                        engine.requestSaveAtNextStep();
                        next();
                    }
                }
                case 3 -> {
                    // The engine's pages now include the transfer; crash before the acknowledgement reaches
                    // the journal, so the saved journal still holds it.
                    engine.awaitIdle();
                    if (engine.pagesObsSeq() > 0) {
                        saved = worldSave(session);
                        check("the journal still holds the transfer", saved.journal().size() >= 1);
                        open(crash(), saved);
                        next();
                    }
                }
                case 4 -> {
                    boolean replayed = engine.received(Obs.TRANSFER).stream().anyMatch(ScriptedEngine.Received::skipped);
                    if (replayed) {
                        check("a replay the pages include changes nothing", engine.ledger(STORE) == 5);
                        // The same op id once more, under a new sequence.
                        session.journalTransferWithOpId(javaOp, Obs.Transfer.GENERIC, 1, STORE, 5);
                        next();
                    }
                }
                case 5 -> {
                    if (engine.opDuplicates() == 1) {
                        check("the same op id applied twice is applied once", engine.ledger(STORE) == 5);
                        check("the chest still has exactly the one transfer", diamonds(chest()) == 3);
                        record("journal", "result", "ledger " + engine.ledger(STORE) + ", chest " + diamonds(chest()) + " diamonds, "
                                + engine.opDuplicates() + " duplicate op id refused, " + engine.staleSkipped() + " stale replays skipped");
                        engine.close();
                        session = null;
                        return true;
                    }
                }
                default -> throw new IllegalStateException();
            }
            return false;
        }

        void next() {
            stage++;
            wait = 0;
        }
    }

    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 600)
    public static void journalReplayAppliesExactlyOnceAfterACrash(GameTestHelper helper) {
        helper.setBlock(new BlockPos(5, 1, 5), Blocks.CHEST);
        Run run = new Run(helper);
        boolean[] done = {false};
        helper.onEachTick(() -> {
            if (!done[0] && run.tick())
                done[0] = true;
        });
        helper.succeedWhen(() -> check("the journal test finished", done[0]));
    }

    /**
     * Slice U-i (docs/choices-journal-barrier.md U-i1): a stop names what the journal's file holds to the engine
     * ({@code RtsimSession.close} through {@code BoundarySession.closeEngine}, over the real {@code nClose}). One
     * transfer is saved and acknowledged (the first world save, whose file landed), a second one follows, and the
     * session stops. The journal's file on disk is the first save's, so the stop's final save, which includes the
     * second transfer, stays a pending generation, and the next session started from that older file opens the
     * first save's pages and not the stop's: the engine's ledger is the file's. The control stops with the file
     * landed (it holds everything the pages do): the stop's save is confirmed at once and the next session keeps
     * it. Native engine only: the stub keeps one generation. The sessions are the test's own, like the crash
     * test's.
     */
    private static final class StopRun {
        final GameTestHelper helper;
        final boolean landed;
        int stage;
        int wait;
        RtsimSession session;
        ScriptedEngine engine;
        JournalSavedData older;
        JournalSavedData stopped;
        long olderThrough;
        long stoppedThrough;
        long firstPages;

        StopRun(GameTestHelper helper, boolean landed) {
            this.helper = helper;
            this.landed = landed;
        }

        void open(ScriptedEngine e, JournalSavedData journal) {
            engine = e;
            session = new RtsimSession(helper.getLevel().getServer(), engine, journal);
        }

        boolean tick() {
            if (session != null)
                session.endOfTick();
            if (++wait > 400)
                throw new GameTestAssertException("stop test stuck at stage " + stage);
            switch (stage) {
                case 0 -> {
                    // Session 1: one transfer.
                    open(RtsimTests.freshEngine(), JournalSavedData.fresh());
                    session.journalTransfer(Obs.Transfer.GENERIC, 1, STORE, Items.EMERALD, 5);
                    next();
                }
                case 1 -> {
                    if (engine.ledger(STORE) == 5) {
                        // The first world save: the journal's file landed, and the engine saves its pages and
                        // is acknowledged (a script save, confirmed at once).
                        RtsimTests.acknowledge(session, engine);
                        next();
                    }
                }
                case 2 -> {
                    engine.awaitIdle();
                    firstPages = engine.pagesObsSeq();
                    if (firstPages > 0) {
                        // The file on disk is this save's. Then a second transfer, which no file holds.
                        older = JournalSavedData.roundTrip(session.journal(), helper.getLevel().registryAccess());
                        olderThrough = older.journal().nextObsSeq() - 1;
                        check("the first save's pages are within the older file: " + firstPages + " <= " + olderThrough, firstPages <= olderThrough);
                        session.journalTransfer(Obs.Transfer.GENERIC, 1, STORE, Items.EMERALD, 3);
                        next();
                    }
                }
                case 3 -> {
                    if (engine.ledger(STORE) == 8) {
                        stopped = JournalSavedData.roundTrip(session.journal(), helper.getLevel().registryAccess());
                        stoppedThrough = stopped.journal().nextObsSeq() - 1;
                        check("the stop's journal is past the older file", stoppedThrough > olderThrough);
                        // The file on disk: the first save's, or, landed, the stop's own. The stop reads it.
                        if (landed)
                            session.journal().fileLandedForTest(stoppedThrough);
                        check("the file on disk is " + (landed ? "the stop's" : "the first save's, short of the stop's"),
                                (session.journal().writtenThrough() >= stoppedThrough) == landed);
                        session.close();
                        session = null;
                        next();
                    }
                }
                case 4 -> {
                    // The next start, from the file that is on disk.
                    String dir = ((NativeEngine) engine).pagesDir().toString();
                    ScriptedEngine again = NativeEngine.open(java.nio.file.Path.of(dir), 0, NativeEngine.FLAG_TEST | NativeEngine.FLAG_NO_FSYNC);
                    open(again, landed ? stopped : older);
                    long pages = session.boundary().start().pagesObsSeq();
                    record("journalStop", landed ? "landed" : "held", "pages through " + pages + ", first save " + firstPages
                            + ", older file " + olderThrough + ", stop's " + stoppedThrough);
                    check("the start opened the pages of the " + (landed ? "stop's save, which the file holds" : "first save, which the file holds")
                            + ": " + pages, landed ? pages > firstPages && pages <= stoppedThrough : pages == firstPages);
                    next();
                }
                case 5 -> {
                    // A few ticks of the replay: the held arm's older file replays nothing the pages lack.
                    if (wait >= 10) {
                        long want = landed ? 8 : 5;
                        check("the engine's ledger is the file's: " + engine.ledger(STORE), engine.ledger(STORE) == want);
                        record("journalStop", "ledger", "ledger " + engine.ledger(STORE));
                        engine.close();
                        session = null;
                        return true;
                    }
                }
                default -> throw new IllegalStateException();
            }
            return false;
        }

        void next() {
            stage++;
            wait = 0;
        }
    }

    private static void stopTest(GameTestHelper helper, boolean landed) {
        if (!(RtsimTests.engine() instanceof NativeEngine)) {
            record("journalStop", "skipped", "the stub keeps one generation");
            helper.succeed();
            return;
        }
        StopRun run = new StopRun(helper, landed);
        boolean[] done = {false};
        helper.onEachTick(() -> {
            if (!done[0] && run.tick())
                done[0] = true;
        });
        helper.succeedWhen(() -> check("the stop test finished", done[0]));
    }

    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 600)
    public static void aStopWhoseJournalFileLacksItsFinalSaveLeavesItPending(GameTestHelper helper) {
        stopTest(helper, false);
    }

    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 600)
    public static void aStopWhoseJournalFileHoldsItsFinalSaveConfirmsItAtOnce(GameTestHelper helper) {
        stopTest(helper, true);
    }
}
