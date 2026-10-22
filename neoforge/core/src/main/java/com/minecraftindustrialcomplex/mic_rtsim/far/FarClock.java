package com.minecraftindustrialcomplex.mic_rtsim.far;

/**
 * The far view's clock: client ticks that are not paused. Sources stamp their samples with it
 * ({@link FarMover#sampleTick()}) and a renderer reads it with the frame's partial tick, so both sides of the
 * registry agree on time without either knowing the other's mod. The client side of the core advances it
 * (see {@code MicRtsimClient}); on a server it stays at 0 and nothing reads it.
 */
public final class FarClock {
    private static volatile long ticks;

    private FarClock() {}

    /** Client ticks counted so far. */
    public static long ticks() {
        return ticks;
    }

    /** Counts one client tick. Only the core's client tick handler calls it. */
    public static void advance() {
        ticks++;
    }
}
