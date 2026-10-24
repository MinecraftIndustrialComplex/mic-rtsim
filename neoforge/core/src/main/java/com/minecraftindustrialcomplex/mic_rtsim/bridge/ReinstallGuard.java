package com.minecraftindustrialcomplex.mic_rtsim.bridge;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.Tag;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.level.storage.LevelResource;

import com.minecraftindustrialcomplex.mic_rtsim.bridge.ReinstallRules.ModList;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.ReinstallRules.Verdict;

import java.io.IOException;
import java.nio.file.Path;
import java.time.Instant;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/**
 * The reinstall guard (UR-1, docs/choices-reinstall-guard.md): rtsim notices at start that the world ran without it,
 * and then resets the whole sim, loudly, before the engine opens.
 *
 * <p><b>Detection.</b> NeoForge writes the list of loaded mods into {@code level.dat} (root {@code fml.LoadingModList})
 * on every write of it, and reads it back at the start through {@code CommonHooks.readAdditionalLevelSaveData},
 * before the start rewrites it. A mixin there ({@code CommonHooksLevelDatMixin}) hands the root tag to
 * {@link #levelDatRead}. A world that last saved with rtsim lists {@code mic_rtsim}, whether it stopped or crashed
 * (a crash leaves the last save's {@code level.dat}); a server that ran the world without rtsim rewrote
 * {@code level.dat} at its own start, without it. So a list without {@code mic_rtsim}, in a world that holds rtsim's
 * state ({@code world/rtsim/} or a {@code data/mic_rtsim*.dat}), is a gap; without the state it is a first install.
 * A {@code level.dat} with no {@code fml} tag was last written by something that is not NeoForge (a vanilla server),
 * which is a gap too; a malformed list decides nothing.
 *
 * <p><b>The marker.</b> A gap is written to {@code world/mic_rtsim-reinstall.txt} at once, synced, before the start
 * rewrites {@code level.dat} with rtsim in its list: a crash anywhere in the start after that still owes the reset
 * to the next start. The marker goes only after the reset's first world save has written the industry data that
 * remembers what to re-adopt ({@link #onOverworldSave}), queued behind that write on NeoForge's save-data chain.
 *
 * <p><b>The reset</b> ({@link #beforeEngineOpens}): {@code world/rtsim/} is moved to
 * {@code world/rtsim-reinstall-<time>/} (kept for the admin, never read again) and the engine opens empty, as after
 * runbook 8.6's sim reset. What the engine owed (a reconcile it would send again) is gone with its pages, so nothing
 * replays; the journal's observations go to the new engine as after any reset, and every module announces what it
 * knows again ({@code observationsLost}). The industry module re-adopts the districts whose seeds the gap stripped
 * ({@code Districts.planReadopt}). If the move fails there is no session, and {@code rtsim perf} says why: never a
 * partial reset.
 */
public final class ReinstallGuard {
    private ReinstallGuard() {}

    public static final String MARKER = ReinstallRules.MARKER;

    /** One {@code level.dat} read: what it said, and the game time it carried. */
    public record Capture(ModList modList, Verdict verdict, long gameTime, Instant at) {}

    /** A reset this start performs: why, and where the old pages went (null: there were none). */
    public record Reset(String reason, Path aside, Instant at) {}

    /** The last read per world directory (absolute, normalized): the mixin runs before any server exists. */
    private static final Map<Path, Capture> CAPTURES = new ConcurrentHashMap<>();
    /** This start's outcome, per world directory: what {@code rtsim perf} reports. */
    private static final Map<Path, Status> STATUS = new ConcurrentHashMap<>();

    /** This start's outcome for one world. */
    static final class Status {
        final Capture capture;
        final Reset reset;
        /** The marker was deleted (after the reset's first save), or was never needed. */
        volatile boolean markerCleared;
        /** The deletion is queued behind a save. */
        volatile boolean deletionQueued;

        Status(Capture capture, Reset reset) {
            this.capture = capture;
            this.reset = reset;
            this.markerCleared = reset == null;
        }
    }

    // ---- the read (the mixin) -------------------------------------------------------------------

    /**
     * {@code level.dat} was read at a start (dedicated: before the start rewrites it; integrated: as the world
     * opens). Never throws: the world load must not fail over the guard.
     */
    public static void levelDatRead(CompoundTag root, Path worldDir) {
        try {
            Path dir = worldDir.toAbsolutePath().normalize();
            boolean hasFml = root.contains("fml", Tag.TAG_COMPOUND);
            List<String> ids = null;
            if (hasFml) {
                CompoundTag fml = root.getCompound("fml");
                if (fml.contains("LoadingModList", Tag.TAG_LIST)) {
                    ListTag list = fml.getList("LoadingModList", Tag.TAG_COMPOUND);
                    ids = new ArrayList<>(list.size());
                    for (int i = 0; i < list.size(); i++)
                        ids.add(list.getCompound(i).getString("ModId"));
                }
            }
            ModList modList = ReinstallRules.classify(hasFml, ids);
            boolean state = ReinstallRules.stateExists(dir);
            Verdict verdict = ReinstallRules.decide(modList, state);
            long gameTime = root.getCompound("Data").getLong("Time");
            Capture c = new Capture(modList, verdict, gameTime, Instant.now());
            CAPTURES.put(dir, c);
            switch (verdict) {
                case GAP -> {
                    String why = modList == ModList.NO_FML
                            ? "level.dat has no NeoForge mod list (a server without NeoForge wrote it last)"
                            : "level.dat's mod list lacks " + MicRtsim.MODID + " (the world ran without rtsim)";
                    ReinstallRules.writeMarker(dir, why + "; read " + c.at() + " at game time " + gameTime + "\n"
                            + "mic_rtsim resets its sim at this start (UR-1). Deleted by rtsim after the reset's first world save.\n");
                    MicRtsim.LOGGER.error("[mic_rtsim] REINSTALL: {} since rtsim's last save, and the world holds rtsim's state:"
                            + " the sim is reset at this start (UR-1); marker {}", why, dir.resolve(MARKER));
                }
                case UNKNOWN -> MicRtsim.LOGGER.warn("[mic_rtsim] reinstall guard: level.dat's mod list is unreadable; a gap without rtsim cannot be seen at this start");
                case FIRST_INSTALL -> MicRtsim.LOGGER.info("[mic_rtsim] reinstall guard: level.dat lacks {} and the world holds no rtsim state: a first install", MicRtsim.MODID);
                case NO_GAP -> MicRtsim.LOGGER.debug("[mic_rtsim] reinstall guard: level.dat lists {}: no gap", MicRtsim.MODID);
            }
        } catch (IOException | RuntimeException e) {
            MicRtsim.LOGGER.error("[mic_rtsim] reinstall guard: checking level.dat failed; a gap without rtsim cannot be seen at this start", e);
        }
    }

    // ---- the start ------------------------------------------------------------------------------

    private static Path worldDir(MinecraftServer server) {
        return server.getWorldPath(LevelResource.ROOT).toAbsolutePath().normalize();
    }

    /**
     * At the session's start, before the engine opens: performs the reset this start owes, if any. Returns it,
     * or null. Throws when the reset cannot be done; the caller then opens no session.
     */
    static Reset beforeEngineOpens(MinecraftServer server) throws IOException {
        Path dir = worldDir(server);
        Capture capture = CAPTURES.remove(dir);
        String marked = ReinstallRules.readMarker(dir);
        if (!ReinstallRules.resets(capture == null ? null : capture.verdict(), marked != null)) {
            STATUS.put(dir, new Status(capture, null));
            return null;
        }
        String reason = marked != null ? marked : "level.dat's mod list lacked " + MicRtsim.MODID + " (the marker could not be written)";
        if (marked == null)
            ReinstallRules.writeMarker(dir, reason + "\n");
        Instant at = Instant.now();
        Path aside = ReinstallRules.moveAside(dir, at);
        Reset reset = new Reset(reason, aside, at);
        STATUS.put(dir, new Status(capture, reset));
        MicRtsim.LOGGER.error("[mic_rtsim] REINSTALL RESET (UR-1): {}. {}; the engine starts empty and every module announces"
                        + " what it knows again; districts whose seeds the gap stripped are adopted again as their chunks load."
                        + " Lost: the sim's economy and memory (SRV2), players' block marks in chunks that loaded during the gap."
                        + " `rtsim perf` shows the progress.",
                reason, aside == null ? "There were no engine pages to move" : "The engine's pages are moved to " + dir.relativize(aside));
        return reset;
    }

    /** This start's reset for the server's world, or null. */
    public static Reset reset(MinecraftServer server) {
        Status s = STATUS.get(worldDir(server));
        return s == null ? null : s.reset;
    }

    /**
     * The overworld saved (LevelEvent.Save, after its saved data queued their writes): the first save after a
     * reset deletes the marker behind them on the same chain, so after the industry data that remembers what to
     * re-adopt. A crash before then resets again at the next start, which is harmless (the engine is still new).
     */
    static void onOverworldSave(MinecraftServer server) {
        Path dir = worldDir(server);
        Status s = STATUS.get(dir);
        if (s == null || s.markerCleared || s.deletionQueued)
            return;
        s.deletionQueued = true;
        net.neoforged.neoforge.common.IOUtilities.withIOWorker(() -> {
            try {
                ReinstallRules.deleteMarker(dir);
                s.markerCleared = true;
                MicRtsim.LOGGER.info("[mic_rtsim] reinstall reset: saved; marker {} deleted", MARKER);
            } catch (IOException e) {
                s.deletionQueued = false;
                MicRtsim.LOGGER.error("[mic_rtsim] reinstall reset: deleting the marker failed; the next start resets again", e);
            }
        });
    }

    /** The guard's line for {@code rtsim perf}. */
    public static String describe(MinecraftServer server) {
        Status s = STATUS.get(worldDir(server));
        if (s == null)
            return "reinstall guard: not run at this start";
        if (s.reset != null) {
            Reset r = s.reset;
            return "reinstall guard: SIM RESET at this start (UR-1): " + r.reason() + "; "
                    + (r.aside() == null ? "no engine pages to move" : "pages moved to world/" + r.aside().getFileName())
                    + "; marker " + (s.markerCleared ? "deleted after the first save" : "kept until the first save");
        }
        if (s.capture == null)
            return "reinstall guard: level.dat not read at this start (a GameTest server or a new world): no gap seen";
        return switch (s.capture.verdict()) {
            case NO_GAP -> "reinstall guard: level.dat listed " + MicRtsim.MODID + ": no gap";
            case FIRST_INSTALL -> "reinstall guard: first install (level.dat lacked " + MicRtsim.MODID + ", no rtsim state)";
            case UNKNOWN -> "reinstall guard: level.dat's mod list was unreadable: a gap could not be seen";
            case GAP -> "reinstall guard: gap seen but no reset (a bug: report it)";
        };
    }

    /** GameTests: what the last {@code level.dat} read captured for the server's world and this start has not taken yet, or null. */
    public static Capture pendingCapture(MinecraftServer server) {
        return CAPTURES.get(worldDir(server));
    }

    /** GameTests: forgets a capture this start will not take (a test's read that did not restart). */
    public static void forgetCaptureForTest(MinecraftServer server) {
        CAPTURES.remove(worldDir(server));
    }

    /** GameTests: whether this start's reset has deleted its marker. */
    public static boolean markerCleared(MinecraftServer server) {
        Status s = STATUS.get(worldDir(server));
        return s != null && s.markerCleared;
    }
}
