package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim_create.mixin.LevelChunkInvoker;
import com.simibubi.create.foundation.blockEntity.IMultiBlockEntityContainer;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.level.material.Fluids;
import net.neoforged.neoforge.capabilities.BlockCapability;

/**
 * World reads that never load a chunk (SECTIONS.md §2.8, slice S1). Vanilla's {@code Level.getBlockEntity},
 * {@code getBlockState}, {@code getFluidState} and NeoForge's three-argument {@code getCapability} all go
 * through {@code getChunkAt}, which loads a chunk that is not loaded; {@code Level.isLoaded} asks only the
 * chunk's ticket level, so a chunk that is due but still loading passes it and the read then waits for the
 * load. Every read here asks {@code getChunkNow} first: a chunk that is not FULL is not read, and the
 * caller sees "not loaded" (null).
 *
 * <p>A position "ticks" when vanilla's block entity loop would tick a block entity there ({@link #ticks}).
 * Server levels only: on a client or a virtual level (Ponder) every test answers as Create would.
 */
public final class Loaded {
    private Loaded() {}

    /** The FULL chunk holding {@code p}, or null when it is not loaded (or {@code p} is outside the build height). */
    public static LevelChunk chunk(Level level, BlockPos p) {
        if (level.isOutsideBuildHeight(p))
            return null;
        return level.getChunkSource().getChunkNow(p.getX() >> 4, p.getZ() >> 4);
    }

    /** Whether {@code p}'s chunk is loaded (FULL). */
    public static boolean at(Level level, BlockPos p) {
        return chunk(level, p) != null;
    }

    /**
     * Whether a kinetic network's guarded recount keeps its listed {@code k} although {@code found}, another
     * object, is at {@code k}'s position now (PATCHES.md row 53): {@code found} is a block entity of the same
     * network that has not joined yet (its chunk is FULL and has not ticked). The recount
     * ({@code KineticNetworkMixin}) and the compile ({@code CreateCompile.listed}) both ask this, so they count
     * the same block entities.
     */
    public static boolean keptFor(com.simibubi.create.content.kinetics.KineticNetwork net,
            com.simibubi.create.content.kinetics.base.KineticBlockEntity k, BlockEntity found) {
        return found != k && found instanceof com.simibubi.create.content.kinetics.base.KineticBlockEntity o
                && !net.members.containsKey(o) && net.id != null && net.id.equals(o.network);
    }

    /** {@code Level.getBlockEntity} where the chunk is loaded; null where it is not. */
    public static BlockEntity blockEntity(Level level, BlockPos p) {
        LevelChunk c = chunk(level, p);
        return c == null ? null : c.getBlockEntity(p, LevelChunk.EntityCreationType.IMMEDIATE);
    }

    /** {@code Level.getBlockState} where the chunk is loaded (void air above or below the build height); null where it is not. */
    public static BlockState state(Level level, BlockPos p) {
        if (level.isOutsideBuildHeight(p))
            return Blocks.VOID_AIR.defaultBlockState();
        LevelChunk c = level.getChunkSource().getChunkNow(p.getX() >> 4, p.getZ() >> 4);
        return c == null ? null : c.getBlockState(p);
    }

    /** {@code Level.getFluidState} where the chunk is loaded; empty where it is not. */
    public static FluidState fluid(Level level, BlockPos p) {
        LevelChunk c = chunk(level, p);
        return c == null ? Fluids.EMPTY.defaultFluidState() : c.getFluidState(p);
    }

    /**
     * {@code Level.getCapability(cap, p, context)} where the chunk is loaded, through the five-argument
     * form with the chunk's state and block entity (which loads nothing itself). Null where the chunk is not
     * loaded, and for a Create multiblock (a vault, a tank) with any chunk of its box not loaded: a member's
     * handler is its controller's ({@code getControllerBE}, a {@code Level.getBlockEntity}), and a vault
     * controller builds its handler from every member ({@code ConnectivityHandler.partAt}, a
     * {@code Level.getBlockEntity} per block of the box).
     */
    public static <T, C> T capability(Level level, BlockCapability<T, C> cap, BlockPos p, C context) {
        LevelChunk c = chunk(level, p);
        if (c == null)
            return null;
        BlockEntity be = c.getBlockEntity(p, LevelChunk.EntityCreationType.IMMEDIATE);
        if (be instanceof IMultiBlockEntityContainer m && !multiblockLoaded(level, p, m))
            return null;
        return level.getCapability(cap, p, c.getBlockState(p), be, context);
    }

    /**
     * Whether every chunk of {@code m}'s box is loaded: from its controller, the box's least corner,
     * {@code getHeight} blocks along its main axis and {@code getWidth} along the other two (as
     * {@code ConnectivityHandler} lays a multiblock out).
     */
    private static boolean multiblockLoaded(Level level, BlockPos p, IMultiBlockEntityContainer m) {
        BlockPos at = m.isController() ? p : m.getController();
        if (at == null)
            return true;
        if (!(blockEntity(level, at) instanceof IMultiBlockEntityContainer controller))
            return chunk(level, at) != null;
        Direction.Axis axis = controller.getMainConnectionAxis();
        int length = Math.max(1, controller.getHeight()), width = Math.max(1, controller.getWidth());
        int dx = axis == Direction.Axis.X ? length : width;
        int dz = axis == Direction.Axis.Z ? length : width;
        for (int cx = at.getX() >> 4; cx <= (at.getX() + dx - 1) >> 4; cx++)
            for (int cz = at.getZ() >> 4; cz <= (at.getZ() + dz - 1) >> 4; cz++)
                if (level.getChunkSource().getChunkNow(cx, cz) == null)
                    return false;
        return true;
    }

    /**
     * Whether a block entity at {@code p} would tick now: vanilla's two tests for a bound ticker,
     * {@code Level.shouldTickBlocksAt} (made by {@code Level.tickBlockEntities}) and
     * {@code LevelChunk.isTicking} (world border, full status at least BLOCK_TICKING, entities loaded, and the
     * core's reconcile gate), on a chunk that is loaded.
     */
    public static boolean ticks(Level level, BlockPos p) {
        if (!level.shouldTickBlocksAt(ChunkPos.asLong(p)))
            return false;
        LevelChunk c = chunk(level, p);
        return c != null && ((LevelChunkInvoker) c).mic_rtsim$isTicking(p);
    }

    /**
     * Whether a block entity ticking at {@code from} may look at {@code to} (the chain conveyor and network
     * guards, PATCHES.md rows 50 to 52): true on a client or a virtual level, and in {@code from}'s own chunk
     * (which ticks, since {@code from} does); else {@link #ticks}.
     */
    public static boolean ticksFrom(Level level, BlockPos from, BlockPos to) {
        if (!(level instanceof net.minecraft.server.level.ServerLevel))
            return true;
        if ((from.getX() >> 4) == (to.getX() >> 4) && (from.getZ() >> 4) == (to.getZ() >> 4))
            return true;
        return ticks(level, to);
    }
}
