package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.growth.ResidentSpawner;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.village.Census;
import com.minecraftindustrialcomplex.mic_rtsim.village.Village;
import com.minecraftindustrialcomplex.mic_rtsim.village.VillageRegistry;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.monster.Zombie;
import net.minecraft.world.entity.npc.Villager;
import net.minecraft.world.entity.npc.VillagerProfession;
import net.minecraft.world.entity.npc.VillagerType;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.BedBlock;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.properties.BedPart;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ThreadLocalRandom;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * Villages renew (EQUILIBRIUM.md §2.4.3, slice S6, docs/overnight-choices-equilibrium.md EQC150 to EQC183): what the census
 * tells the engine and when it is believed, what a player's kill and a mob's kill do to the wait for settlers, and the rules
 * an arrival follows (never more than the free beds, never into a watched village, walking in from the edge when a player
 * watches, put at the village when nobody does).
 *
 * <p>Every village here stands on the ground at a random far place, its reach (the box and 48 blocks round it) held by
 * forced chunks so the census reads it complete, with beds placed as blocks (the points of interest the census counts) and
 * villagers made no-AI so they stay where they stand. The engine-driven tests take the native engine and read what the
 * engine believes of the village with {@code phase1Village}; days pass with {@code P1_ADVANCE}. The arrival tests order the
 * arrivals themselves (a scripted village, so the engine decides nothing about it) and read the answers.
 *
 * <p>UNVERIFIED on a running server until the core round has run them: the first build of these tests.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class VillageRenewalGameTests {
    private VillageRenewalGameTests() {}

    private static final int DAY = 24_000;
    private static final String PLAINS = "minecraft:village_plains";
    private static final int HALF = 8;

    // ---- the village the tests stand up ---------------------------------------------------------------------

    /**
     * {@link RtsimTests#check}, and a failing check sleeps 5 ms: the GameTest server ticks as fast as it can, and the chunks a
     * wait is for load on other threads in wall time, so a timeout in ticks must not pass in too short a time (see
     * {@link RtsimTests#io}). A check that fails in a {@code thenExecute} fails the test either way.
     */
    private static void check(String what, boolean ok) {
        if (!ok) {
            try {
                Thread.sleep(5);
            } catch (InterruptedException ie) {
                Thread.currentThread().interrupt();
            }
        }
        RtsimTests.check(what, ok);
    }

    /** A village on the ground at a far place, its reach loaded, with beds. */
    private static final class Site {
        final ServerLevel level;
        final Village village;
        final BlockPos meet;
        final BoundingBox box;
        final List<ChunkPos> forced = new ArrayList<>();
        final List<Villager> villagers = new ArrayList<>();

        Site(GameTestHelper helper, boolean live, int beds) {
            level = helper.getLevel();
            int x = 2_000_000 + ThreadLocalRandom.current().nextInt(0, 1_000) * 2_000;
            int z = -2_000_000 - ThreadLocalRandom.current().nextInt(0, 1_000) * 2_000;
            force(x >> 4, z >> 4);
            level.getChunk(x >> 4, z >> 4);
            int y = level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, x, z);
            meet = new BlockPos(x, y, z);
            box = new BoundingBox(x - HALF, y - 2, z - HALF, x + HALF, y + 6, z + HALF);
            int reach = Census.CENSUS_REACH + HALF;
            for (int cx = (x - reach) >> 4; cx <= (x + reach) >> 4; cx++)
                for (int cz = (z - reach) >> 4; cz <= (z + reach) >> 4; cz++)
                    force(cx, cz);
            village = live
                    ? RtsimTests.adoptArenaLive(helper, "human", PLAINS, meet, HALF, 90)
                    : RtsimTests.adoptScripted(helper, "human", box, meet);
            // Beds need their chunk: the middle one is loaded above.
            for (int i = 0; i < beds; i++)
                bed(meet.offset(-HALF + 1 + i, 0, 3));
        }

        void force(int cx, int cz) {
            ChunkPos p = new ChunkPos(cx, cz);
            if (!forced.contains(p)) {
                level.setChunkForced(cx, cz, true);
                forced.add(p);
            }
        }

        /** Forces the chunks round the points {@code distances} blocks from the meeting point toward (x, z). */
        void forceRay(double toX, double toZ, int[] distances) {
            double dx = toX - meet.getX(), dz = toZ - meet.getZ();
            double len = Math.max(1e-6, Math.hypot(dx, dz));
            for (int d : distances) {
                int px = (int) Math.floor(meet.getX() + dx / len * d), pz = (int) Math.floor(meet.getZ() + dz / len * d);
                for (int cx = (px >> 4) - 1; cx <= (px >> 4) + 1; cx++)
                    for (int cz = (pz >> 4) - 1; cz <= (pz >> 4) + 1; cz++)
                        force(cx, cz);
            }
        }

        void bed(BlockPos foot) {
            BlockPos head = foot.relative(Direction.SOUTH);
            level.setBlock(foot, Blocks.RED_BED.defaultBlockState().setValue(BedBlock.FACING, Direction.SOUTH)
                    .setValue(BedBlock.PART, BedPart.FOOT), 3);
            level.setBlock(head, Blocks.RED_BED.defaultBlockState().setValue(BedBlock.FACING, Direction.SOUTH)
                    .setValue(BedBlock.PART, BedPart.HEAD), 3);
        }

        /** {@code n} villagers that stand where they are put, inside the box. */
        void villagers(int n) {
            for (int i = 0; i < n; i++) {
                Villager v = EntityType.VILLAGER.create(level);
                check("a villager was made", v != null);
                v.moveTo(meet.getX() + 0.5 - 4 + (i % 8), meet.getY(), meet.getZ() + 0.5 - 4 + (i / 8) * 2, 0, 0);
                v.setNoAi(true);
                v.setPersistenceRequired();
                check("a villager was added", level.addFreshEntity(v));
                villagers.add(v);
            }
        }

        /** The villagers standing in the box now, the ones the test made or an arrival placed. */
        List<Villager> inBox() {
            return level.getEntitiesOfClass(Villager.class, AABB.of(box).inflate(1), Entity::isAlive);
        }

        /** Whether the census reads the whole reach. */
        boolean complete() {
            return Census.scan(RtsimTests.session().phase1.villages, level, village).complete();
        }

        void close() {
            for (Villager v : level.getEntitiesOfClass(Villager.class, AABB.of(box).inflate(Census.CENSUS_REACH)))
                v.discard();
            for (ChunkPos p : forced)
                level.setChunkForced(p.x, p.z, false);
            RtsimTests.forget(village);
        }

        long[] view(NativeEngine e) {
            return e.phase1Village(village.id);
        }
    }

    /**
     * What holds a walk-in back, for a failure message: each of the village's orders (when it tries again, where it comes
     * from) and, along the ray toward the first one's source, which chunks are loaded, entity-ticking and standable.
     */
    private static String why(Site s, VillageRegistry villages) {
        StringBuilder b = new StringBuilder();
        long now = RtsimTests.session().gameTime();
        double[] src = null;
        for (VillageRegistry.ResidentOrder o : villages.residentOrders.values())
            if (o.village == s.village.id) {
                b.append(" [order retries in ").append(o.nextTry - now).append(", source ").append((long) o.x).append(',')
                        .append((long) o.z).append("]");
                if (src == null)
                    src = new double[] {o.x, o.z};
            }
        String held = b.length() == 0 ? "gone" : "held";
        b.append(" [orders ").append(held).append(", walkers ").append(villages.walkers.size())
                .append(" placed ").append(RtsimTests.session().phase1.walkers.placed()).append(" arrived ")
                .append(RtsimTests.session().phase1.walkers.arrived()).append("]");
        b.append(" [bell ticking ").append(s.level.isPositionEntityTicking(s.village.meeting)).append(", watched ")
                .append(com.minecraftindustrialcomplex.mic_rtsim.api.Arrivals.watched(s.level, s.village.box,
                        com.minecraftindustrialcomplex.mic_rtsim.api.Arrivals.WATCH_BLOCKS)).append("]");
        if (src != null) {
            double dx = src[0] - s.meet.getX(), dz = src[1] - s.meet.getZ(), len = Math.max(1e-6, Math.hypot(dx, dz));
            for (int d : ResidentSpawner.WALK_DISTANCES) {
                int x = (int) Math.floor(s.meet.getX() + dx / len * d), z = (int) Math.floor(s.meet.getZ() + dz / len * d);
                b.append(" [").append(d).append(": chunk ").append(s.level.getChunkSource().getChunkNow(x >> 4, z >> 4) != null)
                        .append(" ticking ").append(s.level.isPositionEntityTicking(new BlockPos(x, s.meet.getY(), z)))
                        .append(" entities ").append(s.level.areEntitiesLoaded(ChunkPos.asLong(x >> 4, z >> 4)))
                        .append(" standable ")
                        .append(com.minecraftindustrialcomplex.mic_rtsim.api.Arrivals.standableSurface(s.level, x, z) != null)
                        .append("]");
            }
        }
        return b.toString();
    }

    private static NativeEngine nativeEngine(String test) {
        if (RtsimTests.engine() instanceof NativeEngine e)
            return e;
        record(test, "skipped", "the stub engine believes nothing; this test needs the native engine's phase 1 world");
        return null;
    }

    private static long now(NativeEngine e) {
        return e.phase1Stats()[NativeEngine.P1_WORLD_TICK];
    }

    private static long engineOp() {
        return ThreadLocalRandom.current().nextLong() & Long.MAX_VALUE;
    }

    private static ScriptedEngine.Received result(ScriptedEngine stub, long op) {
        List<ScriptedEngine.Received> rs = RtsimTests.recordsWith(stub, Obs.COMMAND_RESULT, Obs.CommandResult.OP_ID, op);
        return rs.isEmpty() ? null : rs.get(rs.size() - 1);
    }

    private static long[] ready(NativeEngine e, Site s, long members, long k) {
        long[] v = s.view(e);
        check("the engine knows the village", v != null);
        check("it counts " + members + " at K " + k + " and has seen them alive: " + java.util.Arrays.toString(v),
                v[NativeEngine.VILLAGE_MEMBERS] == members && v[NativeEngine.VILLAGE_K] == k && v[NativeEngine.VILLAGE_SEEN] == 1);
        return v;
    }

    // ---- EQC150 to EQC155: the census both ways -------------------------------------------------------------

    /**
     * A complete census (every chunk of the reach FULL with its entities, every walker found) is believed in both
     * directions, after the second equal reading when it is lower; an incomplete one, whatever it counts, never lowers the
     * engine's count (EQC150 to EQC152). Eight villagers are counted, five are removed and the count follows to three; the
     * outer chunks are let go and a sixth is removed: the census still runs, now incomplete, and the count stays three.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 4000, batch = "mic_rtsim_renewal_census")
    public static void aCompleteCensusLowersTheCountAndAnIncompleteOneDoesNot(GameTestHelper helper) {
        NativeEngine e = nativeEngine("renewalCensus");
        if (e == null) {
            helper.succeed();
            return;
        }
        Site s = new Site(helper, true, 8);
        s.villagers(8);
        long[] taken = {0};
        helper.startSequence()
                .thenWaitUntil(() -> {
                    check("the reach is loaded", s.complete());
                    ready(e, s, 8, 8);
                })
                .thenExecute(() -> {
                    for (int i = 0; i < 5; i++)
                        s.villagers.get(i).discard();
                })
                .thenWaitUntil(() -> check("two complete readings lower it to three: " + java.util.Arrays.toString(s.view(e)),
                        s.view(e)[NativeEngine.VILLAGE_MEMBERS] == 3))
                .thenExecute(() -> {
                    // Let go of everything past the box's own chunks: the reach is no longer loaded.
                    ChunkPos mid = new ChunkPos(s.meet);
                    for (ChunkPos p : new ArrayList<>(s.forced))
                        if (Math.abs(p.x - mid.x) > 1 || Math.abs(p.z - mid.z) > 1) {
                            s.level.setChunkForced(p.x, p.z, false);
                            s.forced.remove(p);
                        }
                })
                .thenWaitUntil(() -> check("the reach is no longer read as loaded", !s.complete()))
                .thenExecute(() -> {
                    s.villagers.get(5).discard();
                    taken[0] = RtsimTests.session().phase1.census.taken();
                })
                .thenWaitUntil(() -> check("the census ran incomplete at least four times",
                        RtsimTests.session().phase1.census.taken() >= taken[0] + 4))
                .thenExecute(() -> {
                    long[] v = s.view(e);
                    record("renewalCensus", "view after the incomplete readings", java.util.Arrays.toString(v));
                    check("an incomplete census never lowers the count", v[NativeEngine.VILLAGE_MEMBERS] == 3);
                    s.close();
                })
                .thenSucceed();
    }

    /**
     * A village the engine has never seen alive is never settled (EQC153, Ben's answer to FB-1): an adopted village with no
     * villagers at all, counted empty by a complete census, 40 days on: no wait runs and no settlers are booked.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 4000, batch = "mic_rtsim_renewal_never_seen")
    public static void aVillageNeverSeenAliveIsNotSettled(GameTestHelper helper) {
        NativeEngine e = nativeEngine("renewalNeverSeen");
        if (e == null) {
            helper.succeed();
            return;
        }
        Site s = new Site(helper, true, 8);
        long[] target = {0};
        helper.startSequence()
                .thenWaitUntil(() -> {
                    check("the reach is loaded", s.complete());
                    long[] v = s.view(e);
                    check("the engine has read it empty, complete: " + java.util.Arrays.toString(v),
                            v != null && v[NativeEngine.VILLAGE_SEEN_READING] == 0);
                })
                .thenExecute(() -> {
                    target[0] = now(e) + 40L * DAY;
                    e.phase1Advance(40L * DAY);
                })
                .thenWaitUntil(() -> check("the world ran 40 days on", now(e) >= target[0]))
                .thenExecute(() -> {
                    long[] v = s.view(e);
                    record("renewalNeverSeen", "view after 40 days", java.util.Arrays.toString(v));
                    check("never seen alive", v[NativeEngine.VILLAGE_SEEN] == 0);
                    check("no wait runs", v[NativeEngine.VILLAGE_WAIT_DUE] == -1);
                    check("nobody was booked", v[NativeEngine.VILLAGE_MEMBERS] == 0 && v[NativeEngine.VILLAGE_OWED] == 0);
                    s.close();
                })
                .thenSucceed();
    }

    // ---- EQC156 to EQC160: kills, pressure and the wait -------------------------------------------------------

    /** The wait for settlers a village's last twelve, killed by {@code killer}, leaves: days left, from the world's tick. */
    private static double waitDays(NativeEngine e, Site s) {
        return (s.view(e)[NativeEngine.VILLAGE_WAIT_DUE] - now(e)) / (double) DAY;
    }

    /**
     * Twelve villagers at K 12 killed by a player: pressure 1.00, so a wait of 32 days from the census that finds the village
     * empty (EQC158, EQC159). The settlers' order is not out a day before it and is out a day after (the count rises to two
     * at the order; Java answers it in the tick it places the pair).
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 6000, batch = "mic_rtsim_renewal_kill")
    public static void aPlayerKillAddsPressureAndTheWaitFollowsIt(GameTestHelper helper) {
        NativeEngine e = nativeEngine("renewalKill");
        if (e == null) {
            helper.succeed();
            return;
        }
        ServerLevel level = helper.getLevel();
        Site s = new Site(helper, true, 12);
        s.villagers(12);
        ServerPlayer killer = FakePlayers.add(level, RtsimTests.centre(helper), "renewal_killer");
        long[] due = {0};
        helper.startSequence()
                .thenWaitUntil(() -> {
                    check("the reach is loaded", s.complete());
                    ready(e, s, 12, 12);
                })
                .thenExecute(() -> {
                    for (Villager v : s.villagers)
                        v.hurt(level.damageSources().playerAttack(killer), 1000f);
                })
                .thenWaitUntil(() -> {
                    long[] v = s.view(e);
                    check("the census reads it empty and a wait runs: " + java.util.Arrays.toString(v),
                            v[NativeEngine.VILLAGE_MEMBERS] == 0 && v[NativeEngine.VILLAGE_WAIT_DUE] >= 0);
                })
                .thenExecute(() -> {
                    long[] v = s.view(e);
                    record("renewalKill", "view when the wait started", java.util.Arrays.toString(v));
                    check("pressure 1.00 for twelve kills at K 12: " + v[NativeEngine.VILLAGE_PRESSURE_MILLI],
                            Math.abs(v[NativeEngine.VILLAGE_PRESSURE_MILLI] - 1000) <= 60);
                    double days = waitDays(e, s);
                    check("a wait of 32 days, not 16: " + days, days > 31.0 && days <= 32.0);
                    due[0] = v[NativeEngine.VILLAGE_WAIT_DUE];
                    e.phase1Advance(due[0] - now(e) - DAY);
                })
                .thenWaitUntil(() -> check("a day before the wait ends", now(e) >= due[0] - DAY))
                .thenExecute(() -> {
                    long[] v = s.view(e);
                    check("no settlers at day 31: " + java.util.Arrays.toString(v),
                            v[NativeEngine.VILLAGE_MEMBERS] == 0 && v[NativeEngine.VILLAGE_OWED] == 0);
                    e.phase1Advance(2L * DAY);
                })
                .thenWaitUntil(() -> {
                    long[] v = s.view(e);
                    check("the pair is booked at day 32: " + java.util.Arrays.toString(v), v[NativeEngine.VILLAGE_MEMBERS] == 2);
                })
                .thenExecute(() -> {
                    FakePlayers.remove(killer);
                    s.close();
                })
                .thenSucceed();
    }

    /**
     * Twelve villagers killed by a zombie (no player behind it): no pressure, the base wait of 16 days (EQC156, EQC158).
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 6000, batch = "mic_rtsim_renewal_mob")
    public static void aMobKillAddsNoPressure(GameTestHelper helper) {
        NativeEngine e = nativeEngine("renewalMob");
        if (e == null) {
            helper.succeed();
            return;
        }
        ServerLevel level = helper.getLevel();
        Site s = new Site(helper, true, 12);
        s.villagers(12);
        Zombie zombie = EntityType.ZOMBIE.create(level);
        check("a zombie was made", zombie != null);
        zombie.moveTo(s.meet.getX() + 6.5, s.meet.getY(), s.meet.getZ() + 6.5, 0, 0);
        zombie.setNoAi(true);
        check("the zombie was added", level.addFreshEntity(zombie));
        helper.startSequence()
                .thenWaitUntil(() -> {
                    check("the reach is loaded", s.complete());
                    ready(e, s, 12, 12);
                })
                .thenExecute(() -> {
                    for (Villager v : s.villagers)
                        v.hurt(level.damageSources().mobAttack(zombie), 1000f);
                })
                .thenWaitUntil(() -> {
                    long[] v = s.view(e);
                    check("the census reads it empty and a wait runs: " + java.util.Arrays.toString(v),
                            v[NativeEngine.VILLAGE_MEMBERS] == 0 && v[NativeEngine.VILLAGE_WAIT_DUE] >= 0);
                })
                .thenExecute(() -> {
                    long[] v = s.view(e);
                    record("renewalMob", "view when the wait started", java.util.Arrays.toString(v));
                    check("a mob's kills add no pressure: " + v[NativeEngine.VILLAGE_PRESSURE_MILLI],
                            v[NativeEngine.VILLAGE_PRESSURE_MILLI] == 0);
                    double days = waitDays(e, s);
                    check("the base wait of 16 days: " + days, days > 15.0 && days <= 16.0);
                    zombie.discard();
                    s.close();
                })
                .thenSucceed();
    }

    // ---- EQC174 to EQC179: arrivals ---------------------------------------------------------------------------

    private static final int AT_VILLAGE_ARRIVAL = Cmd.SpawnResident.FLAG_AT_VILLAGE | Cmd.SpawnResident.FLAG_ARRIVAL;
    private static final int WALK_IN = AT_VILLAGE_ARRIVAL | Cmd.SpawnResident.FLAG_WALK_IN;

    private static void arrival(Site s, long op, int flags, double fromX, double fromZ) {
        RtsimTests.engine().spawnResident(op, s.village.id, s.level.dimension().location().toString(), "minecraft:villager", null, flags,
                fromX, 0, fromZ, 0);
    }

    /**
     * A human arrival never takes more than the free beds (EQC179): four beds, two villagers, three orders for a place: one
     * is placed, the others are refused NOT_FREE, and what stands is the beds (vanilla breeds only into free beds).
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 3000, batch = "mic_rtsim_renewal_beds")
    public static void arrivalsNeverFillMoreBedsThanThereAre(GameTestHelper helper) {
        ScriptedEngine stub = RtsimTests.engine();
        Site s = new Site(helper, false, 3);
        s.villagers(2);
        long[] ops = {engineOp(), engineOp(), engineOp()};
        helper.startSequence()
                .thenWaitUntil(() -> check("the reach is loaded", s.complete()))
                .thenExecute(() -> {
                    for (long op : ops)
                        arrival(s, op, AT_VILLAGE_ARRIVAL, 0, 0);
                })
                .thenWaitUntil(() -> {
                    for (long op : ops)
                        check("order " + op + " answered", result(stub, op) != null);
                })
                .thenExecute(() -> {
                    int placed = 0, full = 0;
                    for (long op : ops) {
                        int status = result(stub, op).aux();
                        if (status == Obs.CommandResult.APPLIED)
                            placed++;
                        else if (status == Obs.CommandResult.NOT_FREE)
                            full++;
                    }
                    record("renewalBeds", "placed, refused", placed + ", " + full);
                    check("one placed and two refused NOT_FREE: " + placed + ", " + full, placed == 1 && full == 2);
                    check("three villagers stand for three beds: " + s.inBox().size(), s.inBox().size() == 3);
                    s.close();
                })
                .thenSucceed();
    }

    /**
     * Settlers from the explored edge, end to end (EQC177, EQC178; the row's): a village of twelve whose villagers are all gone,
     * a player standing in it. When the wait is over the pair is ordered to walk in, the player is within 128 blocks so
     * nothing is placed in the village, and each spawns 96 to 176 blocks out and at least 128 from the player, walks to
     * within 12 blocks of the meeting point inside the timeout, is an ordinary vanilla villager, and the engine's booking
     * for them is answered (owed 0).
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 14000, batch = "mic_rtsim_renewal_walk")
    public static void settlersWalkInFromTheExploredEdge(GameTestHelper helper) {
        NativeEngine e = nativeEngine("renewalWalk");
        if (e == null) {
            helper.succeed();
            return;
        }
        ServerLevel level = helper.getLevel();
        Site s = new Site(helper, true, 12);
        s.villagers(12);
        VillageRegistry villages = RtsimTests.session().phase1.villages;
        ServerPlayer player = FakePlayers.add(level, new Vec3(s.meet.getX() + 0.5, s.meet.getY(), s.meet.getZ() + 12.5),
                "renewal_watcher");
        long[] arrived0 = {0};
        helper.startSequence()
                .thenWaitUntil(() -> {
                    check("the reach is loaded", s.complete());
                    ready(e, s, 12, 12);
                })
                .thenExecute(() -> {
                    record("renewalWalk", "test tick when the census was ready", helper.getTick());
                    for (Villager v : s.villagers)
                        v.discard();
                })
                .thenWaitUntil(() -> {
                    long[] v = s.view(e);
                    check("a wait runs", v[NativeEngine.VILLAGE_MEMBERS] == 0 && v[NativeEngine.VILLAGE_WAIT_DUE] >= 0);
                })
                .thenExecute(() -> {
                    record("renewalWalk", "test tick when the wait was running", helper.getTick());
                    arrived0[0] = RtsimTests.session().phase1.walkers.arrived();
                    e.phase1Advance(s.view(e)[NativeEngine.VILLAGE_WAIT_DUE] - now(e) + DAY / 2);
                })
                .thenWaitUntil(() -> {
                    int mine = 0;
                    for (VillageRegistry.ResidentOrder o : villages.residentOrders.values())
                        if (o.village == s.village.id)
                            mine++;
                    check("the pair's orders wait in the registry, the village being watched: " + mine, mine == 2);
                })
                .thenExecute(() -> {
                    record("renewalWalk", "test tick when the orders were in", helper.getTick());
                    check("nobody is placed in a watched village: " + s.inBox().size(), s.inBox().isEmpty());
                    // The source the engine named: let the chunks along that ray load.
                    for (VillageRegistry.ResidentOrder o : villages.residentOrders.values())
                        if (o.village == s.village.id) {
                            record("renewalWalk", "the order's source", (long) o.x + ", " + (long) o.z + " from the bell " + s.meet
                                    + " flags " + o.flags + " body " + o.body);
                            s.forceRay(o.x, o.z, ResidentSpawner.WALK_DISTANCES);
                            break;
                        }
                })
                .thenWaitUntil(() -> {
                    int walking = 0;
                    for (VillageRegistry.Walker w : villages.walkers.values())
                        if (w.village == s.village.id)
                            walking++;
                    check("both settlers stand on the road, walking: " + walking + why(s, villages), walking == 2);
                })
                .thenExecute(() -> {
                    for (VillageRegistry.Walker w : villages.walkers.values()) {
                        if (w.village != s.village.id)
                            continue;
                        Entity settler = level.getEntity(w.id);
                        check("the settler is in the level", settler != null);
                        double out = Math.hypot(settler.getX() - (s.meet.getX() + 0.5), settler.getZ() - (s.meet.getZ() + 0.5));
                        record("renewalWalk", "spawned this far out", out);
                        check("96 to 176 blocks out: " + out, out >= 94 && out <= 178);
                        check("at least 128 blocks from the player: " + settler.distanceTo(player), settler.distanceTo(player) >= 127);
                        check("an ordinary vanilla villager", settler instanceof Villager && !RtsimAttachments.isTagged(settler));
                    }
                })
                .thenWaitUntil(() -> {
                    check("both walked to within 12 blocks of the bell, not put there",
                            RtsimTests.session().phase1.walkers.arrived() >= arrived0[0] + 2);
                })
                .thenWaitUntil(() -> {
                    long[] v = s.view(e);
                    check("the engine's booking is answered: " + java.util.Arrays.toString(v),
                            v[NativeEngine.VILLAGE_MEMBERS] == 2 && v[NativeEngine.VILLAGE_OWED] == 0);
                })
                .thenExecute(() -> {
                    FakePlayers.remove(player);
                    s.close();
                })
                .thenSucceed();
    }

    /**
     * Never into a watched village (EQC174; the row's): a growth order and a settlers' order, a player within 128 blocks of
     * the footprint. Nothing is placed in the village and neither order is answered; when he leaves 200 blocks the growth order
     * places at the bell, and so does the settlers' (nobody is left to watch the walk).
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 3000, batch = "mic_rtsim_renewal_watched")
    public static void noArrivalIntoAWatchedVillage(GameTestHelper helper) {
        ScriptedEngine stub = RtsimTests.engine();
        ServerLevel level = helper.getLevel();
        Site s = new Site(helper, false, 4);
        ServerPlayer player = FakePlayers.add(level, new Vec3(s.meet.getX() + 60.5, s.meet.getY(), s.meet.getZ() + 0.5),
                "renewal_near");
        long growth = engineOp(), settlers = engineOp();
        helper.startSequence()
                .thenWaitUntil(() -> check("the reach is loaded", s.complete()))
                .thenExecute(() -> {
                    arrival(s, growth, AT_VILLAGE_ARRIVAL, 0, 0);
                    arrival(s, settlers, WALK_IN, s.meet.getX() + 192, s.meet.getZ());
                })
                .thenIdle(200)
                .thenExecute(() -> {
                    check("no villager in a watched village: " + s.inBox().size(), s.inBox().isEmpty());
                    check("the growth order waits", result(stub, growth) == null);
                    check("and so does the walk-in, no spot out there being loaded", result(stub, settlers) == null);
                    player.setPos(s.meet.getX() - 200.5, s.meet.getY(), s.meet.getZ() + 0.5);
                })
                .thenWaitUntil(() -> {
                    check("both orders answered once nobody watches", result(stub, growth) != null && result(stub, settlers) != null);
                })
                .thenExecute(() -> {
                    check("the growth order was placed", result(stub, growth).aux() == Obs.CommandResult.APPLIED);
                    check("and the settlers", result(stub, settlers).aux() == Obs.CommandResult.APPLIED);
                    check("two villagers at the bell: " + s.inBox().size(), s.inBox().size() == 2);
                    for (Villager v : s.inBox())
                        check("within 8 blocks of the bell: " + v.position(), Math.hypot(v.getX() - s.meet.getX() - 0.5,
                                v.getZ() - s.meet.getZ() - 0.5) <= 8);
                    FakePlayers.remove(player);
                    s.close();
                })
                .thenSucceed();
    }

    /**
     * An unwatched village gets its settlers at the meeting point at once, whatever the walk-in flag says (EQC177): no player
     * anywhere near, the order answered APPLIED and the villager standing by the bell.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 3000, batch = "mic_rtsim_renewal_unwatched")
    public static void anUnwatchedVillageGetsItsSettlersAtTheMeetingPoint(GameTestHelper helper) {
        ScriptedEngine stub = RtsimTests.engine();
        Site s = new Site(helper, false, 4);
        long op = engineOp();
        helper.startSequence()
                .thenWaitUntil(() -> check("the reach is loaded", s.complete()))
                .thenExecute(() -> arrival(s, op, WALK_IN, s.meet.getX() + 192, s.meet.getZ()))
                .thenWaitUntil(() -> check("answered", result(stub, op) != null))
                .thenExecute(() -> {
                    check("placed: status " + result(stub, op).aux(), result(stub, op).aux() == Obs.CommandResult.APPLIED);
                    List<Villager> in = s.inBox();
                    check("one villager in the village: " + in.size(), in.size() == 1);
                    check("by the bell: " + in.get(0).position(), Math.hypot(in.get(0).getX() - s.meet.getX() - 0.5,
                            in.get(0).getZ() - s.meet.getZ() - 0.5) <= 8);
                    s.close();
                })
                .thenSucceed();
    }

    /**
     * A walker that loses its watcher is placed at the village (EQC178): a player watches, the settler spawns out on the road
     * and starts to walk; the player leaves 300 blocks, and within a steering interval or two the settler stands by the bell
     * and the registry has forgotten it.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 4000, batch = "mic_rtsim_renewal_lost_watcher")
    public static void aWalkerThatLosesItsWatcherIsPlacedAtTheVillage(GameTestHelper helper) {
        ScriptedEngine stub = RtsimTests.engine();
        ServerLevel level = helper.getLevel();
        Site s = new Site(helper, false, 4);
        VillageRegistry villages = RtsimTests.session().phase1.villages;
        ServerPlayer player = FakePlayers.add(level, new Vec3(s.meet.getX() + 0.5, s.meet.getY(), s.meet.getZ() + 12.5),
                "renewal_leaver");
        long op = engineOp();
        long[] placed0 = {0};
        s.forceRay(s.meet.getX() + 192, s.meet.getZ(), ResidentSpawner.WALK_DISTANCES);
        helper.startSequence()
                .thenWaitUntil(() -> check("the reach is loaded", s.complete()))
                .thenExecute(() -> arrival(s, op, WALK_IN, s.meet.getX() + 192, s.meet.getZ()))
                .thenWaitUntil(() -> check("the settler walks in: status " + (result(stub, op) == null ? "none" : result(stub, op).aux()),
                        result(stub, op) != null && villages.walkers.size() >= 1))
                .thenExecute(() -> {
                    check("placed at the edge, not in the village: " + s.inBox().size(), s.inBox().isEmpty());
                    placed0[0] = RtsimTests.session().phase1.walkers.placed();
                    player.setPos(s.meet.getX() - 300.5, s.meet.getY(), s.meet.getZ() + 0.5);
                })
                .thenWaitUntil(() -> check("put at the village once nobody could see it",
                        RtsimTests.session().phase1.walkers.placed() >= placed0[0] + 1 || !s.inBox().isEmpty()))
                .thenExecute(() -> {
                    List<Villager> in = s.inBox();
                    check("one villager by the bell: " + in.size(), in.size() == 1);
                    check("the registry forgot it: " + villages.walkers.size(), villages.walkers.isEmpty());
                    FakePlayers.remove(player);
                    s.close();
                })
                .thenSucceed();
    }

    /**
     * Settlers are ordinary villagers (EQC180): the factory's villager is of the biome's type, an adult with no profession, not
     * a puppet and not tagged, kept by vanilla's own saving ({@code shouldBeSaved}); given a job it trades.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 3000, batch = "mic_rtsim_renewal_ordinary")
    public static void settlersAreOrdinaryVillagers(GameTestHelper helper) {
        ScriptedEngine stub = RtsimTests.engine();
        ServerLevel level = helper.getLevel();
        Site s = new Site(helper, false, 4);
        long op = engineOp();
        helper.startSequence()
                .thenWaitUntil(() -> check("the reach is loaded", s.complete()))
                .thenExecute(() -> arrival(s, op, AT_VILLAGE_ARRIVAL, 0, 0))
                .thenWaitUntil(() -> check("answered", result(stub, op) != null))
                .thenExecute(() -> {
                    check("placed", result(stub, op).aux() == Obs.CommandResult.APPLIED);
                    List<Villager> in = s.inBox();
                    check("one villager: " + in.size(), in.size() == 1);
                    Villager v = in.get(0);
                    check("of the biome's type: " + v.getVillagerData().getType(),
                            v.getVillagerData().getType() == VillagerType.byBiome(level.getBiome(v.blockPosition())));
                    check("an adult", !v.isBaby());
                    check("with no profession: " + v.getVillagerData().getProfession(),
                            v.getVillagerData().getProfession() == VillagerProfession.NONE);
                    check("not a puppet and not tagged", !RtsimApi.isPuppet(v) && !RtsimAttachments.isTagged(v));
                    check("vanilla saves it", v.shouldBeSaved() && v.isPersistenceRequired());
                    check("its brain is live", v.getBrain().isActive(net.minecraft.world.entity.schedule.Activity.CORE));
                    v.setVillagerData(v.getVillagerData().setProfession(VillagerProfession.FARMER).setLevel(1));
                    check("given a job it trades: " + v.getOffers().size(), !v.getOffers().isEmpty());
                    s.close();
                })
                .thenSucceed();
    }
}
