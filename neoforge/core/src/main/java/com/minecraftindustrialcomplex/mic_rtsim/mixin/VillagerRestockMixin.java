package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import net.minecraft.world.entity.npc.Villager;
import net.minecraft.world.item.trading.MerchantOffer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/**
 * Restock through the treasury (C14, PLAN.md §8.5). {@code Villager.restock} and
 * {@code Villager.catchUpDemand} reset every offer's uses ({@code MerchantOffer.resetUses}); in an
 * adopted village with a market the uses then follow the treasury rule instead (stock for what the
 * village sells, treasury for what it pays for). Vanilla's bookkeeping (demand, restock count, the
 * resend to a trading player) is untouched; outside adopted villages nothing changes.
 */
@Mixin(Villager.class)
public abstract class VillagerRestockMixin {

    @WrapOperation(method = {"restock", "catchUpDemand"}, at = @At(value = "INVOKE",
            target = "Lnet/minecraft/world/item/trading/MerchantOffer;resetUses()V"))
    private void mic_rtsim$treasuryUses(MerchantOffer offer, Operation<Void> original) {
        original.call(offer);
        RtsimSession s = RtsimSession.current();
        if (s != null && !s.failed())
            s.phase1.afterResetUses((Villager) (Object) this, offer);
    }

    /**
     * {@code Villager.updateDemand} raises each offer's demand by its uses, and demand raises the price;
     * an offer the sim prices keeps demand 0, so its price is the market's (reputation still applies).
     */
    @WrapOperation(method = "updateDemand", at = @At(value = "INVOKE",
            target = "Lnet/minecraft/world/item/trading/MerchantOffer;updateDemand()V"))
    private void mic_rtsim$marketDemand(MerchantOffer offer, Operation<Void> original) {
        RtsimSession s = RtsimSession.current();
        if (s == null || s.failed() || !s.phase1.pricesOffer((Villager) (Object) this, offer))
            original.call(offer);
    }
}
