package com.minecraftindustrialcomplex.mic_rtsim.client;

/**
 * A far look's side of a far feed RESET (docs/choices-dh-feed.md DHT70): what it baked for a mover the feed does
 * not send again goes {@link #GRACE_NANOS} after the RESET, and what a mover asked for again by then stays. The
 * server sends a RESET on every subscription (the client's radius, detail or kinds changed, a new dimension, a
 * resubscribe), and every mover it still chooses comes again with it, so freeing at once would blank every far
 * model and bake them all again; waiting for the looks' own sweep would keep a mesh nobody wants for a minute.
 *
 * <p>A look stamps what it draws with the frame's clock, as it does for its sweep; {@link #reset} notes the clock
 * of the RESET, and the first frame {@link #due} past the grace frees each thing whose stamp is not later
 * ({@link #stale}). Render thread.
 */
public final class FarLookReset {
    /** How long after a RESET a mover has to be asked for again (PLACEHOLDER: the models' own 2 s grace). */
    public static final long GRACE_NANOS = 2_000_000_000L;

    private long at;
    private boolean pending;

    /** The far feed started over; {@code clock} is the look's frame clock now. A second RESET starts the grace again. */
    public void reset(long clock) {
        at = clock;
        pending = true;
    }

    /** At a frame's start ({@code now}): true once, the first frame the grace is over; then free what {@link #stale}. */
    public boolean due(long now) {
        if (!pending || now - at < GRACE_NANOS)
            return false;
        pending = false;
        return true;
    }

    /** Whether something last stamped {@code used} was not asked for since the RESET. */
    public boolean stale(long used) {
        return used - at <= 0;
    }

    /** Forgets a RESET not yet due (the look freed everything anyway). */
    public void clear() {
        pending = false;
    }
}
