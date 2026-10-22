package com.minecraftindustrialcomplex.mic_rtsim.far;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.topology.Iso;
import com.minecraftindustrialcomplex.mic_rtsim.topology.LevelTopology;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.function.Function;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.player.PlayerEvent;
import net.neoforged.neoforge.event.server.ServerStoppedEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.network.PacketDistributor;

/**
 * The far feed (research/distant-horizons-2026-10-21.md §6.3; docs/boundary.md §12): once a second, what
 * moves far from each subscribed player, sent to that player only.
 *
 * <ul>
 *   <li><b>Subscribers</b> are clients with a far renderer that asked ({@link FarPayloads.Subscribe}): their
 *       radius (Distant Horizons' LOD radius), their detail tier, their budget and the kinds they draw. A tier
 *       caps the budget ({@link #TIER_BUDGET}), so no client asks for more than HIGH's.</li>
 *   <li><b>Sources</b> ({@link FarSource}) say what moves in a level; the feed asks each once a second per
 *       level a subscriber is in, keeps one item per id (a body beats the engine's leg), moves each to its
 *       image nearest the subscriber (through a seam), filters ({@link FarFilter}) and sends the difference
 *       from what that client holds ({@link FarView}).</li>
 *   <li><b>Nobody subscribed</b>, it does nothing: no source is asked, nothing is sent.</li>
 * </ul>
 *
 * <p>Server thread only.
 */
public final class FarFeed {
    /** A sweep a second. */
    public static final int INTERVAL_TICKS = 20;
    /** The farthest a subscriber may ask for, blocks (1024 chunks). PLACEHOLDER. */
    public static final int MAX_RADIUS = 16_384;
    /** Objects a little past a subscriber's radius are sent, so they do not pop in at its edge. */
    public static final int RADIUS_MARGIN = 64;
    /** The most objects each tier (OFF, LOW, MEDIUM, HIGH) may ask for. PLACEHOLDER. */
    public static final int[] TIER_BUDGET = {0, 48, 128, 320};

    /** Where the bytes go: the player's connection, or a test's recorder. */
    public interface Sink {
        void send(ServerPlayer player, byte[] message);
    }

    private static final Sink NETWORK = (p, m) -> PacketDistributor.sendToPlayer(p, new FarPayloads.Feed(m));
    private static final List<FarSource> SOURCES = new CopyOnWriteArrayList<>();
    private static final Map<UUID, Sub> SUBS = new LinkedHashMap<>();
    private static Sink sink = NETWORK;
    /** Who a subscriber is: the server's player list, or a test's fake players too. */
    private static Function<UUID, ServerPlayer> players;
    private static long ticks;
    private static final Map<String, Integer> SOURCE_ERRORS = new HashMap<>();
    private static int badVersions;

    // Totals since the server started, for /rtsim perf and the bench
    static long sweeps, sweepNanos, sweepMaxNanos, messages, bytes, records;
    /** The sweeps after the first {@link #WARM_SWEEPS} (class loading and the JIT), for the bench. */
    static long warmSweeps, warmNanos;
    static final int WARM_SWEEPS = 10;

    private FarFeed() {}

    /** One subscriber. */
    static final class Sub {
        final UUID player;
        int radius, tier, budget, kinds;
        final FarView view = new FarView();
        String dimension;
        boolean reset = true;
        long bytes, messages, records, since;

        Sub(UUID player) {
            this.player = player;
        }
    }

    /** Totals of one subscriber, for the bench. */
    public record SubStats(UUID player, int radius, int tier, int budget, int held, long messages, long bytes,
            long records, long sinceTick) {
    }

    public static void register() {
        NeoForge.EVENT_BUS.addListener(FarFeed::onServerTick);
        NeoForge.EVENT_BUS.addListener(FarFeed::onLoggedOut);
        NeoForge.EVENT_BUS.addListener(FarFeed::onStopped);
    }

    /** Adds a source (RtsimApi.registerFarSource). */
    public static void addSource(FarSource source) {
        SOURCES.add(source);
    }

    /** Removes a source (a GameTest's). */
    public static void removeSource(FarSource source) {
        SOURCES.remove(source);
    }

    /** Replaces where messages go (GameTests record them); null restores the network. */
    public static void setSink(Sink s) {
        sink = s == null ? NETWORK : s;
    }

    /**
     * Replaces how a subscriber's player is found (GameTests subscribe players that never logged in, so the
     * player list does not have them); null restores the server's player list.
     */
    public static void setPlayers(Function<UUID, ServerPlayer> lookup) {
        players = lookup;
    }

    /** A client's subscription, from its payload (server thread). */
    public static void subscribe(ServerPlayer player, FarPayloads.Subscribe p) {
        if (p.version() != FarPayloads.SUB_VERSION) {
            if (badVersions++ < 5)
                MicRtsim.LOGGER.info("[mic_rtsim] far feed: {} subscribed with version {}, this server reads {}; ignored",
                        player.getGameProfile().getName(), p.version(), FarPayloads.SUB_VERSION);
            return;
        }
        int tier = Math.max(0, Math.min(TIER_BUDGET.length - 1, p.tier()));
        int radius = Math.max(0, Math.min(MAX_RADIUS, p.radius()));
        int budget = Math.max(0, Math.min(TIER_BUDGET[tier], p.budget()));
        if (radius == 0 || budget == 0 || p.kinds() == 0) {
            SUBS.remove(player.getUUID());
            return;
        }
        Sub s = SUBS.computeIfAbsent(player.getUUID(), Sub::new);
        if (s.messages == 0)
            s.since = ticks;
        // A client subscribes when its wants change or its view was lost (a message that did not decode): either
        // way it is sent everything again, from a RESET
        s.view.clear();
        s.reset = true;
        s.radius = radius;
        s.tier = tier;
        s.budget = budget;
        s.kinds = p.kinds();
    }

    /** Ends {@code player}'s subscription (logged out, or a test). */
    public static void unsubscribe(UUID player) {
        SUBS.remove(player);
    }

    public static int subscribers() {
        return SUBS.size();
    }

    public static List<SubStats> stats() {
        List<SubStats> out = new ArrayList<>();
        for (Sub s : SUBS.values())
            out.add(new SubStats(s.player, s.radius, s.tier, s.budget, s.view.size(), s.messages, s.bytes, s.records, s.since));
        return out;
    }

    /** One line for /rtsim perf. */
    public static String describe() {
        return SUBS.size() + " subscribers, " + SOURCES.size() + " sources; " + sweeps + " sweeps ("
                + (sweeps == 0 ? 0 : sweepNanos / sweeps / 1000) + " us each, "
                + (warmSweeps == 0 ? 0 : warmNanos / warmSweeps / 1000) + " us each after the first " + WARM_SWEEPS
                + ", at most " + sweepMaxNanos / 1000 + " us), " + messages + " messages, " + bytes
                + " bytes, " + records + " records";
    }

    private static void onServerTick(ServerTickEvent.Post event) {
        ticks++;
        if (SUBS.isEmpty() || ticks % INTERVAL_TICKS != 0)
            return;
        long t0 = System.nanoTime();
        sweep(event.getServer());
        long took = System.nanoTime() - t0;
        sweepNanos += took;
        sweepMaxNanos = Math.max(sweepMaxNanos, took);
        if (++sweeps > WARM_SWEEPS) {
            warmSweeps++;
            warmNanos += took;
        }
    }

    private static void onLoggedOut(PlayerEvent.PlayerLoggedOutEvent event) {
        SUBS.remove(event.getEntity().getUUID());
    }

    private static void onStopped(ServerStoppedEvent event) {
        if (sweeps > 0) // the server's cost of the feed, for the bench (neoforge/README.md)
            MicRtsim.LOGGER.info("[mic_rtsim] far feed at stop: {}", describe());
        SUBS.clear();
        sink = NETWORK;
        players = null;
        sweeps = sweepNanos = sweepMaxNanos = messages = bytes = records = warmSweeps = warmNanos = 0;
    }

    /** One sweep over every subscriber (a test calls it directly). */
    public static void sweep(MinecraftServer server) {
        long gameTime = server.overworld().getGameTime();
        Map<ServerLevel, List<FarItem>> byLevel = new HashMap<>();
        RtsimSession session = RtsimSession.current();
        Iterator<Sub> it = SUBS.values().iterator();
        while (it.hasNext()) {
            Sub s = it.next();
            ServerPlayer p = players != null ? players.apply(s.player) : server.getPlayerList().getPlayer(s.player);
            if (p == null) {
                it.remove();
                continue;
            }
            ServerLevel level = p.serverLevel();
            String dim = level.dimension().location().toString();
            if (!dim.equals(s.dimension)) {
                s.view.clear();
                s.reset = true;
                s.dimension = dim;
            }
            List<FarItem> items = byLevel.computeIfAbsent(level, l -> collect(l, gameTime, session));
            LevelTopology topo = session != null ? session.topology(level) : LevelTopology.flat();
            send(p, s, items, topo, gameTime);
        }
    }

    private static List<FarItem> collect(ServerLevel level, long gameTime, RtsimSession session) {
        List<FarItem> all = new ArrayList<>();
        if (session != null && !session.failed())
            run(session.farLegs, level, gameTime, all);
        for (FarSource src : SOURCES)
            run(src, level, gameTime, all);
        return FarFilter.dedupe(all);
    }

    private static void run(FarSource src, ServerLevel level, long gameTime, List<FarItem> all) {
        try {
            src.collect(level, gameTime, all::add);
        } catch (RuntimeException | LinkageError e) {
            int n = SOURCE_ERRORS.merge(src.name(), 1, Integer::sum);
            if (n <= 3)
                MicRtsim.LOGGER.error("[mic_rtsim] far feed source {} failed; skipped this second", src.name(), e);
        }
    }

    private static void send(ServerPlayer p, Sub s, List<FarItem> items, LevelTopology topo, long gameTime) {
        double px = p.getX(), pz = p.getZ();
        List<FarItem> mapped;
        if (topo.isFlat()) {
            mapped = items;
        } else {
            mapped = new ArrayList<>(items.size());
            for (FarItem i : items) {
                double[] at = i.position(gameTime);
                Iso iso = topo.nearestImageIso(px, pz, at[0], at[1]);
                mapped.add(i.moved(iso));
            }
        }
        List<FarItem> chosen = FarFilter.select(mapped, px, pz, s.radius + RADIUS_MARGIN, s.budget, s.kinds, gameTime);
        List<FarWire.Rec> recs = s.view.step(chosen);
        if (recs.isEmpty() && !s.reset)
            return;
        FarWire.Message m = new FarWire.Message(s.reset ? FarWire.RESET : 0, gameTime, s.dimension,
                (int) Math.floor(px), (int) Math.floor(p.getY()), (int) Math.floor(pz), recs);
        byte[] b = FarWire.encode(m);
        sink.send(p, b);
        s.reset = false;
        s.messages++;
        s.bytes += b.length;
        s.records += recs.size();
        messages++;
        bytes += b.length;
        records += recs.size();
    }
}
