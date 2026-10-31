package com.minecraftindustrialcomplex.mic_rtsim.sites;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;

/**
 * The site ledger's second pressure (S9, MZB-1231), the part the core's pure unit tests reach: SITE_STATE's layout moved the
 * UUIDs eight bytes, a row's fifth long carries {@code p2} in thousandths beside n, l and m (a row from before reads 0), and a
 * row's long array ({@link SiteRowPacking#encode}, {@link SiteRowPacking#decode}, which {@code SiteLedger.save} and its loader
 * use) round-trips {@code p2} with the rest. What needs a server stays the GameTests': a SITE_STATE applied, the found tail, and
 * the SavedData's own tag ({@code aResetKeepsAGrovesTwoPressures}); the layouts' home is the Rust codec (`rtsim_kernel::sites`).
 */
class SiteLedgerTest {
    @Test
    void theSiteStateLayoutMovedTheUuidsEightBytes() {
        assertEquals(56, Cmd.SiteState.P2);
        assertEquals(64, Cmd.SiteState.UUIDS);
        assertEquals(64 + 16 * 3, Cmd.SiteState.size(3));
    }

    @Test
    void theCountsLongCarriesTheSecondPressureInThousandths() {
        long w = SiteRowPacking.counts(4, 2, 3, 1.2346f);
        assertEquals(4, SiteRowPacking.n(w));
        assertEquals(2, SiteRowPacking.l(w));
        assertEquals(3, SiteRowPacking.m(w));
        assertEquals(1235L, w >>> 48, "thousandths, in the spare bits");
        assertEquals(1.235f, SiteRowPacking.p2(w), 1e-6f);
    }

    @Test
    void aRowWrittenBeforeReadsZeroAndTheValueSaturatesAndIsSane() {
        long old = SiteRowPacking.counts(4, 2, 3, 0f);
        assertEquals(0f, SiteRowPacking.p2(old));
        assertEquals(4, SiteRowPacking.n(old));
        assertEquals(0xFFFF, SiteRowPacking.milli(1_000_000f));
        assertEquals(0, SiteRowPacking.milli(Float.NaN));
        assertEquals(0, SiteRowPacking.milli(-1f));
        assertEquals(0, SiteRowPacking.milli(Float.POSITIVE_INFINITY));
        assertEquals(0f, SiteRowPacking.sane(Float.NaN));
        assertEquals(2.5f, SiteRowPacking.sane(2.5f));
    }

    private static SiteLedger.Row row(int kind, float p, float p2, int n, int l, int m) {
        long[] uuids = new long[2 * (n + l + m)];
        for (int i = 0; i < uuids.length; i++)
            uuids[i] = 0x1000L + i;
        return new SiteLedger.Row(kind, 0xC0B1_0001L, SiteLedger.ROW_WAITING, 6, 4, 2, p, p2, 3, 123_456L, 9_999L, n, l, m, uuids);
    }

    @Test
    void aRowKeepsItsSecondPressureThroughItsLongArray() {
        SiteLedger.Row in = row(SiteLedger.CAMP, 1.5f, 1.25f, 2, 1, 1);
        SiteLedger.Row out = SiteRowPacking.decode(SiteRowPacking.encode(in));
        assertNotNull(out);
        assertEquals(1.25f, out.p2(), 1e-6f, "the chief's pressure survives the save");
        assertEquals(1.5f, out.p());
        assertEquals(in.kind(), out.kind());
        assertEquals(in.key(), out.key());
        assertEquals(in.flags(), out.flags());
        assertEquals(in.k(), out.k());
        assertEquals(in.count(), out.count());
        assertEquals(in.away(), out.away());
        assertEquals(in.gapSeen(), out.gapSeen());
        assertEquals(in.stamp(), out.stamp());
        assertEquals(in.waitLeft(), out.waitLeft());
        assertEquals((in.n() + in.l() + in.m()), out.bodies());
        assertArrayEquals(in.uuids(), out.uuids());
        // A pressure of no worth saves as 0 and a huge one saturates at the u16 of thousandths.
        assertEquals(0f, SiteRowPacking.decode(SiteRowPacking.encode(row(SiteLedger.CAMP, 0f, 0f, 0, 0, 0))).p2());
        assertEquals(65.535f, SiteRowPacking.decode(SiteRowPacking.encode(row(SiteLedger.CAMP, 0f, 1e9f, 0, 0, 0))).p2(), 1e-3f);
    }

    @Test
    void aRowThatDoesNotParseIsDropped() {
        long[] a = SiteRowPacking.encode(row(SiteLedger.CAMP, 1f, 1f, 1, 0, 0));
        assertNull(SiteRowPacking.decode(java.util.Arrays.copyOf(a, a.length - 1)), "a wrong length");
        assertNull(SiteRowPacking.decode(java.util.Arrays.copyOf(a, 5)), "too short");
        assertNull(SiteRowPacking.decode(SiteRowPacking.encode(row(9, 1f, 1f, 0, 0, 0))), "an unknown kind");
    }
}
