package com.minecraftindustrialcomplex.mic_rtsim.industry;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import it.unimi.dsi.fastutil.longs.Long2LongOpenHashMap;
import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.longs.LongArrayList;
import it.unimi.dsi.fastutil.longs.LongArrayFIFOQueue;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
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
 * <p><b>Detection.</b> A chunk is industrial when it holds a <i>seed</i>, a block entity a player placed
 * that has an item, fluid or energy capability or that a plugin recognises, or when an industrial
 * block entity in it touches one in a member chunk across the chunk border (belts, pipes, shafts:
 * closed under physical connection), or a plugin links it (chain conveyors). A district is a connected
 * group of industrial chunks, 8-neighbour, plus the merge links of {@code /rtsim district merge};
 * chunks a {@code split} pinned belong to their pinned district only, and excluded chunks to none.
 * Unloaded member chunks keep their membership (nothing new can be known about them).
 *
 * <p>Detection runs after edits, at most every {@code rescanIntervalTicks}, over every member chunk of
 * the dimension: the edits are rare and the recompute is O(member chunks) plus the block entities of
 * the loaded ones.
 */
public final class Districts {

    /** {@code #mic_rtsim:district_stock}: the blocks whose inventories are boundary stock (I3). */
    public static final TagKey<Block> STOCK_TAG = TagKey.create(Registries.BLOCK,
            ResourceLocation.fromNamespaceAndPath(MicRtsim.MODID, "district_stock"));

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

    /** The last detection's wall time, in nanoseconds (/rtsim district perf, GameTests). */
    public long lastRescanNanos() {
        return lastRescanNanos;
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

    /** Detection now, whatever the interval (commands and GameTests). */
    public void rescanNow(ServerLevel level) {
        dirty.computeIfAbsent(level.dimension(), k -> new LongOpenHashSet());
        rescan(level, ind.gameTime(), true);
        dirty.remove(level.dimension());
        lastRescan = ind.gameTime();
    }

    // ---- session start --------------------------------------------------------------------------

    void sessionStart(long now) {
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

    void onChunkUnload(ServerLevel level, LevelChunk chunk) {
        long c = chunk.getPos().toLong();
        District d = at(level.dimension(), c);
        if (d != null) {
            LongOpenHashSet set = loaded.get(d.id);
            if (set != null)
                set.remove(c);
            d.loadDirty = true;
            ind.sampler.chunkUnloaded(d);
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
        if (be != null && industrial(level, be, new HashMap<>())) {
            IndustryAttachments.Seeds s = chunk.getData(IndustryAttachments.SEEDS.get());
            chunk.setData(IndustryAttachments.SEEDS.get(), s.plus(pos.asLong()));
            chunk.setUnsaved(true);
            markDirty(level.dimension(), chunk.getPos().toLong());
        }
        if (d != null) {
            edit(d, IndRecords.Edit.PLACED, player, pos);
            if (be != null)
                markDirty(level.dimension(), chunk.getPos().toLong());
        }
    }

    /** A block a player broke (before it is gone): an edit, and a rescan if it was industrial. */
    void playerBroke(ServerLevel level, BlockPos pos, UUID player) {
        long c = ChunkPos.asLong(pos.getX() >> 4, pos.getZ() >> 4);
        District d = at(level.dimension(), c);
        if (d != null)
            edit(d, IndRecords.Edit.BROKEN, player, pos);
        if (level.getBlockEntity(pos) != null)
            markDirty(level.dimension(), c);
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
    }

    public void discard(District d, int reason) {
        ByteBuffer r = ind.journal(IndRecords.IND_DISCARD, IndRecords.Discard.SIZE, reason);
        r.putLong(IndRecords.Discard.DISTRICT, d.id);
        ind.commit(r);
    }

    /** Adopts every industrial block entity in the chunks around {@code centre} as seeds (worlds made before the mod). */
    public int adopt(ServerLevel level, ChunkPos centre, int radius) {
        int n = 0;
        Map<BlockPos, Boolean> cache = new HashMap<>();
        for (int dx = -radius; dx <= radius; dx++) {
            for (int dz = -radius; dz <= radius; dz++) {
                LevelChunk chunk = level.getChunkSource().getChunkNow(centre.x + dx, centre.z + dz);
                if (chunk == null)
                    continue;
                IndustryAttachments.Seeds s = chunk.getData(IndustryAttachments.SEEDS.get());
                IndustryAttachments.Seeds before = s;
                for (BlockEntity be : chunk.getBlockEntities().values()) {
                    if (industrial(level, be, cache)) {
                        s = s.plus(be.getBlockPos().asLong());
                    }
                }
                if (s != before) {
                    n += s.positions().length - before.positions().length;
                    chunk.setData(IndustryAttachments.SEEDS.get(), s);
                    chunk.setUnsaved(true);
                    markDirty(level.dimension(), chunk.getPos().toLong());
                }
            }
        }
        return n;
    }

    // ---- the tick -------------------------------------------------------------------------------

    void tick(long now, long ticks) {
        int interval = IndustryConfig.get(IndustryConfig.RESCAN_INTERVAL);
        if (!dirty.isEmpty() && now - lastRescan >= interval) {
            for (Map.Entry<ResourceKey<Level>, LongOpenHashSet> e : new ArrayList<>(dirty.entrySet())) {
                ServerLevel level = ind.level(e.getKey());
                if (level != null)
                    rescan(level, now, fullDims.contains(e.getKey()));
            }
            dirty.clear();
            fullDims.clear();
            lastRescan = now;
        }
        if (now - lastPeriodic >= IndustryConfig.get(IndustryConfig.PERIODIC_RESCAN)) {
            lastPeriodic = now;
            for (District d : data.districts.values())
                if (loadedChunks(d) > 0) {
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
     * IND_APPROACH (choice H23): a player within {@code approachRadiusBlocks} of an unloaded district's
     * bounding box. The engine-agnostic half of the stale-tick race's option (d): what to do with it
     * (sending reconciles ahead) is the engine's and Ben's call.
     */
    private void approach(long now) {
        int radius = IndustryConfig.get(IndustryConfig.APPROACH_RADIUS);
        for (ServerLevel level : ind.server.getAllLevels()) {
            if (level.players().isEmpty())
                continue;
            for (District d : data.districts.values()) {
                if (d.dim != level.dimension() || d.loadState != IndRecords.LoadState.UNLOADED || d.chunks.isEmpty())
                    continue;
                int best = Integer.MAX_VALUE;
                for (ServerPlayer p : level.players()) {
                    double dx = Math.max(0, Math.max(d.minCx * 16 - p.getX(), p.getX() - (d.maxCx * 16 + 16)));
                    double dz = Math.max(0, Math.max(d.minCz * 16 - p.getZ(), p.getZ() - (d.maxCz * 16 + 16)));
                    best = Math.min(best, (int) Math.sqrt(dx * dx + dz * dz));
                }
                if (best > radius) {
                    d.lastApproachDistance = Integer.MAX_VALUE;
                    continue;
                }
                // The rate of closing from the last check (every 20 ticks), for an ETA; unknown at first.
                long eta = 0xFFFF_FFFFL;
                boolean closing = false;
                if (d.lastApproachDistance != Integer.MAX_VALUE && now > d.approachSentAt) {
                    double rate = (double) (d.lastApproachDistance - best) / (now - d.approachSentAt);
                    closing = rate > 0;
                    if (closing)
                        eta = Math.min(0xFFFF_FFFEL, (long) Math.ceil(best / rate));
                }
                d.lastApproachDistance = best;
                d.approachSentAt = now;
                int at = ind.latest(IndRecords.IND_APPROACH, IndRecords.Approach.SIZE, closing ? IndRecords.Approach.FLAG_CLOSING : 0);
                if (at < 0)
                    continue;
                ByteBuffer b = ind.open();
                b.putLong(at + IndRecords.Approach.GAME_TIME, now);
                b.putLong(at + IndRecords.Approach.DISTRICT, d.id);
                b.putInt(at + IndRecords.Approach.DISTANCE, best);
                b.putInt(at + IndRecords.Approach.ETA_TICKS, (int) eta);
            }
        }
    }

    // ---- detection ------------------------------------------------------------------------------

    /** One industrial block entity's touch across a chunk border. */
    private record Touch(long a, long b, BlockPos pa, BlockPos pb) {}

    private record Candidate(BlockPos pos, int kind) {}

    private final class Scan {
        final Map<Long, List<Candidate>> inventories = new HashMap<>();
        final Map<Long, List<IndustryPlugin.StationInfo>> stations = new HashMap<>();
        final LongOpenHashSet creative = new LongOpenHashSet();
        final Map<Long, List<BlockPos>> watch = new HashMap<>();
        final List<Touch> touches = new ArrayList<>();
        final List<long[]> links = new ArrayList<>();
        final Map<BlockPos, Boolean> industrialCache = new HashMap<>();
    }

    /**
     * One detection in a dimension. {@code full}: every district of the dimension (commands, the
     * periodic pass); otherwise only the districts an edit can change: those owning a dirty chunk or a
     * chunk next to one, grown until nothing the walk reaches belongs to a district outside the set.
     */
    private void rescan(ServerLevel level, long now, boolean full) {
        long t0 = System.nanoTime();
        ResourceKey<Level> dim = level.dimension();
        Long2ObjectOpenHashMap<District> index = byChunk.computeIfAbsent(dim, k -> new Long2ObjectOpenHashMap<>());
        LongOpenHashSet excluded = data.excluded(dim);
        Long2LongOpenHashMap pins = data.pins(dim);
        LongOpenHashSet dirtyHere = dirty.getOrDefault(dim, new LongOpenHashSet());

        Set<Long> affected = new HashSet<>();
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

        Scan scan;
        LongOpenHashSet visited;
        while (true) {
            LongOpenHashSet candidates = new LongOpenHashSet(dirtyHere);
            for (long id : affected) {
                District d = data.districts.get(id);
                if (d != null)
                    candidates.addAll(d.chunks);
            }
            for (Long2LongOpenHashMap.Entry e : pins.long2LongEntrySet())
                if (affected.contains(e.getLongValue()) || dirtyHere.contains(e.getLongKey()))
                    candidates.add(e.getLongKey());

            scan = new Scan();
            visited = new LongOpenHashSet();
            LongArrayFIFOQueue queue = new LongArrayFIFOQueue();
            for (long c : candidates) {
                if (excluded.contains(c))
                    continue;
                LevelChunk chunk = level.getChunkSource().getChunkNow(ChunkPos.getX(c), ChunkPos.getZ(c));
                if (chunk == null)
                    continue;
                if (hasSeeds(level, chunk) || pins.containsKey(c) || (index.containsKey(c) && touchesUnloadedMember(level, chunk, index, scan))) {
                    if (visited.add(c))
                        queue.enqueue(c);
                }
            }
            final LongOpenHashSet seen = visited;
            final Scan sc = scan;
            while (!queue.isEmpty()) {
                long c = queue.dequeueLong();
                LevelChunk chunk = level.getChunkSource().getChunkNow(ChunkPos.getX(c), ChunkPos.getZ(c));
                if (chunk == null)
                    continue;
                scanChunk(level, chunk, sc, excluded, (n) -> {
                    if (!excluded.contains(n) && seen.add(n))
                        queue.enqueue(n);
                });
            }
            if (full)
                break;
            // Grow: a district next to (or pinned at) a chunk the walk reached may merge with it.
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
            if (more.isEmpty())
                break;
            affected.addAll(more);
        }

        // Members: the reached loaded chunks, the unloaded chunks of the districts in question, their
        // pinned chunks; never an excluded one.
        LongOpenHashSet members = new LongOpenHashSet(visited);
        for (long id : affected) {
            District d = data.districts.get(id);
            if (d != null)
                for (long c : d.chunks)
                    if (level.getChunkSource().getChunkNow(ChunkPos.getX(c), ChunkPos.getZ(c)) == null)
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
                boolean isLoaded = visited.contains(c);
                if (isLoaded) {
                    for (Candidate cand : scan.inventories.getOrDefault(c, List.of())) {
                        if (seen.add(cand.pos)) {
                            District.Inventory inv = describe(level, cand);
                            if (inv != null)
                                invs.add(inv);
                        } else if (cand.kind != 0) {
                            // Tagged storage that is also a packager's target or a postbox: one inventory, both kinds.
                            for (int i = 0; i < invs.size(); i++)
                                if (invs.get(i).pos().equals(cand.pos))
                                    invs.set(i, new District.Inventory(0, cand.pos, invs.get(i).kind() | cand.kind, invs.get(i).itemSlots(),
                                            invs.get(i).fluidCapacity(), invs.get(i).energyCapacity()));
                        }
                    }
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
                loaded.put(id, ls);
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
        rescans++;
        lastRescanNanos = System.nanoTime() - t0;
        rescanNanos += lastRescanNanos;
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

    private boolean hasSeeds(ServerLevel level, LevelChunk chunk) {
        IndustryAttachments.Seeds s = chunk.getExistingDataOrNull(IndustryAttachments.SEEDS.get());
        if (s == null || s.isEmpty())
            return false;
        // Seeds whose block entity is gone (broken) are dropped here.
        IndustryAttachments.Seeds kept = s;
        for (long p : s.positions())
            if (chunk.getBlockEntities().get(BlockPos.of(p)) == null)
                kept = kept.minus(p);
        if (kept != s) {
            if (kept.isEmpty())
                chunk.removeData(IndustryAttachments.SEEDS.get());
            else
                chunk.setData(IndustryAttachments.SEEDS.get(), kept);
            chunk.setUnsaved(true);
        }
        return !kept.isEmpty();
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
            if (!industrial(level, be, scan.industrialCache))
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

    private void scanChunk(ServerLevel level, LevelChunk chunk, Scan scan, LongOpenHashSet excluded, java.util.function.LongConsumer reach) {
        long c = chunk.getPos().toLong();
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
        for (BlockEntity be : new ArrayList<>(chunk.getBlockEntities().values())) {
            if (be.isRemoved())
                continue;
            BlockPos p = be.getBlockPos();
            for (IndustryPlugin plugin : IndustryApi.PLUGINS)
                plugin.scanned(be, notes);
            boolean isIndustrial = industrial(level, be, scan.industrialCache);
            if (!isIndustrial)
                continue;
            BlockState state = be.getBlockState();
            // Boundary stock (I3): tagged storage, and what plugins add.
            if (state.is(STOCK_TAG)) {
                BlockPos canon = canonicalInventory(level, p);
                if (canon != null)
                    scan.inventories.computeIfAbsent(c, k -> new ArrayList<>()).add(new Candidate(canon, 0));
            }
            for (IndustryPlugin plugin : IndustryApi.PLUGINS)
                plugin.inventories(be, (pos, kind) -> {
                    BlockPos canon = canonicalInventory(level, pos);
                    if (canon != null)
                        scan.inventories.computeIfAbsent(c, k -> new ArrayList<>()).add(new Candidate(canon, kind));
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
                    if (nbe == null || !industrial(level, nbe, scan.industrialCache))
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
                    if (tbe == null || !industrial(level, tbe, scan.industrialCache))
                        return;
                    scan.links.add(new long[] {c, tc});
                    scan.touches.add(new Touch(c, tc, p, target.immutable()));
                    if (!excluded.contains(tc))
                        reach.accept(tc);
                });
            }
        }
    }

    private int crossingCode(ServerLevel level, BlockPos a, BlockPos b) {
        BlockEntity ba = level.getBlockEntity(a), bb = level.getBlockEntity(b);
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

    /** Industrial: a plugin recognises it, or it has an item, fluid or energy capability. */
    boolean industrial(ServerLevel level, BlockEntity be, Map<BlockPos, Boolean> cache) {
        BlockPos p = be.getBlockPos();
        Boolean known = cache.get(p);
        if (known != null)
            return known;
        boolean result = false;
        for (IndustryPlugin plugin : IndustryApi.PLUGINS) {
            if (plugin.isIndustrial(be)) {
                result = true;
                break;
            }
        }
        if (!result) {
            BlockState s = be.getBlockState();
            result = level.getCapability(Capabilities.ItemHandler.BLOCK, p, s, be, null) != null
                    || level.getCapability(Capabilities.FluidHandler.BLOCK, p, s, be, null) != null
                    || level.getCapability(Capabilities.EnergyStorage.BLOCK, p, s, be, null) != null;
            if (!result)
                for (Direction d : Direction.values())
                    if (level.getCapability(Capabilities.ItemHandler.BLOCK, p, s, be, d) != null
                            || level.getCapability(Capabilities.FluidHandler.BLOCK, p, s, be, d) != null) {
                        result = true;
                        break;
                    }
        }
        cache.put(p.immutable(), result);
        return result;
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
        dirty.remove(dim);
        data.setDirty();
    }

    /**
     * GameTests: forgets every district of the level's dimension, the seeds and epochs of their loaded
     * chunks, and the dimension's pins, exclusions and merge links. The industry tests run one at a
     * time (one batch each), so each starts from no industry at all.
     */
    public void forgetAllForTest(ServerLevel level) {
        ResourceKey<Level> dim = level.dimension();
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
        d.loadDirty = true;
        return d;
    }

    /** GameTests: adds a chunk to a district as if detection had kept it (an unloaded member chunk). */
    public void addChunkForTest(District d, long chunk) {
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
        ByteBuffer r = ind.journal(IndRecords.IND_DISTRICT, IndRecords.District.SIZE, d.notLearnable);
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
