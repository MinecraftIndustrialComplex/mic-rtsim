package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.mixin;

import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.GroveTrade;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * A masked trade with a grove's minion (MZB-417). The jar's result slot takes the payment in {@code onTake}, by one
 * {@code ItemStack.shrink} on the input slot, only for a trade that goes through; the injection sits before
 * that call, so one take is one trade. The menu is the player's open one, which holds the minion (no accessor
 * needed). Server side only; the reconciler decides whether the body is a camp's.
 */
@Mixin(targets = "com.bobmowzie.mowziesmobs.server.inventory.ContainerUmvuthanaTrade$SlotResult")
public abstract class ContainerUmvuthanaTradeSlotResultMixin {

    @Inject(method = "onTake", at = @At(value = "INVOKE", target = "Lnet/minecraft/world/item/ItemStack;shrink(I)V"), allow = 1)
    private void mic_rtsim_mowziesmobs$traded(Player player, ItemStack stack, CallbackInfo ci) {
        GroveTrade.taken(player);
    }
}
