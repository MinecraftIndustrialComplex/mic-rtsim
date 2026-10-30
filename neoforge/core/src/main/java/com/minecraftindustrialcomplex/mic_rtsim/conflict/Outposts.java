package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.tags.EntityTypeTags;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructureStart;
import net.neoforged.neoforge.event.level.ChunkEvent;

import java.lang.ref.WeakReference;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Predicate;

/**
 * Reports the vanilla pillager outposts to the conflict as raider camps (EQUILIBRIUM.md §4, slice S7, EQC237 to EQC254;
 * Ben's FB-5 (b)). The raider culture claims {@code minecraft:pillager_outpost} (and Illager Invasion's fort, when that
 * mod is in the pack), so a camp reported under either id is a pool camp of the raider culture: a cap of 50 warriors that
 * are a number, no bodies at home, parties that march as vanilla pillagers, vindicators and ravagers. The outpost's own
 * pillager (its structure spawn override) stays vanilla's and is nobody's body (FB-7 (a)).
 *
 * <p>The finder is {@code MowzieCamps}'s pattern (vanilla is the core's, D4, so no plugin): a chunk holding such a start is
 * queued when it loads, as a small record (the dimension, the chunk, the late flag, the start's box), and looked at on
 * the server thread at the end of the tick ({@link ConflictBridge#endOfTick}). No mixin, no per-entity work: the drain
 * of that queue is all that runs per tick (D1). A chunk loaded again later finds the same camp and changes nothing.
 *
 * <p>An outpost is known for the server run ({@link #KNOWN}) and reported to the engine ({@link ConflictBridge#reportCamp},
 * no bodies) when the conflict module is on and {@code outpostsJoin} is set: once at its find, again at each session
 * start (the engine of a new session knows nothing of it) and at the module's switch-on edge (a report made while the
 * module is off journals nothing). An outpost of a chunk that was not new when the finder saw it is flagged late, and is
 * reported only with {@code outpostsJoinExisting} (FB-9 (a)). A player's kill of a raider (the {@code minecraft:raiders}
 * tag but the witch) inside the outpost's box widened by {@code outpostKinMargin} blocks is the camp's killing signal; a
 * raid that comes of a village, a patrol or a vanilla raid event is not the camp's and nothing here reads it.
 *
 * <p>Uninstall (UR-1): the reporter writes nothing to the world; the camp lives in the engine's pages and the journal.
 */
public final class Outposts {
    private Outposts() {}

    /** The queue's bound: records past it are dropped, and their camp is found at its chunk's next load (PLACEHOLDER). */
    public static final int QUEUE_CAP = 4_096;

    /** The structure ids that are outposts: the raider culture's ({@code raider.json}). */
    public static final Set<String> STRUCTURES = Set.of("minecraft:pillager_outpost", "illagerinvasion:illager_fort");

    /** One outpost start in a loaded chunk: what the finder reads of it, and nothing that holds the chunk. */
    private record Found(String structure, ChunkPos start, BoundingBox box) {}

    private record Loaded(WeakReference<MinecraftServer> server, ResourceKey<Level> dimension, boolean late, List<Found> found) {}

    /** An outpost known to this server run. */
    private record Spot(ResourceKey<Level> dimension, ChunkPos start) {}

    /** What the engine is told of it: its structure, the block that keys it, its kin box and whether it was found late. */
    private record Known(String structure, BlockPos pos, BoundingBox kinBox, boolean late) {}

    private static final ConcurrentLinkedQueue<Loaded> QUEUE = new ConcurrentLinkedQueue<>();
    private static final AtomicInteger QUEUED = new AtomicInteger();

    /** The outposts of this server run, in the order they were found (insertion order keeps the reports deterministic). */
    private static final Map<Spot, Known> KNOWN = new LinkedHashMap<>();
    /** The session every known outpost was last reported to; null while the module is off or after a new session. */
    private static RtsimSession reported;
    /** Known outposts added since the last report. */
    private static final List<Spot> FRESH = new ArrayList<>();

    /** Counters (GameTests, logs). */
    public static long found, reports;
    /** Records dropped past the queue's bound; atomic, since chunk loads can post from another thread. */
    public static final AtomicInteger dropped = new AtomicInteger();

    /** The records waiting for the end of the tick (GameTests). */
    public static int queued() {
        return QUEUED.get();
    }

    /** The outposts known to this server run (GameTests). */
    public static int known() {
        synchronized (KNOWN) {
            return KNOWN.size();
        }
    }

    /** ChunkEvent.Load: a chunk holding an outpost start is queued (public for the GameTests). */
    public static void onChunkLoad(ChunkEvent.Load e) {
        if (e.getLevel().isClientSide() || !(e.getChunk() instanceof LevelChunk c) || !(e.getLevel() instanceof ServerLevel level))
            return;
        Map<Structure, StructureStart> starts = c.getAllStarts();
        if (starts.isEmpty())
            return;
        Registry<Structure> reg = level.registryAccess().registryOrThrow(Registries.STRUCTURE);
        List<Found> found = null;
        for (Map.Entry<Structure, StructureStart> en : starts.entrySet()) {
            StructureStart st = en.getValue();
            if (!st.isValid())
                continue;
            ResourceLocation id = reg.getKey(en.getKey());
            if (id == null || !STRUCTURES.contains(id.toString()))
                continue;
            BoundingBox b = st.getBoundingBox();
            if (found == null)
                found = new ArrayList<>(1);
            found.add(new Found(id.toString(), st.getChunkPos(), new BoundingBox(b.minX(), b.minY(), b.minZ(), b.maxX(), b.maxY(), b.maxZ())));
        }
        if (found == null)
            return;
        if (QUEUED.incrementAndGet() > QUEUE_CAP) {
            QUEUED.decrementAndGet();
            dropped.incrementAndGet();
            return;
        }
        QUEUE.add(new Loaded(new WeakReference<>(level.getServer()), level.dimension(), !e.isNewChunk(), found));
    }

    /**
     * The end of the tick (called by {@link ConflictBridge#endOfTick} whatever the module's state): the queue is taken into
     * the known outposts, and what is known is reported when the module is on and outposts join. Returns at once with
     * nothing queued and nothing to report.
     */
    static void tick(ConflictBridge bridge, RtsimSession s) {
        if (!QUEUE.isEmpty())
            take(s);
        boolean on = RtsimConfig.outpostsJoin() && RtsimConfig.on(Module.CONFLICT);
        if (!on) {
            reported = null; // the module's switch-on, or the setting's, reports every outpost again
            return;
        }
        List<Spot> todo;
        synchronized (KNOWN) {
            if (reported != s) {
                todo = new ArrayList<>(KNOWN.keySet());
                reported = s;
                FRESH.clear();
            } else if (!FRESH.isEmpty()) {
                todo = new ArrayList<>(FRESH);
                FRESH.clear();
            } else {
                return;
            }
        }
        boolean existing = RtsimConfig.outpostsJoinExisting();
        for (Spot sp : todo) {
            Known k;
            synchronized (KNOWN) {
                k = KNOWN.get(sp);
            }
            if (k == null || (k.late() && !existing))
                continue;
            ServerLevel level = s.server().getLevel(sp.dimension());
            if (level == null)
                continue;
            Predicate<Entity> kin = kin(k.kinBox());
            if (bridge.reportCamp(level, k.structure(), k.pos(), kin, null, k.late(), 0, 0) != 0L)
                reports++;
        }
    }

    /** The queued records into the known outposts (the session is live: the spawn chunks load before it starts). */
    private static void take(RtsimSession s) {
        if (s == null || s.failed())
            return; // kept for the session
        for (Loaded l; (l = QUEUE.poll()) != null; ) {
            QUEUED.decrementAndGet();
            MinecraftServer server = l.server().get();
            if (server == null || server != s.server())
                continue; // a chunk of another world (singleplayer: the queue outlived its server)
            ServerLevel level = server.getLevel(l.dimension());
            if (level == null)
                continue;
            int margin = RtsimConfig.outpostKinMargin();
            for (Found f : l.found()) {
                Spot sp = new Spot(l.dimension(), f.start());
                BoundingBox b = f.box();
                BlockPos pos = new BlockPos(f.start().getMiddleBlockX(), b.minY(), f.start().getMiddleBlockZ());
                BoundingBox kinBox = new BoundingBox(b.minX() - margin, b.minY() - margin, b.minZ() - margin,
                        b.maxX() + margin, b.maxY() + margin, b.maxZ() + margin);
                synchronized (KNOWN) {
                    if (KNOWN.putIfAbsent(sp, new Known(f.structure(), pos, kinBox, l.late())) == null) {
                        FRESH.add(sp);
                        found++;
                    }
                }
            }
        }
    }

    /**
     * Which deaths in an outpost's territory are its own kin (FB-6 (a)): a raider of the {@code minecraft:raiders} tag but
     * the witch, inside the outpost's box widened by the margin. The bridge ignores a death of a sim puppet before it asks.
     */
    static Predicate<Entity> kin(BoundingBox box) {
        return x -> x.getType().is(EntityTypeTags.RAIDERS) && x.getType() != EntityType.WITCH && box.isInside(x.blockPosition());
    }

    /** A server run ends: nothing of its world outlives it. */
    public static void serverStopped() {
        QUEUE.clear();
        QUEUED.set(0);
        synchronized (KNOWN) {
            KNOWN.clear();
            FRESH.clear();
        }
        reported = null;
        found = reports = 0;
        dropped.set(0);
        MicRtsim.LOGGER.debug("[mic_rtsim] outposts: server stopped");
    }

    /** Forgets what GameTests made known (their far outposts), so a later test starts clean. */
    public static void forgetForTest() {
        synchronized (KNOWN) {
            KNOWN.clear();
            FRESH.clear();
        }
        QUEUE.clear();
        QUEUED.set(0);
        reported = null;
    }
}
