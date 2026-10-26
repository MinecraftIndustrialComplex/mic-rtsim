package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.SavedDataWriteKit.Kit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.SavedDataWriteKit.Look;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryData;
import com.minecraftindustrialcomplex.mic_rtsim.journal.JournalDependent;
import com.minecraftindustrialcomplex.mic_rtsim.journal.JournalSavedData;
import com.minecraftindustrialcomplex.mic_rtsim.journal.SavedDataWrites;
import com.minecraftindustrialcomplex.mic_rtsim.journal.WorldSaveOrder;
import com.minecraftindustrialcomplex.mic_rtsim.village.VillageRegistry;
import net.minecraft.core.HolderLookup;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.world.level.saveddata.SavedData;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.file.Files;
import java.util.Arrays;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * A failed SavedData write holds the journal's file (docs/choices-journal-barrier.md U-g1, RP-317). Vanilla's
 * {@code SavedData.save} clears the dirty flag and queues the write; a write that throws is logged and lost, and the
 * journal's file, written last (RP-200), would claim ops the failed file lacks.
 *
 * <p>Each test drives its own {@link JournalSavedData} and its own data through the same {@code save(File, ...)}
 * vanilla calls, inside a world save's start and return ({@link WorldSaveOrder#asWorldSaveForTest}), with their files
 * under {@code <world>/mic_rtsim-test/<a directory of its own>/}, which the world save counts as one of its data
 * directories for the test ({@link WorldSaveOrder#EXTRA_DATA_DIRS_FOR_TEST}). A write fails the way a full disk fails
 * it: a directory stands where the data's file goes, so the rename onto it throws a real {@link IOException}. The
 * server's own journal and its data are never touched. Each test has a batch of its own: a failing file holds every
 * journal of its window, so two tests never overlap.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class SavedDataWriteHoldGameTests {
    private SavedDataWriteHoldGameTests() {}

    /** A SavedData the journal counts on. */
    static final class Pad extends SavedData implements JournalDependent {
        int value;

        @Override
        public CompoundTag save(CompoundTag tag, HolderLookup.Provider registries) {
            tag.putInt("value", value);
            return tag;
        }

        @Override
        public void mic_rtsim$remark() {
            setDirty();
        }
    }

    /** A SavedData the journal does not count on. */
    static final class Plain extends SavedData {
        @Override
        public CompoundTag save(CompoundTag tag, HolderLookup.Provider registries) {
            tag.putInt("value", 1);
            return tag;
        }
    }

    /**
     * The whole of it (U-g1). A good world save writes the data and the journal's file (W0). The data's write then
     * fails while the journal has moved on: vanilla has cleared the data's dirty flag (the premise), the failure is
     * recorded, and the journal's file stays byte for byte W0's, dirty for the next save: the held write, as a
     * chunk not yet on disk holds it (RP-205). The next saves mark the data dirty again at their start and try it
     * again, each holding the journal, with one record and one write attempt per save, no more. With the disk back,
     * the next world save writes the data (value 2) and then the journal's file, which is no longer W0.
     */
    @GameTest(template = RtsimTests.TEMPLATE, batch = "mic_rtsim_saved_data_write_hold_a", timeoutTicks = 6000)
    public static void aFailedSavedDataWriteHoldsTheJournalsFileUntilTheDataIsWritten(GameTestHelper helper) {
        Pad pad = new Pad();
        Kit kit = new Kit(helper.getLevel(), pad, true);
        byte[][] w0 = {null};
        long[] failing = {0};
        helper.startSequence()
                .thenExecute(kit.step(() -> {
                    check("a Pad is marked for the journal", (Object) pad instanceof JournalDependent);
                    pad.value = 1;
                    pad.setDirty();
                    kit.journalMoves();
                    kit.save();
                }))
                .thenWaitUntil(kit.looked())
                .thenExecute(kit.step(() -> {
                    Look l = kit.look.get();
                    record("aFailedSavedDataWriteHoldsTheJournalsFileUntilTheDataIsWritten", "W0", l.writes() + " journal writes, value " + l.value());
                    check("W0 wrote both: the data's value is 1 and the journal's file is there (" + l.value() + ", " + l.journal().length + " bytes)",
                            l.value() == 1 && l.journal().length > 0 && l.writes() == 1 && !l.dirty());
                    check("nothing of this directory failed", !kit.heldByFailedWrite());
                    w0[0] = l.journal();
                    failing[0] = SavedDataWrites.INSTANCE.failing();
                    kit.diskFull();
                    pad.value = 2;
                    pad.setDirty();
                    kit.journalMoves();
                    kit.save();
                }))
                .thenWaitUntil(kit.looked())
                .thenExecute(kit.step(() -> {
                    Look l = kit.look.get();
                    record("aFailedSavedDataWriteHoldsTheJournalsFileUntilTheDataIsWritten", "W1", l.writes() + " journal writes, held by a failed write "
                            + kit.journal.savedDataHeld() + ", " + SavedDataWrites.INSTANCE.describe());
                    check("the data's write failed and is recorded", kit.heldByFailedWrite());
                    check("the failure was seen on the IO chain, not on the server thread (thread " + SavedDataWrites.INSTANCE.lastFailureThreadId() + ")",
                            kit.failureSeenOffTheServerThread());
                    check("vanilla cleared the data's dirty flag, so the next save would not write it by itself (the premise)", !pad.isDirty());
                    check("the data was never written: no file, value " + l.value(), l.value() == -1);
                    check("the journal's file is byte for byte W0's, not written once more (" + l.writes() + " writes)",
                            Arrays.equals(w0[0], l.journal()) && l.writes() == 1);
                    check("the journal stays dirty for the next save", l.dirty());
                    check("the hold is counted", kit.journal.savedDataHeld() == 1);
                    // more saves with the disk still full: held each time, the data tried each time, one record
                    pad.value = 3;
                    kit.journalMoves();
                    kit.save();
                }))
                .thenWaitUntil(kit.looked())
                .thenExecute(kit.step(() -> {
                    Look l = kit.look.get();
                    check("held at the second save as well, the file still W0's: " + l.writes(), Arrays.equals(w0[0], l.journal()) && l.writes() == 1 && l.dirty());
                    check("the data was marked dirty again at that save's start and tried again: " + SavedDataWrites.INSTANCE.failures() + " failures since this test began",
                            SavedDataWrites.INSTANCE.failures() == kit.failuresBefore + 2);
                    check("the mark was made on the server thread, by the save's start, not by the IO chain", kit.remarkedOnTheServerThread());
                    check("the failure was seen on the IO chain again", kit.failureSeenOffTheServerThread());
                    check("one record for the file however often it fails", SavedDataWrites.INSTANCE.failing() == failing[0] + 1);
                    check("two holds counted", kit.journal.savedDataHeld() == 2);
                    kit.journalMoves();
                    kit.save();
                }))
                .thenWaitUntil(kit.looked())
                .thenExecute(kit.step(() -> {
                    Look l = kit.look.get();
                    check("held at the third save as well: " + l.writes(), Arrays.equals(w0[0], l.journal()) && l.writes() == 1 && l.dirty());
                    check("still one record", SavedDataWrites.INSTANCE.failing() == failing[0] + 1);
                    check("three holds counted", kit.journal.savedDataHeld() == 3);
                    // the disk is back: this save writes the data, then the journal
                    kit.diskFree();
                    pad.value = 4;
                    kit.journalMoves();
                    kit.save();
                }))
                .thenWaitUntil(kit.looked())
                .thenExecute(kit.step(() -> {
                    Look l = kit.look.get();
                    record("aFailedSavedDataWriteHoldsTheJournalsFileUntilTheDataIsWritten", "after the disk is back",
                            l.writes() + " journal writes, value " + l.value() + ", " + SavedDataWrites.INSTANCE.describe());
                    check("the data is written: value 4, not the 2 it held when it first failed (" + l.value() + ")", l.value() == 4);
                    check("then the journal's file is written, and is not W0's (" + l.writes() + " writes)", l.writes() == 2 && !Arrays.equals(w0[0], l.journal()));
                    check("nothing is dirty any more", !l.dirty() && !pad.isDirty());
                    check("the record is gone", !kit.heldByFailedWrite() && SavedDataWrites.INSTANCE.failing() == failing[0]);
                    check("three holds in all", kit.journal.savedDataHeld() == 3);
                    kit.cleanup();
                })).thenSucceed();
    }

    /**
     * The same on a data class of the mod itself: {@link IndustryData}, one of the files the journal counts on. A failed
     * write of it holds the journal's file, and the next world save writes it again without any change to it.
     */
    @GameTest(template = RtsimTests.TEMPLATE, batch = "mic_rtsim_saved_data_write_hold_b", timeoutTicks = 6000)
    public static void aFailedWriteOfTheIndustryDataHoldsTheJournalsFileToo(GameTestHelper helper) {
        IndustryData industry = IndustryData.FACTORY.constructor().get();
        Kit kit = new Kit(helper.getLevel(), industry, true);
        byte[][] w0 = {null};
        helper.startSequence()
                .thenExecute(kit.step(() -> {
                    check("IndustryData is marked for the journal", (Object) industry instanceof JournalDependent);
                    check("so is VillageRegistry", (Object) VillageRegistry.FACTORY.constructor().get() instanceof JournalDependent);
                    industry.setDirty();
                    kit.journalMoves();
                    kit.save();
                }))
                .thenWaitUntil(kit.looked())
                .thenExecute(kit.step(() -> {
                    Look l = kit.look.get();
                    check("W0 wrote the journal's file: " + l.writes(), l.journal().length > 0 && l.writes() == 1);
                    check("and the industry data's: " + Files.isRegularFile(kit.dataFile), Files.isRegularFile(kit.dataFile));
                    w0[0] = l.journal();
                    kit.diskFull();
                    industry.setDirty();
                    kit.journalMoves();
                    kit.save();
                }))
                .thenWaitUntil(kit.looked())
                .thenExecute(kit.step(() -> {
                    Look l = kit.look.get();
                    check("the failed write of the industry data is recorded", kit.heldByFailedWrite());
                    check("the journal's file stays W0's and dirty: " + l.writes(), Arrays.equals(w0[0], l.journal()) && l.writes() == 1 && l.dirty());
                    check("vanilla cleared the industry data's dirty flag (the premise)", !industry.isDirty());
                    kit.diskFree();
                    kit.journalMoves(); // nothing of the industry data changes: only the mark brings it to this save
                    kit.save();
                }))
                .thenWaitUntil(kit.looked())
                .thenExecute(kit.step(() -> {
                    Look l = kit.look.get();
                    check("the industry data is written again without a change to it: " + Files.isRegularFile(kit.dataFile), Files.isRegularFile(kit.dataFile));
                    check("then the journal's file: " + l.writes(), l.writes() == 2 && !Arrays.equals(w0[0], l.journal()) && !l.dirty());
                    kit.cleanup();
                })).thenSucceed();
    }

    /**
     * The hold is for the save the file belongs to (U-g1): a data that failed in a directory the world save does not
     * count as its own holds nothing, and a SavedData that is not marked is not recorded at all.
     */
    @GameTest(template = RtsimTests.TEMPLATE, batch = "mic_rtsim_saved_data_write_hold_c", timeoutTicks = 6000)
    public static void aFailedWriteOfAnotherDirectoryOrAnUnmarkedDataHoldsNothing(GameTestHelper helper) {
        Pad elsewhere = new Pad();
        Kit outside = new Kit(helper.getLevel(), elsewhere, false);
        Plain plain = new Plain();
        Kit unmarked = new Kit(helper.getLevel(), plain, true);
        long[] failures = {0};
        helper.startSequence()
                .thenExecute(outside.step(() -> {
                    outside.diskFull();
                    elsewhere.value = 1;
                    elsewhere.setDirty();
                    outside.journalMoves();
                    outside.save();
                }))
                .thenWaitUntil(outside.looked())
                .thenExecute(outside.step(() -> {
                    Look l = outside.look.get();
                    check("the failed write is recorded", outside.heldByFailedWrite() && SavedDataWrites.INSTANCE.failures() == outside.failuresBefore + 1);
                    check("but its directory is not one of the world save's, so the journal's file is written all the same: " + l.writes(),
                            l.writes() == 1 && l.journal().length > 0 && !l.dirty());
                    outside.cleanup();
                    failures[0] = SavedDataWrites.INSTANCE.failures();
                    unmarked.diskFull();
                    plain.setDirty();
                    unmarked.journalMoves();
                    unmarked.save();
                }))
                .thenWaitUntil(unmarked.looked())
                .thenExecute(unmarked.step(() -> {
                    Look l = unmarked.look.get();
                    check("a SavedData that is not marked is not recorded", SavedDataWrites.INSTANCE.failures() == failures[0] && !unmarked.heldByFailedWrite());
                    check("the journal's file is written: " + l.writes(), l.writes() == 1 && l.journal().length > 0 && !l.dirty());
                    unmarked.cleanup();
                })).thenSucceed();
    }
}
