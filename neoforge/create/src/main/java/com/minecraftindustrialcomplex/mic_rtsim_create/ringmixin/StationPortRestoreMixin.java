package com.minecraftindustrialcomplex.mic_rtsim_create.ringmixin;

import com.minecraftindustrialcomplex.mic_rtsim_create.PortRing;
import com.simibubi.create.content.logistics.packagePort.PackagePortBlockEntity;
import com.simibubi.create.content.trains.station.GlobalStation;
import com.simibubi.create.content.trains.station.StationBlockEntity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Create 6.0.10's {@code StationBlockEntity.attachPackagePort}, just before it calls
 * {@code GlobalPackagePort.restoreOfflineBuffer} (a postbox registering with its station, every lazy tick): the
 * postbox's chunk is made to carry the ops whose packages the restore brings in ({@link PortRing#beforeRestore},
 * docs/choices-journal-barrier.md RP-206). The chunk can reach disk long before create_tracks.dat and the journal
 * do, and a resend of the op must then find it there.
 *
 * <p>In the ring config (not required, require 0): a Create that moved the call loads without this, and the
 * records name what is then lost (docs/replay-gate.md 9.6, K2).
 */
@Mixin(value = StationBlockEntity.class, remap = false)
public abstract class StationPortRestoreMixin {
    @Inject(method = "attachPackagePort(Lcom/simibubi/create/content/logistics/packagePort/PackagePortBlockEntity;)V",
            at = @At(value = "INVOKE", target = "Lcom/simibubi/create/content/trains/station/GlobalPackagePort;restoreOfflineBuffer(Lnet/neoforged/neoforge/items/IItemHandlerModifiable;)V"),
            require = 0)
    private void mic_rtsim$beforeRestore(PackagePortBlockEntity ppbe, CallbackInfo ci) {
        GlobalStation station = ((StationBlockEntity) (Object) this).getStation();
        if (station != null)
            PortRing.beforeRestore(ppbe, station.connectedPorts.get(ppbe.getBlockPos()));
    }
}
