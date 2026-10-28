package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs;

/**
 * A copy of the grid walk of the jar's {@code MMPathNavigateGround.sweep} (Mowzie's Mobs 1.8.2, decompiled with vineflower),
 * the {@code float} one MZB-950 found running away, with its loop body behind {@link PathSweep.Columns} so that the shipped walk
 * and this one can be run on the same world. Nothing is changed but that: the arithmetic is the jar's (float, world
 * coordinates, the {@code 1.0E-8} nudge, {@code !=} loops).
 */
final class JarFloatSweep {
    private JarFloatSweep() {}

    private static final float EPSILON = 1.0E-8f;

    private static int floor(float f) {
        int i = (int) f;
        return f < i ? i - 1 : i;
    }

    private static int leadEdgeToInt(float coord, int step) {
        return floor(coord - (float) step * EPSILON);
    }

    private static int trailEdgeToInt(float coord, int step) {
        return floor(coord + (float) step * EPSILON);
    }

    static boolean sweep(double[] vec, double[] base, double[] max, PathSweep.Columns columns) {
        float t = 0.0f;
        float maxT = (float) Math.sqrt(vec[0] * vec[0] + vec[1] * vec[1] + vec[2] * vec[2]);
        if (maxT < EPSILON)
            return true;
        float[] tr = new float[3], tDelta = new float[3], tNext = new float[3], normed = new float[3];
        int[] ldi = new int[3], tri = new int[3], step = new int[3];
        for (int i = 0; i < 3; i++) {
            float value = (float) vec[i];
            boolean dir = value >= 0.0f;
            step[i] = dir ? 1 : -1;
            float lead = (float) (dir ? max : base)[i];
            tr[i] = (float) (dir ? base : max)[i];
            ldi[i] = leadEdgeToInt(lead, step[i]);
            tri[i] = trailEdgeToInt(tr[i], step[i]);
            normed[i] = value / maxT;
            tDelta[i] = Math.abs(maxT / value);
            float dist = dir ? (float) (ldi[i] + 1) - lead : lead - (float) ldi[i];
            tNext[i] = tDelta[i] < Float.POSITIVE_INFINITY ? tDelta[i] * dist : Float.POSITIVE_INFINITY;
        }
        do {
            int axis = tNext[0] < tNext[1] ? (tNext[0] < tNext[2] ? 0 : 2) : (tNext[1] < tNext[2] ? 1 : 2);
            float dt = tNext[axis] - t;
            t = tNext[axis];
            ldi[axis] += step[axis];
            tNext[axis] += tDelta[axis];
            for (int i = 0; i < 3; i++) {
                tr[i] += dt * normed[i];
                tri[i] = trailEdgeToInt(tr[i], step[i]);
            }
            int x0 = axis == 0 ? ldi[0] : tri[0], x1 = ldi[0] + step[0];
            int y0 = axis == 1 ? ldi[1] : tri[1], y1 = ldi[1] + step[1];
            int z0 = axis == 2 ? ldi[2] : tri[2], z1 = ldi[2] + step[2];
            for (int x = x0; x != x1; x += step[0])
                for (int z = z0; z != z1; z += step[2])
                    if (!columns.column(x, z, y0, y1, step[1]))
                        return false;
        } while (t <= maxT);
        return true;
    }
}
