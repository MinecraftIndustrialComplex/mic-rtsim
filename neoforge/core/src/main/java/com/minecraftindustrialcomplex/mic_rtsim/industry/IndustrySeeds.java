package com.minecraftindustrialcomplex.mic_rtsim.industry;

import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.chunk.LevelChunk;

import java.util.HashMap;

/**
 * Seeds a plugin records with no player (the oil gate audit's F1, choice OGC60). DS-10 seeds a district
 * only where a player placed a machine; a pumpjack hole is the one exception: it seeds its own district
 * whoever, or whatever, set it down (a contraption, a command, a structure, another mod), so an oil well
 * is always a district the oil gate can see and the engine can tax. The dieselgenerators plugin is the
 * only caller. Do not widen it: every other machine still seeds only when a player places it.
 */
public final class IndustrySeeds {
    private IndustrySeeds() {}

    /**
     * Records {@code be}, a block entity in {@code chunk}, as one of the chunk's seeds and asks for a
     * rescan of the chunk when it was not one already. A block entity that is not a seed
     * ({@link Districts#seed}: a pack can tag it out) is left alone. Returns false only when nothing could be
     * decided (no industry module is running, or the block entity is gone or elsewhere), so the caller
     * tries again later; true when it is done with.
     */
    public static boolean record(ServerLevel level, LevelChunk chunk, BlockEntity be) {
        Industry ind = IndustryApi.current();
        if (ind == null || be.isRemoved())
            return false;
        BlockPos pos = be.getBlockPos();
        if (pos.getX() >> 4 != chunk.getPos().x || pos.getZ() >> 4 != chunk.getPos().z || chunk.getBlockEntities().get(pos) != be)
            return false;
        if (!ind.districts().seed(level, be))
            return true;
        IndustryAttachments.Seeds s = chunk.getData(IndustryAttachments.SEEDS.get());
        if (s.contains(pos.asLong()))
            return true;
        chunk.setData(IndustryAttachments.SEEDS.get(), s.plus(pos.asLong()));
        chunk.setUnsaved(true);
        ind.districts().markDirty(level.dimension(), chunk.getPos().toLong());
        return true;
    }
}
