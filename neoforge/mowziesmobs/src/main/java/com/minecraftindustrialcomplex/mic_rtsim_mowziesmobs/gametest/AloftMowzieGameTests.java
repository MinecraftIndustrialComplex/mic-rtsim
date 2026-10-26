package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest;

import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaMinion;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthi;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
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
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.animal.IronGolem;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.level.ExplosionDamageCalculator;
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
import net.neoforged.neoforge.event.entity.EntityTravelToDimensionEvent;
import net.neoforged.neoforge.event.entity.living.LivingDeathEvent;
import net.neoforged.neoforge.event.tick.LevelTickEvent;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;
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
            this.far = far;
            level = far.level;
            cx0 = far.k;
            cx1 = far.k + chunks - 1;
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
}
