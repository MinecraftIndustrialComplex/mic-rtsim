package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.simibubi.create.Create;
import com.simibubi.create.content.logistics.packagerLink.LogisticsNetwork;
import net.minecraft.core.GlobalPos;
import net.minecraft.world.level.ChunkPos;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.UUID;

/**
 * Gauge networks that span districts (INDUSTRY.md §8.4, I16's first step: warn). A Create 6 logistics
 * network whose links ({@code LogisticsNetwork.totalLinks}: stock links, packager links, tickers and
 * factory gauges) lie in two or more districts waits while any of them is unloaded
 * ({@code getUnloadedLinkCount} is {@code totalLinks - loadedLinks}). Each district on such a network
 * gets a ledger warning naming the others, and the server log a line when the set changes.
 *
 * <p>No mixin counts a simulated district's links as present (choice Q8): Create's summary
 * ({@code LogisticsManager.getSummaryOfNetwork}) only reads loaded links' inventories, so counting them
 * would let gauges order from stock nobody can read, and the packagers that would fulfil the order are in
 * the unloaded district and do not run. Warn only, until fulfilment into offline buffers is built and
 * tested.
 */
public final class GaugeNetworks {
    private GaugeNetworks() {}

    private static final int PERIOD = 200; // ticks between checks (PLACEHOLDER)
    private static final String KEY = "gauge:";
    /** Network id to the districts it spans, as last warned. */
    private static final Map<UUID, Set<Long>> WARNED = new HashMap<>();
    static long checks;

    static void tick(Industry ind, long now) {
        if (now % PERIOD != 0)
            return;
        check(ind);
    }

    /** Checks every network now; returns the networks that span districts, with their districts. */
    public static Map<UUID, Set<Long>> check(Industry ind) {
        checks++;
        Map<UUID, Set<Long>> spans = new HashMap<>();
        for (LogisticsNetwork net : new ArrayList<>(Create.LOGISTICS.logisticsNetworks.values())) {
            Set<Long> districts = new HashSet<>();
            for (GlobalPos gp : new ArrayList<>(net.totalLinks)) {
                District d = ind.districtAt(gp.dimension(), ChunkPos.asLong(gp.pos().getX() >> 4, gp.pos().getZ() >> 4));
                if (d != null)
                    districts.add(d.id);
            }
            if (districts.size() >= 2)
                spans.put(net.id, districts);
        }
        // Clear warnings that no longer hold, then set the current ones.
        for (Map.Entry<UUID, Set<Long>> e : new ArrayList<>(WARNED.entrySet())) {
            Set<Long> now = spans.get(e.getKey());
            for (long d : e.getValue())
                if (now == null || !now.contains(d))
                    ind.ledgers().warn(d, KEY + e.getKey(), null);
            if (now == null) {
                WARNED.remove(e.getKey());
                MicRtsimCreate.LOGGER.info("[mic_rtsim_create] logistics network {} no longer spans districts", e.getKey());
            }
        }
        for (Map.Entry<UUID, Set<Long>> e : spans.entrySet()) {
            Map<Long, String> names = new TreeMap<>();
            for (long d : e.getValue()) {
                District dd = ind.district(d);
                names.put(d, dd == null ? "district " + d : dd.displayName());
            }
            String net = "network " + e.getKey().toString().substring(0, 8);
            for (long d : e.getValue()) {
                List<String> others = new ArrayList<>();
                for (Map.Entry<Long, String> n : names.entrySet())
                    if (n.getKey() != d)
                        others.add(n.getValue());
                ind.ledgers().warn(d, KEY + e.getKey(), net + " spans " + names.get(d) + " and " + String.join(", ", others)
                        + "; its gauges wait while " + String.join(", ", others) + (others.size() == 1 ? " is" : " are") + " unloaded");
            }
            Set<Long> before = WARNED.put(e.getKey(), e.getValue());
            if (!e.getValue().equals(before))
                MicRtsimCreate.LOGGER.info("[mic_rtsim_create] logistics network {} spans districts {}: its gauges wait while any of them is unloaded",
                        e.getKey(), names.values());
        }
        return spans;
    }

    /** Forgets what was warned (GameTests). */
    public static void forget() {
        WARNED.clear();
    }
}
