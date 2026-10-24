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
 *
 * <p><b>The engine life</b> ({@code RtsimSession.engineLife}, docs/choices-reinstall-guard.md RG14).
 * The records speak to the engine that wrote them: its op ids, its members. An engine that started over
 * (a reinstall reset, a deliberate sim reset) has none of that, and took what the world holds from the
 * world itself, so a record of the old engine journaled into it again could only apply a second time
 * what the world already shows. So the holder keeps the life its records were written in, and a holder
 * that loads into another life drops them unjournaled, and its op ids stop counting as applied. The
 * generation is per record, the life per holder: one holder's records can span sessions of one life.
 */
public record PendingRecords(long life, List<Entry> entries) {
    /** One record as first journaled, and the generation it was journaled in. */
    public record Entry(long gen, byte[] bytes) {
        static final Codec<Entry> CODEC = RecordCodecBuilder.create(i -> i.group(
                Codec.LONG.fieldOf("gen").forGetter(Entry::gen),
                Codec.BYTE_BUFFER.fieldOf("record").forGetter(e -> ByteBuffer.wrap(e.bytes))
        ).apply(i, (g, b) -> new Entry(g, PendingRecords.bytes(b))));
    }

    public static final PendingRecords EMPTY = new PendingRecords(0, List.of());

    private static byte[] bytes(ByteBuffer b) {
        byte[] bytes = new byte[b.remaining()];
        b.duplicate().get(bytes);
        return bytes;
    }

    /** Records with no generation (written before generations): gen 0, judged as before (choice SC129). */
    private static final Codec<List<Entry>> PLAIN = Codec.BYTE_BUFFER.listOf().xmap(
            list -> {
                List<Entry> out = new ArrayList<>(list.size());
                for (ByteBuffer b : list)
                    out.add(new Entry(0, bytes(b)));
                return List.copyOf(out);
            },
            entries -> {
                List<ByteBuffer> out = new ArrayList<>(entries.size());
                for (Entry e : entries)
                    out.add(ByteBuffer.wrap(e.bytes));
                return out;
            });

    /**
     * {life, entries}; {life, records} (written before generations) reads with generation 0, and a plain list
     * (written before the life existed) as life 0, an old page directory's.
     */
    public static final Codec<PendingRecords> CODEC = Codec.withAlternative(
            RecordCodecBuilder.create(i -> i.group(
                    Codec.LONG.fieldOf("life").forGetter(PendingRecords::life),
                    Entry.CODEC.listOf().fieldOf("entries").forGetter(PendingRecords::entries)
            ).apply(i, (l, e) -> new PendingRecords(l, List.copyOf(e)))),
            Codec.withAlternative(
                    RecordCodecBuilder.create(i -> i.group(
                            Codec.LONG.fieldOf("life").forGetter(PendingRecords::life),
                            PLAIN.fieldOf("records").forGetter(PendingRecords::entries)).apply(i, PendingRecords::new)),
                    PLAIN.xmap(e -> new PendingRecords(0, e), PendingRecords::entries)));

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

    /**
     * These records and {@code more}, written in engine life {@code life} and journaled in generation {@code gen}.
     * Records of another life are left out: they would be dropped at the holder's next load anyway, and a holder
     * carries one life.
     */
    public PendingRecords plus(long life, long gen, List<byte[]> more) {
        List<Entry> out = new ArrayList<>(this.life == life ? entries : List.of());
        for (byte[] r : more)
            out.add(new Entry(gen, r));
        return new PendingRecords(life, List.copyOf(out));
    }

    /**
     * These records and {@code more}, which keep the generation each was journaled in (a holder taking over
     * another's records), written in engine life {@code life}. Records of another life are left out, as in
     * {@link #plus}.
     */
    public PendingRecords plusEntries(long life, List<Entry> more) {
        List<Entry> out = new ArrayList<>(this.life == life ? entries : List.of());
        out.addAll(more);
        return new PendingRecords(life, List.copyOf(out));
    }

    /** The same life with the records {@code keep} accepts (what is left once some were acknowledged). */
    public PendingRecords filter(Predicate<byte[]> keep) {
        List<Entry> out = new ArrayList<>(entries.size());
        for (Entry e : entries)
            if (keep.test(e.bytes))
                out.add(e);
        return new PendingRecords(life, List.copyOf(out));
    }
}
