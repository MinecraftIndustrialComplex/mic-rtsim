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
 * Exact (the boiler's heaters have duty 1). Measured while watered: 14,336 / 16,384 SU at the millstones'
 * network, 128 / 256 SU on the pump's.
 *
 * <p><b>Burners</b>: filled by hand ("player", {@link SteamHelpers#topUpAsPlayer}): 7 coal reach the 10,000-tick cap
 * ({@code BlazeBurnerBlockEntity.tryUpdateFuel}), more than warm-up plus window (6,600), and stay KINDLED (above
 * 125 ticks left) throughout. They burn whether or not the boiler has water. Coal is not a store good here.
 *
 * <p><b>Stopping</b>: at window tick {@value #STOP} the pool's four sources become air. The open pipe end keeps
 * what it drew beyond the transfer (it takes a whole 1,000 mB source and keeps the rest,
 * {@code OpenEndedPipe.OpenEndFluidHandler.drainInner}): at most 984 mB, {@value #BUFFER_TICKS} ticks at 16. The
 * boiler sums its intake over 5-tick samples and every 10th sample (50 ticks) sets the supply to the largest of
 * the last 10 ({@code BoilerData.tick}), so the supply reaches 0 at the first such reset at least 50 ticks after
 * the last water: under {@value #SUPPLY_TICKS} ticks. Then the level is 0 and the engine's efficiency 0
 * ({@code getEngineEfficiency}: heat 2 but actual heat 0, so 0 / 1 engines), the powered shaft's speed and
 * capacity 0, and the whole steam domain stops in that tick: it stops, it does not overstress (stress needs a
 * running source). The chutes under the millstones land their last stacks within {@value #DRAIN_TICKS} ticks.
 * Gap {@value #SETTLE} ticks, then the stopped phase: every good at 0. Measured: the shaft stopped between
 * window ticks 2,490 and 2,500 (the buffer was nearly empty at the cut).
 *
 * <p>Placement trap: relative y 1 is the yard's polished andesite floor and y 0 is cleared to air
 * ({@code StructureUtils.clearSpaceForStructure}), so the channel is dug out of the floor with stone under it.
 */
public final class BoilerOverstressFactory implements CorpusFactory {
    static final int WINDOW = 6000;
    static final int STOP = 2400;
    /** Water left in the open pipe end at the cut: at most 984 mB at 16 mB per tick. */
    static final int BUFFER_TICKS = (984 + 15) / 16;
    /** The boiler's supply reaches 0 at the first 50-tick reset at least 50 ticks after the last water. */
    static final int SUPPLY_TICKS = 100;
    /** The last output stack in each chute under a millstone lands (5 ticks at 0.2 per tick, plus the pull). */
    static final int DRAIN_TICKS = 10;
    static final int SETTLE = BUFFER_TICKS + SUPPLY_TICKS + DRAIN_TICKS;
    static final int MILL_CYCLE = 11;
    static final int MILLS = 4;
    static final double WHEAT_PER_MIN = MILLS * TICKS_PER_MINUTE / (double) MILL_CYCLE;

    BlockPos tank, engine, shaft, wheel, pump, firstMill;
    boolean stopLogged;
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
        // relative y 1 is the yard's polished andesite floor and y 0 is cleared to air under it
        // (StructureUtils.clearSpaceForStructure): the channel at y 1 is dug out of the floor, with stone under it
        for (int x = 3; x <= 8; x++)
            for (int z = 9; z <= 18; z++)
                set(h, at(h, x, 0, z), stone);
        for (int z = 9; z <= 18; z++) {
            set(h, at(h, 3, 1, z), stone);
            set(h, at(h, 5, 1, z), stone);
        }
        set(h, at(h, 4, 1, 9), stone);
        set(h, at(h, 4, 1, 18), stone);
        for (int z = 11; z <= 17; z++)
            set(h, at(h, 4, 1, z), Blocks.AIR.defaultBlockState());
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
        stopLogged = false;
        return new Built(engine, ins, outs, running,
                List.of(new Phase("running", 0, STOP, running), new Phase("stopped", STOP + SETTLE, WINDOW, stopped)),
                WINDOW,
                "boiler level 1 (2 burners, 16 mB/t, 4 blocks), 1 engine 64 RPM 16,384 SU; 14,336 SU of millstones and crushing wheels at 256 RPM;"
                        + " pump domain: water wheel 8 RPM, 128 of 256 SU; water removed at window tick " + STOP + ", stopped phase from " + (STOP + SETTLE));
    }

    @Override
    public void perturb(GameTestHelper h, Built b, int t) {
        if (t == STOP)
            for (BlockPos p : pool)
                set(h, p, Blocks.AIR.defaultBlockState());
        boolean stoppedNow = t > STOP && speed(h, shaft) == 0;
        if (t % 600 == 0 || (stoppedNow && !stopLogged)) {
            String boiler = "no tank";
            if (h.getLevel().getBlockEntity(tank) instanceof FluidTankBlockEntity tb && tb.getControllerBE() != null) {
                FluidTankBlockEntity c = tb.getControllerBE();
                boiler = "boiler size " + c.getTotalTankSize() + " heat " + c.boiler.activeHeat + " water " + c.boiler.waterSupply
                        + " mB/t engines " + c.boiler.attachedEngines + " eff " + c.boiler.getEngineEfficiency(c.getTotalTankSize());
            }
            log(name(), "t" + t, (stoppedNow && !stopLogged ? "line stopped: " : "") + boiler + "; wheel " + speed(h, wheel) + " pump " + speed(h, pump)
                    + " (" + stressText(h, wheel) + "); shaft " + speed(h, shaft) + " mill " + speed(h, firstMill) + " (" + stressText(h, firstMill) + ")");
            stopLogged |= stoppedNow;
        }
    }
}
