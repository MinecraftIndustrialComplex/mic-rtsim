package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import net.minecraft.world.entity.Mob;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/**
 * {@code Mob.persistenceRequired} has a setter that only sets it: a conflict body carried off by a player
 * gives back the persistence it had before the bridge held it (MZB-102).
 */
@Mixin(Mob.class)
public interface MobAccessor {
    @Accessor("persistenceRequired")
    void mic_rtsim$setPersistenceRequired(boolean required);
}
