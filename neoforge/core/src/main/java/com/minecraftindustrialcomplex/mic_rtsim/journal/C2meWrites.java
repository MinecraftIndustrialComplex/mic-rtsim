package com.minecraftindustrialcomplex.mic_rtsim.journal;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import it.unimi.dsi.fastutil.longs.Long2ReferenceMap;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.storage.IOWorker;
import net.minecraft.world.level.chunk.storage.RegionFileStorage;
import org.jetbrains.annotations.Nullable;

import java.lang.reflect.Field;
import java.nio.file.Path;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.Executor;
import java.util.concurrent.TimeUnit;

/**
 * C2ME's chunk storage (0.4.0-alpha, {@code c2me-rewrites-chunkio}), read by reflection (PATCHES.md row 42,
 * docs/choices-journal-barrier.md JB3).
 *
 * <p>C2ME replaces each level's chunk {@code IOWorker} with {@code C2MEStorageVanillaInterface}, whose
 * {@code backend} is a {@code C2MEStorageThread}. A store puts the position in the thread's {@code cache}
 * (and its write backlog); the write's last step, on the storage thread, removes it from {@code cache} and
 * writes the region file in the same task ({@code if (cache.remove(pos, nbtFuture)) invokeWriteChunk}). So a
 * position missing from {@code cache}, looked at on the storage thread after the store was queued, is on disk
 * (or its write failed; C2ME logs it). C2ME compresses on its own pool and writes as soon as each is ready, so
 * nothing is moved ahead: the barrier looks every {@link ChunkWriteBarrier#POLL_MILLIS} ms with a small task on
 * the storage thread's executor (the cache is that thread's alone). Its {@code synchronize}, by contrast, is a
 * full flush: it waits for every queued write, then fsyncs every open region file, on the storage thread,
 * blocking its reads.
 *
 * <p>C2ME also forces region writes without DSYNC (unless {@code -Dcom.ishland.c2me.chunkio.syncDiskWrites=true}),
 * so the barrier fsyncs the noted region files itself once they are written.
 */
final class C2meWrites implements ChunkWriteBarrier.LevelWrites {
    static final String INTERFACE = "com.ishland.c2me.rewrites.chunkio.common.C2MEStorageVanillaInterface";

    private final Executor executor;
    private final Long2ReferenceMap<?> cache;
    @Nullable
    private final RegionFileStorage storage;

    private C2meWrites(Executor executor, Long2ReferenceMap<?> cache, @Nullable RegionFileStorage storage) {
        this.executor = executor;
        this.cache = cache;
        this.storage = storage;
    }

    /** Null when {@code worker} is not C2ME's, or its fields are not what this was written against (logged). */
    @Nullable
    static C2meWrites of(IOWorker worker) {
        if (!worker.getClass().getName().equals(INTERFACE))
            return null;
        try {
            Object backend = field(worker.getClass(), "backend").get(worker);
            Class<?> thread = backend.getClass();
            Executor executor = (Executor) field(thread, "executor").get(backend);
            Long2ReferenceMap<?> cache = (Long2ReferenceMap<?>) field(thread, "cache").get(backend);
            RegionFileStorage storage = null;
            try {
                storage = (RegionFileStorage) field(thread, "storage").get(backend);
            } catch (ReflectiveOperationException | ClassCastException e) {
                MicRtsim.LOGGER.warn("[mic_rtsim] C2ME's storage thread has no RegionFileStorage field: its region files are not forced to disk before the journal", e);
            }
            return new C2meWrites(executor, cache, storage);
        } catch (ReflectiveOperationException | RuntimeException e) {
            MicRtsim.LOGGER.warn("[mic_rtsim] C2ME's chunk storage is not the one this was written against (0.4.0-alpha): the journal waits for its full flush", e);
            return null;
        }
    }

    private static Field field(Class<?> c, String name) throws NoSuchFieldException {
        Field f = c.getDeclaredField(name);
        f.setAccessible(true);
        return f;
    }

    @Override
    public CompletableFuture<List<ChunkPos>> landed(List<ChunkPos> positions) {
        long[] keys = new long[positions.size()];
        for (int i = 0; i < keys.length; i++)
            keys[i] = positions.get(i).toLong();
        CompletableFuture<List<ChunkPos>> done = new CompletableFuture<>();
        look(keys, done);
        return done;
    }

    private void look(long[] keys, CompletableFuture<List<ChunkPos>> done) {
        if (done.isDone())
            return; // written, or the round gave up and cancelled it
        try {
            executor.execute(() -> {
                try {
                    for (long k : keys) {
                        if (cache.containsKey(k)) {
                            CompletableFuture.delayedExecutor(ChunkWriteBarrier.POLL_MILLIS, TimeUnit.MILLISECONDS)
                                    .execute(() -> look(keys, done));
                            return;
                        }
                    }
                    done.complete(List.of());
                } catch (RuntimeException e) {
                    done.completeExceptionally(e);
                }
            });
        } catch (RuntimeException e) {
            done.completeExceptionally(e);
        }
    }

    @Override
    public Path regionFolder() {
        return ChunkWriteBarrier.folderIfNotSync(storage);
    }

    @Override
    public String kind() {
        return "c2me";
    }
}
