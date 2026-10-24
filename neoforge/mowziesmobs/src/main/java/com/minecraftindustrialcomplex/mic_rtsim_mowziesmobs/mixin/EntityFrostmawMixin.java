package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.mixin;

import com.bobmowzie.mowziesmobs.server.entity.frostmaw.EntityFrostmaw;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * A player steals a sleeping Frostmaw's ice crystal (MOWZIE.md §4.5; MZB-276). The jar's
 * {@code EntityFrostmaw.tick} takes the crystal in its asleep branch and then calls
 * {@code setHasCrystal(false)}, the only call of that setter in {@code tick} (cfr of 1.8.2, line 615;
 * bytecode offset 3327). The injection sits just after that call, not on the setter, which loading NBT
 * also calls ({@code readAdditionalSaveData}).
 *
 * <p>Two things follow, server side only. A body a camp holds is made persistent: the crystal is the jar's
 * only reason to keep a Frostmaw ({@code requiresCustomPersistence()} is {@code getHasCrystal()}), so a
 * robbed one would despawn, and a robbed den Frostmaw must stay disarmed in its den, not walk off to be
 * refilled and robbed again (MOWZIE.md "Robbed Frostmaw"). And the camp reconciler reports CHIEF_STATE with
 * the crystal flag off, for a camp body or a chief puppet; any other Frostmaw is the jar's (MZB-277).
 * Rare (a few steals a world), so one injection costs nothing per tick (D1).
 */
@Mixin(EntityFrostmaw.class)
public abstract class EntityFrostmawMixin {

    @Inject(method = "tick()V", at = @At(value = "INVOKE",
            target = "Lcom/bobmowzie/mowziesmobs/server/entity/frostmaw/EntityFrostmaw;setHasCrystal(Z)V",
            shift = At.Shift.AFTER), allow = 1)
    private void mic_rtsim_mowziesmobs$stolen(CallbackInfo ci) {
        EntityFrostmaw self = (EntityFrostmaw) (Object) this;
        if (self.level().isClientSide())
            return;
        if (RtsimApi.isCampBody(self))
            self.setPersistenceRequired();
        RtsimApi.chiefState(self, 0);
    }
}
