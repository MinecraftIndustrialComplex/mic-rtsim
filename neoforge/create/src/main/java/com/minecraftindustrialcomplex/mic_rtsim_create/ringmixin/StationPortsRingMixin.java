package com.minecraftindustrialcomplex.mic_rtsim_create.ringmixin;

import com.minecraftindustrialcomplex.mic_rtsim_create.PortRing;
import com.simibubi.create.content.trains.graph.DimensionPalette;
import com.simibubi.create.content.trains.station.GlobalStation;
import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.CompoundTag;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Saves and reads the station port's rings (docs/choices-journal-barrier.md RP-206) in Create 6.0.10's
 * {@code GlobalStation.write(CompoundTag, HolderLookup.Provider, DimensionPalette)} and
 * {@code read(CompoundTag, HolderLookup.Provider, boolean, DimensionPalette)}: after Create has written its
 * "Ports" list (Address, OfflineBuffer, Primed, Pos per port), each port's rings go into that port's entry, and
 * after Create has made new ports from the list, the rings go back onto them, matched by Pos. EdgePointStorage
 * (create_tracks.dat) and a station's migration data are the callers, so the rings travel with the buffer.
 *
 * <p>In the train ring's config (mic_rtsim_create_ring.mixins.json, not required, require 0): a Create that
 * moved either method loads without the port's rings, and {@link PortRing#available} finds that out.
 */
@Mixin(value = GlobalStation.class, remap = false)
public abstract class StationPortsRingMixin {
    @Inject(method = "write(Lnet/minecraft/nbt/CompoundTag;Lnet/minecraft/core/HolderLookup$Provider;Lcom/simibubi/create/content/trains/graph/DimensionPalette;)V",
            at = @At("RETURN"), require = 0)
    private void mic_rtsim$writePortRings(CompoundTag nbt, HolderLookup.Provider registries, DimensionPalette dimensions, CallbackInfo ci) {
        PortRing.write((GlobalStation) (Object) this, nbt);
    }

    @Inject(method = "read(Lnet/minecraft/nbt/CompoundTag;Lnet/minecraft/core/HolderLookup$Provider;ZLcom/simibubi/create/content/trains/graph/DimensionPalette;)V",
            at = @At("RETURN"), require = 0)
    private void mic_rtsim$readPortRings(CompoundTag nbt, HolderLookup.Provider registries, boolean migration, DimensionPalette dimensions, CallbackInfo ci) {
        PortRing.read((GlobalStation) (Object) this, nbt);
    }
}
