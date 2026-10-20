package com.minecraftindustrialcomplex.mic_rtsim.api;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;

import java.util.List;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.function.Consumer;

/** The registered {@link SessionPlugin}s, in registration order. */
public final class SessionPlugins {
    private SessionPlugins() {}

    private static final List<SessionPlugin> PLUGINS = new CopyOnWriteArrayList<>();

    static void register(SessionPlugin plugin) {
        PLUGINS.add(plugin);
    }

    public static List<SessionPlugin> all() {
        return PLUGINS;
    }

    /** The plugin that applies commands of this kind, or null. */
    public static SessionPlugin forCommand(int kind) {
        for (SessionPlugin p : PLUGINS)
            if (p.ownsCommand(kind))
                return p;
        return null;
    }

    /** The plugin whose namespace holds this agent, or null (the core's own registry decides). */
    public static SessionPlugin ownerOf(long agent) {
        for (SessionPlugin p : PLUGINS)
            if (p.ownsAgent(agent))
                return p;
        return null;
    }

    /** Runs {@code call} on every plugin; an exception is logged and the others still run. */
    public static void each(String what, Consumer<SessionPlugin> call) {
        for (SessionPlugin p : PLUGINS) {
            try {
                call.accept(p);
            } catch (RuntimeException | LinkageError e) {
                MicRtsim.LOGGER.error("[mic_rtsim] plugin {} failed in {}", p.name(), what, e);
            }
        }
    }
}
