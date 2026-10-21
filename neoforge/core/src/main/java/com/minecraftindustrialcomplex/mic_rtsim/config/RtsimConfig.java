package com.minecraftindustrialcomplex.mic_rtsim.config;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.config.ModConfig;
import net.neoforged.neoforge.common.ModConfigSpec;

import java.util.Arrays;
import java.util.EnumMap;
import java.util.Map;

/**
 * The server config ({@code serverconfig/mic_rtsim-server.toml} in each world).
 *
 * <p>One switch per generic module (C25, PLAN.md §5.4). Switching off trade or travel gives wandering
 * traders back to vanilla ({@code bridge.TraderSpawning}). The quotas and intervals are PLACEHOLDER
 * values until phase 0 measures them (PLAN.md §4.1, P24); none of them is a measured number. The
 * engine switch picks the Rust engine (the default) or the pure-Java stub, which simulates nothing.
 */
public final class RtsimConfig {
    private RtsimConfig() {}

    public static final ModConfigSpec SPEC;
    private static final Map<Module, ModConfigSpec.BooleanValue> MODULES = new EnumMap<>(Module.class);
    private static final ModConfigSpec.IntValue SPAWN_QUOTA;
    private static final ModConfigSpec.IntValue PASS_INTERVAL;
    private static final ModConfigSpec.IntValue SHADOW_INTERVAL;
    private static final ModConfigSpec.IntValue PLAYER_INTERVAL;
    private static final ModConfigSpec.ConfigValue<String> ENGINE;

    /** {@code engine.implementation}: the Rust engine through mic_native. */
    public static final String ENGINE_NATIVE = "native";
    /** {@code engine.implementation}: the pure-Java stub, which simulates nothing (tests). */
    public static final String ENGINE_STUB = "stub";

    /** PLACEHOLDER defaults (not measured). */
    public static final int DEFAULT_SPAWN_QUOTA = 8;
    public static final int DEFAULT_PASS_INTERVAL = 1;
    public static final int DEFAULT_SHADOW_INTERVAL = 20;
    /** PLAN.md §6.3: players are observed every 20 ticks. */
    public static final int DEFAULT_PLAYER_INTERVAL = 20;

    static {
        ModConfigSpec.Builder b = new ModConfigSpec.Builder();
        b.comment("One switch per engine module (C25). A module that is off registers no systems and takes no",
                        "observations; its saved pages stay on disk untouched. While trade and travel are both on, rtsim",
                        "cancels vanilla's wandering trader spawner (the doTraderSpawning gamerule is never written);",
                        "switching either off gives wandering traders back to vanilla.")
                .push("modules");
        for (Module m : Module.values())
            MODULES.put(m, b.define(m.key, true));
        b.pop();

        b.comment("PLACEHOLDER values, to be set from phase 0 measurements (PLAN.md §4.1). None is measured.")
                .push("budgets");
        SPAWN_QUOTA = b.comment("Puppet bodies spawned or despawned on command per tick (PLACEHOLDER). A group is never",
                        "split across ticks: when the rest of the quota is too small, the whole group waits.")
                .defineInRange("spawnQuotaPerTick", DEFAULT_SPAWN_QUOTA, 1, 1024);
        PASS_INTERVAL = b.comment("Ticks between full puppet passes: ticking test, taking checks, despawn-timer refresh",
                        "(PLACEHOLDER). A group a player interacted with is checked on the next tick regardless.")
                .defineInRange("puppetPassInterval", DEFAULT_PASS_INTERVAL, 1, 200);
        SHADOW_INTERVAL = b.comment("Ticks between PUPPET_STATE observations of each embodied member (PLACEHOLDER).")
                .defineInRange("puppetStateInterval", DEFAULT_SHADOW_INTERVAL, 1, 1200);
        PLAYER_INTERVAL = b.comment("Ticks between PLAYER observations (PLAN.md §6.3 says every 20 ticks).")
                .defineInRange("playerInterval", DEFAULT_PLAYER_INTERVAL, 1, 1200);
        b.pop();

        b.push("engine");
        ENGINE = b.comment("Which engine runs the sim, read at server start:",
                        "  native: the Rust engine (rtsim-jni), loaded through mic_native. If it cannot be loaded, the",
                        "          bridge does not start (no fallback) and /rtsim perf says why.",
                        "  stub:   a pure-Java stand-in that speaks the boundary protocol but simulates nothing.",
                        "          It exists for tests; a world played on it has no simulated world.")
                // Arrays.asList, not List.of: the spec tests a missing value with contains(null).
                .defineInList("implementation", ENGINE_NATIVE, Arrays.asList(ENGINE_NATIVE, ENGINE_STUB));
        b.pop();
        SPEC = b.build();
    }

    public static void register(ModContainer container) {
        container.registerConfig(ModConfig.Type.SERVER, SPEC);
    }

    /** True when the config file is loaded (server configs load at server start). */
    public static boolean loaded() {
        return SPEC.isLoaded();
    }

    /** The switch as configured (or overridden by a test), before dependency checks. */
    public static boolean configured(Module m) {
        Boolean override = Test.OVERRIDES.get(m);
        if (override != null)
            return override;
        return !loaded() || MODULES.get(m).get();
    }

    /**
     * The modules that actually run: configured on, with every module they require also on. A module
     * whose requirement is off is treated as off, and the log names what is missing.
     */
    public static int effectiveMask() {
        int mask = 0;
        for (Module m : Module.values()) {
            if (!configured(m))
                continue;
            boolean ok = true;
            for (Module need : m.requires()) {
                if (!configured(need)) {
                    MicRtsim.LOGGER.error("[mic_rtsim] module {} is on but needs {}, which is off; {} stays off",
                            m.key, need.key, m.key);
                    ok = false;
                }
            }
            if (ok)
                mask |= m.bit();
        }
        return mask;
    }

    public static boolean on(Module m) {
        return (effectiveMask() & m.bit()) != 0;
    }

    public static int spawnQuota() {
        return loaded() ? SPAWN_QUOTA.get() : DEFAULT_SPAWN_QUOTA;
    }

    public static int passInterval() {
        return loaded() ? PASS_INTERVAL.get() : DEFAULT_PASS_INTERVAL;
    }

    public static int shadowInterval() {
        return loaded() ? SHADOW_INTERVAL.get() : DEFAULT_SHADOW_INTERVAL;
    }

    public static int playerInterval() {
        return loaded() ? PLAYER_INTERVAL.get() : DEFAULT_PLAYER_INTERVAL;
    }

    /**
     * The engine to open at server start: {@link #ENGINE_NATIVE} or {@link #ENGINE_STUB}. A GameTest
     * server may override it with {@code -Dmic_rtsim.engine} (only with {@code -Dmic_rtsim.gametest=true}).
     */
    public static String engine() {
        if (Test.ENABLED) {
            String override = System.getProperty("mic_rtsim.engine");
            if (override != null && !override.isBlank()) {
                String e = override.trim();
                if (!e.equals(ENGINE_NATIVE) && !e.equals(ENGINE_STUB))
                    throw new IllegalStateException("-Dmic_rtsim.engine=" + override + ": expected " + ENGINE_NATIVE + " or " + ENGINE_STUB);
                return e;
            }
        }
        return loaded() ? ENGINE.get() : ENGINE_NATIVE;
    }

    /**
     * GameTest-only overrides. {@code -Dmic_rtsim.gametest=true} (set by the runGameTestServer run and
     * nothing else) unlocks them, so a normal game cannot move a switch behind the config file's back.
     * It also unlocks {@code -Dmic_rtsim.engine} ({@link #engine()}).
     */
    public static final class Test {
        private Test() {}

        public static final boolean ENABLED = Boolean.getBoolean("mic_rtsim.gametest");
        private static final Map<Module, Boolean> OVERRIDES = new EnumMap<>(Module.class);

        /** Overrides one module switch; null removes the override. */
        public static void setModule(Module m, Boolean on) {
            if (!ENABLED)
                throw new IllegalStateException("config overrides need -Dmic_rtsim.gametest=true");
            if (on == null)
                OVERRIDES.remove(m);
            else
                OVERRIDES.put(m, on);
        }
    }
}
