package com.minecraftindustrialcomplex.mic_rtsim.sites;

/**
 * The pure part of a ledger row's packing (EQUILIBRIUM.md §2.3, slice S9, MZB-1231), apart from {@link SiteLedger} (a SavedData) so
 * that the core's unit tests, which have no Minecraft on their classpath, can reach it. The row's fifth long holds the three list
 * lengths n, l and m (16 bits each, bits 0 to 47) and the second pressure {@code p2} in thousandths, a saturating u16 (bits 48 to
 * 63); a row written before the second pressure has 0 there and reads 0.
 */
final class SiteRowPacking {
    private SiteRowPacking() {}

    /** A pressure in thousandths, a saturating u16 (the row's 16 spare bits). */
    static int milli(float p) {
        return (int) Math.min(0xFFFF, Math.round(sane(p) * 1000f));
    }

    /** A pressure that is not a finite positive number reads as 0 ({@code rtsim_kernel::sites}). */
    static float sane(float p) {
        return Float.isFinite(p) && p > 0 ? p : 0;
    }

    /** The row's fifth long: n, l and m, and {@code p2} in thousandths. */
    static long counts(int n, int l, int m, float p2) {
        return (n & 0xFFFFL) | (long) (l & 0xFFFF) << 16 | (long) (m & 0xFFFF) << 32 | (long) milli(p2) << 48;
    }

    static int n(long word) {
        return (int) (word & 0xFFFF);
    }

    static int l(long word) {
        return (int) (word >>> 16 & 0xFFFF);
    }

    static int m(long word) {
        return (int) (word >>> 32 & 0xFFFF);
    }

    /** The second pressure the long carries, in thousandths made a float. */
    static float p2(long word) {
        return (word >>> 48 & 0xFFFF) / 1000f;
    }
}
