package com.minecraftindustrialcomplex.mic_rtsim_create;

import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.Tag;

import java.util.Arrays;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * A bounded set of applied engine op ids, oldest first, saved under a namespaced key in any
 * {@link CompoundTag} (docs/choices-mowzie-build.md RP-190, RP-198, RP-250). The dedup of an op whose effect
 * is saved in a SavedData other than the journal's: kept in the same compound as the effect, it is saved by
 * the same write, so whichever save holds the effect holds the op (a crash keeps both or neither). It knows
 * nothing of its owner. A Create train's ring is {@link TrainRing} (capacity {@link TrainRing#CAPACITY}, key
 * {@link TrainRing#KEY}).
 *
 * <p>Each op carries the sequence of the answer (COMMAND_RESULT) its owner journaled for it last in this
 * session, or {@link #UNKNOWN}: an op whose answer the engine's pages may lack, so the engine may send it
 * again. The owner removes an op once the engine's durable save holds its answer ({@link #pruneIfAnsweredAt})
 * and, at a session start, settles the ops another session answered ({@link #settle}); RP-250 says when
 * each is safe. The ring also keeps the generation ({@link #gen}) of the session that last settled it.
 *
 * <p>{@link #add} skips an op already held (a resend) and, when full, drops the oldest; the capacity is the
 * owner's bound, and {@link #evictions} counts what it dropped. Only {@link #writeTo}, {@link #readFrom} and
 * {@link #savedIn} touch Minecraft (the NBT classes), so the unit tests run the rest without a server.
 */
public final class AppliedOpRing {
    /** An op's answer that no durable save of the engine is known to hold. */
    public static final long UNKNOWN = 0;

    private final int capacity;
    /** Op to the sequence of its last answer ({@link #UNKNOWN}: none known), oldest op first. */
    private final LinkedHashMap<Long, Long> ops = new LinkedHashMap<>();
    private long gen;
    private long evictions;
    private long lastEvicted;

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
        return ops.size();
    }

    public boolean isEmpty() {
        return ops.isEmpty();
    }

    public boolean contains(long op) {
        return ops.containsKey(op);
    }

    /**
     * Adds an op, its answer {@link #UNKNOWN}; a full ring drops its oldest ({@link #evictions}). An op already
     * held is not added twice and keeps its place and answer. True when the op was added, false when the ring
     * held it.
     */
    public boolean add(long op) {
        if (ops.containsKey(op))
            return false;
        if (ops.size() == capacity) {
            Iterator<Long> oldest = ops.keySet().iterator();
            lastEvicted = oldest.next();
            oldest.remove();
            evictions++;
        }
        ops.put(op, UNKNOWN);
        return true;
    }

    /** The sequence of an op's last answer, {@link #UNKNOWN} when none is known, or -1 when the ring lacks the op. */
    public long answerOf(long op) {
        Long at = ops.get(op);
        return at == null ? -1 : at;
    }

    /** Records that the owner answered a held op at sequence {@code at} (positive). False when the ring lacks the op. */
    public boolean answered(long op, long at) {
        if (at <= UNKNOWN)
            throw new IllegalArgumentException("answer sequence " + at);
        if (!ops.containsKey(op))
            return false;
        ops.put(op, at);
        return true;
    }

    /**
     * Removes an op whose last answer is still the one at {@code at}: the engine's durable save holds that
     * answer. An op answered again since, settled to {@link #UNKNOWN} or gone stays as it is: false.
     */
    public boolean pruneIfAnsweredAt(long op, long at) {
        if (at <= UNKNOWN)
            return false;
        Long now = ops.get(op);
        if (now == null || now != at)
            return false;
        ops.remove(op);
        return true;
    }

    /** Every op's answer becomes {@link #UNKNOWN}; returns how many had one. */
    public int forgetAnswers() {
        int n = 0;
        for (Map.Entry<Long, Long> e : ops.entrySet())
            if (e.getValue() != UNKNOWN) {
                e.setValue(UNKNOWN);
                n++;
            }
        return n;
    }

    /**
     * Settles a ring another session answered against what the engine's pages hold: an op answered at or
     * below {@code bound} leaves, and every other op's answer becomes {@link #UNKNOWN} (an op already UNKNOWN
     * stays, UNKNOWN). A bound of 0 or less proves nothing: every op stays, UNKNOWN. Returns how many left.
     */
    public int settle(long bound) {
        int removed = 0;
        for (Iterator<Map.Entry<Long, Long>> it = ops.entrySet().iterator(); it.hasNext(); ) {
            Map.Entry<Long, Long> e = it.next();
            long at = e.getValue();
            if (at != UNKNOWN && at <= bound) {
                it.remove();
                removed++;
            } else {
                e.setValue(UNKNOWN);
            }
        }
        return removed;
    }

    /** How many ops have no known answer. */
    public int unknownCount() {
        int n = 0;
        for (long at : ops.values())
            if (at == UNKNOWN)
                n++;
        return n;
    }

    /** The generation of the session that last settled the ring (0: none). */
    public long gen() {
        return gen;
    }

    public void gen(long gen) {
        this.gen = gen;
    }

    /** The ops {@link #add} dropped for room since the ring was made. */
    public long evictions() {
        return evictions;
    }

    /** The op {@link #add} dropped last (meaningful once {@link #evictions} is above 0). */
    public long lastEvicted() {
        return lastEvicted;
    }

    public void clear() {
        ops.clear();
    }

    /** The ops, oldest first: what the NBT long array holds. */
    public long[] toArray() {
        long[] out = new long[ops.size()];
        int i = 0;
        for (long op : ops.keySet())
            out[i++] = op;
        return out;
    }

    /** Each op's answer ({@link #UNKNOWN}: none), in {@link #toArray}'s order. */
    public long[] answersArray() {
        long[] out = new long[ops.size()];
        int i = 0;
        for (long at : ops.values())
            out[i++] = at;
        return out;
    }

    /** Calls {@code each} with every op that has a known answer, oldest first. */
    public void forEachAnswered(AnswerConsumer each) {
        for (Map.Entry<Long, Long> e : ops.entrySet())
            if (e.getValue() != UNKNOWN)
                each.accept(e.getKey(), e.getValue());
    }

    /** An op and the sequence of its answer. */
    @FunctionalInterface
    public interface AnswerConsumer {
        void accept(long op, long at);
    }

    /** Replaces the ring with saved ops, oldest first, every answer {@link #UNKNOWN}; past the capacity the newest are kept. */
    public void load(long[] saved) {
        load(saved, null);
    }

    /**
     * Replaces the ring with saved ops and their answers ({@code at} in the same order, or null for none
     * known; a length that differs reads as none known); past the capacity the newest are kept. The ring's
     * {@link #evictions} are not counted for it.
     */
    public void load(long[] saved, long[] at) {
        clear();
        boolean answers = at != null && at.length == saved.length;
        for (int i = Math.max(0, saved.length - capacity); i < saved.length; i++)
            if (add(saved[i]) && answers && at[i] > UNKNOWN)
                ops.put(saved[i], at[i]);
    }

    // ---- NBT ----------------------------------------------------------------------------------

    /** The key of the answers' long array beside the ops' under {@code key}. */
    public static String answersKey(String key) {
        return key + "_at";
    }

    /** The key of the generation beside the ops' under {@code key}. */
    public static String genKey(String key) {
        return key + "_gen";
    }

    /**
     * Saves the ring under {@code key} as a long array of its ops, oldest first; beside it, under
     * {@link #answersKey}, the answers in the same order when any is known, and under {@link #genKey} the
     * generation when it is set. An empty ring removes all three keys, so the compound then holds only its
     * owner's keys. {@code key} is namespaced ({@code "mod:name"}), so none can meet a key of the owner's.
     */
    public void writeTo(CompoundTag tag, String key) {
        checkKey(key);
        if (isEmpty()) {
            tag.remove(key);
            tag.remove(answersKey(key));
            tag.remove(genKey(key));
            return;
        }
        tag.putLongArray(key, toArray());
        if (unknownCount() < size())
            tag.putLongArray(answersKey(key), answersArray());
        else
            tag.remove(answersKey(key));
        if (gen != 0)
            tag.putLong(genKey(key), gen);
        else
            tag.remove(genKey(key));
    }

    /** True when {@code tag} holds a saved ring under {@code key} (a long array). */
    public static boolean savedIn(CompoundTag tag, String key) {
        checkKey(key);
        return tag.contains(key, Tag.TAG_LONG_ARRAY);
    }

    /**
     * Replaces the ring with the one saved under {@code key} (its answers and generation with it), keeping the
     * newest past the capacity. A compound with no ring there (or something else under the key) leaves the
     * ring empty, generation 0: false.
     */
    public boolean readFrom(CompoundTag tag, String key) {
        if (!savedIn(tag, key)) {
            clear();
            gen = 0;
            return false;
        }
        String answers = answersKey(key);
        load(tag.getLongArray(key), tag.contains(answers, Tag.TAG_LONG_ARRAY) ? tag.getLongArray(answers) : null);
        gen = tag.contains(genKey(key), Tag.TAG_LONG) ? tag.getLong(genKey(key)) : 0;
        return true;
    }

    private static void checkKey(String key) {
        if (key == null || key.indexOf(':') < 1 || key.indexOf(':') == key.length() - 1)
            throw new IllegalArgumentException("not a namespaced key: " + key);
    }

    @Override
    public String toString() {
        return "AppliedOpRing" + Arrays.toString(toArray()) + " at " + Arrays.toString(answersArray()) + " gen " + gen;
    }
}
