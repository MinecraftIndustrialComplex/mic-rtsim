package com.minecraftindustrialcomplex.mic_rtsim.client;

import com.minecraftindustrialcomplex.mic_rtsim.far.FarMover;

/**
 * How one kind of far mover looks as the real thing (docs/choices-dh-feed.md DHT63): a carriage as the blocks
 * its player built, a dragon as Ice and Fire's own model. The plugin that knows the kind (mic_rtsim_create,
 * mic_rtsim_iceandfire) registers one with {@link FarLooks}; a far renderer (mic_rtsim_dh) asks it for each
 * mover it would draw and draws what it places, or its own box when the look has nothing.
 *
 * <p>A look bakes its meshes itself, a slice at a time, in {@link #frame}; the renderer never waits on a bake.
 * Render thread only.
 */
public interface FarLook {
    /** Whether a mover has a model now. */
    enum Status {
        /** Baked: {@link #place} draws it. */
        READY,
        /** Asked for: it bakes, or its shape is still on the way. */
        PENDING,
        /** It never will (no shape, or one that would not decode): the renderer draws its box. */
        NONE
    }

    /** Whether mover {@code m} has a model now; asks for its bake when it does not yet. Cheap: a map lookup. */
    Status status(FarMover m);

    /**
     * Places mover {@code m}'s model for this frame into {@code out}: what to draw and how it turns about the
     * mover's midpoint. {@code seg} is the mover's segment at {@code clock} (the leading point, then the trailing
     * one); {@code live} asks for the live model where the look has one (the nearest few dragons). False when the
     * mover has no model now ({@link #status} is not READY).
     */
    boolean place(FarMover m, double[] seg, double clock, boolean live, FarModel out);

    /**
     * Once a frame, outside every render pass: bakes what was asked for until {@code deadline}
     * ({@link System#nanoTime}), and frees what nobody has drawn for a while. {@code now} is the frame's start.
     */
    void frame(long now, long deadline);

    /** Frees everything it holds (the client left its world). */
    void clear();

    /** What it holds and has done, for the bench's log. */
    String stats();
}
