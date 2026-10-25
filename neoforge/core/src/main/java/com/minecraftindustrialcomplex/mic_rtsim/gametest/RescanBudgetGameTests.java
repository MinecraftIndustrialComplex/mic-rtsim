package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Districts;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryConfig;
import net.minecraft.commands.CommandSource;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.AfterBatch;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.level.BlockEvent;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.*;

/**
 * The district detection runs over several ticks (a block entity budget per tick, DECISIONS.md ST3): the
 * district set changes once, when the detection is done; an edit that arrives meanwhile is not lost; a chunk that
 * unloads meanwhile leaves its district whole. Each test sets a budget of 10 block entities a tick and fills its
 * chunks with 30 inert block entities each, so a chunk costs more than a tick's budget, and an inert block on each
 * of its four borders, so that filled chunks side by side touch and are one district (J1, SECTIONS.md §2.1).
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class RescanBudgetGameTests {
    private RescanBudgetGameTests() {}

    private static final String YARD = "industry_yard";
    private static final int BUDGET = 10;
    private static final int FILLER = 30;

    /**
     * Inert blocks on the four borders of a chunk, one each, at the same place in every chunk: two filled chunks side
     * by side touch across their border (J1), so they are one district; diagonal ones join through a filled chunk
     * beside both.
     */
    static final int[][] JOINERS = {{0, 6}, {15, 6}, {10, 0}, {10, 15}};

    /**
     * A machine, {@code FILLER} inert block entities and the {@link #JOINERS} in the chunk whose corner is
     * {@code (cx, cz)} chunks from the first whole chunk.
     */
    private static BlockPos fill(GameTestHelper helper, int cx, int cz, boolean machine) {
        BlockPos base = inChunk(helper, 0, 0);
        BlockPos origin = new BlockPos(base.getX() + 16 * cx, base.getY(), base.getZ() + 16 * cz);
        for (int i = 0; i < FILLER; i++)
            set(helper, origin.offset(2 + i % 6, 0, 8 + i / 6), TestMachines.INERT.get().defaultBlockState());
        for (int[] j : JOINERS)
            set(helper, origin.offset(j[0], 0, j[1]), TestMachines.INERT.get().defaultBlockState());
        BlockPos m = origin.offset(4, 0, 4);
        if (machine) {
            set(helper, m, TestMachines.MACHINE.get().defaultBlockState());
            playerPlaced(helper, m);
        }
        return m;
    }

    private static void breakAndRemove(ServerLevel level, BlockPos pos) {
        NeoForge.EVENT_BUS.post(new BlockEvent.BreakEvent(level, pos, level.getBlockState(pos), FakePlayers.editor(level)));
        level.removeBlock(pos, false);
    }

    /**
     * Four neighbouring chunks, one machine each. The detection takes at least four ticks; while it runs the
     * district set is what it was (no district, none half-built), and when it is done the district has all four.
     */
    @GameTest(template = YARD, batch = "rescan_budget_1", timeoutTicks = 600)
    public static void aBigDetectionSpreadsOverTicksAndChangesTheSetOnce(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, BUDGET);
        int[] busyTicks = {0}, violations = {0};
        int before = ind().districts().all().size();
        BlockPos[] ms = new BlockPos[4];
        helper.startSequence()
                .thenExecute(() -> {
                    for (int i = 0; i < 4; i++)
                        ms[i] = fill(helper, i % 2, i / 2, true);
                })
                .thenWaitUntil(() -> {
                    boolean busy = ind().districts().rescanning();
                    if (busy) {
                        busyTicks[0]++;
                        // Never half-updated: no district yet, and nothing else changed.
                        if (districtAt(helper, ms[0]) != null || ind().districts().all().size() != before)
                            violations[0]++;
                    }
                    check("a detection ran and finished", busyTicks[0] > 0 && !busy && districtAt(helper, ms[0]) != null);
                })
                .thenExecute(() -> {
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, null);
                    District d = districtAt(helper, ms[0]);
                    record("aBigDetectionSpreadsOverTicksAndChangesTheSetOnce", "ticks seen busy", busyTicks[0]);
                    record("aBigDetectionSpreadsOverTicksAndChangesTheSetOnce", "detection ticks / longest tick (us)",
                            ind().districts().lastRescanSlices() + " / " + ind().districts().lastRescanLongestNanos() / 1000);
                    check("the district set was never seen half-updated, " + violations[0] + " times", violations[0] == 0);
                    check("spread over at least four ticks, got " + ind().districts().lastRescanSlices(), ind().districts().lastRescanSlices() >= 4);
                    check("all four chunks, not " + d.chunks.size(), d.chunks.size() == 4);
                    check("four seeds, not " + d.seeds(), d.seeds() == 4);
                })
                .thenSucceed();
    }

    /**
     * An edit that arrives while a detection runs is not lost: a machine placed in a chunk of its own becomes a district,
     * and a machine broken in another dissolves its district, both at the detection after the one under way.
     */
    @GameTest(template = YARD, batch = "rescan_budget_2", timeoutTicks = 800)
    public static void anEditDuringADetectionIsNotLost(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        ServerLevel level = helper.getLevel();
        List<net.minecraft.world.level.ChunkPos> forced = new ArrayList<>();
        BlockPos[] pos = new BlockPos[4]; // two chunks of the big district, a lone one that goes, a lone one that comes
        long[] ids = {0, 0};
        boolean[] acted = {false};
        helper.startSequence()
                .thenExecute(() -> {
                    BlockPos base = inChunk(helper, 0, 0);
                    int bx = base.getX() >> 4, bz = base.getZ() >> 4;
                    for (int[] c : new int[][] {{bx + 4, bz}, {bx + 4, bz + 3}})
                        if (level.setChunkForced(c[0], c[1], true))
                            forced.add(new net.minecraft.world.level.ChunkPos(c[0], c[1]));
                    for (int[] c : new int[][] {{bx + 4, bz}, {bx + 4, bz + 3}})
                        level.getChunk(c[0], c[1]);
                    pos[0] = fill(helper, 0, 0, true);
                    pos[1] = fill(helper, 1, 0, true);
                    pos[2] = fill(helper, 4, 0, true);
                    pos[3] = fill(helper, 4, 3, false);
                })
                .thenWaitUntil(() -> {
                    District big = districtAt(helper, pos[0]), lone = districtAt(helper, pos[2]);
                    check("the big district and the lone one exist", big != null && lone != null && big != lone && !ind().districts().rescanning());
                    check("the big one has two chunks, not " + big.chunks.size(), big.chunks.size() == 2);
                    ids[0] = big.id;
                    ids[1] = lone.id;
                })
                .thenExecute(() -> {
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, BUDGET);
                    ind().districts().markDirty(level.dimension(), chunk(pos[0]));
                })
                .thenWaitUntil(() -> {
                    if (!acted[0]) {
                        check("a detection of the big district is under way", ind().districts().rescanning());
                        acted[0] = true;
                        // Mid-detection: a machine is placed in the fourth chunk, and the lone one is broken.
                        set(helper, pos[3], TestMachines.MACHINE.get().defaultBlockState());
                        playerPlaced(helper, pos[3]);
                        breakAndRemove(level, pos[2]);
                    }
                    check("the detection is done and the next one too", !ind().districts().rescanning());
                    check("the placed machine made a district", districtAt(helper, pos[3]) != null);
                    check("the broken machine's district is gone", ind().district(ids[1]) == null && districtAt(helper, pos[2]) == null);
                })
                .thenExecute(() -> {
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, null);
                    District big = districtAt(helper, pos[0]);
                    check("the big district kept its id and both chunks", big != null && big.id == ids[0] && big.chunks.size() == 2 && big.seeds() == 2);
                    for (net.minecraft.world.level.ChunkPos c : forced)
                        level.setChunkForced(c.x, c.z, false);
                })
                .thenSucceed();
    }

    /**
     * The chunks of a district unload while its detection runs: the district stays whole, with its chunks as
     * unloaded members and their seed counts, whether they went before or after they were looked at.
     */
    @GameTest(template = YARD, batch = "rescan_budget_3", timeoutTicks = 1200)
    public static void aChunkThatUnloadsDuringADetectionKeepsItsDistrict(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        ServerLevel level = helper.getLevel();
        List<net.minecraft.world.level.ChunkPos> forced = new ArrayList<>();
        BlockPos[] pos = new BlockPos[2];
        long[] id = {0};
        boolean[] acted = {false};
        helper.startSequence()
                .thenExecute(() -> {
                    BlockPos base = inChunk(helper, 0, 0);
                    int bx = (base.getX() >> 4) + 6, bz = base.getZ() >> 4;
                    for (int[] c : new int[][] {{bx, bz}, {bx + 1, bz}})
                        if (level.setChunkForced(c[0], c[1], true))
                            forced.add(new net.minecraft.world.level.ChunkPos(c[0], c[1]));
                    for (int[] c : new int[][] {{bx, bz}, {bx + 1, bz}})
                        level.getChunk(c[0], c[1]);
                    pos[0] = fill(helper, 6, 0, true);
                    pos[1] = fill(helper, 7, 0, true);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, pos[0]);
                    check("the district has both far chunks", d != null && d.chunks.size() == 2 && !ind().districts().rescanning());
                    id[0] = d.id;
                })
                .thenExecute(() -> {
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, BUDGET);
                    ind().districts().markDirty(level.dimension(), chunk(pos[0]));
                })
                .thenWaitUntil(() -> {
                    if (!acted[0]) {
                        check("a detection is under way", ind().districts().rescanning());
                        acted[0] = true;
                        // Both go (a forced chunk's neighbour stays loaded at the border while the other is forced): at some tick of the detection.
                        for (net.minecraft.world.level.ChunkPos c : forced)
                            level.setChunkForced(c.x, c.z, false);
                    }
                    check("the detection is done", !ind().districts().rescanning());
                    check("both chunks have unloaded", level.getChunkSource().getChunkNow(pos[0].getX() >> 4, pos[0].getZ() >> 4) == null
                            && level.getChunkSource().getChunkNow(pos[1].getX() >> 4, pos[1].getZ() >> 4) == null);
                })
                .thenExecute(() -> {
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, null);
                    ind().districts().rescanNow(level);
                    District d = ind().district(id[0]);
                    check("the district is still there with both chunks", d != null && d.chunks.size() == 2 && d.chunks.contains(chunk(pos[1])));
                    check("both seeds still counted: " + d.seedCounts, d.seeds() == 2);
                })
                .thenSucceed();
    }

    /** Commands and tests ask for a detection now: it finishes in the one call, whatever the budget. */
    @GameTest(template = YARD, batch = "rescan_budget_4", timeoutTicks = 200)
    public static void rescanNowIgnoresTheBudget(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, 1);
        BlockPos[] ms = new BlockPos[2];
        helper.startSequence()
                .thenExecute(() -> {
                    ms[0] = fill(helper, 0, 0, true);
                    ms[1] = fill(helper, 1, 0, true);
                })
                .thenIdle(2)
                .thenExecute(() -> {
                    ind().districts().rescanNow(helper.getLevel());
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, null);
                    check("one call, one district of two chunks", districtAt(helper, ms[0]) != null && districtAt(helper, ms[0]).chunks.size() == 2);
                    check("and nothing left running", !ind().districts().rescanning());
                })
                .thenSucceed();
    }

    /**
     * What spreading buys: a district of nine chunks with about 200 inert block entities each (1,800 in all, four of
     * each chunk's on its borders, so the chunks touch),
     * detected in one call (the old way: one tick) and at the default budget. Logs both; passes when the
     * default budget's longest tick is under the one-call time (a figure for the report, not a timing gate).
     */
    @GameTest(template = YARD, batch = "rescan_budget_5", timeoutTicks = 1200)
    public static void aBigDetectionsLongestTickIsBelowTheOneCallTime(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        ServerLevel level = helper.getLevel();
        long[] oneCall = new long[3];
        long[] spread = new long[4]; // cold: slices, longest tick; warm: slices, longest tick
        helper.startSequence()
                .thenExecute(() -> {
                    BlockPos base = inChunk(helper, 0, 0);
                    for (int cx = 0; cx < 3; cx++)
                        for (int cz = 0; cz < 3; cz++) {
                            BlockPos o = new BlockPos(base.getX() + 16 * cx, base.getY(), base.getZ() + 16 * cz);
                            for (int x = 1; x < 15; x++)
                                for (int z = 1; z < 15; z++)
                                    set(helper, o.offset(x, 0, z), TestMachines.INERT.get().defaultBlockState());
                            // One inert block on each border, so the nine chunks touch and are one district (J1).
                            for (int[] j : new int[][] {{0, 7}, {15, 7}, {7, 0}, {7, 15}})
                                set(helper, o.offset(j[0], 0, j[1]), TestMachines.INERT.get().defaultBlockState());
                            set(helper, o, TestMachines.MACHINE.get().defaultBlockState());
                            playerPlaced(helper, o);
                        }
                })
                .thenWaitUntil(() -> {
                    check("the district is detected", !ind().districts().rescanning() && ind().districts().all().size() == 1);
                    spread[0] = ind().districts().lastRescanSlices();
                    spread[1] = ind().districts().lastRescanLongestNanos();
                })
                .thenExecute(() -> {
                    for (int i = 0; i < 3; i++) {
                        long t0 = System.nanoTime();
                        ind().districts().rescanNow(level);
                        oneCall[i] = System.nanoTime() - t0;
                    }
                    ind().districts().markDirty(level.dimension(), chunk(inChunk(helper, 1, 1)));
                })
                .thenWaitUntil(() -> {
                    check("the second spread detection is done", !ind().districts().rescanning() && ind().districts().lastRescanSlices() > 1);
                    spread[2] = ind().districts().lastRescanSlices();
                    spread[3] = ind().districts().lastRescanLongestNanos();
                })
                .thenExecute(() -> {
                    java.util.Arrays.sort(oneCall);
                    String t = "aBigDetectionsLongestTickIsBelowTheOneCallTime";
                    record(t, "one call, three runs (us)", oneCall[0] / 1000 + " / " + oneCall[1] / 1000 + " / " + oneCall[2] / 1000);
                    record(t, "default budget, first (cold): ticks, longest tick (us)", spread[0] + ", " + spread[1] / 1000);
                    record(t, "default budget, later (warm): ticks, longest tick (us)", spread[2] + ", " + spread[3] / 1000);
                    check("spread over several ticks, got " + spread[2], spread[2] >= 4);
                    check("the longest spread tick (" + spread[3] / 1000 + " us) is under the one-call time (" + oneCall[1] / 1000 + " us)", spread[3] < oneCall[1]);
                })
                .thenSucceed();
    }
    // ---- the commands (DS-16, SG60) -------------------------------------------------------------

    /** What a command says to its source, line by line, in the tick it says it. */
    private static final class Sink implements CommandSource {
        final List<String> lines = new ArrayList<>();
        final List<Long> ticks = new ArrayList<>();
        private final ServerLevel level;

        Sink(ServerLevel level) {
            this.level = level;
        }

        @Override
        public void sendSystemMessage(Component message) {
            lines.add(message.getString());
            ticks.add(level.getGameTime());
        }

        @Override
        public boolean acceptsSuccess() {
            return true;
        }

        @Override
        public boolean acceptsFailure() {
            return true;
        }

        @Override
        public boolean shouldInformAdmins() {
            return false;
        }
    }

    private static void command(GameTestHelper helper, BlockPos at, Sink sink, String cmd) {
        ServerLevel level = helper.getLevel();
        CommandSourceStack src = level.getServer().createCommandSourceStack().withSource(sink).withPermission(4).withLevel(level)
                .withPosition(Vec3.atCenterOf(at));
        level.getServer().getCommands().performPrefixedCommand(src, cmd);
    }

    /**
     * {@code /rtsim district rescan} is a full detection over the next ticks, not one big tick: the call itself does
     * no detection work, says the detection is under way, and the source hears again, once, when the districts are
     * committed; the districts were what they were until then, and no tick looked at more than a budget and a chunk.
     */
    @GameTest(template = YARD, batch = "rescan_budget_6", timeoutTicks = 800)
    public static void theRescanCommandSpreadsOverTicksAndReportsWhenDone(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, BUDGET);
        Sink sink = new Sink(helper.getLevel());
        BlockPos[] ms = new BlockPos[4];
        long[] looked = {0};
        helper.startSequence()
                .thenExecute(() -> {
                    for (int i = 0; i < 4; i++)
                        ms[i] = fill(helper, i % 2, i / 2, true);
                })
                .thenWaitUntil(() -> check("the district of four chunks is settled",
                        !ind().districts().rescanning() && districtAt(helper, ms[0]) != null && districtAt(helper, ms[0]).chunks.size() == 4))
                .thenExecute(() -> {
                    looked[0] = ind().districts().rescans();
                    command(helper, ms[0], sink, "rtsim district rescan");
                    check("the call did no detection work", ind().districts().rescans() == looked[0]);
                    check("the detection is waiting to start or under way", ind().districts().rescanning());
                    check("the call said it goes on over the next ticks, and said nothing else: " + sink.lines,
                            sink.lines.size() == 1 && sink.lines.get(0).contains("over the next ticks"));
                })
                .thenWaitUntil(() -> {
                    check("the report came: " + sink.lines, sink.lines.size() == 2);
                    check("and the detection is over", !ind().districts().rescanning());
                })
                .thenExecute(() -> {
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, null);
                    Districts ds = ind().districts();
                    record("theRescanCommandSpreadsOverTicksAndReportsWhenDone", "ticks / longest tick (us) / looked / phases",
                            ds.lastRescanSlices() + " / " + ds.lastRescanLongestNanos() / 1000 + " / " + ds.lastRescanLooked() + " / " + ds.lastRescanPhases());
                    check("spread over at least four ticks, got " + ds.lastRescanSlices(), ds.lastRescanSlices() >= 4);
                    checkTickBudget(ds, helper.getLevel(), BUDGET, ms);
                    check("the report came in a later tick than the call", sink.ticks.get(1) > sink.ticks.get(0));
                    check("the report is the districts' count: " + sink.lines.get(1), sink.lines.get(1).contains(": 1 districts;"));
                    check("the district is whole", districtAt(helper, ms[0]).chunks.size() == 4 && districtAt(helper, ms[0]).seeds() == 4);
                })
                .thenSucceed();
    }

    /**
     * {@code merge} records the link at once and the districts become one when the detection commits: until then there are
     * two, after it one of both chunks, and the source hears twice (recorded, then done).
     */
    @GameTest(template = YARD, batch = "rescan_budget_7", timeoutTicks = 800)
    public static void theMergeCommandJoinsTheDistrictsWhenTheDetectionIsDone(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, BUDGET);
        Sink sink = new Sink(helper.getLevel()), refused = new Sink(helper.getLevel());
        BlockPos[] ms = new BlockPos[2];
        long[] ids = {0, 0};
        int[] before = {0};
        helper.startSequence()
                .thenExecute(() -> {
                    ms[0] = fill(helper, 0, 0, true);
                    ms[1] = fill(helper, 3, 0, true); // two chunks between: nothing joins them
                })
                .thenWaitUntil(() -> {
                    check("two districts, settled", !ind().districts().rescanning() && districtAt(helper, ms[0]) != null
                            && districtAt(helper, ms[1]) != null && districtAt(helper, ms[0]) != districtAt(helper, ms[1]));
                    ids[0] = districtAt(helper, ms[0]).id;
                    ids[1] = districtAt(helper, ms[1]).id;
                })
                .thenExecute(() -> {
                    before[0] = ind().districts().all().size();
                    command(helper, ms[0], sink, "rtsim district merge " + ids[0] + " " + ids[1]);
                    check("the call left the districts as they were", ind().districts().all().size() == before[0]
                            && districtAt(helper, ms[0]) != districtAt(helper, ms[1]));
                    check("the call said the merge is recorded and the detection goes on: " + sink.lines,
                            sink.lines.size() == 1 && sink.lines.get(0).contains("recorded") && sink.lines.get(0).contains("over the next ticks"));
                    // merge and split read the district set, which the detection under way has not changed yet
                    command(helper, ms[0], refused, "rtsim district merge " + ids[0] + " " + ids[1]);
                    check("a second merge during the detection is refused: " + refused.lines,
                            refused.lines.size() == 1 && refused.lines.get(0).contains("still running"));
                })
                .thenWaitUntil(() -> {
                    check("the report came: " + sink.lines, sink.lines.size() == 2);
                    check("the detection is over", !ind().districts().rescanning());
                })
                .thenExecute(() -> {
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, null);
                    check("one district now", ind().districts().all().size() == before[0] - 1 && districtAt(helper, ms[0]) == districtAt(helper, ms[1]));
                    check("of both chunks", districtAt(helper, ms[0]).chunks.size() == 2);
                    check("spread over ticks, got " + ind().districts().lastRescanSlices(), ind().districts().lastRescanSlices() >= 3);
                    check("the report says so: " + sink.lines.get(1), sink.lines.get(1).contains("merged"));
                    check("the refused merge changed nothing and was not answered twice: " + refused.lines, refused.lines.size() == 1);
                })
                .thenSucceed();
    }

    /** {@code split} pins the chunk at once and the district divides when the detection commits. */
    @GameTest(template = YARD, batch = "rescan_budget_8", timeoutTicks = 800)
    public static void theSplitCommandDividesTheDistrictWhenTheDetectionIsDone(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, BUDGET);
        Sink sink = new Sink(helper.getLevel());
        BlockPos[] ms = new BlockPos[2];
        helper.startSequence()
                .thenExecute(() -> {
                    ms[0] = fill(helper, 0, 0, true);
                    ms[1] = fill(helper, 1, 0, true);
                })
                .thenWaitUntil(() -> check("one district of two chunks, settled", !ind().districts().rescanning()
                        && districtAt(helper, ms[0]) != null && districtAt(helper, ms[0]) == districtAt(helper, ms[1])
                        && districtAt(helper, ms[0]).chunks.size() == 2))
                .thenExecute(() -> {
                    command(helper, ms[1], sink, "rtsim district split");
                    check("the call left the district whole", districtAt(helper, ms[0]) == districtAt(helper, ms[1]));
                    check("the call said the chunk is pinned and the detection goes on: " + sink.lines,
                            sink.lines.size() == 1 && sink.lines.get(0).contains("pinned") && sink.lines.get(0).contains("over the next ticks"));
                })
                .thenWaitUntil(() -> {
                    check("the report came: " + sink.lines, sink.lines.size() == 2);
                    check("the detection is over", !ind().districts().rescanning());
                })
                .thenExecute(() -> {
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, null);
                    check("two districts of a chunk each", districtAt(helper, ms[0]) != null && districtAt(helper, ms[1]) != null
                            && districtAt(helper, ms[0]) != districtAt(helper, ms[1])
                            && districtAt(helper, ms[0]).chunks.size() == 1 && districtAt(helper, ms[1]).chunks.size() == 1);
                    check("the report says so: " + sink.lines.get(1), sink.lines.get(1).contains("split off"));
                })
                .thenSucceed();
    }

    /** {@code exclude} takes the chunk out when the detection commits, {@code include} brings it back the same way. */
    @GameTest(template = YARD, batch = "rescan_budget_9", timeoutTicks = 1200)
    public static void theExcludeAndIncludeCommandsActWhenTheDetectionIsDone(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, BUDGET);
        Sink sink = new Sink(helper.getLevel());
        BlockPos[] ms = new BlockPos[2];
        helper.startSequence()
                .thenExecute(() -> {
                    ms[0] = fill(helper, 0, 0, true);
                    ms[1] = fill(helper, 1, 0, true);
                })
                .thenWaitUntil(() -> check("one district of two chunks, settled", !ind().districts().rescanning()
                        && districtAt(helper, ms[0]) != null && districtAt(helper, ms[0]).chunks.size() == 2))
                .thenExecute(() -> {
                    command(helper, ms[1], sink, "rtsim district exclude");
                    check("the call left the district whole", districtAt(helper, ms[0]).chunks.size() == 2);
                    check("and said the detection goes on: " + sink.lines, sink.lines.size() == 1 && sink.lines.get(0).contains("over the next ticks"));
                })
                .thenWaitUntil(() -> {
                    check("the report came: " + sink.lines, sink.lines.size() == 2);
                    check("the detection is over", !ind().districts().rescanning());
                })
                .thenExecute(() -> {
                    check("the chunk is out of every district", districtAt(helper, ms[1]) == null && districtAt(helper, ms[0]).chunks.size() == 1);
                    command(helper, ms[1], sink, "rtsim district include");
                    check("include said the detection goes on: " + sink.lines, sink.lines.size() == 3 && sink.lines.get(2).contains("over the next ticks"));
                })
                .thenWaitUntil(() -> {
                    check("the second report came: " + sink.lines, sink.lines.size() == 4);
                    check("the detection is over", !ind().districts().rescanning());
                })
                .thenExecute(() -> {
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, null);
                    check("the chunk is back in the district", districtAt(helper, ms[1]) != null && districtAt(helper, ms[1]) == districtAt(helper, ms[0])
                            && districtAt(helper, ms[0]).chunks.size() == 2);
                })
                .thenSucceed();
    }

    /**
     * A second command while the first one's detection runs restarts it as a full detection and both are answered, once each,
     * when the one detection that covers them is done: the first, cancelled one never commits.
     */
    @GameTest(template = YARD, batch = "rescan_budget_10", timeoutTicks = 800)
    public static void aSecondCommandDuringTheDetectionIsAnsweredWithTheFirst(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, BUDGET);
        Sink first = new Sink(helper.getLevel()), second = new Sink(helper.getLevel());
        BlockPos[] ms = new BlockPos[4];
        long[] rescans = {0};
        helper.startSequence()
                .thenExecute(() -> {
                    for (int i = 0; i < 4; i++)
                        ms[i] = fill(helper, i % 2, i / 2, true);
                })
                .thenWaitUntil(() -> check("settled", !ind().districts().rescanning() && districtAt(helper, ms[0]) != null
                        && districtAt(helper, ms[0]).chunks.size() == 4))
                .thenExecute(() -> {
                    rescans[0] = ind().districts().rescans();
                    command(helper, ms[0], first, "rtsim district rescan");
                })
                .thenIdle(2) // the first detection has started and has chunks left (four chunks of 30 block entities at 10 a tick)
                .thenExecute(() -> {
                    check("the first detection is under way: " + first.lines, ind().districts().rescanning() && first.lines.size() == 1);
                    check("and has not committed", ind().districts().rescans() == rescans[0]);
                    command(helper, ms[0], second, "rtsim district rescan");
                })
                .thenWaitUntil(() -> {
                    check("both were answered: " + first.lines + " and " + second.lines, first.lines.size() == 2 && second.lines.size() == 2);
                    check("and the detection is over", !ind().districts().rescanning());
                })
                .thenExecute(() -> {
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, null);
                    check("both reports came in one tick", first.ticks.get(1).equals(second.ticks.get(1)));
                    check("one detection answered both, the cancelled one never committed: " + (ind().districts().rescans() - rescans[0]),
                            ind().districts().rescans() == rescans[0] + 1);
                    check("the district is whole", districtAt(helper, ms[0]).chunks.size() == 4);
                })
                .thenSucceed();
    }

    /**
     * The time limit (R2-11): a tick's share of a detection stops at the first chunk past {@code rescanSliceMicros}, however
     * much of the block entity budget is left. At 1 us every tick looks at one chunk, so four chunks of a machine and 30
     * block entities each, well inside the default budget of 256, take at least four ticks; the district is what a
     * one-call detection finds (same chunks, same seeds), and no chunk loaded while it looked.
     */
    @GameTest(template = YARD, batch = "rescan_budget_11", timeoutTicks = 600)
    public static void aTickStopsAtTheTimeLimitAndDetectsTheSame(GameTestHelper helper) {
        String t = "aTickStopsAtTheTimeLimitAndDetectsTheSame";
        fastConfig();
        forgetAll(helper);
        ServerLevel level = helper.getLevel();
        IndustryConfig.Test.set(IndustryConfig.RESCAN_SLICE_MICROS, 1);
        BlockPos[] ms = new BlockPos[4];
        long[] spread = new long[6]; // slices, longest looked, longest item, chunk loads, chunks, seeds
        helper.startSequence()
                .thenExecute(() -> {
                    for (int i = 0; i < 4; i++)
                        ms[i] = fill(helper, i % 2, i / 2, true);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, ms[0]);
                    check("the district is detected", !ind().districts().rescanning() && d != null && d.chunks.size() == 4);
                })
                .thenExecute(() -> {
                    Districts ds = ind().districts();
                    District d = districtAt(helper, ms[0]);
                    spread[0] = ds.lastRescanSlices();
                    spread[1] = ds.lastRescanLongestLooked();
                    spread[2] = ds.lastRescanLongestItem();
                    spread[3] = ds.lastRescanChunkLoads();
                    spread[4] = d.chunks.size();
                    spread[5] = d.seeds();
                    record(t, "ticks / longest tick looked / longest chunk looked / longest chunk (us)", spread[0] + " / " + spread[1] + " / "
                            + spread[2] + " / " + ds.lastRescanLongestItemNanos() / 1000);
                    record(t, "last detection", ds.lastRescanCold());
                    IndustryConfig.Test.set(IndustryConfig.RESCAN_SLICE_MICROS, null);
                    check("one chunk a tick: four ticks at least, got " + spread[0], spread[0] >= 4);
                    check("no tick looked at more than one chunk: " + spread[1] + " > " + spread[2], spread[1] <= spread[2]);
                    check("no chunk loaded while the detection looked: " + spread[3], spread[3] == 0);
                    ds.rescanNow(level);
                    District again = districtAt(helper, ms[0]);
                    check("a one-call detection finds the same district: " + again.chunks.size() + " chunks, " + again.seeds() + " seeds",
                            again.chunks.size() == spread[4] && again.seeds() == spread[5] && again.chunks.size() == 4 && again.seeds() == 4);
                    for (BlockPos m : ms)
                        check("every machine is in it", districtAt(helper, m) == again);
                })
                .thenSucceed();
    }

    /** A failed test leaves its budget behind: the next batch starts from the fast configuration the other tests assume. */
    @AfterBatch(batch = "rescan_budget_1")
    public static void resetAfter1(ServerLevel level) {
        reset();
    }

    @AfterBatch(batch = "rescan_budget_2")
    public static void resetAfter2(ServerLevel level) {
        reset();
    }

    @AfterBatch(batch = "rescan_budget_3")
    public static void resetAfter3(ServerLevel level) {
        reset();
    }

    @AfterBatch(batch = "rescan_budget_5")
    public static void resetAfter5(ServerLevel level) {
        reset();
    }

    @AfterBatch(batch = "rescan_budget_6")
    public static void resetAfter6(ServerLevel level) {
        reset();
    }

    @AfterBatch(batch = "rescan_budget_7")
    public static void resetAfter7(ServerLevel level) {
        reset();
    }

    @AfterBatch(batch = "rescan_budget_8")
    public static void resetAfter8(ServerLevel level) {
        reset();
    }

    @AfterBatch(batch = "rescan_budget_9")
    public static void resetAfter9(ServerLevel level) {
        reset();
    }

    @AfterBatch(batch = "rescan_budget_10")
    public static void resetAfter10(ServerLevel level) {
        reset();
    }

    @AfterBatch(batch = "rescan_budget_11")
    public static void resetAfter11(ServerLevel level) {
        reset();
    }

    private static void reset() {
        IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, null);
        IndustryConfig.Test.set(IndustryConfig.RESCAN_SLICE_MICROS, null);
        IndustryConfig.Test.set(IndustryConfig.RESCAN_INTERVAL, 1);
    }
}
