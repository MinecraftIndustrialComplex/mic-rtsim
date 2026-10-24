package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs;

import com.bobmowzie.mowziesmobs.server.world.feature.structure.FrostmawPieces;
import com.bobmowzie.mowziesmobs.server.world.feature.structure.UmvuthanaGrovePieces;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructurePiece;
import net.minecraft.world.level.levelgen.structure.StructureStart;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.event.level.ChunkEvent;

import java.util.Map;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * Finds the dens and groves (MOWZIE.md §4.9, "ChunkEvent.Load only finds camps"; the lair finder's
 * pattern, {@code Lairs.java}): a chunk holding a structure start is queued when it loads and looked at on
 * the server thread at the end of the tick; each Frostmaw den and Umvuthana grove start in it is declared
 * to the core ({@link RtsimApi#declareCamp}), which follows its footprint from then on. A camp is declared
 * once per server run; the core keeps it across sessions.
 *
 * <p>The queue holds only chunks with structure starts, and waits for a live session (the spawn chunks
 * load before it starts). D1: nothing here runs per entity or per tick but the drain of that queue.
 */
public final class MowzieCamps {
    private MowzieCamps() {}

    /** The queue's bound: chunks with starts loaded faster than the server ticks are dropped past it (PLACEHOLDER). */
    public static final int QUEUE_CAP = 4_096;

    /** The grove's footprint: the circle its warriors are restricted to ({@code UmvuthanaGrovePieces.java:507}). */
    public static final double GROVE_RADIUS = 25.0;

    private record Loaded(LevelChunk chunk, boolean late) {}

    private static final ConcurrentLinkedQueue<Loaded> QUEUE = new ConcurrentLinkedQueue<>();
    private static final AtomicInteger QUEUED = new AtomicInteger();

    /** Counters (GameTests, logs). */
    public static long dens, groves, dropped, noFirepit;

    static void onChunkLoad(ChunkEvent.Load e) {
        if (e.getLevel().isClientSide() || !(e.getChunk() instanceof LevelChunk c) || c.getAllStarts().isEmpty())
            return;
        if (QUEUED.incrementAndGet() > QUEUE_CAP) {
            QUEUED.decrementAndGet();
            dropped++;
            return;
        }
        QUEUE.add(new Loaded(c, !e.isNewChunk()));
    }

    static void drain() {
        if (QUEUE.isEmpty())
            return;
        RtsimSession s = RtsimSession.current();
        if (s == null || s.failed())
            return; // kept for the session
        for (Loaded l; (l = QUEUE.poll()) != null; ) {
            QUEUED.decrementAndGet();
            if (!(l.chunk().getLevel() instanceof ServerLevel level) || level.getServer() != s.server())
                continue; // a chunk of another world (singleplayer: the queue outlived its server)
            Registry<Structure> reg = level.registryAccess().registryOrThrow(Registries.STRUCTURE);
            for (Map.Entry<Structure, StructureStart> e : l.chunk().getAllStarts().entrySet()) {
                if (!e.getValue().isValid())
                    continue;
                ResourceLocation id = reg.getKey(e.getKey());
                if (id == null)
                    continue;
                String name = id.toString();
                if (MicRtsimMowziesMobs.DEN_STRUCTURE.equals(name))
                    declareDen(level, e.getValue(), l.late());
                else if (MicRtsimMowziesMobs.GROVE_STRUCTURE.equals(name))
                    declareGrove(level, e.getValue(), l.late());
            }
        }
    }

    static void clear() {
        QUEUE.clear();
        QUEUED.set(0);
        dens = groves = dropped = noFirepit = 0;
    }

    /**
     * A den: its footprint is the template's box (6 x 5 x 7, rotated; the core adds its slack, MZB-205),
     * its spot the box's floor centre, within a block of where the template stands its Frostmaw (at
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
        BoundingBox b = start.getBoundingBox();
        for (StructurePiece p : start.getPieces()) {
            if (p instanceof FrostmawPieces.FrostmawPiece) {
                b = p.getBoundingBox();
                break;
            }
        }
        AABB box = new AABB(b.minX(), b.minY(), b.minZ(), b.maxX() + 1, b.maxY() + 1, b.maxZ() + 1);
        Vec3 spot = denSpot(b);
        long f = RtsimApi.declareCamp(level, start.getChunkPos(), structure, DenKind.INSTANCE,
                BlockPos.containing(spot), spot, box, 0, late);
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
     * template piece's NORTH orientation is (minX + 4, maxZ - 4) of the piece's box, on the
     * MOTION_BLOCKING_NO_LEAVES surface; MZB-218). A start with no fire pit piece is no grove.
     */
    static long declareGrove(ServerLevel level, StructureStart start, boolean late) {
        return declareGrove(level, start, late, MicRtsimMowziesMobs.GROVE_STRUCTURE);
    }

    /** As above under another structure name (GameTests). */
    public static long declareGrove(ServerLevel level, StructureStart start, boolean late, String structure) {
        BoundingBox fp = null;
        for (StructurePiece p : start.getPieces()) {
            if (p instanceof UmvuthanaGrovePieces.FirepitPiece) {
                fp = p.getBoundingBox();
                break;
            }
        }
        if (fp == null) {
            noFirepit++;
            return 0;
        }
        BlockPos centre = groveCentre(level, fp);
        Vec3 spot = new Vec3(centre.getX() + 0.5, centre.getY(), centre.getZ() + 0.5);
        long f = RtsimApi.declareCamp(level, start.getChunkPos(), structure, GroveKind.INSTANCE, centre,
                spot, null, GROVE_RADIUS, late);
        if (f != 0)
            groves++;
        return f;
    }

    /** The fire pit's centre block on the surface; the piece's floor when that column is not loaded. */
    public static BlockPos groveCentre(ServerLevel level, BoundingBox firepit) {
        int x = firepit.minX() + 4, z = firepit.maxZ() - 4;
        int y = level.hasChunk(x >> 4, z >> 4) ? level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, x, z) : firepit.minY();
        return new BlockPos(x, y, z);
    }
}
