package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.minecraftindustrialcomplex.mic_rtsim_create.mixin.ChainConveyorRoutingTableAccessor;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorBlockEntity;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorPackage;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorRoutingTable;
import com.simibubi.create.content.logistics.box.PackageItem;
import it.unimi.dsi.fastutil.longs.Long2ObjectMap;
import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.longs.LongArrayList;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import net.minecraft.core.BlockPos;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.Tag;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.component.CustomData;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.chunk.LevelChunk;
import org.apache.commons.lang3.mutable.MutableInt;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ThreadLocalRandom;

/**
 * The frontier of a package store whose far side does not tick (SECTIONS.md §2.5 "One end loaded, the other
 * away", Q5; slice S4c1). Slice S1's guards already refuse a hand-off into a conveyor that does not tick
 * ({@code ChainConveyorBlockEntityMixin}); this keeps the routes that lead there and holds the packages that
 * follow them.
 *
 * <ul>
 *   <li><b>Routes kept</b> (choice SC135). Create's route entries live 100 ticks unless the next conveyor's
 *       advert refreshes them, and a conveyor that does not tick sends none, so a far frogport's route
 *       expires on the boundary conveyor (the last one that ticks) and every package for it circles with no
 *       route. Just before the tick that would expire it ({@link #beforeRoutes}, before
 *       {@code ChainConveyorRoutingTable.tick}), an entry whose next conveyor does not tick and whose port is
 *       a far frogport of the conveyor's store (one with that filter whose chunk does not tick) is given
 *       Create's full timeout again and kept, with its port, distance, connection and store. Every
 *       {@link #KEEP_TICKS} each conveyor with kept routes inserts them again (Create's private
 *       {@code insert}, which refreshes an identical entry), so they also come back after a restart, when
 *       Create's tables start empty; Create's own adverts carry them along the near side. A kept route goes
 *       when its connection or store is gone or no far frogport of the store has its filter; when its next
 *       conveyor ticks again (the boundary moved out), it moves on along the chain to the new boundary
 *       conveyor ({@link #handOver}).</li>
 *   <li><b>The hold</b> (choice SC138). Where the guard refuses a hand-off ({@link #hold}), a package whose
 *       route is a far frogport's gets a key in vanilla's {@code minecraft:custom_data} ({@link #TAG}: the
 *       store, the epoch, a serial; choice SC136) and one {@code CHAIN_HELD}, whose frogport and destination
 *       are that far frogport and whose position is the boundary conveyor. A key Java holds is followed,
 *       never stripped: it is told again ({@code CHAIN_HELD}, the same key) when its store or destination
 *       changed, and once in each session (a crash can lose a record its chunk kept). Any other key is
 *       stripped first. A package leaves the hold where a frogport takes it off the chain
 *       ({@code FrogportChainMixin}, whose {@code CHAIN_IN} then carries the key), where Create drops it or
 *       a player takes it ({@code CHAIN_RELEASED}), and at a census, which releases every key of the store
 *       its walk did not find (choice SC137).</li>
 *   <li><b>After a crash</b> (choice SC140). {@code IndustryData} and chunks can be saved ahead of the journal,
 *       so either side can hold a key no package has. Once a session Java lists the keys it holds
 *       ({@link #listOnce}, IND_HELD_LIST), and the engine strikes any other key of the epoch; a conveyor's first
 *       tick looks among its packages for those last held on it and releases the ones gone ({@link #firstTick}).
 *       Conveyor tables are read only at the tick their soonest entry may expire and on the conveyor's turn
 *       ({@link #beforeRoutes}).</li>
 *   <li><b>The draw</b> (slice S4c2, choices SC141 to SC143). The far district's replay takes the goods of
 *       packages held toward its frogport: the engine asks ({@code IND_HELD_DRAW}, {@link #heldDraw}) and Java
 *       marks each key drawn and answers {@code CHAIN_DRAWN}. A drawn package circles its boundary conveyor
 *       whatever ticks ahead ({@link #refusesDrawn}), until the engine has it removed (Remove: taken off the
 *       chain, at once or at the conveyor's first tick once it loads, and answered {@code CHAIN_RELEASED}) or
 *       gives it back (Return: held again, and told again). A drawn package a frogport takes off the chain
 *       lands as any held one (its goods go back into the replay's pool first, the engine's).</li>
 *   <li><b>The epoch</b> (choice SC136). It starts anywhere (a random nonzero value, so an engine reset that
 *       also lost this state never meets its old keys again), changes at every announce (the engine forgot
 *       what it was told) and reaches the engine in each IND_PACKAGE_STORE. Keys of any other epoch are not
 *       Java's: their packages are keyed again where they are held next.</li>
 * </ul>
 * The state persists in {@code IndustryData} under {@link #KEY}, updated in place at each change (one tag per
 * held package), apart from the stores' own state, which is written whole.
 */
public final class Frontier {
    /** The key of the frontier's state in {@code IndustryData}. */
    public static final String KEY = "mic_rtsim_create:frontier";
    /** The key's name inside a package's {@code minecraft:custom_data}. */
    public static final String TAG = "mic_rtsim_held";
    /** TUNABLE (choice SC135): ticks between two inserts of a conveyor's kept routes. */
    public static final int KEEP_TICKS = 20;

    /** TUNABLE (choice SC135): at most this many conveyors a kept route moves on when its next conveyor ticks again. */
    static final int HAND_OVER_MAX = 64;

    /** A kept route on a boundary conveyor: Create's entry, as it was when it would have expired, and its store. */
    public record Kept(String port, int distance, BlockPos connection, long store) {}

    /**
     * A package Java holds: its store, the far frogport it heads for, the boundary conveyor it circles; whether
     * the far replay drew it (it circles there until removed or given back), and whether its removal waits for
     * that conveyor to load (slice S4c2).
     */
    public record Held(long store, ResourceKey<Level> dim, BlockPos destination, BlockPos at, boolean drawn, boolean removing) {
        Held(long store, ResourceKey<Level> dim, BlockPos destination, BlockPos at) {
            this(store, dim, destination, at, false, false);
        }

        Held inStore(long s) {
            return new Held(s, dim, destination, at, drawn, removing);
        }

        Held marked(boolean drawn, boolean removing) {
            return new Held(store, dim, destination, at, drawn, removing);
        }
    }

    /** A package's key. */
    public record Key(long store, int epoch, long serial) {}

    private final Industry ind;
    private final PackageStores stores;
    int epoch;
    private long serialHigh;
    private int serialLow;
    private final Map<ResourceKey<Level>, Long2ObjectOpenHashMap<List<Kept>>> kept = new HashMap<>();
    private final Long2ObjectOpenHashMap<Held> held = new Long2ObjectOpenHashMap<>();
    /** The serials told to the engine in this session (a held key is told once more in each). */
    private final LongOpenHashSet sent = new LongOpenHashSet();
    /** The serial of the last keyed {@code CHAIN_IN} ({@link #landed}), for {@link #afterLanding}. */
    private long lastLanded;
    /** Whether this session's IND_HELD_LIST went out ({@link #listOnce}). */
    private boolean listed;
    /**
     * The held packages loaded from {@code IndustryData} that no hook has seen in this session, and the same by
     * the boundary conveyor they were last held on: that conveyor's first tick looks for them ({@link #firstTick}).
     */
    private final LongOpenHashSet unverified = new LongOpenHashSet();
    private final Map<ResourceKey<Level>, Long2ObjectOpenHashMap<LongArrayList>> unverifiedAt = new HashMap<>();
    /** The drawn packages Java holds ({@link #refusesDrawn} looks no further while there are none). */
    private int drawnCount;
    /** The drawn packages whose removal waits for their conveyor's first tick, by that conveyor. */
    private final Map<ResourceKey<Level>, Long2ObjectOpenHashMap<LongArrayList>> removingAt = new HashMap<>();
    /** GameTests: Return asks are not applied (the engine gives back a draw no replay of its own covers). */
    public static volatile boolean ignoreReturnsForTest;
    /** The saved state, kept up to date in place: {@code epoch}, {@code kept} (written whole), {@code held} (by serial). */
    private final CompoundTag tag;
    // Counters (perfLine).
    long captured, insertedAgain, dropped, handedOver, holds, followed, landed, released, stripped, unfound, drawn, removed, returned;

    Frontier(Industry ind, PackageStores stores) {
        this.ind = ind;
        this.stores = stores;
        CompoundTag t = ind.data().plugin(KEY);
        tag = t == null ? new CompoundTag() : t;
        load();
        newSerials();
        if (epoch == 0) {
            epoch = randomEpoch();
            tag.putInt("epoch", epoch);
            save();
        }
    }

    private static int randomEpoch() {
        int e;
        do
            e = ThreadLocalRandom.current().nextInt();
        while (e == 0);
        return e;
    }

    /** A new run of serials: a random high half, so no serial of an earlier session (saved or not) comes back. */
    private void newSerials() {
        serialHigh = (long) randomEpoch() << 32;
        serialLow = 1;
    }

    private long nextSerial() {
        if (serialLow == 0)
            newSerials();
        return serialHigh | (serialLow++ & 0xFFFFFFFFL);
    }

    /** The epoch every key of this session carries. */
    public int epoch() {
        return epoch;
    }

    /** A new epoch (the engine forgot what it was told): no key Java gave out before is Java's any more. */
    void newEpoch() {
        int e = epoch + 1;
        epoch = e == 0 ? 1 : e;
        held.clear();
        sent.clear();
        unverified.clear();
        unverifiedAt.clear();
        drawnCount = 0;
        removingAt.clear();
        tag.putInt("epoch", epoch);
        tag.put("held", new CompoundTag());
        save();
    }

    /** The kept routes of the conveyor at {@code pos}, or null (for tests and the hold). */
    public List<Kept> keptAt(ResourceKey<Level> dim, BlockPos pos) {
        Long2ObjectOpenHashMap<List<Kept>> m = kept.get(dim);
        return m == null ? null : m.get(pos.asLong());
    }

    /** The packages Java holds, by serial (for tests). */
    public Map<Long, Held> held() {
        return java.util.Collections.unmodifiableMap(held);
    }

    // ---- routes kept (ChainConveyorBlockEntityMixin, before the table's tick) --------------------

    /**
     * Before {@code cc}'s routing table ticks: keeps an entry about to expire toward a far frogport, and every
     * {@link #KEEP_TICKS} inserts the conveyor's kept routes again; on the conveyor's first tick since it loaded,
     * looks for the packages last held on it ({@link #firstTick}). {@code scanAt} is the conveyor's own
     * ({@code ChainConveyorBlockEntityMixin}: 0 before its first tick): the game time of the next tick an entry
     * may expire at. Its table is not read before then (an entry's timeout only goes down by one a tick, and a
     * new or refreshed one starts at Create's full timeout), nor is anything looked up unless that tick or the
     * conveyor's turn has come. Returns the next {@code scanAt}.
     */
    public static long beforeRoutes(ChainConveyorBlockEntity cc, ChainConveyorRoutingTable table, long scanAt) {
        if (!(cc.getLevel() instanceof ServerLevel level))
            return scanAt;
        try {
            BlockPos pos = cc.getBlockPos();
            long now = level.getGameTime();
            boolean first = scanAt == 0;
            boolean turn = Math.floorMod(now + pos.hashCode(), KEEP_TICKS) == 0;
            if (!first && !turn && now < scanAt)
                return scanAt;
            Industry ind = IndustryApi.current();
            if (ind == null)
                return scanAt;
            Frontier f = PackageStores.of(ind).frontier;
            if (first)
                f.firstTick(level, cc);
            long next = first || now >= scanAt ? f.scan(level, cc, table, now) : scanAt;
            if (turn)
                f.insertAgain(level, cc, table);
            return next;
        } catch (RuntimeException e) {
            if (!warned) {
                warned = true;
                MicRtsim.LOGGER.warn("[mic_rtsim] create: the frontier's routes at {} were not kept", cc.getBlockPos(), e);
            }
            return scanAt;
        }
    }

    /**
     * Keeps the entries of {@code table} that expire at this tick and lead to a far frogport ({@link #capture});
     * returns the game time of the next tick an entry may expire at.
     */
    private long scan(ServerLevel level, ChainConveyorBlockEntity cc, ChainConveyorRoutingTable table, long now) {
        boolean expiring = false;
        int min = ChainConveyorRoutingTable.ENTRY_TIMEOUT;
        for (ChainConveyorRoutingTable.RoutingTableEntry e : table.entriesByDistance) {
            if (e.endOfRoute())
                continue;
            int t = e.timeout().intValue();
            if (t <= 1)
                expiring = true;
            else
                min = Math.min(min, t);
        }
        if (expiring)
            capture(level, cc, table);
        // An entry of timeout t now has 1 left (and expires) at the tick t - 1 from now.
        return now + min - 1;
    }

    private static boolean warned;

    private void capture(ServerLevel level, ChainConveyorBlockEntity cc, ChainConveyorRoutingTable table) {
        BlockPos pos = cc.getBlockPos();
        PackageStores.Store s = null;
        for (ChainConveyorRoutingTable.RoutingTableEntry e : table.entriesByDistance) {
            if (e.endOfRoute() || e.timeout().intValue() > 1 || !cc.connections.contains(e.nextConnection())
                    || Loaded.ticksFrom(level, pos, pos.offset(e.nextConnection())))
                continue;
            if (s == null)
                s = storeHere(level.dimension(), pos);
            if (s == null)
                return;
            if (awayPort(level, s, e.port()) < 0)
                continue;
            e.timeout().setValue(ChainConveyorRoutingTable.ENTRY_TIMEOUT);
            keep(level.dimension(), pos, new Kept(e.port(), e.distance(), e.nextConnection().immutable(), s.id));
        }
    }

    private void keep(ResourceKey<Level> dim, BlockPos pos, Kept k) {
        List<Kept> l = kept.computeIfAbsent(dim, d -> new Long2ObjectOpenHashMap<>()).computeIfAbsent(pos.asLong(), p -> new ArrayList<>());
        for (int i = 0; i < l.size(); i++)
            if (l.get(i).port().equals(k.port()) && l.get(i).connection().equals(k.connection())) {
                if (l.get(i).equals(k))
                    return;
                l.set(i, k);
                saveKept();
                return;
            }
        l.add(k);
        captured++;
        saveKept();
    }

    /**
     * The store of the chain conveyor at {@code pos}: its last walk's, or else the store of a route kept there (a
     * boundary conveyor that was not loaded at the last walk, which a kept route moved on to, {@link #handOver}).
     */
    private PackageStores.Store storeHere(ResourceKey<Level> dim, BlockPos pos) {
        PackageStores.Store s = stores.storeOfConveyor(dim, pos);
        if (s != null)
            return s;
        List<Kept> l = keptAt(dim, pos);
        if (l != null)
            for (Kept k : l) {
                s = stores.store(k.store());
                if (s != null)
                    return s;
            }
        return null;
    }

    /**
     * The first tick of {@code cc} since it loaded: the packages {@code IndustryData} says were last held on it,
     * and that no hook has seen in this session, are looked for among its packages, and released when not there
     * (choice SC137: a chunk saved, and then a crash before {@code IndustryData} was, lost the package or its
     * release).
     */
    private void firstTick(ServerLevel level, ChainConveyorBlockEntity cc) {
        if (!unverified.isEmpty())
            verify(level, cc);
        if (!removingAt.isEmpty())
            removeWaiting(level, cc);
    }

    private void verify(ServerLevel level, ChainConveyorBlockEntity cc) {
        Long2ObjectOpenHashMap<LongArrayList> m = unverifiedAt.get(level.dimension());
        LongArrayList l = m == null ? null : m.remove(cc.getBlockPos().asLong());
        if (l == null)
            return;
        if (m.isEmpty())
            unverifiedAt.remove(level.dimension());
        LongOpenHashSet here = new LongOpenHashSet();
        for (ChainConveyorPackage p : cc.getLoopingPackages())
            keyInto(p.item, here);
        for (List<ChainConveyorPackage> t : cc.getTravellingPackages().values())
            for (ChainConveyorPackage p : t)
                keyInto(p.item, here);
        BlockPos pos = cc.getBlockPos();
        for (int i = 0; i < l.size(); i++) {
            long serial = l.getLong(i);
            if (!unverified.remove(serial) || here.contains(serial))
                continue;
            Held h = held.get(serial);
            if (h == null || !h.at().equals(pos) || !h.dim().equals(level.dimension()))
                continue;
            unfound++;
            release(new Key(h.store(), epoch, serial), pos);
        }
    }

    /** The drawn packages whose removal waited for {@code cc} to load: taken off it now ({@link #take}). */
    private void removeWaiting(ServerLevel level, ChainConveyorBlockEntity cc) {
        Long2ObjectOpenHashMap<LongArrayList> m = removingAt.get(level.dimension());
        LongArrayList l = m == null ? null : m.remove(cc.getBlockPos().asLong());
        if (l == null)
            return;
        if (m.isEmpty())
            removingAt.remove(level.dimension());
        for (int i = 0; i < l.size(); i++) {
            long serial = l.getLong(i);
            Held h = held.get(serial);
            if (h != null && h.removing() && h.at().equals(cc.getBlockPos()) && h.dim().equals(level.dimension()))
                take(level, cc, serial, h);
        }
    }

    /** GameTests: {@code cc}'s first tick after a restart that {@link PackageStores#forgetForTest} emulates. */
    public static void firstTickForTest(ChainConveyorBlockEntity cc) {
        PackageStores.of(IndustryApi.current()).frontier.firstTick((ServerLevel) cc.getLevel(), cc);
    }

    private void keyInto(ItemStack box, LongOpenHashSet into) {
        Key k = box == null || box.isEmpty() ? null : key(box);
        if (k != null && k.epoch() == epoch)
            into.add(k.serial());
    }

    private void insertAgain(ServerLevel level, ChainConveyorBlockEntity cc, ChainConveyorRoutingTable table) {
        Long2ObjectOpenHashMap<List<Kept>> m = kept.get(level.dimension());
        BlockPos pos = cc.getBlockPos();
        List<Kept> l = m == null ? null : m.get(pos.asLong());
        if (l == null)
            return;
        PackageStores.Store here = stores.storeOfConveyor(level.dimension(), pos);
        boolean changed = false;
        for (int i = l.size() - 1; i >= 0; i--) {
            Kept k = l.get(i);
            PackageStores.Store s = here != null && here.id == k.store() ? here : stores.store(k.store());
            if (s == null || !cc.connections.contains(k.connection()) || awayPort(level, s, k.port()) < 0) {
                l.remove(i);
                dropped++;
                changed = true;
                continue;
            }
            if (Loaded.ticksFrom(level, pos, pos.offset(k.connection()))) {
                l.remove(i);
                changed = true;
                BlockPos to = handOver(level, s, pos, k);
                if (to == null)
                    dropped++;
                else
                    handedOver++;
                continue;
            }
            ((ChainConveyorRoutingTableAccessor) table).mic_rtsim$insert(new ChainConveyorRoutingTable.RoutingTableEntry(k.port(), k.distance(),
                    k.connection(), new MutableInt(ChainConveyorRoutingTable.ENTRY_TIMEOUT), false));
            insertedAgain++;
        }
        if (l.isEmpty())
            m.remove(pos.asLong());
        if (changed)
            saveKept();
    }

    /**
     * Kept route {@code k} of the conveyor at {@code from}, whose next conveyor ticks again (the boundary moved
     * out): it moves on along the chain while each conveyor has one way on (but the way back) and that way
     * ticks, and is kept, and inserted, on the first conveyor whose way on does not tick, the new boundary,
     * one hop nearer for each conveyor passed; Create's adverts then carry it back. Returns that conveyor, or
     * null when it goes: a conveyor on the way has a route of the port already (Create's adverts reach it),
     * the way forks or ends, it reaches the conveyor of a frogport with the port's filter (whatever its distance
     * says: a {@code *} port's counts from 1000), or more than {@link #HAND_OVER_MAX} conveyors.
     */
    private BlockPos handOver(ServerLevel level, PackageStores.Store s, BlockPos from, Kept k) {
        BlockPos back = from;
        BlockPos at = from.offset(k.connection());
        int distance = k.distance() - 1;
        for (int step = 0; step < HAND_OVER_MAX && distance > 0; step++) {
            if (portConveyor(s, k.port(), at))
                return null;
            if (!(Loaded.blockEntity(level, at) instanceof ChainConveyorBlockEntity next))
                return null;
            for (ChainConveyorRoutingTable.RoutingTableEntry e : next.routingTable.entriesByDistance)
                if (e.port().equals(k.port()))
                    return null;
            BlockPos onward = null;
            for (BlockPos c : next.connections) {
                if (at.offset(c).equals(back))
                    continue;
                if (onward != null)
                    return null;
                onward = c;
            }
            if (onward == null)
                return null;
            if (!Loaded.ticksFrom(level, at, at.offset(onward))) {
                Kept moved = new Kept(k.port(), distance, onward.immutable(), s.id);
                keep(level.dimension(), at, moved);
                ((ChainConveyorRoutingTableAccessor) next.routingTable).mic_rtsim$insert(new ChainConveyorRoutingTable.RoutingTableEntry(
                        moved.port(), moved.distance(), moved.connection(), new MutableInt(ChainConveyorRoutingTable.ENTRY_TIMEOUT), false));
                return at.immutable();
            }
            back = at;
            at = at.offset(onward);
            distance--;
        }
        return null;
    }

    /** Whether {@code at} is the conveyor of a frogport of {@code s} whose filter is {@code port}. */
    private static boolean portConveyor(PackageStores.Store s, String port, BlockPos at) {
        for (PackageStores.Port p : s.ports)
            if (port.equals(p.filter) && at.equals(p.conveyor))
                return true;
        return false;
    }

    /** A chain conveyor is destroyed: its kept routes go (its packages are released as Create drops them, {@link #leaves}). */
    public static void destroyed(ChainConveyorBlockEntity cc) {
        if (!(cc.getLevel() instanceof ServerLevel level))
            return;
        Industry ind = IndustryApi.current();
        if (ind == null)
            return;
        Frontier f = PackageStores.of(ind).frontier;
        Long2ObjectOpenHashMap<List<Kept>> m = f.kept.get(level.dimension());
        if (m != null && m.remove(cc.getBlockPos().asLong()) != null)
            f.saveKept();
    }

    /**
     * The index of the first frogport of {@code s} (in position order) whose filter is {@code port} and whose
     * chunk does not tick, or -1.
     */
    static int awayPort(ServerLevel level, PackageStores.Store s, String port) {
        for (int i = 0; i < s.ports.size(); i++) {
            PackageStores.Port p = s.ports.get(i);
            if (port.equals(p.filter) && !Loaded.ticks(level, p.pos))
                return i;
        }
        return -1;
    }

    // ---- the hold (ChainConveyorBlockEntityMixin, at a refused hand-off) --------------------------

    /**
     * The guard refused {@code box}'s hand-off from {@code cc} toward {@code exit}, whose conveyor does not
     * tick: when its route (the table's first entry its address matches, as {@code getExitFor} reads it) is a
     * far frogport of the conveyor's store, it is held there, keyed and recorded once.
     */
    public static void hold(ChainConveyorBlockEntity cc, ChainConveyorRoutingTable table, ItemStack box) {
        if (!(cc.getLevel() instanceof ServerLevel level) || box == null || box.isEmpty())
            return;
        try {
            Industry ind = IndustryApi.current();
            if (ind == null)
                return;
            PackageStores ps = PackageStores.of(ind);
            ps.frontier.hold(level, cc, table, box);
        } catch (RuntimeException e) {
            if (!warnedHold) {
                warnedHold = true;
                MicRtsim.LOGGER.warn("[mic_rtsim] create: a package held at {} was not recorded", cc.getBlockPos(), e);
            }
        }
    }

    private static boolean warnedHold;

    private void hold(ServerLevel level, ChainConveyorBlockEntity cc, ChainConveyorRoutingTable table, ItemStack box) {
        BlockPos pos = cc.getBlockPos();
        ResourceKey<Level> dim = level.dimension();
        Key k = key(box);
        Held h = k != null && k.epoch() == epoch ? held.get(k.serial()) : null;
        // Held here and told in this session: nothing to do (each loop past the refused exit comes here).
        if (h != null && h.at().equals(pos) && h.dim().equals(dim) && sent.contains(k.serial()))
            return;
        if (h != null)
            unverified.remove(k.serial());
        // Drawn: it circles here ({@link #refusesDrawn} answers first), whatever its route says now.
        if (h != null && h.drawn())
            return;
        if (k != null && h == null) {
            // Not Java's: another epoch's, or a key Java no longer holds (the engine may: released).
            strip(box);
            stripped++;
            cc.setChanged();
            release(k, pos);
        }
        PackageStores.Store s = storeHere(dim, pos);
        int dest = s == null ? -1 : routedAway(level, s, table, box);
        if (dest < 0 && h != null) {
            // No route here to a far frogport now: a held package stays held toward where it was.
            PackageStores.Store was = stores.store(h.store());
            int i = was == null ? -1 : was.indexOf(h.destination());
            if (i >= 0 && !Loaded.ticks(level, was.ports.get(i).pos)) {
                s = was;
                dest = i;
            }
        }
        if (dest < 0) {
            if (h != null) {
                strip(box);
                stripped++;
                cc.setChanged();
                release(k, pos);
            }
            return;
        }
        BlockPos destination = s.ports.get(dest).pos;
        long serial;
        if (h != null) {
            serial = k.serial();
            boolean moved = h.store() != s.id || !h.destination().equals(destination);
            if (moved || !h.at().equals(pos) || !h.dim().equals(dim))
                putHeld(serial, new Held(s.id, dim, destination, pos.immutable(), false, h.removing()));
            if (!moved && sent.contains(serial))
                return;
            followed++;
        } else {
            serial = nextSerial();
            CompoundTag kt = new CompoundTag();
            kt.putLong("store", s.id);
            kt.putInt("epoch", epoch);
            kt.putLong("serial", serial);
            CustomData.update(DataComponents.CUSTOM_DATA, box, t -> t.put(TAG, kt));
            cc.setChanged();
            putHeld(serial, new Held(s.id, dim, destination, pos.immutable()));
            holds++;
        }
        sent.add(serial);
        Industry.GoodsCounter goods = ind.counter();
        ind.countStack(box, box.getCount(), goods);
        PackageStores.journalChain(ind, s, dest, pos, null, null, IndRecords.ChainPackage.CHAIN_HELD, dest, box.getCount(),
                PackageItem.getAddress(box), goods, 0, epoch, serial);
        if (h == null) {
            // The record speaks for the key in this chunk: the journal's next file waits for the chunk (R21).
            LevelChunk chunk = level.getChunkSource().getChunkNow(pos.getX() >> 4, pos.getZ() >> 4);
            if (chunk != null)
                ind.journalDependsOn(chunk);
        }
    }

    /**
     * The far frogport of {@code s} that {@code box}'s route from {@code table} leads to (the first entry its
     * address matches, as {@code getExitFor} reads it), or -1.
     */
    private static int routedAway(ServerLevel level, PackageStores.Store s, ChainConveyorRoutingTable table, ItemStack box) {
        for (ChainConveyorRoutingTable.RoutingTableEntry e : table.entriesByDistance)
            if (PackageItem.matchAddress(box, e.port()))
                return awayPort(level, s, e.port());
        return -1;
    }

    /** A package's key, or null. */
    public static Key key(ItemStack box) {
        CustomData cd = box.get(DataComponents.CUSTOM_DATA);
        if (cd == null)
            return null;
        @SuppressWarnings("deprecation")
        CompoundTag t = cd.getUnsafe();
        if (!t.contains(TAG, Tag.TAG_COMPOUND))
            return null;
        CompoundTag k = t.getCompound(TAG);
        return new Key(k.getLong("store"), k.getInt("epoch"), k.getLong("serial"));
    }

    /** Takes the key out of a package (its custom data goes when nothing else is in it). */
    public static void strip(ItemStack box) {
        CustomData.update(DataComponents.CUSTOM_DATA, box, t -> t.remove(TAG));
    }

    /**
     * Java no longer holds the package keyed {@code k}, last at {@code pos}: one {@code CHAIN_RELEASED} for a key of
     * this epoch (the engine may hold it even when Java does not: a crash between their saves), none for another
     * epoch's (the engine dropped those at the declaration that changed it).
     */
    private void release(Key k, BlockPos pos) {
        if (k == null || k.epoch() != epoch)
            return;
        Held h = held.get(k.serial());
        if (h != null)
            removeHeld(k.serial());
        PackageStores.journalRelease(ind, h != null ? h.store() : k.store(), pos, epoch, k.serial());
        released++;
    }

    /**
     * A keyed package leaves the chain otherwise than into a frogport: Create drops it (its conveyor destroyed or a
     * connection removed, {@code ChainConveyorBlockEntityMixin}) or a player takes it
     * ({@code ChainPackageInteractionPacketMixin}). Its key is stripped and released.
     */
    public static void leaves(Level level, BlockPos pos, ItemStack box) {
        if (!(level instanceof ServerLevel) || box == null || box.isEmpty())
            return;
        try {
            Key k = key(box);
            if (k == null)
                return;
            strip(box);
            Industry ind = IndustryApi.current();
            if (ind == null)
                return;
            Frontier f = PackageStores.of(ind).frontier;
            f.stripped++;
            f.release(k, pos);
        } catch (RuntimeException e) {
            if (!warnedLeaves) {
                warnedLeaves = true;
                MicRtsim.LOGGER.warn("[mic_rtsim] create: a held package that left the chain at {} was not released", pos, e);
            }
        }
    }

    private static boolean warnedLeaves;

    /**
     * A frogport took a package keyed {@code k} off the chain (the key taken out already): the key, when Java holds
     * it in this epoch (its {@code CHAIN_IN} carries it, and then {@link #landed} ends the hold), else null. A key moves
     * between stores with its frogport, so the one in the package may name another store than Java's.
     */
    Key heldKey(Key k) {
        return k == null || k.epoch() != epoch || !held.containsKey(k.serial()) ? null : k;
    }

    /** The {@code CHAIN_IN} of {@link #heldKey}'s answer {@code k} is journaled: it is no longer held. */
    void landed(Key k) {
        removeHeld(k.serial());
        lastLanded = k.serial();
        landed++;
    }

    /**
     * After a frogport's insert of a package that came off the chain keyed {@code k}, at {@code pos}
     * ({@code FrogportChainMixin}): released unless its {@code CHAIN_IN} carried it (the insert left something,
     * which Create drops, or the frogport is no store's).
     */
    public static void afterLanding(Level level, BlockPos pos, Key k) {
        if (k == null || !(level instanceof ServerLevel))
            return;
        try {
            Industry ind = IndustryApi.current();
            if (ind == null)
                return;
            Frontier f = PackageStores.of(ind).frontier;
            if (f.lastLanded == k.serial() && k.epoch() == f.epoch) {
                f.lastLanded = 0;
                return;
            }
            f.release(k, pos);
        } catch (RuntimeException e) {
            if (!warnedLeaves) {
                warnedLeaves = true;
                MicRtsim.LOGGER.warn("[mic_rtsim] create: a held package that landed at {} was not released", pos, e);
            }
        }
    }

    /**
     * A census found {@code box} on the chain at {@code pos}: a key Java holds is noted in {@code seen}; any other is
     * stripped (and released when of this epoch). True when the package changed.
     */
    boolean atCensus(ItemStack box, BlockPos pos, LongOpenHashSet seen) {
        Key k = box == null || box.isEmpty() ? null : key(box);
        if (k == null)
            return false;
        if (k.epoch() == epoch && held.containsKey(k.serial())) {
            seen.add(k.serial());
            unverified.remove(k.serial());
            return false;
        }
        strip(box);
        stripped++;
        release(k, pos);
        return true;
    }

    /** A census found a package keyed {@code k} in a frogport's arriving animation: still held until it lands. */
    void seenLanding(Key k, LongOpenHashSet seen) {
        if (k != null && k.epoch() == epoch && held.containsKey(k.serial()))
            seen.add(k.serial());
    }

    /**
     * A census of store {@code s}, whose complete walk {@code w} found the held keys {@code seen}: every other key
     * held for the store, or last held on a conveyor of the walk, is released (the package left the chain some way
     * no hook saw).
     */
    void census(PackageStores.Store s, PackageStores.Walk w, LongOpenHashSet seen) {
        LongArrayList gone = null;
        for (Long2ObjectMap.Entry<Held> e : held.long2ObjectEntrySet()) {
            Held h = e.getValue();
            if (!seen.contains(e.getLongKey()) && h.dim().equals(s.dim) && (h.store() == s.id || w.of(h.at()) >= 0))
                (gone == null ? gone = new LongArrayList() : gone).add(e.getLongKey());
        }
        if (gone == null)
            return;
        for (long serial : gone) {
            Held h = held.get(serial);
            release(new Key(h.store(), epoch, serial), h.at());
        }
    }

    /** Frogport {@code destination} moved from store {@code from} to {@code to}: so do the packages held toward it. */
    void moved(long from, long to, BlockPos destination) {
        LongArrayList serials = null;
        for (Long2ObjectMap.Entry<Held> e : held.long2ObjectEntrySet())
            if (e.getValue().store() == from && e.getValue().destination().equals(destination))
                (serials == null ? serials = new LongArrayList() : serials).add(e.getLongKey());
        if (serials != null)
            for (long serial : serials) {
                Held h = held.get(serial);
                putHeld(serial, h.inStore(to));
            }
    }

    /**
     * Store {@code id} went, into {@code into} (0 for none): its held packages go with it, as the engine's do, or are
     * forgotten (the engine dropped them; a package still keyed is released and keyed again where it is held next).
     */
    void storeGone(long id, long into) {
        LongArrayList serials = null;
        for (Long2ObjectMap.Entry<Held> e : held.long2ObjectEntrySet())
            if (e.getValue().store() == id)
                (serials == null ? serials = new LongArrayList() : serials).add(e.getLongKey());
        if (serials == null)
            return;
        for (long serial : serials) {
            Held h = held.get(serial);
            if (into != 0)
                putHeld(serial, h.inStore(into));
            else
                removeHeld(serial);
        }
    }

    /**
     * Once a session (PackageStores.tick): IND_HELD_LIST, the serials Java holds in this epoch, the drawn ones last
     * (slice S4c2); the engine strikes any other key of the epoch it holds (choice SC140: a crash kept
     * {@code IndustryData} and chunks that are ahead of the journal, so the engine holds a key no package has), and
     * puts the drawn marks as Java has them (choice SC143). A list longer than one record takes is not sent (logged
     * once): the engine keeps what it holds until a release or a new epoch.
     */
    void listOnce() {
        if (listed)
            return;
        listed = true;
        int n = held.size();
        if (n > IndRecords.HeldList.MAX_SERIALS) {
            MicRtsim.LOGGER.warn("[mic_rtsim] create: the frontier holds {} packages, more than one held list takes ({}); none is sent",
                    n, IndRecords.HeldList.MAX_SERIALS);
            return;
        }
        LongArrayList undrawn = new LongArrayList(n);
        LongArrayList drawnSerials = new LongArrayList();
        for (Long2ObjectMap.Entry<Held> e : held.long2ObjectEntrySet())
            (e.getValue().drawn() ? drawnSerials : undrawn).add(e.getLongKey());
        undrawn.sort(null);
        drawnSerials.sort(null);
        undrawn.addAll(drawnSerials);
        ByteBuffer r = ind.journal(IndRecords.IND_HELD_LIST, IndRecords.HeldList.size(n), n);
        r.putInt(IndRecords.HeldList.EPOCH, epoch);
        r.putInt(IndRecords.HeldList.DRAWN, drawnSerials.size());
        for (int i = 0; i < n; i++)
            r.putLong(IndRecords.HeldList.SERIALS + 8 * i, undrawn.getLong(i));
        ind.commit(r);
    }

    // ---- the draw (IND_HELD_DRAW, slice S4c2) -------------------------------------------------------

    /**
     * Whether {@code box}, about to be handed on from {@code cc}, is a drawn package Java holds: it keeps circling
     * there ({@code ChainConveyorBlockEntityMixin}), whatever ticks ahead, until the engine has it removed or gives
     * it back (choice SC141: its goods are the far replay's meanwhile).
     */
    public static boolean refusesDrawn(ChainConveyorBlockEntity cc, ItemStack box) {
        if (!(cc.getLevel() instanceof ServerLevel) || box == null || box.isEmpty())
            return false;
        try {
            Industry ind = IndustryApi.current();
            if (ind == null)
                return false;
            Frontier f = PackageStores.of(ind).frontier;
            if (f.drawnCount == 0)
                return false;
            Key k = key(box);
            Held h = k == null || k.epoch() != f.epoch ? null : f.held.get(k.serial());
            return h != null && h.drawn();
        } catch (RuntimeException e) {
            return false;
        }
    }

    /**
     * IND_HELD_DRAW (slice S4c2; no op id, no COMMAND_RESULT): the engine draws held keys of one store and epoch
     * into the far replay, has drawn ones removed, or gives them back. Each key is answered by a record: Draw by
     * {@code CHAIN_DRAWN} (choice SC141; none while its frogport ticks, where the package goes on), Remove by
     * {@code CHAIN_RELEASED} once its package is off the chain (choice SC142), Return by nothing (held again, and
     * told again with {@code CHAIN_HELD} where it is held next, choice SC143). A key of this epoch Java does not
     * hold is released; one of another epoch is not Java's, and is skipped.
     */
    public static boolean heldDraw(Industry ind, ByteBuffer b, int at, int size, int n, long cmdSeq) {
        if (size < IndRecords.HeldDraw.SERIALS || n < 0 || IndRecords.HeldDraw.size(n) > size)
            return true;
        try {
            PackageStores.of(ind).frontier.heldDraw(b, at, n);
        } catch (RuntimeException e) {
            if (!warnedDraw) {
                warnedDraw = true;
                MicRtsim.LOGGER.warn("[mic_rtsim] create: an IND_HELD_DRAW was not applied", e);
            }
        }
        return true;
    }

    private static boolean warnedDraw;

    private void heldDraw(ByteBuffer b, int at, int n) {
        long store = b.getLong(at + IndRecords.HeldDraw.STORE);
        int action = b.getInt(at + IndRecords.HeldDraw.ACTION);
        if (b.getInt(at + IndRecords.HeldDraw.EPOCH) != epoch || (action == IndRecords.HeldDraw.RETURN && ignoreReturnsForTest))
            return;
        for (int i = 0; i < n; i++) {
            long serial = b.getLong(at + IndRecords.HeldDraw.SERIALS + 8 * i);
            Held h = held.get(serial);
            if (h == null) {
                release(new Key(store, epoch, serial), BlockPos.ZERO);
                continue;
            }
            switch (action) {
                case IndRecords.HeldDraw.DRAW -> draw(serial, h);
                case IndRecords.HeldDraw.REMOVE -> remove(serial, h);
                case IndRecords.HeldDraw.RETURN -> giveBack(serial, h);
                default -> {}
            }
        }
    }

    /** Marks a held package drawn and answers {@code CHAIN_DRAWN} (again for one drawn already: the engine may have lost it). */
    private void draw(long serial, Held h) {
        if (!h.drawn()) {
            ServerLevel level = ind.level(h.dim());
            // Its frogport ticks now: no replay covers it, and the package goes on there.
            if (level == null || Loaded.ticks(level, h.destination()))
                return;
            putHeld(serial, h.marked(true, false));
            drawn++;
        }
        PackageStores.journalKey(ind, h.store(), h.at(), IndRecords.ChainPackage.CHAIN_DRAWN, epoch, serial);
    }

    /**
     * Takes a drawn package off its conveyor ({@link #take}), or, while that conveyor is not loaded, marks it to be
     * taken at its first tick ({@link #firstTick}). One Java does not hold drawn is removed all the same: the
     * engine drew it (Java's mark was lost).
     */
    private void remove(long serial, Held h) {
        ServerLevel level = ind.level(h.dim());
        if (level == null || !Loaded.at(level, h.at())) {
            if (!h.drawn() || !h.removing()) {
                putHeld(serial, h.marked(true, true));
                removingAt.computeIfAbsent(h.dim(), d -> new Long2ObjectOpenHashMap<>())
                        .computeIfAbsent(h.at().asLong(), p -> new LongArrayList()).add(serial);
            }
            return;
        }
        if (Loaded.blockEntity(level, h.at()) instanceof ChainConveyorBlockEntity cc) {
            take(level, cc, serial, h);
            return;
        }
        // No conveyor there now (Create drops the packages of one destroyed, and they were released then).
        unfound++;
        release(new Key(h.store(), epoch, serial), h.at());
    }

    /**
     * Takes the package keyed {@code serial} off {@code cc} and answers {@code CHAIN_RELEASED}, whose next journal
     * file waits for the chunk (R21: the journal never claims a removal the chunk on disk lacks). Not there: released
     * (it left the chain some way no hook saw).
     */
    private void take(ServerLevel level, ChainConveyorBlockEntity cc, long serial, Held h) {
        boolean found = takeKeyed(cc.getLoopingPackages(), serial);
        if (!found)
            for (List<ChainConveyorPackage> t : cc.getTravellingPackages().values())
                if (takeKeyed(t, serial)) {
                    found = true;
                    break;
                }
        BlockPos pos = cc.getBlockPos();
        if (found) {
            removed++;
            cc.notifyUpdate();
        } else
            unfound++;
        release(new Key(h.store(), epoch, serial), pos);
        LevelChunk chunk = found ? Loaded.chunk(level, pos) : null;
        if (chunk != null)
            ind.journalDependsOn(chunk);
    }

    private boolean takeKeyed(List<ChainConveyorPackage> l, long serial) {
        for (int i = 0; i < l.size(); i++) {
            ItemStack box = l.get(i).item;
            Key k = box == null || box.isEmpty() ? null : key(box);
            if (k != null && k.epoch() == epoch && k.serial() == serial) {
                l.remove(i);
                return true;
            }
        }
        return false;
    }

    /** The engine gives a drawn package back: held again, and told again where it is held next. */
    private void giveBack(long serial, Held h) {
        if (h.drawn() || h.removing())
            putHeld(serial, h.marked(false, false));
        sent.remove(serial);
        returned++;
    }

    // ---- persistence (IndustryData, under KEY) ----------------------------------------------------

    private void putHeld(long serial, Held h) {
        Held old = held.put(serial, h);
        drawnCount += (h.drawn() ? 1 : 0) - (old != null && old.drawn() ? 1 : 0);
        CompoundTag e = new CompoundTag();
        e.putLong("store", h.store());
        e.putString("dim", h.dim().location().toString());
        e.putLong("destination", h.destination().asLong());
        e.putLong("at", h.at().asLong());
        if (h.drawn())
            e.putBoolean("drawn", true);
        if (h.removing())
            e.putBoolean("removing", true);
        if (!tag.contains("held", Tag.TAG_COMPOUND))
            tag.put("held", new CompoundTag());
        tag.getCompound("held").put(Long.toHexString(serial), e);
        save();
    }

    private void removeHeld(long serial) {
        Held old = held.remove(serial);
        if (old != null && old.drawn())
            drawnCount--;
        sent.remove(serial);
        unverified.remove(serial);
        tag.getCompound("held").remove(Long.toHexString(serial));
        save();
    }

    private void saveKept() {
        ListTag list = new ListTag();
        for (Map.Entry<ResourceKey<Level>, Long2ObjectOpenHashMap<List<Kept>>> d : kept.entrySet())
            for (Long2ObjectMap.Entry<List<Kept>> c : d.getValue().long2ObjectEntrySet())
                for (Kept k : c.getValue()) {
                    CompoundTag e = new CompoundTag();
                    e.putString("dim", d.getKey().location().toString());
                    e.putLong("pos", c.getLongKey());
                    e.putString("port", k.port());
                    e.putInt("distance", k.distance());
                    e.putLong("connection", k.connection().asLong());
                    e.putLong("store", k.store());
                    list.add(e);
                }
        tag.put("kept", list);
        save();
    }

    private void save() {
        ind.data().putPlugin(KEY, tag);
    }

    private void load() {
        epoch = tag.getInt("epoch");
        for (Tag raw : tag.getList("kept", Tag.TAG_COMPOUND)) {
            CompoundTag e = (CompoundTag) raw;
            ResourceKey<Level> dim = dim(e.getString("dim"));
            if (dim == null)
                continue;
            kept.computeIfAbsent(dim, d -> new Long2ObjectOpenHashMap<>()).computeIfAbsent(e.getLong("pos"), p -> new ArrayList<>())
                    .add(new Kept(e.getString("port"), e.getInt("distance"), BlockPos.of(e.getLong("connection")), e.getLong("store")));
        }
        CompoundTag h = tag.getCompound("held");
        for (String serial : h.getAllKeys()) {
            CompoundTag e = h.getCompound(serial);
            ResourceKey<Level> dim = dim(e.getString("dim"));
            if (dim == null)
                continue;
            try {
                long n = Long.parseUnsignedLong(serial, 16);
                BlockPos at = BlockPos.of(e.getLong("at"));
                Held loaded = new Held(e.getLong("store"), dim, BlockPos.of(e.getLong("destination")), at, e.getBoolean("drawn"),
                        e.getBoolean("removing"));
                held.put(n, loaded);
                if (loaded.drawn())
                    drawnCount++;
                if (loaded.removing())
                    removingAt.computeIfAbsent(dim, d -> new Long2ObjectOpenHashMap<>()).computeIfAbsent(at.asLong(), p -> new LongArrayList()).add(n);
                unverified.add(n);
                unverifiedAt.computeIfAbsent(dim, d -> new Long2ObjectOpenHashMap<>()).computeIfAbsent(at.asLong(), p -> new LongArrayList()).add(n);
            } catch (NumberFormatException ex) {
                // not ours
            }
        }
    }

    private static ResourceKey<Level> dim(String name) {
        ResourceLocation loc = ResourceLocation.tryParse(name);
        return loc == null ? null : ResourceKey.create(Registries.DIMENSION, loc);
    }

    public String perfLine() {
        int routes = 0;
        for (Long2ObjectOpenHashMap<List<Kept>> m : kept.values())
            for (List<Kept> l : m.values())
                routes += l.size();
        return "frontier epoch " + Integer.toUnsignedString(epoch) + " kept routes " + routes + " held " + held.size() + " captured " + captured
                + " inserted again " + insertedAgain + " dropped " + dropped + " handed over " + handedOver + " holds " + holds + " followed " + followed
                + " landed " + landed + " released " + released + " stripped " + stripped + " unfound " + unfound + " unverified "
                + unverified.size() + " drawn " + drawnCount + " (marked " + drawn + ", removed " + removed + ", given back " + returned + ")";
    }
}
