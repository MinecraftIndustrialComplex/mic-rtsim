package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.conflict.RailAxes;
import com.simibubi.create.content.trains.track.FakeTrackBlock;
import com.simibubi.create.content.trains.track.ITrackBlock;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.Mth;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.Vec3;

/**
 * Which way Create's rail runs at a point (M8, MZB-360, the re-check's P2), for the core's lurk placement
 * ({@link RailAxes}): the track blocks within a block of the point (the lurk's track point is a bogey's, on the
 * rail), each with its shape's axes (a straight piece, a diagonal one, a slope; its horizontal direction), the line
 * through the block's centre nearest the point. A piece with two axes (a crossing or a junction), two lines of
 * different directions equally near, or a point farther than {@link #ON_RAIL} from every line (a curve's middle: a
 * bezier piece has no axis of its own there) is no answer, and the core keeps the engine's spot.
 */
public final class RailDirections implements RailAxes {
    /** How far from a block's centre line a point may be to be on that rail (blocks). */
    public static final double ON_RAIL = 0.75;
    /** Two lines this close (blocks) are as near as each other. */
    private static final double TIE = 0.05;

    @Override
    public Vec3 at(ServerLevel level, Vec3 point) {
        return axisAt(level, point);
    }

    /**
     * Whether a piece of Create track is within {@code radius} blocks (horizontal, block centre to point) of the point and
     * in the blocks from the point's own (the body's feet) up to {@code height} above it (its head): any
     * {@code ITrackBlock} (straight, diagonal, slope, crossing, junction) or a {@code FakeTrackBlock}, the block Create
     * fills a curve's span with (a bezier piece has an {@code ITrackBlock} only at its two ends). A rail below the feet
     * is not counted (the third check's P3: a body on an embankment beside a lower line is not refused for it). Not
     * loaded counts as no track.
     */
    @Override
    public boolean trackNear(ServerLevel level, Vec3 point, double radius, double height) {
        int bx = Mth.floor(point.x), by = Mth.floor(point.y), bz = Mth.floor(point.z);
        int top = Mth.floor(point.y + Math.max(height, 0.0));
        int r = Mth.ceil(radius) + 1;
        double r2 = radius * radius;
        BlockPos.MutableBlockPos at = new BlockPos.MutableBlockPos();
        for (int dx = -r; dx <= r; dx++)
            for (int dz = -r; dz <= r; dz++) {
                double cx = bx + dx + 0.5 - point.x, cz = bz + dz + 0.5 - point.z;
                if (cx * cx + cz * cz > r2 || !level.hasChunk((bx + dx) >> 4, (bz + dz) >> 4))
                    continue;
                for (int y = by; y <= top; y++) {
                    at.set(bx + dx, y, bz + dz);
                    if (isTrack(level.getBlockState(at)))
                        return true;
                }
            }
        return false;
    }

    private static boolean isTrack(BlockState state) {
        return state.getBlock() instanceof ITrackBlock || state.getBlock() instanceof FakeTrackBlock;
    }

    /** The horizontal unit direction of the rail at {@code point}, or null (see the class doc). */
    public static Vec3 axisAt(ServerLevel level, Vec3 point) {
        int bx = Mth.floor(point.x), by = Mth.floor(point.y), bz = Mth.floor(point.z);
        double best = Double.POSITIVE_INFINITY;
        double ux = 0.0, uz = 0.0;
        boolean ambiguous = false, junction = false;
        BlockPos.MutableBlockPos at = new BlockPos.MutableBlockPos();
        for (int dx = -1; dx <= 1; dx++)
            for (int dz = -1; dz <= 1; dz++)
                for (int dy = -1; dy <= 1; dy++) {
                    at.set(bx + dx, by + dy, bz + dz);
                    if (!level.hasChunk(at.getX() >> 4, at.getZ() >> 4))
                        continue;
                    BlockState state = level.getBlockState(at);
                    if (!(state.getBlock() instanceof ITrackBlock track))
                        continue;
                    java.util.List<Vec3> axes = track.getTrackAxes(level, at, state);
                    if (axes.size() != 1) {
                        // A crossing or a junction: the point is as near as any of its lines
                        for (Vec3 a : axes)
                            if (lineDistance(point, at, a) <= ON_RAIL)
                                junction = true;
                        continue;
                    }
                    Vec3 axis = axes.get(0);
                    double len = Math.hypot(axis.x, axis.z);
                    if (len < 1.0e-6)
                        continue;
                    double d = lineDistance(point, at, axis);
                    double vx = axis.x / len, vz = axis.z / len;
                    if (d < best - TIE) {
                        best = d;
                        ux = vx;
                        uz = vz;
                        ambiguous = false;
                    } else if (d <= best + TIE && Math.abs(ux * vz - uz * vx) > 0.05) {
                        ambiguous = true;
                    }
                }
        if (junction || ambiguous || best > ON_RAIL)
            return null;
        return new Vec3(ux, 0.0, uz);
    }

    /** The horizontal distance from {@code point} to the line through the block's centre along {@code axis}. */
    private static double lineDistance(Vec3 point, BlockPos block, Vec3 axis) {
        double len = Math.hypot(axis.x, axis.z);
        if (len < 1.0e-6)
            return Double.POSITIVE_INFINITY;
        double cx = block.getX() + 0.5, cz = block.getZ() + 0.5;
        return Math.abs((point.x - cx) * (-axis.z / len) + (point.z - cz) * (axis.x / len));
    }
}
