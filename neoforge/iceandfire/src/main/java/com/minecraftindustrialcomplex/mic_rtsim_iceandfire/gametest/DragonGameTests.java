package com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest;

import com.iafenvoy.iceandfire.entity.DragonBaseEntity;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.AgentTag;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.village.Village;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonRecords;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.IafDragons;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.Lairs;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.MicRtsimIceAndFire;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.MobSpawnType;
import net.minecraft.world.entity.animal.Cow;
import net.minecraft.world.entity.animal.Pig;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.ChestBlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.IntegerProperty;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.UUID;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ThreadLocalRandom;

import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.about;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.body;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.bridge;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.check;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.engine;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.record;

/**
 * The Ice and Fire plugin on a server with Ice and Fire CE 2.1-beta.1 and the pack's own Ice and Fire
 * config, against the session's engine (native or stub). Each test has its own batch, so dragons of two
 * tests never meet. Every test logs what it compared on a {@code [gametest]} line.
 */
@GameTestHolder(MicRtsimIceAndFire.MODID)
@PrefixGameTestTemplate(false)
public final class DragonGameTests {
    private DragonGameTests() {}

    private static double flat(Entity e, Vec3 to) {
        double dx = to.x - e.getX(), dz = to.z - e.getZ();
        return Math.sqrt(dx * dx + dz * dz);
    }

    private static EntityType<?> fire() {
        return BuiltInRegistries.ENTITY_TYPE.get(ResourceLocation.parse(DragonKit.FIRE));
    }

    /**
     * DRAGON_MATERIALIZE writes the engine's state into Ice and Fire's dragon before it joins (AgeTicks
     * with aging disabled, Hunger, Gender, Variant as the colour's name, health, home), tags it and makes
     * it persistence-required; it flies to its first target; DRAGON_STEER sets a new target; DRAGON_DEPART
     * with no player in range captures it (DRAGON_DEMATERIALIZED, reason 1, with health, hunger, age and
     * position) and removes the body. DRAGON_STATE goes out every 100 ticks while it is embodied.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 1200, batch = "dragon_materialize")
    public static void aDragonMaterializesWithTheEnginesStateAndSteersToItsTarget(GameTestHelper helper) {
        String test = "aDragonMaterializesWithTheEnginesStateAndSteersToItsTarget";
        long agent = DragonKit.newAgent();
        Runnable unforce = DragonKit.forceAround(helper, 3);
        Vec3 c = DragonKit.centre(helper);
        // Morning: a fire dragon lands to sleep at night (Ice and Fire wakes it by Level.isDay), and a GameTest
        // server's daylight cycle runs, so without this the suite's length before this test decides the hour.
        long dayWas = helper.getLevel().getDayTime();
        helper.getLevel().setDayTime(1000);
        // A live attack target wins over steering (Ice and Fire flies at it): an earlier test's mob near the
        // arena (a natural dragon is left alone, OG-1) would pull the dragon away, so none is left.
        List<String> strays = new ArrayList<>();
        for (Entity e : helper.getLevel().getEntities((Entity) null, new net.minecraft.world.phys.AABB(BlockPos.containing(c)).inflate(128, 200, 128),
                e -> e instanceof net.minecraft.world.entity.Mob && !RtsimApi.isPuppet(e))) {
            strays.add(BuiltInRegistries.ENTITY_TYPE.getKey(e.getType()) + " at " + e.blockPosition().toShortString());
            e.discard();
        }
        Vec3 at = c.add(0, 12, 0);
        Vec3 first = c.add(28, 12, 0);
        Vec3 second = c.add(-20, 12, 8);
        BlockPos home = BlockPos.containing(c).offset(3, 0, -2);
        DragonKit.materialize(new DragonKit.Spawn(agent, 1, DragonKit.FIRE, DragonKit.dimension(helper), at, DragonKit.AGE_30_DAYS,
                37, true, 2, 60f, home, first, null, 0));
        double[] start = new double[1];
        Entity[] body = new Entity[1];

        helper.startSequence()
                .thenWaitUntil(() -> body(agent))
                .thenWaitUntil(() -> check("DRAGON_MATERIALIZED reached the engine", !about(DragonRecords.DRAGON_MATERIALIZED, agent).isEmpty()))
                .thenExecute(() -> {
                    DragonBaseEntity d = body(agent);
                    body[0] = d;
                    ScriptedEngine.Received m = about(DragonRecords.DRAGON_MATERIALIZED, agent).get(0);
                    check("MATERIALIZED names the body", DragonKit.uuidAt(m, DragonRecords.Materialized.ENTITY_MOST).equals(d.getUUID()));
                    check("epoch 1", m.view().getInt(DragonRecords.Materialized.EPOCH) == 1);
                    check("its dimension", DragonKit.dimension(helper).equals(engine().javaDictName(m.view().getInt(DragonRecords.Materialized.DIM))));
                    check("AgeTicks is the engine's (" + d.getAgeInTicks() + ")", d.getAgeInTicks() == DragonKit.AGE_30_DAYS);
                    check("AgingDisabled: the engine owns age", d.isAgingDisabled());
                    check("Hunger 37 (" + d.getHunger() + ")", d.getHunger() == 37);
                    check("Gender male", d.isMale());
                    check("Variant 2 of fire is bronze (" + d.getVariant() + ")", "bronze".equals(d.getVariant()) && IafDragons.variant(d) == 2);
                    check("health 60 (" + d.getHealth() + " of " + d.getMaxHealth() + ")", Math.abs(d.getHealth() - 60f) < 0.01f);
                    check("max health from age 30 days (" + d.getMaxHealth() + ")", d.getMaxHealth() > 60f && d.getMaxHealth() < 500f);
                    check("its home is the engine's (" + IafDragons.home(d) + ")", home.equals(IafDragons.home(d)));
                    check("a puppet", RtsimApi.isPuppet(d) && RtsimAttachments.tag(d).agent() == agent);
                    check("persistence required", d.isPersistenceRequired());
                    check("never saved by vanilla", !d.shouldBeSaved());
                    check("flying to its first target", d.isFlying() && IafDragons.flightTarget(d).distanceTo(first) < 0.01);
                    start[0] = flat(d, first);
                    record(test, "materialized", "age " + d.getAgeInTicks() + ", hunger " + d.getHunger() + ", " + d.getVariant()
                            + ", health " + d.getHealth() + "/" + d.getMaxHealth() + ", " + start[0] + " blocks from its target; strays removed "
                            + strays + "; day time was " + dayWas + ", set to 1000; " + motion(helper, d));
                })
                .thenWaitUntil(() -> {
                    DragonBaseEntity d = body(agent);
                    check("it flies toward its target (" + flat(d, first) + " of " + start[0] + " blocks left, at " + d.blockPosition().toShortString()
                            + ", flight target " + IafDragons.flightTarget(d) + ", attack target " + d.getTarget() + ", " + motion(helper, d) + ")",
                            flat(d, first) < start[0] - 8);
                })
                .thenExecute(() -> {
                    record(test, "first target", "closed from " + start[0] + " to " + flat(body(agent), first) + " blocks; " + motion(helper, body(agent)));
                    DragonKit.steer(agent, 1, 0, second);
                })
                .thenWaitUntil(() -> check("DRAGON_STEER set its flight target", IafDragons.flightTarget(body(agent)).distanceTo(second) < 0.01))
                .thenExecute(() -> start[0] = flat(body(agent), second))
                .thenWaitUntil(() -> {
                    DragonBaseEntity d = body(agent);
                    check("it turns to the new target (" + flat(d, second) + " of " + start[0] + ", " + motion(helper, d) + ")",
                            flat(d, second) < start[0] - 4 || flat(d, second) <= 6.5);
                })
                .thenWaitUntil(() -> check("DRAGON_STATE while embodied", !DragonKit.received(DragonRecords.DRAGON_STATE, DragonRecords.State.AGENT, agent).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received s = DragonKit.received(DragonRecords.DRAGON_STATE, DragonRecords.State.AGENT, agent).get(0);
                    check("its state's hunger", s.view().getInt(DragonRecords.State.HUNGER) >= 36);
                    check("its state's age", s.view().getInt(DragonRecords.State.AGE_TICKS) == DragonKit.AGE_30_DAYS);
                    check("its state says ticking", (s.aux() & DragonRecords.State.FLAG_TICKING) != 0);
                    record(test, "state", "flags " + s.aux() + ", health " + s.view().getFloat(DragonRecords.State.HEALTH));
                    DragonKit.depart(agent, 1, 1, 40, c.add(-400, 40, 0));
                })
                .thenWaitUntil(() -> check("captured on DEPART", !about(DragonRecords.DRAGON_DEMATERIALIZED, agent).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received r = about(DragonRecords.DRAGON_DEMATERIALIZED, agent).get(0);
                    var v = r.view();
                    check("reason 1, a command (" + r.aux() + ")", r.aux() == DragonRecords.Dematerialized.COMMAND);
                    check("its health (" + v.getFloat(DragonRecords.Dematerialized.HEALTH) + ")", v.getFloat(DragonRecords.Dematerialized.HEALTH) >= 59f);
                    check("its max health", v.getFloat(DragonRecords.Dematerialized.MAX_HEALTH) > 60f);
                    check("its hunger (" + v.getInt(DragonRecords.Dematerialized.HUNGER) + ")", v.getInt(DragonRecords.Dematerialized.HUNGER) >= 36
                            && v.getInt(DragonRecords.Dematerialized.HUNGER) <= 37);
                    check("its age", v.getInt(DragonRecords.Dematerialized.AGE_TICKS) == DragonKit.AGE_30_DAYS);
                    Vec3 where = new Vec3(v.getDouble(DragonRecords.Dematerialized.X), v.getDouble(DragonRecords.Dematerialized.Y), v.getDouble(DragonRecords.Dematerialized.Z));
                    check("its position (" + where + ")", where.distanceTo(body[0].position()) < 0.01);
                    check("the body is gone", body[0].isRemoved() && bridge().body(agent) == null);
                    record(test, "captured", "health " + v.getFloat(DragonRecords.Dematerialized.HEALTH) + ", hunger "
                            + v.getInt(DragonRecords.Dematerialized.HUNGER) + ", age " + v.getInt(DragonRecords.Dematerialized.AGE_TICKS) + " at " + where);
                    unforce.run();
                })
                .thenSucceed();
    }

    /** What a stalled flight needs to be told apart: the test's tick, flying, entity ticking, AI, speed. */
    private static String motion(GameTestHelper helper, DragonBaseEntity d) {
        return "test tick " + helper.getTick() + ", day time " + helper.getLevel().getDayTime() + ", flying " + d.isFlying() + ", entity ticking "
                + helper.getLevel().isPositionEntityTicking(d.blockPosition()) + ", no AI " + d.isNoAi() + ", motion "
                + d.getDeltaMovement();
    }

    /**
     * A player's two hits in one tick are one DRAGON_HURT (hits 2, the player's UUID, health after); a
     * player's kill is a DRAGON_DIED naming the killer, at the end of the tick, after the lethal blow's
     * DRAGON_HURT. Ice and Fire keeps the body as a corpse to harvest; it is no longer a live puppet.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 400, batch = "dragon_hurt")
    public static void aPlayersHitIsHurtAndAKillIsDiedWithTheKiller(GameTestHelper helper) {
        String test = "aPlayersHitIsHurtAndAKillIsDiedWithTheKiller";
        long agent = DragonKit.newAgent();
        Vec3 c = DragonKit.centre(helper);
        DragonKit.materialize(DragonKit.young(agent, helper, c, DragonRecords.Materialize.FLAG_SLEEPING));
        ServerPlayer[] player = new ServerPlayer[1];
        DragonBaseEntity[] dragon = new DragonBaseEntity[1];

        helper.startSequence()
                .thenWaitUntil(() -> body(agent))
                .thenExecute(() -> {
                    DragonBaseEntity d = body(agent);
                    dragon[0] = d;
                    record(test, "materialized", "asleep " + d.isSleeping() + ", flying " + d.isFlying());
                    player[0] = FakePlayers.add(helper.getLevel(), c.add(4, 0, 0), "dragon-hunter");
                    player[0].setGameMode(GameType.CREATIVE);
                    DamageSource hit = helper.getLevel().damageSources().playerAttack(player[0]);
                    d.hurt(hit, 6f);
                    d.invulnerableTime = 0;
                    d.hurt(hit, 9f);
                })
                .thenWaitUntil(() -> check("DRAGON_HURT reached the engine", !about(DragonRecords.DRAGON_HURT, agent).isEmpty()))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> hurts = about(DragonRecords.DRAGON_HURT, agent);
                    check("one record for the tick (" + hurts.size() + ")", hurts.size() == 1);
                    var v = hurts.get(0).view();
                    check("by a player", hurts.get(0).aux() == DragonRecords.ATTACKER_PLAYER);
                    check("the player's UUID", DragonKit.uuidAt(hurts.get(0), DragonRecords.Hurt.ATTACKER_MOST).equals(player[0].getUUID()));
                    check("two hits (" + v.getInt(DragonRecords.Hurt.HITS) + ")", v.getInt(DragonRecords.Hurt.HITS) == 2);
                    check("an amount (" + v.getFloat(DragonRecords.Hurt.AMOUNT) + ")", v.getFloat(DragonRecords.Hurt.AMOUNT) > 0f
                            && v.getFloat(DragonRecords.Hurt.AMOUNT) <= 15f);
                    check("health after (" + v.getFloat(DragonRecords.Hurt.HEALTH_AFTER) + ")",
                            Math.abs(v.getFloat(DragonRecords.Hurt.HEALTH_AFTER) - dragon[0].getHealth()) < 0.5f);
                    record(test, "hurt", v.getInt(DragonRecords.Hurt.HITS) + " hits, " + v.getFloat(DragonRecords.Hurt.AMOUNT) + " damage, "
                            + v.getFloat(DragonRecords.Hurt.HEALTH_AFTER) + " left");
                    dragon[0].invulnerableTime = 0;
                    dragon[0].hurt(helper.getLevel().damageSources().playerAttack(player[0]), 100_000f);
                })
                .thenWaitUntil(() -> check("DRAGON_DIED reached the engine", !about(DragonRecords.DRAGON_DIED, agent).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received died = about(DragonRecords.DRAGON_DIED, agent).get(0);
                    check("killed by a player", died.aux() == DragonRecords.ATTACKER_PLAYER);
                    check("the killer's UUID", DragonKit.uuidAt(died, DragonRecords.Died.KILLER_MOST).equals(player[0].getUUID()));
                    check("where it died", Math.abs(died.view().getDouble(DragonRecords.Died.X) - dragon[0].getX()) < 1.0);
                    List<ScriptedEngine.Received> hurts = about(DragonRecords.DRAGON_HURT, agent);
                    check("the lethal blow is a wound first (" + hurts.size() + ")", hurts.size() == 2 && hurts.get(1).obsSeq() < died.obsSeq());
                    check("no longer a live puppet", bridge().body(agent) == null);
                    check("Ice and Fire keeps a corpse (or removed it)", dragon[0].isRemoved() || dragon[0].isModelDead() || dragon[0].isDeadOrDying());
                    check("the corpse waits, tagged, for the engine's save", dragon[0].isRemoved() || bridge().isCorpse(dragon[0].getUUID()));
                    record(test, "died", "killer " + DragonKit.uuidAt(died, DragonRecords.Died.KILLER_MOST) + ", corpse " + !dragon[0].isRemoved());
                    FakePlayers.remove(player[0]);
                    if (!dragon[0].isRemoved())
                        dragon[0].discard();
                })
                .thenSucceed();
    }

    /**
     * Two dragons another path spawned: a spawn-egg one and a natural one (In Control's spawner spawns
     * with NATURAL). Both are reported DRAGON_UNTAGGED with their spawn kind and Ice and Fire's state;
     * the engine adopts the egg dragon (DRAGON_ADOPT: tagged, persistence-required, DRAGON_MATERIALIZED
     * naming it) and leaves the natural one alone (R18 as amended by OG-1 and P13; adopt_natural_dragons
     * is false, and field flights are off in GameTests, so nothing replaces In Control's dragon: with them
     * on, a natural fire dragon is discarded, OGB-6 and OGC66, which the engine's own tests cover): it is
     * never adopted, discarded or tagged, and stays a plain Ice and Fire mob that is saved as usual. On the
     * native engine the engine decides; on the stub the test answers as the engine would (the egg only).
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 400, batch = "dragon_untagged")
    public static void untaggedEggDragonIsAdoptedAndNaturalOneLeftAlone(GameTestHelper helper) {
        String test = "untaggedEggDragonIsAdoptedAndNaturalOneLeftAlone";
        ServerLevel level = helper.getLevel();
        BlockPos p = BlockPos.containing(DragonKit.centre(helper));
        Entity egg = fire().spawn(level, p.offset(-2, 0, 0), MobSpawnType.SPAWN_EGG);
        Entity natural = fire().spawn(level, p.offset(2, 0, 0), MobSpawnType.NATURAL);
        check("both spawned", egg instanceof DragonBaseEntity && natural instanceof DragonBaseEntity);
        boolean naturalPersistent = ((DragonBaseEntity) natural).isPersistenceRequired();
        long stubAgent = DragonKit.newAgent();
        long[] adopted = new long[2];

        helper.startSequence()
                .thenWaitUntil(() -> check("both reported untagged", !DragonKit.untagged(egg.getUUID()).isEmpty()
                        && !DragonKit.untagged(natural.getUUID()).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received e = DragonKit.untagged(egg.getUUID()).get(0);
                    ScriptedEngine.Received n = DragonKit.untagged(natural.getUUID()).get(0);
                    DragonBaseEntity d = (DragonBaseEntity) egg;
                    check("spawn egg (" + e.aux() + ")", e.aux() == DragonRecords.Untagged.SPAWN_EGG);
                    check("natural (" + n.aux() + ")", n.aux() == DragonRecords.Untagged.SPAWN_NATURAL);
                    var v = e.view();
                    check("the species", DragonKit.FIRE.equals(engine().javaDictName(v.getInt(DragonRecords.Untagged.SPECIES))));
                    // It ages (and may eat) on its own until adopted: the report is its state when it joined.
                    int age = v.getInt(DragonRecords.Untagged.AGE_TICKS);
                    check("its age (" + age + ", now " + d.getAgeInTicks() + ")", age <= d.getAgeInTicks() && age > d.getAgeInTicks() - 400);
                    check("its hunger (" + v.getInt(DragonRecords.Untagged.HUNGER) + ")", v.getInt(DragonRecords.Untagged.HUNGER) >= 0
                            && v.getInt(DragonRecords.Untagged.HUNGER) <= 100);
                    check("its gender", v.getInt(DragonRecords.Untagged.GENDER) == (d.isMale() ? 1 : 0));
                    check("its variant", v.getInt(DragonRecords.Untagged.VARIANT) == IafDragons.variant(d));
                    check("not tamed, in no lair", (v.getInt(DragonRecords.Untagged.FLAGS) & DragonRecords.Untagged.FLAG_TAMED) == 0
                            && v.getLong(DragonRecords.Untagged.LAIR) == 0);
                    record(test, "untagged", "egg dragon " + d.getVariant() + " age " + d.getAgeInTicks() + ", natural dragon");
                    if (!DragonKit.nativeEngine()) {
                        DragonKit.adopt(egg.getUUID(), stubAgent, 1, null);
                    }
                })
                .thenWaitUntil(() -> check("the egg dragon was adopted", RtsimApi.isPuppet(egg) && bridge().agentOf(egg) != 0))
                // The engine answers in the step that adopted the egg dragon; give a discard time to show.
                .thenIdle(40)
                .thenExecute(() -> {
                    DragonBaseEntity n = (DragonBaseEntity) natural;
                    check("the natural dragon was left alone (still in the world)", !natural.isRemoved() && natural.isAlive());
                    check("no tag, not a puppet, no agent", !RtsimAttachments.isTagged(natural) && !RtsimApi.isPuppet(natural)
                            && bridge().agentOf(natural) == 0);
                    check("saving unchanged: still saved, persistence as spawned",
                            n.shouldBeSaved() && n.isPersistenceRequired() == naturalPersistent && !n.isAgingDisabled());
                    check("no DRAGON_MATERIALIZED names it", DragonKit.received(
                            DragonRecords.DRAGON_MATERIALIZED, DragonRecords.Materialized.ENTITY_MOST, natural.getUUID().getMostSignificantBits()).isEmpty());
                    // Left alone by the engine; the test removes it so it fights nothing in a later test's arena.
                    natural.discard();
                })
                .thenExecute(() -> {
                    long agent = bridge().agentOf(egg);
                    adopted[0] = agent;
                    check("a dragon agent", DragonRecords.isDragonAgent(agent));
                    List<ScriptedEngine.Received> m = about(DragonRecords.DRAGON_MATERIALIZED, agent);
                    check("DRAGON_MATERIALIZED names the adopted body", !m.isEmpty()
                            && DragonKit.uuidAt(m.get(0), DragonRecords.Materialized.ENTITY_MOST).equals(egg.getUUID()));
                    adopted[1] = m.get(0).view().getInt(DragonRecords.Materialized.EPOCH);
                    DragonBaseEntity d = (DragonBaseEntity) egg;
                    check("adopted: persistence required, aging the engine's, never saved", d.isPersistenceRequired() && d.isAgingDisabled() && !d.shouldBeSaved());
                    record(test, "answers", "egg dragon adopted as " + Long.toHexString(agent) + " epoch " + adopted[1]
                            + ", natural dragon left alone (" + (DragonKit.nativeEngine() ? "the engine decided" : "scripted") + ")");
                    DragonKit.depart(agent, (int) adopted[1], 1, 20, DragonKit.centre(helper).add(-400, 40, 0));
                })
                .thenWaitUntil(() -> check("the adopted dragon leaves", !about(DragonRecords.DRAGON_DEMATERIALIZED, adopted[0]).isEmpty()))
                .thenSucceed();
    }

    /**
     * RP-245 (audit row D8): the engine adopts a dragon again (adopt_again) under a newer epoch, and the body Java holds
     * for the agent at the older one is not its body any more. A puppet of the agent at epoch 1 stands; DRAGON_ADOPT of
     * another untagged dragon at epoch 2 releases it (captured with reason COMMAND at epoch 1, the body removed) and
     * adopts the new one: one body of the agent is alive, tagged epoch 2, MATERIALIZED at epoch 2 names it. The cases that
     * keep ALREADY_EMBODIED: an adopt at the same epoch, at an older one, and at a newer one whose target is already a
     * puppet (of another agent): each is rejected, and none releases the held puppet.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 600, batch = "dragon_adopt_supersede")
    public static void anAdoptAtANewerEpochReleasesTheOlderPuppet(GameTestHelper helper) {
        String test = "anAdoptAtANewerEpochReleasesTheOlderPuppet";
        ServerLevel level = helper.getLevel();
        long agent = DragonKit.newAgent(), other = DragonKit.newAgent();
        Runnable unforce = DragonKit.forceAround(helper, 3);
        Vec3 c = DragonKit.centre(helper);
        BlockPos p = BlockPos.containing(c);
        List<Entity> made = new ArrayList<>();
        DragonBaseEntity[] old = {null};
        Entity[] next = {null}, same = {null}, older = {null}, theirs = {null};
        long[] before = new long[2];
        DragonKit.onEnd(helper, () -> {
            made.forEach(e -> {
                if (!e.isRemoved())
                    e.discard();
            });
            for (long a : new long[] {agent, other}) {
                Entity b = bridge().body(a);
                if (b != null && !b.isRemoved())
                    b.discard();
            }
            unforce.run();
        });
        DragonKit.materialize(DragonKit.young(agent, helper, c.add(0, 12, 0), 0));
        DragonKit.materialize(DragonKit.young(other, helper, c.add(12, 12, 0), 0));
        helper.startSequence()
                .thenWaitUntil(() -> body(agent))
                .thenWaitUntil(() -> body(other))
                .thenExecute(() -> {
                    old[0] = body(agent);
                    theirs[0] = body(other);
                    check("the held puppet is at epoch 1 (" + RtsimAttachments.tag(old[0]).epoch() + ")", RtsimAttachments.tag(old[0]).epoch() == 1);
                    next[0] = fire().spawn(level, p.offset(-8, 0, 0), MobSpawnType.NATURAL);
                    same[0] = fire().spawn(level, p.offset(-8, 0, 6), MobSpawnType.NATURAL);
                    older[0] = fire().spawn(level, p.offset(-8, 0, -6), MobSpawnType.NATURAL);
                    check("three untagged dragons spawned", next[0] instanceof DragonBaseEntity && same[0] instanceof DragonBaseEntity
                            && older[0] instanceof DragonBaseEntity);
                    made.add(next[0]);
                    made.add(same[0]);
                    made.add(older[0]);
                    before[0] = bridge().superseded;
                    before[1] = bridge().rejected;
                    // The engine adopts again under epoch 2: the stale puppet at epoch 1 is released, the new body adopted.
                    DragonKit.adopt(next[0].getUUID(), agent, 2, null);
                })
                .thenWaitUntil(() -> check("the new body adopted at epoch 2", RtsimApi.isPuppet(next[0]) && bridge().agentOf(next[0]) == agent))
                .thenExecute(() -> {
                    check("at epoch 2 (" + RtsimAttachments.tag(next[0]).epoch() + ")", RtsimAttachments.tag(next[0]).epoch() == 2);
                    check("the older puppet released", old[0].isRemoved());
                    check("counted as superseded (" + (bridge().superseded - before[0]) + ")", bridge().superseded == before[0] + 1);
                    check("nothing rejected (" + (bridge().rejected - before[1]) + ")", bridge().rejected == before[1]);
                    check("the agent's body is the new one", bridge().body(agent) == next[0]);
                    int alive = 0;
                    for (DragonBaseEntity d : level.getEntitiesOfClass(DragonBaseEntity.class, new net.minecraft.world.phys.AABB(p).inflate(128, 200, 128)))
                        if (!d.isRemoved() && RtsimAttachments.isTagged(d) && RtsimAttachments.tag(d).agent() == agent)
                            alive++;
                    check("one body of the agent alive (" + alive + ")", alive == 1);
                    List<ScriptedEngine.Received> gone = about(DragonRecords.DRAGON_DEMATERIALIZED, agent);
                    check("one DEMATERIALIZED, at epoch 1, reason COMMAND (" + gone.size() + ")", gone.size() == 1
                            && gone.get(0).view().getInt(DragonRecords.Dematerialized.EPOCH) == 1 && gone.get(0).aux() == DragonRecords.Dematerialized.COMMAND);
                    List<ScriptedEngine.Received> m = about(DragonRecords.DRAGON_MATERIALIZED, agent);
                    check("MATERIALIZED twice (" + m.size() + ")", m.size() == 2);
                    ScriptedEngine.Received last = m.get(m.size() - 1);
                    check("the last at epoch 2, naming the new body", last.view().getInt(DragonRecords.Materialized.EPOCH) == 2
                            && DragonKit.uuidAt(last, DragonRecords.Materialized.ENTITY_MOST).equals(next[0].getUUID()));
                    // The cases that keep ALREADY_EMBODIED: the same epoch, an older one, a newer one at a target that is a puppet.
                    DragonKit.adopt(same[0].getUUID(), agent, 2, null);
                    DragonKit.adopt(older[0].getUUID(), agent, 1, null);
                    DragonKit.adopt(theirs[0].getUUID(), agent, 3, null);
                })
                .thenWaitUntil(() -> check("three rejections (" + (bridge().rejected - before[1]) + ")", bridge().rejected == before[1] + 3))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> r = about(DragonRecords.DRAGON_SPAWN_REJECTED, agent);
                    check("three SPAWN_REJECTED for the agent (" + r.size() + ")", r.size() == 3);
                    for (ScriptedEngine.Received x : r)
                        check("each ALREADY_EMBODIED (" + x.aux() + ")", x.aux() == DragonRecords.SpawnRejected.ALREADY_EMBODIED);
                    check("none released the held puppet (superseded " + (bridge().superseded - before[0]) + ")",
                            bridge().superseded == before[0] + 1 && !next[0].isRemoved() && bridge().body(agent) == next[0]);
                    check("the other agent's puppet is untouched", !theirs[0].isRemoved() && bridge().body(other) == theirs[0]
                            && RtsimAttachments.tag(theirs[0]).agent() == other);
                    check("the refused dragons stay untagged", !RtsimAttachments.isTagged(same[0]) && !RtsimAttachments.isTagged(older[0]));
                    record(test, "supersede", "a puppet at epoch 1 and an ADOPT of another dragon at epoch 2: the old body released (DEMATERIALIZED epoch 1, "
                            + "reason COMMAND), the new one adopted at epoch 2, one body alive; ADOPT at the same epoch, at an older one, and at a puppet "
                            + "of another agent: ALREADY_EMBODIED, the held puppet kept");
                })
                .thenSucceed();
    }

    /**
     * RP-246 (audit row D8, the fork): an ADOPT at a newer epoch whose target is gone (the dragon was removed before the
     * command was read) still releases the older puppet, since the engine no longer holds that body at its epoch, and is
     * then answered SPAWN_FAILED. A puppet of the agent at epoch 1 stands; a dragon is spawned and removed; DRAGON_ADOPT of
     * its UUID at epoch 2: the puppet is gone (one DEMATERIALIZED at epoch 1, reason COMMAND), {@code superseded} is up by
     * one, one DRAGON_SPAWN_REJECTED with SPAWN_FAILED names the agent at epoch 2, and the agent holds no body: the engine
     * resumes the dragon abstract, and no stale body is left beside it.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 600, batch = "dragon_adopt_failed")
    public static void anAdoptOfAGoneDragonReleasesTheOlderPuppetAndFails(GameTestHelper helper) {
        String test = "anAdoptOfAGoneDragonReleasesTheOlderPuppetAndFails";
        ServerLevel level = helper.getLevel();
        long agent = DragonKit.newAgent();
        Runnable unforce = DragonKit.forceAround(helper, 3);
        Vec3 c = DragonKit.centre(helper);
        BlockPos p = BlockPos.containing(c);
        DragonBaseEntity[] old = {null};
        UUID[] gone = {null};
        long[] before = new long[2];
        DragonKit.onEnd(helper, () -> {
            Entity b = bridge().body(agent);
            if (b != null && !b.isRemoved())
                b.discard();
            unforce.run();
        });
        DragonKit.materialize(DragonKit.young(agent, helper, c.add(0, 12, 0), 0));
        helper.startSequence()
                .thenWaitUntil(() -> body(agent))
                .thenExecute(() -> {
                    old[0] = body(agent);
                    check("the held puppet is at epoch 1 (" + RtsimAttachments.tag(old[0]).epoch() + ")", RtsimAttachments.tag(old[0]).epoch() == 1);
                    Entity target = fire().spawn(level, p.offset(-8, 0, 0), MobSpawnType.NATURAL);
                    check("an untagged dragon spawned", target instanceof DragonBaseEntity);
                    gone[0] = target.getUUID();
                    target.discard();
                    check("and removed before the command is read", target.isRemoved() && level.getEntity(gone[0]) == null);
                    before[0] = bridge().superseded;
                    before[1] = bridge().rejected;
                    DragonKit.adopt(gone[0], agent, 2, null);
                })
                .thenWaitUntil(() -> check("the adopt answered (" + (bridge().rejected - before[1]) + " rejected)", bridge().rejected == before[1] + 1))
                .thenExecute(() -> {
                    check("the older puppet released", old[0].isRemoved());
                    check("counted as superseded (" + (bridge().superseded - before[0]) + ")", bridge().superseded == before[0] + 1);
                    check("the agent holds no body", bridge().body(agent) == null);
                    List<ScriptedEngine.Received> r = about(DragonRecords.DRAGON_SPAWN_REJECTED, agent);
                    check("one SPAWN_REJECTED (" + r.size() + ")", r.size() == 1);
                    check("SPAWN_FAILED (" + r.get(0).aux() + ")", r.get(0).aux() == DragonRecords.SpawnRejected.SPAWN_FAILED);
                    check("at epoch 2", r.get(0).view().getInt(DragonRecords.SpawnRejected.EPOCH) == 2);
                    List<ScriptedEngine.Received> d = about(DragonRecords.DRAGON_DEMATERIALIZED, agent);
                    check("one DEMATERIALIZED, at epoch 1, reason COMMAND (" + d.size() + ")", d.size() == 1
                            && d.get(0).view().getInt(DragonRecords.Dematerialized.EPOCH) == 1 && d.get(0).aux() == DragonRecords.Dematerialized.COMMAND);
                    int alive = 0;
                    for (DragonBaseEntity x : level.getEntitiesOfClass(DragonBaseEntity.class, new net.minecraft.world.phys.AABB(p).inflate(128, 200, 128)))
                        if (!x.isRemoved() && RtsimAttachments.isTagged(x) && RtsimAttachments.tag(x).agent() == agent)
                            alive++;
                    check("no body of the agent alive (" + alive + ")", alive == 0);
                    record(test, "supersede-failed", "a puppet at epoch 1 and an ADOPT at epoch 2 of a dragon that is gone: the puppet released (DEMATERIALIZED "
                            + "epoch 1, reason COMMAND, superseded +1), SPAWN_REJECTED SPAWN_FAILED at epoch 2, the agent holds no body");
                })
                .thenSucceed();
    }

    /**
     * A lair's own dragon: a dragon that joins inside a lair's box with no spawn type (Ice and Fire's lair
     * pieces never call finalizeSpawn) is reported as a STRUCTURE spawn of that lair, after its LAIR_FOUND,
     * and adopted as its occupant with the lair's home. A player then robs the hoard: a gold pile of three
     * layers broken (6 gold nuggets, its loot table) and 4 gold ingots taken out of a chest inside the lair
     * are one HOARD_ROBBED record (the player's tick) with a Java op id.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 400, batch = "dragon_lair")
    public static void aLairsDragonIsAdoptedAndItsHoardRobbed(GameTestHelper helper) {
        String test = "aLairsDragonIsAdoptedAndItsHoardRobbed";
        ServerLevel level = helper.getLevel();
        Vec3 c = DragonKit.centre(helper);
        BoundingBox box = BoundingBox.fromCorners(helper.absolutePos(BlockPos.ZERO), helper.absolutePos(new BlockPos(10, 5, 10)));
        BlockPos home = BlockPos.containing(c);
        long lairId = 0x0E00_0000_0000_0000L | (ThreadLocalRandom.current().nextLong() & 0xFFFF_FFFF_FFFFL);
        Lairs.Lair lair = bridge().lairs.declareForTest(level, lairId, "iceandfire:fire_dragon_roost", box, home, level.getGameTime());
        DragonBaseEntity d = (DragonBaseEntity) fire().create(level);
        check("created", d != null);
        // As Ice and Fire's roost piece makes its dragon: no finalizeSpawn, aging disabled, hunger 50.
        d.setAgeInTicks(DragonKit.AGE_30_DAYS);
        d.setAgingDisabled(true);
        d.setHunger(50);
        d.setInSittingPose(true);
        d.moveTo(c.x, c.y, c.z, 0f, 0f);
        level.addFreshEntity(d);
        long stubAgent = DragonKit.newAgent();
        BlockPos pile = helper.absolutePos(new BlockPos(2, 2, 2));
        BlockPos chestPos = helper.absolutePos(new BlockPos(8, 2, 8));
        ServerPlayer[] player = new ServerPlayer[1];
        long[] agent = new long[2];

        helper.startSequence()
                .thenWaitUntil(() -> check("LAIR_FOUND and DRAGON_UNTAGGED reached the engine",
                        !DragonKit.received(DragonRecords.LAIR_FOUND, DragonRecords.LairFound.LAIR, lairId).isEmpty()
                                && !DragonKit.untagged(d.getUUID()).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received found = DragonKit.received(DragonRecords.LAIR_FOUND, DragonRecords.LairFound.LAIR, lairId).get(0);
                    ScriptedEngine.Received u = DragonKit.untagged(d.getUUID()).get(0);
                    check("the lair's structure", "iceandfire:fire_dragon_roost".equals(engine().javaDictName(found.view().getInt(DragonRecords.LairFound.STRUCTURE))));
                    check("the lair's box", found.view().getInt(DragonRecords.LairFound.MIN_X) == box.minX()
                            && found.view().getInt(DragonRecords.LairFound.MAX_Z) == box.maxZ());
                    check("the lair came first", found.obsSeq() < u.obsSeq());
                    check("a structure spawn (" + u.aux() + ")", u.aux() == DragonRecords.Untagged.SPAWN_STRUCTURE);
                    check("of this lair", u.view().getLong(DragonRecords.Untagged.LAIR) == lairId);
                    check("asleep, aging disabled", (u.view().getInt(DragonRecords.Untagged.FLAGS)
                            & (DragonRecords.Untagged.FLAG_SLEEPING | DragonRecords.Untagged.FLAG_AGING_DISABLED))
                            == (DragonRecords.Untagged.FLAG_SLEEPING | DragonRecords.Untagged.FLAG_AGING_DISABLED));
                    if (!DragonKit.nativeEngine())
                        DragonKit.adopt(d.getUUID(), stubAgent, 1, home);
                })
                // Adopted, and its DRAGON_MATERIALIZED sent (after OG-10's arms the engine can take a step longer).
                .thenWaitUntil(() -> check("adopted", bridge().agentOf(d) != 0
                        && !about(DragonRecords.DRAGON_MATERIALIZED, bridge().agentOf(d)).isEmpty()))
                .thenExecute(() -> {
                    agent[0] = bridge().agentOf(d);
                    agent[1] = about(DragonRecords.DRAGON_MATERIALIZED, agent[0]).get(0).view().getInt(DragonRecords.Materialized.EPOCH);
                    BlockPos h = IafDragons.home(d);
                    check("homed at its lair (" + h + ")", h != null && h.getX() == home.getX() && h.getZ() == home.getZ());
                    record(test, "occupant", "adopted as " + Long.toHexString(agent[0]) + ", home " + h);
                    // The hoard: a pile and a chest inside the lair, robbed by a player.
                    BlockState gold = BuiltInRegistries.BLOCK.get(ResourceLocation.parse("iceandfire:gold_pile")).defaultBlockState();
                    IntegerProperty layers = (IntegerProperty) gold.getBlock().getStateDefinition().getProperty("layers");
                    check("Ice and Fire's gold pile has layers", layers != null);
                    level.setBlockAndUpdate(pile, gold.setValue(layers, 3));
                    level.setBlockAndUpdate(chestPos, Blocks.CHEST.defaultBlockState());
                    ChestBlockEntity chest = (ChestBlockEntity) level.getBlockEntity(chestPos);
                    chest.setItem(0, new ItemStack(Items.GOLD_INGOT, 10));
                    player[0] = FakePlayers.add(level, c.add(1, 0, 1), "hoard-robber");
                    player[0].gameMode.destroyBlock(pile);
                    player[0].openMenu(chest);
                    chest.removeItem(0, 4);
                    player[0].closeContainer();
                })
                .thenWaitUntil(() -> check("HOARD_ROBBED reached the engine", !DragonKit.received(DragonRecords.HOARD_ROBBED, DragonRecords.HoardRobbed.LAIR, lairId).isEmpty()))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> robbed = DragonKit.received(DragonRecords.HOARD_ROBBED, DragonRecords.HoardRobbed.LAIR, lairId);
                    check("one record for the player's tick (" + robbed.size() + ")", robbed.size() == 1);
                    int nuggets = 0, ingots = 0;
                    for (ScriptedEngine.Received r : robbed) {
                        check("by the player", DragonKit.uuidAt(r, DragonRecords.HoardRobbed.PLAYER_MOST).equals(player[0].getUUID()));
                        check("a Java op id", (r.view().getLong(DragonRecords.HoardRobbed.OP_ID) & Obs.JAVA_OP_BIT) != 0);
                        for (int i = 0; i < r.aux(); i++) {
                            int at = DragonRecords.HoardRobbed.ITEMS + i * DragonRecords.HoardRobbed.ITEM;
                            String item = engine().javaDictName(r.view().getInt(at));
                            int count = r.view().getInt(at + 4);
                            if ("minecraft:gold_nugget".equals(item))
                                nuggets += count;
                            if ("minecraft:gold_ingot".equals(item))
                                ingots += count;
                        }
                    }
                    check("6 nuggets from the pile (" + nuggets + ")", nuggets == 6);
                    check("4 ingots from the chest (" + ingots + ")", ingots == 4);
                    record(test, "robbed", nuggets + " gold nuggets and " + ingots + " gold ingots in " + robbed.size() + " records");
                    FakePlayers.remove(player[0]);
                    DragonKit.depart(agent[0], (int) agent[1], 1, 20, c.add(-400, 40, 0));
                })
                .thenWaitUntil(() -> check("the occupant leaves", !about(DragonRecords.DRAGON_DEMATERIALIZED, agent[0]).isEmpty()))
                .thenSucceed();
    }

    /**
     * A puppet dragon is never saved by vanilla's own entity-chunk save, while a pig on the same block is;
     * a copy built from the puppet's NBT (its tag included) is discarded as it joins, under another UUID or
     * an older epoch, so a save and reload never makes a second dragon.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 300, batch = "dragon_save")
    public static void aPuppetDragonIsNeverSavedAndACopyNeverJoins(GameTestHelper helper) {
        String test = "aPuppetDragonIsNeverSavedAndACopyNeverJoins";
        long agent = DragonKit.newAgent();
        Vec3 c = DragonKit.centre(helper);
        DragonKit.materialize(DragonKit.young(agent, helper, c, DragonRecords.Materialize.FLAG_SLEEPING));
        Pig pig = helper.spawn(EntityType.PIG, new Vec3(5.5, 1, 5.5));
        pig.setPersistenceRequired();
        ChunkPos chunk = new ChunkPos(BlockPos.containing(c));
        @SuppressWarnings("unchecked")
        CompletableFuture<List<Entity>>[] read = new CompletableFuture[1];
        UUID[] id = new UUID[1];

        helper.startSequence()
                .thenWaitUntil(() -> body(agent))
                .thenExecute(() -> {
                    DragonBaseEntity d = body(agent);
                    id[0] = d.getUUID();
                    check("shouldBeSaved is false for the puppet", !d.shouldBeSaved());
                    check("Entity.save refuses the puppet", !d.save(new CompoundTag()));
                    check("the pig is saved", pig.shouldBeSaved());
                    read[0] = DragonKit.saveAndReadBack(helper.getLevel(), chunk);
                })
                .thenWaitUntil(() -> check("the entity chunk was read back", read[0] != null && read[0].isDone()))
                .thenExecute(() -> {
                    List<Entity> loaded = read[0].join();
                    check("the pig reached the disk", loaded.stream().anyMatch(e -> e.getUUID().equals(pig.getUUID())));
                    check("the dragon did not", loaded.stream().noneMatch(e -> e.getUUID().equals(id[0])));
                    check("no tagged entity reloads", loaded.stream().noneMatch(RtsimAttachments::isTagged));
                    DragonBaseEntity d = body(agent);
                    // Copies made the way another path would (saveWithoutId keeps the tag: a dimension change
                    // needs it), joining under a new UUID, and as an older epoch.
                    CompoundTag nbt = d.saveWithoutId(new CompoundTag());
                    nbt.putString("id", DragonKit.FIRE);
                    Entity copy = EntityType.loadEntityRecursive(nbt, helper.getLevel(), e -> e);
                    check("a copy was built, tagged", copy != null && RtsimAttachments.isTagged(copy));
                    copy.setUUID(UUID.randomUUID());
                    boolean joined = helper.getLevel().addFreshEntity(copy);
                    Entity older = EntityType.loadEntityRecursive(nbt, helper.getLevel(), e -> e);
                    older.setUUID(UUID.randomUUID());
                    older.setData(RtsimAttachments.AGENT, new AgentTag(agent, 0));
                    boolean olderJoined = helper.getLevel().addFreshEntity(older);
                    check("the copy never joined", !joined && copy.isRemoved());
                    check("the older epoch never joined", !olderJoined && older.isRemoved());
                    check("one body for the agent", bridge().body(agent) == d && !d.isRemoved());
                    record(test, "save", loaded.size() + " entities read back, the dragon not among them; two copies discarded");
                    pig.discard();
                    DragonKit.depart(agent, 1, 1, 20, c.add(-400, 40, 0));
                })
                .thenWaitUntil(() -> check("captured", !about(DragonRecords.DRAGON_DEMATERIALIZED, agent).isEmpty()))
                .thenSucceed();
    }

    /**
     * Two ways a body leaves: a puppet a player tames leaves the sim (A10) as a DRAGON_DIED with no killer
     * (the dragon records have no TAKEN, choice Z7), and stays in the world as the player's, untagged; a
     * puppet carried out of the entity-ticking area is captured (DRAGON_DEMATERIALIZED, not ticking or
     * unloaded) and its body removed.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 300, batch = "dragon_leaves")
    public static void aTamedOrFarAwayPuppetLeavesTheSim(GameTestHelper helper) {
        String test = "aTamedOrFarAwayPuppetLeavesTheSim";
        long tamed = DragonKit.newAgent();
        long far = DragonKit.newAgent();
        Vec3 c = DragonKit.centre(helper);
        DragonKit.materialize(DragonKit.young(tamed, helper, c.add(-2, 0, 0), DragonRecords.Materialize.FLAG_SLEEPING));
        DragonKit.materialize(DragonKit.young(far, helper, c.add(2, 0, 0), DragonRecords.Materialize.FLAG_SLEEPING));
        ServerPlayer[] player = new ServerPlayer[1];
        DragonBaseEntity[] bodies = new DragonBaseEntity[2];

        helper.startSequence()
                .thenWaitUntil(() -> {
                    body(tamed);
                    body(far);
                })
                .thenExecute(() -> {
                    bodies[0] = body(tamed);
                    bodies[1] = body(far);
                    player[0] = FakePlayers.add(helper.getLevel(), c.add(0, 0, 3), "dragon-tamer");
                    bodies[0].tame(player[0]);
                    net.neoforged.neoforge.common.NeoForge.EVENT_BUS.post(new net.neoforged.neoforge.event.entity.living.AnimalTameEvent(bodies[0], player[0]));
                    check("the tamed dragon is no longer a puppet", !RtsimApi.isPuppet(bodies[0]) && bridge().body(tamed) == null);
                    bodies[1].teleportTo(bodies[1].getX() + 20_000, bodies[1].getY(), bodies[1].getZ());
                })
                .thenWaitUntil(() -> check("DRAGON_DIED for the tamed one", !about(DragonRecords.DRAGON_DIED, tamed).isEmpty()))
                .thenWaitUntil(() -> check("the far one was captured", !about(DragonRecords.DRAGON_DEMATERIALIZED, far).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received died = about(DragonRecords.DRAGON_DIED, tamed).get(0);
                    check("no killer", died.aux() == DragonRecords.ATTACKER_NONE && died.view().getLong(DragonRecords.Died.KILLER_MOST) == 0L);
                    check("the tamed body stays, the player's", !bodies[0].isRemoved() && bodies[0].isTame());
                    ScriptedEngine.Received gone = about(DragonRecords.DRAGON_DEMATERIALIZED, far).get(0);
                    check("not ticking or unloaded (" + gone.aux() + ")", gone.aux() == DragonRecords.Dematerialized.NOT_TICKING
                            || gone.aux() == DragonRecords.Dematerialized.UNLOADED);
                    check("the far body is gone", bodies[1].isRemoved() && bridge().body(far) == null);
                    record(test, "left", "tamed: DRAGON_DIED with no killer, body kept; far: DEMATERIALIZED reason " + gone.aux());
                    FakePlayers.remove(player[0]);
                    bodies[0].discard();
                })
                .thenSucceed();
    }

    /**
     * A puppet kills a village's cow: DRAGON_ATE (count 1, minecraft:cow, the village). It picks up a
     * second cow and departs with it: the carried cow goes with it as one unit (C25), reported eaten
     * against the village it was taken from, and removed.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 400, batch = "dragon_ate")
    public static void aDragonsMealInAVillageIsReported(GameTestHelper helper) {
        String test = "aDragonsMealInAVillageIsReported";
        long agent = DragonKit.newAgent();
        Vec3 c = DragonKit.centre(helper);
        Village v = RtsimTests.adoptArena(helper, "human");
        DragonKit.materialize(DragonKit.young(agent, helper, c, DragonRecords.Materialize.FLAG_SLEEPING));
        Cow eaten = helper.spawn(EntityType.COW, new Vec3(2.5, 2, 2.5));
        Cow carried = helper.spawn(EntityType.COW, new Vec3(8.5, 2, 8.5));

        helper.startSequence()
                .thenWaitUntil(() -> body(agent))
                .thenExecute(() -> eaten.hurt(helper.getLevel().damageSources().mobAttack(body(agent)), 1000f))
                .thenWaitUntil(() -> check("DRAGON_ATE reached the engine", !about(DragonRecords.DRAGON_ATE, agent).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received a = about(DragonRecords.DRAGON_ATE, agent).get(0);
                    check("one cow", a.aux() == 1 && "minecraft:cow".equals(engine().javaDictName(a.view().getInt(DragonRecords.Ate.PREY))));
                    check("in the village (" + a.view().getLong(DragonRecords.Ate.VILLAGE) + ")", a.view().getLong(DragonRecords.Ate.VILLAGE) == v.id);
                    check("epoch 1", a.view().getInt(DragonRecords.Ate.EPOCH) == 1);
                    DragonBaseEntity d = body(agent);
                    check("the cow rides the dragon", carried.startRiding(d, true) && carried.getVehicle() == d);
                    DragonKit.depart(agent, 1, 2, 20, c.add(-400, 40, 0));
                })
                .thenWaitUntil(() -> check("captured", !about(DragonRecords.DRAGON_DEMATERIALIZED, agent).isEmpty()))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> meals = about(DragonRecords.DRAGON_ATE, agent);
                    check("the carried cow was eaten too (" + meals.size() + ")", meals.size() == 2);
                    ScriptedEngine.Received second = meals.get(1);
                    check("against the village it was taken from", second.view().getLong(DragonRecords.Ate.VILLAGE) == v.id);
                    check("before the capture", second.obsSeq() < about(DragonRecords.DRAGON_DEMATERIALIZED, agent).get(0).obsSeq());
                    check("and it is gone", carried.isRemoved());
                    record(test, "meals", meals.size() + " cows of village " + v.id);
                    RtsimTests.forget(v);
                })
                .thenSucceed();
    }

    /**
     * Tribute lands in a lair's chest when the lair loads (SINK.md §4.6, docs/boundary.md §6.3). A lair
     * with no chest is reported (LAIR_FOUND) and nothing more; a chest placed in its box is reported once
     * (LAIR_CHEST: the lair, its dimension, the chest's block), and again at the new block when it is
     * broken and another placed. On the native engine a live village with coin in its works purse becomes
     * the lair's tributary (WORKS_GRANT, TRIBUTE_START): the season's tribute waits in the lair while it has
     * no chest, then lands in the chest as emeralds, as the village's books say. On the stub, a scripted
     * TRANSFER_TO_CONTAINER into the reported chest lands.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 2400, batch = "dragon_tribute")
    public static void tributeLandsInALairChestWhenTheLairLoads(GameTestHelper helper) {
        String test = "tributeLandsInALairChestWhenTheLairLoads";
        ServerLevel level = helper.getLevel();
        BoundingBox box = BoundingBox.fromCorners(helper.absolutePos(BlockPos.ZERO), helper.absolutePos(new BlockPos(10, 5, 10)));
        BlockPos home = BlockPos.containing(DragonKit.centre(helper));
        long lairId = 0x0E00_0000_0000_0000L | (ThreadLocalRandom.current().nextLong() & 0xFFFF_FFFF_FFFFL);
        BlockPos chestPos = helper.absolutePos(new BlockPos(8, 2, 8));
        BlockPos movedPos = helper.absolutePos(new BlockPos(2, 2, 2));
        int dim = DragonKit.session().dimId(level.dimension());
        NativeEngine e = engine() instanceof NativeEngine n ? n : null;
        // A live village far from everything (the engine drives it), on the native engine only.
        int ox = -2_000_000 - ThreadLocalRandom.current().nextInt(0, 1_000) * 2_000;
        int oz = 2_000_000 + ThreadLocalRandom.current().nextInt(0, 1_000) * 2_000;
        Village v = e == null ? null : RtsimTests.adoptArenaLive(helper, "human", "minecraft:village_plains", new BlockPos(ox, 64, oz), 20, 90);
        long[] before = e == null ? null : e.dragonsTribute(); // world-wide totals: the GameTest world is reused
        long[] coins = {5}; // the stub's scripted stack
        long op = (1L << 40) | (ThreadLocalRandom.current().nextLong() & 0xFF_FFFF_FFFFL); // an engine op, new each run
        bridge().lairs.declareForTest(level, lairId, "iceandfire:fire_dragon_roost", box, home, level.getGameTime());
        if (e == null)
            record(test, "tribute", "skipped on the stub (it decides nothing): a scripted TRANSFER_TO_CONTAINER instead");

        helper.startSequence()
                .thenWaitUntil(() -> check("LAIR_FOUND reached the engine",
                        !DragonKit.received(DragonRecords.LAIR_FOUND, DragonRecords.LairFound.LAIR, lairId).isEmpty()))
                .thenWaitUntil(() -> check("the engine knows the village and its works",
                        e == null || (e.dragonsVillageRisk(v.id) >= 0 && e.phase1Works(v.id) != null)))
                .thenExecute(() -> {
                    if (e == null)
                        return;
                    e.worksGrant(v.id, 400_000);
                    e.tributeStart(v.id, lairId);
                })
                .thenWaitUntil(() -> {
                    if (e == null)
                        return;
                    long[] t = e.dragonsTribute();
                    check("the village paid the lair a season's tribute (" + Arrays.toString(t) + " from " + Arrays.toString(before) + ")",
                            t[NativeEngine.TRIBUTE_PAID] > before[NativeEngine.TRIBUTE_PAID]
                                    && t[NativeEngine.TRIBUTE_WAITING] > before[NativeEngine.TRIBUTE_WAITING]);
                })
                .thenExecute(() -> {
                    if (e == null)
                        return;
                    long[] t = e.dragonsTribute();
                    coins[0] = t[NativeEngine.TRIBUTE_WAITING] - before[NativeEngine.TRIBUTE_WAITING];
                    long books = e.phase1Works(v.id)[NativeEngine.WORKS_TRIBUTE];
                    check("the village's books: " + books + " thousandths paid for " + coins[0] + " coins", books == coins[0] * 1000);
                    record(test, "paid", coins[0] + " emeralds of village " + v.id + " wait in lair " + Long.toHexString(lairId));
                })
                .thenIdle(Lairs.CHEST_SCAN_TICKS + 20)
                .thenExecute(() -> {
                    check("a lair with no chest sends no LAIR_CHEST",
                            DragonKit.received(DragonRecords.LAIR_CHEST, DragonRecords.LairChest.LAIR, lairId).isEmpty()
                                    && bridge().lairs.chestOf(lairId) == null);
                    if (e != null) {
                        long[] t = e.dragonsTribute();
                        check("and the tribute waits (" + Arrays.toString(t) + ")",
                                t[NativeEngine.TRIBUTE_SENDING] == before[NativeEngine.TRIBUTE_SENDING]
                                        && t[NativeEngine.TRIBUTE_PLACED] == before[NativeEngine.TRIBUTE_PLACED]
                                        && t[NativeEngine.TRIBUTE_WAITING] - before[NativeEngine.TRIBUTE_WAITING] == coins[0]);
                    }
                    level.setBlockAndUpdate(chestPos, Blocks.CHEST.defaultBlockState());
                })
                .thenWaitUntil(() -> check("LAIR_CHEST reached the engine",
                        !DragonKit.received(DragonRecords.LAIR_CHEST, DragonRecords.LairChest.LAIR, lairId).isEmpty()))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> r = DragonKit.received(DragonRecords.LAIR_CHEST, DragonRecords.LairChest.LAIR, lairId);
                    check("once (" + r.size() + ")", r.size() == 1);
                    check("at the chest", at(r.get(0)).equals(chestPos));
                    check("in the lair's dimension", r.get(0).view().getInt(DragonRecords.LairChest.DIM) == dim);
                    check("remembered", chestPos.equals(bridge().lairs.chestOf(lairId)));
                    if (e == null)
                        engine().transferToContainer(op, DragonKit.dimension(helper), chestPos.getX(), chestPos.getY(), chestPos.getZ(),
                                "minecraft:emerald", (int) coins[0]);
                })
                .thenWaitUntil(() -> {
                    int got = emeralds(level, chestPos);
                    if (e == null) {
                        check("the scripted transfer landed (" + got + " emeralds)", got == coins[0]);
                        return;
                    }
                    long[] t = e.dragonsTribute();
                    check("the tribute landed (" + got + " of " + coins[0] + " emeralds, " + Arrays.toString(t) + ")", got == coins[0]
                            && t[NativeEngine.TRIBUTE_PLACED] - before[NativeEngine.TRIBUTE_PLACED] == coins[0]
                            && t[NativeEngine.TRIBUTE_WAITING] == before[NativeEngine.TRIBUTE_WAITING]
                            && t[NativeEngine.TRIBUTE_SENDING] == before[NativeEngine.TRIBUTE_SENDING]);
                })
                .thenExecute(() -> {
                    record(test, "landed", coins[0] + " emeralds in the chest at " + chestPos.toShortString()
                            + (e == null ? " (scripted)" : " (the engine's tribute)"));
                    // The chest is broken and another placed elsewhere in the lair.
                    ((ChestBlockEntity) level.getBlockEntity(chestPos)).clearContent();
                    level.setBlockAndUpdate(chestPos, Blocks.AIR.defaultBlockState());
                    level.setBlockAndUpdate(movedPos, Blocks.CHEST.defaultBlockState());
                })
                .thenWaitUntil(() -> check("LAIR_CHEST again for the new chest",
                        DragonKit.received(DragonRecords.LAIR_CHEST, DragonRecords.LairChest.LAIR, lairId).size() == 2))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> r = DragonKit.received(DragonRecords.LAIR_CHEST, DragonRecords.LairChest.LAIR, lairId);
                    check("at the new chest", at(r.get(1)).equals(movedPos) && movedPos.equals(bridge().lairs.chestOf(lairId)));
                    record(test, "moved", "LAIR_CHEST at " + at(r.get(0)).toShortString() + ", then " + at(r.get(1)).toShortString());
                    if (v != null)
                        RtsimTests.forget(v);
                })
                .thenSucceed();
    }

    /** A LAIR_CHEST record's block. */
    private static BlockPos at(ScriptedEngine.Received r) {
        return new BlockPos(r.view().getInt(DragonRecords.LairChest.X), r.view().getInt(DragonRecords.LairChest.Y),
                r.view().getInt(DragonRecords.LairChest.Z));
    }

    private static int emeralds(ServerLevel level, BlockPos pos) {
        if (!(level.getBlockEntity(pos) instanceof ChestBlockEntity c))
            return -1;
        int n = 0;
        for (int i = 0; i < c.getContainerSize(); i++)
            if (c.getItem(i).is(Items.EMERALD))
                n += c.getItem(i).getCount();
        return n;
    }
}
