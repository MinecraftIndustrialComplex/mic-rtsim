package com.minecraftindustrialcomplex.mic_rtsim.topology;

/**
 * A rigid motion of the block grid: {@code k} clockwise quarter-turns seen from above (one takes north,
 * -z, to east, +x), then {@code (tx, tz)}. Deep Time's wrap engine calls it {@code CellIsometry}; the
 * engine's is {@code rtsim_space::topology::Iso}. On points k = 1 is (x, z) to (-z, x).
 */
public record Iso(int k, long tx, long tz) {

    public static final Iso IDENTITY = new Iso(0, 0, 0);

    public Iso {
        k = Math.floorMod(k, 4);
    }

    public static Iso translation(long tx, long tz) {
        return new Iso(0, tx, tz);
    }

    /** The half-turn about the block corner (a, b). */
    public static Iso halfTurnAbout(long a, long b) {
        return new Iso(2, 2 * a, 2 * b);
    }

    public boolean isIdentity() {
        return k == 0 && tx == 0 && tz == 0;
    }

    /** A point (an entity's position) moved: {x', z'}. */
    public double[] point(double x, double z) {
        double[] r = vector(x, z);
        return new double[]{r[0] + tx, r[1] + tz};
    }

    /** A heading or velocity turned (no translation): {dx', dz'}. */
    public double[] vector(double dx, double dz) {
        return switch (k) {
            case 1 -> new double[]{-dz, dx};
            case 2 -> new double[]{-dx, -dz};
            case 3 -> new double[]{dz, -dx};
            default -> new double[]{dx, dz};
        };
    }

    /** A block cell moved: {x', z'}. */
    public long[] cell(long x, long z) {
        return switch (k) {
            case 1 -> new long[]{-z - 1 + tx, x + tz};
            case 2 -> new long[]{-x - 1 + tx, -z - 1 + tz};
            case 3 -> new long[]{z + tx, -x - 1 + tz};
            default -> new long[]{x + tx, z + tz};
        };
    }

    /** this after other: first {@code other}, then this. */
    public Iso compose(Iso other) {
        double[] t = vector(other.tx, other.tz);
        return new Iso(k + other.k, Math.round(t[0]) + tx, Math.round(t[1]) + tz);
    }

    public Iso inverse() {
        Iso r = new Iso(-k, 0, 0);
        double[] t = r.vector(-tx, -tz);
        return new Iso(-k, Math.round(t[0]), Math.round(t[1]));
    }

    /** Yaw change in degrees for something carried by this motion (Minecraft yaw grows clockwise). */
    public float yawDegrees() {
        return 90f * k;
    }
}
