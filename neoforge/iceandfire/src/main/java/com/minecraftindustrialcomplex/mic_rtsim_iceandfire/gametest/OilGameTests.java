package com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest;

import com.iafenvoy.iceandfire.entity.DragonBaseEntity;
import com.iafenvoy.iceandfire.entity.util.dragon.IafDragonAttacks;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonBridge;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonConfig;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonRecords;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.MicRtsimIceAndFire;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;
import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.about;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.body;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.bridge;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.check;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.engine;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.record;

/**
 * The oil gate's plugin half (OILGATE.md §4.3, OG-4, OG-13): a dragon whose body carries a reserve pays
 * a point for each breath of fire and fights with bite and claws once it is spent; its breaths reach the
 * engine as DRAGON_BREATHS, summed, before its body goes. Plugin-only: either engine. Every test logs
 * what it compared on a {@code [gametest]} line; each has its own batch.
 */
@GameTestHolder(MicRtsimIceAndFire.MODID)
@PrefixGameTestTemplate(false)
public final class OilGameTests {
    private OilGameTests() {}

    /** NONE leaves its attacks alone; MELEE keeps it on the ground (see nudge). */
    enum Fire { NONE, GROUND, HOVER, SCORCH, MELEE }

    /**
     * Keeps a dragon on its target and, unless {@code fire} is NONE, on one kind of fire attack (Ice
     * and Fire picks at random; a test wants each kind on demand). The target is set again only when it
     * changed: setting it resets the flight manager's scorch run.
     */
    static void nudge(DragonBaseEntity d, LivingEntity target, Fire fire) {
        if (d == null || d.isRemoved())
            return;
        if (d.getTarget() != target)
            d.setTarget(target);
        switch (fire) {
            case GROUND -> {
                d.usingGroundAttack = true;
                d.groundAttack = IafDragonAttacks.Ground.FIRE;
            }
            case HOVER -> {
                d.usingGroundAttack = false;
                d.airAttack = IafDragonAttacks.Air.HOVER_BLAST;
                if (!d.isFlying())
                    d.setFlying(true);
            }
            case SCORCH -> {
                d.usingGroundAttack = false;
                d.airAttack = IafDragonAttacks.Air.SCORCH_STREAM;
                if (!d.isFlying())
                    d.setFlying(true);
            }
            case MELEE -> {
                // On the ground, where Ice and Fire's melee goal bites its target directly. In the air a dry
                // dragon tackles (the bridge's choice), and a tackle finds its victims in the level's entity
                // sections, where a mock player never stands; a real player would be hit.
                d.usingGroundAttack = true;
                if (d.isFlying() || d.isHovering()) {
                    d.setFlying(false);
                    d.setHovering(false);
                }
                // Its shake grabs the prey as a passenger and hurts it only there; a guarded mock player
                // never rides (Guard cancels the mount), so the shake never lands. A bite does.
                if (d.groundAttack == IafDragonAttacks.Ground.SHAKE_PREY)
                    d.groundAttack = IafDragonAttacks.Ground.BITE;
            }
            case NONE -> {}
        }
    }

    /** A guarded GameTest player (survival, no damage) at {@code at}. */
    static ServerPlayer guardedPlayer(GameTestHelper helper, Vec3 at, String name) {
        ServerPlayer p = FakePlayers.add(helper.getLevel(), at, name);
        Guard.guard(p);
        return p;
    }

    static void release(ServerPlayer p) {
        if (p == null)
            return;
        // Off any dragon that carries it, out of every mob's sights, and gone: a mock player left as a
        // dragon's target is carried off, and a passenger ticks (Guard's GROUNDED).
        p.stopRiding();
        for (net.minecraft.world.entity.Mob m : p.serverLevel().getEntitiesOfClass(net.minecraft.world.entity.Mob.class,
                p.getBoundingBox().inflate(256), m -> m.getTarget() == p))
            m.setTarget(null);
        Guard.release(p);
        FakePlayers.remove(p);
        p.discard();
    }

    /** A young fire dragon hostile to {@code player}, awake, at {@code at}. */
    static DragonKit.Spawn hostile(long agent, GameTestHelper helper, Vec3 at, UUID player) {
        return new DragonKit.Spawn(agent, 1, DragonKit.FIRE, DragonKit.dimension(helper), at, DragonKit.AGE_30_DAYS, 50, false, 0,
                0f, null, at, player, 0);
    }

    static int breathsSent(long agent) {
        int n = 0;
        for (ScriptedEngine.Received r : about(DragonRecords.DRAGON_BREATHS, agent))
            n += r.aux();
        return n;
    }

    static DragonBaseEntity bodyOrNull(long agent) {
        return bridge().body(agent) instanceof DragonBaseEntity d ? d : null;
    }

    /**
     * OIL_CONFIG went out at session start with the plugin's switches (in GameTests every switch is off
     * until a test turns one on), before any PLAYER; a switch change sends it again.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 200, batch = "oil_config")
    public static void oilConfigGoesOutAtSessionStartWithTheSwitches(GameTestHelper helper) {
        String test = "oilConfigGoesOutAtSessionStartWithTheSwitches";
        List<ScriptedEngine.Received> configs = engine().received(DragonRecords.OIL_CONFIG);
        int sessionFlags = bridge().oilConfigSent;
        long sessionSeq = bridge().sessionOilConfigSeq;
        check("one OIL_CONFIG at least this session (" + bridge().oilConfigs + ")", bridge().oilConfigs >= 1 && sessionSeq >= 0);
        check("the GameTest switches are off (" + sessionFlags + ")", sessionFlags == DragonConfig.oilFlags() && sessionFlags == 0);
        List<ScriptedEngine.Received> all = engine().received();
        int firstConfig = -1, firstPlayer = -1;
        for (int i = 0; i < all.size(); i++) {
            ScriptedEngine.Received r = all.get(i);
            if (firstConfig < 0 && r.kind() == DragonRecords.OIL_CONFIG && r.obsSeq() == sessionSeq)
                firstConfig = i;
            if (firstPlayer < 0 && r.kind() == Obs.PLAYER && r.handover() >= (firstConfig >= 0 ? all.get(firstConfig).handover() : Long.MAX_VALUE))
                firstPlayer = i;
        }
        boolean seen = firstConfig >= 0;
        if (seen) {
            check("before the session's first PLAYER (" + firstConfig + ", " + firstPlayer + ")", firstPlayer < 0 || firstConfig < firstPlayer);
            check("carrying the switches (" + all.get(firstConfig).aux() + ")", all.get(firstConfig).aux() == 0);
        }
        int before = configs.size();
        DragonKit.oilConfig(DragonRecords.OilConfig.FLAG_FIELD_FLIGHTS | DragonRecords.OilConfig.FLAG_PLACE_LAIRS);
        helper.startSequence()
                .thenWaitUntil(() -> check("the new OIL_CONFIG reached the engine", engine().received(DragonRecords.OIL_CONFIG).size() > before))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> now = engine().received(DragonRecords.OIL_CONFIG);
                    ScriptedEngine.Received last = now.get(now.size() - 1);
                    check("flags 3 (" + last.aux() + ")", last.aux() == 3 && last.bytes().length >= DragonRecords.OilConfig.SIZE);
                    DragonKit.restoreOilConfig();
                })
                .thenWaitUntil(() -> {
                    List<ScriptedEngine.Received> now = engine().received(DragonRecords.OIL_CONFIG);
                    check("and back to the switches", now.size() > before + 1 && now.get(now.size() - 1).aux() == 0);
                })
                .thenExecute(() -> record(test, "OIL_CONFIG", "session start flags " + sessionFlags + " (obsSeq " + sessionSeq + ", " + (seen
                        ? "in the received log, before any PLAYER" : "scrolled out of the received log") + "), then 3, then back to 0"))
                .thenSucceed();
    }

    /**
     * A dragon with 15 points of oil fights a player with fire (nudged to its ground fire, the attack that
     * shoots charges with no event too) until the reserve is spent: every breath paid a point, the reserve
     * never goes below zero, the fire is stopped from then on (events cancelled, charges kept out of the
     * level) and the player takes no more fire from it; brought to the ground, it fights on with bite and claws. Its
     * DRAGON_BREATHS add up to the 15 points, at most one per 20 ticks.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 3000, batch = "oil_breaths_dry")
    public static void fireCostsAPointABreathAndStopsAtZero(GameTestHelper helper) {
        String test = "fireCostsAPointABreathAndStopsAtZero";
        int reserve = 15;
        long agent = DragonKit.newAgent();
        Runnable unforce = DragonKit.forceAround(helper, 4);
        Vec3 c = DragonKit.centre(helper);
        ServerPlayer player = guardedPlayer(helper, c, "oil-target");
        DragonKit.materialize(hostile(agent, helper, c.add(8, 6, 0), player.getUUID()), reserve);
        Fire[] mode = {Fire.GROUND};
        long[] zeroAt = {-1}, start = {-1};
        UUID[] dragon = new UUID[1];
        DragonBridge.Breath[] log = new DragonBridge.Breath[1];
        int[] anim = new int[3]; // ticks in the melee phase: biting, shaking, other attack animations
        helper.onEachTick(() -> {
            DragonBaseEntity d = bodyOrNull(agent);
            nudge(d, player, mode[0]);
            if (mode[0] == Fire.MELEE && d != null && d.isPlayingAttackAnimation())
                anim[d.getAnimation() == DragonBaseEntity.ANIMATION_BITE ? 0 : d.getAnimation() == DragonBaseEntity.ANIMATION_SHAKEPREY ? 1 : 2]++;
        });

        helper.startSequence()
                // The target is set at the end of the tick the body joins (the bridge's, or the nudge's), and a
                // failed check in a thenExecute does not stop the sequence: everything here waits.
                .thenWaitUntil(() -> {
                    check("hostile to the player", body(agent).getTarget() == player);
                    check("its breath log", bridge().breathOf(agent) != null);
                    log[0] = bridge().breathOf(agent);
                    dragon[0] = body(agent).getUUID();
                })
                .thenExecute(() -> {
                    start[0] = helper.getLevel().getGameTime();
                    // It may have breathed in the tick it took its target: what is left and what was paid add up.
                    int paid = log[0].eventsPaid + log[0].chargesPaid;
                    check("the body carried the reserve (" + bridge().oilOf(agent) + " left, " + paid + " paid)",
                            bridge().oilOf(agent) + paid == reserve);
                })
                .thenWaitUntil(() -> check("the reserve is spent (" + bridge().oilOf(agent) + " left, "
                        + (log[0] == null ? "?" : String.valueOf(log[0].events)) + " events)", bridge().oilOf(agent) == 0))
                .thenExecute(() -> {
                    zeroAt[0] = helper.getLevel().getGameTime();
                    DragonBridge.Breath b = log[0];
                    check("each breath paid a point: " + b.eventsPaid + " events and " + b.chargesPaid + " charges for " + reserve,
                            b.eventsPaid + b.chargesPaid == reserve);
                    record(test, "spent", reserve + " points in " + (zeroAt[0] - start[0]) + " ticks: " + b.eventsPaid + " fire events, "
                            + b.chargesPaid + " charges with no event, " + b.chargesWithEvent + " charges of a paid event; bursts "
                            + b.bursts);
                })
                .thenIdle(300) // still nudged to fire: everything it tries is stopped
                .thenExecute(() -> {
                    DragonBridge.Breath b = log[0];
                    check("never below zero (" + bridge().oilOf(agent) + ")", bridge().oilOf(agent) == 0);
                    check("nothing more paid (" + (b.eventsPaid + b.chargesPaid) + ")", b.eventsPaid + b.chargesPaid == reserve);
                    long late = Guard.blows(player).stream()
                            .filter(x -> x.fire() && dragon[0].equals(x.attacker()) && x.tick() > zeroAt[0] + 40)
                            .count();
                    check("no fire from it reaches the player once it is dry (" + late + " blows)", late == 0);
                    record(test, "dry", "300 ticks nudged to fire after the reserve ran out: " + b.eventsCancelled + " fire events cancelled, "
                            + b.chargesStopped + " charges stopped, " + late + " fire blows on the player");
                    mode[0] = Fire.MELEE;
                    // Set down beside the player: wherever its fire took it, Ice and Fire lifts a grounded dragon
                    // whose target is more than 5 blocks above or below it, and its melee goal never runs aloft.
                    DragonBaseEntity d = body(agent);
                    d.setFlying(false);
                    d.setHovering(false);
                    d.setDeltaMovement(Vec3.ZERO);
                    d.moveTo(player.getX() + 1.5, player.getY(), player.getZ(), d.getYRot(), 0f);
                })
                .thenWaitUntil(() -> {
                    long melee = Guard.blows(player).stream()
                            .filter(x -> !x.fire() && dragon[0].equals(x.attacker()) && x.tick() > zeroAt[0])
                            .count();
                    DragonBaseEntity d = bodyOrNull(agent);
                    String state = d == null ? "no body" : String.format("flying %b, hovering %b, on the ground %b, %.1f blocks away, %.1f above, "
                            + "ground attack %s, attack animation %b, can move %b, line of sight %b, target the player %b, "
                            + "ticks biting %d, shaking %d, other %d, reach %.1f",
                            d.isFlying(), d.isHovering(), d.onGround(), d.distanceTo(player), d.getY() - player.getY(), d.groundAttack,
                            d.isPlayingAttackAnimation(), d.canMove(), d.hasLineOfSight(player), d.getTarget() == player,
                            anim[0], anim[1], anim[2], d.getRenderSize() * 0.2574 * 2 + 2);
                    check("it fights on with bite and claws (" + melee + " blows; " + state + ")", melee > 0);
                })
                .thenWaitUntil(() -> check("its breaths reached the engine (" + breathsSent(agent) + " of " + reserve + ")",
                        breathsSent(agent) == reserve))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> sent = about(DragonRecords.DRAGON_BREATHS, agent);
                    for (int i = 1; i < sent.size(); i++) {
                        long gap = sent.get(i).view().getLong(Obs.GAME_TIME) - sent.get(i - 1).view().getLong(Obs.GAME_TIME);
                        check("one DRAGON_BREATHS per 20 ticks at most (" + gap + ")", gap >= DragonBridge.BREATH_INTERVAL);
                    }
                    for (ScriptedEngine.Received r : sent)
                        check("its epoch", r.view().getInt(DragonRecords.Breaths.EPOCH) == 1);
                    Guard.Blow first = Guard.blows(player).stream()
                            .filter(x -> !x.fire() && dragon[0].equals(x.attacker()) && x.tick() > zeroAt[0])
                            .findFirst().orElseThrow();
                    record(test, "melee", "first bite or claw " + (first.tick() - zeroAt[0]) + " ticks after the reserve ran out ("
                            + first.type() + ", " + first.amount() + "); " + sent.size() + " DRAGON_BREATHS records for " + reserve + " breaths");
                    body(agent).discard();
                })
                .thenWaitUntil(() -> check("captured", !about(DragonRecords.DRAGON_DEMATERIALIZED, agent).isEmpty()))
                .thenExecute(() -> {
                    release(player);
                    unforce.run();
                })
                .thenSucceed();
    }

    /**
     * Breaths are summed into DRAGON_BREATHS every 20 ticks, and the ones not yet sent go out before the
     * body's DRAGON_DEMATERIALIZED: a dragon with oil breathes, has breaths waiting, and is removed; its
     * last DRAGON_BREATHS carries exactly the waiting ones, journaled just before its capture.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 2400, batch = "oil_breaths_flush")
    public static void breathsReachTheEngineBeforeTheBodyGoes(GameTestHelper helper) {
        String test = "breathsReachTheEngineBeforeTheBodyGoes";
        long agent = DragonKit.newAgent();
        Runnable unforce = DragonKit.forceAround(helper, 4);
        Vec3 c = DragonKit.centre(helper);
        ServerPlayer player = guardedPlayer(helper, c, "oil-flush");
        DragonKit.materialize(hostile(agent, helper, c.add(8, 6, 0), player.getUUID()), 100);
        int[] waiting = {0};
        DragonBridge.Breath[] log = new DragonBridge.Breath[1];
        helper.onEachTick(() -> nudge(bodyOrNull(agent), player, Fire.GROUND));

        helper.startSequence()
                .thenWaitUntil(() -> body(agent))
                .thenExecute(() -> log[0] = bridge().breathOf(agent))
                .thenWaitUntil(() -> {
                    check("a DRAGON_BREATHS went out while it fought", !about(DragonRecords.DRAGON_BREATHS, agent).isEmpty());
                    check("and breaths wait for the next (" + bridge().pendingBreathsOf(agent) + ")", bridge().pendingBreathsOf(agent) > 0);
                })
                .thenExecute(() -> {
                    waiting[0] = bridge().pendingBreathsOf(agent);
                    body(agent).discard(); // its body is gone: the bridge captures it on its next pass
                })
                .thenWaitUntil(() -> check("DRAGON_DEMATERIALIZED", !about(DragonRecords.DRAGON_DEMATERIALIZED, agent).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received gone = about(DragonRecords.DRAGON_DEMATERIALIZED, agent).get(0);
                    List<ScriptedEngine.Received> sent = about(DragonRecords.DRAGON_BREATHS, agent);
                    ScriptedEngine.Received last = sent.get(sent.size() - 1);
                    check("the last DRAGON_BREATHS carries the waiting " + waiting[0] + " (" + last.aux() + ")", last.aux() == waiting[0]);
                    check("journaled before the capture", last.obsSeq() < gone.obsSeq());
                    check("in the capture's tick", last.view().getLong(Obs.GAME_TIME) == gone.view().getLong(Obs.GAME_TIME));
                    DragonBridge.Breath b = log[0];
                    check("every paid breath sent (" + breathsSent(agent) + " of " + (b.eventsPaid + b.chargesPaid) + ")",
                            breathsSent(agent) == b.eventsPaid + b.chargesPaid && b.reported == breathsSent(agent));
                    record(test, "flushed", sent.size() + " DRAGON_BREATHS, " + breathsSent(agent) + " breaths, the last " + last.aux()
                            + " journaled before DRAGON_DEMATERIALIZED (obsSeq " + last.obsSeq() + " < " + gone.obsSeq() + ")");
                    release(player);
                    unforce.run();
                })
                .thenSucceed();
    }

    /**
     * How many fire events each kind of Ice and Fire fire posts (OILGATE.md §4.3: a point an event): a
     * dragon whose body carries no reserve is nudged through the ground fire, the hover blast and the
     * scorch stream in turn while it fights a player, and its bursts (events less than six ticks apart, and
     * the charges among them) are logged per kind. A body without the oil flag is never counted: no
     * DRAGON_BREATHS, nothing cancelled.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 3200, batch = "oil_breaths_count")
    public static void fireEventsPerStreamAndPerCharge(GameTestHelper helper) {
        String test = "fireEventsPerStreamAndPerCharge";
        long agent = DragonKit.newAgent();
        Runnable unforce = DragonKit.forceAround(helper, 4);
        Vec3 c = DragonKit.centre(helper);
        ServerPlayer player = guardedPlayer(helper, c, "oil-count");
        DragonKit.materialize(hostile(agent, helper, c.add(8, 6, 0), player.getUUID()));
        Fire[] mode = {Fire.GROUND};
        long[] phaseStart = {0};
        List<String> summary = new ArrayList<>();
        DragonBridge.Breath[] log = new DragonBridge.Breath[1];
        helper.onEachTick(() -> nudge(bodyOrNull(agent), player, mode[0]));
        Runnable endPhase = () -> {
            DragonBridge.Breath b = log[0];
            List<DragonBridge.Burst> mine = b.bursts.stream().filter(x -> x.start >= phaseStart[0]).toList();
            String s = mode[0] + ": " + mine.size() + " bursts " + mine;
            summary.add(s);
            record(test, mode[0].name().toLowerCase() + " fire", s);
        };

        helper.startSequence()
                .thenWaitUntil(() -> body(agent))
                .thenExecute(() -> {
                    log[0] = bridge().breathOf(agent);
                    check("no reserve: it breathes freely (" + bridge().oilOf(agent) + ")", bridge().oilOf(agent) == -1);
                    phaseStart[0] = helper.getLevel().getGameTime();
                    record(test, "dragon", body(agent).getAgeInDays() + " days old (a stream lasts at most age x maxBreathTimeMul ticks)");
                })
                .thenIdle(800)
                .thenExecute(() -> {
                    endPhase.run();
                    mode[0] = Fire.HOVER;
                    phaseStart[0] = helper.getLevel().getGameTime();
                })
                .thenIdle(800)
                .thenExecute(() -> {
                    endPhase.run();
                    mode[0] = Fire.SCORCH;
                    phaseStart[0] = helper.getLevel().getGameTime();
                })
                .thenIdle(800)
                .thenExecute(() -> {
                    endPhase.run();
                    DragonBridge.Breath b = log[0];
                    check("it breathed fire (" + b.events + " events, " + b.bursts.size() + " bursts)", b.events > 0);
                    check("a body without the oil flag is never counted (" + breathsSent(agent) + " sent, " + b.eventsCancelled
                            + " cancelled)", breathsSent(agent) == 0 && b.eventsPaid == 0 && b.eventsCancelled == 0 && b.chargesStopped == 0);
                    record(test, "totals", b.events + " fire events, " + (b.chargesWithEvent + b.chargesPaid) + " charges ("
                            + b.chargesWithEvent + " on a tick with an event), none counted: " + String.join("; ", summary));
                    body(agent).discard();
                })
                .thenWaitUntil(() -> check("captured", !about(DragonRecords.DRAGON_DEMATERIALIZED, agent).isEmpty()))
                .thenExecute(() -> {
                    release(player);
                    unforce.run();
                })
                .thenSucceed();
    }
}
