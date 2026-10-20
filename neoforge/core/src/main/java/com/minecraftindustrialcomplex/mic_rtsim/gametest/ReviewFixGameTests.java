package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.animal.horse.TraderLlama;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.living.LivingDeathEvent;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.Set;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;

/** The cases an adversarial review of the bridge found, each pinned by a test. */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class ReviewFixGameTests {
    private ReviewFixGameTests() {}

    /** Entities whose deaths a test cancels, as a totem-like mod would (and heals them). */
    private static final Set<UUID> SPARED = ConcurrentHashMap.newKeySet();
    private static boolean listening;

    private static void listen() {
        if (listening)
            return;
        listening = true;
        // Registered after the bridge's own LOWEST listener, so it runs after it: the case where the
        // bridge has already seen the death when someone else cancels it.
        NeoForge.EVENT_BUS.addListener(EventPriority.LOWEST, false, LivingDeathEvent.class, e -> {
            if (SPARED.remove(e.getEntity().getUUID())) {
                e.setCanceled(true);
                e.getEntity().setHealth(5f);
            }
        });
    }

    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 200)
    public static void despawnOfAnAgentThatIsNotEmbodiedIsAnswered(GameTestHelper helper) {
        long agent = RtsimTests.newAgent();
        RtsimTests.engine().despawnGroup(agent, 3);
        helper.startSequence()
                .thenWaitUntil(() -> check("the engine received DEMATERIALIZED",
                        !RtsimTests.recordsFor(RtsimTests.engine(), Obs.DEMATERIALIZED, agent).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received r = RtsimTests.recordsFor(RtsimTests.engine(), Obs.DEMATERIALIZED, agent).get(0);
                    check("as not embodied", r.aux() == Obs.Dematerialized.NOT_EMBODIED);
                    check("with its epoch and no members", r.view().getInt(Obs.Dematerialized.EPOCH) == 3
                            && r.view().getInt(Obs.Dematerialized.MEMBERS) == 0);
                })
                .thenSucceed();
    }

    /**
     * A capture mod serializes the creature while it is still a puppet (saveWithoutId keeps the tag),
     * removes it, and later releases the copy. The copy comes back as an ordinary entity, once.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 300)
    public static void aCapturedCopyComesBackOnceAsAnOrdinaryEntity(GameTestHelper helper) {
        long agent = RtsimTests.newAgent();
        CaravanGameTests.spawnCaravan(helper, agent, 1);
        CompoundTag[] saved = new CompoundTag[1];
        helper.startSequence()
                .thenWaitUntil(() -> RtsimTests.embodied(agent))
                .thenExecute(() -> {
                    Entity llama = RtsimTests.member(RtsimTests.group(agent), 2);
                    saved[0] = new CompoundTag();
                    llama.saveWithoutId(saved[0]);
                    saved[0].putString("id", "minecraft:trader_llama");
                    llama.discard(); // the capture
                })
                .thenExecuteAfter(1, () -> {
                    ServerLevel level = helper.getLevel();
                    Entity back = EntityType.loadEntityRecursive(saved[0], level, e -> e);
                    check("the captured NBT still carries the tag", back != null && RtsimAttachments.isTagged(back));
                    back.setUUID(UUID.randomUUID()); // released under a new UUID, as many capture mods do
                    check("the released creature joins", level.addFreshEntity(back));
                    check("as an ordinary entity", !RtsimApi.isPuppet(back) && back.shouldBeSaved());
                    Entity again = EntityType.loadEntityRecursive(saved[0], level, e -> e);
                    again.setUUID(UUID.randomUUID());
                    check("a second copy of the same capture is stale", !level.addFreshEntity(again));
                    back.discard();
                    RtsimTests.engine().despawnGroup(agent, 1);
                })
                .thenWaitUntil(() -> check("the rest despawned", RtsimTests.group(agent) == null))
                .thenSucceed();
    }

    /** A death the bridge saw and another mod then cancelled (healing the creature) changes nothing. */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 300)
    public static void aCancelledDeathKeepsTheMember(GameTestHelper helper) {
        listen();
        long agent = RtsimTests.newAgent();
        CaravanGameTests.spawnCaravan(helper, agent, 1);
        Entity[] llama = new Entity[1];
        helper.startSequence()
                .thenWaitUntil(() -> RtsimTests.embodied(agent))
                .thenExecute(() -> {
                    llama[0] = RtsimTests.member(RtsimTests.group(agent), 1);
                    SPARED.add(llama[0].getUUID());
                    llama[0].kill();
                })
                .thenExecuteAfter(2, () -> {
                    PuppetGroup g = RtsimTests.group(agent);
                    check("the llama is alive", llama[0].isAlive() && !llama[0].isRemoved());
                    check("and still a member", g != null && !g.members[1].gone && g.liveMembers() == 3);
                    check("no DIED was journaled", RtsimTests.recordsFor(RtsimTests.engine(), Obs.DIED, agent).isEmpty()
                            && RtsimTests.session().registry().byUuid(llama[0].getUUID()) == g);
                    RtsimTests.engine().despawnGroup(agent, 1);
                })
                .thenWaitUntil(() -> check("despawned", RtsimTests.group(agent) == null))
                .thenSucceed();
    }

    /** A llama a player tamed stays: its despawn timer is out of reach once it is not the sim's. */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 300)
    public static void aTakenLlamaDoesNotDespawn(GameTestHelper helper) {
        long agent = RtsimTests.newAgent();
        CaravanGameTests.spawnCaravan(helper, agent, 1);
        TraderLlama[] llama = new TraderLlama[1];
        helper.startSequence()
                .thenWaitUntil(() -> RtsimTests.embodied(agent))
                .thenExecute(() -> {
                    llama[0] = (TraderLlama) RtsimTests.member(RtsimTests.group(agent), 1);
                    check("taken through the plugin API", RtsimApi.takeFromSim(llama[0],
                            com.minecraftindustrialcomplex.mic_rtsim.api.TakeReason.PICKUP, null));
                    check("let go of the trader", llama[0].getLeashHolder() == null);
                    CompoundTag tag = new CompoundTag();
                    llama[0].addAdditionalSaveData(tag);
                    check("its despawn delay is out of reach (" + tag.getInt("DespawnDelay") + ")",
                            tag.getInt("DespawnDelay") > 1_000_000_000);
                    RtsimTests.engine().despawnGroup(agent, 1);
                })
                .thenWaitUntil(() -> check("the rest despawned", RtsimTests.group(agent) == null))
                .thenExecute(() -> {
                    check("the taken llama stays", !llama[0].isRemoved());
                    llama[0].discard();
                })
                .thenSucceed();
    }
}
