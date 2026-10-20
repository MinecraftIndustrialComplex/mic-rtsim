package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
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
            StubEngine e = (StubEngine) st[1];
            if (s != null)
                s.endOfTick();
            if (++wait[0] > 200)
                throw new GameTestAssertException("stuck at stage " + stage[0]);
            Container chest = (Container) level.getBlockEntity(chestPos);
            switch (stage[0]) {
                case 0 -> {
                    JournalSavedData journal = JournalSavedData.fresh();
                    e = new StubEngine(StubEngine.Pages.EMPTY, null, StubEngine.DEFAULT_CAPACITY);
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
                        e.close(); // crash: neither the journal nor the engine's pages know about it
                        e = new StubEngine(StubEngine.Pages.EMPTY, null, StubEngine.DEFAULT_CAPACITY);
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
     * A tamed llama carries its MEMBER_STATE and TAKEN. A copy of it loaded from disk journals them
     * again, with the same op id; once the engine acknowledges, both drop them.
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
                .thenWaitUntil(() -> check("the stub received the TAKEN twice",
                        RtsimTests.recordsFor(RtsimTests.stub(), Obs.TAKEN, agent).size() == 2))
                .thenExecute(() -> {
                    List<StubEngine.Received> t = RtsimTests.recordsFor(RtsimTests.stub(), Obs.TAKEN, agent);
                    check("with the same op id", t.get(0).view().getLong(Obs.Taken.OP_ID) == t.get(1).view().getLong(Obs.Taken.OP_ID));
                    check("under a new sequence", t.get(1).obsSeq() > t.get(0).obsSeq());
                    RtsimTests.stub().requestSaveAtNextStep();
                })
                .thenWaitUntil(() -> check("both dropped their records once the engine acknowledged",
                        !e[0].hasData(RtsimAttachments.PENDING) && !e[1].hasData(RtsimAttachments.PENDING)))
                .thenExecute(() -> {
                    e[0].discard();
                    e[1].discard();
                    RtsimTests.stub().despawnGroup(agent, 1);
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
                    RtsimTests.stub().despawnGroup(agent, 1);
                })
                .thenWaitUntil(() -> check("the rest despawned", RtsimTests.group(agent) == null))
                .thenSucceed();
    }

    private static int kind(byte[] record) {
        return Short.toUnsignedInt(ByteBuffer.wrap(record).order(Layout.ORDER).getShort(Layout.R_KIND));
    }
}
