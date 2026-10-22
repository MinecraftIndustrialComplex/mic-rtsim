package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.ThreatConfig;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.level.block.Blocks;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.ByteBuffer;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.*;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.io;

/**
 * The roof gate (OILGATE.md OG-11, INDUSTRY.md §13.6), native only: the stub resolves no attack. The
 * plugin's survey says which machines are roofed; the engine's {@code resolve} and {@code damage} let a
 * flying attack break only blocks with sky access, fewer as more machines are roofed, while a ground
 * attack ignores roofs.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class RoofGateGameTests {
    private RoofGateGameTests() {}

    private static final String YARD = "industry_yard";
    private static final String TEST = "aRoofShieldsAMachineFromAFlyerNotFromASiege";

    /**
     * One district on a flat yard: a chest and two furnaces, one under a stone roof. The survey counts two
     * machines, one roofed (a roof pass of 1/2). An unwatched flying attack by a dragon, eight strong (eight
     * blocks before the gate, four after it), breaks blocks with sky access only: the open furnace or the
     * chest goes, the roofed furnace stands although the gate's four would reach it. A siege of twenty
     * afterwards (its own class, so no cooldown; the open yard's gaps let a share through) breaks the
     * roofed furnace: the roof, not the survey's block list, is what spared it. Before the attack the
     * engine must know the district's chunks loaded (they were loaded before detection made it).
     */
    @GameTest(template = YARD, batch = "roof_gate", timeoutTicks = 2400)
    public static void aRoofShieldsAMachineFromAFlyerNotFromASiege(GameTestHelper helper) {
        if (!nativeEngine()) {
            record(TEST, "skipped", "on the stub: the engine resolves attacks");
            helper.succeed();
            return;
        }
        NativeEngine engine = (NativeEngine) engine();
        fastConfig();
        forgetAll(helper);
        ThreatConfig.setForTest(ThreatConfig.SURVEY_MIN_INTERVAL, 1);
        ThreatConfig.setForTest(ThreatConfig.SURVEY_CHUNKS_PER_TICK, 64);
        BlockPos chest = inChunk(helper, 1, 1);
        BlockPos roofed = inChunk(helper, 8, 8);
        BlockPos open = inChunk(helper, 13, 3);
        long[] id = {0};
        int[] n = {0};
        long[] t = new long[4]; // the flying attack sent, its blocks applied, the siege sent, its blocks applied
        long threat = 0x4F47_3131_0000_0000L | (System.nanoTime() & 0xFFFF_FFFFL);
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, chest, Blocks.CHEST.defaultBlockState());
                    set(helper, roofed, Blocks.FURNACE.defaultBlockState());
                    set(helper, open, Blocks.FURNACE.defaultBlockState());
                    set(helper, roofed.above(3), Blocks.STONE.defaultBlockState());
                    playerPlaced(helper, chest);
                    playerPlaced(helper, roofed);
                    playerPlaced(helper, open);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, roofed);
                    check("one district with both furnaces and the chest", d != null && d.chunks.contains(chunk(chest))
                            && d.chunks.contains(chunk(open)));
                    id[0] = d.id;
                })
                // "Roofed" is the sky above the furnace: lighting runs on its own thread.
                .thenWaitUntil(io(() -> check("the light engine darkened the block above the roofed furnace",
                        !helper.getLevel().canSeeSky(roofed.above()) && helper.getLevel().canSeeSky(open.above()))))
                .thenExecute(() -> {
                    long[] s = ind().threats().lastSurvey(id[0]);
                    n[0] = s == null ? 0 : (int) s[18];
                    ind().threats().surveySoon(ind().district(id[0]));
                })
                .thenWaitUntil(() -> {
                    long[] s = ind().threats().lastSurvey(id[0]);
                    check("a survey", s != null && s[18] > n[0]);
                    check("two machines, one roofed (" + s[3] + ", " + s[4] + ")", s[3] == 2 && s[4] == 1);
                    List<ByteBuffer> blocks = received(IndRecords.IND_SURVEY_BLOCKS, IndRecords.SurveyBlocks.DISTRICT, id[0]);
                    check("IND_SURVEY_BLOCKS", !blocks.isEmpty());
                    ByteBuffer part = blocks.get(blocks.size() - 1);
                    int rows = aux(part), roofless = 0;
                    boolean roofedListed = false, roofedRoofless = true;
                    for (int i = 0; i < rows; i++) {
                        int at = IndRecords.SurveyBlocks.BLOCKS + i * IndRecords.SurveyBlocks.BLOCK;
                        int f = part.getInt(at + IndRecords.SurveyBlocks.B_FLAGS);
                        if ((f & IndRecords.SurveyBlocks.FLAG_ROOFLESS) != 0)
                            roofless++;
                        if (part.getInt(at + IndRecords.SurveyBlocks.B_X) == roofed.getX() && part.getInt(at + IndRecords.SurveyBlocks.B_Y) == roofed.getY()
                                && part.getInt(at + IndRecords.SurveyBlocks.B_Z) == roofed.getZ()) {
                            roofedListed = true;
                            roofedRoofless = (f & IndRecords.SurveyBlocks.FLAG_ROOFLESS) != 0;
                        }
                    }
                    check("the roofed furnace is an exposed block, not roofless", roofedListed && !roofedRoofless);
                    record(TEST, "survey", "machines " + s[3] + ", roofed " + s[4] + "; " + rows + " exposed blocks, " + roofless + " with sky access; the whole survey "
                            + java.util.Arrays.toString(s));
                })
                // The engine reconciles only into chunks it knows loaded: the district's chunks were loaded
                // before it existed, so their IND_CHUNK_LOADED went out when detection made it.
                .thenWaitUntil(io(() -> check("the engine knows the district's chunks loaded (" + where(helper, id[0]) + ")",
                        engine.industryThreatProbe(id[0])[3] >= 1)))
                .thenExecute(() -> {
                    engine.industryThreat(threat, NativeEngine.THREAT_DRAGON, NativeEngine.CLASS_FLYING, 8, 8.0, id[0], ind().gameTime());
                    t[0] = helper.getLevel().getGameTime();
                })
                // The engine's round trip (the order taken, the attack resolved, its reconcile applied) runs on
                // other threads: io() holds each failing tick 5 ms, so the timeout is real time too.
                .thenWaitUntil(io(() -> check("the flyer broke a block with sky access (" + where(helper, id[0]) + ")",
                        helper.getLevel().getBlockState(open).isAir() || helper.getLevel().getBlockState(chest).isAir())))
                .thenExecute(() -> t[1] = helper.getLevel().getGameTime())
                .thenIdle(100) // the whole reconcile round lands
                .thenExecute(() -> {
                    boolean openGone = helper.getLevel().getBlockState(open).isAir();
                    boolean chestGone = helper.getLevel().getBlockState(chest).isAir();
                    check("the roofed furnace stands (" + helper.getLevel().getBlockState(roofed) + ")",
                            helper.getLevel().getBlockState(roofed).is(Blocks.FURNACE));
                    check("the roof still stands", helper.getLevel().getBlockState(roofed.above(3)).is(Blocks.STONE));
                    record(TEST, "flyer", "dragon x8, flying: applied " + (t[1] - t[0]) + " ticks after the order; open furnace "
                            + (openGone ? "broken" : "stands") + ", chest " + (chestGone ? "broken" : "stands") + ", roofed furnace stands");
                    engine.industryThreat(threat + 1, NativeEngine.THREAT_BANDIT, NativeEngine.CLASS_SIEGE, 20, 20.0, id[0], ind().gameTime());
                    t[2] = helper.getLevel().getGameTime();
                })
                .thenWaitUntil(io(() -> check("the siege broke the roofed furnace (" + helper.getLevel().getBlockState(roofed) + "; "
                        + where(helper, id[0]) + ")", helper.getLevel().getBlockState(roofed).isAir())))
                .thenExecute(() -> {
                    t[3] = helper.getLevel().getGameTime();
                    record(TEST, "siege", "bandits x20, siege: the roofed furnace broken " + (t[3] - t[2]) + " ticks after the order (walkers ignore roofs)");
                    ThreatConfig.setForTest(ThreatConfig.SURVEY_MIN_INTERVAL, null);
                    ThreatConfig.setForTest(ThreatConfig.SURVEY_CHUNKS_PER_TICK, null);
                })
                .thenSucceed();
    }

    /**
     * What a failing wait says: the district's watch (a watched district's attacks are skipped), its
     * reconcile results, and the engine's view: presence, loaded chunks, exposed and roofless blocks, held
     * destruction, the last attack (skipped, through, survivors, destroyed) and the link's queue
     * (NativeEngine.industryThreatProbe).
     */
    private static String where(GameTestHelper helper, long district) {
        District d = ind().district(district);
        int results = received(IndRecords.IND_RECONCILE_RESULT, IndRecords.ReconcileResult.DISTRICT, district).size();
        String engine = engine() instanceof NativeEngine e ? "; engine steps " + e.steps() + ", the engine's district "
                + java.util.Arrays.toString(e.industryThreatProbe(district)) : "";
        return (d == null ? "no district" : "watched " + d.watched() + ", load state " + d.loadState()) + ", " + results
                + " reconcile results" + engine;
    }
}
