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
                          int verdict, int strikes, List<Good> goods) {

    /** How a good is judged. */
    public static final int EXACT = 0, BOUNDED = 1, UNBOUNDED = 2;
    /** A window's verdict. */
    public static final int NONE = 0, PASS = 1, FAIL = 2;

    public static final int HEADER = 16, GOOD_STRIDE = 12;

    /**
     * One good: flow into and out of the district's stores as L and U, the predicted net change of the
     * sampled stock, the window's measured slope (NaN before a window), its standard error and batch
     * quantization, the verdict and the sweeps in the window.
     */
    public record Good(int good, int cls, double inL, double inU, double outL, double outU, double predicted,
                       double measured, double sigma, double q, int verdict, int samples) {

        /** The good's interval for the net change: one point when exact. */
        public double lo() {
            return cls == EXACT ? predicted : inL - outU;
        }

        public double hi() {
            return cls == EXACT ? predicted : inU - outL;
        }

        public String className() {
            return switch (cls) {
                case EXACT -> "exact";
                case BOUNDED -> "bounded";
                default -> "unbounded";
            };
        }

        /** Per minute (1,200 ticks), as players think of rates. */
        public String describe(String name) {
            return String.format(Locale.ROOT, "%s %s: measured %.3f/min predicted %.3f/min [%.3f, %.3f] sigma %.4f q %.4f %s (%d sweeps)",
                    name, className(), measured * 1200, predicted * 1200, lo() * 1200, hi() * 1200, sigma * 1200, q * 1200,
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
                    v[b + 8], v[b + 9], (int) v[b + 10], (int) v[b + 11]));
        }
        return new GlassReport((int) v[0], (int) v[1], v[2] != 0, (int) v[3], (int) v[4], v[5], (long) v[6], (long) v[7],
                (long) v[8], (int) v[9], (int) v[10], (int) v[11], List.copyOf(goods));
    }

    public Good good(int id) {
        for (Good g : goods)
            if (g.good == id)
                return g;
        return null;
    }

    public String summary() {
        return String.format(Locale.ROOT, "compile %d: %d nodes, %d edges, %d rounds (%s), solve %.0f us; window %d/%d ticks from %d, %d done, verdict %s, strikes %d",
                compile, nodes, edges, rounds, converged ? "converged" : "NOT converged", solveMicros, windowTicksDone,
                windowTicksTarget, windowStart, windowsCompleted, verdict == PASS ? "pass" : verdict == FAIL ? "FAIL" : "none", strikes);
    }
}
