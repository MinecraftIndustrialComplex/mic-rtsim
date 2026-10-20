package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Goods;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.minecraftindustrialcomplex.mic_rtsim.industry.StockAccess;
import com.simibubi.create.Create;
import com.simibubi.create.content.contraptions.minecart.TrainCargoManager;
import com.simibubi.create.content.trains.entity.Carriage;
import com.simibubi.create.content.trains.entity.Train;
import com.simibubi.create.content.trains.station.GlobalStation;
import net.minecraft.core.BlockPos;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.neoforged.neoforge.fluids.FluidStack;
import net.neoforged.neoforge.fluids.capability.IFluidHandler;
import net.neoforged.neoforge.items.IItemHandler;

import java.lang.ref.WeakReference;
import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;

/**
 * Train ports and events at district stations (INDUSTRY.md §3.3, §8.1, docs/industry-boundary.md §3.3),
 * from the two Create mixins on {@code Train.arriveAt} (TAIL) and {@code Train.leaveStation} (HEAD):
 * <ul>
 *   <li><b>IND_TRAIN_ARRIVED</b> with every carriage's contents and the {@code simulated} flag: the
 *       station's district is unloaded and no carriage has an entity in that dimension;</li>
 *   <li><b>IND_PORT</b> (train cargo) at departure from a loaded district: what the carriages lost
 *       between arrival and departure went into the district (packages by their contents);</li>
 *   <li><b>IND_TRAIN_DEPARTED</b> once the train has its next destination (or 40 ticks after it left),
 *       with the destination station and the ETA at the train's top speed;</li>
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

    /** Trains seen at a hook, by wire id: IND_CARRIAGE_DELTA finds a train here when Create's map lacks it. */
    private static final Map<Long, WeakReference<Train>> SEEN = new LinkedHashMap<>(64, 0.75f, true) {
        @Override
        protected boolean removeEldestEntry(Map.Entry<Long, WeakReference<Train>> eldest) {
            return size() > 4096;
        }
    };
    private static final Map<UUID, Stop> AT_STATION = new HashMap<>();
    private static final List<Leaving> LEAVING = new ArrayList<>();

    static long arrivals;
    static long departures;
    static long cargoPorts;

    /** Mixin: Train.arriveAt(GlobalStation), at its end. */
    public static void arrived(Train train, GlobalStation station) {
        try {
            Industry ind = IndustryApi.current();
            if (ind == null || station == null || station.getBlockEntityPos() == null || station.getBlockEntityDimension() == null)
                return;
            ResourceKey<Level> dim = station.getBlockEntityDimension();
            BlockPos pos = station.getBlockEntityPos();
            District d = ind.districtAt(dim, ChunkPos.asLong(pos.getX() >> 4, pos.getZ() >> 4));
            if (d == null)
                return;
            long trainId = CreateIds.of(train.id);
            SEEN.put(trainId, new WeakReference<>(train));
            boolean simulated = d.loadState() == IndRecords.LoadState.UNLOADED && !hasEntityIn(train, dim);
            List<Cargo> cargo = cargo(ind, train);
            int size = IndRecords.Train.ARRIVED_CARRIAGES;
            for (Cargo c : cargo)
                size += IndRecords.Train.carriageSize(c.goods.nonZeroIds().length);
            ByteBuffer r = ind.journal(IndRecords.IND_TRAIN_ARRIVED, size, cargo.size());
            r.putLong(IndRecords.Train.TRAIN, trainId);
            r.putLong(IndRecords.Train.STATION, CreateIds.of(station.id));
            r.putLong(IndRecords.Train.DISTRICT, d.id);
            r.putInt(IndRecords.Train.ARRIVED_FLAGS, simulated ? IndRecords.Train.FLAG_SIMULATED : 0);
            writeCarriages(r, IndRecords.Train.ARRIVED_CARRIAGES, cargo);
            ind.commit(r);
            AT_STATION.put(train.id, new Stop(CreateIds.of(station.id), d.id, dim, simulated, cargo, ind.gameTime()));
            arrivals++;
        } catch (RuntimeException e) {
            MicRtsim.LOGGER.error("[mic_rtsim_create] observing a train's arrival failed", e);
        }
    }

    /** Mixin: Train.leaveStation(), at its start (the station is still the current one). */
    public static void departing(Train train) {
        try {
            Industry ind = IndustryApi.current();
            Stop stop = AT_STATION.remove(train.id);
            if (ind == null || stop == null)
                return;
            District d = ind.district(stop.district);
            if (d != null && !stop.simulated) {
                // Loaded: the carriages' change while they stood here crossed the boundary (§3.3).
                List<Cargo> now = cargo(ind, train);
                Industry.GoodsCounter port = ind.counter();
                for (Cargo c : stop.cargo)
                    port.addAll(c.goods, 1);
                for (Cargo c : now)
                    port.addAll(c.goods, -1);
                if (!port.isEmpty()) {
                    ind.port(d, IndRecords.Port.TRAIN_CARGO, IndRecords.NO_INVENTORY, CreateIds.of(train.id), 0, port);
                    cargoPorts++;
                }
            }
            LEAVING.add(new Leaving(new WeakReference<>(train), stop.station, stop.district, ind.gameTime()));
        } catch (RuntimeException e) {
            MicRtsim.LOGGER.error("[mic_rtsim_create] observing a train's departure failed", e);
        }
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
            IItemHandler items = storage == null ? null : storage.getAllItems();
            if (items != null) {
                slots = items.getSlots();
                StockAccess.readItems(ind, items, goods);
            }
            IFluidHandler fluids = storage == null ? null : storage.getFluids();
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
        if (n < 0 || IndRecords.CarriageDelta.size(n) > size) {
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
        if (train == null) {
            ind.commandResult(cmdSeq, opId, Obs.CommandResult.FAILED);
            return true;
        }
        long[][] results = new long[n][4];
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
                if (good.cls() == 'i' && storage != null && storage.getAllItems() != null)
                    applied = StockAccess.applyItems(ind, storage.getAllItems(), ind.goods().itemPrototype(good), delta);
                else if (good.cls() == 'f' && storage != null && storage.getFluids() != null)
                    applied = StockAccess.applyFluid(storage.getFluids(), ind.goods().fluidPrototype(good), delta);
            }
            results[j] = new long[] {carriage & 0xFFFF_FFFFL, goodId & 0xFFFF_FFFFL, delta, applied};
        }
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
        ind.commandResult(cmdSeq, opId, Obs.CommandResult.APPLIED);
        return true;
    }

    /** Test helper: forget the hooks' state for a train. */
    public static void forget(Train train) {
        AT_STATION.remove(train.id);
        SEEN.remove(CreateIds.of(train.id));
    }

    public static long arrivals() {
        return arrivals;
    }
}
