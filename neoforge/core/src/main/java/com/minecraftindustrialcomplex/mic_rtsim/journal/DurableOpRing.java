package com.minecraftindustrialcomplex.mic_rtsim.journal;

import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.concurrent.atomic.AtomicLong;

/**
 * The engine ops applied to one SavedData store, kept in the store's own file beside their effect (slice U,
 * docs/choices-journal-barrier.md RP-204): the dedup of an op whose effect lives outside the journal's file is
 * "this ring, or the journal". The journal's file is written last in a world save (RP-200), so a crash can leave a
 * store newer than the journal on disk; the store's ring then still knows the op, and the engine's resend of it is
 * answered DUPLICATE_OP instead of being applied a second time. Pure Java (the unit tests touch no game class); the
 * NBT and the session's marks are {@link DurableOpRings}'.
 *
 * <p><b>Entries</b> are (op id, sequence of the COMMAND_RESULT that answered it), in the order they came. An entry is
 * needed until the journal on disk holds that result: then the journal is the dedup, and once the engine has
 * acknowledged the result it never sends the op again. For an APPLIED answer the journal's dedup is its applied
 * index; for an entry stamped by a DUPLICATE_OP answer, which that index does not count, it is the resend's own
 * command sequence, at or below the journal's last applied one, which {@code BoundarySession} skips as stale. So the
 * store's save trims by the durable mark ({@link #trim}): an entry whose result is at or below the sequence the
 * journal's file holds goes.
 *
 * <p><b>Entries read from disk</b> carry sequences of the life that wrote them. Those at or below the journal's file
 * as it was read at this start ({@code durableAtLoad}) were held by that file: they go at the first trim. The others
 * are what a crash between the store's write and the journal's left: their sequences are handed out again in this
 * life, so they wait as {@link #PENDING} until the engine sends the op again and the answer stamps them with its
 * own sequence. One the engine never sends again stays until the cap pushes it out.
 *
 * <p><b>The cap</b> ({@value #DEFAULT_CAP}, TUNABLE): the oldest entry goes first. Pushing out one the journal on
 * disk does not hold yet is an overflow, counted ({@link #overflowed}), since its resend could then be applied twice.
 *
 * <p>Server thread only.
 */
public final class DurableOpRing {
    /** The key under which a store's tag keeps its ring. */
    public static final String KEY = "mic_rtsim:applied_ops";
    /** Entries kept at most. TUNABLE, PLACEHOLDER 4096: a store's entries live about two autosaves. */
    public static final int DEFAULT_CAP = 4096;
    /** The sequence of an entry read from disk that the journal's file did not hold: waits for its op's resend. */
    public static final long PENDING = Long.MAX_VALUE;

    private static final AtomicLong OVERFLOWED = new AtomicLong();

    private static final class Entry {
        long seq;
        boolean fromDisk;

        Entry(long seq, boolean fromDisk) {
            this.seq = seq;
            this.fromDisk = fromDisk;
        }
    }

    private final int cap;
    private final LinkedHashMap<Long, Entry> entries = new LinkedHashMap<>();
    /** The journal's durable mark at the last trim: entries above it are not on disk in the journal. */
    private long mark;
    private long overflowed;
    private Runnable onChange = () -> {};

    public DurableOpRing() {
        this(DEFAULT_CAP);
    }

    public DurableOpRing(int cap) {
        if (cap < 1)
            throw new IllegalArgumentException("cap " + cap);
        this.cap = cap;
    }

    /** A ring read from a store's file: {@code ops[i]} was answered at {@code seqs[i]} in the life that wrote it. */
    public static DurableOpRing fromDisk(long[] ops, long[] seqs, int cap) {
        if (ops.length != seqs.length)
            throw new IllegalArgumentException(ops.length + " ops, " + seqs.length + " sequences");
        DurableOpRing r = new DurableOpRing(cap);
        for (int i = 0; i < ops.length; i++)
            r.entries.put(ops[i], new Entry(seqs[i], true));
        while (r.entries.size() > cap)
            r.evictOldest();
        return r;
    }

    /**
     * A ring whose file kept its ops and lost their sequences (corrupt): every op waits for its resend, as
     * {@link #PENDING}. A resend is then refused: an effect left out, never one applied twice.
     */
    public static DurableOpRing allPending(long[] ops, int cap) {
        long[] pending = new long[ops.length];
        java.util.Arrays.fill(pending, PENDING);
        return fromDisk(ops, pending, cap);
    }

    /**
     * The key of one part of an op, for a store an op writes more than once (a reconcile's deposits, one per good):
     * the op id mixed with the part's name (FNV-1a, then a 64-bit finalizer).
     */
    public static long keyOf(long op, String part) {
        long h = 0xcbf29ce484222325L;
        for (int i = 0; i < part.length(); i++) {
            h ^= part.charAt(i);
            h *= 0x100000001b3L;
        }
        long x = op ^ Long.rotateLeft(h, 29);
        x ^= x >>> 33;
        x *= 0xff51afd7ed558ccdL;
        x ^= x >>> 33;
        x *= 0xc4ceb9fe1a85ec53L;
        x ^= x >>> 33;
        return x;
    }

    /** Runs when an entry is added: the store's {@code setDirty}. */
    public void onChange(Runnable onChange) {
        this.onChange = onChange;
    }

    public boolean contains(long op) {
        return entries.containsKey(op);
    }

    /**
     * {@code op} was answered at journal sequence {@code seq} (APPLIED, or DUPLICATE_OP on a hit of this ring the
     * journal did not know): kept until the journal on disk holds that answer. A known op is stamped again and moves
     * to the newest end.
     */
    public void add(long op, long seq) {
        entries.remove(op);
        entries.put(op, new Entry(seq, false));
        while (entries.size() > cap)
            evictOldest();
        onChange.run();
    }

    private void evictOldest() {
        Iterator<Map.Entry<Long, Entry>> it = entries.entrySet().iterator();
        Entry e = it.next().getValue();
        it.remove();
        if (e.fromDisk || e.seq > mark) {
            overflowed++;
            OVERFLOWED.incrementAndGet();
        }
    }

    /**
     * At the store's save: drops the entries the journal's file holds. {@code writtenThrough} is the last sequence
     * the journal's file on disk holds now, {@code durableAtLoad} the last one it held when this start read it.
     * Returns the entries dropped.
     */
    public int trim(long writtenThrough, long durableAtLoad) {
        mark = Math.max(mark, writtenThrough);
        int dropped = 0;
        for (Iterator<Entry> it = entries.values().iterator(); it.hasNext(); ) {
            Entry e = it.next();
            if (e.fromDisk) {
                if (e.seq <= durableAtLoad) {
                    it.remove();
                    dropped++;
                } else {
                    e.seq = PENDING;
                    e.fromDisk = false;
                }
            } else if (e.seq <= mark) {
                it.remove();
                dropped++;
            }
        }
        return dropped;
    }

    public int size() {
        return entries.size();
    }

    /** Entries waiting for their op's resend ({@link #PENDING}). */
    public int pending() {
        int n = 0;
        for (Entry e : entries.values())
            if (e.seq == PENDING)
                n++;
        return n;
    }

    /** The op ids, oldest first: what the store's file keeps, with {@link #seqs}. */
    public long[] ops() {
        long[] out = new long[entries.size()];
        int i = 0;
        for (Long op : entries.keySet())
            out[i++] = op;
        return out;
    }

    /** The sequences, in the order of {@link #ops}. */
    public long[] seqs() {
        long[] out = new long[entries.size()];
        int i = 0;
        for (Entry e : entries.values())
            out[i++] = e.seq;
        return out;
    }

    /** Entries pushed out by the cap before the journal on disk held them (this ring). */
    public long overflowed() {
        return overflowed;
    }

    /** {@link #overflowed} over every ring since the server started (rtsim perf). */
    public static long overflowedTotal() {
        return OVERFLOWED.get();
    }
}
