package com.minecraftindustrialcomplex.mic_rtsim_iceandfire;

import net.neoforged.fml.ModContainer;
import net.neoforged.fml.config.ModConfig;
import net.neoforged.neoforge.common.ModConfigSpec;

/**
 * The plugin's server config ({@code serverconfig/mic_rtsim_iceandfire-server.toml} in each world): the
 * oil gate's switches (OILGATE.md §4.2, §4.4, §4.8), sent to the engine as OIL_CONFIG at session start,
 * and the oil sampler's budget. Field flights stay off until OILGATE.md §4.8's measurement passes; In
 * Control's fire dragon rules are the pack's gate until then.
 */
public final class DragonConfig {
    private DragonConfig() {}

    public static final ModConfigSpec SPEC;
    private static final ModConfigSpec.BooleanValue FIELD_FLIGHTS;
    private static final ModConfigSpec.BooleanValue PLACE_LAIRS;
    private static final ModConfigSpec.IntValue SAMPLES_PER_TICK;

    /** PLACEHOLDER: oil samples (CDG's getBaseOilAmount, one chunk each) per server tick. */
    public static final int DEFAULT_SAMPLES_PER_TICK = 4;

    static {
        ModConfigSpec.Builder b = new ModConfigSpec.Builder();
        b.comment("The oil gate (OILGATE.md): fire dragons drink oil and burn it as fire.").push("oil");
        FIELD_FLIGHTS = b.comment("The engine draws fire dragons to players near oil fields (OILGATE.md §4.2). On: they",
                        "replace In Control's desert fire dragons (DECISIONS.md OGB-6), and a fire dragon In Control spawns",
                        "is discarded as it joins. Off: nothing draws fire dragons to oil, and In Control's are left alone.")
                .define("fieldFlights", true);
        PLACE_LAIRS = b.comment("An oil field with no fire lair in reach gets a fire roost placed near it on the first load",
                        "of a free site (OILGATE.md §4.4, OG-7). At most one per field.")
                .define("placeLairs", true);
        SAMPLES_PER_TICK = b.comment("Chunks whose oil is sampled per tick, new chunks and the engine's surveys together",
                        "(PLACEHOLDER). One sample is one call to Create: Diesel Generators' getBaseOilAmount.")
                .defineInRange("samplesPerTick", DEFAULT_SAMPLES_PER_TICK, 1, 4096);
        b.pop();
        SPEC = b.build();
    }

    public static void register(ModContainer container) {
        container.registerConfig(ModConfig.Type.SERVER, SPEC);
    }

    private static boolean loaded() {
        return SPEC.isLoaded();
    }

    public static boolean fieldFlights() {
        if (Test.fieldFlights != null)
            return Test.fieldFlights;
        return loaded() ? FIELD_FLIGHTS.get() : true;
    }

    public static boolean placeLairs() {
        if (Test.placeLairs != null)
            return Test.placeLairs;
        return loaded() ? PLACE_LAIRS.get() : true;
    }

    public static int samplesPerTick() {
        if (Test.samplesPerTick != null)
            return Test.samplesPerTick;
        return loaded() ? SAMPLES_PER_TICK.get() : DEFAULT_SAMPLES_PER_TICK;
    }

    /** Chunks are sampled as they first generate (OG-2). */
    public static boolean sampleNewChunks() {
        return Test.sampleNewChunks != null ? Test.sampleNewChunks : true;
    }

    /** The engine's OIL_SURVEYs are answered. */
    public static boolean answerSurveys() {
        return Test.answerSurveys != null ? Test.answerSurveys : true;
    }

    /**
     * A breath is a burst, not a fire event: fire events and charges less than {@link DragonBridge#BURST_GAP}
     * ticks after the last one of a paid burst go free (Ice and Fire's stream fires an event every tick, about
     * 40 to a stream). GameTests only (OG-10's arm C): off, a point a fire event.
     */
    public static boolean breathPerBurst() {
        return Test.breathPerBurst != null && Test.breathPerBurst;
    }

    /** OIL_CONFIG's flags from the switches. */
    public static int oilFlags() {
        return (fieldFlights() ? DragonRecords.OilConfig.FLAG_FIELD_FLIGHTS : 0)
                | (placeLairs() ? DragonRecords.OilConfig.FLAG_PLACE_LAIRS : 0);
    }

    /**
     * GameTest overrides ({@code -Dmic_rtsim.gametest=true}). The GameTest world is reused across runs and
     * its tests share one engine, so the oil gate's side effects start off there: no lair placed, no chunk
     * sampled, no survey answered, no field flight, until a test switches one on and back off.
     */
    public static final class Test {
        private Test() {}

        public static final boolean ENABLED = Boolean.getBoolean("mic_rtsim.gametest");
        public static Boolean fieldFlights = ENABLED ? Boolean.FALSE : null;
        public static Boolean placeLairs = ENABLED ? Boolean.FALSE : null;
        public static Boolean sampleNewChunks = ENABLED ? Boolean.FALSE : null;
        public static Boolean answerSurveys = ENABLED ? Boolean.FALSE : null;
        public static Integer samplesPerTick = null;
        public static Boolean breathPerBurst = null;

        /** Back to the GameTest defaults. */
        public static void reset() {
            if (!ENABLED)
                throw new IllegalStateException("config overrides need -Dmic_rtsim.gametest=true");
            fieldFlights = Boolean.FALSE;
            placeLairs = Boolean.FALSE;
            sampleNewChunks = Boolean.FALSE;
            answerSurveys = Boolean.FALSE;
            samplesPerTick = null;
            breathPerBurst = null;
        }
    }
}
