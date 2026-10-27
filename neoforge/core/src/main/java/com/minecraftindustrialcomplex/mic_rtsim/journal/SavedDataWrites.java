package com.minecraftindustrialcomplex.mic_rtsim.journal;

import java.lang.ref.WeakReference;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.Consumer;

/**
 * The SavedData writes that failed, for the files the journal's file counts on (docs/choices-journal-barrier.md U-g1,
 * RP-317 of docs/choices-mowzie-build.md).
 *
 * <p>Vanilla's {@code SavedData.save(File, ...)} clears the dirty flag, queues the write on NeoForge's IO chain, and
 * there logs an {@code IOException} and goes on: a failed write loses nothing it can see. RP-200 made the journal's
 * file the last write of a save so that a journal on disk implies every file of its save is on disk, which holds only
 * when every write succeeds. A full disk fails every one, and the journal's own write could still succeed (it is
 * written by hand, and later); a crash then restarts from a journal that knows ops whose effect never reached its file,
 * and the engine never sends them again: direction A (docs/replay-gate.md 9.6).
 *
 * <p>So a failed write of a {@link JournalDependent} file is recorded here, by file ({@link #failed}, from a wrap on
 * the write in {@code SavedDataWriteMixin}, on the IO chain, which then rethrows: vanilla logs it as before), and
 * the journal's write task, last on the same chain, finds it ({@link #firstFailing}) and holds its write: the file on
 * disk stays at its last good state, as for a chunk it waits for (RP-205). Vanilla cleared the data's dirty flag, so
 * the next world save would not write it: {@link #remarkAll}, at that save's start on the server thread, marks every
 * failed data dirty again (it cannot be done from the IO thread, which races the server's changes to it). A write of
 * the file that succeeds ({@link #written}) ends the streak, and the journal's next write goes ahead.
 *
 * <p>Never blocks the server thread (a lock held for a few map operations), never grows (one entry per failing file,
 * at most one per journal-dependent file in the world), and logs once per streak: the first failure of a file says so
 * and the write that ends the streak says so; the later failures and each held write are counted. An entry holds
 * its data weakly: data that was dropped (its level unloaded) cannot be written again, so its entry goes at the next
 * {@link #remarkAll}, logged, and the journal goes on without it.
 */
public final class SavedDataWrites {
    /** The server's. */
    public static final SavedDataWrites INSTANCE = new SavedDataWrites(
            m -> com.minecraftindustrialcomplex.mic_rtsim.MicRtsim.LOGGER.error(m),
            m -> com.minecraftindustrialcomplex.mic_rtsim.MicRtsim.LOGGER.info(m));

    /** Consecutive held world saves after which a file stops holding the journal's write (PLACEHOLDER; docs/choices-journal-barrier.md U-g12). */
    public static final int GIVE_UP_AFTER = 12;

    private static final class Entry {
        final Path file;
        WeakReference<JournalDependent> owner;
        long failed;
        String why;

        Entry(Path file) {
            this.file = file;
        }
    }

    private final Consumer<String> error;
    private final Consumer<String> info;
    private final Object lock = new Object();
    private final Map<Path, Entry> failing = new LinkedHashMap<>();
    /** {@code failing.size()}, for the fast path of a write that succeeds. */
    private volatile int count;
    private final AtomicLong failures = new AtomicLong();
    private final AtomicLong recovered = new AtomicLong();
    private final AtomicLong remarked = new AtomicLong();
    private final AtomicLong abandoned = new AtomicLong();
    /** The thread that saw the last failure: NeoForge's IO worker, where the write runs, never the server thread (-1: none yet). */
    private volatile long failedOnThread = -1;
    /** The thread that last marked a failed data dirty again: the server thread, at a world save's start (-1: none yet). */
    private volatile long remarkedOnThread = -1;

    /** {@code error} and {@code info} are where the log lines go (the unit test puts its own; the mod's logger needs the game). */
    public SavedDataWrites(Consumer<String> error, Consumer<String> info) {
        this.error = error;
        this.info = info;
    }

    /** A log line that cannot throw: a failed write's rethrow, and the IO chain behind it, must not depend on a logger. */
    private static void say(Consumer<String> sink, String line) {
        try {
            sink.accept(line);
        } catch (RuntimeException ignored) {
            // nothing to do with a logger that fails
        }
    }

    private static Path key(Path file) {
        return file.toAbsolutePath().normalize();
    }

    /**
     * A write of {@code file} by {@code owner} threw {@code why}: on the IO chain, from the wrap on the write. The first
     * failure of a streak is logged; the rest are counted. Never throws.
     */
    public void failed(JournalDependent owner, Path file, Throwable why) {
        try {
            failedOnThread = Thread.currentThread().threadId();
            Path k = key(file);
            boolean first;
            long n;
            synchronized (lock) {
                Entry e = failing.get(k);
                first = e == null;
                if (first) {
                    e = new Entry(k);
                    failing.put(k, e);
                    count = failing.size();
                }
                e.owner = new WeakReference<>(owner);
                n = ++e.failed;
                e.why = String.valueOf(why);
            }
            failures.incrementAndGet();
            if (first)
                say(error, "[mic_rtsim] the write of " + k + " failed (" + why + "): the journal's file is kept as it was until that file is"
                        + " written again, and the next world save tries it again. A stop or crash before then restarts from the journal's"
                        + " last good file, and the ops decided since replay from it; further failures of this file are counted, not"
                        + " logged (docs/choices-journal-barrier.md U-g1)");
        } catch (RuntimeException e) {
            say(error, "[mic_rtsim] could not record the failed write of " + file + ": " + e);
        }
    }

    /** A write of {@code file} succeeded: on the IO chain. Ends its streak, logged once. Never throws. */
    public void written(Path file) {
        if (count == 0)
            return;
        try {
            Entry e;
            synchronized (lock) {
                e = failing.remove(key(file));
                count = failing.size();
            }
            if (e != null) {
                recovered.incrementAndGet();
                say(info, "[mic_rtsim] " + e.file + " was written again after " + e.failed + " failed write(s): the journal's file goes on");
            }
        } catch (RuntimeException e) {
            say(error, "[mic_rtsim] could not record the write of " + file + ": " + e);
        }
    }

    /**
     * A world save starts (server thread): every failed file's data is marked dirty again, so this save writes it.
     * An entry whose data is gone is dropped, logged.
     */
    public void remarkAll() {
        if (count == 0)
            return;
        List<JournalDependent> owners = new ArrayList<>();
        List<Entry> gone = new ArrayList<>();
        synchronized (lock) {
            for (Iterator<Entry> it = failing.values().iterator(); it.hasNext(); ) {
                Entry e = it.next();
                JournalDependent o = e.owner == null ? null : e.owner.get();
                if (o == null) {
                    gone.add(e);
                    it.remove();
                } else {
                    owners.add(o);
                }
            }
            count = failing.size();
        }
        for (JournalDependent o : owners) {
            o.mic_rtsim$remark();
            remarked.incrementAndGet();
            remarkedOnThread = Thread.currentThread().threadId();
        }
        for (Entry e : gone) {
            abandoned.incrementAndGet();
            say(error, "[mic_rtsim] the data of " + e.file + " is gone (its level unloaded?) after " + e.failed + " failed write(s) and cannot be"
                    + " written again: the journal's file goes on without it; what it held that the journal's file counts on is lost"
                    + " (docs/choices-journal-barrier.md U-g1)");
        }
    }

    /**
     * Why the journal's write must be held, or null: the first file under one of {@code dirs} (the directories of the save
     * the journal's file belongs to) whose last write failed and has not succeeded since. On the IO chain.
     */
    public String firstFailing(Collection<Path> dirs) {
        if (count == 0)
            return null;
        List<Path> roots = new ArrayList<>(dirs.size());
        for (Path d : dirs)
            roots.add(key(d));
        synchronized (lock) {
            for (Entry e : failing.values())
                for (Path root : roots)
                    if (e.file.startsWith(root))
                        return e.file + " (" + e.failed + " failed write(s), last " + e.why + ")";
        }
        return null;
    }

    /** (stub: the next commit) The journal's question at its write: as {@link #firstFailing}, and counts the hold against each file. */
    public String holdFor(Collection<Path> dirs) {
        return firstFailing(dirs);
    }

    /** (stub: the next commit) Whether {@code file}'s last write failed and it still holds the journal's write. */
    public boolean holds(Path file) {
        return false;
    }

    /** (stub: the next commit) Files the journal stopped holding for after {@link #GIVE_UP_AFTER} held world saves. */
    public long gaveUp() {
        return 0;
    }

    /** (stub: the next commit) Forgets every entry and counter: a server stopped. */
    public void reset() {
    }

    /** (stub: the next commit) GameTests: the held world saves after which a file stops holding; {@code <= 0}: back to {@link #GIVE_UP_AFTER}. */
    public void giveUpAfterForTest(int worldSaves) {
    }

    /** Files whose last write failed. */
    public int failing() {
        return count;
    }

    /** Writes that failed since the server started. */
    public long failures() {
        return failures.get();
    }

    /** Streaks ended by a write that succeeded. */
    public long recovered() {
        return recovered.get();
    }

    /** Files marked dirty again at a world save's start. */
    public long remarked() {
        return remarked.get();
    }

    /** Entries dropped because their data was gone. */
    public long abandoned() {
        return abandoned.get();
    }

    /** The id of the thread that saw the last failure (the IO worker's), or -1. GameTests assert it is not the server thread's. */
    public long lastFailureThreadId() {
        return failedOnThread;
    }

    /** The id of the thread that last marked a failed data dirty again (the server thread's), or -1. */
    public long lastRemarkThreadId() {
        return remarkedOnThread;
    }

    /** One line for {@code /rtsim perf}. */
    public String describe() {
        return "SavedData writes failed " + failures() + ", failing now " + failing() + ", recovered " + recovered()
                + ", marked dirty again " + remarked() + ", abandoned " + abandoned();
    }

    /** GameTests: forgets every failing file under {@code dir}, as after the data is deleted. */
    public void forgetForTest(Path dir) {
        Path root = key(dir);
        synchronized (lock) {
            failing.values().removeIf(e -> e.file.startsWith(root));
            count = failing.size();
        }
    }
}
