package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import net.minecraft.world.level.entity.PersistentEntitySectionManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

import java.util.function.Consumer;

/**
 * {@code PersistentEntitySectionManager.storeChunkSections} (private): vanilla's own store of one entity
 * chunk, as its autosave does it (a no-op action on each entity), so the journal's barrier can put an entity
 * chunk a camp record speaks for on its way to disk (MOWZIE.md ledger 22). False when the chunk's entities
 * are still loading (nothing to store yet).
 */
@Mixin(PersistentEntitySectionManager.class)
public interface EntityManagerInvoker {
    @Invoker("storeChunkSections")
    boolean mic_rtsim$store(long chunkPos, Consumer<?> action);
}
