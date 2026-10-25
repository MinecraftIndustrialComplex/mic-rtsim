package com.minecraftindustrialcomplex.mic_rtsim.journal;

import java.io.IOException;
import java.nio.channels.FileChannel;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.Consumer;

/**
 * Makes a directory's entries durable (fsync on the directory), so a power loss keeps the renames and new files in
 * it that were made before (docs/choices-journal-barrier.md RP-201). NeoForge's {@code IOUtilities.atomicWrite}
 * forces a SavedData file's bytes before it renames the file into place, but nothing syncs the rename; vanilla's
 * region files are synced (DSYNC on a dedicated server, the barrier's force otherwise) but a region file made in this
 * save is a new entry in its folder. On the IO chain only.
 *
 * <p>Java cannot open a directory on Windows: there the sync is skipped, and the first failure is logged once
 * (NTFS orders its metadata but does not promise a rename across a power loss: the residual RP-201 records).
 */
public final class DirectorySync {
    private DirectorySync() {}

    /** Where the one warning goes; the unit test puts its own here (the mod's logger needs the game's classes). */
    static volatile Consumer<String> warn = m -> com.minecraftindustrialcomplex.mic_rtsim.MicRtsim.LOGGER.warn(m);

    private static final AtomicLong synced = new AtomicLong(), failed = new AtomicLong(), logged = new AtomicLong();
    private static final AtomicLong nanos = new AtomicLong();

    /** Syncs {@code dir}; false when it could not (logged the first time only). Never throws. */
    public static boolean sync(Path dir) {
        long t0 = System.nanoTime();
        try (FileChannel ch = FileChannel.open(dir, StandardOpenOption.READ)) {
            ch.force(true);
            synced.incrementAndGet();
            return true;
        } catch (IOException | RuntimeException e) {
            failed.incrementAndGet();
            if (logged.compareAndSet(0, 1))
                warn.accept("[mic_rtsim] cannot sync directory " + dir + " (" + e + "): a power loss here can undo a rename the"
                        + " journal's file counts on; logged once, later failures are only counted");
            return false;
        } finally {
            nanos.addAndGet(System.nanoTime() - t0);
        }
    }

    /** Directories synced since the server started. */
    public static long synced() {
        return synced.get();
    }

    /** Syncs that failed (skipped). */
    public static long failed() {
        return failed.get();
    }

    /** Failures logged: at most one. */
    public static long logged() {
        return logged.get();
    }

    /** Time spent syncing directories, in nanoseconds (on the IO chain, never the server thread). */
    public static long nanos() {
        return nanos.get();
    }
}
