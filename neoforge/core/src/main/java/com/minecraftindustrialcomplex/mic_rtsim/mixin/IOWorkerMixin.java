package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import com.minecraftindustrialcomplex.mic_rtsim.journal.ChunkWriteBarrier;
import com.mojang.datafixers.util.Either;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.storage.IOWorker;
import net.minecraft.world.level.chunk.storage.RegionFileStorage;
import org.slf4j.Logger;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CompletableFuture;
import java.util.function.Supplier;

/**
 * Vanilla's chunk {@code IOWorker}: the journal's chunk barrier writes the chunks it waits for ahead of the
 * worker's queue (PATCHES.md row 40, docs/choices-journal-barrier.md JB2).
 *
 * <p>Vanilla queues each store in {@code pendingWrites} (a {@code LinkedHashMap}: a position stored again keeps
 * its place) and writes one entry a background turn, oldest first, after every foreground task (stores,
 * reads, scans) queued. A chunk saved at a world save waits behind every chunk queued before it: on a GameTest
 * server, which ticks unthrottled, tens of seconds. {@link #mic_rtsim$writeNow} runs as a foreground task (the
 * priority of a store, so after every store submitted before it) and writes the given positions' pending
 * entries there, as {@code storePendingChunk} would (remove the entry, then {@code runStore}'s two steps).
 * A position with no pending entry was written already: its store came first, and writes run on this
 * worker's mailbox, one task at a time.
 *
 * <p>Only for an instance of vanilla's own class: C2ME's {@code C2MEStorageVanillaInterface} extends it but
 * nulls these fields (the barrier checks {@code getClass() == IOWorker.class}).
 */
@Mixin(IOWorker.class)
public abstract class IOWorkerMixin implements ChunkWriteBarrier.TargetedWrites {
    @Shadow
    @Final
    private static Logger LOGGER;

    @Shadow
    @Final
    private Map<ChunkPos, Object> pendingWrites;

    @Shadow
    @Final
    private RegionFileStorage storage;

    @Shadow
    private <T> CompletableFuture<T> submitTask(Supplier<Either<T, Exception>> task) {
        throw new AssertionError();
    }

    @Override
    public RegionFileStorage mic_rtsim$storage() {
        return storage;
    }

    @Override
    public CompletableFuture<List<ChunkPos>> mic_rtsim$writeNow(List<ChunkPos> positions) {
        return this.submitTask(() -> {
            List<ChunkPos> failed = new ArrayList<>(0);
            for (ChunkPos pos : positions) {
                Object pending = pendingWrites.remove(pos);
                if (pending == null)
                    continue;
                IOWorkerPendingStoreAccessor p = (IOWorkerPendingStoreAccessor) pending;
                try {
                    ((RegionFileStorageAccessor) (Object) storage).mic_rtsim$write(pos, p.mic_rtsim$data());
                    p.mic_rtsim$result().complete(null);
                } catch (Exception e) {
                    LOGGER.error("Failed to store chunk {}", pos, e);
                    p.mic_rtsim$result().completeExceptionally(e);
                    failed.add(pos);
                }
            }
            return Either.left(failed);
        });
    }
}
