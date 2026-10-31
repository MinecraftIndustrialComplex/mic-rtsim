package com.minecraftindustrialcomplex.mic_rtsim.sites;

/**
 * The pure part of a ledger row's packing (EQUILIBRIUM.md §2.3, slice S9, MZB-1231), apart from {@link SiteLedger} (a SavedData) so
 * that the core's unit tests, which have no Minecraft on their classpath, can reach it. The row's fifth long holds the three list
 * lengths n, l and m (16 bits each, bits 0 to 47) and the second pressure {@code p2} in thousandths, a saturating u16 (bits 48 to
 * 63); a row written before the second pressure has 0 there and reads 0. {@link #encode} and {@link #decode} are the row's whole NBT
 * long array, so the unit tests round-trip a row (its {@code p2} among the fields) the way the ledger saves and loads it.
 */
final class SiteRowPacking {
    private SiteRowPacking() {}

    /** A row's long array: the key, the packed kind, flags, K, count and away, P and gapSeen, the stamp, the wait, the counts, the UUIDs. */
    static long[] encode(SiteLedger.Row r) {
        long[] a = new long[6 + r.uuids().length];
        a[0] = r.key();
        a[1] = (r.kind() & 0xFFL) | (long) (r.flags() & 0xFF) << 8 | (long) (r.k() & 0xFFFF) << 16 | (long) (r.count() & 0xFFFF) << 32
                | (long) (r.away() & 0xFFFF) << 48;
        a[2] = (Float.floatToRawIntBits(r.p()) & 0xFFFF_FFFFL) | (long) r.gapSeen() << 32;
        a[3] = r.stamp();
        a[4] = r.waitLeft();
        a[5] = counts(r.n(), r.l(), r.m(), r.p2());
        System.arraycopy(r.uuids(), 0, a, 6, r.uuids().length);
        return a;
    }

    /** A row from its longs, or null when it does not parse (a wrong length, an unknown kind, a list above K). */
    static SiteLedger.Row decode(long[] a) {
        if (a.length < 6)
            return null;
        int kind = (int) (a[1] & 0xFF);
        int flags = (int) (a[1] >>> 8 & 0xFF);
        int k = (int) (a[1] >>> 16 & 0xFFFF);
        int n = n(a[5]), l = l(a[5]), m = m(a[5]);
        int cap = Math.min(k, SiteLedger.MAX_BODIES / 3);
        if (!(kind == SiteLedger.CAMP || kind == SiteLedger.LAIR || kind == SiteLedger.NEST) || n > cap || l > cap || m > cap
                || a.length != 6 + 2 * (n + l + m))
            return null;
        long[] uuids = new long[a.length - 6];
        System.arraycopy(a, 6, uuids, 0, uuids.length);
        return new SiteLedger.Row(kind, a[0], flags, k, (int) (a[1] >>> 32 & 0xFFFF), (int) (a[1] >>> 48 & 0xFFFF),
                sane(Float.intBitsToFloat((int) a[2])), p2(a[5]), (int) (a[2] >>> 32), a[3], a[4], n, l, m, uuids);
    }

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
