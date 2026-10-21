package com.minecraftindustrialcomplex.mic_rtsim.industry;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.chunk.LevelChunk;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;

/**
 * Exactly-once reconciliation on load (I8, docs/industry-boundary.md §4.1), and damage (C18, I23).
 *
 * <ol>
 *   <li>A command for a chunk that is not loaded is held (in memory: the engine keeps every pending
 *       delta until a result says it is applied, so a restart resends them). The engine sends them ahead
 *       of a player's arrival (IND_APPROACH), so they are usually here before the chunk loads.</li>
 *   <li>At {@code ChunkEvent.Load} of a chunk with held commands, its block entities and those of its
 *       district's other loaded chunks stop ticking ({@link TickGate}) until the next drain of loads (the
 *       start of the next tick, or the end of this one). There the held commands are applied in epoch
 *       order, whatever the quota, and then IND_CHUNK_LOADED goes out with the epoch now in the chunk's
 *       {@code mic_rtsim:reconcile} attachment (0 if none), after their results (R1 to R3,
 *       docs/choices-repair.md). The engine answers with the pending commands above that epoch.</li>
 *   <li>A command whose epoch the chunk already holds, or whose op id the journal holds as applied, is
 *       answered DUPLICATE and changes nothing.</li>
 *   <li>Every removal first, then the additions, through the inventory's capability (or its plugin's
 *       access) at the delta's position. A removal takes up to what is there, an addition puts in up to
 *       what fits. Then each destroy entry whose block still has the expected state is broken with
 *       {@code level.destroyBlock(pos, true)}, so its contents drop as items; the others are skipped.
 *       Fire is never placed.</li>
 *   <li>In the same tick, the chunk's attachment takes the new epoch (with the result record, until the
 *       engine acknowledges it), so the contents and "epoch e applied" reach disk together in the
 *       chunk's NBT. IND_RECONCILE_RESULT (what was asked, what was applied, the blocks broken, the
 *       count skipped) and COMMAND_RESULT are journaled.</li>
 * </ol>
 * A crash anywhere replays correctly: a chunk saved before step 5 still has its older epoch and its
 * older contents. Applying is under the block quota ({@code reconcileOpsPerTick}); a command bigger
 * than the whole quota goes alone at the start of a tick.
 */
public final class Reconciler {

    private record ChunkKey(ResourceKey<Level> dim, long chunk) {}

    private record Held(byte[] bytes, long cmdSeq, int epoch) {}

    private final Industry ind;
    private final Map<ChunkKey, List<Held>> held = new HashMap<>();
    private final Set<String> recommitted = new HashSet<>();
    private int opsLeft;
    private int quota;
    private long quotaTick = Long.MIN_VALUE;

    // counters
    long applied;
    long duplicates;
    long heldCount;
    long blocksBroken;
    long blocksSkipped;
    long shortfalls;

    Reconciler(Industry ind) {
        this.ind = ind;
    }

    public int held() {
        int n = 0;
        for (List<Held> l : held.values())
            n += l.size();
        return n;
    }

    /** U10, the module switched on again: a result a chunk still carries is journaled again, as at a session start. */
    void forgetAnnounced() {
        recommitted.clear();
    }

    /**
     * GameTests: what a crash and a restart lose on the Java side. Held commands live in memory only (the
     * engine sends them again, ahead or at the chunk's next IND_CHUNK_LOADED, F27), a result a chunk still
     * carries is journaled again in the new session, and the journal, rolled back to the last save, no
     * longer has the op ids applied after it ({@code rolledBack}).
     */
    public void restartForTest(long... rolledBack) {
        held.clear();
        recommitted.clear();
        for (long op : rolledBack)
            ind.boundary.journal().forgetAppliedForTest(op);
    }

    /** True while a command waits for this chunk to load (R2: its load gates ticking until it lands). */
    boolean holdsFor(ResourceKey<Level> dim, long chunk) {
        return !held.isEmpty() && held.containsKey(new ChunkKey(dim, chunk));
    }

    // ---- loads ----------------------------------------------------------------------------------

    /**
     * A district chunk loaded (R1, R3): re-journal an unacknowledged result it carries, apply the commands
     * held for it in epoch order (whatever the block quota: nothing of the chunk has ticked yet), then
     * report the epoch it now holds. So every IND_RECONCILE_RESULT of the load comes before its
     * IND_CHUNK_LOADED, and the engine never takes a reconcile applied at the load for one the chunk
     * lost (engine.rs, ChunkLoaded: an epoch at or below the reported one counts as applied as asked).
     */
    void chunkLoaded(ServerLevel level, LevelChunk chunk, District d, long now) {
        if (chunk == null)
            return;
        IndustryAttachments.Epochs epochs = chunk.getExistingDataOrNull(IndustryAttachments.RECONCILE.get());
        if (epochs != null) {
            for (IndustryAttachments.EpochEntry e : epochs.entries()) {
                // A result saved with the chunk but maybe not with the journal (a crash): journaled
                // again (same bytes, same op id, a new sequence); the engine skips it by op id.
                if (e.pendingResult().isPresent()) {
                    String key = level.dimension().location() + "/" + chunk.getPos().toLong() + "/" + e.district() + "/" + e.epoch();
                    if (recommitted.add(key)) {
                        long seq = ind.boundary.recommit(e.pendingResult().get());
                        dropResultWhenAcked(level, chunk.getPos(), e.district(), e.epoch(), seq);
                    }
                }
            }
        }
        List<Held> list = held.remove(new ChunkKey(level.dimension(), chunk.getPos().toLong()));
        if (list != null) {
            refreshQuota(now);
            list.sort(Comparator.comparingInt(Held::epoch));
            for (Held h : list)
                apply(level, chunk, ByteBuffer.wrap(h.bytes).order(Layout.ORDER), h.cmdSeq, now, true);
            epochs = chunk.getExistingDataOrNull(IndustryAttachments.RECONCILE.get());
        }
        int applied = epochs == null ? 0 : epochs.epoch(d.id);
        ByteBuffer r = ind.journal(IndRecords.IND_CHUNK_LOADED, IndRecords.ChunkLoaded.SIZE, 0);
        r.putLong(IndRecords.ChunkLoaded.DISTRICT, d.id);
        r.putInt(IndRecords.ChunkLoaded.DIM, ind.dimId(level.dimension()));
        r.putInt(IndRecords.ChunkLoaded.X, chunk.getPos().x);
        r.putInt(IndRecords.ChunkLoaded.Z, chunk.getPos().z);
        r.putInt(IndRecords.ChunkLoaded.APPLIED_EPOCH, applied);
        ind.commit(r);
    }

    // ---- commands -------------------------------------------------------------------------------

    /** The block quota is per game tick, whether commands or held reconciles spend it. */
    private void refreshQuota(long now) {
        if (now != quotaTick) {
            quotaTick = now;
            quota = IndustryConfig.get(IndustryConfig.RECONCILE_OPS_PER_TICK);
            opsLeft = quota;
        }
    }

    boolean accept(ByteBuffer b, int at, int size, int deltas, long cmdSeq, long now) {
        refreshQuota(now);
        long opId = b.getLong(at + IndRecords.ChunkReconcile.OP_ID);
        int destroys = b.getInt(at + IndRecords.ChunkReconcile.DESTROY_COUNT);
        if (deltas < 0 || destroys < 0 || IndRecords.ChunkReconcile.size(deltas, destroys) > size
                || IndRecords.ReconcileResult.size(deltas, destroys) > 0xFFF8) {
            MicRtsim.LOGGER.warn("[mic_rtsim] industry: IND_CHUNK_RECONCILE of {} bytes does not hold {} deltas and {} destroys", size, deltas, destroys);
            ind.commandResult(cmdSeq, opId, Obs.CommandResult.FAILED);
            return true;
        }
        ServerLevel level = ind.engineLevel(b.getInt(at + IndRecords.ChunkReconcile.DIM));
        if (level == null) {
            ind.commandResult(cmdSeq, opId, Obs.CommandResult.FAILED);
            return true;
        }
        int cx = b.getInt(at + IndRecords.ChunkReconcile.CHUNK_X), cz = b.getInt(at + IndRecords.ChunkReconcile.CHUNK_Z);
        LevelChunk chunk = level.getChunkSource().getChunkNow(cx, cz);
        byte[] bytes = new byte[size];
        b.get(at, bytes, 0, size);
        int epoch = b.getInt(at + IndRecords.ChunkReconcile.EPOCH);
        if (chunk == null) {
            // Held until the chunk loads (§4.1 step 1).
            held.computeIfAbsent(new ChunkKey(level.dimension(), ChunkPos.asLong(cx, cz)), k -> new ArrayList<>())
                    .add(new Held(bytes, cmdSeq, epoch));
            heldCount++;
            return true;
        }
        ChunkKey key = new ChunkKey(level.dimension(), ChunkPos.asLong(cx, cz));
        if (held.containsKey(key)) {
            // Earlier epochs for this chunk are still waiting: keep the order.
            held.get(key).add(new Held(bytes, cmdSeq, epoch));
            return true;
        }
        return apply(level, chunk, ByteBuffer.wrap(bytes).order(Layout.ORDER), cmdSeq, now, false);
    }

    void tick(long now) {
        refreshQuota(now);
        if (held.isEmpty())
            return;
        for (Iterator<Map.Entry<ChunkKey, List<Held>>> it = held.entrySet().iterator(); it.hasNext(); ) {
            Map.Entry<ChunkKey, List<Held>> e = it.next();
            ServerLevel level = ind.level(e.getKey().dim());
            if (level == null)
                continue;
            LevelChunk chunk = level.getChunkSource().getChunkNow(ChunkPos.getX(e.getKey().chunk()), ChunkPos.getZ(e.getKey().chunk()));
            if (chunk == null)
                continue;
            List<Held> list = e.getValue();
            list.sort(Comparator.comparingInt(Held::epoch));
            while (!list.isEmpty()) {
                Held h = list.get(0);
                if (!apply(level, chunk, ByteBuffer.wrap(h.bytes).order(Layout.ORDER), h.cmdSeq, now, false))
                    return; // the quota is spent: the rest next tick
                list.remove(0);
            }
            it.remove();
        }
    }

    /**
     * Applies one IND_CHUNK_RECONCILE record (at offset 0 of {@code r}). Returns false to defer it (quota);
     * {@code atLoad} applies it whatever the quota (it spends the quota all the same).
     */
    private boolean apply(ServerLevel level, LevelChunk chunk, ByteBuffer r, long cmdSeq, long now, boolean atLoad) {
        long opId = r.getLong(IndRecords.ChunkReconcile.OP_ID);
        long districtId = r.getLong(IndRecords.ChunkReconcile.DISTRICT);
        int epoch = r.getInt(IndRecords.ChunkReconcile.EPOCH);
        int n = r.getInt(com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout.R_AUX);
        int m = r.getInt(IndRecords.ChunkReconcile.DESTROY_COUNT);

        IndustryAttachments.Epochs epochs = chunk.getData(IndustryAttachments.RECONCILE.get());
        if (epochs.epoch(districtId) >= epoch || ind.opApplied(opId)) {
            duplicates++;
            ind.commandResult(cmdSeq, opId, Obs.CommandResult.DUPLICATE_OP);
            return true;
        }
        int ops = n + m;
        if (!atLoad && ops > opsLeft && opsLeft < quota)
            return false;
        opsLeft -= ops;

        District d = ind.district(districtId);
        ChunkPos cp = chunk.getPos();
        int[] order = new int[n];
        int k = 0;
        for (int i = 0; i < n; i++)
            if (r.getLong(IndRecords.ChunkReconcile.DELTAS + i * IndRecords.ChunkReconcile.DELTA_SIZE + IndRecords.ChunkReconcile.D_DELTA) < 0)
                order[k++] = i;
        for (int i = 0; i < n; i++)
            if (r.getLong(IndRecords.ChunkReconcile.DELTAS + i * IndRecords.ChunkReconcile.DELTA_SIZE + IndRecords.ChunkReconcile.D_DELTA) >= 0)
                order[k++] = i;

        long[][] results = new long[n][4]; // inventory, good, requested, applied
        for (int j = 0; j < n; j++) {
            int i = order[j];
            int e = IndRecords.ChunkReconcile.DELTAS + i * IndRecords.ChunkReconcile.DELTA_SIZE;
            int inventory = r.getInt(e + IndRecords.ChunkReconcile.D_INVENTORY);
            int goodId = r.getInt(e + IndRecords.ChunkReconcile.D_GOOD);
            BlockPos pos = new BlockPos(r.getInt(e + IndRecords.ChunkReconcile.D_X), r.getInt(e + IndRecords.ChunkReconcile.D_Y), r.getInt(e + IndRecords.ChunkReconcile.D_Z));
            long delta = r.getLong(e + IndRecords.ChunkReconcile.D_DELTA);
            long applied = 0;
            if (inventory == IndRecords.DEPOSIT_INVENTORY) {
                // A finite deposit of this chunk (class d): its plugin writes it (Ore Excavation's vein,
                // Destroy's crude oil), in this tick, with the epoch below (I11, F15).
                applied = ind.applyDeposit(level, cp.x, cp.z, ind.engineName(IndRecords.DICT_GOOD, goodId), delta);
            } else if ((pos.getX() >> 4) == cp.x && (pos.getZ() >> 4) == cp.z) {
                Goods.Name good = Goods.parse(ind.engineName(IndRecords.DICT_GOOD, goodId));
                if (good != null) {
                    District.Inventory inv = d == null ? null : d.inventoryAt(pos);
                    if (inv == null)
                        inv = new District.Inventory(inventory, pos, IndRecords.Inventory.KIND_ITEMS | IndRecords.Inventory.KIND_FLUIDS | IndRecords.Inventory.KIND_ENERGY, 0, 0, 0);
                    try {
                        applied = ind.sampler.access(level, inv).apply(good, delta);
                    } catch (RuntimeException ex) {
                        ind.fail("applying " + delta + " of " + good + " at " + pos, ex);
                    }
                }
            }
            // A deposit no plugin owns is reported unapplied, and the engine keeps it owed.
            if (applied != delta)
                shortfalls++;
            results[j] = new long[] {inventory & 0xFFFF_FFFFL, goodId & 0xFFFF_FFFFL, delta, applied};
        }

        List<BlockPos> broken = new ArrayList<>();
        int skipped = 0;
        int b0 = IndRecords.ChunkReconcile.DELTAS + n * IndRecords.ChunkReconcile.DELTA_SIZE;
        for (int i = 0; i < m; i++) {
            int e = b0 + i * IndRecords.ChunkReconcile.DESTROY_SIZE;
            BlockPos pos = new BlockPos(r.getInt(e + IndRecords.ChunkReconcile.B_X), r.getInt(e + IndRecords.ChunkReconcile.B_Y), r.getInt(e + IndRecords.ChunkReconcile.B_Z));
            String expected = ind.engineName(IndRecords.DICT_BLOCK_STATE, r.getInt(e + IndRecords.ChunkReconcile.B_STATE));
            if ((pos.getX() >> 4) != cp.x || (pos.getZ() >> 4) != cp.z || expected == null
                    || !BlockStateParser.serialize(level.getBlockState(pos)).equals(expected)) {
                skipped++;
                continue;
            }
            // Broken with drops (C18, I23): containers spill their contents as recoverable items. The
            // engine books the spill as a port (engine.rs reconcile_result), so the drops are marked:
            // a player picking them up is not a second outflow. Another mod's onRemove may throw; the
            // epoch and the result are written whatever happens here.
            ind.ports.markDropsPorted(level, pos);
            try {
                if (level.destroyBlock(pos, true))
                    broken.add(pos);
                else
                    skipped++;
            } catch (RuntimeException ex) {
                ind.fail("breaking " + pos + " for a reconcile", ex);
                skipped++;
            }
        }
        blocksBroken += broken.size();
        blocksSkipped += skipped;

        // The result, then the epoch in the same tick (the chunk carries the result until acknowledged).
        ByteBuffer res = ind.journal(IndRecords.IND_RECONCILE_RESULT, IndRecords.ReconcileResult.size(n, broken.size()), n);
        res.putLong(IndRecords.ReconcileResult.OP_ID, opId);
        res.putLong(IndRecords.ReconcileResult.DISTRICT, districtId);
        res.putInt(IndRecords.ReconcileResult.EPOCH, epoch);
        res.putInt(IndRecords.ReconcileResult.DIM, ind.dimId(level.dimension()));
        res.putInt(IndRecords.ReconcileResult.CHUNK_X, cp.x);
        res.putInt(IndRecords.ReconcileResult.CHUNK_Z, cp.z);
        res.putInt(IndRecords.ReconcileResult.DESTROYED, broken.size());
        res.putInt(IndRecords.ReconcileResult.SKIPPED, skipped);
        for (int j = 0; j < n; j++) {
            int e = IndRecords.ReconcileResult.ENTRIES + j * IndRecords.RESULT24;
            res.putInt(e, (int) results[j][0]);
            res.putInt(e + 4, (int) results[j][1]);
            res.putLong(e + 8, results[j][2]);
            res.putLong(e + 16, results[j][3]);
        }
        int p0 = IndRecords.ReconcileResult.ENTRIES + n * IndRecords.RESULT24;
        for (int i = 0; i < broken.size(); i++) {
            res.putInt(p0 + i * IndRecords.POS16, broken.get(i).getX());
            res.putInt(p0 + i * IndRecords.POS16 + 4, broken.get(i).getY());
            res.putInt(p0 + i * IndRecords.POS16 + 8, broken.get(i).getZ());
        }
        long seq = ind.commit(res);
        chunk.setData(IndustryAttachments.RECONCILE.get(), epochs.with(new IndustryAttachments.EpochEntry(districtId, epoch, Optional.of(res.array()))));
        chunk.setUnsaved(true);
        ind.commandResult(cmdSeq, opId, Obs.CommandResult.APPLIED);
        recommitted.add(level.dimension().location() + "/" + cp.toLong() + "/" + districtId + "/" + epoch);
        dropResultWhenAcked(level, cp, districtId, epoch, seq);
        applied++;
        if (d != null)
            ind.diagnostics.noteReconcile(d, results, broken.size(), skipped);
        return true;
    }

    private void dropResultWhenAcked(ServerLevel level, ChunkPos cp, long district, int epoch, long seq) {
        ResourceKey<Level> dim = level.dimension();
        ind.whenAcked(seq, () -> {
            ServerLevel l = ind.level(dim);
            LevelChunk c = l == null ? null : l.getChunkSource().getChunkNow(cp.x, cp.z);
            if (c == null)
                return; // unloaded: the result is journaled again at its next load, and skipped by op id
            IndustryAttachments.Epochs ep = c.getExistingDataOrNull(IndustryAttachments.RECONCILE.get());
            IndustryAttachments.EpochEntry e = ep == null ? null : ep.get(district);
            if (e != null && e.epoch() == epoch && e.pendingResult().isPresent()) {
                c.setData(IndustryAttachments.RECONCILE.get(), ep.with(new IndustryAttachments.EpochEntry(district, epoch, Optional.empty())));
                c.setUnsaved(true);
            }
        });
    }
}
