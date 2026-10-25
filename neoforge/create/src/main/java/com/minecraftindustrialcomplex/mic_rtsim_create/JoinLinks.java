package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim_create.glass.CreateGlassAccess;
import com.simibubi.create.content.kinetics.RotationPropagator;
import com.simibubi.create.content.kinetics.base.IRotate;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorBlockEntity;
import com.simibubi.create.content.kinetics.mechanicalArm.ArmBlockEntity;
import com.simibubi.create.content.kinetics.mechanicalArm.ArmInteractionPoint;
import com.simibubi.create.content.logistics.depot.EjectorBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;

import java.util.ArrayList;
import java.util.List;
import java.util.function.Consumer;

/**
 * Create's joins between chunks that are not a face touch (SECTIONS.md §2.1, slice S6), besides a fan's current
 * ({@link FanLinks}):
 * <ul>
 *   <li><b>J2.</b> A mechanical arm's interaction points ({@code ArmBlockEntity.inputs} and {@code outputs}) and a
 *       weighted ejector's target ({@code EjectorBlockEntity.getTargetPosition}, arithmetic on its launcher)
 *       move goods across a border without touching (choice SC4). An arm's points are empty until its first
 *       tick reads them from its saved tag ({@link CreateGlassAccess#armPointsPending}): such an arm's links are
 *       not known ({@link #known}), and its chunk keeps the joins its district saved.</li>
 *   <li><b>J4.</b> Rotation across a border that is not a face: a cog's diagonals and any class that overrides
 *       {@code addPropagationLocations}, confirmed by Create's own {@code RotationPropagator.isConnected} either
 *       way, as {@code findConnectedNeighbour} asks (choice SC3). A chain conveyor is never a join (§2.6): its
 *       rotation between districts is a shared network.</li>
 * </ul>
 * Read only; never loads a chunk: a target whose chunk is not loaded is skipped (the core skips it too), and a
 * saved join stands for it (§2.3).
 */
final class JoinLinks {
    private JoinLinks() {}

    /** Whether {@code be}'s links can be read now: false for an arm whose saved points are not read yet. */
    static boolean known(BlockEntity be) {
        return !(be instanceof ArmBlockEntity arm) || !CreateGlassAccess.armPointsPending(arm);
    }

    static void links(BlockEntity be, Consumer<BlockPos> out) {
        if (be instanceof ArmBlockEntity arm) {
            points(CreateGlassAccess.armInputs(arm), out);
            points(CreateGlassAccess.armOutputs(arm), out);
        } else if (be instanceof EjectorBlockEntity ejector) {
            BlockPos t = ejector.getTargetPosition();
            if (t != null && !t.equals(be.getBlockPos()))
                out.accept(t);
        }
        if (be instanceof KineticBlockEntity k)
            rotation(k, out);
    }

    private static void points(List<ArmInteractionPoint> points, Consumer<BlockPos> out) {
        if (points != null)
            for (ArmInteractionPoint p : points)
                out.accept(p.getPos());
    }

    /**
     * J4: where {@code k} propagates into another chunk beyond its faces, to a loaded kinetic block entity it is
     * connected to. Only a block on its chunk's border row can have a diagonal outside its chunk; an overriding
     * class (Ender Transmission's transmitters) is asked wherever it is.
     */
    private static void rotation(KineticBlockEntity k, Consumer<BlockPos> out) {
        if (k instanceof ChainConveyorBlockEntity)
            return;
        BlockPos p = k.getBlockPos();
        int lx = p.getX() & 15, lz = p.getZ() & 15;
        boolean border = lx == 0 || lx == 15 || lz == 0 || lz == 15;
        if (!border && !StaleKinetics.ownPropagation(k))
            return;
        BlockState state = k.getBlockState();
        Level level = k.getLevel();
        if (level == null || !(state.getBlock() instanceof IRotate block))
            return;
        for (BlockPos n : k.addPropagationLocations(block, state, new ArrayList<>())) {
            if ((n.getX() >> 4) == (p.getX() >> 4) && (n.getZ() >> 4) == (p.getZ() >> 4))
                continue;
            LevelChunk chunk = Loaded.chunk(level, n);
            if (chunk == null || !(chunk.getBlockEntities().get(n) instanceof KineticBlockEntity kn) || kn instanceof ChainConveyorBlockEntity)
                continue;
            if (RotationPropagator.isConnected(k, kn) || RotationPropagator.isConnected(kn, k))
                out.accept(n);
        }
    }
}
