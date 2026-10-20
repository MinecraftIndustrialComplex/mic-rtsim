package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.entity.PersistentEntitySectionManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/**
 * The level's entity manager. Used by the GameTests to run vanilla's own entity-chunk save
 * ({@code autoSave}) and read the chunk back from disk, so "a puppet is not saved" is tested on the
 * real save path.
 */
@Mixin(ServerLevel.class)
public interface ServerLevelAccessor {
    @Accessor("entityManager")
    PersistentEntitySectionManager<Entity> mic_rtsim$entityManager();
}
