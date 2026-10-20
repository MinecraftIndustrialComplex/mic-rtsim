package com.minecraftindustrialcomplex.mic_rtsim.config;

import java.util.EnumSet;
import java.util.Set;

/**
 * The engine's generic modules (PLAN.md §1.2), each with a server config switch (C25, PLAN.md §5.4).
 * The bit sent to the engine in MODULES and SESSION_START is {@link #ordinal()}, so new modules go at
 * the end.
 */
public enum Module {
    KNOWLEDGE("knowledge"),
    SETTLEMENTS("settlements"),
    TRADE("trade"),
    TRAVEL("travel"),
    ECOLOGY("ecology"),
    CONFLICT("conflict"),
    SHIPS("ships"),
    INDUSTRY("industry"),
    COSMOS("cosmos");

    public final String key;

    Module(String key) {
        this.key = key;
    }

    public int bit() {
        return 1 << ordinal();
    }

    /**
     * Modules this one cannot run without. PLAN.md §5.4 says "a module that others need cannot be on
     * without them; the config check names the missing one". Which module needs which is not decided
     * yet, so every set is empty: the check runs, and has nothing to find.
     */
    public Set<Module> requires() {
        return EnumSet.noneOf(Module.class);
    }
}
