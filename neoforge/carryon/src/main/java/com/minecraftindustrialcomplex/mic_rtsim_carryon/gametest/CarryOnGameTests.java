package com.minecraftindustrialcomplex.mic_rtsim_carryon.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.CaravanGameTests;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import com.minecraftindustrialcomplex.mic_rtsim_carryon.MicRtsimCarryOn;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.animal.horse.TraderLlama;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;
import tschipp.carryon.events.EntityPickupEvent;

import java.util.List;
import java.util.UUID;

/**
 * A Carry On pickup takes the creature out of the sim (A10, PLAN.md §6.2), with Carry On 2.2.6.13
 * itself on the GameTest server.
 */
@GameTestHolder(MicRtsimCarryOn.MODID)
@PrefixGameTestTemplate(false)
public final class CarryOnGameTests {
    private CarryOnGameTests() {}

    /**
     * A Carry On pickup of a caravan llama: Carry On's own event ({@code tschipp.carryon.events.EntityPickupEvent},
     * Carry On's class, from its jar on this server), posted on {@code NeoForge.EVENT_BUS} as Carry On's
     * {@code CommonEvents.onEntityRightClick} posts it before a pickup. The llama leaves the sim (TAKEN,
     * reason PICKUP, with its cargo and a Java op id); {@code Entity.save}, which Carry On calls next and
     * which refused the puppet, now works; the rest of the caravan stays one group.
     *
     * <p>Carry On's {@code PickupHandler} itself is not run: after the event it sends its carry data to
     * the player as a custom payload, which a GameTest player's connection has not negotiated (the payload
     * check throws). The event is the hook (PLAN.md §6.2, choice G18).
     */
    @GameTest(template = "arena", timeoutTicks = 300)
    public static void aCarryOnPickupTakesTheCreatureOutOfTheSim(GameTestHelper helper) {
        long agent = RtsimTests.newAgent();
        CaravanGameTests.spawnCaravan(helper, agent, 1);
        ScriptedEngine stub = RtsimTests.engine();
        ServerPlayer[] player = new ServerPlayer[1];
        TraderLlama[] llama = new TraderLlama[1];

        helper.startSequence()
                .thenWaitUntil(() -> RtsimTests.embodied(agent))
                .thenExecute(() -> {
                    PuppetGroup g = RtsimTests.group(agent);
                    llama[0] = (TraderLlama) RtsimTests.member(g, 1);
                    player[0] = FakePlayers.add(helper.getLevel(), llama[0].position().add(1, 0, 0), "carryon-test");
                    RtsimTests.check("a puppet: Entity.save refuses it", RtsimApi.isPuppet(llama[0]) && !llama[0].save(new CompoundTag()));
                    EntityPickupEvent event = NeoForge.EVENT_BUS.post(new EntityPickupEvent(player[0], llama[0]));
                    RtsimTests.check("nobody cancelled the pickup", !event.isCanceled());
                    RtsimTests.check("it is no longer a puppet", !RtsimApi.isPuppet(llama[0]));
                    CompoundTag saved = new CompoundTag();
                    RtsimTests.check("Carry On's save of it works now", llama[0].save(saved) && saved.contains("id"));
                    RtsimTests.check("the rest of the caravan stays one group", RtsimTests.group(agent) != null
                            && RtsimTests.group(agent).liveMembers() == 2);
                })
                .thenWaitUntil(() -> RtsimTests.check("TAKEN reached the engine", !RtsimTests.recordsFor(stub, Obs.TAKEN, agent).isEmpty()))
                .thenExecute(() -> {
                    StubEngine.Received t = RtsimTests.recordsFor(stub, Obs.TAKEN, agent).get(0);
                    RtsimTests.check("taken by a pickup", t.aux() == Obs.Taken.PICKUP);
                    RtsimTests.check("member 1", t.view().getInt(Obs.Taken.MEMBER) == 1);
                    UUID by = new UUID(t.view().getLong(Obs.Taken.PLAYER_MOST), t.view().getLong(Obs.Taken.PLAYER_LEAST));
                    RtsimTests.check("by the player", by.equals(player[0].getUUID()));
                    RtsimTests.check("with a Java op id for its cargo", (t.view().getLong(Obs.Taken.OP_ID) & Obs.JAVA_OP_BIT) != 0);
                    List<StubEngine.Received> all = RtsimTests.allFor(stub, agent);
                    int i = all.indexOf(t);
                    RtsimTests.check("its cargo came right before it", i > 0 && all.get(i - 1).kind() == Obs.MEMBER_STATE
                            && all.get(i - 1).view().getInt(Obs.MemberState.ITEM_COUNT) == 2);
                    RtsimTests.record("carryon", "pickup", "TAKEN(PICKUP) member 1 by " + by);
                    FakePlayers.remove(player[0]);
                    if (!llama[0].isRemoved())
                        llama[0].discard();
                    stub.despawnGroup(agent, 1);
                })
                .thenWaitUntil(() -> RtsimTests.check("the rest despawned", RtsimTests.group(agent) == null))
                .thenSucceed();
    }
}
