package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import net.minecraft.world.entity.npc.Villager;
import net.minecraft.world.entity.player.Player;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * {@code Villager.updateSpecialPrices(Player)} (private, run when trading starts) applies the player's
 * reputation and Hero of the Village to every offer's {@code specialPriceDiff}. Reputation stays on top of
 * the sim's prices (C20), but on an offer the sim prices it may not take the player's price past the
 * market's marginal price, which would let a loop of trades make money (C13; choice G31).
 */
@Mixin(Villager.class)
public abstract class VillagerReputationMixin {

    @Inject(method = "updateSpecialPrices", at = @At("TAIL"))
    private void mic_rtsim$clampReputation(Player player, CallbackInfo ci) {
        RtsimSession s = RtsimSession.current();
        if (s != null && !s.failed())
            s.phase1.afterSpecialPrices((Villager) (Object) this);
    }
}
