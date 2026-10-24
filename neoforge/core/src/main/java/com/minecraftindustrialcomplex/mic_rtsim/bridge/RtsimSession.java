package com.minecraftindustrialcomplex.mic_rtsim.bridge;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.body.BodyAdapter;
import com.minecraftindustrialcomplex.mic_rtsim.body.BodyKinds;
import com.minecraftindustrialcomplex.mic_rtsim.body.GroupKind;
import com.minecraftindustrialcomplex.mic_rtsim.body.MemberCapture;
import com.minecraftindustrialcomplex.mic_rtsim.body.MemberSpawn;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.BoundarySession;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Buffers;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Engine;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
import com.minecraftindustrialcomplex.mic_rtsim.journal.JournalSavedData;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.AgentTag;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetRegistry;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.storage.LevelResource;
import net.minecraft.world.phys.Vec3;

import java.nio.ByteBuffer;
import java.nio.file.Path;
import java.util.UUID;

/**
 * The bridge for one server session (PLAN.md §6): the boundary session (engine, hand-over, journal),
 * the live puppet registry, and everything that turns Minecraft events into observations and
 * commands into entities. Server thread only.
 *
 * <p><b>Per tick</b>, at the end of the server tick ({@link #endOfTick}):
 * <ol>
 *   <li>apply the commands of finished steps, spawns and despawns under the per-tick quota, a whole
 *       group at a time;</li>
 *   <li>the puppet pass: capture groups a leave event flagged, check taking (leash holder, tamed),
 *       test ticking per group, refresh despawn timers, send puppet state;</li>
 *   <li>observe players every {@code playerInterval} ticks;</li>
 *   <li>hand over: copy unsent journal entries and submit the next step if the protocol allows.</li>
 * </ol>
 * Nothing here waits for the engine.
 */
public final class RtsimSession {

    private static RtsimSession current;
    /** Why the last server start opened no session, or null. */
    private static volatile String startFailure;

    /** The server's session, or null before start, after stop, on a client, or when the engine did not open. */
    public static RtsimSession current() {
        return current;
    }

    /** Why the last server start has no session (the engine did not open), or null. */
    public static String startFailure() {
        return startFailure;
    }

    // ---- lifecycle ------------------------------------------------------------------------------

    /**
     * Opens the engine {@code engine.implementation} names for this world: the Rust engine on
     * {@code <world>/rtsim/}, or the stub on {@code <world>/rtsim/stub-pages.bin}.
     */
    private static Engine openEngine(MinecraftServer server, String kind) {
        Path dir = server.getWorldPath(LevelResource.ROOT).resolve("rtsim");
        if (RtsimConfig.ENGINE_STUB.equals(kind))
            return StubEngine.open(dir.resolve("stub-pages.bin"));
        int flags = RtsimConfig.Test.ENABLED ? NativeEngine.FLAG_TEST | NativeEngine.FLAG_NO_FSYNC : 0;
        return NativeEngine.open(dir, 0, flags);
    }

    /**
     * Server started: open the engine and the journal, replay what the engine has not acknowledged.
     * If the engine does not open, there is no session this time (no fallback to the stub, A8): the
     * log and {@code /rtsim perf} say why.
     */
    static void startGlobal(MinecraftServer server) {
        if (current != null)
            current.close();
        current = null;
        startFailure = null;
        // UR-1: a world that ran without rtsim is reset before the engine opens; a reset that cannot be done opens nothing.
        try {
            ReinstallGuard.beforeEngineOpens(server);
        } catch (java.io.IOException | RuntimeException e) {
            startFailure = "the world ran without rtsim and the reinstall reset failed (" + e + "); move world/rtsim/ aside by hand"
                    + " and start again (docs/deploy-rehearsal-omegasrv.md runbook 8.6)";
            MicRtsim.LOGGER.error("[mic_rtsim] {}; the bridge does not run this session", startFailure, e);
            return;
        }
        String kind = "configured";
        Engine engine = null;
        try {
            kind = RtsimConfig.engine();
            engine = openEngine(server, kind);
            current = new RtsimSession(server, engine, JournalSavedData.get(server));
        } catch (RuntimeException | LinkageError e) {
            startFailure = "the " + kind + " engine did not start: " + e.getMessage();
            MicRtsim.LOGGER.error("[mic_rtsim] {}; the bridge does not run this session (no fallback to the stub)", startFailure, e);
            if (engine != null) {
                try {
                    engine.close();
                } catch (RuntimeException e2) {
                    MicRtsim.LOGGER.error("[mic_rtsim] closing the engine that did not start also failed", e2);
                }
            }
            return;
        }
        RtsimSession started = current;
        com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugins.each("sessionStarted", p -> p.sessionStarted(started));
        MicRtsim.LOGGER.info("[mic_rtsim] session started: {}; journal holds {} unacknowledged observations ({} bytes), last applied command {}",
                engine.describe(), current.journal().journal().size(), current.journal().journal().bytes(),
                current.journal().journal().lastAppliedCmdSeq());
    }

    /** Server stopping, before the world save: capture every puppet so its state reaches the journal. */
    static void stoppingGlobal() {
        if (current != null) {
            RtsimSession s = current;
            s.captureAll(Obs.Dematerialized.SESSION_END);
            com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugins.each("sessionStopping", p -> p.sessionStopping(s));
        }
    }

    /** Server stopped: close the engine. */
    static void stopGlobal() {
        if (current != null) {
            current.close();
            current = null;
        }
    }

    /**
     * GameTests only: what a restart does to the session, in this server: stopping (every puppet captured),
     * the world saved, stopped, started again (UR-1's reinstall test). The world's saved data stay the same
     * objects, as a restart would read them back.
     */
    public static void restartGlobalForTest(MinecraftServer server) {
        if (!RtsimConfig.Test.ENABLED)
            throw new IllegalStateException("a session restart is for GameTests only");
        stoppingGlobal();
        server.saveEverything(true, true, true);
        stopGlobal();
        startGlobal(server);
    }

    // ---- state ----------------------------------------------------------------------------------

    final MinecraftServer server;
    final Engine engine;
    final JournalSavedData journalData;
    final BoundarySession boundary;
    /**
     * The engine life (ReinstallRules.engineLife, RG13): the page directory's, new when a new directory's
     * engine starts over (new pages and observations lost), which is when its reconcile epochs start again
     * from 1 without the old results. The industry module stamps applied epochs with it.
     */
    final long engineLife;
    final PuppetRegistry registry = new PuppetRegistry();
    final CommandDispatcher commands;
    /** Phase 1: villages, markets, gossip, damage, rescues, robbery, bases, growth (PLAN.md §8). */
    public final Phase1 phase1;
    /** The industry module (C15 to C19, INDUSTRY.md): districts, the stock sampler, ports, reconciles. */
    public final com.minecraftindustrialcomplex.mic_rtsim.industry.Industry industry;
    /** The engine's dragon legs for the far feed (FAR_LEG, docs/boundary.md §12), per session. */
    public final com.minecraftindustrialcomplex.mic_rtsim.far.FarLegs farLegs = new com.minecraftindustrialcomplex.mic_rtsim.far.FarLegs();
    /** Every level's topology (TP18): sent to the engine, read by the bridge's steering. */
    final com.minecraftindustrialcomplex.mic_rtsim.topology.TopologyTracker topology =
            new com.minecraftindustrialcomplex.mic_rtsim.topology.TopologyTracker();

    private boolean failed;
    /**
     * Tagged entities that belong to no live puppet but were never removed: vanilla never unloads an
     * entity that refuses to be saved (PersistentEntitySectionManager unloads only what it stored), so
     * the bridge discards them itself at the end of the tick.
     */
    private final java.util.ArrayList<Entity> orphans = new java.util.ArrayList<>();
    /**
     * Members that left the sim, by UUID, and one admission token per member by tag. A copy another mod
     * serialized while the member was still tagged (a capture item) is admitted as an ordinary entity
     * when it comes back, instead of being discarded as stale. Bounded (PLACEHOLDER size).
     */
    private final java.util.LinkedHashMap<UUID, AgentTag> releasedUuids = new java.util.LinkedHashMap<>(16, 0.75f, false) {
        @Override
        protected boolean removeEldestEntry(java.util.Map.Entry<UUID, AgentTag> eldest) {
            return size() > RELEASED_CAP;
        }
    };
    private final java.util.HashMap<AgentTag, Integer> releaseTokens = new java.util.HashMap<>();
    private static final int RELEASED_CAP = 4096;
    /** Things to do once the engine acknowledges a sequence: drop pending records from their holders. */
    private final java.util.TreeMap<Long, java.util.List<Runnable>> ackWaiters = new java.util.TreeMap<>();
    /** The pass's copy of the registry, reused every tick (no allocation per tick). */
    private final java.util.ArrayList<PuppetGroup> passList = new java.util.ArrayList<>();
    private long ticks;
    int spawnBudget;

    // Counters for /rtsim perf and the GameTests.
    long spawnedGroups;
    long rejectedSpawns;
    long capturedGroups;
    long staleDiscarded;
    long takes;
    long deaths;
    long removedByOther;
    long passNanos;
    long reasserted;
    /**
     * Holders whose pending records were journaled in this session (a load's reassert or a carry), so a
     * camp's census can journal the rest of its footprint's holders first, once each (MZB-174, MZB-201).
     * Weak: an unloaded holder is journaled again when it loads.
     */
    private final java.util.Set<Entity> asserted = java.util.Collections.newSetFromMap(new java.util.WeakHashMap<>());
    /** Pending records a holder brought from an earlier engine life, dropped unjournaled (RG14). */
    long stalePending;
    /** Leaders steered again after a seam folded them (TP11). */
    long resteered;

    public RtsimSession(MinecraftServer server, Engine engine, JournalSavedData journal) {
        this.server = server;
        this.engine = engine;
        this.journalData = journal;
        this.boundary = new BoundarySession(engine, journal.journal(), RtsimConfig.effectiveMask(), gameTime());
        boolean startsOver = ReinstallRules.startsOver(boundary.start().pagesObsSeq(), boundary.observationsLost());
        try {
            this.engineLife = ReinstallRules.engineLife(server.getWorldPath(LevelResource.ROOT).resolve("rtsim"), startsOver,
                    new java.security.SecureRandom()::nextLong);
        } catch (java.io.IOException e) {
            throw new java.io.UncheckedIOException("the engine life (world/rtsim/" + ReinstallRules.LIFE_FILE + "): " + e.getMessage(), e);
        }
        if (startsOver)
            MicRtsim.LOGGER.info("[mic_rtsim] the engine starts over (new pages, observations lost): engine life {}; reconcile"
                    + " epochs that chunks hold of another life are void as the chunks load", Long.toHexString(engineLife));
        this.commands = new CommandDispatcher(this);
        this.boundary.onAck(this::onAcked);
        for (int i = 0; i < 2; i++) {
            int o = Buffers.capacity(engine.observationBuffer(i).duplicate().order(Layout.ORDER));
            if (o < Layout.MIN_CAPACITY)
                MicRtsim.LOGGER.warn("[mic_rtsim] observation buffer {} holds {} bytes, below the {} a largest record needs",
                        i, o, Layout.MIN_CAPACITY);
        }
        for (ServerLevel level : server.getAllLevels())
            dimId(level.dimension());
        for (GroupKind kind : BodyKinds.groups())
            boundary.dictId(Obs.DICT_BODY, kind.name());
        for (BodyAdapter body : BodyKinds.bodies())
            boundary.dictId(Obs.DICT_BODY, body.name());
        this.phase1 = new Phase1(this);
        this.industry = new com.minecraftindustrialcomplex.mic_rtsim.industry.Industry(this, server);
    }

    public MinecraftServer server() {
        return server;
    }

    /** A level's topology as the bridge last sent it to the engine (TP18). */
    public com.minecraftindustrialcomplex.mic_rtsim.topology.LevelTopology topology(Level level) {
        return topology.of(level);
    }

    /** The tracker itself (plugins report a change through {@code RtsimApi.topologyChanged}). */
    public com.minecraftindustrialcomplex.mic_rtsim.topology.TopologyTracker topologies() {
        return topology;
    }

    /** The id of a name in Java's dictionary (Obs.DICT_*). */
    public int dictId(int dictKind, String name) {
        return boundary.dictId(dictKind, name);
    }

    /** Journals a COMMAND_RESULT; returns the record (its sequence at Obs.OBS_SEQ). */
    public ByteBuffer commandResult(long cmdSeq, long opId, int status, long now) {
        return journalCommandResult(cmdSeq, opId, status, now);
    }

    /** The chunk carries an applied engine op's COMMAND_RESULT until the engine acknowledges it (B42). */
    public void carryResultInChunk(net.minecraft.world.level.chunk.LevelChunk chunk, ByteBuffer commandResult) {
        carryInChunk(chunk, commandResult);
    }

    /** An entity carries journal records until the engine acknowledges {@code lastSeq} (B42). */
    public void carryOnEntity(Entity holder, java.util.List<byte[]> records, long lastSeq) {
        carry(holder, records, lastSeq);
    }

    /**
     * The tag of a {@code minecraft:marker} that only holds records (MOWZIE.md §4.9): it is discarded once
     * the engine has acknowledged what it holds. Saved in the marker's vanilla {@code Tags}, so a world
     * without rtsim keeps an inert marker (no tick, never sent to a client).
     */
    public static final String RECORD_MARKER_TAG = "mic_rtsim.record";

    /**
     * A camp record rides on a body or a marker (MOWZIE.md §4.9): carried as {@link #carryOnEntity}, and the
     * journal's next file waits for the holder's entity chunk (ledger 22). The records come with the
     * generation each was journaled in (docs/boundary.md §3.2, MZB-652): a marker takes over a body's
     * records from earlier sessions, and a copy of one is judged by the generation that first journaled it,
     * so it must not be stamped with this session's.
     */
    public void carryOnCampHolder(Entity holder, java.util.List<com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords.Entry> records, long lastSeq) {
        carryEntries(holder, records, lastSeq);
        if (holder.level() instanceof ServerLevel level)
            journalData.chunks().dependsOnHolder(level, holder);
    }

    public Engine engine() {
        return engine;
    }

    public JournalSavedData journal() {
        return journalData;
    }

    public BoundarySession boundary() {
        return boundary;
    }

    /** The engine life (RG13): changes exactly when the engine's pages start over. */
    public long engineLife() {
        return engineLife;
    }

    public PuppetRegistry registry() {
        return registry;
    }

    public boolean failed() {
        return failed;
    }

    public long gameTime() {
        return server.overworld().getGameTime();
    }

    public void close() {
        industry.close();
        engine.close();
    }

    public void onModulesChanged() {
        int mask = RtsimConfig.effectiveMask();
        boundary.setModuleMask(mask);
        farLegs.setEnabled((mask & com.minecraftindustrialcomplex.mic_rtsim.config.Module.ECOLOGY.bit()) != 0);
    }

    // ---- the tick -------------------------------------------------------------------------------

    public void endOfTick() {
        if (failed)
            return;
        try {
            long now = gameTime();
            spawnBudget = RtsimConfig.spawnQuota();
            boundary.applyCommands(commands);
            topology.tick(server, now, boundary, this::dimId);
            long t0 = System.nanoTime();
            pass(now);
            discardOrphans();
            phase1.endOfTick(ticks, now);
            passNanos = System.nanoTime() - t0;
            if (ticks % RtsimConfig.playerInterval() == 0)
                observePlayers(now);
            industry.endOfTick(now);
            com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugins.each("endOfTick", p -> p.endOfTick(this, now));
            boundary.handOver(now);
            ticks++;
        } catch (Throwable t) {
            fail(t);
        }
    }

    private void fail(Throwable t) {
        failed = true;
        MicRtsim.LOGGER.error("[mic_rtsim] the bridge failed and stops for this session; puppets are captured and removed", t);
        try {
            captureAll(Obs.Dematerialized.SESSION_END);
        } catch (Throwable t2) {
            MicRtsim.LOGGER.error("[mic_rtsim] capturing puppets after the failure also failed", t2);
        }
        com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugins.each("sessionFailed", p -> p.sessionFailed(this));
    }

    // ---- the puppet pass ------------------------------------------------------------------------

    private void pass(long now) {
        if (registry.size() == 0)
            return;
        boolean full = ticks % RtsimConfig.passInterval() == 0;
        boolean shadow = ticks % RtsimConfig.shadowInterval() == 0;
        for (PuppetGroup g : registry.snapshotInto(passList)) {
            settleDeaths(g, now);
            if (registry.byAgent(g.agent) != g)
                continue; // every member died
            if (g.captureRequested != 0) {
                captureGroup(g, g.captureRequested, now);
                continue;
            }
            if (!full && !g.dirty) {
                if (shadow)
                    shadowGroup(g, now);
                continue;
            }
            g.dirty = false;

            // Taking first: a member a player took leaves the sim before the ticking test can capture it.
            for (PuppetGroup.Member m : g.members) {
                if (m.gone || m.entity == null || m.entity.isRemoved())
                    continue;
                int how = adapter(g, m).takenBy(m.entity, g);
                if (how != 0)
                    take(g, m, m.entity, how, takerOf(m.entity), now);
            }
            if (registry.byAgent(g.agent) != g)
                continue; // every member was taken

            // The ticking test is per group: one frozen member and the whole group goes (C25).
            if (!groupTicking(g)) {
                captureGroup(g, Obs.Dematerialized.NOT_TICKING, now);
                continue;
            }
            for (PuppetGroup.Member m : g.members)
                if (!m.gone)
                    adapter(g, m).maintain(m.entity, g);
            resteer(g);
            if (shadow || phase1.cargoCheckSoon)
                phase1.cargo.check(g, now);
            if (shadow)
                shadowGroup(g, now);
        }
    }

    /**
     * The image of an engine target nearest a body, in the body's own frame (TP11): never through an
     * edge_stop seam (TP12). On a flat level the target itself.
     */
    public Vec3 steerImage(Entity body, Vec3 target) {
        com.minecraftindustrialcomplex.mic_rtsim.topology.LevelTopology t = topology.of(body.level());
        if (t.isFlat())
            return target;
        double[] i = t.nearestImage(body.getX(), body.getZ(), target.x, target.z, true);
        return new Vec3(i[0], target.y, i[1]);
    }

    /**
     * A leader that crossed a seam was folded back into the domain by Toroidal World or the wrap engine
     * (no event fires): its target's nearest image moved, so it is steered again toward the new one.
     * The jump is a short step through the seam, never a teleport (TP11).
     */
    private void resteer(PuppetGroup g) {
        Entity leader = g.leader();
        if (g.steerTarget == null || leader == null)
            return;
        Vec3 image = steerImage(leader, g.steerTarget);
        if (g.steerImage != null && image.distanceToSqr(g.steerImage) < 1.0)
            return;
        g.steerImage = image;
        adapter(g, g.members[0]).steer(leader, image, g.steerSpeed);
        resteered++;
    }

    /** True while every live member stands where entities actually tick (PLAN.md §6.1 item 4). */
    static boolean groupTicking(PuppetGroup g) {
        for (PuppetGroup.Member m : g.members) {
            if (m.gone)
                continue;
            Entity e = m.entity;
            if (e == null || e.isRemoved() || !(e.level() instanceof ServerLevel level))
                return false;
            if (!entitiesTickAt(level, e.blockPosition()))
                return false;
        }
        return true;
    }

    /**
     * Whether an entity standing at {@code pos} is ticked. {@code isPositionEntityTicking} is the entity
     * section's TICKING visibility and the distance manager's entity-ticking range, the two per-entity
     * tests ServerLevel.tick applies. ServerLevel.tick also stops ticking every entity in a level that
     * has had no player and no forced chunk for 300 ticks ({@code emptyTime}, private); a level with
     * neither counts as not ticking here at once, which is what a puppet there needs anyway: no player
     * is interested.
     */
    public static boolean entitiesTickAt(ServerLevel level, net.minecraft.core.BlockPos pos) {
        if (level.players().isEmpty() && !net.neoforged.neoforge.common.world.chunk.ForcedChunkManager.hasForcedChunks(level))
            return false;
        return level.isPositionEntityTicking(pos);
    }

    /**
     * Takes {@code bodies} from this tick's spawn and despawn quota (PLAN.md §4.1), for a plugin's own
     * puppets: false when they do not fit what is left (the command waits for the next tick). A group
     * bigger than the whole quota goes alone at the start of a tick, as the core's own.
     */
    public boolean spendSpawnBudget(int bodies) {
        if (bodies > spawnBudget && spawnBudget != RtsimConfig.spawnQuota())
            return false;
        spawnBudget -= bodies;
        return true;
    }

    /** Journals the deaths LivingDeathEvent reported this tick, for members that are really dead. */
    private void settleDeaths(PuppetGroup g, long now) {
        for (PuppetGroup.Member m : g.members) {
            if (m.gone || m.pendingDeath == null)
                continue;
            Entity e = m.entity;
            boolean dead = e == null || e.isRemoved() || (e instanceof net.minecraft.world.entity.LivingEntity l && l.isDeadOrDying());
            if (dead) {
                died(g, m, m.pendingDeath, m.pendingKiller, now);
            } else {
                m.pendingDeath = null; // a later handler cancelled the death
                m.pendingKiller = null;
            }
            if (registry.byAgent(g.agent) != g)
                return;
        }
    }

    private void discardOrphans() {
        if (orphans.isEmpty())
            return;
        for (Entity e : orphans) {
            AgentTag tag = RtsimAttachments.tag(e);
            if (!e.isRemoved() && tag != null && registry.live(tag, e.getUUID()) == null)
                e.discard();
        }
        orphans.clear();
    }

    private static UUID takerOf(Entity e) {
        if (e instanceof net.minecraft.world.entity.Leashable l && l.getLeashHolder() instanceof Player p)
            return p.getUUID();
        if (e instanceof net.minecraft.world.entity.OwnableEntity o)
            return o.getOwnerUUID();
        return null;
    }

    static BodyAdapter adapter(PuppetGroup g, PuppetGroup.Member m) {
        return g.kind.slots().get(m.index).adapter();
    }

    private void shadowGroup(PuppetGroup g, long now) {
        for (PuppetGroup.Member m : g.members) {
            if (m.gone || m.entity == null)
                continue;
            Entity e = m.entity;
            int dim = dimId(e.level().dimension()); // before direct(): an id is defined before its first use
            int at = boundary.direct(Obs.PUPPET_STATE, Obs.PuppetState.SIZE, m.index, now);
            if (at < 0)
                return;
            ByteBuffer b = boundary.openBuffer();
            b.putLong(at + Obs.PuppetState.AGENT, g.agent);
            b.putInt(at + Obs.PuppetState.EPOCH, g.epoch);
            b.putInt(at + Obs.PuppetState.DIM, dim);
            b.putFloat(at + Obs.PuppetState.HEALTH, e instanceof net.minecraft.world.entity.LivingEntity l ? l.getHealth() : 0f);
            boolean ticking = e.level() instanceof ServerLevel sl && sl.isPositionEntityTicking(e.blockPosition());
            b.putInt(at + Obs.PuppetState.FLAGS, ticking ? Obs.MemberState.FLAG_TICKING : 0);
            b.putDouble(at + Obs.PuppetState.X, e.getX());
            b.putDouble(at + Obs.PuppetState.Y, e.getY());
            b.putDouble(at + Obs.PuppetState.Z, e.getZ());
        }
    }

    private void observePlayers(long now) {
        for (ServerPlayer p : server.getPlayerList().getPlayers()) {
            phase1.observePlayer(p, now);
            int dim = dimId(p.level().dimension());
            int at = boundary.direct(Obs.PLAYER, Obs.Player.SIZE, dim, now);
            if (at < 0)
                return;
            ByteBuffer b = boundary.openBuffer();
            UUID id = p.getUUID();
            b.putLong(at + Obs.Player.UUID_MOST, id.getMostSignificantBits());
            b.putLong(at + Obs.Player.UUID_LEAST, id.getLeastSignificantBits());
            b.putDouble(at + Obs.Player.X, p.getX());
            b.putDouble(at + Obs.Player.Y, p.getY());
            b.putDouble(at + Obs.Player.Z, p.getZ());
        }
    }

    // ---- spawn, capture, take -------------------------------------------------------------------

    /**
     * Materializes a group as one unit: every member is checked, created and tagged first, the group is
     * registered, and only then are the members added to the level, so the join check already knows
     * them. Returns 0, or a {@code SpawnRejected} reason (the caller journals it).
     *
     * @param flags {@code Cmd.SpawnGroup} flags: with {@link Cmd.SpawnGroup#FLAG_SURFACE} each member's y
     *              is the surface at its own x, z, not the anchor's y
     */
    int spawnGroup(long agent, int epoch, GroupKind kind, ServerLevel level, Vec3 anchor, MemberSpawn[] spawns, int flags, long now) {
        PuppetGroup existing = registry.byAgent(agent);
        if (existing != null)
            return Obs.SpawnRejected.ALREADY_EMBODIED;
        Vec3[] at = new Vec3[kind.size()];
        for (int i = 0; i < at.length; i++) {
            at[i] = anchor.add(kind.slots().get(i).offset());
            if (!entitiesTickAt(level, net.minecraft.core.BlockPos.containing(at[i])))
                return Obs.SpawnRejected.NOT_TICKING;
            if ((flags & Cmd.SpawnGroup.FLAG_SURFACE) != 0) {
                // Only now: the chunk is known to be loaded, so the height map exists.
                int x = net.minecraft.util.Mth.floor(at[i].x), z = net.minecraft.util.Mth.floor(at[i].z);
                at[i] = new Vec3(at[i].x, level.getHeight(net.minecraft.world.level.levelgen.Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, x, z), at[i].z);
            }
        }
        AgentTag tag = new AgentTag(agent, epoch);
        Entity[] bodies = new Entity[at.length];
        for (int i = 0; i < at.length; i++) {
            MemberSpawn spawn = spawns[i] != null ? spawns[i] : new MemberSpawn(i, 0f, -1, -1, java.util.List.of());
            Entity e;
            int refused = Obs.SpawnRejected.SPAWN_FAILED;
            try {
                e = kind.slots().get(i).adapter().create(level, at[i], spawn);
            } catch (BodyAdapter.SpawnRefused ex) {
                MicRtsim.LOGGER.warn("[mic_rtsim] spawn of agent {} refused by {}: {}", agent, kind.slots().get(i).adapter().name(), ex.getMessage());
                e = null;
                refused = ex.reason;
            } catch (RuntimeException ex) {
                MicRtsim.LOGGER.error("[mic_rtsim] body adapter {} failed to create a body", kind.slots().get(i).adapter().name(), ex);
                e = null;
            }
            if (e != null && wouldBeDiscarded(e)) {
                e.discard();
                e = null;
                refused = Obs.SpawnRejected.DISCARDED_BY_SERVER;
            }
            if (e == null) {
                for (int j = 0; j < i; j++)
                    bodies[j].discard();
                return refused;
            }
            e.setData(RtsimAttachments.AGENT, tag);
            if (e instanceof net.minecraft.world.entity.Mob mob)
                mob.setPersistenceRequired();
            bodies[i] = e;
        }
        PuppetGroup group = new PuppetGroup(agent, epoch, kind, level.dimension(), bodies, now);
        registry.add(group);
        for (int i = 0; i < bodies.length; i++) {
            if (!level.addFreshEntity(bodies[i])) {
                group.removingByBridge = true;
                for (Entity e : bodies)
                    if (!e.isRemoved())
                        e.discard();
                registry.remove(group);
                return Obs.SpawnRejected.SPAWN_FAILED;
            }
        }
        for (int i = 0; i < bodies.length; i++)
            kind.slots().get(i).adapter().afterGroupSpawned(bodies[i], group);
        com.minecraftindustrialcomplex.mic_rtsim.market.CargoWatch.expectAll(group);

        ByteBuffer r = boundary.beginJournaled(Obs.MATERIALIZED, Obs.Materialized.SIZE, bodies.length, now);
        r.putLong(Obs.Materialized.AGENT, agent);
        r.putInt(Obs.Materialized.EPOCH, epoch);
        r.putInt(Obs.Materialized.DIM, dimId(level.dimension()));
        boundary.commit(r);
        spawnedGroups++;
        return 0;
    }

    /**
     * ServerLevel.shouldDiscardEntity (private): with {@code spawn-animals} or {@code spawn-npcs} off, the
     * server discards every animal or NPC on its first tick. A puppet there would be destroyed with its
     * cargo, so such a spawn is refused.
     */
    private boolean wouldBeDiscarded(Entity e) {
        if (!server.isSpawningAnimals() && (e instanceof net.minecraft.world.entity.animal.Animal || e instanceof net.minecraft.world.entity.animal.WaterAnimal))
            return true;
        return !server.areNpcsEnabled() && e instanceof net.minecraft.world.entity.npc.Npc;
    }

    void journalRejected(long agent, int epoch, long cmdSeq, int reason, long now) {
        ByteBuffer r = boundary.beginJournaled(Obs.SPAWN_REJECTED, Obs.SpawnRejected.SIZE, reason, now);
        r.putLong(Obs.SpawnRejected.AGENT, agent);
        r.putInt(Obs.SpawnRejected.EPOCH, epoch);
        r.putLong(Obs.SpawnRejected.CMD_SEQ, cmdSeq);
        boundary.commit(r);
        rejectedSpawns++;
    }

    /**
     * Captures every member, journals their state and the DEMATERIALIZED event, and removes them, all
     * in this call: no tick runs between the first removal and the last, so no llama is ever left
     * leashed to a removed trader.
     */
    public void captureGroup(PuppetGroup g, int reason, long now) {
        if (phase1 != null && reason != Obs.Dematerialized.SESSION_END)
            phase1.cargo.check(g, now); // a change nobody reported yet is attributed before the capture
        g.removingByBridge = true;
        int captured = 0;
        for (PuppetGroup.Member m : g.members) {
            if (m.gone)
                continue;
            MemberCapture cap = m.precaptured;
            if (cap == null && m.entity != null)
                cap = capture(g, m);
            if (cap != null) {
                journalMemberState(g, m, cap, now);
                captured++;
            }
        }
        for (int i = g.members.length - 1; i >= 0; i--) {
            PuppetGroup.Member m = g.members[i];
            if (m.gone || m.entity == null || m.entity.isRemoved())
                continue;
            m.entity.ejectPassengers();
            m.entity.discard();
        }
        journalDematerialized(g, reason, captured, now);
        registry.remove(g);
        capturedGroups++;
    }

    void captureAll(int reason) {
        long now = gameTime();
        for (PuppetGroup g : registry.snapshot())
            captureGroup(g, reason, now);
    }

    private MemberCapture capture(PuppetGroup g, PuppetGroup.Member m) {
        MemberCapture cap = new MemberCapture();
        try {
            adapter(g, m).capture(m.entity, g, cap);
        } catch (RuntimeException ex) {
            MicRtsim.LOGGER.error("[mic_rtsim] body adapter {} failed to capture {}", adapter(g, m).name(), m.entity, ex);
            cap.x = m.entity.getX();
            cap.y = m.entity.getY();
            cap.z = m.entity.getZ();
        }
        return cap;
    }

    /**
     * A member leaves the sim and becomes an ordinary entity (A10, PLAN.md §6.2): its state and cargo are
     * journaled with a TAKEN event carrying the cargo's op id, its tag is removed (vanilla saves it from
     * now on), and the rest of the group stays one unit.
     */
    void take(PuppetGroup g, PuppetGroup.Member m, Entity entity, int how, UUID player, long now) {
        if (m.entity != entity)
            m.entity = entity;
        MemberCapture cap = capture(g, m);
        byte[] state = journalMemberState(g, m, cap, now);
        ByteBuffer r = boundary.beginJournaled(Obs.TAKEN, Obs.Taken.SIZE, how, now);
        r.putLong(Obs.Taken.AGENT, g.agent);
        r.putInt(Obs.Taken.EPOCH, g.epoch);
        r.putInt(Obs.Taken.MEMBER, m.index);
        if (player != null) {
            r.putLong(Obs.Taken.PLAYER_MOST, player.getMostSignificantBits());
            r.putLong(Obs.Taken.PLAYER_LEAST, player.getLeastSignificantBits());
        }
        boundary.commitWithOpId(r, Obs.Taken.OP_ID);
        entity.removeData(RtsimAttachments.AGENT);
        // The creature is ordinary now and vanilla saves it, maybe on a chunk unload long before the
        // journal's next save: it carries its own TAKEN until the engine has it.
        if (!entity.isRemoved())
            carry(entity, java.util.List.of(state, r.array()), r.getLong(Obs.OBS_SEQ));
        try {
            adapter(g, m).onTaken(entity, g);
        } catch (RuntimeException ex) {
            MicRtsim.LOGGER.error("[mic_rtsim] body adapter {} failed in onTaken", adapter(g, m).name(), ex);
        }
        release(g, m);
        registry.forgetMember(g, m);
        if (registry.byAgent(g.agent) == null)
            journalDematerialized(g, Obs.Dematerialized.MEMBERS_GONE, 0, now);
        else
            phase1.recapOffers(g); // the cargo it carried is gone from what the merchant can sell
        takes++;
    }

    /** Takes a live puppet out of the sim. Returns false when the entity is not a live puppet. */
    public boolean takeEntity(Entity e, int how, UUID player) {
        AgentTag tag = RtsimAttachments.tag(e);
        if (tag == null)
            return false;
        com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugin owner =
                com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugins.ownerOf(tag.agent());
        if (owner != null) {
            // A plugin's own puppet (a herd animal): the plugin takes it, with its own records.
            try {
                return owner.take(this, e, tag, how, player);
            } catch (RuntimeException ex) {
                MicRtsim.LOGGER.error("[mic_rtsim] plugin {} failed taking {}", owner.name(), e, ex);
                return false;
            }
        }
        PuppetGroup g = registry.live(tag, e.getUUID());
        if (g == null)
            return false;
        take(g, g.member(e.getUUID()), e, how, player, gameTime());
        return true;
    }

    private void release(PuppetGroup g, PuppetGroup.Member m) {
        AgentTag tag = g.tag();
        releasedUuids.put(m.uuid, tag);
        releaseTokens.merge(tag, 1, Integer::sum);
        if (releaseTokens.size() > RELEASED_CAP)
            releaseTokens.clear(); // PLACEHOLDER bound; a copy that comes back later is treated as stale
    }

    /** A tagged copy of a member that already left the sim: admit it once, as an ordinary entity. */
    private boolean admitReleased(Entity e, AgentTag tag) {
        AgentTag byUuid = releasedUuids.remove(e.getUUID());
        AgentTag key = byUuid != null ? byUuid : tag;
        Integer tokens = releaseTokens.get(key);
        if (byUuid == null && (tokens == null || tokens <= 0))
            return false;
        if (tokens != null) {
            if (tokens <= 1)
                releaseTokens.remove(key);
            else
                releaseTokens.put(key, tokens - 1);
        }
        e.removeData(RtsimAttachments.AGENT);
        return true;
    }

    void died(PuppetGroup g, PuppetGroup.Member m, MemberCapture atDeath, UUID killer, long now) {
        byte[] state = journalMemberState(g, m, atDeath != null ? atDeath : capture(g, m), now);
        ByteBuffer r = boundary.beginJournaled(Obs.DIED, Obs.Died.SIZE, m.index, now);
        r.putLong(Obs.Died.AGENT, g.agent);
        r.putInt(Obs.Died.EPOCH, g.epoch);
        if (killer != null) {
            r.putLong(Obs.Died.KILLER_MOST, killer.getMostSignificantBits());
            r.putLong(Obs.Died.KILLER_LEAST, killer.getLeastSignificantBits());
        }
        boundary.commitWithOpId(r, Obs.Died.OP_ID);
        // Each item it dropped carries the DIED: an item saved on a chunk unload before the journal
        // must not let the engine bring the cargo back as well.
        if (m.pendingDrops != null) {
            for (Entity drop : m.pendingDrops)
                if (!drop.isRemoved())
                    carry(drop, java.util.List.of(state, r.array()), r.getLong(Obs.OBS_SEQ));
            m.pendingDrops = null;
        }
        m.pendingDeath = null;
        registry.forgetMember(g, m);
        if (registry.byAgent(g.agent) == null)
            journalDematerialized(g, Obs.Dematerialized.MEMBERS_GONE, 0, now);
        else
            phase1.recapOffers(g);
        deaths++;
    }

    private byte[] journalMemberState(PuppetGroup g, PuppetGroup.Member m, MemberCapture cap, long now) {
        int items = Math.min(cap.items.size(), (0xFFF8 - Obs.MemberState.ITEMS) / Obs.Item.SIZE);
        ByteBuffer r = boundary.beginJournaled(Obs.MEMBER_STATE, Obs.MemberState.size(items), m.index, now);
        r.putLong(Obs.MemberState.AGENT, g.agent);
        r.putInt(Obs.MemberState.EPOCH, g.epoch);
        r.putInt(Obs.MemberState.BODY_KIND, boundary.dictId(Obs.DICT_BODY, adapter(g, m).name()));
        ResourceKey<Level> dim = m.entity != null ? m.entity.level().dimension() : g.dimension;
        r.putInt(Obs.MemberState.DIM, dimId(dim));
        r.putInt(Obs.MemberState.FLAGS, cap.flags);
        r.putFloat(Obs.MemberState.HEALTH, cap.health);
        r.putFloat(Obs.MemberState.MAX_HEALTH, cap.maxHealth);
        r.putDouble(Obs.MemberState.X, cap.x);
        r.putDouble(Obs.MemberState.Y, cap.y);
        r.putDouble(Obs.MemberState.Z, cap.z);
        r.putInt(Obs.MemberState.BODY_A, cap.bodyA);
        r.putInt(Obs.MemberState.BODY_B, cap.bodyB);
        r.putInt(Obs.MemberState.ITEM_COUNT, items);
        for (int i = 0; i < items; i++) {
            MemberCapture.Stack s = cap.items.get(i);
            int at = Obs.MemberState.ITEMS + i * Obs.Item.SIZE;
            r.putInt(at + Obs.Item.SLOT, s.slot());
            r.putInt(at + Obs.Item.ITEM, itemId(s.item()));
            r.putInt(at + Obs.Item.COUNT, s.count());
        }
        boundary.commit(r);
        return r.array();
    }

    private void journalDematerialized(PuppetGroup g, int reason, int members, long now) {
        journalDematerialized(g.agent, g.epoch, reason, members, now);
    }

    void journalDematerialized(long agent, int epoch, int reason, int members, long now) {
        ByteBuffer r = boundary.beginJournaled(Obs.DEMATERIALIZED, Obs.Dematerialized.SIZE, reason, now);
        r.putLong(Obs.Dematerialized.AGENT, agent);
        r.putInt(Obs.Dematerialized.EPOCH, epoch);
        r.putInt(Obs.Dematerialized.MEMBERS, members);
        boundary.commit(r);
    }

    /** Journals a COMMAND_RESULT; returns the committed record (its sequence at {@link Obs#OBS_SEQ}). */
    ByteBuffer journalCommandResult(long cmdSeq, long opId, int status, long now) {
        ByteBuffer r = boundary.beginJournaled(Obs.COMMAND_RESULT, Obs.CommandResult.SIZE, status, now);
        r.putLong(Obs.CommandResult.CMD_SEQ, cmdSeq);
        r.putLong(Obs.CommandResult.OP_ID, opId);
        boundary.commit(r);
        return r;
    }

    // ---- pending records: the world side of a transfer carries its journal records ---------------

    /** Runs {@code then} once the engine has acknowledged {@code seq} in this session (for modules and plugins). */
    public void afterAck(long seq, Runnable then) {
        whenAcked(seq, then);
    }

    /** Runs {@code then} once the engine has acknowledged {@code seq} in this session. */
    void whenAcked(long seq, Runnable then) {
        ackWaiters.computeIfAbsent(seq, k -> new java.util.ArrayList<>(1)).add(then);
    }

    private void onAcked(long acked) {
        while (!ackWaiters.isEmpty() && ackWaiters.firstKey() <= acked) {
            for (Runnable r : ackWaiters.pollFirstEntry().getValue()) {
                try {
                    r.run();
                } catch (RuntimeException ex) {
                    MicRtsim.LOGGER.error("[mic_rtsim] dropping acknowledged pending records failed", ex);
                }
            }
        }
    }

    /**
     * Adds records journaled in this session to an entity's pending list, to be dropped once {@code lastSeq}
     * is acknowledged.
     */
    private void carry(Entity holder, java.util.List<byte[]> records, long lastSeq) {
        java.util.List<com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords.Entry> entries = new java.util.ArrayList<>(records.size());
        for (byte[] r : records)
            entries.add(new com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords.Entry(boundary.gen(), r));
        carryEntries(holder, entries, lastSeq);
    }

    /** As {@link #carry}, for records that already carry the generation they were journaled in. */
    private void carryEntries(Entity holder, java.util.List<com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords.Entry> entries, long lastSeq) {
        ensureReasserted(holder); // what it carried from disk goes first, under its own sequence
        asserted.add(holder);
        com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords old = holder.getData(RtsimAttachments.PENDING);
        if (!old.isEmpty() && old.life() != engineLife)
            stalePending += old.entries().size(); // of an engine that started over (RG14): left out below, never journaled; counted, not asserted
        holder.setData(RtsimAttachments.PENDING, old.plusEntries(engineLife, entries));
        dropPendingWhenAcked(holder, lastSeq);
    }

    /**
     * Once {@code lastSeq} is acknowledged, drops the records the holder carries now (each journaled at or
     * before it), by identity: a record carried later, under a newer sequence, stays until its own
     * acknowledgement (MZB-212). A record marker left with none is discarded. It finds the holder among the
     * visible entities only, so one that is hidden when the acknowledgement comes keeps its records until
     * its next reassert, which drops them at the next acknowledgement; that is harmless (MZB-241).
     */
    private void dropPendingWhenAcked(Entity holder, long lastSeq) {
        ResourceKey<Level> dim = holder.level().dimension();
        UUID id = holder.getUUID();
        com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords now = holder.getExistingDataOrNull(RtsimAttachments.PENDING);
        java.util.Set<byte[]> acked = java.util.Collections.newSetFromMap(new java.util.IdentityHashMap<>());
        if (now != null)
            acked.addAll(now.records());
        whenAcked(lastSeq, () -> {
            ServerLevel level = server.getLevel(dim);
            Entity e = level == null ? null : level.getEntity(id);
            if (e == null)
                return; // unloaded: journaled again when it loads
            com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords p = e.getExistingDataOrNull(RtsimAttachments.PENDING);
            if (p != null) {
                com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords keep = p.filter(r -> !acked.contains(r));
                if (keep.isEmpty())
                    e.removeData(RtsimAttachments.PENDING);
                else if (keep.entries().size() != p.entries().size())
                    e.setData(RtsimAttachments.PENDING, keep);
                if (!keep.isEmpty())
                    return;
            }
            if (e instanceof net.minecraft.world.entity.Marker && e.getTags().contains(RECORD_MARKER_TAG))
                e.discard();
        });
    }

    /**
     * An entity that carries pending records came back from disk: journal a copy of each (REASSERT,
     * docs/boundary.md §3.2; the engine has maybe never seen them, if the journal on disk was older than
     * the entity's chunk, and applies a copy only then) and drop them once the copies are acknowledged.
     */
    public void reassert(Entity holder) {
        if (!asserted.add(holder))
            return; // already journaled in this session (a camp's census came first, or a carry)
        com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords p = holder.getExistingDataOrNull(RtsimAttachments.PENDING);
        if (p == null || p.isEmpty())
            return;
        if (p.life() != engineLife) {
            stalePending += p.records().size(); // of an engine that started over since (RG14): never journaled again
            holder.removeData(RtsimAttachments.PENDING);
            return;
        }
        long last = 0;
        for (com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords.Entry r : p.entries())
            last = boundary.reassert(r.gen(), r.bytes());
        reasserted += p.entries().size();
        dropPendingWhenAcked(holder, last);
    }

    /**
     * Journals a holder's pending records again unless this session already has (MZB-174): a camp's
     * census calls it for every holder in its footprint before it is sent, so a holder's answer reaches
     * the engine before the census whose epoch would make it stale. Idempotent.
     */
    public void ensureReasserted(Entity holder) {
        if (!asserted.contains(holder) && holder.hasData(RtsimAttachments.PENDING))
            reassert(holder);
    }

    /** Whether this session has journaled the holder's pending records (GameTests). */
    public boolean reasserted(Entity holder) {
        return asserted.contains(holder);
    }

    /**
     * The same for a chunk: its pending records are COMMAND_RESULTs of engine ops applied into it, each
     * with the records Java journaled for it just before (IND_WRITE_CHAIN's CHAIN_IN). An op's records are
     * dropped together once the last of them is acknowledged.
     */
    public void reassert(net.minecraft.world.level.chunk.LevelChunk chunk) {
        com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords p = chunk.getExistingDataOrNull(RtsimAttachments.PENDING);
        if (p == null || p.isEmpty())
            return;
        if (p.life() != engineLife) {
            stalePending += p.records().size(); // of an engine that started over since (RG14): never journaled again
            chunk.removeData(RtsimAttachments.PENDING);
            chunk.setUnsaved(true);
            return;
        }
        java.util.Map<Long, Long> last = new java.util.LinkedHashMap<>();
        for (com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords.Entry r : p.entries())
            last.put(pendingOp(r.bytes()), boundary.reassert(r.gen(), r.bytes()));
        for (java.util.Map.Entry<Long, Long> e : last.entrySet())
            dropFromChunkWhenAcked(chunk, e.getKey(), e.getValue());
        reasserted += p.entries().size();
    }

    /**
     * The op id a record a chunk carries is pending for: a COMMAND_RESULT's, or the engine op of an
     * IND_CHAIN_PACKAGE an IND_WRITE_CHAIN journaled (SECTIONS.md §2.5).
     */
    public static long pendingOp(byte[] record) {
        ByteBuffer b = ByteBuffer.wrap(record).order(com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout.ORDER);
        int kind = Short.toUnsignedInt(b.getShort(com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout.R_KIND));
        if (kind == com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords.IND_CHAIN_PACKAGE
                && record.length >= com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords.ChainPackage.OP_ID + 8)
            return b.getLong(com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords.ChainPackage.OP_ID);
        return record.length >= Obs.CommandResult.OP_ID + 8 ? b.getLong(Obs.CommandResult.OP_ID) : 0;
    }

    /**
     * The COMMAND_RESULT of an engine op applied into {@code chunk}, if the chunk still carries one of this engine
     * life (an op id of an engine that started over is no op of this one, RG14).
     */
    public static boolean chunkCarriesOp(net.minecraft.world.level.chunk.LevelChunk chunk, long opId) {
        com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords p = chunk.getExistingDataOrNull(RtsimAttachments.PENDING);
        RtsimSession s = current();
        if (p == null || s != null && p.life() != s.engineLife)
            return false;
        for (byte[] r : p.records())
            if (pendingOp(r) == opId)
                return true;
        return false;
    }

    void carryInChunk(net.minecraft.world.level.chunk.LevelChunk chunk, ByteBuffer commandResult) {
        com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords old = chunk.getData(RtsimAttachments.PENDING);
        chunk.setData(RtsimAttachments.PENDING, old.plus(engineLife, boundary.gen(), java.util.List.of(commandResult.array())));
        chunk.setUnsaved(true);
        journalData.chunks().dependsOn(chunk); // the journal's next file waits for this chunk's (R21)
        dropFromChunkWhenAcked(chunk, commandResult.getLong(Obs.CommandResult.OP_ID), commandResult.getLong(Obs.OBS_SEQ));
    }

    void dropFromChunkWhenAcked(net.minecraft.world.level.chunk.LevelChunk chunk, long opId, long seq) {
        ResourceKey<Level> dim = chunk.getLevel().dimension();
        net.minecraft.world.level.ChunkPos pos = chunk.getPos();
        whenAcked(seq, () -> {
            ServerLevel level = server.getLevel(dim);
            net.minecraft.world.level.chunk.LevelChunk c = level == null ? null : level.getChunkSource().getChunkNow(pos.x, pos.z);
            if (c == null)
                return; // unloaded: it is journaled again when it loads
            com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords p = c.getExistingDataOrNull(RtsimAttachments.PENDING);
            if (p == null)
                return;
            com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords keep = p.filter(r -> pendingOp(r) != opId);
            if (keep.isEmpty())
                c.removeData(RtsimAttachments.PENDING);
            else
                c.setData(RtsimAttachments.PENDING, keep);
            c.setUnsaved(true);
        });
    }

    /**
     * Journals a generic cross-store transfer seen on the Minecraft side, with a Java op id. Returns the
     * op id. (Phase 1 content uses specific kinds; this one is what the journal GameTest exercises.)
     */
    public long journalTransfer(int transferKind, long from, long to, Item item, long amount) {
        long now = gameTime();
        ByteBuffer r = boundary.beginJournaled(Obs.TRANSFER, Obs.Transfer.SIZE, transferKind, now);
        r.putLong(Obs.Transfer.FROM, from);
        r.putLong(Obs.Transfer.TO, to);
        r.putInt(Obs.Transfer.ITEM, item == null ? 0 : itemId(item));
        r.putLong(Obs.Transfer.AMOUNT, amount);
        return boundary.commitWithOpId(r, Obs.Transfer.OP_ID);
    }

    /**
     * Journals a TRANSFER with a given op id: what a replay or a buggy double report looks like. Used by
     * the journal GameTest to send one op id twice.
     */
    public long journalTransferWithOpId(long opId, int transferKind, long from, long to, long amount) {
        ByteBuffer r = boundary.beginJournaled(Obs.TRANSFER, Obs.Transfer.SIZE, transferKind, gameTime());
        r.putLong(Obs.Transfer.OP_ID, opId);
        r.putLong(Obs.Transfer.FROM, from);
        r.putLong(Obs.Transfer.TO, to);
        r.putLong(Obs.Transfer.AMOUNT, amount);
        return boundary.commit(r);
    }

    // ---- events (called by BridgeEvents) --------------------------------------------------------

    /** EntityJoinLevelEvent: true when the joining entity is a stale copy and must not join. */
    boolean rejectOnJoin(Entity e, Level level) {
        AgentTag tag = RtsimAttachments.tag(e);
        if (tag == null)
            return false;
        com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugin owner =
                com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugins.ownerOf(tag.agent());
        if (owner != null) {
            // A plugin's own puppet (a dragon): the plugin knows its live bodies, the registry does not.
            try {
                if (owner.admit(this, e, tag, level))
                    return false;
            } catch (RuntimeException ex) {
                MicRtsim.LOGGER.error("[mic_rtsim] plugin {} failed deciding a joining {}", owner.name(), e, ex);
            }
            staleDiscarded++;
            return true;
        }
        PuppetGroup g = registry.live(tag, e.getUUID());
        if (g == null) {
            if (admitReleased(e, tag))
                return false;
            staleDiscarded++;
            MicRtsim.LOGGER.debug("[mic_rtsim] discarding a stale copy of agent {} epoch {} ({})", tag.agent(), tag.epoch(), e);
            return true;
        }
        PuppetGroup.Member m = g.member(e.getUUID());
        if (m.entity != e) {
            // Only a dimension change may replace a live member: vanilla removes the old entity
            // (CHANGED_DIMENSION) before it adds the copy that Entity.restoreFrom built, tag included.
            // Any other entity with a live member's UUID is a copy, even if it would fail to join anyway.
            if (m.entity != null && !(m.entity.isRemoved() && m.entity.getRemovalReason() == Entity.RemovalReason.CHANGED_DIMENSION)) {
                staleDiscarded++;
                return true;
            }
            m.entity = e;
            // A group is one unit and can no longer stand together: capture it.
            if (!level.dimension().equals(g.dimension) && g.captureRequested == 0)
                g.captureRequested = Obs.Dematerialized.UNLOADED;
        }
        return false;
    }

    /** EntityLeaveLevelEvent for a tagged entity. */
    void onLeave(Entity e) {
        AgentTag tag = RtsimAttachments.tag(e);
        if (tag == null)
            return;
        com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugin owner =
                com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugins.ownerOf(tag.agent());
        if (owner != null) {
            try {
                owner.left(this, e, tag);
            } catch (RuntimeException ex) {
                MicRtsim.LOGGER.error("[mic_rtsim] plugin {} failed on a leaving {}", owner.name(), e, ex);
            }
            return;
        }
        PuppetGroup g = registry.live(tag, e.getUUID());
        Entity.RemovalReason reason = e.getRemovalReason();
        if (g == null) {
            // Tracking ended for a tagged entity no live puppet owns (a cancelled death, a stray copy):
            // vanilla will never unload it, because it refuses to be saved. Discard it after the tick.
            if (reason == null)
                orphans.add(e);
            return;
        }
        if (g.removingByBridge)
            return;
        PuppetGroup.Member m = g.member(e.getUUID());
        if (m.entity != e)
            return; // an old copy leaving after its replacement joined
        long now = gameTime();
        if (m.index == 0 && (reason == Entity.RemovalReason.CHANGED_DIMENSION || reason == Entity.RemovalReason.DISCARDED))
            letGoOfLeader(g, e);
        if (reason == Entity.RemovalReason.CHANGED_DIMENSION)
            return; // the copy joins in the other level
        if (reason == Entity.RemovalReason.KILLED) {
            died(g, m, m.pendingDeath, m.pendingKiller, now); // a death LivingDeathEvent reported, or one it did not
            return;
        }
        if (reason == Entity.RemovalReason.DISCARDED) {
            // Not the bridge (removingByBridge is false): vanilla despawn or another mod (PLAN.md §6.2).
            removedByOther++;
            MicRtsim.LOGGER.warn("[mic_rtsim] puppet {} of agent {} was removed by something else; it leaves the sim", e, g.agent);
            take(g, m, e, Obs.Taken.REMOVED_BY_OTHER, null, now);
            return;
        }
        // Tracking ended (its chunk is no longer accessible) or it is being unloaded: capture this member
        // now, while its fields are still there, and the rest of the group at the end of the tick. Not
        // here: this runs inside the entity manager's own iteration.
        if (m.precaptured == null)
            m.precaptured = capture(g, m);
        if (g.captureRequested == 0)
            g.captureRequested = Obs.Dematerialized.UNLOADED;
    }

    /**
     * The leader is leaving the level without dying (a portal, another mod): its llamas would see a
     * holder that is no longer alive on their next tick and drop the leash with a lead item, before the
     * end-of-tick capture. Letting go now drops no item (this only changes leash data).
     */
    private static void letGoOfLeader(PuppetGroup g, Entity leader) {
        for (PuppetGroup.Member other : g.members) {
            if (other.gone || other.entity == null || other.entity == leader)
                continue;
            if (other.entity instanceof net.minecraft.world.entity.Leashable l && l.getLeashHolder() == leader)
                l.dropLeash(true, false);
        }
    }

    /**
     * LivingDeathEvent (lowest priority, not cancelled so far): capture the state now, before the drops,
     * and journal the death at the end of the tick once it has really happened.
     */
    void onDeath(Entity e, Entity killer) {
        AgentTag tag = RtsimAttachments.tag(e);
        if (tag == null)
            return;
        PuppetGroup g = registry.live(tag, e.getUUID());
        if (g == null)
            return;
        PuppetGroup.Member m = g.member(e.getUUID());
        m.pendingDeath = capture(g, m);
        m.pendingKiller = killer == null ? null : killer.getUUID();
    }

    /** LivingDropsEvent: remember a dying member's drops, so they can carry its DIED. */
    void onDrops(Entity e, java.util.Collection<net.minecraft.world.entity.item.ItemEntity> drops) {
        AgentTag tag = RtsimAttachments.tag(e);
        if (tag == null)
            return;
        PuppetGroup g = registry.live(tag, e.getUUID());
        if (g == null)
            return;
        PuppetGroup.Member m = g.member(e.getUUID());
        if (m.pendingDeath != null)
            m.pendingDrops = new java.util.ArrayList<>(drops);
    }

    void onTame(Entity e, Player tamer) {
        takeEntity(e, Obs.Taken.TAME, tamer == null ? null : tamer.getUUID());
    }

    /** A player used a lead on a puppet: check its group on this tick's pass, not the next full one. */
    void onLeadInteract(Entity e) {
        AgentTag tag = RtsimAttachments.tag(e);
        if (tag == null)
            return;
        PuppetGroup g = registry.live(tag, e.getUUID());
        if (g != null)
            g.dirty = true;
    }

    // ---- dictionary -----------------------------------------------------------------------------

    public int dimId(ResourceKey<Level> dim) {
        return boundary.dictId(Obs.DICT_DIMENSION, dim.location().toString());
    }

    public int itemId(Item item) {
        return boundary.dictId(Obs.DICT_ITEM, BuiltInRegistries.ITEM.getKey(item).toString());
    }

    // ---- counters -------------------------------------------------------------------------------

    public long spawnedGroups() { return spawnedGroups; }
    public long rejectedSpawns() { return rejectedSpawns; }
    public long capturedGroups() { return capturedGroups; }
    public long staleDiscarded() { return staleDiscarded; }
    public long resteered() { return resteered; }
    public long takes() { return takes; }
    public long deaths() { return deaths; }
    public long removedByOther() { return removedByOther; }
    public long passNanos() { return passNanos; }
    public long ticks() { return ticks; }
    public long reasserted() { return reasserted; }
    public long stalePending() { return stalePending; }
    public int ackWaiters() { return ackWaiters.size(); }
}
