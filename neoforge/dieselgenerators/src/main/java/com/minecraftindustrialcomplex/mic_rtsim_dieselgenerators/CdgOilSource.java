package com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators;

import com.jesz.createdieselgenerators.world.OilChunksSavedData;
import com.minecraftindustrialcomplex.mic_rtsim.api.OilSource;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;

/**
 * Create: Diesel Generators' oil for the oil gate (OILGATE.md §4.1, OG-2): a chunk's base oil,
 * {@code OilChunksSavedData.getBaseOilAmount} (the seed's Perlin field shaped by the chunk's biomes, 12,800
 * {@code getBiome} lookups between y 60 and 109; under KubeJS, the pack's own rule), what the chunk holds
 * before any pumpjack drew from it. {@link Integer#MAX_VALUE} past Diesel's infinite threshold. In a Deep
 * Time level Deep Time's mixin answers at the head of the same method (its trap's share).
 */
public final class CdgOilSource implements OilSource {
    @Override
    public String name() {
        return "createdieselgenerators";
    }

    @Override
    public int source() {
        return CDG;
    }

    @Override
    public int baseOil(ServerLevel level, ChunkPos chunk) {
        return OilChunksSavedData.getBaseOilAmount(level, chunk);
    }
}
