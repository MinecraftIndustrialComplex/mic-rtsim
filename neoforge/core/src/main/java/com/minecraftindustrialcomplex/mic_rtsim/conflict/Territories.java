package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.Level;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Circles on the map by dimension and position (docs/boundary.md §9's TERRITORY): a small spatial hash
 * of {@value #CELL}-block cells, each circle listed in every cell it overlaps, so a lookup reads one cell.
 * Reusable: the conflict bridge keeps its factions' territories here, and M7's train sampler asks the
 * same structure which circles a track passes (MOWZIE.md §9).
 *
 * <p>Distances are flat x, z (MZB-104): a territory across a world seam counts on its own side only.
 * Server thread only.
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

    private record Cell(ResourceKey<Level> dimension, int cx, int cz) {}

    /** A circle over more cells than this is kept apart and tested on every lookup (a bad radius). */
    static final long MAX_CELLS = 1024;

    private final Map<Long, Territory> byFaction = new LinkedHashMap<>();
    private final Map<Cell, List<Territory>> cells = new HashMap<>();
    private final List<Territory> wide = new ArrayList<>();

    /** Adds or replaces a faction's territory. */
    public void put(Territory t) {
        remove(t.faction());
        byFaction.put(t.faction(), t);
        if (cellCount(t) > MAX_CELLS)
            wide.add(t);
        else
            forCells(t, c -> cells.computeIfAbsent(c, k -> new ArrayList<>(2)).add(t));
    }

    /** Removes a faction's territory; the removed one, or null. */
    public Territory remove(long faction) {
        Territory old = byFaction.remove(faction);
        if (old != null && wide.remove(old))
            return old;
        if (old != null)
            forCells(old, c -> {
                List<Territory> l = cells.get(c);
                if (l != null) {
                    l.remove(old);
                    if (l.isEmpty())
                        cells.remove(c);
                }
            });
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
        List<Territory> l = cells.get(new Cell(dimension, Math.floorDiv(x, CELL), Math.floorDiv(z, CELL)));
        if (l == null && wide.isEmpty())
            return List.of();
        List<Territory> out = new ArrayList<>(2);
        if (l != null)
            for (Territory t : l)
                if (t.contains(x, z))
                    out.add(t);
        for (Territory t : wide)
            if (t.dimension().equals(dimension) && t.contains(x, z))
                out.add(t);
        return out;
    }

    /** The first territory holding (x, z) that minds {@code signal}, or null. */
    public Territory minding(ResourceKey<Level> dimension, int x, int z, int signal) {
        List<Territory> l = cells.get(new Cell(dimension, Math.floorDiv(x, CELL), Math.floorDiv(z, CELL)));
        if (l != null)
            for (Territory t : l)
                if (t.minds(signal) && t.contains(x, z))
                    return t;
        for (Territory t : wide)
            if (t.dimension().equals(dimension) && t.minds(signal) && t.contains(x, z))
                return t;
        return null;
    }

    private static long cellCount(Territory t) {
        long r = Math.max(0, t.radius());
        long side = (2 * r) / CELL + 2;
        return side * side;
    }

    private static void forCells(Territory t, java.util.function.Consumer<Cell> f) {
        int r = Math.max(0, t.radius());
        int x0 = Math.floorDiv(t.x() - r, CELL), x1 = Math.floorDiv(t.x() + r, CELL);
        int z0 = Math.floorDiv(t.z() - r, CELL), z1 = Math.floorDiv(t.z() + r, CELL);
        for (int cx = x0; cx <= x1; cx++)
            for (int cz = z0; cz <= z1; cz++)
                f.accept(new Cell(t.dimension(), cx, cz));
    }
}
