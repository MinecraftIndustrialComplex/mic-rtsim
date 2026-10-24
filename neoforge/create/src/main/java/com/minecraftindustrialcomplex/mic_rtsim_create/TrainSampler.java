package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Territories;
import com.simibubi.create.Create;
import com.simibubi.create.content.trains.entity.Train;
import com.simibubi.create.content.trains.entity.TravellingPoint;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.event.tick.ServerTickEvent;

import java.util.ArrayList;
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
 * the sample nearest the circle's centre (only samples, so the point is on the track), and a pass whose
 * circle no longer holds the train is sent with {@code exit} the sample's game time. A segment across
 * dimensions, longer than {@link #MAX_SEGMENT} blocks (a portal, a wrapped world's seam, a teleport), or of
 * a train that derailed or has no graph, is not tested: it ends the train's passes at its last sample and
 * the next sample starts a fresh segment.
 *
 * <p>D1: with no territory that minds the rail the tick is one array read; a sample reads at most the cells
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
    private static final int SIGNAL = ConflictRecords.SIGNAL_RAIL;

    /** A pass in progress: the circle's faction and the sample nearest its centre so far. */
    private static final class Open {
        final long faction;
        final long entry;
        double x, y, z, d2 = Double.POSITIVE_INFINITY;
        long stamp = Long.MIN_VALUE;

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

    /** What the sampler remembers of a train. */
    private static final class State {
        Train train;
        final long id;
        final int bucket;
        boolean seen, has;
        ResourceKey<Level> dim;
        double x, y, z;
        long tick;
        final List<Open> open = new ArrayList<>(2);

        State(Train train) {
            this.train = train;
            this.id = CreateIds.of(train.id);
            this.bucket = (int) Long.remainderUnsigned(id, PERIOD);
        }
    }

    private static final Map<UUID, State> STATES = new HashMap<>();
    @SuppressWarnings("unchecked")
    private static final List<State>[] BUCKETS = new List[PERIOD];
    /** The circles one segment meets (reused). */
    private static final List<Territories.Territory> HIT = new ArrayList<>(4);
    private static boolean warned;
    /** The bench's hooks (SamplerBench): ns for one sample and for one whole tick; null otherwise. */
    private static LongConsumer sampleTimer, tickTimer;
    /** Samples taken and passes sent since the server started (the bench and GameTests read them). */
    public static long samples, passes;

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
        STATES.clear();
        for (List<State> b : BUCKETS)
            b.clear();
    }

    /** The server stopped: nothing of it outlives it (single player starts another in the same JVM). */
    public static void stopped(net.neoforged.neoforge.event.server.ServerStoppedEvent event) {
        clear();
        samples = passes = 0;
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
        // One array read when no territory minds the rail; the config and session are asked only past it
        if (!RtsimApi.territories().anyMinding(SIGNAL) || !RtsimApi.conflictUp()) {
            if (!STATES.isEmpty())
                clear();
            return;
        }
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
            s.seen = true;
            BUCKETS[s.bucket].add(s);
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
        double x = at.x, y = at.y, z = at.z;
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
            Open o = find(s, c.faction());
            if (o == null) {
                o = new Open(c.faction(), entry);
                if (continued)
                    o.take(c, s.x, s.y, s.z); // a segment that clips the circle keeps the nearer of its two ends
                s.open.add(o);
            }
            o.take(c, x, y, z);
            // Kept while the new sample is inside: a pass whose circle the train has left is sent below
            o.stamp = c.meets(x, z, x, z) ? now : Long.MIN_VALUE;
        }
        s.dim = dim;
        for (int i = s.open.size() - 1; i >= 0; i--) {
            Open o = s.open.get(i);
            if (o.stamp != now) {
                s.open.remove(i);
                send(s, o, now);
            }
        }
        s.has = true;
        s.x = x;
        s.y = y;
        s.z = z;
        s.tick = now;
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
        if (RtsimApi.railPass(o.faction, s.id, s.train.owner, s.dim, o.x, o.y, o.z, o.entry, exit))
            passes++;
    }
}
