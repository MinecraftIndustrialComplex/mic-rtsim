package com.minecraftindustrialcomplex.mic_rtsim.journal;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** The applied ops a store keeps beside its effect (RP-204): kept until the journal's file holds their answer. */
class DurableOpRingTest {
    @Test
    void anEntryStaysUntilTheJournalOnDiskHoldsItsAnswer() {
        DurableOpRing r = new DurableOpRing();
        int[] changes = {0};
        r.onChange(() -> changes[0]++);
        r.add(11, 100);
        r.add(12, 105);
        assertEquals(2, changes[0], "each add marks the store dirty");
        assertEquals(0, r.trim(99, 0), "the journal's file holds neither answer");
        assertTrue(r.contains(11) && r.contains(12));
        assertEquals(1, r.trim(100, 0), "the file holds 100, not 105");
        assertFalse(r.contains(11));
        assertTrue(r.contains(12));
        assertEquals(0, r.trim(50, 0), "the mark never goes back");
        assertEquals(1, r.trim(105, 0));
        assertEquals(0, r.size());
    }

    @Test
    void entriesReadFromDiskGoWhenTheJournalReadAtTheStartHeldThem() {
        // The store's file was written after the journal's (a crash between the two in one world save): 300 and 310
        // were answered after the journal's file was written through 250.
        DurableOpRing r = DurableOpRing.fromDisk(new long[] {1, 2, 3}, new long[] {240, 300, 310}, DurableOpRing.DEFAULT_CAP);
        assertTrue(r.contains(1) && r.contains(2) && r.contains(3));
        assertEquals(1, r.trim(250, 250), "only 240 was in the journal's file");
        assertEquals(2, r.pending(), "300 and 310 wait for their resend");
        // This life hands out 251, 252, ... again: a durable mark past 310 must not drop them.
        assertEquals(0, r.trim(400, 250));
        assertTrue(r.contains(2) && r.contains(3));
        // The resend of 2 is answered DUPLICATE_OP at 420: stamped with it, dropped once the file holds it.
        r.add(2, 420);
        assertEquals(0, r.trim(410, 250));
        assertEquals(1, r.trim(420, 250));
        assertFalse(r.contains(2));
        assertTrue(r.contains(3), "never resent: kept");
        assertEquals(1, r.pending());
    }

    @Test
    void theCapPushesOutTheOldestAndCountsTheOnesTheJournalDidNotHold() {
        DurableOpRing r = new DurableOpRing(3);
        long before = DurableOpRing.overflowedTotal();
        r.add(1, 10);
        r.add(2, 20);
        r.add(3, 30);
        r.trim(5, 0);
        r.add(4, 40);
        assertFalse(r.contains(1), "the oldest goes first");
        assertEquals(1, r.overflowed(), "10 was above the durable mark 5");
        assertEquals(before + 1, DurableOpRing.overflowedTotal());
        // A stamped-again op moves to the newest end.
        r.add(2, 50);
        r.add(5, 60);
        assertTrue(r.contains(2) && !r.contains(3));
        assertArrayEquals(new long[] {4, 2, 5}, r.ops());
        assertArrayEquals(new long[] {40, 50, 60}, r.seqs());
    }

    @Test
    void theTrimMakesRoomBeforeTheCapPushesAnythingOut() {
        DurableOpRing r = new DurableOpRing(2);
        r.add(1, 10);
        r.add(2, 20);
        r.trim(10, 0); // the file holds 10: dropped, so the next add pushes nothing out
        r.add(3, 30);
        assertEquals(0, r.overflowed());
        assertArrayEquals(new long[] {2, 3}, r.ops());
        r.add(4, 40);
        assertEquals(1, r.overflowed(), "20 was above the mark");
    }

    @Test
    void aCorruptRingKeepsEveryOpUntilItsResendIsAnswered() {
        // DurableOpRings.fromTag's recovery when the file's two arrays differ in length: the sequences are lost.
        DurableOpRing r = DurableOpRing.allPending(new long[] {7, 8}, DurableOpRing.DEFAULT_CAP);
        assertTrue(r.contains(7) && r.contains(8));
        assertEquals(0, r.trim(250, 250), "no mark of the journal drops an op whose answer is unknown");
        assertEquals(2, r.pending());
        // The resend of 7 is refused, answered DUPLICATE_OP at 300: stamped with it, dropped once the file holds it.
        r.add(7, 300);
        assertEquals(1, r.pending());
        assertEquals(0, r.trim(299, 250));
        assertEquals(1, r.trim(300, 250));
        assertFalse(r.contains(7));
        assertEquals(0, r.trim(Long.MAX_VALUE - 1, 250), "8, never resent, outlives any mark");
        assertTrue(r.contains(8));
    }

    @Test
    void aMalformedRingIsRefused() {
        try {
            DurableOpRing.fromDisk(new long[] {1}, new long[0], 8);
        } catch (IllegalArgumentException expected) {
            return;
        }
        throw new AssertionError("lengths that differ must throw");
    }
}
