package com.minecraftindustrialcomplex.mic_rtsim.industry;

import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.config.ModConfig;
import net.neoforged.neoforge.common.ModConfigSpec;

import java.util.HashMap;
import java.util.Map;

/**
 * The industry module's server config ({@code mic_rtsim-industry-server.toml}). Every default is a
 * PLACEHOLDER until phase 0 measures the bridge budget (INDUSTRY.md §3.2); none is a measured number.
 */
public final class IndustryConfig {
    private IndustryConfig() {}

    public static final ModConfigSpec SPEC;
    private static final Map<String, ModConfigSpec.IntValue> INTS = new HashMap<>();
    private static final ModConfigSpec.BooleanValue APPROACH;

    public static final String SWEEP_PERIOD = "sweepPeriodTicks";
    public static final String INVENTORIES_PER_TICK = "inventoriesPerTick";
    public static final String SLOTS_PER_TICK = "slotsPerTick";
    public static final String RECONCILE_OPS_PER_TICK = "reconcileOpsPerTick";
    public static final String RESCAN_INTERVAL = "rescanIntervalTicks";
    public static final String PERIODIC_RESCAN = "periodicRescanTicks";
    public static final String LOAD_CHECK_INTERVAL = "loadCheckIntervalTicks";
    public static final String WATCHED_RESEND = "watchedResendTicks";
    public static final String APPROACH_RADIUS = "approachRadiusBlocks";

    private static final Map<String, Integer> DEFAULTS = Map.of(
            SWEEP_PERIOD, 100,
            INVENTORIES_PER_TICK, 64,
            SLOTS_PER_TICK, 20_000,
            RECONCILE_OPS_PER_TICK, 512,
            RESCAN_INTERVAL, 20,
            PERIODIC_RESCAN, 6000,
            LOAD_CHECK_INTERVAL, 20,
            WATCHED_RESEND, 100,
            APPROACH_RADIUS, 256);

    static {
        ModConfigSpec.Builder b = new ModConfigSpec.Builder();
        b.comment("mic_rtsim industry (INDUSTRY.md, C15 to C19). PLACEHOLDER defaults, not measured.").push("industry");
        INTS.put(SWEEP_PERIOD, b.comment("Ticks between the starts of a loaded district's stock sweeps (the sample period).")
                .defineInRange(SWEEP_PERIOD, DEFAULTS.get(SWEEP_PERIOD), 1, 72_000));
        INTS.put(INVENTORIES_PER_TICK, b.comment("Boundary inventories read per tick, across all districts (K, INDUSTRY.md §3.2).")
                .defineInRange(INVENTORIES_PER_TICK, DEFAULTS.get(INVENTORIES_PER_TICK), 1, 100_000));
        INTS.put(SLOTS_PER_TICK, b.comment("Item slots read per tick, across all districts (at least one inventory a tick is read whatever its size).",
                        "Measured 2026-10-20 on the laptop: 23 ns per full slot of a 1,620-slot Create vault, so 20,000 slots is about 0.5 ms.")
                .defineInRange(SLOTS_PER_TICK, DEFAULTS.get(SLOTS_PER_TICK), 1, 10_000_000));
        INTS.put(RECONCILE_OPS_PER_TICK, b.comment("Reconcile deltas and broken blocks applied per tick (the block quota, I8, I23).")
                .defineInRange(RECONCILE_OPS_PER_TICK, DEFAULTS.get(RECONCILE_OPS_PER_TICK), 1, 1_000_000));
        INTS.put(RESCAN_INTERVAL, b.comment("Ticks at least between two district detections after edits.")
                .defineInRange(RESCAN_INTERVAL, DEFAULTS.get(RESCAN_INTERVAL), 1, 72_000));
        INTS.put(PERIODIC_RESCAN, b.comment("Ticks between re-detections of loaded districts without edits (inventories added by machines).")
                .defineInRange(PERIODIC_RESCAN, DEFAULTS.get(PERIODIC_RESCAN), 20, 1_728_000));
        INTS.put(LOAD_CHECK_INTERVAL, b.comment("Ticks between load-state and watched checks of a district with loaded chunks.")
                .defineInRange(LOAD_CHECK_INTERVAL, DEFAULTS.get(LOAD_CHECK_INTERVAL), 1, 1200));
        INTS.put(WATCHED_RESEND, b.comment("Ticks between IND_WATCHED repeats while a loaded district's flag holds.")
                .defineInRange(WATCHED_RESEND, DEFAULTS.get(WATCHED_RESEND), 1, 72_000));
        INTS.put(APPROACH_RADIUS, b.comment("Blocks beyond an unloaded district within which a player sends IND_APPROACH.")
                .defineInRange(APPROACH_RADIUS, DEFAULTS.get(APPROACH_RADIUS), 0, 100_000));
        APPROACH = b.comment("Send IND_APPROACH (a player nears an unloaded district): the engine-agnostic hook of the stale-tick race's option (d).")
                .define("approachSignal", true);
        b.pop();
        SPEC = b.build();
    }

    static void register(ModContainer container) {
        container.registerConfig(ModConfig.Type.SERVER, SPEC, "mic_rtsim-industry-server.toml");
    }

    private static boolean loaded() {
        return SPEC.isLoaded();
    }

    public static int get(String key) {
        Integer o = Test.OVERRIDES.get(key);
        if (o != null && RtsimConfig.Test.ENABLED)
            return o;
        ModConfigSpec.IntValue v = INTS.get(key);
        return loaded() && v != null ? v.get() : DEFAULTS.get(key);
    }

    public static boolean approachSignal() {
        return !loaded() || APPROACH.get();
    }

    /** GameTest-only overrides, unlocked by {@code -Dmic_rtsim.gametest=true} as RtsimConfig's are. */
    public static final class Test {
        private Test() {}

        static final Map<String, Integer> OVERRIDES = new java.util.concurrent.ConcurrentHashMap<>();

        public static void set(String key, Integer value) {
            if (!RtsimConfig.Test.ENABLED)
                throw new IllegalStateException("industry config overrides are for GameTests only");
            if (value == null)
                OVERRIDES.remove(key);
            else
                OVERRIDES.put(key, value);
        }
    }
}
