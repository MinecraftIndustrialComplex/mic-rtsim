package com.minecraftindustrialcomplex.mic_rtsim_dh;

/**
 * When a carriage's boxes must be cut again (research/distant-horizons-2026-10-21.md §4.1). Moving the group's
 * origin is free every frame; changing its boxes makes DH rebuild the group's buffer off-thread (§3.3). So the
 * boxes are re-cut only when the run they were cut for no longer fits: the heading has turned by more than
 * {@link #MAX_DEGREES} (on a curve, or onto a slope), or the run's length has changed by more than
 * {@link #MAX_LENGTH_CHANGE} blocks.
 *
 * <p>Which way the carriage faces does not matter: the cut is symmetric about the midpoint, so a train that
 * reverses keeps its boxes.
 *
 * <p>Plain Java: no DH or Minecraft class.
 */
public final class HeadingRule {
    public static final double MAX_DEGREES = 10.0;
    public static final double MAX_LENGTH_CHANGE = 0.5;

    private HeadingRule() {}

    /**
     * The angle between two runs' lines in degrees, 0 to 90, ignoring which way each faces. 0 when either has
     * no length.
     */
    public static double angleDegrees(double ax, double ay, double az, double bx, double by, double bz) {
        double la = Math.sqrt(ax * ax + ay * ay + az * az);
        double lb = Math.sqrt(bx * bx + by * by + bz * bz);
        if (la < 1e-9 || lb < 1e-9)
            return 0.0;
        double cos = Math.abs(ax * bx + ay * by + az * bz) / (la * lb);
        return Math.toDegrees(Math.acos(Math.min(1.0, cos)));
    }

    /** True when boxes cut for the run {@code (cx, cy, cz)} no longer fit the run {@code (x, y, z)}. */
    public static boolean needsRecut(double cx, double cy, double cz, double x, double y, double z) {
        return needsRecut(cx, cy, cz, x, y, z, MAX_DEGREES, MAX_LENGTH_CHANGE);
    }

    /** {@link #needsRecut(double, double, double, double, double, double)} with its own limits. */
    public static boolean needsRecut(double cx, double cy, double cz, double x, double y, double z,
            double maxDegrees, double maxLengthChange) {
        if (!Double.isFinite(x) || !Double.isFinite(y) || !Double.isFinite(z))
            return false;
        double cutLength = Math.sqrt(cx * cx + cy * cy + cz * cz);
        double length = Math.sqrt(x * x + y * y + z * z);
        if (Math.abs(length - cutLength) > maxLengthChange)
            return true;
        return angleDegrees(cx, cy, cz, x, y, z) > maxDegrees;
    }
}
