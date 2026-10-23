package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassReport;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTestHelper;

/**
 * SEC-1 fixture (SECTIONS.md §9.2): two lines in one district, one of them in chunks that are out of
 * simulation distance. Line A is {@link SawShaftsFactory} (the anchor's chunk keeps ticking), line B is
 * {@link MillstoneFlourFactory} {@value #B_DX} blocks along x, in the chunk next to it that loses its tickets
 * (loaded and FULL, not block-ticking: the partial variant of {@link GlassReplayGameTests}, which puts a
 * chunk border between the lines). Run as that variant.
 *
 * <p>Both lines learn while every chunk ticks, as they would in a district whose chunks all tick; then B's
 * chunk stops ticking. While the district is partial A is live, so its unit goes on judging windows, and B's
 * unit, which passed, replays (its non-ticking part is the glass box's to simulate, I7). The reconcile when
 * B's chunk ticks again matches (the corpus cycle's judging). SECTIONS.md §9.2 says "the ticking line learns
 * and passes; the other replays if passed": the first half is A's window while away, the second is B.
 */
public final class HalfLoadedFactory implements CorpusFactory {
    static final int B_DX = 14;
    static final BlockPos A_SAW = new BlockPos(8, 3, 8), B_MILL = new BlockPos(8 + B_DX, 3, 8);

    @Override
    public String name() {
        return "half_loaded";
    }

    @Override
    public int warmupTicks() {
        return 300;
    }

    @Override
    public Built build(GameTestHelper h) {
        Built a = new SawShaftsFactory().build(h);
        Built b = new MillstoneFlourFactory().build(new SectionsFx.Shifted(h, new BlockPos(B_DX, 0, 0)));
        return SectionsFx.merge(7200, "line A saw shafts, line B millstone flour " + B_DX + " along x; " + a.notes() + "; " + b.notes(), a, b);
    }

    /** The checks of §9.2's row. */
    static final class Check implements GlassReplayGameTests.Scenario {
        private boolean structure;
        private GlassReport.Unit aFirst, bFirst;

        @Override
        public void built(GlassReplayGameTests.Run run) {
            structure = false;
            aFirst = bFirst = null;
        }

        @Override
        public String ready(GlassReplayGameTests.Run run, GlassReport far) {
            if (far.units().size() < 2)
                return far.units().size() + " units of 2";
            for (GlassReport.Unit u : far.units())
                if (!SectionsFx.learned(u))
                    return "a unit has not learned: " + SectionsFx.describe(far);
            if (!structure) {
                structure = true;
                GlassReport.Unit a = SectionsFx.unitAt(far, run.far.absolutePos(A_SAW)), b = SectionsFx.unitAt(far, run.far.absolutePos(B_MILL));
                String d = SectionsFx.describe(far);
                SectionsFx.info(run, "far units", d);
                SectionsFx.check(run, "two units, one per line", far.units().size() == 2 && a != null && b != null && a != b, d);
                SectionsFx.check(run, "both lines learned while every chunk ticked", SectionsFx.learned(a) && SectionsFx.learned(b), d);
            }
            return null;
        }

        @Override
        public void away(GlassReplayGameTests.Run run, GlassReport far, boolean last) {
            GlassReport.Unit a = SectionsFx.unitAt(far, run.far.absolutePos(A_SAW)), b = SectionsFx.unitAt(far, run.far.absolutePos(B_MILL));
            if (aFirst == null && a != null) {
                aFirst = a;
                bFirst = b;
                SectionsFx.info(run, "first units while partial", SectionsFx.describe(far));
            }
            if (!last)
                return;
            SectionsFx.info(run, "units while partial", SectionsFx.describe(far));
            SectionsFx.check(run, "the ticking line's unit is not frozen and still has its pass", a != null && a.passed() && !a.frozen(),
                    a == null ? "no unit" : a.summary());
            SectionsFx.check(run, "the ticking line goes on judging windows (a window open or one more done)",
                    a != null && aFirst != null && (a.windowTicks() > 0 || a.windowsCompleted() > aFirst.windowsCompleted()),
                    a == null ? "no unit" : "now " + a.summary() + "; first " + (aFirst == null ? "none" : aFirst.summary()));
            SectionsFx.check(run, "the other line, which passed, replays", b != null && b.mayReplay() && !b.frozen() && !b.stale(),
                    b == null ? "no unit" : b.summary());
        }

        @Override
        public void judged(GlassReplayGameTests.Run run, GlassReport far, GlassReport control) {
            SectionsFx.info(run, "units after the reconcile", SectionsFx.describe(far));
            GlassReport.Unit b = SectionsFx.unitAt(far, run.far.absolutePos(B_MILL));
            SectionsFx.check(run, "the replayed line is neither frozen nor stale after the reconcile", b != null && !b.frozen() && !b.stale(),
                    b == null ? "no unit" : b.summary());
        }
    }
}
