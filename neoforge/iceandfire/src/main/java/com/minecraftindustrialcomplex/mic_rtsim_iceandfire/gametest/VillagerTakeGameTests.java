package com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest;

import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonBridge;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.MicRtsimIceAndFire;
import java.util.List;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.npc.Villager;
import net.minecraft.world.phys.AABB;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

/**
 * A dragon took a villager of an unwatched village (TAKE_RESIDENT; docs/overnight-choices-equilibrium.md EQC556,
 * Ben's PP-2 (a)): one adult, unnamed villager is removed, now where it is loaded, else the first that joins there;
 * a child and a named villager are never taken.
 */
@GameTestHolder(MicRtsimIceAndFire.MODID)
@PrefixGameTestTemplate(false)
public final class VillagerTakeGameTests {
    private VillagerTakeGameTests() {}

    private static List<Villager> villagers(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        BlockPos c = helper.absolutePos(new BlockPos(4, 2, 4));
        return level.getEntitiesOfClass(Villager.class, new AABB(c).inflate(16));
    }

    /** Three adults, a child and a named adult at a loaded village: one adult goes, the rest stay. */
    @GameTest(template = DragonKit.ARENA, batch = "villager_take")
    public static void aTakenVillagerLeavesItsVillage(GameTestHelper helper) {
        DragonBridge.INSTANCE.villagerTakes.clear();
        for (int i = 0; i < 3; i++)
            helper.spawn(EntityType.VILLAGER, new BlockPos(2 + i, 2, 4));
        Villager child = helper.spawn(EntityType.VILLAGER, new BlockPos(6, 2, 4));
        child.setAge(-24000);
        Villager named = helper.spawn(EntityType.VILLAGER, new BlockPos(4, 2, 6));
        named.setCustomName(Component.literal("Kept"));
        BlockPos c = helper.absolutePos(new BlockPos(4, 2, 4));
        int before = DragonBridge.INSTANCE.villagerTakes.taken;
        DragonBridge.INSTANCE.villagerTakes.take(helper.getLevel(), c.getX(), c.getZ(), 8);
        helper.runAfterDelay(2, () -> {
            List<Villager> left = villagers(helper);
            helper.assertTrue(left.size() == 4, "villagers left: " + left.size());
            helper.assertTrue(left.contains(child) && left.contains(named), "a child or a named villager was taken");
            helper.assertTrue(DragonBridge.INSTANCE.villagerTakes.taken == before + 1, "not counted");
            helper.succeed();
        });
    }

    /** A take where no villager is: held, and the first villager that joins there never does; the next one does. */
    @GameTest(template = DragonKit.ARENA, batch = "villager_take_held")
    public static void aHeldTakeTakesTheFirstToJoin(GameTestHelper helper) {
        DragonBridge.INSTANCE.villagerTakes.clear();
        BlockPos c = helper.absolutePos(new BlockPos(4, 2, 4));
        DragonBridge.INSTANCE.villagerTakes.take(helper.getLevel(), c.getX(), c.getZ(), 8);
        helper.assertTrue(DragonBridge.INSTANCE.villagerTakes.held() == 1, "the take was not held");
        helper.spawn(EntityType.VILLAGER, new BlockPos(3, 2, 4));
        helper.runAfterDelay(2, () -> {
            helper.assertTrue(villagers(helper).isEmpty(), "the first villager joined");
            helper.assertTrue(DragonBridge.INSTANCE.villagerTakes.held() == 0, "the take is still held");
            helper.spawn(EntityType.VILLAGER, new BlockPos(5, 2, 4));
            helper.runAfterDelay(2, () -> {
                helper.assertTrue(villagers(helper).size() == 1, "the second villager did not join");
                helper.succeed();
            });
        });
    }
}
