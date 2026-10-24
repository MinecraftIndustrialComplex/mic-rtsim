package com.minecraftindustrialcomplex.mic_rtsim.industry.glass;

import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Districts;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import it.unimi.dsi.fastutil.ints.Int2LongOpenHashMap;
import it.unimi.dsi.fastutil.ints.IntArrayList;
import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.longs.LongArrayList;
import it.unimi.dsi.fastutil.objects.Reference2ObjectArrayMap;
import net.minecraft.core.BlockPos;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Counting at the movers (SECTIONS.md §4.2, SEC-1 slice S3): what each mover put into a shared store and took
 * out of it, by good, since the engine asked for it. The engine names the (mover, store) node pairs of a
 * district's counted stores in IND_COUNT_SET; Java keys a counter by the two blocks' positions, so a pair a
 * later set names again keeps its counters and its {@code since}, and a pair no longer named is dropped.
 * Each sweep, when the sampler reads a counted store's inventory, the counters of that store's keys are
 * snapshotted with the stock (a count and the stock it explains belong to one instant), and the snapshots go
 * out as IND_COUNT_SAMPLE records just before the sweep's IND_SWEEP_END ({@link #sample}).
 *
 * <p><b>Count points.</b> The movers of a mod call {@link #moved} (or {@link #inserted} and
 * {@link #extracted}) at the place where a real move happens, with the mover's block position and the position
 * of the inventory it moved to or from. This class knows no mod's names: {@code mic_rtsim_create}'s mixins
 * are the count points of Create (PATCHES.md). A call costs a null test of {@link #active()} when nothing is
 * counted, and one hash lookup on the mover's position when something is, the target's canonical inventory
 * ({@link Districts#canonicalInventory}) being worked out only for a mover that has keys.
 *
 * <p>State is this session's and runtime only (UR-1): nothing is saved, a restart counts from nothing, and
 * the engine, which does not keep a set across a session either, names its pairs again.
 * Server thread only.
 */
public final class Counting {
    private static Counting active;

    /**
     * This session's counting while any pair is counted, else null: a count point tests this first, so a
     * world with nothing counted pays one static load and a branch per real move.
     */
    public static Counting active() {
        return active;
    }

    /** One counted (mover block, store inventory) pair. */
    static final class Key {
        final long district;
        final long mover, store;
        /** The game time its counters started at: they restart only when the key is dropped and named again. */
        final long since;
        final Int2LongOpenHashMap in = new Int2LongOpenHashMap();
        final Int2LongOpenHashMap out = new Int2LongOpenHashMap();
        /** The counters as of the sweep's read of the store ({@link #snapSweep}, -1 none yet). */
        int snapSweep = -1;
        int[] snapGood = new int[0];
        long[] snapIn = new long[0], snapOut = new long[0];

        Key(long district, long mover, long store, long since) {
            this.district = district;
            this.mover = mover;
            this.store = store;
            this.since = since;
        }

        void snapshot(int sweep) {
            int[] ids = new int[in.size() + out.size()];
            int n = 0;
            for (int id : in.keySet())
                ids[n++] = id;
            for (int id : out.keySet())
                if (!in.containsKey(id))
                    ids[n++] = id;
            ids = Arrays.copyOf(ids, n);
            Arrays.sort(ids);
            long[] i = new long[n], o = new long[n];
            for (int k = 0; k < n; k++) {
                i[k] = in.get(ids[k]);
                o[k] = out.get(ids[k]);
            }
            snapGood = ids;
            snapIn = i;
            snapOut = o;
            snapSweep = sweep;
        }
    }

    private record Pair(long mover, long store) {}

    /** A district's keys. */
    private static final class Held {
        final ResourceKey<Level> dim;
        final Map<Pair, Key> keys;
        final Long2ObjectOpenHashMap<List<Key>> byStore = new Long2ObjectOpenHashMap<>();

        Held(ResourceKey<Level> dim, Map<Pair, Key> keys) {
            this.dim = dim;
            this.keys = keys;
            for (Key k : keys.values())
                byStore.computeIfAbsent(k.store, s -> new ArrayList<>(1)).add(k);
        }
    }

    private final Industry ind;
    private final Long2ObjectOpenHashMap<Held> held = new Long2ObjectOpenHashMap<>();
    /** Per dimension, the keys by their mover's position: what a count point looks up. */
    private final Reference2ObjectArrayMap<ResourceKey<Level>, Long2ObjectOpenHashMap<Key[]>> byMover = new Reference2ObjectArrayMap<>();
    private Industry.GoodsCounter scratch;

    // Counters for the GameTests and /rtsim district perf (measured, not tuned).
    long setsTaken;
    long setsIgnored;
    long samplesSent;
    long samplesDropped;
    long movesCounted;

    public Counting(Industry ind) {
        this.ind = ind;
    }

    // ---- the count points -----------------------------------------------------------------------

    /**
     * A real move of {@code n} of {@code stack} by the block at {@code mover}, between it and the inventory
     * at {@code target}: into the store when {@code into}, else out of it. Simulated moves and client-side
     * ones are not moves: the caller does not report them (and a level that is not a {@link ServerLevel} is
     * skipped here). Counted when the (mover, canonical inventory of target) pair is one the engine named.
     */
    public void moved(Level level, BlockPos mover, BlockPos target, ItemStack stack, int n, boolean into) {
        if (n <= 0 || stack.isEmpty() || !(level instanceof ServerLevel serverLevel))
            return;
        Long2ObjectOpenHashMap<Key[]> map = byMover.get(level.dimension());
        if (map == null)
            return;
        Key[] ks = map.get(mover.asLong());
        if (ks == null)
            return;
        BlockPos store = Districts.canonicalInventory(serverLevel, target);
        if (store == null)
            return;
        long sp = store.asLong();
        for (Key k : ks)
            if (k.store == sp)
                count(k, stack, n, into);
    }

    /** {@code stack} was offered to the inventory at {@code target} and {@code remainder} came back: the difference moved in. */
    public void inserted(Level level, BlockPos mover, BlockPos target, ItemStack stack, ItemStack remainder) {
        moved(level, mover, target, stack, stack.getCount() - remainder.getCount(), true);
    }

    /** {@code taken} was drawn from the inventory at {@code target}. */
    public void extracted(Level level, BlockPos mover, BlockPos target, ItemStack taken) {
        moved(level, mover, target, taken, taken.getCount(), false);
    }

    private void count(Key k, ItemStack stack, int n, boolean into) {
        if (scratch == null)
            scratch = ind.counter();
        scratch.clear();
        ind.countStack(stack, n, scratch);
        Int2LongOpenHashMap m = into ? k.in : k.out;
        for (int id : scratch.nonZeroIds())
            m.addTo(id, scratch.get(id));
        movesCounted++;
    }

    // ---- IND_COUNT_SET --------------------------------------------------------------------------

    /**
     * Makes {@code district}'s counted pairs these (mover position, store position) pairs, in {@code dim}.
     * A pair already held keeps its counters and {@code since}; one not named is dropped; new ones start now.
     */
    void apply(long district, ResourceKey<Level> dim, long[] movers, long[] stores, int n, long now) {
        Held old = held.get(district);
        if (old != null && !old.dim.equals(dim)) {
            drop(district);
            old = null;
        }
        Map<Pair, Key> next = new LinkedHashMap<>();
        for (int i = 0; i < n; i++) {
            Pair p = new Pair(movers[i], stores[i]);
            if (next.containsKey(p))
                continue;
            Key k = old == null ? null : old.keys.get(p);
            next.put(p, k != null ? k : new Key(district, p.mover, p.store, now));
        }
        if (old != null)
            for (Map.Entry<Pair, Key> e : old.keys.entrySet())
                if (!next.containsKey(e.getKey()))
                    unindex(old.dim, e.getValue());
        for (Map.Entry<Pair, Key> e : next.entrySet())
            if (old == null || !old.keys.containsKey(e.getKey()))
                index(dim, e.getValue());
        held.put(district, new Held(dim, next));
        setsTaken++;
        refresh();
    }

    /** Drops every counter of {@code district} (an empty set, its removal). */
    void drop(long district) {
        Held h = held.remove(district);
        if (h == null)
            return;
        for (Key k : h.keys.values())
            unindex(h.dim, k);
        refresh();
    }

    /** Drops everything: the session ends, or the module goes off. */
    public void clear() {
        held.clear();
        byMover.clear();
        refresh();
    }

    private void index(ResourceKey<Level> dim, Key k) {
        Long2ObjectOpenHashMap<Key[]> map = byMover.computeIfAbsent(dim, d -> new Long2ObjectOpenHashMap<>());
        Key[] old = map.get(k.mover);
        if (old == null) {
            map.put(k.mover, new Key[] {k});
        } else {
            Key[] grown = Arrays.copyOf(old, old.length + 1);
            grown[old.length] = k;
            map.put(k.mover, grown);
        }
    }

    private void unindex(ResourceKey<Level> dim, Key k) {
        Long2ObjectOpenHashMap<Key[]> map = byMover.get(dim);
        if (map == null)
            return;
        Key[] old = map.get(k.mover);
        if (old == null)
            return;
        int at = -1;
        for (int i = 0; i < old.length; i++)
            if (old[i] == k)
                at = i;
        if (at < 0)
            return;
        if (old.length == 1) {
            map.remove(k.mover);
            if (map.isEmpty())
                byMover.remove(dim);
            return;
        }
        Key[] shrunk = new Key[old.length - 1];
        System.arraycopy(old, 0, shrunk, 0, at);
        System.arraycopy(old, at + 1, shrunk, at, old.length - at - 1);
        map.put(k.mover, shrunk);
    }

    /** The static pointer follows whether anything is held. */
    private void refresh() {
        if (!held.isEmpty()) {
            active = this;
        } else if (active == this) {
            active = null;
        }
    }

    // ---- IND_COUNT_SAMPLE -----------------------------------------------------------------------

    /**
     * The sampler read the inventory {@code inv} of {@code d} in sweep {@code sweep}: the counters of the
     * keys at that store are taken now, as of this read (SC88: a store not read in the sweep has none).
     */
    public void snapshot(District d, District.Inventory inv, int sweep) {
        Held h = held.get(d.id);
        if (h == null)
            return;
        List<Key> ks = h.byStore.get(inv.pos().asLong());
        if (ks == null)
            return;
        for (Key k : ks)
            k.snapshot(sweep);
    }

    /**
     * The sweep's snapshots, as IND_COUNT_SAMPLE records of the sweep in the node numbers of {@code num}, the
     * latest compile whose END Java sent (the engine reads a sample against the compile it holds). Each key
     * goes out under every (node at its mover's position that is no store, STORE node at its store's position)
     * pair: several nodes can sit at one block (a packager is two), the engine takes a key's counts once per
     * block (SC94) and skips the pairs it did not ask for. One entry per good; a key with no good counted yet
     * has one entry with good NONE, for its {@code since}. Several records when the entries do not fit one.
     */
    void sample(District d, Numbering num, int sweep, long now) {
        Held h = held.get(d.id);
        if (h == null || num.frame != 0)
            return;
        IntArrayList mover = new IntArrayList(), store = new IntArrayList(), good = new IntArrayList();
        LongArrayList since = new LongArrayList(), in = new LongArrayList(), out = new LongArrayList();
        for (Key k : h.keys.values()) {
            if (k.snapSweep != sweep)
                continue;
            int[] ms = num.moversAt(k.mover), ss = num.storesAt(k.store);
            for (int m : ms)
                for (int s : ss) {
                    int goods = Math.max(1, k.snapGood.length);
                    for (int g = 0; g < goods; g++) {
                        boolean none = k.snapGood.length == 0;
                        mover.add(m);
                        store.add(s);
                        good.add(none ? GlassRecords.NONE : k.snapGood[g]);
                        since.add(k.since);
                        in.add(none ? 0 : k.snapIn[g]);
                        out.add(none ? 0 : k.snapOut[g]);
                    }
                }
        }
        int total = mover.size();
        int per = (GraphBuilder.MAX_RECORD - GlassRecords.CountSample.ENTRIES) / GlassRecords.CountSample.ENTRY;
        for (int from = 0; from < total; from += per) {
            int n = Math.min(per, total - from);
            int at = ind.latest(GlassRecords.IND_COUNT_SAMPLE, GlassRecords.CountSample.size(n), n);
            if (at < 0) {
                samplesDropped++;
                continue;
            }
            ByteBuffer b = ind.open();
            b.putLong(at + GlassRecords.GAME_TIME, now);
            b.putLong(at + GlassRecords.DISTRICT, d.id);
            b.putInt(at + GlassRecords.CountSample.SWEEP, sweep);
            b.putInt(at + GlassRecords.CountSample.COMPILE, num.compile);
            for (int i = 0; i < n; i++) {
                int e = at + GlassRecords.CountSample.ENTRIES + GlassRecords.CountSample.ENTRY * i;
                b.putInt(e + GlassRecords.CountSample.E_MOVER, mover.getInt(from + i));
                b.putInt(e + GlassRecords.CountSample.E_STORE, store.getInt(from + i));
                b.putInt(e + GlassRecords.CountSample.E_GOOD, good.getInt(from + i));
                b.putLong(e + GlassRecords.CountSample.E_SINCE, since.getLong(from + i));
                b.putLong(e + GlassRecords.CountSample.E_IN, in.getLong(from + i));
                b.putLong(e + GlassRecords.CountSample.E_OUT, out.getLong(from + i));
            }
            samplesSent++;
        }
    }

    // ---- diagnostics (GameTests, /rtsim district perf) -------------------------------------------

    /** Count sets taken. */
    public long setsTaken() {
        return setsTaken;
    }

    /** Count sets ignored: a compile that is not the latest complete one, a moving frame, nothing resolvable. */
    public long setsIgnored() {
        return setsIgnored;
    }

    /** IND_COUNT_SAMPLE records sent. */
    public long samplesSent() {
        return samplesSent;
    }

    /** Moves counted (each real move at a counted pair). */
    public long movesCounted() {
        return movesCounted;
    }

    /** Pairs held now, in every district. */
    public int keysHeld() {
        int n = 0;
        for (Held h : held.values())
            n += h.keys.size();
        return n;
    }

    /** Whether {@code district} holds a key for this (mover, store) pair of positions. */
    public boolean holds(long district, BlockPos mover, BlockPos store) {
        Held h = held.get(district);
        return h != null && h.keys.containsKey(new Pair(mover.asLong(), store.asLong()));
    }

    /**
     * A held key's {@code {since, items in, items out}} (all goods summed), or null when none: for the GameTests,
     * which compare it to what the movers did.
     */
    public long[] counters(long district, BlockPos mover, BlockPos store) {
        Held h = held.get(district);
        Key k = h == null ? null : h.keys.get(new Pair(mover.asLong(), store.asLong()));
        if (k == null)
            return null;
        long i = 0, o = 0;
        for (long v : k.in.values())
            i += v;
        for (long v : k.out.values())
            o += v;
        return new long[] {k.since, i, o};
    }
}
