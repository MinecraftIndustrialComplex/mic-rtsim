package com.minecraftindustrialcomplex.mic_rtsim.industry;

import net.minecraft.world.level.chunk.LevelChunk;

import java.util.ArrayList;
import java.util.List;

/**
 * Holds a chunk's block entities still until a held reconcile has landed in it (reconcile ahead of
 * arrival, docs/choices-repair.md R2). {@code ChunkEvent.Load} fires inside the chunk system's main-thread
 * task, where applying the reconcile could load a neighbour synchronously (a block update, a plugin's
 * multiblock); the reconcile lands at the next drain instead (the start of the next server tick for a
 * load between ticks, the end of this tick for a load inside one), and until then
 * {@code LevelChunk.isTicking} answers false for the chunk and for the district's other loaded chunks
 * (core mixin {@code LevelChunkTickGateMixin}, PATCHES.md). A gate lapses by itself after
 * {@link #MAX_TICKS} server ticks, so a failed session or a module switched off never freezes a chunk.
 */
public final class TickGate {
    private TickGate() {}

    /** Implemented by {@code LevelChunk} through the core's mixin. */
    public interface Gated {
        /** The server tick count before which the chunk's block entities do not tick; 0 for none. */
        int mic_rtsim$gatedUntil();

        void mic_rtsim$gateUntil(int serverTick);
    }

    /** A gate never outlives this many server ticks (one drain is at most one tick away). */
    static final int MAX_TICKS = 20;

    private static final List<LevelChunk> GATED = new ArrayList<>();

    /** Holds the chunk's block entities still until the next {@link #releaseAll}, or {@link #MAX_TICKS}. */
    static void hold(LevelChunk chunk) {
        int now = chunk.getLevel().getServer().getTickCount();
        Gated g = (Gated) chunk;
        boolean listed = g.mic_rtsim$gatedUntil() != 0;
        g.mic_rtsim$gateUntil(now + MAX_TICKS);
        if (listed)
            return;
        if (GATED.size() >= 4096) // no drain for a while (a failed session): forget the lapsed ones
            GATED.removeIf(c -> ((Gated) c).mic_rtsim$gatedUntil() <= now);
        GATED.add(chunk);
    }

    /** Every gate lifts: the held reconciles of the chunks loaded since the last drain have landed. */
    static void releaseAll() {
        if (GATED.isEmpty())
            return;
        for (LevelChunk c : GATED)
            ((Gated) c).mic_rtsim$gateUntil(0);
        GATED.clear();
    }

    /** True while the chunk's block entities are held still (GameTests). */
    public static boolean holds(LevelChunk chunk) {
        int until = ((Gated) chunk).mic_rtsim$gatedUntil();
        return until != 0 && chunk.getLevel().getServer().getTickCount() < until;
    }
}
