package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.industry.ThreatConfig;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.MobSpawnType;
import net.minecraft.world.entity.animal.IronGolem;
import net.minecraft.world.entity.monster.Creeper;
import net.minecraft.world.entity.monster.Skeleton;
import net.minecraft.world.entity.monster.Zombie;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LightLayer;
import net.minecraft.world.level.block.Blocks;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.common.util.BlockSnapshot;
import net.neoforged.neoforge.common.util.FakePlayer;
import net.neoforged.neoforge.common.util.FakePlayerFactory;
import net.neoforged.neoforge.event.entity.living.FinalizeSpawnEvent;
import net.neoforged.neoforge.event.level.BlockEvent;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.ByteBuffer;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.*;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.io;

/**
 * The threat sampler (I20, I21; INDUSTRY.md §13.4, §13.5) on vanilla alone: the defence survey, natural
 * spawn samples and real fights, and vanilla's IND_CONDITIONS. They read only what Java sends, so they run
 * on either engine; each has its own batch.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class ThreatGameTests {
    private ThreatGameTests() {}

    private static final String YARD = "industry_yard";

    /** How many surveys of the district finished this session. */
    private static int surveyCount(long district) {
        long[] s = ind().threats().lastSurvey(district);
        return s == null ? 0 : (int) s[18];
    }

    /** The district's last survey. */
    private static long[] survey(long district) {
        return ind().threats().lastSurvey(district);
    }

    /**
     * The district's exposed blocks as the engine holds them, from every IND_SURVEY_BLOCKS sent for it in
     * order: a chunk's part 0 replaces that chunk's rows (a partial survey keeps its wall faces), its later
     * parts add (threat.rs {@code Defence::survey_blocks}). Each row is {x, y, z, flags}.
     */
    static List<int[]> exposedRows(long district) {
        java.util.Map<Long, List<int[]>> byChunk = new java.util.LinkedHashMap<>();
        for (ByteBuffer r : received(IndRecords.IND_SURVEY_BLOCKS, IndRecords.SurveyBlocks.DISTRICT, district)) {
            long c = net.minecraft.world.level.ChunkPos.asLong(r.getInt(IndRecords.SurveyBlocks.CHUNK_X), r.getInt(IndRecords.SurveyBlocks.CHUNK_Z));
            List<int[]> rows = byChunk.computeIfAbsent(c, k -> new java.util.ArrayList<>());
            if (r.getInt(IndRecords.SurveyBlocks.PART) == 0) {
                boolean partial = (r.getInt(IndRecords.SurveyBlocks.FLAGS) & IndRecords.SurveyBlocks.PARTIAL) != 0;
                rows.removeIf(row -> !partial || (row[3] & IndRecords.SurveyBlocks.FLAG_PERIMETER) == 0);
            }
            for (int i = 0; i < aux(r); i++) {
                int at = IndRecords.SurveyBlocks.BLOCKS + i * IndRecords.SurveyBlocks.BLOCK;
                rows.add(new int[] {r.getInt(at + IndRecords.SurveyBlocks.B_X), r.getInt(at + IndRecords.SurveyBlocks.B_Y),
                        r.getInt(at + IndRecords.SurveyBlocks.B_Z), r.getInt(at + IndRecords.SurveyBlocks.B_FLAGS)});
            }
        }
        List<int[]> all = new java.util.ArrayList<>();
        byChunk.values().forEach(all::addAll);
        return all;
    }

    /** The flags of the exposed row at {@code p}, or -1 when none is listed there. */
    static int rowFlags(List<int[]> rows, BlockPos p) {
        for (int[] r : rows)
            if (r[0] == p.getX() && r[1] == p.getY() && r[2] == p.getZ())
                return r[3];
        return -1;
    }

    /**
     * I21's survey on a flat yard: a machine in a district whose stock is a chest (the machine seeds it, DS-10). In the open the
     * smallest set of columns that seals the furnace is its four neighbours (gaps 4, perimeter 4); a closed
     * cobblestone ring two blocks above the floor seals it (gaps 0); a one-column doorway is one gap; a stone
     * block three over the furnace is a roof a flyer reaches (OGB-7): the machine counts as open and the stone
     * is listed as a roof row; a second stone ten over it is cover beyond reach, which roofs the machine and
     * drops the roof row; the dark spawnable count is measured (block light 0) and falls when torches light
     * the yard. IND_SURVEY carries each, and IND_SURVEY_BLOCKS lists the wall faces, the machine and the chest.
     */
    @GameTest(template = YARD, batch = "threat_1", timeoutTicks = 1200)
    public static void theDefenceSurveyMeasuresWallsRoofsAndDark(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        ThreatConfig.setForTest(ThreatConfig.SURVEY_MIN_INTERVAL, 1);
        ThreatConfig.setForTest(ThreatConfig.SURVEY_CHUNKS_PER_TICK, 64);
        BlockPos furnace = inChunk(helper, 8, 8);
        BlockPos chest = inChunk(helper, 1, 1);
        long[] id = {0};
        long[] open = {0}, closed = {0}, door = {0}, roofed = {0}, lit = {0};
        int[] n = {0};
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, chest, Blocks.CHEST.defaultBlockState());
                    set(helper, furnace, TestMachines.MACHINE.get().defaultBlockState());
                    playerPlaced(helper, chest);
                    playerPlaced(helper, furnace);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, furnace);
                    check("the district", d != null && d.chunks.contains(chunk(chest)));
                    id[0] = d.id;
                })
                .thenExecute(() -> {
                    n[0] = surveyCount(id[0]);
                    ind().threats().surveySoon(ind().district(id[0]));
                })
                .thenWaitUntil(() -> {
                    check("a survey", surveyCount(id[0]) > n[0]);
                    long[] s = survey(id[0]);
                    record("theDefenceSurveyMeasuresWallsRoofsAndDark", "open (" + engineName() + ")", java.util.Arrays.toString(s));
                    check("one machine, not roofed", s[3] == 1 && s[4] == 0);
                    check("open: 4 gaps of a 4-column perimeter, got " + s[1] + " of " + s[0], s[1] == 4 && s[0] == 4);
                    check("dark spawnable blocks measured: " + s[2], s[2] > 0);
                    open[0] = s[2];
                    // A closed ring of cobblestone at Chebyshev distance 2 around the furnace, two blocks above
                    // the yard's floor (the yard's floor is the layer the blocks are placed in, y 0).
                    for (int dx = -2; dx <= 2; dx++)
                        for (int dz = -2; dz <= 2; dz++)
                            if (Math.max(Math.abs(dx), Math.abs(dz)) == 2)
                                for (int y = 0; y <= 2; y++)
                                    set(helper, furnace.offset(dx, y, dz), Blocks.COBBLESTONE.defaultBlockState());
                    n[0] = surveyCount(id[0]);
                    ind().threats().surveySoon(ind().district(id[0]));
                })
                .thenWaitUntil(() -> {
                    check("a second survey", surveyCount(id[0]) > n[0]);
                    long[] s = survey(id[0]);
                    record("theDefenceSurveyMeasuresWallsRoofsAndDark", "closed ring", java.util.Arrays.toString(s) + " around " + ind().threats().lastAround(id[0])
                            + ", block east+2 " + helper.getLevel().getBlockState(furnace.offset(2, 0, 0)));
                    check("a closed ring: 0 gaps, got " + s[1], s[1] == 0);
                    check("the perimeter a wall needs is still 4", s[0] == 4);
                    closed[0] = s[1];
                    set(helper, furnace.offset(2, 1, 0), Blocks.AIR.defaultBlockState());
                    set(helper, furnace.offset(2, 2, 0), Blocks.AIR.defaultBlockState());
                    n[0] = surveyCount(id[0]);
                    ind().threats().surveySoon(ind().district(id[0]));
                })
                .thenWaitUntil(() -> {
                    check("a third survey", surveyCount(id[0]) > n[0]);
                    long[] s = survey(id[0]);
                    record("theDefenceSurveyMeasuresWallsRoofsAndDark", "doorway", java.util.Arrays.toString(s));
                    check("a one-column doorway: 1 gap, got " + s[1], s[1] == 1);
                    door[0] = s[1];
                    set(helper, furnace.above(3), Blocks.STONE.defaultBlockState());
                })
                // "Roofed" is the sky light above the furnace: lighting runs on its own thread, so survey
                // only once it has darkened that block.
                .thenWaitUntil(io(() -> check("the light engine darkened the block above the furnace: "
                        + helper.getLevel().getBrightness(LightLayer.SKY, furnace.above()),
                        !helper.getLevel().canSeeSky(furnace.above()))))
                .thenExecute(() -> {
                    n[0] = surveyCount(id[0]);
                    ind().threats().surveySoon(ind().district(id[0]));
                })
                .thenWaitUntil(() -> {
                    check("a fourth survey", surveyCount(id[0]) > n[0]);
                    long[] s = survey(id[0]);
                    List<int[]> rows = exposedRows(id[0]);
                    int roof = rowFlags(rows, furnace.above(3)), machine = rowFlags(rows, furnace);
                    record("theDefenceSurveyMeasuresWallsRoofsAndDark", "roof in reach", java.util.Arrays.toString(s) + "; roof row flags " + roof + ", machine row flags " + machine);
                    check("a roof three up is in a flyer's reach: not roofed (" + s[4] + "), one roof in reach (" + s[20] + ")", s[4] == 0 && s[20] == 1);
                    check("the roof is a roof row with sky access", roof == (IndRecords.SurveyBlocks.FLAG_ROOF | IndRecords.SurveyBlocks.FLAG_ROOFLESS));
                    check("the machine under it has no sky access", machine >= 0 && (machine & IndRecords.SurveyBlocks.FLAG_ROOFLESS) == 0);
                    // Cover ten up: the top block is past roofReachBlocks (8). The sky light sources the survey
                    // reads change with the block, so no wait for the light engine.
                    set(helper, furnace.above(10), Blocks.STONE.defaultBlockState());
                    n[0] = surveyCount(id[0]);
                    ind().threats().surveySoon(ind().district(id[0]));
                })
                .thenWaitUntil(() -> {
                    check("a fifth survey", surveyCount(id[0]) > n[0]);
                    long[] s = survey(id[0]);
                    check("cover ten up shields: roofed " + s[4] + ", in reach " + s[20], s[4] == 1 && s[20] == 0);
                    List<int[]> rows = exposedRows(id[0]);
                    check("no roof row any more", rows.stream().noneMatch(r -> (r[3] & IndRecords.SurveyBlocks.FLAG_ROOF) != 0));
                    roofed[0] = s[4];
                    for (int x = 0; x < 16; x += 4)
                        for (int z = 0; z < 16; z += 4)
                            if (Math.abs(x - 8) > 3 || Math.abs(z - 8) > 3)
                                set(helper, inChunk(helper, x, z).above(), Blocks.TORCH.defaultBlockState());
                })
                // the light engine spreads the torches' light on its own thread: wait until each one shines
                .thenWaitUntil(io(() -> {
                    for (int x = 0; x < 16; x += 4)
                        for (int z = 0; z < 16; z += 4)
                            if (Math.abs(x - 8) > 3 || Math.abs(z - 8) > 3) {
                                BlockPos t = inChunk(helper, x, z).above();
                                check("the torch at " + t + " is lit: " + helper.getLevel().getBrightness(LightLayer.BLOCK, t),
                                        helper.getLevel().getBrightness(LightLayer.BLOCK, t) >= 10);
                            }
                }))
                .thenExecute(() -> {
                    n[0] = surveyCount(id[0]);
                    ind().threats().surveySoon(ind().district(id[0]));
                })
                .thenWaitUntil(() -> {
                    check("a sixth survey", surveyCount(id[0]) > n[0]);
                    long[] s = survey(id[0]);
                    record("theDefenceSurveyMeasuresWallsRoofsAndDark", "lit", java.util.Arrays.toString(s));
                    check("torches leave fewer dark blocks: " + s[2] + " < " + open[0], s[2] < open[0]);
                    lit[0] = s[2];
                    List<ByteBuffer> surveys = received(IndRecords.IND_SURVEY, IndRecords.Survey.DISTRICT, id[0]);
                    check("IND_SURVEY records", surveys.size() >= 6);
                    ByteBuffer last = surveys.get(surveys.size() - 1);
                    check("the record carries the last survey", last.getInt(IndRecords.Survey.PERIMETER_GAPS) == 1
                            && last.getInt(IndRecords.Survey.ROOFED_MACHINES) == 1 && last.getInt(IndRecords.Survey.DARK_SPAWNABLE) == lit[0]);
                    check("mobGriefing flag", (aux(last) & IndRecords.Survey.FLAG_MOB_GRIEFING) != 0
                            == helper.getLevel().getGameRules().getBoolean(net.minecraft.world.level.GameRules.RULE_MOBGRIEFING));
                    int perim = 0, machine = 0, inv = 0;
                    for (int[] row : exposedRows(id[0])) {
                        int f = row[3];
                        if ((f & IndRecords.SurveyBlocks.FLAG_PERIMETER) != 0)
                            perim++;
                        if ((f & IndRecords.SurveyBlocks.FLAG_MACHINE) != 0)
                            machine++;
                        if ((f & IndRecords.SurveyBlocks.FLAG_INVENTORY) != 0)
                            inv++;
                    }
                    record("theDefenceSurveyMeasuresWallsRoofsAndDark", "exposed blocks", "wall faces " + perim + ", machines " + machine + ", inventories " + inv);
                    check("the machine and the chest are exposed blocks", machine == 1 && inv == 1);
                    check("wall faces are exposed blocks", perim > 0);
                    record("theDefenceSurveyMeasuresWallsRoofsAndDark", "summary", "dark open " + open[0] + ", lit " + lit[0] + "; gaps open 4, closed "
                            + closed[0] + ", doorway " + door[0] + "; roofed " + roofed[0]);
                    set(helper, furnace.above(10), Blocks.AIR.defaultBlockState());
                    ThreatConfig.setForTest(ThreatConfig.SURVEY_MIN_INTERVAL, null);
                    ThreatConfig.setForTest(ThreatConfig.SURVEY_CHUNKS_PER_TICK, null);
                })
                .thenSucceed();
    }

    /**
     * F5 (docs/oilgate-cheese-audit.md): a partly loaded district is surveyed where it is loaded. A machine's
     * district gains a far member chunk that is not loaded (as detection keeps one that recorded a seed): no
     * whole survey runs, since the perimeter cut needs every chunk, but a partial one does, and its
     * IND_SURVEY_BLOCKS for the loaded chunk is flagged partial and lists the machine; the unloaded chunk gets
     * no part. A second machine set down in the loaded chunk is in the next partial survey, and the engine's
     * view (each chunk's part 0 replaces its rows) holds both machines, once each.
     */
    @GameTest(template = YARD, batch = "threat_partial", timeoutTicks = 600)
    public static void aPartlyLoadedDistrictIsSurveyedWhereItIsLoaded(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        ThreatConfig.setForTest(ThreatConfig.SURVEY_MIN_INTERVAL, 1);
        ThreatConfig.setForTest(ThreatConfig.SURVEY_CHUNKS_PER_TICK, 64);
        ServerLevel level = helper.getLevel();
        BlockPos machine = inChunk(helper, 4, 4), second = inChunk(helper, 10, 10);
        int cx = machine.getX() >> 4, cz = machine.getZ() >> 4;
        long far = net.minecraft.world.level.ChunkPos.asLong(cx + 9300, cz + 9300);
        long[] id = {0};
        long[] before = new long[2]; // partial surveys, this district's whole surveys
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, machine, TestMachines.MACHINE.get().defaultBlockState());
                    playerPlaced(helper, machine);
                    check("the far chunk is not loaded", level.getChunkSource().getChunkNow(cx + 9300, cz + 9300) == null);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, machine);
                    check("the district", d != null);
                    id[0] = d.id;
                })
                .thenExecute(() -> {
                    ind().districts().addSeededChunkForTest(ind().district(id[0]), far);
                    before[0] = ind().threats().partialSurveys();
                    before[1] = surveyCount(id[0]);
                    ind().threats().surveySoon(ind().district(id[0]));
                })
                .thenWaitUntil(() -> {
                    check("a partial survey", ind().threats().partialSurveys() > before[0]);
                    List<ByteBuffer> parts = received(IndRecords.IND_SURVEY_BLOCKS, IndRecords.SurveyBlocks.DISTRICT, id[0]);
                    // The count moves when the survey runs; its records reach the engine at the next hand-over.
                    check("an IND_SURVEY_BLOCKS part", !parts.isEmpty());
                    ByteBuffer last = parts.get(parts.size() - 1);
                    record("aPartlyLoadedDistrictIsSurveyedWhereItIsLoaded", "partial (" + engineName() + ")", "chunk " + last.getInt(IndRecords.SurveyBlocks.CHUNK_X) + ","
                            + last.getInt(IndRecords.SurveyBlocks.CHUNK_Z) + ", flags " + last.getInt(IndRecords.SurveyBlocks.FLAGS) + ", rows " + aux(last));
                    check("the last part is the loaded chunk's, flagged partial", (last.getInt(IndRecords.SurveyBlocks.FLAGS) & IndRecords.SurveyBlocks.PARTIAL) != 0
                            && last.getInt(IndRecords.SurveyBlocks.CHUNK_X) == cx && last.getInt(IndRecords.SurveyBlocks.CHUNK_Z) == cz);
                    check("no part for the unloaded chunk", parts.stream().noneMatch(r -> r.getInt(IndRecords.SurveyBlocks.CHUNK_X) == cx + 9300));
                    int f = rowFlags(exposedRows(id[0]), machine);
                    check("the machine is listed: " + f, f >= 0 && (f & IndRecords.SurveyBlocks.FLAG_MACHINE) != 0);
                    check("no whole survey of a partly loaded district", surveyCount(id[0]) == before[1]);
                    set(helper, second, TestMachines.MACHINE.get().defaultBlockState());
                    before[0] = ind().threats().partialSurveys();
                    ind().threats().surveySoon(ind().district(id[0]));
                })
                .thenWaitUntil(() -> {
                    check("a second partial survey", ind().threats().partialSurveys() > before[0]);
                    District d = ind().district(id[0]);
                    check("the district still has its unloaded member", d != null && d.chunks.contains(far));
                    List<int[]> rows = exposedRows(id[0]);
                    long atFirst = rows.stream().filter(r -> r[0] == machine.getX() && r[1] == machine.getY() && r[2] == machine.getZ()).count();
                    int f = rowFlags(rows, second);
                    record("aPartlyLoadedDistrictIsSurveyedWhereItIsLoaded", "second", rows.size() + " rows; the first machine listed " + atFirst + " times, the second's flags " + f);
                    check("both machines listed, the first once", atFirst == 1 && f >= 0 && (f & IndRecords.SurveyBlocks.FLAG_MACHINE) != 0);
                    check("still no whole survey", surveyCount(id[0]) == before[1]);
                    ThreatConfig.setForTest(ThreatConfig.SURVEY_MIN_INTERVAL, null);
                    ThreatConfig.setForTest(ThreatConfig.SURVEY_CHUNKS_PER_TICK, null);
                })
                .thenSucceed();
    }

    /**
     * I20: while the district is observed (a player in it at night; a GameTest server has no player, so
     * the observation is started by hand), natural spawns of a zombie and a skeleton in it count as ground
     * melee and ground ranged; a spawner's zombie and a natural zombie far away do not. When the
     * observation ends the sample goes out: the observed ticks, the dark blocks of the district and its
     * ring (from the survey), and the spawns per class.
     */
    @GameTest(template = YARD, batch = "threat_2", timeoutTicks = 800)
    public static void naturalSpawnsAreSampledWhileObserved(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        ThreatConfig.setForTest(ThreatConfig.SURVEY_MIN_INTERVAL, 1);
        ThreatConfig.setForTest(ThreatConfig.SURVEY_CHUNKS_PER_TICK, 64);
        BlockPos chest = inChunk(helper, 4, 4);
        long[] id = {0};
        long[] dark = {0};
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, chest, TestMachines.MACHINE.get().defaultBlockState());
                    playerPlaced(helper, chest);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, chest);
                    check("the district", d != null);
                    id[0] = d.id;
                })
                .thenExecute(() -> ind().threats().surveySoon(ind().district(id[0])))
                .thenWaitUntil(() -> {
                    long[] s = survey(id[0]);
                    check("a survey with the ring", s != null);
                    dark[0] = s[2] + s[16];
                })
                .thenExecute(() -> {
                    District d = ind().district(id[0]);
                    ind().threats().observeForTest(d, 600);
                    spawn(helper, EntityType.ZOMBIE, chest.offset(2, 0, 2), MobSpawnType.NATURAL);
                    spawn(helper, EntityType.SKELETON, chest.offset(3, 0, 1), MobSpawnType.NATURAL);
                    spawn(helper, EntityType.ZOMBIE, chest.offset(1, 0, 3), MobSpawnType.SPAWNER);
                    spawn(helper, EntityType.ZOMBIE, chest.offset(200, 0, 200), MobSpawnType.NATURAL);
                    long[] open = ind().threats().openSample(d.id);
                    record("naturalSpawnsAreSampledWhileObserved", "open sample", java.util.Arrays.toString(open));
                    check("one melee and one ranged counted", open[1] == 1 && open[2] == 1 && open[3] == 0 && open[4] == 0);
                })
                .thenWaitUntil(() -> {
                    List<ByteBuffer> samples = received(IndRecords.IND_SPAWN_SAMPLE, IndRecords.SpawnSample.DISTRICT, id[0]);
                    check("an IND_SPAWN_SAMPLE", !samples.isEmpty());
                    ByteBuffer s = samples.get(samples.size() - 1);
                    record("naturalSpawnsAreSampledWhileObserved", "sample (" + engineName() + ")", "observed " + s.getLong(IndRecords.SpawnSample.OBSERVED_TICKS)
                            + " ticks, dark " + s.getLong(IndRecords.SpawnSample.DARK_BLOCKS) + ", spawns " + s.getInt(IndRecords.SpawnSample.SPAWNS) + "/"
                            + s.getInt(IndRecords.SpawnSample.SPAWNS + 4) + "/" + s.getInt(IndRecords.SpawnSample.SPAWNS + 8) + "/" + s.getInt(IndRecords.SpawnSample.SPAWNS + 12));
                    check("600 observed ticks", s.getLong(IndRecords.SpawnSample.OBSERVED_TICKS) == 600);
                    check("the dark blocks of the district and its ring", s.getLong(IndRecords.SpawnSample.DARK_BLOCKS) == dark[0]);
                    check("spawns 1, 1, 0, 0", s.getInt(IndRecords.SpawnSample.SPAWNS) == 1 && s.getInt(IndRecords.SpawnSample.SPAWNS + 4) == 1
                            && s.getInt(IndRecords.SpawnSample.SPAWNS + 8) == 0 && s.getInt(IndRecords.SpawnSample.SPAWNS + 12) == 0);
                    ThreatConfig.setForTest(ThreatConfig.SURVEY_MIN_INTERVAL, null);
                    ThreatConfig.setForTest(ThreatConfig.SURVEY_CHUNKS_PER_TICK, null);
                })
                .thenSucceed();
    }

    /** Fires the FinalizeSpawnEvent a spawn of {@code type} at {@code pos} would, without adding the mob. */
    private static void spawn(GameTestHelper helper, EntityType<? extends net.minecraft.world.entity.Mob> type, BlockPos pos, MobSpawnType why) {
        net.minecraft.world.entity.Mob mob = type.create(helper.getLevel());
        mob.moveTo(pos.getX() + 0.5, pos.getY(), pos.getZ() + 0.5);
        NeoForge.EVENT_BUS.post(new FinalizeSpawnEvent(mob, helper.getLevel(), mob.getX(), mob.getY(), mob.getZ(),
                helper.getLevel().getCurrentDifficultyAt(pos), why, null, null));
        mob.discard();
    }

    /**
     * I21's observed fights: in a district, an iron golem a player built kills a zombie (melee: one attacker,
     * one killed) and a creeper's explosion breaks blocks (siege). Each class's fight ends after
     * {@code fightGapTicks} without an engagement and goes out as IND_FIGHT.
     */
    @GameTest(template = YARD, batch = "threat_3", timeoutTicks = 800)
    public static void realFightsAreObserved(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        ThreatConfig.setForTest(ThreatConfig.FIGHT_GAP, 20);
        BlockPos chest = inChunk(helper, 2, 2);
        BlockPos boom = inChunk(helper, 10, 10);
        long[] id = {0};
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, chest, TestMachines.MACHINE.get().defaultBlockState());
                    playerPlaced(helper, chest);
                    for (int dx = -1; dx <= 1; dx++)
                        for (int dz = -1; dz <= 1; dz++)
                            set(helper, boom.offset(dx, 0, dz), Blocks.OAK_PLANKS.defaultBlockState());
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, chest);
                    check("the district", d != null && d.chunks.contains(chunk(boom)));
                    id[0] = d.id;
                })
                .thenExecute(() -> {
                    IronGolem golem = EntityType.IRON_GOLEM.create(helper.getLevel());
                    golem.setPlayerCreated(true);
                    golem.moveTo(chest.getX() + 3.5, chest.getY(), chest.getZ() + 0.5);
                    helper.getLevel().addFreshEntity(golem);
                    Zombie zombie = EntityType.ZOMBIE.create(helper.getLevel());
                    zombie.moveTo(chest.getX() + 4.5, chest.getY(), chest.getZ() + 0.5);
                    helper.getLevel().addFreshEntity(zombie);
                    zombie.setTarget(golem);
                    zombie.hurt(helper.getLevel().damageSources().mobAttack(golem), 1000f);
                    Creeper creeper = EntityType.CREEPER.create(helper.getLevel());
                    creeper.moveTo(boom.getX() + 0.5, boom.getY() + 1, boom.getZ() + 0.5);
                    helper.getLevel().addFreshEntity(creeper);
                    helper.getLevel().explode(creeper, boom.getX() + 0.5, boom.getY() + 1, boom.getZ() + 0.5, 3f, Level.ExplosionInteraction.MOB);
                    creeper.discard();
                    golem.discard();
                })
                .thenWaitUntil(() -> {
                    List<ByteBuffer> fights = received(IndRecords.IND_FIGHT, IndRecords.Fight.DISTRICT, id[0]);
                    ByteBuffer melee = null, siege = null;
                    for (ByteBuffer f : fights) {
                        if (aux(f) == IndRecords.ThreatClass.GROUND_MELEE)
                            melee = f;
                        if (aux(f) == IndRecords.ThreatClass.SIEGE)
                            siege = f;
                    }
                    check("a melee fight and a siege fight", melee != null && siege != null);
                    record("realFightsAreObserved", "melee (" + engineName() + ")", "attackers " + melee.getInt(IndRecords.Fight.ATTACKERS) + ", killed "
                            + melee.getInt(IndRecords.Fight.ATTACKERS_KILLED) + ", strength " + melee.getFloat(IndRecords.Fight.ATTACKER_STRENGTH));
                    record("realFightsAreObserved", "siege", "attackers " + siege.getInt(IndRecords.Fight.ATTACKERS) + ", blocks destroyed "
                            + siege.getInt(IndRecords.Fight.BLOCKS_DESTROYED) + ", machines " + siege.getInt(IndRecords.Fight.MACHINES_DESTROYED));
                    check("one zombie, killed", melee.getInt(IndRecords.Fight.ATTACKERS) == 1 && melee.getInt(IndRecords.Fight.ATTACKERS_KILLED) == 1);
                    check("strength 1 per natural mob", melee.getFloat(IndRecords.Fight.ATTACKER_STRENGTH) == 1f);
                    boolean griefing = helper.getLevel().getGameRules().getBoolean(net.minecraft.world.level.GameRules.RULE_MOBGRIEFING);
                    check("the creeper broke blocks (mobGriefing " + griefing + ")", !griefing || siege.getInt(IndRecords.Fight.BLOCKS_DESTROYED) > 0);
                    ThreatConfig.setForTest(ThreatConfig.FIGHT_GAP, null);
                })
                .thenSucceed();
    }

    /**
     * INDUSTRY.md §3.3: machine edits are no change. A Create deployer is a NeoForge FakePlayer and fires the
     * place and break events a player does (on a stress world a busy district's glass compile restarted
     * twice a second, every deployer swing journaled an IND_EDIT, and its defences were surveyed every 200
     * ticks). A FakePlayer placing and breaking a furnace in a district makes no IND_EDIT, no seed and asks
     * for no survey; the same events from a server player make all three.
     */
    @GameTest(template = YARD, batch = "threat_5", timeoutTicks = 400)
    public static void machineEditsAreNoPlayerEdits(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        ServerLevel level = helper.getLevel();
        BlockPos chest = inChunk(helper, 1, 1);
        BlockPos furnace = inChunk(helper, 8, 8);
        BlockPos machine = inChunk(helper, 4, 4);
        ServerPlayer player = FakePlayers.editor(level);
        long[] id = {0};
        int[] edits = {0};
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, chest, Blocks.CHEST.defaultBlockState());
                    set(helper, furnace, TestMachines.MACHINE.get().defaultBlockState());
                    playerPlaced(helper, chest);
                    playerPlaced(helper, furnace);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, furnace);
                    check("the district", d != null && d.chunks.contains(chunk(chest)) && d.chunks.contains(chunk(machine)));
                    id[0] = d.id;
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    edits[0] = received(IndRecords.IND_EDIT, IndRecords.Edit.DISTRICT, id[0]).size();
                    check("no survey asked before the edits", !ind().threats().surveyAsked(id[0]));
                    set(helper, machine, TestMachines.MACHINE.get().defaultBlockState());
                    FakePlayer deployer = FakePlayerFactory.getMinecraft(level);
                    NeoForge.EVENT_BUS.post(new BlockEvent.EntityPlaceEvent(BlockSnapshot.create(level.dimension(), level, machine),
                            level.getBlockState(machine.below()), deployer));
                    NeoForge.EVENT_BUS.post(new BlockEvent.BreakEvent(level, machine, level.getBlockState(machine), deployer));
                    check("a machine's edit asks for no survey", !ind().threats().surveyAsked(id[0]));
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    check("a machine's block is no seed", !level.getChunkAt(machine).getData(IndustryAttachments.SEEDS.get()).contains(machine.asLong()));
                    NeoForge.EVENT_BUS.post(new BlockEvent.BreakEvent(level, machine, level.getBlockState(machine), player));
                    check("a player's edit asks for a survey", ind().threats().surveyAsked(id[0]));
                    playerPlaced(helper, machine);
                })
                .thenWaitUntil(() -> {
                    List<ByteBuffer> all = received(IndRecords.IND_EDIT, IndRecords.Edit.DISTRICT, id[0]);
                    check("two new IND_EDITs", all.size() - edits[0] >= 2);
                    List<ByteBuffer> fresh = all.subList(edits[0], all.size());
                    int players = 0;
                    for (ByteBuffer e : fresh)
                        if (e.getLong(IndRecords.Edit.PLAYER_MOST) == player.getUUID().getMostSignificantBits()
                                && e.getLong(IndRecords.Edit.PLAYER_LEAST) == player.getUUID().getLeastSignificantBits())
                            players++;
                    record("machineEditsAreNoPlayerEdits", "new edits (" + engineName() + ")", fresh.size() + ", the player's " + players);
                    check("only the player's break and placement are edits: " + fresh.size() + " new, " + players + " the player's",
                            fresh.size() == 2 && players == 2);
                    check("a player's machine is a seed", level.getChunkAt(machine).getData(IndustryAttachments.SEEDS.get()).contains(machine.asLong()));
                })
                .thenSucceed();
    }

    /** I12 without a season mod: IND_CONDITIONS says no seasons (season 0 of 1, length 0). */
    @GameTest(template = YARD, batch = "threat_4", timeoutTicks = 200)
    public static void vanillaConditionsHaveNoSeasons(GameTestHelper helper) {
        helper.startSequence()
                .thenExecute(() -> ind().conditions().resend())
                .thenWaitUntil(() -> {
                    List<ByteBuffer> c = received(IndRecords.IND_CONDITIONS, r -> true);
                    check("an IND_CONDITIONS", !c.isEmpty());
                    ByteBuffer last = c.get(c.size() - 1);
                    record("vanillaConditionsHaveNoSeasons", "conditions (" + engineName() + ")", "season " + last.getInt(IndRecords.Conditions.SEASON)
                            + " of " + last.getInt(IndRecords.Conditions.SEASON_COUNT) + ", length " + last.getLong(IndRecords.Conditions.SEASON_LENGTH));
                    check("no seasons", last.getInt(IndRecords.Conditions.SEASON_COUNT) == 1 && last.getLong(IndRecords.Conditions.SEASON_LENGTH) == 0);
                })
                .thenSucceed();
    }
}
