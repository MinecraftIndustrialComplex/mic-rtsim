package com.minecraftindustrialcomplex.mic_rtsim.journal;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.Tag;

/**
 * A {@link DurableOpRing} in a store's NBT, under {@link DurableOpRing#KEY}: {@code ops} and {@code seqs}, two long
 * arrays, oldest first. A store without the key reads as an empty ring; a store's own loader that does not know the
 * key ignores it, and its next save drops it (UR-1: uninstalling rtsim leaves nothing that matters).
 */
public final class DurableOpRings {
    private DurableOpRings() {}

    /**
     * Writes {@code ring} into the store's tag at the store's save (server thread), first dropping what the
     * journal's file on disk holds; an empty ring writes nothing. Without a session nothing is dropped.
     */
    public static void write(DurableOpRing ring, CompoundTag storeTag) {
        RtsimSession s = RtsimSession.current();
        if (s != null) {
            JournalSavedData j = s.journal();
            ring.trim(j.writtenThrough(), j.durableAtLoad());
        }
        if (ring.size() == 0)
            return; // nothing to keep: the store's file stays as its own mod writes it
        CompoundTag t = new CompoundTag();
        t.putLongArray("ops", ring.ops());
        t.putLongArray("seqs", ring.seqs());
        storeTag.put(DurableOpRing.KEY, t);
    }

    /** The ring a store's tag holds, read from disk; empty when it has none. */
    public static DurableOpRing read(CompoundTag storeTag) {
        if (!storeTag.contains(DurableOpRing.KEY, Tag.TAG_COMPOUND))
            return new DurableOpRing();
        CompoundTag t = storeTag.getCompound(DurableOpRing.KEY);
        long[] ops = t.getLongArray("ops"), seqs = t.getLongArray("seqs");
        if (ops.length != seqs.length) {
            // Corrupt: keep every op, each pending (a resend is then refused, a loss at worst, never a second draw).
            MicRtsim.LOGGER.error("[mic_rtsim] a store's applied ops are corrupt ({} ops, {} sequences): each is kept as pending", ops.length, seqs.length);
            long[] pending = new long[ops.length];
            java.util.Arrays.fill(pending, DurableOpRing.PENDING);
            return DurableOpRing.fromDisk(ops, pending, DurableOpRing.DEFAULT_CAP);
        }
        return DurableOpRing.fromDisk(ops, seqs, DurableOpRing.DEFAULT_CAP);
    }
}
