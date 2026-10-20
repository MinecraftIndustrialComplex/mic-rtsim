package com.minecraftindustrialcomplex.mic_rtsim.bridge;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.body.BodyAdapter;
import com.minecraftindustrialcomplex.mic_rtsim.body.BodyKinds;
import com.minecraftindustrialcomplex.mic_rtsim.body.GroupKind;
import com.minecraftindustrialcomplex.mic_rtsim.body.MemberCapture;
import com.minecraftindustrialcomplex.mic_rtsim.body.MemberSpawn;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.BoundarySession;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Engine;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
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

    /** The server's session, or null before start, after stop, or on a client. */
    public static RtsimSession current() {
        return current;
    }

    // ---- lifecycle ------------------------------------------------------------------------------

    /** Server started: open the engine and the journal, replay what the engine has not acknowledged. */
    static void startGlobal(MinecraftServer server) {
        if (current != null)
            current.close();
        Path pages = server.getWorldPath(LevelResource.ROOT).resolve("rtsim").resolve("stub-pages.bin");
        Engine engine = StubEngine.open(pages);
        current = new RtsimSession(server, engine, JournalSavedData.get(server));
        MicRtsim.LOGGER.info("[mic_rtsim] session started: {}; journal holds {} unacknowledged observations ({} bytes), last applied command {}",
                engine.describe(), current.journal().journal().size(), current.journal().journal().bytes(),
                current.journal().journal().lastAppliedCmdSeq());
    }

    /** Server stopping, before the world save: capture every puppet so its state reaches the journal. */
    static void stoppingGlobal() {
        if (current != null)
            current.captureAll(Obs.Dematerialized.SESSION_END);
    }

    /** Server stopped: close the engine. */
    static void stopGlobal() {
        if (current != null) {
            current.close();
            current = null;
        }
    }

    // ---- state ----------------------------------------------------------------------------------

    final MinecraftServer server;
    final Engine engine;
    final JournalSavedData journalData;
    final BoundarySession boundary;
    final PuppetRegistry registry = new PuppetRegistry();
    final CommandDispatcher commands;

    private boolean failed;
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

    public RtsimSession(MinecraftServer server, Engine engine, JournalSavedData journal) {
        this.server = server;
        this.engine = engine;
        this.journalData = journal;
        this.boundary = new BoundarySession(engine, journal.journal(), RtsimConfig.effectiveMask(), gameTime());
        this.commands = new CommandDispatcher(this);
        for (ServerLevel level : server.getAllLevels())
            dimId(level.dimension());
        for (GroupKind kind : BodyKinds.groups())
            boundary.dictId(Obs.DICT_BODY, kind.name());
        for (BodyAdapter body : BodyKinds.bodies())
            boundary.dictId(Obs.DICT_BODY, body.name());
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
        engine.close();
    }

    public void onModulesChanged() {
        boundary.setModuleMask(RtsimConfig.effectiveMask());
    }

    // ---- the tick -------------------------------------------------------------------------------

    public void endOfTick() {
        if (failed)
            return;
        try {
            long now = gameTime();
            spawnBudget = RtsimConfig.spawnQuota();
            boundary.applyCommands(commands);
            long t0 = System.nanoTime();
            pass(now);
            passNanos = System.nanoTime() - t0;
            if (ticks % RtsimConfig.playerInterval() == 0)
                observePlayers(now);
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
    }

    // ---- the puppet pass ------------------------------------------------------------------------

    private void pass(long now) {
        if (registry.size() == 0)
            return;
        boolean full = ticks % RtsimConfig.passInterval() == 0;
        boolean shadow = ticks % RtsimConfig.shadowInterval() == 0;
        for (PuppetGroup g : registry.snapshot()) {
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
                    take(g, m, how, takerOf(m.entity), now);
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
            if (shadow)
                shadowGroup(g, now);
        }
    }

    /** True while every live member stands where entities actually tick (PLAN.md §6.1 item 4). */
    static boolean groupTicking(PuppetGroup g) {
        for (PuppetGroup.Member m : g.members) {
            if (m.gone)
                continue;
            Entity e = m.entity;
            if (e == null || e.isRemoved() || !(e.level() instanceof ServerLevel level))
                return false;
            // isPositionEntityTicking is the entity section's TICKING visibility and the distance
            // manager's entity-ticking range together: exactly the two tests ServerLevel.tick applies.
            if (!level.isPositionEntityTicking(e.blockPosition()))
                return false;
        }
        return true;
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
            int at = boundary.direct(Obs.PUPPET_STATE, Obs.PuppetState.SIZE, m.index, now);
            if (at < 0)
                return;
            ByteBuffer b = boundary.openBuffer();
            b.putLong(at + Obs.PuppetState.AGENT, g.agent);
            b.putInt(at + Obs.PuppetState.EPOCH, g.epoch);
            b.putInt(at + Obs.PuppetState.DIM, dimId(e.level().dimension()));
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
     */
    int spawnGroup(long agent, int epoch, GroupKind kind, ServerLevel level, Vec3 anchor, MemberSpawn[] spawns, long now) {
        PuppetGroup existing = registry.byAgent(agent);
        if (existing != null)
            return Obs.SpawnRejected.ALREADY_EMBODIED;
        Vec3[] at = new Vec3[kind.size()];
        for (int i = 0; i < at.length; i++) {
            at[i] = anchor.add(kind.slots().get(i).offset());
            if (!level.isPositionEntityTicking(net.minecraft.core.BlockPos.containing(at[i])))
                return Obs.SpawnRejected.NOT_TICKING;
        }
        AgentTag tag = new AgentTag(agent, epoch);
        Entity[] bodies = new Entity[at.length];
        for (int i = 0; i < at.length; i++) {
            MemberSpawn spawn = spawns[i] != null ? spawns[i] : new MemberSpawn(i, 0f, -1, -1, java.util.List.of());
            Entity e;
            try {
                e = kind.slots().get(i).adapter().create(level, at[i], spawn);
            } catch (RuntimeException ex) {
                MicRtsim.LOGGER.error("[mic_rtsim] body adapter {} failed to create a body", kind.slots().get(i).adapter().name(), ex);
                e = null;
            }
            if (e == null) {
                for (int j = 0; j < i; j++)
                    bodies[j].discard();
                return Obs.SpawnRejected.SPAWN_FAILED;
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

        ByteBuffer r = boundary.beginJournaled(Obs.MATERIALIZED, Obs.Materialized.SIZE, bodies.length, now);
        r.putLong(Obs.Materialized.AGENT, agent);
        r.putInt(Obs.Materialized.EPOCH, epoch);
        r.putInt(Obs.Materialized.DIM, dimId(level.dimension()));
        boundary.commit(r);
        spawnedGroups++;
        return 0;
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
    void take(PuppetGroup g, PuppetGroup.Member m, int how, UUID player, long now) {
        MemberCapture cap = capture(g, m);
        journalMemberState(g, m, cap, now);
        ByteBuffer r = boundary.beginJournaled(Obs.TAKEN, Obs.Taken.SIZE, how, now);
        r.putLong(Obs.Taken.AGENT, g.agent);
        r.putInt(Obs.Taken.EPOCH, g.epoch);
        r.putInt(Obs.Taken.MEMBER, m.index);
        if (player != null) {
            r.putLong(Obs.Taken.PLAYER_MOST, player.getMostSignificantBits());
            r.putLong(Obs.Taken.PLAYER_LEAST, player.getLeastSignificantBits());
        }
        boundary.commitWithOpId(r, Obs.Taken.OP_ID);
        m.entity.removeData(RtsimAttachments.AGENT);
        registry.forgetMember(g, m);
        if (registry.byAgent(g.agent) == null)
            journalDematerialized(g, Obs.Dematerialized.MEMBERS_GONE, 0, now);
        takes++;
    }

    /** Takes a live puppet out of the sim. Returns false when the entity is not a live puppet. */
    public boolean takeEntity(Entity e, int how, UUID player) {
        AgentTag tag = RtsimAttachments.tag(e);
        if (tag == null)
            return false;
        PuppetGroup g = registry.live(tag, e.getUUID());
        if (g == null)
            return false;
        take(g, g.member(e.getUUID()), how, player, gameTime());
        return true;
    }

    void died(PuppetGroup g, PuppetGroup.Member m, UUID killer, long now) {
        journalMemberState(g, m, capture(g, m), now);
        ByteBuffer r = boundary.beginJournaled(Obs.DIED, Obs.Died.SIZE, m.index, now);
        r.putLong(Obs.Died.AGENT, g.agent);
        r.putInt(Obs.Died.EPOCH, g.epoch);
        if (killer != null) {
            r.putLong(Obs.Died.KILLER_MOST, killer.getMostSignificantBits());
            r.putLong(Obs.Died.KILLER_LEAST, killer.getLeastSignificantBits());
        }
        boundary.commitWithOpId(r, Obs.Died.OP_ID);
        registry.forgetMember(g, m);
        if (registry.byAgent(g.agent) == null)
            journalDematerialized(g, Obs.Dematerialized.MEMBERS_GONE, 0, now);
        deaths++;
    }

    private void journalMemberState(PuppetGroup g, PuppetGroup.Member m, MemberCapture cap, long now) {
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
    }

    private void journalDematerialized(PuppetGroup g, int reason, int members, long now) {
        ByteBuffer r = boundary.beginJournaled(Obs.DEMATERIALIZED, Obs.Dematerialized.SIZE, reason, now);
        r.putLong(Obs.Dematerialized.AGENT, g.agent);
        r.putInt(Obs.Dematerialized.EPOCH, g.epoch);
        r.putInt(Obs.Dematerialized.MEMBERS, members);
        boundary.commit(r);
    }

    void journalCommandResult(long cmdSeq, long opId, int status, long now) {
        ByteBuffer r = boundary.beginJournaled(Obs.COMMAND_RESULT, Obs.CommandResult.SIZE, status, now);
        r.putLong(Obs.CommandResult.CMD_SEQ, cmdSeq);
        r.putLong(Obs.CommandResult.OP_ID, opId);
        boundary.commit(r);
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
        PuppetGroup g = registry.live(tag, e.getUUID());
        if (g == null) {
            staleDiscarded++;
            MicRtsim.LOGGER.debug("[mic_rtsim] discarding a stale copy of agent {} epoch {} ({})", tag.agent(), tag.epoch(), e);
            return true;
        }
        PuppetGroup.Member m = g.member(e.getUUID());
        if (m.entity != e) {
            // A dimension change: vanilla rebuilt the member in another level (Entity.restoreFrom copies
            // the tag through saveWithoutId). Follow it, then capture the group: a group is one unit, and
            // it can no longer stand together.
            m.entity = e;
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
        PuppetGroup g = registry.live(tag, e.getUUID());
        if (g == null || g.removingByBridge)
            return;
        PuppetGroup.Member m = g.member(e.getUUID());
        if (m.entity != e)
            return; // an old copy leaving after its replacement joined
        Entity.RemovalReason reason = e.getRemovalReason();
        long now = gameTime();
        if (reason == Entity.RemovalReason.CHANGED_DIMENSION)
            return; // the copy joins in the other level
        if (reason == Entity.RemovalReason.KILLED) {
            died(g, m, null, now); // a death that LivingDeathEvent did not report
            return;
        }
        if (reason == Entity.RemovalReason.DISCARDED) {
            // Not the bridge (removingByBridge is false): vanilla despawn or another mod (PLAN.md §6.2).
            removedByOther++;
            MicRtsim.LOGGER.warn("[mic_rtsim] puppet {} of agent {} was removed by something else; it leaves the sim", e, g.agent);
            take(g, m, Obs.Taken.REMOVED_BY_OTHER, null, now);
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

    void onDeath(Entity e, Entity killer) {
        AgentTag tag = RtsimAttachments.tag(e);
        if (tag == null)
            return;
        PuppetGroup g = registry.live(tag, e.getUUID());
        if (g == null)
            return;
        died(g, g.member(e.getUUID()), killer == null ? null : killer.getUUID(), gameTime());
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

    int dimId(ResourceKey<Level> dim) {
        return boundary.dictId(Obs.DICT_DIMENSION, dim.location().toString());
    }

    int itemId(Item item) {
        return boundary.dictId(Obs.DICT_ITEM, BuiltInRegistries.ITEM.getKey(item).toString());
    }

    // ---- counters -------------------------------------------------------------------------------

    public long spawnedGroups() { return spawnedGroups; }
    public long rejectedSpawns() { return rejectedSpawns; }
    public long capturedGroups() { return capturedGroups; }
    public long staleDiscarded() { return staleDiscarded; }
    public long takes() { return takes; }
    public long deaths() { return deaths; }
    public long removedByOther() { return removedByOther; }
    public long passNanos() { return passNanos; }
    public long ticks() { return ticks; }
}
