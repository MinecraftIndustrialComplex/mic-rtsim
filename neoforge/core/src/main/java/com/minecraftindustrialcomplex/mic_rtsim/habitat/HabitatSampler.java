package com.minecraftindustrialcomplex.mic_rtsim.habitat;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugin;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.herd.HerdBodies;
import it.unimi.dsi.fastutil.longs.Long2IntOpenHashMap;
import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.longs.LongLinkedOpenHashSet;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.QuartPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.tags.BlockTags;
import net.minecraft.tags.FluidTags;
import net.minecraft.tags.TagKey;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.MobCategory;
import net.minecraft.world.entity.MobSpawnType;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.MobSpawnSettings;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.material.FluidState;

import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;

/**
 * The habitat sampler (docs/boundary.md §10; choices X2-# in docs/overnight-choices-worldmodel.md): the engine's
 * world model comes from here. Vanilla is built in (D4), so the core registers it as its own
 * {@link SessionPlugin} (commands 0x8900 to 0x89FF).
 *
 * <p><b>What it samples.</b> A 64-block cell (4 x 4 chunks) once all 16 of its chunks are loaded in full: one
 * point per chunk, at the chunk's centre column, read from the chunk itself (no lookup through the level):
 * the {@code MOTION_BLOCKING_NO_LEAVES} top block and what it is (water, ice, another fluid or land), the
 * {@code OCEAN_FLOOR} under water, and the noise biome at the surface. The cell's record carries masks of the
 * 16 points, the most common biome (of all points and of the land points), tag bits, heights, vanilla's
 * temperature and downfall, a climate plugin's degrees and season when one is registered
 * ({@link Climates}), the herd species' share of each biome's {@code CREATURE} spawn weight, and the animals
 * of those species vanilla spawned at chunk generation in the cell. A biome's facts are read once per
 * session and cached.
 *
 * <p><b>When.</b> A cell is queued when one of its chunks loads (generated or read from disk) and sampled at
 * the end of a tick under a budget ({@link #cellsPerTick}, {@link #budgetNanos}); one whose chunks are not
 * all loaded leaves the queue and comes back with its next chunk. A cell is sampled at most once per load
 * of its chunks: an unload lets it be sampled again on the next load, so a change players made shows.
 *
 * <p><b>Sent once, then only on change.</b> At a session start the engine lists what it holds
 * (HABITAT_KNOWN: each cell's fingerprint, then END). Until END arrives nothing is sampled. A cell is sent
 * when its fingerprint differs from the engine's (a new cell, a changed one, one a crash took from the
 * engine) or when chunk-generation animals were counted in it this session; otherwise nothing crosses.
 */
public final class HabitatSampler implements SessionPlugin {
    public static final HabitatSampler INSTANCE = new HabitatSampler();

    /** Cells sampled per tick at most. PLACEHOLDER. */
    public static int cellsPerTick = 8;
    /** Sampling time per tick at most, nanoseconds: no cell starts past it. PLACEHOLDER 0.25 ms. */
    public static long budgetNanos = 250_000L;

    /** The creature kinds the records carry: the vanilla herd adapter's species (docs/boundary.md §8.2). */
    public static final List<String> CREATURES = HerdBodies.SPECIES;

    private static final TagKey<Biome>[] TAG_KEYS = tagKeys();

    @SuppressWarnings("unchecked")
    private static TagKey<Biome>[] tagKeys() {
        TagKey<Biome>[] keys = new TagKey[HabitatRecords.TAGS.length];
        for (int i = 0; i < keys.length; i++)
            keys[i] = TagKey.create(Registries.BIOME, ResourceLocation.parse(HabitatRecords.TAGS[i]));
        return keys;
    }

    /** Point kinds. */
    static final int LAND = 0, WATER = 1, ICE = 2, FLUID = 3;

    /** Chunks loaded in full, per level, whatever the session (chunks load before a session starts). */
    private static final Map<ResourceKey<Level>, LongOpenHashSet> LOADED = new HashMap<>();

    /** One level's state for this session. */
    static final class Dim {
        /** The engine's fingerprint per cell (HABITAT_KNOWN, then every cell sent). */
        final Long2IntOpenHashMap known = new Long2IntOpenHashMap();
        /** Cells sampled since their chunks last loaded. */
        final LongOpenHashSet sampled = new LongOpenHashSet();
        final LongLinkedOpenHashSet queue = new LongLinkedOpenHashSet();
        /** Chunk-generation animals per cell, per {@link #CREATURES} index, not yet sent. */
        final Long2ObjectOpenHashMap<int[]> generated = new Long2ObjectOpenHashMap<>();
    }

    /** A biome's facts, read once per session. */
    record BiomeInfo(String name, int tags, float temperature, float downfall, int[] shares) {}

    private RtsimSession session;
    private boolean synced;
    private final Map<ResourceKey<Level>, Dim> dims = new HashMap<>();
    private final IdentityHashMap<Holder<Biome>, BiomeInfo> biomes = new IdentityHashMap<>();
    /** Chunk-generation animals that joined this tick, per level and chunk: their chunk's load claims them. */
    private final Map<ResourceKey<Level>, Long2ObjectOpenHashMap<int[]>> joinedThisTick = new HashMap<>();
    private final BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();
    private long[] seasonThisTick;

    // ---- counters (tests, /rtsim perf, the bench) ------------------------------------------------
    public long chunkLoads, cellsSampled, cellsSent, cellsUnchanged, cellsIncomplete, recordsDropped, knownRecords,
            knownCells, generatedAnimals;
    /** Nanoseconds spent sampling cells (reading their 16 points and building the record), and the worst cell. */
    public long sampleNanos, maxCellNanos;
    /** Nanoseconds spent in the chunk-load hook. */
    public long loadHookNanos;

    private HabitatSampler() {}

    public RtsimSession session() {
        return session;
    }

    /** True once the engine's HABITAT_KNOWN list ended this session. */
    public boolean synced() {
        return synced;
    }

    /** Cells waiting in the queue, every level. */
    public int queued() {
        int n = 0;
        for (Dim d : dims.values())
            n += d.queue.size();
        return n;
    }

    /** The engine's fingerprint of a cell as this session knows it, or null. */
    public Integer knownFingerprint(ResourceKey<Level> level, int cx, int cz) {
        Dim d = dims.get(level);
        long k = HabitatRecords.key(cx, cz);
        return d == null || !d.known.containsKey(k) ? null : d.known.get(k);
    }

    private Dim dim(ResourceKey<Level> level) {
        return dims.computeIfAbsent(level, k -> new Dim());
    }

    private boolean live() {
        RtsimSession s = RtsimSession.current();
        return s != null && s == session && !s.failed();
    }

    // ---- SessionPlugin ---------------------------------------------------------------------------

    @Override
    public String name() {
        return "habitat";
    }

    @Override
    public boolean ownsCommand(int kind) {
        return HabitatRecords.isHabitatCommand(kind);
    }

    @Override
    public void sessionStarted(RtsimSession s) {
        session = s;
        synced = false;
        dims.clear();
        biomes.clear();
        joinedThisTick.clear();
        chunkLoads = cellsSampled = cellsSent = cellsUnchanged = cellsIncomplete = recordsDropped = knownRecords = knownCells = 0;
        generatedAnimals = sampleNanos = maxCellNanos = loadHookNanos = 0;
        // Every chunk already loaded (the spawn area, a world loaded before this session) is a first load.
        for (Map.Entry<ResourceKey<Level>, LongOpenHashSet> e : LOADED.entrySet()) {
            Dim d = dim(e.getKey());
            for (long c : e.getValue())
                d.queue.add(HabitatRecords.key(ChunkPos.getX(c) >> 2, ChunkPos.getZ(c) >> 2));
        }
    }

    @Override
    public boolean accept(RtsimSession s, ByteBuffer b, int at, int kind, int size, int aux, EngineNames names) {
        if (kind != HabitatRecords.HABITAT_KNOWN || size < HabitatRecords.Known.ENTRIES)
            return true;
        knownRecords++;
        if ((aux & HabitatRecords.Known.FLAG_END) != 0) {
            synced = true;
            return true;
        }
        int n = b.getInt(at + HabitatRecords.Known.COUNT);
        if (n < 0 || size < HabitatRecords.Known.ENTRIES + HabitatRecords.Known.ENTRY * n)
            return true;
        String dimName = names.name(Obs.DICT_DIMENSION, b.getInt(at + HabitatRecords.Known.DIM));
        if (dimName == null)
            return true;
        Dim d = dim(ResourceKey.create(Registries.DIMENSION, ResourceLocation.parse(dimName)));
        int rx = b.getInt(at + HabitatRecords.Known.REGION_X), rz = b.getInt(at + HabitatRecords.Known.REGION_Z);
        for (int i = 0; i < n; i++) {
            int e = at + HabitatRecords.Known.ENTRIES + HabitatRecords.Known.ENTRY * i;
            int idx = b.get(e) & 0xFF;
            int cx = rx * HabitatRecords.REGION_CELLS + idx % HabitatRecords.REGION_CELLS;
            int cz = rz * HabitatRecords.REGION_CELLS + idx / HabitatRecords.REGION_CELLS;
            d.known.put(HabitatRecords.key(cx, cz), b.getInt(e + 4));
            knownCells++;
        }
        return true;
    }

    @Override
    public void endOfTick(RtsimSession s, long now) {
        if (s != session)
            return;
        try {
            if (synced)
                drain(s, now);
        } finally {
            joinedThisTick.clear();
            seasonThisTick = null;
        }
    }

    // ---- hooks (HabitatEvents) -------------------------------------------------------------------

    /** A chunk reached full status ({@code ChunkEvent.Load}, server thread). */
    void onChunkLoad(ServerLevel level, LevelChunk chunk, boolean fresh) {
        long t0 = System.nanoTime();
        ChunkPos cp = chunk.getPos();
        LOADED.computeIfAbsent(level.dimension(), k -> new LongOpenHashSet()).add(cp.toLong());
        if (!live())
            return;
        chunkLoads++;
        Dim d = dim(level.dimension());
        long cell = HabitatRecords.key(cp.x >> 2, cp.z >> 2);
        if (fresh) {
            Long2ObjectOpenHashMap<int[]> j = joinedThisTick.get(level.dimension());
            int[] counts = j == null ? null : j.remove(cp.toLong());
            if (counts != null) {
                int[] g = d.generated.computeIfAbsent(cell, k -> new int[CREATURES.size()]);
                for (int i = 0; i < counts.length; i++)
                    g[i] += counts[i];
            }
            d.sampled.remove(cell);
        }
        if (!d.sampled.contains(cell))
            d.queue.add(cell);
        loadHookNanos += System.nanoTime() - t0;
    }

    /** A chunk unloaded: its cell may be sampled again when its chunks are back. */
    void onChunkUnload(ServerLevel level, ChunkPos cp) {
        LongOpenHashSet loaded = LOADED.get(level.dimension());
        if (loaded != null)
            loaded.remove(cp.toLong());
        if (!live())
            return;
        Dim d = dims.get(level.dimension());
        if (d != null)
            d.sampled.remove(HabitatRecords.key(cp.x >> 2, cp.z >> 2));
    }

    /**
     * An entity joined a level. A herd species' animal that vanilla spawned at chunk generation
     * ({@code MobSpawnType.CHUNK_GENERATION}; such entities join with {@code loadedFromDisk} true, from their
     * proto chunk's NBT, in the same call that then posts the chunk's {@code ChunkEvent.Load}) is counted for
     * its chunk; the chunk's load as a new chunk claims the count (choice X2-13).
     */
    void onEntityJoin(ServerLevel level, net.minecraft.world.entity.Entity e, boolean loadedFromDisk) {
        if (!loadedFromDisk || !(e instanceof Mob m) || m.getSpawnType() != MobSpawnType.CHUNK_GENERATION || !live())
            return;
        int idx = CREATURES.indexOf(BuiltInRegistries.ENTITY_TYPE.getKey(e.getType()).toString());
        if (idx < 0)
            return;
        ChunkPos cp = e.chunkPosition();
        joinedThisTick.computeIfAbsent(level.dimension(), k -> new Long2ObjectOpenHashMap<>())
                .computeIfAbsent(cp.toLong(), k -> new int[CREATURES.size()])[idx]++;
        generatedAnimals++;
    }

    /** Forgets that a cell was sampled, so it is sampled again (GameTests that change a cell's ground). */
    public void resample(ServerLevel level, int cx, int cz) {
        Dim d = dim(level.dimension());
        long cell = HabitatRecords.key(cx, cz);
        d.sampled.remove(cell);
        d.queue.add(cell);
    }

    /**
     * The bench (GameTests with {@code -PrtsimBench}): reads one cell's sixteen points and builds its record's
     * fields and fingerprint as {@link #sample} does, writing nothing. Returns the nanoseconds it took, or -1
     * when a chunk of the cell is not loaded. {@code coldBiomes} forgets the biome facts first.
     */
    public long benchRead(ServerLevel level, int cx, int cz, boolean coldBiomes) {
        if (coldBiomes)
            biomes.clear();
        long t0 = System.nanoTime();
        LevelChunk[] chunks = new LevelChunk[HabitatRecords.POINTS];
        for (int i = 0; i < HabitatRecords.POINTS; i++) {
            chunks[i] = level.getChunkSource().getChunkNow(cx * 4 + (i & 3), cz * 4 + (i >> 2));
            if (chunks[i] == null)
                return -1;
        }
        Cell c = read(level, cx, cz, chunks);
        long spent = System.nanoTime() - t0;
        return c.fingerprint == 0x7FFF_FFFF && spent == 0 ? -2 : spent; // keep the read alive
    }

    /** Forgets every loaded chunk (a GameTest server that reloads levels keeps none; not used in play). */
    static void forgetLoaded(ResourceKey<Level> level) {
        LOADED.remove(level);
    }

    // ---- the work --------------------------------------------------------------------------------

    private void drain(RtsimSession s, long now) {
        long t0 = System.nanoTime();
        int done = 0;
        MinecraftServer server = s.server();
        boolean any = true;
        while (any && done < cellsPerTick && System.nanoTime() - t0 < budgetNanos) {
            any = false;
            for (Map.Entry<ResourceKey<Level>, Dim> e : dims.entrySet()) {
                Dim d = e.getValue();
                if (d.queue.isEmpty())
                    continue;
                ServerLevel level = server.getLevel(e.getKey());
                long cell = d.queue.removeFirstLong();
                any = true;
                if (level == null)
                    continue;
                if (!sample(s, level, d, cell, now)) {
                    // No room in the buffer: try again next tick, first.
                    d.queue.addAndMoveToFirst(cell);
                    return;
                }
                done++;
                if (done >= cellsPerTick || System.nanoTime() - t0 >= budgetNanos)
                    return;
            }
        }
    }

    /** Samples one cell and sends it when the engine's copy differs. False when the record found no room. */
    private boolean sample(RtsimSession s, ServerLevel level, Dim d, long cell, long now) {
        long t0 = System.nanoTime();
        int cx = HabitatRecords.keyX(cell), cz = HabitatRecords.keyZ(cell);
        LevelChunk[] chunks = new LevelChunk[HabitatRecords.POINTS];
        for (int i = 0; i < HabitatRecords.POINTS; i++) {
            LevelChunk c = level.getChunkSource().getChunkNow(cx * 4 + (i & 3), cz * 4 + (i >> 2));
            if (c == null) {
                cellsIncomplete++; // back in the queue with its next chunk
                return true;
            }
            chunks[i] = c;
        }
        Cell c = read(level, cx, cz, chunks);
        d.sampled.add(cell);
        cellsSampled++;
        int[] gen = d.generated.get(cell);
        boolean same = d.known.containsKey(cell) && d.known.get(cell) == c.fingerprint;
        long spent = System.nanoTime() - t0;
        sampleNanos += spent;
        maxCellNanos = Math.max(maxCellNanos, spent);
        if (same && gen == null) {
            cellsUnchanged++;
            return true;
        }
        if (!write(s, level, cx, cz, c, gen, now)) {
            d.sampled.remove(cell);
            recordsDropped++;
            return false;
        }
        d.known.put(cell, c.fingerprint);
        d.generated.remove(cell);
        cellsSent++;
        return true;
    }

    /** One sampled cell, before it is written. */
    static final class Cell {
        int water, ice, fluid;
        BiomeInfo biome, landBiome;
        int tags;
        short min, max, mean, waterMean, floorMean;
        float temperature, downfall;
        int[] shares = new int[CREATURES.size()];
        int fingerprint;
    }

    private BiomeInfo info(Holder<Biome> h) {
        BiomeInfo i = biomes.get(h);
        if (i != null)
            return i;
        String name = h.unwrapKey().map(k -> k.location().toString()).orElse("mic_rtsim:unknown");
        int tags = 0;
        for (int b = 0; b < TAG_KEYS.length; b++)
            if (h.is(TAG_KEYS[b]))
                tags |= 1 << b;
        Biome v = h.value();
        int[] shares = new int[CREATURES.size()];
        int total = 0;
        for (MobSpawnSettings.SpawnerData sd : v.getMobSettings().getMobs(MobCategory.CREATURE).unwrap()) {
            int w = sd.getWeight().asInt();
            total += w;
            int idx = CREATURES.indexOf(BuiltInRegistries.ENTITY_TYPE.getKey(sd.type).toString());
            if (idx >= 0)
                shares[idx] += w;
        }
        for (int k = 0; k < shares.length; k++)
            shares[k] = total > 0 ? shares[k] * 1000 / total : 0;
        i = new BiomeInfo(name, tags, v.getBaseTemperature(), v.getModifiedClimateSettings().downfall(), shares);
        biomes.put(h, i);
        return i;
    }

    /** Reads the 16 points of a cell from its chunks. */
    Cell read(ServerLevel level, int cx, int cz, LevelChunk[] chunks) {
        Cell c = new Cell();
        @SuppressWarnings("unchecked")
        Holder<Biome>[] hs = new Holder[HabitatRecords.POINTS];
        BiomeInfo[] infos = new BiomeInfo[HabitatRecords.POINTS];
        int[] tops = new int[HabitatRecords.POINTS];
        int[] kinds = new int[HabitatRecords.POINTS];
        int min = Integer.MAX_VALUE, max = Integer.MIN_VALUE;
        long sum = 0, waterSum = 0, floorSum = 0;
        int waterN = 0, landN = 0;
        double temp = 0, down = 0;
        int minY = level.getMinBuildHeight();
        for (int i = 0; i < HabitatRecords.POINTS; i++) {
            LevelChunk ch = chunks[i];
            int bx = ch.getPos().getMinBlockX() + 8, bz = ch.getPos().getMinBlockZ() + 8;
            int top = ch.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, 8, 8);
            int kind;
            if (top < minY) {
                kind = FLUID; // no ground at all (the void)
            } else {
                BlockState st = ch.getBlockState(pos.set(bx, top, bz));
                FluidState fl = st.getFluidState();
                if (fl.is(FluidTags.WATER))
                    kind = WATER;
                else if (st.is(BlockTags.ICE))
                    kind = ICE;
                else if (!fl.isEmpty())
                    kind = FLUID;
                else
                    kind = LAND;
            }
            int y = Math.max(top, minY);
            kinds[i] = kind;
            tops[i] = y;
            switch (kind) {
                case WATER -> {
                    c.water |= 1 << i;
                    waterSum += y;
                    floorSum += Math.max(ch.getHeight(Heightmap.Types.OCEAN_FLOOR, 8, 8), minY);
                    waterN++;
                }
                case ICE -> c.ice |= 1 << i;
                case FLUID -> c.fluid |= 1 << i;
                default -> landN++;
            }
            hs[i] = ch.getNoiseBiome(QuartPos.fromBlock(bx), QuartPos.fromBlock(y), QuartPos.fromBlock(bz));
            infos[i] = info(hs[i]);
            c.tags |= infos[i].tags();
            temp += infos[i].temperature();
            down += infos[i].downfall();
            min = Math.min(min, y);
            max = Math.max(max, y);
            sum += y;
        }
        c.biome = most(hs, infos, kinds, -1);
        c.landBiome = landN > 0 ? most(hs, infos, kinds, LAND) : null;
        c.min = (short) min;
        c.max = (short) max;
        c.mean = (short) Math.floorDiv(sum, HabitatRecords.POINTS);
        c.waterMean = waterN > 0 ? (short) Math.floorDiv(waterSum, waterN) : 0;
        c.floorMean = waterN > 0 ? (short) Math.floorDiv(floorSum, waterN) : 0;
        c.temperature = (float) (temp / HabitatRecords.POINTS);
        c.downfall = (float) (down / HabitatRecords.POINTS);
        if (landN > 0) {
            for (int i = 0; i < HabitatRecords.POINTS; i++)
                if (kinds[i] == LAND)
                    for (int k = 0; k < c.shares.length; k++)
                        c.shares[k] += infos[i].shares()[k];
            for (int k = 0; k < c.shares.length; k++)
                c.shares[k] /= landN;
        }
        c.fingerprint = fingerprint(c);
        return c;
    }

    /** The most common biome among the points of a kind (-1: every point); ties go to the earliest point. */
    private static BiomeInfo most(Holder<Biome>[] hs, BiomeInfo[] infos, int[] kinds, int kind) {
        BiomeInfo best = null;
        int bestN = 0;
        for (int i = 0; i < hs.length; i++) {
            if (kind >= 0 && kinds[i] != kind)
                continue;
            int n = 0;
            for (int j = 0; j < hs.length; j++)
                if (hs[j] == hs[i] && (kind < 0 || kinds[j] == kind))
                    n++;
            if (n > bestN) {
                bestN = n;
                best = infos[i];
            }
        }
        return best;
    }

    /**
     * The fingerprint of a cell's terrain (docs/boundary.md §10.4): FNV-1a 64 over the little-endian bytes of
     * eight words, folded to 32 bits, as {@code rtsim_space::habitat::fingerprint}. Chunk-generation counts,
     * degrees and the season are left out: they change without the ground changing.
     */
    static int fingerprint(Cell c) {
        long shares = 0;
        for (int k = 0; k < c.shares.length; k++)
            shares = Long.rotateLeft(shares, 11) ^ c.shares[k];
        long[] words = {
                (c.water & 0xFFFFL) | (c.ice & 0xFFFFL) << 16 | (c.fluid & 0xFFFFL) << 32,
                fnv(c.biome == null ? "" : c.biome.name()),
                fnv(c.landBiome == null ? "" : c.landBiome.name()),
                (c.tags & 0xFFFF_FFFFL) | (long) (c.landBiome == null ? 0 : c.landBiome.tags()) << 32,
                (c.min & 0xFFFFL) | (c.max & 0xFFFFL) << 16 | (c.mean & 0xFFFFL) << 32 | (c.waterMean & 0xFFFFL) << 48,
                c.floorMean & 0xFFFFL,
                (Float.floatToRawIntBits(c.temperature) & 0xFFFF_FFFFL) | (long) Float.floatToRawIntBits(c.downfall) << 32,
                shares,
        };
        long h = 0xcbf29ce484222325L;
        for (long w : words)
            for (int b = 0; b < 8; b++) {
                h ^= (w >>> (8 * b)) & 0xFF;
                h *= 0x100000001b3L;
            }
        return (int) (h ^ (h >>> 32));
    }

    static long fnv(String s) {
        long h = 0xcbf29ce484222325L;
        for (byte b : s.getBytes(StandardCharsets.UTF_8)) {
            h ^= b & 0xFF;
            h *= 0x100000001b3L;
        }
        return h;
    }

    /** Writes a cell's HABITAT. False when the buffer had no room (the record was dropped and counted). */
    private boolean write(RtsimSession s, ServerLevel level, int cx, int cz, Cell c, int[] gen, long now) {
        // Ids before direct(): an id is defined before its first use.
        int dim = s.dimId(level.dimension());
        int biome = c.biome == null ? 0 : s.dictId(HabitatRecords.DICT_BIOME, c.biome.name());
        int land = c.landBiome == null ? 0 : s.dictId(HabitatRecords.DICT_BIOME, c.landBiome.name());
        int n = 0;
        for (int k = 0; k < CREATURES.size(); k++)
            if (c.shares[k] > 0 || (gen != null && gen[k] > 0))
                n++;
        int[] bodies = new int[CREATURES.size()];
        for (int k = 0; k < CREATURES.size(); k++)
            if (c.shares[k] > 0 || (gen != null && gen[k] > 0))
                bodies[k] = s.dictId(Obs.DICT_BODY, CREATURES.get(k));
        BlockPos centre = new BlockPos(cx * HabitatRecords.CELL_BLOCKS + 32, c.mean + 1, cz * HabitatRecords.CELL_BLOCKS + 32);
        float celsius = Climates.temperature(level, centre);
        if (seasonThisTick == null)
            seasonThisTick = seasonNow(s.server());
        int at = s.boundary().direct(HabitatRecords.HABITAT, HabitatRecords.Habitat.size(n), n, now);
        if (at < 0)
            return false;
        ByteBuffer b = s.boundary().openBuffer();
        b.putInt(at + HabitatRecords.Habitat.DIM, dim);
        b.putInt(at + HabitatRecords.Habitat.FINGERPRINT, c.fingerprint);
        b.putInt(at + HabitatRecords.Habitat.CELL_X, cx);
        b.putInt(at + HabitatRecords.Habitat.CELL_Z, cz);
        b.putShort(at + HabitatRecords.Habitat.KNOWN, (short) 0xFFFF);
        b.putShort(at + HabitatRecords.Habitat.WATER, (short) c.water);
        b.putShort(at + HabitatRecords.Habitat.ICE, (short) c.ice);
        b.putShort(at + HabitatRecords.Habitat.FLUID, (short) c.fluid);
        b.putInt(at + HabitatRecords.Habitat.BIOME, biome);
        b.putInt(at + HabitatRecords.Habitat.LAND_BIOME, land);
        b.putInt(at + HabitatRecords.Habitat.TAGS, c.tags);
        b.putInt(at + HabitatRecords.Habitat.LAND_TAGS, c.landBiome == null ? 0 : c.landBiome.tags());
        b.putShort(at + HabitatRecords.Habitat.SURFACE_MIN, c.min);
        b.putShort(at + HabitatRecords.Habitat.SURFACE_MAX, c.max);
        b.putShort(at + HabitatRecords.Habitat.SURFACE_MEAN, c.mean);
        b.putShort(at + HabitatRecords.Habitat.WATER_MEAN, c.waterMean);
        b.putShort(at + HabitatRecords.Habitat.FLOOR_MEAN, c.floorMean);
        b.putFloat(at + HabitatRecords.Habitat.TEMPERATURE, c.temperature);
        b.putFloat(at + HabitatRecords.Habitat.DOWNFALL, c.downfall);
        b.putFloat(at + HabitatRecords.Habitat.CELSIUS, celsius);
        b.put(at + HabitatRecords.Habitat.SEASON, (byte) seasonThisTick[0]);
        b.put(at + HabitatRecords.Habitat.SEASON_COUNT, (byte) seasonThisTick[1]);
        int e = at + HabitatRecords.Habitat.CREATURES;
        for (int k = 0; k < CREATURES.size(); k++) {
            if (bodies[k] == 0)
                continue;
            b.putInt(e + HabitatRecords.Habitat.C_BODY, bodies[k]);
            b.putShort(e + HabitatRecords.Habitat.C_SHARE, (short) Math.min(c.shares[k], 0xFFFF));
            b.putShort(e + HabitatRecords.Habitat.C_GENERATED, (short) (gen == null ? 0 : Math.min(gen[k], 0xFFFF)));
            e += HabitatRecords.Habitat.CREATURE;
        }
        return true;
    }

    /** {season, count} for the record: a climate plugin's, clamped to a byte each; {0, 0} without seasons. */
    private static long[] seasonNow(MinecraftServer server) {
        long[] c = Climates.season(server);
        if (c == null)
            return new long[]{0, 0};
        long count = Math.min(Math.max(c[1], 0), 255);
        return new long[]{count == 0 ? 0 : Math.floorMod(c[0], count), count};
    }

    /** A one-line summary for logs and {@code /rtsim perf}. */
    public String summary() {
        double perCell = cellsSampled == 0 ? 0 : sampleNanos / 1000.0 / cellsSampled;
        return String.format("habitat: %s, %d cells sampled (%.1f us each, worst %.1f us), %d sent, %d unchanged, "
                        + "%d incomplete, %d dropped, %d queued, %d chunk loads (hook %.2f us each), %d known from the engine, "
                        + "%d chunk-generation animals",
                synced ? "synced" : "waiting for the engine's list", cellsSampled, perCell, maxCellNanos / 1000.0,
                cellsSent, cellsUnchanged, cellsIncomplete, recordsDropped, queued(), chunkLoads,
                chunkLoads == 0 ? 0 : loadHookNanos / 1000.0 / chunkLoads, knownCells, generatedAnimals);
    }

    static void logSummary() {
        MicRtsim.LOGGER.info("[mic_rtsim] {}", INSTANCE.summary());
    }
}
