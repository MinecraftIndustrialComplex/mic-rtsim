package com.minecraftindustrialcomplex.mic_rtsim_climate;

import com.minecraftindustrialcomplex.mic_climate.Climate;
import com.minecraftindustrialcomplex.mic_rtsim.api.ClimateSource;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryPlugin;
import net.minecraft.core.BlockPos;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.neoforged.fml.ModList;
import sereneseasons.api.season.ISeasonState;
import sereneseasons.api.season.SeasonHelper;

/**
 * Seasons and temperature (I12, F28) from mic-climate's sources, for the industry engine
 * (IND_CONDITIONS), and the season for the habitat sampler (a {@link ClimateSource}, docs/boundary.md §10).
 *
 * <ul>
 *   <li><b>Season:</b> Serene Seasons' cycle, the one mic-climate's {@code YearClock} reads (its cycle is
 *       the year): four seasons (spring, summer, autumn, winter: {@code Season.ordinal()}), each
 *       {@code getSeasonDuration()} ticks, season 0 having begun {@code getSeasonCycleTicks()} ago in the
 *       overworld. Without Serene Seasons, none (vanilla: the core's default).</li>
 *   <li><b>Temperature:</b> {@code Climate.celsius(level, pos)}, mic-climate's unified ambient
 *       temperature in degrees Celsius (Deep Time, Project Atmosphere, Thermoo), for the industry only. It
 *       is the moment's: with Project Atmosphere the live weather of a 2,000-block region, its day and night
 *       swing and rain cooling in it. The habitat sampler keeps a cell's degrees for good, so it takes no
 *       live reading: this plugin answers no {@link ClimateSource#yearRoundCelsius} and the core uses the
 *       biome's vanilla temperature on mic-climate's scale (docs/overnight-choices-equilibrium.md, EQC520).</li>
 * </ul>
 */
public final class ClimatePlugin implements IndustryPlugin, ClimateSource {

    @Override
    public String name() {
        return "mic_climate and Serene Seasons";
    }

    private static final boolean SEASONS = ModList.get().isLoaded("sereneseasons");

    @Override
    public long[] season(MinecraftServer server) {
        if (!SEASONS)
            return null;
        ServerLevel level = server.overworld();
        ISeasonState s = SeasonHelper.getSeasonState(level);
        if (s == null || s.getSeasonDuration() <= 0 || s.getCycleDuration() <= 0)
            return null;
        long cycle = s.getCycleDuration();
        long into = Math.floorMod((long) s.getSeasonCycleTicks(), cycle);
        long origin = level.getGameTime() - into;
        return new long[] {s.getSeason().ordinal(), 4, s.getSeasonDuration(), origin};
    }

    @Override
    public float temperature(ServerLevel level, BlockPos pos) {
        return Climate.celsius(level, pos);
    }
}
