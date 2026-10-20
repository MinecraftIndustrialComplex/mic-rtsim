package com.minecraftindustrialcomplex.mic_rtsim_climate;

import com.minecraftindustrialcomplex.mic_climate.Climate;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryPlugin;
import net.minecraft.core.BlockPos;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.neoforged.fml.ModList;
import sereneseasons.api.season.ISeasonState;
import sereneseasons.api.season.SeasonHelper;

/**
 * Seasons and temperature (I12, F28) from mic-climate's sources.
 *
 * <ul>
 *   <li><b>Season:</b> Serene Seasons' cycle, the one mic-climate's {@code YearClock} reads (its cycle is
 *       the year): four seasons (spring, summer, autumn, winter: {@code Season.ordinal()}), each
 *       {@code getSeasonDuration()} ticks, season 0 having begun {@code getSeasonCycleTicks()} ago in the
 *       overworld. Without Serene Seasons, none (vanilla: the core's default).</li>
 *   <li><b>Temperature:</b> {@code Climate.celsius(level, pos)}, mic-climate's unified ambient
 *       temperature in degrees Celsius (Deep Time, Project Atmosphere, Thermoo).</li>
 * </ul>
 */
public final class ClimatePlugin implements IndustryPlugin {

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
