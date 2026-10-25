package com.minecraftindustrialcomplex.mic_rtsim_destroy.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Deposits;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.journal.ChunkWriteBarrier;
import com.minecraftindustrialcomplex.mic_rtsim.journal.DurableOpRing;
import com.minecraftindustrialcomplex.mic_rtsim.journal.DurableOpRings;
import com.minecraftindustrialcomplex.mic_rtsim.journal.JournalSavedData;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim_destroy.DestroyPlugin;
import com.minecraftindustrialcomplex.mic_rtsim_destroy.MicRtsimDestroy;
import com.minecraftindustrialcomplex.mic_rtsim_destroy.PollutionLedger;
import net.minecraft.core.BlockPos;
import net.minecraft.core.HolderLookup;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.resources.ResourceLocation;
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
import petrolpark.mc.destroy.DestroyPollutionTypes;
import petrolpark.mc.destroy.core.pollution.PollutionHelper;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.Consumer;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.*;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.io;

/**
 * Level pollution's IND_DEPOSIT_DELTA across a crash inside a world save (slice U; docs/choices-journal-barrier.md
 * RP-207).
 *
 * <p>The op's effect is in two files of the overworld besides the journal's: Destroy's greenhouse value is a level
 * attachment in NeoForge's {@code neoforge_data_attachments.dat}, with the applied ops beside it (the attachment
 * {@code mic_rtsim:applied_ops}); the district's booked total is {@code mic_rtsim_destroy_pollution.dat}, with its
 * own ring of the ops it booked. Both are written before the journal's file in a world save (RP-200), in the order
 * of the overworld's SavedData map, so a crash right before the journal's write keeps both, and a crash between
 * the two keeps one of them; which one depends on that map's order. Each test emits 40 greenhouse in a district,
 * saves the world as an autosave does (the "settled" files), applies a delta of 25 with the op, saves again and
 * reads the three files at one instant of the journal's write on the IO chain. The restart is emulated on the live
 * state from the files: the greenhouse value and the level's ring from one attachments file, the ledger from one
 * ledger file, the journal's knowledge of the op from its file; then the engine sends the op again.
 *
 * <p>Not failing-first: without RP-207 neither file kept a ring to read back (the tests do not compile there).
 * What the files held without it follows from the outcomes below: every case that keeps the attachments file
 * applied the op a second time (greenhouse from nothing).
 */
@GameTestHolder(MicRtsimDestroy.MODID)
@PrefixGameTestTemplate(false)
public final class PollutionCrashGameTests {
    private PollutionCrashGameTests() {}

    private static final int EMITTED = 40, DELTA = 25;
    private static final ResourceLocation GH = ResourceLocation.parse("destroy:greenhouse");
    private static final String ATTACHMENTS = "neoforge_data_attachments", POLLUTION_KEY = "destroy:level_pollution";

    /** Which of the two files the restart reads from the crash, the other from the settled save. */
    private enum Kept { BOTH, ATTACHMENTS_ONLY, LEDGER_ONLY }

    /** The overworld's three files: the {@code data} tags of the attachments and the ledger, and the journal's knowledge. */
    private record OnDisk(CompoundTag attachments, CompoundTag ledger, boolean journalHasOp, String error) {}

    /**
     * The server dead right before the journal's write: both files hold the delta and the op, the journal does not.
     * The level's ring refuses the resend: 25 once, booked once.
     */
    @GameTest(template = "yard", batch = "destroy_crash_before_both", timeoutTicks = 2400)
    public static void levelPollutionIsAppliedOnceWhenTheServerDiesRightBeforeTheJournal(GameTestHelper helper) {
        crash(helper, "levelPollutionIsAppliedOnceWhenTheServerDiesRightBeforeTheJournal", false, Kept.BOTH,
                Obs.CommandResult.DUPLICATE_OP, EMITTED + DELTA, 0, 1);
    }

    /**
     * The attachments file reached disk and the ledger's did not: the level's ring refuses the resend, so the
     * greenhouse is right, and the district's total misses the 25 (the ledger under-counts: the benign residual
     * RP-207 records; the engine reads a total 25 lower at the next sample, never pollution from nothing).
     */
    @GameTest(template = "yard", batch = "destroy_crash_before_ledger_lost", timeoutTicks = 2400)
    public static void aLostLedgerUnderCountsAndTheDeltaIsAppliedOnce(GameTestHelper helper) {
        crash(helper, "aLostLedgerUnderCountsAndTheDeltaIsAppliedOnce", false, Kept.ATTACHMENTS_ONLY,
                Obs.CommandResult.DUPLICATE_OP, EMITTED, 0, 0);
    }

    /**
     * The ledger's file reached disk and the attachments file did not: the greenhouse on disk lacks the 25 and
     * the level's ring lacks the op, so the resend is applied again (exact: the 25 once), and the ledger's own
     * ring keeps it from booking the 25 a second time.
     */
    @GameTest(template = "yard", batch = "destroy_crash_before_pollution_lost", timeoutTicks = 2400)
    public static void aLostPollutionFileIsAppliedAgainAndBookedOnce(GameTestHelper helper) {
        crash(helper, "aLostPollutionFileIsAppliedAgainAndBookedOnce", false, Kept.LEDGER_ONLY,
                Obs.CommandResult.APPLIED, EMITTED + DELTA, 1, 0);
    }

    /**
     * The server dead right after the journal's write, with the journal first in the overworld's SavedData map (as
     * in a world whose map has grown): both files are on disk before it all the same (RP-200), the journal knows
     * the op and refuses the resend.
     */
    @GameTest(template = "yard", batch = "destroy_crash_after", timeoutTicks = 2400)
    public static void levelPollutionIsOnDiskWhenTheServerDiesRightAfterTheJournal(GameTestHelper helper) {
        crash(helper, "levelPollutionIsOnDiskWhenTheServerDiesRightAfterTheJournal", true, Kept.BOTH,
                Obs.CommandResult.DUPLICATE_OP, EMITTED + DELTA, 0, 0);
    }

    private static void crash(GameTestHelper helper, String test, boolean afterJournal, Kept kept, int secondAnswer,
                              long bookedAfter, long refusedBookings, long restamps) {
        MinecraftServer server = helper.getLevel().getServer();
        ServerLevel level = server.overworld();
        check("the test's level is the overworld", helper.getLevel() == level);
        fastConfig();
        // The stub engine acknowledges an answer within ticks, which a real engine does only once its durable save holds
        // it, and a real engine never sends that op again; the crash emulated here needs the op kept until its resend.
        DurableOpRings.holdPrunesForTest(true);
        forgetAll(helper);
        HolderLookup.Provider registries = level.registryAccess();
        Path data = DimensionType.getStorageFolder(Level.OVERWORLD, server.getWorldPath(LevelResource.ROOT)).resolve("data");
        Path attachmentsFile = data.resolve(ATTACHMENTS + ".dat"), ledgerFile = data.resolve(PollutionLedger.FILE + ".dat");
        BlockPos chest = inChunk(helper, 3, 3);
        long op = engineOp();
        long[] district = {0};
        int[] chunk = {0, 0};
        String[] good = {null};
        int[] settledGreenhouse = {0};
        int[] saves = {0};
        long[] refused0 = {0};
        long[] restamped0 = {0};
        long[] booked0 = {0};
        AtomicReference<OnDisk> settled = new AtomicReference<>(), disk = new AtomicReference<>();
        JournalSavedData.BEFORE_WRITE_FOR_TEST.set(null);
        JournalSavedData.AFTER_WRITE_FOR_TEST.set(null);
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, chest, com.minecraftindustrialcomplex.mic_rtsim.gametest.TestMachines.MACHINE.get().defaultBlockState());
                    playerPlaced(helper, chest);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, chest);
                    check("the district", d != null);
                    district[0] = d.id;
                    chunk[0] = ChunkPos.getX(d.anchorChunk());
                    chunk[1] = ChunkPos.getZ(d.anchorChunk());
                    good[0] = Deposits.name(GH, DestroyPlugin.pollutionKey(d.id));
                })
                .thenExecute(() -> {
                    check("pollution is on in Destroy's config", PollutionHelper.isPollutionEnabled());
                    booked0[0] = booked(level, district[0]); // a district id an earlier test used may have a total
                    PollutionHelper.changePollution(level, chest, DestroyPollutionTypes.GREENHOUSE.get(), EMITTED);
                    check("40 greenhouse booked to the district", booked(level, district[0]) == booked0[0] + EMITTED);
                    if (afterJournal)
                        record(test, "overworld SavedData order", putTheJournalFirst(level));
                    // The settled save: the files a crash that lost this save's write of one of them falls back on.
                    server.saveEverything(true, false, false);
                    IOUtilities.waitUntilIOWorkerComplete();
                    settled.set(read(attachmentsFile, ledgerFile, null, op, registries));
                })
                .thenExecute(() -> {
                    OnDisk s = settled.get();
                    check("read the settled files: " + s.error(), s.error() == null);
                    settledGreenhouse[0] = greenhouse(s.attachments());
                    check("the settled greenhouse is the live one: " + settledGreenhouse[0],
                            settledGreenhouse[0] == PollutionHelper.getPollution(level, DestroyPollutionTypes.GREENHOUSE.get()));
                    check("the settled ledger holds the 40", booked(s.ledger(), district[0]) == booked0[0] + EMITTED);
                    depositDelta(op, IndRecords.DepositPlugin.DESTROY_CRUDE, level, chunk[0], chunk[1], good[0], DELTA);
                })
                .thenWaitUntil(() -> check("applied", commandResults(op).contains(Obs.CommandResult.APPLIED)))
                .thenExecute(() -> {
                    check("the delta is in the level", PollutionHelper.getPollution(level, DestroyPollutionTypes.GREENHOUSE.get())
                            == settledGreenhouse[0] + DELTA);
                    check("and booked", booked(level, district[0]) == booked0[0] + EMITTED + DELTA);
                    Consumer<Path> peek = journalFile -> disk.set(read(attachmentsFile, ledgerFile, journalFile, op, registries));
                    (afterJournal ? JournalSavedData.AFTER_WRITE_FOR_TEST : JournalSavedData.BEFORE_WRITE_FOR_TEST).set(peek);
                    server.saveEverything(true, false, false);
                    IOUtilities.waitUntilIOWorkerComplete();
                    saves[0]++;
                })
                .thenWaitUntil(io(() -> {
                    if (disk.get() != null)
                        return;
                    if (saves[0] < 8) { // held by the barrier: the journal's next save writes it
                        server.saveEverything(true, false, false);
                        IOUtilities.waitUntilIOWorkerComplete();
                        saves[0]++;
                    }
                    check("the journal's file was written (" + saves[0] + " saves; " + barrierLine() + ")", disk.get() != null);
                }))
                .thenExecute(() -> {
                    OnDisk d = disk.get(), s = settled.get();
                    check("read the disk: " + d.error(), d.error() == null);
                    long key = DurableOpRing.keyOf(op, good[0]);
                    boolean levelRing = DurableOpRings.read(d.attachments()).contains(op), ledgerRing = DurableOpRings.read(d.ledger()).contains(key);
                    record(test, "on disk at the crash (" + (afterJournal ? "right after" : "right before") + " the journal's write)",
                            "greenhouse " + greenhouse(d.attachments()) + " (settled " + settledGreenhouse[0] + "), the level's ring "
                                    + (levelRing ? "knows" : "does not know") + " the op; the ledger " + booked(d.ledger(), district[0])
                                    + ", its ring " + (ledgerRing ? "knows" : "does not know") + " it; the journal "
                                    + (d.journalHasOp() ? "knows" : "does not know") + " it; " + saves[0] + " saves; " + barrierLine());
                    check("the attachments file holds the delta", greenhouse(d.attachments()) == settledGreenhouse[0] + DELTA);
                    check("and the op beside it", levelRing);
                    check("the ledger's file holds the booking", booked(d.ledger(), district[0]) == booked0[0] + EMITTED + DELTA);
                    check("and the op beside it", ledgerRing);
                    check("the journal's file " + (afterJournal ? "knows" : "does not know") + " the op", d.journalHasOp() == afterJournal);
                    // The restart: the server holds what is on disk, and the engine sends the op again.
                    CompoundTag attachments = kept == Kept.LEDGER_ONLY ? s.attachments() : d.attachments();
                    CompoundTag ledger = kept == Kept.ATTACHMENTS_ONLY ? s.ledger() : d.ledger();
                    PollutionHelper.changePollution(level, DestroyPollutionTypes.GREENHOUSE.get(),
                            greenhouse(attachments) - PollutionHelper.getPollution(level, DestroyPollutionTypes.GREENHOUSE.get()));
                    level.setData(RtsimAttachments.LEVEL_APPLIED_OPS.get(), DurableOpRings.read(attachments));
                    level.getDataStorage().set(PollutionLedger.FILE, PollutionLedger.load(ledger, registries));
                    if (d.journalHasOp())
                        RtsimSession.current().journal().journal().rememberAppliedForTest(op);
                    else
                        RtsimSession.current().journal().journal().forgetAppliedForTest(op);
                    refused0[0] = PollutionLedger.rebooksRefused();
                    restamped0[0] = PollutionLedger.restampedOnRefusal();
                    record(test, "the restart reads", "the attachments file " + (attachments == d.attachments() ? "of the crash" : "settled")
                            + ", the ledger's " + (ledger == d.ledger() ? "of the crash" : "settled") + ": greenhouse "
                            + PollutionHelper.getPollution(level, DestroyPollutionTypes.GREENHOUSE.get()) + ", booked " + booked(level, district[0]));
                    depositDelta(op, IndRecords.DepositPlugin.DESTROY_CRUDE, level, chunk[0], chunk[1], good[0], DELTA);
                })
                .thenWaitUntil(() -> check("answered again", commandResults(op).size() >= 2))
                .thenExecute(() -> {
                    List<Integer> answers = commandResults(op);
                    int now = PollutionHelper.getPollution(level, DestroyPollutionTypes.GREENHOUSE.get());
                    long booked = booked(level, district[0]) - booked0[0], refused = PollutionLedger.rebooksRefused() - refused0[0];
                    record(test, "after the restart", "greenhouse " + now + " (settled " + settledGreenhouse[0] + "), booked " + booked
                            + " over the district's start, bookings refused " + refused + ", answers " + answers);
                    check("the second answer " + answers, answers.get(1) == secondAnswer);
                    check(String.format("greenhouse %d after the restart, expected %d (%d is the delta applied twice)", now,
                            settledGreenhouse[0] + DELTA, settledGreenhouse[0] + 2 * DELTA), now == settledGreenhouse[0] + DELTA);
                    check("booked " + booked + ", expected " + bookedAfter, booked == bookedAfter);
                    check("bookings refused " + refused + ", expected " + refusedBookings, refused == refusedBookings);
                    // A refusal by the level's ring stamps the ledger's entry with the refusal's answer too (P3.3):
                    // left with the first life's answer, the journal's next save past it would drop it.
                    long restamped = PollutionLedger.restampedOnRefusal() - restamped0[0];
                    check("ledger entries stamped again " + restamped + ", expected " + restamps, restamped == restamps);
                    if (restamps > 0) {
                        long levelSeq = seqOf(level.getData(RtsimAttachments.LEVEL_APPLIED_OPS.get()), op);
                        DurableOpRing ledgerRing = ObfuscationReflectionHelper.getPrivateValue(PollutionLedger.class, PollutionLedger.get(level), "ring");
                        long ledgerSeq = seqOf(ledgerRing, DurableOpRing.keyOf(op, good[0]));
                        check("the ledger's entry has the refusal's answer " + levelSeq + ", not " + ledgerSeq, ledgerSeq == levelSeq);
                    }
                    DurableOpRings.holdPrunesForTest(false);
                })
                .thenSucceed();
    }

    /** The answer's sequence a ring keeps {@code key} with, or -1. */
    private static long seqOf(DurableOpRing ring, long key) {
        long[] ops = ring.ops(), seqs = ring.seqs();
        for (int i = 0; i < ops.length; i++)
            if (ops[i] == key)
                return seqs[i];
        return -1;
    }

    /** The district's booked greenhouse in the live ledger. */
    private static long booked(ServerLevel level, long district) {
        return PollutionLedger.get(level).totals(district).getOrDefault(GH, 0L);
    }

    /** The district's booked greenhouse in a ledger file's {@code data} tag. */
    private static long booked(CompoundTag ledger, long district) {
        return ledger.getCompound("districts").getCompound(Long.toString(district)).getLong(GH.toString());
    }

    /** The greenhouse value in an attachments file's {@code data} tag (Destroy's codec: {@code values}, by type id). */
    private static int greenhouse(CompoundTag attachments) {
        return attachments.getCompound(POLLUTION_KEY).getCompound("values").getInt(GH.toString());
    }

    /** The files as the IO chain leaves them; {@code journalFile} null for the settled read. */
    private static OnDisk read(Path attachmentsFile, Path ledgerFile, Path journalFile, long op, HolderLookup.Provider registries) {
        try {
            check("the attachments file " + attachmentsFile, Files.isRegularFile(attachmentsFile));
            check("the ledger's file " + ledgerFile, Files.isRegularFile(ledgerFile));
            CompoundTag a = NbtIo.readCompressed(attachmentsFile, NbtAccounter.unlimitedHeap()).getCompound("data");
            check("Destroy's level pollution in the attachments file: " + a.getAllKeys(), a.contains(POLLUTION_KEY));
            CompoundTag l = NbtIo.readCompressed(ledgerFile, NbtAccounter.unlimitedHeap()).getCompound("data");
            boolean has = false;
            if (journalFile != null && Files.isRegularFile(journalFile)) {
                CompoundTag j = NbtIo.readCompressed(journalFile, NbtAccounter.unlimitedHeap());
                has = JournalSavedData.load(j.getCompound("data"), registries).journal().commandOpApplied(op);
            }
            return new OnDisk(a, l, has, null);
        } catch (Exception e) {
            return new OnDisk(new CompoundTag(), new CompoundTag(), false, e.toString());
        }
    }

    /** A SavedData that is never dirty, so never written: it only grows the storage's map. */
    private static final class Pad extends SavedData {
        @Override
        public CompoundTag save(CompoundTag tag, HolderLookup.Provider registries) {
            return tag;
        }
    }

    /**
     * Grows the overworld's SavedData map with pads until the journal comes before both files in the order
     * {@code DimensionDataStorage.save} writes them (its HashMap's iteration order). The pads stay for the rest of
     * the server's run.
     */
    private static String putTheJournalFirst(ServerLevel overworld) {
        DimensionDataStorage storage = overworld.getDataStorage();
        Map<String, SavedData> cache = ObfuscationReflectionHelper.getPrivateValue(DimensionDataStorage.class, storage, "cache");
        check("the storage's map", cache != null);
        int before = cache.size(), pads = 0;
        while ((order(cache, JournalSavedData.FILE) > order(cache, ATTACHMENTS) || order(cache, JournalSavedData.FILE) > order(cache, PollutionLedger.FILE))
                && pads < 400) {
            storage.computeIfAbsent(new SavedData.Factory<>(Pad::new, (t, r) -> new Pad()), "mic_rtsim_test_pad_" + pads);
            pads++;
        }
        check("the journal before both files", order(cache, JournalSavedData.FILE) < order(cache, ATTACHMENTS)
                && order(cache, JournalSavedData.FILE) < order(cache, PollutionLedger.FILE));
        return before + " files, " + pads + " pads: the journal at " + order(cache, JournalSavedData.FILE) + ", the attachments at "
                + order(cache, ATTACHMENTS) + ", the ledger at " + order(cache, PollutionLedger.FILE) + " of " + cache.size();
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
