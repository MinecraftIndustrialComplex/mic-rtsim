package com.minecraftindustrialcomplex.mic_rtsim.sites;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;

/**
 * The site ledger's second pressure (S9, MZB-1231), the part the core's pure unit tests reach: SITE_STATE's layout moved the
 * UUIDs eight bytes, and a row's fifth long carries {@code p2} in thousandths beside n, l and m (a row from before reads 0). The
 * rest (a SITE_STATE applied, the found tail, the NBT round trip) is the GameTests' ({@code aResetKeepsAGrovesTwoPressures}) and
 * the Rust codec's (`rtsim_kernel::sites`).
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
}
