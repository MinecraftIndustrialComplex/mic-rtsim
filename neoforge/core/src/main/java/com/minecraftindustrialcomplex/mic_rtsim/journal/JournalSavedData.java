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
import java.util.concurrent.atomic.AtomicLong;

/**
 * The journal SavedData, {@code mic_rtsim:journal} (PLAN.md §4.9), in the overworld's data storage.
 * On disk it is {@code data/mic_rtsim_journal.dat}: Windows forbids the colon in a file name.
 *
 * <p>Vanilla writes it at its own save, from a copy, on NeoForge's SavedData IO chain; the server thread
 * never waits for it. Appending only sets the dirty flag. Its write waits for the chunks its records speak
 * for ({@link ChunkWriteBarrier}) and is skipped, to the next world save, when they are not on disk: the file
 * on disk never claims what a chunk on disk lacks.
 */
public final class JournalSavedData extends SavedData {
    public static final String FILE = "mic_rtsim_journal";

    private final Journal journal;
    private final ChunkWriteBarrier chunks = new ChunkWriteBarrier();
    /** The last write was held or failed (set on the IO chain). */
    private volatile boolean retry;
    private volatile long writtenThrough;
    private final AtomicLong writes = new AtomicLong();

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
     * task queued on NeoForge's SavedData IO chain waits for them and writes the file only if they are all on
     * disk. Otherwise the file stays as it was and the next world save writes a newer snapshot ({@link #isDirty}).
     */
    @Override
    public void save(File file, HolderLookup.Provider registries) {
        if (!isDirty())
            return;
        retry = false;
        ChunkWriteBarrier.Round round = chunks.arm();
        Journal.Snapshot s = journal.snapshot();
        CompoundTag tag = new CompoundTag();
        tag.put("data", write(s, new CompoundTag()));
        NbtUtils.addCurrentDataVersion(tag);
        CompoundTag copied = tag.copy(); // as SavedData.save: nothing shared with the live journal
        long through = s.nextObsSeq() - 1;
        setDirty(false);
        IOUtilities.withIOWorker(() -> writeWhenLanded(round, copied, file, through));
    }

    /** On the IO chain. Never throws: an exception would skip every SavedData write queued after it. */
    private void writeWhenLanded(ChunkWriteBarrier.Round round, CompoundTag tag, File file, long through) {
        try {
            if (!chunks.await(round)) {
                retry = true;
                return;
            }
            IOUtilities.writeNbtCompressed(tag, file.toPath());
            if (through > writtenThrough)
                writtenThrough = through;
            writes.incrementAndGet();
        } catch (IOException | RuntimeException e) {
            retry = true;
            MicRtsim.LOGGER.error("[mic_rtsim] could not write the journal's file {}", file, e);
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
                tag.getLong("lastAppliedCmdSeq"), tag.getLong("nextObsSeq"), seqs, records, kinds, names)));
        d.writtenThrough = Math.max(0, tag.getLong("nextObsSeq") - 1);
        return d;
    }

    /** A copy through the same NBT encoding vanilla writes: what a reload after a crash reads back. */
    public static JournalSavedData roundTrip(JournalSavedData data, HolderLookup.Provider registries) {
        return load(data.save(new CompoundTag(), registries).copy(), registries);
    }
}
