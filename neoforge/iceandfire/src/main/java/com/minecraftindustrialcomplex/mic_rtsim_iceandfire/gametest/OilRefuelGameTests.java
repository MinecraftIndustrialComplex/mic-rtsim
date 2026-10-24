package com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest;

import com.iafenvoy.iceandfire.entity.DragonBaseEntity;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ChunkWait;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonRecords;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.MicRtsimIceAndFire;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.about;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.bridge;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.check;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.engine;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.record;

/**
 * Refuelling (OILGATE.md §4.3, OG-5), native only: the stub decides nothing. The slice the Ice and Fire
 * server allows: it has no industry districts, so a raid on a district's tanks (the theft) is the
 * engine's and the protocol's tests; here a field flight's dragon spends oil on a player and drinks at its
 * field once out of view.
 */
@GameTestHolder(MicRtsimIceAndFire.MODID)
@PrefixGameTestTemplate(false)
public final class OilRefuelGameTests {
    private OilRefuelGameTests() {}

    /** Breaths the fight must cost before the player goes. */
    static final int SPEND = 10;
    /** Ticks of fighting with no breath before the test takes the dragon for one that cannot breathe. */
    static final int NO_BREATH = 1200;

    /**
     * A field flight's dragon breathes at a player and the engine's reserve falls by the breaths the
     * bridge sent (one point a breath, within the slow drain's point). The player goes, its presence goes
     * stale, its chunks are let go: the body is captured, and the dragon rests on its field and drinks; the
     * engine step in which it leaves the sim is the step that counts a refuel and a visitor gone, so the
     * refuel is its own. If the engine flew it off first (its visit ran out: DEMATERIALIZED reason 1), a
     * visitor leaving a player's view leaves without drinking, and the record says that path ran.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "oil_refuel")
    public static void aVisitorThatBreathedDrinksAtItsFieldOutOfView(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 9000);
        String test = "aVisitorThatBreathedDrinksAtItsFieldOutOfView";
        if (!DragonKit.nativeEngine()) {
            record(test, "skipped", "on the stub: refuelling is the engine's");
            helper.succeed();
            return;
        }
        ServerLevel level = helper.getLevel();
        BlockPos site = DragonKit.farSite();
        ChunkPos sc = new ChunkPos(site);
        int force = 11; // the engine's view radius (160) and a chunk
        Runnable unforce = DragonKit.forceChunks(level, sc, force);
        ServerPlayer[] p = {null};
        boolean[] fight = {false};
        int[] ticks = {0};
        long[] agent = {0};
        // Game times: 0 the player came, 1 the fight began, 2 the player went, 3 the chunks went,
        // 4 the body was captured, 5 the dragon left the sim.
        long[] t = new long[6];
        int[] at = new int[3]; // at the fight's start: the bridge's reserve, the engine's, breaths sent
        long[][] before = {null}; // the engine's counters in the last step the dragon was in the sim
        long[][] after = {null}; // and in the step it left
        Vec3 high = new Vec3(site.getX() + 0.5, 70, site.getZ() + 0.5);
        waits.eachTick(() -> {
            if (fight[0])
                OilGameTests.nudge(OilGameTests.bodyOrNull(agent[0]), p[0], OilGameTests.Fire.SCORCH);
            if (ticks[0]++ % 20 == 0 && p[0] != null)
                DragonKit.reportPlayer(p[0]);
            if (agent[0] != 0 && after[0] == null) {
                long[] o = DragonKit.nativeOil(agent[0]);
                if (o[NativeEngine.OIL_LEVEL] >= 0) {
                    before[0] = o;
                } else if (before[0] != null) {
                    after[0] = o;
                    t[5] = level.getGameTime();
                }
            }
        });
        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> check("the site's chunks loaded", DragonKit.ready(level, sc, force))))
                .thenExecute(() -> {
                    List<int[]> field = new ArrayList<>();
                    for (int dx = -1; dx <= 1; dx++)
                        for (int dz = -1; dz <= 1; dz++)
                            field.add(new int[] {sc.x + dx, sc.z + dz, 40_000});
                    DragonKit.oilChunks(level, field);
                    DragonKit.oilConfig(DragonRecords.OilConfig.FLAG_FIELD_FLIGHTS);
                    p[0] = OilGameTests.guardedPlayer(helper, high, "oil_refuel");
                    t[0] = level.getGameTime();
                })
                .thenWaitUntil(waits.logic(() -> {
                    int want = DragonRecords.Materialize.FLAG_HOSTILE | DragonRecords.Materialize.FLAG_OIL;
                    for (ScriptedEngine.Received r : engine().received(DragonRecords.DRAGON_MATERIALIZED)) {
                        long a = r.view().getLong(DragonRecords.Materialized.AGENT);
                        int flags = bridge().materializeFlagsOf(a);
                        if (r.view().getLong(Obs.GAME_TIME) >= t[0] && flags >= 0 && (flags & want) == want)
                            agent[0] = a;
                    }
                    check("a field flight's dragon materialized hostile with its oil", agent[0] != 0);
                    DragonKit.body(agent[0]);
                }))
                .thenExecute(() -> {
                    DragonKit.oilConfig(0); // this one visitor and no other
                    t[1] = level.getGameTime();
                    at[0] = bridge().oilOf(agent[0]);
                    at[1] = (int) DragonKit.nativeOil(agent[0])[NativeEngine.OIL_LEVEL];
                    at[2] = OilGameTests.breathsSent(agent[0]);
                    fight[0] = true;
                })
                .thenWaitUntil(waits.logic(() -> {
                    int sent = OilGameTests.breathsSent(agent[0]) - at[2];
                    check("it breathed " + SPEND + " times (" + sent + ")",
                            sent >= SPEND || (sent == 0 && level.getGameTime() - t[1] >= NO_BREATH));
                }))
                .thenExecute(() -> {
                    DragonBaseEntity d = OilGameTests.bodyOrNull(agent[0]);
                    int sent = OilGameTests.breathsSent(agent[0]) - at[2];
                    if (sent == 0) {
                        int stage = d == null ? 0 : d.getDragonStage();
                        record(test, "no breath", "no fire in " + NO_BREATH + " ticks: stage " + stage + ", "
                                + (d == null ? "?" : d.getAgeInDays()) + " days old");
                        check("a dragon past stage 1 breathes (stage " + stage + ")", stage <= 1);
                    }
                    fight[0] = false;
                    t[2] = level.getGameTime();
                    OilGameTests.release(p[0]);
                    p[0] = null;
                })
                // Every breath flushed (the bridge sends them every 20 ticks) and taken in an engine step.
                .thenWaitUntil(waits.logic(() -> check("the breaths flushed",
                        bridge().body(agent[0]) == null || bridge().pendingBreathsOf(agent[0]) == 0)))
                .thenIdle(40)
                .thenExecute(() -> {
                    int sent = OilGameTests.breathsSent(agent[0]) - at[2];
                    long engineNow = DragonKit.nativeOil(agent[0])[NativeEngine.OIL_LEVEL];
                    int bridgeNow = bridge().oilOf(agent[0]);
                    long drop = at[1] - engineNow;
                    check("the engine's reserve fell by the breaths sent (" + drop + " for " + sent + ")",
                            engineNow >= 0 && (drop == Math.min(sent, at[1]) || drop == Math.min(sent, at[1]) + 1));
                    record(test, "spent", sent + " breaths sent in " + (t[2] - t[1]) + " ticks of scorch fire; the engine's reserve "
                            + at[1] + " to " + engineNow + ", the body's " + at[0] + " to " + bridgeNow);
                })
                .thenIdle(260) // the player's presence goes stale: nothing sees the dragon fly again
                .thenExecute(() -> {
                    unforce.run();
                    t[3] = level.getGameTime();
                })
                .thenWaitUntil(waits.logic(() -> {
                    List<ScriptedEngine.Received> gone = about(DragonRecords.DRAGON_DEMATERIALIZED, agent[0]);
                    check("DRAGON_DEMATERIALIZED", !gone.isEmpty());
                    t[4] = gone.get(gone.size() - 1).view().getLong(Obs.GAME_TIME);
                }))
                .thenWaitUntil(waits.logic(() -> check("it left the sim", after[0] != null)))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> gone = about(DragonRecords.DRAGON_DEMATERIALIZED, agent[0]);
                    int reason = gone.get(gone.size() - 1).aux();
                    long refuels = after[0][NativeEngine.OIL_REFUELS] - before[0][NativeEngine.OIL_REFUELS];
                    long left = after[0][NativeEngine.OIL_VISITORS_LEFT] - before[0][NativeEngine.OIL_VISITORS_LEFT];
                    String timing = "chunks let go " + (t[3] - t[2]) + " ticks after the player went, captured "
                            + (t[4] - t[3]) + " ticks after, gone from the sim " + (t[5] - t[4]) + " ticks after the capture";
                    if (reason == DragonRecords.Dematerialized.COMMAND) {
                        check("flown off: a visitor gone (" + left + ") and no refuel (" + refuels + ")", left >= 1 && refuels == 0);
                        record(test, "refuel", "the engine flew it off first (reason 1): it left without drinking; " + timing);
                    } else {
                        check("captured out of view (reason " + reason + ")", reason == DragonRecords.Dematerialized.NOT_TICKING
                                || reason == DragonRecords.Dematerialized.UNLOADED);
                        check("a refuel (" + refuels + ") and a visitor gone (" + left + ") in the step it left",
                                refuels >= 1 && left >= 1);
                        check("after resting on its field (" + (t[5] - t[4]) + " ticks)", t[5] - t[4] >= 900);
                        record(test, "refuel", "captured (reason " + reason + "), rested on its field and drank: refuels +" + refuels
                                + " and visitors gone +" + left + " in the engine step it left the sim; " + timing);
                    }
                    DragonKit.restoreOilConfig();
                })
                .thenSucceed();
    }
}
