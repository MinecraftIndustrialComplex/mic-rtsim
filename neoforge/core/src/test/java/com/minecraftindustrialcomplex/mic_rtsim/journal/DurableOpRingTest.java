package com.minecraftindustrialcomplex.mic_rtsim.journal;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The applied ops a store keeps beside its effect (RP-204), under T2's rule (RP-250, RP-251): an op leaves when the
 * engine acknowledges its last answer, or at a session's settle when the engine's pages provably hold it.
 */
class DurableOpRingTest {
    @Test
    void anOpLeavesOnlyWhenItsLastAnswerIsAcknowledged() {
        DurableOpRing r = new DurableOpRing();
        int[] changes = {0};
        r.onChange(() -> changes[0]++);
        r.add(11, 100);
        r.add(12, 105);
        assertEquals(2, changes[0], "each answer marks the store dirty");
        assertFalse(r.pruneIfAnsweredAt(11, 99), "another answer's acknowledgement");
        assertTrue(r.pruneIfAnsweredAt(11, 100));
        assertFalse(r.contains(11));
        assertEquals(3, changes[0], "a prune marks the store dirty");
        // 12 is answered again (a refused resend) at 130 before 105's acknowledgement arrives: it stays for 130.
        r.add(12, 130);
        assertFalse(r.pruneIfAnsweredAt(12, 105));
        assertTrue(r.contains(12));
        assertTrue(r.pruneIfAnsweredAt(12, 130));
        assertEquals(0, r.size());
        assertFalse(r.pruneIfAnsweredAt(12, 130), "gone: nothing to prune");
    }

    @Test
    void anOpWhoseAnswerIsUnknownIsNeverPrunedNorSettledAway() {
        DurableOpRing r = new DurableOpRing();
        r.add(5, DurableOpRing.UNKNOWN);
        assertFalse(r.pruneIfAnsweredAt(5, DurableOpRing.UNKNOWN));
        assertEquals(0, r.settle(Long.MAX_VALUE));
        assertTrue(r.contains(5));
        assertEquals(1, r.unknownCount());
    }

    @Test
    void theSettleDropsWhatThePagesHoldAndMakesTheRestUnknown() {
        // A ring the session of generation 77 answered, read from disk.
        DurableOpRing r = DurableOpRing.fromDisk(new long[] {1, 2, 3}, new long[] {240, 300, 0}, 77, DurableOpRing.DEFAULT_CAP);
        assertEquals(77, r.gen());
        assertEquals(1, r.unknownCount(), "3's answer was already unknown");
        assertEquals(1, r.settle(250), "the pages hold 240, not 300");
        assertFalse(r.contains(1));
        assertTrue(r.contains(2) && r.contains(3));
        assertEquals(2, r.unknownCount(), "300 is this life's to hand out again: unknown now");
        // The resend of 2 is refused, answered DUPLICATE_OP at 420 in this session: it leaves at that acknowledgement.
        r.add(2, 420);
        assertTrue(r.pruneIfAnsweredAt(2, 420));
        assertTrue(r.contains(3), "never resent: kept");
    }

    @Test
    void aSettleWithNothingProvenKeepsEveryOpUnknown() {
        DurableOpRing r = DurableOpRing.fromDisk(new long[] {1, 2}, new long[] {10, 20}, 77, DurableOpRing.DEFAULT_CAP);
        assertEquals(0, r.settle(0));
        assertEquals(2, r.size());
        assertEquals(2, r.unknownCount());
    }

    @Test
    void forgettingTheAnswersKeepsTheOpsForTheirResend() {
        DurableOpRing r = new DurableOpRing();
        r.add(1, 10);
        r.add(2, DurableOpRing.UNKNOWN);
        r.add(3, 30);
        assertEquals(2, r.forgetAnswers());
        assertFalse(r.pruneIfAnsweredAt(1, 10), "an acknowledgement of a forgotten answer prunes nothing");
        assertEquals(3, r.size());
        long[] seen = {0};
        r.forEachAnswered((op, at) -> seen[0]++);
        assertEquals(0, seen[0]);
    }

    @Test
    void theCapPushesOutTheOldestAndCountsEachOne() {
        DurableOpRing r = new DurableOpRing(3);
        long before = DurableOpRing.overflowedTotal();
        r.add(1, 10);
        r.add(2, 20);
        r.add(3, 30);
        r.add(4, 40);
        assertFalse(r.contains(1), "the oldest goes first");
        assertEquals(1, r.overflowed(), "every op in the ring is unacknowledged: pushing one out is an overflow");
        assertEquals(before + 1, DurableOpRing.overflowedTotal());
        // An op answered again moves to the newest end.
        r.add(2, 50);
        r.add(5, 60);
        assertTrue(r.contains(2) && !r.contains(3));
        assertArrayEquals(new long[] {4, 2, 5}, r.ops());
        assertArrayEquals(new long[] {40, 50, 60}, r.seqs());
    }

    @Test
    void anAcknowledgementMakesRoomBeforeTheCapPushesAnythingOut() {
        DurableOpRing r = new DurableOpRing(2);
        r.add(1, 10);
        r.add(2, 20);
        assertTrue(r.pruneIfAnsweredAt(1, 10));
        r.add(3, 30);
        assertEquals(0, r.overflowed());
        assertArrayEquals(new long[] {2, 3}, r.ops());
        r.add(4, 40);
        assertEquals(1, r.overflowed());
    }

    @Test
    void aCorruptRingKeepsEveryOpUntilItsResendIsAnswered() {
        // DurableOpRings.fromTag's recovery when the file's two arrays differ in length: the sequences are lost.
        DurableOpRing r = DurableOpRing.allUnknown(new long[] {7, 8}, DurableOpRing.DEFAULT_CAP);
        assertTrue(r.contains(7) && r.contains(8));
        assertEquals(0, r.gen(), "no session settled it");
        assertEquals(0, r.settle(Long.MAX_VALUE), "no settle drops an op whose answer is unknown");
        assertEquals(2, r.unknownCount());
        // The resend of 7 is refused, answered DUPLICATE_OP at 300: it leaves at that acknowledgement.
        r.add(7, 300);
        assertEquals(1, r.unknownCount());
        assertFalse(r.pruneIfAnsweredAt(7, 299));
        assertTrue(r.pruneIfAnsweredAt(7, 300));
        assertTrue(r.contains(8), "8, never resent, stays");
    }

    @Test
    void aMalformedRingIsRefused() {
        try {
            DurableOpRing.fromDisk(new long[] {1}, new long[0], 0, 8);
        } catch (IllegalArgumentException expected) {
            return;
        }
        throw new AssertionError("lengths that differ must throw");
    }
}
