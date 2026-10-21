package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.Create;
import com.simibubi.create.content.contraptions.glue.SuperGlueEntity;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorBlockEntity;
import com.simibubi.create.content.logistics.packagePort.PackagePortTarget;
import com.simibubi.create.content.logistics.packagePort.frogport.FrogportBlockEntity;
import com.simibubi.create.content.logistics.packager.PackagerBlock;
import com.simibubi.create.content.logistics.packager.PackagerBlockEntity;
import com.simibubi.create.content.trains.entity.Train;
import com.simibubi.create.content.trains.graph.EdgePointType;
import com.simibubi.create.content.trains.graph.TrackEdge;
import com.simibubi.create.content.trains.graph.TrackGraph;
import com.simibubi.create.content.trains.graph.TrackNode;
import com.simibubi.create.content.trains.graph.TrackNodeLocation;
import com.simibubi.create.content.trains.schedule.Schedule;
import com.simibubi.create.content.trains.schedule.ScheduleEntry;
import com.simibubi.create.content.trains.schedule.destination.DestinationInstruction;
import com.simibubi.create.content.trains.signal.SignalBlockEntity;
import com.simibubi.create.content.trains.station.GlobalStation;
import com.simibubi.create.content.trains.station.StationBlockEntity;
import com.simibubi.create.content.trains.track.BezierConnection;
import com.simibubi.create.content.trains.track.TrackBlock;
import com.simibubi.create.content.trains.track.TrackBlockEntity;
import com.simibubi.create.content.trains.track.TrackMaterial;
import com.simibubi.create.content.trains.track.TrackShape;
import net.createmod.catnip.data.Couple;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestGenerator;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.TestFunction;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtUtils;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.Rotation;
import net.minecraft.world.level.block.WallSignBlock;
import net.minecraft.world.level.block.entity.SignBlockEntity;
import net.minecraft.world.level.block.entity.SignText;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.record;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.*;

/**
 * Builds the rtsim-view parser fixture {@code crates/rtsim-view/tests/data/create-rail}: real Create
 * 6.0.10 rail with curves, stations, a signal, an assembled train and logistics block entities, at fixed
 * world coordinates inside region r.1.3 (chunks x 38..40, z 100..103, force-loaded so the server saves
 * them). {@code crates/rtsim-view/tests/extract_rail_fixture.py} cuts the fixture out of the saved world.
 *
 * <p>Runs only with {@code MIC_RAIL_FIXTURE=1}, on a fresh world (the coordinates are fixed, so a reused
 * world already holds the build): {@code MIC_RAIL_FIXTURE=1 MIC_GLASS_SUITE=none ./gametest.sh create
 * -PrtsimEngine=stub -PskipJni} after moving {@code create/run-gametest/world} aside. It asserts only that
 * the build took; the fixture's facts are checked by the extraction script and the parser tests.
 *
 * <p>Layout (y {@value #Y}, the flat world's first air layer):
 * <pre>
 *   straight A   x 608..624, z 1604, XO; "Fixture Central" south of x 616, the train east or west of it
 *   curve 1      90 degrees, (624,1604) leaving east to (634,1614) leaving north: legs 9.5, radius about 9.5
 *   straight B   x 634, z 1614..1634, ZO; signal west of z 1620, "Fixture Yard" east of z 1624
 *   S-bend       (634,1634) leaving south to (638,1648) leaving north
 *   straight C   x 638, z 1648..1656
 *   logistics    x 608..620, z 1610..1618: chain conveyors, frogport, packager and sign, belt, vault, depot
 * </pre>
 * Curves are made as {@code TrackPlacement.placeTracks} makes them: both end blocks with {@code turn=true}
 * and the {@code BezierConnection} on each, the second end's the {@code secondary()}.
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class RailFixtureGameTests {
    private RailFixtureGameTests() {}

    static final int Y = -60;
    static final int CX0 = 38, CX1 = 40, CZ0 = 100, CZ1 = 103;
    static final String CENTRAL = "Fixture Central", YARD = "Fixture Yard", TRAIN = "Fixture Express";
    static final String DEPOT = "Fixture Depot", MILL = "Fixture Mill";

    static final BlockPos C1_A = new BlockPos(624, Y, 1604), C1_B = new BlockPos(634, Y, 1614);
    static final BlockPos S_A = new BlockPos(634, Y, 1634), S_B = new BlockPos(638, Y, 1648);
    static final BlockPos CENTRAL_POS = new BlockPos(616, Y, 1605), CENTRAL_TRACK = new BlockPos(616, Y, 1604);
    static final BlockPos YARD_POS = new BlockPos(635, Y, 1624), SIGNAL_POS = new BlockPos(633, Y, 1620);
    static final BlockPos CHAIN_1 = new BlockPos(610, Y + 2, 1610), CHAIN_2 = new BlockPos(610, Y + 2, 1618);
    static final BlockPos FROGPORT = new BlockPos(612, Y + 2, 1618);
    static final BlockPos PACKAGER = new BlockPos(614, Y, 1610), SIGN = new BlockPos(614, Y, 1611);
    static final BlockPos BELT_START = new BlockPos(614, Y + 1, 1614), BELT_END = new BlockPos(618, Y + 1, 1614);
    static final BlockPos VAULT = new BlockPos(620, Y, 1610), DEPOT_POS = new BlockPos(620, Y, 1614);

    @GameTestGenerator
    public static Collection<TestFunction> railFixture() {
        if (!"1".equals(System.getenv("MIC_RAIL_FIXTURE")))
            return List.of();
        return List.of(new TestFunction("railfixture", "railfixture.build", MicRtsimCreate.MODID + ":" + IndustryKit.YARD,
                Rotation.NONE, 1200, 0L, true, false, 1, 1, false, RailFixtureGameTests::build));
    }

    static void build(GameTestHelper h) {
        ServerLevel level = h.getLevel();
        Direction[] dir = {null};
        Train[] train = {null};
        h.startSequence()
                .thenExecute(() -> {
                    for (int cx = CX0; cx <= CX1; cx++)
                        for (int cz = CZ0; cz <= CZ1; cz++) {
                            level.setChunkForced(cx, cz, true);
                            level.getChunk(cx, cz);
                        }
                    record("railFixture", "surface at (616,1604)", level.getHeight(Heightmap.Types.WORLD_SURFACE, 616, 1604));
                    for (BlockPos p : BlockPos.betweenClosed(CX0 * 16, Y, CZ0 * 16, CX1 * 16 + 15, Y + 6, CZ1 * 16 + 15))
                        if (!level.getBlockState(p).isAir())
                            level.setBlock(p, Blocks.AIR.defaultBlockState(), 2);
                    rail(level);
                })
                .thenWaitUntil(() -> check("one graph with both curves", turnEdges(railGraph()) == 4))
                .thenExecute(() -> {
                    point(level, CENTRAL_POS, AllBlocks.TRACK_STATION.getDefaultState(), new BlockPos(0, 0, -1));
                    point(level, YARD_POS, AllBlocks.TRACK_STATION.getDefaultState(), new BlockPos(-1, 0, 0));
                    point(level, SIGNAL_POS, AllBlocks.TRACK_SIGNAL.getDefaultState(), new BlockPos(1, 0, 0));
                    logistics(h);
                })
                .thenWaitUntil(() -> {
                    check("Fixture Central on the graph", station(level, CENTRAL_POS).getStation() != null);
                    check("Fixture Yard on the graph", station(level, YARD_POS).getStation() != null);
                    check("the signal on the graph", level.getBlockEntity(SIGNAL_POS) instanceof SignalBlockEntity s && s.getSignal() != null);
                    check("the vault has an item handler", level.getCapability(Capabilities.ItemHandler.BLOCK, VAULT, null) != null);
                })
                .thenExecute(() -> {
                    station(level, CENTRAL_POS).updateName(CENTRAL);
                    station(level, YARD_POS).updateName(YARD);
                    fill(h, VAULT, Items.IRON_INGOT, 64);
                    fill(h, VAULT, Items.COPPER_INGOT, 32);
                    fill(h, DEPOT_POS, item("create:andesite_alloy"), 16);
                    dir[0] = station(level, CENTRAL_POS).getAssemblyDirection();
                    check("an assembly direction along straight A", dir[0] != null && dir[0].getAxis() == Direction.Axis.X);
                    cars(h, dir[0]);
                    Create.RAILWAYS.markTracksDirty();
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    StationBlockEntity sbe = station(level, CENTRAL_POS);
                    check("assembly mode", sbe.enterAssemblyMode(null));
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    StationBlockEntity sbe = station(level, CENTRAL_POS);
                    sbe.assemble(UUID.nameUUIDFromBytes("mic-rtsim rail fixture".getBytes()));
                    GlobalStation st = sbe.getStation();
                    train[0] = st == null ? null : st.getPresentTrain();
                    if (train[0] == null) {
                        CompoundTag tag = sbe.saveWithFullMetadata(level.registryAccess());
                        throw new GameTestAssertException("no train assembled: " + tag.get("LastException"));
                    }
                    train[0].name = Component.literal(TRAIN);
                    Schedule schedule = new Schedule();
                    ScheduleEntry entry = new ScheduleEntry();
                    entry.instruction = new DestinationInstruction();
                    entry.instruction.getData().putString("Text", YARD);
                    schedule.entries.add(entry);
                    train[0].runtime.setSchedule(schedule, false);
                    train[0].runtime.paused = true;
                    sbe.exitAssemblyMode();
                    Create.RAILWAYS.markTracksDirty();
                })
                .thenIdle(20)
                .thenExecute(() -> {
                    TrackGraph g = railGraph();
                    check("the train is still on the rail", train[0].graph == g && !train[0].derailed);
                    report(level, g, dir[0], train[0]);
                    Create.RAILWAYS.markTracksDirty();
                })
                .thenSucceed();
    }

    /** The straights, then the curves' {@code BezierConnection}s on both end blocks. */
    static void rail(ServerLevel level) {
        BlockState xo = AllBlocks.TRACK.getDefaultState().setValue(TrackBlock.SHAPE, TrackShape.XO);
        BlockState zo = AllBlocks.TRACK.getDefaultState().setValue(TrackBlock.SHAPE, TrackShape.ZO);
        for (int x = 608; x <= 624; x++)
            level.setBlock(new BlockPos(x, Y, 1604), x == 624 ? xo.setValue(TrackBlock.HAS_BE, true) : xo, 3);
        for (int z = 1614; z <= 1634; z++)
            level.setBlock(new BlockPos(634, Y, z), z == 1614 || z == 1634 ? zo.setValue(TrackBlock.HAS_BE, true) : zo, 3);
        for (int z = 1648; z <= 1656; z++)
            level.setBlock(new BlockPos(638, Y, z), z == 1648 ? zo.setValue(TrackBlock.HAS_BE, true) : zo, 3);
        curve(level, C1_A, new Vec3(1, 0, 0), C1_B, new Vec3(0, 0, -1));
        curve(level, S_A, new Vec3(0, 0, 1), S_B, new Vec3(0, 0, -1));
    }

    /** {@code TrackBlock.getCurveStart}: the bottom centre of the face the curve leaves by. */
    static Vec3 curveStart(BlockPos pos, Vec3 axis) {
        return Vec3.atCenterOf(pos).add(0, -0.5, 0).add(axis.scale(0.5));
    }

    static void curve(ServerLevel level, BlockPos a, Vec3 axisA, BlockPos b, Vec3 axisB) {
        Vec3 up = new Vec3(0, 1, 0);
        BezierConnection bc = new BezierConnection(Couple.create(a, b), Couple.create(curveStart(a, axisA), curveStart(b, axisB)),
                Couple.create(axisA, axisB), Couple.create(up, up), true, false, TrackMaterial.ANDESITE);
        TrackBlockEntity ta = (TrackBlockEntity) level.getBlockEntity(a), tb = (TrackBlockEntity) level.getBlockEntity(b);
        ta.addConnection(bc);
        tb.addConnection(bc.secondary());
        ta.tilt.tryApplySmoothing();
        tb.tilt.tryApplySmoothing();
    }

    /** A station or signal targeting the track at {@code pos + target}, as the existing station tests place one. */
    static void point(ServerLevel level, BlockPos pos, BlockState state, BlockPos target) {
        level.setBlock(pos, state, 3);
        var be = level.getBlockEntity(pos);
        CompoundTag tag = be.saveWithFullMetadata(level.registryAccess());
        tag.put("TargetTrack", NbtUtils.writeBlockPos(target));
        tag.putBoolean("TargetDirection", true);
        be.loadWithComponents(tag, level.registryAccess());
    }

    static StationBlockEntity station(ServerLevel level, BlockPos pos) {
        if (!(level.getBlockEntity(pos) instanceof StationBlockEntity s))
            throw new GameTestAssertException("no station at " + pos.toShortString());
        return s;
    }

    /** The graph holding the rail: the one with a node at curve 1's first end. */
    static TrackGraph railGraph() {
        Vec3 end = curveStart(C1_A, new Vec3(1, 0, 0));
        for (TrackGraph g : Create.RAILWAYS.trackNetworks.values())
            for (TrackNodeLocation loc : g.getNodes())
                if (loc.getLocation().distanceToSqr(end) < 0.01)
                    return g;
        return null;
    }

    static int turnEdges(TrackGraph g) {
        if (g == null)
            return 0;
        int n = 0;
        for (TrackNodeLocation loc : g.getNodes())
            for (TrackEdge e : g.getConnectionsFrom(g.locateNode(loc)).values())
                if (e.isTurn())
                    n++;
        return n;
    }

    /**
     * Two single-bogey carriages on the station's track, as a player builds them: a small bogey at the
     * first block past the station and one four further, controls facing the assembly direction on the
     * first, a chest with coal on the second, each glued.
     */
    static void cars(GameTestHelper h, Direction d) {
        ServerLevel level = h.getLevel();
        BlockState bogey = AllBlocks.SMALL_BOGEY.getDefaultState().setValue(BlockStateProperties.HORIZONTAL_AXIS, Direction.Axis.X);
        BlockPos b1 = CENTRAL_TRACK.relative(d, 1).above(), b2 = CENTRAL_TRACK.relative(d, 5).above();
        level.setBlock(b1, bogey, 3);
        level.setBlock(b2, bogey, 3);
        level.setBlock(b1.above(), AllBlocks.TRAIN_CONTROLS.getDefaultState().setValue(BlockStateProperties.HORIZONTAL_FACING, d), 3);
        chest(h, b2.above());
        fill(h, b2.above(), Items.COAL, 32);
        level.addFreshEntity(new SuperGlueEntity(level, SuperGlueEntity.span(b1, b1.above())));
        level.addFreshEntity(new SuperGlueEntity(level, SuperGlueEntity.span(b2, b2.above())));
    }

    /**
     * Two chain conveyors joined as {@code ChainConveyorConnectionPacket} joins them, a frogport on the
     * second's loop filtering {@value #DEPOT}, a packager with a sign reading {@value #MILL}, a five-block
     * belt, a vault and a depot. Motors turn the chain and the belt.
     */
    static void logistics(GameTestHelper h) {
        ServerLevel level = h.getLevel();
        motor(h, CHAIN_1.below(), Direction.UP, 32);
        set(h, CHAIN_1, AllBlocks.CHAIN_CONVEYOR.getDefaultState());
        set(h, CHAIN_2, AllBlocks.CHAIN_CONVEYOR.getDefaultState());
        ChainConveyorBlockEntity cc1 = be(h, CHAIN_1, ChainConveyorBlockEntity.class), cc2 = be(h, CHAIN_2, ChainConveyorBlockEntity.class);
        cc1.prepareStats();
        cc2.prepareStats();
        if (!cc2.addConnectionTo(CHAIN_1) || !cc1.addConnectionTo(CHAIN_2))
            throw new GameTestAssertException("could not join the chain conveyors");

        set(h, FROGPORT, AllBlocks.PACKAGE_FROGPORT.getDefaultState());
        FrogportBlockEntity port = be(h, FROGPORT, FrogportBlockEntity.class);
        BlockPos off = FROGPORT.subtract(CHAIN_2);
        float angle = (float) Math.toDegrees(Math.atan2(off.getX(), off.getZ()));
        port.target = new PackagePortTarget.ChainConveyorFrogportTarget(CHAIN_2.subtract(FROGPORT), angle < 0 ? angle + 360 : angle, (BlockPos) null, false);
        port.addressFilter = DEPOT;
        port.filterChanged();
        port.notifyUpdate();

        set(h, PACKAGER, AllBlocks.PACKAGER.getDefaultState().setValue(PackagerBlock.FACING, Direction.EAST));
        set(h, SIGN, Blocks.OAK_WALL_SIGN.defaultBlockState().setValue(WallSignBlock.FACING, Direction.SOUTH));
        SignBlockEntity sign = be(h, SIGN, SignBlockEntity.class);
        sign.setText(new SignText().setMessage(0, Component.literal(MILL)), true);
        PackagerBlockEntity packager = be(h, PACKAGER, PackagerBlockEntity.class);
        packager.updateSignAddress();
        packager.notifyUpdate();

        belt(h, BELT_START, BELT_END);
        motor(h, BELT_START.north(), Direction.SOUTH, 16);
        set(h, VAULT, AllBlocks.ITEM_VAULT.getDefaultState());
        set(h, DEPOT_POS, AllBlocks.DEPOT.getDefaultState());
    }

    /** Every fact a parser test may assert, on [gametest] lines. */
    static void report(ServerLevel level, TrackGraph g, Direction d, Train t) {
        int nodes = g.getNodes().size(), edges = 0;
        List<String> turns = new ArrayList<>();
        for (TrackNodeLocation loc : g.getNodes())
            for (var e : g.getConnectionsFrom(g.locateNode(loc)).entrySet()) {
                edges++;
                if (e.getValue().isTurn()) {
                    BezierConnection bc = e.getValue().getTurn();
                    turns.add(loc.getLocation() + " -> " + e.getKey().getLocation() + " starts " + bc.starts + " axes " + bc.axes
                            + " length " + bc.getLength());
                }
            }
        record("railFixture", "graph", g.id + " nodes " + nodes + ", directed edges " + edges + ", turn edges " + turns.size()
                + ", graphs in the world " + Create.RAILWAYS.trackNetworks.size());
        turns.forEach(s -> record("railFixture", "turn", s));
        for (GlobalStation s : g.getPoints(EdgePointType.STATION))
            record("railFixture", "station", s.name + " block " + s.blockEntityPos + " edge " + s.edgeLocation + " at " + s.position);
        for (var s : g.getPoints(EdgePointType.SIGNAL))
            record("railFixture", "signal", s.id + " blocks " + s.blockEntities + " edge " + s.edgeLocation + " at " + s.position);
        record("railFixture", "assembly direction", d);
        record("railFixture", "train", t.name.getString() + " id " + t.id + " carriages " + t.carriages.size() + " spacing " + t.carriageSpacing
                + " station " + (t.getCurrentStation() == null ? null : t.getCurrentStation().name) + " paused " + t.runtime.paused);
        for (BlockPos p : List.of(CHAIN_1, CHAIN_2, FROGPORT, PACKAGER, SIGN, BELT_START, BELT_END, VAULT, DEPOT_POS))
            record("railFixture", "block entity", p.toShortString() + " " + level.getBlockState(p) + " "
                    + level.getBlockEntity(p).saveWithoutMetadata(level.registryAccess()));
    }
}
