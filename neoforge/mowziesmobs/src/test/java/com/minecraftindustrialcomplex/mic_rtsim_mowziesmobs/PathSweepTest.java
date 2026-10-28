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

        /**
         * The cells the box's swept volume spans and two beyond each way: the walk reads the crossing into the cell past the end
         * (the leading edge ends on a cell's boundary, a touch) and, as the jar's do-while does, the first crossing after the end.
         */
        int[] bounds() {
            int[] b = new int[6];
            for (int i = 0; i < 3; i++) {
                b[i] = (int) Math.floor(Math.min(base[i], base[i] + vec[i])) - 2;
                b[3 + i] = (int) Math.floor(Math.max(max[i], max[i] + vec[i])) + 2;
            }
            return b;
        }

        Sweep perturbed(SplittableRandom r, double d) {
            double[] v = vec.clone(), bs = base.clone(), mx = max.clone();
            for (int i = 0; i < 3; i++) {
                v[i] += (r.nextDouble() * 2 - 1) * d;
                bs[i] += (r.nextDouble() * 2 - 1) * d;
                mx[i] += (r.nextDouble() * 2 - 1) * d;
            }
            return new Sweep(v, bs, mx);
        }
    }

    private interface Walk {
        boolean sweep(double[] vec, double[] base, double[] max, PathSweep.Columns columns);
    }

    private static final Walk SHIPPED = PathSweep::sweep;
    private static final Walk JAR = JarFloatSweep::sweep;

    /** A number in [lo, hi) as a multiple of 1 / perBlock, or any double when perBlock is 0. */
    private static double quantized(SplittableRandom r, double lo, double hi, int perBlock) {
        double v = lo + r.nextDouble() * (hi - lo);
        return perBlock == 0 ? v : Math.round(v * perBlock) / (double) perBlock;
    }

    /**
     * A sweep as the jar's callers make one: a mob's box (feet on a block half the time, standing at a block's middle a third of
     * the time), a path node at most 80 blocks off, a third of them level, a few along one axis. {@code perBlock} 64 makes every
     * number a multiple of 2^-6, which a float and a double hold exactly out to 2^17 blocks (many edges then fall on a cell's
     * boundary, as a mob's on a block's middle do); 0 makes them any double.
     */
    private static Sweep random(SplittableRandom r, double cx, double cz, int perBlock) {
        double w = quantized(r, 0.5, 5.0, perBlock == 0 ? 0 : 16), h = quantized(r, 0.5, 5.5, perBlock == 0 ? 0 : 16);
        double x = r.nextInt(3) == 0 && perBlock != 0 ? Math.floor(cx) + 0.5 : cx + quantized(r, 0, 1, perBlock);
        double z = r.nextInt(3) == 0 && perBlock != 0 ? Math.floor(cz) + 0.5 : cz + quantized(r, 0, 1, perBlock);
        double y = r.nextBoolean() && perBlock != 0 ? Math.floor(quantized(r, -60, 60, 1)) : quantized(r, -60, 60, perBlock);
        double len = 1 + r.nextDouble() * 79, angle = r.nextDouble() * Math.PI * 2;
        double vx = Math.cos(angle) * len, vz = Math.sin(angle) * len;
        double vy = switch (r.nextInt(3)) {
            case 0 -> 0;
            case 1 -> quantized(r, -2, 2, perBlock);
            default -> quantized(r, -6, 6, perBlock);
        };
        if (perBlock != 0) {
            vx = Math.round(vx * perBlock) / (double) perBlock;
            vz = Math.round(vz * perBlock) / (double) perBlock;
        }
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
        if (v.minX < b[0] || v.minY < b[1] || v.minZ < b[2] || v.maxX > b[3] || v.maxY > b[4] || v.maxZ > b[5])
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
            Sweep s = random(r, x, z, 0); // any doubles: a mob's position is
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
            Sweep near = random(r, r.nextInt(-3000, 3000), r.nextInt(-3000, 3000), 64);
            Visits n = walk(SHIPPED, near, false);
            double dx = -(3_000_000 + r.nextInt(4_100_000)), dz = 3_000_000 + r.nextInt(4_100_000);
            Visits f = walk(SHIPPED, near.shifted(dx, dz), false);
            assertEquals(n.cells, f.cells, "cells at shift " + dx + ", " + dz + " of " + describe(near));
            assertEquals(n.columns, f.columns, "columns at shift " + dx + ", " + dz + " of " + describe(near));
        }
    }

    // ---- near the origin -------------------------------------------------------------------------

    /**
     * The shipped walk and the jar's are the same algorithm in two precisions. A float near the origin is off by some
     * hundred thousandths of a block (a ten thousandth at 1,000 blocks, a trailing edge's sum of 60 steps included), and a
     * trailing edge that lies that close to a cell's boundary at a crossing is in one cell for the float and in the next for the
     * double: a box's few edge cells then differ. So the two are compared on the sweeps whose cells do not depend on the last
     * ten thousandth of a block: the double walk reads the same cells and columns under twelve random nudges of that size to the
     * box and the vector (nine numbers). On those, within 16 blocks of the origin, the jar's float walk reads exactly the same
     * cells and columns. Most sweeps are stable; the count is asserted so that the filter cannot be hiding a difference.
     */
    @Test
    void nearTheOriginTheShippedWalkReadsTheJarsCellsWhereTheFloatIsExact() {
        SplittableRandom r = new SplittableRandom(1_021_951L), nudges = new SplittableRandom(7L);
        int n = 12_000, stable = 0, differ = 0;
        List<String> examples = new ArrayList<>();
        for (int k = 0; k < n; k++) {
            Sweep s = random(r, r.nextInt(-16, 16), r.nextInt(-16, 16), 0);
            Visits shipped = walk(SHIPPED, s, false);
            boolean robust = true;
            for (int j = 0; j < 12 && robust; j++) {
                Visits p = walk(SHIPPED, s.perturbed(nudges, 1.0E-4), false);
                robust = shipped.cells.equals(p.cells) && shipped.columns.equals(p.columns);
            }
            if (!robust)
                continue;
            stable++;
            Visits jar = walk(JAR, s, true);
            if (!jar.cells.equals(shipped.cells) || !jar.columns.equals(shipped.columns)) {
                differ++;
                if (examples.size() < 3)
                    examples.add(describe(s) + ": jar " + jar.cells.size() + " cells, shipped " + shipped.cells.size());
            }
        }
        assertEquals(0, differ, differ + " of " + stable + " stable sweeps differ, e.g. " + examples);
        assertTrue(stable > n * 0.7, "only " + stable + " of " + n + " sweeps were stable");
    }

    /**
     * Farther out the float is coarser, and the same sweeps differ more: at 1,000 blocks 1 sweep in 30 or so has a trailing-edge
     * cell or two the float puts in the next cell. Compared as they are, without the stability filter: most are the same, and a
     * different one differs by a few cells in hundreds (cells the box's trailing edge just touches).
     */
    @Test
    void atAThousandBlocksTheTwoWalksStillReadAlmostEveryCellTheSame() {
        SplittableRandom r = new SplittableRandom(1_000_951L);
        int n = 10_000, differ = 0, worstPermille = 0;
        for (int k = 0; k < n; k++) {
            Sweep s = random(r, r.nextInt(-1000, 1000), r.nextInt(-1000, 1000), 0);
            Visits jar = walk(JAR, s, true), shipped = walk(SHIPPED, s, false);
            if (jar.cells.equals(shipped.cells))
                continue;
            differ++;
            Set<Long> sym = new HashSet<>(jar.cells);
            sym.addAll(shipped.cells);
            Set<Long> both = new HashSet<>(jar.cells);
            both.retainAll(shipped.cells);
            sym.removeAll(both);
            worstPermille = Math.max(worstPermille, (int) (sym.size() * 1000L / shipped.cells.size()));
        }
        assertTrue(differ < n * 0.06, differ + " of " + n + " sweeps differ");
        assertTrue(worstPermille < 60, "the worst difference is " + worstPermille + " cells in a thousand");
    }

    /**
     * Where the arithmetic is exact in a float too (a box with edges on multiples of a half, a vector along one axis, all
     * of them well away from 0), the two walks are the same walk to the last cell, with the box's edges on the cell boundaries
     * that tell the jar's {@code floor} from the nudge its source names and its float never applies (a leading edge on an integer
     * is in the cell above it, a trailing one in the cell it starts).
     */
    @Test
    void anAxisAlignedSweepOnExactEdgesReadsTheSameCellsInBothWalks() {
        SplittableRandom r = new SplittableRandom(950_951L);
        for (int k = 0; k < 8_000; k++) {
            double w = (1 + r.nextInt(9)) * 0.5, h = (1 + r.nextInt(11)) * 0.5;
            double[] base = {100 + r.nextInt(600) * 0.5, 40 + r.nextInt(80) * 0.5, 100 + r.nextInt(600) * 0.5};
            double[] max = {base[0] + w, base[1] + h, base[2] + w};
            double[] vec = {0, 0, 0};
            vec[r.nextInt(3)] = (r.nextBoolean() ? 1 : -1) * (1 + r.nextInt(120)) * 0.25;
            Sweep s = new Sweep(vec, base, max);
            Visits jar = walk(JAR, s, true), shipped = walk(SHIPPED, s, false);
            assertEquals(jar.cells, shipped.cells, "cells of " + describe(s));
            assertEquals(jar.columns, shipped.columns, "columns of " + describe(s));
        }
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
