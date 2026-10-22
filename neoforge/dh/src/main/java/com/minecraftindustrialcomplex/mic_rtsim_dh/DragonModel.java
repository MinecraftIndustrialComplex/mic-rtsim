package com.minecraftindustrialcomplex.mic_rtsim_dh;

import com.minecraftindustrialcomplex.mic_rtsim.far.FarState;
import java.util.ArrayList;
import java.util.List;

/**
 * A dragon far away as axis-aligned boxes (research/distant-horizons-2026-10-21.md §4.2): a body (torso, neck
 * and head, tail) and a pair of wings in one of three poses, played as a flipbook. DH boxes cannot rotate
 * (§3.3), so each part is a run cut into a staircase by {@link StaircaseCutter}, for one of
 * {@link #HEADINGS} headings; a dragon turning less than half a step keeps its boxes.
 *
 * <p>Sizes follow the dragon's stage ({@link #length}); the wingspan is {@link #WINGSPAN} lengths. Boxes are
 * relative to the dragon's centre. Plain Java: no DH or Minecraft class.
 */
public final class DragonModel {
    /** Nose to tail by stage 1 to 5, blocks. PLACEHOLDER (by eye from Ice and Fire's render sizes). */
    static final double[] STAGE_LENGTH = {3.0, 3.0, 6.0, 10.0, 14.0, 20.0};
    /** Wingtip to wingtip, in lengths. PLACEHOLDER. */
    public static final double WINGSPAN = 1.4;
    /** Headings a dragon is cut for: 22.5 degrees apart. */
    public static final int HEADINGS = 16;
    /** Client ticks each flipbook frame shows: a wingbeat of four frames is a second. PLACEHOLDER. */
    public static final double FLAP_TICKS = 5.0;

    /** Where the wings are. */
    public enum Pose {
        UP(0.35), LEVEL(0.0), DOWN(-0.30);

        /** How far the tips rise above the shoulders, in lengths. */
        final double rise;

        Pose(double rise) {
            this.rise = rise;
        }
    }

    private DragonModel() {}

    /** A dragon's length at {@code stage} (1 to 5; outside that, the nearest). */
    public static double length(int stage) {
        return STAGE_LENGTH[Math.max(1, Math.min(5, stage))];
    }

    /** The heading step nearest (hx, hz): 0 is +x, counting towards +z. */
    public static int headingStep(double hx, double hz) {
        if (Math.abs(hx) < 1e-9 && Math.abs(hz) < 1e-9)
            return 0;
        double a = Math.atan2(hz, hx);
        return Math.floorMod((int) Math.round(a / (2.0 * Math.PI / HEADINGS)), HEADINGS);
    }

    /** The unit heading of a step: {hx, hz}. */
    public static double[] heading(int step) {
        double a = step * 2.0 * Math.PI / HEADINGS;
        return new double[]{Math.cos(a), Math.sin(a)};
    }

    /** The poses the flipbook plays, in order: up, level, down, level with three; up, down with two. */
    public static Pose[] cycle(int poses) {
        return poses >= 3 ? new Pose[]{Pose.UP, Pose.LEVEL, Pose.DOWN, Pose.LEVEL} : new Pose[]{Pose.UP, Pose.DOWN};
    }

    /** The poses that need a group of their own with {@code poses} frames. */
    public static Pose[] groups(int poses) {
        return poses >= 3 ? new Pose[]{Pose.UP, Pose.LEVEL, Pose.DOWN} : new Pose[]{Pose.UP, Pose.DOWN};
    }

    /**
     * The wing pose at {@code clock} (far clock ticks) for a dragon doing {@code state}, or null with its wings
     * folded (at rest). Each dragon starts its wingbeat at its own point, so a flight does not flap as one.
     */
    public static Pose poseAt(double clock, int poses, long id, FarState state) {
        if (state == FarState.ROOST)
            return null;
        Pose[] c = cycle(poses);
        long frame = (long) Math.floor(clock / FLAP_TICKS) + (id & 0xFF);
        return c[(int) Math.floorMod(frame, c.length)];
    }

    /** The body's boxes (torso, neck and head, tail) for {@code stage} facing heading step {@code h}. */
    public static List<double[]> body(int stage, int h, double step) {
        double l = length(stage);
        double[] f = heading(h);
        List<double[]> out = new ArrayList<>();
        // Torso
        part(out, f, -0.25 * l, 0.0, 0.0, 0.25 * l, 0.0, 0.0, 0.30 * l, 0.28 * l, step);
        // Neck and head, a little up
        part(out, f, 0.2 * l, 0.0, 0.05 * l, 0.5 * l, 0.0, 0.12 * l, 0.14 * l, 0.14 * l, step);
        // Tail, a little down
        part(out, f, -0.2 * l, 0.0, 0.0, -0.5 * l, 0.0, -0.05 * l, 0.10 * l, 0.10 * l, step);
        return out;
    }

    /** Both wings' boxes in {@code pose} for {@code stage} facing heading step {@code h}. */
    public static List<double[]> wings(int stage, int h, Pose pose, double step) {
        double l = length(stage);
        double[] f = heading(h);
        double half = WINGSPAN * l / 2.0;
        double chord = 0.32 * l, thick = Math.max(0.3, 0.04 * l);
        double shoulderY = 0.1 * l, tipY = shoulderY + pose.rise * l;
        List<double[]> out = new ArrayList<>();
        for (int side = -1; side <= 1; side += 2)
            part(out, f, 0.05 * l, side * 0.12 * l, shoulderY, -0.05 * l, side * half, tipY, chord, thick, step);
        return out;
    }

    /**
     * Adds one part: a run from (a forward, a right, ay) to (b forward, b right, by) in the dragon's frame,
     * {@code width} across and {@code height} thick, centred on its line.
     */
    private static void part(List<double[]> out, double[] f, double af, double ar, double ay, double bf, double br,
            double by, double width, double height, double step) {
        // Right of forward (hx, hz) is (-hz, hx) with +z south: which side is which does not matter
        double ax = af * f[0] - ar * f[1], az = af * f[1] + ar * f[0];
        double bx = bf * f[0] - br * f[1], bz = bf * f[1] + br * f[0];
        double mx = (ax + bx) / 2.0, my = (ay + by) / 2.0, mz = (az + bz) / 2.0;
        List<double[]> cut = StaircaseCutter.cut(bx - ax, by - ay, bz - az,
                new StaircaseCutter.Shape(width, height, 0.0, step));
        for (double[] b : cut)
            out.add(new double[]{b[0] + mx, b[1] + my - height / 2.0, b[2] + mz, b[3] + mx, b[4] + my - height / 2.0,
                    b[5] + mz});
    }
}
