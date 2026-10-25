package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryData;
import com.minecraftindustrialcomplex.mic_rtsim_create.CreateIds;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.TrainWatch;
import com.minecraftindustrialcomplex.mic_rtsim_create.glass.CreateGlassAccess;
import com.simibubi.create.content.contraptions.actors.psi.PortableStorageInterfaceBlock;
import com.simibubi.create.content.kinetics.RotationPropagator;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import com.simibubi.create.content.kinetics.mechanicalArm.ArmBlockEntity;
import com.simibubi.create.content.logistics.packager.PackagerBlock;
import com.simibubi.create.content.logistics.vault.ItemVaultBlock;
import com.simibubi.create.content.logistics.vault.ItemVaultBlockEntity;
import com.simibubi.create.content.trains.bogey.AbstractBogeyBlock;
import com.simibubi.create.content.trains.entity.Carriage;
import com.simibubi.create.content.trains.entity.CarriageBogey;
import com.simibubi.create.content.trains.entity.Train;
import com.simibubi.create.content.trains.station.GlobalStation;
import com.simibubi.create.content.trains.station.StationBlockEntity;
import it.unimi.dsi.fastutil.longs.Long2LongOpenHashMap;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.TicketType;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;
import net.neoforged.neoforge.items.IItemHandler;
import net.neoforged.neoforge.items.ItemHandlerHelper;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.Set;
import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.*;

/**
 * SEC-1 slice S6, the joins between chunks (SECTIONS.md §2.1, §2.3 and §2.4, §9.1; choices SC1 to SC6 and SC155 to
 * SC159): what joins two member chunks into one district, and what does not.
 *
 * <ul>
 *   <li>Two factories in neighbouring chunks with nothing between them stay two districts: bare 8-adjacency joins
 *       nothing.</li>
 *   <li>J3: a vault across a border joins the chunk of its controller to the chunk of its other block.</li>
 *   <li>J4: a large and a small cogwheel meshing across a border without a face between them join their chunks.</li>
 *   <li>J2 and the saved pair: a mechanical arm reaching a depot across a border joins the chunks, and the pair
 *       survives a detection that comes before the arm's first tick.</li>
 *   <li>The saved pair of an unloaded member: a detection that cannot look at a member chunk keeps its joins.</li>
 *   <li>J6: a station between two factories joins the district its interfaces last served, and the pair outlives a
 *       save and a detection.</li>
 * </ul>
 *
 * <p>Every seed block is a mechanical press (or another real machine) a player placed
 * ({@link IndustryKit#playerPlacedBare}); a press stands at least two blocks from any border it must not touch
 * across. The layouts are synthetic; each test is its own batch and starts by forgetting the yard.
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class SectionsJoinGameTests {
    private SectionsJoinGameTests() {}

    /** A ticket that keeps a chunk FULL (level 33) and not block-ticking: the unloaded neighbour of it is at level 34. */
    private static final TicketType<ChunkPos> FULL_ONLY = TicketType.create("mic_rtsim_create_sec1_join_full_only",
            Comparator.comparingLong(ChunkPos::toLong));

    /** The far fixture's height, above any terrain, and its offset in chunks (clear of the chunk-load and replay tests'). */
    private static final int FAR_Y = 180;
    private static final int FAR_OFFSET = 5500;

    // ---- helpers -----------------------------------------------------------------------------------

    private static Block block(String id) {
        return BuiltInRegistries.BLOCK.get(ResourceLocation.parse(id));
    }

    /** {@code dx} blocks into chunk column {@code cx} past the yard's first border, {@code dz} into the chunk row of the yard's middle row. */
    private static BlockPos at(GameTestHelper helper, int cx, int dx, int dz) {
        return new BlockPos(borderX(helper) + cx * 16 + dx, floorY(helper), ((midZ(helper) >> 4) << 4) + dz);
    }

    private static int seedsAt(GameTestHelper helper, BlockPos pos) {
        return helper.getLevel().getChunkAt(pos).getData(IndustryAttachments.SEEDS.get()).positions().length;
    }

    private static String name(long chunk) {
        return "(" + ChunkPos.getX(chunk) + "," + ChunkPos.getZ(chunk) + ")";
    }

    /** A district's chunks, joins and seeds, for a failure message. */
    private static String describe(District d) {
        if (d == null)
            return "no district";
        StringBuilder sb = new StringBuilder("district " + d.id + " chunks");
        for (long c : d.chunks)
            sb.append(' ').append(name(c));
        sb.append(", joins");
        for (District.Join j : d.joins)
            sb.append(' ').append(name(j.a())).append('-').append(name(j.b()));
        return sb.append(", seeds ").append(d.seedCounts).toString();
    }

    private static boolean holds(District d, long... chunks) {
        if (d == null || d.chunks.size() != chunks.length)
            return false;
        for (long c : chunks)
            if (!d.chunks.contains(c))
                return false;
        return true;
    }

    private static final BlockPos NOWHERE = null;

    // ---- 1. two factories side by side ---------------------------------------------------------------

    /**
     * Two press lines in neighbouring chunks, nothing between them: chunk 0 holds a press, a run of three shafts
     * east of it ending two blocks short of the border, and a depot; chunk 1 mirrors it (shafts ending two blocks
     * short of the same border). Every block is two or more blocks from the border between them. Two districts,
     * one chunk each, and neither has a join pair: touching in the 8-neighbourhood joins nothing.
     */
    @GameTest(template = YARD, batch = "sec1_join_1", timeoutTicks = 600)
    public static void twoFactoriesSideBySideStayTwo(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        String t = "twoFactoriesSideBySideStayTwo";
        BlockState shaft = block("create:shaft").defaultBlockState().setValue(BlockStateProperties.AXIS, Direction.Axis.X);
        BlockPos pressA = at(helper, 0, 10, 4), depotA = at(helper, 0, 10, 8);
        BlockPos pressB = at(helper, 1, 5, 4), depotB = at(helper, 1, 5, 8);
        BlockPos[] shaftsA = {pressA.east(), pressA.east(2), pressA.east(3)};
        BlockPos[] shaftsB = {pressB.west(), pressB.west(2), pressB.west(3)};
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, pressA, block("create:mechanical_press").defaultBlockState());
                    set(helper, pressB, block("create:mechanical_press").defaultBlockState());
                    set(helper, depotA, block("create:depot").defaultBlockState());
                    set(helper, depotB, block("create:depot").defaultBlockState());
                    for (BlockPos p : shaftsA)
                        set(helper, p, shaft);
                    for (BlockPos p : shaftsB)
                        set(helper, p, shaft);
                    playerPlacedBare(helper, pressA);
                    playerPlacedBare(helper, pressB);
                })
                .thenIdle(5)
                .thenExecute(() -> ind().districts().rescanNow(helper.getLevel()))
                .thenWaitUntil(() -> {
                    District a = districtAt(helper, pressA), b = districtAt(helper, pressB);
                    check("a district at each press: " + describe(a) + " | " + describe(b), a != null && b != null);
                    check("two districts, not one: " + describe(a), a != b);
                    check("one chunk each: " + describe(a) + " | " + describe(b),
                            holds(a, chunk(pressA)) && holds(b, chunk(pressB)));
                    check("neither has a join pair: " + describe(a) + " | " + describe(b), a.joins.isEmpty() && b.joins.isEmpty());
                    check("each holds its depot and shafts", districtAt(helper, depotA) == a && districtAt(helper, shaftsA[2]) == a
                            && districtAt(helper, depotB) == b && districtAt(helper, shaftsB[2]) == b);
                    record(t, "district ids", a.id + " and " + b.id);
                    record(t, "chunks of each", a.chunks.size() + " and " + b.chunks.size());
                })
                .thenSucceed();
    }

    // ---- 2. a vault across a border ------------------------------------------------------------------

    private static ItemVaultBlockEntity vaultAt(GameTestHelper helper, BlockPos p) {
        return helper.getLevel().getBlockEntity(p) instanceof ItemVaultBlockEntity v ? v : null;
    }

    /**
     * An item vault two blocks long across the border between chunks -1 and 0 (west block on the last column of -1,
     * east block on the first column of 0). Once Create forms it, one of the two is the controller: the test reads
     * which at run time. A packager beside the other block (the one that is not the controller) in its own chunk,
     * facing the vault, and a press seed in that same chunk. The controller's chunk has no seed, so only J3 (the
     * inventory's canonical position lies in another chunk than the block that leads to it) holds it: one district
     * with both chunks, whose joins are exactly that pair.
     */
    @GameTest(template = YARD, batch = "sec1_join_2", timeoutTicks = 900)
    public static void aVaultAcrossABorderJoinsItsChunks(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        String t = "aVaultAcrossABorderJoinsItsChunks";
        ServerLevel level = helper.getLevel();
        BlockPos west = at(helper, -1, 15, 6), east = at(helper, 0, 0, 6);
        BlockState vault = block("create:item_vault").defaultBlockState().setValue(ItemVaultBlock.HORIZONTAL_AXIS, Direction.Axis.X);
        BlockPos[] controller = {NOWHERE};
        BlockPos[] member = {NOWHERE};
        BlockPos[] press = {NOWHERE};
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, west, vault);
                    set(helper, east, vault);
                })
                .thenWaitUntil(() -> {
                    ItemVaultBlockEntity a = vaultAt(helper, west), b = vaultAt(helper, east);
                    check("both vault blocks have a block entity", a != null && b != null);
                    check("the vault formed: controllers " + a.getController() + " and " + b.getController(),
                            a.getControllerBE() != null && a.getControllerBE() == b.getControllerBE());
                    BlockPos c = a.getController();
                    check("the controller is one of the two: " + c, c != null && (c.equals(west) || c.equals(east)));
                    controller[0] = c.immutable();
                })
                .thenExecute(() -> {
                    boolean controllerWest = controller[0].equals(west);
                    member[0] = controllerWest ? east : west;
                    // the packager stands beside the member block, on its own chunk's side, its inventory behind it
                    BlockPos packager = controllerWest ? east.east() : west.west();
                    Direction facing = controllerWest ? Direction.EAST : Direction.WEST;
                    set(helper, packager, block("create:packager").defaultBlockState().setValue(PackagerBlock.FACING, facing));
                    press[0] = at(helper, controllerWest ? 0 : -1, 8, 11);
                    set(helper, press[0], block("create:mechanical_press").defaultBlockState());
                    playerPlacedBare(helper, press[0]);
                    record(t, "controller", controller[0].toShortString() + " in chunk " + name(chunk(controller[0])) + ", member block "
                            + member[0].toShortString() + " in chunk " + name(chunk(member[0])));
                })
                .thenIdle(5)
                .thenExecute(() -> ind().districts().rescanNow(level))
                .thenWaitUntil(() -> {
                    long cc = chunk(controller[0]), cm = chunk(member[0]);
                    District d = districtAt(helper, press[0]);
                    check("the press's district: " + describe(d), d != null);
                    check("both chunks, nothing else: " + describe(d), holds(d, cc, cm));
                    check("the controller's chunk is in it", districtAt(helper, controller[0]) == d);
                    check("its joins are exactly the vault's pair: " + describe(d), d.joins.equals(Set.of(District.join(cc, cm))));
                    check("the controller's chunk has no seed (only J3 holds it): attachment " + seedsAt(helper, controller[0])
                            + ", district " + d.seedCounts.get(cc), seedsAt(helper, controller[0]) == 0 && d.seedCounts.get(cc) == 0);
                    check("the press is the member chunk's seed: " + d.seedCounts, d.seedCounts.get(cm) == 1);
                    record(t, "district", d.id + " with " + d.chunks.size() + " chunks and " + d.joins.size() + " join");
                })
                .thenSucceed();
    }

    // ---- 3. diagonal cogwheels across a border ---------------------------------------------------------

    /**
     * A large cogwheel (axis Y) on the last column of chunk 0 and a small one (axis Y) diagonally beside it in chunk 1
     * (one block further in x and in z): Create meshes them (a large cog turns a small cog diagonally). The two face
     * neighbours between them are air, so no face touch joins the chunks; a press seed in each chunk, away from the
     * border. J4 (rotation that is not a face, confirmed by {@code RotationPropagator.isConnected}) makes one district
     * of both chunks with that pair saved. The test records whether Create's own check holds one way or the other, so
     * a failure says whether it is the meshing or the join.
     */
    @GameTest(template = YARD, batch = "sec1_join_3", timeoutTicks = 600)
    public static void diagonalCogsAcrossABorderJoin(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        String t = "diagonalCogsAcrossABorderJoin";
        BlockPos large = at(helper, 0, 15, 6), small = at(helper, 1, 0, 7);
        BlockPos pressA = at(helper, 0, 6, 11), pressB = at(helper, 1, 9, 11);
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, large, block("create:large_cogwheel").defaultBlockState().setValue(BlockStateProperties.AXIS, Direction.Axis.Y));
                    set(helper, small, block("create:cogwheel").defaultBlockState().setValue(BlockStateProperties.AXIS, Direction.Axis.Y));
                    // the two blocks that would join them by a face
                    set(helper, new BlockPos(small.getX(), large.getY(), large.getZ()), Blocks.AIR.defaultBlockState());
                    set(helper, new BlockPos(large.getX(), large.getY(), small.getZ()), Blocks.AIR.defaultBlockState());
                    set(helper, pressA, block("create:mechanical_press").defaultBlockState());
                    set(helper, pressB, block("create:mechanical_press").defaultBlockState());
                    playerPlacedBare(helper, pressA);
                    playerPlacedBare(helper, pressB);
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    KineticBlockEntity l = helper.getLevel().getBlockEntity(large) instanceof KineticBlockEntity k ? k : null;
                    KineticBlockEntity s = helper.getLevel().getBlockEntity(small) instanceof KineticBlockEntity k ? k : null;
                    check("both cogwheels have a kinetic block entity: " + l + " and " + s, l != null && s != null);
                    boolean ls = RotationPropagator.isConnected(l, s), sl = RotationPropagator.isConnected(s, l);
                    record(t, "RotationPropagator.isConnected large to small, small to large", ls + ", " + sl);
                    check("Create meshes the two cogwheels (isConnected large to small " + ls + ", small to large " + sl + ")", ls || sl);
                    ind().districts().rescanNow(helper.getLevel());
                })
                .thenWaitUntil(() -> {
                    long ca = chunk(large), cb = chunk(small);
                    District d = districtAt(helper, pressA);
                    check("the first press's district: " + describe(d), d != null);
                    check("both chunks in one district: " + describe(d) + " | other " + describe(districtAt(helper, pressB)), holds(d, ca, cb));
                    check("the second press is in it", districtAt(helper, pressB) == d);
                    check("the pair is saved: " + describe(d), d.joins.equals(Set.of(District.join(ca, cb))));
                    check("one seed in each chunk: " + d.seedCounts, d.seedCounts.get(ca) == 1 && d.seedCounts.get(cb) == 1);
                    record(t, "district", d.id + " with " + d.chunks.size() + " chunks");
                })
                .thenSucceed();
    }

    // ---- 4. an arm reaching across a border ------------------------------------------------------------

    private static ArmBlockEntity armAt(GameTestHelper helper, BlockPos p) {
        return helper.getLevel().getBlockEntity(p) instanceof ArmBlockEntity a ? a : null;
    }

    private static int armInputs(ArmBlockEntity arm) {
        var in = CreateGlassAccess.armInputs(arm);
        return in == null ? -1 : in.size();
    }

    /**
     * A mechanical arm one block short of the last column of chunk 0, a press seed in chunk 0, taking from a depot
     * on the first columns of chunk 1 (three blocks away, in the arm's range) and depositing onto a depot of its own
     * chunk; nothing else in chunk 1. After the arm has ticked (its points read): one district, both chunks, the pair
     * saved (J2). Then the arm before its first tick: its block entity is replaced, in one tick, by a fresh one built
     * from the old one's saved tag (as a chunk load builds it), so its points are not read yet, and a full detection
     * runs in that tick: the district still holds both chunks and the pair (the arm's chunk keeps its saved joins,
     * SC4). Once the fresh arm has ticked, a detection again: still one district.
     */
    @GameTest(template = YARD, batch = "sec1_join_4", timeoutTicks = 1200)
    public static void anArmReachingAcrossABorderJoins(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        String t = "anArmReachingAcrossABorderJoins";
        ServerLevel level = helper.getLevel();
        BlockPos arm = at(helper, 0, 14, 8), take = at(helper, 1, 1, 8), deposit = at(helper, 0, 11, 8), press = at(helper, 0, 5, 3);
        ArmBlockEntity[] fresh = {null};
        long[] id = {0};
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, take, block("create:depot").defaultBlockState());
                    set(helper, deposit, block("create:depot").defaultBlockState());
                    set(helper, press, block("create:mechanical_press").defaultBlockState());
                    // the arm after every point it names: it reads them from its tag at its first tick
                    set(helper, arm, block("create:mechanical_arm").defaultBlockState());
                    GlassCalibrationKit.armTargets(helper, arm, List.of(take), List.of(deposit), ArmBlockEntity.SelectionMode.ROUND_ROBIN);
                    playerPlacedBare(helper, press);
                    playerPlacedBare(helper, arm);
                })
                .thenWaitUntil(() -> {
                    ArmBlockEntity a = armAt(helper, arm);
                    check("the arm exists", a != null);
                    check("the arm has read its points (pending " + CreateGlassAccess.armPointsPending(a) + ", inputs " + armInputs(a) + ")",
                            !CreateGlassAccess.armPointsPending(a) && armInputs(a) == 1);
                })
                .thenExecute(() -> ind().districts().rescanNow(level))
                .thenWaitUntil(() -> {
                    long ca = chunk(arm), cd = chunk(take);
                    District d = districtAt(helper, press);
                    check("the press's district: " + describe(d), d != null);
                    check("both chunks in one district: " + describe(d), holds(d, ca, cd));
                    check("the depot is in it", districtAt(helper, take) == d);
                    check("the pair is saved: " + describe(d), d.joins.contains(District.join(ca, cd)));
                    check("the depot's chunk has no seed: " + d.seedCounts, d.seedCounts.get(cd) == 0);
                    id[0] = d.id;
                    record(t, "after the arm's tick", describe(d));
                })
                .thenExecute(() -> {
                    // The arm as a chunk load builds it: a new block entity from the saved tag, points not read.
                    var registries = level.registryAccess();
                    ArmBlockEntity old = armAt(helper, arm);
                    CompoundTag tag = old.saveWithFullMetadata(registries);
                    BlockEntity made = BlockEntity.loadStatic(arm, level.getBlockState(arm), tag, registries);
                    check("the saved tag builds an arm: " + made, made instanceof ArmBlockEntity);
                    fresh[0] = (ArmBlockEntity) made;
                    level.setBlockEntity(fresh[0]);
                    check("the fresh arm is the block entity now", armAt(helper, arm) == fresh[0] && fresh[0] != old);
                    check("the fresh arm's points are pending", CreateGlassAccess.armPointsPending(fresh[0]));
                    record(t, "fresh arm before its first tick", "pending " + CreateGlassAccess.armPointsPending(fresh[0]) + ", inputs " + armInputs(fresh[0]));
                    ind().districts().rescanNow(level);
                    long ca = chunk(arm), cd = chunk(take);
                    District d = districtAt(helper, press);
                    check("the district is still there: " + describe(d), d != null && d.id == id[0]);
                    check("still both chunks: " + describe(d), holds(d, ca, cd));
                    check("the pair is kept: " + describe(d), d.joins.contains(District.join(ca, cd)));
                    check("the fresh arm has not ticked inside this tick", CreateGlassAccess.armPointsPending(fresh[0]));
                    record(t, "detection before the fresh arm's tick", describe(d));
                })
                .thenWaitUntil(() -> check("the fresh arm has read its points (pending " + CreateGlassAccess.armPointsPending(fresh[0])
                        + ", inputs " + armInputs(fresh[0]) + ")", !CreateGlassAccess.armPointsPending(fresh[0]) && armInputs(fresh[0]) == 1))
                .thenExecute(() -> {
                    ind().districts().rescanNow(level);
                    long ca = chunk(arm), cd = chunk(take);
                    District d = districtAt(helper, press);
                    check("after the fresh arm's tick the district is still one: " + describe(d), d != null && d.id == id[0] && holds(d, ca, cd));
                    check("the pair is still saved: " + describe(d), d.joins.contains(District.join(ca, cd)));
                    record(t, "detection after the fresh arm's tick", describe(d));
                })
                .thenSucceed();
    }

    // ---- 5. an unloaded member keeps its joins -----------------------------------------------------------

    /** Removes the block entities from {@code FAR_Y - 3} to {@code FAR_Y + 3} in the loaded chunks of a box, then forgets the box's industry. */
    private static void clearFar(ServerLevel level, int minCx, int minCz, int maxCx, int maxCz) {
        for (int cx = minCx; cx <= maxCx; cx++)
            for (int cz = minCz; cz <= maxCz; cz++) {
                LevelChunk c = level.getChunkSource().getChunkNow(cx, cz);
                if (c == null)
                    continue;
                for (BlockEntity be : new ArrayList<>(c.getBlockEntities().values()))
                    if (be.getBlockPos().getY() >= FAR_Y - 3 && be.getBlockPos().getY() <= FAR_Y + 3)
                        level.removeBlock(be.getBlockPos(), false);
            }
        ind().districts().forgetArea(level, minCx, minCz, maxCx, maxCz);
    }

    /**
     * Two force-loaded chunks A and B, far from the yard, side by side: a press seed in A, a run of shafts east of it
     * across the border into B, ending in a depot (B holds no seed, so it is a member by the face touch alone): one
     * district with both chunks and the pair. Then B alone unloads. A forced chunk keeps its neighbour loaded
     * (ticket level 31 reaches 32 next door), so A is let go to a FULL-only ticket (level 33) first and B, at 34,
     * unloads with the forcing gone; A stays loaded. A detection that looks at A and cannot look at B
     * ({@code markDirty} A, {@code rescanNow}) keeps the same district id, both chunks and the pair: the join was
     * seen through B's loaded face and an unloaded member keeps what its district saw (SECTIONS.md §2.3, SC157).
     * The chunks are forced and cleared again at the end.
     */
    @GameTest(template = YARD, batch = "sec1_join_5", timeoutTicks = 6000)
    public static void anUnloadedMemberKeepsItsJoins(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        String t = "anUnloadedMemberKeepsItsJoins";
        ServerLevel level = helper.getLevel();
        int ax = farChunk(helper, true, FAR_OFFSET), az = farChunk(helper, false, FAR_OFFSET), bx = ax + 1, z = az * 16 + 8;
        ChunkPos a = new ChunkPos(ax, az);
        BlockPos press = new BlockPos(ax * 16 + 4, FAR_Y, z), depot = new BlockPos(bx * 16 + 4, FAR_Y, z);
        long ca = a.toLong(), cb = ChunkPos.asLong(bx, az);
        BlockState shaft = block("create:shaft").defaultBlockState().setValue(BlockStateProperties.AXIS, Direction.Axis.X);
        long[] id = {0};
        helper.startSequence()
                .thenExecute(() -> {
                    level.setChunkForced(ax, az, true);
                    level.setChunkForced(bx, az, true);
                })
                .thenWaitUntil(paced(() -> check("A and B load", level.getChunkSource().getChunkNow(ax, az) != null
                        && level.getChunkSource().getChunkNow(bx, az) != null)))
                .thenExecute(() -> {
                    clearFar(level, ax - 1, az - 1, bx + 1, az + 1);
                    set(helper, press, block("create:mechanical_press").defaultBlockState());
                    for (int x = press.getX() + 1; x < depot.getX(); x++)
                        set(helper, new BlockPos(x, FAR_Y, z), shaft);
                    set(helper, depot, block("create:depot").defaultBlockState());
                    playerPlacedBare(helper, press);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, press);
                    check("the press's district: " + describe(d), d != null);
                    check("both chunks, the pair: " + describe(d), holds(d, ca, cb) && d.joins.equals(Set.of(District.join(ca, cb))));
                    check("B has no seed: " + describe(d), d.seedCounts.get(cb) == 0 && d.seedCounts.get(ca) == 1);
                    id[0] = d.id;
                    record(t, "district before the unload", describe(d));
                })
                .thenExecute(() -> {
                    level.getChunkSource().addRegionTicket(FULL_ONLY, a, 0, a);
                    level.setChunkForced(ax, az, false);
                    level.setChunkForced(bx, az, false);
                })
                .thenWaitUntil(paced(() -> {
                    District d = ind().district(id[0]);
                    check("B unloads, A stays: A " + (level.getChunkSource().getChunkNow(ax, az) != null) + ", B "
                            + (level.getChunkSource().getChunkNow(bx, az) != null) + ", loaded chunks "
                            + (d == null ? -1 : ind().districts().loadedChunks(d)),
                            level.getChunkSource().getChunkNow(bx, az) == null && level.getChunkSource().getChunkNow(ax, az) != null
                                    && d != null && ind().districts().loadedChunks(d) == 1);
                }))
                .thenExecute(() -> {
                    ind().districts().markDirty(level.dimension(), ca);
                    ind().districts().rescanNow(level);
                    District d = districtAt(helper, press);
                    check("the same district: " + describe(d), d != null && d.id == id[0]);
                    check("both chunks: " + describe(d), holds(d, ca, cb));
                    check("the pair is kept: " + describe(d), d.joins.equals(Set.of(District.join(ca, cb))));
                    check("B is still unloaded", level.getChunkSource().getChunkNow(bx, az) == null);
                    record(t, "district after the detection with B unloaded", describe(d));
                })
                .thenExecute(() -> {
                    level.setChunkForced(ax, az, true);
                    level.setChunkForced(bx, az, true);
                })
                .thenWaitUntil(paced(() -> check("A and B load for the clean-up", level.getChunkSource().getChunkNow(ax, az) != null
                        && level.getChunkSource().getChunkNow(bx, az) != null)))
                .thenExecute(() -> {
                    clearFar(level, ax - 1, az - 1, bx + 1, az + 1);
                    level.getChunkSource().removeRegionTicket(FULL_ONLY, a, 0, a);
                    level.setChunkForced(ax, az, false);
                    level.setChunkForced(bx, az, false);
                })
                .thenSucceed();
    }

    private static int farChunk(GameTestHelper helper, boolean x, int offset) {
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        return ((x ? o.getX() : o.getZ()) >> 4) + offset;
    }

    // ---- 6. a station between two factories ---------------------------------------------------------------

    /** A station on a track along its row, its target track set so the graph takes it in (CreateSeedGameTests). */
    private static void trackFor(GameTestHelper helper, BlockPos stationPos, int fromX, int toX) {
        BlockState track = block("create:track").defaultBlockState()
                .setValue(com.simibubi.create.content.trains.track.TrackBlock.SHAPE, com.simibubi.create.content.trains.track.TrackShape.XO);
        for (int x = fromX; x <= toX; x++)
            set(helper, new BlockPos(x, stationPos.getY(), stationPos.getZ() - 1), track);
    }

    private static void placeStation(GameTestHelper helper, BlockPos stationPos) {
        set(helper, stationPos, block("create:track_station").defaultBlockState());
        StationBlockEntity sbe = (StationBlockEntity) helper.getLevel().getBlockEntity(stationPos);
        CompoundTag tag = sbe.saveWithFullMetadata(helper.getLevel().registryAccess());
        tag.put("TargetTrack", net.minecraft.nbt.NbtUtils.writeBlockPos(new BlockPos(0, 0, -1)));
        tag.putBoolean("TargetDirection", true);
        sbe.loadWithComponents(tag, helper.getLevel().registryAccess());
        playerPlacedBare(helper, stationPos);
    }

    private static Train syntheticTrain(ServerLevel level, BlockPos vaultPos) {
        level.setBlock(vaultPos, block("create:item_vault").defaultBlockState(), 3);
        AbstractBogeyBlock<?> bogeyBlock = (AbstractBogeyBlock<?>) block("create:small_bogey");
        Carriage carriage = new Carriage(new CarriageBogey(bogeyBlock, false, new CompoundTag()),
                new CarriageBogey(bogeyBlock, false, new CompoundTag()), 2);
        carriage.storage.addBlock(level, level.getBlockState(vaultPos), BlockPos.ZERO, vaultPos, level.getBlockEntity(vaultPos));
        carriage.storage.initialize();
        level.setBlock(vaultPos, Blocks.AIR.defaultBlockState(), 3);
        return new Train(UUID.randomUUID(), null, null, new ArrayList<>(List.of(carriage)), new ArrayList<>(), false, 0);
    }

    private static GlobalStation syntheticStation(ServerLevel level, BlockPos pos, String name) {
        GlobalStation st = new GlobalStation();
        st.id = UUID.randomUUID();
        st.name = name;
        st.blockEntityPos = pos;
        st.blockEntityDimension = level.dimension();
        return st;
    }

    private static IItemHandler items(ServerLevel level, BlockPos pos) {
        IItemHandler h = level.getCapability(Capabilities.ItemHandler.BLOCK, pos, null);
        check("an item handler at " + pos.toShortString(), h != null);
        return h;
    }

    /** Moves up to {@code n} of {@code item} from one handler to the other; returns how many moved. */
    private static int move(IItemHandler from, IItemHandler to, Item item, int n) {
        int moved = 0;
        for (int s = 0; s < from.getSlots() && moved < n; s++) {
            if (!from.getStackInSlot(s).is(item))
                continue;
            ItemStack got = from.extractItem(s, n - moved, false);
            ItemStack rest = ItemHandlerHelper.insertItemStacked(to, got, false);
            moved += got.getCount() - rest.getCount();
            if (!rest.isEmpty()) {
                from.insertItem(s, rest, false);
                break;
            }
        }
        return moved;
    }

    private static final int CARGO = 30;

    /**
     * A loaded call at {@code station}, in one tick (as StationCargoGameTests.loadedCall): a synthetic train arrives
     * with {@value #CARGO} gold, the interface at {@code psi} docks, the gold goes into {@code store} and
     * {@value #CARGO} copper comes out of it into the carriage (the interface's work), and the train leaves for
     * {@code partner}. Returns the train.
     */
    private static Train loadedCall(GameTestHelper helper, GlobalStation station, GlobalStation partner, BlockPos psi, BlockPos store) {
        ServerLevel level = helper.getLevel();
        Train train = syntheticTrain(level, helper.absolutePos(new BlockPos(5, 1, 5)));
        IItemHandler car = train.carriages.get(0).storage.getAllItems();
        ItemHandlerHelper.insertItemStacked(car, new ItemStack(Items.GOLD_INGOT, CARGO), false);
        train.arriveAt(station);
        TrainWatch.dockedForTest(train, station, level.dimension(), psi);
        int gold = move(car, items(level, store), Items.GOLD_INGOT, CARGO);
        int copper = move(items(level, store), car, Items.COPPER_INGOT, CARGO);
        train.leaveStation();
        // Create sets the next destination after leaveStation (the schedule's next instruction).
        train.navigation.destination = partner;
        train.navigation.distanceToDestination = 400;
        check("the call moved " + gold + " gold and " + copper + " copper, not " + CARGO + " each", gold == CARGO && copper == CARGO);
        return train;
    }

    /**
     * Three chunks in a row: S (chunk 0) holds only a real station at its first column and its track; A (chunk -1,
     * west of S) holds a press seed far from the border and a second press on A's last column touching the station
     * across the border; B (chunk 1, east of S) holds a press seed, the station's port (a portable storage interface
     * over a chute over a chest, all inside B) and nothing in S touches B. A station and the block beside it across
     * a border join nothing (J1), so S is a chunk with no seed:
     * <ol>
     *   <li>Before any docking there are two districts, A's and B's, and S is in exactly one of them: the one with
     *       the lower id (J6's fallback: the lowest district id beside the station's chunk). The test records which.</li>
     *   <li>A loaded call: a synthetic train arrives at a synthetic station at the station block's position, the
     *       interface docks, cargo moves between the carriage and B's chest, and the train leaves. The station served
     *       the chunk of B's chest ({@code stationServedChunk}); after a detection S is in B's district, A's district
     *       does not hold it, and the two are still apart.</li>
     *   <li>A restart before the next docking: the sent ports are forgotten, a copy of the saved data (saved and loaded
     *       again) still maps S to B's chest chunk, and a detection of S, A and B leaves S with B.</li>
     *   <li>A second loaded call: its IND_TRAIN_ARRIVED names B's district, and so does every IND_STATION_PORTS for
     *       the station.</li>
     * </ol>
     * Districts are named by the chunk they hold at each step, not by id: a detection may hand an id from one district
     * to another (the larger group takes the lower id of the districts it overlaps equally).
     */
    @GameTest(template = YARD, batch = "sec1_join_6", timeoutTicks = 2400)
    public static void aStationBetweenTwoFactories(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        TrainWatch.forgetPorts();
        String t = "aStationBetweenTwoFactories";
        ServerLevel level = helper.getLevel();
        BlockPos stationPos = at(helper, 0, 0, 6);
        BlockPos touch = at(helper, -1, 15, 6), pressA = at(helper, -1, 7, 11);
        BlockPos pressB = at(helper, 1, 10, 11), chest = at(helper, 1, 4, 8), chute = chest.above(), psi = chute.above();
        long cA = chunk(pressA), cS = chunk(stationPos), cB = chunk(pressB);
        check("the chest is in B and the station in S", chunk(chest) == cB && cS != cA && cS != cB);
        long[] ids = new long[2];
        GlobalStation station = syntheticStation(level, stationPos, "Between Halt");
        GlobalStation partner = syntheticStation(level, stationPos.offset(400, 0, 0), "Far Halt");
        Train[] trains = new Train[2];
        helper.startSequence()
                .thenExecute(() -> {
                    trackFor(helper, stationPos, stationPos.getX(), stationPos.getX() + 14);
                    set(helper, pressA, block("create:mechanical_press").defaultBlockState());
                    set(helper, touch, block("create:mechanical_press").defaultBlockState());
                    set(helper, pressB, block("create:mechanical_press").defaultBlockState());
                    set(helper, chest, Blocks.CHEST.defaultBlockState());
                    set(helper, chute, block("create:chute").defaultBlockState());
                    set(helper, psi, block("create:portable_storage_interface").defaultBlockState()
                            .setValue(PortableStorageInterfaceBlock.FACING, Direction.SOUTH));
                    ItemHandlerHelper.insertItemStacked(items(level, chest), new ItemStack(Items.COPPER_INGOT, 64), false);
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    placeStation(helper, stationPos);
                    playerPlacedBare(helper, pressA);
                    playerPlacedBare(helper, touch);
                    playerPlacedBare(helper, pressB);
                })
                .thenWaitUntil(() -> {
                    StationBlockEntity sbe = level.getBlockEntity(stationPos) instanceof StationBlockEntity s ? s : null;
                    check("the station joined the track graph", sbe != null && sbe.getStation() != null);
                })
                .thenExecute(() -> ind().districts().rescanNow(level))
                .thenWaitUntil(() -> {
                    District a = districtAt(helper, pressA), b = districtAt(helper, pressB);
                    check("a district at each factory: " + describe(a) + " | " + describe(b), a != null && b != null);
                    check("two districts, not one: " + describe(a), a != b);
                    District low = a.id < b.id ? a : b, high = low == a ? b : a;
                    check("S is in the lower id's district (" + low.id + "): " + describe(low) + " | " + describe(high),
                            districtAt(helper, stationPos) == low && low.chunks.contains(cS) && !high.chunks.contains(cS));
                    check("A's chunk is A's alone: " + describe(a), a.chunks.contains(cA) && !b.chunks.contains(cA));
                    check("B's chunk is B's alone: " + describe(b), b.chunks.contains(cB) && !a.chunks.contains(cB));
                    check("each is its own chunk and, for the lower id, the station's: " + describe(low) + " | " + describe(high),
                            low.chunks.size() == 2 && high.chunks.size() == 1);
                    check("no join pair holds S to its district: " + describe(low), low.joins.isEmpty() && high.joins.isEmpty());
                    ids[0] = a.id;
                    ids[1] = b.id;
                    record(t, "districts before any docking", "A " + a.id + ", B " + b.id + "; S is in " + (low == a ? "A's" : "B's") + " (the lower id " + low.id + ")");
                })
                .thenExecute(() -> {
                    trains[0] = loadedCall(helper, station, partner, psi, chest);
                    Long served = ind().districts().stationServedChunk(level.dimension(), cS);
                    check("the station served B's chest chunk " + name(cB) + ": " + (served == null ? "nothing" : name(served)),
                            served != null && served == chunk(chest));
                    record(t, "stationServedChunk(S) after the first call", name(served));
                    ind().districts().rescanNow(level);
                })
                .thenWaitUntil(() -> {
                    District a = districtAt(helper, pressA), b = districtAt(helper, pressB);
                    check("a district at each factory: " + describe(a) + " | " + describe(b), a != null && b != null);
                    check("still two districts: " + describe(a), a != b);
                    check("S is with B: " + describe(b), districtAt(helper, stationPos) == b && holds(b, cB, cS));
                    check("A's district does not hold S: " + describe(a), holds(a, cA));
                    record(t, "districts after the first call and a detection", "A " + a.id + " (" + a.chunks.size() + " chunks), B " + b.id
                            + " (" + b.chunks.size() + " chunks); before: A " + ids[0] + ", B " + ids[1]);
                })
                .thenExecute(() -> {
                    TrainWatch.forgetPorts();
                    var registries = level.registryAccess();
                    IndustryData copy = IndustryData.load(IndustryData.get(level.getServer()).save(new CompoundTag(), registries), registries);
                    Long2LongOpenHashMap saved = copy.stationJoins(level.dimension());
                    check("a save and a load still map the station to B's chest chunk: " + saved,
                            saved.containsKey(stationPos.asLong()) && saved.get(stationPos.asLong()) == chunk(chest));
                    ind().districts().markDirty(level.dimension(), cS);
                    ind().districts().markDirty(level.dimension(), cA);
                    ind().districts().markDirty(level.dimension(), cB);
                    ind().districts().rescanNow(level);
                    District a = districtAt(helper, pressA), b = districtAt(helper, pressB);
                    check("after the restart's detection S is still with B: " + describe(a) + " | " + describe(b),
                            a != null && b != null && a != b && districtAt(helper, stationPos) == b && holds(b, cB, cS) && holds(a, cA));
                    record(t, "after the restart", "A " + a.id + ", B " + b.id + ", S with B");
                })
                .thenExecute(() -> {
                    ids[1] = districtAt(helper, pressB).id;
                    trains[1] = loadedCall(helper, station, partner, psi, chest);
                })
                .thenWaitUntil(paced(() -> {
                    List<ByteBuffer> arrived = received(IndRecords.IND_TRAIN_ARRIVED, IndRecords.Train.TRAIN, CreateIds.of(trains[1].id));
                    List<ByteBuffer> ports = received(IndRecords.IND_STATION_PORTS, IndRecords.StationPorts.STATION, CreateIds.of(station.id));
                    check("the second call's records: " + arrived.size() + " arrived, " + ports.size() + " station ports",
                            !arrived.isEmpty() && !ports.isEmpty());
                    long arrivedAt = arrived.get(0).getLong(IndRecords.Train.DISTRICT);
                    check("the arrival names B's district " + ids[1] + ", not " + arrivedAt, arrivedAt == ids[1]);
                    for (ByteBuffer p : ports)
                        check("every station ports record names B's district " + ids[1] + ", not " + p.getLong(IndRecords.StationPorts.DISTRICT),
                                p.getLong(IndRecords.StationPorts.DISTRICT) == ids[1]);
                    record(t, "second call", "IND_TRAIN_ARRIVED district " + arrivedAt + ", " + ports.size() + " IND_STATION_PORTS records, all district " + ids[1]);
                }))
                .thenSucceed();
    }
}
