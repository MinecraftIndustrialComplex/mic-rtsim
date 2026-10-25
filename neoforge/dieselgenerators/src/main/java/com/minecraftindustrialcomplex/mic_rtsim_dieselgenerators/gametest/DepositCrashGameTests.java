package com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators.gametest;

import com.jesz.createdieselgenerators.world.OilChunksSavedData;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Deposits;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.journal.ChunkWriteBarrier;
import com.minecraftindustrialcomplex.mic_rtsim.journal.DurableOpRings;
import com.minecraftindustrialcomplex.mic_rtsim.journal.JournalSavedData;
import com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators.DieselOilPlugin;
import com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators.MicRtsimDieselGenerators;
import com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators.OilRingHolder;
import net.minecraft.core.HolderLookup;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.Tag;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.dimension.DimensionType;
import net.minecraft.world.level.saveddata.SavedData;
import net.minecraft.world.level.storage.DimensionDataStorage;
import net.minecraft.world.level.storage.LevelResource;
import net.neoforged.fml.util.ObfuscationReflectionHelper;
import net.neoforged.neoforge.common.IOUtilities;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.Consumer;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.*;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.io;

/**
 * IND_DEPOSIT_DELTA across a crash inside a world save (slice U; docs/choices-journal-barrier.md RP-200 on).
 *
 * <p>The deposit's amount is Diesel's {@code cdg_oil_chunks} SavedData, one file per dimension; Java's dedup of the
 * op is that file's own ring of applied ops ({@code mic_rtsim:applied_ops}, RP-204) or the journal's applied set,
 * {@code mic_rtsim_journal.dat} in the overworld's. Each test draws 1,234 mB from a chunk holding 5,000,000, saves
 * the world as an autosave does, and at one instant of the journal's write on the SavedData IO chain (right after
 * it, or right before it) reads what a crash then leaves on disk: the chunk's oil, the oil file's ring, and whether
 * the journal holds the op. The restart is emulated on the live state: the oil and the ring are set to what the oil
 * file holds, the journal's knowledge of the op to what its file holds, and the engine sends the same op again.
 *
 * <p>A crash right after the journal's write must never leave the deposit undrawn: the oil the delta took is in
 * the tanks (the reconcile's chunks reach disk before the journal, ChunkWriteBarrier), so a deposit that keeps it
 * too is oil from nothing. A crash right before it must not draw the deposit twice: the oil file holds the draw
 * and its ring the op.
 */
@GameTestHolder(MicRtsimDieselGenerators.MODID)
@PrefixGameTestTemplate(false)
public final class DepositCrashGameTests {
    private DepositCrashGameTests() {}

    private static final int START = 5_000_000, DELTA = -1234;

    /** What a crash at the hook's instant leaves on disk; {@code oilData} is the oil file's {@code data} tag. */
    private record OnDisk(int oil, boolean ringHasOp, boolean journalHasOp, CompoundTag oilData, String error) {}

    /**
     * Nether oil, the server dead right after the journal's file is written. Before the fix the nether's
     * SavedData were queued after the overworld's journal: the journal on disk knew the op, the oil file did not
     * hold the draw, and the op sent again was a duplicate (5,000,000 left, the dupe). The failing-first run read
     * exactly that on disk; its restart then applied the op again only because the live journal had dropped the
     * op at the engine's acknowledgement, which a restart from the file does not, so the restart now takes the
     * journal's knowledge from its file.
     */
    @GameTest(template = "yard", batch = "diesel_crash_nether_after", timeoutTicks = 2400)
    public static void aNetherDrawIsOnDiskWhenTheServerDiesRightAfterTheJournal(GameTestHelper helper) {
        crash(helper, "aNetherDrawIsOnDiskWhenTheServerDiesRightAfterTheJournal", Level.NETHER, true, false, START + DELTA);
    }

    /**
     * Overworld oil, the server dead right after the journal's file is written, with the overworld's SavedData
     * map grown until the journal comes before Diesel's file in its iteration order (capacities 64, 128 and 256
     * put it there in a real world). Before the fix: 5,000,000 left, the dupe.
     */
    @GameTest(template = "yard", batch = "diesel_crash_overworld_after", timeoutTicks = 2400)
    public static void anOverworldDrawIsOnDiskWhenTheServerDiesRightAfterTheJournal(GameTestHelper helper) {
        crash(helper, "anOverworldDrawIsOnDiskWhenTheServerDiesRightAfterTheJournal", Level.OVERWORLD, true, true, START + DELTA);
    }

    /**
     * Nether oil, the server dead right before the journal's file is written: the oil file holds the draw and the
     * journal does not know the op. Its ring does, so the op sent again is refused: one draw, 4,998,766 left. With the
     * journal written last and no ring the op was applied again (4,997,532 left, the loss RP-202 recorded); before
     * the journal was written last the oil file was not on disk yet at this instant.
     */
    @GameTest(template = "yard", batch = "diesel_crash_nether_before", timeoutTicks = 2400)
    public static void aNetherDrawIsTakenOnceWhenTheServerDiesRightBeforeTheJournal(GameTestHelper helper) {
        crash(helper, "aNetherDrawIsTakenOnceWhenTheServerDiesRightBeforeTheJournal", Level.NETHER, false, false, START + DELTA);
    }

    private static void crash(GameTestHelper helper, String test, ResourceKey<Level> dim, boolean afterJournal, boolean journalFirst, int expected) {
        MinecraftServer server = helper.getLevel().getServer();
        ServerLevel level = server.getLevel(dim);
        check("the level " + dim.location(), level != null);
        fastConfig();
        forgetAll(helper);
        java.util.Random rnd = new java.util.Random();
        ChunkPos cp = new ChunkPos(60_000 + rnd.nextInt(20_000), 60_000 + rnd.nextInt(20_000)); // nobody loads it
        String good = Deposits.name(DieselOilPlugin.OIL, dim, cp.x, cp.z);
        Path root = server.getWorldPath(LevelResource.ROOT);
        Path oilFile = DimensionType.getStorageFolder(dim, root).resolve("data").resolve("cdg_oil_chunks.dat");
        HolderLookup.Provider registries = level.registryAccess();
        long op = engineOp();
        AtomicReference<Integer> settled = new AtomicReference<>();
        AtomicReference<OnDisk> disk = new AtomicReference<>();
        AtomicBoolean passed = new AtomicBoolean();
        int[] saves = {0};
        JournalSavedData.BEFORE_WRITE_FOR_TEST.set(null);
        JournalSavedData.AFTER_WRITE_FOR_TEST.set(null);
        helper.startSequence()
                .thenExecute(() -> {
                    OilChunksSavedData.setChunkOilAmount(level, cp, START);
                    if (journalFirst)
                        record(test, "overworld SavedData order", putTheJournalFirst(server.overworld()));
                    server.saveEverything(true, false, false);
                    IOUtilities.withIOWorker(() -> {
                        try {
                            settled.set(oilOnDisk(oilFile, cp));
                        } catch (IOException e) {
                            settled.set(-3);
                        }
                    });
                })
                .thenWaitUntil(io(() -> check("the first save is on disk", settled.get() != null)))
                .thenExecute(() -> {
                    check("the oil file holds 5,000,000 before the draw: " + settled.get(), settled.get() == START);
                    depositDelta(op, IndRecords.DepositPlugin.DIESEL_OIL, level, cp.x, cp.z, good, DELTA);
                })
                .thenWaitUntil(() -> check("applied", commandResults(op).contains(Obs.CommandResult.APPLIED)))
                .thenExecute(() -> {
                    check("4,998,766 in memory", OilChunksSavedData.getChunkOilAmount(level, cp) == START + DELTA);
                    IOUtilities.waitUntilIOWorkerComplete(); // no journal write of an earlier save takes the hook
                    Consumer<Path> peek = journalFile -> disk.set(peek(oilFile, journalFile, cp, op, registries));
                    (afterJournal ? JournalSavedData.AFTER_WRITE_FOR_TEST : JournalSavedData.BEFORE_WRITE_FOR_TEST).set(peek);
                    save(server, passed);
                    saves[0]++;
                })
                .thenWaitUntil(io(() -> { // the chain's fsyncs take wall time a tick count does not give (RtsimTests.io)
                    if (disk.get() != null)
                        return;
                    if (passed.get() && saves[0] < 8) { // held by the barrier: the journal's next save writes it
                        save(server, passed);
                        saves[0]++;
                    }
                    check("the journal's file was written (" + saves[0] + " saves; " + barrierLine() + ")", false);
                }))
                .thenExecute(() -> {
                    OnDisk d = disk.get();
                    check("read the disk: " + d.error(), d.error() == null);
                    record(test, "on disk at the crash (" + (afterJournal ? "right after" : "right before") + " the journal's write)",
                            "oil " + d.oil() + ", its ring " + (d.ringHasOp() ? "knows" : "does not know") + " the op, the journal "
                                    + (d.journalHasOp() ? "knows" : "does not know") + " it; " + saves[0] + " saves; " + barrierLine());
                    check("the oil file holds the chunk: " + d.oil(), d.oil() >= 0);
                    check("the oil file's ring holds the op beside its draw", d.oil() != START + DELTA || d.ringHasOp());
                    // The restart: the server holds what is on disk, and the engine sends the op again.
                    OilChunksSavedData.setChunkOilAmount(level, cp, d.oil());
                    ((OilRingHolder) OilChunksSavedData.load(level)).mic_rtsim$setRing(DurableOpRings.read(d.oilData()));
                    if (d.journalHasOp())
                        RtsimSession.current().journal().journal().rememberAppliedForTest(op);
                    else
                        RtsimSession.current().journal().journal().forgetAppliedForTest(op);
                    depositDelta(op, IndRecords.DepositPlugin.DIESEL_OIL, level, cp.x, cp.z, good, DELTA);
                })
                .thenWaitUntil(() -> check("answered again", commandResults(op).size() >= 2))
                .thenExecute(() -> {
                    int now = OilChunksSavedData.getChunkOilAmount(level, cp);
                    record(test, "after the restart", now + " left (" + commandResults(op) + ")");
                    check(String.format("%,d left after the restart, expected %,d (%,d is the deposit kept: oil from nothing)", now, expected, START),
                            now == expected);
                })
                .thenSucceed();
    }

    /** A world save as an autosave makes it; {@code passed} is set once the IO chain has run everything it queued. */
    private static void save(MinecraftServer server, AtomicBoolean passed) {
        passed.set(false);
        server.saveEverything(true, false, false);
        IOUtilities.withIOWorker(() -> passed.set(true));
    }

    /**
     * On the IO chain, at the hook: the chunk's oil in its file, whether the file's ring and the journal's file know
     * the op, and the oil file's data for the restart.
     */
    private static OnDisk peek(Path oilFile, Path journalFile, ChunkPos cp, long op, HolderLookup.Provider registries) {
        try {
            int oil = oilOnDisk(oilFile, cp);
            CompoundTag oilData = Files.isRegularFile(oilFile)
                    ? NbtIo.readCompressed(oilFile, NbtAccounter.unlimitedHeap()).getCompound("data") : new CompoundTag();
            boolean ring = DurableOpRings.read(oilData).contains(op);
            boolean has = false;
            if (Files.isRegularFile(journalFile)) {
                CompoundTag j = NbtIo.readCompressed(journalFile, NbtAccounter.unlimitedHeap());
                has = JournalSavedData.load(j.getCompound("data"), registries).journal().commandOpApplied(op);
            }
            return new OnDisk(oil, ring, has, oilData, null);
        } catch (Exception e) {
            return new OnDisk(-1, false, false, new CompoundTag(), e.toString());
        }
    }

    /** The chunk's amount in Diesel's file ({@code OilChunks}: x, z, Amountmb); -1 with no file, -2 with no entry. */
    private static int oilOnDisk(Path file, ChunkPos cp) throws IOException {
        if (!Files.isRegularFile(file))
            return -1;
        CompoundTag root = NbtIo.readCompressed(file, NbtAccounter.unlimitedHeap());
        ListTag list = root.getCompound("data").getList("OilChunks", Tag.TAG_COMPOUND);
        for (int i = 0; i < list.size(); i++) {
            CompoundTag c = list.getCompound(i);
            if (c.getInt("x") == cp.x && c.getInt("z") == cp.z)
                return c.contains("Amountmb") ? c.getInt("Amountmb") : c.getInt("Amount");
        }
        return -2;
    }

    /** A SavedData that is never dirty, so never written: it only grows the storage's map. */
    private static final class Pad extends SavedData {
        @Override
        public CompoundTag save(CompoundTag tag, HolderLookup.Provider registries) {
            return tag;
        }
    }

    /**
     * Grows the overworld's SavedData map with pads until the journal comes before Diesel's file in the order
     * {@code DimensionDataStorage.save} writes them (its HashMap's iteration order), as it does in a world whose
     * map has grown to capacity 64, 128 or 256. The pads stay for the rest of the server's run.
     */
    private static String putTheJournalFirst(ServerLevel overworld) {
        DimensionDataStorage storage = overworld.getDataStorage();
        Map<String, SavedData> cache = ObfuscationReflectionHelper.getPrivateValue(DimensionDataStorage.class, storage, "cache");
        check("the storage's map", cache != null);
        int before = cache.size(), pads = 0;
        while (order(cache, JournalSavedData.FILE) > order(cache, "cdg_oil_chunks") && pads < 400) {
            storage.computeIfAbsent(new SavedData.Factory<>(Pad::new, (t, r) -> new Pad()), "mic_rtsim_test_pad_" + pads);
            pads++;
        }
        check("the journal before Diesel's file", order(cache, JournalSavedData.FILE) < order(cache, "cdg_oil_chunks"));
        return before + " files, " + pads + " pads: the journal at " + order(cache, JournalSavedData.FILE) + ", Diesel's at "
                + order(cache, "cdg_oil_chunks") + " of " + cache.size();
    }

    private static int order(Map<String, SavedData> cache, String key) {
        List<String> keys = new ArrayList<>(cache.keySet());
        int i = keys.indexOf(key);
        check(key + " in the overworld's SavedData", i >= 0);
        return i;
    }

    private static String barrierLine() {
        ChunkWriteBarrier b = RtsimSession.current().journal().chunks();
        return "the barrier held " + b.held() + " writes, " + b.late() + " late";
    }
}
