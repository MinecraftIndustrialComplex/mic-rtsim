package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugin;
import com.minecraftindustrialcomplex.mic_rtsim.body.BodyKinds;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.AgentTag;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Talk;
import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.longs.LongIterator;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.game.ClientboundSoundPacket;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundEvents;
import net.minecraft.sounds.SoundSource;
import net.minecraft.util.Mth;
import net.minecraft.world.Difficulty;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.ai.goal.WrappedGoal;
import net.minecraft.world.entity.ai.targeting.TargetingConditions;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.common.util.FakePlayer;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.function.Predicate;

/**
 * The conflict module's Java half (MOWZIE.md ledger 16 without the camp reconciler; docs/boundary.md §9),
 * played by vanilla bodies (D4, {@link ConflictBodies}): a {@link SessionPlugin} the core registers
 * itself, owning the conflict command range 0x8800 to 0x88FF and agent namespace 0x43. With no faction it
 * holds nothing and does nothing (D1). Server thread only.
 *
 * <p><b>Territories and signals.</b> TERRITORY puts each faction's circle in {@link Territories}; a
 * player's log broken (2), ore mined (3), block placed (5) or a camp's kin killed (4) inside a circle that
 * minds it is counted, batched for {@value #SIGNAL_FLUSH} ticks per (faction, signal, player) and
 * journaled as one TERRITORY_SIGNAL. {@link #reportCamp} sends FACTION_FOUND.
 *
 * <p><b>Parties.</b> ENVOY_MATERIALIZE and BAND_MATERIALIZE embody a party; each stack (a BAND_MATERIALIZE
 * entry; the envoy's whole party) is its own group (ledger 25, MZ-17 (a)): placed only when every chunk its
 * members stand in ticks entities, else pending, never reported NOT_TICKING. Once a tick the bridge reads
 * each chunk a pending stack waits on, and tests a pending stack only when one of them changed or its
 * place moved into another chunk ({@link #pendingTests}). A placed stack goes back to pending only when a
 * member is out of the area and its place is too; a member out of the area whose place still ticks is
 * moved back to it (MZB-125), and a requeued stack comes again no sooner than {@value #REQUEUE_COOLDOWN}
 * ticks later. Members keep their place in a two-abreast stack as its anchor walks
 * (BAND_STEER), through {@link ConflictFollowGoal}. BAND_RECALL despawns each stack out of view or at the
 * timeout, then reports every living member, placed or pending, as a survivor (BAND_DEMATERIALIZED).
 *
 * <p><b>Words.</b> None of its own (Ben, 2026-10-22, MZB-101): no chat, action bar or title. A notice is a
 * sound; an envoy is named "<Culture> Envoy" and says its demand only to the player who clicks it
 * ({@link ConflictTalker}, CL-1); news goes to {@link ConflictNews}.
 */
public final class ConflictBridge implements SessionPlugin {
    public static final ConflictBridge INSTANCE = new ConflictBridge();

    /** BAND_STATE every this many ticks per party (PLACEHOLDER, as the herds'). */
    public static final int STATE_INTERVAL = 100;
    /** A member farther than this from its place is walked back to it (blocks, PLACEHOLDER). */
    public static final double LEASH = 6.0;
    /** A recalled stack with no player this close to a member despawns (blocks, PLACEHOLDER). */
    public static final double OUT_OF_VIEW = 64.0;
    /** BAND_RECALL's timeout when the record gives none (ticks; the engine's default). */
    public static final int RECALL_TIMEOUT = 1200;
    /** A batch of territory signals is journaled this many ticks after its first event (PLACEHOLDER). */
    public static final int SIGNAL_FLUSH = 100;
    /** Kept notices are checked every this many ticks (PLACEHOLDER). */
    public static final int NOTICE_CHECK = 20;
    /** A notice's sound plays this far from the player toward the camp (blocks, PLACEHOLDER). */
    public static final double NOTICE_DISTANCE = 32.0;
    public static final float NOTICE_VOLUME = 4.0f;
    /** A notice plays to a player inside the faction's territory or this close to its position (blocks). */
    public static final int NOTICE_RANGE = 256;
    /** A stack's rows and files (blocks, MZB-106): two abreast, 1.5 a row (MOWZIE.md §4.16). */
    static final double ROW = 1.5, FILE = 1.2;
    /** Blocks between the centres of neighbouring stacks placed at one point (PLACEHOLDER). */
    static final double STACK_SPACING = 4.0;
    /** A stack whose bodies could not be made is tried again after this many ticks. */
    static final int RETRY_FAILED = 100;
    /**
     * A stack taken back to pending (its place stopped ticking), or a member whose body went, comes again
     * no sooner than this many ticks later: no flicker of healed, re-geared bodies at an edge (MZB-125,
     * PLACEHOLDER).
     */
    public static final int REQUEUE_COOLDOWN = 200;
    /** M8: a lurk puppet with no target looks for a player this often (ticks, MZB-362). */
    public static final int LURK_TARGET_CHECK = 20;
    /** M8: the range of that look, and of a puppet's follow range (blocks, MZB-362). */
    public static final double LURK_TARGET_RANGE = 50.0;
    /** M8: a roar at a station is heard by players within this many blocks of it (PLACEHOLDER, MZB-366). */
    public static final double ROAR_RANGE = 64.0;
    /** M8: robbed trains waiting for their station's roar; the oldest is dropped past this (MZB-366). */
    public static final int PENDING_ROARS = 256;
    /** A slot's chunk before it is watched. */
    static final long NO_CHUNK = Long.MIN_VALUE;
    /** M1's records (DIFFICULTY, NOTICE_SHOWN): on since M1's engine half is on main. */
    static boolean m1Records = true;

    /** Whether M1's records are sent (GameTests check NOTICE_SHOWN only then). */
    public static boolean m1Records() {
        return m1Records;
    }

    private ConflictBridge() {}

    // ---- state ----------------------------------------------------------------------------------

    record Death(int by, UUID killer) {}

    /** What an envoy asks: tribute (an item and how many are still owed) or a signal stopped, by a deadline. */
    public static final class Demand {
        public final int flags, signal;
        public final String itemName;
        public final Item item;
        int remaining;
        public final long deadline;

        Demand(int flags, int signal, String itemName, Item item, int remaining, long deadline) {
            this.flags = flags;
            this.signal = signal;
            this.itemName = itemName;
            this.item = item;
            this.remaining = remaining;
            this.deadline = deadline;
        }

        public int remaining() {
            return remaining;
        }
    }

    /** One embodied party: an envoy's or a band's (every wave of one epoch). */
    public static final class Party {
        public final long agent;
        public final int epoch;
        public final ResourceKey<Level> dimension;
        public final long faction;
        public final boolean envoy;
        public final int purpose;
        final int flags;
        final List<Stack> stacks = new ArrayList<>();
        final Set<Integer> waves = new HashSet<>();
        float speed;
        boolean attack;
        UUID targetPlayer;
        boolean recalling;
        long recallDeadline;
        int recallReason;
        long lastState;
        boolean arrivedNews;
        /** The envoy's: its demand while open, whom it is to (null: any player), the guard and the answers sent. */
        Demand demand;
        UUID player;
        boolean guarded, insulted;
        /** The envoy's: players other than its demand's who struck it, each answered once (their own insult, MZB-145). */
        final Set<UUID> struckBy = new HashSet<>();
        /** The envoy's: DEMAND_SETTLED's outcome once its demand is closed, 0 while open (the envoy then says it). */
        int settled;
        /** The envoy's: the good its demand asked for, kept after settlement so a click offering it is answered. */
        Item good;

        Party(long agent, int epoch, ResourceKey<Level> dimension, long faction, boolean envoy, int purpose, int flags) {
            this.agent = agent;
            this.epoch = epoch;
            this.dimension = dimension;
            this.faction = faction;
            this.envoy = envoy;
            this.purpose = purpose;
            this.flags = flags;
        }

        public List<Stack> stacks() {
            return List.copyOf(stacks);
        }

        public Demand demand() {
            return demand;
        }

        public boolean guarded() {
            return guarded;
        }

        public boolean recalling() {
            return recalling;
        }

        public boolean attack() {
            return attack;
        }

        public int settled() {
            return settled;
        }
    }

    public enum StackState { PENDING, PLACED, GONE }

    /** One stack: its own group (ledger 25), a front-centre anchor walking a straight line, its members behind. */
    public static final class Stack {
        public final Party party;
        public final int index;
        public final int wave;
        final List<Slot> slots = new ArrayList<>();
        /** The anchor walks from (ax, az) at t0 to (ex, ez) at t1. */
        double ax, az, ex, ez;
        long t0, t1;
        /** Its place beside the band's target (blocks, x and z). */
        double offX, offZ;
        /** The way it faces (a unit vector). */
        double hx = 1.0, hz = 0.0;
        StackState state = StackState.PENDING;
        boolean dirty = true;
        /** Not before this tick: a pending stack's next placing, a placed stack's refill of a body that went. */
        long retryAt;
        /** The chunks a pending stack waits on (empty while unwatched). */
        final LongOpenHashSet chunks = new LongOpenHashSet(4);
        /** Times placed (GameTests: a stack that walked out and came back is placed twice). */
        int placings;
        /** Times tested while pending, and times its place moved into other chunks (GameTests, ledger 25). */
        int tests, moves;

        Stack(Party party, int index, int wave) {
            this.party = party;
            this.index = index;
            this.wave = wave;
        }

        double[] anchor(long now) {
            double f = walked(now);
            return new double[]{ax + (ex - ax) * f, az + (ez - az) * f};
        }

        /** How far along its line the anchor is at {@code now}, 0 to 1. */
        double walked(long now) {
            return t1 <= t0 ? 1.0 : Mth.clamp((double) (now - t0) / (double) (t1 - t0), 0.0, 1.0);
        }

        /** The anchor walks from where it is now to (tx, tz), arriving at {@code arrive} or at {@code speed}. */
        void walk(double tx, double tz, float speed, long arrive, long now) {
            double[] a = anchor(now);
            ax = a[0];
            az = a[1];
            t0 = now;
            double d = Math.hypot(tx - ax, tz - az);
            if (d > 1e-6) {
                hx = (tx - ax) / d;
                hz = (tz - az) / d;
            }
            if (arrive > now) {
                ex = tx;
                ez = tz;
                t1 = arrive;
            } else if (speed > 1e-4f) {
                ex = tx;
                ez = tz;
                t1 = now + (long) Math.ceil(d / speed);
            } else {
                ex = ax; // no speed and no time: it holds where it is
                ez = az;
                t1 = now;
            }
        }

        public StackState state() {
            return state;
        }

        public int placings() {
            return placings;
        }

        public int tests() {
            return tests;
        }

        public int moves() {
            return moves;
        }

        /** The chunks its living members would stand in now (GameTests). */
        public Set<Long> chunksNow(long now) {
            return chunksOf(this, now);
        }

        public List<Slot> slots() {
            return List.copyOf(slots);
        }

        /** The live bodies of this stack. */
        public List<Mob> bodies() {
            List<Mob> out = new ArrayList<>();
            for (Slot s : slots)
                if (s.alive && s.entity != null && !s.entity.isRemoved())
                    out.add(s.entity);
            return out;
        }

        int alive() {
            int n = 0;
            for (Slot s : slots)
                if (s.alive)
                    n++;
            return n;
        }

        public double[] target() {
            return new double[]{ex, ez};
        }
    }

    /** One member's place in a stack, embodied or not. */
    public static final class Slot {
        public final Stack stack;
        public final boolean chief;
        public final boolean envoy;
        /** The body kind's name (the command's) and its id in Java's dictionary (3). */
        public final String body;
        final int javaBody;
        final EntityType<?> type;
        /** Its stack's units have no crystal on them (STACK_BARE): its body is made bare, and made so again if it is refilled. */
        boolean bare;
        /** Its place behind the anchor: {@code lat} to the right, {@code back} behind (blocks). */
        final double lat, back;
        UUID uuid;
        Mob entity;
        /**
         * The entities that came with its body (a raptor's pack, MZB-428): tagged and held by the bridge, kept
         * in {@link ConflictBridge#extraOf} so that they may join, and gone with the slot's despawn or the
         * party's end. They book nothing and are never saved.
         */
        List<Mob> extras = List.of();
        /** Whether the body was persistence-required before the bridge made it so (a take gives that back). */
        boolean ownPersistence;
        /** The chunk its place was in when its pending stack was last watched ({@link #NO_CHUNK}: none). */
        long chunk = NO_CHUNK;
        boolean alive = true;
        Death pendingDeath;
        List<ItemEntity> drops;
        boolean removing;
        ConflictFollowGoal goal;
        List<WrappedGoal> stripped;
        /** M8: where the lurk holds this body (its place at the spot, on the floor); null while it is not held. */
        Vec3 holdAt;
        /**
         * M8: what the body's {@link Lurker} keeps for it while it holds it (the goals it took out, which reference the
         * body): on the slot, so it goes with the slot, and emptied by {@code releaseLurk} (the second re-check's P2:
         * a map by body in the plugin held every killed puppet and its level for good).
         */
        Lurker.Memo lurkMemo;

        Slot(Stack stack, boolean chief, boolean envoy, String body, int javaBody, EntityType<?> type, double lat, double back) {
            this.stack = stack;
            this.chief = chief;
            this.envoy = envoy;
            this.body = body;
            this.javaBody = javaBody;
            this.type = type;
            this.lat = lat;
            this.back = back;
        }

        double[] placeXZ(long now) {
            double f = stack.walked(now);
            return new double[]{placeX(f), placeZ(f)};
        }

        /** Its place's x and z with the anchor {@code f} along its line (no allocation: these run every tick). */
        double placeX(double f) {
            Stack st = stack;
            return st.ax + (st.ex - st.ax) * f - st.hz * lat - st.hx * back;
        }

        double placeZ(double f) {
            Stack st = stack;
            return st.az + (st.ez - st.az) * f + st.hx * lat - st.hz * back;
        }

        /** The chunk its place is in at {@code now}. */
        long placeChunk(long now) {
            double f = stack.walked(now);
            return ChunkPos.asLong(Mth.floor(placeX(f)) >> 4, Mth.floor(placeZ(f)) >> 4);
        }

        /** Whether its place ticks entities now. */
        boolean placeTicks(ServerLevel level, long now, BlockPos.MutableBlockPos at) {
            double f = stack.walked(now);
            return RtsimSession.entitiesTickAt(level, at.set(Mth.floor(placeX(f)), level.getSeaLevel(), Mth.floor(placeZ(f))));
        }

        /** Its place at the mob's level's game time, on the ground where loaded; a held lurk puppet's is its hold point (M8). */
        Vec3 place(Mob m) {
            if (holdAt != null)
                return holdAt;
            double f = stack.walked(m.level().getGameTime());
            double x = placeX(f), z = placeZ(f);
            double y = m.getY();
            int bx = Mth.floor(x), bz = Mth.floor(z);
            if (m.level().hasChunk(bx >> 4, bz >> 4))
                y = m.level().getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, bx, bz);
            return new Vec3(x, y, z);
        }

        /** Horizontal blocks between the mob and its place (flat, MZB-104). */
        double lag(Mob m) {
            if (holdAt != null)
                return Math.hypot(m.getX() - holdAt.x, m.getZ() - holdAt.z);
            double f = stack.walked(m.level().getGameTime());
            return Math.hypot(m.getX() - placeX(f), m.getZ() - placeZ(f));
        }

        public Mob entity() {
            return entity;
        }

        /** The entities that came with its body (GameTests): a raptor's followers; empty for any other. */
        public List<Mob> extras() {
            return extras;
        }

        public boolean alive() {
            return alive;
        }

        /** Blocks between its body and its place now; NaN with no body (GameTests). */
        public double lag() {
            return entity == null ? Double.NaN : lag(entity);
        }
    }

    /** A camp a plugin reported: what counts as its kin and the sound its notices make. */
    record Camp(long faction, ResourceKey<Level> dimension, BlockPos pos, String structure, Predicate<Entity> kin,
                Holder<SoundEvent> sound) {}

    /** A notice kept until its player is in range ((faction, player); player null for the settlers). */
    record Notice(long faction, UUID player, ResourceKey<Level> dimension, int signal, int x, int z, int step,
                  float grievance, int serial) {}

    private record NoticeKey(long faction, UUID player) {}

    private record SignalKey(long faction, int signal, UUID player, ResourceKey<Level> dimension, long territory) {}

    private static final class SignalBatch {
        int count;
        final BlockPos first;
        final long since;

        SignalBatch(BlockPos first, long since) {
            this.first = first;
            this.since = since;
        }
    }

    /** A chunk a pending stack waits on, read once a tick. */
    private static final class Watch {
        boolean ticking;
        final Set<Stack> stacks = new HashSet<>(2);
    }

    /** One position reused by the tick's chunk reads (server thread only). */
    private final BlockPos.MutableBlockPos probe = new BlockPos.MutableBlockPos();
    /** The parties of one pass, reused each tick. */
    private final List<Party> passing = new ArrayList<>();

    private RtsimSession session;
    private final Map<Long, Party> byAgent = new LinkedHashMap<>();
    private final Map<UUID, Slot> byUuid = new HashMap<>();
    /** The slot each extra entity came with (MZB-428), so that it may join the level. */
    private final Map<UUID, Slot> extraOf = new HashMap<>();
    private final Territories territories = new Territories();
    /**
     * The camp kin whose killing signal was counted in the tick {@link #killedAt}, by UUID (P2-6): the jar's Umvuthi
     * posts LivingDeathEvent twice for one death, and one kill is one signal. Reset when the tick moves on.
     */
    private final Set<UUID> killedThisTick = new HashSet<>();
    private long killedAt = Long.MIN_VALUE;
    /** Kept across sessions: a plugin reports a camp when it finds it. Cleared when the server stops. */
    private final Map<Long, Camp> camps = new HashMap<>();
    private final Map<NoticeKey, Notice> notices = new LinkedHashMap<>();
    private final Map<SignalKey, SignalBatch> signals = new LinkedHashMap<>();
    private final Map<ResourceKey<Level>, Long2ObjectOpenHashMap<Watch>> watches = new HashMap<>();
    private Difficulty difficulty;
    private boolean held;
    /** The camp reconciler (M5): the camps plugins declare, their censuses and reconciles. */
    private final Camps reconciler = new Camps(this);
    /** The animals raids on unwatched districts owe (M9c): held until their district loads. */
    private final OwedHunts owedHunts = new OwedHunts();
    /**
     * M8 (MZB-359): the lurk table, by agent (never by epoch), in the order the engine said them. Per session: it
     * is empty at a session's start and end, and the engine says each lurk once more after a start (MZB-353).
     */
    private final Map<Long, Lurk> lurks = new LinkedHashMap<>();
    private final Map<Long, Lurk> lurksView = Collections.unmodifiableMap(lurks);
    /** M8 (MZB-366): trains a lurk robbed whose station has not roared yet, train to faction, oldest first. */
    private final Map<Long, Long> pendingRoars = new LinkedHashMap<>();
    /**
     * M10 (MZB-468): the march the engine last said for each party agent, whether or not the party is embodied.
     * A record replaces its agent's path only with a newer revision; nothing here is saved (UR-1). Per session.
     */
    private final MarchTable marches = new MarchTable();

    // Counters for logs and the GameTests (this session).
    public long materialized, rejected, placed, requeued, pendingTests, watchFlips, placeMoves, casualties, chiefsKilled,
            taken, recalled, dematerialized, steered, tributes, answers, signalsCounted, signalsJournaled, noticesPlayed,
            territoryCommands, settledCommands, campsReported, looted, malformed, unknownCommands, blockedHits, blockedTargets,
            strays, refills, demandsShown, paymentsRefused, noticeHearers, railPasses;
    /** M8's (this session): AMBUSH_SPOTs kept and removed, TRAIN_PASSEDs written, strikes started, roars told and heard. */
    public long lurksKept, lurksRemoved, trainPasses, lurkStrikes, roarsTold, roarsPlayed, roarHearers, lurksRefused;
    /**
     * M10's (this session): BAND_MARCH records kept, dropped as older than the held one or as an ended agent's, and
     * ended; stacks cut by the trim, walked into the square and clamped on it (MZB-472, MZB-473).
     */
    public long marchRecords, marchStale, marchEnds, trimmed, walkIns, clamps;
    /** The last DEMAND_SETTLED's outcome (GameTests). */
    public int lastSettled;

    public RtsimSession session() {
        return session;
    }

    /** The camp reconciler (GameTests and the core's hooks). */
    public Camps reconciler() {
        return reconciler;
    }

    /** The owed hunts (GameTests). */
    public OwedHunts owedHunts() {
        return owedHunts;
    }

    // ---- SessionPlugin: lifecycle ---------------------------------------------------------------

    @Override
    public String name() {
        return "mic_rtsim:conflict";
    }

    @Override
    public void sessionStarted(RtsimSession s) {
        for (Party p : List.copyOf(byAgent.values()))
            discardAll(p);
        session = s;
        byAgent.clear();
        byUuid.clear();
        extraOf.clear();
        territories.clear();
        notices.clear();
        signals.clear();
        watches.clear();
        held = false;
        materialized = rejected = placed = requeued = pendingTests = watchFlips = placeMoves = casualties = chiefsKilled = 0;
        taken = recalled = dematerialized = steered = tributes = answers = signalsCounted = signalsJournaled = 0;
        noticesPlayed = territoryCommands = settledCommands = campsReported = looted = malformed = unknownCommands = 0;
        blockedHits = blockedTargets = strays = refills = demandsShown = paymentsRefused = noticeHearers = railPasses = 0;
        lurksKept = lurksRemoved = trainPasses = lurkStrikes = roarsTold = roarsPlayed = roarHearers = lurksRefused = 0;
        marchRecords = marchStale = marchEnds = trimmed = walkIns = clamps = 0;
        lastSettled = 0;
        owedHunts.clear();
        clearLurks();
        clearMarches();
        difficulty = s.server().getWorldData().getDifficulty();
        sendDifficulty(s.gameTime());
        reconciler.sessionStarted();
    }

    @Override
    public void sessionStopping(RtsimSession s) {
        if (s != session)
            return;
        long now = s.gameTime();
        settleDeaths(now);
        reconciler.sessionStopping(now);
        flushSignals(now, true);
        for (Party p : List.copyOf(byAgent.values()))
            capture(p, ConflictRecords.Dematerialized.SESSION_END, now);
        clearLurks();
        clearMarches();
    }

    @Override
    public void sessionFailed(RtsimSession s) {
        if (s != session)
            return;
        for (Party p : List.copyOf(byAgent.values()))
            discardAll(p);
        byAgent.clear();
        byUuid.clear();
        extraOf.clear();
        watches.clear();
        signals.clear();
        notices.clear();
        owedHunts.clear();
        clearLurks();
        clearMarches();
        reconciler.sessionFailed();
    }

    /**
     * The server stopped: nothing of its world outlives it. Camps, territories, kept notices, signal
     * batches and watches go, and the session with them (its parties went back at its end).
     */
    void serverStopped() {
        camps.clear();
        reconciler.clear();
        owedHunts.clear();
        byAgent.clear();
        byUuid.clear();
        extraOf.clear();
        territories.clear();
        notices.clear();
        signals.clear();
        watches.clear();
        clearLurks();
        clearMarches();
        held = false;
        difficulty = null;
        session = null;
    }

    /** The lurk table and the pending roars are per session: empty them (the puppets were despawned or are going). */
    private void clearLurks() {
        lurks.clear();
        pendingRoars.clear();
    }

    /** The march map is per session (MZB-468): the engine says each march again when it changes. */
    private void clearMarches() {
        marches.clear();
    }

    /** Removes a party's bodies and forgets it, journaling nothing (the session failed). */
    private void discardAll(Party p) {
        for (Stack st : p.stacks)
            for (Slot sl : st.slots)
                despawn(sl);
        unregister(p);
    }

    // ---- SessionPlugin: puppets -----------------------------------------------------------------

    @Override
    public boolean ownsAgent(long agent) {
        return ConflictRecords.isConflictAgent(agent);
    }

    /** Only the exact live body the bridge made joins: a copy (a dimension change, a reload) is refused. */
    @Override
    public boolean admit(RtsimSession s, Entity e, AgentTag tag, Level level) {
        if (s != session)
            return false;
        Slot sl = byUuid.get(e.getUUID());
        if (sl != null)
            return sl.entity == e && sl.alive && sl.stack.party.agent == tag.agent()
                    && sl.stack.party.epoch == tag.epoch() && byAgent.get(tag.agent()) == sl.stack.party;
        // A body's extra (a raptor's follower, MZB-428): the exact object the bridge made for a live party's slot.
        sl = extraOf.get(e.getUUID());
        return sl != null && sl.extras.contains(e) && sl.stack.party.agent == tag.agent()
                && sl.stack.party.epoch == tag.epoch() && byAgent.get(tag.agent()) == sl.stack.party;
    }

    @Override
    public void left(RtsimSession s, Entity e, AgentTag tag) {
        if (s != session)
            return;
        Slot sl = live(e);
        if (sl == null || sl.removing || !sl.alive)
            return;
        Entity.RemovalReason reason = e.getRemovalReason();
        if (reason == Entity.RemovalReason.KILLED) {
            if (sl.pendingDeath == null)
                sl.pendingDeath = new Death(ConflictRecords.BY_NONE, null);
            return;
        }
        if (reason == Entity.RemovalReason.DISCARDED) {
            if (e.level().getDifficulty() == Difficulty.PEACEFUL && ConflictBodies.hostile(e.getType()))
                return; // the server's Peaceful removal: the pass takes the stack back to pending
            // Anything but the bridge (a command, another mod): the member is gone, no one's kill.
            MicRtsim.LOGGER.info("[mic_rtsim] conflict member {} of {} was removed by something else", e,
                    Long.toHexString(sl.stack.party.agent));
            gone(sl, ConflictRecords.BY_OTHER, null, s.gameTime(), false);
            return;
        }
        // Unloaded or changed dimension: the pass makes it again at its place after a cooldown, or takes
        // its stack back to pending if the place no longer ticks (MZB-125).
    }

    @Override
    public boolean take(RtsimSession s, Entity e, AgentTag tag, int how, UUID player) {
        if (s != session)
            return false;
        Slot sl = live(e);
        if (sl == null || !sl.alive || sl.stack.party.agent != tag.agent() || sl.stack.party.epoch != tag.epoch())
            return false;
        // A player's take (Carry On) is a casualty at that player's hands; anything else's is no one's.
        int by = player != null ? ConflictRecords.BY_PLAYER : ConflictRecords.BY_OTHER;
        gone(sl, by, player, s.gameTime(), true);
        return true;
    }

    // ---- SessionPlugin: commands ----------------------------------------------------------------

    @Override
    public boolean ownsCommand(int kind) {
        return ConflictRecords.isConflictCommand(kind);
    }

    @Override
    public boolean accept(RtsimSession s, ByteBuffer b, int at, int kind, int size, int aux, EngineNames names) {
        if (s != session)
            sessionStarted(s);
        if (size < ConflictRecords.minSize(kind)) {
            malformed++;
            MicRtsim.LOGGER.warn("[mic_rtsim] conflict command 0x{} of {} bytes is shorter than its layout; skipped",
                    Integer.toHexString(kind), size);
            return true;
        }
        long now = s.gameTime();
        switch (kind) {
            case ConflictRecords.TERRITORY -> territory(b, at, aux, names);
            case ConflictRecords.FACTION_NOTICE -> notice(b, at, aux, names);
            case ConflictRecords.ENVOY_MATERIALIZE -> envoy(b, at, size, aux, names, now);
            case ConflictRecords.BAND_MATERIALIZE -> materialize(b, at, size, aux, names, now);
            case ConflictRecords.BAND_STEER -> steer(b, at, aux, now);
            case ConflictRecords.BAND_RECALL -> recall(b, at, aux, now);
            case ConflictRecords.DEMAND_SETTLED -> settled(b, at, aux);
            case ConflictRecords.AMBUSH_SPOT -> ambushSpot(b, at, aux, names, now);
            case ConflictRecords.BAND_MARCH -> march(b, at, size, aux, names);
            case ConflictRecords.TRAIN_ROBBED -> trainRobbed(b, at);
            case ConflictRecords.CAMP_RECONCILE -> {
                if (RtsimConfig.on(Module.CONFLICT))
                    reconciler.reconcile(b, at, size, aux, names, now);
            }
            case ConflictRecords.CHIEF_BODY -> {
                if (RtsimConfig.on(Module.CONFLICT))
                    reconciler.chiefBody(b, at, size, aux);
            }
            case ConflictRecords.OWED_HUNT -> {
                if (RtsimConfig.on(Module.CONFLICT))
                    owedHunts.owed(b, at, names);
            }
            default -> unknownCommands++; // a later slice's command (MOWZIE.md §9): nothing here plays it
        }
        return true;
    }

    private ServerLevel level(EngineNames names, int dimId) {
        String name = names.name(Obs.DICT_DIMENSION, dimId);
        ResourceLocation loc = name == null ? null : ResourceLocation.tryParse(name);
        return loc == null ? null : session.server().getLevel(ResourceKey.create(Registries.DIMENSION, loc));
    }

    ServerLevel level(ResourceKey<Level> dim) {
        return session == null ? null : session.server().getLevel(dim);
    }

    private static UUID uuid(long most, long least) {
        return most == 0 && least == 0 ? null : new UUID(most, least);
    }

    /** TERRITORY: a faction's circle added, moved or removed. */
    private void territory(ByteBuffer b, int at, int flags, EngineNames names) {
        territoryCommands++;
        long faction = b.getLong(at + ConflictRecords.Territory.FACTION);
        if ((flags & ConflictRecords.Territory.FLAG_REMOVED) != 0) {
            territories.remove(faction);
            return;
        }
        ServerLevel level = level(names, b.getInt(at + ConflictRecords.Territory.DIM));
        if (level == null) {
            malformed++;
            return;
        }
        territories.put(new Territories.Territory(faction, level.dimension(), b.getInt(at + ConflictRecords.Territory.MASK),
                b.getInt(at + ConflictRecords.Territory.X), b.getInt(at + ConflictRecords.Territory.Z),
                b.getInt(at + ConflictRecords.Territory.RADIUS)));
    }

    /** FACTION_NOTICE: kept until its player is in range, then played as a sound. */
    private void notice(ByteBuffer b, int at, int step, EngineNames names) {
        ServerLevel level = level(names, b.getInt(at + ConflictRecords.Notice.DIM));
        if (level == null) {
            malformed++;
            return;
        }
        long faction = b.getLong(at + ConflictRecords.Notice.FACTION);
        UUID player = uuid(b.getLong(at + ConflictRecords.Notice.PLAYER_MOST), b.getLong(at + ConflictRecords.Notice.PLAYER_LEAST));
        notices.put(new NoticeKey(faction, player), new Notice(faction, player, level.dimension(),
                b.getInt(at + ConflictRecords.Notice.SIGNAL), b.getInt(at + ConflictRecords.Notice.X),
                b.getInt(at + ConflictRecords.Notice.Z), step, b.getFloat(at + ConflictRecords.Notice.GRIEVANCE),
                m1Records ? b.getInt(at + ConflictRecords.Notice.SERIAL) : 0));
    }

    /** One body kind of a materialize command, resolved. */
    private record Kind(String body, int javaBody, EntityType<?> type) {}

    private Kind kind(EngineNames names, int id) {
        String body = names.name(Obs.DICT_BODY, id);
        EntityType<?> type = body == null ? null : ConflictBodies.type(session.server().overworld(), body);
        if (type == null)
            return null;
        ConflictTalker.ensure(type);
        return new Kind(body, session.dictId(Obs.DICT_BODY, body), type);
    }

    /**
     * ENVOY_MATERIALIZE: the envoy and its guards as one stack at the record's position, the first member
     * the envoy (named "<Culture> Envoy", MZB-101); guarded (§4.5) until a player strikes one of them.
     */
    private void envoy(ByteBuffer b, int at, int size, int m, EngineNames names, long now) {
        long cmdSeq = b.getLong(at + Cmd.CMD_SEQ);
        long agent = b.getLong(at + ConflictRecords.Envoy.AGENT);
        int epoch = b.getInt(at + ConflictRecords.Envoy.EPOCH);
        if (!ConflictRecords.isConflictAgent(agent) || m <= 0 || ConflictRecords.Envoy.size(m) > size) {
            reject(agent, epoch, 0, cmdSeq, ConflictRecords.SpawnRejected.BAD_RECORD, now);
            return;
        }
        List<Kind> kinds = new ArrayList<>();
        List<Integer> counts = new ArrayList<>();
        for (int i = 0; i < m; i++) {
            int o = at + ConflictRecords.Envoy.MEMBERS + i * ConflictRecords.Envoy.MEMBER;
            int count = b.getInt(o + ConflictRecords.Envoy.M_COUNT);
            if (count < 0 || count > 256) {
                reject(agent, epoch, 0, cmdSeq, ConflictRecords.SpawnRejected.BAD_RECORD, now);
                return;
            }
            Kind k = kind(names, b.getInt(o + ConflictRecords.Envoy.M_BODY));
            if (k == null) {
                reject(agent, epoch, 0, cmdSeq, ConflictRecords.SpawnRejected.UNKNOWN_BODY, now);
                return;
            }
            kinds.add(k);
            counts.add(count);
        }
        ServerLevel level = level(names, b.getInt(at + ConflictRecords.Envoy.DIM));
        if (level == null) {
            reject(agent, epoch, 0, cmdSeq, ConflictRecords.SpawnRejected.UNKNOWN_DIMENSION, now);
            return;
        }
        Party old = byAgent.get(agent);
        if (old != null && old.epoch == epoch) {
            reject(agent, epoch, 0, cmdSeq, ConflictRecords.SpawnRejected.ALREADY_EMBODIED, now);
            return;
        }
        if (old != null)
            capture(old, ConflictRecords.Dematerialized.COMMAND, now);
        long faction = b.getLong(at + ConflictRecords.Envoy.FACTION);
        Party p = new Party(agent, epoch, level.dimension(), faction, true, ConflictRecords.PURPOSE_ENVOY, 0);
        p.waves.add(0);
        p.player = uuid(b.getLong(at + ConflictRecords.Envoy.PLAYER_MOST), b.getLong(at + ConflictRecords.Envoy.PLAYER_LEAST));
        p.guarded = true;
        int itemId = b.getInt(at + ConflictRecords.Envoy.ITEM);
        String itemName = itemId == 0 ? null : names.name(Obs.DICT_ITEM, itemId);
        ResourceLocation itemLoc = itemName == null ? null : ResourceLocation.tryParse(itemName);
        Item item = itemLoc == null ? null : BuiltInRegistries.ITEM.getOptional(itemLoc).orElse(null);
        p.demand = new Demand(b.getInt(at + ConflictRecords.Envoy.DEMAND), b.getInt(at + ConflictRecords.Envoy.SIGNAL), itemName,
                item, Math.max(0, b.getInt(at + ConflictRecords.Envoy.COUNT)), b.getLong(at + ConflictRecords.Envoy.DEADLINE));
        double x = b.getDouble(at + ConflictRecords.Envoy.X), z = b.getDouble(at + ConflictRecords.Envoy.Z);
        Stack st = new Stack(p, 0, 0);
        st.ax = st.ex = x;
        st.az = st.ez = z;
        st.t0 = st.t1 = now;
        ServerPlayer facing = p.player == null ? null : playerIn(level, p.player);
        if (facing != null) {
            double d = Math.hypot(facing.getX() - x, facing.getZ() - z);
            if (d > 1e-6) {
                st.hx = (facing.getX() - x) / d;
                st.hz = (facing.getZ() - z) / d;
            }
        }
        // The envoy at the front, its guards two abreast behind it.
        int j = 0;
        for (int i = 0; i < kinds.size(); i++) {
            Kind k = kinds.get(i);
            for (int c = 0; c < counts.get(i); c++, j++) {
                if (j == 0) {
                    st.slots.add(new Slot(st, false, true, k.body(), k.javaBody(), k.type(), 0.0, 0.0));
                } else {
                    int g = j - 1;
                    st.slots.add(new Slot(st, false, false, k.body(), k.javaBody(), k.type(), lateral(g, Integer.MAX_VALUE),
                            ROW * (1 + g / 2)));
                }
            }
        }
        if (st.slots.isEmpty()) {
            reject(agent, epoch, 0, cmdSeq, ConflictRecords.SpawnRejected.BAD_RECORD, now);
            return;
        }
        p.stacks.add(st);
        p.lastState = now;
        byAgent.put(agent, p);
        int n = tryPlace(st, level, now) ? st.slots.size() : 0;
        journalMaterialized(p, 0, 0, n, now);
        ConflictNews.post(ConflictNews.Event.DEMAND_MADE, faction, p.player);
    }

    /** The lateral place of the g-th member of a two-abreast stack of {@code n} (blocks right of centre). */
    private static double lateral(int g, int n) {
        boolean lastAlone = n != Integer.MAX_VALUE && n % 2 == 1 && g == n - 1;
        if (n == 1 || lastAlone)
            return 0.0;
        return g % 2 == 0 ? -FILE / 2 : FILE / 2;
    }

    /**
     * BAND_MATERIALIZE: one wave. Each stack entry is a stack, placed on the arc (bearing ± half width at
     * the arc radius from its centre; at the centre itself, side by side, when the radius is 0) and
     * walking to its place beside the target. Stacks whose chunks tick and that fit the spawn quota are
     * placed now, the rest are pending. Answered BAND_MATERIALIZED (aux: members placed now), never
     * NOT_TICKING; a wave the bridge has already had is ALREADY_EMBODIED, and a new epoch ends the old one
     * (BAND_DEMATERIALIZED, reason 1, its survivors).
     */
    private void materialize(ByteBuffer b, int at, int size, int m, EngineNames names, long now) {
        long cmdSeq = b.getLong(at + Cmd.CMD_SEQ);
        long agent = b.getLong(at + ConflictRecords.Materialize.AGENT);
        int epoch = b.getInt(at + ConflictRecords.Materialize.EPOCH);
        int wave = b.getShort(at + ConflictRecords.Materialize.WAVE) & 0xFFFF;
        if (!ConflictRecords.isConflictAgent(agent) || m <= 0 || ConflictRecords.Materialize.size(m) > size) {
            reject(agent, epoch, wave, cmdSeq, ConflictRecords.SpawnRejected.BAD_RECORD, now);
            return;
        }
        List<Kind> kinds = new ArrayList<>();
        int[] counts = new int[m], sflags = new int[m];
        for (int i = 0; i < m; i++) {
            int o = at + ConflictRecords.Materialize.STACKS + i * ConflictRecords.Materialize.STACK;
            counts[i] = b.getInt(o + ConflictRecords.Materialize.S_COUNT);
            sflags[i] = b.getInt(o + ConflictRecords.Materialize.S_FLAGS);
            if (counts[i] < 0 || counts[i] > 256) {
                reject(agent, epoch, wave, cmdSeq, ConflictRecords.SpawnRejected.BAD_RECORD, now);
                return;
            }
            Kind k = kind(names, b.getInt(o + ConflictRecords.Materialize.S_BODY));
            if (k == null) {
                reject(agent, epoch, wave, cmdSeq, ConflictRecords.SpawnRejected.UNKNOWN_BODY, now);
                return;
            }
            kinds.add(k);
        }
        ServerLevel level = level(names, b.getInt(at + ConflictRecords.Materialize.DIM));
        if (level == null) {
            reject(agent, epoch, wave, cmdSeq, ConflictRecords.SpawnRejected.UNKNOWN_DIMENSION, now);
            return;
        }
        Party p = byAgent.get(agent);
        if (p != null && p.epoch == epoch && (p.waves.contains(wave) || p.recalling || !p.dimension.equals(level.dimension()))) {
            reject(agent, epoch, wave, cmdSeq, ConflictRecords.SpawnRejected.ALREADY_EMBODIED, now);
            return;
        }
        if (p != null && p.epoch != epoch) {
            capture(p, ConflictRecords.Dematerialized.COMMAND, now);
            p = null;
        }
        int pflags = b.get(at + ConflictRecords.Materialize.FLAGS) & 0xFF;
        if (p == null) {
            p = new Party(agent, epoch, level.dimension(), b.getLong(at + ConflictRecords.Materialize.FACTION), false,
                    b.get(at + ConflictRecords.Materialize.PURPOSE) & 0xFF, pflags);
            p.lastState = now;
            byAgent.put(agent, p);
        }
        p.waves.add(wave);
        p.speed = b.getFloat(at + ConflictRecords.Materialize.SPEED);
        p.targetPlayer = uuid(b.getLong(at + ConflictRecords.Materialize.TARGET_PLAYER_MOST),
                b.getLong(at + ConflictRecords.Materialize.TARGET_PLAYER_LEAST));
        double tx = b.getDouble(at + ConflictRecords.Materialize.TARGET_X), tz = b.getDouble(at + ConflictRecords.Materialize.TARGET_Z);
        double cx = b.getDouble(at + ConflictRecords.Materialize.ARC_X), cz = b.getDouble(at + ConflictRecords.Materialize.ARC_Z);
        long radius = b.getInt(at + ConflictRecords.Materialize.ARC_RADIUS) & 0xFFFF_FFFFL;
        double bearing = b.getFloat(at + ConflictRecords.Materialize.BEARING);
        double half = b.getFloat(at + ConflictRecords.Materialize.HALF_WIDTH);
        if (!Double.isFinite(bearing))
            bearing = 0;
        if (!Double.isFinite(half))
            half = 0;
        // Where each stack appears.
        double[][] at0 = new double[m][];
        for (int i = 0; i < m; i++) {
            if (radius == 0) {
                at0[i] = new double[]{cx, cz};
            } else {
                double f = m == 1 ? 0.0 : 2.0 * (i + 0.5) / m - 1.0;
                double a = Math.toRadians(bearing + half * f);
                at0[i] = new double[]{cx + radius * Math.cos(a), cz + radius * Math.sin(a)};
            }
        }
        // The band's heading, from where it appears to its target; stacks side by side across it.
        double mx = 0, mz = 0;
        for (double[] q : at0) {
            mx += q[0] / m;
            mz += q[1] / m;
        }
        double hd = Math.hypot(tx - mx, tz - mz);
        double hx = hd > 1e-6 ? (tx - mx) / hd : 1.0, hz = hd > 1e-6 ? (tz - mz) / hd : 0.0;
        double rx = -hz, rz = hx;
        int first = p.stacks.size();
        Lurk lurk = lurks.get(agent);
        boolean chiefFlag = (pflags & ConflictRecords.Materialize.FLAG_CHIEF) != 0;
        for (int i = 0; i < m; i++) {
            boolean chief = chiefFlag && (sflags[i] & ConflictRecords.Materialize.STACK_CHIEF) != 0;
            if (counts[i] == 0 && !chief)
                continue; // a chief's stack with no units left is the chief alone
            Kind k = kinds.get(i);
            Stack st = new Stack(p, p.stacks.size(), wave);
            double side = (i - (m - 1) / 2.0) * STACK_SPACING;
            st.offX = rx * side;
            st.offZ = rz * side;
            double sx = at0[i][0], sz = at0[i][1];
            if (radius == 0) {
                sx += st.offX;
                sz += st.offZ;
            }
            st.ax = st.ex = sx;
            st.az = st.ez = sz;
            st.t0 = st.t1 = now;
            st.hx = hx;
            st.hz = hz;
            for (int g = 0; g < counts[i]; g++)
                st.slots.add(new Slot(st, false, false, k.body(), k.javaBody(), k.type(), lateral(g, counts[i]), ROW * (g / 2)));
            if (chief) {
                // The war chief is no unit of the band (the engine counts it apart): an extra body of the
                // stack's kind at its rear (MZB-109).
                st.slots.add(new Slot(st, true, false, k.body(), k.javaBody(), k.type(), 0.0, ROW * ((counts[i] + 1) / 2)));
            }
            if ((sflags[i] & ConflictRecords.Materialize.STACK_BARE) != 0)
                for (Slot sl : st.slots)
                    sl.bare = true;
            st.walk(tx + st.offX, tz + st.offZ, p.speed, 0L, now);
            if (lurk != null)
                anchorAtSpot(st, lurk, now); // a lurking agent's stack stands at the spot, not on the arc (MZB-360)
            p.stacks.add(st);
        }
        int n = 0;
        for (int i = first; i < p.stacks.size(); i++) {
            Stack st = p.stacks.get(i);
            if (tryPlace(st, level, now))
                n += st.alive();
        }
        journalMaterialized(p, wave, pflags, n, now);
    }

    /** {@code flags}: the command's, echoed (docs/boundary.md §9.3): a replay tells a fight from a sighting by them. */
    private void journalMaterialized(Party p, int wave, int flags, int spawned, long now) {
        int dim = session.dimId(p.dimension);
        ByteBuffer r = session.boundary().beginJournaled(ConflictRecords.BAND_MATERIALIZED, ConflictRecords.Materialized.SIZE, spawned, now);
        r.putLong(ConflictRecords.Materialized.AGENT, p.agent);
        r.putInt(ConflictRecords.Materialized.EPOCH, p.epoch);
        r.putInt(ConflictRecords.Materialized.DIM, dim);
        r.putInt(ConflictRecords.Materialized.WAVE, wave);
        r.putInt(ConflictRecords.Materialized.FLAGS, flags);
        session.boundary().commit(r);
        materialized++;
    }

    private void reject(long agent, int epoch, int wave, long cmdSeq, int reason, long now) {
        ByteBuffer r = session.boundary().beginJournaled(ConflictRecords.BAND_SPAWN_REJECTED, ConflictRecords.SpawnRejected.SIZE, reason, now);
        r.putLong(ConflictRecords.SpawnRejected.AGENT, agent);
        r.putInt(ConflictRecords.SpawnRejected.EPOCH, epoch);
        r.putInt(ConflictRecords.SpawnRejected.WAVE, wave);
        r.putLong(ConflictRecords.SpawnRejected.CMD_SEQ, cmdSeq);
        session.boundary().commit(r);
        rejected++;
    }

    /** BAND_STEER: every stack's anchor walks to the point beside the new target it keeps. */
    private void steer(ByteBuffer b, int at, int flags, long now) {
        Party p = byAgent.get(b.getLong(at + ConflictRecords.Steer.AGENT));
        if (p == null || p.epoch != b.getInt(at + ConflictRecords.Steer.EPOCH) || p.recalling)
            return;
        if (lurks.containsKey(p.agent))
            return; // a lurking band stands at its spot; the engine steers it again once the lurk is removed (MZB-352)
        float speed = b.getFloat(at + ConflictRecords.Steer.SPEED);
        if (!Float.isFinite(speed) || speed < 0)
            speed = 0;
        p.speed = speed;
        p.attack = (flags & ConflictRecords.Steer.FLAG_ATTACK) != 0;
        double x = b.getDouble(at + ConflictRecords.Steer.X), z = b.getDouble(at + ConflictRecords.Steer.Z);
        long arrive = b.getLong(at + ConflictRecords.Steer.ARRIVE);
        for (Stack st : p.stacks)
            if (st.state != StackState.GONE)
                st.walk(x + st.offX, z + st.offZ, speed, arrive, now);
        steered++;
    }

    /**
     * BAND_RECALL: pending stacks never come; placed ones walk off toward the record's point and each
     * despawns when no player is within {@value #OUT_OF_VIEW} blocks of it, or all at the timeout. Then
     * BAND_DEMATERIALIZED (reason 1) with every living member as a survivor. An agent the bridge does not
     * hold is not answered.
     */
    private void recall(ByteBuffer b, int at, int reason, long now) {
        Party p = byAgent.get(b.getLong(at + ConflictRecords.Recall.AGENT));
        if (p == null || p.epoch != b.getInt(at + ConflictRecords.Recall.EPOCH) || p.recalling)
            return;
        long timeout = b.getInt(at + ConflictRecords.Recall.TIMEOUT) & 0xFFFF_FFFFL;
        startRecall(p, reason, timeout, b.getDouble(at + ConflictRecords.Recall.TOWARD_X),
                b.getDouble(at + ConflictRecords.Recall.TOWARD_Z), now);
    }

    /**
     * A band handed back to its camp is home (MOWZIE.md §4.9, a homecoming): the engine sends no recall at a
     * camp that is in, so what is left of it (a raptor pack, a puppet with no line) walks toward the camp and
     * goes as a recalled band does, and its BAND_DEMATERIALIZED, for a party the engine has ended, books
     * nothing (MZB-211).
     */
    void homeAt(Party p, double x, double z, long now) {
        if (byAgent.get(p.agent) != p || p.recalling)
            return;
        startRecall(p, ConflictRecords.RECALL_DONE, 0L, x, z, now);
    }

    private void startRecall(Party p, int reason, long timeout, double x, double z, long now) {
        releaseLurk(p); // a recalled puppet is let go; the lurk itself stays (ledger 8: a recall alone does not end it)
        p.recalling = true;
        p.recallReason = reason;
        p.recallDeadline = now + (timeout > 0 ? timeout : RECALL_TIMEOUT);
        for (Stack st : p.stacks) {
            if (st.state == StackState.PENDING) {
                unwatch(st);
                st.state = StackState.GONE;
            } else if (st.state == StackState.PLACED) {
                st.walk(x + st.offX, z + st.offZ, p.speed, 0L, now);
            }
        }
        recalled++;
        recallStep(p, now);
    }

    /**
     * BAND_MARCH (M10, MZB-468): the agent's march, kept for the column, the turns, the stop and the trim. A record
     * replaces the held path only with a newer revision; ENDED drops the path, remembers the agent and trims
     * nothing; an ended agent's later records are dropped. Nothing is journaled and nothing is saved.
     */
    private void march(ByteBuffer b, int at, int size, int waypoints, EngineNames names) {
        MarchPath m = MarchPath.decode(b, at, size, waypoints, names);
        if (m == null || !ConflictRecords.isConflictAgent(m.agent())) {
            malformed++;
            return;
        }
        marchRecords++;
        switch (marches.offer(m)) {
            case STALE -> marchStale++;
            case ENDED -> marchEnds++;
            case KEPT -> {
            }
        }
    }

    /**
     * The march the engine last said for a party agent, or null: none said, ended, or a session that restarted.
     * It is the engine's word whether or not the party is embodied (GameTests, and M11's aloft).
     */
    public MarchPath march(long agent) {
        return marches.get(agent);
    }

    /** True when the engine ended this agent's march this session (GameTests). */
    public boolean marchEnded(long agent) {
        return marches.ended(agent);
    }

    /** DEMAND_SETTLED: the envoy's demand is closed; its name goes. */
    private void settled(ByteBuffer b, int at, int outcome) {
        settledCommands++;
        lastSettled = outcome;
        long faction = b.getLong(at + ConflictRecords.Settled.FACTION);
        UUID player = uuid(b.getLong(at + ConflictRecords.Settled.PLAYER_MOST), b.getLong(at + ConflictRecords.Settled.PLAYER_LEAST));
        Party p = byAgent.get(b.getLong(at + ConflictRecords.Settled.AGENT));
        if (p != null && p.envoy) {
            if (p.demand != null)
                p.good = p.demand.item;
            p.demand = null;
            p.settled = outcome;
            nameEnvoy(p);
        }
        ConflictNews.post(ConflictNews.Event.DEMAND_SETTLED, faction, player);
    }

    // ---- placing --------------------------------------------------------------------------------

    /**
     * Tests a stack and places it if it can be now: every chunk its members would stand in ticks
     * entities, Peaceful does not remove its kind, and it fits the spawn quota. True when it is placed.
     */
    private boolean tryPlace(Stack st, ServerLevel level, long now) {
        if (now < st.retryAt)
            return false;
        pendingTests++;
        st.tests++;
        for (Slot sl : st.slots) {
            if (!sl.alive || sl.entity != null)
                continue;
            double[] q = sl.placeXZ(now);
            if (!RtsimSession.entitiesTickAt(level, BlockPos.containing(q[0], level.getSeaLevel(), q[1]))) {
                st.dirty = false;
                watch(st, level, now);
                return false;
            }
        }
        if (level.getDifficulty() == Difficulty.PEACEFUL) {
            for (Slot sl : st.slots) {
                if (sl.alive && ConflictBodies.hostile(sl.type)) {
                    st.dirty = false; // a difficulty change marks it again
                    watch(st, level, now);
                    return false;
                }
            }
        }
        int n = 0;
        for (Slot sl : st.slots)
            if (sl.alive && sl.entity == null)
                n++;
        if (n == 0) {
            st.state = StackState.PLACED;
            unwatch(st);
            return true;
        }
        if (!session.spendSpawnBudget(n)) {
            st.dirty = true; // next tick
            watch(st, level, now);
            return false;
        }
        if (!spawn(st, level, now)) {
            st.retryAt = now + RETRY_FAILED;
            st.dirty = true;
            watch(st, level, now);
            return false;
        }
        st.state = StackState.PLACED;
        st.dirty = false;
        st.placings++;
        unwatch(st);
        Party p = st.party;
        if (!p.envoy && !p.arrivedNews) {
            p.arrivedNews = true;
            ConflictNews.post(ConflictNews.Event.BAND_ARRIVED, p.faction, p.targetPlayer);
        }
        return true;
    }

    /** Makes and adds every living member of a stack not in the world: all or none. */
    private boolean spawn(Stack st, ServerLevel level, long now) {
        Party p = st.party;
        AgentTag tag = new AgentTag(p.agent, p.epoch);
        List<Slot> todo = new ArrayList<>();
        for (Slot sl : st.slots)
            if (sl.alive && sl.entity == null)
                todo.add(sl);
        float yaw = (float) (Mth.atan2(st.hz, st.hx) * (180.0 / Math.PI)) - 90.0f;
        List<Mob> made = new ArrayList<>();
        Map<Slot, Vec3> holds = new HashMap<>();
        // What came with each body (a raptor's pack, MZB-428), in the order of todo.
        List<List<Mob>> madeExtras = new ArrayList<>();
        // A camp body this party set out with in this hand-over is handed to its puppet where it stands (§4.9).
        List<Camps.SetOut> handOffs = new ArrayList<>();
        Lurk lk = lurks.get(p.agent);
        for (Slot sl : todo) {
            // M8: the hold point first, on the rail's normal at the spot distance, not at the engine's rounded spot (the
            // re-check's P2). None standable refuses the lurk: the stack is tried again after RETRY_FAILED ticks. It is
            // decided before a camp body is claimed (the second re-check's P3): a refusal after the claim left the hand-off
            // claimed for the tick, or handed a body a player can see to a puppet that holdLurks then discarded; before it,
            // the SET_OUT is unclaimed and the tick's end resolves it as a take, with the body's records on a marker: the camp
            // body is still removed (taken, its health and crystal state lost), not discarded unseen after a hand-off. A kind
            // whose takes wait (a grove's, Rule U, MZB-433) holds that take while a player is near; a den's goes at once.
            Vec3 at = lk == null ? null : holdPoint(level, lk, sl);
            if (lk != null && at == null) {
                lurksRefused++;
                for (Mob q2 : made)
                    q2.discard();
                for (List<Mob> xs : madeExtras) // and what came with them (a raptor's pack, MZB-428)
                    for (Mob q2 : xs)
                        q2.discard();
                for (Camps.SetOut h : handOffs)
                    reconciler.unclaim(h);
                return false;
            }
            Camps.SetOut so = reconciler.claim(p.agent, p.epoch, sl.body);
            Mob from = so == null ? null : reconciler.claimed(so);
            double x, y, z;
            float yw = yaw;
            if (from != null) {
                // handed to its puppet where it stands; the hold walks the puppet to the hold point
                x = from.getX();
                y = from.getY();
                z = from.getZ();
                yw = from.getYRot();
                if (at != null)
                    holds.put(sl, at);
            } else if (at != null) {
                x = at.x;
                y = at.y;
                z = at.z;
                yw = (float) (Mth.atan2(lk.track().z - z, lk.track().x - x) * (180.0 / Math.PI)) - 90.0f;
                holds.put(sl, at);
            } else {
                double[] q = sl.placeXZ(now);
                x = q[0];
                z = q[1];
                y = level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, Mth.floor(x), Mth.floor(z));
            }
            Mob m;
            List<Mob> extras = new ArrayList<>();
            try {
                ConflictBodies.Maker maker = ConflictBodies.maker(sl.body);
                m = maker != null ? maker.make(level, x, y, z, yw, extras) : ConflictBodies.create(level, sl.type, x, y, z, yw);
                // A bare unit's body is made without its crystal, then a camp body's own state is copied over it.
                if (m != null && sl.bare)
                    BodyKinds.makeBare(sl.body, m);
                if (m != null && from != null)
                    so.site.kind.copy(from, m);
            } catch (RuntimeException ex) {
                MicRtsim.LOGGER.error("[mic_rtsim] creating a conflict body {} failed", sl.body, ex);
                m = null;
            }
            if (so != null)
                handOffs.add(so);
            if (m == null) {
                for (Mob q : made)
                    q.discard();
                for (List<Mob> xs : madeExtras)
                    for (Mob q : xs)
                        q.discard();
                for (Mob q : extras)
                    q.discard();
                for (Camps.SetOut h : handOffs)
                    reconciler.unclaim(h);
                return false;
            }
            m.setData(RtsimAttachments.AGENT, tag);
            sl.ownPersistence = ConflictBodies.hold(m);
            for (Mob x2 : extras) {
                x2.setData(RtsimAttachments.AGENT, tag);
                ConflictBodies.hold(x2);
            }
            made.add(m);
            madeExtras.add(extras);
        }
        // Registered before any joins: the join check already knows them.
        for (int i = 0; i < todo.size(); i++) {
            Slot sl = todo.get(i);
            sl.entity = made.get(i);
            sl.uuid = sl.entity.getUUID();
            sl.removing = false;
            sl.holdAt = holds.get(sl); // M8: set at once, so the follow goal's place is the hold point from the first tick
            byUuid.put(sl.uuid, sl);
            sl.extras = madeExtras.get(i);
            for (Mob x2 : sl.extras)
                extraOf.put(x2.getUUID(), sl);
        }
        for (int i = 0; i < todo.size(); i++) {
            // A body joins before what came with it: a raptor's follower looks for its leader in its first tick.
            boolean in = level.addFreshEntity(made.get(i));
            for (Mob x2 : madeExtras.get(i))
                in = in && level.addFreshEntity(x2);
            if (!in) {
                for (Slot sl : todo)
                    despawn(sl);
                for (Camps.SetOut h : handOffs)
                    reconciler.unclaim(h);
                return false;
            }
        }
        for (Camps.SetOut h : handOffs)
            reconciler.handedOff(h, now);
        for (Slot sl : todo) {
            Mob m = sl.entity;
            sl.goal = new ConflictFollowGoal(m, sl);
            m.goalSelector.addGoal(ConflictFollowGoal.PRIORITY, sl.goal);
            if (p.envoy && p.guarded)
                sl.stripped = ConflictBodies.stripTargets(m);
            if (sl.envoy)
                nameEnvoy(p);
        }
        placed += todo.size();
        return true;
    }

    /**
     * Takes a slot's extras out of the world (MZB-428): a body's despawn, a stack's going back to pending and a
     * party's end do, and so a follower whose raptor died goes with them. Books nothing.
     */
    private void dropExtras(Slot sl) {
        if (sl.extras.isEmpty())
            return;
        for (Mob x : sl.extras) {
            extraOf.remove(x.getUUID(), sl);
            if (!x.isRemoved())
                x.discard();
        }
        sl.extras = List.of();
    }

    /** Takes a member's body out of the world (it stays a living member, not in Minecraft). */
    private void despawn(Slot sl) {
        Mob m = sl.entity;
        releaseLurk(sl); // the Lurker lets go of the body it is about to lose (its goals, its stamp), so nothing of it outlives it
        if (m != null) {
            sl.removing = true;
            if (sl.goal != null)
                m.goalSelector.removeGoal(sl.goal);
            if (!m.isRemoved()) {
                m.ejectPassengers();
                m.discard();
            }
        }
        if (sl.uuid != null)
            byUuid.remove(sl.uuid, sl);
        dropExtras(sl);
        sl.goal = null;
        sl.stripped = null;
        sl.entity = null;
        sl.uuid = null;
        sl.holdAt = null;
    }

    // ---- the pending stacks' chunks -------------------------------------------------------------

    /** The chunks a stack's living members' places are in now (GameTests; the tick uses {@link #watch}). */
    private static Set<Long> chunksOf(Stack st, long now) {
        Set<Long> out = new HashSet<>(4);
        for (Slot sl : st.slots)
            if (sl.alive)
                out.add(sl.placeChunk(now));
        return out;
    }

    /**
     * Watches the chunks a pending stack's members would stand in; a change of chunks marks it for a test.
     * Each member's chunk is kept, so a stack whose places stay in their chunks costs no allocation
     * (D1): the set is rebuilt only when a place crosses a chunk edge or a member dies.
     */
    private void watch(Stack st, ServerLevel level, long now) {
        boolean moved = st.chunks.isEmpty();
        for (Slot sl : st.slots) {
            long c = sl.alive ? sl.placeChunk(now) : NO_CHUNK;
            if (c != sl.chunk) {
                sl.chunk = c;
                moved = true;
            }
        }
        if (!moved)
            return;
        LongOpenHashSet now2 = new LongOpenHashSet(4);
        for (Slot sl : st.slots)
            if (sl.chunk != NO_CHUNK)
                now2.add(sl.chunk);
        if (now2.equals(st.chunks))
            return;
        Long2ObjectOpenHashMap<Watch> w = watches.computeIfAbsent(st.party.dimension, k -> new Long2ObjectOpenHashMap<>());
        for (LongIterator it = st.chunks.iterator(); it.hasNext(); ) {
            long c = it.nextLong();
            if (now2.contains(c))
                continue;
            Watch x = w.get(c);
            if (x != null) {
                x.stacks.remove(st);
                if (x.stacks.isEmpty())
                    w.remove(c);
            }
        }
        for (LongIterator it = now2.iterator(); it.hasNext(); ) {
            long c = it.nextLong();
            if (st.chunks.contains(c))
                continue;
            Watch x = w.get(c);
            if (x == null) {
                x = new Watch();
                x.ticking = RtsimSession.entitiesTickAt(level, centre(level, c));
                w.put(c, x);
            }
            x.stacks.add(st);
        }
        if (!st.chunks.isEmpty()) {
            placeMoves++;
            st.moves++;
            st.dirty = true;
        }
        st.chunks.clear();
        st.chunks.addAll(now2);
    }

    /** The centre of chunk {@code c} at sea level, in the reused {@link #probe}. */
    private BlockPos centre(ServerLevel level, long c) {
        return probe.set(ChunkPos.getX(c) * 16 + 8, level.getSeaLevel(), ChunkPos.getZ(c) * 16 + 8);
    }

    private void unwatch(Stack st) {
        for (Slot sl : st.slots)
            sl.chunk = NO_CHUNK;
        if (st.chunks.isEmpty())
            return;
        Long2ObjectOpenHashMap<Watch> w = watches.get(st.party.dimension);
        if (w != null) {
            for (LongIterator it = st.chunks.iterator(); it.hasNext(); ) {
                long c = it.nextLong();
                Watch x = w.get(c);
                if (x != null) {
                    x.stacks.remove(st);
                    if (x.stacks.isEmpty())
                        w.remove(c);
                }
            }
        }
        st.chunks.clear();
    }

    /** Reads every watched chunk once; one whose entity-ticking state changed marks its stacks for a test. */
    private void pollWatches() {
        for (var e : watches.entrySet()) {
            ServerLevel level = level(e.getKey());
            for (var c : e.getValue().long2ObjectEntrySet()) {
                boolean t = level != null && RtsimSession.entitiesTickAt(level, centre(level, c.getLongKey()));
                Watch w = c.getValue();
                if (t != w.ticking) {
                    w.ticking = t;
                    watchFlips++;
                    for (Stack st : w.stacks)
                        st.dirty = true;
                }
            }
        }
    }

    // ---- the tick -------------------------------------------------------------------------------

    @Override
    public void endOfTick(RtsimSession s, long now) {
        if (s != session || s.failed())
            return;
        if (!RtsimConfig.on(Module.CONFLICT)) {
            if (held)
                switchedOff(now);
            reconciler.switchedOff(now);
            owedHunts.clear();
            clearMarches(); // not gated on `held`: a march is kept with no party (MZB-468)
            return;
        }
        Difficulty d = s.server().getWorldData().getDifficulty();
        if (d != difficulty) {
            difficulty = d;
            sendDifficulty(now);
            for (Party p : byAgent.values())
                for (Stack st : p.stacks)
                    if (st.state == StackState.PENDING)
                        st.dirty = true;
        }
        owedHunts.tick(s, now);
        held = !idle(); // a lurk or a pending roar is held too: a switch-off clears them (MZB-359)
        if (!held) {
            reconciler.tick(now); // nothing without a camp declared (D1)
            return; // D1: no faction, nothing to do
        }
        settleDeaths(now);
        passing.clear();
        passing.addAll(byAgent.values());
        for (Party p : passing)
            pass(p, now);
        pollWatches();
        for (Party p : passing) {
            if (byAgent.get(p.agent) != p || p.recalling)
                continue;
            ServerLevel level = level(p.dimension);
            if (level == null)
                continue;
            for (Stack st : p.stacks)
                if (st.state == StackState.PENDING && st.dirty)
                    tryPlace(st, level, now);
        }
        passing.clear();
        holdLurks(now); // after every level ticked and the placings: a puppet placed this tick is held at once
        // After the placings: a materialize of this tick has claimed the set-out bodies it hands off.
        reconciler.tick(now);
        flushSignals(now, false);
        if (now % NOTICE_CHECK == 0)
            checkNotices(now);
    }

    /** The conflict module was switched off: every party goes back to the engine as it stands, and the rest is forgotten. */
    private void switchedOff(long now) {
        settleDeaths(now);
        flushSignals(now, true);
        for (Party p : List.copyOf(byAgent.values()))
            capture(p, ConflictRecords.Dematerialized.SESSION_END, now);
        territories.clear();
        notices.clear();
        signals.clear();
        watches.clear();
        clearLurks();
        held = false;
    }

    private void pass(Party p, long now) {
        if (byAgent.get(p.agent) != p)
            return;
        ServerLevel level = level(p.dimension);
        for (Stack st : p.stacks) {
            if (st.state == StackState.PLACED)
                checkPlaced(st, level, now);
            if (st.state != StackState.GONE && st.alive() == 0) {
                unwatch(st);
                st.state = StackState.GONE;
            }
        }
        if (p.recalling) {
            recallStep(p, now);
            return;
        }
        if (level != null)
            for (Stack st : p.stacks)
                if (st.state == StackState.PENDING)
                    watch(st, level, now);
        if (now - p.lastState >= STATE_INTERVAL)
            state(p, now);
    }

    /**
     * A placed stack keeps to the area (MZB-125). A member out of it (not ticking where it stands, or its
     * body gone) whose place still ticks is moved back to its place, or, its body gone, made again at its
     * place after {@value #REQUEUE_COOLDOWN} ticks; the stack goes back to pending (every body out of the
     * world, made again no sooner than that) only when such a member's place stops ticking too, or
     * Peaceful took a hostile body. A member that fights out of its place in a ticking chunk is left alone.
     */
    private void checkPlaced(Stack st, ServerLevel level, long now) {
        if (level == null) {
            requeue(st, null, now);
            return;
        }
        boolean refill = false;
        for (Slot sl : st.slots) {
            if (!sl.alive || sl.pendingDeath != null)
                continue;
            Mob e = sl.entity;
            boolean present = e != null && !e.isRemoved() && e.level() == level;
            if (present && RtsimSession.entitiesTickAt(level, e.blockPosition()))
                continue;
            if (!sl.placeTicks(level, now, probe)
                    || (!present && level.getDifficulty() == Difficulty.PEACEFUL && ConflictBodies.hostile(sl.type))) {
                requeue(st, level, now);
                return;
            }
            if (present) {
                Vec3 q = sl.place(e);
                e.stopRiding();
                e.teleportTo(q.x, q.y, q.z);
                e.getNavigation().stop();
                strays++;
            } else {
                if (e != null) {
                    despawn(sl); // gone from the world (another dimension, unloaded): a body to make again
                    st.retryAt = Math.max(st.retryAt, now + REQUEUE_COOLDOWN);
                }
                refill = true;
            }
        }
        if (refill && now >= st.retryAt && !st.party.recalling)
            refill(st, level, now);
    }

    /** Makes again, at their places, the bodies of a placed stack's members whose bodies went. */
    private void refill(Stack st, ServerLevel level, long now) {
        int n = 0;
        for (Slot sl : st.slots)
            if (sl.alive && sl.pendingDeath == null && sl.entity == null)
                n++;
        if (n == 0 || !session.spendSpawnBudget(n))
            return;
        if (spawn(st, level, now))
            refills += n;
        else
            st.retryAt = now + RETRY_FAILED;
    }

    /** Takes a placed stack back to pending: its bodies out of the world, made again no sooner than {@value #REQUEUE_COOLDOWN} ticks on. */
    private void requeue(Stack st, ServerLevel level, long now) {
        for (Slot x : st.slots) {
            if (x.alive && x.pendingDeath == null)
                despawn(x);
            dropExtras(x); // a dead raptor's followers fight on until the stack goes (MZB-428)
        }
        if (st.party.recalling) {
            st.state = StackState.GONE;
            return;
        }
        st.state = StackState.PENDING;
        st.dirty = true;
        st.retryAt = now + REQUEUE_COOLDOWN;
        requeued++;
        if (level != null)
            watch(st, level, now);
    }

    private void recallStep(Party p, long now) {
        ServerLevel level = level(p.dimension);
        boolean timeUp = now >= p.recallDeadline;
        boolean any = false;
        for (Stack st : p.stacks) {
            if (st.state != StackState.PLACED)
                continue;
            if (timeUp || level == null || !seen(st, level)) {
                for (Slot sl : st.slots) {
                    if (sl.alive && sl.pendingDeath == null)
                        despawn(sl);
                    dropExtras(sl);
                }
                st.state = StackState.GONE;
            } else {
                any = true;
            }
        }
        if (!any && !dying(p))
            finish(p, ConflictRecords.Dematerialized.COMMAND, now);
    }

    private static boolean dying(Party p) {
        for (Stack st : p.stacks)
            for (Slot sl : st.slots)
                if (sl.pendingDeath != null)
                    return true;
        return false;
    }

    /** Whether a player (not a spectator) is within {@link #OUT_OF_VIEW} of a living member. */
    private static boolean seen(Stack st, ServerLevel level) {
        double r2 = OUT_OF_VIEW * OUT_OF_VIEW;
        for (Slot sl : st.slots) {
            if (!sl.alive || sl.entity == null)
                continue;
            for (ServerPlayer pl : level.players())
                if (!pl.isSpectator() && pl.distanceToSqr(sl.entity) <= r2)
                    return true;
        }
        return false;
    }

    private void state(Party p, long now) {
        int dim = session.dimId(p.dimension); // before direct(): an id is defined before its first use
        double x = 0, y = 0, z = 0;
        int present = 0;
        boolean fighting = false;
        for (Stack st : p.stacks) {
            for (Slot sl : st.slots) {
                if (!sl.alive || sl.pendingDeath != null || sl.entity == null)
                    continue;
                x += sl.entity.getX();
                y += sl.entity.getY();
                z += sl.entity.getZ();
                present++;
                fighting |= sl.entity.getTarget() != null;
            }
        }
        if (present == 0) {
            int n = 0;
            for (Stack st : p.stacks) {
                if (st.state == StackState.GONE)
                    continue;
                double[] a = st.anchor(now);
                x += a[0];
                z += a[1];
                n++;
            }
            if (n > 0) {
                x /= n;
                z /= n;
            }
            y = 64.0;
        } else {
            x /= present;
            y /= present;
            z /= present;
        }
        int flags = (present > 0 ? ConflictRecords.BandState.FLAG_TICKING : 0) | (fighting ? ConflictRecords.BandState.FLAG_FIGHTING : 0);
        int at = session.boundary().direct(ConflictRecords.BAND_STATE, ConflictRecords.BandState.SIZE, flags, now);
        if (at < 0)
            return; // dropped for space: sent again at the next interval
        ByteBuffer b = session.boundary().openBuffer();
        b.putLong(at + ConflictRecords.BandState.AGENT, p.agent);
        b.putInt(at + ConflictRecords.BandState.EPOCH, p.epoch);
        b.putInt(at + ConflictRecords.BandState.DIM, dim);
        b.putInt(at + ConflictRecords.BandState.PRESENT, present);
        b.putDouble(at + ConflictRecords.BandState.X, x);
        b.putDouble(at + ConflictRecords.BandState.Y, y);
        b.putDouble(at + ConflictRecords.BandState.Z, z);
        p.lastState = now;
    }

    // ---- ending a party -------------------------------------------------------------------------

    /** Ends an embodiment as it stands (a new epoch, the session's end, the module off). */
    private void capture(Party p, int reason, long now) {
        settleDeaths(p, now);
        for (Stack st : p.stacks)
            for (Slot sl : st.slots)
                if (sl.alive && sl.pendingDeath == null)
                    despawn(sl);
        finish(p, reason, now);
    }

    /** BAND_DEMATERIALIZED with every living member, in Minecraft or pending, as a survivor of its body; the party goes. */
    private void finish(Party p, int reason, long now) {
        Map<Integer, Integer> survivors = new LinkedHashMap<>();
        for (Stack st : p.stacks)
            for (Slot sl : st.slots)
                if (sl.alive && !sl.chief)
                    survivors.merge(sl.javaBody, 1, Integer::sum);
        unregister(p);
        int n = survivors.size();
        ByteBuffer r = session.boundary().beginJournaled(ConflictRecords.BAND_DEMATERIALIZED, ConflictRecords.Dematerialized.size(n), reason, now);
        r.putLong(ConflictRecords.Dematerialized.AGENT, p.agent);
        r.putInt(ConflictRecords.Dematerialized.EPOCH, p.epoch);
        r.putInt(ConflictRecords.Dematerialized.COUNT, n);
        int i = 0;
        for (var e : survivors.entrySet()) {
            int o = ConflictRecords.Dematerialized.SURVIVORS + i++ * ConflictRecords.Dematerialized.ENTRY;
            r.putInt(o + ConflictRecords.Dematerialized.E_BODY, e.getKey());
            r.putInt(o + ConflictRecords.Dematerialized.E_COUNT, e.getValue());
        }
        session.boundary().commit(r);
        dematerialized++;
    }

    private void unregister(Party p) {
        if (byAgent.get(p.agent) == p)
            byAgent.remove(p.agent);
        for (Stack st : p.stacks) {
            unwatch(st);
            st.state = StackState.GONE;
            for (Slot sl : st.slots) {
                if (sl.uuid != null)
                    byUuid.remove(sl.uuid, sl);
                dropExtras(sl);
            }
        }
    }

    // ---- deaths and takes -----------------------------------------------------------------------

    /** Who did it: a player (a NeoForge FakePlayer is a machine, MZB-111: other), a mob, something else. */
    static int byKind(Entity killer) {
        if (killer == null)
            return ConflictRecords.BY_NONE;
        if (killer instanceof FakePlayer)
            return ConflictRecords.BY_OTHER;
        if (killer instanceof Player)
            return ConflictRecords.BY_PLAYER;
        if (killer instanceof Mob)
            return ConflictRecords.BY_MOB;
        return ConflictRecords.BY_OTHER;
    }

    /** Who is credited with a death, for the records that name a killer (MZB-261). */
    record Credit(int by, UUID killer) {}

    /**
     * The kill credit of a death (MZB-261): {@link #byKind} of the damage's entity, and when that is not a
     * player, vanilla's {@code getKillCredit} (a player's hit within 100 ticks, or a tame animal's owner's)
     * when it is a real player (a FakePlayer is a machine, MZB-111). So a pet's kill, and a death within five
     * seconds of a player's hit, are the player's (EQUILIBRIUM item 9). Used for a camp body's CAMP_DEATH and
     * a chief puppet's CHIEF_KILLED; BAND_CASUALTIES keep {@link #byKind}, since nothing reads them.
     */
    static Credit credit(LivingEntity dead, DamageSource source) {
        Entity killer = source.getEntity();
        int by = byKind(killer);
        if (by == ConflictRecords.BY_PLAYER)
            return new Credit(by, killer.getUUID());
        if (dead.getKillCredit() instanceof Player pl && !(pl instanceof FakePlayer))
            return new Credit(ConflictRecords.BY_PLAYER, pl.getUUID());
        return new Credit(by, null);
    }

    /**
     * The faction whose chief an entity is: a chief puppet of a live party, else 0. (A camp's own body is
     * {@link Camps}'s to name.)
     */
    long chiefPuppetFaction(Entity e) {
        Slot sl = live(e);
        return sl != null && sl.chief ? sl.stack.party.faction : 0L;
    }

    /** LivingDeathEvent (lowest, not cancelled): a member's death is journaled at the end of the tick, once certain. */
    void onDeath(LivingEntity dead, DamageSource source, long now) {
        reconciler.onDeath(dead, source);
        if (reconciler.secondBody(dead))
            return; // no path journals a second body's death, and it kills nobody's kin (MZB-431)
        Slot sl = live(dead);
        Entity killer = source.getEntity();
        if (sl != null) {
            // a chief's death goes by the kill credit (CHIEF_KILLED renews the den, MZB-261); the rest by the hit
            if (sl.chief) {
                Credit c = credit(dead, source);
                sl.pendingDeath = new Death(c.by(), c.killer());
                return;
            }
            int by = byKind(killer);
            sl.pendingDeath = new Death(by, by == ConflictRecords.BY_PLAYER ? killer.getUUID() : null);
            return;
        }
        // A camp's kin killed by a player inside its faction's territory: killing (4).
        if (territories.isEmpty() || !(killer instanceof Player pl) || pl.isSpectator()
                || RtsimAttachments.tag(dead) != null || !(dead.level() instanceof ServerLevel level))
            return;
        int x = Mth.floor(dead.getX()), z = Mth.floor(dead.getZ());
        for (Territories.Territory t : territories.containing(level.dimension(), x, z)) {
            Camp c = camps.get(t.faction());
            if (c != null && c.kin() != null && t.minds(ConflictRecords.SIGNAL_KILLING) && c.kin().test(dead)) {
                if (killedAt != now) {
                    killedAt = now;
                    killedThisTick.clear();
                }
                if (!killedThisTick.add(dead.getUUID()))
                    return; // the same death posted again in this tick
                count(ConflictRecords.SIGNAL_KILLING, t.faction(), actor(pl), level.dimension(), dead.blockPosition(), t, now);
                return;
            }
        }
    }

    /** LivingDropsEvent: a dying member's drops carry its record until the engine has it (§3.1). */
    void onDrops(LivingEntity dead, Collection<ItemEntity> drops) {
        Slot sl = live(dead);
        if (sl != null && sl.pendingDeath != null)
            sl.drops = new ArrayList<>(drops);
    }

    private void settleDeaths(long now) {
        for (Party p : byAgent.values()) // journals and forgets members, never parties
            settleDeaths(p, now);
    }

    private void settleDeaths(Party p, long now) {
        for (Stack st : p.stacks) {
            for (Slot sl : st.slots) {
                if (sl.pendingDeath == null)
                    continue;
                Mob e = sl.entity;
                boolean dead = e == null || e.isRemoved() || e.isDeadOrDying();
                if (!dead) {
                    sl.pendingDeath = null; // another mod cancelled it
                    sl.drops = null;
                    continue;
                }
                Death d = sl.pendingDeath;
                sl.pendingDeath = null;
                ByteBuffer r = journalLoss(sl, d.by(), d.killer(), now);
                long seq = r.getLong(Obs.OBS_SEQ);
                if (sl.drops != null)
                    for (ItemEntity drop : sl.drops)
                        if (!drop.isRemoved())
                            session.carryOnEntity(drop, List.of(r.array()), seq);
                sl.drops = null;
                forget(sl);
            }
        }
    }

    /**
     * A member leaves the band other than by death: taken by a player (Carry On: killer kind player and
     * that player) or removed by something else (kind other, no killer, no killing signal). A body still
     * in the world becomes an ordinary mob carrying its record.
     */
    private void gone(Slot sl, int by, UUID player, long now, boolean keepBody) {
        Mob e = sl.entity;
        ByteBuffer r = journalLoss(sl, by, player, now);
        if (e != null) {
            e.removeData(RtsimAttachments.AGENT);
            if (keepBody && !e.isRemoved()) {
                if (sl.goal != null)
                    e.goalSelector.removeGoal(sl.goal);
                ConflictBodies.restoreTargets(e, sl.stripped);
                e.setCustomName(null);
                e.setCustomNameVisible(false);
                ConflictBodies.release(e, sl.ownPersistence);
                session.carryOnEntity(e, List.of(r.array()), r.getLong(Obs.OBS_SEQ));
            }
        }
        taken++;
        forget(sl);
    }

    /** CHIEF_KILLED for the chief, else BAND_CASUALTIES of one member, each with a Java op id (MZB-589 for the latter). */
    private ByteBuffer journalLoss(Slot sl, int by, UUID killer, long now) {
        Party p = sl.stack.party;
        ByteBuffer r;
        if (sl.chief) {
            r = session.boundary().beginJournaled(ConflictRecords.CHIEF_KILLED, ConflictRecords.ChiefKilled.SIZE, by, now);
            r.putLong(ConflictRecords.ChiefKilled.AGENT, p.agent);
            r.putInt(ConflictRecords.ChiefKilled.EPOCH, p.epoch);
            if (killer != null) {
                r.putLong(ConflictRecords.ChiefKilled.KILLER_MOST, killer.getMostSignificantBits());
                r.putLong(ConflictRecords.ChiefKilled.KILLER_LEAST, killer.getLeastSignificantBits());
            }
            session.boundary().commitWithOpId(r, ConflictRecords.ChiefKilled.OP_ID);
            chiefsKilled++;
            ConflictNews.post(ConflictNews.Event.CHIEF_KILLED, p.faction, killer);
        } else {
            r = session.boundary().beginJournaled(ConflictRecords.BAND_CASUALTIES, ConflictRecords.Casualties.SIZE, by, now);
            r.putLong(ConflictRecords.Casualties.AGENT, p.agent);
            r.putInt(ConflictRecords.Casualties.EPOCH, p.epoch);
            r.putInt(ConflictRecords.Casualties.BODY, sl.javaBody);
            r.putInt(ConflictRecords.Casualties.COUNT, 1);
            if (killer != null) {
                r.putLong(ConflictRecords.Casualties.KILLER_MOST, killer.getMostSignificantBits());
                r.putLong(ConflictRecords.Casualties.KILLER_LEAST, killer.getLeastSignificantBits());
            }
            session.boundary().commitWithOpId(r, ConflictRecords.Casualties.OP_ID);
            casualties++;
        }
        return r;
    }

    /** A member is no longer the party's: out of the registry, its goal gone. */
    private void forget(Slot sl) {
        sl.alive = false;
        sl.removing = true;
        if (sl.goal != null && sl.entity != null)
            sl.entity.goalSelector.removeGoal(sl.goal);
        sl.goal = null;
        // A body that outlives the party (a take) is an ordinary mob again: no restriction, not awake for good (UR-1, MZB-369)
        releaseLurk(sl);
        if (sl.uuid != null)
            byUuid.remove(sl.uuid, sl);
    }

    /** The live member an entity is, or null. */
    Slot live(Entity e) {
        Slot sl = byUuid.get(e.getUUID());
        return sl != null && sl.entity == e ? sl : null;
    }

    /**
     * Whether an entity is a live member's extra (a raptor's follower, MZB-428). Its death books nothing (it is no
     * member), so it drops no loot and no experience either (MZB-643): the stack is made again with new ones.
     */
    public boolean isExtra(Entity e) {
        Slot sl = extraOf.get(e.getUUID());
        return sl != null && sl.extras.contains(e);
    }

    // ---- the envoy: guard, answers, payment -----------------------------------------------------

    /**
     * Why a faction is at odds with a player, for its band's warning (MZB-124): the signal of an open
     * demand of that faction's envoy to them (or to anyone), else of a notice kept for them; -1 for none.
     */
    int reasonFor(long faction, UUID player) {
        for (Party q : byAgent.values())
            if (q.envoy && q.faction == faction && q.demand != null && (q.player == null || q.player.equals(player)))
                return q.demand.signal;
        Notice n = notices.get(new NoticeKey(faction, player));
        return n != null ? n.signal() : -1;
    }

    /** The guarded party member an entity is, or null. */
    Slot guarded(Entity e) {
        Slot sl = live(e);
        return sl != null && sl.alive && sl.stack.party.guarded ? sl : null;
    }

    /** LivingChangeTargetEvent: no one takes a guarded member as a target, and a guarded member takes none. */
    boolean blockTarget(LivingEntity who, LivingEntity target) {
        if (target == null)
            return false;
        Slot self = guarded(who);
        if (self != null) {
            blockedTargets++;
            return true;
        }
        Slot on = guarded(target);
        if (on == null)
            return false;
        Slot mine = live(who);
        if (mine != null && mine.stack.party == on.stack.party)
            return false;
        blockedTargets++;
        return true;
    }

    /**
     * LivingIncomingDamageEvent: a guarded member takes no hit unless a player (not a machine, not a
     * spectator) is the attacker or the projectile's owner, or the damage bypasses invulnerability. A
     * player's hit lifts the guard and is an insult (ENVOY_ANSWER 3, once per player). True to cancel.
     */
    boolean blockHit(LivingEntity victim, DamageSource source, long now) {
        Slot sl = guarded(victim);
        if (sl == null)
            return false;
        if (source.is(net.minecraft.tags.DamageTypeTags.BYPASSES_INVULNERABILITY))
            return false;
        if (source.getEntity() instanceof Player pl && !(pl instanceof FakePlayer) && !pl.isSpectator()) {
            struck(sl.stack.party, pl.getUUID(), now);
            return false;
        }
        blockedHits++;
        return true;
    }

    /**
     * A player struck the envoy or a guard: all get their target goals back, for any player's blow. The
     * insult is answered once. A blow from the demand's player (or any player's, for a demand to none)
     * closes the demand: the envoy takes nothing more and the engine refuses it. Another player's blow is
     * that player's own insult (the engine books it against them, answered once per player) and leaves the
     * demand open to its player (MZB-145).
     */
    private void struck(Party p, UUID player, long now) {
        p.guarded = false;
        for (Stack st : p.stacks)
            for (Slot sl : st.slots)
                if (sl.alive && sl.entity != null) {
                    ConflictBodies.restoreTargets(sl.entity, sl.stripped);
                    sl.stripped = null;
                }
        if (p.player == null || p.player.equals(player)) {
            if (!p.insulted) {
                p.insulted = true;
                answer(p, ConflictRecords.ANSWER_ATTACKED, player, now);
            }
        } else if (p.struckBy.add(player)) {
            answer(p, ConflictRecords.ANSWER_ATTACKED, player, now);
        }
    }

    private void answer(Party p, int answer, UUID player, long now) {
        ByteBuffer r = session.boundary().beginJournaled(ConflictRecords.ENVOY_ANSWER, ConflictRecords.EnvoyAnswer.SIZE, answer, now);
        r.putLong(ConflictRecords.EnvoyAnswer.AGENT, p.agent);
        r.putInt(ConflictRecords.EnvoyAnswer.EPOCH, p.epoch);
        if (player != null) {
            r.putLong(ConflictRecords.EnvoyAnswer.PLAYER_MOST, player.getMostSignificantBits());
            r.putLong(ConflictRecords.EnvoyAnswer.PLAYER_LEAST, player.getLeastSignificantBits());
        }
        session.boundary().commit(r);
        answers++;
    }

    /**
     * A player right-clicks the envoy (EntityInteract, highest; MZB-110). No click refuses: a demand is
     * refused only by a blow (ENVOY_ANSWER 3, {@link #struck}) or runs out at its deadline. The demand's
     * player holding the demanded good pays what is still owed from the main hand, or from the off hand
     * when the main hand is empty (or, on the off hand's own click, holds something else): TRIBUTE_PAID
     * with a Java op id, taken even in creative, and only while the demand is open (not after its
     * deadline, a blow or its settlement). Each such click, and an empty-handed plain click, is answered
     * with the envoy's line, the same as TALK's and to that player only (MZB-122): what is still owed, or
     * why it takes nothing. Crouching with an empty main hand and no good is TALK's own gesture, left to
     * it. A settlers' demand (no player) is only shown. Another player's click on an envoy not sent to
     * them is ordinary. True when the click was the envoy's (cancel the interaction).
     */
    boolean interact(Player player, Entity target, InteractionHand hand, long now) {
        Slot sl = live(target);
        if (sl == null || !sl.envoy || !sl.alive || !(player instanceof ServerPlayer sp) || player instanceof FakePlayer)
            return false;
        Party p = sl.stack.party;
        if (p.player != null && !p.player.equals(player.getUUID()))
            return false;
        if (p.demand == null && p.settled == 0)
            return false;
        Item good = p.demand != null ? p.demand.item : p.good;
        ItemStack main = player.getMainHandItem(), off = player.getOffhandItem();
        boolean mainGood = good != null && main.is(good), offGood = good != null && off.is(good);
        if (hand == InteractionHand.OFF_HAND) {
            // The main hand's click came first and answered an empty or paying main hand.
            if (main.isEmpty() || mainGood)
                return true;
            return offGood && pay(sp, target, p, off, now);
        }
        if (mainGood)
            return pay(sp, target, p, main, now);
        if (!main.isEmpty())
            return false;
        if (offGood)
            return pay(sp, target, p, off, now);
        if (player.isSecondaryUseActive())
            return false; // TALK's gesture: it says the same line
        say(sp, target);
        return true;
    }

    /**
     * Takes what is owed of the good in {@code held} (TRIBUTE_PAID) when the demand is open and to this
     * player, else nothing; either way the envoy says its line (what is still owed, or why it took none).
     * A settlers' demand (no player) takes nothing from a player's hand: the click only shows it.
     */
    private boolean pay(ServerPlayer player, Entity envoy, Party p, ItemStack held, long now) {
        Demand d = p.demand;
        if (p.player == null) {
            say(player, envoy);
            return true;
        }
        if (closed(p, now) != null) {
            paymentsRefused++;
            say(player, envoy);
            return true;
        }
        int n = Math.min(held.getCount(), d.remaining);
        held.shrink(n);
        d.remaining -= n;
        int item = session.itemId(d.item);
        ByteBuffer r = session.boundary().beginJournaled(ConflictRecords.TRIBUTE_PAID, ConflictRecords.TributePaid.size(1), 1, now);
        r.putLong(ConflictRecords.TributePaid.FACTION, p.faction);
        r.putLong(ConflictRecords.TributePaid.AGENT, p.agent);
        r.putInt(ConflictRecords.TributePaid.EPOCH, p.epoch);
        r.putLong(ConflictRecords.TributePaid.PLAYER_MOST, player.getUUID().getMostSignificantBits());
        r.putLong(ConflictRecords.TributePaid.PLAYER_LEAST, player.getUUID().getLeastSignificantBits());
        r.putInt(ConflictRecords.TributePaid.ITEMS + ConflictRecords.I_ITEM, item);
        r.putInt(ConflictRecords.TributePaid.ITEMS + ConflictRecords.I_COUNT, n);
        session.boundary().commitWithOpId(r, ConflictRecords.TributePaid.OP_ID);
        tributes++;
        say(player, envoy);
        return true;
    }

    /** The envoy's line to this player (TALK's limits apply: at most once per {@link com.minecraftindustrialcomplex.mic_rtsim.talk.RateLimit#PAIR} ticks). */
    private void say(ServerPlayer player, Entity envoy) {
        try {
            if (Talk.speak(ConflictTalker.INSTANCE, envoy, player))
                demandsShown++;
        } catch (Throwable e) {
            Talk.failed(ConflictTalker.INSTANCE, "speak", e);
        }
    }

    /** The paid outcome's word, said once the tribute is all paid and before the engine settles. */
    static final String CLOSED_PAID = "paid";

    /**
     * Why an envoy's demand takes nothing now, as its settled line's key ({@code envoy.settled.<key>}),
     * or null while it is open: its settlement's outcome; "struck" after a blow; "other" for a demand of
     * nothing; "expired" from its deadline on; "paid" once its tribute is all paid.
     */
    static String closed(Party p, long now) {
        Demand d = p.demand;
        if (d == null)
            return ConflictTalker.outcomeKey(p.settled);
        if (p.insulted)
            return "struck";
        boolean tribute = (d.flags & ConflictRecords.Envoy.DEMAND_TRIBUTE) != 0 && d.item != null;
        boolean stop = (d.flags & ConflictRecords.Envoy.DEMAND_STOP) != 0;
        if (!tribute && !stop)
            return "other";
        if (now >= d.deadline)
            return "expired";
        if (tribute && d.remaining <= 0)
            return CLOSED_PAID;
        return null;
    }

    /** The envoy is named "<Culture> Envoy" while its demand is open, and no more (MZB-101: its demand is said only to its player). */
    private void nameEnvoy(Party p) {
        Slot envoy = null;
        for (Stack st : p.stacks)
            for (Slot sl : st.slots)
                if (sl.envoy && sl.alive && sl.entity != null)
                    envoy = sl;
        if (envoy == null)
            return;
        Component name = p.demand == null ? null : ConflictTalker.envoyName(envoy.body);
        envoy.entity.setCustomName(name);
        envoy.entity.setCustomNameVisible(name != null);
    }

    static Component signalWord(int signal) {
        String name = ConflictRecords.signalName(signal);
        return Component.translatableWithFallback("mic_rtsim.conflict.signal." + name.replace(' ', '_'), name);
    }

    // ---- territory signals ----------------------------------------------------------------------

    /** The actor a player is to the engine: itself, or the settlers (null) for a machine's fake player. */
    private static UUID actor(Player p) {
        return p instanceof FakePlayer ? null : p.getUUID();
    }

    /** A player broke a block: a log is logging (2), an ore mining (3), inside a territory that minds it. */
    void onBreak(ServerLevel level, Player player, BlockPos pos, net.minecraft.world.level.block.state.BlockState state, long now) {
        if (territories.isEmpty() || player.isSpectator())
            return;
        int signal;
        if (state.is(net.minecraft.tags.BlockTags.LOGS))
            signal = ConflictRecords.SIGNAL_LOGGING;
        else if (state.is(net.neoforged.neoforge.common.Tags.Blocks.ORES))
            signal = ConflictRecords.SIGNAL_MINING;
        else
            return;
        Territories.Territory t = territories.minding(level.dimension(), pos.getX(), pos.getZ(), signal);
        if (t != null)
            count(signal, 0L, actor(player), level.dimension(), pos, t, now);
    }

    /** A player placed a block inside a territory that minds building (5). */
    void onPlace(ServerLevel level, Player player, BlockPos pos, long now) {
        if (territories.isEmpty() || player.isSpectator())
            return;
        Territories.Territory t = territories.minding(level.dimension(), pos.getX(), pos.getZ(), ConflictRecords.SIGNAL_BUILDING);
        if (t != null)
            count(ConflictRecords.SIGNAL_BUILDING, 0L, actor(player), level.dimension(), pos, t, now);
    }

    private void count(int signal, long faction, UUID player, ResourceKey<Level> dim, BlockPos pos, Territories.Territory t, long now) {
        SignalKey k = new SignalKey(faction, signal, player, dim, t.faction());
        signals.computeIfAbsent(k, x -> new SignalBatch(pos.immutable(), now)).count++;
        signalsCounted++;
    }

    /**
     * Whether the engine side is up for M7's train sampler: a live session whose conflict module is on and
     * which takes M1's records (RAIL_PASS is one). Without it the sampler has nothing to say (MZB-320).
     */
    public boolean up() {
        RtsimSession s = session;
        return m1Records && s != null && !s.failed() && RtsimSession.current() == s && RtsimConfig.on(Module.CONFLICT);
    }

    /**
     * RAIL_PASS (M7, MZB-325): a train's pass through {@code faction}'s territory, journaled at once as
     * {@link #flushSignals} journals a batch. {@code owner} is the train's owner (null: none, written (0, 0));
     * the point is a sample on the track, nearest the circle's centre; {@code entry} and {@code exit} are game
     * time. False when the engine side is not up ({@link #up}).
     */
    public boolean railPass(long faction, long train, UUID owner, ResourceKey<Level> dimension, double x, double y, double z,
                            long entry, long exit) {
        if (!up())
            return false;
        RtsimSession s = session;
        int dim = s.dimId(dimension); // before beginJournaled: a define may journal
        ByteBuffer r = s.boundary().beginJournaled(ConflictRecords.RAIL_PASS, ConflictRecords.RailPass.SIZE, 0, s.gameTime());
        r.putLong(ConflictRecords.RailPass.FACTION, faction);
        r.putLong(ConflictRecords.RailPass.TRAIN, train);
        if (owner != null) {
            r.putLong(ConflictRecords.RailPass.OWNER_MOST, owner.getMostSignificantBits());
            r.putLong(ConflictRecords.RailPass.OWNER_LEAST, owner.getLeastSignificantBits());
        }
        r.putInt(ConflictRecords.RailPass.DIM, dim);
        r.putDouble(ConflictRecords.RailPass.X, x);
        r.putDouble(ConflictRecords.RailPass.Y, y);
        r.putDouble(ConflictRecords.RailPass.Z, z);
        r.putLong(ConflictRecords.RailPass.ENTRY, entry);
        r.putLong(ConflictRecords.RailPass.EXIT, exit);
        s.boundary().commit(r);
        railPasses++;
        return true;
    }

    /** Journals each batch {@value #SIGNAL_FLUSH} ticks after its first event (all of them when {@code all}). */
    private void flushSignals(long now, boolean all) {
        if (signals.isEmpty() || session == null)
            return;
        for (Iterator<Map.Entry<SignalKey, SignalBatch>> it = signals.entrySet().iterator(); it.hasNext(); ) {
            var e = it.next();
            SignalBatch b = e.getValue();
            if (!all && now - b.since < SIGNAL_FLUSH)
                continue;
            it.remove();
            SignalKey k = e.getKey();
            int dim = session.dimId(k.dimension());
            ByteBuffer r = session.boundary().beginJournaled(ConflictRecords.TERRITORY_SIGNAL, ConflictRecords.TerritorySignal.SIZE, k.signal(), now);
            r.putLong(ConflictRecords.TerritorySignal.FACTION, k.faction());
            r.putInt(ConflictRecords.TerritorySignal.DIM, dim);
            r.putInt(ConflictRecords.TerritorySignal.COUNT, b.count);
            if (k.player() != null) {
                r.putLong(ConflictRecords.TerritorySignal.PLAYER_MOST, k.player().getMostSignificantBits());
                r.putLong(ConflictRecords.TerritorySignal.PLAYER_LEAST, k.player().getLeastSignificantBits());
            }
            r.putInt(ConflictRecords.TerritorySignal.X, b.first.getX());
            r.putInt(ConflictRecords.TerritorySignal.Y, b.first.getY());
            r.putInt(ConflictRecords.TerritorySignal.Z, b.first.getZ());
            session.boundary().commit(r);
            signalsJournaled++;
        }
    }

    // ---- notices --------------------------------------------------------------------------------

    private static ServerPlayer playerIn(ServerLevel level, UUID id) {
        for (ServerPlayer p : level.players())
            if (p.getUUID().equals(id))
                return p;
        return null;
    }

    /**
     * Plays each kept notice once its player is in range, then forgets it. A settlers' notice (no player)
     * plays to every player in range on the tick the first one comes in range (MZB-118).
     */
    private void checkNotices(long now) {
        if (notices.isEmpty())
            return;
        for (Iterator<Notice> it = notices.values().iterator(); it.hasNext(); ) {
            Notice n = it.next();
            ServerLevel level = level(n.dimension());
            if (level == null)
                continue;
            boolean played = false;
            if (n.player() != null) {
                ServerPlayer p = playerIn(level, n.player());
                if (p != null && !p.isSpectator() && inRange(n, p)) {
                    play(n, p);
                    played = true;
                }
            } else {
                for (ServerPlayer p : level.players())
                    if (!p.isSpectator() && !(p instanceof FakePlayer) && inRange(n, p)) {
                        play(n, p);
                        played = true;
                        noticeHearers++;
                    }
            }
            if (!played)
                continue;
            it.remove();
            noticesPlayed++;
            shown(n, now);
            ConflictNews.post(ConflictNews.Event.NOTICE_PLAYED, n.faction(), n.player());
        }
    }

    private boolean inRange(Notice n, ServerPlayer p) {
        Territories.Territory t = territories.get(n.faction());
        if (t != null && t.dimension().equals(n.dimension()) && t.contains(Mth.floor(p.getX()), Mth.floor(p.getZ())))
            return true;
        return Math.hypot(p.getX() - n.x(), p.getZ() - n.z()) <= NOTICE_RANGE;
    }

    /** The camp's sound (a raid horn when it gave none), {@value #NOTICE_DISTANCE} blocks from the player toward the camp, to that player only. */
    private void play(Notice n, ServerPlayer to) {
        playToward(n.faction(), n.dimension(), n.x() + 0.5, n.z() + 0.5, to);
    }

    /**
     * The camp's sound ({@code faction}'s, a raid horn when it gave none) {@value #NOTICE_DISTANCE} blocks from
     * {@code to} toward the camp, at {@value #NOTICE_VOLUME}: a notice's, and a train's roar (MZB-366). The camp
     * is the faction's declared one in {@code dimension}, else its territory's centre, else (fx, fz).
     */
    private void playToward(long faction, ResourceKey<Level> dimension, double fx, double fz, ServerPlayer to) {
        Camp c = camps.get(faction);
        Territories.Territory t = territories.get(faction);
        double cx, cz;
        if (c != null && c.dimension().equals(dimension)) {
            cx = c.pos().getX() + 0.5;
            cz = c.pos().getZ() + 0.5;
        } else if (t != null) {
            cx = t.x() + 0.5;
            cz = t.z() + 0.5;
        } else {
            cx = fx;
            cz = fz;
        }
        double dx = cx - to.getX(), dz = cz - to.getZ();
        double d = Math.hypot(dx, dz);
        double reach = Math.min(NOTICE_DISTANCE, d);
        double x = d > 1e-6 ? to.getX() + dx / d * reach : to.getX();
        double z = d > 1e-6 ? to.getZ() + dz / d * reach : to.getZ();
        Holder<SoundEvent> sound = c != null && c.sound() != null ? c.sound() : SoundEvents.RAID_HORN;
        to.connection.send(new ClientboundSoundPacket(sound, SoundSource.HOSTILE, x, to.getEyeY(), z, NOTICE_VOLUME, 1.0f,
                to.getRandom().nextLong()));
    }

    /** NOTICE_SHOWN (M1): the engine forgets a kept notice when the serial matches. */
    private void shown(Notice n, long now) {
        if (!m1Records)
            return;
        ByteBuffer r = session.boundary().beginJournaled(ConflictRecords.NOTICE_SHOWN, ConflictRecords.NoticeShown.SIZE, n.serial(), now);
        r.putLong(ConflictRecords.NoticeShown.FACTION, n.faction());
        if (n.player() != null) {
            r.putLong(ConflictRecords.NoticeShown.PLAYER_MOST, n.player().getMostSignificantBits());
            r.putLong(ConflictRecords.NoticeShown.PLAYER_LEAST, n.player().getLeastSignificantBits());
        }
        session.boundary().commit(r);
    }

    /** DIFFICULTY (M1, journaled at session start and on each change): the engine sends no war party on Peaceful. */
    private void sendDifficulty(long now) {
        if (!m1Records || session == null || difficulty == null)
            return;
        ByteBuffer r = session.boundary().beginJournaled(ConflictRecords.DIFFICULTY, ConflictRecords.DifficultyRecord.SIZE, 0, now);
        r.putInt(ConflictRecords.DifficultyRecord.DIFFICULTY, difficulty.getId());
        session.boundary().commit(r);
    }

    // ---- lying in wait (M8; MOWZIE.md §4.4, MZB-359 to 368) --------------------------------------

    /**
     * AMBUSH_SPOT. REMOVED drops the agent's lurk and lets its puppets go; otherwise the lurk replaces any the
     * agent had (latest wins: a session start's resend, a watch's new epoch), and a party already embodied
     * stands at the spot at once. Keyed by agent, never by epoch: a lurk the engine saw has its passes carry
     * this epoch against the party's next, and its REMOVED goes out at the party's.
     */
    private void ambushSpot(ByteBuffer b, int at, int flags, EngineNames names, long now) {
        long agent = b.getLong(at + ConflictRecords.AmbushSpot.AGENT);
        if (!ConflictRecords.isConflictAgent(agent)) {
            malformed++;
            return;
        }
        if ((flags & ConflictRecords.AmbushSpot.REMOVED) != 0) {
            endLurk(agent);
            return;
        }
        String dimName = names.name(Obs.DICT_DIMENSION, b.getInt(at + ConflictRecords.AmbushSpot.DIM));
        ResourceLocation loc = dimName == null ? null : ResourceLocation.tryParse(dimName);
        Vec3 track = new Vec3(b.getDouble(at + ConflictRecords.AmbushSpot.TRACK_X), b.getDouble(at + ConflictRecords.AmbushSpot.TRACK_Y),
                b.getDouble(at + ConflictRecords.AmbushSpot.TRACK_Z));
        Vec3 spot = new Vec3(b.getDouble(at + ConflictRecords.AmbushSpot.SPOT_X), b.getDouble(at + ConflictRecords.AmbushSpot.SPOT_Y),
                b.getDouble(at + ConflictRecords.AmbushSpot.SPOT_Z));
        float reach = b.getFloat(at + ConflictRecords.AmbushSpot.REACH);
        if (loc == null || !finite(track) || !finite(spot) || !Float.isFinite(reach) || reach <= 0) {
            malformed++;
            return;
        }
        Lurk l = new Lurk(agent, b.getInt(at + ConflictRecords.AmbushSpot.EPOCH), b.getLong(at + ConflictRecords.AmbushSpot.FACTION),
                ResourceKey.create(Registries.DIMENSION, loc), track, spot, reach);
        Lurk old = lurks.put(agent, l);
        lurksKept++;
        Party p = byAgent.get(agent);
        if (p == null || p.recalling)
            return;
        boolean moved = old == null || !old.spot().equals(l.spot());
        for (Stack st : p.stacks) {
            if (st.state == StackState.GONE)
                continue;
            anchorAtSpot(st, l, now);
            if (moved)
                for (Slot sl : st.slots)
                    releaseLurk(sl); // the next hold restricts it to the new spot and finds its floor
        }
    }

    private static boolean finite(Vec3 v) {
        return Double.isFinite(v.x) && Double.isFinite(v.y) && Double.isFinite(v.z);
    }

    /** A lurk ended: dropped by agent, and its puppets let go (the engine sends the steer or the recall after this). */
    private void endLurk(long agent) {
        if (lurks.remove(agent) != null)
            lurksRemoved++;
        Party p = byAgent.get(agent);
        if (p != null)
            releaseLurk(p);
    }

    /** A stack of a lurking party stands at the spot, its heading toward the rail, in place (the puppets walk there). */
    private static void anchorAtSpot(Stack st, Lurk l, long now) {
        st.ax = st.ex = l.spot().x;
        st.az = st.ez = l.spot().z;
        st.t0 = st.t1 = now;
        st.offX = st.offZ = 0.0;
        double dx = l.track().x - l.spot().x, dz = l.track().z - l.spot().z;
        double d = Math.hypot(dx, dz);
        if (d > 1e-6) {
            st.hx = dx / d;
            st.hz = dz / d;
        }
    }

    /**
     * The y a body of {@code type} stands at at (x, z) beside a lurk's track point (MZB-360): the highest block from
     * the track's y + 4 down to its y - 8 with a sturdy top face and the body's box free above it, else the
     * motion-blocking height. The spot's own y is the track's, which may be in the air or a cut.
     */
    private double floorAt(ServerLevel level, Lurk l, double x, double z, EntityType<?> type) {
        int bx = Mth.floor(x), bz = Mth.floor(z);
        if (!level.hasChunk(bx >> 4, bz >> 4))
            return l.spot().y;
        int top = Mth.floor(l.track().y) + 4, bottom = Mth.floor(l.track().y) - 8;
        BlockPos.MutableBlockPos at = new BlockPos.MutableBlockPos();
        for (int by = top; by >= bottom; by--) {
            at.set(bx, by, bz);
            if (!level.getBlockState(at).isFaceSturdy(level, at, Direction.UP))
                continue;
            double sy = by + 1;
            if (level.noCollision(type.getSpawnAABB(x, sy, z)))
                return sy;
        }
        return level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, bx, bz);
    }

    /** How far above or below the rail's y a held body's feet may stand (blocks): the second re-check's P3, embankments and cuttings. */
    static final double STAND_RISE = 2.0;

    /**
     * Like {@link #floorAt} for a point a body is to be held at, with no fallback: NaN when the point's chunk does not
     * tick entities, or no block with a sturdy top face whose top is within {@link #STAND_RISE} of the track's y has the
     * body's box free above it.
     */
    private double standAt(ServerLevel level, Lurk l, double x, double z, EntityType<?> type) {
        int bx = Mth.floor(x), bz = Mth.floor(z);
        if (!RtsimSession.entitiesTickAt(level, new BlockPos(bx, level.getSeaLevel(), bz)))
            return Double.NaN;
        // Standing y within STAND_RISE of the rail's: a rim of a cutting or the crown of an embankment is another level,
        // out of the contract's reach (the strike's reach is horizontal) and a long walk from the rail
        int top = Mth.floor(l.track().y + STAND_RISE) - 1, bottom = Mth.ceil(l.track().y - STAND_RISE) - 1;
        BlockPos.MutableBlockPos at = new BlockPos.MutableBlockPos();
        for (int by = top; by >= bottom; by--) {
            at.set(bx, by, bz);
            if (!level.getBlockState(at).isFaceSturdy(level, at, Direction.UP))
                continue;
            double sy = by + 1;
            if (level.noCollision(type.getSpawnAABB(x, sy, z)))
                return sy;
        }
        return Double.NaN;
    }

    /**
     * Where the puppet of {@code sl} is held (M8, MZB-360, the re-check's P2): with a rail direction known at the lurk's
     * track point ({@link LurkSpots}), the first standable block centre on the rail's normal at the spot distance, on
     * the den's side and then the other, plus the slot's offset in its stack; null when neither side has one (the
     * lurk is refused). With none known (no plugin, no rail block at the point, a crossing, a curve's middle) the
     * engine's spot, as before, unless a rail lies within the body's half-width and the rail clearance of it (refused).
     */
    private Vec3 holdPoint(ServerLevel level, Lurk l, Slot sl) {
        double ox = sl.placeX(1.0) - l.spot().x, oz = sl.placeZ(1.0) - l.spot().z;
        RailAxes rails = LurkSpots.axes();
        Vec3 axis = rails == null ? null : rails.at(level, l.track());
        if (axis == null) {
            double x = sl.placeX(1.0), z = sl.placeZ(1.0);
            // No direction known (a curve's middle, a crossing, no plugin): the engine's spot stands, unless a rail lies where
            // the body's box would reach it (the second re-check's P3): half the body's width and the rail clearance from
            // the spot, 3.5 blocks for the 4-wide Frostmaw, in the blocks from the floor the body stands on up to its head
            // (the third check's P3: a rail below the floor is not in its way). Refused, as when no side has a floor.
            double y = floorAt(level, l, x, z, sl.type);
            if (rails != null && rails.trackNear(level, new Vec3(x, y, z), sl.type.getWidth() / 2.0 + LurkGeometry.RAIL_CLEARANCE, sl.type.getHeight()))
                return null;
            return new Vec3(x, y, z);
        }
        for (Vec3 c : LurkSpots.candidates(l.track(), l.spot(), axis)) {
            double x = c.x + ox, z = c.z + oz, y = standAt(level, l, x, z, sl.type);
            if (!Double.isNaN(y))
                return new Vec3(x, y, z);
        }
        return null;
    }

    /** Where the lurk puppet of {@code agent} is held, or null while it is not (M8; GameTests only, through {@code ConflictKit.lurkHold}). */
    public Vec3 lurkHold(long agent) {
        Party p = byAgent.get(agent);
        if (p == null)
            return null;
        for (Stack st : p.stacks)
            for (Slot sl : st.slots)
                if (sl.holdAt != null && sl.entity != null && !sl.entity.isRemoved())
                    return sl.holdAt;
        return null;
    }

    /**
     * The hold tick, on {@code ServerTickEvent.Post} (after every level ticked, so a path set here is the one an
     * awake mob follows next tick, MZB-361): each live puppet of a lurking party that stands in the world is held
     * by its {@link Lurker}, and every {@value #LURK_TARGET_CHECK} ticks given a target (MZB-362).
     */
    private void holdLurks(long now) {
        if (lurks.isEmpty())
            return;
        boolean look = now % LURK_TARGET_CHECK == 0;
        for (Lurk l : lurks.values()) {
            Party p = byAgent.get(l.agent());
            if (p == null || p.recalling)
                continue;
            ServerLevel level = level(p.dimension);
            if (level == null)
                continue;
            for (Stack st : p.stacks) {
                if (st.state != StackState.PLACED)
                    continue;
                for (Slot sl : st.slots) {
                    Mob m = sl.entity;
                    if (!sl.alive || sl.pendingDeath != null || m == null || m.isRemoved() || m.level() != level)
                        continue;
                    if (sl.holdAt == null) {
                        Vec3 at = holdPoint(level, l, sl);
                        if (at == null) {
                            // No standable point on either side of the rail: the body is taken out of the world and made again
                            // after the requeue cooldown (and refused again if the world is the same). Only a body that stood
                            // before its lurk was known, or whose spot moved, gets here: a stack placed by `spawn` has its hold point (or was refused
                            // there, before a camp body was claimed), the second re-check's P3
                            lurksRefused++;
                            despawn(sl);
                            st.retryAt = Math.max(st.retryAt, now + REQUEUE_COOLDOWN);
                            continue;
                        }
                        sl.holdAt = at;
                    }
                    if (sl.lurkMemo == null)
                        sl.lurkMemo = new Lurker.Memo();
                    Lurkers.of(m).hold(m, sl.holdAt, sl.lurkMemo);
                    if (look)
                        aim(m, level);
                }
            }
        }
    }

    /**
     * The restriction gates a mob's own targeting (M6, MZB-279), so a held puppet never picks a player up by
     * itself: with no target it takes the nearest player in {@value #LURK_TARGET_RANGE} blocks, and it drops one
     * that died, left the level or went out of view.
     */
    private void aim(Mob m, ServerLevel level) {
        LivingEntity t = m.getTarget();
        if (t != null && (!t.isAlive() || t.level() != level || m.distanceToSqr(t) > OUT_OF_VIEW * OUT_OF_VIEW)) {
            m.setTarget(null);
            t = null;
        }
        if (t != null)
            return;
        Player pl = level.getNearestPlayer(TargetingConditions.forCombat().range(LURK_TARGET_RANGE), m);
        if (pl != null)
            m.setTarget(pl);
    }

    /**
     * A puppet let go: its {@link Lurker} undoes the hold and empties its memo. A body not held is not asked; one that is
     * out of the world (killed, taken, discarded) is, since the Lurker's state for it must go too (the second re-check's
     * P2: a skipped release left a killed puppet and its goals in the plugin for the life of the server).
     */
    private void releaseLurk(Slot sl) {
        if (sl.holdAt == null && sl.lurkMemo == null)
            return;
        Lurker.Memo memo = sl.lurkMemo != null ? sl.lurkMemo : new Lurker.Memo();
        sl.holdAt = null;
        sl.lurkMemo = null;
        Mob m = sl.entity;
        if (m != null)
            Lurkers.of(m).release(m, memo);
    }

    private void releaseLurk(Party p) {
        for (Stack st : p.stacks)
            for (Slot sl : st.slots)
                releaseLurk(sl);
    }

    /** TRAIN_ROBBED: the train waits for its next station's roar, and a lurk puppet of the faction roars now (MZB-366, 368). */
    private void trainRobbed(ByteBuffer b, int at) {
        long faction = b.getLong(at + ConflictRecords.TrainRobbed.FACTION);
        long train = b.getLong(at + ConflictRecords.TrainRobbed.TRAIN);
        pendingRoars.remove(train); // a train robbed again waits as the newest
        pendingRoars.put(train, faction);
        while (pendingRoars.size() > PENDING_ROARS) {
            Iterator<Long> oldest = pendingRoars.keySet().iterator();
            oldest.next();
            oldest.remove();
        }
        for (Lurk l : lurks.values()) {
            if (l.faction() != faction)
                continue;
            Party p = byAgent.get(l.agent());
            if (p == null || p.recalling)
                continue;
            for (Stack st : p.stacks)
                for (Slot sl : st.slots) {
                    Mob m = sl.entity;
                    if (sl.alive && sl.holdAt != null && sl.pendingDeath == null && m != null && !m.isRemoved()) {
                        Lurkers.of(m).roar(m);
                        roarsTold++;
                    }
                }
        }
    }

    /**
     * A train called at a station (the create plugin's arrival): when a lurk robbed it, every player within
     * {@value #ROAR_RANGE} blocks hears the camp's sound toward the den (MZB-366, as a notice's). The entry goes only
     * once a player heard it, so an arrival with nobody near keeps it for the next. A train not pending returns at once.
     */
    public void trainCalled(long train, ServerLevel level, BlockPos station) {
        if (pendingRoars.isEmpty() || !RtsimConfig.on(Module.CONFLICT)) // a module that is off plays nothing (MZB-359)
            return;
        Long faction = pendingRoars.get(train);
        if (faction == null)
            return;
        double cx = station.getX() + 0.5, cz = station.getZ() + 0.5, r2 = ROAR_RANGE * ROAR_RANGE;
        boolean heard = false;
        for (ServerPlayer p : level.players()) {
            if (p.isSpectator() || p instanceof FakePlayer)
                continue;
            double dx = p.getX() - cx, dy = p.getY() - (station.getY() + 0.5), dz = p.getZ() - cz;
            if (dx * dx + dy * dy + dz * dz > r2)
                continue;
            playToward(faction, level.dimension(), cx, cz, p);
            roarHearers++;
            heard = true;
        }
        if (heard) {
            pendingRoars.remove(train);
            roarsPlayed++;
        }
    }

    /**
     * TRAIN_PASSED (M8, MZB-363), journaled at once: the lurk of {@code agent} (the table's {@code epoch}) had
     * {@code train} pass within its reach, {@code carriage} the index of the carriage nearest the track point;
     * {@code watched} when a puppet of the lurk stands in the world. False when the engine side is not up.
     */
    public boolean trainPassed(long agent, int epoch, boolean watched, int carriage, long train) {
        if (!up())
            return false;
        RtsimSession s = session;
        ByteBuffer r = s.boundary().beginJournaled(ConflictRecords.TRAIN_PASSED, ConflictRecords.TrainPassed.SIZE,
                watched ? ConflictRecords.TrainPassed.WATCHED : 0, s.gameTime());
        r.putLong(ConflictRecords.TrainPassed.AGENT, agent);
        r.putInt(ConflictRecords.TrainPassed.EPOCH, epoch);
        r.putInt(ConflictRecords.TrainPassed.CARRIAGE, carriage);
        r.putLong(ConflictRecords.TrainPassed.TRAIN, train);
        s.boundary().commit(r);
        trainPasses++;
        return true;
    }

    /** The lurk table, by agent: an unmodifiable view, read by the create plugin's sampler (MZB-359). */
    public Map<Long, Lurk> lurks() {
        return lurksView;
    }

    /** The live puppet of a lurking agent that stands in the world and is not recalled, or null (MZB-359, 364). */
    public Mob lurkBody(long agent) {
        Party p = byAgent.get(agent);
        if (p == null || p.recalling)
            return null;
        for (Stack st : p.stacks)
            for (Slot sl : st.slots) {
                Mob m = sl.entity;
                if (sl.alive && sl.pendingDeath == null && m != null && !m.isRemoved() && m.isAlive())
                    return m;
            }
        return null;
    }

    /**
     * A train's carriage point is in reach of the lurk puppet: the held puppet of the agent nearest to {@code at}
     * starts its strike through its {@link Lurker}, given the point's motion per tick. True when one started
     * (counted in {@link #lurkStrikes}).
     */
    public boolean strike(long agent, Vec3 at, Vec3 motion) {
        Party p = byAgent.get(agent);
        if (p == null || p.recalling)
            return false;
        Mob best = null;
        double bd = Double.MAX_VALUE;
        for (Stack st : p.stacks)
            for (Slot sl : st.slots) {
                Mob m = sl.entity;
                if (!sl.alive || sl.holdAt == null || sl.pendingDeath != null || m == null || m.isRemoved() || !m.isAlive())
                    continue;
                double d = m.distanceToSqr(at);
                if (d < bd) {
                    bd = d;
                    best = m;
                }
            }
        if (best == null || !Lurkers.of(best).strike(best, at, motion))
            return false;
        lurkStrikes++;
        return true;
    }

    /** Trains waiting for their station's roar (GameTests). */
    public int pendingRoars() {
        return pendingRoars.size();
    }

    public boolean roarPending(long train) {
        return pendingRoars.containsKey(train);
    }

    // ---- camps and loot (plugins) ---------------------------------------------------------------

    /**
     * A plugin found a camp (MOWZIE.md §4.9): FACTION_FOUND goes to the engine (when the conflict module
     * is on), which joins the camp's culture to the world if a culture claims {@code structure}. {@code pos}
     * is a block in the structure start's chunk, whose position keys the faction ({@link Obs#villageId}).
     * {@code kin} tells which deaths in its territory are the camp's own (a player's kill of one is
     * killing, 4); {@code sound} is its notices' sound (null: a raid horn). {@code homeBodies} and
     * {@code census} ({@link ConflictRecords.FactionFound#CENSUS_CHIEF}, {@code CENSUS_CRYSTAL}) are the
     * census at discovery. Returns the faction key, or 0 with no live session.
     */
    public long reportCamp(ServerLevel level, String structure, BlockPos pos, Predicate<Entity> kin, Holder<SoundEvent> sound,
                           boolean late, int homeBodies, int census) {
        RtsimSession s = session;
        if (s == null || s.failed() || RtsimSession.current() != s)
            return 0L;
        int dim = s.dimId(level.dimension());
        long key = Obs.villageId(dim, pos.getX() >> 4, pos.getZ() >> 4);
        camps.put(key, new Camp(key, level.dimension(), pos.immutable(), structure, kin, sound));
        campsReported++;
        if (!RtsimConfig.on(Module.CONFLICT))
            return key;
        journalFound(key, level, structure, pos, late, homeBodies, census, s.gameTime());
        return key;
    }

    /** FACTION_FOUND for a camp at {@code pos} (a block in its start chunk). */
    private void journalFound(long key, ServerLevel level, String structure, BlockPos pos, boolean late, int homeBodies, int census,
                              long now) {
        RtsimSession s = session;
        int dim = s.dimId(level.dimension());
        int structureId = s.dictId(Obs.DICT_STRUCTURE, structure); // before beginJournaled: a define may journal
        ByteBuffer r = s.boundary().beginJournaled(ConflictRecords.FACTION_FOUND, ConflictRecords.FactionFound.SIZE,
                late ? ConflictRecords.FactionFound.FLAG_LATE : 0, now);
        r.putLong(ConflictRecords.FactionFound.FACTION, key);
        r.putInt(ConflictRecords.FactionFound.DIM, dim);
        r.putInt(ConflictRecords.FactionFound.STRUCTURE, structureId);
        r.putInt(ConflictRecords.FactionFound.X, pos.getX());
        r.putInt(ConflictRecords.FactionFound.Y, pos.getY());
        r.putInt(ConflictRecords.FactionFound.Z, pos.getZ());
        r.putShort(ConflictRecords.FactionFound.HOME_BODIES, (short) Mth.clamp(homeBodies, 0, 0xFFFF));
        r.put(ConflictRecords.FactionFound.CENSUS, (byte) census);
        s.boundary().commit(r);
    }

    /** A camp the reconciler declared: its kin and sound as a reported camp's (the killing signal, notices). */
    void putCamp(Camps.Site c) {
        camps.put(c.faction, new Camp(c.faction, c.dimension, c.pos, c.structure, c.kind.kin(), c.kind.sound()));
        campsReported++;
    }

    /** A declared camp's FACTION_FOUND, at its first census of the session (MZB-206). */
    void factionFound(Camps.Site c, ServerLevel level, int homeBodies, int census, long now) {
        journalFound(c.faction, level, c.structure, c.pos, c.late, homeBodies, census, now);
    }

    /** A party's live puppet of {@code body}, for a hand-back; null with none. */
    Slot puppetOf(Party p, String body) {
        for (Stack st : p.stacks)
            for (Slot sl : st.slots)
                if (sl.alive && sl.pendingDeath == null && !sl.removing && sl.entity != null && !sl.entity.isRemoved()
                        && sl.entity.isAlive() && sl.body.equals(body))
                    return sl;
        return null;
    }

    /** A puppet handed back to its camp: its body goes and the member is the party's no more (the camp's body stands for it). */
    void handBack(Slot sl) {
        despawn(sl);
        sl.alive = false;
        sl.removing = true;
    }

    /** A member carried items off from a container (BAND_LOOTED, a Java op id). False when it is no live member. */
    public boolean looted(Entity member, List<ItemStack> items) {
        Slot sl = live(member);
        if (sl == null || !sl.alive || session == null || items.isEmpty())
            return false;
        List<ItemStack> real = new ArrayList<>();
        for (ItemStack it : items)
            if (!it.isEmpty())
                real.add(it);
        if (real.isEmpty())
            return false;
        int n = real.size();
        int[] ids = new int[n];
        for (int i = 0; i < n; i++)
            ids[i] = session.itemId(real.get(i).getItem());
        long now = session.gameTime();
        ByteBuffer r = session.boundary().beginJournaled(ConflictRecords.BAND_LOOTED, ConflictRecords.Looted.size(n), n, now);
        r.putLong(ConflictRecords.Looted.AGENT, sl.stack.party.agent);
        r.putInt(ConflictRecords.Looted.EPOCH, sl.stack.party.epoch);
        for (int i = 0; i < n; i++) {
            int o = ConflictRecords.Looted.ITEMS + i * ConflictRecords.ITEM;
            r.putInt(o + ConflictRecords.I_ITEM, ids[i]);
            r.putInt(o + ConflictRecords.I_COUNT, real.get(i).getCount());
        }
        session.boundary().commitWithOpId(r, ConflictRecords.Looted.OP_ID);
        looted++;
        return true;
    }

    // ---- reads for plugins and GameTests --------------------------------------------------------

    /** The factions' territories (M7's train sampler reads them; never change them). */
    public Territories territories() {
        return territories;
    }

    public Party party(long agent) {
        return byAgent.get(agent);
    }

    public List<Party> parties() {
        return List.copyOf(byAgent.values());
    }

    /** The live bodies of a party, stack by stack. */
    public List<Mob> bodies(long agent) {
        Party p = byAgent.get(agent);
        List<Mob> out = new ArrayList<>();
        if (p != null)
            for (Stack st : p.stacks)
                out.addAll(st.bodies());
        return out;
    }

    /** The slot a live body fills, or null. */
    public Slot slotOf(Entity e) {
        return live(e);
    }

    public int noticesKept() {
        return notices.size();
    }

    public int watchedChunks() {
        int n = 0;
        for (Map<Long, Watch> w : watches.values())
            n += w.size();
        return n;
    }

    public boolean hasCamp(long faction) {
        return camps.containsKey(faction);
    }

    /** Whether the bridge holds anything (D1: nothing without a faction). */
    public boolean idle() {
        return byAgent.isEmpty() && territories.isEmpty() && notices.isEmpty() && signals.isEmpty() && lurks.isEmpty()
                && pendingRoars.isEmpty();
    }
}
