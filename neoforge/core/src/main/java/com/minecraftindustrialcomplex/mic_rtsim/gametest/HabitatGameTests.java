package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.habitat.HabitatRecords;
import com.minecraftindustrialcomplex.mic_rtsim.habitat.HabitatSampler;
import com.minecraftindustrialcomplex.mic_rtsim.herd.HerdBridge;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ThreadLocalRandom;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The habitat sampler and the engine's world model (docs/boundary.md §10; choices X2-# in
 * docs/overnight-choices-worldmodel.md). The GameTest world is superflat plains (the FLAT preset), so each test
 * paints the water it needs at a place far from every earlier run's (the GameTest world is reused, B37), and
 * loads its chunks itself. Each test has its own batch.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class HabitatGameTests {
    private HabitatGameTests() {}

    /** Loads (and forces) the chunks of a block rectangle in full, now. */
    private static List<ChunkPos> load(ServerLevel level, int x0, int z0, int x1, int z1) {
        List<ChunkPos> forced = new ArrayList<>();
        for (int cx = x0 >> 4; cx <= (x1 - 1) >> 4; cx++)
            for (int cz = z0 >> 4; cz <= (z1 - 1) >> 4; cz++) {
                if (level.setChunkForced(cx, cz, true))
                    forced.add(new ChunkPos(cx, cz));
                level.getChunk(cx, cz);
            }
        return forced;
    }

    /** The flat world's top block y at a column. */
    private static int top(ServerLevel level, int x, int z) {
        return level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, x, z) - 1;
    }

    /** Puts a block on top of a column (no neighbour updates: the painted ground stays as painted). */
    private static void paint(ServerLevel level, int x, int z, BlockState state) {
        level.setBlock(new BlockPos(x, top(level, x, z) + 1, z), state, Block.UPDATE_CLIENTS | Block.UPDATE_KNOWN_SHAPE);
    }

    /** The latest HABITAT the engine received for each cell of a dimension, by cell key. */
    private static Map<Long, ScriptedEngine.Received> habitats(int dim) {
        Map<Long, ScriptedEngine.Received> out = new HashMap<>();
        for (ScriptedEngine.Received r : RtsimTests.engine().received(HabitatRecords.HABITAT)) {
            ByteBuffer b = r.view();
            if (b.getInt(HabitatRecords.Habitat.DIM) == dim)
                out.put(HabitatRecords.key(b.getInt(HabitatRecords.Habitat.CELL_X), b.getInt(HabitatRecords.Habitat.CELL_Z)), r);
        }
        return out;
    }

    private static int count(int dim, int cx, int cz) {
        int n = 0;
        for (ScriptedEngine.Received r : RtsimTests.engine().received(HabitatRecords.HABITAT)) {
            ByteBuffer b = r.view();
            if (b.getInt(HabitatRecords.Habitat.DIM) == dim && b.getInt(HabitatRecords.Habitat.CELL_X) == cx
                    && b.getInt(HabitatRecords.Habitat.CELL_Z) == cz)
                n++;
        }
        return n;
    }

    private static int u16(ByteBuffer b, int at) {
        return b.getShort(at) & 0xFFFF;
    }

    /** A record's creature entries: body name to {share, generated}. */
    private static Map<String, int[]> creatures(ScriptedEngine.Received r) {
        ByteBuffer b = r.view();
        Map<String, int[]> out = new HashMap<>();
        for (int i = 0; i < r.aux(); i++) {
            int e = HabitatRecords.Habitat.CREATURES + HabitatRecords.Habitat.CREATURE * i;
            out.put(RtsimTests.engine().javaDictName(b.getInt(e)), new int[]{u16(b, e + 4), u16(b, e + 6)});
        }
        return out;
    }

    /**
     * Docs/boundary.md §10 on either engine: the chunks of four cells far out are loaded; one cell is painted
     * water at its sixteen sample points, one ice, two are left as the flat world's plains. Each cell crosses
     * once as HABITAT, with its masks, the plains biome by Java's dictionary, the flat surface, and the herd
     * species' share of vanilla plains' creature spawns (sheep 12 of 46: 260 per mille). Nothing is sent again
     * while nothing changes; a cell whose ground changed is sent again when it is sampled again, one that did
     * not is not. On the native engine the world model holds each cell with the fingerprint Java sent.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 1200, batch = "habitat_sampled")
    public static void habitatIsSentForAGeneratedArea(GameTestHelper helper) {
        String test = "habitatIsSentForAGeneratedArea";
        ServerLevel level = helper.getLevel();
        int x0 = -3_000_000 - ThreadLocalRandom.current().nextInt(0, 4_000) * 1_024;
        int z0 = 3_000_000 + ThreadLocalRandom.current().nextInt(0, 4_000) * 1_024;
        int cx0 = x0 >> 6, cz0 = z0 >> 6;
        int dim = RtsimTests.session().dimId(level.dimension());
        List<ChunkPos> forced = new ArrayList<>();
        HabitatSampler sampler = HabitatSampler.INSTANCE;
        long[] before = new long[2];
        helper.startSequence()
                .thenWaitUntil(() -> check("the sampler has the engine's list", sampler.synced()))
                .thenExecute(() -> {
                    before[0] = sampler.cellsSampled;
                    before[1] = sampler.sampleNanos;
                    forced.addAll(load(level, x0, z0, x0 + 128, z0 + 128));
                    // Cell (1, 0) water and cell (0, 1) ice at their sixteen points: each chunk's centre column.
                    for (int i = 0; i < 16; i++) {
                        int px = (i & 3) * 16 + 8, pz = (i >> 2) * 16 + 8;
                        paint(level, x0 + 64 + px, z0 + pz, Blocks.WATER.defaultBlockState());
                        paint(level, x0 + px, z0 + 64 + pz, Blocks.ICE.defaultBlockState());
                    }
                })
                .thenWaitUntil(() -> {
                    Map<Long, ScriptedEngine.Received> h = habitats(dim);
                    for (int dx = 0; dx < 2; dx++)
                        for (int dz = 0; dz < 2; dz++)
                            check("cell (" + dx + ", " + dz + ") sent", h.containsKey(HabitatRecords.key(cx0 + dx, cz0 + dz)));
                })
                .thenExecute(() -> {
                    Map<Long, ScriptedEngine.Received> h = habitats(dim);
                    int surface = top(level, x0 + 8 + 16, z0 + 8 + 16);
                    for (int dx = 0; dx < 2; dx++)
                        for (int dz = 0; dz < 2; dz++) {
                            ScriptedEngine.Received r = h.get(HabitatRecords.key(cx0 + dx, cz0 + dz));
                            ByteBuffer b = r.view();
                            String where = "cell (" + dx + ", " + dz + ")";
                            check(where + " is whole", u16(b, HabitatRecords.Habitat.KNOWN) == 0xFFFF);
                            int water = u16(b, HabitatRecords.Habitat.WATER), ice = u16(b, HabitatRecords.Habitat.ICE);
                            if (dx == 1 && dz == 0)
                                check(where + " is water at every point (" + Integer.toHexString(water) + ")", water == 0xFFFF && ice == 0);
                            else if (dx == 0 && dz == 1)
                                check(where + " is ice at every point (" + Integer.toHexString(ice) + ")", ice == 0xFFFF && water == 0);
                            else {
                                check(where + " is land", water == 0 && ice == 0 && u16(b, HabitatRecords.Habitat.FLUID) == 0);
                                check(where + " stands at the flat surface (" + b.getShort(HabitatRecords.Habitat.SURFACE_MEAN) + " vs " + surface + ")",
                                        b.getShort(HabitatRecords.Habitat.SURFACE_MEAN) == surface);
                                String land = RtsimTests.engine().javaDictName(b.getInt(HabitatRecords.Habitat.LAND_BIOME));
                                check(where + "'s land biome is plains (" + land + ")", "minecraft:plains".equals(land));
                                check(where + " is tagged overworld", (b.getInt(HabitatRecords.Habitat.LAND_TAGS) & (1 << 21)) != 0);
                                Map<String, int[]> c = creatures(r);
                                int[] sheep = c.get("minecraft:sheep");
                                check(where + " supports sheep at vanilla plains' 12 of 46 (" + (sheep == null ? "none" : sheep[0]) + ")",
                                        sheep != null && sheep[0] == 260);
                                check(where + " has no goats", !c.containsKey("minecraft:goat"));
                                check("vanilla plains: 0.8 and 0.4", Math.abs(b.getFloat(HabitatRecords.Habitat.TEMPERATURE) - 0.8f) < 1e-6
                                        && Math.abs(b.getFloat(HabitatRecords.Habitat.DOWNFALL) - 0.4f) < 1e-6);
                                check("no climate plugin: no degrees, no season", Float.isNaN(b.getFloat(HabitatRecords.Habitat.CELSIUS))
                                        && b.get(HabitatRecords.Habitat.SEASON_COUNT) == 0);
                            }
                            check(where + " was sent once", count(dim, cx0 + dx, cz0 + dz) == 1);
                            if (RtsimTests.engine() instanceof NativeEngine e) {
                                long[] info = e.habitatInfo(dim, (cx0 + dx) * 64 + 8, (cz0 + dz) * 64 + 8);
                                check(where + " is in the engine's world model with its fingerprint",
                                        info[NativeEngine.HABITAT_KNOWN_CELL] == 1
                                                && (int) info[NativeEngine.HABITAT_FINGERPRINT] == b.getInt(HabitatRecords.Habitat.FINGERPRINT)
                                                && info[NativeEngine.HABITAT_WATER] == water);
                            }
                        }
                    long cells = sampler.cellsSampled - before[0];
                    record(test, "sampled", cells + " cells in " + (sampler.sampleNanos - before[1]) / 1000 + " us; " + sampler.summary());
                })
                .thenIdle(60)
                .thenExecute(() -> {
                    for (int dx = 0; dx < 2; dx++)
                        for (int dz = 0; dz < 2; dz++)
                            check("nothing sent again while nothing changed", count(dim, cx0 + dx, cz0 + dz) == 1);
                    // The ground of cell (0, 0) changes at one point; cell (1, 1) is sampled again unchanged.
                    paint(level, x0 + 8, z0 + 8, Blocks.WATER.defaultBlockState());
                    sampler.resample(level, cx0, cz0);
                    sampler.resample(level, cx0 + 1, cz0 + 1);
                })
                .thenWaitUntil(() -> check("the changed cell is sent again", count(dim, cx0, cz0) == 2))
                .thenIdle(10)
                .thenExecute(() -> {
                    ByteBuffer b = habitats(dim).get(HabitatRecords.key(cx0, cz0)).view();
                    check("its first point is water now", u16(b, HabitatRecords.Habitat.WATER) == 1);
                    check("the unchanged cell is not sent again", count(dim, cx0 + 1, cz0 + 1) == 1);
                    record(test, "changed", "a point turned to water: sent again; an unchanged cell sampled again: not sent");
                    forced.forEach(p -> level.setChunkForced(p.x, p.z, false));
                })
                .thenSucceed();
    }

    /**
     * The sampler's cost (only with {@code -PrtsimBench}, results in {@code bench/results/habitat-sampler.md}):
     * a 32 x 32 chunk area far out is loaded (64 cells, a quarter of them painted water at their points); every
     * cell is read 50 times as the sampler reads it, warm, and once with the biome facts forgotten (cold). The
     * chunk-load hook's own cost comes from the sampler's counters over the whole run.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 2400, batch = "habitat_bench")
    public static void habitatSamplerCost(GameTestHelper helper) {
        String test = "habitatSamplerCost";
        if (!Boolean.getBoolean("mic_rtsim.bench")) {
            record(test, "skipped", "only with -PrtsimBench");
            helper.succeed();
            return;
        }
        ServerLevel level = helper.getLevel();
        int x0 = 6_000_000 + ThreadLocalRandom.current().nextInt(0, 2_000) * 1_024;
        int z0 = 6_000_000 + ThreadLocalRandom.current().nextInt(0, 2_000) * 1_024;
        List<ChunkPos> forced = new ArrayList<>();
        HabitatSampler sampler = HabitatSampler.INSTANCE;
        helper.startSequence()
                .thenExecute(() -> {
                    long t0 = System.nanoTime();
                    forced.addAll(load(level, x0, z0, x0 + 512, z0 + 512));
                    long loadNanos = System.nanoTime() - t0;
                    for (int cz = 0; cz < 8; cz++)
                        for (int cx = 0; cx < 2; cx++)
                            for (int i = 0; i < 16; i++)
                                paint(level, x0 + cx * 64 + (i & 3) * 16 + 8, z0 + cz * 64 + (i >> 2) * 16 + 8,
                                        Blocks.WATER.defaultBlockState());
                    record(test, "loaded", forced.size() + " chunks generated and loaded in " + loadNanos / 1_000_000 + " ms");
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    int cx0 = x0 >> 6, cz0 = z0 >> 6;
                    long cold = sampler.benchRead(level, cx0, cz0, true);
                    long[] all = new long[64 * 50];
                    int n = 0;
                    for (int round = 0; round < 50; round++)
                        for (int dz = 0; dz < 8; dz++)
                            for (int dx = 0; dx < 8; dx++) {
                                long t = sampler.benchRead(level, cx0 + dx, cz0 + dz, false);
                                check("every cell's chunks are loaded", t >= 0);
                                all[n++] = t;
                            }
                    java.util.Arrays.sort(all);
                    long sum = 0;
                    for (long t : all)
                        sum += t;
                    double mean = sum / 1000.0 / all.length;
                    record(test, "cost", String.format(
                            "cell reads %d: mean %.2f us, p50 %.2f us, p90 %.2f us, p99 %.2f us, max %.2f us per cell "
                                    + "(%.3f us per chunk); cold biome facts %.2f us; chunk-load hook %.3f us per load over %d loads",
                            all.length, mean, all[all.length / 2] / 1000.0, all[all.length * 9 / 10] / 1000.0,
                            all[all.length * 99 / 100] / 1000.0, all[all.length - 1] / 1000.0, mean / 16, cold / 1000.0,
                            sampler.chunkLoads == 0 ? 0 : sampler.loadHookNanos / 1000.0 / sampler.chunkLoads, sampler.chunkLoads));
                    record(test, "sampler", sampler.summary());
                    forced.forEach(p -> level.setChunkForced(p.x, p.z, false));
                })
                .thenSucceed();
    }

    /** Writes a PLAYER record for a GameTest player (the server's player list does not hold them). */
    private static void reportPlayer(ServerPlayer p) {
        var s = RtsimTests.session();
        int dim = s.dimId(p.level().dimension()); // before direct(): an id is defined before its first use
        int at = s.boundary().direct(Obs.PLAYER, Obs.Player.SIZE, dim, s.gameTime());
        if (at < 0)
            return;
        ByteBuffer b = s.boundary().openBuffer();
        b.putLong(at + Obs.Player.UUID_MOST, p.getUUID().getMostSignificantBits());
        b.putLong(at + Obs.Player.UUID_LEAST, p.getUUID().getLeastSignificantBits());
        b.putDouble(at + Obs.Player.X, p.getX());
        b.putDouble(at + Obs.Player.Y, p.getY());
        b.putDouble(at + Obs.Player.Z, p.getZ());
    }

    /**
     * The world model founds a herd (native engine only; the stub decides nothing). A quarter of a region far
     * out is loaded (4 x 4 cells), its western half painted a sea; the sampler sends the sixteen cells and the
     * engine founds one herd for the region, of a plains species, on the land half. A player beside it sees
     * the herd materialize: every animal on land, and none walks into the sea while the player watches.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 3000, batch = "habitat_herd")
    public static void aHerdFoundedFromTheHabitatWalksOnLand(GameTestHelper helper) {
        String test = "aHerdFoundedFromTheHabitatWalksOnLand";
        if (!(RtsimTests.engine() instanceof NativeEngine e)) {
            record(test, "skipped", "the stub engine decides nothing; this test needs the native engine's world model");
            helper.succeed();
            return;
        }
        ServerLevel level = helper.getLevel();
        int x0 = 512 * (7_900 + ThreadLocalRandom.current().nextInt(0, 2_000) * 4);
        int z0 = 512 * (-7_900 - ThreadLocalRandom.current().nextInt(0, 2_000) * 4);
        int sea = x0 + 128; // west of it: sea
        int dim = RtsimTests.session().dimId(level.dimension());
        List<ChunkPos> forced = new ArrayList<>();
        ServerPlayer[] player = new ServerPlayer[1];
        boolean[] watching = {false};
        long[] agent = {0};
        int[] seen = {0};
        helper.onEachTick(() -> {
            if (player[0] != null && RtsimTests.session().gameTime() % 20 == 0)
                reportPlayer(player[0]);
            if (watching[0] && agent[0] != 0)
                for (Entity a : HerdBridge.INSTANCE.bodies(agent[0])) {
                    check("animal at x " + a.getX() + " stays out of the sea (x < " + sea + ")", a.getX() >= sea);
                    check("animal at " + a.blockPosition() + " stands on land", level.getFluidState(a.blockPosition()).isEmpty()
                            && level.getFluidState(a.blockPosition().below()).isEmpty());
                    seen[0]++;
                }
        });
        helper.startSequence()
                .thenWaitUntil(() -> check("the sampler has the engine's list", HabitatSampler.INSTANCE.synced()))
                .thenExecute(() -> {
                    forced.addAll(load(level, x0, z0, x0 + 256, z0 + 256));
                    BlockState water = Blocks.WATER.defaultBlockState();
                    for (int x = x0; x < sea; x++)
                        for (int z = z0; z < z0 + 256; z++)
                            paint(level, x, z, water);
                })
                .thenWaitUntil(() -> check("a herd founded for the region",
                        e.habitatInfo(dim, sea + 64, z0 + 64)[NativeEngine.HABITAT_REGION_HERD] != 0))
                .thenExecute(() -> {
                    long[] land = e.habitatInfo(dim, sea + 64, z0 + 64);
                    long[] wet = e.habitatInfo(dim, x0 + 8, z0 + 8);
                    check("the world model holds 16 new cells: a sea and land", wet[NativeEngine.HABITAT_WATER] == 0xFFFF
                            && land[NativeEngine.HABITAT_LAND] == 0xFFFF);
                    agent[0] = land[NativeEngine.HABITAT_REGION_HERD];
                    long hx = land[NativeEngine.HABITAT_HERD_X], hz = land[NativeEngine.HABITAT_HERD_Z];
                    check("founded on the land (" + hx + ", " + hz + ")", hx >= sea && hx < x0 + 256 && hz >= z0 && hz < z0 + 256);
                    long[] herd = e.herdInfo(agent[0]);
                    record(test, "founded", "herd " + Long.toHexString(agent[0]) + " of " + herd[NativeEngine.HERD_ALIVE]
                            + " at (" + hx + ", " + hz + "), the sea west of x " + sea);
                    int y = top(level, (int) hx, (int) hz) + 1;
                    player[0] = FakePlayers.add(level, new Vec3(hx + 0.5, y, hz + 0.5), "habitat_watcher");
                })
                .thenWaitUntil(RtsimTests.io(() -> check("the herd materialized", HerdBridge.INSTANCE.group(agent[0]) != null
                        && !HerdBridge.INSTANCE.bodies(agent[0]).isEmpty())))
                .thenExecute(() -> {
                    watching[0] = true;
                    record(test, "materialized", HerdBridge.INSTANCE.bodies(agent[0]).size() + " animals, all on land");
                })
                .thenIdle(300)
                .thenExecute(() -> {
                    record(test, "watched", seen[0] + " animal-ticks on land, none in the sea");
                    check("the animals were watched", seen[0] > 0);
                    watching[0] = false;
                    FakePlayers.remove(player[0]);
                    player[0] = null;
                })
                .thenWaitUntil(() -> check("taken back once the player is gone", HerdBridge.INSTANCE.group(agent[0]) == null))
                .thenExecute(() -> forced.forEach(p -> level.setChunkForced(p.x, p.z, false)))
                .thenSucceed();
    }
}
