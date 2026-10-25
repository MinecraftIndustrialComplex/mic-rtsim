package com.minecraftindustrialcomplex.mic_rtsim.journal;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.Tag;
import net.neoforged.neoforge.attachment.IAttachmentHolder;
import net.neoforged.neoforge.attachment.IAttachmentSerializer;
import org.jetbrains.annotations.Nullable;

/**
 * A {@link DurableOpRing} in NBT: {@code ops} and {@code seqs}, two long arrays, oldest first; in a store's own tag
 * under {@link DurableOpRing#KEY}, or as a level attachment ({@link #SERIALIZER}). A store without it reads as an
 * empty ring; a store's own loader that does not know the key ignores it, and its next save drops it (UR-1:
 * uninstalling rtsim leaves nothing that matters). An empty ring writes nothing.
 */
public final class DurableOpRings {
    private DurableOpRings() {}

    /**
     * Writes {@code ring} into a store's tag at the store's save (server thread), first dropping what the journal's
     * file on disk holds.
     */
    public static void write(DurableOpRing ring, CompoundTag storeTag) {
        CompoundTag t = toTag(ring);
        if (t != null)
            storeTag.put(DurableOpRing.KEY, t);
    }

    /** The ring a store's tag holds, read from disk; empty when it has none. */
    public static DurableOpRing read(CompoundTag storeTag) {
        return storeTag.contains(DurableOpRing.KEY, Tag.TAG_COMPOUND) ? fromTag(storeTag.getCompound(DurableOpRing.KEY)) : new DurableOpRing();
    }

    /**
     * The ring's own tag at its store's save, trimmed by the session's journal first (nothing is dropped without a
     * session); null when it is empty. Written whatever the industry module's switch, unlike the train and port
     * rings (docs/choices-journal-barrier.md RP-204, "The module switch"): a save that dropped it after a switch off
     * could die before the journal's write, and the op applied before the switch would be applied again.
     */
    public static @Nullable CompoundTag toTag(DurableOpRing ring) {
        RtsimSession s = RtsimSession.current();
        if (s != null) {
            JournalSavedData j = s.journal();
            ring.trim(j.writtenThrough(), j.durableAtLoad());
        }
        if (ring.size() == 0)
            return null;
        CompoundTag t = new CompoundTag();
        t.putLongArray("ops", ring.ops());
        t.putLongArray("seqs", ring.seqs());
        return t;
    }

    /** A ring read from disk from its own tag. */
    public static DurableOpRing fromTag(CompoundTag t) {
        long[] ops = t.getLongArray("ops"), seqs = t.getLongArray("seqs");
        if (ops.length != seqs.length) {
            // Corrupt: keep every op, each pending. A resend is then refused: an effect left out (for a deposit's draw,
            // ore the world keeps), never one applied twice.
            MicRtsim.LOGGER.error("[mic_rtsim] a store's applied ops are corrupt ({} ops, {} sequences): each is kept as pending", ops.length, seqs.length);
            return DurableOpRing.allPending(ops, DurableOpRing.DEFAULT_CAP);
        }
        return DurableOpRing.fromDisk(ops, seqs, DurableOpRing.DEFAULT_CAP);
    }

    /**
     * A ring as a level attachment ({@code mic_rtsim:applied_ops}, {@code RtsimAttachments.LEVEL_APPLIED_OPS}): NeoForge
     * saves a level's attachments together in that level's {@code neoforge_data_attachments}, so the ring is in the
     * same file as another mod's level attachment it guards (Destroy's level pollution). NeoForge always saves that
     * file, so the ring needs no dirty flag. A world without rtsim logs "unknown or non-serializable data attachment
     * mic_rtsim:applied_ops. Skipping." at the level's load and drops it at the next save (UR-1).
     */
    public static final IAttachmentSerializer<CompoundTag, DurableOpRing> SERIALIZER = new IAttachmentSerializer<>() {
        @Override
        public DurableOpRing read(IAttachmentHolder holder, CompoundTag tag, HolderLookup.Provider registries) {
            return fromTag(tag);
        }

        @Override
        public @Nullable CompoundTag write(DurableOpRing ring, HolderLookup.Provider registries) {
            return toTag(ring);
        }
    };
}
