package com.minecraftindustrialcomplex.mic_rtsim.industry;

import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;

import java.nio.ByteBuffer;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.List;

/**
 * The boundary stock sampler (I3, INDUSTRY.md §3.2, docs/industry-boundary.md §3.1). It reads the
 * boundary inventories of districts with loaded chunks round-robin, K per tick ({@code inventoriesPerTick}),
 * one sweep per district every {@code sweepPeriodTicks}. Each inventory's contents go out as one
 * IND_STOCK_SAMPLE, or several with the same sweep when they do not fit; after the last, IND_SWEEP_END
 * says how many records the sweep sent, so the engine can drop a sweep that lost one (F5).
 *
 * <p>{@code live} is set only when every chunk of the district was block-ticking at every tick of the
 * sweep, from its first read to its end; a sweep of a partly loaded district reads what is loaded and is
 * not live. Cost grows with the boundary inventories of loaded districts over the period, never with the
 * items moved (§15).
 */
public final class Sampler {

    private final Industry ind;
    /** Districts with a sweep due or running, in the order they become due. */
    private final ArrayDeque<District> queue = new ArrayDeque<>();
    private final List<District> scratch = new ArrayList<>();
    private District current;

    // Counters for /rtsim district perf and the GameTests (measured, not tuned).
    long sweeps;
    long liveSweeps;
    long inventoriesRead;
    long slotsRead;
    long recordsSent;
    long recordsDropped;
    long readNanos;
    long lastSweepNanos;
    long maxInventoryNanos;
    /** The last finished sweep of each district: (sweep number, flags, records) for the GameTests. */
    final java.util.Map<Long, long[]> lastSweep = new java.util.HashMap<>();

    Sampler(Industry ind) {
        this.ind = ind;
    }

    void revisionChanged(District d) {
        // Inventory indices are per revision: a sweep that began under the old list is abandoned.
        if (d.sweepCursor >= 0) {
            d.sweepCursor = -1;
            if (current == d)
                current = null;
        }
        d.nextSweepAt = Math.min(d.nextSweepAt, ind.gameTime());
        ind.glass.revisionChanged(d); // a new revision (or a removal) recompiles the glass box's graph
    }

    void chunkUnloaded(District d) {
        if (d.sweepCursor >= 0)
            d.sweepLive = false;
    }

    /** Starts the next sweep of {@code d} at once (commands, GameTests). */
    public void sweepSoon(District d) {
        d.nextSweepAt = ind.gameTime();
    }

    void tick(long now) {
        int period = IndustryConfig.get(IndustryConfig.SWEEP_PERIOD);
        // Due districts join the queue: any district with a loaded chunk.
        scratch.clear();
        for (District d : ind.districts.all())
            if (d.sweepCursor < 0 && !d.queued && now >= d.nextSweepAt && ind.districts.loadedChunks(d) > 0 && d != current) {
                d.queued = true;
                scratch.add(d);
            }
        queue.addAll(scratch);
        // A sweep spanning ticks is live only if every tick saw every chunk ticking.
        if (current != null && current.sweepLive && !ind.districts.allTicking(current))
            current.sweepLive = false;

        int budget = IndustryConfig.get(IndustryConfig.INVENTORIES_PER_TICK);
        long slotBudget = IndustryConfig.get(IndustryConfig.SLOTS_PER_TICK);
        long slotsThisTick = 0;
        while (budget > 0 && slotsThisTick < slotBudget) {
            if (current == null) {
                current = queue.poll();
                if (current == null)
                    return;
                current.queued = false;
                if (ind.district(current.id) != current || ind.districts.loadedChunks(current) == 0) {
                    current = null;
                    continue;
                }
                start(current, now, period);
            }
            District d = current;
            ServerLevel level = ind.level(d.dim);
            if (level == null || d.sweepRevision != d.revision) {
                d.sweepCursor = -1;
                current = null;
                continue;
            }
            while (budget > 0 && slotsThisTick < slotBudget && d.sweepCursor < d.inventories.size()) {
                District.Inventory inv = d.inventories.get(d.sweepCursor++);
                if (!StockAccess.loaded(level, inv.pos())) {
                    // An inventory in an unloaded chunk, or with a part in one (a vault across a chunk still
                    // loading, whose read would wait for the load, R2-11): a partial sweep.
                    d.sweepLive = false;
                    continue;
                }
                read(level, d, inv, now);
                budget--;
                // The slot budget bounds a tick's cost by what is read, not by how many inventories:
                // one 1,620-slot vault costs what 60 chests do.
                slotsThisTick += Math.max(1, inv.itemSlots());
            }
            if (d.sweepCursor >= d.inventories.size()) {
                finish(level, d, now);
                current = null;
            }
        }
    }

    private void start(District d, long now, int period) {
        d.sweepNo++;
        d.sweepCursor = 0;
        d.sweepRecords = 0;
        d.sweepInventories = 0;
        d.sweepRevision = d.revision;
        d.sweepLive = ind.districts.allTicking(d);
        d.nextSweepAt = now + period;
        sweepStart = System.nanoTime();
    }

    private long sweepStart;

    private void read(ServerLevel level, District d, District.Inventory inv, long now) {
        long t0 = System.nanoTime();
        Industry.GoodsCounter goods = ind.counter();
        StockAccess access = access(level, inv);
        access.read(goods);
        int[] ids = goods.nonZeroIds();
        int from = 0;
        do {
            int n = Math.min(IndRecords.StockSample.MAX_ENTRIES, ids.length - from);
            int at = ind.latest(IndRecords.IND_STOCK_SAMPLE, IndRecords.StockSample.size(n), n);
            d.sweepRecords++;
            if (at >= 0) {
                ByteBuffer b = ind.open();
                b.putLong(at + IndRecords.StockSample.GAME_TIME, now);
                b.putLong(at + IndRecords.StockSample.DISTRICT, d.id);
                b.putInt(at + IndRecords.StockSample.INVENTORY, inv.index());
                b.putInt(at + IndRecords.StockSample.SWEEP, d.sweepNo);
                for (int i = 0; i < n; i++) {
                    int e = at + IndRecords.StockSample.ENTRIES + i * IndRecords.ENTRY16;
                    b.putInt(e, ids[from + i]);
                    b.putLong(e + 8, goods.get(ids[from + i]));
                }
                recordsSent++;
            } else {
                recordsDropped++;
            }
            from += n;
        } while (from < ids.length);
        d.sweepInventories++;
        inventoriesRead++;
        slotsRead += inv.itemSlots();
        long dt = System.nanoTime() - t0;
        readNanos += dt;
        maxInventoryNanos = Math.max(maxInventoryNanos, dt);
    }

    /** The access for an inventory: a plugin's for its kinds (postboxes), else the capabilities. */
    StockAccess access(ServerLevel level, District.Inventory inv) {
        for (IndustryPlugin p : IndustryApi.PLUGINS) {
            StockAccess a = p.access(level, inv);
            if (a != null)
                return a;
        }
        return StockAccess.capabilities(ind, level, inv);
    }

    private void finish(ServerLevel level, District d, long now) {
        deposits(level, d, now);
        ind.glass.swept(level, d, now, d.sweepNo); // probed buffers (IND_PROBE_SAMPLE), not counted in the sweep (GB3)
        if (d.sweepLive && !ind.districts.allTicking(d))
            d.sweepLive = false;
        int flags = d.sweepLive ? IndRecords.SweepEnd.FLAG_LIVE : 0;
        if (d.creative)
            flags |= IndRecords.SweepEnd.FLAG_CREATIVE;
        for (IndustryPlugin p : IndustryApi.PLUGINS)
            flags |= p.sweepFlags(level, d) & IndRecords.SweepEnd.FLAG_OVERSTRESSED;
        int at = ind.latest(IndRecords.IND_SWEEP_END, IndRecords.SweepEnd.SIZE, flags);
        if (at >= 0) {
            ByteBuffer b = ind.open();
            b.putLong(at + IndRecords.SweepEnd.GAME_TIME, now);
            b.putLong(at + IndRecords.SweepEnd.DISTRICT, d.id);
            b.putInt(at + IndRecords.SweepEnd.SWEEP, d.sweepNo);
            b.putInt(at + IndRecords.SweepEnd.RECORDS, d.sweepRecords);
            b.putInt(at + IndRecords.SweepEnd.INVENTORIES, d.sweepInventories);
        } else {
            recordsDropped++;
        }
        sweeps++;
        if (d.sweepLive)
            liveSweeps++;
        lastSweepNanos = System.nanoTime() - sweepStart;
        lastSweep.put(d.id, new long[] {d.sweepNo, flags, d.sweepRecords, d.sweepInventories});
        d.sweepCursor = -1;
    }

    /**
     * The district's finite deposits (I11, F15): each plugin reports the deposits of every loaded member
     * chunk as goods of class {@code d}, one IND_DEPOSIT_SAMPLE each, counted in the sweep's records like
     * the stock samples, so a dropped one discards the sweep. Read once per sweep, at its end.
     */
    private void deposits(ServerLevel level, District d, long now) {
        if (IndustryApi.PLUGINS.isEmpty())
            return;
        int dim = -1;
        for (long c : d.chunks) {
            net.minecraft.world.level.chunk.LevelChunk chunk = level.getChunkSource().getChunkNow(ChunkPos.getX(c), ChunkPos.getZ(c));
            if (chunk == null)
                continue;
            for (IndustryPlugin p : IndustryApi.PLUGINS) {
                if (dim < 0)
                    dim = ind.dimId(level.dimension());
                final int dimId = dim;
                try {
                    p.deposits(level, d, chunk, (plugin, good, remaining, infinite) -> {
                        int at = ind.latest(IndRecords.IND_DEPOSIT_SAMPLE, IndRecords.DepositSample.SIZE, plugin);
                        d.sweepRecords++;
                        if (at < 0) {
                            recordsDropped++;
                            return;
                        }
                        ByteBuffer b = ind.open();
                        b.putLong(at + IndRecords.DepositSample.GAME_TIME, now);
                        b.putLong(at + IndRecords.DepositSample.DISTRICT, d.id);
                        b.putInt(at + IndRecords.DepositSample.GOOD, ind.goodId(good));
                        b.putInt(at + IndRecords.DepositSample.SWEEP, d.sweepNo);
                        b.putLong(at + IndRecords.DepositSample.REMAINING, remaining);
                        b.putInt(at + IndRecords.DepositSample.DIM, dimId);
                        b.putInt(at + IndRecords.DepositSample.CHUNK_X, chunk.getPos().x);
                        b.putInt(at + IndRecords.DepositSample.CHUNK_Z, chunk.getPos().z);
                        b.putInt(at + IndRecords.DepositSample.FLAGS, infinite ? IndRecords.DepositSample.FLAG_INFINITE : 0);
                        recordsSent++;
                        depositsSent++;
                    });
                } catch (RuntimeException e) {
                    ind.fail("deposits of " + p.getClass().getSimpleName() + " in " + chunk.getPos(), e);
                }
            }
        }
    }

    long depositsSent;

    public long depositsSent() {
        return depositsSent;
    }

    /** Test helper: reads one inventory now and returns its goods (no record is sent). */
    public Industry.GoodsCounter peek(ServerLevel level, District.Inventory inv) {
        Industry.GoodsCounter goods = ind.counter();
        access(level, inv).read(goods);
        return goods;
    }

    public long[] lastSweep(long district) {
        return lastSweep.get(district);
    }

    public long sweeps() {
        return sweeps;
    }

    public long inventoriesRead() {
        return inventoriesRead;
    }

    public long readNanos() {
        return readNanos;
    }

    public long slotsRead() {
        return slotsRead;
    }

    public long maxInventoryNanos() {
        return maxInventoryNanos;
    }

    static long chunkOf(net.minecraft.core.BlockPos pos) {
        return ChunkPos.asLong(pos.getX() >> 4, pos.getZ() >> 4);
    }
}
