package com.minecraftindustrialcomplex.mic_rtsim.puppet;

import com.mojang.serialization.Codec;

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
 */
public record PendingRecords(List<byte[]> records) {
    public static final PendingRecords EMPTY = new PendingRecords(List.of());

    public static final Codec<PendingRecords> CODEC = Codec.BYTE_BUFFER.listOf().xmap(
            list -> {
                List<byte[]> out = new ArrayList<>(list.size());
                for (ByteBuffer b : list) {
                    byte[] bytes = new byte[b.remaining()];
                    b.duplicate().get(bytes);
                    out.add(bytes);
                }
                return new PendingRecords(List.copyOf(out));
            },
            p -> {
                List<ByteBuffer> out = new ArrayList<>(p.records().size());
                for (byte[] r : p.records())
                    out.add(ByteBuffer.wrap(r));
                return out;
            });

    public boolean isEmpty() {
        return records.isEmpty();
    }

    public PendingRecords plus(List<byte[]> more) {
        List<byte[]> out = new ArrayList<>(records);
        out.addAll(more);
        return new PendingRecords(List.copyOf(out));
    }
}
