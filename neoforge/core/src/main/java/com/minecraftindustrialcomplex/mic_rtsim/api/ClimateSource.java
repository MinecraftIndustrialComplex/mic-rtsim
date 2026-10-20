package com.minecraftindustrialcomplex.mic_rtsim.api;

import net.minecraft.core.BlockPos;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;

/**
 * A climate the core cannot read itself: a seasons mod's calendar, a temperature mod's degrees Celsius
 * (mic-climate, Serene Seasons, Project Atmosphere through mic-climate; Deep Time's monthly climate later).
 * The habitat sampler (docs/boundary.md §10) asks the registered sources, first answer wins, and falls back
 * to vanilla's biome values (no seasons, the biome's unitless temperature). Register with
 * {@link RtsimApi#registerClimate} during mod construction.
 *
 * <p>The two methods have the shapes of the industry plugins' {@code IndustryPlugin.season} and
 * {@code temperature} (IND_CONDITIONS), so one plugin class can answer both (choice X2-7).
 */
public interface ClimateSource {

    /** A short name for logs. */
    String name();

    /**
     * The world's season calendar, {@code {season, seasonCount, seasonLength (ticks), seasonOrigin (the game
     * time season 0 last began)}}, or null when this source knows none.
     */
    default long[] season(MinecraftServer server) {
        return null;
    }

    /** The temperature at a position in degrees Celsius, or NaN when this source knows none. */
    default float temperature(ServerLevel level, BlockPos pos) {
        return Float.NaN;
    }
}
