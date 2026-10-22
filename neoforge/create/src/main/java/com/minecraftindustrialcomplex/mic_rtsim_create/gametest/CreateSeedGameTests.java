package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryAttachments;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorBlockEntity;
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

    /** {@code dx} blocks into chunk column {@code cx} past the first border, {@code dz} into the chunk row of the yard's middle row. */
    private static BlockPos at(GameTestHelper helper, int cx, int dx, int dz) {
        return new BlockPos(borderX(helper) + cx * 16 + dx, floorY(helper), ((midZ(helper) >> 4) << 4) + dz);
    }

    /** One storage block per chunk, then a run of shafts, cogwheels and a gearbox: no district however long we wait. */
    @GameTest(template = YARD, batch = "create_seed_1", timeoutTicks = 400)
    public static void loneCreateStorageAndKineticPartsMakeNoDistrict(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        BlockPos vault = at(helper, 0, 2, 2), tank = at(helper, 1, 2, 2), depot = at(helper, 0, 8, 2);
        BlockPos basin = at(helper, 1, 8, 2), furnace = at(helper, 0, 12, 2), chest = at(helper, 1, 12, 2);
        BlockPos[] stores = {vault, tank, depot, basin, furnace, chest};
        BlockState shaft = block("create:shaft").defaultBlockState().setValue(BlockStateProperties.AXIS, Direction.Axis.X);
        BlockPos[] kinetic = {at(helper, 0, 2, 9), at(helper, 0, 3, 9), at(helper, 0, 4, 9), at(helper, 0, 5, 9), at(helper, 0, 6, 9)};
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
        BlockPos press = at(helper, 0, 10, 6), vault = at(helper, 0, 12, 9);
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
                    // A shaft run along the press's row out past the chunk border: it touches the press and the next chunk.
                    for (int x = press.getX() + 1; x < press.getX() + 12; x++)
                        set(helper, new BlockPos(x, press.getY(), press.getZ()), shaft);
                    playerPlacedBare(helper, press);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, press);
                    check("the press's chunk is a district", d != null);
                    check("the vault is in it", districtAt(helper, vault) == d);
                    check("the vault's inventory is boundary stock", d.inventoryAt(vault) != null);
                    check("the shaft run took in the next chunk: " + d.chunks.size(), d.chunks.size() == 2
                            && d.chunks.contains(chunk(new BlockPos(press.getX() + 11, press.getY(), press.getZ()))));
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

    /** A district's chunks, each with its seeds and block entities (for a failure message). */
    private static String describe(GameTestHelper helper, District d) {
        StringBuilder sb = new StringBuilder();
        for (long c : d.chunks) {
            net.minecraft.world.level.chunk.LevelChunk ch = helper.getLevel().getChunkSource().getChunkNow(net.minecraft.world.level.ChunkPos.getX(c), net.minecraft.world.level.ChunkPos.getZ(c));
            sb.append("[chunk ").append(net.minecraft.world.level.ChunkPos.getX(c)).append(',').append(net.minecraft.world.level.ChunkPos.getZ(c));
            if (ch == null)
                sb.append(" unloaded");
            else {
                sb.append(" seeds ").append(ch.getData(IndustryAttachments.SEEDS.get()).positions().length).append(':');
                ch.getBlockEntities().forEach((p, be) -> sb.append(' ').append(p.toShortString()).append('=').append(be.getBlockState().getBlock()));
            }
            sb.append(']');
        }
        return sb.toString();
    }

    private static void noSeedNoDistrict(GameTestHelper helper, String what, BlockPos... at) {
        for (BlockPos p : at) {
            String n = helper.getLevel().getBlockState(p).getBlock().toString();
            check(what + ": no seed at " + n, seedsAt(helper, p) == 0);
            check(what + ": no district at " + n, districtAt(helper, p) == null);
        }
    }

    /** Rotation made from nothing is a fountain, not a factory: a lone water wheel (small and large) makes no district. */
    @GameTest(template = YARD, batch = "create_seed_3", timeoutTicks = 400)
    public static void aLoneWaterWheelMakesNoDistrict(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        BlockPos small = at(helper, 0, 4, 4), large = at(helper, 0, 10, 10), crank = at(helper, 1, 4, 4), motor = at(helper, 1, 10, 10);
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, small, block("create:water_wheel").defaultBlockState());
                    set(helper, large, block("create:large_water_wheel").defaultBlockState());
                    set(helper, crank, block("create:hand_crank").defaultBlockState());
                    set(helper, motor, block("create:creative_motor").defaultBlockState());
                    for (BlockPos p : new BlockPos[] {small, large, crank, motor})
                        playerPlacedBare(helper, p);
                })
                .thenIdle(10)
                .thenExecute(() -> {
                    ind().districts().rescanNow(helper.getLevel());
                    noSeedNoDistrict(helper, "source", small, large, crank, motor);
                })
                .thenSucceed();
    }

    /** A mechanical pump only moves fluid, like a pipe: tank, pump, tank in a row make no district. */
    @GameTest(template = YARD, batch = "create_seed_4", timeoutTicks = 400)
    public static void aLonePumpBetweenTwoTanksMakesNoDistrict(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        BlockPos a = at(helper, 0, 4, 4), pump = at(helper, 0, 5, 4), b = at(helper, 0, 6, 4);
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, a, block("create:fluid_tank").defaultBlockState());
                    set(helper, pump, block("create:mechanical_pump").defaultBlockState().setValue(BlockStateProperties.FACING, Direction.EAST));
                    set(helper, b, block("create:fluid_tank").defaultBlockState());
                    for (BlockPos p : new BlockPos[] {a, pump, b})
                        playerPlacedBare(helper, p);
                })
                .thenIdle(10)
                .thenExecute(() -> {
                    ind().districts().rescanNow(helper.getLevel());
                    noSeedNoDistrict(helper, "pump", a, pump, b);
                })
                .thenSucceed();
    }

    /**
     * A water wheel in the next chunk, joined to a press by a shaft run across the border: one district, the press
     * its seed, the wheel and the shafts members (the wheel's chunk is in it).
     */
    @GameTest(template = YARD, batch = "create_seed_5", timeoutTicks = 600)
    public static void aWaterWheelDrivingAPressIsOneDistrictWheelIncluded(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        BlockPos press = at(helper, 0, 10, 6), wheel = at(helper, 1, 3, 6);
        BlockState shaft = block("create:shaft").defaultBlockState().setValue(BlockStateProperties.AXIS, Direction.Axis.X);
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, press, block("create:mechanical_press").defaultBlockState());
                    for (int x = press.getX() + 1; x < wheel.getX(); x++)
                        set(helper, new BlockPos(x, press.getY(), press.getZ()), shaft);
                    set(helper, wheel, block("create:water_wheel").defaultBlockState().setValue(BlockStateProperties.FACING, Direction.EAST));
                    playerPlacedBare(helper, wheel);
                    playerPlacedBare(helper, press);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, press);
                    check("the press's district", d != null);
                    check("the wheel is in it", districtAt(helper, wheel) == d);
                    check("two chunks: " + d.chunks.size(), d.chunks.size() == 2);
                    check("one seed, the press's: " + d.seedCounts, d.seeds() == 1 && seedsAt(helper, wheel) == 0);
                })
                .thenSucceed();
    }

    private static void chain(GameTestHelper helper, BlockPos... points) {
        for (BlockPos p : points) {
            set(helper, p, AllBlocks.CHAIN_CONVEYOR.getDefaultState());
            helper.getLevel().getBlockEntity(p);
            ((ChainConveyorBlockEntity) helper.getLevel().getBlockEntity(p)).prepareStats();
        }
        for (int i = 0; i + 1 < points.length; i++) {
            ChainConveyorBlockEntity a = (ChainConveyorBlockEntity) helper.getLevel().getBlockEntity(points[i]);
            ChainConveyorBlockEntity b = (ChainConveyorBlockEntity) helper.getLevel().getBlockEntity(points[i + 1]);
            if (!b.addConnectionTo(points[i]) || !a.addConnectionTo(points[i + 1]))
                throw new net.minecraft.gametest.framework.GameTestAssertException("could not join the chain conveyors");
        }
    }

    /**
     * SEC-1: a chain conveyor is a link between districts, not a reason to join them. A press at each end of a chain,
     * two chunks apart: two districts, the chunk between in neither.
     */
    @GameTest(template = YARD, batch = "create_seed_6", timeoutTicks = 600)
    public static void aChainBetweenTwoDistrictsLeavesThemTwo(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        BlockPos pressA = at(helper, -1, 8, 6), chainA = at(helper, -1, 12, 6);
        BlockPos pressB = at(helper, 1, 8, 6), chainB = at(helper, 1, 4, 6);
        BlockPos between = at(helper, 0, 8, 6);
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, pressA, block("create:mechanical_press").defaultBlockState());
                    set(helper, pressB, block("create:mechanical_press").defaultBlockState());
                    chain(helper, chainA, chainB);
                    for (BlockPos p : new BlockPos[] {pressA, chainA, pressB, chainB})
                        playerPlacedBare(helper, p);
                })
                .thenWaitUntil(() -> {
                    District a = districtAt(helper, pressA), b = districtAt(helper, pressB);
                    check("a district at each press", a != null && b != null);
                    check("two districts, not one", a != b);
                    check("each holds its chain end", districtAt(helper, chainA) == a && districtAt(helper, chainB) == b);
                    check("the chunk between is in neither", districtAt(helper, between) == null);
                    check("one chunk each: " + describe(helper, a) + " | " + describe(helper, b), a.chunks.size() == 1 && b.chunks.size() == 1);
                })
                .thenSucceed();
    }

    /** A chain through chunks with nothing else in them, three conveyors long, makes no district. */
    @GameTest(template = YARD, batch = "create_seed_7", timeoutTicks = 400)
    public static void aChainThroughEmptyChunksMakesNoDistrict(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        BlockPos a = at(helper, -1, 12, 6), b = at(helper, 0, 8, 6), c = at(helper, 1, 4, 6);
        helper.startSequence()
                .thenExecute(() -> {
                    chain(helper, a, b, c);
                    for (BlockPos p : new BlockPos[] {a, b, c})
                        playerPlacedBare(helper, p);
                })
                .thenIdle(10)
                .thenExecute(() -> {
                    ind().districts().rescanNow(helper.getLevel());
                    noSeedNoDistrict(helper, "chain", a, b, c);
                })
                .thenSucceed();
    }

    /** A station on a track along {@code row} (a chunk row's dz), its target track set so the graph takes it in. */
    private static void station(GameTestHelper helper, BlockPos stationPos, int fromX, int toX) {
        BlockState track = block("create:track").defaultBlockState()
                .setValue(com.simibubi.create.content.trains.track.TrackBlock.SHAPE, com.simibubi.create.content.trains.track.TrackShape.XO);
        for (int x = fromX; x <= toX; x++)
            set(helper, new BlockPos(x, stationPos.getY(), stationPos.getZ() - 1), track);
    }

    private static void placeStation(GameTestHelper helper, BlockPos stationPos) {
        set(helper, stationPos, block("create:track_station").defaultBlockState());
        net.minecraft.world.level.block.entity.BlockEntity be = helper.getLevel().getBlockEntity(stationPos);
        com.simibubi.create.content.trains.station.StationBlockEntity sbe = (com.simibubi.create.content.trains.station.StationBlockEntity) be;
        net.minecraft.nbt.CompoundTag tag = sbe.saveWithFullMetadata(helper.getLevel().registryAccess());
        tag.put("TargetTrack", net.minecraft.nbt.NbtUtils.writeBlockPos(new BlockPos(0, 0, -1)));
        tag.putBoolean("TargetDirection", true);
        sbe.loadWithComponents(tag, helper.getLevel().registryAccess());
        playerPlacedBare(helper, stationPos);
    }

    /** A station one chunk from a press: it joins the press's district (it is a member and is listed), and starts none itself. */
    @GameTest(template = YARD, batch = "create_seed_8", timeoutTicks = 600)
    public static void aStationNextToAMachineJoinsItsDistrict(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        BlockPos press = at(helper, 0, 8, 2), stationPos = at(helper, 1, 8, 6);
        helper.startSequence()
                .thenExecute(() -> {
                    station(helper, stationPos, at(helper, 0, 2, 5).getX(), at(helper, 1, 14, 5).getX());
                    set(helper, press, block("create:mechanical_press").defaultBlockState());
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    placeStation(helper, stationPos);
                    playerPlacedBare(helper, press);
                })
                .thenWaitUntil(() -> {
                    var sbe = (com.simibubi.create.content.trains.station.StationBlockEntity) helper.getLevel().getBlockEntity(stationPos);
                    check("the station joined the track graph", sbe != null && sbe.getStation() != null);
                })
                .thenExecute(() -> ind().districts().rescanNow(helper.getLevel()))
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, press);
                    check("the press's district", d != null);
                    check("the station's chunk is in it", districtAt(helper, stationPos) == d);
                    check("two chunks, one district: " + d.chunks.size(), d.chunks.size() == 2);
                    check("the district lists the station", d.stations.size() == 1);
                    check("one seed, the press's: " + d.seeds(), d.seeds() == 1);
                    record("aStationNextToAMachineJoinsItsDistrict", "chunks", d.chunks.size());
                })
                .thenSucceed();
    }

    /** A station with no machine anywhere near it makes no district: it is an anchor, never a seed. */
    @GameTest(template = YARD, batch = "create_seed_9", timeoutTicks = 400)
    public static void aLoneStationMakesNoDistrict(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        BlockPos stationPos = at(helper, 1, 8, 6);
        helper.startSequence()
                .thenExecute(() -> station(helper, stationPos, at(helper, 0, 2, 5).getX(), at(helper, 1, 14, 5).getX()))
                .thenIdle(5)
                .thenExecute(() -> placeStation(helper, stationPos))
                .thenWaitUntil(() -> {
                    var sbe = (com.simibubi.create.content.trains.station.StationBlockEntity) helper.getLevel().getBlockEntity(stationPos);
                    check("the station joined the track graph", sbe != null && sbe.getStation() != null);
                })
                .thenExecute(() -> ind().districts().rescanNow(helper.getLevel()))
                .thenIdle(10)
                .thenExecute(() -> {
                    ind().districts().rescanNow(helper.getLevel());
                    noSeedNoDistrict(helper, "station", stationPos);
                    record("aLoneStationMakesNoDistrict", "districts", 0);
                })
                .thenSucceed();
    }

    /**
     * A bearing facing up with {@code top} in front of it (and, when {@code side} is given, a block beside that: it is not
     * carried without glue). Returns the bearing's block entity.
     */
    private static com.simibubi.create.content.contraptions.bearing.MechanicalBearingBlockEntity bearingRig(GameTestHelper helper,
            BlockPos bearing, BlockState top, BlockState side) {
        set(helper, bearing, block("create:mechanical_bearing").defaultBlockState().setValue(BlockStateProperties.FACING, Direction.UP));
        set(helper, bearing.above(), top);
        if (side != null)
            set(helper, bearing.above().east(), side);
        // an unpowered bearing takes its contraption apart again on its next tick, so a creative motor drives it
        set(helper, bearing.below(), block("create:creative_motor").defaultBlockState().setValue(BlockStateProperties.FACING, Direction.UP));
        playerPlacedBare(helper, bearing);
        return (com.simibubi.create.content.contraptions.bearing.MechanicalBearingBlockEntity) helper.getLevel().getBlockEntity(bearing);
    }

    /**
     * A tree farm on a bearing: a saw in front of it (a contraption moves what is in front of the bearing; only glue or a chassis carries more). While it is assembled the saw is a contraption actor, not
     * a block entity, and the bearing is all that is left in the world: it seeds a district. Taken apart again, the saw is
     * back in the world, no player placed it, and the district goes.
     */
    @GameTest(template = YARD, batch = "create_seed_10", timeoutTicks = 600)
    public static void aBearingFarmWithSawsMakesADistrict(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        BlockPos bearing = at(helper, 0, 6, 6);
        var be = new com.simibubi.create.content.contraptions.bearing.MechanicalBearingBlockEntity[1];
        helper.startSequence()
                .thenExecute(() -> {
                    be[0] = bearingRig(helper, bearing, block("create:mechanical_saw").defaultBlockState(), null);
                    ind().districts().rescanNow(helper.getLevel());
                    check("a bearing that has not assembled anything makes no district", districtAt(helper, bearing) == null);
                })
                .thenWaitUntil(() -> check("the motor turns the bearing", be[0].getSpeed() != 0))
                .thenExecute(() -> {
                    be[0].assemble();
                    check("the bearing runs", be[0].isRunning() && be[0].getMovedContraption() != null);
                    check("the saw is an actor and no longer in the world", !be[0].getMovedContraption().getContraption().getActors().isEmpty()
                            && !helper.getLevel().getBlockState(bearing.above()).is(block("create:mechanical_saw")));
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, bearing);
                    check("the bearing's district (seeds " + seedsAt(helper, bearing) + ", seed() "
                            + ind().districts().seed(helper.getLevel(), be[0], new java.util.HashMap<>()) + ", running " + be[0].isRunning()
                            + ", moved " + be[0].getMovedContraption() + ", removed " + be[0].isRemoved() + ")", d != null);
                    check("one seed, the bearing's: " + d.seeds(), d.seeds() == 1);
                })
                .thenExecute(() -> be[0].disassemble())
                .thenWaitUntil(() -> {
                    ind().districts().rescanNow(helper.getLevel());
                    check("taken apart, the district dissolved", districtAt(helper, bearing) == null);
                    check("the saw is back in the world", helper.getLevel().getBlockState(bearing.above()).is(block("create:mechanical_saw")));
                })
                .thenSucceed();
    }

    /** A door on a bearing carries no actors, and neither does a seat (a passive actor): no district. */
    @GameTest(template = YARD, batch = "create_seed_11", timeoutTicks = 600)
    public static void aBearingDoorMakesNoDistrict(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        BlockPos door = at(helper, 0, 4, 4), seat = at(helper, 1, 4, 4);
        var doorBe = new com.simibubi.create.content.contraptions.bearing.MechanicalBearingBlockEntity[1];
        var seatBe = new com.simibubi.create.content.contraptions.bearing.MechanicalBearingBlockEntity[1];
        helper.startSequence()
                .thenExecute(() -> {
                    doorBe[0] = bearingRig(helper, door, Blocks.OAK_PLANKS.defaultBlockState(), null);
                    seatBe[0] = bearingRig(helper, seat, block("create:white_seat").defaultBlockState(), null);
                })
                .thenWaitUntil(() -> check("the motors turn both bearings", doorBe[0].getSpeed() != 0 && seatBe[0].getSpeed() != 0))
                .thenExecute(() -> {
                    doorBe[0].assemble();
                    seatBe[0].assemble();
                    check("both run", doorBe[0].isRunning() && seatBe[0].isRunning());
                    check("the seat is an actor, the door has none", doorBe[0].getMovedContraption().getContraption().getActors().isEmpty()
                            && !seatBe[0].getMovedContraption().getContraption().getActors().isEmpty());
                })
                .thenIdle(10)
                .thenExecute(() -> {
                    check("both still run", doorBe[0].isRunning() && seatBe[0].isRunning());
                    ind().districts().rescanNow(helper.getLevel());
                    check("no district at the door bearing", districtAt(helper, door) == null);
                    check("no district at the seat bearing", districtAt(helper, seat) == null);
                    record("aBearingDoorMakesNoDistrict", "districts", 0);
                })
                .thenSucceed();
    }
}
