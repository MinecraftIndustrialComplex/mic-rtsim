package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.gametest.SavedDataWriteKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.SavedDataWriteKit.Kit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.SavedDataWriteKit.Look;
import com.minecraftindustrialcomplex.mic_rtsim.journal.JournalDependent;
import com.minecraftindustrialcomplex.mic_rtsim.journal.SavedDataWrites;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.simibubi.create.content.trains.RailwaySavedData;
import net.minecraft.gametest.framework.AfterBatch;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.file.Files;
import java.util.Arrays;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * A failed write of Create's {@code create_tracks} holds the journal's file (docs/choices-journal-barrier.md U-g1,
 * RP-317 of docs/choices-mowzie-build.md): the hole the Mowzie lead found in RP-200's "always B". Create's
 * {@code RailwaySavedData} keeps a train's position, its schedule and the dedup of each station exchange; if its write
 * fails while the journal's succeeds and the server crashes before the next good write, every train op the restored
 * pages already decided is gone from the train and still on the books.
 *
 * <p>The data is a real {@code RailwaySavedData} (its marker is the mixin {@code RailwaySavedDataMixin}), saved by the
 * same {@code save(File, ...)} vanilla calls, in a world save's start and return, beside a test journal of its own. A
 * write fails the way a full disk fails it: a directory stands where {@code create_tracks.dat} goes. What it writes is
 * Create's global manager as it stands (the test does not touch it, and asserts nothing about its content).
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class TracksWriteHoldGameTests {
    private TracksWriteHoldGameTests() {}

    /**
     * W0 writes the tracks file and the journal's. The tracks write then fails while the journal has moved on: the
     * journal's file stays byte for byte W0's, dirty, and the tracks data (whose dirty flag vanilla cleared) is
     * marked dirty again by the next world save and tried again, still holding. With the disk back, that save writes
     * the tracks file and then the journal's.
     */
    @GameTest(template = IndustryKit.YARD, batch = "mic_rtsim_tracks_write_hold", timeoutTicks = 6000)
    public static void aFailedWriteOfCreateTracksHoldsTheJournalsFileUntilItIsWritten(GameTestHelper helper) {
        RailwaySavedData tracks = RailwaySavedData.factory().constructor().get();
        Kit kit = new Kit(helper.getLevel(), tracks, true);
        byte[][] w0 = {null};
        helper.startSequence()
                .thenExecute(kit.step(() -> {
                    check("Create's tracks data is marked for the journal (RailwaySavedDataMixin applied)", (Object) tracks instanceof JournalDependent);
                    tracks.setDirty();
                    kit.journalMoves();
                    kit.save();
                }))
                .thenWaitUntil(kit.looked())
                .thenExecute(kit.step(() -> {
                    Look l = kit.look.get();
                    check("W0 wrote the journal's file: " + l.writes(), l.writes() == 1 && l.journal().length > 0 && !l.dirty());
                    check("and the tracks file", Files.isRegularFile(kit.dataFile));
                    check("nothing of this directory failed", !kit.heldByFailedWrite());
                    w0[0] = l.journal();
                    kit.diskFull();
                    tracks.setDirty();
                    kit.journalMoves();
                    kit.save();
                }))
                .thenWaitUntil(kit.looked())
                .thenExecute(kit.step(() -> {
                    Look l = kit.look.get();
                    record("aFailedWriteOfCreateTracksHoldsTheJournalsFileUntilItIsWritten", "W1",
                            l.writes() + " journal writes, held by a failed write " + kit.journal.savedDataHeld() + ", " + SavedDataWrites.INSTANCE.describe());
                    check("the tracks write failed and is recorded", kit.heldByFailedWrite());
                    check("the failure was seen on the IO chain, not on the server thread (thread " + SavedDataWrites.INSTANCE.lastFailureThreadId() + ")",
                            kit.failureSeenOffTheServerThread());
                    check("vanilla cleared the tracks data's dirty flag, so the next save would not write it by itself (the premise)", !tracks.isDirty());
                    check("the journal's file is byte for byte W0's (" + l.writes() + " writes)", Arrays.equals(w0[0], l.journal()) && l.writes() == 1);
                    check("the journal stays dirty for the next save", l.dirty());
                    // a second save, the disk still full: the tracks data is marked dirty again at its start, tried, and holds again
                    kit.journalMoves();
                    kit.save();
                }))
                .thenWaitUntil(kit.looked())
                .thenExecute(kit.step(() -> {
                    Look l = kit.look.get();
                    check("held at the second save, the file still W0's: " + l.writes(), Arrays.equals(w0[0], l.journal()) && l.writes() == 1 && l.dirty());
                    check("the tracks data was marked dirty again and tried again: " + SavedDataWrites.INSTANCE.failures() + " failures since this test began",
                            SavedDataWrites.INSTANCE.failures() == kit.failuresBefore + 2);
                    check("the mark was made on the server thread, by the save's start, not by the IO chain", kit.remarkedOnTheServerThread());
                    check("two holds counted, in one streak (logged once)", kit.journal.savedDataHeld() == 2 && kit.journal.savedDataStreaks() == 1);
                    kit.diskFree();
                    kit.journalMoves(); // nothing of the tracks data changes: only the mark brings it to this save
                    kit.save();
                }))
                .thenWaitUntil(kit.looked())
                .thenExecute(kit.step(() -> {
                    Look l = kit.look.get();
                    record("aFailedWriteOfCreateTracksHoldsTheJournalsFileUntilItIsWritten", "after the disk is back",
                            l.writes() + " journal writes, " + SavedDataWrites.INSTANCE.describe());
                    check("the tracks file is written without any change to it", Files.isRegularFile(kit.dataFile));
                    check("then the journal's file, which is no longer W0's (" + l.writes() + " writes)", l.writes() == 2 && !Arrays.equals(w0[0], l.journal()) && !l.dirty());
                    check("the record is gone", !kit.heldByFailedWrite());
                    kit.cleanup();
                })).thenSucceed();
    }

    @AfterBatch(batch = "mic_rtsim_tracks_write_hold")
    public static void cleanUp(ServerLevel level) {
        SavedDataWriteKit.cleanupAll();
    }
}
