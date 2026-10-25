package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import net.minecraft.world.level.chunk.storage.IOWorker;
import net.minecraft.world.level.chunk.storage.SimpleRegionStorage;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** A region storage's IOWorker (the entity storage's, for the journal's barrier; MOWZIE.md ledger 22). */
@Mixin(SimpleRegionStorage.class)
public interface SimpleRegionStorageAccessor {
    @Accessor("worker")
    IOWorker mic_rtsim$worker();
}
