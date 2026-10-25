package com.minecraftindustrialcomplex.mic_rtsim_create;

import java.util.Arrays;

/**
 * The op ids of the last {@link #CAPACITY} IND_CARRIAGE_DELTA commands applied to one Create train, oldest
 * first (docs/choices-mowzie-build.md RP-190 to RP-192). It is saved in the train's own NBT, in
 * create_tracks.dat beside the carriage goods, so whichever save holds the goods holds the ops that moved
 * them. Pure Java (no Minecraft class), so the unit tests run it without a server.
 */
public final class AppliedOpRing {
    /**
     * The bound (RP-192): one train takes at most one station exchange per simulated arrival and a few
     * loot ops per raid cooldown, and the ring must cover what it takes between the engine's last durable
     * save and Create's (one autosave, up to four while the chunk barrier holds the journal back: 24,000
     * ticks). 256 covers an arrival every 94 ticks over that window, at most 2 KiB of NBT per train.
     */
    public static final int CAPACITY = 256;

    private final int capacity;
    private long[] ops = new long[0];
    /** The oldest op's slot. */
    private int start;
    private int size;

    public AppliedOpRing() {
        this(CAPACITY);
    }

    AppliedOpRing(int capacity) {
        if (capacity < 1)
            throw new IllegalArgumentException("capacity " + capacity);
        this.capacity = capacity;
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

    /** Adds an op; a full ring drops its oldest. An op already held is not added twice. */
    public void add(long op) {
        if (contains(op))
            return;
        if (size == capacity) {
            // Full: ops.length == capacity, and the oldest slot takes the new op.
            ops[start] = op;
            start = (start + 1) % ops.length;
            return;
        }
        if (size == ops.length)
            grow();
        ops[(start + size) % ops.length] = op;
        size++;
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
        ops = new long[0];
        start = 0;
        size = 0;
        for (long op : saved)
            add(op);
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
