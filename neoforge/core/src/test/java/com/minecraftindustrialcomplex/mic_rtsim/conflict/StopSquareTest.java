package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** The stop's square and the walk-in's entry point (MZB-472). */
class StopSquareTest {
    private static final double EPS = 1e-9;

    @Test
    void theSquareIsTwoChunksInsideTheEntityTickingEdge() {
        // Simulation distance 10, the player in chunk (0, 0): chunks -8 to 8, blocks -128 to 143.
        assertArrayEquals(new int[] {-128, -128, 143, 143}, StopSquare.square(0, 0, 10));
        // In chunk (3, -2): chunks -5 to 11 in x and -10 to 6 in z.
        assertArrayEquals(new int[] {-80, -160, 191, 111}, StopSquare.square(3, -2, 10));
        // Simulation distance 4: the ticking area is nine chunks across, the square five.
        assertArrayEquals(new int[] {-32, -32, 47, 47}, StopSquare.square(0, 0, 4));
        assertArrayEquals(new int[] {16, -32, 31, -17}, StopSquare.square(1, -2, 2));
    }

    @Test
    void aSmallSimulationDistanceStillLeavesTheChunkTheyStandIn() {
        // The floor is the player's own chunk, for a simulation distance of 2, 1, 0 or less.
        for (int s : new int[] {2, 1, 0, -3})
            assertArrayEquals(new int[] {16, -32, 31, -17}, StopSquare.square(1, -2, s), "simulation distance " + s);
    }

    @Test
    void theSquareStaysTwoChunksFromTheEdgeAtEveryDistance() {
        for (int s = 3; s <= 32; s++) {
            int[] sq = StopSquare.square(7, -5, s);
            // The ticking area's blocks run 16 * (pcx - s) to 16 * (pcx + s + 1) - 1: the square is two chunks (32 blocks) in from each side.
            assertEquals(16 * (7 - s) + 32, sq[0], "west edge at " + s);
            assertEquals(16 * (7 + s + 1) - 1 - 32, sq[2], "east edge at " + s);
            assertEquals(16 * (-5 - s) + 32, sq[1], "north edge at " + s);
            assertEquals(16 * (-5 + s + 1) - 1 - 32, sq[3], "south edge at " + s);
        }
    }

    @Test
    void aPointIsInsideBetweenTheMinimumAndTheBlockAfterTheMaximum() {
        int[] sq = StopSquare.square(0, 0, 10);
        assertTrue(StopSquare.inside(0, 0, sq));
        assertTrue(StopSquare.inside(-128, -128, sq));
        assertTrue(StopSquare.inside(143.99, 143.99, sq));
        assertFalse(StopSquare.inside(144, 0, sq));
        assertFalse(StopSquare.inside(0, -128.01, sq));
    }

    @Test
    void aClampLandsHalfABlockInsideTheEdge() {
        int[] sq = StopSquare.square(0, 0, 10);
        assertArrayEquals(new double[] {-127.5, 143.5}, StopSquare.clampInto(-1000, 1000, sq), EPS);
        assertArrayEquals(new double[] {5.25, -7.5}, StopSquare.clampInto(5.25, -7.5, sq), EPS);
        assertTrue(StopSquare.inside(StopSquare.clampInto(1e9, -1e9, sq)[0], StopSquare.clampInto(1e9, -1e9, sq)[1], sq));
    }

    @Test
    void aStackComesInWhereItsLineToTheHeadCrossesTheEdge() {
        int[] sq = StopSquare.square(0, 0, 10);
        // From far west toward a head at the centre: in at the west edge, half a block inside.
        assertArrayEquals(new double[] {-127.5, 0.0}, StopSquare.entry(-500, 0, 0, 0, sq), EPS);
        // Diagonally from the north-west: the line meets the west edge first, at (-128, -64).
        assertArrayEquals(new double[] {-127.5, -64.0}, StopSquare.entry(-400, -200, 0, 0, sq), EPS);
        // From the north, the north edge.
        assertArrayEquals(new double[] {20.0, -127.5}, StopSquare.entry(20, -900, 20, 100, sq), EPS);
    }

    @Test
    void aSegmentThatNeverEntersTheSquareBringsTheHeadClampedIn() {
        int[] sq = StopSquare.square(0, 0, 10);
        // Both ends west of the square: the head, pulled in.
        assertArrayEquals(new double[] {-127.5, 30.0}, StopSquare.entry(-900, 0, -400, 30, sq), EPS);
        // The line passes by the corner without touching it.
        double[] c = StopSquare.entry(-300, -100, -100, -400, sq);
        assertTrue(StopSquare.inside(c[0], c[1], sq));
        // Parallel to an edge and outside it: no entry, the head pulled in.
        assertArrayEquals(new double[] {-127.5, 143.5}, StopSquare.entry(-300, 400, -200, 400, sq), EPS);
    }

    @Test
    void aHeadInsideTheSquareIsReachedFromAnyDirection() {
        int[] sq = StopSquare.square(2, 5, 8);
        double[][] places = {{-900, 80 * 5}, {900, 80 * 5}, {160, 900}, {160, -900}, {-700, -700}, {900, 900}};
        for (double[] a : places) {
            double[] e = StopSquare.entry(a[0], a[1], 40, 100, sq);
            assertTrue(StopSquare.inside(e[0], e[1], sq), "from " + a[0] + ", " + a[1]);
        }
    }
}
