package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import net.minecraft.world.item.trading.MerchantOffer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/**
 * {@code MerchantOffer.uses} and {@code demand} are private with no setters. Uses follow the treasury and
 * the cargo (C14, C21); demand is held at 0 on offers the sim prices, because vanilla's demand would
 * otherwise read the sim's uses as sales and raise the price on top of the market's.
 */
@Mixin(MerchantOffer.class)
public interface MerchantOfferAccessor {
    @Accessor("uses")
    void mic_rtsim$setUses(int uses);

    @Accessor("demand")
    void mic_rtsim$setDemand(int demand);
}
