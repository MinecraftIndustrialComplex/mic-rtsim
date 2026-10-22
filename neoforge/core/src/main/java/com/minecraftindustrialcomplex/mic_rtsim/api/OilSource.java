package com.minecraftindustrialcomplex.mic_rtsim.api;

import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;

/**
 * Where oil is (OILGATE.md §4.1, OG-2): a mod that knows a chunk's oil, asked by the plugins that tell
 * the engine (the Ice and Fire plugin's OIL_CHUNKS). Create: Diesel Generators' plugin registers its
 * {@code getBaseOilAmount}; the plugins that ask never name the mod. Register with
 * {@link RtsimApi#registerOilSource} during mod construction. Server thread.
 */
public interface OilSource {
    /** OIL_CHUNKS's source codes (docs/boundary.md §6.7). */
    int DEEP_TIME = 1, CDG = 2, DEPOSIT = 3;

    /** A short name for logs. */
    String name();

    /** The OIL_CHUNKS source code of this source's answers. */
    int source();

    /** True when this source answers for {@code level}. */
    default boolean covers(ServerLevel level) {
        return true;
    }

    /**
     * The chunk's untouched oil in the source's unit (CDG: millibuckets), {@link Integer#MAX_VALUE} for a
     * bottomless chunk, 0 for none. The chunk need not be generated: a source that reads biomes may then
     * cost far more (OILGATE.md §7 test 4).
     */
    int baseOil(ServerLevel level, ChunkPos chunk);
}
