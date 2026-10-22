package com.minecraftindustrialcomplex.mic_rtsim.industry;

import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.config.ModConfig;
import net.neoforged.neoforge.common.ModConfigSpec;

import java.util.LinkedHashMap;
import java.util.Map;

/**
 * The threat sampler's and the conditions' server config ({@code mic_rtsim-threats-server.toml}, its own
 * file so the industry config keeps its keys). Every default is a PLACEHOLDER, none is measured
 * (docs/overnight-choices-indplugins.md).
 */
public final class ThreatConfig {
    private ThreatConfig() {}

    public static final String CONDITIONS_CHECK = "conditionsCheckTicks";
    public static final String SPAWN_RING = "spawnRingChunks";
    public static final String SPAWN_SAMPLE = "spawnSampleTicks";
    public static final String OBSERVE_CHECK = "observeCheckTicks";
    public static final String SURVEY_CHUNKS_PER_TICK = "surveyChunksPerTick";
    public static final String SURVEY_DEPTH = "surveyDepthBlocks";
    public static final String SURVEY_MIN_INTERVAL = "surveyMinIntervalTicks";
    public static final String SURVEY_PERIOD = "surveyPeriodTicks";
    public static final String SURVEY_MAX_BLOCKS = "surveyMaxBlocks";
    public static final String SURVEY_MARGIN = "surveyMarginColumns";
    public static final String SURVEY_MAX_COLUMNS = "surveyMaxColumns";
    public static final String ROOF_REACH = "roofReachBlocks";
    public static final String FIGHT_GAP = "fightGapTicks";

    private static final Map<String, Object[]> DEFS = new LinkedHashMap<>();

    static {
        def(CONDITIONS_CHECK, 100, 1, 72_000, "Ticks between checks of the season calendar (IND_CONDITIONS is sent when it changes).");
        def(SPAWN_RING, 1, 0, 8, "Chunks around a district whose natural spawns and dark blocks count with it (I20).");
        def(SPAWN_SAMPLE, 2400, 20, 1_728_000, "Night ticks observed with a player in a district per IND_SPAWN_SAMPLE.");
        def(OBSERVE_CHECK, 20, 1, 1200, "Ticks between checks of whether a player is in a district at night.");
        def(SURVEY_CHUNKS_PER_TICK, 2, 1, 1024, "Chunks the defence survey reads per tick (I21).");
        def(SURVEY_DEPTH, 16, 1, 384, "Blocks below a column's top surface the survey checks for dark spawnable positions.");
        def(SURVEY_MIN_INTERVAL, 200, 1, 1_728_000, "Ticks at least between two surveys of one district.");
        def(SURVEY_PERIOD, 24_000, 20, 17_280_000, "Ticks between surveys of a loaded district nobody edited.");
        def(SURVEY_MAX_BLOCKS, 2048, 0, 1_000_000, "Exposed blocks a survey lists at most (IND_SURVEY_BLOCKS).");
        def(SURVEY_MARGIN, 8, 1, 256, "Columns around the machines the perimeter cut is computed over.");
        def(SURVEY_MAX_COLUMNS, 40_000, 64, 4_000_000, "Columns at most in the perimeter cut's graph; a larger area is surveyed without it.");
        // OGB-7 (PLACEHOLDER 8): a roof this close above a machine is a block a flyer can break.
        def(ROOF_REACH, 8, 0, 384, "Blocks above a roofed machine within which its roof is a block a flyer can break (OGB-7); deeper cover shields it.");
        def(FIGHT_GAP, 200, 1, 72_000, "Ticks without an engagement that end a real fight (IND_FIGHT).");
    }

    private static void def(String key, int value, int min, int max, String comment) {
        DEFS.put(key, new Object[] {value, min, max, comment});
    }

    public static final ModConfigSpec SPEC;
    private static final Map<String, ModConfigSpec.IntValue> INTS = new LinkedHashMap<>();
    static final Map<String, Integer> OVERRIDES = new java.util.concurrent.ConcurrentHashMap<>();

    static {
        ModConfigSpec.Builder b = new ModConfigSpec.Builder();
        b.comment("mic_rtsim threat sampling and conditions (INDUSTRY.md §13.4, §13.5, I12). PLACEHOLDER defaults, not measured.").push("threats");
        for (Map.Entry<String, Object[]> e : DEFS.entrySet()) {
            Object[] d = e.getValue();
            INTS.put(e.getKey(), b.comment((String) d[3]).defineInRange(e.getKey(), (int) d[0], (int) d[1], (int) d[2]));
        }
        b.pop();
        SPEC = b.build();
    }

    static void register(ModContainer container) {
        container.registerConfig(ModConfig.Type.SERVER, SPEC, "mic_rtsim-threats-server.toml");
    }

    public static int get(String key) {
        Integer o = OVERRIDES.get(key);
        if (o != null && RtsimConfig.Test.ENABLED)
            return o;
        ModConfigSpec.IntValue v = INTS.get(key);
        return SPEC.isLoaded() && v != null ? v.get() : (int) DEFS.get(key)[0];
    }

    /** GameTest-only overrides, unlocked by {@code -Dmic_rtsim.gametest=true}. */
    public static void setForTest(String key, Integer value) {
        if (!RtsimConfig.Test.ENABLED)
            throw new IllegalStateException("threat config overrides are for GameTests only");
        if (value == null)
            OVERRIDES.remove(key);
        else
            OVERRIDES.put(key, value);
    }
}
