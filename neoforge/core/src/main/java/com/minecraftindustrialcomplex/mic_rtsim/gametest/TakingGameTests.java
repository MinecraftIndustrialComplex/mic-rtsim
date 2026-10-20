package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.world.entity.animal.horse.TraderLlama;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.GameType;
import net.neoforged.neoforge.event.EventHooks;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.List;
import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * Taking a creature makes it leave the sim (A10, PLAN.md §6.2): it loses its tag, vanilla saves it
 * from then on, the engine gets a TAKEN observation after the member's captured state, and the rest of
 * the group stays one unit (C25).
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class TakingGameTests {
    private TakingGameTests() {}

    /**
     * Taming, through vanilla's own tame line: {@code RunAroundLikeCrazyGoal.tick} runs
     * {@code if (... && !EventHooks.onAnimalTame(horse, player)) horse.tameWithName(player)} behind two
     * random rolls; the test runs the same two calls without the rolls. A trader llama on its trader's
     * leash cannot be ridden ({@code TraderLlama.doPlayerRide}), so the leash snaps first, as it does in
     * play when the llama strays more than 10 blocks.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 300)
    public static void tamingTakesACreatureOutOfTheSim(GameTestHelper helper) {
        long agent = RtsimTests.newAgent();
        CaravanGameTests.spawnCaravan(helper, agent, 1);
        Player player = helper.makeMockPlayer(GameType.SURVIVAL);
        TraderLlama[] llama = new TraderLlama[1];
        UUID[] id = new UUID[1];

        helper.startSequence()
                .thenWaitUntil(() -> RtsimTests.embodied(agent))
                .thenExecute(() -> {
                    PuppetGroup g = RtsimTests.group(agent);
                    llama[0] = (TraderLlama) RtsimTests.member(g, 1);
                    id[0] = llama[0].getUUID();
                    llama[0].dropLeash(true, false);
                    check("a puppet before it is tamed", RtsimApi.isPuppet(llama[0]) && !llama[0].shouldBeSaved());
                    if (!EventHooks.onAnimalTame(llama[0], player))
                        llama[0].tameWithName(player);
                    check("the llama is tamed", llama[0].isTamed());
                    check("it is no longer a puppet", !RtsimApi.isPuppet(llama[0]));
                    check("vanilla saves it now", llama[0].shouldBeSaved() && llama[0].save(new CompoundTag()));
                    check("its UUID is not a puppet's", RtsimTests.session().registry().byUuid(id[0]) == null);
                    check("the rest of the group stays one unit", RtsimTests.group(agent) != null && RtsimTests.group(agent).liveMembers() == 2);
                })
                .thenWaitUntil(() -> check("the engine received TAKEN", !RtsimTests.recordsFor(RtsimTests.engine(), Obs.TAKEN, agent).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received taken = RtsimTests.recordsFor(RtsimTests.engine(), Obs.TAKEN, agent).get(0);
                    check("taken by taming", taken.aux() == Obs.Taken.TAME);
                    check("member 1 was taken", taken.view().getInt(Obs.Taken.MEMBER) == 1);
                    UUID by = new UUID(taken.view().getLong(Obs.Taken.PLAYER_MOST), taken.view().getLong(Obs.Taken.PLAYER_LEAST));
                    check("by the player who tamed it", by.equals(player.getUUID()));
                    check("the cargo's transfer carries a Java op id (top bit set)",
                            (taken.view().getLong(Obs.Taken.OP_ID) & Obs.JAVA_OP_BIT) != 0
                                    && taken.view().getLong(Obs.Taken.OP_ID) != Obs.JAVA_OP_BIT);
                    List<ScriptedEngine.Received> all = RtsimTests.allFor(RtsimTests.engine(), agent);
                    int at = all.indexOf(taken);
                    check("its captured state comes right before it", at > 0 && all.get(at - 1).kind() == Obs.MEMBER_STATE && all.get(at - 1).aux() == 1);
                    check("with the cargo it carried off", all.get(at - 1).view().getInt(Obs.MemberState.ITEM_COUNT) == 2);
                    record("taming", "TAKEN", "member 1 by " + by);
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    check("the group is still embodied, two members", RtsimTests.group(agent) != null && RtsimTests.group(agent).liveMembers() == 2);
                    check("the tamed llama is still in the world, as an ordinary entity", !llama[0].isRemoved());
                    RtsimTests.engine().despawnGroup(agent, 1);
                })
                .thenWaitUntil(() -> check("the rest of the caravan despawned", RtsimTests.group(agent) == null))
                .thenExecute(() -> check("the tamed llama stays: it is not the sim's any more", !llama[0].isRemoved()))
                .thenExecute(() -> llama[0].discard())
                .thenSucceed();
    }

    /**
     * A lead (NeoForge 21.1 has no leash event): the next pass sees a leash holder that is not the
     * group's trader and takes the llama. Same call vanilla's {@code Entity.interact} makes for a
     * player holding a lead.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 300)
    public static void aLeadTakesACreatureOutOfTheSim(GameTestHelper helper) {
        long agent = RtsimTests.newAgent();
        CaravanGameTests.spawnCaravan(helper, agent, 1);
        Player player = helper.makeMockPlayer(GameType.SURVIVAL);
        TraderLlama[] llama = new TraderLlama[1];

        helper.startSequence()
                .thenWaitUntil(() -> RtsimTests.embodied(agent))
                .thenExecute(() -> {
                    llama[0] = (TraderLlama) RtsimTests.member(RtsimTests.group(agent), 2);
                    llama[0].dropLeash(true, false);
                    check("an unleashed llama can take a lead", llama[0].canHaveALeashAttachedToIt());
                    llama[0].setLeashedTo(player, true);
                })
                .thenExecuteAfter(1, () -> {
                    check("the pass took the leashed llama", !RtsimApi.isPuppet(llama[0]));
                    check("the rest stays one unit", RtsimTests.group(agent) != null && RtsimTests.group(agent).liveMembers() == 2);
                })
                .thenWaitUntil(() -> check("the engine received TAKEN", !RtsimTests.recordsFor(RtsimTests.engine(), Obs.TAKEN, agent).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received taken = RtsimTests.recordsFor(RtsimTests.engine(), Obs.TAKEN, agent).get(0);
                    check("taken by a lead", taken.aux() == Obs.Taken.LEAD);
                    check("member 2", taken.view().getInt(Obs.Taken.MEMBER) == 2);
                    llama[0].dropLeash(true, false);
                    RtsimTests.engine().despawnGroup(agent, 1);
                })
                .thenWaitUntil(() -> check("the rest of the caravan despawned", RtsimTests.group(agent) == null))
                .thenExecute(() -> llama[0].discard())
                .thenSucceed();
    }

    /**
     * Deaths are always kept (PLAN.md §4.5): DIED follows the member's state at death, cargo included
     * (the chest's contents drop, a journaled transfer, C21). When the trader dies, its llamas stay one
     * group, and the leash they still hold on the dead trader is not mistaken for a player's lead.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 300)
    public static void deathsAreJournaledAndTheRestStaysOneGroup(GameTestHelper helper) {
        long agent = RtsimTests.newAgent();
        CaravanGameTests.spawnCaravan(helper, agent, 1);
        net.minecraft.world.entity.Entity[] bodies = new net.minecraft.world.entity.Entity[3];

        helper.startSequence()
                .thenWaitUntil(() -> RtsimTests.embodied(agent))
                .thenExecute(() -> {
                    PuppetGroup g = RtsimTests.group(agent);
                    for (int i = 0; i < 3; i++)
                        bodies[i] = RtsimTests.member(g, i);
                    bodies[0].kill();
                })
                .thenExecuteAfter(1, () -> {
                    // The death is journaled at the end of the tick it happened in, once it is certain.
                    PuppetGroup g = RtsimTests.group(agent);
                    check("the trader's death left the group", g != null && g.members[0].gone);
                    check("the llamas are still one group", g.liveMembers() == 2);
                })
                .thenIdle(3)
                .thenExecute(() -> {
                    check("no llama was taken by the dead trader's leash",
                            RtsimTests.recordsFor(RtsimTests.engine(), Obs.TAKEN, agent).isEmpty() && RtsimTests.group(agent).liveMembers() == 2);
                    bodies[1].kill();
                })
                .thenWaitUntil(() -> check("the engine received both deaths", RtsimTests.recordsFor(RtsimTests.engine(), Obs.DIED, agent).size() == 2))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> all = RtsimTests.allFor(RtsimTests.engine(), agent);
                    List<ScriptedEngine.Received> died = RtsimTests.recordsFor(RtsimTests.engine(), Obs.DIED, agent);
                    check("the trader died first", died.get(0).aux() == 0 && died.get(1).aux() == 1);
                    for (ScriptedEngine.Received d : died) {
                        int at = all.indexOf(d);
                        check("a MEMBER_STATE comes right before each DIED", at > 0 && all.get(at - 1).kind() == Obs.MEMBER_STATE
                                && all.get(at - 1).aux() == d.aux());
                        check("each death's dropped cargo has a Java op id", (d.view().getLong(Obs.Died.OP_ID) & Obs.JAVA_OP_BIT) != 0);
                    }
                    int at = all.indexOf(died.get(1));
                    check("llama 1's state at death lists the cargo that dropped", all.get(at - 1).view().getInt(Obs.MemberState.ITEM_COUNT) == 2);
                    record("deaths", "DIED members", died.get(0).aux() + ", " + died.get(1).aux());
                    RtsimTests.engine().despawnGroup(agent, 1);
                })
                .thenWaitUntil(() -> check("the last llama despawned", RtsimTests.group(agent) == null))
                .thenSucceed();
    }

    /**
     * Risk 11 (PLAN.md §15): another mod removes one member (a capture item, a cleanup command). The
     * member leaves the sim as REMOVED_BY_OTHER with its state, and the rest stays one unit.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 300)
    public static void aMemberRemovedByAnotherModLeavesTheRestAsOneGroup(GameTestHelper helper) {
        long agent = RtsimTests.newAgent();
        CaravanGameTests.spawnCaravan(helper, agent, 1);
        helper.startSequence()
                .thenWaitUntil(() -> RtsimTests.embodied(agent))
                .thenExecute(() -> {
                    PuppetGroup g = RtsimTests.group(agent);
                    long before = RtsimTests.session().removedByOther(); // read here: other tests count too
                    RtsimTests.member(g, 2).discard(); // what another mod's removal looks like
                    check("the removal was counted", RtsimTests.session().removedByOther() == before + 1);
                    check("the rest stays one group", RtsimTests.group(agent) == g && g.liveMembers() == 2);
                })
                .thenIdle(3)
                .thenExecute(() -> check("the trader and the other llama are still embodied",
                        RtsimTests.group(agent) != null && RtsimTests.group(agent).liveMembers() == 2))
                .thenWaitUntil(() -> check("the engine received TAKEN", !RtsimTests.recordsFor(RtsimTests.engine(), Obs.TAKEN, agent).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received t = RtsimTests.recordsFor(RtsimTests.engine(), Obs.TAKEN, agent).get(0);
                    check("as removed by another mod", t.aux() == Obs.Taken.REMOVED_BY_OTHER && t.view().getInt(Obs.Taken.MEMBER) == 2);
                    RtsimTests.engine().despawnGroup(agent, 1);
                })
                .thenWaitUntil(() -> check("the rest despawned", RtsimTests.group(agent) == null))
                .thenSucceed();
    }
}
