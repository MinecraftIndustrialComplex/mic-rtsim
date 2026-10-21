package com.minecraftindustrialcomplex.mic_rtsim_oreexcavation.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim_oreexcavation.MicRtsimOreExcavation;
import com.tom.createores.Config;
import com.tom.createores.OreData;
import com.tom.createores.OreDataAttachment;
import com.tom.createores.recipe.VeinRecipe;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.ByteBuffer;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.*;

/**
 * mic_rtsim_oreexcavation on Create Ore Excavation 1.6.8's own jar: a vein under a drilling machine is a
 * finite deposit in every sweep, and a reconcile's deposit delta depletes it with the chunk's epoch.
 */
@GameTestHolder(MicRtsimOreExcavation.MODID)
@PrefixGameTestTemplate(false)
public final class OreExcavationGameTests {
    private OreExcavationGameTests() {}

    private static final ResourceLocation IRON = ResourceLocation.parse("createoreexcavation:ore_vein_type/iron");

    /**
     * IND_DEPOSIT_SAMPLE (plugin 1) carries the vein's remaining extraction cycles under the chunk's own
     * deposit good; an IND_CHUNK_RECONCILE with -25 of it at the deposit inventory is applied through
     * {@code OreData.extract} (IND_RECONCILE_RESULT applied -25), and the next sample is 25 lower.
     * Ore Excavation's server config makes default veins infinite; the test makes them finite
     * ({@code Config.defaultInfinite}) and puts it back.
     */
    @GameTest(template = "yard", batch = "oreexcavation_1", timeoutTicks = 800)
    public static void aVeinIsAFiniteDepositDepletedOnLoad(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        boolean wasInfinite = Config.defaultInfinite;
        BlockPos chest = inChunk(helper, 3, 3);
        BlockPos drill = inChunk(helper, 8, 8);
        long[] id = {0};
        long[] remaining = {0};
        String[] good = {null};
        long op = engineOp();
        helper.startSequence()
                .thenExecute(() -> {
                    Config.defaultInfinite = false;
                    set(helper, chest, com.minecraftindustrialcomplex.mic_rtsim.gametest.TestMachines.MACHINE.get().defaultBlockState());
                    playerPlaced(helper, chest);
                    set(helper, drill, block("createoreexcavation:drilling_machine").defaultBlockState());
                    LevelChunk chunk = helper.getLevel().getChunkAt(drill);
                    OreData data = OreDataAttachment.getData(chunk);
                    data.setRecipe(IRON);
                    data.setLoaded(true);
                    data.setRandomMul(0.5f);
                    data.setExtractedAmount(0);
                    chunk.setUnsaved(true);
                    VeinRecipe r = data.getRecipe(helper.getLevel().getRecipeManager()).value();
                    remaining[0] = data.getResourcesRemaining(r);
                    check("a finite vein: " + remaining[0], remaining[0] > 25);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, chest);
                    check("the district", d != null);
                    id[0] = d.id;
                    List<ByteBuffer> s = received(IndRecords.IND_DEPOSIT_SAMPLE, IndRecords.DepositSample.DISTRICT, d.id);
                    check("an IND_DEPOSIT_SAMPLE", !s.isEmpty());
                    ByteBuffer last = s.get(s.size() - 1);
                    good[0] = javaName(last.getInt(IndRecords.DepositSample.GOOD));
                    check("plugin 1 (Ore Excavation)", aux(last) == IndRecords.DepositPlugin.ORE_EXCAVATION);
                    check("the vein's good: " + good[0], good[0] != null && good[0].startsWith("d/0/" + IRON + "/"));
                    check("its remaining cycles: " + last.getLong(IndRecords.DepositSample.REMAINING) + " vs " + remaining[0],
                            last.getLong(IndRecords.DepositSample.REMAINING) == remaining[0]);
                    check("finite", (last.getInt(IndRecords.DepositSample.FLAGS) & IndRecords.DepositSample.FLAG_INFINITE) == 0);
                    check("in the drill's chunk", last.getInt(IndRecords.DepositSample.CHUNK_X) == (drill.getX() >> 4)
                            && last.getInt(IndRecords.DepositSample.CHUNK_Z) == (drill.getZ() >> 4));
                    record("aVeinIsAFiniteDepositDepletedOnLoad", "sample (" + engineName() + ")", good[0] + " remaining " + remaining[0]);
                })
                .thenExecute(() -> reconcile(op, id[0], 1, helper.getLevel(), drill.getX() >> 4, drill.getZ() >> 4,
                        List.of(new Delta(IndRecords.DEPOSIT_INVENTORY, good[0], new BlockPos((drill.getX() >> 4) * 16, 0, (drill.getZ() >> 4) * 16), -25))))
                .thenWaitUntil(() -> {
                    List<ByteBuffer> res = received(IndRecords.IND_RECONCILE_RESULT, IndRecords.ReconcileResult.OP_ID, op);
                    check("IND_RECONCILE_RESULT", res.size() == 1);
                    ByteBuffer r = res.get(0);
                    check("one entry", aux(r) == 1);
                    check("at the deposit inventory", r.getInt(IndRecords.ReconcileResult.ENTRIES) == IndRecords.DEPOSIT_INVENTORY);
                    check("-25 requested, -25 applied: " + r.getLong(IndRecords.ReconcileResult.ENTRIES + 16),
                            r.getLong(IndRecords.ReconcileResult.ENTRIES + 8) == -25 && r.getLong(IndRecords.ReconcileResult.ENTRIES + 16) == -25);
                    OreData data = OreDataAttachment.getData(helper.getLevel().getChunkAt(drill));
                    long now = data.getResourcesRemaining(data.getRecipe(helper.getLevel().getRecipeManager()).value());
                    check("the vein holds 25 fewer: " + now, now == remaining[0] - 25);
                })
                .thenWaitUntil(() -> {
                    List<ByteBuffer> s = received(IndRecords.IND_DEPOSIT_SAMPLE, IndRecords.DepositSample.DISTRICT, id[0]);
                    ByteBuffer last = s.get(s.size() - 1);
                    check("a later sample sees it: " + last.getLong(IndRecords.DepositSample.REMAINING),
                            last.getLong(IndRecords.DepositSample.REMAINING) == remaining[0] - 25);
                    record("aVeinIsAFiniteDepositDepletedOnLoad", "after the reconcile", last.getLong(IndRecords.DepositSample.REMAINING));
                    Config.defaultInfinite = wasInfinite;
                })
                .thenSucceed();
    }
}
