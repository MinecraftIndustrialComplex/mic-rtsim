package com.minecraftindustrialcomplex.mic_rtsim_terraentity.gametest;

import com.minecraftindustrialcomplex.mic_rtsim_terraentity.MicRtsimTerraEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Test 5: block breaking. Six slices of Create machinery (pumps, pipes, shafts, cogwheels, tanks, presses,
 * fans, casings...) and vanilla blocks of every softness sit across the worm's path, each a 6 by 6 plate.
 * Worms dash through them back and forth for 400 ticks; afterwards every block of the plates and of the
 * stone around them is compared with a snapshot taken before the worm spawned, and dropped items are counted.
 */
@GameTestHolder(MicRtsimTerraEntity.MODID)
@PrefixGameTestTemplate(false)
public final class BlockTests {
    private BlockTests() {}

    private static final List<String> BLOCKS = List.of("create:mechanical_pump", "create:fluid_pipe", "create:shaft", "create:cogwheel", "create:large_cogwheel",
            "create:fluid_tank", "create:mechanical_press", "create:encased_fan", "create:steam_engine", "create:andesite_casing", "create:brass_casing",
            "create:fluid_valve", "create:smart_fluid_pipe", "create:spout", "create:depot", "create:gearbox", "create:mechanical_drill",
            "minecraft:glass", "minecraft:oak_planks", "minecraft:barrel", "minecraft:chest", "minecraft:furnace", "minecraft:iron_block", "minecraft:redstone_lamp");

    @GameTest(template = "arena", timeoutTicks = 2000, batch = "block_giant_worm")
    public static void breaksNothingGiantWorm(GameTestHelper helper) {
        pass(helper, "breaksNothingGiantWorm", "giant_worm");
    }

    @GameTest(template = "arena", timeoutTicks = 2000, batch = "block_bone_serpent")
    public static void breaksNothingBoneSerpent(GameTestHelper helper) {
        pass(helper, "breaksNothingBoneSerpent", "bone_serpent");
    }

    private static void pass(GameTestHelper helper, String test, String id) {
        if (!Kit.run(helper, test, true)) return;
        Arena a = new Arena(helper, -25, 85, 6, 46, -45, 65);
        a.fill(Blocks.STONE.defaultBlockState());
        List<BlockState> palette = new ArrayList<>();
        List<String> missing = new ArrayList<>();
        for (String b : BLOCKS) {
            ResourceLocation rl = ResourceLocation.parse(b);
            if (BuiltInRegistries.BLOCK.containsKey(rl)) palette.add(BuiltInRegistries.BLOCK.get(rl).defaultBlockState());
            else missing.add(b);
        }
        Kit.check("no machinery in the palette", !palette.isEmpty());
        // plates across the path (the worm runs along z = origin + 10, y = origin + 26)
        int n = 0;
        int[] sliceX = new int[6];
        for (int s = 0; s < 6; s++) {
            int x = a.origin.getX() + 18 + 3 * s;
            sliceX[s] = x;
            for (int y = 23; y <= 29; y++)
                for (int z = 7; z <= 13; z++)
                    a.set(new BlockPos(x, a.origin.getY() + y, a.origin.getZ() + z), palette.get(n++ % palette.size()));
        }
        Vec3 target = a.at(60.5, 26, 10.5);
        a.room(target, 2);
        LivingEntity marker = Kit.marker(a.level, target);
        Map<BlockPos, BlockState> before = new HashMap<>();
        BlockPos.betweenClosedStream(a.x0, a.y0, a.z0, a.x1, a.y1, a.z1).forEach(p -> before.put(p.immutable(), a.level.getBlockState(p)));
        long machines = before.values().stream().filter(b -> BuiltInRegistries.BLOCK.getKey(b.getBlock()).getNamespace().equals("create")).count();
        Kit.say(test, "setup", id + "; plates of " + palette.size() + " block kinds (missing " + missing + "), " + machines + " Create machine blocks in " + before.size() + " blocks snapshotted");
        int[] t = {0};
        Mob[] worm = new Mob[1];
        long[] headInPlates = {0};
        long[] partsInPlates = {0};
        int total = 600;
        helper.onEachTick(() -> {
            if (t[0] > total) return;
            if (t[0] == 30) {
                worm[0] = (Mob) Kit.spawn(a.level, id, a.at(10.5, 26, 10.5));
                worm[0].setTarget(marker);
            }
            if (worm[0] != null && worm[0].isAlive()) {
                if (inPlates(worm[0].position(), sliceX, a)) headInPlates[0]++;
                for (var part : worm[0].getParts())
                    if (inPlates(part.position(), sliceX, a)) partsInPlates[0]++;
            }
            if (t[0] == total) {
                int changed = 0, air = 0;
                Map<String, Integer> what = new HashMap<>();
                for (var e : before.entrySet()) {
                    BlockState now = a.level.getBlockState(e.getKey());
                    if (now != e.getValue()) {
                        changed++;
                        if (now.isAir()) air++;
                        what.merge(BuiltInRegistries.BLOCK.getKey(e.getValue().getBlock()) + " -> " + BuiltInRegistries.BLOCK.getKey(now.getBlock()), 1, Integer::sum);
                    }
                }
                int items = a.level.getEntitiesOfClass(ItemEntity.class, new AABB(a.x0, a.y0, a.z0, a.x1 + 1, a.y1 + 1, a.z1 + 1)).size();
                Kit.say(test, "blocks changed after " + (total - 30) + " ticks with the worm in the slab", changed + " of " + before.size() + " (to air " + air + ") " + what);
                Kit.say(test, "dropped items in the slab", items);
                Kit.say(test, "ticks the head / part-ticks the body spent inside the plates' volume", headInPlates[0] + " / " + partsInPlates[0]);
                Kit.say(test, "worm alive", worm[0].isAlive());
                worm[0].discard();
                marker.discard();
                a.close();
                helper.succeed();
            }
            t[0]++;
        });
    }

    private static boolean inPlates(Vec3 p, int[] sliceX, Arena a) {
        for (int x : sliceX)
            if (p.x >= x && p.x < x + 1 && p.y >= a.origin.getY() + 23 && p.y < a.origin.getY() + 30 && p.z >= a.origin.getZ() + 7 && p.z < a.origin.getZ() + 14)
                return true;
        return false;
    }
}
