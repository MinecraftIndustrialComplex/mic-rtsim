package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import it.unimi.dsi.fastutil.longs.Long2ObjectLinkedOpenHashMap;
import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.LongConsumer;

/**
 * Circles on the map by dimension and position (docs/boundary.md §9's TERRITORY): a small spatial hash
 * of {@value #CELL}-block cells, each circle listed in every cell it overlaps, so a lookup reads one cell.
 * Reusable: the conflict bridge keeps its factions' territories here, and M7's train sampler asks the
 * same structure which circles a track passes (MOWZIE.md §9).
 *
 * <p>Distances are flat x, z (MZB-104): a territory across a world seam counts on its own side only.
 * Cells are keyed by packed (cx, cz) per dimension, so {@link #minding} allocates nothing. Server thread
 * only.
 */
public final class Territories {
    /** Cell side, blocks (PLACEHOLDER; a culture's territory is a few hundred blocks). */
    public static final int CELL = 512;

    /** One faction's circle and the signals it minds (bit n = signal n). */
    public record Territory(long faction, ResourceKey<Level> dimension, int mask, int x, int z, int radius) {
        public boolean contains(int px, int pz) {
            long dx = (long) px - x, dz = (long) pz - z;
            return dx * dx + dz * dz <= (long) radius * radius;
        }

        public boolean minds(int signal) {
            return signal >= 0 && signal < 32 && (mask & (1 << signal)) != 0;
        }

        /** Horizontal distance from (px, pz) to the centre. */
        public double distance(double px, double pz) {
            return Math.hypot(px - x, pz - z);
        }
    }

    /** A circle over more cells than this is kept apart and tested on every lookup (a bad radius). */
    static final long MAX_CELLS = 1024;

    private final Long2ObjectLinkedOpenHashMap<Territory> byFaction = new Long2ObjectLinkedOpenHashMap<>();
    private final Map<ResourceKey<Level>, Long2ObjectOpenHashMap<List<Territory>>> cells = new HashMap<>();
    private final List<Territory> wide = new ArrayList<>();

    /** Adds or replaces a faction's territory. */
    public void put(Territory t) {
        remove(t.faction());
        byFaction.put(t.faction(), t);
        if (cellCount(t) > MAX_CELLS)
            wide.add(t);
        else {
            Long2ObjectOpenHashMap<List<Territory>> dim = cells.computeIfAbsent(t.dimension(), k -> new Long2ObjectOpenHashMap<>());
            forCells(t, c -> {
                List<Territory> l = dim.get(c);
                if (l == null)
                    dim.put(c, l = new ArrayList<>(2));
                l.add(t);
            });
        }
    }

    /** Removes a faction's territory; the removed one, or null. */
    public Territory remove(long faction) {
        Territory old = byFaction.remove(faction);
        if (old != null && wide.remove(old))
            return old;
        Long2ObjectOpenHashMap<List<Territory>> dim = old == null ? null : cells.get(old.dimension());
        if (dim != null) {
            forCells(old, c -> {
                List<Territory> l = dim.get(c);
                if (l != null) {
                    l.remove(old);
                    if (l.isEmpty())
                        dim.remove(c);
                }
            });
            if (dim.isEmpty())
                cells.remove(old.dimension());
        }
        return old;
    }

    public void clear() {
        byFaction.clear();
        cells.clear();
        wide.clear();
    }

    public boolean isEmpty() {
        return byFaction.isEmpty();
    }

    public int size() {
        return byFaction.size();
    }

    public Territory get(long faction) {
        return byFaction.get(faction);
    }

    public List<Territory> all() {
        return List.copyOf(byFaction.values());
    }

    /** Every territory in {@code dimension} whose circle holds (x, z), in the order they were put. */
    public List<Territory> containing(ResourceKey<Level> dimension, int x, int z) {
        List<Territory> l = cell(dimension, x, z);
        List<Territory> out = null;
        if (l != null)
            for (Territory t : l)
                if (t.contains(x, z))
                    (out == null ? out = new ArrayList<>(2) : out).add(t);
        for (Territory t : wide)
            if (t.dimension().equals(dimension) && t.contains(x, z))
                (out == null ? out = new ArrayList<>(2) : out).add(t);
        return out == null ? List.of() : out;
    }

    /** The first territory holding (x, z) that minds {@code signal}, or null. */
    public Territory minding(ResourceKey<Level> dimension, int x, int z, int signal) {
        List<Territory> l = cell(dimension, x, z);
        if (l != null)
            for (Territory t : l)
                if (t.minds(signal) && t.contains(x, z))
                    return t;
        for (Territory t : wide)
            if (t.dimension().equals(dimension) && t.minds(signal) && t.contains(x, z))
                return t;
        return null;
    }

    private List<Territory> cell(ResourceKey<Level> dimension, int x, int z) {
        Long2ObjectOpenHashMap<List<Territory>> dim = cells.get(dimension);
        return dim == null ? null : dim.get(ChunkPos.asLong(Math.floorDiv(x, CELL), Math.floorDiv(z, CELL)));
    }

    private static long cellCount(Territory t) {
        long r = Math.max(0, t.radius());
        long side = (2 * r) / CELL + 2;
        return side * side;
    }

    /** Each cell key (packed cx, cz) the circle overlaps. */
    private static void forCells(Territory t, LongConsumer f) {
        int r = Math.max(0, t.radius());
        int x0 = Math.floorDiv(t.x() - r, CELL), x1 = Math.floorDiv(t.x() + r, CELL);
        int z0 = Math.floorDiv(t.z() - r, CELL), z1 = Math.floorDiv(t.z() + r, CELL);
        for (int cx = x0; cx <= x1; cx++)
            for (int cz = z0; cz <= z1; cz++)
                f.accept(ChunkPos.asLong(cx, cz));
    }
}
