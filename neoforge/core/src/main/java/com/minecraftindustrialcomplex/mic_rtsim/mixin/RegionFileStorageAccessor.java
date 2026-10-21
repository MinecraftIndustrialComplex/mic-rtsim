package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import net.minecraft.nbt.CompoundTag;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.storage.RegionFileStorage;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;
import org.spongepowered.asm.mixin.gen.Invoker;

import java.io.IOException;
import java.nio.file.Path;

/**
 * {@code RegionFileStorage}: its protected {@code write} (what {@code IOWorker.runStore} calls), its folder and
 * whether it writes with DSYNC. Used by {@link IOWorkerMixin} and the journal's chunk barrier (PATCHES.md row 41).
 */
@Mixin(RegionFileStorage.class)
public interface RegionFileStorageAccessor {
    @Invoker("write")
    void mic_rtsim$write(ChunkPos pos, CompoundTag data) throws IOException;

    @Accessor("folder")
    Path mic_rtsim$folder();

    @Accessor("sync")
    boolean mic_rtsim$sync();
}
