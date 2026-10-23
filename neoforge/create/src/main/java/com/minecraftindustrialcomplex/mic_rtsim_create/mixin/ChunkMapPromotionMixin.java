package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.minecraftindustrialcomplex.mic_rtsim_create.StaleKinetics;
import net.minecraft.server.level.ChunkMap;
import net.minecraft.server.level.FullChunkStatus;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * A chunk that becomes FULL again without loading (PATCHES.md row 57; SECTIONS.md §2.8, choice SC37). A chunk
 * that drops below FULL (ticket level 34 to 44) stays in memory with its block entities; when it is FULL again
 * vanilla runs no load ({@code ChunkStatusTasks.full} ran once; no {@code ChunkEvent.Load}) and Create attaches
 * nothing, so a kinetic part there that went stale while it was away would stay stale.
 * {@code ChunkMap.onFullChunkStatusChange} runs on the server thread when the holder's full status changes
 * (after the promotion's future completes, or at a demotion); at FULL the chunk is queued for
 * {@link StaleKinetics#chunkFull}, drained at the next server tick's start or end, outside whatever chunk
 * wait ran the promotion.
 */
@Mixin(value = ChunkMap.class, remap = false)
public abstract class ChunkMapPromotionMixin {
    @Shadow
    @Final
    ServerLevel level;

    @Inject(method = "onFullChunkStatusChange", at = @At("TAIL"))
    private void mic_rtsim$fullAgain(ChunkPos pos, FullChunkStatus status, CallbackInfo ci) {
        if (status == FullChunkStatus.FULL)
            StaleKinetics.becameFull(level, pos);
    }
}
