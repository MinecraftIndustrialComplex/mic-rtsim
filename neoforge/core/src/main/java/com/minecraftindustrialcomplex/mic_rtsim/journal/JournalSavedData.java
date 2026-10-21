package com.minecraftindustrialcomplex.mic_rtsim.journal;

import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.ByteArrayTag;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.StringTag;
import net.minecraft.nbt.Tag;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.level.saveddata.SavedData;

import java.io.File;

/**
 * The journal SavedData, {@code mic_rtsim:journal} (PLAN.md §4.9), in the overworld's data storage.
 * On disk it is {@code data/mic_rtsim_journal.dat}: Windows forbids the colon in a file name.
 *
 * <p>Vanilla writes it at its own save, from a copy, on its IO worker; the server thread never
 * waits for it. Appending only sets the dirty flag. Its write waits for the chunks its records speak
 * for ({@link ChunkWriteBarrier}), so the file on disk never claims what a chunk on disk lacks.
 */
public final class JournalSavedData extends SavedData {
    public static final String FILE = "mic_rtsim_journal";

    private final Journal journal;
    private final ChunkWriteBarrier chunks = new ChunkWriteBarrier();

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

    /** The chunks the journal's next file waits for (docs/choices-repair.md R21). */
    public ChunkWriteBarrier chunks() {
        return chunks;
    }

    /**
     * Vanilla's write of this file ({@code DimensionDataStorage.save}, in {@code saveLevelData}, on the server
     * thread): first the chunks the journal speaks for go on their way to disk, and a wait for them is queued
     * on the SavedData IO chain, then {@code super} serializes the journal and queues its write after that wait.
     */
    @Override
    public void save(File file, HolderLookup.Provider registries) {
        if (isDirty())
            chunks.beforeJournalWrite();
        super.save(file, registries);
    }

    @Override
    public CompoundTag save(CompoundTag tag, HolderLookup.Provider registries) {
        Journal.Snapshot s = journal.snapshot();
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
        return new JournalSavedData(Journal.restore(new Journal.Snapshot(
                tag.getLong("lastAppliedCmdSeq"), tag.getLong("nextObsSeq"), seqs, records, kinds, names)));
    }

    /** A copy through the same NBT encoding vanilla writes: what a reload after a crash reads back. */
    public static JournalSavedData roundTrip(JournalSavedData data, HolderLookup.Provider registries) {
        return load(data.save(new CompoundTag(), registries).copy(), registries);
    }
}
