package com.minecraftindustrialcomplex.mic_rtsim.client;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import java.util.Arrays;

/**
 * The far looks, one per kind of far mover (docs/choices-dh-feed.md DHT63): plugins register theirs as they load,
 * a far renderer asks here. With no look for a kind, or no renderer, nothing is baked and the boxes stay.
 */
public final class FarLooks {
    /** What every look may spend baking in one frame, together (PLACEHOLDER: the spike's 2 ms kept every frame smooth). */
    public static final long BUDGET_NANOS = 2_000_000L;

    private static volatile FarLook[] looks = new FarLook[FarKind.values().length];
    private static int errors;

    private FarLooks() {}

    /** Registers the look of {@code kind} (a plugin's client entry, as it loads); a second one replaces the first. */
    public static synchronized void register(FarKind kind, FarLook look) {
        FarLook[] next = Arrays.copyOf(looks, looks.length);
        next[kind.ordinal()] = look;
        looks = next;
    }

    /** The look of {@code kind}, or null. */
    public static FarLook of(FarKind kind) {
        return looks[kind.ordinal()];
    }

    /**
     * Once a frame, outside every render pass (the far renderer calls it at the frame's start): each look bakes
     * within what is left of {@link #BUDGET_NANOS} and frees what nobody drew for a while.
     */
    public static void frame() {
        long now = System.nanoTime(), deadline = now + BUDGET_NANOS;
        for (FarLook look : looks) {
            if (look == null)
                continue;
            try {
                look.frame(now, deadline);
            } catch (RuntimeException | LinkageError e) {
                if (errors++ < 5)
                    MicRtsim.LOGGER.error("[mic_rtsim] a far look's bake failed this frame", e);
            }
        }
    }

    /** Frees every look's meshes (the client left its world). Render thread. */
    public static void clearAll() {
        for (FarLook look : looks) {
            if (look == null)
                continue;
            try {
                look.clear();
            } catch (RuntimeException | LinkageError e) {
                MicRtsim.LOGGER.warn("[mic_rtsim] a far look would not clear", e);
            }
        }
    }
}
