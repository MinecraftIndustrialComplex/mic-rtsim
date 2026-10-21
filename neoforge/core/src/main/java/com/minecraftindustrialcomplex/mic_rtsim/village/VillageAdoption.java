package com.minecraftindustrialcomplex.mic_rtsim.village;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LevelAccessor;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructurePiece;
import net.minecraft.world.level.levelgen.structure.StructureStart;

import java.nio.ByteBuffer;
import java.util.Collections;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentLinkedQueue;

/**
 * A village joins the sim when its structure start generates (C22).
 *
 * <p><b>How the bridge sees it</b> (the hook PLAN.md §6.3 left UNVERIFIED, checked in the 1.21.1
 * source): {@code ChunkGenerator.tryGenerateStructure} hands every valid start to
 * {@code StructureManager.setStartForStructure}, on a worldgen thread, during the chunk's
 * STRUCTURE_STARTS step. A mixin there ({@code StructureManagerMixin}) queues the starts of village
 * structures; the server thread adopts them at the end of the tick. {@code /place structure} does not
 * pass through it, and neither does a world made before the mod: so a chunk that loads with a village
 * start the registry does not know is adopted then, flagged LATE.
 *
 * <p>The filter on the worldgen thread is a set of {@link Structure} objects computed on the server
 * thread at start (and after a datapack reload), so no registry or tag is read off-thread.
 */
public final class VillageAdoption {
    private VillageAdoption() {}

    record Seen(ResourceKey<Level> dimension, Structure structure, StructureStart start, boolean late) {}

    private static final ConcurrentLinkedQueue<Seen> QUEUE = new ConcurrentLinkedQueue<>();
    private static volatile Set<Structure> villageStructures = Collections.emptySet();

    /** Recomputes which structures are villages. Server thread. */
    public static void refresh(MinecraftServer server) {
        refresh(server.registryAccess());
    }

    /**
     * Recomputes which structures are villages: when the server is about to start (before it prepares
     * the spawn area, so starts generated then are seen) and after every datapack reload.
     */
    public static void refresh(net.minecraft.core.RegistryAccess access) {
        Registry<Structure> reg = access.registryOrThrow(Registries.STRUCTURE);
        Set<Structure> set = new HashSet<>();
        for (Map.Entry<ResourceKey<Structure>, Structure> e : reg.entrySet()) {
            Holder<Structure> h = reg.wrapAsHolder(e.getValue());
            if (Cultures.match(h) != null)
                set.add(e.getValue());
        }
        villageStructures = Collections.unmodifiableSet(set);
        MicRtsim.LOGGER.info("[mic_rtsim] {} structures are villages the sim adopts", set.size());
    }

    /** From {@code StructureManagerMixin}, on a worldgen thread. */
    public static void generated(LevelAccessor level, Structure structure, StructureStart start) {
        if (level instanceof ServerLevel sl && start.isValid() && villageStructures.contains(structure))
            QUEUE.add(new Seen(sl.dimension(), structure, start, false));
    }

    /** A chunk loaded: a village start in it that the registry does not know is adopted late. */
    public static void loaded(LevelChunk chunk) {
        Map<Structure, StructureStart> starts = chunk.getAllStarts();
        if (starts.isEmpty() || !(chunk.getLevel() instanceof ServerLevel sl))
            return;
        Set<Structure> villages = villageStructures;
        for (Map.Entry<Structure, StructureStart> e : starts.entrySet())
            if (e.getValue().isValid() && villages.contains(e.getKey()))
                QUEUE.add(new Seen(sl.dimension(), e.getKey(), e.getValue(), true));
    }

    public static int queued() {
        return QUEUE.size();
    }

    /** The server stopped: starts seen in this world must not be adopted into the next one (singleplayer). */
    public static void clear() {
        QUEUE.clear();
        villageStructures = Collections.emptySet();
    }

    /** Adopts what the queue holds. Server thread, end of tick. */
    public static void drain(RtsimSession session, VillageRegistry villages, long now) {
        for (Seen s; (s = QUEUE.poll()) != null; ) {
            try {
                adopt(session, villages, s, now);
            } catch (RuntimeException ex) {
                MicRtsim.LOGGER.error("[mic_rtsim] adopting a village failed", ex);
            }
        }
    }

    private static void adopt(RtsimSession session, VillageRegistry villages, Seen s, long now) {
        MinecraftServer server = session.server();
        Registry<Structure> reg = server.registryAccess().registryOrThrow(Registries.STRUCTURE);
        Holder<Structure> holder = reg.wrapAsHolder(s.structure());
        Cultures.Rule rule = Cultures.match(holder);
        ResourceLocation id = reg.getKey(s.structure());
        if (rule == null || id == null)
            return;
        int dim = session.dimId(s.dimension());
        long villageId = Obs.villageId(dim, s.start().getChunkPos().x, s.start().getChunkPos().z);
        if (villages.get(villageId) != null)
            return;
        StructureStart start = s.start();
        BoundingBox box = start.getBoundingBox();
        BoundingBox first = start.getPieces().isEmpty() ? box : start.getPieces().get(0).getBoundingBox();
        BlockPos meeting = first.getCenter();
        Village v = new Village(villageId, s.dimension(), id, rule.culture(), now, box, meeting, first.minY());
        for (StructurePiece p : start.getPieces())
            v.pieces.add(p.getBoundingBox());
        villages.add(v);

        int structureId = session.dictId(Obs.DICT_STRUCTURE, id.toString());
        int cultureId = session.dictId(Obs.DICT_CULTURE, rule.culture());
        ByteBuffer r = session.boundary().beginJournaled(Obs.VILLAGE_ADOPTED, Obs.VillageAdopted.SIZE,
                s.late() ? Obs.VillageAdopted.FLAG_LATE : 0, now);
        r.putLong(Obs.VillageAdopted.VILLAGE, villageId);
        r.putInt(Obs.VillageAdopted.DIM, dim);
        r.putInt(Obs.VillageAdopted.STRUCTURE, structureId);
        r.putInt(Obs.VillageAdopted.CULTURE, cultureId);
        r.putInt(Obs.VillageAdopted.PIECES, start.getPieces().size());
        r.putInt(Obs.VillageAdopted.MIN_X, box.minX());
        r.putInt(Obs.VillageAdopted.MIN_Y, box.minY());
        r.putInt(Obs.VillageAdopted.MIN_Z, box.minZ());
        r.putInt(Obs.VillageAdopted.MAX_X, box.maxX());
        r.putInt(Obs.VillageAdopted.MAX_Y, box.maxY());
        r.putInt(Obs.VillageAdopted.MAX_Z, box.maxZ());
        r.putInt(Obs.VillageAdopted.MEET_X, meeting.getX());
        r.putInt(Obs.VillageAdopted.MEET_Y, meeting.getY());
        r.putInt(Obs.VillageAdopted.MEET_Z, meeting.getZ());
        session.boundary().commit(r);
        MicRtsim.LOGGER.debug("[mic_rtsim] adopted {} village {} ({}) at {}", rule.culture(), villageId, id, meeting);
    }

    /**
     * Every village the registry holds, adopted again (flag LATE): for an engine whose pages lack
     * observations the journal no longer holds (a sim reset, {@code BoundarySession#observationsLost}).
     * The registry is the world's, so its villages are never adopted again by their starts; without
     * this the new engine would know none of them. The engine takes a village it knows as a no-op (C22).
     * Returns how many were sent.
     */
    public static int announceAgain(RtsimSession session, VillageRegistry villages, long now) {
        int n = 0;
        for (Village v : villages.all()) {
            BoundingBox box = v.box;
            BlockPos meeting = v.meeting;
            ByteBuffer r = session.boundary().beginJournaled(Obs.VILLAGE_ADOPTED, Obs.VillageAdopted.SIZE,
                    Obs.VillageAdopted.FLAG_LATE, now);
            r.putLong(Obs.VillageAdopted.VILLAGE, v.id);
            r.putInt(Obs.VillageAdopted.DIM, session.dimId(v.dimension));
            r.putInt(Obs.VillageAdopted.STRUCTURE, session.dictId(Obs.DICT_STRUCTURE, v.structure.toString()));
            r.putInt(Obs.VillageAdopted.CULTURE, session.dictId(Obs.DICT_CULTURE, v.culture));
            r.putInt(Obs.VillageAdopted.PIECES, v.pieces.size());
            r.putInt(Obs.VillageAdopted.MIN_X, box.minX());
            r.putInt(Obs.VillageAdopted.MIN_Y, box.minY());
            r.putInt(Obs.VillageAdopted.MIN_Z, box.minZ());
            r.putInt(Obs.VillageAdopted.MAX_X, box.maxX());
            r.putInt(Obs.VillageAdopted.MAX_Y, box.maxY());
            r.putInt(Obs.VillageAdopted.MAX_Z, box.maxZ());
            r.putInt(Obs.VillageAdopted.MEET_X, meeting.getX());
            r.putInt(Obs.VillageAdopted.MEET_Y, meeting.getY());
            r.putInt(Obs.VillageAdopted.MEET_Z, meeting.getZ());
            session.boundary().commit(r);
            n++;
        }
        return n;
    }

    /**
     * Adopts a village directly (GameTests, and later a command): the same record a generated start
     * produces. Returns the village (the existing one when its id is known).
     */
    public static Village adoptDirect(RtsimSession session, VillageRegistry villages, ResourceKey<Level> dimension,
                                      ResourceLocation structure, String culture, BoundingBox box, BlockPos meeting, long now) {
        return adoptDirect(session, villages, Obs.villageId(session.dimId(dimension), meeting.getX() >> 4, meeting.getZ() >> 4),
                dimension, structure, culture, box, meeting, now);
    }

    /** As above, under a given id (GameTests use ids no structure start can have). */
    public static Village adoptDirect(RtsimSession session, VillageRegistry villages, long villageId, ResourceKey<Level> dimension,
                                      ResourceLocation structure, String culture, BoundingBox box, BlockPos meeting, long now) {
        return adoptDirect(session, villages, villageId, dimension, structure, culture, box, meeting, 0, now);
    }

    /**
     * As above, with the structure's piece count (the engine guesses a village's population from it
     * until its first census; GameTests use it for villages far from any loaded chunk).
     */
    public static Village adoptDirect(RtsimSession session, VillageRegistry villages, long villageId, ResourceKey<Level> dimension,
                                      ResourceLocation structure, String culture, BoundingBox box, BlockPos meeting, int pieces,
                                      long now) {
        int dim = session.dimId(dimension);
        Village existing = villages.get(villageId);
        if (existing != null)
            return existing;
        Village v = new Village(villageId, dimension, structure, culture, now, box, meeting, meeting.getY());
        villages.add(v);
        ByteBuffer r = session.boundary().beginJournaled(Obs.VILLAGE_ADOPTED, Obs.VillageAdopted.SIZE, 0, now);
        r.putLong(Obs.VillageAdopted.VILLAGE, villageId);
        r.putInt(Obs.VillageAdopted.DIM, dim);
        r.putInt(Obs.VillageAdopted.STRUCTURE, session.dictId(Obs.DICT_STRUCTURE, structure.toString()));
        r.putInt(Obs.VillageAdopted.CULTURE, session.dictId(Obs.DICT_CULTURE, culture));
        r.putInt(Obs.VillageAdopted.PIECES, pieces);
        r.putInt(Obs.VillageAdopted.MIN_X, box.minX());
        r.putInt(Obs.VillageAdopted.MIN_Y, box.minY());
        r.putInt(Obs.VillageAdopted.MIN_Z, box.minZ());
        r.putInt(Obs.VillageAdopted.MAX_X, box.maxX());
        r.putInt(Obs.VillageAdopted.MAX_Y, box.maxY());
        r.putInt(Obs.VillageAdopted.MAX_Z, box.maxZ());
        r.putInt(Obs.VillageAdopted.MEET_X, meeting.getX());
        r.putInt(Obs.VillageAdopted.MEET_Y, meeting.getY());
        r.putInt(Obs.VillageAdopted.MEET_Z, meeting.getZ());
        session.boundary().commit(r);
        return v;
    }
}
