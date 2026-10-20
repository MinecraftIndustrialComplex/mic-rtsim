package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
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
 * <p>The test runs its own bridge sessions next to the server's: each has its own stub engine and its
 * own journal SavedData, and the test drives their ticks itself. A "world save" is the journal through
 * the same NBT encoding vanilla writes ({@link JournalSavedData#roundTrip}); a "crash" is dropping the
 * session and its engine and building new ones from the engine's last pages and that saved journal. The
 * chest is real: it is the Minecraft side of the engine-to-Minecraft transfer.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class JournalGameTests {
    private JournalGameTests() {}

    private static final long STORE = 7;
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
        StubEngine engine;
        JournalSavedData saved;

        Run(GameTestHelper helper) {
            this.helper = helper;
            this.chestPos = helper.absolutePos(new BlockPos(5, 1, 5));
        }

        Container chest() {
            return (Container) helper.getLevel().getBlockEntity(chestPos);
        }

        RtsimSession open(StubEngine.Pages pages, JournalSavedData journal) {
            engine = new StubEngine(pages, null, StubEngine.DEFAULT_CAPACITY);
            session = new RtsimSession(helper.getLevel().getServer(), engine, journal);
            return session;
        }

        void crash() {
            engine.close(); // no save: whatever the engine did not put in its pages is lost
            session = null;
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
                    open(StubEngine.Pages.EMPTY, JournalSavedData.fresh());
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
                        crash();
                        // Session 2: the engine is back at its last pages (empty). It re-derives the same
                        // engine op under a new command sequence, as an engine rolled back by a crash does.
                        open(StubEngine.Pages.EMPTY, saved);
                        engine.transferToContainer(ENGINE_OP, dim, chestPos.getX(), chestPos.getY(), chestPos.getZ(), "minecraft:diamond", 3);
                        next();
                    }
                }
                case 2 -> {
                    List<StubEngine.Received> results = engine.received(Obs.COMMAND_RESULT);
                    boolean dupSeen = results.stream().anyMatch(r -> r.aux() == Obs.CommandResult.DUPLICATE_OP && !r.skipped());
                    if (engine.ledger(STORE) == 5 && dupSeen) {
                        check("the replayed Java transfer was applied once", engine.ledger(STORE) == 5);
                        check("the re-issued engine op was not applied again", diamonds(chest()) == 3);
                        record("journal", "session 2", "replay applied once; engine op " + ENGINE_OP + " answered DUPLICATE_OP");
                        // Let the step in flight finish without a save, so the next step is the one that
                        // saves: its acknowledgement is then read on the tick after the snapshot below.
                        // (Blocking on the engine is for this test only; the bridge never does.)
                        engine.awaitIdle();
                        engine.requestSave();
                        next();
                    }
                }
                case 3 -> {
                    // The engine's pages now include the transfer; crash before the acknowledgement reaches
                    // the journal, so the saved journal still holds it.
                    engine.awaitIdle();
                    if (engine.pages().obsSeq() > 0) {
                        saved = worldSave(session);
                        check("the journal still holds the transfer", saved.journal().size() >= 1);
                        crash();
                        open(engine.pages(), saved);
                        next();
                    }
                }
                case 4 -> {
                    boolean replayed = engine.received(Obs.TRANSFER).stream().anyMatch(StubEngine.Received::skipped);
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
}
