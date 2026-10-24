package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassReport;
import com.simibubi.create.AllItems;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.item.ItemStack;
import net.neoforged.neoforge.items.IItemHandler;
import net.neoforged.neoforge.items.ItemHandlerHelper;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.HopperAtASharedVaultFactory.MIXER;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.HopperAtASharedVaultFactory.PRESS;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.HopperAtASharedVaultFactory.VAULT;

/**
 * SEC-1 fixture (SECTIONS.md §9.2, slice S3, choice SC88): {@link VaultBetweenMixersFactory}'s two lines on a
 * vault, and the test inserting brass ingots into the vault through its capability for one window, as another
 * mod's mover would (no count point sees it: nothing Java counts moved them). The vault's stock then changes
 * by more than the two lines' movers counted, and the glass cannot say which unit's the difference is: the
 * residue at a counted store goes to every unit at it (SC88), as the one row of a judging group would have it.
 * So that window fails both units on brass, and the next window, with nothing inserted, judges the two apart
 * again.
 *
 * <p>The insert lands in both copies (control and far) so the copy check still compares like with like, and
 * mid-window for both units: each unit's windows run back to back, so the test waits until both have run for
 * {@link Check#INTO} ticks of a window and inserts then. The checks read each unit's verdict at the poll after
 * its window completes.
 */
public final class ResidueAtASharedVaultFactory implements CorpusFactory {
    /** Brass ingots inserted: far more than a window's moves at the vault (75 a minute in, 63 out) can hide. */
    static final int INSERTED = 1000;

    private final HopperAtASharedVaultFactory lines = new HopperAtASharedVaultFactory(false, "residue_vault");

    @Override
    public String name() {
        return "residue_vault";
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
        enum Phase { LEARN, WAIT_MID, WAIT_RESIDUE_WINDOW, WAIT_NEXT_WINDOW, DONE }

        static final int READ = 10;
        /** Both units must be this far into a window to insert (and not yet this far from its end). */
        static final int INTO = 200, UNTIL = 1400;
        static final int LEARN_DEADLINE = 9000, WINDOW_DEADLINE = 9000;

        private Phase phase = Phase.LEARN;
        private long phaseAt, nextRead;
        /** Per unit (0 the mixing line's, 1 the press line's): completed windows when last read, and the tick that changed. */
        private final int[] done = new int[2];
        private final long[] doneAt = new long[2];
        private final int[] doneAtInsert = new int[2];
        private final int[] residueVerdict = {-1, -1}, nextVerdict = {-1, -1};
        private final int[] residueStrikes = new int[2], nextStrikes = new int[2];
        private final int[] nextDone = new int[2];
    
        @Override
        public int extraTicks() {
            return 9000;
        }

        @Override
        public void built(GlassReplayGameTests.Run run) {
            phase = Phase.LEARN;
            phaseAt = run.now();
            nextRead = 0;
            done[0] = done[1] = -1;
            doneAt[0] = doneAt[1] = 0;
            residueVerdict[0] = residueVerdict[1] = nextVerdict[0] = nextVerdict[1] = -1;
        }

        @Override
        public String ready(GlassReplayGameTests.Run run, GlassReport far) {
            return phase == Phase.DONE ? null : "residue_vault in phase " + phase;
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
            GlassReport.Unit[] u = {SectionsFx.unitAt(fr, run.far.absolutePos(MIXER)), SectionsFx.unitAt(fr, run.far.absolutePos(PRESS))};
            // Each unit's last completed window changes at its completion: remember when, and what it said.
            for (int i = 0; i < 2; i++)
                if (u[i] != null && u[i].windowsCompleted() != done[i]) {
                    done[i] = u[i].windowsCompleted();
                    doneAt[i] = now;
                    onWindow(run, i, u[i], now);
                }
            switch (phase) {
                case LEARN -> learn(run, fr, u, now);
                case WAIT_MID -> waitMid(run, fr, u, now);
                case WAIT_RESIDUE_WINDOW -> waitResidue(run, fr, u, now);
                case WAIT_NEXT_WINDOW -> waitNext(run, fr, u, now);
                default -> { }
            }
        }

        /** A unit completed a window: in the phases that wait for it, take its verdict (read before the next completes). */
        private void onWindow(GlassReplayGameTests.Run run, int i, GlassReport.Unit u, long now) {
            String name = i == 0 ? "mixing line" : "press line";
            SectionsFx.info(run, "window of the " + name + " completed at " + now, u.summary());
            if (phase == Phase.WAIT_RESIDUE_WINDOW && residueVerdict[i] < 0 && u.windowsCompleted() > doneAtInsert[i]) {
                residueVerdict[i] = u.verdict();
                residueStrikes[i] = u.strikes();
                nextDone[i] = u.windowsCompleted();
            } else if (phase == Phase.WAIT_NEXT_WINDOW && nextVerdict[i] < 0 && u.windowsCompleted() > nextDone[i]) {
                nextVerdict[i] = u.verdict();
                nextStrikes[i] = u.strikes();
            }
        }

        private void learn(GlassReplayGameTests.Run run, GlassReport fr, GlassReport.Unit[] u, long now) {
            if (now - phaseAt > LEARN_DEADLINE) {
                SectionsFx.check(run, "the lines' units learn before the scenario's deadline", false, SectionsFx.describe(fr) + "; " + VaultBetweenMixersFactory.pairsText(run.fd));
                go(Phase.DONE, now);
                return;
            }
            if (fr.units().size() < 2)
                return;
            for (GlassReport.Unit x : fr.units())
                if (!SectionsFx.learned(x))
                    return;
            String d = SectionsFx.describe(fr);
            SectionsFx.info(run, "far units", d);
            SectionsFx.check(run, "the mixing line and the press line are two judging units", u[0] != null && u[1] != null && u[0] != u[1], d);
            VaultBetweenMixersFactory.counted(run, VAULT, 2);
            if (u[0] == null || u[1] == null || u[0] == u[1]) {
                go(Phase.DONE, now);
                return;
            }
            go(Phase.WAIT_MID, now);
        }

        private void waitMid(GlassReplayGameTests.Run run, GlassReport fr, GlassReport.Unit[] u, long now) {
            if (now - phaseAt > WINDOW_DEADLINE) {
                SectionsFx.check(run, "both units are mid-window at once, to insert in", false, "windows done " + done[0] + " and " + done[1]);
                go(Phase.DONE, now);
                return;
            }
            for (int i = 0; i < 2; i++) {
                long into = now - doneAt[i];
                if (u[i] == null || into < INTO || into > UNTIL)
                    return;
            }
            // Both are in the body of a window: insert brass through the vault's capability, in both copies.
            long[] before = VaultBetweenMixersFactory.moved(run.fd);
            long stock = 0;
            for (BlockPos abs : new BlockPos[] {run.helper.absolutePos(VAULT), run.far.absolutePos(VAULT)}) {
                IItemHandler inv = VaultBetweenMixersFactory.handler(run, abs);
                ItemStack rest = ItemStack.EMPTY;
                int left = INSERTED;
                while (left > 0 && inv != null) {
                    rest = ItemHandlerHelper.insertItemStacked(inv, new ItemStack(AllItems.BRASS_INGOT.get(), Math.min(left, 64)), false);
                    left -= Math.min(left, 64) - rest.getCount();
                    if (!rest.isEmpty())
                        break;
                }
                stock = VaultBetweenMixersFactory.total(inv);
                SectionsFx.info(run, "insert at " + SectionsFx.pos(abs), (INSERTED - left) + " brass ingots through the capability, the vault now holds " + stock);
            }
            long[] after = VaultBetweenMixersFactory.moved(run.fd);
            SectionsFx.check(run, "the insert is no count point's: no pair counted it", before[0] == after[0] && before[1] == after[1],
                    "in " + before[0] + " to " + after[0] + ", out " + before[1] + " to " + after[1]);
            doneAtInsert[0] = done[0];
            doneAtInsert[1] = done[1];
            SectionsFx.info(run, "inserted at " + now, "windows done " + done[0] + " and " + done[1] + "; into the windows " + (now - doneAt[0]) + " and " + (now - doneAt[1]) + " ticks");
            go(Phase.WAIT_RESIDUE_WINDOW, now);
        }

        private void waitResidue(GlassReplayGameTests.Run run, GlassReport fr, GlassReport.Unit[] u, long now) {
            if (now - phaseAt > WINDOW_DEADLINE) {
                SectionsFx.check(run, "both units complete the window the insert fell in", false, "verdicts " + residueVerdict[0] + " and " + residueVerdict[1]);
                go(Phase.DONE, now);
                return;
            }
            if (residueVerdict[0] < 0 || residueVerdict[1] < 0)
                return;
            String d = SectionsFx.describe(fr);
            SectionsFx.info(run, "the residue window", "mixing line " + verdict(residueVerdict[0]) + " (strikes " + residueStrikes[0] + "), press line "
                    + verdict(residueVerdict[1]) + " (strikes " + residueStrikes[1] + "); " + d);
            SectionsFx.check(run, "the window with the insert fails the mixing line's unit (the residue is charged to it)", residueVerdict[0] == GlassReport.FAIL,
                    verdict(residueVerdict[0]));
            SectionsFx.check(run, "...and the press line's unit, as a judging group would fail both", residueVerdict[1] == GlassReport.FAIL, verdict(residueVerdict[1]));
            go(Phase.WAIT_NEXT_WINDOW, now);
        }

        private void waitNext(GlassReplayGameTests.Run run, GlassReport fr, GlassReport.Unit[] u, long now) {
            boolean both = nextVerdict[0] >= 0 && nextVerdict[1] >= 0;
            if (!both && now - phaseAt <= WINDOW_DEADLINE)
                return;
            String d = SectionsFx.describe(fr);
            SectionsFx.info(run, "the next window", "mixing line " + verdict(nextVerdict[0]) + " (strikes " + nextStrikes[0] + "), press line "
                    + verdict(nextVerdict[1]) + " (strikes " + nextStrikes[1] + "); " + d);
            SectionsFx.check(run, "the next window is judged for both units, each its own", both, "verdicts " + verdict(nextVerdict[0]) + " and " + verdict(nextVerdict[1]));
            SectionsFx.check(run, "...and the mixing line's passes again", nextVerdict[0] == GlassReport.PASS, verdict(nextVerdict[0]));
            SectionsFx.check(run, "...and the press line's passes again", nextVerdict[1] == GlassReport.PASS, verdict(nextVerdict[1]));
            go(Phase.DONE, now);
        }

        private static String verdict(int v) {
            return v < 0 ? "not seen" : v == GlassReport.PASS ? "pass" : v == GlassReport.FAIL ? "FAIL" : "no verdict";
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
