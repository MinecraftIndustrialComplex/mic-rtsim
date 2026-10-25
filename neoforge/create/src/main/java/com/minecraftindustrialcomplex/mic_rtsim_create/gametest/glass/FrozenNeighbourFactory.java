package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassReport;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTestHelper;

/**
 * SEC-1 fixture (SECTIONS.md §9.2): two lines on a vault, the first made to oscillate. The fallback names the
 * first section, which freezes; the replay is rebuilt at the fallback's tick; the second goes on drawing from
 * the vault until it is empty; the district does not go to the black box.
 *
 * <p>The lines are {@link HopperAtASharedVaultFactory}'s without the hopper: the brass mixing line fills the
 * vault and the press line draws from it. Both movers at the vault are count points (§4.2), so the vault is
 * counted and the lines are two judging units (S3); {@link HopperAtASharedVaultFactory}, the same lines with an
 * uncounted hopper at the vault, keeps them one group (review M5: the two fixtures together show that the
 * hopper is what joins them). One of the table's sentences cannot be met, and is kept as checks that log
 * {@code GAP} (they never fail the test, and say what closes them): nothing makes a line oscillate.
 * {@code Fallback::Oscillating} fires when a replay takes more than {@code glass.breakpointsPerEventMax} (64)
 * breakpoints between outside events (tunables.rs), and no Java route reaches that tunable (the engine's own
 * tests set it to 0); the corpus' natural oscillations were fixed in the engine. The fixture can only show
 * that a healthy replay does not oscillate or freeze.
 * What does run: the corpus cycle on a shared vault (loaded for a window, away for a replay, back for a
 * reconcile that matches), with the replay model GLASS all the way through (never the black box), two units,
 * none frozen, and no oscillation.
 */
public final class FrozenNeighbourFactory implements CorpusFactory {
    private final HopperAtASharedVaultFactory lines = new HopperAtASharedVaultFactory(false, "frozen_neighbour");

    @Override
    public String name() {
        return "frozen_neighbour";
    }

    @Override
    public int warmupTicks() {
        return lines.warmupTicks();
    }

    @Override
    public Built build(GameTestHelper h) {
        return lines.build(h);
    }

    /** The checks of §9.2's row that can be made, and the oscillation that cannot. */
    static final class Check implements GlassReplayGameTests.Scenario {
        private boolean structure;

        @Override
        public void built(GlassReplayGameTests.Run run) {
            structure = false;
        }

        @Override
        public String ready(GlassReplayGameTests.Run run, GlassReport far) {
            if (far.units().size() < 2)
                return far.units().size() + " units of at least 2";
            for (GlassReport.Unit u : far.units())
                if (!SectionsFx.learned(u))
                    return "a unit has not learned: " + SectionsFx.describe(far);
            String gate = SectionsFx.replayGate(far, SectionsFx.unitAt(far, run.far.absolutePos(HopperAtASharedVaultFactory.MIXER)));
            if (gate != null)
                return gate;
            if (!structure) {
                structure = true;
                BlockPos mixer = run.far.absolutePos(HopperAtASharedVaultFactory.MIXER), press = run.far.absolutePos(HopperAtASharedVaultFactory.PRESS);
                GlassReport.Unit first = SectionsFx.unitAt(far, mixer), second = SectionsFx.unitAt(far, press);
                String d = SectionsFx.describe(far);
                SectionsFx.info(run, "far units", d);
                // Review M5: the vault's movers are both counted, so the vault joins no group and the lines are two units;
                // HopperAtASharedVault's uncounted hopper at the same vault keeps them one.
                SectionsFx.check(run, "the mixing line and the press line are two judging units (the vault is counted, so a freeze of the first spares the second)",
                        first != null && second != null && first != second, d);
                SectionsFx.check(run, "both lines have learned before the far copy leaves", SectionsFx.learned(first) && SectionsFx.learned(second), d);
            }
            return null;
        }

        @Override
        public void away(GlassReplayGameTests.Run run, GlassReport far, boolean last) {
            if (!last)
                return;
            GlassReport.Unit first = SectionsFx.unitAt(far, run.far.absolutePos(HopperAtASharedVaultFactory.MIXER));
            GlassReport.Unit second = SectionsFx.unitAt(far, run.far.absolutePos(HopperAtASharedVaultFactory.PRESS));
            String d = SectionsFx.describe(far) + "; " + far.replay().summary();
            SectionsFx.info(run, "units while away", d);
            SectionsFx.check(run, "the district stays on the glass box while away (never the black box)",
                    run.sawGlass && run.leftGlass == 0 && far.replay().model() == GlassReport.REPLAY_GLASS, far.replay().summary());
            SectionsFx.check(run, "a healthy replay does not oscillate and freezes nothing", far.replay().oscillations() == 0
                    && first != null && !first.frozen() && second != null && !second.frozen(), d);
            // TODO(engine): no Java route forces glass.breakpointsPerEventMax (tunables.rs, 64; the engine's tests set 0), so no section oscillates
            // here. Needs a tunable the bridge can set (a pack config or the GlassConfig.Test table) or a fixture that oscillates by itself.
            SectionsFx.gap(run, "an oscillation fallback names the first section and freezes it",
                    far.replay().oscillations() > 0 && first != null && first.frozen(),
                    "a way to make a replay oscillate from Java: the tunable glass.breakpointsPerEventMax is engine-side only", d);
            SectionsFx.gap(run, "the second line goes on drawing from the vault while the first is frozen (the replay rebuilt at the fallback's tick)",
                    first != null && first.frozen() && second != null && !second.frozen() && second != first,
                    "the oscillation above (the lines are two units, checked when ready)", d);
        }

        @Override
        public void judged(GlassReplayGameTests.Run run, GlassReport far, GlassReport control) {
            SectionsFx.info(run, "units after the reconcile", SectionsFx.describe(far));
            GlassReport.Unit first = SectionsFx.unitAt(far, run.far.absolutePos(HopperAtASharedVaultFactory.MIXER));
            SectionsFx.check(run, "neither line is frozen or stale after the reconcile", first != null && !first.frozen() && !first.stale(),
                    first == null ? "no unit" : first.summary());
        }
    }
}
