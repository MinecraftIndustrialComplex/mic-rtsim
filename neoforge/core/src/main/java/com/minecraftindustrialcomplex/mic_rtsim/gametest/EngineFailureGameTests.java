package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.journal.JournalSavedData;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * An engine that fails (a panic in a native step, a protocol error) hands back empty command buffers
 * flagged {@code CMD_FLAG_FAILED}, and the bridge stops for the session (B30) instead of waiting on
 * it forever. Runs on its own session and engine, so the server's session is untouched.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class EngineFailureGameTests {
    private EngineFailureGameTests() {}

    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 200)
    public static void aFailedEngineStopsTheBridgeForTheSession(GameTestHelper helper) {
        ScriptedEngine engine = RtsimTests.freshEngine();
        RtsimSession session = new RtsimSession(helper.getLevel().getServer(), engine, JournalSavedData.fresh());
        int[] tick = {0};
        boolean[] done = {false};
        helper.onEachTick(() -> {
            if (done[0])
                return;
            session.endOfTick();
            tick[0]++;
            if (tick[0] == 5) {
                check("the session runs before the failure", !session.failed());
                engine.failForTest();
            }
            if (session.failed()) {
                record("engine-failure", "bridge stopped after", (tick[0] - 5) + " ticks; engine says: " + engine.protocolError());
                check("the engine reports why", engine.protocolError() != null);
                engine.close();
                done[0] = true;
            } else if (tick[0] > 150) {
                engine.close();
                throw new GameTestAssertException("the bridge never noticed the failed engine");
            }
        });
        helper.succeedWhen(() -> check("finished", done[0]));
    }
}
