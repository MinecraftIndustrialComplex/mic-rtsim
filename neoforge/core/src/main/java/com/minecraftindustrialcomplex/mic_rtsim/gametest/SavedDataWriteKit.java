package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.journal.JournalSavedData;
import com.minecraftindustrialcomplex.mic_rtsim.journal.SavedDataWrites;
import com.minecraftindustrialcomplex.mic_rtsim.journal.WorldSaveOrder;
import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.saveddata.SavedData;
import net.minecraft.world.level.storage.LevelResource;
import net.neoforged.neoforge.common.IOUtilities;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Comparator;
import java.util.List;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicReference;
import java.util.stream.Stream;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.io;

/**
 * The fixture of the tests of a failed SavedData write holding the journal's file (docs/choices-journal-barrier.md U-g1;
 * {@link SavedDataWriteHoldGameTests}, and the Create plugin's test of {@code create_tracks}): a test's own journal and
 * its own data, their files under {@code <world>/mic_rtsim-test/<a directory of its own>/}, saved the way vanilla saves
 * them, inside a world save ({@link WorldSaveOrder#asWorldSaveForTest}). A write fails the way a full disk fails it: a
 * directory stands where the data's file goes, so the rename onto it throws a real {@link IOException}.
 */
public final class SavedDataWriteKit {
    private SavedDataWriteKit() {}

    /** What the IO chain saw after a save's last task. */
    public record Look(byte[] journal, int value, long writes, boolean dirty) {}

    /** One test's directory, journal and data. */
    public static final class Kit {
        public final ServerLevel level;
        public final Path dir, dataFile, journalFile;
        public final SavedData data;
        public final JournalSavedData journal = JournalSavedData.fresh();
        public final AtomicReference<Look> look = new AtomicReference<>();
        public final long failuresBefore = SavedDataWrites.INSTANCE.failures();

        /** {@code counted}: the directory is one the world save counts as its own (a failed write there holds the journal). */
        public Kit(ServerLevel level, SavedData data, boolean counted) {
            this.level = level;
            this.data = data;
            try {
                dir = level.getServer().getWorldPath(LevelResource.ROOT).resolve("mic_rtsim-test").resolve("saved-data-write-" + UUID.randomUUID());
                Files.createDirectories(dir);
            } catch (IOException e) {
                throw new RuntimeException(e);
            }
            dataFile = dir.resolve("data.dat");
            journalFile = dir.resolve("journal.dat");
            if (counted)
                WorldSaveOrder.EXTRA_DATA_DIRS_FOR_TEST.add(dir);
        }

        /** The journal moves on, as every applied op moves it: a write of it would differ from the last. */
        public void journalMoves() {
            journal.journal().reserveBeyond(journal.journal().nextObsSeq() + 10);
            journal.setDirty();
        }

        /** A directory where the data's file goes: the rename of a write onto it throws. */
        public void diskFull() {
            try {
                Files.deleteIfExists(dataFile);
                Files.createDirectory(dataFile);
            } catch (IOException e) {
                throw new RuntimeException(e);
            }
        }

        public void diskFree() {
            try {
                Files.deleteIfExists(dataFile);
            } catch (IOException e) {
                throw new RuntimeException(e);
            }
        }

        /** A world save: the data's save, then the journal's, which queues its write last (RP-200). */
        public void save() {
            look.set(null);
            HolderLookup.Provider reg = level.registryAccess();
            WorldSaveOrder.asWorldSaveForTest(level.getServer(), () -> {
                data.save(dataFile.toFile(), reg);
                journal.save(journalFile.toFile(), reg);
            });
            IOUtilities.withIOWorker(() -> look.set(new Look(bytesOrEmpty(journalFile), valueOrMinusOne(dataFile), journal.writes(), journal.isDirty())));
        }

        public Runnable looked() {
            return io(() -> check("the IO chain looked", look.get() != null));
        }

        /** Runs a step; a failed check cleans up first, so a failure here does not hold the next test's journal. */
        public Runnable step(Runnable body) {
            return () -> {
                try {
                    body.run();
                } catch (RuntimeException | Error e) {
                    cleanup();
                    throw e;
                }
            };
        }

        public void cleanup() {
            SavedDataWrites.INSTANCE.forgetForTest(dir);
            WorldSaveOrder.EXTRA_DATA_DIRS_FOR_TEST.remove(dir);
            try (Stream<Path> s = Files.walk(dir)) {
                s.sorted(Comparator.reverseOrder()).forEach(p -> {
                    try {
                        Files.deleteIfExists(p);
                    } catch (IOException ignored) {
                    }
                });
            } catch (IOException ignored) {
            }
        }

        /** Whether the journal's write is held by a failed file of this test's directory. */
        public boolean heldByFailedWrite() {
            return SavedDataWrites.INSTANCE.firstFailing(List.of(dir)) != null;
        }
    }

    /** A file's bytes, or none when it is not a regular file; on the IO chain, so it never throws. */
    static byte[] bytesOrEmpty(Path file) {
        try {
            return Files.isRegularFile(file) ? Files.readAllBytes(file) : new byte[0];
        } catch (IOException | RuntimeException e) {
            return new byte[0];
        }
    }

    /** The {@code value} of a Pad's file, -1 when there is none; on the IO chain. */
    static int valueOrMinusOne(Path file) {
        try {
            return Files.isRegularFile(file) ? NbtIo.readCompressed(file, NbtAccounter.unlimitedHeap()).getCompound("data").getInt("value") : -1;
        } catch (IOException | RuntimeException e) {
            return -1;
        }
    }
}
