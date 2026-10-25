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
                          Replay replay, List<Good> goods, List<Unit> units, List<Section> sections,
                          List<Shared> shared) {

    /** How a good is judged. */
    public static final int EXACT = 0, BOUNDED = 1, UNBOUNDED = 2;
    /** A window's verdict. */
    public static final int NONE = 0, PASS = 1, FAIL = 2;

    public static final int HEADER = 42, GOOD_STRIDE = 16, UNIT_STRIDE = 15;
    /** The sections part ({@code glass::report::SECTION_STRIDE} and the strides after it). */
    public static final int SECTION_STRIDE = 27, SECTION_STORE_STRIDE = 3, SHARED_STRIDE = 12, SHARED_LINK_STRIDE = 2,
            SHARED_RESIDUE_STRIDE = 3;

    /** A section's state ({@code glass::report::section_state}); index = code. */
    public static final int LEARNING = 0, PASSED_STATE = 1, FAILED = 2, STALE_STATE = 3, FROZEN_STATE = 4, REPLAYING = 5;
    public static final String[] SECTION_STATES = {"learning", "passed", "failed", "stale", "frozen", "replaying"};

    /** What replays the district ({@code glass::report::replay_model}). */
    public static final int REPLAY_NONE = 0, REPLAY_BLACK_BOX = 1, REPLAY_GLASS = 2;

    /** Why a window was thrown away or a sweep not counted ({@code glass::report::discard}); index = reason. */
    public static final String[] DISCARDS = {"none", "no sweep", "dropped records", "not live", "edit", "revision",
            "recompile", "session start", "station cargo"};

    /** Why a good's class is what it is ({@code glass::report::why}); index = reason. */
    public static final String[] WHY = {"none", "node flag", "rule", "cycle", "black box", "port", "unread store",
            "edge flag", "line", "drop", "unconverged", "U infinite", "unlimited flow", "shared edge", "short inputs",
            "shared capacity", "time share", "shared input", "part duty", "priority tie", "shared network"};
    /** The {@link #WHY} reasons whose place is an edge, not a node (7 to 10). */
    private static final int WHY_EDGE_FLAG = 7, WHY_LINE = 8, WHY_DROP = 9, WHY_UNCONVERGED = 10;

    /**
     * The checksum's bookkeeping for this compile (header slots 16 to 32, and 41): when the compile was accepted
     * and the next window may open, sweeps taken and sampled, heaters still lit on their compile-time burn
     * and the tick the next of them changes, the compile's and the district's inventory revisions, and
     * the window discards and sweep skips by reason ({@link #DISCARDS}). -1 is "none".
     */
    public record Checksum(long acceptedAt, long readyAt, int sweeps, int sweepsSampled, int heatersBurning,
                           long nextBreakpoint, int revision, long districtRevision, long lastDiscardTick,
                           int lastDiscardReason, long[] discards) {

        /** Discards and skips for one reason (1 to 8). */
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

    /**
     * One judging unit (SECTIONS.md §3.4, choice SC74): its sections and nodes, the box its sections' node
     * boxes span ({@code hasBox} false: none has one), its open window's ticks, windows completed, last
     * verdict and strikes in a row, whether a window of it passed, an edit staled it since, it may replay
     * now, a fallback froze it, its goods start unbounded, and the tick its window may open from.
     */
    public record Unit(int sections, int nodes, boolean hasBox, int loX, int loY, int loZ, int hiX, int hiY, int hiZ,
                       long windowTicks, int windowsCompleted, int verdict, int strikes, boolean passed, boolean stale,
                       boolean mayReplay, boolean frozen, boolean unbounded, long readyAt) {

        /** Whether its box holds the position. */
        public boolean holds(int x, int y, int z) {
            return hasBox && loX <= x && x <= hiX && loY <= y && y <= hiY && loZ <= z && z <= hiZ;
        }

        public String summary() {
            return String.format(Locale.ROOT, "%d sections, %d nodes, box %s, window %d ticks, %d done, verdict %s, strikes %d%s%s%s%s%s",
                    sections, nodes, hasBox ? "(" + loX + " " + loY + " " + loZ + ")..(" + hiX + " " + hiY + " " + hiZ + ")" : "none",
                    windowTicks, windowsCompleted, verdict == PASS ? "pass" : verdict == FAIL ? "FAIL" : "none", strikes,
                    passed ? ", passed" : "", stale ? ", stale" : "", mayReplay ? ", may replay" : "", frozen ? ", frozen" : "",
                    unbounded ? ", unbounded" : "");
        }
    }


    /**
     * One section (SECTIONS.md §8.1, choice SC165): its judging unit (-1: the district's window judges), judging
     * group, nodes, stores (with the inventories they hold, -1 for none), domains, box, lowest node position,
     * fingerprint, state ({@link #SECTION_STATES}), last verdict, strikes, windows completed, open window ticks and
     * the failed good of its last window that missed its band by the most ({@code worstGood} -1: none).
     */
    public record Section(int unit, int group, int nodes, int storeCount, int sharedStores, int domains, int flags,
                          int state, boolean hasBox, int loX, int loY, int loZ, int hiX, int hiY, int hiZ, boolean hasLow,
                          int lowX, int lowY, int lowZ, long fingerprint, int verdict, int strikes, int windowsCompleted,
                          long windowTicks, int worstGood, double worstMeasured, double worstPredicted,
                          double worstExcess, List<StoreRef> stores) {

        public static final int PASSED = 1, STALE = 2, MAY_REPLAY = 4, FROZEN = 8, WORKING = 16;

        public boolean passed() {
            return (flags & PASSED) != 0;
        }

        public boolean stale() {
            return (flags & STALE) != 0;
        }

        public boolean mayReplay() {
            return (flags & MAY_REPLAY) != 0;
        }

        public boolean frozen() {
            return (flags & FROZEN) != 0;
        }

        public boolean working() {
            return (flags & WORKING) != 0;
        }

        public String stateName() {
            return state >= 0 && state < SECTION_STATES.length ? SECTION_STATES[state] : "state " + state;
        }

        /** Whether its box holds the position. */
        public boolean holds(int x, int y, int z) {
            return hasBox && loX <= x && x <= hiX && loY <= y && y <= hiY && loZ <= z && z <= hiZ;
        }

        /** The 16 hex digits of the fingerprint. */
        public String fingerprintHex() {
            return String.format(Locale.ROOT, "%016x", fingerprint);
        }
    }

    /** A store a section touches: its inventory (-1: none) and whether another section touches it too. */
    public record StoreRef(int inventory, boolean shared) {}

    /**
     * A store shared by sections (SECTIONS.md §4.2, choice SC168): its node, inventory (-1 none), position, sections
     * that touch it, whether its moves are counted at its movers (it lies between units), the (mover, store) pairs
     * Java counts at it, whether the latest count sample covered every pair, that sample's sweep (-1 none) and its
     * residue per good since the baseline (goods and amounts).
     */
    public record Shared(int node, int inventory, boolean hasPos, int x, int y, int z, List<Integer> sections,
                         boolean counted, int pairs, boolean countedNow, long sampleSweep, List<long[]> residue) {

        /** The amount the residue holds of a good; 0 when none. */
        public long residueOf(int good) {
            for (long[] r : residue)
                if (r[0] == good)
                    return r[1];
            return 0;
        }
    }

    /** Null for a null array (no complete compile). */
    public static GlassReport decode(double[] v) {
        if (v == null)
            return null;
        if (v.length < HEADER + 1)
            throw new IllegalArgumentException("a glass report of " + v.length + " values");
        int n = (int) v[12];
        int tail = HEADER + n * GOOD_STRIDE;
        int u = tail < v.length ? (int) v[tail] : -1;
        int at = tail + 1 + u * UNIT_STRIDE;
        if (u < 0 || at >= v.length)
            throw new IllegalArgumentException("a glass report of " + v.length + " values for " + n + " goods");
        List<Good> goods = new ArrayList<>(n);
        for (int i = 0; i < n; i++) {
            int b = HEADER + i * GOOD_STRIDE;
            goods.add(new Good((int) v[b], (int) v[b + 1], v[b + 2], v[b + 3], v[b + 4], v[b + 5], v[b + 6], v[b + 7],
                    v[b + 8], v[b + 9], (int) v[b + 10], (int) v[b + 11], v[b + 12], v[b + 13], (int) v[b + 14],
                    (int) v[b + 15]));
        }
        long[] discards = new long[8];
        for (int i = 0; i < 7; i++)
            discards[i] = (long) v[26 + i];
        discards[7] = (long) v[41];
        Checksum c = new Checksum((long) v[16], (long) v[17], (int) v[18], (int) v[19], (int) v[20], (long) v[21],
                (int) v[22], (long) v[23], (long) v[24], (int) v[25], discards);
        Replay rp = new Replay((int) v[33], (long) v[34], (int) v[35], (int) v[36], (int) v[37], (int) v[38], v[39] != 0,
                (int) v[40]);
        int[] pos = {at};
        List<Section> sections = sections(v, pos);
        List<Shared> shared = shared(v, pos);
        if (pos[0] != v.length)
            throw new IllegalArgumentException("a glass report of " + v.length + " values for " + n + " goods");
        return new GlassReport((int) v[0], (int) v[1], v[2] != 0, (int) v[3], (int) v[4], v[5], (long) v[6], (long) v[7],
                (long) v[8], (int) v[9], (int) v[10], (int) v[11], (int) v[13], (int) v[14], (long) v[15], c, rp,
                List.copyOf(goods), units(v, tail + 1, u), sections, shared);
    }


    private static List<Section> sections(double[] v, int[] pos) {
        int at = pos[0];
        int n = at < v.length ? (int) v[at] : -1;
        if (n < 0 || at + 1 + n * SECTION_STRIDE >= v.length)
            throw new IllegalArgumentException("a glass report of " + v.length + " values has no sections part");
        int sc = at + 1 + n * SECTION_STRIDE;
        int m = (int) v[sc];
        if (m < 0 || sc + 1 + m * SECTION_STORE_STRIDE > v.length)
            throw new IllegalArgumentException("a glass report of " + v.length + " values has a short store list");
        List<List<StoreRef>> stores = new ArrayList<>(n);
        for (int i = 0; i < n; i++)
            stores.add(new ArrayList<>());
        for (int i = 0; i < m; i++) {
            int b = sc + 1 + i * SECTION_STORE_STRIDE;
            int s = (int) v[b];
            if (s < 0 || s >= n)
                throw new IllegalArgumentException("a store of section " + s + " in a report of " + n + " sections");
            stores.get(s).add(new StoreRef((int) v[b + 1], v[b + 2] != 0));
        }
        List<Section> out = new ArrayList<>(n);
        for (int i = 0; i < n; i++) {
            int b = at + 1 + i * SECTION_STRIDE;
            int flags = (int) v[b + 6];
            long fp = ((long) v[b + 17] << 32) | (long) v[b + 18];
            out.add(new Section((int) v[b], (int) v[b + 1], (int) v[b + 2], (int) v[b + 3], (int) v[b + 4], (int) v[b + 5],
                    flags, (int) v[b + 7], (flags & 32) != 0, (int) v[b + 8], (int) v[b + 9], (int) v[b + 10],
                    (int) v[b + 11], (int) v[b + 12], (int) v[b + 13], (flags & 64) != 0, (int) v[b + 14], (int) v[b + 15],
                    (int) v[b + 16], fp, (int) v[b + 19], (int) v[b + 20], (int) v[b + 21], (long) v[b + 22],
                    (int) v[b + 23], v[b + 24], v[b + 25], v[b + 26], List.copyOf(stores.get(i))));
        }
        pos[0] = sc + 1 + m * SECTION_STORE_STRIDE;
        return List.copyOf(out);
    }

    private static List<Shared> shared(double[] v, int[] pos) {
        int at = pos[0];
        int n = at < v.length ? (int) v[at] : -1;
        if (n < 0 || at + 1 + n * SHARED_STRIDE >= v.length)
            throw new IllegalArgumentException("a glass report of " + v.length + " values has no shared stores part");
        int lc = at + 1 + n * SHARED_STRIDE;
        int links = (int) v[lc];
        int rc = lc + 1 + links * SHARED_LINK_STRIDE;
        if (links < 0 || rc >= v.length)
            throw new IllegalArgumentException("a glass report of " + v.length + " values has a short link list");
        int res = (int) v[rc];
        int end = rc + 1 + res * SHARED_RESIDUE_STRIDE;
        if (res < 0 || end > v.length)
            throw new IllegalArgumentException("a glass report of " + v.length + " values has a short residue list");
        List<List<Integer>> sections = new ArrayList<>(n);
        List<List<long[]>> residues = new ArrayList<>(n);
        for (int i = 0; i < n; i++) {
            sections.add(new ArrayList<>());
            residues.add(new ArrayList<>());
        }
        for (int i = 0; i < links; i++) {
            int b = lc + 1 + i * SHARED_LINK_STRIDE;
            sections.get(checkStore((int) v[b], n)).add((int) v[b + 1]);
        }
        for (int i = 0; i < res; i++) {
            int b = rc + 1 + i * SHARED_RESIDUE_STRIDE;
            residues.get(checkStore((int) v[b], n)).add(new long[] {(long) v[b + 1], (long) v[b + 2]});
        }
        List<Shared> out = new ArrayList<>(n);
        for (int i = 0; i < n; i++) {
            int b = at + 1 + i * SHARED_STRIDE;
            out.add(new Shared((int) v[b], (int) v[b + 1], v[b + 2] != 0, (int) v[b + 3], (int) v[b + 4], (int) v[b + 5],
                    List.copyOf(sections.get(i)), v[b + 7] != 0, (int) v[b + 8], v[b + 9] != 0, (long) v[b + 10],
                    List.copyOf(residues.get(i))));
        }
        pos[0] = end;
        return List.copyOf(out);
    }

    private static int checkStore(int i, int n) {
        if (i < 0 || i >= n)
            throw new IllegalArgumentException("shared store " + i + " in a report of " + n);
        return i;
    }

    private static List<Unit> units(double[] v, int at, int n) {
        List<Unit> out = new ArrayList<>(n);
        for (int i = 0; i < n; i++) {
            int b = at + i * UNIT_STRIDE;
            int flags = (int) v[b + 13];
            out.add(new Unit((int) v[b], (int) v[b + 1], v[b + 2] != 0, (int) v[b + 3], (int) v[b + 4], (int) v[b + 5],
                    (int) v[b + 6], (int) v[b + 7], (int) v[b + 8], (long) v[b + 9], (int) v[b + 10], (int) v[b + 11],
                    (int) v[b + 12], (flags & 1) != 0, (flags & 2) != 0, (flags & 4) != 0, (flags & 8) != 0,
                    (flags & 16) != 0, (long) v[b + 14]));
        }
        return List.copyOf(out);
    }

    /** The units whose box holds the position (an edit there stales them, §3.6). */
    public List<Unit> unitsAt(int x, int y, int z) {
        List<Unit> out = new ArrayList<>();
        for (Unit un : units)
            if (un.holds(x, y, z))
                out.add(un);
        return out;
    }

    /** The sections whose node box holds the position. */
    public List<Section> sectionsAt(int x, int y, int z) {
        List<Section> out = new ArrayList<>();
        for (Section s : sections)
            if (s.holds(x, y, z))
                out.add(s);
        return out;
    }

    /** Sections that may replay now (their unit's rule, or the district's). */
    public int sectionsMayReplay() {
        int n = 0;
        for (Section s : sections)
            if (s.mayReplay())
                n++;
        return n;
    }

    /** Sections the district is replaying in glass now. */
    public int sectionsReplaying() {
        int n = 0;
        for (Section s : sections)
            if (s.state() == REPLAYING)
                n++;
        return n;
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
