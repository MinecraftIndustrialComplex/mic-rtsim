package com.minecraftindustrialcomplex.mic_rtsim.puppet;

import com.mojang.serialization.Codec;
import com.mojang.serialization.codecs.RecordCodecBuilder;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Predicate;

/**
 * Journal records that the world side of a cross-store transfer carries with it, in the
 * {@code mic_rtsim:pending} attachment, until the engine has acknowledged them.
 *
 * <p><b>Why.</b> Minecraft saves a chunk (blocks or entities) whenever it unloads, but the journal
 * only at an autosave. So a chunk on disk can be newer than the journal: a chest that received an
 * engine transfer, a llama a player took, the items a caravan llama dropped when it died. After a
 * crash the journal would not know about them, and the engine would resend the transfer, or bring
 * the llama and its cargo back. The record travels with the thing instead:
 * <ul>
 *   <li>a chunk carries the COMMAND_RESULT of every transfer applied into it, and an engine op id
 *       found there is never applied again;</li>
 *   <li>a taken creature carries its MEMBER_STATE and TAKEN; each item a dying member dropped
 *       carries its MEMBER_STATE and DIED.</li>
 * </ul>
 * When the holder loads in a later session, the bridge journals a copy of each record (REASSERT,
 * docs/boundary.md §3.2), and drops them from the holder once the engine acknowledges the copies. So
 * each record is kept with the generation it was journaled in (the session's JOURNAL_GEN nonce): the
 * engine applies a copy only when its state lacks the record, judged by that generation, never by
 * an op id it may have forgotten.
 */
public record PendingRecords(List<Entry> entries) {
    /** One record as first journaled, and the generation it was journaled in. */
    public record Entry(long gen, byte[] bytes) {
        static final Codec<Entry> CODEC = RecordCodecBuilder.create(i -> i.group(
                Codec.LONG.fieldOf("gen").forGetter(Entry::gen),
                Codec.BYTE_BUFFER.fieldOf("record").forGetter(e -> ByteBuffer.wrap(e.bytes))
        ).apply(i, (g, b) -> {
            byte[] bytes = new byte[b.remaining()];
            b.duplicate().get(bytes);
            return new Entry(g, bytes);
        }));
    }

    public static final PendingRecords EMPTY = new PendingRecords(List.of());

    public static final Codec<PendingRecords> CODEC = Entry.CODEC.listOf().xmap(
            l -> new PendingRecords(List.copyOf(l)), PendingRecords::entries);

    public boolean isEmpty() {
        return entries.isEmpty();
    }

    /** The records' bytes, in order. */
    public List<byte[]> records() {
        List<byte[]> out = new ArrayList<>(entries.size());
        for (Entry e : entries)
            out.add(e.bytes);
        return out;
    }

    /** These records and {@code more}, journaled in generation {@code gen}. */
    public PendingRecords plus(long gen, List<byte[]> more) {
        List<Entry> out = new ArrayList<>(entries);
        for (byte[] r : more)
            out.add(new Entry(gen, r));
        return new PendingRecords(List.copyOf(out));
    }

    /** The records {@code keep} accepts (what is left once some were acknowledged). */
    public PendingRecords filter(Predicate<byte[]> keep) {
        List<Entry> out = new ArrayList<>(entries.size());
        for (Entry e : entries)
            if (keep.test(e.bytes))
                out.add(e);
        return new PendingRecords(List.copyOf(out));
    }
}
