package com.minecraftindustrialcomplex.mic_rtsim.boundary;

import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
import com.minecraftindustrialcomplex.mic_rtsim.journal.Journal;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.Test;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CountDownLatch;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The boundary protocol, the journal and the stub engine, in pure Java: no Minecraft. The GameTests
 * cover the same paths inside a real server.
 */
class BoundaryProtocolTest {

    private static final long ENGINE_900 = 900L; // engine op ids leave the top bit clear

    private final List<StubEngine> engines = new ArrayList<>();
    private long time = 1000;

    @AfterEach
    void closeEngines() {
        engines.forEach(StubEngine::close);
    }

    private StubEngine stub(StubEngine.Pages pages, int capacity) {
        StubEngine e = new StubEngine(pages, null, capacity);
        engines.add(e);
        return e;
    }

    /** A command sink that records commands and applies TRANSFER_TO_CONTAINER into a map, once per op id. */
    static final class Sink implements Handover.CommandSink {
        final BoundarySession session;
        final Map<Long, Integer> containers = new HashMap<>();
        final List<Integer> kinds = new ArrayList<>();
        boolean defer;
        long lastAck;

        Sink(BoundarySession session) {
            this.session = session;
        }

        @Override
        public void onHeader(long handover, long ackedObsSeq, long gameTime, int flags) {
            lastAck = ackedObsSeq;
        }

        @Override
        public boolean accept(ByteBuffer b, int at, int kind, int size, int aux) {
            if (defer && kind != Cmd.DICT)
                return false;
            kinds.add(kind);
            if (kind == Cmd.TRANSFER_TO_CONTAINER) {
                long op = b.getLong(at + Cmd.TransferToContainer.OP_ID);
                int status;
                if (session.journal().commandOpApplied(op)) {
                    status = Obs.CommandResult.DUPLICATE_OP;
                } else {
                    containers.merge((long) b.getInt(at + Cmd.TransferToContainer.X), b.getInt(at + Cmd.TransferToContainer.COUNT), Integer::sum);
                    status = Obs.CommandResult.APPLIED;
                }
                ByteBuffer r = session.beginJournaled(Obs.COMMAND_RESULT, Obs.CommandResult.SIZE, status, 0);
                r.putLong(Obs.CommandResult.CMD_SEQ, b.getLong(at + Cmd.CMD_SEQ));
                r.putLong(Obs.CommandResult.OP_ID, op);
                session.commit(r);
            }
            return true;
        }

        @Override
        public void onMalformed(long handover, String error) {
            throw new AssertionError(error);
        }
    }

    private boolean tick(BoundarySession s, Sink sink, StubEngine e) {
        s.applyCommands(sink);
        boolean submitted = s.handOver(time++);
        e.awaitIdle();
        return submitted;
    }

    private static long transfer(BoundarySession s, long to, long amount) {
        ByteBuffer r = s.beginJournaled(Obs.TRANSFER, Obs.Transfer.SIZE, Obs.Transfer.GENERIC, 0);
        r.putLong(Obs.Transfer.TO, to);
        r.putLong(Obs.Transfer.AMOUNT, amount);
        return s.commitWithOpId(r, Obs.Transfer.OP_ID);
    }

    private static void transferWithOp(BoundarySession s, long op, long to, long amount) {
        ByteBuffer r = s.beginJournaled(Obs.TRANSFER, Obs.Transfer.SIZE, Obs.Transfer.GENERIC, 0);
        r.putLong(Obs.Transfer.OP_ID, op);
        r.putLong(Obs.Transfer.TO, to);
        r.putLong(Obs.Transfer.AMOUNT, amount);
        s.commit(r);
    }

    @Test
    void buffersAreAlignedForAtomicAccess() {
        ByteBuffer b = Buffers.allocate(1024);
        assertTrue(b.isDirect());
        assertEquals(0, b.alignmentOffset(0, Layout.BUFFER_ALIGN));
        assertEquals(1024, Buffers.capacity(b));
        Seq.setRelease(b, Layout.H_PUBLISHED, 42L);
        Seq.setRelease(b, Layout.H_CONSUMED, 41L);
        assertEquals(42L, Seq.getAcquire(b, Layout.H_PUBLISHED));
        assertEquals(41L, Seq.getAcquire(b, Layout.H_CONSUMED));
        assertEquals(42L, b.order(Layout.ORDER).getLong(Layout.H_PUBLISHED), "the atomic word is a plain little-endian u64");
    }

    @Test
    void commandsArriveOnTheTickAfterTheStep() {
        StubEngine e = stub(StubEngine.Pages.EMPTY, StubEngine.DEFAULT_CAPACITY);
        BoundarySession s = new BoundarySession(e, new Journal(), 0, time);
        Sink sink = new Sink(s);
        e.transferToContainer(77L, "minecraft:overworld", 5, 64, 9, "minecraft:diamond", 3);

        assertTrue(tick(s, sink, e), "hand-over 1 is submitted on the first tick");
        assertTrue(sink.kinds.isEmpty(), "nothing is applied before the step ran");
        assertTrue(tick(s, sink, e), "hand-over 2 follows once step 1 finished");
        assertEquals(List.of(Cmd.DICT, Cmd.DICT, Cmd.TRANSFER_TO_CONTAINER), sink.kinds);
        assertEquals(3, sink.containers.get(5L));
        assertEquals(3, s.journal().lastAppliedCmdSeq(), "two DICT commands and the transfer, sequences 1 to 3");
        assertEquals(1, e.received(Obs.SESSION_START).size());
    }

    @Test
    void journaledObservationsStayUntilAcknowledged() {
        StubEngine e = stub(StubEngine.Pages.EMPTY, StubEngine.DEFAULT_CAPACITY);
        BoundarySession s = new BoundarySession(e, new Journal(), 0, time);
        Sink sink = new Sink(s);
        transfer(s, 7, 5);
        tick(s, sink, e);
        assertEquals(5, e.ledger(7));
        tick(s, sink, e);
        assertEquals(1, s.journal().size(), "applied but not on the engine's disk: still journaled");

        e.requestSaveAtNextStep();
        tick(s, sink, e); // the step saves its pages and acknowledges in its command header
        tick(s, sink, e); // Java reads that header and trims
        assertEquals(0, s.journal().size());
        assertEquals(e.pages().obsSeq(), sink.lastAck);
    }

    @Test
    void crashReplayAppliesExactlyOnceAndOneOpIdNeverTwice() {
        // Session 1: a transfer reaches the engine, which never saves its pages before the crash.
        StubEngine e1 = stub(StubEngine.Pages.EMPTY, StubEngine.DEFAULT_CAPACITY);
        BoundarySession s1 = new BoundarySession(e1, new Journal(), 0, time);
        Sink k1 = new Sink(s1);
        long op = transfer(s1, 7, 5);
        tick(s1, k1, e1);
        tick(s1, k1, e1);
        assertEquals(5, e1.ledger(7));
        Journal.Snapshot worldSave = s1.journal().snapshot(); // the world saved its journal, then the crash

        // Session 2: the engine comes back from its last pages (empty); the journal replays the transfer.
        StubEngine e2 = stub(e1.pages(), StubEngine.DEFAULT_CAPACITY);
        BoundarySession s2 = new BoundarySession(e2, Journal.restore(worldSave), 0, time);
        Sink k2 = new Sink(s2);
        tick(s2, k2, e2);
        tick(s2, k2, e2);
        assertEquals(5, e2.ledger(7), "replayed into an engine that lost it: applied once");
        assertTrue(e2.received(Obs.TRANSFER).get(0).bytes().length > 0);

        // The engine saves pages that include the transfer, but the crash comes before the
        // acknowledgement reaches the journal: the journal still holds it.
        e2.requestSaveAtNextStep();
        tick(s2, k2, e2);
        Journal.Snapshot worldSave2 = s2.journal().snapshot();
        assertEquals(1, Journal.restore(worldSave2).size());

        // Session 3: the replay arrives at an engine whose pages already include it: skipped.
        StubEngine e3 = stub(e2.pages(), StubEngine.DEFAULT_CAPACITY);
        BoundarySession s3 = new BoundarySession(e3, Journal.restore(worldSave2), 0, time);
        Sink k3 = new Sink(s3);
        tick(s3, k3, e3);
        tick(s3, k3, e3);
        assertEquals(5, e3.ledger(7), "a replay of what the pages include changes nothing");
        assertTrue(e3.staleSkipped() >= 1);

        // The same op id a second time, under a new sequence: the op id decides, still once.
        transferWithOp(s3, op, 7, 5);
        tick(s3, k3, e3);
        tick(s3, k3, e3);
        assertEquals(5, e3.ledger(7));
        assertEquals(1, e3.opDuplicates());
    }

    @Test
    void anEngineOpIdIsAppliedInMinecraftAtMostOnce() {
        StubEngine e1 = stub(StubEngine.Pages.EMPTY, StubEngine.DEFAULT_CAPACITY);
        BoundarySession s1 = new BoundarySession(e1, new Journal(), 0, time);
        Sink k1 = new Sink(s1);
        e1.transferToContainer(ENGINE_900, "minecraft:overworld", 1, 64, 1, "minecraft:diamond", 3);
        tick(s1, k1, e1);
        tick(s1, k1, e1);
        assertEquals(3, k1.containers.get(1L));
        Journal.Snapshot worldSave = s1.journal().snapshot();

        // The engine crashed before saving; rebuilt from its old pages it issues op 900 again.
        StubEngine e2 = stub(e1.pages(), StubEngine.DEFAULT_CAPACITY);
        BoundarySession s2 = new BoundarySession(e2, Journal.restore(worldSave), 0, time);
        assertTrue(s2.start().nextCmdSeq() > s1.journal().lastAppliedCmdSeq(), "new sequences start past the last applied one");
        Sink k2 = new Sink(s2);
        k2.containers.putAll(k1.containers); // the world kept the items
        e2.transferToContainer(ENGINE_900, "minecraft:overworld", 1, 64, 1, "minecraft:diamond", 3);
        tick(s2, k2, e2);
        tick(s2, k2, e2);
        tick(s2, k2, e2);
        assertEquals(3, k2.containers.get(1L), "the second issue of op 900 is a duplicate");
        assertTrue(e2.opApplied(ENGINE_900), "the replayed COMMAND_RESULT told the engine op 900 is done");
    }

    @Test
    void theBridgeNeverWaitsForASlowStep() {
        StubEngine e = stub(StubEngine.Pages.EMPTY, StubEngine.DEFAULT_CAPACITY);
        BoundarySession s = new BoundarySession(e, new Journal(), 0, time);
        Sink sink = new Sink(s);
        CountDownLatch gate = new CountDownLatch(1);
        e.gate(gate);
        s.applyCommands(sink);
        assertTrue(s.handOver(time++), "hand-over 1 goes out");
        for (int i = 0; i < 5; i++) {
            transfer(s, 3, 1);
            long t0 = System.nanoTime();
            s.applyCommands(sink);
            assertFalse(s.handOver(time++), "step 1 is still running: no new hand-over");
            assertTrue(System.nanoTime() - t0 < 50_000_000L, "a tick never blocks on the engine");
        }
        assertEquals(5, s.handover().ticksWaiting());
        gate.countDown();
        e.awaitIdle();
        assertTrue(tick(s, sink, e), "hand-over 2 carries everything collected while waiting");
        tick(s, sink, e);
        assertEquals(5, e.ledger(3));
    }

    @Test
    void unappliedCommandsHoldBackTheStepThatWouldOverwriteThem() {
        StubEngine e = stub(StubEngine.Pages.EMPTY, StubEngine.DEFAULT_CAPACITY);
        BoundarySession s = new BoundarySession(e, new Journal(), 0, time);
        Sink sink = new Sink(s);
        sink.defer = true;
        e.transferToContainer(1L, "minecraft:overworld", 2, 0, 0, "minecraft:stone", 1);
        assertTrue(tick(s, sink, e));  // h1
        assertTrue(tick(s, sink, e));  // h2: command buffer 0 is free; step 1's commands wait in buffer 1
        assertFalse(tick(s, sink, e), "h3 would overwrite step 1's unapplied commands");
        assertTrue(s.handover().commandsPending());
        sink.defer = false;
        assertTrue(tick(s, sink, e), "applied, so h3 goes out");
        assertEquals(1, sink.containers.get(2L));
    }

    @Test
    void aFullBufferDelaysJournaledRecordsButNeverDropsThem() {
        StubEngine e = stub(StubEngine.Pages.EMPTY, 512);
        BoundarySession s = new BoundarySession(e, new Journal(), 0, time);
        Sink sink = new Sink(s);
        for (int i = 0; i < 50; i++)
            transfer(s, 9, 1);
        for (int i = 0; i < 40 && e.ledger(9) < 50; i++)
            tick(s, sink, e);
        assertEquals(50, e.ledger(9));
        List<StubEngine.Received> got = e.received(Obs.TRANSFER);
        assertEquals(50, got.size(), "each one exactly once");
        for (int i = 1; i < got.size(); i++)
            assertTrue(got.get(i).obsSeq() > got.get(i - 1).obsSeq(), "in sequence order");
    }

    @Test
    void staleCommandSequencesAreSkipped() {
        StubEngine e = stub(StubEngine.Pages.EMPTY, StubEngine.DEFAULT_CAPACITY);
        BoundarySession s = new BoundarySession(e, new Journal(), 0, time);
        Sink sink = new Sink(s);
        e.transferToContainer(5L, "minecraft:overworld", 4, 0, 0, "minecraft:stone", 1);
        tick(s, sink, e);
        tick(s, sink, e);
        long last = s.journal().lastAppliedCmdSeq();
        e.rewindCommandSequence(last); // the next command reuses an applied sequence
        e.transferToContainer(6L, "minecraft:overworld", 4, 0, 0, "minecraft:stone", 1);
        tick(s, sink, e);
        tick(s, sink, e);
        assertEquals(1, s.skippedStaleCommands());
        assertEquals(1, sink.containers.get(4L));
    }

    @Test
    void theDictionaryPrecedesItsFirstUseAndComesBackEverySession() {
        StubEngine e1 = stub(StubEngine.Pages.EMPTY, StubEngine.DEFAULT_CAPACITY);
        Journal j = new Journal();
        BoundarySession s1 = new BoundarySession(e1, j, 0, time);
        Sink k1 = new Sink(s1);
        int dim = s1.dictId(Obs.DICT_DIMENSION, "minecraft:overworld");
        int at = s1.direct(Obs.PLAYER, Obs.Player.SIZE, dim, time);
        assertTrue(at > 0);
        tick(s1, k1, e1);
        assertEquals("minecraft:overworld", e1.javaDictName(dim));
        List<StubEngine.Received> all = e1.received();
        int dictAt = -1, playerAt = -1;
        for (int i = 0; i < all.size(); i++) {
            if (all.get(i).kind() == Obs.DICT && dictAt < 0)
                dictAt = i;
            if (all.get(i).kind() == Obs.PLAYER)
                playerAt = i;
        }
        assertTrue(dictAt >= 0 && dictAt < playerAt, "DICT before the record that uses it");

        StubEngine e2 = stub(e1.pages(), StubEngine.DEFAULT_CAPACITY);
        BoundarySession s2 = new BoundarySession(e2, Journal.restore(j.snapshot()), 0, time);
        tick(s2, new Sink(s2), e2);
        assertEquals("minecraft:overworld", e2.javaDictName(dim), "the same id in the next session");
    }

    @Test
    void sessionStartCarriesWhatTheKernelReconcilesAndSavesAreReported() {
        StubEngine e1 = stub(StubEngine.Pages.EMPTY, StubEngine.DEFAULT_CAPACITY);
        BoundarySession s1 = new BoundarySession(e1, new Journal(), 0b101, time);
        Sink k1 = new Sink(s1);
        transfer(s1, 1, 1);
        transfer(s1, 1, 1);
        tick(s1, k1, e1);
        s1.worldSaveDurable(2);
        tick(s1, k1, e1);
        List<StubEngine.Received> saved = e1.received(Obs.WORLD_SAVED);
        assertEquals(1, saved.size());
        assertEquals(2L, saved.get(0).view().getLong(Obs.WorldSaved.DURABLE_OBS_SEQ));
        tick(s1, k1, e1);
        assertEquals(1, e1.received(Obs.WORLD_SAVED).size(), "reported once per save");

        // A world save that knew sequences 1 and 2, against engine pages that include only 1.
        StubEngine e2 = stub(new StubEngine.Pages(1, 1, java.util.Set.of(), java.util.Map.of()), StubEngine.DEFAULT_CAPACITY);
        Journal j = Journal.restore(s1.journal().snapshot());
        BoundarySession s2 = new BoundarySession(e2, j, 0b101, time);
        tick(s2, new Sink(s2), e2);
        ByteBuffer start = e2.received(Obs.SESSION_START).get(0).view();
        assertEquals(1L, start.getLong(Obs.SessionStart.JOURNAL_FIRST_OBS_SEQ));
        assertEquals(3L, start.getLong(Obs.SessionStart.WORLD_NEXT_OBS_SEQ));
        assertEquals(0b101, start.getInt(Obs.SessionStart.MODULE_MASK));
        assertEquals(3L, j.nextObsSeq(), "the world is ahead of the pages: no sequence moves");

        // Engine pages ahead of the world save (P29): Java moves past them and never reuses one.
        StubEngine e3 = stub(new StubEngine.Pages(10, 1, java.util.Set.of(), java.util.Map.of()), StubEngine.DEFAULT_CAPACITY);
        Journal j3 = Journal.restore(s1.journal().snapshot());
        BoundarySession s3 = new BoundarySession(e3, j3, 0, time);
        tick(s3, new Sink(s3), e3);
        assertEquals(3L, e3.received(Obs.SESSION_START).get(0).view().getLong(Obs.SessionStart.WORLD_NEXT_OBS_SEQ),
                "the engine sees the world's own next sequence, so it can tell it is ahead");
        assertEquals(11L, j3.nextObsSeq());
    }

    @Test
    void commandsThatDoNotFitWaitForTheNextStepInOrder() {
        StubEngine e = stub(StubEngine.Pages.EMPTY, 512);
        BoundarySession s = new BoundarySession(e, new Journal(), 0, time);
        Sink sink = new Sink(s);
        for (int i = 0; i < 30; i++)
            e.transferToContainer(1000 + i, "minecraft:overworld", 8, 0, 0, "minecraft:stone", 1);
        for (int i = 0; i < 40 && sink.containers.getOrDefault(8L, 0) < 30; i++)
            tick(s, sink, e);
        assertEquals(30, sink.containers.get(8L), "every command arrives, over several steps");
        assertEquals(0, e.scriptedPending());
        assertTrue(s.journal().lastAppliedCmdSeq() >= 32, "two DICT and thirty transfers, each with its own sequence");
        assertEquals(0, s.skippedStaleCommands(), "no sequence was reused when a step rolled a command back");
    }

    @Test
    void aSaveRequestNeverTakesThePagesPastTheWorldSave() {
        StubEngine e = stub(StubEngine.Pages.EMPTY, StubEngine.DEFAULT_CAPACITY);
        BoundarySession s = new BoundarySession(e, new Journal(), 0, time);
        Sink sink = new Sink(s);
        transfer(s, 1, 1);
        transfer(s, 1, 1);
        e.requestSave(2); // the world save knew sequences 1 and 2
        transfer(s, 1, 1); // 3 and 4 come after it, in the same hand-over
        transfer(s, 1, 1);
        tick(s, sink, e);
        assertEquals(4, e.ledger(1), "everything is applied");
        assertEquals(2, e.pages().obsSeq(), "but the pages stop where the world save did");
        tick(s, sink, e);
        assertEquals(2, s.journal().size(), "and only what they include is trimmed");
    }

    @Test
    void buffersFromAnEarlierSessionAreRefused() {
        StubEngine e = stub(StubEngine.Pages.EMPTY, StubEngine.DEFAULT_CAPACITY);
        Seq.setRelease(e.commandBuffer(1), Layout.H_PUBLISHED, 1);
        org.junit.jupiter.api.Assertions.assertThrows(IllegalStateException.class,
                () -> new BoundarySession(e, new Journal(), 0, time));
    }
}
