package com.minecraftindustrialcomplex.mic_rtsim.industry.glass;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Districts;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryConfig;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryPlugin;
import it.unimi.dsi.fastutil.longs.Long2LongOpenHashMap;
import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.longs.LongArraySet;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.fluids.FluidStack;
import net.neoforged.neoforge.fluids.capability.IFluidHandler;
import net.neoforged.neoforge.items.IItemHandler;

import java.nio.ByteBuffer;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;

/**
 * The glass box's compiler (GLASSBOX.md §1.4, §1.5; docs/industry-boundary.md §9; G1 read only). For each
 * district it extracts a graph through the plugins' {@link GlassExtractor}s and sends it to the engine as
 * latest-wins records with a compile number and a count, as the sampler sends sweeps (F5):
 * NODE, EDGE, RECIPE and DOMAIN records, then IND_GRAPH_END with the exact record count.
 *
 * <p><b>When.</b> A district compiles when first seen (each session), on every IND_EDIT, on a new revision,
 * and when its <b>fingerprint</b> changes: a 64-bit hash over each block entity's position, type, block and
 * compile-relevant configuration ({@link GlassExtractor#fingerprint}), recomputed round-robin one chunk at a
 * time once per sweep period (G-B3). It compiles only while every chunk of the district is block-ticking,
 * and a pass hashes only block-ticking chunks: in a chunk that is loaded but not ticking (a view-distance
 * border) the block entities have not run their first tick, so a generator has no network and a deployer
 * no player yet, and the graph and hash would differ from the running factory's (choice G2-63).
 *
 * <p><b>Slicing.</b> Extraction runs under {@code glass.compileSliceMicros} per tick, one district at a
 * time; an edit, a new revision, an unloaded chunk or a chunk whose hash changed under it restarts it.
 * Sending runs under {@code glass.sendBytesPerTick}. The last compile is resent every
 * {@code glass.resendTicks} with the RESEND flag (GB4), sooner when a record was dropped.
 *
 * <p>Also: IND_CHUNK_TICKING (journaled) on every change of a district chunk's block-ticking state
 * ({@code shouldTickBlocksAt}, not merely loaded), and IND_PROBE_SET's probes, read at the end of each of
 * the district's sweeps as IND_PROBE_SAMPLE (not counted in the sweep, GB3).
 */
public final class GlassCompiler {

    /** Restarts after which a compile gives up as UNSTABLE (its fingerprint keeps changing). */
    static final int MAX_RESTARTS = 8;

    private final Industry ind;
    private final Map<Long, State> states = new HashMap<>();
    private final ArrayDeque<State> fingerprintQueue = new ArrayDeque<>();
    /** Districts waiting for a compile, oldest first (a district not fully loaded goes to the back). */
    private final ArrayDeque<State> dirtyQueue = new ArrayDeque<>();
    private boolean started;
    private final ArrayDeque<Send> sends = new ArrayDeque<>();
    private Job job;
    private long ticks;

    // Counters for the GameTests and /rtsim district perf (measured, not tuned).
    long compiles;
    long restarts;
    long recordsSent;
    long recordsDropped;
    long compileNanos;
    long lastCompileNanos;
    long fingerprintNanos;
    long chunkTickingSent;
    long probeSamplesSent;

    public GlassCompiler(Industry ind) {
        this.ind = ind;
    }

    /** One district's compile state, this session. */
    static final class State {
        final long district;
        int compileNo;
        boolean dirty = true;
        int restarts;
        long notBefore;
        // The fingerprint: per chunk hashes of the last pass, and of the compile in force.
        final Long2LongOpenHashMap passHash = new Long2LongOpenHashMap();
        Long2LongOpenHashMap compiledHash = new Long2LongOpenHashMap();
        long fingerprint;
        long nextPassAt;
        long[] passChunks;
        int passCursor = -1;
        boolean queued;
        boolean inDirtyQueue;
        // The last complete compile, for resends and probes.
        List<GraphBuilder.Encoded> records;
        GraphBuilder.Encoded end;
        long resendAt = Long.MAX_VALUE;
        long[] nodePos;
        /** The node numbering of the latest compile (IND_COUNT_SET is read in it), and of the one whose END was last sent (IND_COUNT_SAMPLE is written in it). */
        Numbering numbering, sent;
        // IND_PROBE_SET: the compile it named and its nodes.
        int probeCompile;
        int[] probeNodes = new int[0];
        // IND_CHUNK_TICKING: chunks last reported ticking.
        final LongOpenHashSet ticking = new LongOpenHashSet();

        State(long district) {
            this.district = district;
        }
    }

    /** A compile being extracted. */
    private final class Job {
        final State st;
        final District d;
        final ServerLevel level;
        final GraphBuilder g;
        final List<GlassExtractor> extractors = new ArrayList<>();
        final List<GlassExtractor.Compile> compiles = new ArrayList<>();
        final long[] chunks;
        final Long2LongOpenHashMap chunkHash = new Long2LongOpenHashMap();
        final int revision;
        final long startTick;
        int chunkCursor;
        List<BlockEntity> pending;
        int beCursor;
        long chunkAcc;
        boolean visiting = true;
        long nanos;

        Job(State st, District d, ServerLevel level) {
            this.st = st;
            this.d = d;
            this.level = level;
            this.g = new GraphBuilder(ind, level, d);
            this.chunks = d.chunks.toLongArray();
            java.util.Arrays.sort(chunks);
            this.revision = d.revision;
            this.startTick = ind.gameTime();
            for (IndustryPlugin p : IndustryApi.plugins()) {
                GlassExtractor x = p.glass();
                if (x != null) {
                    extractors.add(x);
                    compiles.add(x.begin(g));
                }
            }
        }
    }

    /** A compile (or a resend) going out over ticks. */
    private static final class Send {
        final State st;
        final List<GraphBuilder.Encoded> records;
        final GraphBuilder.Encoded end;
        final boolean resend;
        /** The numbering of the compile these records are: what the engine holds once the END is written. */
        final Numbering numbering;
        int cursor;
        boolean dropped;

        Send(State st, List<GraphBuilder.Encoded> records, GraphBuilder.Encoded end, boolean resend, Numbering numbering) {
            this.st = st;
            this.records = records;
            this.end = end;
            this.resend = resend;
            this.numbering = numbering;
        }
    }

    // ---- hooks from the industry module ---------------------------------------------------------

    private State state(District d) {
        State st = states.get(d.id);
        if (st == null) {
            st = new State(d.id);
            states.put(d.id, st);
            markDirty(st);
        }
        return st;
    }

    private void markDirty(State st) {
        st.dirty = true;
        if (!st.inDirtyQueue) {
            st.inDirtyQueue = true;
            dirtyQueue.add(st);
        }
    }

    /** IND_EDIT in {@code d} (a player's block event or a command): recompile. */
    public void edited(District d) {
        State st = state(d);
        markDirty(st);
        st.restarts = 0;
        if (job != null && job.st == st)
            abort("edited");
    }

    /** A new revision of {@code d}, or its removal. */
    public void revisionChanged(District d) {
        if (ind.district(d.id) != d) {
            State st = states.remove(d.id);
            setReached(d.id, null, null);
            ind.counting().drop(d.id);
            if (st != null) {
                if (job != null && job.st == st)
                    abort("removed");
                sends.removeIf(s -> s.st == st);
                fingerprintQueue.remove(st);
                dirtyQueue.remove(st);
            }
            return;
        }
        State st = state(d);
        markDirty(st);
        if (job != null && job.st == st)
            abort("new revision");
    }

    /** The module came back on (U10): resend every compile and report every ticking chunk again. */
    public void forgetAnnounced() {
        for (State st : states.values()) {
            st.ticking.clear();
            if (st.end != null)
                st.resendAt = 0;
        }
    }

    /** Asks for a compile of {@code d} now (commands, GameTests). */
    public void requestCompile(District d) {
        State st = state(d);
        markDirty(st);
        st.notBefore = 0;
        st.restarts = 0;
    }

    /** The last compile number sent for a district (0: none yet). */
    public int compileNo(long district) {
        State st = states.get(district);
        return st == null || st.records == null ? 0 : st.compileNo;
    }

    /** True while a district waits for, or runs, a compile. */
    public boolean pending(long district) {
        State st = states.get(district);
        return st == null || st.dirty || (job != null && job.st == st) || sends.stream().anyMatch(s -> s.st == st);
    }

    public long fingerprint(long district) {
        State st = states.get(district);
        return st == null ? 0 : st.fingerprint;
    }

    public long compiles() {
        return compiles;
    }

    public long restarts() {
        return restarts;
    }

    public long lastCompileNanos() {
        return lastCompileNanos;
    }

    public long recordsDropped() {
        return recordsDropped;
    }

    // ---- the tick -------------------------------------------------------------------------------

    public void tick(long now) {
        if (!GlassConfig.enabled())
            return;
        ticks++;
        if (!started) {
            // Every district is new to this session: each compiles once its chunks are all loaded.
            started = true;
            for (District d : ind.districts().all())
                state(d);
        }
        chunkTicking(now);
        fingerprints(now);
        compile(now);
        send(now);
        resends(now);
    }

    private void compile(long now) {
        long budget = GlassConfig.get(GlassConfig.COMPILE_SLICE_MICROS) * 1000L;
        long t0 = System.nanoTime();
        long deadline = t0 + budget;
        if (job == null)
            job = next(now);
        if (job == null)
            return;
        Job j = job;
        try {
            District d = ind.district(j.st.district);
            if (d != j.d || d.revision != j.revision) {
                abort("revision");
                return;
            }
            if (!fullyLoaded(j.level, d)) {
                abort("a chunk unloaded or stopped ticking");
                return;
            }
            while (j.visiting && System.nanoTime() < deadline) {
                if (!visitSome(j, deadline)) {
                    j.visiting = false;
                    j.g.groupResiduals();
                }
            }
            if (!j.visiting && System.nanoTime() < deadline) {
                boolean done = true;
                for (GlassExtractor.Compile c : j.compiles)
                    done &= c.finish(deadline);
                if (done)
                    complete(j, now);
            }
        } catch (RuntimeException e) {
            MicRtsim.LOGGER.error("[mic_rtsim] glass: compiling district {} failed", j.st.district, e);
            j.g.notCompilable(GlassRecords.End.REASON_OTHER, e.toString());
            complete(j, now);
        } finally {
            long dt = System.nanoTime() - t0;
            j.nanos += dt;
            compileNanos += dt;
        }
    }

    /** The next dirty district that is fully loaded, as a new job; null when none. */
    private Job next(long now) {
        // At most 64 looks a tick: a district that is not ready goes to the back of the queue.
        for (int looks = Math.min(64, dirtyQueue.size()); looks > 0; looks--) {
            State st = dirtyQueue.poll();
            if (!st.dirty || states.get(st.district) != st) {
                st.inDirtyQueue = false;
                continue;
            }
            District d = ind.district(st.district);
            if (d == null) {
                st.inDirtyQueue = false;
                continue;
            }
            ServerLevel level = ind.level(d.dim);
            if (now < st.notBefore || level == null || d.chunks.isEmpty() || !fullyLoaded(level, d)) {
                dirtyQueue.add(st);
                continue;
            }
            st.inDirtyQueue = false;
            st.dirty = false;
            sends.removeIf(s -> s.st == st && !s.resend);
            return new Job(st, d, level);
        }
        return null;
    }

    /** Every chunk of {@code d} loaded and block-ticking (choice G2-63). */
    private boolean fullyLoaded(ServerLevel level, District d) {
        if (ind.districts().loadedChunks(d) < d.chunks.size())
            return false;
        for (long c : d.chunks)
            if (!Districts.blockTicking(level, c))
                return false;
        return true;
    }

    /** Visits block entities until the deadline; false when every chunk is done. */
    private boolean visitSome(Job j, long deadline) {
        int n = 0;
        while (true) {
            if (j.pending == null) {
                if (j.chunkCursor >= j.chunks.length)
                    return false;
                long c = j.chunks[j.chunkCursor];
                LevelChunk chunk = j.level.getChunkSource().getChunkNow(ChunkPos.getX(c), ChunkPos.getZ(c));
                j.pending = chunk == null ? List.of() : sortedBlockEntities(chunk);
                j.beCursor = 0;
                j.chunkAcc = 0;
            }
            if (j.beCursor < j.pending.size()) {
                BlockEntity be = j.pending.get(j.beCursor++);
                if (be.isRemoved())
                    continue;
                j.chunkAcc += hash(be);
                boolean claimed = false;
                for (GlassExtractor.Compile c : j.compiles)
                    if (c.visit(be)) {
                        claimed = true;
                        break;
                    }
                if (!claimed && j.g.inventoryAt(be.getBlockPos()) == null && hasCapability(j.level, be))
                    j.g.addResidual(be);
                if ((++n & 7) == 0 && System.nanoTime() >= deadline)
                    return true;
                continue;
            }
            j.chunkHash.put(j.chunks[j.chunkCursor], j.chunkAcc);
            j.chunkCursor++;
            j.pending = null;
        }
    }

    private static List<BlockEntity> sortedBlockEntities(LevelChunk chunk) {
        List<BlockEntity> list = new ArrayList<>(chunk.getBlockEntities().values());
        list.sort((a, b) -> Long.compare(a.getBlockPos().asLong(), b.getBlockPos().asLong()));
        return list;
    }

    private static boolean hasCapability(ServerLevel level, BlockEntity be) {
        BlockPos p = be.getBlockPos();
        BlockState s = be.getBlockState();
        if (level.getCapability(Capabilities.ItemHandler.BLOCK, p, s, be, null) != null
                || level.getCapability(Capabilities.FluidHandler.BLOCK, p, s, be, null) != null
                || level.getCapability(Capabilities.EnergyStorage.BLOCK, p, s, be, null) != null)
            return true;
        for (Direction d : Direction.values())
            if (level.getCapability(Capabilities.ItemHandler.BLOCK, p, s, be, d) != null
                    || level.getCapability(Capabilities.FluidHandler.BLOCK, p, s, be, d) != null)
                return true;
        return false;
    }

    private void abort(String why) {
        if (job == null)
            return;
        State st = job.st;
        markDirty(st);
        st.restarts++;
        restarts++;
        MicRtsim.LOGGER.debug("[mic_rtsim] glass: compile of district {} restarts ({})", st.district, why);
        Job j = job;
        job = null;
        if (st.restarts > MAX_RESTARTS) {
            // The district keeps changing under the compile: say so, and try again much later.
            st.restarts = 0;
            st.notBefore = ind.gameTime() + GlassConfig.get(GlassConfig.RESEND_TICKS);
            j.g.notCompilable(GlassRecords.End.REASON_UNSTABLE, "fingerprint changed " + MAX_RESTARTS + " times");
            finishNotCompilable(j, ind.gameTime());
        }
    }

    private void complete(Job j, long now) {
        job = null;
        State st = j.st;
        GraphBuilder g = j.g;
        if (g.notCompilable == 0) {
            g.completeStores();
            if (g.nodeCount() > GlassConfig.get(GlassConfig.MAX_NODES))
                g.notCompilable(GlassRecords.End.REASON_TOO_BIG, g.nodeCount() + " nodes");
            String bad = g.notCompilable == 0 ? g.validate() : null;
            if (bad != null) {
                MicRtsim.LOGGER.error("[mic_rtsim] glass: district {} compiled an invalid graph: {}", st.district, bad);
                g.notCompilable(GlassRecords.End.REASON_OTHER, bad);
            }
        }
        if (g.notCompilable != 0) {
            finishNotCompilable(j, now);
            return;
        }
        st.compileNo++;
        List<GraphBuilder.Encoded> records = g.encode(st.district, st.compileNo);
        long fp = 0;
        for (Long2LongOpenHashMap.Entry e : j.chunkHash.long2LongEntrySet())
            fp += chunkMix(e.getLongKey(), e.getLongValue());
        st.compiledHash = new Long2LongOpenHashMap(j.chunkHash);
        st.fingerprint = fp;
        int flags = g.hasResidual() ? GlassRecords.End.BLACK_BOX : GlassRecords.End.COMPLETE;
        int compileTicks = (int) Math.min(Integer.MAX_VALUE, Math.max(0, now - j.startTick));
        st.end = end(st, flags, records.size(), g.nodeCount(), g.edges().size(), g.recipeCount(), g.domains().size(),
                GlassRecords.End.REASON_NONE, fp, g.frame, j.revision, compileTicks);
        st.records = records;
        st.nodePos = new long[g.nodeCount()];
        boolean[] store = new boolean[g.nodeCount()];
        for (GraphBuilder.Node n : g.nodes()) {
            st.nodePos[n.index] = n.pos().asLong();
            store[n.index] = n.kind == GlassRecords.Node.STORE;
        }
        st.numbering = new Numbering(st.compileNo, st.nodePos, store, g.frame);
        st.restarts = 0;
        st.probeNodes = new int[0];
        sends.add(new Send(st, records, st.end, false, st.numbering));
        compiles++;
        lastCompileNanos = j.nanos;
        lastGraph.put(st.district, g);
        setReached(st.district, j.level.dimension(), g.reachedChunks);
        if (MicRtsim.LOGGER.isDebugEnabled())
            MicRtsim.LOGGER.debug("[mic_rtsim] glass: district {} compile {}: {} nodes, {} edges, {} recipes, {} domains{}",
                    st.district, st.compileNo, g.nodeCount(), g.edges().size(), g.recipeCount(), g.domains().size(),
                    g.hasResidual() ? ", residual of " + g.residualPositions() + " block entities in " + g.residualGroups() + " groups ("
                            + g.residualKinds(8) + ")" : "");
    }

    private void finishNotCompilable(Job j, long now) {
        State st = j.st;
        st.compileNo++;
        MicRtsim.LOGGER.warn("[mic_rtsim] glass: district {} is not compilable (reason {}): {}", st.district, j.g.notCompilable, j.g.notCompilableWhy);
        st.records = List.of();
        st.nodePos = new long[0];
        st.numbering = new Numbering(st.compileNo, st.nodePos, new boolean[0], j.g.frame);
        st.end = end(st, GlassRecords.End.NOT_COMPILABLE, 0, 0, 0, 0, 0, j.g.notCompilable, 0, j.g.frame, j.revision,
                (int) Math.max(0, now - j.startTick));
        st.compiledHash = new Long2LongOpenHashMap(j.chunkHash);
        sends.add(new Send(st, st.records, st.end, false, st.numbering));
        lastGraph.remove(st.district);
        // the last compile's reached chunks stay (SC77): an edit there still asks this district to recompile
    }

    private GraphBuilder.Encoded end(State st, int flags, int records, int nodes, int edges, int recipes, int domains, int reason,
            long fingerprint, long frame, int revision, int compileTicks) {
        ByteBuffer b = ByteBuffer.allocate(GlassRecords.End.SIZE).order(com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout.ORDER);
        b.putLong(GlassRecords.DISTRICT, st.district);
        b.putInt(GlassRecords.COMPILE, st.compileNo);
        b.putInt(GlassRecords.End.WINDOW, GlassConfig.get(GlassConfig.CHECKSUM_WINDOW_TICKS));
        b.putInt(GlassRecords.End.RECORDS, records);
        b.putInt(GlassRecords.End.NODES, nodes);
        b.putInt(GlassRecords.End.EDGES, edges);
        b.putInt(GlassRecords.End.RECIPES, recipes);
        b.putInt(GlassRecords.End.DOMAINS, domains);
        b.putInt(GlassRecords.End.REASON, reason);
        b.putLong(GlassRecords.End.FINGERPRINT, fingerprint);
        b.putLong(GlassRecords.End.FRAME, frame);
        b.putInt(GlassRecords.End.REVISION, revision);
        b.putInt(GlassRecords.End.COMPILE_TICKS, compileTicks);
        return new GraphBuilder.Encoded(GlassRecords.IND_GRAPH_END, flags, b.array());
    }

    /**
     * Per dimension and chunk outside a district, the districts whose last compile has a kinetic network
     * reaching it (choice SC77), and per district the chunks it was listed at: a player's edit in such a chunk
     * that no district holds is an edit of those districts too. This session's compiles only; a district's
     * entries go with its next compile that succeeds, or its removal (one that is not compilable keeps them).
     */
    private final Map<ResourceKey<Level>, Long2ObjectOpenHashMap<LongArraySet>> reachedBy = new HashMap<>();
    private final Map<Long, Reached> reached = new HashMap<>();

    private record Reached(ResourceKey<Level> dim, long[] chunks) {}

    /** The districts whose last compile has a kinetic network reaching chunk {@code chunk} of {@code dim} (SC77). */
    public long[] reachedBy(ResourceKey<Level> dim, long chunk) {
        Long2ObjectOpenHashMap<LongArraySet> m = reachedBy.get(dim);
        LongArraySet s = m == null ? null : m.get(chunk);
        return s == null ? new long[0] : s.toLongArray();
    }

    private void setReached(long district, ResourceKey<Level> dim, LongOpenHashSet chunks) {
        Reached old = reached.remove(district);
        if (old != null) {
            Long2ObjectOpenHashMap<LongArraySet> m = reachedBy.get(old.dim);
            for (long c : old.chunks) {
                LongArraySet s = m.get(c);
                if (s != null && s.remove(district) && s.isEmpty())
                    m.remove(c);
            }
        }
        if (chunks == null || chunks.isEmpty())
            return;
        long[] cs = chunks.toLongArray();
        reached.put(district, new Reached(dim, cs));
        Long2ObjectOpenHashMap<LongArraySet> m = reachedBy.computeIfAbsent(dim, k -> new Long2ObjectOpenHashMap<>());
        for (long c : cs)
            m.computeIfAbsent(c, k -> new LongArraySet(2)).add(district);
    }

    /** The last graph compiled per district, this session (GameTests; dropped with the next compile). */
    private final Map<Long, GraphBuilder> lastGraph = new HashMap<>();

    public GraphBuilder lastGraph(long district) {
        return lastGraph.get(district);
    }

    // ---- sending --------------------------------------------------------------------------------

    private void send(long now) {
        int budget = GlassConfig.get(GlassConfig.SEND_BYTES_PER_TICK);
        while (budget > 0 && !sends.isEmpty()) {
            Send s = sends.peek();
            if (s.cursor < s.records.size()) {
                GraphBuilder.Encoded r = s.records.get(s.cursor++);
                budget -= r.bytes().length;
                if (!write(r, now, 0))
                    s.dropped = true;
                continue;
            }
            budget -= s.end.bytes().length;
            int flags = s.end.aux() | (s.resend ? GlassRecords.End.RESEND : 0);
            if (!write(s.end, now, flags))
                s.dropped = true;
            else
                s.st.sent = s.numbering; // the engine now holds this compile: its samples are numbered by it
            sends.poll();
            // A dropped record loses the compile on the engine's side: send it again soon.
            s.st.resendAt = now + (s.dropped ? 20 : GlassConfig.get(GlassConfig.RESEND_TICKS));
        }
    }

    /** Writes one record into the open hand-over; false when it was dropped for space. */
    private boolean write(GraphBuilder.Encoded r, long now, int auxOverride) {
        byte[] bytes = r.bytes();
        int aux = r.kind() == GlassRecords.IND_GRAPH_END ? auxOverride : r.aux();
        int at = ind.latest(r.kind(), bytes.length, aux);
        if (at < 0) {
            recordsDropped++;
            return false;
        }
        ByteBuffer b = ind.open();
        b.put(at + 8, bytes, 8, bytes.length - 8);
        b.putLong(at + GlassRecords.GAME_TIME, now);
        recordsSent++;
        return true;
    }

    private void resends(long now) {
        if ((ticks & 15) != 0)
            return;
        for (State st : states.values()) {
            if (st.end == null || now < st.resendAt || st.dirty || (job != null && job.st == st))
                continue;
            boolean queued = false;
            for (Send s : sends)
                if (s.st == st) {
                    queued = true;
                    break;
                }
            if (queued)
                continue;
            st.resendAt = Long.MAX_VALUE;
            sends.add(new Send(st, st.records, st.end, true, st.numbering));
        }
    }

    // ---- fingerprints (G-B3) --------------------------------------------------------------------

    private final Map<BlockEntityType<?>, Long> typeHash = new HashMap<>();
    /** Per block: its registry key's string hash (a fingerprint pass hashes every block entity of a district). */
    private final it.unimi.dsi.fastutil.objects.Reference2LongOpenHashMap<net.minecraft.world.level.block.Block> blockHash =
            new it.unimi.dsi.fastutil.objects.Reference2LongOpenHashMap<>();

    /** One block entity's part of its chunk's fingerprint: position, type, block, and the plugins' configuration. */
    private long hash(BlockEntity be) {
        long h = mix(be.getBlockPos().asLong());
        net.minecraft.world.level.block.Block block = be.getBlockState().getBlock();
        long bh;
        if (blockHash.containsKey(block)) {
            bh = blockHash.getLong(block);
        } else {
            bh = BuiltInRegistries.BLOCK.getKey(block).toString().hashCode();
            blockHash.put(block, bh);
        }
        h += mix(typeHash.computeIfAbsent(be.getType(), t -> {
            ResourceLocation k = BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(t);
            return k == null ? 0L : (long) k.toString().hashCode() * 0x9E3779B97F4A7C15L;
        }) ^ bh);
        for (IndustryPlugin p : IndustryApi.plugins()) {
            GlassExtractor x = p.glass();
            if (x != null) {
                long c = x.fingerprint(be);
                if (c != 0)
                    h += mix(c ^ 0x5851F42D4C957F2DL);
            }
        }
        return mix(h);
    }

    private static long chunkMix(long chunk, long hash) {
        return mix(chunk * 0x9E3779B97F4A7C15L + hash);
    }

    /** SplitMix64's finalizer. */
    static long mix(long z) {
        z = (z ^ (z >>> 30)) * 0xBF58476D1CE4E5B9L;
        z = (z ^ (z >>> 27)) * 0x94D049BB133111EBL;
        return z ^ (z >>> 31);
    }

    /**
     * Round-robin: districts with a compile in force and every chunk loaded get a pass every sweep period
     * ({@code glass.fingerprintPeriodTicks}); a pass hashes one chunk at a time under
     * {@code glass.fingerprintBlockEntitiesPerTick}. A chunk whose hash differs from the one the compile
     * saw makes the district dirty (and restarts its running compile).
     */
    private void fingerprints(long now) {
        int period = GlassConfig.get(GlassConfig.FINGERPRINT_PERIOD);
        if (period <= 0)
            period = IndustryConfig.get(IndustryConfig.SWEEP_PERIOD);
        if (ticks % 20 == 0)
            for (State st : states.values()) {
                if (st.queued || now < st.nextPassAt || (st.end == null && (job == null || job.st != st)))
                    continue;
                st.queued = true;
                fingerprintQueue.add(st);
            }
        long t0 = System.nanoTime();
        int budget = GlassConfig.get(GlassConfig.FINGERPRINT_PER_TICK);
        while (budget > 0 && !fingerprintQueue.isEmpty()) {
            State st = fingerprintQueue.peek();
            District d = ind.district(st.district);
            ServerLevel level = d == null ? null : ind.level(d.dim);
            if (level == null || ind.districts().loadedChunks(d) == 0) {
                fingerprintQueue.poll();
                st.queued = false;
                st.passCursor = -1;
                st.nextPassAt = now + period;
                continue;
            }
            if (st.passCursor < 0) {
                st.passChunks = d.chunks.toLongArray();
                st.passCursor = 0;
            }
            if (st.passCursor >= st.passChunks.length) {
                fingerprintQueue.poll();
                st.queued = false;
                st.passCursor = -1;
                st.nextPassAt = now + period;
                continue;
            }
            long c = st.passChunks[st.passCursor++];
            LevelChunk chunk = level.getChunkSource().getChunkNow(ChunkPos.getX(c), ChunkPos.getZ(c));
            // Loaded but not ticking: its block entities have not run their first tick (choice G2-63).
            if (chunk == null || !Districts.blockTicking(level, c))
                continue;
            long acc = 0;
            for (BlockEntity be : chunk.getBlockEntities().values()) {
                if (be.isRemoved())
                    continue;
                acc += hash(be);
                budget--;
            }
            budget--;
            st.passHash.put(c, acc);
            Long2LongOpenHashMap ref = job != null && job.st == st && job.chunkHash.containsKey(c) ? job.chunkHash
                    : st.compiledHash.containsKey(c) ? st.compiledHash : null;
            if (ref != null && ref.get(c) != acc) {
                markDirty(st);
                st.restarts = 0;
                if (job != null && job.st == st)
                    abort("fingerprint of chunk " + ChunkPos.getX(c) + ", " + ChunkPos.getZ(c));
            }
        }
        fingerprintNanos += System.nanoTime() - t0;
    }

    // ---- IND_CHUNK_TICKING (I7, §6.1) -----------------------------------------------------------

    private void chunkTicking(long now) {
        int every = GlassConfig.get(GlassConfig.TICKING_CHECK_TICKS);
        for (State st : states.values()) {
            if ((ticks + st.district) % every != 0)
                continue;
            District d = ind.district(st.district);
            if (d == null)
                continue;
            if (ind.districts().loadedChunks(d) == 0 && st.ticking.isEmpty())
                continue;
            ServerLevel level = ind.level(d.dim);
            for (long c : d.chunks) {
                boolean t = level != null && Districts.blockTicking(level, c);
                if (t != st.ticking.contains(c)) {
                    if (t)
                        st.ticking.add(c);
                    else
                        st.ticking.remove(c);
                    journalTicking(d, c, t);
                }
            }
            // Chunks that left the district stop being reported.
            for (Iterator<Long> it = st.ticking.iterator(); it.hasNext(); ) {
                long c = it.next();
                if (!d.chunks.contains(c)) {
                    it.remove();
                    journalTicking(d, c, false);
                }
            }
        }
    }

    private void journalTicking(District d, long chunk, boolean ticking) {
        ByteBuffer r = ind.journal(GlassRecords.IND_CHUNK_TICKING, GlassRecords.ChunkTicking.SIZE, ticking ? 1 : 0);
        r.putLong(GlassRecords.ChunkTicking.DISTRICT, d.id);
        r.putInt(GlassRecords.ChunkTicking.DIM, ind.dimId(d.dim));
        r.putInt(GlassRecords.ChunkTicking.CHUNK_X, ChunkPos.getX(chunk));
        r.putInt(GlassRecords.ChunkTicking.CHUNK_Z, ChunkPos.getZ(chunk));
        ind.commit(r);
        chunkTickingSent++;
    }

    /** The chunks last reported ticking (GameTests). */
    public boolean reportedTicking(long district, long chunk) {
        State st = states.get(district);
        return st != null && st.ticking.contains(chunk);
    }

    // ---- probes (§5.3, IND_PROBE_SET and IND_PROBE_SAMPLE) --------------------------------------

    /** IND_PROBE_SET from the engine: the buffers to read in the district's later sweeps. */
    public void probeSet(ByteBuffer b, int at, int size, int aux) {
        if (size < GlassRecords.ProbeSet.NODES)
            return;
        long district = b.getLong(at + GlassRecords.ProbeSet.DISTRICT);
        int compile = b.getInt(at + GlassRecords.ProbeSet.COMPILE);
        int n = Math.max(0, Math.min(aux, (size - GlassRecords.ProbeSet.NODES) / 4));
        State st = states.get(district);
        if (st == null)
            return;
        int[] nodes = new int[n];
        for (int i = 0; i < n; i++)
            nodes[i] = b.getInt(at + GlassRecords.ProbeSet.NODES + 4 * i);
        st.probeCompile = compile;
        st.probeNodes = nodes;
    }

    // ---- counting at the movers (§4.2, IND_COUNT_SET and IND_COUNT_SAMPLE) ----------------------

    /**
     * IND_COUNT_SET from the engine: the (mover node, store node) pairs of the compile it names that Java
     * counts the moves of. Resolved to block positions with that compile's node numbers and handed to
     * {@link Counting}, which keeps a pair it already holds (its counters and {@code since}) and drops the
     * ones no longer named; an empty set drops the district's counters. Ignored (counted, and the counters
     * left as they are) when the compile is not Java's latest complete one, or sits on a moving frame, whose
     * positions are not the world's, or the set is malformed (shorter than its count says, none of its nodes
     * in the compile).
     */
    public void countSet(ByteBuffer b, int at, int size, int aux) {
        Counting counting = ind.counting();
        // A set that is not what it says (too short for its header or for its count of pairs, a negative count)
        // is a malformed one: counted as ignored, and the district's counters are left as they are.
        if (size < GlassRecords.CountSet.PAIRS || aux < 0
                || (long) aux * GlassRecords.CountSet.PAIR > size - GlassRecords.CountSet.PAIRS) {
            counting.setsIgnored++;
            return;
        }
        long district = b.getLong(at + GlassRecords.CountSet.DISTRICT);
        int compile = b.getInt(at + GlassRecords.CountSet.COMPILE);
        int n = aux;
        State st = states.get(district);
        District d = ind.district(district);
        if (n == 0) {
            counting.setsTaken++;
            counting.empty(district);
            return;
        }
        Numbering num = st == null ? null : st.numbering;
        if (st == null || d == null || num == null || num.compile != compile || st.compileNo != compile || num.frame != 0) {
            counting.setsIgnored++;
            return;
        }
        long[] movers = new long[n], stores = new long[n];
        int k = 0;
        for (int i = 0; i < n; i++) {
            int m = b.getInt(at + GlassRecords.CountSet.PAIRS + GlassRecords.CountSet.PAIR * i + GlassRecords.CountSet.P_MOVER);
            int s = b.getInt(at + GlassRecords.CountSet.PAIRS + GlassRecords.CountSet.PAIR * i + GlassRecords.CountSet.P_STORE);
            if (m < 0 || m >= num.pos.length || s < 0 || s >= num.pos.length)
                continue;
            movers[k] = num.pos[m];
            stores[k] = num.pos[s];
            k++;
        }
        if (k == 0) {
            counting.setsIgnored++; // none of the nodes is in the compile: not the set of this compile
            return;
        }
        counting.apply(district, d.dim, movers, stores, k, ind.gameTime());
    }

    /** Called at the end of each of a district's sweeps: its probes, as IND_PROBE_SAMPLE. */
    public void swept(ServerLevel level, District d, long now, int sweep) {
        State st = states.get(d.id);
        if (st != null && st.sent != null)
            ind.counting().sample(d, st.sent, sweep, now);
        if (st == null || st.probeNodes.length == 0 || st.probeCompile != st.compileNo || st.nodePos == null)
            return;
        for (int node : st.probeNodes) {
            if (node < 0 || node >= st.nodePos.length)
                continue;
            BlockPos pos = BlockPos.of(st.nodePos[node]);
            if (level.getChunkSource().getChunkNow(pos.getX() >> 4, pos.getZ() >> 4) == null)
                continue;
            Industry.GoodsCounter goods = ind.counter();
            IItemHandler items = level.getCapability(Capabilities.ItemHandler.BLOCK, pos, null);
            if (items != null)
                for (int s = 0; s < items.getSlots(); s++) {
                    var stack = items.getStackInSlot(s);
                    if (!stack.isEmpty())
                        ind.countStack(stack, stack.getCount(), goods);
                }
            IFluidHandler fluids = level.getCapability(Capabilities.FluidHandler.BLOCK, pos, null);
            if (fluids != null)
                for (int t = 0; t < fluids.getTanks(); t++) {
                    FluidStack f = fluids.getFluidInTank(t);
                    if (!f.isEmpty())
                        goods.add(ind.goods().name(f), f.getAmount());
                }
            int[] ids = goods.nonZeroIds();
            int count = Math.min(ids.length, (GraphBuilder.MAX_RECORD - GlassRecords.ProbeSample.ENTRIES) / 16);
            int at = ind.latest(GlassRecords.IND_PROBE_SAMPLE, GlassRecords.ProbeSample.size(count), count);
            if (at < 0)
                continue;
            ByteBuffer b = ind.open();
            b.putLong(at + GlassRecords.GAME_TIME, now);
            b.putLong(at + GlassRecords.DISTRICT, d.id);
            b.putInt(at + GlassRecords.ProbeSample.SWEEP, sweep);
            b.putInt(at + GlassRecords.ProbeSample.NODE, node);
            b.putInt(at + GlassRecords.ProbeSample.COMPILE, st.compileNo);
            for (int i = 0; i < count; i++) {
                int e = at + GlassRecords.ProbeSample.ENTRIES + 16 * i;
                b.putInt(e, ids[i]);
                b.putLong(e + 8, goods.get(ids[i]));
            }
            probeSamplesSent++;
        }
    }

    public long probeSamplesSent() {
        return probeSamplesSent;
    }

    public long chunkTickingSent() {
        return chunkTickingSent;
    }
}
