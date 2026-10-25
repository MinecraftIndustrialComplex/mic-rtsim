package com.minecraftindustrialcomplex.mic_rtsim.journal;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** The directory sync the journal's write makes first (RP-201): where it cannot sync, it skips, and says so once. */
class DirectorySyncTest {
    @TempDir
    Path dir;

    @Test
    void aDirectoryThatCannotBeSyncedIsSkippedAndLoggedOnce() {
        List<String> warnings = new ArrayList<>();
        DirectorySync.warn = warnings::add;
        long synced = DirectorySync.synced(), failed = DirectorySync.failed();
        assertTrue(DirectorySync.sync(dir), "a directory here syncs");
        assertEquals(synced + 1, DirectorySync.synced());
        // What Windows answers for any directory (Java cannot open one there): the open throws.
        Path missing = dir.resolve("no-such-dir");
        for (int i = 0; i < 3; i++)
            assertFalse(DirectorySync.sync(missing), "skipped, never thrown");
        assertEquals(failed + 3, DirectorySync.failed(), "every failure counted");
        assertEquals(1, DirectorySync.logged(), "logged once");
        assertEquals(1, warnings.size(), "one warning: " + warnings);
        assertTrue(warnings.get(0).contains("no-such-dir"), warnings.get(0));
        assertTrue(DirectorySync.sync(dir), "a later sync that can run still runs");
    }
}
