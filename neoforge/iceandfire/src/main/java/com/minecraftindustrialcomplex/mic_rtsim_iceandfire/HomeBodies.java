package com.minecraftindustrialcomplex.mic_rtsim_iceandfire;

import com.iafenvoy.iceandfire.entity.DragonBaseEntity;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.AgentTag;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.sites.ChunkWatch;
import com.minecraftindustrialcomplex.mic_rtsim.sites.VanillaChunks;
import com.mojang.serialization.Codec;
import com.mojang.serialization.codecs.RecordCodecBuilder;
import net.minecraft.core.BlockPos;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.MobSpawnType;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.phys.AABB;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentLinkedQueue;

/**
 * Dragon home bodies (EQUILIBRIUM.md slice S5, EQ-2 (a); docs/overnight-choices-equilibrium.md EQC100 to EQC149): a
 * dragon roosting at its lair is a vanilla body that vanilla saves, and a puppet when it is away. A <b>home body</b>
 * carries this plugin's {@link HomeTag} attachment, serialised with the entity (the engine's {@code Dragon.body} is
 * the entity it believes stands); it carries no {@code mic_rtsim:agent} tag, so vanilla saves it. The plugin swaps
 * the two tags in place on one entity, never replacing it: home to puppet at the leash ({@link #leashScan}), puppet to
 * home at a hand-back ({@link #handBack}).
 *
 * <p><b>The lair census.</b> A lair is <i>in</i> while every chunk within {@link DragonConfig#lairFootprint} blocks of
 * its home is loaded and ticking ({@link ChunkWatch}, core). When it comes in the plugin counts what stands in the
 * footprint (a home body of this lair, or an untagged structure dragon in the lair's box) and sends LAIR_CENSUS; the
 * engine answers with ADOPT, DISCARD and MATERIALIZE lines, and the lair is recounted each game day while it stays in.
 * When it goes out each loaded home body is released with DRAGON_DEMATERIALIZED reason 8 (it stays in the world and
 * vanilla saves it), then LAIR_OUT. Only lairs whose home chunk is loaded are followed (a few dozen, not the
 * thousands a session finds).
 *
 * <p>Server thread, except {@link #chunkLoaded} and {@link #chunkUnloaded}.
 */
public final class HomeBodies {
    /** {@code mic_rtsim_iceandfire:home}'s payload: the lair a body is home at and the agent it embodies (EQC105). */
    public record HomeTag(long lair, long agent) {
        public static final Codec<HomeTag> CODEC = RecordCodecBuilder.create(i -> i.group(
                Codec.LONG.fieldOf("lair").forGetter(HomeTag::lair),
                Codec.LONG.fieldOf("agent").forGetter(HomeTag::agent)
        ).apply(i, HomeTag::new));
    }

    private record ChunkNote(boolean loaded, ResourceKey<Level> dimension, long chunk) {}

    private final DragonBridge bridge;
    private ChunkWatch watch;
    private final Map<Long, Lairs.Lair> active = new HashMap<>();
    /** Lairs whose home block lies in a chunk, by dimension then chunk: the ones to follow when it loads. */
    private final Map<ResourceKey<Level>, Map<Long, List<Lairs.Lair>>> byHomeChunk = new HashMap<>();
    /** The epoch of each lair that is in, and the tick of its last census. */
    private final Map<Long, Long> epochs = new HashMap<>();
    private final Map<Long, Long> censusAt = new HashMap<>();
    private final ConcurrentLinkedQueue<ChunkNote> chunkEvents = new ConcurrentLinkedQueue<>();
    private long epochSeq;
    private long lastLeash;
    private boolean wasOn = true;

    // Counters for logs and the GameTests (this session).
    public long censuses, outs, leashed, handBacks;

    HomeBodies(DragonBridge bridge) {
        this.bridge = bridge;
    }

    void clear() {
        watch = null;
        active.clear();
        byHomeChunk.clear();
        epochs.clear();
        censusAt.clear();
        chunkEvents.clear();
        epochSeq = 0;
        lastLeash = 0;
        wasOn = true;
        censuses = outs = leashed = handBacks = 0;
    }

    private ChunkWatch watch() {
        if (watch == null) {
            RtsimSession s = bridge.session();
            watch = new ChunkWatch(VanillaChunks.source(k -> s.server().getLevel(k)), new ChunkWatch.Listener() {
                @Override
                public long in(ChunkWatch.Area area, long now) {
                    return enter((Lairs.Lair) area.tag, now, false, false);
                }

                @Override
                public void out(ChunkWatch.Area area, long now) {
                    leave((Lairs.Lair) area.tag, now);
                }
            }, DragonConfig.censusGap());
        }
        return watch;
    }

    // ---- following lairs -------------------------------------------------------------------------

    /** A lair was reported (LAIR_FOUND): followed from now when its home chunk is loaded, and when it next loads. */
    void lairKnown(Lairs.Lair lair) {
        long chunk = ChunkPos.asLong(lair.home().getX() >> 4, lair.home().getZ() >> 4);
        List<Lairs.Lair> l = byHomeChunk.computeIfAbsent(lair.level().dimension(), k -> new HashMap<>()).computeIfAbsent(chunk, k -> new ArrayList<>(1));
        if (!l.contains(lair))
            l.add(lair);
        if (lair.level().hasChunk(lair.home().getX() >> 4, lair.home().getZ() >> 4))
            activate(lair);
    }

    private void activate(Lairs.Lair lair) {
        if (!DragonConfig.watchLairs() || active.containsKey(lair.id()) || !RtsimConfig.on(Module.ECOLOGY))
            return;
        active.put(lair.id(), lair);
        ChunkWatch.Area a = watch().declare(lair.id(), lair.level().dimension(), lair.home().getX() + 0.5, lair.home().getZ() + 0.5,
                DragonConfig.lairFootprint());
        a.tag = lair;
    }

    /** ChunkEvent.Load or Unload, possibly off the server thread: applied at the end of the tick. */
    void chunkLoaded(ServerLevel level, long chunk) {
        chunkEvents.add(new ChunkNote(true, level.dimension(), chunk));
    }

    void chunkUnloaded(ServerLevel level, long chunk) {
        chunkEvents.add(new ChunkNote(false, level.dimension(), chunk));
    }

    /** ChunkTicketLevelUpdatedEvent: an early hint only (C2ME does not post it); the poll is the authority. */
    void onTicket(ServerLevel level, long chunk, int newLevel) {
        if (watch != null)
            watch.onTicket(level.dimension(), chunk, VanillaChunks.norm(newLevel));
    }

    private void drainChunks(long now) {
        for (ChunkNote e; (e = chunkEvents.poll()) != null; ) {
            Map<Long, List<Lairs.Lair>> m = byHomeChunk.get(e.dimension());
            List<Lairs.Lair> l = m == null ? null : m.get(e.chunk());
            if (l == null)
                continue;
            for (Lairs.Lair lair : l) {
                if (e.loaded()) {
                    activate(lair);
                } else if (active.remove(lair.id()) != null && watch != null) {
                    watch.remove(lair.id(), now);
                }
            }
        }
    }

    // ---- the tick --------------------------------------------------------------------------------

    void endOfTick(long now) {
        if (!RtsimConfig.on(Module.ECOLOGY)) {
            wasOn = false;
            return;
        }
        if (!wasOn && watch != null) {
            watch.reset(); // switched on again: every lair comes in anew
            epochs.clear();
        }
        wasOn = true;
        drainChunks(now);
        if (watch != null)
            watch.tick(now);
        if (now - lastLeash >= DragonConfig.leashScan()) {
            lastLeash = now;
            leashScan(now);
            recount(now);
        }
    }

    /** A lair that stays in is recounted each game day (EQC128). */
    private void recount(long now) {
        if (watch == null)
            return;
        for (Lairs.Lair lair : active.values()) {
            ChunkWatch.Area a = watch.get(lair.id());
            if (a != null && a.in() && now - censusAt.getOrDefault(lair.id(), now) >= DragonConfig.censusPeriod())
                watch.recount(a);
        }
    }

    // ---- in and out ------------------------------------------------------------------------------

    /**
     * The lair came in: LAIR_CENSUS. Returns the epoch (a counter per session; the engine voids its lines at its own
     * session start, so a counter that starts over is safe).
     */
    long enter(Lairs.Lair lair, long now, boolean forceUnwatched, boolean forceWatched) {
        RtsimSession s = bridge.session();
        long epoch = ++epochSeq;
        epochs.put(lair.id(), epoch);
        censusAt.put(lair.id(), now);
        List<DragonBaseEntity> bodies = standing(lair);
        boolean watched = forceWatched || (!forceUnwatched && watched(lair));
        int n = bodies.size();
        int dim = s.dimId(lair.level().dimension());
        int[] species = new int[n];
        for (int i = 0; i < n; i++)
            species[i] = s.dictId(Obs.DICT_BODY, IafDragons.SPECIES.get(IafDragons.species(bodies.get(i)))); // before the record
        ByteBuffer r = s.boundary().beginJournaled(DragonRecords.LAIR_CENSUS, DragonRecords.LairCensus.size(n), n, now);
        r.putLong(DragonRecords.LairCensus.LAIR, lair.id());
        r.putLong(DragonRecords.LairCensus.EPOCH, epoch);
        r.putInt(DragonRecords.LairCensus.DIM, dim);
        r.putInt(DragonRecords.LairCensus.FLAGS, watched ? DragonRecords.LairCensus.FLAG_WATCHED : 0);
        r.putInt(DragonRecords.LairCensus.N, n);
        for (int i = 0; i < n; i++) {
            DragonBaseEntity d = bodies.get(i);
            int at = DragonRecords.LairCensus.BODIES + DragonRecords.LairCensus.BODY * i;
            HomeTag tag = d.getExistingDataOrNull(MicRtsimIceAndFire.HOME);
            r.putLong(at + DragonRecords.LairCensus.B_ENTITY_MOST, d.getUUID().getMostSignificantBits());
            r.putLong(at + DragonRecords.LairCensus.B_ENTITY_LEAST, d.getUUID().getLeastSignificantBits());
            r.putLong(at + DragonRecords.LairCensus.B_AGENT, tag == null ? 0L : tag.agent());
            r.putInt(at + DragonRecords.LairCensus.B_SPECIES, species[i]);
            int flags = (IafDragons.sleeping(d) ? DragonRecords.LairCensus.BODY_SLEEPING : 0)
                    | (IafDragons.flying(d) ? DragonRecords.LairCensus.BODY_FLYING : 0)
                    | (IafDragons.home(d) != null ? DragonRecords.LairCensus.BODY_HAS_HOME : 0);
            r.putInt(at + DragonRecords.LairCensus.B_FLAGS, flags);
            r.putInt(at + DragonRecords.LairCensus.B_AGE_TICKS, IafDragons.ageTicks(d));
            r.putInt(at + DragonRecords.LairCensus.B_HUNGER, IafDragons.hunger(d));
            r.putFloat(at + DragonRecords.LairCensus.B_HEALTH, d.getHealth());
            r.putInt(at + DragonRecords.LairCensus.B_GENDER, IafDragons.male(d) ? 1 : 0);
            r.putInt(at + DragonRecords.LairCensus.B_VARIANT, IafDragons.variant(d));
            r.putDouble(at + DragonRecords.LairCensus.B_X, d.getX());
            r.putDouble(at + DragonRecords.LairCensus.B_Y, d.getY());
            r.putDouble(at + DragonRecords.LairCensus.B_Z, d.getZ());
        }
        s.boundary().commit(r);
        censuses++;
        return epoch;
    }

    /** The lair went out: each loaded home body of it is released (reason 8), then LAIR_OUT. */
    void leave(Lairs.Lair lair, long now) {
        Long epoch = epochs.remove(lair.id());
        if (epoch == null)
            return;
        RtsimSession s = bridge.session();
        for (DragonBridge.Puppet p : bridge.homePuppetsOf(lair.id()))
            bridge.capture(p, DragonRecords.Dematerialized.HOME_SAVED, now);
        ByteBuffer r = s.boundary().beginJournaled(DragonRecords.LAIR_OUT, DragonRecords.LairOut.SIZE, 0, now);
        r.putLong(DragonRecords.LairOut.LAIR, lair.id());
        r.putLong(DragonRecords.LairOut.EPOCH, epoch);
        s.boundary().commit(r);
        outs++;
    }

    /** GameTests: the lair comes in now (a census), unwatched or watched, whatever its chunks say. */
    public long testIn(long lairId, boolean watched, long now) {
        Lairs.Lair lair = bridge.lairs.known(lairId);
        if (lair == null)
            throw new IllegalStateException("no lair " + lairId);
        if (epochs.containsKey(lairId))
            leave(lair, now);
        return enter(lair, now, !watched, watched);
    }

    /** GameTests: the lair goes out now. */
    public void testOut(long lairId, long now) {
        Lairs.Lair lair = bridge.lairs.known(lairId);
        if (lair != null)
            leave(lair, now);
    }

    /** The epoch of the lair's stay in, or 0 (GameTests). */
    public long epochOf(long lairId) {
        return epochs.getOrDefault(lairId, 0L);
    }

    /** Whether the lair is in (GameTests, logs). */
    public boolean isIn(long lairId) {
        return epochs.containsKey(lairId);
    }

    // ---- the census's bodies ---------------------------------------------------------------------

    /** The dragons standing in a lair's footprint that the census lists: its home bodies and its box's untagged structure dragons. */
    List<DragonBaseEntity> standing(Lairs.Lair lair) {
        ServerLevel level = lair.level();
        double r = DragonConfig.lairFootprint();
        BlockPos h = lair.home();
        AABB box = new AABB(h.getX() - r, level.getMinBuildHeight(), h.getZ() - r, h.getX() + r, level.getMaxBuildHeight(), h.getZ() + r);
        List<DragonBaseEntity> out = new ArrayList<>();
        for (DragonBaseEntity d : level.getEntitiesOfClass(DragonBaseEntity.class, box)) {
            if (d.isRemoved() || IafDragons.dead(d) || d.isTame() || RtsimAttachments.isTagged(d) || IafDragons.species(d) < 0)
                continue;
            HomeTag tag = d.getExistingDataOrNull(MicRtsimIceAndFire.HOME);
            if (tag != null && tag.agent() != 0) {
                if (tag.lair() == lair.id())
                    out.add(d);
                continue;
            }
            if (inBox(lair, d) && structureSpawn(d))
                out.add(d);
        }
        out.sort(Comparator.comparingDouble((DragonBaseEntity d) -> d.distanceToSqr(h.getX() + 0.5, h.getY(), h.getZ() + 0.5)));
        return out.size() > DragonRecords.LairCensus.MAX_BODIES ? new ArrayList<>(out.subList(0, DragonRecords.LairCensus.MAX_BODIES)) : out;
    }

    /** Whether a dragon stands in the lair's box, with some room above and below (its box is 4 tall for a roost). */
    static boolean inBox(Lairs.Lair lair, Entity e) {
        BoundingBox b = lair.box();
        return e.getX() >= b.minX() && e.getX() <= b.maxX() + 1 && e.getZ() >= b.minZ() && e.getZ() <= b.maxZ() + 1
                && e.getY() >= b.minY() - 8 && e.getY() <= b.maxY() + 24;
    }

    /** A dragon a structure placed (or one with no spawn type recorded, as every one read from disk): the lair's own. */
    static boolean structureSpawn(Entity e) {
        MobSpawnType type = e.getExistingDataOrNull(MicRtsimIceAndFire.SPAWN_TYPE);
        return DragonBridge.spawnKind(type, true) == DragonRecords.Untagged.SPAWN_STRUCTURE;
    }

    /** Any non-spectator player within the larger of {@code watchBlocks} and the distance a client tracks an entity at. */
    boolean watched(Lairs.Lair lair) {
        RtsimSession s = bridge.session();
        int view = s.server().getPlayerList().getViewDistance() * 16;
        double range = Math.max(DragonConfig.watchBlocks(), view);
        var topo = s.topology(lair.level());
        BlockPos h = lair.home();
        for (ServerPlayer p : lair.level().players()) {
            if (p.isSpectator())
                continue;
            double[] at = topo.nearestImage(h.getX() + 0.5, h.getZ() + 0.5, p.getX(), p.getZ(), true);
            double dx = at[0] - p.getX(), dz = at[1] - p.getZ();
            if (dx * dx + dz * dz <= range * range)
                return true;
        }
        return false;
    }

    // ---- bodies joining and leaving --------------------------------------------------------------

    /**
     * A body with the home attachment joined a level (read from disk, or swapped across a dimension): a corpse or a
     * tame loses the attachment quietly; a registered body whose old copy left by a dimension change takes the new
     * copy; one the engine does not hold asks its lair, if it is in, for a new census.
     */
    void joined(DragonBaseEntity d) {
        HomeTag tag = d.getExistingDataOrNull(MicRtsimIceAndFire.HOME);
        if (tag == null)
            return;
        if (IafDragons.dead(d) || d.isTame()) {
            d.removeData(MicRtsimIceAndFire.HOME);
            return;
        }
        d.setPersistenceRequired();
        DragonBridge.Puppet p = bridge.puppetOfEntity(d.getUUID());
        if (p != null && p.home) {
            if (p.entity != d && (p.entity == null || p.entity.isRemoved()))
                p.entity = d;
            return;
        }
        if (watch != null && epochs.containsKey(tag.lair())) {
            ChunkWatch.Area a = watch.get(tag.lair());
            if (a != null)
                watch.recount(a);
        }
    }

    /**
     * A home body left its level (EntityLeaveLevelEvent): killed (a death to settle), changed dimension (the copy joins
     * there), or removed otherwise: saved by vanilla with its chunk (reason 8), or taken by something else (a capture).
     */
    void left(Entity e) {
        DragonBridge.Puppet p = bridge.puppetOfEntity(e.getUUID());
        if (p == null || !p.home || p.entity != e || p.removing)
            return;
        Entity.RemovalReason reason = e.getRemovalReason();
        if (reason == null || reason == Entity.RemovalReason.CHANGED_DIMENSION)
            return;
        if (reason == Entity.RemovalReason.KILLED) {
            if (p.pendingDeath == null)
                p.pendingDeath = new DragonBridge.Death(DragonRecords.ATTACKER_NONE, null);
            return;
        }
        if (p.precaptured == null)
            p.precaptured = DragonBridge.snapshot(e);
        if (p.captureRequested == 0)
            p.captureRequested = reason.shouldSave() ? DragonRecords.Dematerialized.HOME_SAVED : DragonRecords.Dematerialized.UNLOADED;
    }

    // ---- the leash and the swaps -----------------------------------------------------------------

    /** Every look at the home bodies: one past the leash, or in another dimension, is a puppet where it stands (EQC109). */
    private void leashScan(long now) {
        for (DragonBridge.Puppet p : bridge.homePuppets()) {
            Entity e = p.entity;
            if (e == null || e.isRemoved() || p.pendingDeath != null || p.captureRequested != 0)
                continue;
            Lairs.Lair lair = bridge.lairs.known(p.lair);
            boolean away = lair == null || e.level() != lair.level();
            if (!away) {
                var topo = bridge.session().topology(lair.level());
                double[] at = topo.nearestImage(lair.home().getX() + 0.5, lair.home().getZ() + 0.5, e.getX(), e.getZ(), true);
                double dx = at[0] - e.getX(), dz = at[1] - e.getZ();
                double leash = DragonConfig.homeLeash();
                away = dx * dx + dz * dz > leash * leash;
            }
            if (away)
                leashSwap(p, now);
        }
    }

    /** The home body becomes a puppet in place: DRAGON_LEFT_HOME first, then the tags. */
    void leashSwap(DragonBridge.Puppet p, long now) {
        Entity e = p.entity;
        RtsimSession s = bridge.session();
        int dim = s.dimId(e.level().dimension());
        ByteBuffer r = s.boundary().beginJournaled(DragonRecords.DRAGON_LEFT_HOME, DragonRecords.LeftHome.SIZE, 0, now);
        r.putLong(DragonRecords.LeftHome.AGENT, p.agent);
        r.putInt(DragonRecords.LeftHome.EPOCH, p.epoch);
        r.putInt(DragonRecords.LeftHome.DIM, dim);
        r.putLong(DragonRecords.LeftHome.ENTITY_MOST, p.uuid.getMostSignificantBits());
        r.putLong(DragonRecords.LeftHome.ENTITY_LEAST, p.uuid.getLeastSignificantBits());
        r.putDouble(DragonRecords.LeftHome.X, e.getX());
        r.putDouble(DragonRecords.LeftHome.Y, e.getY());
        r.putDouble(DragonRecords.LeftHome.Z, e.getZ());
        s.boundary().commit(r);
        e.removeData(MicRtsimIceAndFire.HOME);
        e.setData(RtsimAttachments.AGENT, new AgentTag(p.agent, p.epoch));
        p.home = false;
        p.lair = 0L;
        leashed++;
    }

    /** DRAGON_HAND_BACK: a settled puppet at its lair becomes a home body, in place (EQC110). */
    void handBack(long agent, int epoch, long now) {
        DragonBridge.Puppet p = bridge.puppetOf(agent);
        if (p == null || p.epoch != epoch || p.home || !(p.entity instanceof DragonBaseEntity d) || d.isRemoved()
                || IafDragons.dead(d) || d.isTame() || !(d.level() instanceof ServerLevel level))
            return;
        Lairs.Lair lair = bridge.lairs.nearestKnown(level, d.position(), 64.0);
        if (lair == null)
            return;
        d.removeData(RtsimAttachments.AGENT);
        d.setData(MicRtsimIceAndFire.HOME, new HomeTag(lair.id(), agent));
        d.setPersistenceRequired();
        d.setAgingDisabled(true);
        p.home = true;
        p.lair = lair.id();
        p.mode = DragonBridge.Mode.IDLE;
        p.target = null;
        bridge.journalMaterialized(p, level, now);
        handBacks++;
    }
}
