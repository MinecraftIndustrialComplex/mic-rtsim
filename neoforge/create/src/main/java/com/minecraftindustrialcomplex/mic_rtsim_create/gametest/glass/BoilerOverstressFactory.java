package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.fluids.pipes.FluidPipeBlock;
import com.simibubi.create.content.fluids.tank.FluidTankBlockEntity;
import com.simibubi.create.content.kinetics.steamEngine.SteamEngineBlock;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.PipeBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.AttachFace;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;

import java.util.ArrayList;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.*;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.SteamHelpers.*;

/**
 * Corpus 9: a boiler and a steam engine that stop when the water stops. Two lit blaze burners heat a
 * 2 x 2 x 1 fluid tank; a mechanical pump feeds it water from an infinite pool; one steam engine on the
 * tank turns, through two large-to-small cog steps, four millstones grinding wheat into barrels and five
 * idle crushing wheels (ballast that loads the network like busy machines), all at 256 RPM. Nothing in the
 * steam domain has a creative motor. The pump turns on its own domain: a water wheel on a channel of
 * flowing water, geared up four times. Mid-window, {@link #perturb} removes the pool; the boiler's level
 * falls to 0, the engine stops and so does every machine.
 *
 * <pre>
 *   pump domain (x 3..9)                      boiler (x 10..12)          steam domain (x 14..20)
 *   channel x 4, y 1, z 9..18, source (4,1,10)  tank (10..11, 4, 13..14)   powered shaft P (14,4,13), axis Y
 *   water wheel (4,2,12) axis X                 burners (10,3,13) (11,3,14) large cog L1 (14,5,13): 64 RPM
 *   large cog LA (5,2,12): 8 RPM                engine (12,4,13) facing E   cog S1 (15,5,14): 128
 *   cog SA (5,3,13): 16, shaft, large cog LB (7,3,13)                     shaft, large cog L2 (15,7,14): 128
 *   pump (7,4,14) facing E: 32 RPM                                        cog S2 (16,7,15): 256
 *   pool (6..7, 1, 14..15), pipe up (6,2..4,14), pipe (8..9,4,14) into the tank
 *   millstones (17..20, 7, 15) mesh in a row from S2; chutes above and below; barrels in (y 9), out (y 5)
 *   crushing wheels (16, 8..12, 15) on S2's axis
 * </pre>
 *
 * <p><b>Water</b> ({@code PumpBlockEntity.distributePressureTo}, {@code FluidNetwork.tick}): the pump's
 * pressure is |speed| = 32 on its one branch, so the network moves max(1, 32 / 2) = 16 mB per tick from the
 * pool (an open pipe end over a source with two source neighbours: {@code OpenEndedPipe.removeFluidFromSpace}
 * never removes it, the vanilla infinite-water rule) into the boiler's handler, which accepts all water and
 * voids it ({@code BoilerData.BoilerFluidHandler}). The water wheel turns at 8 RPM
 * ({@code WaterWheelBlockEntity.getGeneratedSpeed}: flow score clamped to 1, x 8) with 32 x 8 = 256 SU; the
 * pump at 32 RPM loads 4 x 32 = 128 SU. Water is not a store's good here: the pool is world water.
 *
 * <p><b>Boiler</b> ({@code BoilerData}): heat 1 per lit burner ({@code BoilerHeaters.blazeBurner}), 2 in all;
 * water level min(18, ceil(16) / 10) = 1; size level min(18, 4 / 4) = 1. Level min(2, 1, 1) = 1. One engine
 * at level 1: efficiency 1 ({@code getEngineEfficiency}), so the powered shaft turns at 16 x 4 = 64 RPM with
 * 1024 x 1 / 4 = 256 SU per RPM ({@code PoweredShaftBlockEntity.getGeneratedSpeed}, {@code getSpeedModifier},
 * {@code calculateAddedStressCapacity}; steam engine capacity 1,024): 16,384 SU.
 *
 * <p><b>Stress</b> ({@code KineticNetwork.calculateStress}, impacts from {@code AllBlocks}): large to small cog
 * is x -2 ({@code RotationPropagator.getRotationSpeedModifier}), twice: 256 RPM. Four millstones (4 SU per RPM)
 * 4 x 4 x 256 = 4,096; five crushing wheels (8) 5 x 8 x 256 = 10,240; cogs and shafts none. 14,336 of
 * 16,384 SU: 87.5%, below capacity (overstress needs capacity below stress).
 *
 * <p><b>Running</b> ({@code MillstoneBlockEntity}): s = 256 / 16 = 16, wheat's {@code processing_time} 150,
 * ceil(150 / 16) + 1 = 11 ticks per wheat per millstone: 4 x 1200 / 11 = 436.4 wheat per minute; flour
 * 1 + Binomial(2, 0.25) per wheat (1.5, variance 0.375), seeds Bernoulli(0.25) (0.25, variance 0.1875).
 * Exact (the boiler's heaters have duty 1: the burners are filled by hand to 10,000 ticks, more than
 * warm-up plus window).
 *
 * <p><b>Stopping</b>: at window tick {@value #STOP} the pool's four sources become air. The open pipe end
 * still holds what it drew last (it draws 1,000 mB and keeps the rest: at most 984 mB, 62 ticks at 16).
 * The boiler samples its intake every 5 ticks and every 10th sample sets the supply to the largest of the
 * last 10 ({@code BoilerData.tick}), so the supply reaches 0 within 105 ticks of the last water. Then the
 * level is 0, the engine's efficiency 0 ({@code getEngineEfficiency}: heat but no level), the shaft's
 * capacity and speed 0, and the domain stops. Gap {@value #SETTLE} ticks (at least twice the 62 + 105 + 1
 * bound), then the stopped phase: every good at 0.
 */
public final class BoilerOverstressFactory implements CorpusFactory {
    static final int WINDOW = 6000;
    static final int STOP = 2400;
    static final int SETTLE = 400;
    static final int MILL_CYCLE = 11;
    static final int MILLS = 4;
    static final double WHEAT_PER_MIN = MILLS * TICKS_PER_MINUTE / (double) MILL_CYCLE;

    BlockPos tank, engine, shaft, wheel, pump, firstMill;
    final List<BlockPos> pool = new ArrayList<>();

    @Override
    public String name() {
        return "boiler_overstress";
    }

    @Override
    public int warmupTicks() {
        return 600;
    }

    @Override
    public Built build(GameTestHelper h) {
        BlockState stone = Blocks.STONE.defaultBlockState();
        BlockState water = Blocks.WATER.defaultBlockState();

        // the pump's domain: a channel of flowing water under a water wheel, geared up to the pump
        // relative y 1 is the yard's floor layer and y 0 is cleared to air under it
        // (StructureUtils.clearSpaceForStructure): the water at y 1 needs stone under it or it falls away
        for (int x = 3; x <= 8; x++)
            for (int z = 9; z <= 18; z++)
                set(h, at(h, x, 0, z), stone);
        for (int z = 9; z <= 18; z++) {
            set(h, at(h, 3, 1, z), stone);
            set(h, at(h, 5, 1, z), stone);
        }
        set(h, at(h, 4, 1, 9), stone);
        set(h, at(h, 4, 1, 18), stone);
        set(h, at(h, 4, 1, 10), water);
        wheel = at(h, 4, 2, 12);
        set(h, wheel, AllBlocks.WATER_WHEEL.getDefaultState().setValue(BlockStateProperties.FACING, Direction.EAST));
        largeCog(h, at(h, 5, 2, 12), Direction.Axis.X);
        cog(h, at(h, 5, 3, 13), Direction.Axis.X);
        shaft(h, at(h, 6, 3, 13), Direction.Axis.X);
        largeCog(h, at(h, 7, 3, 13), Direction.Axis.X);

        // the pool (2 x 2 sources, walled) and the pipes
        pool.clear();
        for (int x = 6; x <= 7; x++) {
            set(h, at(h, x, 1, 13), stone);
            set(h, at(h, x, 1, 16), stone);
            for (int z = 14; z <= 15; z++)
                pool.add(at(h, x, 1, z));
        }
        set(h, at(h, 8, 1, 14), stone);
        set(h, at(h, 8, 1, 15), stone);
        for (BlockPos p : pool)
            set(h, p, water);

        // the boiler: burners first, then the tank blocks (they join into one 2 x 2 tank on placement)
        BlockPos burnerA = at(h, 10, 3, 13), burnerB = at(h, 11, 3, 14);
        burner(h, burnerA);
        burner(h, burnerB);
        topUpAsPlayer(h, burnerA, Items.COAL, 7);
        topUpAsPlayer(h, burnerB, Items.COAL, 7);
        tank = at(h, 10, 4, 13);
        for (int x = 10; x <= 11; x++)
            for (int z = 13; z <= 14; z++)
                set(h, at(h, x, 4, z), AllBlocks.FLUID_TANK.getDefaultState());

        pump = at(h, 7, 4, 14);
        set(h, pump, AllBlocks.MECHANICAL_PUMP.getDefaultState().setValue(BlockStateProperties.FACING, Direction.EAST));
        FluidPipeBlock pipe = (FluidPipeBlock) AllBlocks.FLUID_PIPE.get();
        set(h, at(h, 6, 2, 14), pipe.getAxisState(Direction.Axis.Y));
        set(h, at(h, 6, 3, 14), pipe.getAxisState(Direction.Axis.Y));
        BlockState corner = pipe.defaultBlockState();
        for (Direction d : Direction.values())
            corner = corner.setValue(PipeBlock.PROPERTY_BY_DIRECTION.get(d), d == Direction.DOWN || d == Direction.EAST);
        set(h, at(h, 6, 4, 14), corner);
        set(h, at(h, 8, 4, 14), pipe.getAxisState(Direction.Axis.X));
        set(h, at(h, 9, 4, 14), pipe.getAxisState(Direction.Axis.X));

        // the steam domain: the shaft first (the engine turns it into a powered shaft when placed)
        shaft = at(h, 14, 4, 13);
        shaft(h, shaft, Direction.Axis.Y);
        engine = at(h, 12, 4, 13);
        set(h, engine, AllBlocks.STEAM_ENGINE.getDefaultState()
                .setValue(SteamEngineBlock.FACE, AttachFace.WALL).setValue(SteamEngineBlock.FACING, Direction.EAST));
        largeCog(h, at(h, 14, 5, 13), Direction.Axis.Y);
        cog(h, at(h, 15, 5, 14), Direction.Axis.Y);
        shaft(h, at(h, 15, 6, 14), Direction.Axis.Y);
        largeCog(h, at(h, 15, 7, 14), Direction.Axis.Y);
        cog(h, at(h, 16, 7, 15), Direction.Axis.Y);
        for (int y = 8; y <= 12; y++)
            set(h, at(h, 16, y, 15), AllBlocks.CRUSHING_WHEEL.getDefaultState().setValue(BlockStateProperties.AXIS, Direction.Axis.Y));

        List<BlockPos> ins = new ArrayList<>(), outs = new ArrayList<>();
        for (int x = 17; x < 17 + MILLS; x++) {
            BlockPos in = at(h, x, 9, 15), out = at(h, x, 5, 15);
            barrel(h, out);
            set(h, at(h, x, 6, 15), AllBlocks.CHUTE.getDefaultState());
            set(h, at(h, x, 7, 15), AllBlocks.MILLSTONE.getDefaultState());
            set(h, at(h, x, 8, 15), AllBlocks.CHUTE.getDefaultState());
            barrel(h, in);
            fill(h, in, Items.WHEAT, 768);
            ins.add(in);
            outs.add(out);
        }
        firstMill = at(h, 17, 7, 15);

        double w = WHEAT_PER_MIN;
        List<Expect> running = List.of(
                Expect.exact("minecraft:wheat", -w,
                        "MillstoneBlockEntity at 256 RPM (engine 64 x 2 x 2): s 16, ceil(150/16) + 1 = 11 ticks per wheat, 4 millstones"),
                Expect.expected("create:wheat_flour", 1.5 * w, 0.375 * w, "milling/wheat: 1 + Binomial(2, 0.25) flour per wheat"),
                Expect.expected("minecraft:wheat_seeds", 0.25 * w, 0.1875 * w, "milling/wheat: Bernoulli(0.25) seeds per wheat"));
        List<Expect> stopped = List.of(
                Expect.exact("minecraft:wheat", 0, "no water: boiler level 0, engine efficiency 0, domain at 0 RPM"),
                Expect.exact("create:wheat_flour", 0, "domain stopped"),
                Expect.exact("minecraft:wheat_seeds", 0, "domain stopped"));
        return new Built(engine, ins, outs, running,
                List.of(new Phase("running", 0, STOP, running), new Phase("stopped", STOP + SETTLE, WINDOW, stopped)),
                WINDOW,
                "boiler level 1 (2 burners, 16 mB/t, 4 blocks), 1 engine 64 RPM 16,384 SU; 14,336 SU of millstones and crushing wheels at 256 RPM; water removed at window tick " + STOP);
    }

    @Override
    public void perturb(GameTestHelper h, Built b, int t) {
        if (t == 0) {
            StringBuilder sb = new StringBuilder();
            for (int[] r : new int[][]{{4, 0, 10}, {4, 1, 10}, {4, 1, 11}, {4, 1, 12}, {4, 1, 13}, {6, 1, 14}, {7, 1, 15}, {6, 2, 14}, {4, 2, 12}}) {
                BlockPos q = h.absolutePos(new BlockPos(r[0], r[1], r[2]));
                sb.append(" (").append(r[0]).append(',').append(r[1]).append(',').append(r[2]).append(") ").append(h.getLevel().getBlockState(q));
            }
            log(name(), "t0 blocks", sb.toString());
        }
        if (t == STOP)
            for (BlockPos p : pool)
                set(h, p, Blocks.AIR.defaultBlockState());
        if (t % 200 == 0 || (t >= STOP && t < STOP + 300 && t % 10 == 0)) {
            String boiler = "no tank";
            if (h.getLevel().getBlockEntity(tank) instanceof FluidTankBlockEntity tb && tb.getControllerBE() != null) {
                FluidTankBlockEntity c = tb.getControllerBE();
                boiler = "size " + c.getTotalTankSize() + " heat " + c.boiler.activeHeat + " water " + c.boiler.waterSupply
                        + " engines " + c.boiler.attachedEngines + " eff " + c.boiler.getEngineEfficiency(c.getTotalTankSize());
            }
            String flow = h.getLevel().getBlockEntity(wheel) instanceof com.simibubi.create.content.kinetics.waterwheel.WaterWheelBlockEntity ww
                    ? "flow " + ww.flowScore + " fluid " + h.getLevel().getFluidState(wheel.below()) + " " + h.getLevel().getFluidState(wheel.below()).getFlow(h.getLevel(), wheel.below())
                    : "no wheel " + h.getLevel().getBlockState(wheel);
            log(name(), "t" + t, boiler + "; " + flow + " wheel stress " + stressText(h, wheel) + " LA " + speed(h, wheel.east()) + " SA " + speed(h, wheel.east().above().south())
                    + " pump state " + h.getLevel().getBlockState(pump) + "; wheel " + speed(h, wheel) + " pump " + speed(h, pump) + " shaft " + speed(h, shaft)
                    + " mill " + speed(h, firstMill) + " stress " + stressText(h, firstMill));
        }
    }
}
