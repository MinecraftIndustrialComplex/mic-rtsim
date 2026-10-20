package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.minecraftindustrialcomplex.mic_rtsim_create.TrainWatch;
import com.simibubi.create.content.trains.entity.Train;
import com.simibubi.create.content.trains.station.GlobalStation;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * The station hooks of INDUSTRY.md §3.3 and §8.1 (PATCHES.md, Create rows): a train arriving at and
 * leaving a station. Create 6.0.10 has no event for either. {@code arriveAt} is hooked at its start (the
 * cargo the train brought, before Create's own mail transfer moves packages) and at its end (the
 * record); {@code leaveStation} at its start, while the station is still current.
 */
@Mixin(value = Train.class, remap = false)
public abstract class TrainMixin {
    @Inject(method = "arriveAt", at = @At("HEAD"))
    private void mic_rtsim$arriving(GlobalStation station, CallbackInfo ci) {
        TrainWatch.arriving((Train) (Object) this, station);
    }

    @Inject(method = "arriveAt", at = @At("TAIL"))
    private void mic_rtsim$arrived(GlobalStation station, CallbackInfo ci) {
        TrainWatch.arrived((Train) (Object) this, station);
    }

    @Inject(method = "leaveStation", at = @At("HEAD"))
    private void mic_rtsim$leaving(CallbackInfo ci) {
        TrainWatch.departing((Train) (Object) this);
    }
}
