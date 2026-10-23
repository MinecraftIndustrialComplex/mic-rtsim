package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Goods;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.minecraftindustrialcomplex.mic_rtsim.industry.StockAccess;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Districts;
import com.simibubi.create.Create;
import com.simibubi.create.content.contraptions.Contraption;
import com.simibubi.create.content.contraptions.actors.psi.PortableStorageInterfaceBlockEntity;
import com.simibubi.create.content.contraptions.minecart.TrainCargoManager;
import com.simibubi.create.content.fluids.pipes.EncasedPipeBlock;
import com.simibubi.create.content.fluids.pipes.FluidPipeBlock;
import com.simibubi.create.content.fluids.pipes.GlassFluidPipeBlock;
import com.simibubi.create.content.fluids.pipes.SmartFluidPipeBlock;
import com.simibubi.create.content.fluids.pipes.valve.FluidValveBlock;
import com.simibubi.create.content.fluids.pump.PumpBlock;
import com.simibubi.create.content.logistics.chute.AbstractChuteBlock;
import com.simibubi.create.content.logistics.funnel.AbstractFunnelBlock;
import com.simibubi.create.content.trains.entity.Carriage;
import com.simibubi.create.content.trains.entity.CarriageContraptionEntity;
import com.simibubi.create.content.trains.entity.Train;
import com.simibubi.create.content.trains.station.GlobalStation;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.HopperBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.neoforge.fluids.FluidStack;
import net.neoforged.neoforge.fluids.capability.IFluidHandler;
import net.neoforged.neoforge.items.IItemHandler;

import java.lang.ref.WeakReference;
import java.nio.ByteBuffer;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import java.util.TreeSet;
import java.util.UUID;

/**
 * Train ports and events at district stations (INDUSTRY.md §3.3, §8.1, docs/industry-boundary.md §3.3),
 * from the Create mixins on {@code Train.arriveAt}, {@code Train.leaveStation} (HEAD) and
 * {@code PortableStorageInterfaceBlockEntity.startTransferringTo} (HEAD):
 * <ul>
 *   <li><b>IND_TRAIN_ARRIVED</b> with every carriage's contents and the {@code simulated} flag: the
 *       station's district is unloaded and no carriage has an entity in that dimension. The contents
 *       are the cargo the train brought, read before any interface docked at the station (SG30), and
 *       the flags say where they were read (SG31);</li>
 *   <li><b>IND_STATION_PORTS</b> at a loaded stop's departure: the district inventories its docked
 *       interfaces feed (SG32);</li>
 *   <li><b>IND_PORT</b> (train cargo) at departure from a loaded district: what the carriages lost
 *       between arrival and departure went into the district (packages by their contents);</li>
 *   <li><b>IND_TRAIN_DEPARTED</b> once the train has its next destination (or 40 ticks after it left),
 *       with the destination station and the ETA at the train's top speed;</li>
 *   <li><b>IND_TRAIN_UNWATCHED</b> when a train calls at a station outside every district (SG50): the
 *       engine ends the leg the train began at its last district station, so the next district station
 *       is no edge from that one;</li>
 *   <li><b>IND_CARRIAGE_DELTA</b> applied through {@code TrainCargoManager}'s own handlers (whose
 *       {@code changeDetected} resets the idle-cargo counter), answered by IND_CARRIAGE_RESULT.</li>
 * </ul>
 */
public final class TrainWatch {
    private TrainWatch() {}

    /** One carriage's cargo at a moment. */
    record Cargo(int index, int itemSlots, long fluidCapacity, Industry.GoodsCounter goods) {}

    private record Stop(long station, long district, ResourceKey<Level> dim, boolean simulated, List<Cargo> cargo, long at) {}

    private record Leaving(WeakReference<Train> train, long station, long district, long at) {}

    /** The cargo when the first interface docked near the station the train was braking for (SG30). */
    private record Early(UUID station, List<Cargo> cargo) {}

    /** An interface that docked with no station to match it against (no destination, not at a station). */
    private record LooseDock(ResourceKey<Level> dim, BlockPos pos) {}

    /** The interfaces docked at one station during one stop, found by {@link #docking}. */
    private record Docks(UUID station, ResourceKey<Level> dim, Set<BlockPos> psis) {}

    /** Trains seen at a hook, by wire id: IND_CARRIAGE_DELTA finds a train here when Create's map lacks it. */
    private static final Map<Long, WeakReference<Train>> SEEN = new LinkedHashMap<>(64, 0.75f, true) {
        @Override
        protected boolean removeEldestEntry(Map.Entry<Long, WeakReference<Train>> eldest) {
            return size() > 4096;
        }
    };
    private static final Map<UUID, Stop> AT_STATION = new HashMap<>();
    /** Cargo at the head of arriveAt, before Create's own mail transfer moves packages (review finding 1). */
    private static final Map<UUID, List<Cargo>> ARRIVING = new HashMap<>();
    private static final List<Leaving> LEAVING = new ArrayList<>();
    /** Goods the station's mail moved from the train into postboxes (minus the reverse) during the stop (choice Q6). */
    private static final Map<UUID, Industry.GoodsCounter> MAIL = new HashMap<>();
    private static final Map<UUID, Early> EARLY = new HashMap<>();
    /** The cargo at the train's last {@code leaveStation} this session: the trip was watched (SG31). */
    private static final Map<UUID, List<Cargo>> DEPARTED = new HashMap<>();
    private static final Map<UUID, List<LooseDock>> LOOSE = new HashMap<>();
    private static final Map<UUID, Docks> DOCKS = new HashMap<>();
    /** Trains whose leg an IND_TRAIN_UNWATCHED already ended: the engine needs one per leg (SG50). */
    private static final Set<UUID> UNWATCHED = new HashSet<>();
    /** IND_STATION_PORTS last journaled per station, with the district revision it numbers (SG32). */
    private static final Map<Long, String> PORTS_SENT = new HashMap<>();
    /** The conveyance walk from an interface: steps and blocks visited (SG32). */
    static final int PORT_DEPTH = 8;
    static final int PORT_VISITS = 128;

    static long arrivals;
    static long simulatedArrivals;
    static long departures;
    static long cargoPorts;
    static long approachArrivals;
    static long departureArrivals;
    static long lateArrivals;
    static long docks;
    static long portRecords;
    static long unwatchedStops;
    /** Off only in a GameTest that shows what the engine does without the report (the control of SG51's guard). */
    private static boolean reportUnwatched = true;

    /** GameTests: send no IND_TRAIN_UNWATCHED, as a bridge from before SG50 would not. */
    public static void reportUnwatchedForTest(boolean on) {
        reportUnwatched = on;
    }

    /**
     * Mixin: Train.arriveAt(GlobalStation), at its start. {@code arriveAt} runs the station's mail
     * transfer before it returns, so the cargo the train brought is taken here.
     */
    public static void arriving(Train train, GlobalStation station) {
        try {
            Industry ind = IndustryApi.current();
            if (ind == null || station == null || station.getBlockEntityPos() == null || station.getBlockEntityDimension() == null)
                return;
            BlockPos pos = station.getBlockEntityPos();
            MAIL.remove(train.id);
            if (ind.districtAt(station.getBlockEntityDimension(), ChunkPos.asLong(pos.getX() >> 4, pos.getZ() >> 4)) != null)
                ARRIVING.put(train.id, cargo(ind, train));
        } catch (RuntimeException e) {
            MicRtsim.LOGGER.error("[mic_rtsim_create] observing a train's arrival failed", e);
        }
    }

    /**
     * Mixin: PortableStorageInterfaceBlockEntity.startTransferringTo, at its start (SG30): a station's
     * interface docks with a carriage's. Nothing has moved yet, and nothing moves at a station before
     * some interface docks there, so the first dock near the station the train is braking for is the
     * cargo it brought, on every arrival, slow or not. Docks while the train stands at a station are
     * the station's ports (SG32).
     */
    public static void docking(PortableStorageInterfaceBlockEntity be, Contraption contraption) {
        try {
            if (!(contraption.entity instanceof CarriageContraptionEntity cce) || !(be.getLevel() instanceof ServerLevel level))
                return;
            Industry ind = IndustryApi.current();
            if (ind == null)
                return;
            Carriage carriage = cce.getCarriage();
            Train train = carriage == null ? null : carriage.train;
            if (train == null)
                train = Create.RAILWAYS.trains.get(cce.trainId);
            if (train == null)
                return;
            docks++;
            BlockPos psi = be.getBlockPos().immutable();
            ResourceKey<Level> dim = level.dimension();
            GlobalStation at = train.getCurrentStation();
            GlobalStation dest = train.navigation == null ? null : train.navigation.destination;
            GlobalStation st = at != null ? at : dest;
            if (st == null || !near(train, st, dim, psi)) {
                if (st == null) {
                    List<LooseDock> loose = LOOSE.computeIfAbsent(train.id, k -> new ArrayList<>());
                    if (loose.size() < 64)
                        loose.add(new LooseDock(dim, psi));
                }
                return;
            }
            Early early = EARLY.get(train.id);
            if (at == null && (early == null || !early.station.equals(st.id)))
                EARLY.put(train.id, new Early(st.id, cargo(ind, train)));
            Docks d = DOCKS.get(train.id);
            if (d == null || !d.station.equals(st.id)) {
                d = new Docks(st.id, dim, new HashSet<>());
                DOCKS.put(train.id, d);
            }
            if (d.psis.size() < 64)
                d.psis.add(psi);
        } catch (RuntimeException e) {
            MicRtsim.LOGGER.error("[mic_rtsim_create] observing an interface dock failed", e);
        }
    }

    /** The interface is within the train's length (and a margin) of the station, in its dimension. */
    private static boolean near(Train train, GlobalStation st, ResourceKey<Level> dim, BlockPos psi) {
        BlockPos pos = st.getBlockEntityPos();
        if (pos == null || !dim.equals(st.getBlockEntityDimension()))
            return false;
        long length = 16;
        for (Carriage c : train.carriages)
            length += c.bogeySpacing;
        if (train.carriageSpacing != null)
            for (Integer s : train.carriageSpacing)
                length += s == null ? 0 : s;
        return pos.distSqr(psi) <= (double) length * length;
    }

    /** Mixin: Train.arriveAt(GlobalStation), at its end. */
    public static void arrived(Train train, GlobalStation station) {
        try {
            Industry ind = IndustryApi.current();
            if (ind == null || station == null)
                return;
            if (station.getBlockEntityPos() == null || station.getBlockEntityDimension() == null) {
                unwatched(ind, train, station); // a station with no place is in no district
                return;
            }
            ResourceKey<Level> dim = station.getBlockEntityDimension();
            BlockPos pos = station.getBlockEntityPos();
            District d = ind.districtAt(dim, ChunkPos.asLong(pos.getX() >> 4, pos.getZ() >> 4));
            if (d == null) {
                unwatched(ind, train, station);
                return;
            }
            long trainId = CreateIds.of(train.id);
            SEEN.put(trainId, new WeakReference<>(train));
            UNWATCHED.remove(train.id);
            boolean simulated = d.loadState() == IndRecords.LoadState.UNLOADED && !hasEntityIn(train, dim);
            List<Cargo> head = ARRIVING.remove(train.id);
            if (head == null)
                head = cargo(ind, train);
            Early early = EARLY.remove(train.id);
            List<Cargo> left = DEPARTED.remove(train.id);
            List<LooseDock> loose = LOOSE.remove(train.id);
            // Which cargo is what the train brought (SG30, SG31): the first dock's near this station; the
            // head of arriveAt when the whole trip was watched and no interface docked here unmatched;
            // the cargo it left its last station with when one did; else the head, flagged late.
            List<Cargo> cargo;
            int from;
            if (simulated) {
                cargo = head;
                from = 0;
            } else if (early != null && early.station.equals(station.id)) {
                cargo = early.cargo;
                from = IndRecords.Train.FLAG_APPROACH;
            } else if (left != null && !looseNear(loose, train, station, dim)) {
                cargo = head;
                from = IndRecords.Train.FLAG_APPROACH;
            } else if (left != null) {
                cargo = left;
                from = IndRecords.Train.FLAG_DEPARTURE;
            } else {
                cargo = head;
                from = 0;
            }
            if (!simulated) {
                if (from == IndRecords.Train.FLAG_APPROACH)
                    approachArrivals++;
                else if (from == IndRecords.Train.FLAG_DEPARTURE)
                    departureArrivals++;
                else
                    lateArrivals++;
            }
            int size = IndRecords.Train.ARRIVED_CARRIAGES;
            for (Cargo c : cargo)
                size += IndRecords.Train.carriageSize(c.goods.nonZeroIds().length);
            ByteBuffer r = ind.journal(IndRecords.IND_TRAIN_ARRIVED, size, cargo.size());
            r.putLong(IndRecords.Train.TRAIN, trainId);
            r.putLong(IndRecords.Train.STATION, CreateIds.of(station.id));
            r.putLong(IndRecords.Train.DISTRICT, d.id);
            r.putInt(IndRecords.Train.ARRIVED_FLAGS, (simulated ? IndRecords.Train.FLAG_SIMULATED : 0) | from);
            writeCarriages(r, IndRecords.Train.ARRIVED_CARRIAGES, cargo);
            ind.commit(r);
            AT_STATION.put(train.id, new Stop(CreateIds.of(station.id), d.id, dim, simulated, cargo, ind.gameTime()));
            arrivals++;
            if (simulated)
                simulatedArrivals++;
        } catch (RuntimeException e) {
            MicRtsim.LOGGER.error("[mic_rtsim_create] observing a train's arrival failed", e);
        }
    }

    /**
     * A call at a station outside every district (SG50): nothing is watched there, and the engine's leg
     * for this train (begun at its last district station) ends. Without it, a train that went out and
     * back through such a station made an edge from its district station to itself, with the round
     * trip's travel time (docs/stress-worlds-results.md 10.9: adopts that deferred their chunks while a
     * detection ran left stations outside every district for a while). Once per leg: a train that stops
     * at several unwatched stations in a row sends one record, until it leaves a district station again.
     * A departure the bridge has not journaled yet goes first, so the engine sees them in order.
     */
    private static void unwatched(Industry ind, Train train, GlobalStation station) {
        if (!reportUnwatched || !UNWATCHED.add(train.id))
            return;
        if (UNWATCHED.size() > 4096) {
            UNWATCHED.clear();
            UNWATCHED.add(train.id);
        }
        flushLeaving(ind, train);
        ByteBuffer r = ind.journal(IndRecords.IND_TRAIN_UNWATCHED, IndRecords.TrainUnwatched.SIZE, 0);
        r.putLong(IndRecords.TrainUnwatched.TRAIN, CreateIds.of(train.id));
        r.putLong(IndRecords.TrainUnwatched.STATION, CreateIds.of(station.id));
        ind.commit(r);
        unwatchedStops++;
    }

    private static boolean looseNear(List<LooseDock> loose, Train train, GlobalStation station, ResourceKey<Level> dim) {
        if (loose == null)
            return false;
        for (LooseDock l : loose)
            if (l.dim.equals(dim) && near(train, station, dim, l.pos))
                return true;
        return false;
    }

    /** Mixin: Train.leaveStation(), at its start (the station is still the current one). */
    public static void departing(Train train) {
        try {
            Industry ind = IndustryApi.current();
            Stop stop = AT_STATION.remove(train.id);
            Industry.GoodsCounter mail = MAIL.remove(train.id);
            Docks docked = DOCKS.remove(train.id);
            EARLY.remove(train.id);
            LOOSE.remove(train.id);
            if (ind == null)
                return;
            DEPARTED.put(train.id, cargo(ind, train));
            if (stop == null) {
                // The arrival was before this session (a restart while it stood here): its cargo diff is
                // unknown, so a loaded district's learning window is discarded rather than fed a hole.
                GlobalStation st = train.getCurrentStation();
                BlockPos pos = st == null ? null : st.getBlockEntityPos();
                District d = pos == null || st.getBlockEntityDimension() == null ? null
                        : ind.districtAt(st.getBlockEntityDimension(), ChunkPos.asLong(pos.getX() >> 4, pos.getZ() >> 4));
                if (d != null)
                    UNWATCHED.remove(train.id);
                if (d != null && d.loadState() != IndRecords.LoadState.UNLOADED)
                    ind.districts().discard(d, IndRecords.Discard.OTHER);
                return;
            }
            // A leg from a district station begins: its next unwatched stop is reported again (SG50).
            UNWATCHED.remove(train.id);
            District d = ind.district(stop.district);
            if (d != null && stop.simulated && d.loadState() != IndRecords.LoadState.UNLOADED) {
                // Simulated at arrival, loaded since: what its interfaces moved after the load is not a
                // port anyone measured. Discard the window.
                ind.districts().discard(d, IndRecords.Discard.OTHER);
            }
            if (d != null && !stop.simulated) {
                // Loaded: the carriages' change while they stood here crossed the boundary (§3.3).
                List<Cargo> now = cargo(ind, train);
                Industry.GoodsCounter port = ind.counter();
                for (Cargo c : stop.cargo)
                    port.addAll(c.goods, 1);
                for (Cargo c : now)
                    port.addAll(c.goods, -1);
                // Mail went to postboxes, each counted by its own district as IND_PACKAGE (Q6), not here.
                if (mail != null)
                    port.addAll(mail, -1);
                if (!port.isEmpty()) {
                    ind.port(d, IndRecords.Port.TRAIN_CARGO, IndRecords.NO_INVENTORY, CreateIds.of(train.id), 0, port);
                    cargoPorts++;
                }
                if (docked != null && CreateIds.of(docked.station) == stop.station && docked.dim.equals(stop.dim))
                    ports(ind, d, stop.station, ind.level(stop.dim), docked.psis);
            }
            LEAVING.add(new Leaving(new WeakReference<>(train), stop.station, stop.district, ind.gameTime()));
        } catch (RuntimeException e) {
            MicRtsim.LOGGER.error("[mic_rtsim_create] observing a train's departure failed", e);
        }
    }

    /**
     * The station's mail transfer moved these goods out of the train into postboxes (negative: from
     * postboxes into the train), {@link MailWatch}. Taken out of the train's cargo port at departure.
     */
    public static void mailMoved(Train train, Industry.GoodsCounter intoPostboxes) {
        Industry ind = IndustryApi.current();
        if (ind == null)
            return;
        MAIL.computeIfAbsent(train.id, k -> ind.counter()).addAll(intoPostboxes, 1);
    }

    /** Each tick: departures whose destination is known now (or that waited 40 ticks) are journaled. */
    static void tick(Industry ind, long now) {
        if (LEAVING.isEmpty())
            return;
        for (Iterator<Leaving> it = LEAVING.iterator(); it.hasNext(); ) {
            Leaving l = it.next();
            Train train = l.train.get();
            if (train == null) {
                it.remove();
                continue;
            }
            GlobalStation dest = train.navigation == null ? null : train.navigation.destination;
            if (dest == null && now - l.at < 40)
                continue;
            it.remove();
            departed(ind, l, train, dest);
        }
    }

    /**
     * The train's departure not journaled yet goes out now, with the destination it has (SG50: an
     * unwatched stop must follow the departure it ends, not precede it).
     */
    private static void flushLeaving(Industry ind, Train train) {
        for (Iterator<Leaving> it = LEAVING.iterator(); it.hasNext(); ) {
            Leaving l = it.next();
            Train t = l.train.get();
            if (t == null || !t.id.equals(train.id))
                continue;
            it.remove();
            departed(ind, l, train, train.navigation == null ? null : train.navigation.destination);
        }
    }

    private static void departed(Industry ind, Leaving l, Train train, GlobalStation dest) {
        long eta = 0;
        if (dest != null) {
            double speed = Math.max(1e-3, train.maxSpeed());
            eta = (long) Math.ceil(Math.max(0, train.navigation.distanceToDestination) / speed);
        }
        List<Cargo> cargo = cargo(ind, train);
        int size = IndRecords.Train.DEPARTED_CARRIAGES;
        for (Cargo c : cargo)
            size += IndRecords.Train.carriageSize(c.goods.nonZeroIds().length);
        ByteBuffer r = ind.journal(IndRecords.IND_TRAIN_DEPARTED, size, cargo.size());
        r.putLong(IndRecords.Train.TRAIN, CreateIds.of(train.id));
        r.putLong(IndRecords.Train.STATION, l.station);
        r.putLong(IndRecords.Train.DISTRICT, l.district);
        r.putLong(IndRecords.Train.DEPARTED_DESTINATION, dest == null ? 0 : CreateIds.of(dest.id));
        r.putLong(IndRecords.Train.DEPARTED_ETA, eta);
        writeCarriages(r, IndRecords.Train.DEPARTED_CARRIAGES, cargo);
        ind.commit(r);
        departures++;
    }

    /**
     * IND_STATION_PORTS (SG32): the district inventories the station's docked interfaces reach through
     * conveyance (chutes, funnels, hoppers, pipes, pumps and valves; not belts), at most
     * {@link #PORT_DEPTH} steps and {@link #PORT_VISITS} blocks from each, never into an unloaded chunk.
     * A container ends the walk; one touching an interface with nothing between them is not a port.
     * Journaled when the set differs from the last one sent for the station at this revision.
     */
    static void ports(Industry ind, District d, long station, ServerLevel level, Set<BlockPos> psis) {
        if (level == null || psis.isEmpty())
            return;
        TreeSet<Integer> found = new TreeSet<>();
        for (BlockPos psi : psis) {
            Map<BlockPos, Integer> depth = new HashMap<>();
            ArrayDeque<BlockPos> queue = new ArrayDeque<>();
            depth.put(psi, 0);
            queue.add(psi);
            while (!queue.isEmpty() && depth.size() < PORT_VISITS) {
                BlockPos p = queue.poll();
                int k = depth.get(p);
                for (Direction dir : Direction.values()) {
                    BlockPos q = p.relative(dir);
                    if (depth.containsKey(q) || !level.isLoaded(q))
                        continue;
                    depth.put(q, k + 1);
                    Block b = level.getBlockState(q).getBlock();
                    if (conveys(b)) {
                        if (k + 1 < PORT_DEPTH)
                            queue.add(q);
                        continue;
                    }
                    if (k == 0)
                        continue;
                    LevelChunk chunk = level.getChunkSource().getChunkNow(q.getX() >> 4, q.getZ() >> 4);
                    BlockEntity be = chunk == null ? null : chunk.getBlockEntity(q);
                    if (be == null || be instanceof PortableStorageInterfaceBlockEntity)
                        continue;
                    BlockPos canon = Districts.canonicalInventory(level, q);
                    District.Inventory inv = canon == null ? null : d.inventoryAt(canon);
                    if (inv != null)
                        found.add(inv.index());
                }
            }
        }
        if (found.isEmpty())
            return;
        String key = d.revision + ":" + found;
        if (Objects.equals(PORTS_SENT.get(station), key))
            return;
        PORTS_SENT.put(station, key);
        ByteBuffer r = ind.journal(IndRecords.IND_STATION_PORTS, IndRecords.StationPorts.size(found.size()), found.size());
        r.putLong(IndRecords.StationPorts.STATION, station);
        r.putLong(IndRecords.StationPorts.DISTRICT, d.id);
        int i = 0;
        for (int inv : found)
            r.putInt(IndRecords.StationPorts.INVENTORIES + 4 * i++, inv);
        ind.commit(r);
        portRecords++;
    }

    /** Blocks that move goods between an interface and a container next to them (SG32). */
    static boolean conveys(Block b) {
        return b instanceof AbstractChuteBlock || b instanceof AbstractFunnelBlock || b instanceof HopperBlock
                || b instanceof FluidPipeBlock || b instanceof EncasedPipeBlock || b instanceof GlassFluidPipeBlock
                || b instanceof SmartFluidPipeBlock || b instanceof PumpBlock || b instanceof FluidValveBlock;
    }

    private static boolean hasEntityIn(Train train, ResourceKey<Level> dim) {
        for (Carriage c : train.carriages) {
            Carriage.DimensionalCarriageEntity dce = c.getDimensionalIfPresent(dim);
            if (dce != null && dce.entity != null && dce.entity.get() != null)
                return true;
        }
        return false;
    }

    /** Every carriage's cargo, packages by their contents. {@code getAllItems} can be null (risk 5). */
    static List<Cargo> cargo(Industry ind, Train train) {
        List<Cargo> out = new ArrayList<>(train.carriages.size());
        for (int i = 0; i < train.carriages.size(); i++) {
            TrainCargoManager storage = train.carriages.get(i).storage;
            Industry.GoodsCounter goods = ind.counter();
            int slots = 0;
            long fluidCap = 0;
            IItemHandler items = items(storage);
            if (items != null) {
                slots = items.getSlots();
                StockAccess.readItems(ind, items, goods);
            }
            IFluidHandler fluids = fluids(storage);
            if (fluids != null) {
                for (int t = 0; t < fluids.getTanks(); t++) {
                    fluidCap += fluids.getTankCapacity(t);
                    FluidStack fs = fluids.getFluidInTank(t);
                    if (!fs.isEmpty())
                        goods.add(ind.goods().name(fs), fs.getAmount());
                }
            }
            out.add(new Cargo(i, slots, fluidCap, goods));
        }
        return out;
    }

    /** A carriage's item handler; null when it has none or is not initialized (Create throws then). */
    static IItemHandler items(TrainCargoManager storage) {
        try {
            return storage == null ? null : storage.getAllItems();
        } catch (IllegalStateException e) {
            return null;
        }
    }

    static IFluidHandler fluids(TrainCargoManager storage) {
        try {
            return storage == null ? null : storage.getFluids();
        } catch (IllegalStateException e) {
            return null;
        }
    }

    private static void writeCarriages(ByteBuffer r, int at, List<Cargo> cargo) {
        for (Cargo c : cargo) {
            int[] ids = c.goods.nonZeroIds();
            r.putInt(at + IndRecords.Train.C_INDEX, c.index);
            r.putInt(at + IndRecords.Train.C_ITEM_SLOTS, c.itemSlots);
            r.putLong(at + IndRecords.Train.C_FLUID_CAPACITY, c.fluidCapacity);
            r.putInt(at + IndRecords.Train.C_ENTRY_COUNT, ids.length);
            for (int k = 0; k < ids.length; k++) {
                int e = at + IndRecords.Train.C_ENTRIES + k * IndRecords.ENTRY16;
                r.putInt(e, ids[k]);
                r.putLong(e + 8, c.goods.get(ids[k]));
            }
            at += IndRecords.Train.carriageSize(ids.length);
        }
    }

    /** The train a wire id names: Create's own map first, then the trains seen at the hooks. */
    static Train train(long id) {
        for (Train t : Create.RAILWAYS.trains.values())
            if (CreateIds.of(t.id) == id)
                return t;
        WeakReference<Train> ref = SEEN.get(id);
        return ref == null ? null : ref.get();
    }

    /** IND_CARRIAGE_DELTA (§4.3): applied wherever the train is now, through the cargo handlers. */
    static boolean carriageDelta(Industry ind, ByteBuffer b, int at, int size, int n, long cmdSeq) {
        long opId = b.getLong(at + IndRecords.CarriageDelta.OP_ID);
        long trainId = b.getLong(at + IndRecords.CarriageDelta.TRAIN);
        if (n < 0 || IndRecords.CarriageDelta.size(n) > size || IndRecords.CarriageResult.size(n) > 0xFFF8) {
            ind.commandResult(cmdSeq, opId, Obs.CommandResult.FAILED);
            return true;
        }
        // Train cargo and the journal reach disk at the same save (RailwaySavedData and the journal are
        // both SavedData), so the journal's op index is the dedup (choice H17).
        if (ind.opApplied(opId)) {
            ind.commandResult(cmdSeq, opId, Obs.CommandResult.DUPLICATE_OP);
            return true;
        }
        Train train = train(trainId);
        long[][] results = new long[n][4];
        if (train == null) {
            // Nothing applied: IND_CARRIAGE_RESULT says so (the engine corrects stock from it, §4.3).
            for (int i = 0; i < n; i++) {
                int e = at + IndRecords.CarriageDelta.ENTRIES + i * IndRecords.CarriageDelta.ENTRY;
                results[i] = new long[] {b.getInt(e + IndRecords.CarriageDelta.E_CARRIAGE) & 0xFFFF_FFFFL,
                        b.getInt(e + IndRecords.CarriageDelta.E_GOOD) & 0xFFFF_FFFFL, b.getLong(e + IndRecords.CarriageDelta.E_DELTA), 0};
            }
            writeCarriageResult(ind, opId, trainId, results);
            ind.commandResult(cmdSeq, opId, Obs.CommandResult.FAILED);
            return true;
        }
        // Removals first, as in a reconcile: what leaves frees the room what arrives needs.
        int[] order = new int[n];
        int k = 0;
        for (int pass = 0; pass < 2; pass++)
            for (int i = 0; i < n; i++) {
                long delta = b.getLong(at + IndRecords.CarriageDelta.ENTRIES + i * IndRecords.CarriageDelta.ENTRY + IndRecords.CarriageDelta.E_DELTA);
                if ((pass == 0) == (delta < 0))
                    order[k++] = i;
            }
        for (int j = 0; j < n; j++) {
            int e = at + IndRecords.CarriageDelta.ENTRIES + order[j] * IndRecords.CarriageDelta.ENTRY;
            int carriage = b.getInt(e + IndRecords.CarriageDelta.E_CARRIAGE);
            int goodId = b.getInt(e + IndRecords.CarriageDelta.E_GOOD);
            long delta = b.getLong(e + IndRecords.CarriageDelta.E_DELTA);
            long applied = 0;
            Goods.Name good = Goods.parse(ind.engineName(IndRecords.DICT_GOOD, goodId));
            if (good != null && carriage >= 0 && carriage < train.carriages.size()) {
                TrainCargoManager storage = train.carriages.get(carriage).storage;
                IItemHandler items = items(storage);
                IFluidHandler fluids = fluids(storage);
                try {
                    if (good.cls() == 'i' && items != null)
                        applied = StockAccess.applyItems(ind, items, ind.goods().itemPrototype(good), delta);
                    else if (good.cls() == 'f' && fluids != null)
                        applied = StockAccess.applyFluid(fluids, ind.goods().fluidPrototype(good), delta);
                } catch (RuntimeException ex) {
                    MicRtsim.LOGGER.error("[mic_rtsim_create] applying {} of {} to a carriage failed", delta, good, ex);
                }
            }
            results[j] = new long[] {carriage & 0xFFFF_FFFFL, goodId & 0xFFFF_FFFFL, delta, applied};
        }
        writeCarriageResult(ind, opId, trainId, results);
        ind.commandResult(cmdSeq, opId, Obs.CommandResult.APPLIED);
        return true;
    }

    private static void writeCarriageResult(Industry ind, long opId, long trainId, long[][] results) {
        int n = results.length;
        ByteBuffer r = ind.journal(IndRecords.IND_CARRIAGE_RESULT, IndRecords.CarriageResult.size(n), n);
        r.putLong(IndRecords.CarriageResult.OP_ID, opId);
        r.putLong(IndRecords.CarriageResult.TRAIN, trainId);
        for (int j = 0; j < n; j++) {
            int e = IndRecords.CarriageResult.ENTRIES + j * IndRecords.RESULT24;
            r.putInt(e, (int) results[j][0]);
            r.putInt(e + 4, (int) results[j][1]);
            r.putLong(e + 8, results[j][2]);
            r.putLong(e + 16, results[j][3]);
        }
        ind.commit(r);
    }

    /** Test helper: forget the hooks' state for a train. */
    public static void forget(Train train) {
        AT_STATION.remove(train.id);
        ARRIVING.remove(train.id);
        MAIL.remove(train.id);
        EARLY.remove(train.id);
        DEPARTED.remove(train.id);
        LOOSE.remove(train.id);
        DOCKS.remove(train.id);
        UNWATCHED.remove(train.id);
        SEEN.remove(CreateIds.of(train.id));
    }

    /**
     * Test helper: an interface at {@code psi} docked with the train while it stands at {@code station}, as
     * {@link #docking} records it. A synthetic train has no carriage entity, so nothing docks on its own.
     */
    public static void dockedForTest(Train train, GlobalStation station, ResourceKey<Level> dim, BlockPos psi) {
        Docks d = DOCKS.get(train.id);
        if (d == null || !d.station.equals(station.id)) {
            d = new Docks(station.id, dim, new HashSet<>());
            DOCKS.put(train.id, d);
        }
        d.psis.add(psi.immutable());
    }

    /** Test helper: forget which ports were sent, so the next loaded stop journals them again. */
    public static void forgetPorts() {
        PORTS_SENT.clear();
    }

    public static long arrivals() {
        return arrivals;
    }

    /** For {@code /rtsim district perf}: what the train hooks journaled this session. */
    static String perfLine() {
        return "trains: " + arrivals + " arrivals at district stations (" + simulatedArrivals + " simulated), " + departures
                + " departures, " + cargoPorts + " cargo ports, " + SEEN.size() + " trains seen, " + AT_STATION.size() + " at a station; "
                + "cargo read at " + approachArrivals + " approaches, " + departureArrivals + " departures, " + lateArrivals + " late; "
                + docks + " interface docks, " + portRecords + " station port records, " + unwatchedStops + " unwatched stops";
    }
}
