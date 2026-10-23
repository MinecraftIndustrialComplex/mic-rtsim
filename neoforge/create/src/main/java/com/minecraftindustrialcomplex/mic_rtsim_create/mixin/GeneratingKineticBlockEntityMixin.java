package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.minecraftindustrialcomplex.mic_rtsim_create.KineticJoin;
import com.simibubi.create.content.kinetics.base.GeneratingKineticBlockEntity;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * A generator that has not joined its network joins it before its rotation update lists it (PATCHES.md
 * row 58, SECTIONS.md choice SC73): {@code updateGeneratedRotation} puts it in the network's sources and
 * members through {@code updateCapacityFor} and {@code updateStressFor}, which never count it out of the
 * unloaded share. A steam engine that ticks updates its powered shaft two blocks away, which can lie in a
 * FULL chunk that does not tick. The join takes the generator's share out at its generated speed now. A
 * powered shaft, whose engine changes its fields before this update, joins before them
 * ({@code PoweredShaftBlockEntityMixin}); another generator whose fields a neighbour changes before its
 * first tick (a water wheel's flow) takes out its share at the new speed, not the one the network counted
 * (row 58's residual).
 */
@Mixin(value = GeneratingKineticBlockEntity.class, remap = false)
public abstract class GeneratingKineticBlockEntityMixin {
    @Inject(method = "updateGeneratedRotation", at = @At("HEAD"))
    private void mic_rtsim$joinFirst(CallbackInfo ci) {
        if (((KineticBlockEntity) (Object) this).network != null)
            ((KineticJoin) this).mic_rtsim$join();
    }
}
