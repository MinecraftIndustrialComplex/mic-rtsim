package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.SplittableRandom;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The path sweep that ships ({@link PathSweep}, which the mixin runs: MZB-951) against a copy of the jar's float walk
 * ({@link JarFloatSweep}, the one that runs away far from the origin: MZB-950), on the same worlds. Three things: where the
 * float copy runs away the shipped walk ends, with only the cells between the box and its target; near the origin the two walk
 * the same cells; and the shipped walk finds the same cells for a box and for the same box moved by whole blocks to 7 million out.
 */
class PathSweepTest {
    /** A real sweep reads a few thousand cells (80 blocks, a box 5 wide and 5.5 tall); a runaway counts through the int range. */
    private static final long CAP = 400_000;

    /** A walk that read more than {@link #CAP} cells: stopped there, the jar's loops would go on for minutes. */
    private static final class Runaway extends RuntimeException {
        Runaway() {
            super(null, null, false, false);
        }
    }

    /** What a walk visited: the cells (relative to an origin cell, packed), the columns' ground cells, and the corners of both. */
    private static final class Visits implements PathSweep.Columns {
        final int ox, oy, oz;
        final boolean jar;
        final Set<Long> cells = new HashSet<>();
        final Set<Long> columns = new HashSet<>();
        long read;
        int minX = Integer.MAX_VALUE, minY = Integer.MAX_VALUE, minZ = Integer.MAX_VALUE;
        int maxX = Integer.MIN_VALUE, maxY = Integer.MIN_VALUE, maxZ = Integer.MIN_VALUE;

        /** {@code jar}: the y loop is the jar's {@code y != y1}, which counts through the int range from a start past its end. */
        Visits(int ox, int oy, int oz, boolean jar) {
            this.ox = ox;
            this.oy = oy;
            this.oz = oz;
            this.jar = jar;
        }

        static long pack(int x, int y, int z) {
            return ((long) x & 0x1FFFFFFL) << 38 | ((long) z & 0x1FFFFFFL) << 13 | ((long) y & 0x1FFFL);
        }

        @Override
        public boolean column(int x, int z, int y0, int y1, int stepY) {
            int span = (y1 - y0) * stepY;
            long n = jar ? Integer.toUnsignedLong(span) : Math.max(0, span);
            read += n + 1;
            if (read > CAP)
                throw new Runaway();
            columns.add(pack(x - ox, y0 - oy, z - oz));
            minX = Math.min(minX, x);
            maxX = Math.max(maxX, x);
            minZ = Math.min(minZ, z);
            maxZ = Math.max(maxZ, z);
            for (int k = 0; k < n; k++) {
                int y = y0 + k * stepY;
                minY = Math.min(minY, y);
                maxY = Math.max(maxY, y);
                cells.add(pack(x - ox, y - oy, z - oz));
            }
            return true;
        }
    }

    private record Sweep(double[] vec, double[] base, double[] max) {
        Sweep shifted(double dx, double dz) {
            return new Sweep(vec, new double[] {base[0] + dx, base[1], base[2] + dz}, new double[] {max[0] + dx, max[1], max[2] + dz});
        }

        int[] origin() {
            return new int[] {(int) Math.floor(base[0]), (int) Math.floor(base[1]), (int) Math.floor(base[2])};
        }

        /** The cells the box's swept volume spans, a block beyond each way (the walk's last slab is the first past the end). */
        int[] bounds() {
            int[] b = new int[6];
            for (int i = 0; i < 3; i++) {
                b[i] = (int) Math.floor(Math.min(base[i], base[i] + vec[i])) - 1;
                b[3 + i] = (int) Math.floor(Math.max(max[i], max[i] + vec[i])) + 1;
            }
            return b;
        }
    }

    private interface Walk {
        boolean sweep(double[] vec, double[] base, double[] max, PathSweep.Columns columns);
    }

    private static final Walk SHIPPED = PathSweep::sweep;
    private static final Walk JAR = JarFloatSweep::sweep;

    private static double quantized(SplittableRandom r, double lo, double hi, int perBlock) {
        return Math.round((lo + r.nextDouble() * (hi - lo)) * perBlock) / (double) perBlock;
    }

    /**
     * A sweep as the jar's callers make one: a mob's box (feet on a block 50 times in 100, standing at a block's middle or
     * anywhere), a path node at most 80 blocks off, a third of them level, level or up or down; every number a multiple of 2^-6,
     * so that a float holds it exactly out to 2^17 blocks (the near-origin comparison needs the casts to lose nothing).
     */
    private static Sweep random(SplittableRandom r, double cx, double cz) {
        double w = quantized(r, 0.5, 5.0, 16), h = quantized(r, 0.5, 5.5, 16);
        double x = r.nextInt(3) == 0 ? Math.floor(cx) + 0.5 : cx + quantized(r, 0, 1, 64);
        double z = r.nextInt(3) == 0 ? Math.floor(cz) + 0.5 : cz + quantized(r, 0, 1, 64);
        double y = r.nextBoolean() ? Math.floor(quantized(r, -60, 60, 1)) : quantized(r, -60, 60, 64);
        double len = 1 + r.nextDouble() * 79, angle = r.nextDouble() * Math.PI * 2;
        double vx = Math.round(Math.cos(angle) * len * 64) / 64.0;
        double vz = Math.round(Math.sin(angle) * len * 64) / 64.0;
        double vy = switch (r.nextInt(3)) {
            case 0 -> 0;
            case 1 -> quantized(r, -2, 2, 64);
            default -> quantized(r, -6, 6, 64);
        };
        if (r.nextInt(8) == 0)
            vz = 0;
        if (r.nextInt(8) == 0)
            vx = 0;
        double[] base = {x - w / 2, y, z - w / 2};
        return new Sweep(new double[] {vx, vy, vz}, base, new double[] {base[0] + w, base[1] + h, base[2] + w});
    }

    private static Visits walk(Walk walk, Sweep s, boolean jar) {
        int[] o = s.origin();
        Visits v = new Visits(o[0], o[1], o[2], jar);
        walk.sweep(s.vec, s.base, s.max, v);
        return v;
    }

    /** Whether the walk read more than {@link #CAP} cells, or a cell outside the sweep's bounds; null when it did neither. */
    private static String fault(Walk w, boolean jar, Sweep s) {
        Visits v;
        try {
            v = walk(w, s, jar);
        } catch (Runaway e) {
            return "ran away";
        }
        if (v.cells.isEmpty())
            return null;
        int[] b = s.bounds();
        if (v.minX < b[0] || v.minY < b[1] - 1 || v.minZ < b[2] || v.maxX > b[3] || v.maxY > b[4] || v.maxZ > b[5])
            return "read outside the box and its target: x " + v.minX + " to " + v.maxX + ", y " + v.minY + " to " + v.maxY + ", z " + v.minZ + " to " + v.maxZ
                    + " beyond " + java.util.Arrays.toString(b);
        return null;
    }

    // ---- far from the origin ---------------------------------------------------------------------

    @Test
    void theSweepOfMzb950RunsAwayInTheJarAndEndsInTheShippedWalk() {
        // found by fuzzing the jar's copy (PathSweepFloatTest in core): a Frostmaw-sized box 3.5 wide, a path node 38.6 blocks off
        double[] vec = {-37.251516399504, 0.0, 10.136477625167588};
        double x = -3_797_619.272, z = 4_715_190.381;
        double[] base = {x - 1.75, -59.75, z - 1.75};
        Sweep far = new Sweep(vec, base, new double[] {base[0] + 3.5, base[1] + 4.5, base[2] + 3.5});
        assertEquals("ran away", fault(JAR, true, far), "the jar's walk, at the far base of the ice-rates test's second run");
        assertEquals(null, fault(SHIPPED, false, far), "the shipped walk");
        Visits v = walk(SHIPPED, far, false);
        assertTrue(v.read > 100 && v.read < 10_000, "it read " + v.read + " cells");
        // and it is the walk of the same sweep 3.7 and 4.6 million blocks nearer, cell for cell
        Sweep near = far.shifted(3_700_000, -4_600_000);
        assertEquals(v.cells, walk(SHIPPED, near, false).cells);
    }

    @Test
    void whereTheJarsWalkRunsAwayTheShippedWalkEndsAndReadsOnlyBetweenTheBoxAndItsTarget() {
        SplittableRandom r = new SplittableRandom(20261028L);
        int ranAway = 0, sweeps = 0;
        List<String> faults = new ArrayList<>();
        for (int k = 0; k < 6_000; k++) {
            double x = -(500_000 + r.nextDouble() * 6_600_000), z = 500_000 + r.nextDouble() * 6_600_000;
            // unquantized: the doubles a mob's position is
            Sweep s = random(r, x, z);
            sweeps++;
            if (k < 1_500 && "ran away".equals(fault(JAR, true, s)))
                ranAway++;
            String f = fault(SHIPPED, false, s);
            if (f != null)
                faults.add(f + " at " + s.base[0] + ", " + s.base[2]);
        }
        assertTrue(faults.isEmpty(), "the shipped walk faulted in " + faults.size() + " of " + sweeps + ": " + faults.stream().limit(3).toList());
        assertTrue(ranAway > 20, "only " + ranAway + " of 1,500 sweeps ran away in the jar's copy: the far inputs do not show the bug");
    }

    @Test
    void aBoxAndItsShiftByWholeBlocksFindTheSameCellsToSevenMillionBlocksOut() {
        SplittableRandom r = new SplittableRandom(951L);
        for (int k = 0; k < 4_000; k++) {
            Sweep near = random(r, r.nextInt(-3000, 3000), r.nextInt(-3000, 3000));
            Visits n = walk(SHIPPED, near, false);
            double dx = -(3_000_000 + r.nextInt(4_100_000)), dz = 3_000_000 + r.nextInt(4_100_000);
            Visits f = walk(SHIPPED, near.shifted(dx, dz), false);
            assertEquals(n.cells, f.cells, "cells at shift " + dx + ", " + dz + " of " + describe(near));
            assertEquals(n.columns, f.columns, "columns at shift " + dx + ", " + dz + " of " + describe(near));
        }
    }

    // ---- near the origin -------------------------------------------------------------------------

    /**
     * Within 3,000 blocks of the origin a float holds the box's edges exactly (the sweeps here are multiples of 2^-6) and the
     * two walks read the same cells and the same columns. They are the same algorithm in two precisions, so a crossing the
     * float's rounding makes a tie or breaks a tie the other way (a vector that meets a cell's corner) can read another
     * corner cell; the walks are compared on a seeded set and the disagreements counted.
     */
    @Test
    void nearTheOriginTheShippedWalkReadsTheJarsCells() {
        SplittableRandom r = new SplittableRandom(1_021_951L);
        int n = 20_000, differ = 0;
        List<String> examples = new ArrayList<>();
        for (int k = 0; k < n; k++) {
            Sweep s = random(r, r.nextInt(-3000, 3000), r.nextInt(-3000, 3000));
            Visits jar = walk(JAR, s, true), shipped = walk(SHIPPED, s, false);
            if (!jar.cells.equals(shipped.cells) || !jar.columns.equals(shipped.columns)) {
                differ++;
                if (examples.size() < 3)
                    examples.add(describe(s) + ": jar " + jar.cells.size() + " cells, shipped " + shipped.cells.size());
            }
        }
        assertEquals(0, differ, differ + " of " + n + " sweeps differ, e.g. " + examples);
    }

    @Test
    void aSweepAlongAnAxisReadsTheSlabsOfItsCrossings() {
        // a box 1 wide and 2 tall at x 0.5 to 1.5, y 0.5 to 2.5, z 0.5 to 1.5, moving 2 along x
        double[] base = {0.5, 0.5, 0.5}, max = {1.5, 2.5, 1.5};
        for (Walk w : List.of(SHIPPED, JAR)) {
            List<int[]> cols = new ArrayList<>();
            w.sweep(new double[] {2, 0, 0}, base, max, (x, z, y0, y1, stepY) -> cols.add(new int[] {x, z, y0, y1, stepY}));
            // the leading face (x 1.5, in cell 1) crosses into cells 2, 3 and 4 at t 0.5, 1.5 and 2.5; the vector ends at t 2, so the
            // crossing into cell 4 is the first past the end, which the jar's do-while reads before it stops. The top face is at y 2.5,
            // in cell 2: y 0 to 2. Across z the box spans cells 0 and 1.
            List<String> got = new ArrayList<>();
            for (int[] c : cols)
                got.add(c[0] + "," + c[1] + "," + c[2] + ".." + c[3]);
            assertEquals(List.of("2,0,0..3", "2,1,0..3", "3,0,0..3", "3,1,0..3", "4,0,0..3", "4,1,0..3"), got);
        }
    }

    // ---- what the walk does with what the world answers -----------------------------------------

    @Test
    void theWalkEndsAtTheFirstBlockedColumnAndNoVectorIsNoMove() {
        double[] base = {0.5, 0.5, 0.5}, max = {1.5, 2.5, 1.5};
        int[] seen = new int[1];
        assertFalse(PathSweep.sweep(new double[] {10, 0, 0}, base, max, (x, z, y0, y1, stepY) -> ++seen[0] < 3));
        assertEquals(3, seen[0], "it stops at the column that answers false");
        seen[0] = 0;
        assertTrue(PathSweep.sweep(new double[] {0, 0, 0}, base, max, (x, z, y0, y1, stepY) -> ++seen[0] < 0));
        assertTrue(PathSweep.sweep(new double[] {1.0E-9, 0, 0}, base, max, (x, z, y0, y1, stepY) -> ++seen[0] < 0));
        assertEquals(0, seen[0], "a vector shorter than 1.0E-8 reads nothing and is clear, as in the jar");
    }

    @Test
    void anInputThatIsNoNumberOrNoWhereNearAnyWorldIsBlockedWithNothingRead() {
        int[] seen = new int[1];
        PathSweep.Columns count = (x, z, y0, y1, stepY) -> {
            seen[0]++;
            return true;
        };
        double[] ok = {1, 1, 1}, big = {2, 2, 2};
        assertFalse(PathSweep.sweep(new double[] {Double.NaN, 0, 0}, ok, big, count));
        assertFalse(PathSweep.sweep(new double[] {1, 0, 0}, new double[] {Double.POSITIVE_INFINITY, 1, 1}, big, count));
        assertFalse(PathSweep.sweep(new double[] {1, 0, 0}, ok, new double[] {2, 2, Double.NaN}, count));
        assertFalse(PathSweep.sweep(new double[] {1, 0, 0}, new double[] {2.0E9, 1, 1}, new double[] {2.0E9 + 1, 2, 2}, count));
        assertEquals(0, seen[0]);
    }

    @Test
    void aBudgetStopsAWalkThatReadsMoreThanItWasGivenAndSaysSo() {
        double[] base = {0.5, 0.5, 0.5}, max = {1.5, 2.5, 1.5};
        PathSweep.Budget budget = new PathSweep.Budget(10, 60_000);
        PathSweep.budget(budget);
        try {
            assertFalse(PathSweep.sweep(new double[] {40, 0, 0}, base, max, (x, z, y0, y1, stepY) -> true));
        } finally {
            PathSweep.budget(null);
        }
        assertTrue(budget.overran());
        assertTrue(budget.cells() <= 10 + 4, "it read " + budget.cells() + " cells after the budget of 10");
        assertTrue(PathSweep.sweep(new double[] {40, 0, 0}, base, max, (x, z, y0, y1, stepY) -> true), "no budget, no bound");
    }

    @Test
    void aTimeBudgetStopsAWalkToo() {
        PathSweep.Budget budget = new PathSweep.Budget(Long.MAX_VALUE, 0);
        PathSweep.budget(budget);
        try {
            assertFalse(PathSweep.sweep(new double[] {40, 0, 0}, new double[] {0.5, 0.5, 0.5}, new double[] {1.5, 2.5, 1.5}, (x, z, y0, y1, stepY) -> true));
        } finally {
            PathSweep.budget(null);
        }
        assertTrue(budget.overran());
    }

    private static String describe(Sweep s) {
        return "base " + java.util.Arrays.toString(s.base) + " max " + java.util.Arrays.toString(s.max) + " vec " + java.util.Arrays.toString(s.vec);
    }
}
