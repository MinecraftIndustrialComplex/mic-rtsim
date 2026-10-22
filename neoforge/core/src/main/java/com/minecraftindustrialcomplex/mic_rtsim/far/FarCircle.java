package com.minecraftindustrialcomplex.mic_rtsim.far;

/**
 * A mover circling a centre (a hunting dragon, research/distant-horizons-2026-10-21.md §4.2): at
 * {@code angle = phase0 + omega * (now - startTick)} it is at {@code centre + radius * (cos, sin)} on x and z,
 * heading along the circle. It may join the circle from off it: then its distance from the centre starts at
 * {@code joinRadius} and eases to {@code radius} over {@code joinTicks}, a spiral in or out. Ticks are
 * {@link FarClock}'s.
 *
 * @param cx       centre x, blocks
 * @param cy       height, blocks
 * @param cz       centre z, blocks
 * @param radius   blocks, above 0
 * @param omega    radians per tick; the sign is the direction
 * @param phase0   radians at {@code startTick}
 * @param startTick the {@link FarClock} tick {@code phase0} holds at
 * @param joinRadius blocks from the centre at {@code startTick}
 * @param joinTicks  ticks to reach the circle from {@code joinRadius}; 0 is on it from the start
 */
public record FarCircle(double cx, double cy, double cz, double radius, double omega, double phase0,
        double startTick, double joinRadius, double joinTicks) {

    public FarCircle {
        if (!(radius > 0.0))
            throw new IllegalArgumentException("a circle's radius must be above 0, got " + radius);
        if (!(joinRadius >= 0.0) || !(joinTicks >= 0.0))
            throw new IllegalArgumentException("a circle's join must not be negative, got " + joinRadius + " over "
                    + joinTicks + " ticks");
    }

    /** On the circle from the start. */
    public FarCircle(double cx, double cy, double cz, double radius, double omega, double phase0, double startTick) {
        this(cx, cy, cz, radius, omega, phase0, startTick, radius, 0.0);
    }

    /** Its distance from the centre at {@code now}. */
    public double radiusAt(double now) {
        if (joinTicks <= 0.0)
            return radius;
        double t = Math.max(0.0, Math.min(1.0, (now - startTick) / joinTicks));
        double ease = t * t * (3.0 - 2.0 * t);
        return joinRadius + (radius - joinRadius) * ease;
    }

    /**
     * Writes the segment at {@code now} into {@code out}: one block along the circle's tangent, centred on the
     * point on the circle, leading end ahead, so a renderer reads the position (the midpoint) and the heading
     * as from any other dragon's segment.
     */
    public void segmentAt(double now, double[] out) {
        double a = phase0 + omega * (now - startTick);
        double c = Math.cos(a), s = Math.sin(a);
        double r = radiusAt(now);
        double x = cx + r * c, z = cz + r * s;
        // The tangent, in the direction of travel
        double sign = omega >= 0.0 ? 1.0 : -1.0;
        double tx = -s * sign * 0.5, tz = c * sign * 0.5;
        out[0] = x + tx;
        out[1] = cy;
        out[2] = z + tz;
        out[3] = x - tx;
        out[4] = cy;
        out[5] = z - tz;
    }
}
