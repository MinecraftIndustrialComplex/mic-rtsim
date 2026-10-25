package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest;

import com.bobmowzie.mowziesmobs.server.entity.frostmaw.EntityFrostmaw;
import com.ilexiconn.llibrary.server.animation.IAnimatedEntity;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.api.TakeReason;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.BridgeEvents;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictNews;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.CampLine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers.Hearer;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.FrostmawLurker;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.MicRtsimMowziesMobs;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Den;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Far;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Markers;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestSequence;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.Tag;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.contents.TranslatableContents;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.util.Mth;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.animal.Wolf;
import net.minecraft.world.entity.monster.Zombie;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.entity.projectile.Arrow;
import net.minecraft.world.entity.projectile.Snowball;
import net.minecraft.world.entity.projectile.ThrownTrident;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.pathfinder.Path;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.List;
import java.util.UUID;
import java.util.stream.Stream;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.io;
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

    /** Inside {@code thenWaitUntil}: the entity is out of the level (its chunk is unloaded). */
    private static Runnable gone(ServerLevel level, UUID... ids) {
        return io(() -> {
            for (UUID id : ids)
                check("entity " + id + " is unloaded", level.getEntity(id) == null);
        });
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
    @GameTest(template = ARENA, timeoutTicks = 3000, batch = "frostmaw_robbed", skyAccess = true)
    public static void aRobbedFrostmawIsKeptAndAResetDoesNotGiveItsCrystalBack(GameTestHelper helper) {
        String test = "aRobbedFrostmawIsKeptAndAResetDoesNotGiveItsCrystalBack";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Den den = new Den(far, far.k, far.z0);
        Den fresh = new Den(far, far.k + 4, far.z0);
        EntityFrostmaw[] f = new EntityFrostmaw[1];
        Player[] thief = new Player[1];
        UUID[] id = new UUID[1];
        long[] op = new long[2];
        long[] queued = new long[1];

        helper.startSequence()
                .thenExecute(() -> {
                    den.force();
                    fresh.force();
                })
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    den.lay().declare();
                    fresh.lay().declare();
                    f[0] = MowzieKit.frostmaw(level, den.at(3, 0.5), true);
                    id[0] = f[0].getUUID();
                    check("the Frostmaw has its crystal", f[0].getHasCrystal());
                    check("and no persistence of its own", !f[0].isPersistenceRequired());
                })
                .thenWaitUntil(() -> {
                    check("the den's census", MowzieKit.censusCount(den.faction) >= 1);
                    check("the second den's census", MowzieKit.censusCount(fresh.faction) >= 1);
                })
                .thenExecute(() -> op[0] = MowzieKit.reconcile(den.faction, MowzieKit.epoch(MowzieKit.census(den.faction, 1)),
                        List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, ADOPT, CHIEF, id[0]))))
                .thenWaitUntil(() -> MowzieKit.answered(den.faction, op[0], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("the ADOPT", ConflictKit.outcomes(den.faction, op[0]), DONE);
                    check("the Frostmaw is tagged (faction, 0, 1)", MowzieKit.tagged(f[0], den.faction, 0, 1));
                    check("and still not persistence-required (the jar's, MZB-210)", !f[0].isPersistenceRequired());
                    check("the den has had no CHIEF_STATE", ConflictKit.chiefStates(den.faction).isEmpty());
                    queued[0] = MowzieKit.camps().chiefStatesQueued;
                    thief[0] = MowzieKit.thief(helper, f[0], GameType.SURVIVAL, true, false);
                })
                .thenWaitUntil(() -> {
                    check("the thief holds the crystal (" + MowzieKit.crystals(thief[0]) + ")", MowzieKit.crystals(thief[0]) == 1);
                    check("the Frostmaw has lost it", !f[0].getHasCrystal());
                    check("a CHIEF_STATE", !ConflictKit.chiefStates(den.faction).isEmpty());
                })
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
                    thief[0].discard();
                    den.release();
                })
                .thenWaitUntil(() -> check("CAMP_OUT", ConflictKit.campOuts(den.faction).size() == 1))
                .thenWaitUntil(gone(level, id[0]))
                .thenExecute(() -> {
                    check("no second census yet", MowzieKit.censusCount(den.faction) == 1);
                    den.force();
                })
                .thenWaitUntil(far.ready())
                .thenWaitUntil(() -> check("census 2", MowzieKit.censusCount(den.faction) >= 2))
                .thenExecute(() -> {
                    Entity e = level.getEntity(id[0]);
                    check("the Frostmaw is back, a new object", e instanceof EntityFrostmaw && e != f[0]);
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
                    List<EntityFrostmaw> all = level.getEntitiesOfClass(EntityFrostmaw.class, den.area());
                    check("one Frostmaw at the den, crystal-less (" + all.size() + ")", all.size() == 1 && !all.get(0).getHasCrystal());
                    op[1] = MowzieKit.reconcile(fresh.faction, MowzieKit.epoch(MowzieKit.census(fresh.faction, 1)),
                            List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, SPAWN, CHIEF)));
                })
                .thenWaitUntil(() -> MowzieKit.answered(fresh.faction, op[1], 1))
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
    @GameTest(template = ARENA, timeoutTicks = 1200, batch = "frostmaw_arrows", skyAccess = true)
    public static void anArrowAndATridentDoNothingAndABulletHurtsAFrostmaw(GameTestHelper helper) {
        String test = "anArrowAndATridentDoNothingAndABulletHurtsAFrostmaw";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        int cx = far.k, cz = far.z0;
        EntityFrostmaw[] f = new EntityFrostmaw[1];
        ServerPlayer[] player = new ServerPlayer[1];

        helper.startSequence()
                .thenExecute(() -> far.forceRect(cx - 1, cz - 1, cx + 1, cz + 1))
                .thenWaitUntil(far.ready())
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
     * MZB-277), and no CHIEF_STATE is sent. A visible creative player takes it too. Nothing happens after 100
     * more ticks to an invisible player whose 36 main slots each hold something, to a visible survival
     * player or to an invisible spectator.
     */
    @GameTest(template = ARENA, timeoutTicks = 1500, batch = "frostmaw_thieves", skyAccess = true)
    public static void anInvisiblePlayerStealsTheCrystalFromASleepingFrostmaw(GameTestHelper helper) {
        String test = "anInvisiblePlayerStealsTheCrystalFromASleepingFrostmaw";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        int n = 5, cz = far.z0;
        int[] cx = new int[n];
        for (int i = 0; i < n; i++)
            cx[i] = far.k + 3 * i;
        EntityFrostmaw[] f = new EntityFrostmaw[n];
        Player[] thief = new Player[n];
        boolean[] persistent = new boolean[n];
        int[] states = new int[1];
        long[] queued = new long[2];

        helper.startSequence()
                .thenExecute(() -> {
                    for (int i = 0; i < n; i++)
                        far.forceRect(cx[i] - 1, cz - 1, cx[i] + 1, cz + 1);
                })
                .thenWaitUntil(far.ready())
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
                .thenWaitUntil(() -> {
                    check("the invisible survival thief holds a crystal (" + MowzieKit.crystals(thief[0]) + ")", MowzieKit.crystals(thief[0]) == 1);
                    check("the visible creative thief holds a crystal (" + MowzieKit.crystals(thief[1]) + ")", MowzieKit.crystals(thief[1]) == 1);
                    check("the first Frostmaw has lost its crystal", !f[0].getHasCrystal());
                    check("the second Frostmaw has lost its crystal", !f[1].getHasCrystal());
                })
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
                        check("and sleeps", !f[i].getActive());
                    }
                    check("no CHIEF_STATE was sent (" + (chiefStateRecords() - states[0]) + ")", chiefStateRecords() == states[0]);
                    check("none was queued (" + (MowzieKit.camps().chiefStatesQueued - queued[0]) + ")", MowzieKit.camps().chiefStatesQueued == queued[0]);
                    check("none was written (" + (MowzieKit.camps().chiefStates - queued[1]) + ")", MowzieKit.camps().chiefStates == queued[1]);
                    record(test, "thieves", "invisible survival and visible creative stole; a full inventory, a visible survival player and a spectator did not; no CHIEF_STATE for strays");
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
    @GameTest(template = ARENA, timeoutTicks = 3500, batch = "frostmaw_kill", skyAccess = true)
    public static void aChiefKilledAtTheCampIsReportedWithTheKiller(GameTestHelper helper) {
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
                .thenWaitUntil(far.ready())
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
                .thenWaitUntil(() -> {
                    for (int i = 0; i < n; i++)
                        check("den " + i + "'s census", MowzieKit.censusCount(den[i].faction) >= 1);
                })
                .thenExecute(() -> {
                    for (int i = 0; i < n; i++)
                        op[i] = MowzieKit.reconcile(den[i].faction, MowzieKit.epoch(MowzieKit.census(den[i].faction, 1)),
                                List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, ADOPT, CHIEF, f[i].getUUID())));
                })
                .thenWaitUntil(() -> {
                    for (int i = 0; i < n; i++)
                        MowzieKit.answered(den[i].faction, op[i], 1);
                })
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
                .thenWaitUntil(() -> check("CAMP_DEATH of den 0", ConflictKit.campDeaths(den[0].faction).size() == 1))
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
                .thenWaitUntil(() -> check("the body is out of the level", removed[0] != 0))
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
                .thenWaitUntil(() -> check("CAMP_DEATH of den 1", ConflictKit.campDeaths(den[1].faction).size() == 1))
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
                .thenWaitUntil(() -> check("CAMP_DEATH of den 2", ConflictKit.campDeaths(den[2].faction).size() == 1))
                .thenExecute(() -> {
                    chiefDeath("the generic kill after the hit", ConflictKit.campDeaths(den[2].faction).get(0), ConflictRecords.BY_PLAYER, hearer[0].getUUID());
                    check("the news hook was reached three times (" + (ConflictNews.posted() - news[0]) + ")", ConflictNews.posted() == news[0] + 3);
                    // a zombie's attack on the fourth
                    Zombie zombie = EntityType.ZOMBIE.create(level);
                    check("a zombie is made", zombie != null);
                    zombie.moveTo(f[3].position().add(-2, 0, 0));
                    MowzieKit.kill(f[3], level.damageSources().mobAttack(zombie));
                })
                .thenWaitUntil(() -> check("CAMP_DEATH of den 3", ConflictKit.campDeaths(den[3].faction).size() == 1))
                .thenExecute(() -> {
                    chiefDeath("the zombie's kill", ConflictKit.campDeaths(den[3].faction).get(0), ConflictRecords.BY_MOB, null);
                    check("the news hook was reached four times (" + (ConflictNews.posted() - news[0]) + ")", ConflictNews.posted() == news[0] + 4);
                    // a generic kill with no hit before
                    MowzieKit.kill(f[4], level.damageSources().genericKill());
                })
                .thenWaitUntil(() -> check("CAMP_DEATH of den 4", ConflictKit.campDeaths(den[4].faction).size() == 1))
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

    // ---- a lurk puppet taken, or still held when the module goes (MZB-369, UR-1) ---------------------

    /** One held lurk puppet of a one-unit Frostmaw band at a spot, and what the tests below do to it. */
    private static final class Held {
        final String test;
        final Far far;
        final ServerLevel level;
        final long agent = ConflictKit.newAgent(), faction = ConflictKit.newFaction();
        final int x, z;
        final Vec3[] spot = new Vec3[1], track = new Vec3[1];
        final EntityFrostmaw[] f = new EntityFrostmaw[1];
        /** The saved NBT of the body (or of the copy), kept to compare. */
        final CompoundTag[] saved = {null};
        final Entity[] copy = {null};

        Held(GameTestHelper helper, String test) {
            this.test = test;
            this.far = new Far(helper, test);
            this.level = far.level;
            this.x = far.bx + 8;
            this.z = far.bz + 8;
        }

        /** The sequence up to a held puppet: the lurk is in the table, the puppet stands, tagged, awake for good, restricted. */
        GameTestSequence up(GameTestHelper helper) {
            return helper.startSequence()
                    .thenExecute(() -> far.force(far.k, far.z0))
                    .thenWaitUntil(far.ready())
                    .thenExecute(() -> {
                        int y = MowzieKit.platform(level, far.bx + 2, far.bx + 14, far.bz + 2, far.bz + 14) + 1;
                        spot[0] = new Vec3(x + 0.5, y, z + 0.5);
                        track[0] = new Vec3(x + 0.5, y, z - 3.5);
                        ConflictKit.ambushSpot(agent, 1, faction, far.dim, new double[] {track[0].x, track[0].y, track[0].z},
                                new double[] {spot[0].x, spot[0].y, spot[0].z}, 8.0f);
                    })
                    .thenWaitUntil(io(() -> check("the lurk is in the table", RtsimApi.lurks().containsKey(agent))))
                    .thenExecute(() -> {
                        ConflictKit.Band b = ConflictKit.band(agent, faction, far.dim);
                        b.arcX = spot[0].x;
                        b.arcZ = spot[0].z;
                        b.targetX = track[0].x;
                        b.targetY = track[0].y;
                        b.targetZ = track[0].z;
                        b.speed = 0f;
                        ConflictKit.materialize(b, List.of(new ConflictKit.StackSpec(MowzieKit.FROSTMAW, 1, false)));
                    })
                    .thenWaitUntil(io(() -> {
                        check("the puppet stands", RtsimApi.lurkBody(agent) instanceof EntityFrostmaw);
                        EntityFrostmaw p = (EntityFrostmaw) RtsimApi.lurkBody(agent);
                        check("and is held: awake for good and restricted", p.isAlwaysActive() && p.hasRestriction());
                    }))
                    .thenExecute(() -> {
                        f[0] = (EntityFrostmaw) RtsimApi.lurkBody(agent);
                        ConflictKit.onEnd(helper, () -> {
                            f[0].discard();
                            if (copy[0] != null)
                                copy[0].discard();
                            RtsimConfig.Test.setModule(Module.CONFLICT, null);
                            BridgeEvents.moduleSwitchesChanged(level.getServer());
                        });
                        CompoundTag nbt = f[0].saveWithoutId(new CompoundTag());
                        check("the puppet is tagged", RtsimApi.isPuppet(f[0]));
                        check("its NBT says alwaysActive (the premise: " + nbt.getBoolean("alwaysActive") + ")", nbt.getBoolean("alwaysActive"));
                        check("its restriction is two blocks about the spot (" + f[0].getRestrictRadius() + ")", f[0].getRestrictRadius() == 2.0f);
                    });
        }

        /** What an ordinary Frostmaw is, of a live body: no puppet, no restriction, not always-active, not the bridge's. */
        void ordinary(String what, Entity e) {
            check(what + " is a Frostmaw", e instanceof EntityFrostmaw);
            EntityFrostmaw g = (EntityFrostmaw) e;
            check(what + " is no puppet", !RtsimApi.isPuppet(g));
            check(what + " is no member of the bridge's", ConflictKit.bridge().slotOf(g) == null);
            check(what + " has no restriction (" + g.hasRestriction() + ")", !g.hasRestriction());
            check(what + " is not always-active (" + g.isAlwaysActive() + ")", !g.isAlwaysActive());
            check(what + " is alive and in the world", g.isAlive() && !g.isRemoved());
        }

        /** A body's saved NBT, with the hold's own mark, {@code alwaysActive}, false; kept in {@link #saved}. */
        void saveOrdinary(String what, Entity e) {
            CompoundTag nbt = new CompoundTag();
            check(what + " saves (it is no puppet)", e.save(nbt));
            check(what + ": alwaysActive false in its saved NBT", !nbt.getBoolean("alwaysActive"));
            saved[0] = nbt;
        }

        /**
         * A body's saved NBT as a world without mic_rtsim and the plugin would read it (UR-1): nothing of
         * mic_rtsim in it for NeoForge to skip (no attachment, no tag, no persistent data, no word of it anywhere),
         * and {@code alwaysActive} false. Only once the engine has acknowledged what the body carried.
         */
        void leavesNothing(String what, Entity e) {
            saveOrdinary(what, e);
            CompoundTag nbt = saved[0];
            for (String k : nbt.getCompound("neoforge:attachments").getAllKeys())
                check(what + ": no attachment of mic_rtsim (" + k + ")", !k.startsWith("mic_rtsim:"));
            for (String k : nbt.getCompound("NeoForgeData").getAllKeys())
                check(what + ": no persistent data of mic_rtsim (" + k + ")", !k.contains("mic_rtsim"));
            for (Tag t : nbt.getList("Tags", 8))
                check(what + ": no tag of mic_rtsim (" + t.getAsString() + ")", !t.getAsString().contains("mic_rtsim"));
            check(what + ": nothing of mic_rtsim anywhere in its saved NBT", !nbt.toString().contains("mic_rtsim"));
        }

        void off(GameTestHelper helper, boolean off) {
            RtsimConfig.Test.setModule(Module.CONFLICT, off ? Boolean.FALSE : null);
            BridgeEvents.moduleSwitchesChanged(helper.getLevel().getServer());
        }
    }

    /**
     * A held lurk puppet taken by a player is an ordinary Frostmaw again (UR-1, MZB-369). A one-unit Frostmaw band
     * lurking at a spot, held (awake for good, restricted to the spot, with that in its saved NBT); a player's
     * pickup ({@code takeFromSim}, Carry On's) takes it. At once: the body is no longer a puppet, has no restriction,
     * is not always-active, its saved NBT says {@code alwaysActive} false, and a body made from that NBT is not
     * always-active either. The uninstall and the reinstall, by the module switch the other GameTests use: once the
     * engine has acknowledged the casualty (the body's {@code mic_rtsim:pending} record drops), the body's saved NBT
     * has nothing of mic_rtsim in it for a world without the mod to skip; the conflict module is switched off, and
     * the body stays, alive and ordinary; switched on again, a copy made from the saved NBT joins the level as an
     * ordinary Frostmaw and nothing holds either of them again.
     * <p>On the code before the fix (forget cleared the hold without asking the Lurker) the body keeps
     * {@code restrictTo(spot, 2)} (hasRestriction true) and writes {@code alwaysActive true} to its NBT, so the first
     * check after the take fails: "the taken body has no restriction (true)". Not run.
     */
    @GameTest(template = ARENA, timeoutTicks = 1500, batch = "frostmaw_lurk_taken", skyAccess = true)
    public static void aLurkPuppetTakenByAPlayerIsLetGoLikeAnyOrdinaryFrostmaw(GameTestHelper helper) {
        taken(helper, "aLurkPuppetTakenByAPlayerIsLetGoLikeAnyOrdinaryFrostmaw", true);
    }

    /**
     * The same for a puppet taken by something that is not a player ({@code TakeReason.PLUGIN}, no player): the
     * casualty is by other, and the body is let go the same way. On the code before the fix: "the taken body has no
     * restriction (true)". Not run.
     */
    @GameTest(template = ARENA, timeoutTicks = 1500, batch = "frostmaw_lurk_taken_other", skyAccess = true)
    public static void aLurkPuppetTakenByNoPlayerIsLetGoLikeAnyOrdinaryFrostmaw(GameTestHelper helper) {
        taken(helper, "aLurkPuppetTakenByNoPlayerIsLetGoLikeAnyOrdinaryFrostmaw", false);
    }

    private static void taken(GameTestHelper helper, String test, boolean byPlayer) {
        Held h = new Held(helper, test);
        Player[] taker = new Player[1];
        int by = byPlayer ? ConflictRecords.BY_PLAYER : ConflictRecords.BY_OTHER;
        h.up(helper)
                .thenExecute(() -> {
                    EntityFrostmaw f = h.f[0];
                    taker[0] = byPlayer ? helper.makeMockPlayer(GameType.SURVIVAL) : null;
                    check("the take works", RtsimApi.takeFromSim(f, byPlayer ? TakeReason.PICKUP : TakeReason.PLUGIN, taker[0]));
                    check("it is no puppet now", !RtsimApi.isPuppet(f));
                    check("the taken body has no restriction (" + f.hasRestriction() + ")", !f.hasRestriction());
                    check("the taken body is not always-active (" + f.isAlwaysActive() + ")", !f.isAlwaysActive());
                    // The record it carries (mic_rtsim:pending) is the one thing of mic_rtsim on it, until the engine has it
                    h.saveOrdinary("the taken body, before the engine has the casualty,", f);
                    Entity again = EntityType.create(h.saved[0], h.level).orElse(null);
                    check("a body made from that NBT is a Frostmaw", again instanceof EntityFrostmaw);
                    check("and is not always-active (" + ((EntityFrostmaw) again).isAlwaysActive() + ")", !((EntityFrostmaw) again).isAlwaysActive());
                    check("and has no restriction (" + ((EntityFrostmaw) again).hasRestriction() + ")", !((EntityFrostmaw) again).hasRestriction());
                    check("the lurk is still in the table", RtsimApi.lurks().containsKey(h.agent));
                })
                .thenWaitUntil(io(() -> {
                    List<ScriptedEngine.Received> c = ConflictKit.about(ConflictRecords.BAND_CASUALTIES, h.agent);
                    check("the engine has the casualty", c.size() == 1);
                    check("by " + by + ", not " + c.get(0).aux(), c.get(0).aux() == by);
                }))
                .thenExecute(() -> RtsimTests.engine().requestSaveAtNextStep())
                .thenWaitUntil(io(() -> check("acknowledged: the body carries no records", !h.f[0].hasData(RtsimAttachments.PENDING))))
                .thenExecute(() -> {
                    // The uninstall: the module off. The body is not the bridge's, so it stays as it is.
                    h.off(helper, true);
                    h.leavesNothing("the taken body, acknowledged,", h.f[0]);
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    h.ordinary("the taken body with the module off", h.f[0]);
                    h.leavesNothing("the taken body with the module off,", h.f[0]);
                    // The reinstall: the module on, and a copy of the saved body joins the level
                    h.off(helper, false);
                    Entity c = EntityType.create(h.saved[0], h.level).orElse(null);
                    check("the copy is made from the saved NBT", c != null);
                    c.setUUID(UUID.randomUUID());
                    h.level.addLegacyChunkEntities(Stream.of(c));
                    check("the copy joined", h.level.getEntity(c.getUUID()) == c);
                    h.copy[0] = c;
                })
                .thenIdle(60)
                .thenExecute(() -> {
                    h.ordinary("the taken body after the reinstall", h.f[0]);
                    h.ordinary("the copy of it", h.copy[0]);
                    check("nothing is held by the lurk", RtsimApi.lurkBody(h.agent) == null);
                    record(test, "taken", "the lurk puppet taken " + (byPlayer ? "by a player" : "by no player") + " at " + h.f[0].position()
                            + " is an ordinary Frostmaw: no restriction, not always-active, in its NBT or in a copy made from it, and"
                            + " nothing of mic_rtsim on it once the engine has the casualty; the module off and on left it alone");
                    ConflictKit.ambushRemoved(h.agent, 1, h.faction, h.far.dim);
                })
                .thenWaitUntil(io(() -> check("the lurk is gone from the table", !RtsimApi.lurks().containsKey(h.agent))))
                .thenSucceed();
    }

    /**
     * The other end of the same uninstall (UR-1): a puppet still held when the conflict module is switched off is
     * not left in the world. The body is discarded and the lurk's table cleared, and switched on again nothing
     * stands in its place. (This holds on the code before the fix too: it is the guard that the release does not
     * change the switch-off.) Not run.
     */
    @GameTest(template = ARENA, timeoutTicks = 1500, batch = "frostmaw_lurk_off", skyAccess = true)
    public static void aLurkPuppetHeldWhenTheModuleSwitchesOffLeavesNothingInTheWorld(GameTestHelper helper) {
        String test = "aLurkPuppetHeldWhenTheModuleSwitchesOffLeavesNothingInTheWorld";
        Held h = new Held(helper, test);
        h.up(helper)
                .thenExecute(() -> h.off(helper, true))
                .thenWaitUntil(() -> check("the held body is gone with the module off", h.f[0].isRemoved()))
                .thenExecute(() -> {
                    check("the lurk has no body", RtsimApi.lurkBody(h.agent) == null);
                    h.off(helper, false);
                })
                .thenIdle(60)
                .thenExecute(() -> {
                    AABB around = new AABB(h.spot[0], h.spot[0]).inflate(24);
                    check("no Frostmaw stands about the spot", h.level.getEntitiesOfClass(EntityFrostmaw.class, around).isEmpty());
                    check("the lurk has no body", RtsimApi.lurkBody(h.agent) == null);
                    record(test, "off", "a held lurk puppet was gone with the module switched off, and nothing stood in its place after it was on again");
                    ConflictKit.ambushRemoved(h.agent, 1, h.faction, h.far.dim);
                })
                .thenWaitUntil(io(() -> check("the lurk is gone from the table", !RtsimApi.lurks().containsKey(h.agent))))
                .thenSucceed();
    }

    // ---- the hold against the jar's chase (MZB-361) -------------------------------------------------

    /** Ticks the chase test gives a displaced puppet to walk back, and the steady stretch it then stands for. */
    private static final int BACK_TICKS = 200, STEADY_TICKS = 100;
    /** The most new paths a puppet standing within its stand may be given in the steady stretch with a player near (the hold gives none). */
    private static final int STEADY_PATHS = 2;

    /**
     * The hold wins against the jar's chase (MZB-361, the re-check's P1). A held puppet is pushed 2 blocks off its spot,
     * across the line to a player 8 blocks west who is its target (the jar's {@code tick} walks an awake Frostmaw to
     * its target every tick it is more than 5.5 blocks off, replacing the hold's path, and stops it when it is
     * not). The puppet has no crystal, so no ice breath or ice ball animation stops its walk. Asserted: it is back
     * within {@link FrostmawLurker#STAND} of the spot within {@value #BACK_TICKS} ticks; it never comes closer to
     * the player than its spot is, less the stand (it did not walk after the player); and in the {@value #STEADY_TICKS}
     * ticks after, it stays within the stand and is given at most {@value #STEADY_PATHS} new paths (the jar's fresh
     * A* each tick, after a hold's stop cleared the last, was twenty a second). The wandering stroll goal, which a
     * Frostmaw has and which moves a body up to its restriction's radius, is out while it is held too.
     * <p>On the code before the fix the jar's chase wins nine ticks in ten and the puppet walks west: the test fails
     * with "back within the stand of its spot in 200 ticks" (the closest it came to the spot is printed), or, inside
     * 5.5 of the player, a stop each tick keeps it where it is. Not run.
     */
    @GameTest(template = ARENA, timeoutTicks = 1500, batch = "frostmaw_lurk_chase", skyAccess = true)
    public static void aHeldPuppetWalksBackToItsSpotAndDoesNotChaseItsTarget(GameTestHelper helper) {
        String test = "aHeldPuppetWalksBackToItsSpotAndDoesNotChaseItsTarget";
        Held h = new Held(helper, test);
        ServerPlayer[] player = new ServerPlayer[1];
        double[] closest = {Double.MAX_VALUE}, nearest = {Double.MAX_VALUE}, stray = {0};
        long[] from = new long[2];
        int[] paths = {0, 0};
        Path[] last = new Path[1];
        h.up(helper)
                .thenExecute(() -> {
                    EntityFrostmaw f = h.f[0];
                    f.setHasCrystal(false);
                    player[0] = FakePlayers.add(h.level, h.spot[0].add(-8, 0, 0), "mowzie-chase");
                    player[0].setNoGravity(true);
                    player[0].getAttribute(Attributes.MAX_HEALTH).setBaseValue(1000.0);
                    player[0].setHealth(1000f);
                    ConflictKit.onEnd(helper, () -> FakePlayers.remove(player[0]));
                    // Two blocks north of the spot: the player (west) is then 8.2 blocks off, past the jar's 5.5 chase threshold and 6.5 swipe range
                    f.setPos(h.spot[0].x, h.spot[0].y, h.spot[0].z - 2.0);
                    f.setDeltaMovement(Vec3.ZERO);
                    f.getNavigation().stop();
                    f.setTarget(player[0]);
                    from[0] = h.level.getGameTime();
                    from[1] = -1;
                })
                .thenWaitUntil(() -> {
                    EntityFrostmaw f = h.f[0];
                    f.setTarget(player[0]); // the restriction gates its own targeting (MZB-362)
                    double fromSpot = Math.hypot(f.getX() - h.spot[0].x, f.getZ() - h.spot[0].z);
                    closest[0] = Math.min(closest[0], fromSpot);
                    nearest[0] = Math.min(nearest[0], Math.hypot(f.getX() - player[0].getX(), f.getZ() - player[0].getZ()));
                    long now = h.level.getGameTime();
                    check("back within " + FrostmawLurker.STAND + " blocks of its spot in " + BACK_TICKS + " ticks (" + (now - from[0])
                            + " gone, the closest it came to the spot " + closest[0] + ", the closest it came to the player " + nearest[0] + ")",
                            fromSpot <= FrostmawLurker.STAND);
                })
                .thenExecute(() -> {
                    from[1] = h.level.getGameTime();
                    last[0] = h.f[0].getNavigation().getPath();
                    stray[0] = Math.hypot(h.f[0].getX() - h.spot[0].x, h.f[0].getZ() - h.spot[0].z);
                    record(test, "back", "pushed 2 blocks off its spot, the puppet was back within " + FrostmawLurker.STAND + " in " + (from[1] - from[0])
                            + " ticks, never nearer the player than " + nearest[0]);
                })
                .thenWaitUntil(() -> {
                    EntityFrostmaw f = h.f[0];
                    f.setTarget(player[0]);
                    Path p = f.getNavigation().getPath();
                    if (p != null && p != last[0])
                        paths[0]++;
                    last[0] = p;
                    stray[0] = Math.max(stray[0], Math.hypot(f.getX() - h.spot[0].x, f.getZ() - h.spot[0].z));
                    nearest[0] = Math.min(nearest[0], Math.hypot(f.getX() - player[0].getX(), f.getZ() - player[0].getZ()));
                    check("the steady stretch ran " + (h.level.getGameTime() - from[1]) + " of " + STEADY_TICKS + " ticks",
                            h.level.getGameTime() - from[1] >= STEADY_TICKS);
                })
                .thenExecute(() -> {
                    double floor = h.spot[0].distanceTo(player[0].position().with(net.minecraft.core.Direction.Axis.Y, h.spot[0].y)) - FrostmawLurker.STAND;
                    check("it never walked after the player: the closest it came was " + nearest[0] + ", its spot is " + (floor + FrostmawLurker.STAND)
                            + " off (" + floor + " allowed)", nearest[0] >= floor - 0.05);
                    check("it stayed within its stand for " + STEADY_TICKS + " ticks (strayed " + stray[0] + ")", stray[0] <= FrostmawLurker.STAND + 1e-6);
                    check("it was given at most " + STEADY_PATHS + " new paths in " + STEADY_TICKS + " ticks standing within its stand with a player near ("
                            + paths[0] + ")", paths[0] <= STEADY_PATHS);
                    check("it still holds: alwaysActive and restricted", h.f[0].isAlwaysActive() && h.f[0].hasRestriction());
                    record(test, "steady", STEADY_TICKS + " ticks within the stand with the player 8 blocks off: strayed " + stray[0] + ", new paths " + paths[0]);
                    ConflictKit.ambushRemoved(h.agent, 1, h.faction, h.far.dim);
                })
                .thenWaitUntil(io(() -> check("the lurk is gone from the table", !RtsimApi.lurks().containsKey(h.agent))))
                .thenSucceed();
    }

    // ---- a strike's bearing (MZB-361) --------------------------------------------------------------

    /**
     * A strike's bearing ends with its swing (MZB-361, review C item 3). A held puppet strikes at a point east of it
     * (it faces yaw -90), the swing ends, and a player stands 4 blocks west: the Frostmaw's own swipe at the player
     * (the jar's {@code AnimationAreaAttackAI} turns it to its target by up to 30 degrees a tick until the hit frame
     * at tick 9) turns the body away from the strike's bearing, to within 90 degrees of the player. On the code
     * before the fix {@code FrostmawLurker.bearings} kept the strike's yaw until the puppet was released, and
     * {@code hold} pinned the body to it through every plain swipe: the body stays at -90 (180 degrees from the
     * player) and the test fails with "the swipe at the player turned away from the strike's bearing -90.0 ...
     * (yBodyRot -90.0)". A swipe-twice is not pinned (the plugin pins only the swipe), so the test waits for a plain
     * swipe; the player has 1000 health so a swing cannot end it. Not run.
     */
    @GameTest(template = ARENA, timeoutTicks = 1500, batch = "frostmaw_lurk_bearing", skyAccess = true)
    public static void aStrikesBearingDoesNotTurnTheFrostmawsOwnLaterSwipes(GameTestHelper helper) {
        String test = "aStrikesBearingDoesNotTurnTheFrostmawsOwnLaterSwipes";
        Held h = new Held(helper, test);
        ServerPlayer[] player = new ServerPlayer[1];
        float[] stale = new float[1], yaw = new float[1];
        boolean[] swiped = new boolean[1];
        h.up(helper)
                .thenExecute(() -> {
                    check("the strike starts", RtsimApi.strike(h.agent, h.spot[0].add(6, 0, 0), Vec3.ZERO));
                    stale[0] = h.f[0].yBodyRot;
                    check("it turned east, to yaw -90 (" + stale[0] + ")", Mth.degreesDifferenceAbs(stale[0], -90f) < 1f);
                    check("and swipes", h.f[0].getAnimation() == EntityFrostmaw.SWIPE_ANIMATION);
                })
                .thenWaitUntil(() -> check("the strike's swing is over (tick " + h.f[0].getAnimationTick() + ")",
                        h.f[0].getAnimation() == IAnimatedEntity.NO_ANIMATION))
                .thenExecute(() -> {
                    player[0] = FakePlayers.add(h.level, h.spot[0].add(-4, 0, 0), "mowzie-bearing");
                    player[0].getAttribute(Attributes.MAX_HEALTH).setBaseValue(1000.0);
                    player[0].setHealth(1000f);
                    ConflictKit.onEnd(helper, () -> FakePlayers.remove(player[0]));
                })
                .thenWaitUntil(() -> {
                    EntityFrostmaw f = h.f[0];
                    f.setTarget(player[0]); // the restriction gates its own targeting (MZB-362)
                    if (!swiped[0] && f.getAnimation() == EntityFrostmaw.SWIPE_ANIMATION && f.getAnimationTick() >= SWIPE_LATE
                            && f.getAnimationTick() <= FrostmawLurker.SWIPE_HIT_TICK) {
                        swiped[0] = true;
                        yaw[0] = f.yBodyRot;
                    }
                    check("the Frostmaw swiped at the player (a swipe: " + (f.getAnimation() == EntityFrostmaw.SWIPE_ANIMATION) + ", tick "
                            + f.getAnimationTick() + ")", swiped[0]);
                })
                .thenExecute(() -> {
                    float off = Mth.degreesDifferenceAbs(yaw[0], stale[0]);
                    check("the swipe at the player turned away from the strike's bearing " + stale[0] + " (yBodyRot " + yaw[0] + ", "
                            + off + " degrees off it)", off > 90f);
                    record(test, "bearing", "the swipe at a player 4 blocks west faced yaw " + yaw[0] + ", " + off + " degrees from the strike's " + stale[0]);
                    ConflictKit.ambushRemoved(h.agent, 1, h.faction, h.far.dim);
                })
                .thenWaitUntil(io(() -> check("the lurk is gone from the table", !RtsimApi.lurks().containsKey(h.agent))))
                .thenSucceed();
    }

    /** The swipe's tick from which its yaw is read: the jar has turned the body by up to 30 degrees on each of the ticks before. */
    private static final int SWIPE_LATE = 8;

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
