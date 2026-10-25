package com.minecraftindustrialcomplex.mic_rtsim.far;

import com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugin;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import java.nio.ByteBuffer;
import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.function.Consumer;
import net.minecraft.server.level.ServerLevel;

/**
 * The engine's far legs (FAR_LEG, 0x8C00, docs/boundary.md §12.1): what every dragon is doing, as the engine
 * sends it whenever that changes, kept per session and drawn by the far feed. A dragon in the game is drawn
 * from its body instead (a body source gives the same id at a higher priority), so a BODY record is kept only
 * to know where the engine last had it, and never drawn from here: the entity is the truth.
 */
public final class FarLegs implements FarSource {
    public static final int FAR_LEG = 0x8C00;
    public static final int SIZE = 80;

    /** FAR_LEG's fields (docs/boundary.md §12.1). */
    public static final int AGENT = 16, BODY = 24, DIM = 28, KIND = 32, VARIANT = 33, STAGE = 34, Y = 36, X0 = 40,
            Z0 = 44, X1 = 48, Z1 = 52, T0 = 56, T1 = 64, RADIUS = 72, AGE = 76;

    /**
     * How a body's look is drawn: its style number, colours per variant, and how far above its feet its middle
     * is at each stage (registered by its plugin). The engine's y is the body's feet, as its entity's is; a far
     * mover's is its middle, as a body source samples it, so a leg is drawn {@link #centre} higher
     * (docs/choices-dh-feed.md DHT42).
     *
     * @param centreByStage blocks from the feet to the middle for stages 1, 2, ...; empty for none
     */
    public record Style(int style, int[] rgbByVariant, double[] centreByStage) {
        public Style(int style, int[] rgbByVariant) {
            this(style, rgbByVariant, new double[0]);
        }

        public int rgb(int variant) {
            return rgbByVariant.length == 0 ? 0x808080 : rgbByVariant[Math.floorMod(variant, rgbByVariant.length)];
        }

        /** Blocks from the feet to the middle at {@code stage} (1 up): the nearest stage known, or 0. */
        public double centre(int stage) {
            if (centreByStage.length == 0)
                return 0.0;
            return centreByStage[Math.max(0, Math.min(centreByStage.length - 1, stage - 1))];
        }
    }

    private static final Map<String, Style> STYLES = new ConcurrentHashMap<>();

    /** The look of a dragon of entity type {@code body} ({@code iceandfire:ice_dragon}); its plugin says. */
    public static void registerStyle(String body, Style style) {
        STYLES.put(body, style);
    }

    public static Style style(String body) {
        Style s = body == null ? null : STYLES.get(body);
        return s != null ? s : new Style(0, new int[]{0x808080});
    }

    /**
     * One dragon's last record, and the way its last flown leg went ({@code hx}, {@code hz}, a unit vector; 0, 0
     * while none was seen): a client draws a dragon that stopped flying facing that way. {@code ageDays} is its
     * age in whole days at the record's time, as Ice and Fire counts it (DECISIONS.md DH-7).
     */
    public record Rec(long agent, FarState state, String body, String dimension, int variant, int stage, int y,
            int x0, int z0, int x1, int z1, long t0, long t1, int radius, int ageDays, double hx, double hz) {
    }

    private final Map<Long, Rec> legs = new HashMap<>();
    private long accepted, gone, malformed;
    private boolean enabled = true;

    public static boolean isFarCommand(int kind) {
        return kind >= 0x8C00 && kind <= 0x8CFF;
    }

    /**
     * The ecology module switched (the session's {@code onModulesChanged}, server thread). Off forgets every leg
     * and drops any record still on its way: the engine clears its own queue, GONE records included, and resends
     * every dragon when the module is back on, so a leg kept now would be a ghost then.
     */
    public void setEnabled(boolean on) {
        enabled = on;
        if (!on)
            legs.clear();
    }

    /** A FAR_LEG from the engine (the dispatcher's sequence check has passed). */
    public boolean accept(ByteBuffer b, int at, int kind, int size, int aux, SessionPlugin.EngineNames names) {
        if (kind != FAR_LEG || size < SIZE) {
            malformed++;
            return true;
        }
        if (!enabled)
            return true;
        long agent = b.getLong(at + AGENT);
        FarState state = FarState.ofWire(aux);
        if (state == null || state == FarState.MOVING) {
            malformed++;
            return true;
        }
        accepted++;
        if (state == FarState.GONE) {
            legs.remove(agent);
            gone++;
            return true;
        }
        String dim = names.name(Obs.DICT_DIMENSION, b.getInt(at + DIM));
        String body = names.name(Obs.DICT_BODY, b.getInt(at + BODY));
        if (dim == null) {
            malformed++;
            legs.remove(agent);
            return true;
        }
        int x0 = b.getInt(at + X0), z0 = b.getInt(at + Z0), x1 = b.getInt(at + X1), z1 = b.getInt(at + Z1);
        Rec was = legs.get(agent);
        double hx = was != null ? was.hx : 0.0, hz = was != null ? was.hz : 0.0;
        double n = Math.hypot(x1 - x0, z1 - z0);
        if (state == FarState.FLY && n > 1e-6) {
            hx = (x1 - x0) / n;
            hz = (z1 - z0) / n;
        }
        // A u32 on the wire: held to an int's range
        int age = (int) Math.min(Integer.MAX_VALUE, Integer.toUnsignedLong(b.getInt(at + AGE)));
        legs.put(agent, new Rec(agent, state, body, dim, b.get(at + VARIANT) & 0xFF, b.get(at + STAGE) & 0xFF,
                b.getInt(at + Y), x0, z0, x1, z1, b.getLong(at + T0), b.getLong(at + T1), b.getInt(at + RADIUS), age,
                hx, hz));
        return true;
    }

    /** The dragon's last record, or null. */
    public Rec get(long agent) {
        return legs.get(agent);
    }

    public int size() {
        return legs.size();
    }

    public String describe() {
        return legs.size() + " dragons, " + accepted + " records (" + gone + " gone, " + malformed + " malformed)";
    }

    @Override
    public String name() {
        return "engine dragon legs";
    }

    @Override
    public void collect(ServerLevel level, long gameTime, Consumer<FarItem> out) {
        if ((RtsimConfig.effectiveMask() & Module.ECOLOGY.bit()) == 0) {
            // The dragons are switched off: the engine resends every dragon when they come back
            if (!legs.isEmpty())
                legs.clear();
            return;
        }
        String dim = level.dimension().location().toString();
        for (Rec r : legs.values()) {
            if (r.state == FarState.BODY || !dim.equals(r.dimension))
                continue;
            Style s = style(r.body);
            int x1 = r.state == FarState.FLY ? r.x1 : r.x0, z1 = r.state == FarState.FLY ? r.z1 : r.z0;
            // The engine's y is the feet; the mover's is the middle, where the body's samples put it. The age sizes
            // the far model between stages as the body is sized (DH-7)
            int y = r.y + (int) Math.round(s.centre(r.stage));
            out.accept(new FarItem(r.agent, FarKind.DRAGON, s.style(), r.variant, r.stage, s.rgb(r.variant), r.state,
                    null, new FarItem.Leg(y, r.x0, r.z0, x1, z1, r.t0, r.t1, r.state == FarState.HUNT ? r.radius : 0),
                    -1, 0, null, r.ageDays));
        }
    }
}
