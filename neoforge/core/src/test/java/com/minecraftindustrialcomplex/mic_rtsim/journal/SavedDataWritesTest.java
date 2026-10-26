package com.minecraftindustrialcomplex.mic_rtsim.journal;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.IOException;
import java.lang.ref.WeakReference;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The record of failed SavedData writes the journal's write holds on (docs/choices-journal-barrier.md U-g1): logged
 * once per streak, scoped by directory, bounded, and never a reason for the IO chain to throw.
 */
class SavedDataWritesTest {
    @TempDir
    Path dir;

    private final List<String> errors = new ArrayList<>();
    private final List<String> infos = new ArrayList<>();
    private final SavedDataWrites writes = new SavedDataWrites(errors::add, infos::add);

    /** A data that counts how often it was marked dirty again. */
    private static final class Data implements JournalDependent {
        final AtomicInteger remarks = new AtomicInteger();

        @Override
        public void mic_rtsim$remark() {
            remarks.incrementAndGet();
        }
    }

    @Test
    void aFailureIsLoggedOncePerStreakAndTheRestAreCounted() {
        Data d = new Data();
        Path file = dir.resolve("data").resolve("create_tracks.dat");
        for (int i = 0; i < 5; i++)
            writes.failed(d, file, new IOException("No space left on device"));
        assertEquals(1, errors.size(), "logged once: " + errors);
        assertTrue(errors.get(0).contains("create_tracks.dat") && errors.get(0).contains("No space left on device"), errors.get(0));
        assertEquals(5, writes.failures());
        assertEquals(1, writes.failing(), "one entry per file, however often it fails");
        assertTrue(infos.isEmpty());

        writes.written(file);
        assertEquals(0, writes.failing());
        assertEquals(1, writes.recovered());
        assertEquals(1, infos.size(), "the end of the streak is said once: " + infos);
        writes.written(file);
        assertEquals(1, infos.size(), "a write of a file that is not failing says nothing");

        writes.failed(d, file, new IOException("again"));
        assertEquals(2, errors.size(), "a new streak is logged again: " + errors);
    }

    @Test
    void theRecordNeverGrowsPastOneEntryPerFile() {
        Data d = new Data();
        Path file = dir.resolve("x.dat");
        for (int i = 0; i < 10_000; i++)
            writes.failed(d, file, new IOException("full"));
        assertEquals(1, writes.failing());
        assertEquals(1, errors.size());
        Path other = dir.resolve("y.dat");
        writes.failed(d, other, new IOException("full"));
        assertEquals(2, writes.failing());
        assertEquals(2, errors.size(), "each file's first failure is logged");
    }

    @Test
    void aFileIsFoundByItsDirectoryAndOnlyThere() {
        Data d = new Data();
        Path mine = dir.resolve("world").resolve("data");
        Path sibling = dir.resolve("world2").resolve("data");
        writes.failed(d, mine.resolve("a.dat"), new IOException("full"));
        assertNotNull(writes.firstFailing(List.of(mine)));
        assertTrue(writes.firstFailing(List.of(sibling, mine)).contains("a.dat"));
        assertNull(writes.firstFailing(List.of(sibling)), "another world's directory does not see it");
        assertNull(writes.firstFailing(List.of(dir.resolve("world").resolve("dat"))), "a name that merely starts alike is another directory");
        assertNull(writes.firstFailing(List.of()));
        assertNotNull(writes.firstFailing(List.of(dir.resolve("world"))), "a parent directory sees it");
        assertNotNull(writes.firstFailing(List.of(mine.resolve("..").resolve("data"))), "paths are compared normalized");
        writes.written(mine.resolve(".").resolve("a.dat"));
        assertNull(writes.firstFailing(List.of(mine)), "a write by another spelling of the same path ends it");
    }

    @Test
    void aWorldSavesStartMarksEveryFailedDataDirtyAgainOnce() {
        Data a = new Data(), b = new Data();
        writes.failed(a, dir.resolve("a.dat"), new IOException("full"));
        writes.failed(b, dir.resolve("b.dat"), new IOException("full"));
        writes.failed(a, dir.resolve("a.dat"), new IOException("full"));
        writes.remarkAll();
        assertEquals(1, a.remarks.get());
        assertEquals(1, b.remarks.get());
        assertEquals(2, writes.remarked());
        assertEquals(2, writes.failing(), "marked again, not yet written: still failing");
        writes.remarkAll();
        assertEquals(2, a.remarks.get(), "every world save marks it again until it is written");
        writes.written(dir.resolve("a.dat"));
        writes.remarkAll();
        assertEquals(2, a.remarks.get(), "a written file is not marked again");
        assertEquals(3, b.remarks.get());
    }

    /** The data a failing file belonged to was dropped (its level unloaded): the entry goes, said once, and the hold ends. */
    @Test
    void aDataThatWasDroppedEndsItsHoldAndIsSaidSo() throws InterruptedException {
        Path file = dir.resolve("gone.dat");
        WeakReference<Data> ref = failOnDroppableData(file);
        for (int i = 0; i < 100 && ref.get() != null; i++) {
            System.gc();
            Thread.sleep(10);
        }
        assertNull(ref.get(), "the record holds its data weakly, so a dropped data can be collected");
        assertNotNull(writes.firstFailing(List.of(dir)), "until it is noticed, it holds");
        writes.remarkAll();
        assertEquals(1, writes.abandoned());
        assertEquals(0, writes.failing());
        assertNull(writes.firstFailing(List.of(dir)), "the hold ended");
        assertEquals(2, errors.size(), "the failure and the abandon: " + errors);
        assertTrue(errors.get(1).contains("gone.dat") && errors.get(1).contains("is gone"), errors.get(1));
        writes.remarkAll();
        assertEquals(2, errors.size(), "said once");
    }

    private WeakReference<Data> failOnDroppableData(Path file) {
        Data d = new Data();
        writes.failed(d, file, new IOException("full"));
        return new WeakReference<>(d);
    }

    /** The log is not the write's business: a sink that throws changes nothing, and nothing escapes into the IO chain. */
    @Test
    void aLoggerThatThrowsNeverReachesTheWrite() {
        SavedDataWrites loud = new SavedDataWrites(m -> {
            throw new IllegalStateException("logger down");
        }, m -> {
            throw new IllegalStateException("logger down");
        });
        Data d = new Data();
        Path file = dir.resolve("a.dat");
        loud.failed(d, file, new IOException("full"));
        assertEquals(1, loud.failing(), "recorded all the same");
        loud.written(file);
        assertEquals(0, loud.failing());
        loud.failed(d, file, new IOException("full"));
        loud.forgetForTest(dir);
        assertFalse(loud.failing() > 0);
    }
}
