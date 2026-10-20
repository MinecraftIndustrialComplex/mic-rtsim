package com.minecraftindustrialcomplex.mic_rtsim_powergrid;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.saveddata.SavedData;
import org.patryk3211.powergrid.electricity.GlobalElectricNetworks;
import org.patryk3211.powergrid.electricity.WorldNetworks;
import org.patryk3211.powergrid.electricity.sim.node.OwnedFloatingNode;
import org.patryk3211.powergrid.electricity.sim.special.TransmissionLine;
import org.patryk3211.powergrid.electricity.wire.BlockWireEndpoint;
import org.patryk3211.powergrid.electricity.wire.IWireEndpoint;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;

/**
 * Power Grid transmission lines between districts as power links (I13, INDUSTRY.md §8.6, F21):
 * IND_POWER_LINK(sink, source) says the sink district's loads run on the source district's generators,
 * so while both are simulated the sink runs only while the source does.
 *
 * <p><b>Measured</b> (choice Q10): every {@code sampleTicks}, for every line whose two ends are block
 * terminals in two different districts, both loaded, the power that crosses from one district to the
 * other is summed over the lines between them: for each conductor, the current from district A to B times
 * the voltage at A's end ({@code TransmissionLine.current()}, positive from node 1 to node 2, and the
 * nodes' solved voltages). Over a closed circuit the sum is the power A delivers to B whatever the
 * voltage reference (Tellegen's theorem on the cut between them). Every {@code decideTicks} the window's
 * mean decides: the district that delivers more than {@code thresholdWatts} is the source; a sink with
 * several sources keeps the one that delivers most (the engine's link has one source). Lines that end at a
 * free junction (not a block terminal) are not followed (UNVERIFIED how they chain).
 *
 * <p>A link ends (IND_POWER_LINK with source 0) when a window saw both districts loaded at every sample
 * and no line between them. Links sent are kept in {@code data/mic_rtsim_powergrid_links.dat}, so a restart
 * neither repeats nor forgets them.
 */
public final class PowerLinks {
    private PowerLinks() {}

    // PLACEHOLDERS, overridable by GameTests.
    public static int sampleTicks = 20;
    public static int decideTicks = 200;
    public static double thresholdWatts = 1.0;

    private record Pair(long a, long b) {
        static Pair of(long x, long y) {
            return x < y ? new Pair(x, y) : new Pair(y, x);
        }
    }

    /** Window sums: watts from a to b (a < b), summed over samples, and the samples both were loaded. */
    private static final Map<Pair, double[]> WINDOW = new HashMap<>();
    /** Pairs a line joined in this window, and the samples the window took. */
    private static final Set<Pair> JOINED = new HashSet<>();
    private static int samples;
    /** The last window's mean watts by ordered pair (sink, source) for the GameTests and the log. */
    private static final Map<Long, double[]> LAST = new HashMap<>();
    static long linksSent;

    public static long linksSent() {
        return linksSent;
    }

    /** The last decided coupling of a sink: {source, mean watts}, or null. */
    public static double[] last(long sink) {
        return LAST.get(sink);
    }

    static void tick(Industry ind, long now) {
        if (now % sampleTicks == 0)
            sample(ind);
        if (now % decideTicks == 0)
            decide(ind);
    }

    /** One sample of every line between districts (GameTests call it too). */
    public static void sample(Industry ind) {
        samples++;
        Map<Pair, Double> here = new HashMap<>();
        for (ServerLevel level : ind.server().getAllLevels()) {
            WorldNetworks wn;
            try {
                wn = GlobalElectricNetworks.getWorldNetworks(level);
            } catch (RuntimeException e) {
                continue;
            }
            if (wn == null)
                continue;
            for (TransmissionLine line : new ArrayList<>(wn.transmissionLines.values())) {
                IWireEndpoint e1 = line.getEndpoint1(), e2 = line.getEndpoint2();
                if (!(e1 instanceof BlockWireEndpoint b1) || !(e2 instanceof BlockWireEndpoint b2))
                    continue;
                District d1 = ind.districtAt(level, b1.getPos()), d2 = ind.districtAt(level, b2.getPos());
                if (d1 == null || d2 == null || d1 == d2)
                    continue;
                Pair p = Pair.of(d1.id, d2.id);
                JOINED.add(p);
                if (!level.isLoaded(b1.getPos()) || !level.isLoaded(b2.getPos()))
                    continue;
                OwnedFloatingNode n1 = line.getNode1(), n2 = line.getNode2();
                if (n1 == null || n2 == null)
                    continue;
                double i = line.current(); // node 1 to node 2
                // The conductor's share of what a delivers to b: the current from a to b times the voltage at
                // a's end (each side its own end, so the line's own I^2R loss stays out of both).
                double fromA = d1.id == p.a() ? i * n1.getVoltage() : -i * n2.getVoltage();
                if (Double.isNaN(fromA) || Double.isInfinite(fromA))
                    continue;
                here.merge(p, fromA, Double::sum);
            }
        }
        for (Map.Entry<Pair, Double> e : here.entrySet()) {
            double[] w = WINDOW.computeIfAbsent(e.getKey(), k -> new double[2]);
            w[0] += e.getValue();
            w[1]++;
        }
    }

    /** Every line of the level, described (GameTests and debugging). */
    public static String describe(Industry ind, ServerLevel level) {
        StringBuilder sb = new StringBuilder();
        WorldNetworks wn = GlobalElectricNetworks.getWorldNetworks(level);
        sb.append(wn == null ? "no networks" : wn.transmissionLines.size() + " lines");
        if (wn != null)
            for (TransmissionLine line : new ArrayList<>(wn.transmissionLines.values())) {
                IWireEndpoint e1 = line.getEndpoint1(), e2 = line.getEndpoint2();
                sb.append("; [").append(e1).append(" -> ").append(e2).append(", I ").append(line.current());
                OwnedFloatingNode n1 = line.getNode1(), n2 = line.getNode2();
                sb.append(", V1 ").append(n1 == null ? "none" : n1.getVoltage()).append(", V2 ").append(n2 == null ? "none" : n2.getVoltage());
                if (e1 instanceof BlockWireEndpoint b1 && e2 instanceof BlockWireEndpoint b2) {
                    District d1 = ind.districtAt(level, b1.getPos()), d2 = ind.districtAt(level, b2.getPos());
                    sb.append(", districts ").append(d1 == null ? "none" : d1.id).append("/").append(d2 == null ? "none" : d2.id);
                }
                sb.append("]");
            }
        return sb.toString();
    }

    /** Decides the window: sends IND_POWER_LINK for each sink whose source changed (GameTests call it too). */
    public static void decide(Industry ind) {
        Links saved = Links.get(ind);
        // Each sink's best source this window.
        Map<Long, long[]> best = new HashMap<>(); // sink -> {source}
        Map<Long, Double> bestWatts = new HashMap<>();
        for (Map.Entry<Pair, double[]> e : WINDOW.entrySet()) {
            if (e.getValue()[1] <= 0)
                continue;
            double mean = e.getValue()[0] / e.getValue()[1];
            long source, sink;
            if (mean > thresholdWatts) {
                source = e.getKey().a();
                sink = e.getKey().b();
            } else if (mean < -thresholdWatts) {
                source = e.getKey().b();
                sink = e.getKey().a();
            } else {
                continue;
            }
            double w = Math.abs(mean);
            if (w > bestWatts.getOrDefault(sink, 0.0)) {
                bestWatts.put(sink, w);
                best.put(sink, new long[] {source});
            }
        }
        for (Map.Entry<Long, long[]> e : best.entrySet()) {
            long sink = e.getKey(), source = e.getValue()[0];
            LAST.put(sink, new double[] {source, bestWatts.get(sink)});
            Long had = saved.links.get(sink);
            if (had == null || had != source) {
                send(ind, sink, source);
                saved.links.put(sink, source);
                saved.setDirty();
                MicRtsim.LOGGER.info("[mic_rtsim_powergrid] district {} runs on district {}: {} W measured across their lines", sink, source,
                        String.format("%.1f", bestWatts.get(sink)));
            }
        }
        // Links whose districts were both live all window with no line between them end.
        for (Map.Entry<Long, Long> e : new ArrayList<>(saved.links.entrySet())) {
            long sink = e.getKey(), source = e.getValue();
            if (best.containsKey(sink) || JOINED.contains(Pair.of(sink, source)))
                continue;
            District ds = ind.district(sink), dr = ind.district(source);
            boolean bothLive = ds == null || dr == null
                    || (ds.loadState() == IndRecords.LoadState.LIVE && dr.loadState() == IndRecords.LoadState.LIVE && samples > 0);
            if (bothLive) {
                send(ind, sink, 0);
                saved.links.remove(sink);
                saved.setDirty();
                LAST.remove(sink);
            }
        }
        WINDOW.clear();
        JOINED.clear();
        samples = 0;
    }

    private static void send(Industry ind, long sink, long source) {
        ByteBuffer r = ind.journal(IndRecords.IND_POWER_LINK, IndRecords.PowerLink.SIZE, 0);
        r.putLong(IndRecords.PowerLink.SINK, sink);
        r.putLong(IndRecords.PowerLink.SOURCE, source);
        ind.commit(r);
        linksSent++;
    }

    /** Forgets the window and the links sent (GameTests). */
    public static void forget(Industry ind) {
        WINDOW.clear();
        JOINED.clear();
        LAST.clear();
        samples = 0;
        Links l = Links.get(ind);
        l.links.clear();
        l.setDirty();
    }

    /** The links sent, sink to source ({@code data/mic_rtsim_powergrid_links.dat}). */
    static final class Links extends SavedData {
        final Map<Long, Long> links = new HashMap<>();
        static final Factory<Links> FACTORY = new Factory<>(Links::new, Links::load);

        static Links get(Industry ind) {
            return ind.server().overworld().getDataStorage().computeIfAbsent(FACTORY, "mic_rtsim_powergrid_links");
        }

        @Override
        public CompoundTag save(CompoundTag tag, HolderLookup.Provider registries) {
            long[] flat = new long[links.size() * 2];
            int k = 0;
            for (Map.Entry<Long, Long> e : links.entrySet()) {
                flat[k++] = e.getKey();
                flat[k++] = e.getValue();
            }
            tag.putLongArray("links", flat);
            return tag;
        }

        static Links load(CompoundTag tag, HolderLookup.Provider registries) {
            Links l = new Links();
            long[] flat = tag.getLongArray("links");
            for (int k = 0; k + 1 < flat.length; k += 2)
                l.links.put(flat[k], flat[k + 1]);
            return l;
        }
    }
}
