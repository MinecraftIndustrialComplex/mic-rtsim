package com.minecraftindustrialcomplex.mic_rtsim.bridge;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.level.GameRules;

/**
 * The sim owns every wandering trader (C8): while it does, vanilla's {@code WanderingTraderSpawner}
 * is off through the gamerule {@code doTraderSpawning}. Switching off the trade or the travel module
 * gives traders back to vanilla (C25, PLAN.md §5.4): the gamerule gets back the value it had when the
 * sim took it.
 *
 * <p>Synced at server start and whenever the server config reloads. While the sim owns the rule, a
 * sync also sets it false again if someone turned it on by hand.
 */
public final class TraderSpawning {
    private TraderSpawning() {}

    /** The sim owns traders only while both caravan modules run. */
    public static boolean simOwnsTraders() {
        return RtsimConfig.on(Module.TRADE) && RtsimConfig.on(Module.TRAVEL);
    }

    public static void sync(MinecraftServer server) {
        BridgeStateData state = BridgeStateData.get(server);
        GameRules.BooleanValue rule = server.getGameRules().getRule(GameRules.RULE_DO_TRADER_SPAWNING);
        if (simOwnsTraders()) {
            if (!state.traderRuleOwned) {
                state.traderRulePrior = rule.get();
                state.traderRuleOwned = true;
                state.setDirty();
                MicRtsim.LOGGER.info("[mic_rtsim] the sim owns wandering traders now: doTraderSpawning false (was {})", state.traderRulePrior);
            }
            if (rule.get())
                rule.set(false, server);
        } else if (state.traderRuleOwned) {
            rule.set(state.traderRulePrior, server);
            state.traderRuleOwned = false;
            state.setDirty();
            MicRtsim.LOGGER.info("[mic_rtsim] trade or travel is off: doTraderSpawning back to {}", state.traderRulePrior);
        }
    }
}
