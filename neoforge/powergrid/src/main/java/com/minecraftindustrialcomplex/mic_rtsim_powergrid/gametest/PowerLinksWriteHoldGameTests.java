package com.minecraftindustrialcomplex.mic_rtsim_powergrid.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.gametest.SavedDataWriteKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.SavedDataWriteKit.Kit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.SavedDataWriteKit.Look;
import com.minecraftindustrialcomplex.mic_rtsim.journal.JournalDependent;
import com.minecraftindustrialcomplex.mic_rtsim.journal.SavedDataWrites;
import com.minecraftindustrialcomplex.mic_rtsim_powergrid.MicRtsimPowerGrid;
import com.minecraftindustrialcomplex.mic_rtsim_powergrid.PowerLinks;
import net.minecraft.gametest.framework.AfterBatch;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.file.Files;
import java.util.Arrays;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;

/**
 * A failed write of the power links' file holds the journal's file (docs/choices-journal-barrier.md U-g3, U-g1). The links
 * ({@code mic_rtsim_powergrid_links}) are the dedup of IND_POWER_LINK: a link sent to the engine is on the books once the
 * journal's file says so, and the file that remembers which links were sent must be on disk first. The data is the plugin's
 * own {@link PowerLinks.Links}, saved by the same {@code save(File, ...)} vanilla calls, beside a test journal of its own
 * (core's {@link SavedDataWriteKit}); a write fails the way a full disk fails it, a directory where the file goes.
 */
@GameTestHolder(MicRtsimPowerGrid.MODID)
@PrefixGameTestTemplate(false)
public final class PowerLinksWriteHoldGameTests {
    private PowerLinksWriteHoldGameTests() {}

    @GameTest(template = "yard", batch = "mic_rtsim_powergrid_links_write_hold", timeoutTicks = 6000)
    public static void aFailedWriteOfThePowerLinksHoldsTheJournalsFileUntilItIsWritten(GameTestHelper helper) {
        PowerLinks.Links links = new PowerLinks.Links();
        Kit kit = new Kit(helper.getLevel(), links, true);
        byte[][] w0 = {null};
        helper.startSequence()
                .thenExecute(kit.step(() -> {
                    check("the power links are marked for the journal", (Object) links instanceof JournalDependent);
                    links.setDirty();
                    kit.journalMoves();
                    kit.save();
                }))
                .thenWaitUntil(kit.looked())
                .thenExecute(kit.step(() -> {
                    Look l = kit.look.get();
                    check("W0 wrote the journal's file and the links' (" + l.writes() + " writes)", l.writes() == 1 && l.journal().length > 0 && Files.isRegularFile(kit.dataFile));
                    w0[0] = l.journal();
                    kit.diskFull();
                    links.setDirty();
                    kit.journalMoves();
                    kit.save();
                }))
                .thenWaitUntil(kit.looked())
                .thenExecute(kit.step(() -> {
                    Look l = kit.look.get();
                    check("the links' write failed and is recorded", kit.heldByFailedWrite());
                    check("the journal's file is byte for byte W0's (" + l.writes() + " writes), dirty for the next save",
                            Arrays.equals(w0[0], l.journal()) && l.writes() == 1 && l.dirty());
                    kit.diskFree();
                    kit.journalMoves(); // nothing of the links changes: only the mark brings them to this save
                    kit.save();
                }))
                .thenWaitUntil(kit.looked())
                .thenExecute(kit.step(() -> {
                    Look l = kit.look.get();
                    check("the links are written again without a change to them", Files.isRegularFile(kit.dataFile));
                    check("then the journal's file (" + l.writes() + " writes)", l.writes() == 2 && !Arrays.equals(w0[0], l.journal()) && !l.dirty());
                    check("the record is gone", !kit.heldByFailedWrite() && SavedDataWrites.INSTANCE.failing() == 0);
                    kit.cleanup();
                })).thenSucceed();
    }

    @AfterBatch(batch = "mic_rtsim_powergrid_links_write_hold")
    public static void cleanUp(ServerLevel level) {
        SavedDataWriteKit.cleanupAll();
    }
}
