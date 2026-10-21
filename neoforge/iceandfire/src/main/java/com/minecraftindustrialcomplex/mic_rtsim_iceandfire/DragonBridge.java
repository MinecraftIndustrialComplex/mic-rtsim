package com.minecraftindustrialcomplex.mic_rtsim_iceandfire;

import com.iafenvoy.iceandfire.entity.DragonBaseEntity;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugin;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.AgentTag;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.village.Village;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonRecords.Adopt;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonRecords.Depart;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonRecords.Discard;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonRecords.Materialize;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonRecords.Steer;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.MobSpawnType;
import net.minecraft.world.entity.animal.Animal;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.Vec3;
import net.neoforged.bus.api.ICancellableEvent;

import java.nio.ByteBuffer;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;

/**
 * The dragon body adapter and the plugin's whole session (docs/boundary.md §6.5, §6.6): the engine's
 * dragon commands applied to Ice and Fire's dragons, and everything the dragons do reported back.
 * Server thread only.
 *
 * <p><b>Puppets.</b> A materialized or adopted dragon carries the core's {@code mic_rtsim:agent} tag
 * (so vanilla never saves it, PLAN.md §6.1 item 3) and is persistence-required; the core asks this
 * plugin, which owns namespace 0x44 (R13), whether a tagged dragon joining a level is live. It is one
 * puppet of one body; prey it carries goes with it as one unit (C25, choice Z8).
 *
 * <p><b>Per tick</b>, after the core's own work and before the hand-over ({@link #endOfTick}): lairs from
 * loaded chunks, their chests (LAIR_CHEST, looked for every 100 ticks), untagged dragons that joined,
 * wounds summed over the tick, deaths confirmed, the puppet pass (captures, the ticking test, departures,
 * steering, DRAGON_STATE every 100 ticks), meals and robbed hoards summed over the tick.
 */
public final class DragonBridge implements SessionPlugin {
    public static final DragonBridge INSTANCE = new DragonBridge();

    /** DRAGON_STATE every this many ticks per embodied dragon (PLACEHOLDER, docs/boundary.md §6.3). */
    public static final int STATE_INTERVAL = 100;
    /** A steered dragon this close to its target (horizontally, blocks) has arrived (PLACEHOLDER). */
    public static final double ARRIVE = 6.0;
    /** MATERIALIZE flag 1 raises y to the motion-blocking surface plus this (PLACEHOLDER 8). */
    public static final int CLAMP_MARGIN = 8;
    /** A DEPART with timeout 0 is captured after this many ticks at most (PLACEHOLDER, choice Z11). */
    public static final int DEFAULT_DEPART_TIMEOUT = 6000;
    /** A puppet's breaths are summed into one DRAGON_BREATHS at most this often (OILGATE.md §4.3). */
    public static final int BREATH_INTERVAL = 20;
    /** Fire events this many ticks apart or more are two bursts in a puppet's breath log (GameTests). */
    public static final int BURST_GAP = 6;
    /** Bursts kept per puppet in the breath log (GameTests). */
    public static final int BURSTS_KEPT = 64;

    private DragonBridge() {}

    // ---- per-session state ----------------------------------------------------------------------

    enum Mode { IDLE, STEER, SETTLE, DEPART }

    /** What a body's capture reads (health, hunger, age, position). */
    record Snapshot(ResourceKey<Level> dimension, float health, float maxHealth, int hunger, int ageTicks, double x, double y, double z) {}

    record Death(int killerKind, UUID killer) {}

    /** One embodied dragon. */
    static final class Puppet {
        final long agent;
        final int epoch;
        final UUID uuid;
        Entity entity;
        Mode mode = Mode.IDLE;
        Vec3 target;
        long deadline;
        long lastState;
        int captureRequested;
        Snapshot precaptured;
        Death pendingDeath;
        boolean removing;
        /** The oil reserve the engine gave the body (MAT_OIL), spent a point a breath; -1 breathes freely. */
        int oil = -1;
        /** Breaths since the last DRAGON_BREATHS. */
        int pendingBreaths;
        long lastBreathFlush;
        /** The game tick of the last fire event paid for (a charge spawned on that tick is the same breath). */
        long lastFireTick = Long.MIN_VALUE;
        final Breath breath = new Breath();

        Puppet(long agent, int epoch, Entity entity, long now) {
            this.agent = agent;
            this.epoch = epoch;
            this.uuid = entity.getUUID();
            this.entity = entity;
            this.lastState = now;
        }

        DragonBaseEntity dragon() {
            return entity instanceof DragonBaseEntity d ? d : null;
        }
    }

    /**
     * What one puppet's fire did this session (GameTests and logs): every fire event and charge seen,
     * what was paid from the reserve, what was stopped, and the last bursts (fire events less than
     * {@link #BURST_GAP} ticks apart, with the charges spawned among them).
     */
    public static final class Breath {
        public int events, eventsPaid, eventsCancelled, chargesWithEvent, chargesPaid, chargesStopped, reported;
        long lastSeen = Long.MIN_VALUE;
        public final ArrayDeque<Burst> bursts = new ArrayDeque<>();

        void saw(long now, boolean event, boolean charge) {
            if (bursts.isEmpty() || now - lastSeen >= BURST_GAP) {
                bursts.addLast(new Burst(now));
                if (bursts.size() > BURSTS_KEPT)
                    bursts.removeFirst();
            }
            lastSeen = now;
            Burst b = bursts.peekLast();
            b.last = now;
            if (event)
                b.events++;
            if (charge)
                b.charges++;
        }
    }

    /** Fire events less than {@link #BURST_GAP} ticks apart, and the charges among them. */
    public static final class Burst {
        public final long start;
        public long last;
        public int events, charges;

        Burst(long start) {
            this.start = start;
            this.last = start;
        }

        @Override
        public String toString() {
            return events + " events" + (charges > 0 ? " + " + charges + " charge" + (charges > 1 ? "s" : "") : "") + " over "
                    + (last - start + 1) + " ticks";
        }
    }

    /** A dead puppet's body, still tagged until the engine has its DRAGON_DIED (choice Z6). */
    record Corpse(long agent, int epoch, ResourceKey<Level> dimension) {}

    private record HurtKey(long agent, UUID attacker) {}

    private static final class HurtSum {
        int epoch;
        int kind;
        int hits;
        float amount;
    }

    private record AteKey(long agent, int epoch, String prey, long village) {}

    private static final class AteSum {
        int count;
        double x, y, z;
    }

    private record Kill(Puppet by, LivingEntity victim, double x, double y, double z) {}

    private RtsimSession session;
    private final Map<Long, Puppet> byAgent = new HashMap<>();
    private final Map<UUID, Puppet> byUuid = new HashMap<>();
    private final Map<UUID, Corpse> corpses = new HashMap<>();
    private final ArrayDeque<Entity> joined = new ArrayDeque<>();
    private final Set<UUID> untaggedReported = new HashSet<>();
    private final Map<HurtKey, HurtSum> hurts = new LinkedHashMap<>();
    private final List<Kill> kills = new ArrayList<>();
    private final Map<AteKey, AteSum> meals = new LinkedHashMap<>();
    /** Prey a puppet picked up: the village it was taken from (0 none), for its DRAGON_ATE. */
    private final Map<UUID, Long> pickedUpFrom = new HashMap<>();
    public final Lairs lairs = new Lairs(this);
    /** PLACE_LAIR (OG-7). */
    public final LairPlacer placer = new LairPlacer(this);
    /** OIL_SURVEY answers and new chunks' oil (OG-2). */
    public final OilSampler sampler = new OilSampler(this);

    // Counters for logs and the GameTests (this session).
    public long materialized, rejected, adopted, discarded, captured, died, hurtRecords, ateRecords, untagged, stateRecords,
            steered, malformed, breathRecords, breathsReported, fireCancelled, chargesStopped, oilConfigs;
    /** The flags of the last OIL_CONFIG sent this session (-1 none). */
    public int oilConfigSent = -1;
    /** The obsSeq of this session's first OIL_CONFIG (the one sent at session start); -1 before it. */
    public long sessionOilConfigSeq = -1;
    /** The flags of each body's DRAGON_MATERIALIZE this session, the last 1,024 agents (GameTests). */
    private final Map<Long, Integer> materializeFlags = new LinkedHashMap<>() {
        @Override
        protected boolean removeEldestEntry(Map.Entry<Long, Integer> eldest) {
            return size() > 1024;
        }
    };

    /** The flags of {@code agent}'s last DRAGON_MATERIALIZE this session that made a body; -1 none. */
    public int materializeFlagsOf(long agent) {
        Integer f = materializeFlags.get(agent);
        return f == null ? -1 : f;
    }

    public RtsimSession session() {
        return session;
    }

    // ---- SessionPlugin: lifecycle ---------------------------------------------------------------

    @Override
    public String name() {
        return MicRtsimIceAndFire.MODID;
    }

    @Override
    public void sessionStarted(RtsimSession s) {
        session = s;
        byAgent.clear();
        byUuid.clear();
        corpses.clear();
        untaggedReported.clear();
        hurts.clear();
        kills.clear();
        meals.clear();
        pickedUpFrom.clear();
        lairs.clear();
        placer.clear();
        sampler.clear();
        materializeFlags.clear();
        materialized = rejected = adopted = discarded = captured = died = hurtRecords = ateRecords = untagged = stateRecords = steered = malformed = 0;
        breathRecords = breathsReported = fireCancelled = chargesStopped = oilConfigs = 0;
        oilConfigSent = -1;
        sessionOilConfigSeq = -1;
        // Java's dictionary names the species and lair structures from the start (docs/boundary.md §6.2).
        for (String sp : IafDragons.SPECIES)
            s.dictId(Obs.DICT_BODY, sp);
        for (String[] pair : IafDragons.LAIRS)
            for (String l : pair)
                s.dictId(Obs.DICT_STRUCTURE, l);
        // The oil gate's switches, before the session's first PLAYER (the core reports players at the end
        // of a tick, and no tick has run yet).
        sendOilConfig(DragonConfig.oilFlags(), s.gameTime());
    }

    /**
     * OIL_CONFIG: the oil gate's switches ({@link DragonConfig#oilFlags}); the engine takes the last one
     * it gets. Sent at session start, and again by a GameTest that changes a switch.
     */
    public void sendOilConfig(int flags, long now) {
        ByteBuffer r = session.boundary().beginJournaled(DragonRecords.OIL_CONFIG, DragonRecords.OilConfig.SIZE, flags, now);
        long seq = session.boundary().commit(r);
        if (oilConfigs == 0)
            sessionOilConfigSeq = seq;
        oilConfigSent = flags;
        oilConfigs++;
    }

    @Override
    public void sessionStopping(RtsimSession s) {
        if (s != session)
            return;
        long now = s.gameTime();
        endOfTickRecords(now);
        for (Puppet p : List.copyOf(byAgent.values()))
            capture(p, DragonRecords.Dematerialized.SESSION_END, now);
        // The journal holding their deaths is saved with this world save: the corpses are ordinary from now.
        for (UUID id : List.copyOf(corpses.keySet()))
            untagCorpse(id);
    }

    @Override
    public void sessionFailed(RtsimSession s) {
        if (s != session)
            return;
        long now = s.gameTime();
        for (Puppet p : List.copyOf(byAgent.values())) {
            try {
                capture(p, DragonRecords.Dematerialized.SESSION_END, now);
            } catch (RuntimeException e) {
                MicRtsimIceAndFire.LOGGER.error("[mic_rtsim_iceandfire] capturing dragon {} after the bridge failed", p.agent, e);
                if (p.entity != null && !p.entity.isRemoved())
                    p.entity.discard();
            }
        }
        for (UUID id : List.copyOf(corpses.keySet()))
            untagCorpse(id);
    }

    // ---- SessionPlugin: puppets -----------------------------------------------------------------

    @Override
    public boolean ownsAgent(long agent) {
        return DragonRecords.isDragonAgent(agent);
    }

    @Override
    public boolean admit(RtsimSession s, Entity e, AgentTag tag, Level level) {
        if (s != session)
            return false;
        Puppet p = byAgent.get(tag.agent());
        if (p != null && p.epoch == tag.epoch() && p.uuid.equals(e.getUUID())) {
            if (p.entity == e)
                return true;
            // Only a dimension change may replace the body (vanilla removes the old one first); the
            // puppet can no longer stay where the engine put it, so it is captured.
            if (p.entity != null && !(p.entity.isRemoved() && p.entity.getRemovalReason() == Entity.RemovalReason.CHANGED_DIMENSION))
                return false;
            p.entity = e;
            if (p.captureRequested == 0)
                p.captureRequested = DragonRecords.Dematerialized.UNLOADED;
            return true;
        }
        Corpse c = corpses.get(e.getUUID());
        return c != null && c.agent() == tag.agent() && c.epoch() == tag.epoch();
    }

    @Override
    public void left(RtsimSession s, Entity e, AgentTag tag) {
        if (s != session)
            return;
        Puppet p = byUuid.get(e.getUUID());
        if (p == null || p.entity != e || p.epoch != tag.epoch()) {
            if (e.getRemovalReason() != null && e.getRemovalReason().shouldDestroy())
                corpses.remove(e.getUUID()); // a corpse harvested to the end
            return;
        }
        if (p.removing)
            return;
        Entity.RemovalReason reason = e.getRemovalReason();
        if (reason == Entity.RemovalReason.CHANGED_DIMENSION)
            return; // the copy joins in the other level, see admit
        if (reason == Entity.RemovalReason.KILLED) {
            if (p.pendingDeath == null)
                p.pendingDeath = new Death(DragonRecords.ATTACKER_NONE, null);
            return;
        }
        // Unloaded, or removed by something else (another mod, a command): the body is lost to the sim,
        // which takes the dragon back where it was last seen (docs/boundary.md §6.5).
        if (p.precaptured == null)
            p.precaptured = snapshot(e);
        if (p.captureRequested == 0)
            p.captureRequested = DragonRecords.Dematerialized.UNLOADED;
    }

    // ---- SessionPlugin: commands ----------------------------------------------------------------

    @Override
    public boolean ownsCommand(int kind) {
        return DragonRecords.isDragonCommand(kind);
    }

    @Override
    public boolean accept(RtsimSession s, ByteBuffer b, int at, int kind, int size, int aux, EngineNames names) {
        if (s != session)
            sessionStarted(s);
        if (size < DragonRecords.minSize(kind)) {
            malformed++;
            MicRtsimIceAndFire.LOGGER.warn("[mic_rtsim_iceandfire] dragon command 0x{} of {} bytes is shorter than its layout; skipped",
                    Integer.toHexString(kind), size);
            return true;
        }
        long now = s.gameTime();
        return switch (kind) {
            case DragonRecords.DRAGON_MATERIALIZE -> materialize(b, at, aux, names, now);
            case DragonRecords.DRAGON_STEER -> {
                steer(b.getLong(at + Steer.AGENT), b.getInt(at + Steer.EPOCH), aux,
                        new Vec3(b.getDouble(at + Steer.X), b.getDouble(at + Steer.Y), b.getDouble(at + Steer.Z)));
                yield true;
            }
            case DragonRecords.DRAGON_DEPART -> {
                depart(b.getLong(at + Depart.AGENT), b.getInt(at + Depart.EPOCH), b.getInt(at + Depart.TIMEOUT_TICKS),
                        new Vec3(b.getDouble(at + Depart.X), b.getDouble(at + Depart.Y), b.getDouble(at + Depart.Z)), now);
                yield true;
            }
            case DragonRecords.DRAGON_ADOPT -> {
                adopt(b, at, now);
                yield true;
            }
            case DragonRecords.DRAGON_DISCARD -> {
                discardCommand(b.getLong(at + com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd.CMD_SEQ),
                        new UUID(b.getLong(at + Discard.ENTITY_MOST), b.getLong(at + Discard.ENTITY_LEAST)), now);
                yield true;
            }
            case DragonRecords.PLACE_LAIR -> {
                placer.request(b.getLong(at + DragonRecords.PlaceLair.FIELD), names.name(Obs.DICT_DIMENSION, b.getInt(at + DragonRecords.PlaceLair.DIM)),
                        names.name(Obs.DICT_STRUCTURE, b.getInt(at + DragonRecords.PlaceLair.STRUCTURE)), b.getInt(at + DragonRecords.PlaceLair.X),
                        b.getInt(at + DragonRecords.PlaceLair.Z), b.getInt(at + DragonRecords.PlaceLair.RADIUS));
                yield true;
            }
            case DragonRecords.OIL_SURVEY -> {
                sampler.survey(names.name(Obs.DICT_DIMENSION, b.getInt(at + DragonRecords.OilSurvey.DIM)), b.getInt(at + DragonRecords.OilSurvey.X),
                        b.getInt(at + DragonRecords.OilSurvey.Z), b.getInt(at + DragonRecords.OilSurvey.RADIUS), now);
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

    /** DRAGON_MATERIALIZE: a new body for an abstract dragon (docs/boundary.md §6.5). */
    private boolean materialize(ByteBuffer b, int at, int flags, EngineNames names, long now) {
        long cmdSeq = b.getLong(at + com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd.CMD_SEQ);
        long agent = b.getLong(at + Materialize.AGENT);
        int epoch = b.getInt(at + Materialize.EPOCH);
        if (!DragonRecords.isDragonAgent(agent)) {
            reject(agent, epoch, cmdSeq, DragonRecords.SpawnRejected.BAD_RECORD, now);
            return true;
        }
        EntityType<?> type = IafDragons.speciesType(names.name(Obs.DICT_BODY, b.getInt(at + Materialize.SPECIES)));
        if (type == null) {
            reject(agent, epoch, cmdSeq, DragonRecords.SpawnRejected.UNKNOWN_SPECIES, now);
            return true;
        }
        ServerLevel level = level(names, b.getInt(at + Materialize.DIM));
        if (level == null) {
            reject(agent, epoch, cmdSeq, DragonRecords.SpawnRejected.UNKNOWN_DIMENSION, now);
            return true;
        }
        if (byAgent.containsKey(agent)) {
            reject(agent, epoch, cmdSeq, DragonRecords.SpawnRejected.ALREADY_EMBODIED, now);
            return true;
        }
        double x = b.getDouble(at + Materialize.X), y = b.getDouble(at + Materialize.Y), z = b.getDouble(at + Materialize.Z);
        if (!RtsimSession.entitiesTickAt(level, BlockPos.containing(x, y, z))) {
            reject(agent, epoch, cmdSeq, DragonRecords.SpawnRejected.NOT_TICKING, now);
            return true;
        }
        if (!session.spendSpawnBudget(1))
            return false; // waits for the next tick, with every command after it
        if ((flags & Materialize.FLAG_CLAMP_ABOVE_SURFACE) != 0) {
            // Only now: the position is entity-ticking, so its chunk and height map are there.
            int surface = level.getHeight(Heightmap.Types.MOTION_BLOCKING, (int) Math.floor(x), (int) Math.floor(z));
            if (y < surface + CLAMP_MARGIN)
                y = surface + CLAMP_MARGIN;
        }
        Entity e;
        try {
            e = type.create(level);
        } catch (RuntimeException ex) {
            MicRtsimIceAndFire.LOGGER.error("[mic_rtsim_iceandfire] creating a {} failed", type, ex);
            e = null;
        }
        if (!(e instanceof DragonBaseEntity d)) {
            if (e != null)
                e.discard();
            reject(agent, epoch, cmdSeq, DragonRecords.SpawnRejected.SPAWN_FAILED, now);
            return true;
        }
        if (!session.server().isSpawningAnimals() && d instanceof Animal) {
            // ServerLevel.shouldDiscardEntity would discard it on its first tick.
            d.discard();
            reject(agent, epoch, cmdSeq, DragonRecords.SpawnRejected.DISCARDED_BY_SERVER, now);
            return true;
        }
        boolean sleeping = (flags & Materialize.FLAG_SLEEPING) != 0;
        BlockPos home = (flags & Materialize.FLAG_HAS_HOME) != 0
                ? new BlockPos(b.getInt(at + Materialize.HOME_X), b.getInt(at + Materialize.HOME_Y), b.getInt(at + Materialize.HOME_Z))
                : null;
        d.moveTo(x, y, z, level.random.nextFloat() * 360f, 0f);
        IafDragons.write(d, level, b.getInt(at + Materialize.AGE_TICKS), b.getInt(at + Materialize.HUNGER),
                b.getInt(at + Materialize.GENDER) == 1, b.getInt(at + Materialize.VARIANT), sleeping, home);
        float health = b.getFloat(at + Materialize.HEALTH);
        d.setHealth(health > 0 ? Math.min(health, d.getMaxHealth()) : d.getMaxHealth());
        d.setPersistenceRequired();
        d.setData(RtsimAttachments.AGENT, new AgentTag(agent, epoch));
        Puppet p = new Puppet(agent, epoch, d, now);
        if ((flags & Materialize.FLAG_OIL) != 0)
            p.oil = Math.max(0, b.getInt(at + Materialize.OIL));
        p.lastBreathFlush = now;
        register(p);
        if (!level.addFreshEntity(d)) {
            unregister(p);
            if (!d.isRemoved())
                d.discard();
            reject(agent, epoch, cmdSeq, DragonRecords.SpawnRejected.SPAWN_FAILED, now);
            return true;
        }
        if (!sleeping) {
            p.mode = Mode.STEER;
            p.target = new Vec3(b.getDouble(at + Materialize.TARGET_X), b.getDouble(at + Materialize.TARGET_Y), b.getDouble(at + Materialize.TARGET_Z));
            IafDragons.flyTo(d, session.steerImage(d, p.target));
        }
        if ((flags & Materialize.FLAG_HOSTILE) != 0) {
            UUID target = new UUID(b.getLong(at + Materialize.TARGET_PLAYER_MOST), b.getLong(at + Materialize.TARGET_PLAYER_LEAST));
            // The level's own players: a GameTest's players stand in the level, not in the player list.
            Player player = level.getPlayerByUUID(target);
            if (player != null && !player.isSpectator())
                d.setTarget(player);
        }
        journalMaterialized(p, level, now);
        materialized++;
        materializeFlags.put(agent, flags);
        return true;
    }

    private void journalMaterialized(Puppet p, ServerLevel level, long now) {
        int dim = session.dimId(level.dimension());
        ByteBuffer r = session.boundary().beginJournaled(DragonRecords.DRAGON_MATERIALIZED, DragonRecords.Materialized.SIZE, 0, now);
        r.putLong(DragonRecords.Materialized.AGENT, p.agent);
        r.putInt(DragonRecords.Materialized.EPOCH, p.epoch);
        r.putInt(DragonRecords.Materialized.DIM, dim);
        r.putLong(DragonRecords.Materialized.ENTITY_MOST, p.uuid.getMostSignificantBits());
        r.putLong(DragonRecords.Materialized.ENTITY_LEAST, p.uuid.getLeastSignificantBits());
        session.boundary().commit(r);
    }

    private void reject(long agent, int epoch, long cmdSeq, int reason, long now) {
        ByteBuffer r = session.boundary().beginJournaled(DragonRecords.DRAGON_SPAWN_REJECTED, DragonRecords.SpawnRejected.SIZE, reason, now);
        r.putLong(DragonRecords.SpawnRejected.AGENT, agent);
        r.putInt(DragonRecords.SpawnRejected.EPOCH, epoch);
        r.putLong(DragonRecords.SpawnRejected.CMD_SEQ, cmdSeq);
        session.boundary().commit(r);
        rejected++;
    }

    /** DRAGON_STEER: the flight target; flag 1 lands it there. Ignored for a dragon not embodied. */
    private void steer(long agent, int epoch, int flags, Vec3 target) {
        Puppet p = byAgent.get(agent);
        if (p == null || p.epoch != epoch || p.mode == Mode.DEPART)
            return;
        p.mode = (flags & Steer.FLAG_SETTLE) != 0 ? Mode.SETTLE : Mode.STEER;
        p.target = target;
        steered++;
        DragonBaseEntity d = p.dragon();
        if (d != null && !d.isRemoved())
            steerTick(p, d);
    }

    /**
     * DRAGON_DEPART: fly to the target, and once no player has the dragon in entity-ticking range (or
     * the timeout passed) capture it: DRAGON_DEMATERIALIZED, reason 1. A dragon that is not embodied is
     * answered at once (reason 6), so the engine never waits.
     */
    private void depart(long agent, int epoch, int timeout, Vec3 target, long now) {
        Puppet p = byAgent.get(agent);
        if (p == null || p.epoch != epoch) {
            ByteBuffer r = session.boundary().beginJournaled(DragonRecords.DRAGON_DEMATERIALIZED, DragonRecords.Dematerialized.SIZE,
                    DragonRecords.Dematerialized.NOT_EMBODIED, now);
            r.putLong(DragonRecords.Dematerialized.AGENT, agent);
            r.putInt(DragonRecords.Dematerialized.EPOCH, epoch);
            session.boundary().commit(r);
            return;
        }
        p.mode = Mode.DEPART;
        p.target = target;
        p.deadline = now + (timeout > 0 ? timeout : DEFAULT_DEPART_TIMEOUT);
        DragonBaseEntity d = p.dragon();
        if (d != null && !d.isRemoved())
            IafDragons.flyTo(d, session.steerImage(d, target));
    }

    /**
     * DRAGON_ADOPT: an untagged dragon becomes a puppet as it stands (docs/boundary.md §6.5), answered
     * DRAGON_MATERIALIZED with its UUID; DRAGON_SPAWN_REJECTED(5) when the body is gone, dead or tamed
     * (a tamed dragon is a player's, A10), (4) when it is already a puppet.
     */
    private void adopt(ByteBuffer b, int at, long now) {
        long cmdSeq = b.getLong(at + com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd.CMD_SEQ);
        UUID id = new UUID(b.getLong(at + Adopt.ENTITY_MOST), b.getLong(at + Adopt.ENTITY_LEAST));
        long agent = b.getLong(at + Adopt.AGENT);
        int epoch = b.getInt(at + Adopt.EPOCH);
        if (!DragonRecords.isDragonAgent(agent)) {
            reject(agent, epoch, cmdSeq, DragonRecords.SpawnRejected.BAD_RECORD, now);
            return;
        }
        if (byAgent.containsKey(agent)) {
            reject(agent, epoch, cmdSeq, DragonRecords.SpawnRejected.ALREADY_EMBODIED, now);
            return;
        }
        Entity e = find(id);
        if (e != null && RtsimAttachments.isTagged(e)) {
            reject(agent, epoch, cmdSeq, DragonRecords.SpawnRejected.ALREADY_EMBODIED, now);
            return;
        }
        if (!(e instanceof DragonBaseEntity d) || d.isRemoved() || IafDragons.dead(d) || d.isTame()
                || !(d.level() instanceof ServerLevel level)) {
            reject(agent, epoch, cmdSeq, DragonRecords.SpawnRejected.SPAWN_FAILED, now);
            return;
        }
        d.setData(RtsimAttachments.AGENT, new AgentTag(agent, epoch));
        d.setPersistenceRequired();
        d.setAgingDisabled(true);
        if ((b.getInt(at + Adopt.FLAGS) & Adopt.FLAG_HAS_HOME) != 0)
            IafDragons.setHome(d, level, new BlockPos(b.getInt(at + Adopt.HOME_X), b.getInt(at + Adopt.HOME_Y), b.getInt(at + Adopt.HOME_Z)));
        Puppet p = new Puppet(agent, epoch, d, now);
        register(p);
        journalMaterialized(p, level, now);
        adopted++;
    }

    /**
     * DRAGON_DISCARD: the entity goes, with no drops (its passengers stay); answered COMMAND_RESULT with
     * the command's sequence and op id 0. A body already gone is APPLIED (repeating a discard is harmless);
     * a tamed one, or one that became a puppet, is left alone and answered FAILED.
     */
    private void discardCommand(long cmdSeq, UUID id, long now) {
        Entity e = find(id);
        int status = Obs.CommandResult.APPLIED;
        if (e != null && !e.isRemoved()) {
            if (RtsimAttachments.isTagged(e) || (e instanceof DragonBaseEntity d && d.isTame())) {
                status = Obs.CommandResult.FAILED;
            } else {
                e.ejectPassengers();
                e.discard();
                discarded++;
            }
        }
        session.commandResult(cmdSeq, 0L, status, now);
    }

    private Entity find(UUID id) {
        for (ServerLevel l : session.server().getAllLevels()) {
            Entity e = l.getEntity(id);
            if (e != null)
                return e;
        }
        return null;
    }

    private void register(Puppet p) {
        byAgent.put(p.agent, p);
        byUuid.put(p.uuid, p);
    }

    private void unregister(Puppet p) {
        if (byAgent.get(p.agent) == p)
            byAgent.remove(p.agent);
        byUuid.remove(p.uuid, p);
    }

    /** The live puppet an entity is, or null. */
    Puppet live(Entity e) {
        Puppet p = byUuid.get(e.getUUID());
        return p != null && p.entity == e ? p : null;
    }

    /** The agent a live puppet embodies, or 0 (GameTests). */
    public long agentOf(Entity e) {
        Puppet p = live(e);
        return p == null ? 0L : p.agent;
    }

    /** The live puppet of an agent, or null (GameTests). */
    public Entity body(long agent) {
        Puppet p = byAgent.get(agent);
        return p == null ? null : p.entity;
    }

    public int embodied() {
        return byAgent.size();
    }

    public boolean isCorpse(UUID id) {
        return corpses.containsKey(id);
    }

    // ---- the tick -------------------------------------------------------------------------------

    @Override
    public void endOfTick(RtsimSession s, long now) {
        if (s != session || s.failed())
            return;
        lairs.drain(now);
        placer.tick(now);
        sampler.tick(now);
        lairs.scanChests(now);
        reportUntagged(now);
        endOfTickRecords(now);
        pass(now);
        lairs.flushRobberies(now);
    }

    /**
     * Wounds, meals, then deaths of this tick, in that order: a fatal blow is a wound too, and a meal must
     * reach the engine while the dragon is still embodied.
     */
    private void endOfTickRecords(long now) {
        flushHurts(now);
        settleKills(now);
        flushMeals(now);
        settleDeaths(now);
    }

    private void pass(long now) {
        if (byAgent.isEmpty())
            return;
        for (Puppet p : List.copyOf(byAgent.values())) {
            if (byAgent.get(p.agent) != p)
                continue;
            if (p.pendingDeath != null)
                continue; // settled next tick, or cancelled
            if (p.captureRequested != 0) {
                capture(p, p.captureRequested, now);
                continue;
            }
            Entity e = p.entity;
            if (e == null || e.isRemoved()) {
                capture(p, DragonRecords.Dematerialized.UNLOADED, now);
                continue;
            }
            if (!(e.level() instanceof ServerLevel level) || !RtsimSession.entitiesTickAt(level, e.blockPosition())) {
                capture(p, DragonRecords.Dematerialized.NOT_TICKING, now);
                continue;
            }
            DragonBaseEntity d = p.dragon();
            if (d == null || IafDragons.dead(d))
                continue;
            if (d.isTame()) {
                onTame(d, now); // tamed by a path that fired no AnimalTameEvent (another mod's tool)
                continue;
            }
            if (p.mode == Mode.DEPART && (now >= p.deadline || !inAnyPlayersRange(level, e))) {
                capture(p, DragonRecords.Dematerialized.COMMAND, now);
                continue;
            }
            if (p.oil == 0)
                IafDragons.withoutFire(d); // bite and claws (OG-13)
            steerTick(p, d);
            if (p.pendingBreaths > 0 && now - p.lastBreathFlush >= BREATH_INTERVAL)
                flushBreaths(p, now);
            if (now - p.lastState >= STATE_INTERVAL)
                state(p, d, level, now);
        }
    }

    // ---- fire (OILGATE.md §4.3, OG-4, OG-13) ----------------------------------------------------

    /**
     * Ice and Fire's DragonFireEvent (lowest priority, not cancelled so far): one breath of a puppet's
     * fire. A body carrying a reserve pays a point for it; at zero the fire is cancelled, so the dragon
     * fights with bite and claws. A body without one breathes freely and nothing is counted.
     */
    void onFire(DragonBaseEntity d, ICancellableEvent event) {
        Puppet p = live(d);
        if (p == null)
            return;
        long now = d.level().getGameTime();
        p.breath.events++;
        p.breath.saw(now, true, false);
        if (p.oil < 0)
            return;
        if (p.oil == 0) {
            event.setCanceled(true);
            p.breath.eventsCancelled++;
            fireCancelled++;
            IafDragons.withoutFire(d);
            return;
        }
        p.oil--;
        p.pendingBreaths++;
        p.breath.eventsPaid++;
        p.lastFireTick = now;
    }

    /**
     * A puppet's fire charge joining its level. The hover blast's charge comes out of a breath already
     * paid on this tick; the ground charge Ice and Fire shoots with no event is a breath of its own,
     * paid here, and stopped (the join cancelled) when the reserve is empty.
     */
    void onCharge(DragonBaseEntity d, ICancellableEvent event) {
        Puppet p = live(d);
        if (p == null)
            return;
        long now = d.level().getGameTime();
        boolean paid = p.lastFireTick == now;
        p.breath.saw(now, false, true);
        if (paid || p.oil < 0) {
            p.breath.chargesWithEvent += paid ? 1 : 0;
            return;
        }
        if (p.oil == 0) {
            event.setCanceled(true);
            p.breath.chargesStopped++;
            chargesStopped++;
            IafDragons.withoutFire(d);
            return;
        }
        p.oil--;
        p.pendingBreaths++;
        p.breath.chargesPaid++;
        p.lastFireTick = now;
    }

    /** DRAGON_BREATHS: a puppet's breaths since its last report. Before anything that ends its embodiment. */
    private void flushBreaths(Puppet p, long now) {
        p.lastBreathFlush = now;
        if (p.pendingBreaths <= 0)
            return;
        ByteBuffer r = session.boundary().beginJournaled(DragonRecords.DRAGON_BREATHS, DragonRecords.Breaths.SIZE, p.pendingBreaths, now);
        r.putLong(DragonRecords.Breaths.AGENT, p.agent);
        r.putInt(DragonRecords.Breaths.EPOCH, p.epoch);
        session.boundary().commit(r);
        p.breath.reported += p.pendingBreaths;
        breathsReported += p.pendingBreaths;
        breathRecords++;
        p.pendingBreaths = 0;
    }

    /** The reserve a live puppet has left (-1 when it breathes freely, or no such puppet). GameTests. */
    public int oilOf(long agent) {
        Puppet p = byAgent.get(agent);
        return p == null ? -1 : p.oil;
    }

    /** A live puppet's breaths not yet reported (GameTests). */
    public int pendingBreathsOf(long agent) {
        Puppet p = byAgent.get(agent);
        return p == null ? 0 : p.pendingBreaths;
    }

    /** A live puppet's breath log, or null. GameTests. */
    public Breath breathOf(long agent) {
        Puppet p = byAgent.get(agent);
        return p == null ? null : p.breath;
    }

    /**
     * Keeps a steered dragon on its target: Ice and Fire's flight manager replaces the target when the
     * dragon reaches it or wanders, so it is set again every tick until the dragon arrives. A live attack
     * target wins (Ice and Fire flies at it). Settling lands it on arrival.
     */
    private void steerTick(Puppet p, DragonBaseEntity d) {
        if (p.mode == Mode.IDLE || p.target == null)
            return;
        LivingEntity target = d.getTarget();
        if (target != null && target.isAlive())
            return;
        // The target's image nearest the dragon (TP11): Toroidal World folds a dragon that crosses an
        // edge back into the domain without an event, and it flies on through the seam.
        Vec3 goal = session.steerImage(d, p.target);
        double dx = goal.x - d.getX(), dz = goal.z - d.getZ();
        if (p.mode != Mode.DEPART && dx * dx + dz * dz <= ARRIVE * ARRIVE) {
            if (p.mode == Mode.SETTLE)
                IafDragons.land(d);
            p.mode = Mode.IDLE;
            return;
        }
        IafDragons.flyTo(d, goal);
    }

    /** True while any player in the level has the dragon's chunk within the simulation distance. */
    private boolean inAnyPlayersRange(ServerLevel level, Entity e) {
        int range = session.server().getPlayerList().getSimulationDistance();
        var topo = session.topology(level);
        for (ServerPlayer pl : level.players()) {
            if (pl.isSpectator())
                continue;
            // Through the seams (TP11): a player just across an edge has the dragon in range.
            double[] at = topo.nearestImage(pl.getX(), pl.getZ(), e.getX(), e.getZ(), true);
            int cx = net.minecraft.util.Mth.floor(at[0]) >> 4, cz = net.minecraft.util.Mth.floor(at[1]) >> 4;
            int px = pl.blockPosition().getX() >> 4, pz = pl.blockPosition().getZ() >> 4;
            if (Math.max(Math.abs(px - cx), Math.abs(pz - cz)) <= range)
                return true;
        }
        return false;
    }

    private void state(Puppet p, DragonBaseEntity d, ServerLevel level, long now) {
        int dim = session.dimId(level.dimension()); // before direct(): an id is defined before its first use
        int flags = 0;
        if (IafDragons.sleeping(d))
            flags |= DragonRecords.State.FLAG_SLEEPING;
        if (IafDragons.flying(d))
            flags |= DragonRecords.State.FLAG_FLYING;
        if (d.getTarget() != null && d.getTarget().isAlive())
            flags |= DragonRecords.State.FLAG_HAS_TARGET;
        flags |= DragonRecords.State.FLAG_TICKING; // the pass just checked
        int at = session.boundary().direct(DragonRecords.DRAGON_STATE, DragonRecords.State.SIZE, flags, now);
        if (at < 0)
            return; // dropped for space: sent again at the next pass
        ByteBuffer b = session.boundary().openBuffer();
        b.putLong(at + DragonRecords.State.AGENT, p.agent);
        b.putInt(at + DragonRecords.State.EPOCH, p.epoch);
        b.putInt(at + DragonRecords.State.DIM, dim);
        b.putFloat(at + DragonRecords.State.HEALTH, d.getHealth());
        b.putFloat(at + DragonRecords.State.MAX_HEALTH, d.getMaxHealth());
        b.putInt(at + DragonRecords.State.HUNGER, IafDragons.hunger(d));
        b.putInt(at + DragonRecords.State.AGE_TICKS, IafDragons.ageTicks(d));
        b.putDouble(at + DragonRecords.State.X, d.getX());
        b.putDouble(at + DragonRecords.State.Y, d.getY());
        b.putDouble(at + DragonRecords.State.Z, d.getZ());
        p.lastState = now;
        stateRecords++;
    }

    // ---- capture --------------------------------------------------------------------------------

    private static Snapshot snapshot(Entity e) {
        DragonBaseEntity d = e instanceof DragonBaseEntity x ? x : null;
        return new Snapshot(e.level().dimension(),
                d == null ? 0f : d.getHealth(), d == null ? 0f : d.getMaxHealth(),
                d == null ? 0 : IafDragons.hunger(d), d == null ? 0 : IafDragons.ageTicks(d),
                e.getX(), e.getY(), e.getZ());
    }

    /**
     * Captures a puppet and removes its body (DRAGON_DEMATERIALIZED with its health, hunger, age and
     * position). Prey it carries goes with it as one unit (C25): eaten, reported as DRAGON_ATE, and
     * removed; any other passenger (a player, another mod's puppet) is set down.
     */
    void capture(Puppet p, int reason, long now) {
        Entity e = p.entity;
        Snapshot s = p.precaptured != null ? p.precaptured : e == null ? null : snapshot(e);
        if (e != null && !e.isRemoved()) {
            for (Entity passenger : List.copyOf(e.getPassengers())) {
                passenger.stopRiding();
                if (isPrey(passenger) && !RtsimAttachments.isTagged(passenger) && passenger instanceof LivingEntity prey) {
                    ate(p, prey, prey.getX(), prey.getY(), prey.getZ());
                    prey.discard();
                }
            }
            p.removing = true;
            e.discard();
        }
        flushMeals(now); // what it ate, before the capture ends its embodiment
        flushBreaths(p, now); // and the fire it breathed
        int dim = s == null ? 0 : session.dimId(s.dimension());
        ByteBuffer r = session.boundary().beginJournaled(DragonRecords.DRAGON_DEMATERIALIZED, DragonRecords.Dematerialized.SIZE, reason, now);
        r.putLong(DragonRecords.Dematerialized.AGENT, p.agent);
        r.putInt(DragonRecords.Dematerialized.EPOCH, p.epoch);
        r.putInt(DragonRecords.Dematerialized.DIM, dim);
        if (s != null) {
            r.putFloat(DragonRecords.Dematerialized.HEALTH, s.health());
            r.putFloat(DragonRecords.Dematerialized.MAX_HEALTH, s.maxHealth());
            r.putInt(DragonRecords.Dematerialized.HUNGER, s.hunger());
            r.putInt(DragonRecords.Dematerialized.AGE_TICKS, s.ageTicks());
            r.putDouble(DragonRecords.Dematerialized.X, s.x());
            r.putDouble(DragonRecords.Dematerialized.Y, s.y());
            r.putDouble(DragonRecords.Dematerialized.Z, s.z());
        }
        session.boundary().commit(r);
        unregister(p);
        captured++;
    }

    // ---- wounds, deaths, meals ------------------------------------------------------------------

    static int attackerKind(Entity attacker) {
        if (attacker == null)
            return DragonRecords.ATTACKER_NONE;
        if (attacker instanceof Player)
            return DragonRecords.ATTACKER_PLAYER;
        if (attacker instanceof net.minecraft.world.entity.Mob)
            return DragonRecords.ATTACKER_MOB;
        return DragonRecords.ATTACKER_OTHER;
    }

    /** LivingDamageEvent.Post on a puppet: summed per attacker over the tick. */
    void onDamage(LivingEntity victim, DamageSource source, float amount) {
        if (amount <= 0)
            return;
        Puppet p = live(victim);
        if (p == null)
            return;
        Entity attacker = source.getEntity();
        HurtSum h = hurts.computeIfAbsent(new HurtKey(p.agent, attacker == null ? null : attacker.getUUID()), k -> new HurtSum());
        h.epoch = p.epoch;
        h.kind = attackerKind(attacker);
        h.hits++;
        h.amount += amount;
    }

    private void flushHurts(long now) {
        if (hurts.isEmpty())
            return;
        for (Map.Entry<HurtKey, HurtSum> en : hurts.entrySet()) {
            HurtKey k = en.getKey();
            HurtSum h = en.getValue();
            Puppet p = byAgent.get(k.agent());
            float after = p != null && p.dragon() != null ? Math.max(0f, p.dragon().getHealth()) : 0f;
            ByteBuffer r = session.boundary().beginJournaled(DragonRecords.DRAGON_HURT, DragonRecords.Hurt.SIZE, h.kind, now);
            r.putLong(DragonRecords.Hurt.AGENT, k.agent());
            r.putInt(DragonRecords.Hurt.EPOCH, h.epoch);
            r.putInt(DragonRecords.Hurt.HITS, h.hits);
            r.putFloat(DragonRecords.Hurt.AMOUNT, h.amount);
            r.putFloat(DragonRecords.Hurt.HEALTH_AFTER, after);
            if (k.attacker() != null) {
                r.putLong(DragonRecords.Hurt.ATTACKER_MOST, k.attacker().getMostSignificantBits());
                r.putLong(DragonRecords.Hurt.ATTACKER_LEAST, k.attacker().getLeastSignificantBits());
            }
            session.boundary().commit(r);
            hurtRecords++;
        }
        hurts.clear();
    }

    /**
     * LivingDeathEvent (lowest priority, not cancelled so far): a puppet's death is journaled at the end
     * of the tick once it is certain (docs/boundary.md §2.3); a puppet's kill of prey is a meal.
     */
    void onDeath(LivingEntity dead, DamageSource source) {
        Puppet p = live(dead);
        Entity killer = source.getEntity();
        if (p != null) {
            p.pendingDeath = new Death(attackerKind(killer), killer == null ? null : killer.getUUID());
            return;
        }
        Puppet by = killer == null ? null : live(killer);
        if (by != null && isPrey(dead))
            kills.add(new Kill(by, dead, dead.getX(), dead.getY(), dead.getZ()));
    }

    private void settleDeaths(long now) {
        for (Puppet p : List.copyOf(byAgent.values())) {
            if (p.pendingDeath == null)
                continue;
            DragonBaseEntity d = p.dragon();
            boolean dead = d == null || IafDragons.dead(d);
            if (!dead) {
                p.pendingDeath = null; // another mod cancelled it
                continue;
            }
            Entity e = p.entity;
            flushBreaths(p, now);
            ResourceKey<Level> dimKey = e.level().dimension();
            int dim = session.dimId(dimKey);
            ByteBuffer r = session.boundary().beginJournaled(DragonRecords.DRAGON_DIED, DragonRecords.Died.SIZE, p.pendingDeath.killerKind(), now);
            r.putLong(DragonRecords.Died.AGENT, p.agent);
            r.putInt(DragonRecords.Died.EPOCH, p.epoch);
            r.putInt(DragonRecords.Died.DIM, dim);
            if (p.pendingDeath.killer() != null) {
                r.putLong(DragonRecords.Died.KILLER_MOST, p.pendingDeath.killer().getMostSignificantBits());
                r.putLong(DragonRecords.Died.KILLER_LEAST, p.pendingDeath.killer().getLeastSignificantBits());
            }
            r.putDouble(DragonRecords.Died.X, e.getX());
            r.putDouble(DragonRecords.Died.Y, e.getY());
            r.putDouble(DragonRecords.Died.Z, e.getZ());
            long seq = session.boundary().commit(r);
            unregister(p);
            died++;
            if (!e.isRemoved()) {
                // Ice and Fire keeps the body as a corpse to harvest. It stays tagged (never saved) until
                // the engine's saved pages hold the death, then becomes an ordinary entity (choice Z6).
                UUID id = p.uuid;
                corpses.put(id, new Corpse(p.agent, p.epoch, dimKey));
                session.afterAck(seq, () -> untagCorpse(id));
            }
        }
    }

    private void untagCorpse(UUID id) {
        Corpse c = corpses.remove(id);
        if (c == null || session == null)
            return;
        ServerLevel level = session.server().getLevel(c.dimension());
        Entity e = level == null ? null : level.getEntity(id);
        if (e != null)
            e.removeData(RtsimAttachments.AGENT);
    }

    static boolean isPrey(Entity e) {
        if (!(e instanceof LivingEntity) || e instanceof Player || IafDragons.isDragon(e))
            return false;
        return e.getType().is(IafDragons.DRAGON_TARGETS) || e instanceof Animal;
    }

    /** EntityMountEvent: a puppet picked up prey; its meal counts against the village it was taken from. */
    void onMount(Entity mounting, Entity mount) {
        if (live(mount) != null && isPrey(mounting))
            pickedUpFrom.put(mounting.getUUID(), villageAt(mounting));
    }

    private static long villageAt(Entity e) {
        Village v = RtsimApi.villageOf(e);
        return v == null ? 0L : v.id;
    }

    private void settleKills(long now) {
        if (kills.isEmpty())
            return;
        for (Kill k : kills)
            if (k.victim().isRemoved() || k.victim().isDeadOrDying())
                ate(k.by(), k.victim(), k.x(), k.y(), k.z());
        kills.clear();
    }

    private void ate(Puppet by, LivingEntity prey, double x, double y, double z) {
        Long from = pickedUpFrom.remove(prey.getUUID());
        long village = from != null ? from : villageAt(prey);
        ResourceLocation type = BuiltInRegistries.ENTITY_TYPE.getKey(prey.getType());
        AteSum a = meals.computeIfAbsent(new AteKey(by.agent, by.epoch, type.toString(), village), k -> new AteSum());
        a.count++;
        a.x = x;
        a.y = y;
        a.z = z;
    }

    private void flushMeals(long now) {
        if (meals.isEmpty())
            return;
        for (Map.Entry<AteKey, AteSum> en : meals.entrySet()) {
            AteKey k = en.getKey();
            AteSum a = en.getValue();
            int prey = session.dictId(Obs.DICT_BODY, k.prey());
            ByteBuffer r = session.boundary().beginJournaled(DragonRecords.DRAGON_ATE, DragonRecords.Ate.SIZE, a.count, now);
            r.putLong(DragonRecords.Ate.AGENT, k.agent());
            r.putInt(DragonRecords.Ate.EPOCH, k.epoch());
            r.putInt(DragonRecords.Ate.PREY, prey);
            r.putLong(DragonRecords.Ate.VILLAGE, k.village());
            r.putDouble(DragonRecords.Ate.X, a.x);
            r.putDouble(DragonRecords.Ate.Y, a.y);
            r.putDouble(DragonRecords.Ate.Z, a.z);
            session.boundary().commit(r);
            ateRecords++;
        }
        meals.clear();
    }

    // ---- untagged dragons (P13: the sim owns every dragon, docs/boundary.md §6.6) ---------------

    /** EntityJoinLevelEvent for a dragon without the tag: reported at the end of the tick. */
    void onUntaggedJoin(Entity e) {
        joined.add(e);
    }

    /** The NeoForge spawn type a dragon's FinalizeSpawnEvent carried (null when none fired). */
    static int spawnKind(MobSpawnType type, boolean inLair) {
        if (type == null)
            return inLair ? DragonRecords.Untagged.SPAWN_STRUCTURE : DragonRecords.Untagged.SPAWN_OTHER;
        return switch (type) {
            case STRUCTURE -> DragonRecords.Untagged.SPAWN_STRUCTURE;
            case NATURAL, CHUNK_GENERATION, PATROL, EVENT -> DragonRecords.Untagged.SPAWN_NATURAL;
            case SPAWNER, TRIAL_SPAWNER -> DragonRecords.Untagged.SPAWN_SPAWNER;
            case SPAWN_EGG, DISPENSER -> DragonRecords.Untagged.SPAWN_EGG;
            case COMMAND -> DragonRecords.Untagged.SPAWN_COMMAND;
            case BREEDING -> DragonRecords.Untagged.SPAWN_HATCHED;
            default -> DragonRecords.Untagged.SPAWN_OTHER;
        };
    }

    private void reportUntagged(long now) {
        while (!joined.isEmpty()) {
            Entity e = joined.poll();
            if (e.isRemoved() || RtsimAttachments.isTagged(e) || !(e instanceof DragonBaseEntity d) || !(e.level() instanceof ServerLevel level)
                    || level.getServer() != session.server())
                continue;
            if (IafDragons.dead(d) || !RtsimConfig.on(Module.ECOLOGY))
                continue;
            if (!untaggedReported.add(e.getUUID()))
                continue; // once per entity per session (choice Z9)
            int species = IafDragons.species(d);
            if (species < 0)
                continue;
            Lairs.Lair lair = lairs.at(level, e.blockPosition(), now);
            MobSpawnType type = e.getExistingDataOrNull(MicRtsimIceAndFire.SPAWN_TYPE.get());
            int kind = spawnKind(type, lair != null);
            int flags = 0;
            if (d.isTame())
                flags |= DragonRecords.Untagged.FLAG_TAMED;
            BlockPos home = IafDragons.home(d);
            if (home != null)
                flags |= DragonRecords.Untagged.FLAG_HAS_HOME;
            if (IafDragons.sleeping(d))
                flags |= DragonRecords.Untagged.FLAG_SLEEPING;
            if (IafDragons.agingDisabled(d))
                flags |= DragonRecords.Untagged.FLAG_AGING_DISABLED;
            int speciesId = session.dictId(Obs.DICT_BODY, IafDragons.SPECIES.get(species));
            int dim = session.dimId(level.dimension());
            ByteBuffer r = session.boundary().beginJournaled(DragonRecords.DRAGON_UNTAGGED, DragonRecords.Untagged.SIZE, kind, now);
            r.putLong(DragonRecords.Untagged.ENTITY_MOST, e.getUUID().getMostSignificantBits());
            r.putLong(DragonRecords.Untagged.ENTITY_LEAST, e.getUUID().getLeastSignificantBits());
            r.putInt(DragonRecords.Untagged.SPECIES, speciesId);
            r.putInt(DragonRecords.Untagged.DIM, dim);
            r.putInt(DragonRecords.Untagged.FLAGS, flags);
            r.putInt(DragonRecords.Untagged.AGE_TICKS, IafDragons.ageTicks(d));
            r.putInt(DragonRecords.Untagged.HUNGER, IafDragons.hunger(d));
            r.putInt(DragonRecords.Untagged.GENDER, IafDragons.male(d) ? 1 : 0);
            r.putInt(DragonRecords.Untagged.VARIANT, IafDragons.variant(d));
            if (home != null) {
                r.putInt(DragonRecords.Untagged.HOME_X, home.getX());
                r.putInt(DragonRecords.Untagged.HOME_Y, home.getY());
                r.putInt(DragonRecords.Untagged.HOME_Z, home.getZ());
            }
            r.putLong(DragonRecords.Untagged.LAIR, lair == null ? 0L : lair.id());
            r.putDouble(DragonRecords.Untagged.X, e.getX());
            r.putDouble(DragonRecords.Untagged.Y, e.getY());
            r.putDouble(DragonRecords.Untagged.Z, e.getZ());
            session.boundary().commit(r);
            untagged++;
        }
    }

    // ---- taming (A10) ---------------------------------------------------------------------------

    /**
     * A puppet a player tames leaves the sim (A10). The dragon records have no TAKEN, so it leaves as a
     * death with no killer: the engine's dragon is gone (no grudge), the body is the player's (choice Z7).
     */
    void onTame(Entity e, long now) {
        Puppet p = live(e);
        if (p == null)
            return;
        p.pendingDeath = null;
        flushBreaths(p, now);
        int dim = session.dimId(e.level().dimension());
        ByteBuffer r = session.boundary().beginJournaled(DragonRecords.DRAGON_DIED, DragonRecords.Died.SIZE, DragonRecords.ATTACKER_NONE, now);
        r.putLong(DragonRecords.Died.AGENT, p.agent);
        r.putInt(DragonRecords.Died.EPOCH, p.epoch);
        r.putInt(DragonRecords.Died.DIM, dim);
        r.putDouble(DragonRecords.Died.X, e.getX());
        r.putDouble(DragonRecords.Died.Y, e.getY());
        r.putDouble(DragonRecords.Died.Z, e.getZ());
        session.boundary().commit(r);
        e.removeData(RtsimAttachments.AGENT);
        unregister(p);
    }

    MinecraftServer server() {
        return session.server();
    }
}
