package com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import net.minecraft.gametest.framework.GameTestServer;
import net.minecraft.server.MinecraftServer;
import net.neoforged.neoforge.event.server.ServerStartedEvent;

/**
 * A GameTest server makes its level data afresh every run (game time 0) but keeps the world's files,
 * and with them the native engine's pages: the engine resumes at the tick its last session saved, and
 * its clock stands still until the game time passes it (a field flight booked at that clock never comes
 * due). On a GameTest server only, after the session started and before the first test, the game time
 * is moved up to the engine's clock, as a real server's level.dat would have it.
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
        long engine = n.dragonsOil(0)[NativeEngine.OIL_NOW];
        long game = server.overworld().getGameTime();
        if (engine <= game)
            return;
        server.getWorldData().overworldData().setGameTime(engine);
        MicRtsim.LOGGER.info("[gametest] clock: the engine resumed at tick {} from the world's pages, the game time was {}: moved up to {}",
                engine, game, server.overworld().getGameTime());
    }
}
