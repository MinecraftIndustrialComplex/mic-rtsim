package com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators.gametest;

import com.jesz.createdieselgenerators.world.OilChunksSavedData;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Deposits;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators.DieselOilPlugin;
import com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators.MicRtsimDieselGenerators;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Blocks;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.ByteBuffer;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.*;

/** mic_rtsim_dieselgenerators on Create Diesel Generators 1.3.15's own jar. */
@GameTestHolder(MicRtsimDieselGenerators.MODID)
@PrefixGameTestTemplate(false)
public final class DieselGameTests {
    private DieselGameTests() {}

    /**
     * A pumpjack hole's chunk oil is IND_DEPOSIT_SAMPLE (plugin 2) in millibuckets; IND_DEPOSIT_DELTA
     * -1,234 is applied to {@code OilChunksSavedData} at once (APPLIED), the same op again is DUPLICATE, and
     * a delta for a far chunk nobody has loaded is applied too (level SavedData, F15).
     */
    @GameTest(template = "yard", batch = "diesel_1", timeoutTicks = 600)
    public static void chunkOilIsADepositWrittenAtOnce(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        BlockPos chest = inChunk(helper, 3, 3);
        BlockPos hole = inChunk(helper, 8, 8);
        ChunkPos cp = new ChunkPos(hole);
        ChunkPos far = new ChunkPos(cp.x + 9000, cp.z + 9000);
        long[] id = {0};
        String good = Deposits.name(DieselOilPlugin.OIL, helper.getLevel().dimension(), cp.x, cp.z);
        String farGood = Deposits.name(DieselOilPlugin.OIL, helper.getLevel().dimension(), far.x, far.z);
        long op = engineOp(), opFar = engineOp();
        helper.startSequence()
                .thenExecute(() -> {
                    OilChunksSavedData.setChunkOilAmount(helper.getLevel(), cp, 5_000_000);
                    OilChunksSavedData.setChunkOilAmount(helper.getLevel(), far, 6_000_000);
                    set(helper, chest, Blocks.CHEST.defaultBlockState());
                    playerPlaced(helper, chest);
                    set(helper, hole, block("createdieselgenerators:pumpjack_hole").defaultBlockState());
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, chest);
                    check("the district", d != null);
                    id[0] = d.id;
                    List<ByteBuffer> s = received(IndRecords.IND_DEPOSIT_SAMPLE, r -> r.getLong(IndRecords.DepositSample.DISTRICT) == d.id
                            && good.equals(javaName(r.getInt(IndRecords.DepositSample.GOOD))));
                    check("an IND_DEPOSIT_SAMPLE of the chunk's oil", !s.isEmpty());
                    ByteBuffer last = s.get(s.size() - 1);
                    check("plugin 2 (Diesel oil)", aux(last) == IndRecords.DepositPlugin.DIESEL_OIL);
                    check("5,000,000 mB: " + last.getLong(IndRecords.DepositSample.REMAINING), last.getLong(IndRecords.DepositSample.REMAINING) == 5_000_000);
                    record("chunkOilIsADepositWrittenAtOnce", "sample (" + engineName() + ")", good + " = " + last.getLong(IndRecords.DepositSample.REMAINING));
                    depositDelta(op, IndRecords.DepositPlugin.DIESEL_OIL, helper.getLevel(), cp.x, cp.z, good, -1234);
                    depositDelta(opFar, IndRecords.DepositPlugin.DIESEL_OIL, helper.getLevel(), far.x, far.z, farGood, -500_000);
                })
                .thenWaitUntil(() -> {
                    check("applied", commandResults(op).contains(Obs.CommandResult.APPLIED));
                    check("the far one applied", commandResults(opFar).contains(Obs.CommandResult.APPLIED));
                    int now = OilChunksSavedData.getChunkOilAmount(helper.getLevel(), cp);
                    int farNow = OilChunksSavedData.getChunkOilAmount(helper.getLevel(), far);
                    record("chunkOilIsADepositWrittenAtOnce", "after", now + " here, " + farNow + " far away (chunk loaded: "
                            + (helper.getLevel().getChunkSource().getChunkNow(far.x, far.z) != null) + ")");
                    check("4,998,766 left: " + now, now == 4_998_766);
                    check("5,500,000 left far away: " + farNow, farNow == 5_500_000);
                    depositDelta(op, IndRecords.DepositPlugin.DIESEL_OIL, helper.getLevel(), cp.x, cp.z, good, -1234);
                })
                .thenWaitUntil(() -> check("the same op again is a duplicate", commandResults(op).contains(Obs.CommandResult.DUPLICATE_OP)))
                .thenExecute(() -> check("still 4,998,766", OilChunksSavedData.getChunkOilAmount(helper.getLevel(), cp) == 4_998_766))
                .thenSucceed();
    }
}
