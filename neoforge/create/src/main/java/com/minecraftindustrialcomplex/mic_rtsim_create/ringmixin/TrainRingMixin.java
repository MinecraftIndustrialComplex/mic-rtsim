package com.minecraftindustrialcomplex.mic_rtsim_create.ringmixin;

import com.minecraftindustrialcomplex.mic_rtsim_create.AppliedOpRing;
import com.minecraftindustrialcomplex.mic_rtsim_create.AppliedOpsHolder;
import com.minecraftindustrialcomplex.mic_rtsim_create.TrainRing;
import com.simibubi.create.content.trains.entity.Train;
import com.simibubi.create.content.trains.graph.DimensionPalette;
import com.simibubi.create.content.trains.graph.TrackGraph;
import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.CompoundTag;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

import java.util.Map;
import java.util.UUID;

/**
 * The train op ring (docs/choices-mowzie-build.md RP-190): a field on Create's {@code Train} and the key
 * {@link TrainRing#KEY} in the compound Create 6.0.10's {@code Train.write(DimensionPalette,
 * HolderLookup.Provider)} returns and {@code Train.read(CompoundTag, HolderLookup.Provider, Map,
 * DimensionPalette)} takes. RailwaySavedData (create_tracks.dat) is their only caller, and the carriage goods
 * are written inside the same {@code Train.write} (Carriage.write, its EntityPositioning entries).
 *
 * <p>Its own config, mic_rtsim_create_ring.mixins.json, not required and require 0: a Create that moved
 * either method loads without the ring, and {@link TrainRing#available} finds that out (RP-193).
 */
@Mixin(value = Train.class, remap = false)
public abstract class TrainRingMixin implements AppliedOpsHolder {
    @Unique
    private AppliedOpRing mic_rtsim$ring;

    @Override
    public AppliedOpRing mic_rtsim$appliedOps() {
        if (mic_rtsim$ring == null)
            mic_rtsim$ring = new AppliedOpRing(TrainRing.CAPACITY);
        return mic_rtsim$ring;
    }

    @Override
    public AppliedOpRing mic_rtsim$appliedOpsIfAny() {
        return mic_rtsim$ring;
    }

    @Inject(method = "write(Lcom/simibubi/create/content/trains/graph/DimensionPalette;Lnet/minecraft/core/HolderLookup$Provider;)Lnet/minecraft/nbt/CompoundTag;",
            at = @At("RETURN"), require = 0)
    private void mic_rtsim$writeRing(DimensionPalette dimensions, HolderLookup.Provider registries, CallbackInfoReturnable<CompoundTag> cir) {
        TrainRing.write(this, cir.getReturnValue());
    }

    @Inject(method = "read(Lnet/minecraft/nbt/CompoundTag;Lnet/minecraft/core/HolderLookup$Provider;Ljava/util/Map;Lcom/simibubi/create/content/trains/graph/DimensionPalette;)Lcom/simibubi/create/content/trains/entity/Train;",
            at = @At("RETURN"), require = 0)
    private static void mic_rtsim$readRing(CompoundTag tag, HolderLookup.Provider registries, Map<UUID, TrackGraph> trackNetworks,
            DimensionPalette dimensions, CallbackInfoReturnable<Train> cir) {
        TrainRing.read(cir.getReturnValue(), tag);
    }
}
