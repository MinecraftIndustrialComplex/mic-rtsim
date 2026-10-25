package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Lurk;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Territories;
import com.simibubi.create.Create;
import com.simibubi.create.content.trains.entity.Carriage;
import com.simibubi.create.content.trains.entity.CarriageContraptionEntity;
import com.simibubi.create.content.trains.entity.Train;
import com.simibubi.create.content.trains.entity.TravellingPoint;
import com.simibubi.create.content.trains.graph.TrackGraph;
import com.simibubi.create.content.trains.station.GlobalStation;
import net.minecraft.core.BlockPos;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.server.ServerLifecycleHooks;

import java.util.ArrayList;
import java.util.BitSet;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.function.LongConsumer;

/**
 * M7's train sampler (MOWZIE.md §9, docs/choices-mowzie-build.md MZB-320 to 326): where Create trains pass
 * through a territory that minds the rail (a Frostmaw's den, TERRITORY mask bit 6), as RAIL_PASS records the
 * engine's rail meter counts.
 *
 * <p>Once a second ({@code gameTime % 20 == 0}) it files every train of {@code Create.RAILWAYS.trains} in a
 * bucket of {@link #PERIOD}, by {@link CreateIds#of} its id; each tick it samples one bucket, so each train
 * is sampled every 20 ticks (10 trains a tick at 200). A sample is the leading point of the first carriage
 * on its edge ({@code TravellingPoint.getPosition}), so a curve is read where the track is and not by the
 * chord between two stations. The segment from the train's last sample to this one is tested against the
 * territories ({@link Territories#crossing}): a circle it meets opens a pass for the train, which keeps
 * the sample nearest the circle's centre (only samples, so the point is on the track), and a pass is sent,
 * with {@code exit} the sample's game time, at the first sample beyond the circle's radius plus
 * {@link #MARGIN} (MZB-390). A segment across dimensions, longer than {@link #MAX_SEGMENT} blocks (a portal,
 * a wrapped world's seam, a teleport), or of a train that derailed or has no graph, is not tested: it ends
 * the train's passes at its last sample and the next sample starts a fresh segment.
 *
 * <p><b>Lurks (M8, MZB-363 to 366).</b> The same samples feed the lurk pass: for each lurk of the conflict
 * engine in the train's dimension ({@link RtsimApi#lurks}), a segment within the lurk's {@code reach} of its
 * track point sends one TRAIN_PASSED per approach (armed again beyond {@link #APPROACH_RESET} blocks), the
 * engine rolls the robbery, and a train whose sample is within that distance of a live lurk puppet joins a
 * near set that is read every tick: a carriage whose living rider (else whose bogey point) will be within
 * {@link #STRIKE_REACH} blocks of the puppet when the swing lands, {@link #SWING_TICKS} ticks on, calls
 * {@link RtsimApi#strike} once per carriage per approach. {@link #arrived} is the mixin's call at a
 * station, for the roar of a robbed train. The sampler runs for a lurk without a territory minding the rail.
 *
 * <p>D1: with no territory that minds the rail and no lurk the tick is two reads; a sample reads at most the cells
 * of its segment's box (two blocks of track at most 128 long, so at most four) and a short list, allocating
 * only the position {@code Vec3} and a pass at its start. D3: no entity. D4: Create code that reaches the
 * core only through {@link RtsimApi}. UR-1: it keeps nothing in the world (MZB-326); its state is dropped
 * when the engine side goes (a pass open then is lost) and when the server stops.
 */
public final class TrainSampler {
    private TrainSampler() {}

    /** Ticks between one train's samples, and the number of buckets (MZB-321). */
    public static final int PERIOD = 20;
    /** A segment longer than this, in blocks, is not tested (MZB-322). */
    public static final double MAX_SEGMENT = 128.0;
    /**
     * How far beyond a circle's radius, in blocks, a train may stray before its pass ends (MZB-390): the
     * distance a train at Create's default powered top speed (2 blocks a tick) covers in one sample period, so a
     * train inside the circle cannot reach the margin's far side by its next sample, and a track along the
     * edge, or a train reversing on it, is one pass and not one for each swap in and out.
     */
    public static final double MARGIN = 2.0 * PERIOD;
    /** A train beyond this many blocks of a lurk's track point may send TRAIN_PASSED again, and strikes again (MZB-363, 365). */
    public static final double APPROACH_RESET = 64.0;
    /**
     * A carriage's rider or bogey point that will be this close (blocks, horizontal) to a lurk puppet when the swing
     * lands is struck at: the swipe's range of 6.5 less what its 3D, feet-to-feet test adds for a rider seated above
     * the puppet's floor (MZB-365, second amendment of review C).
     */
    public static final double STRIKE_REACH = 6.0;
    /**
     * Mirror of {@code SPOT_BLOCKS} in the engine's {@code crates/rtsim-conflict/src/engine/rail.rs}: how far from the
     * track point (blocks, toward the den, whole blocks) the engine puts a lurking body's spot. The engine owns the
     * number and a Rust test pins it (lurk.rs); change both, and the contract below, together (MZB-374).
     */
    public static final double SPOT_BLOCKS = 5.0;
    /** The nearest a lurking body's box may come to the rail's centre line (blocks): clear of the carriages' boxes (MZB-370). */
    public static final double RAIL_CLEARANCE = 1.5;

    /**
     * The spot-distance contract (MZB-374, one rule in two inequalities), for a Lurker that rests its body within
     * {@code stand} blocks of the spot and whose body is {@code halfWidth} blocks across to its side: the far side of
     * its stand is still inside the strike reach ({@code SPOT_BLOCKS + stand <= STRIKE_REACH}) and the near side of its
     * box is still off the rail ({@code SPOT_BLOCKS - stand - halfWidth >= RAIL_CLEARANCE}). Null when both hold, else
     * the one that fails, as a sentence.
     */
    public static String spotContract(double stand, double halfWidth) {
        double far = SPOT_BLOCKS + stand, near = SPOT_BLOCKS - stand - halfWidth;
        if (far > STRIKE_REACH)
            return "the far side of the stand is " + far + " blocks from the track point, past the strike reach (" + STRIKE_REACH + ")";
        if (near < RAIL_CLEARANCE)
            return "the near edge of the body at the near side of the stand is " + near + " blocks from the rail's centre line (" + RAIL_CLEARANCE + " needed)";
        return null;
    }
    /**
     * Ticks from a strike's call to the blow landing: the jar's swipe hits at animation tick 9
     * ({@code FrostmawLurker.SWIPE_HIT_TICK}, which turns the puppet to {@code at + motion * 9}). The create project
     * names no Mowzie class, so the number is here too; a plugin whose swing takes longer reads a stale aim.
     */
    public static final int SWING_TICKS = 9;
    /** How far a carriage's riders and body can lie beyond the span of its two bogeys (blocks, a bound for the cheap first test). */
    private static final double BODY_SLACK = 4.0;
    private static final int SIGNAL = ConflictRecords.SIGNAL_RAIL;

    /** A pass in progress: the circle's faction and the sample nearest its centre so far. */
    private static final class Open {
        final long faction;
        final long entry;
        double x, y, z, d2 = Double.POSITIVE_INFINITY;

        Open(long faction, long entry) {
            this.faction = faction;
            this.entry = entry;
        }

        void take(Territories.Territory c, double sx, double sy, double sz) {
            double dx = sx - c.x(), dz = sz - c.z();
            double d = dx * dx + dz * dz;
            if (d < d2) {
                d2 = d;
                x = sx;
                y = sy;
                z = sz;
            }
        }
    }

    /** One train's approach to one lurk (MZB-363, 365): whether TRAIN_PASSED may go, and the carriages struck at. */
    private static final class Approach {
        /** The table entry it belongs to: a lurk begun again by its agent starts armed. */
        final Lurk lurk;
        boolean armed = true;
        BitSet struck;

        Approach(Lurk lurk) {
            this.lurk = lurk;
        }

        boolean struck(int carriage) {
            return struck != null && struck.get(carriage);
        }

        void mark(int carriage) {
            if (struck == null)
                struck = new BitSet();
            struck.set(carriage);
        }
    }

    /** What the sampler remembers of a train. */
    private static final class State {
        /** Null for a {@link Walk}. */
        Train train;
        final long id;
        UUID owner;
        final int bucket;
        boolean seen, has;
        ResourceKey<Level> dim;
        double x, y, z;
        long tick;
        final List<Open> open = new ArrayList<>(2);
        /** Its approaches to lurks, by agent; made when a pass or a strike first needs one, null until then. */
        Map<Long, Approach> approaches;
        /** In {@link #NEAR}: a live lurk puppet is within {@link #APPROACH_RESET} blocks of its last sample. */
        boolean near;

        State(Train train) {
            this(CreateIds.of(train.id), train.owner);
            this.train = train;
        }

        State(long id, UUID owner) {
            this.id = id;
            this.owner = owner;
            this.bucket = (int) Long.remainderUnsigned(id, PERIOD);
        }
    }

    private static final Map<UUID, State> STATES = new HashMap<>();
    @SuppressWarnings("unchecked")
    private static final List<State>[] BUCKETS = new List[PERIOD];
    /** The trains near a live lurk puppet (MZB-365): rebuilt by their samples, read every tick for strikes. */
    private static final List<State> NEAR = new ArrayList<>();
    /** The circles one segment meets (reused). */
    private static final List<Territories.Territory> HIT = new ArrayList<>(4);
    private static boolean warned;
    /** The bench's hooks (SamplerBench): ns for one sample and for one whole tick; null otherwise. */
    private static LongConsumer sampleTimer, tickTimer;
    /** Samples taken and passes sent since the server started (the bench and GameTests read them). */
    public static long samples, passes;
    /** M8's, since the server started: TRAIN_PASSEDs sent and strikes the puppets started (GameTests read them). */
    public static long lurkPasses, strikes;

    static {
        for (int i = 0; i < PERIOD; i++)
            BUCKETS[i] = new ArrayList<>();
    }

    /** SamplerBench's timers: ns per sample and ns per tick of the sampler's handler. Null clears them. */
    public static void bench(LongConsumer perSample, LongConsumer perTick) {
        sampleTimer = perSample;
        tickTimer = perTick;
    }

    /** Drops what is remembered: the server stopped, or the engine side went (a pass open then is lost). */
    public static void clear() {
        NEAR.clear();
        STATES.clear();
        for (List<State> b : BUCKETS)
            b.clear();
    }

    /** The server stopped: nothing of it outlives it (single player starts another in the same JVM). */
    public static void stopped(net.neoforged.neoforge.event.server.ServerStoppedEvent event) {
        clear();
        samples = passes = lurkPasses = strikes = 0;
    }

    /** How many trains are remembered (GameTests). */
    public static int tracked() {
        return STATES.size();
    }

    public static void tick(ServerTickEvent.Post event) {
        LongConsumer timer = tickTimer;
        long t0 = timer == null ? 0L : System.nanoTime();
        run(event.getServer().overworld().getGameTime());
        if (timer != null)
            timer.accept(System.nanoTime() - t0);
    }

    private static void run(long now) {
        // Two reads when no territory minds the rail and no lurk waits; the config and session are asked only past them
        if ((!RtsimApi.territories().anyMinding(SIGNAL) && RtsimApi.lurks().isEmpty()) || !RtsimApi.conflictUp()) {
            if (!STATES.isEmpty())
                clear();
            return;
        }
        strike();
        int phase = (int) Math.floorMod(now, (long) PERIOD);
        if (phase == 0)
            rebucket();
        List<State> bucket = BUCKETS[phase];
        LongConsumer timer = sampleTimer;
        for (int i = 0, n = bucket.size(); i < n; i++) {
            State s = bucket.get(i);
            long t0 = timer == null ? 0L : System.nanoTime();
            try {
                sample(s, now);
            } catch (RuntimeException e) {
                // A train in a state Create's own tick would also trip on: forget its segment, say so once
                s.has = false;
                s.open.clear();
                warn(e);
            }
            if (timer != null)
                timer.accept(System.nanoTime() - t0);
            samples++;
        }
    }

    /** Says once per server run that the sampler skipped something it could not read (MZB-387). */
    private static void warn(RuntimeException e) {
        if (!warned) {
            warned = true;
            MicRtsimCreate.LOGGER.warn("mic_rtsim_create: the train sampler skipped a train it could not read", e);
        }
    }

    /**
     * Files each train in its bucket, and closes the passes of the trains gone since the last pass. Nothing
     * throws out of it: a gone train whose passes cannot be sent loses them (MZB-387), and the rest go on.
     */
    private static void rebucket() {
        for (List<State> b : BUCKETS)
            b.clear();
        for (Train t : Create.RAILWAYS.trains.values()) {
            State s = STATES.get(t.id);
            if (s == null)
                STATES.put(t.id, s = new State(t));
            s.train = t;
            s.owner = t.owner;
            s.seen = true;
            BUCKETS[s.bucket].add(s);
            if (s.approaches != null)
                forgetApproaches(s);
        }
        for (Iterator<State> it = STATES.values().iterator(); it.hasNext(); ) {
            State s = it.next();
            if (s.seen)
                s.seen = false;
            else {
                try {
                    closeAll(s, s.tick);
                } catch (RuntimeException e) {
                    s.open.clear();
                    warn(e);
                }
                if (s.near)
                    NEAR.remove(s);
                it.remove();
            }
        }
    }

    private static void sample(State s, long now) {
        Train t = s.train;
        if (t.invalid || t.derailed || t.graph == null || t.carriages.isEmpty()) {
            lose(s);
            return;
        }
        TravellingPoint p = t.carriages.get(0).getLeadingPoint();
        if (p == null || p.edge == null || p.node1 == null || p.edge.isInterDimensional()) {
            lose(s);
            return;
        }
        ResourceKey<Level> dim = p.node1.getLocation().getDimension();
        Vec3 at = p.getPosition(t.graph);
        advance(s, dim, at.x, at.y, at.z, now);
    }

    /** The train is at (x, y, z) in {@code dim} at game time {@code now}: opens, extends and sends its passes. */
    private static void advance(State s, ResourceKey<Level> dim, double x, double y, double z, long now) {
        // The segment from the last sample, unless a portal, a seam or a teleport lies in it
        double mx = x - s.x, mz = z - s.z;
        boolean continued = s.has && dim.equals(s.dim) && mx * mx + mz * mz <= MAX_SEGMENT * MAX_SEGMENT;
        if (s.has && !continued)
            closeAll(s, s.tick);
        double x0 = continued ? s.x : x, z0 = continued ? s.z : z;
        long entry = continued ? s.tick : now;

        Territories territories = RtsimApi.territories();
        HIT.clear();
        territories.crossing(dim, x0, z0, x, z, SIGNAL, HIT);
        for (int i = 0, n = HIT.size(); i < n; i++) {
            Territories.Territory c = HIT.get(i);
            if (find(s, c.faction()) == null) {
                Open o = new Open(c.faction(), entry);
                if (continued)
                    o.take(c, s.x, s.y, s.z); // a segment that clips the circle keeps the nearer of its two ends
                s.open.add(o);
            }
        }
        s.dim = dim;
        // A pass opens where a segment meets the radius and ends beyond radius + MARGIN (or when the circle is
        // gone or no longer minds the rail), so a train at the edge is one pass
        for (int i = s.open.size() - 1; i >= 0; i--) {
            Open o = s.open.get(i);
            Territories.Territory c = territories.get(o.faction);
            if (c != null)
                o.take(c, x, y, z);
            if (c == null || !c.minds(SIGNAL) || !c.dimension().equals(dim) || !c.within(x, z, MARGIN)) {
                s.open.remove(i);
                send(s, o, now);
            }
        }
        lurkPass(s, dim, continued ? s.x : x, continued ? s.y : y, continued ? s.z : z, x, y, z);
        s.has = true;
        s.x = x;
        s.y = y;
        s.z = z;
        s.tick = now;
    }

    // ---- lurks (M8) -------------------------------------------------------------------------------

    /**
     * The sample's segment (from the last sample, or the point alone) against each lurk in the train's dimension
     * (MZB-363): within the lurk's reach and armed sends TRAIN_PASSED, once per approach, and beyond
     * {@link #APPROACH_RESET} blocks arms it again. Then whether a live puppet is within that distance of the
     * lurk's track point joins or leaves the near set (MZB-365). The distance is the segment's from the track
     * point in three dimensions: a rail above or below the lurk is not beside it.
     */
    private static void lurkPass(State s, ResourceKey<Level> dim, double x0, double y0, double z0, double x, double y, double z) {
        Map<Long, Lurk> lurks = RtsimApi.lurks();
        boolean near = false;
        for (Lurk l : lurks.values()) {
            if (!l.dimension().equals(dim))
                continue;
            double d = segmentDistance(l.track(), x0, y0, z0, x, y, z);
            Approach a = approach(s, l);
            if (d <= l.reach() && (a == null || a.armed)) {
                if (a == null)
                    a = openApproach(s, l);
                a.armed = false;
                if (RtsimApi.trainPassed(l.agent(), l.epoch(), RtsimApi.lurkBody(l.agent()) != null, carriageNear(s, l.track()), s.id))
                    lurkPasses++;
            } else if (d > APPROACH_RESET && a != null) {
                s.approaches.remove(l.agent());
            }
            if (s.train != null && d <= APPROACH_RESET && RtsimApi.lurkBody(l.agent()) != null)
                near = true;
        }
        if (near != s.near) {
            s.near = near;
            if (near)
                NEAR.add(s);
            else
                NEAR.remove(s);
        }
    }

    /** The approach of a train to a lurk, or null when it has none (it is armed and has struck nothing). */
    private static Approach approach(State s, Lurk l) {
        Approach a = s.approaches == null ? null : s.approaches.get(l.agent());
        return a != null && a.lurk == l ? a : null;
    }

    private static Approach openApproach(State s, Lurk l) {
        if (s.approaches == null)
            s.approaches = new HashMap<>(2);
        Approach a = new Approach(l);
        s.approaches.put(l.agent(), a);
        return a;
    }

    /** Drops the approaches to lurks that are gone or begun again (once a second per train). */
    private static void forgetApproaches(State s) {
        Map<Long, Lurk> lurks = RtsimApi.lurks();
        s.approaches.values().removeIf(a -> lurks.get(a.lurk.agent()) != a.lurk);
        if (s.approaches.isEmpty())
            s.approaches = null;
    }

    /** The distance from {@code p} to the segment (x0, y0, z0) to (x1, y1, z1). */
    static double segmentDistance(Vec3 p, double x0, double y0, double z0, double x1, double y1, double z1) {
        double dx = x1 - x0, dy = y1 - y0, dz = z1 - z0;
        double len2 = dx * dx + dy * dy + dz * dz;
        double t = len2 < 1e-9 ? 0.0 : Math.max(0.0, Math.min(1.0, ((p.x - x0) * dx + (p.y - y0) * dy + (p.z - z0) * dz) / len2));
        double ex = x0 + t * dx - p.x, ey = y0 + t * dy - p.y, ez = z0 + t * dz - p.z;
        return Math.sqrt(ex * ex + ey * ey + ez * ez);
    }

    /** The index of the carriage whose leading point is nearest {@code track}; 0 for a walked point or an unreadable train. */
    private static int carriageNear(State s, Vec3 track) {
        Train t = s.train;
        if (t == null || t.graph == null)
            return 0;
        int best = 0;
        double bd = Double.POSITIVE_INFINITY;
        List<Carriage> cs = t.carriages;
        for (int c = 0, n = cs.size(); c < n; c++) {
            Vec3 v = positionOf(cs.get(c).getLeadingPoint(), t.graph);
            if (v == null)
                continue;
            double d = v.distanceToSqr(track);
            if (d < bd) {
                bd = d;
                best = c;
            }
        }
        return best;
    }

    private static Vec3 positionOf(TravellingPoint p, TrackGraph graph) {
        if (p == null || p.edge == null || p.node1 == null || p.edge.isInterDimensional())
            return null;
        return p.getPosition(graph);
    }

    /**
     * Every tick, for the trains near a live lurk puppet: a carriage whose target will be within
     * {@link #STRIKE_REACH} blocks (horizontal) of the puppet when the swing lands calls {@link RtsimApi#strike}
     * (MZB-365). A strike that started marks the carriage struck for the approach; one a busy puppet refused is
     * tried again next tick while the target stays in reach, so a carriage is struck at most once and a refusal
     * loses nothing.
     */
    private static void strike() {
        if (NEAR.isEmpty())
            return;
        Map<Long, Lurk> lurks = RtsimApi.lurks();
        for (int i = NEAR.size() - 1; i >= 0; i--) {
            State s = NEAR.get(i);
            Train t = s.train;
            if (t == null || t.invalid || t.derailed || t.graph == null || s.dim == null || lurks.isEmpty()) {
                s.near = false;
                NEAR.remove(i);
                continue;
            }
            try {
                for (Lurk l : lurks.values()) {
                    if (!l.dimension().equals(s.dim))
                        continue;
                    Mob body = RtsimApi.lurkBody(l.agent());
                    if (body != null)
                        strikeAt(s, t, l, body);
                }
            } catch (RuntimeException e) {
                warn(e);
            }
        }
    }

    private static void strikeAt(State s, Train t, Lurk l, Mob body) {
        List<Carriage> cs = t.carriages;
        // The farthest a bogey can be now and still have a target landing in reach: the reach, what the train covers in a swing, the carriage
        double far = STRIKE_REACH + SWING_TICKS * Math.abs(t.speed) + BODY_SLACK;
        for (int c = 0, n = cs.size(); c < n; c++) {
            Approach a = approach(s, l);
            if (a != null && a.struck(c))
                continue;
            Carriage car = cs.get(c);
            Vec3 lead = positionOf(car.getLeadingPoint(), t.graph), trail = positionOf(car.getTrailingPoint(), t.graph);
            double span = far + car.bogeySpacing;
            if (!near(lead, body, span) && !near(trail, body, span))
                continue;
            Vec3[] aim = aimAt(car, t, lead, trail, body);
            if (aim == null || !RtsimApi.strike(l.agent(), aim[0], aim[1]))
                continue;
            if (a == null)
                a = openApproach(s, l);
            a.mark(c);
            strikes++;
        }
    }

    /**
     * Where to strike a carriage and how that target moves a tick (the mean over the swing, so the plugin's aim at
     * {@code at + motion * SWING_TICKS} is where it will be when the blow lands): the first living rider that will
     * be in reach then; the swipe hits living entities only, and a bogey point misses a rider at the middle or
     * the rear of a long carriage. A carriage with no living rider is struck at its leading bogey's point, else
     * its trailing one's. A carriage with a rider that will not be in reach is not struck. Null for none.
     */
    private static Vec3[] aimAt(Carriage car, Train t, Vec3 lead, Vec3 trail, Mob body) {
        TravellingPoint lp = car.getLeadingPoint(), tp = car.getTrailingPoint();
        Vec3 leadMove = lead == null ? null : landingOf(lp, t, lead).subtract(lead);
        Vec3 trailMove = trail == null ? null : landingOf(tp, t, trail).subtract(trail);
        boolean riders = false;
        CarriageContraptionEntity e = car.anyAvailableEntity();
        if (e != null)
            for (Entity p : e.getPassengers()) {
                if (!(p instanceof LivingEntity) || !p.isAlive())
                    continue;
                riders = true;
                // A rider moves as the bogey nearer to it does
                Vec3 at = p.position();
                boolean byLead = lead != null && (trail == null || at.distanceToSqr(lead) <= at.distanceToSqr(trail));
                Vec3 move = byLead ? leadMove : trailMove;
                if (move != null && reaches(at.add(move), body))
                    return new Vec3[] {at, move.scale(1.0 / SWING_TICKS)};
            }
        if (riders)
            return null;
        if (lead != null && reaches(lead.add(leadMove), body))
            return new Vec3[] {lead, leadMove.scale(1.0 / SWING_TICKS)};
        if (trail != null && reaches(trail.add(trailMove), body))
            return new Vec3[] {trail, trailMove.scale(1.0 / SWING_TICKS)};
        return null;
    }

    /**
     * Where the point will be {@link #SWING_TICKS} ticks on at the train's present speed (a signed number of blocks a
     * tick; a positive offset on an edge goes toward its second node, which a positive speed travels). Along the
     * track, curves included, while it stays on its edge (the position Create gives it after that travel); past an
     * edge's end, straight on from the end's tangent (the next edge is not read, so a bend beyond the node errs by
     * about the rest squared over twice its radius). A stopped train's point stays where it is.
     */
    static Vec3 landingOf(TravellingPoint p, Train t, Vec3 here) {
        if (t.speed == 0.0)
            return here;
        double d = t.speed * SWING_TICKS;
        double length = p.edge.getLength();
        double to = p.position + d;
        if (to >= 0.0 && to <= length)
            return p.getPositionWithOffset(t.graph, d, false);
        double end = to > length ? length : 0.0;
        Vec3 edgeEnd = p.getPositionWithOffset(t.graph, end - p.position, false);
        return edgeEnd.add(p.edge.getDirectionAt(end).scale(to - end));
    }

    /** Whether a landing point is within {@link #STRIKE_REACH} blocks (horizontal) of the body. */
    private static boolean reaches(Vec3 landing, Mob body) {
        return near(landing, body, STRIKE_REACH);
    }

    private static boolean near(Vec3 v, Mob body, double reach) {
        if (v == null)
            return false;
        double dx = v.x - body.getX(), dz = v.z - body.getZ();
        return dx * dx + dz * dz <= reach * reach;
    }

    /**
     * Mixin: a train arrived at a station (Train.arriveAt, at its end, beside {@link TrainWatch#arrived}, which
     * returns early without an industry or district): a train a lurk robbed roars at its players (MZB-366).
     */
    public static void arrived(Train train, GlobalStation station) {
        try {
            ResourceKey<Level> dim = station == null ? null : station.getBlockEntityDimension();
            BlockPos pos = station == null ? null : station.getBlockEntityPos();
            MinecraftServer server = ServerLifecycleHooks.getCurrentServer();
            if (dim == null || pos == null || server == null)
                return;
            ServerLevel level = server.getLevel(dim);
            if (level != null)
                RtsimApi.trainCalled(CreateIds.of(train.id), level, pos);
        } catch (RuntimeException e) {
            warn(e);
        }
    }

    private static Open find(State s, long faction) {
        for (int i = 0, n = s.open.size(); i < n; i++)
            if (s.open.get(i).faction == faction)
                return s.open.get(i);
        return null;
    }

    /** The train cannot be read: its passes end at its last sample, and the next sample starts afresh. */
    private static void lose(State s) {
        if (s.has)
            closeAll(s, s.tick);
        s.has = false;
    }

    private static void closeAll(State s, long exit) {
        for (int i = 0, n = s.open.size(); i < n; i++)
            send(s, s.open.get(i), exit);
        s.open.clear();
    }

    private static void send(State s, Open o, long exit) {
        if (RtsimApi.railPass(o.faction, s.id, s.owner, s.dim, o.x, o.y, o.z, o.entry, exit))
            passes++;
    }

    /**
     * A point walked through the pass rules with no Create train (GameTests: the circle's edge, MZB-390). Not
     * filed in the sampler's lists, so it is sampled only when {@link #to} says; it sends what a train would.
     */
    public static final class Walk {
        private final State s;

        public Walk(long train, UUID owner) {
            s = new State(train, owner);
        }

        /** The next sample, at (x, y, z) in {@code dim} and game time {@code now}. */
        public void to(ResourceKey<Level> dim, double x, double y, double z, long now) {
            advance(s, dim, x, y, z, now);
        }

        /** The point is lost, as a train that derailed is: its passes end at its last sample. */
        public void lose() {
            TrainSampler.lose(s);
        }

        /** How many passes are open. */
        public int open() {
            return s.open.size();
        }
    }
}
