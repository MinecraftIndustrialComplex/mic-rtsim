package com.minecraftindustrialcomplex.mic_rtsim.journal;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.mixin.ChunkMapInvoker;
import com.minecraftindustrialcomplex.mic_rtsim.mixin.LevelChunkAccessor;
import com.minecraftindustrialcomplex.mic_rtsim.mixin.RegionFileStorageAccessor;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.storage.ChunkScanAccess;
import net.minecraft.world.level.chunk.storage.IOWorker;
import net.minecraft.world.level.chunk.storage.RegionFileStorage;
import org.jetbrains.annotations.Nullable;

import java.io.IOException;
import java.nio.channels.FileChannel;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;
import java.util.concurrent.atomic.AtomicLong;

/**
 * The journal never reaches disk ahead of a chunk it speaks for (I8; docs/choices-repair.md R21,
 * docs/choices-journal-barrier.md JB1 to JB5).
 *
 * <p>A journaled record can claim something that lives in a chunk: a reconcile applied into it
 * (IND_RECONCILE_RESULT, its epoch in {@code mic_rtsim:reconcile}), a transfer, package, house or resident
 * whose COMMAND_RESULT the chunk carries ({@code mic_rtsim:pending}). Vanilla writes the journal (a SavedData,
 * in {@code saveLevelData}) before it serializes any chunk, both asynchronously, and skips a chunk saved in the
 * last 10 s. A crash between the two writes left a journal that says applied and a chunk on disk from before:
 * the engine took the result and never sent the command again, so the goods were lost.
 *
 * <p>So each such chunk is noted here ({@link #dependsOn}) when the record is committed. Each write of the
 * journal's file is a round ({@link #arm}, on the server thread, where {@link JournalSavedData} takes the
 * snapshot): every noted chunk still in its level and unsaved goes through vanilla's own {@code ChunkMap.save},
 * then each level's chunk storage is asked to put those chunks on disk now, ahead of whatever it has queued
 * (JB2). The journal's write, a task on NeoForge's SavedData IO chain, first waits for them, at most
 * {@link #WAIT_MILLIS} ({@link #await}). If they are all on disk, the file is written. If not, it is not: the
 * file stays as it was (older than the chunks, the safe order: the chunks carry their records until the engine
 * acknowledges them), and the next world save writes it, with every note still unconfirmed (JB1). Only after
 * {@link #GIVE_UP_ROUNDS} rounds in a row held does a write go ahead without its chunks, logged and counted
 * ({@link #late}).
 *
 * <p>The server thread never waits: it serializes each noted unsaved chunk once (that autosave would have done
 * most of them) and queues one small task per level. The wait is on the IO chain.
 *
 * <p>Notes, the round counter and the failed-write list are the server thread's; {@link #await} runs on the
 * IO chain (one task at a time); the two meet at {@link #confirmedRound} and the queue of failed writes.
 */
public final class ChunkWriteBarrier {

    /**
     * How long one journal write waits on the IO chain for its chunks (PLACEHOLDER, docs/choices-journal-barrier.md
     * JB4). With the targeted writes it is the chunk worker's foreground queue plus the noted chunks' own writes:
     * milliseconds. Past it the round is held, not written late.
     */
    public static final long WAIT_MILLIS = 10_000;

    /** World saves in a row whose journal write is held before one goes ahead without its chunks (PLACEHOLDER, JB4). */
    public static final int GIVE_UP_ROUNDS = 3;

    /** How often the C2ME strategy looks at its storage thread's cache, in ms. */
    static final long POLL_MILLIS = 20;

    /** Vanilla's {@code IOWorker}, through {@code IOWorkerMixin} (PATCHES.md row 40). */
    public interface TargetedWrites {
        /**
         * A foreground task on the worker's mailbox that writes the pending entries of {@code positions} now.
         * Completes once they are written, with the positions whose write failed.
         */
        CompletableFuture<List<ChunkPos>> mic_rtsim$writeNow(List<ChunkPos> positions);

        RegionFileStorage mic_rtsim$storage();
    }

    /**
     * One level's chunk storage, as far as the barrier needs it: a way to know that a position's latest store
     * is on disk.
     */
    interface LevelWrites {
        /**
         * Called on the server thread at the arm, after every store of {@code positions}: completes once each
         * position's latest store is written, with the positions whose write failed.
         */
        CompletableFuture<List<ChunkPos>> landed(List<ChunkPos> positions);

        /** The region files to fsync once written; null when the writes are synchronous already (DSYNC) or unknown. */
        @Nullable
        Path regionFolder();

        /** For the log and the GameTests. */
        String kind();
    }

    private static final class Note {
        long round;
        boolean unloadSeen;
        /** Its write failed after it left its level: what it held is not on disk, and cannot be saved again. */
        boolean lost;

        Note(long round) {
            this.round = round;
        }
    }

    /** One journal write's chunks, armed on the server thread and waited for on the IO chain. */
    public static final class Round {
        final long number;
        /** Not null: the round cannot be confirmed (decided at the arm). */
        @Nullable
        final String held;
        final CompletableFuture<List<ChunkPos>> landed;
        final List<CompletableFuture<?>> cancel;
        final Set<Path> regionFiles;
        final int chunks;

        Round(long number, @Nullable String held, CompletableFuture<List<ChunkPos>> landed,
              List<CompletableFuture<?>> cancel, Set<Path> regionFiles, int chunks) {
            this.number = number;
            this.held = held;
            this.landed = landed;
            this.cancel = cancel;
            this.regionFiles = regionFiles;
            this.chunks = chunks;
        }
    }

    private record Failed(ServerLevel level, ChunkPos pos) {
    }

    private final Map<LevelChunk, Note> notes = new IdentityHashMap<>();
    private final Map<Object, LevelWrites> writers = new IdentityHashMap<>();
    private final ConcurrentLinkedQueue<Failed> failed = new ConcurrentLinkedQueue<>();
    private final ConcurrentLinkedQueue<LevelChunk> offThreadUnloads = new ConcurrentLinkedQueue<>();
    private long nextRound = 1;
    /** The last round whose notes are written (or given up on); set on the IO chain, read at the arm. */
    private volatile long confirmedRound;
    /** Rounds held in a row (IO chain only). */
    private int heldInARow;
    private boolean warnedScanner;

    // counters (the GameTests read them); the server thread's
    private long noted;
    private long forcedSaves;
    private long rounds;
    // the IO chain's
    private final AtomicLong held = new AtomicLong();
    private final AtomicLong late = new AtomicLong();
    private final AtomicLong failedWrites = new AtomicLong();
    private final AtomicLong lastWaitMillis = new AtomicLong(-1);
    private final AtomicLong maxWaitMillis = new AtomicLong(-1);
    private volatile String lastHeld = "";

    /**
     * A record just committed to the journal speaks for {@code chunk}'s contents: the journal's next file waits
     * for the chunk's next write. Server thread.
     */
    public void dependsOn(LevelChunk chunk) {
        Note n = notes.get(chunk);
        if (n == null) {
            notes.put(chunk, new Note(nextRound));
            noted++;
        } else {
            n.round = nextRound; // a newer state of the chunk: an earlier round's write does not cover it
        }
    }

    /**
     * {@code ChunkEvent.Unload} of {@code chunk} (server thread): its last write has been handed to the chunk
     * storage (vanilla saves it in the same task, after the event; C2ME's async unload posts the event after the
     * write is queued). Until then a noted chunk that is no longer loaded is in flight.
     */
    public void onUnload(LevelChunk chunk) {
        if (!(chunk.getLevel() instanceof ServerLevel level) || !level.getServer().isSameThread()) {
            offThreadUnloads.add(chunk); // not seen in vanilla or C2ME; the notes are the server thread's
            return;
        }
        Note n = notes.get(chunk);
        if (n != null)
            n.unloadSeen = true;
    }

    /** Chunks noted and not yet confirmed written. */
    public int pending() {
        return notes.size();
    }

    public long noted() {
        return noted;
    }

    public long forcedSaves() {
        return forcedSaves;
    }

    /** Journal writes that had chunks to wait for. */
    public long rounds() {
        return rounds;
    }

    /** Journal writes held to a later world save. */
    public long held() {
        return held.get();
    }

    /** Why the last held round was held, "" before the first. */
    public String lastHeld() {
        return lastHeld;
    }

    /** Journal writes that went ahead without their chunks (after {@link #GIVE_UP_ROUNDS} held rounds). */
    public long late() {
        return late.get();
    }

    /** Chunk writes the targeted path saw fail. */
    public long failedWrites() {
        return failedWrites.get();
    }

    /** The last round's wait for its chunks, in ms; -1 before the first. */
    public long lastWaitMillis() {
        return lastWaitMillis.get();
    }

    public long maxWaitMillis() {
        return maxWaitMillis.get();
    }

    /** The strategy the barrier uses for {@code level}'s chunk storage (for the GameTests and the log). */
    public String strategy(ServerLevel level) {
        LevelWrites w = writesFor(level);
        return w == null ? "none" : w.kind();
    }

    /**
     * The journal's snapshot is being taken (server thread): sends the noted chunks on their way to disk. Null
     * when no chunk is noted: the journal is written at once.
     */
    @Nullable
    Round arm() {
        for (LevelChunk c; (c = offThreadUnloads.poll()) != null; ) {
            Note n = notes.get(c);
            if (n != null)
                n.unloadSeen = true;
        }
        long confirmed = confirmedRound;
        notes.values().removeIf(n -> n.round <= confirmed);
        for (Failed f; (f = failed.poll()) != null; ) {
            // a write that failed: the chunk is saved again (vanilla cleared its unsaved flag before writing); one
            // that has left its level cannot be, and holds every round until the valve
            for (Map.Entry<LevelChunk, Note> e : notes.entrySet()) {
                LevelChunk c = e.getKey();
                if (c.getLevel() != f.level() || !c.getPos().equals(f.pos()))
                    continue;
                if (((LevelChunkAccessor) c).mic_rtsim$loaded())
                    c.setUnsaved(true);
                else
                    e.getValue().lost = true;
            }
        }
        if (notes.isEmpty())
            return null;
        long number = nextRound++;
        rounds++;
        String hold = null;
        Map<ServerLevel, List<ChunkPos>> byLevel = new LinkedHashMap<>();
        for (Iterator<Map.Entry<LevelChunk, Note>> it = notes.entrySet().iterator(); it.hasNext(); ) {
            Map.Entry<LevelChunk, Note> e = it.next();
            LevelChunk c = e.getKey();
            if (!(c.getLevel() instanceof ServerLevel level) || level.noSave()) {
                it.remove(); // a level that is not saving writes no chunk; nothing to wait for
                continue;
            }
            boolean loaded = ((LevelChunkAccessor) c).mic_rtsim$loaded();
            if (e.getValue().lost && !loaded) {
                hold = "chunk " + c.getPos() + " in " + level.dimension().location() + " was unloaded and its write failed";
                continue;
            }
            if (loaded && c.isUnsaved()) {
                boolean saved;
                try {
                    saved = ((ChunkMapInvoker) level.getChunkSource().chunkMap).mic_rtsim$save(c);
                } catch (RuntimeException ex) {
                    MicRtsim.LOGGER.error("[mic_rtsim] saving chunk {} before the journal", c.getPos(), ex);
                    saved = false;
                }
                if (!saved) {
                    // vanilla logged why and cleared the flag; the next world save tries again
                    c.setUnsaved(true);
                    hold = "chunk " + c.getPos() + " in " + level.dimension().location() + " could not be serialized";
                    continue;
                }
                forcedSaves++;
            } else if (!loaded && !e.getValue().unloadSeen) {
                // Not loaded and its unload not seen: C2ME's async unload is serializing it, and nothing has been
                // handed to the storage yet (or a chunk that was never in its level).
                hold = "chunk " + c.getPos() + " in " + level.dimension().location() + " is being unloaded";
                continue;
            }
            // Loaded and saved, or unloaded: its latest write was handed to the storage before now (a store is
            // queued on the server thread, in ChunkMap.save).
            byLevel.computeIfAbsent(level, k -> new ArrayList<>()).add(c.getPos());
        }
        if (hold != null)
            return new Round(number, hold, CompletableFuture.completedFuture(List.of()), List.of(), Set.of(), notes.size());
        CompletableFuture<List<ChunkPos>> all = CompletableFuture.completedFuture(List.of());
        List<CompletableFuture<?>> parts = new ArrayList<>(byLevel.size());
        Set<Path> regionFiles = new LinkedHashSet<>();
        int count = 0;
        for (Map.Entry<ServerLevel, List<ChunkPos>> e : byLevel.entrySet()) {
            ServerLevel level = e.getKey();
            List<ChunkPos> positions = e.getValue();
            count += positions.size();
            LevelWrites w = writesFor(level);
            if (w == null)
                continue; // warned once: this level's writes cannot be ordered
            CompletableFuture<List<ChunkPos>> f;
            try {
                f = w.landed(positions);
            } catch (RuntimeException ex) {
                MicRtsim.LOGGER.error("[mic_rtsim] asking {} to write the chunks the journal speaks for", level.dimension().location(), ex);
                f = CompletableFuture.failedFuture(ex);
            }
            f = f.thenApply(bad -> {
                for (ChunkPos p : bad)
                    failed.add(new Failed(level, p));
                return bad;
            });
            parts.add(f);
            all = all.thenCombine(f, ChunkWriteBarrier::merge);
            Path folder = w.regionFolder();
            if (folder != null)
                for (ChunkPos p : positions)
                    regionFiles.add(folder.resolve("r." + p.getRegionX() + "." + p.getRegionZ() + ".mca"));
        }
        return new Round(number, null, all, parts, regionFiles, count);
    }

    /**
     * On the IO chain, right before the journal's write: waits for the round's chunks. True to write the file,
     * false to hold it to the next world save. Never throws (an exception in a task of NeoForge's chain would
     * skip every SavedData write after it), and never leaves the thread interrupted.
     */
    boolean await(@Nullable Round round) {
        if (round == null) {
            heldInARow = 0;
            return true;
        }
        long start = System.nanoTime();
        String why = round.held;
        if (why == null) {
            try {
                List<ChunkPos> bad = round.landed.get(WAIT_MILLIS, TimeUnit.MILLISECONDS);
                if (!bad.isEmpty()) {
                    failedWrites.addAndGet(bad.size());
                    why = bad.size() + " chunk write(s) failed, " + bad.get(0) + " first";
                } else {
                    why = fsync(round.regionFiles);
                }
            } catch (TimeoutException e) {
                why = round.chunks + " chunk(s) not on disk after " + WAIT_MILLIS + " ms";
            } catch (ExecutionException e) {
                why = "a chunk write failed: " + e.getCause();
            } catch (InterruptedException e) {
                // Only a shutting-down IO pool interrupts. The flag is not set again: the journal's write may run
                // on this thread next, and an interrupted thread fails every FileChannel call.
                why = "interrupted";
            } catch (RuntimeException e) {
                why = e.toString();
            }
            for (CompletableFuture<?> f : round.cancel)
                f.cancel(false); // stops a C2ME poll; a vanilla write still runs, early
            long ms = (System.nanoTime() - start) / 1_000_000;
            lastWaitMillis.set(ms);
            maxWaitMillis.accumulateAndGet(ms, Math::max);
        }
        if (why == null) {
            heldInARow = 0;
            confirmedRound = round.number;
            return true;
        }
        held.incrementAndGet();
        lastHeld = why;
        if (++heldInARow >= GIVE_UP_ROUNDS) {
            heldInARow = 0;
            late.incrementAndGet();
            confirmedRound = round.number; // given up on: these notes go
            MicRtsim.LOGGER.error("[mic_rtsim] the journal's file has waited {} world saves for the chunks it speaks for ({}); it is written without them",
                    GIVE_UP_ROUNDS, why);
            return true;
        }
        MicRtsim.LOGGER.warn("[mic_rtsim] the journal's file is kept as it was until the next world save: {}", why);
        return false;
    }

    static List<ChunkPos> merge(List<ChunkPos> a, List<ChunkPos> b) {
        if (b.isEmpty())
            return a;
        if (a.isEmpty())
            return b;
        List<ChunkPos> l = new ArrayList<>(a);
        l.addAll(b);
        return l;
    }

    /** Null when every file is forced to disk, else why not. */
    @Nullable
    private static String fsync(Set<Path> files) {
        for (Path p : files) {
            try (FileChannel ch = FileChannel.open(p, StandardOpenOption.WRITE)) {
                ch.force(true);
            } catch (IOException e) {
                return "forcing " + p + " to disk: " + e;
            }
        }
        return null;
    }

    @Nullable
    private LevelWrites writesFor(ServerLevel level) {
        ChunkScanAccess scanner = level.getChunkSource().chunkMap.chunkScanner();
        LevelWrites w = writers.get(scanner);
        if (w == null && !writers.containsKey(scanner)) {
            w = strategyFor(level, scanner);
            writers.put(scanner, w);
        }
        return w;
    }

    @Nullable
    private LevelWrites strategyFor(ServerLevel level, ChunkScanAccess scanner) {
        if (scanner.getClass() == IOWorker.class && scanner instanceof TargetedWrites t)
            return new VanillaWrites(t);
        if (scanner instanceof IOWorker worker) {
            LevelWrites c2me = C2meWrites.of(worker);
            if (c2me != null)
                return c2me;
            MicRtsim.LOGGER.warn("[mic_rtsim] {}'s chunk storage is {}, not vanilla's IOWorker: the journal waits for all of its queued writes",
                    level.dimension().location(), scanner.getClass().getName());
            return new SynchronizeWrites(worker);
        }
        if (!warnedScanner) {
            warnedScanner = true;
            MicRtsim.LOGGER.warn("[mic_rtsim] {}'s chunk storage is not an IOWorker ({}): the journal cannot wait for its chunk writes",
                    level.dimension().location(), scanner.getClass().getName());
        }
        return null;
    }

    /** The region folder to fsync, or null when the storage writes with DSYNC already. */
    @Nullable
    static Path folderIfNotSync(@Nullable RegionFileStorage storage) {
        if (storage == null)
            return null;
        RegionFileStorageAccessor a = (RegionFileStorageAccessor) (Object) storage;
        return a.mic_rtsim$sync() ? null : a.mic_rtsim$folder();
    }

    /**
     * Vanilla: the noted positions are written by foreground tasks, ahead of the background queue (JB2). One
     * position per task, the next queued when the last is done, so a chunk read the server thread waits for
     * (a synchronous load) gets in between, as it does between vanilla's own background writes.
     */
    private record VanillaWrites(TargetedWrites worker) implements LevelWrites {
        @Override
        public CompletableFuture<List<ChunkPos>> landed(List<ChunkPos> positions) {
            // the first task is queued now, on the server thread, after every store ChunkMap.save queued
            CompletableFuture<List<ChunkPos>> f = CompletableFuture.completedFuture(List.of());
            for (ChunkPos p : List.copyOf(positions))
                f = f.thenCompose(bad -> worker.mic_rtsim$writeNow(List.of(p)).thenApply(b -> merge(bad, b)));
            return f;
        }

        @Override
        public Path regionFolder() {
            return folderIfNotSync(worker.mic_rtsim$storage());
        }

        @Override
        public String kind() {
            return "vanilla";
        }
    }

    /** Another IOWorker subclass: everything queued, as the first barrier did. */
    private record SynchronizeWrites(IOWorker worker) implements LevelWrites {
        @Override
        public CompletableFuture<List<ChunkPos>> landed(List<ChunkPos> positions) {
            return worker.synchronize(true).thenApply(v -> List.of());
        }

        @Override
        public Path regionFolder() {
            return null; // synchronize(true) flushed
        }

        @Override
        public String kind() {
            return "synchronize";
        }
    }
}
