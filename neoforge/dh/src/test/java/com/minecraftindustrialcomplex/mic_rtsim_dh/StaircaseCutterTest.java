package com.minecraftindustrialcomplex.mic_rtsim_dh;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import org.junit.jupiter.api.Test;

/** The staircase cut: one box on straight level track, a gap-free staircase of the right width otherwise. */
class StaircaseCutterTest {
    private static final StaircaseCutter.Shape SHAPE = new StaircaseCutter.Shape(3.0, 3.0, 1.0, 1.0);
    private static final double EPS = 1e-9;

    @Test
    void straightLevelTrackIsOneCuboid() {
        // An 8-block carriage running east: 8 + 2 overhang long, 3 wide, 3 tall, centred on the midpoint
        List<double[]> boxes = StaircaseCutter.cut(8, 0, 0, SHAPE);
        assertEquals(1, boxes.size());
        assertBox(boxes.get(0), -5, 0, -1.5, 5, 3, 1.5);

        // Running north, backwards: the same box turned
        boxes = StaircaseCutter.cut(0, 0, -8, SHAPE);
        assertEquals(1, boxes.size());
        assertBox(boxes.get(0), -1.5, 0, -5, 1.5, 3, 5);
    }

    @Test
    void nearlyStraightIsStillOneCuboid() {
        // 0.1 blocks of drift and 0.1 of rise over 10 blocks: inside the tolerance
        List<double[]> boxes = StaircaseCutter.cut(8, 0.08, 0.08, SHAPE);
        assertEquals(1, boxes.size());
    }

    @Test
    void diagonalIsAStaircaseThreeWideWithoutGaps() {
        double d = 8.0 / Math.sqrt(2.0); // an 8-block carriage at 45 degrees
        List<double[]> boxes = StaircaseCutter.cut(d, 0, d, SHAPE);
        assertEquals(10, boxes.size(), "10 blocks of run at 1-block steps");
        double ux = 1 / Math.sqrt(2), uz = 1 / Math.sqrt(2);
        double nx = -uz, nz = ux; // across the track
        double across = 0, prevAlongEnd = Double.NEGATIVE_INFINITY;
        for (double[] b : boxes) {
            // The box's extent across the track (corner to corner) is the carriage's width
            double[] acrossRange = project(b, nx, nz);
            across = Math.max(across, acrossRange[1] - acrossRange[0]);
            assertEquals(0.0, (acrossRange[0] + acrossRange[1]) / 2.0, 1e-9, "centred on the track");
            // Along the track each box reaches past the start of the next: no gaps
            double[] along = project(b, ux, uz);
            assertTrue(along[0] <= prevAlongEnd + EPS || prevAlongEnd == Double.NEGATIVE_INFINITY,
                    "gap before a box: " + along[0] + " after " + prevAlongEnd);
            prevAlongEnd = along[1];
            assertEquals(0.0, b[1], EPS);
            assertEquals(3.0, b[4], EPS);
        }
        assertEquals(3.0, across, 1e-9);
        // The run spans the carriage plus overhang: centres from -4.5 to 4.5 along it
        double[] first = project(boxes.get(0), ux, uz), last = project(boxes.get(boxes.size() - 1), ux, uz);
        assertEquals(-4.5, (first[0] + first[1]) / 2.0, 1e-9);
        assertEquals(4.5, (last[0] + last[1]) / 2.0, 1e-9);
    }

    @Test
    void shallowAngleKeepsTheWidth() {
        // About 20 degrees off east
        double a = Math.toRadians(20);
        List<double[]> boxes = StaircaseCutter.cut(8 * Math.cos(a), 0, 8 * Math.sin(a), SHAPE);
        assertTrue(boxes.size() > 1);
        double nx = -Math.sin(a), nz = Math.cos(a);
        for (double[] b : boxes) {
            double[] r = project(b, nx, nz);
            assertEquals(3.0, r[1] - r[0], 1e-9);
        }
    }

    @Test
    void slopeOnAnAxisClimbsInSteps() {
        // 8 east, rising 4 (Create's steepest slopes are 1 in 2 or so): a staircase straight along x
        List<double[]> boxes = StaircaseCutter.cut(8, 4, 0, SHAPE);
        assertEquals(10, boxes.size());
        for (int i = 0; i < boxes.size(); i++) {
            double[] b = boxes.get(i);
            assertEquals(1.0, b[3] - b[0], 1e-9, "1 block long");
            assertBoxZ(b, -1.5, 1.5);
            double centre = (b[0] + b[3]) / 2.0;
            assertEquals(centre * 0.5, b[1], 1e-9, "the box sits on the slope at its centre");
            if (i > 0)
                assertTrue(b[1] > boxes.get(i - 1)[1]);
        }
    }

    @Test
    void longerStepsMakeFewerBoxes() {
        double d = 8.0 / Math.sqrt(2.0);
        List<double[]> boxes = StaircaseCutter.cut(d, 0, d, new StaircaseCutter.Shape(3.0, 3.0, 1.0, 2.0));
        assertEquals(5, boxes.size());
        double[] r = project(boxes.get(2), -1 / Math.sqrt(2), 1 / Math.sqrt(2));
        assertEquals(3.0, r[1] - r[0], 1e-9);
    }

    @Test
    void veryLongRunsAreCapped() {
        List<double[]> boxes = StaircaseCutter.cut(300, 0, 300, SHAPE);
        assertEquals(StaircaseCutter.MAX_BOXES, boxes.size());
    }

    @Test
    void degenerateRuns() {
        assertEquals(0, StaircaseCutter.cut(Double.NaN, 0, 0, SHAPE).size());
        assertEquals(0, StaircaseCutter.cut(1, Double.POSITIVE_INFINITY, 0, SHAPE).size());
        List<double[]> point = StaircaseCutter.cut(0, 0, 0, SHAPE);
        assertEquals(1, point.size());
        assertBox(point.get(0), -1.5, 0, -1.5, 1.5, 3, 1.5);
    }

    /** The range of a box's corners projected on the horizontal direction (x, z). */
    private static double[] project(double[] b, double x, double z) {
        double lo = Double.POSITIVE_INFINITY, hi = Double.NEGATIVE_INFINITY;
        for (double cx : new double[] {b[0], b[3]})
            for (double cz : new double[] {b[2], b[5]}) {
                double p = cx * x + cz * z;
                lo = Math.min(lo, p);
                hi = Math.max(hi, p);
            }
        return new double[] {lo, hi};
    }

    private static void assertBox(double[] b, double x0, double y0, double z0, double x1, double y1, double z1) {
        assertEquals(x0, b[0], EPS);
        assertEquals(y0, b[1], EPS);
        assertEquals(z0, b[2], EPS);
        assertEquals(x1, b[3], EPS);
        assertEquals(y1, b[4], EPS);
        assertEquals(z1, b[5], EPS);
    }

    private static void assertBoxZ(double[] b, double z0, double z1) {
        assertEquals(z0, b[2], EPS);
        assertEquals(z1, b[5], EPS);
    }
}
