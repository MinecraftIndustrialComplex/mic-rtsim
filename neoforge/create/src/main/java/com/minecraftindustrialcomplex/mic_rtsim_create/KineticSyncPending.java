package com.minecraftindustrialcomplex.mic_rtsim_create;

/**
 * A kinetic block entity whose network update was held back while its chunk was not loaded (PATCHES.md row
 * 54, {@code KineticNetworkMixin}): implemented on Create's {@code KineticBlockEntity} by
 * {@code KineticBlockEntityMixin}, applied when the chunk is FULL again ({@link StaleKinetics#chunkFull}).
 * Runtime only, never saved.
 */
public interface KineticSyncPending {
    boolean mic_rtsim$syncPending();

    void mic_rtsim$setSyncPending(boolean pending);
}
