package com.minecraftindustrialcomplex.mic_rtsim.bridge;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.journal.Journal;
import com.minecraftindustrialcomplex.mic_rtsim.sites.SiteLedger;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.level.storage.LevelResource;
import net.minecraft.world.level.storage.LevelStorageSource;
import net.neoforged.fml.ModList;
import net.neoforged.neoforge.common.CommonHooks;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.stream.Stream;

/**
 * GameTests only: the resets and the readouts the site ledger's GameTests use (EQUILIBRIUM.md §2.3, slice S3, MZB-1016).
 * A reset is the whole server's, so a test that calls one runs alone in its batch and calls {@link #cleanUp} when it ends.
 *
 * <ul>
 *   <li>{@link #plainReset}: the session stops, the world saves, {@code world/rtsim} is moved aside and a session starts on a
 *       new engine. rtsim ran the whole time, so it is no gap (fork F9).
 *   <li>{@link #gapRestart}: the reinstall guard's own emulation ({@code ReinstallGameTests}): {@code level.dat} read with a mod
 *       list that lacks rtsim, the clock moved on by the gap, a restart; the guard moves the pages aside and the ledger
 *       counts the gap.
 *   <li>{@link #stop}: the session stops (an uninstall) and none runs until {@link #gapRestart}.
 * </ul>
 *
 * <p>Before either reset, the test asks for an engine save ({@link #requestSave}) and waits until it covers what the test
 * did ({@link #savedThrough}): the new engine replays only what the journal still holds, and the old pages, moved aside,
 * must not come back through it (RG13).
 */
public final class SiteTestHooks {
    private SiteTestHooks() {}

    private static void testsOnly() {
        if (!RtsimConfig.Test.ENABLED)
            throw new IllegalStateException("the site ledger's test hooks are for GameTests only");
    }

    /** The ledger's row for a site, or null. */
    public static SiteLedger.Row row(MinecraftServer server, int kind, long key) {
        return SiteLedger.get(server).row(kind, key);
    }

    /** The last observation sequence journaled so far (0 with none). */
    public static long journalEnd() {
        return RtsimSession.current().boundary().journal().nextObsSeq() - 1;
    }

    /**
     * Asks the engine to save its pages at its next step (an engine that takes scripted orders), and the world to save the
     * journal's file: the journal is trimmed only to what the file on disk holds (JB9), and a GameTest world does not save
     * itself, so a test waiting for {@link #savedThrough} would otherwise wait for whatever world save came by (a test's
     * result then followed the phase of the server's autosave clock, not the code).
     */
    public static void requestSave() {
        testsOnly();
        if (RtsimSession.current().engine() instanceof ScriptedEngine e) {
            e.requestSaveAtNextStep();
            RtsimSession.current().server().saveEverything(true, false, true);
        } else
            throw new IllegalStateException("the session's engine takes no scripted orders: " + RtsimSession.current().engine().describe());
    }

    /** Whether an acknowledged engine save covers every observation up to {@code seq}: the journal holds none of them. */
    public static boolean savedThrough(long seq) {
        Journal j = RtsimSession.current().boundary().journal();
        return j.size() == 0 ? j.nextObsSeq() > seq : j.firstSeq() > seq;
    }

    /**
     * A plain reset: the session stops (every puppet captured), the world saves, the engine's pages go aside and a session
     * starts on a new engine. Returns where the pages went.
     */
    public static Path plainReset(MinecraftServer server) {
        testsOnly();
        RtsimSession.stoppingGlobal();
        server.saveEverything(true, true, true);
        RtsimSession.stopGlobal();
        Path aside;
        try {
            aside = ReinstallRules.moveAside(worldDir(server), Instant.now());
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
        RtsimSession.startGlobal(server);
        MicRtsim.LOGGER.info("[gametest] site ledger: a plain reset, the engine's pages moved to {}; a session runs: {}", aside,
                RtsimSession.current() != null);
        return aside;
    }

    /** An uninstall: the session stops and the world saves; no session runs until {@link #gapRestart}. */
    public static void stop(MinecraftServer server) {
        testsOnly();
        RtsimSession.stoppingGlobal();
        server.saveEverything(true, true, true);
        RtsimSession.stopGlobal();
    }

    /**
     * The reinstall after a gap of {@code gapTicks}: {@code level.dat} as a server without rtsim writes it is read through
     * NeoForge's own hook, the game time moves on by the gap, and a session starts (stopping the running one first, if any).
     * The guard resets the engine. Returns where the guard moved the pages, or null when it did not reset.
     */
    public static Path gapRestart(MinecraftServer server, long gapTicks) {
        testsOnly();
        CommonHooks.readAdditionalLevelSaveData(levelDatWithoutRtsim(server), new LevelStorageSource.LevelDirectory(worldDir(server)));
        long t = server.overworld().getGameTime();
        server.getWorldData().overworldData().setGameTime(t + gapTicks);
        RtsimSession.restartGlobalForTest(server);
        ReinstallGuard.Reset reset = ReinstallGuard.reset(server);
        MicRtsim.LOGGER.info("[gametest] site ledger: a reinstall after {} ticks: {}; a session runs: {}", gapTicks, reset,
                RtsimSession.current() != null);
        return reset == null ? null : reset.aside();
    }

    /** What a NeoForge server without rtsim writes into {@code level.dat}: the loaded mods but rtsim, and the game time. */
    private static CompoundTag levelDatWithoutRtsim(MinecraftServer server) {
        ListTag mods = new ListTag();
        ModList.get().getMods().forEach(m -> {
            if (m.getModId().equals(MicRtsim.MODID))
                return;
            CompoundTag t = new CompoundTag();
            t.putString("ModId", m.getModId());
            t.putString("ModVersion", m.getVersion().toString());
            mods.add(t);
        });
        CompoundTag fml = new CompoundTag();
        fml.put("LoadingModList", mods);
        CompoundTag data = new CompoundTag();
        data.putLong("Time", server.overworld().getGameTime());
        CompoundTag root = new CompoundTag();
        root.put("fml", fml);
        root.put("Data", data);
        return root;
    }

    /**
     * Leaves no marker, no capture and no moved pages behind, and a session running: for a test's end, passed or failed. A
     * session stopped by {@link #stop} is started again (a plain start, not a reset).
     */
    public static void cleanUp(MinecraftServer server) {
        try {
            ReinstallRules.deleteMarker(worldDir(server));
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
        ReinstallGuard.forgetCaptureForTest(server);
        List<Path> asides = new ArrayList<>();
        try (DirectoryStream<Path> s = Files.newDirectoryStream(worldDir(server), ReinstallRules.ASIDE_PREFIX + "*")) {
            s.forEach(asides::add);
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
        for (Path p : asides)
            deleteTree(p);
        if (RtsimSession.current() == null)
            RtsimSession.startGlobal(server);
    }

    private static Path worldDir(MinecraftServer server) {
        return server.getWorldPath(LevelResource.ROOT).toAbsolutePath().normalize();
    }

    private static void deleteTree(Path p) {
        try (Stream<Path> walk = Files.walk(p)) {
            for (Path q : walk.sorted(Comparator.reverseOrder()).toList())
                Files.deleteIfExists(q);
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }
}
