package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest;

import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthi;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictNews;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ChunkWait;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.CampLine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.MicRtsimMowziesMobs;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Far;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Grove;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.List;
import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The aloft chief's GameTests (MOWZIE.md ledger 24; docs/choices-mowzie-build.md, "M11 design for the implementer", MZB-837, and the
 * M11c build records, MZB-890 on): the Umvuthi's puppet that floats with its Invasion, its death, its summons, its homecoming and its
 * Rule U holds. Each test is its own batch on its own far base ({@link MowzieKit.Far}) and scripts the engine's side
 * ({@link ConflictKit}): the bands, marches, steers, recalls, reconciles and CHIEF_BODY are the test's own records, so none needs the
 * real engine. Every wait is on {@link ChunkWait}'s two clocks, every player and listener is taken out inside the test's sequence, and
 * every test logs what it compared on a {@code [gametest]} line.
 */
@GameTestHolder(MicRtsimMowziesMobs.MODID)
@PrefixGameTestTemplate(false)
public final class AloftMowzieGameTests {
    private AloftMowzieGameTests() {}

    private static final String ARENA = RtsimTests.TEMPLATE;

    private static final int DONE = ConflictRecords.CampBodies.DONE, STALE = ConflictRecords.CampBodies.STALE,
            NOTHING = ConflictRecords.CampBodies.NOTHING;
    private static final int ADOPT = ConflictRecords.CampReconcile.ADOPT, HAND_BACK = ConflictRecords.CampReconcile.HAND_BACK;
    private static final int CHIEF_SLOT = ConflictRecords.CampReconcile.CHIEF_SLOT, CHIEF = ConflictRecords.CampReconcile.LINE_CHIEF;
    private static final String UMVUTHI = MowzieKit.UMVUTHI, WARRIOR = MowzieKit.WARRIOR;

    // ---- helpers ----------------------------------------------------------------------------------

    /** A grove 20 chunks (320 blocks) from its neighbours in x and z, so no player of one is near another (the holds read 160). */
    private static Grove groveAt(Far far, int i, int j) {
        return new Grove(far, far.k + 20 * i, far.z0 + 20 * j);
    }

    /** A point {@code dx}, {@code dz} from the grove's centre, one block over the ground (for a player). */
    private static Vec3 off(Grove g, double dx, double dz) {
        return new Vec3(g.spot.x + dx, g.spot.y + 1, g.spot.z + dz);
    }

    private static long epochOf(Grove g) {
        return g.site().epoch();
    }

    /** ADOPT of a stray into the chief's slot as generation {@code gen}; the op. */
    private static long adoptChief(Grove g, net.minecraft.world.entity.Entity e, int gen) {
        return MowzieKit.reconcile(g.faction, epochOf(g), List.of(CampLine.of(CHIEF_SLOT, gen, UMVUTHI, ADOPT, CHIEF, e.getUUID())));
    }

    /** Inside {@code thenWaitUntil}: every grove has sent its first census. */
    private static Runnable censused(Grove... gs) {
        return () -> {
            for (Grove g : gs)
                check("the census of " + Long.toHexString(g.faction), MowzieKit.censusCount(g.faction) >= 1);
        };
    }

    /** Sol Visage item entities within {@code r} blocks of {@code c}, by count. */
    private static int visagesNear(ServerLevel level, Vec3 c, double r) {
        ResourceLocation sol = ResourceLocation.fromNamespaceAndPath("mowziesmobs", "sol_visage");
        int n = 0;
        for (ItemEntity e : level.getEntitiesOfClass(ItemEntity.class, new AABB(c.x - r, c.y - 40, c.z - r, c.x + r, c.y + 40, c.z + r)))
            if (sol.equals(BuiltInRegistries.ITEM.getKey(e.getItem().getItem())))
                n += e.getItem().getCount();
        return n;
    }

    /** The chief's slot of a party, alive or not (inside a wait: the party may not be there yet). */
    private static ConflictBridge.Slot chiefSlot(long agent) {
        ConflictBridge.Party p = ConflictKit.party(agent);
        for (ConflictBridge.Stack st : p.stacks())
            for (ConflictBridge.Slot s : st.slots())
                if (s.chief)
                    return s;
        throw new GameTestAssertException("the party of " + Long.toHexString(agent) + " has no chief slot");
    }

    /** Inside {@code thenWaitUntil}: the party's chief puppet stands in the world, an Umvuthi that floats. */
    private static EntityUmvuthi puppet(long agent) {
        ConflictBridge.Slot s = chiefSlot(agent);
        check("the chief's puppet is embodied (alive " + s.alive() + ", body " + s.entity() + ")", s.alive() && s.entity() != null
                && !s.entity().isRemoved());
        check("it is the jar's Umvuthi and a puppet", s.entity() instanceof EntityUmvuthi && RtsimApi.isPuppet(s.entity()));
        return (EntityUmvuthi) s.entity();
    }

    /**
     * An Invasion's band of a grove: CHIEF_BODY away and its MATERIALIZE in one hand-over (MZB-805), with {@code warriors} warriors and
     * the chief's stack, its head at ({@code x}, {@code z}) heading +x and standing still.
     */
    private static ConflictKit.Band chiefBand(GameTestHelper helper, Grove g, long agent, double x, double z, float health,
            List<UUID> traded, int warriors) {
        ConflictKit.Band b = ConflictKit.band(agent, g.faction, RtsimTests.dimension(helper));
        b.flags |= ConflictRecords.Materialize.FLAG_CHIEF;
        b.purpose = ConflictRecords.PURPOSE_INVASION;
        b.arcX = x;
        b.arcZ = z;
        b.bearing = 0f;
        b.targetX = x;
        b.targetY = g.spot.y;
        b.targetZ = z;
        b.speed = 0f;
        ConflictKit.together(() -> {
            ConflictKit.chiefBody(g.faction, agent, b.epoch, health, traded);
            ConflictKit.materialize(b, List.of(new ConflictKit.StackSpec(WARRIOR, warriors, false), new ConflictKit.StackSpec(UMVUTHI, 0, true)));
        });
        return b;
    }

    /** The answer to line 0 of an op, once it has come. */
    private static int answerOf(Grove g, long op) {
        return MowzieKit.answered(g.faction, op, 1).get(0);
    }

    /** The throne bodies that stand at a grove now: living Umvuthi that are no puppet. */
    private static List<EntityUmvuthi> thrones(ServerLevel level, Grove g) {
        return level.getEntitiesOfClass(EntityUmvuthi.class, g.area(), u -> !u.isDeadOrDying() && !u.isRemoved() && !RtsimApi.isPuppet(u));
    }

    /** The first tick a BAND_DEMATERIALIZED about an agent was seen, or -1. */
    private static List<ScriptedEngine.Received> dematerialized(long agent) {
        return ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, agent);
    }

    // ---- step 0: the M11b review's P2 and P3 (MZB-890, MZB-891) ---------------------------------------

    /**
     * A second puppet's death animation ends and journals nothing (MZB-890, the M11b review's P2). Grove S's Invasion floats its
     * chief's puppet while a throne body stands at the grove (the engine's own chief, which the held takes leave standing).
     * The throne body is killed by a player, so the puppet becomes a second body (MZB-825); then the player kills the puppet. Its
     * 115-tick animation ends with the jar removing the body as KILLED, which {@code left} read as a death the engine had not
     * heard of. Past the animation:
     * <ul>
     * <li>the throne body's CAMP_DEATH is the only booking: no CHIEF_KILLED for the party, {@code chiefsKilled} and
     * {@code chiefDeathsAfterEnd} unchanged, no news, no Sol Visage where the puppet fell (the throne body's is the one);</li>
     * <li>the slot is forgotten and nothing is dying;</li>
     * <li>a recall then ends the band: its BAND_DEMATERIALIZED goes only after the puppet is gone.</li>
     * </ul>
     * The control, grove C, is a puppet that is not second, killed the same way: one CHIEF_KILLED naming the player, one Sol Visage
     * where it fell, nothing left dying. Fails on the code before MZB-890: S's party journals a CHIEF_KILLED at the animation's end.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_second_puppet_death", skyAccess = true)
    public static void aSecondPuppetsAnimationEndJournalsNothing(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 4000);
        String test = "aSecondPuppetsAnimationEndJournalsNothing";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove[] g = {groveAt(far, 0, 0), groveAt(far, 1, 0)}; // S: the second puppet; C: the control
        long[] agent = {ConflictKit.newAgent(), ConflictKit.newAgent()};
        EntityUmvuthi[] throne = new EntityUmvuthi[1];
        EntityUmvuthi[] puppet = new EntityUmvuthi[2];
        ServerPlayer[] player = new ServerPlayer[2];
        Vec3[] fell = new Vec3[2];
        long[] op = new long[1];
        long[] before = new long[4]; // chiefsKilled, chiefDeathsAfterEnd, news, puppetsSecond
        long[] tick = {-1, -1}; // the first tick the puppet was seen gone, the first the BAND_DEMATERIALIZED was seen

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    for (Grove x : g)
                        x.force();
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    for (Grove x : g)
                        x.lay().declare();
                    throne[0] = MowzieKit.umvuthi(level, g[0].at(-20, 0));
                    for (int i = 0; i < 2; i++)
                        player[i] = MowzieKit.player(helper, off(g[i], 0, 200), "mowzie-second-puppet-" + i);
                })
                .thenWaitUntil(waits.logic(censused(g)))
                .thenExecute(() -> op[0] = adoptChief(g[0], throne[0], 1))
                .thenWaitUntil(waits.logic(() -> MowzieKit.answered(g[0].faction, op[0], 1)))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("ADOPT", ConflictKit.outcomes(g[0].faction, op[0]), DONE);
                    for (int i = 0; i < 2; i++)
                        chiefBand(helper, g[i], agent[i], g[i].spot.x + 25, g[i].spot.z + 10, 90f, List.of(), 2);
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < 2; i++) {
                        puppet[i] = puppet(agent[i]);
                        check("puppet " + i + " floats", puppet[i].isNoGravity());
                    }
                }))
                .thenExecute(() -> {
                    before[3] = ConflictKit.puppetsSecond();
                    throne[0].setLastHurtByPlayer(player[0]);
                    MowzieKit.kill(throne[0], level.damageSources().playerAttack(player[0]));
                })
                .thenWaitUntil(waits.logic(() -> {
                    check("S's throne body booked its death (" + ConflictKit.campDeaths(g[0].faction).size() + ")",
                            ConflictKit.campDeaths(g[0].faction).size() == 1);
                    check("the puppet was marked second (" + (ConflictKit.puppetsSecond() - before[3]) + ")", ConflictKit.puppetsSecond() == before[3] + 1);
                }))
                .thenExecute(() -> {
                    check("S's puppet is a second body", MowzieKit.camps().secondBody(puppet[0]));
                    check("C's puppet is not", !MowzieKit.camps().secondBody(puppet[1]));
                    before[0] = ConflictKit.bridge().chiefsKilled;
                    before[1] = ConflictKit.chiefDeathsAfterEnd();
                    before[2] = ConflictNews.posted();
                    fell[0] = puppet[0].position();
                    puppet[0].setLastHurtByPlayer(player[0]);
                    MowzieKit.kill(puppet[0], level.damageSources().playerAttack(player[0]));
                    MowzieKit.everyTick(helper, () -> {
                        long now = level.getGameTime();
                        if (tick[0] < 0 && puppet[0].isRemoved())
                            tick[0] = now;
                        if (tick[1] < 0 && !dematerialized(agent[0]).isEmpty())
                            tick[1] = now;
                    });
                })
                .thenWaitUntil(waits.logic(() -> check("S's puppet's animation has ended (removed " + puppet[0].isRemoved() + ")", puppet[0].isRemoved())))
                .thenIdle(10)
                .thenExecute(() -> {
                    check("no CHIEF_KILLED for S's party (" + ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[0]).size() + ")",
                            ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[0]).isEmpty());
                    check("chiefsKilled is unchanged (" + before[0] + " to " + ConflictKit.bridge().chiefsKilled + ")",
                            ConflictKit.bridge().chiefsKilled == before[0]);
                    check("chiefDeathsAfterEnd is unchanged (" + before[1] + " to " + ConflictKit.chiefDeathsAfterEnd() + ")",
                            ConflictKit.chiefDeathsAfterEnd() == before[1]);
                    check("no news (" + before[2] + " to " + ConflictNews.posted() + ")", ConflictNews.posted() == before[2]);
                    check("S's chief slot is forgotten", !chiefSlot(agent[0]).alive());
                    check("nothing is dying (" + ConflictKit.bridge().dyingBodies() + ")", ConflictKit.bridge().dyingBodies() == 0);
                    check("no Sol Visage where the puppet fell (" + visagesNear(level, fell[0], 6) + ")", visagesNear(level, fell[0], 6) == 0);
                    check("the throne body's death booked once and no more (" + ConflictKit.campDeaths(g[0].faction).size() + ")",
                            ConflictKit.campDeaths(g[0].faction).size() == 1);
                    check("the band still stands: no BAND_DEMATERIALIZED yet", dematerialized(agent[0]).isEmpty());
                    ConflictKit.recall(agent[0], ConflictKit.party(agent[0]).epoch, ConflictRecords.RECALL_DONE, 40, g[0].spot.x, g[0].spot.z);
                })
                .thenWaitUntil(waits.logic(() -> check("S's band went (" + dematerialized(agent[0]).size() + ")", dematerialized(agent[0]).size() == 1)))
                .thenExecute(() -> {
                    check("the puppet was gone (tick " + tick[0] + ") no later than the BAND_DEMATERIALIZED (tick " + tick[1] + ")",
                            tick[0] >= 0 && tick[0] <= tick[1]);
                    check("still no CHIEF_KILLED for S's party", ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[0]).isEmpty());
                    // The control: a puppet that is not second, killed the same way, journals its death.
                    before[0] = ConflictKit.bridge().chiefsKilled;
                    fell[1] = puppet[1].position();
                    puppet[1].setLastHurtByPlayer(player[1]);
                    MowzieKit.kill(puppet[1], level.damageSources().playerAttack(player[1]));
                })
                .thenWaitUntil(waits.logic(() -> check("C's puppet's animation has ended", puppet[1].isRemoved())))
                .thenIdle(10)
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> killed = ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[1]);
                    check("C's puppet journals one CHIEF_KILLED (" + killed.size() + ")", killed.size() == 1);
                    check("by a player", killed.get(0).aux() == ConflictRecords.BY_PLAYER);
                    check("naming the player", player[1].getUUID().equals(ConflictKit.uuidAt(killed.get(0), ConflictRecords.ChiefKilled.KILLER_MOST)));
                    check("chiefsKilled rose by one (" + before[0] + " to " + ConflictKit.bridge().chiefsKilled + ")",
                            ConflictKit.bridge().chiefsKilled == before[0] + 1);
                    check("one Sol Visage where it fell (" + visagesNear(level, fell[1], 6) + ")", visagesNear(level, fell[1], 6) == 1);
                    check("nothing is dying (" + ConflictKit.bridge().dyingBodies() + ")", ConflictKit.bridge().dyingBodies() == 0);
                    record(test, "second puppet", "S journaled no CHIEF_KILLED and dropped nothing; its band went at tick " + tick[1] + " after the puppet at "
                            + tick[0] + "; the control's puppet booked one");
                    for (int i = 0; i < 2; i++)
                        com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers.remove(player[i]);
                    far.releaseAll();
                })
                .thenSucceed();
    }

    /**
     * A HAND_BACK of a retired chief generation is stale, puppet or not (MZB-891, the M11b review's P3). Grove S's throne body is
     * killed while its Invasion's puppet floats (the generation is retired), then a player kills the puppet. The chief's HAND_BACK
     * of that generation, in flight across the CAMP_DEATH, is delivered twice:
     * <ul>
     * <li>while the puppet is in its death animation: no puppet is alive to find and nothing is waiting to be journaled, so the line
     * played as a spawn;</li>
     * <li>after the puppet is gone: no puppet at all.</li>
     * </ul>
     * Each is answered STALE and no throne body of the retired generation stands afterwards. The control, grove C, has no throne body
     * and a live puppet near its throne: its HAND_BACK floats the puppet to the spot and hands back (DONE, one Umvuthi on the
     * throne tagged as the chief). Fails on the code before MZB-891: S's line makes a throne body.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_retired_hand_back", skyAccess = true)
    public static void aHandBackOfARetiredChiefIsAnsweredStale(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 6000);
        String test = "aHandBackOfARetiredChiefIsAnsweredStale";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove[] g = {groveAt(far, 0, 0), groveAt(far, 1, 0)}; // S: the retired generation; C: the control
        long[] agent = {ConflictKit.newAgent(), ConflictKit.newAgent()};
        EntityUmvuthi[] throne = new EntityUmvuthi[1];
        EntityUmvuthi[] puppet = new EntityUmvuthi[2];
        ServerPlayer[] player = new ServerPlayer[2];
        long[] op = new long[4];

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    for (Grove x : g)
                        x.force();
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    for (Grove x : g)
                        x.lay().declare();
                    throne[0] = MowzieKit.umvuthi(level, g[0].at(-20, 0));
                    for (int i = 0; i < 2; i++)
                        player[i] = MowzieKit.player(helper, off(g[i], 0, 200), "mowzie-retired-" + i);
                })
                .thenWaitUntil(waits.logic(censused(g)))
                .thenExecute(() -> op[0] = adoptChief(g[0], throne[0], 1))
                .thenWaitUntil(waits.logic(() -> MowzieKit.answered(g[0].faction, op[0], 1)))
                .thenExecute(() -> {
                    chiefBand(helper, g[0], agent[0], g[0].spot.x + 25, g[0].spot.z + 10, 90f, List.of(), 2);
                    chiefBand(helper, g[1], agent[1], g[1].spot.x + 8, g[1].spot.z, 90f, List.of(), 1);
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < 2; i++) {
                        puppet[i] = puppet(agent[i]);
                        check("puppet " + i + " floats", puppet[i].isNoGravity());
                    }
                }))
                .thenExecute(() -> {
                    throne[0].setLastHurtByPlayer(player[0]);
                    MowzieKit.kill(throne[0], level.damageSources().playerAttack(player[0]));
                })
                .thenWaitUntil(waits.logic(() -> check("the throne body's death is booked and the puppet marked second",
                        ConflictKit.campDeaths(g[0].faction).size() == 1 && MowzieKit.camps().secondBody(puppet[0]))))
                .thenExecute(() -> {
                    puppet[0].setLastHurtByPlayer(player[0]);
                    MowzieKit.kill(puppet[0], level.damageSources().playerAttack(player[0]));
                })
                .thenIdle(20)
                .thenExecute(() -> {
                    check("the puppet is in its animation (dying " + puppet[0].isDeadOrDying() + ", removed " + puppet[0].isRemoved() + ")",
                            puppet[0].isDeadOrDying() && !puppet[0].isRemoved());
                    op[1] = MowzieKit.reconcile(g[0].faction, epochOf(g[0]), g[0].spot, 0f, List.of(
                            CampLine.party(CHIEF_SLOT, 1, UMVUTHI, HAND_BACK, CHIEF, agent[0], ConflictKit.party(agent[0]).epoch)));
                })
                .thenWaitUntil(waits.logic(() -> check("the line in the animation is answered", ConflictKit.outcomes(g[0].faction, op[1]).size() == 1)))
                .thenExecute(() -> {
                    check("answered STALE, not " + MowzieKit.outcomeName(answerOf(g[0], op[1])), answerOf(g[0], op[1]) == STALE);
                    check("no throne body stands (" + thrones(level, g[0]).size() + ")", thrones(level, g[0]).isEmpty());
                })
                .thenWaitUntil(waits.logic(() -> check("the puppet's animation has ended", puppet[0].isRemoved())))
                .thenExecute(() -> op[2] = MowzieKit.reconcile(g[0].faction, epochOf(g[0]), g[0].spot, 0f, List.of(
                        CampLine.party(CHIEF_SLOT, 1, UMVUTHI, HAND_BACK, CHIEF, agent[0], ConflictKit.party(agent[0]).epoch))))
                .thenWaitUntil(waits.logic(() -> check("the line after the puppet is answered", ConflictKit.outcomes(g[0].faction, op[2]).size() == 1)))
                .thenIdle(20)
                .thenExecute(() -> {
                    check("answered STALE, not " + MowzieKit.outcomeName(answerOf(g[0], op[2])), answerOf(g[0], op[2]) == STALE);
                    check("no throne body of the retired generation stands (" + thrones(level, g[0]).size() + ")", thrones(level, g[0]).isEmpty());
                    check("none is held for it", MowzieKit.held(g[0].faction, CHIEF_SLOT, 1).isEmpty());
                    // The control: a live puppet is handed back to its throne.
                    op[3] = MowzieKit.reconcile(g[1].faction, epochOf(g[1]), g[1].spot, 0f, List.of(
                            CampLine.party(CHIEF_SLOT, 1, UMVUTHI, HAND_BACK, CHIEF, agent[1], ConflictKit.party(agent[1]).epoch)));
                })
                .thenWaitUntil(waits.logic(() -> check("the control's line is answered", ConflictKit.outcomes(g[1].faction, op[3]).size() == 1)))
                .thenExecute(() -> {
                    check("answered DONE, not " + MowzieKit.outcomeName(answerOf(g[1], op[3])), answerOf(g[1], op[3]) == DONE);
                    check("one Umvuthi stands on C's throne, tagged as the chief (" + MowzieKit.held(g[1].faction, CHIEF_SLOT, 1).size() + ")",
                            MowzieKit.held(g[1].faction, CHIEF_SLOT, 1).size() == 1);
                    check("C's puppet is gone", puppet[1].isRemoved());
                    record(test, "retired hand-back", "S answered STALE twice (in the animation and after) and stood no body; C handed back");
                    for (int i = 0; i < 2; i++)
                        com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers.remove(player[i]);
                    far.releaseAll();
                })
                .thenSucceed();
    }
}
