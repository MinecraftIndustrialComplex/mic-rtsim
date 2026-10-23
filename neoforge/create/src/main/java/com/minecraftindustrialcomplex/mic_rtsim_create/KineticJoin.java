package com.minecraftindustrialcomplex.mic_rtsim_create;

/**
 * A kinetic block entity that joins its network as its first tick would (PATCHES.md row 58): implemented on
 * Create's {@code KineticBlockEntity} by {@code KineticBlockEntityMixin}, which calls it where Create lists a
 * block entity again or takes it out before its first tick ({@code removeSource}, {@code setSpeed}, a
 * {@code setNetwork} to another network, {@code remove}, {@code clearKineticInformation}, a generator's
 * {@code updateGeneratedRotation}, a powered shaft's {@code update} and {@code remove}). A block entity in a chunk that is FULL and has not ticked has not joined,
 * and Create's {@code add} would list it without counting it out of the network's unloaded share, which
 * counted it twice; removed, it would leave its share there.
 */
public interface KineticJoin {
    /**
     * The network half of Create's {@code KineticBlockEntity.initialize}, once, before it: with a network,
     * its network (created if none is listed for its id, and initialized from what this block entity saved if
     * not yet) lists it through {@code addSilently}, which counts it out of the unloaded share (Create's
     * {@code addSilently} returns early for a member). During a {@code sync} of that network it becomes a
     * member once the sync ends ({@code KineticNetworkMixin}). Nothing once {@code initialize} has run or
     * this has, or on a client.
     */
    void mic_rtsim$join();
}
