package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs;

import com.bobmowzie.mowziesmobs.server.world.feature.structure.FrostmawPieces;
import com.bobmowzie.mowziesmobs.server.world.feature.structure.UmvuthanaGrovePieces;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructurePiece;
import net.minecraft.world.level.levelgen.structure.StructureStart;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.event.level.ChunkEvent;

import java.lang.ref.WeakReference;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * Finds the dens and groves (MOWZIE.md §4.9, "ChunkEvent.Load only finds camps"; the lair finder's
 * pattern, {@code Lairs.java}): a chunk holding a den or grove structure start is queued when it loads, as
 * a small record of what the finder reads (MZB-237), and looked at on the server thread at the end of the
 * tick; each start in it is declared to the core ({@link RtsimApi#declareCamp}), which follows its
 * footprint from then on. A camp is declared once per server run; the core keeps it across sessions.
 *
 * <p>The queue holds no chunk: only the dimension, the chunk's position, the late flag and the boxes of the
 * start's pieces, so a flood of loads pins no memory but the records. It waits for a live session (the spawn
 * chunks load before it starts). D1: nothing here runs per entity or per tick but the drain of that queue.
 */
public final class MowzieCamps {
    private MowzieCamps() {}

    /** The queue's bound: records past it are dropped, and their camp is found at its chunk's next load (PLACEHOLDER). */
    public static final int QUEUE_CAP = 4_096;

    /** The grove's footprint: the circle its warriors are restricted to ({@code UmvuthanaGrovePieces.java:507}). */
    public static final double GROVE_RADIUS = 25.0;

    /** One camp start in a loaded chunk: what the finder reads of it, and nothing that holds the chunk. */
    private record Found(boolean den, ChunkPos start, BoundingBox box) {}

    private record Loaded(WeakReference<MinecraftServer> server, ResourceKey<Level> dimension, boolean late, List<Found> found) {}

    private static final ConcurrentLinkedQueue<Loaded> QUEUE = new ConcurrentLinkedQueue<>();
    private static final AtomicInteger QUEUED = new AtomicInteger();

    /** Counters (GameTests, logs). */
    public static long dens, groves, noFirepit;
    /** Records dropped past the queue's bound; atomic, since chunk loads can post from another thread (MZB-599). */
    public static final AtomicInteger dropped = new AtomicInteger();

    /** The records waiting for the end of the tick (GameTests). */
    public static int queued() {
        return QUEUED.get();
    }

    /** ChunkEvent.Load: a chunk holding a den or grove start is queued (public for the GameTests). */
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
            if (id == null)
                continue;
            String name = id.toString();
            boolean den = MicRtsimMowziesMobs.DEN_STRUCTURE.equals(name);
            if (!den && !MicRtsimMowziesMobs.GROVE_STRUCTURE.equals(name))
                continue;
            BoundingBox box = den ? denBox(st) : groveBox(st);
            if (found == null)
                found = new ArrayList<>(1);
            found.add(new Found(den, st.getChunkPos(), box));
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

    static void drain() {
        if (QUEUE.isEmpty())
            return;
        RtsimSession s = RtsimSession.current();
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
            for (Found f : l.found()) {
                if (f.den())
                    declareDen(level, f.start(), f.box(), l.late(), MicRtsimMowziesMobs.DEN_STRUCTURE);
                else
                    declareGrove(level, f.start(), f.box(), l.late(), MicRtsimMowziesMobs.GROVE_STRUCTURE);
            }
        }
    }

    static void clear() {
        QUEUE.clear();
        QUEUED.set(0);
        dens = groves = noFirepit = 0;
        dropped.set(0);
    }

    /** A den start's box: its template piece's, else the start's. */
    private static BoundingBox denBox(StructureStart start) {
        BoundingBox b = start.getBoundingBox();
        for (StructurePiece p : start.getPieces()) {
            if (p instanceof FrostmawPieces.FrostmawPiece) {
                b = p.getBoundingBox();
                break;
            }
        }
        return copy(b);
    }

    /** A grove start's fire pit box, or null with none. */
    private static BoundingBox groveBox(StructureStart start) {
        for (StructurePiece p : start.getPieces())
            if (p instanceof UmvuthanaGrovePieces.FirepitPiece)
                return copy(p.getBoundingBox());
        return null;
    }

    private static BoundingBox copy(BoundingBox b) {
        return new BoundingBox(b.minX(), b.minY(), b.minZ(), b.maxX(), b.maxY(), b.maxZ());
    }

    /**
     * A den: its footprint is the cylinder of {@link DenKind#FOOTPRINT_RADIUS} round its spot (MZB-230), its
     * spot the template box's floor centre, within a block of where the template stands its Frostmaw (at
     * (3, 0, 3.5) of the unrotated template, MZB-217).
     */
    static long declareDen(ServerLevel level, StructureStart start, boolean late) {
        return declareDen(level, start, late, MicRtsimMowziesMobs.DEN_STRUCTURE);
    }

    /**
     * As above under another structure name (GameTests: a name no culture claims, so the engine leaves the
     * camp to the test's scripted lines).
     */
    public static long declareDen(ServerLevel level, StructureStart start, boolean late, String structure) {
        return declareDen(level, start.getChunkPos(), denBox(start), late, structure);
    }

    private static long declareDen(ServerLevel level, ChunkPos start, BoundingBox b, boolean late, String structure) {
        Vec3 spot = denSpot(b);
        long f = RtsimApi.declareCamp(level, start, structure, DenKind.INSTANCE, BlockPos.containing(spot), spot,
                DenKind.FOOTPRINT_RADIUS, DenKind.FOOTPRINT_RADIUS, late);
        if (f != 0)
            dens++;
        return f;
    }

    /** A den's spot from its template box: the floor's centre. */
    public static Vec3 denSpot(BoundingBox b) {
        return new Vec3((b.minX() + b.maxX() + 1) / 2.0, b.minY(), (b.minZ() + b.maxZ() + 1) / 2.0);
    }

    /**
     * A grove: its footprint is the 25-block circle round the fire pit's centre, the block its warriors
     * are restricted to ({@code FirepitPiece.postProcess}: {@code findGround(4, 4)}, which with the
     * template piece's NORTH orientation is (minX + 4, maxZ - 4) of the piece's box; MZB-218). A start with
     * no fire pit piece is no grove.
     */
    static long declareGrove(ServerLevel level, StructureStart start, boolean late) {
        return declareGrove(level, start, late, MicRtsimMowziesMobs.GROVE_STRUCTURE);
    }

    /** As above under another structure name (GameTests). */
    public static long declareGrove(ServerLevel level, StructureStart start, boolean late, String structure) {
        return declareGrove(level, start.getChunkPos(), groveBox(start), late, structure);
    }

    private static long declareGrove(ServerLevel level, ChunkPos start, BoundingBox fp, boolean late, String structure) {
        if (fp == null) {
            noFirepit++;
            return 0;
        }
        BlockPos centre = groveCentre(fp);
        Vec3 spot = new Vec3(centre.getX() + 0.5, centre.getY(), centre.getZ() + 0.5);
        long f = RtsimApi.declareCamp(level, start, structure, GroveKind.INSTANCE, centre, spot, GROVE_RADIUS, 0, late);
        if (f != 0)
            groves++;
        return f;
    }

    /**
     * The fire pit's centre block, from the structure start alone (MZB-242): the piece box's (minX + 4,
     * maxZ - 4) at its floor. Never the height map, which depends on what is loaded and what players build; the
     * jar's own warriors are restricted to the surface block there, which differs only by the ground's rise
     * over the pit (a restriction is a sphere of 25).
     */
    public static BlockPos groveCentre(BoundingBox firepit) {
        return new BlockPos(firepit.minX() + 4, firepit.minY(), firepit.maxZ() - 4);
    }
}
