package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim_create.glass.CreateGlassAccess;
import com.minecraftindustrialcomplex.mic_rtsim_create.mixin.KineticNetworkInvoker;
import com.minecraftindustrialcomplex.mic_rtsim_create.mixin.RotationPropagatorInvoker;
import com.simibubi.create.content.kinetics.KineticNetwork;
import com.simibubi.create.content.kinetics.base.IRotate;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
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
 * The re-sync of a part of a kinetic network that comes back stale (SECTIONS.md §2.8, choice SC37; PATCHES.md
 * rows 56 and 57). Rotation stops at a chunk that is not loaded ({@code RotationPropagatorMixin}), so a part
 * in such a chunk keeps its last speed while its source, in another chunk that ticks, changes. It comes back
 * two ways:
 * <ul>
 *   <li>loaded anew: each block entity's first attach ({@code KineticBlockEntityMixin}) re-syncs it;</li>
 *   <li>FULL again without loading (it dropped below FULL and stayed in memory, ticket level 34 to 44): no
 *       block entity attaches, so the chunk is noted at its demotion and queued at its promotion
 *       ({@code ChunkMapPromotionMixin}), and re-synced here at the next server tick's start or end
 *       ({@link #chunkFull}), before Create's next propagation can meet the stale part and break it.</li>
 * </ul>
 */
public final class StaleKinetics {
    private StaleKinetics() {}

    private static int resyncs;
    /** Chunks that dropped below FULL and are still in memory, per level (server thread only). */
    private static final Map<ServerLevel, LongOpenHashSet> AWAY = new IdentityHashMap<>();
    /** Chunks of {@link #AWAY} that are FULL again, per level, until the next drain (server thread only). */
    private static final Map<ServerLevel, LongLinkedOpenHashSet> FULL_AGAIN = new IdentityHashMap<>();

    /** Re-syncs since the server started (for tests). */
    public static int resyncs() {
        return resyncs;
    }

    static void register() {
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener((ServerTickEvent.Pre e) -> drain());
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener((ServerTickEvent.Post e) -> drain());
        // a chunk that unloads, or loads anew (its block entities attach, which re-syncs them), is no longer away
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener((ChunkEvent.Unload e) -> forget(e));
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener((ChunkEvent.Load e) -> forget(e));
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener((ServerStoppedEvent e) -> {
            AWAY.clear();
            FULL_AGAIN.clear();
        });
    }

    /**
     * The maps are the server thread's: an event another thread posts (a chunk system that loads off it) is
     * left out, which at worst costs one spurious {@link #chunkFull} pass over a chunk that loaded anew.
     */
    private static void forget(ChunkEvent e) {
        if (e.getLevel() instanceof ServerLevel level && level.getServer().isSameThread()) {
            LongOpenHashSet away = AWAY.get(level);
            if (away != null)
                away.remove(e.getChunk().getPos().toLong());
        }
    }

    /**
     * {@code ChunkMap.onFullChunkStatusChange}: a chunk below FULL is noted away until it unloads or loads
     * anew; one that is FULL or more again while noted is queued for {@link #chunkFull}. A promotion by
     * several steps at once reports only its last status (each step cancels the one before), so any status
     * from FULL on counts. A promotion is reported later, from the main thread's task queue, and a demotion
     * can come between (it is reported at once): a promotion reported for a chunk that is no longer FULL
     * leaves it noted, for its next promotion.
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
     * A chunk that is FULL: every kinetic block entity in it that has attached (one that has not re-syncs at
     * its first attach) gets a network update held back while the chunk was not loaded, and is re-synced when
     * its source, in another chunk, no longer gives it its speed or network; then every block entity in
     * another chunk that takes its speed from one here is checked the same way (the source's chunk was the
     * one away). A chunk whose block entities are in step costs one pass over them.
     */
    public static void chunkFull(ServerLevel level, int cx, int cz) {
        LevelChunk c = level.getChunkSource().getChunkNow(cx, cz);
        if (c == null)
            return;
        List<KineticBlockEntity> ks = null;
        for (BlockEntity be : c.getBlockEntities().values())
            if (be instanceof KineticBlockEntity k) {
                if (ks == null)
                    ks = new ArrayList<>();
                ks.add(k);
            }
        if (ks == null)
            return;
        for (KineticBlockEntity k : ks) {
            if (k.isRemoved() || k.updateSpeed)
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
            if (k.isRemoved() || k.updateSpeed || k.getTheoreticalSpeed() == 0)
                continue;
            for (BlockPos n : linksOut(k))
                if (Loaded.blockEntity(level, n) instanceof KineticBlockEntity o && !o.updateSpeed
                        && k.getBlockPos().equals(o.source)) {
                    KineticBlockEntity s = staleSource(o);
                    if (s != null)
                        resync(o, s);
                }
        }
    }

    /**
     * Where {@code k} propagates into another chunk: a face across its chunk's border, and its own additions
     * ({@code addPropagationLocations}: a chain conveyor's connections, a large cog's diagonals) outside its
     * chunk. Reads nothing.
     */
    private static List<BlockPos> linksOut(KineticBlockEntity k) {
        BlockPos p = k.getBlockPos();
        List<BlockPos> out = new ArrayList<>();
        int lx = p.getX() & 15, lz = p.getZ() & 15;
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
