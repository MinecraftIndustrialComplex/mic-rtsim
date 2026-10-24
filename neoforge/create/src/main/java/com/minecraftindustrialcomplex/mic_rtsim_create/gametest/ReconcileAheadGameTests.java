package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ChunkWait;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.industry.TickGate;
import com.minecraftindustrialcomplex.mic_rtsim.journal.JournalSavedData;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import net.minecraft.core.BlockPos;
import net.minecraft.core.HolderLookup;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.FullChunkStatus;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.level.TicketType;
import net.minecraft.world.Container;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.storage.RegionFileVersion;
import net.minecraft.world.level.dimension.DimensionType;
import net.minecraft.world.level.storage.LevelResource;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.attachment.AttachmentHolder;
import net.neoforged.neoforge.common.IOUtilities;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.ByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.concurrent.atomic.AtomicReference;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.*;

/**
 * Reconcile ahead of arrival, the Java side (DECISIONS.md "Stale tick", choices IR9 and IR10,
 * docs/choices-repair.md R1 to R8): a reconcile the engine sent ahead, held for a chunk that is not
 * loaded, lands before any block entity in the chunk ticks; its IND_RECONCILE_RESULT is journaled before
 * the chunk's IND_CHUNK_LOADED, which carries the epoch now applied; it is exactly once across a crash at
 * any step; and a partly loaded district sends IND_APPROACH for its unloaded part, closing only while a
 * player really closes in. Far chunks, each test its own batch, either engine. The timeouts are long: the
 * GameTest server ticks unthrottled, so generating and unloading a far chunk costs thousands of ticks.
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class ReconcileAheadGameTests {
    private ReconcileAheadGameTests() {}

    /** A ticket that keeps a chunk FULL (level 33) and not block-ticking: block entities placed there never tick. */
    private static final TicketType<ChunkPos> FULL_ONLY = TicketType.create("mic_rtsim_create_full_only", Comparator.comparingLong(ChunkPos::toLong));

    private static final String IRON = "i/64/minecraft:iron_ingot";
    private static final String GOLD = "i/64/minecraft:gold_ingot";

    /**
     * A wait on the world's IO (a far chunk generating, loading, saving its entities): the GameTest server
     * ticks as fast as it can, so on a busy machine a timeout in ticks passes before the IO is done. While
     * the check fails, the tick waits 5 ms, so a timeout of 12,000 ticks is at least a minute of IO.
     */
    private static Runnable io(Runnable check) {
        return () -> {
            try {
                check.run();
            } catch (RuntimeException e) {
                try {
                    Thread.sleep(5);
                } catch (InterruptedException ie) {
                    Thread.currentThread().interrupt();
                }
                throw e;
            }
        };
    }

    /** Where a scripted command is (a held check that fails says why). */
    static String pipeline(long op) {
        StringBuilder b = new StringBuilder();
        b.append(": held ").append(ind().reconciler().held()).append(", result ").append(result(op) != null)
                .append(", command results ").append(commandResults(op)).append(", industry on ").append(ind().enabled());
        com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession s = com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession.current();
        if (s != null) {
            com.minecraftindustrialcomplex.mic_rtsim.boundary.Handover h = s.boundary().handover();
            b.append(", session failed ").append(s.failed()).append(", submitted ").append(h.submitted()).append(", in flight ")
                    .append(h.stepInFlight()).append(", commands pending ").append(h.commandsPending()).append(", ticks waiting ").append(h.ticksWaiting())
                    .append(", stale skipped ").append(s.boundary().skippedStaleCommands()).append(", malformed ").append(s.boundary().malformed())
                    .append(", last error ").append(s.boundary().lastError());
            if (s.engine() instanceof com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine st)
                b.append(", stub steps ").append(st.steps()).append(", scripted pending ").append(st.scriptedPending()).append(", stub error ").append(st.protocolError());
        }
        return b.toString();
    }

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
    @GameTest(template = YARD, batch = "ahead_1", timeoutTicks = ChunkWait.NO_TICK_TIMEOUT)
    public static void aHeldReconcileLandsBeforeAHopperInItsChunkMoves(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        ChunkWait waits = ChunkWait.of(helper, 12000);
        int fx = farChunk(helper, true, 3200), fz = farChunk(helper, false, 3200);
        waits.onEnd(() -> level.setChunkForced(fx, fz, false));
        ChunkPos cp = new ChunkPos(fx, fz);
        BlockPos chestPos = new BlockPos(fx * 16 + 6, 100, fz * 16 + 6);
        BlockPos hopperPos = chestPos.below();
        long op = engineOp(), more = engineOp();
        long[] id = {0};
        int[] heldBefore = {0};
        helper.startSequence()
                // Generated and loaded under a forced ticket, then held at FULL alone.
                .thenExecute(() -> {
                    level.getChunkSource().addRegionTicket(FULL_ONLY, cp, 0, cp);
                    level.setChunkForced(fx, fz, true);
                })
                .thenWaitUntil(waits.chunks(() -> check("the far chunk loads", level.getChunkSource().getChunkNow(fx, fz) != null)))
                .thenExecute(() -> level.setChunkForced(fx, fz, false))
                .thenWaitUntil(waits.chunks(() -> {
                    LevelChunk c = level.getChunkSource().getChunkNow(fx, fz);
                    check("the far chunk is loaded and not block-ticking: " + (c == null ? "not loaded" : c.getFullStatus()),
                            c != null && !c.getFullStatus().isOrAfter(FullChunkStatus.BLOCK_TICKING));
                }))
                .thenExecute(() -> {
                    ind().districts().forgetArea(level, fx - 1, fz - 1, fx + 1, fz + 1);
                    level.setBlock(chestPos, Blocks.CHEST.defaultBlockState(), 3);
                    level.setBlock(hopperPos, Blocks.HOPPER.defaultBlockState(), 3);
                    container(level, chestPos).setItem(0, new ItemStack(Items.IRON_INGOT, 9));
                    playerPlaced(helper, chestPos);
                    playerPlaced(helper, hopperPos);
                })
                .thenWaitUntil(waits.engine(() -> {
                    District d = ind().districtAt(level, chestPos);
                    check("district", d != null);
                    id[0] = d.id;
                }))
                .thenExecute(() -> {
                    check("the hopper never ticked while the chunk was FULL only", count(container(level, chestPos), Items.IRON_INGOT) == 9
                            && count(container(level, hopperPos), Items.IRON_INGOT) == 0);
                    level.getChunkSource().removeRegionTicket(FULL_ONLY, cp, 0, cp);
                })
                .thenWaitUntil(waits.chunks(() -> check("the far chunk unloads (saved)", unloaded(level, fx, fz, id[0]))))
                .thenExecute(() -> {
                    heldBefore[0] = ind().reconciler().held();
                    reconcile(op, id[0], 1, level, fx, fz, List.of(new Delta(0, IRON, chestPos, -9)), List.of());
                })
                .thenWaitUntil(waits.engine(() -> check("the reconcile sent ahead is held" + pipeline(op), ind().reconciler().held() == heldBefore[0] + 1 && result(op) == null)))
                .thenExecute(() -> level.setChunkForced(fx, fz, true))
                .thenWaitUntil(waits.chunks(() -> loads(level, fx, fz)))
                .thenWaitUntil(waits.engine(() -> {
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
                }))
                // The chunk ticks again once the reconcile landed: an iron added now reaches the hopper.
                .thenExecute(() -> reconcile(more, id[0], 2, level, fx, fz, List.of(new Delta(0, IRON, chestPos, 1)), List.of()))
                .thenWaitUntil(waits.engine(() -> {
                    LevelChunk c = level.getChunkSource().getChunkNow(fx, fz);
                    ByteBuffer r = result(more);
                    check("the hopper pulls once the chunk ticks: chest " + count(container(level, chestPos), Items.IRON_INGOT)
                            + ", hopper " + count(container(level, hopperPos), Items.IRON_INGOT)
                            + ", +1 result " + (r == null ? "none" : r.getLong(IndRecords.ReconcileResult.ENTRIES + 16))
                            + ", held " + ind().reconciler().held()
                            + ", chunk " + (c == null ? "not loaded" : c.getFullStatus() + (TickGate.holds(c) ? " gated" : ""))
                            + ", block ticking " + level.shouldTickBlocksAt(cp.toLong())
                            + ", hopper " + level.getBlockState(hopperPos) + ", below " + level.getBlockState(hopperPos.below()),
                            count(container(level, hopperPos), Items.IRON_INGOT) == 1);
                }))
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
    @GameTest(template = YARD, batch = "ahead_2", timeoutTicks = ChunkWait.NO_TICK_TIMEOUT)
    public static void aReconcileSentAheadAppliesOnceAcrossACrashAtEachStep(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        ChunkWait waits = ChunkWait.of(helper, 16000);
        int fx = farChunk(helper, true, 3300), fz = farChunk(helper, false, 3300);
        waits.onEnd(() -> level.setChunkForced(fx, fz, false));
        BlockPos chestPos = new BlockPos(fx * 16 + 9, 100, fz * 16 + 4);
        long a = engineOp(), b = engineOp(), c = engineOp(), d4 = engineOp();
        long[] id = {0};
        int[] mark = {0};
        helper.startSequence()
                .thenExecute(() -> level.setChunkForced(fx, fz, true))
                .thenWaitUntil(waits.chunks(() -> check("the far chunk loads", level.getChunkSource().getChunkNow(fx, fz) != null)))
                .thenExecute(() -> {
                    ind().districts().forgetArea(level, fx - 1, fz - 1, fx + 1, fz + 1);
                    level.setBlock(chestPos, Blocks.CHEST.defaultBlockState(), 3);
                    playerPlaced(helper, chestPos);
                })
                .thenWaitUntil(waits.engine(() -> {
                    District d = ind().districtAt(level, chestPos);
                    check("district", d != null);
                    id[0] = d.id;
                }))
                .thenExecute(() -> level.setChunkForced(fx, fz, false))
                .thenWaitUntil(waits.chunks(() -> check("unloads", unloaded(level, fx, fz, id[0]))))
                // 1. Held, then the server stops before the load.
                .thenExecute(() -> {
                    mark[0] = ind().reconciler().held();
                    reconcile(a, id[0], 1, level, fx, fz, List.of(new Delta(0, GOLD, chestPos, 7)), List.of());
                })
                .thenWaitUntil(waits.engine(() -> check("held" + pipeline(a), ind().reconciler().held() == mark[0] + 1)))
                .thenExecute(() -> {
                    ind().reconciler().restartForTest(); // what a restart loses: the held commands live in memory
                    mark[0] = loadedCount(id[0], fx, fz);
                    level.setChunkForced(fx, fz, true);
                })
                .thenWaitUntil(waits.chunks(() -> loads(level, fx, fz)))
                .thenWaitUntil(waits.engine(() -> {
                    check("the load is reported", loadedCount(id[0], fx, fz) > mark[0]);
                    check("with epoch 0: nothing was applied", lastLoaded(id[0], fx, fz).view().getInt(IndRecords.ChunkLoaded.APPLIED_EPOCH) == 0);
                    check("no result for a yet", result(a) == null);
                }))
                .thenExecute(() -> reconcile(a, id[0], 1, level, fx, fz, List.of(new Delta(0, GOLD, chestPos, 7)), List.of())) // the engine answers the load (F27)
                .thenWaitUntil(waits.engine(() -> {
                    check("a applied once at the engine's answer", result(a) != null && result(a).getLong(IndRecords.ReconcileResult.ENTRIES + 16) == 7);
                    check("7 gold", count(container(level, chestPos), Items.GOLD_INGOT) == 7);
                    check("epoch 1", epochIn(level, fx, fz, id[0]) == 1);
                }))
                // 2. Held and applied at the load.
                .thenExecute(() -> level.setChunkForced(fx, fz, false))
                .thenWaitUntil(waits.chunks(() -> check("unloads", unloaded(level, fx, fz, id[0]))))
                .thenExecute(() -> {
                    mark[0] = ind().reconciler().held();
                    reconcile(b, id[0], 2, level, fx, fz, List.of(new Delta(0, GOLD, chestPos, 5)), List.of());
                })
                .thenWaitUntil(waits.engine(() -> check("held" + pipeline(b), ind().reconciler().held() == mark[0] + 1)))
                .thenExecute(() -> level.setChunkForced(fx, fz, true))
                .thenWaitUntil(waits.chunks(() -> loads(level, fx, fz)))
                .thenWaitUntil(waits.engine(() -> {
                    check("b applied at the load", result(b) != null && result(b).getLong(IndRecords.ReconcileResult.ENTRIES + 16) == 5);
                    check("12 gold", count(container(level, chestPos), Items.GOLD_INGOT) == 12);
                    ScriptedEngine.Received loaded = lastLoaded(id[0], fx, fz);
                    check("IND_CHUNK_LOADED after b's result, with epoch 2", loaded != null && resultRecord(b).obsSeq() < loaded.obsSeq()
                            && loaded.view().getInt(IndRecords.ChunkLoaded.APPLIED_EPOCH) == 2);
                }))
                // 3. The chunk saved ahead of the journal: b again (same op) and epoch 2 under op c.
                .thenExecute(() -> level.setChunkForced(fx, fz, false))
                .thenWaitUntil(waits.chunks(() -> check("unloads with epoch 2 on disk", unloaded(level, fx, fz, id[0]))))
                .thenExecute(() -> {
                    ind().reconciler().restartForTest(b); // the journal rolled back past b: only the chunk's epoch knows
                    mark[0] = ind().reconciler().held();
                    reconcile(b, id[0], 2, level, fx, fz, List.of(new Delta(0, GOLD, chestPos, 5)), List.of());
                    reconcile(c, id[0], 2, level, fx, fz, List.of(new Delta(0, GOLD, chestPos, 5)), List.of());
                })
                .thenWaitUntil(waits.engine(() -> check("both held" + pipeline(c), ind().reconciler().held() == mark[0] + 2)))
                .thenExecute(() -> level.setChunkForced(fx, fz, true))
                .thenWaitUntil(waits.chunks(() -> loads(level, fx, fz)))
                .thenWaitUntil(waits.engine(() -> {
                    check("b again is a duplicate", commandResults(b).contains(Obs.CommandResult.DUPLICATE_OP));
                    check("c, epoch 2 under a new op id, is a duplicate", commandResults(c).contains(Obs.CommandResult.DUPLICATE_OP));
                    check("still 12 gold", count(container(level, chestPos), Items.GOLD_INGOT) == 12);
                    check("IND_CHUNK_LOADED says epoch 2", lastLoaded(id[0], fx, fz).view().getInt(IndRecords.ChunkLoaded.APPLIED_EPOCH) == 2);
                }))
                // 4. Applied at the load, and the chunk on disk predates it.
                .thenExecute(() -> level.setChunkForced(fx, fz, false))
                .thenWaitUntil(waits.chunks(() -> check("unloads", unloaded(level, fx, fz, id[0]))))
                .thenExecute(() -> {
                    mark[0] = ind().reconciler().held();
                    reconcile(d4, id[0], 3, level, fx, fz, List.of(new Delta(0, GOLD, chestPos, 4)), List.of());
                })
                .thenWaitUntil(waits.engine(() -> check("held" + pipeline(d4), ind().reconciler().held() == mark[0] + 1)))
                .thenExecute(() -> level.setChunkForced(fx, fz, true))
                .thenWaitUntil(waits.chunks(() -> loads(level, fx, fz)))
                .thenWaitUntil(waits.engine(() -> check("d applied at the load: 16 gold", count(container(level, chestPos), Items.GOLD_INGOT) == 16 && epochIn(level, fx, fz, id[0]) == 3)))
                .thenExecute(() -> {
                    // The crash: the chunk that reaches disk is the one from before d (12 gold, epoch 2).
                    Container chest = container(level, chestPos);
                    chest.clearContent();
                    chest.setItem(0, new ItemStack(Items.GOLD_INGOT, 12));
                    LevelChunk chunk = level.getChunkSource().getChunkNow(fx, fz);
                    chunk.setData(IndustryAttachments.RECONCILE.get(), IndustryAttachments.Epochs.EMPTY
                            .with(new IndustryAttachments.EpochEntry(id[0], 2, ind().engineLife())));
                    chunk.setUnsaved(true);
                    ind().reconciler().restartForTest(d4); // the journal rolled back to the same save: d never happened
                    level.setChunkForced(fx, fz, false);
                })
                .thenWaitUntil(waits.chunks(() -> check("unloads with the older chunk", unloaded(level, fx, fz, id[0]))))
                .thenExecute(() -> {
                    mark[0] = ind().reconciler().held();
                    reconcile(d4, id[0], 3, level, fx, fz, List.of(new Delta(0, GOLD, chestPos, 4)), List.of()); // the engine never had d's result
                })
                .thenWaitUntil(waits.engine(() -> check("held" + pipeline(d4), ind().reconciler().held() == mark[0] + 1)))
                .thenExecute(() -> level.setChunkForced(fx, fz, true))
                .thenWaitUntil(waits.chunks(() -> loads(level, fx, fz)))
                .thenWaitUntil(waits.engine(() -> {
                    check("the older chunk takes d once: 16 gold", count(container(level, chestPos), Items.GOLD_INGOT) == 16);
                    check("epoch 3", epochIn(level, fx, fz, id[0]) == 3);
                    record("aReconcileSentAheadAppliesOnceAcrossACrashAtEachStep", "results",
                            "a " + commandResults(a) + ", b " + commandResults(b) + ", c " + commandResults(c) + ", d " + commandResults(d4));
                }))
                .thenExecute(() -> {
                    level.setBlock(chestPos, Blocks.AIR.defaultBlockState(), 3);
                    ind().districts().forgetArea(level, fx - 1, fz - 1, fx + 1, fz + 1);
                    level.setChunkForced(fx, fz, false);
                })
                .thenSucceed();
    }

    /** What a crash leaves on disk at one instant: a chunk's reconcile state and gold from its region file, and the journal's file. */
    private record OnDisk(IndustryAttachments.Epochs epochs, int gold, boolean journalHasOp, String error, long stamp) {}

    /**
     * The crash test's far chunk, watched: every serialization of it ({@code ChunkDataEvent.Save}: what the
     * tag holds, on which thread, from which chunk object) and every load, so a state on disk can be traced
     * to the save that wrote it. Listeners registered once; they look at {@link #WATCH} only.
     */
    private record Watch(net.minecraft.resources.ResourceKey<Level> dim, ChunkPos pos, long district, BlockPos chest, List<String> events) {}

    private static volatile Watch WATCH;
    private static final java.util.concurrent.atomic.AtomicBoolean WATCHING = new java.util.concurrent.atomic.AtomicBoolean();

    private static void watch(ServerLevel level, ChunkPos pos, long district, BlockPos chest) {
        if (WATCHING.compareAndSet(false, true)) {
            net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener(
                    (net.neoforged.neoforge.event.level.ChunkDataEvent.Save e) -> seen("save", e.getLevel(), e.getChunk(), e.getData()));
            net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener(
                    (net.neoforged.neoforge.event.level.ChunkDataEvent.Load e) -> seen("load", e.getLevel(), e.getChunk(), e.getData()));
        }
        WATCH = new Watch(level.dimension(), pos, district, chest, java.util.Collections.synchronizedList(new ArrayList<>()));
    }

    private static void seen(String what, net.minecraft.world.level.LevelAccessor l, net.minecraft.world.level.chunk.ChunkAccess chunk, CompoundTag tag) {
        Watch w = WATCH;
        if (w == null || !(l instanceof ServerLevel level) || level.dimension() != w.dim() || !chunk.getPos().equals(w.pos()))
            return;
        int epoch = epochsIn(tag).epoch(w.district());
        int gold = goldIn(tag, w.chest());
        StringBuilder b = new StringBuilder().append(what).append(" t=").append(level.getGameTime()).append(" ms=").append(System.currentTimeMillis() % 1_000_000)
                .append(' ').append(Thread.currentThread().getName()).append(' ').append(chunk.getClass().getSimpleName()).append('@')
                .append(Integer.toHexString(System.identityHashCode(chunk))).append(" epoch ").append(epoch).append(", gold ").append(gold);
        if ("save".equals(what) && gold > 0 && epoch == 0) {
            StackTraceElement[] st = new Throwable().getStackTrace();
            for (int i = 0; i < Math.min(st.length, 30); i++)
                b.append(" < ").append(st[i].getClassName().substring(st[i].getClassName().lastIndexOf('.') + 1)).append('.').append(st[i].getMethodName());
        }
        w.events().add(b.toString());
    }

    private static String trace() {
        Watch w = WATCH;
        com.minecraftindustrialcomplex.mic_rtsim.journal.ChunkWriteBarrier b = barrier();
        return "; barrier: noted " + b.noted() + ", rounds " + b.rounds() + ", forced " + b.forcedSaves() + ", pending " + b.pending()
                + ", held " + b.held() + ", late " + b.late() + ", wait " + b.lastWaitMillis() + " ms; the chunk's saves and loads: "
                + (w == null ? "-" : String.join(" | ", new ArrayList<>(w.events())));
    }

    private static IndustryAttachments.Epochs epochsIn(@org.jetbrains.annotations.Nullable CompoundTag chunk) {
        if (chunk == null)
            return IndustryAttachments.Epochs.EMPTY;
        Tag a = chunk.getCompound(AttachmentHolder.ATTACHMENTS_NBT_KEY).get("mic_rtsim:reconcile");
        return a == null ? IndustryAttachments.Epochs.EMPTY : IndustryAttachments.Epochs.CODEC.parse(NbtOps.INSTANCE, a).getOrThrow();
    }

    private static int goldIn(@org.jetbrains.annotations.Nullable CompoundTag chunk, BlockPos chest) {
        int gold = 0;
        if (chunk == null)
            return 0;
        ListTag bes = chunk.getList("block_entities", Tag.TAG_COMPOUND);
        for (int i = 0; i < bes.size(); i++) {
            CompoundTag be = bes.getCompound(i);
            if (be.getInt("x") != chest.getX() || be.getInt("y") != chest.getY() || be.getInt("z") != chest.getZ())
                continue;
            ListTag items = be.getList("Items", Tag.TAG_COMPOUND);
            for (int j = 0; j < items.size(); j++)
                if ("minecraft:gold_ingot".equals(items.getCompound(j).getString("id")))
                    gold += items.getCompound(j).getInt("count");
        }
        return gold;
    }

    /**
     * Saves the world as an autosave does ({@code saveEverything}), and right after the journal's file is
     * written reads what a crash at that instant leaves: the chunk from its region file (not through the IO
     * worker, whose queue serves a chunk not yet written), and whether the journal's file holds {@code op}
     * applied. The read runs on NeoForge's SavedData IO chain, queued after the journal's write, as the bridge's
     * WORLD_SAVED marker is (BridgeEvents.onLevelSave).
     */
    private static void saveAndPeek(ServerLevel level, int cx, int cz, BlockPos chest, long op, AtomicReference<OnDisk> out) {
        out.set(null);
        MinecraftServer server = level.getServer();
        server.saveEverything(true, false, false);
        Path root = server.getWorldPath(LevelResource.ROOT);
        Path region = DimensionType.getStorageFolder(level.dimension(), root).resolve("region");
        Path journal = DimensionType.getStorageFolder(Level.OVERWORLD, root).resolve("data").resolve(JournalSavedData.FILE + ".dat");
        HolderLookup.Provider registries = level.registryAccess();
        IOUtilities.withIOWorker(() -> {
            try {
                CompoundTag chunk = readRegionChunk(region, new ChunkPos(cx, cz));
                IndustryAttachments.Epochs epochs = epochsIn(chunk);
                int gold = goldIn(chunk, chest);
                long stamp = regionStamp(region, new ChunkPos(cx, cz));
                CompoundTag j = NbtIo.readCompressed(journal, NbtAccounter.unlimitedHeap());
                boolean has = JournalSavedData.load(j.getCompound("data"), registries).journal().commandOpApplied(op);
                out.set(new OnDisk(epochs, gold, has, chunk == null ? "the chunk is not in its region file" : null, stamp));
            } catch (Exception e) {
                out.set(new OnDisk(IndustryAttachments.Epochs.EMPTY, -1, false, e.toString(), -1));
            }
        });
    }

    private static com.minecraftindustrialcomplex.mic_rtsim.journal.ChunkWriteBarrier barrier() {
        return com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession.current().journal().chunks();
    }

    private static String barrierLine() {
        com.minecraftindustrialcomplex.mic_rtsim.journal.ChunkWriteBarrier b = barrier();
        return "the journal's write waited " + b.lastWaitMillis() + " ms (max " + b.maxWaitMillis() + " ms; " + b.forcedSaves()
                + " chunks saved early, " + b.held() + " writes held, " + b.late() + " late so far)";
    }

    /** When the region file says the chunk was last written (its timestamp table, seconds), -1 when it is not there. */
    private static long regionStamp(Path regionDir, ChunkPos pos) throws java.io.IOException {
        Path file = regionDir.resolve("r." + pos.getRegionX() + "." + pos.getRegionZ() + ".mca");
        if (!Files.isRegularFile(file))
            return -1;
        try (FileChannel ch = FileChannel.open(file, StandardOpenOption.READ)) {
            ByteBuffer t = ByteBuffer.allocate(4);
            ch.read(t, 4096 + (pos.getRegionLocalX() + pos.getRegionLocalZ() * 32) * 4L);
            return t.getInt(0) & 0xFFFF_FFFFL;
        }
    }

    /** One chunk's NBT straight from its region file (the anvil format: a sector table, then length, compression, data). */
    private static CompoundTag readRegionChunk(Path regionDir, ChunkPos pos) throws java.io.IOException {
        Path file = regionDir.resolve("r." + pos.getRegionX() + "." + pos.getRegionZ() + ".mca");
        if (!Files.isRegularFile(file))
            return null;
        try (FileChannel ch = FileChannel.open(file, StandardOpenOption.READ)) {
            ByteBuffer header = ByteBuffer.allocate(4096);
            ch.read(header, 0);
            int entry = header.getInt((pos.getRegionLocalX() + pos.getRegionLocalZ() * 32) * 4);
            if (entry == 0)
                return null;
            ByteBuffer data = ByteBuffer.allocate((entry & 0xFF) * 4096);
            ch.read(data, (long) (entry >>> 8) * 4096);
            data.flip();
            int length = data.getInt();
            byte version = data.get();
            if ((version & 0x80) != 0)
                throw new java.io.IOException("the chunk is in an external .mcc file");
            byte[] bytes = new byte[length - 1];
            data.get(bytes);
            try (java.io.DataInputStream in = new java.io.DataInputStream(
                    RegionFileVersion.fromId(version).wrap(new java.io.ByteArrayInputStream(bytes)))) {
                return NbtIo.read(in, NbtAccounter.unlimitedHeap());
            }
        }
    }

    /**
     * Waits for the peek of {@link #saveAndPeek} and checks I8 on it: a journal on disk that holds {@code op}
     * never meets a chunk on disk without it ({@code epoch}, {@code gold}). A journal write the barrier held (its
     * chunk not on disk yet, docs/choices-journal-barrier.md JB1) leaves the file as it was: the world saves
     * again and is looked at again, until the file has {@code op}.
     */
    private static Runnable journalOnDiskHas(ServerLevel level, int cx, int cz, BlockPos chest, long op, long[] districtId, int epoch, int gold,
                                             AtomicReference<OnDisk> disk, int[] saves) {
        return io(() -> {
            long district = districtId[0]; // read now: the sequence is built before the district is detected
            OnDisk d = disk.get();
            check("the files are read after the journal's write", d != null);
            if (d.journalHasOp()) {
                check("read: " + d.error(), d.error() == null);
                check("the journal on disk has op " + op + " applied, so the chunk on disk has it too: epoch "
                        + d.epochs().epoch(district) + ", " + d.gold() + " gold (written at " + d.stamp() + " s, now "
                        + System.currentTimeMillis() / 1000 + " s" + trace() + ")", d.epochs().epoch(district) == epoch && d.gold() == gold);
                return;
            }
            check("read: " + d.error(), d.error() == null || d.gold() >= 0);
            com.minecraftindustrialcomplex.mic_rtsim.journal.ChunkWriteBarrier barrier =
                    com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession.current().journal().chunks();
            saves[0]++;
            record("aReconcileSurvivesACrashBetweenTheJournalWriteAndTheChunkWrite", "journal write held",
                    "save " + saves[0] + ": " + barrier.lastHeld() + "; the chunk on disk epoch " + d.epochs().epoch(district) + ", " + d.gold() + " gold");
            saveAndPeek(level, cx, cz, chest, op, disk);
            check("the journal's file was held (" + barrier.lastHeld() + "); the world saves again", false);
        });
    }

    /**
     * Exactly once across a crash after the journal's file is written and before the chunk's is (I8;
     * docs/choices-repair.md R21, "For Ben" 1 before it; docs/choices-journal-barrier.md). Two reconciles into a
     * chest in a far chunk, each followed by a world save and a look at the files right after the journal's
     * write. A write the barrier held (JB1: its chunk not on disk in time) leaves the file as it was, so the
     * world saves again and the files are looked at again until the journal's file holds the reconcile; I8 is
     * checked on that file, whatever the IO latency, and no write may have gone ahead of its chunk (late):
     * <ol>
     *   <li>held and applied at the chunk's load (+7 gold, epoch 1), the world saved: the chunk on disk already
     *       holds epoch 1;</li>
     *   <li>applied to the loaded chunk (+5 gold, epoch 2) within 10 s of that save, so vanilla's autosave skips
     *       the chunk ({@code ChunkMap.saveChunkIfNeeded}'s eager-save cooldown), and the world saved again. The
     *       journal's file says op b applied; the chunk on disk must hold it too. Then the crash at that instant
     *       and a restart: the chunk takes what was on disk, the journal keeps b (the engine, as old as the
     *       journal, never sends it again), and at its next load the chest has 12 gold, epoch 2, b applied once.</li>
     * </ol>
     */
    @GameTest(template = YARD, batch = "ahead_4", timeoutTicks = ChunkWait.NO_TICK_TIMEOUT)
    public static void aReconcileSurvivesACrashBetweenTheJournalWriteAndTheChunkWrite(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        ChunkWait waits = ChunkWait.of(helper, 16000);
        int fx = farChunk(helper, true, 3600), fz = farChunk(helper, false, 3600);
        waits.onEnd(() -> level.setChunkForced(fx, fz, false));
        BlockPos chestPos = new BlockPos(fx * 16 + 5, 100, fz * 16 + 11);
        long a = engineOp(), b = engineOp();
        long[] id = {0};
        int[] mark = {0};
        AtomicReference<OnDisk> disk = new AtomicReference<>();
        int[] saves = {0};
        long[] late0 = {0};
        helper.startSequence()
                .thenExecute(() -> level.setChunkForced(fx, fz, true))
                .thenWaitUntil(waits.chunks(() -> check("the far chunk loads", level.getChunkSource().getChunkNow(fx, fz) != null)))
                .thenExecute(() -> {
                    ind().districts().forgetArea(level, fx - 1, fz - 1, fx + 1, fz + 1);
                    level.setBlock(chestPos, Blocks.CHEST.defaultBlockState(), 3);
                    playerPlaced(helper, chestPos);
                })
                .thenWaitUntil(waits.engine(() -> {
                    District d = ind().districtAt(level, chestPos);
                    check("district", d != null);
                    id[0] = d.id;
                }))
                .thenExecute(() -> {
                    watch(level, new ChunkPos(fx, fz), id[0], chestPos);
                    level.setChunkForced(fx, fz, false);
                })
                .thenWaitUntil(waits.chunks(() -> check("unloads", unloaded(level, fx, fz, id[0]))))
                // 1. Held and applied at the load, then the world saves.
                .thenExecute(() -> {
                    mark[0] = ind().reconciler().held();
                    reconcile(a, id[0], 1, level, fx, fz, List.of(new Delta(0, GOLD, chestPos, 7)), List.of());
                })
                .thenWaitUntil(waits.engine(() -> check("held" + pipeline(a) + ", chunk loaded now " + (level.getChunkSource().getChunkNow(fx, fz) != null)
                        + ", loads reported " + loadedCount(id[0], fx, fz), ind().reconciler().held() == mark[0] + 1)))
                .thenExecute(() -> level.setChunkForced(fx, fz, true))
                .thenWaitUntil(waits.chunks(() -> loads(level, fx, fz)))
                .thenWaitUntil(waits.engine(() -> {
                    check("a applied at the load" + pipeline(a), result(a) != null && result(a).getLong(IndRecords.ReconcileResult.ENTRIES + 16) == 7);
                    check("7 gold, epoch 1", count(container(level, chestPos), Items.GOLD_INGOT) == 7 && epochIn(level, fx, fz, id[0]) == 1);
                }))
                .thenExecute(() -> {
                    late0[0] = barrier().late();
                    note(level, fx, fz, "first save");
                    saveAndPeek(level, fx, fz, chestPos, a, disk);
                })
                .thenWaitUntil(waits.chunks(journalOnDiskHas(level, fx, fz, chestPos, a, id, 1, 7, disk, saves)))
                .thenExecute(() -> {
                    OnDisk d = disk.get();
                    record("aReconcileSurvivesACrashBetweenTheJournalWriteAndTheChunkWrite", "after the first save",
                            "chunk on disk epoch " + d.epochs().epoch(id[0]) + ", " + d.gold() + " gold; journal on disk has a: " + d.journalHasOp()
                                    + "; " + barrierLine() + trace());
                })
                // 2. Applied to the loaded chunk right after that save (the chunk's eager-save cooldown), then saved.
                .thenExecute(() -> reconcile(b, id[0], 2, level, fx, fz, List.of(new Delta(0, GOLD, chestPos, 5)), List.of()))
                .thenWaitUntil(waits.engine(() -> {
                    check("b applied" + pipeline(b), result(b) != null && result(b).getLong(IndRecords.ReconcileResult.ENTRIES + 16) == 5);
                    check("12 gold, epoch 2", count(container(level, chestPos), Items.GOLD_INGOT) == 12 && epochIn(level, fx, fz, id[0]) == 2);
                }))
                .thenExecute(() -> {
                    note(level, fx, fz, "second save");
                    saveAndPeek(level, fx, fz, chestPos, b, disk);
                })
                .thenWaitUntil(waits.chunks(journalOnDiskHas(level, fx, fz, chestPos, b, id, 2, 12, disk, saves)))
                .thenExecute(() -> {
                    OnDisk d = disk.get();
                    record("aReconcileSurvivesACrashBetweenTheJournalWriteAndTheChunkWrite", "after the second save",
                            "chunk on disk epoch " + d.epochs().epoch(id[0]) + ", " + d.gold() + " gold; journal on disk has b: " + d.journalHasOp()
                                    + "; " + barrierLine() + "; " + saves[0] + " saves held" + trace());
                    check("no journal write went ahead of its chunks: " + (barrier().late() - late0[0]) + " late", barrier().late() == late0[0]);
                })
                .thenExecute(() -> {
                    // The crash right after the journal's write, and the restart: the chunk is what its region file
                    // held, the journal keeps b (nothing rolled back), and the engine, as old as the journal, has b's
                    // result and sends nothing again.
                    OnDisk d = disk.get();
                    Container chest = container(level, chestPos);
                    chest.clearContent();
                    if (d.gold() > 0)
                        chest.setItem(0, new ItemStack(Items.GOLD_INGOT, d.gold()));
                    LevelChunk chunk = level.getChunkSource().getChunkNow(fx, fz);
                    chunk.setData(IndustryAttachments.RECONCILE.get(), d.epochs());
                    chunk.setUnsaved(true);
                    ind().reconciler().restartForTest();
                    mark[0] = loadedCount(id[0], fx, fz);
                    level.setChunkForced(fx, fz, false);
                })
                .thenWaitUntil(waits.chunks(() -> check("unloads with what was on disk", unloaded(level, fx, fz, id[0]))))
                .thenExecute(() -> level.setChunkForced(fx, fz, true))
                .thenWaitUntil(waits.chunks(() -> loads(level, fx, fz)))
                .thenWaitUntil(waits.engine(() -> {
                    check("the load is reported", loadedCount(id[0], fx, fz) > mark[0]);
                    int gold = count(container(level, chestPos), Items.GOLD_INGOT);
                    record("aReconcileSurvivesACrashBetweenTheJournalWriteAndTheChunkWrite", "after the restart",
                            gold + " gold, epoch " + epochIn(level, fx, fz, id[0]) + ", a " + commandResults(a) + ", b " + commandResults(b));
                    check("b's gold survived the crash, once: 12 gold, got " + gold, gold == 12);
                    check("epoch 2", epochIn(level, fx, fz, id[0]) == 2);
                    check("IND_CHUNK_LOADED says epoch 2", lastLoaded(id[0], fx, fz).view().getInt(IndRecords.ChunkLoaded.APPLIED_EPOCH) == 2);
                    check("a and b each applied once", commandResults(a).stream().filter(s -> s == Obs.CommandResult.APPLIED).count() == 1
                            && commandResults(b).stream().filter(s -> s == Obs.CommandResult.APPLIED).count() == 1);
                }))
                .thenExecute(() -> {
                    WATCH = null;
                    level.setBlock(chestPos, Blocks.AIR.defaultBlockState(), 3);
                    ind().districts().forgetArea(level, fx - 1, fz - 1, fx + 1, fz + 1);
                    level.setChunkForced(fx, fz, false);
                })
                .thenSucceed();
    }

    /** The watched chunk in memory at a step of the test. */
    private static void note(ServerLevel level, int cx, int cz, String step) {
        Watch w = WATCH;
        LevelChunk c = level.getChunkSource().getChunkNow(cx, cz);
        if (w != null)
            w.events().add(step + " t=" + level.getGameTime() + " ms=" + System.currentTimeMillis() % 1_000_000 + " in memory "
                    + (c == null ? "not loaded" : "LevelChunk@" + Integer.toHexString(System.identityHashCode(c)) + " epoch "
                    + c.getData(IndustryAttachments.RECONCILE.get()).epoch(w.district()) + ", unsaved " + c.isUnsaved()));
    }

    /**
     * A partly loaded district (I7): chunk A loaded, chunk B four chunks east of it unloaded (a member
     * detection kept). A player west of A walks east, one block a tick: IND_APPROACH goes out for the
     * district, closing, with the distance to B (the unloaded part), not to the district's box, and an ETA
     * near that distance. Standing still is not closing. Once B loads, nothing of the district is unloaded
     * and the approach records stop.
     */
    @GameTest(template = YARD, batch = "ahead_3", timeoutTicks = ChunkWait.NO_TICK_TIMEOUT)
    public static void aPartlyLoadedDistrictSendsApproachForItsUnloadedPart(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        ChunkWait waits = ChunkWait.of(helper, 12000);
        int ax = farChunk(helper, true, 3400), az = farChunk(helper, false, 3400);
        int bx = ax + 4;
        waits.onEnd(() -> {
            level.setChunkForced(ax, az, false);
            level.setChunkForced(bx, az, false);
        });
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
                .thenWaitUntil(waits.chunks(() -> check("chunk A loads", level.getChunkSource().getChunkNow(ax, az) != null)))
                .thenExecute(() -> {
                    ind().districts().forgetArea(level, ax - 1, az - 1, bx + 1, az + 1);
                    level.setBlock(chestPos, Blocks.CHEST.defaultBlockState(), 3);
                    playerPlaced(helper, chestPos);
                })
                .thenWaitUntil(waits.engine(() -> {
                    District d = ind().districtAt(level, chestPos);
                    check("district", d != null);
                    id[0] = d.id;
                }))
                .thenWaitUntil(waits.chunks(() -> check("B is not loaded (four chunks from a forced chunk)", level.getChunkSource().getChunkNow(bx, az) == null)))
                .thenExecute(() -> ind().districts().addSeededChunkForTest(ind().district(id[0]), ChunkPos.asLong(bx, az)))
                .thenWaitUntil(waits.engine(() -> check("partly loaded", ind().district(id[0]).loadState() == IndRecords.LoadState.PARTIAL)))
                .thenExecute(() -> {
                    // West of A, 120 blocks from A's west edge: the district's box is 120 away, B is 184 away.
                    player[0] = FakePlayers.add(level, new Vec3(ax * 16 - 120 + 0.5, 100, az * 16 + 8.5), "ahead_walker");
                    mark[0] = received(IndRecords.IND_APPROACH, IndRecords.Approach.DISTRICT, id[0]).size();
                    walk[0] = 1;
                })
                .thenWaitUntil(waits.engine(() -> check("approach records while walking",
                        received(IndRecords.IND_APPROACH, IndRecords.Approach.DISTRICT, id[0]).size() >= mark[0] + 3)))
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
                .thenWaitUntil(waits.engine(() -> check("standing still",
                        received(IndRecords.IND_APPROACH, IndRecords.Approach.DISTRICT, id[0]).size() >= mark[0] + 3)))
                .thenExecute(() -> {
                    List<ByteBuffer> recs = received(IndRecords.IND_APPROACH, IndRecords.Approach.DISTRICT, id[0]);
                    check("standing still is not closing", (aux(recs.get(recs.size() - 1)) & IndRecords.Approach.FLAG_CLOSING) == 0);
                    walk[0] = -1;
                    mark[0] = recs.size();
                })
                .thenWaitUntil(waits.engine(() -> check("walking away",
                        received(IndRecords.IND_APPROACH, IndRecords.Approach.DISTRICT, id[0]).size() >= mark[0] + 2)))
                .thenExecute(() -> {
                    List<ByteBuffer> recs = received(IndRecords.IND_APPROACH, IndRecords.Approach.DISTRICT, id[0]);
                    check("walking away is not closing", (aux(recs.get(recs.size() - 1)) & IndRecords.Approach.FLAG_CLOSING) == 0);
                    walk[0] = 0;
                    level.setChunkForced(bx, az, true);
                })
                .thenWaitUntil(waits.chunks(() -> check("B loads", level.getChunkSource().getChunkNow(bx, az) != null)))
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
