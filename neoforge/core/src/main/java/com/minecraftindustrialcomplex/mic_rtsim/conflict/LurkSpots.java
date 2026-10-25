package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import net.minecraft.world.phys.Vec3;

import java.util.ArrayList;
import java.util.List;

/**
 * Where a lurking body stands (M8, MZB-360, the re-check's P2): the rail registry and the world-vector face of
 * {@link LurkGeometry}, which holds the rule and the numbers (on the rail's normal through the lurk's track point,
 * {@code SPOT_BLOCKS} from the rail's centre line, on the den's side, a block centre).
 */
public final class LurkSpots {
    private LurkSpots() {}

    /** The distance from the rail's centre line at which a lurking body stands: the engine's {@code SPOT_BLOCKS} ({@link LurkGeometry#SPOT_BLOCKS}). */
    public static final double SPOT_BLOCKS = LurkGeometry.SPOT_BLOCKS;
    /** See {@link LurkGeometry#LATERAL_SLACK}. */
    public static final double LATERAL_SLACK = LurkGeometry.LATERAL_SLACK;
    /** See {@link LurkGeometry#OBLIQUE_NEAR_SLACK}. */
    public static final double OBLIQUE_NEAR_SLACK = LurkGeometry.OBLIQUE_NEAR_SLACK;

    private static volatile RailAxes axes;

    /** Registers what knows the rail's direction (the create plugin); a later call replaces an earlier one. */
    public static void register(RailAxes a) {
        axes = a;
    }

    /** What knows the rail's direction, or null when no plugin does. */
    public static RailAxes axes() {
        return axes;
    }

    /** {@link LurkGeometry#candidates} in world vectors: each point has the track's y; the caller finds the floor. */
    public static List<Vec3> candidates(Vec3 track, Vec3 spot, Vec3 axis) {
        List<Vec3> out = new ArrayList<>();
        for (double[] p : LurkGeometry.candidates(track.x, track.z, spot.x, spot.z, axis.x, axis.z))
            out.add(new Vec3(p[0], track.y, p[1]));
        return out;
    }
}
