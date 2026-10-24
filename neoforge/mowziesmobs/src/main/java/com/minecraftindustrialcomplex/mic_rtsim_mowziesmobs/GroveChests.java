package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs;

import com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugin;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Camps;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import net.minecraft.core.BlockPos;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.BarrelBlockEntity;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.ChestBlockEntity;
import net.minecraft.world.level.block.entity.RandomizableContainerBlockEntity;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.capabilities.Capabilities;

import java.nio.ByteBuffer;
import java.util.HashMap;
import java.util.LinkedHashSet;
import java.util.Map;
import java.util.Set;

/**
 * The chest a grove's tribute goes into (MZB-422, MZB-446; docs/boundary.md §9.3), after Ice and Fire's
 * {@code Lairs}. The engine owes a grove the goods its Umvuthi was paid; it sends a stack to the chest this tells it
 * of, as a TRANSFER_TO_CONTAINER the core already applies and answers (APPLIED, DUPLICATE_OP, FAILED). All this does
 * is name the chest and say how many slots are free in it: HAVEN_CHEST, journaled, the chest's state, so a copy
 * changes nothing and the latest record wins.
 *
 * <p>The chest is a {@code ChestBlockEntity} or {@code BarrelBlockEntity} in the grove's footprint, in a loaded chunk,
 * with an empty slot or a loot table not rolled yet (all its slots count free; reading one would roll it): the
 * nearest to the fire pit's centre, ties by position. Every {@link #SCAN_TICKS} ticks each grove that is in is looked
 * at, and a HAVEN_CHEST goes when the grove is new to the engine in this epoch (a session start, or the grove coming
 * in: the engine forgets the chest at CAMP_OUT), when the chest or its free slots changed, and, with 0 free, when the
 * chest it told of is gone or full and there is no other. A grove with no chest, never told of one, sends nothing.
 *
 * <p>After every answer to a transfer into the chest it told of, whatever the status and whether or not the free
 * count changed, it sends again (the re-check, R5: the engine takes a slot off its count for each stack it sends, as
 * if the stack filled one; a stack that merged into a partly filled slot leaves the count one short; and a FAILED
 * makes the engine forget the chest), at the end of that tick, so at most once a tick for a grove.
 *
 * <p>The record names a place and a count and claims nothing the world must keep, so the chunk barrier is not
 * asked for the chest's chunk: a replay that names a chest the world lost meets a FAILED transfer, which this
 * answers with a record of what the chest is now.
 *
 * <p>Looks only at groves that are in, every {@link #SCAN_TICKS} ticks (D1); between scans it holds a set of the
 * groves whose chest took a transfer.
 */
public final class GroveChests implements SessionPlugin {
    public static final GroveChests INSTANCE = new GroveChests();

    /** How often the groves that are in are looked at for their chest (PLACEHOLDER, MZB-422). */
    public static final int SCAN_TICKS = 200;

    /** What was told the engine for a grove: in which epoch, and the chest and its free slots. */
    private record Sent(long epoch, ResourceKey<Level> dimension, BlockPos pos, int free) {}

    /** A chest as found. */
    private record Found(BlockPos pos, int free) {}

    private final Map<Long, Sent> sent = new HashMap<>();
    /** The groves whose chest took a transfer since they were last refreshed. */
    private final Set<Long> dirty = new LinkedHashSet<>();
    private long lastScan = Long.MIN_VALUE;

    /** This session's counters (GameTests). */
    public long records, transfers;

    private GroveChests() {}

    @Override
    public String name() {
        return "mic_rtsim_mowziesmobs:grove_chests";
    }

    @Override
    public void sessionStarted(RtsimSession session) {
        clear();
    }

    @Override
    public void sessionFailed(RtsimSession session) {
        clear();
    }

    private void clear() {
        sent.clear();
        dirty.clear();
        lastScan = Long.MIN_VALUE;
        records = transfers = 0;
    }

    /** The next tick looks at every grove that is in (GameTests). */
    public void scanNextTick() {
        lastScan = Long.MIN_VALUE;
    }

    /** The chest and free slots last told the engine for a grove this session, or null (GameTests). */
    public BlockPos chestOf(long faction) {
        Sent s = sent.get(faction);
        return s == null ? null : s.pos();
    }

    /** The free slots last told the engine for a grove this session, or -1 (GameTests). */
    public int freeOf(long faction) {
        Sent s = sent.get(faction);
        return s == null ? -1 : s.free();
    }

    // ---- the hooks ------------------------------------------------------------------------------

    /** A transfer was answered for the container at {@code pos}: a grove that told the engine of it tells it again. */
    @Override
    public void containerTransferred(RtsimSession session, ServerLevel level, BlockPos pos, int status) {
        if (sent.isEmpty())
            return;
        for (Map.Entry<Long, Sent> e : sent.entrySet()) {
            Sent s = e.getValue();
            if (s.dimension().equals(level.dimension()) && s.pos().equals(pos)) {
                dirty.add(e.getKey());
                transfers++;
            }
        }
    }

    @Override
    public void endOfTick(RtsimSession session, long now) {
        if (!RtsimConfig.on(Module.CONFLICT)) {
            sent.clear(); // switched on again, the engine may have forgotten every chest
            dirty.clear();
            return;
        }
        boolean scan = lastScan == Long.MIN_VALUE || now - lastScan >= SCAN_TICKS;
        if (!scan && dirty.isEmpty())
            return;
        Camps camps = ConflictBridge.INSTANCE.reconciler();
        if (scan) {
            lastScan = now;
            for (Camps.Site site : camps.sites()) {
                if (site.kind != GroveKind.INSTANCE)
                    continue;
                if (!site.in()) {
                    sent.remove(site.faction); // out: the engine forgot its chest, the next epoch tells it again
                    dirty.remove(site.faction);
                    continue;
                }
                refresh(session, site, now, dirty.remove(site.faction));
            }
        }
        if (!dirty.isEmpty()) {
            for (Long faction : Set.copyOf(dirty)) {
                dirty.remove(faction);
                Camps.Site site = camps.site(faction);
                if (site != null && site.in())
                    refresh(session, site, now, true);
            }
        }
    }

    // ---- the chest ------------------------------------------------------------------------------

    /** Looks at a grove's chest and sends HAVEN_CHEST if it must: {@code forced} after a transfer into the chest. */
    private void refresh(RtsimSession session, Camps.Site site, long now, boolean forced) {
        ServerLevel level = session.server().getLevel(site.dimension);
        if (level == null)
            return;
        Sent had = sent.get(site.faction);
        boolean fresh = had == null || had.epoch() != site.epoch();
        Found found = find(level, site);
        if (found == null) {
            // Gone or full, and no other: say so, once (a grove never told of a chest has nothing to take back).
            if (had != null && !fresh && (had.free() > 0 || forced))
                send(session, site, level, had.pos(), 0, now);
            return;
        }
        if (fresh || forced || !found.pos().equals(had.pos()) || found.free() != had.free())
            send(session, site, level, found.pos(), found.free(), now);
    }

    private void send(RtsimSession session, Camps.Site site, ServerLevel level, BlockPos pos, int free, long now) {
        int dim = session.dimId(level.dimension()); // before the record: an id is defined before its first use
        ByteBuffer r = session.boundary().beginJournaled(ConflictRecords.HAVEN_CHEST, ConflictRecords.HavenChest.SIZE, 0, now);
        r.putLong(ConflictRecords.HavenChest.FACTION, site.faction);
        r.putInt(ConflictRecords.HavenChest.DIM, dim);
        r.putInt(ConflictRecords.HavenChest.X, pos.getX());
        r.putInt(ConflictRecords.HavenChest.Y, pos.getY());
        r.putInt(ConflictRecords.HavenChest.Z, pos.getZ());
        r.putInt(ConflictRecords.HavenChest.FREE, free);
        session.boundary().commit(r);
        sent.put(site.faction, new Sent(site.epoch(), level.dimension(), pos.immutable(), free));
        records++;
    }

    /**
     * The chest in the grove's loaded footprint chunks with room, nearest the centre, or null. A chest or barrel
     * counts, inside the footprint's circle and with an item handler at its place (MZB-642); a double chest's halves
     * are two chests, each by its own slots (the handler at one half takes into both, so a half that reports room is
     * never wrong, only short).
     */
    static Found find(ServerLevel level, Camps.Site site) {
        BlockPos best = null;
        int bestFree = 0;
        double bestD = Double.MAX_VALUE;
        for (ChunkPos cp : site.footprintChunks()) {
            LevelChunk chunk = level.getChunkSource().getChunkNow(cp.x, cp.z);
            if (chunk == null)
                continue;
            for (Map.Entry<BlockPos, BlockEntity> e : chunk.getBlockEntities().entrySet()) {
                if (!(e.getValue() instanceof RandomizableContainerBlockEntity c)
                        || !(c instanceof ChestBlockEntity || c instanceof BarrelBlockEntity))
                    continue;
                BlockPos p = e.getKey();
                Vec3 at = Vec3.atCenterOf(p);
                if (!site.contains(at))
                    continue;
                int free = free(c);
                if (free <= 0)
                    continue;
                // A container nothing can insert into is no haven, however many slots it shows: Lootr's chests are
                // ChestBlockEntity subclasses with their loot table kept (all slots free) and no item handler (NeoForge
                // registers the chest's only for the vanilla blocks), so every transfer into one would FAIL and be
                // told again, for ever (MZB-642).
                if (level.getCapability(Capabilities.ItemHandler.BLOCK, p, null) == null)
                    continue;
                double d = at.distanceToSqr(site.spot);
                if (d < bestD || (d == bestD && p.compareTo(best) < 0)) {
                    best = p.immutable();
                    bestFree = free;
                    bestD = d;
                }
            }
        }
        return best == null ? null : new Found(best, bestFree);
    }

    /** A container's empty slots; a loot table not rolled yet counts them all (reading a slot would roll it). */
    static int free(RandomizableContainerBlockEntity c) {
        if (c.getLootTable() != null)
            return c.getContainerSize();
        int n = 0;
        for (int i = 0; i < c.getContainerSize(); i++)
            if (c.getItem(i).isEmpty())
                n++;
        return n;
    }
}
