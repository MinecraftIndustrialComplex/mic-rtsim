package com.minecraftindustrialcomplex.mic_rtsim.sites;

import com.minecraftindustrialcomplex.mic_rtsim.mixin.ChunkMapInvoker;
import com.minecraftindustrialcomplex.mic_rtsim.mixin.EntitySectionManagerAccessor;
import com.minecraftindustrialcomplex.mic_rtsim.mixin.ServerLevelAccessor;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ChunkHolder;
import net.minecraft.server.level.ChunkLevel;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.Level;

import java.util.function.Function;

/**
 * {@link ChunkWatch}'s source over the game's levels (kept apart so that the tracker itself needs no Minecraft class
 * and is unit-tested in plain Java): the dimension key is the level's {@code ResourceKey<Level>}.
 */
public final class VanillaChunks {
    private VanillaChunks() {}

    /** The chunk map's holders and the entity manager's visibility, read the way the camps read them (MZB-660). */
    public static ChunkWatch.Source source(Function<ResourceKey<Level>, ServerLevel> levels) {
        return new ChunkWatch.Source() {
            @Override
            @SuppressWarnings("unchecked")
            public int level(Object dimension, long chunk) {
                ServerLevel level = levels.apply((ResourceKey<Level>) dimension);
                if (level == null)
                    return ChunkWatch.NO_HOLDER;
                ChunkMapInvoker map = (ChunkMapInvoker) level.getChunkSource().chunkMap;
                ChunkHolder h = map.mic_rtsim$updating(chunk);
                return h == null ? ChunkWatch.NO_HOLDER : norm(h.getTicketLevel());
            }

            @Override
            @SuppressWarnings("unchecked")
            public boolean entitiesIn(Object dimension, long chunk) {
                ServerLevel level = levels.apply((ResourceKey<Level>) dimension);
                if (level == null || !level.areEntitiesLoaded(chunk))
                    return false;
                var manager = (EntitySectionManagerAccessor) ((ServerLevelAccessor) level).mic_rtsim$entityManager();
                return manager.mic_rtsim$chunkVisibility().get(chunk).isAccessible();
            }
        };
    }

    /** A ticket level as kept: every level above the maximum is an unloaded chunk and reads {@link ChunkWatch#NO_HOLDER}. */
    public static int norm(int level) {
        return level > ChunkLevel.MAX_LEVEL ? ChunkWatch.NO_HOLDER : level;
    }
}
