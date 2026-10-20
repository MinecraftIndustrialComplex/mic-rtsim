package com.minecraftindustrialcomplex.mic_rtsim.market;

import com.minecraftindustrialcomplex.mic_rtsim.mixin.MerchantOfferAccessor;
import net.minecraft.world.item.trading.MerchantOffer;

/**
 * Sets a vanilla offer's uses. {@code MerchantOffer.uses} has no setter (only {@code increaseUses},
 * {@code resetUses} and {@code setToOutOfStock}), so the bridge reaches it through an accessor mixin
 * (PATCHES.md). An offer whose uses reach its max uses shows crossed out.
 */
public final class Uses {
    private Uses() {}

    public static void set(MerchantOffer offer, int uses) {
        ((MerchantOfferAccessor) offer).mic_rtsim$setUses(Math.max(0, Math.min(uses, offer.getMaxUses())));
    }
}
