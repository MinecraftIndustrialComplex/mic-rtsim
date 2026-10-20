package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Ledgers;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.industry.StockAccess;
import com.minecraftindustrialcomplex.mic_rtsim_create.CreateIds;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.TrainWatch;
import com.simibubi.create.content.logistics.box.PackageItem;
import com.simibubi.create.content.logistics.packagePort.PackagePortBlockEntity;
import com.simibubi.create.content.redstone.displayLink.DisplayLinkBlockEntity;
import com.simibubi.create.content.trains.bogey.AbstractBogeyBlock;
import com.simibubi.create.content.trains.entity.Carriage;
import com.simibubi.create.content.trains.entity.CarriageBogey;
import com.simibubi.create.content.trains.entity.Train;
import com.simibubi.create.content.trains.station.GlobalStation;
import com.simibubi.create.content.trains.station.StationBlockEntity;
import com.simibubi.create.content.trains.track.TrackBlock;
import com.simibubi.create.content.trains.track.TrackShape;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtUtils;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.FullChunkStatus;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.Container;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.SignBlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;
import net.neoforged.neoforge.items.IItemHandler;
import net.neoforged.neoforge.items.ItemHandlerHelper;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.*;

/**
 * The industry GameTests (C15 to C19, INDUSTRY.md phase I-A and the Java side of I-B): the core's
 * district sampler and the Create plugin on Create 6.0.10's own jar, against the stub engine. Every test
 * logs what it compared on a {@code [gametest]} line.
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class IndustryGameTests {
    private IndustryGameTests() {}

    private static Block block(String id) {
        return BuiltInRegistries.BLOCK.get(ResourceLocation.parse(id));
    }

    private static CommandSourceStack source(GameTestHelper helper, BlockPos at) {
        return helper.getLevel().getServer().createCommandSourceStack().withPermission(4).withLevel(helper.getLevel())
                .withPosition(Vec3.atCenterOf(at)).withSuppressedOutput();
    }

    private static void command(GameTestHelper helper, BlockPos at, String cmd) {
        helper.getLevel().getServer().getCommands().performPrefixedCommand(source(helper, at), cmd);
    }

    private static int count(Container c, net.minecraft.world.item.Item item) {
        int n = 0;
        for (int i = 0; i < c.getContainerSize(); i++)
            if (c.getItem(i).is(item))
                n += c.getItem(i).getCount();
        return n;
    }

    private static Container chest(GameTestHelper helper, BlockPos abs) {
        return (Container) helper.getLevel().getBlockEntity(abs);
    }

    // ---- I2: detection, closure under physical connection, names, edits ------------------------

    /**
     * A chest a player placed is a seed; Create shafts run from it across two chunk borders. The district
     * is the three chunks the shafts touch (closed under physical connection), not the chest's chunk
     * alone; rename sets its name (IND_DISTRICT again, a new revision, IND_EDIT aux 4); a split at the
     * far chunk makes two districts, and a shaft across their boundary makes both not learnable
     * (rotation crosses the boundary).
     */
    @GameTest(template = YARD, batch = "industry_01", timeoutTicks = 400)
    public static void aDistrictIsDetectedAcrossShaftsAndNamed(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        int bx = borderX(helper), y = floorY(helper), z = midZ(helper);
        BlockPos chestPos = new BlockPos(bx - 1, y, z);
        set(helper, chestPos, Blocks.CHEST.defaultBlockState());
        BlockState shaft = block("create:shaft").defaultBlockState().setValue(BlockStateProperties.AXIS, Direction.Axis.X);
        for (int x = bx; x <= bx + 16; x++)
            set(helper, new BlockPos(x, y, z), shaft);
        playerPlaced(helper, chestPos);
        long[] id = {0};
        int[] rev = {0};
        BlockPos far = new BlockPos(bx + 16, y, z);
        helper.startSequence()
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, chestPos);
                    check("the chest's chunk is in a district", d != null);
                    check("the district has the three chunks the shafts touch, not " + d.chunks.size(), d.chunks.size() == 3);
                    check("the far chunk is in it", d.chunks.contains(chunk(far)));
                    id[0] = d.id;
                    rev[0] = d.revision;
                })
                .thenExecute(() -> {
                    District d = ind().district(id[0]);
                    record("aDistrictIsDetectedAcrossShaftsAndNamed", "district", d);
                    // The detection's cost for this district (3 chunks, 18 block entities), warm, timed.
                    long best = Long.MAX_VALUE;
                    for (int i = 0; i < 50; i++) {
                        ind().districts().rescanNow(helper.getLevel());
                        best = Math.min(best, ind().districts().lastRescanNanos());
                    }
                    record("aDistrictIsDetectedAcrossShaftsAndNamed", "detection, best of 50 (us)", best / 1000.0);
                    check("a repeated detection keeps the district and its revision", ind().district(d.id) == d && d.chunks.size() == 3 && d.revision == rev[0]);
                    check("named by its coordinates, got " + d.displayName(), d.displayName().startsWith("District "));
                    List<ByteBuffer> decl = received(IndRecords.IND_DISTRICT, IndRecords.District.DISTRICT, d.id);
                    check("IND_DISTRICT was journaled", !decl.isEmpty());
                    command(helper, chestPos, "rtsim district rename " + d.id + " Iron Coast");
                })
                .thenWaitUntil(() -> {
                    District d = ind().district(id[0]);
                    check("renamed: " + d.displayName(), d.displayName().equals("Iron Coast"));
                    boolean named = false;
                    for (ByteBuffer r : received(IndRecords.IND_DISTRICT, IndRecords.District.DISTRICT, d.id))
                        if (r.getInt(IndRecords.District.REVISION) > rev[0] && "Iron Coast".equals(javaName(r.getInt(IndRecords.District.NAME))))
                            named = true;
                    check("a new revision carries the name", named);
                    check("the rename is an IND_EDIT (command)", received(IndRecords.IND_EDIT, IndRecords.Edit.DISTRICT, d.id).stream()
                            .anyMatch(r -> aux(r) == IndRecords.Edit.COMMAND));
                })
                .thenExecute(() -> command(helper, far, "rtsim district split"))
                .thenWaitUntil(() -> {
                    District d = ind().district(id[0]);
                    District split = districtAt(helper, far);
                    check("the far chunk is its own district", split != null && split.id != id[0]);
                    check("the old district kept two chunks: " + d.chunks.size(), d.chunks.size() == 2);
                    check("rotation crosses the boundary: " + d.notLearnable + ", " + split.notLearnable,
                            d.notLearnable == IndRecords.NotLearnable.ROTATION_CROSSES && split.notLearnable == IndRecords.NotLearnable.ROTATION_CROSSES);
                    record("aDistrictIsDetectedAcrossShaftsAndNamed", "after split", d + " and " + split + ", not learnable " + d.notLearnable);
                })
                .thenSucceed();
    }

    /**
     * A real Create station on a real track graph: the district is named after it (GlobalStation.name),
     * and IND_STATION carries the station's id, district and name.
     */
    @GameTest(template = YARD, batch = "industry_02", timeoutTicks = 400)
    public static void aDistrictIsNamedAfterItsStation(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        int y = floorY(helper);
        BlockPos origin = helper.absolutePos(BlockPos.ZERO);
        int zr = origin.getZ() + 10;
        BlockState track = block("create:track").defaultBlockState().setValue(TrackBlock.SHAPE, TrackShape.XO);
        for (int x = origin.getX() + 2; x <= origin.getX() + 31; x++)
            set(helper, new BlockPos(x, y, zr), track);
        BlockPos stationPos = new BlockPos(origin.getX() + 16, y, zr + 1);
        BlockPos vaultPos = stationPos.east();
        helper.startSequence()
                .thenIdle(5) // the track graph forms on the tracks' scheduled ticks
                .thenExecute(() -> {
                    set(helper, stationPos, block("create:track_station").defaultBlockState());
                    StationBlockEntity sbe = (StationBlockEntity) helper.getLevel().getBlockEntity(stationPos);
                    CompoundTag tag = sbe.saveWithFullMetadata(helper.getLevel().registryAccess());
                    tag.put("TargetTrack", NbtUtils.writeBlockPos(new BlockPos(0, 0, -1)));
                    tag.putBoolean("TargetDirection", true);
                    sbe.loadWithComponents(tag, helper.getLevel().registryAccess());
                    set(helper, vaultPos, block("create:item_vault").defaultBlockState());
                    playerPlaced(helper, vaultPos);
                })
                .thenWaitUntil(() -> {
                    StationBlockEntity sbe = (StationBlockEntity) helper.getLevel().getBlockEntity(stationPos);
                    check("the station joined the track graph", sbe != null && sbe.getStation() != null);
                })
                .thenExecute(() -> {
                    StationBlockEntity sbe = (StationBlockEntity) helper.getLevel().getBlockEntity(stationPos);
                    sbe.getStation().name = "Iron Coast";
                    ind().districts().rescanNow(helper.getLevel());
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, vaultPos);
                    check("the vault's district", d != null);
                    check("named after its station: " + d.displayName(), d.displayName().equals("Iron Coast"));
                    StationBlockEntity sbe = (StationBlockEntity) helper.getLevel().getBlockEntity(stationPos);
                    long sid = CreateIds.of(sbe.getStation().id);
                    List<ByteBuffer> st = received(IndRecords.IND_STATION, IndRecords.Station.STATION, sid);
                    check("IND_STATION journaled", st.stream().anyMatch(r -> r.getLong(IndRecords.Station.DISTRICT) == d.id
                            && "Iron Coast".equals(javaName(r.getInt(IndRecords.Station.NAME)))));
                    record("aDistrictIsNamedAfterItsStation", "district", d + ", station id " + Long.toHexString(sid));
                })
                .thenSucceed();
    }

    // ---- I3: the boundary stock sampler ---------------------------------------------------------

    /**
     * A chest and a Create vault (two chunks, one district) and a postbox holding a package: a sweep
     * reports every boundary inventory, the package by its contents, ends with IND_SWEEP_END whose record
     * count matches, and is live. With an unloaded chunk in the district the next sweep is not live.
     */
    @GameTest(template = YARD, batch = "industry_03", timeoutTicks = 600)
    public static void aSweepReportsBoundaryStockAndTheLiveFlag(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        int bx = borderX(helper), y = floorY(helper), z = midZ(helper);
        BlockPos chestPos = new BlockPos(bx - 2, y, z);
        BlockPos vaultPos = new BlockPos(bx + 2, y, z);
        BlockPos postPos = new BlockPos(bx + 4, y, z);
        set(helper, chestPos, Blocks.CHEST.defaultBlockState());
        set(helper, vaultPos, block("create:item_vault").defaultBlockState());
        set(helper, postPos, block("create:white_postbox").defaultBlockState());
        chest(helper, chestPos).setItem(0, new ItemStack(Items.IRON_INGOT, 10));
        chest(helper, chestPos).setItem(1, new ItemStack(Items.GOLD_INGOT, 5));
        playerPlaced(helper, chestPos);
        playerPlaced(helper, vaultPos);
        playerPlaced(helper, postPos);
        long[] id = {0};
        int[] liveSweep = {-1};
        long[] filledAt = {Long.MAX_VALUE};
        helper.startSequence()
                .thenIdle(2)
                .thenExecute(() -> {
                    IItemHandler vault = helper.getLevel().getCapability(Capabilities.ItemHandler.BLOCK, vaultPos, null);
                    check("the vault has an item handler", vault != null);
                    ItemHandlerHelper.insertItemStacked(vault, new ItemStack(Items.COBBLESTONE, 64), false);
                    PackagePortBlockEntity post = (PackagePortBlockEntity) helper.getLevel().getBlockEntity(postPos);
                    post.inventory.setStackInSlot(0, PackageItem.containing(List.of(new ItemStack(Items.DIAMOND, 7))));
                    filledAt[0] = helper.getLevel().getGameTime();
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, chestPos);
                    check("one district for the chest, the vault and the postbox", d != null && d == districtAt(helper, vaultPos) && d == districtAt(helper, postPos));
                    check("three boundary inventories: " + d.inventories, d.inventories.size() == 3);
                    check("the postbox is a postbox inventory", (d.inventoryAt(postPos).kind() & IndRecords.Inventory.KIND_POSTBOX) != 0);
                    id[0] = d.id;
                })
                .thenWaitUntil(() -> {
                    // A live sweep of the three inventories that began after they were filled.
                    String last = "no live sweep of the three inventories after they were filled";
                    for (ByteBuffer end : received(IndRecords.IND_SWEEP_END, IndRecords.SweepEnd.DISTRICT, id[0])) {
                        if ((aux(end) & IndRecords.SweepEnd.FLAG_LIVE) == 0 || end.getInt(IndRecords.SweepEnd.INVENTORIES) != 3)
                            continue;
                        int sweep = end.getInt(IndRecords.SweepEnd.SWEEP);
                        int records = end.getInt(IndRecords.SweepEnd.RECORDS);
                        Map<String, Long> totals = new HashMap<>();
                        int got = 0;
                        boolean afterFill = true;
                        for (ByteBuffer s : received(IndRecords.IND_STOCK_SAMPLE, IndRecords.StockSample.DISTRICT, id[0])) {
                            if (s.getInt(IndRecords.StockSample.SWEEP) != sweep)
                                continue;
                            got++;
                            afterFill &= s.getLong(IndRecords.StockSample.GAME_TIME) > filledAt[0];
                            for (int i = 0; i < aux(s); i++) {
                                int e = IndRecords.StockSample.ENTRIES + i * IndRecords.ENTRY16;
                                totals.merge(javaName(s.getInt(e)), s.getLong(e + 8), Long::sum);
                            }
                        }
                        if (!afterFill)
                            continue;
                        last = "sweep " + sweep + ": records " + records + ", samples " + got + ", totals " + totals;
                        if (records != got || totals.getOrDefault("i/64/minecraft:iron_ingot", 0L) != 10
                                || totals.getOrDefault("i/64/minecraft:gold_ingot", 0L) != 5
                                || totals.getOrDefault("i/64/minecraft:cobblestone", 0L) != 64
                                || totals.getOrDefault("i/64/minecraft:diamond", 0L) != 7
                                || totals.keySet().stream().anyMatch(k -> k.contains("package") || k.contains("cardboard")))
                            continue;
                        liveSweep[0] = sweep;
                        record("aSweepReportsBoundaryStockAndTheLiveFlag", "live sweep", last);
                        return;
                    }
                    throw new net.minecraft.gametest.framework.GameTestAssertException(last
                            + " (wanted: records = samples, iron 10, gold 5, cobblestone 64 in the vault, 7 diamonds from the package, no package good)");
                })
                .thenExecute(() -> {
                    // An unloaded member chunk far away: every later sweep is partial, not live.
                    District d = ind().district(id[0]);
                    ind().districts().addChunkForTest(d, ChunkPos.asLong((bx >> 4) + 4000, (z >> 4) + 4000));
                })
                .thenWaitUntil(() -> {
                    boolean partial = false;
                    for (ByteBuffer end : received(IndRecords.IND_SWEEP_END, IndRecords.SweepEnd.DISTRICT, id[0]))
                        if (end.getInt(IndRecords.SweepEnd.SWEEP) > liveSweep[0] + 1 && (aux(end) & IndRecords.SweepEnd.FLAG_LIVE) == 0)
                            partial = true;
                    check("a sweep with an unloaded chunk is not live", partial);
                    District d = ind().district(id[0]);
                    check("the district is partly loaded: load state " + d.loadState(), d.loadState() == IndRecords.LoadState.PARTIAL);
                    check("IND_LOAD_STATE went out for the transition", received(IndRecords.IND_LOAD_STATE, IndRecords.LoadState.DISTRICT, id[0]).stream()
                            .anyMatch(r -> aux(r) == IndRecords.LoadState.PARTIAL));
                    // No player on a GameTest server: the district is never watched, and says so (F22).
                    List<ByteBuffer> watched = received(IndRecords.IND_WATCHED, IndRecords.DistrictState.DISTRICT, id[0]);
                    check("IND_WATCHED went out, not watched", !watched.isEmpty() && watched.stream().allMatch(r -> aux(r) == 0));
                })
                .thenSucceed();
    }

    // ---- trains ---------------------------------------------------------------------------------

    private static Train syntheticTrain(GameTestHelper helper, BlockPos vaultPos, int iron) {
        ServerLevel level = helper.getLevel();
        set(helper, vaultPos, block("create:item_vault").defaultBlockState());
        IItemHandler vh = level.getCapability(Capabilities.ItemHandler.BLOCK, vaultPos, null);
        if (iron > 0)
            ItemHandlerHelper.insertItemStacked(vh, new ItemStack(Items.IRON_INGOT, iron), false);
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

    private static long carriageCount(Train train, net.minecraft.world.item.Item item) {
        IItemHandler h = train.carriages.get(0).storage.getAllItems();
        long n = 0;
        for (int s = 0; s < h.getSlots(); s++)
            if (h.getStackInSlot(s).is(item))
                n += h.getStackInSlot(s).getCount();
        return n;
    }

    /**
     * A train arrives at a station of a loaded district (Train.arriveAt, through the mixin):
     * IND_TRAIN_ARRIVED with its carriage's contents, not simulated. Ten ingots leave the carriage while
     * it stands there; at leaveStation the cargo diff is an IND_PORT (train cargo, +10 into the district),
     * and IND_TRAIN_DEPARTED follows.
     */
    @GameTest(template = YARD, batch = "industry_04", timeoutTicks = 400)
    public static void aTrainArrivalIsObserved(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        int bx = borderX(helper), y = floorY(helper), z = midZ(helper);
        BlockPos chestPos = new BlockPos(bx + 2, y, z);
        set(helper, chestPos, Blocks.CHEST.defaultBlockState());
        playerPlaced(helper, chestPos);
        Train[] train = {null};
        GlobalStation[] st = {null};
        GlobalStation[] next = {null};
        long[] id = {0};
        helper.startSequence()
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, chestPos);
                    check("district", d != null);
                    id[0] = d.id;
                })
                .thenExecute(() -> {
                    train[0] = syntheticTrain(helper, new BlockPos(bx + 6, y, z), 32);
                    st[0] = syntheticStation(helper.getLevel(), new BlockPos(bx + 3, y, z), "Test Halt");
                    train[0].arriveAt(st[0]);
                })
                .thenWaitUntil(() -> {
                    long tid = CreateIds.of(train[0].id);
                    List<ByteBuffer> arr = received(IndRecords.IND_TRAIN_ARRIVED, IndRecords.Train.TRAIN, tid);
                    check("IND_TRAIN_ARRIVED", arr.size() == 1);
                    ByteBuffer a = arr.get(0);
                    check("at the station", a.getLong(IndRecords.Train.STATION) == CreateIds.of(st[0].id));
                    check("of the district", a.getLong(IndRecords.Train.DISTRICT) == id[0]);
                    check("not simulated (the district is loaded)", (a.getInt(IndRecords.Train.ARRIVED_FLAGS) & IndRecords.Train.FLAG_SIMULATED) == 0);
                    check("one carriage", aux(a) == 1);
                    int c = IndRecords.Train.ARRIVED_CARRIAGES;
                    int n = a.getInt(c + IndRecords.Train.C_ENTRY_COUNT);
                    long iron = 0;
                    for (int i = 0; i < n; i++) {
                        int e = c + IndRecords.Train.C_ENTRIES + i * IndRecords.ENTRY16;
                        if ("i/64/minecraft:iron_ingot".equals(javaName(a.getInt(e))))
                            iron += a.getLong(e + 8);
                    }
                    record("aTrainArrivalIsObserved", "arrival", "slots " + a.getInt(c + IndRecords.Train.C_ITEM_SLOTS) + ", iron " + iron);
                    check("the carriage's 32 iron ingots are in the record: " + iron, iron == 32);
                })
                .thenExecute(() -> {
                    train[0].carriages.get(0).storage.getAllItems().extractItem(0, 10, false);
                    train[0].leaveStation();
                    // Create sets the next destination after leaveStation (the schedule's next instruction).
                    next[0] = syntheticStation(helper.getLevel(), new BlockPos(bx + 400, y, z), "Next Halt");
                    train[0].navigation.destination = next[0];
                    train[0].navigation.distanceToDestination = 400;
                })
                .thenWaitUntil(() -> {
                    long tid = CreateIds.of(train[0].id);
                    List<ByteBuffer> ports = received(IndRecords.IND_PORT, p -> p.getLong(IndRecords.Port.DISTRICT) == id[0]
                            && p.getInt(IndRecords.Port.PORT_KIND) == IndRecords.Port.TRAIN_CARGO && p.getLong(IndRecords.Port.COUNTERPART) == tid);
                    check("an IND_PORT for the train's cargo", ports.size() == 1);
                    ByteBuffer p = ports.get(0);
                    check("one good", aux(p) == 1);
                    check("+10 iron into the district", "i/64/minecraft:iron_ingot".equals(javaName(p.getInt(IndRecords.Port.ENTRIES)))
                            && p.getLong(IndRecords.Port.ENTRIES + 8) == 10);
                    List<ByteBuffer> dep = received(IndRecords.IND_TRAIN_DEPARTED, IndRecords.Train.TRAIN, tid);
                    check("IND_TRAIN_DEPARTED", dep.size() == 1 && dep.get(0).getLong(IndRecords.Train.STATION) == CreateIds.of(st[0].id));
                    check("bound for the next station", dep.get(0).getLong(IndRecords.Train.DEPARTED_DESTINATION) == CreateIds.of(next[0].id));
                    long eta = (long) Math.ceil(400 / Math.max(1e-3, train[0].maxSpeed()));
                    check("ETA = 400 blocks at its top speed: " + dep.get(0).getLong(IndRecords.Train.DEPARTED_ETA) + " vs " + eta,
                            dep.get(0).getLong(IndRecords.Train.DEPARTED_ETA) == eta);
                    record("aTrainArrivalIsObserved", "departure", "destination " + dep.get(0).getLong(IndRecords.Train.DEPARTED_DESTINATION)
                            + ", eta " + dep.get(0).getLong(IndRecords.Train.DEPARTED_ETA));
                    TrainWatch.forget(train[0]);
                })
                .thenSucceed();
    }

    /**
     * A train at a station of an unloaded district (no carriage entity): IND_TRAIN_ARRIVED is simulated.
     * The engine's IND_CARRIAGE_DELTA (the I15 exchange) is applied through TrainCargoManager's handlers,
     * removals first, answered with IND_CARRIAGE_RESULT; the same op id again is a duplicate.
     */
    @GameTest(template = YARD, batch = "industry_05", timeoutTicks = 400)
    public static void aSimulatedStationExchangesThroughTheCargoManager(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        int y = floorY(helper);
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        long farChunk = ChunkPos.asLong((o.getX() >> 4) + 5000, (o.getZ() >> 4) + 5000);
        BlockPos farStation = new BlockPos(ChunkPos.getX(farChunk) * 16 + 8, 64, ChunkPos.getZ(farChunk) * 16 + 8);
        District[] d = {null};
        Train[] train = {null};
        long op = engineOp();
        helper.startSequence()
                .thenExecute(() -> d[0] = ind().districts().createForTest(helper.getLevel(), farChunk))
                .thenWaitUntil(() -> check("the far district is unloaded: " + d[0].loadState(), d[0].loadState() == IndRecords.LoadState.UNLOADED))
                .thenExecute(() -> {
                    train[0] = syntheticTrain(helper, helper.absolutePos(new BlockPos(5, 1, 5)), 32);
                    train[0].arriveAt(syntheticStation(helper.getLevel(), farStation, "Far Halt"));
                })
                .thenWaitUntil(() -> {
                    List<ByteBuffer> arr = received(IndRecords.IND_TRAIN_ARRIVED, IndRecords.Train.TRAIN, CreateIds.of(train[0].id));
                    check("IND_TRAIN_ARRIVED", arr.size() == 1);
                    check("simulated", (arr.get(0).getInt(IndRecords.Train.ARRIVED_FLAGS) & IndRecords.Train.FLAG_SIMULATED) != 0);
                })
                .thenExecute(() -> carriageDelta(op, CreateIds.of(train[0].id), IndRecords.CarriageDelta.EXCHANGE, List.<Object[]>of(
                        new Object[] {0, "i/64/minecraft:copper_ingot", 5L},
                        new Object[] {0, "i/64/minecraft:iron_ingot", -3L})))
                .thenWaitUntil(() -> {
                    List<ByteBuffer> res = received(IndRecords.IND_CARRIAGE_RESULT, IndRecords.CarriageResult.OP_ID, op);
                    check("IND_CARRIAGE_RESULT", res.size() == 1);
                    ByteBuffer r = res.get(0);
                    check("two entries", aux(r) == 2);
                    // Removals first: the iron entry is the first result.
                    int e0 = IndRecords.CarriageResult.ENTRIES, e1 = e0 + IndRecords.RESULT24;
                    check("iron -3 applied first", "i/64/minecraft:iron_ingot".equals(engineName(r.getInt(e0 + 4))) && r.getLong(e0 + 16) == -3);
                    check("copper +5 applied", r.getLong(e1 + 8) == 5 && r.getLong(e1 + 16) == 5);
                    check("the carriage holds 29 iron and 5 copper", carriageCount(train[0], Items.IRON_INGOT) == 29 && carriageCount(train[0], Items.COPPER_INGOT) == 5);
                    check("COMMAND_RESULT applied", commandResults(op).contains(Obs.CommandResult.APPLIED));
                    record("aSimulatedStationExchangesThroughTheCargoManager", "carriage", "iron " + carriageCount(train[0], Items.IRON_INGOT)
                            + ", copper " + carriageCount(train[0], Items.COPPER_INGOT) + ", idle ticks " + train[0].carriages.get(0).storage.getTicksSinceLastExchange());
                })
                .thenExecute(() -> carriageDelta(op, CreateIds.of(train[0].id), IndRecords.CarriageDelta.EXCHANGE, List.<Object[]>of(
                        new Object[] {0, "i/64/minecraft:copper_ingot", 5L})))
                .thenWaitUntil(() -> {
                    check("the same op id again is a duplicate", commandResults(op).contains(Obs.CommandResult.DUPLICATE_OP));
                    check("and changes nothing", carriageCount(train[0], Items.COPPER_INGOT) == 5);
                    TrainWatch.forget(train[0]);
                })
                .thenSucceed();
    }

    /**
     * Mail at an unloaded district (IND_PACKAGE, F26): a package dropped into a postbox's offline buffer
     * while its district is unloaded is IND_PACKAGE in, with its contents and address; one fetched from it
     * is IND_PACKAGE out. (MailWatch is driven around the buffer change the way the runMailTransfer mixins
     * drive it; a real present train needs a track graph.)
     */
    @GameTest(template = YARD, batch = "industry_12", timeoutTicks = 400)
    public static void mailAtAnUnloadedDistrictIsAPackageRecord(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        long farChunk = ChunkPos.asLong((o.getX() >> 4) + 6000, (o.getZ() >> 4) + 6000);
        BlockPos postbox = new BlockPos(ChunkPos.getX(farChunk) * 16 + 3, 64, ChunkPos.getZ(farChunk) * 16 + 3);
        District[] d = {null};
        GlobalStation st = syntheticStation(helper.getLevel(), postbox.east(), "Mail Halt");
        com.simibubi.create.content.trains.station.GlobalPackagePort port = new com.simibubi.create.content.trains.station.GlobalPackagePort();
        port.address = "Iron Coast";
        st.connectedPorts.put(postbox, port);
        helper.startSequence()
                .thenExecute(() -> d[0] = ind().districts().createForTest(helper.getLevel(), farChunk))
                .thenWaitUntil(() -> check("unloaded", d[0].loadState() == IndRecords.LoadState.UNLOADED))
                .thenExecute(() -> {
                    ItemStack box = PackageItem.containing(List.of(new ItemStack(Items.IRON_INGOT, 40), new ItemStack(Items.COAL, 3)));
                    PackageItem.addAddress(box, "Iron Coast");
                    com.minecraftindustrialcomplex.mic_rtsim_create.MailWatch.before(st);
                    port.offlineBuffer.setStackInSlot(0, box);
                    com.minecraftindustrialcomplex.mic_rtsim_create.MailWatch.after(st);
                    com.minecraftindustrialcomplex.mic_rtsim_create.MailWatch.before(st);
                    port.offlineBuffer.setStackInSlot(0, ItemStack.EMPTY);
                    com.minecraftindustrialcomplex.mic_rtsim_create.MailWatch.after(st);
                })
                .thenWaitUntil(() -> {
                    List<ByteBuffer> recs = received(IndRecords.IND_PACKAGE, IndRecords.Package.DISTRICT, d[0].id);
                    check("two IND_PACKAGE records", recs.size() == 2);
                    ByteBuffer in = recs.get(0), out = recs.get(1);
                    check("in, then out", in.getInt(IndRecords.Package.DIRECTION) == IndRecords.Package.IN && out.getInt(IndRecords.Package.DIRECTION) == IndRecords.Package.OUT);
                    check("one package each", in.getInt(IndRecords.Package.PACKAGES) == 1 && out.getInt(IndRecords.Package.PACKAGES) == 1);
                    check("addressed to Iron Coast", "Iron Coast".equals(javaName(in.getInt(IndRecords.Package.ADDRESS))));
                    Map<String, Long> goods = new HashMap<>();
                    for (int i = 0; i < aux(in); i++) {
                        int e = IndRecords.Package.ENTRIES + i * IndRecords.ENTRY16;
                        goods.put(javaName(in.getInt(e)), in.getLong(e + 8));
                    }
                    record("mailAtAnUnloadedDistrictIsAPackageRecord", "package in", goods);
                    check("its contents: 40 iron, 3 coal", goods.getOrDefault("i/64/minecraft:iron_ingot", 0L) == 40 && goods.getOrDefault("i/64/minecraft:coal", 0L) == 3);
                })
                .thenSucceed();
    }

    /**
     * Ports through a player (§3.3, H14), driven by the NeoForge events a real player fires, with a fake
     * player: a container session on a district chest (right-click, open, five ingots go in, close) is an
     * IND_PORT of +5 into the district with the chest's index; a toss inside the district is +3; a pickup
     * there is -2. None of them is an edit.
     */
    @GameTest(template = YARD, batch = "industry_13", timeoutTicks = 400)
    public static void playerPortsAreCounted(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        int bx = borderX(helper), y = floorY(helper), z = midZ(helper);
        BlockPos chestPos = new BlockPos(bx + 4, y, z);
        set(helper, chestPos, Blocks.CHEST.defaultBlockState());
        playerPlaced(helper, chestPos);
        ServerLevel level = helper.getLevel();
        net.neoforged.neoforge.common.util.FakePlayer fp = net.neoforged.neoforge.common.util.FakePlayerFactory.getMinecraft(level);
        long[] id = {0};
        int[] edits = {0};
        helper.startSequence()
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, chestPos);
                    check("district", d != null && d.inventoryAt(chestPos) != null);
                    id[0] = d.id;
                    edits[0] = received(IndRecords.IND_EDIT, IndRecords.Edit.DISTRICT, d.id).size();
                })
                .thenExecute(() -> {
                    fp.setPos(chestPos.getX() + 0.5, chestPos.getY() + 1, chestPos.getZ() + 0.5); // in the chest's chunk
                    fp.getInventory().clearContent();
                    fp.getInventory().setItem(0, new ItemStack(Items.IRON_INGOT, 5));
                    net.minecraft.world.inventory.ChestMenu menu = net.minecraft.world.inventory.ChestMenu.threeRows(1, fp.getInventory(), chest(helper, chestPos));
                    net.neoforged.neoforge.common.NeoForge.EVENT_BUS.post(new net.neoforged.neoforge.event.entity.player.PlayerInteractEvent.RightClickBlock(
                            fp, net.minecraft.world.InteractionHand.MAIN_HAND, chestPos,
                            new net.minecraft.world.phys.BlockHitResult(Vec3.atCenterOf(chestPos), Direction.UP, chestPos, false)));
                    net.neoforged.neoforge.common.NeoForge.EVENT_BUS.post(new net.neoforged.neoforge.event.entity.player.PlayerContainerEvent.Open(fp, menu));
                    chest(helper, chestPos).setItem(0, fp.getInventory().removeItem(0, 5));
                    net.neoforged.neoforge.common.NeoForge.EVENT_BUS.post(new net.neoforged.neoforge.event.entity.player.PlayerContainerEvent.Close(fp, menu));
                    ItemEntity tossed = new ItemEntity(level, fp.getX(), fp.getY(), fp.getZ(), new ItemStack(Items.COAL, 3));
                    net.neoforged.neoforge.common.NeoForge.EVENT_BUS.post(new net.neoforged.neoforge.event.entity.item.ItemTossEvent(tossed, fp));
                    ItemEntity lying = new ItemEntity(level, chestPos.getX() + 0.5, chestPos.getY() + 1, chestPos.getZ() + 0.5, new ItemStack(Items.REDSTONE, 2));
                    ItemStack original = lying.getItem().copy();
                    lying.setItem(ItemStack.EMPTY);
                    net.neoforged.neoforge.common.NeoForge.EVENT_BUS.post(new net.neoforged.neoforge.event.entity.player.ItemEntityPickupEvent.Post(fp, lying, original));
                })
                .thenWaitUntil(() -> {
                    Map<Integer, Map<String, Long>> byKind = new HashMap<>();
                    int inventory = -2;
                    for (ByteBuffer p : received(IndRecords.IND_PORT, IndRecords.Port.DISTRICT, id[0])) {
                        int kind = p.getInt(IndRecords.Port.PORT_KIND);
                        if (kind == IndRecords.Port.PLAYER_SESSION)
                            inventory = p.getInt(IndRecords.Port.INVENTORY);
                        for (int i = 0; i < aux(p); i++) {
                            int e = IndRecords.Port.ENTRIES + i * IndRecords.ENTRY16;
                            byKind.computeIfAbsent(kind, k -> new HashMap<>()).merge(javaName(p.getInt(e)), p.getLong(e + 8), Long::sum);
                        }
                    }
                    record("playerPortsAreCounted", "ports by kind", byKind);
                    check("the session: +5 iron into the district", byKind.getOrDefault(IndRecords.Port.PLAYER_SESSION, Map.of()).getOrDefault("i/64/minecraft:iron_ingot", 0L) == 5);
                    check("with the chest's inventory index", inventory == ind().district(id[0]).inventoryAt(chestPos).index());
                    check("the toss: +3 coal", byKind.getOrDefault(IndRecords.Port.PLAYER_TOSS, Map.of()).getOrDefault("i/64/minecraft:coal", 0L) == 3);
                    check("the pickup: -2 redstone", byKind.getOrDefault(IndRecords.Port.PLAYER_PICKUP, Map.of()).getOrDefault("i/64/minecraft:redstone", 0L) == -2);
                    check("no edit from any of them", received(IndRecords.IND_EDIT, IndRecords.Edit.DISTRICT, id[0]).size() == edits[0]);
                    fp.getInventory().clearContent();
                })
                .thenSucceed();
    }

    /**
     * A double chest is one boundary inventory, at its lower half, whichever half is named: tagged
     * storage, a packager's target and a player's click all go through one canonical function, which
     * never loads a chunk (review findings 3 and 10).
     */
    @GameTest(template = YARD, batch = "industry_14", timeoutTicks = 400)
    public static void aDoubleChestIsOneInventory(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        int bx = borderX(helper), y = floorY(helper), z = midZ(helper);
        BlockPos a = new BlockPos(bx + 2, y, z), b = a.east();
        set(helper, a, Blocks.CHEST.defaultBlockState().setValue(net.minecraft.world.level.block.ChestBlock.FACING, Direction.NORTH)
                .setValue(net.minecraft.world.level.block.ChestBlock.TYPE, net.minecraft.world.level.block.state.properties.ChestType.LEFT));
        set(helper, b, Blocks.CHEST.defaultBlockState().setValue(net.minecraft.world.level.block.ChestBlock.FACING, Direction.NORTH)
                .setValue(net.minecraft.world.level.block.ChestBlock.TYPE, net.minecraft.world.level.block.state.properties.ChestType.RIGHT));
        playerPlaced(helper, a);
        playerPlaced(helper, b);
        helper.startSequence()
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, a);
                    check("district", d != null);
                    BlockPos lower = a.asLong() < b.asLong() ? a : b;
                    check("one inventory for the two halves: " + d.inventories, d.inventories.size() == 1 && d.inventories.get(0).pos().equals(lower));
                    check("54 slots", d.inventories.get(0).itemSlots() == 54);
                    check("either half leads to the lower one", lower.equals(com.minecraftindustrialcomplex.mic_rtsim.industry.Districts.canonicalInventory(helper.getLevel(), a))
                            && lower.equals(com.minecraftindustrialcomplex.mic_rtsim.industry.Districts.canonicalInventory(helper.getLevel(), b)));
                    long far = ChunkPos.asLong((a.getX() >> 4) + 7000, (a.getZ() >> 4) + 7000);
                    check("an unloaded position has none (and is not loaded)", com.minecraftindustrialcomplex.mic_rtsim.industry.Districts.canonicalInventory(
                            helper.getLevel(), new BlockPos(ChunkPos.getX(far) * 16, 64, ChunkPos.getZ(far) * 16)) == null
                            && helper.getLevel().getChunkSource().getChunkNow(ChunkPos.getX(far), ChunkPos.getZ(far)) == null);
                })
                .thenSucceed();
    }

    private static String engineName(int engineGoodId) {
        return ind().engineName(IndRecords.DICT_GOOD, engineGoodId);
    }

    // ---- I8: reconciliation ---------------------------------------------------------------------

    private static ByteBuffer result(long op) {
        List<ByteBuffer> r = received(IndRecords.IND_RECONCILE_RESULT, IndRecords.ReconcileResult.OP_ID, op);
        return r.isEmpty() ? null : r.get(r.size() - 1);
    }

    /**
     * Exactly once (I8): a reconcile adds 20 iron and writes epoch 1 in the chunk; the same op id resent
     * (an engine rolled back) and a new op id for the same epoch (a journal lost, the chunk kept) are both
     * duplicates; a chunk restored to its state before the reconcile (saved before the epoch) takes it
     * again, once; epoch 2 then applies on top.
     */
    @GameTest(template = YARD, batch = "industry_06", timeoutTicks = 600)
    public static void aReconcileAppliesOnceAcrossASimulatedCrash(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        int bx = borderX(helper), y = floorY(helper), z = midZ(helper);
        BlockPos chestPos = new BlockPos(bx + 2, y, z);
        set(helper, chestPos, Blocks.CHEST.defaultBlockState());
        playerPlaced(helper, chestPos);
        ServerLevel level = helper.getLevel();
        int cx = chestPos.getX() >> 4, cz = chestPos.getZ() >> 4;
        long[] id = {0};
        long a = engineOp(), b = engineOp(), c = engineOp(), e = engineOp();
        List<Delta> plus20 = List.of(new Delta(0, "i/64/minecraft:iron_ingot", chestPos, 20));
        helper.startSequence()
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, chestPos);
                    check("district", d != null);
                    id[0] = d.id;
                })
                .thenExecute(() -> {
                    // A player's block in the district is an edit (I5); a block set by anything else is not.
                    BlockPos barrel = chestPos.east(2), stone = chestPos.east(3);
                    set(helper, barrel, Blocks.BARREL.defaultBlockState());
                    playerPlaced(helper, barrel);
                    set(helper, stone, Blocks.STONE.defaultBlockState());
                    reconcile(a, id[0], 1, level, cx, cz, plus20, List.of());
                })
                .thenWaitUntil(() -> {
                    List<ByteBuffer> edits = received(IndRecords.IND_EDIT, IndRecords.Edit.DISTRICT, id[0]);
                    check("one IND_EDIT (placed) for the player's barrel, none for the other block: " + edits.size(),
                            edits.size() == 1 && aux(edits.get(0)) == IndRecords.Edit.PLACED && edits.get(0).getInt(IndRecords.Edit.X) == chestPos.getX() + 2);
                    ByteBuffer r = result(a);
                    check("IND_RECONCILE_RESULT for op a", r != null);
                    check("+20 applied", r.getLong(IndRecords.ReconcileResult.ENTRIES + 16) == 20);
                    check("the chest holds 20 iron", count(chest(helper, chestPos), Items.IRON_INGOT) == 20);
                    LevelChunk chunk = level.getChunk(cx, cz);
                    check("epoch 1 is in the chunk", chunk.getData(IndustryAttachments.RECONCILE.get()).epoch(id[0]) == 1);
                    check("COMMAND_RESULT applied", commandResults(a).contains(Obs.CommandResult.APPLIED));
                })
                .thenExecute(() -> reconcile(a, id[0], 1, level, cx, cz, plus20, List.of()))   // the engine resends (rolled back)
                .thenWaitUntil(() -> check("op a again is a duplicate", commandResults(a).contains(Obs.CommandResult.DUPLICATE_OP)))
                .thenExecute(() -> reconcile(b, id[0], 1, level, cx, cz, plus20, List.of()))   // journal lost, chunk kept epoch 1
                .thenWaitUntil(() -> {
                    check("a new op id for epoch 1 is a duplicate by the chunk's epoch", commandResults(b).contains(Obs.CommandResult.DUPLICATE_OP));
                    check("the chest still holds 20", count(chest(helper, chestPos), Items.IRON_INGOT) == 20);
                })
                .thenExecute(() -> {
                    // The crash the other way: the chunk on disk is older than the reconcile.
                    chest(helper, chestPos).clearContent();
                    level.getChunk(cx, cz).removeData(IndustryAttachments.RECONCILE.get());
                    reconcile(c, id[0], 1, level, cx, cz, plus20, List.of());
                })
                .thenWaitUntil(() -> {
                    ByteBuffer r = result(c);
                    check("the older chunk takes epoch 1", r != null && r.getLong(IndRecords.ReconcileResult.ENTRIES + 16) == 20);
                    check("exactly once: 20 iron", count(chest(helper, chestPos), Items.IRON_INGOT) == 20);
                })
                .thenExecute(() -> reconcile(e, id[0], 2, level, cx, cz, List.of(new Delta(0, "i/64/minecraft:iron_ingot", chestPos, 5)), List.of()))
                .thenWaitUntil(() -> {
                    check("epoch 2 applies on top: 25", count(chest(helper, chestPos), Items.IRON_INGOT) == 25);
                    check("epoch 2 is in the chunk", level.getChunk(cx, cz).getData(IndustryAttachments.RECONCILE.get()).epoch(id[0]) == 2);
                    record("aReconcileAppliesOnceAcrossASimulatedCrash", "results", "a " + commandResults(a) + ", b " + commandResults(b)
                            + ", c " + commandResults(c) + ", e " + commandResults(e));
                })
                .thenSucceed();
    }

    /**
     * Removals first (I8): a full chest, and a reconcile listing +64 iron before -64 cobblestone and
     * -50 diamonds. Only removing first makes room for the iron. The chest holds 10 diamonds, so the
     * diamond removal comes up short, and the result says so (requested -50, applied -10).
     */
    @GameTest(template = YARD, batch = "industry_07", timeoutTicks = 400)
    public static void removalsGoFirstAndAShortfallIsReported(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        int bx = borderX(helper), y = floorY(helper), z = midZ(helper);
        BlockPos chestPos = new BlockPos(bx + 3, y, z);
        set(helper, chestPos, Blocks.CHEST.defaultBlockState());
        Container chest = chest(helper, chestPos);
        for (int i = 0; i < 26; i++)
            chest.setItem(i, new ItemStack(Items.COBBLESTONE, 64));
        chest.setItem(26, new ItemStack(Items.DIAMOND, 10));
        playerPlaced(helper, chestPos);
        ServerLevel level = helper.getLevel();
        long op = engineOp();
        long[] id = {0};
        helper.startSequence()
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, chestPos);
                    check("district", d != null);
                    id[0] = d.id;
                })
                .thenExecute(() -> reconcile(op, id[0], 1, level, chestPos.getX() >> 4, chestPos.getZ() >> 4, List.of(
                        new Delta(0, "i/64/minecraft:iron_ingot", chestPos, 64),
                        new Delta(0, "i/64/minecraft:cobblestone", chestPos, -64),
                        new Delta(0, "i/64/minecraft:diamond", chestPos, -50)), List.of()))
                .thenWaitUntil(() -> {
                    ByteBuffer r = result(op);
                    check("IND_RECONCILE_RESULT", r != null);
                    Map<String, long[]> byGood = new HashMap<>();
                    List<String> order = new ArrayList<>();
                    for (int i = 0; i < aux(r); i++) {
                        int e = IndRecords.ReconcileResult.ENTRIES + i * IndRecords.RESULT24;
                        String g = engineName(r.getInt(e + 4));
                        order.add(g);
                        byGood.put(g, new long[] {r.getLong(e + 8), r.getLong(e + 16)});
                    }
                    record("removalsGoFirstAndAShortfallIsReported", "result order", order);
                    check("the removals come first in the result", !order.get(0).contains("iron") && !order.get(1).contains("iron"));
                    check("cobblestone -64 applied", byGood.get("i/64/minecraft:cobblestone")[1] == -64);
                    check("diamond: -50 requested, -10 applied", byGood.get("i/64/minecraft:diamond")[0] == -50 && byGood.get("i/64/minecraft:diamond")[1] == -10);
                    check("iron +64 fit after the removals", byGood.get("i/64/minecraft:iron_ingot")[1] == 64);
                    check("the chest: 64 iron, 0 diamonds, 25 stacks of cobblestone", count(chest(helper, chestPos), Items.IRON_INGOT) == 64
                            && count(chest(helper, chestPos), Items.DIAMOND) == 0 && count(chest(helper, chestPos), Items.COBBLESTONE) == 25 * 64);
                })
                .thenSucceed();
    }

    /**
     * Damage (C18, I23): a reconcile's destroy entry whose block still has its expected state is broken
     * with drops, so the chest's 12 emeralds lie on the ground; one whose state changed is skipped; no
     * fire. The result names the block broken.
     */
    @GameTest(template = YARD, batch = "industry_08", timeoutTicks = 400)
    public static void aBrokenBlockDropsItsContents(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        int bx = borderX(helper), y = floorY(helper), z = midZ(helper);
        BlockPos chestPos = new BlockPos(bx + 5, y, z);
        BlockPos barrelPos = new BlockPos(bx + 8, y, z);
        set(helper, chestPos, Blocks.CHEST.defaultBlockState());
        set(helper, barrelPos, Blocks.BARREL.defaultBlockState());
        chest(helper, chestPos).setItem(0, new ItemStack(Items.EMERALD, 12));
        playerPlaced(helper, chestPos);
        ServerLevel level = helper.getLevel();
        long op = engineOp();
        long[] id = {0};
        String chestState = BlockStateParser.serialize(level.getBlockState(chestPos));
        String wrongBarrel = BlockStateParser.serialize(level.getBlockState(barrelPos).setValue(BlockStateProperties.OPEN, true));
        helper.startSequence()
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, chestPos);
                    check("district", d != null && d == districtAt(helper, barrelPos));
                    id[0] = d.id;
                })
                .thenExecute(() -> reconcile(op, id[0], 1, level, chestPos.getX() >> 4, chestPos.getZ() >> 4, List.of(),
                        List.of(new Destroy(chestPos, chestState), new Destroy(barrelPos, wrongBarrel))))
                .thenWaitUntil(() -> {
                    ByteBuffer r = result(op);
                    check("IND_RECONCILE_RESULT", r != null);
                    check("one block broken, one skipped", r.getInt(IndRecords.ReconcileResult.DESTROYED) == 1 && r.getInt(IndRecords.ReconcileResult.SKIPPED) == 1);
                    int p = IndRecords.ReconcileResult.ENTRIES;
                    check("the broken block is the chest", r.getInt(p) == chestPos.getX() && r.getInt(p + 4) == chestPos.getY() && r.getInt(p + 8) == chestPos.getZ());
                    check("the chest is gone", level.getBlockState(chestPos).isAir());
                    check("the barrel stands", level.getBlockState(barrelPos).is(Blocks.BARREL));
                    int emeralds = 0;
                    for (ItemEntity it : level.getEntitiesOfClass(ItemEntity.class, new AABB(chestPos).inflate(3)))
                        if (it.getItem().is(Items.EMERALD))
                            emeralds += it.getItem().getCount();
                    check("its 12 emeralds dropped: " + emeralds, emeralds == 12);
                    boolean fire = false;
                    for (BlockPos q : BlockPos.betweenClosed(chestPos.offset(-3, -1, -3), chestPos.offset(3, 2, 3)))
                        if (level.getBlockState(q).is(Blocks.FIRE))
                            fire = true;
                    check("no fire", !fire);
                    record("aBrokenBlockDropsItsContents", "dropped emeralds", emeralds);
                    for (ItemEntity it : level.getEntitiesOfClass(ItemEntity.class, new AABB(chestPos).inflate(3)))
                        it.discard();
                })
                .thenSucceed();
    }

    /**
     * Commands held for a chunk that is not loaded (§4.1 step 1): a far chunk's district, unloaded; its
     * reconcile waits; when the chunk loads, IND_CHUNK_LOADED (epoch 0) goes out and the held command
     * applies. The epoch went to disk with the chunk: after another unload and load, IND_CHUNK_LOADED
     * carries epoch 1, read back from the chunk's NBT.
     */
    @GameTest(template = YARD, batch = "industry_09", timeoutTicks = 2400)
    public static void aHeldReconcileAppliesWhenItsChunkLoads(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        int fx = (o.getX() >> 4) + 3000, fz = (o.getZ() >> 4) + 3000;
        BlockPos chestPos = new BlockPos(fx * 16 + 5, 100, fz * 16 + 5);
        long op = engineOp();
        long[] id = {0};
        int[] heldBefore = {0};
        int[] loadsBefore = {0};
        helper.startSequence()
                .thenExecute(() -> level.setChunkForced(fx, fz, true))
                .thenWaitUntil(() -> {
                    LevelChunk c = level.getChunkSource().getChunkNow(fx, fz);
                    check("the far chunk loads", c != null && c.getFullStatus().isOrAfter(FullChunkStatus.BLOCK_TICKING));
                })
                .thenExecute(() -> {
                    ind().districts().forgetArea(level, fx - 1, fz - 1, fx + 1, fz + 1);
                    level.setBlock(chestPos, Blocks.CHEST.defaultBlockState(), 3);
                    playerPlaced(helper, chestPos);
                })
                .thenWaitUntil(() -> {
                    District d = ind().districtAt(level, chestPos);
                    check("district", d != null);
                    id[0] = d.id;
                })
                .thenExecute(() -> level.setChunkForced(fx, fz, false))
                .thenWaitUntil(() -> check("the far chunk unloads (ChunkEvent.Unload, saved)", level.getChunkSource().getChunkNow(fx, fz) == null
                        && ind().districts().loadedChunks(ind().district(id[0])) == 0))
                .thenExecute(() -> {
                    heldBefore[0] = ind().reconciler().held();
                    reconcile(op, id[0], 1, level, fx, fz, List.of(new Delta(0, "i/64/minecraft:gold_ingot", chestPos, 7)), List.of());
                })
                .thenWaitUntil(() -> check("the reconcile is held", ind().reconciler().held() == heldBefore[0] + 1 && result(op) == null))
                .thenExecute(() -> {
                    loadsBefore[0] = received(IndRecords.IND_CHUNK_LOADED, IndRecords.ChunkLoaded.DISTRICT, id[0]).size();
                    level.setChunkForced(fx, fz, true);
                })
                .thenWaitUntil(() -> {
                    ByteBuffer r = result(op);
                    check("applied once the chunk loaded", r != null && r.getLong(IndRecords.ReconcileResult.ENTRIES + 16) == 7);
                    check("the chest holds 7 gold", count((Container) level.getBlockEntity(chestPos), Items.GOLD_INGOT) == 7);
                    List<ByteBuffer> loads = received(IndRecords.IND_CHUNK_LOADED, IndRecords.ChunkLoaded.DISTRICT, id[0]);
                    check("IND_CHUNK_LOADED went out at the load", loads.size() > loadsBefore[0]);
                    check("with epoch 0", loads.get(loads.size() - 1).getInt(IndRecords.ChunkLoaded.APPLIED_EPOCH) == 0);
                })
                .thenExecute(() -> level.setChunkForced(fx, fz, false))
                .thenWaitUntil(() -> check("unloads again (ChunkEvent.Unload, saved)", level.getChunkSource().getChunkNow(fx, fz) == null
                        && ind().districts().loadedChunks(ind().district(id[0])) == 0))
                .thenExecute(() -> {
                    loadsBefore[0] = received(IndRecords.IND_CHUNK_LOADED, IndRecords.ChunkLoaded.DISTRICT, id[0]).size();
                    level.setChunkForced(fx, fz, true);
                })
                .thenWaitUntil(() -> {
                    List<ByteBuffer> loads = received(IndRecords.IND_CHUNK_LOADED, IndRecords.ChunkLoaded.DISTRICT, id[0]);
                    check("loaded again", loads.size() > loadsBefore[0]);
                    int epoch = loads.get(loads.size() - 1).getInt(IndRecords.ChunkLoaded.APPLIED_EPOCH);
                    check("epoch 1 came back from the chunk's NBT: " + epoch, epoch == 1);
                    check("and so did the gold", count((Container) level.getBlockEntity(chestPos), Items.GOLD_INGOT) == 7);
                    record("aHeldReconcileAppliesWhenItsChunkLoads", "chunk loads reported", loads.size());
                })
                .thenExecute(() -> {
                    level.setBlock(chestPos, Blocks.AIR.defaultBlockState(), 3);
                    ind().districts().forgetArea(level, fx - 1, fz - 1, fx + 1, fz + 1);
                    level.setChunkForced(fx, fz, false);
                })
                .thenSucceed();
    }

    // ---- I18: the ledger on a display ---------------------------------------------------------------

    /**
     * The "District ledger" display source on a Stock Link: a Display Link above it targets a sign, and
     * the sign shows the district's name, state and its top good per hour from the engine's IND_LEDGER.
     * Asking is IND_LEDGER_REQUEST.
     */
    @GameTest(template = YARD, batch = "industry_10", timeoutTicks = 400)
    public static void theLedgerShowsOnADisplay(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        int bx = borderX(helper), y = floorY(helper), z = midZ(helper);
        BlockPos vaultPos = new BlockPos(bx + 2, y, z);
        BlockPos linkPos = vaultPos.east();
        BlockPos displayPos = linkPos.above();
        BlockPos signPos = new BlockPos(bx + 6, y, z);
        set(helper, vaultPos, block("create:item_vault").defaultBlockState());
        set(helper, linkPos, block("create:stock_link").defaultBlockState());
        set(helper, displayPos, block("create:display_link").defaultBlockState().setValue(BlockStateProperties.FACING, Direction.UP));
        set(helper, signPos, Blocks.OAK_SIGN.defaultBlockState());
        playerPlaced(helper, vaultPos);
        long[] id = {0};
        helper.startSequence()
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, vaultPos);
                    check("district", d != null);
                    id[0] = d.id;
                })
                .thenExecute(() -> {
                    ind().ledgers().request(ind().district(id[0]));
                    // The native engine's industry engine answers the request itself (its line for a
                    // district it is still learning); the stub needs the answer scripted.
                    if (!nativeEngine())
                        ledger(id[0], IndRecords.Ledger.STATE_LEARNED, 0, -1, 0, null, 0,
                                List.of(new LedgerGood("i/64/minecraft:iron_ingot", 120, 500)));
                })
                .thenWaitUntil(() -> check("the ledger came", ind().ledgers().latest(id[0]) != null))
                .thenExecute(() -> {
                    DisplayLinkBlockEntity link = (DisplayLinkBlockEntity) helper.getLevel().getBlockEntity(displayPos);
                    check("the display link's source is the stock link: " + link.getSourcePosition(), link.getSourcePosition().equals(linkPos));
                    link.target(signPos);
                    link.activeSource = MicRtsimCreate.DISTRICT_LEDGER.get();
                    link.updateGatheredData();
                })
                .thenWaitUntil(() -> {
                    SignBlockEntity sign = (SignBlockEntity) helper.getLevel().getBlockEntity(signPos);
                    List<String> lines = new ArrayList<>();
                    for (int i = 0; i < 4; i++)
                        lines.add(sign.getFrontText().getMessage(i, false).getString() + " | " + sign.getBackText().getMessage(i, false).getString());
                    String all = String.join(" / ", lines);
                    record("theLedgerShowsOnADisplay", "sign", all);
                    District d = ind().district(id[0]);
                    check("the sign names the district", all.contains(d.displayName()));
                    if (nativeEngine()) {
                        // The engine's own answer: a district detected this test has not been learned.
                        var line = ind().ledgers().latest(id[0]);
                        record("theLedgerShowsOnADisplay", "native ledger state", Ledgers.stateName(line.state()));
                        check("the engine's own state, " + Ledgers.stateName(line.state()), all.contains(Ledgers.stateName(line.state())));
                        check("not learned yet", line.state() == IndRecords.Ledger.STATE_LEARNING || line.state() == IndRecords.Ledger.STATE_LIVE);
                    } else {
                        check("and its state", all.contains("running (simulated)"));
                        check("and the iron per hour", all.contains("+120/h"));
                        check("and what it would have made", all.contains("would have made 500"));
                    }
                    check("the display asked for the ledger", !received(IndRecords.IND_LEDGER_REQUEST, IndRecords.DistrictState.DISTRICT, id[0]).isEmpty());
                })
                .thenSucceed();
    }

    // ---- sweep cost on large vaults (INDUSTRY.md §19 risk 2) -------------------------------------

    /**
     * Measures a sweep's cost on the largest Create vault (3 x 3 x 9 blocks, 81 x 20 = 1,620 slots, every
     * slot full) against a single vault block and a full chest. The multiblock is one boundary inventory
     * (its controller). Logs the measured numbers; it asserts only that the vault is one inventory and
     * the sampler read every slot's goods.
     */
    @GameTest(template = YARD, batch = "industry_11", timeoutTicks = 600)
    public static void sweepCostOnLargeVaults(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        ServerLevel level = helper.getLevel();
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        int y = floorY(helper);
        BlockState vault = block("create:item_vault").defaultBlockState();
        List<BlockPos> blocks = new ArrayList<>();
        for (int dx = 0; dx < 9; dx++)
            for (int dy = 0; dy < 3; dy++)
                for (int dz = 0; dz < 3; dz++) {
                    BlockPos p = new BlockPos(o.getX() + 2 + dx, y + dy, o.getZ() + 2 + dz);
                    set(helper, p, vault);
                    blocks.add(p);
                }
        BlockPos single = new BlockPos(o.getX() + 20, y, o.getZ() + 20);
        BlockPos chestPos = new BlockPos(o.getX() + 24, y, o.getZ() + 20);
        set(helper, single, vault);
        set(helper, chestPos, Blocks.CHEST.defaultBlockState());
        net.minecraft.world.item.Item[] kinds = {Items.IRON_INGOT, Items.GOLD_INGOT, Items.COPPER_INGOT, Items.COBBLESTONE, Items.DIRT,
                Items.OAK_LOG, Items.REDSTONE, Items.COAL, Items.DIAMOND, Items.EMERALD, Items.QUARTZ, Items.GRAVEL, Items.SAND,
                Items.STONE, Items.ANDESITE, Items.GRANITE, Items.DIORITE, Items.LAPIS_LAZULI, Items.BONE, Items.STRING};
        long[] id = {0};
        helper.startSequence()
                .thenIdle(5) // the vault blocks join into one multiblock on their ticks
                .thenExecute(() -> {
                    IItemHandler h = level.getCapability(Capabilities.ItemHandler.BLOCK, blocks.get(0), null);
                    int slot = 0;
                    for (int s = 0; s < h.getSlots(); s++)
                        h.insertItem(s, new ItemStack(kinds[s % kinds.length], 64), false);
                    IItemHandler hs = level.getCapability(Capabilities.ItemHandler.BLOCK, single, null);
                    for (int s = 0; s < hs.getSlots(); s++)
                        hs.insertItem(s, new ItemStack(kinds[s % kinds.length], 64), false);
                    Container c = chest(helper, chestPos);
                    for (int s = 0; s < c.getContainerSize(); s++)
                        c.setItem(s, new ItemStack(kinds[s % kinds.length], 64));
                    playerPlaced(helper, blocks.get(0));
                    playerPlaced(helper, single);
                    playerPlaced(helper, chestPos);
                    record("sweepCostOnLargeVaults", "multiblock vault slots", h.getSlots());
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, blocks.get(0));
                    check("district", d != null);
                    id[0] = d.id;
                    long vaultInvs = d.inventories.stream().filter(i -> blocks.contains(i.pos())).count();
                    check("the multiblock vault is one boundary inventory, not " + vaultInvs, vaultInvs == 1);
                })
                .thenExecute(() -> {
                    District d = ind().district(id[0]);
                    Industry ind = ind();
                    for (District.Inventory inv : d.inventories) {
                        String what = blocks.contains(inv.pos()) ? "vault 3x3x9" : inv.pos().equals(single) ? "vault 1 block" : "chest";
                        IItemHandler h = StockAccess.items(level, inv.pos());
                        int slots = h.getSlots();
                        // Warm up, then time.
                        for (int i = 0; i < 200; i++)
                            ind.sampler().peek(level, inv);
                        int reps = 2000;
                        long t0 = System.nanoTime();
                        long total = 0;
                        for (int i = 0; i < reps; i++)
                            total += ind.sampler().peek(level, inv).total();
                        long dt = System.nanoTime() - t0;
                        double perRead = dt / (double) reps;
                        record("sweepCostOnLargeVaults", what, String.format("%d slots, %.1f us per read, %.1f ns per slot, %d items counted",
                                slots, perRead / 1000.0, perRead / slots, total / reps));
                        check(what + ": every slot's goods counted", total / reps == (long) slots * 64);
                    }
                })
                .thenWaitUntil(() -> {
                    long[] last = ind().sampler().lastSweep(id[0]);
                    check("a sweep of the district finished", last != null && last[3] == 3);
                    record("sweepCostOnLargeVaults", "sampler totals", "sweeps " + ind().sampler().sweeps() + ", inventories read "
                            + ind().sampler().inventoriesRead() + ", read time " + ind().sampler().readNanos() / 1000 + " us, slowest inventory "
                            + ind().sampler().maxInventoryNanos() / 1000 + " us");
                })
                .thenSucceed();
    }
}
