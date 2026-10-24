package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.simibubi.create.content.kinetics.KineticNetwork;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

/**
 * Create's own update of one member from its network ({@code KineticNetwork.updateFromNetwork}, private):
 * the network update a member missed while its chunk was not loaded, applied when it is FULL again
 * (PATCHES.md row 54).
 */
@Mixin(value = KineticNetwork.class, remap = false)
public interface KineticNetworkInvoker {
    @Invoker("updateFromNetwork")
    void mic_rtsim$updateFromNetwork(KineticBlockEntity be);
}
