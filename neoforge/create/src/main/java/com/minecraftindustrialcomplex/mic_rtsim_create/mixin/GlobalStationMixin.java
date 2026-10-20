package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.minecraftindustrialcomplex.mic_rtsim_create.MailWatch;
import com.simibubi.create.content.trains.station.GlobalStation;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Mail at unloaded districts (PATCHES.md, Create rows): the offline buffers before and after
 * {@code GlobalStation.runMailTransfer}, so their changes are IND_PACKAGE records ({@link MailWatch}).
 */
@Mixin(value = GlobalStation.class, remap = false)
public abstract class GlobalStationMixin {
    @Inject(method = "runMailTransfer", at = @At("HEAD"))
    private void mic_rtsim$beforeMail(CallbackInfo ci) {
        MailWatch.before((GlobalStation) (Object) this);
    }

    @Inject(method = "runMailTransfer", at = @At("TAIL"))
    private void mic_rtsim$afterMail(CallbackInfo ci) {
        MailWatch.after((GlobalStation) (Object) this);
    }
}
