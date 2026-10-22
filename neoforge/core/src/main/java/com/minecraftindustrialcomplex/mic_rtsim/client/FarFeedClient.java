package com.minecraftindustrialcomplex.mic_rtsim.client;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarCircle;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarClock;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKey;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarMover;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarObjects;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarPayloads;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarState;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarWire;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.UUID;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientPacketListener;
import net.neoforged.neoforge.network.PacketDistributor;

/**
 * The client end of the far feed (research/distant-horizons-2026-10-21.md §6.3; docs/boundary.md §12): it
 * subscribes for the far renderers that want it, and turns the server's messages into far registry movers
 * (source {@value #SOURCE}).
 *
 * <ul>
 *   <li><b>Subscribing.</b> A renderer says what it wants ({@link #want}); the largest of every renderer's
 *       wants goes to the server, at most once a second and only when it changed, and only when the server has
 *       the payload (an optional channel: a server without it is never asked). Radius 0 withdraws.</li>
 *   <li><b>Legs</b> (the engine's dragons) become movers that fly from one end to the other on the far
 *       registry's clock, circle a hunt, or rest: nothing more arrives until the leg changes.</li>
 *   <li><b>Samples</b> (trains, dragons in the game) arrive once a second; each mover moves from where it was
 *       drawn to the new sample over the next second, so the view runs a second behind the server.</li>
 *   <li><b>{@link #active()}</b> says the server answered, so a source with its own way to learn the same
 *       things (Create's map sync, {@code mic_rtsim_create}) can stop asking.</li>
 * </ul>
 *
 * <p>Client thread only (the payload's handler runs on it).
 */
public final class FarFeedClient {
    public static final String SOURCE = MicRtsim.MODID + ":feed";
    /** Samples come a second apart; a mover takes this long to reach the next one. */
    static final double SAMPLE_TICKS = 20.0;
    /** A hunting dragon's circling speed, blocks a tick (10 m/s). PLACEHOLDER. */
    static final double HUNT_SPEED = 0.5;
    /** A subscription change goes out at most this often, in client ticks. */
    static final int RESEND_TICKS = 20;

    private record Want(int radius, int tier, int budget, int kinds) {
    }

    private static final Map<String, Want> WANTS = new HashMap<>();
    private static Want sent;
    private static ClientPacketListener sentOn;
    private static long sentAt = Long.MIN_VALUE / 2;
    private static boolean answered;
    private static long ticks;

    private static final Map<Integer, Obj> OBJS = new HashMap<>();
    // What came, for the bench and the log
    static long messages, bytes, records, malformed;

    private FarFeedClient() {}

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
            mover = new FarMover(new FarKey(SOURCE, new UUID(id, 0L), 0), kind, dim, prev, cur, sampleTick, interval,
                    rgb, false, state, style, variant, size, circle, entity);
        }
    }

    public static void register() {
        FarPayloads.setClientHandler(FarFeedClient::onMessage);
    }

    // ---------------------------------------------------------------------------------------------------------
    // Subscribing

    /**
     * Says what {@code consumer} (a far renderer's mod id) wants from the server: movers out to {@code radius}
     * blocks, at its detail {@code tier} (1 LOW to 3 HIGH), at most {@code budget} objects, of the kinds in
     * {@code kinds} (a mask of {@link FarKind#bit()}). Radius 0 withdraws it.
     */
    public static void want(String consumer, int radius, int tier, int budget, int kinds) {
        if (radius > 0 && tier > 0 && budget > 0 && kinds != 0)
            WANTS.put(consumer, new Want(radius, tier, budget, kinds));
        else
            WANTS.remove(consumer);
    }

    /** True while the server's far feed answers this client's subscription. */
    public static boolean active() {
        return answered && sent != null;
    }

    /** Messages, bytes and records received since the client started, and messages that did not decode. */
    public static long[] meter() {
        return new long[]{messages, bytes, records, malformed};
    }

    /** Objects the feed holds now. */
    public static int held() {
        return OBJS.size();
    }

    private static Want merged() {
        if (WANTS.isEmpty())
            return null;
        int r = 0, t = 0, b = 0, k = 0;
        for (Want w : WANTS.values()) {
            r = Math.max(r, w.radius);
            t = Math.max(t, w.tier);
            b = Math.max(b, w.budget);
            k |= w.kinds;
        }
        // Whole chunks, so a DH radius that wobbles does not resend
        return new Want((r + 15) / 16 * 16, t, b, k);
    }

    public static void tick() {
        ticks++;
        Minecraft mc = Minecraft.getInstance();
        ClientPacketListener conn = mc.getConnection();
        if (conn == null || mc.level == null) {
            forget();
            return;
        }
        if (conn != sentOn) {
            // A new connection knows nothing of an old subscription
            forget();
            sentOn = conn;
        }
        if (!conn.hasChannel(FarPayloads.Subscribe.TYPE))
            return;
        Want target = merged();
        if (Objects.equals(target, sent) || ticks - sentAt < RESEND_TICKS)
            return;
        PacketDistributor.sendToServer(target == null
                ? new FarPayloads.Subscribe(FarPayloads.SUB_VERSION, 0, 0, 0, 0)
                : new FarPayloads.Subscribe(FarPayloads.SUB_VERSION, target.radius, target.tier, target.budget, target.kinds));
        if (target == null)
            clearObjects();
        else if (sent == null)
            answered = false;
        sent = target;
        sentAt = ticks;
    }

    private static void forget() {
        sent = null;
        sentOn = null;
        answered = false;
        clearObjects();
    }

    private static void clearObjects() {
        if (OBJS.isEmpty())
            return;
        OBJS.clear();
        FarObjects.clear(SOURCE);
    }

    public static void loggingOut() {
        forget();
    }

    // ---------------------------------------------------------------------------------------------------------
    // Messages

    private static void onMessage(byte[] data) {
        messages++;
        bytes += data.length;
        if (sent == null)
            return; // a message already on its way when the subscription ended
        FarWire.Message m;
        try {
            m = FarWire.decode(data);
        } catch (IllegalArgumentException e) {
            if (malformed++ < 3)
                MicRtsim.LOGGER.warn("[mic_rtsim] far feed: a message did not decode ({}); the view starts over", e.getMessage());
            OBJS.clear();
            FarObjects.clear(SOURCE);
            return;
        }
        records += m.recs().size();
        if ((m.flags() & FarWire.RESET) != 0) {
            OBJS.clear();
            answered = true;
        }
        double now = FarClock.ticks();
        for (FarWire.Rec r : m.recs())
            apply(r, m, now);
        List<FarMover> out = new ArrayList<>(OBJS.size());
        for (Obj o : OBJS.values())
            if (o.mover != null)
                out.add(o.mover);
        FarObjects.commit(SOURCE, out);
    }

    private static void apply(FarWire.Rec r, FarWire.Message m, double now) {
        switch (r) {
            case FarWire.Gone g -> OBJS.remove(g.handle());
            case FarWire.Intro i -> {
                Obj o = OBJS.get(i.handle());
                if (o == null || o.id != i.id()) {
                    o = new Obj();
                    OBJS.put(i.handle(), o);
                }
                o.id = i.id();
                o.kind = FarKind.ofWire(i.kind());
                o.style = i.style();
                o.variant = i.variant();
                o.size = i.size();
                o.rgb = i.rgb();
                if (o.kind == null)
                    OBJS.remove(i.handle()); // a kind this client does not know
                else
                    o.rebuild(m.dimension());
            }
            case FarWire.Leg l -> {
                Obj o = OBJS.get(l.handle());
                if (o == null)
                    return;
                leg(o, l, m.gameTime(), now);
                o.rebuild(m.dimension());
            }
            case FarWire.Sample s -> {
                Obj o = OBJS.get(s.handle());
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
                Obj o = OBJS.get(e.handle());
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
