package com.minecraftindustrialcomplex.mic_rtsim.far;

/**
 * One far mover as its source last saw it: a segment (a leading and a trailing point) at two samples, how to
 * colour it, and whether the real thing is on the client already.
 *
 * <p>The segment moves from {@code prev} to {@code cur} over {@code intervalTicks}, starting at
 * {@code sampleTick} on {@link FarClock}: a renderer draws {@code prev} at the sample tick, {@code cur} one
 * interval later, and holds {@code cur} after that (the way Create's own train map interpolates). Both arrays
 * are leading x, y, z then trailing x, y, z, in block coordinates; nobody changes them after the record is
 * made.
 *
 * @param key            which mover
 * @param kind           what it is
 * @param dimension      the level it is in, as a resource location ({@code minecraft:overworld})
 * @param prev           the older sample's points (6 values)
 * @param cur            the newer sample's points (6 values)
 * @param sampleTick     the {@link FarClock} tick the move from {@code prev} to {@code cur} starts at
 * @param intervalTicks  how long the move takes, in client ticks (above 0)
 * @param rgb            the colour, 0xRRGGBB
 * @param real           the client has the real thing (an entity) for this mover, so a far view hides it
 */
public record FarMover(FarKey key, FarKind kind, String dimension, double[] prev, double[] cur,
        double sampleTick, double intervalTicks, int rgb, boolean real) {

    public FarMover {
        if (prev.length != 6 || cur.length != 6)
            throw new IllegalArgumentException("a segment is 6 values, got " + prev.length + " and " + cur.length);
        if (!(intervalTicks > 0.0))
            throw new IllegalArgumentException("intervalTicks must be above 0, got " + intervalTicks);
    }

    /** How far along the move from {@code prev} to {@code cur} the mover is at {@code now}, from 0 to 1. */
    public double fraction(double now) {
        double f = (now - sampleTick) / intervalTicks;
        return f <= 0.0 ? 0.0 : f >= 1.0 ? 1.0 : f;
    }

    /** Writes the segment at {@code now} into {@code out} (6 values: leading x, y, z, trailing x, y, z). */
    public void segmentAt(double now, double[] out) {
        double f = fraction(now);
        for (int i = 0; i < 6; i++)
            out[i] = prev[i] + (cur[i] - prev[i]) * f;
    }
}
