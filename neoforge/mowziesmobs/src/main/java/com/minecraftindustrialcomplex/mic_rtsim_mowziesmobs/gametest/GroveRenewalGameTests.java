package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest;

import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaMinion;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthi;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.SiteTestHooks;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ChunkWait;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim.sites.SiteLedger;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.GroveKind;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.MicRtsimMowziesMobs;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Far;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Grove;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestSequence;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;
import static com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.SiteLedgerGameTests.ARENA;
import static com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.SiteLedgerGameTests.DAY;
import static com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.SiteLedgerGameTests.DONE;
import static com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.SiteLedgerGameTests.SPAWN;
import static com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.SiteLedgerGameTests.TOL;
import static com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.SiteLedgerGameTests.advance;
import static com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.SiteLedgerGameTests.adopted;
import static com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.SiteLedgerGameTests.aged;
import static com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.SiteLedgerGameTests.describe;
import static com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.SiteLedgerGameTests.last;
import static com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.SiteLedgerGameTests.lines;
import static com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.SiteLedgerGameTests.nativeOnly;
import static com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.SiteLedgerGameTests.row;
import static com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.SiteLedgerGameTests.saved;

/**
 * The Umvuthana grove on the shared renewal rule, on the real engine (EQUILIBRIUM.md §2.4.7 and §4's S9 row, slice S9,
 * MZB-1210 to MZB-1216, MZB-1265): a warrior a player kills is replaced after the wait its pressure sets, one at a time; the
 * Umvuthi's pressure is its own and the warriors' is not touched by it; a reset hands the pressures over; the heir takes a
 * warrior and the grove replaces it by the same rule. Every grove is declared under the jar's real structure name so the
 * native engine runs it; each test is its own batch on its own far base. A grove's warriors on a march (a party the engine
 * sends by test order 42, {@link RtsimTests#sendParty}) and the Frostmaw killed away are left to the Rust tests: a puppet on
 * the road needs the far view's bodies, which the GameTest world does not draw.
 */
@GameTestHolder(MicRtsimMowziesMobs.MODID)
@PrefixGameTestTemplate(false)
public final class GroveRenewalGameTests {
    private GroveRenewalGameTests() {}

    /**
     * Steps: a grove declared under the jar's structure with three warriors, and an Umvuthi if {@code withUmvuthi}, all
     * adopted by the engine and its row naming the three warriors at home.
     */
    private static GameTestSequence realGrove(GameTestHelper helper, ChunkWait waits, Far far, Grove g, EntityUmvuthi[] u,
                                              EntityUmvuthanaMinion[] w, boolean withUmvuthi) {
        ServerLevel level = far.level;
        return helper.startSequence()
                .thenExecute(g::force)
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    g.lay().declare(MicRtsimMowziesMobs.GROVE_STRUCTURE, GroveKind.INSTANCE);
                    if (withUmvuthi)
                        u[0] = MowzieKit.umvuthi(level, g.at(-10, 0));
                    for (int i = 0; i < w.length; i++)
                        w[i] = MowzieKit.warrior(level, g.at(6 * i - 6, 8));
                })
                .thenWaitUntil(waits.engine(() -> {
                    if (withUmvuthi)
                        check("the engine adopted the Umvuthi", adopted(u[0], g.faction));
                    for (EntityUmvuthanaMinion m : w)
                        check("and every warrior", adopted(m, g.faction));
                }))
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, g.faction);
                    check("the grove's row names the warriors at home: " + describe(r), r != null && r.n() == w.length);
                }));
    }

    /**
     * A killed warrior is replaced after the wait (S9, MZB-1265). A player kills one of three adopted warriors: CAMP_DEATH is a
     * player's, and the row is short by one, waiting, with the warriors' P 1/K and the 6 x 2^(1/K) days of delay(P); the
     * Umvuthi's pressure (p2) is 0. Half the wait on there is no SPAWN; past it the arrival counts first (RECOUNT) and one
     * SPAWN brings a warrior to the grove's edge, so three warriors stand again.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "grove_replaced", skyAccess = true)
    public static void aKilledWarriorIsReplacedAfterTheWait(GameTestHelper helper) {
        String test = "aKilledWarriorIsReplacedAfterTheWait";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 8000);
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove g = new Grove(far, far.k, far.z0);
        EntityUmvuthi[] u = new EntityUmvuthi[1];
        EntityUmvuthanaMinion[] w = new EntityUmvuthanaMinion[3];
        ServerPlayer[] p = new ServerPlayer[1];
        SiteLedger.Row[] killed = new SiteLedger.Row[1];
        int[] deaths = new int[1], recounts = new int[1];
        long[] rest = new long[1];

        GameTestSequence s = realGrove(helper, waits, far, g, u, w, true);
        s.thenExecute(() -> {
                    deaths[0] = ConflictKit.campDeaths(g.faction).size();
                    p[0] = MowzieKit.player(helper, g.at(4, 0), "grove-farmer");
                    MowzieKit.kill(w[0], level.damageSources().playerAttack(p[0]));
                    FakePlayers.remove(p[0]);
                })
                .thenWaitUntil(waits.engine(() -> check("the warrior's CAMP_DEATH", ConflictKit.campDeaths(g.faction).size() > deaths[0])))
                .thenExecute(() -> {
                    ScriptedEngine.Received d = last(ConflictKit.campDeaths(g.faction));
                    check("a player's kill (aux " + d.aux() + ")", d.aux() == ConflictRecords.BY_PLAYER);
                    check("credited to the player", p[0].getUUID().equals(ConflictKit.uuidAt(d, ConflictRecords.CampDeath.KILLER_MOST)));
                })
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, g.faction);
                    check("the row is short by one and waiting, the warriors' P 1/K: " + describe(r),
                            r != null && r.n() == 2 && r.waiting() && Math.abs(r.p() - 1f / r.k()) < 1e-3);
                    killed[0] = r;
                }))
                .thenExecute(() -> {
                    SiteLedger.Row r = killed[0];
                    double want = 6 * DAY * Math.pow(2.0, 1.0 / r.k());
                    check(String.format("the wait is 6 x 2^(1/K) days: %d against %.0f", r.waitLeft(), want),
                            Math.abs(r.waitLeft() - want) <= 400);
                    check("the Umvuthi's pressure is untouched: " + describe(r), r.p2() == 0f);
                    advance(r.waitLeft() / 2);
                    rest[0] = r.waitLeft() - r.waitLeft() / 2;
                })
                .thenIdle(100)
                .thenExecute(() -> {
                    check("no SPAWN half way through the wait (" + lines(g.faction, SPAWN, -1) + ")", lines(g.faction, SPAWN, -1) == 0);
                    recounts[0] = MowzieKit.camps().engineRecounts;
                    advance(rest[0] + DAY / 4);
                })
                .thenWaitUntil(waits.engine(() -> check("the arrival's SPAWN is DONE", lines(g.faction, SPAWN, DONE) >= 1)))
                .thenIdle(20)
                .thenExecute(() -> {
                    check("one SPAWN (" + lines(g.faction, SPAWN, -1) + ")", lines(g.faction, SPAWN, -1) == 1);
                    List<EntityUmvuthanaMinion> standing = level.getEntitiesOfClass(EntityUmvuthanaMinion.class, g.area(), Entity::isAlive);
                    check("three warriors stand again (" + standing.size() + ")", standing.size() == 3);
                    check("the arrival counted first (RECOUNT, MZB-1015): " + recounts[0] + " before the wait's end, " + MowzieKit.camps().engineRecounts
                            + " after", MowzieKit.camps().engineRecounts > recounts[0]);
                    record(test, "replaced", "killed: " + describe(killed[0]) + "; one SPAWN past the wait, after "
                            + (MowzieKit.camps().engineRecounts - recounts[0]) + " RECOUNT");
                })
                .thenSucceed();
    }

    /**
     * A killed Umvuthi leaves the warriors' pressure alone (S9, MZB-1211, MZB-1265). A player kills the Umvuthi: the row's
     * second pressure (p2) is the Umvuthi's 1, and the warriors' (p) is 0 whatever the jar's own `die` did to the
     * warriors near it (they fall with it, no one's loss).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "grove_umvuthi_pressure", skyAccess = true)
    public static void aKilledUmvuthiLeavesTheWarriorsPressureAlone(GameTestHelper helper) {
        String test = "aKilledUmvuthiLeavesTheWarriorsPressureAlone";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 8000);
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove g = new Grove(far, far.k, far.z0);
        EntityUmvuthi[] u = new EntityUmvuthi[1];
        EntityUmvuthanaMinion[] w = new EntityUmvuthanaMinion[3];
        ServerPlayer[] p = new ServerPlayer[1];
        int[] deaths = new int[1];

        GameTestSequence s = realGrove(helper, waits, far, g, u, w, true);
        s.thenExecute(() -> {
                    deaths[0] = ConflictKit.campDeaths(g.faction).size();
                    p[0] = MowzieKit.player(helper, g.at(4, 0), "grove-sun-farmer");
                    MowzieKit.kill(u[0], level.damageSources().playerAttack(p[0]));
                    FakePlayers.remove(p[0]);
                })
                .thenWaitUntil(waits.engine(() -> check("the Umvuthi's CAMP_DEATH", ConflictKit.campDeaths(g.faction).size() > deaths[0])))
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, g.faction);
                    check("the Umvuthi's pressure is 1: " + describe(r), r != null && Math.abs(r.p2() - 1f) < 1e-2);
                    check("the warriors' is 0: " + describe(r), r.p() == 0f);
                    record(test, "row", describe(r) + String.format(", p2 %.4f", r.p2()));
                }))
                .thenSucceed();
    }

    /**
     * A reset hands the grove's pressures over (S9, MZB-1231, MZB-1265). A player kills a warrior and then the Umvuthi: the row
     * has the warriors' P 1/K and the Umvuthi's 1. A plain reset: the found tail carries both (p and p2), the new engine's
     * warriors' pressure is the row's aged, and its Umvuthi's is the cap's, 5 (the Umvuthi is dead, so the grove is found without
     * one and waits the cap for an heir, MZB-1218; {@link #aResetHandsAnAliveUmvuthisPressureOver} is the other case), and the
     * warriors' wait runs on.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "grove_reset_pressures", skyAccess = true)
    public static void aResetKeepsAGrovesTwoPressures(GameTestHelper helper) {
        String test = "aResetKeepsAGrovesTwoPressures";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 8000);
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        MinecraftServer server = level.getServer();
        ConflictKit.onEnd(helper, () -> SiteTestHooks.cleanUp(server));
        Grove g = new Grove(far, far.k, far.z0);
        EntityUmvuthi[] u = new EntityUmvuthi[1];
        EntityUmvuthanaMinion[] w = new EntityUmvuthanaMinion[3];
        ServerPlayer[] p = new ServerPlayer[1];
        SiteLedger.Row[] before = new SiteLedger.Row[1], after = new SiteLedger.Row[1];
        int[] deaths = new int[1];
        long[] t = new long[1];

        GameTestSequence s = realGrove(helper, waits, far, g, u, w, true);
        s.thenExecute(() -> {
                    deaths[0] = ConflictKit.campDeaths(g.faction).size();
                    p[0] = MowzieKit.player(helper, g.at(4, 0), "grove-reset-farmer");
                    MowzieKit.kill(w[0], level.damageSources().playerAttack(p[0]));
                })
                .thenWaitUntil(waits.engine(() -> check("the warrior's CAMP_DEATH", ConflictKit.campDeaths(g.faction).size() > deaths[0])))
                .thenExecute(() -> {
                    deaths[0] = ConflictKit.campDeaths(g.faction).size();
                    MowzieKit.kill(u[0], level.damageSources().playerAttack(p[0]));
                    FakePlayers.remove(p[0]);
                })
                .thenWaitUntil(waits.engine(() -> check("the Umvuthi's CAMP_DEATH", ConflictKit.campDeaths(g.faction).size() > deaths[0])))
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, g.faction);
                    check("both pressures are in the row: " + describe(r) + String.format(", p2 %.4f", r.p2()),
                            r != null && Math.abs(r.p() - 1f / r.k()) < 1e-3 && Math.abs(r.p2() - 1f) < 1e-2 && r.waiting());
                    before[0] = r;
                }));
        s = saved(s, waits);
        s.thenExecute(() -> {
                    t[0] = level.getGameTime();
                    SiteTestHooks.plainReset(server);
                    check("a session runs after the reset: " + RtsimSession.startFailure(), RtsimSession.current() != null);
                })
                .thenWaitUntil(waits.engine(() -> check("the new engine has the grove's FACTION_FOUND", !ConflictKit.factionsFound(g.faction).isEmpty())))
                .thenExecute(() -> {
                    java.nio.ByteBuffer v = ConflictKit.factionsFound(g.faction).get(0).view();
                    int at = ConflictRecords.FactionFound.TAIL;
                    float tp = v.getFloat(at + SiteLedger.Tail.P), tp2 = v.getFloat(at + SiteLedger.Tail.P2);
                    check(String.format("its tail carries both pressures: p %.4f (the row's %.4f), p2 %.4f (the row's %.4f)", tp,
                            before[0].p(), tp2, before[0].p2()), Math.abs(tp - before[0].p()) < 1e-3 && Math.abs(tp2 - before[0].p2()) < 1e-3);
                })
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, g.faction);
                    check("the new engine's row: " + describe(r), r != null && r.stamp() >= t[0]);
                    after[0] = r;
                }))
                .thenExecute(() -> {
                    SiteLedger.Row a = before[0], b = after[0];
                    long elapsed = b.stamp() - a.stamp();
                    check(String.format("the warriors' P is the row's aged %d ticks: %.4f against %.4f", elapsed, b.p(), aged(a.p(), elapsed)),
                            Math.abs(b.p() - aged(a.p(), elapsed)) < 1e-3);
                    // The Umvuthi is dead at the reset, so the grove is found without one and waits the cap for an heir (MZB-1218):
                    // its pressure is the cap's, log2(192 / 6) = 5, whatever the row's 1 aged.
                    check(String.format("found without its Umvuthi the grove is at the cap, 5: %.4f (the row's was %.4f)", b.p2(), a.p2()),
                            Math.abs(b.p2() - 5f) < 0.05 && b.p2() > aged(a.p2(), elapsed));
                    check("the warriors' wait runs on: " + describe(b), b.waiting());
                    check("it is the wait left less the ticks since (" + b.waitLeft() + " against " + (a.waitLeft() - elapsed) + ")",
                            Math.abs(b.waitLeft() - (a.waitLeft() - elapsed)) <= TOL);
                    record(test, "handed over", "before: " + describe(a) + String.format(", p2 %.4f", a.p2()) + "; after " + elapsed + " ticks: "
                            + describe(b) + String.format(", p2 %.4f", b.p2()));
                })
                .thenSucceed();
    }

    /**
     * A reset hands an Umvuthi that stands its pressure (S9 review, MZB-1290). A grove found with three warriors and no Umvuthi
     * waits the cap's 192 days for an heir; the pressure of that wait, the cap's 5, is then halved twice (the chief's half-life is
     * 96 days), 1.25, and the row carries it with the heir seated. A plain reset finds the Umvuthi alive: the new engine's
     * pressure is the row's aged (the found tail carries it), not the cap's again, which a grove found without its Umvuthi would
     * take (see {@link #aResetKeepsAGrovesTwoPressures}).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "grove_reset_heir", skyAccess = true)
    public static void aResetHandsAnAliveUmvuthisPressureOver(GameTestHelper helper) {
        String test = "aResetHandsAnAliveUmvuthisPressureOver";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 8000);
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        MinecraftServer server = level.getServer();
        ConflictKit.onEnd(helper, () -> SiteTestHooks.cleanUp(server));
        Grove g = new Grove(far, far.k, far.z0);
        EntityUmvuthi[] u = new EntityUmvuthi[1];
        EntityUmvuthanaMinion[] w = new EntityUmvuthanaMinion[3];
        SiteLedger.Row[] before = new SiteLedger.Row[1], after = new SiteLedger.Row[1];
        long[] t = new long[1];

        GameTestSequence s = realGrove(helper, waits, far, g, u, w, false);
        s.thenExecute(() -> {
                    check("the grove has no Umvuthi", MowzieKit.held(g.faction).stream().noneMatch(e -> e instanceof EntityUmvuthi));
                    advance(192 * DAY + DAY / 8);
                })
                .thenWaitUntil(waits.engine(() -> check("the heir's pair is DONE (MASK " + lines(g.faction, ConflictRecords.CampReconcile.MASK, DONE)
                        + ", SEAT_HEIR " + lines(g.faction, ConflictRecords.CampReconcile.SEAT_HEIR, DONE) + ")",
                        lines(g.faction, ConflictRecords.CampReconcile.MASK, DONE) >= 1
                                && lines(g.faction, ConflictRecords.CampReconcile.SEAT_HEIR, DONE) >= 1)))
                .thenWaitUntil(waits.engine(() -> {
                    check("an Umvuthi stands at the grove", MowzieKit.held(g.faction).stream().anyMatch(e -> e instanceof EntityUmvuthi));
                    SiteLedger.Row r = row(level, g.faction);
                    check("the row carries the heir wait's pressure aged, 1.25, and the warriors' is 0: " + describe(r) + String.format(", p2 %.4f", r.p2()),
                            r != null && r.n() == 2 && r.waiting() && r.p() == 0f && Math.abs(r.p2() - 1.25f) < 0.1f);
                    before[0] = r;
                }));
        s = saved(s, waits);
        s.thenExecute(() -> {
                    t[0] = level.getGameTime();
                    SiteTestHooks.plainReset(server);
                    check("a session runs after the reset: " + RtsimSession.startFailure(), RtsimSession.current() != null);
                })
                .thenWaitUntil(waits.engine(() -> check("the new engine has the grove's FACTION_FOUND", !ConflictKit.factionsFound(g.faction).isEmpty())))
                .thenExecute(() -> {
                    java.nio.ByteBuffer v = ConflictKit.factionsFound(g.faction).get(0).view();
                    float tp2 = v.getFloat(ConflictRecords.FactionFound.TAIL + SiteLedger.Tail.P2);
                    check(String.format("its tail carries the Umvuthi's pressure: p2 %.4f (the row's %.4f)", tp2, before[0].p2()),
                            Math.abs(tp2 - before[0].p2()) < 1e-3);
                })
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, g.faction);
                    check("the new engine's row: " + describe(r), r != null && r.stamp() >= t[0]);
                    after[0] = r;
                }))
                .thenExecute(() -> {
                    SiteLedger.Row a = before[0], b = after[0];
                    long elapsed = b.stamp() - a.stamp();
                    check(String.format("the Umvuthi's is the row's aged %d ticks: %.4f against %.4f (the cap's would be 5)", elapsed, b.p2(),
                            aged(a.p2(), elapsed)), Math.abs(b.p2() - aged(a.p2(), elapsed)) < 0.02);
                    record(test, "handed over", "before: " + describe(a) + String.format(", p2 %.4f", a.p2()) + "; after " + elapsed + " ticks: "
                            + describe(b) + String.format(", p2 %.4f", b.p2()));
                })
                .thenSucceed();
    }

    /**
     * The heir takes a warrior and the grove replaces it by the rule (S9, MZB-1265; EQUILIBRIUM.md S9's name
     * `anUmvuthiHeirTakesAWarrior`, the scripted half being {@code anUmvuthiHeirTakesTheFarthestFreeWarrior}). A grove found
     * with three warriors and no Umvuthi waits the cap's 192 days for an heir. The engine's world runs past it: the pair
     * (MASK, SEAT_HEIR) is answered and an Umvuthi stands; the warrior is no loss (the warriors' P is 0, no player's), the row
     * is short by one and waiting for the 6 days of delay(P = 0), and a warrior is SPAWNed when it ends.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "grove_heir_warrior", skyAccess = true)
    public static void anUmvuthiHeirTakesAWarrior(GameTestHelper helper) {
        String test = "anUmvuthiHeirTakesAWarrior";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 8000);
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove g = new Grove(far, far.k, far.z0);
        EntityUmvuthi[] u = new EntityUmvuthi[1];
        EntityUmvuthanaMinion[] w = new EntityUmvuthanaMinion[3];
        SiteLedger.Row[] seated = new SiteLedger.Row[1];
        int[] spawns = new int[1];

        GameTestSequence s = realGrove(helper, waits, far, g, u, w, false);
        s.thenExecute(() -> {
                    check("the grove has no Umvuthi", MowzieKit.held(g.faction).stream().noneMatch(e -> e instanceof EntityUmvuthi));
                    advance(192 * DAY + DAY / 8);
                })
                .thenWaitUntil(waits.engine(() -> check("the heir's pair is DONE (MASK " + lines(g.faction, ConflictRecords.CampReconcile.MASK, DONE)
                        + ", SEAT_HEIR " + lines(g.faction, ConflictRecords.CampReconcile.SEAT_HEIR, DONE) + ")",
                        lines(g.faction, ConflictRecords.CampReconcile.MASK, DONE) >= 1
                                && lines(g.faction, ConflictRecords.CampReconcile.SEAT_HEIR, DONE) >= 1)))
                .thenWaitUntil(waits.engine(() -> {
                    check("an Umvuthi stands at the grove", MowzieKit.held(g.faction).stream().anyMatch(e -> e instanceof EntityUmvuthi));
                    SiteLedger.Row r = row(level, g.faction);
                    check("the row is short by one, waiting, the warriors' P 0: " + describe(r),
                            r != null && r.n() == 2 && r.waiting() && r.p() == 0f);
                    seated[0] = r;
                }))
                .thenExecute(() -> {
                    SiteLedger.Row r = seated[0];
                    check("the wait is delay(P = 0), 6 days: " + describe(r), r.waitLeft() <= 6 * DAY && r.waitLeft() >= 6 * DAY - 2 * DAY / 8);
                    spawns[0] = lines(g.faction, SPAWN, DONE);
                    advance(r.waitLeft() + DAY / 4);
                })
                .thenWaitUntil(waits.engine(() -> check("the replacement's SPAWN is DONE", lines(g.faction, SPAWN, DONE) > spawns[0])))
                .thenIdle(20)
                .thenExecute(() -> {
                    List<EntityUmvuthanaMinion> standing = level.getEntitiesOfClass(EntityUmvuthanaMinion.class, g.area(), Entity::isAlive);
                    check("three warriors stand again (" + standing.size() + ")", standing.size() == 3);
                    record(test, "seated", describe(seated[0]) + "; a warrior SPAWNed past the 6 days");
                })
                .thenSucceed();
    }
}
