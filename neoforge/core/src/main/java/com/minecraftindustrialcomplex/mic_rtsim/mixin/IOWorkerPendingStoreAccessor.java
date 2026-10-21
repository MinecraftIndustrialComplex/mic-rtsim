package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import net.minecraft.nbt.CompoundTag;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

import java.util.concurrent.CompletableFuture;

/**
 * {@code IOWorker.PendingStore} (package-private): a queued chunk write, its latest data and the future its
 * store returned. Used by {@link IOWorkerMixin} (PATCHES.md row 40).
 */
@Mixin(targets = "net.minecraft.world.level.chunk.storage.IOWorker$PendingStore")
public interface IOWorkerPendingStoreAccessor {
    @Accessor("data")
    CompoundTag mic_rtsim$data();

    @Accessor("result")
    CompletableFuture<Void> mic_rtsim$result();
}
