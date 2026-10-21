package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import com.llamalad7.mixinextras.injector.ModifyReturnValue;
import com.minecraftindustrialcomplex.mic_rtsim.industry.TickGate;
import net.minecraft.world.level.chunk.LevelChunk;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;

/**
 * Reconcile ahead of arrival (docs/choices-repair.md R2): a chunk loaded with a held reconcile does not
 * tick its block entities until the reconcile has landed. {@code LevelChunk.isTicking(BlockPos)} is what
 * every bound block-entity ticker asks before it ticks; it answers false while the chunk's gate is set
 * ({@link TickGate}). One int field read per call when no gate is set: nothing else changes.
 */
@Mixin(LevelChunk.class)
public abstract class LevelChunkTickGateMixin implements TickGate.Gated {

    @Unique
    private int mic_rtsim$gatedUntil;

    @Override
    public int mic_rtsim$gatedUntil() {
        return mic_rtsim$gatedUntil;
    }

    @Override
    public void mic_rtsim$gateUntil(int serverTick) {
        mic_rtsim$gatedUntil = serverTick;
    }

    @ModifyReturnValue(method = "isTicking(Lnet/minecraft/core/BlockPos;)Z", at = @At("RETURN"))
    private boolean mic_rtsim$tickGate(boolean ticking) {
        if (!ticking || mic_rtsim$gatedUntil == 0)
            return ticking;
        if (((LevelChunk) (Object) this).getLevel().getServer().getTickCount() < mic_rtsim$gatedUntil)
            return false;
        mic_rtsim$gatedUntil = 0; // lapsed (a drain that never came)
        return true;
    }
}
