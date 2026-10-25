package com.minecraftindustrialcomplex.mic_rtsim.journal;

import java.util.ArrayList;
import java.util.Collections;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.WeakHashMap;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.Consumer;

/**
 * The engine ops applied to one SavedData store, kept in the store's own file beside their effect (slice U,
 * docs/choices-journal-barrier.md RP-204): the dedup of an op whose effect lives outside the journal's file is
 * "this ring, or the journal". The journal's file is written last in a world save (RP-200), so a crash can leave a
 * store newer than the journal on disk; the store's ring then still knows the op, and the engine's resend of it is
 * answered DUPLICATE_OP instead of being applied a second time. Pure Java (the unit tests touch no game class); the
 * NBT, the session's acknowledgements and its settle are {@link DurableOpRings}'.
 *
 * <p><b>Entries</b> are (op id, sequence of the COMMAND_RESULT that answered it last, or {@link #UNKNOWN}), in the
 * order they came; an op answered again moves to the newest end. An entry stays until the engine can no longer send
 * the op again, T2's rule (docs/choices-mowzie-build.md RP-250, RP-251): the engine's acknowledgement of the answer's
 * sequence, its durable save holding the answer, removes it ({@link #pruneIfAnsweredAt}); and a ring another session
 * answered is settled at this session's start against the engine's pages ({@link #settle}): an op leaves only when
 * the pages provably hold its answer, else it stays with its answer unknown until the engine sends it again and the
 * refusal's answer stamps it. The ring keeps the generation ({@link #gen}) of the session that settled it last.
 *
 * <p><b>The cap</b> ({@value #DEFAULT_CAP}, a valve): the oldest entry goes first. Every entry is one the engine has
 * not acknowledged, so pushing one out is an overflow, counted ({@link #overflowed}): its resend could then be
 * applied twice.
 *
 * <p>Server thread only.
 */
public final class DurableOpRing {
    /** The key under which a store's tag keeps its ring. */
    public static final String KEY = "mic_rtsim:applied_ops";
    /** Entries kept at most: a valve, as T2's train ring's (RP-250). At most 64 KiB of ids and as many sequences. */
    public static final int DEFAULT_CAP = 4096;
    /** An op's answer that no durable save of the engine is known to hold. */
    public static final long UNKNOWN = 0;

    private static final AtomicLong OVERFLOWED = new AtomicLong();
    /** Every ring alive, for the session's settle and the industry's announce ({@link DurableOpRings}). */
    private static final Set<DurableOpRing> LIVE = Collections.newSetFromMap(new WeakHashMap<>());

    private final int cap;
    /** Op to the sequence of its last answer ({@link #UNKNOWN}: none known), oldest first. */
    private final LinkedHashMap<Long, Long> entries = new LinkedHashMap<>();
    private long gen;
    private long overflowed;
    private Runnable onChange = () -> {};

    public DurableOpRing() {
        this(DEFAULT_CAP);
    }

    public DurableOpRing(int cap) {
        if (cap < 1)
            throw new IllegalArgumentException("cap " + cap);
        this.cap = cap;
        synchronized (LIVE) {
            LIVE.add(this);
        }
    }

    /**
     * A ring read from a store's file: {@code ops[i]} was answered at {@code seqs[i]} by the session of generation
     * {@code gen} (0: none), which settled it last.
     */
    public static DurableOpRing fromDisk(long[] ops, long[] seqs, long gen, int cap) {
        if (ops.length != seqs.length)
            throw new IllegalArgumentException(ops.length + " ops, " + seqs.length + " sequences");
        DurableOpRing r = new DurableOpRing(cap);
        for (int i = 0; i < ops.length; i++) {
            r.entries.remove(ops[i]);
            r.entries.put(ops[i], Math.max(UNKNOWN, seqs[i]));
        }
        while (r.entries.size() > cap)
            r.entries.remove(r.entries.keySet().iterator().next());
        r.gen = gen;
        return r;
    }

    /**
     * A ring whose file kept its ops and lost their sequences (corrupt): every op's answer is {@link #UNKNOWN}, so
     * no settle drops it and it waits for its resend. A resend is then refused: an effect left out, never one applied
     * twice.
     */
    public static DurableOpRing allUnknown(long[] ops, int cap) {
        return fromDisk(ops, new long[ops.length], 0, cap);
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

    /** Runs when the ring changes (an op answered or gone): the store's {@code setDirty}. */
    public void onChange(Runnable onChange) {
        this.onChange = onChange;
    }

    /** The store's {@code setDirty}, for a change {@link DurableOpRings} made (a settle). */
    void changed() {
        onChange.run();
    }

    public boolean contains(long op) {
        return entries.containsKey(op);
    }

    /**
     * {@code op} was answered at journal sequence {@code seq} (APPLIED, or DUPLICATE_OP on a hit of this ring the
     * journal did not know; {@link #UNKNOWN} for none known): kept until the engine acknowledges that answer. A known
     * op is stamped again and moves to the newest end.
     */
    public void add(long op, long seq) {
        entries.remove(op);
        entries.put(op, Math.max(UNKNOWN, seq));
        while (entries.size() > cap)
            evictOldest();
        onChange.run();
    }

    private void evictOldest() {
        Iterator<Long> it = entries.keySet().iterator();
        it.next();
        it.remove();
        overflowed++;
        OVERFLOWED.incrementAndGet();
    }

    /** The sequence of an op's last answer, {@link #UNKNOWN} when none is known, or -1 when the ring lacks the op. */
    public long answerOf(long op) {
        Long at = entries.get(op);
        return at == null ? -1 : at;
    }

    /**
     * Removes an op whose last answer is still the one at {@code at}: the engine's durable save holds that answer. An
     * op answered again since, made {@link #UNKNOWN} or gone stays as it is: false.
     */
    public boolean pruneIfAnsweredAt(long op, long at) {
        Long now = entries.get(op);
        if (now == null || now != at || at == UNKNOWN)
            return false;
        entries.remove(op);
        onChange.run();
        return true;
    }

    /** Every op's answer becomes {@link #UNKNOWN}; returns how many had one. */
    public int forgetAnswers() {
        int n = 0;
        for (Map.Entry<Long, Long> e : entries.entrySet())
            if (e.getValue() != UNKNOWN) {
                e.setValue(UNKNOWN);
                n++;
            }
        return n;
    }

    /**
     * Settles a ring another session answered against what the engine's pages hold: an op answered at or below
     * {@code bound} leaves, and every other op's answer becomes {@link #UNKNOWN}. A bound of 0 or less proves
     * nothing: every op stays, unknown. Returns how many left.
     */
    public int settle(long bound) {
        int away = 0;
        for (Iterator<Map.Entry<Long, Long>> it = entries.entrySet().iterator(); it.hasNext(); ) {
            Map.Entry<Long, Long> e = it.next();
            long at = e.getValue();
            if (bound > 0 && at != UNKNOWN && at <= bound) {
                it.remove();
                away++;
            } else {
                e.setValue(UNKNOWN);
            }
        }
        return away;
    }

    /** Calls {@code each} with every op that has a known answer, oldest first. */
    public void forEachAnswered(AnswerConsumer each) {
        for (Map.Entry<Long, Long> e : new ArrayList<>(entries.entrySet()))
            if (e.getValue() != UNKNOWN)
                each.accept(e.getKey(), e.getValue());
    }

    /** An op and the sequence of its answer. */
    @FunctionalInterface
    public interface AnswerConsumer {
        void accept(long op, long at);
    }

    /** The generation of the session that last settled the ring (0: none). */
    public long gen() {
        return gen;
    }

    public void gen(long gen) {
        this.gen = gen;
    }

    public int size() {
        return entries.size();
    }

    /** Entries whose answer is {@link #UNKNOWN}: they wait for their op's resend. */
    public int unknownCount() {
        int n = 0;
        for (long at : entries.values())
            if (at == UNKNOWN)
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
        for (Long at : entries.values())
            out[i++] = at;
        return out;
    }

    /** Entries pushed out by the cap (this ring): each an op the engine had not acknowledged. */
    public long overflowed() {
        return overflowed;
    }

    /** {@link #overflowed} over every ring since the server started (rtsim perf). */
    public static long overflowedTotal() {
        return OVERFLOWED.get();
    }

    /** Calls {@code each} with every ring alive now (a copy of the set, so {@code each} may make rings). */
    static void forEachLive(Consumer<DurableOpRing> each) {
        List<DurableOpRing> rings;
        synchronized (LIVE) {
            rings = new ArrayList<>(LIVE);
        }
        for (DurableOpRing r : rings)
            each.accept(r);
    }
}
