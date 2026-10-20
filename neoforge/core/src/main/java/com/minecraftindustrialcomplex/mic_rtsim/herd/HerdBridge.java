package com.minecraftindustrialcomplex.mic_rtsim.herd;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugin;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.AgentTag;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.Mth;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.PathfinderMob;
import net.minecraft.world.entity.ai.memory.MemoryModuleType;
import net.minecraft.world.entity.ai.memory.WalkTarget;
import net.minecraft.world.entity.animal.Animal;
import net.minecraft.world.entity.animal.goat.Goat;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.Vec3;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Collection;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;

/**
 * The vanilla herd adapter (docs/boundary.md §8.5; choices Y8 to Y20 in docs/overnight-choices-herdsmc.md):
 * the engine's migrating herds embodied as groups of vanilla sheep, cows, pigs, horses and goats
 * ({@link HerdBodies}), built into the core because vanilla is (D4). A {@link SessionPlugin} the core
 * registers itself: it owns the herd command range 0x8700 to 0x87FF and agent namespace 0x48. Server
 * thread only.
 *
 * <p><b>Puppets</b> follow the bridge's protocol: each body carries the core's {@code mic_rtsim:agent}
 * tag (so vanilla never saves it, PLAN.md §6.1 item 3), is persistence-required while embodied, and a
 * copy whose (agent, epoch, UUID) is not a live member is discarded as it joins ({@link #admit}). A group
 * is one unit (C25): spawned in one tick, captured whole; a member a player takes leaves it alone.
 *
 * <p><b>Per tick</b> ({@link #endOfTick}): deaths confirmed (HERD_KILLED, with the killer), takings (a
 * lead or a tamed horse: HERD_TAKEN), the ticking test (a frozen member captures the group), shearing
 * (HERD_SHEARED), goats steered, the obstacle rule, and HERD_GROUP_STATE every
 * {@value #STATE_INTERVAL} ticks.
 *
 * <p><b>Steering</b> (Y14): the group's anchor walks from where it was to the engine's waypoint at the
 * herd's speed (or to arrive at the STEER's arrive tick); each member keeps its place around it (its
 * spawn offset from the herd's centre) and is walked back toward it by {@link HerdFollowGoal} when it
 * strays more than {@value #LEASH} blocks. Otherwise vanilla's AI runs: the animals graze and wander.
 *
 * <p><b>Obstacles</b> (Y15): a member lagging more than {@link #LEASH} + {@value #LAG} blocks behind the
 * anchor's progress that has gained less than a block toward the waypoint for {@link #stallTicks} ticks
 * has failed to pass; when at least half the group present has, HERD_OBSTACLE goes once per waypoint,
 * with where the failing members stand.
 */
public final class HerdBridge implements SessionPlugin {
    public static final HerdBridge INSTANCE = new HerdBridge();

    /** HERD_GROUP_STATE every this many ticks per group (docs/boundary.md §8.3's PLACEHOLDER). */
    public static final int STATE_INTERVAL = 100;
    /** A member farther than this from its slot is walked back to it (blocks, PLACEHOLDER). */
    public static final double LEASH = 8.0;
    /** Lagging more than {@link #LEASH} + this behind the herd counts toward a stall (blocks, PLACEHOLDER). */
    public static final double LAG = 6.0;
    /** A lagging member that gains less than a block in this many ticks failed to pass (PLACEHOLDER 30 s). */
    public static int stallTicks = 600;
    /** The obstacle rule and goats' steering run every this many ticks per group (PLACEHOLDER). */
    static final int CHECK_INTERVAL = 20;

    private HerdBridge() {}

    // ---- per-session state ----------------------------------------------------------------------

    record Death(int by, UUID killer) {}

    record Capture(long member, int flags, float health, int x, int z) {}

    /** One embodied group (C25: one puppet unit). */
    public static final class Group {
        public final long agent;
        public final int epoch;
        public final ResourceKey<Level> dimension;
        public final String species;
        final List<Member> members = new ArrayList<>();
        /** The anchor walks from (ax, az) at t0 to (tx, tz), reached at {@code arrive}. */
        double ax, az, tx, tz;
        long t0, arrive;
        float speed;
        boolean obstacleReported;
        long lastState;
        int captureRequested;

        Group(long agent, int epoch, ResourceKey<Level> dimension, String species) {
            this.agent = agent;
            this.epoch = epoch;
            this.dimension = dimension;
            this.species = species;
        }

        /** Where the group's centre should be at {@code now}. */
        double[] anchor(long now) {
            double f = arrive <= t0 ? 1.0 : Mth.clamp((double) (now - t0) / (double) (arrive - t0), 0.0, 1.0);
            return new double[]{ax + (tx - ax) * f, az + (tz - az) * f};
        }

        /** How far the anchor has walked since the last waypoint was set, blocks. */
        double anchorProgress(long now) {
            double[] a = anchor(now);
            return Math.hypot(a[0] - ax, a[1] - az);
        }

        /** A new waypoint: the anchor walks there from where it is now. */
        void steer(int x, int z, float speed, long arriveTick, long now) {
            double[] a = anchor(now);
            ax = a[0];
            az = a[1];
            tx = x;
            tz = z;
            t0 = now;
            this.speed = speed;
            double d = Math.hypot(tx - ax, tz - az);
            long walk = speed > 1e-4f ? (long) Math.ceil(d / speed) : 0L;
            arrive = arriveTick > now ? arriveTick : now + walk;
            obstacleReported = false;
            for (Member m : members)
                m.resetStall(now);
        }

        /** The waypoint the group is steered toward (blocks). */
        public int targetX() {
            return (int) Math.floor(tx);
        }

        public int targetZ() {
            return (int) Math.floor(tz);
        }

        public List<Member> members() {
            return List.copyOf(members);
        }
    }

    /** One member of an embodied group. */
    public static final class Member {
        final Group group;
        public final long member;
        final UUID uuid;
        Entity entity;
        /** Its place relative to the group's anchor (its spawn offset from the herd's centre). */
        final double offX, offZ;
        HerdFollowGoal goal;
        boolean wasSheared;
        Death pendingDeath;
        List<ItemEntity> drops;
        Capture precaptured;
        boolean removing;
        double bestProgress;
        long bestAt;

        Member(Group group, long member, Entity entity, double offX, double offZ) {
            this.group = group;
            this.member = member;
            this.uuid = entity.getUUID();
            this.entity = entity;
            this.offX = offX;
            this.offZ = offZ;
        }

        public Entity entity() {
            return entity;
        }

        /** Its slot at the entity's level's game time: the anchor plus its offset, on the ground where loaded. */
        Vec3 slot(Entity e) {
            double[] a = group.anchor(e.level().getGameTime());
            double x = a[0] + offX, z = a[1] + offZ;
            double y = e.getY();
            int bx = Mth.floor(x), bz = Mth.floor(z);
            if (e.level().hasChunk(bx >> 4, bz >> 4))
                y = e.level().getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, bx, bz);
            return new Vec3(x, y, z);
        }

        /** Horizontal blocks between the member and its slot. */
        double lag(Entity e) {
            double[] a = group.anchor(e.level().getGameTime());
            return Math.hypot(a[0] + offX - e.getX(), a[1] + offZ - e.getZ());
        }

        /** How far the member has come toward the waypoint since it was set, blocks. */
        double progress() {
            double dx = group.tx - group.ax, dz = group.tz - group.az;
            double d = Math.hypot(dx, dz);
            if (d < 1e-6 || entity == null)
                return 0;
            double px = entity.getX() - (group.ax + offX), pz = entity.getZ() - (group.az + offZ);
            return (px * dx + pz * dz) / d;
        }

        void resetStall(long now) {
            bestProgress = progress();
            bestAt = now;
        }
    }

    private RtsimSession session;
    private final Map<Long, Group> byAgent = new LinkedHashMap<>();
    private final Map<UUID, Member> byUuid = new HashMap<>();
    /** The last player who used shears on a member this tick, by the member's UUID. */
    private final Map<UUID, UUID> shearers = new HashMap<>();

    // Counters for logs and the GameTests (this session).
    public long materialized, rejected, captured, killed, sheared, taken, obstacles, groupStates, steered, malformed;

    public RtsimSession session() {
        return session;
    }

    // ---- SessionPlugin: lifecycle ---------------------------------------------------------------

    @Override
    public String name() {
        return "mic_rtsim:herds";
    }

    @Override
    public void sessionStarted(RtsimSession s) {
        session = s;
        byAgent.clear();
        byUuid.clear();
        shearers.clear();
        materialized = rejected = captured = killed = sheared = taken = obstacles = groupStates = steered = malformed = 0;
        // Java's dictionary names the species from the start (docs/boundary.md §8.2).
        for (String sp : HerdBodies.SPECIES)
            s.dictId(Obs.DICT_BODY, sp);
    }

    @Override
    public void sessionStopping(RtsimSession s) {
        if (s != session)
            return;
        long now = s.gameTime();
        settleDeaths(now);
        captureAll(HerdRecords.Dematerialized.SESSION_END, now);
    }

    @Override
    public void sessionFailed(RtsimSession s) {
        if (s != session)
            return;
        long now = s.gameTime();
        for (Group g : List.copyOf(byAgent.values())) {
            try {
                capture(g, HerdRecords.Dematerialized.SESSION_END, now);
            } catch (RuntimeException e) {
                MicRtsim.LOGGER.error("[mic_rtsim] capturing herd group {} after the bridge failed", Long.toHexString(g.agent), e);
                for (Member m : g.members)
                    if (m.entity != null && !m.entity.isRemoved())
                        m.entity.discard();
                unregister(g);
            }
        }
    }

    private void captureAll(int reason, long now) {
        for (Group g : List.copyOf(byAgent.values()))
            capture(g, reason, now);
    }

    // ---- SessionPlugin: puppets -----------------------------------------------------------------

    @Override
    public boolean ownsAgent(long agent) {
        return HerdRecords.isHerdAgent(agent);
    }

    @Override
    public boolean admit(RtsimSession s, Entity e, AgentTag tag, Level level) {
        if (s != session)
            return false;
        Member m = byUuid.get(e.getUUID());
        if (m == null || m.group.agent != tag.agent() || m.group.epoch != tag.epoch() || byAgent.get(m.group.agent) != m.group)
            return false;
        if (m.entity == e)
            return true;
        // Only a dimension change may replace a live member (vanilla removes the old one first); the group
        // can no longer stand together, so it is captured (C25).
        if (m.entity != null && !(m.entity.isRemoved() && m.entity.getRemovalReason() == Entity.RemovalReason.CHANGED_DIMENSION))
            return false;
        m.entity = e;
        if (m.group.captureRequested == 0)
            m.group.captureRequested = HerdRecords.Dematerialized.UNLOADED;
        return true;
    }

    @Override
    public void left(RtsimSession s, Entity e, AgentTag tag) {
        if (s != session)
            return;
        Member m = live(e);
        if (m == null || m.group.epoch != tag.epoch() || m.removing)
            return;
        Entity.RemovalReason reason = e.getRemovalReason();
        if (reason == Entity.RemovalReason.CHANGED_DIMENSION)
            return; // the copy joins in the other level, see admit
        if (reason == Entity.RemovalReason.KILLED) {
            if (m.pendingDeath == null)
                m.pendingDeath = new Death(HerdRecords.BY_NONE, null);
            return;
        }
        if (reason == Entity.RemovalReason.DISCARDED) {
            // Not the bridge (removing is false): a command or another mod took it out of the world.
            MicRtsim.LOGGER.warn("[mic_rtsim] herd member {} of {} was removed by something else; it leaves the sim", e, Long.toHexString(m.group.agent));
            take(m, Obs.Taken.REMOVED_BY_OTHER, null, s.gameTime());
            return;
        }
        // Unloaded, or tracking ended: capture this member's state now, the group at the end of the tick
        // (not here: this runs inside the entity manager's own iteration).
        if (m.precaptured == null)
            m.precaptured = captureOf(m);
        if (m.group.captureRequested == 0)
            m.group.captureRequested = HerdRecords.Dematerialized.UNLOADED;
    }

    @Override
    public boolean take(RtsimSession s, Entity e, AgentTag tag, int how, UUID player) {
        if (s != session)
            return false;
        Member m = live(e);
        if (m == null || m.group.agent != tag.agent() || m.group.epoch != tag.epoch())
            return false;
        take(m, how, player, s.gameTime());
        return true;
    }

    // ---- SessionPlugin: commands ----------------------------------------------------------------

    @Override
    public boolean ownsCommand(int kind) {
        return HerdRecords.isHerdCommand(kind);
    }

    @Override
    public boolean accept(RtsimSession s, ByteBuffer b, int at, int kind, int size, int aux, EngineNames names) {
        if (s != session)
            sessionStarted(s);
        if (size < HerdRecords.minSize(kind)) {
            malformed++;
            MicRtsim.LOGGER.warn("[mic_rtsim] herd command 0x{} of {} bytes is shorter than its layout; skipped", Integer.toHexString(kind), size);
            return true;
        }
        long now = s.gameTime();
        return switch (kind) {
            case HerdRecords.HERD_MATERIALIZE -> materialize(b, at, size, aux, names, now);
            case HerdRecords.HERD_STEER -> {
                steer(b.getLong(at + HerdRecords.Steer.AGENT), b.getInt(at + HerdRecords.Steer.EPOCH), b.getFloat(at + HerdRecords.Steer.SPEED),
                        b.getInt(at + HerdRecords.Steer.TARGET_X), b.getInt(at + HerdRecords.Steer.TARGET_Z), b.getLong(at + HerdRecords.Steer.ARRIVE), now);
                yield true;
            }
            case HerdRecords.HERD_DESPAWN -> {
                despawn(b.getLong(at + HerdRecords.Despawn.AGENT), b.getInt(at + HerdRecords.Despawn.EPOCH), now);
                yield true;
            }
            default -> {
                malformed++;
                yield true;
            }
        };
    }

    private ServerLevel level(EngineNames names, int dimId) {
        String name = names.name(Obs.DICT_DIMENSION, dimId);
        ResourceLocation loc = name == null ? null : ResourceLocation.tryParse(name);
        return loc == null ? null : session.server().getLevel(ResourceKey.create(Registries.DIMENSION, loc));
    }

    /**
     * HERD_MATERIALIZE: the group as one unit (C25), every member at its own (x, z) on the surface, all in
     * one tick or none; answered HERD_MATERIALIZED, or HERD_SPAWN_REJECTED with §2.3's reason (9: not a
     * herd species). Returns false to wait for the next tick's spawn quota.
     */
    private boolean materialize(ByteBuffer b, int at, int size, int count, EngineNames names, long now) {
        long cmdSeq = b.getLong(at + Cmd.CMD_SEQ);
        long agent = b.getLong(at + HerdRecords.Materialize.AGENT);
        int epoch = b.getInt(at + HerdRecords.Materialize.EPOCH);
        if (!HerdRecords.isHerdAgent(agent) || count <= 0 || HerdRecords.Materialize.size(count) > size) {
            reject(agent, epoch, cmdSeq, HerdRecords.SpawnRejected.BAD_RECORD, now);
            return true;
        }
        String speciesName = names.name(Obs.DICT_BODY, b.getInt(at + HerdRecords.Materialize.SPECIES));
        EntityType<?> type = HerdBodies.type(speciesName);
        if (type == null) {
            reject(agent, epoch, cmdSeq, HerdRecords.SpawnRejected.UNKNOWN_SPECIES, now);
            return true;
        }
        ServerLevel level = level(names, b.getInt(at + HerdRecords.Materialize.DIM));
        if (level == null) {
            reject(agent, epoch, cmdSeq, HerdRecords.SpawnRejected.UNKNOWN_DIMENSION, now);
            return true;
        }
        if (byAgent.containsKey(agent)) {
            reject(agent, epoch, cmdSeq, HerdRecords.SpawnRejected.ALREADY_EMBODIED, now);
            return true;
        }
        int[] xs = new int[count], zs = new int[count];
        for (int i = 0; i < count; i++) {
            int o = at + HerdRecords.Materialize.MEMBERS + i * HerdRecords.Materialize.MEMBER;
            xs[i] = b.getInt(o + HerdRecords.Materialize.M_X);
            zs[i] = b.getInt(o + HerdRecords.Materialize.M_Z);
            if (!RtsimSession.entitiesTickAt(level, new BlockPos(xs[i], level.getSeaLevel(), zs[i]))) {
                reject(agent, epoch, cmdSeq, HerdRecords.SpawnRejected.NOT_TICKING, now);
                return true;
            }
        }
        if (!session.server().isSpawningAnimals()) {
            // ServerLevel.shouldDiscardEntity would discard every animal on its first tick.
            reject(agent, epoch, cmdSeq, HerdRecords.SpawnRejected.DISCARDED_BY_SERVER, now);
            return true;
        }
        if (!session.spendSpawnBudget(count))
            return false; // waits for the next tick, with every command after it
        int cx = b.getInt(at + HerdRecords.Materialize.CENTRE_X), cz = b.getInt(at + HerdRecords.Materialize.CENTRE_Z);
        Group g = new Group(agent, epoch, level.dimension(), speciesName);
        Animal[] bodies = new Animal[count];
        AgentTag tag = new AgentTag(agent, epoch);
        for (int i = 0; i < count; i++) {
            int o = at + HerdRecords.Materialize.MEMBERS + i * HerdRecords.Materialize.MEMBER;
            // Only now: the column is entity-ticking, so its chunk and height map are there.
            double y = level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, xs[i], zs[i]);
            Animal a;
            try {
                a = HerdBodies.create(level, type, xs[i] + 0.5, y, zs[i] + 0.5, b.getLong(o + HerdRecords.Materialize.M_SEED),
                        b.getInt(o + HerdRecords.Materialize.M_VARIANT), b.getInt(o + HerdRecords.Materialize.M_VANILLA_AGE),
                        (b.get(o + HerdRecords.Materialize.M_FLAGS) & HerdRecords.MEMBER_SHEARED) != 0);
            } catch (RuntimeException ex) {
                MicRtsim.LOGGER.error("[mic_rtsim] creating a herd {} failed", speciesName, ex);
                a = null;
            }
            if (a == null) {
                for (int j = 0; j < i; j++)
                    bodies[j].discard();
                reject(agent, epoch, cmdSeq, HerdRecords.SpawnRejected.SPAWN_FAILED, now);
                return true;
            }
            a.setPersistenceRequired();
            a.setData(RtsimAttachments.AGENT, tag);
            bodies[i] = a;
            Member m = new Member(g, b.getLong(o + HerdRecords.Materialize.M_MEMBER), a, xs[i] + 0.5 - cx, zs[i] + 0.5 - cz);
            m.wasSheared = HerdBodies.sheared(a);
            g.members.add(m);
        }
        // Registered before any joins: the join check already knows them.
        register(g);
        for (Animal a : bodies) {
            if (!level.addFreshEntity(a)) {
                for (Member m : g.members)
                    m.removing = true;
                for (Animal x : bodies)
                    if (!x.isRemoved())
                        x.discard();
                unregister(g);
                reject(agent, epoch, cmdSeq, HerdRecords.SpawnRejected.SPAWN_FAILED, now);
                return true;
            }
        }
        g.ax = g.tx = cx;
        g.az = g.tz = cz;
        g.t0 = g.arrive = now;
        g.lastState = now;
        g.steer(b.getInt(at + HerdRecords.Materialize.TARGET_X), b.getInt(at + HerdRecords.Materialize.TARGET_Z),
                b.getFloat(at + HerdRecords.Materialize.SPEED), 0L, now);
        for (Member m : g.members) {
            if (m.entity instanceof PathfinderMob p && !(p instanceof Goat)) {
                m.goal = new HerdFollowGoal(p, m);
                p.goalSelector.addGoal(HerdFollowGoal.PRIORITY, m.goal);
            }
        }
        ByteBuffer r = session.boundary().beginJournaled(HerdRecords.HERD_MATERIALIZED, HerdRecords.Materialized.SIZE, count, now);
        r.putLong(HerdRecords.Materialized.AGENT, agent);
        r.putInt(HerdRecords.Materialized.EPOCH, epoch);
        r.putInt(HerdRecords.Materialized.DIM, session.dimId(level.dimension()));
        session.boundary().commit(r);
        materialized++;
        return true;
    }

    private void reject(long agent, int epoch, long cmdSeq, int reason, long now) {
        ByteBuffer r = session.boundary().beginJournaled(HerdRecords.HERD_SPAWN_REJECTED, HerdRecords.SpawnRejected.SIZE, reason, now);
        r.putLong(HerdRecords.SpawnRejected.AGENT, agent);
        r.putInt(HerdRecords.SpawnRejected.EPOCH, epoch);
        r.putLong(HerdRecords.SpawnRejected.CMD_SEQ, cmdSeq);
        session.boundary().commit(r);
        rejected++;
    }

    /** HERD_STEER: the group's next waypoint. Ignored for a group not embodied. */
    private void steer(long agent, int epoch, float speed, int x, int z, long arrive, long now) {
        Group g = byAgent.get(agent);
        if (g == null || g.epoch != epoch)
            return;
        g.steer(x, z, speed, arrive, now);
        steered++;
    }

    /**
     * HERD_DESPAWN: the group is captured (HERD_DEMATERIALIZED, reason 1, each member's state) and removed.
     * A group not embodied is answered at once (reason 6, no members), so the engine never waits.
     */
    private void despawn(long agent, int epoch, long now) {
        Group g = byAgent.get(agent);
        if (g == null || g.epoch != epoch) {
            journalDematerialized(agent, epoch, HerdRecords.Dematerialized.NOT_EMBODIED, List.of(), now);
            return;
        }
        capture(g, HerdRecords.Dematerialized.COMMAND, now);
    }

    // ---- registry -------------------------------------------------------------------------------

    private void register(Group g) {
        byAgent.put(g.agent, g);
        for (Member m : g.members)
            byUuid.put(m.uuid, m);
    }

    private void unregister(Group g) {
        if (byAgent.get(g.agent) == g)
            byAgent.remove(g.agent);
        for (Member m : g.members)
            byUuid.remove(m.uuid, m);
    }

    /** A member leaves its group (dead, taken): its goal goes; an empty group is answered MEMBERS_GONE. */
    private void forget(Member m, long now) {
        Group g = m.group;
        g.members.remove(m);
        byUuid.remove(m.uuid, m);
        if (m.goal != null && m.entity instanceof Mob mob) {
            mob.goalSelector.removeGoal(m.goal);
            m.goal = null;
        }
        if (g.members.isEmpty() && byAgent.get(g.agent) == g) {
            byAgent.remove(g.agent);
            journalDematerialized(g.agent, g.epoch, HerdRecords.Dematerialized.MEMBERS_GONE, List.of(), now);
        }
    }

    /** The live member an entity is, or null. */
    Member live(Entity e) {
        Member m = byUuid.get(e.getUUID());
        return m != null && m.entity == e ? m : null;
    }

    /** The embodied group of an agent, or null (GameTests). */
    public Group group(long agent) {
        return byAgent.get(agent);
    }

    /** The member index a live member embodies, or -1 (GameTests). */
    public long memberOf(Entity e) {
        Member m = live(e);
        return m == null ? -1 : m.member;
    }

    /** The live bodies of an agent's group, in member order (GameTests). */
    public List<Entity> bodies(long agent) {
        Group g = byAgent.get(agent);
        List<Entity> out = new ArrayList<>();
        if (g != null)
            for (Member m : g.members)
                if (m.entity != null)
                    out.add(m.entity);
        return out;
    }

    public int embodied() {
        return byAgent.size();
    }

    // ---- the tick -------------------------------------------------------------------------------

    @Override
    public void endOfTick(RtsimSession s, long now) {
        if (s != session || s.failed())
            return;
        if (byAgent.isEmpty()) {
            shearers.clear();
            return;
        }
        settleDeaths(now);
        if (!RtsimConfig.on(Module.ECOLOGY)) {
            // The herds are switched off (C25): the engine steers and despawns nothing now, so no group
            // is left standing; it takes its herds back as they were when it is switched on (Y9).
            captureAll(HerdRecords.Dematerialized.SESSION_END, now);
            shearers.clear();
            return;
        }
        for (Group g : List.copyOf(byAgent.values()))
            pass(g, now);
        shearers.clear();
    }

    private void pass(Group g, long now) {
        if (byAgent.get(g.agent) != g)
            return;
        if (g.captureRequested != 0) {
            capture(g, g.captureRequested, now);
            return;
        }
        // Taking first: a member a player took leaves the sim before the ticking test can capture it.
        for (Member m : List.copyOf(g.members)) {
            if (m.pendingDeath != null || m.entity == null || m.entity.isRemoved())
                continue;
            int how = HerdBodies.takenBy(m.entity);
            if (how != 0)
                take(m, how, HerdBodies.taker(m.entity), now);
        }
        if (byAgent.get(g.agent) != g)
            return; // every member was taken
        // The ticking test is per group: one frozen member and the whole group goes (C25).
        for (Member m : g.members) {
            Entity e = m.entity;
            if (m.pendingDeath != null)
                continue;
            if (e == null || e.isRemoved()) {
                capture(g, HerdRecords.Dematerialized.UNLOADED, now);
                return;
            }
            if (!(e.level() instanceof ServerLevel level) || !RtsimSession.entitiesTickAt(level, e.blockPosition())) {
                capture(g, HerdRecords.Dematerialized.NOT_TICKING, now);
                return;
            }
        }
        for (Member m : List.copyOf(g.members)) {
            if (m.pendingDeath != null)
                continue;
            boolean nowSheared = HerdBodies.sheared(m.entity);
            if (nowSheared && !m.wasSheared)
                shorn(m, shearers.get(m.uuid), now);
            m.wasSheared = nowSheared;
        }
        if ((now + (g.agent & 0xF)) % CHECK_INTERVAL == 0) {
            steerGoats(g);
            obstacle(g, now);
        }
        if (now - g.lastState >= STATE_INTERVAL)
            state(g, now);
    }

    /** Goats run a brain, not goals: a goat that strayed gets its slot as its walk target. */
    private static void steerGoats(Group g) {
        for (Member m : g.members) {
            if (m.pendingDeath == null && m.entity instanceof Goat goat && m.lag(goat) > LEASH)
                goat.getBrain().setMemory(MemoryModuleType.WALK_TARGET, new WalkTarget(m.slot(goat), 1.0f, 2));
        }
    }

    /** The obstacle rule (Y15): most of the group lagging and stalled on the way to the waypoint. */
    private void obstacle(Group g, long now) {
        if (g.obstacleReported || g.members.isEmpty())
            return;
        double anchor = g.anchorProgress(now);
        int present = 0, failing = 0;
        double sx = 0, sz = 0;
        for (Member m : g.members) {
            if (m.pendingDeath != null || m.entity == null)
                continue;
            present++;
            double p = m.progress();
            if (p > m.bestProgress + 1.0) {
                m.bestProgress = p;
                m.bestAt = now;
            }
            boolean lagging = anchor - p > LEASH + LAG;
            if (lagging && now - m.bestAt >= stallTicks) {
                failing++;
                sx += m.entity.getX();
                sz += m.entity.getZ();
            }
        }
        if (present == 0 || failing * 2 < present)
            return;
        g.obstacleReported = true;
        ByteBuffer r = session.boundary().beginJournaled(HerdRecords.HERD_OBSTACLE, HerdRecords.Obstacle.SIZE, failing, now);
        r.putLong(HerdRecords.Obstacle.AGENT, g.agent);
        r.putInt(HerdRecords.Obstacle.EPOCH, g.epoch);
        r.putInt(HerdRecords.Obstacle.PRESENT, present);
        r.putInt(HerdRecords.Obstacle.STUCK_X, Mth.floor(sx / failing));
        r.putInt(HerdRecords.Obstacle.STUCK_Z, Mth.floor(sz / failing));
        r.putInt(HerdRecords.Obstacle.TARGET_X, g.targetX());
        r.putInt(HerdRecords.Obstacle.TARGET_Z, g.targetZ());
        session.boundary().commit(r);
        obstacles++;
    }

    private void state(Group g, long now) {
        int dim = session.dimId(g.dimension); // before direct(): an id is defined before its first use
        double x = 0, y = 0, z = 0;
        int present = 0;
        for (Member m : g.members) {
            if (m.pendingDeath != null || m.entity == null)
                continue;
            x += m.entity.getX();
            y += m.entity.getY();
            z += m.entity.getZ();
            present++;
        }
        int at = session.boundary().direct(HerdRecords.HERD_GROUP_STATE, HerdRecords.GroupState.SIZE, HerdRecords.GroupState.FLAG_TICKING, now);
        if (at < 0)
            return; // dropped for space: sent again at the next interval
        ByteBuffer b = session.boundary().openBuffer();
        b.putLong(at + HerdRecords.GroupState.AGENT, g.agent);
        b.putInt(at + HerdRecords.GroupState.EPOCH, g.epoch);
        b.putInt(at + HerdRecords.GroupState.DIM, dim);
        b.putInt(at + HerdRecords.GroupState.PRESENT, present);
        if (present > 0) {
            b.putDouble(at + HerdRecords.GroupState.X, x / present);
            b.putDouble(at + HerdRecords.GroupState.Y, y / present);
            b.putDouble(at + HerdRecords.GroupState.Z, z / present);
        }
        g.lastState = now;
        groupStates++;
    }

    // ---- capture --------------------------------------------------------------------------------

    private static Capture captureOf(Member m) {
        Entity e = m.entity;
        float health = e instanceof LivingEntity l ? l.getHealth() : 0f;
        return new Capture(m.member, HerdBodies.flags(e), health, Mth.floor(e.getX()), Mth.floor(e.getZ()));
    }

    /**
     * Captures a group and removes its bodies (HERD_DEMATERIALIZED with one capture per member present:
     * its index, sheared flag, health and position), all in this call.
     */
    void capture(Group g, int reason, long now) {
        // A member that died this tick is journaled dead first, never captured alive.
        settleDeaths(g, now);
        if (byAgent.get(g.agent) != g)
            return; // its last member died: answered MEMBERS_GONE
        List<Capture> caps = new ArrayList<>();
        for (Member m : g.members) {
            if (m.pendingDeath != null)
                continue; // a death another mod may still cancel: not captured alive either
            Capture c = m.precaptured != null ? m.precaptured : m.entity == null ? null : captureOf(m);
            if (c != null)
                caps.add(c);
        }
        for (Member m : g.members) {
            m.removing = true;
            if (m.goal != null && m.entity instanceof Mob mob)
                mob.goalSelector.removeGoal(m.goal);
            if (m.entity != null && !m.entity.isRemoved() && m.pendingDeath == null) {
                m.entity.ejectPassengers();
                m.entity.discard();
            }
        }
        unregister(g);
        journalDematerialized(g.agent, g.epoch, reason, caps, now);
        captured++;
    }

    private void journalDematerialized(long agent, int epoch, int reason, List<Capture> caps, long now) {
        int n = Math.min(caps.size(), (0xFFF8 - HerdRecords.Dematerialized.CAPTURES) / HerdRecords.Dematerialized.CAPTURE);
        ByteBuffer r = session.boundary().beginJournaled(HerdRecords.HERD_DEMATERIALIZED, HerdRecords.Dematerialized.size(n), reason, now);
        r.putLong(HerdRecords.Dematerialized.AGENT, agent);
        r.putInt(HerdRecords.Dematerialized.EPOCH, epoch);
        r.putInt(HerdRecords.Dematerialized.COUNT, n);
        for (int i = 0; i < n; i++) {
            Capture c = caps.get(i);
            int o = HerdRecords.Dematerialized.CAPTURES + i * HerdRecords.Dematerialized.CAPTURE;
            r.putLong(o + HerdRecords.Dematerialized.C_MEMBER, c.member());
            r.putInt(o + HerdRecords.Dematerialized.C_FLAGS, c.flags());
            r.putFloat(o + HerdRecords.Dematerialized.C_HEALTH, c.health());
            r.putInt(o + HerdRecords.Dematerialized.C_X, c.x());
            r.putInt(o + HerdRecords.Dematerialized.C_Z, c.z());
        }
        session.boundary().commit(r);
    }

    // ---- deaths, takings, shearing --------------------------------------------------------------

    static int byKind(Entity killer) {
        if (killer == null)
            return HerdRecords.BY_NONE;
        if (killer instanceof Player)
            return HerdRecords.BY_PLAYER;
        if (killer instanceof Mob)
            return HerdRecords.BY_MOB;
        return HerdRecords.BY_OTHER;
    }

    /** LivingDeathEvent (lowest priority, not cancelled so far): journaled at the end of the tick, once certain. */
    void onDeath(LivingEntity dead, DamageSource source) {
        Member m = live(dead);
        if (m == null)
            return;
        Entity killer = source.getEntity();
        m.pendingDeath = new Death(byKind(killer), killer == null ? null : killer.getUUID());
    }

    /** LivingDropsEvent: a dying member's drops carry its HERD_KILLED until the engine has it (§3.1). */
    void onDrops(LivingEntity dead, Collection<ItemEntity> drops) {
        Member m = live(dead);
        if (m != null && m.pendingDeath != null)
            m.drops = new ArrayList<>(drops);
    }

    /** A player used shears on a member (PlayerInteractEvent.EntityInteract): named by HERD_SHEARED if it is shorn. */
    void onShearsUsed(Entity target, Player player) {
        if (live(target) != null)
            shearers.put(target.getUUID(), player.getUUID());
    }

    private void settleDeaths(long now) {
        for (Group g : List.copyOf(byAgent.values()))
            settleDeaths(g, now);
    }

    private void settleDeaths(Group g, long now) {
        for (Member m : List.copyOf(g.members)) {
            if (m.pendingDeath == null)
                continue;
            Entity e = m.entity;
            boolean dead = e == null || e.isRemoved() || (e instanceof LivingEntity l && l.isDeadOrDying());
            if (!dead) {
                m.pendingDeath = null; // another mod cancelled it
                m.drops = null;
                continue;
            }
            ByteBuffer r = session.boundary().beginJournaled(HerdRecords.HERD_KILLED, HerdRecords.Killed.SIZE, m.pendingDeath.by(), now);
            r.putLong(HerdRecords.Killed.AGENT, g.agent);
            r.putInt(HerdRecords.Killed.EPOCH, g.epoch);
            r.putLong(HerdRecords.Killed.MEMBER, m.member);
            if (m.pendingDeath.killer() != null) {
                r.putLong(HerdRecords.Killed.KILLER_MOST, m.pendingDeath.killer().getMostSignificantBits());
                r.putLong(HerdRecords.Killed.KILLER_LEAST, m.pendingDeath.killer().getLeastSignificantBits());
            }
            session.boundary().commitWithOpId(r, HerdRecords.Killed.OP_ID);
            long seq = r.getLong(Obs.OBS_SEQ);
            // Its drops carry the kill: a drop saved on a chunk unload before the journal must not
            // let the engine bring the animal back as well (docs/boundary.md §3.1).
            if (m.drops != null)
                for (ItemEntity drop : m.drops)
                    if (!drop.isRemoved())
                        session.carryOnEntity(drop, List.of(r.array()), seq);
            m.drops = null;
            m.removing = true;
            killed++;
            forget(m, now);
        }
    }

    /**
     * A member leaves the sim and becomes an ordinary animal (A10): HERD_TAKEN with a Java op id, its tag
     * goes (vanilla saves it from now on), and it carries its HERD_TAKEN until the engine has it; the rest
     * of the group stays one unit.
     */
    void take(Member m, int how, UUID player, long now) {
        Group g = m.group;
        Entity e = m.entity;
        ByteBuffer r = session.boundary().beginJournaled(HerdRecords.HERD_TAKEN, HerdRecords.Taken.SIZE, how, now);
        r.putLong(HerdRecords.Taken.AGENT, g.agent);
        r.putInt(HerdRecords.Taken.EPOCH, g.epoch);
        r.putLong(HerdRecords.Taken.MEMBER, m.member);
        if (player != null) {
            r.putLong(HerdRecords.Taken.PLAYER_MOST, player.getMostSignificantBits());
            r.putLong(HerdRecords.Taken.PLAYER_LEAST, player.getLeastSignificantBits());
        }
        session.boundary().commitWithOpId(r, HerdRecords.Taken.OP_ID);
        m.removing = true;
        if (e != null) {
            e.removeData(RtsimAttachments.AGENT);
            if (!e.isRemoved())
                session.carryOnEntity(e, List.of(r.array()), r.getLong(Obs.OBS_SEQ));
        }
        taken++;
        forget(m, now);
    }

    private void shorn(Member m, UUID player, long now) {
        ByteBuffer r = session.boundary().beginJournaled(HerdRecords.HERD_SHEARED, HerdRecords.Sheared.SIZE, 0, now);
        r.putLong(HerdRecords.Sheared.AGENT, m.group.agent);
        r.putInt(HerdRecords.Sheared.EPOCH, m.group.epoch);
        r.putLong(HerdRecords.Sheared.MEMBER, m.member);
        if (player != null) {
            r.putLong(HerdRecords.Sheared.PLAYER_MOST, player.getMostSignificantBits());
            r.putLong(HerdRecords.Sheared.PLAYER_LEAST, player.getLeastSignificantBits());
        }
        session.boundary().commit(r);
        sheared++;
    }
}
