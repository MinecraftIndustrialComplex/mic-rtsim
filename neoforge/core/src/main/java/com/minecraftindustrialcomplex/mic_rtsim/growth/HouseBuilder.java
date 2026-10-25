package com.minecraftindustrialcomplex.mic_rtsim.growth;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.village.Cultures;
import com.minecraftindustrialcomplex.mic_rtsim.village.Village;
import com.minecraftindustrialcomplex.mic_rtsim.village.VillageRegistry;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.tags.BlockTags;
import net.minecraft.tags.FluidTags;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.Mirror;
import net.minecraft.world.level.block.Rotation;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.templatesystem.BlockIgnoreProcessor;
import net.minecraft.world.level.levelgen.structure.templatesystem.JigsawReplacementProcessor;
import net.minecraft.world.level.levelgen.structure.templatesystem.StructurePlaceSettings;
import net.minecraft.world.level.levelgen.structure.templatesystem.StructureProcessorList;
import net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplate;
import net.neoforged.neoforge.attachment.AttachmentHolder;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.concurrent.CompletableFuture;

/**
 * Village growth from a mod's own house templates (C7, PLAN.md §8.8, P17's recommendation): one
 * {@code StructureTemplate} placed through vanilla's {@code placeInWorld}, on a free plot, a few layers
 * per tick under a block quota, never over player-placed blocks.
 *
 * <p><b>As worldgen places it.</b> The settings are the ones {@code SinglePoolElement.getSettings}
 * gives a jigsaw piece: structure blocks ignored, jigsaw blocks replaced by their final state
 * ({@code JigsawReplacementProcessor}; a final state of structure void places nothing), then the
 * piece's processor list (for Ribbits, {@code ribbits:main}: mossy planks, pillars to the ground, lapis
 * and nylium markers). Entities in the template are not placed: residents come from SPAWN_RESIDENT.
 * The per-block random is seeded by position, so a placement resumed after a restart is identical.
 *
 * <p><b>Free plot</b> (choice G13; the margins are PLACEHOLDERs): the house's box, widened by
 * {@link #SPACING}, overlaps no building of the village (its structure pieces and the houses the sim
 * added); no block in it was placed by a player ({@link PlayerBlocks}); it holds no block entity; above
 * its floor layer every block is air, a plant, leaves or water, and its floor layer may also be
 * terrain; and most of the ground under it is solid or water within {@link #MAX_DROP} blocks.
 *
 * <p><b>Pillar processors write around vanilla.</b> Ribbits' {@code PillarProcessor} extends pillars
 * downward with {@code ChunkAccess.setBlockState}, which in a live level updates light but tells no
 * client (it is written for worldgen). Once a house is complete the bridge marks the columns under it
 * changed ({@code ServerChunkCache.blockChanged}), so watching players see the pillars (choice G13).
 */
public final class HouseBuilder {

    /** Blocks placed per tick, all houses together (PLACEHOLDER). */
    public static final int BLOCK_QUOTA = 512;
    /** Ticks between tries while a plot's chunks are not loaded (PLACEHOLDER). */
    public static final int RETRY_TICKS = 100;
    /** Blocks kept clear between a new house and any other building (PLACEHOLDER). */
    public static final int SPACING = 2;
    /** Ground must be within this many blocks under a footprint column (PLACEHOLDER). */
    public static final int MAX_DROP = 4;
    /** Plot candidates tested per tick, all houses together (PLACEHOLDER; each scans up to a house's volume). */
    public static final int CANDIDATES_PER_TICK = 8;
    /** Ticks to wait while a mob or player stands where the next layers go (PLACEHOLDER). */
    public static final int OCCUPIED_RETRY = 20;
    /** Ring spacing of the plot search, blocks (PLACEHOLDER). */
    public static final int RING_STEP = 4;
    /** How far below a house the client-sync sweep looks for pillar blocks (PLACEHOLDER). */
    public static final int PILLAR_SWEEP = 32;
    /** How long a searching build waits for the saved chunks of its plots to be read before it goes on without them. */
    public static final long PLOT_READ_WAIT_NANOS = 10_000_000_000L;
    /** Ticks between looks at a searching build's chunk reads (PLACEHOLDER). */
    public static final int PLOT_READ_RETRY = 2;

    private final RtsimSession session;
    private final VillageRegistry villages;
    /** Where each build's search stands (not saved: a reload starts the search again). */
    private final Map<Long, Integer> searchIndex = new HashMap<>();
    private final Map<Long, String> lastRefusal = new HashMap<>();
    private final Map<Long, List<int[]>> ringCache = new HashMap<>();
    /** Each searching build's look for its op in the chunks of all its plots (not saved: a reload looks again). */
    private final Map<Long, OpLook> looks = new HashMap<>();
    long placedBlocks;
    long completed;
    long refused;
    /** Chunks of plots read from disk; builds found already standing by the look; looks that went on without every read. */
    long plotReads, foundByLook, looksCut;

    public HouseBuilder(RtsimSession session, VillageRegistry villages) {
        this.session = session;
        this.villages = villages;
    }

    public long completed() {
        return completed;
    }

    public long refused() {
        return refused;
    }

    /** The look for a searching build's op across its plots (RP-211): chunks read, builds found standing, looks cut short. */
    public String lookCounts() {
        return "plot chunks read " + plotReads + ", found standing " + foundByLook + ", looks cut short " + looksCut;
    }

    /** A PLACE_HOUSE was accepted. Duplicates were already answered by the caller. */
    public void accept(VillageRegistry.Build b) {
        villages.builds.put(b.op, b);
        villages.setDirty();
    }

    public boolean building(long op) {
        return villages.builds.containsKey(op);
    }

    static StructurePlaceSettings settings(ServerLevel level, VillageRegistry.Build b) {
        StructurePlaceSettings s = new StructurePlaceSettings()
                .setRotation(Rotation.values()[Math.floorMod(b.rotation, 4)])
                .setMirror(Mirror.values()[Math.floorMod(b.mirror, 3)])
                .setIgnoreEntities(true)
                .setKnownShape(true)
                .addProcessor(BlockIgnoreProcessor.STRUCTURE_BLOCK)
                .addProcessor(JigsawReplacementProcessor.INSTANCE);
        if (b.processors != null) {
            Optional<StructureProcessorList> list = level.registryAccess().registryOrThrow(Registries.PROCESSOR_LIST).getOptional(b.processors);
            list.ifPresent(l -> l.list().forEach(s::addProcessor));
        }
        return s;
    }

    /** One tick of building: searches and layers under the quotas. */
    public void tick(long now) {
        if (villages.builds.isEmpty())
            return;
        int blocks = BLOCK_QUOTA;
        int[] candidates = {CANDIDATES_PER_TICK};
        for (VillageRegistry.Build b : new ArrayList<>(villages.builds.values())) {
            if (blocks <= 0 && candidates[0] <= 0)
                break;
            if (now < b.nextTry)
                continue;
            try {
                blocks = step(b, now, blocks, candidates);
            } catch (RuntimeException ex) {
                MicRtsim.LOGGER.error("[mic_rtsim] house {} failed", b.op, ex);
                finish(b, Obs.CommandResult.FAILED, null, null, null, now);
            }
        }
    }

    private int step(VillageRegistry.Build b, long now, int blocks, int[] candidates) {
        ServerLevel level = session.server().getLevel(b.dimension);
        Village v = villages.get(b.village);
        Optional<StructureTemplate> template = level == null ? Optional.empty() : level.getStructureManager().get(b.template);
        if (level == null || v == null || template.isEmpty()) {
            MicRtsim.LOGGER.warn("[mic_rtsim] house {}: {}", b.op, level == null ? "unknown dimension" : v == null ? "unknown village" : "no template " + b.template);
            finish(b, Obs.CommandResult.FAILED, null, null, null, now);
            return blocks;
        }
        StructureTemplate t = template.get();
        StructurePlaceSettings s = settings(level, b);
        if (b.origin == null) {
            Search r = search(level, v, b, t, s, candidates);
            if (r == Search.NOT_FREE) {
                finish(b, Obs.CommandResult.NOT_FREE, null, null, null, now);
                return blocks;
            }
            if (r == Search.WAIT) {
                b.nextTry = now + RETRY_TICKS;
                return blocks;
            }
            if (r == Search.READING) {
                b.nextTry = now + PLOT_READ_RETRY;
                return blocks;
            }
            if (r == Search.BUILT) {
                finish(b, Obs.CommandResult.DUPLICATE_OP, null, null, null, now);
                return blocks;
            }
            if (b.origin == null)
                return blocks; // more candidates next tick
            villages.setDirty();
        }
        BoundingBox box = t.getBoundingBox(s, b.origin);
        if (!loaded(level, box)) {
            b.nextTry = now + RETRY_TICKS;
            return blocks;
        }
        int area = box.getXSpan() * box.getZSpan();
        int height = box.getYSpan();
        while (blocks > 0 && b.nextLayer < height) {
            int layers = Math.max(1, Math.min(height - b.nextLayer, blocks / Math.max(area, 1)));
            BoundingBox slice = new BoundingBox(box.minX(), box.minY() + b.nextLayer, box.minZ(),
                    box.maxX(), box.minY() + b.nextLayer + layers - 1, box.maxZ());
            // The plot was free when it was chosen; layers go in later (a quota, a wait, a restart). What
            // is still to be built is checked again: a player's block or a block entity there now stops the
            // house, and a mob or player standing in the next layers makes it wait rather than wall them in.
            BoundingBox rest = new BoundingBox(box.minX(), slice.minY(), box.minZ(), box.maxX(), box.maxY(), box.maxZ());
            String taken = takenSince(level, rest);
            if (taken != null) {
                MicRtsim.LOGGER.info("[mic_rtsim] house {} stopped at layer {} of {}: {}", b.op, b.nextLayer, height, taken);
                finish(b, Obs.CommandResult.NOT_FREE, null, null, null, now);
                return blocks;
            }
            if (!level.getEntitiesOfClass(net.minecraft.world.entity.LivingEntity.class, net.minecraft.world.phys.AABB.of(slice)).isEmpty()) {
                b.nextTry = now + OCCUPIED_RETRY;
                return blocks;
            }
            t.placeInWorld(level, b.origin, b.origin, settings(level, b).setBoundingBox(slice), level.getRandom(), 2);
            b.nextLayer += layers;
            blocks -= layers * area;
            placedBlocks += (long) layers * area;
            villages.setDirty();
        }
        if (b.nextLayer >= height) {
            syncPillars(level, box);
            finish(b, Obs.CommandResult.APPLIED, v, box, slot(level, v, b, t, s, box), now);
        }
        return blocks;
    }

    private enum Search { FOUND, NOT_FREE, WAIT, MORE, BUILT, READING }

    /** The chunk reads a searching build waits for, by chunk ({@link ChunkPos#toLong}). */
    private static final class OpLook {
        final Map<Long, CompletableFuture<Optional<CompoundTag>>> reads = new HashMap<>();
        final long since = System.nanoTime();
        int unread;
        boolean done;
    }

    /** A block a player placed or a block entity in what is still to be built, or null. */
    static String takenSince(ServerLevel level, BoundingBox rest) {
        for (int cx = rest.minX() >> 4; cx <= rest.maxX() >> 4; cx++) {
            for (int cz = rest.minZ() >> 4; cz <= rest.maxZ() >> 4; cz++) {
                LevelChunk chunk = level.getChunkSource().getChunkNow(cx, cz);
                if (chunk == null)
                    continue;
                if (PlayerBlocks.anyIn(chunk, rest))
                    return "a block a player placed";
                for (BlockPos be : chunk.getBlockEntities().keySet())
                    if (rest.isInside(be))
                        return "a block entity at " + be;
            }
        }
        return null;
    }

    /** Tries plot candidates under the per-tick budget; sets {@code b.origin} when one is free. */
    private Search search(ServerLevel level, Village v, VillageRegistry.Build b, StructureTemplate t, StructurePlaceSettings s, int[] budget) {
        int y = ((b.flags & com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd.PlaceHouse.FLAG_Y_FROM_VILLAGE) != 0 ? v.baseY : b.y) + b.yOffset;
        boolean searching = (b.flags & com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd.PlaceHouse.FLAG_SEARCH) != 0;
        List<int[]> candidates = searching ? ringCache.computeIfAbsent(b.op, k -> rings(b)) : List.of(new int[] {b.x, b.z});
        if (searching) {
            Search look = lookForOp(level, b, t, s, y, candidates);
            if (look != null)
                return look;
        }
        int i = searchIndex.getOrDefault(b.op, 0);
        boolean unloaded = false;
        for (; i < candidates.size() && budget[0] > 0; i++) {
            budget[0]--;
            int[] c = candidates.get(i);
            BlockPos origin = searching ? centred(t, s, c[0], y, c[1]) : new BlockPos(c[0], y, c[1]);
            BoundingBox box = t.getBoundingBox(s, origin);
            if (!loaded(level, box)) {
                unloaded = true;
                continue;
            }
            if (carriesOp(level, box, b.op)) {
                // This op's house already stands here: a chunk of its box was saved after the house was
                // built and before the world save that would have recorded it (the registry rolled back).
                // Every chunk of the box carries the result (finish, RP-211). A searching build's look found
                // it before any plot was tried; this is the fixed plot of a build that does not search.
                searchIndex.remove(b.op);
                return Search.BUILT;
            }
            String why = whyNotFree(level, v, box);
            if (why == null) {
                b.origin = origin;
                searchIndex.remove(b.op);
                return Search.FOUND;
            }
            lastRefusal.put(b.op, why);
        }
        if (i < candidates.size()) {
            searchIndex.put(b.op, i);
            return Search.MORE;
        }
        searchIndex.remove(b.op);
        if (!unloaded)
            MicRtsim.LOGGER.info("[mic_rtsim] house {} for village {}: no free plot among {} candidates; the last was refused for {}",
                    b.op, b.village, candidates.size(), lastRefusal.remove(b.op));
        return unloaded ? Search.WAIT : Search.NOT_FREE;
    }

    /**
     * Before a searching build takes any plot, it looks for its op in every chunk its candidate plots touch (C3,
     * RP-211): this op's house may stand on any of them. After a crash a resend searches from the first candidate
     * again, and the plot the house stands on may be unloaded, or an earlier one loaded and free, so a look at the
     * candidate in hand would miss it. A loaded chunk is looked at as it is ({@code chunkCarriesOp}); one not loaded
     * is read as saved ({@code ChunkStorage.read}, off the server thread) and the build held until the reads are in,
     * at most {@link #PLOT_READ_WAIT_NANOS}, after which it goes on without the rest (logged and counted). BUILT when
     * a chunk carries the op, READING while reads are out, null when the search can go on.
     */
    private Search lookForOp(ServerLevel level, VillageRegistry.Build b, StructureTemplate t, StructurePlaceSettings s, int y, List<int[]> candidates) {
        OpLook look = looks.get(b.op);
        if (look != null && look.done)
            return null;
        if (look == null) {
            look = new OpLook();
            looks.put(b.op, look);
            Set<Long> chunks = new LinkedHashSet<>();
            for (int[] c : candidates) {
                BoundingBox box = t.getBoundingBox(s, centred(t, s, c[0], y, c[1]));
                for (int cx = box.minX() >> 4; cx <= box.maxX() >> 4; cx++)
                    for (int cz = box.minZ() >> 4; cz <= box.maxZ() >> 4; cz++)
                        chunks.add(ChunkPos.asLong(cx, cz));
            }
            for (long p : chunks) {
                LevelChunk chunk = level.getChunkSource().getChunkNow(ChunkPos.getX(p), ChunkPos.getZ(p));
                if (chunk != null) {
                    if (RtsimSession.chunkCarriesOp(chunk, b.op))
                        return foundByLook(b, new ChunkPos(p));
                    continue;
                }
                try {
                    look.reads.put(p, level.getChunkSource().chunkMap.read(new ChunkPos(p)).thenApply(HouseBuilder::attachmentsOnly));
                    plotReads++;
                } catch (RuntimeException e) {
                    look.unread++;
                }
            }
        }
        for (var it = look.reads.entrySet().iterator(); it.hasNext(); ) {
            var e = it.next();
            if (!e.getValue().isDone())
                continue;
            it.remove();
            Optional<CompoundTag> tag;
            try {
                tag = e.getValue().join();
            } catch (RuntimeException ex) {
                look.unread++;
                continue;
            }
            if (tag.isPresent() && RtsimSession.opsCarriedInChunkTag(tag.get(), level.registryAccess()).contains(b.op))
                return foundByLook(b, new ChunkPos(e.getKey()));
        }
        if (!look.reads.isEmpty()) {
            if (System.nanoTime() - look.since < PLOT_READ_WAIT_NANOS)
                return Search.READING;
            look.unread += look.reads.size();
            look.reads.clear();
        }
        look.done = true;
        if (look.unread > 0) {
            looksCut++;
            MicRtsim.LOGGER.warn("[mic_rtsim] house {}: {} chunks of its plots were not read in {} s; the search goes on without them "
                    + "(docs/choices-journal-barrier.md RP-211)", b.op, look.unread, PLOT_READ_WAIT_NANOS / 1_000_000_000L);
        }
        return null;
    }

    private Search foundByLook(VillageRegistry.Build b, ChunkPos where) {
        foundByLook++;
        looks.remove(b.op);
        searchIndex.remove(b.op);
        MicRtsim.LOGGER.info("[mic_rtsim] house {}: chunk {} carries its result, so it stands already", b.op, where);
        return Search.BUILT;
    }

    /** The NeoForge attachments of a chunk's saved data, all a look needs of it (the rest is let go on the IO thread). */
    private static Optional<CompoundTag> attachmentsOnly(Optional<CompoundTag> chunk) {
        return chunk.map(tag -> {
            CompoundTag keep = new CompoundTag();
            keep.put(AttachmentHolder.ATTACHMENTS_NBT_KEY, tag.getCompound(AttachmentHolder.ATTACHMENTS_NBT_KEY));
            return keep;
        });
    }

    /** Candidate centres on rings around (x, z), out to the radius, starting at an angle from the seed. */
    static List<int[]> rings(VillageRegistry.Build b) {
        List<int[]> out = new ArrayList<>();
        double start = (b.seed & 0xFFFF) / 65536.0 * Math.PI * 2;
        out.add(new int[] {b.x, b.z});
        for (int r = RING_STEP; r <= Math.max(RING_STEP, b.radius); r += RING_STEP) {
            int n = Math.max(8, (int) Math.ceil(2 * Math.PI * r / RING_STEP));
            for (int k = 0; k < n; k++) {
                double a = start + 2 * Math.PI * k / n;
                out.add(new int[] {b.x + (int) Math.round(r * Math.cos(a)), b.z + (int) Math.round(r * Math.sin(a))});
            }
        }
        return out;
    }

    /** The origin that puts the template's box centred on (x, z). */
    static BlockPos centred(StructureTemplate t, StructurePlaceSettings s, int x, int y, int z) {
        BlockPos o = new BlockPos(x, y, z);
        BoundingBox box = t.getBoundingBox(s, o);
        return o.offset(x - (box.minX() + box.maxX()) / 2, 0, z - (box.minZ() + box.maxZ()) / 2);
    }

    /** True when a chunk under the box carries this op's COMMAND_RESULT (the house was built: B42's record). */
    static boolean carriesOp(ServerLevel level, BoundingBox box, long op) {
        for (int cx = box.minX() >> 4; cx <= box.maxX() >> 4; cx++)
            for (int cz = box.minZ() >> 4; cz <= box.maxZ() >> 4; cz++) {
                LevelChunk chunk = level.getChunkSource().getChunkNow(cx, cz);
                if (chunk != null && RtsimSession.chunkCarriesOp(chunk, op))
                    return true;
            }
        return false;
    }

    static boolean loaded(ServerLevel level, BoundingBox box) {
        for (int cx = box.minX() >> 4; cx <= box.maxX() >> 4; cx++)
            for (int cz = box.minZ() >> 4; cz <= box.maxZ() >> 4; cz++)
                if (level.getChunkSource().getChunkNow(cx, cz) == null)
                    return false;
        return true;
    }

    /** The free-plot test (see the class comment). The chunks must be loaded. */
    public static boolean free(ServerLevel level, Village v, BoundingBox box) {
        return whyNotFree(level, v, box) == null;
    }

    /** Null when the plot is free, else why not (for the log). */
    public static String whyNotFree(ServerLevel level, Village v, BoundingBox box) {
        BoundingBox wide = box.inflatedBy(SPACING);
        if (v != null) {
            for (BoundingBox p : v.pieces)
                if (p.intersects(wide))
                    return "a village building";
            for (Village.House h : v.houses)
                if (h.box().intersects(wide))
                    return "a house the sim built";
        }
        if (box.minY() < level.getMinBuildHeight() || box.maxY() >= level.getMaxBuildHeight())
            return "outside the build height";
        for (int cx = box.minX() >> 4; cx <= box.maxX() >> 4; cx++) {
            for (int cz = box.minZ() >> 4; cz <= box.maxZ() >> 4; cz++) {
                LevelChunk chunk = level.getChunkSource().getChunkNow(cx, cz);
                if (chunk == null)
                    return "not loaded";
                if (PlayerBlocks.anyIn(chunk, box))
                    return "a block a player placed";
                for (BlockPos be : chunk.getBlockEntities().keySet())
                    if (box.isInside(be))
                        return "a block entity at " + be;
            }
        }
        BlockPos.MutableBlockPos m = new BlockPos.MutableBlockPos();
        int grounded = 0, columns = 0;
        for (int x = box.minX(); x <= box.maxX(); x++) {
            for (int z = box.minZ(); z <= box.maxZ(); z++) {
                for (int y = box.minY(); y <= box.maxY(); y++) {
                    BlockState st = level.getBlockState(m.set(x, y, z));
                    if (clearable(st))
                        continue;
                    if (y == box.minY() && terrain(st))
                        continue;
                    return st + " at " + m.immutable();
                }
                columns++;
                for (int d = 1; d <= MAX_DROP; d++) {
                    BlockState below = level.getBlockState(m.set(x, box.minY() - d, z));
                    if (terrain(below) || below.getFluidState().is(FluidTags.WATER) || below.isSolidRender(level, m)) {
                        grounded++;
                        break;
                    }
                }
            }
        }
        return grounded * 2 >= columns ? null : "no ground under " + (columns - grounded) + " of " + columns + " columns";
    }

    static boolean clearable(BlockState s) {
        return s.isAir() || s.canBeReplaced() || s.is(BlockTags.LEAVES) || s.is(BlockTags.REPLACEABLE_BY_TREES)
                || s.is(BlockTags.FLOWERS) || s.is(BlockTags.SAPLINGS) || s.is(Blocks.LILY_PAD)
                || (s.getFluidState().is(FluidTags.WATER) && !s.hasBlockEntity() && s.getBlock() == Blocks.WATER);
    }

    static boolean terrain(BlockState s) {
        return s.is(BlockTags.DIRT) || s.is(BlockTags.SAND) || s.is(BlockTags.BASE_STONE_OVERWORLD) || s.is(Blocks.GRAVEL)
                || s.is(Blocks.CLAY) || s.is(Blocks.MUD) || s.is(BlockTags.SNOW) || s.is(BlockTags.TERRACOTTA);
    }

    /** Marks the columns under a house changed, for the pillars a processor wrote around vanilla. */
    static void syncPillars(ServerLevel level, BoundingBox box) {
        BlockPos.MutableBlockPos m = new BlockPos.MutableBlockPos();
        int bottom = Math.max(level.getMinBuildHeight(), box.minY() - PILLAR_SWEEP);
        for (int x = box.minX(); x <= box.maxX(); x++)
            for (int z = box.minZ(); z <= box.maxZ(); z++)
                for (int y = box.minY() - 1; y >= bottom; y--)
                    level.getChunkSource().blockChanged(m.set(x, y, z));
    }

    /**
     * Where a resident of the house stands: above the template's jigsaw block whose pool is the
     * culture's resident pool (Ribbits: {@code ribbits:ribbits}), else the middle of the floor.
     */
    static BlockPos slot(ServerLevel level, Village v, VillageRegistry.Build b, StructureTemplate t, StructurePlaceSettings s, BoundingBox box) {
        String pool = Cultures.residentPool(v.culture);
        if (pool != null) {
            for (StructureTemplate.StructureBlockInfo info : t.filterBlocks(b.origin, s, Blocks.JIGSAW))
                if (info.nbt() != null && pool.equals(info.nbt().getString("pool")))
                    return info.pos().above();
        }
        return new BlockPos((box.minX() + box.maxX()) / 2, box.minY() + 1, (box.minZ() + box.maxZ()) / 2);
    }

    /** Ends a build: the house record, HOUSE_PLACED and the COMMAND_RESULT, which every chunk of the house's box carries. */
    private void finish(VillageRegistry.Build b, int status, Village v, BoundingBox box, BlockPos slot, long now) {
        villages.builds.remove(b.op);
        searchIndex.remove(b.op);
        ringCache.remove(b.op);
        looks.remove(b.op);
        villages.setDirty();
        ServerLevel level = session.server().getLevel(b.dimension);
        if (status == Obs.CommandResult.APPLIED && v != null && box != null) {
            v.houses.add(new Village.House(b.op, box, slot));
            villages.grow(v, box); // the house and its resident belong to the village
            ByteBuffer r = session.boundary().beginJournaled(Obs.HOUSE_PLACED, Obs.HousePlaced.SIZE, 0, now);
            r.putLong(Obs.HousePlaced.OP_ID, b.op);
            r.putLong(Obs.HousePlaced.VILLAGE, b.village);
            r.putInt(Obs.HousePlaced.DIM, session.dimId(b.dimension));
            r.putInt(Obs.HousePlaced.TEMPLATE, session.dictId(Obs.DICT_TEMPLATE, b.template.toString()));
            r.putInt(Obs.HousePlaced.MIN_X, box.minX());
            r.putInt(Obs.HousePlaced.MIN_Y, box.minY());
            r.putInt(Obs.HousePlaced.MIN_Z, box.minZ());
            r.putInt(Obs.HousePlaced.MAX_X, box.maxX());
            r.putInt(Obs.HousePlaced.MAX_Y, box.maxY());
            r.putInt(Obs.HousePlaced.MAX_Z, box.maxZ());
            r.putInt(Obs.HousePlaced.SLOT_X, slot.getX());
            r.putInt(Obs.HousePlaced.SLOT_Y, slot.getY());
            r.putInt(Obs.HousePlaced.SLOT_Z, slot.getZ());
            r.put(Obs.HousePlaced.ROTATION, (byte) b.rotation);
            r.put(Obs.HousePlaced.MIRROR, (byte) b.mirror);
            session.boundary().commit(r);
            completed++;
        } else if (status != Obs.CommandResult.APPLIED) {
            refused++;
        }
        ByteBuffer result = session.commandResult(b.cmdSeq, b.op, status, now);
        if (status == Obs.CommandResult.APPLIED && level != null && box != null) {
            // Every chunk of the box carries it, and the journal's next file waits for each (C3): the search
            // after a crash looks for the op in every chunk of the box (carriesOp), so a box chunk that was
            // saved ahead of the journal answers DUPLICATE_OP rather than a second house beside the first.
            // The copies are one record; the engine takes a repeated result by state.
            for (int cx = box.minX() >> 4; cx <= box.maxX() >> 4; cx++)
                for (int cz = box.minZ() >> 4; cz <= box.maxZ() >> 4; cz++) {
                    LevelChunk chunk = level.getChunkSource().getChunkNow(cx, cz);
                    if (chunk != null)
                        session.carryResultInChunk(chunk, result);
                }
        }
    }
}
