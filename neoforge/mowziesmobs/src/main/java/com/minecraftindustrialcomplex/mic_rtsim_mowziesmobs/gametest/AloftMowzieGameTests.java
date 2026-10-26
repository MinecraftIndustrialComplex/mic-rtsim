package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest;

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
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.MicRtsimMowziesMobs;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.UmvuthiAloft;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Far;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Grove;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.mixin.EntityUmvuthiAccessor;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.animal.IronGolem;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.LeavesBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.bus.api.Event;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.living.LivingDeathEvent;
import net.neoforged.neoforge.event.tick.LevelTickEvent;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;
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
                    check("its health is 250 (" + u.getHealth() + ")", u.getHealth() == 250f);
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
}
