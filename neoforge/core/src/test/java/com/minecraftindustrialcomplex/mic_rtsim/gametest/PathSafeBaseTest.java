package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import org.junit.jupiter.api.Test;

import java.util.SplittableRandom;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The bases of the GameTests that have a Mowzie's Mobs body walk a path (RailFrostmawGameTests) stay where the jar's float
 * sweep is exact (PathSafeBase): every slot, the strip built from it (150 blocks of track, three chunk rows, a Frostmaw's
 * sweep of 80 blocks beyond), and the margin the release looks in.
 */
class PathSafeBaseTest {
    /** The strip of RailFrostmawGameTests: 150 blocks east of the base, up to 3 chunk rows north and one south, 80 blocks of sweep. */
    private static final int EAST = 150 + 80, NORTH = 3 * 16 + 80, SOUTH = 16 + 80;

    @Test
    void everySlotAndTheStripBuiltFromItLieInsideTheLimit() {
        for (int sx = 0; sx < PathSafeBase.SLOTS; sx++)
            for (int sz = 0; sz < PathSafeBase.SLOTS; sz++) {
                int x = PathSafeBase.X0 - sx * PathSafeBase.STEP, z = PathSafeBase.Z0 + sz * PathSafeBase.STEP;
                assertTrue(Math.abs(x - 80) < PathSafeBase.LIMIT && Math.abs(x + EAST) < PathSafeBase.LIMIT, "x of slot " + sx + ": " + x);
                assertTrue(Math.abs(z - NORTH) < PathSafeBase.LIMIT && Math.abs(z + SOUTH) < PathSafeBase.LIMIT, "z of slot " + sz + ": " + z);
            }
    }

    @Test
    void aBaseIsChunkAlignedAndInsideTheReleasedArea() {
        SplittableRandom random = new SplittableRandom(20261027L);
        for (int i = 0; i < 10_000; i++) {
            int[] b = PathSafeBase.pick(random);
            assertEquals(0, b[0] & 15, "x " + b[0]);
            assertEquals(0, b[1] & 15, "z " + b[1]);
            assertTrue(PathSafeBase.areaHas(b[0] >> 4, b[1] >> 4), "the release misses the base " + b[0] + ", " + b[1]);
            assertTrue(PathSafeBase.areaHas((b[0] + EAST) >> 4, (b[1] - NORTH) >> 4), "the release misses the strip of " + b[0] + ", " + b[1]);
        }
    }

    @Test
    void theReleasedAreaIsNotTheFarAreaOfTheOtherTests() {
        // ConflictKit.farBase: x below -3,000,000, z above 3,000,000 (a test there is another test's)
        assertFalse(PathSafeBase.areaHas(-3_000_000 >> 4, 3_000_000 >> 4));
        assertFalse(PathSafeBase.areaHas(-3_100_000 >> 4, 3_100_000 >> 4));
    }
}
