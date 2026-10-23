package com.minecraftindustrialcomplex.mic_rtsim_dh;

/**
 * Who draws a far mover this frame (docs/choices-dh-feed.md DHT40, DHT45, DHT47): the hand-over rule on its own,
 * apart from how anything is drawn, so another renderer can take the boxes' place and keep it.
 *
 * <ul>
 *   <li><b>The client has the real thing</b> (its entity, near where the mover is, and for a dragon its body
 *       drawn by vanilla): no far model; vanilla draws it. A dragon's body is copied into DH's pass toward DH's
 *       vanilla fade ({@link #bodyCopied}), so the fade mixes its pixels toward itself.</li>
 *   <li><b>Its midpoint is in a chunk the client has not loaded</b>: DH alone.</li>
 *   <li><b>In a loaded chunk</b>: the band renderer, in vanilla's pass. DH draws it too wherever any part of it
 *       could be in DH's vanilla fade ({@link FadeRing}), and {@link #PREPARE} blocks before that: the fade mixes
 *       vanilla's pixels toward what DH drew there, so without DH's copy the band's box fades into the
 *       terrain behind it. Nearer, vanilla's pixels cover DH's copy exactly, so drawing both shows one.</li>
 * </ul>
 *
 * <p>Plain Java: no DH or Minecraft class.
 */
public final class HandOverRule {
    /** Blocks of margin on the fade start for the camera and the mover moving within the frame. */
    public static final double SLACK = 2.0;
    /**
     * DH draws this many blocks nearer than it must, so that a group just made, whose boxes DH builds off-thread
     * over the next frames and only while it is drawn, is ready by the time the fade starts.
     */
    public static final double PREPARE = 16.0;

    private HandOverRule() {}

    /**
     * True when DH draws the mover. {@code distance} is from the camera to its midpoint, in 3D (the fade's own
     * measure); {@code reach} is its farthest point from the midpoint; {@code fadeStart} is
     * {@link FadeRing#start()}.
     */
    public static boolean dh(boolean present, boolean loaded, double distance, double reach, double fadeStart) {
        if (present)
            return false;
        return !loaded || distance + reach >= fadeStart - SLACK - PREPARE;
    }

    /**
     * True when DH's pass draws a copy of a dragon's body that vanilla draws (DHT47): where {@link #dh} would
     * draw the dragon were its body not there. So wherever any part of it could be in DH's vanilla fade, both
     * margins before that ({@code distance} is the body's own, in 3D; {@code reach} the box's, its wingspan; a
     * model's wings reach past it by 15.2 blocks at most in the bench, inside the margins), and anywhere its chunk
     * is not {@code loaded}, where vanilla leaves it out. The fade mixes the body's pixels toward DH's image, so
     * with the copy there, toward themselves; short of the fade, vanilla's pixels cover the copy exactly. Never
     * while the ring is nowhere, in a loaded chunk.
     */
    public static boolean bodyCopied(boolean loaded, double distance, double reach, double fadeStart) {
        return dh(false, loaded, distance, reach, fadeStart);
    }

    /** True when the band renderer draws the mover: in a loaded chunk, with no entity. */
    public static boolean band(boolean present, boolean loaded) {
        return !present && loaded;
    }
}
