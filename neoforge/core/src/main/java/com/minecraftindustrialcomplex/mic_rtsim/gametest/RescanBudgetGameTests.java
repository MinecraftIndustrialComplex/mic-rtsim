package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryConfig;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
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
 * chunks with 30 inert block entities each, so a chunk costs more than a tick's budget.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class RescanBudgetGameTests {
    private RescanBudgetGameTests() {}

    private static final String YARD = "industry_yard";
    private static final int BUDGET = 10;
    private static final int FILLER = 30;

    /** A machine and {@code FILLER} inert block entities in the chunk whose corner is {@code (cx, cz)} chunks from the first whole chunk. */
    private static BlockPos fill(GameTestHelper helper, int cx, int cz, boolean machine) {
        BlockPos base = inChunk(helper, 0, 0);
        BlockPos origin = new BlockPos(base.getX() + 16 * cx, base.getY(), base.getZ() + 16 * cz);
        for (int i = 0; i < FILLER; i++)
            set(helper, origin.offset(2 + i % 6, 0, 8 + i / 6), TestMachines.INERT.get().defaultBlockState());
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
                            ind().districts().lastRescanSlices() + " / " + ind().districts().maxSliceNanos() / 1000);
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
}
