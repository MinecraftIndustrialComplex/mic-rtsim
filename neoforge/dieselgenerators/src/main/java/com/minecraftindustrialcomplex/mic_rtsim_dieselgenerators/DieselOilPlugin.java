package com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators;

import com.jesz.createdieselgenerators.content.pumpjack.PumpjackHoleBlockEntity;
import com.jesz.createdieselgenerators.world.OilChunksSavedData;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Deposits;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryPlugin;
import com.minecraftindustrialcomplex.mic_rtsim.journal.DurableOpRing;
import com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators.mixin.OilChunksSavedDataAccessor;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.chunk.LevelChunk;

import java.util.HashMap;
import java.util.Map;

/**
 * Diesel Generators' chunk oil as a finite deposit (I11, F15; INDUSTRY.md §6 case 4).
 *
 * <ul>
 *   <li><b>Read</b> once per sweep, for each loaded member chunk with a pumpjack hole
 *       ({@code PumpjackHoleBlockEntity}, which pumps its own chunk): the chunk's amount in millibuckets,
 *       the stored one when a pumpjack or command wrote it, else the amount Diesel computes from the seed,
 *       computed once per session (choice Q5). {@code Integer.MAX_VALUE} is Diesel's infinite chunk, sent
 *       with the infinite flag: never stock.</li>
 *   <li><b>Good:</b> {@code d/0/createdieselgenerators:oil/<chunk hash>} ({@link Deposits}).</li>
 *   <li><b>Write</b> at once with IND_DEPOSIT_DELTA (F15: level SavedData, writable unloaded):
 *       {@code OilChunksSavedData.setChunkOilAmount}, clamped at 0. The dedup is the oil file's own ring of
 *       applied ops or the journal (the core's {@code Industry.depositDelta}): {@code cdg_oil_chunks} is a file
 *       of its own, written before the journal's in a world save, and keeps the ops of its draws beside them
 *       ({@link OilRingHolder}), so a crash between the two writes neither draws a chunk twice nor leaves a
 *       draw the journal knows off the deposit (docs/choices-journal-barrier.md RP-200, RP-204).</li>
 * </ul>
 */
public final class DieselOilPlugin implements IndustryPlugin {

    public static final ResourceLocation OIL = ResourceLocation.fromNamespaceAndPath("createdieselgenerators", "oil");

    private record Key(ResourceKey<Level> dim, long chunk) {}

    /** Amounts of chunks nothing has written, computed once (Diesel recomputes them on every read). */
    private final Map<Key, Integer> base = new HashMap<>();

    private int amount(ServerLevel level, ChunkPos cp) {
        OilChunksSavedData data = OilChunksSavedData.load(level);
        Integer stored = ((OilChunksSavedDataAccessor) data).mic_rtsim$chunks().get(cp);
        if (stored != null) {
            base.remove(new Key(level.dimension(), cp.toLong()));
            return OilChunksSavedData.getChunkOilAmount(level, cp); // applies Diesel's infinite threshold
        }
        return base.computeIfAbsent(new Key(level.dimension(), cp.toLong()), k -> OilChunksSavedData.getChunkOilAmount(level, cp));
    }

    /**
     * A pumpjack hole is a machine: it seeds its district (DS-10). It is also the one block that seeds with
     * no player ({@link HoleSeeds}, the oil gate audit's F1, choice OGC60).
     */
    @Override
    public Boolean seed(BlockEntity be) {
        return be instanceof PumpjackHoleBlockEntity ? Boolean.TRUE : null;
    }

    @Override
    public void deposits(ServerLevel level, District district, LevelChunk chunk, DepositSink out) {
        boolean pumpjack = false;
        for (BlockEntity be : chunk.getBlockEntities().values())
            if (be instanceof PumpjackHoleBlockEntity) {
                pumpjack = true;
                break;
            }
        if (!pumpjack)
            return;
        ChunkPos cp = chunk.getPos();
        int amount = amount(level, cp);
        if (amount <= 0)
            return; // no oil here: nothing to deplete
        boolean infinite = amount == Integer.MAX_VALUE;
        out.add(IndRecords.DepositPlugin.DIESEL_OIL, Deposits.name(OIL, level.dimension(), cp.x, cp.z), infinite ? 0 : amount, infinite);
    }

    /** The applied ops kept in this dimension's {@code cdg_oil_chunks}, beside the draws (RP-204). */
    @Override
    public DurableOpRing depositRing(ServerLevel level, int chunkX, int chunkZ, String good) {
        if (!OIL.equals(Deposits.kindIn(good, level.dimension(), chunkX, chunkZ)))
            return null;
        return OilChunksSavedData.load(level) instanceof OilRingHolder h ? h.mic_rtsim$ring() : null;
    }

    @Override
    public Long applyDeposit(ServerLevel level, int chunkX, int chunkZ, String good, long delta) {
        if (!OIL.equals(Deposits.kindIn(good, level.dimension(), chunkX, chunkZ)))
            return null;
        ChunkPos cp = new ChunkPos(chunkX, chunkZ);
        int before = amount(level, cp);
        if (before == Integer.MAX_VALUE)
            return 0L; // infinite: never stock, so the engine sends it nothing
        long after = Math.max(0, Math.min((long) Integer.MAX_VALUE - 1, (long) before + delta));
        OilChunksSavedData.setChunkOilAmount(level, cp, (int) after);
        base.remove(new Key(level.dimension(), cp.toLong()));
        int read = OilChunksSavedData.getChunkOilAmount(level, cp);
        return (read == Integer.MAX_VALUE ? after : read) - (long) before;
    }
}
