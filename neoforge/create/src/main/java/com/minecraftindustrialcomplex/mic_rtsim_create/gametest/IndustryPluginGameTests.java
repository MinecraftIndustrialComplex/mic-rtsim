package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim_create.GaugeNetworks;
import com.minecraftindustrialcomplex.mic_rtsim_create.MailWatch;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.TrainWatch;
import com.minecraftindustrialcomplex.mic_rtsim_create.WritePackages;
import com.simibubi.create.Create;
import com.simibubi.create.content.logistics.box.PackageItem;
import com.simibubi.create.content.logistics.packagePort.postbox.PostboxBlockEntity;
import com.simibubi.create.content.logistics.packagerLink.LogisticsNetwork;
import com.simibubi.create.content.trains.bogey.AbstractBogeyBlock;
import com.simibubi.create.content.trains.entity.Carriage;
import com.simibubi.create.content.trains.entity.CarriageBogey;
import com.simibubi.create.content.trains.entity.Train;
import com.simibubi.create.content.trains.station.GlobalPackagePort;
import com.simibubi.create.content.trains.station.GlobalStation;
import com.simibubi.create.content.trains.station.StationBlockEntity;
import com.simibubi.create.content.trains.track.TrackBlock;
import com.simibubi.create.content.trains.track.TrackShape;
import net.minecraft.core.BlockPos;
import net.minecraft.core.GlobalPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtUtils;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;
import net.neoforged.neoforge.items.IItemHandler;
import net.neoforged.neoforge.items.ItemHandlerHelper;
import net.neoforged.neoforge.items.ItemStackHandler;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.*;

/**
 * GameTests for overnight agent 14's Create work: IND_WRITE_PACKAGE into an offline buffer and into a
 * loaded postbox, IND_PACKAGE for a loaded district with the mail out of the train's cargo port, and the
 * gauge network warning (I16). Each in its own batch; they run on either engine (choice W17).
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class IndustryPluginGameTests {
    private IndustryPluginGameTests() {}

    private static Block block(String id) {
        return BuiltInRegistries.BLOCK.get(ResourceLocation.parse(id));
    }

    private static int floorY(GameTestHelper helper) {
        return helper.absolutePos(BlockPos.ZERO).getY() + 1;
    }

    /** A position {@code dx, dz} into the first whole chunk inside the yard, so nearby blocks share a chunk. */
    private static BlockPos inChunk(GameTestHelper helper, int dx, int dz) {
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        return new BlockPos((((o.getX() >> 4) + 1) << 4) + dx, floorY(helper), (((o.getZ() >> 4) + 1) << 4) + dz);
    }

    /** The goods of every package in a handler: (good name, total), and the packages' addresses. */
    private static Map<String, Long> contents(IItemHandler h, List<String> addresses) {
        Map<String, Long> out = new HashMap<>();
        for (int s = 0; s < h.getSlots(); s++) {
            ItemStack st = h.getStackInSlot(s);
            if (!PackageItem.isPackage(st))
                continue;
            addresses.add(PackageItem.getAddress(st));
            ItemStackHandler in = PackageItem.getContents(st);
            for (int i = 0; i < in.getSlots(); i++)
                if (!in.getStackInSlot(i).isEmpty())
                    out.merge(BuiltInRegistries.ITEM.getKey(in.getStackInSlot(i).getItem()).toString(), (long) in.getStackInSlot(i).getCount(), Long::sum);
        }
        return out;
    }

    /**
     * IND_WRITE_PACKAGE (F26, §8.2) to a postbox whose chunk is not loaded: the packages go into the
     * station port's offline buffer (found through the station on a real track graph), addressed and
     * primed, answered APPLIED; the same op again is DUPLICATE; twenty more into a buffer with sixteen free
     * slots write sixteen (the rest logged), still APPLIED.
     */
    @GameTest(template = IndustryKit.YARD, batch = "industry_20", timeoutTicks = 600)
    public static void aWrittenPackageGoesIntoAnOfflineBuffer(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        int y = floorY(helper);
        BlockPos origin = helper.absolutePos(BlockPos.ZERO);
        int zr = origin.getZ() + 10;
        BlockState track = block("create:track").defaultBlockState().setValue(TrackBlock.SHAPE, TrackShape.XO);
        for (int x = origin.getX() + 2; x <= origin.getX() + 31; x++)
            helper.getLevel().setBlock(new BlockPos(x, y, zr), track, 3);
        BlockPos stationPos = new BlockPos(origin.getX() + 16, y, zr + 1);
        long farChunk = ChunkPos.asLong((origin.getX() >> 4) + 7000, (origin.getZ() >> 4) + 7000);
        BlockPos postbox = new BlockPos(ChunkPos.getX(farChunk) * 16 + 5, 70, ChunkPos.getZ(farChunk) * 16 + 5);
        GlobalPackagePort port = new GlobalPackagePort();
        port.address = "Home";
        District[] d = {null};
        long op1 = engineOp(), op2 = engineOp();
        long written0 = WritePackages.written();
        helper.startSequence()
                .thenIdle(5)
                .thenExecute(() -> {
                    helper.getLevel().setBlock(stationPos, block("create:track_station").defaultBlockState(), 3);
                    StationBlockEntity sbe = (StationBlockEntity) helper.getLevel().getBlockEntity(stationPos);
                    CompoundTag tag = sbe.saveWithFullMetadata(helper.getLevel().registryAccess());
                    tag.put("TargetTrack", NbtUtils.writeBlockPos(new BlockPos(0, 0, -1)));
                    tag.putBoolean("TargetDirection", true);
                    sbe.loadWithComponents(tag, helper.getLevel().registryAccess());
                })
                .thenWaitUntil(() -> {
                    StationBlockEntity sbe = (StationBlockEntity) helper.getLevel().getBlockEntity(stationPos);
                    check("the station joined the track graph", sbe != null && sbe.getStation() != null);
                })
                .thenExecute(() -> {
                    GlobalStation st = ((StationBlockEntity) helper.getLevel().getBlockEntity(stationPos)).getStation();
                    st.connectedPorts.put(postbox, port);
                    d[0] = ind().districts().createForTest(helper.getLevel(), farChunk);
                    writePackage(op1, d[0].id, 0, postbox, "Home", 2, new LinkedHashMap<>(Map.of("i/64/minecraft:iron_ingot", 16L, "i/64/minecraft:coal", 4L)));
                })
                .thenWaitUntil(() -> check("applied", commandResults(op1).contains(Obs.CommandResult.APPLIED)))
                .thenExecute(() -> {
                    List<String> addresses = new ArrayList<>();
                    Map<String, Long> got = contents(port.offlineBuffer, addresses);
                    record("aWrittenPackageGoesIntoAnOfflineBuffer", "buffer after op 1 (" + engineName() + ")", got + ", addresses " + addresses + ", primed " + port.primed);
                    check("two packages", addresses.size() == 2);
                    check("addressed Home", addresses.stream().allMatch("Home"::equals));
                    check("32 iron and 8 coal in all: " + got, got.getOrDefault("minecraft:iron_ingot", 0L) == 32 && got.getOrDefault("minecraft:coal", 0L) == 8);
                    check("primed, so the postbox takes them at its load", port.primed);
                    writePackage(op1, d[0].id, 0, postbox, "Home", 2, Map.of("i/64/minecraft:iron_ingot", 16L));
                })
                .thenWaitUntil(() -> check("the same op again is a duplicate", commandResults(op1).contains(Obs.CommandResult.DUPLICATE_OP)))
                .thenExecute(() -> {
                    List<String> addresses = new ArrayList<>();
                    contents(port.offlineBuffer, addresses);
                    check("still two packages", addresses.size() == 2);
                    writePackage(op2, d[0].id, 0, postbox, "Home", 20, Map.of("i/64/minecraft:diamond", 1L));
                })
                .thenWaitUntil(() -> check("op 2 applied", commandResults(op2).contains(Obs.CommandResult.APPLIED)))
                .thenExecute(() -> {
                    List<String> addresses = new ArrayList<>();
                    Map<String, Long> got = contents(port.offlineBuffer, addresses);
                    record("aWrittenPackageGoesIntoAnOfflineBuffer", "buffer after op 2", addresses.size() + " packages, " + got
                            + ", written " + (WritePackages.written() - written0) + ", overflow total " + WritePackages.overflow());
                    check("the buffer is full: 18 packages", addresses.size() == 18);
                    check("16 diamonds fitted", got.getOrDefault("minecraft:diamond", 0L) == 16);
                })
                .thenSucceed();
    }

    /**
     * IND_WRITE_PACKAGE to a postbox in a loaded chunk: the package goes into the postbox's own inventory
     * (as Create's mail would), its chunk carries the COMMAND_RESULT until the engine acknowledges it, and
     * the op again is DUPLICATE.
     */
    @GameTest(template = IndustryKit.YARD, batch = "industry_21", timeoutTicks = 400)
    public static void aWrittenPackageGoesIntoALoadedPostbox(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        BlockPos postbox = inChunk(helper, 4, 4);
        BlockPos chest = inChunk(helper, 7, 4);
        long op = engineOp();
        long[] id = {0};
        boolean[] carried = {false};
        helper.startSequence()
                .thenExecute(() -> {
                    helper.getLevel().setBlock(postbox, block("create:white_postbox").defaultBlockState(), 3);
                    helper.getLevel().setBlock(chest, Blocks.CHEST.defaultBlockState(), 3);
                    playerPlaced(helper, chest);
                    playerPlaced(helper, postbox);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, postbox);
                    check("the postbox's district", d != null && d.inventoryAt(postbox) != null);
                    id[0] = d.id;
                })
                .thenExecute(() -> writePackage(op, id[0], ind().district(id[0]).inventoryAt(postbox).index(), postbox, "Mill", 1,
                        Map.of("i/64/minecraft:copper_ingot", 30L)))
                .thenWaitUntil(() -> {
                    check("applied", commandResults(op).contains(Obs.CommandResult.APPLIED));
                    LevelChunk c = helper.getLevel().getChunkAt(postbox);
                    carried[0] = carried[0] || ind().chunkCarriesOp(c, op);
                })
                .thenExecute(() -> {
                    PostboxBlockEntity pb = (PostboxBlockEntity) helper.getLevel().getBlockEntity(postbox);
                    List<String> addresses = new ArrayList<>();
                    Map<String, Long> got = contents(pb.inventory, addresses);
                    record("aWrittenPackageGoesIntoALoadedPostbox", "postbox (" + engineName() + ")", got + ", addresses " + addresses
                            + ", chunk carried the result " + carried[0]);
                    check("one package of 30 copper to Mill", addresses.equals(List.of("Mill")) && got.getOrDefault("minecraft:copper_ingot", 0L) == 30);
                    check("the chunk carried the COMMAND_RESULT", carried[0]);
                    writePackage(op, id[0], 0, postbox, "Mill", 1, Map.of("i/64/minecraft:copper_ingot", 30L));
                })
                .thenWaitUntil(() -> check("the op again is a duplicate", commandResults(op).contains(Obs.CommandResult.DUPLICATE_OP)))
                .thenExecute(() -> {
                    PostboxBlockEntity pb = (PostboxBlockEntity) helper.getLevel().getBlockEntity(postbox);
                    List<String> addresses = new ArrayList<>();
                    contents(pb.inventory, addresses);
                    check("still one package", addresses.size() == 1);
                })
                .thenSucceed();
    }

    private static Train syntheticTrain(GameTestHelper helper, BlockPos vaultPos, int iron) {
        ServerLevel level = helper.getLevel();
        level.setBlock(vaultPos, block("create:item_vault").defaultBlockState(), 3);
        IItemHandler vh = level.getCapability(Capabilities.ItemHandler.BLOCK, vaultPos, null);
        ItemHandlerHelper.insertItemStacked(vh, new ItemStack(Items.IRON_INGOT, iron), false);
        AbstractBogeyBlock<?> bogeyBlock = (AbstractBogeyBlock<?>) block("create:small_bogey");
        Carriage carriage = new Carriage(new CarriageBogey(bogeyBlock, false, new CompoundTag()),
                new CarriageBogey(bogeyBlock, false, new CompoundTag()), 2);
        carriage.storage.addBlock(level, level.getBlockState(vaultPos), BlockPos.ZERO, vaultPos, level.getBlockEntity(vaultPos));
        carriage.storage.initialize();
        level.setBlock(vaultPos, Blocks.AIR.defaultBlockState(), 3);
        return new Train(UUID.randomUUID(), null, null, new ArrayList<>(List.of(carriage)), new ArrayList<>(), false, 0);
    }

    /**
     * Mail at a loaded district (choice Q6, F26): a package the station's mail moves from a train into a
     * loaded district's postbox is an IND_PACKAGE (in, 10 iron) for the postbox's district, and is not
     * also in the train's cargo port at departure. (The synthetic train has no track graph, so its station
     * has no present train for Create's own transfer: the test moves the package and tells TrainWatch, as
     * MailWatch does for a real train.)
     */
    @GameTest(template = IndustryKit.YARD, batch = "industry_22", timeoutTicks = 400)
    public static void mailAtALoadedDistrictIsAPackageNotACargoPort(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        BlockPos postbox = inChunk(helper, 4, 4);
        BlockPos chest = inChunk(helper, 7, 4);
        long[] id = {0};
        Train[] train = {null};
        GlobalStation st = new GlobalStation();
        st.id = UUID.randomUUID();
        st.name = "Mail Halt";
        st.blockEntityPos = inChunk(helper, 4, 8);
        st.blockEntityDimension = helper.getLevel().dimension();
        GlobalPackagePort port = new GlobalPackagePort();
        port.address = "Home";
        st.connectedPorts.put(postbox, port);
        helper.startSequence()
                .thenExecute(() -> {
                    helper.getLevel().setBlock(postbox, block("create:white_postbox").defaultBlockState(), 3);
                    helper.getLevel().setBlock(chest, Blocks.CHEST.defaultBlockState(), 3);
                    playerPlaced(helper, chest);
                    playerPlaced(helper, postbox);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, postbox);
                    check("the district is loaded", d != null && d.loadState() == IndRecords.LoadState.LIVE);
                    id[0] = d.id;
                })
                .thenExecute(() -> {
                    train[0] = syntheticTrain(helper, inChunk(helper, 10, 10), 32);
                    train[0].arriveAt(st);
                    Industry ind = ind();
                    MailWatch.before(st);
                    ItemStack taken = train[0].carriages.get(0).storage.getAllItems().extractItem(0, 10, false);
                    ItemStack box = PackageItem.containing(List.of(taken));
                    PackageItem.addAddress(box, "Home");
                    PostboxBlockEntity pb = (PostboxBlockEntity) helper.getLevel().getBlockEntity(postbox);
                    pb.inventory.setStackInSlot(0, box);
                    MailWatch.after(st);
                    Industry.GoodsCounter moved = ind.counter();
                    ind.countStack(box, 1, moved);
                    TrainWatch.mailMoved(train[0], moved);
                    train[0].leaveStation();
                })
                .thenWaitUntil(() -> {
                    List<ByteBuffer> pk = received(IndRecords.IND_PACKAGE, IndRecords.Package.DISTRICT, id[0]);
                    check("an IND_PACKAGE for the loaded district", pk.size() == 1);
                    ByteBuffer in = pk.get(0);
                    check("in", in.getInt(IndRecords.Package.DIRECTION) == IndRecords.Package.IN);
                    check("10 iron", aux(in) == 1 && "i/64/minecraft:iron_ingot".equals(javaName(in.getInt(IndRecords.Package.ENTRIES)))
                            && in.getLong(IndRecords.Package.ENTRIES + 8) == 10);
                    check("addressed Home", "Home".equals(javaName(in.getInt(IndRecords.Package.ADDRESS))));
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    long tid = com.minecraftindustrialcomplex.mic_rtsim_create.CreateIds.of(train[0].id);
                    long iron = 0;
                    for (ByteBuffer p : received(IndRecords.IND_PORT, p -> p.getLong(IndRecords.Port.DISTRICT) == id[0]
                            && p.getInt(IndRecords.Port.PORT_KIND) == IndRecords.Port.TRAIN_CARGO && p.getLong(IndRecords.Port.COUNTERPART) == tid))
                        for (int i = 0; i < aux(p); i++)
                            if ("i/64/minecraft:iron_ingot".equals(javaName(p.getInt(IndRecords.Port.ENTRIES + i * IndRecords.ENTRY16))))
                                iron += p.getLong(IndRecords.Port.ENTRIES + i * IndRecords.ENTRY16 + 8);
                    record("mailAtALoadedDistrictIsAPackageNotACargoPort", "iron in the train's cargo port (" + engineName() + ")", iron);
                    check("the mail is not in the cargo port too: " + iron, iron == 0);
                    TrainWatch.forget(train[0]);
                })
                .thenSucceed();
    }

    /**
     * I16, warn first: a logistics network with links in two districts puts a diagnostics warning on both districts
     * naming the other; with one district left on it the warnings go.
     */
    @GameTest(template = IndustryKit.YARD, batch = "industry_23", timeoutTicks = 400)
    public static void aGaugeNetworkAcrossDistrictsIsWarned(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        ServerLevel level = helper.getLevel();
        BlockPos origin = helper.absolutePos(BlockPos.ZERO);
        long homeChunk = ChunkPos.asLong((origin.getX() >> 4) + 8000, (origin.getZ() >> 4));
        long coastChunk = ChunkPos.asLong((origin.getX() >> 4) + 8100, (origin.getZ() >> 4));
        UUID netId = UUID.randomUUID();
        District[] d = {null, null};
        helper.startSequence()
                .thenExecute(() -> {
                    d[0] = ind().districts().createForTest(level, homeChunk);
                    d[1] = ind().districts().createForTest(level, coastChunk);
                    LogisticsNetwork net = new LogisticsNetwork(netId);
                    net.totalLinks.add(GlobalPos.of(level.dimension(), new BlockPos(ChunkPos.getX(homeChunk) * 16 + 1, 64, ChunkPos.getZ(homeChunk) * 16 + 1)));
                    net.totalLinks.add(GlobalPos.of(level.dimension(), new BlockPos(ChunkPos.getX(coastChunk) * 16 + 1, 64, ChunkPos.getZ(coastChunk) * 16 + 1)));
                    Create.LOGISTICS.logisticsNetworks.put(netId, net);
                    Map<UUID, Set<Long>> spans = GaugeNetworks.check(ind());
                    check("the network spans the two districts", spans.containsKey(netId) && spans.get(netId).equals(Set.of(d[0].id, d[1].id)));
                    List<String> w0 = ind().diagnostics().warnings(d[0].id), w1 = ind().diagnostics().warnings(d[1].id);
                    record("aGaugeNetworkAcrossDistrictsIsWarned", "warnings", w0 + " / " + w1);
                    check("both districts warn", w0.size() == 1 && w1.size() == 1);
                    check("naming the other district", w0.get(0).contains(d[1].displayName()) && w1.get(0).contains(d[0].displayName()));
                    net.totalLinks.removeIf(gp -> (gp.pos().getX() >> 4) == ChunkPos.getX(coastChunk));
                    GaugeNetworks.check(ind());
                    check("one district left: no warning", ind().diagnostics().warnings(d[0].id).isEmpty() && ind().diagnostics().warnings(d[1].id).isEmpty());
                    Create.LOGISTICS.logisticsNetworks.remove(netId);
                })
                .thenSucceed();
    }
}
