package com.minecraftindustrialcomplex.mic_rtsim.far;

/**
 * What a far mover is, so a renderer can pick its shape (research/distant-horizons-2026-10-21.md §4). The
 * wire numbers are the far feed's (docs/boundary.md §12.2); a dragon's is FAR_LEG's kind byte too.
 */
public enum FarKind {
    /** An Ice and Fire dragon: a body, wings and a tail along the segment, from its tail to its head. */
    DRAGON(1),
    /** A train carriage: a straight run from its trailing to its leading bogey point. */
    CARRIAGE(2);

    /** The far feed's number for it. */
    public final int wire;

    FarKind(int wire) {
        this.wire = wire;
    }

    /** The kind the far feed numbers {@code wire}, or null. */
    public static FarKind ofWire(int wire) {
        for (FarKind k : values())
            if (k.wire == wire)
                return k;
        return null;
    }

    /** This kind's bit in a subscription's kinds mask. */
    public int bit() {
        return 1 << wire;
    }
}
