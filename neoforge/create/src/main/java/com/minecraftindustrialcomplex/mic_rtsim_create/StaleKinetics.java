package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim_create.glass.CreateGlassAccess;
import com.minecraftindustrialcomplex.mic_rtsim_create.mixin.KineticNetworkInvoker;
import com.minecraftindustrialcomplex.mic_rtsim_create.mixin.RotationPropagatorInvoker;
import com.simibubi.create.content.kinetics.KineticNetwork;
import com.simibubi.create.content.kinetics.base.IRotate;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import com.simibubi.create.content.kinetics.simpleRelays.SimpleKineticBlockEntity;
import it.unimi.dsi.fastutil.longs.LongLinkedOpenHashSet;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.FullChunkStatus;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.neoforge.event.level.ChunkEvent;
import net.neoforged.neoforge.event.server.ServerStoppedEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;

import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;

/**
 * The re-sync of a part of a kinetic network that comes back stale (SECTIONS.md §2.8, choices SC37 and SC73;
 * PATCHES.md rows 56 and 58). Rotation stops at a chunk that is not loaded ({@code RotationPropagatorMixin}),
 * so a part in such a chunk keeps its last speed while its source, in another chunk that ticks, changes. It
 * comes back FULL in one of two ways, loaded anew or (it dropped below FULL and stayed in memory, ticket level
 * 34 to 44) without loading. Either way the chunk is noted (at its load, or at its demotion) and queued when
 * its holder reports it FULL ({@code ChunkMapPromotionMixin}), and re-synced here at the next server tick's
 * start or end ({@link #chunkFull}), before Create's next propagation can meet the stale part.
 * {@code KineticBlockEntityMixin}'s re-sync at a block entity's first attach stays for a chunk that loaded
 * off the server thread (left out here); in a chunk that is FULL and does not tick nothing attaches, so this
 * is its only re-sync.
 */
public final class StaleKinetics {
    private StaleKinetics() {}

    private static int resyncs;
    /**
     * Chunks that dropped below FULL and are still in memory, or loaded and not yet reported FULL, per level
     * (server thread only).
     */
    private static final Map<ServerLevel, LongOpenHashSet> AWAY = new IdentityHashMap<>();
    /** Chunks of {@link #AWAY} reported FULL, per level, until the next drain (server thread only). */
    private static final Map<ServerLevel, LongLinkedOpenHashSet> FULL_AGAIN = new IdentityHashMap<>();

    /** Re-syncs since the server started (for tests). */
    public static int resyncs() {
        return resyncs;
    }

    static void register() {
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener((ServerTickEvent.Pre e) -> drain());
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener((ServerTickEvent.Post e) -> drain());
        // a chunk that unloads is no longer away; one that loads anew is noted, so that its FULL report queues it
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener((ChunkEvent.Unload e) -> {
            if (serverThread(e) instanceof ServerLevel level && AWAY.get(level) instanceof LongOpenHashSet away)
                away.remove(e.getChunk().getPos().toLong());
        });
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener((ChunkEvent.Load e) -> {
            if (serverThread(e) instanceof ServerLevel level)
                AWAY.computeIfAbsent(level, l -> new LongOpenHashSet()).add(e.getChunk().getPos().toLong());
        });
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener((ServerStoppedEvent e) -> {
            AWAY.clear();
            FULL_AGAIN.clear();
        });
    }

    /**
     * The event's level when it is a server level and the event runs on its server's thread; null otherwise.
     * The maps are the server thread's: an event another thread posts (a chunk system that loads off it) is
     * left out, which at worst leaves a chunk that loaded there to {@code KineticBlockEntityMixin}'s re-sync
     * at the first attach, or costs one spurious {@link #chunkFull} pass over a chunk that unloaded and loaded.
     */
    private static ServerLevel serverThread(ChunkEvent e) {
        return e.getLevel() instanceof ServerLevel level && level.getServer().isSameThread() ? level : null;
    }

    /**
     * {@code ChunkMap.onFullChunkStatusChange}: a chunk below FULL is noted away until it unloads; one that is
     * FULL or more while noted (since its demotion or its load) is queued for {@link #chunkFull}. A load comes
     * before the chunk's first FULL report ({@code ChunkStatusTasks.full} posts it, and the report waits on
     * the holder's full chunk future, which waits on that task). A promotion by several steps at once reports
     * only its last status (each step cancels the one before), so any status from FULL on counts. A promotion
     * is reported later, from the main thread's task queue, and a demotion can come between (it is reported at
     * once): a promotion reported for a chunk that is no longer FULL leaves it noted, for its next promotion.
     */
    public static void fullStatusChanged(ServerLevel level, ChunkPos pos, FullChunkStatus status) {
        long c = pos.toLong();
        if (!status.isOrAfter(FullChunkStatus.FULL)) {
            AWAY.computeIfAbsent(level, l -> new LongOpenHashSet()).add(c);
            return;
        }
        if (level.getChunkSource().getChunkNow(pos.x, pos.z) == null)
            return;
        LongOpenHashSet away = AWAY.get(level);
        if (away != null && away.remove(c))
            FULL_AGAIN.computeIfAbsent(level, l -> new LongLinkedOpenHashSet()).add(c);
    }

    private static void drain() {
        if (FULL_AGAIN.isEmpty())
            return;
        List<Map.Entry<ServerLevel, LongLinkedOpenHashSet>> todo = new ArrayList<>(FULL_AGAIN.entrySet());
        FULL_AGAIN.clear();
        for (var e : todo)
            for (long c : e.getValue())
                chunkFull(e.getKey(), ChunkPos.getX(c), ChunkPos.getZ(c));
    }

    /**
     * A chunk that is FULL: every kinetic block entity in it, attached or not (one in a chunk that has not
     * ticked has not), gets a network update held back while the chunk was not loaded, and is re-synced when
     * its source, in another chunk, no longer gives it its speed or network; then every block entity in
     * another chunk that takes its speed from one here is checked the same way (the source's chunk was the
     * one away). A block entity the re-sync lists again before its first tick joins its network first
     * ({@link KineticJoin}, in {@code KineticBlockEntityMixin}). A chunk whose block entities are in step
     * costs one pass over them, and a look past the chunk's border from its border rows and from a block
     * entity with propagation of its own (a chain conveyor's connections).
     */
    public static void chunkFull(ServerLevel level, int cx, int cz) {
        List<KineticBlockEntity> ks = kinetics(level.getChunkSource().getChunkNow(cx, cz));
        for (KineticBlockEntity k : ks) {
            if (k.isRemoved())
                continue;
            KineticSyncPending p = (KineticSyncPending) k;
            if (p.mic_rtsim$syncPending()) {
                p.mic_rtsim$setSyncPending(false);
                KineticNetwork net = CreateGlassAccess.network(level, k.network);
                if (net != null && net.members.containsKey(k))
                    ((KineticNetworkInvoker) net).mic_rtsim$updateFromNetwork(k);
            }
            KineticBlockEntity s = staleSource(k);
            if (s != null)
                resync(k, s);
        }
        for (KineticBlockEntity k : ks) {
            if (k.isRemoved() || k.getTheoreticalSpeed() == 0)
                continue;
            for (BlockPos n : linksOut(k))
                if (Loaded.blockEntity(level, n) instanceof KineticBlockEntity o && k.getBlockPos().equals(o.source)) {
                    KineticBlockEntity s = staleSource(o);
                    if (s != null)
                        resync(o, s);
                }
        }
    }

    /** The kinetic block entities of a chunk (none when it is null). */
    private static List<KineticBlockEntity> kinetics(LevelChunk c) {
        if (c == null)
            return List.of();
        List<KineticBlockEntity> ks = null;
        for (BlockEntity be : c.getBlockEntities().values())
            if (be instanceof KineticBlockEntity k) {
                if (ks == null)
                    ks = new ArrayList<>();
                ks.add(k);
            }
        return ks == null ? List.of() : ks;
    }

    /**
     * Whether a kinetic block entity class adds propagation of its own beyond one block
     * ({@code addPropagationLocations} overridden: Create's chain conveyor, any addon's). Create's
     * {@code KineticBlockEntity} (a small cog's diagonals) and {@code SimpleKineticBlockEntity} (a large
     * cog's) add only diagonals one block away, which leave the chunk only from its border rows.
     */
    private static final ClassValue<Boolean> OWN_PROPAGATION = new ClassValue<>() {
        @Override
        protected Boolean computeValue(Class<?> c) {
            try {
                Class<?> d = c.getMethod("addPropagationLocations", IRotate.class, BlockState.class, List.class).getDeclaringClass();
                return d != KineticBlockEntity.class && d != SimpleKineticBlockEntity.class;
            } catch (NoSuchMethodException e) {
                return true;
            }
        }
    };

    /**
     * Where {@code k} propagates into another chunk: a face across its chunk's border, and its own additions
     * ({@code addPropagationLocations}: a chain conveyor's connections, a large cog's diagonals) outside its
     * chunk. Reads nothing.
     */
    /** Whether {@code k}'s class adds propagation of its own beyond one block ({@link #OWN_PROPAGATION}); J4's walk asks it too. */
    static boolean ownPropagation(KineticBlockEntity k) {
        return OWN_PROPAGATION.get(k.getClass());
    }

    private static List<BlockPos> linksOut(KineticBlockEntity k) {
        BlockPos p = k.getBlockPos();
        int lx = p.getX() & 15, lz = p.getZ() & 15;
        boolean border = lx == 0 || lx == 15 || lz == 0 || lz == 15;
        if (!border && !OWN_PROPAGATION.get(k.getClass()))
            return List.of();
        List<BlockPos> out = new ArrayList<>();
        if (lx == 0)
            out.add(p.west());
        if (lx == 15)
            out.add(p.east());
        if (lz == 0)
            out.add(p.north());
        if (lz == 15)
            out.add(p.south());
        BlockState state = k.getBlockState();
        if (state.getBlock() instanceof IRotate block)
            for (BlockPos n : k.addPropagationLocations(block, state, new ArrayList<>()))
                if (!sameChunk(p, n))
                    out.add(n);
        return out;
    }

    private static boolean sameChunk(BlockPos a, BlockPos b) {
        return (a.getX() >> 4) == (b.getX() >> 4) && (a.getZ() >> 4) == (b.getZ() >> 4);
    }

    /**
     * The block entity {@code k} takes its speed from, when that lies in another chunk, is loaded and turns,
     * and gives {@code k} another speed or network than {@code k} has; null otherwise (and on a client, for a
     * source of its own, or when the two are no longer connected: Create's own validation handles those).
     */
    public static KineticBlockEntity staleSource(KineticBlockEntity k) {
        if (!(k.getLevel() instanceof ServerLevel level) || !k.hasSource() || k.isSource())
            return null;
        BlockPos at = k.getBlockPos(), from = k.source;
        if (sameChunk(at, from))
            return null;
        if (!(Loaded.blockEntity(level, from) instanceof KineticBlockEntity s) || s.getTheoreticalSpeed() == 0)
            return null;
        float want = RotationPropagatorInvoker.mic_rtsim$conveyedSpeed(s, k);
        if (want == 0)
            return null;
        float have = k.getTheoreticalSpeed();
        // Create's own tolerance for one speed in a network (propagateNewSource): |speed| / 65536
        boolean sameSpeed = Math.abs(want - have) <= Math.abs(want) / 65536f;
        return sameSpeed && Objects.equals(k.network, s.network) ? null : s;
    }

    /** Clears {@code k}'s part of the network and gives it {@code source}'s speed again, as Create does when a source changes. */
    public static void resync(KineticBlockEntity k, KineticBlockEntity source) {
        resyncs++;
        RotationPropagatorInvoker.mic_rtsim$propagateMissingSource(k);
        RotationPropagatorInvoker.mic_rtsim$propagateNewSource(source);
    }
}
