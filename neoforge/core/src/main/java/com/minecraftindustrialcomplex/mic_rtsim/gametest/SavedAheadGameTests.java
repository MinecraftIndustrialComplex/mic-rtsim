package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.growth.HouseBuilder;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.village.Cultures;
import com.minecraftindustrialcomplex.mic_rtsim.village.Village;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.Tag;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.Container;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.animal.IronGolem;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.ChestBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.ChestType;
import net.minecraft.world.level.dimension.DimensionType;
import net.minecraft.world.level.storage.LevelResource;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.io.IOException;
import java.nio.file.Path;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * Holders saved ahead of the journal (slice U-b, docs/choices-journal-barrier.md RP-210 to RP-219): a chunk
 * or an entity section on disk newer than the journal and the SavedData, emulated on live state. What the
 * crash keeps is left as it is; what it rolls back is undone by hand (blocks, the carried records, the
 * village registry's entries, the journal's applied ops), and the engine's resend is ordered again under the
 * same op id. Each test builds on a pair of far chunks it forces, at its own offset, cleared first: the
 * GameTest world is reused between runs (B37).
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class SavedAheadGameTests {
    private SavedAheadGameTests() {}

    private static long engineOp() {
        return java.util.concurrent.ThreadLocalRandom.current().nextLong() & Long.MAX_VALUE; // an engine op: top bit clear
    }

    private static List<ScriptedEngine.Received> results(ScriptedEngine stub, long op) {
        return RtsimTests.recordsWith(stub, Obs.COMMAND_RESULT, Obs.CommandResult.OP_ID, op);
    }

    /** A failed wait sleeps a little: the GameTest server ticks unthrottled, and chunk loads take wall time. */
    private static Runnable paced(Runnable check) {
        return RtsimTests.io(check);
    }

    private static void fill(ServerLevel level, int x0, int y0, int z0, int x1, int y1, int z1, net.minecraft.world.level.block.state.BlockState st) {
        for (int x = x0; x <= x1; x++)
            for (int y = y0; y <= y1; y++)
                for (int z = z0; z <= z1; z++)
                    level.setBlock(new BlockPos(x, y, z), st, 2 | 16);
    }

    /** Takes this op's carried records off a chunk, as a crash that keeps the chunk from before would. */
    private static void dropCarried(LevelChunk chunk, long op) {
        PendingRecords p = chunk.getExistingDataOrNull(RtsimAttachments.PENDING);
        if (p == null)
            return;
        PendingRecords keep = p.filter(r -> RtsimSession.pendingOp(r) != op);
        if (keep.isEmpty())
            chunk.removeData(RtsimAttachments.PENDING);
        else
            chunk.setData(RtsimAttachments.PENDING, keep);
    }

    /** Whether the entities of {@code c} and of the eight chunks around it are loaded (in the level, not on their way). */
    private static boolean entitiesLoadedAround(ServerLevel level, ChunkPos c) {
        for (int dx = -1; dx <= 1; dx++)
            for (int dz = -1; dz <= 1; dz++)
                if (!level.areEntitiesLoaded(ChunkPos.asLong(c.x + dx, c.z + dz)))
                    return false;
        return true;
    }

    private static List<IronGolem> golems(ServerLevel level, AABB area) {
        return level.getEntitiesOfClass(IronGolem.class, area, e -> !e.isRemoved());
    }

    /** Items of {@code item} lying in item entities in {@code area}. */
    private static int lying(ServerLevel level, AABB area, Item item) {
        int n = 0;
        for (ItemEntity e : level.getEntitiesOfClass(ItemEntity.class, area, e -> !e.isRemoved()))
            if (e.getItem().is(item))
                n += e.getItem().getCount();
        return n;
    }

    private static int count(Container c, Item item) {
        int n = 0;
        for (int i = 0; i < c.getContainerSize(); i++)
            if (c.getItem(i).is(item))
                n += c.getItem(i).getCount();
        return n;
    }

    private static Path regionFolder(ServerLevel level) {
        return DimensionType.getStorageFolder(level.dimension(), level.getServer().getWorldPath(LevelResource.ROOT)).resolve("region");
    }

    /**
     * What the region file holds now for {@code pos}'s chunk: {epoch of {@code district} in its
     * {@code mic_rtsim:reconcile}, 1 if a block entity stands at {@code be}}; null when the chunk is not on disk.
     */
    private static int[] onDisk(ServerLevel level, ChunkPos pos, long district, BlockPos be) {
        CompoundTag chunk;
        try {
            chunk = JournalBarrierGameTests.readRegionChunk(regionFolder(level), pos);
        } catch (IOException e) {
            throw new net.minecraft.gametest.framework.GameTestAssertException("reading " + pos + " from its region file: " + e);
        }
        if (chunk == null)
            return null;
        int epoch = 0;
        ListTag entries = chunk.getCompound("neoforge:attachments").getList("mic_rtsim:reconcile", Tag.TAG_COMPOUND);
        for (int i = 0; i < entries.size(); i++)
            if (entries.getCompound(i).getLong("district") == district)
                epoch = entries.getCompound(i).getInt("epoch");
        int here = 0;
        ListTag bes = chunk.getList("block_entities", Tag.TAG_COMPOUND);
        for (int i = 0; i < bes.size(); i++) {
            CompoundTag t = bes.getCompound(i);
            if (t.getInt("x") == be.getX() && t.getInt("y") == be.getY() && t.getInt("z") == be.getZ())
                here = 1;
        }
        return new int[] {epoch, here};
    }

    // ---- C3: PLACE_HOUSE -------------------------------------------------------------------------

    /**
     * C3 (RP-211): a house across a chunk border, its origin in chunk A, the rest in chunk N. A crash keeps N
     * as written after the house was built, while A, the village registry and the journal are from before.
     * The engine resends the searching PLACE_HOUSE under its op id: the search must know the house from N's
     * carried result and answer DUPLICATE_OP, not refuse the plot N's half fills and build a second house on
     * the next candidate. The half in A is lost (lose, never double).
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 1200, batch = "mic_rtsim_saved_ahead")
    public static void aHouseWhoseOtherChunkIsSavedAheadIsNotBuiltTwice(GameTestHelper helper) {
        String t = "aHouseWhoseOtherChunkIsSavedAheadIsNotBuiltTwice";
        ServerLevel level = helper.getLevel();
        ScriptedEngine stub = RtsimTests.engine();
        String dim = RtsimTests.dimension(helper);
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        int cx = (o.getX() >> 4) + 6810, cz = (o.getZ() >> 4) + 6810;
        ChunkPos a = new ChunkPos(cx, cz), n = new ChunkPos(cx + 1, cz);
        // The hut is 3 x 3 x 3; the search centres it on (x0, z0): x0 - 1 is in A (its origin), x0 and x0 + 1 in N.
        int x0 = n.getMinBlockX(), z0 = a.getMinBlockZ() + 8, y = 180;
        long op = engineOp();
        long seed = 7;
        Village[] v = new Village[1];
        helper.startSequence()
                .thenExecute(() -> {
                    level.setChunkForced(a.x, a.z, true);
                    level.setChunkForced(n.x, n.z, true);
                })
                .thenWaitUntil(paced(() -> check("A and N load", level.getChunkSource().getChunkNow(a.x, a.z) != null
                        && level.getChunkSource().getChunkNow(n.x, n.z) != null)))
                .thenExecute(() -> {
                    // A stone floor under the search's rings (radius 4), air above it.
                    fill(level, x0 - 8, y, z0 - 7, x0 + 7, y + 5, z0 + 7, Blocks.AIR.defaultBlockState());
                    fill(level, x0 - 8, y - 1, z0 - 7, x0 + 7, y - 1, z0 + 7, Blocks.STONE.defaultBlockState());
                    v[0] = RtsimTests.adoptScripted(helper, Cultures.HUMAN, new BoundingBox(x0 - 8, y - 2, z0 - 7, x0 + 7, y + 6, z0 + 7),
                            new BlockPos(x0, y, z0));
                    stub.placeHouse(op, v[0].id, dim, "mic_rtsim:hut", null, x0, y, z0, 0, 0, Cmd.PlaceHouse.FLAG_SEARCH, 4, seed, 0);
                })
                // The house's result is carried the tick it is built, long before the engine can acknowledge it.
                .thenWaitUntil(paced(() -> check("the house is built", RtsimSession.chunkCarriesOp(level.getChunkAt(new BlockPos(x0 - 1, y, z0)), op))))
                .thenExecute(() -> {
                    LevelChunk ca = level.getChunkSource().getChunkNow(a.x, a.z), cn = level.getChunkSource().getChunkNow(n.x, n.z);
                    Village.House h = v[0].house(op);
                    check("the village knows the house", h != null);
                    check("at the first candidate, across the border: " + h.box(), h.box().minX() == x0 - 1 && h.box().maxX() == x0 + 1
                            && h.box().minZ() == z0 - 1 && h.box().maxZ() == z0 + 1);
                    check("its floor in N", level.getBlockState(new BlockPos(x0, y, z0)).is(Blocks.OAK_PLANKS));
                    boolean nCarries = RtsimSession.chunkCarriesOp(cn, op);
                    record(t, "N carries the result", nCarries);
                    // The crash: N stays as it is; A, the registry and the journal go back to before the house.
                    fill(level, x0 - 1, y, z0 - 1, x0 - 1, y + 2, z0 + 1, Blocks.AIR.defaultBlockState());
                    dropCarried(ca, op);
                    v[0].houses.removeIf(x -> x.op() == op);
                    RtsimTests.session().phase1.villages.setDirty();
                    RtsimTests.session().boundary().journal().forgetAppliedForTest(op);
                    // The engine, rolled back too, sends the same PLACE_HOUSE again.
                    stub.placeHouse(op, v[0].id, dim, "mic_rtsim:hut", null, x0, y, z0, 0, 0, Cmd.PlaceHouse.FLAG_SEARCH, 4, seed, 0);
                })
                .thenWaitUntil(paced(() -> check("the resend answered", results(stub, op).size() >= 2)))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> rs = results(stub, op);
                    ScriptedEngine.Received again = rs.get(rs.size() - 1);
                    int houses = 0;
                    for (int x = x0 - 8; x <= x0 + 7; x++)
                        for (int z = z0 - 7; z <= z0 + 7; z++)
                            if (level.getBlockState(new BlockPos(x, y + 1, z)).is(Blocks.COBBLESTONE) && (x < x0 - 1 || x > x0 + 1 || z < z0 - 1 || z > z0 + 1))
                                houses++;
                    record(t, "results, walls outside the first house", rs.stream().map(r -> r.aux()).toList() + ", " + houses);
                    check("no second house: the resend answered " + again.aux() + " with " + houses + " wall blocks elsewhere",
                            again.aux() == Obs.CommandResult.DUPLICATE_OP && houses == 0);
                    check("the registry holds no house for the op", v[0].house(op) == null);
                })
                .thenExecute(() -> {
                    fill(level, x0 - 8, y - 1, z0 - 7, x0 + 7, y + 5, z0 + 7, Blocks.AIR.defaultBlockState());
                    RtsimTests.forget(v[0]);
                    level.setChunkForced(a.x, a.z, false);
                    level.setChunkForced(n.x, n.z, false);
                })
                .thenSucceed();
    }

    /** The search radius of the two far-plot tests: its outer ring reaches a chunk three away, past a forced chunk's FULL reach. */
    private static final int FAR_RING = 44;

    /** Cobblestone (the hut's walls) at {@code y + 1} in the square of half-width 2 around (x, z). */
    private static int walls(ServerLevel level, int x, int y, int z) {
        int n = 0;
        for (int dx = -2; dx <= 2; dx++)
            for (int dz = -2; dz <= 2; dz++)
                if (level.getBlockState(new BlockPos(x + dx, y + 1, z + dz)).is(Blocks.COBBLESTONE))
                    n++;
        return n;
    }

    /** A clear plot for the hut centred on (x, z): air above, a stone floor under it. */
    private static void plot(ServerLevel level, int x, int y, int z) {
        fill(level, x - 2, y, z - 2, x + 2, y + 5, z + 2, Blocks.AIR.defaultBlockState());
        fill(level, x - 2, y - 1, z - 2, x + 2, y - 1, z + 2, Blocks.STONE.defaultBlockState());
    }

    /**
     * C3 (RP-211), the first house's plot on disk only: the hut stands at the search's first candidate, in chunk H,
     * and H is written to disk and unloaded (a forced chunk L three chunks east keeps H's neighbours loaded, not H).
     * The village registry and the journal go back to before the house; the engine resends the searching PLACE_HOUSE.
     * The resend's search finds the first candidate not loaded and goes on to the first loaded free plot, on the outer
     * ring in L. It must find the op in H as saved and answer DUPLICATE_OP, not build a second house in L.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 2400, batch = "mic_rtsim_saved_ahead")
    public static void aHouseWhosePlotIsOnlyOnDiskIsNotBuiltTwice(GameTestHelper helper) {
        String t = "aHouseWhosePlotIsOnlyOnDiskIsNotBuiltTwice";
        ServerLevel level = helper.getLevel();
        ScriptedEngine stub = RtsimTests.engine();
        String dim = RtsimTests.dimension(helper);
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        ChunkPos h = new ChunkPos((o.getX() >> 4) + 6890, (o.getZ() >> 4) + 6810), l = new ChunkPos(h.x + 3, h.z);
        // The first candidate is H's middle; the outer ring's first candidate, FAR_RING east, is in L.
        int x0 = h.getMinBlockX() + 8, z0 = h.getMinBlockZ() + 8, y = 180, xl = x0 + FAR_RING;
        long op = engineOp();
        long seed = 7; // the rings start at an angle of about 0: the first candidate of each ring is east of the centre
        Village[] v = new Village[1];
        boolean[] written = new boolean[1];
        helper.startSequence()
                .thenExecute(() -> {
                    level.setChunkForced(h.x, h.z, true);
                    level.setChunkForced(l.x, l.z, true);
                })
                .thenWaitUntil(paced(() -> check("H and L load", level.getChunkSource().getChunkNow(h.x, h.z) != null
                        && level.getChunkSource().getChunkNow(l.x, l.z) != null)))
                .thenExecute(() -> {
                    plot(level, x0, y, z0);
                    plot(level, xl, y, z0);
                    v[0] = RtsimTests.adoptScripted(helper, Cultures.HUMAN,
                            new BoundingBox(x0 - FAR_RING - 4, y - 2, z0 - FAR_RING - 4, x0 + FAR_RING + 4, y + 6, z0 + FAR_RING + 4), new BlockPos(x0, y, z0));
                    stub.placeHouse(op, v[0].id, dim, "mic_rtsim:hut", null, x0, y, z0, 0, 0, Cmd.PlaceHouse.FLAG_SEARCH, FAR_RING, seed, 0);
                })
                .thenWaitUntil(paced(() -> check("the house is built in H", RtsimSession.chunkCarriesOp(level.getChunk(h.x, h.z), op))))
                .thenExecute(() -> {
                    Village.House house = v[0].house(op);
                    check("at the first candidate: " + (house == null ? null : house.box()), house != null && house.box().minX() == x0 - 1);
                    // The crash: H as it is now, on disk; the registry and the journal from before the house.
                    v[0].houses.removeIf(x -> x.op() == op);
                    RtsimTests.session().phase1.villages.setDirty();
                    RtsimTests.session().boundary().journal().forgetAppliedForTest(op);
                    RtsimTests.session().journal().chunks().writeNow(level, List.of(level.getChunk(h.x, h.z))).thenAccept(ok -> written[0] = ok);
                })
                .thenWaitUntil(paced(() -> check("H is on disk", written[0])))
                .thenExecute(() -> level.setChunkForced(h.x, h.z, false))
                .thenWaitUntil(paced(() -> check("H is no longer loaded", level.getChunkSource().getChunkNow(h.x, h.z) == null)))
                // The engine, rolled back too, sends the same PLACE_HOUSE again.
                .thenExecute(() -> stub.placeHouse(op, v[0].id, dim, "mic_rtsim:hut", null, x0, y, z0, 0, 0, Cmd.PlaceHouse.FLAG_SEARCH, FAR_RING, seed, 0))
                .thenWaitUntil(paced(() -> check("the resend answered", results(stub, op).size() >= 2)))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> rs = results(stub, op);
                    int again = rs.get(rs.size() - 1).aux(), inL = walls(level, xl, y, z0);
                    record(t, "results, wall blocks in L", rs.stream().map(r -> r.aux()).toList() + ", " + inL);
                    check("no second house: the resend answered " + again + " with " + inL + " wall blocks in L",
                            again == Obs.CommandResult.DUPLICATE_OP && inL == 0);
                })
                .thenExecute(() -> level.setChunkForced(h.x, h.z, true))
                .thenWaitUntil(paced(() -> check("H loads for the clean-up", level.getChunkSource().getChunkNow(h.x, h.z) != null)))
                .thenExecute(() -> {
                    fill(level, x0 - 2, y - 1, z0 - 2, x0 + 2, y + 5, z0 + 2, Blocks.AIR.defaultBlockState());
                    fill(level, xl - 2, y - 1, z0 - 2, xl + 2, y + 5, z0 + 2, Blocks.AIR.defaultBlockState());
                    dropCarried(level.getChunk(h.x, h.z), op);
                    dropCarried(level.getChunk(l.x, l.z), op);
                    RtsimTests.forget(v[0]);
                    level.setChunkForced(h.x, h.z, false);
                    level.setChunkForced(l.x, l.z, false);
                })
                .thenSucceed();
    }

    /**
     * C3 (RP-211), an earlier plot free at the resend: the search's first candidate, in chunk E, is not loaded when
     * the house is built (E unforced, three chunks from the forced L), so the hut goes on the outer ring in L. The
     * crash keeps L with the house; the registry and the journal go back to before it. E is loaded again and its plot
     * is free when the engine resends the searching PLACE_HOUSE: the resend must find the op in L and answer
     * DUPLICATE_OP, not build a second house at the first candidate in E.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 2400, batch = "mic_rtsim_saved_ahead")
    public static void aHouseIsNotBuiltTwiceOnAnEarlierPlotFreeAtTheResend(GameTestHelper helper) {
        String t = "aHouseIsNotBuiltTwiceOnAnEarlierPlotFreeAtTheResend";
        ServerLevel level = helper.getLevel();
        ScriptedEngine stub = RtsimTests.engine();
        String dim = RtsimTests.dimension(helper);
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        ChunkPos e = new ChunkPos((o.getX() >> 4) + 6910, (o.getZ() >> 4) + 6810), l = new ChunkPos(e.x + 3, e.z);
        int x0 = e.getMinBlockX() + 8, z0 = e.getMinBlockZ() + 8, y = 180, xl = x0 + FAR_RING;
        long op = engineOp();
        long seed = 7;
        Village[] v = new Village[1];
        helper.startSequence()
                .thenExecute(() -> {
                    level.setChunkForced(e.x, e.z, true);
                    level.setChunkForced(l.x, l.z, true);
                })
                .thenWaitUntil(paced(() -> check("E and L load", level.getChunkSource().getChunkNow(e.x, e.z) != null
                        && level.getChunkSource().getChunkNow(l.x, l.z) != null)))
                .thenExecute(() -> {
                    plot(level, x0, y, z0);
                    plot(level, xl, y, z0);
                    level.setChunkForced(e.x, e.z, false);
                })
                .thenWaitUntil(paced(() -> check("E is no longer loaded", level.getChunkSource().getChunkNow(e.x, e.z) == null)))
                .thenExecute(() -> {
                    v[0] = RtsimTests.adoptScripted(helper, Cultures.HUMAN,
                            new BoundingBox(x0 - FAR_RING - 4, y - 2, z0 - FAR_RING - 4, x0 + FAR_RING + 4, y + 6, z0 + FAR_RING + 4), new BlockPos(x0, y, z0));
                    stub.placeHouse(op, v[0].id, dim, "mic_rtsim:hut", null, x0, y, z0, 0, 0, Cmd.PlaceHouse.FLAG_SEARCH, FAR_RING, seed, 0);
                })
                .thenWaitUntil(paced(() -> check("the house is built in L", RtsimSession.chunkCarriesOp(level.getChunk(l.x, l.z), op))))
                .thenExecute(() -> {
                    Village.House house = v[0].house(op);
                    check("on the outer ring: " + (house == null ? null : house.box()), house != null && house.box().minX() == xl - 1);
                    // The crash: L as it is now; the registry and the journal from before the house.
                    v[0].houses.removeIf(x -> x.op() == op);
                    RtsimTests.session().phase1.villages.setDirty();
                    RtsimTests.session().boundary().journal().forgetAppliedForTest(op);
                    level.setChunkForced(e.x, e.z, true);
                })
                .thenWaitUntil(paced(() -> check("E loads", level.getChunkSource().getChunkNow(e.x, e.z) != null)))
                .thenExecute(() -> {
                    check("E's plot is free", HouseBuilder.free(level, v[0], new BoundingBox(x0 - 1, y, z0 - 1, x0 + 1, y + 2, z0 + 1)));
                    // The engine, rolled back too, sends the same PLACE_HOUSE again.
                    stub.placeHouse(op, v[0].id, dim, "mic_rtsim:hut", null, x0, y, z0, 0, 0, Cmd.PlaceHouse.FLAG_SEARCH, FAR_RING, seed, 0);
                })
                .thenWaitUntil(paced(() -> check("the resend answered", results(stub, op).size() >= 2)))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> rs = results(stub, op);
                    int again = rs.get(rs.size() - 1).aux(), inE = walls(level, x0, y, z0);
                    record(t, "results, wall blocks in E", rs.stream().map(r -> r.aux()).toList() + ", " + inE);
                    check("no second house: the resend answered " + again + " with " + inE + " wall blocks in E",
                            again == Obs.CommandResult.DUPLICATE_OP && inE == 0);
                })
                .thenExecute(() -> {
                    fill(level, x0 - 2, y - 1, z0 - 2, x0 + 2, y + 5, z0 + 2, Blocks.AIR.defaultBlockState());
                    fill(level, xl - 2, y - 1, z0 - 2, xl + 2, y + 5, z0 + 2, Blocks.AIR.defaultBlockState());
                    dropCarried(level.getChunk(e.x, e.z), op);
                    dropCarried(level.getChunk(l.x, l.z), op);
                    RtsimTests.forget(v[0]);
                    level.setChunkForced(e.x, e.z, false);
                    level.setChunkForced(l.x, l.z, false);
                })
                .thenSucceed();
    }

    // ---- C4: SPAWN_RESIDENT ----------------------------------------------------------------------

    /**
     * C4 (RP-212): a resident whose entity chunk is on disk ahead of the spawn chunk and the journal. The crash
     * keeps the resident with the COMMAND_RESULT it carries; the spawn chunk and the journal are from before the
     * spawn. It goes through a real unload and load: the spawn chunk's record and the journal's applied op are
     * taken away, the chunk unloads (its entities stored with the resident), the engine resends SPAWN_RESIDENT
     * under the same op id while the chunk is away, and the chunk loads again. The resend must spawn nobody:
     * one golem, not two.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 4000, batch = "mic_rtsim_saved_ahead")
    public static void aResidentWhoseEntityChunkIsSavedAheadIsNotSpawnedTwice(GameTestHelper helper) {
        String t = "aResidentWhoseEntityChunkIsSavedAheadIsNotSpawnedTwice";
        ServerLevel level = helper.getLevel();
        ScriptedEngine stub = RtsimTests.engine();
        String dim = RtsimTests.dimension(helper);
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        ChunkPos s = new ChunkPos((o.getX() >> 4) + 6830, (o.getZ() >> 4) + 6810);
        int y = 180;
        // The meeting point in the middle of S: the golem stands within 5 blocks of it, so in S.
        BlockPos meet = new BlockPos(s.getMinBlockX() + 8, y, s.getMinBlockZ() + 8);
        AABB area = new AABB(s.getMinBlockX() - 16, y - 4, s.getMinBlockZ() - 16, s.getMaxBlockX() + 17, y + 8, s.getMaxBlockZ() + 17);
        long op = engineOp();
        Village[] v = new Village[1];
        PendingRecords[] carried = new PendingRecords[1];
        IronGolem[] first = new IronGolem[1];
        helper.startSequence()
                .thenExecute(() -> level.setChunkForced(s.x, s.z, true))
                .thenWaitUntil(paced(() -> check("S and the entities around it load", level.getChunkSource().getChunkNow(s.x, s.z) != null
                        && entitiesLoadedAround(level, s))))
                .thenExecute(() -> {
                    golems(level, area).forEach(Entity::discard); // a failed run's, in the reused world
                    fill(level, s.getMinBlockX(), y, s.getMinBlockZ(), s.getMaxBlockX(), y + 4, s.getMaxBlockZ(), Blocks.AIR.defaultBlockState());
                    fill(level, s.getMinBlockX(), y - 1, s.getMinBlockZ(), s.getMaxBlockX(), y - 1, s.getMaxBlockZ(), Blocks.STONE.defaultBlockState());
                    v[0] = RtsimTests.adoptScripted(helper, Cultures.HUMAN,
                            new BoundingBox(s.getMinBlockX(), y - 2, s.getMinBlockZ(), s.getMaxBlockX(), y + 6, s.getMaxBlockZ()), meet);
                    stub.spawnResident(op, v[0].id, dim, "minecraft:iron_golem", null, Cmd.SpawnResident.FLAG_AT_VILLAGE, 0, 0, 0, 0);
                })
                .thenWaitUntil(paced(() -> check("the golem spawned", !results(stub, op).isEmpty())))
                .thenExecute(() -> {
                    check("APPLIED", results(stub, op).get(0).aux() == Obs.CommandResult.APPLIED);
                    List<IronGolem> gs = golems(level, area);
                    check("one golem: " + gs.size(), gs.size() == 1);
                    first[0] = gs.get(0);
                    check("standing in S", first[0].chunkPosition().equals(s));
                    carried[0] = first[0].getExistingDataOrNull(RtsimAttachments.PENDING);
                    check("it carries its COMMAND_RESULT", carried[0] != null
                            && carried[0].records().stream().anyMatch(r -> RtsimSession.pendingOp(r) == op));
                    // The crash: the resident's entity chunk as stored now (its record on it, whatever the engine
                    // has acknowledged since); the spawn chunk and the journal from before the spawn.
                    first[0].setData(RtsimAttachments.PENDING, carried[0]);
                    dropCarried(level.getChunk(s.x, s.z), op);
                    RtsimTests.session().boundary().journal().forgetAppliedForTest(op);
                    level.setChunkForced(s.x, s.z, false);
                })
                .thenWaitUntil(paced(() -> check("S unloads, its golem stored with it", first[0].isRemoved()
                        && level.getChunkSource().getChunkNow(s.x, s.z) == null)))
                .thenExecute(() -> {
                    record(t, "the golem left with", first[0].getRemovalReason());
                    // The engine, rolled back with the journal, sends the same SPAWN_RESIDENT again.
                    stub.spawnResident(op, v[0].id, dim, "minecraft:iron_golem", null, Cmd.SpawnResident.FLAG_AT_VILLAGE, 0, 0, 0, 0);
                })
                .thenWaitUntil(paced(() -> check("the resend waits for S", RtsimTests.session().phase1.villages.residentOrders.containsKey(op))))
                .thenExecute(() -> level.setChunkForced(s.x, s.z, true))
                .thenWaitUntil(paced(() -> check("the resend answered", results(stub, op).size() >= 2)))
                .thenWaitUntil(paced(() -> check("S's entities and its neighbours' are loaded", entitiesLoadedAround(level, s))))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> rs = results(stub, op);
                    int again = rs.get(rs.size() - 1).aux();
                    List<IronGolem> gs = golems(level, area);
                    record(t, "results, golems, golems reasserted", rs.stream().map(r -> r.aux()).toList() + ", " + gs.size() + ", "
                            + gs.stream().filter(g -> RtsimTests.session().reasserted(g)).count());
                    check("no second resident: the resend answered " + again + " and " + gs.size() + " golems stand in S",
                            again == Obs.CommandResult.DUPLICATE_OP && gs.size() == 1);
                })
                .thenExecute(() -> {
                    golems(level, area).forEach(Entity::discard);
                    fill(level, s.getMinBlockX(), y - 1, s.getMinBlockZ(), s.getMaxBlockX(), y + 4, s.getMaxBlockZ(), Blocks.AIR.defaultBlockState());
                    RtsimTests.forget(v[0]);
                    level.setChunkForced(s.x, s.z, false);
                })
                .thenSucceed();
    }

    // ---- I2: reconcile breaks ----------------------------------------------------------------------

    private static final String GOOD_DIAMOND = "i/64/minecraft:diamond";

    /**
     * I2 (RP-213): a reconcile breaks a chest, whose contents and the chest itself drop as item entities, stored
     * in their own entity chunk. The crash keeps what is on disk at the first tick the drops are in the world:
     * if the chunk on disk still holds the chest, it goes back to that (the chest, its contents, no epoch), the
     * journal goes back to before the reconcile, the drops stay, and the engine resends the reconcile under the
     * same op id, which breaks the chest again. The drops must reach the world only once the chunk on disk has
     * the chest gone and the epoch applied: then the resend is a duplicate, and the drops are there once.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 2400, batch = "mic_rtsim_saved_ahead")
    public static void aReconcileThatBreaksAChestDropsItsContentsOnce(GameTestHelper helper) {
        String t = "aReconcileThatBreaksAChestDropsItsContentsOnce";
        ServerLevel level = helper.getLevel();
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        ChunkPos c = new ChunkPos((o.getX() >> 4) + 6850, (o.getZ() >> 4) + 6810);
        int y = 180;
        BlockPos chest = new BlockPos(c.getMinBlockX() + 8, y, c.getMinBlockZ() + 8);
        AABB around = new AABB(chest).inflate(6);
        BlockState chestState = Blocks.CHEST.defaultBlockState();
        String expected = BlockStateParser.serialize(chestState);
        long op = engineOp();
        long district = engineOp(); // no district: a reconcile's breaks need none
        int epoch = 1;
        IndustryAttachments.Epochs[] before = new IndustryAttachments.Epochs[1];
        int[][] disk = new int[1][];
        Runnable placeChest = () -> {
            level.setBlock(chest, chestState, 2 | 16);
            Container box = (Container) level.getBlockEntity(chest);
            box.setItem(0, new ItemStack(Items.DIAMOND, 64));
            box.setItem(1, new ItemStack(Items.DIAMOND, 36));
        };
        Runnable reconcile = () -> IndustryTestKit.reconcile(op, district, epoch, level, c.x, c.z, List.of(),
                List.of(new IndustryTestKit.Destroy(chest, expected)));
        helper.startSequence()
                .thenExecute(() -> level.setChunkForced(c.x, c.z, true))
                .thenWaitUntil(paced(() -> check("C and the entities around it load", level.getChunkSource().getChunkNow(c.x, c.z) != null
                        && entitiesLoadedAround(level, c))))
                .thenExecute(() -> {
                    fill(level, chest.getX() - 4, y, chest.getZ() - 4, chest.getX() + 4, y + 3, chest.getZ() + 4, Blocks.AIR.defaultBlockState());
                    fill(level, chest.getX() - 4, y - 1, chest.getZ() - 4, chest.getX() + 4, y - 1, chest.getZ() + 4, Blocks.STONE.defaultBlockState());
                    level.getEntitiesOfClass(ItemEntity.class, around).forEach(Entity::discard); // a failed run's, and what its chest spilled just now
                    placeChest.run();
                    before[0] = level.getChunk(c.x, c.z).getData(IndustryAttachments.RECONCILE.get());
                    reconcile.run();
                })
                // The first tick the drops are in the world, the crash keeps what the region file holds then.
                .thenWaitUntil(paced(() -> {
                    check("the drops are in the world", lying(level, around, Items.DIAMOND) > 0);
                    disk[0] = onDisk(level, c, district, chest);
                }))
                .thenExecute(() -> {
                    check("the reconcile applied", IndustryTestKit.commandResults(op).equals(List.of(Obs.CommandResult.APPLIED)));
                    check("the chest is gone", level.getBlockState(chest).isAir());
                    boolean ahead = disk[0] == null || disk[0][0] < epoch;
                    record(t, "on disk when the drops came (epoch, chest)", disk[0] == null ? "not on disk" : disk[0][0] + ", " + disk[0][1]);
                    if (ahead) {
                        // The chunk on disk is from before the reconcile: the crash brings back the chest and its contents, without the epoch.
                        placeChest.run();
                        level.getChunk(c.x, c.z).setData(IndustryAttachments.RECONCILE.get(), before[0]);
                    }
                    // The journal is from before (B), and the engine resends the reconcile.
                    RtsimTests.session().boundary().journal().forgetAppliedForTest(op);
                    reconcile.run();
                })
                .thenWaitUntil(paced(() -> check("the resend answered", IndustryTestKit.commandResults(op).size() >= 2)))
                .thenIdle(40) // what a second break would drop has had time to come
                .thenExecute(() -> {
                    int diamonds = lying(level, around, Items.DIAMOND), chests = lying(level, around, Items.CHEST);
                    record(t, "results, diamonds and chests lying", IndustryTestKit.commandResults(op) + ", " + diamonds + ", " + chests);
                    check("the chest's contents dropped once: " + diamonds + " diamonds and " + chests + " chests lie there",
                            diamonds == 100 && chests == 1);
                    check("the resend was a duplicate", IndustryTestKit.commandResults(op).get(1) == Obs.CommandResult.DUPLICATE_OP);
                    check("the drops are marked as ported", level.getEntitiesOfClass(ItemEntity.class, around).stream()
                            .allMatch(i -> i.hasData(IndustryAttachments.PORTED.get()) && i.getData(IndustryAttachments.PORTED.get())));
                })
                .thenExecute(() -> {
                    level.getEntitiesOfClass(ItemEntity.class, around).forEach(Entity::discard);
                    fill(level, chest.getX() - 4, y - 1, chest.getZ() - 4, chest.getX() + 4, y + 3, chest.getZ() + 4, Blocks.AIR.defaultBlockState());
                    level.setChunkForced(c.x, c.z, false);
                })
                .thenSucceed();
    }

    /**
     * I2 (RP-213), drops held while their chunk would unload: a reconcile breaks a chest in chunk C, and its drops
     * are held back (here by the test, as a slow write would hold them) while C's force ticket is taken away. C must
     * stay loaded, its entities with it, until the drops have joined the level, or they wait for C to load again,
     * all session, and a stop loses them. The hold covers C only.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 2400, batch = "mic_rtsim_saved_ahead")
    public static void aReconcilesHeldDropsKeepTheirChunkLoadedUntilTheyJoin(GameTestHelper helper) {
        String t = "aReconcilesHeldDropsKeepTheirChunkLoadedUntilTheyJoin";
        ServerLevel level = helper.getLevel();
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        ChunkPos c = new ChunkPos((o.getX() >> 4) + 6930, (o.getZ() >> 4) + 6810);
        int y = 180;
        BlockPos chest = new BlockPos(c.getMinBlockX() + 8, y, c.getMinBlockZ() + 8);
        AABB around = new AABB(chest).inflate(6);
        BlockState chestState = Blocks.CHEST.defaultBlockState();
        long op = engineOp();
        long district = engineOp();
        long[] since = new long[1];
        helper.startSequence()
                .thenExecute(() -> level.setChunkForced(c.x, c.z, true))
                .thenWaitUntil(paced(() -> check("C and the entities around it load", level.getChunkSource().getChunkNow(c.x, c.z) != null
                        && entitiesLoadedAround(level, c))))
                .thenExecute(() -> {
                    fill(level, chest.getX() - 4, y, chest.getZ() - 4, chest.getX() + 4, y + 3, chest.getZ() + 4, Blocks.AIR.defaultBlockState());
                    fill(level, chest.getX() - 4, y - 1, chest.getZ() - 4, chest.getX() + 4, y - 1, chest.getZ() + 4, Blocks.STONE.defaultBlockState());
                    level.getEntitiesOfClass(ItemEntity.class, around).forEach(Entity::discard); // a failed run's
                    level.setBlock(chest, chestState, 2 | 16);
                    ((Container) level.getBlockEntity(chest)).setItem(0, new ItemStack(Items.DIAMOND, 64));
                    com.minecraftindustrialcomplex.mic_rtsim.industry.Reconciler.holdDropsForTest(c);
                    IndustryTestKit.reconcile(op, district, 1, level, c.x, c.z, List.of(),
                            List.of(new IndustryTestKit.Destroy(chest, BlockStateParser.serialize(chestState))));
                })
                .thenWaitUntil(paced(() -> check("the reconcile applied", !IndustryTestKit.commandResults(op).isEmpty())))
                .thenExecute(() -> {
                    check("APPLIED", IndustryTestKit.commandResults(op).get(0) == Obs.CommandResult.APPLIED);
                    check("the chest is gone and its drops are held", level.getBlockState(chest).isAir()
                            && lying(level, around, Items.DIAMOND) == 0);
                    level.setChunkForced(c.x, c.z, false);
                    since[0] = System.nanoTime();
                })
                // Long enough for an unforced far chunk to unload (the C4 test sees it in well under this).
                .thenWaitUntil(paced(() -> check("C unloads, or 5 s pass", level.getChunkSource().getChunkNow(c.x, c.z) == null
                        || System.nanoTime() - since[0] > 5_000_000_000L)))
                .thenExecute(() -> {
                    boolean loaded = level.getChunkSource().getChunkNow(c.x, c.z) != null && level.areEntitiesLoaded(c.toLong());
                    level.setChunkForced(c.x, c.z, true); // so the joined drops stay to be counted
                    com.minecraftindustrialcomplex.mic_rtsim.industry.Reconciler.holdDropsForTest(null);
                    record(t, "C and its entities loaded while the drops are held", loaded);
                    check("C stays loaded, its entities with it, while its drops are held", loaded);
                })
                .thenWaitUntil(paced(() -> check("the drops joined", lying(level, around, Items.DIAMOND) == 64 && lying(level, around, Items.CHEST) == 1)))
                .thenExecute(() -> {
                    level.getEntitiesOfClass(ItemEntity.class, around).forEach(Entity::discard);
                    fill(level, chest.getX() - 4, y - 1, chest.getZ() - 4, chest.getX() + 4, y + 3, chest.getZ() + 4, Blocks.AIR.defaultBlockState());
                    level.setChunkForced(c.x, c.z, false);
                })
                .thenSucceed();
    }

    /**
     * The audit's uncertainty 6.5 (RP-214): a drop marked as already ported (the contents of a stock block a
     * player or a reconcile broke) must still be marked after its entity chunk is saved and loaded, or a player
     * picking it up then is booked as a second outflow (PLAYER_PICKUP), which the engine's learner reads as
     * production. Saved and loaded the way vanilla stores an entity; an unmarked drop saves no mark at all.
     */
    @GameTest(template = RtsimTests.TEMPLATE, batch = "mic_rtsim_saved_ahead")
    public static void aPortedDropKeepsItsMarkThroughASave(GameTestHelper helper) {
        String t = "aPortedDropKeepsItsMarkThroughASave";
        ServerLevel level = helper.getLevel();
        BlockPos at = helper.absolutePos(new BlockPos(1, 2, 1));
        ItemEntity ported = new ItemEntity(level, at.getX() + 0.5, at.getY(), at.getZ() + 0.5, new ItemStack(Items.DIAMOND, 5));
        ported.setData(IndustryAttachments.PORTED.get(), Boolean.TRUE);
        CompoundTag saved = new CompoundTag();
        check("the drop saves", ported.save(saved));
        Entity back = net.minecraft.world.entity.EntityType.create(saved, level).orElse(null);
        boolean still = back != null && back.hasData(IndustryAttachments.PORTED.get()) && back.getData(IndustryAttachments.PORTED.get());
        record(t, "loaded back as ported", still);
        check("the drop loads back as ported", still);
        ItemEntity plain = new ItemEntity(level, at.getX() + 0.5, at.getY(), at.getZ() + 0.5, new ItemStack(Items.DIAMOND, 5));
        CompoundTag p = new CompoundTag();
        check("the plain drop saves", plain.save(p));
        check("an unmarked drop saves no mark", !p.getCompound("neoforge:attachments").contains("mic_rtsim:ported"));
        helper.succeed();
    }

    // ---- I1: a double chest across a chunk border --------------------------------------------------

    /**
     * I1, verifying only (RP-215; the audit's uncertainty 6.3): a double chest across a chunk border, its first
     * half P in chunk A full of stone, the other half Q in chunk N empty. A reconcile of A puts diamonds into P's
     * position; NeoForge's chest item handler is the pair's, so they go into Q, in N, which no command names and
     * no note covers. The crash keeps N (direction B: Q's chunk on disk ahead), while A and the journal are from
     * before, so the engine resends the reconcile under the same op id. What Q holds then is recorded: twice the
     * delta is a duplicate. The test checks the facts the outcome rests on, not the outcome.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 1200, batch = "mic_rtsim_saved_ahead")
    public static void aReconcileThroughADoubleChestAcrossAChunkBorder(GameTestHelper helper) {
        String t = "aReconcileThroughADoubleChestAcrossAChunkBorder";
        ServerLevel level = helper.getLevel();
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        ChunkPos a = new ChunkPos((o.getX() >> 4) + 6870, (o.getZ() >> 4) + 6810), n = new ChunkPos(a.x + 1, a.z);
        int y = 180;
        // Facing north, a LEFT half's other half is to its east: P at A's east edge, Q at N's west edge.
        BlockPos p = new BlockPos(a.getMaxBlockX(), y, a.getMinBlockZ() + 8), q = p.east();
        long op = engineOp();
        long district = engineOp();
        int epoch = 1, delta = 20;
        IndustryAttachments.Epochs[] before = new IndustryAttachments.Epochs[1];
        Runnable reconcile = () -> IndustryTestKit.reconcile(op, district, epoch, level, a.x, a.z,
                List.of(new IndustryTestKit.Delta(0, GOOD_DIAMOND, p, delta)));
        helper.startSequence()
                .thenExecute(() -> {
                    level.setChunkForced(a.x, a.z, true);
                    level.setChunkForced(n.x, n.z, true);
                })
                .thenWaitUntil(paced(() -> check("A and N load", level.getChunkSource().getChunkNow(a.x, a.z) != null
                        && level.getChunkSource().getChunkNow(n.x, n.z) != null)))
                .thenExecute(() -> {
                    fill(level, p.getX() - 2, y, p.getZ() - 2, q.getX() + 2, y + 2, q.getZ() + 2, Blocks.AIR.defaultBlockState());
                    level.getEntitiesOfClass(ItemEntity.class, new AABB(p).inflate(6)).forEach(Entity::discard); // what a failed run's chest spilled
                    BlockState left = Blocks.CHEST.defaultBlockState().setValue(ChestBlock.FACING, Direction.NORTH).setValue(ChestBlock.TYPE, ChestType.LEFT);
                    level.setBlock(p, left, 2 | 16);
                    level.setBlock(q, left.setValue(ChestBlock.TYPE, ChestType.RIGHT), 2 | 16);
                    Container pc = (Container) level.getBlockEntity(p);
                    for (int i = 0; i < pc.getContainerSize(); i++)
                        pc.setItem(i, new ItemStack(Items.STONE, 64));
                    before[0] = level.getChunk(a.x, a.z).getData(IndustryAttachments.RECONCILE.get());
                    reconcile.run();
                })
                .thenWaitUntil(paced(() -> check("the reconcile answered", !IndustryTestKit.commandResults(op).isEmpty())))
                .thenExecute(() -> {
                    Container pc = (Container) level.getBlockEntity(p), qc = (Container) level.getBlockEntity(q);
                    int inP = count(pc, Items.DIAMOND), inQ = count(qc, Items.DIAMOND);
                    record(t, "after the reconcile: result, diamonds in P, in Q", IndustryTestKit.commandResults(op) + ", " + inP + ", " + inQ);
                    check("applied", IndustryTestKit.commandResults(op).get(0) == Obs.CommandResult.APPLIED);
                    check("the pair's handler put the diamonds into Q, in N: P " + inP + ", Q " + inQ, inP == 0 && inQ == delta);
                    IndustryAttachments.Epochs inN = level.getChunk(n.x, n.z).getExistingDataOrNull(IndustryAttachments.RECONCILE.get());
                    check("N carries no epoch of the reconcile", inN == null || inN.epoch(district) == 0);
                    // The crash keeps N; A (no epoch, P as it was: full of stone) and the journal are from before.
                    level.getChunk(a.x, a.z).setData(IndustryAttachments.RECONCILE.get(), before[0]);
                    RtsimTests.session().boundary().journal().forgetAppliedForTest(op);
                    reconcile.run();
                })
                .thenWaitUntil(paced(() -> check("the resend answered", IndustryTestKit.commandResults(op).size() >= 2)))
                .thenExecute(() -> {
                    Container qc = (Container) level.getBlockEntity(q);
                    int inQ = count(qc, Items.DIAMOND);
                    record(t, "after the resend: results, diamonds in Q (the delta was " + delta + ")", IndustryTestKit.commandResults(op) + ", " + inQ);
                    record(t, "direction B, a positive delta through the other half", inQ > delta ? "DUPE" : inQ == delta ? "EXACT" : "LOSS");
                })
                .thenExecute(() -> {
                    ((Container) level.getBlockEntity(p)).clearContent();
                    ((Container) level.getBlockEntity(q)).clearContent();
                    fill(level, p.getX() - 2, y, p.getZ() - 2, q.getX() + 2, y + 2, q.getZ() + 2, Blocks.AIR.defaultBlockState());
                    level.setChunkForced(a.x, a.z, false);
                    level.setChunkForced(n.x, n.z, false);
                })
                .thenSucceed();
    }
}
