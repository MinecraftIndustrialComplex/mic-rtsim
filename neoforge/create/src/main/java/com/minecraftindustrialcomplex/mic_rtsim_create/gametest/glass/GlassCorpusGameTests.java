package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestGenerator;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.TestFunction;
import net.minecraft.world.level.block.Rotation;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.List;
import java.util.Locale;
import java.util.Set;
import java.util.stream.Collectors;

/**
 * The corpus's smoke GameTests: one per factory, each in its own batch (districts never merge and
 * {@code IndustryKit.forget} never wipes a neighbour's). Each builds the factory, waits out its warm-up,
 * samples its stores every tick of the window and checks each good's least-squares rate against the
 * hand-worked expectation. These measure Create itself; nothing here reads the glass extraction.
 *
 * <p>Tolerance: an EXACT good within 5% of its rate or 3σ of its chance outputs, whichever is wider (σ of
 * a least-squares slope over W minutes of a count whose variance grows by v per minute is
 * √(1.2 v / W)); a BOUNDED good inside [lo, hi] widened by 5%. A produced good must be above 0.
 *
 * <p>The environment variable {@code MIC_GLASS_ONLY} (comma-separated factory names) limits the run to
 * those factories, for iterating on one; unset, all run.
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class GlassCorpusGameTests {
    private GlassCorpusGameTests() {}

    /** Relative tolerance for EXACT goods and for the edges of BOUNDED ones. */
    public static final double REL_TOL = 0.05;
    /** Standard deviations allowed for chance outputs. */
    public static final double Z = 3.0;
    /** Absolute floor (per minute) for goods expected at 0. */
    public static final double ZERO_TOL = 0.1;

    @GameTestGenerator
    public static Collection<TestFunction> glassCorpus() {
        // MIC_GLASS_SUITE=smoke or exit runs only that suite of the corpus (iterating); unset, both run.
        String suite = System.getenv("MIC_GLASS_SUITE");
        if (suite != null && !suite.isBlank() && !suite.trim().equals("smoke"))
            return List.of();
        String only = System.getenv("MIC_GLASS_ONLY");
        Set<String> filter = only == null || only.isBlank() ? Set.of()
                : Arrays.stream(only.split(",")).map(String::trim).filter(s -> !s.isEmpty()).collect(Collectors.toSet());
        List<TestFunction> out = new ArrayList<>();
        for (CorpusFactory f : GlassCorpus.ALL) {
            if (!filter.isEmpty() && !filter.contains(f.name()))
                continue;
            int maxTicks = f.warmupTicks() + maxWindow(f) + 400;
            out.add(new TestFunction("glass_" + f.name(), "glasscorpus." + f.name().toLowerCase(Locale.ROOT),
                    MicRtsimCreate.MODID + ":" + IndustryKit.YARD, Rotation.NONE, maxTicks, 0L, true, false, 1, 1, true,
                    helper -> smoke(helper, f)));
        }
        return out;
    }

    /** The window is only known after build; the timeout allows the longest a factory may ask for. */
    private static int maxWindow(CorpusFactory f) {
        return 7200;
    }

    static void smoke(GameTestHelper helper, CorpusFactory f) {
        CorpusFactory.Built[] built = {null};
        StockWindow[] window = {null};
        District[] district = {null};
        helper.startSequence()
                .thenExecute(() -> {
                    built[0] = GlassCorpus.prepare(helper, f);
                    GlassCorpusKit.log(f.name(), "built", built[0].notes());
                })
                .thenIdle(f.warmupTicks())
                .thenExecute(() -> {
                    CorpusFactory.Built b = built[0];
                    if (b == null)
                        throw new GameTestAssertException(f.name() + ": build failed");
                    if (b.window() < 2400 || b.window() > maxWindow(f))
                        throw new GameTestAssertException("window " + b.window() + " outside [2400, " + maxWindow(f) + "]");
                    district[0] = GlassCorpus.district(helper, b);
                    GlassCorpusKit.log(f.name(), "district", district[0] + ", stores " + b.stores().size());
                    window[0] = new StockWindow(b.stores(), b.window());
                })
                .thenWaitUntil(() -> {
                    StockWindow w = window[0];
                    // A failed setup step has already failed the test, but the sequence still runs its
                    // next events (GameTestSequence.executeWithoutFail): no window, nothing to measure.
                    if (w == null || w.full())
                        return;
                    f.perturb(helper, built[0], w.ticks());
                    w.sample(helper.getLevel());
                    throw new GameTestAssertException("measuring " + f.name() + ": " + w.ticks() + " of " + built[0].window());
                })
                .thenExecute(() -> {
                    if (window[0] == null)
                        throw new GameTestAssertException(f.name() + ": the window never started (build or district step failed)");
                    verify(f, built[0], window[0]);
                })
                .thenSucceed();
    }

    static void verify(CorpusFactory f, CorpusFactory.Built b, StockWindow w) {
        List<String> failures = new ArrayList<>();
        for (CorpusFactory.Phase ph : b.effectivePhases()) {
            double minutes = (ph.to() - ph.from()) / (double) GlassCorpusKit.TICKS_PER_MINUTE;
            Set<String> expected = new java.util.HashSet<>();
            for (CorpusFactory.Expect e : ph.expect()) {
                expected.add(e.good());
                double m = w.slopePerMinute(e.good(), ph.from(), ph.to());
                double ends = w.endsPerMinute(e.good(), ph.from(), ph.to());
                boolean ok;
                String tolText;
                if (e.kind() == CorpusFactory.Kind.EXACT) {
                    double sigma = Math.sqrt(1.2 * e.varPerMinute() / minutes);
                    double tol = Math.max(Math.max(REL_TOL * Math.abs(e.perMinute()), Z * sigma), ZERO_TOL);
                    ok = Math.abs(m - e.perMinute()) <= tol;
                    tolText = String.format(Locale.ROOT, "±%.3f (sigma %.3f)", tol, sigma);
                } else {
                    double tol = Math.max(REL_TOL * Math.max(Math.abs(e.lo()), Math.abs(e.hi())), ZERO_TOL);
                    ok = m >= e.lo() - tol && m <= e.hi() + tol;
                    tolText = String.format(Locale.ROOT, "[%.3f, %.3f] ±%.3f", e.lo(), e.hi(), tol);
                }
                if (e.perMinute() > 0 && m <= 0)
                    ok = false;
                MicRtsim.LOGGER.info(String.format(Locale.ROOT,
                        "[gametest] glass corpus %s %s: measured %.3f/min expected %.3f/min (%s), phase %s, tolerance %s, ends %.3f/min, %s",
                        f.name(), e.good(), m, e.perMinute(), e.kind().name().toLowerCase(Locale.ROOT), ph.name(), tolText, ends,
                        ok ? "pass" : "FAIL"));
                if (!ok)
                    failures.add(String.format(Locale.ROOT, "%s %s (%s): measured %.3f/min, expected %.3f/min %s",
                            f.name(), e.good(), ph.name(), m, e.perMinute(), tolText));
            }
            for (String g : w.moving(ph.from(), ph.to()))
                if (!expected.contains(g))
                    MicRtsim.LOGGER.info(String.format(Locale.ROOT,
                            "[gametest] glass corpus %s %s: measured %.3f/min, not in the expectations (phase %s)",
                            f.name(), g, w.slopePerMinute(g, ph.from(), ph.to()), ph.name()));
        }
        if (!failures.isEmpty())
            throw new GameTestAssertException(String.join("; ", failures));
    }
}
