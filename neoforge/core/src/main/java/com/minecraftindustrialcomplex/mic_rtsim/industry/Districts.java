package com.minecraftindustrialcomplex.mic_rtsim.industry;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import it.unimi.dsi.fastutil.longs.Long2ByteOpenHashMap;
import it.unimi.dsi.fastutil.longs.Long2IntOpenHashMap;
import it.unimi.dsi.fastutil.longs.Long2LongOpenHashMap;
import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.longs.LongArrayList;
import it.unimi.dsi.fastutil.longs.LongArrayFIFOQueue;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.FullChunkStatus;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.tags.TagKey;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.EntityBlock;
import net.minecraft.world.level.block.ChestBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.ChestType;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.energy.IEnergyStorage;
import net.neoforged.neoforge.fluids.capability.IFluidHandler;
import net.neoforged.neoforge.items.IItemHandler;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;

/**
 * District detection and the per-chunk index (I2), and the district records the engine takes as
 * membership (IND_DISTRICT, IND_DISTRICT_CHUNKS, IND_INVENTORY, IND_STATION, IND_DISTRICT_REMOVED),
 * load states (IND_LOAD_STATE), watching (IND_WATCHED), edits (IND_EDIT) and approach signals.
 *
 * <p><b>Detection (DS-10).</b> Only <i>machines</i> start a district. A chunk is industrial when it holds a
 * <i>seed</i>: a block entity a player placed that {@link #seed} calls a machine, or when an industrial
 * block entity in it touches one in a member chunk across the chunk border (belts, pipes, shafts, and
 * storage or a hopper next to a machine: closed under physical connection), or a plugin links it (chain
 * conveyors). A district is a connected group of industrial chunks, 8-neighbour, plus the merge links of
 * {@code /rtsim district merge}, that holds at least one seed; a group with none dissolves. Chunks a
 * {@code split} pinned belong to their pinned district only, and excluded chunks to none.
 * Unloaded member chunks keep their membership (nothing new can be known about them), and their seed
 * counts, recorded per chunk in {@link District#seedCounts}, let a district whose machines are gone dissolve
 * without loading them.
 *
 * <p><b>What a machine is.</b> Never a block of the {@code minecraft} namespace (a player's furnace and
 * hopper sorter is a home, not a factory), never a block in {@code #mic_rtsim:district_stock} (chests,
 * barrels, vaults, tanks: boundary stock, I3) or {@code #mic_rtsim:district_not_seed} (other ticking storage
 * and the like), and never a furnace of any mod (a block entity that is an {@code AbstractFurnaceBlockEntity}).
 * Else a block in {@code #mic_rtsim:district_seed}; else what a plugin says
 * ({@link IndustryPlugin#seed}); else, for a modded block entity nobody knows, one that ticks on the server
 * and has an item, fluid or energy capability.
 *
 * <p>Detection runs after edits, at most every {@code rescanIntervalTicks}, over every member chunk of
 * the dimension: the edits are rare and the recompute is O(member chunks) plus the block entities of
 * the loaded ones.
 */
public final class Districts {

    /** {@code #mic_rtsim:district_stock}: the blocks whose inventories are boundary stock (I3). */
    public static final TagKey<Block> STOCK_TAG = TagKey.create(Registries.BLOCK,
            ResourceLocation.fromNamespaceAndPath(MicRtsim.MODID, "district_stock"));

    /** {@code #mic_rtsim:district_seed}: blocks that are machines whatever the generic rule would say (DS-10). */
    public static final TagKey<Block> SEED_TAG = TagKey.create(Registries.BLOCK,
            ResourceLocation.fromNamespaceAndPath(MicRtsim.MODID, "district_seed"));

    /** {@code #mic_rtsim:district_not_seed}: blocks that never start a district: ticking storage and the like (DS-10). */
    public static final TagKey<Block> NOT_SEED_TAG = TagKey.create(Registries.BLOCK,
            ResourceLocation.fromNamespaceAndPath(MicRtsim.MODID, "district_not_seed"));

    /** The JVM's class count, for the cold-code counter of a detection (R2-11). */
    private static final java.lang.management.ClassLoadingMXBean CLASSES = java.lang.management.ManagementFactory.getClassLoadingMXBean();

    private final Industry ind;
    private final IndustryData data;
    private final Map<ResourceKey<Level>, Long2ObjectOpenHashMap<District>> byChunk = new HashMap<>();
    private final Map<ResourceKey<Level>, LongOpenHashSet> dirty = new HashMap<>();
    /** Dimensions whose next detection covers every district (periodic), not only the edited ones. */
    private final Set<ResourceKey<Level>> fullDims = new HashSet<>();
    /** Loaded member chunks per district this session (ChunkEvent.Load and Unload). */
    private final Map<Long, LongOpenHashSet> loaded = new HashMap<>();
    private long lastRescan = Long.MIN_VALUE / 2;
    private long lastPeriodic;
    private final List<District> passList = new ArrayList<>();

    // counters
    long rescans;
    long rescanNanos;
    long lastRescanNanos;
    /** Ticks the last detection took, its longest tick, and the longest tick of ticked detection work since the module started. */
    int lastRescanSlices;
    long lastRescanLongestNanos;
    /** Where the last detection's time went: looking at roots (seeds, stations), walking reached chunks, and the commit. */
    long lastRescanRootNanos, lastRescanWalkNanos, lastRescanCommitNanos;
    long maxSliceNanos;
    /** Block entities the last detection looked at in all, and in its biggest tick (counts, not time: GameTests gate on these). */
    long lastRescanLooked;
    long lastRescanLongestLooked;
    /** The most one look at one chunk cost in the last detection: the part of a tick the budget does not bound. */
    long lastRescanLongestItem;
    /** What the longest tick was: the commit's, or a look at this many block entities. */
    String maxSliceWhat = "";
    /**
     * The last detection's costliest single look at one chunk, in wall time: what a tick's time limit
     * ({@code rescanSliceMicros}) does not bound, since a chunk is looked at whole (R2-11).
     */
    long lastRescanLongestItemNanos;
    /**
     * Chunks that loaded while the last detection looked, and classes the JVM loaded meanwhile (R2-11). The first
     * should be 0: the detection reads only loaded chunks, but another mod's code it calls (a capability, a plugin's
     * query) can read a neighbour that has a ticket and is still loading, and that read waits for the load. The
     * second is cold code.
     */
    long lastRescanChunkLoads, lastRescanClassLoads;
    /** The block entity type whose look loaded the last detection's first chunk, or "". */
    String lastRescanLoadedBy = "";
    /**
     * R2-11: the stack of the first chunk load inside a detection step is logged once a session. Detection reads
     * only FULL chunks, so a load there is a read that waited for a chunk still loading (a plugin's, say).
     */
    boolean loadStackLogged;
    long declared;

    Districts(Industry ind) {
        this.ind = ind;
        this.data = ind.data;
        rebuildIndex();
    }

    private void rebuildIndex() {
        byChunk.clear();
        for (District d : data.districts.values()) {
            Long2ObjectOpenHashMap<District> m = byChunk.computeIfAbsent(d.dim, k -> new Long2ObjectOpenHashMap<>());
            for (long c : d.chunks)
                m.put(c, d);
            d.recomputeBounds();
        }
    }

    /** Detections finished since the module started (GameTests count them to tell one detection from two). */
    public long rescans() {
        return rescans;
    }

    /** The last detection's wall time, in nanoseconds (/rtsim district perf, GameTests). */
    public long lastRescanNanos() {
        return lastRescanNanos;
    }

    /** The ticks the last detection was spread over. */
    public int lastRescanSlices() {
        return lastRescanSlices;
    }

    /** Where the last detection's time went, for the perf line and for profiling (SG60). */
    public String lastRescanPhases() {
        return "roots " + lastRescanRootNanos / 1000 + " us, walks " + lastRescanWalkNanos / 1000 + " us, commit "
                + lastRescanCommitNanos / 1000 + " us";
    }

    /** The longest single tick of the last detection (a slice, or the commit with it). */
    public long lastRescanLongestNanos() {
        return lastRescanLongestNanos;
    }

    /** Block entities the last detection looked at in all (a chunk costs its block entities, and one more). */
    public long lastRescanLooked() {
        return lastRescanLooked;
    }

    /** Block entities the last detection looked at in its biggest tick: under the budget before the last chunk, and that chunk. */
    public long lastRescanLongestLooked() {
        return lastRescanLongestLooked;
    }

    /** What the costliest single look at a chunk spent in the last detection (a tick's spend is at most the budget less one, plus this). */
    public long lastRescanLongestItem() {
        return lastRescanLongestItem;
    }

    /** The last detection's costliest single look at one chunk, in nanoseconds (R2-11). */
    public long lastRescanLongestItemNanos() {
        return lastRescanLongestItemNanos;
    }

    /** Chunks that loaded while the last detection looked (R2-11: should be 0). */
    public long lastRescanChunkLoads() {
        return lastRescanChunkLoads;
    }

    /** Classes the JVM loaded while the last detection looked (R2-11: cold code). */
    public long lastRescanClassLoads() {
        return lastRescanClassLoads;
    }

    /** What the last detection's time limits and loads were, for the perf line (R2-11). */
    public String lastRescanCold() {
        return "longest chunk " + lastRescanLongestItemNanos / 1000 + " us, " + lastRescanClassLoads + " classes loaded, "
                + lastRescanChunkLoads + " chunk loads" + (lastRescanLoadedBy.isEmpty() ? "" : " (first by " + lastRescanLoadedBy + ")");
    }

    /**
     * The longest single tick of detection work (a slice, or the commit with it) since the module started. A
     * detection a command runs in one call ({@link #rescanNow}) is not counted: it is asked for, not scheduled.
     */
    public long maxSliceNanos() {
        return maxSliceNanos;
    }

    /** Whether a detection is under way or waiting to start (GameTests). */
    public boolean rescanning() {
        return job != null || !passes.isEmpty();
    }

    public District at(ResourceKey<Level> dim, long chunk) {
        Long2ObjectOpenHashMap<District> m = byChunk.get(dim);
        return m == null ? null : m.get(chunk);
    }

    public java.util.Collection<District> all() {
        return data.districts.values();
    }

    public District byName(String name) {
        for (District d : data.districts.values())
            if (d.displayName().equalsIgnoreCase(name) || Long.toString(d.id).equals(name))
                return d;
        return null;
    }

    public void markDirty(ResourceKey<Level> dim, long chunk) {
        dirty.computeIfAbsent(dim, k -> new LongOpenHashSet()).add(chunk);
    }

    /** Detection now, whatever the interval, in this tick (GameTests, and the baseline the commands' ticked detection is checked against). */
    public void rescanNow(ServerLevel level) {
        ResourceKey<Level> dim = level.dimension();
        List<Runnable> waiters = requeueRescans(dim);
        LongOpenHashSet taken = dirty.remove(dim);
        Rescan r = new Rescan(level, true, taken == null ? new LongOpenHashSet() : taken);
        r.manual = true;
        while (!r.step(Integer.MAX_VALUE))
            ;
        lastRescan = ind.gameTime();
        fire(waiters);
    }

    /**
     * A full detection of {@code level}'s dimension, as {@link #rescanNow} runs it but spread over the next ticks as a
     * ticked one is (DS-16): what the operator commands that ask for one ({@code rescan}, {@code merge}, {@code split},
     * {@code exclude}, {@code include}) use, so that none of them is one big tick at a large district count. It starts
     * at the next tick, not at the next {@code rescanIntervalTicks}, ahead of the passes waiting; a detection of the
     * dimension under way (or its passes waiting) is put back into the dirty set and covered by this one, with the
     * callbacks it was to run. {@code done} runs in the tick the detection commits, after the districts changed.
     * Detections of the dimension that began earlier and are cancelled by this one are not reported done by their
     * own end: {@code done} callbacks of all of them run at this one's.
     */
    public void rescanSoon(ServerLevel level, Runnable done) {
        ResourceKey<Level> dim = level.dimension();
        List<Runnable> waiters = requeueRescans(dim);
        waiters.add(done);
        LongOpenHashSet taken = dirty.remove(dim);
        fullDims.remove(dim);
        passes.addFirst(new Pass(dim, taken == null ? new LongOpenHashSet() : taken, true, waiters));
    }

    /**
     * True while a detection of {@code dim} that a command asked for ({@link #rescanSoon}) is under way or waiting:
     * the district set is not yet what that command's edit makes it, so a command that reads the set ({@code merge},
     * {@code split}) waits for the result line (DS-16).
     */
    public boolean commandPending(ResourceKey<Level> dim) {
        if (job != null && job.dim == dim && !job.waiters.isEmpty())
            return true;
        for (Pass p : passes)
            if (p.dim == dim && !p.waiters.isEmpty())
                return true;
        return false;
    }

    /** Runs the callbacks of a finished detection; one that throws does not stop the others or the tick. */
    private static void fire(List<Runnable> waiters) {
        for (Runnable w : waiters) {
            try {
                w.run();
            } catch (RuntimeException e) {
                MicRtsim.LOGGER.warn("[industry] a district command's completion report failed", e);
            }
        }
    }

    // ---- session start --------------------------------------------------------------------------

    void sessionStart(long now) {
        // After a reinstall reset, chunks loaded before the session (spawn chunks, forced chunks) get their seeds back first (UR-1).
        if (!data.readopt.isEmpty())
            for (Map.Entry<ResourceKey<Level>, LongOpenHashSet> e : new ArrayList<>(data.readopt.entrySet())) {
                ServerLevel level = ind.level(e.getKey());
                if (level == null)
                    continue;
                for (long c : e.getValue().toLongArray()) {
                    LevelChunk chunk = level.getChunkSource().getChunkNow(ChunkPos.getX(c), ChunkPos.getZ(c));
                    if (chunk != null)
                        readopt(level, chunk);
                }
            }
        for (District d : data.districts.values()) {
            if (d.declaredRevision < d.revision || d.declaredRevision == 0)
                declare(d, now);
            ServerLevel level = ind.level(d.dim);
            LongOpenHashSet set = new LongOpenHashSet();
            if (level != null)
                for (long c : d.chunks)
                    if (level.getChunkSource().getChunkNow(ChunkPos.getX(c), ChunkPos.getZ(c)) != null)
                        set.add(c);
            loaded.put(d.id, set);
            d.loadDirty = true;
            // Chunks loaded before the session (spawn chunks) report their epochs now (I8).
            if (level != null)
                for (long c : set)
                    ind.reconciler.chunkLoaded(level, level.getChunkSource().getChunkNow(ChunkPos.getX(c), ChunkPos.getZ(c)), d, now);
        }
    }

    // ---- chunk events (via IndustryEvents) ------------------------------------------------------

    void onChunkLoad(ServerLevel level, LevelChunk chunk, long now) {
        long c = chunk.getPos().toLong();
        readopt(level, chunk);
        District d = at(level.dimension(), c);
        if (d != null) {
            loaded.computeIfAbsent(d.id, k -> new LongOpenHashSet()).add(c);
            d.loadDirty = true;
            ind.reconciler.chunkLoaded(level, chunk, d, now);
        } else {
            IndustryAttachments.Seeds s = chunk.getExistingDataOrNull(IndustryAttachments.SEEDS.get());
            if (s != null && !s.isEmpty())
                markDirty(level.dimension(), c); // seeds with no district (a crash between): detect again
        }
    }

    /**
     * ChunkEvent.Load of a chunk with a held reconcile (R2): it and the district's other loaded chunks
     * tick no block entity until the next drain, where the reconcile lands. A machine next door would
     * otherwise pull from or push into an inventory whose count the reconcile is about to set.
     */
    void holdTicking(ServerLevel level, LevelChunk chunk) {
        TickGate.hold(chunk);
        District d = at(level.dimension(), chunk.getPos().toLong());
        LongOpenHashSet set = d == null ? null : loaded.get(d.id);
        if (set == null)
            return;
        for (long c : set) {
            LevelChunk other = level.getChunkSource().getChunkNow(ChunkPos.getX(c), ChunkPos.getZ(c));
            if (other != null && other != chunk)
                TickGate.hold(other);
        }
    }

    void onChunkUnload(ServerLevel level, LevelChunk chunk) {
        long c = chunk.getPos().toLong();
        District d = at(level.dimension(), c);
        if (d != null) {
            LongOpenHashSet set = loaded.get(d.id);
            if (set != null)
                set.remove(c);
            d.loadDirty = true;
            ind.sampler.chunkUnloaded(d);
            // I7: the engine's replay of the district's unloaded part takes the chunk back.
            ByteBuffer r = ind.journal(IndRecords.IND_CHUNK_UNLOADED, IndRecords.ChunkUnloaded.SIZE, 0);
            r.putLong(IndRecords.ChunkUnloaded.DISTRICT, d.id);
            r.putInt(IndRecords.ChunkUnloaded.DIM, ind.dimId(level.dimension()));
            r.putInt(IndRecords.ChunkUnloaded.X, chunk.getPos().x);
            r.putInt(IndRecords.ChunkUnloaded.Z, chunk.getPos().z);
            ind.commit(r);
        }
    }

    /**
     * U10, the module switched on again: every district is declared anew, its load state, its watched flag
     * and its chunks reported anew (the session start that follows does the chunks).
     */
    void forgetAnnounced() {
        for (District d : data.districts.values()) {
            d.declaredRevision = 0;
            d.loadState = -1;
            d.loadDirty = true;
            d.watched = false;
            d.watchedSentAt = Long.MIN_VALUE;
        }
    }

    public int loadedChunks(District d) {
        LongOpenHashSet set = loaded.get(d.id);
        return set == null ? 0 : set.size();
    }

    // ---- seeds and edits ------------------------------------------------------------------------

    /** A block a player placed: a seed if it is industrial, and an edit of the district it is in (I5). */
    void playerPlaced(ServerLevel level, BlockPos pos, UUID player, long now) {
        LevelChunk chunk = level.getChunkSource().getChunkNow(pos.getX() >> 4, pos.getZ() >> 4);
        if (chunk == null)
            return;
        BlockEntity be = chunk.getBlockEntities().get(pos);
        District d = at(level.dimension(), chunk.getPos().toLong());
        if (be != null && (seed(level, be) || mayBecomeSeed(be))) {
            IndustryAttachments.Seeds s = chunk.getData(IndustryAttachments.SEEDS.get());
            chunk.setData(IndustryAttachments.SEEDS.get(), s.plus(pos.asLong()));
            chunk.setUnsaved(true);
            markDirty(level.dimension(), chunk.getPos().toLong());
        }
        if (d != null) {
            edit(d, IndRecords.Edit.PLACED, player, pos);
            if (be != null)
                markDirty(level.dimension(), chunk.getPos().toLong());
        } else if (be != null && networkPart(be))
            editReached(level, chunk.getPos().toLong(), IndRecords.Edit.PLACED, player, pos);
        if (d == null && be != null && industrial(level, be, kindCache())) {
            // Industrial but no seed, outside any district: a station (whose track graph forms a moment after it
            // is placed) beside one joins it at the next detection (DS-10); it starts none.
            markDirty(level.dimension(), chunk.getPos().toLong());
        }
    }

    /** A block a player broke (before it is gone): an edit, and a rescan if it was industrial. */
    void playerBroke(ServerLevel level, BlockPos pos, UUID player) {
        long c = ChunkPos.asLong(pos.getX() >> 4, pos.getZ() >> 4);
        District d = at(level.dimension(), c);
        BlockEntity broken = level.getBlockEntity(pos);
        boolean be = broken != null;
        if (d != null)
            edit(d, IndRecords.Edit.BROKEN, player, pos);
        else if (be && networkPart(broken))
            editReached(level, c, IndRecords.Edit.BROKEN, player, pos);
        if (be)
            markDirty(level.dimension(), c);
    }

    /** Whether a plugin counts {@code be} as a part of a network a compile follows out of its district (SC77). */
    private static boolean networkPart(BlockEntity be) {
        for (IndustryPlugin plugin : IndustryApi.PLUGINS)
            if (plugin.networkPart(be))
                return true;
        return false;
    }

    /**
     * A player's network part ({@link IndustryPlugin#networkPart}: a kinetic block entity) placed or broken in
     * chunk {@code c} of no district: an edit of every district whose last compile has a kinetic network
     * reaching that chunk (SECTIONS.md §3.6, choice SC77). The
     * position lies in no node box of theirs, so it stales no section; it drops their open windows and
     * recompiles them, which updates the network's part.
     */
    private void editReached(ServerLevel level, long c, int kind, UUID player, BlockPos pos) {
        for (long id : ind.glass.reachedBy(level.dimension(), c)) {
            District o = ind.district(id);
            if (o != null)
                edit(o, kind, player, pos);
        }
    }

    public void edit(District d, int kind, UUID player, BlockPos pos) {
        ByteBuffer r = ind.journal(IndRecords.IND_EDIT, IndRecords.Edit.SIZE, kind);
        r.putLong(IndRecords.Edit.DISTRICT, d.id);
        r.putLong(IndRecords.Edit.PLAYER_MOST, player == null ? 0 : player.getMostSignificantBits());
        r.putLong(IndRecords.Edit.PLAYER_LEAST, player == null ? 0 : player.getLeastSignificantBits());
        r.putInt(IndRecords.Edit.X, pos.getX());
        r.putInt(IndRecords.Edit.Y, pos.getY());
        r.putInt(IndRecords.Edit.Z, pos.getZ());
        ind.commit(r);
        ind.glass.edited(d); // an edit recompiles the glass box's graph (GLASSBOX.md §1.5)
    }

    public void discard(District d, int reason) {
        discard(d, reason, 0);
    }

    /**
     * A discard a station's call made ({@code station}, 0 for none): the engine discards the windows of
     * the units the station's last {@code IND_STATION_PORTS} named (SECTIONS.md §3.6).
     */
    public void discard(District d, int reason, long station) {
        ByteBuffer r = ind.journal(IndRecords.IND_DISCARD, IndRecords.Discard.SIZE, reason);
        r.putLong(IndRecords.Discard.DISTRICT, d.id);
        r.putLong(IndRecords.Discard.STATION, station);
        ind.commit(r);
    }

    /** What {@link #adoptAround} did: the seeds adopted, and whether the detection around them is done already. */
    public record Adopted(int seeds, boolean detected) {}

    /**
     * {@code /rtsim district adopt} (DS-12): adopts the machines around {@code centre}, then detects around the
     * chunks it changed only: the districts owning or touching them, grown as a ticked pass grows, never the
     * whole dimension. That finishes in this call when the neighbourhood is within one tick's block entity
     * budget; else the detection goes on over the next ticks as a ticked one does, and {@code detected} is false.
     * A detection of the same dimension under way takes the adopted chunks into itself ({@link Rescan#join}, SG55,
     * SG56): the ones it has not counted yet are looked at as roots, with the districts around them, and a chunk it
     * counted before the adopt has its seeds counted again; it finishes with the others, as one detection.
     */
    public Adopted adoptAround(ServerLevel level, ChunkPos centre, int radius) {
        ResourceKey<Level> dim = level.dimension();
        LongOpenHashSet changed = new LongOpenHashSet();
        int n = adopt(level, centre, radius, changed);
        if (changed.isEmpty())
            return new Adopted(n, true);
        // Only these chunks leave the dirty set; other edits wait for their own pass.
        LongOpenHashSet pending = dirty.get(dim);
        if (pending != null) {
            pending.removeAll(changed);
            if (pending.isEmpty() && !fullDims.contains(dim))
                dirty.remove(dim);
        }
        if (job != null && job.dim == dim) {
            // The running detection takes the chunks into itself: no second detection, none after it.
            job.join(changed);
            return new Adopted(n, false);
        }
        Rescan r = new Rescan(level, false, changed);
        if (r.step(IndustryConfig.get(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK)))
            return new Adopted(n, true);
        if (job == null)
            job = r;
        else
            passes.addFirst(new Pass(dim, changed, false)); // another dimension's detection is under way
        return new Adopted(n, false);
    }

    /** Adopts every machine in the chunks around {@code centre} as a seed (worlds made before the mod). */
    public int adopt(ServerLevel level, ChunkPos centre, int radius) {
        return adopt(level, centre, radius, new LongOpenHashSet());
    }

    /** As {@link #adopt(ServerLevel, ChunkPos, int)}; the chunks that gained seeds (marked dirty) are added to {@code changed}. */
    private int adopt(ServerLevel level, ChunkPos centre, int radius, LongOpenHashSet changed) {
        int n = 0;
        Long2ByteOpenHashMap cache = kindCache();
        for (int dx = -radius; dx <= radius; dx++) {
            for (int dz = -radius; dz <= radius; dz++) {
                LevelChunk chunk = level.getChunkSource().getChunkNow(centre.x + dx, centre.z + dz);
                if (chunk != null)
                    n += adoptChunk(level, chunk, cache, changed);
            }
        }
        return n;
    }

    /** Adopts every machine of one loaded chunk as a seed; returns the seeds added (the chunk is marked dirty and added to {@code changed} if any). */
    private int adoptChunk(ServerLevel level, LevelChunk chunk, Long2ByteOpenHashMap cache, LongOpenHashSet changed) {
        IndustryAttachments.Seeds s = chunk.getData(IndustryAttachments.SEEDS.get());
        IndustryAttachments.Seeds before = s;
        for (BlockEntity be : chunk.getBlockEntities().values()) {
            if (seed(level, be, cache) || mayBecomeSeed(be)) {
                s = s.plus(be.getBlockPos().asLong());
            }
        }
        if (s == before)
            return 0;
        chunk.setData(IndustryAttachments.SEEDS.get(), s);
        chunk.setUnsaved(true);
        markDirty(level.dimension(), chunk.getPos().toLong());
        changed.add(chunk.getPos().toLong());
        return s.positions().length - before.positions().length;
    }

    // ---- the reinstall reset (UR-1) -------------------------------------------------------------

    /** Chunks adopted again after a reinstall reset, those found with their seeds kept, and the seeds adopted. */
    long readoptChunks, readoptKept, readoptSeeds;

    /**
     * A reinstall reset (UR-1): every member chunk that held seeds is to be looked at again at its first load. The
     * gap's saves dropped the seed attachment of each chunk that loaded while rtsim was out, so without this every
     * district would dissolve as its chunks load (round 2 of the deploy rehearsal). Kept with the districts in
     * {@code mic_rtsim_industry.dat}, so a crash before every chunk has loaded keeps the rest.
     */
    void planReadopt() {
        int n = 0;
        for (District d : data.districts.values())
            for (it.unimi.dsi.fastutil.longs.Long2IntMap.Entry e : d.seedCounts.long2IntEntrySet())
                if (e.getIntValue() > 0 && data.readopt.computeIfAbsent(d.dim, k -> new LongOpenHashSet()).add(e.getLongKey()))
                    n++;
        data.setDirty();
        MicRtsim.LOGGER.warn("[industry] reinstall reset: {} chunks of {} districts are adopted again as they load", n, data.districts.size());
    }

    /**
     * A chunk to adopt again (UR-1) loaded: it leaves the set; if the gap stripped its seed attachment, its machines
     * are adopted, as {@code rtsim district adopt} would (radius 0), before any detection looks at it. A chunk that
     * kept its attachment did not load during the gap and is left as it is.
     */
    private void readopt(ServerLevel level, LevelChunk chunk) {
        LongOpenHashSet set = data.readopt.get(level.dimension());
        if (set == null || !set.remove(chunk.getPos().toLong()))
            return;
        if (set.isEmpty())
            data.readopt.remove(level.dimension());
        data.setDirty();
        IndustryAttachments.Seeds s = chunk.getExistingDataOrNull(IndustryAttachments.SEEDS.get());
        if (s != null && !s.isEmpty()) {
            readoptKept++;
            return;
        }
        readoptChunks++;
        readoptSeeds += adoptChunk(level, chunk, kindCache(), new LongOpenHashSet());
    }

    /** Chunks adopted again after a reinstall reset (UR-1). */
    public long readoptChunks() {
        return readoptChunks;
    }

    /** Chunks planned for re-adoption that loaded with their seeds kept (they did not load during the gap). */
    public long readoptKept() {
        return readoptKept;
    }

    /** Machines adopted again after a reinstall reset. */
    public long readoptSeeds() {
        return readoptSeeds;
    }

    /** Chunks still to adopt again after a reinstall reset (UR-1). */
    public int readoptLeft() {
        int n = 0;
        for (LongOpenHashSet set : data.readopt.values())
            n += set.size();
        return n;
    }

    /** The reinstall reset's re-adoption, for {@code rtsim perf}, or null when there is none. */
    public String readoptSummary() {
        int left = readoptLeft();
        if (left == 0 && readoptChunks == 0 && readoptKept == 0)
            return null;
        return "re-adoption after the reinstall reset: " + readoptChunks + " chunks adopted again (" + readoptSeeds + " machines), "
                + readoptKept + " kept their seeds, " + left + " not loaded yet";
    }

    // ---- the tick -------------------------------------------------------------------------------

    void tick(long now, long ticks) {
        int interval = IndustryConfig.get(IndustryConfig.RESCAN_INTERVAL);
        if (job == null && passes.isEmpty() && !dirty.isEmpty() && now - lastRescan >= interval) {
            // A pass: what is dirty now, a dimension at a time. Edits from here on wait for the next pass.
            for (Map.Entry<ResourceKey<Level>, LongOpenHashSet> e : new ArrayList<>(dirty.entrySet()))
                if (ind.level(e.getKey()) != null)
                    passes.add(new Pass(e.getKey(), e.getValue(), fullDims.contains(e.getKey())));
            dirty.clear();
            fullDims.clear();
            if (passes.isEmpty())
                lastRescan = now;
        }
        if (job == null && !passes.isEmpty()) {
            Pass p = passes.poll();
            ServerLevel level = ind.level(p.dim);
            if (level != null) {
                job = new Rescan(level, p.full, p.dirtyHere);
                job.waiters.addAll(p.waiters);
            } else {
                fire(p.waiters); // the dimension is gone: a command waiting on it hears that the districts are what they are
            }
        }
        if (job != null && job.step(IndustryConfig.get(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK))) {
            List<Runnable> waiters = job.waiters;
            job = null;
            if (passes.isEmpty())
                lastRescan = now;
            fire(waiters);
        }
        if (now - lastPeriodic >= IndustryConfig.get(IndustryConfig.PERIODIC_RESCAN)) {
            lastPeriodic = now;
            for (District d : data.districts.values())
                if (loadedChunks(d) > 0 || seedless(d)) {
                    dirty.computeIfAbsent(d.dim, k -> new LongOpenHashSet());
                    fullDims.add(d.dim);
                }
        }
        // Load states and watching: every district with a loaded chunk, every interval, and at once on
        // a load or unload of one of its chunks.
        int check = IndustryConfig.get(IndustryConfig.LOAD_CHECK_INTERVAL);
        passList.clear();
        passList.addAll(data.districts.values());
        for (District d : passList) {
            int n = loadedChunks(d);
            boolean due = d.loadDirty || (n > 0 && (ticks + d.id) % check == 0) || d.loadState < 0;
            if (due) {
                d.loadDirty = false;
                updateLoadState(d, now);
            }
            if (n > 0 && (ticks + d.id) % check == 0)
                updateWatched(d, now);
        }
        if (IndustryConfig.approachSignal() && ticks % 20 == 0)
            approach(now);
    }

    /** A district with no recorded seed that no operator pinned: detection dissolves it, loaded or not (DS-10). */
    private boolean seedless(District d) {
        return d.seedCounts.isEmpty() && !data.pins(d.dim).containsValue(d.id);
    }

    private void updateLoadState(District d, long now) {
        ServerLevel level = ind.level(d.dim);
        int ticking = 0;
        LongOpenHashSet set = loaded.get(d.id);
        if (level != null && set != null && !set.isEmpty())
            for (long c : d.chunks)
                if (blockTicking(level, c))
                    ticking++;
        int total = d.chunks.size();
        int state = ticking == 0 ? IndRecords.LoadState.UNLOADED : ticking == total ? IndRecords.LoadState.LIVE : IndRecords.LoadState.PARTIAL;
        d.tickingChunks = ticking;
        if (state != d.loadState) {
            d.loadState = state;
            ByteBuffer r = ind.journal(IndRecords.IND_LOAD_STATE, IndRecords.LoadState.SIZE, state);
            r.putLong(IndRecords.LoadState.DISTRICT, d.id);
            r.putInt(IndRecords.LoadState.TICKING_CHUNKS, ticking);
            r.putInt(IndRecords.LoadState.TOTAL_CHUNKS, total);
            ind.commit(r);
            if (state == IndRecords.LoadState.UNLOADED)
                d.watched = false; // an unloaded district is never watched (F22)
        }
    }

    /**
     * True while the chunk's block entities tick: loaded, at least BLOCK_TICKING, and inside the
     * simulation distance ({@code shouldTickBlocksAt}). Player tickets give chunks within the view
     * distance a ticking full status, but {@code LevelChunk.isTicking} also needs the block-ticking range,
     * which is the simulation distance.
     */
    public static boolean blockTicking(ServerLevel level, long chunk) {
        LevelChunk c = level.getChunkSource().getChunkNow(ChunkPos.getX(chunk), ChunkPos.getZ(chunk));
        return c != null && c.getFullStatus().isOrAfter(FullChunkStatus.BLOCK_TICKING) && level.shouldTickBlocksAt(chunk);
    }

    /** True while every chunk of the district is block-ticking. */
    public boolean allTicking(District d) {
        ServerLevel level = ind.level(d.dim);
        if (level == null || loadedChunks(d) < d.chunks.size())
            return false;
        for (long c : d.chunks)
            if (!blockTicking(level, c))
                return false;
        return true;
    }

    /**
     * IND_WATCHED (F22): a player in the dimension within the server's simulation distance of one of
     * the district's chunks, where puppets could fight for real. Sent on a change and every
     * {@code watchedResendTicks} while the district has loaded chunks.
     */
    private void updateWatched(District d, long now) {
        ServerLevel level = ind.level(d.dim);
        boolean w = false;
        if (level != null && d.loadState != IndRecords.LoadState.UNLOADED) {
            int sim = ind.server.getPlayerList().getSimulationDistance();
            for (ServerPlayer p : level.players()) {
                int cx = p.chunkPosition().x, cz = p.chunkPosition().z;
                if (cx >= d.minCx - sim && cx <= d.maxCx + sim && cz >= d.minCz - sim && cz <= d.maxCz + sim) {
                    w = nearAChunk(d, cx, cz, sim);
                    if (w)
                        break;
                }
            }
        }
        if (w != d.watched || d.watchedSentAt == Long.MIN_VALUE || now - d.watchedSentAt >= IndustryConfig.get(IndustryConfig.WATCHED_RESEND)) {
            int at = ind.latest(IndRecords.IND_WATCHED, IndRecords.DistrictState.SIZE, w ? 1 : 0);
            if (at >= 0) {
                ByteBuffer b = ind.open();
                b.putLong(at + IndRecords.DistrictState.GAME_TIME, now);
                b.putLong(at + IndRecords.DistrictState.DISTRICT, d.id);
                d.watchedSentAt = now;
                d.watched = w; // a dropped change is tried again next check, not after the resend interval
            }
        }
    }

    private static boolean nearAChunk(District d, int cx, int cz, int r) {
        for (long c : d.chunks)
            if (Math.abs(ChunkPos.getX(c) - cx) <= r && Math.abs(ChunkPos.getZ(c) - cz) <= r)
                return true;
        return false;
    }

    /**
     * IND_APPROACH (choice H23, R5): a player within {@code approachRadiusBlocks} of a district chunk that is
     * not loaded, whether the district is unloaded or partly loaded (I7: the engine replays the unloaded
     * part, so its reconcile can go ahead too). Every 20 ticks. The distance is to the nearest chunk that is
     * not loaded. Closing is per player, from that player's own distance at the last check: a player who
     * stands still or walks away is not closing, whoever else is near. When some player is closing, the
     * record carries the closing player who arrives first (distance and ETA); else the nearest player's
     * distance and an unknown ETA. Once every chunk is loaded, no more records.
     */
    private void approach(long now) {
        int radius = IndustryConfig.get(IndustryConfig.APPROACH_RADIUS);
        for (ServerLevel level : ind.server.getAllLevels()) {
            List<ServerPlayer> players = level.players();
            for (District d : data.districts.values()) {
                if (d.dim != level.dimension() || d.chunks.isEmpty())
                    continue;
                LongOpenHashSet set = loaded.get(d.id);
                if (players.isEmpty() || (set != null && set.size() >= d.chunks.size())) {
                    d.approachSeen.clear();
                    continue;
                }
                int best = Integer.MAX_VALUE;
                int closingDistance = Integer.MAX_VALUE;
                long eta = 0xFFFF_FFFFL;
                for (ServerPlayer p : players) {
                    double px = p.getX(), pz = p.getZ();
                    if (toBox(px, pz, d.minCx, d.maxCx, d.minCz, d.maxCz) > radius) {
                        d.approachSeen.remove(p.getUUID());
                        continue;
                    }
                    int dist = Integer.MAX_VALUE;
                    for (long c : d.chunks)
                        if (set == null || !set.contains(c))
                            dist = Math.min(dist, (int) toBox(px, pz, ChunkPos.getX(c), ChunkPos.getX(c), ChunkPos.getZ(c), ChunkPos.getZ(c)));
                    if (dist > radius) {
                        d.approachSeen.remove(p.getUUID());
                        continue;
                    }
                    long[] seen = d.approachSeen.get(p.getUUID());
                    if (seen != null && now > seen[1] && seen[0] > dist) {
                        double rate = (double) (seen[0] - dist) / (now - seen[1]);
                        long e = Math.min(0xFFFF_FFFEL, (long) Math.ceil(dist / rate));
                        if (closingDistance == Integer.MAX_VALUE || e < eta) {
                            eta = e;
                            closingDistance = dist;
                        }
                    }
                    if (seen == null)
                        d.approachSeen.put(p.getUUID(), new long[] {dist, now});
                    else {
                        seen[0] = dist;
                        seen[1] = now;
                    }
                    best = Math.min(best, dist);
                }
                if (d.approachSeen.size() > players.size())
                    d.approachSeen.keySet().removeIf(u -> level.getPlayerByUUID(u) == null);
                if (best == Integer.MAX_VALUE)
                    continue;
                boolean closing = closingDistance != Integer.MAX_VALUE;
                int at = ind.latest(IndRecords.IND_APPROACH, IndRecords.Approach.SIZE, closing ? IndRecords.Approach.FLAG_CLOSING : 0);
                if (at < 0)
                    continue;
                ByteBuffer b = ind.open();
                b.putLong(at + IndRecords.Approach.GAME_TIME, now);
                b.putLong(at + IndRecords.Approach.DISTRICT, d.id);
                b.putInt(at + IndRecords.Approach.DISTANCE, closing ? closingDistance : best);
                b.putInt(at + IndRecords.Approach.ETA_TICKS, (int) eta);
            }
        }
    }

    /** Horizontal blocks from (x, z) to the box of chunks [minCx, maxCx] x [minCz, maxCz]; 0 inside. */
    private static double toBox(double x, double z, int minCx, int maxCx, int minCz, int maxCz) {
        double dx = Math.max(0, Math.max(minCx * 16 - x, x - (maxCx * 16 + 16)));
        double dz = Math.max(0, Math.max(minCz * 16 - z, z - (maxCz * 16 + 16)));
        return Math.sqrt(dx * dx + dz * dz);
    }

    // ---- detection ------------------------------------------------------------------------------

    /** One industrial block entity's touch across a chunk border. */
    private record Touch(long a, long b, BlockPos pa, BlockPos pb) {}

    private record Candidate(BlockPos pos, int kind) {}

    private final class Scan {
        /** Each scanned chunk's boundary inventories, described when the chunk was looked at (a chunk may unload before the commit). */
        final Map<Long, List<District.Inventory>> inventories = new HashMap<>();
        final Map<Long, List<IndustryPlugin.StationInfo>> stations = new HashMap<>();
        final LongOpenHashSet creative = new LongOpenHashSet();
        final Map<Long, List<BlockPos>> watch = new HashMap<>();
        final List<Touch> touches = new ArrayList<>();
        final List<long[]> links = new ArrayList<>();
        /** What the detection learned of each block entity, by position: {@link #K_INDUSTRIAL} and {@link #K_SEED}. */
        final Long2ByteOpenHashMap kinds = kindCache();
        /** Live seeds of each loaded chunk the detection looked at (every candidate and every reached chunk). */
        final Long2IntOpenHashMap seedCounts = new Long2IntOpenHashMap();
        /** Chunks looked at for a station, and the ones that hold one. */
        final LongOpenHashSet stationLooked = new LongOpenHashSet();
        final LongOpenHashSet stationChunks = new LongOpenHashSet();
        /** The block entity type whose look first loaded a chunk (R2-11), or null. */
        String loadedBy;

        /** After a look at {@code be} that began when {@link IndustryEvents#chunkLoads} was {@code before}: names it if a chunk loaded meanwhile. */
        void loads(long before, BlockEntity be) {
            if (loadedBy == null && IndustryEvents.chunkLoads() != before) {
                ResourceLocation key = BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(be.getType());
                loadedBy = key == null ? be.getClass().getName() : key.toString();
            }
        }
    }

    /**
     * One dimension's share of a detection pass: the dirty chunks taken when the pass began, and what to run when the
     * detection that does it is done (a command's report, {@link #rescanSoon}).
     */
    private record Pass(ResourceKey<Level> dim, LongOpenHashSet dirtyHere, boolean full, List<Runnable> waiters) {
        Pass(ResourceKey<Level> dim, LongOpenHashSet dirtyHere, boolean full) {
            this(dim, dirtyHere, full, new ArrayList<>());
        }
    }

    /** A detection under way (at most one at a time), and the dimensions waiting behind it. */
    private Rescan job;
    private final java.util.ArrayDeque<Pass> passes = new java.util.ArrayDeque<>();

    /**
     * Puts the detection under way for {@code dim}, and its queued passes, back where edits wait. Returns the
     * callbacks they were to run (a new list the caller may add to): {@link #rescanNow} and {@link #rescanSoon} run
     * them with their own detection, the GameTests' helpers that wipe the dimension let them go.
     */
    private List<Runnable> requeueRescans(ResourceKey<Level> dim) {
        List<Runnable> waiters = new ArrayList<>();
        if (job != null && job.dim == dim) {
            requeue(job.dim, job.dirtyHere, job.full);
            waiters.addAll(job.waiters);
            job = null;
        }
        for (java.util.Iterator<Pass> it = passes.iterator(); it.hasNext(); ) {
            Pass p = it.next();
            if (p.dim == dim) {
                requeue(p.dim, p.dirtyHere, p.full);
                waiters.addAll(p.waiters);
                it.remove();
            }
        }
        return waiters;
    }

    private void requeue(ResourceKey<Level> dim, LongOpenHashSet dirtyHere, boolean full) {
        dirty.computeIfAbsent(dim, k -> new LongOpenHashSet()).addAll(dirtyHere);
        if (full)
            fullDims.add(dim);
    }

    /** Forgets the detection under way for {@code dim}, and its queued passes, edits and all (test helpers that wipe the dimension). */
    private void dropRescans(ResourceKey<Level> dim) {
        if (job != null && job.dim == dim)
            job = null;
        passes.removeIf(p -> p.dim == dim);
    }

    /**
     * One detection in a dimension, done over several ticks. {@code full}: every district of the dimension
     * (commands, the periodic pass); otherwise only the districts an edit can change: those owning a dirty
     * chunk or a chunk next to one, grown until nothing the walk reaches belongs to a district outside the set.
     *
     * <p>Looking at the block entities of the chunks is the cost, so it is spread: {@link #step} looks at chunks
     * until the tick's block entity budget is spent. Nothing a reader can see changes meanwhile: the district
     * set, the index and the engine's records change in {@link #commit}, in one tick, from what the walk
     * recorded. A chunk is looked at whole and what it holds is recorded then (its seeds, inventories, stations,
     * links), so a chunk that unloads after it was looked at still counts as it was, and one that unloads before
     * it is looked at is an unloaded member (it keeps what its district knew). Edits that arrive meanwhile are in
     * {@link #dirty}, not in this detection's snapshot, and make the next one; the exception is an adopt, whose
     * chunks join the detection under way ({@link #join}, DS-14).
     */
    private final class Rescan {
        final ServerLevel level;
        final ResourceKey<Level> dim;
        final boolean full;
        /** The dirty chunks this detection took; kept so an abort can hand them back. */
        final LongOpenHashSet dirtyHere;
        final Long2ObjectOpenHashMap<District> index;
        final LongOpenHashSet excluded;
        final Long2LongOpenHashMap pins;
        final Set<Long> affected = new HashSet<>();
        final Scan scan = new Scan();
        final LongOpenHashSet visited = new LongOpenHashSet();
        final LongArrayFIFOQueue queue = new LongArrayFIFOQueue();
        /** Chunks asked about as roots (once each), in order, and how far the asking has got. */
        final LongOpenHashSet scheduled = new LongOpenHashSet();
        final LongArrayList pending = new LongArrayList();
        int cursor;
        /** Candidate chunks that were loaded when they were looked at. */
        final LongOpenHashSet loadedAtTest = new LongOpenHashSet();
        long nanos;
        int slices;
        long maxSlice;
        /** Time in {@link #root}, in {@link #walk}, and in {@link #commit}. */
        long rootNanos, walkNanos, commitNanosAll;
        /** Block entities looked at in all, and in the biggest step. */
        long looked, maxLooked, maxItem;
        /** The costliest look at one chunk in wall time, and the chunk and class loads during the steps (R2-11). */
        long maxItemNanos, chunkLoads, classLoads;
        /** Run in one call by a command or a test: its one big tick is not a scheduling figure. */
        boolean manual;
        /** What to run when this detection is done and has committed ({@link #rescanSoon}). */
        final List<Runnable> waiters = new ArrayList<>();

        Rescan(ServerLevel level, boolean full, LongOpenHashSet dirtyHere) {
            this.level = level;
            this.dim = level.dimension();
            this.full = full;
            this.dirtyHere = dirtyHere;
            this.index = byChunk.computeIfAbsent(dim, k -> new Long2ObjectOpenHashMap<>());
            this.excluded = data.excluded(dim);
            this.pins = data.pins(dim);
            if (full) {
                for (District d : data.districts.values())
                    if (d.dim == dim)
                        affected.add(d.id);
                affected.addAll(pins.values());
            } else {
                for (long c : dirtyHere)
                    for (int dx = -1; dx <= 1; dx++)
                        for (int dz = -1; dz <= 1; dz++) {
                            District o = index.get(ChunkPos.asLong(ChunkPos.getX(c) + dx, ChunkPos.getZ(c) + dz));
                            if (o != null)
                                affected.add(o.id);
                        }
            }
            for (long c : dirtyHere)
                schedule(c);
            scheduleDistricts(affected);
        }

        private void schedule(long c) {
            if (scheduled.add(c))
                pending.add(c);
        }

        /**
         * Chunks that gained seeds while this detection runs (a deferred adopt, SG55). The detection takes them as it
         * would have at its start: a chunk it has not counted yet is scheduled as a root (again, if the walk went by
         * it while it was unloaded and recorded nothing), with the districts owning or touching it (all of the
         * dimension's already, in a full detection); a chunk it counted before has its seeds counted again in place,
         * and is enqueued as a root if it has some now and the walk had not reached it (SG56). Nothing but the
         * seed count of a chunk depends on its seeds, so the rest of what the detection recorded stands. The chunks
         * go back to the dirty set with the job if the job is put back.
         */
        void join(LongOpenHashSet chunks) {
            Set<Long> near = new HashSet<>();
            for (long c : chunks) {
                dirtyHere.add(c);
                if (!full)
                    for (int dx = -1; dx <= 1; dx++)
                        for (int dz = -1; dz <= 1; dz++) {
                            District o = index.get(ChunkPos.asLong(ChunkPos.getX(c) + dx, ChunkPos.getZ(c) + dz));
                            if (o != null && affected.add(o.id))
                                near.add(o.id);
                        }
                if (!scan.seedCounts.containsKey(c)) {
                    // Not counted: scheduled behind the cursor too, because a root that found the chunk unloaded counted nothing.
                    scheduled.add(c);
                    pending.add(c);
                    continue;
                }
                LevelChunk chunk = level.getChunkSource().getChunkNow(ChunkPos.getX(c), ChunkPos.getZ(c));
                if (chunk == null)
                    continue; // unloaded since: its district keeps the seeds it recorded, and the adopt's seeds wait for the chunk
                scan.seedCounts.remove(c);
                if (seedsIn(level, chunk, scan) > 0 && visited.add(c)) {
                    loadedAtTest.add(c);
                    queue.enqueue(c);
                }
            }
            if (!near.isEmpty())
                scheduleDistricts(near);
        }

        /** The chunks of these districts, and the chunks pinned to them, are candidates. */
        private void scheduleDistricts(Set<Long> ids) {
            for (long id : ids) {
                District d = data.districts.get(id);
                if (d != null)
                    for (long c : d.chunks)
                        schedule(c);
            }
            for (Long2LongOpenHashMap.Entry e : pins.long2LongEntrySet())
                if (ids.contains(e.getLongValue()))
                    schedule(e.getLongKey());
        }

        /**
         * Looks at chunks until about {@code budget} block entities are spent (at least one chunk, whatever the
         * budget), or commits when everything is looked at. True when the detection is done.
         */
        boolean step(int budget) {
            long t0 = System.nanoTime();
            long loads0 = IndustryEvents.chunkLoads(), classes0 = CLASSES.getTotalLoadedClassCount();
            int micros = IndustryConfig.get(IndustryConfig.RESCAN_SLICE_MICROS);
            long limit = micros <= 0 ? Long.MAX_VALUE : micros * 1000L;
            long spent = 0, tickItemNanos = 0;
            boolean progressed = false, done = false;
            long commitNanos = 0;
            IndustryEvents.loadInDetection = null;
            IndustryEvents.watchLoads = true;
            try {
                while (true) {
                    // At least one chunk a tick; then the block entity budget or the time limit, whichever comes first (R2-11).
                    if (progressed && (spent >= budget || System.nanoTime() - t0 >= limit))
                        break;
                    if (cursor < pending.size()) {
                        long r0 = System.nanoTime();
                        int cost = root(pending.getLong(cursor++));
                        spent += cost;
                        maxItem = Math.max(maxItem, cost);
                        long dt = System.nanoTime() - r0;
                        rootNanos += dt;
                        tickItemNanos = Math.max(tickItemNanos, dt);
                        maxItemNanos = Math.max(maxItemNanos, dt);
                    } else if (!queue.isEmpty()) {
                        long w0 = System.nanoTime();
                        int cost = walk(queue.dequeueLong());
                        spent += cost;
                        maxItem = Math.max(maxItem, cost);
                        long dt = System.nanoTime() - w0;
                        walkNanos += dt;
                        tickItemNanos = Math.max(tickItemNanos, dt);
                        maxItemNanos = Math.max(maxItemNanos, dt);
                    } else {
                        Set<Long> more = full ? Set.of() : grown();
                        if (!more.isEmpty()) {
                            affected.addAll(more);
                            scheduleDistricts(more);
                            continue;
                        }
                        long c0 = System.nanoTime();
                        commit();
                        commitNanos = System.nanoTime() - c0;
                        commitNanosAll += commitNanos;
                        done = true;
                        break;
                    }
                    progressed = true;
                }
            } finally {
                IndustryEvents.watchLoads = false;
            }
            Throwable load = IndustryEvents.loadInDetection;
            if (load != null && !loadStackLogged) {
                loadStackLogged = true;
                MicRtsim.LOGGER.warn("[industry] a chunk loaded inside a detection step: a read waited for a chunk still loading,"
                        + " which detection never should (R2-11); the stack of the first, once a session:", load);
            }
            long dt = System.nanoTime() - t0;
            nanos += dt;
            slices++;
            maxSlice = Math.max(maxSlice, dt);
            looked += spent;
            maxLooked = Math.max(maxLooked, spent);
            chunkLoads += IndustryEvents.chunkLoads() - loads0;
            classLoads += CLASSES.getTotalLoadedClassCount() - classes0;
            if (done) {
                rescans++;
                rescanNanos += nanos;
                lastRescanNanos = nanos;
                lastRescanSlices = slices;
                lastRescanLongestNanos = maxSlice;
                lastRescanLooked = looked;
                lastRescanLongestLooked = maxLooked;
                lastRescanLongestItem = maxItem;
                lastRescanRootNanos = rootNanos;
                lastRescanWalkNanos = walkNanos;
                lastRescanCommitNanos = commitNanosAll;
                lastRescanLongestItemNanos = maxItemNanos;
                lastRescanChunkLoads = chunkLoads;
                lastRescanClassLoads = classLoads;
                lastRescanLoadedBy = scan.loadedBy == null ? "" : scan.loadedBy;
            }
            if (!manual && dt > maxSliceNanos) {
                maxSliceNanos = dt;
                maxSliceWhat = (done ? spent + " block entities, then the commit " + commitNanos / 1000 + " us" : spent + " block entities")
                        + " (the longest chunk " + tickItemNanos / 1000 + " us), " + (CLASSES.getTotalLoadedClassCount() - classes0) + " classes and "
                        + (IndustryEvents.chunkLoads() - loads0) + " chunks loaded meanwhile";
            }
            return done;
        }

        /** Whether candidate chunk {@code c} starts the walk; enqueues it when it does. Returns the block entities spent. */
        private int root(long c) {
            if (excluded.contains(c))
                return 1;
            LevelChunk chunk = level.getChunkSource().getChunkNow(ChunkPos.getX(c), ChunkPos.getZ(c));
            if (chunk == null)
                return 1;
            loadedAtTest.add(c);
            int cost = 1;
            boolean root = seedsIn(level, chunk, scan) > 0 || pins.containsKey(c);
            if (!root) {
                cost += chunk.getBlockEntities().size();
                root = holdsStation(chunk, scan) || (index.containsKey(c) && touchesUnloadedMember(level, chunk, index, scan));
            }
            if (root && visited.add(c))
                queue.enqueue(c);
            return cost;
        }

        /** Looks at one reached chunk. Returns the block entities spent. */
        private int walk(long c) {
            LevelChunk chunk = level.getChunkSource().getChunkNow(ChunkPos.getX(c), ChunkPos.getZ(c));
            if (chunk == null) {
                // Unloaded since it was reached: an unloaded member, as if the detection had come a moment earlier.
                visited.remove(c);
                loadedAtTest.remove(c);
                return 1;
            }
            seedsIn(level, chunk, scan);
            return 1 + scanChunk(level, chunk, scan, excluded, (n) -> {
                if (!excluded.contains(n) && visited.add(n))
                    queue.enqueue(n);
            });
        }

        /** Grow: a district next to (or pinned at) a chunk the walk reached may merge with it. */
        private Set<Long> grown() {
            Set<Long> more = new HashSet<>();
            for (long c : visited) {
                for (int dx = -1; dx <= 1; dx++)
                    for (int dz = -1; dz <= 1; dz++) {
                        District o = index.get(ChunkPos.asLong(ChunkPos.getX(c) + dx, ChunkPos.getZ(c) + dz));
                        if (o != null && !affected.contains(o.id))
                            more.add(o.id);
                    }
                if (pins.containsKey(c) && !affected.contains(pins.get(c)))
                    more.add(pins.get(c));
            }
            return more;
        }

        /** The district set, the index and the records, from what the walk recorded: one tick, no block entity visited. */
        private void commit() {
            long now = ind.gameTime();
            // Members: the reached loaded chunks, the chunks of the districts in question that were not loaded
            // when they were looked at, their pinned chunks; never an excluded one.
            LongOpenHashSet members = new LongOpenHashSet(visited);
            for (long id : affected) {
                District d = data.districts.get(id);
                if (d != null)
                    for (long c : d.chunks)
                        if (!loadedAtTest.contains(c))
                            members.add(c);
            }
            for (Long2LongOpenHashMap.Entry e : pins.long2LongEntrySet())
                if (affected.contains(e.getLongValue()))
                    members.add(e.getLongKey());
            members.removeAll(excluded);

            // Components: unpinned members by 8-neighbour adjacency, plugin links and merge links; pinned
            // chunks by their pin.
            Long2LongOpenHashMap parent = new Long2LongOpenHashMap();
            parent.defaultReturnValue(Long.MIN_VALUE);
            for (long c : members)
                parent.put(c, c);
            for (long c : members) {
                if (pins.containsKey(c))
                    continue;
                int x = ChunkPos.getX(c), z = ChunkPos.getZ(c);
                for (int dx = -1; dx <= 1; dx++)
                    for (int dz = -1; dz <= 1; dz++) {
                        long n = ChunkPos.asLong(x + dx, z + dz);
                        if ((dx != 0 || dz != 0) && members.contains(n) && !pins.containsKey(n))
                            union(parent, c, n);
                    }
            }
            for (long[] l : scan.links)
                if (members.contains(l[0]) && members.contains(l[1]) && !pins.containsKey(l[0]) && !pins.containsKey(l[1]))
                    union(parent, l[0], l[1]);
            for (long[] l : data.mergeLinks(dim))
                if (members.contains(l[0]) && members.contains(l[1]) && !pins.containsKey(l[0]) && !pins.containsKey(l[1]))
                    union(parent, l[0], l[1]);
            Map<Long, LongArrayList> groups = new HashMap<>();
            Map<Long, LongArrayList> pinned = new HashMap<>();
            for (long c : members) {
                if (pins.containsKey(c))
                    pinned.computeIfAbsent(pins.get(c), k -> new LongArrayList()).add(c);
                else
                    groups.computeIfAbsent(find(parent, c), k -> new LongArrayList()).add(c);
            }

            // Seeds (DS-10): a district holds at least one machine a player placed. A loaded chunk counts what it
            // held when it was looked at, an unloaded one what its district recorded; a group with none is not a
            // district (its old district, if any, has no successor and is removed below). A pinned group is the operator's.
            Long2IntOpenHashMap counts = new Long2IntOpenHashMap();
            for (long c : members) {
                if (visited.contains(c))
                    counts.put(c, scan.seedCounts.get(c));
                else {
                    District old = index.get(c);
                    if (old != null)
                        counts.put(c, old.seedCounts.get(c));
                }
            }
            groups.values().removeIf(g -> {
                for (long c : g)
                    if (counts.get(c) > 0)
                        return false;
                return true;
            });

            // Ids: a pinned group keeps its pin; an automatic one the old district it overlaps most (each
            // old id once, the biggest group first); the rest get new ids (never reused, F3).
            Map<Long, LongArrayList> assigned = new HashMap<>(pinned);
            List<LongArrayList> autos = new ArrayList<>(groups.values());
            autos.sort(Comparator.comparingInt((LongArrayList g) -> -g.size()).thenComparingLong(g -> minChunk(g)));
            Set<Long> taken = new HashSet<>(pinned.keySet());
            Set<Long> pinnedIds = new HashSet<>(pins.values());
            for (LongArrayList g : autos) {
                Map<Long, Integer> overlap = new HashMap<>();
                for (long c : g) {
                    District old = index.get(c);
                    if (old != null && !taken.contains(old.id) && !pinnedIds.contains(old.id))
                        overlap.merge(old.id, 1, Integer::sum);
                }
                long id = -1;
                int best = 0;
                for (Map.Entry<Long, Integer> e : overlap.entrySet())
                    if (e.getValue() > best || (e.getValue() == best && e.getKey() < id)) {
                        best = e.getValue();
                        id = e.getKey();
                    }
                if (id < 0)
                    id = data.allocateId();
                taken.add(id);
                assigned.put(id, g);
            }

            // Build the new districts: chunks, inventories, stations, name, crossings.
            Long2LongOpenHashMap owner = new Long2LongOpenHashMap();
            owner.defaultReturnValue(-1);
            for (Map.Entry<Long, LongArrayList> e : assigned.entrySet())
                for (long c : e.getValue())
                    owner.put(c, (long) e.getKey());
            Map<Long, Integer> crossing = new HashMap<>();
            for (Touch t : scan.touches) {
                long oa = owner.get(t.a), ob = owner.get(t.b);
                if (oa == ob)
                    continue;
                int code = crossingCode(level, t.pa, t.pb);
                if (code == 0)
                    continue;
                if (oa >= 0)
                    crossing.merge(oa, code, Math::min);
                if (ob >= 0)
                    crossing.merge(ob, code, Math::min);
            }

            Set<Long> survivors = new HashSet<>();
            for (Map.Entry<Long, LongArrayList> e : assigned.entrySet()) {
                long id = e.getKey();
                survivors.add(id);
                District d = data.districts.get(id);
                boolean fresh = d == null;
                if (fresh) {
                    d = new District(id, dim);
                    data.districts.put(id, d);
                }
                LongOpenHashSet chunks = new LongOpenHashSet(e.getValue());
                List<District.Inventory> invs = new ArrayList<>();
                List<District.Station> stations = new ArrayList<>();
                Set<BlockPos> seen = new HashSet<>();
                boolean creative = false;
                List<BlockPos> watch = new ArrayList<>();
                for (long c : chunks) {
                    if (visited.contains(c)) {
                        for (District.Inventory inv : scan.inventories.getOrDefault(c, List.of()))
                            addInventory(invs, seen, inv.pos(), inv.kind(), inv);
                        for (IndustryPlugin.StationInfo s : scan.stations.getOrDefault(c, List.of()))
                            stations.add(new District.Station(s.id(), s.pos(), s.name()));
                        creative |= scan.creative.contains(c);
                        watch.addAll(scan.watch.getOrDefault(c, List.of()));
                    } else {
                        // Unloaded: keep what the old districts knew of this chunk.
                        District old = index.get(c);
                        if (old != null) {
                            for (District.Inventory inv : old.inventories)
                                if (inv.chunk() == c && seen.add(inv.pos()))
                                    invs.add(inv);
                            for (District.Station s : old.stations)
                                if (ChunkPos.asLong(s.pos().getX() >> 4, s.pos().getZ() >> 4) == c)
                                    stations.add(s);
                            for (BlockPos w : old.watch)
                                if (ChunkPos.asLong(w.getX() >> 4, w.getZ() >> 4) == c)
                                    watch.add(w);
                            creative |= old.creative;
                        }
                    }
                }
                invs.sort(Comparator.comparingLong(i -> i.pos().asLong()));
                for (int i = 0; i < invs.size(); i++)
                    invs.set(i, invs.get(i).withIndex(i));
                stations.sort(Comparator.comparingLong(s -> s.pos().asLong()));
                String stationName = stations.isEmpty() ? null : stations.get(0).name();
                int notLearnable = crossing.getOrDefault(id, 0);
                boolean changed = fresh || !d.chunks.equals(chunks) || !d.inventories.equals(invs) || !d.stations.equals(stations)
                        || !java.util.Objects.equals(d.stationName, stationName) || d.notLearnable != notLearnable;
                d.chunks.clear();
                d.chunks.addAll(chunks);
                d.seedCounts.clear();
                for (long c : chunks)
                    if (counts.get(c) > 0)
                        d.seedCounts.put(c, counts.get(c));
                d.inventories = invs;
                d.stations = stations;
                d.stationName = stationName;
                d.notLearnable = notLearnable;
                d.creative = creative;
                d.watch = watch;
                d.recomputeBounds();
                if (changed) {
                    d.revision++;
                    declare(d, now);
                    d.loadDirty = true;
                    LongOpenHashSet ls = new LongOpenHashSet();
                    for (long c : chunks)
                        if (level.getChunkSource().getChunkNow(ChunkPos.getX(c), ChunkPos.getZ(c)) != null)
                            ls.add(c);
                    LongOpenHashSet before = loaded.put(id, ls);
                    reportLoaded(level, d, ls, before, now);
                    ind.sampler.revisionChanged(d);
                }
            }
            // Removed: districts in question with no successor.
            for (District old : new ArrayList<>(data.districts.values())) {
                if (old.dim != dim || survivors.contains(old.id) || !affected.contains(old.id))
                    continue;
                data.districts.remove(old.id);
                loaded.remove(old.id);
                ByteBuffer r = ind.journal(IndRecords.IND_DISTRICT_REMOVED, IndRecords.DistrictRemoved.SIZE, 0);
                r.putLong(IndRecords.DistrictRemoved.DISTRICT, old.id);
                ind.commit(r);
                ind.sampler.revisionChanged(old);
            }
            index.clear();
            for (District d : data.districts.values())
                if (d.dim == dim)
                    for (long c : d.chunks)
                        index.put(c, d);
            data.setDirty();
        }
    }

    /**
     * Adds an inventory to a district's list once: a second sighting of the same position (a packager's target
     * that is also tagged storage, a postbox) only adds its kind bits to the one already there.
     */
    private static void addInventory(List<District.Inventory> invs, Set<BlockPos> seen, BlockPos pos, int kind, District.Inventory inv) {
        if (seen.add(pos)) {
            if (inv != null)
                invs.add(inv);
            return;
        }
        if (kind == 0)
            return;
        for (int i = 0; i < invs.size(); i++) {
            District.Inventory old = invs.get(i);
            if (old.pos().equals(pos)) {
                if ((old.kind() | kind) != old.kind())
                    invs.set(i, new District.Inventory(0, pos, old.kind() | kind, old.itemSlots(), old.fluidCapacity(), old.energyCapacity()));
                return;
            }
        }
    }

    private static long minChunk(LongArrayList g) {
        long m = Long.MAX_VALUE;
        for (long c : g)
            m = Math.min(m, c);
        return m;
    }

    private static void union(Long2LongOpenHashMap parent, long a, long b) {
        long ra = find(parent, a), rb = find(parent, b);
        if (ra != rb)
            parent.put(Math.max(ra, rb), Math.min(ra, rb));
    }

    private static long find(Long2LongOpenHashMap parent, long c) {
        long r = c;
        while (parent.get(r) != r)
            r = parent.get(r);
        while (parent.get(c) != r) {
            long next = parent.get(c);
            parent.put(c, r);
            c = next;
        }
        return r;
    }

    /**
     * The live seeds of a loaded chunk, counted once per detection: the positions its attachment lists whose
     * block entity is still there and still a machine (a seed broken since, or one an older rule took for a
     * machine, is dropped here, so a world saved with chests as seeds sheds them at its next detection).
     */
    private int seedsIn(ServerLevel level, long c, Scan scan) {
        if (scan.seedCounts.containsKey(c))
            return scan.seedCounts.get(c);
        LevelChunk chunk = level.getChunkSource().getChunkNow(ChunkPos.getX(c), ChunkPos.getZ(c));
        int n = chunk == null ? 0 : seedsIn(level, chunk, scan);
        scan.seedCounts.put(c, n);
        return n;
    }

    private int seedsIn(ServerLevel level, LevelChunk chunk, Scan scan) {
        long c = chunk.getPos().toLong();
        if (scan.seedCounts.containsKey(c))
            return scan.seedCounts.get(c);
        int n = 0;
        IndustryAttachments.Seeds s = chunk.getExistingDataOrNull(IndustryAttachments.SEEDS.get());
        if (s != null && !s.isEmpty()) {
            IndustryAttachments.Seeds kept = s;
            for (long p : s.positions()) {
                BlockEntity be = chunk.getBlockEntities().get(BlockPos.of(p));
                if (be == null || be.isRemoved()) {
                    kept = kept.minus(p);
                    continue;
                }
                long loads = IndustryEvents.chunkLoads();
                if (seed(level, be, scan.kinds)) {
                    n++;
                } else if (!mayBecomeSeed(be)) {
                    kept = kept.minus(p);
                }
                scan.loads(loads, be);
            }
            if (kept != s) {
                if (kept.isEmpty())
                    chunk.removeData(IndustryAttachments.SEEDS.get());
                else
                    chunk.setData(IndustryAttachments.SEEDS.get(), kept);
                chunk.setUnsaved(true);
            }
        }
        scan.seedCounts.put(c, n);
        return n;
    }

    /**
     * Whether a loaded chunk holds a station (DS-10). A station is an anchor, not a seed: its chunk is
     * rooted in the walk, so it joins the district beside it (adjacent, or linked by what touches it), but
     * it never counts toward a district's survival, so a station far from any machine makes no district.
     */
    private boolean holdsStation(LevelChunk chunk, Scan scan) {
        long c = chunk.getPos().toLong();
        if (!scan.stationLooked.add(c))
            return scan.stationChunks.contains(c);
        for (BlockEntity be : chunk.getBlockEntities().values()) {
            if (be.isRemoved())
                continue;
            long loads = IndustryEvents.chunkLoads();
            for (IndustryPlugin plugin : IndustryApi.PLUGINS)
                if (plugin.isStation(be)) {
                    scan.stationChunks.add(c);
                    scan.loads(loads, be);
                    return true;
                }
            scan.loads(loads, be);
        }
        return false;
    }

    /**
     * An old member chunk with no seeds is kept when one of its industrial block entities sits on a
     * border facing an unloaded member chunk: what it touches there cannot be seen, and dropping it
     * would flap the district's membership with every load.
     */
    private boolean touchesUnloadedMember(ServerLevel level, LevelChunk chunk, Long2ObjectOpenHashMap<District> index, Scan scan) {
        ChunkPos cp = chunk.getPos();
        for (BlockEntity be : chunk.getBlockEntities().values()) {
            BlockPos p = be.getBlockPos();
            int lx = p.getX() & 15, lz = p.getZ() & 15;
            if (lx != 0 && lx != 15 && lz != 0 && lz != 15)
                continue;
            long loads = IndustryEvents.chunkLoads();
            boolean isIndustrial = industrial(level, be, scan.kinds);
            scan.loads(loads, be);
            if (!isIndustrial)
                continue;
            for (Direction d : Direction.Plane.HORIZONTAL) {
                BlockPos n = p.relative(d);
                long nc = ChunkPos.asLong(n.getX() >> 4, n.getZ() >> 4);
                if (nc == cp.toLong())
                    continue;
                if (index.containsKey(nc) && level.getChunkSource().getChunkNow(ChunkPos.getX(nc), ChunkPos.getZ(nc)) == null)
                    return true;
            }
        }
        return false;
    }

    /** Looks at every block entity of a loaded chunk and records what it holds in {@code scan}. Returns the block entities looked at. */
    private int scanChunk(ServerLevel level, LevelChunk chunk, Scan scan, LongOpenHashSet excluded, java.util.function.LongConsumer reach) {
        long c = chunk.getPos().toLong();
        List<Candidate> candidates = new ArrayList<>();
        IndustryPlugin.ScanNotes notes = new IndustryPlugin.ScanNotes() {
            @Override
            public void creative() {
                scan.creative.add(c);
            }

            @Override
            public void watch(BlockPos pos) {
                scan.watch.computeIfAbsent(c, k -> new ArrayList<>()).add(pos);
            }
        };
        List<BlockEntity> blockEntities = new ArrayList<>(chunk.getBlockEntities().values());
        BlockEntity last = null;
        long loads = 0;
        for (BlockEntity be : blockEntities) {
            // Names the look that loaded a chunk (R2-11): checked as the next look begins, since a look has several exits.
            if (last != null)
                scan.loads(loads, last);
            last = be;
            loads = IndustryEvents.chunkLoads();
            if (be.isRemoved())
                continue;
            BlockPos p = be.getBlockPos();
            for (IndustryPlugin plugin : IndustryApi.PLUGINS)
                plugin.scanned(be, notes);
            boolean isIndustrial = industrial(level, be, scan.kinds);
            if (!isIndustrial)
                continue;
            BlockState state = be.getBlockState();
            // Boundary stock (I3): tagged storage, and what plugins add.
            if (state.is(STOCK_TAG)) {
                BlockPos canon = canonicalInventory(level, p);
                if (canon != null)
                    candidates.add(new Candidate(canon, 0));
            }
            for (IndustryPlugin plugin : IndustryApi.PLUGINS)
                plugin.inventories(be, (pos, kind) -> {
                    BlockPos canon = canonicalInventory(level, pos);
                    if (canon != null)
                        candidates.add(new Candidate(canon, kind));
                });
            for (IndustryPlugin plugin : IndustryApi.PLUGINS) {
                IndustryPlugin.StationInfo s = plugin.station(be);
                if (s != null)
                    scan.stations.computeIfAbsent(c, k -> new ArrayList<>()).add(s);
            }
            // Touches across the chunk border: closed under physical connection.
            int lx = p.getX() & 15, lz = p.getZ() & 15;
            if (lx == 0 || lx == 15 || lz == 0 || lz == 15) {
                for (Direction d : Direction.Plane.HORIZONTAL) {
                    BlockPos n = p.relative(d);
                    long nc = ChunkPos.asLong(n.getX() >> 4, n.getZ() >> 4);
                    if (nc == c)
                        continue;
                    LevelChunk nchunk = level.getChunkSource().getChunkNow(ChunkPos.getX(nc), ChunkPos.getZ(nc));
                    if (nchunk == null)
                        continue;
                    BlockEntity nbe = nchunk.getBlockEntities().get(n);
                    if (nbe == null || !industrial(level, nbe, scan.kinds))
                        continue;
                    // Two stores side by side carry no work from one chunk to the other (a chest wall along a
                    // border does not annex the next chunk); a machine, belt or hopper next to one does.
                    if (isStorage(state) && isStorage(nbe.getBlockState()))
                        continue;
                    scan.touches.add(new Touch(c, nc, p, n));
                    if (!excluded.contains(nc))
                        reach.accept(nc);
                }
            }
            for (IndustryPlugin plugin : IndustryApi.PLUGINS) {
                plugin.links(be, target -> {
                    long tc = ChunkPos.asLong(target.getX() >> 4, target.getZ() >> 4);
                    if (tc == c)
                        return;
                    LevelChunk tchunk = level.getChunkSource().getChunkNow(ChunkPos.getX(tc), ChunkPos.getZ(tc));
                    if (tchunk == null)
                        return;
                    BlockEntity tbe = tchunk.getBlockEntities().get(target);
                    if (tbe == null || !industrial(level, tbe, scan.kinds))
                        return;
                    scan.links.add(new long[] {c, tc});
                    scan.touches.add(new Touch(c, tc, p, target.immutable()));
                    if (!excluded.contains(tc))
                        reach.accept(tc);
                });
            }
        }
        if (last != null)
            scan.loads(loads, last);
        // Describe the chunk's boundary inventories now, while their chunks are loaded (the commit may come ticks later).
        if (!candidates.isEmpty()) {
            List<District.Inventory> out = new ArrayList<>();
            Set<BlockPos> seen = new HashSet<>();
            for (Candidate cand : candidates) {
                if (seen.contains(cand.pos)) {
                    addInventory(out, seen, cand.pos, cand.kind, null);
                    continue;
                }
                District.Inventory inv = describe(level, cand);
                seen.add(cand.pos);
                if (inv != null)
                    out.add(inv);
            }
            if (!out.isEmpty())
                scan.inventories.put(c, out);
        }
        return blockEntities.size() + candidates.size();
    }

    /** A block entity of a loaded chunk, or null: never loads a chunk. */
    private static BlockEntity loadedBlockEntity(ServerLevel level, BlockPos pos) {
        LevelChunk chunk = level.getChunkSource().getChunkNow(pos.getX() >> 4, pos.getZ() >> 4);
        return chunk == null ? null : chunk.getBlockEntities().get(pos);
    }

    private int crossingCode(ServerLevel level, BlockPos a, BlockPos b) {
        BlockEntity ba = loadedBlockEntity(level, a), bb = loadedBlockEntity(level, b);
        if (ba == null || bb == null)
            return 0;
        int best = 0;
        for (IndustryPlugin plugin : IndustryApi.PLUGINS) {
            int code = plugin.crossing(ba, bb);
            if (code != 0 && (best == 0 || code < best))
                best = code;
        }
        return best;
    }

    /**
     * Industrial: part of a district once there is one (it joins the chunks it touches, its inventory may
     * be boundary stock): a machine, a plugin recognises it, or it has an item, fluid or energy capability.
     * Industrial is wider than a seed: a chest, a furnace and a shaft are industrial and seed nothing.
     */
    boolean industrial(ServerLevel level, BlockEntity be, Long2ByteOpenHashMap cache) {
        return (classify(level, be, cache) & K_INDUSTRIAL) != 0;
    }

    /** A cache for {@link #classify}: block entities by {@code long} position, -1 for "not asked yet". */
    static Long2ByteOpenHashMap kindCache() {
        Long2ByteOpenHashMap m = new Long2ByteOpenHashMap();
        m.defaultReturnValue((byte) -1);
        return m;
    }

    private static final int K_INDUSTRIAL = 1, K_SEED = 2;

    private static int classify(ServerLevel level, BlockEntity be, Long2ByteOpenHashMap cache) {
        long key = be.getBlockPos().asLong();
        byte known = cache.get(key);
        if (known >= 0)
            return known;
        int kinds = classify(level, be);
        cache.put(key, (byte) kinds);
        return kinds;
    }

    /**
     * Industrial and seed together, so the capability probe (up to 15 queries, the dearest part of a detection)
     * runs at most once per block entity and not at all when a plugin or a tag has already said.
     */
    private static int classify(ServerLevel level, BlockEntity be) {
        boolean plugin = false;
        for (IndustryPlugin p : IndustryApi.PLUGINS) {
            if (p.isIndustrial(be)) {
                plugin = true;
                break;
            }
        }
        Boolean verdict = seedVerdict(be);
        int capability = -1;
        boolean seed;
        if (verdict != null) {
            seed = verdict;
        } else if (!ticksOnServer(level, be)) {
            seed = false;
        } else {
            capability = hasCapability(level, be) ? 1 : 0;
            seed = capability == 1;
        }
        boolean industrial = plugin || seed;
        if (!industrial) {
            if (capability < 0)
                capability = hasCapability(level, be) ? 1 : 0;
            industrial = capability == 1;
        }
        return (industrial ? K_INDUSTRIAL : 0) | (seed ? K_SEED : 0);
    }

    private static final Direction[] DIRECTIONS = Direction.values();

    private static boolean hasCapability(ServerLevel level, BlockEntity be) {
        BlockPos p = be.getBlockPos();
        BlockState s = be.getBlockState();
        if (level.getCapability(Capabilities.ItemHandler.BLOCK, p, s, be, null) != null
                || level.getCapability(Capabilities.FluidHandler.BLOCK, p, s, be, null) != null
                || level.getCapability(Capabilities.EnergyStorage.BLOCK, p, s, be, null) != null)
            return true;
        for (Direction d : DIRECTIONS)
            if (level.getCapability(Capabilities.ItemHandler.BLOCK, p, s, be, d) != null
                    || level.getCapability(Capabilities.FluidHandler.BLOCK, p, s, be, d) != null)
                return true;
        return false;
    }

    /** True when the block entity's block ticks it on the server (an {@code EntityBlock} with a server ticker). */
    private static boolean ticksOnServer(ServerLevel level, BlockEntity be) {
        BlockState s = be.getBlockState();
        return s.getBlock() instanceof EntityBlock eb && eb.getTicker(level, s, be.getType()) != null;
    }

    /** Storage by tag: its inventory is boundary stock, or a block that never starts a district. */
    public static boolean isStorage(BlockState s) {
        return s.is(STOCK_TAG) || s.is(NOT_SEED_TAG);
    }

    /**
     * A seed: a machine, the block entity that starts a district when a player places it (DS-10). In order:
     * a block of the {@code minecraft} namespace never is; nor is storage ({@code #mic_rtsim:district_stock},
     * {@code #mic_rtsim:district_not_seed}); a block in {@code #mic_rtsim:district_seed} is; else the first
     * plugin with an opinion ({@link IndustryPlugin#seed}) decides; else a block entity that ticks on the
     * server and has an item, fluid or energy capability is.
     */
    private static boolean mayBecomeSeed(BlockEntity be) {
        for (IndustryPlugin plugin : IndustryApi.PLUGINS)
            if (plugin.mayBecomeSeed(be))
                return true;
        return false;
    }

    /** Whether {@code be} is a seed (not cached). */
    public boolean seed(ServerLevel level, BlockEntity be) {
        return (classify(level, be) & K_SEED) != 0;
    }

    boolean seed(ServerLevel level, BlockEntity be, Long2ByteOpenHashMap cache) {
        return (classify(level, be, cache) & K_SEED) != 0;
    }

    /**
     * What the rules say of a seed before the generic capability test: false for the {@code minecraft} namespace,
     * storage and furnaces, true for the seed tag, else the first plugin with an opinion. Null: nobody has one.
     */
    private static Boolean seedVerdict(BlockEntity be) {
        BlockState s = be.getBlockState();
        if ("minecraft".equals(BuiltInRegistries.BLOCK.getKey(s.getBlock()).getNamespace()))
            return Boolean.FALSE;
        if (isStorage(s))
            return Boolean.FALSE;
        // A furnace, smoker or blast furnace of any mod: a utility block every build has, never a factory (DS-10).
        if (be instanceof net.minecraft.world.level.block.entity.AbstractFurnaceBlockEntity)
            return Boolean.FALSE;
        if (s.is(SEED_TAG))
            return Boolean.TRUE;
        for (IndustryPlugin plugin : IndustryApi.PLUGINS) {
            Boolean v = plugin.seed(be);
            if (v != null)
                return v;
        }
        return null;
    }

    /**
     * The position whose capability is the whole inventory at {@code pos}: a plugin's controller (a
     * vault's), a double chest's lower half, else {@code pos}. Null when {@code pos}, or the position it
     * leads to, is in a chunk that is not loaded: nothing here ever loads a chunk. One function for every
     * caller (tagged stock, packager targets, ports), so an inventory is counted once.
     */
    public static BlockPos canonicalInventory(ServerLevel level, BlockPos pos) {
        LevelChunk chunk = level.getChunkSource().getChunkNow(pos.getX() >> 4, pos.getZ() >> 4);
        if (chunk == null)
            return null;
        BlockEntity be = chunk.getBlockEntities().get(pos);
        if (be == null)
            return pos.immutable();
        BlockPos result = null;
        for (IndustryPlugin plugin : IndustryApi.PLUGINS) {
            BlockPos c = plugin.canonicalInventory(be);
            if (c != null) {
                result = c.immutable();
                break;
            }
        }
        if (result == null) {
            BlockState s = be.getBlockState();
            if (s.getBlock() instanceof ChestBlock && s.hasProperty(ChestBlock.TYPE) && s.getValue(ChestBlock.TYPE) != ChestType.SINGLE) {
                BlockPos other = pos.relative(ChestBlock.getConnectedDirection(s));
                result = other.asLong() < pos.asLong() ? other.immutable() : pos.immutable();
            } else {
                result = pos.immutable();
            }
        }
        if (level.getChunkSource().getChunkNow(result.getX() >> 4, result.getZ() >> 4) == null)
            return null;
        return result;
    }

    private District.Inventory describe(ServerLevel level, Candidate c) {
        if (level.getChunkSource().getChunkNow(c.pos.getX() >> 4, c.pos.getZ() >> 4) == null)
            return null; // never load a chunk to describe an inventory
        int kind = c.kind;
        int slots = 0;
        long fluid = 0, energy = 0;
        if ((kind & IndRecords.Inventory.KIND_PLUGIN) != 0) {
            // A plugin's own store (a PowerGrid battery): its shape comes from the plugin, not capabilities.
            for (IndustryPlugin plugin : IndustryApi.PLUGINS) {
                long[] shape = plugin.describe(level, c.pos);
                if (shape != null && shape.length == 4)
                    return new District.Inventory(0, c.pos, kind | (int) shape[0], (int) shape[1], shape[2], shape[3]);
            }
        }
        IItemHandler items = StockAccess.items(level, c.pos);
        if (items != null) {
            kind |= IndRecords.Inventory.KIND_ITEMS;
            slots = items.getSlots();
        }
        IFluidHandler fluids = StockAccess.fluids(level, c.pos);
        if (fluids != null) {
            kind |= IndRecords.Inventory.KIND_FLUIDS;
            for (int t = 0; t < fluids.getTanks(); t++)
                fluid += fluids.getTankCapacity(t);
        }
        IEnergyStorage e = StockAccess.energy(level, c.pos);
        if (e != null) {
            kind |= IndRecords.Inventory.KIND_ENERGY;
            energy = e.getMaxEnergyStored();
        }
        return new District.Inventory(0, c.pos, kind, slots, fluid, energy);
    }

    // ---- test helpers (GameTests) ---------------------------------------------------------------

    /**
     * GameTests: forgets every district of {@code level} with a chunk in the box (removed, as a player's
     * removal would be), and the seeds, epochs, pins and exclusions of the box's loaded chunks, so a test
     * starts from no industry wherever the runner put its structure.
     */
    public void forgetArea(ServerLevel level, int minCx, int minCz, int maxCx, int maxCz) {
        ResourceKey<Level> dim = level.dimension();
        requeueRescans(dim); // a detection under way would commit districts this wipes
        for (District d : new ArrayList<>(data.districts.values())) {
            if (d.dim != dim)
                continue;
            boolean hit = false;
            for (long c : d.chunks) {
                int x = ChunkPos.getX(c), z = ChunkPos.getZ(c);
                if (x >= minCx && x <= maxCx && z >= minCz && z <= maxCz) {
                    hit = true;
                    break;
                }
            }
            if (!hit)
                continue;
            data.districts.remove(d.id);
            loaded.remove(d.id);
            ByteBuffer r = ind.journal(IndRecords.IND_DISTRICT_REMOVED, IndRecords.DistrictRemoved.SIZE, 0);
            r.putLong(IndRecords.DistrictRemoved.DISTRICT, d.id);
            ind.commit(r);
            ind.sampler.revisionChanged(d);
        }
        for (int x = minCx; x <= maxCx; x++)
            for (int z = minCz; z <= maxCz; z++) {
                long c = ChunkPos.asLong(x, z);
                data.pins(dim).remove(c);
                data.excluded(dim).remove(c);
                LevelChunk chunk = level.getChunkSource().getChunkNow(x, z);
                if (chunk != null) {
                    chunk.removeData(IndustryAttachments.SEEDS.get());
                    chunk.removeData(IndustryAttachments.RECONCILE.get());
                    chunk.setUnsaved(true);
                }
            }
        Long2ObjectOpenHashMap<District> index = byChunk.computeIfAbsent(dim, k -> new Long2ObjectOpenHashMap<>());
        index.clear();
        for (District d : data.districts.values())
            if (d.dim == dim)
                for (long c : d.chunks)
                    index.put(c, d);
        LongOpenHashSet pendingHere = dirty.get(dim);
        if (pendingHere != null)
            pendingHere.removeIf(c -> ChunkPos.getX(c) >= minCx && ChunkPos.getX(c) <= maxCx && ChunkPos.getZ(c) >= minCz && ChunkPos.getZ(c) <= maxCz);
        data.setDirty();
    }

    /**
     * GameTests: forgets every district of the level's dimension, the seeds and epochs of their loaded
     * chunks, and the dimension's pins, exclusions and merge links. The industry tests run one at a
     * time (one batch each), so each starts from no industry at all.
     */
    public void forgetAllForTest(ServerLevel level) {
        ResourceKey<Level> dim = level.dimension();
        dropRescans(dim);
        for (District d : new ArrayList<>(data.districts.values())) {
            if (d.dim != dim)
                continue;
            for (long c : d.chunks) {
                LevelChunk chunk = level.getChunkSource().getChunkNow(ChunkPos.getX(c), ChunkPos.getZ(c));
                if (chunk != null) {
                    chunk.removeData(IndustryAttachments.SEEDS.get());
                    chunk.removeData(IndustryAttachments.RECONCILE.get());
                    chunk.setUnsaved(true);
                }
            }
            data.districts.remove(d.id);
            loaded.remove(d.id);
            ByteBuffer r = ind.journal(IndRecords.IND_DISTRICT_REMOVED, IndRecords.DistrictRemoved.SIZE, 0);
            r.putLong(IndRecords.DistrictRemoved.DISTRICT, d.id);
            ind.commit(r);
            ind.sampler.revisionChanged(d);
        }
        data.pins(dim).clear();
        data.excluded(dim).clear();
        data.mergeLinks(dim).clear();
        byChunk.computeIfAbsent(dim, k -> new Long2ObjectOpenHashMap<>()).clear();
        dirty.remove(dim);
        data.setDirty();
    }

    /** GameTests: a district of one chunk, made directly (no seed, no scan), declared. */
    public District createForTest(ServerLevel level, long chunk) {
        requeueRescans(level.dimension());
        District d = new District(data.allocateId(), level.dimension());
        d.chunks.add(chunk);
        d.recomputeBounds();
        data.districts.put(d.id, d);
        byChunk.computeIfAbsent(level.dimension(), k -> new Long2ObjectOpenHashMap<>()).put(chunk, d);
        LongOpenHashSet ls = new LongOpenHashSet();
        if (level.getChunkSource().getChunkNow(ChunkPos.getX(chunk), ChunkPos.getZ(chunk)) != null)
            ls.add(chunk);
        loaded.put(d.id, ls);
        d.revision++;
        declare(d, ind.gameTime());
        reportLoaded(level, d, ls, null, ind.gameTime());
        d.loadDirty = true;
        return d;
    }

    /** As {@link #addChunkForTest}, for a far member that holds a machine (it records one seed, so detection keeps it). */
    public void addSeededChunkForTest(District d, long chunk) {
        d.seedCounts.put(chunk, 1);
        addChunkForTest(d, chunk);
    }

    /**
     * The engine learns that a district chunk is loaded from IND_CHUNK_LOADED alone (I7, I8), sent at its
     * ChunkEvent.Load and at a session start. A chunk that was already loaded when it joined the district
     * (a new district, or one that grew) had its load before the district existed: it reports now, after
     * the declaration, as at a session start. Without it the engine counts the chunk unloaded until it next
     * loads: a partly loaded district replays its running chunks too, and their reconciles wait for a load
     * that never comes. Chunks the district already had ({@code before}) reported at their own load.
     */
    private void reportLoaded(ServerLevel level, District d, LongOpenHashSet chunks, LongOpenHashSet before, long time) {
        for (long c : chunks)
            if (before == null || !before.contains(c))
                ind.reconciler.chunkLoaded(level, level.getChunkSource().getChunkNow(ChunkPos.getX(c), ChunkPos.getZ(c)), d, time);
    }

    /** GameTests: adds a chunk to a district as if detection had kept it (an unloaded member chunk). */
    public void addChunkForTest(District d, long chunk) {
        requeueRescans(d.dim);
        d.chunks.add(chunk);
        d.recomputeBounds();
        byChunk.computeIfAbsent(d.dim, k -> new Long2ObjectOpenHashMap<>()).put(chunk, d);
        redeclare(d);
        d.loadDirty = true;
    }

    // ---- declaring ------------------------------------------------------------------------------

    /** IND_DISTRICT, its chunk parts, its inventories and its stations, all with the current revision. */
    void declare(District d, long now) {
        int dimId = ind.dimId(d.dim);
        int nameId = ind.nameId(d.displayName());
        ByteBuffer r = ind.journal(IndRecords.IND_DISTRICT, IndRecords.District.SIZE, IndRecords.NotLearnable.sent(d.notLearnable));
        r.putLong(IndRecords.District.DISTRICT, d.id);
        r.putInt(IndRecords.District.REVISION, d.revision);
        r.putInt(IndRecords.District.DIM, dimId);
        r.putInt(IndRecords.District.NAME, nameId);
        r.putInt(IndRecords.District.INVENTORY_COUNT, d.inventories.size());
        ind.commit(r);
        long[] chunks = d.chunks.toLongArray();
        java.util.Arrays.sort(chunks);
        int part = 0;
        for (int from = 0; from < chunks.length || part == 0; from += IndRecords.DistrictChunks.MAX_CHUNKS) {
            int n = Math.min(IndRecords.DistrictChunks.MAX_CHUNKS, chunks.length - from);
            ByteBuffer c = ind.journal(IndRecords.IND_DISTRICT_CHUNKS, IndRecords.DistrictChunks.size(n), n);
            c.putLong(IndRecords.DistrictChunks.DISTRICT, d.id);
            c.putInt(IndRecords.DistrictChunks.REVISION, d.revision);
            c.putInt(IndRecords.DistrictChunks.PART, part++);
            for (int i = 0; i < n; i++) {
                c.putInt(IndRecords.DistrictChunks.CHUNKS + 8 * i, ChunkPos.getX(chunks[from + i]));
                c.putInt(IndRecords.DistrictChunks.CHUNKS + 8 * i + 4, ChunkPos.getZ(chunks[from + i]));
            }
            ind.commit(c);
            if (n == 0)
                break;
        }
        for (District.Inventory inv : d.inventories) {
            ByteBuffer i = ind.journal(IndRecords.IND_INVENTORY, IndRecords.Inventory.SIZE, inv.kind());
            i.putLong(IndRecords.Inventory.DISTRICT, d.id);
            i.putInt(IndRecords.Inventory.REVISION, d.revision);
            i.putInt(IndRecords.Inventory.INDEX, inv.index());
            i.putInt(IndRecords.Inventory.X, inv.pos().getX());
            i.putInt(IndRecords.Inventory.Y, inv.pos().getY());
            i.putInt(IndRecords.Inventory.Z, inv.pos().getZ());
            i.putInt(IndRecords.Inventory.ITEM_SLOTS, inv.itemSlots());
            i.putLong(IndRecords.Inventory.FLUID_CAPACITY, inv.fluidCapacity());
            i.putLong(IndRecords.Inventory.ENERGY_CAPACITY, inv.energyCapacity());
            ind.commit(i);
        }
        for (District.Station s : d.stations) {
            ByteBuffer st = ind.journal(IndRecords.IND_STATION, IndRecords.Station.SIZE, 0);
            st.putLong(IndRecords.Station.STATION, s.id());
            st.putLong(IndRecords.Station.DISTRICT, d.id);
            st.putInt(IndRecords.Station.DIM, dimId);
            st.putInt(IndRecords.Station.X, s.pos().getX());
            st.putInt(IndRecords.Station.Y, s.pos().getY());
            st.putInt(IndRecords.Station.Z, s.pos().getZ());
            st.putInt(IndRecords.Station.NAME, ind.nameId(s.name()));
            ind.commit(st);
        }
        d.declaredRevision = d.revision;
        data.setDirty();
        declared++;
    }

    /** A name, exclusion, split or merge changed: a new revision, declared now. */
    void redeclare(District d) {
        d.revision++;
        declare(d, ind.gameTime());
        ind.sampler.revisionChanged(d);
    }
}
