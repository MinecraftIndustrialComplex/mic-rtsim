package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest;

import com.bobmowzie.mowziesmobs.server.entity.EntityHandler;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaMinion;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaRaptor;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthi;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.MaskType;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Camps;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.MarchPath;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ChunkWait;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.CampLine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.GroveKind;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.MicRtsimMowziesMobs;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Far;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Grove;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Markers;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.level.GameType;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The march's GameTests that need the jar's bodies (MOWZIE.md slice M10c, §7 items 22, 28 and 32; docs/choices-mowzie-build.md,
 * MZB-740 on): a band that walks home in view and is handed back to its grove, a band that is handed back to a grove that went
 * out, a grove that comes in with its march's bodies left behind, and a band that sets out and comes home in a player's plain view.
 * They script the engine's side ({@link ConflictKit}) for test agents and groves under names no culture claims (MZB-227), in a far
 * place of their own ({@link MowzieKit.Far}). Every test logs what it compared on a {@code [gametest]} line, and every failure
 * message names the values, positions and ticks compared. A chunk wait is a {@link ChunkWait} one on its two clocks.
 *
 * <p>What these tests cannot write is listed with each: the Umvuthi's puppet and its throne part are M11's; a march that is
 * driven by the engine (the Invasion reaching its target, the pool plus away staying at its cap) is the engine's own test.
 */
@GameTestHolder(MicRtsimMowziesMobs.MODID)
@PrefixGameTestTemplate(false)
public final class MarchMowzieGameTests {
    private MarchMowzieGameTests() {}

    private static final String ARENA = RtsimTests.TEMPLATE;

    private static final int DONE = ConflictRecords.CampBodies.DONE, STALE = ConflictRecords.CampBodies.STALE;
    private static final int SPAWN = ConflictRecords.CampReconcile.SPAWN, ADOPT = ConflictRecords.CampReconcile.ADOPT,
            TAKE = ConflictRecords.CampReconcile.TAKE, SET_OUT = ConflictRecords.CampReconcile.SET_OUT,
            HAND_BACK = ConflictRecords.CampReconcile.HAND_BACK;
    private static final int CHIEF_SLOT = ConflictRecords.CampReconcile.CHIEF_SLOT, LINE_CHIEF = ConflictRecords.CampReconcile.LINE_CHIEF;
    private static final ConflictBridge.StackState PLACED = ConflictBridge.StackState.PLACED;

    /** A band's walk, blocks a tick (the stack's anchor; the puppets follow it). */
    private static final float WALK = 0.2f;
    /** The distance a watching player stands from a band: inside a warrior puppet's sight (ConflictBridge.sightRange). */
    private static final double WATCH = 50.0;

    // ---- helpers ---------------------------------------------------------------------------------

    /** The states of a party's stacks, for a failure message. */
    private static String states(ConflictBridge.Party p) {
        StringBuilder out = new StringBuilder("[");
        for (ConflictBridge.Stack st : p.stacks())
            out.append(out.length() > 1 ? ", " : "").append(st.index).append(' ').append(st.state()).append(" x").append(st.placings());
        return out.append(']').toString();
    }

    private static String xz(double[] a) {
        return String.format("(%.2f, %.2f)", a[0], a[1]);
    }

    private static String at(Vec3 v) {
        return String.format("(%.1f, %.1f, %.1f)", v.x, v.y, v.z);
    }

    /** The distance on the ground. */
    private static double flat(Vec3 a, Vec3 b) {
        return Math.hypot(a.x - b.x, a.z - b.z);
    }

    private static double flat(double x, double z, Vec3 b) {
        return Math.hypot(x - b.x, z - b.z);
    }

    /**
     * Inside {@code thenWaitUntil}: the band's stacks are all placed and {@code n} members are embodied; the slots, in stack and
     * slot order (the order a HAND_BACK takes its puppets in).
     */
    private static List<ConflictBridge.Slot> embodied(long agent, int n) {
        ConflictBridge.Party p = ConflictKit.party(agent);
        List<ConflictBridge.Slot> out = new ArrayList<>();
        check("the band has a stack (" + states(p) + ")", !p.stacks().isEmpty());
        for (ConflictBridge.Stack st : p.stacks()) {
            check("stack " + st.index + " is placed, " + states(p), st.state() == PLACED);
            for (ConflictBridge.Slot s : st.slots())
                if (s.alive() && s.entity() != null && !s.entity().isRemoved())
                    out.add(s);
        }
        check(n + " members stand (" + out.size() + "; " + states(p) + ")", out.size() == n);
        return out;
    }

    /** The slots of one body kind, in the order given. */
    private static List<ConflictBridge.Slot> of(List<ConflictBridge.Slot> all, String body) {
        List<ConflictBridge.Slot> out = new ArrayList<>();
        for (ConflictBridge.Slot s : all)
            if (s.body.equals(body))
                out.add(s);
        return out;
    }

    /** A waypoint of a march record: block x and z, and the game tick the band reaches it. */
    private static long[] wp(double x, double z, long tick) {
        return new long[] {(long) Math.floor(x), (long) Math.floor(z), tick};
    }

    /** What a march record holds, in one line (a march the world did not change prints as it did). */
    private static String snapshot(MarchPath m) {
        if (m == null)
            return "no march";
        StringBuilder s = new StringBuilder("revision ").append(m.revision()).append(", phase ").append(m.phase()).append(", flags ")
                .append(m.flags()).append(", kind ").append(m.kind()).append(", muster end ").append(m.musterEnd()).append(", units ")
                .append(m.units()).append(", waypoints");
        for (int i = 0; i < m.waypoints(); i++)
            s.append(" (").append(m.waypointX(i)).append(", ").append(m.waypointZ(i)).append(" at ").append(m.waypointTick(i)).append(')');
        return s.toString();
    }

    /**
     * The jar's warriors of an area seen at the end of every tick, for "no body appears or vanishes": each tick's difference from the
     * last is an appearance (a new object) or a vanishing (an object gone), with where it stood, its state and how far it was
     * from the watching player. {@link #pair} matches a vanishing and an appearance that are one swap (a body handed to its puppet
     * or the other way): within two ticks and six blocks.
     */
    private static final class Swaps {
        record Snap(UUID id, double x, double z, float health, MaskType mask, int weapon, boolean puppet, double away) {}

        record Event(long tick, boolean appeared, Snap s) {}

        record Swap(Event gone, Event came) {
            double apart() {
                return Math.hypot(came.s().x() - gone.s().x(), came.s().z() - gone.s().z());
            }
        }

        record Pairs(List<Swap> swaps, List<Event> loose) {}

        private final ServerLevel level;
        private final AABB area;
        private final Vec3 viewer;
        private Map<UUID, Snap> last = new LinkedHashMap<>();
        final List<Event> events = new ArrayList<>();

        Swaps(GameTestHelper helper, ServerLevel level, AABB area, Vec3 viewer) {
            this.level = level;
            this.area = area;
            this.viewer = viewer;
            MowzieKit.everyTick(helper, this::tick);
        }

        private void tick() {
            long now = level.getGameTime();
            Map<UUID, Snap> cur = new LinkedHashMap<>();
            for (EntityUmvuthanaMinion e : level.getEntitiesOfClass(EntityUmvuthanaMinion.class, area, x -> !x.isRemoved()))
                if (e.getType() == EntityHandler.UMVUTHANA_MINION.get())
                    cur.put(e.getUUID(), new Snap(e.getUUID(), e.getX(), e.getZ(), e.getHealth(), e.getMaskType(), e.getWeapon(),
                            RtsimApi.isPuppet(e), Math.hypot(e.getX() - viewer.x, e.getZ() - viewer.z)));
            for (Map.Entry<UUID, Snap> c : cur.entrySet())
                if (!last.containsKey(c.getKey()))
                    events.add(new Event(now, true, c.getValue()));
            for (Map.Entry<UUID, Snap> l : last.entrySet())
                if (!cur.containsKey(l.getKey()))
                    events.add(new Event(now, false, l.getValue()));
            last = cur;
        }

        /** The events from tick {@code since} on: the swaps, and what is left over. */
        Pairs pair(long since) {
            List<Event> gone = new ArrayList<>(), came = new ArrayList<>();
            for (Event e : events)
                if (e.tick() >= since)
                    (e.appeared() ? came : gone).add(e);
            List<Swap> swaps = new ArrayList<>();
            List<Event> loose = new ArrayList<>();
            for (Event g : gone) {
                Event best = null;
                double nearest = 6.0;
                for (Event c : came) {
                    if (Math.abs(c.tick() - g.tick()) > 2)
                        continue;
                    double d = Math.hypot(c.s().x() - g.s().x(), c.s().z() - g.s().z());
                    if (d <= nearest) {
                        nearest = d;
                        best = c;
                    }
                }
                if (best == null) {
                    loose.add(g);
                } else {
                    came.remove(best);
                    swaps.add(new Swap(g, best));
                }
            }
            loose.addAll(came);
            return new Pairs(swaps, loose);
        }

        static String describe(Pairs p) {
            StringBuilder s = new StringBuilder();
            for (Swap w : p.swaps())
                s.append(String.format("swap at tick %d (%.1f, %.1f), %.1f apart, %.0f from the player; ", w.gone().tick(), w.gone().s().x(),
                        w.gone().s().z(), w.apart(), w.gone().s().away()));
            for (Event e : p.loose())
                s.append(String.format("%s at tick %d (%.1f, %.1f), %.0f from the player, %s; ", e.appeared() ? "appearance" : "vanishing",
                        e.tick(), e.s().x(), e.s().z(), e.s().away(), e.s().puppet() ? "a puppet" : "a body"));
            return s.toString();
        }
    }

    // ---- 32: a band walks home, and a grove that is out ---------------------------------------------

    /**
     * A band that walks to a grove that is in and is handed back there (MOWZIE.md §4.9, a homecoming; §7 test 32, the in-view half).
     * Three warriors and a raptor pack are placed 25 blocks west of the grove's centre with a player 50 blocks off (a seen player:
     * the walk home goes on in view); the engine steers the band to the centre and each stack's anchor arrives. Then four HAND_BACK
     * lines of the warrior body, one for each slot, come at the grove's epoch:
     * <ul>
     * <li>the walk itself moved the head puppet about 25 blocks, the band did not recall and did not go;</li>
     * <li>lines 0 to 2 hand the three warrior puppets back where they stand: three bodies tagged (faction, slot, 1), no puppet, persistent
     * and saved, restricted 25 round the centre, within four blocks of where its puppet stood, with its health (set to 9, 11 and
     * 13), mask and weapon; each puppet is removed; line 3 has no puppet left (the pack is a raptor's) and so spawns a warrior 14 to
     * 21 blocks from the centre on the side away from the player (counters: handed back 3, spawned 1);</li>
     * <li>what is left of the band, the raptor pack, is recalling toward the centre and stays 40 ticks while the player is in view (no
     * BAND_DEMATERIALIZED);</li>
     * <li>the player gone, it goes the beat after: BAND_DEMATERIALIZED, reason 1, survivors one raptor, its pack removed, no party, and the
     * four camp bodies still alive.</li>
     * </ul>
     * Not written: the Umvuthi's puppet and its throne (M11), so the chief is no part of the band; and "a band met on its way home is
     * embodied", which is the column's test 33 (core).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "mowzie_march_home", skyAccess = true)
    public static void aBandWalksHomeInViewAndIsHandedBack(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 6000);
        String test = "aBandWalksHomeInViewAndIsHandedBack";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove grove = new Grove(far, far.k, far.z0);
        long[] agent = new long[1], op = new long[1], count = new long[2], tick = new long[2];
        ServerPlayer[] watcher = new ServerPlayer[1];
        Vec3[] head = new Vec3[1], last = new Vec3[3];
        Mob[] puppet = new Mob[3], raptor = new Mob[1];
        List<Mob> pack = new ArrayList<>();
        float[] health = {9f, 11f, 13f};
        MaskType[] mask = {MaskType.FURY, MaskType.FEAR, MaskType.RAGE};
        int[] weapon = new int[3];

        MowzieKit.seq(helper)
                .thenExecute(grove::force)
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    grove.lay().declare();
                    watcher[0] = MowzieKit.player(helper, grove.spot.add(0, 1, WATCH), "mowzie-home-watcher");
                    agent[0] = ConflictKit.newAgent();
                    ConflictKit.Band b = ConflictKit.band(agent[0], grove.faction, RtsimTests.dimension(helper));
                    b.arcX = grove.spot.x - 25;
                    b.arcZ = grove.spot.z;
                    b.bearing = 0f;
                    b.targetX = b.arcX;
                    b.targetY = grove.spot.y;
                    b.targetZ = b.arcZ;
                    b.speed = 0f;
                    ConflictKit.materialize(b, List.of(new ConflictKit.StackSpec(MowzieKit.WARRIOR, 3, false),
                            new ConflictKit.StackSpec(MowzieKit.RAPTOR, 1, false)));
                })
                .thenWaitUntil(waits.logic(() -> {
                    check("the grove's census 1 (" + MowzieKit.censusCount(grove.faction) + ")", MowzieKit.censusCount(grove.faction) >= 1);
                    embodied(agent[0], 4);
                }))
                .thenExecute(() -> {
                    ConflictBridge.Party party = ConflictKit.party(agent[0]);
                    List<Mob> ws = new ArrayList<>();
                    for (ConflictBridge.Slot s : of(embodied(agent[0], 4), MowzieKit.WARRIOR))
                        ws.add(s.entity());
                    check("three warrior puppets (" + ws.size() + ")", ws.size() == 3);
                    for (int i = 0; i < 3; i++) {
                        check("warrior puppet " + i + " is the jar's warrior and a puppet", ws.get(i) instanceof EntityUmvuthanaMinion
                                && RtsimApi.isPuppet(ws.get(i)));
                        EntityUmvuthanaMinion w = (EntityUmvuthanaMinion) ws.get(i);
                        w.setHealth(health[i]);
                        w.setMask(mask[i]);
                    }
                    head[0] = ws.get(0).position();
                    check("the band is placed west of the centre, not yet walking: head at " + at(head[0]) + ", centre " + at(grove.spot),
                            head[0].x < grove.spot.x - 15);
                    // The engine steers the band to the grove's centre: every stack walks to its place beside it.
                    ConflictKit.steer(agent[0], party.epoch, WALK, grove.spot.x, grove.spot.z, 0, false);
                })
                .thenWaitUntil(waits.logic(() -> {
                    ConflictBridge.Party p = ConflictKit.party(agent[0]);
                    long now = level.getGameTime();
                    for (ConflictBridge.Stack st : p.stacks()) {
                        double[] a = st.anchorAt(now), e = st.target();
                        check("stack " + st.index + " has not arrived: anchor " + xz(a) + ", end " + xz(e) + ", tick " + now,
                                Math.hypot(a[0] - e[0], a[1] - e[1]) < 0.3);
                    }
                }))
                .thenIdle(20)
                .thenExecute(() -> {
                    ConflictBridge.Party party = ConflictKit.party(agent[0]);
                    check("the band is not recalling after its walk", !party.recalling());
                    check("and has not gone", ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, agent[0]).isEmpty());
                    List<ConflictBridge.Slot> all = embodied(agent[0], 4);
                    List<ConflictBridge.Slot> ws = of(all, MowzieKit.WARRIOR), rs = of(all, MowzieKit.RAPTOR);
                    check("three warriors and a raptor (" + ws.size() + ", " + rs.size() + ")", ws.size() == 3 && rs.size() == 1);
                    double walked = flat(ws.get(0).entity().position(), head[0]);
                    record(test, "walk", "the head puppet walked " + walked + " blocks from " + at(head[0]) + " to " + at(ws.get(0).entity().position())
                            + "; the stacks' ends " + states(party));
                    check("the head puppet walked about 25 blocks (" + walked + ")", walked >= 15 && walked <= 35);
                    for (int i = 0; i < 3; i++) {
                        puppet[i] = ws.get(i).entity();
                        EntityUmvuthanaMinion w = (EntityUmvuthanaMinion) puppet[i];
                        last[i] = w.position();
                        health[i] = w.getHealth();
                        mask[i] = w.getMaskType();
                        weapon[i] = w.getWeapon();
                        check("puppet " + i + " kept its mask through the walk (" + mask[i] + ")", mask[i] == w.getMaskType());
                    }
                    raptor[0] = rs.get(0).entity();
                    check("the raptor stack's puppet is the jar's raptor", raptor[0] instanceof EntityUmvuthanaRaptor);
                    pack.addAll(rs.get(0).extras());
                    check("with its pack of 4 (" + pack.size() + ")", pack.size() == 4);
                    count[0] = MowzieKit.camps().handedBack;
                    count[1] = MowzieKit.camps().spawned;
                    tick[0] = level.getGameTime();
                    List<CampLine> lines = new ArrayList<>();
                    for (int i = 0; i < 4; i++)
                        lines.add(CampLine.party(i, 1, MowzieKit.WARRIOR, HAND_BACK, 0, agent[0], party.epoch));
                    op[0] = MowzieKit.reconcile(grove.faction, grove.site().epoch(), lines);
                })
                .thenWaitUntil(waits.logic(() -> MowzieKit.answered(grove.faction, op[0], 4)))
                .thenExecute(() -> {
                    Camps camps = MowzieKit.camps();
                    MowzieKit.allOutcomes("the four HAND_BACK lines", ConflictKit.outcomes(grove.faction, op[0]), DONE);
                    check("three puppets handed back (" + (camps.handedBack - count[0]) + ")", camps.handedBack - count[0] == 3);
                    check("and one body spawned, for the line that had no puppet (" + (camps.spawned - count[1]) + ")",
                            camps.spawned - count[1] == 1);
                    check("the camp holds four bodies (" + MowzieKit.held(grove.faction).size() + ")", MowzieKit.held(grove.faction).size() == 4);
                    StringBuilder log = new StringBuilder();
                    for (int i = 0; i < 3; i++) {
                        List<Entity> h = MowzieKit.held(grove.faction, i, 1);
                        check("slot " + i + " holds one body, tagged (faction, " + i + ", 1) (" + h.size() + ")", h.size() == 1);
                        check("body " + i + " is a warrior and no puppet", h.get(0) instanceof EntityUmvuthanaMinion && !RtsimApi.isPuppet(h.get(0)));
                        EntityUmvuthanaMinion b = (EntityUmvuthanaMinion) h.get(0);
                        double from = b.position().distanceTo(last[i]);
                        log.append(String.format("body %d %.2f blocks from its puppet, health %.1f (puppet %.1f); ", i, from, b.getHealth(), health[i]));
                        check("body " + i + " is persistence-required and saved", b.isPersistenceRequired() && b.shouldBeSaved());
                        check("body " + i + " is restricted " + GroveKind.RESTRICT + " round the centre (radius " + b.getRestrictRadius() + ")",
                                b.hasRestriction() && b.getRestrictRadius() == GroveKind.RESTRICT
                                        && b.getRestrictCenter().equals(BlockPos.containing(grove.spot)));
                        check("body " + i + " stands where its puppet stood (" + from + " blocks on, at " + at(b.position()) + ")", from < 4);
                        check("with its health (" + b.getHealth() + ", the puppet's " + health[i] + ")", Math.abs(b.getHealth() - health[i]) < 1.5f);
                        check("its mask and weapon (" + b.getMaskType() + " " + b.getWeapon() + ", the puppet's " + mask[i] + " " + weapon[i] + ")",
                                b.getMaskType() == mask[i] && b.getWeapon() == weapon[i]);
                        check("puppet " + i + " is removed (" + puppet[i].getRemovalReason() + ")", puppet[i].isRemoved());
                    }
                    List<Entity> h3 = MowzieKit.held(grove.faction, 3, 1);
                    check("slot 3 holds one spawned body (" + h3.size() + ")", h3.size() == 1 && h3.get(0) instanceof EntityUmvuthanaMinion);
                    Entity b3 = h3.get(0);
                    double d3 = flat(b3.position(), grove.spot), dz3 = b3.getZ() - grove.spot.z;
                    log.append(String.format("spawned body %.1f from the centre, %.1f south of it (the player stands %.0f north); ", d3, dz3, WATCH));
                    check("the spawned warrior is 14 to 21 blocks from the centre (" + d3 + ")", d3 >= 14 && d3 <= 21);
                    check("on the side away from the player (dz " + dz3 + ", the player at +" + WATCH + ")", dz3 < -8);
                    ConflictBridge.Party party = ConflictKit.bridge().party(agent[0]);
                    check("the band is still there and recalling (" + party + ")", party != null && party.recalling());
                    int placed = 0;
                    for (ConflictBridge.Stack st : party.stacks())
                        if (st.state() == PLACED) {
                            placed++;
                            double[] t = st.target();
                            double off = flat(t[0], t[1], grove.spot);
                            log.append(String.format("the pack's stack walks home to %s, %.1f from the centre; ", xz(t), off));
                            check("the pack's stack walks home to the centre (end " + xz(t) + ", " + off + " from it)", off < 12);
                        }
                    check("only the raptor stack is still placed (" + states(party) + ")", placed == 1);
                    check("the raptor and its pack stand", !raptor[0].isRemoved() && raptor[0].isAlive());
                    for (Mob f : pack)
                        check("a follower of the pack stands (" + f.getRemovalReason() + ")", !f.isRemoved());
                    check("no BAND_DEMATERIALIZED yet", ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, agent[0]).isEmpty());
                    record(test, "hand back", log.toString());
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    ConflictBridge.Party party = ConflictKit.bridge().party(agent[0]);
                    check("40 ticks on with the player in view the band is still recalling (" + party + ")", party != null && party.recalling());
                    check("its raptor stands (" + raptor[0].getRemovalReason() + ")", !raptor[0].isRemoved());
                    check("and it has not gone", ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, agent[0]).isEmpty());
                    tick[1] = level.getGameTime();
                    FakePlayers.remove(watcher[0]);
                })
                .thenWaitUntil(waits.logic(() -> check("the band goes once the player does",
                        !ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, agent[0]).isEmpty())))
                .thenExecute(() -> {
                    ScriptedEngine.Received done = ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, agent[0]).get(0);
                    Map<String, Integer> left = ConflictKit.survivors(done);
                    long took = level.getGameTime() - tick[1];
                    record(test, "gone", "BAND_DEMATERIALIZED " + took + " ticks after the player left, reason " + done.aux() + ", survivors " + left);
                    check("the reason is the command's, 1 (" + done.aux() + ")", done.aux() == ConflictRecords.Dematerialized.COMMAND);
                    check("its survivors are the one raptor (" + left + ")", left.equals(Map.of(MowzieKit.RAPTOR, 1)));
                    check("it went within a few ticks of the player's going (" + took + ")", took <= 5);
                    check("the raptor is out of the world (" + raptor[0].getRemovalReason() + ")", raptor[0].isRemoved());
                    for (Mob f : pack)
                        check("so is each of its pack (" + f.getRemovalReason() + ")", f.isRemoved());
                    check("the party is gone", ConflictKit.bridge().party(agent[0]) == null);
                    check("no casualty was booked", ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent[0]).isEmpty());
                    check("the four camp bodies are alive (" + MowzieKit.held(grove.faction).size() + ")", MowzieKit.held(grove.faction).size() == 4);
                })
                .thenSucceed();
    }

    /**
     * A grove that is out when its band is handed back (§7 test 32, the out half), and the next census (MZB-464, MZB-211). The grove's
     * own chunks are let go while a road of forced chunks keeps a band 100 blocks east of it ticking, with a player 45 blocks from the
     * band. The grove goes out (CAMP_OUT); two HAND_BACK lines at the epoch it had are both answered STALE (no body, the counter
     * unmoved), and the band is still sent home: recalling, each stack's end beside the grove's centre (the homecoming of a site, not
     * of the band's own front), the puppets walking toward it, and staying while the player is in view (no BAND_DEMATERIALIZED).
     * The player gone, the band goes the beat after (reason 1, two warriors as survivors, both puppets out of the world). The grove
     * comes in again: a new epoch and a census 2 that lists no body, and two SPAWN lines at the new epoch are DONE, two bodies tagged
     * (faction, 0 or 1, 1), 2 to 11 blocks from the centre (no player).
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "mowzie_march_out", skyAccess = true)
    public static void aGroveThatIsOutRecallsItsBandAndTheNextCensusSpawnsItsBodies(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 6000);
        String test = "aGroveThatIsOutRecallsItsBandAndTheNextCensusSpawnsItsBodies";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove grove = new Grove(far, far.k, far.z0);
        long[] agent = new long[1], op = new long[3], epoch = new long[2], count = new long[3], tick = new long[2];
        ServerPlayer[] watcher = new ServerPlayer[1];
        Mob[] puppet = new Mob[2];
        double[] x0 = new double[2];

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    grove.force();
                    // The road: the chunks the band stands in, 5 to 8 chunks east, apart from the grove's so that releasing the grove's
                    // takes its footprint out (the grove's east edge is three chunks from the road's first chunk, level 34).
                    far.forceRect(grove.cx + 5, grove.cz, grove.cx + 8, grove.cz);
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    grove.lay().declare();
                    watcher[0] = MowzieKit.player(helper, grove.spot.add(100, 1, 45), "mowzie-out-watcher");
                    agent[0] = ConflictKit.newAgent();
                    ConflictKit.Band b = ConflictKit.band(agent[0], grove.faction, RtsimTests.dimension(helper));
                    b.arcX = grove.spot.x + 100;
                    b.arcZ = grove.spot.z;
                    b.bearing = 180f;
                    b.targetX = b.arcX;
                    b.targetY = grove.spot.y;
                    b.targetZ = b.arcZ;
                    b.speed = WALK;
                    ConflictKit.materialize(b, List.of(new ConflictKit.StackSpec(MowzieKit.WARRIOR, 2, false)));
                })
                .thenWaitUntil(waits.logic(() -> {
                    check("the grove's census 1 (" + MowzieKit.censusCount(grove.faction) + ")", MowzieKit.censusCount(grove.faction) >= 1);
                    embodied(agent[0], 2);
                }))
                .thenExecute(() -> {
                    List<ConflictBridge.Slot> m = embodied(agent[0], 2);
                    for (int i = 0; i < 2; i++) {
                        puppet[i] = m.get(i).entity();
                        x0[i] = puppet[i].getX();
                    }
                    epoch[0] = MowzieKit.epoch(MowzieKit.census(grove.faction, 1));
                    count[0] = MowzieKit.camps().handedBack;
                    far.releaseRect(grove.cx - 2, grove.cz - 2, grove.cx + 2, grove.cz + 2);
                })
                .thenWaitUntil(waits.logic(() -> check("the grove went out (" + ConflictKit.campOuts(grove.faction).size() + " CAMP_OUT)",
                        ConflictKit.campOuts(grove.faction).size() == 1)))
                .thenExecute(() -> {
                    check("the band is still placed on the road (" + states(ConflictKit.party(agent[0])) + ")",
                            ConflictKit.party(agent[0]).stacks().get(0).state() == PLACED);
                    List<CampLine> lines = new ArrayList<>();
                    for (int i = 0; i < 2; i++)
                        lines.add(CampLine.party(i, 1, MowzieKit.WARRIOR, HAND_BACK, 0, agent[0], ConflictKit.party(agent[0]).epoch));
                    op[0] = MowzieKit.reconcile(grove.faction, epoch[0], lines);
                })
                .thenWaitUntil(waits.logic(() -> MowzieKit.answered(grove.faction, op[0], 2)))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("HAND_BACK lines of a grove that is out", ConflictKit.outcomes(grove.faction, op[0]), STALE);
                    check("nothing was handed back (" + (MowzieKit.camps().handedBack - count[0]) + ")", MowzieKit.camps().handedBack == count[0]);
                    check("the camp holds no body (" + MowzieKit.held(grove.faction).size() + ")", MowzieKit.held(grove.faction).isEmpty());
                    ConflictBridge.Party party = ConflictKit.bridge().party(agent[0]);
                    check("the band is still sent home: recalling (" + party + ")", party != null && party.recalling());
                    for (ConflictBridge.Stack st : party.stacks()) {
                        double[] t = st.target();
                        double off = flat(t[0], t[1], grove.spot);
                        record(test, "home", "stack " + st.index + " walks home to " + xz(t) + ", " + off + " from the grove's centre, state " + st.state());
                        check("stack " + st.index + " walks home to the grove's centre, not its own front (end " + xz(t) + ", " + off + " from it)", off < 8);
                    }
                    check("no BAND_DEMATERIALIZED while the player sees it", ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, agent[0]).isEmpty());
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    ConflictBridge.Party party = ConflictKit.bridge().party(agent[0]);
                    check("40 ticks on the band is recalling still (" + party + ")", party != null && party.recalling());
                    for (int i = 0; i < 2; i++) {
                        double closer = x0[i] - puppet[i].getX();
                        record(test, "walk " + i, "puppet " + i + " is " + closer + " blocks nearer the grove (x " + x0[i] + " to " + puppet[i].getX() + ")");
                        check("puppet " + i + " walks toward the grove: " + closer + " blocks nearer (x " + x0[i] + " to " + puppet[i].getX() + ")",
                                closer >= 3 && !puppet[i].isRemoved());
                    }
                    check("no BAND_DEMATERIALIZED in view", ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, agent[0]).isEmpty());
                    tick[0] = level.getGameTime();
                    FakePlayers.remove(watcher[0]);
                })
                .thenWaitUntil(waits.logic(() -> check("the band goes once the player does",
                        !ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, agent[0]).isEmpty())))
                .thenExecute(() -> {
                    ScriptedEngine.Received done = ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, agent[0]).get(0);
                    Map<String, Integer> left = ConflictKit.survivors(done);
                    long took = level.getGameTime() - tick[0];
                    record(test, "gone", "BAND_DEMATERIALIZED " + took + " ticks after the player left, reason " + done.aux() + ", survivors " + left);
                    check("the reason is the command's, 1 (" + done.aux() + ")", done.aux() == ConflictRecords.Dematerialized.COMMAND);
                    check("its two warriors are its survivors (" + left + ")", left.equals(Map.of(MowzieKit.WARRIOR, 2)));
                    check("it went within a few ticks of the player's going (" + took + ")", took <= 5);
                    for (int i = 0; i < 2; i++)
                        check("puppet " + i + " is out of the world (" + puppet[i].getRemovalReason() + ")", puppet[i].isRemoved());
                    check("the party is gone", ConflictKit.bridge().party(agent[0]) == null);
                    grove.force();
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenWaitUntil(waits.logic(() -> check("the grove's census 2 (" + MowzieKit.censusCount(grove.faction) + ")",
                        MowzieKit.censusCount(grove.faction) >= 2)))
                .thenExecute(() -> {
                    ScriptedEngine.Received c2 = MowzieKit.census(grove.faction, 2);
                    epoch[1] = MowzieKit.epoch(c2);
                    int bodies = ConflictKit.censusBodies(c2).size();
                    record(test, "back in", "epoch " + epoch[0] + " then " + epoch[1] + ", census 2 lists " + bodies + " bodies");
                    check("the grove came in under a new epoch (" + epoch[0] + " then " + epoch[1] + ")", epoch[1] != epoch[0]);
                    check("census 2 lists no body (" + bodies + ")", bodies == 0);
                    count[1] = MowzieKit.camps().spawned;
                    op[1] = MowzieKit.reconcile(grove.faction, epoch[1], List.of(
                            CampLine.of(0, 1, MowzieKit.WARRIOR, SPAWN, 0), CampLine.of(1, 1, MowzieKit.WARRIOR, SPAWN, 0)));
                })
                .thenWaitUntil(waits.logic(() -> MowzieKit.answered(grove.faction, op[1], 2)))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("the SPAWN lines of the new epoch", ConflictKit.outcomes(grove.faction, op[1]), DONE);
                    check("two bodies spawned (" + (MowzieKit.camps().spawned - count[1]) + ")", MowzieKit.camps().spawned - count[1] == 2);
                    StringBuilder log = new StringBuilder();
                    for (int i = 0; i < 2; i++) {
                        List<Entity> h = MowzieKit.held(grove.faction, i, 1);
                        check("slot " + i + " holds one body tagged (faction, " + i + ", 1) (" + h.size() + ")", h.size() == 1
                                && h.get(0) instanceof EntityUmvuthanaMinion && !RtsimApi.isPuppet(h.get(0)));
                        double d = flat(h.get(0).position(), grove.spot);
                        log.append(String.format("body %d %.1f from the centre; ", i, d));
                        check("body " + i + " stands 2 to 11 blocks from the centre, with no player near (" + d + ")", d >= 1.5 && d <= 12);
                    }
                    record(test, "spawned", log.toString());
                })
                .thenSucceed();
    }

    // ---- 28: a grove that comes in with its march's bodies left behind ---------------------------------

    /**
     * A grove comes in while its march is out, and takes the bodies the party left behind (MOWZIE.md §4.9, §7 test 28's Java half; the
     * engine's half, that the Invasion still reaches its target and that pool plus away stay at the cap, is a protocol test of
     * rtsim-jni and is not written here). Two groves 20 chunks apart, each with three adopted warriors and an adopted Umvuthi
     * (slots 0 to 2 and the chief's, generation 1), and each with its march recorded as the engine would say it (phase OUT, three
     * warriors and the chief). Each grove goes out and its bodies are saved and unloaded with their chunks; both come in again. Grove
     * 0 has a player 30 blocks from its throne when it does, grove 1 has none. Census 2 lists the four bodies of each by slot and
     * generation, and a TAKE line for each comes at its epoch:
     * <ul>
     * <li>grove 1: all four DONE at once, every body out of the world with a record marker where it stood, no held removal;</li>
     * <li>grove 0: the three warriors DONE at once; the Umvuthi's TAKE has no answer while the player is near (one held removal, one
     * held line, the Umvuthi alive and tagged), through 60 ticks, and is DONE once the player is gone, with its marker;</li>
     * <li>both: no CAMP_DEATH, no party, no BAND_DEMATERIALIZED or BAND_CASUALTIES for the agent, and its march record is what it was
     * (the same snapshot, still held, not ended; no stale record and no end counted): the take does not touch the march.</li>
     * </ul>
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "mowzie_march_in", skyAccess = true)
    public static void aGroveThatComesInTakesWhatItsMarchLeft(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 6000);
        String test = "aGroveThatComesInTakesWhatItsMarchLeft";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        String dim = RtsimTests.dimension(helper);
        // Grove 0 has a player near when it comes in, grove 1 none; 320 blocks apart, past Rule U's 160.
        Grove[] g = {new Grove(far, far.k, far.z0), new Grove(far, far.k + 20, far.z0)};
        EntityUmvuthanaMinion[][] w = new EntityUmvuthanaMinion[2][3];
        EntityUmvuthi[] chief = new EntityUmvuthi[2];
        UUID[][] id = new UUID[2][4];
        Vec3[][] stood = new Vec3[2][4];
        Entity[][] back = new Entity[2][4];
        Markers[] markers = new Markers[2];
        long[] agent = new long[2], op = new long[2], was = new long[6];
        String[] march = new String[2];
        ServerPlayer[] player = new ServerPlayer[1];

        MowzieKit.seq(helper)
                .thenExecute(() -> {
                    for (Grove x : g)
                        x.force();
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++) {
                        g[i].lay().declare();
                        markers[i] = new Markers(helper, level, g[i].area());
                        AABB area = g[i].area();
                        ConflictKit.onEnd(helper, () -> MowzieKit.discardRecordMarkers(level, area));
                        w[i][0] = MowzieKit.warrior(level, g[i].at(6, 4));
                        w[i][1] = MowzieKit.warrior(level, g[i].at(-6, 4));
                        w[i][2] = MowzieKit.warrior(level, g[i].at(0, -7));
                        chief[i] = MowzieKit.umvuthi(level, g[i].at(0, 0));
                    }
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < 2; i++)
                        check("the census 1 of grove " + i + " (" + MowzieKit.censusCount(g[i].faction) + ")", MowzieKit.censusCount(g[i].faction) >= 1);
                }))
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++) {
                        List<CampLine> lines = new ArrayList<>();
                        for (int j = 0; j < 3; j++)
                            lines.add(CampLine.of(j, 1, MowzieKit.WARRIOR, ADOPT, 0, w[i][j].getUUID()));
                        lines.add(CampLine.of(CHIEF_SLOT, 1, MowzieKit.UMVUTHI, ADOPT, LINE_CHIEF, chief[i].getUUID()));
                        op[i] = MowzieKit.reconcile(g[i].faction, MowzieKit.epoch(MowzieKit.census(g[i].faction, 1)), lines);
                    }
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < 2; i++)
                        MowzieKit.answered(g[i].faction, op[i], 4);
                }))
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++) {
                        MowzieKit.allOutcomes("ADOPT in grove " + i, ConflictKit.outcomes(g[i].faction, op[i]), DONE);
                        for (int j = 0; j < 3; j++) {
                            id[i][j] = w[i][j].getUUID();
                            stood[i][j] = w[i][j].position();
                        }
                        id[i][3] = chief[i].getUUID();
                        stood[i][3] = chief[i].position();
                        // The engine's march of a party out of this grove: three warriors and the chief, phase OUT.
                        agent[i] = ConflictKit.newAgent();
                        long now = level.getGameTime();
                        ConflictKit.march(new ConflictKit.MarchSpec(agent[i], 1, dim, g[i].faction, ConflictRecords.PURPOSE_INVASION,
                                ConflictRecords.March.OUT, ConflictRecords.March.FLAG_CHIEF, now,
                                new LinkedHashMap<>(Map.of(MowzieKit.WARRIOR, 3)),
                                List.of(wp(g[i].spot.x, g[i].spot.z, now + 100), wp(g[i].spot.x + 400, g[i].spot.z, now + 4000))));
                    }
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < 2; i++)
                        check("the engine's march of grove " + i + " is held", ConflictKit.bridge().march(agent[i]) != null);
                }))
                .thenExecute(() -> {
                    ConflictBridge bridge = ConflictKit.bridge();
                    for (int i = 0; i < 2; i++) {
                        march[i] = snapshot(bridge.march(agent[i]));
                        check("grove " + i + " has no party (" + bridge.party(agent[i]) + ")", bridge.party(agent[i]) == null);
                    }
                    was[3] = bridge.marchStale;
                    was[4] = bridge.marchEnds;
                    was[5] = bridge.marchRecords;
                    record(test, "march", "grove 0: " + march[0] + "; grove 1: " + march[1]);
                    for (Grove x : g)
                        far.releaseRect(x.cx - 2, x.cz - 2, x.cx + 2, x.cz + 2);
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < 2; i++)
                        check("grove " + i + " went out (" + ConflictKit.campOuts(g[i].faction).size() + " CAMP_OUT)", ConflictKit.campOuts(g[i].faction).size() == 1);
                }))
                .thenWaitUntil(waits.chunks(MowzieKit.removed(level, () -> w[0][0], () -> w[0][1], () -> w[0][2], () -> chief[0],
                        () -> w[1][0], () -> w[1][1], () -> w[1][2], () -> chief[1])))
                .thenExecute(() -> {
                    for (Grove x : g)
                        x.force();
                })
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> player[0] = MowzieKit.player(helper, g[0].spot.add(30, 1, 0), "mowzie-march-in-player"))
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = 0; i < 2; i++)
                        check("the census 2 of grove " + i + " (" + MowzieKit.censusCount(g[i].faction) + ")", MowzieKit.censusCount(g[i].faction) >= 2);
                }))
                .thenExecute(() -> {
                    Camps camps = MowzieKit.camps();
                    was[0] = camps.taken;
                    was[1] = camps.heldRemovals;
                    was[2] = camps.heldLines();
                    for (int i = 0; i < 2; i++) {
                        ScriptedEngine.Received c2 = MowzieKit.census(g[i].faction, 2);
                        List<CampLine> lines = new ArrayList<>();
                        for (int j = 0; j < 4; j++) {
                            int slot = j < 3 ? j : CHIEF_SLOT;
                            ConflictKit.CensusBody cb = MowzieKit.bodyOf(c2, id[i][j]);
                            check("census 2 of grove " + i + " lists body " + j + " (" + id[i][j] + ")", cb != null);
                            check("body " + j + " of grove " + i + " is listed in slot " + cb.slot() + ", generation " + cb.generation(),
                                    cb.generation() == 1 && (j == 3 || cb.slot() == j));
                            back[i][j] = level.getEntity(id[i][j]);
                            check("body " + j + " of grove " + i + " is back in the world, tagged (faction, " + slot + ", 1)",
                                    back[i][j] != null && !back[i][j].isRemoved() && MowzieKit.tagged(back[i][j], g[i].faction, slot, 1));
                            lines.add(j < 3 ? CampLine.of(j, 1, MowzieKit.WARRIOR, TAKE, 0)
                                    : CampLine.of(CHIEF_SLOT, 1, MowzieKit.UMVUTHI, TAKE, LINE_CHIEF));
                        }
                        op[i] = MowzieKit.reconcile(g[i].faction, MowzieKit.epoch(c2), lines);
                    }
                })
                .thenWaitUntil(waits.logic(() -> {
                    MowzieKit.answered(g[1].faction, op[1], 4);
                    MowzieKit.answered(g[0].faction, op[0], 3);
                }))
                .thenExecute(() -> {
                    Camps camps = MowzieKit.camps();
                    ConflictBridge bridge = ConflictKit.bridge();
                    MowzieKit.allOutcomes("the TAKE lines of grove 1 (no player near)", ConflictKit.outcomes(g[1].faction, op[1]), DONE);
                    Map<Integer, Integer> o0 = ConflictKit.outcomes(g[0].faction, op[0]);
                    check("grove 0's three warriors are DONE, the Umvuthi's line has no answer (" + o0 + ")", o0.size() == 3 && !o0.containsKey(3));
                    MowzieKit.allOutcomes("the warriors' TAKE lines of grove 0", o0, DONE);
                    for (int j = 0; j < 4; j++)
                        check("body " + j + " of grove 1 is out of the world (" + back[1][j].getRemovalReason() + ")", back[1][j].isRemoved());
                    for (int j = 0; j < 3; j++)
                        check("warrior " + j + " of grove 0 is out of the world (" + back[0][j].getRemovalReason() + ")", back[0][j].isRemoved());
                    check("the Umvuthi of grove 0 is held while the player is near: alive and tagged", back[0][3].isAlive() && !back[0][3].isRemoved()
                            && MowzieKit.tagged(back[0][3], g[0].faction, CHIEF_SLOT, 1));
                    check("seven taken so far (" + (camps.taken - was[0]) + ")", camps.taken - was[0] == 7);
                    check("one held removal (" + (camps.heldRemovals - was[1]) + ")", camps.heldRemovals - was[1] == 1);
                    check("one held line (" + (camps.heldLines() - was[2]) + ")", camps.heldLines() - was[2] == 1);
                    StringBuilder log = new StringBuilder();
                    for (int i = 0; i < 2; i++)
                        for (int j = 0; j < (i == 0 ? 3 : 4); j++) {
                            List<Markers.Seen> m = markers[i].near(stood[i][j], 4);
                            log.append(String.format("grove %d body %d: %d markers; ", i, j, m.size()));
                            check("a record marker where body " + j + " of grove " + i + " stood, at " + at(stood[i][j]) + " (" + m.size() + ")",
                                    !m.isEmpty() && m.get(0).tagged() && m.get(0).carriesRecords());
                        }
                    record(test, "taken", log + "taken " + (camps.taken - was[0]) + ", held removals " + (camps.heldRemovals - was[1]));
                    for (int i = 0; i < 2; i++) {
                        check("grove " + i + " has no party (" + bridge.party(agent[i]) + ")", bridge.party(agent[i]) == null);
                        check("no BAND_DEMATERIALIZED for grove " + i + "'s agent", ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, agent[i]).isEmpty());
                        check("no BAND_CASUALTIES for grove " + i + "'s agent", ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent[i]).isEmpty());
                        check("no CAMP_DEATH in grove " + i + " (" + ConflictKit.campDeaths(g[i].faction).size() + ")", ConflictKit.campDeaths(g[i].faction).isEmpty());
                        String now = snapshot(bridge.march(agent[i]));
                        check("grove " + i + "'s march is what it was: was " + march[i] + ", now " + now, now.equals(march[i]));
                        check("and not ended", !bridge.marchEnded(agent[i]));
                    }
                    check("no march record was dropped as stale (" + (bridge.marchStale - was[3]) + ") or ended (" + (bridge.marchEnds - was[4]) + ")",
                            bridge.marchStale == was[3] && bridge.marchEnds == was[4]);
                    check("and none was said again (" + (bridge.marchRecords - was[5]) + ")", bridge.marchRecords == was[5]);
                })
                .thenIdle(60)
                .thenExecute(() -> {
                    check("60 ticks on the Umvuthi's TAKE has no answer (" + ConflictKit.outcomes(g[0].faction, op[0]) + ")",
                            ConflictKit.outcomes(g[0].faction, op[0]).size() == 3);
                    check("the Umvuthi stands, tagged", back[0][3].isAlive() && !back[0][3].isRemoved());
                    check("and the line is still held (" + (MowzieKit.camps().heldLines() - was[2]) + ")", MowzieKit.camps().heldLines() - was[2] == 1);
                    FakePlayers.remove(player[0]);
                })
                .thenWaitUntil(waits.logic(() -> MowzieKit.answered(g[0].faction, op[0], 4)))
                .thenExecute(() -> {
                    Camps camps = MowzieKit.camps();
                    ConflictBridge bridge = ConflictKit.bridge();
                    MowzieKit.allOutcomes("the TAKE lines of grove 0 once the player is gone", ConflictKit.outcomes(g[0].faction, op[0]), DONE);
                    check("the Umvuthi is out of the world (" + back[0][3].getRemovalReason() + ")", back[0][3].isRemoved());
                    List<Markers.Seen> m = markers[0].near(stood[0][3], 4);
                    check("a record marker where it stood (" + m.size() + ")", !m.isEmpty() && m.get(0).tagged() && m.get(0).carriesRecords());
                    check("eight taken in all (" + (camps.taken - was[0]) + ")", camps.taken - was[0] == 8);
                    check("the held lines are back to the start (" + (camps.heldLines() - was[2]) + ")", camps.heldLines() == was[2]);
                    check("no CAMP_DEATH in grove 0", ConflictKit.campDeaths(g[0].faction).isEmpty());
                    String now = snapshot(bridge.march(agent[0]));
                    check("grove 0's march is what it was: was " + march[0] + ", now " + now, now.equals(march[0]));
                    check("no party and no band gone for grove 0's agent", bridge.party(agent[0]) == null
                            && ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, agent[0]).isEmpty());
                    record(test, "held take", "the Umvuthi's TAKE was held 60+ ticks with a player 30 blocks off, then DONE; eight bodies taken, no death, the march unchanged");
                })
                .thenSucceed();
    }

    // ---- 22: a band sets out and comes home in plain view --------------------------------------------

    /**
     * A band that sets out and comes home in a player's plain view (MOWZIE.md §7 test 22, the "in plain view" bullet). A player
     * stands 150 blocks east of a grove that is in, with four adopted warriors (slots 0 to 3), three given health 9, 11 and 13 and the
     * masks fury, fear and rage; the fourth stands farthest from the player. A per-tick sampler watches every warrior of the
     * grove for what appears and what vanishes. The band sets out as the engine's hand-over does: four SET_OUT lines and a materialize
     * of three warriors and a raptor pack together (the pack stands for the fourth warrior, which no puppet claims):
     * <ul>
     * <li>four DONE; three handed off and one taken; three swaps, each a body that vanishes and a puppet that appears within six
     * blocks and two ticks, with the body's health, mask and weapon; one vanishing is left over, the fourth warrior, 128 blocks or
     * more from the player and the farthest from it, with a record marker where it stood;</li>
     * <li>the puppets walk (60 ticks: at least one is two blocks on);</li>
     * <li>four HAND_BACK lines of the warrior body: four DONE; three swaps, each a puppet that vanishes and a body that appears within
     * four blocks, with its health, mask and weapon; one appearance left over, the spawned fourth body, 128 blocks or more from the
     * player; no event within 128 blocks of the player at all;</li>
     * <li>the raptor pack is out of sight (the player is beyond the 64 blocks of the recall's view), so it goes at once: BAND_DEMATERIALIZED,
     * survivors one raptor, its pack removed.</li>
     * </ul>
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "mowzie_march_view", skyAccess = true)
    public static void aBandSetsOutAndComesHomeInPlainView(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 6000);
        String test = "aBandSetsOutAndComesHomeInPlainView";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove grove = new Grove(far, far.k, far.z0);
        EntityUmvuthanaMinion[] w = new EntityUmvuthanaMinion[4];
        Vec3[] stood = new Vec3[4];
        float[] health = {9f, 11f, 13f};
        MaskType[] mask = {MaskType.FURY, MaskType.FEAR, MaskType.RAGE};
        long[] agent = new long[1], op = new long[2], count = new long[4], tick = new long[2];
        Vec3[] viewer = new Vec3[1];
        Vec3[] walked = new Vec3[3];
        Swaps[] swaps = new Swaps[1];
        Markers[] markers = new Markers[1];
        Mob[] raptor = new Mob[1];
        List<Mob> pack = new ArrayList<>();
        UUID[] spawned = new UUID[1];

        MowzieKit.seq(helper)
                .thenExecute(grove::force)
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    grove.lay().declare();
                    viewer[0] = grove.spot.add(150, 1, 0);
                    MowzieKit.player(helper, viewer[0], "mowzie-view-player");
                    AABB area = new AABB(grove.spot.x - 48, level.getMinBuildHeight(), grove.spot.z - 48, grove.spot.x + 48, level.getMaxBuildHeight(),
                            grove.spot.z + 48);
                    swaps[0] = new Swaps(helper, level, area, viewer[0]);
                    markers[0] = new Markers(helper, level, grove.area());
                    ConflictKit.onEnd(helper, () -> MowzieKit.discardRecordMarkers(level, grove.area()));
                    double[][] at = {{15, 0}, {5, 10}, {-8, -6}, {-25, 0}};
                    for (int i = 0; i < 4; i++) {
                        w[i] = MowzieKit.warrior(level, grove.at(at[i][0], at[i][1]));
                        if (i < 3) {
                            w[i].setHealth(health[i]);
                            w[i].setMask(mask[i]);
                        }
                    }
                })
                .thenWaitUntil(waits.logic(() -> check("the grove's census 1 (" + MowzieKit.censusCount(grove.faction) + ")",
                        MowzieKit.censusCount(grove.faction) >= 1)))
                .thenExecute(() -> {
                    List<CampLine> lines = new ArrayList<>();
                    for (int i = 0; i < 4; i++)
                        lines.add(CampLine.of(i, 1, MowzieKit.WARRIOR, ADOPT, 0, w[i].getUUID()));
                    op[0] = MowzieKit.reconcile(grove.faction, MowzieKit.epoch(MowzieKit.census(grove.faction, 1)), lines);
                })
                .thenWaitUntil(waits.logic(() -> MowzieKit.answered(grove.faction, op[0], 4)))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("ADOPT of the four warriors", ConflictKit.outcomes(grove.faction, op[0]), DONE);
                    double[] away = new double[4];
                    for (int i = 0; i < 4; i++) {
                        stood[i] = w[i].position();
                        away[i] = flat(stood[i], viewer[0]);
                    }
                    record(test, "bodies", String.format("the four bodies stand %.0f, %.0f, %.0f and %.0f blocks from the player", away[0], away[1], away[2], away[3]));
                    check("the fourth warrior is the farthest from the player (" + away[3] + " against " + away[0] + ", " + away[1] + ", " + away[2] + ")",
                            away[3] > away[0] && away[3] > away[1] && away[3] > away[2]);
                    check("and all four are 128 blocks or more from it (" + away[0] + " nearest)", away[0] >= 128);
                    count[0] = MowzieKit.camps().handedOff;
                    count[1] = MowzieKit.camps().taken;
                    tick[0] = level.getGameTime();
                    agent[0] = ConflictKit.newAgent();
                    ConflictKit.Band b = ConflictKit.band(agent[0], grove.faction, RtsimTests.dimension(helper));
                    b.arcX = grove.spot.x;
                    b.arcZ = grove.spot.z;
                    b.bearing = 180f;
                    b.targetX = grove.spot.x - 20;
                    b.targetY = grove.spot.y;
                    b.targetZ = grove.spot.z;
                    b.speed = WALK;
                    int epoch = b.epoch;
                    // The hand-over of a set-out: the four lines and the materialize in one step, as the engine sends them.
                    ConflictKit.together(() -> {
                        List<CampLine> lines = new ArrayList<>();
                        for (int i = 0; i < 4; i++)
                            lines.add(CampLine.party(i, 1, MowzieKit.WARRIOR, SET_OUT, 0, agent[0], epoch));
                        op[1] = MowzieKit.reconcile(grove.faction, grove.site().epoch(), lines);
                        ConflictKit.materialize(b, List.of(new ConflictKit.StackSpec(MowzieKit.WARRIOR, 3, false),
                                new ConflictKit.StackSpec(MowzieKit.RAPTOR, 1, false)));
                    });
                })
                .thenWaitUntil(waits.logic(() -> {
                    MowzieKit.answered(grove.faction, op[1], 4);
                    embodied(agent[0], 4);
                }))
                .thenExecute(() -> {
                    Camps camps = MowzieKit.camps();
                    MowzieKit.allOutcomes("the four SET_OUT lines", ConflictKit.outcomes(grove.faction, op[1]), DONE);
                    check("three handed off (" + (camps.handedOff - count[0]) + ")", camps.handedOff - count[0] == 3);
                    check("one taken (" + (camps.taken - count[1]) + ")", camps.taken - count[1] == 1);
                    Swaps.Pairs p = swaps[0].pair(tick[0]);
                    record(test, "set out", Swaps.describe(p));
                    check("three swaps (" + p.swaps().size() + "): " + Swaps.describe(p), p.swaps().size() == 3);
                    check("and one body left over (" + p.loose().size() + "): " + Swaps.describe(p), p.loose().size() == 1);
                    List<UUID> handed = new ArrayList<>();
                    for (Swaps.Swap s : p.swaps()) {
                        Swaps.Snap body = s.gone().s(), puppet = s.came().s();
                        handed.add(body.id());
                        check("a swap is a body that goes and a puppet that comes (" + body.puppet() + ", " + puppet.puppet() + ")",
                                !body.puppet() && puppet.puppet());
                        check("the puppet's health is the body's (" + puppet.health() + ", the body's " + body.health() + ")",
                                Math.abs(puppet.health() - body.health()) < 1.5f);
                        check("and its mask and weapon (" + puppet.mask() + " " + puppet.weapon() + ", the body's " + body.mask() + " " + body.weapon() + ")",
                                puppet.mask() == body.mask() && puppet.weapon() == body.weapon());
                        check("the swap is 128 blocks or more from the player (" + body.away() + ")", body.away() >= 128);
                    }
                    for (int i = 0; i < 3; i++)
                        check("body " + i + " was one of the three handed off (" + w[i].getUUID() + " among " + handed + ")", handed.contains(w[i].getUUID()));
                    Swaps.Event left = p.loose().get(0);
                    check("the body left over is the fourth warrior, taken (" + left.s().id() + ", expected " + w[3].getUUID() + ")",
                            !left.appeared() && left.s().id().equals(w[3].getUUID()));
                    check("it vanished 128 blocks or more from the player (" + left.s().away() + ")", left.s().away() >= 128);
                    for (int i = 0; i < 4; i++)
                        check("body " + i + " is out of the world (" + w[i].getRemovalReason() + ")", w[i].isRemoved());
                    for (int i = 0; i < 4; i++) {
                        List<Markers.Seen> m = markers[0].near(stood[i], 4);
                        check("a record marker where body " + i + " stood, at " + at(stood[i]) + " (" + m.size() + ")", !m.isEmpty());
                    }
                    List<ConflictBridge.Slot> all = embodied(agent[0], 4);
                    List<ConflictBridge.Slot> ws = of(all, MowzieKit.WARRIOR), rs = of(all, MowzieKit.RAPTOR);
                    check("three warrior puppets and a raptor (" + ws.size() + ", " + rs.size() + ")", ws.size() == 3 && rs.size() == 1);
                    for (int i = 0; i < 3; i++)
                        walked[i] = ws.get(i).entity().position();
                    raptor[0] = rs.get(0).entity();
                    pack.addAll(rs.get(0).extras());
                    check("the raptor has its pack of 4 (" + pack.size() + ")", raptor[0] instanceof EntityUmvuthanaRaptor && pack.size() == 4);
                })
                .thenIdle(60)
                .thenExecute(() -> {
                    List<ConflictBridge.Slot> ws = of(embodied(agent[0], 4), MowzieKit.WARRIOR);
                    double most = 0;
                    StringBuilder log = new StringBuilder();
                    for (int i = 0; i < 3; i++) {
                        double d = flat(ws.get(i).entity().position(), walked[i]);
                        log.append(String.format("puppet %d walked %.1f blocks; ", i, d));
                        most = Math.max(most, d);
                    }
                    record(test, "walk", log.toString());
                    check("the puppets walk (the most " + most + " blocks in 60 ticks): " + log, most >= 2);
                    count[2] = MowzieKit.camps().handedBack;
                    count[3] = MowzieKit.camps().spawned;
                    tick[1] = level.getGameTime();
                    ConflictBridge.Party party = ConflictKit.party(agent[0]);
                    List<CampLine> lines = new ArrayList<>();
                    for (int i = 0; i < 4; i++)
                        lines.add(CampLine.party(i, 1, MowzieKit.WARRIOR, HAND_BACK, 0, agent[0], party.epoch));
                    op[0] = MowzieKit.reconcile(grove.faction, grove.site().epoch(), lines);
                })
                .thenWaitUntil(waits.logic(() -> MowzieKit.answered(grove.faction, op[0], 4)))
                .thenWaitUntil(waits.logic(() -> check("the band goes at once: its raptor pack is out of sight",
                        !ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, agent[0]).isEmpty())))
                .thenExecute(() -> {
                    Camps camps = MowzieKit.camps();
                    MowzieKit.allOutcomes("the four HAND_BACK lines", ConflictKit.outcomes(grove.faction, op[0]), DONE);
                    check("three handed back (" + (camps.handedBack - count[2]) + ")", camps.handedBack - count[2] == 3);
                    check("one spawned (" + (camps.spawned - count[3]) + ")", camps.spawned - count[3] == 1);
                    Swaps.Pairs p = swaps[0].pair(tick[1]);
                    record(test, "home", Swaps.describe(p));
                    check("three swaps (" + p.swaps().size() + "): " + Swaps.describe(p), p.swaps().size() == 3);
                    check("and one body appeared (" + p.loose().size() + "): " + Swaps.describe(p), p.loose().size() == 1);
                    for (Swaps.Swap s : p.swaps()) {
                        Swaps.Snap puppet = s.gone().s(), body = s.came().s();
                        check("a swap is a puppet that goes and a body that comes (" + puppet.puppet() + ", " + body.puppet() + ")",
                                puppet.puppet() && !body.puppet());
                        check("the body stands where the puppet did (" + s.apart() + " blocks apart)", s.apart() < 4);
                        check("with its health (" + body.health() + ", the puppet's " + puppet.health() + ")", Math.abs(body.health() - puppet.health()) < 1.5f);
                        check("mask and weapon (" + body.mask() + " " + body.weapon() + ", the puppet's " + puppet.mask() + " " + puppet.weapon() + ")",
                                body.mask() == puppet.mask() && body.weapon() == puppet.weapon());
                    }
                    Swaps.Event came = p.loose().get(0);
                    List<Entity> h3 = MowzieKit.held(grove.faction, 3, 1);
                    check("slot 3 holds the spawned body (" + h3.size() + ")", h3.size() == 1);
                    spawned[0] = h3.get(0).getUUID();
                    check("the appearance left over is that body (" + came.s().id() + ", expected " + spawned[0] + ")", came.appeared() && came.s().id().equals(spawned[0]));
                    for (Swaps.Event e : swaps[0].events)
                        if (e.tick() >= tick[0])
                            check("no event within 128 blocks of the player: " + (e.appeared() ? "an appearance" : "a vanishing") + " " + e.s().away()
                                    + " blocks from it at tick " + e.tick(), e.s().away() >= 128);
                    ScriptedEngine.Received done = ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, agent[0]).get(0);
                    Map<String, Integer> left = ConflictKit.survivors(done);
                    check("the band went with its raptor as the one survivor (reason " + done.aux() + ", " + left + ")",
                            done.aux() == ConflictRecords.Dematerialized.COMMAND && left.equals(Map.of(MowzieKit.RAPTOR, 1)));
                    check("the raptor is out of the world (" + raptor[0].getRemovalReason() + ")", raptor[0].isRemoved());
                    for (Mob f : pack)
                        check("and each of its pack (" + f.getRemovalReason() + ")", f.isRemoved());
                    check("the party is gone", ConflictKit.bridge().party(agent[0]) == null);
                })
                .thenSucceed();
    }

    // ---- the trim and the recall see as far as the server sends (MZB-745) ----------------------------------

    /** Sets the server's view distance for the test and puts it back when the test ends (a GameTest server's is 0: no bound). */
    private static void viewDistance(GameTestHelper helper, ServerLevel level, int chunks) {
        var players = level.getServer().getPlayerList();
        int saved = players.getViewDistance();
        players.setViewDistance(chunks);
        ConflictKit.onEnd(helper, () -> players.setViewDistance(saved));
    }

    /** The distance between a player and an entity, as the sight rule measures it. */
    private static double reach(ServerPlayer p, Entity e) {
        return Math.sqrt(p.distanceToSqr(e));
    }

    /**
     * The trim and the recall see as far as the server sends an entity (MZB-745, P2 of the M10b review: both used 64 blocks, so a
     * band was cut, or sent home and gone, in plain view of a player 100 blocks off). The view distance is 10 for the test. A band
     * of three warriors stands placed and held 25 blocks west of a grove's centre and the march says two:
     * <ul>
     * <li>a warrior puppet is seen from 128 blocks ({@link ConflictBridge#sightRange});</li>
     * <li>a plain player 100 blocks from the head puppet: for 60 ticks nothing is cut and the party stays due;</li>
     * <li>that player gone and a plain player 170 blocks off in its place, with a spectator 10 blocks from the head (a spectator
     * never sees, as vanilla's despawn ignores one): the rearmost member is cut on the next beat, the party is due no more;</li>
     * <li>the player at 100 blocks again and a BAND_RECALL: for 60 ticks the band does not go (the recall's rule was 64), and the
     * beat after the player goes it does, with its two warriors as survivors.</li>
     * </ul>
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "mowzie_march_sight", skyAccess = true)
    public static void theTrimAndTheRecallSeeAsFarAsTheServerSends(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 6000);
        String test = "theTrimAndTheRecallSeeAsFarAsTheServerSends";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        String dim = RtsimTests.dimension(helper);
        Grove grove = new Grove(far, far.k, far.z0);
        long[] agent = new long[1], n = new long[4]; // trimmed before the record, marchRecords before, the tick the watch began, the tick of the last change
        ServerPlayer[] player = new ServerPlayer[3];
        ConflictBridge.Slot[] member = new ConflictBridge.Slot[3];
        boolean[] watching = {false};
        int[] cut = {0}, notDue = {0};
        viewDistance(helper, level, 10);
        MowzieKit.everyTick(helper, () -> {
            if (!watching[0])
                return;
            if (ConflictKit.trimmed() != n[0])
                cut[0]++;
            if (level.getGameTime() > n[2] && !ConflictKit.party(agent[0]).trimDue())
                notDue[0]++;
        });

        MowzieKit.seq(helper)
                .thenExecute(grove::force)
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    grove.lay().declare();
                    agent[0] = ConflictKit.newAgent();
                    ConflictKit.Band b = ConflictKit.band(agent[0], grove.faction, dim);
                    b.arcX = grove.spot.x - 25;
                    b.arcZ = grove.spot.z;
                    b.bearing = 0f;
                    b.targetX = b.arcX;
                    b.targetY = grove.spot.y;
                    b.targetZ = b.arcZ;
                    b.speed = 0f;
                    ConflictKit.materialize(b, List.of(new ConflictKit.StackSpec(MowzieKit.WARRIOR, 3, false)));
                })
                .thenWaitUntil(waits.logic(() -> {
                    check("the grove's census 1 (" + MowzieKit.censusCount(grove.faction) + ")", MowzieKit.censusCount(grove.faction) >= 1);
                    embodied(agent[0], 3);
                }))
                .thenExecute(() -> {
                    List<ConflictBridge.Slot> all = embodied(agent[0], 3);
                    for (int i = 0; i < 3; i++)
                        member[i] = all.get(i);
                    Vec3 head = member[0].entity().position();
                    double r = ConflictBridge.sightRange(member[0].entity(), level);
                    record(test, "range", "a warrior puppet is seen from " + r + " blocks at view distance " + level.getServer().getPlayerList().getViewDistance());
                    check("a warrior puppet is seen from 128 blocks at view distance 10 (" + r + ")", r == 128.0);
                    player[0] = MowzieKit.player(helper, head.add(0, 1, 100), "mowzie-sight-100");
                    StringBuilder log = new StringBuilder();
                    for (int i = 0; i < 3; i++) {
                        double d = reach(player[0], member[i].entity());
                        log.append(String.format("member %d %.1f blocks from the player; ", i, d));
                        check("member " + i + " stands 90 to 110 blocks from the player (" + d + ")", d >= 90 && d <= 110);
                    }
                    record(test, "player at 100", log.toString());
                    n[0] = ConflictKit.trimmed();
                    n[1] = ConflictKit.bridge().marchRecords;
                    ConflictKit.march(new ConflictKit.MarchSpec(agent[0], 1, dim, grove.faction, ConflictRecords.PURPOSE_RAID, ConflictRecords.March.OUT, 0, 0L,
                            new LinkedHashMap<>(Map.of(MowzieKit.WARRIOR, 2)),
                            List.of(wp(grove.spot.x, grove.spot.z, level.getGameTime()), wp(grove.spot.x - 200, grove.spot.z, level.getGameTime() + 10_000))));
                })
                .thenWaitUntil(waits.logic(() -> check("the march was kept (" + (ConflictKit.bridge().marchRecords - n[1]) + " new)",
                        ConflictKit.bridge().marchRecords == n[1] + 1)))
                .thenExecute(() -> {
                    n[2] = level.getGameTime();
                    watching[0] = true;
                })
                .thenIdle(60)
                .thenExecute(() -> {
                    record(test, "in sight at 100", (ConflictKit.trimmed() - n[0]) + " cut, " + cut[0] + " ticks with a cut, " + notDue[0] + " ticks not due");
                    check("nothing was cut while a player stood 100 blocks off (" + cut[0] + " ticks saw a cut, trimmed " + (ConflictKit.trimmed() - n[0]) + ")",
                            cut[0] == 0 && ConflictKit.trimmed() == n[0]);
                    check("the band stayed due the whole time (" + notDue[0] + " ticks not due)", notDue[0] == 0 && ConflictKit.party(agent[0]).trimDue());
                    for (int i = 0; i < 3; i++)
                        check("member " + i + " stands (" + member[i].alive() + ")", member[i].alive() && !member[i].entity().isRemoved());
                    watching[0] = false;
                })
                // The player at 100 blocks goes; one at 170 and a spectator at 10 blocks come.
                .thenWaitUntil(waits.logic(() -> check("the middle of a beat, tick " + level.getGameTime(), level.getGameTime() % 20 == 10)))
                .thenExecute(() -> {
                    Vec3 head = member[0].entity().position();
                    FakePlayers.remove(player[0]);
                    player[1] = MowzieKit.player(helper, head.add(0, 1, 170), "mowzie-sight-170");
                    player[2] = MowzieKit.player(helper, head.add(0, 1, 10), "mowzie-sight-spectator");
                    player[2].setGameMode(GameType.SPECTATOR);
                    check("the near player is a spectator", player[2].isSpectator() && !player[1].isSpectator());
                    for (int i = 0; i < 3; i++) {
                        double d = reach(player[1], member[i].entity());
                        check("member " + i + " stands 160 blocks or more from the far player (" + d + ")", d >= 160);
                    }
                    n[3] = level.getGameTime();
                })
                .thenWaitUntil(waits.logic(() -> check("the member was cut (trimmed " + (ConflictKit.trimmed() - n[0]) + ")", ConflictKit.trimmed() == n[0] + 1)))
                .thenExecute(() -> {
                    long took = level.getGameTime() - n[3];
                    ConflictBridge.Party p = ConflictKit.party(agent[0]);
                    record(test, "out of sight", "the cut came " + took + " ticks after the near player went and a spectator stood 10 blocks off");
                    check("the cut waited for the beat and came on it: " + took + " ticks", took >= 5 && took <= 25);
                    check("the party is due no more", !p.trimDue());
                    double rear = 0;
                    for (ConflictBridge.Stack st : p.stacks())
                        rear = Math.max(rear, st.back());
                    int alive = 0;
                    for (int i = 0; i < 3; i++) {
                        if (member[i].alive())
                            alive++;
                        else
                            check("the member cut is in the rearmost row (its row is " + member[i].stack.back() + " blocks back, the rearmost " + rear + ")",
                                    member[i].stack.back() == rear && (member[i].entity() == null || member[i].entity().isRemoved()));
                    }
                    check("two warriors are left (" + alive + ")", alive == 2);
                    check("no casualty was booked for a cut member (" + ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent[0]).size() + ")",
                            ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent[0]).isEmpty());
                })
                // The recall: a player 100 blocks from the nearest member sees the band, and it stays.
                .thenExecute(() -> {
                    FakePlayers.remove(player[1]);
                    FakePlayers.remove(player[2]);
                    Vec3 near = null;
                    for (int i = 0; i < 3; i++)
                        if (member[i].alive())
                            near = member[i].entity().position();
                    player[0] = MowzieKit.player(helper, near.add(0, 1, 100), "mowzie-sight-recall");
                    n[3] = level.getGameTime();
                    ConflictKit.recall(agent[0], ConflictKit.party(agent[0]).epoch, ConflictRecords.RECALL_DONE, 0, near.x, near.z + 60);
                })
                .thenWaitUntil(waits.logic(() -> check("the band is recalling", ConflictKit.party(agent[0]).recalling())))
                .thenIdle(60)
                .thenExecute(() -> {
                    ConflictBridge.Party p = ConflictKit.bridge().party(agent[0]);
                    check("60 ticks on, with a player 100 blocks off, the band is still there and recalling (" + p + ")", p != null && p.recalling());
                    check("and has not gone", ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, agent[0]).isEmpty());
                    for (int i = 0; i < 3; i++)
                        if (member[i].alive())
                            check("member " + i + " stands (" + member[i].entity().getRemovalReason() + ")", !member[i].entity().isRemoved());
                    n[3] = level.getGameTime();
                    FakePlayers.remove(player[0]);
                })
                .thenWaitUntil(waits.logic(() -> check("the band goes once the player does",
                        !ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, agent[0]).isEmpty())))
                .thenExecute(() -> {
                    ScriptedEngine.Received done = ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, agent[0]).get(0);
                    Map<String, Integer> left = ConflictKit.survivors(done);
                    long took = level.getGameTime() - n[3];
                    record(test, "recall", "BAND_DEMATERIALIZED " + took + " ticks after the player left, survivors " + left);
                    check("its two warriors are its survivors (" + left + ")", left.equals(Map.of(MowzieKit.WARRIOR, 2)));
                    check("it went within a few ticks of the player's going (" + took + ")", took <= 5);
                })
                .thenSucceed();
    }

    /**
     * A member is seen when any of its extras is (MZB-745): a raptor's pack. A band of a warrior and a raptor with its four followers
     * stands placed and held, the march says the warrior only (the raptor is one too many), and a plain player stands 140 blocks
     * from the raptor, beyond its 128:
     * <ul>
     * <li>one follower, held where it is put (no AI), is 120 blocks from the player and the other three, as the raptor, are beyond 128:
     * for 60 ticks nothing is cut and the party stays due (the leader is out of sight, a follower is not);</li>
     * <li>that follower moved to 148 blocks: on the next beat the raptor is cut, and with it the raptor's body and all four
     * followers are out of the world; the warrior stands.</li>
     * </ul>
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "mowzie_march_sight_pack", skyAccess = true)
    public static void aRaptorWhoseFollowerAPlayerSeesIsNotCutAndIsOnceNoneIs(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 6000);
        String test = "aRaptorWhoseFollowerAPlayerSeesIsNotCutAndIsOnceNoneIs";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        String dim = RtsimTests.dimension(helper);
        Grove grove = new Grove(far, far.k, far.z0);
        long[] agent = new long[1], n = new long[4];
        ServerPlayer[] player = new ServerPlayer[1];
        ConflictBridge.Slot[] member = new ConflictBridge.Slot[2];
        List<Mob> pack = new ArrayList<>();
        Vec3[] lead = new Vec3[1];
        boolean[] watching = {false};
        int[] cut = {0}, notDue = {0};
        viewDistance(helper, level, 10);
        MowzieKit.everyTick(helper, () -> {
            if (!watching[0])
                return;
            if (ConflictKit.trimmed() != n[0])
                cut[0]++;
            if (level.getGameTime() > n[2] && !ConflictKit.party(agent[0]).trimDue())
                notDue[0]++;
        });

        MowzieKit.seq(helper)
                .thenExecute(grove::force)
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    grove.lay().declare();
                    agent[0] = ConflictKit.newAgent();
                    ConflictKit.Band b = ConflictKit.band(agent[0], grove.faction, dim);
                    b.arcX = grove.spot.x - 25;
                    b.arcZ = grove.spot.z;
                    b.bearing = 0f;
                    b.targetX = b.arcX;
                    b.targetY = grove.spot.y;
                    b.targetZ = b.arcZ;
                    b.speed = 0f;
                    ConflictKit.materialize(b, List.of(new ConflictKit.StackSpec(MowzieKit.WARRIOR, 1, false),
                            new ConflictKit.StackSpec(MowzieKit.RAPTOR, 1, false)));
                })
                .thenWaitUntil(waits.logic(() -> {
                    check("the grove's census 1 (" + MowzieKit.censusCount(grove.faction) + ")", MowzieKit.censusCount(grove.faction) >= 1);
                    embodied(agent[0], 2);
                }))
                .thenExecute(() -> {
                    List<ConflictBridge.Slot> all = embodied(agent[0], 2);
                    member[0] = of(all, MowzieKit.WARRIOR).get(0);
                    member[1] = of(all, MowzieKit.RAPTOR).get(0);
                    pack.addAll(member[1].extras());
                    check("the raptor has its pack of 4 (" + pack.size() + ")", member[1].entity() instanceof EntityUmvuthanaRaptor && pack.size() == 4);
                    Mob leader = member[1].entity();
                    lead[0] = leader.position();
                    // Three followers stand behind the leader, one stands 20 blocks toward the player; all held where they are put.
                    for (int i = 0; i < 4; i++) {
                        Mob f = pack.get(i);
                        f.setNoAi(true);
                        f.moveTo(lead[0].x, lead[0].y, lead[0].z + (i == 0 ? 20 : -6), f.getYRot(), f.getXRot());
                    }
                    player[0] = MowzieKit.player(helper, lead[0].add(0, 1, 140), "mowzie-sight-pack");
                    StringBuilder log = new StringBuilder(String.format("the raptor %.1f blocks from the player; ", reach(player[0], leader)));
                    check("the raptor is out of its sight (" + reach(player[0], leader) + " against " + ConflictBridge.sightRange(leader, level) + ")",
                            reach(player[0], leader) > ConflictBridge.sightRange(leader, level));
                    for (int i = 0; i < 4; i++) {
                        double d = reach(player[0], pack.get(i));
                        double r = ConflictBridge.sightRange(pack.get(i), level);
                        log.append(String.format("follower %d %.1f of %.0f; ", i, d, r));
                        check("follower " + i + " is " + (i == 0 ? "in" : "out of") + " its sight (" + d + " against " + r + ")", (d <= r) == (i == 0));
                    }
                    record(test, "pack", log.toString());
                    n[0] = ConflictKit.trimmed();
                    n[1] = ConflictKit.bridge().marchRecords;
                    ConflictKit.march(new ConflictKit.MarchSpec(agent[0], 1, dim, grove.faction, ConflictRecords.PURPOSE_RAID, ConflictRecords.March.OUT, 0, 0L,
                            new LinkedHashMap<>(Map.of(MowzieKit.WARRIOR, 1)),
                            List.of(wp(grove.spot.x, grove.spot.z, level.getGameTime()), wp(grove.spot.x - 200, grove.spot.z, level.getGameTime() + 10_000))));
                })
                .thenWaitUntil(waits.logic(() -> check("the march was kept (" + (ConflictKit.bridge().marchRecords - n[1]) + " new)",
                        ConflictKit.bridge().marchRecords == n[1] + 1)))
                .thenExecute(() -> {
                    n[2] = level.getGameTime();
                    watching[0] = true;
                })
                .thenIdle(60)
                .thenExecute(() -> {
                    record(test, "follower in sight", (ConflictKit.trimmed() - n[0]) + " cut, " + cut[0] + " ticks with a cut, " + notDue[0] + " ticks not due");
                    check("the raptor was not cut while a follower was in sight (" + cut[0] + " ticks saw a cut, trimmed " + (ConflictKit.trimmed() - n[0]) + ")",
                            cut[0] == 0 && ConflictKit.trimmed() == n[0]);
                    check("the band stayed due the whole time (" + notDue[0] + " ticks not due)", notDue[0] == 0 && ConflictKit.party(agent[0]).trimDue());
                    double d = reach(player[0], pack.get(0));
                    check("the follower stayed where it was put: " + d + " blocks from the player", !pack.get(0).isRemoved() && Math.abs(d - 120) < 6);
                    check("the raptor stands (" + member[1].alive() + ")", member[1].alive() && !member[1].entity().isRemoved());
                    watching[0] = false;
                })
                .thenWaitUntil(waits.logic(() -> check("the middle of a beat, tick " + level.getGameTime(), level.getGameTime() % 20 == 10)))
                .thenExecute(() -> {
                    pack.get(0).moveTo(lead[0].x, lead[0].y, lead[0].z - 8, pack.get(0).getYRot(), pack.get(0).getXRot());
                    double d = reach(player[0], pack.get(0));
                    check("the follower is out of sight now (" + d + ")", d > ConflictBridge.sightRange(pack.get(0), level));
                    n[3] = level.getGameTime();
                })
                .thenWaitUntil(waits.logic(() -> check("the raptor was cut (trimmed " + (ConflictKit.trimmed() - n[0]) + ")", ConflictKit.trimmed() == n[0] + 1)))
                .thenExecute(() -> {
                    long took = level.getGameTime() - n[3];
                    record(test, "out of sight", "the raptor was cut " + took + " ticks after its last follower went out of sight");
                    check("the cut came on the beat (" + took + " ticks)", took >= 5 && took <= 25);
                    check("the raptor is no longer the band's (" + member[1].alive() + ")", !member[1].alive());
                    check("its body is out of the world (" + member[1].entity() + ")", member[1].entity() == null || member[1].entity().isRemoved());
                    for (int i = 0; i < 4; i++)
                        check("follower " + i + " is out of the world (" + pack.get(i).getRemovalReason() + ")", pack.get(i).isRemoved());
                    check("the warrior stands (" + member[0].alive() + ")", member[0].alive() && !member[0].entity().isRemoved());
                    check("the party is due no more", !ConflictKit.party(agent[0]).trimDue());
                })
                .thenSucceed();
    }
}
