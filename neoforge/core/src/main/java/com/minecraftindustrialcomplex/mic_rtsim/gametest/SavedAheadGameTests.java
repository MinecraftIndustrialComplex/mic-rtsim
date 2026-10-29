package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.growth.HouseBuilder;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Districts;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
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

    // ---- I1 (RP-215): a vanilla double chest across a chunk border is two inventories ---------------

    /**
     * A vanilla double chest across a chunk border is two inventories, each half its own 27 slots: NeoForge's chest
     * item handler is the pair's and reads the other half through {@code Level.getBlockState}, so a command at one half
     * would change the other, in a chunk the command does not name and the op does not note (the audit's uncertainty
     * 6.3). These tests run the sim's writes through such a pair, P the LEFT half at the east edge of chunk A, Q the
     * RIGHT half at the west edge of N, facing north (a LEFT half's other half is to its east). A pair inside one chunk
     * stays one inventory: one chunk's write covers it ({@code IndustryGameTests.aDoubleChestIsOneInventory}).
     */
    private record Split(ChunkPos a, ChunkPos n, BlockPos p, BlockPos q, int y) {}

    private static Split split(GameTestHelper helper, int xOffset) {
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        ChunkPos a = new ChunkPos((o.getX() >> 4) + xOffset, (o.getZ() >> 4) + 6810), n = new ChunkPos(a.x + 1, a.z);
        int y = 180;
        BlockPos p = new BlockPos(a.getMaxBlockX(), y, a.getMinBlockZ() + 8);
        return new Split(a, n, p, p.east(), y);
    }

    private static void force(ServerLevel level, Split s, boolean on) {
        level.setChunkForced(s.a().x, s.a().z, on);
        level.setChunkForced(s.n().x, s.n().z, on);
    }

    private static boolean loaded(ServerLevel level, ChunkPos c) {
        return level.getChunkSource().getChunkNow(c.x, c.z) != null;
    }

    /**
     * A double chest across a chunk border, cleared first (the air around, what a failed run's chest spilled): P of type
     * {@code pType} at {@code p}, its other half Q at {@code q} (east of P), facing {@code facing}. Facing north, a LEFT
     * half's other half is to its east; facing south, a RIGHT half's is. A RIGHT half is the pair's first (its slots
     * 0 to 26, filled first), a LEFT half its second (slots 27 to 53, emptied first), as a pair's handler counts them.
     */
    private static void doublePair(ServerLevel level, BlockPos p, BlockPos q, int y, Direction facing, ChestType pType) {
        fill(level, p.getX() - 2, y, p.getZ() - 2, q.getX() + 2, y + 2, q.getZ() + 2, Blocks.AIR.defaultBlockState());
        level.getEntitiesOfClass(ItemEntity.class, new AABB(p).inflate(6)).forEach(Entity::discard);
        BlockState half = Blocks.CHEST.defaultBlockState().setValue(ChestBlock.FACING, facing);
        level.setBlock(p, half.setValue(ChestBlock.TYPE, pType), 2 | 16);
        level.setBlock(q, half.setValue(ChestBlock.TYPE, pType == ChestType.LEFT ? ChestType.RIGHT : ChestType.LEFT), 2 | 16);
    }

    /** Facing north, P the LEFT half in the west chunk, Q the RIGHT half east of it. */
    private static void doublePair(ServerLevel level, Split s) {
        doublePair(level, s.p(), s.q(), s.y(), Direction.NORTH, ChestType.LEFT);
    }

    private static void emptyPair(ServerLevel level, Split s) {
        if (level.getBlockEntity(s.p()) instanceof Container c)
            c.clearContent();
        if (level.getBlockEntity(s.q()) instanceof Container c)
            c.clearContent();
        fill(level, s.p().getX() - 2, s.y(), s.p().getZ() - 2, s.q().getX() + 2, s.y() + 2, s.q().getZ() + 2, Blocks.AIR.defaultBlockState());
    }

    private static Container half(ServerLevel level, BlockPos pos) {
        return (Container) level.getBlockEntity(pos);
    }

    private static void stone(Container c, int from) {
        for (int i = from; i < c.getContainerSize(); i++)
            c.setItem(i, new ItemStack(Items.STONE, 64));
    }

    /** Whether {@code c} holds an applied epoch of {@code district} of at least {@code epoch}. */
    private static boolean epochIn(ServerLevel level, ChunkPos c, long district, int epoch) {
        IndustryAttachments.Epochs e = level.getChunk(c.x, c.z).getExistingDataOrNull(IndustryAttachments.RECONCILE.get());
        return e != null && e.epoch(district) >= epoch;
    }

    /** The result record's {@code requested} and {@code applied} of delta {@code j} of the reconcile of {@code op}. */
    private static long[] resultOf(long op, int j) {
        List<java.nio.ByteBuffer> rs = IndustryTestKit.received(IndRecords.IND_RECONCILE_RESULT, IndRecords.ReconcileResult.OP_ID, op);
        check("a result record for the reconcile", !rs.isEmpty());
        java.nio.ByteBuffer r = rs.get(rs.size() - 1);
        int e = IndRecords.ReconcileResult.ENTRIES + j * IndRecords.RESULT24;
        return new long[] {r.getLong(e + 8), r.getLong(e + 16)};
    }

    /** What the chunk's saved state would change if it were saved now: it is unsaved, or carries the op, or holds an epoch. */
    private static String touched(ServerLevel level, ChunkPos c, long op, long district) {
        LevelChunk chunk = level.getChunk(c.x, c.z);
        IndustryAttachments.Epochs e = chunk.getExistingDataOrNull(IndustryAttachments.RECONCILE.get());
        return "unsaved " + chunk.isUnsaved() + ", carries the op " + RtsimSession.chunkCarriesOp(chunk, op)
                + ", epoch " + (e == null ? 0 : e.epoch(district));
    }

    private static String outcome(int now, int exact) {
        return now > exact ? "DUPE" : now == exact ? "EXACT" : "LOSS";
    }

    /**
     * I1 (RP-215, slice U-f), a positive delta: P in A holds 40 diamonds in slot 0 and stone in slots 1 to 26 (it takes
     * 24 more), Q in N is empty. A reconcile of A puts 100 diamonds at P's position. P is its own inventory, so P takes
     * 24 and the result says so (applied 24 of 100: the engine owes the 76 left), and nothing is written to N: Q is
     * empty, N is not unsaved, carries no result and holds no epoch. The pair's handler (before) filled Q, in N, which
     * no command named: applied 100, 76 of them in N, whose chunk neither noted nor carried the op.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 1200, batch = "mic_rtsim_saved_ahead")
    public static void aReconcileIntoASplitHalfWritesOnlyThatChunk(GameTestHelper helper) {
        String t = "aReconcileIntoASplitHalfWritesOnlyThatChunk";
        ServerLevel level = helper.getLevel();
        Split s = split(helper, 6870);
        long op = engineOp(), district = engineOp();
        int delta = 100;
        helper.startSequence()
                .thenExecute(() -> force(level, s, true))
                .thenWaitUntil(paced(() -> check("A and N load", loaded(level, s.a()) && loaded(level, s.n()))))
                .thenExecute(() -> {
                    doublePair(level, s);
                    Container p = half(level, s.p());
                    p.setItem(0, new ItemStack(Items.DIAMOND, 40));
                    stone(p, 1);
                })
                .thenIdle(40) // what the setup marked, and the light updates after it, are done
                .thenExecute(() -> {
                    level.getChunk(s.n().x, s.n().z).setUnsaved(false);
                    IndustryTestKit.reconcile(op, district, 1, level, s.a().x, s.a().z, List.of(new IndustryTestKit.Delta(0, GOOD_DIAMOND, s.p(), delta)));
                })
                .thenWaitUntil(paced(() -> check("the reconcile answered", !IndustryTestKit.commandResults(op).isEmpty())))
                .thenExecute(() -> {
                    int inP = count(half(level, s.p()), Items.DIAMOND), inQ = count(half(level, s.q()), Items.DIAMOND);
                    long[] r = resultOf(op, 0);
                    String inN = touched(level, s.n(), op, district);
                    record(t, "result, requested and applied, diamonds in P, in Q, N", IndustryTestKit.commandResults(op) + ", " + r[0] + ", " + r[1] + ", " + inP + ", " + inQ + ", " + inN);
                    check("applied", IndustryTestKit.commandResults(op).get(0) == Obs.CommandResult.APPLIED);
                    check("P took what fits, 24 of 100, and the result says so: requested " + r[0] + ", applied " + r[1], r[0] == delta && r[1] == 24);
                    check("P holds 64, Q nothing: P " + inP + ", Q " + inQ, inP == 64 && inQ == 0);
                    check("N was not written, noted or carried: " + inN, inN.equals("unsaved false, carries the op false, epoch 0"));
                    check("A carries the op", RtsimSession.chunkCarriesOp(level.getChunk(s.a().x, s.a().z), op) || epochIn(level, s.a(), district, 1));
                })
                .thenExecute(() -> {
                    dropCarried(level.getChunk(s.a().x, s.a().z), op);
                    emptyPair(level, s);
                    force(level, s, false);
                })
                .thenSucceed();
    }

    /**
     * I1 (RP-215, slice U-f), the crash with the half rolled back: a +20 reconcile into P (40 diamonds in slot 0), then
     * a crash that takes A back to before it, P's slots and its epoch, and the journal with it. The engine resends the
     * op, which applies once: P holds 60.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 1200, batch = "mic_rtsim_saved_ahead")
    public static void aSplitHalfRolledBackWithItsEpochTakesTheResendOnce(GameTestHelper helper) {
        String t = "aSplitHalfRolledBackWithItsEpochTakesTheResendOnce";
        ServerLevel level = helper.getLevel();
        Split s = split(helper, 6950);
        long op = engineOp(), district = engineOp();
        int delta = 20;
        IndustryAttachments.Epochs[] before = new IndustryAttachments.Epochs[1];
        helper.startSequence()
                .thenExecute(() -> force(level, s, true))
                .thenWaitUntil(paced(() -> check("A and N load", loaded(level, s.a()) && loaded(level, s.n()))))
                .thenExecute(() -> {
                    doublePair(level, s);
                    half(level, s.p()).setItem(0, new ItemStack(Items.DIAMOND, 40));
                    before[0] = level.getChunk(s.a().x, s.a().z).getData(IndustryAttachments.RECONCILE.get());
                    IndustryTestKit.reconcile(op, district, 1, level, s.a().x, s.a().z, List.of(new IndustryTestKit.Delta(0, GOOD_DIAMOND, s.p(), delta)));
                })
                .thenWaitUntil(paced(() -> check("the reconcile answered", !IndustryTestKit.commandResults(op).isEmpty())))
                .thenExecute(() -> {
                    int inP = count(half(level, s.p()), Items.DIAMOND);
                    check("the first apply: P " + inP, inP == 60);
                    // The crash takes A back: P as it was, no epoch, nothing carried; the journal is from before.
                    Container p = half(level, s.p());
                    p.clearContent();
                    p.setItem(0, new ItemStack(Items.DIAMOND, 40));
                    level.getChunk(s.a().x, s.a().z).setData(IndustryAttachments.RECONCILE.get(), before[0]);
                    dropCarried(level.getChunk(s.a().x, s.a().z), op);
                    RtsimTests.session().boundary().journal().forgetAppliedForTest(op);
                    IndustryTestKit.reconcile(op, district, 1, level, s.a().x, s.a().z, List.of(new IndustryTestKit.Delta(0, GOOD_DIAMOND, s.p(), delta)));
                })
                .thenWaitUntil(paced(() -> check("the resend answered", IndustryTestKit.commandResults(op).size() >= 2)))
                .thenExecute(() -> {
                    int inP = count(half(level, s.p()), Items.DIAMOND), inQ = count(half(level, s.q()), Items.DIAMOND);
                    List<Integer> rs = IndustryTestKit.commandResults(op);
                    record(t, "results, diamonds in P and Q after the resend (the delta was " + delta + ")", rs + ", " + inP + ", " + inQ);
                    record(t, "a positive delta through a half rolled back with its epoch", outcome(inP + inQ - 40, delta));
                    check("the resend applied once: " + rs + ", P " + inP + ", Q " + inQ,
                            rs.size() == 2 && rs.get(1) == Obs.CommandResult.APPLIED && inP == 60 && inQ == 0);
                })
                .thenExecute(() -> {
                    dropCarried(level.getChunk(s.a().x, s.a().z), op);
                    emptyPair(level, s);
                    force(level, s, false);
                })
                .thenSucceed();
    }

    /**
     * I1 (RP-215, slice U-f), the crash with the half kept: the same +20, and a crash that keeps A (P at 60, the epoch)
     * while the journal is from before. The resend is refused by the epoch in A (DUPLICATE_OP): P holds 60, not 80.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 1200, batch = "mic_rtsim_saved_ahead")
    public static void aSplitHalfKeptByTheCrashRefusesTheResendByItsEpoch(GameTestHelper helper) {
        String t = "aSplitHalfKeptByTheCrashRefusesTheResendByItsEpoch";
        ServerLevel level = helper.getLevel();
        Split s = split(helper, 6970);
        long op = engineOp(), district = engineOp();
        int delta = 20;
        helper.startSequence()
                .thenExecute(() -> force(level, s, true))
                .thenWaitUntil(paced(() -> check("A and N load", loaded(level, s.a()) && loaded(level, s.n()))))
                .thenExecute(() -> {
                    doublePair(level, s);
                    half(level, s.p()).setItem(0, new ItemStack(Items.DIAMOND, 40));
                    IndustryTestKit.reconcile(op, district, 1, level, s.a().x, s.a().z, List.of(new IndustryTestKit.Delta(0, GOOD_DIAMOND, s.p(), delta)));
                })
                .thenWaitUntil(paced(() -> check("the reconcile answered", !IndustryTestKit.commandResults(op).isEmpty())))
                .thenExecute(() -> {
                    check("the first apply: P " + count(half(level, s.p()), Items.DIAMOND), count(half(level, s.p()), Items.DIAMOND) == 60);
                    // The crash keeps A, with P and its epoch; the journal is from before.
                    dropCarried(level.getChunk(s.a().x, s.a().z), op);
                    RtsimTests.session().boundary().journal().forgetAppliedForTest(op);
                    IndustryTestKit.reconcile(op, district, 1, level, s.a().x, s.a().z, List.of(new IndustryTestKit.Delta(0, GOOD_DIAMOND, s.p(), delta)));
                })
                .thenWaitUntil(paced(() -> check("the resend answered", IndustryTestKit.commandResults(op).size() >= 2)))
                .thenExecute(() -> {
                    int inP = count(half(level, s.p()), Items.DIAMOND), inQ = count(half(level, s.q()), Items.DIAMOND);
                    List<Integer> rs = IndustryTestKit.commandResults(op);
                    record(t, "results, diamonds in P and Q after the resend (the delta was " + delta + ")", rs + ", " + inP + ", " + inQ);
                    record(t, "a positive delta through a half the crash kept", outcome(inP + inQ - 40, delta));
                    check("the resend was refused by A's epoch, and P holds 60: " + rs + ", P " + inP + ", Q " + inQ,
                            rs.size() == 2 && rs.get(1) == Obs.CommandResult.DUPLICATE_OP && inP == 60 && inQ == 0);
                })
                .thenExecute(() -> {
                    dropCarried(level.getChunk(s.a().x, s.a().z), op);
                    emptyPair(level, s);
                    force(level, s, false);
                })
                .thenSucceed();
    }

    /**
     * I1 (RP-215, slice U-f), a negative delta: P in A full of stone, Q in N holding 40 diamonds. A reconcile of -20
     * diamonds at P's position finds none in P: the goods are in the other inventory. It takes nothing (applied 0, a
     * shortfall the engine books), and Q keeps its 40. The pair's handler (before) took the 20 from Q, in N, which no
     * command named.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 1200, batch = "mic_rtsim_saved_ahead")
    public static void aRemovalWhoseGoodsSitInTheOtherHalfIsAShortfall(GameTestHelper helper) {
        String t = "aRemovalWhoseGoodsSitInTheOtherHalfIsAShortfall";
        ServerLevel level = helper.getLevel();
        Split s = split(helper, 6990);
        long op = engineOp(), district = engineOp();
        int delta = -20;
        helper.startSequence()
                .thenExecute(() -> force(level, s, true))
                .thenWaitUntil(paced(() -> check("A and N load", loaded(level, s.a()) && loaded(level, s.n()))))
                .thenExecute(() -> {
                    doublePair(level, s);
                    stone(half(level, s.p()), 0);
                    half(level, s.q()).setItem(0, new ItemStack(Items.DIAMOND, 40));
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    level.getChunk(s.n().x, s.n().z).setUnsaved(false);
                    IndustryTestKit.reconcile(op, district, 1, level, s.a().x, s.a().z, List.of(new IndustryTestKit.Delta(0, GOOD_DIAMOND, s.p(), delta)));
                })
                .thenWaitUntil(paced(() -> check("the reconcile answered", !IndustryTestKit.commandResults(op).isEmpty())))
                .thenExecute(() -> {
                    int inP = count(half(level, s.p()), Items.DIAMOND), inQ = count(half(level, s.q()), Items.DIAMOND);
                    long[] r = resultOf(op, 0);
                    String inN = touched(level, s.n(), op, district);
                    record(t, "result, requested and applied, diamonds in P, in Q, N", IndustryTestKit.commandResults(op) + ", " + r[0] + ", " + r[1] + ", " + inP + ", " + inQ + ", " + inN);
                    check("applied", IndustryTestKit.commandResults(op).get(0) == Obs.CommandResult.APPLIED);
                    check("nothing was taken, a shortfall: requested " + r[0] + ", applied " + r[1], r[0] == delta && r[1] == 0);
                    check("Q keeps its 40, P has none: P " + inP + ", Q " + inQ, inP == 0 && inQ == 40);
                    check("N was not written, noted or carried: " + inN, inN.equals("unsaved false, carries the op false, epoch 0"));
                })
                .thenExecute(() -> {
                    dropCarried(level.getChunk(s.a().x, s.a().z), op);
                    emptyPair(level, s);
                    force(level, s, false);
                })
                .thenSucceed();
    }

    /**
     * The reviewer's P1-a sequence (RP-215): a reconcile X held for A while A is not loaded, N loading first and its
     * carried records acknowledged, then A loading. X must apply exactly once. The first apply goes into P, and the
     * crash takes A back (P empty, no epoch, the journal from before) while N is kept and unloaded with A. N alone
     * loads, and the engine acknowledges until N carries nothing of the op (it never carried any: the reconcile
     * changed nothing in N). X comes while A is unloaded and waits. A loads, and X applies: P holds the delta once, Q
     * nothing. The pair's handler (before) had put the first apply into Q, in N, whose carry the engine's
     * acknowledgement dropped: the held X then put the delta into Q again, 40 for 20.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 2400, batch = "mic_rtsim_saved_ahead")
    public static void aReconcileHeldForAnUnloadedHalfAppliesOnceAfterTheOtherChunkIsAcked(GameTestHelper helper) {
        String t = "aReconcileHeldForAnUnloadedHalfAppliesOnceAfterTheOtherChunkIsAcked";
        ServerLevel level = helper.getLevel();
        ScriptedEngine stub = RtsimTests.engine();
        Split s = split(helper, 7010);
        long op = engineOp(), district = engineOp();
        int delta = 20;
        IndustryAttachments.Epochs[] before = new IndustryAttachments.Epochs[1];
        int[] heldBefore = new int[1];
        Runnable send = () -> IndustryTestKit.reconcile(op, district, 1, level, s.a().x, s.a().z, List.of(new IndustryTestKit.Delta(0, GOOD_DIAMOND, s.p(), delta)));
        helper.startSequence()
                .thenExecute(() -> force(level, s, true))
                .thenWaitUntil(paced(() -> check("A and N load", loaded(level, s.a()) && loaded(level, s.n()))))
                .thenExecute(() -> {
                    doublePair(level, s);
                    before[0] = level.getChunk(s.a().x, s.a().z).getData(IndustryAttachments.RECONCILE.get());
                    send.run();
                })
                .thenWaitUntil(paced(() -> check("the reconcile answered", !IndustryTestKit.commandResults(op).isEmpty())))
                .thenExecute(() -> {
                    int inP = count(half(level, s.p()), Items.DIAMOND), inQ = count(half(level, s.q()), Items.DIAMOND);
                    record(t, "after the first apply: result, diamonds in P, in Q", IndustryTestKit.commandResults(op) + ", " + inP + ", " + inQ);
                    check("applied", IndustryTestKit.commandResults(op).get(0) == Obs.CommandResult.APPLIED);
                    check("the delta went into P: P " + inP + ", Q " + inQ, inP == delta && inQ == 0);
                    // The crash keeps N as it is; A (P empty, no epoch, nothing carried) and the journal are from before.
                    half(level, s.p()).clearContent();
                    level.getChunk(s.a().x, s.a().z).setData(IndustryAttachments.RECONCILE.get(), before[0]);
                    dropCarried(level.getChunk(s.a().x, s.a().z), op);
                    RtsimTests.session().boundary().journal().forgetAppliedForTest(op);
                    // The restart: no chunk is loaded yet. A forced chunk keeps its neighbours loaded, so both are let go.
                    force(level, s, false);
                })
                .thenWaitUntil(paced(() -> check("A and N unload", !loaded(level, s.a()) && !loaded(level, s.n()))))
                // N alone: its load ticket lasts a tick, so it is asked for every tick, and the engine is asked to
                // acknowledge what it has until N carries nothing of the op.
                .thenWaitUntil(paced(() -> {
                    level.getChunk(s.n().x, s.n().z);
                    stub.requestSaveAtNextStep();
                    check("N carries nothing of the op", !RtsimSession.chunkCarriesOp(level.getChunk(s.n().x, s.n().z), op));
                    check("A is not loaded", !loaded(level, s.a()));
                }))
                .thenExecute(() -> {
                    level.getChunk(s.n().x, s.n().z);
                    check("A is still not loaded", !loaded(level, s.a()));
                    heldBefore[0] = IndustryTestKit.ind().reconciler().held();
                    send.run(); // the engine's resend: held for A
                })
                .thenWaitUntil(paced(() -> {
                    level.getChunk(s.n().x, s.n().z);
                    check("the reconcile is held for A", IndustryTestKit.ind().reconciler().held() > heldBefore[0]);
                }))
                .thenExecute(() -> level.setChunkForced(s.a().x, s.a().z, true))
                .thenWaitUntil(paced(() -> check("A loads", loaded(level, s.a()))))
                .thenWaitUntil(paced(() -> check("the held reconcile answered again", IndustryTestKit.commandResults(op).size() >= 2)))
                .thenExecute(() -> {
                    int inP = count(half(level, s.p()), Items.DIAMOND), inQ = count(half(level, s.q()), Items.DIAMOND);
                    List<Integer> rs = IndustryTestKit.commandResults(op);
                    record(t, "after the held reconcile: results, diamonds in P, in Q (the delta was " + delta + ")", rs + ", " + inP + ", " + inQ);
                    record(t, "direction B, a reconcile held until after the other chunk's ack", outcome(inP + inQ, delta));
                    check("applied once: " + rs + ", P " + inP + ", Q " + inQ, rs.get(rs.size() - 1) == Obs.CommandResult.APPLIED && inP + inQ == delta && inP == delta);
                })
                .thenExecute(() -> {
                    dropCarried(level.getChunk(s.a().x, s.a().z), op);
                    emptyPair(level, s);
                    force(level, s, false);
                })
                .thenSucceed();
    }

    /**
     * A transfer into a half is a transfer into that half (RP-215): P in A holds 27 stacks of stone, Q in N is empty.
     * 20 diamonds to P's position do not fit, so the transfer answers FAILED and places nothing, though the pair's
     * handler (before) would have put them into Q, in N, which the transfer does not name.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 1200, batch = "mic_rtsim_saved_ahead")
    public static void aTransferIntoAFullSplitHalfFailsWhileTheOtherHalfHasRoom(GameTestHelper helper) {
        String t = "aTransferIntoAFullSplitHalfFailsWhileTheOtherHalfHasRoom";
        ServerLevel level = helper.getLevel();
        ScriptedEngine stub = RtsimTests.engine();
        String dim = RtsimTests.dimension(helper);
        Split s = split(helper, 7030);
        long op = engineOp();
        helper.startSequence()
                .thenExecute(() -> force(level, s, true))
                .thenWaitUntil(paced(() -> check("A and N load", loaded(level, s.a()) && loaded(level, s.n()))))
                .thenExecute(() -> {
                    doublePair(level, s);
                    stone(half(level, s.p()), 0);
                    stub.transferToContainer(op, dim, s.p().getX(), s.p().getY(), s.p().getZ(), "minecraft:diamond", 20);
                })
                .thenWaitUntil(paced(() -> check("the transfer answered", !results(stub, op).isEmpty())))
                .thenExecute(() -> {
                    int inP = count(half(level, s.p()), Items.DIAMOND), inQ = count(half(level, s.q()), Items.DIAMOND);
                    List<Integer> rs = results(stub, op).stream().map(r -> r.aux()).toList();
                    record(t, "results, diamonds in P, in Q", rs + ", " + inP + ", " + inQ);
                    check("the transfer failed: " + rs, rs.equals(List.of(Obs.CommandResult.FAILED)));
                    check("nothing was placed: P " + inP + ", Q " + inQ, inP == 0 && inQ == 0);
                })
                .thenExecute(() -> {
                    emptyPair(level, s);
                    force(level, s, false);
                })
                .thenSucceed();
    }

    /**
     * A transfer into a half with room (RP-215): 20 diamonds to P's position go into P, and only A carries the result:
     * N, whose Q the pair's handler (before) filled first, carries nothing of it.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 1200, batch = "mic_rtsim_saved_ahead")
    public static void aTransferIntoASplitHalfIsCarriedByThatChunkOnly(GameTestHelper helper) {
        String t = "aTransferIntoASplitHalfIsCarriedByThatChunkOnly";
        ServerLevel level = helper.getLevel();
        ScriptedEngine stub = RtsimTests.engine();
        String dim = RtsimTests.dimension(helper);
        Split s = split(helper, 7050);
        long op = engineOp();
        helper.startSequence()
                .thenExecute(() -> force(level, s, true))
                .thenWaitUntil(paced(() -> check("A and N load", loaded(level, s.a()) && loaded(level, s.n()))))
                .thenExecute(() -> {
                    doublePair(level, s);
                    stub.transferToContainer(op, dim, s.p().getX(), s.p().getY(), s.p().getZ(), "minecraft:diamond", 20);
                })
                .thenWaitUntil(paced(() -> check("the transfer answered", !results(stub, op).isEmpty())))
                .thenExecute(() -> {
                    int inP = count(half(level, s.p()), Items.DIAMOND), inQ = count(half(level, s.q()), Items.DIAMOND);
                    List<Integer> rs = results(stub, op).stream().map(r -> r.aux()).toList();
                    boolean aCarries = RtsimSession.chunkCarriesOp(level.getChunk(s.a().x, s.a().z), op);
                    boolean nCarries = RtsimSession.chunkCarriesOp(level.getChunk(s.n().x, s.n().z), op);
                    record(t, "results, diamonds in P, in Q, A carries, N carries", rs + ", " + inP + ", " + inQ + ", " + aCarries + ", " + nCarries);
                    check("applied: " + rs, rs.equals(List.of(Obs.CommandResult.APPLIED)));
                    check("the diamonds went into P: P " + inP + ", Q " + inQ, inP == 20 && inQ == 0);
                    check("A carries the result", aCarries);
                    check("N carries nothing", !nCarries);
                })
                .thenExecute(() -> {
                    dropCarried(level.getChunk(s.a().x, s.a().z), op);
                    emptyPair(level, s);
                    force(level, s, false);
                })
                .thenSucceed();
    }

    /**
     * A comparator by the far half (RP-215): a comparator beside Q, in N, reads the pair's fullness, as a vanilla pair
     * answers it. A reconcile writes diamonds into P, in A, through P's own handler, whose change reaches only the
     * comparators beside P; the sim's write tells the other half's neighbours too, so the comparator beside Q
     * leaves 0.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 1200, batch = "mic_rtsim_saved_ahead")
    public static void aComparatorByTheFarHalfFollowsAWriteThroughTheNearHalf(GameTestHelper helper) {
        String t = "aComparatorByTheFarHalfFollowsAWriteThroughTheNearHalf";
        ServerLevel level = helper.getLevel();
        Split s = split(helper, 7070);
        BlockPos cmp = s.q().east();
        long op = engineOp(), district = engineOp();
        helper.startSequence()
                .thenExecute(() -> force(level, s, true))
                .thenWaitUntil(paced(() -> check("A and N load", loaded(level, s.a()) && loaded(level, s.n()))))
                .thenExecute(() -> {
                    doublePair(level, s);
                    level.setBlock(cmp.below(), Blocks.STONE.defaultBlockState(), 2 | 16);
                    // Its input is the block it faces: the far half, to its west.
                    level.setBlock(cmp, Blocks.COMPARATOR.defaultBlockState().setValue(net.minecraft.world.level.block.ComparatorBlock.FACING, Direction.WEST), 3);
                })
                .thenIdle(10)
                .thenExecute(() -> {
                    check("the comparator reads 0 for an empty pair", level.getBlockEntity(cmp) instanceof net.minecraft.world.level.block.entity.ComparatorBlockEntity c && c.getOutputSignal() == 0);
                    IndustryTestKit.reconcile(op, district, 1, level, s.a().x, s.a().z, List.of(new IndustryTestKit.Delta(0, GOOD_DIAMOND, s.p(), 20)));
                })
                .thenWaitUntil(paced(() -> check("the reconcile answered", !IndustryTestKit.commandResults(op).isEmpty())))
                .thenWaitUntil(paced(() -> check("the comparator beside the far half reads the pair's diamonds",
                        level.getBlockEntity(cmp) instanceof net.minecraft.world.level.block.entity.ComparatorBlockEntity c && c.getOutputSignal() >= 1)))
                .thenExecute(() -> {
                    int out = ((net.minecraft.world.level.block.entity.ComparatorBlockEntity) level.getBlockEntity(cmp)).getOutputSignal();
                    record(t, "the comparator's output after a write through the near half", out);
                    dropCarried(level.getChunk(s.a().x, s.a().z), op);
                    level.setBlock(cmp, Blocks.AIR.defaultBlockState(), 2 | 16);
                    level.setBlock(cmp.below(), Blocks.AIR.defaultBlockState(), 2 | 16);
                    emptyPair(level, s);
                    force(level, s, false);
                })
                .thenSucceed();
    }

    /**
     * A sim break of a pair inside one chunk (RP-215, the reviewer's finding on the pair design): P and Q are one
     * inventory there, described at its lower half P, so the engine's doomed inventory holds both halves' goods
     * ({@code Inventory.effective}) and books them all as dropped when P breaks. The world drops what P itself holds
     * ({@code Containers.dropContentsOnDestroy} reads the half's own block entity): 100 diamonds, and the chest. Q
     * keeps its 30 emeralds, now a single chest. Nothing is spawned beyond what the world held, so the overbooked
     * dropped ledger is not a dupe: the goods in Q rejoin the sim when its next detection finds Q a chest of its own.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 2400, batch = "mic_rtsim_saved_ahead")
    public static void aSimBreakOfAPairsFirstHalfDropsOnlyThatHalf(GameTestHelper helper) {
        String t = "aSimBreakOfAPairsFirstHalfDropsOnlyThatHalf";
        ServerLevel level = helper.getLevel();
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        ChunkPos c = new ChunkPos((o.getX() >> 4) + 7090, (o.getZ() >> 4) + 6810);
        int y = 180;
        BlockPos p = new BlockPos(c.getMinBlockX() + 8, y, c.getMinBlockZ() + 8), q = p.east(); // p has the lower position
        AABB around = new AABB(p).inflate(6);
        BlockState pState = Blocks.CHEST.defaultBlockState().setValue(ChestBlock.FACING, Direction.NORTH).setValue(ChestBlock.TYPE, ChestType.LEFT);
        long op = engineOp(), district = engineOp();
        helper.startSequence()
                .thenExecute(() -> level.setChunkForced(c.x, c.z, true))
                .thenWaitUntil(paced(() -> check("C and the entities around it load", loaded(level, c) && entitiesLoadedAround(level, c))))
                .thenExecute(() -> {
                    doublePair(level, p, q, y, Direction.NORTH, ChestType.LEFT);
                    fill(level, p.getX() - 4, y - 1, p.getZ() - 4, q.getX() + 4, y - 1, p.getZ() + 4, Blocks.STONE.defaultBlockState());
                    half(level, p).setItem(0, new ItemStack(Items.DIAMOND, 64));
                    half(level, p).setItem(1, new ItemStack(Items.DIAMOND, 36));
                    half(level, q).setItem(0, new ItemStack(Items.EMERALD, 30));
                    IndustryTestKit.reconcile(op, district, 1, level, c.x, c.z, List.of(),
                            List.of(new IndustryTestKit.Destroy(p, BlockStateParser.serialize(pState))));
                })
                .thenWaitUntil(paced(() -> check("the drops are in the world", lying(level, around, Items.DIAMOND) > 0)))
                .thenIdle(40)
                .thenExecute(() -> {
                    int diamonds = lying(level, around, Items.DIAMOND), emeralds = lying(level, around, Items.EMERALD), chests = lying(level, around, Items.CHEST);
                    BlockState left = level.getBlockState(q);
                    int inQ = level.getBlockEntity(q) instanceof Container qc ? count(qc, Items.EMERALD) : -1;
                    record(t, "diamonds, emeralds and chests lying, emeralds left in Q, Q's type", diamonds + ", " + emeralds + ", " + chests + ", " + inQ + ", " + left.getValue(ChestBlock.TYPE));
                    check("the reconcile applied", IndustryTestKit.commandResults(op).equals(List.of(Obs.CommandResult.APPLIED)));
                    check("P's goods dropped, once: " + diamonds + " diamonds, " + chests + " chests", diamonds == 100 && chests == 1);
                    check("Q's goods did not: " + emeralds + " lying, " + inQ + " in Q", emeralds == 0 && inQ == 30);
                    check("Q is a single chest now: " + left, left.is(Blocks.CHEST) && left.getValue(ChestBlock.TYPE) == ChestType.SINGLE);
                    // The goods the engine booked as dropped but the world kept rejoin the sim at the next detection: it finds
                    // Q a chest of its own, an inventory of 27 slots at its own position.
                    District.Inventory survivor = Districts.describeInventory(level, q, 0);
                    record(t, "the surviving chest's inventory at a detection", survivor);
                    check("Q leads to itself", q.equals(Districts.canonicalInventory(level, q)));
                    check("Q describes as an inventory of 27 slots: " + survivor, survivor != null
                            && survivor.kind() == IndRecords.Inventory.KIND_ITEMS && survivor.itemSlots() == 27);
                })
                .thenExecute(() -> {
                    level.getEntitiesOfClass(ItemEntity.class, around).forEach(Entity::discard);
                    dropCarried(level.getChunk(c.x, c.z), op);
                    if (level.getBlockEntity(q) instanceof Container qc)
                        qc.clearContent();
                    fill(level, p.getX() - 4, y - 1, p.getZ() - 4, q.getX() + 4, y + 3, p.getZ() + 4, Blocks.AIR.defaultBlockState());
                    level.setChunkForced(c.x, c.z, false);
                })
                .thenSucceed();
    }

    /**
     * The same break from the other side (RP-215, slice U-f's open question on a same-chunk pair): the engine dooms only
     * the pair's lower half P, so a break of the higher half Q reaches it as a block with no inventory. The world drops
     * what Q itself holds, 30 emeralds, and the chest; P keeps its 100 diamonds, now a single chest. As for P's break,
     * nothing is spawned beyond what the world held: no dupe and no loss, whichever half breaks.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 2400, batch = "mic_rtsim_saved_ahead")
    public static void aSimBreakOfAPairsSecondHalfDropsOnlyThatHalf(GameTestHelper helper) {
        String t = "aSimBreakOfAPairsSecondHalfDropsOnlyThatHalf";
        ServerLevel level = helper.getLevel();
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        ChunkPos c = new ChunkPos((o.getX() >> 4) + 7130, (o.getZ() >> 4) + 6810);
        int y = 180;
        BlockPos p = new BlockPos(c.getMinBlockX() + 8, y, c.getMinBlockZ() + 8), q = p.east();
        AABB around = new AABB(p).inflate(6);
        BlockState qState = Blocks.CHEST.defaultBlockState().setValue(ChestBlock.FACING, Direction.NORTH).setValue(ChestBlock.TYPE, ChestType.RIGHT);
        long op = engineOp(), district = engineOp();
        helper.startSequence()
                .thenExecute(() -> level.setChunkForced(c.x, c.z, true))
                .thenWaitUntil(paced(() -> check("C and the entities around it load", loaded(level, c) && entitiesLoadedAround(level, c))))
                .thenExecute(() -> {
                    doublePair(level, p, q, y, Direction.NORTH, ChestType.LEFT);
                    fill(level, p.getX() - 4, y - 1, p.getZ() - 4, q.getX() + 4, y - 1, p.getZ() + 4, Blocks.STONE.defaultBlockState());
                    half(level, p).setItem(0, new ItemStack(Items.DIAMOND, 64));
                    half(level, p).setItem(1, new ItemStack(Items.DIAMOND, 36));
                    half(level, q).setItem(0, new ItemStack(Items.EMERALD, 30));
                    IndustryTestKit.reconcile(op, district, 1, level, c.x, c.z, List.of(),
                            List.of(new IndustryTestKit.Destroy(q, BlockStateParser.serialize(qState))));
                })
                .thenWaitUntil(paced(() -> check("the drops are in the world", lying(level, around, Items.EMERALD) > 0)))
                .thenIdle(40)
                .thenExecute(() -> {
                    int diamonds = lying(level, around, Items.DIAMOND), emeralds = lying(level, around, Items.EMERALD), chests = lying(level, around, Items.CHEST);
                    BlockState left = level.getBlockState(p);
                    int inP = level.getBlockEntity(p) instanceof Container pc ? count(pc, Items.DIAMOND) : -1;
                    record(t, "diamonds, emeralds and chests lying, diamonds left in P, P's type", diamonds + ", " + emeralds + ", " + chests + ", " + inP + ", " + left.getValue(ChestBlock.TYPE));
                    check("the reconcile applied", IndustryTestKit.commandResults(op).equals(List.of(Obs.CommandResult.APPLIED)));
                    check("Q's goods dropped, once: " + emeralds + " emeralds, " + chests + " chests", emeralds == 30 && chests == 1);
                    check("P's goods did not: " + diamonds + " lying, " + inP + " in P", diamonds == 0 && inP == 100);
                    check("P is a single chest now: " + left, left.is(Blocks.CHEST) && left.getValue(ChestBlock.TYPE) == ChestType.SINGLE);
                })
                .thenExecute(() -> {
                    level.getEntitiesOfClass(ItemEntity.class, around).forEach(Entity::discard);
                    dropCarried(level.getChunk(c.x, c.z), op);
                    if (level.getBlockEntity(p) instanceof Container pc)
                        pc.clearContent();
                    fill(level, p.getX() - 4, y - 1, p.getZ() - 4, q.getX() + 4, y + 3, p.getZ() + 4, Blocks.AIR.defaultBlockState());
                    level.setChunkForced(c.x, c.z, false);
                })
                .thenSucceed();
    }

    /**
     * Only a pair is a pair (the reviewer's P2, RP-215): a chest whose TYPE says it is half of a double chest but
     * whose neighbour is not its matching half (set with setBlock, as a world edit or another mod does) is a single
     * chest to NeoForge's item handler, which holds the whole stack in it, and to the sim's. A LEFT half at A's east
     * edge with nothing east of it, and another with a single chest east of it: a +20 reconcile fills each one, and N
     * is not carried, since nothing was done to it.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 1200, batch = "mic_rtsim_saved_ahead")
    public static void aChestThatOnlyLooksLikeAPairIsNotCarriedInTheOtherChunk(GameTestHelper helper) {
        String t = "aChestThatOnlyLooksLikeAPairIsNotCarriedInTheOtherChunk";
        ServerLevel level = helper.getLevel();
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        ChunkPos a = new ChunkPos((o.getX() >> 4) + 7110, (o.getZ() >> 4) + 6810), n = new ChunkPos(a.x + 1, a.z);
        int y = 180;
        BlockPos p1 = new BlockPos(a.getMaxBlockX(), y, a.getMinBlockZ() + 4), q1 = p1.east();   // q1 stays air
        BlockPos p2 = new BlockPos(a.getMaxBlockX(), y, a.getMinBlockZ() + 12), q2 = p2.east();  // q2 is a single chest
        long op = engineOp();
        long district = engineOp();
        int epoch = 1, delta = 20;
        helper.startSequence()
                .thenExecute(() -> {
                    level.setChunkForced(a.x, a.z, true);
                    level.setChunkForced(n.x, n.z, true);
                })
                .thenWaitUntil(paced(() -> check("A and N load", loaded(level, a) && loaded(level, n))))
                .thenExecute(() -> {
                    fill(level, p1.getX() - 2, y, p1.getZ() - 2, q2.getX() + 2, y + 2, q2.getZ() + 2, Blocks.AIR.defaultBlockState());
                    level.getEntitiesOfClass(ItemEntity.class, new AABB(p1).minmax(new AABB(q2)).inflate(6)).forEach(Entity::discard);
                    BlockState left = Blocks.CHEST.defaultBlockState().setValue(ChestBlock.FACING, Direction.NORTH).setValue(ChestBlock.TYPE, ChestType.LEFT);
                    level.setBlock(p1, left, 2 | 16);
                    level.setBlock(p2, left, 2 | 16);
                    level.setBlock(q2, Blocks.CHEST.defaultBlockState().setValue(ChestBlock.FACING, Direction.NORTH), 2 | 16);
                    IndustryTestKit.reconcile(op, district, epoch, level, a.x, a.z, List.of(
                            new IndustryTestKit.Delta(0, GOOD_DIAMOND, p1, delta), new IndustryTestKit.Delta(0, GOOD_DIAMOND, p2, delta)));
                })
                .thenWaitUntil(paced(() -> check("the reconcile answered", !IndustryTestKit.commandResults(op).isEmpty())))
                .thenExecute(() -> {
                    int in1 = count((Container) level.getBlockEntity(p1), Items.DIAMOND), in2 = count((Container) level.getBlockEntity(p2), Items.DIAMOND);
                    int inQ2 = count((Container) level.getBlockEntity(q2), Items.DIAMOND);
                    boolean nCarries = RtsimSession.chunkCarriesOp(level.getChunk(n.x, n.z), op);
                    record(t, "after the reconcile: result, diamonds in P1, P2, the single chest, N carries the op",
                            IndustryTestKit.commandResults(op) + ", " + in1 + ", " + in2 + ", " + inQ2 + ", " + nCarries);
                    check("applied", IndustryTestKit.commandResults(op).get(0) == Obs.CommandResult.APPLIED);
                    check("each half holds its delta, whole: " + in1 + ", " + in2 + ", " + inQ2, in1 == delta && in2 == delta && inQ2 == 0);
                    check("N carries nothing of an op that did nothing to N", !nCarries);
                })
                .thenExecute(() -> {
                    dropCarried(level.getChunk(n.x, n.z), op);
                    dropCarried(level.getChunk(a.x, a.z), op);
                    fill(level, p1.getX() - 2, y, p1.getZ() - 2, q2.getX() + 2, y + 2, q2.getZ() + 2, Blocks.AIR.defaultBlockState());
                    level.setChunkForced(a.x, a.z, false);
                    level.setChunkForced(n.x, n.z, false);
                })
                .thenSucceed();
    }
}
