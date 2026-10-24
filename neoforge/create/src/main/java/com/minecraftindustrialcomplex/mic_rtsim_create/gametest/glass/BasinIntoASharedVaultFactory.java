package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.Counting;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassReport;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTestHelper;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.HopperAtASharedVaultFactory.A_BY_BASIN;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.HopperAtASharedVaultFactory.FEED;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.HopperAtASharedVaultFactory.PRESS;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.HopperAtASharedVaultFactory.VAULT;

/**
 * SEC-1 fixture (SECTIONS.md §9.2, slice S3): a mixer's basin putting its output straight into a vault that
 * another line draws from. {@link BrassMixingFactory}'s line moved one lower, so its basin's spout (facing east)
 * pours into the vault's top layer with no chute between ({@code BasinBlockEntity.tryClearingSpoutputOverflow}
 * inserts into the inventory at {@code below().relative(FACING)}); the second line is
 * {@link HopperAtASharedVaultFactory}'s press line under the vault. The basin's output is the move Java counts
 * at the basin ({@code BasinBlockEntityMixin}, PATCHES.md).
 *
 * <p><b>The engine does not count a mixer's basin (SC82).</b> A mixer or a press over a basin compiles to a MIXER
 * or PRESS node at the basin's position ({@code CreateCompile.basinNode}); {@code COUNTED_MODELS} names only
 * BASIN, the node of a basin with no machine over it, so the vault has a goods edge from an uncounted model, is
 * not countable, and the two lines are one judging group at it (as with a hopper). The table's row (two sections,
 * each passes) is then a gap check, and the Java count point at the basin is tested on its own: the fixture holds
 * the (basin, vault) and (smart chute, vault) pairs for the far district itself
 * ({@code Counting.holdForTest}, which the engine skips when it reads a sample), and compares over a stretch what
 * the vault's stock changed by with what the two counted. The Java side counts the basin whichever model the
 * engine gives the block, so adding MIXER and PRESS to {@code COUNTED_MODELS} closes the gap with no Java change,
 * and then the engine's own pairs are the ones checked.
 */
public final class BasinIntoASharedVaultFactory implements CorpusFactory {
    /** The basin of line A: BrassMixing's (8,3,8) moved by {@link HopperAtASharedVaultFactory#A_BY_BASIN}. */
    static final BlockPos BASIN = new BlockPos(8, 3, 8).offset(A_BY_BASIN);
    static final BlockPos MIXER = BASIN.above(2);

    private final HopperAtASharedVaultFactory lines = new HopperAtASharedVaultFactory(false, "basin_vault", true);

    @Override
    public String name() {
        return "basin_vault";
    }

    @Override
    public int warmupTicks() {
        return lines.warmupTicks();
    }

    @Override
    public Built build(GameTestHelper h) {
        return lines.build(h);
    }

    static final class Check implements GlassReplayGameTests.Scenario {
        enum Phase { LEARN, COUNT, DONE }

        static final int READ = 10;
        static final int COUNT_TICKS = 600, LEARN_DEADLINE = 9000;
        /** One craft per 32 ticks makes 2 brass ({@link BrassMixingFactory}): 37.5 in {@link #COUNT_TICKS}; bounds for the start. */
        static final int MIN_IN = 30, MAX_IN = 45;

        private Phase phase = Phase.LEARN;
        private long phaseAt, nextRead;
        private long stock0, in0, out0;
        private boolean engineCounts;

        @Override
        public int extraTicks() {
            return 4000;
        }

        @Override
        public void built(GlassReplayGameTests.Run run) {
            phase = Phase.LEARN;
            phaseAt = run.now();
            nextRead = 0;
            engineCounts = false;
        }

        @Override
        public String ready(GlassReplayGameTests.Run run, GlassReport far) {
            return phase == Phase.DONE ? null : "basin_vault in phase " + phase;
        }

        private void go(Phase next, long now) {
            phase = next;
            phaseAt = now;
        }

        @Override
        public void tick(GlassReplayGameTests.Run run) {
            if (phase == Phase.DONE)
                return;
            long now = run.now();
            if (now < nextRead)
                return;
            nextRead = now + READ;
            GlassReport fr = GlassReplayGameTests.report(run, run.fd);
            if (fr == null)
                return;
            if (phase == Phase.LEARN)
                learn(run, fr, now);
            else
                count(run, now);
        }

        private void learn(GlassReplayGameTests.Run run, GlassReport fr, long now) {
            if (now - phaseAt > LEARN_DEADLINE) {
                SectionsFx.check(run, "the lines' units learn before the scenario's deadline", false, SectionsFx.describe(fr));
                go(Phase.DONE, now);
                return;
            }
            if (fr.units().size() < 2)
                return;
            for (GlassReport.Unit u : fr.units())
                if (!SectionsFx.learned(u))
                    return;
            BlockPos mixer = run.far.absolutePos(MIXER), press = run.far.absolutePos(PRESS), basin = run.far.absolutePos(BASIN);
            GlassReport.Unit first = SectionsFx.unitAt(fr, mixer), second = SectionsFx.unitAt(fr, press);
            String d = SectionsFx.describe(fr);
            SectionsFx.info(run, "far units", d);
            SectionsFx.check(run, "the mixer's line and the press line each learn (a window passed)",
                    SectionsFx.learned(first) && SectionsFx.learned(second), d);
            engineCounts = VaultBetweenMixersFactory.hasMover(run.fd, basin);
            // TODO(engine): COUNTED_MODELS (glass/sections.rs) names BASIN only; a basin under a mixer or press is a MIXER or PRESS node
            // at the basin's position, so a vault it fills is not countable and the lines are one group (SC82). Adding MIXER and PRESS
            // there makes this turn ok, with no Java change (the count point is at the basin block, whatever its model).
            SectionsFx.gap(run, "the basin's output is counted and the two lines are two judging units, each passing",
                    engineCounts && first != null && second != null && first != second,
                    "S3: COUNTED_MODELS lacks MIXER and PRESS, the models of a basin with a machine over it (SC82)", d + "; " + VaultBetweenMixersFactory.pairsText(run.fd));
            // The Java count point, with the pairs held here when the engine named none: the basin's output and the smart chute's draw.
            Counting c = IndustryKit.ind().counting();
            BlockPos vault = run.far.absolutePos(VAULT), store = VaultBetweenMixersFactory.storeOf(run, vault);
            SectionsFx.check(run, "the vault has a canonical inventory to count at", store != null, store);
            if (store == null) {
                go(Phase.DONE, now);
                return;
            }
            if (!engineCounts) {
                c.holdForTest(run.fd, run.level().dimension(), basin, store, now);
                c.holdForTest(run.fd, run.level().dimension(), run.far.absolutePos(FEED), store, now);
                SectionsFx.info(run, "the engine named no pair at the basin", "held for the test: " + VaultBetweenMixersFactory.pairsText(run.fd));
            }
            stock0 = VaultBetweenMixersFactory.total(VaultBetweenMixersFactory.handler(run, vault));
            long[] m = VaultBetweenMixersFactory.moved(run.fd);
            in0 = m[0];
            out0 = m[1];
            go(Phase.COUNT, now);
        }

        private void count(GlassReplayGameTests.Run run, long now) {
            if (now - phaseAt < COUNT_TICKS)
                return;
            BlockPos basin = run.far.absolutePos(BASIN), vault = run.far.absolutePos(VAULT);
            Counting c = IndustryKit.ind().counting();
            long stock = VaultBetweenMixersFactory.total(VaultBetweenMixersFactory.handler(run, vault));
            long[] m = VaultBetweenMixersFactory.moved(run.fd);
            long ds = stock - stock0, in = m[0] - in0, out = m[1] - out0;
            BlockPos store = VaultBetweenMixersFactory.storeOf(run, vault);
            long[] k = c.counters(run.fd, basin, store);
            SectionsFx.info(run, "counts over " + (now - phaseAt) + " ticks", "the vault's stock " + stock0 + " to " + stock + ", counted in " + in
                    + ", out " + out + "; " + VaultBetweenMixersFactory.pairsText(run.fd));
            SectionsFx.check(run, "the basin's pair counted its spout's output (one craft per 32 ticks makes 2 brass)",
                    k != null && in >= MIN_IN && in <= MAX_IN, "in " + in + " (expected " + MIN_IN + " to " + MAX_IN + ")");
            SectionsFx.check(run, "every move at the vault is counted once: the stock changed by what was counted in less what was counted out",
                    ds == in - out, "stock " + (ds >= 0 ? "+" : "") + ds + ", counted " + (in - out) + " (in " + in + ", out " + out + ")");
            go(Phase.DONE, now);
        }

        @Override
        public void judged(GlassReplayGameTests.Run run, GlassReport far, GlassReport control) {
            SectionsFx.info(run, "units after the reconcile", SectionsFx.describe(far));
            GlassReport.Unit first = SectionsFx.unitAt(far, run.far.absolutePos(MIXER)), second = SectionsFx.unitAt(far, run.far.absolutePos(PRESS));
            SectionsFx.check(run, "neither line is frozen or stale after the reconcile", first != null && !first.frozen() && !first.stale()
                    && second != null && !second.frozen() && !second.stale(), SectionsFx.describe(far));
        }
    }
}
