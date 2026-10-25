package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.mixin;

import com.bobmowzie.mowziesmobs.server.entity.frostmaw.EntityFrostmaw;
import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.FrostmawLurker;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.ai.navigation.PathNavigation;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
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
 *
 * <p>A lurk puppet is held where it stands by {@link FrostmawLurker} (MZB-361), and the jar's own chase must not
 * move it: {@code tick()} paths to its target every tick it is more than 5.5 blocks off ({@code moveTo(target, ...)},
 * three calls) and stops the navigation when it is not ({@code stop()}, three calls), so the hold's walk-back would
 * lose to the chase about nine ticks in ten and the puppet would walk onto the rail after a player. For a body the
 * hold has in hand, those six calls do nothing. The counts are the 1.8.2 jar's (cfr lines 423, 532, 552, 554, 557,
 * 591); a jar that has others fails to load rather than leave a chase in.
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

    /** The chase: a held puppet is not walked to its target by the jar. */
    @WrapOperation(method = "tick()V", at = @At(value = "INVOKE",
            target = "Lnet/minecraft/world/entity/ai/navigation/PathNavigation;moveTo(Lnet/minecraft/world/entity/Entity;D)Z"),
            require = 3, allow = 3)
    private boolean mic_rtsim_mowziesmobs$noChase(PathNavigation nav, Entity target, double speed, Operation<Boolean> original) {
        return mic_rtsim_mowziesmobs$held() ? Boolean.FALSE : original.call(nav, target, speed);
    }

    /** The jar's stops (it stands still in range, in a dodge, asleep) would drop the hold's walk-back path every tick. */
    @WrapOperation(method = "tick()V", at = @At(value = "INVOKE",
            target = "Lnet/minecraft/world/entity/ai/navigation/PathNavigation;stop()V"),
            require = 3, allow = 3)
    private void mic_rtsim_mowziesmobs$noStop(PathNavigation nav, Operation<Void> original) {
        if (!mic_rtsim_mowziesmobs$held())
            original.call(nav);
    }

    @Unique
    private boolean mic_rtsim_mowziesmobs$held() {
        EntityFrostmaw self = (EntityFrostmaw) (Object) this;
        return !self.level().isClientSide() && FrostmawLurker.holds(self);
    }
}
