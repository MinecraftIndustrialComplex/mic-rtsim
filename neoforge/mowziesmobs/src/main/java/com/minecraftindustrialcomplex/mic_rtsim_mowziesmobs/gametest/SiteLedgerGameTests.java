package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest;

import com.bobmowzie.mowziesmobs.server.entity.frostmaw.EntityFrostmaw;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaMinion;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthi;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.SiteTestHooks;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.CampMember;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ChunkWait;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim.sites.SiteLedger;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.DenKind;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.FrostmawFights;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.GroveKind;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.MicRtsimMowziesMobs;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Den;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Far;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Grove;
import com.mojang.authlib.GameProfile;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestSequence;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.common.util.FakePlayer;
import net.neoforged.neoforge.common.util.FakePlayerFactory;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.ByteBuffer;
import java.util.List;
import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The site ledger on the real engine for Mowzie's camps (EQUILIBRIUM.md §2.3 and §4's S3 row, slice S3, MZB-1016): a farmed
 * den stays farmed across a plain reset and across a reinstall after a gap, an uninstall leaves the grove's home bodies to be
 * found again by UUID, a trap's and a machine's kill at a boss site press as a player's, a lured Frostmaw is written off as a
 * player's loss and its death later counts, and a den that never unloads refills when nobody watches. Every den and grove is
 * declared under the jar's real structure name, so the native engine runs it; each test is its own batch on its own far base,
 * and a test that resets the sim cleans up the moved pages, the marker and the capture when it ends.
 */
@GameTestHolder(MicRtsimMowziesMobs.MODID)
@PrefixGameTestTemplate(false)
public final class SiteLedgerGameTests {
    private SiteLedgerGameTests() {}

    private static final String ARENA = RtsimTests.TEMPLATE;
    private static final long DAY = 24_000L;
    /** frostmaw.json's renewal: base 6 days, so delay(P = 1) is 12 days; pressure halves in 96 days. */
    private static final long FROSTMAW_W1 = 12 * DAY;
    /** frostmaw.json's renewal cap: 192 days, the wait of a loss no one caused (MZB-1014, fork F5). */
    private static final long FROSTMAW_CAP = 192 * DAY;
    private static final double HALF_LIFE_TICKS = 96.0 * DAY;
    /**
     * How far a handed-over wait may be from the old row's less the game ticks between their stamps: the old engine's row is
     * stamped the step after the kill, the new one's the step after the found.
     */
    private static final long TOL = 40;
    private static final int SPAWN = ConflictRecords.CampReconcile.SPAWN, REMOVE = ConflictRecords.CampReconcile.REMOVE;
    private static final int DONE = ConflictRecords.CampBodies.DONE;
    private static final int ROW = SiteLedger.Tail.FLAG_ROW, GAP = SiteLedger.Tail.FLAG_GAP, WAITING = SiteLedger.Tail.FLAG_WAITING;

    // ---- helpers --------------------------------------------------------------------------------

    /** True on the native engine; else the test succeeds now, recorded as skipped (the stub runs no camp). */
    private static boolean nativeOnly(GameTestHelper helper, String test) {
        if (RtsimTests.engine() instanceof NativeEngine)
            return true;
        record(test, "skipped", "the camps are the engine's only on the native engine");
        helper.succeed();
        return false;
    }

    /** Declares the den under the jar's den structure (the frostmaw culture claims it, so the engine runs the den). */
    private static void declareReal(Den d) {
        ServerLevel level = d.far.level;
        int x = d.cx * 16 + 8, z = d.cz * 16 + 8;
        int y = level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, x, z);
        Vec3 s = new Vec3(x, y, z);
        d.faction = RtsimApi.declareCamp(level, new ChunkPos(d.cx, d.cz), MicRtsimMowziesMobs.DEN_STRUCTURE, DenKind.INSTANCE,
                BlockPos.containing(s), s, DenKind.FOOTPRINT_RADIUS, DenKind.FOOTPRINT_RADIUS, false);
        check("the den is declared (a live session)", d.faction != 0);
        d.spot = s;
    }

    private static SiteLedger.Row row(ServerLevel level, long faction) {
        return SiteTestHooks.row(level.getServer(), SiteLedger.CAMP, faction);
    }

    private static String describe(SiteLedger.Row r) {
        if (r == null)
            return "no row";
        return String.format("K %d, count %d, away %d, P %.4f, %s, wait left %d, stamp %d, home %s, left %s, missing %s", r.k(),
                r.count(), r.away(), r.p(), r.waiting() ? "waiting" : "not waiting", r.waitLeft(), r.stamp(), r.home(), r.leftBehind(),
                r.missing());
    }

    /** P as of a stamp, aged {@code ticks} by the camps' half-life. */
    private static double aged(float p, long ticks) {
        return p * Math.pow(2.0, -ticks / HALF_LIFE_TICKS);
    }

    /** CAMP_BODIES lines of the camp answering {@code action}, with {@code outcome} (any when negative), this engine. */
    private static int lines(long faction, int action, int outcome) {
        int n = 0;
        for (ScriptedEngine.Received r : ConflictKit.campBodies(faction)) {
            ByteBuffer b = r.view();
            for (int i = 0; i < r.aux(); i++) {
                int o = ConflictRecords.CampBodies.LINES + i * ConflictRecords.CampBodies.LINE;
                if ((b.get(o + ConflictRecords.CampBodies.L_ACTION) & 0xFF) == action
                        && (outcome < 0 || (b.get(o + ConflictRecords.CampBodies.L_OUTCOME) & 0xFF) == outcome))
                    n++;
            }
        }
        return n;
    }

    /** The engine's world runs {@code ticks} ahead of game time from now on (P1_ADVANCE; the camps' clock is that world's). */
    private static void advance(long ticks) {
        ((NativeEngine) RtsimTests.engine()).phase1Advance(ticks);
    }

    private static boolean adopted(Entity e, long faction) {
        CampMember m = MowzieKit.tag(e);
        return m != null && m.faction() == faction;
    }

    private static ScriptedEngine.Received last(List<ScriptedEngine.Received> l) {
        return l.get(l.size() - 1);
    }

    /** The tail flags of the camp's first FACTION_FOUND to this engine. */
    private static int tailFlags(long faction) {
        return ConflictKit.factionsFound(faction).get(0).view().getInt(ConflictRecords.FactionFound.TAIL + SiteLedger.Tail.FLAGS);
    }

    /** Steps: the den declared under the real structure with a sleeping Frostmaw at it, the engine's adoption, its row. */
    private static GameTestSequence realDen(GameTestSequence s, ChunkWait waits, Den den, EntityFrostmaw[] f) {
        ServerLevel level = den.far.level;
        return s.thenExecute(() -> {
                    den.lay();
                    declareReal(den);
                    f[0] = MowzieKit.frostmaw(level, den.at(3, 0.5), true);
                })
                .thenWaitUntil(waits.engine(() -> check("the engine adopted the den's Frostmaw", adopted(f[0], den.faction))))
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, den.faction);
                    check("the den's row names its Frostmaw at home: " + describe(r),
                            r != null && r.count() == 1 && r.home().contains(f[0].getUUID()));
                }));
    }

    /**
     * Steps: a player kills the den's Frostmaw and goes at once (DenKind holds a SPAWN while a player is within 160 blocks);
     * CAMP_DEATH is a player's, and the row is waiting at P 1 the 12 days of delay(P = 1). The row goes into {@code farmed}.
     */
    private static GameTestSequence farm(GameTestSequence s, ChunkWait waits, GameTestHelper helper, Den den, EntityFrostmaw[] f,
                                         SiteLedger.Row[] farmed, String name) {
        ServerLevel level = den.far.level;
        ServerPlayer[] p = new ServerPlayer[1];
        int[] deaths = new int[1];
        return s.thenExecute(() -> {
                    deaths[0] = ConflictKit.campDeaths(den.faction).size();
                    p[0] = MowzieKit.player(helper, den.at(4, 0), name);
                    MowzieKit.kill(f[0], level.damageSources().playerAttack(p[0]));
                    FakePlayers.remove(p[0]);
                })
                .thenWaitUntil(waits.engine(() -> check("the Frostmaw's CAMP_DEATH", ConflictKit.campDeaths(den.faction).size() > deaths[0])))
                .thenExecute(() -> {
                    ScriptedEngine.Received d = last(ConflictKit.campDeaths(den.faction));
                    check("a player's kill (aux " + d.aux() + ")", d.aux() == ConflictRecords.BY_PLAYER);
                    check("credited to the player", p[0].getUUID().equals(ConflictKit.uuidAt(d, ConflictRecords.CampDeath.KILLER_MOST)));
                    check("naming the Frostmaw (MZB-1014)", f[0].getUUID().equals(ConflictKit.uuidAt(d, ConflictRecords.CampDeath.BODY_MOST)));
                })
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, den.faction);
                    check("the den's row is farmed (empty, waiting, P 1): " + describe(r),
                            r != null && r.count() == 0 && r.waiting() && Math.abs(r.p() - 1f) < 1e-3);
                    farmed[0] = r;
                }))
                .thenExecute(() -> check("the wait is delay(P = 1), 12 days: " + describe(farmed[0]),
                        farmed[0].waitLeft() <= FROSTMAW_W1 && farmed[0].waitLeft() >= FROSTMAW_W1 - 400));
    }

    /** Steps: an engine save asked for and acknowledged through everything journaled so far (the reset's premise, RG13). */
    private static GameTestSequence saved(GameTestSequence s, ChunkWait waits) {
        long[] upTo = new long[1];
        return s.thenExecute(() -> {
                    upTo[0] = SiteTestHooks.journalEnd();
                    SiteTestHooks.requestSave();
                })
                .thenWaitUntil(waits.engine(() -> check("an engine save covers the journal through " + upTo[0], SiteTestHooks.savedThrough(upTo[0]))));
    }

    /**
     * Steps after a gap's restart: the budget of the logic waits starts again. {@link SiteTestHooks#gapRestart} moves the game
     * clock on by the gap, and a logic wait counts the test's ticks (the game's since the test began) from the last chunk wait's
     * end ({@link ChunkWait#logic}): a gap of a day or three is past any budget at once, and the first failing poll of the next
     * wait ends the test. The test's tick count is read at the start of each tick, so the jump shows from the tick after the
     * restart's: wait for it, then a chunk wait (the den's chunks are forced and stand through the restart, so it passes at once)
     * marks the tick again.
     */
    private static GameTestSequence clockMoved(GameTestSequence s, ChunkWait waits, GameTestHelper helper, Far far) {
        long[] at = new long[1];
        return s.thenExecute(() -> at[0] = helper.getTick())
                .thenWaitUntil(() -> check("the tick after the restart", helper.getTick() > at[0]))
                .thenWaitUntil(waits.chunks(far.ready()));
    }

    // ---- 1: a plain reset ------------------------------------------------------------------------

    /**
     * A reset keeps a farmed site farmed (S3 test 1). A den's Frostmaw, adopted by the engine, is killed by a player: the
     * row is waiting at P 1 for delay(P = 1). A plain reset (the pages moved aside, a new engine, no gap): the den's census
     * hands the row over in FACTION_FOUND's tail (ROW and WAITING, no GAP), and the new engine's row has P aged from 1 and
     * the wait left less the game ticks between the two rows' stamps, within {@link #TOL}. Half the wait on, no SPAWN; past
     * it, the arrival counts first (RECOUNT) and one SPAWN brings a new Frostmaw.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "site_reset_farmed", skyAccess = true)
    public static void resetKeepsAFarmedSiteFarmed(GameTestHelper helper) {
        String test = "resetKeepsAFarmedSiteFarmed";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 8000);
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        MinecraftServer server = level.getServer();
        ConflictKit.onEnd(helper, () -> SiteTestHooks.cleanUp(server));
        Den den = new Den(far, far.k, far.z0);
        EntityFrostmaw[] f = new EntityFrostmaw[1];
        SiteLedger.Row[] farmed = new SiteLedger.Row[1], handed = new SiteLedger.Row[1];
        long[] t = new long[2];

        GameTestSequence s = helper.startSequence().thenExecute(den::force).thenWaitUntil(waits.chunks(far.ready()));
        s = realDen(s, waits, den, f);
        s = farm(s, waits, helper, den, f, farmed, "site-farmer");
        s = saved(s, waits);
        s.thenExecute(() -> {
                    t[0] = level.getGameTime();
                    SiteTestHooks.plainReset(server);
                    check("a session runs after the reset: " + RtsimSession.startFailure(), RtsimSession.current() != null);
                })
                .thenWaitUntil(waits.engine(() -> check("the new engine has the den's FACTION_FOUND", !ConflictKit.factionsFound(den.faction).isEmpty())))
                .thenExecute(() -> {
                    int flags = tailFlags(den.faction);
                    check("its tail carries the row, waiting, with no gap (flags " + flags + ")",
                            (flags & ROW) != 0 && (flags & WAITING) != 0 && (flags & GAP) == 0);
                })
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, den.faction);
                    check("the new engine's row: " + describe(r), r != null && r.stamp() >= t[0]);
                    handed[0] = r;
                }))
                .thenExecute(() -> {
                    SiteLedger.Row a = farmed[0], b = handed[0];
                    long elapsed = b.stamp() - a.stamp();
                    double p = aged(a.p(), elapsed);
                    long wait = a.waitLeft() - elapsed;
                    record(test, "handed over", "before: " + describe(a) + "; after " + elapsed + " ticks: " + describe(b)
                            + String.format("; expected P %.4f, wait left %d", p, wait));
                    check(String.format("still farmed: P %.4f is 1 aged %d ticks, %.4f", b.p(), elapsed, p), Math.abs(b.p() - p) < 1e-3);
                    check("waiting the wait left less the ticks since (" + b.waitLeft() + " against " + wait + ")",
                            b.waiting() && Math.abs(b.waitLeft() - wait) <= TOL);
                    check("no Frostmaw at the den", MowzieKit.held(den.faction).isEmpty());
                    advance(b.waitLeft() / 2);
                    t[1] = b.waitLeft() - b.waitLeft() / 2;
                })
                .thenIdle(100)
                .thenExecute(() -> {
                    check("no SPAWN half way through the wait (" + lines(den.faction, SPAWN, -1) + ")", lines(den.faction, SPAWN, -1) == 0);
                    check("no Frostmaw at the den", MowzieKit.held(den.faction).isEmpty());
                    advance(t[1] + DAY / 4);
                })
                .thenWaitUntil(waits.engine(() -> check("the arrival's SPAWN is DONE", lines(den.faction, SPAWN, DONE) >= 1)))
                .thenIdle(20)
                .thenExecute(() -> {
                    check("one SPAWN (" + lines(den.faction, SPAWN, -1) + ")", lines(den.faction, SPAWN, -1) == 1);
                    List<Entity> h = MowzieKit.held(den.faction);
                    check("one new Frostmaw at the den (" + h.size() + ")", h.size() == 1 && h.get(0) instanceof EntityFrostmaw
                            && !h.get(0).getUUID().equals(f[0].getUUID()));
                    check("the arrival counted first (RECOUNT, MZB-1015): " + MowzieKit.camps().engineRecounts,
                            MowzieKit.camps().engineRecounts >= 1);
                    record(test, "refilled", "no SPAWN half way through the handed-over wait; one SPAWN past it, after "
                            + MowzieKit.camps().engineRecounts + " RECOUNT");
                })
                .thenSucceed();
    }

    // ---- 3: a reinstall after a gap ----------------------------------------------------------------

    /**
     * A reinstall keeps the pressure (S3 test 3). A den farmed as in test 1; the old engine's clock is then run two days
     * ahead (P1_ADVANCE), which no row records. An engine save, then the reinstall guard's emulation of three days without
     * rtsim: the guard moves the pages aside and the new engine's clock is game time, below the old one. The found tail
     * carries GAP; the new row's wait left is the old one less the game ticks between the stamps (within {@link #TOL}), so
     * the wait ends at the same game time and never shortens, and P is 1 aged by those ticks.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "site_reinstall_pressure", skyAccess = true)
    public static void reinstallKeepsPressure(GameTestHelper helper) {
        String test = "reinstallKeepsPressure";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 8000);
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        MinecraftServer server = level.getServer();
        ConflictKit.onEnd(helper, () -> SiteTestHooks.cleanUp(server));
        Den den = new Den(far, far.k, far.z0);
        EntityFrostmaw[] f = new EntityFrostmaw[1];
        SiteLedger.Row[] farmed = new SiteLedger.Row[1], handed = new SiteLedger.Row[1];
        long[] t = new long[1];
        long gap = 3 * DAY;

        GameTestSequence s = helper.startSequence().thenExecute(den::force).thenWaitUntil(waits.chunks(far.ready()));
        s = realDen(s, waits, den, f);
        s = farm(s, waits, helper, den, f, farmed, "site-reinstall-farmer");
        s = s.thenExecute(() -> advance(2 * DAY)).thenIdle(40);
        s = saved(s, waits);
        s.thenExecute(() -> {
                    SiteLedger.Row r = row(level, den.faction);
                    check("the old engine's run ahead wrote no row: " + describe(r), r != null && r.stamp() == farmed[0].stamp());
                    t[0] = level.getGameTime();
                    check("the guard reset the sim", SiteTestHooks.gapRestart(server, gap) != null);
                    check("a session runs after the reinstall: " + RtsimSession.startFailure(), RtsimSession.current() != null);
                });
        s = clockMoved(s, waits, helper, far);
        s.thenWaitUntil(waits.engine(() -> check("the new engine has the den's FACTION_FOUND", !ConflictKit.factionsFound(den.faction).isEmpty())))
                .thenExecute(() -> {
                    int flags = tailFlags(den.faction);
                    check("its tail carries the row, waiting, and GAP (flags " + flags + ")",
                            (flags & ROW) != 0 && (flags & WAITING) != 0 && (flags & GAP) != 0);
                })
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, den.faction);
                    check("the new engine's row: " + describe(r), r != null && r.stamp() >= t[0] + gap);
                    handed[0] = r;
                }))
                .thenExecute(() -> {
                    SiteLedger.Row a = farmed[0], b = handed[0];
                    long elapsed = b.stamp() - a.stamp();
                    double p = aged(a.p(), elapsed);
                    long wait = a.waitLeft() - elapsed;
                    long dueBefore = a.stamp() + a.waitLeft(), dueAfter = b.stamp() + b.waitLeft();
                    record(test, "handed over", "before: " + describe(a) + "; after " + elapsed + " game ticks (a gap of " + gap
                            + "): " + describe(b) + String.format("; expected P %.4f, wait left %d; due at game time %d, then %d", p, wait,
                            dueBefore, dueAfter));
                    check(String.format("P %.4f is 1 aged %d ticks, %.4f", b.p(), elapsed, p), Math.abs(b.p() - p) < 1e-3);
                    check("waiting the wait left less the game ticks since (" + b.waitLeft() + " against " + wait + ")",
                            b.waiting() && Math.abs(b.waitLeft() - wait) <= TOL);
                    check("the wait ends at the same game time, never sooner (" + dueAfter + " against " + dueBefore + ")",
                            dueAfter >= dueBefore - TOL);
                    check("no Frostmaw at the den", MowzieKit.held(den.faction).isEmpty());
                })
                .thenSucceed();
    }

    // ---- 4: an uninstall -------------------------------------------------------------------------

    /**
     * An uninstall leaves the home bodies (S3 test 4). A grove declared under the jar's structure with its Umvuthi and three
     * warriors, all adopted by the engine, its row naming the three warriors at home. The session stops (the uninstall);
     * with no rtsim running, one warrior is killed (a vanilla death, nothing journaled), and every other body still stands.
     * The reinstall after a day's gap: the grove's tail carries GAP and its home UUIDs, and the new engine's first census
     * finds the two survivors by their UUIDs (its row names them at home) with no SPAWN and no REMOVE, and no third
     * warrior made. Two days on (the daily recount, MZB-1015) the dead warrior is written off as missing with P unchanged
     * (Other, not a player's loss). The design's party out at the uninstall is left to the Rust tests: only the engine's own
     * decision sends a party, which a GameTest cannot ask for on the real engine (MZB-1016).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "site_uninstall_bodies", skyAccess = true)
    public static void uninstallLeavesHomeBodies(GameTestHelper helper) {
        String test = "uninstallLeavesHomeBodies";
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
        int[] census = new int[1];
        long[] t = new long[1];

        GameTestSequence s = helper.startSequence()
                .thenExecute(g::force)
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    g.lay().declare(MicRtsimMowziesMobs.GROVE_STRUCTURE, GroveKind.INSTANCE);
                    u[0] = MowzieKit.umvuthi(level, g.at(-10, 0));
                    for (int i = 0; i < w.length; i++)
                        w[i] = MowzieKit.warrior(level, g.at(6 * i - 6, 8));
                })
                .thenWaitUntil(waits.engine(() -> {
                    check("the engine adopted the Umvuthi", adopted(u[0], g.faction));
                    for (EntityUmvuthanaMinion m : w)
                        check("and every warrior", adopted(m, g.faction));
                }))
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, g.faction);
                    check("the grove's row names the three warriors at home: " + describe(r), r != null && r.n() == 3
                            && r.home().contains(w[0].getUUID()) && r.home().contains(w[1].getUUID()) && r.home().contains(w[2].getUUID()));
                    before[0] = r;
                }));
        s = saved(s, waits);
        s.thenExecute(() -> {
                    SiteTestHooks.stop(server);
                    check("no session runs (the uninstall)", RtsimSession.current() == null);
                    MowzieKit.kill(w[0], level.damageSources().generic());
                })
                .thenIdle(20)
                .thenExecute(() -> {
                    check("the warrior killed while rtsim was out is dead", w[0].isRemoved() || !w[0].isAlive());
                    check("the other warriors still stand", !w[1].isRemoved() && w[1].isAlive() && !w[2].isRemoved() && w[2].isAlive());
                    check("and the Umvuthi", !u[0].isRemoved() && u[0].isAlive());
                    t[0] = level.getGameTime();
                    check("the guard reset the sim", SiteTestHooks.gapRestart(server, DAY) != null);
                    check("a session runs after the reinstall: " + RtsimSession.startFailure(), RtsimSession.current() != null);
                });
        s = clockMoved(s, waits, helper, far);
        s.thenWaitUntil(waits.engine(() -> check("the new engine has the grove's FACTION_FOUND", !ConflictKit.factionsFound(g.faction).isEmpty())))
                .thenExecute(() -> {
                    int flags = tailFlags(g.faction);
                    check("its tail carries the row and GAP (flags " + flags + ")", (flags & ROW) != 0 && (flags & GAP) != 0);
                })
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, g.faction);
                    check("the new engine's row names the survivors at home: " + describe(r), r != null && r.stamp() >= t[0]
                            && r.home().contains(w[1].getUUID()) && r.home().contains(w[2].getUUID()));
                }))
                .thenIdle(20)
                .thenExecute(() -> {
                    check("the survivors are the grove's", adopted(w[1], g.faction) && adopted(w[2], g.faction));
                    check("no SPAWN (" + lines(g.faction, SPAWN, -1) + ")", lines(g.faction, SPAWN, -1) == 0);
                    check("no REMOVE (" + lines(g.faction, REMOVE, -1) + ")", lines(g.faction, REMOVE, -1) == 0);
                    List<EntityUmvuthanaMinion> standing = level.getEntitiesOfClass(EntityUmvuthanaMinion.class, g.area(), Entity::isAlive);
                    check("two warriors stand, no double (" + standing.size() + ")", standing.size() == 2);
                    census[0] = MowzieKit.censusCount(g.faction);
                    advance(DAY + DAY / 8);
                })
                .thenWaitUntil(waits.engine(() -> check("the daily recount's census", MowzieKit.censusCount(g.faction) > census[0])))
                .thenExecute(() -> {
                    census[0] = MowzieKit.censusCount(g.faction);
                    advance(DAY + DAY / 8);
                })
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, g.faction);
                    check("the dead warrior is written off as missing: " + describe(r), r != null && r.missing().contains(w[0].getUUID())
                            && !r.home().contains(w[0].getUUID()));
                    after[0] = r;
                }))
                .thenExecute(() -> {
                    SiteLedger.Row a = before[0], b = after[0];
                    record(test, "rows", "before the uninstall: " + describe(a) + "; after the write-off: " + describe(b) + "; recounts "
                            + MowzieKit.camps().engineRecounts);
                    check(String.format("P unchanged: Other, not a player's loss (%.4f, then %.4f)", a.p(), b.p()), b.p() <= a.p() + 1e-3);
                    check("the survivors still the grove's", adopted(w[1], g.faction) && adopted(w[2], g.faction));
                    check("still no REMOVE", lines(g.faction, REMOVE, -1) == 0);
                })
                .thenSucceed();
    }

    /**
     * An uninstall at a boss's den (S3 test 4's den variant, the review's P3 3, MZB-1405): a Frostmaw killed while rtsim was out
     * is no one's loss after the reinstall (fork F5's GAP rule). The den's Frostmaw, adopted, its row naming it at home. The
     * session stops (the uninstall); with no rtsim running it is killed (a vanilla death, nothing journaled). The reinstall
     * after a day's gap: the den's tail carries ROW and GAP, the new engine expects the Frostmaw at home by its UUID (count 1)
     * and spawns no Frostmaw for it. Its first census and two daily recounts miss it: it is written off as missing with P
     * unchanged (Other, not a player's loss, where a write-off with no gap is a player's at P 1 and a 12-day wait), and the
     * den waits the cap's 192 days. The grove's test cannot show this: its warriors have no member renewal.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "site_uninstall_den", skyAccess = true)
    public static void uninstallAtADenMakesAFrostmawKilledMeanwhileNoOnesLoss(GameTestHelper helper) {
        String test = "uninstallAtADenMakesAFrostmawKilledMeanwhileNoOnesLoss";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 8000);
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        MinecraftServer server = level.getServer();
        ConflictKit.onEnd(helper, () -> SiteTestHooks.cleanUp(server));
        Den den = new Den(far, far.k, far.z0);
        EntityFrostmaw[] f = new EntityFrostmaw[1];
        SiteLedger.Row[] before = new SiteLedger.Row[1], after = new SiteLedger.Row[1];
        int[] census = new int[1];
        long[] t = new long[1];

        GameTestSequence s = helper.startSequence().thenExecute(den::force).thenWaitUntil(waits.chunks(far.ready()));
        s = realDen(s, waits, den, f);
        s = s.thenExecute(() -> before[0] = row(level, den.faction));
        s = saved(s, waits);
        s.thenExecute(() -> {
                    SiteTestHooks.stop(server);
                    check("no session runs (the uninstall)", RtsimSession.current() == null);
                    MowzieKit.kill(f[0], level.damageSources().generic());
                })
                .thenIdle(20)
                .thenExecute(() -> {
                    check("the Frostmaw killed while rtsim was out is dead", f[0].isRemoved() || !f[0].isAlive());
                    t[0] = level.getGameTime();
                    check("the guard reset the sim", SiteTestHooks.gapRestart(server, DAY) != null);
                    check("a session runs after the reinstall: " + RtsimSession.startFailure(), RtsimSession.current() != null);
                });
        s = clockMoved(s, waits, helper, far);
        s.thenWaitUntil(waits.engine(() -> check("the new engine has the den's FACTION_FOUND", !ConflictKit.factionsFound(den.faction).isEmpty())))
                .thenExecute(() -> {
                    int flags = tailFlags(den.faction);
                    check("its tail carries the row and GAP (flags " + flags + ")", (flags & ROW) != 0 && (flags & GAP) != 0);
                })
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, den.faction);
                    check("the new engine expects the Frostmaw at home: " + describe(r), r != null && r.stamp() >= t[0] && r.count() == 1
                            && r.home().contains(f[0].getUUID()));
                }))
                .thenIdle(20)
                .thenExecute(() -> {
                    check("no SPAWN for it (" + lines(den.faction, SPAWN, -1) + ")", lines(den.faction, SPAWN, -1) == 0);
                    census[0] = MowzieKit.censusCount(den.faction);
                    advance(DAY + DAY / 8);
                })
                .thenWaitUntil(waits.engine(() -> check("the daily recount's census", MowzieKit.censusCount(den.faction) > census[0])))
                .thenExecute(() -> advance(DAY + DAY / 8))
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, den.faction);
                    check("the dead Frostmaw is written off as missing: " + describe(r), r != null && r.missing().contains(f[0].getUUID())
                            && r.count() == 0);
                    after[0] = r;
                }))
                .thenExecute(() -> {
                    SiteLedger.Row a = before[0], b = after[0];
                    record(test, "rows", "before the uninstall: " + describe(a) + "; after the write-off: " + describe(b));
                    check(String.format("P unchanged: Other, not a player's loss (%.4f, then %.4f)", a.p(), b.p()), b.p() <= a.p() + 1e-3);
                    check("the cap's wait (" + b.waitLeft() + " against " + FROSTMAW_CAP + "): " + describe(b),
                            b.waiting() && Math.abs(b.waitLeft() - FROSTMAW_CAP) <= TOL);
                    check("still no SPAWN", lines(den.faction, SPAWN, -1) == 0);
                })
                .thenSucceed();
    }

    // ---- 6: a trap's and a machine's kill ---------------------------------------------------------

    /**
     * A trap's kill at a boss site is a player's loss (S3 test 6). Two dens, each with its Frostmaw adopted. Den A's dies in
     * lava with no hit: CAMP_DEATH aux NONE with no killer. Den B's is killed by a NeoForge FakePlayer (a machine,
     * MZB-111): aux OTHER with no killer. Both rows are waiting at P 1: the boss rule books either as a player's loss.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "site_trap_kill", skyAccess = true)
    public static void trapKillAtABossSiteIsAPlayerLoss(GameTestHelper helper) {
        String test = "trapKillAtABossSiteIsAPlayerLoss";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 4000);
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Den a = new Den(far, far.k, far.z0), b = new Den(far, far.k + 20, far.z0);
        EntityFrostmaw[] fa = new EntityFrostmaw[1], fb = new EntityFrostmaw[1];

        GameTestSequence s = helper.startSequence()
                .thenExecute(() -> {
                    a.force();
                    b.force();
                })
                .thenWaitUntil(waits.chunks(far.ready()));
        s = realDen(s, waits, a, fa);
        s = realDen(s, waits, b, fb);
        s.thenExecute(() -> {
                    MowzieKit.kill(fa[0], level.damageSources().lava());
                    FakePlayer machine = FakePlayerFactory.get(level, new GameProfile(UUID.randomUUID(), "site-machine"));
                    machine.setPos(fb[0].getX() + 3, fb[0].getY(), fb[0].getZ());
                    MowzieKit.kill(fb[0], level.damageSources().playerAttack(machine));
                })
                .thenWaitUntil(waits.engine(() -> check("both CAMP_DEATHs", !ConflictKit.campDeaths(a.faction).isEmpty()
                        && !ConflictKit.campDeaths(b.faction).isEmpty())))
                .thenExecute(() -> {
                    ScriptedEngine.Received da = ConflictKit.campDeaths(a.faction).get(0), db = ConflictKit.campDeaths(b.faction).get(0);
                    check("den A's in lava: aux NONE (" + da.aux() + ")", da.aux() == ConflictRecords.BY_NONE);
                    check("with no killer", ConflictKit.uuidAt(da, ConflictRecords.CampDeath.KILLER_MOST) == null);
                    check("den B's by a machine: aux OTHER (" + db.aux() + ")", db.aux() == ConflictRecords.BY_OTHER);
                    check("with no killer", ConflictKit.uuidAt(db, ConflictRecords.CampDeath.KILLER_MOST) == null);
                })
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row ra = row(level, a.faction), rb = row(level, b.faction);
                    check("den A's row is waiting at P 1: " + describe(ra), ra != null && ra.count() == 0 && ra.waiting() && Math.abs(ra.p() - 1f) < 1e-3);
                    check("den B's row is waiting at P 1: " + describe(rb), rb != null && rb.count() == 0 && rb.waiting() && Math.abs(rb.p() - 1f) < 1e-3);
                }))
                .thenExecute(() -> record(test, "rows", "lava: " + describe(row(level, a.faction)) + "; machine: " + describe(row(level, b.faction))))
                .thenSucceed();
    }

    // ---- 8: a lured body --------------------------------------------------------------------------

    /** Both Frostmaws' health for their fight (MZB-1404): the jar's 250 would make a fight of a few thousand ticks. */
    private static final float FIGHT_HEALTH = 30f;

    /**
     * A lured body is a player's loss, and back at its full den it fights its replacement to the death (S3 test 8; fork F3 as
     * Ben changed it, 2026-10-30, MZB-1404). A den's Frostmaw, adopted, is taken 400 blocks off into a chunk that is then
     * released: its hide asks for a census (MZB-1015), which misses it, and the engine's daily recounts miss it twice more
     * over two days: it is written off as a player's loss (P 1), its UUID missing. Past the wait a new Frostmaw spawns at the
     * den. The far chunk forced again, the lured Frostmaw loads with its old tags 400 blocks off: it is not removed and is
     * still missing (no fight from there). It walks home (moved into the den, its AI on again) and the den counts: the census
     * sets the two on each other (FIGHT). Both have {@link #FIGHT_HEALTH} so the fight is short. Within the fight's bound one
     * Frostmaw is left of the two, whichever it is; the row names it at home, count 1, with nobody missing, no wait (no
     * renewal owed) and the pressure the write-off left, aged (the fight's death books nothing). Its CAMP_DEATH names the
     * dead one's UUID.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "site_lured_body", skyAccess = true)
    public static void aLuredBodyIsAPlayerLossAndFightsItsReplacementOnItsReturn(GameTestHelper helper) {
        String test = "aLuredBodyIsAPlayerLossAndFightsItsReplacementOnItsReturn";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 8000);
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Den den = new Den(far, far.k, far.z0);
        int lx = den.cx + 25, lz = den.cz;
        EntityFrostmaw[] f = new EntityFrostmaw[1];
        UUID[] id = new UUID[1];
        Entity[] lured = new Entity[1];
        SiteLedger.Row[] off = new SiteLedger.Row[1];
        EntityFrostmaw[] replacement = new EntityFrostmaw[1], survivor = new EntityFrostmaw[1];
        int[] census = new int[1], deaths = new int[1];

        GameTestSequence s = helper.startSequence()
                .thenExecute(() -> {
                    den.force();
                    far.force(lx, lz);
                })
                .thenWaitUntil(waits.chunks(far.ready()));
        s = realDen(s, waits, den, f);
        s.thenExecute(() -> {
                    id[0] = f[0].getUUID();
                    census[0] = MowzieKit.censusCount(den.faction);
                    int x = lx * 16 + 8, z = lz * 16 + 8;
                    f[0].setNoAi(true);
                    f[0].teleportTo(x + 0.5, level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, x, z), z + 0.5);
                    far.release(lx, lz);
                })
                .thenWaitUntil(waits.chunks(MowzieKit.removed(level, () -> f[0])))
                .thenWaitUntil(waits.engine(() -> check("the hide asked for a census (" + MowzieKit.camps().leaveRecounts + " asked)",
                        MowzieKit.censusCount(den.faction) > census[0] && MowzieKit.camps().leaveRecounts >= 1)))
                .thenExecute(() -> {
                    census[0] = MowzieKit.censusCount(den.faction);
                    advance(DAY + DAY / 8);
                })
                .thenWaitUntil(waits.engine(() -> check("the daily recount's census", MowzieKit.censusCount(den.faction) > census[0])))
                .thenExecute(() -> advance(DAY + DAY / 8))
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, den.faction);
                    check("the Frostmaw is written off, missing: " + describe(r), r != null && r.missing().contains(id[0]) && r.count() == 0);
                    off[0] = r;
                }))
                .thenExecute(() -> {
                    SiteLedger.Row r = off[0];
                    check(String.format("a player's loss: P %.4f is near 1", r.p()), Math.abs(r.p() - 1f) < 0.01);
                    check("waiting: " + describe(r), r.waiting());
                    record(test, "written off", describe(r) + " after " + MowzieKit.camps().engineRecounts + " engine RECOUNTs");
                    advance(r.waitLeft() + DAY / 4);
                })
                .thenWaitUntil(waits.engine(() -> check("the arrival's SPAWN is DONE", lines(den.faction, SPAWN, DONE) >= 1)))
                .thenExecute(() -> {
                    List<Entity> h = MowzieKit.held(den.faction);
                    check("a new Frostmaw at the den (" + h.size() + ")", h.size() == 1 && !h.get(0).getUUID().equals(id[0]));
                    replacement[0] = (EntityFrostmaw) h.get(0);
                    census[0] = MowzieKit.censusCount(den.faction);
                    far.force(lx, lz);
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenWaitUntil(waits.engine(() -> {
                    lured[0] = level.getEntity(id[0]);
                    check("the lured Frostmaw is back in the world", lured[0] instanceof EntityFrostmaw);
                    check("and a census came after it (" + MowzieKit.censusCount(den.faction) + ")", MowzieKit.censusCount(den.faction) > census[0]);
                }))
                .thenIdle(40)
                .thenExecute(() -> {
                    check("the lured Frostmaw is not removed", !lured[0].isRemoved() && lured[0].isAlive());
                    check("no REMOVE (" + lines(den.faction, REMOVE, -1) + ")", lines(den.faction, REMOVE, -1) == 0);
                    SiteLedger.Row r = row(level, den.faction);
                    check("still missing, 400 blocks off: " + describe(r), r != null && r.missing().contains(id[0]));
                    check("no fight from there (" + MowzieKit.camps().fights + ")", MowzieKit.camps().fights == 0);
                    // It walks home: into the den, its AI on again (the lure turned it off).
                    EntityFrostmaw back = (EntityFrostmaw) lured[0];
                    Vec3 home = den.at(6, 0);
                    back.setNoAi(false);
                    back.teleportTo(home.x, home.y, home.z);
                    back.setHealth(FIGHT_HEALTH);
                    replacement[0].setHealth(FIGHT_HEALTH);
                    deaths[0] = ConflictKit.campDeaths(den.faction).size();
                    MowzieKit.camps().forceRecount(den.faction);
                })
                .thenWaitUntil(waits.engine(() -> check("the den's census set the two on each other (FIGHT " + MowzieKit.camps().fights + ")",
                        MowzieKit.camps().fights >= 1)))
                .thenWaitUntil(waits.engine(() -> {
                    boolean a = replacement[0].isAlive(), b = lured[0].isAlive();
                    check("one Frostmaw is left of the two (the replacement " + (a ? "stands" : "fell") + ", the returner "
                            + (b ? "stands" : "fell") + "; health " + replacement[0].getHealth() + ", " + ((EntityFrostmaw) lured[0]).getHealth()
                            + "; targets " + replacement[0].getTarget() + ", " + ((EntityFrostmaw) lured[0]).getTarget() + ")", a != b);
                }))
                .thenExecute(() -> survivor[0] = replacement[0].isAlive() ? replacement[0] : (EntityFrostmaw) lured[0])
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, den.faction);
                    check("the row names the survivor at home, count 1, nobody missing: " + describe(r), r != null && r.count() == 1
                            && r.home().equals(List.of(survivor[0].getUUID())) && r.missing().isEmpty());
                }))
                .thenIdle(20)
                .thenExecute(() -> {
                    SiteLedger.Row r = row(level, den.faction);
                    SiteLedger.Row w = off[0];
                    check("no renewal owed: " + describe(r), !r.waiting());
                    // The engine's clock ran on by the wait's advance (P1_ADVANCE), which the stamps (game time) do not show; a
                    // day more for the engine's whole days.
                    long ran = w.waitLeft() + DAY / 4 + (r.stamp() - w.stamp()) + DAY;
                    check(String.format("P is the write-off's, aged: no loss booked for the fight (%.4f, then %.4f)", w.p(), r.p()),
                            r.p() <= w.p() + 1e-3 && r.p() >= aged(w.p(), ran) - 1e-3);
                    List<EntityFrostmaw> standing = level.getEntitiesOfClass(EntityFrostmaw.class, den.area().inflate(32), Entity::isAlive);
                    check("exactly one Frostmaw at the den (" + standing.size() + ")", standing.size() == 1 && standing.get(0) == survivor[0]);
                    check("the den's own (" + MowzieKit.held(den.faction).size() + ")", adopted(survivor[0], den.faction));
                    EntityFrostmaw dead = survivor[0] == replacement[0] ? (EntityFrostmaw) lured[0] : replacement[0];
                    ScriptedEngine.Received d = last(ConflictKit.campDeaths(den.faction));
                    check("the dead one's CAMP_DEATH names it", ConflictKit.campDeaths(den.faction).size() > deaths[0]
                            && dead.getUUID().equals(ConflictKit.uuidAt(d, ConflictRecords.CampDeath.BODY_MOST)));
                    record(test, "the fight", (survivor[0] == replacement[0] ? "the replacement" : "the returner") + " survived; its kill aux "
                            + d.aux() + "; before: " + describe(w) + "; after: " + describe(r) + "; FrostmawFights started "
                            + FrostmawFights.started + ", set again " + FrostmawFights.reasserted + ", over " + FrostmawFights.ended);
                })
                .thenSucceed();
    }

    // ---- 9: a den that never unloads ----------------------------------------------------------------

    /**
     * A force-loaded site refills when unwatched (S3 test 9). Two dens forced the whole test, their Frostmaws adopted and
     * killed by a player who goes. A watcher stands 40 blocks from den B. The engine's world runs past both waits with no
     * unload: den A's arrival asks for a RECOUNT, its census follows with no CAMP_OUT, and a SPAWN places a tagged Frostmaw.
     * Den B's SPAWN is held while the watcher stands within 160 blocks (DenKind) and placed once the watcher goes.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "site_force_loaded", skyAccess = true)
    public static void aForceLoadedSiteRefillsWhenUnwatched(GameTestHelper helper) {
        String test = "aForceLoadedSiteRefillsWhenUnwatched";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 8000);
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Den a = new Den(far, far.k, far.z0), b = new Den(far, far.k + 20, far.z0);
        EntityFrostmaw[] fa = new EntityFrostmaw[1], fb = new EntityFrostmaw[1];
        SiteLedger.Row[] ra = new SiteLedger.Row[1], rb = new SiteLedger.Row[1];
        ServerPlayer[] watcher = new ServerPlayer[1];
        long[] before = new long[5];

        GameTestSequence s = helper.startSequence()
                .thenExecute(() -> {
                    a.force();
                    b.force();
                })
                .thenWaitUntil(waits.chunks(far.ready()));
        s = realDen(s, waits, a, fa);
        s = realDen(s, waits, b, fb);
        s = farm(s, waits, helper, a, fa, ra, "site-farmer-a");
        s = farm(s, waits, helper, b, fb, rb, "site-farmer-b");
        s.thenExecute(() -> {
                    watcher[0] = MowzieKit.player(helper, b.at(40, 0), "site-watcher");
                    before[0] = MowzieKit.camps().engineRecounts;
                    before[1] = MowzieKit.censusCount(a.faction);
                    before[2] = ConflictKit.campOuts(a.faction).size();
                    before[3] = MowzieKit.camps().heldSpawns;
                    advance(Math.max(ra[0].waitLeft(), rb[0].waitLeft()) + DAY / 4);
                })
                .thenWaitUntil(waits.engine(() -> check("den A's SPAWN is DONE", lines(a.faction, SPAWN, DONE) >= 1)))
                .thenExecute(() -> {
                    check("the arrivals asked for RECOUNTs (" + (MowzieKit.camps().engineRecounts - before[0]) + ")",
                            MowzieKit.camps().engineRecounts >= before[0] + 1);
                    check("den A took a census (" + MowzieKit.censusCount(a.faction) + " against " + before[1] + ")",
                            MowzieKit.censusCount(a.faction) > before[1]);
                    check("with no unload: no CAMP_OUT", ConflictKit.campOuts(a.faction).size() == before[2]);
                    List<Entity> h = MowzieKit.held(a.faction);
                    check("a tagged Frostmaw at den A (" + h.size() + ")", h.size() == 1 && h.get(0) instanceof EntityFrostmaw);
                })
                .thenWaitUntil(waits.engine(() -> check("den B's SPAWN is held while the watcher stands near ("
                        + (MowzieKit.camps().heldSpawns - before[3]) + " held)", MowzieKit.camps().heldSpawns > before[3])))
                .thenIdle(40)
                .thenExecute(() -> {
                    check("no SPAWN placed at den B yet", lines(b.faction, SPAWN, -1) == 0 && MowzieKit.held(b.faction).isEmpty());
                    FakePlayers.remove(watcher[0]);
                })
                .thenWaitUntil(waits.engine(() -> check("den B's SPAWN is DONE once the watcher went", lines(b.faction, SPAWN, DONE) >= 1)))
                .thenExecute(() -> {
                    List<Entity> h = MowzieKit.held(b.faction);
                    check("a tagged Frostmaw at den B (" + h.size() + ")", h.size() == 1 && h.get(0) instanceof EntityFrostmaw);
                    record(test, "refilled", "den A at the wait's end with no unload (" + (MowzieKit.camps().engineRecounts - before[0])
                            + " RECOUNTs); den B held " + (MowzieKit.camps().heldSpawns - before[3]) + " times, then placed");
                })
                .thenSucceed();
    }
}
