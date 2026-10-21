package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import com.minecraftindustrialcomplex.mic_rtsim.bridge.TraderSpawning;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.npc.WanderingTraderSpawner;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * The spawn veto (C8a): while the sim owns wandering traders ({@link TraderSpawning#simOwnsTraders},
 * trade and travel both on), vanilla's random trader spawner does nothing. Returning at the head of
 * {@code tick} is exactly what the {@code doTraderSpawning} gamerule check at the head of the same
 * method does: the countdown, the chance and the level data's trader fields stand still, so switching
 * the modules off lets the spawner carry on from where it was.
 *
 * <p>Only this spawner is touched. The caravan traders the sim spawns itself (puppets), traders other
 * mods spawn and the gamerule are not: the veto is read at every tick, so it follows the config switches
 * with no sync and nothing saved.
 */
@Mixin(WanderingTraderSpawner.class)
public abstract class WanderingTraderSpawnerMixin {

    @Inject(method = "tick", at = @At("HEAD"), cancellable = true)
    private void mic_rtsim$simOwnsTraders(ServerLevel level, boolean spawnHostiles, boolean spawnPassives,
                                          CallbackInfoReturnable<Integer> cir) {
        if (TraderSpawning.simOwnsTraders())
            cir.setReturnValue(0);
    }
}
