package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTestHelper;

import java.util.List;

/**
 * One factory of the glass box's G1 corpus (GLASSBOX.md §11.3, plus C32's sheep farm): a real, running
 * Create build placed in the GameTest yard, with the rate per good worked out by hand from Create's own
 * code. The smoke GameTests ({@link GlassCorpusGameTests}) check that Create really runs it at that rate;
 * the G1 exit test checks the glass box's prediction against the same window.
 *
 * <p>Ticks are game ticks; rates are per minute (1,200 ticks), net over every store of the factory:
 * positive for a good the factory makes, negative for one it uses.
 */
public interface CorpusFactory {

    /** A short id, used in test names, batches and log lines. */
    String name();

    /**
     * Places the factory in the yard (blocks, block entity settings, store contents) and returns where its
     * stores are and what it should do. Positions in {@link Built} are absolute. It does not mark anything
     * as player placed: {@link GlassCorpus#prepare} does that for every store, so the factory becomes
     * exactly one district.
     */
    Built build(GameTestHelper helper);

    /** Ticks from the end of {@link #build} until belts, buffers and machines are in their steady state. */
    int warmupTicks();

    /**
     * Called every tick of the measuring window with the tick since the window began (0 is the first tick
     * after the warm-up). Factories whose regime changes on purpose (the boiler losing its water, the
     * threshold switch) act here; the rest do nothing.
     */
    default void perturb(GameTestHelper helper, Built built, int windowTick) {}

    /** The good counted, as a registry id: an item ({@code minecraft:wheat}) or a fluid in mB ({@code fluid:minecraft:water}). */
    enum Kind {
        /** Every rule on the good's paths is exact (§9.3): the prediction is one number. */
        EXACT,
        /** A bounded rule lies on its paths: the prediction is an interval [lo, hi]. */
        BOUNDED
    }

    /**
     * What the factory should do with one good, per minute.
     *
     * @param good         item id, or {@code fluid:<id>} for a fluid in mB
     * @param perMinute    the expected net rate (for BOUNDED, the most likely value inside [lo, hi])
     * @param kind         EXACT or BOUNDED
     * @param lo           BOUNDED: the floor; EXACT: equal to perMinute
     * @param hi           BOUNDED: the ceiling; EXACT: equal to perMinute
     * @param varPerMinute the variance of the count added per minute of window, for chance outputs and
     *                     animals (a window of W minutes has variance {@code varPerMinute × W}); 0 when
     *                     the good is deterministic
     * @param derivation   one line: where the number comes from in Create's code
     */
    record Expect(String good, double perMinute, Kind kind, double lo, double hi, double varPerMinute, String derivation) {
        public static Expect exact(String good, double perMinute, String derivation) {
            return new Expect(good, perMinute, Kind.EXACT, perMinute, perMinute, 0, derivation);
        }

        /** Exact in expectation, with chance outputs: the count's variance grows by {@code varPerMinute} per minute. */
        public static Expect expected(String good, double perMinute, double varPerMinute, String derivation) {
            return new Expect(good, perMinute, Kind.EXACT, perMinute, perMinute, varPerMinute, derivation);
        }

        public static Expect bounded(String good, double lo, double perMinute, double hi, String derivation) {
            return new Expect(good, perMinute, Kind.BOUNDED, lo, hi, 0, derivation);
        }
    }

    /**
     * A stretch of the window in one regime: ticks [from, to) after the window began, and what each good
     * does there. A factory without regime changes has one phase over the whole window.
     */
    record Phase(String name, int from, int to, List<Expect> expect) {}

    /**
     * What {@link #build} placed.
     *
     * @param anchor  a block inside the district (the main machine)
     * @param inputs  stores the factory takes from (chests, vaults, tanks), absolute
     * @param outputs stores it fills, absolute
     * @param expect  the steady state per good, over the whole window (for a factory with phases, the
     *                first phase's steady state)
     * @param phases  the regimes inside the window; empty means one phase with {@code expect}
     * @param window  the smoke test's window in ticks (at least 2,400)
     * @param notes   free text for the log: speeds, regimes, what is bounded and why
     */
    record Built(BlockPos anchor, List<BlockPos> inputs, List<BlockPos> outputs, List<Expect> expect,
                 List<Phase> phases, int window, String notes) {
        public Built(BlockPos anchor, List<BlockPos> inputs, List<BlockPos> outputs, List<Expect> expect, String notes) {
            this(anchor, inputs, outputs, expect, List.of(), 2400, notes);
        }

        /** The phases the smoke test checks: the declared ones, or one over the whole window. */
        public List<Phase> effectivePhases() {
            return phases.isEmpty() ? List.of(new Phase("steady", 0, window, expect)) : phases;
        }

        public List<BlockPos> stores() {
            java.util.ArrayList<BlockPos> all = new java.util.ArrayList<>(inputs);
            for (BlockPos p : outputs)
                if (!all.contains(p))
                    all.add(p);
            return all;
        }
    }
}
