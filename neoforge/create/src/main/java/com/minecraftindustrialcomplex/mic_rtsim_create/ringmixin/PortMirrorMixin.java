package com.minecraftindustrialcomplex.mic_rtsim_create.ringmixin;

import com.minecraftindustrialcomplex.mic_rtsim_create.PortRingHolder;
import com.simibubi.create.content.trains.station.GlobalPackagePort;
import net.neoforged.neoforge.items.IItemHandlerModifiable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Marks a station port's offline buffer as matching its postbox for this server life
 * ({@link PortRingHolder#mic_rtsim$markMirrorFresh}, docs/choices-journal-barrier.md RP-206) when Create 6.0.10 makes
 * them match: {@code restoreOfflineBuffer} of a primed buffer (the postbox takes the buffer) and
 * {@code saveOfflineBuffer} outside a restore (the buffer takes the postbox's inventory).
 *
 * <p>In the ring config (not required, require 0): without these injections a port is never marked by Create, and
 * {@code PortRing.refreshed} reads its postbox's chunk once before its first offline write of the life instead.
 */
@Mixin(value = GlobalPackagePort.class, remap = false)
public abstract class PortMirrorMixin {
    @Shadow
    public boolean primed;
    @Shadow
    private boolean restoring;

    @Inject(method = "restoreOfflineBuffer(Lnet/neoforged/neoforge/items/IItemHandlerModifiable;)V", at = @At("HEAD"), require = 0)
    private void mic_rtsim$restored(IItemHandlerModifiable inventory, CallbackInfo ci) {
        if (primed && (Object) this instanceof PortRingHolder h)
            h.mic_rtsim$markMirrorFresh();
    }

    @Inject(method = "saveOfflineBuffer(Lnet/neoforged/neoforge/items/IItemHandlerModifiable;)V", at = @At("HEAD"), require = 0)
    private void mic_rtsim$mirrored(IItemHandlerModifiable inventory, CallbackInfo ci) {
        if (!restoring && (Object) this instanceof PortRingHolder h)
            h.mic_rtsim$markMirrorFresh();
    }
}
