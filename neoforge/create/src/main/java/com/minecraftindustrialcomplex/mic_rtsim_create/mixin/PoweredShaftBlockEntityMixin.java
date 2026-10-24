package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.minecraftindustrialcomplex.mic_rtsim_create.KineticJoin;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import com.simibubi.create.content.kinetics.steamEngine.PoweredShaftBlockEntity;
import net.minecraft.core.BlockPos;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * A powered shaft that has not joined its network joins it before its steam engine changes it (PATCHES.md
 * row 58, SECTIONS.md choice SC73). {@code update} and {@code remove(BlockPos)} set the shaft's engine
 * efficiency, capacity key and direction before {@code updateGeneratedRotation}, where
 * {@code GeneratingKineticBlockEntityMixin} would join it: the join would then take out a share at the new
 * generated speed, which the network never counted (a shaft saved cold whose engine heats). A steam engine
 * that ticks updates its shaft two blocks away, which can lie in a FULL chunk that does not tick. Joined
 * here, the shaft takes out the share it saved, as its first tick would have before the engine's update.
 */
@Mixin(value = PoweredShaftBlockEntity.class, remap = false)
public abstract class PoweredShaftBlockEntityMixin {
    @Inject(method = "update(Lnet/minecraft/core/BlockPos;IF)V", at = @At("HEAD"))
    private void mic_rtsim$joinBeforeUpdate(BlockPos sourcePos, int direction, float efficiency, CallbackInfo ci) {
        mic_rtsim$joinFirst();
    }

    @Inject(method = "remove(Lnet/minecraft/core/BlockPos;)V", at = @At("HEAD"))
    private void mic_rtsim$joinBeforeRemove(BlockPos sourcePos, CallbackInfo ci) {
        mic_rtsim$joinFirst();
    }

    @Unique
    private void mic_rtsim$joinFirst() {
        if (((KineticBlockEntity) (Object) this).network != null)
            ((KineticJoin) this).mic_rtsim$join();
    }
}
