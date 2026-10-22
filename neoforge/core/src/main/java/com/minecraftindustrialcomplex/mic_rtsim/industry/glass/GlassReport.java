package com.minecraftindustrialcomplex.mic_rtsim.industry.glass;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

/**
 * A district's glass report (G1, read only), decoded from the engine's flat form
 * ({@code rtsim_industry::glass::report::Report::to_f64s}; docs/industry-boundary.md §9.3 says what each
 * number means). Rates are per tick: items in units, fluids in millibuckets. Operators read it through
 * {@code /rtsim district} and the log, GameTests through {@code NativeEngine.glassReport}; nothing in the
 * world shows it (I18).
 */
public record GlassReport(int compile, int rounds, boolean converged, int nodes, int edges, double solveMicros,
                          long windowStart, long windowTicksDone, long windowTicksTarget, int windowsCompleted,
                          int verdict, int strikes, int solves, int demoted, long latency, Checksum checksum,
                          Replay replay, List<Good> goods) {

    /** How a good is judged. */
    public static final int EXACT = 0, BOUNDED = 1, UNBOUNDED = 2;
    /** A window's verdict. */
    public static final int NONE = 0, PASS = 1, FAIL = 2;

    public static final int HEADER = 41, GOOD_STRIDE = 16;

    /** What replays the district ({@code glass::report::replay_model}). */
    public static final int REPLAY_NONE = 0, REPLAY_BLACK_BOX = 1, REPLAY_GLASS = 2;

    /** Why a window was thrown away or a sweep not counted ({@code glass::report::discard}); index = reason. */
    public static final String[] DISCARDS = {"none", "no sweep", "dropped records", "not live", "edit", "revision",
            "recompile", "session start"};

    /** Why a good's class is what it is ({@code glass::report::why}); index = reason. */
    public static final String[] WHY = {"none", "node flag", "rule", "cycle", "black box", "port", "unread store",
            "edge flag", "line", "drop", "unconverged", "U infinite", "unlimited flow", "shared edge", "short inputs",
            "shared capacity", "time share", "shared input", "part duty", "priority tie"};
    /** The {@link #WHY} reasons whose place is an edge, not a node (7 to 10). */
    private static final int WHY_EDGE_FLAG = 7, WHY_LINE = 8, WHY_DROP = 9, WHY_UNCONVERGED = 10;

    /**
     * The checksum's bookkeeping for this compile (header slots 16 to 32): when the compile was accepted
     * and the next window may open, sweeps taken and sampled, heaters still lit on their compile-time burn
     * and the tick the next of them changes, the compile's and the district's inventory revisions, and
     * the window discards and sweep skips by reason ({@link #DISCARDS}). -1 is "none".
     */
    public record Checksum(long acceptedAt, long readyAt, int sweeps, int sweepsSampled, int heatersBurning,
                           long nextBreakpoint, int revision, long districtRevision, long lastDiscardTick,
                           int lastDiscardReason, long[] discards) {

        /** Discards and skips for one reason (1 to 7). */
        public long discards(int reason) {
            return reason >= 1 && reason <= discards.length ? discards[reason - 1] : 0;
        }

        public String summary() {
            StringBuilder sb = new StringBuilder(String.format(Locale.ROOT,
                    "accepted at %d, ready at %d, %d sweeps (%d sampled), %d heaters burning%s, revision %d (district %s)",
                    acceptedAt, readyAt, sweeps, sweepsSampled, heatersBurning,
                    nextBreakpoint < 0 ? "" : " until " + nextBreakpoint, revision,
                    districtRevision < 0 ? "none" : Long.toString(districtRevision)));
            if (lastDiscardTick >= 0)
                sb.append(", last discard ").append(reason(lastDiscardReason)).append(" at ").append(lastDiscardTick);
            StringBuilder counts = new StringBuilder();
            for (int i = 1; i <= discards.length; i++)
                if (discards(i) != 0)
                    counts.append(counts.isEmpty() ? "" : ", ").append(reason(i)).append(' ').append(discards(i));
            if (!counts.isEmpty())
                sb.append(" (").append(counts).append(')');
            return sb.toString();
        }

        private static String reason(int r) {
            return r >= 0 && r < DISCARDS.length ? DISCARDS[r] : "reason " + r;
        }
    }

    /**
     * The district's replay while unloaded (header slots 33 to 40, G2): which model replays it
     * ({@link #REPLAY_NONE}, {@link #REPLAY_BLACK_BOX}, {@link #REPLAY_GLASS}), the tick its replay started
     * (-1 none), the regimes entered and breakpoints handled since the compile, regimes that fell back to
     * the black box as oscillating, cut edges between live and simulated nodes, whether a window of the
     * compile passed (G-B4), and goods the residual replays as learned sources.
     */
    public record Replay(int model, long since, int regimes, int breakpoints, int oscillations, int cutEdges,
                         boolean eligible, int learnedSources) {

        public String modelName() {
            return switch (model) {
                case REPLAY_NONE -> "none";
                case REPLAY_BLACK_BOX -> "black box";
                case REPLAY_GLASS -> "glass";
                default -> "model " + model;
            };
        }

        public String summary() {
            return String.format(Locale.ROOT, "replay %s%s, %s, %d regimes, %d breakpoints, %d oscillations, %d cut edges, %d learned sources",
                    modelName(), since < 0 ? "" : " since " + since, eligible ? "eligible" : "not eligible", regimes, breakpoints,
                    oscillations, cutEdges, learnedSources);
        }
    }

    /**
     * One good: flow into and out of the district's stores as L and U, the predicted net change of the
     * sampled stock, the window's measured slope (NaN before a window), its standard error and batch
     * quantization, the verdict, the sweeps in the window, the band a bounded good is judged in (before
     * the tolerance) and why the good has its class, with the node (or edge) it came from (-1 none).
     */
    public record Good(int good, int cls, double inL, double inU, double outL, double outU, double predicted,
                       double measured, double sigma, double q, int verdict, int samples, double lo, double hi,
                       int why, int whyAt) {

        public String className() {
            return switch (cls) {
                case EXACT -> "exact";
                case BOUNDED -> "bounded";
                default -> "unbounded";
            };
        }

        /** Why the good has its class, and where: "rule at n5", "drop at e3"; empty for none. */
        public String reason() {
            if (why <= 0)
                return "";
            String r = why < WHY.length ? WHY[why] : "reason " + why;
            if (whyAt < 0)
                return r;
            boolean edge = why == WHY_EDGE_FLAG || why == WHY_LINE || why == WHY_DROP || why == WHY_UNCONVERGED;
            return r + " at " + (edge ? "e" : "n") + whyAt;
        }

        /** Per minute (1,200 ticks), as players think of rates. */
        public String describe(String name) {
            String why = reason();
            return String.format(Locale.ROOT, "%s %s%s: measured %.3f/min predicted %.3f/min [%.3f, %.3f] sigma %.4f q %.4f %s (%d sweeps)",
                    name, className(), why.isEmpty() ? "" : " (" + why + ")", measured * 1200, predicted * 1200,
                    lo * 1200, hi * 1200, sigma * 1200, q * 1200,
                    verdict == PASS ? "pass" : verdict == FAIL ? "FAIL" : "not judged", samples);
        }
    }

    /** Null for a null array (no complete compile). */
    public static GlassReport decode(double[] v) {
        if (v == null)
            return null;
        if (v.length < HEADER)
            throw new IllegalArgumentException("a glass report of " + v.length + " values");
        int n = (int) v[12];
        if (v.length != HEADER + n * GOOD_STRIDE)
            throw new IllegalArgumentException("a glass report of " + v.length + " values for " + n + " goods");
        List<Good> goods = new ArrayList<>(n);
        for (int i = 0; i < n; i++) {
            int b = HEADER + i * GOOD_STRIDE;
            goods.add(new Good((int) v[b], (int) v[b + 1], v[b + 2], v[b + 3], v[b + 4], v[b + 5], v[b + 6], v[b + 7],
                    v[b + 8], v[b + 9], (int) v[b + 10], (int) v[b + 11], v[b + 12], v[b + 13], (int) v[b + 14],
                    (int) v[b + 15]));
        }
        long[] discards = new long[7];
        for (int i = 0; i < discards.length; i++)
            discards[i] = (long) v[26 + i];
        Checksum c = new Checksum((long) v[16], (long) v[17], (int) v[18], (int) v[19], (int) v[20], (long) v[21],
                (int) v[22], (long) v[23], (long) v[24], (int) v[25], discards);
        Replay rp = new Replay((int) v[33], (long) v[34], (int) v[35], (int) v[36], (int) v[37], (int) v[38], v[39] != 0,
                (int) v[40]);
        return new GlassReport((int) v[0], (int) v[1], v[2] != 0, (int) v[3], (int) v[4], v[5], (long) v[6], (long) v[7],
                (long) v[8], (int) v[9], (int) v[10], (int) v[11], (int) v[13], (int) v[14], (long) v[15], c, rp,
                List.copyOf(goods));
    }

    public Good good(int id) {
        for (Good g : goods)
            if (g.good == id)
                return g;
        return null;
    }

    public String summary() {
        return String.format(Locale.ROOT, "compile %d: %d nodes, %d edges, %d rounds (%s), %d solves, last %.0f us, latency %d ticks; window %d/%d ticks from %d, %d done, verdict %s, strikes %d, %d kinds demoted",
                compile, nodes, edges, rounds, converged ? "converged" : "NOT converged", solves, solveMicros, latency, windowTicksDone,
                windowTicksTarget, windowStart, windowsCompleted, verdict == PASS ? "pass" : verdict == FAIL ? "FAIL" : "none", strikes, demoted);
    }
}
