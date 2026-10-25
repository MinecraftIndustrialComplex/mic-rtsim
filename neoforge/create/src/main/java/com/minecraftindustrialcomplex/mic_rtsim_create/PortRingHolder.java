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
}
