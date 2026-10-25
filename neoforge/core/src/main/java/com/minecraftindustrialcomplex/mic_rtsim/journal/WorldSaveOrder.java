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
 * <p>Server thread only. The journal's file is written only inside saveAllChunks (RP-203: a level saved by itself
 * leaves it dirty); the marker of such a save is queued at once, and reports the file it finds on disk. If
 * saveAllChunks throws, the held tasks go on the chain at the next one's return: later, the safe side.
 */
public final class WorldSaveOrder {
    private WorldSaveOrder() {}

    private static boolean saving;
    private static final List<Runnable> held = new ArrayList<>();
    private static List<Path> dataDirs = List.of();

    /** saveAllChunks starts (its HEAD, MinecraftServerSaveMixin). */
    public static void begin(MinecraftServer server) {
        saving = true;
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
     * each level's.
     */
    public static void end(boolean flush) {
        saving = false;
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
     * GameTests: runs {@code save} (a test journal's own {@code save(File, ...)}) as saveAllChunks runs its levels'
     * saves, between its start and its return, without saving anything else.
     */
    public static void asWorldSaveForTest(MinecraftServer server, Runnable save) {
        begin(server);
        try {
            save.run();
        } finally {
            end(false);
        }
    }

    /** Whether a world save is under way on the server thread. */
    public static boolean saving() {
        return saving;
    }
}
