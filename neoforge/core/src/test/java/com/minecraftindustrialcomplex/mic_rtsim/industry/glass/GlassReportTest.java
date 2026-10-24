package com.minecraftindustrialcomplex.mic_rtsim.industry.glass;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/** The flat report's layout, as {@code rtsim_industry::glass::report}'s own test pins it. */
class GlassReportTest {

    @Test
    void decodesTheDocumentedLayout() {
        double[] v = new double[GlassReport.HEADER + GlassReport.GOOD_STRIDE + 1 + GlassReport.UNIT_STRIDE];
        v[0] = 3; // compile
        v[1] = 2; // rounds
        v[2] = 1; // converged
        v[9] = 1; // windows completed
        v[10] = GlassReport.PASS;
        v[12] = 1; // goods
        v[13] = 4; // solves
        v[14] = 1; // demoted
        v[15] = 40; // latency
        double[] checksum = {100, 1300, 9, 7, 2, 2000, 4, -1, 900, 2, 1, 2, 3, 4, 5, 6, 7};
        System.arraycopy(checksum, 0, v, 16, checksum.length);
        double[] replay = {GlassReport.REPLAY_GLASS, 5000, 3, 9, 1, 2, 1, 4, 8};
        System.arraycopy(replay, 0, v, 33, replay.length);
        int b = GlassReport.HEADER;
        v[b] = 7;
        v[b + 1] = GlassReport.BOUNDED;
        v[b + 2] = 0.0625;
        v[b + 3] = 0.1175;
        v[b + 6] = 0.0625;
        v[b + 7] = 0.07;
        v[b + 10] = GlassReport.PASS;
        v[b + 11] = 120;
        v[b + 12] = 0.0625;
        v[b + 13] = 0.1175;
        v[b + 14] = 2; // why: rule
        v[b + 15] = 5;
        int t = GlassReport.HEADER + GlassReport.GOOD_STRIDE;
        v[t] = 1; // units
        double[] unit = {2, 9, 1, -3, 60, 4, 5, 64, 9, 300, 1, GlassReport.PASS, 0, 21, 700};
        System.arraycopy(unit, 0, v, t + 1, unit.length);
        GlassReport r = GlassReport.decode(v);
        assertEquals(3, r.compile());
        assertTrue(r.converged());
        assertEquals(4, r.solves());
        assertEquals(1, r.demoted());
        assertEquals(40, r.latency());
        GlassReport.Checksum c = r.checksum();
        assertEquals(100, c.acceptedAt());
        assertEquals(1300, c.readyAt());
        assertEquals(9, c.sweeps());
        assertEquals(7, c.sweepsSampled());
        assertEquals(2, c.heatersBurning());
        assertEquals(2000, c.nextBreakpoint());
        assertEquals(4, c.revision());
        assertEquals(-1, c.districtRevision());
        assertEquals(900, c.lastDiscardTick());
        assertEquals(2, c.lastDiscardReason());
        for (int reason = 1; reason <= 8; reason++)
            assertEquals(reason, c.discards(reason));
        assertTrue(c.summary().contains("station cargo 8"), c.summary());
        assertTrue(c.summary().contains("last discard dropped records at 900"), c.summary());
        GlassReport.Replay rp = r.replay();
        assertEquals(GlassReport.REPLAY_GLASS, rp.model());
        assertEquals(5000, rp.since());
        assertEquals(3, rp.regimes());
        assertEquals(9, rp.breakpoints());
        assertEquals(1, rp.oscillations());
        assertEquals(2, rp.cutEdges());
        assertTrue(rp.eligible());
        assertEquals(4, rp.learnedSources());
        assertTrue(rp.summary().startsWith("replay glass since 5000, eligible"), rp.summary());
        assertEquals(1, r.goods().size());
        GlassReport.Good g = r.good(7);
        assertEquals(GlassReport.BOUNDED, g.cls());
        assertEquals(0.0625, g.lo());
        assertEquals(0.1175, g.hi());
        assertEquals(120, g.samples());
        assertEquals("rule at n5", g.reason());
        assertEquals(1, r.units().size());
        GlassReport.Unit u = r.units().get(0);
        assertEquals(2, u.sections());
        assertEquals(9, u.nodes());
        assertTrue(u.holds(-3, 60, 4) && u.holds(5, 64, 9) && !u.holds(6, 64, 9));
        assertEquals(300, u.windowTicks());
        assertEquals(GlassReport.PASS, u.verdict());
        assertTrue(u.passed() && !u.stale() && u.mayReplay() && !u.frozen() && u.unbounded());
        assertEquals(700, u.readyAt());
        assertEquals(1, r.unitsAt(0, 62, 5).size());
        assertNull(GlassReport.decode(null));
        assertThrows(IllegalArgumentException.class, () -> GlassReport.decode(new double[GlassReport.HEADER + 3]));
    }
}
