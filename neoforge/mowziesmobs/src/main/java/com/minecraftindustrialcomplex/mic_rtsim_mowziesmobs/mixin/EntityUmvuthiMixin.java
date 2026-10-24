package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.mixin;

import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthi;
import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import net.minecraft.world.entity.player.Player;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

import java.util.List;

/**
 * Two hooks on the jar's Umvuthi (MZB-418, MZB-431), both server side and both rare.
 *
 * <p>{@code rememberTrade} is called only from {@code MessageUmvuthiTrade.handleServer}, right after
 * {@code fulfillDesire} took the payment (its desires stack, the exact item and count, from the input slot). At its
 * head the payment is reported to the camp reconciler as CHIEF_PAID; the jar's own memory entry follows as
 * ever. The jar's {@code die} takes the devoted Umvuthana within 30 blocks with it; for a camp's second chief
 * body (a body the engine already replaced, MZB-431) the search is empty, so a removed old chief takes no warrior with it.
 */
@Mixin(EntityUmvuthi.class)
public abstract class EntityUmvuthiMixin {

    @Inject(method = "rememberTrade", at = @At("HEAD"))
    private void mic_rtsim_mowziesmobs$paid(Player player, CallbackInfo ci) {
        EntityUmvuthi self = (EntityUmvuthi) (Object) this;
        if (self.level().isClientSide())
            return;
        RtsimApi.chiefPaid(self, player, List.of(self.getDesires().copy()));
    }

    @WrapOperation(method = "die", at = @At(value = "INVOKE",
            target = "Lcom/bobmowzie/mowziesmobs/server/entity/umvuthana/EntityUmvuthi;getEntitiesNearby(Ljava/lang/Class;DDDD)Ljava/util/List;"),
            allow = 1)
    private List<?> mic_rtsim_mowziesmobs$devoted(EntityUmvuthi self, Class<?> type, double x, double y, double z, double range,
                                                  Operation<List<?>> original) {
        return RtsimApi.secondChiefBody(self) ? List.of() : original.call(self, type, x, y, z, range);
    }
}
