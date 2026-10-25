package com.minecraftindustrialcomplex.mic_rtsim_iceandfire;

import com.iafenvoy.iceandfire.entity.DragonBaseEntity;
import it.unimi.dsi.fastutil.longs.LongSet;
import net.minecraft.core.BlockPos;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.Registry;
import net.minecraft.core.SectionPos;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.Tag;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.storage.LevelResource;
import net.minecraft.world.level.chunk.ChunkGenerator;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructureStart;
import net.minecraft.world.level.saveddata.SavedData;
import net.minecraft.world.phys.AABB;
import net.neoforged.neoforge.common.IOUtilities;

import java.io.File;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;

/**
 * PLACE_LAIR (OILGATE.md §4.4, OG-7): a fire roost placed near an oil field the engine found without a
 * fire lair in reach. The structure goes through vanilla's {@code /place structure} code path
 * ({@code PlaceCommand.placeStructure}): {@code Structure.generate} with every biome allowed, then
 * {@code StructureStart.placeInChunk} over the start's chunks. Server thread.
 *
 * <p><b>Where</b>: the chunks whose centre lies within the command's radius of its point, nearest the
 * point first, on the first tick a site there is free: its chunks and those around it, as far as the
 * roost reaches ({@link #SITE_CHUNKS}), loaded; no lair structure referenced in them; no player within
 * {@link #CLEAR_OF_PLAYERS} blocks. Ice and Fire's own rolls decide the rest: its roost generates in a
 * chunk with its configured chance (the pack's 0.5) and not within {@code dangerousDistanceLimit}
 * (1,000 blocks) of the world spawn, the same draw for a chunk every time; a chunk whose draw fails is
 * passed over for good. A request whose every chunk failed is dropped.
 *
 * <p><b>Once per field</b>: the fields asked for, the requests still waiting and the lairs placed are
 * saved with the overworld ({@link Data}), so a repeat (an engine replaying its outbox after a crash, a
 * restart before the site loaded) never places a second roost, and a waiting request survives a restart.
 *
 * <p><b>The lair</b>: {@code /place} itself leaves no structure start in the chunk, so nothing would know
 * the roost was there. The plugin stores the start in its chunk and a reference in each chunk of its box,
 * as world generation does: vanilla saves them with the chunks, {@link Lairs} finds the roost when its
 * chunk loads (late) and when its dragon joins (DRAGON_UNTAGGED names it), and the engine hears of it at
 * once (LAIR_FOUND, flag late). Ice and Fire's roost spawns its own dragon while it builds (verified from
 * the piece's {@code postProcess}): that dragon joins untagged and the engine adopts it as the lair's
 * occupant.
 *
 * <p><b>Durable first, checked again</b> (RP-243, RP-244): the saved state is written to its own file at the placement,
 * before the tick ends, so no chunk of the roost can reach the disk ahead of the record that it was placed (a crash
 * between the two would place a second roost at the next request); and each placed lair's start is looked for once per
 * session when its chunk loads, and placed again at the same chunk when it is not there (a crash the other way round).
 */
public final class LairPlacer {
    /** Sites tried (one {@code Structure.generate} each) per tick, PLACEHOLDER. */
    public static final int TRIES_PER_TICK = 2;
    /** Candidate chunks looked at per tick across the waiting requests, PLACEHOLDER. */
    public static final int CHECKS_PER_TICK = 64;
    /**
     * Chunks around a site's chunk that must be loaded: Ice and Fire's roost builds out to 34 blocks from
     * its pivot (its decoration radius, 12 + 0..7 - 2 + 15), and the pivot is near the chunk's middle.
     */
    public static final int SITE_CHUNKS = 3;
    /** A site is not chosen this close to a player (blocks), so no roost appears under anyone, PLACEHOLDER. */
    public static final int CLEAR_OF_PLAYERS = 48;

    /** One PLACE_LAIR still waiting for a free site. */
    public record Request(long field, String dimension, String structure, int x, int z, int radius) {}

    /**
     * One placed lair, for the log and the tests. {@code start} is the chunk the structure's start is stored in, the one
     * the lair's id comes from (RP-244); null for a lair from a file from before it, which is never checked.
     */
    public record Placed(long field, long lair, String dimension, String structure, BoundingBox box, BlockPos pivot,
                         long gameTime, int sitesTried, long placeNanos, ChunkPos start) {}

    private final DragonBridge bridge;
    /** Per waiting request: its candidate chunks, nearest first, and the index of the next to look at. */
    private final Map<Long, Cursor> cursors = new HashMap<>();

    private static final class Cursor {
        final List<ChunkPos> candidates;
        final boolean[] dead;
        int next, tried, alive;

        Cursor(List<ChunkPos> candidates) {
            this.candidates = candidates;
            this.dead = new boolean[candidates.size()];
            this.alive = candidates.size();
        }
    }

    /** Placed lairs this session has found with their start, or given up on (by field): never looked at again. */
    private final Set<Long> verified = new HashSet<>();
    /** The ticks between two looks at the placed lairs not yet verified (a lair whose chunk nobody loads stays on the list). */
    private static final int VERIFY_EVERY = 20;
    private long lastVerify = Long.MIN_VALUE;

    /** This session's counts: {@code savedNow} the placements written at once, {@code replaced} the lairs whose start was missing. */
    public long requests, repeats, refused, badRequests, placedCount, sitesTried, sitesFailed, exhausted, savedNow, replaced, replaceFailed,
            replacedDragonsDiscarded;
    /** This session's placements, oldest first. */
    public final List<Placed> placements = new ArrayList<>();

    LairPlacer(DragonBridge bridge) {
        this.bridge = bridge;
    }

    void clear() {
        cursors.clear();
        requests = repeats = refused = badRequests = placedCount = sitesTried = sitesFailed = exhausted = 0;
        savedNow = replaced = replaceFailed = replacedDragonsDiscarded = 0;
        verified.clear();
        lastVerify = Long.MIN_VALUE;
        placements.clear();
    }

    /** GameTests: forgets which placed lairs this session has looked at, so the next tick looks at each again. */
    public void unverifyForTest() {
        verified.clear();
        lastVerify = Long.MIN_VALUE;
    }

    /** The saved state, from the overworld's data storage. */
    public Data data() {
        return bridge.session().server().overworld().getDataStorage().computeIfAbsent(Data.FACTORY, Data.NAME);
    }

    /** PLACE_LAIR from the engine. */
    public void request(long field, String dimension, String structure, int x, int z, int radius) {
        requests++;
        if (!DragonConfig.placeLairs()) {
            refused++;
            MicRtsimIceAndFire.LOGGER.info("[mic_rtsim_iceandfire] PLACE_LAIR for field {} refused: placeLairs is off", field);
            return;
        }
        if (dimension == null || structure == null || IafDragons.lairKind(structure) == null || radius < 0 || level(dimension) == null) {
            badRequests++;
            MicRtsimIceAndFire.LOGGER.warn("[mic_rtsim_iceandfire] PLACE_LAIR for field {} names {} in {}: skipped", field, structure, dimension);
            return;
        }
        Data d = data();
        if (!d.fields.add(field)) {
            repeats++;
            return;
        }
        d.pending.add(new Request(field, dimension, structure, x, z, Math.min(radius, 1024)));
        d.setDirty();
        MicRtsimIceAndFire.LOGGER.info("[mic_rtsim_iceandfire] PLACE_LAIR: a {} for oil field {} within {} blocks of ({}, {})",
                structure, field, radius, x, z);
    }

    private ServerLevel level(String dimension) {
        ResourceLocation loc = ResourceLocation.tryParse(dimension);
        return loc == null ? null : bridge.session().server().getLevel(ResourceKey.create(Registries.DIMENSION, loc));
    }

    /** End of tick, before untagged dragons are reported (a roost's dragon must find its lair known). */
    void tick(long now) {
        if (!DragonConfig.placeLairs())
            return;
        Data d = data();
        if (verified.size() < d.placed.size() && now >= lastVerify + VERIFY_EVERY) {
            lastVerify = now;
            verifyPlaced(d, now);
        }
        if (d.pending.isEmpty())
            return;
        int checks = CHECKS_PER_TICK, tries = TRIES_PER_TICK;
        for (Iterator<Request> it = d.pending.iterator(); it.hasNext() && checks > 0 && tries > 0; ) {
            Request r = it.next();
            ServerLevel level = level(r.dimension());
            if (level == null)
                continue;
            Cursor c = cursors.computeIfAbsent(r.field(), k -> new Cursor(candidates(r)));
            int n = c.candidates.size();
            for (int i = 0; i < n && checks > 0 && tries > 0; i++) {
                int k = c.next;
                c.next = (c.next + 1) % Math.max(1, n);
                if (c.dead[k])
                    continue;
                checks--;
                ChunkPos at = c.candidates.get(k);
                if (!free(level, at, true))
                    continue;
                tries--;
                c.tried++;
                sitesTried++;
                Placed p = place(level, r, at, c.tried, now, false);
                if (p != null) {
                    it.remove();
                    cursors.remove(r.field());
                    d.placed.add(p);
                    verified.add(p.field());
                    d.setDirty();
                    saveNow(d);
                    break;
                }
                sitesFailed++;
                c.dead[k] = true;
                if (--c.alive == 0) {
                    exhausted++;
                    it.remove();
                    cursors.remove(r.field());
                    d.setDirty();
                    MicRtsimIceAndFire.LOGGER.info("[mic_rtsim_iceandfire] PLACE_LAIR for field {}: no chunk within {} blocks of ({}, {}) takes a {} ({} tried); dropped",
                            r.field(), r.radius(), r.x(), r.z(), r.structure(), c.tried);
                    break;
                }
            }
        }
    }

    /**
     * Writes the saved state to its own file now (RP-243): this file alone, never {@code DimensionDataStorage.save()}, which
     * would write the journal and the other stores out of their order. {@code SavedData.save(File, Provider)} (NeoForge's)
     * serializes at once, clears the dirty flag and queues the write behind the writes already queued, on a chain
     * {@code IOUtilities.waitUntilIOWorkerComplete()} then joins, so the file is on disk when this returns. It runs on the
     * server thread, where vanilla's background chunk saver runs too, so no chunk of the roost is written between the
     * placement and this. The overworld's data folder is {@code <world>/data}. Once per lair placed, and lairs are rare. The
     * flag is set again afterwards (RP-243): one more small write at the next save, and a write that failed is made again.
     * The join waits for every write already queued, so a placement the tick after an autosave waits on the server thread
     * for that autosave's whole queue (a few hundred milliseconds at most for the small stores of this world).
     */
    void saveNow(Data d) {
        try {
            MinecraftServer server = bridge.session().server();
            File file = server.getWorldPath(LevelResource.ROOT).resolve("data").resolve(Data.NAME + ".dat").toFile();
            d.setDirty();
            d.save(file, server.registryAccess());
            IOUtilities.waitUntilIOWorkerComplete();
            // The save cleared the flag at the call and an IOException in the queued write is only logged: dirty again, so
            // the next save of the world writes the file once more (one small write) and a failed write is not left to stand.
            d.setDirty();
            savedNow++;
        } catch (RuntimeException e) {
            MicRtsimIceAndFire.LOGGER.warn("[mic_rtsim_iceandfire] PLACE_LAIR: the placed lairs' file could not be written at once", e);
        }
    }

    /**
     * The first time in a session a placed lair's start chunk is loaded, its structure start is looked for (RP-244): a start
     * that is not there means the chunk is older than the record that the lair was placed (a crash after the record, before
     * the chunks: with {@link #saveNow} the record is on disk before any chunk of the roost can be), and the lair is placed
     * again at the same chunk, once. The lair's id comes from that chunk, so it stays the same and the engine, which holds
     * the lair by it, is told nothing new. Waits while the site is not loaded around or a player is near, as a placement does.
     * A pass looks at every unverified lair and stops at the first re-placement attempt (so at most one a pass; the passes
     * are {@link #VERIFY_EVERY} ticks apart); once the check ran (found, placed again or failed) the lair is not looked at
     * again this session. A re-placement builds only in the box chunks that do not hold the lair's reference yet and
     * discards the dragons it spawns when any chunk did ({@link #place}).
     */
    private void verifyPlaced(Data d, long now) {
        for (int i = 0; i < d.placed.size(); i++) {
            Placed p = d.placed.get(i);
            if (verified.contains(p.field()))
                continue;
            ServerLevel level = p.start() == null ? null : level(p.dimension());
            if (level == null) {
                verified.add(p.field()); // a lair from before the start was kept, or of a dimension that is not here
                continue;
            }
            LevelChunk home = level.getChunkSource().getChunkNow(p.start().x, p.start().z);
            if (home == null)
                continue; // not loaded yet: looked at again when it is
            Registry<Structure> reg = level.registryAccess().registryOrThrow(Registries.STRUCTURE);
            Structure structure = reg.get(ResourceLocation.parse(p.structure()));
            StructureStart start = structure == null ? null : home.getStartForStructure(structure);
            if (structure == null || (start != null && start.isValid())) {
                verified.add(p.field());
                continue;
            }
            if (!free(level, p.start(), false))
                continue;
            verified.add(p.field());
            Placed again = place(level, new Request(p.field(), p.dimension(), p.structure(), p.start().getMiddleBlockX(),
                    p.start().getMiddleBlockZ(), 0), p.start(), 1, now, true);
            if (again == null) {
                replaceFailed++;
                MicRtsimIceAndFire.LOGGER.warn("[mic_rtsim_iceandfire] lair of field {}: its start is missing from chunk {} and {} does not generate there again",
                        p.field(), p.start(), p.structure());
            } else {
                replaced++;
                d.placed.set(i, again);
                d.setDirty();
                MicRtsimIceAndFire.LOGGER.info("[mic_rtsim_iceandfire] lair of field {}: its start was missing from chunk {}: placed again",
                        p.field(), p.start());
            }
            return;
        }
    }

    /** The chunks whose centre is within the request's radius of its point, nearest first. */
    static List<ChunkPos> candidates(Request r) {
        int cx = SectionPos.blockToSectionCoord(r.x()), cz = SectionPos.blockToSectionCoord(r.z());
        int reach = (r.radius() >> 4) + 1;
        List<ChunkPos> out = new ArrayList<>();
        long r2 = (long) r.radius() * r.radius();
        for (int dx = -reach; dx <= reach; dx++)
            for (int dz = -reach; dz <= reach; dz++) {
                ChunkPos p = new ChunkPos(cx + dx, cz + dz);
                long ex = p.getMiddleBlockX() - r.x(), ez = p.getMiddleBlockZ() - r.z();
                if (ex * ex + ez * ez <= r2 || (dx == 0 && dz == 0))
                    out.add(p);
            }
        out.sort((a, b) -> Long.compare(d2(a, r), d2(b, r)));
        return out;
    }

    private static long d2(ChunkPos p, Request r) {
        long ex = p.getMiddleBlockX() - r.x(), ez = p.getMiddleBlockZ() - r.z();
        return ex * ex + ez * ez;
    }

    /**
     * A free site: loaded around, no lair structure there (when {@code references}), nobody close. A lair placed again at its
     * own chunk (RP-244) skips the reference test: its own references may survive in the box chunks that were saved.
     */
    private static boolean free(ServerLevel level, ChunkPos at, boolean references) {
        if (level.getChunkSource().getChunkNow(at.x, at.z) == null)
            return false;
        Registry<Structure> reg = level.registryAccess().registryOrThrow(Registries.STRUCTURE);
        for (int dx = -SITE_CHUNKS; dx <= SITE_CHUNKS; dx++)
            for (int dz = -SITE_CHUNKS; dz <= SITE_CHUNKS; dz++) {
                LevelChunk c = level.getChunkSource().getChunkNow(at.x + dx, at.z + dz);
                if (c == null)
                    return false;
                if (!references)
                    continue;
                for (Structure s : c.getAllReferences().keySet()) {
                    ResourceLocation id = reg.getKey(s);
                    if (id != null && IafDragons.lairKind(id.toString()) != null && !c.getReferencesForStructure(s).isEmpty())
                        return false;
                }
            }
        double mx = at.getMiddleBlockX(), mz = at.getMiddleBlockZ();
        for (ServerPlayer p : level.players()) {
            double ex = p.getX() - mx, ez = p.getZ() - mz;
            if (ex * ex + ez * ez < (double) CLEAR_OF_PLAYERS * CLEAR_OF_PLAYERS)
                return false;
        }
        return true;
    }

    /**
     * {@code PlaceCommand.placeStructure} at the chunk {@code at} (its arguments, every biome allowed), the
     * start then stored as world generation stores one; null when Ice and Fire's draw fails there.
     *
     * <p>{@code again} is a lair placed again at its own chunk (RP-244), its start missing: a box chunk that already holds
     * the lair's reference was saved with the roost's blocks, so it is not built a second time (the start is restored and
     * the references that are missing are added). When any box chunk held it, or a dragon already stands in the box (its
     * entity file was loaded), the roost has its dragon or may have had one in an entity file that was lost, and the dragons
     * this placement spawned are discarded: a roost with none at worst, never two (a loss, never a double). A placement over
     * chunks that hold no reference and no dragon (every first one, and a roost whose chunks and entities were all lost)
     * builds all of them, as before.
     */
    private Placed place(ServerLevel level, Request r, ChunkPos at, int tried, long now, boolean again) {
        Registry<Structure> reg = level.registryAccess().registryOrThrow(Registries.STRUCTURE);
        Structure structure = reg.get(ResourceLocation.parse(r.structure()));
        if (structure == null)
            return null;
        long t0 = System.nanoTime();
        ChunkGenerator gen = level.getChunkSource().getGenerator();
        StructureStart start = structure.generate(level.registryAccess(), gen, gen.getBiomeSource(), level.getChunkSource().randomState(),
                level.getStructureManager(), level.getSeed(), at, 0, level, b -> true);
        if (!start.isValid())
            return null;
        BoundingBox box = start.getBoundingBox();
        ChunkPos from = new ChunkPos(SectionPos.blockToSectionCoord(box.minX()), SectionPos.blockToSectionCoord(box.minZ()));
        ChunkPos to = new ChunkPos(SectionPos.blockToSectionCoord(box.maxX()), SectionPos.blockToSectionCoord(box.maxZ()));
        if (ChunkPos.rangeClosed(from, to).anyMatch(p -> level.getChunkSource().getChunkNow(p.x, p.z) == null))
            return null; // cannot happen for a free site (its box is within SITE_CHUNKS); /place refuses the same
        long ref = at.toLong();
        Set<Long> held = new HashSet<>(); // the box chunks that already hold this lair's reference (a lair placed again)
        if (again)
            ChunkPos.rangeClosed(from, to).forEach(p -> {
                LongSet refs = level.getChunkSource().getChunkNow(p.x, p.z).getAllReferences().get(structure);
                if (refs != null && refs.contains(ref))
                    held.add(p.toLong());
            });
        Set<UUID> dragonsBefore = again ? dragonsIn(level, box) : Set.of();
        boolean roostHasItsDragon = !held.isEmpty() || !dragonsBefore.isEmpty();
        ChunkPos.rangeClosed(from, to).forEach(p -> {
            if (!held.contains(p.toLong()))
                start.placeInChunk(level, level.structureManager(), gen, level.getRandom(),
                        new BoundingBox(p.getMinBlockX(), level.getMinBuildHeight(), p.getMinBlockZ(), p.getMaxBlockX(), level.getMaxBuildHeight(), p.getMaxBlockZ()), p);
        });
        if (roostHasItsDragon)
            for (DragonBaseEntity d : dragonEntities(level, box))
                if (!dragonsBefore.contains(d.getUUID())) {
                    replacedDragonsDiscarded++;
                    d.discard();
                }
        long nanos = System.nanoTime() - t0;
        // What world generation keeps and /place does not: the start in its chunk, a reference in each.
        LevelChunk home = level.getChunkSource().getChunkNow(at.x, at.z);
        home.setStartForStructure(structure, start);
        ChunkPos.rangeClosed(from, to).forEach(p -> {
            if (!held.contains(p.toLong()))
                level.getChunkSource().getChunkNow(p.x, p.z).addReferenceForStructure(structure, ref);
        });
        BlockPos pivot = start.getPieces().isEmpty() ? box.getCenter() : start.getPieces().get(0).getBoundingBox().getCenter();
        Lairs.Lair lair = bridge.lairs.placed(level, r.structure(), start, now);
        Placed p = new Placed(r.field(), lair.id(), r.dimension(), r.structure(), box, pivot, now, tried, nanos, at);
        placements.add(p);
        placedCount++;
        MicRtsimIceAndFire.LOGGER.info("[mic_rtsim_iceandfire] PLACE_LAIR: placed a {} for oil field {} at {} (chunk {}, {} tried, {} us)",
                r.structure(), r.field(), pivot.toShortString(), at, tried, nanos / 1000);
        return p;
    }

    /** The dragons within a roost's box (and a margin for the one that spawns off its centre). */
    private static List<DragonBaseEntity> dragonEntities(ServerLevel level, BoundingBox box) {
        return level.getEntitiesOfClass(DragonBaseEntity.class, AABB.of(box).inflate(8));
    }

    private static Set<UUID> dragonsIn(ServerLevel level, BoundingBox box) {
        Set<UUID> out = new HashSet<>();
        for (DragonBaseEntity d : dragonEntities(level, box))
            out.add(d.getUUID());
        return out;
    }

    /** What the plugin keeps with the world: the fields asked for, the waiting requests, the placed lairs. */
    public static final class Data extends SavedData {
        public static final String NAME = "mic_rtsim_iceandfire_oil_lairs";
        static final Factory<Data> FACTORY = new Factory<>(Data::new, Data::load);

        public final Set<Long> fields = new LinkedHashSet<>();
        public final List<Request> pending = new ArrayList<>();
        public final List<Placed> placed = new ArrayList<>();

        @Override
        public CompoundTag save(CompoundTag tag, HolderLookup.Provider registries) {
            tag.putLongArray("fields", fields.stream().mapToLong(Long::longValue).toArray());
            ListTag p = new ListTag();
            for (Request r : pending) {
                CompoundTag t = new CompoundTag();
                t.putLong("field", r.field());
                t.putString("dimension", r.dimension());
                t.putString("structure", r.structure());
                t.putInt("x", r.x());
                t.putInt("z", r.z());
                t.putInt("radius", r.radius());
                p.add(t);
            }
            tag.put("pending", p);
            ListTag l = new ListTag();
            for (Placed x : placed) {
                CompoundTag t = new CompoundTag();
                t.putLong("field", x.field());
                t.putLong("lair", x.lair());
                t.putString("dimension", x.dimension());
                t.putString("structure", x.structure());
                t.putIntArray("box", new int[] {x.box().minX(), x.box().minY(), x.box().minZ(), x.box().maxX(), x.box().maxY(), x.box().maxZ()});
                t.putLong("pivot", x.pivot().asLong());
                t.putLong("gameTime", x.gameTime());
                if (x.start() != null)
                    t.putLong("start", x.start().toLong());
                l.add(t);
            }
            tag.put("placed", l);
            return tag;
        }

        public static Data load(CompoundTag tag, HolderLookup.Provider registries) {
            Data d = new Data();
            for (long f : tag.getLongArray("fields"))
                d.fields.add(f);
            for (Tag e : tag.getList("pending", Tag.TAG_COMPOUND)) {
                CompoundTag t = (CompoundTag) e;
                d.pending.add(new Request(t.getLong("field"), t.getString("dimension"), t.getString("structure"), t.getInt("x"),
                        t.getInt("z"), t.getInt("radius")));
            }
            for (Tag e : tag.getList("placed", Tag.TAG_COMPOUND)) {
                CompoundTag t = (CompoundTag) e;
                int[] b = t.getIntArray("box");
                BoundingBox box = b.length == 6 ? new BoundingBox(b[0], b[1], b[2], b[3], b[4], b[5]) : new BoundingBox(0, 0, 0, 0, 0, 0);
                d.placed.add(new Placed(t.getLong("field"), t.getLong("lair"), t.getString("dimension"), t.getString("structure"), box,
                        BlockPos.of(t.getLong("pivot")), t.getLong("gameTime"), 0, 0,
                        t.contains("start", Tag.TAG_LONG) ? new ChunkPos(t.getLong("start")) : null));
            }
            return d;
        }
    }
}
