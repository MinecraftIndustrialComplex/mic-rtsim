package com.minecraftindustrialcomplex.mic_rtsim_create;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/**
 * The applied op ring's valve and order, its answers, the acknowledgement's prune and the settle, and the view a
 * session settles against (docs/choices-mowzie-build.md RP-190, RP-198, RP-250, RP-251). Minecraft is not on this
 * classpath: the NBT form is TrainRingGameTests' {@code anAppliedOpRingSavesUnderItsKeyBesideTheOwnersKeys}.
 */
class AppliedOpRingTest {
    /** A train ring's valve, TrainRing.CAPACITY (TrainRing itself needs Create, not on this classpath). */
    private static final int TRAIN = 4096;

    @Test
    void theOldestOpFallsOutFirstAtTheBound() {
        AppliedOpRing ring = new AppliedOpRing(TRAIN);
        for (long op = 1; op <= TRAIN; op++)
            ring.add(op);
        assertEquals(TRAIN, ring.size());
        assertTrue(ring.contains(1) && ring.contains(TRAIN));
        ring.add(TRAIN + 1L);
        assertEquals(TRAIN, ring.size());
        assertFalse(ring.contains(1), "the oldest op is out");
        assertTrue(ring.contains(2) && ring.contains(TRAIN + 1L));
        assertEquals(1, ring.evictions(), "the eviction is counted");
        assertEquals(1, ring.lastEvicted());
        long[] saved = ring.toArray();
        assertEquals(2, saved[0], "oldest first");
        assertEquals(TRAIN + 1L, saved[saved.length - 1]);
    }

    @Test
    void anOpHeldIsNotAddedTwice() {
        AppliedOpRing ring = new AppliedOpRing(3);
        assertTrue(ring.add(7));
        assertFalse(ring.add(7), "a resend is not added");
        ring.add(8);
        assertArrayEquals(new long[] {7, 8}, ring.toArray());
        ring.add(9);
        ring.add(7);
        assertArrayEquals(new long[] {7, 8, 9}, ring.toArray(), "7 stays where it was");
        ring.add(10);
        assertArrayEquals(new long[] {8, 9, 10}, ring.toArray());
    }

    @Test
    void aSavedRingLoadsBackInOrderAndKeepsTheNewestPastTheBound() {
        AppliedOpRing ring = new AppliedOpRing(TRAIN);
        long[] ops = {-5, 0, Long.MIN_VALUE, Long.MAX_VALUE, 42};
        ring.load(ops);
        assertArrayEquals(ops, ring.toArray());
        ring.load(new long[0]);
        assertTrue(ring.isEmpty());

        AppliedOpRing small = new AppliedOpRing(2);
        small.load(new long[] {1, 2, 3});
        assertArrayEquals(new long[] {2, 3}, small.toArray());
        small.add(4);
        assertArrayEquals(new long[] {3, 4}, small.toArray());
    }

    @Test
    void aLoadedRingIsNotCountedAsEvictions() {
        AppliedOpRing small = new AppliedOpRing(2);
        small.load(new long[] {1, 2, 3}, new long[] {10, 20, 30});
        assertEquals(0, small.evictions(), "a load past the cap is not an eviction");
        assertEquals(20, small.answerOf(2));
        assertEquals(30, small.answerOf(3));
        small.add(4);
        assertEquals(1, small.evictions());
    }

    // ---- answers and the acknowledgement (RP-250) -----------------------------------------------

    @Test
    void anAnsweredOpLeavesOnlyOnTheAcknowledgementOfItsLastAnswer() {
        AppliedOpRing ring = new AppliedOpRing(8);
        ring.add(7);
        assertEquals(AppliedOpRing.UNKNOWN, ring.answerOf(7), "a new op has no answer yet");
        assertEquals(-1, ring.answerOf(8), "an op not held");
        assertTrue(ring.answered(7, 100));
        assertFalse(ring.answered(8, 100), "an op not held takes no answer");
        assertThrows(IllegalArgumentException.class, () -> ring.answered(7, 0));
        assertEquals(100, ring.answerOf(7));

        assertTrue(ring.answered(7, 140), "answered again (a resend's DUPLICATE)");
        assertFalse(ring.pruneIfAnsweredAt(7, 100), "the first answer's acknowledgement does not cover the second");
        assertTrue(ring.contains(7));
        assertFalse(ring.pruneIfAnsweredAt(7, 0), "never on an unknown answer");
        assertTrue(ring.pruneIfAnsweredAt(7, 140));
        assertFalse(ring.contains(7));
        assertFalse(ring.pruneIfAnsweredAt(7, 140), "gone");
    }

    @Test
    void aHeldOpKeepsItsAnswerWhenAddedAgain() {
        AppliedOpRing ring = new AppliedOpRing(4);
        ring.add(1);
        ring.answered(1, 50);
        assertFalse(ring.add(1));
        assertEquals(50, ring.answerOf(1));
    }

    @Test
    void forgettingTheAnswersKeepsEveryOpUnknown() {
        AppliedOpRing ring = new AppliedOpRing(4);
        ring.add(1);
        ring.add(2);
        ring.add(3);
        ring.answered(1, 10);
        ring.answered(3, 30);
        assertEquals(1, ring.unknownCount());
        assertEquals(2, ring.forgetAnswers());
        assertEquals(3, ring.unknownCount());
        assertFalse(ring.pruneIfAnsweredAt(1, 10), "an acknowledgement that comes after the forget prunes nothing");
        assertArrayEquals(new long[] {1, 2, 3}, ring.toArray());
    }

    @Test
    void theAnswersFollowTheOpsInOrder() {
        AppliedOpRing ring = new AppliedOpRing(4);
        ring.add(5);
        ring.add(6);
        ring.add(7);
        ring.answered(7, 70);
        ring.answered(5, 50);
        assertArrayEquals(new long[] {5, 6, 7}, ring.toArray());
        assertArrayEquals(new long[] {50, 0, 70}, ring.answersArray());
        StringBuilder seen = new StringBuilder();
        ring.forEachAnswered((op, at) -> seen.append(op).append('@').append(at).append(' '));
        assertEquals("5@50 7@70 ", seen.toString());

        AppliedOpRing back = new AppliedOpRing(4);
        back.load(ring.toArray(), ring.answersArray());
        assertArrayEquals(ring.answersArray(), back.answersArray());
        back.load(ring.toArray(), new long[] {1});
        assertEquals(3, back.unknownCount(), "answers of another length read as none known");
    }

    // ---- the settle's three cases (RP-251) --------------------------------------------------------

    @Test
    void theSettleRemovesWhatThePagesHoldAndKeepsTheRestUnknown() {
        AppliedOpRing ring = new AppliedOpRing(8);
        ring.add(1); // answered at 90: at or below the pages, leaves
        ring.add(2); // answered at 100: at the pages, leaves
        ring.add(3); // answered at 101: past the pages, stays, unknown
        ring.add(4); // never answered (unknown): stays, unknown
        ring.answered(1, 90);
        ring.answered(2, 100);
        ring.answered(3, 101);
        assertEquals(2, ring.settle(100));
        assertArrayEquals(new long[] {3, 4}, ring.toArray());
        assertEquals(2, ring.unknownCount(), "what stays has no answer");
        assertEquals(0, ring.settle(1_000_000), "an unknown answer never leaves on a settle");
        assertArrayEquals(new long[] {3, 4}, ring.toArray());
    }

    @Test
    void aSettleThatProvesNothingKeepsEveryOp() {
        AppliedOpRing ring = new AppliedOpRing(8);
        ring.add(1);
        ring.add(2);
        ring.answered(1, 5);
        assertEquals(0, ring.settle(0));
        assertArrayEquals(new long[] {1, 2}, ring.toArray());
        assertEquals(2, ring.unknownCount());
    }

    // ---- the view (RP-251) -------------------------------------------------------------------------

    private static final long W = 0x1111, V = 0x2222, U = 0x3333;

    @Test
    void aViewProvesThePagesOnlyForTheJournalWritersRing() {
        // U loaded W's journal file (through 500, U's first record 501) and pages through 400.
        RingView view = new RingView(U, W, 400, 500, 501);
        assertNull(view.unproven());
        assertEquals(400, view.bound(W), "W's ring settles against the pages");
        assertEquals(0, view.bound(V), "a ring another session settled last proves nothing");
        assertEquals(0, view.bound(0), "a ring no session settled proves nothing");
        assertEquals(500, new RingView(U, W, 500, 500, 501).bound(W), "pages at the file's end");
    }

    @Test
    void aViewProvesNothingWhenThePagesMayBeALaterSessions() {
        // P29: the pages ahead of the loaded journal file, so a later session may have made them.
        RingView ahead = new RingView(U, W, 501, 500, 502);
        assertNotNull(ahead.unproven());
        assertEquals(0, ahead.bound(W));
        // A write of this session already landed: the file's end is no longer the loaded one's.
        RingView late = new RingView(U, W, 400, 501, 501);
        assertNotNull(late.unproven());
        assertEquals(0, late.bound(W));
        // A journal that names no earlier session.
        RingView none = new RingView(U, 0, 400, 500, 501);
        assertNotNull(none.unproven());
        assertEquals(0, none.bound(0));
    }

    @Test
    void theWriterIsTheNewestGenerationBelowTheSessionsFirstRecord() {
        long[] seqs = {10, 300, 501};
        long[] gens = {W, V, U};
        assertEquals(V, RingView.writerGen(seqs, gens, 501), "U's own record is not a writer");
        assertEquals(W, RingView.writerGen(seqs, gens, 300));
        assertEquals(0, RingView.writerGen(seqs, gens, 10));
        assertEquals(V, RingView.writerGen(new long[] {300, 10}, new long[] {V, W}, 501), "any order");
        assertEquals(0, RingView.writerGen(new long[0], new long[0], 501));
    }

    /**
     * The three sessions the settle must survive (RP-251): W answers op X at 450 and its tracks file holds X
     * answered; W's journal file ends at 500, X's answer in it. V replays that file, so its engine forgets X's
     * answer and decides X again live after the replay end (hole a), and V's engine saves pages past V's first
     * record (501). V crashes before its tracks file is written. U loads W's tracks (X answered at 450 by W).
     * Either V's journal file landed (its writer is V, not the ring's W) or it did not (the pages, V's, are ahead
     * of W's file): X stays, its answer unknown, either way. A bound of min(pages, file end) would have dropped it.
     */
    @Test
    void anOpALaterSessionReplayedStaysUnknownWhicheverOfItsFilesLanded() {
        // V's journal file landed: U's first record past it, its writer V.
        RingView landed = new RingView(U, V, 620, 650, 651);
        AppliedOpRing a = ringOfW(450);
        assertEquals(0, a.settle(landed.bound(a.gen())));
        assertTrue(a.contains(0xA) && a.answerOf(0xA) == AppliedOpRing.UNKNOWN);
        // V's journal file did not land: U loaded W's (through 500), the pages V's (through 620).
        RingView lost = new RingView(U, W, 620, 500, 621);
        AppliedOpRing b = ringOfW(450);
        assertEquals(0, b.settle(lost.bound(b.gen())));
        assertTrue(b.contains(0xA) && b.answerOf(0xA) == AppliedOpRing.UNKNOWN);
        assertTrue(Math.min(lost.pages(), lost.written()) >= 450, "min(pages, file end) would have covered X's answer");
    }

    private static AppliedOpRing ringOfW(long at) {
        AppliedOpRing ring = new AppliedOpRing(TRAIN);
        ring.add(0xA);
        ring.answered(0xA, at);
        ring.gen(W);
        return ring;
    }
}
