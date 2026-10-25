package com.minecraftindustrialcomplex.mic_rtsim.industry.glass;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

/** {@link Counting#cuts}: a count sample's records are cut only between (mover node, store node) pairs. */
class CountingSampleCutsTest {
    private static int[] of(int... v) {
        return v;
    }

    @Test
    void everythingThatFitsIsOneRecord() {
        assertArrayEquals(of(7), Counting.cuts(of(0, 3, 5), 7, 10));
        assertArrayEquals(of(0), Counting.cuts(of(), 0, 10));
    }

    @Test
    void recordsAreCutOnPairBoundaries() {
        // pairs of 4, 4, 4, 2 entries; a record holds 10: the third pair opens the second record
        assertArrayEquals(of(8, 14), Counting.cuts(of(0, 4, 8, 12), 14, 10));
        // a pair that would straddle the cap starts the next record
        assertArrayEquals(of(6, 12), Counting.cuts(of(0, 6), 12, 10));
    }

    @Test
    void aPairLargerThanARecordIsSplitAndNothingElseIs() {
        // pairs of 2, 25, 3 entries and a record of 10: the big pair is split in 10s, its last 5 shared with the next pair
        assertArrayEquals(of(2, 12, 22, 30), Counting.cuts(of(0, 2, 27), 30, 10));
        assertArrayEquals(of(10, 20, 25), Counting.cuts(of(0), 25, 10));
    }

    @Test
    void aRecordThatHoldsNoEntryIsRefusedNotLoopedOn() {
        assertThrows(IllegalArgumentException.class, () -> Counting.cuts(of(0), 5, 0));
        assertThrows(IllegalArgumentException.class, () -> Counting.cuts(of(0), 5, -3));
    }
}
