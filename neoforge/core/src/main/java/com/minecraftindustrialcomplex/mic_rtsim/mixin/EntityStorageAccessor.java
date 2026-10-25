package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import net.minecraft.world.level.chunk.storage.EntityStorage;
import net.minecraft.world.level.chunk.storage.SimpleRegionStorage;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** The entity storage's region storage: its IOWorker is the one the barrier asks for an entity chunk's write. */
@Mixin(EntityStorage.class)
public interface EntityStorageAccessor {
    @Accessor("simpleRegionStorage")
    SimpleRegionStorage mic_rtsim$regions();
}
