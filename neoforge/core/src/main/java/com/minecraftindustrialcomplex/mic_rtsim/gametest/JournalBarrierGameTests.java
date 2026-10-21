package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.journal.ChunkWriteBarrier;
import com.minecraftindustrialcomplex.mic_rtsim.journal.JournalSavedData;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.Tag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.Container;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.storage.IOWorker;
import net.minecraft.world.level.chunk.storage.RegionFileVersion;
import net.minecraft.world.level.dimension.DimensionType;
import net.minecraft.world.level.storage.LevelResource;
import net.neoforged.neoforge.common.IOUtilities;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
import java.util.List;
import java.util.UUID;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ThreadLocalRandom;
import java.util.concurrent.atomic.AtomicReference;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.io;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The journal's chunk barrier (docs/choices-journal-barrier.md JB1 to JB5, the tests JB8) on the real chunk storage: the
 * journal's file waits for the chunks it speaks for and for nothing else, and a write it cannot order is held
 * to the next world save, never made late.
 *
 * <p>Each test but the marker's drives its own {@link JournalSavedData} (not the server's), through the same
 * {@code save(File, ...)} vanilla calls from {@code saveLevelData}, so the server's autosaves never count in its
 * rounds. Its file goes under {@code <world>/mic_rtsim-test/}.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class JournalBarrierGameTests {
    private JournalBarrierGameTests() {}

    /** Chunk writes queued ahead of the noted chunk: a far region of the test level nothing else uses. */
    private static final int BACKLOG = 1000;
    private static final int BACKLOG_REGION = -900;
    private static final int FILLER_BYTES = 16 * 1024;

    private static Path testFile(ServerLevel level) throws IOException {
        Path dir = level.getServer().getWorldPath(LevelResource.ROOT).resolve("mic_rtsim-test");
        Files.createDirectories(dir);
        return dir.resolve("journal-barrier-" + UUID.randomUUID() + ".dat");
    }

    /** A chunk tag the region file takes as it is: an empty proto chunk with incompressible filler. */
    private static CompoundTag filler(ChunkPos pos) {
        CompoundTag t = new CompoundTag();
        t.putInt("DataVersion", SharedConstants.getCurrentVersion().getDataVersion().getVersion());
        t.putString("Status", "minecraft:empty");
        t.putInt("xPos", pos.x);
        t.putInt("zPos", pos.z);
        byte[] b = new byte[FILLER_BYTES];
        ThreadLocalRandom.current().nextBytes(b);
        t.putByteArray("mic_rtsim_test_filler", b);
        return t;
    }

    /** What the IO chain saw right after the test journal's write task. */
    private record Peek(int diamonds, int emeralds, boolean journalOnDisk, long journalNext, int backlogDone, String error) {}

    /**
     * The journal's write goes ahead of the chunk worker's queue, not ahead of its chunk (JB2). A chest in the
     * arena gets a random number of diamonds and emeralds (so a chunk left on disk by an earlier run cannot
     * pass), the chunk is noted, then {@value #BACKLOG} chunk writes of 16 KB each are queued on the level's
     * chunk worker, and the test journal is saved. Right after its write, on the IO chain: the file is there,
     * the chunk on disk has the chest's counts, and most of the backlog is still queued. The first barrier
     * ({@code IOWorker.synchronize}) waited for the whole backlog, and the backlog is what made its waits
     * tens of seconds on a loaded GameTest server: a vanilla worker writes one queued chunk per background turn,
     * oldest first, each with DSYNC.
     */
    @GameTest(template = RtsimTests.TEMPLATE, batch = "mic_rtsim_journal_barrier", timeoutTicks = 12000)
    public static void aJournalWriteGoesAheadOfTheChunkQueueButNotAheadOfItsChunk(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        BlockPos chestPos = helper.absolutePos(new BlockPos(5, 2, 5));
        int diamonds = ThreadLocalRandom.current().nextInt(2, 65), emeralds = ThreadLocalRandom.current().nextInt(2, 65);
        JournalSavedData data = JournalSavedData.fresh();
        List<CompletableFuture<Void>> backlog = new ArrayList<>();
        List<ChunkPos> backlogAt = new ArrayList<>();
        AtomicReference<Peek> peek = new AtomicReference<>();
        Path[] file = {null};
        long[] started = {0};
        boolean[] vanilla = {true};
        helper.startSequence()
                .thenExecute(() -> {
                    level.setBlock(chestPos, Blocks.CHEST.defaultBlockState(), 3);
                    Container chest = (Container) level.getBlockEntity(chestPos);
                    chest.setItem(0, new ItemStack(Items.DIAMOND, diamonds));
                    chest.setItem(1, new ItemStack(Items.EMERALD, emeralds));
                    LevelChunk chunk = level.getChunkAt(chestPos);
                    check("the arena chunk is unsaved", chunk.isUnsaved());
                    data.chunks().dependsOn(chunk);
                    data.setDirty();
                    IOWorker worker = (IOWorker) level.getChunkSource().chunkMap.chunkScanner();
                    String expected = expectedStrategy(worker);
                    check("the barrier uses the " + expected + " strategy for " + worker.getClass().getName() + ", not "
                            + data.chunks().strategy(level), expected.equals(data.chunks().strategy(level)));
                    vanilla[0] = "vanilla".equals(expected);
                    int base = BACKLOG_REGION * 32;
                    for (int i = 0; i < BACKLOG; i++) {
                        ChunkPos p = new ChunkPos(base + (i & 31), base + (i >> 5));
                        backlogAt.add(p);
                        backlog.add(worker.store(p, filler(p)));
                    }
                    try {
                        file[0] = testFile(level);
                    } catch (IOException e) {
                        throw new RuntimeException(e);
                    }
                    started[0] = System.nanoTime();
                    data.save(file[0].toFile(), level.registryAccess());
                    peekAfter(level, chestPos, file[0], backlog, peek);
                })
                .thenWaitUntil(io(() -> check("the IO chain ran the journal's write and the peek", peek.get() != null)))
                .thenExecute(() -> {
                    Peek p = peek.get();
                    ChunkWriteBarrier b = data.chunks();
                    record("aJournalWriteGoesAheadOfTheChunkQueueButNotAheadOfItsChunk", "peek",
                            "chest on disk " + p.diamonds() + "/" + diamonds + " diamonds, " + p.emeralds() + "/" + emeralds
                                    + " emeralds; journal on disk " + p.journalOnDisk() + " (next " + p.journalNext() + "); backlog written " + p.backlogDone() + " of " + BACKLOG
                                    + "; barrier wait " + b.lastWaitMillis() + " ms, " + b.forcedSaves() + " forced saves, held " + b.held()
                                    + " (" + b.lastHeld() + "), late " + b.late() + "; " + (System.nanoTime() - started[0]) / 1_000_000 + " ms since the save");
                    check("read: " + p.error(), p.error() == null);
                    check("the journal's file was written, not held (" + b.lastHeld() + ")", p.journalOnDisk() && data.writes() == 1 && b.held() == 0);
                    check("and never ahead of its chunk: the chest on disk has " + p.diamonds() + " diamonds and " + p.emeralds() + " emeralds",
                            p.diamonds() == diamonds && p.emeralds() == emeralds);
                    // C2ME writes each chunk as soon as it is compressed, in no queue order: nothing to go ahead of
                    if (vanilla[0])
                        check("it went ahead of the chunk worker's queue: " + p.backlogDone() + " of " + BACKLOG + " queued writes were done",
                                p.backlogDone() < BACKLOG);
                    check("nothing late", b.late() == 0);
                })
                .thenExecute(() -> {
                    level.setBlock(chestPos, Blocks.AIR.defaultBlockState(), 3);
                    IOWorker worker = (IOWorker) level.getChunkSource().chunkMap.chunkScanner();
                    try {
                        for (ChunkPos p : backlogAt)
                            worker.store(p, null); // the region file keeps its sectors free; the next run reuses them
                    } catch (RuntimeException e) {
                        MicRtsim.LOGGER.warn("[gametest] clearing the backlog's chunks", e);
                    }
                    try {
                        Files.deleteIfExists(file[0]);
                    } catch (IOException ignored) {
                    }
                })
                .thenSucceed();
    }

    /** The strategy {@link ChunkWriteBarrier#strategy} should pick for {@code worker} (JB2, JB3). */
    private static String expectedStrategy(IOWorker worker) {
        if (worker.getClass() == IOWorker.class)
            return "vanilla";
        if (worker.getClass().getName().equals("com.ishland.c2me.rewrites.chunkio.common.C2MEStorageVanillaInterface"))
            return "c2me";
        return "synchronize";
    }

    /**
     * Detached chunks for the hold tests sit in the backlog's region, on its last row past the backlog's end, so
     * their region file exists: C2ME's writes are not synchronous and the barrier forces each noted chunk's region
     * file, and one that is missing holds the round. The anchor is written first.
     */
    private static ChunkPos spare(int i) {
        int base = BACKLOG_REGION * 32;
        return new ChunkPos(base + 20 + i, base + 31);
    }

    private static CompletableFuture<Void> writeAnchor(ServerLevel level) {
        ChunkPos anchor = new ChunkPos(BACKLOG_REGION * 32 + 31, BACKLOG_REGION * 32 + 31);
        return ((IOWorker) level.getChunkSource().chunkMap.chunkScanner()).store(anchor, filler(anchor));
    }

    /** On the IO chain, after everything queued on it so far (the test journal's write task). */
    private static void peekAfter(ServerLevel level, BlockPos chestPos, Path journalFile, List<CompletableFuture<Void>> backlog,
                                  AtomicReference<Peek> out) {
        Path region = DimensionType.getStorageFolder(level.dimension(), level.getServer().getWorldPath(LevelResource.ROOT)).resolve("region");
        ChunkPos pos = new ChunkPos(chestPos);
        IOUtilities.withIOWorker(() -> {
            int done = 0;
            for (CompletableFuture<Void> f : backlog)
                if (f.isDone())
                    done++;
            try {
                CompoundTag chunk = readRegionChunk(region, pos);
                int d = 0, e = 0;
                if (chunk != null) {
                    ListTag bes = chunk.getList("block_entities", Tag.TAG_COMPOUND);
                    for (int i = 0; i < bes.size(); i++) {
                        CompoundTag be = bes.getCompound(i);
                        if (be.getInt("x") != chestPos.getX() || be.getInt("y") != chestPos.getY() || be.getInt("z") != chestPos.getZ())
                            continue;
                        ListTag items = be.getList("Items", Tag.TAG_COMPOUND);
                        for (int j = 0; j < items.size(); j++) {
                            String id = items.getCompound(j).getString("id");
                            if ("minecraft:diamond".equals(id))
                                d += items.getCompound(j).getInt("count");
                            if ("minecraft:emerald".equals(id))
                                e += items.getCompound(j).getInt("count");
                        }
                    }
                }
                boolean on = Files.isRegularFile(journalFile);
                long next = on ? NbtIo.readCompressed(journalFile, NbtAccounter.unlimitedHeap()).getCompound("data").getLong("nextObsSeq") : -1;
                out.set(new Peek(d, e, on, next, done, chunk == null ? "the chunk is not in its region file" : null));
            } catch (Exception ex) {
                out.set(new Peek(-1, -1, false, -1, done, ex.toString()));
            }
        });
    }

    /** What the IO chain saw after a test journal's save. */
    private record Round(boolean onDisk, long writes, long held, long late, boolean dirty, String lastHeld) {}

    private static void saveAndLook(ServerLevel level, JournalSavedData data, Path file, AtomicReference<Round> out) {
        out.set(null);
        data.setDirty();
        data.save(file.toFile(), level.registryAccess());
        ChunkWriteBarrier b = data.chunks();
        IOUtilities.withIOWorker(() -> out.set(new Round(Files.isRegularFile(file), data.writes(), b.held(), b.late(), data.isDirty(), b.lastHeld())));
    }

    /**
     * A chunk whose write the barrier cannot see yet holds the journal's file back, without a wait (JB1, JB5).
     * A noted chunk that is not loaded and whose unload was never seen is what C2ME's async unload leaves while it
     * serializes the chunk off the server thread: nothing has reached the storage. Here it is a detached
     * {@code LevelChunk} that never was in its level. The write is held (the file is not written, the journal
     * stays dirty for the next save); after the chunk's {@code ChunkEvent.Unload} the next save writes it. Then the
     * valve: a chunk that never resolves holds {@link ChunkWriteBarrier#GIVE_UP_ROUNDS} - 1 saves, and the next
     * one is written without it, counted late.
     */
    @GameTest(template = RtsimTests.TEMPLATE, batch = "mic_rtsim_journal_barrier", timeoutTicks = 6000)
    public static void aChunkStillBeingUnloadedHoldsTheJournalsFileToTheNextSave(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        JournalSavedData data = JournalSavedData.fresh();
        LevelChunk inFlight = new LevelChunk(level, spare(0));
        LevelChunk never = new LevelChunk(level, spare(1));
        AtomicReference<Round> round = new AtomicReference<>();
        Path[] file = {null};
        CompletableFuture<Void> anchor = writeAnchor(level);
        var seq = helper.startSequence()
                .thenWaitUntil(() -> check("the region file is there", anchor.isDone()))
                .thenExecute(() -> {
                    try {
                        file[0] = testFile(level);
                    } catch (IOException e) {
                        throw new RuntimeException(e);
                    }
                    data.chunks().dependsOn(inFlight);
                    saveAndLook(level, data, file[0], round);
                })
                .thenWaitUntil(io(() -> check("the chain looked", round.get() != null)))
                .thenExecute(() -> {
                    Round r = round.get();
                    record("aChunkStillBeingUnloadedHoldsTheJournalsFileToTheNextSave", "while in flight", r);
                    check("not written", !r.onDisk() && r.writes() == 0);
                    check("held once, for the unload: " + r.lastHeld(), r.held() == 1 && r.lastHeld().contains("being unloaded"));
                    check("still dirty for the next world save", r.dirty());
                    check("waited for nothing", data.chunks().lastWaitMillis() == -1);
                    data.chunks().onUnload(inFlight);
                    saveAndLook(level, data, file[0], round);
                })
                .thenWaitUntil(io(() -> check("the chain looked", round.get() != null)))
                .thenExecute(() -> {
                    Round r = round.get();
                    record("aChunkStillBeingUnloadedHoldsTheJournalsFileToTheNextSave", "after the unload", r);
                    check("written once the unload is seen", r.onDisk() && r.writes() == 1 && r.held() == 1 && r.late() == 0);
                    check("clean", !r.dirty());
                    data.chunks().dependsOn(never);
                    saveAndLook(level, data, file[0], round);
                });
        for (int i = 1; i < ChunkWriteBarrier.GIVE_UP_ROUNDS; i++) {
            int n = i;
            seq = seq.thenWaitUntil(io(() -> check("the chain looked", round.get() != null)))
                    .thenExecute(() -> {
                        Round r = round.get();
                        check("held " + n + " time(s) in a row: " + r, r.writes() == 1 && r.held() == 1 + n && r.late() == 0 && r.dirty());
                        saveAndLook(level, data, file[0], round);
                    });
        }
        seq.thenWaitUntil(io(() -> check("the chain looked", round.get() != null)))
                .thenExecute(() -> {
                    Round r = round.get();
                    record("aChunkStillBeingUnloadedHoldsTheJournalsFileToTheNextSave", "the valve", r);
                    check("written without it after " + ChunkWriteBarrier.GIVE_UP_ROUNDS + " held saves, counted late: " + r,
                            r.writes() == 2 && r.held() == ChunkWriteBarrier.GIVE_UP_ROUNDS + 1 && r.late() == 1 && !r.dirty());
                    try {
                        Files.deleteIfExists(file[0]);
                    } catch (IOException ignored) {
                    }
                })
                .thenSucceed();
    }

    /**
     * The world-save marker (WORLD_SAVED) never claims more than the journal's file on disk holds (JB1): while
     * the server's journal is held by a chunk in flight, the marker chained after a world save reports at most
     * what the file had before. The old marker reported the journal in memory, whatever the write did.
     */
    @GameTest(template = RtsimTests.TEMPLATE, batch = "mic_rtsim_journal_barrier_marker", timeoutTicks = 6000)
    public static void theWorldSavedMarkerNeverPassesTheJournalOnDisk(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        RtsimSession s = RtsimTests.session();
        JournalSavedData data = s.journal();
        LevelChunk inFlight = new LevelChunk(level, spare(2));
        AtomicReference<long[]> before = new AtomicReference<>();
        AtomicReference<long[]> after = new AtomicReference<>();
        CompletableFuture<Void> anchor = writeAnchor(level);
        helper.startSequence()
                .thenWaitUntil(() -> check("the region file is there", anchor.isDone()))
                .thenExecute(() -> {
                    data.chunks().dependsOn(inFlight);
                    data.setDirty();
                    // read on the chain, after any write task of an earlier save still queued there
                    IOUtilities.withIOWorker(() -> before.set(new long[] {data.writtenThrough(), data.chunks().held(), data.writes()}));
                    level.getServer().saveEverything(true, false, false);
                    IOUtilities.withIOWorker(() -> after.set(new long[] {s.boundary().durablePending(), data.writtenThrough(), data.chunks().held(), data.writes()}));
                })
                .thenWaitUntil(io(() -> check("the chain looked", after.get() != null)))
                .thenExecute(() -> {
                    long[] a = after.get(), b = before.get();
                    long inMemory = s.boundary().journal().nextObsSeq() - 1;
                    record("theWorldSavedMarkerNeverPassesTheJournalOnDisk", "marker, file, in memory",
                            a[0] + ", " + a[1] + " (before " + b[0] + "), " + inMemory + "; held " + (a[2] - b[1]));
                    check("the save was held", a[2] > b[1]);
                    check("the file was not written", a[3] == b[2] && a[1] == b[0]);
                    check("the marker is at most the file: " + a[0] + " <= " + a[1], a[0] <= a[1]);
                    data.chunks().onUnload(inFlight);
                    after.set(null);
                    level.getServer().saveEverything(true, false, false);
                    IOUtilities.withIOWorker(() -> after.set(new long[] {s.boundary().durablePending(), data.writtenThrough(), data.chunks().held(), data.writes()}));
                })
                .thenWaitUntil(io(() -> check("the chain looked", after.get() != null)))
                .thenExecute(() -> {
                    long[] a = after.get();
                    record("theWorldSavedMarkerNeverPassesTheJournalOnDisk", "after the unload: marker, file", a[0] + ", " + a[1]);
                    check("the file was written", a[3] > before.get()[2]);
                    check("the marker is the file: " + a[0] + " == " + a[1], a[0] == a[1]);
                })
                .thenSucceed();
    }

    /** One chunk's NBT straight from its region file (a sector table, then length, compression, data). */
    static CompoundTag readRegionChunk(Path regionDir, ChunkPos pos) throws IOException {
        Path file = regionDir.resolve("r." + pos.getRegionX() + "." + pos.getRegionZ() + ".mca");
        if (!Files.isRegularFile(file))
            return null;
        try (FileChannel ch = FileChannel.open(file, StandardOpenOption.READ)) {
            ByteBuffer header = ByteBuffer.allocate(4096);
            ch.read(header, 0);
            int entry = header.getInt((pos.getRegionLocalX() + pos.getRegionLocalZ() * 32) * 4);
            if (entry == 0)
                return null;
            ByteBuffer data = ByteBuffer.allocate((entry & 0xFF) * 4096);
            ch.read(data, (long) (entry >>> 8) * 4096);
            data.flip();
            int length = data.getInt();
            byte version = data.get();
            if ((version & 0x80) != 0)
                throw new IOException("the chunk is in an external .mcc file");
            byte[] bytes = new byte[length - 1];
            data.get(bytes);
            try (java.io.DataInputStream in = new java.io.DataInputStream(
                    RegionFileVersion.fromId(version).wrap(new java.io.ByteArrayInputStream(bytes)))) {
                return NbtIo.read(in, NbtAccounter.unlimitedHeap());
            }
        }
    }
}
