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
    private static final ModConfigSpec.BooleanValue PLACE_NESTS;
    private static final ModConfigSpec.IntValue SAMPLES_PER_TICK;
    private static final ModConfigSpec.IntValue HOME_LEASH;
    private static final ModConfigSpec.IntValue LAIR_FOOTPRINT;
    private static final ModConfigSpec.IntValue CENSUS_PERIOD;
    private static final ModConfigSpec.IntValue CENSUS_GAP;
    private static final ModConfigSpec.IntValue LEASH_SCAN;
    private static final ModConfigSpec.IntValue WATCH_BLOCKS;

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
        PLACE_NESTS = b.comment("The engine founds an oil nest under its oil chunks (OILGATE.md §4.6, OG-9e): a chamber of sand",
                        "20 to 60 blocks down with a spawner heart that never spawns, carved on the chunk's first load, and",
                        "Death Worms the engine renews there while a pumpjack drills nearby. Off: no nest is placed.")
                .define("placeNests", true);
        SAMPLES_PER_TICK = b.comment("Chunks whose oil is sampled per tick, new chunks and the engine's surveys together",
                        "(PLACEHOLDER). One sample is one call to Create: Diesel Generators' getBaseOilAmount.")
                .defineInRange("samplesPerTick", DEFAULT_SAMPLES_PER_TICK, 1, 4096);
        b.pop();
        b.comment("Dragon home bodies (EQUILIBRIUM.md S5, EQ-2 (a)): a dragon roosting at its lair is a vanilla body that",
                "vanilla saves; away from it, a puppet.").push("home");
        HOME_LEASH = b.comment("A home body this many blocks (horizontally) from its lair, or in another dimension, becomes a",
                        "puppet where it stands: it stays the lair's dragon and costs the lair nothing (EQC109).")
                .defineInRange("homeLeash", 48, 16, 512);
        LAIR_FOOTPRINT = b.comment("A lair is in while every chunk within this many blocks of its home is loaded and ticking",
                        "(a disc, at most 7 by 7 chunks at 48): its census counts the dragons standing there (EQC128).")
                .defineInRange("lairFootprint", 48, 16, 64);
        CENSUS_PERIOD = b.comment("Game ticks between a lair's recounts while it stays in (a game day).")
                .defineInRange("censusPeriod", 24000, 1200, 240000);
        CENSUS_GAP = b.comment("Ticks a lair that went out waits before it can come in again (no record storm at a boundary).")
                .defineInRange("censusGap", 200, 0, 24000);
        LEASH_SCAN = b.comment("Ticks between looks at each loaded home body's distance from its lair.")
                .defineInRange("leashScan", 20, 1, 200);
        WATCH_BLOCKS = b.comment("A lair is watched when a player is within the larger of this and the distance their client",
                        "tracks an entity at (view distance times 16): a body in view is never removed (EQC115, EQC128).")
                .defineInRange("watchBlocks", 128, 16, 1024);
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

    public static boolean placeNests() {
        if (Test.placeNests != null)
            return Test.placeNests;
        return loaded() ? PLACE_NESTS.get() : true;
    }

    public static int samplesPerTick() {
        if (Test.samplesPerTick != null)
            return Test.samplesPerTick;
        return loaded() ? SAMPLES_PER_TICK.get() : DEFAULT_SAMPLES_PER_TICK;
    }

    public static int homeLeash() {
        return Test.homeLeash != null ? Test.homeLeash : loaded() ? HOME_LEASH.get() : 48;
    }

    public static int lairFootprint() {
        return loaded() ? LAIR_FOOTPRINT.get() : 48;
    }

    public static int censusPeriod() {
        return Test.censusPeriod != null ? Test.censusPeriod : loaded() ? CENSUS_PERIOD.get() : 24000;
    }

    public static int censusGap() {
        return loaded() ? CENSUS_GAP.get() : 200;
    }

    public static int leashScan() {
        return Test.leashScan != null ? Test.leashScan : loaded() ? LEASH_SCAN.get() : 20;
    }

    public static int watchBlocks() {
        return loaded() ? WATCH_BLOCKS.get() : 128;
    }

    /** Lairs are followed in and out by the chunk watch; the GameTests drive them by hand ({@link HomeBodies#testIn}). */
    public static boolean watchLairs() {
        return Test.watchLairs == null || Test.watchLairs;
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
                | (placeLairs() ? DragonRecords.OilConfig.FLAG_PLACE_LAIRS : 0)
                | (placeNests() ? DragonRecords.OilConfig.FLAG_PLACE_NESTS : 0);
    }

    /**
     * GameTest overrides ({@code -Dmic_rtsim.gametest=true}). The GameTest world is reused across runs and
     * its tests share one engine, so the oil gate's side effects start off there: no lair or nest placed, no chunk
     * sampled, no survey answered, no field flight, until a test switches one on and back off.
     */
    public static final class Test {
        private Test() {}

        public static final boolean ENABLED = Boolean.getBoolean("mic_rtsim.gametest");
        public static Boolean fieldFlights = ENABLED ? Boolean.FALSE : null;
        public static Boolean placeLairs = ENABLED ? Boolean.FALSE : null;
        public static Boolean placeNests = ENABLED ? Boolean.FALSE : null;
        /**
         * A nest chamber's floor at this y, whatever PLACE_NEST's depth (the flat GameTest world has no ground 20
         * blocks deep): the first spot tried, the seeded alternatives above and below it as usual. Null: the depth.
         */
        public static Integer nestFloorY = null;
        /** Ticks before a nest whose worms could not be placed tries again (null: one nest day, 24,000). */
        public static Integer nestRetryTicks = null;
        public static Boolean sampleNewChunks = ENABLED ? Boolean.FALSE : null;
        public static Boolean answerSurveys = ENABLED ? Boolean.FALSE : null;
        public static Integer samplesPerTick = null;
        public static Boolean breathPerBurst = null;
        /** Lairs followed by the chunk watch (GameTests drive them by hand, off). */
        public static Boolean watchLairs = ENABLED ? Boolean.FALSE : null;
        public static Integer homeLeash = null;
        public static Integer censusPeriod = null;
        public static Integer leashScan = null;

        /** Back to the GameTest defaults. */
        public static void reset() {
            if (!ENABLED)
                throw new IllegalStateException("config overrides need -Dmic_rtsim.gametest=true");
            fieldFlights = Boolean.FALSE;
            placeLairs = Boolean.FALSE;
            placeNests = Boolean.FALSE;
            nestFloorY = null;
            nestRetryTicks = null;
            sampleNewChunks = Boolean.FALSE;
            answerSurveys = Boolean.FALSE;
            samplesPerTick = null;
            breathPerBurst = null;
            watchLairs = Boolean.FALSE;
            homeLeash = null;
            censusPeriod = null;
            leashScan = null;
        }
    }
}
