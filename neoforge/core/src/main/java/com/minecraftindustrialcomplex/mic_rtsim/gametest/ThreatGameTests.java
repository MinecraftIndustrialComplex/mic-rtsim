package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.ThreatConfig;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.MobSpawnType;
import net.minecraft.world.entity.animal.IronGolem;
import net.minecraft.world.entity.monster.Creeper;
import net.minecraft.world.entity.monster.Skeleton;
import net.minecraft.world.entity.monster.Zombie;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Blocks;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.living.FinalizeSpawnEvent;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.ByteBuffer;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.*;

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

    /** Waits for a survey of the district newer than {@code after} surveys, returns it. */
    private static long[] survey(long district) {
        return ind().threats().lastSurvey(district);
    }

    /**
     * I21's survey on a flat yard: a furnace (a machine) in a district seeded by a chest. In the open the
     * smallest set of columns that seals the furnace is its four neighbours (gaps 4, perimeter 4); a closed
     * cobblestone ring two blocks high seals it (gaps 0); a one-column doorway in the ring is one gap; a stone
     * block over the furnace roofs it; the dark spawnable count is measured (block light 0) and falls when
     * torches light the yard. IND_SURVEY carries each, and IND_SURVEY_BLOCKS lists the wall faces, the
     * machine and the chest.
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
                    set(helper, furnace, Blocks.FURNACE.defaultBlockState());
                    playerPlaced(helper, chest);
                    playerPlaced(helper, furnace);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, furnace);
                    check("the district", d != null && d.chunks.contains(chunk(chest)));
                    id[0] = d.id;
                    ind().threats().surveySoon(d);
                    n[0] = (int) ind().threats().surveys();
                })
                .thenWaitUntil(() -> {
                    check("a survey", ind().threats().surveys() > n[0] && survey(id[0]) != null);
                    long[] s = survey(id[0]);
                    record("theDefenceSurveyMeasuresWallsRoofsAndDark", "open (" + engineName() + ")", java.util.Arrays.toString(s));
                    check("one machine, not roofed", s[3] == 1 && s[4] == 0);
                    check("open: 4 gaps of a 4-column perimeter, got " + s[1] + " of " + s[0], s[1] == 4 && s[0] == 4);
                    check("dark spawnable blocks measured: " + s[2], s[2] > 0);
                    open[0] = s[2];
                    // A closed ring of cobblestone, two high, at Chebyshev distance 2 around the furnace.
                    for (int dx = -2; dx <= 2; dx++)
                        for (int dz = -2; dz <= 2; dz++)
                            if (Math.max(Math.abs(dx), Math.abs(dz)) == 2)
                                for (int y = 0; y < 2; y++)
                                    set(helper, furnace.offset(dx, y, dz), Blocks.COBBLESTONE.defaultBlockState());
                    ind().threats().surveySoon(ind().district(id[0]));
                    n[0] = (int) ind().threats().surveys();
                })
                .thenWaitUntil(() -> {
                    check("a second survey", ind().threats().surveys() > n[0]);
                    long[] s = survey(id[0]);
                    record("theDefenceSurveyMeasuresWallsRoofsAndDark", "closed ring", java.util.Arrays.toString(s));
                    check("a closed ring: 0 gaps, got " + s[1], s[1] == 0);
                    check("the perimeter a wall needs is still 4", s[0] == 4);
                    closed[0] = s[1];
                    set(helper, furnace.offset(2, 0, 0), Blocks.AIR.defaultBlockState());
                    set(helper, furnace.offset(2, 1, 0), Blocks.AIR.defaultBlockState());
                    ind().threats().surveySoon(ind().district(id[0]));
                    n[0] = (int) ind().threats().surveys();
                })
                .thenWaitUntil(() -> {
                    check("a third survey", ind().threats().surveys() > n[0]);
                    long[] s = survey(id[0]);
                    record("theDefenceSurveyMeasuresWallsRoofsAndDark", "doorway", java.util.Arrays.toString(s));
                    check("a one-column doorway: 1 gap, got " + s[1], s[1] == 1);
                    door[0] = s[1];
                    set(helper, furnace.above(2), Blocks.STONE.defaultBlockState());
                    ind().threats().surveySoon(ind().district(id[0]));
                    n[0] = (int) ind().threats().surveys();
                })
                .thenWaitUntil(() -> {
                    check("a fourth survey", ind().threats().surveys() > n[0]);
                    long[] s = survey(id[0]);
                    check("the furnace is roofed", s[4] == 1);
                    roofed[0] = s[4];
                    for (int x = 0; x < 16; x += 4)
                        for (int z = 0; z < 16; z += 4)
                            if (Math.abs(x - 8) > 3 || Math.abs(z - 8) > 3)
                                set(helper, inChunk(helper, x, z), Blocks.TORCH.defaultBlockState());
                    ind().threats().surveySoon(ind().district(id[0]));
                    n[0] = (int) ind().threats().surveys();
                })
                .thenWaitUntil(() -> {
                    check("a fifth survey", ind().threats().surveys() > n[0]);
                    long[] s = survey(id[0]);
                    record("theDefenceSurveyMeasuresWallsRoofsAndDark", "lit", java.util.Arrays.toString(s));
                    check("torches leave fewer dark blocks: " + s[2] + " < " + open[0], s[2] < open[0]);
                    lit[0] = s[2];
                    List<ByteBuffer> surveys = received(IndRecords.IND_SURVEY, IndRecords.Survey.DISTRICT, id[0]);
                    check("IND_SURVEY records", surveys.size() >= 5);
                    ByteBuffer last = surveys.get(surveys.size() - 1);
                    check("the record carries the last survey", last.getInt(IndRecords.Survey.PERIMETER_GAPS) == 1
                            && last.getInt(IndRecords.Survey.ROOFED_MACHINES) == 1 && last.getInt(IndRecords.Survey.DARK_SPAWNABLE) == lit[0]);
                    check("mobGriefing flag", (aux(last) & IndRecords.Survey.FLAG_MOB_GRIEFING) != 0
                            == helper.getLevel().getGameRules().getBoolean(net.minecraft.world.level.GameRules.RULE_MOBGRIEFING));
                    List<ByteBuffer> blocks = received(IndRecords.IND_SURVEY_BLOCKS, IndRecords.SurveyBlocks.DISTRICT, id[0]);
                    ByteBuffer part = blocks.get(blocks.size() - 1);
                    int perim = 0, machine = 0, inv = 0;
                    for (int i = 0; i < aux(part); i++) {
                        int f = part.getInt(IndRecords.SurveyBlocks.BLOCKS + i * IndRecords.SurveyBlocks.BLOCK + IndRecords.SurveyBlocks.B_FLAGS);
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
                    set(helper, chest, Blocks.CHEST.defaultBlockState());
                    playerPlaced(helper, chest);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, chest);
                    check("the district", d != null);
                    id[0] = d.id;
                    ind().threats().surveySoon(d);
                })
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
                    set(helper, chest, Blocks.CHEST.defaultBlockState());
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
