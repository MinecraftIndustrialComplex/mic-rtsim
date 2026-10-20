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
        double[] v = new double[GlassReport.HEADER + GlassReport.GOOD_STRIDE];
        v[0] = 3; // compile
        v[1] = 2; // rounds
        v[2] = 1; // converged
        v[9] = 1; // windows completed
        v[10] = GlassReport.PASS;
        v[12] = 1; // goods
        int b = GlassReport.HEADER;
        v[b] = 7;
        v[b + 1] = GlassReport.BOUNDED;
        v[b + 2] = 0.0625;
        v[b + 3] = 0.1175;
        v[b + 6] = 0.0625;
        v[b + 7] = 0.07;
        v[b + 10] = GlassReport.PASS;
        v[b + 11] = 120;
        GlassReport r = GlassReport.decode(v);
        assertEquals(3, r.compile());
        assertTrue(r.converged());
        assertEquals(1, r.goods().size());
        GlassReport.Good g = r.good(7);
        assertEquals(GlassReport.BOUNDED, g.cls());
        assertEquals(0.0625, g.lo());
        assertEquals(0.1175, g.hi());
        assertEquals(120, g.samples());
        assertNull(GlassReport.decode(null));
        assertThrows(IllegalArgumentException.class, () -> GlassReport.decode(new double[GlassReport.HEADER + 3]));
    }
}
