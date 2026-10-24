package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.api.TakeReason;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.herd.HerdBridge;
import com.minecraftindustrialcomplex.mic_rtsim.herd.HerdRecords;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.AgentTag;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.util.RandomSource;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.animal.Animal;
import net.minecraft.world.entity.animal.Sheep;
import net.minecraft.world.entity.animal.goat.Goat;
import net.minecraft.world.entity.animal.horse.Horse;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ThreadLocalRandom;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The vanilla herd adapter (docs/boundary.md §8.5) on the server's real session, native engine or stub.
 * The tests script herd commands as the engine's herds would send them ({@link HerdKit}); the last one is
 * the native engine's own herd, founded by the test order HERD_FOUND, seen by a player and remembered
 * across a capture (skipped on the stub, which decides nothing). Each test has its own batch, so groups
 * of two tests never meet. Every test logs what it compared on a {@code [gametest]} line.
 *
 * <p>The scripted tests have sky access: without it the framework roofs the test with barriers
 * ({@code StructureUtils.encaseStructure}), which are motion-blocking, so the surface rule would stand
 * the animals on the roof.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class HerdGameTests {
    private HerdGameTests() {}

    private static HerdBridge bridge() {
        return HerdBridge.INSTANCE;
    }

    /** What vanilla's {@code Horse.randomizeAttributes} draws from a random seeded with {@code seed}: health, speed, jump. */
    private static double[] horseStats(long seed) {
        RandomSource r = RandomSource.create(seed);
        double health = 15.0F + (float) r.nextInt(8) + (float) r.nextInt(9);
        double speed = (0.45F + r.nextDouble() * 0.3 + r.nextDouble() * 0.3 + r.nextDouble() * 0.3) * 0.25;
        double jump = 0.4F + r.nextDouble() * 0.2 + r.nextDouble() * 0.2 + r.nextDouble() * 0.2;
        return new double[]{health, speed, jump};
    }

    private static double[] statsOf(Horse h) {
        return new double[]{h.getAttributeBaseValue(Attributes.MAX_HEALTH), h.getAttributeBaseValue(Attributes.MOVEMENT_SPEED),
                h.getAttributeBaseValue(Attributes.JUMP_STRENGTH)};
    }

    private static boolean same(double[] a, double[] b) {
        for (int i = 0; i < a.length; i++)
            if (Math.abs(a[i] - b[i]) > 1e-6)
                return false;
        return true;
    }

    private static long member(Entity e) {
        return bridge().memberOf(e);
    }

    private static Entity byMember(long agent, long member) {
        for (Entity e : bridge().bodies(agent))
            if (member(e) == member)
                return e;
        return null;
    }

    private static UUID uuidAt(ScriptedEngine.Received r, int offset) {
        return new UUID(r.view().getLong(offset), r.view().getLong(offset + 8));
    }

    /**
     * HERD_MATERIALIZE puts the engine's animals in the world as one group each: sheep with the engine's
     * colours (black, pink, a sheared white one, a brown lamb at vanilla {@code Age} -12,000), horses with
     * the engine's coat and markings and the health, speed and jump vanilla's own roll gives their identity
     * seeds, and a screaming goat; all tagged, persistence-required, never saved by vanilla, standing on
     * the floor. HERD_MATERIALIZED names each group, HERD_GROUP_STATE follows; HERD_DESPAWN captures them.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 600, batch = "herd_materialize", skyAccess = true)
    public static void aHerdGroupMaterializesWithTheEnginesLookAgeAndSeeds(GameTestHelper helper) {
        String test = "aHerdGroupMaterializesWithTheEnginesLookAgeAndSeeds";
        String dim = RtsimTests.dimension(helper);
        long sheep = HerdKit.newAgent(), horses = HerdKit.newAgent(), goats = HerdKit.newAgent();
        int[] c = HerdKit.abs(helper, 5, 5);
        int[] far = c; // speed 0: the groups stay where they are
        int floor = helper.absolutePos(new BlockPos(5, 2, 5)).getY();
        long seedA = 0x5EED_0001L, seedB = 0x5EED_0002L;
        int coatA = 3 | 2 << 8, coatB = 6 | 4 << 8; // chestnut with white field; dark brown with black dots
        HerdKit.materialize(sheep, 1, HerdKit.SHEEP, dim, c[0], c[1], far[0], far[1], 0f, List.of(
                HerdKit.Member.adult(10, 101, 15, c[0] - 2, c[1] - 2),
                HerdKit.Member.adult(11, 102, 6, c[0] + 2, c[1] - 2),
                new HerdKit.Member(12, 103, 0, 0, 1, true, c[0] - 2, c[1] + 2),
                new HerdKit.Member(13, 104, 12, -12_000, 0, false, c[0] + 2, c[1] + 2)));
        HerdKit.materialize(horses, 1, HerdKit.HORSE, dim, c[0], c[1], far[0], far[1], 0f, List.of(
                HerdKit.Member.adult(20, seedA, coatA, c[0] - 3, c[1]),
                HerdKit.Member.adult(21, seedB, coatB, c[0] + 3, c[1])));
        HerdKit.materialize(goats, 1, HerdKit.GOAT, dim, c[0], c[1], far[0], far[1], 0f, List.of(
                HerdKit.Member.adult(30, 301, 1, c[0], c[1] + 3)));

        helper.startSequence()
                .thenWaitUntil(() -> {
                    HerdKit.bodies(sheep, 4);
                    HerdKit.bodies(horses, 2);
                    HerdKit.bodies(goats, 1);
                })
                .thenWaitUntil(() -> check("HERD_MATERIALIZED reached the engine", !HerdKit.about(HerdRecords.HERD_MATERIALIZED, sheep).isEmpty()
                        && !HerdKit.about(HerdRecords.HERD_MATERIALIZED, horses).isEmpty() && !HerdKit.about(HerdRecords.HERD_MATERIALIZED, goats).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received m = HerdKit.about(HerdRecords.HERD_MATERIALIZED, sheep).get(0);
                    check("four sheep spawned (" + m.aux() + ")", m.aux() == 4);
                    check("epoch 1", m.view().getInt(HerdRecords.Materialized.EPOCH) == 1);
                    check("its dimension", dim.equals(RtsimTests.engine().javaDictName(m.view().getInt(HerdRecords.Materialized.DIM))));
                    Map<Long, Integer> colour = Map.of(10L, 15, 11L, 6, 12L, 0, 13L, 12);
                    for (Entity e : HerdKit.bodies(sheep, 4)) {
                        check("a sheep", e instanceof Sheep);
                        Sheep s = (Sheep) e;
                        long k = member(s);
                        check("member " + k + "'s colour is the engine's (" + s.getColor() + ")", s.getColor().getId() == colour.get(k));
                        check("member " + k + " sheared only if the engine said so", s.isSheared() == (k == 12));
                        check("member " + k + "'s age (" + s.getAge() + ")", k == 13 ? s.getAge() <= -11_900 && s.getAge() > -12_100 && s.isBaby() : !s.isBaby());
                        check("member " + k + " stands on the floor (" + s.getY() + ")", Math.abs(s.getY() - floor) < 0.5);
                        check("tagged", RtsimApi.isPuppet(s) && RtsimAttachments.tag(s).agent() == sheep);
                        check("persistence required", s.isPersistenceRequired());
                        check("never saved by vanilla", !s.shouldBeSaved() && !s.save(new CompoundTag()));
                    }
                    for (Entity e : HerdKit.bodies(horses, 2)) {
                        Horse h = (Horse) e;
                        long k = member(h);
                        long seed = k == 20 ? seedA : seedB;
                        int coat = k == 20 ? coatA : coatB;
                        CompoundTag t = new CompoundTag();
                        h.addAdditionalSaveData(t);
                        check("horse " + k + "'s coat and markings are the engine's (" + t.getInt("Variant") + ")", t.getInt("Variant") == coat);
                        double[] want = horseStats(seed), got = statsOf(h);
                        check("horse " + k + "'s health, speed and jump are its seed's (" + java.util.Arrays.toString(got) + " vs "
                                + java.util.Arrays.toString(want) + ")", same(want, got));
                        check("at full health", Math.abs(h.getHealth() - h.getMaxHealth()) < 1e-3);
                        check("not tame", !h.isTamed());
                    }
                    check("two seeds, two horses", !same(statsOf((Horse) HerdKit.bodies(horses, 2).get(0)), statsOf((Horse) HerdKit.bodies(horses, 2).get(1))));
                    Goat g = (Goat) HerdKit.bodies(goats, 1).get(0);
                    check("the goat screams, as the engine said", g.isScreamingGoat());
                    record(test, "materialized", "4 sheep (black, pink, sheared white, brown lamb), 2 horses "
                            + java.util.Arrays.toString(statsOf((Horse) HerdKit.bodies(horses, 2).get(0))) + ", a screaming goat");
                })
                .thenWaitUntil(() -> check("HERD_GROUP_STATE while embodied", !HerdKit.states(sheep).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received s = HerdKit.states(sheep).get(0);
                    check("four present", s.view().getInt(HerdRecords.GroupState.PRESENT) == 4);
                    check("ticking", (s.aux() & HerdRecords.GroupState.FLAG_TICKING) != 0);
                    HerdKit.despawn(sheep, 1, HerdKit.SHEEP);
                    HerdKit.despawn(horses, 1, HerdKit.HORSE);
                    HerdKit.despawn(goats, 1, HerdKit.GOAT);
                })
                .thenWaitUntil(() -> check("all three captured", bridge().group(sheep) == null && bridge().group(horses) == null && bridge().group(goats) == null))
                .thenWaitUntil(() -> check("HERD_DEMATERIALIZED for each", !HerdKit.about(HerdRecords.HERD_DEMATERIALIZED, sheep).isEmpty()
                        && !HerdKit.about(HerdRecords.HERD_DEMATERIALIZED, horses).isEmpty() && !HerdKit.about(HerdRecords.HERD_DEMATERIALIZED, goats).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received d = HerdKit.about(HerdRecords.HERD_DEMATERIALIZED, sheep).get(0);
                    check("captured on command", d.aux() == HerdRecords.Dematerialized.COMMAND);
                    check("four captures", d.view().getInt(HerdRecords.Dematerialized.COUNT) == 4);
                    int sheared = 0;
                    for (int i = 0; i < 4; i++) {
                        int o = HerdRecords.Dematerialized.CAPTURES + i * HerdRecords.Dematerialized.CAPTURE;
                        if ((d.view().getInt(o + HerdRecords.Dematerialized.C_FLAGS) & HerdRecords.MEMBER_SHEARED) != 0) {
                            check("the sheared one is member 12", d.view().getLong(o + HerdRecords.Dematerialized.C_MEMBER) == 12);
                            sheared++;
                        }
                    }
                    check("one sheared capture", sheared == 1);
                    check("no body left", helper.getLevel().getEntities((Entity) null, new AABB(BlockPos.containing(RtsimTests.centre(helper))).inflate(16),
                            e -> RtsimAttachments.isTagged(e) && HerdRecords.isHerdAgent(RtsimAttachments.tag(e).agent())).isEmpty());
                    record(test, "captured", "sheep, horses and goat, reason 1; the sheared sheep captured sheared");
                })
                .thenSucceed();
    }

    /**
     * A player kills a cow of a group: HERD_KILLED at the end of the tick, naming the player as the killer,
     * with a Java op id, and the cow's drops carry it (docs/boundary.md §3.1); a death with no killer is
     * HERD_KILLED by none. The group shrinks and stays one unit: HERD_GROUP_STATE counts one present.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 600, batch = "herd_kill", skyAccess = true)
    public static void killingAHerdAnimalReportsItAndTheGroupShrinks(GameTestHelper helper) {
        String test = "killingAHerdAnimalReportsItAndTheGroupShrinks";
        long agent = HerdKit.newAgent();
        int[] c = HerdKit.abs(helper, 5, 5);
        HerdKit.materialize(agent, 1, HerdKit.COW, RtsimTests.dimension(helper), c[0], c[1], c[0], c[1], 0f, List.of(
                HerdKit.Member.adult(1, 11, 0, c[0] - 2, c[1]),
                HerdKit.Member.adult(2, 12, 0, c[0], c[1]),
                HerdKit.Member.adult(3, 13, 0, c[0] + 2, c[1])));
        ServerPlayer[] player = new ServerPlayer[1];
        long[] statesBefore = new long[1];

        helper.startSequence()
                .thenWaitUntil(() -> HerdKit.bodies(agent, 3))
                .thenExecute(() -> {
                    player[0] = FakePlayers.add(helper.getLevel(), RtsimTests.centre(helper), "herd_hunter");
                    Animal cow = (Animal) byMember(agent, 1);
                    cow.hurt(helper.getLevel().damageSources().playerAttack(player[0]), 1000f);
                    Animal other = (Animal) byMember(agent, 3);
                    other.hurt(helper.getLevel().damageSources().generic(), 1000f);
                })
                .thenWaitUntil(() -> check("two HERD_KILLED", HerdKit.about(HerdRecords.HERD_KILLED, agent).size() == 2))
                .thenExecute(() -> {
                    ScriptedEngine.Received byPlayer = null, byNone = null;
                    for (ScriptedEngine.Received r : HerdKit.about(HerdRecords.HERD_KILLED, agent)) {
                        if (r.view().getLong(HerdRecords.Killed.MEMBER) == 1)
                            byPlayer = r;
                        else if (r.view().getLong(HerdRecords.Killed.MEMBER) == 3)
                            byNone = r;
                    }
                    check("member 1's kill", byPlayer != null);
                    check("by a player", byPlayer.aux() == HerdRecords.BY_PLAYER);
                    check("naming the player", uuidAt(byPlayer, HerdRecords.Killed.KILLER_MOST).equals(player[0].getUUID()));
                    check("with a Java op id", (byPlayer.view().getLong(HerdRecords.Killed.OP_ID) & Obs.JAVA_OP_BIT) != 0);
                    check("member 3 died of nothing that is an entity", byNone != null && byNone.aux() == HerdRecords.BY_NONE
                            && byNone.view().getLong(HerdRecords.Killed.KILLER_MOST) == 0);
                    check("the group goes on with one", HerdKit.bodies(agent, 1).size() == 1 && member(HerdKit.bodies(agent, 1).get(0)) == 2);
                    int carrying = 0;
                    for (ItemEntity drop : helper.getLevel().getEntitiesOfClass(ItemEntity.class, new AABB(BlockPos.containing(RtsimTests.centre(helper))).inflate(12)))
                        if (drop.hasData(RtsimAttachments.PENDING))
                            carrying++;
                    check("the drops carry the kill (" + carrying + ")", carrying >= 2);
                    statesBefore[0] = HerdKit.states(agent).size();
                    record(test, "killed", "member 1 by " + player[0].getUUID() + ", member 3 by none; " + carrying + " drops carry their kill");
                })
                .thenWaitUntil(() -> check("a later HERD_GROUP_STATE", HerdKit.states(agent).size() > statesBefore[0]))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> s = HerdKit.states(agent);
                    check("one present", s.get(s.size() - 1).view().getInt(HerdRecords.GroupState.PRESENT) == 1);
                    FakePlayers.remove(player[0]);
                    HerdKit.despawn(agent, 1, HerdKit.COW);
                })
                .thenWaitUntil(() -> check("captured", bridge().group(agent) == null && !HerdKit.about(HerdRecords.HERD_DEMATERIALIZED, agent).isEmpty()))
                .thenExecute(() -> check("one capture",
                        HerdKit.about(HerdRecords.HERD_DEMATERIALIZED, agent).get(0).view().getInt(HerdRecords.Dematerialized.COUNT) == 1))
                .thenSucceed();
    }

    /**
     * A10: a lead on a pig takes it out of the sim (HERD_TAKEN, lead, the player); so does taming a horse
     * (taming) and a pickup through the core's Carry On hook ({@code RtsimApi.takeFromSim}). A taken animal
     * is ordinary (no tag, saved by vanilla) and carries its HERD_TAKEN; the rest stays one group. Shears
     * mark a sheep (HERD_SHEARED with the player) without taking it.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 600, batch = "herd_take", skyAccess = true)
    public static void aLeadTamingAndAPickupTakeAnimalsOutAndShearsMarkOne(GameTestHelper helper) {
        String test = "aLeadTamingAndAPickupTakeAnimalsOutAndShearsMarkOne";
        String dim = RtsimTests.dimension(helper);
        long pigs = HerdKit.newAgent(), horses = HerdKit.newAgent(), sheep = HerdKit.newAgent();
        int[] c = HerdKit.abs(helper, 5, 5);
        HerdKit.materialize(pigs, 1, HerdKit.PIG, dim, c[0], c[1], c[0], c[1], 0f, List.of(
                HerdKit.Member.adult(1, 1, 0, c[0] - 3, c[1] - 3),
                HerdKit.Member.adult(2, 2, 0, c[0] - 1, c[1] - 3),
                HerdKit.Member.adult(3, 3, 0, c[0] + 1, c[1] - 3)));
        HerdKit.materialize(horses, 1, HerdKit.HORSE, dim, c[0], c[1], c[0], c[1], 0f, List.of(
                HerdKit.Member.adult(1, 4, 0, c[0] - 3, c[1] + 3),
                HerdKit.Member.adult(2, 5, 1, c[0] + 3, c[1] + 3)));
        HerdKit.materialize(sheep, 1, HerdKit.SHEEP, dim, c[0], c[1], c[0], c[1], 0f, List.of(
                HerdKit.Member.adult(1, 6, 0, c[0], c[1]),
                HerdKit.Member.adult(2, 7, 0, c[0] + 1, c[1])));
        ServerPlayer[] player = new ServerPlayer[1];
        Entity[] led = new Entity[1], tamed = new Entity[1], picked = new Entity[1], shorn = new Entity[1];

        helper.startSequence()
                .thenWaitUntil(() -> {
                    HerdKit.bodies(pigs, 3);
                    HerdKit.bodies(horses, 2);
                    HerdKit.bodies(sheep, 2);
                })
                .thenExecute(() -> {
                    player[0] = FakePlayers.add(helper.getLevel(), RtsimTests.centre(helper), "herd_taker");
                    led[0] = byMember(pigs, 1);
                    ((net.minecraft.world.entity.Leashable) led[0]).setLeashedTo(player[0], true);
                    tamed[0] = byMember(horses, 2);
                    ((Horse) tamed[0]).tameWithName(player[0]);
                    picked[0] = byMember(pigs, 3);
                    check("the core's pickup hook takes a herd animal", RtsimApi.takeFromSim(picked[0], TakeReason.PICKUP, player[0]));
                    shorn[0] = byMember(sheep, 2);
                    player[0].setItemInHand(InteractionHand.MAIN_HAND, new ItemStack(Items.SHEARS));
                    player[0].interactOn(shorn[0], InteractionHand.MAIN_HAND);
                    check("shorn", ((Sheep) shorn[0]).isSheared());
                })
                .thenWaitUntil(() -> check("two pigs taken and a horse", HerdKit.about(HerdRecords.HERD_TAKEN, pigs).size() == 2
                        && HerdKit.about(HerdRecords.HERD_TAKEN, horses).size() == 1))
                .thenWaitUntil(() -> check("the shearing reported", HerdKit.about(HerdRecords.HERD_SHEARED, sheep).size() == 1))
                .thenExecute(() -> {
                    Map<Long, ScriptedEngine.Received> takings = new HashMap<>();
                    for (ScriptedEngine.Received r : HerdKit.about(HerdRecords.HERD_TAKEN, pigs))
                        takings.put(r.view().getLong(HerdRecords.Taken.MEMBER), r);
                    ScriptedEngine.Received lead = takings.get(1L), pickup = takings.get(3L);
                    ScriptedEngine.Received tame = HerdKit.about(HerdRecords.HERD_TAKEN, horses).get(0);
                    check("the lead: TAKEN(lead)", lead != null && lead.aux() == Obs.Taken.LEAD);
                    check("the pickup: TAKEN(pickup)", pickup != null && pickup.aux() == Obs.Taken.PICKUP);
                    check("taming: TAKEN(taming) of horse 2", tame.aux() == Obs.Taken.TAME && tame.view().getLong(HerdRecords.Taken.MEMBER) == 2);
                    for (ScriptedEngine.Received r : List.of(lead, pickup, tame)) {
                        check("by the player", uuidAt(r, HerdRecords.Taken.PLAYER_MOST).equals(player[0].getUUID()));
                        check("with a Java op id", (r.view().getLong(HerdRecords.Taken.OP_ID) & Obs.JAVA_OP_BIT) != 0);
                    }
                    for (Entity e : List.of(led[0], tamed[0], picked[0])) {
                        check("ordinary now: no tag", !RtsimApi.isPuppet(e));
                        check("saved by vanilla now", e.shouldBeSaved());
                        check("and carries its TAKEN until the engine has it", e.hasData(RtsimAttachments.PENDING));
                    }
                    check("the rest of the pigs stays one group", HerdKit.bodies(pigs, 1).size() == 1 && member(HerdKit.bodies(pigs, 1).get(0)) == 2);
                    check("and the other horse", HerdKit.bodies(horses, 1).size() == 1);
                    ScriptedEngine.Received s = HerdKit.about(HerdRecords.HERD_SHEARED, sheep).get(0);
                    check("sheep 2 sheared", s.view().getLong(HerdRecords.Sheared.MEMBER) == 2);
                    check("by the player", uuidAt(s, HerdRecords.Sheared.PLAYER_MOST).equals(player[0].getUUID()));
                    check("still a member", HerdKit.bodies(sheep, 2).size() == 2 && RtsimApi.isPuppet(shorn[0]));
                    record(test, "taken", "a pig on a lead, a pig picked up, a horse tamed; a sheep shorn, still in its group");
                    FakePlayers.remove(player[0]);
                    for (Entity e : List.of(led[0], tamed[0], picked[0]))
                        e.discard();
                    HerdKit.despawn(pigs, 1, HerdKit.PIG);
                    HerdKit.despawn(horses, 1, HerdKit.HORSE);
                    HerdKit.despawn(sheep, 1, HerdKit.SHEEP);
                })
                .thenWaitUntil(() -> check("the rest captured", bridge().group(pigs) == null && bridge().group(horses) == null && bridge().group(sheep) == null))
                .thenSucceed();
    }

    /**
     * A capture keeps each member's state (HERD_DEMATERIALIZED: index, health, position, sheared), and a
     * member materialized again from its identity seed is the same animal: a wounded horse captured with
     * its wound comes back with the same max health, speed and jump.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 600, batch = "herd_capture", skyAccess = true)
    public static void aCaptureKeepsEachMembersStateAndTheSeedBringsBackTheSameHorse(GameTestHelper helper) {
        String test = "aCaptureKeepsEachMembersStateAndTheSeedBringsBackTheSameHorse";
        String dim = RtsimTests.dimension(helper);
        long agent = HerdKit.newAgent();
        int[] c = HerdKit.abs(helper, 5, 5);
        List<HerdKit.Member> members = List.of(
                HerdKit.Member.adult(7, 0x0A11_CE01L, 2, c[0] - 2, c[1]),
                HerdKit.Member.adult(9, 0x0A11_CE02L, 4 | 1 << 8, c[0] + 2, c[1]));
        HerdKit.materialize(agent, 1, HerdKit.HORSE, dim, c[0], c[1], c[0], c[1], 0f, members);
        Map<Long, double[]> first = new HashMap<>();
        Vec3[] at = new Vec3[1];

        helper.startSequence()
                .thenWaitUntil(() -> HerdKit.bodies(agent, 2))
                .thenExecute(() -> {
                    for (Entity e : HerdKit.bodies(agent, 2))
                        first.put(member(e), statsOf((Horse) e));
                    Horse h = (Horse) byMember(agent, 7);
                    h.setHealth(5f);
                    at[0] = h.position();
                    HerdKit.despawn(agent, 1, HerdKit.HORSE);
                })
                .thenWaitUntil(() -> check("captured", bridge().group(agent) == null && !HerdKit.about(HerdRecords.HERD_DEMATERIALIZED, agent).isEmpty()))
                .thenExecute(() -> {
                    ByteBuffer d = HerdKit.about(HerdRecords.HERD_DEMATERIALIZED, agent).get(0).view();
                    check("two captures", d.getInt(HerdRecords.Dematerialized.COUNT) == 2);
                    boolean seen = false;
                    for (int i = 0; i < 2; i++) {
                        int o = HerdRecords.Dematerialized.CAPTURES + i * HerdRecords.Dematerialized.CAPTURE;
                        if (d.getLong(o + HerdRecords.Dematerialized.C_MEMBER) != 7)
                            continue;
                        seen = true;
                        float health = d.getFloat(o + HerdRecords.Dematerialized.C_HEALTH);
                        // A horse heals a point now and then (AbstractHorse.aiStep, one tick in 900).
                        check("its wound (" + health + ")", health >= 5f && health <= 6f);
                        check("where it stood", Math.abs(d.getInt(o + HerdRecords.Dematerialized.C_X) - Math.floor(at[0].x)) <= 2
                                && Math.abs(d.getInt(o + HerdRecords.Dematerialized.C_Z) - Math.floor(at[0].z)) <= 2);
                    }
                    check("member 7 captured", seen);
                    HerdKit.materialize(agent, 2, HerdKit.HORSE, dim, c[0], c[1], c[0], c[1], 0f, members);
                })
                .thenWaitUntil(() -> {
                    HerdKit.bodies(agent, 2);
                    check("epoch 2", bridge().group(agent).epoch == 2);
                })
                .thenExecute(() -> {
                    for (Entity e : HerdKit.bodies(agent, 2)) {
                        double[] again = statsOf((Horse) e);
                        check("horse " + member(e) + " comes back the same (" + java.util.Arrays.toString(again) + ")", same(first.get(member(e)), again));
                    }
                    record(test, "identity", "both horses: the same max health, speed and jump in epochs 1 and 2; the wound captured");
                    HerdKit.despawn(agent, 2, HerdKit.HORSE);
                })
                .thenWaitUntil(() -> check("captured again", bridge().group(agent) == null))
                .thenSucceed();
    }

    /**
     * A save and reload duplicates nothing: vanilla's own entity-chunk save leaves the herd's puppets out
     * and keeps a taken (now ordinary) animal; copies of a puppet built from its NBT, under a new UUID or
     * an older epoch, never join; the group still has its one body per member.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "herd_save", skyAccess = true)
    public static void aSaveAndReloadDuplicatesNothing(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 600);
        String test = "aSaveAndReloadDuplicatesNothing";
        long agent = HerdKit.newAgent();
        int[] c = HerdKit.abs(helper, 5, 5);
        HerdKit.materialize(agent, 2, HerdKit.SHEEP, RtsimTests.dimension(helper), c[0], c[1], c[0], c[1], 0f, List.of(
                HerdKit.Member.adult(1, 1, 7, c[0] - 1, c[1]),
                HerdKit.Member.adult(2, 2, 8, c[0] + 1, c[1]),
                HerdKit.Member.adult(3, 3, 0, c[0], c[1] + 1)));
        ServerPlayer[] player = new ServerPlayer[1];
        Entity[] taken = new Entity[1];
        Set<UUID> puppets = new HashSet<>();
        @SuppressWarnings("unchecked")
        CompletableFuture<List<Entity>>[] read = new CompletableFuture[1];
        long[] staleBefore = new long[1];

        helper.startSequence()
                .thenWaitUntil(waits.logic(() -> HerdKit.bodies(agent, 3)))
                .thenExecute(() -> {
                    player[0] = FakePlayers.add(helper.getLevel(), RtsimTests.centre(helper), "herd_saver");
                    taken[0] = byMember(agent, 3);
                    ((net.minecraft.world.entity.Leashable) taken[0]).setLeashedTo(player[0], true);
                })
                .thenWaitUntil(waits.logic(() -> check("the led sheep is taken", !HerdKit.about(HerdRecords.HERD_TAKEN, agent).isEmpty())))
                .thenExecute(() -> {
                    for (Entity e : HerdKit.bodies(agent, 2))
                        puppets.add(e.getUUID());
                    ((net.minecraft.world.entity.Leashable) taken[0]).dropLeash(true, false);
                    BlockPos p = BlockPos.containing(RtsimTests.centre(helper));
                    List<ChunkPos> chunks = new ArrayList<>();
                    for (int dx = -1; dx <= 1; dx++)
                        for (int dz = -1; dz <= 1; dz++)
                            chunks.add(new ChunkPos(p.offset(dx * 8, 0, dz * 8)));
                    read[0] = PuppetSaveGameTests.saveAndReadBack(helper.getLevel(), chunks.toArray(new ChunkPos[0]));
                })
                .thenWaitUntil(waits.chunks(() -> check("the entity chunks were read back", read[0] != null && read[0].isDone())))
                .thenExecute(() -> {
                    List<Entity> loaded = read[0].join();
                    int puppetsSaved = 0;
                    boolean takenSaved = false;
                    for (Entity e : loaded) {
                        if (puppets.contains(e.getUUID()))
                            puppetsSaved++;
                        if (e.getUUID().equals(taken[0].getUUID()))
                            takenSaved = true;
                        check("no tagged entity reloads (" + e + ")", !RtsimAttachments.isTagged(e));
                    }
                    check("no herd puppet reached the disk (" + puppetsSaved + ")", puppetsSaved == 0);
                    check("the taken sheep did, as an ordinary sheep", takenSaved);
                    // Copies of a puppet from its own NBT: a template copy under a new UUID, an older epoch.
                    ServerLevel level = helper.getLevel();
                    staleBefore[0] = RtsimTests.session().staleDiscarded();
                    Entity puppet = HerdKit.bodies(agent, 2).get(0);
                    for (AgentTag tag : List.of(new AgentTag(agent, 2), new AgentTag(agent, 1))) {
                        CompoundTag nbt = new CompoundTag();
                        puppet.saveWithoutId(nbt);
                        nbt.putString("id", EntityType.getKey(puppet.getType()).toString());
                        Entity copy = EntityType.loadEntityRecursive(nbt, level, e -> e);
                        check("a copy rebuilt from the puppet's NBT", copy != null && RtsimAttachments.isTagged(copy));
                        copy.setData(RtsimAttachments.AGENT, tag);
                        copy.setUUID(UUID.randomUUID());
                        check("a copy of epoch " + tag.epoch() + " never joins", !level.addFreshEntity(copy));
                    }
                    long discarded = RtsimTests.session().staleDiscarded() - staleBefore[0];
                    check("two stale copies discarded (" + discarded + ")", discarded == 2);
                    check("the group still has one body per member", HerdKit.bodies(agent, 2).size() == 2);
                    int ours = 0, takenCopies = 0;
                    for (Sheep x : level.getEntitiesOfClass(Sheep.class, new AABB(BlockPos.containing(RtsimTests.centre(helper))).inflate(32))) {
                        AgentTag t = RtsimAttachments.tag(x);
                        if (t != null && t.agent() == agent)
                            ours++;
                        if (x.getUUID().equals(taken[0].getUUID()))
                            takenCopies++;
                    }
                    check("two puppets of the herd in the world, not more (" + ours + ")", ours == 2);
                    check("and the taken sheep once (" + takenCopies + ")", takenCopies == 1);
                    record(test, "saved", "0 puppets on disk, the taken sheep saved; 2 stale copies discarded; the 2 puppets and the taken sheep once each");
                    FakePlayers.remove(player[0]);
                    taken[0].discard();
                    HerdKit.despawn(agent, 2, HerdKit.SHEEP);
                })
                .thenWaitUntil(waits.logic(() -> check("captured", bridge().group(agent) == null)))
                .thenSucceed();
    }

    private static double meanX(List<Entity> bodies) {
        double x = 0;
        for (Entity e : bodies)
            x += e.getX();
        return x / Math.max(1, bodies.size());
    }

    /**
     * Steering (Y14): a group walks with its herd toward the waypoint, sheep by their goal and goats by
     * their brain's walk target, as far as the arena lets them; HERD_STEER turns them, and they walk back
     * the other way. The herd's place moves 0.2 blocks a tick, so the members fall behind it and follow.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 1200, batch = "herd_steer", skyAccess = true)
    public static void aHerdWalksTowardItsWaypointAndTurnsOnSteer(GameTestHelper helper) {
        String test = "aHerdWalksTowardItsWaypointAndTurnsOnSteer";
        String dim = RtsimTests.dimension(helper);
        long sheep = HerdKit.newAgent(), goats = HerdKit.newAgent();
        int[] c = HerdKit.abs(helper, 2, 5);
        int[] east = {c[0] + 40, c[1]}, west = {c[0] - 40, c[1]};
        HerdKit.materialize(sheep, 1, HerdKit.SHEEP, dim, c[0], c[1], east[0], east[1], 0.2f, List.of(
                HerdKit.Member.adult(1, 1, 0, c[0], c[1] - 1),
                HerdKit.Member.adult(2, 2, 0, c[0], c[1]),
                HerdKit.Member.adult(3, 3, 0, c[0], c[1] + 1)));
        HerdKit.materialize(goats, 1, HerdKit.GOAT, dim, c[0], c[1], east[0], east[1], 0.2f, List.of(
                HerdKit.Member.adult(1, 4, 0, c[0], c[1] - 3),
                HerdKit.Member.adult(2, 5, 0, c[0], c[1] + 3)));
        double[] start = new double[2], turned = new double[2];

        helper.startSequence()
                .thenWaitUntil(() -> {
                    HerdKit.bodies(sheep, 3);
                    HerdKit.bodies(goats, 2);
                })
                .thenExecute(() -> {
                    start[0] = meanX(HerdKit.bodies(sheep, 3));
                    start[1] = meanX(HerdKit.bodies(goats, 2));
                })
                .thenWaitUntil(() -> {
                    check("the sheep walk east (" + meanX(HerdKit.bodies(sheep, 3)) + " from " + start[0] + ")",
                            meanX(HerdKit.bodies(sheep, 3)) > start[0] + 4);
                    check("the goats too (" + meanX(HerdKit.bodies(goats, 2)) + " from " + start[1] + ")",
                            meanX(HerdKit.bodies(goats, 2)) > start[1] + 4);
                })
                .thenExecute(() -> {
                    turned[0] = meanX(HerdKit.bodies(sheep, 3));
                    turned[1] = meanX(HerdKit.bodies(goats, 2));
                    long now = RtsimTests.session().gameTime();
                    HerdKit.steer(sheep, 1, HerdKit.SHEEP, 0.2f, west[0], west[1], now + 300);
                    HerdKit.steer(goats, 1, HerdKit.GOAT, 0.2f, west[0], west[1], now + 300);
                    record(test, "east", "sheep " + start[0] + " to " + turned[0] + ", goats " + start[1] + " to " + turned[1]);
                })
                .thenWaitUntil(() -> {
                    check("HERD_STEER turns the sheep west (" + meanX(HerdKit.bodies(sheep, 3)) + " from " + turned[0] + ")",
                            meanX(HerdKit.bodies(sheep, 3)) < turned[0] - 4);
                    check("and the goats (" + meanX(HerdKit.bodies(goats, 2)) + " from " + turned[1] + ")",
                            meanX(HerdKit.bodies(goats, 2)) < turned[1] - 4);
                })
                .thenExecute(() -> {
                    check("the waypoint is the STEER's", bridge().group(sheep).targetX() == west[0] && bridge().group(goats).targetX() == west[0]);
                    record(test, "west", "sheep " + turned[0] + " to " + meanX(HerdKit.bodies(sheep, 3)) + ", goats " + turned[1] + " to "
                            + meanX(HerdKit.bodies(goats, 2)));
                    HerdKit.despawn(sheep, 1, HerdKit.SHEEP);
                    HerdKit.despawn(goats, 1, HerdKit.GOAT);
                })
                .thenWaitUntil(() -> check("captured", bridge().group(sheep) == null && bridge().group(goats) == null))
                .thenSucceed();
    }

    /**
     * A fence across the group's way: its members walk to it and can go no farther while their herd's
     * place moves on toward the waypoint; once most of them have stalled for the stall time, the adapter
     * reports HERD_OBSTACLE once, with the members that failed, where they are stuck and the waypoint.
     * (The stall time is cut to 60 ticks for the test; the rule is otherwise the adapter's.)
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 1200, batch = "herd_fence", skyAccess = true)
    public static void aFenceProducesAnObstacleReport(GameTestHelper helper) {
        String test = "aFenceProducesAnObstacleReport";
        long agent = HerdKit.newAgent();
        // A fence across the arena at relative x 6 (z 1 to 9): the glass ring closes the rest.
        for (int z = 1; z <= 9; z++)
            helper.setBlock(new BlockPos(6, 2, z), Blocks.OAK_FENCE.defaultBlockState());
        int[] c = HerdKit.abs(helper, 3, 5);
        int[] fence = HerdKit.abs(helper, 6, 5);
        int[] target = {c[0] + 60, c[1]};
        int stall = HerdBridge.stallTicks;
        HerdBridge.stallTicks = 60;
        HerdKit.materialize(agent, 1, HerdKit.SHEEP, RtsimTests.dimension(helper), c[0], c[1], target[0], target[1], 0.25f, List.of(
                HerdKit.Member.adult(1, 1, 0, c[0], c[1] - 2),
                HerdKit.Member.adult(2, 2, 0, c[0], c[1]),
                HerdKit.Member.adult(3, 3, 0, c[0] - 1, c[1] + 2),
                HerdKit.Member.adult(4, 4, 0, c[0] - 1, c[1] - 1)));

        helper.startSequence()
                .thenWaitUntil(() -> HerdKit.bodies(agent, 4))
                .thenWaitUntil(() -> check("HERD_OBSTACLE", !HerdKit.about(HerdRecords.HERD_OBSTACLE, agent).isEmpty()))
                .thenExecute(() -> {
                    HerdBridge.stallTicks = stall;
                    ScriptedEngine.Received o = HerdKit.about(HerdRecords.HERD_OBSTACLE, agent).get(0);
                    ByteBuffer b = o.view();
                    check("most of the group failed (" + o.aux() + " of " + b.getInt(HerdRecords.Obstacle.PRESENT) + ")",
                            o.aux() * 2 >= 4 && b.getInt(HerdRecords.Obstacle.PRESENT) == 4);
                    check("stuck before the fence (" + b.getInt(HerdRecords.Obstacle.STUCK_X) + " < " + fence[0] + ")",
                            b.getInt(HerdRecords.Obstacle.STUCK_X) < fence[0]);
                    check("toward the waypoint", b.getInt(HerdRecords.Obstacle.TARGET_X) == target[0] && b.getInt(HerdRecords.Obstacle.TARGET_Z) == target[1]);
                    for (Entity e : HerdKit.bodies(agent, 4))
                        check("no sheep passed the fence (" + e.getX() + ")", e.getX() < fence[0]);
                    record(test, "obstacle", o.aux() + " of 4 failed, stuck at (" + b.getInt(HerdRecords.Obstacle.STUCK_X) + ", "
                            + b.getInt(HerdRecords.Obstacle.STUCK_Z) + "), the fence at x " + fence[0]);
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    check("reported once per waypoint", HerdKit.about(HerdRecords.HERD_OBSTACLE, agent).size() == 1);
                    HerdKit.despawn(agent, 1, HerdKit.SHEEP);
                })
                .thenWaitUntil(() -> check("captured", bridge().group(agent) == null))
                .thenSucceed();
    }

    // ---- the native engine's own herd -----------------------------------------------------------

    /** Writes a PLAYER record for a GameTest player (the server's player list does not hold them). */
    private static void reportPlayer(ServerPlayer p) {
        var s = RtsimTests.session();
        int dim = s.dimId(p.level().dimension()); // before direct(): an id is defined before its first use
        int at = s.boundary().direct(Obs.PLAYER, Obs.Player.SIZE, dim, s.gameTime());
        if (at < 0)
            return;
        ByteBuffer b = s.boundary().openBuffer();
        b.putLong(at + Obs.Player.UUID_MOST, p.getUUID().getMostSignificantBits());
        b.putLong(at + Obs.Player.UUID_LEAST, p.getUUID().getLeastSignificantBits());
        b.putDouble(at + Obs.Player.X, p.getX());
        b.putDouble(at + Obs.Player.Y, p.getY());
        b.putDouble(at + Obs.Player.Z, p.getZ());
    }

    /** The embodied herd group whose bodies stand within 48 blocks of (x, z), or null. */
    private static HerdBridge.Group groupNear(double x, double z) {
        for (HerdBridge.Group g : bridge().groups())
            for (Entity e : bridge().bodies(g.agent))
                if (Math.hypot(e.getX() - x, e.getZ() - z) < 48)
                    return g;
        return null;
    }

    /**
     * Docs/boundary.md §8 end to end on the native engine (skipped on the stub): a herd of 12 sheep founded
     * (HERD_FOUND) at a place far from every earlier run's, a player beside it reported every 20 ticks.
     * The engine materializes the group near the player: every sheep shows the colour the engine gave it.
     * The player kills one: HERD_KILLED, and the engine's count of the herd drops by one. The player
     * leaves (no PLAYER for over 200 ticks): the engine despawns the group and it is captured. The player
     * comes back: the same sheep, with the same colours, and never the dead one.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "herd_native")
    public static void theEnginesOwnHerdMaterializesNearAPlayerAndRemembersItsSheep(GameTestHelper helper) {
        String test = "theEnginesOwnHerdMaterializesNearAPlayerAndRemembersItsSheep";
        if (!(RtsimTests.engine() instanceof NativeEngine e)) {
            record(test, "skipped", "the stub engine decides nothing; this test needs the native engine's herds");
            helper.succeed();
            return;
        }
        ServerLevel level = helper.getLevel();
        ChunkWait waits = ChunkWait.of(helper, 3000);
        int bx = 3_000_000 + ThreadLocalRandom.current().nextInt(0, 2_000) * 1_024 + 8;
        int bz = -3_000_000 - ThreadLocalRandom.current().nextInt(0, 2_000) * 1_024 + 8;
        List<ChunkPos> forced = new ArrayList<>();
        for (int dx = -3; dx <= 3; dx++)
            for (int dz = -3; dz <= 3; dz++) {
                ChunkPos p = new ChunkPos((bx >> 4) + dx, (bz >> 4) + dz);
                if (level.setChunkForced(p.x, p.z, true))
                    forced.add(p);
            }
        Runnable unforce = () -> {
            forced.forEach(p -> level.setChunkForced(p.x, p.z, false));
            forced.clear();
        };
        waits.onEnd(unforce);
        boolean[] reporting = {false};
        ServerPlayer[] player = new ServerPlayer[1];
        long[] agent = {0};
        long[] alive = {0};
        Map<Long, Integer> firstColours = new HashMap<>();
        long[] dead = {-1};
        // The agent id is the engine's own, and an engine of another life (a reset earlier in the run) numbered
        // its herds from the same first id: its records may still be in the received log, replayed from the
        // journal into it. This herd's are those journaled after its founding.
        long[] since = {0};
        java.util.function.Supplier<List<ScriptedEngine.Received>> taken = () -> HerdKit.about(HerdRecords.HERD_DEMATERIALIZED, agent[0])
                .stream().filter(r -> r.obsSeq() >= since[0]).toList();
        waits.eachTick(() -> {
            if (reporting[0] && player[0] != null && RtsimTests.session().gameTime() % 20 == 0)
                reportPlayer(player[0]);
        });

        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> check("the far chunks tick entities",
                        com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession.entitiesTickAt(level, new BlockPos(bx, 64, bz)))))
                .thenExecute(() -> {
                    int y = level.getHeight(net.minecraft.world.level.levelgen.Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, bx, bz);
                    player[0] = FakePlayers.add(level, new Vec3(bx + 0.5, y, bz + 0.5), "herd_watcher");
                    since[0] = RtsimTests.session().boundary().journal().nextObsSeq();
                    e.herdFound("GameTest flock", HerdKit.SHEEP, RtsimTests.session().dimId(level.dimension()), bx, bz, 12,
                            ThreadLocalRandom.current().nextLong(1, Long.MAX_VALUE));
                    reporting[0] = true;
                })
                .thenWaitUntil(waits.engine(() -> check("the engine's group near the player", groupNear(bx, bz) != null)))
                .thenWaitUntil(waits.engine(() -> check("all of it", groupNear(bx, bz) != null && bridge().bodies(groupNear(bx, bz).agent).size() == 12)))
                .thenExecute(() -> {
                    HerdBridge.Group g = groupNear(bx, bz);
                    agent[0] = g.agent;
                    long[] info = e.herdInfo(g.agent);
                    check("the engine knows the herd", info != null && info[NativeEngine.HERD_EMBODIED_EPOCH] == g.epoch);
                    alive[0] = info[NativeEngine.HERD_ALIVE];
                    for (Entity b : bridge().bodies(g.agent)) {
                        Sheep s = (Sheep) b;
                        int want = g.variantOf(member(s));
                        check("sheep " + member(s) + " shows the engine's colour (" + s.getColor().getId() + " vs " + want + ")",
                                s.getColor().getId() == want);
                        check("a natural sheep colour", Set.of(0, 6, 7, 8, 12, 15).contains(want));
                        firstColours.put(member(s), want);
                    }
                    Sheep victim = (Sheep) bridge().bodies(g.agent).get(0);
                    dead[0] = member(victim);
                    victim.hurt(level.damageSources().playerAttack(player[0]), 1000f);
                    record(test, "materialized", "12 sheep of herd " + Long.toHexString(g.agent) + " (epoch " + g.epoch + "), colours " + firstColours.values());
                })
                .thenWaitUntil(waits.engine(() -> {
                    long[] info = e.herdInfo(agent[0]);
                    check("the engine booked the kill", info != null && info[NativeEngine.HERD_KILLED] == 1);
                }))
                .thenExecute(() -> {
                    long[] info = e.herdInfo(agent[0]);
                    check("the herd's count dropped by one (" + info[NativeEngine.HERD_ALIVE] + " of " + alive[0] + ")",
                            info[NativeEngine.HERD_ALIVE] == alive[0] - 1);
                    check("the dead one is in the dead set", info[NativeEngine.HERD_DEAD] == 1);
                    reporting[0] = false; // the player leaves
                })
                .thenWaitUntil(waits.engine(() -> check("the engine took the group back", bridge().group(agent[0]) == null
                        && !taken.get().isEmpty())))
                .thenExecute(() -> {
                    ScriptedEngine.Received d = taken.get().get(0);
                    check("11 sheep captured with their states: " + d.view().getInt(HerdRecords.Dematerialized.COUNT) + " in epoch "
                                    + d.view().getInt(HerdRecords.Dematerialized.EPOCH) + ", reason " + d.aux() + ", observation " + d.obsSeq()
                                    + " (the founding after " + since[0] + ")",
                            d.view().getInt(HerdRecords.Dematerialized.COUNT) == 11);
                    reporting[0] = true; // the player is back
                })
                .thenWaitUntil(waits.engine(() -> check("the group again", groupNear(bx, bz) != null && groupNear(bx, bz).agent == agent[0]
                        && bridge().bodies(agent[0]).size() == 11)))
                .thenExecute(() -> {
                    HerdBridge.Group g = bridge().group(agent[0]);
                    check("a new epoch", g.epoch > 1);
                    for (Entity b : bridge().bodies(agent[0])) {
                        long k = member(b);
                        check("never the dead sheep", k != dead[0]);
                        check("sheep " + k + " is one of those seen", firstColours.containsKey(k));
                        check("with the same colour", ((Sheep) b).getColor().getId() == firstColours.get(k));
                    }
                    record(test, "remembered", "the same 11 sheep in epoch " + g.epoch + ", the dead one " + dead[0] + " never again");
                    reporting[0] = false;
                    FakePlayers.remove(player[0]);
                })
                .thenWaitUntil(waits.engine(() -> check("taken back once the player is gone", bridge().group(agent[0]) == null)))
                .thenExecute(unforce)
                .thenSucceed();
    }
}
