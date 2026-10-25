package com.minecraftindustrialcomplex.mic_rtsim_destroy;

import com.minecraftindustrialcomplex.mic_rtsim.industry.Deposits;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryPlugin;
import com.minecraftindustrialcomplex.mic_rtsim.journal.DurableOpRing;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.chunk.LevelChunk;
import petrolpark.mc.destroy.DestroyAttachmentTypes;
import petrolpark.mc.destroy.DestroyRegistries;
import petrolpark.mc.destroy.content.oil.ChunkCrudeOil;
import petrolpark.mc.destroy.content.oil.pumpjack.PumpjackBlockEntity;

import java.util.Map;

/**
 * Destroy for the industry engine: chunk crude oil as a finite deposit (I11, F15) and pollution
 * attributed to districts (I14), both as deposit goods with Destroy's plugin code (3, written back with
 * the chunk).
 *
 * <ul>
 *   <li><b>Crude oil:</b> for each loaded member chunk with a pumpjack ({@code PumpjackBlockEntity},
 *       which pumps its own chunk), the {@code destroy:chunk_crude_oil} attachment's amount, once the chunk's
 *       oil is generated (read with {@code getExistingDataOrNull}: Destroy generates it on discovery, with
 *       a player, and the sampler never does). Good {@code d/0/destroy:crude_oil/<chunk hash>}. A reconcile's
 *       delta goes through {@code ChunkCrudeOil.setAmount} (clamped at 0) in the chunk's own command.</li>
 *   <li><b>Pollution:</b> once per sweep, at the district's anchor chunk (its lowest), each pollution
 *       type's cumulative change attributed to the district ({@link PollutionLedger}): good
 *       {@code d/0/<pollution type id>/<hash of the district id>}, which rises while the district pollutes,
 *       so the engine learns it as an output. Written back at the anchor chunk's reconcile: level types
 *       (greenhouse, ozone depletion, acid rain) to the level, chunk types (smog, radioactivity) to that
 *       chunk (choice Q9).</li>
 * </ul>
 */
public final class DestroyPlugin implements IndustryPlugin {

    public static final ResourceLocation CRUDE = ResourceLocation.fromNamespaceAndPath("destroy", "crude_oil");

    public static String pollutionKey(long district) {
        return "pollution|" + district;
    }

    @Override
    public void deposits(ServerLevel level, District district, LevelChunk chunk, DepositSink out) {
        ChunkPos cp = chunk.getPos();
        boolean pumpjack = false;
        for (BlockEntity be : chunk.getBlockEntities().values())
            if (be instanceof PumpjackBlockEntity) {
                pumpjack = true;
                break;
            }
        if (pumpjack) {
            ChunkCrudeOil oil = chunk.getExistingDataOrNull(DestroyAttachmentTypes.CHUNK_CRUDE_OIL.get());
            if (oil != null && oil.isGenerated())
                out.add(IndRecords.DepositPlugin.DESTROY_CRUDE, Deposits.name(CRUDE, level.dimension(), cp.x, cp.z), oil.getAmount(), false);
        }
        if (cp.toLong() == district.anchorChunk()) {
            for (Map.Entry<ResourceLocation, Long> e : PollutionLedger.get(level).totals(district.id).entrySet())
                out.add(IndRecords.DepositPlugin.DESTROY_CRUDE, Deposits.name(e.getKey(), pollutionKey(district.id)), e.getValue(), false);
        }
    }

    /**
     * Level pollution's applied ops (RP-207): a level type is Destroy's {@code LEVEL_POLLUTION} level attachment,
     * which NeoForge saves with the level's other attachments in its {@code neoforge_data_attachments}, so its ring
     * is a level attachment too ({@code mic_rtsim:applied_ops}). Crude oil and the chunk types are in the chunk, with
     * the reconcile's epoch: no ring.
     */
    @Override
    public DurableOpRing depositRing(ServerLevel level, int chunkX, int chunkZ, String good) {
        ResourceLocation type = pollutionType(level, chunkX, chunkZ, good);
        return type != null && DestroyRegistries.LEVEL_POLLUTION_TYPES.get(type) != null ? level.getData(RtsimAttachments.LEVEL_APPLIED_OPS) : null;
    }

    /** A refused resend of a level type's op: the ledger, which booked it with the pollution, keeps it again. */
    @Override
    public void depositRefused(ServerLevel level, int chunkX, int chunkZ, String good, DepositOp op) {
        if (pollutionType(level, chunkX, chunkZ, good) != null)
            PollutionLedger.get(level).refused(good, op);
    }

    private static ResourceLocation pollutionType(ServerLevel level, int chunkX, int chunkZ, String good) {
        Industry ind = IndustryApi.current();
        District d = ind == null ? null : ind.districtAt(level.dimension(), ChunkPos.asLong(chunkX, chunkZ));
        return d == null ? null : Deposits.kindFor(good, pollutionKey(d.id));
    }

    @Override
    public Long applyDeposit(ServerLevel level, int chunkX, int chunkZ, String good, long delta) {
        return applyDeposit(level, chunkX, chunkZ, good, delta, null);
    }

    @Override
    public Long applyDeposit(ServerLevel level, int chunkX, int chunkZ, String good, long delta, DepositOp op) {
        if (CRUDE.equals(Deposits.kindIn(good, level.dimension(), chunkX, chunkZ))) {
            LevelChunk chunk = level.getChunkSource().getChunkNow(chunkX, chunkZ);
            ChunkCrudeOil oil = chunk == null ? null : chunk.getExistingDataOrNull(DestroyAttachmentTypes.CHUNK_CRUDE_OIL.get());
            if (oil == null || !oil.isGenerated())
                return 0L;
            int before = oil.getAmount();
            int after = oil.setAmount((int) Math.max(0, Math.min(Integer.MAX_VALUE, (long) before + delta)));
            chunk.setUnsaved(true);
            return (long) after - before;
        }
        Industry ind = IndustryApi.current();
        District d = ind == null ? null : ind.districtAt(level.dimension(), ChunkPos.asLong(chunkX, chunkZ));
        if (d == null)
            return null;
        ResourceLocation type = Deposits.kindFor(good, pollutionKey(d.id));
        if (type == null)
            return null;
        LevelChunk chunk = level.getChunkSource().getChunkNow(chunkX, chunkZ);
        return PollutionLedger.get(level).apply(level, chunk, d.id, type, delta, good, op);
    }
}
