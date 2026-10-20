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
 * reputation and Hero of the Village to every offer's {@code specialPriceDiff}. On an offer the sim prices,
 * the reputation is already inside the engine's bundle (G31, Ben: "Inside the sim quote": the player's
 * standing picks the MARKET's price level), so vanilla's change is taken off again: one formula sets the
 * price (choice PR1).
 */
@Mixin(Villager.class)
public abstract class VillagerReputationMixin {

    @Inject(method = "updateSpecialPrices", at = @At("TAIL"))
    private void mic_rtsim$foldReputation(Player player, CallbackInfo ci) {
        RtsimSession s = RtsimSession.current();
        if (s != null && !s.failed())
            s.phase1.afterSpecialPrices((Villager) (Object) this);
    }
}
