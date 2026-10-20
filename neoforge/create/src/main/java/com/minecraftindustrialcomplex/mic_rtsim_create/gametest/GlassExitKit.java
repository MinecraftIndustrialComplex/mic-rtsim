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
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            for (ScriptedEngine.Received r : kept)
                out.writeBytes(r.bytes());
            return out.toByteArray();
        }

        public synchronized int size() {
            return kept.size();
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
        for (GlassReport.Good g : r.goods()) {
            String name = names.apply(g.good());
            IndustryKit.record(test, "glass good", g.describe(name == null ? "good " + g.good() : name));
            if (g.cls() != GlassReport.UNBOUNDED && g.verdict() == GlassReport.FAIL)
                failed.add((name == null ? "good " + g.good() : name) + " (" + g.className() + ")");
        }
        return failed;
    }
}
