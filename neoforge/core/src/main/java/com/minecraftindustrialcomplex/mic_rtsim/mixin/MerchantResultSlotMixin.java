package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import com.llamalad7.mixinextras.sugar.Local;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.MerchantResultSlot;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.trading.Merchant;
import net.minecraft.world.item.trading.MerchantOffer;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Every trade with any merchant passes {@code MerchantResultSlot.onTake}, which calls
 * {@code Merchant.notifyTrade(offer)} once per trade after the payment was taken (1.21.1 source). Right
 * after it, the bridge journals the trade (TRADE, with an op id) and moves the market or the caravan's
 * cargo. One hook covers villagers, wandering traders and other mods' merchants (Ribbits) alike;
 * NeoForge's {@code TradeWithVillagerEvent} only covers {@code AbstractVillager} (choice G6).
 */
@Mixin(MerchantResultSlot.class)
public abstract class MerchantResultSlotMixin {
    @Shadow
    @Final
    private Merchant merchant;

    @Inject(method = "onTake", at = @At(value = "INVOKE",
            target = "Lnet/minecraft/world/item/trading/Merchant;notifyTrade(Lnet/minecraft/world/item/trading/MerchantOffer;)V",
            shift = At.Shift.AFTER))
    private void mic_rtsim$traded(Player player, ItemStack stack, CallbackInfo ci, @Local MerchantOffer offer) {
        if (!(this.merchant instanceof Entity e) || e.level().isClientSide())
            return;
        RtsimSession s = RtsimSession.current();
        if (s != null && !s.failed())
            s.phase1.onTrade(e, offer, player);
    }
}
