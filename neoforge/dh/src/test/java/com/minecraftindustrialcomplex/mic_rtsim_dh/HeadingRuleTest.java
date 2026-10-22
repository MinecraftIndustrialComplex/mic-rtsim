package com.minecraftindustrialcomplex.mic_rtsim_dh;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/** The heading-change rule and the pacing of re-cuts. */
class HeadingRuleTest {
    private static double[] heading(double degrees, double length) {
        double a = Math.toRadians(degrees);
        return new double[] {length * Math.cos(a), 0, length * Math.sin(a)};
    }

    @Test
    void smallTurnsKeepTheCut() {
        double[] cut = heading(0, 8);
        double[] now = heading(9.5, 8);
        assertFalse(HeadingRule.needsRecut(cut[0], cut[1], cut[2], now[0], now[1], now[2]));
    }

    @Test
    void turnsPastTenDegreesRecut() {
        double[] cut = heading(30, 8);
        double[] now = heading(40.5, 8);
        assertTrue(HeadingRule.needsRecut(cut[0], cut[1], cut[2], now[0], now[1], now[2]));
    }

    @Test
    void slopesCountAsTurns() {
        // Level, then onto a 1-in-4 rise (14 degrees up)
        assertTrue(HeadingRule.needsRecut(8, 0, 0, 8, 2, 0));
        assertFalse(HeadingRule.needsRecut(8, 0, 0, 8, 1, 0), "7 degrees up");
    }

    @Test
    void reversingKeepsTheCut() {
        // The cut is symmetric about the midpoint, so a train backing up needs no new boxes
        assertFalse(HeadingRule.needsRecut(8, 0, 0, -8, 0, 0));
        assertEquals(0.0, HeadingRule.angleDegrees(3, 1, 2, -3, -1, -2), 1e-6);
    }

    @Test
    void lengthChangesRecut() {
        assertFalse(HeadingRule.needsRecut(8, 0, 0, 8.4, 0, 0));
        assertTrue(HeadingRule.needsRecut(8, 0, 0, 8.6, 0, 0));
    }

    @Test
    void badSamplesKeepTheCut() {
        assertFalse(HeadingRule.needsRecut(8, 0, 0, Double.NaN, 0, 0));
        assertEquals(0.0, HeadingRule.angleDegrees(0, 0, 0, 1, 0, 0));
        assertEquals(90.0, HeadingRule.angleDegrees(1, 0, 0, 0, 0, 1), 1e-9);
    }

    @Test
    void pacerAllowsOneCutPerIntervalAndConfirmsOnce() {
        RecutPacer p = new RecutPacer();
        long t = 1_000_000_000L;
        assertTrue(p.mayCut(t), "a new group may cut at once");
        p.cut(t);
        assertFalse(p.mayCut(t + RecutPacer.MIN_INTERVAL_NANOS - 1));
        assertTrue(p.mayCut(t + RecutPacer.MIN_INTERVAL_NANOS));

        assertFalse(p.confirmDue(t + RecutPacer.CONFIRM_DELAY_NANOS - 1));
        // A second cut pushes the confirmation back
        long t2 = t + RecutPacer.MIN_INTERVAL_NANOS;
        p.cut(t2);
        assertFalse(p.confirmDue(t + RecutPacer.CONFIRM_DELAY_NANOS));
        assertTrue(p.confirmDue(t2 + RecutPacer.CONFIRM_DELAY_NANOS));
        assertFalse(p.confirmDue(t2 + 10 * RecutPacer.CONFIRM_DELAY_NANOS), "only once");
    }
}
