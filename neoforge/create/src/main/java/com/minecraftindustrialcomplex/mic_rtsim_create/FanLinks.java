package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.simibubi.create.AllTags;
import com.simibubi.create.content.decoration.copycat.CopycatBlock;
import com.simibubi.create.content.kinetics.belt.behaviour.TransportedItemStackHandlerBehaviour;
import com.simibubi.create.content.kinetics.fan.AirCurrent;
import com.simibubi.create.content.kinetics.fan.EncasedFanBlock;
import com.simibubi.create.content.kinetics.fan.EncasedFanBlockEntity;
import com.simibubi.create.foundation.blockEntity.behaviour.BlockEntityBehaviour;
import com.simibubi.create.infrastructure.config.AllConfigs;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.shapes.Shapes;

import java.util.function.Consumer;

/**
 * An encased fan's air current as a district link (choice G2-73): the fan joins the chunks of what its
 * current works, the hosts (belt segments, depots: {@code TransportedItemStackHandlerBehaviour}) at the
 * current's height or one below; and a host joins the chunks of the fans whose current reaches it, so a
 * fan nobody placed is found from its belt. The glass box's compile visits only the district's chunks: a
 * fan left out has no zones. A blaze burner in the current is not linked: {@link CreatePlugin} does not
 * count burners industrial, and the core drops a link to a block entity that is not industrial.
 *
 * <p>The reach is the current at full speed ({@code AirCurrent.getMaxDistance} at {@code fanRotationArgmax}
 * or faster, the longer of the push and pull distances) cut by the blocks ({@code AirCurrent.getFlowLimit}),
 * not the current at the fan's speed now: speed is state, and membership must not follow a clutch (G2-74).
 * A vertical current stays in its fan's chunk column and links nothing. The core drops targets in the
 * source's own chunk and targets that are not industrial.
 *
 * <p><b>Never loads a chunk, nor waits for one</b> (R2-11): every read goes through {@link Loaded}, and the
 * current stops at the first chunk that is not FULL, as Create's own {@code getFlowLimit} stops at an unloaded
 * one. {@code Level.isLoaded} is not enough: it asks only the chunk's ticket level, so a chunk that is due but
 * still loading passes it, and the read that follows waits for it, running every other chunk load due on the
 * server thread meanwhile (the first detection after a start spent up to 2.2 s in one tick that way, in a
 * host's look). Such a chunk is linked once it loads: its load marks it for the next detection.
 */
final class FanLinks {
    private FanLinks() {}

    /** The longest current any fan makes: the larger of the push and pull distances, at least 3 (the lerp's floor). */
    static float maxReach() {
        var k = AllConfigs.server().kinetics;
        return Math.max(3, Math.max(k.fanPushDistance.get(), k.fanPullDistance.get()));
    }

    /**
     * The blocks the current of the fan at {@code fan} covers at full speed, counted as the compile's {@code fanZones}
     * counts them, cut short of the first chunk that is not loaded ({@code getFlowLimit} reads every block up to the
     * reach, and asks only {@code Level.isLoaded}).
     */
    static int limit(Level level, BlockPos fan, Direction facing) {
        float max = maxReach();
        for (int d = 1; d <= (int) Math.ceil(max); d++)
            if (!Loaded.at(level, fan.relative(facing, d))) {
                max = Math.min(max, d - 1);
                break;
            }
        float flow = AirCurrent.getFlowLimit(level, fan, max, facing);
        return (float) (int) flow == flow ? (int) flow : (int) flow + 1;
    }

    static void links(BlockEntity be, Consumer<BlockPos> out) {
        Level level = be.getLevel();
        if (level == null)
            return;
        if (be instanceof EncasedFanBlockEntity fan)
            fromFan(level, fan.getBlockPos(), fan.getBlockState().getValue(EncasedFanBlock.FACING), out);
        else if (host(be))
            fromHost(level, be.getBlockPos(), out);
    }

    private static boolean host(BlockEntity be) {
        return BlockEntityBehaviour.get(be, TransportedItemStackHandlerBehaviour.TYPE) != null;
    }

    /** The hosts in the current, outside the fan's chunk. */
    private static void fromFan(Level level, BlockPos start, Direction facing, Consumer<BlockPos> out) {
        if (facing.getAxis().isVertical())
            return;
        int cx = start.getX() >> 4, cz = start.getZ() >> 4;
        int limit = limit(level, start, facing);
        for (int d = 1; d <= limit; d++) {
            BlockPos p = start.relative(facing, d);
            if (p.getX() >> 4 == cx && p.getZ() >> 4 == cz)
                continue;
            if (!Loaded.at(level, p))
                return;
            for (int off = 0; off <= 1; off++) {
                BlockPos q = p.below(off);
                BlockEntity be = Loaded.blockEntity(level, q);
                if (be != null && host(be))
                    out.accept(q);
            }
        }
    }

    /**
     * The horizontal fans in other chunks whose current reaches the host at {@code host}: along each
     * horizontal ray from the host's block and the one above it, the first fan facing back, if its own
     * flow limit covers the host. A full block that is not fan-transparent ends a ray, as it ends any
     * current (a copycat never does here: its material decides, and the fan's own flow limit checks it).
     */
    private static void fromHost(Level level, BlockPos host, Consumer<BlockPos> out) {
        int cx = host.getX() >> 4, cz = host.getZ() >> 4;
        int reach = (int) Math.ceil(maxReach());
        for (int off = 0; off <= 1; off++) {
            BlockPos col = host.above(off);
            BlockState here = level.isOutsideBuildHeight(col) ? null : Loaded.state(level, col);
            if (here == null || stops(level, col, here))
                continue;
            for (Direction dir : Direction.Plane.HORIZONTAL)
                for (int k = 1; k <= reach; k++) {
                    BlockPos q = col.relative(dir, k);
                    BlockState s = Loaded.state(level, q);
                    if (s == null)
                        break;
                    if (s.getBlock() instanceof EncasedFanBlock) {
                        Direction facing = dir.getOpposite();
                        if (s.getValue(EncasedFanBlock.FACING) == facing && (q.getX() >> 4 != cx || q.getZ() >> 4 != cz)
                                && limit(level, q, facing) >= k)
                            out.accept(q.immutable());
                        break; // a fan is a full block
                    }
                    if (stops(level, q, s))
                        break;
                }
        }
    }

    /** True when no current passes the block: {@code AirCurrent.getFlowLimit} stops at a full block that is not fan-transparent. */
    private static boolean stops(Level level, BlockPos p, BlockState s) {
        return !AllTags.AllBlockTags.FAN_TRANSPARENT.matches(s) && !(s.getBlock() instanceof CopycatBlock)
                && s.getCollisionShape(level, p) == Shapes.block();
    }

    /**
     * True when {@code other} is a host on {@code a}'s current line within the full-speed reach, {@code a} a
     * fan: the pairs {@link #links} joins. The blocks between are not read (a crossing code).
     */
    static boolean blows(BlockEntity a, BlockEntity other) {
        if (!(a instanceof EncasedFanBlockEntity fan) || !host(other))
            return false;
        Direction facing = fan.getBlockState().getValue(EncasedFanBlock.FACING);
        BlockPos rel = other.getBlockPos().subtract(fan.getBlockPos());
        int along = rel.getX() * facing.getStepX() + rel.getY() * facing.getStepY() + rel.getZ() * facing.getStepZ();
        if (along < 1 || along > Math.ceil(maxReach()))
            return false;
        BlockPos side = rel.subtract(new BlockPos(facing.getStepX() * along, facing.getStepY() * along, facing.getStepZ() * along));
        return side.getX() == 0 && side.getZ() == 0 && (side.getY() == 0 || side.getY() == -1 && facing.getAxis().isHorizontal());
    }
}
