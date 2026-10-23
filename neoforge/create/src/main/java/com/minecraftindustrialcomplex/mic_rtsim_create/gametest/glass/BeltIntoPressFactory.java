package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassReport;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.kinetics.base.DirectionalAxisKineticBlock;
import com.simibubi.create.content.kinetics.press.MechanicalPressBlock;
import com.simibubi.create.content.kinetics.saw.SawBlock;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.item.Items;

import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.*;

/**
 * SEC-1 fixture (SECTIONS.md §9.2): a belt straight from a press into a saw. One section: a belt between two
 * machines is no cut (the compile cuts at stores), so the whole run is one section of one unit.
 *
 * <pre>
 *   y5          press (x 7, NORTH, 128 RPM from z 9)
 *   y4  chute
 *   y3  belt    belt    belt   belt   belt   belt   saw (up, axis Z)   chute
 *   y2                                                              chest (sheets)
 *       x4      x5      x6     x7     x8     x9     x10                x11            (z 8)
 * </pre>
 * A chest of iron ingots (x 4, y 5) drains through a smart chute (iron ingots, one at a time) onto the
 * first of six belt blocks; the press over the fourth makes each ingot a sheet; the sixth hands it to the
 * saw, which has no recipe for a sheet and passes it on along its movement vector into a chute over the
 * output chest. The belt moves +X at speed -64 (motor south at its start, the way {@code PressSheetsFactory}
 * and the calibration have it), the saw moves items +X at speed +96 with its axis along z
 * ({@code SawBlockEntity.getItemMovementVec}: x = sign(speed) when {@code AXIS_ALONG_FIRST_COORDINATE} is
 * false), its motor south of it at (10, 3, 7). The line's own rate is the press's (belt cycle 19 ticks).
 *
 * <p>The bystander ({@link SectionsFx#bystander}) makes the report carry its units.
 */
public final class BeltIntoPressFactory implements CorpusFactory {
    static final int PRESS_RPM = 128, BELT_RPM = 64, SAW_RPM = 96;
    static final int CYCLE = 19;

    static final BlockPos IN = new BlockPos(4, 5, 8), FEED = new BlockPos(4, 4, 8);
    static final BlockPos BELT_START = new BlockPos(4, 3, 8), BELT_END = new BlockPos(9, 3, 8);
    static final BlockPos PRESS = new BlockPos(7, 5, 8), SAW = new BlockPos(10, 3, 8);
    static final BlockPos DROP = new BlockPos(11, 3, 8), OUT = new BlockPos(11, 2, 8);

    @Override
    public String name() {
        return "belt_into_press";
    }

    @Override
    public int warmupTicks() {
        return 300;
    }

    @Override
    public Built build(GameTestHelper h) {
        BlockPos in = at(h, IN.getX(), IN.getY(), IN.getZ()), out = at(h, OUT.getX(), OUT.getY(), OUT.getZ());
        chest(h, in);
        fill(h, in, Items.IRON_INGOT, 1024);
        SteamHelpers.smartChute(h, abs(h, FEED), Items.IRON_INGOT, 1);
        belt(h, abs(h, BELT_START), abs(h, BELT_END));
        // An X belt moves +X at negative speed; a motor facing south generates its value as is.
        motor(h, at(h, 4, 3, 7), Direction.SOUTH, -BELT_RPM);
        set(h, abs(h, PRESS), AllBlocks.MECHANICAL_PRESS.getDefaultState().setValue(MechanicalPressBlock.HORIZONTAL_FACING, Direction.NORTH));
        motor(h, at(h, 7, 5, 9), Direction.NORTH, PRESS_RPM);
        set(h, abs(h, SAW), AllBlocks.MECHANICAL_SAW.getDefaultState().setValue(SawBlock.FACING, Direction.UP)
                .setValue(DirectionalAxisKineticBlock.AXIS_ALONG_FIRST_COORDINATE, false));
        motor(h, at(h, 10, 3, 7), Direction.SOUTH, SAW_RPM);
        set(h, abs(h, DROP), AllBlocks.CHUTE.getDefaultState());
        chest(h, out);

        double r = TICKS_PER_MINUTE / (double) CYCLE;
        Built line = new Built(abs(h, PRESS), List.of(in), List.of(out), List.of(
                Expect.exact("minecraft:iron_ingot", -r, "PressingBehaviour on a belt: 19 ticks per ingot (nominal)"),
                Expect.exact("create:iron_sheet", r, "pressing/iron_ingot: one sheet per ingot")),
                "press over a belt at 64 RPM straight into a saw at 96 RPM; 1,024 ingots last 19,456 ticks");
        return SectionsFx.merge(7200, line.notes() + "; bystander saw shafts at z +" + SectionsFx.BYSTANDER_DZ, line, SectionsFx.bystander(h));
    }

    private static BlockPos abs(GameTestHelper h, BlockPos rel) {
        return at(h, rel.getX(), rel.getY(), rel.getZ());
    }

    /** The checks of §9.2's row: one unit for the whole run, the bystander's apart, both learn, the line replays. */
    static final class Check implements GlassReplayGameTests.Scenario {
        private boolean structure;
        private GlassReport.Unit awayFirst;

        @Override
        public void built(GlassReplayGameTests.Run run) {
            structure = false;
            awayFirst = null;
        }

        @Override
        public String ready(GlassReplayGameTests.Run run, GlassReport far) {
            if (far.units().size() < 2)
                return "units " + far.units().size() + " (" + far.units().size() + " of 2)";
            for (GlassReport.Unit u : far.units())
                if (!SectionsFx.learned(u))
                    return "a unit has not learned: " + SectionsFx.describe(far);
            if (!structure) {
                structure = true;
                structure(run, "far", far, run.far::absolutePos);
                GlassReport control = GlassReplayGameTests.report(run, run.cd);
                if (control != null)
                    structure(run, "control", control, run.helper::absolutePos);
            }
            return null;
        }

        private void structure(GlassReplayGameTests.Run run, String copy, GlassReport r, java.util.function.Function<BlockPos, BlockPos> abs) {
            boolean far = copy.equals("far");
            GlassReport.Unit line = SectionsFx.unitAt(r, abs.apply(PRESS));
            GlassReport.Unit bystander = SectionsFx.unitAt(r, abs.apply(SectionsFx.BYSTANDER_SAW));
            String d = copy + ": " + SectionsFx.describe(r);
            SectionsFx.info(run, copy + " units", d);
            if (!far)
                return;
            SectionsFx.check(run, "two units, the line's and the bystander's", r.units().size() == 2, d);
            SectionsFx.check(run, "the press and the bystander's saw are in different units", line != null && bystander != null && line != bystander, d);
            SectionsFx.check(run, "the line is one section (a belt from a press into a saw is no cut)", line != null && line.sections() == 1,
                    line == null ? "no unit" : line.sections() + " sections");
            SectionsFx.check(run, "its box holds the belt from end to end, the press and the saw",
                    line != null && line.holds(abs.apply(BELT_START).getX(), abs.apply(BELT_START).getY(), abs.apply(BELT_START).getZ())
                            && line.holds(abs.apply(BELT_END).getX(), abs.apply(BELT_END).getY(), abs.apply(BELT_END).getZ())
                            && line.holds(abs.apply(PRESS).getX(), abs.apply(PRESS).getY(), abs.apply(PRESS).getZ())
                            && line.holds(abs.apply(SAW).getX(), abs.apply(SAW).getY(), abs.apply(SAW).getZ()),
                    line == null ? "no unit" : line.summary());
            SectionsFx.check(run, "the bystander is one section of its own", bystander != null && bystander.sections() == 1,
                    bystander == null ? "no unit" : bystander.summary());
            SectionsFx.check(run, "the bystander's box does not hold the press", bystander != null && !bystander.holds(
                    abs.apply(PRESS).getX(), abs.apply(PRESS).getY(), abs.apply(PRESS).getZ()), "");
            SectionsFx.check(run, "both units learned", SectionsFx.learned(line) && SectionsFx.learned(bystander), SectionsFx.describe(r));
        }

        @Override
        public void away(GlassReplayGameTests.Run run, GlassReport far, boolean last) {
            GlassReport.Unit line = SectionsFx.unitAt(far, run.far.absolutePos(PRESS));
            if (awayFirst == null && line != null)
                awayFirst = line;
            if (!last)
                return;
            SectionsFx.info(run, "units while away", SectionsFx.describe(far));
            SectionsFx.check(run, "the line replays while away (its window passed, nothing staled or froze it)",
                    line != null && line.mayReplay() && !line.frozen() && !line.stale(), line == null ? "no unit" : line.summary());
            SectionsFx.check(run, "the bystander replays too", SectionsFx.unitAt(far, run.far.absolutePos(SectionsFx.BYSTANDER_SAW)) != null
                    && SectionsFx.unitAt(far, run.far.absolutePos(SectionsFx.BYSTANDER_SAW)).mayReplay(), "");
        }

        @Override
        public void judged(GlassReplayGameTests.Run run, GlassReport far, GlassReport control) {
            SectionsFx.info(run, "units after the reconcile", SectionsFx.describe(far));
            GlassReport.Unit line = SectionsFx.unitAt(far, run.far.absolutePos(PRESS));
            SectionsFx.check(run, "the line is neither frozen nor stale after the reconcile", line != null && !line.frozen() && !line.stale(),
                    line == null ? "no unit" : line.summary());
        }
    }
}
