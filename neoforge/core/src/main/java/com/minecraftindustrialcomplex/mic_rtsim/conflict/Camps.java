package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugin;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.mixin.ChunkMapInvoker;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import net.minecraft.core.BlockPos;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ChunkHolder;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.util.Mth;
import net.minecraft.world.Difficulty;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Marker;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.IdentityHashMap;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;

/**
 * The camp reconciler's Java half (MOWZIE.md §4.9, ledgers 16 and 22; slice M5), mod-agnostic (D4): a
 * plugin declares each camp it finds ({@link #declare}) with its {@link CampKind}, and this class follows
 * the camp's footprint chunks in and out, takes its census, plays the engine's CAMP_RECONCILE lines on
 * the jar's own bodies and answers each (CAMP_BODIES), and reports the deaths and takes of the bodies a
 * camp holds (CAMP_DEATH). Owned by {@link ConflictBridge}; server thread only.
 *
 * <p><b>In and out.</b> Each footprint chunk's ticket level is read at declaration and then followed: by
 * a poll, the authority (every {@link #POLL_PERIOD} ticks, each camp in its own slot of them, MZB-290),
 * and by {@code ChunkTicketLevelUpdatedEvent} as an early hint where it fires (vanilla; C2ME's chunk
 * system replaces the method that posts it, MZB-249). At the end of a tick a camp whose chunks moved
 * (or that waits for its entities) is looked at once: a chunk above FULL (33), or one whose entities are
 * not in ({@code areEntitiesLoaded}), is out (CAMP_OUT with the epoch that ended, if it was in); all FULL
 * with their entities in, and out, it comes in: a new epoch (a fresh Java op id), every holder in the
 * footprint journaled again first (MZB-174, MZB-201), FACTION_FOUND at its first census of the session,
 * then CAMP_CENSUS. Decided from the final levels of the tick, as the event fires twice for one rise
 * (MZB-203).
 *
 * <p><b>Records ride on what they change</b> (ledger 22): an answer for a body that is there rides on
 * it, one for a body removed rides on a {@code minecraft:marker} left where it stood, a death's on a
 * marker where it died; each is journaled again when its holder loads, and the journal's next file waits
 * for the holder's entity chunk. An answer with no world change (nothing to do, stale) rides on nothing.
 *
 * <p>With no camp declared it holds nothing and does nothing (D1).
 */
public final class Camps {
    /** The ticket level of a FULL chunk; above it a chunk's entities are out. */
    public static final int FULL = 33;
    /** The level a chunk with no holder is read as (beyond every ticket). */
    public static final int NO_HOLDER = 46;
    /** The longest a held spawn waits (MOWZIE.md §7 test 5; PLACEHOLDER). */
    public static final int HOLD_MAX = 6_000;
    /** The census's body cap (the record's u16 size, MZB-169's twin). */
    public static final int CENSUS_CAP = 1024;
    /**
     * The poll of the footprint levels (MZB-290): a camp's chunks are read every this many ticks, the camp in
     * the slot of its declaration order mod this, so one tick reads a twentieth of the camps. PLACEHOLDER.
     */
    public static final int POLL_PERIOD = 20;

    // ---- a camp ---------------------------------------------------------------------------------

    /** One declared camp: its footprint and where it stands with the engine. */
    public static final class Site {
        public final long faction;
        public final ResourceKey<Level> dimension;
        public final String structure;
        public final CampKind kind;
        /** FACTION_FOUND's block (in the structure start's chunk). */
        public final BlockPos pos;
        /** The den's spot or the grove's centre: where a body is kept home and spawned. */
        public final Vec3 spot;
        /**
         * The footprint: the circle of {@link #radius} round {@link #spot}, and with a {@link #halfHeight} above
         * zero a cylinder (the den's, MZB-230; the grove's has no height bound).
         */
        public final double radius, halfHeight;
        final long[] chunks;
        final int[] levels;
        /** The tick (mod {@link #POLL_PERIOD}) this camp's levels are read in. */
        int pollSlot;
        /** The epoch a census opened; 0 while out. */
        long epoch;
        boolean waiting, recount, reported;
        /** A body of the kind joined beyond the footprint: the next census looks wide for strays (MZB-232). */
        boolean strayWatch;
        final boolean late;
        public int censuses, outs;

        Site(long faction, ResourceKey<Level> dimension, String structure, CampKind kind, BlockPos pos, Vec3 spot, double radius,
             double halfHeight, boolean late) {
            this.faction = faction;
            this.dimension = dimension;
            this.structure = structure;
            this.kind = kind;
            this.pos = pos.immutable();
            this.spot = spot;
            this.radius = radius;
            this.halfHeight = halfHeight;
            this.late = late;
            List<Long> cs = new ArrayList<>();
            for (int cx = Mth.floor(spot.x - radius) >> 4; cx <= Mth.floor(spot.x + radius) >> 4; cx++) {
                for (int cz = Mth.floor(spot.z - radius) >> 4; cz <= Mth.floor(spot.z + radius) >> 4; cz++) {
                    // the chunk's nearest point to the centre is within the circle
                    double nx = Mth.clamp(spot.x, cx * 16.0, cx * 16.0 + 16.0), nz = Mth.clamp(spot.z, cz * 16.0, cz * 16.0 + 16.0);
                    if ((nx - spot.x) * (nx - spot.x) + (nz - spot.z) * (nz - spot.z) <= radius * radius)
                        cs.add(ChunkPos.asLong(cx, cz));
                }
            }
            chunks = new long[cs.size()];
            for (int i = 0; i < chunks.length; i++)
                chunks[i] = cs.get(i);
            levels = new int[chunks.length];
        }

        /** Whether a position is in the footprint. */
        public boolean contains(Vec3 p) {
            double dx = p.x - spot.x, dz = p.z - spot.z;
            return dx * dx + dz * dz <= radius * radius && (halfHeight <= 0 || Math.abs(p.y - spot.y) <= halfHeight);
        }

        /** Whether the camp was found in a world from before the plugin (the structure start's chunk was not new). */
        public boolean late() {
            return late;
        }

        public boolean containsChunk(long c) {
            for (long x : chunks)
                if (x == c)
                    return true;
            return false;
        }

        /** The footprint chunks' whole columns: every entity a census or its holders can be. */
        AABB columns(ServerLevel level) {
            int x0 = Integer.MAX_VALUE, z0 = Integer.MAX_VALUE, x1 = Integer.MIN_VALUE, z1 = Integer.MIN_VALUE;
            for (long c : chunks) {
                x0 = Math.min(x0, ChunkPos.getX(c));
                z0 = Math.min(z0, ChunkPos.getZ(c));
                x1 = Math.max(x1, ChunkPos.getX(c));
                z1 = Math.max(z1, ChunkPos.getZ(c));
            }
            return new AABB(x0 * 16.0, level.getMinBuildHeight(), z0 * 16.0, x1 * 16.0 + 16.0, level.getMaxBuildHeight(), z1 * 16.0 + 16.0);
        }

        public boolean in() {
            return epoch != 0;
        }

        public long epoch() {
            return epoch;
        }

        public boolean waiting() {
            return waiting;
        }

        public List<ChunkPos> footprintChunks() {
            List<ChunkPos> out = new ArrayList<>(chunks.length);
            for (long c : chunks)
                out.add(new ChunkPos(c));
            return out;
        }

        /** The ticket level last seen for each footprint chunk (GameTests). */
        public int[] levels() {
            return levels.clone();
        }
    }

    /** One CAMP_RECONCILE line, as read. */
    record Line(int index, int slot, int generation, String body, int action, int flags, long a, long b) {}

    /** A SPAWN line that waits (a Frostmaw's while a player is near, or the spawn budget). */
    private record Held(Site site, long epoch, long op, Line line, long since) {}

    /** A SET_OUT line, played at the end of the tick unless a materialize hands its body off first. */
    static final class SetOut {
        final Site site;
        final long epoch, op;
        final Line line;
        boolean done;

        SetOut(Site site, long epoch, long op, Line line) {
            this.site = site;
            this.epoch = epoch;
            this.op = op;
            this.line = line;
        }
    }

    /** A tagged body's death, journaled at the end of the tick once certain (or at the next reconcile). */
    private record Dying(LivingEntity body, int by, UUID killer) {}

    private final ConflictBridge bridge;
    private final List<CampKind> kinds = new ArrayList<>();
    private final Map<Long, Site> sites = new LinkedHashMap<>();
    private final Map<ResourceKey<Level>, Long2ObjectOpenHashMap<List<Site>>> byChunk = new HashMap<>();
    /** The loaded bodies each camp holds, by faction (EntityJoinLevelEvent and EntityLeaveLevelEvent). */
    private final Map<Long, Set<Entity>> index = new HashMap<>();
    /** Camps to look at at the end of the tick: their chunks moved, they wait for entities, or recount. */
    private final Set<Site> attention = new LinkedHashSet<>();
    /** The camps by the tick of {@link #POLL_PERIOD} their levels are polled in (MZB-290). */
    private final List<List<Site>> pollSlots = new ArrayList<>(POLL_PERIOD);
    private int declared;
    private final List<Held> held = new ArrayList<>();
    private final List<SetOut> setOuts = new ArrayList<>();
    private final Map<UUID, Dying> dying = new LinkedHashMap<>();
    private boolean off;

    // Counters for logs and the GameTests (this session).
    public long censuses, outs, reconciles, lines, staleLines, spawned, adopted, taken, removed, handedOff, handedBack,
            deaths, carried, markers, heldSpawns, recounts, strayWatches, strays;
    /** Camps polled, and the footprint chunks whose level the poll (or a play-time check) found moved (MZB-290). */
    public long polls, pollChanges;

    /**
     * GameTests only: while the level's game time is below this, {@code ChunkTicketLevelUpdatedEvent} is
     * ignored, as on a server whose chunk system never posts it (C2ME, MZB-249); the poll alone follows the
     * levels. A time, not a switch, so a test that fails leaves it off for a bounded while.
     */
    public volatile long ticketEventsSilentUntil;

    Camps(ConflictBridge bridge) {
        this.bridge = bridge;
        for (int i = 0; i < POLL_PERIOD; i++)
            pollSlots.add(new ArrayList<>(2));
    }

    // ---- lifecycle ------------------------------------------------------------------------------

    /** A session started: every camp is out (the engine voids every line at its start) and is looked at again. */
    void sessionStarted() {
        for (Site c : sites.values()) {
            c.epoch = 0;
            c.reported = false;
            c.waiting = false;
            c.recount = false;
            attention.add(c);
        }
        held.clear();
        setOuts.clear();
        dying.clear();
        off = false;
        censuses = outs = reconciles = lines = staleLines = spawned = adopted = taken = removed = handedOff = handedBack = 0;
        deaths = carried = markers = heldSpawns = recounts = strayWatches = strays = 0;
        polls = pollChanges = 0;
    }

    /** The session ends: deaths seen are journaled; nothing else (the next session starts every camp out). */
    void sessionStopping(long now) {
        settleDeaths(now);
    }

    void sessionFailed() {
        held.clear();
        setOuts.clear();
        dying.clear();
    }

    /** The server stopped: nothing of its world outlives it. */
    void clear() {
        sites.clear();
        for (List<Site> slot : pollSlots)
            slot.clear();
        declared = 0;
        ticketEventsSilentUntil = 0;
        byChunk.clear();
        index.clear();
        attention.clear();
        held.clear();
        setOuts.clear();
        dying.clear();
        off = false;
    }

    /** The conflict module was switched off: each camp that is in goes out (CAMP_OUT), its held lines stale. */
    void switchedOff(long now) {
        if (off)
            return;
        off = true;
        settleDeaths(now);
        for (SetOut so : setOuts)
            if (!so.done)
                answer(so.site, so.epoch, so.op, so.line, ConflictRecords.CampBodies.STALE, null, now);
        setOuts.clear();
        for (Site c : sites.values())
            if (c.in())
                out(c, now);
        held.clear();
    }

    // ---- kinds and camps ------------------------------------------------------------------------

    /** A plugin's camp kind; its bodies' deaths are named by it even before their camp is declared. */
    public void registerKind(CampKind kind) {
        if (!kinds.contains(kind))
            kinds.add(kind);
    }

    /**
     * A plugin found a camp (its structure start's chunk loaded): registered once per server run by its
     * faction key, the start chunk's village id ({@link Obs#villageId}); each footprint chunk's ticket level
     * is read once now. Bodies of the camp already loaded are kept home ({@link CampKind#joined}). The
     * site, or null with no live session (the plugin asks again when its chunk next loads).
     */
    public Site declare(ServerLevel level, ChunkPos start, String structure, CampKind kind, BlockPos pos, Vec3 spot,
                        double radius, double halfHeight, boolean late) {
        RtsimSession s = bridge.session();
        if (s == null || s.failed())
            return null;
        registerKind(kind);
        long key = Obs.villageId(s.dimId(level.dimension()), start.x, start.z);
        Site c = sites.get(key);
        if (c != null)
            return c;
        // FACTION_FOUND's block must be in the start chunk, which keys the faction (MZB-206).
        BlockPos at = new BlockPos(Mth.clamp(pos.getX(), start.getMinBlockX(), start.getMaxBlockX()), pos.getY(),
                Mth.clamp(pos.getZ(), start.getMinBlockZ(), start.getMaxBlockZ()));
        c = new Site(key, level.dimension(), structure, kind, at, spot, radius, halfHeight, late);
        sites.put(key, c);
        ChunkMapInvoker map = (ChunkMapInvoker) level.getChunkSource().chunkMap;
        Long2ObjectOpenHashMap<List<Site>> m = byChunk.computeIfAbsent(level.dimension(), k -> new Long2ObjectOpenHashMap<>());
        for (int i = 0; i < c.chunks.length; i++) {
            ChunkHolder h = map.mic_rtsim$updating(c.chunks[i]);
            c.levels[i] = h == null ? NO_HOLDER : Math.min(h.getTicketLevel(), NO_HOLDER);
            m.computeIfAbsent(c.chunks[i], k -> new ArrayList<>(1)).add(c);
        }
        c.pollSlot = declared++ % POLL_PERIOD;
        pollSlots.get(c.pollSlot).add(c);
        bridge.putCamp(c);
        Set<Entity> loaded = index.get(key);
        if (loaded != null)
            for (Entity e : loaded)
                if (e instanceof Mob mob && e.level().dimension().equals(c.dimension))
                    kind.joined(c, mob);
        attention.add(c);
        return c;
    }

    public Site site(long faction) {
        return sites.get(faction);
    }

    public List<Site> sites() {
        return List.copyOf(sites.values());
    }

    /**
     * ChunkTicketLevelUpdatedEvent: a footprint chunk's level moved; its camps are looked at at the end of the
     * tick. An early hint only (MZB-290): the event is not posted at all where C2ME runs the chunk system
     * (MZB-249), and the poll ({@link #poll}) is what keeps the levels true.
     */
    void onTicket(ServerLevel level, long chunk, int newLevel) {
        if (level.getGameTime() < ticketEventsSilentUntil)
            return;
        Long2ObjectOpenHashMap<List<Site>> m = byChunk.get(level.dimension());
        List<Site> l = m == null ? null : m.get(chunk);
        if (l == null)
            return;
        for (Site c : l) {
            for (int i = 0; i < c.chunks.length; i++)
                if (c.chunks[i] == chunk)
                    c.levels[i] = Math.min(newLevel, NO_HOLDER);
            attention.add(c);
        }
    }

    // ---- the index of the bodies camps hold -----------------------------------------------------

    /** EntityJoinLevelEvent (not cancelled): a body a camp holds is indexed and kept home. */
    void onJoin(Entity e, boolean fromDisk) {
        CampMember m = RtsimAttachments.campMember(e);
        if (m == null) {
            if (!sites.isEmpty())
                strayJoined(e);
            return;
        }
        if (RtsimAttachments.isTagged(e))
            return;
        Site c = sites.get(m.faction());
        // A body that changed dimension is not its camp's to keep: its copy in the other dimension is not indexed (MZB-240).
        if (c != null && !c.dimension.equals(e.level().dimension()))
            return;
        index.computeIfAbsent(m.faction(), k -> Collections.newSetFromMap(new IdentityHashMap<>())).add(e);
        if (c == null)
            return;
        if (e instanceof Mob mob)
            c.kind.joined(c, mob);
        // A body read from disk outside the footprint of a camp that is in: a new census (§4.9).
        if (fromDisk && c.in() && !c.containsChunk(e.chunkPosition().toLong())) {
            c.recount = true;
            recounts++;
            attention.add(c);
        }
    }

    /**
     * An untagged body of a kind that watches for strays joined a level (MZB-232): a camp of that kind within
     * the kind's stray radius with a free slot is to look for it, so its next census lists it (and the engine
     * adopts it into the slot). A camp that is out looks at its next census anyway; one with no free slot
     * leaves the body alone: it is the jar's, outside the books.
     */
    private void strayJoined(Entity e) {
        if (!(e instanceof Mob))
            return;
        for (Site c : sites.values()) {
            double r = c.kind.strayRadius(c);
            if (r <= 0 || !c.dimension.equals(e.level().dimension()) || !c.kind.isBody(e) || !e.isAlive())
                continue;
            if (e.position().distanceToSqr(c.spot) > r * r || freeSlots(c) <= 0)
                continue;
            c.strayWatch = true;
            strayWatches++;
            if (c.in()) {
                c.recount = true;
                attention.add(c);
            }
        }
    }

    /** How many bodies a camp can still take: its kind's slots less the live bodies it holds loaded. */
    private int freeSlots(Site c) {
        int slots = c.kind.slots(c);
        if (slots == Integer.MAX_VALUE)
            return slots;
        int used = 0;
        Set<Entity> s = index.get(c.faction);
        if (s != null)
            for (Entity e : s)
                if (e.isAlive() && !e.isRemoved())
                    used++;
        return Math.max(0, slots - used);
    }

    /**
     * EntityLeaveLevelEvent. A body leaves the index when it is removed (unloaded with its chunk, discarded,
     * changed dimension); one that only went hidden with its chunk, still alive, is kept, since its
     * re-promotion posts no join (MZB-231). A body discarded by anything but the reconciler (which takes the
     * tag off first) is gone from its camp: CAMP_DEATH, killer kind none (§4.9).
     */
    void onLeave(Entity e) {
        CampMember m = RtsimAttachments.campMember(e);
        if (m == null)
            return;
        if (e.getRemovalReason() == null)
            return;
        Set<Entity> s = index.get(m.faction());
        if (s != null) {
            s.remove(e);
            if (s.isEmpty())
                index.remove(m.faction());
        }
        if (e.getRemovalReason() == Entity.RemovalReason.DISCARDED && live() && !RtsimAttachments.isTagged(e))
            death(e, ConflictRecords.BY_NONE, null, bridge.session().gameTime());
    }

    private boolean live() {
        RtsimSession s = bridge.session();
        return s != null && !s.failed() && !off;
    }

    private void indexAdd(Entity e, long faction) {
        index.computeIfAbsent(faction, k -> Collections.newSetFromMap(new IdentityHashMap<>())).add(e);
    }

    /** Takes a body's tag off (out of the index first): it is the camp's no more. */
    private void untag(Entity e) {
        CampMember m = RtsimAttachments.campMember(e);
        if (m != null) {
            Set<Entity> s = index.get(m.faction());
            if (s != null) {
                s.remove(e);
                if (s.isEmpty())
                    index.remove(m.faction());
            }
        }
        e.removeData(RtsimAttachments.CAMP_MEMBER);
    }

    /** The loaded living body a camp holds in a slot and generation, or null. */
    Entity tagged(long faction, int slot, int generation) {
        Set<Entity> s = index.get(faction);
        if (s == null)
            return null;
        Entity found = null;
        for (Iterator<Entity> it = s.iterator(); it.hasNext(); ) {
            Entity e = it.next();
            if (e.isRemoved()) {
                it.remove(); // a leave that was not seen (MZB-231)
                continue;
            }
            CampMember m = RtsimAttachments.campMember(e);
            if (found == null && m != null && m.slot() == slot && m.generation() == generation && e.isAlive())
                found = e;
        }
        if (s.isEmpty())
            index.remove(faction);
        return found;
    }

    /** The loaded bodies a camp holds (GameTests). */
    public List<Entity> held(long faction) {
        Set<Entity> s = index.get(faction);
        return s == null ? List.of() : List.copyOf(s);
    }

    public int heldLines() {
        return held.size();
    }

    // ---- the end of a tick ----------------------------------------------------------------------

    /** Deaths, the tick's set-outs, held spawns, then the camps whose chunks moved. */
    void tick(long now) {
        if (off) {
            off = false;
            attention.addAll(sites.values()); // switched back on: every camp is looked at again
        }
        poll(now);
        settleDeaths(now);
        resolveSetOuts(now);
        if (!held.isEmpty())
            playHeld(now);
        if (attention.isEmpty())
            return;
        Iterator<Site> it = attention.iterator();
        List<Site> again = null;
        while (it.hasNext()) {
            Site c = it.next();
            it.remove();
            if (look(c, now)) {
                if (again == null)
                    again = new ArrayList<>(2);
                again.add(c);
            }
        }
        if (again != null)
            attention.addAll(again);
    }

    /**
     * The levels of this tick's slot of camps, read from the chunk map (MZB-290): a camp whose chunks moved is
     * looked at at the end of this tick, whether or not the event said so. Correct with the event off, and
     * with it on finds nothing the event did not (a change it finds is only a correction). A camp costs one
     * lookup per footprint chunk (5 to 21) every {@link #POLL_PERIOD} ticks, a twentieth of the camps a tick.
     */
    private void poll(long now) {
        List<Site> slot = pollSlots.get((int) Math.floorMod(now, (long) POLL_PERIOD));
        for (int i = 0; i < slot.size(); i++) {
            Site c = slot.get(i);
            ServerLevel level = bridge.level(c.dimension);
            if (level == null)
                continue;
            polls++;
            if (refreshLevels(level, c))
                attention.add(c);
        }
    }

    /**
     * A camp's footprint levels read from the chunk map now, through {@code getUpdatingChunkIfPresent}, the
     * method both vanilla's chunk map and C2ME's answer (C2ME's holder derives {@code getTicketLevel} from
     * its backing holder's target status; no holder is {@link #NO_HOLDER}). Levels past {@link #NO_HOLDER}
     * read as it, so the event's and the poll's agree. True when any moved.
     */
    private boolean refreshLevels(ServerLevel level, Site c) {
        ChunkMapInvoker map = (ChunkMapInvoker) level.getChunkSource().chunkMap;
        boolean moved = false;
        for (int i = 0; i < c.chunks.length; i++) {
            ChunkHolder h = map.mic_rtsim$updating(c.chunks[i]);
            int l = h == null ? NO_HOLDER : Math.min(h.getTicketLevel(), NO_HOLDER);
            if (l != c.levels[i]) {
                c.levels[i] = l;
                moved = true;
                pollChanges++;
            }
        }
        return moved;
    }

    /** One camp: out, waiting, or in. True when it is to be looked at again next tick (it waits for entities). */
    private boolean look(Site c, long now) {
        ServerLevel level = bridge.level(c.dimension);
        if (level == null)
            return false;
        for (int l : c.levels) {
            if (l > FULL) {
                if (c.in())
                    out(c, now);
                c.waiting = false;
                c.recount = false;
                return false;
            }
        }
        for (long ch : c.chunks) {
            if (!level.areEntitiesLoaded(ch)) {
                // Its entities went with a drop below FULL that came back, or are not in yet (§4.9).
                if (c.in())
                    out(c, now);
                c.waiting = true;
                return true;
            }
        }
        c.waiting = false;
        if (!c.in() || c.recount)
            census(level, c, now);
        c.recount = false;
        return false;
    }

    private void out(Site c, long now) {
        RtsimSession s = bridge.session();
        ByteBuffer r = s.boundary().beginJournaled(ConflictRecords.CAMP_OUT, ConflictRecords.CampOut.SIZE, 0, now);
        r.putLong(ConflictRecords.CampOut.FACTION, c.faction);
        r.putLong(ConflictRecords.CampOut.EPOCH, c.epoch);
        s.boundary().commit(r);
        staleHeld(c, now);
        c.epoch = 0;
        c.outs++;
        outs++;
    }

    /** Every line of a camp still waiting, answered stale (its epoch ended). */
    private void staleHeld(Site c, long now) {
        for (Iterator<Held> it = held.iterator(); it.hasNext(); ) {
            Held h = it.next();
            if (h.site() == c) {
                it.remove();
                answer(c, h.epoch(), h.op(), h.line(), ConflictRecords.CampBodies.STALE, null, now);
            }
        }
    }

    // ---- the census -----------------------------------------------------------------------------

    private record CensusBody(int body, int flags, int slot, int generation, UUID uuid) {}

    /**
     * The strays beyond the footprint a census also lists (MZB-232): a late camp's first census of the session,
     * or one a stray's join asked for, takes the loaded untagged bodies of its kind within the kind's stray
     * radius of the spot, nearest first, as many as the camp has free slots. Not the footprint's own (counted
     * already), not another camp's.
     */
    private List<Entity> wideStrays(ServerLevel level, Site c, Set<Entity> insideSet, int strayInside) {
        double r = c.kind.strayRadius(c);
        if (r <= 0 || !(c.strayWatch || c.late && !c.reported))
            return List.of();
        int free = freeSlots(c) - strayInside;
        if (free <= 0)
            return List.of();
        double r2 = r * r;
        List<Entity> found = level.getEntities((Entity) null, new AABB(c.spot, c.spot).inflate(r), e -> {
            if (insideSet.contains(e) || !c.kind.isBody(e) || !e.isAlive() || RtsimAttachments.isTagged(e) || RtsimAttachments.campMember(e) != null)
                return false;
            return e.position().distanceToSqr(c.spot) <= r2;
        });
        if (found.size() > 1)
            found.sort(java.util.Comparator.comparingDouble(e -> e.position().distanceToSqr(c.spot)));
        return found.size() > free ? found.subList(0, free) : found;
    }

    /**
     * A new epoch and its census (CAMP_CENSUS): every holder in the footprint's chunks journals its records
     * first (MZB-174), so an answer of the last epoch reaches the engine while that epoch is still the
     * latest; then the bodies of the camp's kinds in the footprint, tagged or strays, and its tagged bodies
     * loaded elsewhere.
     */
    private void census(ServerLevel level, Site c, long now) {
        RtsimSession s = bridge.session();
        staleHeld(c, now); // a recount ends the epoch its waiting lines belong to
        long epoch = s.boundary().journal().newOpId();
        List<Entity> all = level.getEntities((Entity) null, c.columns(level), e -> true);
        List<Entity> inside = new ArrayList<>();
        Set<Entity> insideSet = Collections.newSetFromMap(new IdentityHashMap<>());
        for (Entity e : all) {
            if (c.contains(e.position())) {
                inside.add(e);
                insideSet.add(e);
            }
        }
        List<CensusBody> bodies = new ArrayList<>();
        int home = 0, strayHome = 0;
        for (Entity e : inside) {
            if (bodies.size() >= CENSUS_CAP)
                break;
            if (!c.kind.isBody(e) || !e.isAlive() || RtsimAttachments.isTagged(e))
                continue;
            CampMember m = RtsimAttachments.campMember(e);
            if (m != null && m.faction() != c.faction)
                continue; // another camp's body that walked in: its own camp counts it elsewhere (MZB-208)
            if (m != null)
                indexAdd(e, c.faction); // a tagged body standing here is indexed whatever its visibility did (MZB-231)
            else
                strayHome++;
            int flags = (m == null ? ConflictRecords.CampCensus.BODY_STRAY : 0)
                    | (c.kind.isChief(e) ? ConflictRecords.CampCensus.BODY_CHIEF : 0);
            bodies.add(new CensusBody(s.dictId(Obs.DICT_BODY, c.kind.bodyName(e)), flags, m == null ? 0 : m.slot(),
                    m == null ? 0 : m.generation(), e.getUUID()));
            home++;
        }
        // What the chief is read from: the footprint's entities, the camp's bodies loaded elsewhere and the strays found wide.
        List<Entity> view = new ArrayList<>(inside);
        Set<Entity> mine = index.get(c.faction);
        if (mine != null) {
            for (Entity e : mine) {
                if (bodies.size() >= CENSUS_CAP)
                    break;
                if (insideSet.contains(e) || !e.isAlive() || e.isRemoved() || !e.level().dimension().equals(c.dimension))
                    continue;
                CampMember m = RtsimAttachments.campMember(e);
                int flags = ConflictRecords.CampCensus.BODY_ELSEWHERE | (c.kind.isChief(e) ? ConflictRecords.CampCensus.BODY_CHIEF : 0);
                bodies.add(new CensusBody(s.dictId(Obs.DICT_BODY, c.kind.bodyName(e)), flags, m.slot(), m.generation(), e.getUUID()));
                view.add(e);
            }
        }
        for (Entity e : wideStrays(level, c, insideSet, strayHome)) {
            if (bodies.size() >= CENSUS_CAP)
                break;
            int flags = ConflictRecords.CampCensus.BODY_STRAY | (c.kind.isChief(e) ? ConflictRecords.CampCensus.BODY_CHIEF : 0);
            bodies.add(new CensusBody(s.dictId(Obs.DICT_BODY, c.kind.bodyName(e)), flags, 0, 0, e.getUUID()));
            view.add(e);
            home++;
            strays++;
        }
        c.strayWatch = false;
        CampKind.Chief chief = c.kind.chief(level, c, view);
        if (!c.reported) {
            int census = ((chief.flags() & ConflictRecords.CampCensus.CHIEF) != 0 ? ConflictRecords.FactionFound.CENSUS_CHIEF : 0)
                    | ((chief.flags() & ConflictRecords.CampCensus.CRYSTAL) != 0 ? ConflictRecords.FactionFound.CENSUS_CRYSTAL : 0);
            bridge.factionFound(c, level, home, census, now);
            c.reported = true;
        }
        // The holders in the footprint's chunks, before the census (MZB-174): each once per session.
        for (Entity e : all)
            if (e.hasData(RtsimAttachments.PENDING))
                s.ensureReasserted(e);
        int n = bodies.size();
        ByteBuffer r = s.boundary().beginJournaled(ConflictRecords.CAMP_CENSUS, ConflictRecords.CampCensus.size(n, 0), n, now);
        r.putLong(ConflictRecords.CampCensus.FACTION, c.faction);
        r.putLong(ConflictRecords.CampCensus.EPOCH, epoch);
        r.putInt(ConflictRecords.CampCensus.FLAGS, chief.flags());
        r.putFloat(ConflictRecords.CampCensus.HEALTH, chief.health());
        r.putDouble(ConflictRecords.CampCensus.THRONE_X, chief.x());
        r.putDouble(ConflictRecords.CampCensus.THRONE_Y, chief.y());
        r.putDouble(ConflictRecords.CampCensus.THRONE_Z, chief.z());
        r.putFloat(ConflictRecords.CampCensus.DIRECTION, chief.direction());
        r.putInt(ConflictRecords.CampCensus.TRADED, 0); // the Umvuthi's trade memory is M9's (MZB-202)
        for (int i = 0; i < n; i++) {
            CensusBody b = bodies.get(i);
            int o = ConflictRecords.CampCensus.BODIES + i * ConflictRecords.CampCensus.BODY;
            r.putInt(o + ConflictRecords.CampCensus.B_BODY, b.body());
            r.putInt(o + ConflictRecords.CampCensus.B_FLAGS, b.flags());
            r.putInt(o + ConflictRecords.CampCensus.B_SLOT, b.slot());
            r.putInt(o + ConflictRecords.CampCensus.B_GENERATION, b.generation());
            r.putLong(o + ConflictRecords.CampCensus.B_UUID_MOST, b.uuid().getMostSignificantBits());
            r.putLong(o + ConflictRecords.CampCensus.B_UUID_LEAST, b.uuid().getLeastSignificantBits());
        }
        s.boundary().commit(r);
        c.epoch = epoch;
        c.censuses++;
        censuses++;
    }

    // ---- CAMP_RECONCILE -------------------------------------------------------------------------

    /**
     * Whether a camp that is in can be played on now (MZB-233): every footprint chunk still at a FULL level
     * and its entities in. The session runs the engine's commands before {@link #tick} looks at the camps,
     * so a camp whose chunks went down in this tick is still "in" when its lines arrive; they are answered
     * stale like a line for a camp that is out.
     */
    private boolean playable(ServerLevel level, Site c) {
        // The levels as they are now, not as the last poll or event left them (MZB-290): a line is played at
        // once, so it cannot wait for the next poll.
        if (refreshLevels(level, c))
            attention.add(c);
        for (int l : c.levels)
            if (l > FULL)
                return false;
        for (long ch : c.chunks)
            if (!level.areEntitiesLoaded(ch))
                return false;
        return true;
    }

    /**
     * CAMP_RECONCILE: each line played on the world now, but a SET_OUT (at the end of the tick, unless a
     * materialize in this hand-over hands its body to a puppet first) and a held SPAWN. A camp that is not
     * in at the command's epoch answers every line stale and changes nothing.
     */
    void reconcile(ByteBuffer b, int at, int size, int n, SessionPlugin.EngineNames names, long now) {
        reconciles++;
        if (n < 0 || ConflictRecords.CampReconcile.size(n) > size) {
            bridge.malformed++;
            return;
        }
        settleDeaths(now); // a body that died this tick is no body to play a line on
        long faction = b.getLong(at + ConflictRecords.CampReconcile.FACTION);
        long epoch = b.getLong(at + ConflictRecords.CampReconcile.EPOCH);
        long op = b.getLong(at + ConflictRecords.CampReconcile.OP_ID);
        Site c = sites.get(faction);
        ServerLevel level = c == null ? null : bridge.level(c.dimension);
        boolean in = c != null && level != null && c.in() && c.epoch == epoch && !off && playable(level, c);
        List<Line> quiet = new ArrayList<>();
        List<Integer> quietOutcomes = new ArrayList<>();
        Map<Long, ConflictBridge.Party> homecoming = new LinkedHashMap<>();
        for (int i = 0; i < n; i++) {
            int o = at + ConflictRecords.CampReconcile.LINES + i * ConflictRecords.CampReconcile.LINE;
            Line ln = new Line(i, b.getInt(o + ConflictRecords.CampReconcile.L_SLOT), b.getInt(o + ConflictRecords.CampReconcile.L_GENERATION),
                    names.name(Obs.DICT_BODY, b.getInt(o + ConflictRecords.CampReconcile.L_BODY)),
                    b.get(o + ConflictRecords.CampReconcile.L_ACTION) & 0xFF, b.get(o + ConflictRecords.CampReconcile.L_FLAGS) & 0xFF,
                    b.getLong(o + ConflictRecords.CampReconcile.L_A), b.getLong(o + ConflictRecords.CampReconcile.L_B));
            lines++;
            int outcome = in ? play(level, c, epoch, op, ln, homecoming, now) : ConflictRecords.CampBodies.STALE;
            if (outcome != 0) {
                quiet.add(ln);
                quietOutcomes.add(outcome);
            }
        }
        if (!quiet.isEmpty())
            answerAll(faction, epoch, op, quiet, quietOutcomes, now);
        // A band handed back is home: what is left of it walks off and goes (no BAND_RECALL comes, MZB-211).
        for (ConflictBridge.Party p : homecoming.values())
            bridge.homeAt(p, c.spot.x, c.spot.z, now);
    }

    /**
     * Plays one line. Returns the outcome of a line answered with no holder (nothing to do, stale), or 0
     * when the line was answered on its holder or is waiting.
     */
    private int play(ServerLevel level, Site c, long epoch, long op, Line ln, Map<Long, ConflictBridge.Party> homecoming, long now) {
        if (ln.body() == null)
            return ConflictRecords.CampBodies.STALE;
        boolean chiefLine = (ln.flags() & ConflictRecords.CampReconcile.LINE_CHIEF) != 0;
        if (chiefLine && !c.kind.chiefLines())
            return ConflictRecords.CampBodies.STALE; // the Umvuthi's lines are M9's (MZB-209)
        return switch (ln.action()) {
            case ConflictRecords.CampReconcile.SPAWN -> spawnLine(level, c, epoch, op, ln, now, true);
            case ConflictRecords.CampReconcile.ADOPT -> adopt(level, c, epoch, op, ln, now);
            case ConflictRecords.CampReconcile.TAKE, ConflictRecords.CampReconcile.REMOVE -> takeLine(level, c, epoch, op, ln, now);
            case ConflictRecords.CampReconcile.SET_OUT -> {
                setOuts.add(new SetOut(c, epoch, op, ln));
                yield 0;
            }
            case ConflictRecords.CampReconcile.HAND_BACK -> handBack(level, c, epoch, op, ln, homecoming, now);
            default -> ConflictRecords.CampBodies.STALE; // SEAT_HEIR is M9's (MZB-209)
        };
    }

    /**
     * SPAWN: nothing to do if the slot's body is here; stale on Peaceful (no new body appears there,
     * MZB-215); held while its kind waits; else made at its place (stale when the kind finds none).
     */
    private int spawnLine(ServerLevel level, Site c, long epoch, long op, Line ln, long now, boolean mayHold) {
        if (tagged(c.faction, ln.slot(), ln.generation()) != null)
            return ConflictRecords.CampBodies.NOTHING;
        if (level.getDifficulty() == Difficulty.PEACEFUL)
            return ConflictRecords.CampBodies.STALE;
        if (mayHold && (c.kind.holdSpawn(level, c, ln.body(), ln.flags()) || !bridge.session().spendSpawnBudget(1))) {
            held.add(new Held(c, epoch, op, ln, now));
            heldSpawns++;
            return 0;
        }
        Vec3 p = c.kind.spawnAt(level, c, ln.body(), ln.flags());
        if (p == null)
            return ConflictRecords.CampBodies.STALE; // no free place this time: the next census orders it again
        Mob m = makeBody(level, c, ln, p.x, p.y, p.z, level.random.nextFloat() * 360f - 180f);
        if (m == null)
            return ConflictRecords.CampBodies.STALE; // the next census orders it again
        spawned++;
        answer(c, epoch, op, ln, ConflictRecords.CampBodies.DONE, m, now);
        return 0;
    }

    /** Makes, tags and adds a body for a line; null when the kind made none or the level refused it. */
    private Mob makeBody(ServerLevel level, Site c, Line ln, double x, double y, double z, float yaw) {
        Mob m;
        try {
            m = c.kind.make(level, c, ln.body(), ln.flags(), x, y, z, yaw);
        } catch (RuntimeException ex) {
            MicRtsim.LOGGER.error("[mic_rtsim] making camp body {} failed", ln.body(), ex);
            m = null;
        }
        if (m == null)
            return null;
        m.setData(RtsimAttachments.CAMP_MEMBER, new CampMember(c.faction, ln.slot(), ln.generation()));
        m.setPersistenceRequired();
        if (!level.addFreshEntity(m))
            return null; // the join event indexed it and kept it home
        return m;
    }

    /** ADOPT: the stray named by UUID becomes the slot's body where it stands. */
    private int adopt(ServerLevel level, Site c, long epoch, long op, Line ln, long now) {
        Entity e = byUuid(level, ln.a(), ln.b());
        if (!(e instanceof Mob mob) || !e.isAlive() || !c.kind.isBody(e) || RtsimAttachments.isTagged(e))
            return ConflictRecords.CampBodies.NOTHING;
        CampMember was = RtsimAttachments.campMember(e);
        if (was != null && was.faction() != c.faction)
            return ConflictRecords.CampBodies.NOTHING;
        if (was != null)
            untag(e);
        // Its persistence stays the jar's (MZB-210).
        e.setData(RtsimAttachments.CAMP_MEMBER, new CampMember(c.faction, ln.slot(), ln.generation()));
        indexAdd(e, c.faction);
        c.kind.joined(c, mob);
        adopted++;
        answer(c, epoch, op, ln, ConflictRecords.CampBodies.DONE, e, now);
        return 0;
    }

    /** TAKE and REMOVE: the body (by UUID if the line names one, else by its tag) goes; its record on a marker. */
    private int takeLine(ServerLevel level, Site c, long epoch, long op, Line ln, long now) {
        Entity e;
        if (ln.a() != 0 || ln.b() != 0) {
            e = byUuid(level, ln.a(), ln.b());
            if (e != null && (!c.kind.isBody(e) || RtsimAttachments.isTagged(e)))
                e = null;
            CampMember m = e == null ? null : RtsimAttachments.campMember(e);
            if (m != null && m.faction() != c.faction)
                e = null;
        } else {
            e = tagged(c.faction, ln.slot(), ln.generation());
        }
        if (e == null || !e.isAlive() || e.isRemoved())
            return ConflictRecords.CampBodies.NOTHING;
        Moved mv = removeWithMarker(level, e);
        if (ln.action() == ConflictRecords.CampReconcile.TAKE)
            taken++;
        else
            removed++;
        answer(c, epoch, op, ln, ConflictRecords.CampBodies.DONE, mv.marker(), mv.records(), now);
        return 0;
    }

    /**
     * Takes a body out of the world, leaving a record marker where it stood; the records it carried ride
     * on the marker with the caller's answer.
     */
    private Moved removeWithMarker(ServerLevel level, Entity e) {
        Marker mk = marker(level, e.position());
        List<byte[]> old = takeRecords(e);
        untag(e);
        e.ejectPassengers();
        e.discard();
        return new Moved(mk, old);
    }

    /** A marker left where a body stood, and the records the body carried, for the marker to carry. */
    private record Moved(Marker marker, List<byte[]> records) {}

    /**
     * The records a holder carries, taken off it: journaled in this session first if they were not yet,
     * so the marker that carries them on carries records the engine has been sent.
     */
    private List<byte[]> takeRecords(Entity e) {
        bridge.session().ensureReasserted(e);
        PendingRecords old = e.getExistingDataOrNull(RtsimAttachments.PENDING);
        e.removeData(RtsimAttachments.PENDING);
        return old == null ? List.of() : old.records();
    }

    /** A record marker at a position (MOWZIE.md §4.9): inert, saved with its entity chunk, never sent to a client. */
    private Marker marker(ServerLevel level, Vec3 at) {
        Marker mk = EntityType.MARKER.create(level);
        if (mk == null)
            throw new IllegalStateException("no marker");
        mk.moveTo(at.x, at.y, at.z, 0f, 0f);
        mk.addTag(RtsimSession.RECORD_MARKER_TAG);
        level.addFreshEntity(mk);
        markers++;
        return mk;
    }

    private static Entity byUuid(ServerLevel level, long most, long least) {
        return most == 0 && least == 0 ? null : level.getEntity(new UUID(most, least));
    }

    /**
     * HAND_BACK: the party's puppet of that body becomes the camp's body where it stands (its state kept),
     * the puppet goes; with no puppet in the world, as a spawn (MZB-213). The party's rest goes home after.
     */
    private int handBack(ServerLevel level, Site c, long epoch, long op, Line ln, Map<Long, ConflictBridge.Party> homecoming, long now) {
        ConflictBridge.Party p = bridge.party(ln.a());
        if (p != null && p.epoch == (int) ln.b()) {
            ConflictBridge.Slot sl = bridge.puppetOf(p, ln.body());
            if (sl != null) {
                if (tagged(c.faction, ln.slot(), ln.generation()) != null) {
                    homecoming.put(p.agent, p);
                    bridge.handBack(sl); // its body is home already: the puppet only goes
                    return ConflictRecords.CampBodies.NOTHING;
                }
                Mob from = sl.entity;
                Mob m;
                try {
                    m = c.kind.make(level, c, ln.body(), ln.flags(), from.getX(), from.getY(), from.getZ(), from.getYRot());
                } catch (RuntimeException ex) {
                    MicRtsim.LOGGER.error("[mic_rtsim] making camp body {} failed", ln.body(), ex);
                    m = null;
                }
                if (m != null) {
                    c.kind.copy(from, m);
                    m.setData(RtsimAttachments.CAMP_MEMBER, new CampMember(c.faction, ln.slot(), ln.generation()));
                    m.setPersistenceRequired();
                    // The body is added before the puppet goes (one tick with both is fine): a join another
                    // mod cancels keeps the puppet and loses no body (MZB-236).
                    if (level.addFreshEntity(m)) {
                        homecoming.put(p.agent, p);
                        bridge.handBack(sl);
                        handedBack++;
                        answer(c, epoch, op, ln, ConflictRecords.CampBodies.DONE, m, now);
                        return 0;
                    }
                }
                return ConflictRecords.CampBodies.STALE; // the puppet stays; the next census orders it again
            }
            homecoming.put(p.agent, p);
        }
        return spawnLine(level, c, epoch, op, ln, now, true);
    }

    // ---- SET_OUT and the hand-off ---------------------------------------------------------------

    /**
     * A materialize places a puppet of {@code body} for a party: the camp body of one of this tick's
     * SET_OUT lines for that party (by agent and epoch), if one is loaded, to make the puppet in its place
     * (MZB-207). Null when none is waiting.
     */
    SetOut claim(long agent, int partyEpoch, String body) {
        for (SetOut so : setOuts) {
            if (so.done || so.line.a() != agent || (int) so.line.b() != partyEpoch || !so.line.body().equals(body))
                continue;
            if (!so.site.in() || so.site.epoch != so.epoch)
                continue;
            ServerLevel lv = bridge.level(so.site.dimension);
            if (lv == null || !playable(lv, so.site))
                continue; // resolved at the end of the tick as stale (MZB-233)
            if (tagged(so.site.faction, so.line.slot(), so.line.generation()) == null)
                continue;
            so.done = true;
            return so;
        }
        return null;
    }

    /** The camp body a claimed line hands off. */
    Mob claimed(SetOut so) {
        return tagged(so.site.faction, so.line.slot(), so.line.generation()) instanceof Mob m ? m : null;
    }

    /** A claimed line not played after all (the materialize failed): back to the end of the tick. */
    void unclaim(SetOut so) {
        so.done = false;
    }

    /** The puppet is in the world: its camp body goes, its record on a marker where it stood. */
    void handedOff(SetOut so, long now) {
        Entity body = tagged(so.site.faction, so.line.slot(), so.line.generation());
        ServerLevel level = bridge.level(so.site.dimension);
        if (body == null || level == null) {
            answer(so.site, so.epoch, so.op, so.line, ConflictRecords.CampBodies.NOTHING, null, now);
            return;
        }
        Moved mv = removeWithMarker(level, body);
        handedOff++;
        answer(so.site, so.epoch, so.op, so.line, ConflictRecords.CampBodies.DONE, mv.marker(), mv.records(), now);
    }

    /**
     * The tick's SET_OUT lines no materialize in this hand-over claimed: the body goes, as a take (§4.9:
     * the party gets fresh puppets when it is seen later); nothing to do with no body.
     */
    private void resolveSetOuts(long now) {
        if (setOuts.isEmpty())
            return;
        List<SetOut> todo = new ArrayList<>(setOuts);
        setOuts.clear();
        for (SetOut so : todo) {
            if (so.done)
                continue;
            so.done = true;
            ServerLevel level = bridge.level(so.site.dimension);
            if (level == null || !so.site.in() || so.site.epoch != so.epoch || !playable(level, so.site)) {
                answer(so.site, so.epoch, so.op, so.line, ConflictRecords.CampBodies.STALE, null, now);
                continue;
            }
            Entity body = tagged(so.site.faction, so.line.slot(), so.line.generation());
            if (body == null) {
                answer(so.site, so.epoch, so.op, so.line, ConflictRecords.CampBodies.NOTHING, null, now);
                continue;
            }
            Moved mv = removeWithMarker(level, body);
            taken++;
            answer(so.site, so.epoch, so.op, so.line, ConflictRecords.CampBodies.DONE, mv.marker(), mv.records(), now);
        }
    }

    /** Held spawns placed once their kind stops waiting, or at {@link #HOLD_MAX}. */
    private void playHeld(long now) {
        List<Held> todo = new ArrayList<>(held);
        held.clear();
        for (Held h : todo) {
            Site c = h.site();
            ServerLevel level = bridge.level(c.dimension);
            if (level == null || !c.in() || c.epoch != h.epoch() || !playable(level, c)) {
                answer(c, h.epoch(), h.op(), h.line(), ConflictRecords.CampBodies.STALE, null, now);
                continue;
            }
            boolean wait = now - h.since() < HOLD_MAX && c.kind.holdSpawn(level, c, h.line().body(), h.line().flags());
            if (wait || !bridge.session().spendSpawnBudget(1)) {
                held.add(h);
                continue;
            }
            int outcome = spawnLine(level, c, h.epoch(), h.op(), h.line(), now, false);
            if (outcome != 0)
                answer(c, h.epoch(), h.op(), h.line(), outcome, null, now);
        }
    }

    // ---- answers --------------------------------------------------------------------------------

    /**
     * One line's CAMP_BODIES: on its holder (the body there, or the marker of the body gone) when it has
     * one, carried until acknowledged and journaled again when the holder loads (ledger 22).
     */
    private void answer(Site c, long epoch, long op, Line ln, int outcome, Entity holder, long now) {
        answer(c, epoch, op, ln, outcome, holder, List.of(), now);
    }

    /** As above, the holder (a marker) also carrying the records of the body it stands for. */
    private void answer(Site c, long epoch, long op, Line ln, int outcome, Entity holder, List<byte[]> before, long now) {
        // What the holder carried from disk is journaled before the new record, so it reaches the engine first (MZB-239).
        if (holder != null)
            bridge.session().ensureReasserted(holder);
        ByteBuffer r = bodiesRecord(c.faction, epoch, op, List.of(ln), List.of(outcome), now);
        if (outcome == ConflictRecords.CampBodies.STALE)
            staleLines++;
        if (holder != null) {
            List<byte[]> records = new ArrayList<>(before.size() + 1);
            records.addAll(before);
            records.add(r.array());
            bridge.session().carryOnCampHolder(holder, records, r.getLong(Obs.OBS_SEQ));
            carried++;
        }
    }

    private void answerAll(long faction, long epoch, long op, List<Line> ls, List<Integer> outcomes, long now) {
        bodiesRecord(faction, epoch, op, ls, outcomes, now);
        for (int o : outcomes)
            if (o == ConflictRecords.CampBodies.STALE)
                staleLines++;
    }

    private ByteBuffer bodiesRecord(long faction, long epoch, long op, List<Line> ls, List<Integer> outcomes, long now) {
        RtsimSession s = bridge.session();
        int n = ls.size();
        ByteBuffer r = s.boundary().beginJournaled(ConflictRecords.CAMP_BODIES, ConflictRecords.CampBodies.size(n), n, now);
        r.putLong(ConflictRecords.CampBodies.FACTION, faction);
        r.putLong(ConflictRecords.CampBodies.EPOCH, epoch);
        r.putLong(ConflictRecords.CampBodies.OP_ID, op);
        for (int i = 0; i < n; i++) {
            Line ln = ls.get(i);
            int o = ConflictRecords.CampBodies.LINES + i * ConflictRecords.CampBodies.LINE;
            r.putInt(o + ConflictRecords.CampBodies.L_LINE, ln.index());
            r.putInt(o + ConflictRecords.CampBodies.L_SLOT, ln.slot());
            r.putInt(o + ConflictRecords.CampBodies.L_GENERATION, ln.generation());
            r.put(o + ConflictRecords.CampBodies.L_ACTION, (byte) ln.action());
            r.put(o + ConflictRecords.CampBodies.L_OUTCOME, (byte) outcomes.get(i).intValue());
        }
        s.boundary().commit(r);
        return r;
    }

    // ---- deaths and takes -----------------------------------------------------------------------

    /** LivingDeathEvent (lowest, not cancelled): a held body's death, journaled once certain. */
    void onDeath(LivingEntity dead, DamageSource source) {
        if (RtsimAttachments.campMember(dead) == null || RtsimAttachments.isTagged(dead))
            return;
        Entity killer = source.getEntity();
        int by = ConflictBridge.byKind(killer);
        dying.putIfAbsent(dead.getUUID(), new Dying(dead, by, by == ConflictRecords.BY_PLAYER ? killer.getUUID() : null));
    }

    void settleDeaths(long now) {
        if (dying.isEmpty())
            return;
        List<Dying> todo = new ArrayList<>(dying.values());
        dying.clear();
        for (Dying d : todo) {
            LivingEntity e = d.body();
            if (!e.isRemoved() && !e.isDeadOrDying())
                continue; // another mod cancelled it
            if (RtsimAttachments.campMember(e) == null)
                continue;
            death(e, d.by(), d.killer(), now);
        }
    }

    /**
     * A Carry On pickup of a body a camp holds: CAMP_DEATH, killer kind taken, by that player; its tag goes
     * before Carry On saves it, so it is put down a stray (§7 test 22). False when no camp holds it.
     */
    public boolean take(Entity e, ServerPlayer player) {
        if (RtsimAttachments.campMember(e) == null || RtsimAttachments.isTagged(e) || !live())
            return false;
        death(e, ConflictRecords.BY_TAKEN, player == null ? null : player.getUUID(), bridge.session().gameTime());
        return true;
    }

    /** CAMP_DEATH (a Java op id) on a marker where the body is, with the records it carried; its tag goes. */
    private void death(Entity e, int by, UUID killer, long now) {
        CampMember m = RtsimAttachments.campMember(e);
        if (m == null || !(e.level() instanceof ServerLevel level))
            return;
        Site c = sites.get(m.faction());
        CampKind kind = c != null ? c.kind : kindOf(e);
        RtsimSession s = bridge.session();
        int body = kind == null ? 0 : s.dictId(Obs.DICT_BODY, kind.bodyName(e));
        boolean chief = kind != null && kind.isChief(e);
        List<byte[]> records = new ArrayList<>(takeRecords(e)); // journaled first if they were not yet
        ByteBuffer r = s.boundary().beginJournaled(ConflictRecords.CAMP_DEATH, ConflictRecords.CampDeath.SIZE, by, now);
        r.putLong(ConflictRecords.CampDeath.FACTION, m.faction());
        r.putInt(ConflictRecords.CampDeath.BODY, body);
        r.putInt(ConflictRecords.CampDeath.FLAGS, chief ? ConflictRecords.CampDeath.FLAG_CHIEF : 0);
        r.putInt(ConflictRecords.CampDeath.SLOT, m.slot());
        r.putInt(ConflictRecords.CampDeath.GENERATION, m.generation());
        if (killer != null) {
            r.putLong(ConflictRecords.CampDeath.KILLER_MOST, killer.getMostSignificantBits());
            r.putLong(ConflictRecords.CampDeath.KILLER_LEAST, killer.getLeastSignificantBits());
        }
        s.boundary().commitWithOpId(r, ConflictRecords.CampDeath.OP_ID);
        Marker mk = marker(level, e.position());
        records.add(r.array());
        untag(e);
        s.carryOnCampHolder(mk, records, r.getLong(Obs.OBS_SEQ));
        deaths++;
    }

    private CampKind kindOf(Entity e) {
        for (CampKind k : kinds)
            if (k.isBody(e))
                return k;
        return null;
    }

    /** Whether an entity is a body a camp holds (Carry On asks before its pickup). */
    public static boolean isHeld(Entity e) {
        return RtsimAttachments.campMember(e) != null && !RtsimAttachments.isTagged(e);
    }

    /** The players within {@code r} blocks of a point (not spectators): a kind's hold asks this. */
    public static boolean playerNear(ServerLevel level, Vec3 at, double r) {
        double r2 = r * r;
        for (Player p : level.players())
            if (!p.isSpectator() && p.distanceToSqr(at) <= r2)
                return true;
        return false;
    }
}
