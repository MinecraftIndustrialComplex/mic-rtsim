package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.Counting;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassReport;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTestHelper;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.HopperAtASharedVaultFactory.A_BY;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.HopperAtASharedVaultFactory.FEED;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.HopperAtASharedVaultFactory.MIXER;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.HopperAtASharedVaultFactory.PRESS;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.HopperAtASharedVaultFactory.VAULT;

/**
 * SEC-1 fixture (SECTIONS.md §9.2, slice S3): a mixer's basin putting its output into a vault another line draws
 * from. The line is {@link BrassMixingFactory}'s, its basin's spout (facing east) pouring into the chute at the
 * diagonal below, which drops onto the vault; the second line is {@link HopperAtASharedVaultFactory}'s press
 * line under the vault (the same build as {@link VaultBetweenMixersFactory}, whose checks are the table row's:
 * two units, each passes, counted once).
 *
 * <p><b>A basin cannot pour into a vault itself (SC82).</b> {@code BasinBlock.canOutputTo} turns a basin's
 * spout sideways only for a belt, a funnel or a block with a {@code DirectBeltInputBehaviour} at the block
 * diagonally below (a chute, another basin, a depot's behaviour, a drill or saw, ...): no store has one, so a
 * vault or chest beside and below a basin leaves its spout facing down (it then puts goods into its own output
 * inventory). A basin does pour into a vault through a funnel facing up on it (SC120, counted at the funnel;
 * {@link BasinThroughFunnelFactory} is this line with the funnel where this fixture has the chute). Here the
 * basin's hop is to a chute, which the engine does not count (a chute is a mover, not a store), so the move
 * into the vault is counted at the chute ({@code ChuteBlockEntityMixin}).
 *
 * <p>What this fixture adds is the Java count point at the basin ({@code BasinBlockEntityMixin}, PATCHES.md),
 * tested on the real spout hop: it forces the pair (basin, spout chute) onto the far district
 * ({@code Counting.holdForTest}, which every later set keeps and which the engine skips when it reads a
 * sample), and checks over a stretch that what the basin counted is the recipe's output and is what the chute
 * then counted into the vault, within the chute's contents in flight.
 */
public final class BasinIntoASharedVaultFactory implements CorpusFactory {
    /** The basin of line A (BrassMixing's (8,3,8)) and the chute its spout pours into (its (9,2,8)). */
    static final BlockPos BASIN = new BlockPos(8, 3, 8).offset(A_BY);
    static final BlockPos SPOUT_CHUTE = new BlockPos(9, 2, 8).offset(A_BY);

    private final HopperAtASharedVaultFactory lines = new HopperAtASharedVaultFactory(false, "basin_vault");

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
        /** What the spout chute holds in flight between the basin's count and its own: a stack of 2 and one dropping. */
        static final int IN_FLIGHT = 4;

        private Phase phase = Phase.LEARN;
        private long phaseAt, nextRead;
        private long basin0, chute0;

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

        private static long chuteIn(GlassReplayGameTests.Run run, BlockPos chute, BlockPos store) {
            long[] k = IndustryKit.ind().counting().counters(run.fd, chute, store);
            return k == null ? -1 : k[1];
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
            BlockPos chute = run.far.absolutePos(SPOUT_CHUTE), vault = run.far.absolutePos(VAULT);
            BlockPos store = VaultBetweenMixersFactory.storeOf(run, vault);
            GlassReport.Unit first = SectionsFx.unitAt(fr, mixer), second = SectionsFx.unitAt(fr, press);
            String d = SectionsFx.describe(fr);
            SectionsFx.info(run, "far units", d);
            SectionsFx.check(run, "the mixer's line and the press line are two judging units, each passing (the vault is counted at the chute above it and the smart chute under it)",
                    first != null && second != null && first != second && SectionsFx.learned(first) && SectionsFx.learned(second)
                            && VaultBetweenMixersFactory.hasMover(run.fd, chute) && VaultBetweenMixersFactory.hasMover(run.fd, run.far.absolutePos(FEED)),
                    d + "; " + VaultBetweenMixersFactory.pairsText(run.fd));
            SectionsFx.check(run, "the vault has a canonical inventory to count at", store != null, store);
            if (store == null) {
                go(Phase.DONE, now);
                return;
            }
            // The engine names no pair at the basin (its hop is to a chute): force the one the count point reports.
            SectionsFx.info(run, "the engine's pair at the basin", String.valueOf(VaultBetweenMixersFactory.hasMover(run.fd, basin)));
            Counting c = IndustryKit.ind().counting();
            c.holdForTest(run.fd, run.level().dimension(), basin, chute, now);
            basin0 = 0;
            chute0 = chuteIn(run, chute, store);
            SectionsFx.info(run, "forced for the test", "(basin, spout chute) " + SectionsFx.pos(basin) + " to " + SectionsFx.pos(chute) + "; "
                    + VaultBetweenMixersFactory.pairsText(run.fd));
            go(Phase.COUNT, now);
        }

        private void count(GlassReplayGameTests.Run run, long now) {
            if (now - phaseAt < COUNT_TICKS)
                return;
            BlockPos basin = run.far.absolutePos(BASIN), chute = run.far.absolutePos(SPOUT_CHUTE), vault = run.far.absolutePos(VAULT);
            BlockPos store = VaultBetweenMixersFactory.storeOf(run, vault);
            Counting c = IndustryKit.ind().counting();
            long[] k = c.counters(run.fd, basin, chute);
            long in = k == null ? -1 : k[1] - basin0, onward = chuteIn(run, chute, store) - chute0;
            SectionsFx.info(run, "counts over " + (now - phaseAt) + " ticks", "the basin counted in " + in + ", the chute counted into the vault " + onward
                    + "; " + VaultBetweenMixersFactory.pairsText(run.fd));
            SectionsFx.check(run, "the forced pair survives the engine's sets", k != null, VaultBetweenMixersFactory.pairsText(run.fd));
            SectionsFx.check(run, "the basin's pair counted its spout's output (one craft per 32 ticks makes 2 brass)",
                    k != null && in >= MIN_IN && in <= MAX_IN, "in " + in + " (expected " + MIN_IN + " to " + MAX_IN + ")");
            SectionsFx.check(run, "...and the chute counted the same goods on into the vault, less what it holds in flight (each real move once)",
                    k != null && onward >= 0 && Math.abs(in - onward) <= IN_FLIGHT, "basin in " + in + ", chute into the vault " + onward);
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
