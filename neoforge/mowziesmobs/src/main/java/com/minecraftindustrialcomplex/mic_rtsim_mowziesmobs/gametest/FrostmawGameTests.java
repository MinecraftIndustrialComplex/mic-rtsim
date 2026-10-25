package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest;

import com.bobmowzie.mowziesmobs.server.entity.frostmaw.EntityFrostmaw;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictNews;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ChunkWait;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.CampLine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers.Hearer;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.MicRtsimMowziesMobs;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Den;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Far;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Markers;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.contents.TranslatableContents;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.animal.Wolf;
import net.minecraft.world.entity.monster.Zombie;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.entity.projectile.Arrow;
import net.minecraft.world.entity.projectile.Snowball;
import net.minecraft.world.entity.projectile.ThrownTrident;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.GameType;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.List;
import java.util.UUID;
import java.util.function.Supplier;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The Frostmaw's den on the jar's own body (MOWZIE.md §4.7, §4.9, §7 tests 2, 6, 7 and 15;
 * docs/choices-mowzie-build.md, M6): a robbed Frostmaw kept in its den, what an arrow, a trident and a
 * bullet do to one, who can steal its crystal, and a chief killed at its camp reported with the one who gets
 * the credit. Every test works in a far place of its own (the world is reused), declares its camps under a
 * name no culture claims (MZB-227) and has a batch of its own, since the reconciler's counters are global.
 */
@GameTestHolder(MicRtsimMowziesMobs.MODID)
@PrefixGameTestTemplate(false)
public final class FrostmawGameTests {
    private FrostmawGameTests() {}

    private static final String ARENA = RtsimTests.TEMPLATE;

    private static final int DONE = ConflictRecords.CampBodies.DONE;
    private static final int SPAWN = ConflictRecords.CampReconcile.SPAWN, ADOPT = ConflictRecords.CampReconcile.ADOPT;
    private static final int CHIEF = ConflictRecords.CampReconcile.LINE_CHIEF;
    private static final int SET_OUT = ConflictRecords.CampReconcile.SET_OUT;

    /**
     * Inside {@code thenWaitUntil}: each entity object is saved and unloaded with its chunk, read at each poll
     * ({@link MowzieKit#removed}: under C2ME a null {@code getEntity} is not a save, MZB-662).
     */
    @SafeVarargs
    private static Runnable gone(ServerLevel level, Supplier<? extends Entity>... objects) {
        return MowzieKit.removed(level, objects);
    }

    /** Every CHIEF_STATE the engine has received, whatever the faction. */
    private static int chiefStateRecords() {
        int n = 0;
        for (ScriptedEngine.Received ignored : RtsimTests.engine().received(ConflictRecords.CHIEF_STATE))
            n++;
        return n;
    }

    // ---- 1: a robbed Frostmaw ---------------------------------------------------------------------

    /**
     * A Frostmaw robbed at its den stays the den's, and a reset does not give its crystal back (§7 test 2). A
     * den with its crystal Frostmaw adopted (persistence as the jar left it: none). A thief (survival,
     * invisible, MZB-281) is kept at the crystal's point: it ends with an ice crystal, the Frostmaw without
     * its one, awake, persistence-required (the mixin, MZB-276), and exactly one CHIEF_STATE, flags 0, with
     * the den's faction and the Frostmaw's health (MZB-273). The thief goes; the den's chunks are released
     * until the Frostmaw is out of memory and forced again: the one that loads (same UUID, another object) is
     * tagged slot 0, generation 1, persistent and crystal-less, there is still one CHIEF_STATE, and the
     * census that follows reads the chief without its crystal. A second den's scripted SPAWN with the chief
     * flag only is DONE with a crystal-less, persistence-required, sleeping Frostmaw.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "frostmaw_robbed", skyAccess = true)
    public static void aRobbedFrostmawIsKeptAndAResetDoesNotGiveItsCrystalBack(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 3000);
        String test = "aRobbedFrostmawIsKeptAndAResetDoesNotGiveItsCrystalBack";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Den den = new Den(far, far.k, far.z0);
        Den fresh = new Den(far, far.k + 6, far.z0);
        EntityFrostmaw[] f = new EntityFrostmaw[1];
        ServerPlayer[] thief = new ServerPlayer[1];
        UUID[] id = new UUID[1];
        long[] op = new long[2];
        long[] queued = new long[1];

        helper.startSequence()
                .thenExecute(() -> {
                    den.force();
                    fresh.force();
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    den.lay().declare();
                    fresh.lay().declare();
                    f[0] = MowzieKit.frostmaw(level, den.at(3, 0.5), true);
                    id[0] = f[0].getUUID();
                    check("the Frostmaw has its crystal", f[0].getHasCrystal());
                    check("and no persistence of its own", !f[0].isPersistenceRequired());
                })
                .thenWaitUntil(waits.logic(() -> {
                    check("the den's census", MowzieKit.censusCount(den.faction) >= 1);
                    check("the second den's census", MowzieKit.censusCount(fresh.faction) >= 1);
                }))
                .thenExecute(() -> op[0] = MowzieKit.reconcile(den.faction, MowzieKit.epoch(MowzieKit.census(den.faction, 1)),
                        List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, ADOPT, CHIEF, id[0]))))
                .thenWaitUntil(waits.logic(() -> MowzieKit.answered(den.faction, op[0], 1)))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("the ADOPT", ConflictKit.outcomes(den.faction, op[0]), DONE);
                    check("the Frostmaw is tagged (faction, 0, 1)", MowzieKit.tagged(f[0], den.faction, 0, 1));
                    check("and still not persistence-required (the jar's, MZB-210)", !f[0].isPersistenceRequired());
                    check("the den has had no CHIEF_STATE", ConflictKit.chiefStates(den.faction).isEmpty());
                    queued[0] = MowzieKit.camps().chiefStatesQueued;
                    thief[0] = MowzieKit.thief(helper, f[0], GameType.SURVIVAL, true, false);
                })
                .thenWaitUntil(waits.logic(() -> {
                    check("the thief holds the crystal (" + MowzieKit.crystals(thief[0]) + ")", MowzieKit.crystals(thief[0]) == 1);
                    check("the Frostmaw has lost it", !f[0].getHasCrystal());
                    check("a CHIEF_STATE", !ConflictKit.chiefStates(den.faction).isEmpty());
                }))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> cs = ConflictKit.chiefStates(den.faction);
                    check("exactly one CHIEF_STATE (" + cs.size() + ")", cs.size() == 1);
                    ScriptedEngine.Received c = cs.get(0);
                    check("flags 0: the crystal is gone (" + c.aux() + ")", c.aux() == 0);
                    check("for the den's faction", c.view().getLong(ConflictRecords.ChiefState.FACTION) == den.faction);
                    float h = c.view().getFloat(ConflictRecords.ChiefState.HEALTH);
                    check("with the Frostmaw's health (" + h + " against " + f[0].getHealth() + ")", Math.abs(h - f[0].getHealth()) <= 1f);
                    check("the session counted one queued (" + (MowzieKit.camps().chiefStatesQueued - queued[0]) + ")",
                            MowzieKit.camps().chiefStatesQueued - queued[0] == 1);
                    check("the Frostmaw is persistence-required now (MZB-276)", f[0].isPersistenceRequired());
                    check("awake", f[0].getActive());
                    check("still tagged (faction, 0, 1)", MowzieKit.tagged(f[0], den.faction, 0, 1));
                    FakePlayers.removeSeen(thief[0]);
                    den.release();
                })
                .thenWaitUntil(waits.logic(() -> check("CAMP_OUT", ConflictKit.campOuts(den.faction).size() == 1)))
                .thenWaitUntil(waits.chunks(gone(level, () -> f[0])))
                .thenExecute(() -> {
                    check("no second census yet", MowzieKit.censusCount(den.faction) == 1);
                    den.force();
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenWaitUntil(waits.logic(() -> check("census 2", MowzieKit.censusCount(den.faction) >= 2)))
                .thenExecute(() -> {
                    Entity e = level.getEntity(id[0]);
                    check("the Frostmaw is back", e instanceof EntityFrostmaw);
                    check("a new object: the old one was saved and unloaded, not tracked again (MZB-662)", e != f[0]);
                    EntityFrostmaw nf = (EntityFrostmaw) e;
                    check("tagged (faction, 0, 1)", MowzieKit.tagged(nf, den.faction, 0, 1));
                    check("persistence-required", nf.isPersistenceRequired());
                    check("without its crystal: the reset gave nothing back", !nf.getHasCrystal());
                    check("still one CHIEF_STATE (" + ConflictKit.chiefStates(den.faction).size() + ")",
                            ConflictKit.chiefStates(den.faction).size() == 1);
                    int c1 = MowzieKit.chiefFlags(MowzieKit.census(den.faction, 1)), c2 = MowzieKit.chiefFlags(MowzieKit.census(den.faction, 2));
                    check("census 1 read the crystal (" + c1 + ")", (c1 & ConflictRecords.CampCensus.CRYSTAL) != 0);
                    check("census 2 reads the chief without it (" + c2 + ")",
                            (c2 & ConflictRecords.CampCensus.CHIEF) != 0 && (c2 & ConflictRecords.CampCensus.CRYSTAL) == 0);
                    // The census's bodies carry the crystal each on its own (review C): the engine reads the kept chief
                    // body's bit, never the camp's byte.
                    ConflictKit.CensusBody b1 = MowzieKit.bodyOf(MowzieKit.census(den.faction, 1), id[0]);
                    ConflictKit.CensusBody b2 = MowzieKit.bodyOf(MowzieKit.census(den.faction, 2), id[0]);
                    check("census 1 lists the Frostmaw as a stray chief with its crystal", b1 != null && b1.flags() == (ConflictRecords.CampCensus.BODY_STRAY
                            | ConflictRecords.CampCensus.BODY_CHIEF | ConflictRecords.CampCensus.BODY_CRYSTAL));
                    check("census 2 lists it as the den's chief with no crystal bit", b2 != null && b2.flags() == ConflictRecords.CampCensus.BODY_CHIEF);
                    List<EntityFrostmaw> all = level.getEntitiesOfClass(EntityFrostmaw.class, den.area());
                    check("one Frostmaw at the den, crystal-less (" + all.size() + ")", all.size() == 1 && !all.get(0).getHasCrystal());
                    op[1] = MowzieKit.reconcile(fresh.faction, MowzieKit.epoch(MowzieKit.census(fresh.faction, 1)),
                            List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, SPAWN, CHIEF)));
                })
                .thenWaitUntil(waits.logic(() -> MowzieKit.answered(fresh.faction, op[1], 1)))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("the second den's SPAWN", ConflictKit.outcomes(fresh.faction, op[1]), DONE);
                    List<Entity> h = MowzieKit.held(fresh.faction, 0, 1);
                    check("a Frostmaw is the second den's (" + h.size() + ")", h.size() == 1 && h.get(0) instanceof EntityFrostmaw);
                    EntityFrostmaw nf = (EntityFrostmaw) h.get(0);
                    check("without a crystal", !nf.getHasCrystal());
                    check("persistence-required (MZB-210)", nf.isPersistenceRequired());
                    check("asleep", !nf.getActive());
                    record(test, "robbed", "one CHIEF_STATE (flags 0), the Frostmaw kept persistent across a reset with no crystal; a chief-only SPAWN came without one");
                })
                .thenSucceed();
    }

    // ---- 2: arrows ------------------------------------------------------------------------------

    /**
     * What an arrow, a trident and a bullet do to a sleeping Frostmaw (§7 test 6). An untagged Frostmaw and
     * a player 80 blocks off (beyond its follow range, so that it has no target of its own). An arrow as the
     * direct entity: {@code hurt} is false, the health is the same, the player is its target and it is awake. A trident (an {@code AbstractArrow} too): false, the health the same. A
     * snowball, a {@code Projectile} that is no arrow (TaCZ's bullet is one, MZB-282), last, since a hit that
     * lands makes it invulnerable for 10 ticks: true, and the health falls.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "frostmaw_arrows", skyAccess = true)
    public static void anArrowAndATridentDoNothingAndABulletHurtsAFrostmaw(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 1200);
        String test = "anArrowAndATridentDoNothingAndABulletHurtsAFrostmaw";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        int cx = far.k, cz = far.z0;
        EntityFrostmaw[] f = new EntityFrostmaw[1];
        ServerPlayer[] player = new ServerPlayer[1];

        helper.startSequence()
                .thenExecute(() -> far.forceRect(cx - 1, cz - 1, cx + 1, cz + 1))
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    int top = MowzieKit.platform(level, (cx - 1) * 16, (cx + 1) * 16 + 15, (cz - 1) * 16, (cz + 1) * 16 + 15);
                    f[0] = MowzieKit.frostmaw(level, new Vec3(cx * 16 + 8.5, top + 1, cz * 16 + 8.5), true);
                    // beyond the Frostmaw's follow range (50): a nearer survival player would be its target already
                    player[0] = FakePlayers.add(level, f[0].position().add(80, 0, 0), "mowzie-archer");
                    ConflictKit.onEnd(helper, () -> FakePlayers.remove(player[0]));
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    EntityFrostmaw m = f[0];
                    ServerPlayer p = player[0];
                    check("asleep, with no target", !m.getActive() && m.getTarget() == null);
                    float h0 = m.getHealth();

                    Arrow arrow = new Arrow(level, p, new ItemStack(Items.ARROW), null);
                    boolean a = m.hurt(level.damageSources().arrow(arrow, p), 10f);
                    check("an arrow does not hurt (" + a + ")", !a);
                    check("the health is the same (" + h0 + ", now " + m.getHealth() + ")", m.getHealth() == h0);
                    check("the archer is its target", m.getTarget() == p);
                    check("it is awake", m.getActive());

                    ThrownTrident trident = new ThrownTrident(level, p, new ItemStack(Items.TRIDENT));
                    boolean t = m.hurt(level.damageSources().trident(trident, p), 10f);
                    check("a trident does not hurt (" + t + ")", !t);
                    check("the health is the same (" + h0 + ", now " + m.getHealth() + ")", m.getHealth() == h0);

                    Snowball ball = new Snowball(level, p);
                    boolean s = m.hurt(level.damageSources().thrown(ball, p), 10f); // the projectile goes first, despite the names
                    check("a bullet that is no arrow hurts (" + s + ")", s);
                    check("the health has fallen (" + h0 + ", now " + m.getHealth() + ")", m.getHealth() < h0);
                    record(test, "arrows", "arrow and trident: false, " + h0 + " kept; snowball: true, " + m.getHealth());
                })
                .thenSucceed();
    }

    // ---- 3: thieves -----------------------------------------------------------------------------

    /**
     * Who can steal a sleeping Frostmaw's crystal (§7 test 7). Five untagged Frostmaws 48 blocks apart, a thief
     * (MZB-281) at each crystal point. An invisible survival player takes it: the crystal is in its
     * inventory, the Frostmaw is without one and awake, still not persistence-required (it is no camp's,
     * MZB-277), and no CHIEF_STATE is sent. A visible creative player takes it too. After 100 more ticks an
     * invisible player whose 36 main slots each hold something, a visible survival player and an invisible
     * spectator have taken nothing, and each of those Frostmaws still has its crystal. The thieves are server
     * players in the entity manager and in {@code level.players()} (review C), so the goals see them as they
     * see a real player: only the spectator's Frostmaw is asserted asleep, and whether the other two woke is
     * recorded.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "frostmaw_thieves", skyAccess = true)
    public static void anInvisiblePlayerStealsTheCrystalFromASleepingFrostmaw(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 1500);
        String test = "anInvisiblePlayerStealsTheCrystalFromASleepingFrostmaw";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        int n = 5, cz = far.z0;
        int[] cx = new int[n];
        for (int i = 0; i < n; i++)
            cx[i] = far.k + 3 * i;
        EntityFrostmaw[] f = new EntityFrostmaw[n];
        ServerPlayer[] thief = new ServerPlayer[n];
        boolean[] persistent = new boolean[n];
        int[] states = new int[1];
        long[] queued = new long[2];

        helper.startSequence()
                .thenExecute(() -> {
                    for (int i = 0; i < n; i++)
                        far.forceRect(cx[i] - 1, cz - 1, cx[i] + 1, cz + 1);
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    int top = MowzieKit.platform(level, (cx[0] - 1) * 16, (cx[n - 1] + 1) * 16 + 15, (cz - 1) * 16, (cz + 1) * 16 + 15);
                    for (int i = 0; i < n; i++) {
                        f[i] = MowzieKit.frostmaw(level, new Vec3(cx[i] * 16 + 8.5, top + 1, cz * 16 + 8.5), true);
                        persistent[i] = f[i].isPersistenceRequired();
                    }
                    states[0] = chiefStateRecords();
                    queued[0] = MowzieKit.camps().chiefStatesQueued;
                    queued[1] = MowzieKit.camps().chiefStates;
                    thief[0] = MowzieKit.thief(helper, f[0], GameType.SURVIVAL, true, false);
                    thief[1] = MowzieKit.thief(helper, f[1], GameType.CREATIVE, false, false);
                    thief[2] = MowzieKit.thief(helper, f[2], GameType.SURVIVAL, true, true);
                    thief[3] = MowzieKit.thief(helper, f[3], GameType.SURVIVAL, false, false);
                    thief[4] = MowzieKit.thief(helper, f[4], GameType.SPECTATOR, true, false);
                })
                .thenWaitUntil(waits.logic(() -> {
                    check("the invisible survival thief holds a crystal (" + MowzieKit.crystals(thief[0]) + ")", MowzieKit.crystals(thief[0]) == 1);
                    check("the visible creative thief holds a crystal (" + MowzieKit.crystals(thief[1]) + ")", MowzieKit.crystals(thief[1]) == 1);
                    check("the first Frostmaw has lost its crystal", !f[0].getHasCrystal());
                    check("the second Frostmaw has lost its crystal", !f[1].getHasCrystal());
                }))
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++) {
                        check("Frostmaw " + i + " is awake", f[i].getActive());
                        check("Frostmaw " + i + " is persistence-required as the jar left it, so no camp's (" + f[i].isPersistenceRequired() + ")",
                                f[i].isPersistenceRequired() == persistent[i]);
                    }
                })
                .thenIdle(100)
                .thenExecute(() -> {
                    String[] who = {"", "", "an invisible player with a full inventory", "a visible survival player", "an invisible spectator"};
                    for (int i = 2; i < n; i++) {
                        check(who[i] + " steals nothing (" + MowzieKit.crystals(thief[i]) + ")", MowzieKit.crystals(thief[i]) == 0);
                        check("Frostmaw " + i + " keeps its crystal", f[i].getHasCrystal());
                    }
                    // The thieves are players the goals see (review C). A spectator is no target and no entity of the jar's
                    // search (NO_SPECTATORS), so only its Frostmaw is certain to sleep on; the visible survival player is
                    // a target within 8 blocks, which wakes a sleeper, and the invisible one has its target dropped each
                    // tick: whether those two woke is recorded, not asserted (the mock player of M6 made all three sleep
                    // because no goal could see it).
                    check("the spectator's Frostmaw sleeps on", !f[4].getActive());
                    check("no CHIEF_STATE was sent (" + (chiefStateRecords() - states[0]) + ")", chiefStateRecords() == states[0]);
                    check("none was queued (" + (MowzieKit.camps().chiefStatesQueued - queued[0]) + ")", MowzieKit.camps().chiefStatesQueued == queued[0]);
                    check("none was written (" + (MowzieKit.camps().chiefStates - queued[1]) + ")", MowzieKit.camps().chiefStates == queued[1]);
                    record(test, "thieves", "invisible survival and visible creative stole; a full inventory, a visible survival player and a spectator did not; no CHIEF_STATE for strays; awake after 100 ticks: full inventory "
                            + f[2].getActive() + ", visible survival " + f[3].getActive() + ", spectator " + f[4].getActive());
                })
                .thenSucceed();
    }

    // ---- 4: a chief killed at its camp -------------------------------------------------------------

    /**
     * A Frostmaw killed in its den is booked with the kill credit (§7 test 15, the Frostmaw half; MZB-261,
     * 275). Five dens, each with its Frostmaw adopted, and a listening player 24 blocks from the first. A
     * kill by that player: CAMP_DEATH by a player, his UUID, flagged chief, slot 0, generation 1, a
     * Frostmaw, a Java op id; a record marker where it fell; the tag gone; the body out of the level 70 to 80
     * ticks later (74); the news hook reached once. A tame wolf of his killing the second: by a player, him.
     * His hit of 10 on the third and a generic kill 40 ticks later: by a player, him. A zombie's attack on the
     * fourth: by a mob, no killer. A generic kill of the fifth with no hit before: by none. Nothing of it
     * reaches the player's chat, action bar or talk.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "frostmaw_kill", skyAccess = true)
    public static void aChiefKilledAtTheCampIsReportedWithTheKiller(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 3500);
        String test = "aChiefKilledAtTheCampIsReportedWithTheKiller";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        int n = 5;
        Den[] den = new Den[n];
        for (int i = 0; i < n; i++)
            den[i] = new Den(far, far.k + 4 * i, far.z0);
        EntityFrostmaw[] f = new EntityFrostmaw[n];
        Hearer[] hearer = new Hearer[1];
        Markers[] markers = new Markers[1];
        long[] op = new long[n];
        long[] news = new long[1];
        long[] died = new long[1], removed = new long[1];
        Vec3[] fell = new Vec3[1];
        int[] heard = new int[3];

        helper.startSequence()
                .thenExecute(() -> {
                    for (Den d : den)
                        d.force();
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    for (int i = 0; i < n; i++) {
                        den[i].lay().declare();
                        f[i] = MowzieKit.frostmaw(level, den[i].at(3, 0.5), true);
                    }
                    markers[0] = new Markers(helper, level, den[0].area());
                    hearer[0] = FakePlayers.hearing(level, den[0].spot.add(24, 1, 0), "mowzie-hearer");
                    ConflictKit.onEnd(helper, () -> FakePlayers.remove(hearer[0]));
                    MowzieKit.everyTick(helper, () -> {
                        if (removed[0] == 0 && f[0].isRemoved())
                            removed[0] = level.getGameTime();
                    });
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < n; i++)
                        check("den " + i + "'s census", MowzieKit.censusCount(den[i].faction) >= 1);
                }))
                .thenExecute(() -> {
                    for (int i = 0; i < n; i++)
                        op[i] = MowzieKit.reconcile(den[i].faction, MowzieKit.epoch(MowzieKit.census(den[i].faction, 1)),
                                List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, ADOPT, CHIEF, f[i].getUUID())));
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < n; i++)
                        MowzieKit.answered(den[i].faction, op[i], 1);
                }))
                .thenExecute(() -> {
                    for (int i = 0; i < n; i++) {
                        check("Frostmaw " + i + " is tagged (faction, 0, 1)", MowzieKit.tagged(f[i], den[i].faction, 0, 1));
                        check("den " + i + " has booked no death", ConflictKit.campDeaths(den[i].faction).isEmpty());
                    }
                    news[0] = ConflictNews.posted();
                    heard[0] = hearer[0].raw.size();
                    heard[1] = hearer[0].overlay.size();
                    heard[2] = hearer[0].heard.size();
                    fell[0] = f[0].position();
                    died[0] = level.getGameTime();
                    MowzieKit.kill(f[0], level.damageSources().playerAttack(hearer[0]));
                })
                .thenWaitUntil(waits.logic(() -> check("CAMP_DEATH of den 0", ConflictKit.campDeaths(den[0].faction).size() == 1)))
                .thenExecute(() -> {
                    chiefDeath("the player's kill", ConflictKit.campDeaths(den[0].faction).get(0), ConflictRecords.BY_PLAYER, hearer[0].getUUID());
                    List<Markers.Seen> m = markers[0].near(fell[0], 4);
                    check("a record marker where it fell (" + m.size() + ")", !m.isEmpty());
                    check("it is tagged and carries the death", m.get(0).tagged() && m.get(0).carriesRecords());
                    check("the tag is off the corpse", MowzieKit.tag(f[0]) == null);
                    check("the news hook was reached once (" + (ConflictNews.posted() - news[0]) + ")", ConflictNews.posted() == news[0] + 1);
                    for (int i = 1; i < n; i++)
                        check("den " + i + " has booked nothing", ConflictKit.campDeaths(den[i].faction).isEmpty());
                })
                .thenWaitUntil(waits.logic(() -> check("the body is out of the level", removed[0] != 0)))
                .thenExecute(() -> {
                    long after = removed[0] - died[0];
                    check("removed 70 to 80 ticks after the death (" + after + ")", after >= 70 && after <= 80);
                    check("and no second death was booked for it", ConflictKit.campDeaths(den[0].faction).size() == 1);
                    // a tame wolf of the player's kills the second
                    Wolf wolf = EntityType.WOLF.create(level);
                    check("a wolf is made", wolf != null);
                    wolf.moveTo(f[1].position().add(-2, 0, 0));
                    wolf.setTame(true, true);
                    wolf.setOwnerUUID(hearer[0].getUUID());
                    check("the wolf's owner is the player", wolf.getOwner() == hearer[0]);
                    MowzieKit.kill(f[1], level.damageSources().mobAttack(wolf));
                })
                .thenWaitUntil(waits.logic(() -> check("CAMP_DEATH of den 1", ConflictKit.campDeaths(den[1].faction).size() == 1)))
                .thenExecute(() -> {
                    chiefDeath("the wolf's kill", ConflictKit.campDeaths(den[1].faction).get(0), ConflictRecords.BY_PLAYER, hearer[0].getUUID());
                    check("the news hook was reached twice (" + (ConflictNews.posted() - news[0]) + ")", ConflictNews.posted() == news[0] + 2);
                    // a hit of 10, and a generic kill 40 ticks later
                    boolean hit = f[2].hurt(level.damageSources().playerAttack(hearer[0]), 10f);
                    check("the player's hit lands (" + hit + ")", hit);
                    check("den 2 has booked nothing yet", ConflictKit.campDeaths(den[2].faction).isEmpty());
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    check("the Frostmaw lives after the hit", f[2].isAlive() && ConflictKit.campDeaths(den[2].faction).isEmpty());
                    MowzieKit.kill(f[2], level.damageSources().genericKill());
                })
                .thenWaitUntil(waits.logic(() -> check("CAMP_DEATH of den 2", ConflictKit.campDeaths(den[2].faction).size() == 1)))
                .thenExecute(() -> {
                    chiefDeath("the generic kill after the hit", ConflictKit.campDeaths(den[2].faction).get(0), ConflictRecords.BY_PLAYER, hearer[0].getUUID());
                    check("the news hook was reached three times (" + (ConflictNews.posted() - news[0]) + ")", ConflictNews.posted() == news[0] + 3);
                    // a zombie's attack on the fourth
                    Zombie zombie = EntityType.ZOMBIE.create(level);
                    check("a zombie is made", zombie != null);
                    zombie.moveTo(f[3].position().add(-2, 0, 0));
                    MowzieKit.kill(f[3], level.damageSources().mobAttack(zombie));
                })
                .thenWaitUntil(waits.logic(() -> check("CAMP_DEATH of den 3", ConflictKit.campDeaths(den[3].faction).size() == 1)))
                .thenExecute(() -> {
                    chiefDeath("the zombie's kill", ConflictKit.campDeaths(den[3].faction).get(0), ConflictRecords.BY_MOB, null);
                    check("the news hook was reached four times (" + (ConflictNews.posted() - news[0]) + ")", ConflictNews.posted() == news[0] + 4);
                    // a generic kill with no hit before
                    MowzieKit.kill(f[4], level.damageSources().genericKill());
                })
                .thenWaitUntil(waits.logic(() -> check("CAMP_DEATH of den 4", ConflictKit.campDeaths(den[4].faction).size() == 1)))
                .thenExecute(() -> {
                    chiefDeath("the generic kill with no hit", ConflictKit.campDeaths(den[4].faction).get(0), ConflictRecords.BY_NONE, null);
                    check("the news hook was reached five times (" + (ConflictNews.posted() - news[0]) + ")", ConflictNews.posted() == news[0] + 5);
                    Hearer h = hearer[0];
                    check("no talk line reached the player (" + (h.heard.size() - heard[2]) + ")", h.heard.size() == heard[2]);
                    check("nothing reached the action bar (" + (h.overlay.size() - heard[1]) + ")", h.overlay.size() == heard[1]);
                    for (int i = heard[0]; i < h.raw.size(); i++) {
                        Component c = h.raw.get(i);
                        check("no line of rtsim's reached the chat (" + c.getString() + ")",
                                !(c.getContents() instanceof TranslatableContents t && t.getKey().startsWith("mic_rtsim.")));
                    }
                    record(test, "kills", "player, wolf of the player and a generic kill after his hit: by a player; zombie: by a mob; generic: by none; the body went "
                            + (removed[0] - died[0]) + " ticks after the death; " + (h.raw.size() - heard[0]) + " chat lines came, none rtsim's");
                })
                .thenSucceed();
    }

    // ---- 5: a Frostmaw raid ----------------------------------------------------------------------

    /**
     * A Frostmaw raid embodies one Frostmaw, the den's (review C, MZB-109, MZB-127). An adopted den Frostmaw
     * with its crystal; in one hand-over a SET_OUT of its slot and a BAND_MATERIALIZE of one stack,
     * {@code StackSpec(FROSTMAW, 0, chief)}, as the engine sends a boss's raid: the stack is the chief alone.
     * The SET_OUT is DONE, one body is handed off, and the band is one slot (the chief) whose puppet is the
     * only Frostmaw within 64 blocks of the den: the den's body is its puppet, with its crystal, and the den
     * holds nothing. A stack of count 1 with the chief flag made two (the den's, and a fresh one with a
     * crystal for the extra chief slot).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "frostmaw_raid_one", skyAccess = true)
    public static void aFrostmawRaidMadeFromTheDensBodyEmbodiesOneFrostmaw(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 2000);
        String test = "aFrostmawRaidMadeFromTheDensBodyEmbodiesOneFrostmaw";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Den den = new Den(far, far.k, far.z0);
        EntityFrostmaw[] f = new EntityFrostmaw[1];
        long[] op = new long[2], agent = new long[1], was = new long[1];

        helper.startSequence()
                .thenExecute(den::force)
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    den.lay().declare();
                    f[0] = MowzieKit.frostmaw(level, den.at(3, 0.5), true);
                })
                .thenWaitUntil(waits.logic(() -> check("census 1", MowzieKit.censusCount(den.faction) >= 1)))
                .thenExecute(() -> op[0] = MowzieKit.reconcile(den.faction, MowzieKit.epoch(MowzieKit.census(den.faction, 1)),
                        List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, ADOPT, CHIEF, f[0].getUUID()))))
                .thenWaitUntil(waits.logic(() -> MowzieKit.answered(den.faction, op[0], 1)))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("ADOPT", ConflictKit.outcomes(den.faction, op[0]), DONE);
                    was[0] = MowzieKit.camps().handedOff;
                    agent[0] = ConflictKit.newAgent();
                    ConflictKit.Band b = ConflictKit.band(agent[0], den.faction, RtsimTests.dimension(helper));
                    b.arcX = den.spot.x;
                    b.arcZ = den.spot.z;
                    b.targetX = den.spot.x + 1000;
                    b.targetY = den.spot.y;
                    b.targetZ = den.spot.z;
                    b.speed = 0f;
                    b.flags = ConflictRecords.Materialize.FLAG_HOSTILE | ConflictRecords.Materialize.FLAG_CHIEF;
                    ConflictKit.together(() -> {
                        op[1] = MowzieKit.reconcile(den.faction, den.site().epoch(), List.of(
                                CampLine.party(0, 1, MowzieKit.FROSTMAW, SET_OUT, CHIEF, agent[0], b.epoch)));
                        ConflictKit.materialize(b, List.of(new ConflictKit.StackSpec(MowzieKit.FROSTMAW, 0, true)));
                    });
                })
                .thenWaitUntil(waits.logic(() -> {
                    MowzieKit.answered(den.faction, op[1], 1);
                    check("the band is placed", !ConflictKit.party(agent[0]).stacks().isEmpty()
                            && ConflictKit.party(agent[0]).stacks().get(0).state() == ConflictBridge.StackState.PLACED);
                }))
                .thenIdle(40)
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("SET_OUT", ConflictKit.outcomes(den.faction, op[1]), DONE);
                    check("one body handed off (" + (MowzieKit.camps().handedOff - was[0]) + ")", MowzieKit.camps().handedOff - was[0] == 1);
                    ConflictBridge.Party party = ConflictKit.party(agent[0]);
                    check("one stack", party.stacks().size() == 1);
                    List<ConflictBridge.Slot> slots = party.stacks().get(0).slots();
                    check("of one slot, the chief (" + slots.size() + ")", slots.size() == 1 && slots.get(0).chief);
                    Mob p = slots.get(0).entity();
                    check("a Frostmaw puppet", p instanceof EntityFrostmaw && RtsimApi.isPuppet(p));
                    List<EntityFrostmaw> all = level.getEntitiesOfClass(EntityFrostmaw.class, den.area().inflate(64));
                    check("the only Frostmaw about (" + all.size() + ")", all.size() == 1 && all.get(0) == p);
                    check("with the crystal the den's body had", ((EntityFrostmaw) p).getHasCrystal());
                    check("the den holds nothing now", MowzieKit.held(den.faction).isEmpty());
                    record(test, "one", "a count-0 chief stack made one Frostmaw, the den's, handed to its puppet");
                })
                .thenSucceed();
    }

    /**
     * A robbed den's raid comes without the crystal (review C). A band made fresh, with no den body to hand off
     * (a raid seen by a player while its den is out): a count-0 chief stack of a Frostmaw flagged bare is a
     * Frostmaw with no crystal (the plugin's {@code registerBare}), and the same stack without the flag, beside
     * it, is a fresh one with its crystal, since the jar's {@code finalizeSpawn} gives every new Frostmaw one.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "frostmaw_raid_bare", skyAccess = true)
    public static void anUnwatchedRaidOfARobbedDenPlacesAFrostmawWithoutItsCrystal(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 1500);
        String test = "anUnwatchedRaidOfARobbedDenPlacesAFrostmawWithoutItsCrystal";
        Far far = new Far(helper, test);
        int[] cx = {far.k, far.k + 4};
        long[] agent = new long[2];

        helper.startSequence()
                .thenExecute(() -> {
                    for (int x : cx)
                        far.forceRect(x - 1, far.z0 - 1, x + 1, far.z0 + 1);
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++) {
                        agent[i] = ConflictKit.newAgent();
                        ConflictKit.Band b = ConflictKit.band(agent[i], ConflictKit.newFaction(), RtsimTests.dimension(helper));
                        b.arcX = cx[i] * 16 + 8.5;
                        b.arcZ = far.z0 * 16 + 8.5;
                        b.targetX = b.arcX + 1000;
                        b.targetZ = b.arcZ;
                        b.speed = 0f;
                        b.flags = ConflictRecords.Materialize.FLAG_HOSTILE | ConflictRecords.Materialize.FLAG_CHIEF;
                        ConflictKit.materialize(b, List.of(new ConflictKit.StackSpec(MowzieKit.FROSTMAW, 0, true, i == 0)));
                    }
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < 2; i++)
                        check("band " + i + " is placed", !ConflictKit.party(agent[i]).stacks().isEmpty()
                                && ConflictKit.party(agent[i]).stacks().get(0).state() == ConflictBridge.StackState.PLACED);
                }))
                .thenExecute(() -> {
                    Mob[] p = new Mob[2];
                    for (int i = 0; i < 2; i++) {
                        List<ConflictBridge.Slot> slots = ConflictKit.party(agent[i]).stacks().get(0).slots();
                        check("band " + i + " is one chief body (" + slots.size() + ")", slots.size() == 1 && slots.get(0).chief);
                        p[i] = slots.get(0).entity();
                        check("band " + i + "'s body is a Frostmaw puppet", p[i] instanceof EntityFrostmaw && RtsimApi.isPuppet(p[i]));
                    }
                    check("the bare one has no crystal", !((EntityFrostmaw) p[0]).getHasCrystal());
                    check("the one without the flag is a fresh Frostmaw with its crystal", ((EntityFrostmaw) p[1]).getHasCrystal());
                    record(test, "bare", "a bare chief stack: no crystal; the same stack unflagged: crystal");
                })
                .thenSucceed();
    }

    /**
     * A chief's CAMP_DEATH: who it is by, the killer's UUID (null for none), flagged chief, a Frostmaw, slot 0,
     * generation 1, with a Java op id.
     */
    private static void chiefDeath(String what, ScriptedEngine.Received d, int by, UUID killer) {
        check(what + ": by " + by + ", not " + d.aux(), d.aux() == by);
        UUID got = ConflictKit.uuidAt(d, ConflictRecords.CampDeath.KILLER_MOST);
        check(what + ": the killer is " + killer + ", not " + got, killer == null ? got == null : killer.equals(got));
        check(what + ": flagged chief", (d.view().getInt(ConflictRecords.CampDeath.FLAGS) & ConflictRecords.CampDeath.FLAG_CHIEF) != 0);
        check(what + ": a Frostmaw", d.view().getInt(ConflictRecords.CampDeath.BODY) == ConflictKit.bodyId(MowzieKit.FROSTMAW));
        check(what + ": slot 0, generation 1", d.view().getInt(ConflictRecords.CampDeath.SLOT) == 0
                && d.view().getInt(ConflictRecords.CampDeath.GENERATION) == 1);
        check(what + ": with a Java op id", (d.view().getLong(ConflictRecords.CampDeath.OP_ID) & Obs.JAVA_OP_BIT) != 0);
    }
}
