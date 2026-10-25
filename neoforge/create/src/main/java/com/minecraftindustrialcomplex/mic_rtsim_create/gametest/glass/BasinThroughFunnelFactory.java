package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassReport;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.logistics.funnel.FunnelBlock;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.BasinIntoASharedVaultFactory.SPOUT_CHUTE;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.set;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.HopperAtASharedVaultFactory.FEED;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.HopperAtASharedVaultFactory.MIXER;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.HopperAtASharedVaultFactory.PRESS;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.HopperAtASharedVaultFactory.VAULT;

/**
 * SEC-1 fixture (SECTIONS.md §9.2, slice S3, review B1): a mixer's basin pouring through a funnel into a vault
 * another line draws from. The lines are {@link BasinIntoASharedVaultFactory}'s (the mixing line over the vault,
 * the press line under it) with one change: the block at the basin's spout, which is a chute there, is an
 * andesite funnel facing up on the vault, in insert mode.
 *
 * <p>A funnel with nothing in front of it takes what a basin pours ({@code BasinBlock.canOutputTo}: a funnel
 * facing up has a {@code DirectBeltInputBehaviour}). {@code BasinBlockEntity.tryClearingSpoutputOverflow}
 * finds no item handler at the funnel and inserts into the funnel's own inventory (the vault behind it) with
 * {@code insertItemStacked}, which skips {@code InvManipulationBehaviour.insert}: the funnel's count point
 * never sees the move, and the basin's count point ({@code BasinBlockEntityMixin}) counts it under the funnel
 * and the vault, the pair the engine names from the funnel's adjacency edge.
 *
 * <p>The checks: two units and a bystander, as in the other vault fixtures; the engine names a pair at the
 * funnel and Java holds it; over a stretch the vault's stock changes by exactly what the pairs counted in less
 * what they counted out, and what went in is the funnel's, which is the recipe's output (one craft per 32 ticks
 * makes 2 brass), so a basin pour that went uncounted would show as the stock rising with nothing counted in
 * and the drawing line's residue growing in the engine.
 */
public final class BasinThroughFunnelFactory implements CorpusFactory {
    private final HopperAtASharedVaultFactory lines = new HopperAtASharedVaultFactory(false, "basin_funnel_vault");

    @Override
    public String name() {
        return "basin_funnel_vault";
    }

    @Override
    public int warmupTicks() {
        return lines.warmupTicks();
    }

    @Override
    public Built build(GameTestHelper h) {
        Built b = lines.build(h);
        BlockPos funnel = GlassCorpusKit.at(h, SPOUT_CHUTE.getX(), SPOUT_CHUTE.getY(), SPOUT_CHUTE.getZ());
        set(h, funnel, AllBlocks.ANDESITE_FUNNEL.getDefaultState().setValue(BlockStateProperties.FACING, Direction.UP).setValue(FunnelBlock.EXTRACTING, false));
        return b;
    }

    static final class Check implements GlassReplayGameTests.Scenario {
        enum Phase { LEARN, COUNT, DONE }

        static final int READ = 10;
        static final int COUNT_TICKS = 600, LEARN_DEADLINE = 9000;
        /** One craft per 32 ticks makes 2 brass ({@link BrassMixingFactory}): 37.5 in {@link #COUNT_TICKS}; bounds for the start. */
        static final int MIN_IN = 30, MAX_IN = 45;

        private Phase phase = Phase.LEARN;
        private long phaseAt, nextRead;
        private long stock0, funnelIn0;
        private long[] all0 = {0, 0};

        @Override
        public int extraTicks() {
            return 4000;
        }

        @Override
        public void built(GlassReplayGameTests.Run run) {
            phase = Phase.LEARN;
            phaseAt = run.now();
            nextRead = 0;
        }

        @Override
        public String ready(GlassReplayGameTests.Run run, GlassReport far) {
            return phase == Phase.DONE ? null : "basin_funnel_vault in phase " + phase;
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

        private static long funnelIn(GlassReplayGameTests.Run run, BlockPos funnel, BlockPos store) {
            long[] k = IndustryKit.ind().counting().counters(run.fd, funnel, store);
            return k == null ? -1 : k[1];
        }

        private void learn(GlassReplayGameTests.Run run, GlassReport fr, long now) {
            BlockPos funnel = run.far.absolutePos(SPOUT_CHUTE), vault = run.far.absolutePos(VAULT);
            if (now - phaseAt > LEARN_DEADLINE) {
                SectionsFx.check(run, "the lines' units learn and the engine names a pair at the funnel before the scenario's deadline", false,
                        SectionsFx.describe(fr) + "; " + VaultBetweenMixersFactory.pairsText(run.fd));
                go(Phase.DONE, now);
                return;
            }
            if (fr.units().size() < 2)
                return;
            for (GlassReport.Unit u : fr.units())
                if (!SectionsFx.learned(u))
                    return;
            if (!VaultBetweenMixersFactory.hasMover(run.fd, funnel))
                return;
            BlockPos mixer = run.far.absolutePos(MIXER), press = run.far.absolutePos(PRESS);
            BlockPos store = VaultBetweenMixersFactory.storeOf(run, vault);
            GlassReport.Unit first = SectionsFx.unitAt(fr, mixer), second = SectionsFx.unitAt(fr, press);
            String d = SectionsFx.describe(fr);
            SectionsFx.info(run, "far units", d);
            SectionsFx.check(run, "the mixer's line and the press line are two judging units, each passing (the vault is counted at the funnel above it and the smart chute under it)",
                    first != null && second != null && first != second && SectionsFx.learned(first) && SectionsFx.learned(second)
                            && VaultBetweenMixersFactory.hasMover(run.fd, run.far.absolutePos(FEED)),
                    d + "; " + VaultBetweenMixersFactory.pairsText(run.fd));
            SectionsFx.check(run, "the vault has a canonical inventory to count at", store != null, store);
            if (store == null) {
                go(Phase.DONE, now);
                return;
            }
            VaultBetweenMixersFactory.counted(run, VAULT, 2);
            SectionsFx.check(run, "the engine's pair for the basin's pour is (the funnel, the vault)",
                    IndustryKit.ind().counting().holds(run.fd, funnel, store), VaultBetweenMixersFactory.pairsText(run.fd));
            stock0 = VaultBetweenMixersFactory.total(VaultBetweenMixersFactory.handler(run, vault));
            all0 = VaultBetweenMixersFactory.moved(run.fd);
            funnelIn0 = funnelIn(run, funnel, store);
            go(Phase.COUNT, now);
        }

        private void count(GlassReplayGameTests.Run run, long now) {
            if (now - phaseAt < COUNT_TICKS)
                return;
            BlockPos funnel = run.far.absolutePos(SPOUT_CHUTE), vault = run.far.absolutePos(VAULT);
            BlockPos store = VaultBetweenMixersFactory.storeOf(run, vault);
            long stock = VaultBetweenMixersFactory.total(VaultBetweenMixersFactory.handler(run, vault));
            long[] m = VaultBetweenMixersFactory.moved(run.fd);
            long ds = stock - stock0, in = m[0] - all0[0], out = m[1] - all0[1];
            long viaFunnel = funnelIn(run, funnel, store) - funnelIn0;
            SectionsFx.info(run, "counts over " + (now - phaseAt) + " ticks", "the vault's stock " + stock0 + " to " + stock + ", counted in " + in + " (the funnel's "
                    + viaFunnel + "), out " + out + "; " + VaultBetweenMixersFactory.pairsText(run.fd));
            SectionsFx.check(run, "the funnel's pair counted the basin's pour (one craft per 32 ticks makes 2 brass)",
                    viaFunnel >= MIN_IN && viaFunnel <= MAX_IN, "in " + viaFunnel + " (expected " + MIN_IN + " to " + MAX_IN + ")");
            SectionsFx.check(run, "the funnel's pair is all that went into the vault (the funnel's own count point and the basin's are not both counting)",
                    in == viaFunnel, "in " + in + ", the funnel's " + viaFunnel);
            SectionsFx.check(run, "every move at the vault is counted once: the stock changed by what was counted in less what was counted out",
                    out > 0 && ds == in - out, "stock " + (ds >= 0 ? "+" : "") + ds + ", counted " + (in - out) + " (in " + in + ", out " + out + ")");
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
