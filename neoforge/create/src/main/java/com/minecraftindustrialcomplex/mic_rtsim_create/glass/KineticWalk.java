package com.minecraftindustrialcomplex.mic_rtsim_create.glass;

import com.minecraftindustrialcomplex.mic_rtsim_create.Loaded;
import com.simibubi.create.content.kinetics.base.DirectionalShaftHalvesBlockEntity;
import com.simibubi.create.content.kinetics.base.IRotate;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import com.simibubi.create.content.kinetics.gearbox.GearboxBlockEntity;
import com.simibubi.create.content.kinetics.simpleRelays.ICogWheel;
import com.simibubi.create.content.kinetics.transmission.ClutchBlockEntity;
import com.simibubi.create.content.kinetics.transmission.GearshiftBlockEntity;
import com.simibubi.create.content.kinetics.transmission.SplitShaftBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;

import java.util.ArrayList;
import java.util.List;
import java.util.function.LongPredicate;

/**
 * Create's rotation propagation re-enacted from block states (choice GB51): which kinetic block entities
 * connect and the speed factor between them, as {@code RotationPropagator} computes them, but with each
 * shaft-half block's source given by the walk instead of read live. The compile walks with it where the
 * live networks are not the running line: past a clutch a modelled switch drives (compiled engaged, the
 * switch's CONTROL edge gates it), into block entities that have not joined their network yet (their
 * first tick), and to find what a switch-driven clutch turns. So a compile is the same whether the clutch
 * is on or off at that moment, and before or after a machine's first tick.
 *
 * <p>Read only; server thread; never loads a chunk ({@link #neighbours} skips unloaded positions, as
 * Create's {@code getPotentialNeighbourLocations} does).
 */
final class KineticWalk {
    private KineticWalk() {}

    /**
     * {@code RotationPropagator.getPotentialNeighbourLocations}: the six neighbours that are loaded, then
     * the block entity's own additions ({@code addPropagationLocations}: cogs' diagonals, belts' other end,
     * chain conveyors' connections), kept only where loaded, as {@code RotationPropagatorMixin} keeps them.
     */
    static List<BlockPos> neighbours(KineticBlockEntity be) {
        List<BlockPos> out = new ArrayList<>();
        Level level = be.getLevel();
        BlockPos p = be.getBlockPos();
        if (level == null || !Loaded.at(level, p))
            return out;
        for (Direction d : Direction.values()) {
            BlockPos n = p.relative(d);
            if (Loaded.at(level, n))
                out.add(n);
        }
        BlockState s = be.getBlockState();
        if (!(s.getBlock() instanceof IRotate block))
            return out;
        List<BlockPos> all = new ArrayList<>(be.addPropagationLocations(block, s, out));
        all.removeIf(n -> !Loaded.at(level, n));
        return all;
    }

    /** The side a block entity's live source is on, or null without one. */
    static Direction liveSource(KineticBlockEntity be) {
        BlockPos s = be.source;
        if (s == null)
            return null;
        BlockPos d = s.subtract(be.getBlockPos());
        return Direction.getNearest(d.getX(), d.getY(), d.getZ());
    }

    /** The side of {@code to} that faces {@code from}: {@code to}'s source when the walk enters it from {@code from}. */
    static Direction sideToward(BlockPos to, BlockPos from) {
        BlockPos d = from.subtract(to);
        return Direction.getNearest(d.getX(), d.getY(), d.getZ());
    }

    /**
     * The factor from {@code from}'s speed to {@code to}'s ({@code RotationPropagator.getRotationSpeedModifier}),
     * 0 when they do not connect. {@code fromSource} is the side {@code from} is driven from (null when it
     * has none); {@code to} is driven from {@code from}. Shaft-to-shaft links are computed here with
     * {@link #axisModifier}; every other link (cogs, large cogs, chain drives, belts) is Create's own, which
     * reads no source for them. NaN when Create's method is gone (a Create update): the caller stops.
     *
     * @param engaged positions of clutches and gearshifts compiled unpowered (a modelled switch drives them)
     */
    static float modifier(KineticBlockEntity from, Direction fromSource, KineticBlockEntity to, LongPredicate engaged) {
        BlockState sf = from.getBlockState(), st = to.getBlockState();
        if (!(sf.getBlock() instanceof IRotate df) || !(st.getBlock() instanceof IRotate dt))
            return 0;
        BlockPos diff = to.getBlockPos().subtract(from.getBlockPos());
        Direction dir = Direction.getNearest(diff.getX(), diff.getY(), diff.getZ());
        boolean aligned = true;
        for (Direction.Axis a : Direction.Axis.values())
            if (a != dir.getAxis() && a.choose(diff.getX(), diff.getY(), diff.getZ()) != 0)
                aligned = false;
        Level level = from.getLevel();
        boolean byAxis = aligned && df.hasShaftTowards(level, from.getBlockPos(), sf, dir)
                && dt.hasShaftTowards(level, to.getBlockPos(), st, dir.getOpposite());
        if (!byAxis)
            return CreateGlassAccess.rotationModifier(from, to);
        boolean byGears = ICogWheel.isSmallCog(sf) && ICogWheel.isSmallCog(st);
        float custom = from.propagateRotationTo(to, sf, st, diff, true, byGears);
        if (custom != 0)
            return custom;
        float toMod = axisModifier(to, dir.getOpposite(), dir.getOpposite(), engaged);
        if (toMod != 0)
            toMod = 1 / toMod;
        return axisModifier(from, dir, fromSource, engaged) * toMod;
    }

    /**
     * {@code RotationPropagator.getAxisModifier} with the source given: 1 for anything but a shaft-half
     * block; a gearbox reverses the perpendicular faces it is not driven along; a clutch passes (engaged) or,
     * powered and not driven by a modelled switch, stops the face away from its source; a gearshift,
     * powered, reverses it ({@code ClutchBlockEntity}, {@code GearshiftBlockEntity.getRotationSpeedModifier}).
     * A switch-driven gearshift is compiled unpowered too (UNVERIFIED: no corpus factory has one). Another
     * split shaft (the sequenced gearshift) is asked live when its live source is the walk's, else 1
     * (UNVERIFIED).
     */
    static float axisModifier(KineticBlockEntity be, Direction face, Direction source, LongPredicate engaged) {
        if (!(be instanceof DirectionalShaftHalvesBlockEntity) || source == null)
            return 1;
        if (be instanceof GearboxBlockEntity)
            return face.getAxis() == source.getAxis() ? (face == source ? 1 : -1)
                    : face.getAxisDirection() == source.getAxisDirection() ? -1 : 1;
        if (!(be instanceof SplitShaftBlockEntity s))
            return 1;
        boolean powered = !engaged.test(be.getBlockPos().asLong()) && be.getBlockState().hasProperty(BlockStateProperties.POWERED)
                && be.getBlockState().getValue(BlockStateProperties.POWERED);
        if (be instanceof ClutchBlockEntity)
            return face != source && powered ? 0 : 1;
        if (be instanceof GearshiftBlockEntity)
            return face != source && powered ? -1 : 1;
        return source == liveSource(be) ? s.getRotationSpeedModifier(face) : 1;
    }
}
