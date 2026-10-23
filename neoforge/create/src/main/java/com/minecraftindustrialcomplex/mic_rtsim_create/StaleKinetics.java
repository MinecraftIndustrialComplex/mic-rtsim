package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim_create.mixin.RotationPropagatorInvoker;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;

import java.util.Objects;

/**
 * The re-sync of a kinetic block entity that loads stale (PATCHES.md row 56, the mixin
 * {@code KineticBlockEntityMixin}): its source is in another chunk, which stayed loaded while this one was
 * not, and gives it another speed or network now.
 */
public final class StaleKinetics {
    private StaleKinetics() {}

    private static int resyncs;

    /** Re-syncs since the server started (for tests). */
    public static int resyncs() {
        return resyncs;
    }

    /**
     * The block entity {@code k} takes its speed from, when that lies in another chunk, is loaded and turns,
     * and gives {@code k} another speed or network than {@code k} has; null otherwise (and on a client, for a
     * source of its own, or when the two are no longer connected: Create's own validation handles those).
     */
    public static KineticBlockEntity staleSource(KineticBlockEntity k) {
        if (!(k.getLevel() instanceof ServerLevel level) || !k.hasSource() || k.isSource())
            return null;
        BlockPos at = k.getBlockPos(), from = k.source;
        if ((at.getX() >> 4) == (from.getX() >> 4) && (at.getZ() >> 4) == (from.getZ() >> 4))
            return null;
        if (!(Loaded.blockEntity(level, from) instanceof KineticBlockEntity s) || s.getTheoreticalSpeed() == 0)
            return null;
        float want = RotationPropagatorInvoker.mic_rtsim$conveyedSpeed(s, k);
        if (want == 0)
            return null;
        float have = k.getTheoreticalSpeed();
        // Create's own tolerance for one speed in a network (propagateNewSource): |speed| / 65536
        boolean sameSpeed = Math.abs(want - have) <= Math.abs(want) / 65536f;
        return sameSpeed && Objects.equals(k.network, s.network) ? null : s;
    }

    /** Clears {@code k}'s part of the network and gives it {@code source}'s speed again, as Create does when a source changes. */
    public static void resync(KineticBlockEntity k, KineticBlockEntity source) {
        resyncs++;
        RotationPropagatorInvoker.mic_rtsim$propagateMissingSource(k);
        RotationPropagatorInvoker.mic_rtsim$propagateNewSource(source);
    }
}
