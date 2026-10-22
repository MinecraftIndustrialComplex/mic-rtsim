package com.minecraftindustrialcomplex.mic_rtsim.industry;

import it.unimi.dsi.fastutil.longs.Long2IntOpenHashMap;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import net.minecraft.core.BlockPos;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;

import java.util.ArrayList;
import java.util.List;

/**
 * A district (INDUSTRY.md §2.1, I2): a set of chunks in one dimension, each in at most one district,
 * the unit that is measured, simulated, reconciled and attacked. Its id is stable for the world and
 * never reused (F3). Persisted in {@link IndustryData}; the fields below the line are this session's.
 */
public final class District {

    /** A boundary inventory (I3). Its index is per revision: the position in {@link #inventories}. */
    public record Inventory(int index, BlockPos pos, int kind, int itemSlots, long fluidCapacity, long energyCapacity) {
        public long chunk() {
            return ChunkPos.asLong(pos.getX() >> 4, pos.getZ() >> 4);
        }

        Inventory withIndex(int i) {
            return new Inventory(i, pos, kind, itemSlots, fluidCapacity, energyCapacity);
        }
    }

    /** A Create station (or another plugin's) inside the district. */
    public record Station(long id, BlockPos pos, String name) {}

    public final long id;
    public final ResourceKey<Level> dim;
    public final LongOpenHashSet chunks = new LongOpenHashSet();
    /**
     * Per member chunk, the seeds (machines a player placed) it held when detection last saw it loaded.
     * A district with no seed in any member chunk dissolves; the count is what lets detection decide that
     * for chunks that are unloaded, without loading them. A chunk absent from the map held none (storage,
     * a hopper chain, a belt: a member for what it touches, not for what it is).
     */
    public final Long2IntOpenHashMap seedCounts = new Long2IntOpenHashMap();
    /** Set by {@code /rtsim district rename}; wins over the station name. */
    public String customName;
    /** The first station's name (lowest position), else null. */
    public String stationName;
    /** The not-learnable reason (IND_DISTRICT aux), 0 learnable. */
    public int notLearnable;
    public int revision;
    public List<Inventory> inventories = new ArrayList<>();
    public List<Station> stations = new ArrayList<>();
    /** A creative motor or crate inside (plugin): sweeps carry the creative flag (I5). */
    public boolean creative;
    /** Positions the plugin checks each sweep (one kinetic block entity per network, for overstress). */
    public List<BlockPos> watch = new ArrayList<>();

    // ---- this session --------------------------------------------------------------------------
    /** IND_LOAD_STATE last sent (-1: never this session). */
    int loadState = -1;
    int tickingChunks;
    boolean watched;
    long watchedSentAt = Long.MIN_VALUE;
    boolean loadDirty = true;
    /** The revision last declared to the engine this world (persisted). */
    int declaredRevision;
    // bounding box in chunk coordinates
    int minCx, maxCx, minCz, maxCz;
    // sampler state
    long nextSweepAt;
    int sweepCursor = -1;
    int sweepNo;
    int sweepRecords;
    int sweepInventories;
    boolean sweepLive;
    int sweepRevision;
    boolean queued;
    /** IND_APPROACH (R5): each player's last distance to the district's unloaded part, and when: {distance, gameTime}. */
    final java.util.Map<java.util.UUID, long[]> approachSeen = new java.util.HashMap<>();

    public District(long id, ResourceKey<Level> dim) {
        this.id = id;
        this.dim = dim;
    }

    /** The name the engine and the ledger use: custom, else station, else the coordinates. */
    public String displayName() {
        if (customName != null && !customName.isBlank())
            return customName;
        if (stationName != null && !stationName.isBlank())
            return stationName;
        long c = anchorChunk();
        return "District " + (ChunkPos.getX(c) * 16 + 8) + ", " + (ChunkPos.getZ(c) * 16 + 8);
    }

    /** The lowest chunk (x, then z): the district's stable anchor for its default name. */
    public long anchorChunk() {
        long best = 0;
        boolean any = false;
        for (long c : chunks) {
            if (!any || ChunkPos.getX(c) < ChunkPos.getX(best)
                    || (ChunkPos.getX(c) == ChunkPos.getX(best) && ChunkPos.getZ(c) < ChunkPos.getZ(best))) {
                best = c;
                any = true;
            }
        }
        return best;
    }

    void recomputeBounds() {
        minCx = minCz = Integer.MAX_VALUE;
        maxCx = maxCz = Integer.MIN_VALUE;
        for (long c : chunks) {
            int x = ChunkPos.getX(c), z = ChunkPos.getZ(c);
            minCx = Math.min(minCx, x);
            maxCx = Math.max(maxCx, x);
            minCz = Math.min(minCz, z);
            maxCz = Math.max(maxCz, z);
        }
    }

    /** The seeds of the member chunks together. */
    public int seeds() {
        int n = 0;
        for (int v : seedCounts.values())
            n += v;
        return n;
    }

    /** The inventory at a position, or null. */
    public Inventory inventoryAt(BlockPos pos) {
        for (Inventory inv : inventories)
            if (inv.pos.equals(pos))
                return inv;
        return null;
    }

    public int loadState() {
        return loadState;
    }

    public boolean watched() {
        return watched;
    }

    @Override
    public String toString() {
        return "district " + id + " '" + displayName() + "' (" + chunks.size() + " chunks, " + inventories.size()
                + " inventories, revision " + revision + ")";
    }
}
