package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.BridgeEvents;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.BridgeStateData;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.level.GameRules;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * A switch per module (C25, PLAN.md §5.4): while trade and travel are on, the sim owns wandering
 * traders and {@code doTraderSpawning} is false (C8); switching either off gives the gamerule back the
 * value it had. The switches also reach the engine as a MODULES record. Its own batch: the gamerule
 * and the switches are server-wide.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class ModuleSwitchGameTests {
    private ModuleSwitchGameTests() {}

    private static boolean rule(MinecraftServer server) {
        return server.getGameRules().getBoolean(GameRules.RULE_DO_TRADER_SPAWNING);
    }

    private static int lastModuleMask(StubEngine stub) {
        List<StubEngine.Received> recs = stub.received(Obs.MODULES);
        return recs.isEmpty() ? -1 : recs.get(recs.size() - 1).view().getInt(Obs.Modules.MASK);
    }

    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 200, batch = "mic_rtsim_modules")
    public static void doTraderSpawningFollowsTheTradeAndTravelSwitches(GameTestHelper helper) {
        MinecraftServer server = helper.getLevel().getServer();
        BridgeStateData state = BridgeStateData.get(server);
        check("both caravan modules are on by default", RtsimConfig.on(Module.TRADE) && RtsimConfig.on(Module.TRAVEL));
        check("so the sim owns traders and took the gamerule at start", state.traderRuleOwned());
        check("doTraderSpawning is false while the sim owns traders", !rule(server));
        boolean prior = state.traderRulePrior();
        record("modules", "doTraderSpawning before the sim took it", prior);

        helper.startSequence()
                .thenExecute(() -> {
                    RtsimConfig.Test.setModule(Module.TRADE, false);
                    BridgeEvents.moduleSwitchesChanged(server);
                    check("trade off: the gamerule is back to " + prior, rule(server) == prior);
                    check("and the sim no longer owns it", !state.traderRuleOwned());
                })
                .thenWaitUntil(() -> check("the engine was told trade is off",
                        lastModuleMask(RtsimTests.stub()) == RtsimConfig.effectiveMask()
                                && (lastModuleMask(RtsimTests.stub()) & Module.TRADE.bit()) == 0))
                .thenExecute(() -> {
                    RtsimConfig.Test.setModule(Module.TRADE, null);
                    BridgeEvents.moduleSwitchesChanged(server);
                    check("trade on again: doTraderSpawning false", !rule(server) && state.traderRuleOwned());
                    RtsimConfig.Test.setModule(Module.TRAVEL, false);
                    BridgeEvents.moduleSwitchesChanged(server);
                    check("travel off: the gamerule is back to " + prior, rule(server) == prior && !state.traderRuleOwned());
                    RtsimConfig.Test.setModule(Module.TRAVEL, null);
                    BridgeEvents.moduleSwitchesChanged(server);
                    check("travel on again: doTraderSpawning false", !rule(server) && state.traderRuleOwned());
                    // Someone turns it on by hand while the sim owns traders: the next sync takes it back.
                    server.getGameRules().getRule(GameRules.RULE_DO_TRADER_SPAWNING).set(true, server);
                    BridgeEvents.moduleSwitchesChanged(server);
                    check("a hand-set true is taken back while the sim owns traders", !rule(server));
                })
                .thenWaitUntil(() -> check("the engine was told trade is back on",
                        (lastModuleMask(RtsimTests.stub()) & Module.TRADE.bit()) != 0))
                .thenSucceed();
    }
}
