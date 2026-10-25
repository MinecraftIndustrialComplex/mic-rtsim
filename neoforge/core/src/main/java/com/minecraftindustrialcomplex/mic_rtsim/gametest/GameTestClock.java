package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.server.MinecraftServer;
import net.neoforged.neoforge.event.server.ServerStartedEvent;

import java.util.Arrays;

/**
 * A GameTest server makes its level data afresh every run (game time near 0) but keeps the world's files,
 * and with them the native engine's pages: every module resumes at the clock its last session saved, and
 * stands still until the game time passes it. A herd restored that way books its next look at the
 * players at the old clock, so a herd test on a reused world waited for a materialization that never came
 * in time; the dragons' field flights did the same. On a GameTest server only, after the session started
 * and before the first test, the game time is moved up to the latest module's clock, as a real server's
 * level.dat would have it.
 */
public final class GameTestClock {
    private GameTestClock() {}

    public static void onServerStarted(ServerStartedEvent event) {
        MinecraftServer server = event.getServer();
        if (!(server instanceof GameTestServer))
            return;
        RtsimSession s = RtsimSession.current();
        if (s == null || s.failed() || !(s.engine() instanceof NativeEngine n))
            return;
        long[] clocks = n.clocks();
        long engine = n.gameTimeOfClocks();
        long game = server.overworld().getGameTime();
        MicRtsim.LOGGER.info("[gametest] clock: the engine's clocks (game time, offset, phase 1, dragons, ships, herds, conflict, industry) = {}",
                Arrays.toString(clocks));
        if (engine <= game)
            return;
        server.getWorldData().overworldData().setGameTime(engine);
        MicRtsim.LOGGER.info("[gametest] clock: the engine resumed at game time {} from the world's pages, the game time was {}: moved up to {}",
                engine, game, server.overworld().getGameTime());
    }
}
