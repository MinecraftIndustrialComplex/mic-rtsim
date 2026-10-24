package com.minecraftindustrialcomplex.mic_rtsim.industry.glass;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
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
 * <p><b>Count points.</b> The movers of a mod call {@link #isMover} and then {@link #moved} at the place
 * where a real move happens, with the mover's block position and the position of the inventory it moved
 * to or from. This class knows no mod's names: {@code mic_rtsim_create}'s mixins
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

    /** The arm inserting through a funnel point now, else null (server thread; {@link #moverOf}). */
    private static BlockPos armAtFunnel;

    /**
     * A mechanical arm's insert at a funnel point runs through the funnel's {@code InvManipulationBehaviour}
     * while {@code arm} is set (null after): the compile names that move an ARM edge from the arm to the store
     * behind the funnel, so it is counted under the arm, not the funnel (choice SC125).
     */
    public static void armThroughFunnel(BlockPos arm) {
        armAtFunnel = arm;
    }

    /** The mover a behaviour at {@code behaviour} counts under: the arm inserting through it, else itself. */
    public static BlockPos moverOf(BlockPos behaviour) {
        BlockPos arm = armAtFunnel;
        return arm != null ? arm : behaviour;
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

    /** Pairs a GameTest forces ({@link #holdForTest}): every set keeps them. */
    private record Forced(ResourceKey<Level> dim, java.util.Set<Pair> pairs) {}

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
    private final Long2ObjectOpenHashMap<Forced> forced = new Long2ObjectOpenHashMap<>();
    /** Per dimension, the keys by their mover's position: what a count point looks up. */
    private final Reference2ObjectArrayMap<ResourceKey<Level>, Long2ObjectOpenHashMap<Key[]>> byMover = new Reference2ObjectArrayMap<>();
    private Industry.GoodsCounter scratch;

    // Counters for the GameTests and /rtsim district perf (measured, not tuned).
    long setsTaken;
    long setsIgnored;
    long samplesSent;
    long samplesDropped;
    long movesCounted;
    /** Count points that threw (logged, the move uncounted). */
    long failures;

    public Counting(Industry ind) {
        this.ind = ind;
    }

    // ---- the count points -----------------------------------------------------------------------

    /**
     * The cheap test of a count point: whether the block at {@code mover} has counted pairs in this
     * (server) level. One hash lookup; a count point asks it before it works out the move's target.
     */
    public boolean isMover(Level level, BlockPos mover) {
        try {
            if (!(level instanceof ServerLevel))
                return false;
            Long2ObjectOpenHashMap<Key[]> map = byMover.get(level.dimension());
            return map != null && map.containsKey(mover.asLong());
        } catch (RuntimeException e) {
            failed("the mover test at " + mover, e);
            return false;
        }
    }

    /**
     * A count point failed: it runs inside Create's block entity tick, where an exception would break the
     * machine, so it is logged as the bridge logs a failure ({@code Industry.fail}) and the move goes
     * uncounted (a residue the unit's window shows, SC88). Logged for the first failure and each time the
     * number of failures doubles, so a point that throws at every move does not flood the log.
     */
    public void failed(String what, Throwable t) {
        long n = ++failures;
        if ((n & (n - 1)) == 0)
            MicRtsim.LOGGER.error("[mic_rtsim] industry: counting {} (failure {})", what, n, t);
    }

    /**
     * A real move of {@code n} of {@code stack} by the block at {@code mover}, between it and the inventory
     * at {@code target}: into the store when {@code into}, else out of it. Simulated moves and client-side
     * ones are not moves: the caller does not report them (and a level that is not a {@link ServerLevel} is
     * skipped here). Counted when the (mover, canonical inventory of target) pair is one the engine named.
     */
    public void moved(Level level, BlockPos mover, BlockPos target, ItemStack stack, int n, boolean into) {
        try {
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
        } catch (RuntimeException e) {
            failed("a move at " + mover + " to " + target, e);
        }
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
        Forced f = forced.get(district);
        if (f != null && f.dim.equals(dim))
            for (Pair p : f.pairs) {
                Key k = old == null ? null : old.keys.get(p);
                next.putIfAbsent(p, k != null ? k : new Key(district, p.mover, p.store, now));
            }
        if (old != null)
            for (Map.Entry<Pair, Key> e : old.keys.entrySet())
                if (!next.containsKey(e.getKey()))
                    unindex(old.dim, e.getValue());
        for (Map.Entry<Pair, Key> e : next.entrySet())
            if (old == null || !old.keys.containsKey(e.getKey()))
                index(dim, e.getValue());
        // A guard: the caller sends an empty set to empty(), so this is not reached with n == 0, but a set that
        // leaves nothing holds nothing: `active` must go null, or every hooked move pays for isMover.
        if (next.isEmpty())
            held.remove(district);
        else
            held.put(district, new Held(dim, next));
        setsTaken++;
        refresh();
    }

    /** Drops every counter of {@code district}: its removal (the pairs a GameTest forced go too). */
    void drop(long district) {
        forced.remove(district);
        Held h = held.remove(district);
        if (h == null)
            return;
        for (Key k : h.keys.values())
            unindex(h.dim, k);
        refresh();
    }

    /** An empty set: no pair is named, so every counter of {@code district} goes (but the pairs a GameTest forced). */
    void empty(long district) {
        Held h = held.get(district);
        if (h == null)
            return;
        Forced f = forced.get(district);
        Map<Pair, Key> keep = new LinkedHashMap<>();
        for (Map.Entry<Pair, Key> e : h.keys.entrySet()) {
            if (f != null && f.pairs.contains(e.getKey()))
                keep.put(e.getKey(), e.getValue());
            else
                unindex(h.dim, e.getValue());
        }
        if (keep.isEmpty())
            held.remove(district);
        else
            held.put(district, new Held(h.dim, keep));
        refresh();
    }

    /** Drops everything: the session ends, or the module goes off. */
    public void clear() {
        held.clear();
        forced.clear();
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
     * has one entry with good NONE, for its {@code since}. Several records when the entries do not fit one, each cut between (mover node, store node) pairs.
     */
    void sample(District d, Numbering num, int sweep, long now) {
        Held h = held.get(d.id);
        if (h == null || num.frame != 0)
            return;
        IntArrayList mover = new IntArrayList(), store = new IntArrayList(), good = new IntArrayList();
        // each entry's pair's total entry count (the reserved u32: the engine refuses a pair it got fewer of)
        IntArrayList of = new IntArrayList();
        LongArrayList since = new LongArrayList(), in = new LongArrayList(), out = new LongArrayList();
        // where each (mover node, store node) pair's entries start: a record is cut only between pairs
        IntArrayList starts = new IntArrayList();
        for (Key k : h.keys.values()) {
            if (k.snapSweep != sweep)
                continue;
            int[] ms = num.moversAt(k.mover), ss = num.storesAt(k.store);
            for (int m : ms)
                for (int s : ss) {
                    starts.add(mover.size());
                    int goods = Math.max(1, k.snapGood.length);
                    for (int g = 0; g < goods; g++) {
                        boolean none = k.snapGood.length == 0;
                        mover.add(m);
                        store.add(s);
                        good.add(none ? GlassRecords.NONE : k.snapGood[g]);
                        of.add(goods);
                        since.add(k.since);
                        in.add(none ? 0 : k.snapIn[g]);
                        out.add(none ? 0 : k.snapOut[g]);
                    }
                }
        }
        int total = mover.size();
        int per = (GraphBuilder.MAX_RECORD - GlassRecords.CountSample.ENTRIES) / GlassRecords.CountSample.ENTRY;
        int[] cuts = cuts(starts.toIntArray(), total, per);
        int from = 0;
        for (int to : cuts) {
            if (to <= from)
                continue;
            int n = to - from;
            int at = ind.latest(GlassRecords.IND_COUNT_SAMPLE, GlassRecords.CountSample.size(n), n);
            if (at < 0) {
                samplesDropped++;
                from = to;
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
                b.putInt(e + GlassRecords.CountSample.E_ENTRIES, of.getInt(from + i));
                b.putLong(e + GlassRecords.CountSample.E_SINCE, since.getLong(from + i));
                b.putLong(e + GlassRecords.CountSample.E_IN, in.getLong(from + i));
                b.putLong(e + GlassRecords.CountSample.E_OUT, out.getLong(from + i));
            }
            samplesSent++;
            from = to;
        }
    }

    /**
     * Where a sample's entries are cut into records of at most {@code per} entries (the last cut is
     * {@code total}): whole pairs each ({@code starts} are the entries' first index of each (mover node, store
     * node) pair, ascending), so a record lost on a full outbound ring leaves a pair missing (the engine sees
     * no cover for it and discards the sweep's unit) and never a pair with some of its goods (which would read
     * as goods not moved). A single pair with more goods than one record holds (about 1,600 goods at one
     * (mover, store): a key keeps every good ever moved through the pair, never pruned, and goods that differ
     * in components, such as damaged gear, Destroy mixtures or enchanted books, count apart) is split in
     * {@code per} entries. That case alone can lose part of a pair, so every entry carries its pair's total
     * entry count ({@code E_ENTRIES}) and the engine gives no cover to a pair it got fewer entries of (SC123).
     * Not pruned: dropping a good's counters would move the cumulative counts the engine's window is based
     * on and read as a residue; a counter restarts only with its key ({@code since}).
     */
    static int[] cuts(int[] starts, int total, int per) {
        if (per <= 0)
            throw new IllegalArgumentException("a record must hold at least one entry, per = " + per);
        int[] cuts = new int[starts.length + total / Math.max(1, per) + 2];
        int n = 0, recStart = 0;
        for (int g = 0; g < starts.length; g++) {
            int gs = starts[g], ge = g + 1 < starts.length ? starts[g + 1] : total;
            if (ge - gs > per) {
                if (gs > recStart)
                    cuts[n++] = gs;
                int at = gs + per;
                for (; at < ge; at += per)
                    cuts[n++] = at;
                recStart = at - per;
            } else if (ge - recStart > per) {
                cuts[n++] = gs;
                recStart = gs;
            }
        }
        cuts[n++] = total;
        return Arrays.copyOf(cuts, n);
    }

    // ---- diagnostics (GameTests, /rtsim district perf) -------------------------------------------

    /** Count sets taken. */
    public long setsTaken() {
        return setsTaken;
    }

    /** Count sets ignored: a compile that is not the latest complete one, a moving frame, nothing resolvable, a malformed one. */
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

    /** The (mover, store) positions {@code district} holds keys for, in the order the engine named them. */
    public List<BlockPos[]> pairs(long district) {
        Held h = held.get(district);
        List<BlockPos[]> out = new ArrayList<>();
        if (h != null)
            for (Key k : h.keys.values())
                out.add(new BlockPos[] {BlockPos.of(k.mover), BlockPos.of(k.store)});
        return out;
    }

    /**
     * Forces one pair onto {@code district}'s keys, its counters starting at {@code now}: for a GameTest of a
     * count point whose block the engine names no pair for (see BasinIntoASharedVaultFactory). Every set the
     * engine sends afterwards keeps the pair, so the test can count over a stretch; the engine skips a pair it
     * did not name when it reads a sample, and the district's removal drops it.
     */
    public void holdForTest(long district, ResourceKey<Level> dim, BlockPos mover, BlockPos store, long now) {
        Pair p = new Pair(mover.asLong(), store.asLong());
        Forced f = forced.get(district);
        if (f == null || !f.dim.equals(dim)) {
            f = new Forced(dim, new java.util.LinkedHashSet<>());
            forced.put(district, f);
        }
        f.pairs.add(p);
        Held old = held.get(district);
        Map<Pair, Key> next = new LinkedHashMap<>();
        if (old != null && old.dim.equals(dim))
            next.putAll(old.keys);
        if (next.containsKey(p))
            return;
        Key k = new Key(district, p.mover, p.store, now);
        next.put(p, k);
        index(dim, k);
        held.put(district, new Held(dim, next));
        refresh();
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
