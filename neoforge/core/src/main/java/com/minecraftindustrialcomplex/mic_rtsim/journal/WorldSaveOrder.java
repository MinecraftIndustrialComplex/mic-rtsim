package com.minecraftindustrialcomplex.mic_rtsim.journal;

import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.dimension.DimensionType;
import net.minecraft.world.level.storage.LevelResource;
import net.neoforged.neoforge.common.IOUtilities;

import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

/**
 * The journal's file is the last SavedData a world save writes (slice U; docs/choices-journal-barrier.md RP-200).
 *
 * <p>Java deduplicates a command by the journal's applied set, and the command's effect can live in another
 * SavedData file: Diesel's {@code cdg_oil_chunks} (one per dimension), Create's {@code create_tracks}. Vanilla queues
 * each level's SavedData on NeoForge's IO chain level by level, the overworld first, and within a level in its
 * HashMap's order, so the journal could reach disk before them. A crash between the two writes left a journal that
 * knew the op and a store from before it: Java never applied the op again, and the goods it had moved existed twice.
 *
 * <p>So during {@code MinecraftServer.saveAllChunks} (every world save: autosave, {@code /save-all}, the stop) the
 * journal's write task, and the WORLD_SAVED marker behind it, are held here ({@link #queue}) and go on the chain when
 * it returns ({@link #end}), behind every level's SavedData. The chain runs its tasks one at a time, so a journal on
 * disk implies every file of the same save is on disk too (a process crash); the journal's task syncs the levels'
 * {@code data/} directories before its write, so a power loss cannot keep the journal's rename and lose theirs
 * (RP-201). The chunks the journal speaks for are not SavedData: {@link ChunkWriteBarrier} orders them.
 *
 * <p>A flush save (the stop, {@code /save-all flush}) waits for the chain at its return. A task that asked for it
 * ({@link #afterFlush}: a journal whose write was held, RP-205) then runs once more, still inside the save, with every
 * chunk flushed, and the chain is waited for again. A second hold keeps the older file: a flush save never waits
 * longer than two of the journal's waits.
 *
 * <p>Server thread only. The journal's file is written only inside saveAllChunks (RP-203: a level saved by itself
 * leaves it dirty); the marker of such a save is queued at once, and reports the file it finds on disk. If
 * saveAllChunks throws, the held tasks go on the chain at the next one's return: later, the safe side.
 */
public final class WorldSaveOrder {
    private WorldSaveOrder() {}

    private static boolean saving;
    private static boolean flushing;
    private static boolean retrying;
    private static final List<Runnable> held = new ArrayList<>();
    private static final List<Runnable> afterFlush = new ArrayList<>();
    private static List<Path> dataDirs = List.of();
    /** GameTests: run once on the server thread, between a flush save's first wait and its second try. */
    public static Runnable beforeFlushRetryForTest;

    /** saveAllChunks starts (its HEAD, MinecraftServerSaveMixin). */
    public static void begin(MinecraftServer server) {
        begin(server, false);
    }

    /** saveAllChunks starts; {@code flush}: its flush argument (the stop, {@code /save-all flush}). */
    public static void begin(MinecraftServer server, boolean flush) {
        saving = true;
        flushing = flush;
        retrying = false;
        afterFlush.clear();
        Path root = server.getWorldPath(LevelResource.ROOT);
        List<Path> dirs = new ArrayList<>();
        for (ServerLevel level : server.getAllLevels())
            dirs.add(DimensionType.getStorageFolder(level.dimension(), root).resolve("data"));
        dataDirs = List.copyOf(dirs);
    }

    /** Queues {@code task} on NeoForge's SavedData IO chain now, or when the world save under way returns. */
    public static void queue(Runnable task) {
        if (saving)
            held.add(task);
        else
            IOUtilities.withIOWorker(task);
    }

    /**
     * The {@code data/} directories the journal's write syncs first: every level's during a world save, else the
     * journal's own.
     */
    public static List<Path> dataDirs(Path journalDir) {
        return saving ? dataDirs : List.of(journalDir);
    }

    /**
     * saveAllChunks returned (every level's SavedData queued, {@code level.dat} written): the held tasks go on the
     * chain in the order they came. A flush save (the stop, {@code /save-all flush}) waits for them, as it waits for
     * each level's; then the tasks that asked for it ({@link #afterFlush}) run once more and are waited for again.
     */
    public static void end(boolean flush) {
        try {
            drain(flush);
            if (flush && !afterFlush.isEmpty()) {
                List<Runnable> again = new ArrayList<>(afterFlush);
                afterFlush.clear();
                Runnable hook = beforeFlushRetryForTest;
                beforeFlushRetryForTest = null;
                if (hook != null)
                    hook.run();
                retrying = true; // still saving: a task queued now is held, and goes on the chain below
                for (Runnable r : again)
                    r.run();
                drain(true);
            }
        } finally {
            saving = false;
            flushing = false;
            retrying = false;
            afterFlush.clear();
        }
    }

    private static void drain(boolean flush) {
        if (held.isEmpty())
            return;
        List<Runnable> tasks = new ArrayList<>(held);
        held.clear();
        for (Runnable t : tasks)
            IOUtilities.withIOWorker(t);
        if (flush)
            IOUtilities.waitUntilIOWorkerComplete();
    }

    /**
     * In a flush save, before its return: runs {@code retry} on the server thread once the chain has written this
     * save's tasks (RP-205). Ignored outside a flush save and during the second try.
     */
    public static void afterFlush(Runnable retry) {
        if (saving && flushing && !retrying)
            afterFlush.add(retry);
    }

    /**
     * GameTests: runs {@code save} (a test journal's own {@code save(File, ...)}) as saveAllChunks runs its levels'
     * saves, between its start and its return, without saving anything else.
     */
    public static void asWorldSaveForTest(MinecraftServer server, Runnable save) {
        asWorldSaveForTest(server, save, false);
    }

    /** As {@link #asWorldSaveForTest(MinecraftServer, Runnable)}, as a flush save when {@code flush}. */
    public static void asWorldSaveForTest(MinecraftServer server, Runnable save, boolean flush) {
        begin(server, flush);
        try {
            save.run();
        } finally {
            end(flush);
        }
    }

    /** Whether a world save is under way on the server thread. */
    public static boolean saving() {
        return saving;
    }

    /** Whether this is a flush save's second try (RP-205). */
    public static boolean retrying() {
        return saving && retrying;
    }
}
