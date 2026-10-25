package com.minecraftindustrialcomplex.mic_rtsim.industry.glass;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.Arrays;

import org.junit.jupiter.api.Test;

/** The flat report's layout, as {@code rtsim_industry::glass::report}'s own test pins it. */
class GlassReportTest {

    @Test
    void decodesTheDocumentedLayout() {
        int sectionsAt = GlassReport.HEADER + GlassReport.GOOD_STRIDE + 1 + GlassReport.UNIT_STRIDE;
        int listAt = sectionsAt + 1 + 2 * GlassReport.SECTION_STRIDE;
        int sharedAt = listAt + 1 + 3 * GlassReport.SECTION_STORE_STRIDE;
        int linksAt = sharedAt + 1 + GlassReport.SHARED_STRIDE;
        int residuesAt = linksAt + 1 + 2 * GlassReport.SHARED_LINK_STRIDE;
        double[] v = new double[residuesAt + 1 + GlassReport.SHARED_RESIDUE_STRIDE];
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
        // Two sections: the first in unit 0 and replaying, the second judged by the district and failed.
        v[sectionsAt] = 2;
        double[] first = {0, 0, 9, 2, 1, 1, 1 + 4 + 16 + 32 + 64, 5, -3, 60, 4, 5, 64, 9, -2, 61, 5, 0xDEADBEEFL, 0x01234567L,
                GlassReport.PASS, 0, 1, 300, -1, 0, 0, 0};
        double[] second = {-1, 0, 2, 1, 1, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7, GlassReport.FAIL, 2, 0, 0, 7, 0.05, 0.1, 0.05};
        System.arraycopy(first, 0, v, sectionsAt + 1, first.length);
        System.arraycopy(second, 0, v, sectionsAt + 1 + GlassReport.SECTION_STRIDE, second.length);
        double[] stores = {3, 0, 4, 1, 0, -1, 0, 1, 4, 1};
        System.arraycopy(stores, 0, v, listAt, stores.length);
        double[] shared = {1, 12, 4, 1, 1, 62, 3, 2, 1, 2, 1, 41, 1};
        System.arraycopy(shared, 0, v, sharedAt, shared.length);
        double[] links = {2, 0, 0, 0, 1};
        System.arraycopy(links, 0, v, linksAt, links.length);
        double[] residues = {1, 0, 7, -68};
        System.arraycopy(residues, 0, v, residuesAt, residues.length);
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
        assertEquals(2, r.sections().size());
        GlassReport.Section a = r.sections().get(0), sb = r.sections().get(1);
        assertEquals(0, a.unit());
        assertEquals(-1, sb.unit());
        assertEquals(9, a.nodes());
        assertEquals(2, a.storeCount());
        assertEquals(1, a.sharedStores());
        assertEquals(GlassReport.REPLAYING, a.state());
        assertEquals("replaying", a.stateName());
        assertTrue(a.passed() && a.mayReplay() && a.working() && !a.stale() && !a.frozen());
        assertTrue(a.holds(-3, 60, 4) && a.holds(5, 64, 9) && !a.holds(6, 64, 9));
        assertTrue(a.hasLow());
        assertEquals(-2, a.lowX());
        assertEquals("deadbeef01234567", a.fingerprintHex());
        assertEquals(300, a.windowTicks());
        assertEquals(-1, a.worstGood());
        assertEquals(java.util.List.of(new GlassReport.StoreRef(4, true), new GlassReport.StoreRef(-1, false)), a.stores());
        assertEquals("failed", sb.stateName());
        assertEquals(GlassReport.FAIL, sb.verdict());
        assertEquals(2, sb.strikes());
        assertEquals(7, sb.worstGood());
        assertEquals(0.05, sb.worstMeasured());
        assertEquals(0.05, sb.worstExcess());
        assertEquals(java.util.List.of(new GlassReport.StoreRef(4, true)), sb.stores());
        assertEquals(1, r.sectionsAt(0, 62, 5).size());
        assertEquals(1, r.sectionsMayReplay());
        assertEquals(1, r.sectionsReplaying());
        assertEquals(1, r.shared().size());
        GlassReport.Shared h = r.shared().get(0);
        assertEquals(12, h.node());
        assertEquals(4, h.inventory());
        assertTrue(h.hasPos() && h.x() == 1 && h.y() == 62 && h.z() == 3);
        assertEquals(java.util.List.of(0, 1), h.sections());
        assertTrue(h.counted() && h.countedNow());
        assertEquals(2, h.pairs());
        assertEquals(41, h.sampleSweep());
        assertEquals(-68, h.residueOf(7));
        assertEquals(0, h.residueOf(8));
        // A district with no goods, units or sections: the header and six empty counts.
        GlassReport empty = GlassReport.decode(new double[GlassReport.HEADER + 6]);
        assertTrue(empty.sections().isEmpty() && empty.shared().isEmpty() && empty.units().isEmpty());
        // A truncated sections part is refused, not read past.
        assertThrows(IllegalArgumentException.class, () -> GlassReport.decode(Arrays.copyOf(v, v.length - 1)));
        assertThrows(IllegalArgumentException.class, () -> GlassReport.decode(Arrays.copyOf(v, sectionsAt + 1)));
        // A section naming a unit the report lacks is refused.
        double[] bad = v.clone();
        bad[sectionsAt + 1] = 1;
        assertThrows(IllegalArgumentException.class, () -> GlassReport.decode(bad));
        assertNull(GlassReport.decode(null));
        assertThrows(IllegalArgumentException.class, () -> GlassReport.decode(new double[GlassReport.HEADER + 3]));
    }
}
