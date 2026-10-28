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

    /** A server stopped in a JVM that goes on (an integrated server's next world): nothing of the last one is left (U-g11). */
    @Test
    void aStoppedServerLeavesNothingBehind() {
        Data d = new Data();
        writes.failed(d, dir.resolve("a.dat"), new IOException("full"));
        writes.failed(d, dir.resolve("b.dat"), new IOException("full"));
        writes.remarkAll();
        writes.written(dir.resolve("b.dat"));
        writes.reset();
        assertEquals(0, writes.failing());
        assertNull(writes.firstFailing(List.of(dir)));
        assertEquals(0, writes.failures());
        assertEquals(0, writes.recovered());
        assertEquals(0, writes.remarked());
        assertEquals(0, writes.abandoned());
        assertEquals(0, writes.gaveUp());
        assertEquals(-1, writes.lastFailureThreadId());
        assertEquals(-1, writes.lastRemarkThreadId());
        writes.failed(d, dir.resolve("a.dat"), new IOException("full again"));
        assertEquals(3, errors.size(), "a new server's first failure is logged again: " + errors);
    }

    /** More than one failing file: the journal's write goes ahead only when every one of them has written. */
    @Test
    void severalFailingFilesAllHaveToWriteBeforeTheHoldEnds() {
        Data d = new Data();
        Path a = dir.resolve("data").resolve("a.dat"), b = dir.resolve("data").resolve("b.dat");
        writes.failed(d, a, new IOException("full"));
        writes.failed(d, b, new IOException("full"));
        assertNotNull(writes.holdFor(List.of(dir)));
        writes.written(a);
        String still = writes.holdFor(List.of(dir));
        assertNotNull(still, "b still holds");
        assertTrue(still.contains("b.dat"), still);
        writes.written(b);
        assertNull(writes.holdFor(List.of(dir)));
    }

    /** The give-up (U-g12): after N held world saves a file stops holding; its record stays until it writes. */
    @Test
    void aFileStopsHoldingAfterTheLimitAndItsRecordStaysUntilItWrites() {
        Data d = new Data();
        Path file = dir.resolve("data").resolve("create_tracks.dat");
        writes.giveUpAfterForTest(3);
        writes.failed(d, file, new IOException("full"));
        for (int i = 1; i <= 3; i++) {
            assertNotNull(writes.holdFor(List.of(dir)), "held at world save " + i);
            assertEquals(0, writes.gaveUp());
        }
        assertTrue(writes.holds(file));
        assertNull(writes.holdFor(List.of(dir)), "the 4th save goes on");
        assertEquals(1, writes.gaveUp());
        assertEquals(2, errors.size(), "the failure and the give-up, each said once: " + errors);
        assertTrue(errors.get(1).contains("create_tracks.dat") && errors.get(1).contains("3"), errors.get(1));
        assertNull(writes.holdFor(List.of(dir)));
        assertNull(writes.firstFailing(List.of(dir)), "given up: the pure question says so too");
        assertFalse(writes.holds(file), "and so does the marker's");
        assertEquals(1, writes.gaveUp(), "counted once");
        assertEquals(2, errors.size(), "said once");
        writes.failed(d, file, new IOException("still full"));
        assertNull(writes.holdFor(List.of(dir)), "a later failure of the same streak does not hold again");
        assertEquals(1, writes.failing(), "the record stays: the data is still marked dirty at every world save");
        writes.remarkAll();
        assertEquals(1, d.remarks.get());

        writes.written(file);
        assertEquals(0, writes.failing());
        assertEquals(1, infos.size());
        assertTrue(infos.get(0).contains("given up"), infos.get(0));
        writes.failed(d, file, new IOException("full again"));
        assertNotNull(writes.holdFor(List.of(dir)), "a new streak holds again from its first save");
        assertEquals(1, writes.gaveUp());
    }

    /** One file given up does not release the journal while another still holds it. */
    @Test
    void aGivenUpFileLeavesTheOthersHolding() {
        Data d = new Data();
        Path a = dir.resolve("data").resolve("a.dat");
        writes.giveUpAfterForTest(1);
        writes.failed(d, a, new IOException("full"));
        assertNotNull(writes.holdFor(List.of(dir)));
        writes.failed(d, dir.resolve("data").resolve("b.dat"), new IOException("full"));
        String second = writes.holdFor(List.of(dir));
        assertNotNull(second, "a is given up at this save, b holds (its first)");
        assertTrue(second.contains("b.dat"), second);
        assertEquals(1, writes.gaveUp());
    }

    /** A data whose re-mark throws never aborts the world save, nor the other files' re-marks; the failure is said once (P3-8). */
    @Test
    void aReMarkThatThrowsIsLoggedOnceAndDoesNotStopTheOthers() {
        JournalDependent bad = () -> {
            throw new IllegalStateException("remark broke");
        };
        Data good = new Data();
        writes.failed(bad, dir.resolve("a.dat"), new IOException("full"));
        writes.failed(good, dir.resolve("b.dat"), new IOException("full"));
        writes.remarkAll();
        writes.remarkAll();
        assertEquals(2, good.remarks.get(), "the other file is marked at every save");
        long remarkErrors = errors.stream().filter(e -> e.contains("remark broke")).count();
        assertEquals(1, remarkErrors, "said once: " + errors);
        assertEquals(2, writes.failing(), "both are still on record");
    }
}
