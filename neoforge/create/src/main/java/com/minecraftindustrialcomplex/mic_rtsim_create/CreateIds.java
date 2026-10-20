package com.minecraftindustrialcomplex.mic_rtsim_create;

import java.util.UUID;

/** Stable u64 ids for Create's UUIDs (stations, trains): docs/industry-boundary.md §2.1 allows a hash. */
public final class CreateIds {
    private CreateIds() {}

    /** SplitMix64 over the UUID's two halves; never 0 (0 means "none" on the wire). */
    public static long of(UUID id) {
        if (id == null)
            return 0;
        long x = id.getMostSignificantBits() * 0x9E3779B97F4A7C15L + id.getLeastSignificantBits();
        x = (x ^ (x >>> 30)) * 0xBF58476D1CE4E5B9L;
        x = (x ^ (x >>> 27)) * 0x94D049BB133111EBL;
        x ^= x >>> 31;
        return x == 0 ? 1 : x;
    }
}
