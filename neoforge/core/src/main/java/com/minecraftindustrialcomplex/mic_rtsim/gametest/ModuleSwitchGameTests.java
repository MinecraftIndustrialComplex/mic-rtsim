package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.BridgeEvents;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.TraderSpawning;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.mixin.ServerLevelAccessor;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.animal.horse.TraderLlama;
import net.minecraft.world.entity.npc.WanderingTrader;
import net.minecraft.world.entity.npc.WanderingTraderSpawner;
import net.minecraft.world.level.CustomSpawner;
import net.minecraft.world.level.GameRules;
import net.minecraft.world.level.storage.ServerLevelData;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.List;
import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * A switch per module (C25, PLAN.md §5.4) and the trader veto it drives (C8a): while trade and travel
 * are on, the sim owns wandering traders and vanilla's {@code WanderingTraderSpawner} is cancelled by a
 * mixin; with either off the spawner runs as vanilla, and the bridge never writes the
 * {@code doTraderSpawning} gamerule. The switches also reach the engine as a MODULES record. Its own
 * batch: the switches, the gamerules and the level's trader fields are server-wide.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class ModuleSwitchGameTests {
    private ModuleSwitchGameTests() {}

    /**
     * Spawner attempts per phase. One attempt spawns with a chance of 1 in 10 times the odds a spawn
     * position near the player is free (vanilla's {@code random.nextInt(10) != 0}), so 300 attempts miss
     * a spawn that is allowed with a chance of about 1e-13: a phase that must not spawn is not lucky,
     * and a phase that must spawn does.
     */
    private static final int ATTEMPTS = 300;
    /** A level-data delay no vanilla countdown writes: the spawner writes {@code spawnDelay - 1200}. */
    private static final int SENTINEL_DELAY = 31_337;
    private static final UUID NO_TRADER = new UUID(0, 0);

    private static boolean rule(MinecraftServer server) {
        return server.getGameRules().getBoolean(GameRules.RULE_DO_TRADER_SPAWNING);
    }

    private static void setRule(MinecraftServer server, GameRules.Key<GameRules.BooleanValue> key, boolean value) {
        server.getGameRules().getRule(key).set(value, server);
    }

    private static int lastModuleMask(ScriptedEngine engine) {
        List<ScriptedEngine.Received> recs = engine.received(Obs.MODULES);
        return recs.isEmpty() ? -1 : recs.get(recs.size() - 1).view().getInt(Obs.Modules.MASK);
    }

    private static int wanderingTraders(ServerLevel level) {
        int n = 0;
        for (Entity e : level.getAllEntities())
            if (e instanceof WanderingTrader && !e.isRemoved())
                n++;
        return n;
    }

    /**
     * The spawner the level really holds, ticked 1,200 times: true when its countdown ran, which it
     * announces by writing the level data's spawn delay. Vanilla writes it at least once in 1,200 ticks
     * and the GameTest world has {@code doMobSpawning} off (set here too), so the spawner never spawns.
     */
    private static boolean installedSpawnerCountsDown(ServerLevel level, ServerLevelData data) {
        List<CustomSpawner> all = ((ServerLevelAccessor) level).mic_rtsim$customSpawners();
        WanderingTraderSpawner spawner = null;
        for (CustomSpawner s : all) {
            if (s instanceof WanderingTraderSpawner w) {
                check("the level holds one wandering trader spawner", spawner == null);
                spawner = w;
            }
        }
        check("the level holds a wandering trader spawner", spawner != null);
        MinecraftServer server = level.getServer();
        boolean mobSpawning = server.getGameRules().getBoolean(GameRules.RULE_DOMOBSPAWNING);
        setRule(server, GameRules.RULE_DOMOBSPAWNING, false);
        try {
            data.setWanderingTraderSpawnDelay(SENTINEL_DELAY);
            for (int t = 0; t < 1200; t++)
                spawner.tick(level, true, true);
            return data.getWanderingTraderSpawnDelay() != SENTINEL_DELAY;
        } finally {
            setRule(server, GameRules.RULE_DOMOBSPAWNING, mobSpawning);
        }
    }

    /**
     * Runs vanilla's wandering trader spawner, class and all, against the real server: a new
     * {@code WanderingTraderSpawner} over the overworld's real level data, with the delay and chance
     * forced so the next due check spawns whenever vanilla's own 1 in 10 allows (chance 100: the
     * {@code nextInt(100) > chance} test never fails; delay 1,200: one 1,200-tick cycle is due), and
     * {@code doMobSpawning} on for the duration of this call only (the GameTest world turns it off, and
     * the spawner returns before it rolls when it is off). A server player stands in the level, as
     * {@code spawn} needs one. Nothing ticks inside the call, so nothing else spawns.
     *
     * @return the attempt (1 based) that spawned a trader, which is removed again with its llamas, or 0
     *         when {@code attempts} attempts spawned none
     */
    private static int attemptsUntilSpawn(ServerLevel level, ServerLevelData data, int attempts) {
        MinecraftServer server = level.getServer();
        boolean mobSpawning = server.getGameRules().getBoolean(GameRules.RULE_DOMOBSPAWNING);
        setRule(server, GameRules.RULE_DOMOBSPAWNING, true);
        try {
            for (int attempt = 1; attempt <= attempts; attempt++) {
                data.setWanderingTraderSpawnDelay(1200);
                data.setWanderingTraderSpawnChance(100);
                data.setWanderingTraderId(NO_TRADER);
                WanderingTraderSpawner spawner = new WanderingTraderSpawner(data);
                int returned = 0;
                for (int t = 0; t < 1200; t++)
                    returned += spawner.tick(level, true, true);
                UUID id = data.getWanderingTraderId();
                boolean spawned = !NO_TRADER.equals(id);
                check("the spawner reports a spawn exactly when it made a trader", (returned == 1) == spawned);
                if (spawned) {
                    Entity e = level.getEntity(id);
                    check("the trader it recorded is a wandering trader", e instanceof WanderingTrader);
                    removeWithLlamas(level, (WanderingTrader) e);
                    return attempt;
                }
            }
            return 0;
        } finally {
            setRule(server, GameRules.RULE_DOMOBSPAWNING, mobSpawning);
        }
    }

    private static void removeWithLlamas(ServerLevel level, WanderingTrader trader) {
        for (TraderLlama llama : level.getEntitiesOfClass(TraderLlama.class, trader.getBoundingBox().inflate(16)))
            if (llama.getLeashHolder() == trader) {
                llama.dropLeash(false, false);
                llama.discard();
            }
        trader.discard();
    }

    private static void switches(MinecraftServer server, Boolean trade, Boolean travel) {
        RtsimConfig.Test.setModule(Module.TRADE, trade);
        RtsimConfig.Test.setModule(Module.TRAVEL, travel);
        BridgeEvents.moduleSwitchesChanged(server);
    }

    /**
     * The veto, on the server's real spawner class and level data. Everything here is one server tick
     * (the spawner is driven by hand), and it puts back what it changes.
     */
    private static void traderVeto(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        MinecraftServer server = level.getServer();
        ServerLevelData data = server.getWorldData().overworldData();
        boolean ruleStart = rule(server);
        UUID idStart = data.getWanderingTraderId();
        int delayStart = data.getWanderingTraderSpawnDelay();
        int chanceStart = data.getWanderingTraderSpawnChance();
        ServerPlayer player = FakePlayers.add(level, helper.absoluteVec(new Vec3(5.5, 1.0, 5.5)), "trader_veto");
        try {
            setRule(server, GameRules.RULE_DO_TRADER_SPAWNING, true);
            check("trade and travel are on by default", RtsimConfig.on(Module.TRADE) && RtsimConfig.on(Module.TRAVEL));
            check("so the sim owns traders", TraderSpawning.simOwnsTraders());
            int tradersBefore = wanderingTraders(level);

            // Both on: vanilla's spawner stands still, on the real instance and on fresh ones.
            check("both on: the level's own spawner does not count down", !installedSpawnerCountsDown(level, data));
            int onAttempt = attemptsUntilSpawn(level, data, ATTEMPTS);
            record("modules", "both on: attempts that spawned a trader out of " + ATTEMPTS, onAttempt);
            check("both on: vanilla's spawner spawned no trader in " + ATTEMPTS + " attempts", onAttempt == 0);
            check("both on: no wandering trader appeared", wanderingTraders(level) == tradersBefore);
            check("both on: the gamerule is untouched", rule(server));

            // Trade off: the spawner is vanilla's again, and the gamerule is still not written.
            switches(server, false, null);
            check("trade off: the sim does not own traders", !TraderSpawning.simOwnsTraders());
            check("trade off: the gamerule is untouched", rule(server));
            check("trade off: the level's own spawner counts down", installedSpawnerCountsDown(level, data));
            int tradeOff = attemptsUntilSpawn(level, data, ATTEMPTS);
            record("modules", "trade off: attempt that spawned a trader", tradeOff);
            check("trade off: vanilla's spawner spawns a trader as vanilla does", tradeOff > 0);
            check("trade off: the gamerule is still untouched", rule(server));

            // Travel off, trade on again.
            switches(server, null, false);
            check("travel off: the sim does not own traders", !TraderSpawning.simOwnsTraders());
            check("travel off: the gamerule is untouched", rule(server));
            check("travel off: the level's own spawner counts down", installedSpawnerCountsDown(level, data));
            int travelOff = attemptsUntilSpawn(level, data, ATTEMPTS);
            record("modules", "travel off: attempt that spawned a trader", travelOff);
            check("travel off: vanilla's spawner spawns a trader as vanilla does", travelOff > 0);

            // Both on again: the veto is back, with nothing to restore.
            switches(server, null, null);
            check("both on again: the sim owns traders", TraderSpawning.simOwnsTraders());
            check("both on again: the gamerule is untouched", rule(server));
            check("both on again: the level's own spawner stands still", !installedSpawnerCountsDown(level, data));
            check("both on again: vanilla's spawner spawns no trader in " + ATTEMPTS + " attempts",
                    attemptsUntilSpawn(level, data, ATTEMPTS) == 0);

            // An admin's own false: it stays false whatever the switches do, and vanilla's spawner,
            // when the sim lets it run, still honours it.
            setRule(server, GameRules.RULE_DO_TRADER_SPAWNING, false);
            BridgeEvents.moduleSwitchesChanged(server);
            check("admin false, both on: still false", !rule(server));
            switches(server, false, null);
            check("admin false, trade off: still false", !rule(server));
            check("admin false, trade off: vanilla's spawner spawns no trader in " + ATTEMPTS + " attempts",
                    attemptsUntilSpawn(level, data, ATTEMPTS) == 0);
            check("admin false, trade off: the level's own spawner stands still (the gamerule)", !installedSpawnerCountsDown(level, data));
            switches(server, null, null);
            check("admin false, both on again: still false", !rule(server));
            check("no wandering trader is left behind", wanderingTraders(level) == tradersBefore);
        } finally {
            RtsimConfig.Test.setModule(Module.TRADE, null);
            RtsimConfig.Test.setModule(Module.TRAVEL, null);
            FakePlayers.remove(player);
            setRule(server, GameRules.RULE_DO_TRADER_SPAWNING, ruleStart);
            data.setWanderingTraderId(idStart);
            data.setWanderingTraderSpawnDelay(delayStart);
            data.setWanderingTraderSpawnChance(chanceStart);
        }
    }

    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 400, batch = "mic_rtsim_modules")
    public static void vanillaTraderSpawnerIsCancelledWhileTradeAndTravelAreOn(GameTestHelper helper) {
        MinecraftServer server = helper.getLevel().getServer();
        check("both caravan modules are on by default", RtsimConfig.on(Module.TRADE) && RtsimConfig.on(Module.TRAVEL));

        helper.startSequence()
                .thenExecute(() -> traderVeto(helper))
                // The engine hears the switches (MODULES), whatever the veto does.
                .thenExecute(() -> {
                    RtsimConfig.Test.setModule(Module.TRADE, false);
                    BridgeEvents.moduleSwitchesChanged(server);
                })
                .thenWaitUntil(() -> check("the engine was told trade is off",
                        lastModuleMask(RtsimTests.engine()) == RtsimConfig.effectiveMask()
                                && (lastModuleMask(RtsimTests.engine()) & Module.TRADE.bit()) == 0))
                .thenExecute(() -> {
                    RtsimConfig.Test.setModule(Module.TRADE, null);
                    BridgeEvents.moduleSwitchesChanged(server);
                })
                .thenWaitUntil(() -> check("the engine was told trade is back on",
                        (lastModuleMask(RtsimTests.engine()) & Module.TRADE.bit()) != 0))
                .thenSucceed();
    }
}
