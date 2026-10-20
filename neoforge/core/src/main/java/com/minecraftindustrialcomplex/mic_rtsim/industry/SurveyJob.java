package com.minecraftindustrialcomplex.mic_rtsim.industry;

import it.unimi.dsi.fastutil.longs.Long2IntOpenHashMap;
import it.unimi.dsi.fastutil.longs.LongArrayFIFOQueue;
import it.unimi.dsi.fastutil.longs.LongArrayList;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.tags.BlockTags;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.SpawnPlacements;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.LightLayer;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.levelgen.Heightmap;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * One defence survey of one district (I21, INDUSTRY.md §13.5), read a few chunks a tick
 * ({@code surveyChunksPerTick}) and then reduced to the IND_SURVEY numbers and the exposed block set.
 * Everything it reads is vanilla or a plugin hook; the rules below are choices Q13 to Q17.
 *
 * <ul>
 *   <li><b>Columns.</b> Each column's top is the highest motion-blocking block that is not leaves
 *       ({@code MOTION_BLOCKING_NO_LEAVES}); a column whose top is a fence, wall or fence gate is a
 *       barrier (a collision box 1.5 high).</li>
 *   <li><b>Walking.</b> A walker steps from a column into a neighbour (4-neighbour) unless the neighbour
 *       is a barrier or its top is at least 2 blocks higher (INDUSTRY.md §13.5: "a column blocks if its
 *       barrier is at least 2 blocks above the walk surface, or is a fence or wall"). Dropping down is
 *       always allowed. A machine column can be attacked from any open neighbour, whatever its height.</li>
 *   <li><b>Perimeter.</b> {@code perimeterGaps} is the smallest number of open columns that separate
 *       every machine column from the outside (a minimum vertex cut of the walking graph, outside
 *       being the columns next to the district): a closed wall has 0, a wall with a one-column doorway 1,
 *       no wall the open ring around the machines. {@code perimeterColumns} is the same cut with every
 *       transition allowed: the length a closed wall around the machines needs at least.</li>
 *   <li><b>Dark spawnable.</b> Positions from a column's top surface down {@code surveyDepthBlocks}
 *       where a zombie could spawn ({@code SpawnPlacements.isSpawnPositionOk}) at block light at most
 *       the dimension's {@code monster_spawn_block_light_limit}; the district's own, and the ring's
 *       ({@code spawnRingChunks}) for the spawn samples.</li>
 *   <li><b>Machines</b> are the district's industrial block entities that are not boundary stock;
 *       roofed when a motion-blocking block is above them.</li>
 * </ul>
 */
final class SurveyJob {

    /** A block an attack could break: position, state, flags, inventory index. */
    record Exposed(BlockPos pos, BlockState state, int flags, int inventory) {}

    final District district;
    final int revision;
    final ServerLevel level;
    final long startedAt;
    private final LongArrayList chunks = new LongArrayList();
    private final LongOpenHashSet districtChunks;
    private int cursor;
    private final int depth;
    private final int blockLightLimit;

    // per column: packed (x, z)
    final Long2IntOpenHashMap top = new Long2IntOpenHashMap();
    final LongOpenHashSet barrier = new LongOpenHashSet();
    final LongOpenHashSet districtCols = new LongOpenHashSet();
    final LongOpenHashSet machineCols = new LongOpenHashSet();
    final LongOpenHashSet killCols = new LongOpenHashSet();

    int darkDistrict;
    int darkRing;
    int machines;
    int roofed;
    int killMachines;
    final List<Exposed> machineBlocks = new ArrayList<>();
    final List<Exposed> inventoryBlocks = new ArrayList<>();
    final float[] turretStrength = new float[IndRecords.ThreatClass.COUNT];
    final Map<String, Long> ammo = new HashMap<>();
    boolean aborted;

    SurveyJob(District d, ServerLevel level, long now, int ring) {
        this.district = d;
        this.revision = d.revision;
        this.level = level;
        this.startedAt = now;
        this.districtChunks = new LongOpenHashSet(d.chunks);
        this.depth = ThreatConfig.get(ThreatConfig.SURVEY_DEPTH);
        this.blockLightLimit = level.dimensionType().monsterSpawnBlockLightLimit();
        chunks.addAll(d.chunks);
        LongOpenHashSet ringSet = new LongOpenHashSet();
        int r = Math.max(1, ring); // at least the neighbours: the walkers start next to the district
        for (long c : d.chunks)
            for (int dx = -r; dx <= r; dx++)
                for (int dz = -r; dz <= r; dz++) {
                    long n = ChunkPos.asLong(ChunkPos.getX(c) + dx, ChunkPos.getZ(c) + dz);
                    if (!districtChunks.contains(n) && ringSet.add(n))
                        chunks.add(n);
                }
        this.ringLimit = ring;
    }

    private final int ringLimit;

    static long col(int x, int z) {
        return ((long) x & 0xFFFF_FFFFL) | ((long) z << 32);
    }

    static int colX(long c) {
        return (int) c;
    }

    static int colZ(long c) {
        return (int) (c >> 32);
    }

    boolean done() {
        return aborted || cursor >= chunks.size();
    }

    /** Reads up to {@code budget} chunks. A district chunk that is not loaded aborts the survey. */
    void step(Industry ind, int budget) {
        while (budget-- > 0 && cursor < chunks.size()) {
            long c = chunks.getLong(cursor++);
            boolean inDistrict = districtChunks.contains(c);
            LevelChunk chunk = level.getChunkSource().getChunkNow(ChunkPos.getX(c), ChunkPos.getZ(c));
            if (chunk == null) {
                if (inDistrict) {
                    aborted = true;
                    return;
                }
                continue;
            }
            boolean countRing = !inDistrict && withinRing(c);
            readColumns(chunk, inDistrict, countRing);
            if (inDistrict)
                readBlockEntities(ind, chunk);
        }
    }

    private boolean withinRing(long c) {
        if (ringLimit <= 0)
            return false;
        int x = ChunkPos.getX(c), z = ChunkPos.getZ(c);
        for (long d : districtChunks)
            if (Math.abs(ChunkPos.getX(d) - x) <= ringLimit && Math.abs(ChunkPos.getZ(d) - z) <= ringLimit)
                return true;
        return false;
    }

    private void readColumns(LevelChunk chunk, boolean inDistrict, boolean countRing) {
        int bx = chunk.getPos().getMinBlockX(), bz = chunk.getPos().getMinBlockZ();
        BlockPos.MutableBlockPos p = new BlockPos.MutableBlockPos();
        int minY = level.getMinBuildHeight();
        for (int lx = 0; lx < 16; lx++) {
            for (int lz = 0; lz < 16; lz++) {
                int x = bx + lx, z = bz + lz;
                int t = chunk.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, lx, lz);
                long key = col(x, z);
                top.put(key, t);
                BlockState ts = chunk.getBlockState(p.set(x, t, z));
                if (ts.is(BlockTags.FENCES) || ts.is(BlockTags.WALLS) || ts.is(BlockTags.FENCE_GATES))
                    barrier.add(key);
                if (inDistrict)
                    districtCols.add(key);
                if (inDistrict || countRing) {
                    int dark = 0;
                    for (int y = t + 1; y > t + 1 - depth && y > minY; y--) {
                        p.set(x, y, z);
                        if (SpawnPlacements.isSpawnPositionOk(EntityType.ZOMBIE, level, p)
                                && level.getBrightness(LightLayer.BLOCK, p) <= blockLightLimit)
                            dark++;
                    }
                    if (inDistrict)
                        darkDistrict += dark;
                    else
                        darkRing += dark;
                }
            }
        }
    }

    private void readBlockEntities(Industry ind, LevelChunk chunk) {
        Map<BlockPos, Boolean> cache = new HashMap<>();
        for (BlockEntity be : new ArrayList<>(chunk.getBlockEntities().values())) {
            if (be.isRemoved())
                continue;
            BlockPos pos = be.getBlockPos();
            for (IndustryPlugin plugin : IndustryApi.PLUGINS) {
                try {
                    if (plugin.killMachine(be)) {
                        killMachines++;
                        killCols.add(col(pos.getX(), pos.getZ()));
                    }
                    plugin.turret(be, (tp, strength, ammoGood, rounds) -> {
                        for (int c = 0; c < IndRecords.ThreatClass.COUNT && c < strength.length; c++)
                            turretStrength[c] += strength[c];
                        if (ammoGood != null)
                            ammo.merge(ammoGood, rounds, Long::sum);
                    });
                } catch (RuntimeException e) {
                    ind.fail("survey hook of " + plugin.getClass().getSimpleName() + " at " + pos, e);
                }
            }
            if (!ind.districts.industrial(level, be, cache))
                continue;
            int topHere = chunk.getHeight(Heightmap.Types.MOTION_BLOCKING, pos.getX() & 15, pos.getZ() & 15);
            boolean isRoofed = topHere > pos.getY();
            District.Inventory inv = district.inventoryAt(pos);
            int flags = isRoofed ? 0 : IndRecords.SurveyBlocks.FLAG_ROOFLESS;
            if (inv != null) {
                inventoryBlocks.add(new Exposed(pos, be.getBlockState(), flags | IndRecords.SurveyBlocks.FLAG_INVENTORY, inv.index()));
            } else {
                machines++;
                if (isRoofed)
                    roofed++;
                machineCols.add(col(pos.getX(), pos.getZ()));
                machineBlocks.add(new Exposed(pos, be.getBlockState(), flags | IndRecords.SurveyBlocks.FLAG_MACHINE, IndRecords.NO_INVENTORY));
            }
        }
    }

    // ---- the reduction --------------------------------------------------------------------------

    /** The results of the walk and the cuts. */
    record Walk(int perimeterColumns, int perimeterGaps, int killAtGaps, List<Long> wallColumns, boolean cutComputed) {}

    private static final int[] DX = {1, -1, 0, 0};
    private static final int[] DZ = {0, 0, 1, -1};

    private int topOf(long c, long fallbackFrom) {
        if (top.containsKey(c))
            return top.get(c);
        return top.getOrDefault(fallbackFrom, 0);
    }

    private boolean canEnter(long from, long to) {
        if (barrier.contains(to))
            return false;
        if (machineCols.contains(to))
            return true;
        return topOf(to, from) - topOf(from, to) <= 1;
    }

    /** The walk from outside, the kill machines it meets, the wall faces, and the two minimum cuts. */
    Walk walk() {
        // Outside: every column next to the district that is not the district's.
        LongOpenHashSet outside = new LongOpenHashSet();
        for (long c : districtCols)
            for (int k = 0; k < 4; k++) {
                long n = col(colX(c) + DX[k], colZ(c) + DZ[k]);
                if (!districtCols.contains(n))
                    outside.add(n);
            }
        // Reach: breadth first from outside through allowed transitions.
        LongOpenHashSet reached = new LongOpenHashSet();
        LongArrayFIFOQueue q = new LongArrayFIFOQueue();
        for (long o : outside)
            q.enqueue(o);
        LongOpenHashSet seen = new LongOpenHashSet(outside);
        List<Long> walls = new ArrayList<>();
        LongOpenHashSet wallSet = new LongOpenHashSet();
        while (!q.isEmpty()) {
            long a = q.dequeueLong();
            for (int k = 0; k < 4; k++) {
                long b = col(colX(a) + DX[k], colZ(a) + DZ[k]);
                if (!districtCols.contains(b) || seen.contains(b))
                    continue;
                if (canEnter(a, b)) {
                    seen.add(b);
                    reached.add(b);
                    if (!machineCols.contains(b))
                        q.enqueue(b);
                } else if (wallSet.add(b)) {
                    walls.add(b);
                }
            }
        }
        walls.removeIf(c -> reached.contains(c.longValue()));
        int killAtGaps = 0;
        for (long k : killCols) {
            boolean near = reached.contains(k) || outside.contains(k);
            for (int i = 0; i < 4 && !near; i++) {
                long n = col(colX(k) + DX[i], colZ(k) + DZ[i]);
                near = reached.contains(n) || outside.contains(n);
            }
            if (near)
                killAtGaps++;
        }
        if (machineCols.isEmpty())
            return new Walk(0, 0, killAtGaps, walls, true);
        // The cut graph: the machines' bounding box plus a margin, inside the district.
        int margin = ThreatConfig.get(ThreatConfig.SURVEY_MARGIN);
        int minX = Integer.MAX_VALUE, maxX = Integer.MIN_VALUE, minZ = Integer.MAX_VALUE, maxZ = Integer.MIN_VALUE;
        for (long m : machineCols) {
            minX = Math.min(minX, colX(m));
            maxX = Math.max(maxX, colX(m));
            minZ = Math.min(minZ, colZ(m));
            maxZ = Math.max(maxZ, colZ(m));
        }
        minX -= margin;
        maxX += margin;
        minZ -= margin;
        maxZ += margin;
        long area = (long) (maxX - minX + 1) * (maxZ - minZ + 1);
        if (area > ThreatConfig.get(ThreatConfig.SURVEY_MAX_COLUMNS))
            return new Walk(0, 0, killAtGaps, walls, false);
        int gaps = minCut(minX, maxX, minZ, maxZ, reached, outside, true);
        int ring = minCut(minX, maxX, minZ, maxZ, reached, outside, false);
        return new Walk(ring, gaps, killAtGaps, walls, true);
    }

    /**
     * The minimum number of district columns (not machines) whose removal cuts every machine column off
     * from outside, in the box. Sources: columns outside the district, and box-edge columns the walk
     * reached (walkers come in from beyond the box). {@code walls}: the walking rules; false, every
     * transition is open. Edmonds-Karp on a unit vertex-capacity graph (each column split in two).
     */
    private int minCut(int minX, int maxX, int minZ, int maxZ, LongOpenHashSet reached, LongOpenHashSet outside, boolean walls) {
        int w = maxX - minX + 1, h = maxZ - minZ + 1;
        int cols = w * h;
        int src = 2 * cols, sink = 2 * cols + 1;
        Graph g = new Graph(2 * cols + 2, cols * 12 + 16);
        final int inf = 1 << 28;
        for (int ix = 0; ix < w; ix++) {
            for (int iz = 0; iz < h; iz++) {
                int x = minX + ix, z = minZ + iz;
                long c = col(x, z);
                int v = ix * h + iz;
                boolean inD = districtCols.contains(c);
                boolean machine = machineCols.contains(c);
                boolean edge = ix == 0 || iz == 0 || ix == w - 1 || iz == h - 1;
                // split: v_in = v, v_out = v + cols. Every column but a machine's can be closed, the
                // columns just outside the district too (a machine on the district's edge).
                g.add(v, v + cols, machine ? inf : 1);
                if (machine)
                    g.add(v + cols, sink, inf);
                if (!inD) {
                    if (outside.contains(c) || edge)
                        g.add(src, v, inf);
                    else
                        continue; // a column of another district deep inside the box: not walkable here
                } else if (edge && (reached.contains(c) || !walls)) {
                    g.add(src, v, inf);
                }
                for (int k = 0; k < 4; k++) {
                    int nx = ix + DX[k], nz = iz + DZ[k];
                    if (nx < 0 || nz < 0 || nx >= w || nz >= h)
                        continue;
                    long nc = col(minX + nx, minZ + nz);
                    if (!districtCols.contains(nc))
                        continue; // walkers enter the district, they do not walk outside it here
                    if (machine)
                        continue; // nobody walks out of a machine column
                    if (walls && !canEnter(c, nc))
                        continue;
                    g.add(v + cols, nx * h + nz, inf);
                }
            }
        }
        return g.maxFlow(src, sink, inf);
    }

    /** A flow network with int arrays (residual edges paired). */
    private static final class Graph {
        int[] head, next, to, cap;
        int edges;

        Graph(int nodes, int expectEdges) {
            head = new int[nodes];
            java.util.Arrays.fill(head, -1);
            next = new int[expectEdges * 2];
            to = new int[expectEdges * 2];
            cap = new int[expectEdges * 2];
        }

        void add(int u, int v, int c) {
            grow();
            to[edges] = v;
            cap[edges] = c;
            next[edges] = head[u];
            head[u] = edges++;
            to[edges] = u;
            cap[edges] = 0;
            next[edges] = head[v];
            head[v] = edges++;
        }

        private void grow() {
            if (edges + 2 <= to.length)
                return;
            int n = Math.max(16, to.length * 2);
            next = java.util.Arrays.copyOf(next, n);
            to = java.util.Arrays.copyOf(to, n);
            cap = java.util.Arrays.copyOf(cap, n);
        }

        int maxFlow(int s, int t, int inf) {
            int flow = 0;
            int n = head.length;
            int[] via = new int[n];
            int[] queue = new int[n];
            while (true) {
                java.util.Arrays.fill(via, -1);
                via[s] = -2;
                int qh = 0, qt = 0;
                queue[qt++] = s;
                while (qh < qt && via[t] == -1) {
                    int u = queue[qh++];
                    for (int e = head[u]; e >= 0; e = next[e]) {
                        if (cap[e] > 0 && via[to[e]] == -1) {
                            via[to[e]] = e;
                            queue[qt++] = to[e];
                        }
                    }
                }
                if (via[t] == -1)
                    return flow;
                int push = inf;
                for (int v = t; v != s; v = to[via[v] ^ 1])
                    push = Math.min(push, cap[via[v]]);
                if (push >= inf)
                    return flow; // cannot happen: every path crosses a column of capacity 1
                for (int v = t; v != s; v = to[via[v] ^ 1]) {
                    cap[via[v]] -= push;
                    cap[via[v] ^ 1] += push;
                }
                flow += push;
            }
        }
    }
}
