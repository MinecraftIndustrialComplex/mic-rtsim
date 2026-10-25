package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import com.minecraftindustrialcomplex.mic_rtsim.conflict.MarchColumn.Plan;
import org.junit.jupiter.api.Test;

import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** The march's column: how entries split into stacks and where each stands (MZB-469). */
class MarchColumnTest {
    private static final double EPS = 1e-9;

    private static List<Plan> met(int[] counts, boolean[] pack, boolean[] chief) {
        return MarchColumn.plan(counts, pack, chief, true);
    }

    private static boolean[] flags(int m, int... on) {
        boolean[] f = new boolean[m];
        for (int i : on)
            f[i] = true;
        return f;
    }

    private static void assertPlan(Plan p, int entry, int slots, boolean chief, boolean pack, double back, double side) {
        assertEquals(entry, p.entry(), "entry");
        assertEquals(slots, p.slots(), "slots");
        assertEquals(chief, p.chief(), "chief");
        assertEquals(pack, p.pack(), "pack");
        assertEquals(back, p.back(), EPS, "back");
        assertEquals(side, p.side(), EPS, "side");
    }

    @Test
    void anEntryIsRowsOfTwoOneRowBehindTheOther() {
        List<Plan> plan = met(new int[] {5}, flags(1), flags(1));
        assertEquals(3, plan.size());
        assertPlan(plan.get(0), 0, 2, false, false, 0.0, 0.0);
        assertPlan(plan.get(1), 0, 2, false, false, 1.5, 0.0);
        assertPlan(plan.get(2), 0, 1, false, false, 3.0, 0.0);
    }

    @Test
    void metOnTheMarchTheEntriesLineUpInRecordOrderAndTheChiefRidesAtTheVeryRear() {
        // Entry 0: 4 grunts and the chief; entry 1: 3 raptors (packs); entry 2: 2 more grunts.
        List<Plan> plan = met(new int[] {4, 3, 2}, flags(3, 1), flags(3, 0));
        // 2 rows (0, 1.5), then the packs from 3.0: a pair abreast at 3.0, the third alone 6.0 further, then 2 grunts' row, the chief last.
        assertEquals(2 + 3 + 1 + 1, plan.size());
        assertPlan(plan.get(0), 0, 2, false, false, 0.0, 0.0);
        assertPlan(plan.get(1), 0, 2, false, false, 1.5, 0.0);
        assertPlan(plan.get(2), 1, 1, false, true, 3.0, MarchColumn.PACK_SIDE);
        assertPlan(plan.get(3), 1, 1, false, true, 3.0, -MarchColumn.PACK_SIDE);
        assertPlan(plan.get(4), 1, 1, false, true, 9.0, MarchColumn.PACK_SIDE);
        // Two pairs' worth of room (the third raptor starts a pair), then the last entry's row.
        assertPlan(plan.get(5), 2, 2, false, false, 15.0, 0.0);
        // The chief of entry 0 is behind everything, whatever its entry.
        assertPlan(plan.get(6), 0, 1, true, false, 16.5, 0.0);
    }

    @Test
    void onAnArcEachEntryKeepsItsOwnColumnAndItsSpread() {
        List<Plan> plan = MarchColumn.plan(new int[] {4, 2}, flags(2), flags(2, 0), false);
        // STACK_SPACING 4 across two entries: sides -2 and +2.
        assertEquals(2 + 1 + 1, plan.size());
        assertPlan(plan.get(0), 0, 2, false, false, 0.0, -2.0);
        assertPlan(plan.get(1), 0, 2, false, false, 1.5, -2.0);
        // The chief of entry 0 is behind its entry's rows, not the whole band's.
        assertPlan(plan.get(2), 0, 1, true, false, 3.0, -2.0);
        assertPlan(plan.get(3), 1, 2, false, false, 0.0, 2.0);
    }

    @Test
    void anArcPackAlternatesAboutItsEntrysSide() {
        List<Plan> plan = MarchColumn.plan(new int[] {3, 4}, flags(2, 1), flags(2), false);
        // Entry 0: rows at side -2; entry 1 (packs): sides 2 +/- 3, pairs 6 apart.
        assertPlan(plan.get(2), 1, 1, false, true, 0.0, 2.0 + MarchColumn.PACK_SIDE);
        assertPlan(plan.get(3), 1, 1, false, true, 0.0, 2.0 - MarchColumn.PACK_SIDE);
        assertPlan(plan.get(4), 1, 1, false, true, 6.0, 2.0 + MarchColumn.PACK_SIDE);
        assertPlan(plan.get(5), 1, 1, false, true, 6.0, 2.0 - MarchColumn.PACK_SIDE);
    }

    @Test
    void anEntryWithNoUnitsMakesNoStackUnlessItHoldsTheChief() {
        assertTrue(met(new int[] {0, 0}, flags(2), flags(2)).isEmpty());
        List<Plan> plan = met(new int[] {0, 2}, flags(2), flags(2, 0));
        assertEquals(2, plan.size());
        assertPlan(plan.get(0), 1, 2, false, false, 0.0, 0.0);
        // The chief of an empty entry still rides at the rear, alone.
        assertPlan(plan.get(1), 0, 1, true, false, 1.5, 0.0);
        List<Plan> alone = met(new int[] {0}, flags(1), flags(1, 0));
        assertEquals(1, alone.size());
        assertPlan(alone.get(0), 0, 1, true, false, 0.0, 0.0);
    }

    @Test
    void everyUnitIsInOneStackAndOnlyTheChiefIsExtra() {
        int[] counts = {7, 5, 1};
        List<Plan> plan = met(counts, flags(3, 1), flags(3, 2));
        int[] seen = new int[3];
        int chiefs = 0;
        for (Plan p : plan) {
            assertTrue(p.slots() >= 1 && p.slots() <= 2);
            if (p.chief()) {
                chiefs++;
                assertEquals(1, p.slots());
                assertFalse(p.pack());
            } else {
                seen[p.entry()] += p.slots();
            }
        }
        assertEquals(1, chiefs);
        assertEquals(7, seen[0]);
        assertEquals(5, seen[1]);
        assertEquals(1, seen[2]);
    }

    @Test
    void theHeadingIsTheBearingFromPlusXTowardPlusZ() {
        double[] east = MarchColumn.heading(0);
        assertEquals(1.0, east[0], EPS);
        assertEquals(0.0, east[1], EPS);
        double[] south = MarchColumn.heading(90);
        assertEquals(0.0, south[0], EPS);
        assertEquals(1.0, south[1], EPS);
        double[] north = MarchColumn.heading(-90);
        assertEquals(0.0, north[0], EPS);
        assertEquals(-1.0, north[1], EPS);
    }

    @Test
    void aSteerTurnsTheColumnTowardThePointFromTheHead() {
        // Heading east, a point due south of the head: the column turns to face south.
        double[] h = MarchColumn.turn(10, 10, 10, 50, 1, 0);
        assertEquals(0.0, h[0], EPS);
        assertEquals(1.0, h[1], EPS);
        // A point behind turns it right round; a point on the head keeps the heading it had.
        double[] back = MarchColumn.turn(0, 0, -5, 0, 1, 0);
        assertEquals(-1.0, back[0], EPS);
        assertEquals(0.0, back[1], EPS);
        double[] same = MarchColumn.turn(3, 4, 3, 4, 0.6, 0.8);
        assertEquals(0.6, same[0], EPS);
        assertEquals(0.8, same[1], EPS);
        double[] near = MarchColumn.turn(3, 4, 3 + 1e-9, 4, 0.6, 0.8);
        assertEquals(0.6, near[0], EPS);
    }

    @Test
    void aTurnedColumnKeepsEachStacksDistanceFromTheHead() {
        // Two stacks of one column, 3 back and 2 to the right, and 6 back on the line: a turn moves where they stand
        // in the plane, never how far each is from the head.
        for (double deg : new double[] {0, 37, 90, 135, 180, -60}) {
            double[] h = MarchColumn.heading(deg);
            double[] a = MarchColumn.offset(h[0], h[1], 3, 2);
            double[] b = MarchColumn.offset(h[0], h[1], 6, 0);
            assertEquals(Math.hypot(3, 2), Math.hypot(a[0], a[1]), 1e-9);
            // The line stack is exactly 6 behind: against the heading.
            assertEquals(-6.0, b[0] * h[0] + b[1] * h[1], 1e-9);
            // The side stack is 2 to the right: along r = (-hz, hx).
            assertEquals(2.0, a[0] * -h[1] + a[1] * h[0], 1e-9);
        }
    }

    @Test
    void anOffsetIsBehindTheHeadAndToItsRight() {
        // Facing east (+x), right is +z (south): 3 back, 2 right is 3 west and 2 south.
        double[] o = MarchColumn.offset(1, 0, 3, 2);
        assertEquals(-3.0, o[0], EPS);
        assertEquals(2.0, o[1], EPS);
        // Facing south (+z), right is -x (west): 3 back is 3 north, 2 right is 2 west.
        double[] s = MarchColumn.offset(0, 1, 3, 2);
        assertEquals(-2.0, s[0], EPS);
        assertEquals(-3.0, s[1], EPS);
        // The offset's length is the hypotenuse of back and side, whatever the heading.
        double a = Math.toRadians(37);
        double[] t = MarchColumn.offset(Math.cos(a), Math.sin(a), 6, 3);
        assertEquals(Math.hypot(6, 3), Math.hypot(t[0], t[1]), 1e-9);
    }
}
