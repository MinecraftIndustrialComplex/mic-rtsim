package com.minecraftindustrialcomplex.mic_rtsim.journal;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.ByteArrayTag;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtUtils;
import net.minecraft.nbt.StringTag;
import net.minecraft.nbt.Tag;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.level.saveddata.SavedData;
import net.neoforged.neoforge.common.IOUtilities;

import java.io.File;
import java.io.IOException;
import java.nio.file.Path;
import java.util.List;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.Consumer;

/**
 * The journal SavedData, {@code mic_rtsim:journal} (PLAN.md §4.9), in the overworld's data storage.
 * On disk it is {@code data/mic_rtsim_journal.dat}: Windows forbids the colon in a file name.
 *
 * <p>Vanilla writes it at its own save, from a copy, on NeoForge's SavedData IO chain; the server thread
 * never waits for it. Appending only sets the dirty flag. Its write waits for the chunks its records speak
 * for ({@link ChunkWriteBarrier}) and is skipped, to the next world save, when they are not on disk: the file
 * on disk never claims what a chunk on disk lacks. In a world save it is the last SavedData written, behind every
 * level's ({@link WorldSaveOrder}), with the levels' {@code data/} directories synced first: the file on disk never
 * claims what another SavedData file of the same save lacks either (docs/choices-journal-barrier.md RP-200, RP-201).
 * Only a world save writes it: a level saved by itself leaves it dirty for the next world save (RP-203).
 *
 * <p>A held write is held again at every world save, with no limit; a flush save tries once more and then keeps the
 * older file, logged at error ({@link #flushKept}). The file's lag is counted in applied ops ({@link #appliedBehind})
 * and logged once past {@link #BEHIND_WARN} (RP-205).
 */
public final class JournalSavedData extends SavedData {
    public static final String FILE = "mic_rtsim_journal";
    /**
     * Applied ops the file on disk may be behind the journal before it is logged, once (PLACEHOLDER, TUNABLE: several
     * autosaves' worth on a busy server; RP-205).
     */
    public static final long BEHIND_WARN = 10_000;

    private final Journal journal;
    private final ChunkWriteBarrier chunks = new ChunkWriteBarrier();
    /** The last write was held or failed (set on the IO chain). */
    private volatile boolean retry;
    private volatile long writtenThrough;
    private final AtomicLong writes = new AtomicLong();
    private final AtomicLong skipped = new AtomicLong();
    /** The last write was held by its chunks (not an IO error), set on the IO chain: a flush save's retry looks. */
    private volatile boolean heldLast;
    /** Flush saves that kept the older file after their second try. */
    private final AtomicLong flushKept = new AtomicLong();
    /** {@link Journal#appliedRecords} at the snapshot the file on disk holds (this start's; 0 before the first write). */
    private volatile long appliedDurable;
    private boolean warnedBehind;

    /**
     * GameTests: run once, on the IO chain, right before the journal's next write (its chunks on disk), with the
     * file. What it reads on disk is what a crash at that instant leaves.
     */
    public static final AtomicReference<Consumer<Path>> BEFORE_WRITE_FOR_TEST = new AtomicReference<>();
    /** GameTests: run once, on the IO chain, right after the journal's next write. */
    public static final AtomicReference<Consumer<Path>> AFTER_WRITE_FOR_TEST = new AtomicReference<>();

    private JournalSavedData(Journal journal) {
        this.journal = journal;
        journal.onChange(this::setDirty);
    }

    public static JournalSavedData fresh() {
        return new JournalSavedData(new Journal());
    }

    public static final Factory<JournalSavedData> FACTORY = new Factory<>(JournalSavedData::fresh, JournalSavedData::load);

    public static JournalSavedData get(MinecraftServer server) {
        return server.overworld().getDataStorage().computeIfAbsent(FACTORY, FILE);
    }

    public Journal journal() {
        return journal;
    }

    /** The chunks the journal's next file waits for (docs/choices-repair.md R21, docs/choices-journal-barrier.md). */
    public ChunkWriteBarrier chunks() {
        return chunks;
    }

    /**
     * Vanilla's write of this file ({@code DimensionDataStorage.save}, in {@code saveLevelData}, on the server
     * thread), replacing {@code SavedData.save}'s (docs/choices-journal-barrier.md JB1). The snapshot is taken and
     * encoded here, and the chunks it speaks for are sent on their way to disk ({@link ChunkWriteBarrier#arm}); the
     * task, queued on NeoForge's SavedData IO chain when the world save returns ({@link WorldSaveOrder}), waits for
     * them and writes the file only if they are all on disk. Otherwise the file stays as it was and the next world
     * save writes a newer snapshot ({@link #isDirty}).
     */
    @Override
    public void save(File file, HolderLookup.Provider registries) {
        if (!isDirty())
            return;
        if (!WorldSaveOrder.saving()) {
            skip();
            return;
        }
        boolean last = WorldSaveOrder.retrying();
        retry = false;
        heldLast = false;
        ChunkWriteBarrier.Round round = chunks.arm(last);
        Journal.Snapshot s = journal.snapshot();
        CompoundTag tag = new CompoundTag();
        tag.put("data", write(s, new CompoundTag()));
        NbtUtils.addCurrentDataVersion(tag);
        CompoundTag copied = tag.copy(); // as SavedData.save: nothing shared with the live journal
        long through = s.nextObsSeq() - 1;
        long applied = journal.appliedRecords();
        watchBehind(applied);
        List<Path> dirs = WorldSaveOrder.dataDirs(file.toPath().getParent());
        setDirty(false);
        WorldSaveOrder.queue(() -> writeWhenLanded(round, copied, file, through, applied, dirs, last));
        WorldSaveOrder.afterFlush(() -> retryAfterFlush(file, registries));
    }

    /**
     * A flush save's second try, on the server thread, after the chain wrote that save (RP-205): when the first was
     * held by its chunks, every chunk is flushed now, so the barrier has what it waited for, or never will.
     */
    private void retryAfterFlush(File file, HolderLookup.Provider registries) {
        if (!heldLast)
            return;
        MicRtsim.LOGGER.warn("[mic_rtsim] a flush save held the journal's file ({}); every chunk is flushed now, so it tries once more", chunks.lastHeld());
        save(file, registries);
    }

    /** Logs once when the file on disk is {@link #BEHIND_WARN} applied ops behind; again after it catches up. */
    private void watchBehind(long applied) {
        long behind = applied - appliedDurable;
        if (behind < BEHIND_WARN) {
            warnedBehind = false;
        } else if (!warnedBehind) {
            warnedBehind = true;
            MicRtsim.LOGGER.warn("[mic_rtsim] the journal's file on disk is {} applied ops behind (its writes held {} world saves in a row: {}); a crash"
                    + " replays them from the older file, and the chunks and stores they changed must answer them (RP-205)",
                    behind, chunks.heldInARow(), chunks.lastHeld());
        }
    }

    /**
     * On the IO chain, behind every SavedData of the save. Never throws: an exception would skip every SavedData
     * write queued after it. {@code last}: a flush save's second try, whose hold keeps the older file.
     */
    private void writeWhenLanded(ChunkWriteBarrier.Round round, CompoundTag tag, File file, long through, long applied, List<Path> dirs, boolean last) {
        try {
            if (!chunks.await(round)) {
                retry = true;
                heldLast = true;
                if (last) {
                    flushKept.incrementAndGet();
                    MicRtsim.LOGGER.error("[mic_rtsim] the journal's file is kept as it was at this flush save, after a second try ({}): it is {}"
                            + " applied ops behind; a restart replays them from the older file (docs/choices-journal-barrier.md RP-205)",
                            chunks.lastHeld(), applied - appliedDurable);
                }
                return;
            }
            for (Path d : dirs) // their renames are durable before the journal's can be (RP-201)
                DirectorySync.sync(d);
            forTest(BEFORE_WRITE_FOR_TEST, file);
            IOUtilities.writeNbtCompressed(tag, file.toPath());
            DirectorySync.sync(file.toPath().getParent()); // the marker (WORLD_SAVED) never claims a rename a power loss undoes
            if (through > writtenThrough)
                writtenThrough = through;
            if (applied > appliedDurable)
                appliedDurable = applied;
            writes.incrementAndGet();
            forTest(AFTER_WRITE_FOR_TEST, file);
        } catch (IOException | RuntimeException e) {
            retry = true;
            MicRtsim.LOGGER.error("[mic_rtsim] could not write the journal's file {}", file, e);
        }
    }

    /**
     * The overworld saved by itself, outside {@code MinecraftServer.saveAllChunks} (a mod's own
     * {@code ServerLevel.save}, or a save off the server thread): the other levels' SavedData are not being written,
     * so a file written now could claim an effect that lives in one of them (Diesel's {@code cdg_oil_chunks} in the
     * nether) while that file is older on disk (RP-203). The journal stays dirty, its barrier keeps its notes, and
     * the next world save writes it. Vanilla saves a level only inside saveAllChunks; the first skip is logged, with
     * where it came from, and every one is counted ({@link #skipped}).
     */
    private void skip() {
        if (skipped.incrementAndGet() == 1)
            MicRtsim.LOGGER.info("[mic_rtsim] the overworld saved outside a world save (thread {}): the journal's file waits for the"
                    + " next world save; logged once, counted in rtsim perf", Thread.currentThread().getName(), new Throwable("where the save came from"));
    }

    /** Saves of the overworld outside a world save that left the journal's file to the next one (RP-203). */
    public long skipped() {
        return skipped.get();
    }

    /** Flush saves (the stop, {@code /save-all flush}) that kept the older file after their second try (RP-205). */
    public long flushKept() {
        return flushKept.get();
    }

    /**
     * Records holding an applied op journaled since the snapshot the file on disk holds (this start's): what a crash
     * now would replay from the older file (RP-205). Server thread.
     */
    public long appliedBehind() {
        return journal.appliedRecords() - appliedDurable;
    }

    /** One line for {@code /rtsim perf} (RP-205). Server thread. */
    public String describe() {
        ChunkWriteBarrier b = chunks;
        return "journal file: written " + writes() + ", through observation " + writtenThrough + ", " + appliedBehind() + " applied ops behind"
                + "; held " + b.held() + " (" + b.heldInARow() + " in a row" + (b.lastHeld().isEmpty() ? "" : ", last: " + b.lastHeld()) + ")"
                + ", late " + b.late() + ", wait max " + b.maxWaitMillis() + " ms, kept at a flush save " + flushKept()
                + ", outside a world save " + skipped() + "; " + DurableOpRings.perfLine()
                + "; directory syncs " + DirectorySync.synced() + " (" + DirectorySync.failed() + " failed, " + DirectorySync.nanos() / 1_000_000 + " ms)";
    }

    private static void forTest(AtomicReference<Consumer<Path>> hook, File file) {
        Consumer<Path> h = hook.getAndSet(null);
        if (h == null)
            return;
        try {
            h.accept(file.toPath());
        } catch (RuntimeException e) {
            MicRtsim.LOGGER.error("[mic_rtsim] a GameTest's journal write hook failed", e);
        }
    }

    /** Dirty, or the last write was held or failed: the next world save writes it. */
    @Override
    public boolean isDirty() {
        return super.isDirty() || retry;
    }

    /**
     * The last observation sequence the file on disk holds: what a crash now would come back to. Advanced on the IO
     * chain once a write succeeded; read by the world-save marker (WORLD_SAVED), which goes on the same chain.
     */
    public long writtenThrough() {
        return writtenThrough;
    }

    /** Files written since the server started (the GameTests read it). */
    public long writes() {
        return writes.get();
    }

    @Override
    public CompoundTag save(CompoundTag tag, HolderLookup.Provider registries) {
        return write(journal.snapshot(), tag);
    }

    private static CompoundTag write(Journal.Snapshot s, CompoundTag tag) {
        tag.putLong("lastAppliedCmdSeq", s.lastAppliedCmdSeq());
        tag.putLong("nextObsSeq", s.nextObsSeq());
        tag.putLongArray("seqs", s.seqs());
        ListTag records = new ListTag();
        for (byte[] r : s.records())
            records.add(new ByteArrayTag(r));
        tag.put("records", records);
        tag.putIntArray("dictKinds", s.dictKinds());
        ListTag names = new ListTag();
        for (String n : s.dictNames())
            names.add(StringTag.valueOf(n));
        tag.put("dictNames", names);
        if (s.pinnedGen() != null)
            tag.putByteArray("pinnedGen", s.pinnedGen());
        return tag;
    }

    public static JournalSavedData load(CompoundTag tag, HolderLookup.Provider registries) {
        long[] seqs = tag.getLongArray("seqs");
        ListTag recordList = tag.getList("records", Tag.TAG_BYTE_ARRAY);
        if (recordList.size() != seqs.length)
            throw new IllegalStateException("mic_rtsim journal is corrupt: " + seqs.length + " sequences, " + recordList.size() + " records");
        byte[][] records = new byte[seqs.length][];
        for (int i = 0; i < seqs.length; i++)
            records[i] = ((ByteArrayTag) recordList.get(i)).getAsByteArray();
        int[] kinds = tag.getIntArray("dictKinds");
        ListTag nameList = tag.getList("dictNames", Tag.TAG_STRING);
        String[] names = new String[nameList.size()];
        for (int i = 0; i < names.length; i++)
            names[i] = nameList.getString(i);
        if (kinds.length != names.length)
            throw new IllegalStateException("mic_rtsim journal dictionary is corrupt");
        JournalSavedData d = new JournalSavedData(Journal.restore(new Journal.Snapshot(
                tag.getLong("lastAppliedCmdSeq"), tag.getLong("nextObsSeq"), seqs, records, kinds, names,
                tag.contains("pinnedGen", Tag.TAG_BYTE_ARRAY) ? tag.getByteArray("pinnedGen") : null)));
        d.writtenThrough = Math.max(0, tag.getLong("nextObsSeq") - 1);
        return d;
    }

    /** A copy through the same NBT encoding vanilla writes: what a reload after a crash reads back. */
    public static JournalSavedData roundTrip(JournalSavedData data, HolderLookup.Provider registries) {
        return load(data.save(new CompoundTag(), registries).copy(), registries);
    }
}
