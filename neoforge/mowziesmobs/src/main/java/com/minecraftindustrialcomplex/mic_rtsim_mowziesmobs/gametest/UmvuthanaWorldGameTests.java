package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest;

import com.bobmowzie.mowziesmobs.server.entity.EntityHandler;
import com.bobmowzie.mowziesmobs.server.entity.frostmaw.EntityFrostmaw;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaFollowerToRaptor;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaMinion;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaRaptor;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthi;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.MaskType;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.trade.Trade;
import com.bobmowzie.mowziesmobs.server.inventory.ContainerUmvuthanaTrade;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.OwedHunts;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.TestMachines;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.ThreatConfig;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.GroveChests;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.GroveKind;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.GroveTrade;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.MicRtsimMowziesMobs;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Far;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Grove;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestSequence;
import net.minecraft.network.chat.contents.TranslatableContents;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.Container;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.ExperienceOrb;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.animal.Animal;
import net.minecraft.world.entity.animal.Cow;
import net.minecraft.world.entity.animal.IronGolem;
import net.minecraft.world.entity.animal.Sheep;
import net.minecraft.world.entity.animal.horse.AbstractHorse;
import net.minecraft.world.entity.animal.horse.Horse;
import net.minecraft.world.entity.decoration.LeashFenceKnotEntity;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.monster.Skeleton;
import net.minecraft.world.entity.projectile.Arrow;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.entity.ChestBlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;
import java.util.UUID;
import java.util.function.Supplier;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The Umvuthana's bands and world on the jar's own bodies (MOWZIE.md §7 tests 12, 13, 14, 16, 17 and 20;
 * docs/choices-mowzie-build.md, M9c): a band that leaves a masked player alone and answers a blow, the Umvuthi's trade
 * and a minion's trade reported at the camp, an envoy party of the jar's bodies, tribute that lands in a grove's chest
 * when the grove loads, raptors that hunt the base's animals and an owed hunt that kills the nearest eligible ones
 * when its district loads, and each body's share of a fight. Every test works in a far place of its own (the world
 * is reused, B37) and has a batch of its own, since the bridge's counters are global (a test reads them as deltas).
 * The players are server players that are never ticked (a FakePlayers one), so a test that lets bodies strike one
 * raises its health every tick.
 */
@GameTestHolder(MicRtsimMowziesMobs.MODID)
@PrefixGameTestTemplate(false)
public final class UmvuthanaWorldGameTests {
    private UmvuthanaWorldGameTests() {}

    private static final String ARENA = RtsimTests.TEMPLATE;
    private static final String YARD = "industry_yard";
    private static final String WARRIOR = MicRtsimMowziesMobs.WARRIOR, RAPTOR = MicRtsimMowziesMobs.RAPTOR,
            ENVOY = MicRtsimMowziesMobs.ENVOY;
    private static final int CHIEF_SLOT = ConflictRecords.CampReconcile.CHIEF_SLOT;

    // ---- helpers ----------------------------------------------------------------------------------

    /**
     * A place of a test's own: the chunk {@code (k + 1, z0 + 1)} of its far base, a 3 x 3 or a 5 x 5 of chunks round it
     * forced, and a stone floor over the 3 x 3. Bodies stand at {@link #top} + 1.
     */
    private static final class Stage {
        final Far far;
        final ServerLevel level;
        final int cx, cz;
        int top;
        /** The middle of the middle chunk, one block over the floor (where an owed hunt's circle is centred). */
        Vec3 centre;

        Stage(GameTestHelper helper, String test) {
            far = new Far(helper, test);
            level = far.level;
            cx = far.k + 1;
            cz = far.z0 + 1;
        }

        /** Forces the square of chunks {@code r} each way from the middle one. */
        void force(int r) {
            far.forceRect(cx - r, cz - r, cx + r, cz + r);
        }

        void lay() {
            top = MowzieKit.platform(level, (cx - 1) * 16, (cx + 1) * 16 + 15, (cz - 1) * 16, (cz + 1) * 16 + 15);
            centre = new Vec3(cx * 16 + 8.5, top + 1, cz * 16 + 8.5);
        }

        Vec3 at(double dx, double dz) {
            return new Vec3(centre.x + dx, top + 1, centre.z + dz);
        }
    }

    /** A hostile band of the stage's faction standing still at its middle: one stack for each spec. */
    private static long band(GameTestHelper helper, Stage st, ConflictKit.StackSpec... stacks) {
        long agent = ConflictKit.newAgent();
        ConflictKit.Band b = ConflictKit.band(agent, ConflictKit.newFaction(), RtsimTests.dimension(helper));
        b.arcX = st.centre.x;
        b.arcZ = st.centre.z;
        b.targetX = st.centre.x + 1000;
        b.targetY = st.centre.y;
        b.targetZ = st.centre.z;
        b.speed = 0f;
        ConflictKit.materialize(b, List.of(stacks));
        return agent;
    }

    /** Every body of a party that stands: each slot's own and the extras that came with it (a raptor's pack). */
    private static List<Mob> everyone(long agent) {
        List<Mob> out = new ArrayList<>();
        ConflictBridge.Party p = ConflictKit.party(agent);
        if (p == null)
            return out;
        for (ConflictBridge.Stack st : p.stacks())
            for (ConflictBridge.Slot s : st.slots()) {
                if (s.entity() != null && !s.entity().isRemoved())
                    out.add(s.entity());
                for (Mob x : s.extras())
                    if (!x.isRemoved())
                        out.add(x);
            }
        return out;
    }

    /** A player that bodies may strike all they like: a great health, raised again at the end of every tick. */
    private static ServerPlayer tough(GameTestHelper helper, Vec3 at, String name) {
        ServerPlayer p = MowzieKit.player(helper, at, name);
        p.getAttribute(Attributes.MAX_HEALTH).setBaseValue(4000);
        p.setHealth(4000f);
        MowzieKit.everyTick(helper, () -> p.setHealth(p.getMaxHealth()));
        return p;
    }

    private static <T extends Animal> T animal(ServerLevel level, EntityType<T> type, Vec3 at) {
        T a = type.create(level);
        check(type + " is made", a != null);
        a.moveTo(at.x, at.y, at.z, level.random.nextFloat() * 360f - 180f, 0f);
        a.setNoAi(true);
        a.setPersistenceRequired();
        check(type + " is added", level.addFreshEntity(a));
        return a;
    }

    /** A fence at {@code fenceAt} with the mob leashed to its knot. */
    private static void leash(ServerLevel level, Mob mob, Vec3 fenceAt) {
        BlockPos fence = BlockPos.containing(fenceAt);
        level.setBlock(fence, Blocks.OAK_FENCE.defaultBlockState(), 3);
        mob.setLeashedTo(LeashFenceKnotEntity.getOrCreateKnot(level, fence), true);
        check("the " + mob.getType() + " is leashed", mob.isLeashed());
    }

    /** A 9 x 9 stone floor at the stage's floor height with three blocks of air over it, around {@code at}: a floor outside the stage's 3 x 3. */
    private static void pad(ServerLevel level, int top, Vec3 at) {
        BlockPos c = BlockPos.containing(at);
        for (int dx = -4; dx <= 4; dx++)
            for (int dz = -4; dz <= 4; dz++) {
                level.setBlock(new BlockPos(c.getX() + dx, top, c.getZ() + dz), Blocks.STONE.defaultBlockState(), 2);
                for (int y = 1; y <= 3; y++)
                    level.setBlock(new BlockPos(c.getX() + dx, top + y, c.getZ() + dz), Blocks.AIR.defaultBlockState(), 2);
            }
    }

    /**
     * Inside {@code thenWaitUntil}: each entity object is saved and unloaded with its chunk, read at each poll
     * ({@link MowzieKit#removed}: under C2ME a null {@code getEntity} is not a save, MZB-662).
     */
    @SafeVarargs
    private static Runnable gone(ServerLevel level, Supplier<? extends Entity>... objects) {
        return MowzieKit.removed(level, objects);
    }

    /** The same for every object of the arrays, read at each poll: they are filled after the sequence is built. */
    @SuppressWarnings("unchecked")
    private static Runnable gone(ServerLevel level, Entity[]... arrays) {
        List<Supplier<Entity>> objects = new ArrayList<>();
        for (Entity[] a : arrays)
            for (int i = 0; i < a.length; i++) {
                int k = i;
                objects.add(() -> a[k]);
            }
        return MowzieKit.removed(level, objects.toArray(new Supplier[0]));
    }

    private static boolean alive(ServerLevel level, UUID id) {
        Entity e = level.getEntity(id);
        return e != null && e.isAlive();
    }

    private static void click(ServerPlayer player, Entity e) {
        player.interactOn(e, InteractionHand.MAIN_HAND);
        player.interactOn(e, InteractionHand.OFF_HAND);
    }

    /** Named "<Culture> Envoy" (the speaker's key), not its demand. */
    private static boolean envoyNamed(Mob m) {
        return m.getCustomName() != null && m.getCustomName().getContents() instanceof TranslatableContents t
                && t.getKey().equals("mic_rtsim.talk.speaker.envoy");
    }

    private static List<ScriptedEngine.Received> paidRecords(Grove g) {
        return RtsimTests.recordsWith(RtsimTests.engine(), ConflictRecords.CHIEF_PAID, ConflictRecords.ChiefPaid.FACTION, g.faction);
    }

    private static List<ScriptedEngine.Received> havens(Grove g) {
        return RtsimTests.recordsWith(RtsimTests.engine(), ConflictRecords.HAVEN_CHEST, ConflictRecords.HavenChest.FACTION, g.faction);
    }

    /** Inside {@code thenWaitUntil}: the Umvuthi carries the chief's tag (the native engine adopted it). */
    private static Runnable adopted(EntityUmvuthi[] u) {
        return () -> {
            var tag = MowzieKit.tag(u[0]);
            check("the engine adopted the Umvuthi into the chief's slot", tag != null && tag.slot() == CHIEF_SLOT);
        };
    }

    private static int nuggetsIn(Container c) {
        int n = 0;
        for (int i = 0; i < c.getContainerSize(); i++)
            if (c.getItem(i).is(Items.GOLD_NUGGET))
                n += c.getItem(i).getCount();
        return n;
    }

    private static int emptySlots(Container c) {
        int n = 0;
        for (int i = 0; i < c.getContainerSize(); i++)
            if (c.getItem(i).isEmpty())
                n++;
        return n;
    }

    private static Container chestAt(ServerLevel level, BlockPos pos) {
        BlockEntity be = level.getBlockEntity(pos);
        check("a chest stands at " + pos.toShortString(), be instanceof ChestBlockEntity);
        return (ChestBlockEntity) be;
    }

    // ---- 12: a masked player -------------------------------------------------------------------

    /**
     * A hostile Umvuthana band leaves a masked player alone and goes for a bare one (§7 test 12; the jar's player-
     * targeting goals, which the bridge leaves on a hostile band's bodies). A band of three grove warriors and a
     * raptor with its four followers stands still at the middle of a floor; a masked player (the jar's fear mask)
     * stands 12 blocks to the east and a bare one 12 blocks to the west, both with health the test raises every
     * tick. Some body is within 14 blocks of the masked player (the check is not vacuous), no body or extra ever
     * targets the masked player while the band is unprovoked, and one targets the bare player. The masked player then
     * strikes a warrior (its mask is the fear one, which does not block): it, and so the band, answers within 20
     * ticks. A recall takes the band away.
     */
    @GameTest(template = ARENA, timeoutTicks = 2400, batch = "umvuthana_mask", skyAccess = true)
    public static void aMaskedPlayerIsNotTargetedByTheBand(GameTestHelper helper) {
        String test = "aMaskedPlayerIsNotTargetedByTheBand";
        Stage st = new Stage(helper, test);
        ServerLevel level = st.level;
        long[] agent = new long[1];
        ServerPlayer[] masked = new ServerPlayer[1], bare = new ServerPlayer[1];
        EntityUmvuthanaMinion[] struck = new EntityUmvuthanaMinion[1];
        List<String> violations = new ArrayList<>();
        int[] watching = {0}; // 1: unprovoked, 2: the masked player has struck
        int[] ticks = {0}, bareFirst = {0}, items0 = {0}, orbs0 = {0};

        helper.startSequence()
                .thenExecute(() -> st.force(1))
                .thenWaitUntil(st.far.ready())
                .thenExecute(() -> {
                    st.lay();
                    agent[0] = band(helper, st, new ConflictKit.StackSpec(WARRIOR, 3, false), new ConflictKit.StackSpec(RAPTOR, 1, false));
                })
                .thenWaitUntil(() -> {
                    ConflictBridge.Party p = ConflictKit.party(agent[0]);
                    check("the party stands", p != null && p.stacks().size() == 2);
                    check("the warriors are placed", p.stacks().get(0).state() == ConflictBridge.StackState.PLACED && p.stacks().get(0).bodies().size() == 3);
                    check("the raptor is placed", p.stacks().get(1).state() == ConflictBridge.StackState.PLACED && p.stacks().get(1).bodies().size() == 1);
                })
                .thenIdle(10)
                .thenExecute(() -> {
                    List<Mob> all = everyone(agent[0]);
                    check("three warriors, a raptor and its four followers stand (" + all.size() + ")", all.size() == 8);
                    double x = 0, z = 0;
                    for (Mob m : all) {
                        x += m.getX();
                        z += m.getZ();
                        if (m instanceof EntityUmvuthanaMinion w)
                            w.setMask(MaskType.FEAR);
                    }
                    x /= all.size();
                    z /= all.size();
                    masked[0] = tough(helper, new Vec3(x + 12, st.top + 1, z), "mowzie-masked");
                    bare[0] = tough(helper, new Vec3(x - 12, st.top + 1, z), "mowzie-bare");
                    MowzieKit.mask(masked[0]);
                    double nearest = Double.MAX_VALUE;
                    for (Mob m : all)
                        nearest = Math.min(nearest, m.distanceTo(masked[0]));
                    check("a body is within 14 blocks of the masked player (" + nearest + ")", nearest <= 14);
                    for (ConflictBridge.Slot s : ConflictKit.party(agent[0]).stacks().get(0).slots())
                        if (struck[0] == null && s.entity() instanceof EntityUmvuthanaMinion w)
                            struck[0] = w;
                    check("a warrior to strike", struck[0] != null);
                    MowzieKit.everyTick(helper, () -> {
                        if (watching[0] == 0)
                            return;
                        ticks[0]++;
                        for (Mob m : everyone(agent[0])) {
                            LivingEntity t = m.getTarget();
                            if (t == masked[0] && watching[0] == 1)
                                violations.add(m.getType() + " targeted the masked player at tick " + ticks[0]);
                            if (t == bare[0] && bareFirst[0] == 0)
                                bareFirst[0] = ticks[0];
                        }
                    });
                    watching[0] = 1;
                })
                .thenWaitUntil(() -> check("a body went for the bare player", bareFirst[0] > 0))
                .thenIdle(100)
                .thenExecute(() -> {
                    record(test, "unprovoked", "the bare player was first targeted at tick " + bareFirst[0] + " of the watch");
                    check("no body targeted the masked player (" + violations + ")", violations.isEmpty());
                    watching[0] = 2;
                    boolean hit = struck[0].hurt(level.damageSources().playerAttack(masked[0]), 1f);
                    check("the masked player's blow lands", hit);
                })
                .thenIdle(20)
                .thenExecute(() -> {
                    check("the struck warrior turns on the masked player (" + struck[0].getTarget() + ")", struck[0].getTarget() == masked[0]);
                    // The raptor's followers are extras, no members: a player's kill of each drops no loot and no experience
                    // (MZB-643; the stack is made again with new ones, so each would be a mask and experience to farm).
                    AABB around = new AABB(st.centre, st.centre).inflate(40);
                    items0[0] = level.getEntitiesOfClass(ItemEntity.class, around).size();
                    orbs0[0] = level.getEntitiesOfClass(ExperienceOrb.class, around).size();
                    int killed = 0;
                    for (Mob m : everyone(agent[0]))
                        if (m instanceof EntityUmvuthanaFollowerToRaptor f) {
                            f.setMask(MaskType.FEAR);
                            f.hurt(level.damageSources().playerAttack(masked[0]), 1000f);
                            check("the follower died to the player", f.isDeadOrDying());
                            killed++;
                        }
                    check("the raptor's four followers were killed (" + killed + ")", killed == 4);
                })
                .thenIdle(10)
                .thenExecute(() -> {
                    AABB around = new AABB(st.centre, st.centre).inflate(40);
                    int items = level.getEntitiesOfClass(ItemEntity.class, around).size();
                    int orbs = level.getEntitiesOfClass(ExperienceOrb.class, around).size();
                    check("four followers dropped no item (" + items0[0] + " before, " + items + " after)", items == items0[0]);
                    check("and no experience (" + orbs0[0] + " orbs before, " + orbs + " after)", orbs == orbs0[0]);
                    ConflictKit.recall(agent[0], 1, ConflictRecords.RECALL_DONE, 1, st.centre.x - 200, st.centre.z);
                })
                .thenWaitUntil(() -> check("BAND_DEMATERIALIZED", !ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, agent[0]).isEmpty()))
                .thenSucceed();
    }

    // ---- 13: tribute and trade at the camp ----------------------------------------------------------

    /**
     * The Umvuthi's trade and a minion's trade reach the engine as the camp's (§7 test 13; MZB-417, MZB-418, MZB-600).
     * Native engine only (the grove is the engine's): a grove with its Umvuthi adopted and a masked player who pays the
     * Umvuthi's own desires (7 gold blocks) through the jar's trade handler: exactly one CHIEF_PAID with the camp's
     * faction, the chief's slot and generation, flags 0, the payer, the desires and a Java op id. What the engine
     * gives back for it, the 567 nuggets of gift, is the engine's and shows in test 16. The same player then takes the
     * result of a trade of 4 gold nuggets with a warrior standing in the grove: exactly one TRADED, with 4 nuggets and
     * the payer. A second Umvuthi standing outside every grove, paid the same way, writes nothing.
     */
    @GameTest(template = ARENA, timeoutTicks = 2400, batch = "umvuthana_trade", skyAccess = true)
    public static void theUmvuthisTradeIsReportedAsTributeAtTheCamp(GameTestHelper helper) {
        String test = "theUmvuthisTradeIsReportedAsTributeAtTheCamp";
        if (!(RtsimTests.engine() instanceof NativeEngine)) {
            record(test, "skipped", "the grove is the engine's only on the native engine");
            helper.succeed();
            return;
        }
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove g = new Grove(far, far.k, far.z0);
        EntityUmvuthi[] u = new EntityUmvuthi[1], outside = new EntityUmvuthi[1];
        EntityUmvuthanaMinion[] minion = new EntityUmvuthanaMinion[1];
        ServerPlayer[] payer = new ServerPlayer[1];
        int[] generation = new int[1], paidEver = new int[1];
        long[] paid0 = new long[1], traded0 = new long[1];
        ItemStack[] desires = new ItemStack[1];
        Trade trade = new Trade(new ItemStack(Items.GOLD_NUGGET, 4), new ItemStack(Items.STICK, 12), 1);

        helper.startSequence()
                .thenExecute(g::force)
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    g.lay().declare(MicRtsimMowziesMobs.GROVE_STRUCTURE, GroveKind.INSTANCE);
                    u[0] = MowzieKit.umvuthi(level, g.at(-10, 0));
                    outside[0] = MowzieKit.umvuthi(level, g.at(36, 0));
                })
                .thenWaitUntil(adopted(u))
                .thenExecute(() -> {
                    generation[0] = MowzieKit.tag(u[0]).generation();
                    check("the Umvuthi outside is no camp's", MowzieKit.tag(outside[0]) == null);
                    desires[0] = u[0].getDesires().copy();
                    payer[0] = MowzieKit.player(helper, g.at(0, 8).add(0, 1, 0), "mowzie-trader");
                    MowzieKit.mask(payer[0]);
                    paid0[0] = MowzieKit.camps().chiefPaid;
                    check("no CHIEF_PAID yet", paidRecords(g).isEmpty());
                    check("the jar remembers the payer after the trade", MowzieKit.payUmvuthi(payer[0], u[0]));
                })
                .thenWaitUntil(() -> check("one CHIEF_PAID reached the engine (" + paidRecords(g).size() + ")", paidRecords(g).size() == 1))
                .thenExecute(() -> {
                    ScriptedEngine.Received r = paidRecords(g).get(0);
                    check("one item entry (aux " + r.aux() + ")", r.aux() == 1);
                    check("the chief's slot", r.view().getInt(ConflictRecords.ChiefPaid.SLOT) == CHIEF_SLOT);
                    check("and generation " + generation[0], r.view().getInt(ConflictRecords.ChiefPaid.GENERATION) == generation[0]);
                    check("flags 0 (a tagged chief)", r.view().getInt(ConflictRecords.ChiefPaid.FLAGS) == 0);
                    check("the payer", payer[0].getGameProfile().getId().equals(ConflictKit.uuidAt(r, ConflictRecords.ChiefPaid.PLAYER_MOST)));
                    check("the desires' item", r.view().getInt(ConflictRecords.ChiefPaid.ITEMS + ConflictRecords.I_ITEM)
                            == RtsimTests.session().itemId(desires[0].getItem()));
                    check("and count " + desires[0].getCount(), r.view().getInt(ConflictRecords.ChiefPaid.ITEMS + ConflictRecords.I_COUNT) == desires[0].getCount());
                    check("with a Java op id", (r.view().getLong(ConflictRecords.ChiefPaid.OP_ID) & Obs.JAVA_OP_BIT) != 0);
                    check("the reconciler counted one payment", MowzieKit.camps().chiefPaid == paid0[0] + 1);
                    // A warrior in the grove with a trade of gold nuggets (not finalized, so its own trade store is empty
                    // and nothing re-rolls the trade).
                    minion[0] = MowzieKit.stray(level, EntityHandler.UMVUTHANA_MINION.get(), g.at(8, 8).x, g.at(8, 8).y, g.at(8, 8).z, true, false);
                    minion[0].setOfferingTrade(trade);
                    check("the warrior offers the trade", minion[0].isOfferingTrade());
                    traded0[0] = MowzieKit.camps().traded;
                    check("no TRADED yet", RtsimTests.recordsWith(RtsimTests.engine(), ConflictRecords.TRADED, ConflictRecords.Traded.FACTION, g.faction).isEmpty());
                    minion[0].setCustomer(payer[0]);
                    ContainerUmvuthanaTrade menu = new ContainerUmvuthanaTrade(98, minion[0], payer[0].getInventory());
                    payer[0].containerMenu = menu;
                    menu.getSlot(0).set(trade.getInput());
                    net.minecraft.world.inventory.Slot result = menu.getSlot(1);
                    check("the result is the trade's output (" + result.getItem() + ")", result.getItem().is(Items.STICK) && result.getItem().getCount() == 12);
                    ItemStack taken = result.remove(result.getItem().getCount());
                    result.onTake(payer[0], taken);
                    menu.removed(payer[0]);
                    payer[0].containerMenu = payer[0].inventoryMenu;
                    // The Umvuthi outside every grove, paid the same way: nothing is written.
                    paidEver[0] = RtsimTests.engine().received(ConflictRecords.CHIEF_PAID).size();
                    // The payer stands beside it, so that a payment is made (and the camp's refusal is what writes nothing).
                    payer[0].setPos(outside[0].getX() + 1.5, outside[0].getY(), outside[0].getZ());
                    check("the jar remembers the payer at the outside Umvuthi too", MowzieKit.payUmvuthi(payer[0], outside[0]));
                })
                .thenWaitUntil(() -> check("one TRADED reached the engine", RtsimTests.recordsWith(RtsimTests.engine(), ConflictRecords.TRADED,
                        ConflictRecords.Traded.FACTION, g.faction).size() == 1))
                .thenIdle(20)
                .thenExecute(() -> {
                    ScriptedEngine.Received t = RtsimTests.recordsWith(RtsimTests.engine(), ConflictRecords.TRADED, ConflictRecords.Traded.FACTION, g.faction).get(0);
                    check("4 nuggets (" + t.view().getInt(ConflictRecords.Traded.NUGGETS) + ")", t.view().getInt(ConflictRecords.Traded.NUGGETS) == GroveTrade.nuggets(trade)
                            && GroveTrade.nuggets(trade) == 4);
                    check("by the payer", payer[0].getGameProfile().getId().equals(ConflictKit.uuidAt(t, ConflictRecords.Traded.PLAYER_MOST)));
                    check("with a Java op id", (t.view().getLong(ConflictRecords.Traded.OP_ID) & Obs.JAVA_OP_BIT) != 0);
                    check("the reconciler counted one trade", MowzieKit.camps().traded == traded0[0] + 1);
                    check("the Umvuthi outside wrote no CHIEF_PAID (" + RtsimTests.engine().received(ConflictRecords.CHIEF_PAID).size() + " of " + paidEver[0] + ")",
                            RtsimTests.engine().received(ConflictRecords.CHIEF_PAID).size() == paidEver[0] && paidRecords(g).size() == 1);
                    check("nor did the reconciler count one", MowzieKit.camps().chiefPaid == paid0[0] + 1);
                    record(test, "reported", "one CHIEF_PAID of " + desires[0] + ", one TRADED of 4 nuggets, none for the Umvuthi outside");
                })
                .thenSucceed();
    }

    // ---- 14: the envoy of the jar's bodies ---------------------------------------------------------

    /**
     * The core's envoy test (MOWZIE.md §8, §4.5) on the Umvuthana's bodies: an envoy party of grove warriors is one
     * group, its envoy named, and while its demand is open nothing targets the party and it targets nothing (no
     * target goals on any body: an unmasked bystander three blocks off is not targeted, and a golem's target, a guard's,
     * a golem's blow, an arrow with no owner, a skeleton's arrow and a golem's blow on the envoy are all refused).
     * The player it came to is masked, and the envoy offers a trade, so a click the bridge did not take would open the
     * jar's trade window: paying with the good in the main hand, then in the off hand, and a plain click with empty
     * hands (the demand shown) open nothing and leave the player's menu the inventory's. The payments are two
     * TRIBUTE_PAID with Java op ids (3 and 1 emeralds), the second leaving 2 owed. The player's arrow lifts the guard
     * and is one ENVOY_ANSWER ANSWER_ATTACKED. A settlers' party's guard killed by /kill is a casualty with no killer and
     * no answer, and the engine's withdrawal takes the demand away. No talk line is asserted (the core's test
     * does that on its own bodies). A recall takes the bodies away.
     */
    @GameTest(template = ARENA, timeoutTicks = 2400, batch = "umvuthana_envoy", skyAccess = true)
    public static void anUmvuthanaEnvoyPartyIgnoresGolemsAndPlayersUntilStruckAndTakesPayment(GameTestHelper helper) {
        String test = "anUmvuthanaEnvoyPartyIgnoresGolemsAndPlayersUntilStruckAndTakesPayment";
        Far far = new Far(helper, test); // a hostile body is removed on Peaceful
        ServerLevel level = helper.getLevel();
        String dim = RtsimTests.dimension(helper);
        Vec3 c = RtsimTests.centre(helper);
        ConflictBridge bridge = ConflictKit.bridge();
        long a = ConflictKit.newAgent(), b = ConflictKit.newAgent();
        long faction = ConflictKit.newFaction();
        ServerPlayer payer = tough(helper, c.add(3, 0, 0), "mowzie-envoy-payer");
        ServerPlayer bystander = tough(helper, c.add(3, 0, 3), "mowzie-envoy-bystander");
        MowzieKit.mask(payer);
        int demand = ConflictRecords.Envoy.DEMAND_TRIBUTE | ConflictRecords.Envoy.DEMAND_STOP;
        long deadline = level.getGameTime() + 24_000;
        ConflictKit.envoy(a, 1, faction, dim, c.x - 1.5, c.y, c.z, payer.getUUID(), demand, ConflictRecords.SIGNAL_MINING,
                "minecraft:emerald", 6, deadline, List.of(new ConflictKit.MemberSpec(ENVOY, 1), new ConflictKit.MemberSpec(WARRIOR, 2)));
        ConflictKit.envoy(b, 1, faction, dim, c.x + 1.0, c.y, c.z - 3.5, null, ConflictRecords.Envoy.DEMAND_TRIBUTE, 0,
                "minecraft:emerald", 1, deadline, List.of(new ConflictKit.MemberSpec(ENVOY, 1), new ConflictKit.MemberSpec(WARRIOR, 1)));
        Mob[] envoy = new Mob[1], guard = new Mob[1], envoyB = new Mob[1], guardB = new Mob[1];
        IronGolem[] golem = new IronGolem[1];
        long[] blockedHits = {0}, blockedTargets = {0};
        float[] health = {0};

        helper.startSequence()
                .thenWaitUntil(() -> {
                    check("party A placed", ConflictKit.party(a) != null && ConflictKit.party(a).stacks().size() == 1
                            && ConflictKit.party(a).stacks().get(0).bodies().size() == 3);
                    check("party B placed", ConflictKit.party(b) != null && ConflictKit.party(b).stacks().size() == 1
                            && ConflictKit.party(b).stacks().get(0).bodies().size() == 2);
                    check("BAND_MATERIALIZED with all three", !ConflictKit.about(ConflictRecords.BAND_MATERIALIZED, a).isEmpty());
                })
                .thenExecute(() -> {
                    for (ConflictBridge.Slot sl : ConflictKit.party(a).stacks().get(0).slots()) {
                        if (sl.envoy)
                            envoy[0] = sl.entity();
                        else if (guard[0] == null)
                            guard[0] = sl.entity();
                    }
                    for (ConflictBridge.Slot sl : ConflictKit.party(b).stacks().get(0).slots()) {
                        if (sl.envoy)
                            envoyB[0] = sl.entity();
                        else
                            guardB[0] = sl.entity();
                    }
                    for (Mob m : bridge.bodies(a)) {
                        check("every body of the party is the jar's warrior (" + m.getType() + ")", m instanceof EntityUmvuthanaMinion);
                        ((EntityUmvuthanaMinion) m).setMask(MaskType.FEAR); // the fear mask does not block a blow
                    }
                    for (Mob m : bridge.bodies(b))
                        ((EntityUmvuthanaMinion) m).setMask(MaskType.FEAR);
                    check("the envoy is named by its culture: " + envoy[0].getCustomName(), envoyNamed(envoy[0]) && envoy[0].isCustomNameVisible());
                    check("party B's too", envoyNamed(envoyB[0]));
                    check("only the envoy is named", guard[0].getCustomName() == null);
                    check("both guarded", ConflictKit.party(a).guarded() && ConflictKit.party(b).guarded());
                    for (Mob m : bridge.bodies(a))
                        check("no target goals while guarded", m.targetSelector.getAvailableGoals().isEmpty());
                    EntityUmvuthanaMinion e = (EntityUmvuthanaMinion) envoy[0];
                    if (!e.isOfferingTrade())
                        e.setOfferingTrade(new Trade(new ItemStack(Items.GOLD_NUGGET, 4), new ItemStack(Items.STICK, 12), 1));
                    check("the envoy offers a trade and the payer is masked, so an untaken click would open the window",
                            e.isOfferingTrade() && e.getTarget() == null);
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    for (Mob m : bridge.bodies(a))
                        check("no body targets anyone (" + m.getTarget() + ")", m.getTarget() == null);
                    blockedTargets[0] = bridge.blockedTargets;
                    blockedHits[0] = bridge.blockedHits;
                    golem[0] = EntityType.IRON_GOLEM.create(level);
                    golem[0].moveTo(c.x, c.y, c.z + 3.0);
                    level.addFreshEntity(golem[0]);
                    golem[0].setTarget(guard[0]);
                    check("the golem may not target a guard", golem[0].getTarget() == null);
                    guard[0].setTarget(payer);
                    check("a guard may not target the player", guard[0].getTarget() == null);
                    check("both refusals counted (" + (bridge.blockedTargets - blockedTargets[0]) + ")", bridge.blockedTargets >= blockedTargets[0] + 2);
                    health[0] = guard[0].getHealth();
                    golem[0].doHurtTarget(guard[0]);
                    Arrow arrow = EntityType.ARROW.create(level);
                    guard[0].hurt(level.damageSources().arrow(arrow, null), 4f);
                    Skeleton skeleton = EntityType.SKELETON.create(level);
                    guard[0].hurt(level.damageSources().arrow(arrow, skeleton), 4f);
                    envoy[0].hurt(level.damageSources().mobAttack(golem[0]), 4f);
                    arrow.discard();
                    skeleton.discard();
                    check("no damage (" + guard[0].getHealth() + " of " + health[0] + ")", guard[0].getHealth() == health[0]
                            && envoy[0].getHealth() == envoy[0].getMaxHealth());
                    check("four blocked hits (" + (bridge.blockedHits - blockedHits[0]) + ")", bridge.blockedHits >= blockedHits[0] + 4);
                    check("no answer for any of it", ConflictKit.about(ConflictRecords.ENVOY_ANSWER, a).isEmpty());
                })
                .thenIdle(20)
                .thenExecute(() -> {
                    check("the golem never got a guarded target", golem[0].getTarget() == null);
                    record(test, "guard", (bridge.blockedHits - blockedHits[0]) + " hits and " + (bridge.blockedTargets - blockedTargets[0]) + " targets refused");
                    // 3 of 6 emeralds from the main hand.
                    payer.setItemInHand(InteractionHand.MAIN_HAND, new ItemStack(Items.EMERALD, 3));
                    click(payer, envoy[0]);
                    check("3 paid, 3 owed", ConflictKit.party(a).demand().remaining() == 3 && payer.getMainHandItem().isEmpty());
                    check("no trade window opened", payer.containerMenu == payer.inventoryMenu && ((EntityUmvuthanaMinion) envoy[0]).getCustomer() == null);
                    // 1 more from the off hand, the main hand empty.
                    payer.setItemInHand(InteractionHand.OFF_HAND, new ItemStack(Items.EMERALD, 1));
                    click(payer, envoy[0]);
                    check("1 paid from the off hand, 2 owed", ConflictKit.party(a).demand().remaining() == 2 && payer.getOffhandItem().isEmpty());
                    check("no trade window opened", payer.containerMenu == payer.inventoryMenu && ((EntityUmvuthanaMinion) envoy[0]).getCustomer() == null);
                    // A plain click with empty hands shows the demand; it too opens no window.
                    click(payer, envoy[0]);
                    check("an empty-hand click opened no trade window (" + payer.containerMenu + ")",
                            payer.containerMenu == payer.inventoryMenu && ((EntityUmvuthanaMinion) envoy[0]).getCustomer() == null
                                    && !((EntityUmvuthanaMinion) envoy[0]).isTrading());
                    check("and took nothing", ConflictKit.party(a).demand().remaining() == 2);
                    // The settlers' envoy only shows its demand: it takes nothing from a player.
                    payer.setItemInHand(InteractionHand.MAIN_HAND, new ItemStack(Items.EMERALD, 2));
                    click(payer, envoyB[0]);
                    check("B took nothing", ConflictKit.party(b).demand().remaining() == 1 && payer.getMainHandItem().getCount() == 2);
                    payer.setItemInHand(InteractionHand.MAIN_HAND, ItemStack.EMPTY);
                    // The player's arrow hurts a guard and is an insult.
                    golem[0].discard();
                    payer.setGameMode(GameType.CREATIVE);
                    health[0] = guard[0].getHealth();
                    Arrow arrow = EntityType.ARROW.create(level);
                    guard[0].hurt(level.damageSources().arrow(arrow, payer), 2f);
                    arrow.discard();
                    check("the player's arrow hurt (" + guard[0].getHealth() + " of " + health[0] + ")", guard[0].getHealth() < health[0]);
                    check("the guard is lifted", !ConflictKit.party(a).guarded());
                    for (Mob m : bridge.bodies(a))
                        check("their target goals are back", !m.targetSelector.getAvailableGoals().isEmpty());
                    check("party B still guarded", ConflictKit.party(b).guarded());
                    guardB[0].kill();
                })
                .thenWaitUntil(() -> check("B's casualty", ConflictKit.about(ConflictRecords.BAND_CASUALTIES, b).size() == 1))
                .thenWaitUntil(() -> check("TRIBUTE_PAID twice", ConflictKit.tributes(a).size() == 2))
                .thenExecute(() -> {
                    check("one insult (" + ConflictKit.answers(a, ConflictRecords.ANSWER_ATTACKED) + ")", ConflictKit.answers(a, ConflictRecords.ANSWER_ATTACKED) == 1);
                    ScriptedEngine.Received insult = null;
                    for (ScriptedEngine.Received r : ConflictKit.about(ConflictRecords.ENVOY_ANSWER, a))
                        if (r.aux() == ConflictRecords.ANSWER_ATTACKED)
                            insult = r;
                    check("by the player", insult != null && payer.getUUID().equals(ConflictKit.uuidAt(insult, ConflictRecords.EnvoyAnswer.PLAYER_MOST)));
                    for (long x : new long[]{a, b})
                        check("ENVOY_ANSWER 2 is never sent", ConflictKit.answers(x, ConflictRecords.ANSWER_REFUSED) == 0);
                    ScriptedEngine.Received cb = ConflictKit.about(ConflictRecords.BAND_CASUALTIES, b).get(0);
                    check("/kill: no killer", cb.aux() == ConflictRecords.BY_NONE && ConflictKit.uuidAt(cb, ConflictRecords.Casualties.KILLER_MOST) == null);
                    check("the guard", cb.view().getInt(ConflictRecords.Casualties.BODY) == ConflictKit.bodyId(WARRIOR));
                    check("and no ENVOY_ANSWER from B", ConflictKit.about(ConflictRecords.ENVOY_ANSWER, b).isEmpty());
                    List<ScriptedEngine.Received> ts = ConflictKit.tributes(a);
                    int[] counts = {3, 1};
                    for (int i = 0; i < 2; i++) {
                        ScriptedEngine.Received t = ts.get(i);
                        check("one item", t.aux() == 1);
                        check(counts[i] + " emeralds", t.view().getInt(ConflictRecords.TributePaid.ITEMS + ConflictRecords.I_ITEM) == RtsimTests.session().itemId(Items.EMERALD)
                                && t.view().getInt(ConflictRecords.TributePaid.ITEMS + ConflictRecords.I_COUNT) == counts[i]);
                        check("from the player", payer.getUUID().equals(ConflictKit.uuidAt(t, ConflictRecords.TributePaid.PLAYER_MOST)));
                        check("to the faction", t.view().getLong(ConflictRecords.TributePaid.FACTION) == faction);
                        check("with a Java op id", (t.view().getLong(ConflictRecords.TributePaid.OP_ID) & Obs.JAVA_OP_BIT) != 0);
                    }
                    check("none to B", ConflictKit.tributes(b).isEmpty());
                    // The engine withdraws a demand whose party fell to no player (DEMAND_SETTLED 5).
                    ConflictKit.settled(faction, b, null, ConflictRecords.SETTLED_WITHDRAWN, 0);
                })
                .thenWaitUntil(() -> check("B's demand withdrawn", ConflictKit.party(b).demand() == null))
                .thenExecute(() -> {
                    check("DEMAND_SETTLED 5 taken (" + bridge.lastSettled + ")", bridge.lastSettled == ConflictRecords.SETTLED_WITHDRAWN);
                    check("B's name is gone", envoyB[0].getCustomName() == null);
                    ConflictKit.recall(a, 1, ConflictRecords.RECALL_DONE, 1, c.x - 100, c.z);
                    ConflictKit.recall(b, 1, ConflictRecords.RECALL_DONE, 1, c.x - 100, c.z);
                })
                .thenWaitUntil(() -> {
                    check("A is recalled", !ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, a).isEmpty());
                    check("B is recalled", !ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, b).isEmpty());
                })
                .thenExecute(() -> record(test, "envoy", "refused " + (bridge.blockedHits - blockedHits[0]) + " hits, paid 3 and 1 emeralds, the arrow was one insult"))
                .thenSucceed();
    }

    // ---- 16: tribute in a grove's chest -------------------------------------------------------------

    /**
     * Tribute lands in a grove's chest when the grove loads (§7 test 16; MZB-419, MZB-422, MZB-423). Native engine only:
     * a grove with its Umvuthi adopted and no chest, whose masked payer pays the desires (the engine owes the gift
     * of {@code GroveTrade.nuggets(desires)} nuggets and has nowhere to put it: no HAVEN_CHEST is sent). The grove is
     * released (CAMP_OUT, the Umvuthi unloaded), a chest is set at its middle chunk with only that chunk loaded, and
     * the grove is loaded again: HAVEN_CHEST comes first, with the chest's place, the dimension and all 27 slots
     * free, then the engine's transfers into it are applied (answered APPLIED, none FAILED: the grove counts the answers
     * for its chest alone, MZB-648) and the chest holds exactly
     * the owed nuggets, which stays so; the last HAVEN_CHEST tells the slots left. A chest with no free slot is told as
     * 0: a second player's payment (the engine owes another gift) puts nothing in it over 100 ticks, and with room
     * made again (the scan is brought forward) the gift lands.
     */
    @GameTest(template = ARENA, timeoutTicks = 6000, batch = "umvuthana_chest", skyAccess = true)
    public static void tributeLandsInAGroveChestWhenTheGroveLoads(GameTestHelper helper) {
        String test = "tributeLandsInAGroveChestWhenTheGroveLoads";
        if (!(RtsimTests.engine() instanceof NativeEngine)) {
            record(test, "skipped", "the grove is the engine's only on the native engine");
            helper.succeed();
            return;
        }
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove g = new Grove(far, far.k, far.z0);
        EntityUmvuthi[] u = new EntityUmvuthi[1];
        UUID[] id = new UUID[1];
        ServerPlayer[] payer = new ServerPlayer[2];
        BlockPos[] chest = new BlockPos[1];
        int[] expected = new int[1], emptyAt = new int[1];
        long[] transfers = new long[2];
        List<Integer> dirt = new ArrayList<>();
        GroveChests chests = GroveChests.INSTANCE;

        helper.startSequence()
                .thenExecute(g::force)
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    g.lay().declare(MicRtsimMowziesMobs.GROVE_STRUCTURE, GroveKind.INSTANCE);
                    u[0] = MowzieKit.umvuthi(level, g.at(-10, 0));
                    id[0] = u[0].getUUID();
                })
                .thenWaitUntil(adopted(u))
                .thenExecute(() -> {
                    payer[0] = MowzieKit.player(helper, g.at(0, 8).add(0, 1, 0), "mowzie-chest-payer-1");
                    payer[1] = MowzieKit.player(helper, g.at(0, 10).add(0, 1, 0), "mowzie-chest-payer-2");
                    MowzieKit.mask(payer[0]);
                    MowzieKit.mask(payer[1]);
                    expected[0] = GroveTrade.nuggets(u[0].getDesires());
                    check("the desires are gold (" + expected[0] + " nuggets)", expected[0] > 0);
                    check("the grove has told no chest", chests.chestOf(g.faction) == null && havens(g).isEmpty());
                    check("the jar remembers the payer", MowzieKit.payUmvuthi(payer[0], u[0]));
                })
                .thenWaitUntil(() -> check("CHIEF_PAID reached the engine", paidRecords(g).size() == 1))
                .thenIdle(40)
                .thenExecute(() -> {
                    check("still no chest told, so no HAVEN_CHEST (" + havens(g).size() + ")", havens(g).isEmpty() && chests.chestOf(g.faction) == null);
                    far.releaseRect(g.cx - 2, g.cz - 2, g.cx + 2, g.cz + 2);
                })
                .thenWaitUntil(() -> check("CAMP_OUT", !ConflictKit.campOuts(g.faction).isEmpty()))
                .thenWaitUntil(gone(level, () -> u[0]))
                .thenExecute(() -> far.force(g.cx, g.cz))
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    chest[0] = BlockPos.containing(g.at(4, 4));
                    level.setBlock(chest[0], Blocks.CHEST.defaultBlockState(), 3);
                    Container c = chestAt(level, chest[0]);
                    check("the chest is empty", emptySlots(c) == c.getContainerSize() && c.getContainerSize() == 27);
                    check("only the middle chunk is loaded: the camp is out", !g.site().in());
                    g.far.forceRect(g.cx - 2, g.cz - 2, g.cx + 2, g.cz + 2);
                })
                .thenWaitUntil(() -> {
                    chests.scanNextTick();
                    g.far.ready().run();
                    check("HAVEN_CHEST has come", !havens(g).isEmpty());
                })
                .thenExecute(() -> {
                    ByteBuffer h = havens(g).get(0).view();
                    check("the chest's place (" + h.getInt(ConflictRecords.HavenChest.X) + ", " + h.getInt(ConflictRecords.HavenChest.Y) + ", "
                                    + h.getInt(ConflictRecords.HavenChest.Z) + ")",
                            h.getInt(ConflictRecords.HavenChest.X) == chest[0].getX() && h.getInt(ConflictRecords.HavenChest.Y) == chest[0].getY()
                                    && h.getInt(ConflictRecords.HavenChest.Z) == chest[0].getZ());
                    check("the dimension", h.getInt(ConflictRecords.HavenChest.DIM) == RtsimTests.session().dimId(level.dimension()));
                    check("all 27 slots free (" + h.getInt(ConflictRecords.HavenChest.FREE) + ")", h.getInt(ConflictRecords.HavenChest.FREE) == 27);
                })
                .thenWaitUntil(() -> {
                    chests.scanNextTick();
                    check("the chest holds the gift (" + nuggetsIn(chestAt(level, chest[0])) + " of " + expected[0] + ")",
                            nuggetsIn(chestAt(level, chest[0])) == expected[0]);
                })
                .thenIdle(100)
                .thenExecute(() -> {
                    Container c = chestAt(level, chest[0]);
                    check("the gift stays exactly as it was (" + nuggetsIn(c) + ")", nuggetsIn(c) == expected[0]);
                    // The transfers into this chest alone (the grove counts the answers for the place it told of), not
                    // every COMMAND_RESULT after the HAVEN_CHEST: other commands of the engine's are answered meanwhile.
                    long applied = chests.transfersApplied, failed = chests.transfersFailed;
                    check("a HAVEN_CHEST of the grove is on record", !havens(g).isEmpty());
                    check("the transfers into the chest were applied (" + applied + ") and none failed (" + failed + ")", applied >= 1 && failed == 0);
                    check("the grove saw its chest take them (" + chests.transfers + ")", chests.transfers >= applied + failed);
                    emptyAt[0] = emptySlots(c);
                    check("the last HAVEN_CHEST tells the slots left (" + chests.freeOf(g.faction) + " of " + emptyAt[0] + ")",
                            chests.freeOf(g.faction) == emptyAt[0] && emptyAt[0] < 27);
                    record(test, "gift", expected[0] + " nuggets in " + (27 - emptyAt[0]) + " slots, " + applied + " transfers applied");
                    // The chest is made full: dirt in every free slot, and the grove tells the engine so.
                    for (int i = 0; i < c.getContainerSize(); i++)
                        if (c.getItem(i).isEmpty()) {
                            c.setItem(i, new ItemStack(Items.DIRT));
                            dirt.add(i);
                        }
                    chests.scanNextTick();
                })
                .thenWaitUntil(() -> check("the grove told the full chest as 0 (" + chests.freeOf(g.faction) + ")", chests.freeOf(g.faction) == 0))
                .thenExecute(() -> {
                    Entity e = level.getEntity(id[0]);
                    check("the Umvuthi is loaded again, tagged", e instanceof EntityUmvuthi && MowzieKit.tag(e) != null);
                    check("a new object: the old one was saved and unloaded, not tracked again (MZB-662)", e != u[0]);
                    transfers[0] = chests.transfers;
                    paidEverCheck(g, 1);
                    check("the second payer is a stranger to the Umvuthi", !((EntityUmvuthi) e).hasTradedWith(payer[1]));
                    check("the jar remembers the second payer", MowzieKit.payUmvuthi(payer[1], (EntityUmvuthi) e));
                })
                .thenWaitUntil(() -> check("a second CHIEF_PAID reached the engine (" + paidRecords(g).size() + ")", paidRecords(g).size() == 2))
                .thenIdle(100)
                .thenExecute(() -> {
                    Container c = chestAt(level, chest[0]);
                    check("a full chest took nothing (" + nuggetsIn(c) + ")", nuggetsIn(c) == expected[0] && emptySlots(c) == 0);
                    check("no transfer was answered into it (" + (chests.transfers - transfers[0]) + ")", chests.transfers == transfers[0]);
                    check("it is still told as full", chests.freeOf(g.faction) == 0);
                    for (int slot : dirt)
                        c.setItem(slot, ItemStack.EMPTY);
                    chests.scanNextTick();
                })
                .thenWaitUntil(() -> {
                    chests.scanNextTick();
                    check("the second gift lands (" + nuggetsIn(chestAt(level, chest[0])) + " of " + expected[0] + " and more)",
                            nuggetsIn(chestAt(level, chest[0])) > expected[0]);
                })
                .thenExecute(() -> record(test, "second gift", nuggetsIn(chestAt(level, chest[0])) + " nuggets in all after the room was made"))
                .thenSucceed();
    }

    /**
     * A chest that nothing can insert into (MZB-642): a {@code ChestBlockEntity} on a block NeoForge registers no item
     * handler for, as Lootr's chests are (they keep their loot table, so show all slots free). Made on the enchanting
     * table's block with a type of its own, not registered, so the test removes it before its chunk can be saved.
     */
    private static final class HandlerlessChest extends ChestBlockEntity {
        static final BlockEntityType<HandlerlessChest> TYPE =
                BlockEntityType.Builder.of(HandlerlessChest::new, Blocks.ENCHANTING_TABLE).build(null);

        HandlerlessChest(BlockPos pos, BlockState state) {
            super(TYPE, pos, state);
        }
    }

    private static void removeHandlerless(ServerLevel level, BlockPos pos) {
        LevelChunk chunk = level.getChunkSource().getChunkNow(pos.getX() >> 4, pos.getZ() >> 4);
        if (chunk != null && chunk.getBlockEntity(pos, LevelChunk.EntityCreationType.CHECK) instanceof HandlerlessChest) {
            chunk.removeBlockEntity(pos);
            chunk.setBlockState(pos, Blocks.AIR.defaultBlockState(), false);
        }
    }

    /**
     * A container with no item handler is never a grove's chest (MZB-642; the review's P2-1: the pack's Lootr turns the
     * grove's worldgen chests into such chests, so every transfer into one FAILED and was told again, for ever).
     * A chest-like block entity with all 27 slots free and no handler stands nearer the middle than a vanilla chest:
     * with only it there, a scan sends no HAVEN_CHEST and the grove names no chest; once the vanilla chest stands, the
     * first HAVEN_CHEST names that one, with 27 free. Runs on either engine (the record is the Java side's).
     */
    @GameTest(template = ARENA, timeoutTicks = 1200, batch = "umvuthana_chest_handler", skyAccess = true)
    public static void aChestWithNoItemHandlerIsNeverTheGrovesHaven(GameTestHelper helper) {
        String test = "aChestWithNoItemHandlerIsNeverTheGrovesHaven";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove g = new Grove(far, far.k, far.z0);
        GroveChests chests = GroveChests.INSTANCE;
        BlockPos[] stuck = new BlockPos[1], real = new BlockPos[1];

        helper.startSequence()
                .thenExecute(g::force)
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    g.lay().declare();
                    stuck[0] = BlockPos.containing(g.at(3, 3));
                    real[0] = BlockPos.containing(g.at(-9, -9));
                    ConflictKit.onEnd(helper, () -> removeHandlerless(level, stuck[0]));
                    level.setBlock(stuck[0], Blocks.ENCHANTING_TABLE.defaultBlockState(), 3);
                    level.removeBlockEntity(stuck[0]);
                    level.setBlockEntity(new HandlerlessChest(stuck[0], level.getBlockState(stuck[0])));
                    BlockEntity be = level.getBlockEntity(stuck[0]);
                    check("a chest block entity stands at " + stuck[0].toShortString() + " (" + be + ")", be instanceof HandlerlessChest);
                    check("all of its slots show free", emptySlots((Container) be) == 27);
                    check("and nothing can insert into it", level.getCapability(Capabilities.ItemHandler.BLOCK, stuck[0], null) == null);
                    check("it is in the grove's footprint", g.site().contains(Vec3.atCenterOf(stuck[0])));
                })
                .thenWaitUntil(() -> check("the grove is in", g.site().in()))
                .thenExecute(chests::scanNextTick)
                .thenIdle(45)
                .thenExecute(() -> {
                    check("no HAVEN_CHEST for a chest with no handler (" + havens(g).size() + ")", havens(g).isEmpty());
                    check("and the grove names no chest (" + chests.chestOf(g.faction) + ")", chests.chestOf(g.faction) == null);
                    level.setBlock(real[0], Blocks.CHEST.defaultBlockState(), 3);
                    check("a vanilla chest stands further off", level.getBlockEntity(real[0]) instanceof ChestBlockEntity
                            && level.getCapability(Capabilities.ItemHandler.BLOCK, real[0], null) != null);
                    chests.scanNextTick();
                })
                .thenWaitUntil(() -> check("HAVEN_CHEST has come for the vanilla chest", !havens(g).isEmpty()))
                .thenExecute(() -> {
                    ByteBuffer h = havens(g).get(0).view();
                    check("the first names the vanilla chest, not the nearer one ("
                                    + h.getInt(ConflictRecords.HavenChest.X) + ", " + h.getInt(ConflictRecords.HavenChest.Y) + ", " + h.getInt(ConflictRecords.HavenChest.Z) + ")",
                            h.getInt(ConflictRecords.HavenChest.X) == real[0].getX() && h.getInt(ConflictRecords.HavenChest.Y) == real[0].getY()
                                    && h.getInt(ConflictRecords.HavenChest.Z) == real[0].getZ());
                    check("with 27 free", h.getInt(ConflictRecords.HavenChest.FREE) == 27);
                    check("the grove's chest is that one", real[0].equals(chests.chestOf(g.faction)));
                    removeHandlerless(level, stuck[0]);
                    record(test, "haven", "the chest with no handler was skipped for the vanilla one at " + real[0].toShortString());
                })
                .thenSucceed();
    }

    private static void paidEverCheck(Grove g, int n) {
        check("CHIEF_PAID so far (" + paidRecords(g).size() + ")", paidRecords(g).size() == n);
    }

    // ---- 17: raptors hunt, and an owed hunt ----------------------------------------------------------

    /**
     * The jar's raptors hunt the base's animals when the band is embodied (§7 test 17, the embodied half; MZB-428,
     * MZB-442). A raptor unit stands still at the middle of a floor with a pack of four followers, and two cows and a
     * sheep (no AI, so they stay) stand 5 to 8 blocks off. The raptor's hunting goal picks a target at random, so the
     * test waits for the first animal to die and asks that the one who last hurt it is an Umvuthana of the pack.
     * The band is then recalled.
     */
    @GameTest(template = ARENA, timeoutTicks = 5000, batch = "umvuthana_raptor_hunt", skyAccess = true)
    public static void raptorsHuntTheBasesAnimals(GameTestHelper helper) {
        String test = "raptorsHuntTheBasesAnimals";
        Stage st = new Stage(helper, test);
        ServerLevel level = st.level;
        long[] agent = new long[1];
        Animal[] prey = new Animal[3];
        long[] start = new long[1];

        helper.startSequence()
                .thenExecute(() -> st.force(1))
                .thenWaitUntil(st.far.ready())
                .thenExecute(() -> {
                    st.lay();
                    agent[0] = band(helper, st, new ConflictKit.StackSpec(RAPTOR, 1, false));
                    prey[0] = animal(level, EntityType.COW, st.at(6, 0));
                    prey[1] = animal(level, EntityType.COW, st.at(-5, 4));
                    prey[2] = animal(level, EntityType.SHEEP, st.at(0, -8));
                })
                .thenWaitUntil(() -> {
                    ConflictBridge.Party p = ConflictKit.party(agent[0]);
                    check("the raptor stands", p != null && p.stacks().size() == 1 && p.stacks().get(0).state() == ConflictBridge.StackState.PLACED
                            && p.stacks().get(0).bodies().size() == 1);
                    check("it is the jar's raptor", p.stacks().get(0).bodies().get(0) instanceof EntityUmvuthanaRaptor);
                })
                .thenIdle(20)
                .thenExecute(() -> {
                    check("its pack stands (" + everyone(agent[0]).size() + ")", everyone(agent[0]).size() == 5);
                    start[0] = level.getGameTime();
                })
                .thenWaitUntil(() -> {
                    boolean any = false;
                    for (Animal a : prey)
                        any |= !a.isAlive();
                    check("an animal has been hunted", any);
                })
                .thenExecute(() -> {
                    int dead = 0;
                    for (Animal a : prey) {
                        if (a.isAlive())
                            continue;
                        dead++;
                        LivingEntity by = a.getLastHurtByMob();
                        check(a.getType() + " died to an Umvuthana (" + by + ")", by instanceof EntityUmvuthanaRaptor || by instanceof EntityUmvuthanaFollowerToRaptor);
                    }
                    record(test, "hunted", dead + " of 3 animals in " + (level.getGameTime() - start[0]) + " ticks");
                    ConflictKit.recall(agent[0], 1, ConflictRecords.RECALL_DONE, 1, st.centre.x - 200, st.centre.z);
                })
                .thenWaitUntil(() -> check("BAND_DEMATERIALIZED", !ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, agent[0]).isEmpty()))
                .thenSucceed();
    }

    /**
     * An owed hunt is applied when its district loads, and kills the nearest eligible animals (§7 test 17, the owed half;
     * MZB-426 to MZB-429, MZB-442; the circle is 30 blocks, not the engine's 48, to keep the chunks few: 21 of them).
     * Seven animals of the jar's eligible kinds stand at 3, 5, 7, 9, 11, 14 and 17 blocks from the middle, among five
     * that are no prey: a named cow, a horse with an owner (not tame), a tame horse (no owner), a leashed sheep, and a
     * leashed sheep in a chunk of the circle's outer ring, 26 blocks out. The horses are here because a horse has no
     * attack damage, so the damage rule does not spare it (a tamed parrot is spared by its 3.0 whatever its taming:
     * the first form of this test passed with the owner and tame rules deleted); each stands nearer than the fifth
     * prey, so with either rule deleted the first hunt kills it and the count of five fails. All stay (no AI), and
     * every chunk is released until none is loaded. OWED_HUNT (count 5): held, nothing killed with the chunks out;
     * with only the middle 3 x 3 loaded, after 60 ticks it is still held and all live; with the rest of the circle
     * loaded it is applied: exactly the five nearest eligible die, the five ineligible and the two farthest live,
     * their drops lie on the ground, and HUNT_APPLIED is journaled with the op and 5. The same op again is ignored.
     * A second hunt that replaces it (rest 1) kills one (the 14-block cow). A third that names the first (stale)
     * kills its count, 1 (the 17-block cow). A fourth is sent with every chunk out again, and applied when all load
     * together (MZB-645): the leashed sheep, whose leash is data until its first tick, are what a hunt that counted
     * isLeashed would kill (count 10); it finds nothing and kills none, though it is answered.
     */
    @GameTest(template = ARENA, timeoutTicks = 6000, batch = "umvuthana_owed_hunt", skyAccess = true)
    public static void anOwedHuntIsAppliedWhenItsDistrictLoadsAndKillsTheNearestEligibleAnimals(GameTestHelper helper) {
        String test = "anOwedHuntIsAppliedWhenItsDistrictLoadsAndKillsTheNearestEligibleAnimals";
        Stage st = new Stage(helper, test);
        ServerLevel level = st.level;
        String dim = RtsimTests.dimension(helper);
        OwedHunts hunts = ConflictKit.bridge().owedHunts();
        long faction = ConflictKit.newFaction(), district = 0x17B0_0001L;
        long op1 = ConflictKit.newOp(), op2 = ConflictKit.newOp(), op3 = ConflictKit.newOp(), op4 = ConflictKit.newOp();
        // The eligible prey, nearest first, then the ineligible.
        double[][] eligible = {{3, 0}, {0, -5}, {-7, 0}, {0, 9}, {-8, -8}, {14, 0}, {0, -17}};
        UUID[] prey = new UUID[eligible.length], no = new UUID[5];
        // The objects the unload waits wait on (MZB-662): as made, then as loaded again before the second release.
        Entity[] preyMade = new Entity[eligible.length], noMade = new Entity[5], noLoaded = new Entity[5];
        long[] received0 = new long[1], ignored0 = new long[1], applied0 = new long[1], killed0 = new long[1];
        int[] held0 = new int[1];
        int cxBlock = st.cx * 16 + 8, czBlock = st.cz * 16 + 8, radius = 30;

        helper.startSequence()
                .thenExecute(() -> st.force(2))
                .thenWaitUntil(st.far.ready())
                .thenExecute(() -> {
                    st.lay();
                    for (int i = 0; i < eligible.length; i++) {
                        Vec3 at = st.at(eligible[i][0], eligible[i][1]);
                        preyMade[i] = i == 3 ? animal(level, EntityType.SHEEP, at) : animal(level, EntityType.COW, at);
                        prey[i] = preyMade[i].getUUID();
                    }
                    Cow named = animal(level, EntityType.COW, st.at(1, -1));
                    named.setCustomName(net.minecraft.network.chat.Component.literal("Daisy"));
                    // A horse has no attack damage (a parrot's 3.0 would spare it whatever its taming) and fits the volume
                    // rule, so only the owner and tame rules can spare these two. An owner is what the save keeps of a
                    // taming for a TamableAnimal; a horse keeps its tame flag alone, so one has the flag and one the owner.
                    Horse owned = animal(level, EntityType.HORSE, st.at(-3, 2));
                    owned.setOwnerUUID(UUID.randomUUID());
                    Horse tame = animal(level, EntityType.HORSE, st.at(-4, -3));
                    tame.setTamed(true);
                    for (Horse h : List.of(owned, tame)) {
                        var damage = h.getAttribute(Attributes.ATTACK_DAMAGE);
                        float volume = h.getBbWidth() * h.getBbWidth() * h.getBbHeight();
                        check("a horse is not spared by its damage (" + damage + ") or its size (" + volume + ")",
                                (damage == null || damage.getValue() < 3.0) && volume > 0.1 && volume < 6.0f);
                    }
                    check("the owned horse is not tame, the tame one has no owner", !owned.isTamed() && owned.getOwnerUUID() != null
                            && tame.isTamed() && tame.getOwnerUUID() == null);
                    Sheep leashed = animal(level, EntityType.SHEEP, st.at(3, 3));
                    leash(level, leashed, st.at(5, 3));
                    // In a chunk of the circle that only the second force loads (the platform is the middle 3 x 3).
                    Vec3 outerAt = st.at(26, 0);
                    pad(level, st.top, outerAt);
                    Sheep outer = animal(level, EntityType.SHEEP, outerAt);
                    leash(level, outer, st.at(28, 0));
                    check("the outer sheep is in a chunk the middle 3 x 3 does not hold",
                            (outer.blockPosition().getX() >> 4) == st.cx + 2 && (outer.blockPosition().getZ() >> 4) == st.cz);
                    noMade[0] = named;
                    noMade[1] = owned;
                    noMade[2] = tame;
                    noMade[3] = leashed;
                    noMade[4] = outer;
                    for (int i = 0; i < no.length; i++)
                        no[i] = noMade[i].getUUID();
                    st.far.releaseRect(st.cx - 2, st.cz - 2, st.cx + 2, st.cz + 2);
                })
                .thenWaitUntil(gone(level, preyMade, noMade))
                .thenExecute(() -> {
                    received0[0] = hunts.received;
                    ignored0[0] = hunts.ignored;
                    applied0[0] = hunts.appliedHunts;
                    killed0[0] = hunts.killed;
                    held0[0] = hunts.heldCount();
                    ConflictKit.owedHunt(faction, district, op1, 0, dim, 5, cxBlock, czBlock, radius, 0);
                })
                .thenWaitUntil(() -> check("OWED_HUNT received", hunts.received == received0[0] + 1))
                .thenIdle(45)
                .thenExecute(() -> {
                    check("held with every chunk out (" + hunts.heldCount() + ")", hunts.heldCount() == held0[0] + 1 && hunts.appliedHunts == applied0[0]);
                    st.force(1);
                })
                .thenWaitUntil(st.far.ready())
                .thenIdle(60)
                .thenExecute(() -> {
                    check("still held with only the middle 3 x 3 loaded", hunts.heldCount() == held0[0] + 1 && hunts.appliedHunts == applied0[0]);
                    check("the leash held through the reload", level.getEntity(no[3]) instanceof Mob m && m.isLeashed());
                    check("and the owner", level.getEntity(no[1]) instanceof AbstractHorse o && o.getOwnerUUID() != null && !o.isTamed());
                    check("and the taming", level.getEntity(no[2]) instanceof AbstractHorse q && q.isTamed() && q.getOwnerUUID() == null);
                    check("and the name", level.getEntity(no[0]) instanceof Cow cw && cw.hasCustomName());
                    for (UUID p : prey)
                        check("prey " + p + " lives", alive(level, p));
                    for (int i = 0; i < 4; i++) // not the outer sheep: whether its chunk holds its entities yet is not claimed
                        check("the ineligible " + no[i] + " lives", alive(level, no[i]));
                    st.force(2);
                })
                .thenWaitUntil(() -> check("the hunt is applied", hunts.appliedHunts == applied0[0] + 1))
                .thenExecute(() -> {
                    check("held no more", hunts.heldCount() == held0[0]);
                    check("five killed (" + (hunts.killed - killed0[0]) + ")", hunts.killed == killed0[0] + 5);
                    check("the op is the last applied", hunts.lastApplied(faction, district) == op1);
                    for (int i = 0; i < prey.length; i++)
                        check("prey " + i + (i < 5 ? " (near) is dead" : " (far) lives"), alive(level, prey[i]) == (i >= 5));
                    for (UUID p : no)
                        check("the ineligible " + p + " lives", alive(level, p));
                    int drops = 0;
                    for (ItemEntity d : level.getEntitiesOfClass(ItemEntity.class, new AABB(st.centre, st.centre).inflate(32)))
                        drops += d.getItem().getCount();
                    check("the drops lie on the ground (" + drops + ")", drops >= 5);
                })
                .thenWaitUntil(() -> check("the outer sheep's leash is restored", level.getEntity(no[4]) instanceof Mob m && m.isLeashed()))
                .thenWaitUntil(() -> {
                    List<ScriptedEngine.Received> r = RtsimTests.recordsWith(RtsimTests.engine(), ConflictRecords.HUNT_APPLIED, ConflictRecords.HuntApplied.OP_ID, op1);
                    check("HUNT_APPLIED is journaled", r.size() == 1);
                    check("with five killed", r.get(0).view().getInt(ConflictRecords.HuntApplied.KILLED) == 5);
                    check("for the grove's faction", r.get(0).view().getLong(ConflictRecords.HuntApplied.FACTION) == faction
                            && r.get(0).view().getLong(ConflictRecords.HuntApplied.DISTRICT) == district);
                })
                .thenExecute(() -> {
                    ConflictKit.owedHunt(faction, district, op1, 0, dim, 5, cxBlock, czBlock, radius, 0);
                })
                .thenWaitUntil(() -> check("the same op again is ignored", hunts.ignored == ignored0[0] + 1))
                .thenIdle(25)
                .thenExecute(() -> {
                    check("nothing more was applied or killed", hunts.appliedHunts == applied0[0] + 1 && hunts.killed == killed0[0] + 5 && hunts.heldCount() == held0[0]);
                    check("no second HUNT_APPLIED", RtsimTests.recordsWith(RtsimTests.engine(), ConflictRecords.HUNT_APPLIED, ConflictRecords.HuntApplied.OP_ID, op1).size() == 1);
                    ConflictKit.owedHunt(faction, district, op2, op1, dim, 10, cxBlock, czBlock, radius, 1);
                })
                .thenWaitUntil(() -> check("the replacing hunt is applied", hunts.appliedHunts == applied0[0] + 2))
                .thenExecute(() -> {
                    check("it killed the rest, 1 (" + (hunts.killed - killed0[0]) + ")", hunts.killed == killed0[0] + 6);
                    check("the 14-block cow is dead, the 17-block cow lives", !alive(level, prey[5]) && alive(level, prey[6]));
                    ConflictKit.owedHunt(faction, district, op3, op1, dim, 1, cxBlock, czBlock, radius, 9);
                })
                .thenWaitUntil(() -> check("the stale-replacing hunt is applied", hunts.appliedHunts == applied0[0] + 3))
                .thenExecute(() -> {
                    check("it killed its count, 1 (" + (hunts.killed - killed0[0]) + ")", hunts.killed == killed0[0] + 7);
                    check("the 17-block cow is dead", !alive(level, prey[6]));
                    for (int i = 0; i < no.length; i++) {
                        noLoaded[i] = level.getEntity(no[i]);
                        check("the ineligible " + no[i] + " is loaded before the release", noLoaded[i] != null);
                    }
                    st.far.releaseRect(st.cx - 2, st.cz - 2, st.cx + 2, st.cz + 2); // the fourth meets the district as it loads
                })
                .thenWaitUntil(gone(level, noLoaded))
                .thenExecute(() -> {
                    received0[0] = hunts.received;
                    ConflictKit.owedHunt(faction, district, op4, 0, dim, 10, cxBlock, czBlock, radius, 0);
                })
                .thenWaitUntil(() -> check("OWED_HUNT received", hunts.received == received0[0] + 1))
                .thenIdle(45)
                .thenExecute(() -> {
                    check("held with every chunk out", hunts.heldCount() == held0[0] + 1 && hunts.appliedHunts == applied0[0] + 3);
                    st.force(2);
                })
                .thenWaitUntil(() -> check("the last hunt is applied", hunts.appliedHunts == applied0[0] + 4))
                .thenExecute(() -> {
                    check("it found nothing (" + (hunts.killed - killed0[0]) + ")", hunts.killed == killed0[0] + 7);
                    for (UUID p : no)
                        check("the ineligible " + p + " lives", alive(level, p));
                })
                .thenWaitUntil(() -> {
                    check("both leashed sheep are leashed again", level.getEntity(no[3]) instanceof Mob a && a.isLeashed()
                            && level.getEntity(no[4]) instanceof Mob b && b.isLeashed());
                })
                .thenWaitUntil(() -> {
                    long[] ops = {op2, op3, op4};
                    int[] want = {1, 1, 0};
                    for (int i = 0; i < ops.length; i++) {
                        List<ScriptedEngine.Received> r = RtsimTests.recordsWith(RtsimTests.engine(), ConflictRecords.HUNT_APPLIED, ConflictRecords.HuntApplied.OP_ID, ops[i]);
                        check("HUNT_APPLIED for op " + (i + 2) + " is journaled", r.size() == 1);
                        check("with " + want[i] + " killed", r.get(0).view().getInt(ConflictRecords.HuntApplied.KILLED) == want[i]);
                    }
                })
                .thenExecute(() -> record(test, "hunts", "5 of 5, then 1, 1, 0; the named cow, the owned horse, the tame horse and the two leashed sheep lived"))
                .thenSucceed();
    }

    // ---- 20: the share of a fight ---------------------------------------------------------------------

    private static final String[] KINDS = {"Umvuthi", "grove warrior", "raptor with its four followers", "Frostmaw"};
    private static final float[] SHARES = {5.0f, 0.4f, 2.6f, 41.7f};
    private static final int[] ATTACKERS = {1, 1, 5, 1};

    /** Makes the bodies of one kind at the machine's chunk. */
    private static List<Mob> bodies(int kind, ServerLevel level, BlockPos chest) {
        Vec3 at = new Vec3(chest.getX() + 4.5, chest.getY(), chest.getZ() + 2.5);
        List<Mob> out = new ArrayList<>();
        switch (kind) {
            case 0 -> out.add(MowzieKit.umvuthi(level, at));
            case 1 -> out.add(MowzieKit.warrior(level, at));
            case 2 -> {
                EntityUmvuthanaRaptor raptor = EntityHandler.UMVUTHANA_RAPTOR.get().create(level);
                check("a raptor is made", raptor != null);
                raptor.moveTo(at.x, at.y, at.z, 0f, 0f);
                check("the raptor is added", level.addFreshEntity(raptor));
                out.add(raptor);
                for (int i = 0; i < 4; i++) {
                    EntityUmvuthanaFollowerToRaptor f = new EntityUmvuthanaFollowerToRaptor(EntityHandler.UMVUTHANA_FOLLOWER_TO_RAPTOR.get(), level, raptor);
                    f.setPos(at.x + 0.1 * Math.cos(i * Math.PI / 2), at.y, at.z + 0.1 * Math.sin(i * Math.PI / 2));
                    check("a follower is added", level.addFreshEntity(f));
                    out.add(f);
                }
            }
            default -> {
                EntityFrostmaw f = MowzieKit.stray(level, EntityHandler.FROSTMAW.get(), at.x, at.y, at.z, true);
                f.setActive(true);
                out.add(f);
            }
        }
        return out;
    }

    private static List<ByteBuffer> melee(long district) {
        List<ByteBuffer> out = new ArrayList<>();
        for (ByteBuffer f : IndustryTestKit.received(IndRecords.IND_FIGHT, IndRecords.Fight.DISTRICT, district))
            if (IndustryTestKit.aux(f) == IndRecords.ThreatClass.GROUND_MELEE)
                out.add(f);
        return out;
    }

    /**
     * Each of Mowzie's bodies counts as its share in an observed fight (§4.13, ledger 19; MZB-435): in a district with
     * a machine a player placed, a player-made iron golem and a body of the mod fight (the body is hurt by the golem and
     * targets it, then both are discarded at once), one body kind after another, each fight ended (20 ticks without
     * an engagement) before the next begins, as all of the mod's bodies are ground melee and would share a fight. The
     * IND_FIGHT of the Umvuthi counts 1 attacker of strength 5.0, a grove warrior 1 of 0.4, a raptor with its four
     * followers 5 of 2.6 and a Frostmaw 1 of 41.7.
     */
    @GameTest(template = YARD, batch = "umvuthana_strength", timeoutTicks = 1600)
    public static void eachMowzieBodysFightStrengthIsItsShare(GameTestHelper helper) {
        String test = "eachMowzieBodysFightStrengthIsItsShare";
        new Far(helper, test); // a hostile body is removed on Peaceful
        IndustryTestKit.fastConfig();
        IndustryTestKit.forgetAll(helper);
        // Reset at the start too: a run that failed before its end left the gap set for every later test.
        ThreatConfig.setForTest(ThreatConfig.FIGHT_GAP, null);
        ThreatConfig.setForTest(ThreatConfig.FIGHT_GAP, 20);
        ConflictKit.onEnd(helper, () -> ThreatConfig.setForTest(ThreatConfig.FIGHT_GAP, null));
        ServerLevel level = helper.getLevel();
        BlockPos chest = IndustryTestKit.inChunk(helper, 2, 2);
        long[] id = {0};

        GameTestSequence seq = helper.startSequence()
                .thenExecute(() -> {
                    IndustryTestKit.set(helper, chest, TestMachines.MACHINE.get().defaultBlockState());
                    IndustryTestKit.playerPlaced(helper, chest);
                })
                .thenWaitUntil(() -> {
                    District d = IndustryTestKit.districtAt(helper, chest);
                    check("the district", d != null);
                    id[0] = d.id;
                });
        for (int k = 0; k < KINDS.length; k++) {
            final int kind = k;
            seq = seq
                    .thenExecute(() -> {
                        check("no fight is open (" + melee(id[0]).size() + " ended)", melee(id[0]).size() == kind);
                        IronGolem golem = EntityType.IRON_GOLEM.create(level);
                        golem.setPlayerCreated(true);
                        golem.moveTo(chest.getX() + 3.5, chest.getY(), chest.getZ() + 0.5);
                        level.addFreshEntity(golem);
                        List<Mob> made = bodies(kind, level, chest);
                        for (Mob m : made) {
                            m.hurt(level.damageSources().mobAttack(golem), 1f);
                            m.setTarget(golem);
                        }
                        for (Mob m : made)
                            m.discard();
                        golem.discard();
                    })
                    .thenWaitUntil(() -> check("the fight of the " + KINDS[kind] + " ended", melee(id[0]).size() == kind + 1))
                    .thenExecute(() -> {
                        ByteBuffer f = melee(id[0]).get(kind);
                        int attackers = f.getInt(IndRecords.Fight.ATTACKERS);
                        float strength = f.getFloat(IndRecords.Fight.ATTACKER_STRENGTH);
                        record(test, KINDS[kind] + " (" + IndustryTestKit.engineName() + ")", "attackers " + attackers + ", strength " + strength);
                        check(KINDS[kind] + ": " + ATTACKERS[kind] + " attackers (" + attackers + ")", attackers == ATTACKERS[kind]);
                        check(KINDS[kind] + ": strength " + SHARES[kind] + " (" + strength + ")", Math.abs(strength - SHARES[kind]) < 0.01f);
                    });
        }
        seq.thenSucceed();
    }
}
