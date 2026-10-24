package com.minecraftindustrialcomplex.mic_rtsim.bridge;

import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.charset.StandardCharsets;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.nio.file.StandardOpenOption;
import java.time.Instant;
import java.time.ZoneOffset;
import java.time.format.DateTimeFormatter;
import java.util.List;

/**
 * The reinstall guard's decisions and file steps (UR-1), with no Minecraft class, so plain unit tests run them
 * ({@code ReinstallRulesTest}). {@link ReinstallGuard} reads {@code level.dat} and the server with them.
 */
public final class ReinstallRules {
    private ReinstallRules() {}

    /** The core mod's id, as {@code level.dat}'s mod list names it. */
    public static final String MODID = "mic_rtsim";
    /** The marker's name in the world directory. */
    public static final String MARKER = "mic_rtsim-reinstall.txt";
    /** The prefix of the directory the reset moves {@code world/rtsim/} to. */
    public static final String ASIDE_PREFIX = "rtsim-reinstall-";

    /** What {@code level.dat}'s mod list says about rtsim. */
    public enum ModList {
        /** {@code mic_rtsim} is in the list: the world last saved with rtsim. */
        LISTS_RTSIM,
        /** A list without {@code mic_rtsim}: the world last saved under NeoForge without rtsim. */
        LACKS_RTSIM,
        /** No {@code fml} tag: something that is not NeoForge wrote {@code level.dat} last. */
        NO_FML,
        /** An {@code fml} tag without a readable list: decides nothing. */
        MALFORMED
    }

    /** What the guard concludes at a {@code level.dat} read. */
    public enum Verdict {
        /** rtsim saved this world last. */
        NO_GAP,
        /** No rtsim state in the world: nothing to reset. */
        FIRST_INSTALL,
        /** The world ran without rtsim since rtsim's last save: reset. */
        GAP,
        /** The mod list could not be read: no reset, a warning. */
        UNKNOWN
    }

    /** What a mod list says: {@code hasFml} the root has an {@code fml} compound, {@code ids} its list's mod ids, or null when it has none. */
    public static ModList classify(boolean hasFml, List<String> ids) {
        if (!hasFml)
            return ModList.NO_FML;
        if (ids == null || ids.isEmpty())
            return ModList.MALFORMED;
        return ids.contains(MODID) ? ModList.LISTS_RTSIM : ModList.LACKS_RTSIM;
    }

    /** The verdict: a mod list without rtsim is a gap only in a world that holds rtsim's state. */
    public static Verdict decide(ModList list, boolean stateExists) {
        return switch (list) {
            case LISTS_RTSIM -> Verdict.NO_GAP;
            case MALFORMED -> Verdict.UNKNOWN;
            case LACKS_RTSIM, NO_FML -> stateExists ? Verdict.GAP : Verdict.FIRST_INSTALL;
        };
    }

    /**
     * Whether a start resets: a marker is there (left by this start's read, or by an earlier start that crashed
     * before its reset saved), or this start's read saw a gap ({@code readVerdict}, null when nothing was read).
     */
    public static boolean resets(Verdict readVerdict, boolean markerExists) {
        return markerExists || readVerdict == Verdict.GAP;
    }

    /** Whether the world holds rtsim's state: a non-empty {@code rtsim/}, or any {@code data/mic_rtsim*.dat}. */
    public static boolean stateExists(Path worldDir) throws IOException {
        Path pages = worldDir.resolve("rtsim");
        if (Files.isDirectory(pages))
            try (DirectoryStream<Path> s = Files.newDirectoryStream(pages)) {
                if (s.iterator().hasNext())
                    return true;
            }
        Path data = worldDir.resolve("data");
        if (Files.isDirectory(data))
            try (DirectoryStream<Path> s = Files.newDirectoryStream(data, "mic_rtsim*.dat")) {
                if (s.iterator().hasNext())
                    return true;
            }
        return false;
    }

    /** Writes the marker and syncs it and its directory: after this the reset is owed even if the start crashes. */
    public static void writeMarker(Path worldDir, String text) throws IOException {
        Path tmp = worldDir.resolve(MARKER + ".tmp");
        try (FileChannel ch = FileChannel.open(tmp, StandardOpenOption.CREATE, StandardOpenOption.WRITE, StandardOpenOption.TRUNCATE_EXISTING)) {
            ByteBuffer b = ByteBuffer.wrap(text.getBytes(StandardCharsets.UTF_8));
            while (b.hasRemaining())
                ch.write(b);
            ch.force(true);
        }
        Files.move(tmp, worldDir.resolve(MARKER), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
        syncDir(worldDir);
    }

    /** The marker's first line (the reason), or null without a marker. An empty marker still owes the reset. */
    public static String readMarker(Path worldDir) throws IOException {
        Path m = worldDir.resolve(MARKER);
        if (!Files.exists(m))
            return null;
        String text = Files.readString(m, StandardCharsets.UTF_8).strip();
        int nl = text.indexOf('\n');
        String first = nl < 0 ? text : text.substring(0, nl);
        return first.isEmpty() ? "a reinstall marker was left by an earlier start" : first;
    }

    /** Deletes the marker and syncs the directory. */
    public static void deleteMarker(Path worldDir) throws IOException {
        Files.deleteIfExists(worldDir.resolve(MARKER));
        syncDir(worldDir);
    }

    /**
     * Moves {@code worldDir/rtsim} to a fresh {@code rtsim-reinstall-<time>} beside it, and syncs the directory.
     * Returns where it went, or null when there was no {@code rtsim/}. Throws when it cannot.
     */
    public static Path moveAside(Path worldDir, Instant at) throws IOException {
        Path pages = worldDir.resolve("rtsim");
        if (!Files.exists(pages))
            return null;
        String stamp = DateTimeFormatter.ofPattern("yyyyMMdd-HHmmss").withZone(ZoneOffset.UTC).format(at);
        Path aside = worldDir.resolve(ASIDE_PREFIX + stamp);
        for (int i = 2; Files.exists(aside); i++)
            aside = worldDir.resolve(ASIDE_PREFIX + stamp + "-" + i);
        Files.move(pages, aside, StandardCopyOption.ATOMIC_MOVE);
        syncDir(worldDir);
        return aside;
    }

    // ---- the engine life ------------------------------------------------------------------------

    /** The engine life's file, inside the engine's page directory, so it goes wherever the pages go. */
    public static final String LIFE_FILE = "bridge-life.txt";

    /**
     * The engine life for a session (RG13): a token that changes exactly when the engine's state starts
     * over. An engine whose pages are new ({@code pagesFresh}: they include no observation) numbers every
     * district's reconcile epochs from 1 again, so it gets a new token, written and synced before anything
     * is stamped with it. Pages that are not new keep the token in {@code pagesDir}, or 0 when there is
     * none (pages from before the token, whose chunk entries carry none either). The file lives in the
     * page directory itself: deleting, moving or restoring {@code world/rtsim/} takes the token with it.
     */
    public static long engineLife(Path pagesDir, boolean pagesFresh, java.util.function.LongSupplier random) throws IOException {
        Path f = pagesDir.resolve(LIFE_FILE);
        if (!pagesFresh) {
            if (Files.exists(f))
                return Long.parseUnsignedLong(Files.readString(f, StandardCharsets.UTF_8).strip(), 16);
            writeLife(pagesDir, 0);
            return 0;
        }
        long life = 0;
        while (life == 0)
            life = random.getAsLong();
        writeLife(pagesDir, life);
        return life;
    }

    private static void writeLife(Path pagesDir, long life) throws IOException {
        Files.createDirectories(pagesDir);
        Path tmp = pagesDir.resolve(LIFE_FILE + ".tmp");
        try (FileChannel ch = FileChannel.open(tmp, StandardOpenOption.CREATE, StandardOpenOption.WRITE, StandardOpenOption.TRUNCATE_EXISTING)) {
            ByteBuffer b = ByteBuffer.wrap((Long.toHexString(life) + "\n").getBytes(StandardCharsets.UTF_8));
            while (b.hasRemaining())
                ch.write(b);
            ch.force(true);
        }
        Files.move(tmp, pagesDir.resolve(LIFE_FILE), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
        syncDir(pagesDir);
    }

    /** Syncs a directory's entries (a rename's durability) where the platform allows it. */
    static void syncDir(Path dir) {
        try (FileChannel ch = FileChannel.open(dir, StandardOpenOption.READ)) {
            ch.force(true);
        } catch (IOException | UnsupportedOperationException e) {
            // Windows cannot open a directory: the rename is done, its durability is the OS's.
        }
    }
}
