package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** The stop's square and the walk-in's entry point (MZB-472). */
class StopSquareTest {
    private static final double EPS = 1e-9;

    @Test
    void theSquareIsOneChunkInsideTheEntityTickingEdge() {
        // Simulation distance 10, the player in chunk (0, 0): chunks -9 to 9, blocks -144 to 159.
        assertArrayEquals(new int[] {-144, -144, 159, 159}, StopSquare.square(0, 0, 10));
        // In chunk (3, -2): chunks -6 to 12 in x and -11 to 7 in z.
        assertArrayEquals(new int[] {-96, -176, 207, 127}, StopSquare.square(3, -2, 10));
        // The smallest square is the player's own chunk.
        assertArrayEquals(new int[] {16, -32, 31, -17}, StopSquare.square(1, -2, 1));
        assertArrayEquals(new int[] {16, -32, 31, -17}, StopSquare.square(1, -2, 0));
    }

    @Test
    void aPointIsInsideBetweenTheMinimumAndTheBlockAfterTheMaximum() {
        int[] sq = StopSquare.square(0, 0, 10);
        assertTrue(StopSquare.inside(0, 0, sq));
        assertTrue(StopSquare.inside(-144, -144, sq));
        assertTrue(StopSquare.inside(159.99, 159.99, sq));
        assertFalse(StopSquare.inside(160, 0, sq));
        assertFalse(StopSquare.inside(0, -144.01, sq));
    }

    @Test
    void aClampLandsHalfABlockInsideTheEdge() {
        int[] sq = StopSquare.square(0, 0, 10);
        assertArrayEquals(new double[] {-143.5, 159.5}, StopSquare.clampInto(-1000, 1000, sq), EPS);
        assertArrayEquals(new double[] {5.25, -7.5}, StopSquare.clampInto(5.25, -7.5, sq), EPS);
        assertTrue(StopSquare.inside(StopSquare.clampInto(1e9, -1e9, sq)[0], StopSquare.clampInto(1e9, -1e9, sq)[1], sq));
    }

    @Test
    void aStackComesInWhereItsLineToTheHeadCrossesTheEdge() {
        int[] sq = StopSquare.square(0, 0, 10);
        // From far west toward a head at the centre: in at the west edge, half a block inside.
        assertArrayEquals(new double[] {-143.5, 0.0}, StopSquare.entry(-500, 0, 0, 0, sq), EPS);
        // Diagonally from the north-west: the line meets the west edge first, at (-144, -72).
        assertArrayEquals(new double[] {-143.5, -72.0}, StopSquare.entry(-400, -200, 0, 0, sq), EPS);
        // From the north, the north edge.
        assertArrayEquals(new double[] {20.0, -143.5}, StopSquare.entry(20, -900, 20, 100, sq), EPS);
    }

    @Test
    void aSegmentThatNeverEntersTheSquareBringsTheHeadClampedIn() {
        int[] sq = StopSquare.square(0, 0, 10);
        // Both ends west of the square: the head, pulled in.
        assertArrayEquals(new double[] {-143.5, 30.0}, StopSquare.entry(-900, 0, -400, 30, sq), EPS);
        // The line passes by the corner without touching it.
        double[] c = StopSquare.entry(-300, -100, -100, -400, sq);
        assertTrue(StopSquare.inside(c[0], c[1], sq));
        // Parallel to an edge and outside it: no entry, the head pulled in.
        assertArrayEquals(new double[] {-143.5, 159.5}, StopSquare.entry(-300, 400, -200, 400, sq), EPS);
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
