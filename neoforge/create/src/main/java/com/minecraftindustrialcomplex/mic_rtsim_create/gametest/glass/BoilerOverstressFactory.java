package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.fluids.pipes.FluidPipeBlock;
import com.simibubi.create.content.fluids.tank.FluidTankBlockEntity;
import com.simibubi.create.content.kinetics.steamEngine.PoweredShaftBlockEntity;
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
 * Corpus 9: a level-2 boiler whose steam network overstresses when one of its two pumps loses its water.
 * Two lit blaze burners heat a 2 x 2 x 2 fluid tank; two mechanical pumps feed it water, each from its own
 * infinite pool; two steam engines on the tank turn one shaft line that drives, through two large-to-small cog
 * steps, four millstones grinding wheat into barrels and twelve idle crushing wheels (ballast that loads the
 * network like busy machines), all at 256 RPM. Nothing in the steam domain has a creative motor. The pumps turn
 * on their own domain: two water wheels on one axle over a channel of flowing water, geared up four times.
 * Mid-window, {@link #perturb} removes pump B's pool; the boiler falls to level 1, the engines' capacity halves
 * below the load, and the network overstresses: every machine stops while the engines keep generating.
 *
 * <pre>
 *   pump domain (x 2..9)                         boiler (x 10..12)             steam domain (x 14..20)
 *   channel x 3..4, y 1, z 9..18, sources (3..4,1,10) tank (10..11, 4..5, 13..14)  powered shafts (14,4,13) (14,5,13), axis Y
 *   water wheels (3,2,12) (4,2,12) axis X         burners (10,3,13) (11,3,14)   large cog L1 (14,6,13): 64 RPM
 *   large cog LA (5,2,12): 8 RPM                  engines (12,4,13) (12,5,13)   cog S1 (15,6,14): 128
 *   cog SA (5,3,13): 16, shaft, large cog LB (7,3,13)   facing E                  shaft, large cog L2 (15,8,14): 128
 *   pump A (7,4,14), pump B (7,4,12), facing E: 32 RPM                          cog S2 (16,8,15): 256
 *   pool A (6..7, 1, 14..15): pipe up (6,2..4,14), pipe (8..9,4,14) into the tank
 *   pool B (6..7, 1, 11..12): pipe up (6,2..4,12), pipe (8..9,4,12), corner (10,4,12) into the tank
 *   millstones (17..20, 8, 15) mesh in a row from S2; chutes above and below; barrels in (y 10), out (y 6)
 *   crushing wheels (16, 2..7, 15) and (16, 9..14, 15) on S2's axis
 * </pre>
 *
 * <p><b>Water</b> ({@code PumpBlockEntity.distributePressureTo}, {@code FluidNetwork.tick}): each pump's pressure
 * is |speed| = 32 on its one branch, so each of the two pipe networks moves max(1, 32 / 2) = 16 mB per tick from
 * its pool (an open pipe end over a source with two source neighbours: {@code OpenEndedPipe.removeFluidFromSpace}
 * never removes it, the vanilla infinite-water rule) into the boiler's handler, which accepts all water and voids
 * it ({@code BoilerData.BoilerFluidHandler}): 32 mB per tick in all. The water wheels turn at 8 RPM
 * ({@code WaterWheelBlockEntity.getGeneratedSpeed}: flow score clamped to 1, x 8) with 2 x 32 x 8 = 512 SU; the
 * two pumps at 32 RPM load 2 x 4 x 32 = 256 SU. Water is not a store's good here: the pools are world water.
 *
 * <p><b>Boiler</b> ({@code BoilerData}): heat 1 per lit burner ({@code BoilerHeaters.blazeBurner}), 2 in all;
 * water level min(18, ceil(32) / 10) = 3; size level min(18, 8 / 4) = 2. Level min(2, 3, 2) = 2. Two engines at
 * level 2: efficiency 1 each ({@code getEngineEfficiency}: engines &lt;= level), so each powered shaft turns at
 * 16 x m = 64 RPM, m = 1 + 3 ({@code PoweredShaftBlockEntity.getGeneratedSpeed}, {@code getSpeedModifier}), with
 * 1 x 1024 / 4 = 256 SU per RPM ({@code calculateAddedStressCapacity}; steam engine capacity 1,024): 16,384 SU
 * each, capacity 32,768 ({@code KineticNetwork.calculateCapacity}: each source's capacity per RPM x
 * |generated speed|). Both engines push one vertical shaft line ({@code SteamEngineBlockEntity.tick} flips an
 * engine's direction to match a shaft that already turns).
 *
 * <p><b>Stress</b> ({@code KineticNetwork.calculateStress}: impact x |theoretical speed|, impacts from
 * {@code AllBlocks}): large to small cog is x -2 ({@code RotationPropagator.getRotationSpeedModifier}), twice:
 * 256 RPM. Four millstones (4 SU per RPM) 4 x 4 x 256 = 4,096; twelve crushing wheels (8) 12 x 8 x 256 = 24,576;
 * cogs and shafts none. Load 28,672 of 32,768 SU: 87.5%, below capacity while watered.
 *
 * <p><b>Running</b> ({@code MillstoneBlockEntity}): s = 256 / 16 = 16, wheat's {@code processing_time} 150,
 * ceil(150 / 16) + 1 = 11 ticks per wheat per millstone: 4 x 1200 / 11 = 436.4 wheat per minute; flour
 * 1 + Binomial(2, 0.25) per wheat (1.5, variance 0.375), seeds Bernoulli(0.25) (0.25, variance 0.1875).
 * Exact (the boiler's heaters have duty 1).
 *
 * <p><b>Burners</b>: filled by hand ("player", {@link SteamHelpers#topUpAsPlayer}): 7 coal reach the 10,000-tick
 * cap ({@code BlazeBurnerBlockEntity.tryUpdateFuel}), more than warm-up plus window (6,600), and stay KINDLED
 * (above 125 ticks left) throughout. They burn whether or not the network turns. Coal is not a store good here.
 *
 * <p><b>Overstress</b>: at window tick {@value #STOP} pool B's four sources become air. Pump B's open pipe end
 * keeps what it drew beyond the transfer (it takes a whole 1,000 mB source and keeps the rest,
 * {@code OpenEndedPipe.OpenEndFluidHandler.drainInner}): at most 984 mB, {@value #BUFFER_TICKS} ticks at 16. The
 * boiler sums its intake over 5-tick samples and every 10th sample (50 ticks) sets the supply to the largest of
 * the last 10 ({@code BoilerData.tick}), so the supply falls from 32 to 16 at the first such reset at least 50
 * ticks after pump B's last water: under {@value #SUPPLY_TICKS} ticks. Water level ceil(16) / 10 = 1, boiler
 * level 1, and each engine's efficiency 1 / 2 (level / engines). The engines read it on their next tick: m = 1 +
 * min(2, floor(4 x 0.5)) = 3, so each shaft generates 16 x 3 = 48 RPM with 0.5 x 1024 / 3 = 170.7 SU per RPM,
 * 8,192 SU each, capacity 16,384 (half). The line slows by 48 / 64 to 192 RPM at the machines, so the load is
 * 0.75 x 28,672 = 21,504 SU (stress uses the theoretical speed, so it stays there): above capacity, and the
 * network is overstressed ({@code KineticBlockEntity.updateFromNetwork}: capacity below stress). Overstress needs
 * the load at 256 RPM above 16,384 / 0.75 = 21,845 SU; this one is 28,672. While overstressed every member's
 * {@code getSpeed()} reads 0, the powered shafts' too, while their generated speed stays 48 RPM and the capacity
 * 16,384 SU: the millstones stop ({@code MillstoneBlockEntity.tick} needs a speed). The chutes under the
 * millstones land their last stacks within {@value #DRAIN_TICKS} ticks. Gap {@value #SETTLE} ticks, then the
 * overstressed phase: every good at 0.
 *
 * <p>Measured (two runs): while watered, supply 32 mB/t, both shafts at 64 RPM, 28,672 / 32,768 SU on the
 * steam network and 256 / 512 SU on the pumps'; overstressed from window tick 2,496 (96 ticks after the cut;
 * UNVERIFIED: pump B's buffer at the cut was not logged), supply 16 mB/t, efficiency 0.5, both shafts generating
 * 48 RPM with {@code getSpeed()} 0, 21,504 / 16,384 SU, the pumps' domain unchanged.
 *
 * <p>Placement trap: relative y 1 is the yard's polished andesite floor and y 0 is cleared to air
 * ({@code StructureUtils.clearSpaceForStructure}), so the channel is dug out of the floor with stone under it.
 * District trap: the factory spans up to three chunks along x, and its chain from the engines to the
 * millstones crosses x 13 to 16 only by diagonal cog meshes, which are not district touches; seeding only the
 * stores and the anchor left the anchor's chunk out whenever a chunk border fell there (the yard's position is
 * random), so {@link SteamHelpers#seedAll} marks every block entity as placed by a player.
 */
public final class BoilerOverstressFactory implements CorpusFactory {
    static final int WINDOW = 6000;
    static final int STOP = 2400;
    /** Water left in pump B's open pipe end at the cut: at most 984 mB at 16 mB per tick. */
    static final int BUFFER_TICKS = (984 + 15) / 16;
    /** The boiler's supply falls at the first 50-tick reset at least 50 ticks after pump B's last water. */
    static final int SUPPLY_TICKS = 100;
    /** The last output stack in each chute under a millstone lands (5 ticks at 0.2 per tick, plus the pull). */
    static final int DRAIN_TICKS = 10;
    static final int SETTLE = BUFFER_TICKS + SUPPLY_TICKS + DRAIN_TICKS;
    static final int MILL_CYCLE = 11;
    static final int MILLS = 4;
    static final int CRUSHERS = 12;
    static final double WHEAT_PER_MIN = MILLS * TICKS_PER_MINUTE / (double) MILL_CYCLE;

    BlockPos tank, engine, shaftA, shaftB, wheel, pumpA, pumpB, firstMill;
    final List<BlockPos> poolB = new ArrayList<>();
    boolean overLogged;

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
        BlockState air = Blocks.AIR.defaultBlockState();

        // relative y 1 is the yard's polished andesite floor and y 0 is cleared to air under it
        // (StructureUtils.clearSpaceForStructure): the channel and pools at y 1 sit on stone at y 0
        for (int x = 2; x <= 8; x++)
            for (int z = 9; z <= 18; z++)
                set(h, at(h, x, 0, z), stone);

        // the pumps' domain: a 2-wide channel of flowing water under two water wheels on one axle
        for (int z = 9; z <= 18; z++) {
            set(h, at(h, 2, 1, z), stone);
            set(h, at(h, 5, 1, z), stone);
        }
        for (int x = 3; x <= 4; x++) {
            set(h, at(h, x, 1, 9), stone);
            set(h, at(h, x, 1, 18), stone);
            for (int z = 11; z <= 17; z++)
                set(h, at(h, x, 1, z), air);
            set(h, at(h, x, 1, 10), water);
        }
        wheel = at(h, 4, 2, 12);
        set(h, at(h, 3, 2, 12), AllBlocks.WATER_WHEEL.getDefaultState().setValue(BlockStateProperties.FACING, Direction.EAST));
        set(h, wheel, AllBlocks.WATER_WHEEL.getDefaultState().setValue(BlockStateProperties.FACING, Direction.EAST));
        largeCog(h, at(h, 5, 2, 12), Direction.Axis.X);
        cog(h, at(h, 5, 3, 13), Direction.Axis.X);
        shaft(h, at(h, 6, 3, 13), Direction.Axis.X);
        largeCog(h, at(h, 7, 3, 13), Direction.Axis.X);

        // two pools (2 x 2 sources each, walled), A at z 14..15 and B at z 11..12, sharing the wall at z 13
        poolB.clear();
        List<BlockPos> poolA = new ArrayList<>();
        for (int x = 6; x <= 7; x++) {
            for (int z : new int[]{10, 13, 16})
                set(h, at(h, x, 1, z), stone);
            for (int z = 14; z <= 15; z++)
                poolA.add(at(h, x, 1, z));
            for (int z = 11; z <= 12; z++)
                poolB.add(at(h, x, 1, z));
        }
        for (int z = 11; z <= 15; z++)
            if (z != 13)
                set(h, at(h, 8, 1, z), stone);
        for (BlockPos p : poolA)
            set(h, p, water);
        for (BlockPos p : poolB)
            set(h, p, water);

        // the boiler: burners first, then the tank blocks (they join into one 2 x 2 x 2 tank on placement)
        BlockPos burnerA = at(h, 10, 3, 13), burnerB = at(h, 11, 3, 14);
        burner(h, burnerA);
        burner(h, burnerB);
        topUpAsPlayer(h, burnerA, Items.COAL, 7);
        topUpAsPlayer(h, burnerB, Items.COAL, 7);
        tank = at(h, 10, 4, 13);
        for (int y = 4; y <= 5; y++)
            for (int x = 10; x <= 11; x++)
                for (int z = 13; z <= 14; z++)
                    set(h, at(h, x, y, z), AllBlocks.FLUID_TANK.getDefaultState());

        // two pump lines; each pump meshes with the large cog LB (7,3,13) on a diagonal
        FluidPipeBlock pipe = (FluidPipeBlock) AllBlocks.FLUID_PIPE.get();
        pumpA = at(h, 7, 4, 14);
        pumpB = at(h, 7, 4, 12);
        for (BlockPos p : List.of(pumpA, pumpB)) {
            set(h, p, AllBlocks.MECHANICAL_PUMP.getDefaultState().setValue(BlockStateProperties.FACING, Direction.EAST));
            set(h, p.offset(-1, -2, 0), pipe.getAxisState(Direction.Axis.Y));
            set(h, p.offset(-1, -1, 0), pipe.getAxisState(Direction.Axis.Y));
            set(h, p.west(), pipeCorner(pipe, Direction.DOWN, Direction.EAST));
            set(h, p.east(), pipe.getAxisState(Direction.Axis.X));
            set(h, p.east(2), pipe.getAxisState(Direction.Axis.X));
        }
        set(h, pumpB.east(3), pipeCorner(pipe, Direction.WEST, Direction.SOUTH));

        // the steam domain: the shafts first (an engine turns its shaft into a powered shaft when placed)
        shaftA = at(h, 14, 4, 13);
        shaftB = at(h, 14, 5, 13);
        shaft(h, shaftA, Direction.Axis.Y);
        shaft(h, shaftB, Direction.Axis.Y);
        engine = at(h, 12, 4, 13);
        for (BlockPos e : List.of(engine, engine.above()))
            set(h, e, AllBlocks.STEAM_ENGINE.getDefaultState()
                    .setValue(SteamEngineBlock.FACE, AttachFace.WALL).setValue(SteamEngineBlock.FACING, Direction.EAST));
        largeCog(h, at(h, 14, 6, 13), Direction.Axis.Y);
        cog(h, at(h, 15, 6, 14), Direction.Axis.Y);
        shaft(h, at(h, 15, 7, 14), Direction.Axis.Y);
        largeCog(h, at(h, 15, 8, 14), Direction.Axis.Y);
        cog(h, at(h, 16, 8, 15), Direction.Axis.Y);
        for (int y = 2; y <= 14; y++)
            if (y != 8)
                set(h, at(h, 16, y, 15), AllBlocks.CRUSHING_WHEEL.getDefaultState().setValue(BlockStateProperties.AXIS, Direction.Axis.Y));

        List<BlockPos> ins = new ArrayList<>(), outs = new ArrayList<>();
        for (int x = 17; x < 17 + MILLS; x++) {
            BlockPos in = at(h, x, 10, 15), out = at(h, x, 6, 15);
            barrel(h, out);
            set(h, at(h, x, 7, 15), AllBlocks.CHUTE.getDefaultState());
            set(h, at(h, x, 8, 15), AllBlocks.MILLSTONE.getDefaultState());
            set(h, at(h, x, 9, 15), AllBlocks.CHUTE.getDefaultState());
            barrel(h, in);
            fill(h, in, Items.WHEAT, 768);
            ins.add(in);
            outs.add(out);
        }
        firstMill = at(h, 17, 8, 15);

        double w = WHEAT_PER_MIN;
        List<Expect> running = List.of(
                Expect.exact("minecraft:wheat", -w,
                        "MillstoneBlockEntity at 256 RPM (engines 64 x 2 x 2): s 16, ceil(150/16) + 1 = 11 ticks per wheat, 4 millstones"),
                Expect.expected("create:wheat_flour", 1.5 * w, 0.375 * w, "milling/wheat: 1 + Binomial(2, 0.25) flour per wheat"),
                Expect.expected("minecraft:wheat_seeds", 0.25 * w, 0.1875 * w, "milling/wheat: Bernoulli(0.25) seeds per wheat"));
        String why = "one pump dry: boiler level 1, 2 engines at efficiency 0.5 give 2 x 8,192 SU at 48 RPM against 21,504 SU at 192 RPM: overstressed";
        List<Expect> over = List.of(
                Expect.exact("minecraft:wheat", 0, why),
                Expect.exact("create:wheat_flour", 0, "overstressed network: millstones at speed 0"),
                Expect.exact("minecraft:wheat_seeds", 0, "overstressed network: millstones at speed 0"));
        overLogged = false;
        // the factory spans up to three chunks along x and its kinetic chain crosses x 13..16 only by
        // diagonal cog meshes, which are no district touch: seed every block entity, as a player's
        // placements would, so the whole factory is one district wherever the chunk borders fall
        seedAll(h, at(h, 2, 1, 9), at(h, 20, 14, 18));
        return new Built(engine, ins, outs, running,
                List.of(new Phase("running", 0, STOP, running), new Phase("overstressed", STOP + SETTLE, WINDOW, over)),
                WINDOW,
                "boiler level 2 (2 burners, 2 pumps x 16 mB/t, 8 blocks), 2 engines x 64 RPM = 32,768 SU; " + MILLS + " millstones and "
                        + CRUSHERS + " crushing wheels at 256 RPM = 28,672 SU; pump B's pool removed at window tick " + STOP
                        + ", overstressed phase from " + (STOP + SETTLE));
    }

    private static BlockState pipeCorner(FluidPipeBlock pipe, Direction a, Direction b) {
        BlockState s = pipe.defaultBlockState();
        for (Direction d : Direction.values())
            s = s.setValue(PipeBlock.PROPERTY_BY_DIRECTION.get(d), d == a || d == b);
        return s;
    }

    @Override
    public void perturb(GameTestHelper h, Built b, int t) {
        if (t == STOP)
            for (BlockPos p : poolB)
                set(h, p, Blocks.AIR.defaultBlockState());
        boolean overNow = t > STOP && overstressed(h, firstMill);
        if (t % 600 == 0 || (overNow && !overLogged)) {
            String boiler = "no tank";
            if (h.getLevel().getBlockEntity(tank) instanceof FluidTankBlockEntity tb && tb.getControllerBE() != null) {
                FluidTankBlockEntity c = tb.getControllerBE();
                boiler = "boiler size " + c.getTotalTankSize() + " heat " + c.boiler.activeHeat + " water " + c.boiler.waterSupply
                        + " mB/t engines " + c.boiler.attachedEngines + " eff " + c.boiler.getEngineEfficiency(c.getTotalTankSize());
            }
            String engines = "";
            for (BlockPos s : List.of(shaftA, shaftB))
                if (h.getLevel().getBlockEntity(s) instanceof PoweredShaftBlockEntity ps)
                    engines += " shaft generated " + ps.getGeneratedSpeed() + " theoretical " + ps.getTheoreticalSpeed() + " speed " + ps.getSpeed() + ";";
            log(name(), "t" + t, (overNow && !overLogged ? "overstressed: " : "") + boiler + ";" + engines
                    + " mill speed " + speed(h, firstMill) + " overstressed " + overstressed(h, firstMill) + " (" + stressText(h, firstMill) + ");"
                    + " wheels " + speed(h, wheel) + " pumps " + speed(h, pumpA) + "/" + speed(h, pumpB) + " (" + stressText(h, wheel) + ")");
            overLogged |= overNow;
        }
    }
}
