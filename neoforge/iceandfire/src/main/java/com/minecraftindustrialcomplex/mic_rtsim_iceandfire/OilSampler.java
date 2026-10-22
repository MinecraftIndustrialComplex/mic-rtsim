package com.minecraftindustrialcomplex.mic_rtsim_iceandfire;

import com.minecraftindustrialcomplex.mic_rtsim.api.OilSource;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.oil.OilSources;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.chunk.LevelChunk;

import java.nio.ByteBuffer;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentLinkedQueue;

/**
 * Where the oil is, for the engine (OILGATE.md §4.1, OG-2): OIL_CHUNKS from the registered oil sources
 * ({@link OilSources}; Create: Diesel Generators' plugin registers its {@code getBaseOilAmount}, this
 * plugin never names the mod). Server thread, except {@link #loaded}.
 *
 * <ul>
 *   <li><b>New chunks</b>: each chunk as it first generates is sampled once, on the server thread at the
 *   end of a tick.</li>
 *   <li><b>Surveys</b>: an OIL_SURVEY from the engine (a new fire lair asks for the oil within its reach)
 *   covers every chunk within its radius, nearest first: a chunk in memory is sampled now, any other the
 *   first time it loads this session (choice OGC43). A sample of a chunk not in memory reads the biome
 *   source, which on a real overworld costs Create: Diesel Generators about 80 ms a chunk, and a lair's
 *   survey covers up to 13,000 chunks; a loaded one costs about 3 ms.</li>
 *   <li><b>Deep Time levels</b>: their oil traps' chunks go out once a session (source 1), before any
 *   sampling; the seam is reflective and UNTESTED (no GameTest server loads Deep Time).</li>
 * </ul>
 *
 * <p>New chunks and surveys share one budget a tick: {@link DragonConfig#samplesPerTick} samples, and no
 * more once {@link #NANOS_PER_TICK} has gone on them (both PLACEHOLDER). Only chunks with oil are sent
 * (amount above zero; {@link Integer#MAX_VALUE} is sent as flag 1, bottomless), at most 1,024 to a record,
 * at the end of the tick. A chunk is sampled at most once a session.
 */
public final class OilSampler {
    /**
     * PLACEHOLDER: the time sampling may take in one tick (nanoseconds); one sample always runs, so a tick
     * with work pays at least one source call (about 3 ms for Create: Diesel Generators on a loaded chunk
     * with an oil biome; far less for one its first look skips, OGC45).
     */
    public static final long NANOS_PER_TICK = 2_000_000L;
    /** Surveys wider than this are clamped (the engine asks for 1,024). */
    public static final int MAX_SURVEY_RADIUS = 1024;

    private final DragonBridge bridge;

    private record NewChunk(ResourceKey<Level> dimension, long pos, boolean deferred) {}

    private final ConcurrentLinkedQueue<NewChunk> newChunks = new ConcurrentLinkedQueue<>();

    /** One survey under way: its chunks, nearest its point first. */
    private static final class Survey {
        final ServerLevel level;
        final long[] chunks;
        int next;
        final long started;

        Survey(ServerLevel level, long[] chunks, long started) {
            this.level = level;
            this.chunks = chunks;
            this.started = started;
        }
    }

    private final ArrayDeque<Survey> surveys = new ArrayDeque<>();
    private final Map<ResourceKey<Level>, LongOpenHashSet> sampled = new HashMap<>();
    /** Chunks a survey covered while they were not in memory: sampled when they next load (any thread adds). */
    private final Map<ResourceKey<Level>, Set<Long>> deferred = new ConcurrentHashMap<>();
    private record Key(ResourceKey<Level> dimension, int source) {}
    private final Map<Key, List<int[]>> found = new LinkedHashMap<>();
    private boolean deepTimeDone;

    /**
     * The cost of one sample (OILGATE.md §7 test 4), by whether its chunk was in memory with its biomes
     * ("generated") or not ("ungenerated": never generated, or saved and unloaded, which reads the same
     * biome source).
     */
    public static final class Cost {
        public long samples, nanos, maxNanos;

        void add(long n) {
            samples++;
            nanos += n;
            maxNanos = Math.max(maxNanos, n);
        }

        public double meanMicros() {
            return samples == 0 ? 0 : nanos / 1000.0 / samples;
        }

        @Override
        public String toString() {
            return samples + " samples, mean " + String.format("%.1f", meanMicros()) + " us, max " + maxNanos / 1000 + " us";
        }
    }

    public final Cost generated = new Cost(), ungenerated = new Cost();
    public long newSampled, surveySampled, withOil, bottomless, records, surveysAsked, surveysDone, surveysRefused,
            noSource, repeats, deepTimeChunks, ticksOverBudget, surveyDeferred, deferredSampled;

    OilSampler(DragonBridge bridge) {
        this.bridge = bridge;
    }

    void clear() {
        newChunks.clear();
        surveys.clear();
        sampled.clear();
        deferred.clear();
        found.clear();
        deepTimeDone = false;
        newSampled = surveySampled = withOil = bottomless = records = surveysAsked = surveysDone = surveysRefused = noSource = repeats
                = deepTimeChunks = ticksOverBudget = surveyDeferred = deferredSampled = 0;
        generated.samples = generated.nanos = generated.maxNanos = 0;
        ungenerated.samples = ungenerated.nanos = ungenerated.maxNanos = 0;
    }

    /**
     * ChunkEvent.Load (any thread): a chunk generated just now, or one a survey deferred, is queued for its
     * sample.
     */
    void loaded(LevelChunk chunk, boolean newChunk) {
        if (!(chunk.getLevel() instanceof ServerLevel level))
            return;
        long pos = chunk.getPos().toLong();
        Set<Long> waiting = deferred.get(level.dimension());
        boolean wasDeferred = waiting != null && waiting.remove(pos);
        if (wasDeferred || (newChunk && DragonConfig.sampleNewChunks()))
            newChunks.add(new NewChunk(level.dimension(), pos, wasDeferred));
    }

    /** OIL_SURVEY from the engine. */
    void survey(String dimension, int x, int z, int radius, long now) {
        surveysAsked++;
        ServerLevel level = level(dimension);
        if (!DragonConfig.answerSurveys() || level == null || radius < 0) {
            surveysRefused++;
            return;
        }
        if (OilSources.forLevel(level) == null) {
            noSource++;
            return;
        }
        surveys.add(new Survey(level, chunksWithin(x, z, Math.min(radius, MAX_SURVEY_RADIUS)), now));
    }

    /** The chunks whose centre lies within {@code radius} blocks of (x, z), nearest first, packed. */
    public static long[] chunksWithin(int x, int z, int radius) {
        int cx = x >> 4, cz = z >> 4, reach = (radius >> 4) + 1;
        List<long[]> c = new ArrayList<>();
        long r2 = (long) radius * radius;
        for (int dx = -reach; dx <= reach; dx++)
            for (int dz = -reach; dz <= reach; dz++) {
                ChunkPos p = new ChunkPos(cx + dx, cz + dz);
                long ex = p.getMiddleBlockX() - x, ez = p.getMiddleBlockZ() - z, d2 = ex * ex + ez * ez;
                if (d2 <= r2 || (dx == 0 && dz == 0))
                    c.add(new long[] {d2, p.toLong()});
            }
        c.sort((a, b) -> Long.compare(a[0], b[0]));
        long[] out = new long[c.size()];
        for (int i = 0; i < out.length; i++)
            out[i] = c.get(i)[1];
        return out;
    }

    private ServerLevel level(String dimension) {
        ResourceLocation loc = dimension == null ? null : ResourceLocation.tryParse(dimension);
        return loc == null ? null : bridge.session().server().getLevel(ResourceKey.create(Registries.DIMENSION, loc));
    }

    /** End of tick: Deep Time's traps once, then new chunks and surveys under the budget; then OIL_CHUNKS. */
    void tick(long now) {
        RtsimSession s = bridge.session();
        if (!RtsimConfig.on(Module.ECOLOGY))
            return;
        if (!deepTimeDone) {
            deepTimeDone = true;
            for (ServerLevel level : s.server().getAllLevels())
                for (ChunkPos p : OilSources.deepTimeTrapChunks(level)) {
                    OilSource src = OilSources.forLevel(level);
                    int amount = src == null ? 1 : src.baseOil(level, p);
                    if (amount > 0 && seen(level, p.toLong())) {
                        add(level.dimension(), OilSource.DEEP_TIME, p, amount);
                        deepTimeChunks++;
                    }
                }
        }
        int budget = DragonConfig.samplesPerTick();
        long start = System.nanoTime();
        boolean any = false;
        while (budget > 0 && (!any || System.nanoTime() - start < NANOS_PER_TICK)) {
            NewChunk n = newChunks.poll();
            if (n == null)
                break;
            ServerLevel level = s.server().getLevel(n.dimension());
            if (level == null || isSampled(level, n.pos()))
                continue;
            if (!inMemory(level, n.pos())) {
                // Unloaded again before its sample: it waits for its next load (OGC43).
                defer(level, n.pos());
                continue;
            }
            seen(level, n.pos());
            sample(level, new ChunkPos(n.pos()), true);
            if (n.deferred())
                deferredSampled++;
            else
                newSampled++;
            budget--;
            any = true;
        }
        while (budget > 0 && !surveys.isEmpty() && (!any || System.nanoTime() - start < NANOS_PER_TICK)) {
            Survey v = surveys.peek();
            if (v.next >= v.chunks.length) {
                surveys.poll();
                surveysDone++;
                continue;
            }
            long pos = v.chunks[v.next++];
            if (isSampled(v.level, pos)) {
                repeats++;
                continue;
            }
            if (!inMemory(v.level, pos)) {
                // Never read the biome source of a chunk not in memory (OGC43): it is sampled when it loads.
                defer(v.level, pos);
                surveyDeferred++;
                any = true; // the time budget bounds a run of deferrals too
                continue;
            }
            seen(v.level, pos);
            sample(v.level, new ChunkPos(pos), false);
            surveySampled++;
            budget--;
            any = true;
        }
        if (any && System.nanoTime() - start >= NANOS_PER_TICK)
            ticksOverBudget++;
        flush(now);
    }

    /** True the first time a chunk is met this session. */
    private boolean seen(ServerLevel level, long pos) {
        return sampled.computeIfAbsent(level.dimension(), k -> new LongOpenHashSet()).add(pos);
    }

    private boolean isSampled(ServerLevel level, long pos) {
        LongOpenHashSet done = sampled.get(level.dimension());
        return done != null && done.contains(pos);
    }

    /** A full chunk in memory, asked without blocking (server thread). */
    private static boolean inMemory(ServerLevel level, long pos) {
        return level.getChunkSource().getChunkNow(ChunkPos.getX(pos), ChunkPos.getZ(pos)) != null;
    }

    private void defer(ServerLevel level, long pos) {
        deferred.computeIfAbsent(level.dimension(), k -> ConcurrentHashMap.newKeySet()).add(pos);
        // A load between the check and the add would be missed: look again now it is noted.
        if (inMemory(level, pos) && deferred.get(level.dimension()).remove(pos))
            newChunks.add(new NewChunk(level.dimension(), pos, true));
    }

    private void sample(ServerLevel level, ChunkPos p, boolean fresh) {
        OilSource src = OilSources.forLevel(level);
        if (src == null) {
            noSource++;
            return;
        }
        // In memory with its biomes (what LevelReader.getNoiseBiome reads), or not: a chunk saved but
        // unloaded costs what an ungenerated one does (both read the biome source). Since OGC43 only chunks
        // in memory are sampled, so "ungenerated" stays at zero: the GameTests check that it does.
        boolean wasGenerated = fresh || inMemory(level, p.toLong());
        long t0 = System.nanoTime();
        int amount = src.baseOil(level, p);
        long dt = System.nanoTime() - t0;
        (wasGenerated ? generated : ungenerated).add(dt);
        if (amount > 0)
            add(level.dimension(), src.source(), p, amount);
    }

    private void add(ResourceKey<Level> dimension, int source, ChunkPos p, int amount) {
        withOil++;
        int flags = 0;
        if (amount == Integer.MAX_VALUE) {
            flags = DragonRecords.OilChunks.FLAG_BOTTOMLESS;
            bottomless++;
        }
        found.computeIfAbsent(new Key(dimension, source), k -> new ArrayList<>()).add(new int[] {p.x, p.z, amount, flags});
    }

    /** OIL_CHUNKS for what this tick found, at most {@link DragonRecords.OilChunks#MAX} to a record. */
    private void flush(long now) {
        if (found.isEmpty())
            return;
        RtsimSession s = bridge.session();
        for (Map.Entry<Key, List<int[]>> e : found.entrySet()) {
            List<int[]> all = e.getValue();
            int dim = s.dimId(e.getKey().dimension());
            for (int from = 0; from < all.size(); from += DragonRecords.OilChunks.MAX) {
                int n = Math.min(DragonRecords.OilChunks.MAX, all.size() - from);
                ByteBuffer r = s.boundary().beginJournaled(DragonRecords.OIL_CHUNKS, DragonRecords.OilChunks.size(n), n, now);
                r.putInt(DragonRecords.OilChunks.DIM, dim);
                r.putInt(DragonRecords.OilChunks.SOURCE, e.getKey().source());
                for (int k = 0; k < n; k++) {
                    int[] c = all.get(from + k);
                    int o = DragonRecords.OilChunks.ENTRIES + DragonRecords.OilChunks.ENTRY * k;
                    r.putInt(o + DragonRecords.OilChunks.E_CX, c[0]);
                    r.putInt(o + DragonRecords.OilChunks.E_CZ, c[1]);
                    r.putInt(o + DragonRecords.OilChunks.E_AMOUNT, c[2]);
                    r.putInt(o + DragonRecords.OilChunks.E_FLAGS, c[3]);
                }
                s.boundary().commit(r);
                records++;
            }
        }
        found.clear();
    }

    /** Surveys still under way. */
    public int surveysPending() {
        return surveys.size();
    }

    /** New chunks waiting for their sample. */
    public int newChunksPending() {
        return newChunks.size();
    }

    /** Chunks a survey covered that wait for their load, in one dimension. */
    public int deferredPending(ResourceKey<Level> dimension) {
        Set<Long> d = deferred.get(dimension);
        return d == null ? 0 : d.size();
    }
}
