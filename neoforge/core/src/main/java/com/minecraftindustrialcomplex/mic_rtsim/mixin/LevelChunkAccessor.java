package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import net.minecraft.world.level.chunk.LevelChunk;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/**
 * {@code LevelChunk.loaded} (private, no getter): true from the chunk's promotion to FULL
 * ({@code ChunkStatusTasks.full}) until its unload task, which clears it just before vanilla's own save of
 * the chunk. The journal's chunk barrier saves a chunk only while it is in its level: an instance that left
 * was saved by its unload, and writing it again could put stale contents over a newer instance's
 * (PATCHES.md row 26, docs/choices-repair.md R21).
 */
@Mixin(LevelChunk.class)
public interface LevelChunkAccessor {
    @Accessor("loaded")
    boolean mic_rtsim$loaded();
}
