package com.minecraftindustrialcomplex.mic_rtsim_ribbits.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import com.minecraftindustrialcomplex.mic_rtsim_ribbits.RibbitBands;
import com.yungnickyoung.minecraft.ribbits.entity.RibbitEntity;
import com.yungnickyoung.minecraft.ribbits.entity.goal.RibbitPlayMusicGoal;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * Touring bands play around their own master (C7, P16; see {@code RibbitBands}). Verified in the
 * Ribbits 4.1.6 bytecode: {@code RibbitPlayMusicGoal} sets the ribbit's master in {@code canUse} (to null
 * when the band is full), in {@code start} (itself, when it has none), in {@code tick} (null, or
 * itself), and in the lambdas {@code lambda$canUse$0} and {@code lambda$tick$1} (the master of a nearby
 * playing ribbit). For a touring band's member every one of those calls names the band's leader
 * instead; for any other ribbit they are untouched. A band still travelling to its waypoint does not
 * start playing.
 */
@Mixin(RibbitPlayMusicGoal.class)
public abstract class RibbitPlayMusicGoalMixin {
    @Shadow
    @Final
    private RibbitEntity ribbit;

    @Inject(method = "canUse", at = @At("HEAD"), cancellable = true)
    private void mic_rtsim_ribbits$notWhileTravelling(CallbackInfoReturnable<Boolean> cir) {
        PuppetGroup band = RibbitBands.band(this.ribbit);
        if (band != null && !RibbitBands.atStop(band))
            cir.setReturnValue(false);
    }

    @WrapOperation(method = {"canUse", "start", "tick", "lambda$canUse$0", "lambda$tick$1"}, at = @At(value = "INVOKE",
            target = "Lcom/yungnickyoung/minecraft/ribbits/entity/RibbitEntity;setMasterRibbit(Lcom/yungnickyoung/minecraft/ribbits/entity/RibbitEntity;)V"))
    private void mic_rtsim_ribbits$theBandsOwnMaster(RibbitEntity self, RibbitEntity master, Operation<Void> original) {
        RibbitEntity leader = self == this.ribbit ? RibbitBands.master(self) : null;
        original.call(self, leader != null ? leader : master);
    }
}
