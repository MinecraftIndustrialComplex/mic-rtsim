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

import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.*;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.io;

/**
 * The roof gate (OILGATE.md OG-11 as OGB-7 changed it, INDUSTRY.md §13.6), native only: the stub resolves
 * no attack. The plugin's survey says which machines are roofed and lists a roof a flyer reaches as a
 * block it can break; the engine's {@code resolve} and {@code damage} let a flying attack break only blocks
 * with sky access, fewer as more machines are shielded, while a ground attack ignores roofs.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class RoofGateGameTests {
    private RoofGateGameTests() {}

    private static final String YARD = "industry_yard";
    private static final String TEST = "aRoofShieldsAMachineFromAFlyerNotFromASiege";

    /**
     * One district on a flat yard: a chest and three test machines, one in the open, one under a stone
     * roof three up (a shed: within {@code roofReachBlocks}, 8, so a block a flyer breaks, OGB-7), one under
     * stone twelve up (cover a flyer does not reach). The survey counts three machines, one shielded (a roof
     * pass of 2/3), and lists the shed's roof with sky access. An unwatched flying attack by a dragon, eight
     * strong (eight blocks before the gate, five after it), breaks blocks with sky access only: the open
     * machine, the chest and the shed's roof are its three; both covered machines stand. The next survey
     * finds the shed's machine open. A siege of twenty afterwards (its own class, so no cooldown; the open
     * yard's gaps let a share through) breaks the deep-covered machine: the cover, not the survey's block
     * list, is what spared it. Before the attack the engine must know the district's chunks loaded (they
     * were loaded before detection made it).
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
        BlockPos shed = inChunk(helper, 8, 8);
        BlockPos deep = inChunk(helper, 4, 12);
        BlockPos open = inChunk(helper, 13, 3);
        BlockPos roof = shed.above(3), cover = deep.above(12);
        long[] id = {0};
        int[] n = {0};
        long[] t = new long[4]; // the flying attack sent, its blocks applied, the siege sent, its blocks applied
        long threat = 0x4F47_3131_0000_0000L | (System.nanoTime() & 0xFFFF_FFFFL);
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, chest, Blocks.CHEST.defaultBlockState());
                    for (BlockPos m : new BlockPos[] {shed, deep, open})
                        set(helper, m, TestMachines.MACHINE.get().defaultBlockState());
                    set(helper, roof, Blocks.STONE.defaultBlockState());
                    set(helper, cover, Blocks.STONE.defaultBlockState());
                    playerPlaced(helper, chest);
                    playerPlaced(helper, shed);
                    playerPlaced(helper, deep);
                    playerPlaced(helper, open);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, shed);
                    check("one district with the three machines and the chest", d != null && d.chunks.contains(chunk(chest))
                            && d.chunks.contains(chunk(open)) && d.chunks.contains(chunk(deep)));
                    id[0] = d.id;
                })
                // Sky access is the sky light: lighting runs on its own thread.
                .thenWaitUntil(io(() -> check("the light engine darkened the blocks above the covered machines",
                        !helper.getLevel().canSeeSky(shed.above()) && !helper.getLevel().canSeeSky(deep.above()) && helper.getLevel().canSeeSky(open.above()))))
                .thenExecute(() -> {
                    long[] s = ind().threats().lastSurvey(id[0]);
                    n[0] = s == null ? 0 : (int) s[18];
                    ind().threats().surveySoon(ind().district(id[0]));
                })
                .thenWaitUntil(() -> {
                    long[] s = ind().threats().lastSurvey(id[0]);
                    check("a survey", s != null && s[18] > n[0]);
                    check("three machines, one shielded, one roof in reach (" + s[3] + ", " + s[4] + ", " + s[20] + ")", s[3] == 3 && s[4] == 1 && s[20] == 1);
                    List<int[]> rows = ThreatGameTests.exposedRows(id[0]);
                    int roofless = (int) rows.stream().filter(r -> (r[3] & IndRecords.SurveyBlocks.FLAG_ROOFLESS) != 0).count();
                    check("the shed's roof is listed with sky access", ThreatGameTests.rowFlags(rows, roof)
                            == (IndRecords.SurveyBlocks.FLAG_ROOF | IndRecords.SurveyBlocks.FLAG_ROOFLESS));
                    for (BlockPos m : new BlockPos[] {shed, deep}) {
                        int f = ThreatGameTests.rowFlags(rows, m);
                        check("the covered machine at " + m + " is an exposed block without sky access", f >= 0 && (f & IndRecords.SurveyBlocks.FLAG_ROOFLESS) == 0);
                    }
                    check("the deep cover is not listed", ThreatGameTests.rowFlags(rows, cover) < 0);
                    record(TEST, "survey", "machines " + s[3] + ", shielded " + s[4] + ", roofs in reach " + s[20] + "; " + rows.size() + " exposed blocks, "
                            + roofless + " with sky access; the whole survey " + java.util.Arrays.toString(s));
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
                .thenWaitUntil(io(() -> check("the flyer broke the shed's roof (" + where(helper, id[0]) + ")",
                        helper.getLevel().getBlockState(roof).isAir())))
                .thenExecute(() -> t[1] = helper.getLevel().getGameTime())
                .thenIdle(100) // the whole reconcile round lands
                .thenExecute(() -> {
                    boolean openGone = helper.getLevel().getBlockState(open).isAir();
                    boolean chestGone = helper.getLevel().getBlockState(chest).isAir();
                    for (BlockPos m : new BlockPos[] {shed, deep})
                        check("the covered machine at " + m + " stands (" + helper.getLevel().getBlockState(m) + ")",
                                helper.getLevel().getBlockState(m).is(TestMachines.MACHINE.get()));
                    check("the deep cover still stands", helper.getLevel().getBlockState(cover).is(Blocks.STONE));
                    record(TEST, "flyer", "dragon x8, flying: applied " + (t[1] - t[0]) + " ticks after the order; the shed's roof broken, open machine "
                            + (openGone ? "broken" : "stands") + ", chest " + (chestGone ? "broken" : "stands") + ", both covered machines stand");
                    long[] s = ind().threats().lastSurvey(id[0]);
                    n[0] = s == null ? 0 : (int) s[18];
                    ind().threats().surveySoon(ind().district(id[0]));
                })
                .thenWaitUntil(() -> {
                    long[] s = ind().threats().lastSurvey(id[0]);
                    check("a survey after the raid", s != null && s[18] > n[0]);
                    int f = ThreatGameTests.rowFlags(ThreatGameTests.exposedRows(id[0]), shed);
                    check("the shed's machine is open now (" + f + "; " + java.util.Arrays.toString(s) + ")",
                            f >= 0 && (f & IndRecords.SurveyBlocks.FLAG_ROOFLESS) != 0 && s[4] == 1 && s[20] == 0);
                    record(TEST, "after", "the shed's machine has sky access; " + java.util.Arrays.toString(s));
                    engine.industryThreat(threat + 1, NativeEngine.THREAT_BANDIT, NativeEngine.CLASS_SIEGE, 20, 20.0, id[0], ind().gameTime());
                    t[2] = helper.getLevel().getGameTime();
                })
                .thenWaitUntil(io(() -> check("the siege broke the deep-covered machine (" + helper.getLevel().getBlockState(deep) + "; "
                        + where(helper, id[0]) + ")", helper.getLevel().getBlockState(deep).isAir())))
                .thenExecute(() -> {
                    t[3] = helper.getLevel().getGameTime();
                    record(TEST, "siege", "bandits x20, siege: the deep-covered machine broken " + (t[3] - t[2]) + " ticks after the order (walkers ignore roofs)");
                    set(helper, cover, Blocks.AIR.defaultBlockState());
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
