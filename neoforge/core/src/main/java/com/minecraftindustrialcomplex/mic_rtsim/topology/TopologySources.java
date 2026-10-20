package com.minecraftindustrialcomplex.mic_rtsim.topology;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.TopologySource;
import net.minecraft.server.level.ServerLevel;

import java.util.List;
import java.util.concurrent.CopyOnWriteArrayList;

/** The registered {@link TopologySource}s, in registration order, and the detection order (TP18). */
public final class TopologySources {
    private TopologySources() {}

    private static final List<TopologySource> SOURCES = new CopyOnWriteArrayList<>();
    /** Toroidal World's shape: a reflective probe, replaceable by tests. */
    private static volatile ToroidalWorldProbe toroidal = ToroidalWorldProbe.reflective();

    public static void register(TopologySource source) {
        SOURCES.add(source);
    }

    public static List<TopologySource> all() {
        return SOURCES;
    }

    private static final ToroidalWorldProbe REFLECTIVE = ToroidalWorldProbe.reflective();
    private static volatile boolean overridden;

    /** Tests: the Toroidal World probe the detection uses (a fake, or {@link ToroidalWorldProbe#reflective}). */
    public static void setToroidalProbe(ToroidalWorldProbe probe) {
        toroidal = probe;
        overridden = true;
    }

    /** Tests: the real probe again. */
    public static void resetToroidalProbe() {
        toroidal = REFLECTIVE;
        overridden = false;
    }

    /** A test's fake probe is in place: the tracker looks every tick, so a fake that expires goes at once. */
    public static boolean probeOverridden() {
        return overridden;
    }

    /**
     * A level's topology: the first registered source that describes it, else Toroidal World's shape,
     * else flat inside the world border. Never throws: a failing source is logged and skipped.
     */
    public static LevelTopology detect(ServerLevel level) {
        for (TopologySource s : SOURCES) {
            try {
                LevelTopology t = s.topologyOf(level);
                if (t != null)
                    return t;
            } catch (RuntimeException | LinkageError e) {
                MicRtsim.LOGGER.error("[mic_rtsim] topology source {} failed on {}", s.name(), level.dimension().location(), e);
            }
        }
        LevelTopology tw = toroidal.topologyOf(level);
        if (tw != null)
            return tw;
        var b = level.getWorldBorder();
        return LevelTopology.flatWithBorder((long) Math.floor(b.getMinX()), (long) Math.floor(b.getMinZ()),
                (long) Math.ceil(b.getMaxX()), (long) Math.ceil(b.getMaxZ()));
    }
}
