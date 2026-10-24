package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.simibubi.create.content.kinetics.RotationPropagator;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

/**
 * Create's own propagation steps (private static in {@code RotationPropagator}), for the re-sync of a
 * part of a network that comes back stale ({@link KineticBlockEntityMixin}, PATCHES.md row 56).
 */
@Mixin(value = RotationPropagator.class, remap = false)
public interface RotationPropagatorInvoker {
    /** The speed {@code from} gives {@code to} (0 when they are not connected). */
    @Invoker("getConveyedSpeed")
    static float mic_rtsim$conveyedSpeed(KineticBlockEntity from, KineticBlockEntity to) {
        throw new AssertionError();
    }

    /** Clears {@code be} and everything that takes its speed from it, then finds another source. */
    @Invoker("propagateMissingSource")
    static void mic_rtsim$propagateMissingSource(KineticBlockEntity be) {
        throw new AssertionError();
    }

    /** Gives {@code be}'s speed to its neighbours, as Create does when a source is added. */
    @Invoker("propagateNewSource")
    static void mic_rtsim$propagateNewSource(KineticBlockEntity be) {
        throw new AssertionError();
    }
}
