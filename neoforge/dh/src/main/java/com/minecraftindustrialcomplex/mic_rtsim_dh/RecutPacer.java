package com.minecraftindustrialcomplex.mic_rtsim_dh;

/**
 * Paces one group's box changes.
 *
 * <ul>
 *   <li><b>At most one re-cut per {@link #MIN_INTERVAL_NANOS}.</b> A train on a long curve wants a new cut
 *       every few degrees; DH rebuilds the buffer off-thread, and a few rebuilds a second across all groups
 *       is what it is made for (research/distant-horizons-2026-10-21.md §3.3).</li>
 *   <li><b>A confirming change {@link #CONFIRM_DELAY_NANOS} after the last re-cut.</b> DH 3.3.3's
 *       {@code RenderableBoxGroup.tryUpdateInstancedDataAsync} clears the group's dirty flag after its
 *       off-thread rebuild finishes, so a change made while a rebuild is running is lost and the group keeps
 *       stale boxes. Marking the group changed again once things are quiet makes the last cut stick.</li>
 * </ul>
 *
 * <p>Plain Java: times are {@link System#nanoTime()} values passed in.
 */
public final class RecutPacer {
    public static final long MIN_INTERVAL_NANOS = 250_000_000L;
    public static final long CONFIRM_DELAY_NANOS = 1_500_000_000L;

    private boolean cutYet;
    private long lastCut;
    private boolean confirmPending;
    private long confirmAt;

    /** True when a re-cut may happen at {@code now}. */
    public boolean mayCut(long now) {
        return !cutYet || now - lastCut >= MIN_INTERVAL_NANOS;
    }

    /** Records a cut (or the group's first boxes) at {@code now}, and books its confirmation. */
    public void cut(long now) {
        cutYet = true;
        lastCut = now;
        confirmPending = true;
        confirmAt = now + CONFIRM_DELAY_NANOS;
    }

    /** True once, when the last cut's confirming change is due at {@code now}. */
    public boolean confirmDue(long now) {
        if (confirmPending && now - confirmAt >= 0) {
            confirmPending = false;
            return true;
        }
        return false;
    }
}
