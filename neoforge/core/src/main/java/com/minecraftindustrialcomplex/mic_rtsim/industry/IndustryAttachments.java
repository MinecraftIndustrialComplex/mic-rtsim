package com.minecraftindustrialcomplex.mic_rtsim.industry;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.mojang.serialization.Codec;
import com.mojang.serialization.codecs.RecordCodecBuilder;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.attachment.AttachmentType;
import net.neoforged.neoforge.registries.DeferredRegister;
import net.neoforged.neoforge.registries.NeoForgeRegistries;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Optional;
import java.util.function.Supplier;

/** The industry attachments: what chunks and entities carry for the district sampler. */
public final class IndustryAttachments {
    private IndustryAttachments() {}

    private static final DeferredRegister<AttachmentType<?>> TYPES =
            DeferredRegister.create(NeoForgeRegistries.Keys.ATTACHMENT_TYPES, MicRtsim.MODID);

    /** The player-placed industrial block entities of a chunk, as {@code BlockPos.asLong} (I2's seeds). */
    public record Seeds(long[] positions) {
        public static final Seeds EMPTY = new Seeds(new long[0]);
        public static final Codec<Seeds> CODEC = Codec.LONG_STREAM.xmap(s -> new Seeds(s.toArray()), s -> Arrays.stream(s.positions));

        public boolean isEmpty() {
            return positions.length == 0;
        }

        public boolean contains(long pos) {
            for (long p : positions)
                if (p == pos)
                    return true;
            return false;
        }

        public Seeds plus(long pos) {
            if (contains(pos))
                return this;
            long[] out = Arrays.copyOf(positions, positions.length + 1);
            out[positions.length] = pos;
            return new Seeds(out);
        }

        public Seeds minus(long pos) {
            if (!contains(pos))
                return this;
            return new Seeds(Arrays.stream(positions).filter(p -> p != pos).toArray());
        }
    }

    /**
     * One district's reconcile state in a chunk (I8): the last applied epoch and, until the engine
     * acknowledges it, the IND_RECONCILE_RESULT that applied it. {@code life} is the engine life that
     * issued the epoch ({@code RtsimSession.engineLife}): an engine whose pages are new numbers its epochs
     * from 1 again, so an entry of another life says nothing about this engine's epochs (0 when the entry
     * predates the field).
     */
    public record EpochEntry(long district, int epoch, Optional<byte[]> pendingResult, long life) {
        public static final Codec<EpochEntry> CODEC = RecordCodecBuilder.create(i -> i.group(
                Codec.LONG.fieldOf("district").forGetter(EpochEntry::district),
                Codec.INT.fieldOf("epoch").forGetter(EpochEntry::epoch),
                Codec.BYTE_BUFFER.optionalFieldOf("result").forGetter(e -> e.pendingResult.map(ByteBuffer::wrap)),
                Codec.LONG.optionalFieldOf("life", 0L).forGetter(EpochEntry::life)
        ).apply(i, (d, e, r, l) -> new EpochEntry(d, e, r.map(b -> {
            byte[] bytes = new byte[b.remaining()];
            b.duplicate().get(bytes);
            return bytes;
        }), l)));
    }

    /** {@code mic_rtsim:reconcile}: per district, the epoch applied in this chunk (I8). */
    public record Epochs(List<EpochEntry> entries) {
        public static final Epochs EMPTY = new Epochs(List.of());
        public static final Codec<Epochs> CODEC = EpochEntry.CODEC.listOf().xmap(Epochs::new, Epochs::entries);

        public boolean isEmpty() {
            return entries.isEmpty();
        }

        public EpochEntry get(long district) {
            for (EpochEntry e : entries)
                if (e.district == district)
                    return e;
            return null;
        }

        /** The district's applied epoch, whatever life issued it (what a test reads back). */
        public int epoch(long district) {
            EpochEntry e = get(district);
            return e == null ? 0 : e.epoch;
        }

        /** The district's applied epoch as engine life {@code life} numbers them: 0 for an entry of another life. */
        public int epoch(long district, long life) {
            EpochEntry e = get(district);
            return e == null || e.life != life ? 0 : e.epoch;
        }

        /** Whether an entry is of another engine life than {@code life}. */
        public boolean hasStale(long life) {
            for (EpochEntry e : entries)
                if (e.life != life)
                    return true;
            return false;
        }

        /** The entries of engine life {@code life} only. */
        public Epochs ofLife(long life) {
            List<EpochEntry> out = new ArrayList<>(entries.size());
            for (EpochEntry e : entries)
                if (e.life == life)
                    out.add(e);
            return new Epochs(List.copyOf(out));
        }

        public Epochs with(EpochEntry entry) {
            List<EpochEntry> out = new ArrayList<>(entries.size() + 1);
            for (EpochEntry e : entries)
                if (e.district != entry.district)
                    out.add(e);
            out.add(entry);
            return new Epochs(List.copyOf(out));
        }
    }

    /** {@code mic_rtsim:industry_seeds}: saved with the chunk. */
    public static final Supplier<AttachmentType<Seeds>> SEEDS = TYPES.register("industry_seeds",
            () -> AttachmentType.builder(() -> Seeds.EMPTY).serialize(Seeds.CODEC, s -> !s.isEmpty()).build());

    /**
     * {@code mic_rtsim:reconcile}: saved with the chunk, in the same NBT as its block entities
     * ({@code neoforge:attachments}), so "epoch e applied" and the contents it changed reach disk together.
     */
    public static final Supplier<AttachmentType<Epochs>> RECONCILE = TYPES.register("reconcile",
            () -> AttachmentType.builder(() -> Epochs.EMPTY).serialize(Epochs.CODEC, e -> !e.isEmpty()).build());

    /**
     * {@code mic_rtsim:ported}: an item entity whose items already crossed the boundary as a port (the
     * drops of a stock block a player broke), so picking it up is not counted again. Never saved.
     */
    public static final Supplier<AttachmentType<Boolean>> PORTED = TYPES.register("ported",
            () -> AttachmentType.builder(() -> Boolean.FALSE).build());

    static void register(IEventBus modBus) {
        TYPES.register(modBus);
    }
}
