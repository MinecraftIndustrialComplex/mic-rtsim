package com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest;

import com.iafenvoy.iceandfire.entity.DragonBaseEntity;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.SiteTestHooks;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ChunkWait;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.AgentTag;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.sites.SiteLedger;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonConfig;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonRecords;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.HomeBodies;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.IafDragons;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.MicRtsimIceAndFire;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestSequence;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.animal.Wolf;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.List;
import java.util.UUID;
import java.util.concurrent.CompletableFuture;

import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.about;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.bridge;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.check;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.homeAnswers;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.record;

/**
 * Dragon home bodies on the real engine (EQUILIBRIUM.md slice S5, EQ-2 (a); docs/overnight-choices-equilibrium.md EQC139): a
 * dragon roosting at its lair is a vanilla body that vanilla saves, and a puppet while it is away. Native engine only (the
 * stub runs no lair); each test is its own batch. The lair is declared over the arena and brought in by hand
 * ({@link HomeBodies#testIn}, the engine's census) except in {@link #aHeldLairComesInAndGoesOutByItself}, which lets core's
 * chunk watch do it. The leash is lowered to 16 blocks and scanned every 5 ticks, so a walk of a few chunks is a lure.
 */
@GameTestHolder(MicRtsimIceAndFire.MODID)
@PrefixGameTestTemplate(false)
public final class HomeBodyGameTests {
    private HomeBodyGameTests() {}

    private static final long DAY = 24_000L;
    private static final String ROOST = "iceandfire:fire_dragon_roost";
    /** Where the lured dragon stands, east of the lair, past the lowered leash and past the hand-back radius. */
    private static final double LURED = 26.0;

    // ---- helpers --------------------------------------------------------------------------------

    private static boolean nativeOnly(GameTestHelper helper, String test) {
        if (DragonKit.nativeEngine())
            return true;
        record(test, "skipped", "the lairs are the engine's only on the native engine");
        helper.succeed();
        return false;
    }

    private static SiteLedger.Row row(ServerLevel level, long lair) {
        return SiteTestHooks.row(level.getServer(), SiteLedger.LAIR, lair);
    }

    private static String describe(SiteLedger.Row r) {
        if (r == null)
            return "no row";
        return String.format("K %d, count %d, away %d, P %.4f, %s, wait left %d; home %s, left behind %s, missing %s", r.k(), r.count(), r.away(),
                r.p(), r.waiting() ? "waiting" : "not waiting", r.waitLeft(), r.home(), r.leftBehind(), r.missing());
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
     * The lair's home bodies standing in the level near the arena: the dragons carrying the attachment with this lair's key (a
     * neighbouring arena's test, or a failed one's leftover, is not this lair's).
     */
    private static List<DragonBaseEntity> homeBodies(ServerLevel level, Vec3 c, long lair) {
        return level.getEntitiesOfClass(DragonBaseEntity.class, new AABB(c.x - 80, c.y - 40, c.z - 80, c.x + 80, c.y + 40, c.z + 80), e -> {
            HomeBodies.HomeTag tag = e.getExistingDataOrNull(MicRtsimIceAndFire.HOME);
            return !e.isRemoved() && tag != null && tag.lair() == lair;
        });
    }

    /**
     * Steps: a lair over the arena (not late), its own dragon standing in it, and the lair coming in by hand: the engine's
     * census adopts the dragon as the occupant and its body is a home body (DRAGON_MATERIALIZED aux 1). The agent goes into
     * {@code agent}. The first thing a test does, so the chunk waits and the logic waits are the test's own clocks.
     */
    private static GameTestSequence occupied(GameTestHelper helper, ChunkWait waits, long lair, DragonBaseEntity[] d, long[] agent) {
        ServerLevel level = helper.getLevel();
        Vec3 c = DragonKit.centre(helper);
        return helper.startSequence()
                .thenExecute(() -> {
                    bridge().lairs.declareForTest(level, lair, ROOST, DragonKit.arenaBox(helper), BlockPos.containing(c), level.getGameTime());
                    d[0] = DragonKit.roostDragon(level, c);
                    bridge().homes.testIn(lair, false, level.getGameTime());
                })
                .thenWaitUntil(waits.engine(() -> {
                    long a = bridge().agentOf(d[0]);
                    check("the census adopted the lair's dragon as a home body", a != 0 && !homeAnswers(a).isEmpty());
                }))
                .thenExecute(() -> agent[0] = bridge().agentOf(d[0]));
    }

    /** The test's lowered leash and its undo, for the end of the test. */
    private static void lowerLeash(GameTestHelper helper) {
        DragonConfig.Test.homeLeash = 16;
        DragonConfig.Test.leashScan = 5;
        DragonKit.onEnd(helper, DragonConfig.Test::reset);
    }

    /** Whether the chunk ring round the arena is loaded and entity-ticking. */
    private static ChunkPos arenaChunk(GameTestHelper helper) {
        return new ChunkPos(BlockPos.containing(DragonKit.centre(helper)));
    }

    /** The epoch of the dragon's home body, from the engine's last home answer. */
    private static int homeEpoch(long agent) {
        List<ScriptedEngine.Received> a = homeAnswers(agent);
        return a.get(a.size() - 1).view().getInt(DragonRecords.Materialized.EPOCH);
    }

    /**
     * Steps after {@link #occupied}: the home body is walked {@code blocks} east of its lair, past the lowered leash;
     * the plugin journals DRAGON_LEFT_HOME and swaps the tags in place, and the engine forgets the body.
     */
    private static GameTestSequence leashed(GameTestSequence s, GameTestHelper helper, ChunkWait waits, long[] agent, DragonBaseEntity[] d,
                                            int[] epoch, double blocks) {
        Vec3 c = DragonKit.centre(helper);
        return s.thenExecute(() -> {
                    epoch[0] = homeEpoch(agent[0]);
                    d[0].moveTo(c.x + blocks, c.y, c.z, 0f, 0f);
                })
                .thenIdle(30)
                .thenExecute(() -> record("leash", "after 30 ticks", "the dragon at " + d[0].position() + " (the lair's home " + c + "), removed "
                        + d[0].isRemoved() + ", home attachment " + d[0].hasData(MicRtsimIceAndFire.HOME) + ", agent tag " + RtsimAttachments.isTagged(d[0])
                        + ", registered " + (bridge().body(agent[0]) == d[0]) + ", swaps " + bridge().homes.leashed + ", leash " + DragonConfig.homeLeash()
                        + ", scan " + DragonConfig.leashScan()))
                .thenWaitUntil(waits.engine(() -> check("DRAGON_LEFT_HOME reached the engine", !about(DragonRecords.DRAGON_LEFT_HOME, agent[0]).isEmpty())))
                .thenExecute(() -> {
                    ScriptedEngine.Received left = about(DragonRecords.DRAGON_LEFT_HOME, agent[0]).get(0);
                    check("at the body's epoch", left.view().getInt(DragonRecords.LeftHome.EPOCH) == epoch[0]);
                    check("naming the body", DragonKit.uuidAt(left, DragonRecords.LeftHome.ENTITY_MOST).equals(d[0].getUUID()));
                    AgentTag tag = RtsimAttachments.tag(d[0]);
                    check("a puppet now: the agent tag at the same epoch", tag != null && tag.agent() == agent[0] && tag.epoch() == epoch[0]);
                    check("and no home attachment", !d[0].hasData(MicRtsimIceAndFire.HOME));
                    check("vanilla does not save it (shouldBeSaved)", !d[0].shouldBeSaved() && !d[0].save(new CompoundTag()));
                });
    }

    // ---- 1: a roosting dragon is saved by vanilla -----------------------------------------------

    /**
     * A lair over the arena with its own dragon (as Ice and Fire's roost piece makes it): the census adopts it; it carries the
     * home attachment and no agent tag; vanilla's own entity-chunk save and read-back returns it, same UUID, same attachment;
     * the lair's row names its UUID. Settles the plan's UNVERIFIED 3 (setData on a live dragon, and the save filter reading it).
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "home_saved")
    public static void roostingDragonIsSavedByVanilla(GameTestHelper helper) {
        String test = "roostingDragonIsSavedByVanilla";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 2400);
        ServerLevel level = helper.getLevel();
        Vec3 c = DragonKit.centre(helper);
        long lair = DragonKit.newLair();
        DragonBaseEntity[] d = new DragonBaseEntity[1];
        long[] agent = new long[1];
        @SuppressWarnings("unchecked")
        CompletableFuture<List<Entity>>[] read = new CompletableFuture[1];
        UUID[] id = new UUID[1];

        occupied(helper, waits, lair, d, agent)
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, lair);
                    check("the lair's row names its body: " + describe(r), r != null && r.k() == 2 && r.count() == 1 && r.home().contains(d[0].getUUID()));
                }))
                .thenExecute(() -> {
                    id[0] = d[0].getUUID();
                    HomeBodies.HomeTag tag = d[0].getExistingDataOrNull(MicRtsimIceAndFire.HOME);
                    check("the home attachment names the lair and the agent: " + tag, tag != null && tag.lair() == lair && tag.agent() == agent[0]);
                    check("and no agent tag", !RtsimAttachments.isTagged(d[0]));
                    check("vanilla would save it (shouldBeSaved)", d[0].shouldBeSaved());
                    ScriptedEngine.Received census = DragonKit.received(DragonRecords.LAIR_CENSUS, DragonRecords.LairCensus.LAIR, lair).get(0);
                    check("its census listed one body, unwatched", census.aux() == 1
                            && (census.view().getInt(DragonRecords.LairCensus.FLAGS) & DragonRecords.LairCensus.FLAG_WATCHED) == 0);
                    check("the body listed is this dragon, with no attachment yet", DragonKit.uuidAt(census, DragonRecords.LairCensus.BODIES
                            + DragonRecords.LairCensus.B_ENTITY_MOST).equals(id[0])
                            && census.view().getLong(DragonRecords.LairCensus.BODIES + DragonRecords.LairCensus.B_AGENT) == 0);
                    read[0] = DragonKit.saveAndReadBack(level, new ChunkPos(BlockPos.containing(c)));
                })
                .thenWaitUntil(waits.chunks(() -> check("the entity chunk was read back", read[0] != null && read[0].isDone())))
                .thenExecute(() -> {
                    Entity back = read[0].join().stream().filter(e -> e.getUUID().equals(id[0])).findFirst().orElse(null);
                    check("the dragon reached the disk, same UUID", back != null);
                    HomeBodies.HomeTag tag = back.getExistingDataOrNull(MicRtsimIceAndFire.HOME);
                    check("with its attachment: " + tag, tag != null && tag.lair() == lair && tag.agent() == agent[0]);
                    check("and no agent tag", !RtsimAttachments.isTagged(back));
                    record(test, "saved", "the entity chunk read back with the dragon " + id[0] + " and " + tag + "; the lair's row: " + describe(row(level, lair)));
                    bridge().homes.testOut(lair, level.getGameTime());
                    d[0].discard();
                })
                .thenSucceed();
    }

    // ---- 2: an away dragon is a puppet and the census reconciles -----------------------------------

    /**
     * The engine's dragon is walked past the leash: DRAGON_LEFT_HOME, the tags swap in place, vanilla's save does not return
     * it (UNVERIFIED 3, the other way). An old body of it, saved, is found when the lair is counted again: the dragon is away,
     * so the plugin removes it (DISCARD reason 3) and no second body exists. The dragon is taken back out of range, rests at
     * its lair owed a body, and an unwatched census spawns one.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "home_away")
    public static void awayDragonIsAPuppetAndTheCensusReconciles(GameTestHelper helper) {
        String test = "awayDragonIsAPuppetAndTheCensusReconciles";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 2400);
        ServerLevel level = helper.getLevel();
        Vec3 c = DragonKit.centre(helper);
        lowerLeash(helper);
        Runnable unforce = DragonKit.forceAround(helper, 3);
        long lair = DragonKit.newLair();
        DragonBaseEntity[] d = new DragonBaseEntity[1], stale = new DragonBaseEntity[1];
        long[] agent = new long[1];
        int[] epoch = new int[1];
        @SuppressWarnings("unchecked")
        CompletableFuture<List<Entity>>[] read = new CompletableFuture[1];
        long[] discarded0 = new long[1];

        SequenceHolder h = new SequenceHolder();
        h.s = helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> check("the ring of chunks is loaded", DragonKit.ready(level, arenaChunk(helper), 3))));
        occupiedAfter(h, helper, waits, lair, d, agent);
        leashed(h.s, helper, waits, agent, d, epoch, LURED)
                .thenExecute(() -> read[0] = DragonKit.saveAndReadBack(level, new ChunkPos(BlockPos.containing(c.add(LURED, 0, 0))), new ChunkPos(BlockPos.containing(c))))
                .thenWaitUntil(waits.chunks(() -> check("the entity chunks were read back", read[0] != null && read[0].isDone())))
                .thenExecute(() -> {
                    check("the puppet was not saved", read[0].join().stream().noneMatch(e -> e.getUUID().equals(d[0].getUUID())));
                    // An old body of it, standing in the lair with the attachment (a crash between the swap and the chunk's rewrite).
                    stale[0] = DragonKit.newRoostDragon(level, c);
                    stale[0].setData(MicRtsimIceAndFire.HOME, new HomeBodies.HomeTag(lair, agent[0]));
                    level.addFreshEntity(stale[0]);
                    discarded0[0] = bridge().discarded;
                    bridge().homes.testIn(lair, false, level.getGameTime());
                })
                .thenWaitUntil(waits.engine(() -> check("the old body was removed (DISCARD, the dragon is away)", stale[0].isRemoved()
                        && bridge().discarded > discarded0[0])))
                .thenExecute(() -> {
                    check("no second body: no home body stands", homeBodies(level, c, lair).isEmpty());
                    check("the puppet is the one body, still tagged", RtsimAttachments.isTagged(d[0]) && !d[0].isRemoved());
                    record(test, "reconciled", "the old body " + stale[0].getUUID() + " removed by the census; the row: " + describe(row(level, lair)));
                    // The dragon goes out of range: captured, it rests at its lair (26 blocks off) and is owed a body.
                    DragonKit.depart(agent[0], epoch[0], 1, 20, c.add(-400, 40, 0));
                })
                .thenWaitUntil(waits.engine(() -> check("captured (DRAGON_DEMATERIALIZED)", !about(DragonRecords.DRAGON_DEMATERIALIZED, agent[0]).isEmpty()
                        && d[0].isRemoved())))
                .thenExecute(() -> bridge().homes.testIn(lair, false, level.getGameTime()))
                .thenWaitUntil(waits.engine(() -> check("an unwatched census spawned its owed body (a second home answer)", homeAnswers(agent[0]).size() >= 2
                        && bridge().body(agent[0]) != null)))
                .thenExecute(() -> {
                    Entity b = bridge().body(agent[0]);
                    HomeBodies.HomeTag tag = b.getExistingDataOrNull(MicRtsimIceAndFire.HOME);
                    check("the new body is a home body of the lair: " + tag, tag != null && tag.lair() == lair && tag.agent() == agent[0] && !RtsimAttachments.isTagged(b));
                    check("a new body, not the old one", !b.getUUID().equals(d[0].getUUID()));
                    check("one body stands", homeBodies(level, c, lair).size() == 1);
                    record(test, "owed", "the body " + b.getUUID() + " spawned at the lair; the row: " + describe(row(level, lair)));
                    bridge().homes.testOut(lair, level.getGameTime());
                    b.discard();
                    unforce.run();
                })
                .thenSucceed();
    }

    /** A sequence handle, so a helper can continue it. */
    private static final class SequenceHolder {
        GameTestSequence s;
    }

    /** {@link #occupied}'s steps appended to a started sequence. */
    private static void occupiedAfter(SequenceHolder h, GameTestHelper helper, ChunkWait waits, long lair, DragonBaseEntity[] d, long[] agent) {
        ServerLevel level = helper.getLevel();
        Vec3 c = DragonKit.centre(helper);
        h.s = h.s.thenExecute(() -> {
                    bridge().lairs.declareForTest(level, lair, ROOST, DragonKit.arenaBox(helper), BlockPos.containing(c), level.getGameTime());
                    d[0] = DragonKit.roostDragon(level, c);
                    bridge().homes.testIn(lair, false, level.getGameTime());
                })
                .thenWaitUntil(waits.engine(() -> {
                    long a = bridge().agentOf(d[0]);
                    check("the census adopted the lair's dragon as a home body", a != 0 && !homeAnswers(a).isEmpty());
                }))
                .thenExecute(() -> agent[0] = bridge().agentOf(d[0]));
    }

    // ---- 3: uninstalling keeps the roosting dragons -----------------------------------------------

    /**
     * With the plugin stopped ({@code SiteTestHooks.stop}: an uninstall) the roosting dragon stands, same UUID, and with the
     * attachment type unknown (the attachment removed, as NeoForge skips an unknown one) it is an untagged dragon; restarted
     * after a gap, the census adopts it by its UUID: the lair is occupied again by the same body, no second body and no
     * newcomer is minted.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "home_uninstall")
    public static void uninstallKeepsRoostingDragons(GameTestHelper helper) {
        String test = "uninstallKeepsRoostingDragons";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 9000);
        ServerLevel level = helper.getLevel();
        MinecraftServer server = level.getServer();
        DragonKit.onEnd(helper, () -> SiteTestHooks.cleanUp(server));
        Vec3 c = DragonKit.centre(helper);
        long lair = DragonKit.newLair();
        DragonBaseEntity[] d = new DragonBaseEntity[1];
        long[] agent = new long[1], t = new long[1];
        int[] age = new int[1];
        UUID[] id = new UUID[1];

        occupied(helper, waits, lair, d, agent)
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, lair);
                    check("the lair's row names its body: " + describe(r), r != null && r.count() == 1 && r.home().contains(d[0].getUUID()));
                }))
                .thenExecute(() -> {
                    id[0] = d[0].getUUID();
                    t[0] = SiteTestHooks.journalEnd();
                    SiteTestHooks.requestSave();
                })
                .thenWaitUntil(waits.engine(() -> check("an engine save covers the journal through " + t[0], SiteTestHooks.savedThrough(t[0]))))
                .thenExecute(() -> {
                    SiteTestHooks.stop(server);
                    check("no session runs", RtsimSession.current() == null);
                    check("the roosting dragon stands, the same UUID, still the lair's home body", !d[0].isRemoved() && d[0].getUUID().equals(id[0])
                            && d[0].hasData(MicRtsimIceAndFire.HOME) && d[0].shouldBeSaved());
                    // The attachment type is unknown to a server without the plugin: the entity is the jar's own.
                    d[0].removeData(MicRtsimIceAndFire.HOME);
                    check("a plain untagged dragon", !d[0].hasData(MicRtsimIceAndFire.HOME) && !RtsimAttachments.isTagged(d[0]) && d[0].shouldBeSaved());
                    // What a server without the plugin reads is the saved NBT: Ice and Fire's own dragon, aging (UR-1, EQC316).
                    CompoundTag saved = new CompoundTag();
                    check("it is saved", d[0].save(saved));
                    check("saved aging, as Ice and Fire left it: AgingDisabled " + saved.getBoolean("AgingDisabled"), !saved.getBoolean("AgingDisabled"));
                    age[0] = IafDragons.ageTicks(d[0]);
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    int now = IafDragons.ageTicks(d[0]);
                    check("with no plugin it ages: AgeTicks " + age[0] + " then " + now + " 40 ticks on", now > age[0]);
                    // The game clock moves by the gap, and a test's own clock (ChunkWait's) with it: a short gap.
                    SiteTestHooks.gapRestart(server, 1200);
                    check("a session runs after the reinstall: " + RtsimSession.startFailure(), RtsimSession.current() != null);
                    // Lairs are found again at each session's start (Lairs.clear): this one as a test declares it.
                    bridge().lairs.declareForTest(level, lair, ROOST, DragonKit.arenaBox(helper), BlockPos.containing(c), level.getGameTime());
                    bridge().homes.testIn(lair, false, level.getGameTime());
                })
                .thenWaitUntil(RtsimTests.io(() -> {
                    long a = bridge().agentOf(d[0]);
                    check("the census adopted the standing dragon by its UUID", a != 0 && !homeAnswers(a).isEmpty());
                }))
                .thenWaitUntil(RtsimTests.io(() -> {
                    SiteLedger.Row r = row(level, lair);
                    check("the lair is occupied again: " + describe(r), r != null && r.count() == 1 && !r.waiting() && r.home().contains(id[0]));
                }))
                .thenExecute(() -> {
                    HomeBodies.HomeTag tag = d[0].getExistingDataOrNull(MicRtsimIceAndFire.HOME);
                    check("retagged as a home body of the lair: " + tag, tag != null && tag.lair() == lair && !RtsimAttachments.isTagged(d[0]));
                    check("the one body, no newcomer", homeBodies(level, c, lair).size() == 1 && homeBodies(level, c, lair).get(0) == d[0]);
                    record(test, "reinstalled", "the dragon " + id[0] + " adopted by its UUID after the gap; the row: " + describe(row(level, lair)));
                    bridge().homes.testOut(lair, level.getGameTime());
                    d[0].discard();
                })
                .thenSucceed();
    }

    // ---- 4: a lure is no loss ---------------------------------------------------------------------

    /**
     * A home body walked off is a puppet and still the lair's occupant: no pressure, nothing missing, nothing left behind
     * (EQ-2 (a), FB-1 (a)). Unloaded (captured out of range) it is not saved, and the engine flies it home abstractly.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "home_lured")
    public static void aDragonLuredFromItsLairIsAwayNotMissing(GameTestHelper helper) {
        String test = "aDragonLuredFromItsLairIsAwayNotMissing";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 2400);
        ServerLevel level = helper.getLevel();
        Vec3 c = DragonKit.centre(helper);
        lowerLeash(helper);
        Runnable unforce = DragonKit.forceAround(helper, 3);
        long lair = DragonKit.newLair();
        DragonBaseEntity[] d = new DragonBaseEntity[1];
        long[] agent = new long[1];
        int[] epoch = new int[1];
        SequenceHolder h = new SequenceHolder();
        h.s = helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> check("the ring of chunks is loaded", DragonKit.ready(level, arenaChunk(helper), 3))));
        occupiedAfter(h, helper, waits, lair, d, agent);
        leashed(h.s, helper, waits, agent, d, epoch, 40.0)
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, lair);
                    check("still the occupant, away, nothing lost: " + describe(r), r != null && r.count() == 1 && r.away() == 1 && !r.waiting()
                            && r.p() < 1e-3 && r.home().isEmpty() && r.missing().isEmpty() && r.leftBehind().isEmpty());
                }))
                .thenExecute(() -> {
                    check("no death was reported", about(DragonRecords.DRAGON_DIED, agent[0]).isEmpty());
                    record(test, "lured", "the lair's row after the leash: " + describe(row(level, lair)));
                    DragonKit.depart(agent[0], epoch[0], 1, 20, c.add(-400, 40, 0));
                })
                .thenWaitUntil(waits.engine(() -> check("captured out of range", !about(DragonRecords.DRAGON_DEMATERIALIZED, agent[0]).isEmpty() && d[0].isRemoved())))
                .thenIdle(40)
                .thenExecute(() -> {
                    SiteLedger.Row r = row(level, lair);
                    check("after the capture: still the occupant, away, no pressure, no stray left: " + describe(r), r != null && r.count() == 1
                            && r.away() == 1 && !r.waiting() && r.p() < 1e-3 && r.missing().isEmpty() && r.leftBehind().isEmpty());
                    check("no home body stands in the world", homeBodies(level, c, lair).isEmpty());
                    record(test, "gone home", "captured, not saved; the row: " + describe(r));
                    unforce.run();
                })
                .thenSucceed();
    }

    // ---- 5: a vanished home body is missing, not dead ---------------------------------------------

    /**
     * A saved body (its lair out) vanishes with no death, by something other than the plugin. The census misses it three times
     * over two game days (the test's clock: two idles of a day): then the dragon is written off as a player's loss (the lair's
     * pressure 1, waiting) and its UUID is in the row's {@code missing}. The same body put back while the lair is short is home
     * again. The rule with a gap (Other, the cap) is the Rust tests'.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 200_000, batch = "home_vanished")
    public static void aVanishedHomeBodyIsMissingNotDead(GameTestHelper helper) {
        String test = "aVanishedHomeBodyIsMissingNotDead";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 150_000);
        ServerLevel level = helper.getLevel();
        Vec3 c = DragonKit.centre(helper);
        long lair = DragonKit.newLair();
        DragonBaseEntity[] d = new DragonBaseEntity[1];
        long[] agent = new long[1];
        UUID[] id = new UUID[1];
        DragonBaseEntity[] back = new DragonBaseEntity[1];

        occupied(helper, waits, lair, d, agent)
                .thenExecute(() -> {
                    id[0] = d[0].getUUID();
                    // Its lair goes out: the body is released, saved by vanilla; then it vanishes without a word.
                    bridge().homes.testOut(lair, level.getGameTime());
                    d[0].discard();
                    check("nobody died", about(DragonRecords.DRAGON_DIED, agent[0]).isEmpty());
                    bridge().homes.testIn(lair, false, level.getGameTime());
                })
                .thenIdle(200)
                .thenExecute(() -> {
                    SiteLedger.Row r = row(level, lair);
                    check("one miss books nothing: " + describe(r), r != null && r.count() == 1 && !r.waiting() && r.p() < 1e-3);
                })
                .thenIdle((int) DAY)
                .thenExecute(() -> bridge().homes.testIn(lair, false, level.getGameTime()))
                .thenIdle((int) DAY + 200)
                .thenExecute(() -> {
                    SiteLedger.Row r = row(level, lair);
                    check("two misses book nothing: " + describe(r), r != null && r.count() == 1 && !r.waiting() && r.p() < 1e-3);
                    bridge().homes.testIn(lair, false, level.getGameTime());
                })
                .thenWaitUntil(RtsimTests.io(() -> {
                    SiteLedger.Row r = row(level, lair);
                    check("the third miss, two days on, is a player's loss: " + describe(r), r != null && r.count() == 0 && r.waiting() && r.p() > 0.9f
                            && r.missing().contains(id[0]));
                }))
                .thenExecute(() -> {
                    record(test, "written off", "after three misses over two days: " + describe(row(level, lair)));
                    check("the dragon is gone, not dead: no DRAGON_DIED", about(DragonRecords.DRAGON_DIED, agent[0]).isEmpty());
                    // The same body put back while the lair is short: home again.
                    back[0] = DragonKit.newRoostDragon(level, c);
                    back[0].setUUID(id[0]);
                    back[0].setData(MicRtsimIceAndFire.HOME, new HomeBodies.HomeTag(lair, agent[0]));
                    level.addFreshEntity(back[0]);
                    bridge().homes.testIn(lair, false, level.getGameTime());
                })
                .thenWaitUntil(RtsimTests.io(() -> {
                    SiteLedger.Row r = row(level, lair);
                    check("home again: " + describe(r), r != null && r.count() == 1 && r.missing().isEmpty() && r.home().contains(id[0]));
                }))
                .thenExecute(() -> {
                    record(test, "home again", describe(row(level, lair)));
                    bridge().homes.testOut(lair, level.getGameTime());
                    back[0].discard();
                })
                .thenSucceed();
    }

    // ---- 6: a home body's kill credit is the lair's loss --------------------------------------------

    /**
     * Three lairs in the arena, a home body in each: a player's kill, a tamed wolf's kill and a trap's kill (lava, no wild
     * credit) each press their lair once (waiting, P 1), and each leaves a corpse with no attachment.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "home_credit")
    public static void aHomeBodysKillCreditIsTheLairsLoss(GameTestHelper helper) {
        String test = "aHomeBodysKillCreditIsTheLairsLoss";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 2400);
        ServerLevel level = helper.getLevel();
        Vec3 c = DragonKit.centre(helper);
        long[] lair = {DragonKit.newLair(), DragonKit.newLair(), DragonKit.newLair()};
        int[] x = {0, 4, 8}; // three disjoint boxes along the arena
        DragonBaseEntity[] d = new DragonBaseEntity[3];
        long[] agent = new long[3];
        ServerPlayer[] player = new ServerPlayer[2];
        Wolf[] wolf = new Wolf[1];

        helper.startSequence()
                .thenExecute(() -> {
                    for (int i = 0; i < 3; i++) {
                        BoundingBox box = BoundingBox.fromCorners(helper.absolutePos(new BlockPos(x[i], 0, 0)), helper.absolutePos(new BlockPos(x[i] + 2, 5, 10)));
                        Vec3 at = helper.absoluteVec(new Vec3(x[i] + 1.5, 2, 5.5));
                        bridge().lairs.declareForTest(level, lair[i], ROOST, box, BlockPos.containing(at), level.getGameTime());
                        d[i] = DragonKit.roostDragon(level, at);
                        bridge().homes.testIn(lair[i], false, level.getGameTime());
                    }
                })
                .thenWaitUntil(waits.engine(() -> {
                    for (int i = 0; i < 3; i++) {
                        long a = bridge().agentOf(d[i]);
                        check("lair " + i + "'s dragon is a home body", a != 0 && !homeAnswers(a).isEmpty());
                    }
                }))
                .thenExecute(() -> {
                    for (int i = 0; i < 3; i++)
                        agent[i] = bridge().agentOf(d[i]);
                    player[0] = FakePlayers.add(level, c.add(0, 0, 4), "home-killer");
                    player[1] = FakePlayers.add(level, c.add(4, 0, 4), "home-wolf-owner");
                    wolf[0] = EntityType.WOLF.create(level);
                    check("a wolf is created", wolf[0] != null);
                    wolf[0].moveTo(c.x + 2, c.y, c.z + 2, 0f, 0f);
                    level.addFreshEntity(wolf[0]);
                    wolf[0].tame(player[1]);
                    kill(d[0], level.damageSources().playerAttack(player[0]));
                    kill(d[1], level.damageSources().mobAttack(wolf[0]));
                    kill(d[2], level.damageSources().lava());
                })
                .thenWaitUntil(waits.engine(() -> {
                    for (int i = 0; i < 3; i++)
                        check("DRAGON_DIED for dragon " + i, !about(DragonRecords.DRAGON_DIED, agent[i]).isEmpty());
                }))
                .thenWaitUntil(waits.engine(() -> {
                    for (int i = 0; i < 3; i++) {
                        SiteLedger.Row r = row(level, lair[i]);
                        check("lair " + i + " pressed once, waiting at P 1: " + describe(r), r != null && r.count() == 0 && r.waiting()
                                && Math.abs(r.p() - 1f) < 1e-3);
                    }
                }))
                .thenExecute(() -> {
                    int[] kinds = {DragonRecords.ATTACKER_PLAYER, DragonRecords.ATTACKER_PLAYER, -1};
                    for (int i = 0; i < 3; i++) {
                        ScriptedEngine.Received died = about(DragonRecords.DRAGON_DIED, agent[i]).get(0);
                        check("dragon " + i + " died with the body's entity and lair named", DragonKit.uuidAt(died, DragonRecords.Died.ENTITY_MOST).equals(d[i].getUUID())
                                && died.view().getLong(DragonRecords.Died.LAIR) == lair[i]);
                        if (kinds[i] >= 0)
                            check("kill " + i + " is a player's (aux " + died.aux() + ")", died.aux() == kinds[i]);
                        check("corpse " + i + " carries no attachment", !d[i].hasData(MicRtsimIceAndFire.HOME) && !RtsimAttachments.isTagged(d[i]));
                        record(test, "lair " + i, "died aux " + died.aux() + "; the row: " + describe(row(level, lair[i])));
                    }
                    FakePlayers.remove(player[0]);
                    FakePlayers.remove(player[1]);
                    wolf[0].discard();
                })
                .thenSucceed();
    }

    // ---- 7: a reset finds the roosting dragon by the census -----------------------------------------

    /**
     * A plain reset (S3's hook): the new engine knows nothing, the world's ledger keeps the lair's row. The new engine's lair
     * expects the row's body; the census adopts it as the occupant, no wait is left, the pressure kept, and no newcomer is
     * minted.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "home_reset")
    public static void resetFindsTheRoostingDragonByCensus(GameTestHelper helper) {
        String test = "resetFindsTheRoostingDragonByCensus";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 9000);
        ServerLevel level = helper.getLevel();
        MinecraftServer server = level.getServer();
        DragonKit.onEnd(helper, () -> SiteTestHooks.cleanUp(server));
        Vec3 c = DragonKit.centre(helper);
        long lair = DragonKit.newLair();
        DragonBaseEntity[] d = new DragonBaseEntity[1];
        long[] agent = new long[1], t = new long[1];
        UUID[] id = new UUID[1];

        occupied(helper, waits, lair, d, agent)
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, lair);
                    check("the lair's row names its body: " + describe(r), r != null && r.count() == 1 && r.home().contains(d[0].getUUID()));
                }))
                .thenExecute(() -> {
                    id[0] = d[0].getUUID();
                    t[0] = SiteTestHooks.journalEnd();
                    SiteTestHooks.requestSave();
                })
                .thenWaitUntil(waits.engine(() -> check("an engine save covers the journal through " + t[0], SiteTestHooks.savedThrough(t[0]))))
                .thenExecute(() -> {
                    SiteTestHooks.plainReset(server);
                    check("a session runs after the reset: " + RtsimSession.startFailure(), RtsimSession.current() != null);
                    check("the body stands through the reset", !d[0].isRemoved() && d[0].hasData(MicRtsimIceAndFire.HOME));
                    bridge().lairs.declareForTest(level, lair, ROOST, DragonKit.arenaBox(helper), BlockPos.containing(c), level.getGameTime());
                })
                .thenWaitUntil(waits.engine(() -> check("the new engine has the lair's LAIR_FOUND",
                        !DragonKit.received(DragonRecords.LAIR_FOUND, DragonRecords.LairFound.LAIR, lair).isEmpty())))
                .thenExecute(() -> bridge().homes.testIn(lair, false, level.getGameTime()))
                .thenWaitUntil(waits.engine(() -> {
                    long a = bridge().agentOf(d[0]);
                    check("the census adopted the saved body", a != 0 && !homeAnswers(a).isEmpty());
                }))
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, lair);
                    check("the lair is occupied, no wait left, its pressure kept: " + describe(r), r != null && r.count() == 1 && !r.waiting()
                            && r.p() < 1e-3 && r.home().contains(id[0]));
                }))
                .thenExecute(() -> {
                    check("the one body, no newcomer", homeBodies(level, c, lair).size() == 1 && homeBodies(level, c, lair).get(0) == d[0]);
                    record(test, "reset", "the body " + id[0] + " adopted by the new engine; the row: " + describe(row(level, lair)));
                    bridge().homes.testOut(lair, level.getGameTime());
                    d[0].discard();
                })
                .thenSucceed();
    }

    // ---- 8, 9: a watched roost and an unwatched one ---------------------------------------------------

    /**
     * Watched or not (the plugin's rule: a player within the larger of {@code watchBlocks} and the distance a client tracks an
     * entity at): the dragon is led off and taken back out of range, so it rests at its lair owed a body. The lair comes in
     * with a player at 60 blocks: the census says WATCHED and the engine spawns nothing (a player's area materializes the
     * dragon, not the census). The player gone, the next census says unwatched and the owed body is spawned. (The recall of an
     * away dragon whose body stands is the Rust tests': a GameTest cannot stage a hunt with no prey.)
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "home_watched")
    public static void aWatchedRoostSpawnsNothingAndAnUnwatchedOneSpawnsTheOwedBody(GameTestHelper helper) {
        String test = "aWatchedRoostSpawnsNothingAndAnUnwatchedOneSpawnsTheOwedBody";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 2400);
        ServerLevel level = helper.getLevel();
        Vec3 c = DragonKit.centre(helper);
        lowerLeash(helper);
        Runnable unforce = DragonKit.forceAround(helper, 4);
        long lair = DragonKit.newLair();
        DragonBaseEntity[] d = new DragonBaseEntity[1];
        long[] agent = new long[1];
        int[] epoch = new int[1];
        ServerPlayer[] watcher = new ServerPlayer[1];
        SequenceHolder h = new SequenceHolder();
        h.s = helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> check("the ring of chunks is loaded", DragonKit.ready(level, arenaChunk(helper), 4))));
        occupiedAfter(h, helper, waits, lair, d, agent);
        leashed(h.s, helper, waits, agent, d, epoch, LURED)
                .thenExecute(() -> DragonKit.depart(agent[0], epoch[0], 1, 20, c.add(-400, 40, 0)))
                .thenWaitUntil(waits.engine(() -> check("captured out of range", !about(DragonRecords.DRAGON_DEMATERIALIZED, agent[0]).isEmpty() && d[0].isRemoved())))
                .thenExecute(() -> {
                    watcher[0] = FakePlayers.add(level, c.add(60, 0, 0), "home-watcher");
                    bridge().homes.testIn(lair, null, level.getGameTime());
                })
                .thenWaitUntil(waits.engine(() -> {
                    List<ScriptedEngine.Received> census = DragonKit.received(DragonRecords.LAIR_CENSUS, DragonRecords.LairCensus.LAIR, lair);
                    check("a census with a player at 60 blocks reached the engine", census.size() >= 2);
                    ScriptedEngine.Received last = census.get(census.size() - 1);
                    check("it says watched", (last.view().getInt(DragonRecords.LairCensus.FLAGS) & DragonRecords.LairCensus.FLAG_WATCHED) != 0);
                }))
                .thenIdle(120)
                .thenExecute(() -> {
                    check("the watched census spawned nothing", homeAnswers(agent[0]).size() == 1 && homeBodies(level, c, lair).isEmpty());
                    FakePlayers.remove(watcher[0]);
                    bridge().homes.testIn(lair, null, level.getGameTime());
                })
                .thenWaitUntil(waits.engine(() -> {
                    List<ScriptedEngine.Received> census = DragonKit.received(DragonRecords.LAIR_CENSUS, DragonRecords.LairCensus.LAIR, lair);
                    ScriptedEngine.Received last = census.get(census.size() - 1);
                    check("the next census says unwatched", census.size() >= 3 && (last.view().getInt(DragonRecords.LairCensus.FLAGS)
                            & DragonRecords.LairCensus.FLAG_WATCHED) == 0);
                    check("and the owed body was spawned (a second home answer)", homeAnswers(agent[0]).size() >= 2 && bridge().body(agent[0]) != null);
                }))
                .thenExecute(() -> {
                    Entity body = bridge().body(agent[0]);
                    record(test, "spawned", "unwatched, the owed body " + body.getUUID() + " stands; the row: " + describe(row(level, lair)));
                    bridge().homes.testOut(lair, level.getGameTime());
                    body.discard();
                    unforce.run();
                })
                .thenSucceed();
    }

    // ---- 9b: a lair held loaded comes in and goes out by itself -------------------------------------

    /**
     * Core's chunk watch end to end, at a far site held entity-ticking by a ticket (a chunk loader): the lair comes in by
     * itself, with no test hook, and the census adopts its dragon as a home body. Let go of the ticket, the lair goes out by
     * itself: the home body is released (DRAGON_DEMATERIALIZED reason 8, LAIR_OUT) and, when its chunk unloads, leaves the
     * level; the removal reason the leave event carried is recorded (UNVERIFIED 1). Settles UNVERIFIED 5 for this footprint.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "home_held")
    public static void aHeldLairComesInAndGoesOutByItself(GameTestHelper helper) {
        String test = "aHeldLairComesInAndGoesOutByItself";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 2400);
        ServerLevel level = helper.getLevel();
        BlockPos site = DragonKit.farSite();
        long lair = DragonKit.newLair();
        DragonConfig.Test.watchLairs = Boolean.TRUE;
        DragonKit.onEnd(helper, DragonConfig.Test::reset);
        ChunkWait.Hold hold = waits.hold(site, 48, ChunkWait.Need.ENTITY_TICKING);
        DragonBaseEntity[] d = new DragonBaseEntity[1];
        long[] agent = new long[1], t = new long[3];
        UUID[] id = new UUID[1];
        long[] giveUp = new long[1];

        helper.startSequence()
                .thenWaitUntil(waits.ready(hold))
                .thenExecute(() -> {
                    int y = level.getHeight(Heightmap.Types.MOTION_BLOCKING, site.getX(), site.getZ());
                    BlockPos home = new BlockPos(site.getX(), y, site.getZ());
                    Vec3 at = new Vec3(site.getX() + 0.5, y, site.getZ() + 0.5);
                    d[0] = DragonKit.roostDragon(level, at);
                    id[0] = d[0].getUUID();
                    t[0] = level.getGameTime();
                    bridge().lairs.declareForTest(level, lair, ROOST, BoundingBox.fromCorners(home.offset(-5, -2, -5), home.offset(5, 6, 5)), home, t[0]);
                })
                .thenWaitUntil(waits.engine(() -> {
                    long a = bridge().agentOf(d[0]);
                    check("the lair came in by itself and its dragon is a home body", bridge().homes.isIn(lair) && a != 0 && !homeAnswers(a).isEmpty());
                }))
                .thenExecute(() -> {
                    agent[0] = bridge().agentOf(d[0]);
                    t[1] = level.getGameTime();
                    ScriptedEngine.Received census = DragonKit.received(DragonRecords.LAIR_CENSUS, DragonRecords.LairCensus.LAIR, lair).get(0);
                    check("an unwatched census listed the dragon", census.aux() == 1
                            && (census.view().getInt(DragonRecords.LairCensus.FLAGS) & DragonRecords.LairCensus.FLAG_WATCHED) == 0);
                    record(test, "in", "the lair came in " + (t[1] - t[0]) + " ticks after it was declared (a poll every 20), a 48 block footprint of "
                            + ChunkWait.Need.ENTITY_TICKING + " chunks held; the census listed the dragon " + id[0]);
                    hold.release();
                })
                .thenWaitUntil(waits.chunks(() -> check("the lair went out by itself (LAIR_OUT)", !DragonKit.received(DragonRecords.LAIR_OUT,
                        DragonRecords.LairOut.LAIR, lair).isEmpty())))
                .thenWaitUntil(waits.chunks(() -> {
                    // At the out, or as soon as its chunk stops ticking (EQC312): the whole footprint let go, at once.
                    List<ScriptedEngine.Received> demat = about(DragonRecords.DRAGON_DEMATERIALIZED, agent[0]);
                    check("the home body was released as saved (reason 8)", !demat.isEmpty() && demat.get(demat.size() - 1).aux() == DragonRecords.Dematerialized.HOME_SAVED);
                }))
                .thenExecute(() -> {
                    t[2] = level.getGameTime();
                    record(test, "out", "the lair went out " + (t[2] - t[1]) + " ticks after the ticket was let go; the body released with reason 8");
                    giveUp[0] = System.nanoTime() + 30_000_000_000L;
                })
                .thenWaitUntil(waits.chunks(() -> {
                    if (System.nanoTime() < giveUp[0])
                        check("the body's chunk unloaded", d[0].isRemoved());
                }))
                .thenExecute(() -> {
                    record(test, "unloaded", d[0].isRemoved() ? "the body left the level, removal reason " + d[0].getRemovalReason()
                            : "its chunk had not unloaded 30 s after the ticket was let go; the body stands, released");
                    if (!d[0].isRemoved())
                        d[0].discard();
                })
                .thenSucceed();
    }

    // ---- 10: a settled puppet at an in lair becomes a home body --------------------------------------

    /**
     * A puppet that has landed at its lair, the lair in: the engine orders DRAGON_HAND_BACK, the tags swap in place, the home
     * answer follows (aux 1), and vanilla's save returns it.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "home_hand_back")
    public static void aSettledPuppetAtAnInLairBecomesAHomeBody(GameTestHelper helper) {
        String test = "aSettledPuppetAtAnInLairBecomesAHomeBody";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 2400);
        ServerLevel level = helper.getLevel();
        Vec3 c = DragonKit.centre(helper);
        lowerLeash(helper);
        Runnable unforce = DragonKit.forceAround(helper, 3);
        long lair = DragonKit.newLair();
        DragonBaseEntity[] d = new DragonBaseEntity[1];
        long[] agent = new long[1];
        int[] epoch = new int[1];
        @SuppressWarnings("unchecked")
        CompletableFuture<List<Entity>>[] read = new CompletableFuture[1];
        SequenceHolder h = new SequenceHolder();
        h.s = helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> check("the ring of chunks is loaded", DragonKit.ready(level, arenaChunk(helper), 3))));
        occupiedAfter(h, helper, waits, lair, d, agent);
        leashed(h.s, helper, waits, agent, d, epoch, LURED)
                .thenExecute(() -> {
                    // It lands back home: within the hand-back radius, on the ground.
                    d[0].moveTo(c.x + 3, c.y, c.z, 0f, 0f);
                })
                .thenWaitUntil(waits.engine(() -> check("a second home answer: the puppet is a home body again", homeAnswers(agent[0]).size() >= 2)))
                .thenExecute(() -> {
                    HomeBodies.HomeTag tag = d[0].getExistingDataOrNull(MicRtsimIceAndFire.HOME);
                    check("the home attachment is back: " + tag, tag != null && tag.lair() == lair && tag.agent() == agent[0]);
                    check("and no agent tag", !RtsimAttachments.isTagged(d[0]));
                    check("the same body", bridge().body(agent[0]) == d[0] && !d[0].isRemoved());
                    check("vanilla would save it", d[0].shouldBeSaved());
                    read[0] = DragonKit.saveAndReadBack(level, new ChunkPos(BlockPos.containing(c)), new ChunkPos(d[0].blockPosition()));
                })
                .thenWaitUntil(waits.chunks(() -> check("the entity chunk was read back", read[0] != null && read[0].isDone())))
                .thenExecute(() -> {
                    check("vanilla's save returns it (the dragon at " + d[0].blockPosition() + ", removed " + d[0].isRemoved() + ", read "
                            + read[0].join().size() + " entities)", read[0].join().stream().anyMatch(e -> e.getUUID().equals(d[0].getUUID())));
                    record(test, "handed back", "the puppet is the home body of lair " + Long.toHexString(lair) + " again; the row: " + describe(row(level, lair)));
                    bridge().homes.testOut(lair, level.getGameTime());
                    d[0].discard();
                    unforce.run();
                })
                .thenSucceed();
    }

    // ---- the Ice and Fire facts the plan left UNVERIFIED (EQC146) -------------------------------------

    /**
     * UNVERIFIED 2: a live body is synced to the engine's age and hunger through Ice and Fire's setters
     * ({@link IafDragons#sync}). The setters only set entity data, so the attributes (max health follows the age) are
     * refreshed after: the test records what the setters alone did and what the refresh did.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 100, batch = "home_sync")
    public static void aLiveBodysAgeSyncRefreshesItsAttributes(GameTestHelper helper) {
        String test = "aLiveBodysAgeSyncRefreshesItsAttributes";
        ServerLevel level = helper.getLevel();
        Vec3 c = DragonKit.centre(helper);
        DragonBaseEntity d = DragonKit.roostDragon(level, c);
        d.refreshDirtyAttributes();
        float m0 = d.getMaxHealth();
        int old = IafDragons.ageTicks(d);
        d.setAgeInTicks(110 * (int) DAY);
        d.setHunger(80);
        float m1 = d.getMaxHealth();
        IafDragons.sync(d, 110 * (int) DAY, 80, 0f);
        float m2 = d.getMaxHealth();
        record(test, "max health", "age " + old / DAY + " days: " + m0 + "; the setters alone to 110 days: " + m1 + "; after refreshDirtyAttributes: " + m2);
        check("the sync follows the age (" + m0 + " to " + m2 + ")", m2 > m0);
        check("age and hunger are the engine's, and it ages as Ice and Fire's own (EQC316)", IafDragons.ageTicks(d) == 110 * (int) DAY
                && IafDragons.hunger(d) == 80 && !IafDragons.agingDisabled(d));
        IafDragons.sync(d, 110 * (int) DAY, 80, 100f);
        check("a health to sync is clamped to the max (" + d.getHealth() + ")", d.getHealth() <= d.getMaxHealth() && d.getHealth() > 0f);
        d.discard();
        helper.succeed();
    }

    /**
     * UNVERIFIED 4: a dragon given a home position keeps it through Ice and Fire's own NBT write and read (what vanilla's save
     * uses), while a home with a coordinate of exactly 0 is dropped on read (EQC102). Recorded for both; asserted for the first.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 100, batch = "home_nbt")
    public static void aHomeSurvivesVanillasSaveUnlessACoordinateIsZero(GameTestHelper helper) {
        String test = "aHomeSurvivesVanillasSaveUnlessACoordinateIsZero";
        ServerLevel level = helper.getLevel();
        Vec3 c = DragonKit.centre(helper);
        BlockPos home = BlockPos.containing(c);
        BlockPos zero = new BlockPos(0, 70, 5);
        BlockPos[] got = new BlockPos[2];
        BlockPos[] want = {home, zero};
        for (int i = 0; i < 2; i++) {
            DragonBaseEntity a = DragonKit.newRoostDragon(level, c);
            IafDragons.setHome(a, level, want[i]);
            CompoundTag t = new CompoundTag();
            check("the save carries the flag", a.save(t) && t.getBoolean("HasHomePosition"));
            DragonBaseEntity b = DragonKit.newRoostDragon(level, c);
            b.readAdditionalSaveData(t);
            got[i] = IafDragons.home(b);
            record(test, i == 0 ? "a home of nonzero coordinates" : "a home with a zero coordinate",
                    want[i].toShortString() + " read back as " + (got[i] == null ? "no home (flag " + b.hasHomePosition + ")" : got[i].toShortString()));
        }
        check("a home of nonzero coordinates survives the save and read", home.equals(got[0]));
        helper.succeed();
    }

    // ---- the S5 review's findings (EQC310 onward) -------------------------------------------------

    /**
     * EQC311 (finding 2): a home body of the dragon stands loaded but unregistered (its lair went out, vanilla holds it), and a
     * spawn of a home body for the same agent comes: the plugin refuses it (SPAWN_REJECTED ALREADY_EMBODIED), so no second body
     * of one dragon stands. The registry alone does not know such a body.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "home_spawn_refused")
    public static void aHomeSpawnBesideAStandingHomeBodyOfTheDragonIsRefused(GameTestHelper helper) {
        String test = "aHomeSpawnBesideAStandingHomeBodyOfTheDragonIsRefused";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 2400);
        ServerLevel level = helper.getLevel();
        Vec3 c = DragonKit.centre(helper);
        Runnable unforce = DragonKit.forceAround(helper, 3);
        long lair = DragonKit.newLair();
        DragonBaseEntity[] d = new DragonBaseEntity[1];
        long[] agent = new long[1];
        int[] epoch = new int[1];
        long[] rejected0 = new long[1];
        SequenceHolder h = new SequenceHolder();
        h.s = helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> check("the ring of chunks is loaded", DragonKit.ready(level, arenaChunk(helper), 3))));
        occupiedAfter(h, helper, waits, lair, d, agent);
        h.s.thenExecute(() -> {
                    epoch[0] = homeEpoch(agent[0]);
                    bridge().homes.testOut(lair, level.getGameTime());
                    check("released, it stands with its attachment", !d[0].isRemoved() && d[0].hasData(MicRtsimIceAndFire.HOME)
                            && bridge().body(agent[0]) == null);
                    rejected0[0] = bridge().rejected;
                    DragonKit.Spawn y = DragonKit.young(agent[0], helper, c, DragonRecords.Materialize.FLAG_HOME_BODY | DragonRecords.Materialize.FLAG_SLEEPING);
                    DragonKit.materialize(new DragonKit.Spawn(y.agent(), epoch[0] + 1, y.species(), y.dimension(), y.at(), y.ageTicks(), y.hunger(), y.male(),
                            y.variant(), y.health(), BlockPos.containing(c), y.target(), null, y.flags()));
                })
                .thenWaitUntil(waits.engine(() -> check("the spawn was answered", bridge().rejected > rejected0[0] || bridge().body(agent[0]) != null)))
                .thenExecute(() -> {
                    Entity second = bridge().body(agent[0]);
                    List<ScriptedEngine.Received> r = about(DragonRecords.DRAGON_SPAWN_REJECTED, agent[0]);
                    record(test, "answer", "rejected " + (bridge().rejected - rejected0[0]) + (r.isEmpty() ? "" : " (reason " + r.get(r.size() - 1).aux() + ")")
                            + ", a body registered " + (second != null) + ", home bodies standing " + homeBodies(level, c, lair).size());
                    if (second != null && second != d[0])
                        second.discard();
                    check("refused: a body carrying the agent stands (SPAWN_REJECTED ALREADY_EMBODIED)", second == null && !r.isEmpty()
                            && r.get(r.size() - 1).aux() == DragonRecords.SpawnRejected.ALREADY_EMBODIED);
                    check("one body of the dragon", homeBodies(level, c, lair).size() == 1);
                    d[0].discard();
                    unforce.run();
                })
                .thenSucceed();
    }

    /**
     * EQC312 (finding 3): a lair goes out (a footprint chunk drops past FULL) while its home body's chunk still entity-ticks
     * (vanilla ticks entities to the simulation distance): the body is not released, it stays registered and leashed, and it is
     * released with reason 8 only when its own chunk stops ticking. Core's chunk watch end to end at a far site.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "home_out_ticking")
    public static void aHomeBodyWhoseChunkStillTicksStaysLeashedPastItsLairsOut(GameTestHelper helper) {
        String test = "aHomeBodyWhoseChunkStillTicksStaysLeashedPastItsLairsOut";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 2400);
        ServerLevel level = helper.getLevel();
        BlockPos site = DragonKit.farSite();
        long lair = DragonKit.newLair();
        DragonConfig.Test.watchLairs = Boolean.TRUE;
        DragonKit.onEnd(helper, DragonConfig.Test::reset);
        ChunkWait.Hold hold = waits.hold(site, 48, ChunkWait.Need.ENTITY_TICKING);
        DragonBaseEntity[] d = new DragonBaseEntity[1];
        long[] agent = new long[1];
        Runnable[] unforce = new Runnable[1];
        long[] giveUp = new long[1];

        helper.startSequence()
                .thenWaitUntil(waits.ready(hold))
                .thenExecute(() -> {
                    int y = level.getHeight(Heightmap.Types.MOTION_BLOCKING, site.getX(), site.getZ());
                    BlockPos home = new BlockPos(site.getX(), y, site.getZ());
                    d[0] = DragonKit.roostDragon(level, new Vec3(site.getX() + 0.5, y, site.getZ() + 0.5));
                    bridge().lairs.declareForTest(level, lair, ROOST, BoundingBox.fromCorners(home.offset(-5, -2, -5), home.offset(5, 6, 5)), home,
                            level.getGameTime());
                })
                .thenWaitUntil(waits.engine(() -> {
                    long a = bridge().agentOf(d[0]);
                    check("the lair came in and its dragon is a home body", bridge().homes.isIn(lair) && a != 0 && !homeAnswers(a).isEmpty());
                }))
                .thenExecute(() -> {
                    agent[0] = bridge().agentOf(d[0]);
                    // The body's own chunk keeps entity-ticking (a forced chunk: level 31 there, 34 three chunks out, so the
                    // footprint's far edge drops past FULL), then the footprint's hold is let go.
                    unforce[0] = DragonKit.forceChunks(level, new ChunkPos(d[0].blockPosition()), 0);
                    hold.release();
                })
                .thenWaitUntil(waits.chunks(() -> check("the lair went out by itself (LAIR_OUT)", !DragonKit.received(DragonRecords.LAIR_OUT,
                        DragonRecords.LairOut.LAIR, lair).isEmpty())))
                .thenIdle(40)
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> demat = about(DragonRecords.DRAGON_DEMATERIALIZED, agent[0]);
                    record(test, "out", "LAIR_OUT; the body's chunk ticking " + RtsimSession.entitiesTickAt(level, d[0].blockPosition())
                            + ", registered " + (bridge().body(agent[0]) == d[0]) + ", DEMATERIALIZED " + demat.size());
                    check("its chunk still ticks", RtsimSession.entitiesTickAt(level, d[0].blockPosition()));
                    check("not released while its chunk ticks (no DEMATERIALIZED)", demat.isEmpty());
                    check("still registered, so still leashed", bridge().body(agent[0]) == d[0] && !d[0].isRemoved());
                    unforce[0].run();
                    giveUp[0] = System.nanoTime() + 30_000_000_000L;
                })
                .thenWaitUntil(waits.chunks(() -> {
                    List<ScriptedEngine.Received> demat = about(DragonRecords.DRAGON_DEMATERIALIZED, agent[0]);
                    check("released as saved (reason 8) once its chunk stopped ticking", !demat.isEmpty()
                            && demat.get(demat.size() - 1).aux() == DragonRecords.Dematerialized.HOME_SAVED && bridge().body(agent[0]) == null);
                }))
                .thenExecute(() -> {
                    record(test, "released", "reason 8 after the body's chunk was let go; the body " + (d[0].isRemoved() ? "left the level ("
                            + d[0].getRemovalReason() + ")" : "stands"));
                    if (!d[0].isRemoved())
                        d[0].discard();
                })
                .thenSucceed();
    }

    /**
     * EQC314 (finding 5): DRAGON_DEPART reason 5 (the engine's dragon is gone: dead of age, starved away) to a home body with a
     * player in range: the plugin discards it at once. Kept for the capture (a player near holds it up to 6,000 ticks), a lair
     * going out in the window saved it with a dead dragon's agent, and the next census adopted it. The GameTest server's
     * simulation distance is 0 (its PlayerList never sets one), so the player is kept on the dragon each tick and the DEPART
     * targets the dragon's own place: only a player in the body's own chunk has it in range.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "home_depart_gone")
    public static void aHomeBodyToldItsDragonIsGoneIsDiscardedAtOnce(GameTestHelper helper) {
        String test = "aHomeBodyToldItsDragonIsGoneIsDiscardedAtOnce";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 2400);
        ServerLevel level = helper.getLevel();
        Runnable unforce = DragonKit.forceAround(helper, 3);
        long lair = DragonKit.newLair();
        DragonBaseEntity[] d = new DragonBaseEntity[1];
        long[] agent = new long[1], told = new long[1];
        ServerPlayer[] near = new ServerPlayer[1];
        SequenceHolder h = new SequenceHolder();
        h.s = helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> check("the ring of chunks is loaded", DragonKit.ready(level, arenaChunk(helper), 3))));
        occupiedAfter(h, helper, waits, lair, d, agent);
        h.s.thenExecute(() -> {
                    near[0] = FakePlayers.add(level, d[0].position(), "home-near");
                    told[0] = level.getGameTime();
                    DragonKit.depart(agent[0], homeEpoch(agent[0]), 5, 0, d[0].position());
                })
                .thenWaitUntil(waits.engine(() -> {
                    if (!d[0].isRemoved())
                        near[0].moveTo(d[0].getX(), d[0].getY(), d[0].getZ());
                    check("the engine passed the DEPART on and the plugin answered", bridge().body(agent[0]) == null
                            || level.getGameTime() - told[0] > 200);
                }))
                .thenExecute(() -> {
                    record(test, "after DEPART 5", "removed " + d[0].isRemoved() + ", registered " + (bridge().body(agent[0]) != null) + ", "
                            + (level.getGameTime() - told[0]) + " ticks after the order");
                    check("discarded at once, a player near", d[0].isRemoved() && bridge().body(agent[0]) == null);
                    bridge().homes.testOut(lair, level.getGameTime());
                    List<ScriptedEngine.Received> demat = about(DragonRecords.DRAGON_DEMATERIALIZED, agent[0]);
                    check("never released as saved (reason 8)", demat.stream().noneMatch(r -> r.aux() == DragonRecords.Dematerialized.HOME_SAVED));
                    FakePlayers.remove(near[0]);
                    if (!d[0].isRemoved())
                        d[0].discard();
                    unforce.run();
                })
                .thenSucceed();
    }

    /**
     * EQC317 (finding 8): with an oil field known to the engine, a fire dragon's home body adopted by its lair's census carries
     * its oil reserve (ADOPT flag 4), as a spawned body does, so its fire is gated (OG-4, OG-13).
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "home_oil")
    public static void aCensusAdoptedFireDragonCarriesItsOilReserve(GameTestHelper helper) {
        String test = "aCensusAdoptedFireDragonCarriesItsOilReserve";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 2400);
        ServerLevel level = helper.getLevel();
        long lair = DragonKit.newLair();
        DragonBaseEntity[] d = new DragonBaseEntity[1];
        long[] agent = new long[1];
        BlockPos far = DragonKit.farSite();
        helper.startSequence()
                .thenExecute(() -> DragonKit.oilChunks(level, List.of(new int[]{far.getX() >> 4, far.getZ() >> 4, 1},
                        new int[]{(far.getX() >> 4) + 1, far.getZ() >> 4, 1}, new int[]{far.getX() >> 4, (far.getZ() >> 4) + 1, 1})))
                .thenIdle(5)
                .thenExecute(() -> {
                    bridge().lairs.declareForTest(level, lair, ROOST, DragonKit.arenaBox(helper), BlockPos.containing(DragonKit.centre(helper)),
                            level.getGameTime());
                    d[0] = DragonKit.roostDragon(level, DragonKit.centre(helper));
                    bridge().homes.testIn(lair, false, level.getGameTime());
                })
                .thenWaitUntil(waits.engine(() -> {
                    long a = bridge().agentOf(d[0]);
                    check("the census adopted the lair's dragon as a home body", a != 0 && !homeAnswers(a).isEmpty());
                }))
                .thenExecute(() -> {
                    agent[0] = bridge().agentOf(d[0]);
                    record(test, "reserve", "the adopted home body's oil reserve " + bridge().oilOf(agent[0]) + " (-1: none, it breathes freely)");
                    check("it carries a reserve", bridge().oilOf(agent[0]) >= 0);
                    bridge().homes.testOut(lair, level.getGameTime());
                    d[0].discard();
                })
                .thenSucceed();
    }

    /**
     * EQC319 (finding 10): a chunk load or unload is queued for the end of the tick only when the chunk holds a known lair's
     * home; any other (every chunk of every dimension, with or without a session) costs nothing.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 100, batch = "home_chunk_events")
    public static void onlyAKnownLairsHomeChunkIsQueued(GameTestHelper helper) {
        String test = "onlyAKnownLairsHomeChunkIsQueued";
        ServerLevel level = helper.getLevel();
        long lair = DragonKit.newLair();
        BlockPos home = BlockPos.containing(DragonKit.centre(helper));
        bridge().lairs.declareForTest(level, lair, ROOST, DragonKit.arenaBox(helper), home, level.getGameTime());
        int before = bridge().homes.queuedChunkEvents();
        for (int i = 0; i < 1000; i++) {
            long chunk = ChunkPos.asLong(1_000_000 + i, -1_000_000);
            bridge().homes.chunkLoaded(level, chunk);
            bridge().homes.chunkUnloaded(level, chunk);
        }
        int strangers = bridge().homes.queuedChunkEvents() - before;
        bridge().homes.chunkLoaded(level, new ChunkPos(home).toLong());
        int own = bridge().homes.queuedChunkEvents() - before - strangers;
        record(test, "queued", "2,000 events of chunks no lair's home lies in: " + strangers + " queued; the lair's own home chunk: " + own);
        check("no stranger's chunk event is queued", strangers == 0);
        check("the lair's home chunk is", own == 1);
        helper.succeed();
    }

    /**
     * EQC321 (finding 12): a puppet settles at its own lair, nearer to a neighbouring lair's home: the hand-back names the
     * dragon's own lair (the engine's key), not the nearest one.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "home_hand_back_lair")
    public static void aHandBackNamesTheDragonsOwnLairNotTheNearest(GameTestHelper helper) {
        String test = "aHandBackNamesTheDragonsOwnLairNotTheNearest";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 2400);
        ServerLevel level = helper.getLevel();
        Vec3 c = DragonKit.centre(helper);
        lowerLeash(helper);
        Runnable unforce = DragonKit.forceAround(helper, 3);
        long lair = DragonKit.newLair(), neighbour = DragonKit.newLair();
        DragonBaseEntity[] d = new DragonBaseEntity[1];
        long[] agent = new long[1];
        int[] epoch = new int[1];
        SequenceHolder h = new SequenceHolder();
        h.s = helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> check("the ring of chunks is loaded", DragonKit.ready(level, arenaChunk(helper), 3))));
        occupiedAfter(h, helper, waits, lair, d, agent);
        leashed(h.s, helper, waits, agent, d, epoch, LURED)
                .thenExecute(() -> {
                    // A neighbouring lair whose home is 16 blocks east; the puppet lands 12 blocks east of its own: 4 from the
                    // neighbour's home, inside its own hand-back radius.
                    BlockPos nh = BlockPos.containing(c.add(16, 0, 0));
                    bridge().lairs.declareForTest(level, neighbour, ROOST, BoundingBox.fromCorners(nh.offset(-2, -2, -2), nh.offset(2, 4, 2)), nh,
                            level.getGameTime());
                    d[0].moveTo(c.x + 12, c.y, c.z, 0f, 0f);
                })
                .thenWaitUntil(waits.engine(() -> check("a second home answer: the puppet is a home body again", homeAnswers(agent[0]).size() >= 2)))
                .thenExecute(() -> {
                    HomeBodies.HomeTag tag = d[0].getExistingDataOrNull(MicRtsimIceAndFire.HOME);
                    record(test, "handed back", "the attachment " + tag + "; its own lair " + Long.toHexString(lair) + ", the neighbour "
                            + Long.toHexString(neighbour));
                    check("the home attachment names its own lair: " + tag, tag != null && tag.lair() == lair && tag.agent() == agent[0]);
                    bridge().homes.testOut(lair, level.getGameTime());
                    d[0].discard();
                    unforce.run();
                })
                .thenSucceed();
    }

    /** EQC322 (finding 13): the leash is at most the footprint, whatever is configured. */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 100, batch = "home_leash_clamp")
    public static void aLeashBeyondTheFootprintIsClampedToIt(GameTestHelper helper) {
        String test = "aLeashBeyondTheFootprintIsClampedToIt";
        DragonConfig.Test.homeLeash = 100;
        DragonKit.onEnd(helper, DragonConfig.Test::reset);
        record(test, "leash", "configured 100, footprint " + DragonConfig.lairFootprint() + ": the leash is " + DragonConfig.homeLeash());
        check("the leash is at most the footprint", DragonConfig.homeLeash() <= DragonConfig.lairFootprint());
        helper.succeed();
    }
}
