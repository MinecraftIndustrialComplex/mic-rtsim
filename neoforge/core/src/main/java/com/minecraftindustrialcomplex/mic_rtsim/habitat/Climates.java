package com.minecraftindustrialcomplex.mic_rtsim.habitat;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.ClimateSource;
import net.minecraft.core.BlockPos;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;

import java.util.List;
import java.util.concurrent.CopyOnWriteArrayList;

/**
 * The registered {@link ClimateSource}s and the vanilla fallback: no seasons, no degrees (the habitat carries
 * vanilla's unitless biome temperature, and the engine reads it on mic-climate's scale, choice X2-8).
 */
public final class Climates {
    private Climates() {}

    private static final List<ClimateSource> SOURCES = new CopyOnWriteArrayList<>();

    public static void register(ClimateSource source) {
        SOURCES.add(source);
    }

    /** Removes a source (GameTests that register their own). */
    public static void unregister(ClimateSource source) {
        SOURCES.remove(source);
    }

    /** {@code {season, seasonCount, seasonLength, seasonOrigin}} from the first source that knows one, else null. */
    public static long[] season(MinecraftServer server) {
        for (ClimateSource s : SOURCES) {
            try {
                long[] c = s.season(server);
                if (c != null && c.length >= 2 && c[1] > 0)
                    return c;
            } catch (RuntimeException | LinkageError e) {
                MicRtsim.LOGGER.error("[mic_rtsim] climate source {} failed on season", s.name(), e);
            }
        }
        return null;
    }

    /** Degrees Celsius at a position from the first source that knows them, else NaN. */
    public static float temperature(ServerLevel level, BlockPos pos) {
        for (ClimateSource s : SOURCES) {
            try {
                float t = s.temperature(level, pos);
                if (Float.isFinite(t))
                    return t;
            } catch (RuntimeException | LinkageError e) {
                MicRtsim.LOGGER.error("[mic_rtsim] climate source {} failed on temperature", s.name(), e);
            }
        }
        return Float.NaN;
    }
}
