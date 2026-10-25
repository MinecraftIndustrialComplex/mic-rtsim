package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import net.minecraft.world.level.entity.EntityPersistentStorage;
import net.minecraft.world.level.entity.PersistentEntitySectionManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;
import org.spongepowered.asm.mixin.gen.Invoker;

import java.util.function.Consumer;

/**
 * The entity manager's storage ({@code EntityStorage}): the GameTests read saved entity chunks through it, and
 * the journal's barrier reaches the entity chunks' IOWorker through it (MOWZIE.md ledger 22).
 */
@Mixin(PersistentEntitySectionManager.class)
public interface EntitySectionManagerAccessor {
    @Accessor("permanentStorage")
    EntityPersistentStorage<?> mic_rtsim$permanentStorage();

    /**
     * {@code storeChunkSections} (private): vanilla's own store of one entity chunk, as its autosave does it,
     * so the barrier can put an entity chunk a camp record speaks for on its way to disk. False while the
     * chunk's entities are still loading (nothing to store yet; vanilla asks for the load).
     */
    @Invoker("storeChunkSections")
    boolean mic_rtsim$store(long chunkPos, Consumer<?> action);
}
