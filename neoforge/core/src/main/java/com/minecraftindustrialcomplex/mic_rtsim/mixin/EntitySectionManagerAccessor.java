package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import it.unimi.dsi.fastutil.longs.Long2ObjectMap;
import net.minecraft.world.level.entity.EntityPersistentStorage;
import net.minecraft.world.level.entity.PersistentEntitySectionManager;
import net.minecraft.world.level.entity.Visibility;
import it.unimi.dsi.fastutil.longs.LongSet;
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
     * The visibility of each entity chunk (HIDDEN where absent). A camp reads it beside
     * {@code areEntitiesLoaded}: under C2ME a chunk can be HIDDEN with its entities loaded (MZB-660), and
     * the census does not see a hidden entity.
     */
    @Accessor("chunkVisibility")
    Long2ObjectMap<Visibility> mic_rtsim$chunkVisibility();

    /**
     * The entity chunks queued for unloading. {@code updateChunkStatus(INACCESSIBLE)} queues the chunk, and the
     * next {@code processUnloads} stores it (HIDDEN and LOADED), so a GameTest that forces the state C2ME leaves
     * (HIDDEN, entities loaded, not queued) takes the chunk out of this set right after the status change (MZB-660).
     */
    @Accessor("chunksToUnload")
    LongSet mic_rtsim$chunksToUnload();

    /**
     * {@code storeChunkSections} (private): vanilla's own store of one entity chunk, as its autosave does it,
     * so the barrier can put an entity chunk a camp record speaks for on its way to disk. False while the
     * chunk's entities are still loading (nothing to store yet; vanilla asks for the load).
     */
    @Invoker("storeChunkSections")
    boolean mic_rtsim$store(long chunkPos, Consumer<?> action);

    /**
     * {@code chunkLoadStatuses} (private): each entity chunk's load state, a package-private enum (FRESH when
     * absent, PENDING while its read is in flight, LOADED). Read by GameTests only ({@code ChunkWait.inMemory}):
     * a PENDING chunk's entities are on their way into the level.
     */
    @Accessor("chunkLoadStatuses")
    Long2ObjectMap<?> mic_rtsim$chunkLoadStatuses();
}
