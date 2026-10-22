package com.minecraftindustrialcomplex.mic_rtsim.far;

import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/**
 * The client-side registry of far objects (research/distant-horizons-2026-10-21.md §6.1): what moves beyond
 * the vanilla view, written by sources and read by a renderer, with neither knowing the other's mod.
 *
 * <ul>
 *   <li><b>Sources</b> (a Create plugin's train reader, later the far feed's decoder) replace their whole
 *       set with {@link #commit} whenever they have news. Each source owns its own keys.</li>
 *   <li><b>Renderers</b> (mic_rtsim_dh) read {@link #movers()}, a snapshot that never changes under them,
 *       and say how far they draw with {@link #want}. A source does no work, and asks its mod for nothing,
 *       while {@link #wantedRadius()} is 0.</li>
 * </ul>
 *
 * <p>It knows no Minecraft class, so it is plain Java (and unit tested as such). The client thread writes it
 * and the render thread reads it; on 1.21.1 those are one thread, but the snapshot is published through a
 * volatile so a renderer on another thread would still see whole commits.
 */
public final class FarObjects {
    private static final Map<String, List<FarMover>> BY_SOURCE = new ConcurrentHashMap<>();
    private static final Map<String, Double> DEMAND = new ConcurrentHashMap<>();
    private static volatile List<FarMover> snapshot = List.of();
    private static volatile long version;

    private FarObjects() {}

    // ---------------------------------------------------------------------------------------------------------
    // Sources

    /** Replaces everything {@code source} has in the registry with {@code movers}. */
    public static synchronized void commit(String source, Collection<FarMover> movers) {
        for (FarMover m : movers)
            if (!m.key().source().equals(source))
                throw new IllegalArgumentException("source " + source + " committed a mover of " + m.key().source());
        if (movers.isEmpty()) {
            if (BY_SOURCE.remove(source) == null)
                return;
        } else {
            BY_SOURCE.put(source, List.copyOf(movers));
        }
        publish();
    }

    /** Removes everything {@code source} has in the registry. */
    public static void clear(String source) {
        commit(source, List.of());
    }

    /** Removes every source's movers (the client left its world). Demand stays: it belongs to renderers. */
    public static synchronized void clearAll() {
        if (BY_SOURCE.isEmpty())
            return;
        BY_SOURCE.clear();
        publish();
    }

    private static void publish() {
        List<FarMover> all = new ArrayList<>();
        for (List<FarMover> l : BY_SOURCE.values())
            all.addAll(l);
        snapshot = List.copyOf(all);
        version++;
    }

    // ---------------------------------------------------------------------------------------------------------
    // Renderers

    /** Every source's movers, as of the last commit. The list never changes; a commit makes a new one. */
    public static List<FarMover> movers() {
        return snapshot;
    }

    /** Goes up by one on every commit that changed the registry, so a renderer can skip unchanged frames. */
    public static long version() {
        return version;
    }

    /**
     * Says that {@code consumer} draws far objects out to {@code radiusBlocks} horizontally from the player;
     * 0 or less withdraws it.
     */
    public static void want(String consumer, double radiusBlocks) {
        if (radiusBlocks > 0.0)
            DEMAND.put(consumer, radiusBlocks);
        else
            DEMAND.remove(consumer);
    }

    /** The largest radius any renderer wants, in blocks; 0 when none wants anything. */
    public static double wantedRadius() {
        double r = 0.0;
        for (double d : DEMAND.values())
            r = Math.max(r, d);
        return r;
    }
}
