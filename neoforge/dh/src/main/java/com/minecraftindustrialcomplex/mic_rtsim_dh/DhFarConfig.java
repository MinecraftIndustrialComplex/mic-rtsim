package com.minecraftindustrialcomplex.mic_rtsim_dh;

import net.neoforged.fml.ModContainer;
import net.neoforged.fml.config.ModConfig;
import net.neoforged.neoforge.common.ModConfigSpec;

/** mic_rtsim_dh's client config: the far view's detail tier (research/distant-horizons-2026-10-21.md §7). */
public final class DhFarConfig {
    private static final ModConfigSpec SPEC;
    private static final ModConfigSpec.EnumValue<FarDetail> DETAIL;

    static {
        ModConfigSpec.Builder b = new ModConfigSpec.Builder();
        b.push("far_view");
        DETAIL = b.comment(
                "How much of what moves far away Distant Horizons draws. OFF draws nothing and asks the server for",
                "nothing. LOW draws up to 32 carriages at once, MEDIUM 96, HIGH 256 (nearest first).",
                "Nothing is drawn on macOS, or while Distant Horizons' generic rendering is off.")
                .defineEnum("detail", FarDetail.MEDIUM);
        b.pop();
        SPEC = b.build();
    }

    private DhFarConfig() {}

    static void register(ModContainer container) {
        container.registerConfig(ModConfig.Type.CLIENT, SPEC);
    }

    /** The detail tier; MEDIUM until the config file has loaded. The bench's tier, when it sets one, wins. */
    public static FarDetail detail() {
        if (FarBench.DETAIL != null)
            return FarBench.DETAIL;
        return SPEC.isLoaded() ? DETAIL.get() : FarDetail.MEDIUM;
    }
}
