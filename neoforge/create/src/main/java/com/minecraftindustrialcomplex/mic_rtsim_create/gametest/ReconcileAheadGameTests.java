package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryAttachments;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.FullChunkStatus;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.level.TicketType;
import net.minecraft.world.Container;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.*;

/**
 * Reconcile ahead of arrival, the Java side (DECISIONS.md "Stale tick", choices IR9 and IR10,
 * docs/choices-repair.md R1 to R8): a reconcile the engine sent ahead, held for a chunk that is not
 * loaded, lands before any block entity in the chunk ticks; its IND_RECONCILE_RESULT is journaled before
 * the chunk's IND_CHUNK_LOADED, which carries the epoch now applied; it is exactly once across a crash at
 * any step; and a partly loaded district sends IND_APPROACH for its unloaded part, closing only while a
 * player really closes in. Far chunks, each test its own batch, either engine.
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class ReconcileAheadGameTests {
    private ReconcileAheadGameTests() {}

    /** A ticket that keeps a chunk FULL (level 33) and not block-ticking: block entities placed there never tick. */
    private static final TicketType<ChunkPos> FULL_ONLY = TicketType.create("mic_rtsim_create_full_only", Comparator.comparingLong(ChunkPos::toLong));

    private static final String IRON = "i/64/minecraft:iron_ingot";
    private static final String GOLD = "i/64/minecraft:gold_ingot";

    private static int count(Container c, Item item) {
        int n = 0;
        for (int i = 0; i < c.getContainerSize(); i++)
            if (c.getItem(i).is(item))
                n += c.getItem(i).getCount();
        return n;
    }

    private static Container container(ServerLevel level, BlockPos pos) {
        return (Container) level.getBlockEntity(pos);
    }

    private static ByteBuffer result(long op) {
        List<ByteBuffer> r = received(IndRecords.IND_RECONCILE_RESULT, IndRecords.ReconcileResult.OP_ID, op);
        return r.isEmpty() ? null : r.get(r.size() - 1);
    }

    private static List<ScriptedEngine.Received> journaled(int kind, int districtOffset, long district) {
        List<ScriptedEngine.Received> out = new ArrayList<>();
        for (ScriptedEngine.Received r : engine().received(kind))
            if (!r.skipped() && r.bytes().length >= districtOffset + 8 && r.view().getLong(districtOffset) == district)
                out.add(r);
        return out;
    }

    /** The journaled IND_RECONCILE_RESULT of an op id (the last one), with its sequence. */
    private static ScriptedEngine.Received resultRecord(long op) {
        ScriptedEngine.Received last = null;
        for (ScriptedEngine.Received r : engine().received(IndRecords.IND_RECONCILE_RESULT))
            if (!r.skipped() && r.view().getLong(IndRecords.ReconcileResult.OP_ID) == op)
                last = r;
        return last;
    }

    /** The last IND_CHUNK_LOADED of a district's chunk, with its sequence, or null. */
    private static ScriptedEngine.Received lastLoaded(long district, int cx, int cz) {
        ScriptedEngine.Received last = null;
        for (ScriptedEngine.Received r : journaled(IndRecords.IND_CHUNK_LOADED, IndRecords.ChunkLoaded.DISTRICT, district))
            if (r.view().getInt(IndRecords.ChunkLoaded.X) == cx && r.view().getInt(IndRecords.ChunkLoaded.Z) == cz)
                last = r;
        return last;
    }

    private static int loadedCount(long district, int cx, int cz) {
        int n = 0;
        for (ScriptedEngine.Received r : journaled(IndRecords.IND_CHUNK_LOADED, IndRecords.ChunkLoaded.DISTRICT, district))
            if (r.view().getInt(IndRecords.ChunkLoaded.X) == cx && r.view().getInt(IndRecords.ChunkLoaded.Z) == cz)
                n++;
        return n;
    }

    private static int epochIn(ServerLevel level, int cx, int cz, long district) {
        LevelChunk c = level.getChunkSource().getChunkNow(cx, cz);
        return c == null ? -1 : c.getData(IndustryAttachments.RECONCILE.get()).epoch(district);
    }

    private static boolean unloaded(ServerLevel level, int cx, int cz, long district) {
        District d = ind().district(district);
        return level.getChunkSource().getChunkNow(cx, cz) == null && d != null && ind().districts().loadedChunks(d) == 0;
    }

    private static int farChunk(GameTestHelper helper, boolean x, int offset) {
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        return ((x ? o.getX() : o.getZ()) >> 4) + offset;
    }

    /**
     * The stale-tick race closed: a chest holds 9 iron, a hopper under it would pull one at its first
     * tick. The chunk unloads; the engine sends a reconcile ahead (the replay ate the 9 iron) and Java
     * holds it. The chunk loads again: the reconcile lands before the hopper ticks, so the removal is not
     * short and the hopper finds the chest empty. Its IND_RECONCILE_RESULT is journaled before the
     * chunk's IND_CHUNK_LOADED, which carries epoch 1. Afterwards the chunk ticks: an iron put in the
     * chest reaches the hopper.
     *
     * <p>The chest and hopper are built in a chunk held FULL and not block-ticking, so the hopper never
     * ticks before the test wants it to.
     */
    @GameTest(template = YARD, batch = "ahead_1", timeoutTicks = 2400)
    public static void aHeldReconcileLandsBeforeAHopperInItsChunkMoves(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        int fx = farChunk(helper, true, 3200), fz = farChunk(helper, false, 3200);
        ChunkPos cp = new ChunkPos(fx, fz);
        BlockPos chestPos = new BlockPos(fx * 16 + 6, 100, fz * 16 + 6);
        BlockPos hopperPos = chestPos.below();
        long op = engineOp(), more = engineOp();
        long[] id = {0};
        int[] heldBefore = {0};
        helper.startSequence()
                .thenExecute(() -> level.getChunkSource().addRegionTicket(FULL_ONLY, cp, 0, cp))
                .thenWaitUntil(() -> {
                    LevelChunk c = level.getChunkSource().getChunkNow(fx, fz);
                    check("the far chunk is loaded and not block-ticking", c != null && !c.getFullStatus().isOrAfter(FullChunkStatus.BLOCK_TICKING));
                })
                .thenExecute(() -> {
                    ind().districts().forgetArea(level, fx - 1, fz - 1, fx + 1, fz + 1);
                    level.setBlock(chestPos, Blocks.CHEST.defaultBlockState(), 3);
                    level.setBlock(hopperPos, Blocks.HOPPER.defaultBlockState(), 3);
                    container(level, chestPos).setItem(0, new ItemStack(Items.IRON_INGOT, 9));
                    playerPlaced(helper, chestPos);
                    playerPlaced(helper, hopperPos);
                })
                .thenWaitUntil(() -> {
                    District d = ind().districtAt(level, chestPos);
                    check("district", d != null);
                    id[0] = d.id;
                })
                .thenExecute(() -> {
                    check("the hopper never ticked while the chunk was FULL only", count(container(level, chestPos), Items.IRON_INGOT) == 9
                            && count(container(level, hopperPos), Items.IRON_INGOT) == 0);
                    level.getChunkSource().removeRegionTicket(FULL_ONLY, cp, 0, cp);
                })
                .thenWaitUntil(() -> check("the far chunk unloads (saved)", unloaded(level, fx, fz, id[0])))
                .thenExecute(() -> {
                    heldBefore[0] = ind().reconciler().held();
                    reconcile(op, id[0], 1, level, fx, fz, List.of(new Delta(0, IRON, chestPos, -9)), List.of());
                })
                .thenWaitUntil(() -> check("the reconcile sent ahead is held", ind().reconciler().held() == heldBefore[0] + 1 && result(op) == null))
                .thenExecute(() -> level.setChunkForced(fx, fz, true))
                .thenWaitUntil(() -> {
                    ByteBuffer r = result(op);
                    check("applied at the load", r != null);
                    long requested = r.getLong(IndRecords.ReconcileResult.ENTRIES + 8), applied = r.getLong(IndRecords.ReconcileResult.ENTRIES + 16);
                    record("aHeldReconcileLandsBeforeAHopperInItsChunkMoves", "requested, applied", requested + ", " + applied);
                    check("no machine ticked first: -9 requested, -9 applied (got " + applied + ")", requested == -9 && applied == -9);
                    check("the chest is empty", count(container(level, chestPos), Items.IRON_INGOT) == 0);
                    check("the hopper moved nothing", count(container(level, hopperPos), Items.IRON_INGOT) == 0);
                    check("COMMAND_RESULT applied", commandResults(op).contains(Obs.CommandResult.APPLIED));
                    ScriptedEngine.Received loaded = lastLoaded(id[0], fx, fz);
                    ScriptedEngine.Received res = resultRecord(op);
                    check("IND_CHUNK_LOADED went out", loaded != null);
                    check("the result is journaled before the chunk's IND_CHUNK_LOADED: result " + res.obsSeq() + ", loaded " + loaded.obsSeq(),
                            res.obsSeq() < loaded.obsSeq());
                    check("IND_CHUNK_LOADED carries the epoch now applied: " + loaded.view().getInt(IndRecords.ChunkLoaded.APPLIED_EPOCH),
                            loaded.view().getInt(IndRecords.ChunkLoaded.APPLIED_EPOCH) == 1);
                    check("epoch 1 is in the chunk", epochIn(level, fx, fz, id[0]) == 1);
                })
                // The chunk ticks again once the reconcile landed: an iron added now reaches the hopper.
                .thenExecute(() -> reconcile(more, id[0], 2, level, fx, fz, List.of(new Delta(0, IRON, chestPos, 1)), List.of()))
                .thenWaitUntil(() -> check("the hopper pulls once the chunk ticks", count(container(level, hopperPos), Items.IRON_INGOT) == 1))
                .thenExecute(() -> {
                    level.setBlock(hopperPos, Blocks.AIR.defaultBlockState(), 3);
                    level.setBlock(chestPos, Blocks.AIR.defaultBlockState(), 3);
                    ind().districts().forgetArea(level, fx - 1, fz - 1, fx + 1, fz + 1);
                    level.setChunkForced(fx, fz, false);
                })
                .thenSucceed();
    }

    /**
     * Exactly once across a crash at each step of a reconcile sent ahead (I8):
     * <ol>
     *   <li>held, then the server stops before the chunk loads: the held command is lost with the process,
     *       the chunk reports epoch 0, and the command the engine sends again at the load applies once;</li>
     *   <li>held and applied at the load: once, its result before IND_CHUNK_LOADED (epoch 2);</li>
     *   <li>the chunk saved after the apply, the journal not (a crash): the engine, rolled back, sends the
     *       same op again and, its result lost, epoch 2 under a new op id. Both are held, and both are
     *       duplicates at the load by the chunk's epoch; the pending result the chunk carries is journaled
     *       again for the new session;</li>
     *   <li>applied at the load, then the chunk on disk predates it (it never reached disk): the command sent
     *       again is held and applies once at the next load.</li>
     * </ol>
     */
    @GameTest(template = YARD, batch = "ahead_2", timeoutTicks = 4800)
    public static void aReconcileSentAheadAppliesOnceAcrossACrashAtEachStep(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        int fx = farChunk(helper, true, 3300), fz = farChunk(helper, false, 3300);
        BlockPos chestPos = new BlockPos(fx * 16 + 9, 100, fz * 16 + 4);
        long a = engineOp(), b = engineOp(), c = engineOp(), d4 = engineOp();
        long[] id = {0};
        int[] mark = {0};
        helper.startSequence()
                .thenExecute(() -> level.setChunkForced(fx, fz, true))
                .thenWaitUntil(() -> check("the far chunk loads", level.getChunkSource().getChunkNow(fx, fz) != null))
                .thenExecute(() -> {
                    ind().districts().forgetArea(level, fx - 1, fz - 1, fx + 1, fz + 1);
                    level.setBlock(chestPos, Blocks.CHEST.defaultBlockState(), 3);
                    playerPlaced(helper, chestPos);
                })
                .thenWaitUntil(() -> {
                    District d = ind().districtAt(level, chestPos);
                    check("district", d != null);
                    id[0] = d.id;
                })
                .thenExecute(() -> level.setChunkForced(fx, fz, false))
                .thenWaitUntil(() -> check("unloads", unloaded(level, fx, fz, id[0])))
                // 1. Held, then the server stops before the load.
                .thenExecute(() -> {
                    mark[0] = ind().reconciler().held();
                    reconcile(a, id[0], 1, level, fx, fz, List.of(new Delta(0, GOLD, chestPos, 7)), List.of());
                })
                .thenWaitUntil(() -> check("held", ind().reconciler().held() == mark[0] + 1))
                .thenExecute(() -> {
                    ind().reconciler().restartForTest(); // what a restart loses: the held commands live in memory
                    mark[0] = loadedCount(id[0], fx, fz);
                    level.setChunkForced(fx, fz, true);
                })
                .thenWaitUntil(() -> {
                    check("the load is reported", loadedCount(id[0], fx, fz) > mark[0]);
                    check("with epoch 0: nothing was applied", lastLoaded(id[0], fx, fz).view().getInt(IndRecords.ChunkLoaded.APPLIED_EPOCH) == 0);
                    check("no result for a yet", result(a) == null);
                })
                .thenExecute(() -> reconcile(a, id[0], 1, level, fx, fz, List.of(new Delta(0, GOLD, chestPos, 7)), List.of())) // the engine answers the load (F27)
                .thenWaitUntil(() -> {
                    check("a applied once at the engine's answer", result(a) != null && result(a).getLong(IndRecords.ReconcileResult.ENTRIES + 16) == 7);
                    check("7 gold", count(container(level, chestPos), Items.GOLD_INGOT) == 7);
                    check("epoch 1", epochIn(level, fx, fz, id[0]) == 1);
                })
                // 2. Held and applied at the load.
                .thenExecute(() -> level.setChunkForced(fx, fz, false))
                .thenWaitUntil(() -> check("unloads", unloaded(level, fx, fz, id[0])))
                .thenExecute(() -> {
                    mark[0] = ind().reconciler().held();
                    reconcile(b, id[0], 2, level, fx, fz, List.of(new Delta(0, GOLD, chestPos, 5)), List.of());
                })
                .thenWaitUntil(() -> check("held", ind().reconciler().held() == mark[0] + 1))
                .thenExecute(() -> level.setChunkForced(fx, fz, true))
                .thenWaitUntil(() -> {
                    check("b applied at the load", result(b) != null && result(b).getLong(IndRecords.ReconcileResult.ENTRIES + 16) == 5);
                    check("12 gold", count(container(level, chestPos), Items.GOLD_INGOT) == 12);
                    ScriptedEngine.Received loaded = lastLoaded(id[0], fx, fz);
                    check("IND_CHUNK_LOADED after b's result, with epoch 2", loaded != null && resultRecord(b).obsSeq() < loaded.obsSeq()
                            && loaded.view().getInt(IndRecords.ChunkLoaded.APPLIED_EPOCH) == 2);
                })
                // 3. The chunk saved ahead of the journal: b again (same op) and epoch 2 under op c.
                .thenExecute(() -> level.setChunkForced(fx, fz, false))
                .thenWaitUntil(() -> check("unloads with epoch 2 on disk", unloaded(level, fx, fz, id[0])))
                .thenExecute(() -> {
                    ind().reconciler().restartForTest();
                    mark[0] = ind().reconciler().held();
                    reconcile(b, id[0], 2, level, fx, fz, List.of(new Delta(0, GOLD, chestPos, 5)), List.of());
                    reconcile(c, id[0], 2, level, fx, fz, List.of(new Delta(0, GOLD, chestPos, 5)), List.of());
                })
                .thenWaitUntil(() -> check("both held", ind().reconciler().held() == mark[0] + 2))
                .thenExecute(() -> level.setChunkForced(fx, fz, true))
                .thenWaitUntil(() -> {
                    check("b again is a duplicate", commandResults(b).contains(Obs.CommandResult.DUPLICATE_OP));
                    check("c, epoch 2 under a new op id, is a duplicate", commandResults(c).contains(Obs.CommandResult.DUPLICATE_OP));
                    check("still 12 gold", count(container(level, chestPos), Items.GOLD_INGOT) == 12);
                    check("IND_CHUNK_LOADED says epoch 2", lastLoaded(id[0], fx, fz).view().getInt(IndRecords.ChunkLoaded.APPLIED_EPOCH) == 2);
                })
                // 4. Applied at the load, and the chunk on disk predates it.
                .thenExecute(() -> level.setChunkForced(fx, fz, false))
                .thenWaitUntil(() -> check("unloads", unloaded(level, fx, fz, id[0])))
                .thenExecute(() -> {
                    mark[0] = ind().reconciler().held();
                    reconcile(d4, id[0], 3, level, fx, fz, List.of(new Delta(0, GOLD, chestPos, 4)), List.of());
                })
                .thenWaitUntil(() -> check("held", ind().reconciler().held() == mark[0] + 1))
                .thenExecute(() -> level.setChunkForced(fx, fz, true))
                .thenWaitUntil(() -> check("d applied at the load: 16 gold", count(container(level, chestPos), Items.GOLD_INGOT) == 16 && epochIn(level, fx, fz, id[0]) == 3))
                .thenExecute(() -> {
                    // The crash: the chunk that reaches disk is the one from before d (12 gold, epoch 2).
                    Container chest = container(level, chestPos);
                    chest.clearContent();
                    chest.setItem(0, new ItemStack(Items.GOLD_INGOT, 12));
                    LevelChunk chunk = level.getChunkSource().getChunkNow(fx, fz);
                    chunk.setData(IndustryAttachments.RECONCILE.get(), IndustryAttachments.Epochs.EMPTY
                            .with(new IndustryAttachments.EpochEntry(id[0], 2, java.util.Optional.empty())));
                    chunk.setUnsaved(true);
                    ind().reconciler().restartForTest();
                    level.setChunkForced(fx, fz, false);
                })
                .thenWaitUntil(() -> check("unloads with the older chunk", unloaded(level, fx, fz, id[0])))
                .thenExecute(() -> {
                    mark[0] = ind().reconciler().held();
                    reconcile(d4, id[0], 3, level, fx, fz, List.of(new Delta(0, GOLD, chestPos, 4)), List.of()); // the engine never had d's result
                })
                .thenWaitUntil(() -> check("held", ind().reconciler().held() == mark[0] + 1))
                .thenExecute(() -> level.setChunkForced(fx, fz, true))
                .thenWaitUntil(() -> {
                    check("the older chunk takes d once: 16 gold", count(container(level, chestPos), Items.GOLD_INGOT) == 16);
                    check("epoch 3", epochIn(level, fx, fz, id[0]) == 3);
                    record("aReconcileSentAheadAppliesOnceAcrossACrashAtEachStep", "results",
                            "a " + commandResults(a) + ", b " + commandResults(b) + ", c " + commandResults(c) + ", d " + commandResults(d4));
                })
                .thenExecute(() -> {
                    level.setBlock(chestPos, Blocks.AIR.defaultBlockState(), 3);
                    ind().districts().forgetArea(level, fx - 1, fz - 1, fx + 1, fz + 1);
                    level.setChunkForced(fx, fz, false);
                })
                .thenSucceed();
    }

    /**
     * A partly loaded district (I7): chunk A loaded, chunk B four chunks east of it unloaded (a member
     * detection kept). A player west of A walks east, one block a tick: IND_APPROACH goes out for the
     * district, closing, with the distance to B (the unloaded part), not to the district's box, and an ETA
     * near that distance. Standing still is not closing. Once B loads, nothing of the district is unloaded
     * and the approach records stop.
     */
    @GameTest(template = YARD, batch = "ahead_3", timeoutTicks = 2400)
    public static void aPartlyLoadedDistrictSendsApproachForItsUnloadedPart(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        int ax = farChunk(helper, true, 3400), az = farChunk(helper, false, 3400);
        int bx = ax + 4;
        BlockPos chestPos = new BlockPos(ax * 16 + 8, 100, az * 16 + 8);
        long[] id = {0};
        ServerPlayer[] player = {null};
        int[] mark = {0};
        int[] walk = {0}; // blocks a tick east
        helper.onEachTick(() -> {
            if (player[0] != null && walk[0] != 0)
                player[0].moveTo(player[0].getX() + walk[0], player[0].getY(), player[0].getZ());
        });
        helper.startSequence()
                .thenExecute(() -> level.setChunkForced(ax, az, true))
                .thenWaitUntil(() -> check("chunk A loads", level.getChunkSource().getChunkNow(ax, az) != null))
                .thenExecute(() -> {
                    ind().districts().forgetArea(level, ax - 1, az - 1, bx + 1, az + 1);
                    level.setBlock(chestPos, Blocks.CHEST.defaultBlockState(), 3);
                    playerPlaced(helper, chestPos);
                })
                .thenWaitUntil(() -> {
                    District d = ind().districtAt(level, chestPos);
                    check("district", d != null);
                    id[0] = d.id;
                })
                .thenWaitUntil(() -> check("B is not loaded (four chunks from a forced chunk)", level.getChunkSource().getChunkNow(bx, az) == null))
                .thenExecute(() -> ind().districts().addChunkForTest(ind().district(id[0]), ChunkPos.asLong(bx, az)))
                .thenWaitUntil(() -> check("partly loaded", ind().district(id[0]).loadState() == IndRecords.LoadState.PARTIAL))
                .thenExecute(() -> {
                    // West of A, 120 blocks from A's west edge: the district's box is 120 away, B is 184 away.
                    player[0] = FakePlayers.add(level, new Vec3(ax * 16 - 120 + 0.5, 100, az * 16 + 8.5), "ahead_walker");
                    mark[0] = received(IndRecords.IND_APPROACH, IndRecords.Approach.DISTRICT, id[0]).size();
                    walk[0] = 1;
                })
                .thenWaitUntil(() -> check("approach records while walking",
                        received(IndRecords.IND_APPROACH, IndRecords.Approach.DISTRICT, id[0]).size() >= mark[0] + 3))
                .thenExecute(() -> {
                    List<ByteBuffer> recs = received(IndRecords.IND_APPROACH, IndRecords.Approach.DISTRICT, id[0]);
                    ByteBuffer last = recs.get(recs.size() - 1);
                    int distance = last.getInt(IndRecords.Approach.DISTANCE);
                    int toB = (int) Math.round(bx * 16 - player[0].getX());
                    int toBox = (int) Math.round(ax * 16 - player[0].getX());
                    record("aPartlyLoadedDistrictSendsApproachForItsUnloadedPart", "distance, to B now, to the box now, eta",
                            distance + ", " + toB + ", " + toBox + ", " + Integer.toUnsignedLong(last.getInt(IndRecords.Approach.ETA_TICKS)));
                    check("closing", (aux(last) & IndRecords.Approach.FLAG_CLOSING) != 0);
                    check("the distance is to the unloaded part (B), not the box: " + distance, Math.abs(distance - toB) <= 25 && distance > toBox + 30);
                    long eta = Integer.toUnsignedLong(last.getInt(IndRecords.Approach.ETA_TICKS));
                    check("an ETA near the distance at one block a tick: " + eta, eta != 0xFFFF_FFFFL && Math.abs(eta - distance) <= 25);
                    walk[0] = 0;
                    mark[0] = recs.size();
                })
                .thenWaitUntil(() -> check("standing still",
                        received(IndRecords.IND_APPROACH, IndRecords.Approach.DISTRICT, id[0]).size() >= mark[0] + 3))
                .thenExecute(() -> {
                    List<ByteBuffer> recs = received(IndRecords.IND_APPROACH, IndRecords.Approach.DISTRICT, id[0]);
                    check("standing still is not closing", (aux(recs.get(recs.size() - 1)) & IndRecords.Approach.FLAG_CLOSING) == 0);
                    walk[0] = -1;
                    mark[0] = recs.size();
                })
                .thenWaitUntil(() -> check("walking away",
                        received(IndRecords.IND_APPROACH, IndRecords.Approach.DISTRICT, id[0]).size() >= mark[0] + 2))
                .thenExecute(() -> {
                    List<ByteBuffer> recs = received(IndRecords.IND_APPROACH, IndRecords.Approach.DISTRICT, id[0]);
                    check("walking away is not closing", (aux(recs.get(recs.size() - 1)) & IndRecords.Approach.FLAG_CLOSING) == 0);
                    walk[0] = 0;
                    level.setChunkForced(bx, az, true);
                })
                .thenWaitUntil(() -> check("B loads", level.getChunkSource().getChunkNow(bx, az) != null))
                .thenExecute(() -> mark[0] = received(IndRecords.IND_APPROACH, IndRecords.Approach.DISTRICT, id[0]).size())
                .thenExecuteAfter(80, () -> check("nothing unloaded, no approach: " + (received(IndRecords.IND_APPROACH, IndRecords.Approach.DISTRICT, id[0]).size() - mark[0]),
                        received(IndRecords.IND_APPROACH, IndRecords.Approach.DISTRICT, id[0]).size() == mark[0]))
                .thenExecute(() -> {
                    FakePlayers.remove(player[0]);
                    player[0] = null;
                    level.setBlock(chestPos, Blocks.AIR.defaultBlockState(), 3);
                    ind().districts().forgetArea(level, ax - 1, az - 1, bx + 1, az + 1);
                    level.setChunkForced(ax, az, false);
                    level.setChunkForced(bx, az, false);
                })
                .thenSucceed();
    }
}
