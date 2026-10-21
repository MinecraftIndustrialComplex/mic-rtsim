package com.minecraftindustrialcomplex.mic_rtsim.industry.glass;

import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryConfig;
import net.neoforged.neoforge.common.ModConfigSpec;

import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/**
 * The glass box's server config (GLASSBOX.md §2.10), the {@code [industry.glass]} section of
 * {@code mic_rtsim-industry-server.toml}, next to the sampler's keys. Every default is a PLACEHOLDER: none
 * is measured. {@link #define} is called from {@link IndustryConfig}'s spec builder.
 */
public final class GlassConfig {
    private GlassConfig() {}

    /** Server-thread time per tick for extraction, across all districts (GLASSBOX.md §2.10). */
    public static final String COMPILE_SLICE_MICROS = "compileSliceMicros";
    /** Ticks between resends of a district's last compile (GB4). */
    public static final String RESEND_TICKS = "resendTicks";
    /** The checksum window END carries (GB7); 0 is the engine's own {@code glass.checksumWindowMin}. */
    public static final String CHECKSUM_WINDOW_TICKS = "checksumWindowTicks";
    /** Bytes of graph records written per tick, across all districts (a compile spans ticks). */
    public static final String SEND_BYTES_PER_TICK = "sendBytesPerTick";
    /** Block entities hashed per tick for the fingerprints, across all districts (G-B3). */
    public static final String FINGERPRINT_PER_TICK = "fingerprintBlockEntitiesPerTick";
    /** Ticks between fingerprint passes of a loaded district (the sweep period by default, 0). */
    public static final String FINGERPRINT_PERIOD = "fingerprintPeriodTicks";
    /** Ticks between checks of each loaded district chunk's block-ticking state (IND_CHUNK_TICKING). */
    public static final String TICKING_CHECK_TICKS = "tickingCheckTicks";
    /** More nodes than this and a district is not compilable (reason TOO_BIG). */
    public static final String MAX_NODES = "maxNodes";
    /** The master switch: 0 compiles nothing (the C19 box alone). */
    public static final String ENABLED = "enabled";

    private static final Map<String, Integer> DEFAULTS = Map.of(
            COMPILE_SLICE_MICROS, 250,
            RESEND_TICKS, 1200,
            CHECKSUM_WINDOW_TICKS, 0,
            SEND_BYTES_PER_TICK, 16 * 1024,
            FINGERPRINT_PER_TICK, 256,
            FINGERPRINT_PERIOD, 0,
            TICKING_CHECK_TICKS, 20,
            MAX_NODES, 20_000,
            ENABLED, 1);

    private static final Map<String, ModConfigSpec.IntValue> INTS = new HashMap<>();

    /** Adds the {@code glass} section to the industry spec (called once, from its static builder). */
    public static void define(ModConfigSpec.Builder b) {
        b.comment("The glass box (GLASSBOX.md, C28, G1 read only): the compiled graph of each district. PLACEHOLDER defaults.").push("glass");
        INTS.put(ENABLED, b.comment("1 compiles districts into graphs for the engine's checksum (G1); 0 sends nothing.")
                .defineInRange(ENABLED, DEFAULTS.get(ENABLED), 0, 1));
        INTS.put(COMPILE_SLICE_MICROS, b.comment("Server-thread microseconds per tick for extraction, across all districts (inside the bridge budget).")
                .defineInRange(COMPILE_SLICE_MICROS, DEFAULTS.get(COMPILE_SLICE_MICROS), 10, 100_000));
        INTS.put(RESEND_TICKS, b.comment("Ticks between resends of a district's last compile (the engine drops a resend it already holds).")
                .defineInRange(RESEND_TICKS, DEFAULTS.get(RESEND_TICKS), 20, 1_728_000));
        INTS.put(CHECKSUM_WINDOW_TICKS, b.comment("The checksum window sent with each compile, in ticks; 0 uses the engine's glass.checksumWindowMin.")
                .defineInRange(CHECKSUM_WINDOW_TICKS, DEFAULTS.get(CHECKSUM_WINDOW_TICKS), 0, 1_728_000));
        INTS.put(SEND_BYTES_PER_TICK, b.comment("Bytes of graph records written per tick, across all districts.")
                .defineInRange(SEND_BYTES_PER_TICK, DEFAULTS.get(SEND_BYTES_PER_TICK), 1024, 1 << 20));
        INTS.put(FINGERPRINT_PER_TICK, b.comment("Block entities hashed per tick for the districts' fingerprints (G-B3).")
                .defineInRange(FINGERPRINT_PER_TICK, DEFAULTS.get(FINGERPRINT_PER_TICK), 1, 1_000_000));
        INTS.put(FINGERPRINT_PERIOD, b.comment("Ticks between fingerprint passes of a loaded district; 0 uses the sweep period.")
                .defineInRange(FINGERPRINT_PERIOD, DEFAULTS.get(FINGERPRINT_PERIOD), 0, 1_728_000));
        INTS.put(TICKING_CHECK_TICKS, b.comment("Ticks between checks of each district chunk's block-ticking state (IND_CHUNK_TICKING).")
                .defineInRange(TICKING_CHECK_TICKS, DEFAULTS.get(TICKING_CHECK_TICKS), 1, 1200));
        INTS.put(MAX_NODES, b.comment("A district with more graph nodes than this is not compilable.")
                .defineInRange(MAX_NODES, DEFAULTS.get(MAX_NODES), 16, 1_000_000));
        b.pop();
    }

    public static int get(String key) {
        Integer o = Test.OVERRIDES.get(key);
        if (o != null && RtsimConfig.Test.ENABLED)
            return o;
        ModConfigSpec.IntValue v = INTS.get(key);
        return v != null && IndustryConfig.SPEC.isLoaded() ? v.get() : DEFAULTS.get(key);
    }

    public static boolean enabled() {
        return get(ENABLED) != 0;
    }

    /** GameTest-only overrides, unlocked by {@code -Dmic_rtsim.gametest=true} as the industry's are. */
    public static final class Test {
        private Test() {}

        static final Map<String, Integer> OVERRIDES = new ConcurrentHashMap<>();

        public static void set(String key, Integer value) {
            if (!RtsimConfig.Test.ENABLED)
                throw new IllegalStateException("glass config overrides are for GameTests only");
            if (value == null)
                OVERRIDES.remove(key);
            else
                OVERRIDES.put(key, value);
        }
    }
}
