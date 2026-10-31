package com.minecraftindustrialcomplex.mic_rtsim.api;

import net.minecraft.core.BlockPos;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;

/**
 * A climate the core cannot read itself: a seasons mod's calendar, a climate mod's year-round
 * temperature (mic-climate, Serene Seasons; Deep Time's climate later). The habitat sampler
 * (docs/boundary.md §10) asks the registered sources, first answer wins, and falls back to vanilla's
 * biome values (no seasons; the biome's temperature on mic-climate's scale). Register with
 * {@link RtsimApi#registerClimate} during mod construction.
 *
 * <p>{@link #season} has the shape of the industry plugins' {@code IndustryPlugin.season}
 * (IND_CONDITIONS), so one plugin class can answer both (choice X2-7). The habitat takes no live
 * temperature: a cell is sampled once and kept until its ground changes (X2-3), so a reading of the
 * moment (Project Atmosphere's weather, through mic-climate) would stand for the cell's climate for
 * good (docs/overnight-choices-equilibrium.md, EQC520). {@code IndustryPlugin.temperature} stays the
 * industry's live reading.
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

    /**
     * The year-round climatological temperature at a position in degrees Celsius, or NaN when this source
     * knows none: the place's mean with no weather, no time of day, no rain and no season (the engine adds
     * its own season offsets to it, rtsim_space's {@code WorldModel::season_offset}, so a season in it would
     * be counted twice). Read once per habitat cell, at its centre.
     */
    default float yearRoundCelsius(ServerLevel level, BlockPos pos) {
        return Float.NaN;
    }
}
