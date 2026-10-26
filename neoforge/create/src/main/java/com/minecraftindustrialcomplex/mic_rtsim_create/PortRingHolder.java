package com.minecraftindustrialcomplex.mic_rtsim_create;

/**
 * What {@code ringmixin.PortRingMixin} adds to Create's {@code GlobalPackagePort} (docs/choices-journal-barrier.md
 * RP-206): the port's two rings of applied IND_WRITE_PACKAGE op ids, one for packages written into its offline
 * buffer and one for packages written into its loaded postbox. Outside the mixin package, so the plugin's own code
 * can name it; whether {@code GlobalPackagePort} implements it is the first half of {@link PortRing#available}.
 */
public interface PortRingHolder {
    /** The ring of ops whose packages went into the offline buffer, made on first use. */
    AppliedOpRing mic_rtsim$bufferOps();

    /** The buffer's ring, or null when the port never had one. */
    AppliedOpRing mic_rtsim$bufferOpsIfAny();

    /** The ring of ops whose packages went into the loaded postbox (mirrored into the buffer), made on first use. */
    AppliedOpRing mic_rtsim$postboxOps();

    /** The postbox's ring, or null when the port never had one. */
    AppliedOpRing mic_rtsim$postboxOpsIfAny();

    /**
     * True once this server life knows the offline buffer to match the postbox's inventory: Create mirrored the
     * postbox into it or restored it into the postbox ({@code ringmixin.PortMirrorMixin}), or {@link PortRing#refreshed}
     * read the postbox's chunk. Not saved: a crash can leave create_tracks.dat older than the postbox's chunk.
     */
    boolean mic_rtsim$mirrorFresh();

    /** Marks the buffer as matching the postbox for the rest of this server life. */
    void mic_rtsim$markMirrorFresh();
}
