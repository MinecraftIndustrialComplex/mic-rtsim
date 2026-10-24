package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Districts;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.Counting;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassReport;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.kinetics.press.MechanicalPressBlock;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.level.block.Blocks;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.level.BlockEvent;
import net.neoforged.neoforge.items.IItemHandler;

import java.util.ArrayList;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.HopperAtASharedVaultFactory.FEED;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.HopperAtASharedVaultFactory.MIXER;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.HopperAtASharedVaultFactory.PRESS;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.HopperAtASharedVaultFactory.VAULT;

/**
 * SEC-1 fixture (SECTIONS.md §9.2, slice S3): a vault between two lines, one filling it and one drawing from
 * it, with nothing else at it. The vault's movers are all counted ones (line A's chute puts brass into it, line
 * B's smart chute takes it out), so the engine counts the vault at them and the lines are two judging units,
 * not the one group a store shared by uncounted movers makes ({@link HopperAtASharedVaultFactory} adds the
 * hopper that makes it).
 *
 * <p>The lines are {@link HopperAtASharedVaultFactory}'s without the hopper (its layout is drawn there).
 * Differs from the table: the second line presses the brass instead of mixing it (no mixing recipe uses
 * brass), and both lines move by chutes, not funnels (SC80): the funnel's count point is not exercised here.
 *
 * <p>The checks: two units and a bystander; Java took the engine's count set and holds pairs at the vault and
 * sends samples; the counts are exact (over a stretch, the vault's stock changes by what the movers counted in
 * less what they counted out, so no move is missed and none counted twice); an edit that breaks and replaces
 * the second line's press stales that unit only, the first keeps its passed window, and the second learns
 * again; while away the first replays and nothing counts.
 */
public final class VaultBetweenMixersFactory implements CorpusFactory {
    private final HopperAtASharedVaultFactory lines = new HopperAtASharedVaultFactory(false, "vault_mixers");

    @Override
    public String name() {
        return "vault_mixers";
    }

    @Override
    public int warmupTicks() {
        return lines.warmupTicks();
    }

    @Override
    public Built build(GameTestHelper h) {
        return lines.build(h);
    }

    // ---- shared with the other S3 fixtures ---------------------------------------------------------

    /** The item handler at {@code abs} (the vault's controller answers for every block of it). */
    static IItemHandler handler(GlassReplayGameTests.Run run, BlockPos abs) {
        return run.level().getCapability(Capabilities.ItemHandler.BLOCK, abs, null);
    }

    static long total(IItemHandler inv) {
        long n = 0;
        for (int i = 0; inv != null && i < inv.getSlots(); i++)
            n += inv.getStackInSlot(i).getCount();
        return n;
    }

    /** {@code {in, out}}: every pair of the district's counters, all goods summed. */
    static long[] moved(long district) {
        Counting c = IndustryKit.ind().counting();
        long in = 0, out = 0;
        for (BlockPos[] p : c.pairs(district)) {
            long[] k = c.counters(district, p[0], p[1]);
            if (k != null) {
                in += k[1];
                out += k[2];
            }
        }
        return new long[] {in, out};
    }

    /** The district's pairs and what each has counted, one line. */
    static String pairsText(long district) {
        Counting c = IndustryKit.ind().counting();
        List<String> out = new ArrayList<>();
        for (BlockPos[] p : c.pairs(district)) {
            long[] k = c.counters(district, p[0], p[1]);
            out.add(SectionsFx.pos(p[0]) + " to " + SectionsFx.pos(p[1]) + (k == null ? " (gone)" : " in " + k[1] + " out " + k[2] + " since " + k[0]));
        }
        return out.isEmpty() ? "no pairs" : String.join("; ", out);
    }

    /** The canonical inventory position of the block at {@code abs} (what a count's store is keyed by). */
    static BlockPos storeOf(GlassReplayGameTests.Run run, BlockPos abs) {
        return Districts.canonicalInventory(run.level(), abs);
    }

    /** Whether every pair of the district names the store (and there are at least {@code least} pairs). */
    static boolean allAt(long district, BlockPos store, int least) {
        List<BlockPos[]> ps = IndustryKit.ind().counting().pairs(district);
        if (ps.size() < least)
            return false;
        for (BlockPos[] p : ps)
            if (!p[1].equals(store))
                return false;
        return true;
    }

    /** Whether the district holds a pair whose mover is at {@code mover}. */
    static boolean hasMover(long district, BlockPos mover) {
        for (BlockPos[] p : IndustryKit.ind().counting().pairs(district))
            if (p[0].equals(mover))
                return true;
        return false;
    }

    /** The checks of one count set's reach: Java holds the pairs for both copies and sends samples. */
    static void counted(GlassReplayGameTests.Run run, BlockPos vault, int pairs) {
        Counting c = IndustryKit.ind().counting();
        BlockPos farStore = storeOf(run, run.far.absolutePos(vault)), controlStore = storeOf(run, run.helper.absolutePos(vault));
        SectionsFx.info(run, "far pairs", pairsText(run.fd));
        SectionsFx.info(run, "control pairs", pairsText(run.cd));
        SectionsFx.check(run, "Java took the engine's count sets (the far copy holds " + pairs + " pairs, all at the vault)",
                c.setsTaken() > 0 && farStore != null && allAt(run.fd, farStore, pairs), pairsText(run.fd) + "; sets taken " + c.setsTaken()
                        + ", ignored " + c.setsIgnored());
        SectionsFx.check(run, "...and the control copy, at its own vault", controlStore != null && allAt(run.cd, controlStore, pairs), pairsText(run.cd));
        SectionsFx.check(run, "Java sends the counts in samples before the sweeps' ends", c.samplesSent() > 0, c.samplesSent() + " samples sent");
    }

    // ---- the checks ----------------------------------------------------------------------------------

    static final class Check implements GlassReplayGameTests.Scenario {
        enum Phase { LEARN, COUNT, REPLACE, RECOMPILE, RELEARN, DONE }

        static final int READ = 10;
        /** The stretch over which the count is compared with the vault's stock. */
        static final int COUNT_TICKS = 600;
        /** Ticks between the break and the placing of the press. */
        static final int GAP = 20;
        static final int SETTLE_AFTER_COMPILE = 40, COMPILE_DEADLINE = 1500;
        static final int LEARN_DEADLINE = 9000, RELEARN_DEADLINE = 9000;

        private Phase phase = Phase.LEARN;
        private long phaseAt, nextRead, acceptedBefore, settleAt;
        private boolean placed, firstStaled;
        private String firstStaledNote = "";
        private long stock0, in0, out0;
        private long[] awayCounted;

        @Override
        public int extraTicks() {
            return 8000;
        }

        @Override
        public void built(GlassReplayGameTests.Run run) {
            phase = Phase.LEARN;
            phaseAt = run.now();
            nextRead = 0;
            placed = false;
            firstStaled = false;
            firstStaledNote = "";
            awayCounted = null;
        }

        @Override
        public String ready(GlassReplayGameTests.Run run, GlassReport far) {
            return phase == Phase.DONE ? null : "vault_mixers in phase " + phase;
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
            GlassReport.Unit first = SectionsFx.unitAt(fr, run.far.absolutePos(MIXER)), second = SectionsFx.unitAt(fr, run.far.absolutePos(PRESS));
            if (phase != Phase.LEARN && phase != Phase.COUNT && first != null && (first.stale() || first.frozen()) && !firstStaled) {
                firstStaled = true;
                firstStaledNote = "at " + now + " in " + phase + ": " + first.summary();
            }
            switch (phase) {
                case LEARN -> learn(run, fr, now);
                case COUNT -> count(run, fr, now);
                case REPLACE -> replace(run, fr, now);
                case RECOMPILE -> recompile(run, fr, first, second, now);
                case RELEARN -> relearn(run, fr, first, second, now);
                default -> { }
            }
        }

        // ---- LEARN: two units, both passed; the structure ----------------------------------------------

        private void learn(GlassReplayGameTests.Run run, GlassReport fr, long now) {
            if (now - phaseAt > LEARN_DEADLINE) {
                SectionsFx.check(run, "the lines' units learn before the scenario's deadline", false, SectionsFx.describe(fr) + "; " + pairsText(run.fd));
                go(Phase.DONE, now);
                return;
            }
            if (fr.units().size() < 2)
                return;
            for (GlassReport.Unit u : fr.units())
                if (!SectionsFx.learned(u))
                    return;
            structure(run, fr);
            stock0 = total(handler(run, run.far.absolutePos(VAULT)));
            long[] m = moved(run.fd);
            in0 = m[0];
            out0 = m[1];
            go(Phase.COUNT, now);
        }

        private void structure(GlassReplayGameTests.Run run, GlassReport fr) {
            BlockPos mixer = run.far.absolutePos(MIXER), press = run.far.absolutePos(PRESS), vault = run.far.absolutePos(VAULT);
            GlassReport.Unit first = SectionsFx.unitAt(fr, mixer), second = SectionsFx.unitAt(fr, press);
            GlassReport.Unit bystander = SectionsFx.unitAt(fr, run.far.absolutePos(SectionsFx.BYSTANDER_FEED));
            String d = SectionsFx.describe(fr);
            SectionsFx.info(run, "far units", d);
            SectionsFx.check(run, "the mixing line and the press line are two judging units (the vault is counted, so it joins no group)",
                    first != null && second != null && first != second, d);
            SectionsFx.check(run, "the bystander is a unit of its own", bystander != null && bystander != first && bystander != second && bystander.sections() == 1, d);
            SectionsFx.check(run, "both lines have learned (a window passed each, apart)", SectionsFx.learned(first) && SectionsFx.learned(second), d);
            // The vault's box: each unit's own (the units' boxes may overlap at the vault).
            SectionsFx.info(run, "the vault in the boxes", "first " + (first != null && first.holds(vault.getX(), vault.getY(), vault.getZ()))
                    + ", second " + (second != null && second.holds(vault.getX(), vault.getY(), vault.getZ())));
            counted(run, VAULT, 2);
            BlockPos chute = run.far.absolutePos(new BlockPos(VAULT.getX(), VAULT.getY() + 1, VAULT.getZ()));
            SectionsFx.check(run, "the pairs' movers are the filling chute above the vault and the smart chute under it",
                    hasMover(run.fd, chute) && hasMover(run.fd, run.far.absolutePos(FEED)), pairsText(run.fd));
        }

        // ---- COUNT: no move missed, none counted twice -----------------------------------------------

        private void count(GlassReplayGameTests.Run run, GlassReport fr, long now) {
            if (now - phaseAt < COUNT_TICKS)
                return;
            long stock = total(handler(run, run.far.absolutePos(VAULT)));
            long[] m = moved(run.fd);
            long ds = stock - stock0, in = m[0] - in0, out = m[1] - out0;
            SectionsFx.info(run, "counts over " + (now - phaseAt) + " ticks", "the vault's stock " + stock0 + " to " + stock + ", counted in " + in + ", out " + out);
            SectionsFx.check(run, "both movers counted (the chute put brass in, the smart chute took it out)", in > 0 && out > 0, "in " + in + ", out " + out);
            SectionsFx.check(run, "every move at the vault is counted once: the stock changed by what was counted in less what was counted out",
                    ds == in - out, "stock " + (ds >= 0 ? "+" : "") + ds + ", counted " + (in - out) + " (in " + in + ", out " + out + ")");
            // The first edit: break the second line's press, and place it again.
            acceptedBefore = fr.checksum().acceptedAt();
            BlockPos[] abs = {run.helper.absolutePos(PRESS), run.far.absolutePos(PRESS)};
            SectionsFx.info(run, "edit", "break the press at " + SectionsFx.pos(abs[1]) + ", place it " + GAP + " ticks later; compile accepted at " + acceptedBefore);
            for (BlockPos p : abs) {
                NeoForge.EVENT_BUS.post(new BlockEvent.BreakEvent(run.level(), p, run.level().getBlockState(p), FakePlayers.editor(run.level())));
                run.level().setBlock(p, Blocks.AIR.defaultBlockState(), 3);
            }
            placed = false;
            go(Phase.REPLACE, now);
        }

        private void replace(GlassReplayGameTests.Run run, GlassReport fr, long now) {
            if (now - phaseAt < GAP)
                return;
            for (BlockPos p : new BlockPos[] {run.helper.absolutePos(PRESS), run.far.absolutePos(PRESS)}) {
                run.level().setBlock(p, AllBlocks.MECHANICAL_PRESS.getDefaultState().setValue(MechanicalPressBlock.HORIZONTAL_FACING, Direction.NORTH), 3);
                IndustryKit.playerPlacedBare(run.helper, p);
            }
            placed = true;
            go(Phase.RECOMPILE, now);
            settleAt = -1;
        }

        // ---- RECOMPILE: the second stale, the first not ---------------------------------------------------

        private void recompile(GlassReplayGameTests.Run run, GlassReport fr, GlassReport.Unit first, GlassReport.Unit second, long now) {
            if (settleAt < 0) {
                if (fr.checksum().acceptedAt() > acceptedBefore) {
                    settleAt = now + SETTLE_AFTER_COMPILE;
                } else if (now - phaseAt > COMPILE_DEADLINE) {
                    SectionsFx.check(run, "the district recompiles after the press is replaced", false,
                            "compile accepted at " + fr.checksum().acceptedAt() + " since " + acceptedBefore + ", " + SectionsFx.describe(fr));
                    go(Phase.RELEARN, now);
                }
                return;
            }
            if (now < settleAt)
                return;
            String d = SectionsFx.describe(fr);
            SectionsFx.check(run, "the edit at the press stales the second line's unit", second != null && second.stale() && !SectionsFx.learned(second),
                    second == null ? d : second.summary());
            SectionsFx.check(run, "...and only it: the first line's unit keeps its passed window", SectionsFx.learned(first), first == null ? d : first.summary());
            go(Phase.RELEARN, now);
        }

        // ---- RELEARN: the second learns again --------------------------------------------------------------

        private void relearn(GlassReplayGameTests.Run run, GlassReport fr, GlassReport.Unit first, GlassReport.Unit second, long now) {
            boolean learned = SectionsFx.learned(second);
            if (!learned && now - phaseAt <= RELEARN_DEADLINE)
                return;
            String d = SectionsFx.describe(fr);
            SectionsFx.check(run, "the second line's unit learns again after the press is replaced", learned, second == null ? d : second.summary());
            SectionsFx.check(run, "the first line's unit was never stale or frozen from the edit on", !firstStaled, firstStaled ? firstStaledNote : "never");
            SectionsFx.check(run, "the first line's unit has passed", SectionsFx.learned(first), first == null ? d : first.summary());
            SectionsFx.info(run, "pairs after the edit", pairsText(run.fd));
            go(Phase.DONE, now);
        }

        // ---- away and after ------------------------------------------------------------------------------

        @Override
        public void away(GlassReplayGameTests.Run run, GlassReport far, boolean last) {
            long[] m = moved(run.fd);
            if (awayCounted == null)
                awayCounted = m;
            if (!last)
                return;
            GlassReport.Unit first = SectionsFx.unitAt(far, run.far.absolutePos(MIXER)), second = SectionsFx.unitAt(far, run.far.absolutePos(PRESS));
            SectionsFx.info(run, "units while away", SectionsFx.describe(far));
            SectionsFx.check(run, "the first line replays while away", first != null && first.mayReplay() && !first.frozen() && !first.stale(),
                    first == null ? "no unit" : first.summary());
            SectionsFx.check(run, "the second line replays while away", second != null && second.mayReplay() && !second.frozen() && !second.stale(),
                    second == null ? "no unit" : second.summary());
            SectionsFx.check(run, "the district stays on the glass box while away (never the black box)",
                    run.sawGlass && run.leftGlass == 0 && far.replay().model() == GlassReport.REPLAY_GLASS, far.replay().summary());
            SectionsFx.check(run, "nothing is counted while the far copy is away (no mover runs there)", m[0] == awayCounted[0] && m[1] == awayCounted[1],
                    "in " + awayCounted[0] + " to " + m[0] + ", out " + awayCounted[1] + " to " + m[1]);
        }

        @Override
        public void judged(GlassReplayGameTests.Run run, GlassReport far, GlassReport control) {
            SectionsFx.info(run, "units after the reconcile", SectionsFx.describe(far));
            GlassReport.Unit first = SectionsFx.unitAt(far, run.far.absolutePos(MIXER)), second = SectionsFx.unitAt(far, run.far.absolutePos(PRESS));
            SectionsFx.check(run, "neither line is frozen or stale after the reconcile", first != null && !first.frozen() && !first.stale()
                    && second != null && !second.frozen() && !second.stale(), SectionsFx.describe(far));
            SectionsFx.info(run, "pairs after the reconcile", pairsText(run.fd));
        }
    }
}
