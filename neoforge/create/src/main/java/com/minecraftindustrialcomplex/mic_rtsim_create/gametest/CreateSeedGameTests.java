package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryAttachments;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.level.BlockEvent;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.*;

/**
 * DS-10 on Create's own blocks: only machines seed a district. A lone item vault, fluid tank, depot, basin,
 * furnace or run of shafts, cogwheels and a gearbox makes none; a mechanical press makes one; a vault beside
 * it is a member and its inventory boundary stock; a shaft out of it takes in the next chunk; and removing
 * the press dissolves the district. These use {@link IndustryKit#playerPlacedBare}: no marker machine.
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class CreateSeedGameTests {
    private CreateSeedGameTests() {}

    private static Block block(String id) {
        return BuiltInRegistries.BLOCK.get(ResourceLocation.parse(id));
    }

    private static int seedsAt(GameTestHelper helper, BlockPos pos) {
        return helper.getLevel().getChunkAt(pos).getData(IndustryAttachments.SEEDS.get()).positions().length;
    }

    private static BlockPos at(GameTestHelper helper, int cx, int dx, int dz) {
        return new BlockPos(borderX(helper) + cx * 16 + dx, floorY(helper), midZ(helper) + dz);
    }

    /** One storage block per chunk, then a run of shafts, cogwheels and a gearbox: no district however long we wait. */
    @GameTest(template = YARD, batch = "create_seed_1", timeoutTicks = 400)
    public static void loneCreateStorageAndKineticPartsMakeNoDistrict(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        BlockPos vault = at(helper, 0, 2, -8), tank = at(helper, 1, 2, -8), depot = at(helper, 0, 8, -8);
        BlockPos basin = at(helper, 1, 8, -8), furnace = at(helper, 0, 12, -8), chest = at(helper, 1, 12, -8);
        BlockPos[] stores = {vault, tank, depot, basin, furnace, chest};
        BlockState shaft = block("create:shaft").defaultBlockState().setValue(BlockStateProperties.AXIS, Direction.Axis.X);
        BlockPos[] kinetic = {at(helper, 0, 2, 6), at(helper, 0, 3, 6), at(helper, 0, 4, 6), at(helper, 0, 5, 6), at(helper, 0, 6, 6)};
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, vault, block("create:item_vault").defaultBlockState());
                    set(helper, tank, block("create:fluid_tank").defaultBlockState());
                    set(helper, depot, block("create:depot").defaultBlockState());
                    set(helper, basin, block("create:basin").defaultBlockState());
                    set(helper, furnace, Blocks.FURNACE.defaultBlockState());
                    set(helper, chest, Blocks.CHEST.defaultBlockState());
                    set(helper, kinetic[0], shaft);
                    set(helper, kinetic[1], block("create:cogwheel").defaultBlockState().setValue(BlockStateProperties.AXIS, Direction.Axis.X));
                    set(helper, kinetic[2], shaft);
                    set(helper, kinetic[3], block("create:gearbox").defaultBlockState());
                    set(helper, kinetic[4], shaft);
                    for (BlockPos p : stores)
                        playerPlacedBare(helper, p);
                    for (BlockPos p : kinetic)
                        playerPlacedBare(helper, p);
                })
                .thenIdle(10)
                .thenExecute(() -> {
                    ind().districts().rescanNow(helper.getLevel());
                    for (BlockPos p : stores) {
                        String n = helper.getLevel().getBlockState(p).getBlock().toString();
                        check("no seed at " + n, seedsAt(helper, p) == 0);
                        check("no district at " + n, districtAt(helper, p) == null);
                    }
                    for (BlockPos p : kinetic) {
                        String n = helper.getLevel().getBlockState(p).getBlock().toString();
                        check("no seed at " + n, seedsAt(helper, p) == 0);
                        check("no district at " + n, districtAt(helper, p) == null);
                    }
                    record("loneCreateStorageAndKineticPartsMakeNoDistrict", "districts", 0);
                })
                .thenSucceed();
    }

    /**
     * A mechanical press makes a district; a vault beside it is a member and its inventory is boundary stock;
     * a shaft run from the press across the chunk border takes in the next chunk; removing the press
     * dissolves the district (the vault and shafts stay), and the shaft chunk goes with it.
     */
    @GameTest(template = YARD, batch = "create_seed_2", timeoutTicks = 600)
    public static void aCreateMachineSeedsAndItsNeighboursAreMembers(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        ServerLevel level = helper.getLevel();
        BlockPos press = at(helper, 0, 10, 0), vault = at(helper, 0, 12, 4);
        BlockState shaft = block("create:shaft").defaultBlockState().setValue(BlockStateProperties.AXIS, Direction.Axis.X);
        long[] id = {0};
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, vault, block("create:item_vault").defaultBlockState());
                    playerPlacedBare(helper, vault);
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    ind().districts().rescanNow(level);
                    check("a vault alone is no district", districtAt(helper, vault) == null);
                    set(helper, press, block("create:mechanical_press").defaultBlockState());
                    for (int x = press.getX() + 1; x < press.getX() + 12; x++)
                        set(helper, new BlockPos(x, press.getY(), press.getZ() + 1), shaft);
                    // the shaft run is a chain from the press's row; the press's side has to touch it
                    playerPlacedBare(helper, press);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, press);
                    check("the press's chunk is a district", d != null);
                    check("the vault is in it", districtAt(helper, vault) == d);
                    check("the vault's inventory is boundary stock", d.inventoryAt(vault) != null);
                    id[0] = d.id;
                    record("aCreateMachineSeedsAndItsNeighboursAreMembers", "district " + d.id + " chunks", d.chunks.size());
                })
                .thenExecute(() -> {
                    NeoForge.EVENT_BUS.post(new BlockEvent.BreakEvent(level, press, level.getBlockState(press), FakePlayers.editor(level)));
                    level.removeBlock(press, false);
                })
                .thenWaitUntil(() -> {
                    check("the district dissolved", ind().district(id[0]) == null && districtAt(helper, vault) == null);
                    check("the vault is still there", level.getBlockState(vault).is(block("create:item_vault")));
                })
                .thenSucceed();
    }
}
