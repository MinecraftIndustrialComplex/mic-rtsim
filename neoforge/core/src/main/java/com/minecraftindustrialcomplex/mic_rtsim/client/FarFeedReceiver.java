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
 * handles, each a leg on the far clock or a sample it moves to over the next second. Plain Java, apart from
 * {@link FarFeedClient}'s networking, so a unit test can play the hand-over through it.
 */
final class FarFeedReceiver {
    /** Samples come a second apart; a mover takes this long to reach the next one. */
    static final double SAMPLE_TICKS = 20.0;
    /** A hunting dragon's circling speed, blocks a tick (10 m/s). PLACEHOLDER. */
    static final double HUNT_SPEED = 0.5;

    private final Map<Integer, Obj> objs = new HashMap<>();

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
        for (FarWire.Rec r : m.recs())
            apply(r, m, now);
        List<FarMover> out = new ArrayList<>(objs.size());
        for (Obj o : objs.values())
            if (o.mover != null)
                out.add(o.mover);
        FarObjects.commit(FarFeedClient.SOURCE, out);
        return m;
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
                leg(o, l, m.gameTime(), now);
                o.rebuild(m.dimension());
            }
            case FarWire.Sample s -> {
                Obj o = objs.get(s.handle());
                if (o == null)
                    return;
                double[] seg = s.segment();
                double[] drawn = o.drawnAt(now);
                o.prev = drawn != null ? drawn : seg;
                o.cur = seg;
                o.sampleTick = now;
                o.interval = SAMPLE_TICKS;
                o.circle = null;
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

    /** A leg as a mover on the far clock: game time {@code t} is {@code now + (t - gameTime)}. */
    private static void leg(Obj o, FarWire.Leg l, long gameTime, double now) {
        FarState state = stateOf(l.state());
        double x0 = l.x0() + 0.5, z0 = l.z0() + 0.5, x1 = l.x1() + 0.5, z1 = l.z1() + 0.5, y = l.y();
        double[] before = o.drawnAt(now);
        o.state = state;
        o.circle = null;
        switch (state) {
            case FLY -> {
                heading(o, x1 - x0, z1 - z0);
                o.prev = seg(x0, y, z0, o.hx, o.hz);
                o.cur = seg(x1, y, z1, o.hx, o.hz);
                o.sampleTick = now + (l.t0() - gameTime);
                o.interval = Math.max(1.0, l.t1() - l.t0());
            }
            case HUNT -> {
                double r = Math.max(1.0, l.radius());
                // Join the circle where the dragon is, and turn the way it was heading
                double phase0 = before != null
                        ? Math.atan2((before[2] + before[5]) / 2.0 - z0, (before[0] + before[3]) / 2.0 - x0)
                        : (o.id & 0xFFFF) / 65536.0 * 2.0 * Math.PI;
                double omega = ((o.id >>> 16) & 1) == 0 ? HUNT_SPEED / r : -HUNT_SPEED / r;
                o.circle = new FarCircle(x0, y, z0, r, omega, phase0, now);
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
