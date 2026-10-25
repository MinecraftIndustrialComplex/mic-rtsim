package com.minecraftindustrialcomplex.mic_rtsim_create;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/**
 * The applied op ring's bound and order (docs/choices-mowzie-build.md RP-190, RP-192, RP-198). Minecraft is not
 * on this classpath: the NBT form is TrainRingGameTests' {@code anAppliedOpRingSavesUnderItsKeyBesideTheOwnersKeys}.
 */
class AppliedOpRingTest {
    /** A train ring's bound, TrainRing.CAPACITY (TrainRing itself needs Create, not on this classpath). */
    private static final int TRAIN = 256;

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
}
