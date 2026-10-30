package com.minecraftindustrialcomplex.mic_rtsim_ribbits.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.village.Village;
import com.minecraftindustrialcomplex.mic_rtsim.village.VillageRegistry;
import com.minecraftindustrialcomplex.mic_rtsim_ribbits.MicRtsimRibbits;
import com.minecraftindustrialcomplex.mic_rtsim_ribbits.Ribbits;
import com.yungnickyoung.minecraft.ribbits.entity.RibbitEntity;
import com.yungnickyoung.minecraft.ribbits.module.RibbitProfessionModule;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * Ribbit villages renew (EQUILIBRIUM.md §2.4.4, slice S6, docs/overnight-choices-equilibrium.md EQC165 to EQC167, EQC180): a
 * Ribbit lost from a village comes back as a resident at its meeting point, not as a new house; the refill keeps the village's
 * merchant (Ben's answer to FB-4: a merchant when the village has none, else a profession picked by the order's hash); and
 * settlers from the edge walk in the way a villager's do. The orders are the engine's own SPAWN_RESIDENT, scripted here so the
 * test decides when they come (a scripted village: the engine decides nothing about it).
 *
 * <p>UNVERIFIED on a running server until the Ribbits round has run them: the first build of these tests.
 */
@GameTestHolder(MicRtsimRibbits.MODID)
@PrefixGameTestTemplate(false)
public final class RibbitRenewalGameTests {
    private RibbitRenewalGameTests() {}

    private static final int AT_VILLAGE_ARRIVAL = Cmd.SpawnResident.FLAG_AT_VILLAGE | Cmd.SpawnResident.FLAG_ARRIVAL;
    private static final int WALK_IN = AT_VILLAGE_ARRIVAL | Cmd.SpawnResident.FLAG_WALK_IN;
    private static final String RIBBIT = "ribbits:ribbit";

    private static ScriptedEngine.Received result(ScriptedEngine stub, long op) {
        List<ScriptedEngine.Received> rs = RtsimTests.recordsWith(stub, Obs.COMMAND_RESULT, Obs.CommandResult.OP_ID, op);
        return rs.isEmpty() ? null : rs.get(rs.size() - 1);
    }

    private static long engineOp() {
        return java.util.concurrent.ThreadLocalRandom.current().nextLong() & Long.MAX_VALUE;
    }

    /** An op whose hash does not pick a merchant (so the profession it picks says it was the hash that picked it). */
    private static long opNotMerchant() {
        for (;;) {
            long op = engineOp();
            if (!"ribbits:merchant".equals(MicRtsimRibbits.professionByOp(op)))
                return op;
        }
    }

    private static void arrive(ScriptedEngine stub, Village v, GameTestHelper helper, long op, int flags, double x, double z) {
        stub.spawnResident(op, v.id, RtsimTests.dimension(helper), RIBBIT, null, flags, x, 0, z, 0);
    }

    private static List<RibbitEntity> ribbitsIn(GameTestHelper helper, Village v) {
        return helper.getLevel().getEntitiesOfClass(RibbitEntity.class, AABB.of(v.box).inflate(8), RibbitEntity::isAlive);
    }

    /**
     * A Ribbit killed out of a village comes back as a resident at the meeting point, ordered as an arrival: no house is
     * built for it (EQC165). With no merchant living in the village, the arrival is a merchant (EQC180).
     */
    @GameTest(template = "arena", timeoutTicks = 600, batch = "mic_rtsim_ribbits_refill")
    public static void aKilledRibbitIsRefilledIntoTheVillageNotANewHouse(GameTestHelper helper) {
        Village v = RtsimTests.adoptArena(helper, MicRtsimRibbits.CULTURE);
        ScriptedEngine stub = RtsimTests.engine();
        long op = engineOp();
        arrive(stub, v, helper, op, AT_VILLAGE_ARRIVAL, 0, 0);
        helper.startSequence()
                .thenWaitUntil(() -> check("the order was answered", result(stub, op) != null))
                .thenExecute(() -> {
                    check("placed: status " + result(stub, op).aux(), result(stub, op).aux() == Obs.CommandResult.APPLIED);
                    List<RibbitEntity> found = ribbitsIn(helper, v);
                    check("one ribbit stands in the village: " + found.size(), found.size() == 1);
                    RibbitEntity r = found.get(0);
                    double away = Math.hypot(r.getX() - (v.meeting.getX() + 0.5), r.getZ() - (v.meeting.getZ() + 0.5));
                    check("by the meeting point: " + away, away <= 8);
                    check("no house was built for it: " + v.houses.size(), v.houses.isEmpty());
                    check("a merchant, the village having none: " + Ribbits.professionName(r),
                            "ribbits:merchant".equals(Ribbits.professionName(r)));
                    check("an ordinary entity, not a puppet", !RtsimApi.isPuppet(r) && !RtsimAttachments.isTagged(r));
                    record("ribbitRefill", "resident", Ribbits.professionName(r) + " at " + r.blockPosition());
                    r.discard();
                    RtsimTests.forget(v);
                })
                .thenSucceed();
    }

    /**
     * A refilled village keeps a merchant (EQC180): with its merchant alive the arrival's profession is the order's hash's pick;
     * the merchant dies, and the next arrival is a merchant again.
     */
    @GameTest(template = "arena", timeoutTicks = 800, batch = "mic_rtsim_ribbits_merchant")
    public static void aRefilledVillageKeepsAMerchant(GameTestHelper helper) {
        Village v = RtsimTests.adoptArena(helper, MicRtsimRibbits.CULTURE);
        ScriptedEngine stub = RtsimTests.engine();
        Vec3 c = RtsimTests.centre(helper);
        RibbitEntity merchant = RibbitGameTests.spawn(helper, c.add(2, 0, 2), RibbitProfessionModule.MERCHANT);
        merchant.setNoAi(true);
        long first = opNotMerchant(), second = engineOp();
        arrive(stub, v, helper, first, AT_VILLAGE_ARRIVAL, 0, 0);
        helper.startSequence()
                .thenWaitUntil(() -> check("the first order was answered", result(stub, first) != null))
                .thenExecute(() -> {
                    check("placed", result(stub, first).aux() == Obs.CommandResult.APPLIED);
                    List<RibbitEntity> arrivals = new ArrayList<>(ribbitsIn(helper, v));
                    arrivals.remove(merchant);
                    check("one arrival beside the merchant: " + arrivals.size(), arrivals.size() == 1);
                    String got = Ribbits.professionName(arrivals.get(0));
                    check("the village had its merchant, so the hash picked: " + got + " for " + MicRtsimRibbits.professionByOp(first),
                            MicRtsimRibbits.professionByOp(first).equals(got));
                    merchant.discard();
                    arrive(stub, v, helper, second, AT_VILLAGE_ARRIVAL, 0, 0);
                })
                .thenWaitUntil(() -> check("the second order was answered", result(stub, second) != null))
                .thenExecute(() -> {
                    check("placed", result(stub, second).aux() == Obs.CommandResult.APPLIED);
                    int merchants = 0;
                    for (RibbitEntity r : ribbitsIn(helper, v)) {
                        if ("ribbits:merchant".equals(Ribbits.professionName(r)))
                            merchants++;
                        r.discard();
                    }
                    check("the merchant was replaced by a merchant: " + merchants, merchants == 1);
                    RtsimTests.forget(v);
                })
                .thenSucceed();
    }

    /**
     * Settlers from the edge walk in the way a villager's do (EQC177, EQC178): a player watching the village, the Ribbit spawns
     * 96 to 176 blocks out on the ray toward the source, at least 128 blocks from him, and walks to within 12 blocks of the
     * meeting point inside the timeout.
     */
    @GameTest(template = "arena", timeoutTicks = 12000, batch = "mic_rtsim_ribbits_walk")
    public static void ribbitSettlersWalkIn(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        Village v = RtsimTests.adoptArena(helper, MicRtsimRibbits.CULTURE);
        ScriptedEngine stub = RtsimTests.engine();
        VillageRegistry villages = RtsimTests.session().phase1.villages;
        Vec3 c = RtsimTests.centre(helper);
        ServerPlayer player = FakePlayers.add(level, c.add(0, 0, 4), "ribbit_watcher");
        List<ChunkPos> forced = new ArrayList<>();
        double sx = v.meeting.getX() + 192, sz = v.meeting.getZ();
        // The whole corridor from the edge to the meeting point, not only the spawn points: a walker stops in a chunk that
        // does not tick, and this test is about the walk, not about which chunks a player's view keeps loaded.
        for (int d = 0; d <= 192; d += 16) {
            int px = (int) Math.floor(v.meeting.getX() + d), pz = v.meeting.getZ();
            for (int cx = (px >> 4) - 1; cx <= (px >> 4) + 1; cx++)
                for (int cz = (pz >> 4) - 1; cz <= (pz >> 4) + 1; cz++) {
                    ChunkPos p = new ChunkPos(cx, cz);
                    if (!forced.contains(p)) {
                        level.setChunkForced(cx, cz, true);
                        forced.add(p);
                    }
                }
        }
        long op = engineOp();
        long[] arrived0 = {RtsimTests.session().phase1.walkers.arrived()};
        long[] placed0 = {RtsimTests.session().phase1.walkers.placed()};
        double[] out = {0};
        java.util.UUID[] settlerId = {null};
        long[] lastSample = {0};
        StringBuilder trace = new StringBuilder();
        arrive(stub, v, helper, op, WALK_IN, sx, sz);
        helper.startSequence()
                .thenWaitUntil(() -> {
                    check("the order was answered", result(stub, op) != null);
                    check("a walker is registered: " + villages.walkers.size(), villages.walkers.size() >= 1);
                })
                .thenExecute(() -> {
                    check("placed at the edge, not in the village", ribbitsIn(helper, v).isEmpty());
                    for (VillageRegistry.Walker w : villages.walkers.values()) {
                        if (w.village != v.id)
                            continue;
                        var settler = level.getEntity(w.id);
                        check("the settler is in the level", settler != null);
                        settlerId[0] = w.id;
                        out[0] = Math.hypot(settler.getX() - (v.meeting.getX() + 0.5), settler.getZ() - (v.meeting.getZ() + 0.5));
                        check("96 to 176 blocks out: " + out[0], out[0] >= 94 && out[0] <= 178);
                        check("at least 128 blocks from the player: " + settler.distanceTo(player), settler.distanceTo(player) >= 127);
                        check("an ordinary ribbit", settler instanceof RibbitEntity && !RtsimAttachments.isTagged(settler));
                    }
                    record("ribbitWalk", "spawned this far out", out[0]);
                })
                .thenWaitUntil(() -> {
                    // A trace of the walk for the failure message: where the settler is every 500 ticks and what holds it.
                    var settler = settlerId[0] == null ? null : level.getEntity(settlerId[0]);
                    long tick = helper.getTick();
                    if (tick - lastSample[0] >= 500) {
                        lastSample[0] = tick;
                        trace.append(" t").append(tick).append(' ');
                        if (settler == null) {
                            trace.append("gone");
                        } else {
                            trace.append((int) Math.hypot(settler.getX() - (v.meeting.getX() + 0.5), settler.getZ() - (v.meeting.getZ() + 0.5)))
                                    .append(settler.isAlive() ? "" : " dead")
                                    .append(level.isPositionEntityTicking(settler.blockPosition()) ? " ticking" : " NOT ticking");
                            if (settler instanceof net.minecraft.world.entity.Mob m)
                                trace.append(m.getNavigation().isDone() ? " idle" : " pathing").append(m.isNoAi() ? " noai" : "");
                        }
                        trace.append(';');
                    }
                    check("it walked to within 12 blocks of the meeting point, not put there: walkers "
                            + villages.walkers.size() + ", placed " + (RtsimTests.session().phase1.walkers.placed() - placed0[0])
                            + ", arrived " + (RtsimTests.session().phase1.walkers.arrived() - arrived0[0]) + ", trace" + trace,
                            RtsimTests.session().phase1.walkers.arrived() >= arrived0[0] + 1);
                })
                .thenExecute(() -> {
                    for (RibbitEntity r : ribbitsIn(helper, v))
                        r.discard();
                    for (ChunkPos p : forced)
                        level.setChunkForced(p.x, p.z, false);
                    FakePlayers.remove(player);
                    RtsimTests.forget(v);
                })
                .thenSucceed();
    }
}
