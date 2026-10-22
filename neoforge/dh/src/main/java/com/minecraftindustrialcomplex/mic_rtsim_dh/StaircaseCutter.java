package com.minecraftindustrialcomplex.mic_rtsim_dh;

import java.util.ArrayList;
import java.util.List;

/**
 * Cuts a carriage into axis-aligned boxes (research/distant-horizons-2026-10-21.md §4.1). DH boxes cannot
 * rotate (§3.3), so a carriage on straight, level, axis-aligned track is one cuboid, and anything else (a
 * diagonal, a slope, a curve's chord) is a staircase of short cuboids along the run.
 *
 * <p>The run goes from the trailing point to the leading point, plus {@link Shape#overhang} past each end
 * (Create's points are bogey wheels, and the body sticks out past them). The boxes are relative to the run's
 * midpoint at rail height: a renderer puts its group's origin there and moves only the origin while the
 * heading holds.
 *
 * <p>On a diagonal each step's box is the bounding box of a {@code step} long piece of the carriage, with the
 * width part shrunk so the staircase is {@link Shape#width} wide across the track (corner to corner), not
 * wider: for a step {@code s}, width {@code w} and heading {@code θ}, the box is
 * {@code s|cos θ| + w k |sin θ|} by {@code s|sin θ| + w k |cos θ|}, with {@code k = 1 - s |sin 2θ| / w}.
 * Neighbouring steps overlap along the track, so the staircase has no gaps.
 *
 * <p>Plain Java: no DH or Minecraft class, so it is unit tested without either.
 */
public final class StaircaseCutter {
    /** How far a run may drift off the nearest axis, or rise, over its whole length and still be one box. */
    public static final double STRAIGHT_TOLERANCE = 0.25;
    /** The most boxes one run is cut into; a longer run gets longer steps. */
    public static final int MAX_BOXES = 48;

    private StaircaseCutter() {}

    /**
     * The look of a carriage.
     *
     * @param width    across the track, in blocks
     * @param height   from the rail up, in blocks
     * @param overhang past each end point along the track, in blocks
     * @param step     the longest a staircase step may be, in blocks
     */
    public record Shape(double width, double height, double overhang, double step) {
        public Shape {
            if (!(width > 0 && height > 0 && overhang >= 0 && step > 0))
                throw new IllegalArgumentException("bad carriage shape " + width + " x " + height + ", overhang "
                        + overhang + ", step " + step);
        }
    }

    /**
     * Cuts the run {@code (dx, dy, dz)} (leading point minus trailing point).
     *
     * @return the boxes, each {min x, min y, min z, max x, max y, max z} relative to the run's midpoint;
     *         empty if the run is not a finite vector
     */
    public static List<double[]> cut(double dx, double dy, double dz, Shape shape) {
        List<double[]> boxes = new ArrayList<>();
        if (!Double.isFinite(dx) || !Double.isFinite(dy) || !Double.isFinite(dz))
            return boxes;
        double w = shape.width(), h = shape.height();
        double flat = Math.hypot(dx, dz);
        double length = flat + 2.0 * shape.overhang();

        // Standing on end (or no length at all): one box around the midpoint
        if (flat < 1e-6) {
            double half = Math.max(w, length) / 2.0;
            double rise = Math.abs(dy) / 2.0;
            boxes.add(new double[] {-half, -rise, -half, half, rise + h, half});
            return boxes;
        }

        double ux = dx / flat, uz = dz / flat;
        double slope = dy / flat;
        double cos = Math.abs(ux), sin = Math.abs(uz);
        boolean onAxis = Math.min(cos, sin) * length < STRAIGHT_TOLERANCE;
        boolean level = Math.abs(slope) * length < STRAIGHT_TOLERANCE;

        if (onAxis) {
            // Snap to the axis, so a run that is nearly straight is cut straight
            if (cos >= sin) {
                ux = Math.signum(ux);
                uz = 0.0;
            } else {
                ux = 0.0;
                uz = Math.signum(uz);
            }
            cos = Math.abs(ux);
            sin = Math.abs(uz);
            if (level) {
                double alongX = cos * length + sin * w;
                double alongZ = sin * length + cos * w;
                boxes.add(new double[] {-alongX / 2.0, 0.0, -alongZ / 2.0, alongX / 2.0, h, alongZ / 2.0});
                return boxes;
            }
        }

        int n = (int) Math.ceil(length / shape.step() - 1e-9);
        n = Math.max(1, Math.min(MAX_BOXES, n));
        double s = length / n;
        double sin2 = 2.0 * cos * sin;
        // The width share shrinks on diagonals so the staircase is w across, corner to corner (see above)
        double k = Math.max(0.1, 1.0 - s * sin2 / w);
        double ax = s * cos + w * k * sin;
        double az = s * sin + w * k * cos;
        for (int i = 0; i < n; i++) {
            double t = -length / 2.0 + (i + 0.5) * s;
            double cx = t * ux, cz = t * uz, cy = t * slope;
            boxes.add(new double[] {cx - ax / 2.0, cy, cz - az / 2.0, cx + ax / 2.0, cy + h, cz + az / 2.0});
        }
        return boxes;
    }
}
