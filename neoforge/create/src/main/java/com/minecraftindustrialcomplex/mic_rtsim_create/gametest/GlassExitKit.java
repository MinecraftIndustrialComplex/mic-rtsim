package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassReport;

import java.io.ByteArrayOutputStream;
import java.util.ArrayList;
import java.util.Collections;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Set;

/**
 * The glass box's report for a GameTest, on either engine (G1's exit test, GLASSBOX.md §11.3).
 *
 * <p>On the native engine the report is the engine's own ({@link NativeEngine#glassReport}). The stub
 * simulates nothing, so a {@link Recorder} keeps every record the stub received from the test's start
 * (the received log holds only the last 10,000, which a long window outruns) and the native code judges
 * them offline on a fresh industry engine ({@link NativeEngine#glassOffline}, choice GB80). With
 * {@code -PskipJni} there are no natives: {@link #canJudge} is false and the test only logs.
 */
public final class GlassExitKit {
    private GlassExitKit() {
    }

    /** Every record the session's engine received since {@link #start}, in arrival order, deduplicated. */
    public static final class Recorder {
        private final List<ScriptedEngine.Received> kept = new ArrayList<>();
        private final Set<ScriptedEngine.Received> seen = Collections.newSetFromMap(new IdentityHashMap<>());

        private Recorder() {
        }

        /** Takes what arrived since the last poll. Poll more often than 10,000 records arrive. */
        public synchronized void poll() {
            for (ScriptedEngine.Received r : IndustryKit.engine().received())
                if (seen.add(r) && !r.skipped())
                    kept.add(r);
        }

        /** The records kept, back to back (each sized by its header), for the offline check. */
        public synchronized byte[] stream() {
            poll();
            return stream(kept.size());
        }

        /**
         * The first {@code upTo} records kept, back to back: the stream as it stood when {@link #size} was
         * {@code upTo} (the G2 exit test's offline report while its far copy was away). No poll.
         */
        public synchronized byte[] stream(int upTo) {
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            for (ScriptedEngine.Received r : kept.subList(0, Math.min(upTo, kept.size())))
                out.writeBytes(r.bytes());
            return out.toByteArray();
        }

        public synchronized int size() {
            return kept.size();
        }

        /** The records kept, oldest first. */
        public synchronized List<ScriptedEngine.Received> records() {
            poll();
            return List.copyOf(kept);
        }
    }

    /** A recorder holding what the engine's log holds now (the session's dictionary among it, if still there). */
    public static Recorder start() {
        Recorder r = new Recorder();
        r.poll();
        return r;
    }

    private static Boolean natives;

    /** Whether this run can judge: the native engine, or the stub with the natives loadable. */
    public static synchronized boolean canJudge() {
        if (IndustryKit.nativeEngine())
            return true;
        if (natives == null) {
            try {
                NativeEngine.ensureLoaded();
                natives = true;
            } catch (IllegalStateException | LinkageError e) {
                IndustryKit.record("glass exit", "natives", "not loadable (" + e.getMessage() + "): the stub run is not judged");
                natives = false;
            }
        }
        return natives;
    }

    /** The district's report now; null while it has no complete compile, or when this run cannot judge. */
    public static GlassReport report(long district, Recorder rec) {
        if (IndustryKit.engine() instanceof NativeEngine n)
            return GlassReport.decode(n.glassReport(district));
        if (!canJudge())
            return null;
        return GlassReport.decode(NativeEngine.glassOffline(rec.stream(), district));
    }

    /**
     * The exit test's judgment of one completed window (GLASSBOX.md §11.3): every exact good within the
     * checksum tolerance and every bounded good inside [L, U] (both are the engine's verdicts, logged
     * with their numbers); unbounded goods are logged, never judged. Returns the failures, empty when
     * the window passes.
     */
    public static List<String> judge(String test, GlassReport r, java.util.function.IntFunction<String> names) {
        List<String> failed = new ArrayList<>();
        IndustryKit.record(test, "glass", r.summary());
        IndustryKit.record(test, "glass checksum", r.checksum().summary());
        for (GlassReport.Good g : r.goods()) {
            String name = names.apply(g.good());
            IndustryKit.record(test, "glass good", g.describe(name == null ? "good " + g.good() : name));
            if (g.cls() != GlassReport.UNBOUNDED && g.verdict() == GlassReport.FAIL)
                failed.add((name == null ? "good " + g.good() : name) + " (" + g.className() + ")");
        }
        return failed;
    }

    private static final String[] NODE_KINDS = {"?", "STORE", "BUFFER", "MACHINE", "MOVER", "JUNCTION", "HEATER", "SOURCE", "SINK",
            "BLACKBOX", "CONTROLLER", "PORT", "BOILER"};
    private static final String[] EDGE_KINDS = {"?", "ADJ", "BELT", "CHUTE", "PIPE", "CHAIN", "AIR", "ARM", "HEAT", "CONTROL", "OBSERVE",
            "LINE", "DROP"};

    private static String name(int good) {
        String n = IndustryKit.javaName(good);
        return n == null ? "g" + good : n.replaceFirst("^[if]/\\d+/", "");
    }

    private static String q(long num, long den) {
        return den == 1 ? Long.toString(num) : num + "/" + den;
    }

    /** One line per node, edge, recipe and domain of a compile, goods by name: what the solver was given. */
    public static List<String> dump(GlassKit.Graph g) {
        List<String> out = new ArrayList<>();
        out.add(g.describe());
        for (GlassKit.Node n : g.nodes()) {
            StringBuilder sb = new StringBuilder("n" + n.index() + " " + NODE_KINDS[Math.min(n.kind(), NODE_KINDS.length - 1)] + " model "
                    + n.model() + " at " + n.x() + "," + n.y() + "," + n.z() + " flags " + n.flags() + " rule " + n.rule());
            if (n.domain() >= 0)
                sb.append(" dom ").append(n.domain());
            if (n.inventory() >= 0)
                sb.append(" inv ").append(n.inventory());
            if (n.capacity() != 0)
                sb.append(" cap ").append(n.capacity());
            if (n.params() != null && java.util.Arrays.stream(n.params()).anyMatch(v -> v != 0))
                sb.append(" params ").append(java.util.Arrays.toString(n.params()));
            for (GlassKit.Rate r : n.rates())
                sb.append(" [l").append(r.level()).append(" r").append(r.recipe() < 0 ? "-" : Integer.toString(r.recipe())).append(' ')
                        .append(r.batch()).append(" per ").append(q(r.ticksNum(), r.ticksDen())).append("t f").append(r.flags()).append(']');
            out.add(sb.toString());
        }
        for (GlassKit.Edge e : g.edges()) {
            StringBuilder sb = new StringBuilder("e" + e.index() + " " + EDGE_KINDS[Math.min(e.kind(), EDGE_KINDS.length - 1)] + " n" + e.from()
                    + " -> n" + e.to() + " order " + e.order() + " flags " + e.flags());
            if (e.capDen() != 0)
                sb.append(" cap ").append(q(e.capNum(), e.capDen()));
            if (e.latency() != 0)
                sb.append(" lat ").append(e.latency());
            if (e.param() != 0)
                sb.append(" param ").append(e.param());
            if (e.filter() != null && e.filter().length > 0) {
                sb.append(" filter");
                for (int f : e.filter())
                    sb.append(' ').append(name(f));
            }
            out.add(sb.toString());
        }
        for (GlassKit.Recipe r : g.recipes()) {
            StringBuilder sb = new StringBuilder("r" + r.index() + " type " + r.type() + " heat " + r.heat() + " "
                    + (r.name() == 0 ? "" : IndustryKit.javaName(r.name())) + ":");
            for (GlassKit.Term t : r.inputs())
                sb.append(' ').append(q(t.num(), t.den())).append(' ').append(name(t.good())).append(t.flags() != 0 ? "(f" + t.flags() + ")" : "");
            sb.append(" ->");
            for (GlassKit.Term t : r.outputs())
                sb.append(' ').append(q(t.num(), t.den())).append(' ').append(name(t.good())).append(t.flags() != 0 ? "(f" + t.flags() + ")" : "");
            out.add(sb.toString());
        }
        for (GlassKit.Domain d : g.domains()) {
            StringBuilder sb = new StringBuilder("d" + d.index() + " current " + d.current() + " driver " + d.driver() + " flags " + d.flags()
                    + " live " + d.liveStress() + "/" + d.liveCapacity() + " levels");
            for (GlassKit.Level l : d.levels())
                sb.append(" [s").append(l.selector()).append(' ').append(l.rpm()).append("rpm ").append(l.capacity()).append("SU]");
            sb.append(" members");
            for (GlassKit.Member m : d.members())
                sb.append(" n").append(m.node()).append('x').append(m.count()).append('@').append(m.ratioNum()).append('/').append(m.ratioDen())
                        .append(" ").append(m.impact());
            out.add(sb.toString());
        }
        return out;
    }

    /** Logs {@link #dump} of the district's latest compile in the recorder's records. */
    public static void logGraph(String test, long district, Recorder rec) {
        GlassKit.Graph g = GlassKit.latest(district, rec.records());
        if (g == null) {
            IndustryKit.record(test, "graph", "no compile received");
            return;
        }
        for (String line : dump(g))
            com.minecraftindustrialcomplex.mic_rtsim.MicRtsim.LOGGER.info("[gametest] {}: graph {}", test, line);
    }
}
