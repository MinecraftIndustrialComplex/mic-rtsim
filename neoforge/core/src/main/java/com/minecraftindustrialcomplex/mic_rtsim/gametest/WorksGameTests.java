package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.village.Village;
import com.minecraftindustrialcomplex.mic_rtsim.village.WorkCatalog;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.animal.IronGolem;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BedPart;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.block.state.properties.DoubleBlockHalf;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.phys.AABB;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.List;
import java.util.Map;
import java.util.TreeMap;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The village money sink in the world (SINK.md): what a works purse buys appears as real blocks and
 * bodies. The scripted test drives the Java side on either engine (a work is a PLACE_HOUSE of the
 * village's own decor template and a SPAWN_RESIDENT at the meeting point); the live ones let the
 * native engine's phase 1 world decide, with the test orders WORKS_GRANT (coin straight into the
 * purse) and WORKS_RAID standing in for a ceiling's slow surplus and a dragon. Native engine only for
 * those: the stub buys nothing.
 *
 * <p>The live tests are one batch: a work's plot search may leave the arena, and no other batch's
 * arena is then next to it. The scripted test is a batch of its own, so its census sees no other test's
 * golem. Chunks around a test are held ({@link ChunkWait}: a ticket, never saved, released when the
 * test ends), and its waits after them are bounded in ticks from the tick they were ready.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class WorksGameTests {
    private WorksGameTests() {}

    static final String PLAINS_LAMP = "minecraft:village/plains/plains_lamp_1";
    static final String BATCH = "mic_rtsim_works";
    static final String BATCH_SCRIPTED = "mic_rtsim_works_scripted";
    /** The live tests' tick budget once their chunks are loaded, for all their in-game waits (the engine's
     * answers, a built lamp, days passing). A passing test ends as soon as it passes. */
    static final int LIVE_TICKS = 24_000;
    /** The scripted test's tick budget once its chunks are loaded. */
    static final int SCRIPTED_TICKS = 600;
    /** Reach of the live tests' held chunks: the works' plot search radius (48) and a lamp's half width. */
    static final int LIVE_REACH = 56;
    /** The vanilla plains village's population guess for 15 pieces: 5 (one lamp, and one golem under threat). */
    static final int PIECES = 15;
    /** A purse grant far above any first work's price: 1,000 coins. */
    static final long GRANT = 1_000_000L;

    private static long engineOp() {
        return java.util.concurrent.ThreadLocalRandom.current().nextLong() & Long.MAX_VALUE;
    }

    private static ScriptedEngine.Received result(ScriptedEngine engine, long op) {
        List<ScriptedEngine.Received> rs = RtsimTests.recordsWith(engine, Obs.COMMAND_RESULT, Obs.CommandResult.OP_ID, op);
        return rs.isEmpty() ? null : rs.get(rs.size() - 1);
    }

    /** The items a box's blocks are, as {@link WorkCatalog#bill} counts a template's (a door or a bed once). */
    static Map<String, Integer> itemsIn(ServerLevel level, BoundingBox box) {
        Map<String, Integer> out = new TreeMap<>();
        for (BlockPos p : BlockPos.betweenClosed(box.minX(), box.minY(), box.minZ(), box.maxX(), box.maxY(), box.maxZ())) {
            BlockState s = level.getBlockState(p);
            if (s.isAir())
                continue;
            if (s.hasProperty(BlockStateProperties.DOUBLE_BLOCK_HALF) && s.getValue(BlockStateProperties.DOUBLE_BLOCK_HALF) == DoubleBlockHalf.UPPER)
                continue;
            if (s.hasProperty(BlockStateProperties.BED_PART) && s.getValue(BlockStateProperties.BED_PART) == BedPart.HEAD)
                continue;
            Item item = s.getBlock().asItem();
            if (item != Items.AIR)
                out.merge(BuiltInRegistries.ITEM.getKey(item).toString(), 1, Integer::sum);
        }
        return out;
    }

    static Map<String, Integer> lampBill(GameTestHelper helper) {
        WorkCatalog.Bill bill = WorkCatalog.bill(helper.getLevel().getServer(), ResourceLocation.parse(PLAINS_LAMP));
        check("vanilla's plains lamp template loads", bill != null);
        return bill.items();
    }

    /** A house of the village whose blocks are the plains lamp's, or null. */
    static Village.House lampOf(GameTestHelper helper, Village v, Map<String, Integer> bill) {
        for (Village.House h : v.houses)
            if (itemsIn(helper.getLevel(), h.box()).equals(bill))
                return h;
        return null;
    }

    static List<IronGolem> golemsNear(ServerLevel level, BlockPos meeting, double r) {
        return level.getEntitiesOfClass(IronGolem.class, new AABB(meeting).inflate(r), IronGolem::isAlive);
    }

    // ---- the Java side, scripted (either engine) ----------------------------------------------------

    /**
     * The Java side of a work (MS-3, MS-11): a lamp is a PLACE_HOUSE of the village's own decor
     * template, searched for near the meeting point and built whole (its jigsaw as its final state), so
     * its blocks are exactly the bill the catalogue priced; a golem is a SPAWN_RESIDENT at the meeting
     * point (flag AT_VILLAGE), standing on the ground beside it, not player made, and the census counts
     * it; a lamp with no free plot in reach is answered NOT_FREE and nothing is built.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = BATCH_SCRIPTED)
    public static void aWorkIsBuiltFromItsTemplateAndAGolemStandsAtTheMeetingPoint(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        ScriptedEngine engine = RtsimTests.engine();
        String dim = RtsimTests.dimension(helper);
        Village v = RtsimTests.adoptArena(helper, "human");
        BlockPos meet = v.meeting;
        Map<String, Integer> bill = lampBill(helper);
        long lamp = engineOp(), golem = engineOp(), buried = engineOp();
        int search = Cmd.PlaceHouse.FLAG_SEARCH | Cmd.PlaceHouse.FLAG_Y_FROM_VILLAGE;
        // Under the arena in the flat world's dirt: no plot within reach is free.
        BlockPos under = new BlockPos(meet.getX(), level.getMinBuildHeight() + 1, meet.getZ());
        ChunkWait waits = ChunkWait.of(helper, SCRIPTED_TICKS);
        ChunkWait.Hold around = waits.hold(meet, 24);

        helper.startSequence()
                // The plot search needs the chunks around: ask only once they are loaded.
                .thenWaitUntil(waits.ready(around))
                .thenExecute(() -> {
                    engine.placeHouse(lamp, v.id, dim, PLAINS_LAMP, null, meet.getX(), 0, meet.getZ(), 1, 0, search, 4, 7, 0);
                    engine.spawnResident(golem, v.id, dim, "minecraft:iron_golem", null, Cmd.SpawnResident.FLAG_AT_VILLAGE, 0, 0, 0, 0);
                    engine.placeHouse(buried, v.id, dim, PLAINS_LAMP, null, under.getX(), under.getY(), under.getZ(), 0, 0,
                            Cmd.PlaceHouse.FLAG_SEARCH, 4, 9, 0);
                })
                .thenWaitUntil(waits.engine(() -> check("three answers", result(engine, lamp) != null && result(engine, golem) != null
                        && result(engine, buried) != null)))
                .thenExecute(() -> {
                    check("the lamp was built", result(engine, lamp).aux() == Obs.CommandResult.APPLIED);
                    Village.House h = v.house(lamp);
                    check("the village knows it (its spacing keeps the next work off it)", h != null);
                    Map<String, Integer> built = itemsIn(level, h.box());
                    record("works", "plains lamp bill", bill);
                    record("works", "built at " + h.box(), built);
                    check("its blocks are its template's, jigsaw as its final state: " + built + " vs " + bill, built.equals(bill));
                    check("it stands on the village's level", h.box().minY() == v.baseY);

                    check("the golem spawned", result(engine, golem).aux() == Obs.CommandResult.APPLIED);
                    List<IronGolem> golems = golemsNear(level, meet, 6);
                    check("one golem beside the meeting point: " + golems.size(), golems.size() == 1);
                    IronGolem g = golems.get(0);
                    record("works", "golem at", g.blockPosition() + " (meeting point " + meet + ")");
                    check("not player made: it defends the village", !g.isPlayerCreated());
                    check("persistent", g.isPersistenceRequired());
                    check("on its feet, not in a block", level.noCollision(g));
                    check("on the arena's floor beside the meeting point, not on the test's barrier roof",
                            g.blockPosition().getY() == meet.getY() && g.blockPosition().distManhattan(meet) <= 6);

                    check("no free plot under the arena: NOT_FREE", result(engine, buried).aux() == Obs.CommandResult.NOT_FREE);
                    check("and nothing built for it", v.house(buried) == null);

                    RtsimTests.session().phase1.census.take(level, v, RtsimTests.session().gameTime());
                })
                // The census sweep (one village every Census.INTERVAL ticks) may have counted this village
                // before the golem spawned, and that record can reach the engine a hand-over ahead of the
                // census above: wait for the latest record to count the golem, not for the first record.
                .thenWaitUntil(waits.engine(() -> {
                    List<ScriptedEngine.Received> census = RtsimTests.recordsWith(engine, Obs.VILLAGE_CENSUS, Obs.VillageCensus.VILLAGE, v.id);
                    int counted = census.isEmpty() ? -1 : census.get(census.size() - 1).view().getInt(Obs.VillageCensus.GOLEMS);
                    check("the census counts the golem: " + (counted < 0 ? "no census reached the engine" : counted), counted == 1);
                }))
                .thenExecute(() -> {
                    golemsNear(level, meet, 6).forEach(net.minecraft.world.entity.Entity::discard);
                    RtsimTests.forget(v);
                })
                .thenSucceed();
    }

    // ---- the engine decides (native) ------------------------------------------------------------------

    private static NativeEngine live(GameTestHelper helper, String test) {
        if (RtsimTests.engine() instanceof NativeEngine e)
            return e;
        record(test, "skipped", "the stub engine buys nothing; this test needs the native engine's phase 1 world");
        helper.succeed();
        return null;
    }

    /**
     * MS-1 to MS-3: a village whose purse affords a lamp buys one, and it is built from the village's
     * own type's template (a plains village gets {@code plains_lamp_1}) by the meeting point; the cap
     * (one per 3 residents) holds after that, and the purse keeps what it did not spend (MS-9).
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = BATCH)
    public static void aVillageBuysALampFromItsOwnTemplateWhenItsPurseAffordsIt(GameTestHelper helper) {
        NativeEngine e = live(helper, "worksLamp");
        if (e == null)
            return;
        Map<String, Integer> bill = lampBill(helper);
        BlockPos meet = BlockPos.containing(RtsimTests.centre(helper));
        Village v = RtsimTests.adoptArenaLive(helper, "human", "minecraft:village_plains", meet, 5, PIECES);
        long[] base = e.phase1Stats();
        long[] asks = {0};
        ChunkWait waits = ChunkWait.of(helper, LIVE_TICKS);
        ChunkWait.Hold around = waits.hold(meet, LIVE_REACH);

        helper.startSequence()
                .thenWaitUntil(waits.ready(around))
                .thenWaitUntil(waits.engine(() -> check("the engine knows the village's works", e.phase1Works(v.id) != null)))
                .thenExecute(() -> e.worksGrant(v.id, GRANT))
                .thenWaitUntil(waits.engine(() -> check("a lamp stands in the village", lampOf(helper, v, bill) != null)))
                .thenWaitUntil(waits.engine(() -> {
                    long[] w = e.phase1Works(v.id);
                    check("the engine heard it was built", e.phase1Stats()[NativeEngine.P1_WORKS_PLACED] > base[NativeEngine.P1_WORKS_PLACED]);
                    check("one work bought", w[NativeEngine.WORKS_BOUGHT] == 1);
                }))
                .thenWaitUntil(waits.engine(() -> {
                    // Two more days: the cap holds.
                    if (asks[0]++ < 2) {
                        e.phase1Advance(24_000);
                        check("days pass", false);
                    }
                }))
                .thenIdle(20)
                .thenExecute(() -> {
                    long[] w = e.phase1Works(v.id);
                    long[] s = e.phase1Stats();
                    Village.House h = lampOf(helper, v, bill);
                    record("worksLamp", "lamp at", h.box() + " (meeting point " + meet + ")");
                    record("worksLamp", "works view (bought, full, purse, golems, tribute, spent)", java.util.Arrays.toString(w));
                    record("worksLamp", "stats bought, placed, refused", (s[NativeEngine.P1_WORKS_BOUGHT] - base[NativeEngine.P1_WORKS_BOUGHT])
                            + ", " + (s[NativeEngine.P1_WORKS_PLACED] - base[NativeEngine.P1_WORKS_PLACED]) + ", "
                            + (s[NativeEngine.P1_WORKS_REFUSED] - base[NativeEngine.P1_WORKS_REFUSED]));
                    check("still one work: the cap (one per 3 of 5 residents) holds", w[NativeEngine.WORKS_BOUGHT] == 1);
                    check("nothing full", w[NativeEngine.WORKS_FULL] == 0);
                    check("it cost something", w[NativeEngine.WORKS_SPENT] > 0);
                    check("the rest stays in the purse (MS-9)", w[NativeEngine.WORKS_PURSE] >= GRANT - w[NativeEngine.WORKS_SPENT]);
                    check("one lamp in the village", v.houses.stream().filter(x -> itemsIn(helper.getLevel(), x.box()).equals(bill)).count() == 1);
                    RtsimTests.forget(v);
                })
                .thenSucceed();
    }

    /**
     * MS-6, MS-8: a raided village buys a golem first; it appears at the meeting point, the census's
     * count of it reaches the engine, and the village's risk to the dragons rises by it.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = BATCH)
    public static void aRaidedVillageBuysAGolemThatTheCensusCounts(GameTestHelper helper) {
        NativeEngine e = live(helper, "worksGolem");
        if (e == null)
            return;
        ServerLevel level = helper.getLevel();
        BlockPos meet = BlockPos.containing(RtsimTests.centre(helper));
        Village v = RtsimTests.adoptArenaLive(helper, "human", "minecraft:village_plains", meet, 5, PIECES);
        IronGolem[] golem = {null};
        long[] risk = {-1};
        ChunkWait waits = ChunkWait.of(helper, LIVE_TICKS);
        ChunkWait.Hold around = waits.hold(meet, LIVE_REACH);

        helper.startSequence()
                .thenWaitUntil(waits.ready(around))
                .thenWaitUntil(waits.engine(() -> check("the engine knows the village's works", e.phase1Works(v.id) != null)))
                .thenExecute(() -> {
                    risk[0] = e.dragonsVillageRisk(v.id);
                    check("the dragons know the village (risk " + risk[0] + ")", risk[0] >= 0);
                    e.worksRaid(v.id, false);
                    e.worksGrant(v.id, GRANT);
                })
                .thenWaitUntil(waits.engine(() -> {
                    List<IronGolem> g = golemsNear(level, meet, 8);
                    check("a golem at the meeting point", g.size() == 1);
                    golem[0] = g.get(0);
                }))
                .thenExecute(() -> RtsimTests.session().phase1.census.take(level, v, RtsimTests.session().gameTime()))
                .thenWaitUntil(waits.engine(() -> {
                    long[] w = e.phase1Works(v.id);
                    check("the engine counts it (census golems " + w[NativeEngine.WORKS_GOLEMS] + ")", w[NativeEngine.WORKS_GOLEMS] == 1);
                    check("the dragons fear the village more (risk " + risk[0] + " to " + e.dragonsVillageRisk(v.id) + ")",
                            e.dragonsVillageRisk(v.id) > risk[0]);
                }))
                .thenExecute(() -> {
                    long[] w = e.phase1Works(v.id);
                    record("worksGolem", "golem at", golem[0].blockPosition() + " (meeting point " + meet + ")");
                    record("worksGolem", "works view (bought, full, purse, golems, tribute, spent)", java.util.Arrays.toString(w));
                    record("worksGolem", "village risk before and after the golem", risk[0] + ", " + e.dragonsVillageRisk(v.id));
                    check("the golem was the first work", w[NativeEngine.WORKS_BOUGHT] >= 1);
                    check("not player made", !golem[0].isPlayerCreated());
                    check("on the meeting point's level", golem[0].blockPosition().getY() == meet.getY());
                    golem[0].discard();
                    RtsimTests.forget(v);
                })
                .thenSucceed();
    }

    /**
     * MS-11: a lamp with no free plot in reach is answered NOT_FREE, the kind is marked full for the
     * village, and the coin stays spent: the village buys no other lamp. The meeting point is buried in
     * the flat world's dirt, so every plot the search tries is refused (the chunks around are held,
     * so none is merely unloaded).
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = BATCH)
    public static void noFreePlotMarksTheKindFull(GameTestHelper helper) {
        NativeEngine e = live(helper, "worksFull");
        if (e == null)
            return;
        ServerLevel level = helper.getLevel();
        BlockPos centre = BlockPos.containing(RtsimTests.centre(helper));
        BlockPos meet = new BlockPos(centre.getX(), level.getMinBuildHeight() + 1, centre.getZ());
        Village v = RtsimTests.adoptArenaLive(helper, "human", "minecraft:village_plains", meet, 5, PIECES);
        long[] base = e.phase1Stats();
        long[] spent = {0};
        long[] asks = {0};
        ChunkWait waits = ChunkWait.of(helper, LIVE_TICKS);
        ChunkWait.Hold around = waits.hold(meet, LIVE_REACH);

        helper.startSequence()
                .thenWaitUntil(waits.ready(around))
                .thenWaitUntil(waits.engine(() -> check("the engine knows the village's works", e.phase1Works(v.id) != null)))
                .thenExecute(() -> e.worksGrant(v.id, GRANT))
                .thenWaitUntil(waits.engine(() -> {
                    long[] w = e.phase1Works(v.id);
                    check("the lamp is full (full " + w[NativeEngine.WORKS_FULL] + ", bought " + w[NativeEngine.WORKS_BOUGHT] + ")",
                            w[NativeEngine.WORKS_FULL] == 1);
                    spent[0] = w[NativeEngine.WORKS_SPENT];
                }))
                .thenWaitUntil(waits.engine(() -> {
                    if (asks[0]++ < 2) {
                        e.phase1Advance(24_000);
                        check("days pass", false);
                    }
                }))
                .thenIdle(20)
                .thenExecute(() -> {
                    long[] w = e.phase1Works(v.id);
                    long[] s = e.phase1Stats();
                    record("worksFull", "works view (bought, full, purse, golems, tribute, spent)", java.util.Arrays.toString(w));
                    check("one lamp bought, no other tried", w[NativeEngine.WORKS_BOUGHT] == 1);
                    check("refused once", s[NativeEngine.P1_WORKS_REFUSED] - base[NativeEngine.P1_WORKS_REFUSED] >= 1);
                    check("no refund: spent " + w[NativeEngine.WORKS_SPENT], w[NativeEngine.WORKS_SPENT] == spent[0] && spent[0] > 0);
                    check("nothing built", v.houses.isEmpty());
                    RtsimTests.forget(v);
                })
                .thenSucceed();
    }
}
