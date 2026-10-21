package com.minecraftindustrialcomplex.mic_rtsim.bridge;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;

/**
 * The sim owns every wandering trader (C8). It keeps vanilla's random traders away by cancelling the
 * spawner, not by the gamerule (C8a): while trade and travel are both on, {@code WanderingTraderSpawnerMixin}
 * returns at the head of {@code WanderingTraderSpawner.tick}. The bridge never writes
 * {@code doTraderSpawning}, so an admin's own value stands, and removing the mod, or switching either
 * module off (C25, PLAN.md §5.4), gives traders back to vanilla with nothing to undo.
 *
 * <p>The veto reads {@link RtsimConfig} at every spawner tick, so a config reload takes effect at once;
 * nothing is saved. Whether a session is running does not matter: with the engine down there are no
 * caravans and no vanilla traders either (A8: no fallback).
 */
public final class TraderSpawning {
    private TraderSpawning() {}

    private static Boolean announced;

    /** The sim owns traders, and vanilla's spawner is cancelled, only while both caravan modules run. */
    public static boolean simOwnsTraders() {
        return RtsimConfig.on(Module.TRADE) && RtsimConfig.on(Module.TRAVEL);
    }

    /**
     * Logs who owns the traders: at server start (a fresh log for each server) and whenever the switches
     * change it.
     */
    public static void announce(boolean serverStart) {
        boolean owns = simOwnsTraders();
        if (!serverStart && announced != null && announced == owns)
            return;
        announced = owns;
        if (owns)
            MicRtsim.LOGGER.info("[mic_rtsim] the sim owns wandering traders now: vanilla's trader spawner is cancelled while trade and travel are on (doTraderSpawning is not touched)");
        else
            MicRtsim.LOGGER.info("[mic_rtsim] trade or travel is off: vanilla's wandering trader spawner runs as usual (doTraderSpawning is not touched)");
    }
}
