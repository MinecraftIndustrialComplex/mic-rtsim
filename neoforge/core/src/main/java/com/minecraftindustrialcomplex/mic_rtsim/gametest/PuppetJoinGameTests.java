package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.AgentTag;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.List;
import java.util.UUID;
import java.util.stream.Stream;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * Stale copies are discarded on join (PLAN.md §6.1 item 2). Each copy is rebuilt from the live
 * puppet's own NBT (saveWithoutId keeps the tag, as another mod's save or a structure template would)
 * and then made stale in one way, and each must fail to join, whether it arrives as a fresh entity or
 * from disk.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class PuppetJoinGameTests {
    private PuppetJoinGameTests() {}

    private static Entity copyOf(Entity puppet, ServerLevel level, AgentTag tag, UUID uuid) {
        CompoundTag nbt = new CompoundTag();
        puppet.saveWithoutId(nbt);
        nbt.putString("id", EntityType.getKey(puppet.getType()).toString());
        Entity copy = EntityType.loadEntityRecursive(nbt, level, e -> e);
        check("the copy is rebuilt from the puppet's NBT", copy != null);
        check("the copy carries the puppet's tag", RtsimAttachments.isTagged(copy));
        copy.setData(RtsimAttachments.AGENT, tag);
        copy.setUUID(uuid);
        return copy;
    }

    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 300)
    public static void staleEpochCopyIsDiscardedOnJoin(GameTestHelper helper) {
        long agent = RtsimTests.newAgent();
        Vec3 at = RtsimTests.centre(helper);
        RtsimTests.stub().spawnGroup(agent, 2, "minecraft:wandering_trader", RtsimTests.dimension(helper), at.x, at.y, at.z,
                List.of(StubEngine.MemberSpec.plain(0)));
        long[] staleBefore = new long[1];

        helper.startSequence()
                .thenWaitUntil(() -> RtsimTests.embodied(agent))
                .thenExecute(() -> {
                    ServerLevel level = helper.getLevel();
                    Entity puppet = RtsimTests.member(RtsimTests.group(agent), 0);
                    staleBefore[0] = RtsimTests.session().staleDiscarded();

                    Entity oldEpoch = copyOf(puppet, level, new AgentTag(agent, 1), UUID.randomUUID());
                    check("a copy from epoch 1 of a puppet now in epoch 2 does not join", !level.addFreshEntity(oldEpoch));
                    check("and is not in the level", level.getEntity(oldEpoch.getUUID()) == null);

                    Entity fromDisk = copyOf(puppet, level, new AgentTag(agent, 1), UUID.randomUUID());
                    level.addLegacyChunkEntities(Stream.of(fromDisk)); // the chunk-load path: loadedFromDisk = true
                    check("a stale copy loaded from disk does not join", level.getEntity(fromDisk.getUUID()) == null);

                    Entity sameEpochOtherUuid = copyOf(puppet, level, new AgentTag(agent, 2), UUID.randomUUID());
                    check("a copy of the live epoch under another UUID (a template copy) does not join",
                            !level.addFreshEntity(sameEpochOtherUuid));

                    Entity unknownAgent = copyOf(puppet, level, new AgentTag(agent + 1_000_000, 1), UUID.randomUUID());
                    check("a copy of an agent that is not embodied does not join", !level.addFreshEntity(unknownAgent));

                    long discarded = RtsimTests.session().staleDiscarded() - staleBefore[0];
                    record("staleEpochCopyIsDiscardedOnJoin", "stale copies discarded", discarded);
                    check("four stale copies were counted, saw " + discarded, discarded == 4);
                    check("the live puppet is untouched", !puppet.isRemoved() && level.getEntity(puppet.getUUID()) == puppet);
                    check("and is still the live member", RtsimTests.group(agent) != null && RtsimTests.group(agent).epoch == 2);
                    RtsimTests.stub().despawnGroup(agent, 2);
                })
                .thenWaitUntil(() -> check("the puppet was despawned", RtsimTests.group(agent) == null))
                .thenSucceed();
    }
}
