package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.simibubi.create.content.trains.entity.Train;
import com.simibubi.create.content.trains.graph.DimensionPalette;
import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.Tag;
import net.minecraft.server.MinecraftServer;

import java.util.ArrayList;
import java.util.Map;
import java.util.UUID;

/**
 * A Create train's ring of applied IND_CARRIAGE_DELTA op ids in the train's own NBT (RP-190 to RP-194): the
 * key, the save and load hooks {@code ringmixin.TrainRingMixin} calls, and whether that mixin applied.
 *
 * <p>The ring is the dedup of {@link TrainWatch#carriageDelta} for a train that exists (RP-191). The goods
 * and the ring are written by the same {@code Train.write} into create_tracks.dat, so a crash keeps both or
 * neither: the journal's op index, a second SavedData file, made J3 a double and J4 a dupe (RP-184).
 *
 * <p>The mixin's config is optional ({@code mic_rtsim_create_ring.mixins.json}: not required, require 0),
 * so a Create that moved {@code Train.write} or {@code Train.read} loads without it. {@link #available}
 * then finds the round trip broken, logs that once and {@code carriageDelta} falls back to the journal's
 * op index, which is slice T's dedup (RP-193).
 */
public final class TrainRing {
    private TrainRing() {}

    /** The NBT key in a Train's compound: a long array of op ids, oldest first. */
    public static final String KEY = "mic_rtsim:applied_ops";
    /** The op the probe writes and reads back: an engine op id no engine hands out (top bit set, F4). */
    private static final long PROBE_OP = 0xFFFF_FFFF_FFFF_FFF1L;

    /** Null until the first {@link #available} after start or a test switch; then the answer. */
    private static Boolean available;
    private static boolean fallbackForTest;
    private static boolean probing;
    private static int fallbackLogs;

    /**
     * True when the ring mixin applied: {@code Train} implements {@link AppliedOpsHolder} and a probe train's
     * ring survives a {@code Train.write} then {@code Train.read}. Probed once, on the first call.
     */
    public static boolean available(MinecraftServer server) {
        if (available == null) {
            String why = fallbackForTest ? "switched off by a GameTest" : probe(server.registryAccess());
            available = why == null;
            if (why != null) {
                fallbackLogs++;
                MicRtsim.LOGGER.warn("[mic_rtsim_create] the train op ring is off ({}): IND_CARRIAGE_DELTA's dedup is the "
                        + "journal's op index, which repeats a train op after a crash that kept the goods file and not the journal's "
                        + "(docs/choices-mowzie-build.md RP-193)", why);
            } else {
                MicRtsim.LOGGER.info("[mic_rtsim_create] the train op ring is on: IND_CARRIAGE_DELTA's dedup is each train's {}", KEY);
            }
        }
        return available;
    }

    /** The ring of a train, made on first use; null when the mixin did not apply. */
    public static AppliedOpRing of(Train train) {
        return (Object) train instanceof AppliedOpsHolder h ? h.mic_rtsim$appliedOps() : null;
    }

    /**
     * {@code Train.write}'s hook: the ring under {@link #KEY}, when it holds an op and the industry module is
     * on. With the module off the key is left out, so the next save drops it (UR-1); the ring itself stays
     * on the train until the server stops.
     */
    public static void write(Object train, CompoundTag tag) {
        if (!(train instanceof AppliedOpsHolder h) || tag == null)
            return;
        AppliedOpRing ring = h.mic_rtsim$appliedOpsIfAny();
        if (ring == null || ring.isEmpty())
            return;
        if (!probing && !RtsimConfig.on(Module.INDUSTRY))
            return;
        tag.putLongArray(KEY, ring.toArray());
    }

    /** {@code Train.read}'s hook: the saved ring onto the train Create made. */
    public static void read(Object train, CompoundTag tag) {
        if (!(train instanceof AppliedOpsHolder h) || tag == null || !tag.contains(KEY, Tag.TAG_LONG_ARRAY))
            return;
        h.mic_rtsim$appliedOps().load(tag.getLongArray(KEY));
    }

    /** Null when the round trip works, else what broke. */
    private static String probe(HolderLookup.Provider registries) {
        try {
            Train train = new Train(UUID.randomUUID(), null, null, new ArrayList<>(), new ArrayList<>(), false, 0);
            AppliedOpRing ring = of(train);
            if (ring == null)
                return "Train does not implement AppliedOpsHolder: the mixin config did not apply";
            ring.add(PROBE_OP);
            CompoundTag tag;
            probing = true;
            try {
                tag = train.write(new DimensionPalette(), registries);
            } finally {
                probing = false;
            }
            long[] saved = tag.getLongArray(KEY);
            if (saved.length != 1 || saved[0] != PROBE_OP)
                return "Train.write wrote no " + KEY + ": its injection did not apply";
            AppliedOpRing back = of(Train.read(tag, registries, Map.of(), new DimensionPalette()));
            if (back == null || !back.contains(PROBE_OP))
                return "Train.read did not read " + KEY + ": its injection did not apply";
            return null;
        } catch (RuntimeException | LinkageError e) {
            return "the probe threw " + e;
        }
    }

    // ---- GameTests ----------------------------------------------------------------------------

    /**
     * Switches the ring off (true) as if the mixin had not applied, or back to the probe (false). A change
     * probes again at the next {@link #available}, and logs again when off.
     */
    public static void fallbackForTest(boolean on) {
        if (fallbackForTest == on)
            return;
        fallbackForTest = on;
        available = null;
    }

    /** A train's ring without making one: null when it never had one or the mixin did not apply. */
    public static AppliedOpRing peekForTest(Train train) {
        return (Object) train instanceof AppliedOpsHolder h ? h.mic_rtsim$appliedOpsIfAny() : null;
    }

    /** How many times the fallback was logged: once per probe that failed. */
    public static int fallbackLogsForTest() {
        return fallbackLogs;
    }
}
