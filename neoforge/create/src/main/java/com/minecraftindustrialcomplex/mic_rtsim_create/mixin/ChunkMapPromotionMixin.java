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
 * A chunk that becomes FULL again, with or without loading (PATCHES.md row 58; SECTIONS.md §2.8, choices SC37
 * and SC73). A chunk that drops below FULL (ticket level 34 to 44) stays in memory with its block entities;
 * when it is FULL again vanilla runs no load ({@code ChunkStatusTasks.full} ran once; no
 * {@code ChunkEvent.Load}) and Create attaches nothing, so a kinetic part there that went stale while it was
 * away would stay stale. A chunk that loads anew FULL and does not tick (ticket level 33) attaches nothing
 * either. {@code ChunkMap.onFullChunkStatusChange} runs on the server thread when the holder's full status
 * changes (after the promotion's future completes, or at a demotion). A chunk demoted below FULL, or loaded
 * ({@code ChunkEvent.Load}, before its first report), is noted; one noted that reaches FULL or more is queued
 * for {@link StaleKinetics#chunkFull}, drained at the next server tick's start or end, outside whatever chunk
 * wait ran the promotion. Chunks that never left FULL are not queued.
 */
@Mixin(value = ChunkMap.class, remap = false)
public abstract class ChunkMapPromotionMixin {
    @Shadow
    @Final
    ServerLevel level;

    @Inject(method = "onFullChunkStatusChange", at = @At("TAIL"))
    private void mic_rtsim$fullAgain(ChunkPos pos, FullChunkStatus status, CallbackInfo ci) {
        StaleKinetics.fullStatusChanged(level, pos, status);
    }
}
