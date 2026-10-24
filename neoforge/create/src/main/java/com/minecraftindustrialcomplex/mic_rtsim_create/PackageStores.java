package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorBlockEntity;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorPackage;
import com.simibubi.create.content.logistics.box.PackageItem;
import com.simibubi.create.content.logistics.packagePort.PackagePortTarget;
import com.simibubi.create.content.logistics.packagePort.frogport.FrogportBlockEntity;
import it.unimi.dsi.fastutil.ints.IntArrayList;
import it.unimi.dsi.fastutil.longs.Long2IntOpenHashMap;
import it.unimi.dsi.fastutil.longs.Long2LongOpenHashMap;
import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.longs.LongArrayList;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import it.unimi.dsi.fastutil.longs.LongSet;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.Tag;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.chunk.LevelChunk;

import java.nio.ByteBuffer;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.WeakHashMap;
import java.util.function.IntUnaryOperator;

/**
 * Package stores (SECTIONS.md §2.5, slice S4a): a Create chain network whose frogports sit in more than one
 * district (a frogport in no district counts as an owner of its own) holds the packages on it, owned by no
 * district, with a port at each frogport. Java owns a store's identity and sends it to the engine as
 * IND_PACKAGE_STORE; every package a store's frogport puts on the chain or takes off it is an
 * IND_CHAIN_PACKAGE ({@code FrogportChainMixin}); while every chunk of a store ticks, a census counts the
 * packages on it (IND_STORE_CENSUS).
 *
 * <ul>
 *   <li><b>The walk.</b> From a conveyor along Create's {@code connections}, through loaded chunks only
 *       ({@link Loaded}): the conveyors, and the frogports whose conveyor is one of them, found in the
 *       loaded chunks the conveyors are in and their neighbours (a frogport is at most
 *       {@code packagePortRange}, 5 blocks, from its conveyor). A walk that stops at a chunk that is not
 *       loaded keeps the frogports the touched stores last listed where it cannot look.</li>
 *   <li><b>Identity.</b> The touched store whose lowest frogport is in the walk keeps its id (the lowest id
 *       when several are, which merges the others into it); else the walk is a new store. A frogport that
 *       comes from another store says so ({@code from}), so what that store held for it moves with it.
 *       Declarations go before removals, so a store that receives frogports has them before the store that
 *       gave them up is told.</li>
 *   <li><b>Routing.</b> A package leaving a frogport heads for the matching frogport of the least distance
 *       (hops along the chain, 1000 further for a {@code *} filter, as Create's routing tables count), ties
 *       to the lowest position; the first matching frogport on a least-hop way there takes it (choice
 *       SC100).</li>
 *   <li><b>The census.</b> Every {@link #CENSUS_TICKS} while every chunk of the store ticks: the walk again
 *       (which refreshes the identity), then the looping and travelling packages of each conveyor by the
 *       frogport their address leads to from there, and each frogport's package in its arrival
 *       animation.</li>
 * </ul>
 * The stores persist in {@code IndustryData} under {@link #KEY} (format 3). Nothing here changes the world: block
 * entities are read with {@code EntityCreationType.CHECK} or through {@link Loaded}, never created.
 */
public final class PackageStores {
    /** The key of the stores' state in {@code IndustryData}. */
    public static final String KEY = "mic_rtsim_create:package_stores";
    /** TUNABLE (choice SC99): ticks between two censuses of a store whose chunks all tick. */
    public static final int CENSUS_TICKS = 200;
    /** TUNABLE: ticks before a frogport on no known store is walked again at its next package. */
    static final int UNKNOWN_WALK_TICKS = 1200;
    /** At most this many conveyors in one walk, as {@code CreateCompile.chainNetwork}. */
    static final int MAX_CONVEYORS = 4096;
    static final int UNKNOWN = IndRecords.PackageStore.UNKNOWN_HOPS;
    /**
     * TUNABLE: a network with more frogports is no store (it keeps the edges it had before S4), so its
     * IND_PACKAGE_STORE stays under a record's 64 KiB (48 bytes a frogport and 2 a pair).
     */
    static final int MAX_PORTS = 200;
    /** At most this many stores split off one walk, then another walk (a later census) carries on. */
    private static final int MAX_DEPTH = 4;

    private static final Map<Industry, PackageStores> BY_INDUSTRY = new WeakHashMap<>();

    /** The stores of {@code ind}'s session, loaded from its {@code IndustryData} at first use. */
    public static PackageStores of(Industry ind) {
        return BY_INDUSTRY.computeIfAbsent(ind, PackageStores::new);
    }

    /** One frogport of a store. */
    public static final class Port {
        public final BlockPos pos;
        /** The conveyor it serves: Create moves a frogport to a connection's far conveyor when the chain turns the other way. */
        public BlockPos conveyor;
        /** Its address filter; null when it takes no packages (Create registers it on no conveyor). */
        public String filter;

        Port(BlockPos pos, BlockPos conveyor, String filter) {
            this.pos = pos;
            this.conveyor = conveyor;
            this.filter = filter;
        }

        boolean star() {
            return "*".equals(filter);
        }
    }

    /** A package store as last declared. */
    public static final class Store {
        public final long id;
        public final ResourceKey<Level> dim;
        /** By position (the order of IND_PACKAGE_STORE's frogports). */
        public final List<Port> ports = new ArrayList<>();
        /** Hops between the ports' conveyors, upper triangle as IND_PACKAGE_STORE has them. */
        int[] hops = new int[0];
        /** The conveyors of the last walk. */
        final LongOpenHashSet conveyors = new LongOpenHashSet();
        /** What the last declaration this session sent said, but {@code from}; null when none was sent. */
        String declared;
        long nextCensus;

        Store(long id, ResourceKey<Level> dim) {
            this.id = id;
            this.dim = dim;
        }

        public int indexOf(BlockPos p) {
            for (int i = 0; i < ports.size(); i++)
                if (ports.get(i).pos.equals(p))
                    return i;
            return -1;
        }

        public int hops(int a, int b) {
            if (a == b)
                return 0;
            int lo = Math.min(a, b), hi = Math.max(a, b), n = ports.size();
            return hops[lo * (2 * n - lo - 1) / 2 + (hi - lo - 1)];
        }
    }

    /** One walk of a chain network. */
    static final class Walk {
        final Long2IntOpenHashMap index = new Long2IntOpenHashMap();
        final LongArrayList order = new LongArrayList();
        final List<int[]> adjacent = new ArrayList<>();
        /** False when the walk stopped at a chunk that is not loaded, or at {@link #MAX_CONVEYORS}. */
        boolean complete = true;
        /** The frogports found in loaded chunks, by position. */
        final TreeMap<BlockPos, Port> ports = new TreeMap<>();

        Walk() {
            index.defaultReturnValue(-1);
        }

        int of(BlockPos conveyor) {
            return conveyor == null ? -1 : index.get(conveyor.asLong());
        }

        /** Hops from conveyor {@code from} to every conveyor of the walk; -1 where none leads. */
        int[] bfs(int from) {
            int[] d = new int[order.size()];
            Arrays.fill(d, -1);
            if (from < 0)
                return d;
            IntArrayList q = new IntArrayList();
            d[from] = 0;
            q.add(from);
            for (int h = 0; h < q.size(); h++) {
                int c = q.getInt(h);
                for (int n : adjacent.get(c))
                    if (d[n] < 0) {
                        d[n] = d[c] + 1;
                        q.add(n);
                    }
            }
            return d;
        }
    }

    final Industry ind;
    final TreeMap<Long, Store> stores = new TreeMap<>();
    private final Map<ResourceKey<Level>, Long2LongOpenHashMap> byPort = new HashMap<>();
    private final Map<ResourceKey<Level>, Long2LongOpenHashMap> nextUnknownWalk = new HashMap<>();
    long nextId = 1;
    // Counters (perfLine).
    long walks, censuses, records, declarations;

    private PackageStores(Industry ind) {
        this.ind = ind;
        load(ind.data().plugin(KEY));
    }

    public java.util.Collection<Store> stores() {
        return stores.values();
    }

    /** The store a frogport at {@code pos} belongs to, or null. */
    public Store storeOfPort(ResourceKey<Level> dim, BlockPos pos) {
        Long2LongOpenHashMap m = byPort.get(dim);
        if (m == null)
            return null;
        long id = m.get(pos.asLong());
        return id == 0 ? null : stores.get(id);
    }

    // ---- the walk -------------------------------------------------------------------------------

    static Walk walk(ServerLevel level, BlockPos start) {
        Walk w = new Walk();
        ArrayDeque<BlockPos> q = new ArrayDeque<>();
        LongOpenHashSet seen = new LongOpenHashSet();
        List<BlockPos[]> links = new ArrayList<>();
        q.add(start);
        seen.add(start.asLong());
        while (!q.isEmpty()) {
            BlockPos p = q.poll();
            if (w.order.size() >= MAX_CONVEYORS) {
                w.complete = false;
                break;
            }
            LevelChunk c = Loaded.chunk(level, p);
            if (c == null) {
                w.complete = false;
                continue;
            }
            if (!(c.getBlockEntity(p, LevelChunk.EntityCreationType.CHECK) instanceof ChainConveyorBlockEntity cc))
                continue;
            w.index.put(p.asLong(), w.order.size());
            w.order.add(p.asLong());
            for (BlockPos rel : cc.connections) {
                BlockPos n = p.offset(rel);
                links.add(new BlockPos[] {p, n});
                if (seen.add(n.asLong()))
                    q.add(n);
            }
        }
        List<IntArrayList> adj = new ArrayList<>();
        for (int i = 0; i < w.order.size(); i++)
            adj.add(new IntArrayList());
        for (BlockPos[] l : links) {
            int a = w.index.get(l[0].asLong()), b = w.index.get(l[1].asLong());
            if (a >= 0 && b >= 0 && a != b) {
                adj.get(a).add(b);
                adj.get(b).add(a);
            }
        }
        for (IntArrayList a : adj)
            w.adjacent.add(a.toIntArray());
        // Frogports: in the loaded chunks of the conveyors and their neighbours.
        LongOpenHashSet chunks = new LongOpenHashSet();
        for (int i = 0; i < w.order.size(); i++) {
            BlockPos p = BlockPos.of(w.order.getLong(i));
            int cx = p.getX() >> 4, cz = p.getZ() >> 4;
            for (int dx = -1; dx <= 1; dx++)
                for (int dz = -1; dz <= 1; dz++)
                    chunks.add(ChunkPos.asLong(cx + dx, cz + dz));
        }
        for (long ch : chunks) {
            LevelChunk c = level.getChunkSource().getChunkNow(ChunkPos.getX(ch), ChunkPos.getZ(ch));
            if (c == null)
                continue;
            for (BlockEntity be : c.getBlockEntities().values()) {
                if (!(be instanceof FrogportBlockEntity f) || !(f.target instanceof PackagePortTarget.ChainConveyorFrogportTarget t))
                    continue;
                BlockPos conveyor = f.getBlockPos().offset(t.relativePos);
                if (w.index.containsKey(conveyor.asLong()))
                    w.ports.put(f.getBlockPos().immutable(), new Port(f.getBlockPos().immutable(), conveyor, f.getFilterString()));
            }
        }
        return w;
    }

    /** The chain conveyor a frogport serves, or null. */
    static BlockPos conveyorOf(FrogportBlockEntity f) {
        return f.target instanceof PackagePortTarget.ChainConveyorFrogportTarget t ? f.getBlockPos().offset(t.relativePos) : null;
    }

    /** What a walk found: the store it is now (null when it is no store), and the walk. */
    public record Observed(Store store, Walk walk) {}

    /**
     * Walks the chain network from {@code conveyor} and brings the stores it touches up to date. Districts
     * whose frogports start or stop being a store's, but {@code compiling}'s, are compiled again.
     */
    public Observed observe(ServerLevel level, BlockPos conveyor, long compiling) {
        return observe(level, conveyor, compiling, 0, null);
    }

    /**
     * {@code leftFrom}: for frogports a parent walk's store just gave up (by position), that store's id, so a
     * store they now form says where they came from and the engine moves what it held for them along.
     */
    private Observed observe(ServerLevel level, BlockPos conveyor, long compiling, int depth, Long2LongOpenHashMap leftFrom) {
        long declaredBefore = declarations;
        Walk w = walk(level, conveyor);
        walks++;
        if (w.order.isEmpty())
            return new Observed(null, w);
        ResourceKey<Level> dim = level.dimension();
        List<Store> touched = new ArrayList<>();
        for (Store s : stores.values())
            if (s.dim == dim && touches(s, w))
                touched.add(s);
        TreeMap<BlockPos, Port> ports = new TreeMap<>(w.ports);
        // A walk that stopped at a chunk that is not loaded cannot tell a frogport beyond it from one that left:
        // it keeps every frogport the touched stores listed that still serves the conveyor they saw it on.
        if (!w.complete)
            for (Store s : touched)
                for (Port p : s.ports)
                    if (!ports.containsKey(p.pos) && (unseen(level, p) || servesAsSeen(level, p)))
                        ports.put(p.pos, new Port(p.pos, p.conveyor, p.filter));
        LongOpenHashSet owners = new LongOpenHashSet();
        for (Port p : ports.values())
            owners.add(owner(dim, p.pos));
        long now = level.getGameTime();
        if (ports.size() > MAX_PORTS && !tooMany) {
            tooMany = true;
            MicRtsim.LOGGER.warn("[mic_rtsim] create: a chain network at {} has {} frogports, more than a package store takes ({}); it stays edges",
                    conveyor, ports.size(), MAX_PORTS);
        }
        if (owners.size() < 2 || ports.size() > MAX_PORTS) {
            Long2LongOpenHashMap later = nextUnknownWalk.computeIfAbsent(dim, d -> new Long2LongOpenHashMap());
            for (Port p : ports.values())
                later.put(p.pos.asLong(), now + UNKNOWN_WALK_TICKS);
            for (Store s : touched)
                shrink(level, s, ports, 0, compiling);
            if (declarations != declaredBefore)
                save();
            return new Observed(null, w);
        }
        Store keep = null;
        for (Store s : touched)
            if (!s.ports.isEmpty() && ports.containsKey(s.ports.get(0).pos)) {
                keep = s;
                break; // touched is in id order: the lowest id
            }
        Store target = keep != null ? keep : new Store(nextId++, dim);
        List<Port> before = new ArrayList<>(target.ports);
        int[] beforeHops = target.hops;
        List<Port> list = new ArrayList<>(ports.values());
        long[] from = new long[list.size()];
        for (int i = 0; i < list.size(); i++) {
            Store old = storeOfPort(dim, list.get(i).pos);
            long hint = leftFrom == null ? 0 : leftFrom.get(list.get(i).pos.asLong());
            from[i] = old != null && old != target ? old.id : old == null && hint != target.id ? hint : 0;
        }
        int[] hops = hops(w, list, touched);
        target.ports.clear();
        target.ports.addAll(list);
        target.hops = hops;
        LongOpenHashSet conveyors = new LongOpenHashSet(w.order.size());
        for (int i = 0; i < w.order.size(); i++)
            conveyors.add(w.order.getLong(i));
        boolean conveyorsChanged = !conveyors.equals(target.conveyors);
        if (conveyorsChanged) {
            target.conveyors.clear();
            target.conveyors.addAll(conveyors);
        }
        if (keep == null) {
            stores.put(target.id, target);
            target.nextCensus = now + CENSUS_TICKS;
        }
        index(target);
        boolean changed = declare(target, 0, from);
        // The frogports the target had that this walk did not keep: they left for another network.
        List<Port> left = new ArrayList<>();
        for (Port p : before)
            if (!ports.containsKey(p.pos))
                left.add(p);
        if (changed)
            recompile(dim, before, list, compiling);
        for (Store s : touched)
            if (s != target)
                shrink(level, s, ports, target.id, compiling);
        if (keep == null || conveyorsChanged || declarations != declaredBefore)
            save();
        if (depth < MAX_DEPTH && !left.isEmpty()) {
            Long2LongOpenHashMap gaveUp = new Long2LongOpenHashMap();
            for (Port p : left)
                gaveUp.put(p.pos.asLong(), target.id);
            for (Port p : left) {
                Store now2 = storeOfPort(dim, p.pos);
                if ((now2 == null || now2 == target) && p.conveyor != null && Loaded.at(level, p.conveyor) && Loaded.at(level, p.pos)
                        && !target.conveyors.contains(p.conveyor.asLong()))
                    observe(level, p.conveyor, compiling, depth + 1, gaveUp);
            }
        }
        return new Observed(target, w);
    }

    /**
     * For a district's compile: the store the frogport at {@code port}, serving {@code conveyor}, is a port of,
     * or null. Each chain network is walked once per {@code cache} (one compile).
     */
    public Store compileStore(ServerLevel level, BlockPos conveyor, BlockPos port, long compiling, Long2ObjectOpenHashMap<Observed> cache) {
        Observed o = cache.get(conveyor.asLong());
        if (o == null) {
            o = observe(level, conveyor, compiling);
            cache.put(conveyor.asLong(), o);
            for (int i = 0; i < o.walk().order.size(); i++)
                cache.put(o.walk().order.getLong(i), o);
        }
        Store s = o.store();
        return s != null && stores.get(s.id) == s && s.indexOf(port) >= 0 ? s : null;
    }

    /** Whether a walk touches a store: a frogport it found, or a conveyor of its last walk. */
    private static boolean touches(Store s, Walk w) {
        for (Port p : s.ports)
            if (w.ports.containsKey(p.pos) || w.index.containsKey(p.conveyor.asLong()))
                return true;
        LongSet c = s.conveyors;
        for (int i = 0; i < w.order.size(); i++)
            if (c.contains(w.order.getLong(i)))
                return true;
        return false;
    }

    /**
     * Whether a frogport a store listed may still be where the walk cannot look: its chunk is not loaded, or
     * its conveyor's is not and the frogport still serves a chain.
     */
    private static boolean unseen(ServerLevel level, Port p) {
        if (!Loaded.at(level, p.pos))
            return true;
        if (Loaded.at(level, p.conveyor))
            return false;
        return Loaded.blockEntity(level, p.pos) instanceof FrogportBlockEntity f
                && f.target instanceof PackagePortTarget.ChainConveyorFrogportTarget;
    }

    /** Whether a frogport in a loaded chunk still serves the conveyor its store saw it on (read only). */
    private static boolean servesAsSeen(ServerLevel level, Port p) {
        return Loaded.blockEntity(level, p.pos) instanceof FrogportBlockEntity f && p.conveyor.equals(conveyorOf(f));
    }

    /** A frogport's owner: its district's id, 0 for none (a chunk of no district). */
    private long owner(ResourceKey<Level> dim, BlockPos pos) {
        District d = ind.districtAt(dim, ChunkPos.asLong(pos.getX() >> 4, pos.getZ() >> 4));
        return d == null ? 0 : d.id;
    }

    /**
     * The hops between the frogports' conveyors along the walk; where the walk did not reach both, the hops a
     * touched store last had between the two, else {@link #UNKNOWN}.
     */
    private static int[] hops(Walk w, List<Port> list, List<Store> touched) {
        int n = list.size();
        int[][] d = new int[n][];
        for (int i = 0; i < n; i++)
            d[i] = w.bfs(w.of(list.get(i).conveyor));
        int[] out = new int[IndRecords.PackageStore.triangle(n)];
        int k = 0;
        for (int a = 0; a < n; a++)
            for (int b = a + 1; b < n; b++) {
                int cb = w.of(list.get(b).conveyor);
                int h = cb >= 0 ? d[a][cb] : -1;
                if (h < 0)
                    h = savedHops(list.get(a).pos, list.get(b).pos, touched);
                out[k++] = Math.min(h, UNKNOWN);
            }
        return out;
    }

    private static int savedHops(BlockPos a, BlockPos b, List<Store> touched) {
        for (Store s : touched) {
            int i = s.indexOf(a), j = s.indexOf(b);
            if (i >= 0 && j >= 0)
                return s.hops(i, j);
        }
        return UNKNOWN;
    }

    /**
     * A touched store other than the walk's own loses the walk's frogports: it is declared again with what it
     * keeps, or removed (what it holds joins {@code into}) when it keeps fewer than two owners or when its lowest
     * frogport went to the walk (a merge).
     */
    private void shrink(ServerLevel level, Store s, Map<BlockPos, Port> taken, long into, long compiling) {
        List<Port> before = new ArrayList<>(s.ports);
        List<Port> kept = new ArrayList<>();
        for (Port p : s.ports)
            if (!taken.containsKey(p.pos) && (unseen(level, p) || stillServes(level, p)))
                kept.add(p);
        if (kept.size() == s.ports.size())
            return;
        LongOpenHashSet owners = new LongOpenHashSet();
        for (Port p : kept)
            owners.add(owner(s.dim, p.pos));
        boolean merged = !s.ports.isEmpty() && taken.containsKey(s.ports.get(0).pos);
        if (merged || owners.size() < 2) {
            remove(s, into, compiling);
            return;
        }
        int[] hops = new int[IndRecords.PackageStore.triangle(kept.size())];
        int k = 0;
        for (int a = 0; a < kept.size(); a++)
            for (int b = a + 1; b < kept.size(); b++)
                hops[k++] = s.hops(s.indexOf(kept.get(a).pos), s.indexOf(kept.get(b).pos));
        s.ports.clear();
        s.ports.addAll(kept);
        s.hops = hops;
        index(s);
        if (declare(s, 0, new long[kept.size()]))
            recompile(s.dim, before, kept, compiling);
    }

    /** Whether a frogport in a loaded chunk still serves a chain conveyor (its conveyor is brought up to date). */
    private static boolean stillServes(ServerLevel level, Port p) {
        if (!(Loaded.blockEntity(level, p.pos) instanceof FrogportBlockEntity f))
            return false;
        BlockPos c = conveyorOf(f);
        if (c == null)
            return false;
        p.conveyor = c;
        p.filter = f.getFilterString();
        return true;
    }

    private void remove(Store s, long into, long compiling) {
        stores.remove(s.id);
        Long2LongOpenHashMap m = byPort.get(s.dim);
        if (m != null)
            for (Port p : s.ports)
                if (m.get(p.pos.asLong()) == s.id)
                    m.remove(p.pos.asLong());
        ByteBuffer r = ind.journal(IndRecords.IND_PACKAGE_STORE, IndRecords.PackageStore.size(0), 0);
        r.putLong(IndRecords.PackageStore.STORE, s.id);
        r.putInt(IndRecords.PackageStore.DIM, ind.dimId(s.dim));
        r.putInt(IndRecords.PackageStore.DIM + 4, 0);
        r.putLong(IndRecords.PackageStore.INTO, into);
        ind.commit(r);
        declarations++;
        recompile(s.dim, s.ports, List.of(), compiling);
    }

    private void index(Store s) {
        Long2LongOpenHashMap m = byPort.computeIfAbsent(s.dim, d -> new Long2LongOpenHashMap());
        m.long2LongEntrySet().removeIf(e -> e.getLongValue() == s.id);
        for (Port p : s.ports)
            m.put(p.pos.asLong(), s.id);
    }

    /** Sends the store's declaration when it differs from the last one sent (or carries frogports from another store). */
    private boolean declare(Store s, long into, long[] from) {
        int n = s.ports.size();
        long[] district = new long[n];
        int[] inventory = new int[n];
        StringBuilder fp = new StringBuilder();
        for (int i = 0; i < n; i++) {
            Port p = s.ports.get(i);
            District d = ind.districtAt(s.dim, ChunkPos.asLong(p.pos.getX() >> 4, p.pos.getZ() >> 4));
            District.Inventory inv = d == null ? null : d.inventoryAt(p.pos);
            district[i] = d == null ? 0 : d.id;
            inventory[i] = inv == null ? IndRecords.NO_INVENTORY : inv.index();
            fp.append(p.pos.asLong()).append(',').append(p.conveyor.asLong()).append(',').append(district[i]).append(',')
                    .append(inventory[i]).append(',').append(p.star() ? 1 : 0).append(';');
        }
        fp.append(Arrays.toString(s.hops));
        boolean moved = false;
        for (long f : from)
            moved |= f != 0;
        String text = fp.toString();
        if (!moved && text.equals(s.declared))
            return false;
        boolean owners = s.declared == null || !owners(s.declared).equals(owners(text));
        s.declared = text;
        send(s, into, from, district, inventory);
        return owners;
    }

    /** The positions and districts of a declaration's fingerprint (what the compiles depend on). */
    private static String owners(String fingerprint) {
        StringBuilder b = new StringBuilder();
        int end = fingerprint.lastIndexOf('[');
        for (String e : fingerprint.substring(0, Math.max(0, end)).split(";")) {
            String[] f = e.split(",");
            if (f.length >= 3)
                b.append(f[0]).append(',').append(f[2]).append(';');
        }
        return b.toString();
    }

    private void send(Store s, long into, long[] from, long[] district, int[] inventory) {
        int n = s.ports.size();
        ByteBuffer r = ind.journal(IndRecords.IND_PACKAGE_STORE, IndRecords.PackageStore.size(n), n);
        r.putLong(IndRecords.PackageStore.STORE, s.id);
        r.putInt(IndRecords.PackageStore.DIM, ind.dimId(s.dim));
        r.putInt(IndRecords.PackageStore.DIM + 4, 0);
        r.putLong(IndRecords.PackageStore.INTO, into);
        for (int i = 0; i < n; i++) {
            Port p = s.ports.get(i);
            int o = IndRecords.PackageStore.FROGPORTS + IndRecords.PackageStore.FROGPORT * i;
            r.putLong(o + IndRecords.PackageStore.F_DISTRICT, district[i]);
            r.putInt(o + IndRecords.PackageStore.F_INVENTORY, inventory[i]);
            r.putInt(o + IndRecords.PackageStore.F_FLAGS, p.star() ? IndRecords.PackageStore.FLAG_STAR : 0);
            r.putInt(o + IndRecords.PackageStore.F_X, p.pos.getX());
            r.putInt(o + IndRecords.PackageStore.F_Y, p.pos.getY());
            r.putInt(o + IndRecords.PackageStore.F_Z, p.pos.getZ());
            r.putInt(o + IndRecords.PackageStore.F_CONVEYOR_X, p.conveyor.getX());
            r.putInt(o + IndRecords.PackageStore.F_CONVEYOR_Y, p.conveyor.getY());
            r.putInt(o + IndRecords.PackageStore.F_CONVEYOR_Z, p.conveyor.getZ());
            r.putLong(o + IndRecords.PackageStore.F_FROM, from[i]);
        }
        for (int a = 0; a < n; a++)
            for (int b = a + 1; b < n; b++)
                r.putShort(IndRecords.PackageStore.hopAt(n, a, b), (short) s.hops(a, b));
        for (int z = IndRecords.PackageStore.FROGPORTS + IndRecords.PackageStore.FROGPORT * n + 2 * IndRecords.PackageStore.triangle(n);
                z < IndRecords.PackageStore.size(n); z++)
            r.put(z, (byte) 0);
        ind.commit(r);
        declarations++;
    }

    /** Compiles again the districts whose frogports joined or left a store, but the one compiling now. */
    private void recompile(ResourceKey<Level> dim, List<Port> before, List<Port> after, long compiling) {
        LongOpenHashSet ids = new LongOpenHashSet();
        for (Port p : before)
            ids.add(owner(dim, p.pos));
        for (Port p : after)
            ids.add(owner(dim, p.pos));
        ids.remove(0L);
        ids.remove(compiling);
        for (long id : ids) {
            District d = ind.district(id);
            if (d != null)
                ind.glass().requestCompile(d);
        }
    }

    /** Sends every store again (the engine forgot what it was told: {@code IndustryPlugin.announce}). */
    public void announce() {
        for (Store s : stores.values()) {
            s.declared = null;
            declare(s, 0, new long[s.ports.size()]);
        }
    }

    // ---- routing (choice SC100) ----------------------------------------------------------------

    /**
     * The frogport index a package with {@code address} reaches when it starts where {@code distance} measures
     * from; {@code exclude} cannot take it on the way (the frogport that sent it), though it may be where it
     * heads. {@link IndRecords#NO_INVENTORY} when no frogport matches (Create's package circles).
     */
    static int route(Store s, String address, IntUnaryOperator distance, int exclude) {
        int n = s.ports.size();
        int best = -1;
        long bestD = Long.MAX_VALUE;
        for (int i = 0; i < n; i++) {
            Port p = s.ports.get(i);
            if (p.filter == null || !PackageItem.matchAddress(address, p.filter))
                continue;
            long d = distance.applyAsInt(i) + (p.star() ? 1000L : 0L);
            if (d < bestD) {
                bestD = d;
                best = i;
            }
        }
        if (best < 0)
            return IndRecords.NO_INVENTORY;
        int take = best;
        int toBest = distance.applyAsInt(best);
        for (int i = 0; i < n; i++) {
            Port p = s.ports.get(i);
            if (i == best || i == exclude || p.filter == null || !PackageItem.matchAddress(address, p.filter))
                continue;
            int di = distance.applyAsInt(i);
            if ((long) di + s.hops(i, best) == toBest && di < distance.applyAsInt(take))
                take = i;
        }
        return take;
    }

    /** Where a package a store's frogport {@code from} sends with {@code address} goes. */
    public static int routeFromPort(Store s, int from, String address) {
        return route(s, address, i -> s.hops(from, i), from);
    }

    // ---- packages at a frogport (FrogportChainMixin) --------------------------------------------

    /**
     * A frogport put a package on its chain ({@code CHAIN_OUT}) or took one off it ({@code CHAIN_IN}): one
     * IND_CHAIN_PACKAGE when the frogport is a store's.
     */
    public static void chainPackage(FrogportBlockEntity f, ItemStack box, int direction) {
        try {
            if (!(f.getLevel() instanceof ServerLevel level) || box == null || box.isEmpty())
                return;
            Industry ind = IndustryApi.current();
            if (ind == null)
                return;
            PackageStores ps = of(ind);
            BlockPos pos = f.getBlockPos();
            Store s = ps.storeOfPort(level.dimension(), pos);
            if (s == null) {
                // On no known store: walk now and then, so a store a district's compile has not seen forms.
                Long2LongOpenHashMap later = ps.nextUnknownWalk.computeIfAbsent(level.dimension(), d -> new Long2LongOpenHashMap());
                long now = level.getGameTime();
                if (later.get(pos.asLong()) > now)
                    return;
                later.put(pos.asLong(), now + UNKNOWN_WALK_TICKS);
                BlockPos conveyor = conveyorOf(f);
                if (conveyor == null)
                    return;
                Observed o = ps.observe(level, conveyor, 0);
                s = o.store();
                if (s == null || s.indexOf(pos) < 0)
                    return;
            }
            int index = s.indexOf(pos);
            if (index < 0)
                return;
            District d = ind.districtAt(level, pos);
            District.Inventory inv = d == null ? null : d.inventoryAt(pos);
            String address = PackageItem.getAddress(box);
            int destination = direction == IndRecords.ChainPackage.CHAIN_OUT ? routeFromPort(s, index, address) : index;
            Industry.GoodsCounter goods = ind.counter();
            ind.countStack(box, box.getCount(), goods);
            journalChain(ind, s, index, d, inv, direction, destination, box.getCount(), address, goods, 0);
            ps.records++;
        } catch (RuntimeException e) {
            if (!warned) {
                warned = true;
                MicRtsim.LOGGER.warn("[mic_rtsim] create: a chain package at {} was not counted", f.getBlockPos(), e);
            }
        }
    }

    private static boolean warned, tooMany;

    /**
     * Journals and commits one IND_CHAIN_PACKAGE: {@code packages} packages with {@code goods} in all at the
     * store's frogport {@code index}. {@code opId}: 0 for what Create moved, the engine's op for what an
     * IND_WRITE_CHAIN wrote ({@link WriteChains}). Returns the committed record.
     */
    static ByteBuffer journalChain(Industry ind, Store s, int index, District d, District.Inventory inv, int direction, int destination,
            int packages, String address, Industry.GoodsCounter goods, long opId) {
        int[] ids = goods.nonZeroIds();
        ByteBuffer r = ind.journal(IndRecords.IND_CHAIN_PACKAGE, IndRecords.ChainPackage.size(ids.length), ids.length);
        r.putLong(IndRecords.ChainPackage.STORE, s.id);
        r.putLong(IndRecords.ChainPackage.DISTRICT, d == null ? 0 : d.id);
        r.putInt(IndRecords.ChainPackage.INVENTORY, inv == null ? IndRecords.NO_INVENTORY : inv.index());
        r.putInt(IndRecords.ChainPackage.DIRECTION, direction);
        r.putInt(IndRecords.ChainPackage.FROGPORT, index);
        r.putInt(IndRecords.ChainPackage.DESTINATION, destination);
        r.putInt(IndRecords.ChainPackage.PACKAGES, packages);
        r.putInt(IndRecords.ChainPackage.ADDRESS, ind.nameId(address));
        r.putLong(IndRecords.ChainPackage.OP_ID, opId);
        r.putLong(IndRecords.ChainPackage.OP_ID + 8, 0);
        for (int i = 0; i < ids.length; i++) {
            int at = IndRecords.ChainPackage.ENTRIES + i * IndRecords.ENTRY16;
            r.putInt(at, ids[i]);
            r.putInt(at + 4, 0);
            r.putLong(at + 8, goods.get(ids[i]));
        }
        ind.commit(r);
        return r;
    }

    /** The store {@code id} names, or null. */
    Store store(long id) {
        return stores.get(id);
    }


    // ---- the census -----------------------------------------------------------------------------

    /** Once per tick (CreatePlugin.tick): the census of each store that is due and whose chunks all tick. */
    public void tick(long now) {
        if (stores.isEmpty())
            return;
        for (Store s : new ArrayList<>(stores.values())) {
            if (now < s.nextCensus || !stores.containsKey(s.id))
                continue;
            s.nextCensus = now + CENSUS_TICKS;
            ServerLevel level = ind.level(s.dim);
            if (level == null || s.conveyors.isEmpty() || !ticks(level, s))
                continue;
            Observed o = observe(level, BlockPos.of(s.conveyors.iterator().nextLong()), 0);
            if (o.store() != null && o.walk().complete)
                census(level, o.store(), o.walk(), now);
        }
    }

    /** Whether every chunk of the store's conveyors and frogports ticks. */
    private static boolean ticks(ServerLevel level, Store s) {
        LongOpenHashSet chunks = new LongOpenHashSet();
        for (long c : s.conveyors)
            chunks.add(ChunkPos.asLong(BlockPos.getX(c) >> 4, BlockPos.getZ(c) >> 4));
        for (Port p : s.ports)
            chunks.add(ChunkPos.asLong(p.pos.getX() >> 4, p.pos.getZ() >> 4));
        for (long ch : chunks)
            if (!Loaded.ticks(level, new BlockPos(ChunkPos.getX(ch) << 4, level.getMinBuildHeight(), ChunkPos.getZ(ch) << 4)))
                return false;
        return true;
    }

    private void census(ServerLevel level, Store s, Walk w, long now) {
        int n = s.ports.size();
        int[][] dist = new int[n][];
        for (int i = 0; i < n; i++)
            dist[i] = w.bfs(w.of(s.ports.get(i).conveyor));
        long[] packages = new long[n + 1];
        Industry.GoodsCounter[] goods = new Industry.GoodsCounter[n + 1];
        for (int c = 0; c < w.order.size(); c++) {
            if (!(Loaded.blockEntity(level, BlockPos.of(w.order.getLong(c))) instanceof ChainConveyorBlockEntity cc))
                continue;
            for (ChainConveyorPackage p : cc.getLoopingPackages())
                count(s, dist, c, p.item, packages, goods);
            for (Map.Entry<BlockPos, List<ChainConveyorPackage>> e : cc.getTravellingPackages().entrySet()) {
                int next = w.of(cc.getBlockPos().offset(e.getKey()));
                for (ChainConveyorPackage p : e.getValue())
                    count(s, dist, next >= 0 ? next : c, p.item, packages, goods);
            }
        }
        for (int i = 0; i < n; i++) {
            if (Loaded.blockEntity(level, s.ports.get(i).pos) instanceof FrogportBlockEntity f && f.isAnimationInProgress()
                    && !f.currentlyDepositing && f.animatedPackage != null && !f.animatedPackage.isEmpty())
                add(i, f.animatedPackage, packages, goods);
        }
        int m = 0, entries = 0;
        List<int[]> ids = new ArrayList<>();
        for (int d = 0; d <= n; d++) {
            if (packages[d] > 0)
                m++;
            int[] g = goods[d] == null ? new int[0] : goods[d].nonZeroIds();
            ids.add(g);
            entries += g.length;
        }
        int size = IndRecords.StoreCensus.size(m, entries);
        if (size > com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GraphBuilder.MAX_RECORD)
            return;
        int at = ind.latest(IndRecords.IND_STORE_CENSUS, size, entries);
        if (at < 0)
            return;
        ByteBuffer b = ind.open();
        b.putLong(at + IndRecords.StoreCensus.GAME_TIME, now);
        b.putLong(at + IndRecords.StoreCensus.STORE, s.id);
        b.putInt(at + IndRecords.StoreCensus.M, m);
        b.putInt(at + IndRecords.StoreCensus.M + 4, 0);
        int k = 0;
        for (int d = 0; d <= n; d++)
            if (packages[d] > 0) {
                int o = at + IndRecords.StoreCensus.PACKAGES + IndRecords.StoreCensus.PAIR * k++;
                b.putInt(o, d == n ? IndRecords.NO_INVENTORY : d);
                b.putInt(o + 4, (int) Math.min(Integer.MAX_VALUE, packages[d]));
            }
        k = 0;
        for (int d = 0; d <= n; d++)
            for (int id : ids.get(d)) {
                int o = at + IndRecords.StoreCensus.goodsAt(m) + 16 * k++;
                b.putInt(o, id);
                b.putInt(o + 4, d == n ? IndRecords.NO_INVENTORY : d);
                b.putLong(o + 8, goods[d].get(id));
            }
        censuses++;
    }

    private void count(Store s, int[][] dist, int c, ItemStack box, long[] packages, Industry.GoodsCounter[] goods) {
        if (box == null || box.isEmpty())
            return;
        int d = route(s, PackageItem.getAddress(box), i -> dist[i][c] < 0 ? UNKNOWN : dist[i][c], -1);
        add(d == IndRecords.NO_INVENTORY ? s.ports.size() : d, box, packages, goods);
    }

    private void add(int d, ItemStack box, long[] packages, Industry.GoodsCounter[] goods) {
        packages[d] += box.getCount();
        if (goods[d] == null)
            goods[d] = ind.counter();
        ind.countStack(box, box.getCount(), goods[d]);
    }

    public String perfLine() {
        return "stores " + stores.size() + " walks " + walks + " censuses " + censuses + " chain records " + records
                + " declarations " + declarations;
    }

    // ---- persistence (IndustryData, format 3) ----------------------------------------------------

    private void save() {
        CompoundTag t = new CompoundTag();
        t.putLong("nextId", nextId);
        ListTag list = new ListTag();
        for (Store s : stores.values()) {
            CompoundTag st = new CompoundTag();
            st.putLong("id", s.id);
            st.putString("dim", s.dim.location().toString());
            ListTag ports = new ListTag();
            for (Port p : s.ports) {
                CompoundTag pt = new CompoundTag();
                pt.putLong("pos", p.pos.asLong());
                pt.putLong("conveyor", p.conveyor.asLong());
                if (p.filter != null)
                    pt.putString("filter", p.filter);
                ports.add(pt);
            }
            st.put("ports", ports);
            st.putIntArray("hops", s.hops);
            st.putLongArray("conveyors", s.conveyors.toLongArray());
            list.add(st);
        }
        t.put("stores", list);
        ind.data().putPlugin(KEY, t);
    }

    private void load(CompoundTag t) {
        if (t == null)
            return;
        nextId = Math.max(1, t.getLong("nextId"));
        for (Tag raw : t.getList("stores", Tag.TAG_COMPOUND)) {
            CompoundTag st = (CompoundTag) raw;
            ResourceLocation loc = ResourceLocation.tryParse(st.getString("dim"));
            if (loc == null)
                continue;
            Store s = new Store(st.getLong("id"), ResourceKey.create(Registries.DIMENSION, loc));
            for (Tag pr : st.getList("ports", Tag.TAG_COMPOUND)) {
                CompoundTag pt = (CompoundTag) pr;
                s.ports.add(new Port(BlockPos.of(pt.getLong("pos")), BlockPos.of(pt.getLong("conveyor")),
                        pt.contains("filter") ? pt.getString("filter") : null));
            }
            s.hops = st.getIntArray("hops");
            if (s.hops.length != IndRecords.PackageStore.triangle(s.ports.size())) {
                // Kept with its id (the engine has it), the hops unknown until the next walk.
                s.hops = new int[IndRecords.PackageStore.triangle(s.ports.size())];
                Arrays.fill(s.hops, UNKNOWN);
            }
            for (long c : st.getLongArray("conveyors"))
                s.conveyors.add(c);
            stores.put(s.id, s);
            index(s);
            nextId = Math.max(nextId, s.id + 1);
        }
    }
}
