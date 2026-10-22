package com.minecraftindustrialcomplex.mic_rtsim.far;

import java.util.ArrayList;
import java.util.Collection;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * The far feed's subscriber filter (research/distant-horizons-2026-10-21.md §6.3; docs/boundary.md §12.3):
 * of what moves in a subscriber's level, the objects inside its radius and of the kinds it draws, dragons
 * first and then nearest first, up to its budget. Plain Java.
 */
public final class FarFilter {
    private FarFilter() {}

    /**
     * Keeps one item per id: the highest priority, and of equal ones the first. Sources that both know an
     * object (the engine's leg and the body in the game) give the same id, and the body wins.
     */
    public static List<FarItem> dedupe(Collection<FarItem> items) {
        Map<Long, FarItem> by = new HashMap<>();
        List<Long> order = new ArrayList<>();
        for (FarItem i : items) {
            FarItem had = by.get(i.id());
            if (had == null) {
                by.put(i.id(), i);
                order.add(i.id());
            } else if (i.priority() > had.priority()) {
                by.put(i.id(), i);
            }
        }
        List<FarItem> out = new ArrayList<>(order.size());
        for (Long id : order)
            out.add(by.get(id));
        return out;
    }

    /**
     * The items a subscriber at (px, pz) gets: within {@code radius} blocks horizontally at {@code gameTime},
     * of a kind in {@code kinds} (a mask of {@link FarKind#bit()}), dragons before carriages and nearer before
     * farther, at most {@code budget}. The items are already in the subscriber's frame.
     */
    public static List<FarItem> select(List<FarItem> items, double px, double pz, double radius, int budget,
            int kinds, long gameTime) {
        record Scored(FarItem item, int rank, double d2) {}
        double r2 = radius * radius;
        List<Scored> in = new ArrayList<>();
        for (FarItem i : items) {
            if ((kinds & i.kind().bit()) == 0)
                continue;
            double[] p = i.position(gameTime);
            double dx = p[0] - px, dz = p[1] - pz;
            double d2 = dx * dx + dz * dz;
            if (!(d2 <= r2))
                continue;
            in.add(new Scored(i, i.kind() == FarKind.DRAGON ? 0 : 1, d2));
        }
        in.sort((a, b) -> a.rank != b.rank ? Integer.compare(a.rank, b.rank) : Double.compare(a.d2, b.d2));
        int n = Math.min(Math.max(0, budget), in.size());
        List<FarItem> out = new ArrayList<>(n);
        for (int k = 0; k < n; k++)
            out.add(in.get(k).item);
        return out;
    }
}
