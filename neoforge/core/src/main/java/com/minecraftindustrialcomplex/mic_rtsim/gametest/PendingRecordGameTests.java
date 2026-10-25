package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.journal.JournalSavedData;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.Container;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.animal.horse.TraderLlama;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.phys.AABB;
import net.neoforged.neoforge.event.EventHooks;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.ByteBuffer;
import java.util.List;
import java.util.UUID;
import java.util.stream.Stream;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The world side of a transfer carries its journal records (PendingRecords): a chunk or entity saved
 * on unload can be newer than the journal, and after a crash only it knows what happened.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class PendingRecordGameTests {
    private PendingRecordGameTests() {}

    private static final long ENGINE_OP = 9191;

    private static int diamonds(Container chest) {
        int n = 0;
        for (int i = 0; i < chest.getContainerSize(); i++)
            if (chest.getItem(i).is(Items.DIAMOND))
                n += chest.getItem(i).getCount();
        return n;
    }

    /**
     * The chest's chunk is saved (on unload) after an engine transfer, the journal is not, and the
     * server crashes. The rolled-back engine sends the transfer again; the chunk says it was applied.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 400)
    public static void aChunkSavedAheadOfTheJournalStopsAResentTransfer(GameTestHelper helper) {
        helper.setBlock(new BlockPos(5, 1, 5), Blocks.CHEST);
        BlockPos chestPos = helper.absolutePos(new BlockPos(5, 1, 5));
        ServerLevel level = helper.getLevel();
        LevelChunk chunk = level.getChunkAt(chestPos);
        String dim = RtsimTests.dimension(helper);
        Object[] st = new Object[4]; // session, engine, worldSave, stage
        int[] stage = {0};
        int[] wait = {0};
        boolean[] done = {false};
        helper.onEachTick(() -> {
            if (done[0])
                return;
            RtsimSession s = (RtsimSession) st[0];
            ScriptedEngine e = (ScriptedEngine) st[1];
            if (s != null)
                s.endOfTick();
            if (++wait[0] > 200)
                throw new GameTestAssertException("stuck at stage " + stage[0]);
            Container chest = (Container) level.getBlockEntity(chestPos);
            switch (stage[0]) {
                case 0 -> {
                    JournalSavedData journal = JournalSavedData.fresh();
                    e = RtsimTests.freshEngine();
                    s = new RtsimSession(level.getServer(), e, journal);
                    st[2] = JournalSavedData.roundTrip(journal, level.registryAccess()); // the last world save
                    e.transferToContainer(ENGINE_OP, dim, chestPos.getX(), chestPos.getY(), chestPos.getZ(), "minecraft:diamond", 3);
                    st[0] = s;
                    st[1] = e;
                    stage[0]++;
                }
                case 1 -> {
                    if (diamonds(chest) == 3) {
                        check("the chunk carries the transfer's COMMAND_RESULT", RtsimSession.chunkCarriesOp(chunk, ENGINE_OP));
                        e = e.crashAndReopen(); // crash: neither the journal nor the engine's pages know about it
                        s = new RtsimSession(level.getServer(), e, (JournalSavedData) st[2]);
                        s.reassert(chunk); // what the chunk's load does at the next session start
                        e.transferToContainer(ENGINE_OP, dim, chestPos.getX(), chestPos.getY(), chestPos.getZ(), "minecraft:diamond", 3);
                        st[0] = s;
                        st[1] = e;
                        stage[0]++;
                        wait[0] = 0;
                    }
                }
                case 2 -> {
                    boolean dup = e.received(Obs.COMMAND_RESULT).stream().anyMatch(r -> r.aux() == Obs.CommandResult.DUPLICATE_OP);
                    if (dup) {
                        check("the chest got the diamonds once", diamonds(chest) == 3);
                        check("the reasserted result told the engine the op is done", e.opApplied(ENGINE_OP));
                        e.awaitIdle();
                        e.requestSaveAtNextStep();
                        stage[0]++;
                        wait[0] = 0;
                    }
                }
                case 3 -> {
                    if (!RtsimSession.chunkCarriesOp(chunk, ENGINE_OP)) {
                        record("pending", "chunk ledger", "dropped once the engine acknowledged; chest " + diamonds(chest));
                        e.close();
                        done[0] = true;
                    }
                }
                default -> throw new IllegalStateException();
            }
        });
        helper.succeedWhen(() -> check("finished", done[0]));
    }

    /**
     * A transfer into a chunk that is not loaded is answered NOT_LOADED, never FAILED (RP-210): the chunk
     * may hold the op already, written ahead of the journal, and only it can say so once it loads.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 200)
    public static void aTransferIntoAChunkNotLoadedIsAnsweredNotLoaded(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        BlockPos far = helper.absolutePos(new BlockPos(5, 1, 5)).offset(100_000, 0, 100_000);
        String dim = RtsimTests.dimension(helper);
        long op = ENGINE_OP + 1;
        Object[] st = new Object[2]; // session, engine
        int[] wait = {0};
        boolean[] done = {false};
        helper.onEachTick(() -> {
            if (done[0])
                return;
            if (st[0] == null) {
                check("the far chunk is not loaded", level.getChunkSource().getChunkNow(far.getX() >> 4, far.getZ() >> 4) == null);
                ScriptedEngine e = RtsimTests.freshEngine();
                st[0] = new RtsimSession(level.getServer(), e, JournalSavedData.fresh());
                st[1] = e;
                e.transferToContainer(op, dim, far.getX(), far.getY(), far.getZ(), "minecraft:diamond", 3);
                return;
            }
            RtsimSession s = (RtsimSession) st[0];
            ScriptedEngine e = (ScriptedEngine) st[1];
            s.endOfTick();
            if (++wait[0] > 150)
                throw new GameTestAssertException("no COMMAND_RESULT for the transfer");
            List<Integer> got = e.received(Obs.COMMAND_RESULT).stream()
                    .filter(r -> r.view().getLong(Obs.CommandResult.OP_ID) == op).map(r -> r.aux()).toList();
            if (got.isEmpty())
                return;
            check("answered NOT_LOADED: " + got, got.equals(List.of(Obs.CommandResult.NOT_LOADED)));
            check("nothing carried for it", !s.boundary().journal().commandOpApplied(op));
            check("the chunk was not loaded for it", level.getChunkSource().getChunkNow(far.getX() >> 4, far.getZ() >> 4) == null);
            e.close();
            done[0] = true;
        });
        helper.succeedWhen(() -> check("finished", done[0]));
    }

    /**
     * A tamed llama carries its MEMBER_STATE and TAKEN. A copy of it loaded from disk journals copies of
     * them (REASSERT), which the engine drops: the generation that journaled the TAKEN admitted it. Once
     * the engine acknowledges, both drop them.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 400)
    public static void aTakenCreatureCarriesItsTakenUntilTheEngineHasIt(GameTestHelper helper) {
        long agent = RtsimTests.newAgent();
        CaravanGameTests.spawnCaravan(helper, agent, 1);
        Player player = helper.makeMockPlayer(GameType.SURVIVAL);
        Entity[] e = new Entity[2];
        helper.startSequence()
                .thenWaitUntil(() -> RtsimTests.embodied(agent))
                .thenExecute(() -> {
                    TraderLlama llama = (TraderLlama) RtsimTests.member(RtsimTests.group(agent), 1);
                    llama.dropLeash(true, false);
                    if (!EventHooks.onAnimalTame(llama, player))
                        llama.tameWithName(player);
                    e[0] = llama;
                    PendingRecords p = llama.getExistingDataOrNull(RtsimAttachments.PENDING);
                    check("the tamed llama carries MEMBER_STATE and TAKEN", p != null && p.records().size() == 2
                            && kind(p.records().get(0)) == Obs.MEMBER_STATE && kind(p.records().get(1)) == Obs.TAKEN);
                    // The same llama read back from a chunk saved before the journal (a new UUID, as the
                    // original is still here).
                    CompoundTag nbt = new CompoundTag();
                    llama.saveWithoutId(nbt);
                    nbt.putString("id", "minecraft:trader_llama");
                    Entity copy = EntityType.loadEntityRecursive(nbt, helper.getLevel(), x -> x);
                    copy.setUUID(UUID.randomUUID());
                    helper.getLevel().addLegacyChunkEntities(Stream.of(copy));
                    e[1] = copy;
                    check("the copy joined", helper.getLevel().getEntity(copy.getUUID()) == copy);
                })
                .thenWaitUntil(() -> check("the engine received the TAKEN, then a copy of it",
                        RtsimTests.copiesFor(RtsimTests.engine(), Obs.TAKEN, agent).size() == 1))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> t = RtsimTests.recordsFor(RtsimTests.engine(), Obs.TAKEN, agent);
                    ScriptedEngine.Received copy = RtsimTests.copiesFor(RtsimTests.engine(), Obs.TAKEN, agent).get(0);
                    check("the TAKEN arrived once", t.size() == 1);
                    check("the copy is the TAKEN as first journaled", copy.view().getLong(Obs.OBS_SEQ) == t.get(0).obsSeq()
                            && copy.view().getLong(Obs.Taken.OP_ID) == t.get(0).view().getLong(Obs.Taken.OP_ID));
                    check("under a new sequence", copy.obsSeq() > t.get(0).obsSeq());
                    check("and dropped", copy.skipped());
                    check("so the take counts once", RtsimTests.engine().eventOpCount(t.get(0).view().getLong(Obs.Taken.OP_ID)) == 1);
                    RtsimTests.engine().requestSaveAtNextStep();
                })
                .thenWaitUntil(() -> check("both dropped their records once the engine acknowledged",
                        !e[0].hasData(RtsimAttachments.PENDING) && !e[1].hasData(RtsimAttachments.PENDING)))
                .thenExecute(() -> {
                    e[0].discard();
                    e[1].discard();
                    RtsimTests.engine().despawnGroup(agent, 1);
                })
                .thenWaitUntil(() -> check("the rest despawned", RtsimTests.group(agent) == null))
                .thenSucceed();
    }

    /** A dead member's drops carry its MEMBER_STATE and DIED. */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 300)
    public static void aDeadMembersDropsCarryItsDied(GameTestHelper helper) {
        long agent = RtsimTests.newAgent();
        CaravanGameTests.spawnCaravan(helper, agent, 1);
        helper.startSequence()
                .thenWaitUntil(() -> RtsimTests.embodied(agent))
                .thenExecute(() -> RtsimTests.member(RtsimTests.group(agent), 1).kill())
                .thenExecuteAfter(1, () -> {
                    int carriers = 0;
                    for (ItemEntity item : helper.getLevel().getEntitiesOfClass(ItemEntity.class, new AABB(helper.absolutePos(BlockPos.ZERO)).inflate(12))) {
                        PendingRecords p = item.getExistingDataOrNull(RtsimAttachments.PENDING);
                        if (p != null && p.records().size() == 2 && kind(p.records().get(1)) == Obs.DIED
                                && ByteBuffer.wrap(p.records().get(1)).order(Layout.ORDER).getLong(Obs.BODY) == agent)
                            carriers++;
                    }
                    record("pending", "drops carrying the DIED", carriers);
                    check("the cargo items it dropped carry its DIED", carriers >= 2);
                    RtsimTests.engine().despawnGroup(agent, 1);
                })
                .thenWaitUntil(() -> check("the rest despawned", RtsimTests.group(agent) == null))
                .thenSucceed();
    }

    private static int kind(byte[] record) {
        return Short.toUnsignedInt(ByteBuffer.wrap(record).order(Layout.ORDER).getShort(Layout.R_KIND));
    }
}
