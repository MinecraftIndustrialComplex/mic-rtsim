package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.minecraftindustrialcomplex.mic_rtsim_create.mixin.ChainConveyorRoutingTableAccessor;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorBlockEntity;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorRoutingTable;
import com.simibubi.create.content.logistics.box.PackageItem;
import it.unimi.dsi.fastutil.longs.Long2ObjectMap;
import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
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
import org.apache.commons.lang3.mutable.MutableInt;

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
 *       Create's full timeout again and kept, with its port, distance and connection. Every
 *       {@link #KEEP_TICKS} each conveyor with kept routes inserts them again (Create's private
 *       {@code insert}, which refreshes an identical entry), so they also come back after a restart, when
 *       Create's tables start empty; Create's own adverts carry them along the near side. A kept route goes
 *       when its connection is gone, its next conveyor ticks (Create's adverts take over), or no far
 *       frogport of the store has its filter.</li>
 *   <li><b>The hold</b> (choice SC138). Where the guard refuses a hand-off ({@link #hold}), a package whose
 *       route is a far frogport's gets a key in vanilla's {@code minecraft:custom_data} ({@link #TAG}: the
 *       store, the epoch, a serial; choice SC136) and one {@code CHAIN_HELD}, whose frogport and destination
 *       are that far frogport and whose position is the boundary conveyor. A package already keyed in this
 *       epoch and held by Java is not recorded again; one with any other key has it stripped first. The key
 *       goes where a frogport takes the package off the chain ({@code FrogportChainMixin}), whose
 *       {@code CHAIN_IN} then carries it, and at a census, which counts every package of a store whose
 *       chunks all tick (nothing is held then).</li>
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

    /** A kept route on a boundary conveyor: Create's entry, as it was when it would have expired. */
    public record Kept(String port, int distance, BlockPos connection) {}

    /** A package Java holds: its store, the far frogport it heads for, the boundary conveyor it circles. */
    public record Held(long store, ResourceKey<Level> dim, BlockPos destination, BlockPos at) {}

    /** A package's key. */
    public record Key(long store, int epoch, long serial) {}

    private final Industry ind;
    private final PackageStores stores;
    int epoch;
    private long serialHigh;
    private int serialLow;
    private final Map<ResourceKey<Level>, Long2ObjectOpenHashMap<List<Kept>>> kept = new HashMap<>();
    private final Long2ObjectOpenHashMap<Held> held = new Long2ObjectOpenHashMap<>();
    /** The saved state, kept up to date in place: {@code epoch}, {@code kept} (written whole), {@code held} (by serial). */
    private final CompoundTag tag;
    // Counters (perfLine).
    long captured, insertedAgain, dropped, holds, landed, stripped;

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
     * {@link #KEEP_TICKS} inserts the conveyor's kept routes again. Nothing is looked up unless an entry is
     * about to expire or the conveyor's turn has come.
     */
    public static void beforeRoutes(ChainConveyorBlockEntity cc, ChainConveyorRoutingTable table) {
        if (!(cc.getLevel() instanceof ServerLevel level))
            return;
        try {
            BlockPos pos = cc.getBlockPos();
            boolean expiring = false;
            for (ChainConveyorRoutingTable.RoutingTableEntry e : table.entriesByDistance)
                if (!e.endOfRoute() && e.timeout().intValue() <= 1) {
                    expiring = true;
                    break;
                }
            boolean turn = Math.floorMod(level.getGameTime() + pos.hashCode(), KEEP_TICKS) == 0;
            if (!expiring && !turn)
                return;
            Industry ind = IndustryApi.current();
            if (ind == null)
                return;
            Frontier f = PackageStores.of(ind).frontier;
            if (expiring)
                f.capture(level, cc, table);
            if (turn)
                f.insertAgain(level, cc, table);
        } catch (RuntimeException e) {
            if (!warned) {
                warned = true;
                MicRtsim.LOGGER.warn("[mic_rtsim] create: the frontier's routes at {} were not kept", cc.getBlockPos(), e);
            }
        }
    }

    private static boolean warned;

    private void capture(ServerLevel level, ChainConveyorBlockEntity cc, ChainConveyorRoutingTable table) {
        BlockPos pos = cc.getBlockPos();
        PackageStores.Store s = null;
        for (ChainConveyorRoutingTable.RoutingTableEntry e : table.entriesByDistance) {
            if (e.endOfRoute() || e.timeout().intValue() > 1 || Loaded.ticksFrom(level, pos, pos.offset(e.nextConnection())))
                continue;
            if (s == null)
                s = stores.storeOfConveyor(level.dimension(), pos);
            if (s == null)
                return;
            if (awayPort(level, s, e.port()) < 0)
                continue;
            e.timeout().setValue(ChainConveyorRoutingTable.ENTRY_TIMEOUT);
            keep(level.dimension(), pos, new Kept(e.port(), e.distance(), e.nextConnection().immutable()));
        }
    }

    private void keep(ResourceKey<Level> dim, BlockPos pos, Kept k) {
        List<Kept> l = kept.computeIfAbsent(dim, d -> new Long2ObjectOpenHashMap<>()).computeIfAbsent(pos.asLong(), p -> new ArrayList<>());
        for (int i = 0; i < l.size(); i++)
            if (l.get(i).port().equals(k.port()) && l.get(i).connection().equals(k.connection())) {
                if (l.get(i).distance() == k.distance())
                    return;
                l.set(i, k);
                saveKept();
                return;
            }
        l.add(k);
        captured++;
        saveKept();
    }

    private void insertAgain(ServerLevel level, ChainConveyorBlockEntity cc, ChainConveyorRoutingTable table) {
        Long2ObjectOpenHashMap<List<Kept>> m = kept.get(level.dimension());
        BlockPos pos = cc.getBlockPos();
        List<Kept> l = m == null ? null : m.get(pos.asLong());
        if (l == null)
            return;
        PackageStores.Store s = stores.storeOfConveyor(level.dimension(), pos);
        boolean changed = false;
        for (int i = l.size() - 1; i >= 0; i--) {
            Kept k = l.get(i);
            if (s == null || !cc.connections.contains(k.connection()) || Loaded.ticksFrom(level, pos, pos.offset(k.connection()))
                    || awayPort(level, s, k.port()) < 0) {
                l.remove(i);
                dropped++;
                changed = true;
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

    /** A chain conveyor is destroyed: its kept routes go. */
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
        Key k = key(box);
        if (k != null) {
            Held h = held.get(k.serial());
            if (k.epoch() == epoch && h != null && h.store() == k.store()) {
                if (!h.at().equals(pos))
                    putHeld(k.serial(), new Held(h.store(), h.dim(), h.destination(), pos.immutable()));
                return;
            }
            strip(box);
            stripped++;
            cc.setChanged();
        }
        PackageStores.Store s = stores.storeOfConveyor(level.dimension(), pos);
        if (s == null)
            return;
        String port = null;
        for (ChainConveyorRoutingTable.RoutingTableEntry e : table.entriesByDistance)
            if (PackageItem.matchAddress(box, e.port())) {
                port = e.port();
                break;
            }
        if (port == null)
            return;
        int dest = awayPort(level, s, port);
        if (dest < 0)
            return;
        long serial = nextSerial();
        CompoundTag kt = new CompoundTag();
        kt.putLong("store", s.id);
        kt.putInt("epoch", epoch);
        kt.putLong("serial", serial);
        CustomData.update(DataComponents.CUSTOM_DATA, box, t -> t.put(TAG, kt));
        cc.setChanged();
        putHeld(serial, new Held(s.id, level.dimension(), s.ports.get(dest).pos, pos.immutable()));
        Industry.GoodsCounter goods = ind.counter();
        ind.countStack(box, box.getCount(), goods);
        PackageStores.journalChain(ind, s, dest, pos, null, null, IndRecords.ChainPackage.CHAIN_HELD, dest, box.getCount(),
                PackageItem.getAddress(box), goods, 0, epoch, serial);
        holds++;
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
     * A frogport of store {@code store} took a package keyed {@code k} off the chain: the key, when Java holds
     * it in this epoch (it is then no longer held), else null.
     */
    Key landed(long store, Key k) {
        if (k == null || k.epoch() != epoch)
            return null;
        Held h = held.get(k.serial());
        if (h == null || h.store() != k.store())
            return null;
        removeHeld(k.serial());
        landed++;
        return k;
    }

    /** A census of store {@code id}: nothing of it is held now (its keys are stripped as it counts them). */
    void census(long id) {
        boolean any = false;
        for (var it = held.long2ObjectEntrySet().iterator(); it.hasNext();) {
            Long2ObjectMap.Entry<Held> e = it.next();
            if (e.getValue().store() == id) {
                it.remove();
                tag.getCompound("held").remove(Long.toHexString(e.getLongKey()));
                any = true;
            }
        }
        if (any)
            save();
    }

    // ---- persistence (IndustryData, under KEY) ----------------------------------------------------

    private void putHeld(long serial, Held h) {
        held.put(serial, h);
        CompoundTag e = new CompoundTag();
        e.putLong("store", h.store());
        e.putString("dim", h.dim().location().toString());
        e.putLong("destination", h.destination().asLong());
        e.putLong("at", h.at().asLong());
        if (!tag.contains("held", Tag.TAG_COMPOUND))
            tag.put("held", new CompoundTag());
        tag.getCompound("held").put(Long.toHexString(serial), e);
        save();
    }

    private void removeHeld(long serial) {
        held.remove(serial);
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
                    .add(new Kept(e.getString("port"), e.getInt("distance"), BlockPos.of(e.getLong("connection"))));
        }
        CompoundTag h = tag.getCompound("held");
        for (String serial : h.getAllKeys()) {
            CompoundTag e = h.getCompound(serial);
            ResourceKey<Level> dim = dim(e.getString("dim"));
            if (dim == null)
                continue;
            try {
                held.put(Long.parseUnsignedLong(serial, 16), new Held(e.getLong("store"), dim, BlockPos.of(e.getLong("destination")),
                        BlockPos.of(e.getLong("at"))));
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
                + " inserted again " + insertedAgain + " dropped " + dropped + " holds " + holds + " landed " + landed + " stripped " + stripped;
    }
}
