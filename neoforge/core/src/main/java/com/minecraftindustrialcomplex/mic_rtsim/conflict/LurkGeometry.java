package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;

/**
 * Where a lurking body stands (M8, MZB-360, the re-check's P2): on the rail's normal through the lurk's track point,
 * {@link #SPOT_BLOCKS} from the rail's centre line, on the den's side. The geometry alone, in doubles, so that JUnit
 * runs it without Minecraft ({@link LurkSpots} is its Minecraft face and holds the rail registry).
 *
 * <p>The engine's {@code spot} in AMBUSH_SPOT is the record and the chunk it names, rounded to whole blocks along the
 * line from the track point to the den. That is {@code SPOT_BLOCKS} from the point and square to the rail only for a
 * rail that runs along an axis and a den square to it: a diagonal rail puts it 5.66 off, a den off the line by more
 * than a few blocks puts the sample's line oblique to the rail, and a den within a block of the line puts it 5 blocks
 * along +x, on the track for an east-west rail. The strike and the clearance (MZB-374's contract) are about the
 * body's distance to the rail's centre line, so Java places the body from the rail's direction instead, and the
 * engine's spot only gives the side.
 *
 * <p>A body's centre can rest only on a block's centre (the navigation ends a wide body's path there), so the point
 * is a block centre whose distance to the centre line is within {@link #LATERAL_SLACK} of {@code SPOT_BLOCKS} (exact
 * on a rail along an axis; up to 0.354 off on a diagonal), within {@link #ALONG_MAX} of the track point along the rail, the
 * nearest to both first. The den's side first, then the other; the bridge takes the first that is standable.
 */
public final class LurkGeometry {
    private LurkGeometry() {}

    /**
     * Mirror of {@code SPOT_BLOCKS} in the engine's {@code crates/rtsim-conflict/src/engine/rail.rs}: the distance from
     * the rail's centre line at which a lurking body stands. The engine owns the number and a Rust test pins it
     * (tests/lurk.rs); the create plugin's {@code TrainSampler.SPOT_BLOCKS} is this one.
     */
    public static final double SPOT_BLOCKS = 5.0;
    /**
     * How far from {@link #SPOT_BLOCKS} a body's distance to the centre line may be (blocks). The block centres of a
     * diagonal rail's side lie on lines 1/sqrt(2) = 0.707 apart, so the nearest is up to 0.354 off whatever the track
     * point's fraction; 0.4 always finds one. The contract counts it ({@code TrainSampler.spotContract}).
     */
    public static final double LATERAL_SLACK = 0.4;
    /** How far along the rail from the track point a body may stand (blocks): the strike does not care where along the rail. */
    public static final double ALONG_MAX = 1.5;
    /** A spot this close to the rail's line (its component along the normal, blocks) is on the line: the den's side is not known. */
    public static final double ON_LINE = 0.5;

    /**
     * The block centres a body may rest at, best first: the den's side, then the other, each as {@code {x, z}}.
     * {@code (axisX, axisZ)} is the rail's direction at the track point (any length or sign); {@code (spotX, spotZ)}
     * is the engine's spot, whose side of the rail is the den's (a spot within {@link #ON_LINE} of the line takes the
     * rail's left-hand side, looking along the axis, then the right). Empty when the axis has no horizontal length.
     */
    public static List<double[]> candidates(double trackX, double trackZ, double spotX, double spotZ, double axisX, double axisZ) {
        double len = Math.hypot(axisX, axisZ);
        if (!(len > 1.0e-6) || !Double.isFinite(len))
            return List.of();
        double ax = axisX / len, az = axisZ / len;
        double nx = -az, nz = ax;
        double side = (spotX - trackX) * nx + (spotZ - trackZ) * nz;
        int s = Math.abs(side) < ON_LINE ? 1 : (side > 0.0 ? 1 : -1);
        List<double[]> out = new ArrayList<>();
        for (int k = 0; k < 2; k++) {
            int sd = k == 0 ? s : -s;
            double cx = trackX + sd * nx * SPOT_BLOCKS, cz = trackZ + sd * nz * SPOT_BLOCKS;
            List<double[]> found = new ArrayList<>();
            for (int bx = (int) Math.floor(cx) - 2; bx <= (int) Math.floor(cx) + 2; bx++)
                for (int bz = (int) Math.floor(cz) - 2; bz <= (int) Math.floor(cz) + 2; bz++) {
                    double px = bx + 0.5, pz = bz + 0.5;
                    double lateral = sd * ((px - trackX) * nx + (pz - trackZ) * nz);
                    double along = (px - trackX) * ax + (pz - trackZ) * az;
                    double off = Math.abs(lateral - SPOT_BLOCKS);
                    if (off > LATERAL_SLACK + 1.0e-9 || Math.abs(along) > ALONG_MAX + 1.0e-9)
                        continue;
                    found.add(new double[] {off, Math.abs(along), px, pz});
                }
            found.sort(Comparator.<double[]>comparingLong(f -> Math.round(f[0] * 1000.0))
                    .thenComparingLong(f -> Math.round(f[1] * 1000.0))
                    .thenComparingDouble(f -> f[2])
                    .thenComparingDouble(f -> f[3]));
            for (double[] f : found)
                out.add(new double[] {f[2], f[3]});
        }
        return out;
    }

    /** A point's distance to the rail's centre line through the track point along the axis (blocks, horizontal). */
    public static double lateral(double trackX, double trackZ, double axisX, double axisZ, double x, double z) {
        double len = Math.hypot(axisX, axisZ);
        return Math.abs((x - trackX) * (-axisZ / len) + (z - trackZ) * (axisX / len));
    }
}
