package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import net.minecraft.world.level.entity.EntityPersistentStorage;
import net.minecraft.world.level.entity.PersistentEntitySectionManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** The entity manager's storage ({@code EntityStorage}): the GameTests read saved entity chunks through it. */
@Mixin(PersistentEntitySectionManager.class)
public interface EntitySectionManagerAccessor {
    @Accessor("permanentStorage")
    EntityPersistentStorage<?> mic_rtsim$permanentStorage();
}
