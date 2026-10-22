package com.minecraftindustrialcomplex.mic_rtsim.client;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarPayloads;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarWire;
import java.util.HashMap;
import java.util.Map;
import java.util.Objects;
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

    private static final FarFeedReceiver RECEIVER = new FarFeedReceiver();
    // What came, for the bench and the log
    static long messages, bytes, records, malformed;

    private FarFeedClient() {}

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
        return RECEIVER.held();
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
        RECEIVER.clear();
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
            m = RECEIVER.accept(data);
        } catch (IllegalArgumentException e) {
            if (malformed++ < 3)
                MicRtsim.LOGGER.warn("[mic_rtsim] far feed: a message did not decode ({}); the view starts over", e.getMessage());
            return;
        }
        records += m.recs().size();
        if ((m.flags() & FarWire.RESET) != 0)
            answered = true;
    }
}
