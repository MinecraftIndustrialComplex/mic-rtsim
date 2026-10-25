package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.journal.Journal;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.PortRing;
import com.simibubi.create.content.logistics.box.PackageItem;
import com.simibubi.create.content.logistics.packagePort.postbox.PostboxBlockEntity;
import com.simibubi.create.content.trains.graph.DimensionPalette;
import com.simibubi.create.content.trains.station.GlobalPackagePort;
import com.simibubi.create.content.trains.station.GlobalStation;
import com.simibubi.create.content.trains.station.StationBlockEntity;
import com.simibubi.create.content.trains.track.TrackBlock;
import com.simibubi.create.content.trains.track.TrackShape;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.NbtUtils;
import net.minecraft.nbt.Tag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;
import net.neoforged.neoforge.attachment.AttachmentHolder;
import net.neoforged.neoforge.items.IItemHandler;
import net.neoforged.neoforge.items.ItemStackHandler;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.lang.ref.WeakReference;
import java.util.Arrays;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.*;

/**
 * The station port's rings (docs/choices-journal-barrier.md RP-206): IND_WRITE_PACKAGE's dedup across a crash that
 * kept the tracks file (create_tracks.dat, which holds the port's offline buffer) and not the journal's, or the
 * other way round.
 *
 * <p>A crash is emulated on one real station of a real track graph. Its tracks file is what EdgePointStorage.write
 * writes for it (Create's own {@code GlobalStation.write}, with a dimension palette, through the bytes of a
 * compressed NBT file) and the restart is Create's {@code GlobalStation.read} of those bytes into the station,
 * which makes its ports anew. The journal's file is its op index ({@link Journal#forgetAppliedForTest},
 * {@link Journal#rememberAppliedForTest}); a postbox chunk from before the write is the postbox emptied and the
 * chunk's carried result dropped; a postbox chunk from after the tracks file is the loaded chunk kept as it is, or,
 * for a postbox not loaded, a chunk tag written through the level's chunk storage, which the chunk read then finds.
 * The engine then sends the op again. Each crash is at the data level, so no test depends on the order the server
 * writes its files in.
 *
 * <p>Every test has its own batch: the fallback switch is server-wide.
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class PortRingGameTests {
    private PortRingGameTests() {}

    private static final String ADDRESS = "Home";

    // ---- the four durability cases on the offline buffer (K2) -------------------------------------

    /** Both files saved: the buffer's ring holds the op and answers DUPLICATE; one package. */
    @GameTest(template = IndustryKit.YARD, batch = "port_ring_1", timeoutTicks = 1200)
    public static void aCrashThatKeptBothFilesWritesAPackageOnce(GameTestHelper helper) {
        crash(helper, "aCrashThatKeptBothFilesWritesAPackageOnce", true, true, true);
    }

    /** Neither saved: the port and its ring are from before the op, which is written; one package. */
    @GameTest(template = IndustryKit.YARD, batch = "port_ring_2", timeoutTicks = 1200)
    public static void aCrashThatKeptNeitherFileWritesAPackageOnce(GameTestHelper helper) {
        crash(helper, "aCrashThatKeptNeitherFileWritesAPackageOnce", true, false, false);
    }

    /** The tracks saved and not the journal (the crash slice U's write order leaves): the ring came back with the package. */
    @GameTest(template = IndustryKit.YARD, batch = "port_ring_3", timeoutTicks = 1200)
    public static void aCrashThatKeptTheBufferAndNotTheJournalWritesAPackageOnce(GameTestHelper helper) {
        crash(helper, "aCrashThatKeptTheBufferAndNotTheJournalWritesAPackageOnce", true, true, false);
    }

    /** The journal saved and not the tracks (a failed tracks write): the ring alone decides, and the package is written again. */
    @GameTest(template = IndustryKit.YARD, batch = "port_ring_4", timeoutTicks = 1200)
    public static void aCrashThatKeptTheJournalAndNotTheBufferWritesAPackageOnce(GameTestHelper helper) {
        crash(helper, "aCrashThatKeptTheJournalAndNotTheBufferWritesAPackageOnce", true, false, true);
    }

    /**
     * The rings switched off as if their mixins had not applied: the fallback is logged once and the dedup is the
     * journal's applied set. The tracks saved and not the journal is then K2's dupe: two packages for one op.
     */
    @GameTest(template = IndustryKit.YARD, batch = "port_ring_5", timeoutTicks = 1200)
    public static void withoutTheRingsACrashThatKeptTheBufferAndNotTheJournalWritesAPackageTwice(GameTestHelper helper) {
        crash(helper, "withoutTheRingsACrashThatKeptTheBufferAndNotTheJournalWritesAPackageTwice", false, true, false);
    }

    /**
     * One crash: a port whose postbox is in an unloaded chunk, one package written into its offline buffer, the tracks
     * file from after the write ({@code tracksSaved}) or before it, the journal knowing the op ({@code journalSaved})
     * or not, and the op sent again. The buffer then holds one package when the dedup knew the op exactly when the
     * tracks were saved: always with the rings, never on the journal when the two files parted.
     */
    private static void crash(GameTestHelper helper, String test, boolean rings, boolean tracksSaved, boolean journalSaved) {
        fastConfig();
        forgetAll(helper);
        IndustryKit.forget(helper);
        ServerLevel level = helper.getLevel();
        BlockPos stationPos = stationPos(helper, 10);
        long farChunk = ChunkPos.asLong((stationPos.getX() >> 4) + 7000, (stationPos.getZ() >> 4) + 7000);
        BlockPos postbox = new BlockPos(ChunkPos.getX(farChunk) * 16 + 5, 70, ChunkPos.getZ(farChunk) * 16 + 5);
        GlobalStation[] st = {null};
        District[] d = {null};
        CompoundTag[] before = {null}, after = {null};
        int[] logs = {0};
        long op = engineOp();
        String dedup = rings ? "rings" : "journal";
        String kase = (tracksSaved ? "tracks" : "no tracks") + ", " + (journalSaved ? "journal" : "no journal");
        helper.startSequence()
                .thenIdle(5)
                .thenExecute(() -> {
                    placeStation(helper, 10);
                    PortRing.fallbackForTest(!rings);
                    logs[0] = PortRing.fallbackLogsForTest();
                    check("the rings are " + (rings ? "on: their mixins applied and the probe passed" : "switched off"),
                            PortRing.available(level.getServer()) == rings);
                })
                .thenWaitUntil(() -> st[0] = joined(helper, stationPos))
                .thenExecute(() -> {
                    GlobalPackagePort port = new GlobalPackagePort();
                    port.address = ADDRESS;
                    st[0].connectedPorts.put(postbox, port);
                    d[0] = ind().districts().createForTest(level, farChunk);
                    before[0] = saveStation(level, st[0]);
                    writePackage(op, d[0].id, 0, postbox, ADDRESS, 1, Map.of("i/64/minecraft:iron_ingot", 16L));
                })
                .thenWaitUntil(() -> check("applied", commandResults(op).equals(List.of(Obs.CommandResult.APPLIED))))
                .thenExecute(() -> {
                    GlobalPackagePort port = st[0].connectedPorts.get(postbox);
                    check("one package in the buffer, primed", packages(port.offlineBuffer) == 1 && port.primed);
                    after[0] = saveStation(level, st[0]);
                    CompoundTag entry = portEntry(after[0]);
                    if (rings)
                        check("the tracks save holds the op in the port's buffer ring: " + Arrays.toString(entry.getLongArray(PortRing.KEY)),
                                Arrays.equals(entry.getLongArray(PortRing.KEY), new long[] {op}) && !entry.contains(PortRing.POSTBOX_KEY));
                    else
                        check("no ring is saved when they are off", !entry.contains(PortRing.KEY) && !entry.contains(PortRing.POSTBOX_KEY));
                    // The crash and the restart: Create reads its tracks file, the journal its own.
                    if (journalSaved)
                        journal().rememberAppliedForTest(op);
                    else
                        journal().forgetAppliedForTest(op);
                    check("the journal " + (journalSaved ? "knows" : "does not know") + " the op", ind().opApplied(op) == journalSaved);
                    loadStation(level, st[0], tracksSaved ? after[0] : before[0]);
                    GlobalPackagePort back = st[0].connectedPorts.get(postbox);
                    check("the port came back: a new object", back != null && back != port);
                    check("with " + (tracksSaved ? 1 : 0) + " packages", packages(back.offlineBuffer) == (tracksSaved ? 1 : 0));
                    writePackage(op, d[0].id, 0, postbox, ADDRESS, 1, Map.of("i/64/minecraft:iron_ingot", 16L));
                })
                .thenWaitUntil(() -> check("the resend is answered", commandResults(op).size() == 2))
                .thenExecute(() -> {
                    int second = commandResults(op).get(1);
                    int got = packages(st[0].connectedPorts.get(postbox).offlineBuffer);
                    record(test, kase + " on the " + dedup + ": second answer, packages in the buffer",
                            status(second) + ", " + got + (got == 1 ? " (exact)" : got > 1 ? " (a dupe)" : " (a loss)"));
                    boolean knows = rings ? tracksSaved : journalSaved;
                    check("the " + dedup + " " + (knows ? "know the op: DUPLICATE" : "lack the op: APPLIED") + ", answered " + status(second),
                            second == (knows ? Obs.CommandResult.DUPLICATE_OP : Obs.CommandResult.APPLIED));
                    int expected = (tracksSaved ? 1 : 0) + (knows ? 0 : 1);
                    check("packages in the buffer " + expected + ": " + got, got == expected);
                    if (rings)
                        check("one package for one op", got == 1);
                    else
                        check("the fallback was logged once: " + (PortRing.fallbackLogsForTest() - logs[0]), PortRing.fallbackLogsForTest() == logs[0] + 1);
                    journal().forgetAppliedForTest(op);
                    st[0].connectedPorts.remove(postbox);
                    PortRing.fallbackForTest(false);
                })
                .thenSucceed();
    }

    // ---- a loaded postbox (K3) ---------------------------------------------------------------------

    /**
     * A package written into a loaded postbox goes in the port's postbox ring. A crash that kept the tracks file (with
     * that ring, and Create's unprimed mirror of the postbox) and lost the postbox's chunk and the journal: the resend
     * finds the postbox loaded and empty, and writes the package again, once (the postbox ring is not the loaded path's
     * dedup). With the postbox no longer there, the resend goes to the offline path, which the postbox ring refuses:
     * nothing goes into the buffer, which is not primed.
     */
    @GameTest(template = IndustryKit.YARD, batch = "port_ring_6", timeoutTicks = 1200)
    public static void aLoadedPostboxWhoseChunkWasOlderThanTheTracksTakesThePackageOnce(GameTestHelper helper) {
        String test = "aLoadedPostboxWhoseChunkWasOlderThanTheTracksTakesThePackageOnce";
        fastConfig();
        forgetAll(helper);
        IndustryKit.forget(helper);
        ServerLevel level = helper.getLevel();
        PortRing.fallbackForTest(false);
        BlockPos postbox = inChunk(helper, 4, 4);
        BlockPos chest = inChunk(helper, 7, 4);
        BlockPos stationPos = stationPos(helper, 28);
        GlobalStation[] st = {null};
        long[] id = {0};
        int[] mirrored = {0};
        long op = engineOp();
        Map<String, Long> copper = Map.of("i/64/minecraft:copper_ingot", 30L);
        helper.startSequence()
                .thenIdle(5)
                .thenExecute(() -> {
                    placeStation(helper, 28);
                    level.setBlock(postbox, block("create:white_postbox").defaultBlockState(), 3);
                    level.setBlock(chest, Blocks.CHEST.defaultBlockState(), 3);
                    // Create's kit: a chest and a postbox seed no district alone (DS-10), so it adds the marker machine.
                    IndustryKit.playerPlaced(helper, chest);
                    IndustryKit.playerPlaced(helper, postbox);
                })
                .thenWaitUntil(() -> {
                    District d = IndustryKit.districtAt(helper, postbox);
                    check("the postbox's district", d != null && d.inventoryAt(postbox) != null);
                    id[0] = d.id;
                    st[0] = joined(helper, stationPos);
                })
                .thenExecute(() -> {
                    check("the rings are on", PortRing.available(level.getServer()));
                    GlobalPackagePort port = new GlobalPackagePort();
                    port.address = "Mill";
                    st[0].connectedPorts.put(postbox, port);
                    ((PostboxBlockEntity) level.getBlockEntity(postbox)).trackedGlobalStation = new WeakReference<>(st[0]);
                    writePackage(op, id[0], ind().district(id[0]).inventoryAt(postbox).index(), postbox, "Mill", 1, copper);
                })
                .thenWaitUntil(() -> check("applied", commandResults(op).equals(List.of(Obs.CommandResult.APPLIED))))
                .thenExecute(() -> {
                    PostboxBlockEntity pb = (PostboxBlockEntity) level.getBlockEntity(postbox);
                    GlobalPackagePort port = st[0].connectedPorts.get(postbox);
                    check("one package in the postbox", packages(pb.inventory) == 1);
                    check("Create mirrored it into the buffer, unprimed", packages(port.offlineBuffer) == 1 && !port.primed);
                    check("the op is in the postbox ring, not the buffer's", PortRing.wroteToPostbox(port, op) && !PortRing.wroteToBuffer(port, op));
                    CompoundTag tracks = saveStation(level, st[0]);
                    check("the tracks save holds it", Arrays.equals(portEntry(tracks).getLongArray(PortRing.POSTBOX_KEY), new long[] {op}));
                    // The crash: the tracks file is this one; the postbox's chunk and the journal are from before the write.
                    for (int s = 0; s < pb.inventory.getSlots(); s++)
                        pb.inventory.setStackInSlot(s, ItemStack.EMPTY);
                    dropCarried(level.getChunkAt(postbox), op);
                    journal().forgetAppliedForTest(op);
                    loadStation(level, st[0], tracks);
                    GlobalPackagePort back = st[0].connectedPorts.get(postbox);
                    check("the port came back with the mirror and the ring", back != port && packages(back.offlineBuffer) == 1
                            && !back.primed && PortRing.wroteToPostbox(back, op));
                    check("the chunk does not carry the op", !ind().chunkCarriesOp(level.getChunkAt(postbox), op));
                    writePackage(op, id[0], ind().district(id[0]).inventoryAt(postbox).index(), postbox, "Mill", 1, copper);
                })
                .thenWaitUntil(() -> check("the resend is answered", commandResults(op).size() == 2))
                .thenExecute(() -> {
                    int second = commandResults(op).get(1);
                    int got = packages(((PostboxBlockEntity) level.getBlockEntity(postbox)).inventory);
                    record(test, "loaded, chunk older than the tracks: second answer, packages in the postbox", status(second) + ", " + got);
                    check("written again into the loaded postbox: APPLIED, answered " + status(second), second == Obs.CommandResult.APPLIED);
                    check("one package: " + got, got == 1);
                    GlobalPackagePort port = st[0].connectedPorts.get(postbox);
                    // The postbox gone, so the next resend takes the offline path.
                    PostboxBlockEntity pb = (PostboxBlockEntity) level.getBlockEntity(postbox);
                    for (int s = 0; s < pb.inventory.getSlots(); s++)
                        pb.inventory.setStackInSlot(s, ItemStack.EMPTY);
                    level.setBlock(postbox, Blocks.AIR.defaultBlockState(), 3);
                    check("no postbox there", !(level.getBlockEntity(postbox) instanceof PostboxBlockEntity));
                    mirrored[0] = packages(port.offlineBuffer);
                    journal().forgetAppliedForTest(op);
                    writePackage(op, id[0], 0, postbox, "Mill", 1, copper);
                })
                .thenWaitUntil(() -> check("the offline resend is answered", commandResults(op).size() == 3))
                .thenExecute(() -> {
                    int third = commandResults(op).get(2);
                    GlobalPackagePort port = st[0].connectedPorts.get(postbox);
                    int got = packages(port.offlineBuffer);
                    record(test, "offline after a loaded write: third answer, packages in the buffer, primed", status(third) + ", " + got + ", " + port.primed);
                    check("the postbox ring refuses it: DUPLICATE, answered " + status(third), third == Obs.CommandResult.DUPLICATE_OP);
                    check("nothing written into the buffer: " + got + " vs " + mirrored[0], got == mirrored[0] && !port.primed);
                    st[0].connectedPorts.remove(postbox);
                })
                .thenSucceed();
    }

    // ---- the postbox's chunk newer than the tracks file (P1 of the slice U review) ------------------

    /**
     * A primed buffer restored into its postbox, whose chunk then reached disk before the tracks file and the journal:
     * the chunk carries the op from the restore, and the resend with the postbox loaded is refused.
     */
    @GameTest(template = IndustryKit.YARD, batch = "port_ring_8", timeoutTicks = 1200)
    public static void aRestoredBufferWhoseChunkOutlivedTheTracksTakesAPackageOnce(GameTestHelper helper) {
        restored(helper, "aRestoredBufferWhoseChunkOutlivedTheTracksTakesAPackageOnce", true);
    }

    /** The same without the restore's carry: the resend writes the package a second time. */
    @GameTest(template = IndustryKit.YARD, batch = "port_ring_9", timeoutTicks = 1200)
    public static void withoutTheChunkChecksARestoredBufferWhoseChunkOutlivedTheTracksTakesAPackageTwice(GameTestHelper helper) {
        restored(helper, "withoutTheChunkChecksARestoredBufferWhoseChunkOutlivedTheTracksTakesAPackageTwice", false);
    }

    /**
     * An unloaded postbox whose chunk reached disk after the tracks file: it took package P of op W back from the
     * buffer at an earlier load (so it carries W), and package Q, which the tracks file's unprimed copy still holds,
     * was taken out since. After the crash (the tracks file and the journal from before W) the engine sends W again
     * and then a new op W2, the postbox still unloaded. The first offline write of the life reads the chunk: W is
     * refused, and the buffer takes the postbox's inventory, so W2 primes P and P2 and not Q.
     */
    @GameTest(template = IndustryKit.YARD, batch = "port_ring_10", timeoutTicks = 1200)
    public static void anUnloadedPostboxWhoseChunkOutlivedTheTracksTakesEachPackageOnce(GameTestHelper helper) {
        unloaded(helper, "anUnloadedPostboxWhoseChunkOutlivedTheTracksTakesEachPackageOnce", true);
    }

    /** The same without the chunk read: W is written again, and the primed buffer puts Q back at the postbox's load. */
    @GameTest(template = IndustryKit.YARD, batch = "port_ring_11", timeoutTicks = 1200)
    public static void withoutTheChunkChecksAnUnloadedPostboxWhoseChunkOutlivedTheTracksTakesPackagesTwice(GameTestHelper helper) {
        unloaded(helper, "withoutTheChunkChecksAnUnloadedPostboxWhoseChunkOutlivedTheTracksTakesPackagesTwice", false);
    }

    private static void restored(GameTestHelper helper, String test, boolean checks) {
        fastConfig();
        forgetAll(helper);
        IndustryKit.forget(helper);
        ServerLevel level = helper.getLevel();
        PortRing.fallbackForTest(false);
        PortRing.chunkChecksForTest(!checks);
        BlockPos postbox = inChunk(helper, 4, 4);
        BlockPos stationPos = stationPos(helper, 28);
        GlobalStation[] st = {null};
        CompoundTag[] before = {null};
        long[] carried = {0};
        long op = engineOp();
        Map<String, Long> copper = Map.of("i/64/minecraft:copper_ingot", 30L);
        helper.startSequence()
                .thenIdle(5)
                .thenExecute(() -> placeStation(helper, 28))
                .thenWaitUntil(() -> st[0] = joined(helper, stationPos))
                .thenExecute(() -> {
                    check("the rings are on", PortRing.available(level.getServer()));
                    check("no postbox there yet", !(level.getBlockEntity(postbox) instanceof PostboxBlockEntity));
                    GlobalPackagePort port = new GlobalPackagePort();
                    port.address = "Mill";
                    st[0].connectedPorts.put(postbox, port);
                    before[0] = saveStation(level, st[0]);
                    carried[0] = PortRing.chunkCountsForTest()[0];
                    writePackage(op, 0, 0, postbox, "Mill", 1, copper);
                })
                .thenWaitUntil(() -> check("applied offline", commandResults(op).equals(List.of(Obs.CommandResult.APPLIED))))
                .thenExecute(() -> {
                    GlobalPackagePort port = st[0].connectedPorts.get(postbox);
                    check("one package in the buffer, primed, the op in its ring",
                            packages(port.offlineBuffer) == 1 && port.primed && PortRing.wroteToBuffer(port, op));
                    check("the engine has not acknowledged it", ind().opApplied(op));
                    // The postbox is placed and registers with its station: Create restores the buffer into it.
                    level.setBlock(postbox, block("create:white_postbox").defaultBlockState(), 3);
                    PostboxBlockEntity pb = (PostboxBlockEntity) level.getBlockEntity(postbox);
                    ((StationBlockEntity) level.getBlockEntity(stationPos)).attachPackagePort(pb);
                    check("restored: one package in the postbox, the buffer unprimed", packages(pb.inventory) == 1 && !port.primed);
                    boolean carries = ind().chunkCarriesOp(level.getChunkAt(postbox), op);
                    check(checks ? "the chunk carries the op" : "the chunk does not carry the op", carries == checks);
                    check("carried at the restore", PortRing.chunkCountsForTest()[0] - carried[0] == (checks ? 1 : 0));
                    // The crash: the postbox's chunk reached disk after the restore; the tracks file and the journal are from before the write.
                    journal().forgetAppliedForTest(op);
                    loadStation(level, st[0], before[0]);
                    GlobalPackagePort back = st[0].connectedPorts.get(postbox);
                    check("the port came back from before the write", back != port && packages(back.offlineBuffer) == 0
                            && !back.primed && !PortRing.wroteToBuffer(back, op));
                    writePackage(op, 0, 0, postbox, "Mill", 1, copper);
                })
                .thenWaitUntil(() -> check("the resend is answered", commandResults(op).size() == 2))
                .thenExecute(() -> {
                    int second = commandResults(op).get(1);
                    PostboxBlockEntity pb = (PostboxBlockEntity) level.getBlockEntity(postbox);
                    int got = packages(pb.inventory);
                    record(test, "restored, the chunk newer than the tracks and the journal, loaded resend: answer, packages in the postbox",
                            status(second) + ", " + got + (got == 1 ? " (exact)" : " (a dupe)"));
                    if (checks) {
                        check("the chunk refuses it: DUPLICATE, answered " + status(second), second == Obs.CommandResult.DUPLICATE_OP);
                        check("one package: " + got, got == 1);
                    } else {
                        check("written again: APPLIED, answered " + status(second), second == Obs.CommandResult.APPLIED);
                        check("two packages, a dupe: " + got, got == 2);
                    }
                    for (int s = 0; s < pb.inventory.getSlots(); s++)
                        pb.inventory.setStackInSlot(s, ItemStack.EMPTY);
                    dropCarried(level.getChunkAt(postbox), op);
                    level.setBlock(postbox, Blocks.AIR.defaultBlockState(), 3);
                    journal().forgetAppliedForTest(op);
                    st[0].connectedPorts.remove(postbox);
                    PortRing.chunkChecksForTest(false);
                })
                .thenSucceed();
    }

    private static void unloaded(GameTestHelper helper, String test, boolean checks) {
        fastConfig();
        forgetAll(helper);
        IndustryKit.forget(helper);
        ServerLevel level = helper.getLevel();
        PortRing.fallbackForTest(false);
        PortRing.chunkChecksForTest(!checks);
        BlockPos stationPos = stationPos(helper, 10);
        ChunkPos far = new ChunkPos((stationPos.getX() >> 4) + 7100 + (checks ? 0 : 4), (stationPos.getZ() >> 4) + 7100);
        BlockPos postbox = new BlockPos(far.getMinBlockX() + 5, 70, far.getMinBlockZ() + 5);
        GlobalStation[] st = {null};
        long[] counts = {0, 0, 0, 0, 0};
        long w = engineOp(), w2 = engineOp();
        Map<String, Long> copper = Map.of("i/64/minecraft:copper_ingot", 30L);
        Map<String, Long> gold = Map.of("i/64/minecraft:gold_ingot", 5L);
        helper.startSequence()
                .thenIdle(5)
                .thenExecute(() -> placeStation(helper, 10))
                .thenWaitUntil(() -> st[0] = joined(helper, stationPos))
                .thenExecute(() -> {
                    check("the rings are on", PortRing.available(level.getServer()));
                    check("the postbox's chunk is not loaded", level.getChunkSource().getChunkNow(far.x, far.z) == null);
                    // The tracks file: the port's unprimed copy of the postbox holds Q (iron), and no ring.
                    GlobalPackagePort port = new GlobalPackagePort();
                    port.address = ADDRESS;
                    port.offlineBuffer.setStackInSlot(0, box(Items.IRON_INGOT, 16));
                    st[0].connectedPorts.put(postbox, port);
                    CompoundTag tracks = saveStation(level, st[0]);
                    // The postbox's chunk, saved after it: P (copper) restored from W, carrying W; Q taken out.
                    ItemStackHandler inventory = new ItemStackHandler(18);
                    inventory.setStackInSlot(0, box(Items.COPPER_INGOT, 30));
                    level.getChunkSource().chunkMap.write(far, savedChunk(level, postbox, inventory, w)).join();
                    // The crash and the restart: Create reads its tracks file; the journal never knew W.
                    journal().forgetAppliedForTest(w);
                    loadStation(level, st[0], tracks);
                    check("the port came back with Q, unprimed", packages(st[0].connectedPorts.get(postbox).offlineBuffer) == 1
                            && !st[0].connectedPorts.get(postbox).primed);
                    long[] now = PortRing.chunkCountsForTest();
                    System.arraycopy(now, 0, counts, 0, counts.length);
                    writePackage(w, 0, 0, postbox, ADDRESS, 1, copper);
                })
                .thenWaitUntil(() -> check("the resend is answered", commandResults(w).size() == 1))
                .thenExecute(() -> {
                    int first = commandResults(w).get(0);
                    GlobalPackagePort port = st[0].connectedPorts.get(postbox);
                    long[] now = PortRing.chunkCountsForTest();
                    record(test, "resend of W, postbox unloaded: answer, packages in the buffer, primed, holds, reads",
                            status(first) + ", " + packages(port.offlineBuffer) + ", " + port.primed + ", "
                                    + (now[1] - counts[1]) + ", " + (now[2] - counts[2]));
                    if (checks) {
                        check("the chunk's carried W refuses it: DUPLICATE, answered " + status(first), first == Obs.CommandResult.DUPLICATE_OP);
                        check("the buffer took the postbox's inventory: P and not Q, unprimed", packages(port.offlineBuffer) == 1
                                && holds(port.offlineBuffer, Items.COPPER_INGOT) && !holds(port.offlineBuffer, Items.IRON_INGOT) && !port.primed);
                        check("one read, the write held for it", now[2] - counts[2] == 1 && now[1] - counts[1] >= 1 && now[3] - counts[3] == 1);
                        check("W went in the postbox ring", PortRing.wroteToPostbox(port, w));
                    } else {
                        check("written again: APPLIED, answered " + status(first), first == Obs.CommandResult.APPLIED);
                        check("Q and P, primed", packages(port.offlineBuffer) == 2 && port.primed);
                    }
                    writePackage(w2, 0, 0, postbox, ADDRESS, 1, gold);
                })
                .thenWaitUntil(() -> check("the new op is answered", commandResults(w2).size() == 1))
                .thenExecute(() -> {
                    GlobalPackagePort port = st[0].connectedPorts.get(postbox);
                    int got = packages(port.offlineBuffer);
                    boolean q = holds(port.offlineBuffer, Items.IRON_INGOT);
                    long[] now = PortRing.chunkCountsForTest();
                    record(test, "after W2: what the postbox takes back at its load (packages, Q in it), reads",
                            got + ", " + q + (q ? " (a dupe of Q)" : " (exact)") + ", " + (now[2] - counts[2]));
                    check("W2 applied", commandResults(w2).equals(List.of(Obs.CommandResult.APPLIED)));
                    check("primed", port.primed);
                    if (checks) {
                        check("P and P2, not Q: " + got + ", " + q, got == 2 && !q && holds(port.offlineBuffer, Items.GOLD_INGOT));
                        check("no second read", now[2] - counts[2] == 1);
                    } else {
                        check("Q, P and P2: Q comes back: " + got + ", " + q, got == 3 && q);
                    }
                    level.getChunkSource().chunkMap.write(far, null).join();
                    st[0].connectedPorts.remove(postbox);
                    PortRing.chunkChecksForTest(false);
                })
                .thenSucceed();
    }

    /** A package of {@code n} of {@code item}, to {@link #ADDRESS}. */
    private static ItemStack box(Item item, int n) {
        ItemStack box = PackageItem.containing(List.of(new ItemStack(item, n)));
        PackageItem.addAddress(box, ADDRESS);
        return box;
    }

    /** True when a package in {@code h} holds {@code item}. */
    private static boolean holds(IItemHandler h, Item item) {
        for (int s = 0; s < h.getSlots(); s++) {
            ItemStack stack = h.getStackInSlot(s);
            if (!PackageItem.isPackage(stack))
                continue;
            ItemStackHandler contents = PackageItem.getContents(stack);
            for (int c = 0; c < contents.getSlots(); c++)
                if (contents.getStackInSlot(c).is(item))
                    return true;
        }
        return false;
    }

    /**
     * A chunk as {@code ChunkStorage.read} gives it, only what the chunk read looks at: its data version, the postbox's
     * block entity with this inventory, and a carried COMMAND_RESULT for {@code carriedOp} of this engine life.
     */
    private static CompoundTag savedChunk(ServerLevel level, BlockPos postbox, ItemStackHandler inventory, long carriedOp) {
        CompoundTag be = new CompoundTag();
        be.putString("id", "create:package_postbox");
        be.putInt("x", postbox.getX());
        be.putInt("y", postbox.getY());
        be.putInt("z", postbox.getZ());
        be.put("Inventory", inventory.serializeNBT(level.registryAccess()));
        ListTag bes = new ListTag();
        bes.add(be);
        CompoundTag chunk = NbtUtils.addCurrentDataVersion(new CompoundTag());
        chunk.put("block_entities", bes);
        ByteBuffer r = ByteBuffer.allocate(Obs.CommandResult.SIZE).order(Layout.ORDER);
        r.putShort(Layout.R_KIND, (short) Obs.COMMAND_RESULT);
        r.putInt(Layout.R_AUX, Obs.CommandResult.APPLIED);
        r.putLong(Obs.CommandResult.OP_ID, carriedOp);
        PendingRecords carried = new PendingRecords(ind().engineLife(), List.of(new PendingRecords.Entry(0, r.array())));
        CompoundTag attachments = new CompoundTag();
        attachments.put("mic_rtsim:pending", PendingRecords.CODEC.encodeStart(
                level.registryAccess().createSerializationContext(NbtOps.INSTANCE), carried).getOrThrow());
        chunk.put(AttachmentHolder.ATTACHMENTS_NBT_KEY, attachments);
        return chunk;
    }

    // ---- UR-1 -----------------------------------------------------------------------------------

    /**
     * UR-1: with the industry module off a station's save leaves the rings out and keeps every key of Create's in the
     * port's entry; the rings stay on the port, and on again a save writes them once more. A tracks file with the rings
     * reads into ports as Create makes them. (With the plugin removed the mixins are gone, Create's read skips the two
     * long arrays and its next write leaves them out.)
     */
    @GameTest(template = IndustryKit.YARD, batch = "port_ring_7", timeoutTicks = 600)
    public static void withIndustryOffAStationSaveDropsThePortRings(GameTestHelper helper) {
        fastConfig();
        IndustryKit.forget(helper);
        ServerLevel level = helper.getLevel();
        PortRing.fallbackForTest(false);
        BlockPos stationPos = stationPos(helper, 10);
        BlockPos postbox = helper.absolutePos(new BlockPos(3, 1, 3));
        GlobalStation[] st = {null};
        long op = engineOp();
        helper.startSequence()
                .thenIdle(5)
                .thenExecute(() -> placeStation(helper, 10))
                .thenWaitUntil(() -> st[0] = joined(helper, stationPos))
                .thenExecute(() -> {
                    check("the rings are on", PortRing.available(level.getServer()));
                    GlobalPackagePort port = new GlobalPackagePort();
                    port.address = ADDRESS;
                    st[0].connectedPorts.put(postbox, port);
                    PortRing.record(port, op, false);
                    PortRing.record(port, op + 1, true);
                    CompoundTag on = portEntry(saveStation(level, st[0]));
                    check("on: both rings saved", Arrays.equals(on.getLongArray(PortRing.KEY), new long[] {op})
                            && Arrays.equals(on.getLongArray(PortRing.POSTBOX_KEY), new long[] {op + 1}));
                    CompoundTag offFile;
                    RtsimConfig.Test.setModule(Module.INDUSTRY, false);
                    try {
                        offFile = saveStation(level, st[0]);
                    } finally {
                        RtsimConfig.Test.setModule(Module.INDUSTRY, null);
                    }
                    CompoundTag off = portEntry(offFile);
                    Set<String> create = new HashSet<>(on.getAllKeys());
                    create.remove(PortRing.KEY);
                    create.remove(PortRing.POSTBOX_KEY);
                    check("off: the rings left out, Create's keys kept: " + off.getAllKeys() + " vs " + create, off.getAllKeys().equals(create));
                    check("the rings stayed on the port", PortRing.wroteToBuffer(port, op) && PortRing.wroteToPostbox(port, op + 1));
                    CompoundTag again = portEntry(saveStation(level, st[0]));
                    check("on again, the next save writes them", Arrays.equals(again.getLongArray(PortRing.KEY), new long[] {op}));
                    loadStation(level, st[0], saveStation(level, st[0]));
                    GlobalPackagePort back = st[0].connectedPorts.get(postbox);
                    check("read back onto the port Create made", back != port && back.address.equals(ADDRESS)
                            && PortRing.wroteToBuffer(back, op) && PortRing.wroteToPostbox(back, op + 1));
                    record("withIndustryOffAStationSaveDropsThePortRings", "port entry keys on, off", on.getAllKeys().size() + ", " + off.getAllKeys().size());
                    st[0].connectedPorts.remove(postbox);
                })
                .thenSucceed();
    }

    // ---- the emulation --------------------------------------------------------------------------

    /** Where {@link #placeStation} puts the station. */
    private static BlockPos stationPos(GameTestHelper helper, int dz) {
        BlockPos origin = helper.absolutePos(BlockPos.ZERO);
        return new BlockPos(origin.getX() + 16, floorY(helper), origin.getZ() + dz + 1);
    }

    /** A straight track along x at {@code dz} into the yard, and a station beside it targeting it. */
    private static void placeStation(GameTestHelper helper, int dz) {
        int y = floorY(helper);
        BlockPos origin = helper.absolutePos(BlockPos.ZERO);
        int zr = origin.getZ() + dz;
        BlockState track = block("create:track").defaultBlockState().setValue(TrackBlock.SHAPE, TrackShape.XO);
        for (int x = origin.getX() + 2; x <= origin.getX() + 31; x++)
            helper.getLevel().setBlock(new BlockPos(x, y, zr), track, 3);
        BlockPos stationPos = stationPos(helper, dz);
        helper.getLevel().setBlock(stationPos, block("create:track_station").defaultBlockState(), 3);
        StationBlockEntity sbe = (StationBlockEntity) helper.getLevel().getBlockEntity(stationPos);
        CompoundTag tag = sbe.saveWithFullMetadata(helper.getLevel().registryAccess());
        tag.put("TargetTrack", NbtUtils.writeBlockPos(new BlockPos(0, 0, -1)));
        tag.putBoolean("TargetDirection", true);
        sbe.loadWithComponents(tag, helper.getLevel().registryAccess());
    }

    /** For a wait: the station on the track graph. */
    private static GlobalStation joined(GameTestHelper helper, BlockPos stationPos) {
        StationBlockEntity sbe = (StationBlockEntity) helper.getLevel().getBlockEntity(stationPos);
        check("the station joined the track graph", sbe != null && sbe.getStation() != null);
        return sbe.getStation();
    }

    /** What EdgePointStorage.write writes for this station (with its palette), through a file's bytes. */
    private static CompoundTag saveStation(ServerLevel level, GlobalStation station) {
        DimensionPalette dimensions = new DimensionPalette();
        CompoundTag point = new CompoundTag();
        station.write(point, level.registryAccess(), dimensions);
        CompoundTag nbt = new CompoundTag();
        nbt.put("Station", point);
        dimensions.write(nbt);
        try {
            ByteArrayOutputStream file = new ByteArrayOutputStream();
            NbtIo.writeCompressed(nbt, file);
            return NbtIo.readCompressed(new ByteArrayInputStream(file.toByteArray()), NbtAccounter.unlimitedHeap());
        } catch (IOException e) {
            throw new GameTestAssertException("the tracks file did not round-trip: " + e);
        }
    }

    /** What EdgePointStorage.read makes of it: Create's GlobalStation.read, which makes the station's ports anew. */
    private static void loadStation(ServerLevel level, GlobalStation station, CompoundTag nbt) {
        station.read(nbt.getCompound("Station"), level.registryAccess(), false, DimensionPalette.read(nbt));
    }

    /** The first port's entry in a saved station. */
    private static CompoundTag portEntry(CompoundTag nbt) {
        return nbt.getCompound("Station").getList("Ports", Tag.TAG_COMPOUND).getCompound(0);
    }

    /** The chunk from before the write: its carried result for this op dropped. */
    private static void dropCarried(LevelChunk chunk, long op) {
        PendingRecords p = chunk.getExistingDataOrNull(RtsimAttachments.PENDING);
        if (p == null)
            return;
        PendingRecords keep = p.filter(rec -> RtsimSession.pendingOp(rec) != op);
        if (keep.isEmpty())
            chunk.removeData(RtsimAttachments.PENDING);
        else
            chunk.setData(RtsimAttachments.PENDING, keep);
    }

    private static int packages(IItemHandler h) {
        int n = 0;
        for (int s = 0; s < h.getSlots(); s++)
            if (PackageItem.isPackage(h.getStackInSlot(s)))
                n++;
        return n;
    }

    private static Journal journal() {
        RtsimSession s = RtsimSession.current();
        if (s == null)
            throw new GameTestAssertException("no mic_rtsim session on the server");
        return s.boundary().journal();
    }

    private static String status(int s) {
        return switch (s) {
            case Obs.CommandResult.APPLIED -> "APPLIED";
            case Obs.CommandResult.DUPLICATE_OP -> "DUPLICATE";
            case Obs.CommandResult.FAILED -> "FAILED";
            default -> "status " + s;
        };
    }
}
