package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import org.junit.jupiter.api.Test;

import java.util.SplittableRandom;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Why a GameTest that has a Mowzie's Mobs body follow a path builds near the origin (PathSafeBase, MZB-950): a copy of the
 * float grid walk of the jar's {@code MMPathNavigateGround.sweep} (Mowzie's Mobs 1.8.2, decompiled with cfr; the block
 * lookups left out, the cells counted), run at a far base of {@code ConflictKit.farBase} and at the slots of PathSafeBase.
 */
class PathSweepFloatTest {
    /** A real sweep visits a few thousand cells (80 blocks, a box 5 wide and 4.5 tall); a runaway counts through the int range. */
    private static final long CAP = 400_000;

    static int floor(float f) {
        int i = (int) f;
        return f < i ? i - 1 : i;
    }

    static int leadEdgeToInt(float coord, int step) {
        return floor(coord - (float) step * 1.0E-8f);
    }

    static int trailEdgeToInt(float coord, int step) {
        return floor(coord + (float) step * 1.0E-8f);
    }

    /** Cells the walk from a box (base to max) along vec visits, or CAP + 1 when it runs away. */
    static long cells(double[] vec, double[] base, double[] max) {
        float t = 0.0f;
        float maxT = (float) Math.sqrt(vec[0] * vec[0] + vec[1] * vec[1] + vec[2] * vec[2]);
        if (maxT < 1.0E-8f)
            return 0;
        float[] tr = new float[3], tDelta = new float[3], tNext = new float[3], normed = new float[3];
        int[] ldi = new int[3], tri = new int[3], step = new int[3];
        for (int i = 0; i < 3; i++) {
            float value = (float) vec[i];
            boolean dir = value >= 0.0f;
            step[i] = dir ? 1 : -1;
            float lead = (float) (dir ? max : base)[i];
            tr[i] = (float) (dir ? base : max)[i];
            ldi[i] = leadEdgeToInt(lead, step[i]);
            tri[i] = trailEdgeToInt(tr[i], step[i]);
            normed[i] = value / maxT;
            tDelta[i] = Math.abs(maxT / value);
            float dist = dir ? (float) (ldi[i] + 1) - lead : lead - (float) ldi[i];
            tNext[i] = tDelta[i] < Float.POSITIVE_INFINITY ? tDelta[i] * dist : Float.POSITIVE_INFINITY;
        }
        long n = 0;
        do {
            int axis = tNext[0] < tNext[1] ? (tNext[0] < tNext[2] ? 0 : 2) : (tNext[1] < tNext[2] ? 1 : 2);
            float dt = tNext[axis] - t;
            t = tNext[axis];
            ldi[axis] += step[axis];
            tNext[axis] += tDelta[axis];
            for (int i = 0; i < 3; i++) {
                tr[i] += dt * normed[i];
                tri[i] = trailEdgeToInt(tr[i], step[i]);
            }
            int x0 = axis == 0 ? ldi[0] : tri[0], x1 = ldi[0] + step[0];
            int y0 = axis == 1 ? ldi[1] : tri[1], y1 = ldi[1] + step[1];
            int z0 = axis == 2 ? ldi[2] : tri[2], z1 = ldi[2] + step[2];
            for (int x = x0; x != x1; x += step[0])
                for (int z = z0; z != z1; z += step[2]) {
                    for (int y = y0; y != y1; y += step[1])
                        if (++n > CAP)
                            return CAP + 1;
                    if (++n > CAP)
                        return CAP + 1;
                }
        } while (t <= maxT);
        return n;
    }

    private static long cellsFrom(double centreX, double centreZ, double width, double height, double[] vec) {
        double[] base = {centreX - width / 2, -59.75, centreZ - width / 2};
        double[] max = {base[0] + width, base[1] + height, base[2] + width};
        return cells(vec, base, max);
    }

    @Test
    void aSweepAtAFarBaseCanRunAwayWhereTheSameSweepNearTheOriginDoesNot() {
        // found by fuzzing the copy: a Frostmaw-sized box 3.5 wide, a path node 38.6 blocks off
        double[] vec = {-37.251516399504, 0.0, 10.136477625167588};
        assertEquals(CAP + 1, cellsFrom(-3_797_619.272, 4_715_190.381, 3.5, 4.5, vec), "at the far base of the ice-rates test's second run");
        assertTrue(cellsFrom(-3_797_619.272 + 3_700_000, 4_715_190.381 - 4_600_000, 3.5, 4.5, vec) < 10_000, "the same sweep 3.7 and 4.6 million blocks nearer");
    }

    @Test
    void noSweepRunsAwayAtAnySlotOfThePathSafeBases() {
        SplittableRandom random = new SplittableRandom(20261027L);
        double[] widths = {0.6, 0.9, 3.0, 3.5, 4.0, 5.0};
        long worst = 0;
        for (int k = 0; k < 300_000; k++) {
            int[] b = PathSafeBase.pick(random.split());
            double len = 10 + random.nextDouble() * 70, angle = random.nextDouble() * Math.PI * 2;
            double[] vec = {Math.cos(angle) * len, random.nextInt(3) == 0 ? 0 : (random.nextDouble() - 0.5) * 6, Math.sin(angle) * len};
            double width = widths[random.nextInt(widths.length)];
            long cells = cellsFrom(b[0] + random.nextInt(150) + 0.5 * random.nextInt(2), b[1] - random.nextInt(48) + 0.5 * random.nextInt(2), width, 4.5, vec);
            assertFalse(cells > CAP, "a sweep ran away at base " + b[0] + ", " + b[1] + " (vec " + vec[0] + ", " + vec[1] + ", " + vec[2] + ", width " + width + ")");
            worst = Math.max(worst, cells);
        }
        assertTrue(worst < 20_000, "the costliest sweep visited " + worst + " cells");
    }

    @Test
    void theFarBasesAreWhereSweepsRunAway() {
        SplittableRandom random = new SplittableRandom(7L);
        int ran = 0;
        for (int k = 0; k < 100_000; k++) {
            double x = -(3_000_000 + random.nextDouble() * 4_000_000), z = 3_000_000 + random.nextDouble() * 4_000_000;
            double len = 10 + random.nextDouble() * 70, angle = random.nextDouble() * Math.PI * 2;
            double[] vec = {Math.cos(angle) * len, 0, Math.sin(angle) * len};
            if (cellsFrom(x, z, 4.0, 4.5, vec) > CAP)
                ran++;
        }
        assertTrue(ran > 500, "only " + ran + " of 100,000 sweeps at the far bases ran away");
    }
}
