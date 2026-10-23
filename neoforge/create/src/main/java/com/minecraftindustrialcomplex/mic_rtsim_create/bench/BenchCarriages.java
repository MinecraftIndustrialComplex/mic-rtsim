package com.minecraftindustrialcomplex.mic_rtsim_create.bench;

import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.mojang.brigadier.exceptions.CommandSyntaxException;
import com.simibubi.create.Create;
import com.simibubi.create.content.trains.entity.Carriage;
import com.simibubi.create.content.trains.entity.CarriageContraption;
import com.simibubi.create.content.trains.entity.Train;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Map;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.Registries;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplate;
import net.minecraft.world.phys.AABB;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.server.ServerStartedEvent;

/**
 * The far-view bench's carriage designs (docs/choices-dh-feed.md DHT66): {@link FarTrainBench}'s synthetic
 * carriages have no blocks, so the far feed would send them as boxes. This gives each one a design the way a
 * player's carriage is stored: a Create {@link CarriageContraption} of Create and vanilla blocks, handed to the
 * carriage with Create's own {@link Carriage#setContraption}, which keeps it in {@code Carriage.serialisedEntity}
 * as an assembled train has it while no entity exists. Nothing is placed in the world.
 *
 * <p>The first carriage of a train is a steam locomotive, the rest wooden coaches, copper tank wagons and glass
 * observation cars in turn. Only with the bench; {@code -Dmic_rtsim.farbench.dress=false} leaves them bare (the
 * boxes, for an A/B). Local frame (Create's): assembly direction west, the leading bogey block at (0, 0, 0), the
 * trailing one at (-6, 0, 0) (the bench's bogey spacing), the body from x -7 to 1 and z -1 to 1, floor at y 1.
 */
public final class BenchCarriages {
    static final boolean DRESS = !"false".equals(System.getProperty("mic_rtsim.farbench.dress"));
    public static final String[] NAMES = {"loco", "wood", "copper", "glass"};
    static final int SPACING = 6;

    private BenchCarriages() {}

    public static void register() {
        if (DRESS)
            NeoForge.EVENT_BUS.addListener(EventPriority.LOW, false, ServerStartedEvent.class, BenchCarriages::started);
    }

    /** After the bench laid its trains (at the normal priority). */
    private static void started(ServerStartedEvent event) {
        MicRtsimCreate.LOGGER.info("far bench: {}", dress(event.getServer().overworld()));
    }

    /** Gives every bench carriage (spacing 6, no entity) a design: the locomotive first, then the others in turn. */
    static String dress(ServerLevel level) {
        int trains = 0, carriages = 0, failed = 0;
        for (Train train : new ArrayList<>(Create.RAILWAYS.trains.values())) {
            boolean any = false;
            for (int i = 0; i < train.carriages.size(); i++) {
                Carriage c = train.carriages.get(i);
                if (c.bogeySpacing != SPACING || c.anyAvailableEntity() != null)
                    continue;
                int design = i == 0 ? 0 : 1 + (i - 1) % 3;
                try {
                    c.setContraption(level, build(level, design));
                    carriages++;
                    any = true;
                } catch (RuntimeException | CommandSyntaxException e) {
                    if (failed++ < 3)
                        MicRtsimCreate.LOGGER.error("far bench: could not dress carriage {} of {}", i, train.id, e);
                }
            }
            if (any)
                trains++;
        }
        return String.format("dressed %d carriages on %d trains (%d failed)", carriages, trains, failed);
    }

    static CarriageContraption build(ServerLevel level, int design) throws CommandSyntaxException {
        Map<BlockPos, String> m = new HashMap<>();
        // The bogeys every design shares
        m.put(new BlockPos(0, 0, 0), "create:small_bogey[axis=x]");
        m.put(new BlockPos(-SPACING, 0, 0), "create:small_bogey[axis=x]");
        switch (design) {
            case 0 -> loco(m);
            case 1 -> wood(m);
            case 2 -> copper(m);
            default -> glass(m);
        }
        CarriageContraption cc = new CarriageContraption(Direction.WEST);
        var blocks = level.holderLookup(Registries.BLOCK);
        Map<BlockPos, StructureTemplate.StructureBlockInfo> into = cc.getBlocks();
        int minX = 0, minY = 0, minZ = 0, maxX = 0, maxY = 0, maxZ = 0;
        for (Map.Entry<BlockPos, String> e : m.entrySet()) {
            BlockState s = BlockStateParser.parseForBlock(blocks, e.getValue(), false).blockState();
            BlockPos p = e.getKey();
            into.put(p, new StructureTemplate.StructureBlockInfo(p, s, null));
            minX = Math.min(minX, p.getX());
            minY = Math.min(minY, p.getY());
            minZ = Math.min(minZ, p.getZ());
            maxX = Math.max(maxX, p.getX());
            maxY = Math.max(maxY, p.getY());
            maxZ = Math.max(maxZ, p.getZ());
        }
        cc.anchor = BlockPos.ZERO;
        cc.bounds = new AABB(minX, minY, minZ, maxX + 1, maxY + 1, maxZ + 1);
        return cc;
    }

    private static void box(Map<BlockPos, String> m, int x0, int y0, int z0, int x1, int y1, int z1, String s) {
        for (int x = x0; x <= x1; x++)
            for (int y = y0; y <= y1; y++)
                for (int z = z0; z <= z1; z++)
                    m.put(new BlockPos(x, y, z), s);
    }

    /** Walls of a box (its four sides), from y0 to y1. */
    private static void walls(Map<BlockPos, String> m, int x0, int y0, int z0, int x1, int y1, int z1, String s) {
        for (int x = x0; x <= x1; x++)
            for (int y = y0; y <= y1; y++)
                for (int z = z0; z <= z1; z++)
                    if (x == x0 || x == x1 || z == z0 || z == z1)
                        m.put(new BlockPos(x, y, z), s);
    }

    /** A black and red steam locomotive: boiler in front with a tall stack and a brass dome, cab behind. */
    private static void loco(Map<BlockPos, String> m) {
        box(m, -7, 1, -1, 1, 1, 1, "create:railway_casing");
        for (int x = -7; x <= 1; x++) {
            m.put(new BlockPos(x, 0, -1), "minecraft:red_concrete");
            m.put(new BlockPos(x, 0, 1), "minecraft:red_concrete");
        }
        box(m, -3, 2, -1, 1, 3, 1, "minecraft:black_concrete");
        m.put(new BlockPos(1, 2, 0), "minecraft:sea_lantern");
        m.put(new BlockPos(1, 3, 0), "create:brass_block");
        m.put(new BlockPos(0, 4, 0), "minecraft:polished_blackstone_bricks");
        m.put(new BlockPos(0, 5, 0), "minecraft:polished_blackstone_wall[up=true]");
        m.put(new BlockPos(0, 6, 0), "minecraft:polished_blackstone_wall[up=true]");
        m.put(new BlockPos(-2, 4, 0), "create:brass_block");
        walls(m, -7, 2, -1, -4, 4, 1, "minecraft:red_concrete");
        m.put(new BlockPos(-5, 3, -1), "minecraft:glass");
        m.put(new BlockPos(-5, 3, 1), "minecraft:glass");
        m.put(new BlockPos(-6, 3, -1), "minecraft:glass");
        m.put(new BlockPos(-6, 3, 1), "minecraft:glass");
        box(m, -7, 5, -1, -4, 5, 1, "minecraft:black_concrete");
    }

    /** A brown wooden coach: spruce floor, oak walls with windows, a dark oak roof. */
    private static void wood(Map<BlockPos, String> m) {
        box(m, -7, 1, -1, 1, 1, 1, "minecraft:spruce_planks");
        walls(m, -7, 2, -1, 1, 3, 1, "minecraft:oak_planks");
        for (int x = -6; x <= 0; x += 2) {
            m.put(new BlockPos(x, 3, -1), "minecraft:glass_pane[east=true,west=true]");
            m.put(new BlockPos(x, 3, 1), "minecraft:glass_pane[east=true,west=true]");
        }
        box(m, -7, 4, -1, 1, 4, 1, "minecraft:dark_oak_planks");
        for (int x = -7; x <= 1; x += 4) {
            m.put(new BlockPos(x, 0, -1), "minecraft:spruce_log[axis=y]");
            m.put(new BlockPos(x, 0, 1), "minecraft:spruce_log[axis=y]");
        }
    }

    /** An orange copper tank wagon: copper casing floor, a copper tank, cut copper bands. */
    private static void copper(Map<BlockPos, String> m) {
        box(m, -7, 1, -1, 1, 1, 1, "create:copper_casing");
        box(m, -7, 2, -1, 1, 3, 1, "minecraft:waxed_copper_block");
        for (int x = -6; x <= 0; x += 3)
            for (int y = 2; y <= 3; y++) {
                m.put(new BlockPos(x, y, -1), "minecraft:waxed_cut_copper");
                m.put(new BlockPos(x, y, 1), "minecraft:waxed_cut_copper");
            }
        box(m, -6, 4, 0, 0, 4, 0, "minecraft:waxed_cut_copper");
        m.put(new BlockPos(-3, 5, 0), "minecraft:lightning_rod");
    }

    /** A white and light blue observation car: quartz frame, clear glass walls, a stained glass roof. */
    private static void glass(Map<BlockPos, String> m) {
        box(m, -7, 1, -1, 1, 1, 1, "minecraft:quartz_block");
        walls(m, -7, 2, -1, 1, 3, 1, "minecraft:glass");
        for (int x : new int[]{-7, -3, 1})
            for (int y = 2; y <= 3; y++) {
                m.put(new BlockPos(x, y, -1), "minecraft:quartz_pillar[axis=y]");
                m.put(new BlockPos(x, y, 1), "minecraft:quartz_pillar[axis=y]");
            }
        box(m, -7, 4, -1, 1, 4, 1, "minecraft:white_concrete");
        box(m, -6, 4, 0, 0, 4, 0, "minecraft:light_blue_stained_glass");
    }
}
