package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.SiteTestHooks;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.sites.SiteLedger;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.monster.Zombie;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.bridge;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * A pool camp under a player's pressure, on the real engine (EQUILIBRIUM.md §2.4.5, slice S7): a goblin camp (a
 * camp whose warriors are a number, no bodies at home) is killed to the last warrior by a test order and given
 * enough pressure that it breaks; its ledger row names the pressure and the wait to its refounding, a plain reset
 * hands both to the new engine, and a broken camp sends nothing however its grievance stands. Each test is its own
 * batch on its own far base; the reset test cleans up the moved pages when it ends. Skipped on the stub, which keeps
 * no camps.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class PoolCampGameTests {
    private PoolCampGameTests() {}

    private static final long DAY = 24_000L;
    /** goblin.json's pool_renewal: pressure halves in 192 days; a break waits min(384, 48 x 2^P) days. */
    private static final double HALF_LIFE_TICKS = 192.0 * DAY;
    /** The goblin culture's camp structure (claimed by the goblin culture; the jar's own is the same culture). */
    private static final String CAMP_STRUCTURE = "mic_rtsim:conflict/goblin_camp";
    /** How far a handed-over wait may be from the old row's less the ticks between their stamps. */
    private static final long TOL = 400;
    private static final int ROW = SiteLedger.Tail.FLAG_ROW, GAP = SiteLedger.Tail.FLAG_GAP, WAITING = SiteLedger.Tail.FLAG_WAITING;

    private static boolean nativeOnly(GameTestHelper helper, String test) {
        if (RtsimTests.engine() instanceof NativeEngine)
            return true;
        record(test, "skipped", "a pool camp is the native engine's only");
        helper.succeed();
        return false;
    }

    private static SiteLedger.Row row(ServerLevel level, long faction) {
        return SiteTestHooks.row(level.getServer(), SiteLedger.CAMP, faction);
    }

    private static String describe(SiteLedger.Row r) {
        if (r == null)
            return "no row";
        return String.format("K %d, count %d, away %d, P %.4f, %s%s, wait left %d, stamp %d", r.k(), r.count(), r.away(), r.p(),
                r.waiting() ? "waiting" : "not waiting", r.broken() ? ", broken" : "", r.waitLeft(), r.stamp());
    }

    private static double aged(float p, long ticks) {
        return p * Math.pow(2.0, -ticks / HALF_LIFE_TICKS);
    }

    // ---- 1: a plain reset ------------------------------------------------------------------------

    /**
     * A reset keeps a pool camp's pressure and its break (S7 test 1). A goblin camp reported, then killed to the last warrior
     * with 1.8 more pressure than those kills gave (P 2.8): it is thin, pressured and idle, so it breaks. Its ledger row is
     * broken and waiting, empty, with P 2.8 and the wait of 48 x 2^2.8 days (334, under the cap of 384). A plain reset (the
     * pages moved aside, a new engine, no gap): the camp's FACTION_FOUND carries the row (ROW and WAITING, no GAP), and the new
     * engine's row is broken still, with P aged by the ticks between the two rows' stamps and the wait left less those ticks.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "pool_reset", skyAccess = true)
    public static void aPoolCampKeepsItsPressureAndItsBreakAcrossAReset(GameTestHelper helper) {
        String test = "aPoolCampKeepsItsPressureAndItsBreakAcrossAReset";
        if (!nativeOnly(helper, test))
            return;
        ServerLevel level = helper.getLevel();
        MinecraftServer server = level.getServer();
        ChunkWait waits = ChunkWait.of(helper, 8000);
        int[] base = ConflictKit.farBase();
        BlockPos camp = new BlockPos(base[0] + 8, 64, base[1] + 8);
        ConflictKit.onEnd(helper, () -> SiteTestHooks.cleanUp(server));
        long[] faction = {0};
        long[] upTo = new long[1], t = new long[1];
        SiteLedger.Row[] broken = new SiteLedger.Row[1], handed = new SiteLedger.Row[1];

        helper.startSequence()
                .thenExecute(() -> {
                    faction[0] = bridge().reportCamp(level, CAMP_STRUCTURE, camp, x -> false, null, false, 0, 0);
                    check("a faction key (a live session)", faction[0] != 0L);
                })
                .thenWaitUntil(waits.engine(() -> check("TERRITORY from the engine", bridge().territories().get(faction[0]) != null)))
                .thenExecute(() -> RtsimTests.slayCamp(faction[0], -1, 1.8f))
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, faction[0]);
                    check("the camp broke (its row is empty, broken and waiting): " + describe(r),
                            r != null && r.broken() && r.waiting() && r.count() == 0);
                    broken[0] = r;
                }))
                .thenExecute(() -> {
                    SiteLedger.Row r = broken[0];
                    check("P is 2.8 (the kills' 1, the order's 1.8): " + describe(r), Math.abs(r.p() - 2.8f) < 0.05f);
                    long expect = (long) (48.0 * Math.pow(2.0, r.p()) * DAY);
                    check("the wait is 48 x 2^P days, the cap not reached: " + describe(r) + " against " + expect,
                            r.waitLeft() <= expect && r.waitLeft() >= expect - 2 * TOL && r.waitLeft() < 384 * DAY);
                    upTo[0] = SiteTestHooks.journalEnd();
                    SiteTestHooks.requestSave();
                })
                .thenWaitUntil(waits.engine(() -> check("an engine save covers the journal through " + upTo[0], SiteTestHooks.savedThrough(upTo[0]))))
                .thenExecute(() -> {
                    t[0] = level.getGameTime();
                    SiteTestHooks.plainReset(server);
                    check("a session runs after the reset: " + RtsimSession.startFailure(), RtsimSession.current() != null);
                    // A reported camp is the session's: the world reports it again, as a chunk load would.
                    check("the camp is reported to the new session", bridge().reportCamp(level, CAMP_STRUCTURE, camp, x -> false, null, false,
                            0, 0) == faction[0]);
                })
                .thenWaitUntil(waits.engine(() -> check("the new engine has the camp's FACTION_FOUND", !ConflictKit.factionsFound(faction[0]).isEmpty())))
                .thenExecute(() -> {
                    int flags = ConflictKit.factionsFound(faction[0]).get(0).view()
                            .getInt(ConflictRecords.FactionFound.TAIL + SiteLedger.Tail.FLAGS);
                    check("its tail carries the row, waiting, with no gap (flags " + flags + ")",
                            (flags & ROW) != 0 && (flags & WAITING) != 0 && (flags & GAP) == 0);
                })
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, faction[0]);
                    check("the new engine's row: " + describe(r), r != null && r.stamp() >= t[0] && r.broken());
                    handed[0] = r;
                }))
                .thenExecute(() -> {
                    SiteLedger.Row a = broken[0], b = handed[0];
                    long elapsed = b.stamp() - a.stamp();
                    double p = aged(a.p(), elapsed);
                    long wait = a.waitLeft() - elapsed;
                    record(test, "handed over", "before: " + describe(a) + "; after " + elapsed + " ticks: " + describe(b)
                            + String.format("; expected P %.4f, wait left %d", p, wait));
                    check(String.format("still pressured: P %.4f is %.4f aged %d ticks, %.4f", b.p(), a.p(), elapsed, p),
                            Math.abs(b.p() - p) < 1e-3);
                    check("broken and waiting the wait left less the ticks since (" + b.waitLeft() + " against " + wait + ")",
                            b.waiting() && b.count() == 0 && Math.abs(b.waitLeft() - wait) <= TOL);
                    check("the new session sent no party for it (" + bridge().marchRecords + " march records)", bridge().marchRecords == 0);
                })
                .thenSucceed();
    }

    // ---- 2: a broken camp sends nothing ------------------------------------------------------------

    /**
     * A broken pool camp sends no party however its grievance stands (S7 test 2). A camp reported and broken (killed to the
     * last warrior, P 2.0 from the order and the kills: a wait of 192 days), a player 40 blocks off kills 32 of its kin: a
     * grievance that sends an envoy within about a day on a camp that is not broken (the ladder test's). The world's clock is
     * moved on thirty days, 1,200 ticks at a time with the player reporting: no BAND_MARCH, no envoy. Then the days to a day
     * short of its refounding go by, the player kills 32 more, and past the refounding a party comes: the new band acts on the
     * grievance it finds.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "pool_broken")
    public static void aBrokenPoolCampSendsNoParty(GameTestHelper helper) {
        String test = "aBrokenPoolCampSendsNoParty";
        if (!(RtsimTests.engine() instanceof NativeEngine e)) {
            record(test, "skipped", "a pool camp is the native engine's only");
            helper.succeed();
            return;
        }
        ServerLevel level = helper.getLevel();
        MinecraftServer server = level.getServer();
        ChunkWait waits = ChunkWait.of(helper, 6000);
        int released = ConflictKit.releaseFarChunks(level);
        int[] base = ConflictKit.farBase();
        int bx = base[0] + 8, bz = base[1] + 8, px = bx + 40;
        String kinTag = "mic_rtsim_test_pool_kin";
        int kills = 32, step = 1_200;
        List<Long> forced = new ArrayList<>();
        for (int cx = (bx - 32) >> 4; cx <= (px + 32) >> 4; cx++)
            for (int cz = (bz - 32) >> 4; cz <= (bz + 32) >> 4; cz++)
                if (ConflictKit.force(level, cx, cz, true))
                    forced.add(ChunkPos.asLong(cx, cz));
        ServerPlayer[] player = new ServerPlayer[1];
        long[] faction = {0};
        long[] mark = new long[3], lastReport = {Long.MIN_VALUE};
        // asks[0] counts polls (an ask every fourth), asks[1] the asks made, asked[0] the ticks they moved the clock on.
        int[] asks = {0, 0};
        long[] asked = {0};
        SiteLedger.Row[] row = new SiteLedger.Row[1];
        // The player reports where it stands every 20 ticks of a wait, as a connected one does.
        Runnable keep = () -> {
            long now = level.getGameTime();
            if (player[0] != null && now - lastReport[0] >= 20) {
                ConflictKit.reportPlayer(player[0]);
                lastReport[0] = now;
            }
        };
        // Every fourth poll: the world's clock moves on a step.
        java.util.function.LongConsumer advance = by -> {
            if (asks[0]++ % 4 == 0) {
                e.phase1Advance(by);
                asks[1]++;
                asked[0] += by;
            }
        };
        Runnable kill = () -> {
            ServerPlayer pl = player[0];
            mark[1] = bridge().signalsCounted;
            for (int i = 0; i < kills; i++) {
                Zombie z = EntityType.ZOMBIE.create(level);
                z.addTag(kinTag);
                z.moveTo(pl.getX() + 1, pl.getY(), pl.getZ() + 1);
                level.addFreshEntity(z);
                z.hurt(level.damageSources().playerAttack(pl), 1000f);
            }
            check("each kill counted (" + (bridge().signalsCounted - mark[1]) + ")", bridge().signalsCounted == mark[1] + kills);
        };
        ConflictKit.onEnd(helper, () -> {
            for (long c : forced)
                ConflictKit.force(level, ChunkPos.getX(c), ChunkPos.getZ(c), false);
            if (player[0] != null)
                FakePlayers.remove(player[0]);
        });

        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> {
                    for (int cx = (bx - 32) >> 4; cx <= (px + 32) >> 4; cx++)
                        for (int cz = (bz - 32) >> 4; cz <= (bz + 32) >> 4; cz++)
                            check("the forced chunks tick entities", RtsimSession.entitiesTickAt(level, new BlockPos(cx * 16 + 8, 64, cz * 16 + 8)));
                }))
                .thenExecute(() -> {
                    // A reused world's clock runs ahead of game time by the last run's asks: one ask of the gap catches it up.
                    mark[2] = e.phase1Stats()[NativeEngine.P1_WORLD_TICK];
                    long gap = mark[2] - level.getGameTime();
                    if (gap > 0)
                        e.phase1Advance(gap);
                })
                .thenWaitUntil(waits.logic(() -> check("the world's clock moves", e.phase1Stats()[NativeEngine.P1_WORLD_TICK] > mark[2])))
                .thenExecute(() -> {
                    int py = level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, px, bz);
                    player[0] = FakePlayers.add(level, new Vec3(px + 0.5, py, bz + 0.5), "pool_broken");
                    player[0].setGameMode(GameType.CREATIVE);
                    BlockPos camp = new BlockPos(bx, level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, bx, bz), bz);
                    faction[0] = bridge().reportCamp(level, CAMP_STRUCTURE, camp, x -> x.getTags().contains(kinTag), null, false, 0, 0);
                    check("a faction key", faction[0] != 0L);
                    record(test, "camp", "(" + bx + ", " + bz + "), the player at (" + px + ", " + bz + "), " + released + " chunks an earlier run left released");
                })
                .thenWaitUntil(waits.logic(() -> {
                    keep.run();
                    check("TERRITORY from the engine", bridge().territories().get(faction[0]) != null);
                }))
                .thenExecute(() -> RtsimTests.slayCamp(faction[0], -1, 1.0f))
                .thenWaitUntil(waits.engine(() -> {
                    keep.run();
                    SiteLedger.Row r = row(level, faction[0]);
                    check("the camp broke: " + describe(r), r != null && r.broken() && r.waiting());
                    row[0] = r;
                }))
                .thenExecute(() -> {
                    check("the wait is 192 days (P 2.0): " + describe(row[0]),
                            Math.abs(row[0].waitLeft() - 192 * DAY) < 4 * TOL && row[0].count() == 0);
                    kill.run();
                })
                .thenWaitUntil(waits.logic(() -> {
                    keep.run();
                    check("the killing reached the engine", !ConflictKit.signals(ConflictRecords.SIGNAL_KILLING, player[0].getUUID()).isEmpty());
                }))
                .thenExecute(() -> {
                    mark[0] = bridge().marchRecords;
                    asks[0] = 0;
                    asks[1] = 0;
                    asked[0] = 0;
                })
                // Thirty days, 1,200 ticks to an ask: a camp that is not broken sends its envoy within 80 of them.
                .thenWaitUntil(waits.logic(() -> {
                    keep.run();
                    check("no party in thirty days (asks " + asks[1] + ")", bridge().marchRecords == mark[0] && partyOf(faction[0]) == null);
                    if (asked[0] >= 30 * DAY)
                        return;
                    advance.accept(step);
                    check("thirty days of asks", false);
                }))
                .thenExecute(() -> {
                    SiteLedger.Row r = row(level, faction[0]);
                    record(test, "after thirty days", describe(r) + "; march records " + bridge().marchRecords + " after " + asks[1] + " asks");
                    check("still broken", r != null && r.broken());
                    check("nothing sent (BAND_MARCH " + bridge().marchRecords + " against " + mark[0] + ")", bridge().marchRecords == mark[0]);
                })
                // On to the last day before the refounding, a day to an ask, the player reporting.
                .thenWaitUntil(waits.logic(() -> {
                    keep.run();
                    check("nothing sent on the way", bridge().marchRecords == mark[0] && partyOf(faction[0]) == null);
                    long left = row[0].waitLeft() - asked[0] - 2 * DAY;
                    if (left <= 0)
                        return;
                    advance.accept(Math.min(DAY, left));
                    check("a day short of the refounding", false);
                }))
                .thenExecute(() -> {
                    check("still nothing sent", bridge().marchRecords == mark[0] && partyOf(faction[0]) == null);
                    kill.run();
                    asks[0] = 0;
                    asks[1] = 0;
                })
                // Past the refounding the new band takes the grievance it finds.
                .thenWaitUntil(waits.logic(() -> {
                    keep.run();
                    if (bridge().marchRecords > mark[0] || partyOf(faction[0]) != null)
                        return;
                    check("a party within 400 asks of " + step + " ticks after the refounding (asks " + asks[1] + ")", asks[1] < 400);
                    advance.accept(step);
                    check("the refounded camp acts", false);
                }))
                .thenExecute(() -> {
                    SiteLedger.Row r = row(level, faction[0]);
                    record(test, "refounded", describe(r) + "; march records " + bridge().marchRecords + " after " + asks[1] + " asks");
                    check("no longer broken: " + describe(r), r != null && !r.broken());
                })
                .thenSucceed();
    }

    /** The faction's embodied party, if any. */
    private static ConflictBridge.Party partyOf(long faction) {
        for (ConflictBridge.Party p : bridge().parties())
            if (p.faction == faction)
                return p;
        return null;
    }
}
