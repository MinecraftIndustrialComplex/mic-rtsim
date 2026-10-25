package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
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
import java.util.regex.Pattern;

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
 * the second line's press within one tick leaves the first line's unit with its passed window, the recompile
 * that finds the press as it was clears the second's edit (SECTIONS.md §3.6), and the pairs' counters go on
 * across it (their {@code since} is unchanged); while away both replay and nothing counts.
 *
 * <p>A compile that sees the press missing is not part of this fixture: it changes the vault's section
 * fingerprints and drops both units' state and the counters (SC84).
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

    /** The {@code since} of each of the district's pairs, in the order held. */
    static List<Long> sinces(long district) {
        Counting c = IndustryKit.ind().counting();
        List<Long> out = new ArrayList<>();
        for (BlockPos[] p : c.pairs(district)) {
            long[] k = c.counters(district, p[0], p[1]);
            out.add(k == null ? -1L : k[0]);
        }
        return out;
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

    // ---- /rtsim district sections ----------------------------------------------------------------------

    private static final Pattern SECTION_LINE = Pattern.compile("^ {2}s(\\d+)[ :].*");

    /**
     * The command's text against the report it was built from (SECTIONS.md §8.1): the district holds the two lines
     * sharing the vault and the bystander, so the report has several sections and a shared store. The report is
     * fetched in the same tick as the command, on the server thread, so the two see one state. Checks the header,
     * one {@code  sN} line per section (counted on its own, apart from the shared store lines, which start
     * {@code  shared}), each line's state word and unit, the vault's shared-store line (counted at its movers, with
     * the report's pair count), the return value, and {@code /rtsim district info}'s glass sections line.
     */
    static void sectionsCommand(GlassReplayGameTests.Run run, BlockPos vault) {
        District d = IndustryKit.ind().district(run.fd);
        var fetch = d == null ? null : IndustryKit.ind().diagnostics().glassReportOf(d);
        if (fetch == null || fetch.report() == null) {
            SectionsFx.check(run, "/rtsim district sections has a report to print", false, fetch == null ? "no district" : fetch.note());
            return;
        }
        GlassReport r = fetch.report();
        IndustryKit.Said said = IndustryKit.say(run.level(), vault, "rtsim district sections");
        List<String> lines = said.lines();
        SectionsFx.info(run, "the sections command", String.join(" | ", lines));
        int sections = r.sections().size(), shared = r.shared().size();
        SectionsFx.check(run, "the report has sections to print (a vault between lines, and a bystander)", sections >= 4 && shared >= 1,
                sections + " sections, " + shared + " shared stores");
        SectionsFx.check(run, "the command returns the report's section count", said.ran() == sections, said.ran() + " vs " + sections);
        SectionsFx.check(run, "its header names the district and the counts", !lines.isEmpty()
                && lines.get(0).startsWith("district " + run.fd + " '") && lines.get(0).contains(": " + sections + " sections")
                && lines.get(0).contains(", " + shared + " shared stores"), lines.isEmpty() ? "no lines" : lines.get(0));
        String[] bySection = new String[sections];
        int sectionLines = 0, sharedLines = 0;
        for (String l : lines) {
            var m = SECTION_LINE.matcher(l);
            if (m.matches()) {
                sectionLines++;
                int i = Integer.parseInt(m.group(1));
                if (i >= 0 && i < sections)
                    bySection[i] = l;
            } else if (l.startsWith("  shared store ")) {
                sharedLines++;
            }
        }
        SectionsFx.check(run, "one line per section, exactly", sectionLines == sections, sectionLines + " lines for " + sections + " sections");
        SectionsFx.check(run, "one line per shared store, counted on its own", sharedLines == shared, sharedLines + " lines for " + shared + " stores");
        boolean words = true;
        String wrong = "";
        for (int i = 0; i < sections; i++) {
            GlassReport.Section s = r.sections().get(i);
            String word = s.state() == GlassReport.FAILED ? "FAILED" : s.stateName();
            String unit = s.unit() < 0 ? ", district window" : ", unit " + s.unit();
            if (bySection[i] == null || !bySection[i].contains(": " + word + unit + ",")) {
                words = false;
                wrong = "s" + i + " should say '" + word + unit + "': " + bySection[i];
                break;
            }
        }
        SectionsFx.check(run, "each section line carries its state word and unit from the report", words, words ? "all " + sections : wrong);
        BlockPos mixer = run.far.absolutePos(MIXER);
        int unit = SectionsFx.indexOf(r, SectionsFx.unitAt(r, mixer));
        GlassReport.Section first = unit < 0 ? null : r.sectionsAt(mixer.getX(), mixer.getY(), mixer.getZ()).stream()
                .filter(x -> x.unit() == unit).findFirst().orElse(null);
        int at = first == null ? -1 : r.sections().indexOf(first);
        SectionsFx.check(run, "the mixing line's section reads passed (it has learned)", at >= 0 && bySection[at] != null && bySection[at].contains(": passed,"),
                at < 0 ? "no section at the mixer" : bySection[at]);
        GlassReport.Shared vaultRec = null;
        for (GlassReport.Shared h : r.shared())
            if (h.counted())
                vaultRec = h;
        String want = vaultRec == null ? "" : ": counted at its movers (" + vaultRec.pairs() + " pairs), ";
        SectionsFx.check(run, "the vault's line says it is counted at its movers, with the report's pair count",
                vaultRec != null && vaultRec.pairs() >= 2 && lines.stream().anyMatch(l -> l.startsWith("  shared store ") && l.contains(want)),
                vaultRec == null ? "no counted shared store" : want);
        // /rtsim district info's one line (§8.1), in the same tick: the report's counts.
        String infoLine = "  glass sections: " + sections + ", " + r.sectionsMayReplay() + " may replay, " + r.sectionsReplaying()
                + " replaying, " + shared + " shared stores";
        List<String> info = IndustryKit.ind().diagnostics().debugLines(d).stream().map(c -> c.getString()).toList();
        SectionsFx.check(run, "/rtsim district info's glass sections line carries the report's counts",
                info.stream().anyMatch(l -> l.startsWith(infoLine)), infoLine + " in " + String.join(" | ", info));
    }

    // ---- the checks ----------------------------------------------------------------------------------

    static final class Check implements GlassReplayGameTests.Scenario {
        enum Phase { LEARN, COUNT, RECOMPILE, DONE }

        static final int READ = 10;
        /** The stretch over which the count is compared with the vault's stock. */
        static final int COUNT_TICKS = 600;
        static final int SETTLE_AFTER_COMPILE = 40, COMPILE_DEADLINE = 1500;
        static final int LEARN_DEADLINE = 9000;

        private Phase phase = Phase.LEARN;
        private long phaseAt, nextRead, acceptedBefore, settleAt;
        private boolean firstStaled;
        private String firstStaledNote = "";
        private long stock0, in0, out0;
        private List<Long> sinceBefore = List.of();
        private String editSeen;
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
            firstStaled = false;
            firstStaledNote = "";
            awayCounted = null;
            sinceBefore = List.of();
            editSeen = null;
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
                case RECOMPILE -> recompile(run, fr, first, second, now);
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
            sectionsCommand(run, vault);
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
            // The edit: break the second line's press and place it again within one tick. A compile between the two would see
            // line B without its press, the vault interior to line A, and reset both lines' units (SC84).
            acceptedBefore = fr.checksum().acceptedAt();
            sinceBefore = sinces(run.fd);
            editSeen = null;
            BlockPos[] abs = {run.helper.absolutePos(PRESS), run.far.absolutePos(PRESS)};
            SectionsFx.info(run, "edit", "break the press at " + SectionsFx.pos(abs[1]) + " and place it again in the same tick; compile accepted at " + acceptedBefore);
            for (BlockPos p : abs) {
                NeoForge.EVENT_BUS.post(new BlockEvent.BreakEvent(run.level(), p, run.level().getBlockState(p), FakePlayers.editor(run.level())));
                run.level().setBlock(p, Blocks.AIR.defaultBlockState(), 3);
                run.level().setBlock(p, AllBlocks.MECHANICAL_PRESS.getDefaultState().setValue(MechanicalPressBlock.HORIZONTAL_FACING, Direction.NORTH), 3);
                IndustryKit.playerPlacedBare(run.helper, p);
            }
            settleAt = -1;
            go(Phase.RECOMPILE, now);
        }

        // ---- RECOMPILE: the compile finds the press as it was ---------------------------------------------

        private void recompile(GlassReplayGameTests.Run run, GlassReport fr, GlassReport.Unit first, GlassReport.Unit second, long now) {
            if (editSeen == null)
                editSeen = (second == null ? "no unit" : second.summary()) + " (compile accepted at " + fr.checksum().acceptedAt() + ", before the edit "
                        + acceptedBefore + ")";
            if (settleAt < 0) {
                if (fr.checksum().acceptedAt() > acceptedBefore) {
                    settleAt = now + SETTLE_AFTER_COMPILE;
                } else if (now - phaseAt > COMPILE_DEADLINE) {
                    SectionsFx.check(run, "the district recompiles after the press is replaced", false,
                            "compile accepted at " + fr.checksum().acceptedAt() + " since " + acceptedBefore + ", " + SectionsFx.describe(fr));
                    go(Phase.DONE, now);
                }
                return;
            }
            if (now < settleAt)
                return;
            String d = SectionsFx.describe(fr);
            SectionsFx.info(run, "the second line's unit at the first read after the edit", editSeen);
            SectionsFx.check(run, "the recompile found the press as it was: the first line keeps its passed window and the second's edit is cleared (§3.6)",
                    SectionsFx.learned(first) && SectionsFx.learned(second), first == null || second == null ? d : first.summary() + "; " + second.summary());
            List<Long> since = sinces(run.fd);
            SectionsFx.check(run, "the counters went on across the edit and the recompile (every pair keeps its since)", !since.isEmpty() && since.equals(sinceBefore),
                    "since " + sinceBefore + " to " + since);
            SectionsFx.check(run, "the first line's unit was never stale or frozen from the edit on", !firstStaled, firstStaled ? firstStaledNote : "never");
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
