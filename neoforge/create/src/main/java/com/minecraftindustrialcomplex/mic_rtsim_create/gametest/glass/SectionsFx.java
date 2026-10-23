package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassReport;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.phys.Vec3;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

/**
 * The kit of SEC-1's section fixtures (SECTIONS.md §9.2, slice S2): a helper that moves a factory, the merge
 * of two lines into one {@link CorpusFactory.Built}, and the reading of a report's judging units. A fixture
 * is a {@link CorpusFactory} plus a {@link GlassReplayGameTests.Scenario}; the corpus cycle (loaded for a
 * window, away for a replay, back for a reconcile that matches) is {@link GlassReplayGameTests}'.
 *
 * <p>Units are read from {@link GlassReport#units()}, which is empty for fewer than two (choice SC74), so a
 * fixture whose own lines make one unit has a bystander: {@link SawShaftsFactory} moved clear of it
 * ({@link #bystander}), in the same district (within one chunk of it) and sharing nothing with it.
 *
 * <p>Every check logs one {@code [gametest] sec1fx <test>: check <what>: ok|FAIL|GAP} line. A GAP is a check
 * the engine or the bridge does not meet yet (the line names it): it never fails the test, and it is marked
 * with a TODO beside the code.
 */
final class SectionsFx {
    private SectionsFx() {}

    /** The bystander's offset along z, and its saw, in the fixture's own coordinates (SawShaftsFactory's (8,3,8)). */
    static final int BYSTANDER_DZ = 12;
    static final BlockPos BYSTANDER_SAW = new BlockPos(8, 3, 8 + BYSTANDER_DZ);

    /** A factory's helper moved by {@code by} (in the base helper's frame): its blocks land {@code by} away. */
    static final class Shifted extends GameTestHelper {
        final GameTestHelper base;
        final BlockPos by;

        Shifted(GameTestHelper base, BlockPos by) {
            super(base.testInfo);
            this.base = base;
            this.by = by;
        }

        @Override
        public BlockPos absolutePos(BlockPos pos) {
            return base.absolutePos(pos.offset(by));
        }

        @Override
        public BlockPos relativePos(BlockPos pos) {
            return base.relativePos(pos).subtract(by);
        }

        @Override
        public Vec3 absoluteVec(Vec3 v) {
            return base.absoluteVec(v.add(by.getX(), by.getY(), by.getZ()));
        }

        @Override
        public Vec3 relativeVec(Vec3 v) {
            return base.relativeVec(v).subtract(by.getX(), by.getY(), by.getZ());
        }
    }

    /** The bystander line: a saw cutting andesite alloy, {@value #BYSTANDER_DZ} blocks along z. */
    static CorpusFactory.Built bystander(GameTestHelper h) {
        return new SawShaftsFactory().build(new Shifted(h, new BlockPos(0, 0, BYSTANDER_DZ)));
    }

    /** One build of several lines: the first's anchor, every line's stores and expectations. */
    static CorpusFactory.Built merge(int window, String notes, CorpusFactory.Built... parts) {
        List<BlockPos> in = new ArrayList<>(), out = new ArrayList<>();
        List<CorpusFactory.Expect> expect = new ArrayList<>();
        for (CorpusFactory.Built p : parts) {
            in.addAll(p.inputs());
            out.addAll(p.outputs());
            expect.addAll(p.expect());
        }
        return new CorpusFactory.Built(parts[0].anchor(), in, out, expect, List.of(), window, notes);
    }

    // ---- reading the units ---------------------------------------------------------------------

    /** The units whose box holds the position. */
    static List<GlassReport.Unit> holding(GlassReport r, BlockPos p) {
        return r == null ? List.of() : r.unitsAt(p.getX(), p.getY(), p.getZ());
    }

    /** The one unit whose box holds the position, or null when none or several do. */
    static GlassReport.Unit unitAt(GlassReport r, BlockPos p) {
        List<GlassReport.Unit> u = holding(r, p);
        return u.size() == 1 ? u.get(0) : null;
    }

    /** The index of {@code u} in the report's units (identity), or -1. */
    static int indexOf(GlassReport r, GlassReport.Unit u) {
        if (r == null || u == null)
            return -1;
        for (int i = 0; i < r.units().size(); i++)
            if (r.units().get(i) == u)
                return i;
        return -1;
    }

    /** A unit that has judged a window and passed it, and no edit has staled since. */
    static boolean learned(GlassReport.Unit u) {
        return u != null && u.passed() && !u.stale() && u.windowsCompleted() > 0 && u.verdict() == GlassReport.PASS;
    }

    /** Every unit of the report, one line each. */
    static String describe(GlassReport r) {
        if (r == null)
            return "no report";
        StringBuilder sb = new StringBuilder(r.units().size() + " units");
        for (int i = 0; i < r.units().size(); i++)
            sb.append("; unit ").append(i).append(": ").append(r.units().get(i).summary()).append(", ready at ").append(r.units().get(i).readyAt());
        return sb.toString();
    }

    // ---- checks ----------------------------------------------------------------------------------

    private static String id(GlassReplayGameTests.Run run) {
        return run.f.name() + (run.partial ? " (partial)" : "");
    }

    /** A check that must hold: logged, and a failure of the test when it does not. */
    static boolean check(GlassReplayGameTests.Run run, String what, boolean ok, Object detail) {
        return check(id(run), run.f.name(), run.test, run.failures, what, ok, detail);
    }

    static boolean check(String id, String factory, String test, List<String> failures, String what, boolean ok, Object detail) {
        MicRtsim.LOGGER.info("[gametest] sec1fx {}: check {}: {} ({})", id, what, ok ? "ok" : "FAIL", detail);
        IndustryKit.record(test, "check", what + ": " + (ok ? "ok" : "FAIL") + " (" + detail + ")");
        if (!ok)
            failures.add(factory + ": " + what + " (" + detail + ")");
        return ok;
    }

    /**
     * A check the engine or the bridge does not meet yet ({@code todo} names the gap). It never fails the
     * test; when it holds after all it logs {@code ok}, so a closed gap shows.
     */
    static void gap(GlassReplayGameTests.Run run, String what, boolean ok, String todo, Object detail) {
        gap(id(run), run.test, what, ok, todo, detail);
    }

    static void gap(String id, String test, String what, boolean ok, String todo, Object detail) {
        MicRtsim.LOGGER.info("[gametest] sec1fx {}: check {}: {} ({}){}", id, what, ok ? "ok" : "GAP", detail, ok ? "" : " TODO " + todo);
        IndustryKit.record(test, "check", what + ": " + (ok ? "ok" : "GAP, TODO " + todo) + " (" + detail + ")");
    }

    static void info(GlassReplayGameTests.Run run, String what, Object detail) {
        info(id(run), run.test, what, detail);
    }

    static void info(String id, String test, String what, Object detail) {
        MicRtsim.LOGGER.info("[gametest] sec1fx {}: {}: {}", id, what, detail);
        IndustryKit.record(test, what, String.valueOf(detail));
    }

    /** {@code rel} in the far copy's frame (its offset and, for the partial variant, its shift included). */
    static BlockPos far(GlassReplayGameTests.Run run, BlockPos rel) {
        return run.far.absolutePos(rel);
    }

    /** {@code rel} in the control's frame (the yard). */
    static BlockPos control(GlassReplayGameTests.Run run, BlockPos rel) {
        return run.helper.absolutePos(rel);
    }

    static String pos(BlockPos p) {
        return String.format(Locale.ROOT, "(%d %d %d)", p.getX(), p.getY(), p.getZ());
    }
}
