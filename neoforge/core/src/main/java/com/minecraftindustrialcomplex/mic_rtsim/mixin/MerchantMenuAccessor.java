package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import net.minecraft.world.item.trading.Merchant;
import net.minecraft.world.inventory.MerchantMenu;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** Who a trade screen belongs to: TALK's first-trade hint (TK27) shows only for villagers and wandering traders. */
@Mixin(MerchantMenu.class)
public interface MerchantMenuAccessor {
    @Accessor("trader")
    Merchant mic_rtsim$trader();
}
