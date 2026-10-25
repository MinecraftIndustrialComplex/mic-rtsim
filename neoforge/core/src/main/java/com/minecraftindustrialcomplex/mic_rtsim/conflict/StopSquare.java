package com.minecraftindustrialcomplex.mic_rtsim.conflict;

/**
 * The stop's geometry (MZB-472, CL-D35): the square of blocks a stopped band's stacks may stand in so that a player
 * sees them and the world ticks them, as pure numbers.
 *
 * <p>A player's entity-ticking chunks lie within Chebyshev distance {@code s} of its chunk, {@code s} being the
 * server's simulation distance. The square is one chunk inside that edge: chunks {@code pcx - s + 1} to
 * {@code pcx + s - 1}, so blocks {@code 16 * (pcx - s + 1)} to {@code 16 * (pcx + s) - 1}, and the same in z. It is
 * {@code {minX, minZ, maxX, maxZ}}, the maxima inclusive; a point is inside when {@code min <= p < max + 1}.
 */
final class StopSquare {
    private StopSquare() {}

    static int[] square(int pcx, int pcz, int simulationDistance) {
        int s = Math.max(1, simulationDistance);
        return new int[] {16 * (pcx - s + 1), 16 * (pcz - s + 1), 16 * (pcx + s) - 1, 16 * (pcz + s) - 1};
    }

    static boolean inside(double x, double z, int[] sq) {
        return x >= sq[0] && x < sq[2] + 1.0 && z >= sq[1] && z < sq[3] + 1.0;
    }

    /** The point pulled into the square: each axis to {@code [min + 0.5, max + 0.5]}. */
    static double[] clampInto(double x, double z, int[] sq) {
        return new double[] {Math.max(sq[0] + 0.5, Math.min(sq[2] + 0.5, x)), Math.max(sq[1] + 0.5, Math.min(sq[3] + 0.5, z))};
    }

    /**
     * Where a stack outside the square comes in: the first point inside the square on the segment from its place
     * {@code (ax, az)} to the head {@code (hx, hz)} (clipped as Liang and Barsky do), pulled to the half block inside;
     * when the segment never enters the square, the head clamped into it.
     */
    static double[] entry(double ax, double az, double hx, double hz, int[] sq) {
        double dx = hx - ax, dz = hz - az;
        double lo = 0.0, hi = 1.0;
        double[] p = {-dx, dx, -dz, dz};
        double[] q = {ax - sq[0], sq[2] + 1.0 - ax, az - sq[1], sq[3] + 1.0 - az};
        for (int i = 0; i < 4; i++) {
            if (p[i] == 0.0) {
                if (q[i] < 0.0)
                    return clampInto(hx, hz, sq);
                continue;
            }
            double r = q[i] / p[i];
            if (p[i] < 0.0) {
                if (r > hi)
                    return clampInto(hx, hz, sq);
                lo = Math.max(lo, r);
            } else {
                if (r < lo)
                    return clampInto(hx, hz, sq);
                hi = Math.min(hi, r);
            }
        }
        return clampInto(ax + dx * lo, az + dz * lo, sq);
    }
}
