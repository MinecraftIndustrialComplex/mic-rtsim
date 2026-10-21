package com.minecraftindustrialcomplex.mic_rtsim_sandworms.gametest;

import com.minecraftindustrialcomplex.mic_rtsim_sandworms.MicRtsimSandworms;
import com.minecraftindustrialcomplex.mic_rtsim_sandworms.Probes;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.item.ItemEntity;
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
 * Test 3: what does the worm destroy? The mod's source has one destroyBlock call (the thumper it is sent to) and a sinkHole method
 * that spawns a particle ripple. These tests check it with blocks. "Mixed" fills a 40 by 30 by 10 block volume on the worm's way with
 * a different block every cell (vanilla, Create machinery and Diesel Generators' pumpjack hole when those are loaded, which is
 * -PwithCreate). "Base" builds what a player would: a house of planks, glass and a chest on the sand, a pumpjack hole at the surface
 * over a 30-block column of pipe, and a villager 20 blocks down the other side of the column, so the worm passes through the column
 * and under the house. Then the blocks are compared with a snapshot taken before the worm spawned and dropped items are counted.
 */
@GameTestHolder(MicRtsimSandworms.MODID)
@PrefixGameTestTemplate(false)
public final class BlockTests {
    private BlockTests() {}

    private static final List<String> MIXED = List.of("create:mechanical_pump", "create:fluid_pipe", "create:shaft", "create:cogwheel", "create:large_cogwheel",
            "create:fluid_tank", "create:mechanical_press", "create:encased_fan", "create:steam_engine", "create:andesite_casing", "create:brass_casing",
            "create:fluid_valve", "create:smart_fluid_pipe", "create:spout", "create:depot", "create:gearbox", "create:mechanical_drill",
            "createdieselgenerators:pumpjack_hole", "createdieselgenerators:diesel_engine", "createdieselgenerators:basin_lid",
            "minecraft:glass", "minecraft:oak_planks", "minecraft:barrel", "minecraft:chest", "minecraft:furnace", "minecraft:iron_block",
            "minecraft:redstone_lamp", "minecraft:obsidian", "minecraft:dirt", "minecraft:gravel", "minecraft:oak_log", "minecraft:hay_block");

    private static BlockState block(String id) {
        ResourceLocation rl = ResourceLocation.parse(id);
        return BuiltInRegistries.BLOCK.containsKey(rl) ? BuiltInRegistries.BLOCK.get(rl).defaultBlockState() : null;
    }

    @GameTest(template = "arena", timeoutTicks = 1500, batch = "block_mixed")
    public static void breaksNothingMixed(GameTestHelper helper) {
        String test = "breaksNothingMixed";
        if (!Kit.run(helper, test, true)) return;
        Arena a = DepthTests.slab(helper);
        List<BlockState> palette = new ArrayList<>();
        List<String> missing = new ArrayList<>();
        for (String b : MIXED) {
            BlockState s = block(b);
            if (s != null) palette.add(s);
            else missing.add(b);
        }
        int n = 0;
        int oy = a.origin.getY();
        for (int x = 10; x < 50; x++)
            for (int y = 30; y < 60; y++)
                for (int z = -5; z < 5; z++)
                    a.set(new BlockPos(a.origin.getX() + x, oy + y, a.origin.getZ() + z), palette.get(n++ % palette.size()));
        Vec3 target = a.at(60.5, DepthTests.SURFACE - 20, 0.5);
        a.room(target, 2);
        LivingEntity marker = Kit.victim(a.level, target, 1000);
        go(helper, a, test, marker, new AABB(a.x0, a.y0, a.z0, a.x1 + 1, a.y1 + 1, a.z1 + 1), "volume of " + palette.size() + " block kinds (missing " + missing + ")", () -> {});
    }

    @GameTest(template = "arena", timeoutTicks = 1500, batch = "block_base")
    public static void breaksNothingBase(GameTestHelper helper) {
        String test = "breaksNothingBase";
        if (!Kit.run(helper, test, true)) return;
        Arena a = DepthTests.slab(helper);
        int ox = a.origin.getX(), oy = a.origin.getY(), oz = a.origin.getZ();
        // a house on the sand at x 14..22, z -4..4: planks walls, glass windows, a chest and a furnace inside
        BlockState planks = Blocks.OAK_PLANKS.defaultBlockState(), glass = Blocks.GLASS.defaultBlockState();
        for (int x = 14; x <= 22; x++)
            for (int z = -4; z <= 4; z++)
                for (int y = 0; y < 5; y++) {
                    boolean wall = x == 14 || x == 22 || z == -4 || z == 4 || y == 4;
                    if (!wall) continue;
                    a.set(new BlockPos(ox + x, oy + DepthTests.SURFACE + y, oz + z), (y == 2 && (x + z) % 3 == 0) ? glass : planks);
                }
        a.set(new BlockPos(ox + 18, oy + DepthTests.SURFACE, oz), Blocks.CHEST.defaultBlockState());
        a.set(new BlockPos(ox + 17, oy + DepthTests.SURFACE, oz + 2), Blocks.FURNACE.defaultBlockState());
        // the well: pumpjack hole in the sand surface at x 30 and a column of pipe below it
        BlockState hole = block("createdieselgenerators:pumpjack_hole");
        BlockState pipe = block("tfmg:industrial_pipe") != null ? block("tfmg:industrial_pipe") : block("create:fluid_pipe") != null ? block("create:fluid_pipe") : Blocks.IRON_BARS.defaultBlockState();
        for (int y = DepthTests.SURFACE - 2; y >= DepthTests.SURFACE - 32; y--)
            a.set(new BlockPos(ox + 30, oy + y, oz), pipe);
        a.set(new BlockPos(ox + 30, oy + DepthTests.SURFACE - 1, oz), hole != null ? hole : Blocks.IRON_BLOCK.defaultBlockState());
        Vec3 target = a.at(36.5, DepthTests.SURFACE - 20, 0.5);
        a.room(target, 2);
        LivingEntity marker = Kit.victim(a.level, target, 1000);
        Kit.say(test, "well", (hole != null ? "createdieselgenerators:pumpjack_hole" : "iron block (Diesel Generators not loaded)") + " at x=30 over a 30-block column of "
                + BuiltInRegistries.BLOCK.getKey(pipe.getBlock()));
        go(helper, a, test, marker, new AABB(a.x0, a.y0, a.z0, a.x1 + 1, oy + DepthTests.SURFACE + 8, a.z1 + 1), "house, well and sand", () -> {
            int intact = 0;
            for (int y = DepthTests.SURFACE - 2; y >= DepthTests.SURFACE - 32; y--)
                if (a.level.getBlockState(new BlockPos(ox + 30, oy + y, oz)).is(pipe.getBlock())) intact++;
            Kit.say(test, "well column pipes still there", intact + " of 31, hole " + (hole == null || a.level.getBlockState(new BlockPos(ox + 30, oy + DepthTests.SURFACE - 1, oz)).is(hole.getBlock())));
        });
    }

    private static void go(GameTestHelper helper, Arena a, String test, LivingEntity marker, AABB box, String what, Runnable extra) {
        Map<BlockPos, BlockState> before = new HashMap<>();
        BlockPos.betweenClosedStream(BlockPos.containing(box.minX, box.minY, box.minZ), BlockPos.containing(box.maxX - 1, box.maxY - 1, box.maxZ - 1))
                .forEach(p -> before.put(p.immutable(), a.level.getBlockState(p)));
        long machines = before.values().stream().filter(b -> !BuiltInRegistries.BLOCK.getKey(b.getBlock()).getNamespace().equals("minecraft")).count();
        ServerPlayer bystander = Kit.trackedPlayer(a.level, a.at(0.5, DepthTests.SURFACE, 300.5), "worm-bystander");
        Entity worm = Sw.create(a.level, a.at(0.5, DepthTests.SURFACE - 3, 0.5));
        Sw.aggro(worm, marker);
        Probes.BREAKS.clear();
        Kit.say(test, "setup", what + "; " + before.size() + " blocks snapshotted, " + machines + " of them modded");
        int total = 700;
        int[] t = {0};
        int[] atMarker = {0};
        Sw.add(a.level, worm);
        helper.onEachTick(() -> {
            if (t[0] > total) return;
            Vec3 head = worm.isAlive() ? Sw.head(worm) : null;
            if (head != null && head.distanceTo(marker.position()) < 8) atMarker[0]++;
            if (t[0] == total) {
                int changed = 0, air = 0;
                Map<String, Integer> change = new HashMap<>();
                for (var e : before.entrySet()) {
                    BlockState now = a.level.getBlockState(e.getKey());
                    if (now != e.getValue()) {
                        changed++;
                        if (now.isAir()) air++;
                        change.merge(BuiltInRegistries.BLOCK.getKey(e.getValue().getBlock()) + " -> " + BuiltInRegistries.BLOCK.getKey(now.getBlock()), 1, Integer::sum);
                    }
                }
                int items = a.level.getEntitiesOfClass(ItemEntity.class, new AABB(a.x0, a.y0, a.z0, a.x1 + 1, a.y1 + 40, a.z1 + 1)).size();
                Kit.say(test, "blocks changed after " + total + " ticks with the worm in the slab", changed + " of " + before.size() + " (to air " + air + ") " + change);
                Kit.say(test, "dropped items / BlockEvent.BreakEvents", items + " / " + Probes.BREAKS.size());
                Kit.say(test, "ticks the head spent within 8 blocks of the villager", atMarker[0]);
                Kit.say(test, "villager health (1000 at start)", marker.getHealth());
                Kit.say(test, "worm alive / head now", worm.isAlive() + " / " + (worm.isAlive() && Sw.head(worm) != null ? Kit.v(Sw.head(worm)) : "-"));
                extra.run();
                worm.discard();
                marker.discard();
                Kit.removeTrackedPlayer(bystander);
                a.close();
                helper.succeed();
            }
            t[0]++;
        });
    }

    /** The thumper block is the one thing the mod destroys: head on thumper in a stone room, then the same with the block one step off the head's line. */
    @GameTest(template = "arena", timeoutTicks = 1500, batch = "block_thumper")
    public static void thumperBlockDestroyed(GameTestHelper helper) {
        String test = "thumperBlockDestroyed";
        if (!Kit.run(helper, test, true)) return;
        Arena a = DepthTests.slab(helper);
        Vec3 target = a.at(60.5, DepthTests.SURFACE - 20, 0.5);
        a.room(target, 2);
        BlockPos thumperPos = BlockPos.containing(target);
        BlockPos beside = thumperPos.offset(0, 0, 2);
        a.set(thumperPos, Sw.thumperBlock(false));
        a.set(beside, Sw.thumperBlock(false));
        a.set(thumperPos.above(), Blocks.OAK_PLANKS.defaultBlockState());
        a.set(thumperPos.offset(1, 0, 0), Blocks.IRON_BLOCK.defaultBlockState());
        ServerPlayer bystander = Kit.trackedPlayer(a.level, a.at(0.5, DepthTests.SURFACE, 300.5), "worm-bystander");
        Entity worm = Sw.create(a.level, a.at(0.5, DepthTests.SURFACE - 3, 0.5));
        Sw.thumperTarget(worm, thumperPos.getCenter());
        Sw.add(a.level, worm);
        Probes.BREAKS.clear();
        int[] t = {0};
        helper.onEachTick(() -> {
            if (t[0] > 600) return;
            if (t[0] % 100 == 0)
                Kit.say(test, "t=" + t[0], "target thumper " + Sw.isThumper(a.level, thumperPos) + ", second thumper (2 blocks off the line) " + Sw.isThumper(a.level, beside) + ", planks above target "
                        + a.level.getBlockState(thumperPos.above()).is(Blocks.OAK_PLANKS) + ", iron block beside target " + a.level.getBlockState(thumperPos.offset(1, 0, 0)).is(Blocks.IRON_BLOCK)
                        + ", head " + (Sw.head(worm) == null ? "-" : Kit.v(Sw.head(worm))) + ", thumperTarget " + (Sw.thumperTarget(worm) != null));
            if (t[0] == 600) {
                worm.discard();
                a.level.removeBlockEntity(thumperPos);
                a.level.removeBlockEntity(beside);
                Kit.removeTrackedPlayer(bystander);
                a.close();
                helper.succeed();
            }
            t[0]++;
        });
    }
}
