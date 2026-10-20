package com.minecraftindustrialcomplex.mic_rtsim.topology;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.BoundarySession;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.Level;

import java.nio.ByteBuffer;
import java.util.HashMap;
import java.util.Map;
import java.util.function.ToIntFunction;

/**
 * Every level's topology for one session (TP18): detected when the session starts, looked at again
 * every {@link #CHECK_TICKS} ticks and whenever a plugin says it changed, and sent to the engine as a
 * TOPOLOGY record (a sticky preamble record, so the first one precedes any journal replay). The
 * bridge's own steering reads it through {@link #of}. Server thread only.
 */
public final class TopologyTracker {

    /** Ticks between looks for a changed topology (PLACEHOLDER: ten seconds). */
    public static final int CHECK_TICKS = 200;

    private final Map<ResourceKey<Level>, LevelTopology> current = new HashMap<>();
    private long lastCheck = Long.MIN_VALUE;
    private boolean dirty = true;
    private long sent;

    /** The level's topology as last sent (flat before the first look). */
    public LevelTopology of(Level level) {
        LevelTopology t = current.get(level.dimension());
        return t != null ? t : FLAT;
    }

    private static final LevelTopology FLAT = LevelTopology.flat();

    /** A plugin said a level's topology changed: look at the next tick. */
    public void changed() {
        dirty = true;
    }

    /** Records sent this session (for {@code /rtsim perf} and the GameTests). */
    public long sent() {
        return sent;
    }

    /** At the end of a tick, before the hand-over: detect and send what changed. */
    public void tick(MinecraftServer server, long now, BoundarySession boundary, ToIntFunction<ResourceKey<Level>> dimId) {
        if (!dirty && lastCheck != Long.MIN_VALUE && now - lastCheck < CHECK_TICKS && !TopologySources.probeOverridden())
            return;
        dirty = false;
        lastCheck = now;
        for (ServerLevel level : server.getAllLevels()) {
            LevelTopology t = TopologySources.detect(level);
            LevelTopology old = current.put(level.dimension(), t);
            if (t.equals(old))
                continue;
            boundary.preamble(TopologyRecords.TOPOLOGY, dimId.applyAsInt(level.dimension()), body(t));
            sent++;
            MicRtsim.LOGGER.info("[mic_rtsim] {}: {} ({})", level.dimension().location(), t, t.source);
        }
    }

    /** The record's bytes after its 8-byte header. */
    public static byte[] body(LevelTopology t) {
        int size = TopologyRecords.size(t);
        ByteBuffer b = ByteBuffer.allocate(size).order(java.nio.ByteOrder.LITTLE_ENDIAN);
        TopologyRecords.write(b, 0, t);
        byte[] out = new byte[size - 8];
        b.get(8, out);
        return out;
    }
}
