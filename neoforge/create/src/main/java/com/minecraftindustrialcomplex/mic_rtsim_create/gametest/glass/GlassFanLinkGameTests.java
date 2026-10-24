package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GraphBuilder;
import com.minecraftindustrialcomplex.mic_rtsim_create.CreatePlugin;
import com.minecraftindustrialcomplex.mic_rtsim_create.Loaded;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.kinetics.fan.EncasedFanBlock;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestInfo;
import net.minecraft.gametest.framework.GameTestListener;
import net.minecraft.gametest.framework.GameTestRunner;
import net.minecraft.server.level.ServerChunkCache;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.TicketType;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.function.Supplier;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.YARD;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.borderX;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.check;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.chunk;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.districtAt;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.fastConfig;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.floorY;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.forget;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.ind;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.midZ;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.paced;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.playerPlaced;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.record;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.at;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.beltMovement;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.motor;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.set;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.speed;

/**
 * A fan's air current is a physical connection for district detection (G2-73): the chunk of a fan joins
 * the district of the hosts its current works (belt segments, depots), and the other way round, whichever
 * side a player placed. Before, nothing crossed a chunk border along a current: a fan whose chunk held no
 * seed and touched no member's block entity was left out, the compile never visited it, and its zones
 * were missing (crushing_wash_blast's 12 nodes instead of 26 when its fans started a chunk column).
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class GlassFanLinkGameTests {
    private GlassFanLinkGameTests() {}

    static String name(long c) {
        return "[" + ChunkPos.getX(c) + ", " + ChunkPos.getZ(c) + "]";
    }

    static String chunks(District d) {
        List<String> out = new ArrayList<>();
        for (long c : d.chunks)
            out.add(name(c));
        out.sort(null);
        return out.toString();
    }

    static List<GraphBuilder.Node> zones(GraphBuilder g) {
        List<GraphBuilder.Node> out = new ArrayList<>();
        for (GraphBuilder.Node n : g.nodes())
            if (n.kind == GlassRecords.Node.MACHINE && n.model == GlassRecords.Model.FAN_ZONE)
                out.add(n);
        return out;
    }

    /**
     * Removes the fans and their motors when the test ends, passed or failed (G2-77). These tests put fans
     * near the yard's east edge, so a fan's chunk can hold the west end of the next batch's yard. Left
     * there, the fan is in that test's district and its link pulls this test's chunks in too. Once this
     * batch ends they are no longer forced, so they are loaded but not block-ticking, and that district's
     * compile waits for them (G2-63): in one stub gate the arm_burners_depots exit test never compiled.
     * A host links only to a fan, so without the fans nothing of this test reaches out; the motors go too,
     * as they share the fans' chunk.
     */
    static void removeAtEnd(GameTestHelper helper, BlockPos... blocks) {
        ServerLevel level = helper.getLevel();
        Runnable remove = () -> {
            for (BlockPos p : blocks)
                if (level.isLoaded(p))
                    level.setBlock(p, Blocks.AIR.defaultBlockState(), 3);
        };
        helper.testInfo.addListener(new GameTestListener() {
            @Override
            public void testStructureLoaded(GameTestInfo info) {}

            @Override
            public void testPassed(GameTestInfo info, GameTestRunner runner) {
                remove.run();
            }

            @Override
            public void testFailed(GameTestInfo info, GameTestRunner runner) {
                remove.run();
            }

            @Override
            public void testAddedForRerun(GameTestInfo oldTest, GameTestInfo newTest, GameTestRunner runner) {}
        });
    }

    /**
     * The bug as the G2 exit test met it: crushing_wash_blast built with its fans (x14) at chunk-local x 0
     * (and the whole build in one chunk row), seeded as the harness seeds it (its stores and its anchor,
     * every one in the belts' chunk). Nothing of the build touches across the border at x13 to x14 (the
     * water and lava there are no block entities), so only the current joins the fans' chunk. The anchor's
     * district must hold it, and the compile must have a fan zone on each of the 7 segments of each belt in
     * a current: 14, in 26 nodes and 24 edges, as at every other alignment.
     */
    @GameTest(template = YARD, batch = "glass_fan_01", timeoutTicks = 1600)
    public static void aFanThatStartsAChunkIsInItsBeltsDistrict(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        BlockPos shift = new BlockPos(Math.floorMod(-(o.getX() + 14), 16), 0, Math.floorMod(-(o.getZ() + 7), 16));
        GlassReplayGameTests.Far h = new GlassReplayGameTests.Far(helper, shift);
        CorpusFactory.Built b = new CrushingWashBlastFactory().build(h);
        GlassReplayGameTests.seed(h, b);
        int za = CrushingWashBlastFactory.LINE_A_Z, zb = CrushingWashBlastFactory.LINE_B_Z;
        BlockPos fanA = at(h, 14, 3, za), fanB = at(h, 14, 3, zb);
        removeAtEnd(helper, fanA, fanB, at(h, 15, 3, za), at(h, 15, 3, zb));
        long fans = chunk(fanA), belts = chunk(at(h, 12, 2, za));
        String t = "glass fan border";
        record(t, "build", "yard origin " + o.toShortString() + ", shifted " + shift.toShortString() + "; fans at " + fanA.toShortString()
                + " and " + fanB.toShortString() + " in chunk " + name(fans) + ", the belts' ends in " + name(belts));
        check("the fans start a chunk column", (fanA.getX() & 15) == 0 && chunk(fanB) == fans && fans != belts);
        check("every seed is in the belts' chunk", b.stores().stream().allMatch(p -> chunk(p) == belts) && chunk(b.anchor()) == belts);
        GraphBuilder[] seen = {null};
        helper.startSequence()
                .thenWaitUntil(() -> {
                    check("the anchor's chunk is a district", districtAt(helper, b.anchor()) != null);
                    check("the fans turn", speed(h, fanA) != 0 && speed(h, fanB) != 0);
                    check("the belts move", beltMovement(h, at(h, 8, 2, za)) != null && beltMovement(h, at(h, 8, 2, zb)) != null);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, b.anchor());
                    check("the anchor's chunk is a district", d != null);
                    check("the anchor's district (chunks " + chunks(d) + ") holds the fans' chunk " + name(fans), d.chunks.contains(fans));
                    GraphBuilder g = ind().glass().lastGraph(d.id);
                    check("a compile of the anchor's district", g != null);
                    int z = zones(g).size();
                    check("14 fan zones in the compile, not " + z + " (" + g.nodeCount() + " nodes, " + g.edges().size() + " edges)", z == 14);
                    seen[0] = g;
                })
                .thenExecute(() -> {
                    GraphBuilder g = seen[0];
                    record(t, "compile", g.nodeCount() + " nodes, " + g.edges().size() + " edges, 14 fan zones; district chunks "
                            + chunks(districtAt(helper, b.anchor())));
                    check("26 nodes and 24 edges, as at every other alignment: " + g.nodeCount() + " and " + g.edges().size(),
                            g.nodeCount() == 26 && g.edges().size() == 24);
                })
                .thenSucceed();
    }

    /** A depot, an empty chunk, then a washing fan blowing back at it across that chunk (see {@link #gap}). */
    record Gap(BlockPos depot, BlockPos fan, long depotChunk, long gapChunk, long fanChunk) {}

    /**
     * The depot at chunk-local x 15 of one chunk, the fan at chunk-local x 0 two chunks east facing west
     * at 256 RPM (reach 20), one block up: the depot is 17 blocks along the current, one below it. The
     * water at the fan's front (held by glass and iron bars) makes the zone washing. The chunk between
     * holds no block entity.
     */
    static Gap gap(GameTestHelper helper) {
        int bx = borderX(helper), y = floorY(helper), z = midZ(helper);
        BlockPos depot = new BlockPos(bx - 1, y, z), fan = new BlockPos(bx + 16, y + 1, z);
        set(helper, depot, AllBlocks.DEPOT.getDefaultState());
        set(helper, fan, AllBlocks.ENCASED_FAN.getDefaultState().setValue(EncasedFanBlock.FACING, Direction.WEST));
        motor(helper, fan.east(), Direction.WEST, CrushingWashBlastFactory.FAN_RPM);
        BlockPos water = fan.west();
        set(helper, water.below(), Blocks.GLASS.defaultBlockState());
        set(helper, water.north(), Blocks.GLASS.defaultBlockState());
        set(helper, water.south(), Blocks.GLASS.defaultBlockState());
        set(helper, water.west(), Blocks.IRON_BARS.defaultBlockState());
        set(helper, water, Blocks.WATER.defaultBlockState());
        removeAtEnd(helper, fan, fan.east());
        return new Gap(depot, fan, chunk(depot), chunk(new BlockPos(bx, y, z)), chunk(fan));
    }

    /** One district holds the depot's and the fan's chunks, and its compile has the depot's washing zone. */
    static void gapChecks(GameTestHelper helper, Gap g, String t) {
        helper.startSequence()
                .thenWaitUntil(() -> check("the fan turns", speed(helper, g.fan()) != 0))
                .thenWaitUntil(() -> {
                    District a = districtAt(helper, g.depot()), b = districtAt(helper, g.fan());
                    check("the depot's chunk " + name(g.depotChunk()) + " is a district", a != null);
                    check("the fan's chunk " + name(g.fanChunk()) + " is a district", b != null);
                    check("one district holds the depot's and the fan's chunks: " + chunks(a) + " and " + chunks(b), a == b);
                    GraphBuilder c = ind().glass().lastGraph(a.id);
                    check("a compile of the district", c != null);
                    List<GraphBuilder.Node> z = zones(c);
                    check("one fan zone, on the depot: " + z.size(), z.size() == 1 && z.get(0).x == g.depot().getX()
                            && z.get(0).y == g.depot().getY() && z.get(0).z == g.depot().getZ());
                })
                .thenExecute(() -> {
                    District d = districtAt(helper, g.depot());
                    record(t, "district", chunks(d) + "; the chunk between " + name(g.gapChunk()) + (d.chunks.contains(g.gapChunk())
                            ? " is a member" : " is not a member") + "; compile " + ind().glass().lastGraph(d.id).nodeCount() + " nodes");
                })
                .thenSucceed();
    }

    /** The fan a player placed, the depot not: the fan's current reaches the depot's chunk across the empty one. */
    @GameTest(template = YARD, batch = "glass_fan_02", timeoutTicks = 800)
    public static void aFansCurrentAcrossAChunkJoinsItsDepot(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        Gap g = gap(helper);
        playerPlaced(helper, g.fan());
        gapChecks(helper, g, "glass fan gap, fan placed");
    }

    /** The depot a player placed, the fan not: the depot finds the fan whose current reaches it. */
    @GameTest(template = YARD, batch = "glass_fan_03", timeoutTicks = 800)
    public static void aDepotFindsTheFanThatBlowsAtIt(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        Gap g = gap(helper);
        playerPlaced(helper, g.depot());
        gapChecks(helper, g, "glass fan gap, depot placed");
    }

    /** A ticket that holds a chunk FULL (level 33), not block-ticking: its neighbours stay at 34, not loaded. */
    private static final TicketType<ChunkPos> FULL_ONLY = TicketType.create("mic_rtsim_create_fan_full_only", Comparator.comparingLong(ChunkPos::toLong));

    /**
     * A link never waits for a chunk that is due but still loading (R2-11). A depot at the east edge of a far
     * chunk A, and a fan in the next chunk N blowing west at it across one block of air. Both are let go until
     * N has unloaded (no chunk holder left), then A alone is held FULL, so N is at level 34. In one tick N gets a
     * FULL ticket and the distance manager's updates run, as the next tick's would ({@code save(false)} runs them
     * and polls no task): {@code Level.isLoaded} now passes at the fan while N is not FULL, the state the first
     * detection after a start met in hundreds of chunks. The depot's links, asked at once, must not name the fan,
     * and N must still not be FULL: before the fix the ray's {@code getBlockState} waited for N's load, and
     * every other chunk load due ran meanwhile. Once N has loaded, the same question names the fan.
     */
    @GameTest(template = YARD, batch = "glass_fan_04", timeoutTicks = 4000)
    public static void aLinkNeverWaitsForAChunkStillLoading(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        ServerChunkCache cache = level.getChunkSource();
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        int ax = (o.getX() >> 4) + 6400, az = (o.getZ() >> 4) + 6400, nx = ax + 1, y = 180, z = az * 16 + 8;
        ChunkPos a = new ChunkPos(ax, az), n = new ChunkPos(nx, az);
        BlockPos depot = new BlockPos(ax * 16 + 15, y, z), fan = new BlockPos(nx * 16 + 1, y, z);
        CreatePlugin plugin = new CreatePlugin();
        BlockEntity[] oldFan = new BlockEntity[1];
        Supplier<List<BlockPos>> links = () -> {
            BlockEntity be = Loaded.blockEntity(level, depot);
            check("the depot's block entity", be != null);
            List<BlockPos> out = new ArrayList<>();
            plugin.links(be, out::add);
            return out;
        };
        helper.startSequence()
                .thenExecute(() -> {
                    level.setChunkForced(ax, az, true);
                    level.setChunkForced(nx, az, true);
                })
                .thenWaitUntil(paced(() -> check("A and N load", cache.getChunkNow(ax, az) != null && cache.getChunkNow(nx, az) != null)))
                .thenExecute(() -> {
                    level.setBlock(depot, AllBlocks.DEPOT.getDefaultState(), 3);
                    level.setBlock(depot.east(), Blocks.AIR.defaultBlockState(), 3);
                    level.setBlock(fan, AllBlocks.ENCASED_FAN.getDefaultState().setValue(EncasedFanBlock.FACING, Direction.WEST), 3);
                    List<BlockPos> got = links.get();
                    check("both loaded, the depot finds the fan: " + got, got.equals(List.of(fan)));
                    oldFan[0] = Loaded.blockEntity(level, fan);
                    level.setChunkForced(ax, az, false);
                    level.setChunkForced(nx, az, false);
                })
                // Unloaded, not only without a visible holder: a holder still in ChunkMap's pendingUnloads comes back
                // FULL at a new ticket.
                .thenWaitUntil(paced(() -> check("N unloads (the fan's block entity removed, no chunk holder)",
                        oldFan[0].isRemoved() && cache.chunkMap.getVisibleChunkIfPresent(n.toLong()) == null)))
                .thenExecute(() -> cache.addRegionTicket(FULL_ONLY, a, 0, a))
                .thenWaitUntil(paced(() -> check("A loads", Loaded.blockEntity(level, depot) != null)))
                .thenExecute(() -> {
                    check("N is away (level 34)", cache.getChunkNow(nx, az) == null && !level.isLoaded(fan));
                    cache.addRegionTicket(FULL_ONLY, n, 0, n);
                    cache.save(false);
                    check("N is due and still loading: Level.isLoaded passes, getChunkNow does not",
                            level.isLoaded(fan) && cache.getChunkNow(nx, az) == null);
                    List<BlockPos> got = links.get();
                    boolean waited = cache.getChunkNow(nx, az) != null;
                    record("aLinkNeverWaitsForAChunkStillLoading", "links while N loads, N FULL after the look", got + ", " + waited);
                    check("the look did not wait for N's load", !waited);
                    check("no link into a chunk still loading: " + got, got.isEmpty());
                })
                .thenWaitUntil(paced(() -> check("N loads", cache.getChunkNow(nx, az) != null)))
                .thenExecute(() -> {
                    List<BlockPos> got = links.get();
                    check("N loaded, the depot finds the fan again: " + got, got.equals(List.of(fan)));
                    level.setBlock(depot, Blocks.AIR.defaultBlockState(), 3);
                    level.setBlock(fan, Blocks.AIR.defaultBlockState(), 3);
                    cache.removeRegionTicket(FULL_ONLY, a, 0, a);
                    cache.removeRegionTicket(FULL_ONLY, n, 0, n);
                })
                .thenSucceed();
    }
}
