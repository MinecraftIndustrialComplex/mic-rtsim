package com.minecraftindustrialcomplex.mic_rtsim.journal;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.mixin.ChunkMapInvoker;
import net.minecraft.server.level.ChunkMap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.storage.ChunkScanAccess;
import net.minecraft.world.level.chunk.storage.IOWorker;
import net.neoforged.neoforge.common.IOUtilities;

import java.util.ArrayList;
import java.util.Collections;
import java.util.IdentityHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Set;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;
import java.util.concurrent.atomic.AtomicLong;

/**
 * The journal never reaches disk ahead of a chunk it speaks for (I8; docs/choices-repair.md R21 to R24).
 *
 * <p>A journaled record can claim something that lives in a chunk: a reconcile applied into it
 * (IND_RECONCILE_RESULT, its epoch in {@code mic_rtsim:reconcile}), a transfer, package, house or resident
 * whose COMMAND_RESULT the chunk carries ({@code mic_rtsim:pending}). Vanilla writes the journal (a
 * SavedData, in {@code saveLevelData}) before it serializes any chunk, both asynchronously, and skips a chunk
 * saved in the last 10 s ({@code ChunkMap.saveChunkIfNeeded}'s cooldown). A crash between the two writes left a
 * journal that says applied and a chunk on disk from before: the engine took the result and never sent the
 * command again, so the goods were lost.
 *
 * <p>So each such chunk is noted here ({@link #dependsOn}) when the record is committed. When the journal's
 * file is about to be written ({@link JournalSavedData#save(java.io.File, net.minecraft.core.HolderLookup.Provider)},
 * on the server thread), every noted chunk that is still unsaved goes through vanilla's own
 * {@code ChunkMap.save} (whatever the cooldown), and a task queued on NeoForge's SavedData IO chain right
 * before the journal's own write waits until those levels' chunk IO workers have written and flushed
 * everything queued so far ({@code IOWorker.synchronize(true)}, the flush vanilla's shutdown save uses). The
 * server thread never waits: it serializes each noted chunk once, which the same autosave would mostly have
 * done anyway; the wait is on an IO thread, and only the journal's write (and the SavedData writes queued
 * after it) waits.
 *
 * <p>The reverse order (a chunk on disk ahead of the journal) is the pending-record path's (docs/boundary.md
 * §3.1): the chunk carries the record until the engine acknowledges it.
 *
 * <p>Server thread only.
 */
public final class ChunkWriteBarrier {

    /**
     * How long the journal's write waits for the chunk writes (PLACEHOLDER). Past it the journal is written
     * anyway (the old order: a crash in the gap loses what those chunks held) and the log says so.
     */
    static final long WAIT_SECONDS = 60;

    private final Set<LevelChunk> chunks = Collections.newSetFromMap(new IdentityHashMap<>());
    private boolean warnedScanner;

    // counters (the GameTests read them)
    long noted;
    long forcedSaves;
    long waits;
    /** Written on an IO thread. */
    private final AtomicLong late = new AtomicLong();

    /**
     * A record just committed to the journal speaks for {@code chunk}'s contents: the chunk is written
     * before the journal's next file.
     */
    public void dependsOn(LevelChunk chunk) {
        if (chunks.add(chunk))
            noted++;
    }

    /** Chunks noted since the journal's last write. */
    public int pending() {
        return chunks.size();
    }

    public long forcedSaves() {
        return forcedSaves;
    }

    public long waits() {
        return waits;
    }

    /** Journal writes that went ahead without the chunk writes (a timeout or a failed chunk write). */
    public long late() {
        return late.get();
    }

    /**
     * Before the journal's file is written: saves the noted chunks that are still unsaved and queues, on the
     * SavedData IO chain, a wait for their levels' chunk writes. The journal's write is queued next, after it.
     */
    void beforeJournalWrite() {
        if (chunks.isEmpty())
            return;
        Set<ServerLevel> levels = new LinkedHashSet<>();
        for (LevelChunk c : chunks) {
            if (!(c.getLevel() instanceof ServerLevel level) || level.noSave())
                continue; // a level that is not saving writes no chunk; nothing to wait for
            if (c.isUnsaved()) {
                ((ChunkMapInvoker) level.getChunkSource().chunkMap).mic_rtsim$save(c);
                forcedSaves++;
            }
            levels.add(level);
        }
        chunks.clear();
        List<CompletableFuture<Void>> writes = new ArrayList<>(levels.size());
        for (ServerLevel level : levels) {
            ChunkMap map = level.getChunkSource().chunkMap;
            ChunkScanAccess scanner = map.chunkScanner();
            if (scanner instanceof IOWorker worker) {
                writes.add(worker.synchronize(true));
            } else if (!warnedScanner) {
                warnedScanner = true;
                MicRtsim.LOGGER.warn("[mic_rtsim] {}'s chunk storage is not vanilla's IOWorker ({}): the journal cannot wait for its chunk writes",
                        level.dimension().location(), scanner.getClass().getName());
            }
        }
        if (writes.isEmpty())
            return;
        waits++;
        CompletableFuture<Void> all = CompletableFuture.allOf(writes.toArray(CompletableFuture[]::new));
        // Never throws: an exception in a task of NeoForge's chain would skip every SavedData write after it.
        IOUtilities.withIOWorker(() -> {
            try {
                all.get(WAIT_SECONDS, TimeUnit.SECONDS);
            } catch (TimeoutException e) {
                late.incrementAndGet();
                MicRtsim.LOGGER.error("[mic_rtsim] the chunk writes the journal depends on took over {} s; the journal is written without them", WAIT_SECONDS);
            } catch (ExecutionException e) {
                late.incrementAndGet();
                MicRtsim.LOGGER.error("[mic_rtsim] a chunk write the journal depends on failed; the journal is written anyway", e.getCause());
            } catch (InterruptedException e) {
                late.incrementAndGet();
                Thread.currentThread().interrupt();
            } catch (RuntimeException e) {
                late.incrementAndGet();
                MicRtsim.LOGGER.error("[mic_rtsim] waiting for the chunk writes the journal depends on", e);
            }
        });
    }
}
