package com.minecraftindustrialcomplex.mic_rtsim_destroy.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.industry.Deposits;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim_destroy.DestroyPlugin;
import com.minecraftindustrialcomplex.mic_rtsim_destroy.MicRtsimDestroy;
import com.minecraftindustrialcomplex.mic_rtsim_destroy.PollutionLedger;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;
import petrolpark.mc.destroy.DestroyAttachmentTypes;
import petrolpark.mc.destroy.DestroyPollutionTypes;
import petrolpark.mc.destroy.content.oil.ChunkCrudeOil;
import petrolpark.mc.destroy.core.pollution.PollutionHelper;

import java.nio.ByteBuffer;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.*;

/** mic_rtsim_destroy on Destroy 0.4.1's own jar (with Petrolpark 1.5.11 and Create 6.0.10). */
@GameTestHolder(MicRtsimDestroy.MODID)
@PrefixGameTestTemplate(false)
public final class DestroyGameTests {
    private DestroyGameTests() {}

    private static Map<String, Long> latestDeposits(long district) {
        Map<String, Long> out = new HashMap<>();
        for (ByteBuffer s : received(IndRecords.IND_DEPOSIT_SAMPLE, IndRecords.DepositSample.DISTRICT, district))
            out.put(javaName(s.getInt(IndRecords.DepositSample.GOOD)), s.getLong(IndRecords.DepositSample.REMAINING));
        return out;
    }

    /**
     * I14 and I11 together. Emissions through {@code PollutionHelper.changePollution} by position: +40
     * greenhouse (a level type) and +12 smog (a chunk type) inside the district are its own, +9 smog outside
     * is nobody's. The district's deposits then read greenhouse 40, smog 12 and its pumpjack chunk's crude
     * oil 1,000. A reconcile at the anchor chunk adds 10 greenhouse to the level and 5 smog to the chunk
     * and takes 100 crude oil; the totals follow (50, 17, 900).
     */
    @GameTest(template = "yard", batch = "destroy_1", timeoutTicks = 800)
    public static void pollutionIsAttributedAndCrudeOilIsADeposit(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        BlockPos chest = inChunk(helper, 3, 3);
        BlockPos pump = inChunk(helper, 8, 8);
        BlockPos outside = inChunk(helper, 8, 8).offset(-48, 0, -48);
        long[] id = {0};
        long[] greenhouse0 = {0};
        long op = engineOp();
        ResourceLocation gh = ResourceLocation.parse("destroy:greenhouse"), smog = ResourceLocation.parse("destroy:smog");
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, chest, Blocks.CHEST.defaultBlockState());
                    playerPlaced(helper, chest);
                    set(helper, pump, block("destroy:pumpjack").defaultBlockState());
                    LevelChunk c = helper.getLevel().getChunkAt(pump);
                    c.setData(DestroyAttachmentTypes.CHUNK_CRUDE_OIL.get(), new ChunkCrudeOil(true, 1000));
                    c.setUnsaved(true);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, chest);
                    check("the district", d != null);
                    id[0] = d.id;
                })
                .thenExecute(() -> {
                    check("pollution is on in Destroy's config", PollutionHelper.isPollutionEnabled());
                    greenhouse0[0] = PollutionHelper.getPollution(helper.getLevel(), DestroyPollutionTypes.GREENHOUSE.get());
                    PollutionHelper.changePollution(helper.getLevel(), chest, DestroyPollutionTypes.GREENHOUSE.get(), 40);
                    PollutionHelper.changePollution(helper.getLevel(), chest, DestroyPollutionTypes.SMOG.get(), 12);
                    PollutionHelper.changePollution(helper.getLevel(), outside, DestroyPollutionTypes.SMOG.get(), 9);
                    Map<ResourceLocation, Long> t = PollutionLedger.get(helper.getLevel()).totals(id[0]);
                    record("pollutionIsAttributedAndCrudeOilIsADeposit", "attributed", t);
                    check("40 greenhouse and 12 smog attributed: " + t, t.getOrDefault(gh, 0L) == 40 && t.getOrDefault(smog, 0L) == 12);
                })
                .thenWaitUntil(() -> {
                    District d = ind().district(id[0]);
                    Map<String, Long> dep = latestDeposits(id[0]);
                    String g = Deposits.name(gh, DestroyPlugin.pollutionKey(id[0])), s = Deposits.name(smog, DestroyPlugin.pollutionKey(id[0]));
                    ChunkPos cp = new ChunkPos(pump);
                    String crude = Deposits.name(DestroyPlugin.CRUDE, helper.getLevel().dimension(), cp.x, cp.z);
                    check("deposits: " + dep, dep.getOrDefault(g, -1L) == 40 && dep.getOrDefault(s, -1L) == 12 && dep.getOrDefault(crude, -1L) == 1000);
                    record("pollutionIsAttributedAndCrudeOilIsADeposit", "deposits (" + engineName() + ")", dep);
                    int ax = ChunkPos.getX(d.anchorChunk()), az = ChunkPos.getZ(d.anchorChunk());
                    BlockPos at = new BlockPos(ax * 16, 0, az * 16);
                    if (ax == cp.x && az == cp.z) {
                        reconcile(op, id[0], 1, helper.getLevel(), ax, az, List.of(new Delta(IndRecords.DEPOSIT_INVENTORY, crude, at, -100),
                                new Delta(IndRecords.DEPOSIT_INVENTORY, g, at, 10), new Delta(IndRecords.DEPOSIT_INVENTORY, s, at, 5)));
                    } else {
                        // The pumpjack's chunk is not the anchor: two commands, one per chunk.
                        reconcile(op, id[0], 1, helper.getLevel(), ax, az, List.of(new Delta(IndRecords.DEPOSIT_INVENTORY, g, at, 10),
                                new Delta(IndRecords.DEPOSIT_INVENTORY, s, at, 5)));
                        reconcile(op + 1, id[0], 1, helper.getLevel(), cp.x, cp.z, List.of(new Delta(IndRecords.DEPOSIT_INVENTORY, crude,
                                new BlockPos(cp.x * 16, 0, cp.z * 16), -100)));
                    }
                })
                .thenWaitUntil(() -> {
                    long applied = 0;
                    int results = 0;
                    for (long o : new long[] {op, op + 1})
                        for (ByteBuffer r : received(IndRecords.IND_RECONCILE_RESULT, IndRecords.ReconcileResult.OP_ID, o)) {
                            results++;
                            for (int i = 0; i < aux(r); i++)
                                applied += Math.abs(r.getLong(IndRecords.ReconcileResult.ENTRIES + i * IndRecords.RESULT24 + 16));
                        }
                    check("the reconcile results", results >= 1 && applied == 115);
                })
                .thenExecute(() -> {
                    long ghNow = PollutionHelper.getPollution(helper.getLevel(), DestroyPollutionTypes.GREENHOUSE.get());
                    ChunkCrudeOil oil = helper.getLevel().getChunkAt(pump).getExistingDataOrNull(DestroyAttachmentTypes.CHUNK_CRUDE_OIL.get());
                    Map<ResourceLocation, Long> t = PollutionLedger.get(helper.getLevel()).totals(id[0]);
                    record("pollutionIsAttributedAndCrudeOilIsADeposit", "after", "greenhouse " + greenhouse0[0] + " -> " + ghNow + ", totals " + t
                            + ", crude " + oil.getAmount());
                    check("the level took 50 greenhouse in all", ghNow - greenhouse0[0] == 50);
                    check("totals 50 and 17", t.getOrDefault(gh, 0L) == 50 && t.getOrDefault(smog, 0L) == 17);
                    check("900 crude oil left", oil.getAmount() == 900);
                })
                .thenSucceed();
    }
}
