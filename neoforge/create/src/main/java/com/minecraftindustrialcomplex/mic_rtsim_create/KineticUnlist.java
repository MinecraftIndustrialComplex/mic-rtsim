package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.simibubi.create.content.kinetics.base.KineticBlockEntity;

/**
 * A kinetic network that takes out a block entity which joined it just before Create forgets its network
 * ({@code clearKineticInformation} outside a {@code read}, PATCHES.md row 58): implemented on Create's {@code KineticNetwork} by
 * {@code KineticNetworkMixin}. Its share stays out of the unloaded share; Create lists it again wherever it
 * next attaches.
 */
public interface KineticUnlist {
    /** Takes {@code be} out of the network's sources and members, outside a sync; nothing during one. */
    void mic_rtsim$unlist(KineticBlockEntity be);
}
