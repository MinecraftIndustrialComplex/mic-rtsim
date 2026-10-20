package com.minecraftindustrialcomplex.mic_rtsim_ribbits.mixin;

import com.llamalad7.mixinextras.injector.ModifyExpressionValue;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.knowledge.Sentiments;
import com.minecraftindustrialcomplex.mic_rtsim.village.Village;
import com.yungnickyoung.minecraft.ribbits.entity.RibbitEntity;
import com.yungnickyoung.minecraft.ribbits.entity.goal.RibbitApplyBuffGoal;
import net.minecraft.world.entity.player.Player;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;

import java.util.ArrayList;
import java.util.List;

/**
 * Sorcerers remember (C7, PLAN.md §8.8, walkthrough F). {@code RibbitApplyBuffGoal} finds the players
 * near the sorcerer with {@code Level.getNearbyPlayers(TargetingConditions, LivingEntity, AABB)}, in
 * {@code canUse} (is anyone there to buff?) and in {@code applyBuffs} (who gets the random effect);
 * verified in the Ribbits 4.1.6 bytecode. Both lists are filtered by the sorcerer's village's sentiment
 * snapshot (SENTIMENT): a player the village dislikes (at or below the engine's threshold, a P18
 * tunable) is left out. A friend, or a player the village has no feeling about, is buffed as Ribbits
 * always did. With nobody left, the sorcerer does not start casting.
 */
@Mixin(RibbitApplyBuffGoal.class)
public abstract class RibbitApplyBuffGoalMixin {
    @Shadow
    @Final
    private RibbitEntity ribbit;

    @ModifyExpressionValue(method = {"canUse", "applyBuffs"}, at = @At(value = "INVOKE",
            target = "Lnet/minecraft/world/level/Level;getNearbyPlayers(Lnet/minecraft/world/entity/ai/targeting/TargetingConditions;Lnet/minecraft/world/entity/LivingEntity;Lnet/minecraft/world/phys/AABB;)Ljava/util/List;"))
    private List<Player> mic_rtsim_ribbits$leaveOutTheDisliked(List<Player> players) {
        if (players.isEmpty())
            return players;
        Village v = RtsimApi.villageOf(this.ribbit);
        Sentiments.Snapshot s = v == null ? null : RtsimApi.sentiment(v.id);
        if (s == null)
            return players;
        List<Player> kept = new ArrayList<>(players.size());
        for (Player p : players)
            if (!s.dislikes(p.getUUID()))
                kept.add(p);
        return kept;
    }
}
