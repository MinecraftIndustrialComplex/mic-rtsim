package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import net.minecraft.core.BlockPos;
import net.minecraft.world.level.chunk.LevelChunk;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

/**
 * {@code LevelChunk.isTicking(BlockPos)} (package-private): the test every bound block entity ticker asks
 * before it ticks (world border, full status at least BLOCK_TICKING, entities loaded, and the core's
 * reconcile gate), for {@code Loaded.ticks} (PATCHES.md rows 50 to 52).
 */
@Mixin(value = LevelChunk.class, remap = false)
public interface LevelChunkInvoker {
    @Invoker("isTicking")
    boolean mic_rtsim$isTicking(BlockPos pos);
}
