package com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators.mixin;

import com.jesz.createdieselgenerators.content.pumpjack.PumpjackHoleBlockEntity;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustrySeeds;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.entity.BlockEntity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * A pumpjack hole records itself as a seed of its chunk at its first server tick, whoever set it down (the
 * oil gate audit's F1, choice OGC60; {@code HoleSeeds}). It tries each tick until the industry module
 * answers, then never again for this block entity.
 */
@Mixin(value = PumpjackHoleBlockEntity.class, remap = false)
public abstract class PumpjackHoleSeedMixin {
    @Unique
    private boolean mic_rtsim$seeded;

    @Inject(method = "tick", at = @At("HEAD"))
    private void mic_rtsim$seed(CallbackInfo ci) {
        if (mic_rtsim$seeded)
            return;
        BlockEntity self = (BlockEntity) (Object) this;
        if (!(self.getLevel() instanceof ServerLevel level))
            return;
        mic_rtsim$seeded = IndustrySeeds.record(level, level.getChunkAt(self.getBlockPos()), self);
    }
}
