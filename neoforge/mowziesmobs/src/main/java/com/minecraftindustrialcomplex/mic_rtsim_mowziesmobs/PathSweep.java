package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs;

/**
 * The grid walk of Mowzie's Mobs 1.8.2's {@code MMPathNavigateGround.sweep} (the path shortcut: may a mob's box walk the
 * straight line to a path node), made exact far from the origin (MZB-951). Pure Java, no Minecraft class: the mixin
 * ({@code MMPathNavigateGroundMixin}) runs it and {@link PathColumns} answers for the world, so the unit tests run the code that
 * ships.
 *
 * <p>The jar's walk steps a grid in {@code float} on world coordinates. Beyond 2^22 blocks a float is a multiple of 0.5, the
 * trailing edge of the box overtakes the leading one, and its loops {@code for (z = z0; z != z1; z += step)} then start
 * beyond their end and count through the int range, a {@code getBlockState} (a chunk load) at each step (MZB-950: one sweep of
 * 40 blocks, minutes, tens of thousands of chunks). This walk is the same algorithm with two changes:
 *
 * <ul>
 * <li>Every coordinate is a {@code double} measured from the integer cell the box's low corner stands in, so the numbers it
 * works on are a few blocks wide wherever the mob is, and the cells are the same for the same box translated by whole blocks
 * (bit for bit when the box's edges are exact in a double, which a block position plus a multiple of 2^-6 is).</li>
 * <li>The three cell loops end by direction ({@code x < x1} for a step of +1, {@code x > x1} for -1), not by {@code !=}: a
 * range that starts past its end is empty, never the int range. And a walk ends after as many crossings as its vector has
 * cell boundaries to cross (their sum, 8 more), the one bound the maths does not need and a bad input might.</li>
 * </ul>
 *
 * <p>What is kept: the walk moves the box's leading corner from one cell boundary to the next along the vector, in the order
 * of the earliest crossing, and at each crossing hands over the slab of cells that corner entered: the one cell layer
 * across the crossed axis, and across the other two the cells from the trailing edge to the leading edge (the box's own
 * span). The last crossing is the first one after the vector's end, as in the jar, so the slab beyond the end is read too.
 * An edge on an exact integer is in the cell above it, as the jar's float finds it (its {@code 1.0E-8} nudge is lost in a float
 * beyond a quarter block of the world origin, where this one does not apply it either). The cell loops run x outermost,
 * then z, and a column is one (x, z) with its y range, as the jar reads it.
 */
public final class PathSweep {
    private PathSweep() {}

    /** The jar's {@code EPSILON}: a vector shorter than this is no move and is clear. */
    static final double MIN_LENGTH = 1.0E-8;

    /** No coordinate or vector component at or beyond this (or NaN) is walked: the answer is blocked, with nothing read. */
    static final double LIMIT = 1.0E9;

    /** What a walk asks of the world, one column at a time. */
    @FunctionalInterface
    public interface Columns {
        /**
         * The column at (x, z): the cells y from {@code y0} by {@code stepY} (1 or -1) up to {@code y1}, exclusive (an empty
         * range when {@code y0} is already past {@code y1} in the step's direction), and the ground under {@code y0}. True
         * when the way through it is clear, false when it is blocked, which ends the walk.
         */
        boolean column(int x, int z, int y0, int y1, int stepY);
    }

    /**
     * A bound on what a walk reads, set by GameTests that call the jar's real sweep far from the origin ({@link #budget}):
     * the cells it may read and the time it may take. A walk past either one stops, blocked, and the budget says it overran.
     */
    public static final class Budget {
        private final long maxCells;
        private final long deadlineNanos;
        private long cells, columns;
        private boolean overran;

        public Budget(long maxCells, long millis) {
            this.maxCells = maxCells;
            this.deadlineNanos = System.nanoTime() + millis * 1_000_000L;
        }

        boolean spend(int yCells) {
            columns++;
            cells += yCells + 1;
            if (cells > maxCells || System.nanoTime() - deadlineNanos > 0)
                overran = true;
            return !overran;
        }

        /** The block reads so far: each column's y cells and its one ground read. */
        public long cells() {
            return cells;
        }

        public long columns() {
            return columns;
        }

        public boolean overran() {
            return overran;
        }
    }

    private static Budget budget;

    /** Sets (or clears, with null) the budget every walk is counted against. The server thread's: a GameTest sets it and clears it. */
    public static void budget(Budget b) {
        budget = b;
    }

    static int floor(double d) {
        int i = (int) d;
        return d < i ? i - 1 : i;
    }

    /** Whether {@code a} has not yet reached {@code b} going by {@code step} (the loops' test: {@code !=} counts through the int range). */
    private static boolean before(int a, int b, int step) {
        return step > 0 ? a < b : a > b;
    }

    /**
     * Walks the box from {@code base} to {@code max} (its low and high corners) along {@code vec}: true when every column its
     * walk reads is clear (or the vector is no move), false at the first that is blocked.
     */
    public static boolean sweep(double[] vec, double[] base, double[] max, Columns columns) {
        for (int i = 0; i < 3; i++)
            if (!(Math.abs(vec[i]) < LIMIT) || !(Math.abs(base[i]) < LIMIT) || !(Math.abs(max[i]) < LIMIT))
                return false;
        double maxT = Math.sqrt(vec[0] * vec[0] + vec[1] * vec[1] + vec[2] * vec[2]);
        if (maxT < MIN_LENGTH)
            return true;
        int[] o = new int[3], ldi = new int[3], tri = new int[3], step = new int[3];
        double[] tr = new double[3], tDelta = new double[3], tNext = new double[3], normed = new double[3];
        long crossings = 0;
        for (int i = 0; i < 3; i++) {
            double value = vec[i];
            boolean dir = value >= 0.0;
            step[i] = dir ? 1 : -1;
            o[i] = floor(base[i]);
            double lead = (dir ? max : base)[i] - o[i];
            tr[i] = (dir ? base : max)[i] - o[i];
            ldi[i] = floor(lead);
            tri[i] = floor(tr[i]);
            normed[i] = value / maxT;
            tDelta[i] = Math.abs(maxT / value);
            double dist = dir ? (ldi[i] + 1) - lead : lead - ldi[i];
            tNext[i] = tDelta[i] < Double.POSITIVE_INFINITY ? tDelta[i] * dist : Double.POSITIVE_INFINITY;
            crossings += (long) Math.ceil(Math.abs(value));
        }
        crossings += 8;
        double[] tr0 = tr.clone();
        Budget b = budget;
        double t;
        do {
            if (--crossings < 0)
                return false;
            int axis = tNext[0] < tNext[1] ? (tNext[0] < tNext[2] ? 0 : 2) : (tNext[1] < tNext[2] ? 1 : 2);
            t = tNext[axis];
            ldi[axis] += step[axis];
            tNext[axis] += tDelta[axis];
            for (int i = 0; i < 3; i++)
                tri[i] = floor(tr0[i] + t * normed[i]);
            int x0 = axis == 0 ? ldi[0] : tri[0], x1 = ldi[0] + step[0];
            int y0 = axis == 1 ? ldi[1] : tri[1], y1 = ldi[1] + step[1];
            int z0 = axis == 2 ? ldi[2] : tri[2], z1 = ldi[2] + step[2];
            int ySpan = before(y0, y1, step[1]) ? Math.abs(y1 - y0) : 0;
            for (int x = x0; before(x, x1, step[0]); x += step[0])
                for (int z = z0; before(z, z1, step[2]); z += step[2]) {
                    if (b != null && !b.spend(ySpan))
                        return false;
                    if (!columns.column(o[0] + x, o[2] + z, o[1] + y0, o[1] + y1, step[1]))
                        return false;
                }
        } while (t <= maxT);
        return true;
    }
}
