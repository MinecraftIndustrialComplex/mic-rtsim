package com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators;

import com.jesz.createdieselgenerators.content.pumpjack.PumpjackHoleBlockEntity;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustrySeeds;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.level.ChunkEvent;

/**
 * A pumpjack hole seeds its own district whoever placed it (the oil gate audit's F1, choice OGC60): a
 * contraption, a command, a structure or another mod sets one down with no player, and DS-10 alone would
 * leave that well outside every district, untaxed and unseen. Two hooks record each hole the server sees
 * as a seed of its chunk ({@link IndustrySeeds#record}): a chunk load, for the holes the chunk carries, and
 * the hole's first server tick ({@code PumpjackHoleSeedMixin}), for a hole set down in a loaded chunk and
 * for holes in chunks that loaded before the industry module started. The exception is the pumpjack's
 * alone; no other machine seeds without a player.
 */
public final class HoleSeeds {
    private HoleSeeds() {}

    static void register() {
        NeoForge.EVENT_BUS.addListener(HoleSeeds::onChunkLoad);
    }

    private static void onChunkLoad(ChunkEvent.Load event) {
        if (event.getLevel().isClientSide() || !(event.getLevel() instanceof ServerLevel level) || !(event.getChunk() instanceof LevelChunk chunk))
            return;
        for (BlockEntity be : chunk.getBlockEntities().values())
            if (be instanceof PumpjackHoleBlockEntity)
                IndustrySeeds.record(level, chunk, be); // false (no module yet): the hole's first tick records it
    }
}
