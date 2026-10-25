package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import java.util.ArrayList;
import java.util.List;

/**
 * The march's column (MZB-469, ledger 25): how a materialized band's entries split into stacks and where each stack
 * stands in the column, as pure numbers so a unit test can check them without a server.
 *
 * <p>An entry splits into stacks of one row each: up to two members side by side, ROW behind the row before. A body
 * whose maker says it travels as a pack (the raptor) gets one stack per member, a pair abreast at plus and minus
 * {@link #PACK_SIDE}, each pair {@link #PACK_ROW} behind the last. The war chief is a stack of its own. Met on the
 * march (the band appears at the arc's centre, radius 0) the entries line up one behind the other in record order and
 * the chief rides at the rear of all; on an arc each entry keeps its own column from its arc point, its side the
 * entry's spread, and its chief at the rear of its own rows.
 *
 * <p>A stack's place is {@code -h * back + r * side} from the head, with {@code h} the column's heading and
 * {@code r = (-hz, hx)} its right.
 */
final class MarchColumn {
    /** A raptor pack's two stand this far to either side of the column's line, and each pair this far behind the last (blocks, PLACEHOLDER). */
    static final double PACK_SIDE = 3.0, PACK_ROW = 6.0;

    /** One stack of the column: its entry, its members (1 or 2), whether it is the chief's, and its place. */
    record Plan(int entry, int slots, boolean chief, boolean pack, double back, double side) {}

    private MarchColumn() {}

    /**
     * The column's stacks in the order they are made. {@code counts[i]} is entry i's units; an entry with none and no
     * chief makes no stack. {@code met}: the band appears at the head (radius 0).
     */
    static List<Plan> plan(int[] counts, boolean[] pack, boolean[] chief, boolean met) {
        int m = counts.length;
        List<Plan> out = new ArrayList<>();
        double cursor = 0;
        List<Integer> rear = new ArrayList<>();
        for (int i = 0; i < m; i++) {
            if (counts[i] == 0 && !chief[i])
                continue;
            double side0 = met ? 0.0 : (i - (m - 1) / 2.0) * ConflictBridge.STACK_SPACING;
            double c = met ? cursor : 0.0;
            int n = counts[i];
            if (pack[i]) {
                for (int j = 0; j < n; j++)
                    out.add(new Plan(i, 1, false, true, c + PACK_ROW * (j / 2), side0 + (j % 2 == 0 ? PACK_SIDE : -PACK_SIDE)));
                c += PACK_ROW * ((n + 1) / 2);
            } else {
                for (int r = 0; r * 2 < n; r++) {
                    out.add(new Plan(i, Math.min(2, n - 2 * r), false, false, c, side0));
                    c += ConflictBridge.ROW;
                }
            }
            if (chief[i]) {
                if (met) {
                    rear.add(i);
                } else {
                    out.add(new Plan(i, 1, true, false, c, side0));
                    c += ConflictBridge.ROW;
                }
            }
            if (met)
                cursor = c;
        }
        for (int i : rear) {
            out.add(new Plan(i, 1, true, false, cursor, 0.0));
            cursor += ConflictBridge.ROW;
        }
        return out;
    }

    /** The column's heading for a bearing in degrees, from +x toward +z (BAND_MATERIALIZE's, MZB-457): a unit vector. */
    static double[] heading(double bearingDegrees) {
        double a = Math.toRadians(bearingDegrees);
        return new double[] {Math.cos(a), Math.sin(a)};
    }

    /** A stack's offset from the head, {@code -h * back + r * side} with {@code r = (-hz, hx)}: {x, z}. */
    static double[] offset(double hx, double hz, double back, double side) {
        return new double[] {-hx * back - hz * side, -hz * back + hx * side};
    }
}
