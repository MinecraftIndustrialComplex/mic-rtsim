package com.minecraftindustrialcomplex.mic_rtsim.bridge;

import com.minecraftindustrialcomplex.mic_rtsim.bridge.ReinstallRules.ModList;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.ReinstallRules.Verdict;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The reinstall guard's rules (UR-1, docs/choices-reinstall-guard.md RG1 to RG4): what each start sees in
 * {@code level.dat} and the world directory, and whether it resets. Each scenario test is one of the edge cases
 * the guard must tell apart, with the files a real start would find.
 */
final class ReinstallRulesTest {
    private static final List<String> WITH = List.of("minecraft", "neoforge", "create", "mic_native", "mic_rtsim", "mic_rtsim_create");
    private static final List<String> WITHOUT = List.of("minecraft", "neoforge", "create");

    @TempDir
    Path world;

    /** What one start decides: the mod list of the level.dat it read (null: none read), and whether a marker is there after the read. */
    private boolean start(boolean hasFml, List<String> ids) throws IOException {
        Verdict v = null;
        if (ids != null || !hasFml) {
            v = ReinstallRules.decide(ReinstallRules.classify(hasFml, ids), ReinstallRules.stateExists(world));
            if (v == Verdict.GAP)
                ReinstallRules.writeMarker(world, "test gap\n");
        }
        return ReinstallRules.resets(v, ReinstallRules.readMarker(world) != null);
    }

    private void rtsimRan() throws IOException {
        Files.createDirectories(world.resolve("rtsim"));
        Files.writeString(world.resolve("rtsim/pages-0.bin"), "pages");
        Files.createDirectories(world.resolve("data"));
        Files.writeString(world.resolve("data/mic_rtsim_journal.dat"), "journal");
        Files.writeString(world.resolve("data/mic_rtsim_industry.dat"), "industry");
    }

    @Test
    void classifiesTheModList() {
        assertEquals(ModList.NO_FML, ReinstallRules.classify(false, null));
        assertEquals(ModList.MALFORMED, ReinstallRules.classify(true, null));
        assertEquals(ModList.MALFORMED, ReinstallRules.classify(true, List.of()));
        assertEquals(ModList.LISTS_RTSIM, ReinstallRules.classify(true, WITH));
        assertEquals(ModList.LACKS_RTSIM, ReinstallRules.classify(true, WITHOUT));
        // a plugin's id is not the core's
        assertEquals(ModList.LACKS_RTSIM, ReinstallRules.classify(true, List.of("minecraft", "mic_rtsim_create")));
    }

    @Test
    void decidesOnlyWithRtsimState() {
        for (boolean state : new boolean[] {false, true}) {
            assertEquals(Verdict.NO_GAP, ReinstallRules.decide(ModList.LISTS_RTSIM, state));
            assertEquals(Verdict.UNKNOWN, ReinstallRules.decide(ModList.MALFORMED, state));
        }
        assertEquals(Verdict.GAP, ReinstallRules.decide(ModList.LACKS_RTSIM, true));
        assertEquals(Verdict.GAP, ReinstallRules.decide(ModList.NO_FML, true));
        assertEquals(Verdict.FIRST_INSTALL, ReinstallRules.decide(ModList.LACKS_RTSIM, false));
        assertEquals(Verdict.FIRST_INSTALL, ReinstallRules.decide(ModList.NO_FML, false));
        assertFalse(ReinstallRules.resets(null, false));
        assertFalse(ReinstallRules.resets(Verdict.NO_GAP, false));
        assertFalse(ReinstallRules.resets(Verdict.UNKNOWN, false));
        assertTrue(ReinstallRules.resets(Verdict.GAP, false));
        assertTrue(ReinstallRules.resets(Verdict.NO_GAP, true));
        assertTrue(ReinstallRules.resets(null, true));
    }

    @Test
    void aNormalRestartOrACrashIsNoGap() throws IOException {
        rtsimRan();
        // a stop and a crash leave the same level.dat: the last save's, which listed rtsim
        assertFalse(start(true, WITH));
        assertFalse(start(true, WITH));
        assertNull(ReinstallRules.readMarker(world));
    }

    @Test
    void aReinstallGapResetsAndACrashInItsStartStillOwesTheReset() throws IOException {
        rtsimRan();
        assertTrue(start(true, WITHOUT), "the world ran without rtsim");
        assertEquals("test gap", ReinstallRules.readMarker(world));
        // The start crashed after rewriting level.dat (now with rtsim) and before the reset saved: the marker still owes it.
        assertTrue(start(true, WITH));
        // The reset saved: the marker goes, the next start is normal.
        ReinstallRules.deleteMarker(world);
        assertFalse(start(true, WITH));
    }

    @Test
    void aServerWithoutNeoForgeIsAGap() throws IOException {
        rtsimRan();
        assertTrue(start(false, null));
    }

    @Test
    void aFirstInstallIsNoReset() throws IOException {
        assertFalse(start(true, WITHOUT));
        Files.createDirectories(world.resolve("rtsim")); // an empty rtsim/ is no state
        Files.createDirectories(world.resolve("data"));
        Files.writeString(world.resolve("data/raids.dat"), "vanilla");
        assertFalse(start(true, WITHOUT));
        assertNull(ReinstallRules.readMarker(world));
    }

    @Test
    void aDeliberateSimResetIsNoGap() throws IOException {
        rtsimRan();
        // runbook 8.6: world/rtsim/ deleted with rtsim installed; level.dat still lists it
        Files.delete(world.resolve("rtsim/pages-0.bin"));
        Files.delete(world.resolve("rtsim"));
        assertFalse(start(true, WITH));
    }

    @Test
    void backupRestores() throws IOException {
        rtsimRan();
        // a whole backup taken while rtsim ran: no gap
        assertFalse(start(true, WITH));
        // a backup without world/rtsim/: the world's data files still say rtsim ran, and level.dat lists it: no gap, the engine opens empty pages and the modules re-announce (observationsLost)
        Files.delete(world.resolve("rtsim/pages-0.bin"));
        Files.delete(world.resolve("rtsim"));
        assertFalse(start(true, WITH));
        // a backup taken during a gap, without world/rtsim/: the data files are rtsim's state, so it is a gap
        assertTrue(start(true, WITHOUT));
    }

    @Test
    void aFormatChangeIsNoGap() throws IOException {
        rtsimRan();
        // an update changes rtsim's version, not its presence; the mod list names ids
        assertFalse(start(true, WITH));
    }

    @Test
    void anUnreadableListDecidesNothing() throws IOException {
        rtsimRan();
        assertFalse(start(true, List.of()));
    }

    @Test
    void theMarkerRoundTrips() throws IOException {
        assertNull(ReinstallRules.readMarker(world));
        ReinstallRules.writeMarker(world, "first line\nsecond line\n");
        assertEquals("first line", ReinstallRules.readMarker(world));
        assertFalse(Files.exists(world.resolve(ReinstallRules.MARKER + ".tmp")));
        ReinstallRules.writeMarker(world, "");
        assertNotEquals(null, ReinstallRules.readMarker(world), "an empty marker still owes the reset");
        ReinstallRules.deleteMarker(world);
        assertNull(ReinstallRules.readMarker(world));
        ReinstallRules.deleteMarker(world); // twice is fine
    }

    @Test
    void thePagesMoveAsideWhole() throws IOException {
        Instant at = Instant.parse("2026-10-23T12:00:00Z");
        assertNull(ReinstallRules.moveAside(world, at), "no rtsim/: nothing to move");
        rtsimRan();
        Path a = ReinstallRules.moveAside(world, at);
        assertEquals("rtsim-reinstall-20261023-120000", a.getFileName().toString());
        assertEquals("pages", Files.readString(a.resolve("pages-0.bin")));
        assertFalse(Files.exists(world.resolve("rtsim")));
        // the next reset in the same second gets its own directory
        rtsimRan();
        Path b = ReinstallRules.moveAside(world, at);
        assertEquals("rtsim-reinstall-20261023-120000-2", b.getFileName().toString());
        assertTrue(Files.exists(a.resolve("pages-0.bin")));
        // the moved pages are not rtsim state
        Files.delete(world.resolve("data/mic_rtsim_journal.dat"));
        Files.delete(world.resolve("data/mic_rtsim_industry.dat"));
        assertFalse(ReinstallRules.stateExists(world));
    }
}
