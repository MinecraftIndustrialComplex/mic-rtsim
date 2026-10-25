package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Districts;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryConfig;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.AfterBatch;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.*;

/**
 * {@code /rtsim district adopt} detects around the chunks it adopted, not over the whole dimension (DS-12). Each
 * test builds the same world, in chunk offsets from the first whole chunk of the yard: district X (0,0) and (1,0), Y
 * (3,0), and a far Z of four chunks at (7..8, 0..1), each with a player-placed machine; and two machines nobody
 * placed: one at (2,0) between X and Y, one alone at (5,3). X's two chunks touch across their border, and so do Z's
 * (an inert block on each side, J1, SECTIONS.md §2.1); nothing touches across any other border, so bare adjacency
 * joins nothing. Adopting around (4,2) with radius 4 must make a district of the bridge's machine and one of the lone
 * machine, both new, leave X and Y as they are and Z as it was, and the districts must be the ones a full detection
 * gives. Every chunk holds 30 inert block entities besides, so a chunk costs 32 to 36 looks.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class AdoptGameTests {
    private AdoptGameTests() {}

    private static final String YARD = "industry_yard";
    private static final int BUDGET = 10;
    private static final int FILLER = 30;
    private static final int[][] PLACED = {{0, 0}, {1, 0}, {3, 0}, {7, 0}, {8, 0}, {7, 1}, {8, 1}};
    private static final int[][] UNADOPTED = {{2, 0}, {5, 3}};
    /** The chunk pairs that touch across their border: X's two, and Z's four sides. */
    private static final int[][] TOUCHING = {{0, 0, 1, 0}, {7, 0, 8, 0}, {7, 1, 8, 1}, {7, 0, 7, 1}, {8, 0, 8, 1}};
    /** The most border blocks a chunk holds: two, in Z. */
    private static final int JOINERS = 2;
    /** The chunks the adopt changes, and so detects around: X, Y and the two new ones, each a chunk of 36 looks at most (a look as a root, one as a reached chunk). */
    private static final int LOCAL_CHUNKS = 5;

    private static BlockPos fill(GameTestHelper helper, int cx, int cz, boolean placed) {
        BlockPos base = inChunk(helper, 0, 0);
        BlockPos origin = new BlockPos(base.getX() + 16 * cx, base.getY(), base.getZ() + 16 * cz);
        for (int i = 0; i < FILLER; i++)
            set(helper, origin.offset(2 + i % 6, 0, 8 + i / 6), TestMachines.INERT.get().defaultBlockState());
        // An inert block on the border toward each chunk this one touches (the other side has its own).
        for (int[] t : TOUCHING)
            for (int side = 0; side < 2; side++) {
                int ax = t[2 * side], az = t[2 * side + 1], bx = t[2 - 2 * side], bz = t[3 - 2 * side];
                if (ax != cx || az != cz)
                    continue;
                int lx = bx > ax ? 15 : bx < ax ? 0 : 10, lz = bz > az ? 15 : bz < az ? 0 : 6;
                set(helper, origin.offset(lx, 0, lz), TestMachines.INERT.get().defaultBlockState());
            }
        BlockPos m = origin.offset(4, 0, 4);
        set(helper, m, TestMachines.MACHINE.get().defaultBlockState());
        if (placed)
            playerPlaced(helper, m);
        return m;
    }

    private static List<ChunkPos> force(GameTestHelper helper, ServerLevel level) {
        BlockPos base = inChunk(helper, 0, 0);
        int bx = base.getX() >> 4, bz = base.getZ() >> 4;
        List<ChunkPos> forced = new ArrayList<>();
        for (int[][] set : new int[][][] {PLACED, UNADOPTED})
            for (int[] c : set) {
                if (level.setChunkForced(bx + c[0], bz + c[1], true))
                    forced.add(new ChunkPos(bx + c[0], bz + c[1]));
                level.getChunk(bx + c[0], bz + c[1]);
            }
        return forced;
    }

    /** The districts without their ids: chunks, seed counts, inventories and stations, one line each, sorted. */
    private static List<String> snapshot() {
        List<String> out = new ArrayList<>();
        for (District d : ind().districts().all()) {
            List<Long> chunks = new ArrayList<>(d.chunks);
            Collections.sort(chunks);
            List<String> seeds = new ArrayList<>();
            d.seedCounts.forEach((c, n) -> seeds.add(c + "=" + n));
            Collections.sort(seeds);
            List<String> invs = new ArrayList<>();
            for (District.Inventory i : d.inventories)
                invs.add(i.pos().toShortString() + "/" + i.kind() + "/" + i.itemSlots() + "/" + i.fluidCapacity() + "/" + i.energyCapacity());
            Collections.sort(invs);
            List<String> stations = new ArrayList<>();
            for (District.Station s : d.stations)
                stations.add(s.pos().toShortString());
            Collections.sort(stations);
            out.add("chunks " + chunks + " seeds " + seeds + " inventories " + invs + " stations " + stations + " name " + d.stationName);
        }
        Collections.sort(out);
        return out;
    }

    private static void freeChunks(ServerLevel level, List<ChunkPos> forced) {
        for (ChunkPos c : forced)
            level.setChunkForced(c.x, c.z, false);
    }

    /** Everything the three tests share: the world built, detected, and the unadopted machines in place. */
    private static BlockPos[] build(GameTestHelper helper, ServerLevel level, List<ChunkPos> forced) {
        forced.addAll(force(helper, level));
        // Earlier tests leave block entities in the chunks around the yard: the adopt would take them too.
        BlockPos centre = inChunk(helper, 0, 0);
        for (int dx = -4; dx <= 4; dx++)
            for (int dz = -4; dz <= 4; dz++) {
                var chunk = level.getChunkSource().getChunkNow((centre.getX() >> 4) + 4 + dx, (centre.getZ() >> 4) + 2 + dz);
                if (chunk != null)
                    for (BlockPos p : new ArrayList<>(chunk.getBlockEntities().keySet()))
                        level.removeBlock(p, false);
            }
        BlockPos[] ms = new BlockPos[PLACED.length + UNADOPTED.length];
        int i = 0;
        for (int[] c : PLACED)
            ms[i++] = fill(helper, c[0], c[1], true);
        for (int[] c : UNADOPTED)
            ms[i++] = fill(helper, c[0], c[1], false);
        return ms;
    }

    /**
     * Districts with a chunk in the area {@link #build} clears. An earlier test's machine just outside it can come
     * back as a district of its own during this one (forgetAll clears the seeds of chunks in districts only), and
     * where it lands depends on how many tests the GameTest grid holds.
     */
    private static int districtsInYard(GameTestHelper helper) {
        BlockPos centre = inChunk(helper, 0, 0);
        int cx = (centre.getX() >> 4) + 4, cz = (centre.getZ() >> 4) + 2;
        int n = 0;
        for (District d : ind().districts().all())
            for (long c : d.chunks)
                if (Math.abs(ChunkPos.getX(c) - cx) <= 4 && Math.abs(ChunkPos.getZ(c) - cz) <= 4) {
                    n++;
                    break;
                }
        return n;
    }

    private static void checkBefore(GameTestHelper helper, BlockPos[] ms) {
        check("a detection is not running", !ind().districts().rescanning());
        int n = districtsInYard(helper);
        check("three districts: X, Y and Z, not " + n, n == 3);
        check("the bridge machine is in no district", districtAt(helper, ms[PLACED.length]) == null);
        check("the lone machine is in no district", districtAt(helper, ms[PLACED.length + 1]) == null);
    }

    private static Districts.Adopted adopt(GameTestHelper helper, ServerLevel level) {
        BlockPos base = inChunk(helper, 0, 0);
        return ind().districts().adoptAround(level, new ChunkPos((base.getX() >> 4) + 4, (base.getZ() >> 4) + 2), 4);
    }

    /** After the adopt's detection: the expected districts, Z untouched, and equal to what a full detection gives. */
    private static void checkAfter(GameTestHelper helper, String test, BlockPos[] ms, long zId, int zRevision, int adoptLimit) {
        Districts districts = ind().districts();
        long looked = districts.lastRescanLooked(), longest = districts.lastRescanLongestLooked();
        int slices = districts.lastRescanSlices();
        List<String> local = snapshot();
        District x = districtAt(helper, ms[0]), y = districtAt(helper, ms[2]), bridge = districtAt(helper, ms[PLACED.length]);
        check("X is its two chunks with two seeds", x != null && x.chunks.size() == 2 && x == districtAt(helper, ms[1]) && x.seeds() == 2);
        check("Y is its one chunk", y != null && y != x && y.chunks.size() == 1 && y.seeds() == 1);
        check("the bridge's machine is a district of its own: nothing joins it to X or Y",
                bridge != null && bridge != x && bridge != y && bridge.chunks.size() == 1 && bridge.seeds() == 1);
        District lone = districtAt(helper, ms[PLACED.length + 1]);
        check("the lone machine is a district of its own", lone != null && lone != x && lone != bridge && lone.chunks.size() == 1 && lone.seeds() == 1);
        District z = districtAt(helper, ms[3]);
        check("Z is the district it was, not redeclared", z != null && z.id == zId && z.revision == zRevision && z.chunks.size() == 4);
        int n = districtsInYard(helper);
        check("five districts, not " + n, n == 5);

        districts.rescanNow(helper.getLevel());
        long fullLooked = districts.lastRescanLooked();
        List<String> full = snapshot();
        record(test, "adopt: ticks / block entities looked at / biggest tick", slices + " / " + looked + " / " + longest);
        record(test, "full detection after it: block entities looked at", fullLooked);
        check("the adopt's districts are a full detection's:\n" + local + "\nagainst\n" + full, local.equals(full));
        check("the adopt looked at " + looked + " block entities, not the " + adoptLimit + " of its own chunks at most", looked <= adoptLimit);
        check("the full detection looked at more than the adopt (" + fullLooked + " against " + looked + ")", fullLooked > looked + 3L * (FILLER + 2));
    }

    /**
     * A budget of 10 block entities a tick: the adopt's detection goes on over several ticks, no tick looks at more
     * than the budget plus one chunk, only the chunks around the adopt are looked at, and the districts are a full
     * detection's.
     */
    @GameTest(template = YARD, batch = "adopt_1", timeoutTicks = 1200)
    public static void anAdoptDetectsAroundItsChunksAndABudgetBoundsEachTick(GameTestHelper helper) {
        String test = "anAdoptDetectsAroundItsChunksAndABudgetBoundsEachTick";
        fastConfig();
        forgetAll(helper);
        ServerLevel level = helper.getLevel();
        List<ChunkPos> forced = new ArrayList<>();
        BlockPos[][] ms = new BlockPos[1][];
        long[] z = new long[2];
        helper.startSequence()
                .thenExecute(() -> ms[0] = build(helper, level, forced))
                .thenWaitUntil(() -> checkBefore(helper, ms[0]))
                .thenExecute(() -> {
                    District d = districtAt(helper, ms[0][3]);
                    z[0] = d.id;
                    z[1] = d.revision;
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, BUDGET);
                    Districts.Adopted a = adopt(helper, level);
                    check("the two machines nobody placed are adopted, not " + a.seeds(), a.seeds() == 2);
                    check("the detection is not done in the command's tick", !a.detected() && ind().districts().rescanning());
                })
                .thenWaitUntil(() -> check("the detection is done", !ind().districts().rescanning()))
                .thenExecute(() -> {
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, null);
                    Districts districts = ind().districts();
                    check("spread over at least four ticks, not " + districts.lastRescanSlices(), districts.lastRescanSlices() >= 4);
                    checkTickBudget(districts, level, BUDGET, ms[0]);
                    checkAfter(helper, test, ms[0], z[0], (int) z[1], LOCAL_CHUNKS * (FILLER + JOINERS + 4));
                    freeChunks(level, forced);
                })
                .thenSucceed();
    }

    /** The default budget: a neighbourhood that fits it is detected in the call itself, which is what the command reports. */
    @GameTest(template = YARD, batch = "adopt_2", timeoutTicks = 600)
    public static void anAdoptThatFitsTheBudgetIsDetectedInTheCall(GameTestHelper helper) {
        String test = "anAdoptThatFitsTheBudgetIsDetectedInTheCall";
        fastConfig();
        forgetAll(helper);
        ServerLevel level = helper.getLevel();
        List<ChunkPos> forced = new ArrayList<>();
        BlockPos[][] ms = new BlockPos[1][];
        long[] z = new long[2];
        helper.startSequence()
                .thenExecute(() -> ms[0] = build(helper, level, forced))
                .thenWaitUntil(() -> checkBefore(helper, ms[0]))
                .thenExecute(() -> {
                    District d = districtAt(helper, ms[0][3]);
                    z[0] = d.id;
                    z[1] = d.revision;
                    // The count budget is what this test is about: the time limit (R2-11) would end the call early on a loaded machine.
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_SLICE_MICROS, 0);
                    Districts.Adopted a = adopt(helper, level);
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_SLICE_MICROS, null);
                    check("two machines adopted, not " + a.seeds(), a.seeds() == 2);
                    check("done in the call, nothing left running", a.detected() && !ind().districts().rescanning());
                    check("in one tick", ind().districts().lastRescanSlices() == 1);
                    checkAfter(helper, test, ms[0], z[0], (int) z[1], LOCAL_CHUNKS * (FILLER + JOINERS + 4));
                    freeChunks(level, forced);
                })
                .thenSucceed();
    }

    /**
     * A detection of the dimension is already under way, and has not looked at the adopted chunks: the adopt does
     * not start a second one beside it, the chunks join the one under way (SG55), and the districts end up a full
     * detection's. The interval between passes is a long one, so a pass of their own after the running one
     * would not come in time; and the one detection is the only one that finishes.
     */
    @GameTest(template = YARD, batch = "adopt_3", timeoutTicks = 1200)
    public static void anAdoptBesideARunningDetectionJoinsIt(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        ServerLevel level = helper.getLevel();
        List<ChunkPos> forced = new ArrayList<>();
        BlockPos[][] ms = new BlockPos[1][];
        long[] rescans = {0};
        helper.startSequence()
                .thenExecute(() -> ms[0] = build(helper, level, forced))
                .thenWaitUntil(() -> checkBefore(helper, ms[0]))
                .thenExecute(() -> {
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, BUDGET);
                    ind().districts().markDirty(level.dimension(), chunk(ms[0][3]));
                })
                .thenWaitUntil(() -> check("a detection of Z is under way", ind().districts().rescanning()))
                .thenExecute(() -> {
                    rescans[0] = ind().districts().rescans();
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_INTERVAL, 100_000);
                    Districts.Adopted a = adopt(helper, level);
                    check("two machines adopted, not " + a.seeds(), a.seeds() == 2);
                    check("not detected in the call", !a.detected());
                    check("the one detection goes on", ind().districts().rescanning());
                })
                .thenWaitUntil(() -> {
                    District bridge = districtAt(helper, ms[0][PLACED.length]);
                    check("the running detection made the bridge's district and the lone machine's",
                            !ind().districts().rescanning() && bridge != null && bridge != districtAt(helper, ms[0][0])
                                    && districtAt(helper, ms[0][PLACED.length + 1]) != null);
                })
                .thenExecute(() -> {
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, null);
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_INTERVAL, 1);
                    check("one detection finished, not " + (ind().districts().rescans() - rescans[0]), ind().districts().rescans() - rescans[0] == 1);
                    check("it looked at the adopted chunks and the districts around them: " + ind().districts().lastRescanLooked(),
                            ind().districts().lastRescanLooked() >= 6 * FILLER);
                    List<String> local = snapshot();
                    ind().districts().rescanNow(level);
                    List<String> full = snapshot();
                    check("the districts are a full detection's:\n" + local + "\nagainst\n" + full, local.equals(full));
                    freeChunks(level, forced);
                })
                .thenSucceed();
    }

    /**
     * The detection under way has counted a chunk's seeds already (the bridge, a dirty chunk of its own) when the
     * adopt gives it one: the chunk's seeds are counted again where they stand, and the chunk is rooted, in the same
     * detection: no pass of its own, after the running one or after the interval, which is a long one here. The
     * lone machine's chunk, not looked at yet, joins as a root.
     */
    @GameTest(template = YARD, batch = "adopt_4", timeoutTicks = 1200)
    public static void anAdoptOfAChunkTheRunningDetectionCountedIsCountedAgain(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        ServerLevel level = helper.getLevel();
        List<ChunkPos> forced = new ArrayList<>();
        BlockPos[][] ms = new BlockPos[1][];
        long[] rescans = {0};
        helper.startSequence()
                .thenExecute(() -> ms[0] = build(helper, level, forced))
                .thenWaitUntil(() -> checkBefore(helper, ms[0]))
                .thenExecute(() -> {
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, BUDGET);
                    // The bridge's chunk is the first one the detection looks at: it counts its seeds (none) in its first tick.
                    ind().districts().markDirty(level.dimension(), chunk(ms[0][PLACED.length]));
                })
                .thenWaitUntil(() -> check("a detection around the bridge is under way", ind().districts().rescanning()))
                .thenExecute(() -> {
                    rescans[0] = ind().districts().rescans();
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_INTERVAL, 100_000);
                    Districts.Adopted a = adopt(helper, level);
                    check("two machines adopted, not " + a.seeds(), a.seeds() == 2);
                    check("not detected in the call", !a.detected());
                    check("the one detection goes on", ind().districts().rescanning());
                })
                .thenWaitUntil(() -> {
                    District bridge = districtAt(helper, ms[0][PLACED.length]);
                    check("the running detection made the bridge's district and the lone machine's",
                            !ind().districts().rescanning() && bridge != null && bridge != districtAt(helper, ms[0][0])
                                    && districtAt(helper, ms[0][PLACED.length + 1]) != null);
                })
                .thenExecute(() -> {
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, null);
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_INTERVAL, 1);
                    check("one detection finished, not " + (ind().districts().rescans() - rescans[0]), ind().districts().rescans() - rescans[0] == 1);
                    List<String> local = snapshot();
                    ind().districts().rescanNow(level);
                    List<String> full = snapshot();
                    check("the districts are a full detection's:\n" + local + "\nagainst\n" + full, local.equals(full));
                    freeChunks(level, forced);
                })
                .thenSucceed();
    }

    /** A failed test leaves its overrides behind: the next batch starts from the fast configuration the other tests assume. */
    @AfterBatch(batch = "adopt_3")
    public static void resetAfterJoin(ServerLevel level) {
        IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, null);
        IndustryConfig.Test.set(IndustryConfig.RESCAN_INTERVAL, 1);
    }

    @AfterBatch(batch = "adopt_4")
    public static void resetAfterRecount(ServerLevel level) {
        resetAfterJoin(level);
    }
}
