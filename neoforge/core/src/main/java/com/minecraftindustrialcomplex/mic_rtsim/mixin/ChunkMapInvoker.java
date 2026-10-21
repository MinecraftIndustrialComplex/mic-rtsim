package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import net.minecraft.server.level.ChunkMap;
import net.minecraft.world.level.chunk.ChunkAccess;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

/**
 * {@code ChunkMap.save(ChunkAccess)} (private): vanilla's own save of one chunk (POI flush, serialize,
 * {@code ChunkDataEvent.Save}, queue the write on the chunk IO worker), used to put a chunk the journal
 * speaks for on its way to disk before the journal's file is written (PATCHES.md row 26,
 * docs/choices-repair.md R21).
 */
@Mixin(ChunkMap.class)
public interface ChunkMapInvoker {
    @Invoker("save")
    boolean mic_rtsim$save(ChunkAccess chunk);
}
