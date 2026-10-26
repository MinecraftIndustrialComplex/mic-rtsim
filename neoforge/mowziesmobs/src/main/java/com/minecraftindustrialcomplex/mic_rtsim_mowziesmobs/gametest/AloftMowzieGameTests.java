package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest;

import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaCrane;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaMinion;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthi;
import com.bobmowzie.mowziesmobs.server.potion.EffectHandler;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.api.TakeReason;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.AloftRules;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictNews;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ChunkWait;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.CampLine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.AgentTag;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.GroveKind;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.MicRtsimMowziesMobs;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.UmvuthiAloft;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Far;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Grove;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.mixin.EntityUmvuthiAccessor;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.particles.ParticleTypes;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.protocol.game.ClientboundLevelParticlesPacket;
import net.minecraft.network.protocol.game.ClientboundAddEntityPacket;
import net.minecraft.network.protocol.game.ClientboundSetEntityDataPacket;
import net.minecraft.network.protocol.game.ClientboundSetEntityLinkPacket;
import net.minecraft.network.protocol.game.ClientboundSetEntityMotionPacket;
import net.minecraft.network.protocol.game.ClientboundSetEquipmentPacket;
import net.minecraft.network.protocol.game.ClientboundSetPassengersPacket;
import net.minecraft.network.protocol.game.ClientboundUpdateAttributesPacket;
import net.minecraft.network.protocol.game.ClientboundUpdateMobEffectPacket;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.network.syncher.SynchedEntityData;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ChunkMap;
import net.minecraft.server.level.ServerEntity;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.effect.MobEffectInstance;
import net.minecraft.world.effect.MobEffects;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.animal.IronGolem;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.level.ExplosionDamageCalculator;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.SimpleExplosionDamageCalculator;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.FarmBlock;
import net.minecraft.world.level.block.LeavesBlock;
import net.minecraft.world.level.block.NetherPortalBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.bus.api.Event;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.EntityJoinLevelEvent;
import net.neoforged.neoforge.event.entity.EntityLeaveLevelEvent;
import net.neoforged.neoforge.event.entity.EntityTravelToDimensionEvent;
import net.neoforged.neoforge.event.entity.living.LivingDeathEvent;
import net.neoforged.neoforge.event.entity.player.PlayerEvent;
import net.neoforged.neoforge.event.tick.LevelTickEvent;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.UUID;
import java.util.function.Consumer;
import java.util.function.Supplier;

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

    /**
     * Forces the grove's row east of its 5 x 5 chunks, out to chunk {@code cx + chunks} (the grove forces {@code cx - 2} to {@code cx + 2}):
     * a body 60 blocks off the throne stands in a chunk that ticks. Rows are 20 chunks apart, so no strip meets the next grove's.
     */
    private static void forceEast(Grove g, int chunks) {
        g.far.forceRect(g.cx + 3, g.cz, g.cx + chunks, g.cz);
    }

    /** Inside {@code thenExecute}, once the strip is ready: a floor on the strip, level with the grove's, joined to it. */
    private static void layEast(Grove g, int chunks) {
        MowzieKit.platform(g.far.level, (g.cx + 2) * 16, (g.cx + chunks) * 16 + 15, g.cz * 16, g.cz * 16 + 15, (int) g.spot.y - 1);
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
        return bandAt(helper, g.faction, agent, x, g.spot.y, z, health, traded, warriors, true);
    }

    /**
     * An Invasion's band with its head at ({@code x}, {@code y}, {@code z}) heading +x and standing still: {@code warriors} warriors
     * and, with {@code chief}, the chief's stack (FLAG_CHIEF, its CHIEF_BODY away in the same hand-over, MZB-805). Without, the
     * same band as M10's: warriors alone, no chief flag, no chief body.
     */
    private static ConflictKit.Band bandAt(GameTestHelper helper, long faction, long agent, double x, double y, double z, float health,
            List<UUID> traded, int warriors, boolean chief) {
        return bandAt(helper, faction, agent, x, y, z, health, traded, warriors, chief, b -> {});
    }

    /** As above, and {@code tune} sets the band's bearing, target and speed before it is materialized (a band that marches, not one that stands). */
    private static ConflictKit.Band bandAt(GameTestHelper helper, long faction, long agent, double x, double y, double z, float health,
            List<UUID> traded, int warriors, boolean chief, Consumer<ConflictKit.Band> tune) {
        ConflictKit.Band b = ConflictKit.band(agent, faction, RtsimTests.dimension(helper));
        if (chief)
            b.flags |= ConflictRecords.Materialize.FLAG_CHIEF;
        b.purpose = ConflictRecords.PURPOSE_INVASION;
        b.arcX = x;
        b.arcZ = z;
        b.bearing = 0f;
        b.targetX = x;
        b.targetY = y;
        b.targetZ = z;
        b.speed = 0f;
        tune.accept(b);
        List<ConflictKit.StackSpec> stacks = new ArrayList<>();
        stacks.add(new ConflictKit.StackSpec(WARRIOR, warriors, false));
        if (chief)
            stacks.add(new ConflictKit.StackSpec(UMVUTHI, 0, true));
        ConflictKit.together(() -> {
            if (chief)
                ConflictKit.chiefBody(faction, agent, b.epoch, health, traded);
            ConflictKit.materialize(b, stacks);
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

    // ---- the lane and what watches it (23a to 23e, 35) -----------------------------------------------

    /** Blocks a tick a steered band walks: under the puppet's cap of 0.1 ({@link AloftRules#PACE}), so the puppet is never left behind. */
    private static final float PACE = 0.08f;

    /**
     * A flat lane of forced chunks on the far base: {@code chunks} chunks along +x on row {@code row} (rows are 20 chunks, 320 blocks,
     * apart), its centre line {@link #z} the middle of a block column. One chunk row wide, so nothing here is laid outside a forced chunk.
     */
    private static final class Lane {
        final Far far;
        final ServerLevel level;
        final int cx0, cx1, cz, x0, x1;
        final double z;
        /** The floor's y (the stone), once {@link #floor} laid it. */
        int top;

        Lane(Far far, int row, int chunks) {
            this(far, far.k, row, chunks);
        }

        /** The lane's first chunk is {@code first} (not the base's {@code k}): a strip kept clear of a grove's footprint declared at {@code k}. */
        Lane(Far far, int first, int row, int chunks) {
            this.far = far;
            level = far.level;
            cx0 = first;
            cx1 = first + chunks - 1;
            cz = far.z0 + 20 * row;
            x0 = cx0 * 16;
            x1 = cx1 * 16 + 15;
            z = cz * 16 + 8.5;
        }

        Lane force() {
            far.forceRect(cx0, cz, cx1, cz);
            return this;
        }

        /** A stone floor over the lane's width with air over it; the floor's y (a body stands at y + 1). */
        int floor() {
            top = MowzieKit.platform(level, x0, x1, cz * 16, cz * 16 + 15);
            return top;
        }

        /** The point {@code dx} blocks along the lane, one over the floor. */
        Vec3 at(double dx) {
            return new Vec3(x0 + dx, top + 1, z);
        }

        /** The block {@code dx} along the lane, {@code dy} over the floor and {@code dz} off the centre line. */
        void set(int dx, int dy, int dz, BlockState state) {
            level.setBlock(new BlockPos(x0 + dx, top + dy, cz * 16 + 8 + dz), state, 2);
        }

        /** A box over the whole lane, from under the floor to 40 over it. */
        AABB area() {
            return new AABB(x0, top - 6, cz * 16, x1 + 1, top + 40, cz * 16 + 16);
        }
    }

    /**
     * A puppet's record, filled at the end of every tick: the most it moved in a tick (across, up and down), the ticks its box was not
     * clear of blocks or gravity was on, its height at marks along x, and the foot target the rule held there. The sample is the test's
     * own, so a violation is counted here and asserted by the sequence (a throw in a per-tick hook is only logged).
     */
    private static final class Trace {
        final Supplier<ConflictBridge.Slot> slot;
        double px, py, pz;
        boolean have;
        int ticks, crowded, gravity;
        double across, up, down, lowest = Double.MAX_VALUE, highest = -Double.MAX_VALUE;
        final double[] markX, markY, markTarget;

        Trace(Supplier<ConflictBridge.Slot> slot, double... marks) {
            this.slot = slot;
            markX = marks;
            markY = new double[marks.length];
            markTarget = new double[marks.length];
            java.util.Arrays.fill(markY, Double.NaN);
        }

        void sample(ServerLevel level) {
            ConflictBridge.Slot s = slot.get();
            if (s == null || !(s.entity() instanceof EntityUmvuthi u) || u.isRemoved() || u.isDeadOrDying())
                return;
            if (have) {
                across = Math.max(across, Math.hypot(u.getX() - px, u.getZ() - pz));
                up = Math.max(up, u.getY() - py);
                down = Math.max(down, py - u.getY());
            }
            px = u.getX();
            py = u.getY();
            pz = u.getZ();
            have = true;
            ticks++;
            lowest = Math.min(lowest, py);
            highest = Math.max(highest, py);
            if (!level.noCollision(u))
                crowded++;
            if (!u.isNoGravity())
                gravity++;
            for (int i = 0; i < markX.length; i++)
                if (Double.isNaN(markY[i]) && px >= markX[i]) {
                    markY[i] = py;
                    markTarget[i] = s.travelMemo() == null ? Double.NaN : UmvuthiAloft.INSTANCE.target(s.travelMemo());
                }
        }
    }

    /** A trace of the chief's slot of a party, sampled every tick until the test ends. */
    private static Trace trace(GameTestHelper helper, ServerLevel level, long agent, double... marks) {
        Trace t = new Trace(() -> {
            ConflictBridge.Party p = ConflictKit.bridge().party(agent);
            if (p == null)
                return null;
            for (ConflictBridge.Stack st : p.stacks())
                for (ConflictBridge.Slot s : st.slots())
                    if (s.chief)
                        return s;
            return null;
        }, marks);
        MowzieKit.everyTick(helper, () -> t.sample(level));
        return t;
    }

    /** Registers {@code c} on the bus until {@code off} runs (or the test ends, passed or failed); the {@code off} to run in the sequence. */
    private static <E extends Event> Runnable listen(GameTestHelper helper, EventPriority priority, Class<E> type, Consumer<E> c) {
        NeoForge.EVENT_BUS.addListener(priority, false, type, c);
        boolean[] on = {true};
        Runnable off = () -> {
            if (on[0]) {
                on[0] = false;
                NeoForge.EVENT_BUS.unregister(c);
            }
        };
        ConflictKit.onEnd(helper, off);
        return off;
    }

    /** Recalls bands at a test's end so that no puppet floats on as its chunks are let go (UR-1); give it a few ticks after. */
    private static void recallAll(long... agents) {
        for (long a : agents) {
            ConflictBridge.Party p = ConflictKit.bridge().party(a);
            if (p != null && !p.recalling())
                ConflictKit.recall(a, p.epoch, ConflictRecords.RECALL_DONE, 20, 0, 0);
        }
    }

    private static void drop(ServerPlayer... players) {
        for (ServerPlayer p : players)
            if (p != null)
                com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers.remove(p);
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
                    op[1] = MowzieKit.reconcile(g[0].faction, epochOf(g[0]), g[0].spot, 3f, List.of(
                            CampLine.party(CHIEF_SLOT, 1, UMVUTHI, HAND_BACK, CHIEF, agent[0], ConflictKit.party(agent[0]).epoch)));
                })
                .thenWaitUntil(waits.logic(() -> check("the line in the animation is answered", ConflictKit.outcomes(g[0].faction, op[1]).size() == 1)))
                .thenExecute(() -> {
                    check("answered STALE, not " + MowzieKit.outcomeName(answerOf(g[0], op[1])), answerOf(g[0], op[1]) == STALE);
                    check("no throne body stands (" + thrones(level, g[0]).size() + ")", thrones(level, g[0]).isEmpty());
                })
                .thenWaitUntil(waits.logic(() -> check("the puppet's animation has ended", puppet[0].isRemoved())))
                .thenExecute(() -> op[2] = MowzieKit.reconcile(g[0].faction, epochOf(g[0]), g[0].spot, 3f, List.of(
                        CampLine.party(CHIEF_SLOT, 1, UMVUTHI, HAND_BACK, CHIEF, agent[0], ConflictKit.party(agent[0]).epoch))))
                .thenWaitUntil(waits.logic(() -> check("the line after the puppet is answered", ConflictKit.outcomes(g[0].faction, op[2]).size() == 1)))
                .thenIdle(20)
                .thenExecute(() -> {
                    check("answered STALE, not " + MowzieKit.outcomeName(answerOf(g[0], op[2])), answerOf(g[0], op[2]) == STALE);
                    check("no throne body of the retired generation stands (" + thrones(level, g[0]).size() + ")", thrones(level, g[0]).isEmpty());
                    check("none is held for it", MowzieKit.held(g[0].faction, CHIEF_SLOT, 1).isEmpty());
                    // The control: a live puppet is handed back to its throne.
                    op[3] = MowzieKit.reconcile(g[1].faction, epochOf(g[1]), g[1].spot, 3f, List.of(
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

    // ---- 23a: the height (MZB-806, MZB-808, MZB-809) -------------------------------------------------

    /**
     * An Invasion's chief puppet keeps its height over a lane (test 23a; MZB-806, MZB-808, MZB-809). The lane is flat, 160 blocks, with a
     * wall (6 high, across the whole lane), an oak (trunk 5, leaves to 8) and a roof (7 by 7 slabs on four posts, 4 up) across the way.
     * A masked player stands beside the start. The script: CHIEF_BODY away (health 250, the player in its memory), the MATERIALIZE
     * (2 warriors and the chief's stack), a steer along the lane at 0.08 a tick.
     * <ul>
     * <li>The puppet appears at its place in the column (within a block of it), floating, facing the way the column goes (direction 4,
     * private and synced), with health 250 and the player in its memory.</li>
     * <li>Every tick it moves at most 0.1 across, at most 0.1 up and 0.05 down, and its box is clear of blocks.</li>
     * <li>Where it passes each structure its foot is at that structure's top + 3 (within 0.05), and the rule's own target says so;
     * before the first and after the last it is at the floor's + 3.</li>
     * </ul>
     * Whether a column the look reaches is unread (MZB-808's {@code getChunkNow}) cannot be made to happen here: a chunk is readable
     * from the ticket's neighbour ring, and the body stays in a ticking chunk, so no chunk the look reads is farther than the ring. The
     * rule's skipping of an unread column is JUnit's ({@code AloftRulesTest}); the counter is logged. The control: the same band without
     * the chief's stack makes no Umvuthi, and its warriors are placed. Fails without MZB-808's look-ahead (it meets the wall), MZB-809's
     * cap (it moves at the column's pace, past 0.1).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_height", skyAccess = true)
    public static void anAloftUmvuthiKeepsItsHeight(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 8000);
        String test = "anAloftUmvuthiKeepsItsHeight";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Lane[] lane = {new Lane(far, 0, 10), new Lane(far, 1, 3)}; // the Invasion's; the control's
        long[] agent = {ConflictKit.newAgent(), ConflictKit.newAgent()};
        long faction = ConflictKit.newFaction();
        int wall = 60, oak = 90, roof = 120, start = 24, finish = 150;
        ServerPlayer[] player = new ServerPlayer[1];
        EntityUmvuthi[] puppet = new EntityUmvuthi[1];
        Trace[] trace = new Trace[1];

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    for (Lane l : lane)
                        l.force();
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    for (Lane l : lane)
                        l.floor();
                    Lane l = lane[0];
                    for (int dz = -7; dz <= 7; dz++)
                        for (int dy = 1; dy <= 6; dy++)
                            l.set(wall, dy, dz, Blocks.STONE_BRICKS.defaultBlockState());
                    BlockState leaves = Blocks.OAK_LEAVES.defaultBlockState().setValue(LeavesBlock.PERSISTENT, true);
                    for (int dy = 1; dy <= 5; dy++)
                        l.set(oak, dy, 0, Blocks.OAK_LOG.defaultBlockState());
                    for (int dy = 6; dy <= 8; dy++) {
                        int r = dy == 8 ? 1 : 2;
                        for (int x = -r; x <= r; x++)
                            for (int z = -r; z <= r; z++)
                                if ((x != 0 || z != 0 || dy > 5) && (r == 1 || Math.abs(x) + Math.abs(z) < 4))
                                    l.set(oak + x, dy, z, leaves);
                    }
                    for (int px : new int[] {-3, 3})
                        for (int pz : new int[] {-3, 3})
                            for (int dy = 1; dy <= 3; dy++)
                                l.set(roof + px, dy, pz, Blocks.OAK_LOG.defaultBlockState());
                    for (int x = -3; x <= 3; x++)
                        for (int z = -3; z <= 3; z++)
                            l.set(roof + x, 4, z, Blocks.OAK_SLAB.defaultBlockState());
                    player[0] = MowzieKit.player(helper, l.at(start - 4).add(0, 0, 5), "mowzie-aloft-height");
                    MowzieKit.mask(player[0]);
                    bandAt(helper, faction, agent[0], l.x0 + start, l.top + 1, l.z, 250f, List.of(player[0].getUUID()), 2, true);
                    Lane c = lane[1];
                    bandAt(helper, faction, agent[1], c.x0 + start, c.top + 1, c.z, 0f, List.of(), 2, false);
                })
                .thenWaitUntil(waits.logic(() -> {
                    puppet[0] = puppet(agent[0]);
                    ConflictBridge.Party control = ConflictKit.party(agent[1]);
                    int warriors = 0;
                    for (ConflictBridge.Stack st : control.stacks())
                        warriors += st.bodies().size();
                    check("the control's two warriors are placed (" + warriors + ")", warriors == 2);
                }))
                .thenExecute(() -> {
                    Lane l = lane[0];
                    EntityUmvuthi u = puppet[0];
                    ConflictBridge.Slot s = chiefSlot(agent[0]);
                    check("it floats", u.isNoGravity());
                    check("it appears at its place in the column (" + s.lag() + " blocks off)", s.lag() <= 1.0);
                    check("its foot is the floor's + 3 (" + (u.getY() - l.top) + ")", Math.abs(u.getY() - (l.top + 3)) <= 0.05);
                    check("its health is the memory's, kept to its maximum (" + u.getHealth() + " of " + u.getMaxHealth() + ")",
                            u.getHealth() == Math.min(250f, u.getMaxHealth()));
                    check("the player is in its memory", u.hasTradedWith(player[0]));
                    check("its synced direction is 4, the quarter of +x (" + u.getDirectionData() + ")", u.getDirectionData() == 4);
                    check("so is its private one (" + ((EntityUmvuthiAccessor) u).mic_rtsim_mowziesmobs$direction() + ")",
                            ((EntityUmvuthiAccessor) u).mic_rtsim_mowziesmobs$direction() == 4);
                    check("no Umvuthi stands in the control's lane", level.getEntitiesOfClass(EntityUmvuthi.class, lane[1].area()).isEmpty());
                    trace[0] = trace(helper, level, agent[0], l.x0 + wall, l.x0 + oak, l.x0 + roof, l.x0 + finish - 1);
                    ConflictKit.steer(agent[0], ConflictKit.party(agent[0]).epoch, PACE, l.x0 + finish, l.z, 0, false);
                })
                .thenWaitUntil(waits.logic(() -> check("it has come to the end of the lane (x " + puppet[0].getX() + ")",
                        puppet[0].getX() >= lane[0].x0 + finish - 1)))
                .thenIdle(5)
                .thenExecute(() -> {
                    Lane l = lane[0];
                    Trace t = trace[0];
                    double[] want = {l.top + 6 + 3, l.top + 8 + 3, l.top + 4 + 3, l.top + 3};
                    String[] what = {"the wall", "the oak", "the roof", "the end"};
                    StringBuilder log = new StringBuilder();
                    for (int i = 0; i < want.length; i++) {
                        check(what[i] + ": its foot at x " + t.markX[i] + " is " + t.markY[i] + ", not " + want[i],
                                Math.abs(t.markY[i] - want[i]) <= 0.05);
                        check(what[i] + ": the rule's target is " + t.markTarget[i] + ", not " + want[i], Math.abs(t.markTarget[i] - want[i]) <= 1.0e-6);
                        log.append(what[i]).append(' ').append(String.format("%.2f", t.markY[i] - l.top)).append(", ");
                    }
                    check("it moved at most 0.1 across (" + t.across + ")", t.across <= 0.1 + 1.0e-6);
                    check("at most 0.1 up (" + t.up + ")", t.up <= 0.1 + 1.0e-6);
                    check("at most 0.05 down (" + t.down + ")", t.down <= 0.05 + 1.0e-6);
                    check("its box was clear every tick (" + t.crowded + " not, of " + t.ticks + ")", t.crowded == 0);
                    check("its gravity stayed off (" + t.gravity + ")", t.gravity == 0);
                    check("it went over the oak's top: highest " + (t.highest - l.top), t.highest >= l.top + 8 + 3 - 0.05);
                    check("and did not stay up: lowest " + (t.lowest - l.top), t.lowest <= l.top + 3 + 0.05);
                    check("no traveller step failed (" + ConflictKit.travellerErrors() + ")", ConflictKit.travellerErrors() == 0);
                    check("the control's band made no Umvuthi", level.getEntitiesOfClass(EntityUmvuthi.class, lane[1].area()).isEmpty());
                    record(test, "heights over the floor", log + "most across " + String.format("%.3f", t.across) + ", up " + String.format("%.3f", t.up)
                            + ", down " + String.format("%.3f", t.down) + "; columns the look skipped for want of a chunk "
                            + UmvuthiAloft.INSTANCE.heightColumnsSkipped);
                    recallAll(agent);
                })
                .thenIdle(30)
                .thenExecute(() -> {
                    drop(player);
                    far.releaseAll();
                })
                .thenSucceed();
    }

    // ---- 23b: settling and rising (MZB-809, MZB-812) --------------------------------------------------

    /**
     * An aloft chief puppet settles and rises (test 23b; MZB-809, MZB-812). Lane A's puppet, then lane B's, then the control's throne
     * body, on flat floors.
     * <ol>
     * <li>A's band is steered 50 blocks on with the attack flag. The puppet stays aloft (the floor's + 3) until it is within 32 blocks
     * of the steer's end, then sinks at most 0.05 a tick. Landed, gravity is on, its restriction centre is its block and it has no
     * restriction (radius -1).</li>
     * <li>B's band is steered with no flag; an unmasked player 20 blocks off, in plain view, is its target: it settles on the ground.</li>
     * <li>With A's flag still set, a player appears, becomes the target, is made invisible and sent 200 blocks off (the target is
     * cleared): the puppet stays down 400 ticks.</li>
     * <li>The flag is dropped (a steer back to its place, with no flag) and the player is back in view: it is the target again, and
     * the puppet stays down. The player is hidden and the target cleared: it rises no sooner than 200 ticks after the last tick it had
     * a player for target, and is at its place (within 2 blocks) inside 400 ticks of rising.</li>
     * <li>An iron golem beside the route, the puppet's target: it neither sinks nor stops (it makes headway each 100 ticks). The
     * Umvuthi's SPAWN started through the ability handler makes summons whose restriction centre is the puppet's block (within a block),
     * radius 25.</li>
     * </ol>
     * The control: a throne Umvuthi's summons have the throne body's own centre (the jar's rule). Fails without the settle rule (step
     * 1 stays aloft), the per-tick {@code restrictTo} (step 5's centre is the puppet's spawn point), and with a settle on any target
     * (the golem sets it down).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_settle", skyAccess = true)
    public static void anAloftUmvuthiSettlesAndRises(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 9000);
        String test = "anAloftUmvuthiSettlesAndRises";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Lane[] lane = {new Lane(far, 0, 8), new Lane(far, 1, 4), new Lane(far, 2, 2)}; // A; B; the throne body's
        long[] agent = {ConflictKit.newAgent(), ConflictKit.newAgent()};
        long faction = ConflictKit.newFaction();
        int start = 20;
        EntityUmvuthi[] puppet = new EntityUmvuthi[2];
        EntityUmvuthi[] throne = new EntityUmvuthi[1];
        ServerPlayer[] player = new ServerPlayer[2];
        IronGolem[] golem = new IronGolem[1];
        Trace[] trace = new Trace[1];
        int[] phase = {0};
        int[] rose = {0}, sunk = {0};
        long[] playerTick = {-1}, riseTick = {-1}, dropTick = {-1};
        double[] sinkDist = {-1};
        double[] x = new double[4];
        double[] summonsAt = new double[3];
        int[] summoned = {0};

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    for (Lane l : lane)
                        l.force();
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    for (Lane l : lane)
                        l.floor();
                    for (int i = 0; i < 2; i++)
                        bandAt(helper, faction, agent[i], lane[i].x0 + start, lane[i].top + 1, lane[i].z, 0f, List.of(), 1, true);
                    throne[0] = MowzieKit.umvuthi(level, lane[2].at(8));
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < 2; i++) {
                        puppet[i] = puppet(agent[i]);
                        check("puppet " + i + " floats", puppet[i].isNoGravity());
                    }
                }))
                .thenExecute(() -> {
                    Lane a = lane[0];
                    trace[0] = trace(helper, level, agent[0]);
                    MowzieKit.everyTick(helper, () -> {
                        EntityUmvuthi u = puppet[0];
                        long now = level.getGameTime();
                        if (u.getTarget() instanceof Player)
                            playerTick[0] = now;
                        switch (phase[0]) {
                            case 1 -> {
                                if (sinkDist[0] < 0 && u.getY() < a.top + 3 - 0.01) {
                                    double[] t = chiefSlot(agent[0]).stack.target();
                                    sinkDist[0] = Math.hypot(t[0] - u.getX(), t[1] - u.getZ());
                                }
                            }
                            case 3 -> {
                                if (u.isNoGravity())
                                    rose[0]++;
                            }
                            case 4 -> {
                                if (riseTick[0] < 0 && u.isNoGravity())
                                    riseTick[0] = now;
                            }
                            case 5 -> {
                                if (!u.isNoGravity())
                                    sunk[0]++;
                                if (golem[0] != null && golem[0].isAlive())
                                    u.setTarget(golem[0]);
                            }
                            default -> {
                            }
                        }
                    });
                    phase[0] = 1;
                    ConflictKit.steer(agent[0], ConflictKit.party(agent[0]).epoch, PACE, a.x0 + start + 50, a.z, 0, true);
                    // B has no flag: a player 20 blocks off in plain view is the target that sets it down
                    Lane b = lane[1];
                    ConflictKit.steer(agent[1], ConflictKit.party(agent[1]).epoch, PACE, b.x0 + start + 50, b.z, 0, false);
                    player[0] = MowzieKit.player(helper, b.at(start + 20), "mowzie-aloft-settle-b");
                })
                .thenWaitUntil(waits.logic(() -> check("A's puppet has landed (gravity " + !puppet[0].isNoGravity() + ")", !puppet[0].isNoGravity())))
                .thenExecute(() -> {
                    Lane a = lane[0];
                    EntityUmvuthi u = puppet[0];
                    phase[0] = 2;
                    check("it stayed aloft until within 32 of the steer's end, and sank from there (" + sinkDist[0] + ")",
                            sinkDist[0] > 31.7 && sinkDist[0] <= 32.0 + 1.0e-6);
                    check("it sank at most 0.05 a tick (" + trace[0].down + ")", trace[0].down <= 0.05 + 1.0e-6);
                    check("it is on the floor (" + (u.getY() - a.top) + " over it)", Math.abs(u.getY() - (a.top + 1)) < 0.06);
                    check("its restriction centre is its block (" + u.getRestrictCenter() + " and " + u.blockPosition() + ")",
                            u.getRestrictCenter().equals(u.blockPosition()));
                    check("it has no restriction (radius " + u.getRestrictRadius() + ")", !u.hasRestriction());
                })
                .thenWaitUntil(waits.logic(() -> check("B's puppet has landed", !puppet[1].isNoGravity())))
                .thenExecute(() -> {
                    EntityUmvuthi u = puppet[1];
                    check("B's puppet targets the player (" + u.getTarget() + ")", u.getTarget() == player[0]);
                    check("B's puppet is on the floor (" + (u.getY() - lane[1].top) + " over it)", Math.abs(u.getY() - (lane[1].top + 1)) < 0.06);
                    // A: the flag is still set; a player comes, is the target, goes invisible and 200 blocks off
                    player[1] = MowzieKit.player(helper, new Vec3(puppet[0].getX() + 12, puppet[0].getY(), puppet[0].getZ()), "mowzie-aloft-settle-a");
                })
                .thenWaitUntil(waits.logic(() -> check("A's puppet targets the player", puppet[0].getTarget() == player[1])))
                .thenExecute(() -> {
                    phase[0] = 3;
                    player[1].setInvisible(true);
                    player[1].setPos(player[1].getX() + 200, player[1].getY(), player[1].getZ());
                    puppet[0].setTarget(null);
                })
                .thenIdle(400)
                .thenExecute(() -> {
                    check("with the flag set it stayed down 400 ticks (" + rose[0] + " ticks aloft)", rose[0] == 0);
                    Lane a = lane[0];
                    phase[0] = 4;
                    dropTick[0] = level.getGameTime();
                    // the flag dropped, and the player is back in plain view: it is the target again, so the rise waits on it
                    ConflictKit.steer(agent[0], ConflictKit.party(agent[0]).epoch, 0.5f, puppet[0].getX() + 11, a.z, 0, false);
                    player[1].setInvisible(false);
                    player[1].setPos(puppet[0].getX() + 12, puppet[0].getY(), puppet[0].getZ());
                })
                .thenWaitUntil(waits.logic(() -> check("A's puppet targets the player again", puppet[0].getTarget() == player[1])))
                .thenIdle(20)
                .thenExecute(() -> {
                    check("a player for target keeps it down with the flag gone", !puppet[0].isNoGravity());
                    player[1].setInvisible(true);
                    player[1].setPos(player[1].getX() + 200, player[1].getY(), player[1].getZ());
                    puppet[0].setTarget(null);
                })
                .thenWaitUntil(waits.logic(() -> check("A's puppet floats again", puppet[0].isNoGravity())))
                .thenExecute(() -> {
                    long waited = riseTick[0] - playerTick[0];
                    check("it rose no sooner than 200 ticks after its last player target (" + waited + ")", waited >= 198);
                    check("and not long after (" + waited + ")", waited <= 240);
                    check("the player was its target after the flag's drop (last at " + playerTick[0] + ", dropped at " + dropTick[0] + ")",
                            playerTick[0] > dropTick[0]);
                })
                .thenWaitUntil(waits.logic(() -> check("it is back at its place (" + chiefSlot(agent[0]).lag() + ")", chiefSlot(agent[0]).lag() <= 2.0)))
                .thenExecute(() -> {
                    check("and it came inside 400 ticks of its rise (" + (level.getGameTime() - riseTick[0]) + ")",
                            level.getGameTime() - riseTick[0] <= 400);
                    Lane a = lane[0];
                    golem[0] = EntityType.IRON_GOLEM.create(level);
                    golem[0].moveTo(puppet[0].getX() + 4, a.top + 1, a.z + 6, 0f, 0f);
                    golem[0].setNoAi(true);
                    check("the golem is added", level.addFreshEntity(golem[0]));
                    phase[0] = 5;
                    x[0] = puppet[0].getX();
                    ConflictKit.steer(agent[0], ConflictKit.party(agent[0]).epoch, PACE, puppet[0].getX() + 45, a.z, 0, false);
                })
                .thenIdle(10)
                .thenExecute(() -> {
                    check("the golem is its target (" + puppet[0].getTarget() + ")", puppet[0].getTarget() == golem[0]);
                    puppet[0].sendAbilityMessage(EntityUmvuthi.SPAWN_ABILITY);
                })
                .thenWaitUntil(waits.logic(() -> check("a summons stands among its extras", !chiefSlot(agent[0]).extras().isEmpty())))
                .thenExecute(() -> {
                    EntityUmvuthi u = puppet[0];
                    Mob first = chiefSlot(agent[0]).extras().get(0);
                    check("it is a warrior of the jar (" + first + ")", first instanceof EntityUmvuthanaMinion);
                    double off = Math.sqrt(first.getRestrictCenter().distSqr(u.blockPosition()));
                    check("its restriction centre is the puppet's block (" + off + " off)", off <= 1.0);
                    check("with radius 25 (" + first.getRestrictRadius() + ")", first.getRestrictRadius() == 25f);
                    check("the puppet floats on (sunk " + sunk[0] + " ticks)", sunk[0] == 0);
                    x[1] = u.getX();
                })
                .thenIdle(100)
                .thenExecute(() -> {
                    check("headway in 100 ticks with the golem for target (" + (puppet[0].getX() - x[1]) + ")", puppet[0].getX() - x[1] >= 3.0);
                    x[2] = puppet[0].getX();
                })
                .thenIdle(100)
                .thenExecute(() -> {
                    check("and in the next 100 (" + (puppet[0].getX() - x[2]) + ")", puppet[0].getX() - x[2] >= 3.0);
                    check("it never sank (" + sunk[0] + ")", sunk[0] == 0);
                    phase[0] = 6;
                    // the control: the throne body's summons keep its own centre
                    throne[0].sendAbilityMessage(EntityUmvuthi.SPAWN_ABILITY);
                })
                .thenWaitUntil(waits.logic(() -> {
                    List<EntityUmvuthanaMinion> made = level.getEntitiesOfClass(EntityUmvuthanaMinion.class, lane[2].area(),
                            m -> !m.isRemoved() && m.getRestrictRadius() == 25f);
                    check("the throne body has made a summons", !made.isEmpty());
                    check("its centre is the throne body's own (" + made.get(0).getRestrictCenter() + " and " + throne[0].getRestrictCenter() + ")",
                            made.get(0).getRestrictCenter().equals(throne[0].getRestrictCenter()));
                }))
                .thenExecute(() -> {
                    record(test, "settle", "sank from " + String.format("%.2f", sinkDist[0]) + " blocks, rose " + (riseTick[0] - playerTick[0])
                            + " ticks after its last player target, summons centre on the puppet's block");
                    recallAll(agent);
                })
                .thenIdle(30)
                .thenExecute(() -> {
                    drop(player);
                    far.releaseAll();
                })
                .thenSucceed();
    }

    // ---- 23c: pushes and portals (MZB-810, MZB-815) ---------------------------------------------------

    /**
     * Explosions, a fan's push and a portal leave an aloft puppet where it floats (test 23c; MZB-810, MZB-815). One puppet marches
     * along a lane with no flag; an untagged Umvuthi with its gravity off (the control, no band's) stands behind it.
     * <ol>
     * <li>A listener on {@code LevelTickEvent.Pre} (before the entities tick) explodes a blast of power 3 two blocks under the puppet and
     * another under the control: the puppet's motion is the same after the blast as before (the knockback hook zeroed it); the
     * control's changes by more than 0.15 blocks a tick. {@code knockbacksZeroed} counts the puppet's blast, once.</li>
     * <li>The same with a wind charge's blast (knockback times 1.22, no damage): the same two results, a second count.</li>
     * <li>A listener on {@code LevelTickEvent.Post} (after the entities tick) gives both a push of 0.4 up and 0.4 out ({@code hurtMarked}):
     * at the next tick's start the puppet's motion is zero (the step took it at the tick's end) and the control still carries the
     * push.</li>
     * <li>Through all of it the puppet's steps are within the rule (across at most 0.1, up at most 0.1, down at most 0.05), its gravity
     * stays off, and its foot is on the rule's target at the end.</li>
     * <li>A {@code EntityTravelToDimensionEvent} posted for the puppet is cancelled and counted; one for the control is not.</li>
     * <li>A column of lit portal blocks stands across the route, over the puppet's height: it flies through it and is still the same body
     * in the same level (the vanilla portal path tries a trip at every tick it is inside; the count is logged).</li>
     * </ol>
     * Fails without the knockback hook (steps 1 and 2: the puppet is thrown), without the step's zeroing (step 3: the push carries it
     * off the line), and without the portal hook (step 5 uncancelled, step 6 a trip to the Nether).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_pushes", skyAccess = true)
    public static void pushesAndPortalsLeaveItAloft(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 6000);
        String test = "pushesAndPortalsLeaveItAloft";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Lane lane = new Lane(far, 0, 6);
        long agent = ConflictKit.newAgent();
        long faction = ConflictKit.newFaction();
        int start = 24;
        EntityUmvuthi[] puppet = new EntityUmvuthi[1];
        EntityUmvuthi[] control = new EntityUmvuthi[1];
        Trace[] trace = new Trace[1];
        // what to do at the next tick's edge: 1 and 2 a blast (Pre), 3 a push (Post), 4 read the push (Pre)
        int[] fire = {0};
        Vec3[] pBefore = new Vec3[3], pAfter = new Vec3[3], cBefore = new Vec3[3], cAfter = new Vec3[3];
        long[] zeroed = new long[3];
        Vec3[] pushed = new Vec3[2];
        Runnable[] off = new Runnable[2];
        long[] portals = new long[3];
        boolean[] cancelled = new boolean[2];

        MowzieKit.seq(helper)
                .thenExecute(lane::force)
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    lane.floor();
                    bandAt(helper, faction, agent, lane.x0 + start, lane.top + 1, lane.z, 0f, List.of(), 1, true);
                    // the control: an Umvuthi no band owns, gravity off (restored and discarded before the sequence ends)
                    control[0] = MowzieKit.umvuthi(level, lane.at(2));
                    control[0].setNoGravity(true);
                    ConflictKit.onEnd(helper, () -> control[0].discard());
                    // a column of portal blocks across the route, 8 high and 7 wide, over the puppet's height
                    for (int dz = -3; dz <= 3; dz++)
                        for (int dy = 1; dy <= 8; dy++)
                            lane.set(start + 46, dy, dz, Blocks.NETHER_PORTAL.defaultBlockState().setValue(NetherPortalBlock.AXIS, Direction.Axis.Z));
                })
                .thenWaitUntil(waits.logic(() -> puppet[0] = puppet(agent)))
                .thenExecute(() -> {
                    check("it floats", puppet[0].isNoGravity());
                    trace[0] = trace(helper, level, agent);
                    off[0] = listen(helper, EventPriority.NORMAL, LevelTickEvent.Pre.class, e -> {
                        if (e.getLevel() != level || fire[0] == 0)
                            return;
                        int k = fire[0];
                        if (k == 1 || k == 2) {
                            fire[0] = 0;
                            ExplosionDamageCalculator calc = new SimpleExplosionDamageCalculator(false, false,
                                    k == 2 ? Optional.of(1.22f) : Optional.empty(), Optional.empty());
                            pBefore[k] = puppet[0].getDeltaMovement();
                            zeroed[k] = ConflictKit.knockbacksZeroed();
                            Vec3 at = puppet[0].position().add(0.0, -2.0, 0.0);
                            level.explode(null, null, calc, at.x, at.y, at.z, 3.0f, false, Level.ExplosionInteraction.NONE);
                            zeroed[k] = ConflictKit.knockbacksZeroed() - zeroed[k];
                            pAfter[k] = puppet[0].getDeltaMovement();
                            cBefore[k] = control[0].getDeltaMovement();
                            Vec3 under = control[0].position().add(0.0, -2.0, 0.0);
                            level.explode(null, null, calc, under.x, under.y, under.z, 3.0f, false, Level.ExplosionInteraction.NONE);
                            cAfter[k] = control[0].getDeltaMovement();
                        } else if (k == 4) {
                            fire[0] = 0;
                            pushed[0] = puppet[0].getDeltaMovement();
                            pushed[1] = control[0].getDeltaMovement();
                        }
                    });
                    off[1] = listen(helper, EventPriority.NORMAL, LevelTickEvent.Post.class, e -> {
                        if (e.getLevel() != level || fire[0] != 3)
                            return;
                        fire[0] = 4;
                        for (EntityUmvuthi u : new EntityUmvuthi[] {puppet[0], control[0]}) {
                            u.setDeltaMovement(0.4, 0.4, 0.0);
                            u.hurtMarked = true;
                        }
                    });
                    ConflictKit.steer(agent, ConflictKit.party(agent).epoch, PACE, lane.x0 + start + 70, lane.z, 0, false);
                })
                .thenWaitUntil(waits.logic(() -> check("it has made 8 blocks", puppet[0].getX() >= lane.x0 + start + 8)))
                .thenExecute(() -> fire[0] = 1)
                .thenWaitUntil(waits.logic(() -> check("blast 1 is done", fire[0] == 0)))
                .thenExecute(() -> {
                    check("blast 1 did not move the puppet (" + pBefore[1] + " then " + pAfter[1] + ")", pAfter[1].distanceTo(pBefore[1]) < 1.0e-6);
                    check("blast 1 moved the control (" + cBefore[1] + " then " + cAfter[1] + ")", cAfter[1].distanceTo(cBefore[1]) > 0.15);
                    check("blast 1 was counted once (" + zeroed[1] + ")", zeroed[1] == 1);
                })
                .thenIdle(40)
                .thenExecute(() -> fire[0] = 2)
                .thenWaitUntil(waits.logic(() -> check("blast 2 is done", fire[0] == 0)))
                .thenExecute(() -> {
                    check("blast 2 did not move the puppet (" + pBefore[2] + " then " + pAfter[2] + ")", pAfter[2].distanceTo(pBefore[2]) < 1.0e-6);
                    check("blast 2 moved the control (" + cBefore[2] + " then " + cAfter[2] + ")", cAfter[2].distanceTo(cBefore[2]) > 0.15);
                    check("blast 2 was counted once (" + zeroed[2] + ")", zeroed[2] == 1);
                })
                .thenIdle(40)
                .thenExecute(() -> fire[0] = 3)
                .thenWaitUntil(waits.logic(() -> check("the push is read", fire[0] == 0)))
                .thenExecute(() -> {
                    check("the puppet carries no push into the next tick (" + pushed[0] + ")", pushed[0].length() < 1.0e-6);
                    check("the control carries it (" + pushed[1] + ")", pushed[1].y > 0.3 && pushed[1].x > 0.3);
                    // the posted trips: the puppet's is cancelled and counted, the control's is not
                    portals[0] = ConflictKit.portalsCancelled();
                    EntityTravelToDimensionEvent mine = new EntityTravelToDimensionEvent(puppet[0], Level.NETHER);
                    NeoForge.EVENT_BUS.post(mine);
                    cancelled[0] = mine.isCanceled();
                    portals[1] = ConflictKit.portalsCancelled();
                    EntityTravelToDimensionEvent theirs = new EntityTravelToDimensionEvent(control[0], Level.NETHER);
                    NeoForge.EVENT_BUS.post(theirs);
                    cancelled[1] = theirs.isCanceled();
                    check("a trip posted for the puppet is cancelled", cancelled[0]);
                    check("and counted once (" + portals[0] + " to " + portals[1] + ")", portals[1] == portals[0] + 1);
                    check("a trip posted for the control is not cancelled", !cancelled[1]);
                    check("and not counted (" + ConflictKit.portalsCancelled() + ")", ConflictKit.portalsCancelled() == portals[1]);
                    off[0].run();
                    off[1].run();
                    control[0].setNoGravity(false);
                    control[0].discard();
                })
                .thenWaitUntil(waits.logic(() -> check("it is past the portals", puppet[0].getX() >= lane.x0 + start + 50)))
                .thenExecute(() -> {
                    portals[2] = ConflictKit.portalsCancelled();
                    check("it is the same body in the same level (" + puppet[0].level().dimension().location() + ")",
                            puppet[0].level() == level && !puppet[0].isRemoved() && level.getEntity(puppet[0].getUUID()) == puppet[0]);
                    check("it floats", puppet[0].isNoGravity());
                    check("it is still the chief's puppet", chiefSlot(agent) != null && chiefSlot(agent).entity() == puppet[0]);
                    Trace t = trace[0];
                    check("no step across was over 0.1 (" + t.across + ")", t.across <= 0.1 + 1.0e-6);
                    check("none up over 0.1 (" + t.up + ")", t.up <= 0.1 + 1.0e-6);
                    check("none down over 0.05 (" + t.down + ")", t.down <= 0.05 + 1.0e-6);
                    check("its gravity was never on (" + t.gravity + ")", t.gravity == 0);
                    check("its box was never in a block (" + t.crowded + ")", t.crowded == 0);
                    double target = UmvuthiAloft.INSTANCE.target(chiefSlot(agent).travelMemo());
                    check("its foot is on the rule's target (" + puppet[0].getY() + " and " + target + ")", Math.abs(puppet[0].getY() - target) <= 0.05);
                    check("no traveller step failed (" + ConflictKit.travellerErrors() + ")", ConflictKit.travellerErrors() == 0);
                    record(test, "pushes", "blasts zeroed " + (zeroed[1] + zeroed[2]) + ", control pushed " + String.format("%.2f", cAfter[1].distanceTo(cBefore[1]))
                            + " and " + String.format("%.2f", cAfter[2].distanceTo(cBefore[2])) + ", fan push left " + pushed[0].length() + ", portal tries " + (portals[2] - portals[1]));
                    recallAll(agent);
                })
                .thenIdle(30)
                .thenExecute(far::releaseAll)
                .thenSucceed();
    }

    // ---- 23d: stuck, raised, held and moved (MZB-811) ------------------------------------------------

    /**
     * A puppet that cannot rise is raised, held and moved to its place (test 23d; MZB-811). Three lanes, one band each, all steered
     * 60 blocks on: A's, whose masked player stands 30 blocks off with a clear line; B's, with no player within 160; and the control's,
     * clear. Once each puppet floats, a stone ceiling is laid over A's and B's, 3 blocks over the foot (30 long, a lane wide): the
     * rule sees it and aims over it, the puppet cannot rise through it and takes no step across while its target is high, so
     * neither makes headway.
     * <ol>
     * <li>Each stuck puppet's raise (read from its memo) reads 8, then 16, then 24, 100 ticks apart.</li>
     * <li>B is moved to its place 100 ticks after the 24 (the first look with no player's line): its raise is 0 again, it stands at
     * its place, which is over a 9 by 9 farmland field. {@code stuckMoves} counts one.</li>
     * <li>A is held while the player has a line to it, and is moved at the first look past 1,200 ticks held. {@code stuckMoves}
     * counts two, the control's raise never read above 0 and it was never moved.</li>
     * <li>B, moved and given the attack flag, sinks onto the farmland. Every field block is farmland still (a fall of the 2 blocks
     * it sank would turn each to dirt, if the fall were counted), and a watching player 20 blocks off is sent no block particle.</li>
     * </ol>
     * Fails without the stuck rule (both stay under the ceiling), without the clip (A is moved in the player's view at once), and
     * without {@code resetFallDistance} in the sink (step 4: the farmland turns to dirt).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_stuck", skyAccess = true)
    public static void aStuckUmvuthiRisesThenIsMoved(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 9000);
        String test = "aStuckUmvuthiRisesThenIsMoved";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Lane[] lane = {new Lane(far, 0, 6), new Lane(far, 1, 6), new Lane(far, 2, 6)}; // A: seen; B: unseen; the control's
        long[] agent = {ConflictKit.newAgent(), ConflictKit.newAgent(), ConflictKit.newAgent()};
        long faction = ConflictKit.newFaction();
        int start = 24, end = 84;
        EntityUmvuthi[] puppet = new EntityUmvuthi[3];
        ServerPlayer[] player = new ServerPlayer[1];
        FakePlayers.Hearer[] hearer = new FakePlayers.Hearer[1];
        Trace[] trace = new Trace[3];
        long[][] raisedAt = new long[3][3];
        long[] movedAt = new long[3];
        int[] last = new int[3], most = new int[3];
        double[] ceilingRoom = {0};
        double[] place = new double[2];
        long[] moves = {0};
        long[] buried = {0};
        int ceiling = 6;

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    for (Lane l : lane)
                        l.force();
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    for (Lane l : lane)
                        l.floor();
                    for (int i = 0; i < 3; i++)
                        bandAt(helper, faction, agent[i], lane[i].x0 + start, lane[i].top + 1, lane[i].z, 0f, List.of(), 1, true);
                    player[0] = MowzieKit.player(helper, lane[0].at(start + 30), "mowzie-aloft-stuck-a");
                    MowzieKit.mask(player[0]);
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < 3; i++)
                        puppet[i] = puppet(agent[i]);
                }))
                .thenIdle(20)
                .thenExecute(() -> {
                    moves[0] = UmvuthiAloft.INSTANCE.stuckMoves;
                    for (int i = 0; i < 3; i += 2)
                        trace[i] = trace(helper, level, agent[i]);
                    for (int i = 0; i < 2; i++)
                        for (int dx = start - 12; dx <= start + 20; dx++)
                            for (int dz = -7; dz <= 7; dz++)
                                lane[i].set(dx, ceiling, dz, Blocks.STONE.defaultBlockState());
                    ceilingRoom[0] = lane[0].top + ceiling - puppet[0].getBbHeight();
                    MowzieKit.everyTick(helper, () -> {
                        long now = level.getGameTime();
                        for (int i = 0; i < 3; i++) {
                            ConflictBridge.Slot sl = chiefSlot(agent[i]);
                            if (sl == null || sl.travelMemo() == null)
                                continue;
                            int r = UmvuthiAloft.INSTANCE.raised(sl.travelMemo());
                            most[i] = Math.max(most[i], r);
                            for (int k = 0; k < 3; k++)
                                if (r == 8 * (k + 1) && raisedAt[i][k] == 0)
                                    raisedAt[i][k] = now;
                            if (r == 0 && last[i] > 0 && movedAt[i] == 0)
                                movedAt[i] = now;
                            last[i] = r;
                            if (i < 2 && movedAt[i] == 0 && puppet[i].getY() > ceilingRoom[0] + 1.0e-6)
                                buried[0]++;
                        }
                    });
                    for (int i = 0; i < 3; i++)
                        ConflictKit.steer(agent[i], ConflictKit.party(agent[i]).epoch, 0.5f, lane[i].x0 + end, lane[i].z, 0, false);
                })
                .thenIdle(160)
                .thenExecute(() -> {
                    // its place once the anchor is there: the field of farmland under it, a pool in one corner to keep it wet
                    ConflictBridge.Stack st = chiefSlot(agent[1]).stack;
                    place[0] = st.target()[0] - st.back();
                    place[1] = st.target()[1] + st.side();
                    int px = (int) Math.floor(place[0]), pz = (int) Math.floor(place[1]);
                    for (int dx = -4; dx <= 4; dx++)
                        for (int dz = -4; dz <= 4; dz++)
                            level.setBlock(new BlockPos(px + dx, lane[1].top, pz + dz),
                                    Blocks.FARMLAND.defaultBlockState().setValue(FarmBlock.MOISTURE, 7), 2);
                    level.setBlock(new BlockPos(px + 4, lane[1].top, pz + 4), Blocks.WATER.defaultBlockState(), 2);
                    check("neither stuck puppet has moved yet (" + movedAt[0] + " and " + movedAt[1] + ")", movedAt[0] == 0 && movedAt[1] == 0);
                    check("each is under the ceiling (" + puppet[0].getY() + ", " + puppet[1].getY() + " under " + ceilingRoom[0] + ")",
                            puppet[0].getY() <= ceilingRoom[0] + 1.0e-6 && puppet[1].getY() <= ceilingRoom[0] + 1.0e-6);
                    hearer[0] = FakePlayers.watching(level, new Vec3(place[0] - 20, lane[1].top + 1, place[1]), "mowzie-aloft-stuck-b-hearer");
                })
                .thenWaitUntil(waits.logic(() -> check("B has been moved", movedAt[1] != 0)))
                .thenExecute(() -> {
                    long a = raisedAt[1][0], b = raisedAt[1][1], c = raisedAt[1][2];
                    check("B's raise read 8, 16 and 24 (" + a + ", " + b + ", " + c + ")", a > 0 && b > 0 && c > 0);
                    check("100 ticks apart (" + (b - a) + " and " + (c - b) + ")", b - a == 100 && c - b == 100);
                    long wait = movedAt[1] - c;
                    check("B was moved at the first look after the next 100 ticks (" + wait + ")", wait >= 100 && wait <= 112);
                    check("one stuck move is counted (" + (UmvuthiAloft.INSTANCE.stuckMoves - moves[0]) + ")", UmvuthiAloft.INSTANCE.stuckMoves - moves[0] == 1);
                    double off = Math.hypot(puppet[1].getX() - place[0], puppet[1].getZ() - place[1]);
                    check("B stands at its place (" + off + " off)", off <= 1.5);
                    check("A is still held under the ceiling (" + most[0] + ", " + movedAt[0] + ")", most[0] == 24 && movedAt[0] == 0);
                    ConflictKit.steer(agent[1], ConflictKit.party(agent[1]).epoch, 0.5f, lane[1].x0 + end, lane[1].z, 0, true);
                })
                .thenWaitUntil(waits.logic(() -> check("B has landed", !puppet[1].isNoGravity())))
                .thenExecute(() -> {
                    int px = (int) Math.floor(place[0]), pz = (int) Math.floor(place[1]);
                    int farmland = 0;
                    for (int dx = -4; dx <= 4; dx++)
                        for (int dz = -4; dz <= 4; dz++)
                            if (level.getBlockState(new BlockPos(px + dx, lane[1].top, pz + dz)).is(Blocks.FARMLAND))
                                farmland++;
                    check("the field is farmland still, 80 blocks of it (" + farmland + ")", farmland == 80);
                    long particles = hearer[0].packets.stream()
                            .filter(q -> q instanceof ClientboundLevelParticlesPacket p && p.getParticle().getType() == ParticleTypes.BLOCK).count();
                    check("the watching player was sent no block particle (" + particles + ")", particles == 0);
                })
                .thenWaitUntil(waits.logic(() -> check("A has been moved", movedAt[0] != 0)))
                .thenExecute(() -> {
                    long c = raisedAt[0][2];
                    long held = movedAt[0] - c - 100;
                    check("A was held 1,200 ticks and moved at the first look after (" + held + ")", held >= 1200 && held <= 1212);
                    check("A's raise read 8, 16 and 24 (" + raisedAt[0][0] + ", " + raisedAt[0][1] + ", " + c + ")",
                            raisedAt[0][0] > 0 && raisedAt[0][1] - raisedAt[0][0] == 100 && c - raisedAt[0][1] == 100);
                    check("two stuck moves are counted (" + (UmvuthiAloft.INSTANCE.stuckMoves - moves[0]) + ")", UmvuthiAloft.INSTANCE.stuckMoves - moves[0] == 2);
                    check("neither stuck puppet was ever above the room under the ceiling (" + buried[0] + ")", buried[0] == 0);
                    check("the control's raise was never above 0 (" + most[2] + ") and it was never moved (" + movedAt[2] + ")", most[2] == 0 && movedAt[2] == 0);
                    check("its box was never in a block (" + trace[0].crowded + " and " + trace[2].crowded + ")", trace[0].crowded == 0 && trace[2].crowded == 0);
                    check("no traveller step failed (" + ConflictKit.travellerErrors() + ")", ConflictKit.travellerErrors() == 0);
                    record(test, "stuck", "raised at " + raisedAt[1][0] + "/" + raisedAt[1][1] + "/" + raisedAt[1][2] + " (B), moved B at +" + (movedAt[1] - raisedAt[1][2])
                            + ", A held " + held + ", farmland kept");
                    recallAll(agent);
                })
                .thenIdle(30)
                .thenExecute(() -> {
                    FakePlayers.removeSeen(hearer[0]);
                    drop(player);
                    far.releaseAll();
                })
                .thenSucceed();
    }

    // ---- 23e: killed aloft (MZB-817) -------------------------------------------------------------------

    /**
     * An Umvuthi killed aloft falls, journals once and drops once (test 23e; MZB-817). A puppet floats 12 blocks over the floor with
     * its band, and a masked player stands 20 blocks off; beside it, on another lane, an untagged Umvuthi stands on the floor with its own
     * player (the control). Both are killed by their player in one tick.
     * <ol>
     * <li>The jar posts two {@code LivingDeathEvent}s for each body (counted, the same for both).</li>
     * <li>From the puppet's death its gravity is on and it falls to the floor.</li>
     * <li>One CHIEF_KILLED names the player (by a player), and {@code chiefsKilled} rises by one; one body is dying until the animation ends.</li>
     * <li>At the animation's end one Sol Visage lies where it fell, the same number of ticks after the death as the control's;
     * {@code deathLootLost} did not rise; nothing is dying.</li>
     * <li>No Umvuthi with its gravity off is left in the lane, and its chunks, let go, saved and forced again, hold no Umvuthi.</li>
     * </ol>
     * Fails without {@code died} (the body hangs aloft through its death), and without the Dying list (the drop carries no record).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_killed", skyAccess = true)
    public static void anUmvuthiKilledAloftFallsAndDropsOnce(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 4000);
        String test = "anUmvuthiKilledAloftFallsAndDropsOnce";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Lane lane = new Lane(far, 0, 2), plain = new Lane(far, 1, 2);
        long agent = ConflictKit.newAgent();
        long faction = ConflictKit.newFaction();
        int start = 8;
        EntityUmvuthi[] puppet = new EntityUmvuthi[1], control = new EntityUmvuthi[1];
        ServerPlayer[] player = new ServerPlayer[2];
        int[] deaths = new int[2];
        long[] diedAt = new long[2], dropAt = new long[2];
        Vec3[] fell = new Vec3[2];
        double[] height = new double[2];
        long[] before = new long[3];
        Runnable[] off = new Runnable[1];

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    lane.force();
                    plain.force();
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    lane.floor();
                    plain.floor();
                    bandAt(helper, faction, agent, lane.x0 + start, lane.top + 1, lane.z, 0f, List.of(), 1, true);
                    player[0] = MowzieKit.player(helper, lane.at(start + 20), "mowzie-aloft-killer");
                    MowzieKit.mask(player[0]);
                    control[0] = MowzieKit.umvuthi(level, plain.at(start));
                    player[1] = MowzieKit.player(helper, plain.at(start + 20), "mowzie-aloft-killer-control");
                    MowzieKit.mask(player[1]);
                })
                .thenWaitUntil(waits.logic(() -> puppet[0] = puppet(agent)))
                .thenExecute(() -> {
                    EntityUmvuthi u = puppet[0];
                    check("it floats", u.isNoGravity());
                    // 12 blocks over the floor: the step brings it down at 0.05 a tick, so it is killed the tick after
                    u.setPos(u.getX(), lane.top + 13.0, u.getZ());
                    off[0] = listen(helper, EventPriority.NORMAL, LivingDeathEvent.class, e -> {
                        if (e.getEntity() == puppet[0])
                            deaths[0]++;
                        else if (e.getEntity() == control[0])
                            deaths[1]++;
                    });
                })
                .thenIdle(1)
                .thenExecute(() -> {
                    EntityUmvuthi u = puppet[0];
                    height[0] = u.getY() - lane.top;
                    check("it is about 12 blocks over the floor (" + height[0] + ")", height[0] > 11.5 && height[0] < 13.1);
                    before[0] = ConflictKit.bridge().chiefsKilled;
                    before[1] = ConflictKit.deathLootLost();
                    fell[0] = u.position();
                    fell[1] = control[0].position();
                    long now = level.getGameTime();
                    diedAt[0] = diedAt[1] = now;
                    u.setLastHurtByPlayer(player[0]);
                    control[0].setLastHurtByPlayer(player[1]);
                    MowzieKit.kill(u, level.damageSources().playerAttack(player[0]));
                    MowzieKit.kill(control[0], level.damageSources().playerAttack(player[1]));
                    check("its gravity is on from its death (" + u.isNoGravity() + ")", !u.isNoGravity());
                    MowzieKit.everyTick(helper, () -> {
                        long t = level.getGameTime();
                        for (int i = 0; i < 2; i++)
                            if (dropAt[i] == 0 && visagesNear(level, fell[i], 6) > 0)
                                dropAt[i] = t;
                    });
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    EntityUmvuthi u = puppet[0];
                    check("it has fallen (" + (u.getY() - lane.top) + " over the floor)", u.getY() - lane.top < 2.0 || u.isRemoved());
                    check("on to the floor", u.onGround() || u.isRemoved());
                    List<ScriptedEngine.Received> killed = ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent);
                    check("one CHIEF_KILLED is journaled (" + killed.size() + ")", killed.size() == 1);
                    check("by a player", killed.get(0).aux() == ConflictRecords.BY_PLAYER);
                    check("naming the player", player[0].getUUID().equals(ConflictKit.uuidAt(killed.get(0), ConflictRecords.ChiefKilled.KILLER_MOST)));
                    check("chiefsKilled rose by one (" + before[0] + " to " + ConflictKit.bridge().chiefsKilled + ")",
                            ConflictKit.bridge().chiefsKilled == before[0] + 1);
                    check("its body is waiting for its animation's end (" + ConflictKit.bridge().dyingBodies() + ")", ConflictKit.bridge().dyingBodies() == 1);
                    check("no Sol Visage yet (" + visagesNear(level, fell[0], 6) + " and " + visagesNear(level, fell[1], 6) + ")",
                            visagesNear(level, fell[0], 6) == 0 && visagesNear(level, fell[1], 6) == 0);
                })
                .thenWaitUntil(waits.logic(() -> check("both drops are on the floor (" + dropAt[0] + " and " + dropAt[1] + ")", dropAt[0] != 0 && dropAt[1] != 0)))
                .thenExecute(() -> {
                    off[0].run();
                    check("two LivingDeathEvents for the puppet (" + deaths[0] + ")", deaths[0] == 2);
                    check("and for the control (" + deaths[1] + ")", deaths[1] == deaths[0]);
                    long mine = dropAt[0] - diedAt[0], theirs = dropAt[1] - diedAt[1];
                    check("the puppet's drop came as many ticks after its death as the control's (" + mine + " and " + theirs + ")", Math.abs(mine - theirs) <= 1);
                    check("one Sol Visage where the puppet fell (" + visagesNear(level, fell[0], 6) + ")", visagesNear(level, fell[0], 6) == 1);
                    check("one where the control fell (" + visagesNear(level, fell[1], 6) + ")", visagesNear(level, fell[1], 6) == 1);
                    check("no loot record was lost (" + before[1] + " to " + ConflictKit.deathLootLost() + ")", ConflictKit.deathLootLost() == before[1]);
                    check("nothing is dying (" + ConflictKit.bridge().dyingBodies() + ")", ConflictKit.bridge().dyingBodies() == 0);
                    check("no second CHIEF_KILLED (" + ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent).size() + ")",
                            ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent).size() == 1);
                    int floating = 0;
                    for (EntityUmvuthi u : level.getEntitiesOfClass(EntityUmvuthi.class, lane.area().minmax(plain.area())))
                        if (u.isNoGravity())
                            floating++;
                    check("no Umvuthi floats in the level (" + floating + ")", floating == 0);
                    record(test, "killed aloft", "fell from " + String.format("%.1f", height[0]) + ", dropped " + (dropAt[0] - diedAt[0]) + " ticks after its death (the control's "
                            + (dropAt[1] - diedAt[1]) + "), " + deaths[0] + " death events");
                    recallAll(agent);
                    far.releaseRect(lane.cx0, lane.cz, lane.cx1, lane.cz);
                })
                .thenWaitUntil(waits.chunks(() -> {
                    far.unloaded(lane.cx0, lane.cz).run();
                    far.unloaded(lane.cx1, lane.cz).run();
                }))
                .thenExecute(() -> lane.force())
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    check("the chunks read back hold no Umvuthi (" + level.getEntitiesOfClass(EntityUmvuthi.class, lane.area()).size() + ")",
                            level.getEntitiesOfClass(EntityUmvuthi.class, lane.area()).isEmpty());
                    drop(player);
                    far.releaseAll();
                })
                .thenSucceed();
    }

    // ---- 35: over water (MZB-812) ---------------------------------------------------------------------

    /**
     * An aloft puppet crosses water and does not come down in it (test 35; MZB-812). Three lanes, each with a flagged or targeted
     * puppet.
     * <ol>
     * <li>The river (lane A): 20 blocks wide, 3 deep, walled at its sides. The puppet carries the attack flag, with its steer's end 14
     * blocks past the far bank: it settles once within 32 of the end, which is over the river, and it floats on over the water (its gravity
     * off the whole way, its foot the floor's + 3, never in the water, never a block of liquid in its box) and sinks only on the far bank.</li>
     * <li>The lake (lane B): 56 wide, with a 5 by 5 raft of planks 40 blocks out. A player stands on the raft once the puppet is over the lake:
     * it is the puppet's target in plain view, so it settles, but over water it hovers within 8 blocks of its steer's end, not down in
     * it: still aloft 300 ticks later, restricted where it floats, never wet.</li>
     * <li>Killed there (by that player) its body falls into the lake, one CHIEF_KILLED is journaled and one Sol Visage drops.</li>
     * <li>The control (lane C, no water): the same flag settles it on the floor within 8 blocks of where it first came within 32.</li>
     * </ol>
     * Fails without the fluid rule in the landing (the puppet sinks into the river as soon as the flag settles it).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_water", skyAccess = true)
    public static void anAloftUmvuthiCrossesARiver(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 9000);
        String test = "anAloftUmvuthiCrossesARiver";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Lane[] lane = {new Lane(far, 0, 6), new Lane(far, 1, 6), new Lane(far, 2, 6)}; // the river; the lake; the control
        long[] agent = {ConflictKit.newAgent(), ConflictKit.newAgent(), ConflictKit.newAgent()};
        long faction = ConflictKit.newFaction();
        int start = 8, end = 74, riverFrom = 40, riverTo = 59, lakeFrom = 24, lakeTo = 79, raftAt = 60;
        EntityUmvuthi[] puppet = new EntityUmvuthi[3];
        ServerPlayer[] player = new ServerPlayer[1];
        // over the river: ticks, ticks with gravity on, ticks wet, ticks with liquid in the box; over the lake: the same
        int[] over = new int[2], grav = new int[2], wet = new int[2], liquid = new int[2];
        double[] firstSettle = {Double.NaN, Double.NaN};
        double[] y0 = new double[1];
        double[] fell = new double[2];
        long[] before = new long[1];
        boolean[] landed = new boolean[3];
        double[] landedX = new double[3];

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    for (Lane l : lane)
                        l.force();
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    for (Lane l : lane)
                        l.floor();
                    BlockState water = Blocks.WATER.defaultBlockState(), stone = Blocks.STONE.defaultBlockState();
                    int[][] spans = {{riverFrom, riverTo}, {lakeFrom, lakeTo}};
                    for (int i = 0; i < 2; i++)
                        for (int dx = spans[i][0]; dx <= spans[i][1]; dx++)
                            for (int dz = -8; dz <= 7; dz++)
                                for (int dy = -2; dy <= 1; dy++) {
                                    boolean wall = dz == -8 || dz == 7;
                                    lane[i].set(dx, dy, dz, wall ? stone : dy <= 0 ? water : Blocks.AIR.defaultBlockState());
                                }
                    // the raft: planks on the lake's surface
                    for (int dx = raftAt; dx < raftAt + 5; dx++)
                        for (int dz = -2; dz <= 2; dz++)
                            lane[1].set(dx, 0, dz, Blocks.OAK_PLANKS.defaultBlockState());
                    for (int i = 0; i < 3; i++)
                        bandAt(helper, faction, agent[i], lane[i].x0 + start, lane[i].top + 1, lane[i].z, 0f, List.of(), 1, true);
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < 3; i++)
                        puppet[i] = puppet(agent[i]);
                }))
                .thenExecute(() -> {
                    MowzieKit.everyTick(helper, () -> {
                        double xa = puppet[0].getX() - lane[0].x0, xb = puppet[1].getX() - lane[1].x0;
                        if (xa >= riverFrom + 1 && xa <= riverTo && !puppet[0].isRemoved()) {
                            over[0]++;
                            if (!puppet[0].isNoGravity())
                                grav[0]++;
                            if (puppet[0].isInWater())
                                wet[0]++;
                            if (level.containsAnyLiquid(puppet[0].getBoundingBox()))
                                liquid[0]++;
                        }
                        if (xb >= lakeFrom + 1 && xb <= lakeTo && !puppet[1].isRemoved() && !puppet[1].isDeadOrDying()) {
                            over[1]++;
                            if (!puppet[1].isNoGravity())
                                grav[1]++;
                            if (puppet[1].isInWater())
                                wet[1]++;
                            if (level.containsAnyLiquid(puppet[1].getBoundingBox()))
                                liquid[1]++;
                        }
                        for (int i = 0; i < 3; i++)
                            if (!landed[i] && !puppet[i].isRemoved() && !puppet[i].isNoGravity()) {
                                landed[i] = true;
                                landedX[i] = puppet[i].getX() - lane[i].x0;
                            }
                    });
                    ConflictKit.steer(agent[0], ConflictKit.party(agent[0]).epoch, PACE, lane[0].x0 + end, lane[0].z, 0, true);
                    ConflictKit.steer(agent[2], ConflictKit.party(agent[2]).epoch, PACE, lane[2].x0 + end, lane[2].z, 0, true);
                    // the lake's puppet goes for the raft, with no flag
                    ConflictKit.steer(agent[1], ConflictKit.party(agent[1]).epoch, PACE, lane[1].x0 + raftAt + 2, lane[1].z, 0, false);
                })
                .thenWaitUntil(waits.logic(() -> check("the lake's puppet is over the water (" + (puppet[1].getX() - lane[1].x0) + ")",
                        puppet[1].getX() - lane[1].x0 >= lakeFrom + 8)))
                .thenExecute(() -> {
                    // a player on the raft, in plain view, within its sight
                    player[0] = MowzieKit.player(helper, new Vec3(lane[1].x0 + raftAt + 2.5, lane[1].top + 1, lane[1].z), "mowzie-aloft-raft");
                })
                .thenWaitUntil(waits.logic(() -> check("the river's puppet has crossed and landed (" + landed[0] + ")", landed[0])))
                .thenExecute(() -> {
                    check("it floated across: gravity was never on over the river (" + grav[0] + " of " + over[0] + " ticks)", over[0] > 100 && grav[0] == 0);
                    check("it was never wet (" + wet[0] + ") and its box never held liquid (" + liquid[0] + ")", wet[0] == 0 && liquid[0] == 0);
                    check("it came down on the far bank (x " + landedX[0] + ")", landedX[0] > riverTo + 0.5);
                    check("and not by the river's edge (" + landedX[0] + " of " + (riverTo + 1) + ")", landedX[0] <= end);
                    check("it stands on the floor (" + (puppet[0].getY() - lane[0].top) + " over it)", Math.abs(puppet[0].getY() - (lane[0].top + 1)) < 0.06);
                    // the control, with no river, was down within 32 of its end: its landing x is about 42 plus the 40 ticks of the sink
                    check("the control settled where the flag first took it (" + landedX[2] + ")", landedX[2] > 0 && landedX[2] < riverFrom + 12);
                    check("the river's puppet landed farther on than the control (" + landedX[0] + " and " + landedX[2] + ")", landedX[0] > landedX[2] + 8);
                })
                .thenWaitUntil(waits.logic(() -> {
                    ConflictBridge.Slot sl = chiefSlot(agent[1]);
                    check("the lake's puppet is hovering (" + UmvuthiAloft.INSTANCE.settled(sl.travelMemo()) + ")", UmvuthiAloft.INSTANCE.settled(sl.travelMemo()));
                }))
                .thenExecute(() -> {
                    EntityUmvuthi u = puppet[1];
                    double[] t = chiefSlot(agent[1]).stack.target();
                    double d = Math.hypot(t[0] - u.getX(), t[1] - u.getZ());
                    check("it hovers within 8 blocks of its end (" + d + ")", d <= 8.0 + 0.2);
                    check("it is aloft (gravity off)", u.isNoGravity());
                    check("its target is the player (" + u.getTarget() + ")", u.getTarget() == player[0]);
                    check("it is restricted where it floats (" + u.getRestrictCenter() + ")", u.getRestrictCenter().equals(u.blockPosition()) && !u.hasRestriction());
                    y0[0] = u.getY();
                    check("its foot is the water's + 3 (" + (u.getY() - lane[1].top) + ")", Math.abs(u.getY() - (lane[1].top + 3)) <= 0.06);
                })
                .thenIdle(300)
                .thenExecute(() -> {
                    EntityUmvuthi u = puppet[1];
                    check("300 ticks on it is still aloft and in the same place (" + (u.getY() - y0[0]) + ")", u.isNoGravity() && Math.abs(u.getY() - y0[0]) < 0.06);
                    check("never wet (" + wet[1] + ") and never with liquid in its box (" + liquid[1] + ")", wet[1] == 0 && liquid[1] == 0);
                    check("and gravity was on for none of its ticks over the lake (" + grav[1] + " of " + over[1] + ")", grav[1] == 0 && over[1] > 100);
                    before[0] = ConflictKit.bridge().chiefsKilled;
                    fell[0] = u.getX();
                    fell[1] = u.getZ();
                    u.setLastHurtByPlayer(player[0]);
                    MowzieKit.kill(u, level.damageSources().playerAttack(player[0]));
                    check("its gravity is on from its death", !u.isNoGravity());
                })
                .thenIdle(60)
                .thenExecute(() -> {
                    EntityUmvuthi u = puppet[1];
                    check("its body is down in the lake (in water " + u.isInWater() + ", " + (u.getY() - lane[1].top) + " over the floor)",
                            u.isInWater() || u.getY() - lane[1].top < 2.0);
                    List<ScriptedEngine.Received> killed = ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[1]);
                    check("one CHIEF_KILLED (" + killed.size() + ")", killed.size() == 1);
                    check("naming the player", player[0].getUUID().equals(ConflictKit.uuidAt(killed.get(0), ConflictRecords.ChiefKilled.KILLER_MOST)));
                    check("chiefsKilled rose by one (" + before[0] + " to " + ConflictKit.bridge().chiefsKilled + ")", ConflictKit.bridge().chiefsKilled == before[0] + 1);
                })
                .thenWaitUntil(waits.logic(() -> check("one Sol Visage in the lake (" + visagesNear(level, new Vec3(fell[0], lane[1].top, fell[1]), 8) + ")",
                        visagesNear(level, new Vec3(fell[0], lane[1].top, fell[1]), 8) == 1)))
                .thenExecute(() -> {
                    check("nothing is dying (" + ConflictKit.bridge().dyingBodies() + ")", ConflictKit.bridge().dyingBodies() == 0);
                    check("no traveller step failed (" + ConflictKit.travellerErrors() + ")", ConflictKit.travellerErrors() == 0);
                    record(test, "water", "crossed " + over[0] + " ticks of river aloft, landed at " + String.format("%.1f", landedX[0]) + " (control " + String.format("%.1f", landedX[2])
                            + "), hovered over the lake " + over[1] + " ticks, killed there and dropped once");
                    recallAll(agent);
                })
                .thenIdle(30)
                .thenExecute(() -> {
                    drop(player);
                    far.releaseAll();
                })
                .thenSucceed();
    }

    // ---- 24: the chief's summons are its puppets (MZB-816) ---------------------------------------------

    /** A warrior or crane at its join: the tag it carried then, and whether the chief's slot listed it as an extra then. */
    private record Join(UUID id, EntityUmvuthanaMinion body, AgentTag tag, boolean extra) {
        boolean crane() {
            return body instanceof EntityUmvuthanaCrane;
        }
    }

    /** The engine's march of a band that stands still: {@code units} (and the war chief when {@code chief}), on a route one block long. */
    private static void marchOf(long agent, long faction, String dim, double x, double z, boolean chief, Map<String, Integer> units) {
        long bx = (long) Math.floor(x), bz = (long) Math.floor(z);
        ConflictKit.march(new ConflictKit.MarchSpec(agent, 1, dim, faction, ConflictRecords.PURPOSE_INVASION, ConflictRecords.March.OUT,
                chief ? ConflictRecords.March.FLAG_CHIEF : 0, 0L, new LinkedHashMap<>(units),
                List.of(new long[] {bx, bz, 0L}, new long[] {bx + 1, bz, 10_000L})));
    }

    /** Members in the party's stacks (the chief's slot too, no extras). */
    private static int slotCount(long agent) {
        int n = 0;
        for (ConflictBridge.Stack st : ConflictKit.party(agent).stacks())
            n += st.slots().size();
        return n;
    }

    /**
     * Inside {@code thenWaitUntil}: every entity of the list is removed, saved and unloaded with its chunk when it was saved
     * ({@link MowzieKit#removed}). The list is read at the first poll, as the test's steps fill it after the sequence is built.
     */
    @SuppressWarnings("unchecked")
    private static Runnable removedAll(ServerLevel level, Supplier<List<? extends Entity>> list) {
        Runnable[] wait = new Runnable[1];
        return () -> {
            if (wait[0] == null) {
                List<? extends Entity> es = list.get();
                Supplier<? extends Entity>[] s = new Supplier[es.size()];
                for (int i = 0; i < s.length; i++) {
                    Entity e = es.get(i);
                    s[i] = () -> e;
                }
                wait[0] = MowzieKit.removed(level, s);
            }
            wait[0].run();
        };
    }

    /**
     * An Umvuthi's summons are puppets of its party when it is a chief puppet, and strays when it stands on its throne (test 24;
     * MZB-816). A puppet that has settled before a player (its target, set each tick: the jar's own AI is off, so the abilities are the test's)
     * starts the jar's SPAWN and then its sunblockers; a throne Umvuthi in a declared grove starts the same.
     * <ol>
     * <li>Each of the puppet's warriors and cranes carries the party's AgentTag at its join and stands among the chief slot's extras then,
     * is let in by {@code admit}, is a puppet and is never saved by vanilla; {@code summonsAdopted} rose by their number and
     * {@code summonsRefused} did not.</li>
     * <li>The engine's march of the band (two warriors, the chief) is unchanged, no member was cut by the trim, and the band has the
     * members it had.</li>
     * <li>A warrior killed by the player books nothing: no BAND_CASUALTIES, no CHIEF_KILLED, the same members.</li>
     * <li>The lane's chunks are let go and forced again: none of the puppet's summons is in them (not one was saved).</li>
     * </ol>
     * The control, the throne Umvuthi's summons: untagged, in no extras, no puppets, saved by vanilla (made persistent so that only the
     * chunk decides: after the unload each is read back with the same id). Fails without the wrap: the puppet's summons are untagged
     * strays, saved and read back.
     */
    @SuppressWarnings("unchecked")
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_summons", skyAccess = true)
    public static void theChiefsSummonsArePuppets(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 8000);
        String test = "theChiefsSummonsArePuppets";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        String dim = RtsimTests.dimension(helper);
        Lane lane = new Lane(far, 0, 4);
        Grove g = groveAt(far, 1, 0);
        long agent = ConflictKit.newAgent();
        long faction = ConflictKit.newFaction();
        int start = 20;
        EntityUmvuthi[] puppet = new EntityUmvuthi[1], throne = new EntityUmvuthi[1];
        ServerPlayer[] player = new ServerPlayer[2];
        List<Join> joins = new ArrayList<>();
        Runnable[] off = new Runnable[1];
        long[] was = new long[5]; // summonsAdopted, summonsRefused, trimmed, casualties, chiefsKilled
        int[] slots = new int[1];
        List<Join>[] mine = new List[1], theirs = new List[1];
        Map<String, Integer>[] units = new Map[1];
        EntityUmvuthanaMinion[] killed = new EntityUmvuthanaMinion[1];
        int[] epoch = new int[1];
        Runnable gone = removedAll(level, () -> {
            List<Entity> all = new ArrayList<>();
            for (Join j : mine[0])
                all.add(j.body());
            for (Join j : theirs[0])
                all.add(j.body());
            return all;
        });

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    lane.force();
                    g.force();
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    lane.floor();
                    g.lay().declare();
                    throne[0] = MowzieKit.umvuthi(level, g.at(-10, 0));
                    bandAt(helper, faction, agent, lane.x0 + start, lane.top + 1, lane.z, 0f, List.of(), 2, true);
                    player[1] = MowzieKit.player(helper, off(g, 0, 20), "mowzie-summons-control");
                })
                .thenWaitUntil(waits.logic(() -> {
                    check("the grove's census has come", MowzieKit.censusCount(g.faction) >= 1);
                    puppet[0] = puppet(agent);
                    check("the puppet floats", puppet[0].isNoGravity());
                }))
                .thenExecute(() -> {
                    // the jar's own AI would summon when it likes: the test starts the abilities, and the target is set by hand
                    puppet[0].setNoAi(true);
                    player[0] = MowzieKit.player(helper, new Vec3(puppet[0].getX() + 10, lane.top + 1, puppet[0].getZ()), "mowzie-summons-target");
                    MowzieKit.everyTick(helper, () -> {
                        if (puppet[0].isAlive() && !puppet[0].isRemoved() && player[0].isAlive())
                            puppet[0].setTarget(player[0]);
                    });
                })
                .thenWaitUntil(waits.logic(() -> check("it has settled before the player (gravity " + !puppet[0].isNoGravity() + ")", !puppet[0].isNoGravity())))
                .thenExecute(() -> {
                    epoch[0] = ConflictKit.party(agent).epoch;
                    units[0] = new LinkedHashMap<>(Map.of(WARRIOR, 2));
                    marchOf(agent, faction, dim, lane.x0 + start, lane.z, true, units[0]);
                })
                .thenWaitUntil(waits.logic(() -> check("the engine's march is held", ConflictKit.bridge().march(agent) != null)))
                .thenExecute(() -> {
                    ConflictBridge b = ConflictKit.bridge();
                    was[0] = ConflictKit.summonsAdopted();
                    was[1] = ConflictKit.summonsRefused();
                    was[2] = ConflictKit.trimmed();
                    was[3] = ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent).size();
                    was[4] = b.chiefsKilled;
                    slots[0] = slotCount(agent);
                    off[0] = listen(helper, EventPriority.HIGHEST, EntityJoinLevelEvent.class, e -> {
                        if (!(e.getEntity() instanceof EntityUmvuthanaMinion m) || e.getLevel() != level)
                            return;
                        boolean extra = false;
                        ConflictBridge.Party p = ConflictKit.bridge().party(agent);
                        if (p != null)
                            for (ConflictBridge.Stack st : p.stacks())
                                for (ConflictBridge.Slot s : st.slots())
                                    if (s.chief && s.extras().contains(m))
                                        extra = true;
                        joins.add(new Join(m.getUUID(), m, RtsimAttachments.tag(m), extra));
                    });
                    puppet[0].sendAbilityMessage(EntityUmvuthi.SPAWN_ABILITY);
                    throne[0].sendAbilityMessage(EntityUmvuthi.SPAWN_ABILITY);
                })
                .thenWaitUntil(waits.logic(() -> {
                    check("the puppet's warriors have come (" + joins.stream().filter(j -> j.tag() != null).count() + ")",
                            joins.stream().filter(j -> j.tag() != null).count() >= 3);
                    check("and the throne body's (" + joins.stream().filter(j -> j.tag() == null).count() + ")",
                            joins.stream().filter(j -> j.tag() == null).count() >= 3);
                    check("both abilities are over", puppet[0].getActiveAbility() == null && throne[0].getActiveAbility() == null);
                }))
                .thenExecute(() -> {
                    puppet[0].sendAbilityMessage(EntityUmvuthi.SPAWN_SUNBLOCKERS_ABILITY);
                    throne[0].sendAbilityMessage(EntityUmvuthi.SPAWN_SUNBLOCKERS_ABILITY);
                })
                .thenWaitUntil(waits.logic(() -> {
                    check("the puppet's cranes have come (" + joins.stream().filter(j -> j.tag() != null && j.crane()).count() + ")",
                            joins.stream().filter(j -> j.tag() != null && j.crane()).count() >= 3);
                    check("and the throne body's (" + joins.stream().filter(j -> j.tag() == null && j.crane()).count() + ")",
                            joins.stream().filter(j -> j.tag() == null && j.crane()).count() >= 3);
                    check("both abilities are over", puppet[0].getActiveAbility() == null && throne[0].getActiveAbility() == null);
                }))
                .thenExecute(() -> {
                    off[0].run();
                    mine[0] = joins.stream().filter(j -> j.tag() != null).toList();
                    theirs[0] = joins.stream().filter(j -> j.tag() == null).toList();
                    int warriors = (int) mine[0].stream().filter(j -> !j.crane()).count(), cranes = mine[0].size() - warriors;
                    ConflictBridge bridge = ConflictKit.bridge();
                    for (Join j : mine[0]) {
                        String what = (j.crane() ? "crane " : "warrior ") + j.id();
                        check(what + " carried the party's tag (" + j.tag() + ")", j.tag().agent() == agent && j.tag().epoch() == epoch[0]);
                        check(what + " stood among the chief slot's extras at its join", j.extra());
                        check(what + " was let in (in the level " + (level.getEntity(j.id()) == j.body()) + ")",
                                !j.body().isRemoved() && level.getEntity(j.id()) == j.body());
                        check(what + " is admitted", bridge.admit(RtsimTests.session(), j.body(), j.tag(), level));
                        check(what + " is a puppet and is never saved", RtsimApi.isPuppet(j.body()) && !j.body().shouldBeSaved());
                        check(what + " is among the extras now", chiefSlot(agent).extras().contains(j.body()));
                    }
                    check("the adopted count rose by their number (" + (ConflictKit.summonsAdopted() - was[0]) + " for " + mine[0].size() + ")",
                            ConflictKit.summonsAdopted() - was[0] == mine[0].size());
                    check("none was refused (" + (ConflictKit.summonsRefused() - was[1]) + ")", ConflictKit.summonsRefused() == was[1]);
                    check("the march is the engine's still (" + bridge.march(agent).count(WARRIOR) + " warriors, the chief "
                            + bridge.march(agent).chief() + ")", bridge.march(agent).count(WARRIOR) == 2 && bridge.march(agent).chief());
                    check("the band has the members it had (" + slotCount(agent) + ")", slotCount(agent) == slots[0]);
                    check("the trim cut none (" + (ConflictKit.trimmed() - was[2]) + ")", ConflictKit.trimmed() == was[2]);
                    for (Join j : theirs[0]) {
                        String what = (j.crane() ? "the throne body's crane " : "the throne body's warrior ") + j.id();
                        check(what + " is untagged", RtsimAttachments.tag(j.body()) == null);
                        check(what + " is no extra of any slot", !j.extra());
                        check(what + " is no puppet and vanilla saves it", !RtsimApi.isPuppet(j.body()) && j.body().shouldBeSaved());
                        j.body().setPersistenceRequired();
                    }
                    check("and the adopted count did not move for them", ConflictKit.summonsAdopted() - was[0] == mine[0].size());
                    record(test, "summons", warriors + " warriors and " + cranes + " cranes of the puppet tagged and listed before their joins; "
                            + theirs[0].size() + " of the throne body's untagged strays");
                    // a warrior killed by the player books nothing
                    killed[0] = (EntityUmvuthanaMinion) mine[0].stream().filter(j -> !j.crane()).findFirst().orElseThrow().body();
                    killed[0].setLastHurtByPlayer(player[0]);
                    MowzieKit.kill(killed[0], level.damageSources().playerAttack(player[0]));
                })
                .thenWaitUntil(waits.logic(() -> check("the warrior's animation has ended", killed[0].isRemoved())))
                .thenIdle(20)
                .thenExecute(() -> {
                    check("no BAND_CASUALTIES for the party (" + ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent).size() + ")",
                            ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent).size() == was[3]);
                    check("no CHIEF_KILLED (" + ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent).size() + ")",
                            ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent).isEmpty() && ConflictKit.bridge().chiefsKilled == was[4]);
                    check("the band has the members it had (" + slotCount(agent) + ")", slotCount(agent) == slots[0]);
                    check("the puppet is still the chief's (alive " + puppet[0].isAlive() + ")", puppet(agent) == puppet[0]);
                    // the lane and the grove are let go, with every summons alive: a saved one would come back
                    far.releaseRect(lane.cx0, lane.cz, lane.cx1, lane.cz);
                    far.releaseRect(g.cx - 2, g.cz - 2, g.cx + 2, g.cz + 2);
                })
                .thenWaitUntil(waits.chunks(() -> {
                    gone.run();
                    far.unloaded(lane.cx0, lane.cz).run();
                    far.unloaded(g.cx, g.cz).run();
                }))
                .thenExecute(() -> {
                    lane.force();
                    g.force();
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    int back = 0;
                    for (Join j : mine[0])
                        if (level.getEntity(j.id()) != null)
                            back++;
                    check("none of the puppet's summons was saved and read back (" + back + " of " + mine[0].size() + ")", back == 0);
                    int kept = 0;
                    for (Join j : theirs[0])
                        if (level.getEntity(j.id()) != null)
                            kept++;
                    check("the throne body's were (" + kept + " of " + theirs[0].size() + ")", kept == theirs[0].size());
                    record(test, "saved", "0 of " + mine[0].size() + " of the puppet's summons came back from the chunk; " + kept + " of "
                            + theirs[0].size() + " of the throne body's did");
                    recallAll(agent);
                })
                .thenWaitUntil(waits.logic(() -> check("the band went (" + dematerialized(agent).size() + ")", dematerialized(agent).size() >= 1)))
                .thenExecute(() -> {
                    drop(player);
                    far.releaseAll();
                })
                .thenSucceed();
    }

    // ---- 25p: a payment to the puppet is remembered (MZB-819, MZB-820) -----------------------------------

    /** The CHIEF_PAID records journaled for a faction. */
    private static List<ScriptedEngine.Received> paidOf(long faction) {
        return RtsimTests.recordsWith(RtsimTests.engine(), ConflictRecords.CHIEF_PAID, ConflictRecords.ChiefPaid.FACTION, faction);
    }

    /**
     * A payment to a chief puppet is journaled and remembered (test 25's puppet clauses; MZB-819, MZB-820). Grove P's Invasion has its
     * puppet 60 blocks from the throne, with no throne body at the grove; a masked player pays it. Then the band comes home: the
     * engine's CHIEF_BODY for the chief at home names no player, and the chief's HAND_BACK follows.
     * <ul>
     * <li>One CHIEF_PAID is journaled with the faction, the chief's slot, generation 0, the flag PAID_PUPPET, the payer and the
     * desires' item; the bridge counted one payment to a puppet; the puppet {@code hasTradedWith} the player.</li>
     * <li>The hand-back answers DONE and one Umvuthi stands on the throne as the chief; it {@code hasTradedWith} the player, and the
     * only player it knows is the payer, though CHIEF_BODY named none.</li>
     * </ul>
     * The control, grove T: a payment to a throne Umvuthi adopted as the chief journals one CHIEF_PAID with its own slot and generation and
     * flags 0 (as M9's), and no payment to a puppet is counted for it. Fails without MZB-819 (no CHIEF_PAID for P) and without MZB-820
     * (the throne body knows no player).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_paid", skyAccess = true)
    public static void aPaymentToThePuppetIsRemembered(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 9000);
        String test = "aPaymentToThePuppetIsRemembered";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove[] g = {groveAt(far, 0, 0), groveAt(far, 1, 0)}; // P: the puppet; T: the throne body
        long agent = ConflictKit.newAgent();
        EntityUmvuthi[] throne = new EntityUmvuthi[1], puppet = new EntityUmvuthi[1];
        ServerPlayer[] payer = new ServerPlayer[2];
        long[] op = new long[2];
        long[] was = new long[1];
        int[] generation = new int[1];

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    for (Grove x : g)
                        x.force();
                    forceEast(g[0], 5); // the puppet is 60 blocks east of P's throne
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    for (Grove x : g)
                        x.lay().declare();
                    layEast(g[0], 5);
                    throne[0] = MowzieKit.umvuthi(level, g[1].at(-10, 0));
                })
                .thenWaitUntil(waits.logic(censused(g)))
                .thenExecute(() -> op[0] = adoptChief(g[1], throne[0], 1))
                .thenWaitUntil(waits.logic(() -> MowzieKit.answered(g[1].faction, op[0], 1)))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("ADOPT", ConflictKit.outcomes(g[1].faction, op[0]), DONE);
                    chiefBand(helper, g[0], agent, g[0].spot.x + 60, g[0].spot.z, 120f, List.of(), 1);
                })
                .thenWaitUntil(waits.logic(() -> {
                    puppet[0] = puppet(agent);
                    check("the puppet floats", puppet[0].isNoGravity());
                }))
                .thenExecute(() -> {
                    generation[0] = MowzieKit.tag(throne[0]).generation();
                    payer[0] = MowzieKit.player(helper, new Vec3(puppet[0].getX() + 6, g[0].spot.y + 1, puppet[0].getZ()), "mowzie-paid-puppet");
                    payer[1] = MowzieKit.player(helper, off(g[1], 0, 8), "mowzie-paid-throne");
                    MowzieKit.mask(payer[0]);
                    MowzieKit.mask(payer[1]);
                    was[0] = ConflictKit.puppetPaid();
                    check("no CHIEF_PAID yet", paidOf(g[0].faction).isEmpty() && paidOf(g[1].faction).isEmpty());
                    check("the jar remembers the payer of the puppet", MowzieKit.payUmvuthi(payer[0], puppet[0]));
                    check("the jar remembers the payer of the throne body", MowzieKit.payUmvuthi(payer[1], throne[0]));
                })
                .thenWaitUntil(waits.logic(() -> {
                    check("one CHIEF_PAID for the puppet's grove (" + paidOf(g[0].faction).size() + ")", paidOf(g[0].faction).size() == 1);
                    check("one for the control's (" + paidOf(g[1].faction).size() + ")", paidOf(g[1].faction).size() == 1);
                }))
                .thenExecute(() -> {
                    ScriptedEngine.Received r = paidOf(g[0].faction).get(0);
                    var v = r.view();
                    check("one item entry (aux " + r.aux() + ")", r.aux() == 1);
                    check("the chief's slot (" + v.getInt(ConflictRecords.ChiefPaid.SLOT) + ")", v.getInt(ConflictRecords.ChiefPaid.SLOT) == CHIEF_SLOT);
                    check("generation 0 (" + v.getInt(ConflictRecords.ChiefPaid.GENERATION) + ")", v.getInt(ConflictRecords.ChiefPaid.GENERATION) == 0);
                    check("PAID_PUPPET (flags " + v.getInt(ConflictRecords.ChiefPaid.FLAGS) + ")",
                            v.getInt(ConflictRecords.ChiefPaid.FLAGS) == ConflictRecords.ChiefPaid.PAID_PUPPET);
                    check("the payer is the masked player",
                            payer[0].getGameProfile().getId().equals(ConflictKit.uuidAt(r, ConflictRecords.ChiefPaid.PLAYER_MOST)));
                    check("with a Java op id", (v.getLong(ConflictRecords.ChiefPaid.OP_ID) & Obs.JAVA_OP_BIT) != 0);
                    check("the bridge counted one payment to a puppet (" + (ConflictKit.puppetPaid() - was[0]) + ")", ConflictKit.puppetPaid() == was[0] + 1);
                    check("the puppet remembers the payer", puppet[0].hasTradedWith(payer[0]));
                    ScriptedEngine.Received c = paidOf(g[1].faction).get(0);
                    var w = c.view();
                    check("the control's slot is the chief's (" + w.getInt(ConflictRecords.ChiefPaid.SLOT) + ")", w.getInt(ConflictRecords.ChiefPaid.SLOT) == CHIEF_SLOT);
                    check("and its generation " + generation[0] + " (" + w.getInt(ConflictRecords.ChiefPaid.GENERATION) + ")",
                            w.getInt(ConflictRecords.ChiefPaid.GENERATION) == generation[0]);
                    check("flags 0 (" + w.getInt(ConflictRecords.ChiefPaid.FLAGS) + ")", w.getInt(ConflictRecords.ChiefPaid.FLAGS) == 0);
                    check("the control's payer is its player",
                            payer[1].getGameProfile().getId().equals(ConflictKit.uuidAt(c, ConflictRecords.ChiefPaid.PLAYER_MOST)));
                    check("no payment to a puppet was counted for the control (" + (ConflictKit.puppetPaid() - was[0]) + ")", ConflictKit.puppetPaid() == was[0] + 1);
                    // the band comes home: the engine's memory of its chief at home names nobody, then the hand-back
                    payer[0].setPos(payer[0].getX(), payer[0].getY(), payer[0].getZ() + 300);
                    ConflictKit.chiefBody(g[0].faction, epochOf(g[0]), 0f, List.of());
                    op[1] = MowzieKit.reconcile(g[0].faction, epochOf(g[0]), g[0].spot, 3f, List.of(
                            CampLine.party(CHIEF_SLOT, 1, UMVUTHI, HAND_BACK, CHIEF, agent, ConflictKit.party(agent).epoch)));
                })
                .thenWaitUntil(waits.logic(() -> check("the hand-back is answered", ConflictKit.outcomes(g[0].faction, op[1]).size() == 1)))
                .thenExecute(() -> {
                    check("answered DONE, not " + MowzieKit.outcomeName(answerOf(g[0], op[1])), answerOf(g[0], op[1]) == DONE);
                    List<net.minecraft.world.entity.Entity> held = MowzieKit.held(g[0].faction, CHIEF_SLOT, 1);
                    check("one Umvuthi stands on the throne as the chief (" + held.size() + ")", held.size() == 1 && held.get(0) instanceof EntityUmvuthi);
                    EntityUmvuthi body = (EntityUmvuthi) held.get(0);
                    check("the puppet is gone", puppet[0].isRemoved());
                    check("the throne body remembers the payer", body.hasTradedWith(payer[0]));
                    check("and no one else (" + body.getTradedPlayers().size() + ")", body.getTradedPlayers().size() == 1);
                    record(test, "paid", "CHIEF_PAID with PAID_PUPPET once; the throne body after the hand-back knows " + body.getTradedPlayers().size()
                            + " player though CHIEF_BODY named none");
                    drop(payer);
                    far.releaseAll();
                })
                .thenSucceed();
    }

    // ---- 26: killed away (MZB-817, MZB-826, MZB-827) -------------------------------------------------

    private static final int REMOVE = ConflictRecords.CampReconcile.REMOVE;

    /** Sets the server's view distance for the test; the Runnable it returns puts it back (it also runs when the test ends). */
    private static Runnable viewDistance(GameTestHelper helper, ServerLevel level, int chunks) {
        var players = level.getServer().getPlayerList();
        int saved = players.getViewDistance();
        players.setViewDistance(chunks);
        boolean[] on = {true};
        Runnable back = () -> {
            if (on[0]) {
                on[0] = false;
                players.setViewDistance(saved);
            }
        };
        ConflictKit.onEnd(helper, back);
        return back;
    }

    /** The living, embodied warriors of a party's stacks (the chief's slot is no warrior). */
    private static List<Mob> warriorsOf(long agent) {
        List<Mob> out = new ArrayList<>();
        for (ConflictBridge.Stack st : ConflictKit.party(agent).stacks())
            for (ConflictBridge.Slot s : st.slots())
                if (!s.chief && s.alive() && s.entity() != null && !s.entity().isRemoved())
                    out.add(s.entity());
        return out;
    }

    /**
     * An Umvuthi killed away routs its Invasion, and its removal waits for a player (test 26; MZB-817, MZB-826, MZB-827).
     * <ol>
     * <li>Killed away (lane A). A puppet settled before a player, with six warriors: three within 30 blocks of it and three past
     * 45, all with no AI, is killed by that player. One CHIEF_KILLED names the player and one Sol Visage lies where it fell at the
     * animation's end. The jar's {@code die} takes the three near warriors, each by a hit with no entity: three BAND_CASUALTIES,
     * each by a player and naming the player (the kill credit, MZB-826), and the three far warriors live and book nothing.</li>
     * <li>The throne body (grove G, adopted as the chief) is a second body once the puppet's death is journaled. The grove's scripted
     * REMOVE of it is held while a second player stands within 160 blocks of the throne (no answer, one more held line and removal,
     * the body standing, no CAMP_DEATH), and played when that player is gone: DONE, the body removed, no Sol Visage.</li>
     * <li>Rule U in the trim (lane B, view distance 8, so sight is 128). A second grove's puppet settles before a player, which then
     * stands 170 blocks off with the puppet's target set again each tick. A BAND_MARCH without the war chief arrives: the books have
     * lost the chief, and the trim, which cuts what no player sees, does not cut this one for 1,300 ticks (it stays due). With its
     * target cleared, the next beat of the stop ({@link ConflictBridge#STOP_BEAT} ticks) cuts it: it books nothing.</li>
     * </ol>
     * Fails without MZB-826 (the casualties name no kind: the warriors' deaths carry no entity), and without MZB-827 (the trim cuts the
     * puppet out of sight, at once).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_killed_away", skyAccess = true)
    public static void anUmvuthiKilledAwayRoutsItsInvasion(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 9000);
        String test = "anUmvuthiKilledAwayRoutsItsInvasion";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        String dim = RtsimTests.dimension(helper);
        Lane lane = new Lane(far, 0, 6), second = new Lane(far, 2, 3);
        Grove g = groveAt(far, 0, 1), h = groveAt(far, 1, 2); // G: the throne body and the first band's faction; H: the second band's
        long agent = ConflictKit.newAgent(), agent2 = ConflictKit.newAgent();
        EntityUmvuthi[] throne = new EntityUmvuthi[1], puppet = new EntityUmvuthi[2];
        ServerPlayer[] player = new ServerPlayer[3]; // the killer, the watcher by the throne, the target of the second puppet
        Mob[] near = new Mob[3], away = new Mob[3];
        long[] op = new long[2];
        long[] was = new long[7]; // chiefsKilled, deathLootLost, held removals, held lines, trimmed, the game time of the clear, the cut
        Vec3[] fell = new Vec3[2];
        boolean[] fight = {false};
        Runnable[] back = new Runnable[1];

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    lane.force();
                    second.force();
                    g.force();
                    h.force();
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    lane.floor();
                    second.floor();
                    g.lay().declare();
                    h.lay().declare();
                    throne[0] = MowzieKit.umvuthi(level, g.at(-10, 0));
                    player[1] = MowzieKit.player(helper, off(g, 0, 60), "mowzie-killed-away-watcher");
                })
                .thenWaitUntil(waits.logic(censused(g, h)))
                .thenExecute(() -> op[0] = adoptChief(g, throne[0], 1))
                .thenWaitUntil(waits.logic(() -> MowzieKit.answered(g.faction, op[0], 1)))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("ADOPT", ConflictKit.outcomes(g.faction, op[0]), DONE);
                    check("the watcher is within 160 blocks of the throne body", player[1].distanceToSqr(throne[0]) < GroveKind.HOLD_RANGE * GroveKind.HOLD_RANGE);
                    bandAt(helper, g.faction, agent, lane.x0 + 20, lane.top + 1, lane.z, 0f, List.of(), 6, true);
                })
                .thenWaitUntil(waits.logic(() -> {
                    puppet[0] = puppet(agent);
                    check("the puppet floats", puppet[0].isNoGravity());
                    check("its six warriors are embodied (" + warriorsOf(agent).size() + ")", warriorsOf(agent).size() == 6);
                }))
                .thenExecute(() -> {
                    // the jar's own AI would summon and fight as it likes: the abilities are not this test's, and the target is set by hand
                    puppet[0].setNoAi(true);
                    puppet[0].getAttribute(Attributes.FOLLOW_RANGE).setBaseValue(1000);
                    player[0] = MowzieKit.player(helper, new Vec3(puppet[0].getX() + 10, lane.top + 1, puppet[0].getZ()), "mowzie-killed-away-killer");
                    List<Mob> w = warriorsOf(agent);
                    for (int i = 0; i < 3; i++) {
                        near[i] = w.get(i);
                        away[i] = w.get(3 + i);
                        near[i].setNoAi(true);
                        away[i].setNoAi(true);
                        near[i].moveTo(puppet[0].getX() + 6 + 3 * i, lane.top + 1, puppet[0].getZ() + 3 * (i - 1));
                        away[i].moveTo(puppet[0].getX() + 45 + 3 * i, lane.top + 1, puppet[0].getZ() + 3 * (i - 1));
                    }
                    MowzieKit.everyTick(helper, () -> {
                        if (puppet[0].isAlive() && !puppet[0].isRemoved() && player[0].isAlive())
                            puppet[0].setTarget(player[0]);
                    });
                })
                .thenWaitUntil(waits.logic(() -> check("it has settled before the player (gravity " + !puppet[0].isNoGravity() + ")", !puppet[0].isNoGravity())))
                .thenExecute(() -> {
                    for (int i = 0; i < 3; i++) {
                        check("warrior " + i + " is within 30 blocks of the puppet (" + near[i].distanceTo(puppet[0]) + ")", near[i].distanceTo(puppet[0]) < 30f);
                        check("far warrior " + i + " is past 40 (" + away[i].distanceTo(puppet[0]) + ")", away[i].distanceTo(puppet[0]) > 40f);
                    }
                    was[0] = ConflictKit.bridge().chiefsKilled;
                    was[1] = ConflictKit.deathLootLost();
                    fell[0] = puppet[0].position();
                    // the jar's `die` hurts each warrior with a source that names no entity: the credit is the hit of a player's lately
                    for (Mob n : near)
                        n.setLastHurtByPlayer(player[0]);
                    puppet[0].setLastHurtByPlayer(player[0]);
                    MowzieKit.kill(puppet[0], level.damageSources().playerAttack(player[0]));
                })
                .thenWaitUntil(waits.logic(() -> check("the three near warriors' deaths are booked ("
                        + ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent).size() + ")", ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent).size() >= 3)))
                .thenWaitUntil(waits.logic(() -> check("the puppet's animation has ended (removed " + puppet[0].isRemoved() + ")", puppet[0].isRemoved())))
                .thenIdle(10)
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> killed = ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent);
                    check("one CHIEF_KILLED (" + killed.size() + ")", killed.size() == 1);
                    check("by a player", killed.get(0).aux() == ConflictRecords.BY_PLAYER);
                    check("naming the killer", player[0].getUUID().equals(ConflictKit.uuidAt(killed.get(0), ConflictRecords.ChiefKilled.KILLER_MOST)));
                    check("chiefsKilled rose by one (" + was[0] + " to " + ConflictKit.bridge().chiefsKilled + ")", ConflictKit.bridge().chiefsKilled == was[0] + 1);
                    check("one Sol Visage where the puppet fell (" + visagesNear(level, fell[0], 6) + ")", visagesNear(level, fell[0], 6) == 1);
                    check("no loot record was lost (" + was[1] + " to " + ConflictKit.deathLootLost() + ")", ConflictKit.deathLootLost() == was[1]);
                    check("nothing is dying (" + ConflictKit.bridge().dyingBodies() + ")", ConflictKit.bridge().dyingBodies() == 0);
                    List<ScriptedEngine.Received> cas = ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent);
                    check("three casualties, no more (" + cas.size() + ")", cas.size() == 3);
                    for (ScriptedEngine.Received r : cas) {
                        check("a casualty is by a player (aux " + r.aux() + ")", r.aux() == ConflictRecords.BY_PLAYER);
                        check("and names the killer", player[0].getUUID().equals(ConflictKit.uuidAt(r, ConflictRecords.Casualties.KILLER_MOST)));
                        check("of one body (" + r.view().getInt(ConflictRecords.Casualties.COUNT) + ")", r.view().getInt(ConflictRecords.Casualties.COUNT) == 1);
                    }
                    for (int i = 0; i < 3; i++) {
                        check("near warrior " + i + " is dead", near[i].isDeadOrDying() || near[i].isRemoved());
                        check("far warrior " + i + " lives", away[i].isAlive() && !away[i].isRemoved());
                    }
                    check("the throne body is a second body now", MowzieKit.camps().secondBody(throne[0]));
                    record(test, "killed away", "one CHIEF_KILLED and one Sol Visage; " + cas.size() + " casualties by the player of the 3 within 30 blocks, the 3 beyond 45 alive");
                    // the throne body's removal waits for the watcher
                    was[2] = MowzieKit.camps().heldRemovals;
                    was[3] = MowzieKit.camps().heldLines();
                    op[1] = MowzieKit.reconcile(g.faction, epochOf(g), List.of(CampLine.of(CHIEF_SLOT, 1, UMVUTHI, REMOVE, CHIEF)));
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    check("the REMOVE has no answer while the watcher stands near", ConflictKit.outcomes(g.faction, op[1]).isEmpty());
                    check("the throne body stands (alive " + throne[0].isAlive() + ", removed " + throne[0].isRemoved() + ")", throne[0].isAlive() && !throne[0].isRemoved());
                    check("one more held removal (" + (MowzieKit.camps().heldRemovals - was[2]) + ")", MowzieKit.camps().heldRemovals == was[2] + 1);
                    check("one more held line (" + (MowzieKit.camps().heldLines() - was[3]) + ")", MowzieKit.camps().heldLines() == was[3] + 1);
                    check("no CAMP_DEATH for the grove (" + ConflictKit.campDeaths(g.faction).size() + ")", ConflictKit.campDeaths(g.faction).isEmpty());
                    fell[1] = throne[0].position();
                    player[1].moveTo(off(g, 300, 0).x, off(g, 300, 0).y, off(g, 300, 0).z);
                })
                .thenWaitUntil(waits.logic(() -> MowzieKit.answered(g.faction, op[1], 1)))
                .thenExecute(() -> {
                    check("the REMOVE is answered DONE, not " + MowzieKit.outcomeName(answerOf(g, op[1])), answerOf(g, op[1]) == DONE);
                    check("the throne body is removed", throne[0].isRemoved());
                    check("no Sol Visage at the throne (" + visagesNear(level, fell[1], 8) + ")", visagesNear(level, fell[1], 8) == 0);
                    check("still no CAMP_DEATH (" + ConflictKit.campDeaths(g.faction).size() + ")", ConflictKit.campDeaths(g.faction).isEmpty());
                    check("the held lines are back to the start (" + MowzieKit.camps().heldLines() + ")", MowzieKit.camps().heldLines() == was[3]);
                    record(test, "removal", "REMOVE of the second body held 40 ticks while the watcher stood within 160 blocks, DONE once it stood 300 off");
                    // Rule U in the trim: a second Invasion's puppet, its player 170 blocks off, out of sight at view distance 8
                    back[0] = viewDistance(helper, level, 8);
                    bandAt(helper, h.faction, agent2, second.x0 + 20, second.top + 1, second.z, 0f, List.of(), 2, true);
                })
                .thenWaitUntil(waits.logic(() -> {
                    puppet[1] = puppet(agent2);
                    check("the second puppet floats", puppet[1].isNoGravity());
                    check("its warriors are embodied (" + warriorsOf(agent2).size() + ")", warriorsOf(agent2).size() == 2);
                }))
                .thenExecute(() -> {
                    puppet[1].setNoAi(true);
                    puppet[1].getAttribute(Attributes.FOLLOW_RANGE).setBaseValue(1000);
                    for (Mob m : warriorsOf(agent2))
                        m.setNoAi(true);
                    player[2] = MowzieKit.player(helper, new Vec3(puppet[1].getX() + 10, second.top + 1, puppet[1].getZ()), "mowzie-killed-away-target");
                    fight[0] = true;
                    MowzieKit.everyTick(helper, () -> {
                        if (fight[0] && puppet[1].isAlive() && !puppet[1].isRemoved())
                            puppet[1].setTarget(player[2]);
                    });
                })
                .thenWaitUntil(waits.logic(() -> check("it has settled before the player (gravity " + !puppet[1].isNoGravity() + ")", !puppet[1].isNoGravity())))
                .thenExecute(() -> {
                    player[2].moveTo(puppet[1].getX(), second.top + 1, puppet[1].getZ() + 170);
                    double sight = ConflictBridge.sightRange(puppet[1], level);
                    check("sight is 128 blocks at view distance 8 (" + sight + ")", sight == 128.0);
                    check("the player is 170 blocks off, out of sight and past the hold's range", Math.abs(player[2].getZ() - puppet[1].getZ()) > 169.9);
                    for (ServerPlayer p : level.players())
                        if (p != player[2])
                            check("no other player is within the hold's range of the second puppet (" + p.getGameProfile().getName() + ")",
                                    Math.hypot(p.getX() - puppet[1].getX(), p.getZ() - puppet[1].getZ()) > GroveKind.HOLD_RANGE + 1);
                    was[4] = ConflictKit.trimmed();
                    marchOf(agent2, h.faction, dim, second.x0 + 20, second.z, false, Map.of(WARRIOR, 2));
                })
                .thenWaitUntil(waits.logic(() -> {
                    check("the march is held", ConflictKit.bridge().march(agent2) != null);
                    check("the trim found the chief it may not cut: it is due", ConflictKit.party(agent2).trimDue());
                }))
                .thenIdle(1300)
                .thenExecute(() -> {
                    check("the second puppet stands after 1,300 ticks (alive " + puppet[1].isAlive() + ", removed " + puppet[1].isRemoved() + ")",
                            puppet[1].isAlive() && !puppet[1].isRemoved());
                    check("the trim cut nothing (" + (ConflictKit.trimmed() - was[4]) + ")", ConflictKit.trimmed() == was[4]);
                    check("the trim is still due", ConflictKit.party(agent2).trimDue());
                    check("the chief's slot is alive", chiefSlot(agent2).alive());
                    check("its warriors stand (" + warriorsOf(agent2).size() + ")", warriorsOf(agent2).size() == 2);
                    fight[0] = false;
                    puppet[1].setTarget(null);
                    was[5] = level.getGameTime();
                })
                .thenWaitUntil(waits.logic(() -> check("the puppet is cut (removed " + puppet[1].isRemoved() + ")", puppet[1].isRemoved())))
                .thenExecute(() -> {
                    was[6] = level.getGameTime();
                    long ticks = was[6] - was[5];
                    check("cut at the next beat, within " + (ConflictBridge.STOP_BEAT + 2) + " ticks of the clear (" + ticks + ")", ticks <= ConflictBridge.STOP_BEAT + 2);
                    check("the trim cut one member (" + (ConflictKit.trimmed() - was[4]) + ")", ConflictKit.trimmed() == was[4] + 1);
                    check("a cut books nothing: no CHIEF_KILLED (" + ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent2).size() + ")",
                            ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent2).isEmpty());
                    check("and no casualty (" + ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent2).size() + ")",
                            ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent2).isEmpty());
                    check("its warriors stand (" + warriorsOf(agent2).size() + ")", warriorsOf(agent2).size() == 2);
                    record(test, "rule U", "the second puppet, its target 170 blocks off out of sight at view distance 8, was not cut in 1300 ticks; cut "
                            + (was[6] - was[5]) + " ticks after the target was cleared");
                    back[0].run();
                    recallAll(agent, agent2);
                })
                .thenWaitUntil(waits.logic(() -> check("both bands went (" + dematerialized(agent).size() + " and " + dematerialized(agent2).size() + ")",
                        dematerialized(agent).size() >= 1 && dematerialized(agent2).size() >= 1)))
                .thenExecute(() -> {
                    drop(player);
                    far.releaseAll();
                })
                .thenSucceed();
    }

    // ---- 29a, 29b: the throne body and its puppet (MZB-825, MZB-823) -----------------------------------

    private static final int TAKE = ConflictRecords.CampReconcile.TAKE;

    /** The CAMP_DEATH records a faction's camp journaled for one slot. */
    private static List<ScriptedEngine.Received> campDeathsOf(long faction, int slot) {
        List<ScriptedEngine.Received> out = new ArrayList<>();
        for (ScriptedEngine.Received r : ConflictKit.campDeaths(faction))
            if (r.view().getInt(ConflictRecords.CampDeath.SLOT) == slot)
                out.add(r);
        return out;
    }

    /**
     * A second Umvuthi body books nothing, in either order (test 29's second-body clause; MZB-825). Two groves 320 blocks apart, each
     * with its throne Umvuthi adopted as the chief, a warrior of the camp 10 blocks from it, an Invasion's puppet 40 blocks off with two
     * warriors (more than 30 from the camp's warrior) and a masked player 50 blocks from the throne, so that the engine's TAKE of the
     * chief, sent after the grove came in, is held.
     * <ul>
     * <li>Grove P, the puppet first. The player kills the puppet: one CHIEF_KILLED naming the player, one Sol Visage where it fell, its
     * two warriors taken by the jar's {@code die}, and the camp's warrior untouched. The throne body is a second body from then on. The
     * player kills the throne body: past its animation no CAMP_DEATH of any kind, no second CHIEF_KILLED, no Sol Visage within 8
     * blocks, the camp's warrior alive and tagged, the held TAKE answered NOTHING, and the body gone.</li>
     * <li>Grove T, the throne body first. The player kills it: one CAMP_DEATH of the chief's slot, one Sol Visage, and the jar's
     * {@code die} takes the camp's warrior (one CAMP_DEATH of slot 0). The puppet is a second body from then on. The player kills it:
     * no CHIEF_KILLED, no Sol Visage within 6 blocks of where it fell, no change in {@code chiefsKilled}, {@code deathLootLost} or
     * {@code chiefDeathsAfterEnd}, and both of its warriors alive (its {@code die} found none).</li>
     * </ul>
     * Each order's first death is the control (it books and drops once). Fails without MZB-825: the second death journals and drops again,
     * and its {@code die} kills the warriors within 30.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_second_body", skyAccess = true)
    public static void aSecondUmvuthiBodyBooksNothing(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 9000);
        String test = "aSecondUmvuthiBodyBooksNothing";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove[] g = {groveAt(far, 0, 0), groveAt(far, 1, 0)}; // P: the puppet dies first; T: the throne body does
        long[] agent = {ConflictKit.newAgent(), ConflictKit.newAgent()};
        EntityUmvuthi[] throne = new EntityUmvuthi[2], puppet = new EntityUmvuthi[2];
        EntityUmvuthanaMinion[] camp = new EntityUmvuthanaMinion[2];
        ServerPlayer[] player = new ServerPlayer[2];
        List<List<Mob>> column = List.of(new ArrayList<>(), new ArrayList<>());
        Vec3[] fellPuppet = new Vec3[2], fellThrone = new Vec3[2];
        long[] op = new long[4];
        long[] was = new long[4]; // chiefsKilled, deathLootLost, chiefDeathsAfterEnd, held lines
        long[] later = new long[2]; // chiefsKilled, the held lines, after the first deaths

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    for (Grove x : g) {
                        x.force();
                        forceEast(x, 4);
                    }
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++) {
                        g[i].lay().declare();
                        layEast(g[i], 4);
                        throne[i] = MowzieKit.umvuthi(level, g[i].at(-12, 0));
                        camp[i] = MowzieKit.warrior(level, g[i].at(-22, 0));
                        player[i] = MowzieKit.player(helper, off(g[i], -12, 50), "mowzie-second-body-" + i);
                        MowzieKit.mask(player[i]);
                    }
                })
                .thenWaitUntil(waits.logic(censused(g)))
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++)
                        op[i] = MowzieKit.reconcile(g[i].faction, epochOf(g[i]), List.of(
                                CampLine.of(CHIEF_SLOT, 1, UMVUTHI, ADOPT, CHIEF, throne[i].getUUID()),
                                CampLine.of(0, 1, WARRIOR, ADOPT, 0, camp[i].getUUID())));
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < 2; i++)
                        MowzieKit.answered(g[i].faction, op[i], 2);
                }))
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++) {
                        MowzieKit.allOutcomes("ADOPT " + i, ConflictKit.outcomes(g[i].faction, op[i]), DONE);
                        chiefBand(helper, g[i], agent[i], g[i].spot.x + 40, g[i].spot.z, 90f, List.of(), 2);
                    }
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < 2; i++) {
                        puppet[i] = puppet(agent[i]);
                        check("puppet " + i + " floats", puppet[i].isNoGravity());
                        check("its column has two warriors (" + warriorsOf(agent[i]).size() + ")", warriorsOf(agent[i]).size() == 2);
                    }
                }))
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++) {
                        column.get(i).addAll(warriorsOf(agent[i]));
                        for (Mob w : column.get(i))
                            check("grove " + i + "'s column warrior stands within 30 blocks of its puppet (" + w.distanceTo(puppet[i]) + ")",
                                    w.distanceTo(puppet[i]) < 30);
                        check("the camp's warrior is farther than 30 from the puppet (" + camp[i].distanceTo(puppet[i]) + ")",
                                camp[i].distanceTo(puppet[i]) > 30);
                        check("and within 30 of its throne body (" + camp[i].distanceTo(throne[i]) + ")", camp[i].distanceTo(throne[i]) <= 30);
                    }
                    was[3] = MowzieKit.camps().heldLines();
                    // the grove came in while the Invasion was out and a player stands near: the engine's TAKE of the chief is held
                    for (int i = 0; i < 2; i++)
                        op[2 + i] = MowzieKit.reconcile(g[i].faction, epochOf(g[i]), g[i].spot, 3f, List.of(CampLine.of(CHIEF_SLOT, 1, UMVUTHI, TAKE, CHIEF)));
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    check("both takes are held (" + (MowzieKit.camps().heldLines() - was[3]) + " lines)", MowzieKit.camps().heldLines() == was[3] + 2);
                    for (int i = 0; i < 2; i++) {
                        check("grove " + i + "'s take has no answer", ConflictKit.outcomes(g[i].faction, op[2 + i]).isEmpty());
                        check("grove " + i + "'s throne body is the chief still", !MowzieKit.camps().secondBody(throne[i]));
                        check("grove " + i + "'s puppet is not a second body", !MowzieKit.camps().secondBody(puppet[i]));
                    }
                    was[0] = ConflictKit.bridge().chiefsKilled;
                    was[1] = ConflictKit.deathLootLost();
                    was[2] = ConflictKit.chiefDeathsAfterEnd();
                    // the first deaths: P's puppet and T's throne body
                    fellPuppet[0] = puppet[0].position();
                    puppet[0].setLastHurtByPlayer(player[0]);
                    MowzieKit.kill(puppet[0], level.damageSources().playerAttack(player[0]));
                    fellThrone[1] = throne[1].position();
                    throne[1].setLastHurtByPlayer(player[1]);
                    MowzieKit.kill(throne[1], level.damageSources().playerAttack(player[1]));
                })
                .thenWaitUntil(waits.logic(() -> check("both animations have ended (P's puppet " + puppet[0].isRemoved() + ", T's throne body "
                        + throne[1].isRemoved() + ")", puppet[0].isRemoved() && throne[1].isRemoved())))
                .thenIdle(10)
                .thenExecute(() -> {
                    // P: the puppet's death is the chief's
                    List<ScriptedEngine.Received> killed = ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[0]);
                    check("P's puppet journals one CHIEF_KILLED (" + killed.size() + ")", killed.size() == 1);
                    check("by a player", killed.get(0).aux() == ConflictRecords.BY_PLAYER);
                    check("naming the player", player[0].getUUID().equals(ConflictKit.uuidAt(killed.get(0), ConflictRecords.ChiefKilled.KILLER_MOST)));
                    check("one Sol Visage where it fell (" + visagesNear(level, fellPuppet[0], 6) + ")", visagesNear(level, fellPuppet[0], 6) == 1);
                    check("the jar's die took the column's warriors (alive " + column.get(0).stream().filter(Mob::isAlive).count() + ")",
                            column.get(0).stream().noneMatch(Mob::isAlive));
                    check("the camp's warrior, 62 blocks off, lives and is tagged", camp[0].isAlive() && MowzieKit.tagged(camp[0], g[0].faction, 0, 1));
                    check("P's throne body is a second body now", MowzieKit.camps().secondBody(throne[0]));
                    check("no CAMP_DEATH for P yet (" + ConflictKit.campDeaths(g[0].faction).size() + ")", ConflictKit.campDeaths(g[0].faction).isEmpty());
                    // T: the throne body's death is the chief's
                    check("T's throne body booked one CAMP_DEATH of the chief's slot (" + campDeathsOf(g[1].faction, CHIEF_SLOT).size() + ")",
                            campDeathsOf(g[1].faction, CHIEF_SLOT).size() == 1);
                    check("one Sol Visage where it fell (" + visagesNear(level, fellThrone[1], 8) + ")", visagesNear(level, fellThrone[1], 8) == 1);
                    check("the jar's die took the camp's warrior: one CAMP_DEATH of slot 0 (" + campDeathsOf(g[1].faction, 0).size() + ")",
                            !camp[1].isAlive() && campDeathsOf(g[1].faction, 0).size() == 1);
                    check("T's puppet is a second body now", MowzieKit.camps().secondBody(puppet[1]));
                    check("T's party journaled no CHIEF_KILLED (" + ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[1]).size() + ")",
                            ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[1]).isEmpty());
                    check("T's column stands (" + warriorsOf(agent[1]).size() + ")", warriorsOf(agent[1]).size() == 2);
                    check("chiefsKilled rose by one (" + was[0] + " to " + ConflictKit.bridge().chiefsKilled + ")", ConflictKit.bridge().chiefsKilled == was[0] + 1);
                    later[0] = ConflictKit.bridge().chiefsKilled;
                    later[1] = MowzieKit.camps().heldLines();
                    // the second deaths: P's throne body and T's puppet
                    fellThrone[0] = throne[0].position();
                    throne[0].setLastHurtByPlayer(player[0]);
                    MowzieKit.kill(throne[0], level.damageSources().playerAttack(player[0]));
                    fellPuppet[1] = puppet[1].position();
                    puppet[1].setLastHurtByPlayer(player[1]);
                    MowzieKit.kill(puppet[1], level.damageSources().playerAttack(player[1]));
                })
                .thenWaitUntil(waits.logic(() -> check("both animations have ended (P's throne body " + throne[0].isRemoved() + ", T's puppet "
                        + puppet[1].isRemoved() + ")", throne[0].isRemoved() && puppet[1].isRemoved())))
                .thenIdle(25)
                .thenExecute(() -> {
                    // P: the throne body's death is nobody's
                    check("no CAMP_DEATH of any kind for P (" + ConflictKit.campDeaths(g[0].faction).size() + ")", ConflictKit.campDeaths(g[0].faction).isEmpty());
                    check("P's party still journaled one CHIEF_KILLED (" + ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[0]).size() + ")",
                            ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[0]).size() == 1);
                    check("no Sol Visage within 8 blocks of P's throne body (" + visagesNear(level, fellThrone[0], 8) + ")", visagesNear(level, fellThrone[0], 8) == 0);
                    check("the camp's warrior lives, tagged (its alive " + camp[0].isAlive() + ")",
                            camp[0].isAlive() && !camp[0].isRemoved() && MowzieKit.tagged(camp[0], g[0].faction, 0, 1));
                    check("P's throne body is gone", throne[0].isRemoved());
                    check("P's held TAKE was answered NOTHING, not " + MowzieKit.outcomeName(answerOf(g[0], op[2])), answerOf(g[0], op[2]) == NOTHING);
                    // T: the puppet's death is nobody's
                    check("T's party journaled no CHIEF_KILLED (" + ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[1]).size() + ")",
                            ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[1]).isEmpty());
                    check("no Sol Visage within 6 blocks of where T's puppet fell (" + visagesNear(level, fellPuppet[1], 6) + ")", visagesNear(level, fellPuppet[1], 6) == 0);
                    check("none of T's column's warriors died (alive " + column.get(1).stream().filter(Mob::isAlive).count() + " of 2)",
                            column.get(1).stream().allMatch(Mob::isAlive));
                    check("T booked its chief and its warrior once each, no more (" + ConflictKit.campDeaths(g[1].faction).size() + ")",
                            ConflictKit.campDeaths(g[1].faction).size() == 2);
                    check("chiefsKilled is unchanged by the second deaths (" + later[0] + " to " + ConflictKit.bridge().chiefsKilled + ")",
                            ConflictKit.bridge().chiefsKilled == later[0]);
                    check("deathLootLost is unchanged (" + was[1] + " to " + ConflictKit.deathLootLost() + ")", ConflictKit.deathLootLost() == was[1]);
                    check("chiefDeathsAfterEnd is unchanged (" + was[2] + " to " + ConflictKit.chiefDeathsAfterEnd() + ")",
                            ConflictKit.chiefDeathsAfterEnd() == was[2]);
                    check("nothing is dying (" + ConflictKit.bridge().dyingBodies() + ")", ConflictKit.bridge().dyingBodies() == 0);
                    record(test, "second body", "P: the puppet booked one CHIEF_KILLED and a visage, its throne body then booked nothing and took no warrior;"
                            + " T: the throne body booked its chief and warrior and a visage, its puppet then booked nothing and took no warrior");
                    recallAll(agent);
                })
                .thenWaitUntil(waits.logic(() -> check("both bands went (" + dematerialized(agent[0]).size() + " and " + dematerialized(agent[1]).size() + ")",
                        dematerialized(agent[0]).size() >= 1 && dematerialized(agent[1]).size() >= 1)))
                .thenExecute(() -> {
                    drop(player);
                    far.releaseAll();
                })
                .thenSucceed();
    }

    /**
     * A held TAKE of the chief is answered stale as its puppet comes home, and the hand-back merges (test 29b; MZB-823, MZB-821,
     * MZB-822, MZB-820). Grove H's Invasion floats its puppet 60 blocks east of the throne (health 180 asked of a jar that holds 150),
     * with a masked player near the throne, so the engine's TAKE of the chief is held, and a second masked player who pays the puppet.
     * The band is steered home.
     * <ul>
     * <li>The take is answered STALE no earlier than the puppet comes within {@link AloftRules#TAKE_STALE} blocks of the throne
     * (horizontally) and within two ticks of it, and the throne body stands.</li>
     * <li>The chief's HAND_BACK, sent with CHIEF_BODY home (naming no player) when the take is answered, floats the puppet to the spot.
     * Then one Umvuthi stands on the throne: the throne body, merged, with the puppet's health in place of its own 40, the payer in
     * its memory (it knew none), and no second body; the puppet is gone, the answer is NOTHING, and the bridge counted one merge.</li>
     * </ul>
     * The control, grove C, has no player within 160 blocks of its throne: the TAKE plays at once (DONE, the throne body gone), and
     * its HAND_BACK makes the body from the puppet (DONE, one Umvuthi tagged as the chief, health 180 of what the jar holds, no merge
     * counted). Fails without MZB-823: the take stays held and the HAND_BACK makes a second body.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_held_take", skyAccess = true)
    public static void aHeldTakeIsAnsweredStaleAsThePuppetComesHome(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 9000);
        String test = "aHeldTakeIsAnsweredStaleAsThePuppetComesHome";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove[] g = {groveAt(far, 0, 0), groveAt(far, 1, 0)}; // H: a player near, the take is held; C: none, the control
        long[] agent = {ConflictKit.newAgent(), ConflictKit.newAgent()};
        EntityUmvuthi[] throne = new EntityUmvuthi[2], puppet = new EntityUmvuthi[2];
        ServerPlayer[] player = new ServerPlayer[2]; // H's holder of the take, H's payer
        long[] op = new long[4]; // ADOPT H, ADOPT C, TAKE H, TAKE C
        long[] handBack = new long[2]; // H's, C's
        long[] was = new long[4]; // staleTakes, chiefMerges, held lines, puppetPaid
        long[] tick = {-1, -1}; // the first tick the puppet is within TAKE_STALE of the throne, and the first the take is no longer held

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    for (Grove x : g) {
                        x.force();
                        forceEast(x, 5);
                    }
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++) {
                        g[i].lay().declare();
                        layEast(g[i], 5);
                        throne[i] = MowzieKit.umvuthi(level, g[i].at(0, 0));
                    }
                    player[0] = MowzieKit.player(helper, off(g[0], 0, 50), "mowzie-held-take-holder");
                    MowzieKit.mask(player[0]);
                })
                .thenWaitUntil(waits.logic(censused(g)))
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++)
                        op[i] = adoptChief(g[i], throne[i], 1);
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < 2; i++)
                        MowzieKit.answered(g[i].faction, op[i], 1);
                }))
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++) {
                        MowzieKit.allOutcomes("ADOPT " + i, ConflictKit.outcomes(g[i].faction, op[i]), DONE);
                        throne[i].setHealth(40f);
                    }
                    chiefBand(helper, g[0], agent[0], g[0].spot.x + 60, g[0].spot.z, 180f, List.of(), 1);
                    chiefBand(helper, g[1], agent[1], g[1].spot.x + 45, g[1].spot.z, 180f, List.of(), 1);
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < 2; i++) {
                        puppet[i] = puppet(agent[i]);
                        check("puppet " + i + " floats", puppet[i].isNoGravity());
                    }
                }))
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++)
                        check("grove " + i + "'s puppet carries the health it was given (" + puppet[i].getHealth() + " of " + puppet[i].getMaxHealth() + ")",
                                puppet[i].getHealth() == Math.min(180f, puppet[i].getMaxHealth()));
                    player[1] = MowzieKit.player(helper, new Vec3(puppet[0].getX() + 6, g[0].spot.y + 1, puppet[0].getZ()), "mowzie-held-take-payer");
                    MowzieKit.mask(player[1]);
                    was[3] = ConflictKit.puppetPaid();
                    check("the jar remembers the payer of the puppet", MowzieKit.payUmvuthi(player[1], puppet[0]));
                    check("the puppet remembers the payer", puppet[0].hasTradedWith(player[1]));
                    check("the throne body does not", !throne[0].hasTradedWith(player[1]));
                    player[1].setPos(player[1].getX(), player[1].getY(), player[1].getZ() + 300); // out of the way of the walk home
                    was[0] = MowzieKit.camps().staleTakes;
                    was[1] = MowzieKit.camps().chiefMerges;
                    was[2] = MowzieKit.camps().heldLines();
                    for (int i = 0; i < 2; i++)
                        op[2 + i] = MowzieKit.reconcile(g[i].faction, epochOf(g[i]), g[i].spot, 3f, List.of(CampLine.of(CHIEF_SLOT, 1, UMVUTHI, TAKE, CHIEF)));
                })
                .thenWaitUntil(waits.logic(() -> check("C's take is answered", ConflictKit.outcomes(g[1].faction, op[3]).size() == 1)))
                .thenExecute(() -> {
                    check("C's take played: DONE, not " + MowzieKit.outcomeName(answerOf(g[1], op[3])), answerOf(g[1], op[3]) == DONE);
                    check("C's throne body is gone", throne[1].isRemoved() && thrones(level, g[1]).isEmpty());
                    check("H's take is held: one line, no answer (" + (MowzieKit.camps().heldLines() - was[2]) + ")",
                            MowzieKit.camps().heldLines() == was[2] + 1 && ConflictKit.outcomes(g[0].faction, op[2]).isEmpty());
                    check("H's throne body stands, the chief still", throne[0].isAlive() && !MowzieKit.camps().secondBody(throne[0]));
                    check("no take was answered stale yet (" + (MowzieKit.camps().staleTakes - was[0]) + ")", MowzieKit.camps().staleTakes == was[0]);
                    check("H's puppet is farther than " + AloftRules.TAKE_STALE + " blocks from its throne (" + puppet[0].distanceTo(throne[0]) + ")",
                            !AloftRules.takeStale(puppet[0].getX() - g[0].spot.x, puppet[0].getZ() - g[0].spot.z));
                    MowzieKit.everyTick(helper, () -> {
                        long now = level.getGameTime();
                        if (tick[0] < 0 && AloftRules.takeStale(puppet[0].getX() - g[0].spot.x, puppet[0].getZ() - g[0].spot.z))
                            tick[0] = now;
                        if (tick[1] < 0 && MowzieKit.camps().staleTakes > was[0])
                            tick[1] = now;
                    });
                    // C's puppet is called home at once (its body made from it); H's walks home with its band
                    ConflictKit.chiefBody(g[1].faction, epochOf(g[1]), 180f, List.of());
                    handBack[1] = MowzieKit.reconcile(g[1].faction, epochOf(g[1]), g[1].spot, 3f, List.of(
                            CampLine.party(CHIEF_SLOT, 1, UMVUTHI, HAND_BACK, CHIEF, agent[1], ConflictKit.party(agent[1]).epoch)));
                    ConflictKit.steer(agent[0], ConflictKit.party(agent[0]).epoch, PACE, g[0].spot.x, g[0].spot.z, 0, false);
                })
                .thenWaitUntil(waits.logic(() -> check("H's take is answered (" + ConflictKit.outcomes(g[0].faction, op[2]).size() + ")",
                        ConflictKit.outcomes(g[0].faction, op[2]).size() == 1)))
                .thenExecute(() -> {
                    check("H's take is answered STALE, not " + MowzieKit.outcomeName(answerOf(g[0], op[2])), answerOf(g[0], op[2]) == STALE);
                    check("one take was counted stale (" + (MowzieKit.camps().staleTakes - was[0]) + ")", MowzieKit.camps().staleTakes == was[0] + 1);
                    check("the puppet came within " + AloftRules.TAKE_STALE + " blocks at tick " + tick[0] + " and the take went at " + tick[1],
                            tick[0] >= 0 && tick[1] >= tick[0] && tick[1] - tick[0] <= 2);
                    check("H's throne body still stands, not second", throne[0].isAlive() && !MowzieKit.camps().secondBody(throne[0]));
                    check("H's throne body has its own health still (" + throne[0].getHealth() + ")", throne[0].getHealth() == 40f);
                    check("the held line is gone (" + (MowzieKit.camps().heldLines() - was[2]) + ")", MowzieKit.camps().heldLines() == was[2]);
                    // the engine's own home sends the chief's HAND_BACK, with its memory of a chief at home (no player named)
                    ConflictKit.chiefBody(g[0].faction, epochOf(g[0]), 180f, List.of());
                    handBack[0] = MowzieKit.reconcile(g[0].faction, epochOf(g[0]), g[0].spot, 3f, List.of(
                            CampLine.party(CHIEF_SLOT, 1, UMVUTHI, HAND_BACK, CHIEF, agent[0], ConflictKit.party(agent[0]).epoch)));
                })
                .thenWaitUntil(waits.logic(() -> {
                    check("both hand-backs are answered", ConflictKit.outcomes(g[0].faction, handBack[0]).size() == 1
                            && ConflictKit.outcomes(g[1].faction, handBack[1]).size() == 1);
                }))
                .thenExecute(() -> {
                    check("H's hand-back is answered NOTHING, not " + MowzieKit.outcomeName(answerOf(g[0], handBack[0])), answerOf(g[0], handBack[0]) == NOTHING);
                    List<EntityUmvuthi> stands = thrones(level, g[0]);
                    check("one Umvuthi stands on H's throne (" + stands.size() + ")", stands.size() == 1 && stands.get(0) == throne[0]);
                    check("it is the throne body, merged, with the puppet's health (" + throne[0].getHealth() + ")",
                            throne[0].getHealth() == Math.min(180f, throne[0].getMaxHealth()));
                    check("it remembers the payer", throne[0].hasTradedWith(player[1]));
                    check("and no one else (" + throne[0].getTradedPlayers().size() + ")", throne[0].getTradedPlayers().size() == 1);
                    check("the puppet is gone", puppet[0].isRemoved());
                    check("the bridge counted one merge (" + (MowzieKit.camps().chiefMerges - was[1]) + ")", MowzieKit.camps().chiefMerges == was[1] + 1);
                    check("and one payment to a puppet (" + (ConflictKit.puppetPaid() - was[3]) + ")", ConflictKit.puppetPaid() == was[3] + 1);
                    // the control
                    check("C's hand-back is DONE, not " + MowzieKit.outcomeName(answerOf(g[1], handBack[1])), answerOf(g[1], handBack[1]) == DONE);
                    List<net.minecraft.world.entity.Entity> made = MowzieKit.held(g[1].faction, CHIEF_SLOT, 1);
                    check("one Umvuthi stands on C's throne as the chief (" + made.size() + ")", made.size() == 1 && made.get(0) instanceof EntityUmvuthi);
                    check("made from the puppet, with its health (" + ((EntityUmvuthi) made.get(0)).getHealth() + ")",
                            ((EntityUmvuthi) made.get(0)).getHealth() == Math.min(180f, ((EntityUmvuthi) made.get(0)).getMaxHealth()));
                    check("C's puppet is gone", puppet[1].isRemoved());
                    record(test, "held take", "the take went stale at tick " + tick[1] + ", " + (tick[1] - tick[0]) + " after the puppet came within "
                            + AloftRules.TAKE_STALE + " blocks; the hand-back merged into the throne body (health " + throne[0].getHealth() + ", payer kept);"
                            + " the control made its body");
                    recallAll(agent);
                })
                .thenWaitUntil(waits.logic(() -> check("both bands went (" + dematerialized(agent[0]).size() + " and " + dematerialized(agent[1]).size() + ")",
                        dematerialized(agent[0]).size() >= 1 && dematerialized(agent[1]).size() >= 1)))
                .thenExecute(() -> {
                    drop(player);
                    far.releaseAll();
                })
                .thenSucceed();
    }

    // ---- 29c: the four exceptions, and the invariant's own test (MZB-827, MZB-817, MZB-824) ---------------------

    /**
     * A server player the world sees, with the simulation distance {@code s} and the view distance {@code s + 2} set for it: the
     * ticking area of a real player, in a far place with no other (a copy of core's {@code MarchGameTests.Ticking}, on a {@link Far}).
     * It stands on its chunk's west column ({@code px = 16 * pcx + 0.5}) and mid-chunk in z, so the ticking edge is
     * {@code 16 * (s + 1) - 0.5} blocks east of it. Both distances are the server's, so {@link #end} puts them back (the player first,
     * before any chunk is let go); it runs when the test ends too, and a second run does nothing. A test using this has a batch of its
     * own and forces no chunk near the player.
     */
    private static final class Watch {
        final Far far;
        final ServerLevel level;
        final int s, pcx, pcz;
        final double px, pz;
        ServerPlayer player;
        private final int savedSimulation, savedView;
        private boolean ended;

        Watch(GameTestHelper helper, Far far, int s) {
            this.far = far;
            this.level = far.level;
            this.s = s;
            pcx = far.k + 32;
            pcz = far.z0 + 32;
            px = 16.0 * pcx + 0.5;
            pz = 16.0 * pcz + 8.0;
            var players = level.getServer().getPlayerList();
            savedSimulation = players.getSimulationDistance();
            savedView = players.getViewDistance();
            ConflictKit.onEnd(helper, this::end);
        }

        /** The first x outside the ticking area, on the player's row, where the player stands now. */
        double edge() {
            return 16.0 * (player.chunkPosition().x + s + 1);
        }

        /** Sets the distances and joins the player (its chunks load and tick over the next ticks). */
        void start() {
            var players = level.getServer().getPlayerList();
            players.setViewDistance(s + 2);
            players.setSimulationDistance(s);
            player = FakePlayers.addSeen(level, new Vec3(px, level.getMinBuildHeight() + 64, pz), "mowzie-aloft-watcher-" + s);
            player.setNoGravity(true);
        }

        /** Stands the player on the ground once its chunk is there. */
        void ground() {
            double y = level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, (int) Math.floor(px), (int) Math.floor(pz));
            player.moveTo(px, y, pz);
        }

        /** Moves the player {@code dx} blocks along x and tells the chunk map, as the server does for a player's move packet. */
        void step(double dx) {
            player.moveTo(player.getX() + dx, player.getY(), player.getZ());
            level.getChunkSource().move(player);
        }

        /** Fails until the square of chunks within {@code s} of the player's ticks entities and the first chunk past it east does not. */
        void check() {
            int n = 0, total = (2 * s + 1) * (2 * s + 1);
            for (int x = pcx - s; x <= pcx + s; x++)
                for (int z = pcz - s; z <= pcz + s; z++)
                    if (RtsimSession.entitiesTickAt(level, new BlockPos(16 * x + 8, level.getSeaLevel(), 16 * z + 8)))
                        n++;
            if (n < total)
                throw new GameTestAssertException(n + " of " + total + " chunks within " + s + " of the player's (" + pcx + ", " + pcz
                        + ") tick entities (simulation distance " + level.getServer().getPlayerList().getSimulationDistance() + ")");
            if (RtsimSession.entitiesTickAt(level, new BlockPos(16 * (pcx + s + 1) + 8, level.getSeaLevel(), 16 * pcz + 8)))
                throw new GameTestAssertException("the chunk " + (s + 1) + " east of the player's ticks entities: the area is wider than " + s);
        }

        /** Takes the player out and puts both distances back. */
        void end() {
            if (ended)
                return;
            ended = true;
            if (player != null)
                FakePlayers.removeSeen(player);
            var players = level.getServer().getPlayerList();
            players.setSimulationDistance(savedSimulation);
            players.setViewDistance(savedView);
        }
    }

    /** The engine's march of a party it has ended (FLAG_ENDED): home, so far as the engine is concerned; its later records are dropped. */
    private static void marchEnded(long agent, long faction, String dim, double x, double z) {
        long bx = (long) Math.floor(x), bz = (long) Math.floor(z);
        ConflictKit.march(new ConflictKit.MarchSpec(agent, 1, dim, faction, ConflictRecords.PURPOSE_INVASION, ConflictRecords.March.HOME,
                ConflictRecords.March.FLAG_CHIEF | ConflictRecords.March.FLAG_ENDED, 0L, new LinkedHashMap<>(Map.of(WARRIOR, 1)),
                List.of(new long[] {bx, bz, 0L}, new long[] {bx + 1, bz, 10_000L})));
    }

    /** The party's stacks, by their states (for a message). */
    private static String stateOf(long agent) {
        ConflictBridge.Party p = ConflictKit.bridge().party(agent);
        if (p == null)
            return "no party";
        StringBuilder b = new StringBuilder();
        for (ConflictBridge.Stack st : p.stacks())
            b.append(b.length() == 0 ? "" : ", ").append(st.state());
        return b.toString();
    }

    // ---- 29c-edge: at the ticking edge it is taken back, not held (MZB-827) --------------------------------

    /**
     * An aloft puppet whose chunk stops ticking entities is taken back to pending that tick, and Rule U does not hold it there (test
     * 29c, the edge; MZB-827). A seen player (a real player's entity-ticking area, {@link Watch}, simulation distance 8 and view 10)
     * stands on its chunk's west column, masked. A band met 132 blocks east of it, on its row, of one warrior and the war chief, is
     * placed (both in chunks that tick). The faction has a declared grove that is out (its chunks are not loaded), so that the grove's rule
     * has a site to ask: with the player within 160 blocks of the puppet it says hold. The player steps 20 blocks west, so that the
     * edge goes from 143.5 to 111.5 blocks east of where it stood and the band stands past it, 152 blocks from the player and
     * still inside the hold's range. Each tick the test notes the first tick the puppet's chunk does not tick entities and the first
     * tick the body is removed, and the same for the warrior.
     * <ul>
     * <li>The puppet is removed within 2 ticks of its chunk's stopping, and no later than a tick after the warrior (the control: the
     * same chunk, the same distance, the same tick). {@code requeued} rose by two (the chief's stack and the warrior's), and each stack is
     * pending, its chief's slot alive.</li>
     * <li>No Umvuthi stands in the loaded chunks within 200 blocks of where it was, at once and 40 ticks on; no CHIEF_KILLED, no
     * casualty. The puppet is removed (the world holds no body of it to save), so no chunk of this area can write it with its gravity off.</li>
     * </ul>
     * Fails if {@code requeue} asked Rule U (the puppet hangs in a chunk that does not tick, with its gravity off, and the chunk's
     * unload saves it). The grove's out-ness is the set-up; the chunk read-back of 23e is not repeated (a removed body is in no save).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_edge", skyAccess = true)
    public static void anAloftUmvuthiAtTheTickingEdgeIsTakenBack(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 6000);
        String test = "anAloftUmvuthiAtTheTickingEdgeIsTakenBack";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Watch w = new Watch(helper, far, 8);
        Grove g = groveAt(far, 0, 0); // declared, never forced: out, and the faction has a site for Rule U to ask
        long agent = ConflictKit.newAgent();
        EntityUmvuthi[] puppet = new EntityUmvuthi[1];
        Mob[] warrior = new Mob[1];
        long[] off = new long[2], gone = new long[2]; // the puppet's, the warrior's: the tick its chunk stopped ticking, the tick it was removed
        long[] was = new long[3]; // requeued, chiefsKilled, casualties
        boolean[] stepped = {false};
        Vec3[] at = new Vec3[1];

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    g.declare();
                    w.start();
                })
                .thenWaitUntil(waits.chunks(w::check))
                .thenExecute(() -> {
                    w.ground();
                    MowzieKit.mask(w.player);
                    check("the grove is out: no epoch, no census (" + g.site().epoch() + ", " + MowzieKit.censusCount(g.faction) + ")",
                            g.site().epoch() == 0L && MowzieKit.censusCount(g.faction) == 0);
                    int gy = level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, (int) Math.floor(w.px + 132), (int) Math.floor(w.pz));
                    was[0] = ConflictKit.bridge().requeued;
                    was[1] = ConflictKit.bridge().chiefsKilled;
                    was[2] = ConflictKit.bridge().casualties;
                    bandAt(helper, g.faction, agent, w.px + 132, gy, w.pz, 0f, List.of(), 1, true);
                })
                .thenWaitUntil(waits.logic(() -> {
                    puppet[0] = puppet(agent);
                    check("the puppet floats", puppet[0].isNoGravity());
                    List<Mob> ws = warriorsOf(agent);
                    check("its warrior is embodied (" + ws.size() + ")", ws.size() == 1);
                    warrior[0] = ws.get(0);
                }))
                .thenExecute(() -> {
                    at[0] = puppet[0].position();
                    check("both stand in chunks that tick entities", RtsimSession.entitiesTickAt(level, puppet[0].blockPosition())
                            && RtsimSession.entitiesTickAt(level, warrior[0].blockPosition()));
                    double before = Math.hypot(w.player.getX() - at[0].x, w.player.getZ() - at[0].z);
                    check("the player is within the hold's range of the puppet (" + before + ")", before < GroveKind.HOLD_RANGE);
                    check("the grove's rule holds the puppet", GroveKind.INSTANCE.holdRemoval(level, g.site(), puppet[0]));
                    check("the edge is past the puppet's chunk now (edge " + w.edge() + ", puppet " + at[0].x + ")", w.edge() > at[0].x);
                    MowzieKit.everyTick(helper, () -> {
                        if (!stepped[0])
                            return;
                        long now = level.getGameTime();
                        Mob[] es = {puppet[0], warrior[0]};
                        for (int i = 0; i < 2; i++) {
                            if (off[i] == 0 && !RtsimSession.entitiesTickAt(level, es[i].blockPosition()))
                                off[i] = now;
                            if (gone[i] == 0 && es[i].isRemoved())
                                gone[i] = now;
                        }
                    });
                    stepped[0] = true;
                    w.step(-20);
                    double after = Math.hypot(w.player.getX() - at[0].x, w.player.getZ() - at[0].z);
                    check("the player's chunk moved, and the edge with it (edge " + w.edge() + ", puppet " + at[0].x + ")", w.edge() < at[0].x);
                    check("it is still within the hold's range (" + after + ")", after < GroveKind.HOLD_RANGE);
                    check("and the grove's rule still holds the puppet", GroveKind.INSTANCE.holdRemoval(level, g.site(), puppet[0]));
                })
                .thenWaitUntil(waits.logic(() -> check("their chunks stopped ticking entities (" + off[0] + ", " + off[1] + ")", off[0] != 0 && off[1] != 0)))
                .thenIdle(6)
                .thenExecute(() -> {
                    check("the puppet was taken back (its chunk stopped at " + off[0] + ", gone " + gone[0] + ", states " + stateOf(agent) + ")", gone[0] != 0);
                    check("so was the warrior, the control (" + gone[1] + ")", gone[1] != 0);
                    check("the puppet went within 2 ticks of its chunk's stopping (" + (gone[0] - off[0]) + ")", gone[0] - off[0] >= 0 && gone[0] - off[0] <= 2);
                    check("the warrior's chunk stopped with the puppet's (" + (off[1] - off[0]) + ")", Math.abs(off[1] - off[0]) <= 1);
                    check("the warrior went within 2 ticks of its chunk's stopping (" + (gone[1] - off[1]) + ")", gone[1] - off[1] >= 0 && gone[1] - off[1] <= 2);
                    check("the puppet went no later than a tick after the control (" + (gone[0] - gone[1]) + ")", gone[0] - gone[1] <= 1);
                    check("two stacks went back to pending: requeued rose by " + (ConflictKit.bridge().requeued - was[0]), ConflictKit.bridge().requeued == was[0] + 2);
                    for (ConflictBridge.Stack st : ConflictKit.party(agent).stacks())
                        check("a stack is pending (" + stateOf(agent) + ")", st.state() == ConflictBridge.StackState.PENDING);
                    check("the chief's slot is alive", chiefSlot(agent).alive());
                    check("the party is not recalled", !ConflictKit.party(agent).recalling());
                    AABB wide = new AABB(at[0].x - 200, level.getMinBuildHeight(), at[0].z - 200, at[0].x + 200, level.getMaxBuildHeight(), at[0].z + 200);
                    List<EntityUmvuthi> left = level.getEntitiesOfClass(EntityUmvuthi.class, wide);
                    check("no Umvuthi stands in the loaded chunks within 200 blocks (" + left.size() + ")", left.isEmpty());
                    check("no CHIEF_KILLED (" + ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent).size() + ")", ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent).isEmpty());
                    check("no casualty (" + ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent).size() + ")", ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent).isEmpty());
                    check("chiefsKilled and casualties are as they were", ConflictKit.bridge().chiefsKilled == was[1] && ConflictKit.bridge().casualties == was[2]);
                    record(test, "edge", "the edge went from " + (w.edge() + 20) + " to " + w.edge() + ", the puppet at " + String.format("%.1f", at[0].x) + " went "
                            + (gone[0] - off[0]) + " ticks after its chunk stopped, the warrior " + (gone[1] - off[1]) + "; requeued +" + (ConflictKit.bridge().requeued - was[0]));
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    AABB wide = new AABB(at[0].x - 200, level.getMinBuildHeight(), at[0].z - 200, at[0].x + 200, level.getMaxBuildHeight(), at[0].z + 200);
                    check("still no Umvuthi 40 ticks on (" + level.getEntitiesOfClass(EntityUmvuthi.class, wide).size() + ")", level.getEntitiesOfClass(EntityUmvuthi.class, wide).isEmpty());
                    check("the stacks are still pending (" + stateOf(agent) + ")", ConflictKit.party(agent).stacks().stream().allMatch(st -> st.state() == ConflictBridge.StackState.PENDING));
                    recallAll(agent);
                })
                .thenWaitUntil(waits.logic(() -> check("the band went (" + dematerialized(agent).size() + ")", dematerialized(agent).size() >= 1)))
                .thenExecute(() -> {
                    w.end();
                    far.releaseAll();
                })
                .thenSucceed();
    }

    // ---- 29c-seam: an edge stop's recall reaches the puppet at its timeout (MZB-827) ---------------------

    /** Ticks a recall of this test lasts before it reaches a puppet Rule U holds. */
    private static final int SEAM_TIMEOUT = 400;

    /**
     * An edge stop's recall (BAND_RECALL, RECALL_PASSING) does not despawn a puppet Rule U holds until the recall's own timeout (test
     * 29c, the seam; MZB-827). Two bands at view distance 8 (sight 128 blocks), each a puppet and a warrior on a lane of forced
     * chunks, the grove of its faction declared and out. Band A has a masked player 140 blocks off (out of sight, within 160: Rule U
     * holds the puppet); band B has one 170 blocks off (out of sight, past the range: nothing holds it). Both are sent the same
     * record, {@value #SEAM_TIMEOUT} ticks.
     * <ul>
     * <li>Band B (the control): the puppet and the warrior go at once (within 6 ticks of the recall's tick); BAND_DEMATERIALIZED follows.</li>
     * <li>Band A: the warrior goes at once (unseen), the puppet stands for the timeout: alive 300 ticks on, no BAND_DEMATERIALIZED,
     * its slot alive. It goes at the timeout, within 3 ticks before to 8 after the {@value #SEAM_TIMEOUT}th tick from the recall's
     * (the record's own timeout, not {@link ConflictBridge#RECALL_TIMEOUT}), and one BAND_DEMATERIALIZED follows it.</li>
     * </ul>
     * Fails without MZB-827 (band A's puppet goes at once, out of sight).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_seam", skyAccess = true)
    public static void anEdgeStopRecallReachesThePuppetAtItsTimeout(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 3000);
        String test = "anEdgeStopRecallReachesThePuppetAtItsTimeout";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove[] g = {groveAt(far, 0, 0), groveAt(far, 0, 1)}; // declared, never forced: out
        Lane[] lane = {new Lane(far, far.k + 6, 0, 2), new Lane(far, far.k + 6, 1, 2)};
        long[] agent = {ConflictKit.newAgent(), ConflictKit.newAgent()};
        EntityUmvuthi[] puppet = new EntityUmvuthi[2];
        Mob[] warrior = new Mob[2];
        ServerPlayer[] player = new ServerPlayer[2];
        long[] recalledAt = new long[2], goneAt = new long[2], warriorGoneAt = new long[2];
        boolean[] sampling = {false};
        Runnable[] back = new Runnable[1];

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    for (Lane l : lane)
                        l.force();
                    g[0].declare();
                    g[1].declare();
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    back[0] = viewDistance(helper, level, 8);
                    for (int i = 0; i < 2; i++) {
                        lane[i].floor();
                        check("the grove " + i + " is out (" + g[i].site().epoch() + ")", g[i].site().epoch() == 0L);
                        bandAt(helper, g[i].faction, agent[i], lane[i].x0 + 20, lane[i].top + 1, lane[i].z, 0f, List.of(), 1, true);
                    }
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < 2; i++) {
                        puppet[i] = puppet(agent[i]);
                        List<Mob> ws = warriorsOf(agent[i]);
                        check("band " + i + "'s warrior is embodied (" + ws.size() + ")", ws.size() == 1);
                        warrior[i] = ws.get(0);
                    }
                }))
                .thenExecute(() -> {
                    double[] range = {140, 170};
                    for (int i = 0; i < 2; i++) {
                        check("the puppet " + i + " floats", puppet[i].isNoGravity());
                        player[i] = MowzieKit.player(helper, new Vec3(puppet[i].getX(), lane[i].top + 1, puppet[i].getZ() + range[i]), "mowzie-seam-" + i);
                        MowzieKit.mask(player[i]);
                    }
                    double sight = ConflictBridge.sightRange(puppet[0], level);
                    check("sight is 128 blocks at view distance 8 (" + sight + ")", sight == 128.0);
                    check("band A's player is out of sight and within the hold's range (140)", 140 > sight && 140 < GroveKind.HOLD_RANGE);
                    check("the grove's rule holds band A's puppet", GroveKind.INSTANCE.holdRemoval(level, g[0].site(), puppet[0]));
                    check("and not band B's (its player is 170 off)", !GroveKind.INSTANCE.holdRemoval(level, g[1].site(), puppet[1]));
                    for (ServerPlayer p : level.players())
                        for (int i = 0; i < 2; i++)
                            if (p != player[i] && Math.hypot(p.getX() - puppet[i].getX(), p.getZ() - puppet[i].getZ()) < GroveKind.HOLD_RANGE + 1)
                                check("no other player is near puppet " + i + " (" + p.getGameProfile().getName() + ")", false);
                    MowzieKit.everyTick(helper, () -> {
                        if (!sampling[0])
                            return;
                        long now = level.getGameTime();
                        for (int i = 0; i < 2; i++) {
                            ConflictBridge.Party p = ConflictKit.bridge().party(agent[i]);
                            if (recalledAt[i] == 0 && p != null && p.recalling())
                                recalledAt[i] = now;
                            if (goneAt[i] == 0 && puppet[i].isRemoved())
                                goneAt[i] = now;
                            if (warriorGoneAt[i] == 0 && warrior[i].isRemoved())
                                warriorGoneAt[i] = now;
                        }
                    });
                    sampling[0] = true;
                    for (int i = 0; i < 2; i++)
                        ConflictKit.recall(agent[i], ConflictKit.party(agent[i]).epoch, ConflictRecords.RECALL_PASSING, SEAM_TIMEOUT, g[i].spot.x, g[i].spot.z);
                })
                .thenWaitUntil(waits.logic(() -> check("the recalls came and the control's puppet went (" + recalledAt[0] + ", " + recalledAt[1] + ", " + goneAt[1] + ")",
                        recalledAt[0] != 0 && recalledAt[1] != 0 && goneAt[1] != 0)))
                .thenIdle(5)
                .thenExecute(() -> {
                    check("band B's puppet went at once (" + (goneAt[1] - recalledAt[1]) + " ticks)", goneAt[1] - recalledAt[1] <= 6);
                    check("so did its warrior (" + (warriorGoneAt[1] - recalledAt[1]) + ")", warriorGoneAt[1] != 0 && warriorGoneAt[1] - recalledAt[1] <= 6);
                    check("band A's warrior went at once (" + (warriorGoneAt[0] - recalledAt[0]) + ")", warriorGoneAt[0] != 0 && warriorGoneAt[0] - recalledAt[0] <= 6);
                    check("band A's puppet stands (alive " + puppet[0].isAlive() + ", removed " + puppet[0].isRemoved() + ")", puppet[0].isAlive() && !puppet[0].isRemoved());
                    check("band A is not over (" + dematerialized(agent[0]).size() + ")", dematerialized(agent[0]).isEmpty());
                })
                .thenWaitUntil(waits.logic(() -> check("band B went (" + dematerialized(agent[1]).size() + ")", dematerialized(agent[1]).size() == 1)))
                .thenIdle(SEAM_TIMEOUT - 100)
                .thenExecute(() -> {
                    long now = level.getGameTime();
                    check("band A's puppet stands " + (now - recalledAt[0]) + " ticks on (removed " + puppet[0].isRemoved() + ")", !puppet[0].isRemoved() && puppet[0].isAlive());
                    check("its slot is alive", chiefSlot(agent[0]).alive());
                    check("band A is not over (" + dematerialized(agent[0]).size() + ")", dematerialized(agent[0]).isEmpty());
                    check("no CHIEF_KILLED for either (" + ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[0]).size() + ")",
                            ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[0]).isEmpty() && ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[1]).isEmpty());
                })
                .thenWaitUntil(waits.logic(() -> check("band A's puppet went (removed " + puppet[0].isRemoved() + ")", goneAt[0] != 0)))
                .thenExecute(() -> {
                    long held = goneAt[0] - recalledAt[0];
                    check("it went at the record's timeout, " + SEAM_TIMEOUT + " ticks on, within 3 before and 8 after (" + held + ")",
                            held >= SEAM_TIMEOUT - 3 && held <= SEAM_TIMEOUT + 8);
                    record(test, "seam", "band B (player 170 off) lost its puppet " + (goneAt[1] - recalledAt[1]) + " ticks after the recall; band A (player 140 off) "
                            + held + " ticks after, at the timeout of " + SEAM_TIMEOUT);
                })
                .thenWaitUntil(waits.logic(() -> check("band A went (" + dematerialized(agent[0]).size() + ")", dematerialized(agent[0]).size() == 1)))
                .thenExecute(() -> {
                    check("one BAND_DEMATERIALIZED each (" + dematerialized(agent[0]).size() + ", " + dematerialized(agent[1]).size() + ")",
                            dematerialized(agent[0]).size() == 1 && dematerialized(agent[1]).size() == 1);
                    check("no second CHIEF_KILLED, no casualty",
                            ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent[0]).isEmpty() && ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent[1]).isEmpty());
                    back[0].run();
                    drop(player);
                    far.releaseAll();
                })
                .thenSucceed();
    }

    // ---- 29c-failed: a failed session captures every puppet (MZB-817, MZB-882) ----------------------------

    /**
     * A failed session takes every puppet out, finishes the dying chief's loot, and floats nothing on (test 29c, the failed session;
     * MZB-817, MZB-882). Three bands on three lanes, each a puppet and a warrior (the control): one aloft, one settled before a masked
     * player, and one killed by a masked player 20 ticks before. The server's engine is made to fail ({@code failForTest}, as
     * {@code EngineFailureGameTests} does on a session of its own), and the session's end of the tick stops the bridge for the session:
     * the puppets are captured and the plugin's {@code sessionFailed} runs.
     * <ul>
     * <li>Before: the dead chief's CHIEF_KILLED is journaled, once, naming its killer; its body is dying and no Sol Visage lies yet.</li>
     * <li>After: the session has failed; both live puppets and the three warriors are removed; no Umvuthi is left in any lane (so none
     * floats); no party is held by the bridge; the dying body is finished: one Sol Visage lies where it fell, nothing is dying, no loot
     * record was lost, and no second CHIEF_KILLED was journaled.</li>
     * <li>The session is started again ({@code restartGlobalForTest}): a new session, not failed, with an engine of its own and none of
     * these parties. Nothing of the failed one outlives it.</li>
     * </ul>
     * Fails without the Dying list's finish in {@code sessionFailed} (the dying body is discarded with no drop).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_failed", skyAccess = true)
    public static void aFailedSessionCapturesEveryPuppet(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 4000);
        String test = "aFailedSessionCapturesEveryPuppet";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Lane[] lane = {new Lane(far, 0, 2), new Lane(far, 1, 2), new Lane(far, 2, 2)}; // aloft, settled, dying
        long[] agent = {ConflictKit.newAgent(), ConflictKit.newAgent(), ConflictKit.newAgent()};
        long[] faction = {ConflictKit.newFaction(), ConflictKit.newFaction(), ConflictKit.newFaction()};
        EntityUmvuthi[] puppet = new EntityUmvuthi[3];
        List<List<Mob>> warriors = new ArrayList<>();
        ServerPlayer[] player = new ServerPlayer[3]; // [0] stays out; [1] the settled puppet's target; [2] the killer
        Vec3[] fell = new Vec3[1];
        long[] was = new long[3]; // chiefsKilled, deathLootLost, the tick of the failure order
        RtsimSession[] old = new RtsimSession[1];
        ScriptedEngine[] oldEngine = new ScriptedEngine[1];
        boolean[] target = {true};

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    for (Lane l : lane)
                        l.force();
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    for (int i = 0; i < 3; i++) {
                        lane[i].floor();
                        bandAt(helper, faction[i], agent[i], lane[i].x0 + 8, lane[i].top + 1, lane[i].z, 0f, List.of(), 1, true);
                    }
                    player[1] = MowzieKit.player(helper, lane[1].at(18), "mowzie-failed-target");
                    player[2] = MowzieKit.player(helper, lane[2].at(28), "mowzie-failed-killer");
                    MowzieKit.mask(player[1]);
                    MowzieKit.mask(player[2]);
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < 3; i++) {
                        puppet[i] = puppet(agent[i]);
                        check("band " + i + "'s warrior is embodied (" + warriorsOf(agent[i]).size() + ")", warriorsOf(agent[i]).size() == 1);
                    }
                }))
                .thenExecute(() -> {
                    for (int i = 0; i < 3; i++)
                        warriors.add(warriorsOf(agent[i]));
                    check("the first puppet floats", puppet[0].isNoGravity());
                    // the second settles before its player: the jar's own AI is off and the target is set by hand
                    puppet[1].setNoAi(true);
                    puppet[1].getAttribute(Attributes.FOLLOW_RANGE).setBaseValue(1000);
                    MowzieKit.everyTick(helper, () -> {
                        if (target[0] && puppet[1].isAlive() && !puppet[1].isRemoved())
                            puppet[1].setTarget(player[1]);
                    });
                })
                .thenWaitUntil(waits.logic(() -> check("the second puppet has settled before its player (gravity " + !puppet[1].isNoGravity() + ")", !puppet[1].isNoGravity())))
                .thenExecute(() -> {
                    was[0] = ConflictKit.bridge().chiefsKilled;
                    was[1] = ConflictKit.deathLootLost();
                    fell[0] = puppet[2].position();
                    puppet[2].setLastHurtByPlayer(player[2]);
                    MowzieKit.kill(puppet[2], level.damageSources().playerAttack(player[2]));
                })
                .thenIdle(20)
                .thenWaitUntil(waits.logic(() -> check("the dead chief's CHIEF_KILLED is journaled (" + ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[2]).size() + ")",
                        ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[2]).size() == 1)))
                .thenExecute(() -> {
                    ScriptedEngine.Received k = ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[2]).get(0);
                    check("by a player, naming the killer", k.aux() == ConflictRecords.BY_PLAYER
                            && player[2].getUUID().equals(ConflictKit.uuidAt(k, ConflictRecords.ChiefKilled.KILLER_MOST)));
                    check("chiefsKilled rose by one (" + was[0] + " to " + ConflictKit.bridge().chiefsKilled + ")", ConflictKit.bridge().chiefsKilled == was[0] + 1);
                    check("its body is dying (" + ConflictKit.bridge().dyingBodies() + ")", ConflictKit.bridge().dyingBodies() == 1);
                    check("no Sol Visage yet (" + visagesNear(level, fell[0], 6) + ")", visagesNear(level, fell[0], 6) == 0);
                    check("the body is still in the world (removed " + puppet[2].isRemoved() + ")", !puppet[2].isRemoved());
                    for (int i = 0; i < 2; i++)
                        check("band " + i + "'s puppet stands (alive " + puppet[i].isAlive() + ")", puppet[i].isAlive() && !puppet[i].isRemoved());
                    for (List<Mob> ws : warriors)
                        check("a warrior stands", ws.get(0).isAlive() && !ws.get(0).isRemoved());
                    old[0] = RtsimTests.session();
                    oldEngine[0] = RtsimTests.engine();
                    check("the session runs before the failure", !old[0].failed());
                    was[2] = level.getGameTime();
                    oldEngine[0].failForTest();
                })
                .thenWaitUntil(waits.logic(() -> check("the session failed", old[0].failed())))
                .thenExecute(() -> {
                    long ticks = level.getGameTime() - was[2];
                    check("the dying chief's body is finished: removed", puppet[2].isRemoved());
                    check("one Sol Visage lies where it fell (" + visagesNear(level, fell[0], 6) + ")", visagesNear(level, fell[0], 6) == 1);
                    check("nothing is dying (" + ConflictKit.bridge().dyingBodies() + ")", ConflictKit.bridge().dyingBodies() == 0);
                    check("no loot record was lost (" + was[1] + " to " + ConflictKit.deathLootLost() + ")", ConflictKit.deathLootLost() == was[1]);
                    for (int i = 0; i < 2; i++)
                        check("band " + i + "'s puppet is removed", puppet[i].isRemoved());
                    for (int i = 0; i < 3; i++) {
                        check("band " + i + "'s warrior is removed (the control)", warriors.get(i).get(0).isRemoved());
                        check("band " + i + " is not held by the bridge", ConflictKit.bridge().party(agent[i]) == null);
                    }
                    int floating = 0, standing = 0;
                    for (Lane l : lane)
                        for (EntityUmvuthi u : level.getEntitiesOfClass(EntityUmvuthi.class, l.area())) {
                            standing++;
                            if (u.isNoGravity())
                                floating++;
                        }
                    check("no Umvuthi stands in any lane (" + standing + ", " + floating + " floating)", standing == 0);
                    check("one CHIEF_KILLED in all (" + ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[2]).size() + ")",
                            ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[2]).size() == 1
                                    && ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[0]).isEmpty() && ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[1]).isEmpty());
                    check("chiefsKilled is as it was after the one death (" + ConflictKit.bridge().chiefsKilled + ")", ConflictKit.bridge().chiefsKilled == was[0] + 1);
                    record(test, "failed", "the session failed " + ticks + " ticks after the order; the aloft, the settled and the dying chief's bodies and their 3 warriors are out, one Sol Visage lies at the dying one");
                    target[0] = false;
                    RtsimSession.restartGlobalForTest(level.getServer());
                    check("a session runs after the restart: " + RtsimSession.startFailure(), RtsimSession.current() != null);
                    check("it is a new session, and it has not failed", RtsimSession.current() != old[0] && !RtsimSession.current().failed());
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    RtsimSession now = RtsimTests.session();
                    check("the new session still runs (failed " + now.failed() + ")", !now.failed());
                    check("with an engine of its own", RtsimTests.engine() != oldEngine[0]);
                    for (int i = 0; i < 3; i++)
                        check("none of the old parties is held (" + i + ")", ConflictKit.bridge().party(agent[i]) == null);
                    int standing = 0;
                    for (Lane l : lane)
                        standing += level.getEntitiesOfClass(EntityUmvuthi.class, l.area()).size();
                    check("no Umvuthi stands in any lane after the restart (" + standing + ")", standing == 0);
                    drop(player);
                    far.releaseAll();
                })
                .thenSucceed();
    }

    // ---- 29c-home: a homecoming with its grove out is recalled (MZB-824, MZB-827) -------------------------

    /**
     * A band the engine has ended whose grove is out is recalled with RECALL_DONE, not handed back, and its puppet stays under Rule U until
     * the recall's timeout (test 29c, the homecoming with the grove out; MZB-824, MZB-827). Two bands, H1 and H2, on lanes of forced
     * chunks 100 blocks and more east of their groves (declared, and not loaded: out, no census, no epoch). Each is a puppet and a warrior,
     * with a masked player 140 blocks off (out of sight at view distance 8, within the hold's range 160). The engine's march of each is
     * ENDED and its BAND_RECALL (RECALL_DONE, no timeout, so {@link ConflictBridge#RECALL_TIMEOUT}) names the throne's spot; the band's
     * speed is 0, so nothing walks.
     * <ul>
     * <li>H1: the warrior goes at once; the puppet stands until the timeout (alive 300 ticks on, and at 1,000), goes within 2 ticks
     * of {@code RECALL_TIMEOUT} after the recall, and one BAND_DEMATERIALIZED follows; no CHIEF_KILLED.</li>
     * <li>H2: its player kills the puppet 300 ticks into the window: one CHIEF_KILLED by that player, one Sol Visage, and
     * {@code chiefDeathsAfterEnd} rises by one (the death of a chief of an ended march with no HAND_BACK held, which comes back at the
     * next census, MZB-824); BAND_DEMATERIALIZED follows the CHIEF_KILLED in the journal.</li>
     * </ul>
     * The control, a grove that is in, sends HAND_BACKs and no recall: test 32t. Fails without MZB-827 (the puppet goes at the recall, out
     * of sight, and H2 has nothing to kill).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_home", skyAccess = true)
    public static void aHomecomingWithItsGroveOutIsRecalled(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 4000);
        String test = "aHomecomingWithItsGroveOutIsRecalled";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        String dim = RtsimTests.dimension(helper);
        Grove[] g = {groveAt(far, 0, 0), groveAt(far, 0, 1)}; // H1 and H2: declared, never forced
        Lane[] lane = {new Lane(far, far.k + 6, 0, 3), new Lane(far, far.k + 6, 1, 3)};
        long[] agent = {ConflictKit.newAgent(), ConflictKit.newAgent()};
        EntityUmvuthi[] puppet = new EntityUmvuthi[2];
        Mob[] warrior = new Mob[2];
        ServerPlayer[] player = new ServerPlayer[2];
        long[] recalledAt = new long[2], goneAt = new long[2], warriorGoneAt = new long[2];
        long[] was = new long[3]; // chiefsKilled, chiefDeathsAfterEnd, deathLootLost
        Vec3[] fell = new Vec3[1];
        boolean[] sampling = {false};
        Runnable[] back = new Runnable[1];

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    for (Lane l : lane)
                        l.force();
                    g[0].declare();
                    g[1].declare();
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    back[0] = viewDistance(helper, level, 8);
                    for (int i = 0; i < 2; i++) {
                        lane[i].floor();
                        bandAt(helper, g[i].faction, agent[i], lane[i].x0 + 24, lane[i].top + 1, lane[i].z, 0f, List.of(), 1, true);
                    }
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < 2; i++) {
                        puppet[i] = puppet(agent[i]);
                        List<Mob> ws = warriorsOf(agent[i]);
                        check("band " + i + "'s warrior is embodied (" + ws.size() + ")", ws.size() == 1);
                        warrior[i] = ws.get(0);
                    }
                }))
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++) {
                        check("grove " + i + " is out: no epoch, no census (" + g[i].site().epoch() + ", " + MowzieKit.censusCount(g[i].faction) + ")",
                                g[i].site().epoch() == 0L && MowzieKit.censusCount(g[i].faction) == 0);
                        check("the puppet " + i + " floats", puppet[i].isNoGravity());
                        player[i] = MowzieKit.player(helper, new Vec3(puppet[i].getX(), lane[i].top + 1, puppet[i].getZ() + 140), "mowzie-home-" + i);
                        MowzieKit.mask(player[i]);
                        check("the grove's rule holds puppet " + i, GroveKind.INSTANCE.holdRemoval(level, g[i].site(), puppet[i]));
                    }
                    double sight = ConflictBridge.sightRange(puppet[0], level);
                    check("sight is 128 blocks at view distance 8 (" + sight + ")", sight == 128.0);
                    for (ServerPlayer p : level.players())
                        for (int i = 0; i < 2; i++)
                            if (p != player[i] && Math.hypot(p.getX() - puppet[i].getX(), p.getZ() - puppet[i].getZ()) < GroveKind.HOLD_RANGE + 1)
                                check("no other player is near puppet " + i + " (" + p.getGameProfile().getName() + ")", false);
                    was[0] = ConflictKit.bridge().chiefsKilled;
                    was[1] = ConflictKit.chiefDeathsAfterEnd();
                    was[2] = ConflictKit.deathLootLost();
                    MowzieKit.everyTick(helper, () -> {
                        if (!sampling[0])
                            return;
                        long now = level.getGameTime();
                        for (int i = 0; i < 2; i++) {
                            ConflictBridge.Party p = ConflictKit.bridge().party(agent[i]);
                            if (recalledAt[i] == 0 && p != null && p.recalling())
                                recalledAt[i] = now;
                            if (goneAt[i] == 0 && puppet[i].isRemoved())
                                goneAt[i] = now;
                            if (warriorGoneAt[i] == 0 && warrior[i].isRemoved())
                                warriorGoneAt[i] = now;
                        }
                    });
                    sampling[0] = true;
                    ConflictKit.together(() -> {
                        for (int i = 0; i < 2; i++) {
                            marchEnded(agent[i], g[i].faction, dim, g[i].spot.x, g[i].spot.z);
                            ConflictKit.recall(agent[i], ConflictKit.party(agent[i]).epoch, ConflictRecords.RECALL_DONE, 0, g[i].spot.x, g[i].spot.z);
                        }
                    });
                })
                .thenWaitUntil(waits.logic(() -> check("both bands are recalled (" + recalledAt[0] + ", " + recalledAt[1] + ")", recalledAt[0] != 0 && recalledAt[1] != 0)))
                .thenIdle(5)
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++) {
                        check("band " + i + "'s warrior went at once (" + (warriorGoneAt[i] - recalledAt[i]) + ")", warriorGoneAt[i] != 0 && warriorGoneAt[i] - recalledAt[i] <= 6);
                        check("band " + i + "'s puppet stands", puppet[i].isAlive() && !puppet[i].isRemoved());
                        check("band " + i + " is not over (" + dematerialized(agent[i]).size() + ")", dematerialized(agent[i]).isEmpty());
                    }
                })
                .thenIdle(295)
                .thenExecute(() -> {
                    long now = level.getGameTime();
                    for (int i = 0; i < 2; i++)
                        check("puppet " + i + " stands " + (now - recalledAt[i]) + " ticks into the window", puppet[i].isAlive() && !puppet[i].isRemoved() && chiefSlot(agent[i]).alive());
                    // H2's player kills it
                    fell[0] = puppet[1].position();
                    puppet[1].setLastHurtByPlayer(player[1]);
                    MowzieKit.kill(puppet[1], level.damageSources().playerAttack(player[1]));
                })
                .thenWaitUntil(waits.logic(() -> check("H2's CHIEF_KILLED is journaled (" + ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[1]).size() + ")",
                        ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[1]).size() == 1)))
                .thenWaitUntil(waits.logic(() -> check("H2's body has ended its animation (removed " + puppet[1].isRemoved() + ")", puppet[1].isRemoved())))
                .thenWaitUntil(waits.logic(() -> check("H2's band went (" + dematerialized(agent[1]).size() + ")", dematerialized(agent[1]).size() == 1)))
                .thenExecute(() -> {
                    ScriptedEngine.Received k = ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[1]).get(0);
                    check("H2's CHIEF_KILLED is by a player and names it", k.aux() == ConflictRecords.BY_PLAYER
                            && player[1].getUUID().equals(ConflictKit.uuidAt(k, ConflictRecords.ChiefKilled.KILLER_MOST)));
                    check("it came before the band's BAND_DEMATERIALIZED (obsSeq " + k.obsSeq() + " and " + dematerialized(agent[1]).get(0).obsSeq() + ")",
                            k.obsSeq() < dematerialized(agent[1]).get(0).obsSeq());
                    check("chiefsKilled rose by one (" + was[0] + " to " + ConflictKit.bridge().chiefsKilled + ")", ConflictKit.bridge().chiefsKilled == was[0] + 1);
                    check("chiefDeathsAfterEnd rose by one (" + was[1] + " to " + ConflictKit.chiefDeathsAfterEnd() + ")", ConflictKit.chiefDeathsAfterEnd() == was[1] + 1);
                    check("one Sol Visage where H2's puppet fell (" + visagesNear(level, fell[0], 6) + ")", visagesNear(level, fell[0], 6) == 1);
                    check("no loot record was lost (" + was[2] + " to " + ConflictKit.deathLootLost() + ")", ConflictKit.deathLootLost() == was[2]);
                    check("H1's puppet still stands (" + puppet[0].isRemoved() + ")", puppet[0].isAlive() && !puppet[0].isRemoved());
                    check("H1 is not over", dematerialized(agent[0]).isEmpty());
                })
                .thenWaitUntil(waits.logic(() -> check("H1's puppet went (removed " + puppet[0].isRemoved() + ")", goneAt[0] != 0)))
                .thenExecute(() -> {
                    long held = goneAt[0] - recalledAt[0];
                    check("H1's puppet went at RECALL_TIMEOUT (" + ConflictBridge.RECALL_TIMEOUT + "), within 3 ticks before and 8 after (" + held + ")",
                            held >= ConflictBridge.RECALL_TIMEOUT - 3 && held <= ConflictBridge.RECALL_TIMEOUT + 8);
                    record(test, "home", "H1's puppet (player 140 off, grove out) went " + held + " ticks after the recall; H2's was killed 300 ticks in: CHIEF_KILLED once, chiefDeathsAfterEnd +"
                            + (ConflictKit.chiefDeathsAfterEnd() - was[1]));
                })
                .thenWaitUntil(waits.logic(() -> check("H1's band went (" + dematerialized(agent[0]).size() + ")", dematerialized(agent[0]).size() == 1)))
                .thenExecute(() -> {
                    check("no CHIEF_KILLED for H1 (" + ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[0]).size() + ")", ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[0]).isEmpty());
                    check("chiefDeathsAfterEnd is still one more (" + ConflictKit.chiefDeathsAfterEnd() + ")", ConflictKit.chiefDeathsAfterEnd() == was[1] + 1);
                    check("neither band has a casualty", ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent[0]).isEmpty() && ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent[1]).isEmpty());
                    back[0].run();
                    drop(player);
                    far.releaseAll();
                })
                .thenSucceed();
    }

    // ---- the invariant's own test: the death is journaled before the band is (MZB-824, MZB-882) ----------

    /**
     * A band's BAND_DEMATERIALIZED never comes before its chief's death is journaled (the lead's invariant; MZB-882). Two bands, each a
     * puppet and a warrior, on lanes, with a masked killer 20 blocks off. Band X's chief is killed, and 20 ticks later band Y's is
     * killed in the tick both bands are recalled (BAND_RECALL, RECALL_DONE, 20 ticks): X's chief died 20 ticks before its recall, Y's
     * in its tick. For each band:
     * <ul>
     * <li>one CHIEF_KILLED by the killer and one BAND_DEMATERIALIZED, and the CHIEF_KILLED has the lower observation sequence in the
     * journal (the engine's received log, by {@code obsSeq});</li>
     * <li>no second CHIEF_KILLED, no casualty; {@code chiefsKilled} rose by two in all.</li>
     * </ul>
     * After the animations one Sol Visage lies at each body, nothing is dying and no loot record was lost: the band's end does not wait
     * for the animation, only for the death's journal. There is no engine command that makes BAND_DEMATERIALIZED (it is the bridge's own
     * observation), so the recall is the engine's record that ends the band. Fails if a recall's finish went before {@code settleDeaths}.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_invariant", skyAccess = true)
    public static void aRecallAfterTheChiefsDeathJournalsTheDeathFirst(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 4000);
        String test = "aRecallAfterTheChiefsDeathJournalsTheDeathFirst";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Lane[] lane = {new Lane(far, 0, 2), new Lane(far, 1, 2)}; // X, Y
        long[] agent = {ConflictKit.newAgent(), ConflictKit.newAgent()};
        long[] faction = {ConflictKit.newFaction(), ConflictKit.newFaction()};
        EntityUmvuthi[] puppet = new EntityUmvuthi[2];
        ServerPlayer[] player = new ServerPlayer[2];
        Vec3[] fell = new Vec3[2];
        long[] was = new long[2]; // chiefsKilled, deathLootLost

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    for (Lane l : lane)
                        l.force();
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++) {
                        lane[i].floor();
                        bandAt(helper, faction[i], agent[i], lane[i].x0 + 8, lane[i].top + 1, lane[i].z, 0f, List.of(), 1, true);
                        player[i] = MowzieKit.player(helper, lane[i].at(28), "mowzie-invariant-" + i);
                        MowzieKit.mask(player[i]);
                    }
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < 2; i++)
                        puppet[i] = puppet(agent[i]);
                }))
                .thenExecute(() -> {
                    was[0] = ConflictKit.bridge().chiefsKilled;
                    was[1] = ConflictKit.deathLootLost();
                    fell[0] = puppet[0].position();
                    puppet[0].setLastHurtByPlayer(player[0]);
                    MowzieKit.kill(puppet[0], level.damageSources().playerAttack(player[0]));
                })
                .thenIdle(20)
                .thenExecute(() -> {
                    check("X's chief died 20 ticks ago and its animation is not over (" + puppet[0].isRemoved() + ")", !puppet[0].isRemoved());
                    fell[1] = puppet[1].position();
                    puppet[1].setLastHurtByPlayer(player[1]);
                    MowzieKit.kill(puppet[1], level.damageSources().playerAttack(player[1]));
                    for (int i = 0; i < 2; i++)
                        ConflictKit.recall(agent[i], ConflictKit.party(agent[i]).epoch, ConflictRecords.RECALL_DONE, 20, lane[i].x0, lane[i].z);
                })
                .thenWaitUntil(waits.logic(() -> check("both bands went (" + dematerialized(agent[0]).size() + ", " + dematerialized(agent[1]).size() + ")",
                        dematerialized(agent[0]).size() >= 1 && dematerialized(agent[1]).size() >= 1)))
                .thenWaitUntil(waits.logic(() -> check("both CHIEF_KILLED are journaled (" + ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[0]).size() + ", "
                        + ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[1]).size() + ")",
                        ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[0]).size() >= 1 && ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[1]).size() >= 1)))
                .thenIdle(10)
                .thenExecute(() -> {
                    String[] name = {"X", "Y"};
                    for (int i = 0; i < 2; i++) {
                        List<ScriptedEngine.Received> killed = ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[i]);
                        List<ScriptedEngine.Received> over = dematerialized(agent[i]);
                        check(name[i] + ": one CHIEF_KILLED (" + killed.size() + ")", killed.size() == 1);
                        check(name[i] + ": one BAND_DEMATERIALIZED (" + over.size() + ")", over.size() == 1);
                        check(name[i] + ": by a player, naming the killer", killed.get(0).aux() == ConflictRecords.BY_PLAYER
                                && player[i].getUUID().equals(ConflictKit.uuidAt(killed.get(0), ConflictRecords.ChiefKilled.KILLER_MOST)));
                        check(name[i] + ": the death is journaled before the band is (obsSeq " + killed.get(0).obsSeq() + " and " + over.get(0).obsSeq() + ")",
                                killed.get(0).obsSeq() < over.get(0).obsSeq());
                        check(name[i] + ": no casualty (" + ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent[i]).size() + ")",
                                ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent[i]).isEmpty());
                    }
                    check("chiefsKilled rose by two (" + was[0] + " to " + ConflictKit.bridge().chiefsKilled + ")", ConflictKit.bridge().chiefsKilled == was[0] + 2);
                    record(test, "invariant", "X (died 20 ticks before the recall): CHIEF_KILLED obsSeq " + ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[0]).get(0).obsSeq()
                            + ", BAND_DEMATERIALIZED " + dematerialized(agent[0]).get(0).obsSeq() + "; Y (died in the recall's tick): "
                            + ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[1]).get(0).obsSeq() + " and " + dematerialized(agent[1]).get(0).obsSeq());
                })
                .thenWaitUntil(waits.logic(() -> check("both animations ended (" + ConflictKit.bridge().dyingBodies() + " dying)", ConflictKit.bridge().dyingBodies() == 0
                        && puppet[0].isRemoved() && puppet[1].isRemoved())))
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++)
                        check("one Sol Visage where body " + i + " fell (" + visagesNear(level, fell[i], 6) + ")", visagesNear(level, fell[i], 6) == 1);
                    check("no loot record was lost (" + was[1] + " to " + ConflictKit.deathLootLost() + ")", ConflictKit.deathLootLost() == was[1]);
                    check("no second CHIEF_KILLED (" + ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[0]).size() + ", " + ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[1]).size() + ")",
                            ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[0]).size() == 1 && ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[1]).size() == 1);
                    drop(player);
                    far.releaseAll();
                })
                .thenSucceed();
    }

    // ---- 32t: the Umvuthi is handed back on its throne (MZB-821, MZB-822) ------------------------------------

    private static final int SPAWN = ConflictRecords.CampReconcile.SPAWN;

    /**
     * The chief's HAND_BACK waits for its puppet and then stands the Umvuthi on the throne (test 32's throne clause; MZB-821, MZB-822,
     * MZB-820). Three groves, each with an Invasion that comes home:
     * <ul>
     * <li><b>N</b>, a grove that is in, whose puppet floats 45 blocks east of the throne (health 120, its memory the masked player X's, a
     * player 100 blocks off). The engine's CHIEF_BODY home names a player Y only it knew, then three HAND_BACK lines in one reconcile:
     * the chief's and two warriors'. The warriors' lines play at once (the control): within 4 ticks their puppets are gone and a tagged
     * body stands within 4 blocks of each puppet's last place. The chief's line is held ({@code heldHandBacks} rises by one, no answer):
     * the puppet floats to the spot, turns to the throne's direction and descends, and only then is one Umvuthi on the throne (DONE,
     * within 0.5 blocks of the spot, tagged as the chief, health 120, no gravity set, the direction of the throne, remembering exactly X
     * and Y), the puppet gone. The last place the puppet was seen is within 0.5 blocks of the spot (not 45 blocks off) and facing the
     * throne's way.</li>
     * <li><b>L</b>, a grove that is in, whose puppet floats 200 blocks east (it cannot be home in the line's time at the pace of
     * 0.1 a tick). The line plays at {@link ConflictBridge#RECALL_TIMEOUT} ticks, within 3 before and 12 after, where the puppet stands:
     * DONE, the body within 3 blocks of the puppet's last place and well off the throne, {@code chiefHandBacksLate} up by one (N's
     * line was not late). The design's blocked spot cannot do this: a puppet over a block lands on it, which counts as settled.</li>
     * <li><b>O</b>, a grove that is out (declared, its chunks not loaded), with its puppet on a lane of its own and no player: the
     * engine sends a BAND_RECALL (RECALL_DONE) and no HAND_BACK. The puppet and its warrior go at once, one BAND_DEMATERIALIZED, no body
     * made ({@code handedBack} and {@code heldHandBacks} unchanged). The grove then comes in (its chunks are forced) and the census's
     * scripted chief SPAWN puts one Umvuthi on the throne: DONE, tagged as the chief, direction 3.</li>
     * </ul>
     * Fails without MZB-822 (N's chief line plays at once and the Umvuthi stands where the puppet was, not on the throne).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_throne", skyAccess = true)
    public static void theUmvuthiIsHandedBackOnItsThrone(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 9000);
        String test = "theUmvuthiIsHandedBackOnItsThrone";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        String dim = RtsimTests.dimension(helper);
        Grove n = groveAt(far, 0, 0), l = groveAt(far, 1, 0), o = groveAt(far, 2, 0); // o: declared, never forced until its census
        Lane lane = new Lane(far, far.k + 46, 0, 2); // O's puppet, clear of its footprint (chunks k + 38 to k + 42)
        long[] agent = {ConflictKit.newAgent(), ConflictKit.newAgent(), ConflictKit.newAgent()};
        EntityUmvuthi[] puppet = new EntityUmvuthi[3];
        Mob[] warrior = new Mob[3];
        List<Mob> nWarriors = new ArrayList<>();
        Vec3[] warriorAt = new Vec3[2];
        ServerPlayer[] player = new ServerPlayer[1]; // X
        UUID y = UUID.randomUUID();
        long[] op = new long[3]; // N's hand-back, L's, O's SPAWN
        long[] sentAt = new long[3];
        long[] warriorsGone = new long[1];
        long[] chiefGone = new long[2]; // N's, L's
        Vec3[] last = new Vec3[2]; // the last place N's and L's puppets were seen
        int[] lastDirection = new int[1];
        boolean[] sampling = {false};
        long[] was = new long[4]; // heldHandBacks, chiefHandBacksLate, handedBack, chiefMerges
        long[] oRecalled = new long[2]; // the recall's tick; the puppet gone

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    n.force();
                    forceEast(n, 6);
                    l.force();
                    forceEast(l, 14);
                    lane.force();
                    o.declare(); // its chunks are not loaded: the grove is out
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    n.lay().declare();
                    layEast(n, 6);
                    l.lay().declare();
                    layEast(l, 14);
                    lane.floor();
                    check("O is out: no epoch, no census (" + o.site().epoch() + ", " + MowzieKit.censusCount(o.faction) + ")",
                            o.site().epoch() == 0L && MowzieKit.censusCount(o.faction) == 0);
                })
                .thenWaitUntil(waits.logic(censused(n, l)))
                .thenExecute(() -> {
                    player[0] = MowzieKit.player(helper, off(n, 0, 100), "mowzie-throne-x");
                    MowzieKit.mask(player[0]);
                    chiefBand(helper, n, agent[0], n.spot.x + 45, n.spot.z, 120f, List.of(player[0].getUUID()), 2);
                    chiefBand(helper, l, agent[1], l.spot.x + 200, l.spot.z, 120f, List.of(), 1);
                    bandAt(helper, o.faction, agent[2], lane.x0 + 20, lane.top + 1, lane.z, 120f, List.of(), 1, true);
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < 3; i++) {
                        puppet[i] = puppet(agent[i]);
                        check("puppet " + i + " floats", puppet[i].isNoGravity());
                    }
                    check("N's two warriors are embodied (" + warriorsOf(agent[0]).size() + ")", warriorsOf(agent[0]).size() == 2);
                    check("L's and O's warrior are embodied", warriorsOf(agent[1]).size() == 1 && warriorsOf(agent[2]).size() == 1);
                }))
                .thenExecute(() -> {
                    nWarriors.addAll(warriorsOf(agent[0]));
                    warrior[1] = warriorsOf(agent[1]).get(0);
                    warrior[2] = warriorsOf(agent[2]).get(0);
                    for (int i = 0; i < 2; i++)
                        warriorAt[i] = nWarriors.get(i).position();
                    check("N's puppet carries the memory of X (" + puppet[0].getTradedPlayers() + ")", puppet[0].hasTradedWith(player[0]));
                    check("N's puppet does not face the throne's way (" + puppet[0].getDirectionData() + ")", puppet[0].getDirectionData() != 3);
                    check("N's puppet is " + String.format("%.1f", puppet[0].getX() - n.spot.x) + " blocks east of the throne", puppet[0].getX() - n.spot.x > 40);
                    was[0] = MowzieKit.camps().heldHandBacks;
                    was[1] = ConflictKit.chiefHandBacksLate();
                    was[2] = MowzieKit.camps().handedBack;
                    was[3] = MowzieKit.camps().chiefMerges;
                    MowzieKit.everyTick(helper, () -> {
                        if (!sampling[0])
                            return;
                        long now = level.getGameTime();
                        if (warriorsGone[0] == 0 && nWarriors.get(0).isRemoved() && nWarriors.get(1).isRemoved())
                            warriorsGone[0] = now;
                        for (int i = 0; i < 2; i++) {
                            if (chiefGone[i] == 0 && puppet[i].isRemoved())
                                chiefGone[i] = now;
                            if (!puppet[i].isRemoved())
                                last[i] = puppet[i].position();
                        }
                        if (!puppet[0].isRemoved())
                            lastDirection[0] = puppet[0].getDirectionData();
                    });
                    sampling[0] = true;
                    sentAt[0] = level.getGameTime();
                    // the engine's own home: CHIEF_BODY home (naming Y), then the chief's HAND_BACK and the two warriors', in one reconcile
                    ConflictKit.together(() -> {
                        ConflictKit.chiefBody(n.faction, epochOf(n), 120f, List.of(y));
                        ConflictKit.chiefBody(l.faction, epochOf(l), 120f, List.of());
                        op[0] = MowzieKit.reconcile(n.faction, epochOf(n), n.spot, 3f, List.of(
                                CampLine.party(CHIEF_SLOT, 1, UMVUTHI, HAND_BACK, CHIEF, agent[0], ConflictKit.party(agent[0]).epoch),
                                CampLine.party(0, 1, WARRIOR, HAND_BACK, 0, agent[0], ConflictKit.party(agent[0]).epoch),
                                CampLine.party(1, 1, WARRIOR, HAND_BACK, 0, agent[0], ConflictKit.party(agent[0]).epoch)));
                        op[1] = MowzieKit.reconcile(l.faction, epochOf(l), l.spot, 3f, List.of(
                                CampLine.party(CHIEF_SLOT, 1, UMVUTHI, HAND_BACK, CHIEF, agent[1], ConflictKit.party(agent[1]).epoch)));
                    });
                    sentAt[1] = sentAt[0];
                })
                .thenWaitUntil(waits.logic(() -> check("N's warriors went (" + warriorsGone[0] + ")", warriorsGone[0] != 0)))
                .thenIdle(3)
                .thenExecute(() -> {
                    check("N's warriors went at once (" + (warriorsGone[0] - sentAt[0]) + " ticks)", warriorsGone[0] - sentAt[0] <= 5);
                    for (int i = 0; i < 2; i++) {
                        List<Entity> body = MowzieKit.held(n.faction, i, 1);
                        check("a tagged warrior body stands for slot " + i + " (" + body.size() + ")", body.size() == 1);
                        check("within 4 blocks of where its puppet stood (" + body.get(0).position().distanceTo(warriorAt[i]) + ")",
                                body.get(0).position().distanceTo(warriorAt[i]) < 4);
                    }
                    Map<Integer, MowzieKit.Answered> lines = MowzieKit.answeredLines(n.faction, op[0]);
                    check("the warriors' lines are DONE and the chief's is held (answered lines: " + lines.keySet() + ")",
                            lines.size() == 2 && lines.containsKey(1) && lines.containsKey(2) && lines.get(1).outcome() == DONE && lines.get(2).outcome() == DONE);
                    check("the chiefs' lines of N and L were held (heldHandBacks +" + (MowzieKit.camps().heldHandBacks - was[0]) + ")",
                            MowzieKit.camps().heldHandBacks == was[0] + 2);
                    check("N's puppet floats still (removed " + puppet[0].isRemoved() + ", gravity off " + puppet[0].isNoGravity() + ")",
                            !puppet[0].isRemoved() && puppet[0].isAlive());
                    check("no Umvuthi stands on N's throne yet (" + thrones(level, n).size() + ")", thrones(level, n).isEmpty());
                    check("L's puppet floats, its line held (" + puppet[1].isRemoved() + ")", !puppet[1].isRemoved());
                })
                .thenWaitUntil(waits.logic(() -> check("N's chief puppet went (" + chiefGone[0] + ")", chiefGone[0] != 0)))
                .thenExecute(() -> {
                    long took = chiefGone[0] - sentAt[0];
                    check("it took the walk, not the first tick (" + took + " ticks)", took > 300);
                    check("the puppet was last seen within 0.5 blocks of the spot (" + Math.hypot(last[0].x - n.spot.x, last[0].z - n.spot.z) + ")",
                            Math.hypot(last[0].x - n.spot.x, last[0].z - n.spot.z) < 0.5);
                    check("and within 0.2 blocks of its height (" + (last[0].y - n.spot.y) + ")", last[0].y - n.spot.y < 0.2);
                    check("facing the throne's way (" + lastDirection[0] + ", the throne's " + n.site().throneDirection() + ")", lastDirection[0] == n.site().throneDirection());
                    MowzieKit.Answered chief = MowzieKit.answeredLines(n.faction, op[0]).get(0);
                    check("the chief's line is answered (" + MowzieKit.answeredLines(n.faction, op[0]).keySet() + ")", chief != null);
                    check("DONE, not " + MowzieKit.outcomeName(chief.outcome()), chief.outcome() == DONE);
                    List<Entity> held = MowzieKit.held(n.faction, CHIEF_SLOT, 1);
                    check("one Umvuthi stands on the throne as the chief (" + held.size() + ")", held.size() == 1 && held.get(0) instanceof EntityUmvuthi);
                    EntityUmvuthi body = (EntityUmvuthi) held.get(0);
                    check("within 0.5 blocks of the spot (" + Math.hypot(body.getX() - n.spot.x, body.getZ() - n.spot.z) + ")", Math.hypot(body.getX() - n.spot.x, body.getZ() - n.spot.z) < 0.5);
                    check("with the puppet's health (" + body.getHealth() + ")", body.getHealth() == 120f);
                    check("its gravity is on", !body.isNoGravity());
                    check("facing direction 3 (" + body.getDirectionData() + ")", body.getDirectionData() == 3);
                    check("it remembers X and Y and no one else (" + body.getTradedPlayers() + ")", body.hasTradedWith(player[0])
                            && body.getTradedPlayers().contains(y) && body.getTradedPlayers().size() == 2);
                    check("the puppet is gone", puppet[0].isRemoved());
                    check("N's line was no merge and no late one (" + (MowzieKit.camps().chiefMerges - was[3]) + ", " + (ConflictKit.chiefHandBacksLate() - was[1]) + ")",
                            MowzieKit.camps().chiefMerges == was[3] && ConflictKit.chiefHandBacksLate() == was[1]);
                    check("one chief and two warriors were handed back (" + (MowzieKit.camps().handedBack - was[2]) + ")", MowzieKit.camps().handedBack - was[2] == 3);
                    record(test, "home", "N's chief line waited " + took + " ticks (the walk of 45 blocks), the puppet was last seen " + String.format("%.2f",
                            Math.hypot(last[0].x - n.spot.x, last[0].z - n.spot.z)) + " blocks off the spot facing " + lastDirection[0] + "; the body remembers " + body.getTradedPlayers().size());
                })
                .thenWaitUntil(waits.logic(() -> check("L's chief puppet went (" + chiefGone[1] + ")", chiefGone[1] != 0)))
                .thenExecute(() -> {
                    long took = chiefGone[1] - sentAt[1];
                    check("L's line played at the timeout, " + ConflictBridge.RECALL_TIMEOUT + " ticks, within 3 before and 12 after (" + took + ")",
                            took >= ConflictBridge.RECALL_TIMEOUT - 3 && took <= ConflictBridge.RECALL_TIMEOUT + 12);
                    check("it was counted late, once (" + (ConflictKit.chiefHandBacksLate() - was[1]) + ")", ConflictKit.chiefHandBacksLate() == was[1] + 1);
                    check("the line is answered DONE", MowzieKit.answeredLines(l.faction, op[1]).containsKey(0) && MowzieKit.answeredLines(l.faction, op[1]).get(0).outcome() == DONE);
                    List<Entity> held = MowzieKit.held(l.faction, CHIEF_SLOT, 1);
                    check("one Umvuthi stands as the chief (" + held.size() + ")", held.size() == 1 && held.get(0) instanceof EntityUmvuthi);
                    double off = held.get(0).position().distanceTo(last[1]);
                    double fromThrone = Math.hypot(held.get(0).getX() - l.spot.x, held.get(0).getZ() - l.spot.z);
                    check("where the puppet stood (" + off + " blocks off its last place)", off < 3);
                    check("and well off the throne (" + fromThrone + " blocks)", fromThrone > 30);
                    check("with the puppet's health (" + ((EntityUmvuthi) held.get(0)).getHealth() + ")", ((EntityUmvuthi) held.get(0)).getHealth() == 120f);
                    check("L's puppet is gone", puppet[1].isRemoved());
                    record(test, "late", "L's line played " + took + " ticks after it was sent, " + String.format("%.0f", fromThrone) + " blocks from the throne; chiefHandBacksLate +"
                            + (ConflictKit.chiefHandBacksLate() - was[1]));
                })
                .thenWaitUntil(waits.logic(() -> check("N and L went (" + dematerialized(agent[0]).size() + ", " + dematerialized(agent[1]).size() + ")",
                        dematerialized(agent[0]).size() == 1 && dematerialized(agent[1]).size() == 1)))
                .thenExecute(() -> {
                    // O: the grove is out, so the engine's home is a recall and no HAND_BACK
                    was[0] = MowzieKit.camps().heldHandBacks;
                    was[2] = MowzieKit.camps().handedBack;
                    oRecalled[0] = level.getGameTime();
                    ConflictKit.together(() -> {
                        marchEnded(agent[2], o.faction, dim, o.spot.x, o.spot.z);
                        ConflictKit.recall(agent[2], ConflictKit.party(agent[2]).epoch, ConflictRecords.RECALL_DONE, 0, o.spot.x, o.spot.z);
                    });
                })
                .thenWaitUntil(waits.logic(() -> check("O's band went (" + dematerialized(agent[2]).size() + ")", dematerialized(agent[2]).size() == 1)))
                .thenExecute(() -> {
                    oRecalled[1] = level.getGameTime();
                    check("O's puppet and warrior are gone (" + puppet[2].isRemoved() + ", " + warrior[2].isRemoved() + ")", puppet[2].isRemoved() && warrior[2].isRemoved());
                    check("it went at once, nothing held it (" + (oRecalled[1] - oRecalled[0]) + " ticks)", oRecalled[1] - oRecalled[0] <= 12);
                    check("no body was made and no line held (" + (MowzieKit.camps().handedBack - was[2]) + ", " + (MowzieKit.camps().heldHandBacks - was[0]) + ")",
                            MowzieKit.camps().handedBack == was[2] && MowzieKit.camps().heldHandBacks == was[0]);
                    check("no CHIEF_KILLED (" + ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[2]).size() + ")", ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent[2]).isEmpty());
                    // the grove comes in: its chunks load, the census comes, and the scripted SPAWN puts the Umvuthi on the throne
                    o.force();
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> o.lay())
                .thenWaitUntil(waits.logic(() -> check("O's census (" + MowzieKit.censusCount(o.faction) + ")", MowzieKit.censusCount(o.faction) >= 1 && o.site().in())))
                .thenExecute(() -> {
                    Vec3 throne = o.at(8, 8);
                    op[2] = MowzieKit.reconcile(o.faction, epochOf(o), throne, 3f, List.of(CampLine.of(CHIEF_SLOT, 1, UMVUTHI, SPAWN, CHIEF)));
                })
                .thenWaitUntil(waits.logic(() -> MowzieKit.answered(o.faction, op[2], 1)))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("O's SPAWN", ConflictKit.outcomes(o.faction, op[2]), DONE);
                    List<Entity> held = MowzieKit.held(o.faction, CHIEF_SLOT, 1);
                    check("one Umvuthi stands on O's throne as the chief (" + held.size() + ")", held.size() == 1 && held.get(0) instanceof EntityUmvuthi);
                    EntityUmvuthi body = (EntityUmvuthi) held.get(0);
                    check("it is no puppet and its gravity is on", !RtsimApi.isPuppet(body) && !body.isNoGravity());
                    check("facing direction 3 (" + body.getDirectionData() + ")", body.getDirectionData() == 3);
                    record(test, "out", "O's puppet went " + (oRecalled[1] - oRecalled[0]) + " ticks after the recall, no body made; the census's SPAWN stood one Umvuthi on the throne");
                    drop(player);
                    far.releaseAll();
                })
                .thenSucceed();
    }

    // ---- 22-aloft: the chief sets out and comes home in plain view (MZB-822, MZB-830) ----------------------

    private static final int SET_OUT = ConflictRecords.CampReconcile.SET_OUT;

    /**
     * The Umvuthi sets out and comes home in a player's plain view (test 22's "in plain view" for the chief; MZB-822, MZB-830). A
     * masked player stands 150 blocks east of grove G (in, view distance 10), whose Umvuthi (health 77, remembering a player A who is
     * gone again) and four warriors are adopted. The band sets out as the engine's hand-over does: SET_OUT lines for the chief and the
     * warriors, CHIEF_BODY away and the materialize of the warriors and the chief's stack, in one step. It is steered 60 blocks toward the
     * player, and there the engine's CHIEF_BODY home and five HAND_BACK lines (the chief's, the warriors') are sent. Every tick's end,
     * the Umvuthi (the body, then the puppet, then the body again) within 160 blocks of the player is counted and followed:
     * <ul>
     * <li>There is exactly one Umvuthi within 160 blocks of the player at the end of every tick, from before the set-out to after the
     * hand-back (the set-out is no take that waits while the player looks on, MZB-830).</li>
     * <li>The one that is within 128 blocks of the player moves less than 0.5 blocks a tick. The two swaps (the hand-off and the
     * hand-back) replace it within 0.1 blocks, one tick's step of the puppet's pace; the measured most is logged.</li>
     * <li>The puppet has the throne body's health and memory; the throne body made at home has the puppet's, with no one else.</li>
     * <li>The five SET_OUT lines are DONE, handed off, none taken (one body each). The warriors' HAND_BACKs play at once where each
     * puppet stands (the control: DONE within 5 ticks, a tagged body within 4 blocks), the chief's waits: the puppet floats home
     * (taking over 300 ticks), turns and settles on the spot before the Umvuthi stands on the throne, DONE.</li>
     * </ul>
     * Fails without MZB-830 (the chief's SET_OUT is a take held while the player looks, so two Umvuthi stand in sight) and without
     * MZB-822 (the hand-back plays at once, and the Umvuthi jumps from the puppet's place to the throne).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_view", skyAccess = true)
    public static void anUmvuthiSetsOutAndComesHomeInPlainView(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 9000);
        String test = "anUmvuthiSetsOutAndComesHomeInPlainView";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        String dim = RtsimTests.dimension(helper);
        Grove g = groveAt(far, 0, 0);
        long agent = ConflictKit.newAgent();
        EntityUmvuthi[] throne = new EntityUmvuthi[1], puppet = new EntityUmvuthi[1];
        EntityUmvuthanaMinion[] w = new EntityUmvuthanaMinion[4];
        ServerPlayer[] viewer = new ServerPlayer[1], payer = new ServerPlayer[1];
        UUID[] a = new UUID[1];
        long[] op = new long[3]; // ADOPT, SET_OUT, HAND_BACK
        long[] was = new long[3]; // handedOff, taken, handedBack
        long[] sentAt = new long[1];
        long[] gone = new long[2]; // the warriors', the chief's puppet's
        List<Mob> pw = new ArrayList<>();
        Vec3[] warriorAt = new Vec3[4];
        Vec3[] last = new Vec3[1];
        int[] lastDirection = new int[1];
        boolean[] sampling = {false};
        Vec3[] prev = new Vec3[1];
        UUID[] prevId = new UUID[1];
        List<String> broke = new ArrayList<>();
        double[] most = new double[2]; // the biggest step of one Umvuthi in sight; the biggest swap
        int[] swaps = new int[1], ticks = new int[1];
        Runnable[] back = new Runnable[1];

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    g.force();
                    forceEast(g, 6);
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    back[0] = viewDistance(helper, level, 10);
                    g.lay().declare();
                    layEast(g, 6);
                    throne[0] = MowzieKit.umvuthi(level, g.at(0, 0));
                    double[][] at = {{12, 0}, {6, 10}, {-8, -6}, {-14, 4}};
                    for (int i = 0; i < 4; i++)
                        w[i] = MowzieKit.warrior(level, g.at(at[i][0], at[i][1]));
                    viewer[0] = MowzieKit.player(helper, off(g, 150, 0), "mowzie-view-chief");
                    MowzieKit.mask(viewer[0]);
                    // A pays the throne body, then goes: its memory is what the puppet must keep
                    payer[0] = MowzieKit.player(helper, off(g, 0, 8), "mowzie-view-payer");
                    MowzieKit.mask(payer[0]);
                    a[0] = payer[0].getUUID();
                })
                .thenWaitUntil(waits.logic(censused(g)))
                .thenExecute(() -> {
                    List<CampLine> lines = new ArrayList<>();
                    lines.add(CampLine.of(CHIEF_SLOT, 1, UMVUTHI, ADOPT, CHIEF, throne[0].getUUID()));
                    for (int i = 0; i < 4; i++)
                        lines.add(CampLine.of(i, 1, WARRIOR, ADOPT, 0, w[i].getUUID()));
                    op[0] = MowzieKit.reconcile(g.faction, epochOf(g), lines);
                })
                .thenWaitUntil(waits.logic(() -> MowzieKit.answered(g.faction, op[0], 5)))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("ADOPT of the chief and four warriors", ConflictKit.outcomes(g.faction, op[0]), DONE);
                    throne[0].setHealth(77f);
                    check("the jar remembers the payer of the throne body", MowzieKit.payUmvuthi(payer[0], throne[0]));
                    drop(payer);
                    payer[0] = null;
                    for (int i = 0; i < 4; i++)
                        warriorAt[i] = w[i].position();
                    check("the throne body is 150 blocks from the viewer (" + Math.hypot(throne[0].getX() - viewer[0].getX(), throne[0].getZ() - viewer[0].getZ()) + ")",
                            Math.abs(Math.hypot(throne[0].getX() - viewer[0].getX(), throne[0].getZ() - viewer[0].getZ()) - 150) < 1);
                    was[0] = MowzieKit.camps().handedOff;
                    was[1] = MowzieKit.camps().taken;
                    AABB wide = new AABB(viewer[0].getX() - 160, level.getMinBuildHeight(), viewer[0].getZ() - 160, viewer[0].getX() + 160,
                            level.getMaxBuildHeight(), viewer[0].getZ() + 160);
                    MowzieKit.everyTick(helper, () -> {
                        if (!sampling[0])
                            return;
                        long now = level.getGameTime();
                        ticks[0]++;
                        List<EntityUmvuthi> near = new ArrayList<>();
                        for (EntityUmvuthi u : level.getEntitiesOfClass(EntityUmvuthi.class, wide, e -> !e.isRemoved()))
                            if (Math.hypot(u.getX() - viewer[0].getX(), u.getZ() - viewer[0].getZ()) < 160)
                                near.add(u);
                        if (near.size() != 1) {
                            if (broke.size() < 8)
                                broke.add("tick " + now + ": " + near.size() + " Umvuthi within 160 blocks");
                            prev[0] = null;
                            return;
                        }
                        EntityUmvuthi u = near.get(0);
                        if (prev[0] != null) {
                            if (u.getUUID().equals(prevId[0])) {
                                double dh = Math.hypot(u.getX() - prev[0].x, u.getZ() - prev[0].z);
                                if (Math.hypot(u.getX() - viewer[0].getX(), u.getZ() - viewer[0].getZ()) < 128) {
                                    most[0] = Math.max(most[0], dh);
                                    if (dh >= 0.5 && broke.size() < 8)
                                        broke.add("tick " + now + ": it moved " + dh + " blocks in sight");
                                }
                            } else {
                                swaps[0]++;
                                double d = u.position().distanceTo(prev[0]);
                                most[1] = Math.max(most[1], d);
                                if (d > 0.1 && broke.size() < 8)
                                    broke.add("tick " + now + ": the swap moved it " + d + " blocks");
                            }
                        }
                        prev[0] = u.position();
                        prevId[0] = u.getUUID();
                        if (puppet[0] != null && !puppet[0].isRemoved()) {
                            last[0] = puppet[0].position();
                            lastDirection[0] = puppet[0].getDirectionData();
                        }
                        if (gone[0] == 0 && !pw.isEmpty() && pw.stream().allMatch(Mob::isRemoved))
                            gone[0] = now;
                        if (gone[1] == 0 && puppet[0] != null && puppet[0].isRemoved())
                            gone[1] = now;
                    });
                    sampling[0] = true;
                    // the hand-over of a set-out: the five lines, CHIEF_BODY away and the materialize, in one step
                    ConflictKit.Band b = ConflictKit.band(agent, g.faction, dim);
                    b.flags |= ConflictRecords.Materialize.FLAG_CHIEF;
                    b.purpose = ConflictRecords.PURPOSE_INVASION;
                    b.arcX = g.spot.x;
                    b.arcZ = g.spot.z;
                    b.bearing = 0f;
                    b.targetX = g.spot.x;
                    b.targetY = g.spot.y;
                    b.targetZ = g.spot.z;
                    b.speed = 0f;
                    int epoch = b.epoch;
                    ConflictKit.together(() -> {
                        List<CampLine> lines = new ArrayList<>();
                        lines.add(CampLine.party(CHIEF_SLOT, 1, UMVUTHI, SET_OUT, CHIEF, agent, epoch));
                        for (int i = 0; i < 4; i++)
                            lines.add(CampLine.party(i, 1, WARRIOR, SET_OUT, 0, agent, epoch));
                        op[1] = MowzieKit.reconcile(g.faction, epochOf(g), lines);
                        ConflictKit.chiefBody(g.faction, agent, epoch, 77f, List.of(a[0]));
                        ConflictKit.materialize(b, List.of(new ConflictKit.StackSpec(WARRIOR, 4, false), new ConflictKit.StackSpec(UMVUTHI, 0, true)));
                    });
                })
                .thenWaitUntil(waits.logic(() -> {
                    MowzieKit.answered(g.faction, op[1], 5);
                    puppet[0] = puppet(agent);
                    check("the band has its four warriors (" + warriorsOf(agent).size() + ")", warriorsOf(agent).size() == 4);
                }))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("the five SET_OUT lines", ConflictKit.outcomes(g.faction, op[1]), DONE);
                    check("five bodies handed off, none taken (" + (MowzieKit.camps().handedOff - was[0]) + ", " + (MowzieKit.camps().taken - was[1]) + ")",
                            MowzieKit.camps().handedOff - was[0] == 5 && MowzieKit.camps().taken == was[1]);
                    check("the throne body is gone and the puppet floats (" + throne[0].isRemoved() + ", " + puppet[0].isNoGravity() + ")", throne[0].isRemoved() && puppet[0].isNoGravity());
                    check("the puppet has the throne body's health (" + puppet[0].getHealth() + ")", puppet[0].getHealth() == 77f);
                    check("and its memory of A (" + puppet[0].getTradedPlayers() + ")", puppet[0].getTradedPlayers().contains(a[0]) && puppet[0].getTradedPlayers().size() == 1);
                    pw.addAll(warriorsOf(agent));
                    ConflictKit.steer(agent, ConflictKit.party(agent).epoch, PACE, g.spot.x + 60, g.spot.z, 0, false);
                })
                .thenWaitUntil(waits.logic(() -> check("the puppet is 55 blocks out (" + (puppet[0].getX() - g.spot.x) + ")", puppet[0].getX() - g.spot.x >= 55)))
                .thenExecute(() -> {
                    double d = Math.hypot(puppet[0].getX() - viewer[0].getX(), puppet[0].getZ() - viewer[0].getZ());
                    check("it passed within 100 blocks of the viewer (" + d + ")", d <= 100);
                    check("it is still aloft", puppet[0].isNoGravity() && !puppet[0].isRemoved());
                    for (int i = 0; i < 4; i++)
                        warriorAt[i] = pw.get(i).position();
                    was[2] = MowzieKit.camps().handedBack;
                    sentAt[0] = level.getGameTime();
                    ConflictKit.together(() -> {
                        ConflictKit.chiefBody(g.faction, epochOf(g), 77f, List.of());
                        List<CampLine> lines = new ArrayList<>();
                        lines.add(CampLine.party(CHIEF_SLOT, 1, UMVUTHI, HAND_BACK, CHIEF, agent, ConflictKit.party(agent).epoch));
                        for (int i = 0; i < 4; i++)
                            lines.add(CampLine.party(i, 1, WARRIOR, HAND_BACK, 0, agent, ConflictKit.party(agent).epoch));
                        op[2] = MowzieKit.reconcile(g.faction, epochOf(g), g.spot, 3f, lines);
                    });
                })
                .thenWaitUntil(waits.logic(() -> check("the warriors went (" + gone[0] + ")", gone[0] != 0)))
                .thenIdle(3)
                .thenExecute(() -> {
                    check("the warriors went at once (" + (gone[0] - sentAt[0]) + " ticks)", gone[0] - sentAt[0] <= 5);
                    Map<Integer, MowzieKit.Answered> lines = MowzieKit.answeredLines(g.faction, op[2]);
                    check("the four warriors' lines are DONE and the chief's waits (answered " + lines.keySet() + ")",
                            lines.size() == 4 && !lines.containsKey(0) && lines.values().stream().allMatch(x -> x.outcome() == DONE));
                    for (int i = 0; i < 4; i++) {
                        List<Entity> body = MowzieKit.held(g.faction, i, 1);
                        check("a tagged warrior body stands for slot " + i + " (" + body.size() + ")", body.size() == 1);
                        check("within 4 blocks of where its puppet stood (" + body.get(0).position().distanceTo(warriorAt[i]) + ")", body.get(0).position().distanceTo(warriorAt[i]) < 4);
                    }
                    check("the puppet floats still, no Umvuthi on the throne (" + thrones(level, g).size() + ")", !puppet[0].isRemoved() && thrones(level, g).isEmpty());
                })
                .thenWaitUntil(waits.logic(() -> check("the chief's puppet went (" + gone[1] + ")", gone[1] != 0)))
                .thenExecute(() -> {
                    long took = gone[1] - sentAt[0];
                    check("it floated home first (" + took + " ticks)", took > 300);
                    check("the puppet was last seen within 0.5 blocks of the spot (" + Math.hypot(last[0].x - g.spot.x, last[0].z - g.spot.z) + ")",
                            Math.hypot(last[0].x - g.spot.x, last[0].z - g.spot.z) < 0.5);
                    check("facing the throne's way (" + lastDirection[0] + ")", lastDirection[0] == 3);
                    Map<Integer, MowzieKit.Answered> lines = MowzieKit.answeredLines(g.faction, op[2]);
                    check("the chief's line is DONE (" + lines.keySet() + ")", lines.containsKey(0) && lines.get(0).outcome() == DONE);
                    List<Entity> held = MowzieKit.held(g.faction, CHIEF_SLOT, 1);
                    check("one Umvuthi stands on the throne as the chief (" + held.size() + ")", held.size() == 1 && held.get(0) instanceof EntityUmvuthi);
                    EntityUmvuthi body = (EntityUmvuthi) held.get(0);
                    check("on the spot (" + Math.hypot(body.getX() - g.spot.x, body.getZ() - g.spot.z) + ")", Math.hypot(body.getX() - g.spot.x, body.getZ() - g.spot.z) < 0.5);
                    check("with the health it had (" + body.getHealth() + ")", body.getHealth() == 77f);
                    check("remembering A and no one else (" + body.getTradedPlayers() + ")", body.getTradedPlayers().contains(a[0]) && body.getTradedPlayers().size() == 1);
                    check("five bodies handed back (" + (MowzieKit.camps().handedBack - was[2]) + ")", MowzieKit.camps().handedBack - was[2] == 5);
                    sampling[0] = false;
                    check("one Umvuthi stood within 160 blocks of the viewer at the end of every tick (" + ticks[0] + " ticks): " + broke, broke.isEmpty());
                    check("the hand-off and the hand-back were the two swaps (" + swaps[0] + ")", swaps[0] == 2);
                    record(test, "view", String.format("%d ticks watched, the most it moved in sight %.3f blocks a tick, the swaps %.3f at the most; the chief's line waited %d ticks",
                            ticks[0], most[0], most[1], took));
                })
                .thenWaitUntil(waits.logic(() -> check("the band went (" + dematerialized(agent).size() + ")", dematerialized(agent).size() == 1)))
                .thenExecute(() -> {
                    back[0].run();
                    drop(viewer);
                    far.releaseAll();
                })
                .thenSucceed();
    }

    // ---- 31s: the re-pair and the trail, as a watching player is sent them (MZB-813, MZB-814, MZB-889) ----------------

    /**
     * Vanilla's tracker, private to {@code ChunkMap}, read by reflection (D3: no new mixin): which players a body is paired with, the
     * position base its adds start from, and how many trackers the map holds for it.
     */
    private static final class Tracked {
        private static java.lang.reflect.Field map, seen, server, body;

        private static void open() {
            if (map != null)
                return;
            try {
                Class<?> t = Class.forName("net.minecraft.server.level.ChunkMap$TrackedEntity", false, ChunkMap.class.getClassLoader());
                java.lang.reflect.Field m = ChunkMap.class.getDeclaredField("entityMap");
                java.lang.reflect.Field s = t.getDeclaredField("seenBy");
                java.lang.reflect.Field v = t.getDeclaredField("serverEntity");
                java.lang.reflect.Field b = t.getDeclaredField("entity");
                m.setAccessible(true);
                s.setAccessible(true);
                v.setAccessible(true);
                b.setAccessible(true);
                seen = s;
                server = v;
                body = b;
                map = m;
            } catch (ReflectiveOperationException | RuntimeException e) {
                throw new GameTestAssertException("cannot read the chunk map's trackers: " + e);
            }
        }

        private static Object get(java.lang.reflect.Field f, Object of) {
            try {
                return f.get(of);
            } catch (ReflectiveOperationException e) {
                throw new GameTestAssertException("cannot read " + f + ": " + e);
            }
        }

        private static java.util.Map<?, ?> trackers(ServerLevel level) {
            open();
            return (java.util.Map<?, ?>) get(map, level.getChunkSource().chunkMap);
        }

        /** The tracker of a body, or null. */
        static Object of(ServerLevel level, Entity e) {
            return trackers(level).get(e.getId());
        }

        /** Whether the body is paired with the player: its connection is in the tracker's {@code seenBy}. */
        static boolean sees(ServerLevel level, Entity e, ServerPlayer p) {
            Object t = of(level, e);
            return t != null && ((java.util.Set<?>) get(seen, t)).contains(p.connection);
        }

        /** The base the live tracker's deltas run from. */
        static Vec3 base(ServerLevel level, Entity e) {
            Object t = of(level, e);
            if (t == null)
                throw new GameTestAssertException("no tracker for " + e);
            return ((ServerEntity) get(server, t)).getPositionBase();
        }

        /** How many trackers in the map follow this entity (one: the map is keyed by id, and the re-pair adds none). */
        static int count(ServerLevel level, Entity e) {
            open();
            int n = 0;
            for (Object t : trackers(level).values())
                if (get(body, t) == e)
                    n++;
            return n;
        }
    }

    /**
     * 31s's reading of one watched puppet, at the end of every tick (the test's own sample, so a violation is noted here and asserted by
     * the sequence: a throw in a per-tick hook is only logged). A re-pair is an add packet that two hearers are sent at once, after the
     * test armed it; each one is read for what the design says it carries, and the server's body is compared across its tick.
     */
    private static final class Reading {
        /** What a pairing bundle holds after its add packet (vanilla's, the plugin's re-pair sends no other kind). */
        private static final java.util.Set<Class<?>> BUNDLED = java.util.Set.of(ClientboundSetEntityDataPacket.class,
                ClientboundUpdateAttributesPacket.class, ClientboundSetEquipmentPacket.class, ClientboundUpdateMobEffectPacket.class,
                ClientboundSetPassengersPacket.class, ClientboundSetEntityLinkPacket.class, ClientboundSetEntityMotionPacket.class);

        final ServerLevel level;
        final EntityUmvuthi body;
        final int id;
        final ServerPlayer credit;
        final FakePlayers.Hearer[] hearers;
        final int directionId;
        final java.lang.reflect.Field boss;
        final List<String> broke = new ArrayList<>();
        final List<Long> repairs = new ArrayList<>();
        /** Per re-pair, the way (the body's move that tick) and the yaw the add packet carried. */
        final List<String> facts = new ArrayList<>();
        final long[] adds;
        /** The last tick each of join, leave, start tracking, stop tracking and death fired for the body. */
        final long[] fired = new long[5];
        /** The first tick after the n-th re-pair that the body's way was more than the turn's limit off its facing. */
        final long[] dueAfter = new long[8];
        boolean armed;
        Vec3 prev;
        String before;
        boolean wasAbility;
        int prevDirection;
        double still1 = -1, still2 = -1;
        /** The ROAR: when the test sent it, the ticks the ability was active, the first tick it was not. */
        long roarAt, roarEnd;
        int roarTicks;
        int removedBefore = -1;

        Reading(ServerLevel level, EntityUmvuthi body, ServerPlayer credit, FakePlayers.Hearer... hearers) {
            this.level = level;
            this.body = body;
            this.id = body.getId();
            this.credit = credit;
            this.hearers = hearers;
            adds = new long[hearers.length];
            try {
                java.lang.reflect.Field d = EntityUmvuthi.class.getDeclaredField("DIRECTION");
                d.setAccessible(true);
                directionId = ((net.minecraft.network.syncher.EntityDataAccessor<?>) d.get(null)).id();
                java.lang.reflect.Field b = com.bobmowzie.mowziesmobs.server.entity.MowzieEntity.class.getDeclaredField("bossInfo");
                b.setAccessible(true);
                boss = b;
            } catch (ReflectiveOperationException e) {
                throw new GameTestAssertException("cannot read the jar's direction or boss bar: " + e);
            }
        }

        void arm() {
            for (int i = 0; i < hearers.length; i++)
                adds[i] = hearers[i].added(id);
            prev = body.position();
            before = state();
            wasAbility = body.getActiveAbility() != null;
            prevDirection = body.getDirectionData();
            armed = true;
        }

        private void note(String s) {
            if (broke.size() < 14)
                broke.add(s);
        }

        private String bossPlayers() {
            try {
                java.util.TreeSet<String> names = new java.util.TreeSet<>();
                for (ServerPlayer p : ((net.minecraft.server.level.ServerBossEvent) boss.get(body)).getPlayers())
                    names.add(p.getName().getString());
                return names.toString();
            } catch (ReflectiveOperationException e) {
                return "unreadable " + e;
            }
        }

        /** What a re-pair must leave as it was: every fact of the server's body but its facing and its position. */
        private String state() {
            StringBuilder s = new StringBuilder();
            s.append(body.getId()).append(' ').append(body.getUUID()).append(" removed ").append(body.isRemoved());
            s.append(" health ").append(body.getHealth()).append(" lost ").append(body.getHealthLost());
            java.util.TreeSet<String> fx = new java.util.TreeSet<>();
            for (MobEffectInstance e : body.getActiveEffects())
                fx.add(e.getEffect().getRegisteredName() + "/" + e.getAmplifier());
            s.append(" effects ").append(fx).append(" glowing ").append(body.hasGlowingTag());
            net.minecraft.world.entity.LivingEntity k = body.getKillCredit();
            s.append(" credit ").append(k == null ? "none" : k.getUUID());
            s.append(" target ").append(body.getTarget() == null ? "none" : body.getTarget().getUUID());
            s.append(" boss ").append(bossPlayers());
            s.append(" trackers ").append(Tracked.count(level, body));
            return s.toString();
        }

        void tick() {
            if (!armed || body.isRemoved())
                return;
            long now = level.getGameTime();
            Vec3 at = body.position();
            boolean ability = body.getActiveAbility() != null;
            int rose = 0;
            for (int i = 0; i < hearers.length; i++) {
                long n = hearers[i].added(id);
                if (n != adds[i]) {
                    if (n != adds[i] + 1)
                        note("tick " + now + ": hearer " + i + " was sent " + (n - adds[i]) + " adds in one tick");
                    adds[i] = n;
                    rose++;
                }
            }
            String state = state();
            double moved = at.distanceTo(prev);
            if (roarAt != 0 && roarEnd == 0) {
                if (ability)
                    roarTicks++;
                else if (roarTicks > 0)
                    roarEnd = now;
            }
            if (rose > 0) {
                if (rose != hearers.length)
                    note("tick " + now + ": only " + rose + " of " + hearers.length + " hearers were sent an add");
                repaired(now, at, moved, state);
            }
            if (!repairs.isEmpty() && dueAfter[repairs.size()] == 0 && moved > 0.05
                    && AloftRules.turn(body.getYRot(), AloftRules.wayYaw(at.x - prev.x, at.z - prev.z)) > AloftRules.TURN)
                dueAfter[repairs.size()] = now;
            still2 = still1;
            still1 = moved;
            prev = at;
            before = state;
            wasAbility = ability;
            prevDirection = body.getDirectionData();
            if (!body.isDeadOrDying())
                body.setLastHurtByPlayer(credit); // after the sample: the jar forgets its attacker 100 ticks on, and the test hits it once
        }

        private void repaired(long now, Vec3 at, double moved, String state) {
            repairs.add(now);
            if (repairs.size() > 1 && now - repairs.get(repairs.size() - 2) < AloftRules.REPAIR_COOLDOWN)
                note("tick " + now + ": a re-pair " + (now - repairs.get(repairs.size() - 2)) + " ticks after the last");
            if (wasAbility)
                note("tick " + now + ": a re-pair with an ability active");
            if (body.isDeadOrDying())
                note("tick " + now + ": a re-pair of a dying body");
            if (still1 != 0.0 || still2 != 0.0)
                note("tick " + now + ": it moved in the two ticks before (" + still2 + ", " + still1 + "): no hold");
            if (moved < 1.0e-6)
                note("tick " + now + ": it did not move in the re-pair's tick");
            if (!state.equals(before))
                note("tick " + now + ": the body changed across its re-pair: [" + before + "] became [" + state + "]");
            String[] kinds = {"join", "leave", "start tracking", "stop tracking", "death"};
            for (int k = 0; k < fired.length; k++)
                if (fired[k] == now)
                    note("tick " + now + ": " + kinds[k] + " fired for the body in the re-pair's tick");
            float way = moved < 1.0e-6 ? Float.NaN : AloftRules.wayYaw(at.x - prev.x, at.z - prev.z);
            for (int i = 0; i < hearers.length; i++)
                packets(now, i, way);
        }

        private void packets(long now, int hearer, float way) {
            List<net.minecraft.network.protocol.Packet<?>> ps = hearers[hearer].packets;
            int k = -1;
            for (int i = ps.size() - 1; i >= 0 && k < 0; i--)
                if (ps.get(i) instanceof ClientboundAddEntityPacket p && p.getId() == id)
                    k = i;
            if (k < 0) {
                note("tick " + now + ": hearer " + hearer + " has no add packet for the body");
                return;
            }
            ClientboundAddEntityPacket add = (ClientboundAddEntityPacket) ps.get(k);
            if (!add.getUUID().equals(body.getUUID()))
                note("tick " + now + ": the add packet names " + add.getUUID() + ", not " + body.getUUID());
            Vec3 packet = new Vec3(add.getX(), add.getY(), add.getZ());
            Vec3 live = Tracked.base(level, body);
            if (packet.distanceTo(prev) > 1.0e-9)
                note("tick " + now + ": the add is at " + packet + ", the body stood at " + prev + " the tick before");
            if (packet.distanceTo(live) > 1.0e-9)
                note("tick " + now + ": the add is at " + packet + ", the live tracker's base is " + live);
            float yaw = add.getYRot();
            float want = AloftRules.quarterYaw(body.getDirectionData());
            float old = AloftRules.quarterYaw(prevDirection);
            if (AloftRules.turn(yaw, want) > 2.5f)
                note("tick " + now + ": the add's yaw is " + yaw + ", the jar's direction " + body.getDirectionData() + " is " + want);
            if (AloftRules.turn(yaw, old) < 80f)
                note("tick " + now + ": the add's yaw " + yaw + " is within 80 degrees of the old facing " + old);
            if (!Float.isNaN(way) && AloftRules.turn(yaw, way) > 47f)
                note("tick " + now + ": the add's yaw " + yaw + " is more than 45 degrees off the way " + way);
            Integer direction = null;
            Byte flags = null;
            java.util.TreeSet<String> sent = new java.util.TreeSet<>();
            int run = 0;
            for (int i = k + 1; i < ps.size() && BUNDLED.contains(ps.get(i).getClass()); i++) {
                run++;
                net.minecraft.network.protocol.Packet<?> q = ps.get(i);
                if (q instanceof ClientboundSetEntityDataPacket d && d.id() == id) {
                    for (SynchedEntityData.DataValue<?> v : d.packedItems()) {
                        if (v.id() == directionId && v.value() instanceof Integer n)
                            direction = n;
                        if (v.id() == 0 && v.value() instanceof Byte b)
                            flags = b;
                    }
                } else if (q instanceof ClientboundUpdateMobEffectPacket e && e.getEntityId() == id) {
                    sent.add(e.getEffect().getRegisteredName() + "/" + e.getEffectAmplifier());
                }
            }
            if (direction == null || direction != body.getDirectionData())
                note("tick " + now + ": the bundle carries direction " + direction + ", the body's is " + body.getDirectionData());
            if (flags == null || (flags & 0x40) == 0)
                note("tick " + now + ": the bundle's flag byte is " + flags + ": no glowing bit");
            java.util.TreeSet<String> have = new java.util.TreeSet<>();
            for (MobEffectInstance e : body.getActiveEffects())
                have.add(e.getEffect().getRegisteredName() + "/" + e.getAmplifier());
            if (!sent.equals(have))
                note("tick " + now + ": the bundle's effects are " + sent + ", the body's " + have);
            if (hearer == 0)
                facts.add(String.format("tick %d: yaw %.1f (old %.1f, way %.1f), %d packets after the add", now, yaw, old, way, run));
        }
    }

    /** Counts the particle packets of a kind a hearer was sent between two marks of its packet list. */
    private static long flames(FakePlayers.Hearer h, int from, int to) {
        return h.packets.subList(from, Math.min(to, h.packets.size())).stream()
                .filter(q -> q instanceof ClientboundLevelParticlesPacket p && p.getParticle().getType() == ParticleTypes.SMALL_FLAME && p.isOverrideLimiter())
                .count();
    }

    /**
     * An aloft Umvuthi reads and turns, as the server sees it (test 31s; MZB-813, MZB-814). Lane A's puppet floats with its Invasion;
     * three watching hearers (a near one, a far one 70 to 95 blocks off, and an away one 180 blocks off) are pumped every tick
     * ({@link FakePlayers#pump}, MZB-889: the first test that runs it), the first two paired with it (read from the tracker's
     * {@code seenBy}). The body carries Sunblock, Fire Resistance and a glowing flag, a health loss and a last attacker.
     * <ol>
     * <li>The trail: over its first 100 ticks east, each of the near and the far hearer is sent about 10 small_flame packets with the
     * long-distance flag, and the away one none, for the whole test.</li>
     * <li>A ROAR starts and the band turns 90 degrees (south at 0.2 a tick, the puppet's cap being 0.1). No add packet is sent while
     * the ability is active; the first re-pair comes two or three ticks after it, after two ticks without a move.</li>
     * <li>Each re-pair, read in the tick it happens, for both hearers: one add packet with the body's id and UUID, at the position the
     * body stood at the tick before and the live tracker's base, a yaw within 45 degrees of the way and 80 or more off the old facing;
     * the bundle's data carries the new direction, the glowing bit and every effect.</li>
     * <li>The server's body is the same across the tick (id, UUID, health, health lost, effects, glowing, kill credit, target, the boss
     * bar's players), the map holds one tracker for it, and none of join, leave, start or stop tracking, or death fired in it.</li>
     * <li>A second turn 100 ticks later (west at 0.3) is due before 200 ticks have passed, and the second re-pair comes 200 or more ticks
     * after the first. A third turn is due when the body is killed 198 ticks after the second re-pair: nothing is sent for the dying body.</li>
     * </ol>
     * The control, lane C, turns 40 degrees at 0.2 a tick, so the puppet lags and its way is 40 degrees off its facing for 400 ticks and
     * longer: no add packet beyond its pairing. Fails without MZB-813 (no add packet), without the hold (the body moved in the two ticks
     * before), and without the cooldown, the ability check or the dying check of the gate (a second, early, mid-ROAR or dying add).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_repair", skyAccess = true)
    public static void anAloftUmvuthiReadsAndTurns(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 9000);
        String test = "anAloftUmvuthiReadsAndTurns";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Lane a = new Lane(far, 0, 8), c = new Lane(far, 1, 8);
        long[] agent = {ConflictKit.newAgent(), ConflictKit.newAgent()};
        long faction = ConflictKit.newFaction();
        EntityUmvuthi[] puppet = new EntityUmvuthi[2];
        FakePlayers.Hearer[] hearer = new FakePlayers.Hearer[4]; // near, far, away (A's), and the control's
        Reading[] reading = new Reading[1];
        Runnable[] back = new Runnable[1];
        long[] at = new long[8]; // first steer, ROAR and the second, third steer, fourth steer
        int[] mark = new int[4]; // the near and far hearers' packet counts at the window's start and end
        long[] counted = new long[4]; // ConflictBridge's repairs and repairsSkipped at the start, and at the kill
        long[] control = new long[3]; // the control's ticks, its adds, the tick it was read at
        double[] turned = new double[2]; // the control's biggest way off its facing, and its lag at 400 ticks
        int[] controlDirection = new int[1];
        Vec3[] controlAt = new Vec3[1];
        double[] anchor = new double[1]; // the anchor's z when the third turn began

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    back[0] = viewDistance(helper, level, 7);
                    for (Lane l : new Lane[] {a, c})
                        far.forceRect(l.cx0, l.cz - 1, l.cx1, l.cz + 4);
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    for (Lane l : new Lane[] {a, c})
                        l.top = MowzieKit.platform(level, l.x0, l.x1, (l.cz - 1) * 16, (l.cz + 4) * 16 + 15);
                    bandAt(helper, faction, agent[0], a.x0 + 48, a.top + 1, a.z, 0f, List.of(), 1, true);
                    bandAt(helper, faction, agent[1], c.x0 + 36, c.top + 1, c.z, 0f, List.of(), 1, true);
                    hearer[0] = FakePlayers.watching(level, new Vec3(a.x0 + 20, a.top + 1, a.z + 16), "mowzie-repair-near");
                    hearer[1] = FakePlayers.watching(level, new Vec3(a.x0 + 126, a.top + 1, a.z + 16), "mowzie-repair-far", 7);
                    hearer[2] = FakePlayers.watching(level, new Vec3(a.x0 - 147, a.top + 1, a.z + 16), "mowzie-repair-away", 2);
                    hearer[3] = FakePlayers.watching(level, new Vec3(c.x0 + 75, c.top + 1, c.z + 25), "mowzie-repair-control");
                    for (FakePlayers.Hearer h : hearer)
                        MowzieKit.mask(h);
                    // every tick, as a connection's own turn would be: the chunk batches acknowledged, the chunks sent, the tracking re-read
                    MowzieKit.everyTick(helper, () -> {
                        for (FakePlayers.Hearer h : hearer)
                            if (h != null && !h.isRemoved())
                                FakePlayers.pump(h);
                    });
                })
                .thenWaitUntil(waits.logic(() -> {
                    puppet[0] = puppet(agent[0]);
                    puppet[1] = puppet(agent[1]);
                }))
                .thenWaitUntil(waits.chunks(() -> {
                    String range = "its tracking range is " + puppet[0].getType().clientTrackingRange() + " chunks";
                    check("the near hearer is paired with the puppet (" + range + ", " + hearer[0].added(puppet[0].getId()) + " adds)",
                            Tracked.sees(level, puppet[0], hearer[0]) && hearer[0].added(puppet[0].getId()) == 1);
                    check("the far hearer is too (" + hearer[1].added(puppet[0].getId()) + " adds)",
                            Tracked.sees(level, puppet[0], hearer[1]) && hearer[1].added(puppet[0].getId()) == 1);
                    check("the control's hearer is paired with the control's puppet (" + hearer[3].added(puppet[1].getId()) + " adds)",
                            Tracked.sees(level, puppet[1], hearer[3]) && hearer[3].added(puppet[1].getId()) == 1);
                }))
                .thenIdle(5)
                .thenExecute(() -> {
                    EntityUmvuthi u = puppet[0];
                    check("the away hearer is not paired with it and the tracker is one (" + Tracked.count(level, u) + ")",
                            !Tracked.sees(level, u, hearer[2]) && Tracked.count(level, u) == 1);
                    check("the far hearer is more than 64 blocks off (" + Math.hypot(u.getX() - hearer[1].getX(), u.getZ() - hearer[1].getZ()) + ")",
                            Math.hypot(u.getX() - hearer[1].getX(), u.getZ() - hearer[1].getZ()) > 64);
                    check("the away hearer is more than the sight rule's 160 off (" + Math.hypot(u.getX() - hearer[2].getX(), u.getZ() - hearer[2].getZ()) + ")",
                            Math.hypot(u.getX() - hearer[2].getX(), u.getZ() - hearer[2].getZ()) > AloftRules.SIGHT + 10);
                    check("it faces east (" + u.getDirectionData() + ")", u.getDirectionData() == 4);
                    u.addEffect(new MobEffectInstance(EffectHandler.SUNBLOCK, 6000, 0));
                    u.addEffect(new MobEffectInstance(MobEffects.FIRE_RESISTANCE, 6000, 0));
                    u.setGlowingTag(true);
                    u.setHealth(u.getMaxHealth() - 20f);
                    u.setHealthLost(20f);
                    check("it has Sunblock and Fire Resistance (" + u.getActiveEffects() + ")",
                            u.hasEffect(EffectHandler.SUNBLOCK) && u.hasEffect(MobEffects.FIRE_RESISTANCE));
                    Reading r = new Reading(level, u, hearer[0], hearer[0], hearer[1]);
                    reading[0] = r;
                    for (int k = 0; k < r.fired.length; k++) {
                        int kind = k;
                        switch (kind) {
                            case 0 -> listen(helper, EventPriority.LOWEST, EntityJoinLevelEvent.class,
                                    e -> r.fired[kind] = e.getEntity() == u ? level.getGameTime() : r.fired[kind]);
                            case 1 -> listen(helper, EventPriority.LOWEST, EntityLeaveLevelEvent.class,
                                    e -> r.fired[kind] = e.getEntity() == u ? level.getGameTime() : r.fired[kind]);
                            case 2 -> listen(helper, EventPriority.LOWEST, PlayerEvent.StartTracking.class,
                                    e -> r.fired[kind] = e.getTarget() == u ? level.getGameTime() : r.fired[kind]);
                            case 3 -> listen(helper, EventPriority.LOWEST, PlayerEvent.StopTracking.class,
                                    e -> r.fired[kind] = e.getTarget() == u ? level.getGameTime() : r.fired[kind]);
                            default -> listen(helper, EventPriority.LOWEST, LivingDeathEvent.class,
                                    e -> r.fired[kind] = e.getEntity() == u ? level.getGameTime() : r.fired[kind]);
                        }
                    }
                    controlDirection[0] = puppet[1].getDirectionData();
                    r.arm();
                    mark[0] = hearer[0].packets.size();
                    mark[1] = hearer[1].packets.size();
                    counted[0] = ConflictKit.bridge().repairs;
                    counted[1] = ConflictKit.bridge().repairsSkipped;
                    MowzieKit.everyTick(helper, () -> {
                        r.tick();
                        EntityUmvuthi k = puppet[1];
                        if (k.isRemoved() || !k.isAlive())
                            return;
                        control[0]++;
                        long n = hearer[3].added(k.getId());
                        if (n != control[1]) {
                            control[1] = n;
                            if (n > 1 && r.broke.size() < 14)
                                r.broke.add("tick " + level.getGameTime() + ": the control was sent " + n + " adds");
                        }
                        if (controlAt[0] != null && k.position().distanceTo(controlAt[0]) > 0.05)
                            turned[0] = Math.max(turned[0], AloftRules.turn(k.getYRot(), AloftRules.wayYaw(k.getX() - controlAt[0].x, k.getZ() - controlAt[0].z)));
                        controlAt[0] = k.position();
                        if (control[0] == 400) {
                            turned[1] = chiefSlot(agent[1]).lag();
                            control[2] = level.getGameTime();
                        }
                    });
                    at[0] = level.getGameTime();
                    ConflictKit.steer(agent[0], ConflictKit.party(agent[0]).epoch, PACE, a.x0 + 56, a.z, 0, false);
                    // the control: 40 degrees off east, 100 blocks at 0.2 a tick (500 ticks), so the puppet lags the anchor all the while
                    ConflictKit.steer(agent[1], ConflictKit.party(agent[1]).epoch, 0.2f, c.x0 + 36 + 76.6, c.z + 64.3, 0, false);
                })
                .thenIdle(100)
                .thenExecute(() -> {
                    Reading r = reading[0];
                    EntityUmvuthi u = puppet[0];
                    mark[2] = hearer[0].packets.size();
                    mark[3] = hearer[1].packets.size();
                    long near = flames(hearer[0], mark[0], mark[2]), far1 = flames(hearer[1], mark[1], mark[3]);
                    check("the near hearer was sent 9 to 11 small_flame packets with the long-distance flag in 100 ticks (" + near + ")", near >= 9 && near <= 11);
                    check("so was the far one (" + far1 + ")", far1 >= 9 && far1 <= 11);
                    check("no re-pair yet, the body still east (" + r.repairs + ", " + u.getDirectionData() + ")", r.repairs.isEmpty() && u.getDirectionData() == 4);
                    record(test, "trail", "near " + near + ", far " + far1 + " small_flame packets in 100 ticks, away "
                            + hearer[2].count(ClientboundLevelParticlesPacket.class));
                    u.sendAbilityMessage(EntityUmvuthi.ROAR_ABILITY);
                    check("the ROAR started (" + u.getActiveAbility() + ")", u.getActiveAbility() != null);
                    r.roarAt = level.getGameTime();
                    at[1] = r.roarAt;
                    // south, 60 blocks at 0.2: the anchor outruns the puppet, so its way is 90 degrees off its facing at once
                    ConflictKit.steer(agent[0], ConflictKit.party(agent[0]).epoch, 0.2f, a.x0 + 56, a.z + 60, 0, false);
                })
                .thenWaitUntil(waits.logic(() -> check("the first re-pair (the ROAR has lasted " + reading[0].roarTicks + " ticks)", !reading[0].repairs.isEmpty())))
                .thenExecute(() -> {
                    Reading r = reading[0];
                    long r1 = r.repairs.get(0);
                    check("the ROAR was active for at least 20 ticks (" + r.roarTicks + ")", r.roarTicks >= 20);
                    check("it ended (" + r.roarEnd + ")", r.roarEnd != 0);
                    long held = r1 - r.roarEnd;
                    check("the re-pair came 2 to 12 ticks after the ability ended, after two ticks held (" + held + ")", held >= 2 && held <= 12);
                    check("it is facing south now (" + puppet[0].getDirectionData() + ")", puppet[0].getDirectionData() == 1);
                    at[2] = r1;
                })
                .thenWaitUntil(waits.logic(() -> check("100 ticks after the re-pair", level.getGameTime() >= at[2] + 100)))
                .thenExecute(() -> {
                    long now = level.getGameTime();
                    double z = Math.min(a.z + 0.2 * (now - at[1]), a.z + 60); // where the anchor is on its way south
                    at[3] = now;
                    anchor[0] = z;
                    // west, at 0.3: the puppet faces south, so its way is soon 90 degrees off
                    ConflictKit.steer(agent[0], ConflictKit.party(agent[0]).epoch, 0.3f, a.x0 + 4, z, 0, false);
                })
                .thenWaitUntil(waits.logic(() -> check("the second re-pair (" + reading[0].repairs + ")", reading[0].repairs.size() >= 2)))
                .thenExecute(() -> {
                    Reading r = reading[0];
                    long r1 = r.repairs.get(0), r2 = r.repairs.get(1);
                    check("the second turn was due before the cooldown ran out (due " + (r.dueAfter[1] - r1) + " ticks after the first re-pair)",
                            r.dueAfter[1] != 0 && r.dueAfter[1] - r1 < AloftRules.REPAIR_COOLDOWN);
                    check("the second re-pair came 200 to 260 ticks after the first (" + (r2 - r1) + ")", r2 - r1 >= AloftRules.REPAIR_COOLDOWN && r2 - r1 <= 260);
                    check("it is facing west now (" + puppet[0].getDirectionData() + ")", puppet[0].getDirectionData() == 2);
                    long now = level.getGameTime();
                    double z = anchor[0];
                    double x = a.x0 + 56 - 0.3 * (now - at[3]);
                    at[5] = r2;
                    // north, at 0.3: the third turn, due again before the cooldown ends
                    ConflictKit.steer(agent[0], ConflictKit.party(agent[0]).epoch, 0.3f, x, z - 60, 0, false);
                })
                .thenWaitUntil(waits.logic(() -> check("198 ticks after the second re-pair", level.getGameTime() >= at[5] + 198)))
                .thenExecute(() -> {
                    Reading r = reading[0];
                    EntityUmvuthi u = puppet[0];
                    check("only two re-pairs so far (" + r.repairs + ")", r.repairs.size() == 2);
                    check("the third turn is due: its way has been 55 degrees off its facing since " + (level.getGameTime() - r.dueAfter[2]) + " ticks ago",
                            r.dueAfter[2] != 0 && level.getGameTime() - r.dueAfter[2] >= 10);
                    check("the body is alive, with no ability (" + u.isDeadOrDying() + ", " + u.getActiveAbility() + ")", !u.isDeadOrDying() && u.getActiveAbility() == null);
                    r.removedBefore = (int) (hearer[0].removed(u.getId()) + hearer[1].removed(u.getId()));
                    counted[2] = ConflictKit.bridge().repairs - counted[0];
                    counted[3] = ConflictKit.bridge().repairsSkipped - counted[1];
                    at[6] = level.getGameTime();
                    MowzieKit.kill(u, level.damageSources().playerAttack(hearer[0]));
                    check("it is dying (" + u.isDeadOrDying() + ")", u.isDeadOrDying());
                })
                .thenWaitUntil(waits.logic(() -> check("the puppet's animation ended (" + (level.getGameTime() - at[6]) + " ticks)", puppet[0].isRemoved())))
                .thenExecute(() -> {
                    Reading r = reading[0];
                    int id = puppet[0].getId();
                    check("no re-pair came for the dying body (" + r.repairs + ", the kill at " + at[6] + ")", r.repairs.size() == 2);
                    check("each of A's hearers was sent the puppet three times as added: once paired, two re-pairs (" + hearer[0].added(id) + ", " + hearer[1].added(id) + ")",
                            hearer[0].added(id) == 3 && hearer[1].added(id) == 3);
                    check("and never as removed before the kill (" + r.removedBefore + ")", r.removedBefore == 0);
                    check("the bridge counted two re-pairs and none skipped (" + counted[2] + ", " + counted[3] + ")", counted[2] == 2 && counted[3] == 0);
                    check("the away hearer was sent no particle in the whole test (" + hearer[2].count(ClientboundLevelParticlesPacket.class) + ") and no add ("
                            + hearer[2].added(id) + ")", hearer[2].count(ClientboundLevelParticlesPacket.class) == 0 && hearer[2].added(id) == 0);
                    check("the near hearer was sent more flames than the window's (" + flames(hearer[0], 0, Integer.MAX_VALUE) + ")",
                            flames(hearer[0], 0, Integer.MAX_VALUE) > flames(hearer[0], mark[0], mark[2]));
                    check("the server's body and the packets agreed in every re-pair: " + r.broke, r.broke.isEmpty());
                    // the control
                    EntityUmvuthi k = puppet[1];
                    check("the control was read at 400 ticks (" + control[2] + ")", control[2] != 0);
                    check("the control's puppet lagged its anchor at 400 ticks (" + turned[1] + " blocks)", turned[1] > 5);
                    check("its way was 30 to 55 degrees off its facing (" + turned[0] + ")", turned[0] > 30 && turned[0] < AloftRules.TURN);
                    check("it was sent its pairing and no more (" + hearer[3].added(k.getId()) + ")", hearer[3].added(k.getId()) == 1);
                    check("it still faces east (" + k.getDirectionData() + ", was " + controlDirection[0] + ")", k.getDirectionData() == controlDirection[0]);
                    check("no traveller step failed (" + ConflictKit.travellerErrors() + ")", ConflictKit.travellerErrors() == 0);
                    record(test, "repair", "re-pairs at " + r.repairs + " (the ROAR " + r.roarTicks + " ticks); " + String.join("; ", r.facts)
                            + "; the control turned " + String.format("%.1f", turned[0]) + " degrees, lagged " + String.format("%.1f", turned[1]) + " blocks, one add");
                    record(test, "l2hostility", net.neoforged.fml.ModList.get().isLoaded("l2hostility") ? "in the run: its moving traits are not asserted here" : "not in the run");
                    recallAll(agent);
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    back[0].run();
                    for (int i = 0; i < hearer.length; i++) {
                        FakePlayers.removeSeen(hearer[i]);
                        hearer[i] = null;
                    }
                    far.releaseAll();
                })
                .thenSucceed();
    }

    // ---- 33u: the Umvuthi appears beyond 128, and Carry On takes it (MZB-829) --------------------------------------

    /**
     * One column's march on a ticking area of simulation distance {@code s} (test 33u, a copy of core's {@code MarchGameTests.column} for
     * a band that carries its chief): a masked player ({@link Watch}) stands on its chunk's west column, and a band of six warriors and
     * its chief (a stack of its own, at the rear) comes from {@code start} blocks east, on the player's row, at 0.1 blocks a tick. The
     * grove of its faction is declared and out, so that nothing but the band's own rules is under test. Every tick it notes when each stack
     * is first placed and what it stood on then.
     */
    private static final class Column {
        final GameTestHelper helper;
        final ServerLevel level;
        final String test;
        final int s;
        final double start;
        final Watch w;
        final Grove g;
        final long agent = ConflictKit.newAgent();
        final ConflictBridge.Stack[] st = new ConflictBridge.Stack[4];
        final long[] firstSeen = new long[4];
        final double[] anchor = new double[4], near = new double[4], farthest = new double[4];
        final boolean[] ticking = {true, true, true, true};
        boolean floats, watching;
        long wasKilled;
        double yAtTake;
        EntityUmvuthi body, copy;
        Player taker;
        CompoundTag saved;

        Column(GameTestHelper helper, Far far, String test, int s, int index) {
            this.helper = helper;
            this.level = far.level;
            this.test = test;
            this.s = s;
            this.start = s == 8 ? 170 : 200;
            this.w = new Watch(helper, far, s);
            this.g = groveAt(far, index, 0); // declared, never forced: out
        }

        /** Declares the grove, sets the distances and joins the player. */
        void join() {
            g.declare();
            w.start();
        }

        /** Fails until the square of chunks within {@code s} of the player ticks entities and the first chunk past it does not. */
        void ready() {
            w.check();
        }

        /** Stands the player on the ground, masks it, and sends the column in from {@code start} blocks east, heading west. */
        void march() {
            w.ground();
            MowzieKit.mask(w.player);
            int gy = level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, (int) Math.floor(w.px), (int) Math.floor(w.pz));
            record(test, "ticking area " + s, "simulation distance " + s + ", the player's chunk (" + w.pcx + ", " + w.pcz + "), the edge is "
                    + (w.edge() - w.player.getX()) + " blocks east of the player; the column comes from " + start + " blocks");
            bandAt(helper, g.faction, agent, w.px + start, gy, w.pz, 0f, List.of(), 6, true, b -> {
                b.bearing = 180f;
                b.targetX = w.px + 60;
                b.targetZ = w.pz;
                b.speed = 0.1f;
            });
            MowzieKit.everyTick(helper, this::sample);
        }

        void materialized() {
            check("BAND_MATERIALIZED for the band", !ConflictKit.about(ConflictRecords.BAND_MATERIALIZED, agent).isEmpty());
        }

        /** The band is made and none of it stands: the column is outside the area; four stacks, all pending. */
        void pending() {
            List<ScriptedEngine.Received> m = ConflictKit.about(ConflictRecords.BAND_MATERIALIZED, agent);
            check("one BAND_MATERIALIZED, none placed now (aux " + m.get(0).aux() + "): the column is outside the area", m.size() == 1 && m.get(0).aux() == 0);
            check("never rejected (" + ConflictKit.about(ConflictRecords.BAND_SPAWN_REJECTED, agent).size() + ")", ConflictKit.about(ConflictRecords.BAND_SPAWN_REJECTED, agent).isEmpty());
            ConflictBridge.Party p = ConflictKit.party(agent);
            check("three rows and the chief make four stacks (" + p.stacks().size() + ")", p.stacks().size() == 4);
            for (int i = 0; i < 4; i++) {
                st[i] = p.stacks().get(i);
                check("stack " + i + " is a row of " + (i < 3 ? 2 : 1) + " (" + st[i].slots().size() + "), pending, " + (1.5 * i) + " behind the head ("
                        + st[i].back() + ")", st[i].slots().size() == (i < 3 ? 2 : 1) && st[i].state() == ConflictBridge.StackState.PENDING
                        && Math.abs(st[i].back() - 1.5 * i) < 1e-9 && st[i].chief() == (i == 3));
            }
            check("no Umvuthi stands (the chief's body is made when its stack is placed)", level.getEntitiesOfClass(EntityUmvuthi.class, new AABB(w.px - 400, level.getMinBuildHeight(), w.pz - 400, w.px + 400, level.getMaxBuildHeight(), w.pz + 400)).isEmpty());
            watching = true;
        }

        void sample() {
            if (!watching)
                return;
            long now = level.getGameTime();
            for (int i = 0; i < 4; i++) {
                if (firstSeen[i] != 0 || st[i].state() != ConflictBridge.StackState.PLACED)
                    continue;
                firstSeen[i] = now;
                anchor[i] = st[i].anchorAt(now)[0];
                near[i] = Double.MAX_VALUE;
                farthest[i] = -Double.MAX_VALUE;
                for (Mob m : st[i].bodies()) {
                    double d = Math.hypot(m.getX() - w.player.getX(), m.getZ() - w.player.getZ());
                    near[i] = Math.min(near[i], d);
                    farthest[i] = Math.max(farthest[i], d);
                    ticking[i] &= RtsimSession.entitiesTickAt(level, m.blockPosition());
                    if (i == 3)
                        floats = m.isNoGravity();
                }
            }
        }

        void allPlaced() {
            for (int i = 0; i < 4; i++)
                check("stack " + i + " is placed (" + st[i].state() + ")", st[i].state() == ConflictBridge.StackState.PLACED);
        }

        /** Where each stack first stood: the chief's never nearer than 128 blocks, each at the edge of the ticking chunks. */
        void appeared() {
            watching = false;
            double edge = w.edge();
            double lo = 16.0 * s, hi = 16.0 * (s + 1);
            for (int i = 0; i < 4; i++) {
                record(test, "stack " + i + " appeared at " + s, "tick " + firstSeen[i] + ", anchor x " + String.format("%.2f", anchor[i]) + " is "
                        + String.format("%.2f", edge - anchor[i]) + " inside the edge, bodies " + String.format("%.2f to %.2f", near[i], farthest[i])
                        + " blocks from the player (the edge is " + String.format("%.1f", edge - w.player.getX()) + "; the warriors' range " + lo + " to " + hi
                        + (i == 3 ? "; the Umvuthi, never under 128" : "") + ")");
                check("stack " + i + " was placed (tick " + firstSeen[i] + ")", firstSeen[i] != 0);
                check("stack " + i + " was placed with every body in an entity-ticking chunk", ticking[i]);
                check("stack " + i + " was placed the tick its last member was inside: its place is " + (edge - anchor[i]) + " inside the edge " + edge
                        + " (0 to 1 block)", anchor[i] < edge && edge - anchor[i] <= 1.0);
                check("stack " + i + " is placed once (" + st[i].placings() + ")", st[i].placings() == 1);
                if (i > 0)
                    check("front first: stack " + (i - 1) + " at tick " + firstSeen[i - 1] + ", stack " + i + " at " + firstSeen[i], firstSeen[i - 1] < firstSeen[i]);
                if (i < 3)
                    check("the warriors' stack " + i + " appeared " + near[i] + " to " + farthest[i] + " blocks from the player, in " + lo + " to " + hi,
                            near[i] >= lo && farthest[i] <= hi);
            }
            check("the Umvuthi appeared " + near[3] + " blocks from the player, never under 128", near[3] >= 128);
            check("the Umvuthi appeared " + near[3] + " to " + farthest[3] + " blocks from the player, in " + lo + " to " + hi, near[3] >= lo && farthest[3] <= hi);
            check("it floated from its first sight", floats);
            check("still never rejected", ConflictKit.about(ConflictRecords.BAND_SPAWN_REJECTED, agent).isEmpty());
        }

        /** A creative player picks the puppet up, as Carry On's handler does: it is an ordinary Umvuthi at once, with its gravity on. */
        void take() {
            body = puppet(agent);
            check("it floats until it is taken", body.isNoGravity());
            yAtTake = body.getY();
            wasKilled = ConflictKit.bridge().chiefsKilled;
            taker = helper.makeMockPlayer(GameType.CREATIVE);
            check("Carry On's pickup takes it", RtsimApi.takeFromSim(body, TakeReason.PICKUP, taker));
            check("it is no puppet at once", !RtsimApi.isPuppet(body));
            check("its gravity is on at once", !body.isNoGravity());
            check("it is no member of the bridge's", ConflictKit.bridge().slotOf(body) == null);
            check("it is alive in the world", body.isAlive() && !body.isRemoved());
            CompoundTag nbt = new CompoundTag();
            check("it saves", body.save(nbt));
            check("its gravity is not off in its saved NBT", !nbt.getBoolean("NoGravity"));
            saved = nbt;
        }

        /** The pickup is booked: one CHIEF_KILLED, by a player, naming the one who took it. */
        void booked() {
            List<ScriptedEngine.Received> k = ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent);
            check("one CHIEF_KILLED is journaled (" + k.size() + ")", k.size() == 1);
            check("by a player (" + k.get(0).aux() + ")", k.get(0).aux() == ConflictRecords.BY_PLAYER);
            check("naming the player who took it", taker.getUUID().equals(ConflictKit.uuidAt(k.get(0), ConflictRecords.ChiefKilled.KILLER_MOST)));
        }

        /** The player puts it down: a body made from the saved NBT joins the level, as Carry On's does. */
        void putDown() {
            Entity e = EntityType.create(saved, level).orElse(null);
            check("a body made from the saved NBT is an Umvuthi", e instanceof EntityUmvuthi);
            copy = (EntityUmvuthi) e;
            copy.setUUID(UUID.randomUUID());
            check("it joins the level", level.addFreshEntity(copy));
        }

        /** Put down, it is an ordinary Umvuthi, a stray: it falls, floats no more, and nothing of the bridge's holds it. */
        void ordinary() {
            for (EntityUmvuthi u : new EntityUmvuthi[] {body, copy}) {
                String what = u == body ? "the taken body" : "the copy put down";
                check(what + " is alive in the world", u.isAlive() && !u.isRemoved());
                check(what + " floats no more (gravity " + !u.isNoGravity() + ")", !u.isNoGravity());
                check(what + " is no puppet", !RtsimApi.isPuppet(u));
                check(what + " is no member of the bridge's", ConflictKit.bridge().slotOf(u) == null);
                check(what + " is no higher than where it was taken (" + u.getY() + " from " + yAtTake + ")", u.getY() <= yAtTake + 0.01);
            }
            check("one CHIEF_KILLED still (" + ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent).size() + ")", ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent).size() == 1);
            record(test, "taken at " + s, "taken from " + String.format("%.2f", yAtTake) + " over, chiefsKilled " + wasKilled + " to " + ConflictKit.bridge().chiefsKilled
                    + "; the body and a copy made from its NBT are ordinary Umvuthi that fell to " + String.format("%.2f and %.2f", body.getY(), copy.getY()));
            copy.discard();
            body.discard();
            recallAll(agent);
        }
    }

    /**
     * The Umvuthi appears no nearer than the ticking area's edge, and a creative player's pickup lets it go (test 33u, test 33's Umvuthi
     * clauses; MZB-829). A masked player ({@link Watch}) stands at the west of its ticking area, at simulation distance 8 and then 10 (view
     * distance two more; each restored inside the sequence). A band of six warriors and its chief comes in from 170 (200) blocks east, on
     * the player's row, at 0.1 blocks a tick: three rows of two and the chief's stack at the rear, four stacks, as M10's test 33 has them.
     * <ul>
     * <li>The band is made and none of it stands. The stacks appear one by one, front first, each in the tick its place is inside the
     * ticking chunks (its anchor within a block of the edge), every body in an entity-ticking chunk. Each stack's distance from the
     * player is logged; the Umvuthi's is never under 128 and, like the warriors' (the control), in 128 to 144 (160 to 176 at 10). It floats
     * from its first sight.</li>
     * <li>A creative mock player takes the puppet with {@code RtsimApi.takeFromSim(body, TakeReason.PICKUP, player)}, the call Carry On's
     * handler makes (as FrostmawGameTests' pickup does): the body is no puppet at once, its gravity is on, it is no member of the
     * bridge's, and its saved NBT does not say NoGravity. One CHIEF_KILLED names that player, by a player.</li>
     * <li>Put down (a body made from that NBT joins the level), it is an ordinary Umvuthi, a stray: 40 ticks on it is alive, no puppet, no
     * member, with its gravity on and no higher than it was taken. The taken body does the same.</li>
     * </ul>
     * Fails without the chief's stack's pending test (it appears nearer than 128 at distance 8, or not in a ticking chunk), and without
     * {@code gone}'s release (put down, it floats).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "aloft_beyond_128", skyAccess = true)
    public static void theUmvuthiAppearsBeyond128AndCarryOnTakesIt(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 6000);
        String test = "theUmvuthiAppearsBeyond128AndCarryOnTakesIt";
        Far far = new Far(helper, test);
        // Both watches take the server's distances now, before either sets its own, so each puts the original back.
        Column[] cs = {new Column(helper, far, test, 8, 0), new Column(helper, far, test, 10, 1)};

        MowzieKit.Seq seq = MowzieKit.seq(helper);
        for (Column c : cs)
            seq.thenExecute(c::join)
                    .thenWaitUntil(waits.chunks(c::ready))
                    .thenExecute(c::march)
                    .thenWaitUntil(waits.logic(c::materialized))
                    .thenExecute(c::pending)
                    .thenWaitUntil(waits.logic(c::allPlaced))
                    .thenIdle(60)
                    .thenExecute(c::appeared)
                    .thenExecute(c::take)
                    .thenWaitUntil(waits.logic(c::booked))
                    .thenExecute(c::putDown)
                    .thenIdle(40)
                    .thenExecute(c::ordinary)
                    .thenWaitUntil(waits.logic(() -> check("the band went (" + dematerialized(c.agent).size() + ")", dematerialized(c.agent).size() >= 1)))
                    .thenExecute(() -> {
                        c.w.end();
                        c.g.far.releaseAll();
                    });
        seq.thenSucceed();
    }
}
