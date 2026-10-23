package com.minecraftindustrialcomplex.mic_rtsim.far;

/**
 * One far mover as its source last saw it: a segment (a leading and a trailing point) at two samples, how to
 * draw it, and how a renderer can tell that the real thing is on the client already.
 *
 * <p>The segment moves from {@code prev} to {@code cur} over {@code intervalTicks}, starting at
 * {@code sampleTick} on {@link FarClock}: a renderer draws {@code prev} at the sample tick, {@code cur} one
 * interval later, and holds {@code cur} after that (the way Create's own train map interpolates). A mover with
 * a {@link #circle()} circles instead and ignores both. Both arrays are leading x, y, z then trailing x, y, z,
 * in block coordinates; nobody changes them after the record is made.
 *
 * <p>What the segment means depends on the kind: a carriage's runs from its trailing to its leading bogey
 * point; a dragon's midpoint is the dragon's centre and its direction the heading, and its length means
 * nothing (the renderer's model knows how long a dragon of each stage is).
 *
 * @param key            which mover
 * @param kind           what it is
 * @param dimension      the level it is in, as a resource location ({@code minecraft:overworld})
 * @param prev           the older sample's points (6 values)
 * @param cur            the newer sample's points (6 values)
 * @param sampleTick     the {@link FarClock} tick the move from {@code prev} to {@code cur} starts at
 * @param intervalTicks  how long the move takes, in client ticks (above 0)
 * @param rgb            the colour, 0xRRGGBB
 * @param real           the source knows the client has the real thing (an entity), so a far view hides it
 * @param state          what it is doing (a dragon's pose); {@link FarState#MOVING} for a carriage
 * @param style          the kind's own sub-type (a dragon's species: 0 fire, 1 ice, 2 lightning)
 * @param variant        the colour variant within the style, 0 to 3
 * @param size           how big (a dragon's stage, 1 to 5); 0 when the kind has one size
 * @param circle         the circle it flies, or null
 * @param entity         the client's network id of the real entity, or -1: while the client's level has an
 *                       entity of that id, a far view hides the mover (research note §5)
 * @param shape          what it looks like: a {@link FarShape}'s hash (a carriage's blocks), whose bytes the far
 *                       feed's client keeps; 0 for none (drawn as a box)
 * @param age            its age in days (a dragon's, for its size between stages), or -1 for unknown
 */
public record FarMover(FarKey key, FarKind kind, String dimension, double[] prev, double[] cur,
        double sampleTick, double intervalTicks, int rgb, boolean real, FarState state, int style, int variant,
        int size, FarCircle circle, int entity, long shape, int age) {

    public FarMover {
        if (prev.length != 6 || cur.length != 6)
            throw new IllegalArgumentException("a segment is 6 values, got " + prev.length + " and " + cur.length);
        if (!(intervalTicks > 0.0))
            throw new IllegalArgumentException("intervalTicks must be above 0, got " + intervalTicks);
        if (state == null || state == FarState.GONE)
            throw new IllegalArgumentException("a mover's state must be set and not GONE, got " + state);
    }

    /** A mover with no shape and no age. */
    public FarMover(FarKey key, FarKind kind, String dimension, double[] prev, double[] cur, double sampleTick,
            double intervalTicks, int rgb, boolean real, FarState state, int style, int variant, int size,
            FarCircle circle, int entity) {
        this(key, kind, dimension, prev, cur, sampleTick, intervalTicks, rgb, real, state, style, variant, size, circle,
                entity, 0L, -1);
    }

    /** A carriage-like mover, as phase 1's sources make them: moving, one style, no circle, no entity id. */
    public FarMover(FarKey key, FarKind kind, String dimension, double[] prev, double[] cur, double sampleTick,
            double intervalTicks, int rgb, boolean real) {
        this(key, kind, dimension, prev, cur, sampleTick, intervalTicks, rgb, real, FarState.MOVING, 0, 0, 0,
                null, -1, 0L, -1);
    }

    /** How far along the move from {@code prev} to {@code cur} the mover is at {@code now}, from 0 to 1. */
    public double fraction(double now) {
        double f = (now - sampleTick) / intervalTicks;
        return f <= 0.0 ? 0.0 : f >= 1.0 ? 1.0 : f;
    }

    /** Writes the segment at {@code now} into {@code out} (6 values: leading x, y, z, trailing x, y, z). */
    public void segmentAt(double now, double[] out) {
        if (circle != null) {
            circle.segmentAt(now, out);
            return;
        }
        double f = fraction(now);
        for (int i = 0; i < 6; i++)
            out[i] = prev[i] + (cur[i] - prev[i]) * f;
    }
}
