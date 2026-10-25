package com.minecraftindustrialcomplex.mic_rtsim_create;

import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.Tag;

import java.util.Arrays;

/**
 * A bounded set of applied engine op ids, oldest first, saved as one long array under a namespaced key
 * in any {@link CompoundTag} (docs/choices-mowzie-build.md RP-190, RP-198). The dedup of an op whose
 * effect is saved in a SavedData other than the journal's: kept in the same compound as the effect, it
 * is saved by the same write, so whichever save holds the effect holds the op (a crash keeps both or
 * neither). It knows nothing of its owner. A Create train's ring is {@link TrainRing} (capacity
 * {@link TrainRing#CAPACITY}, key {@link TrainRing#KEY}); a station port's two rings are {@link PortRing}
 * (slice U, docs/choices-journal-barrier.md RP-206), beside its offline buffer, also in create_tracks.dat.
 *
 * <p>{@link #add} skips an op already held (a resend) and, when full, drops the oldest; the capacity is
 * the owner's bound. Only {@link #writeTo}, {@link #readFrom} and {@link #savedIn} touch Minecraft (the
 * NBT classes), so the unit tests run the rest without a server.
 */
public final class AppliedOpRing {
    private final int capacity;
    private long[] ops = new long[0];
    /** The oldest op's slot. */
    private int start;
    private int size;

    /** An empty ring holding at most {@code capacity} ops (at least 1). */
    public AppliedOpRing(int capacity) {
        if (capacity < 1)
            throw new IllegalArgumentException("capacity " + capacity);
        this.capacity = capacity;
    }

    /** The cap: past it, {@link #add} drops the oldest op. */
    public int capacity() {
        return capacity;
    }

    public int size() {
        return size;
    }

    public boolean isEmpty() {
        return size == 0;
    }

    public boolean contains(long op) {
        for (int i = 0; i < size; i++)
            if (ops[(start + i) % ops.length] == op)
                return true;
        return false;
    }

    /**
     * Adds an op; a full ring drops its oldest. An op already held is not added twice and keeps its
     * place. True when the op was added, false when the ring held it.
     */
    public boolean add(long op) {
        if (contains(op))
            return false;
        if (size == capacity) {
            // Full: ops.length == capacity, and the oldest slot takes the new op.
            ops[start] = op;
            start = (start + 1) % ops.length;
            return true;
        }
        if (size == ops.length)
            grow();
        ops[(start + size) % ops.length] = op;
        size++;
        return true;
    }

    public void clear() {
        ops = new long[0];
        start = 0;
        size = 0;
    }

    /** The ops, oldest first: what the NBT long array holds. */
    public long[] toArray() {
        long[] out = new long[size];
        for (int i = 0; i < size; i++)
            out[i] = ops[(start + i) % ops.length];
        return out;
    }

    /** Replaces the ring with saved ops, oldest first; past the capacity the newest are kept. */
    public void load(long[] saved) {
        clear();
        for (long op : saved)
            add(op);
    }

    // ---- NBT ----------------------------------------------------------------------------------

    /**
     * Saves the ring under {@code key} as a long array, oldest first; an empty ring removes the key, so
     * the compound then holds only its owner's keys. {@code key} is namespaced ({@code "mod:name"}), so it
     * cannot meet a key of the owner's.
     */
    public void writeTo(CompoundTag tag, String key) {
        checkKey(key);
        if (isEmpty())
            tag.remove(key);
        else
            tag.putLongArray(key, toArray());
    }

    /** True when {@code tag} holds a saved ring under {@code key} (a long array). */
    public static boolean savedIn(CompoundTag tag, String key) {
        checkKey(key);
        return tag.contains(key, Tag.TAG_LONG_ARRAY);
    }

    /**
     * Replaces the ring with the one saved under {@code key}, keeping the newest past the capacity. A
     * compound with no ring there (or something else under the key) leaves the ring empty: false.
     */
    public boolean readFrom(CompoundTag tag, String key) {
        if (!savedIn(tag, key)) {
            clear();
            return false;
        }
        load(tag.getLongArray(key));
        return true;
    }

    private static void checkKey(String key) {
        if (key == null || key.indexOf(':') < 1 || key.indexOf(':') == key.length() - 1)
            throw new IllegalArgumentException("not a namespaced key: " + key);
    }

    private void grow() {
        long[] next = new long[Math.min(capacity, Math.max(8, ops.length * 2))];
        for (int i = 0; i < size; i++)
            next[i] = ops[(start + i) % ops.length];
        ops = next;
        start = 0;
    }

    @Override
    public String toString() {
        return "AppliedOpRing" + Arrays.toString(toArray());
    }
}
