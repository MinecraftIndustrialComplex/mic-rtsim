package com.minecraftindustrialcomplex.mic_rtsim.client;

import com.minecraftindustrialcomplex.mic_rtsim.far.FarCircle;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarClock;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKey;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarMover;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarObjects;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarState;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarWire;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;

/**
 * What the far feed's messages say, as far registry movers ({@link FarFeedClient#SOURCE}): the objects by their
 * handles, each a leg on the far clock or a run of samples. Plain Java, apart from {@link FarFeedClient}'s
 * networking, so a unit test can play the hand-over through it.
 *
 * <p>A sample says where the object was when the server swept, a second after the last; drawn as it came, a
 * mover would trail the real thing by that second. So a moving object is run on (docs/choices-dh-feed.md
 * DHT44): from its last two samples, its speed along its way, how that speed changes and how it turns, it is
 * drawn where it will be by the time the next sample comes, from where it is drawn now, and a little past that
 * should the sample be late. One that stopped (the server sends the same sample once more) or is braking to a
 * stop stops where it does, and waits there if the run had it a little further on ({@link #HOLD}).
 *
 * <p>A flown leg is timed in game time and drawn on the far clock (client ticks), so the two are tied by an
 * offset: the far clock minus the client's game time, which vanilla keeps to the server's once a second.
 * {@link #clock} follows it, and when it moves (a server below 20 ticks a second) every flown leg is timed
 * again, so a dragon is where the server has it, not where a full-speed clock would put it.
 */
final class FarFeedReceiver {
    /** Samples come a second apart; a mover takes this long to reach the next one. */
    static final double SAMPLE_TICKS = 20.0;
    /** A hunting dragon's circling speed, blocks a tick (10 m/s). PLACEHOLDER. */
    static final double HUNT_SPEED = 0.5;
    /** A dragon that starts hunting off its circle spirals onto it over this many ticks. PLACEHOLDER. */
    static final double JOIN_TICKS = 40.0;
    /** The offset may wander this far, in ticks, before the legs are timed again. */
    static final double RETIME_TICKS = 2.0;
    /** A sample that comes late: the mover runs on this long past where the next was due, then waits. */
    static final double RUN_ON_TICKS = 10.0;
    /** The furthest a mover is run on from its newest sample, in ticks. */
    static final double MAX_RUN_TICKS = SAMPLE_TICKS + RUN_ON_TICKS + 10.0;
    /** Slower than this (an eighth of a block a sample, what the server does not send) is standing still. */
    static final double STILL = 0.125 / SAMPLE_TICKS;
    /**
     * Faster than this, blocks a tick, is a jump, not a run (a teleport, or a toroidal world's wrap putting the
     * object in another image): it is drawn at its new sample at once. No train or dragon moves at 160 m/s.
     */
    static final double JUMP = 8.0;
    /**
     * A mover run on past where the samples have it waits there rather than rolling back, but once the server has
     * it standing, only this many blocks past where its run ends (PLACEHOLDER): further than that the run on was
     * wrong (a dragon that stopped to hover) and it is drawn back, or it would be left standing far from the real
     * thing.
     */
    static final double HOLD = 16.0;
    /**
     * A mover the server still has moving (its newest sample moved) may yet reach the box, however its run on
     * reads its slowing, so the box waits for it up to this many blocks past (PLACEHOLDER), half the 64 blocks
     * within which a client counts the real thing as there; once the server has it standing, {@link #HOLD}.
     */
    static final double WAIT = 32.0;
    /**
     * A turn, the heading's change between the last two gaps, is followed only when each gap went at least this
     * far across the ground (the wire's eighths say little of a heading over less), and only up to
     * {@link #MAX_TURN} a gap: more than that is a mover going back the way it came, not a turn.
     */
    static final double TURN_MIN_BLOCKS = 2.0;
    static final double MAX_TURN = Math.toRadians(135.0);

    private final Map<Integer, Obj> objs = new HashMap<>();
    /** Far clock minus game time, as the legs are timed; NaN until known. */
    private double offset = Double.NaN;
    long retimes;

    /** One object of the feed, by its handle. */
    private static final class Obj {
        long id;
        FarKind kind;
        int style, variant, size, rgb;
        FarState state = FarState.MOVING;
        double[] prev, cur;
        double sampleTick, interval = 1.0;
        FarCircle circle;
        int entity = -1;
        double hx = 1.0, hz = 0.0;
        /** A flown leg's start in game time, or Long.MIN_VALUE. */
        long legT0 = Long.MIN_VALUE;
        /** The newest sample and its time on the far clock; null before the first, and after a leg. */
        double[] last;
        double lastTick;
        /** Its mean velocity since the sample before (blocks a tick, both points), and over how many ticks. */
        double[] velocity;
        double velocityTicks;
        /** The run's change of speed and turn ({@link Run}), for a second sample in the same tick. */
        double[] acc, turn;
        /** The way the box last ran, at the end of its run: what a mover that then stops must not go back along. */
        double[] way;
        String dim;
        FarMover mover;

        /** The segment drawn at {@code now}, or null before the first position. */
        double[] drawnAt(double now) {
            if (mover == null)
                return null;
            double[] s = new double[6];
            mover.segmentAt(now, s);
            return s;
        }

        void rebuild(String dim) {
            this.dim = dim;
            if (kind == null || prev == null)
                return;
            mover = new FarMover(new FarKey(FarFeedClient.SOURCE, new UUID(id, 0L), 0), kind, dim, prev, cur, sampleTick, interval,
                    rgb, false, state, style, variant, size, circle, entity);
        }
    }

    int held() {
        return objs.size();
    }

    /** Forgets every object, and the registry's movers of the feed. */
    void clear() {
        if (objs.isEmpty())
            return;
        objs.clear();
        FarObjects.clear(FarFeedClient.SOURCE);
    }

    /**
     * Applies one message and commits the movers it leaves. Returns it, or throws IllegalArgumentException
     * (and forgets every object: the server starts the view over on its next RESET) when it does not decode.
     */
    FarWire.Message accept(byte[] data) {
        FarWire.Message m;
        try {
            m = FarWire.decode(data);
        } catch (IllegalArgumentException e) {
            objs.clear();
            FarObjects.clear(FarFeedClient.SOURCE);
            throw e;
        }
        if ((m.flags() & FarWire.RESET) != 0)
            objs.clear();
        double now = FarClock.ticks();
        if (Double.isNaN(offset))
            offset = now - m.gameTime();
        for (FarWire.Rec r : m.recs())
            apply(r, m, now);
        commit();
        return m;
    }

    /**
     * The far clock minus the client's game time now (once a tick): when it has moved {@link #RETIME_TICKS}
     * from the one the legs are timed by, every flown leg is timed again by it.
     */
    void clock(double offsetNow) {
        if (!Double.isNaN(offset) && Math.abs(offsetNow - offset) < RETIME_TICKS)
            return;
        offset = offsetNow;
        boolean any = false;
        for (Obj o : objs.values()) {
            if (o.legT0 == Long.MIN_VALUE || o.mover == null)
                continue;
            o.sampleTick = o.legT0 + offset;
            o.rebuild(o.dim);
            any = true;
        }
        if (any) {
            retimes++;
            commit();
        }
    }

    private void commit() {
        List<FarMover> out = new ArrayList<>(objs.size());
        for (Obj o : objs.values())
            if (o.mover != null)
                out.add(o.mover);
        FarObjects.commit(FarFeedClient.SOURCE, out);
    }

    private void apply(FarWire.Rec r, FarWire.Message m, double now) {
        switch (r) {
            case FarWire.Gone g -> objs.remove(g.handle());
            case FarWire.Intro i -> {
                Obj o = objs.get(i.handle());
                if (o == null || o.id != i.id()) {
                    o = new Obj();
                    objs.put(i.handle(), o);
                }
                o.id = i.id();
                o.kind = FarKind.ofWire(i.kind());
                o.style = i.style();
                o.variant = i.variant();
                o.size = i.size();
                o.rgb = i.rgb();
                if (o.kind == null)
                    objs.remove(i.handle()); // a kind this client does not know
                else
                    o.rebuild(m.dimension());
            }
            case FarWire.Leg l -> {
                Obj o = objs.get(l.handle());
                if (o == null)
                    return;
                leg(o, l, now);
                o.rebuild(m.dimension());
            }
            case FarWire.Sample s -> {
                Obj o = objs.get(s.handle());
                if (o == null)
                    return;
                sample(o, s.segment(), Math.min(now, m.gameTime() + offset), now);
                o.circle = null;
                o.legT0 = Long.MIN_VALUE;
                o.state = stateOf(s.state());
                o.rebuild(m.dimension());
            }
            case FarWire.Real e -> {
                Obj o = objs.get(e.handle());
                if (o == null)
                    return;
                o.entity = e.entity();
                o.rebuild(m.dimension());
            }
        }
    }

    private static FarState stateOf(int wire) {
        FarState s = FarState.ofWire(wire);
        return s == null || s == FarState.GONE ? FarState.MOVING : s;
    }

    /**
     * A sample {@code seg}, taken at far clock {@code at}, as the mover's next stretch from {@code now}: from where
     * it is drawn, through where it is expected one sample later, and on at that pace a little longer
     * (DHT44). Its velocity is from the sample before (or, the first after a leg, the leg's as drawn), and how
     * its speed changes from the velocity before that; a mover braking to a stop stops there.
     */
    private static void sample(Obj o, double[] seg, double at, double now) {
        double[] drawn = o.drawnAt(now);
        if ((o.last != null && gap(seg, o.last) > JUMP * Math.max(1.0, at - o.lastTick))
                || (drawn != null && gap(seg, drawn) > JUMP * SAMPLE_TICKS)) {
            // A jump: drawn where it is now, standing until the next sample says how it moves
            o.last = seg;
            o.lastTick = at;
            o.velocity = null;
            o.way = null;
            o.prev = o.cur = seg.clone();
            o.sampleTick = now;
            o.interval = 1.0;
            return;
        }
        double[] before = o.velocity;
        double[] v = null;
        double[] acc = null;
        double[] turn = null;
        if (o.last != null && at - o.lastTick >= 1.0) {
            boolean stood = o.velocity == null || still(o.velocity);
            // A gap after standing: it set off at most a sweep ago, as the server sends every second it moves
            double dt = stood ? Math.min(at - o.lastTick, SAMPLE_TICKS) : at - o.lastTick;
            v = new double[6];
            for (int i = 0; i < 6; i++)
                v[i] = (seg[i] - o.last[i]) / dt;
            if (!stood) {
                // The change of speed, not of velocity: a dragon turning at one speed neither speeds up nor slows
                double span = (dt + o.velocityTicks) / 2.0;
                acc = new double[2];
                for (int p = 0; p < 2; p++)
                    acc[p] = (speed(v, p) - speed(o.velocity, p)) / span;
                turn = turnOf(o.velocity, o.velocityTicks, v, dt);
            }
            o.velocity = v;
            o.velocityTicks = dt;
        } else if (o.last == null && drawn != null && o.mover != null) {
            // The first sample after a leg: it goes on as the leg was taking it
            double[] back = new double[6];
            o.mover.segmentAt(now - 1.0, back);
            v = new double[6];
            for (int i = 0; i < 6; i++)
                v[i] = drawn[i] - back[i];
            o.velocity = v;
            o.velocityTicks = SAMPLE_TICKS;
        } else if (o.last == null) {
            o.velocity = null;
        } else {
            // A second sample in the same tick: the run so far
            v = o.velocity;
            acc = o.acc;
            turn = o.turn;
        }
        o.acc = acc;
        o.turn = turn;
        o.last = seg;
        o.lastTick = at;
        Run run = new Run(seg, at, v, acc, turn, o.velocityTicks);
        double[] from = drawn != null ? drawn : run.at(now);
        double due = Math.min(now + SAMPLE_TICKS, at + MAX_RUN_TICKS);
        double end = Math.min(now + SAMPLE_TICKS + RUN_ON_TICKS, at + MAX_RUN_TICKS);
        double stop = run.stopTime();
        if (!run.moving())
            end = due;
        else if (stop < end)
            end = Math.max(due, stop); // it comes to rest there, run on or not
        boolean late = end - now < 1.0; // too long since the sample to run it on: it holds where the run ends
        double[] to;
        if (late || (stop <= end && end > due)) {
            to = run.at(end); // or where it comes to rest, after the next sample is due
        } else {
            double[] there = run.at(due);
            double k = (end - now) / (due - now);
            to = new double[6];
            for (int i = 0; i < 6; i++)
                to[i] = from[i] + (there[i] - from[i]) * k;
        }
        // Never backward along its way (its way now, or the way the box last ran if it stopped): run on a little
        // past where it stopped short, it waits there for the samples to pass it rather than rolling back. Further
        // past than WAIT while the server has it moving, or HOLD once it stood, the run on was wrong, and it goes
        // back to where they have it
        double[] way = run.moving() ? run.tangent(now) : o.way != null ? o.way : before;
        boolean goingOn = v != null && !still(v); // the server has it moving yet: it may still reach the box
        if (way != null && !still(way) && along(to, from, way) < 0.0 && gap(to, from) <= (goingOn ? WAIT : HOLD))
            to = from;
        if (run.moving())
            o.way = run.tangent(end);
        o.prev = late ? to : from;
        o.cur = to;
        o.sampleTick = now;
        o.interval = late ? 1.0 : end - now;
    }

    /** The farther of the two points' distances between segments {@code a} and {@code b}. */
    private static double gap(double[] a, double[] b) {
        double g = 0.0;
        for (int p = 0; p < 2; p++) {
            double dx = a[3 * p] - b[3 * p], dy = a[3 * p + 1] - b[3 * p + 1], dz = a[3 * p + 2] - b[3 * p + 2];
            g = Math.max(g, Math.sqrt(dx * dx + dy * dy + dz * dz));
        }
        return g;
    }

    /** How far the midpoint moves from {@code from} to {@code to} along the midpoint's velocity in {@code v}. */
    private static double along(double[] to, double[] from, double[] v) {
        double d = 0.0;
        for (int i = 0; i < 3; i++)
            d += (to[i] + to[3 + i] - from[i] - from[3 + i]) * (v[i] + v[3 + i]);
        return d;
    }

    private static double speed(double[] v, int p) {
        return Math.sqrt(v[3 * p] * v[3 * p] + v[3 * p + 1] * v[3 * p + 1] + v[3 * p + 2] * v[3 * p + 2]);
    }

    private static boolean still(double[] v) {
        return speed(v, 0) < STILL && speed(v, 1) < STILL;
    }

    /**
     * The mover's turn across the ground, from its midpoint's heading over the gap before ({@code prev} over
     * {@code prevTicks}) to that over the last ({@code v} over {@code ticks}): {radians a block, the most it turns
     * on past its heading at the sample}. One turn for the whole mover, so a carriage entering a curve bends
     * neither end alone. It turns on at most half the angle it turned between the two gaps: a dragon that swerved
     * once and flew on straight is still seen turning at the next sample, and turned on in full it would be drawn
     * swinging round as far again. Null when it did not turn, or a gap is too short to say.
     */
    private static double[] turnOf(double[] prev, double prevTicks, double[] v, double ticks) {
        double px = prev[0] + prev[3], pz = prev[2] + prev[5], qx = v[0] + v[3], qz = v[2] + v[5];
        double lp = Math.sqrt(px * px + pz * pz) / 2.0 * prevTicks, lq = Math.sqrt(qx * qx + qz * qz) / 2.0 * ticks;
        if (lp < TURN_MIN_BLOCKS || lq < TURN_MIN_BLOCKS)
            return null;
        double angle = Math.atan2(px * qz - pz * qx, px * qx + pz * qz);
        if (angle == 0.0 || Math.abs(angle) > MAX_TURN)
            return null;
        return new double[]{angle / ((lp + lq) / 2.0), Math.abs(angle) / 2.0};
    }

    /**
     * Where a sampled mover will be: each point on along its velocity since the sample before, at its speed at
     * the sample, speeding up or slowing down as it was (at most doubling its speed a sample), and stopping where
     * slowing brings it to rest. A mover that was turning goes on turning as it was, on an arc across the ground
     * (its climb or dive kept) from its heading at the sample (half a gap's turn past its heading over the last
     * gap), as far as {@link #turnOf} lets it, then straight on; a straight run would take a circling dragon off
     * on the tangent.
     *
     * @param v     the mean velocity over the last gap (6 values), or null for none
     * @param acc   each point's change of speed, blocks a tick a tick, or null for none
     * @param turn  the turn across the ground, {radians a block, the most it turns} ({@link #turnOf}), or null
     * @param ticks the last gap, over which {@code v} is the mean
     */
    private record Run(double[] seg, double at, double[] v, double[] acc, double[] turn, double ticks) {
        boolean moving() {
            return v != null && (speedAt(0) >= STILL || speedAt(1) >= STILL);
        }

        /** When the first point slowing down comes to rest; infinite when none does. */
        double stopTime() {
            double t = Double.POSITIVE_INFINITY;
            for (int p = 0; p < 2 && v != null; p++) {
                double u = speedAt(p), a = accAt(p, u);
                if (u >= STILL && a < 0.0)
                    t = Math.min(t, at + u / -a);
            }
            return t;
        }

        double[] at(double t) {
            double[] out = seg.clone();
            if (v == null)
                return out;
            for (int p = 0; p < 2; p++) {
                double mean = speed(v, p), u = speedAt(p);
                if (u < STILL || mean < 1e-9)
                    continue;
                double d = distance(p, u, t);
                double k = turn == null ? 0.0 : turn[0];
                double ux = v[3 * p] / mean, uz = v[3 * p + 2] / mean, ground = Math.sqrt(ux * ux + uz * uz);
                out[3 * p + 1] += v[3 * p + 1] / mean * d;
                if (k == 0.0 || ground < 1e-9) {
                    out[3 * p] += ux * d;
                    out[3 * p + 2] += uz * d;
                    continue;
                }
                // On an arc of k radians a block from the heading at the sample (hx, hz): k above 0 turns it toward
                // (-hz, hx), below 0 toward (hz, -hx)
                double phi = k * groundGap(p) / 2.0, c = Math.cos(phi), s = Math.sin(phi);
                double hx = (ux * c - uz * s) / ground, hz = (ux * s + uz * c) / ground;
                double sg = ground * d, ks = k * sg, ahead, aside;
                if (Math.abs(ks) <= turn[1]) {
                    ahead = Math.sin(ks) / k;
                    aside = (1.0 - Math.cos(ks)) / k;
                } else {
                    // Turned as far as it may, then straight on
                    double sc = Math.copySign(turn[1] / Math.abs(k), sg), pc = k * sc, r = sg - sc;
                    ahead = Math.sin(pc) / k + r * Math.cos(pc);
                    aside = (1.0 - Math.cos(pc)) / k + r * Math.sin(pc);
                }
                out[3 * p] += hx * ahead - hz * aside;
                out[3 * p + 2] += hz * ahead + hx * aside;
            }
            return out;
        }

        /**
         * Each point's velocity at {@code t} as far as its heading goes: its velocity over the last gap turned
         * across the ground as the run has turned it by then (its speed left as it was over the gap).
         */
        double[] tangent(double t) {
            if (v == null || turn == null)
                return v;
            double[] out = v.clone();
            for (int p = 0; p < 2; p++) {
                double mean = speed(v, p), u = speedAt(p);
                if (u < STILL || mean < 1e-9)
                    continue;
                double ux = v[3 * p] / mean, uz = v[3 * p + 2] / mean, ground = Math.sqrt(ux * ux + uz * uz);
                double k = turn[0], ks = k * ground * distance(p, u, t);
                double phi = k * groundGap(p) / 2.0 + Math.max(-turn[1], Math.min(ks, turn[1]));
                double c = Math.cos(phi), s = Math.sin(phi);
                out[3 * p] = v[3 * p] * c - v[3 * p + 2] * s;
                out[3 * p + 2] = v[3 * p] * s + v[3 * p + 2] * c;
            }
            return out;
        }

        /** How far point {@code p}, at speed {@code u} at the sample, has gone along its run by {@code t}. */
        private double distance(int p, double u, double t) {
            double tau = Math.max(-SAMPLE_TICKS, Math.min(t - at, MAX_RUN_TICKS));
            double a = accAt(p, u);
            if (tau < 0.0)
                return u * tau;
            if (a < 0.0 && tau > u / -a)
                return u * u / (2.0 * -a);
            return u * tau + 0.5 * a * tau * tau;
        }

        /** How far point {@code p} went across the ground over the last gap. */
        private double groundGap(int p) {
            return Math.sqrt(v[3 * p] * v[3 * p] + v[3 * p + 2] * v[3 * p + 2]) * ticks;
        }

        /** The point's speed at the sample: the mean over the gap and half a gap of its change; 0 if it stopped. */
        private double speedAt(int p) {
            double u = speed(v, p) + (acc != null ? acc[p] * ticks / 2.0 : 0.0);
            return Math.max(0.0, u);
        }

        private double accAt(int p, double u) {
            return acc == null ? 0.0 : Math.min(acc[p], u / SAMPLE_TICKS);
        }
    }

    /** A leg as a mover on the far clock: game time {@code t} is far clock {@code t + offset}. */
    private void leg(Obj o, FarWire.Leg l, double now) {
        FarState state = stateOf(l.state());
        o.last = null;
        o.velocity = null;
        o.way = null;
        double x0 = l.x0() + 0.5, z0 = l.z0() + 0.5, x1 = l.x1() + 0.5, z1 = l.z1() + 0.5, y = l.y();
        double[] before = o.drawnAt(now);
        o.state = state;
        o.circle = null;
        o.legT0 = Long.MIN_VALUE;
        switch (state) {
            case FLY -> {
                heading(o, x1 - x0, z1 - z0);
                o.prev = seg(x0, y, z0, o.hx, o.hz);
                o.cur = seg(x1, y, z1, o.hx, o.hz);
                o.legT0 = l.t0();
                o.sampleTick = l.t0() + offset;
                o.interval = Math.max(1.0, l.t1() - l.t0());
            }
            case HUNT -> {
                double r = Math.max(1.0, l.radius());
                // Spiral onto the circle from where the dragon was drawn, turning the way it was heading; a dragon
                // never drawn starts on it, at an angle and a direction of its own
                double phase0 = (o.id & 0xFFFF) / 65536.0 * 2.0 * Math.PI, from = r;
                boolean ccw = ((o.id >>> 16) & 1) == 0;
                if (before != null) {
                    double dx = (before[0] + before[3]) / 2.0 - x0, dz = (before[2] + before[5]) / 2.0 - z0;
                    from = Math.sqrt(dx * dx + dz * dz);
                    if (from > 1e-6)
                        phase0 = Math.atan2(dz, dx);
                    // The heading against the turning-with-the-angle tangent (-sin, cos)
                    double turn = (before[0] - before[3]) * -Math.sin(phase0)
                            + (before[2] - before[5]) * Math.cos(phase0);
                    if (Math.abs(turn) > 1e-3)
                        ccw = turn > 0;
                }
                double omega = ccw ? HUNT_SPEED / r : -HUNT_SPEED / r;
                o.circle = new FarCircle(x0, y, z0, r, omega, phase0, now, from,
                        Math.abs(from - r) < 0.5 ? 0.0 : JOIN_TICKS);
                double[] s = new double[6];
                o.circle.segmentAt(now, s);
                o.prev = o.cur = s;
                o.sampleTick = now;
                o.interval = 1.0;
            }
            default -> {
                o.prev = o.cur = seg(x0, y, z0, o.hx, o.hz);
                o.sampleTick = now;
                o.interval = 1.0;
            }
        }
    }

    private static void heading(Obj o, double dx, double dz) {
        double n = Math.sqrt(dx * dx + dz * dz);
        if (n > 1e-6) {
            o.hx = dx / n;
            o.hz = dz / n;
        }
    }

    /** A dragon's segment: one block along the heading, centred on (x, y, z). */
    private static double[] seg(double x, double y, double z, double hx, double hz) {
        return new double[]{x + hx * 0.5, y, z + hz * 0.5, x - hx * 0.5, y, z - hz * 0.5};
    }
}
