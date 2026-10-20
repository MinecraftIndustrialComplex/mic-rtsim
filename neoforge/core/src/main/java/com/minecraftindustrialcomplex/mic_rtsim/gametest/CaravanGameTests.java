package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.body.BodyKinds;
import com.minecraftindustrialcomplex.mic_rtsim.body.vanilla.WanderingTraderAdapter;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.animal.horse.TraderLlama;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.npc.WanderingTrader;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * A trader and its llamas are one unit (C25, PLAN.md §6.1 items 4 to 7): they spawn in one tick, are
 * captured in one tick as soon as any member stops entity-ticking, and no llama ever despawns itself
 * on the {@code TraderLlama.maybeDespawn} trap while embodied.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class CaravanGameTests {
    private CaravanGameTests() {}

    static void spawnCaravan(GameTestHelper helper, long agent, int epoch) {
        RtsimTests.stub().script(caravanCommand(helper, agent, epoch));
    }

    static java.util.function.Consumer<StubEngine.CommandOut> caravanCommand(GameTestHelper helper, long agent, int epoch) {
        Vec3 at = RtsimTests.centre(helper);
        return RtsimTests.stub().spawnGroupCommand(agent, epoch, BodyKinds.HUMAN_CARAVAN, RtsimTests.dimension(helper), at.x, at.y, at.z, List.of(
                StubEngine.MemberSpec.plain(0),
                new StubEngine.MemberSpec(1, 0f, 2, 3, List.of(new StubEngine.ItemSpec(0, "minecraft:emerald", 12),
                        new StubEngine.ItemSpec(4, "minecraft:wheat", 30))),
                StubEngine.MemberSpec.plain(2)));
    }

    /** A chunk far from every test, forced so that its neighbours are loaded but not entity-ticking. */
    static ChunkPos farChunk(GameTestHelper helper, int salt) {
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        return new ChunkPos((o.getX() >> 4) + 200 + 8 * salt, (o.getZ() >> 4) + 200);
    }

    private static int leads(ServerLevel level, AABB box) {
        int n = 0;
        for (ItemEntity item : level.getEntitiesOfClass(ItemEntity.class, box))
            if (item.getItem().is(Items.LEAD))
                n++;
        return n;
    }

    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 600)
    public static void traderAndLlamasCaptureAsOneGroupAndNoLlamaDespawns(GameTestHelper helper) {
        long agent = RtsimTests.newAgent();
        ServerLevel level = helper.getLevel();
        spawnCaravan(helper, agent, 1);
        ChunkPos forced = farChunk(helper, 0);
        BlockPos frozenAt = new BlockPos(forced.getMinBlockX() + 16 + 8, (int) RtsimTests.centre(helper).y, forced.getMinBlockZ() + 8);
        Entity[] bodies = new Entity[3];
        long[] trapTick = new long[1];

        helper.startSequence()
                .thenWaitUntil(() -> RtsimTests.embodied(agent))
                .thenExecute(() -> {
                    PuppetGroup g = RtsimTests.group(agent);
                    for (int i = 0; i < 3; i++)
                        bodies[i] = RtsimTests.member(g, i);
                    check("member 0 is the trader", bodies[0] instanceof WanderingTrader);
                    check("members 1 and 2 are trader llamas", bodies[1] instanceof TraderLlama && bodies[2] instanceof TraderLlama);
                    for (int i = 1; i < 3; i++)
                        check("llama " + i + " is leashed to the trader", ((TraderLlama) bodies[i]).getLeashHolder() == bodies[0]);
                    TraderLlama cargo = (TraderLlama) bodies[1];
                    check("llama 1 kept its variant and strength", cargo.getVariant().getId() == 2 && cargo.getStrength() == 3);
                    check("llama 1 carries its cargo in its chest (C21)", cargo.hasChest()
                            && cargo.getSlot(500).get().is(Items.EMERALD) && cargo.getSlot(500).get().getCount() == 12
                            && cargo.getSlot(504).get().is(Items.WHEAT));
                    // The trap: a trader at despawnDelay 0 never leaves, and its llamas copy 0 - 1 and
                    // discard themselves on their next tick. The bridge refreshes the delay this tick.
                    ((WanderingTrader) bodies[0]).setDespawnDelay(0);
                    trapTick[0] = level.getGameTime();
                    level.setChunkForced(forced.x, forced.z, true);
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    record("caravan", "trader despawnDelay 40 ticks after it was set to 0", ((WanderingTrader) bodies[0]).getDespawnDelay());
                    check("the bridge restored the trader's despawn delay",
                            ((WanderingTrader) bodies[0]).getDespawnDelay() > WanderingTraderAdapter.VANILLA_DESPAWN_DELAY - 100);
                    for (Entity b : bodies)
                        check(b + " is still embodied after the despawnDelay trap", !b.isRemoved());
                    check("no member left the group (a despawn would show as removed by another)",
                            RtsimTests.group(agent) != null && RtsimTests.group(agent).liveMembers() == 3);
                })
                .thenWaitUntil(() -> check("the far chunk is entity-ticking", level.isPositionEntityTicking(forced.getMiddleBlockPosition(64))))
                .thenWaitUntil(() -> check("its neighbour is loaded", level.hasChunk(forced.x + 1, forced.z)))
                .thenExecute(() -> {
                    check("the neighbour is loaded but not entity-ticking", !level.isPositionEntityTicking(frozenAt));
                    // One llama wanders into a chunk that is loaded but frozen: the whole caravan goes.
                    bodies[2].teleportTo(frozenAt.getX() + 0.5, frozenAt.getY(), frozenAt.getZ() + 0.5);
                })
                .thenExecuteAfter(1, () -> {
                    check("the group was captured", RtsimTests.group(agent) == null);
                    for (Entity b : bodies) {
                        check(b + " was removed", b.isRemoved());
                        check(b + " was removed by the bridge (DISCARDED in its capture)", b.getRemovalReason() == Entity.RemovalReason.DISCARDED);
                    }
                    check("no member was taken or removed by anything else",
                            RtsimTests.recordsFor(RtsimTests.stub(), Obs.TAKEN, agent).isEmpty());
                    check("no lead dropped at the caravan", leads(level, new AABB(helper.absolutePos(BlockPos.ZERO)).inflate(16)) == 0);
                    check("no lead dropped at the frozen llama", leads(level, new AABB(frozenAt).inflate(16)) == 0);
                    level.setChunkForced(forced.x, forced.z, false);
                })
                .thenWaitUntil(() -> check("the stub received the capture",
                        !RtsimTests.recordsFor(RtsimTests.stub(), Obs.DEMATERIALIZED, agent).isEmpty()))
                .thenExecute(() -> {
                    List<StubEngine.Received> recs = RtsimTests.allFor(RtsimTests.stub(), agent);
                    List<Integer> kinds = new ArrayList<>();
                    for (StubEngine.Received r : recs)
                        kinds.add(r.kind());
                    record("caravan", "journaled record kinds for the agent", kinds);
                    check("MATERIALIZED, three MEMBER_STATEs, DEMATERIALIZED: " + kinds, kinds.equals(List.of(
                            Obs.MATERIALIZED, Obs.MEMBER_STATE, Obs.MEMBER_STATE, Obs.MEMBER_STATE, Obs.DEMATERIALIZED)));
                    StubEngine.Received demat = recs.get(4);
                    check("captured because a member stopped ticking", demat.aux() == Obs.Dematerialized.NOT_TICKING);
                    check("three members captured", demat.view().getInt(Obs.Dematerialized.MEMBERS) == 3);
                    long t = RtsimTests.gameTimeOf(demat);
                    for (int i = 1; i <= 3; i++)
                        check("member state " + i + " is from the capture tick", RtsimTests.gameTimeOf(recs.get(i)) == t);
                    StubEngine.Received llama1 = recs.get(2).aux() == 1 ? recs.get(2) : recs.get(1).aux() == 1 ? recs.get(1) : recs.get(3);
                    check("llama 1's captured state lists its two cargo stacks", llama1.view().getInt(Obs.MemberState.ITEM_COUNT) == 2);
                    check("llama 1's captured strength", llama1.view().getInt(Obs.MemberState.BODY_B) == 3);
                })
                .thenSucceed();
    }

    /** The other way a member stops ticking: its chunk stops being accessible (tracking ends). */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 300)
    public static void aMemberLeavingTrackingCapturesTheGroup(GameTestHelper helper) {
        long agent = RtsimTests.newAgent();
        spawnCaravan(helper, agent, 1);
        Entity[] bodies = new Entity[3];
        helper.startSequence()
                .thenWaitUntil(() -> RtsimTests.embodied(agent))
                .thenExecute(() -> {
                    PuppetGroup g = RtsimTests.group(agent);
                    for (int i = 0; i < 3; i++)
                        bodies[i] = RtsimTests.member(g, i);
                    BlockPos o = helper.absolutePos(BlockPos.ZERO);
                    // Far beyond every loaded chunk: the move takes the llama out of tracking at once.
                    bodies[1].teleportTo(o.getX() + 30_000.5, o.getY() + 1, o.getZ() + 0.5);
                })
                .thenExecuteAfter(1, () -> {
                    check("the group was captured", RtsimTests.group(agent) == null);
                    for (Entity b : bodies)
                        check(b + " was removed", b.isRemoved());
                })
                .thenWaitUntil(() -> check("the stub received the capture",
                        !RtsimTests.recordsFor(RtsimTests.stub(), Obs.DEMATERIALIZED, agent).isEmpty()))
                .thenExecute(() -> {
                    StubEngine.Received demat = RtsimTests.recordsFor(RtsimTests.stub(), Obs.DEMATERIALIZED, agent).get(0);
                    check("captured as unloaded", demat.aux() == Obs.Dematerialized.UNLOADED);
                    check("all three members captured", demat.view().getInt(Obs.Dematerialized.MEMBERS) == 3);
                    check("three member states", RtsimTests.recordsFor(RtsimTests.stub(), Obs.MEMBER_STATE, agent).size() == 3);
                })
                .thenSucceed();
    }

    /**
     * The spawn quota counts bodies, and a group is never split across ticks (PLAN.md §4.1): three
     * caravans of three bodies scripted into one step, with a quota of 8 bodies a tick, materialize as
     * two in one tick and the third, whole, in the next.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 300, batch = "mic_rtsim_quota")
    public static void spawnQuotaNeverSplitsAGroup(GameTestHelper helper) {
        check("this test is written for the PLACEHOLDER quota of 8", RtsimConfig.spawnQuota() == 8);
        long[] agents = {RtsimTests.newAgent(), RtsimTests.newAgent(), RtsimTests.newAgent()};
        // One scripted unit, so all three land in the same step's command buffer; its own batch, so no
        // other test's spawns share the tick's quota.
        RtsimTests.stub().scriptTogether(caravanCommand(helper, agents[0], 1), caravanCommand(helper, agents[1], 1),
                caravanCommand(helper, agents[2], 1));
        helper.startSequence()
                .thenWaitUntil(() -> {
                    for (long a : agents)
                        check("agent " + a + " materialized", !RtsimTests.recordsFor(RtsimTests.stub(), Obs.MATERIALIZED, a).isEmpty());
                })
                .thenExecute(() -> {
                    long[] t = new long[3];
                    for (int i = 0; i < 3; i++) {
                        StubEngine.Received m = RtsimTests.recordsFor(RtsimTests.stub(), Obs.MATERIALIZED, agents[i]).get(0);
                        t[i] = RtsimTests.gameTimeOf(m);
                        check("caravan " + i + " materialized whole", m.aux() == 3);
                    }
                    record("spawnQuotaNeverSplitsAGroup", "materialize ticks", t[0] + ", " + t[1] + ", " + t[2]);
                    check("the first two share a tick", t[0] == t[1]);
                    check("the third waits for the next tick", t[2] == t[0] + 1);
                    for (long a : agents)
                        RtsimTests.stub().despawnGroup(a, 1);
                })
                .thenWaitUntil(() -> {
                    for (long a : agents)
                        check("agent " + a + " despawned", RtsimTests.group(a) == null);
                })
                .thenSucceed();
    }

    /**
     * A puppet appears only where entities tick (PLAN.md §6.1 item 4): a spawn whose members would stand
     * in a chunk that is not entity-ticking is rejected, journaled, and nothing is created.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 200)
    public static void aSpawnWhereEntitiesDoNotTickIsRejected(GameTestHelper helper) {
        long agent = RtsimTests.newAgent();
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        RtsimTests.stub().spawnGroup(agent, 1, BodyKinds.HUMAN_CARAVAN, RtsimTests.dimension(helper),
                o.getX() + 40_000.5, o.getY() + 1, o.getZ() + 0.5,
                List.of(StubEngine.MemberSpec.plain(0), StubEngine.MemberSpec.plain(1), StubEngine.MemberSpec.plain(2)));
        helper.startSequence()
                .thenWaitUntil(() -> check("the stub received the rejection",
                        !RtsimTests.recordsFor(RtsimTests.stub(), Obs.SPAWN_REJECTED, agent).isEmpty()))
                .thenExecute(() -> {
                    StubEngine.Received r = RtsimTests.recordsFor(RtsimTests.stub(), Obs.SPAWN_REJECTED, agent).get(0);
                    check("rejected as not ticking", r.aux() == Obs.SpawnRejected.NOT_TICKING);
                    check("nothing was embodied", RtsimTests.group(agent) == null
                            && RtsimTests.recordsFor(RtsimTests.stub(), Obs.MATERIALIZED, agent).isEmpty());
                })
                .thenSucceed();
    }
}
