package com.minecraftindustrialcomplex.mic_rtsim_create;

/**
 * A kinetic block entity that joins its network as its first tick would (PATCHES.md row 58): implemented on
 * Create's {@code KineticBlockEntity} by {@code KineticBlockEntityMixin}. {@link StaleKinetics#chunkFull}
 * joins every block entity of a chunk before it re-syncs a part there: a block entity in a chunk that is
 * FULL and has not ticked has not joined, and the re-sync's {@code KineticNetwork.add} would list it without
 * counting it out of the network's unloaded share, which counted it twice.
 */
public interface KineticJoin {
    /**
     * The network half of Create's {@code KineticBlockEntity.initialize}: with a network, its network
     * (created if none is listed for its id, and initialized from what this block entity saved if not yet)
     * lists it through {@code addSilently}, which counts it out of the unloaded share. Nothing for a member
     * (Create's {@code addSilently} returns early), without a network, or on a client.
     */
    void mic_rtsim$join();
}
