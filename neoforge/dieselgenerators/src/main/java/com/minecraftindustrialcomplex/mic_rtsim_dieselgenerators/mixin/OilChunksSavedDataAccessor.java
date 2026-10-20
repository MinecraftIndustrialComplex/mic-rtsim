package com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators.mixin;

import com.jesz.createdieselgenerators.world.OilChunksSavedData;
import net.minecraft.world.level.ChunkPos;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

import java.util.Map;

/**
 * The stored oil of chunks a pumpjack (or a command) has written (PATCHES.md): {@code chunks} is
 * package-private. A chunk absent from it is recomputed from the seed on every
 * {@code getChunkOilAmount}, a 12,800-sample biome scan, so the sampler reads the map and computes an
 * unwritten chunk's amount once per session.
 */
@Mixin(value = OilChunksSavedData.class, remap = false)
public interface OilChunksSavedDataAccessor {
    @Accessor("chunks")
    Map<ChunkPos, Integer> mic_rtsim$chunks();
}
