package com.minecraftindustrialcomplex.mic_rtsim_create;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/** The train op ring's bound and order (docs/choices-mowzie-build.md RP-190, RP-192). */
class AppliedOpRingTest {

    @Test
    void theOldestOpFallsOutFirstAtTheBound() {
        AppliedOpRing ring = new AppliedOpRing();
        for (long op = 1; op <= AppliedOpRing.CAPACITY; op++)
            ring.add(op);
        assertEquals(AppliedOpRing.CAPACITY, ring.size());
        assertTrue(ring.contains(1) && ring.contains(AppliedOpRing.CAPACITY));
        ring.add(AppliedOpRing.CAPACITY + 1L);
        assertEquals(AppliedOpRing.CAPACITY, ring.size());
        assertFalse(ring.contains(1), "the oldest op is out");
        assertTrue(ring.contains(2) && ring.contains(AppliedOpRing.CAPACITY + 1L));
        long[] saved = ring.toArray();
        assertEquals(2, saved[0], "oldest first");
        assertEquals(AppliedOpRing.CAPACITY + 1L, saved[saved.length - 1]);
    }

    @Test
    void anOpHeldIsNotAddedTwice() {
        AppliedOpRing ring = new AppliedOpRing(3);
        ring.add(7);
        ring.add(7);
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
        AppliedOpRing ring = new AppliedOpRing();
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
}
