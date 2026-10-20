package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import net.minecraft.world.item.trading.MerchantOffer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** {@code MerchantOffer.uses} is private with no setter; uses follow the treasury and the cargo (C14, C21). */
@Mixin(MerchantOffer.class)
public interface MerchantOfferAccessor {
    @Accessor("uses")
    void mic_rtsim$setUses(int uses);
}
