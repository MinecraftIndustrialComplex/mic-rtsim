package com.minecraftindustrialcomplex.mic_rtsim.habitat;

import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.EntityJoinLevelEvent;
import net.neoforged.neoforge.event.level.ChunkEvent;
import net.neoforged.neoforge.event.level.LevelEvent;

/**
 * The NeoForge hooks of the habitat sampler (PATCHES.md): chunk loads and unloads, chunk-generation animals
 * joining, and a level unloading. No mixin.
 */
public final class HabitatEvents {
    private HabitatEvents() {}

    public static void register() {
        RtsimApi.registerPlugin(HabitatSampler.INSTANCE);
        IEventBus bus = NeoForge.EVENT_BUS;
        bus.addListener(EventPriority.LOWEST, false, ChunkEvent.Load.class, HabitatEvents::onLoad);
        bus.addListener(EventPriority.LOWEST, false, ChunkEvent.Unload.class, HabitatEvents::onUnload);
        bus.addListener(EventPriority.LOWEST, false, EntityJoinLevelEvent.class, HabitatEvents::onJoin);
        bus.addListener(EventPriority.LOWEST, false, LevelEvent.Unload.class, HabitatEvents::onLevelUnload);
    }

    private static void onLoad(ChunkEvent.Load event) {
        if (event.getLevel() instanceof ServerLevel level && event.getChunk() instanceof LevelChunk chunk)
            HabitatSampler.INSTANCE.onChunkLoad(level, chunk, event.isNewChunk());
    }

    private static void onUnload(ChunkEvent.Unload event) {
        if (event.getLevel() instanceof ServerLevel level)
            HabitatSampler.INSTANCE.onChunkUnload(level, event.getChunk().getPos());
    }

    private static void onJoin(EntityJoinLevelEvent event) {
        if (event.getLevel() instanceof ServerLevel level)
            HabitatSampler.INSTANCE.onEntityJoin(level, event.getEntity(), event.loadedFromDisk());
    }

    private static void onLevelUnload(LevelEvent.Unload event) {
        if (event.getLevel() instanceof ServerLevel level)
            HabitatSampler.forgetLoaded(level.dimension());
    }
}
