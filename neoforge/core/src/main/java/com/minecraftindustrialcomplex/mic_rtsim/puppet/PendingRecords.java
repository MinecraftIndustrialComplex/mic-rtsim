package com.minecraftindustrialcomplex.mic_rtsim.puppet;

import com.mojang.serialization.Codec;
import com.mojang.serialization.codecs.RecordCodecBuilder;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;

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
 * When the holder loads in a later session, the bridge journals the records again (new sequences,
 * same bytes otherwise, the same op ids), and drops them from the holder once the engine acknowledges
 * them in that session. The engine treats a repeated record as a no-op (by op id, and by state: a
 * member already gone, an op already done).
 *
 * <p><b>The engine life</b> ({@code RtsimSession.engineLife}, docs/choices-reinstall-guard.md RG14).
 * The records speak to the engine that wrote them: its op ids, its members. An engine that started over
 * (a reinstall reset, a deliberate sim reset) has none of that, and took what the world holds from the
 * world itself, so a record of the old engine journaled into it again could only apply a second time
 * what the world already shows. So the holder keeps the life its records were written in, and a holder
 * that loads into another life drops them unjournaled, and its op ids stop counting as applied.
 */
public record PendingRecords(long life, List<byte[]> records) {
    public static final PendingRecords EMPTY = new PendingRecords(0, List.of());

    private static final Codec<List<byte[]>> RECORDS = Codec.BYTE_BUFFER.listOf().xmap(
            list -> {
                List<byte[]> out = new ArrayList<>(list.size());
                for (ByteBuffer b : list) {
                    byte[] bytes = new byte[b.remaining()];
                    b.duplicate().get(bytes);
                    out.add(bytes);
                }
                return List.copyOf(out);
            },
            records -> {
                List<ByteBuffer> out = new ArrayList<>(records.size());
                for (byte[] r : records)
                    out.add(ByteBuffer.wrap(r));
                return out;
            });

    /** {life, records}; a plain list (written before the life existed) reads as life 0, an old page directory's. */
    public static final Codec<PendingRecords> CODEC = Codec.withAlternative(
            RecordCodecBuilder.create(i -> i.group(
                    Codec.LONG.fieldOf("life").forGetter(PendingRecords::life),
                    RECORDS.fieldOf("records").forGetter(PendingRecords::records)).apply(i, PendingRecords::new)),
            RECORDS.xmap(r -> new PendingRecords(0, r), PendingRecords::records));

    public boolean isEmpty() {
        return records.isEmpty();
    }

    /**
     * These records and {@code more}, written in engine life {@code life}. Records of another life are left out:
     * they would be dropped at the holder's next load anyway, and a holder carries one life.
     */
    public PendingRecords plus(long life, List<byte[]> more) {
        List<byte[]> out = new ArrayList<>(this.life == life ? records : List.of());
        out.addAll(more);
        return new PendingRecords(life, List.copyOf(out));
    }

    /** The same life with other records (what is left once some were acknowledged). */
    public PendingRecords with(List<byte[]> kept) {
        return new PendingRecords(life, List.copyOf(kept));
    }
}
