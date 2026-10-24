package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassReport;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.AllItems;
import com.simibubi.create.content.fluids.pipes.FluidPipeBlock;
import com.simibubi.create.content.fluids.tank.FluidTankBlockEntity;
import com.simibubi.create.content.kinetics.press.MechanicalPressBlock;
import com.simibubi.create.content.kinetics.steamEngine.PoweredShaftBlockEntity;
import com.simibubi.create.content.kinetics.steamEngine.SteamEngineBlock;
import com.simibubi.create.content.logistics.chute.ChuteBlockEntity;
import com.simibubi.create.content.processing.burner.BlazeBurnerBlock;
import com.simibubi.create.content.processing.burner.BlazeBurnerBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.PipeBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.AttachFace;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.level.BlockEvent;

import java.util.List;
import java.util.Locale;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.*;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.SteamHelpers.*;

/**
 * SEC-1 fixture (SECTIONS.md §9.2): one boiler network driving a crushing line and a press line that share
 * nothing else. Three sections: the boiler's, and the two lines on one kinetic domain (Q1 a: domains do not
 * join sections). An edit to the press line (its input smart chute broken and placed again with a filter, which
 * changes the press section's fingerprint) makes the press line's unit new and leaves the others as they were.
 * Adding presses until the stress passes the least capacity the crushing line's window ran at stales the
 * crushing line too ({@code check_domains}, SECTIONS.md §3.6).
 *
 * <pre>
 *   pump (x 5..9, z 13..16)        boiler (x 10..12)               hub (x 14..15)             lines
 *   creative motor (5,3,13) 16 RPM  burner (10,3,13), 7 coal        powered shaft (14,4,13)    CRUSHING LINE (z 12)
 *   shaft, large cog LB (7,3,13)    tank 2x2x2 (10..11,4..5,13..14)  cog C (14,5,13): 64 RPM     barrel (14,7,12) wheat
 *   pump (7,4,14) east: 32 RPM      engine (12,4,13), facing east    shaft, large cog L (14,7,13) chute, millstone (14,5,12)
 *   pool (6..7,1,14..15), pipes                                      cog S (15,7,14): 128 RPM     chute, barrel (14,3,12) flour
 *   up (6,2..4,14) and east                                          shaft, gearbox (15,5,14)    PRESS LINE (z 16)
 *   (8..9,4,14) into the tank                                        shaft (15,5,15), press       chest (13,5,16) iron, smart chute,
 *                                                                    (15,5,16) on axis Z          belt (13..14,3,16), depot (15,3,16),
 *                                                                    shafts (15,5,17..33): extras smart chute, chest (15,1,16) sheets
 * </pre>
 * The shafts behind the press stand from the start, so every chunk the extra presses take is a member chunk
 * before the first of them: a press replaces a shaft, which changes no chunk and no inventory, so the
 * district's revision stays and the recompile carries the units and runs the domain check (review H2).
 * The pump is a level-1 boiler's water: one pump at 32 RPM moves 16 mB per tick from a pool of four sources
 * (the vanilla infinite-water rule), so the supply is 16 mB/t, ceil(16) / 10 = 1. One burner gives heat 1 and
 * the 2 x 2 x 2 tank size 8 / 4 = 2: boiler level min(1, 1, 2) = 1. One engine at level 1 has efficiency 1,
 * so the powered shaft turns at 64 RPM with 256 SU per RPM: 16,384 SU of capacity (SteamHelpers' doc has the
 * derivation; BoilerOverstressFactory's the measurements). The pump turns on its own creative motor, a
 * second domain: the steam domain has no creative source, so its capacity is the boiler's.
 *
 * <p>Stress (impact x |RPM|): the millstone 4 x 64 = 256 SU, the press 8 x 128 = 1,024 SU; cogs, shafts and the
 * gearbox none: 1,280 SU of 16,384. Each extra press on the press's axis adds 1,024 SU (idle presses
 * count: a network member with an impact, nothing more), so the fifteenth passes 16,384 (1,280 + 15 x 1,024 =
 * 16,640) and the fourteenth does not (15,616). The scenario reads the stress it is at from Create and adds the
 * presses that fit, then one more.
 *
 * <p>Differs from the table: a millstone line stands for the crushing line (a crushing wheel needs the
 * item-drop machinery around it; the millstone is the corpus' own wheat line), and the press line's press is
 * driven through a gearbox off the 128 RPM stage so that the press, its extras and the millstone share the
 * boiler's domain (SC81).
 *
 * <p>Rates: the millstone at 64 RPM takes 39 ticks per wheat, the press at 128 RPM 19 ticks per ingot, as in
 * {@link MillstoneFlourFactory} and {@link PressSheetsFactory}. The scenario overstresses the network for
 * a while (its machines stop), so the expectations are those of the running state, and the window's judgment
 * is the engine's, over the window after the extras are gone again.
 */
public final class SharedBoilerTwoLinesFactory implements CorpusFactory {
    static final int PRESS_RPM = 128, BELT_RPM = 64, PUMP_MOTOR_RPM = 16;
    static final int MILL_CYCLE = 39, PRESS_CYCLE = 19;
    static final int WHEAT = 1500, INGOTS = 1700;
    static final int WINDOW = 7200;

    static final BlockPos TANK = new BlockPos(10, 4, 13), BURNER = new BlockPos(10, 3, 13), ENGINE = new BlockPos(12, 4, 13);
    static final BlockPos POWERED = new BlockPos(14, 4, 13), PUMP = new BlockPos(7, 4, 14);
    static final BlockPos MILL = new BlockPos(14, 5, 12), CRUSH_IN = new BlockPos(14, 7, 12), CRUSH_OUT = new BlockPos(14, 3, 12);
    static final BlockPos PRESS = new BlockPos(15, 5, 16), PRESS_IN = new BlockPos(13, 5, 16), SHEETS = new BlockPos(15, 1, 16);
    static final BlockPos BELT_START = new BlockPos(13, 3, 16), BELT_END = new BlockPos(14, 3, 16), DEPOT = new BlockPos(15, 3, 16);
    /** The extra presses: along the press's axis, behind it, each in place of a shaft. */
    static final BlockPos EXTRA_FIRST = new BlockPos(15, 5, 17);
    /** The press line's input smart chute, which the edit places again with a filter. */
    static final BlockPos PRESS_CHUTE = new BlockPos(13, 4, 16);
    /** The yard is 34 deep: z 17 to 33. */
    static final int MAX_EXTRA = 17;

    @Override
    public String name() {
        return "shared_boiler";
    }

    @Override
    public int warmupTicks() {
        return 600;
    }

    @Override
    public Built build(GameTestHelper h) {
        BlockState stone = Blocks.STONE.defaultBlockState();
        BlockState water = Blocks.WATER.defaultBlockState();

        // the pump's pool: four sources (x 6..7, z 14..15) on stone at y 0, walled at y 1
        for (int x = 5; x <= 8; x++)
            for (int z = 13; z <= 16; z++)
                set(h, at(h, x, 0, z), stone);
        for (int[] w : new int[][]{{5, 14}, {5, 15}, {6, 13}, {7, 13}, {6, 16}, {7, 16}, {8, 14}, {8, 15}})
            set(h, at(h, w[0], 1, w[1]), stone);
        for (int x = 6; x <= 7; x++)
            for (int z = 14; z <= 15; z++)
                set(h, at(h, x, 1, z), water);

        // the pump's own domain: a creative motor, a shaft and a large cog meshing diagonally with the pump
        motor(h, at(h, 5, 3, 13), Direction.EAST, PUMP_MOTOR_RPM);
        shaft(h, at(h, 6, 3, 13), Direction.Axis.X);
        largeCog(h, at(h, 7, 3, 13), Direction.Axis.X);
        FluidPipeBlock pipe = (FluidPipeBlock) AllBlocks.FLUID_PIPE.get();
        BlockPos pump = at(h, PUMP.getX(), PUMP.getY(), PUMP.getZ());
        set(h, pump, AllBlocks.MECHANICAL_PUMP.getDefaultState().setValue(BlockStateProperties.FACING, Direction.EAST));
        set(h, pump.offset(-1, -2, 0), pipe.getAxisState(Direction.Axis.Y));
        set(h, pump.offset(-1, -1, 0), pipe.getAxisState(Direction.Axis.Y));
        set(h, pump.west(), pipeCorner(pipe, Direction.DOWN, Direction.EAST));
        set(h, pump.east(), pipe.getAxisState(Direction.Axis.X));
        set(h, pump.east(2), pipe.getAxisState(Direction.Axis.X));

        // the boiler: the burner first, then the tank blocks (they join into one tank on placement)
        BlockPos burner = at(h, BURNER.getX(), BURNER.getY(), BURNER.getZ());
        burner(h, burner);
        topUpAsPlayer(h, burner, Items.COAL, 7);
        for (int y = 4; y <= 5; y++)
            for (int x = 10; x <= 11; x++)
                for (int z = 13; z <= 14; z++)
                    set(h, at(h, x, y, z), AllBlocks.FLUID_TANK.getDefaultState());

        // the steam domain: the shaft first (the engine turns it into a powered shaft when placed)
        shaft(h, at(h, POWERED.getX(), POWERED.getY(), POWERED.getZ()), Direction.Axis.Y);
        BlockPos engine = at(h, ENGINE.getX(), ENGINE.getY(), ENGINE.getZ());
        set(h, engine, AllBlocks.STEAM_ENGINE.getDefaultState()
                .setValue(SteamEngineBlock.FACE, AttachFace.WALL).setValue(SteamEngineBlock.FACING, Direction.EAST));
        cog(h, at(h, 14, 5, 13), Direction.Axis.Y);                // 64 RPM, meshes with the millstone
        shaft(h, at(h, 14, 6, 13), Direction.Axis.Y);
        largeCog(h, at(h, 14, 7, 13), Direction.Axis.Y);           // 64 RPM
        cog(h, at(h, 15, 7, 14), Direction.Axis.Y);                // 128 RPM, diagonal to the large cog
        shaft(h, at(h, 15, 6, 14), Direction.Axis.Y);
        set(h, at(h, 15, 5, 14), AllBlocks.GEARBOX.getDefaultState().setValue(BlockStateProperties.AXIS, Direction.Axis.X));
        shaft(h, at(h, 15, 5, 15), Direction.Axis.Z);

        // the crushing line (a millstone line): wheat in a barrel, flour (and seeds) into a barrel
        BlockPos crushIn = at(h, CRUSH_IN.getX(), CRUSH_IN.getY(), CRUSH_IN.getZ());
        BlockPos crushOut = at(h, CRUSH_OUT.getX(), CRUSH_OUT.getY(), CRUSH_OUT.getZ());
        barrel(h, crushOut);
        set(h, at(h, 14, 4, 12), AllBlocks.CHUTE.getDefaultState());
        set(h, at(h, MILL.getX(), MILL.getY(), MILL.getZ()), AllBlocks.MILLSTONE.getDefaultState());
        set(h, at(h, 14, 6, 12), AllBlocks.CHUTE.getDefaultState());
        barrel(h, crushIn);
        fill(h, crushIn, Items.WHEAT, WHEAT);

        // the press line: PressSheetsFactory's, its press on the hub's axis
        BlockPos pressIn = at(h, PRESS_IN.getX(), PRESS_IN.getY(), PRESS_IN.getZ());
        BlockPos sheets = at(h, SHEETS.getX(), SHEETS.getY(), SHEETS.getZ());
        chest(h, pressIn);
        fill(h, pressIn, Items.IRON_INGOT, INGOTS);
        set(h, at(h, PRESS_CHUTE.getX(), PRESS_CHUTE.getY(), PRESS_CHUTE.getZ()), AllBlocks.SMART_CHUTE.getDefaultState());
        belt(h, at(h, BELT_START.getX(), BELT_START.getY(), BELT_START.getZ()), at(h, BELT_END.getX(), BELT_END.getY(), BELT_END.getZ()));
        motor(h, at(h, 13, 3, 15), Direction.SOUTH, -BELT_RPM);
        set(h, at(h, DEPOT.getX(), DEPOT.getY(), DEPOT.getZ()), AllBlocks.DEPOT.getDefaultState());
        BlockPos press = at(h, PRESS.getX(), PRESS.getY(), PRESS.getZ());
        set(h, press, AllBlocks.MECHANICAL_PRESS.getDefaultState().setValue(MechanicalPressBlock.HORIZONTAL_FACING, Direction.NORTH));
        BlockPos sheetChute = at(h, 15, 2, 16);
        set(h, sheetChute, AllBlocks.SMART_CHUTE.getDefaultState());
        filtering(h, sheetChute).setFilter(new ItemStack(AllItems.IRON_SHEET.get()));
        chest(h, sheets);
        // the shafts the extra presses take the place of, out to the yard's end
        for (int i = 0; i < MAX_EXTRA; i++)
            shaft(h, at(h, EXTRA_FIRST.getX(), EXTRA_FIRST.getY(), EXTRA_FIRST.getZ() + i), Direction.Axis.Z);

        double w = TICKS_PER_MINUTE / (double) MILL_CYCLE, r = TICKS_PER_MINUTE / (double) PRESS_CYCLE;
        List<Expect> expect = List.of(
                Expect.exact("minecraft:wheat", -w, "MillstoneBlockEntity at 64 RPM: s 4, ceil(150/4) + 1 = 39 ticks per wheat"),
                Expect.expected("create:wheat_flour", 1.5 * w, 0.375 * w, "milling/wheat: 1 + Binomial(2, 0.25) flour per wheat"),
                Expect.expected("minecraft:wheat_seeds", 0.25 * w, 0.1875 * w, "milling/wheat: Bernoulli(0.25) seeds per wheat"),
                Expect.exact("minecraft:iron_ingot", -r, "PressingBehaviour at 128 RPM: 19 ticks per ingot"),
                Expect.exact("create:iron_sheet", r, "pressing/iron_ingot: one sheet per ingot"));
        // the factory spans up to three chunks along z, and its chain crosses x 13..15 by diagonal cog meshes (no
        // district touch): seed every block entity, as a player's placements would
        seedAll(h, at(h, 5, 0, 12), at(h, 16, 8, 33));
        return new Built(engine, List.of(crushIn, pressIn), List.of(crushOut, sheets), expect, List.of(), WINDOW,
                "boiler level 1 (1 burner, 1 pump at 16 mB/t, size 8), 1 engine at 64 RPM = 16,384 SU; millstone at 64 RPM (256 SU) and "
                        + "press at 128 RPM (1,024 SU) on its domain; the scenario adds presses (1,024 SU each)");
    }

    private static BlockState pipeCorner(FluidPipeBlock pipe, Direction a, Direction b) {
        BlockState s = pipe.defaultBlockState();
        for (Direction d : Direction.values())
            s = s.setValue(PipeBlock.PROPERTY_BY_DIRECTION.get(d), d == a || d == b);
        return s;
    }

    /** [stress, capacity] SU as the block at {@code abs} last heard them (its saved {@code Network} tag), or null. */
    static float[] stress(GameTestHelper h, BlockPos abs) {
        BlockEntity be = h.getLevel().getBlockEntity(abs);
        if (be == null)
            return null;
        CompoundTag n = be.saveWithoutMetadata(h.getLevel().registryAccess()).getCompound("Network");
        return n.isEmpty() ? null : new float[]{n.getFloat("Stress"), n.getFloat("Capacity")};
    }

    static String text(float[] s) {
        return s == null ? "no network" : String.format(Locale.ROOT, "%.0f/%.0f SU", s[0], s[1]);
    }

    /** The three units of the factory, found by the stores and machines each holds. */
    record Units(GlassReport.Unit boiler, GlassReport.Unit crush, GlassReport.Unit press) {}

    /** Among the units whose box holds every position, the one with the fewest nodes. */
    static GlassReport.Unit holdingAll(GlassReport r, BlockPos... ps) {
        GlassReport.Unit best = null;
        for (GlassReport.Unit u : r.units()) {
            boolean all = true;
            for (BlockPos p : ps)
                all &= u.holds(p.getX(), p.getY(), p.getZ());
            if (all && (best == null || u.nodes() < best.nodes()))
                best = u;
        }
        return best;
    }

    /**
     * The scenario: learn, edit the press line's input chute, add presses until the stress passes the capacity, remove
     * them, learn again, and leave for the replay. Every edit is made to both copies in the same tick.
     */
    static final class Check implements GlassReplayGameTests.Scenario {
        enum Phase { LEARN, EDIT, ADD_ONE, ADD_FIT, ADD_CROSS, CLEAN, RELEARN, DONE }

        /** Ticks between reads of the report. */
        static final int READ = 10;
        /** After a recompile is accepted, ticks until the checks look (the domain check ran at the acceptance). */
        static final int SETTLE_AFTER_COMPILE = 40;
        static final int COMPILE_DEADLINE = 1500;
        static final int LEARN_DEADLINE = 9000, RELEARN_DEADLINE = 11000;

        private Phase phase = Phase.LEARN;
        private int step;
        private long startedAt, phaseAt, nextRead, settleAt, deadline, acceptedBefore, learnedAt;
        private boolean boilerLearned, sawStale;
        private long crushReady, boilerReady;
        private int crushDone, boilerDone;
        private float[] stress0, stress1;
        private int placed, fit;
        private long lastLog;

        @Override
        public int extraTicks() {
            return 9000;
        }

        @Override
        public void built(GlassReplayGameTests.Run run) {
            phase = Phase.LEARN;
            step = 0;
            startedAt = run.now();
            nextRead = 0;
            placed = 0;
            fit = 0;
            lastLog = 0;
            learnedAt = -1;
            boilerLearned = false;
            sawStale = false;
        }

        @Override
        public String ready(GlassReplayGameTests.Run run, GlassReport far) {
            return phase == Phase.DONE ? null : "shared boiler scenario in phase " + phase + " step " + step;
        }

        private Units units(GlassReplayGameTests.Run run, GlassReport fr) {
            return new Units(holdingAll(fr, run.far.absolutePos(TANK)),
                    holdingAll(fr, run.far.absolutePos(CRUSH_IN), run.far.absolutePos(CRUSH_OUT)),
                    holdingAll(fr, run.far.absolutePos(PRESS_IN), run.far.absolutePos(SHEETS), run.far.absolutePos(DEPOT)));
        }

        private void go(Phase next, long now) {
            phase = next;
            step = 0;
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
            Units u = units(run, fr);
            switch (phase) {
                case LEARN -> learn(run, fr, u, now);
                case EDIT -> edit(run, fr, u, now);
                case ADD_ONE -> addOne(run, fr, u, now);
                case ADD_FIT -> addFit(run, fr, u, now);
                case ADD_CROSS -> addCross(run, fr, u, now);
                case CLEAN -> clean(run, fr, u, now);
                case RELEARN -> relearn(run, fr, u, now);
                default -> { }
            }
        }

        // ---- LEARN ----------------------------------------------------------------------------

        private void learn(GlassReplayGameTests.Run run, GlassReport fr, Units u, long now) {
            if (now - startedAt > LEARN_DEADLINE) {
                SectionsFx.check(run, "the lines' units learn before the scenario's deadline", false, SectionsFx.describe(fr));
                go(Phase.DONE, now);
                return;
            }
            if (now - lastLog >= 1200) {
                lastLog = now;
                SectionsFx.info(run, "learning", SectionsFx.describe(fr) + "; boiler " + (u.boiler == null ? "none" : u.boiler.summary())
                        + "; stress " + text(stress(run.helper, run.far.absolutePos(PRESS))));
            }
            if (u.crush == null || u.press == null || !SectionsFx.learned(u.crush) || !SectionsFx.learned(u.press))
                return;
            if (u.boiler != null && !SectionsFx.learned(u.boiler)) {
                // The boiler's unit has no goods: give it a while after the lines, then go on without it.
                if (learnedAt < 0)
                    learnedAt = now;
                if (now - learnedAt < 600)
                    return;
            }
            boilerLearned = SectionsFx.learned(u.boiler);
            structure(run, fr, u);
            // the first edit: the press line's input smart chute broken and placed again with a filter of the iron
            // it moves anyway. The materialized filter changes the press section's fingerprint (an edit the
            // recompile finds as it was would only clear the stale mark, choice SC62), at the same revision: a
            // smart chute is no district inventory
            acceptedBefore = fr.checksum().acceptedAt();
            crushReady = u.crush.readyAt();
            crushDone = u.crush.windowsCompleted();
            boilerReady = u.boiler == null ? -1 : u.boiler.readyAt();
            boilerDone = u.boiler == null ? -1 : u.boiler.windowsCompleted();
            BlockPos chute = run.far.absolutePos(PRESS_CHUTE);
            SectionsFx.info(run, "edit at the press line's input smart chute " + SectionsFx.pos(chute), "compile accepted at " + acceptedBefore
                    + "; press unit " + u.press.summary());
            for (BlockPos abs : List.of(run.helper.absolutePos(PRESS_CHUTE), run.far.absolutePos(PRESS_CHUTE))) {
                var level = run.level();
                NeoForge.EVENT_BUS.post(new BlockEvent.BreakEvent(level, abs, level.getBlockState(abs), FakePlayers.editor(level)));
                // the ingot in the chute goes with it (in both copies), not onto the belt as a dropped item
                if (level.getBlockEntity(abs) instanceof ChuteBlockEntity c)
                    c.setItem(ItemStack.EMPTY);
                level.removeBlock(abs, false);
                level.setBlock(abs, AllBlocks.SMART_CHUTE.getDefaultState(), 3);
                filtering(run.helper, abs).setFilter(new ItemStack(Items.IRON_INGOT));
                IndustryKit.playerPlacedBare(run.helper, abs);
            }
            go(Phase.EDIT, now);
            step = 1;
            deadline = now + COMPILE_DEADLINE;
        }

        private void structure(GlassReplayGameTests.Run run, GlassReport fr, Units u) {
            String d = SectionsFx.describe(fr);
            SectionsFx.info(run, "far units", d);
            for (BlockPos rel : List.of(TANK, BURNER, PUMP, ENGINE, POWERED, MILL, CRUSH_IN, CRUSH_OUT, PRESS_IN, DEPOT, SHEETS, BELT_END)) {
                StringBuilder sb = new StringBuilder();
                for (int i = 0; i < fr.units().size(); i++)
                    if (fr.units().get(i).holds(run.far.absolutePos(rel).getX(), run.far.absolutePos(rel).getY(), run.far.absolutePos(rel).getZ()))
                        sb.append(' ').append(i);
                SectionsFx.info(run, "held at " + SectionsFx.pos(rel), "units" + (sb.length() == 0 ? " none" : sb));
            }
            float[] s = stress(run.helper, run.far.absolutePos(PRESS));
            SectionsFx.info(run, "the world", "steam domain " + text(s) + "; powered shaft " + speed(run.helper, run.far.absolutePos(POWERED))
                    + " RPM; millstone " + speed(run.helper, run.far.absolutePos(MILL)) + " RPM; press " + speed(run.helper, run.far.absolutePos(PRESS))
                    + " RPM; pump " + speed(run.helper, run.far.absolutePos(PUMP)) + " RPM; " + boilerText(run));
            SectionsFx.check(run, "the boiler drives the lines: the powered shaft turns and the network is not overstressed",
                    Math.abs(speed(run.helper, run.far.absolutePos(POWERED))) > 1 && !overstressed(run.helper, run.far.absolutePos(PRESS)), text(s));
            SectionsFx.check(run, "the boiler is level 1: 16,384 SU of capacity", s != null && Math.abs(s[1] - 16384) < 1, text(s));
            SectionsFx.check(run, "the report has three units: the boiler's and the two lines'", fr.units().size() == 3, d);
            SectionsFx.check(run, "the crushing line and the press line are units of their own",
                    u.crush != null && u.press != null && u.crush != u.press && u.crush.sections() == 1 && u.press.sections() == 1, d);
            SectionsFx.check(run, "the boiler's section is a unit of its own (a third)",
                    u.boiler != null && u.boiler != u.crush && u.boiler != u.press && u.boiler.sections() == 1, d);
            SectionsFx.check(run, "both lines' units have learned before the edit", SectionsFx.learned(u.crush) && SectionsFx.learned(u.press), d);
            SectionsFx.info(run, "the boiler's unit before the edit", (boilerLearned ? "learned: " : "not learned (it has no goods): ")
                    + (u.boiler == null ? "none" : u.boiler.summary()));
        }

        private String boilerText(GlassReplayGameTests.Run run) {
            BlockPos tank = run.far.absolutePos(TANK);
            if (run.level().getBlockEntity(tank) instanceof FluidTankBlockEntity tb && tb.getControllerBE() != null) {
                FluidTankBlockEntity c = tb.getControllerBE();
                String shaft = run.level().getBlockEntity(run.far.absolutePos(POWERED)) instanceof PoweredShaftBlockEntity ps
                        ? " shaft generated " + ps.getGeneratedSpeed() : "";
                return "boiler size " + c.getTotalTankSize() + " heat " + c.boiler.activeHeat + " water " + c.boiler.waterSupply
                        + " mB/t engines " + c.boiler.attachedEngines + " eff " + c.boiler.getEngineEfficiency(c.getTotalTankSize()) + shaft;
            }
            return "no tank";
        }

        // ---- EDIT: the press line's input chute ----------------------------------------------------

        private void edit(GlassReplayGameTests.Run run, GlassReport fr, Units u, long now) {
            if (step == 1 && fr.checksum().acceptedAt() == acceptedBefore && u.press != null && u.press.stale())
                sawStale = true; // the compile from before the edit, once the engine has heard of it
            if (!compiled(run, fr, "the edit at the press line", now))
                return;
            boolean fresh = u.press != null && u.press.windowsCompleted() == 0 && !u.press.passed();
            if (!fresh && now <= deadline) {
                // a compile already under way at the edit was accepted first: wait for the next
                SectionsFx.info(run, "a compile from before the edit", "accepted at " + fr.checksum().acceptedAt()
                        + "; press unit " + (u.press == null ? "none" : u.press.summary()));
                acceptedBefore = fr.checksum().acceptedAt();
                step = 1;
                return;
            }
            SectionsFx.info(run, "after the edit", "compile accepted at " + fr.checksum().acceptedAt() + " (was " + acceptedBefore
                    + "); the press unit was seen stale before the recompile: " + sawStale);
            String d = SectionsFx.describe(fr);
            SectionsFx.check(run, "the edit makes the press line's unit new: no window done, none passed",
                    u.press != null && u.press.windowsCompleted() == 0 && !u.press.passed(), u.press == null ? d : u.press.summary());
            SectionsFx.check(run, "...and only it: the crushing line's unit keeps its windows and its ready tick",
                    SectionsFx.learned(u.crush) && u.crush.readyAt() == crushReady && u.crush.windowsCompleted() >= crushDone,
                    (u.crush == null ? d : u.crush.summary()) + "; ready at " + (u.crush == null ? -1 : u.crush.readyAt()) + ", was " + crushReady);
            if (boilerLearned)
                SectionsFx.check(run, "...and the boiler's unit keeps its windows and its ready tick",
                        SectionsFx.learned(u.boiler) && u.boiler.readyAt() == boilerReady && u.boiler.windowsCompleted() >= boilerDone,
                        u.boiler == null ? d : u.boiler.summary());
            go(Phase.ADD_ONE, now);
        }

        // ---- ADD: presses --------------------------------------------------------------------------

        private void place(GlassReplayGameTests.Run run, int i) {
            BlockPos rel = EXTRA_FIRST.offset(0, 0, i);
            for (BlockPos abs : List.of(run.helper.absolutePos(rel), run.far.absolutePos(rel))) {
                run.level().setBlock(abs, AllBlocks.MECHANICAL_PRESS.getDefaultState()
                        .setValue(MechanicalPressBlock.HORIZONTAL_FACING, Direction.NORTH), 3);
                IndustryKit.playerPlacedBare(run.helper, abs);
            }
        }

        /** Whether the recompile after the last edit was accepted and the settle passed (the caller then checks). */
        private boolean compiled(GlassReplayGameTests.Run run, GlassReport fr, String what, long now) {
            if (step == 1) {
                if (fr.checksum().acceptedAt() > acceptedBefore) {
                    step = 2;
                    settleAt = now + SETTLE_AFTER_COMPILE;
                } else if (now > deadline) {
                    SectionsFx.check(run, "the district recompiles after " + what, false,
                            "compile accepted at " + fr.checksum().acceptedAt() + " since " + acceptedBefore + ", " + SectionsFx.describe(fr));
                    go(Phase.CLEAN, now);
                }
                return false;
            }
            return step == 2 && now >= settleAt;
        }

        private void addOne(GlassReplayGameTests.Run run, GlassReport fr, Units u, long now) {
            if (step == 0) {
                stress0 = stress(run.helper, run.far.absolutePos(PRESS));
                acceptedBefore = fr.checksum().acceptedAt();
                place(run, 0);
                placed = 1;
                deadline = now + COMPILE_DEADLINE;
                step = 1;
                SectionsFx.info(run, "one press more", "stress before " + text(stress0) + ", compile accepted at " + acceptedBefore);
                return;
            }
            if (!compiled(run, fr, "the first extra press", now))
                return;
            stress1 = stress(run.helper, run.far.absolutePos(PRESS));
            float per = stress0 == null || stress1 == null ? 0 : stress1[0] - stress0[0];
            SectionsFx.info(run, "one press more", "stress " + text(stress1) + ", one press adds " + per + " SU; " + SectionsFx.describe(fr));
            SectionsFx.check(run, "a press adds its impact to the network: 8 x 128 = 1,024 SU", Math.abs(per - 1024) < 1, per);
            if (per < 1 || stress1 == null || stress1[1] <= 0) {
                go(Phase.CLEAN, now);
                return;
            }
            fit = (int) Math.floor((stress1[1] - stress1[0] + 0.5f) / per);
            if (1 + fit + 1 > MAX_EXTRA) {
                SectionsFx.check(run, "the yard has room for the presses that pass the capacity", false, (1 + fit + 1) + " presses of " + MAX_EXTRA);
                go(Phase.CLEAN, now);
                return;
            }
            SectionsFx.info(run, "presses", fit + " more fit under " + text(stress1) + ", then one passes it");
            SectionsFx.check(run, "the crushing line stays learned with one press more", SectionsFx.learned(u.crush), u.crush == null ? "no unit" : u.crush.summary());
            go(fit > 0 ? Phase.ADD_FIT : Phase.ADD_CROSS, now);
        }

        private void addFit(GlassReplayGameTests.Run run, GlassReport fr, Units u, long now) {
            if (step == 0) {
                acceptedBefore = fr.checksum().acceptedAt();
                for (int i = placed; i < placed + fit; i++)
                    place(run, i);
                placed += fit;
                deadline = now + COMPILE_DEADLINE;
                step = 1;
                return;
            }
            if (!compiled(run, fr, "the presses that fit", now))
                return;
            float[] s = stress(run.helper, run.far.absolutePos(PRESS));
            String d = text(s) + " with " + placed + " extra presses; " + SectionsFx.describe(fr);
            SectionsFx.info(run, "just under the capacity", d);
            SectionsFx.check(run, "the network is not overstressed with " + placed + " extra presses", s != null && s[0] <= s[1] && !overstressed(run.helper, run.far.absolutePos(PRESS)), d);
            SectionsFx.check(run, "the stress below the least capacity the window ran at leaves the crushing line's unit learned",
                    SectionsFx.learned(u.crush), u.crush == null ? d : u.crush.summary());
            go(Phase.ADD_CROSS, now);
        }

        private void addCross(GlassReplayGameTests.Run run, GlassReport fr, Units u, long now) {
            if (step == 0) {
                acceptedBefore = fr.checksum().acceptedAt();
                place(run, placed);
                placed++;
                deadline = now + COMPILE_DEADLINE;
                step = 1;
                return;
            }
            if (!compiled(run, fr, "the press that passes the capacity", now))
                return;
            float[] s = stress(run.helper, run.far.absolutePos(PRESS));
            String d = text(s) + " with " + placed + " extra presses; " + SectionsFx.describe(fr);
            SectionsFx.info(run, "past the capacity", d + "; boiler unit " + (u.boiler == null ? "none" : u.boiler.summary()));
            SectionsFx.check(run, "the stress passes the capacity with " + placed + " extra presses", s != null && s[0] > s[1], d);
            SectionsFx.check(run, "the network is overstressed in the world", overstressed(run.helper, run.far.absolutePos(PRESS)), d);
            SectionsFx.check(run, "the press that passes the least capacity the window ran at stales the crushing line's unit too",
                    u.crush != null && u.crush.stale() && !SectionsFx.learned(u.crush), u.crush == null ? d : u.crush.summary());
            go(Phase.CLEAN, now);
        }

        // ---- CLEAN and RELEARN ----------------------------------------------------------------------

        private void clean(GlassReplayGameTests.Run run, GlassReport fr, Units u, long now) {
            for (int i = 0; i < placed; i++) {
                BlockPos rel = EXTRA_FIRST.offset(0, 0, i);
                for (BlockPos abs : List.of(run.helper.absolutePos(rel), run.far.absolutePos(rel))) {
                    NeoForge.EVENT_BUS.post(new BlockEvent.BreakEvent(run.level(), abs, run.level().getBlockState(abs), FakePlayers.editor(run.level())));
                    // the shaft back in its place: the line and its chunks as they were built
                    run.level().setBlock(abs, AllBlocks.SHAFT.getDefaultState().setValue(BlockStateProperties.AXIS, Direction.Axis.Z), 3);
                    IndustryKit.playerPlacedBare(run.helper, abs);
                }
            }
            SectionsFx.info(run, "the extra presses are gone", placed + " removed; " + fuel(run) + "; " + SectionsFx.describe(fr));
            placed = 0;
            go(Phase.RELEARN, now);
            deadline = now + RELEARN_DEADLINE;
            lastLog = now;
            acceptedBefore = fr.checksum().acceptedAt();
        }

        private void relearn(GlassReplayGameTests.Run run, GlassReport fr, Units u, long now) {
            if (now - lastLog >= 1200) {
                lastLog = now;
                SectionsFx.info(run, "learning again", SectionsFx.describe(fr) + "; stress " + text(stress(run.helper, run.far.absolutePos(PRESS))));
            }
            boolean lines = SectionsFx.learned(u.crush) && SectionsFx.learned(u.press);
            boolean all = lines && (!boilerLearned || SectionsFx.learned(u.boiler));
            if (all) {
                SectionsFx.check(run, "the crushing line's unit learns again once the extra presses are gone", SectionsFx.learned(u.crush), u.crush.summary());
                SectionsFx.check(run, "the press line's unit learns again after its edit", SectionsFx.learned(u.press), u.press.summary());
                SectionsFx.info(run, "ready to leave", SectionsFx.describe(fr) + "; " + fuel(run) + "; " + boilerText(run));
                go(Phase.DONE, now);
            } else if (now > deadline) {
                SectionsFx.check(run, "every unit learns again before the deadline", false, SectionsFx.describe(fr));
                go(Phase.DONE, now);
            }
        }

        /** Both copies' burners topped up with coal (a player's hand, the same tick for both), for the rest of the run. */
        private String fuel(GlassReplayGameTests.Run run) {
            StringBuilder sb = new StringBuilder("burners");
            for (BlockPos abs : List.of(run.helper.absolutePos(BURNER), run.far.absolutePos(BURNER))) {
                Level level = run.level();
                int before = level.getBlockEntity(abs) instanceof BlazeBurnerBlockEntity b ? b.getRemainingBurnTime() : -1;
                int n = 0;
                while (n < 8 && BlazeBurnerBlock.tryInsert(level.getBlockState(abs), level, abs, new ItemStack(Items.COAL), false, true, false)
                        .getResult().consumesAction())
                    n++;
                int after = level.getBlockEntity(abs) instanceof BlazeBurnerBlockEntity b ? b.getRemainingBurnTime() : -1;
                sb.append(" ").append(before).append("+").append(n).append(" coal=").append(after);
            }
            return sb.toString();
        }

        // ---- away and after ---------------------------------------------------------------------------

        @Override
        public void away(GlassReplayGameTests.Run run, GlassReport far, boolean last) {
            if (!last)
                return;
            Units u = units(run, far);
            String d = SectionsFx.describe(far) + "; " + far.replay().summary();
            SectionsFx.info(run, "units while away", d);
            SectionsFx.check(run, "the district stays on the glass box while away (never the black box)",
                    run.sawGlass && run.leftGlass == 0 && far.replay().model() == GlassReport.REPLAY_GLASS, far.replay().summary());
            SectionsFx.check(run, "both lines' units replay while away and none is frozen",
                    u.crush != null && u.press != null && u.crush.mayReplay() && u.press.mayReplay() && !u.crush.frozen() && !u.press.frozen(), d);
        }

        @Override
        public void judged(GlassReplayGameTests.Run run, GlassReport far, GlassReport control) {
            Units u = units(run, far);
            SectionsFx.info(run, "units after the reconcile", SectionsFx.describe(far));
            SectionsFx.check(run, "no unit is frozen or stale after the reconcile",
                    u.crush != null && u.press != null && !u.crush.frozen() && !u.crush.stale() && !u.press.frozen() && !u.press.stale(),
                    SectionsFx.describe(far));
        }
    }
}
