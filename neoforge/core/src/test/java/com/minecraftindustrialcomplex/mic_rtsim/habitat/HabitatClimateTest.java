package com.minecraftindustrialcomplex.mic_rtsim.habitat;

import com.minecraftindustrialcomplex.mic_rtsim.api.ClimateSource;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryPlugin;
import net.minecraft.core.BlockPos;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The habitat sampler's climate (docs/boundary.md §10.3; docs/overnight-choices-equilibrium.md, "Habitat
 * climate"): a cell's degrees are its year-round climate, never the weather of the moment it was sampled.
 * The sampler is driven with a stub climate plugin in the pack's shape: {@code mic_rtsim_climate}'s
 * {@code ClimatePlugin} is an {@link IndustryPlugin} and a {@link ClimateSource} in one class, and its
 * {@code temperature} is mic-climate's live reading, which with Project Atmosphere is the weather of a
 * 2,000-block region (its day and night swing and rain cooling in it). No level is needed: the stub reads
 * none, so it is passed as null.
 */
final class HabitatClimateTest {

    /** Project Atmosphere's region through mic-climate, as the stub sees it: one value for every biome in it. */
    static final class WeatherClimate implements IndustryPlugin, ClimateSource {
        boolean night, rain;

        @Override
        public String name() {
            return "a regional weather stub";
        }

        @Override
        public long[] season(MinecraftServer server) {
            return new long[]{0, 4, 24_000L * 8, 0}; // spring of four
        }

        /** 25 °C at noon in clear weather, 3 °C at night in rain, whatever the biome. */
        @Override
        public float temperature(ServerLevel level, BlockPos pos) {
            return night && rain ? 3f : night || rain ? 12f : 25f;
        }
    }

    private final WeatherClimate weather = new WeatherClimate();

    @BeforeEach
    void register() {
        Climates.register(weather);
    }

    @AfterEach
    void unregister() {
        Climates.unregister(weather);
    }

    /** A cell as the sampler reads it: sixteen points of one biome of vanilla temperature {@code t}, at y 64. */
    private static HabitatSampler.Cell cell(float t) {
        HabitatSampler.Cell c = new HabitatSampler.Cell();
        c.temperature = t;
        c.downfall = 0.4f;
        c.min = c.max = c.mean = 64;
        return c;
    }

    @Test
    void aCellSampledAtNightInRainReadsTheClimateOfOneSampledAtNoonInClearWeather() {
        weather.night = true;
        weather.rain = true;
        float wet = HabitatSampler.cellCelsius(null, 10, 20, cell(0.8f));
        weather.night = false;
        weather.rain = false;
        float noon = HabitatSampler.cellCelsius(null, 11, 20, cell(0.8f));
        assertEquals(Float.floatToIntBits(noon), Float.floatToIntBits(wet),
                "two plains cells: the one sampled at night in rain reads " + wet + " °C, the one sampled at noon in "
                        + "clear weather " + noon + " °C");
    }

    @Test
    void twoBiomesOfDifferentBaseTemperatureSampleToDifferentClimates() {
        // Sampled at the same moment, in one Project Atmosphere region: plains (0.8) and desert (2.0).
        float plains = HabitatSampler.cellCelsius(null, 10, 20, cell(0.8f));
        float desert = HabitatSampler.cellCelsius(null, 11, 20, cell(2.0f));
        assertTrue(plains != desert, "plains and desert, sampled at noon in clear weather, both read " + plains + " °C");
        // mic-climate's scale (rtsim_space::model::vanilla_celsius, mic-climate's BiomeSeasonFallback):
        // plains 19.52 °C, desert 56 °C.
        assertEquals(19.52f, plains, 1e-3f, "plains on mic-climate's scale");
        assertEquals(56f, desert, 1e-3f, "desert on mic-climate's scale");
    }
}
