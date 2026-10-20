package com.minecraftindustrialcomplex.mic_rtsim_ribbits.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.yungnickyoung.minecraft.ribbits.entity.RibbitEntity;
import net.minecraft.world.item.trading.MerchantOffer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/**
 * Ribbit merchants restock through their village's treasury (C23, PLAN.md §8.5). Ribbits copies
 * vanilla's villager restock: {@code RibbitEntity.restock} and {@code RibbitEntity.catchUpDemand} reset
 * every offer's uses with {@code MerchantOffer.resetUses} (verified in the 4.1.6 bytecode). In an
 * adopted ribbit village with a market, the uses then follow the treasury rule instead; elsewhere
 * nothing changes.
 */
@Mixin(RibbitEntity.class)
public abstract class RibbitEntityMixin {

    @WrapOperation(method = {"restock", "catchUpDemand"}, at = @At(value = "INVOKE",
            target = "Lnet/minecraft/world/item/trading/MerchantOffer;resetUses()V"))
    private void mic_rtsim_ribbits$treasuryUses(MerchantOffer offer, Operation<Void> original) {
        original.call(offer);
        RtsimApi.afterResetUses((RibbitEntity) (Object) this, offer);
    }

    /** As for villagers: an offer the sim prices keeps demand 0 ({@code RibbitEntity.updateDemand}, 4.1.6 bytecode). */
    @WrapOperation(method = "updateDemand", at = @At(value = "INVOKE",
            target = "Lnet/minecraft/world/item/trading/MerchantOffer;updateDemand()V"))
    private void mic_rtsim_ribbits$marketDemand(MerchantOffer offer, Operation<Void> original) {
        if (!RtsimApi.pricesOffer((RibbitEntity) (Object) this, offer))
            original.call(offer);
    }
}
