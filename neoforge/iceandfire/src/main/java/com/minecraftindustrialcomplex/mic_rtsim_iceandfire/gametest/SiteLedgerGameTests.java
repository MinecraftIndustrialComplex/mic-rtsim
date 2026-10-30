package com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest;

import com.iafenvoy.iceandfire.entity.DragonBaseEntity;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.SiteTestHooks;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim.sites.SiteLedger;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonRecords;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.MicRtsimIceAndFire;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestSequence;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.animal.Wolf;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.living.AnimalTameEvent;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.List;
import java.util.concurrent.ThreadLocalRandom;

import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.about;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.bridge;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.check;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.record;

/**
 * The site ledger on the real engine for Ice and Fire's lairs (EQUILIBRIUM.md §2.3 and §4's S3 row, slice S3, MZB-1016): a
 * lair found late and empty with no row waits the cap and its own dragon's adoption clears that pressure, a pet's kill and
 * a tame press as a player's, and a reset does not mint a dragon before the wait handed over. Each test is its own batch;
 * native engine only (the stub runs no lair).
 */
@GameTestHolder(MicRtsimIceAndFire.MODID)
@PrefixGameTestTemplate(false)
public final class SiteLedgerGameTests {
    private SiteLedgerGameTests() {}

    private static final long DAY = 24_000L;
    private static final String ROOST = "iceandfire:fire_dragon_roost";
    /** The lairs' renewal (MZB-1012): base 20 days, cap 192, pressure halving in 96. */
    private static final double BASE_DAYS = 20.0, CAP_DAYS = 192.0, HALF_LIFE_TICKS = 96.0 * DAY;
    /** delay(P = 1): 40 days. */
    private static final long W1 = 40 * DAY;
    /** How far a handed-over wait may be from the old row's less the game ticks between their stamps. */
    private static final long TOL = 40;

    // ---- helpers --------------------------------------------------------------------------------

    private static boolean nativeOnly(GameTestHelper helper, String test) {
        if (DragonKit.nativeEngine())
            return true;
        record(test, "skipped", "the lairs are the engine's only on the native engine");
        helper.succeed();
        return false;
    }

    private static long newLair() {
        return 0x0E00_0000_0000_0000L | (ThreadLocalRandom.current().nextLong() & 0xFFFF_FFFF_FFFFL);
    }

    private static SiteLedger.Row row(ServerLevel level, long lair) {
        return SiteTestHooks.row(level.getServer(), SiteLedger.LAIR, lair);
    }

    private static String describe(SiteLedger.Row r) {
        if (r == null)
            return "no row";
        return String.format("K %d, count %d, P %.4f, %s, wait left %d, stamp %d", r.k(), r.count(), r.p(),
                r.waiting() ? "waiting" : "not waiting", r.waitLeft(), r.stamp());
    }

    private static double aged(float p, long ticks) {
        return p * Math.pow(2.0, -ticks / HALF_LIFE_TICKS);
    }

    /** The arena's own box, where its dragon stands. */
    private static BoundingBox arenaBox(GameTestHelper helper) {
        return BoundingBox.fromCorners(helper.absolutePos(BlockPos.ZERO), helper.absolutePos(new BlockPos(10, 5, 10)));
    }

    /** A fire dragon as Ice and Fire's roost piece makes its own (no finalizeSpawn, aging disabled, hunger 50, sitting). */
    private static DragonBaseEntity roostDragon(ServerLevel level, Vec3 at) {
        EntityType<?> fire = BuiltInRegistries.ENTITY_TYPE.get(ResourceLocation.parse(DragonKit.FIRE));
        DragonBaseEntity d = (DragonBaseEntity) fire.create(level);
        check("a fire dragon is created", d != null);
        d.setAgeInTicks(DragonKit.AGE_30_DAYS);
        d.setAgingDisabled(true);
        d.setHunger(50);
        d.setInSittingPose(true);
        d.moveTo(at.x, at.y, at.z, 0f, 0f);
        level.addFreshEntity(d);
        return d;
    }

    /** Inside {@code thenWaitUntil}: the engine adopted the dragon as its lair's occupant and it is embodied. */
    private static Runnable adopted(DragonBaseEntity[] d) {
        return RtsimTests.io(() -> check("the engine adopted the lair's dragon", bridge().agentOf(d[0]) != 0
                && !about(DragonRecords.DRAGON_MATERIALIZED, bridge().agentOf(d[0])).isEmpty()));
    }

    /** Kills a body as {@code source} would; if its own hurt does not, its health goes to 0 and it dies. */
    private static void kill(DragonBaseEntity d, DamageSource source) {
        d.hurt(source, 1_000_000f);
        if (!d.isDeadOrDying()) {
            d.setHealth(0f);
            d.die(source);
        }
    }

    /**
     * Steps: a lair over the arena (not late) and its own dragon, adopted by the engine; the lair's row names one occupant
     * at P 0. The agent goes into {@code agent}.
     */
    private static GameTestSequence occupied(GameTestHelper helper, long lair, DragonBaseEntity[] d, long[] agent) {
        ServerLevel level = helper.getLevel();
        Vec3 c = DragonKit.centre(helper);
        return helper.startSequence()
                .thenExecute(() -> {
                    bridge().lairs.declareForTest(level, lair, ROOST, arenaBox(helper), BlockPos.containing(c), level.getGameTime());
                    d[0] = roostDragon(level, c);
                    // A lair's own dragon is adopted by the lair's census (S5, EQC113), as the lair comes in.
                    bridge().homes.testIn(lair, false, level.getGameTime());
                })
                .thenWaitUntil(adopted(d))
                .thenExecute(() -> agent[0] = bridge().agentOf(d[0]))
                .thenWaitUntil(RtsimTests.io(() -> {
                    SiteLedger.Row r = row(level, lair);
                    check("the lair's row names its occupant: " + describe(r), r != null && r.count() == 1 && r.p() < 1e-3);
                }));
    }

    // ---- 2: a lair found late and empty --------------------------------------------------------------

    /**
     * A site found empty with no ledger row waits the cap (S3 test 2). A lair found LATE with no dragon and no row: its row
     * has P = log2(cap / base) = log2(192 / 20) and waits the cap, 192 days. A second lair found LATE has the same row; its
     * own world-generated dragon then joins and the engine adopts it as the occupant: its P goes back to 0 (MZB-1012). The
     * den form of the rule is MZB-257's test.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 1200, batch = "site_lair_found_empty")
    public static void siteFoundEmptyWithoutLedgerWaitsTheCap(GameTestHelper helper) {
        String test = "siteFoundEmptyWithoutLedgerWaitsTheCap";
        if (!nativeOnly(helper, test))
            return;
        ServerLevel level = helper.getLevel();
        Vec3 c = DragonKit.centre(helper);
        long empty = newLair(), adopting = newLair();
        double atCap = Math.log(CAP_DAYS / BASE_DAYS) / Math.log(2.0);
        long cap = (long) (CAP_DAYS * DAY);
        DragonBaseEntity[] d = new DragonBaseEntity[1];

        helper.startSequence()
                .thenExecute(() -> {
                    // The empty lair stands 300 blocks off: a lair's box decides which lair a joining dragon is of.
                    BoundingBox far = arenaBox(helper).moved(300, 0, 0);
                    bridge().lairs.declareForTest(level, empty, ROOST, far, BlockPos.containing(c).offset(300, 0, 0), level.getGameTime(), true);
                    bridge().lairs.declareForTest(level, adopting, ROOST, arenaBox(helper), BlockPos.containing(c), level.getGameTime(), true);
                })
                .thenWaitUntil(RtsimTests.io(() -> {
                    for (long lair : new long[] {empty, adopting}) {
                        SiteLedger.Row r = row(level, lair);
                        check("a lair found late and empty waits the cap: " + describe(r), r != null && r.count() == 0 && r.waiting());
                    }
                }))
                .thenExecute(() -> {
                    for (long lair : new long[] {empty, adopting}) {
                        SiteLedger.Row r = row(level, lair);
                        record(test, "found late, empty, no row", describe(r) + String.format("; log2(192 / 20) = %.4f", atCap));
                        check(String.format("P is log2(cap / base) (%.4f against %.4f)", r.p(), atCap), Math.abs(r.p() - atCap) < 1e-3);
                        check("the wait is the cap, 192 days (" + r.waitLeft() + ")", r.waitLeft() <= cap && r.waitLeft() >= cap - 400);
                    }
                    d[0] = roostDragon(level, c);
                    bridge().homes.testIn(adopting, false, level.getGameTime());
                })
                .thenWaitUntil(adopted(d))
                .thenWaitUntil(RtsimTests.io(() -> {
                    SiteLedger.Row r = row(level, adopting);
                    check("its own dragon adopted, the lair's P is back to 0: " + describe(r), r != null && r.count() == 1 && r.p() < 1e-3
                            && !r.waiting());
                }))
                .thenExecute(() -> {
                    record(test, "adopted", describe(row(level, adopting)) + "; the empty one: " + describe(row(level, empty)));
                    long agent = bridge().agentOf(d[0]);
                    int epoch = about(DragonRecords.DRAGON_MATERIALIZED, agent).get(0).view().getInt(DragonRecords.Materialized.EPOCH);
                    DragonKit.depart(agent, epoch, 1, 20, c.add(-400, 40, 0));
                })
                .thenSucceed();
    }

    // ---- 5: a pet's kill -----------------------------------------------------------------------

    /**
     * A pet's kill raises the pressure (S3 test 5). A lair's dragon, adopted by the engine, is killed by a wolf a player
     * tamed: DRAGON_DIED is a player's (aux PLAYER) naming the wolf's owner, and the lair's row is waiting at P 1.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 1200, batch = "site_pet_kill")
    public static void petKillRaisesPressure(GameTestHelper helper) {
        String test = "petKillRaisesPressure";
        if (!nativeOnly(helper, test))
            return;
        ServerLevel level = helper.getLevel();
        Vec3 c = DragonKit.centre(helper);
        long lair = newLair();
        DragonBaseEntity[] d = new DragonBaseEntity[1];
        long[] agent = new long[1];
        ServerPlayer[] owner = new ServerPlayer[1];
        Wolf[] wolf = new Wolf[1];

        occupied(helper, lair, d, agent)
                .thenExecute(() -> {
                    owner[0] = FakePlayers.add(level, c.add(4, 0, 4), "site-pet-owner");
                    wolf[0] = EntityType.WOLF.create(level);
                    check("a wolf is created", wolf[0] != null);
                    wolf[0].moveTo(c.x + 2, c.y, c.z + 2, 0f, 0f);
                    level.addFreshEntity(wolf[0]);
                    wolf[0].tame(owner[0]);
                    check("the wolf is the player's", wolf[0].isTame() && owner[0].getUUID().equals(wolf[0].getOwnerUUID()));
                    kill(d[0], level.damageSources().mobAttack(wolf[0]));
                })
                .thenWaitUntil(RtsimTests.io(() -> check("DRAGON_DIED", !about(DragonRecords.DRAGON_DIED, agent[0]).isEmpty())))
                .thenExecute(() -> {
                    ScriptedEngine.Received died = about(DragonRecords.DRAGON_DIED, agent[0]).get(0);
                    check("a player's kill (aux " + died.aux() + ")", died.aux() == DragonRecords.ATTACKER_PLAYER);
                    check("credited to the wolf's owner", owner[0].getUUID().equals(DragonKit.uuidAt(died, DragonRecords.Died.KILLER_MOST)));
                })
                .thenWaitUntil(RtsimTests.io(() -> {
                    SiteLedger.Row r = row(level, lair);
                    check("the lair's row is waiting at P 1: " + describe(r), r != null && r.count() == 0 && r.waiting() && Math.abs(r.p() - 1f) < 1e-3);
                }))
                .thenExecute(() -> {
                    record(test, "row", describe(row(level, lair)));
                    FakePlayers.remove(owner[0]);
                    wolf[0].discard();
                })
                .thenSucceed();
    }

    // ---- 7: a tame -----------------------------------------------------------------------------

    /**
     * A tame is a player's loss (S3 test 7). A lair's dragon, adopted by the engine, is tamed by a player (as Ice and Fire
     * tames, with AnimalTameEvent): DRAGON_DIED aux 4 (taken) names the tamer, and the lair's row is waiting at P 1 for
     * delay(P = 1), 40 days.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 1200, batch = "site_tame")
    public static void tameIsAPlayerLoss(GameTestHelper helper) {
        String test = "tameIsAPlayerLoss";
        if (!nativeOnly(helper, test))
            return;
        ServerLevel level = helper.getLevel();
        Vec3 c = DragonKit.centre(helper);
        long lair = newLair();
        DragonBaseEntity[] d = new DragonBaseEntity[1];
        long[] agent = new long[1];
        ServerPlayer[] tamer = new ServerPlayer[1];

        occupied(helper, lair, d, agent)
                .thenExecute(() -> {
                    tamer[0] = FakePlayers.add(level, c.add(0, 0, 3), "site-tamer");
                    d[0].tame(tamer[0]);
                    NeoForge.EVENT_BUS.post(new AnimalTameEvent(d[0], tamer[0]));
                })
                .thenWaitUntil(RtsimTests.io(() -> check("DRAGON_DIED", !about(DragonRecords.DRAGON_DIED, agent[0]).isEmpty())))
                .thenExecute(() -> {
                    ScriptedEngine.Received died = about(DragonRecords.DRAGON_DIED, agent[0]).get(0);
                    check("taken (aux " + died.aux() + ")", died.aux() == DragonRecords.ATTACKER_TAKEN);
                    check("naming the tamer", tamer[0].getUUID().equals(DragonKit.uuidAt(died, DragonRecords.Died.KILLER_MOST)));
                })
                .thenWaitUntil(RtsimTests.io(() -> {
                    SiteLedger.Row r = row(level, lair);
                    check("the lair's row is waiting at P 1: " + describe(r), r != null && r.count() == 0 && r.waiting() && Math.abs(r.p() - 1f) < 1e-3);
                }))
                .thenExecute(() -> {
                    SiteLedger.Row r = row(level, lair);
                    check("the wait is delay(P = 1), 40 days (" + r.waitLeft() + ")", r.waitLeft() <= W1 && r.waitLeft() >= W1 - 400);
                    record(test, "row", describe(r));
                    FakePlayers.remove(tamer[0]);
                    d[0].discard();
                })
                .thenSucceed();
    }

    // ---- 10: a reset mints no dragon -----------------------------------------------------------

    /**
     * A reset does not mint a dragon (S3 test 10, fork F8). A lair's dragon, adopted, is killed by a player: the row waits
     * delay(P = 1), 40 days. An engine save, a plain reset, and the lair found again by the new engine: its LAIR_FOUND
     * carries the row (no gap) and the new row waits the old wait less the game ticks since (within {@link #TOL}), P 1 aged,
     * far past the 20 days a lair found empty and fresh would wait. Half that wait on, the lair is still waiting with no
     * occupant.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 4000, batch = "site_reset_lair")
    public static void resetDoesNotMintADragon(GameTestHelper helper) {
        String test = "resetDoesNotMintADragon";
        if (!nativeOnly(helper, test))
            return;
        ServerLevel level = helper.getLevel();
        MinecraftServer server = level.getServer();
        DragonKit.onEnd(helper, () -> SiteTestHooks.cleanUp(server));
        Vec3 c = DragonKit.centre(helper);
        long lair = newLair();
        DragonBaseEntity[] d = new DragonBaseEntity[1];
        long[] agent = new long[1], t = new long[2];
        ServerPlayer[] killer = new ServerPlayer[1];
        SiteLedger.Row[] farmed = new SiteLedger.Row[1], handed = new SiteLedger.Row[1];

        occupied(helper, lair, d, agent)
                .thenExecute(() -> {
                    killer[0] = FakePlayers.add(level, c.add(0, 0, 4), "site-dragon-killer");
                    kill(d[0], level.damageSources().playerAttack(killer[0]));
                    FakePlayers.remove(killer[0]);
                })
                .thenWaitUntil(RtsimTests.io(() -> check("DRAGON_DIED", !about(DragonRecords.DRAGON_DIED, agent[0]).isEmpty())))
                .thenWaitUntil(RtsimTests.io(() -> {
                    SiteLedger.Row r = row(level, lair);
                    check("the lair's row is waiting at P 1: " + describe(r), r != null && r.count() == 0 && r.waiting() && Math.abs(r.p() - 1f) < 1e-3);
                    farmed[0] = r;
                }))
                .thenExecute(() -> {
                    check("the wait is delay(P = 1), 40 days (" + farmed[0].waitLeft() + ")", farmed[0].waitLeft() <= W1 && farmed[0].waitLeft() >= W1 - 400);
                    t[0] = SiteTestHooks.journalEnd();
                    SiteTestHooks.requestSave();
                })
                .thenWaitUntil(RtsimTests.io(() -> check("an engine save covers the journal through " + t[0], SiteTestHooks.savedThrough(t[0]))))
                .thenExecute(() -> {
                    t[1] = level.getGameTime();
                    SiteTestHooks.plainReset(server);
                    check("a session runs after the reset: " + RtsimSession.startFailure(), RtsimSession.current() != null);
                    // Lairs are declared again at each session's start (Lairs.clear): this one as a test declares it.
                    bridge().lairs.declareForTest(level, lair, ROOST, arenaBox(helper), BlockPos.containing(c), level.getGameTime());
                })
                .thenWaitUntil(RtsimTests.io(() -> check("the new engine has the lair's LAIR_FOUND",
                        !DragonKit.received(DragonRecords.LAIR_FOUND, DragonRecords.LairFound.LAIR, lair).isEmpty())))
                .thenExecute(() -> {
                    ScriptedEngine.Received found = DragonKit.received(DragonRecords.LAIR_FOUND, DragonRecords.LairFound.LAIR, lair).get(0);
                    int flags = found.view().getInt(DragonRecords.LairFound.TAIL + SiteLedger.Tail.FLAGS);
                    check("its tail carries the row, waiting, with no gap (flags " + flags + ")", (flags & SiteLedger.Tail.FLAG_ROW) != 0
                            && (flags & SiteLedger.Tail.FLAG_WAITING) != 0 && (flags & SiteLedger.Tail.FLAG_GAP) == 0);
                })
                .thenWaitUntil(RtsimTests.io(() -> {
                    SiteLedger.Row r = row(level, lair);
                    check("the new engine's row: " + describe(r), r != null && r.stamp() >= t[1]);
                    handed[0] = r;
                }))
                .thenExecute(() -> {
                    SiteLedger.Row a = farmed[0], b = handed[0];
                    long elapsed = b.stamp() - a.stamp();
                    double p = aged(a.p(), elapsed);
                    long wait = a.waitLeft() - elapsed;
                    record(test, "handed over", "before: " + describe(a) + "; after " + elapsed + " ticks: " + describe(b)
                            + String.format("; expected P %.4f, wait left %d", p, wait));
                    check(String.format("P %.4f is 1 aged %d ticks, %.4f", b.p(), elapsed, p), Math.abs(b.p() - p) < 1e-3);
                    check("waiting the wait left less the ticks since (" + b.waitLeft() + " against " + wait + ")",
                            b.waiting() && b.count() == 0 && Math.abs(b.waitLeft() - wait) <= TOL);
                    check("far past a fresh lair's 20 days", b.waitLeft() > (long) (BASE_DAYS * DAY));
                    ((NativeEngine) RtsimTests.engine()).phase1Advance(b.waitLeft() / 2);
                })
                .thenIdle(100)
                .thenExecute(() -> {
                    SiteLedger.Row r = row(level, lair);
                    check("half the wait on, still waiting with no occupant: " + describe(r), r != null && r.waiting() && r.count() == 0);
                    List<ScriptedEngine.Received> found = DragonKit.received(DragonRecords.LAIR_FOUND, DragonRecords.LairFound.LAIR, lair);
                    record(test, "half way", describe(r) + "; LAIR_FOUND " + found.size());
                })
                .thenSucceed();
    }
}
