package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryAttachments;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.level.BlockEvent;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.*;

/**
 * DS-10, the seed rule: only machines start a district. Storage (chests, barrels) and vanilla blocks
 * (furnaces, hoppers) never do, but once a district is there they are its members and its stock. The core's
 * server has no pack mod, so the machines are {@link TestMachines}' blocks: a ticking block entity with an
 * energy capability (the generic rule), one with a capability and no ticker, a ticking store in
 * {@code #mic_rtsim:district_not_seed}, and a block in {@code #mic_rtsim:district_seed}. Each test has its
 * own batch; the Create plugin's own cases (vaults, tanks, shafts, presses) are in its IndustryGameTests.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class DistrictSeedGameTests {
    private DistrictSeedGameTests() {}

    private static final String YARD = "industry_yard";

    private static BlockState machine() {
        return TestMachines.MACHINE.get().defaultBlockState();
    }

    private static void rescan(GameTestHelper helper) {
        ind().districts().rescanNow(helper.getLevel());
    }

    private static int seedsAt(GameTestHelper helper, BlockPos pos) {
        return helper.getLevel().getChunkAt(pos).getData(IndustryAttachments.SEEDS.get()).positions().length;
    }

    /** Several chunks of the yard, one block each: none of these starts a district, however long we wait. */
    @GameTest(template = YARD, batch = "district_seed_1", timeoutTicks = 400)
    public static void loneStorageAndVanillaBlocksMakeNoDistrict(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        BlockPos chest = inChunk(helper, 2, 2);
        BlockPos barrel = inChunk(helper, 18, 2);
        BlockPos furnace = inChunk(helper, 2, 18);
        BlockPos hopper = inChunk(helper, 18, 18);
        BlockPos inert = inChunk(helper, 6, 6);
        BlockPos store = inChunk(helper, 22, 6);
        BlockPos[] all = {chest, barrel, furnace, hopper, inert, store};
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, chest, Blocks.CHEST.defaultBlockState());
                    set(helper, barrel, Blocks.BARREL.defaultBlockState());
                    set(helper, furnace, Blocks.FURNACE.defaultBlockState());
                    set(helper, hopper, Blocks.HOPPER.defaultBlockState());
                    set(helper, inert, TestMachines.INERT.get().defaultBlockState());
                    set(helper, store, TestMachines.STORE.get().defaultBlockState());
                    for (BlockPos p : all)
                        playerPlaced(helper, p);
                })
                .thenIdle(10)
                .thenExecute(() -> {
                    rescan(helper);
                    for (BlockPos p : all) {
                        check("no seed at " + helper.getLevel().getBlockState(p).getBlock(), seedsAt(helper, p) == 0);
                        check("no district at " + helper.getLevel().getBlockState(p).getBlock(), districtAt(helper, p) == null);
                    }
                    record("loneStorageAndVanillaBlocksMakeNoDistrict", "districts in the yard (" + engineName() + ")", 0);
                })
                .thenSucceed();
    }

    /**
     * A ticking modded block entity with a capability is a machine and starts a district; a block in the seed
     * tag does with neither. A chest in the machine's chunk is a member, and its inventory is boundary stock.
     */
    @GameTest(template = YARD, batch = "district_seed_2", timeoutTicks = 400)
    public static void aMachineMakesADistrictAndAChestBesideItIsBoundaryStock(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        BlockPos machine = inChunk(helper, 4, 4);
        BlockPos chest = inChunk(helper, 6, 4);
        BlockPos marker = inChunk(helper, 20, 20);
        long[] id = {0};
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, chest, Blocks.CHEST.defaultBlockState());
                    playerPlaced(helper, chest);
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    rescan(helper);
                    check("a chest alone is no district", districtAt(helper, chest) == null);
                    set(helper, machine, machine());
                    playerPlaced(helper, machine);
                    set(helper, marker, TestMachines.MARKER.get().defaultBlockState());
                    playerPlaced(helper, marker);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, machine);
                    check("the machine's chunk is a district", d != null);
                    check("the chest's chunk is the same district", d == districtAt(helper, chest));
                    // The marker's chunk is diagonal to the machine's: next-door seeded chunks are one district.
                    check("two chunks: " + d.chunks.size(), d.chunks.size() == 2);
                    check("the district counts two seeds: " + d.seedCounts, d.seeds() == 2);
                    check("the chest is boundary stock: " + d.inventories, d.inventories.size() == 1 && d.inventories.get(0).pos().equals(chest));
                    check("a block in the seed tag seeds: the marker's chunk is in the district", districtAt(helper, marker) == d);
                    check("the marker's chunk holds its own seed", seedsAt(helper, marker) == 1);
                    id[0] = d.id;
                    record("aMachineMakesADistrictAndAChestBesideItIsBoundaryStock", "district (" + engineName() + ")", d);
                })
                .thenSucceed();
    }

    /** Break the only machine and the district dissolves at the next detection, chest and all. */
    @GameTest(template = YARD, batch = "district_seed_3", timeoutTicks = 400)
    public static void removingTheMachineDissolvesTheDistrict(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        ServerLevel level = helper.getLevel();
        BlockPos machine = inChunk(helper, 4, 4);
        BlockPos chest = inChunk(helper, 6, 4);
        long[] id = {0};
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, chest, Blocks.CHEST.defaultBlockState());
                    set(helper, machine, machine());
                    playerPlaced(helper, chest);
                    playerPlaced(helper, machine);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, machine);
                    check("the district", d != null && d.inventories.size() == 1);
                    id[0] = d.id;
                })
                .thenExecute(() -> {
                    NeoForge.EVENT_BUS.post(new BlockEvent.BreakEvent(level, machine, level.getBlockState(machine), FakePlayers.editor(level)));
                    level.removeBlock(machine, false);
                })
                .thenWaitUntil(() -> {
                    check("the district dissolved", ind().district(id[0]) == null && districtAt(helper, chest) == null);
                    check("the chest is still there", level.getBlockState(chest).is(Blocks.CHEST));
                    check("no seeds left", seedsAt(helper, machine) == 0);
                    record("removingTheMachineDissolvesTheDistrict", "district " + id[0] + " (" + engineName() + ")", "gone");
                })
                .thenSucceed();
    }

    /**
     * Chunks next to a district are not annexed by what they hold: a chest in the next chunk stays out; so
     * does a chest across the border from a chest (two stores carry no work). A hopper or a chest against a
     * machine across the border is a physical connection: the chunk joins, as a member without a seed.
     */
    @GameTest(template = YARD, batch = "district_seed_4", timeoutTicks = 600)
    public static void aStorageOnlyChunkNextToADistrictJoinsOnlyWhenConnected(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        BlockPos machine = inChunk(helper, 4, 2);
        BlockPos farChest = inChunk(helper, 24, 2);          // chunk +1 in x, no connection
        BlockPos edgeChest = inChunk(helper, 15, 6);         // lx 15 in the machine's chunk
        BlockPos acrossChest = inChunk(helper, 16, 6);       // lx 0 next door: chest against chest
        BlockPos edgeMachine = inChunk(helper, 15, 10);
        BlockPos acrossChest2 = inChunk(helper, 16, 10);     // a chest against a machine
        BlockPos edgeMachine2 = inChunk(helper, 15, 12);
        BlockPos acrossHopper = inChunk(helper, 16, 12);     // a hopper against a machine
        long next = ChunkPos.asLong((machine.getX() >> 4) + 1, machine.getZ() >> 4);
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, machine, machine());
                    playerPlaced(helper, machine);
                    set(helper, farChest, Blocks.CHEST.defaultBlockState());
                    set(helper, edgeChest, Blocks.CHEST.defaultBlockState());
                    set(helper, acrossChest, Blocks.CHEST.defaultBlockState());
                    playerPlaced(helper, farChest);
                    playerPlaced(helper, edgeChest);
                    playerPlaced(helper, acrossChest);
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    rescan(helper);
                    District d = districtAt(helper, machine);
                    check("the machine's district", d != null);
                    check("a chest in the next chunk stays out: " + d.chunks, !d.chunks.contains(next) && districtAt(helper, farChest) == null);
                    check("a chest across a border from a chest stays out", districtAt(helper, acrossChest) == null);
                    check("the chunk's own chest at the edge is stock", d.inventoryAt(edgeChest) != null);
                    set(helper, edgeMachine, machine());
                    set(helper, acrossChest2, Blocks.CHEST.defaultBlockState());
                    playerPlaced(helper, edgeMachine);
                    playerPlaced(helper, acrossChest2);
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    rescan(helper);
                    District d = districtAt(helper, machine);
                    check("a chest against a machine joins its chunk: " + d.chunks, d.chunks.contains(next));
                    check("it is the same district", districtAt(helper, farChest) == d && districtAt(helper, acrossChest) == d);
                    check("the chest across the border is stock: " + d.inventories, d.inventoryAt(acrossChest2) != null);
                    check("the next chunk holds no seed: " + d.seedCounts, d.seedCounts.get(next) == 0 && d.seeds() == 2);
                    record("aStorageOnlyChunkNextToADistrictJoinsOnlyWhenConnected", "joined by the chest (" + engineName() + ")", d);
                    // Take the machine at the border away: the next chunk is held by the other machine against
                    // its chest; with that gone too the chunk leaves while the district stays.
                    set(helper, edgeMachine2, machine());
                    set(helper, acrossHopper, Blocks.HOPPER.defaultBlockState());
                    playerPlaced(helper, edgeMachine2);
                    playerPlaced(helper, acrossHopper);
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    rescan(helper);
                    District d = districtAt(helper, machine);
                    check("still one district with the next chunk: " + d.chunks, d.chunks.contains(next));
                    ServerLevel level = helper.getLevel();
                    for (BlockPos p : new BlockPos[] {edgeMachine, edgeMachine2}) {
                        NeoForge.EVENT_BUS.post(new BlockEvent.BreakEvent(level, p, level.getBlockState(p), FakePlayers.editor(level)));
                        level.removeBlock(p, false);
                    }
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    rescan(helper);
                    District d = districtAt(helper, machine);
                    check("the district stays on the machine in its own chunk", d != null);
                    check("the chunk nothing connects any more leaves: " + d.chunks, !d.chunks.contains(next) && districtAt(helper, acrossChest2) == null);
                })
                .thenSucceed();
    }

    /**
     * Detection never loads a chunk to decide a district's fate. A district whose unloaded chunks recorded no
     * seed (storage only, or a save from before the rule) dissolves at the next detection; one whose unloaded
     * chunk recorded seeds stays whole, its seedless unloaded neighbour with it; and a loaded district is
     * untouched. Nothing is loaded on the way.
     */
    @GameTest(template = YARD, batch = "district_seed_5", timeoutTicks = 400)
    public static void aSeedlessDistrictDissolvesWithoutLoadingItsUnloadedChunks(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        ServerLevel level = helper.getLevel();
        BlockPos machine = inChunk(helper, 4, 4);
        int cx = machine.getX() >> 4, cz = machine.getZ() >> 4;
        long storeOnly = ChunkPos.asLong(cx + 9000, cz + 9000), storeOnly2 = ChunkPos.asLong(cx + 9001, cz + 9000);
        long seeded = ChunkPos.asLong(cx + 9100, cz + 9100), seeded2 = ChunkPos.asLong(cx + 9101, cz + 9100);
        long[] ids = new long[3];
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, machine, machine());
                    playerPlaced(helper, machine);
                    for (long c : new long[] {storeOnly, storeOnly2, seeded, seeded2})
                        check("chunk " + ChunkPos.getX(c) + "," + ChunkPos.getZ(c) + " is unloaded", level.getChunkSource().getChunkNow(ChunkPos.getX(c), ChunkPos.getZ(c)) == null);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, machine);
                    check("the machine's district", d != null);
                    ids[0] = d.id;
                    District a = ind().districts().createForTest(level, storeOnly);
                    ind().districts().addChunkForTest(a, storeOnly2);
                    ids[1] = a.id;
                    District b = ind().districts().createForTest(level, seeded);
                    ind().districts().addChunkForTest(b, seeded2); // an unloaded member that recorded no seed
                    b.seedCounts.put(seeded, 2);
                    ids[2] = b.id;
                })
                .thenExecute(() -> {
                    rescan(helper);
                    check("the district of unloaded storage dissolved", ind().district(ids[1]) == null
                            && ind().districts().at(level.dimension(), storeOnly) == null && ind().districts().at(level.dimension(), storeOnly2) == null);
                    District b = ind().district(ids[2]);
                    check("the district whose unloaded chunk recorded seeds stayed whole: " + (b == null ? null : b.chunks),
                            b != null && b.chunks.contains(seeded) && b.chunks.contains(seeded2));
                    check("the loaded machine's district is untouched", ind().district(ids[0]) != null && districtAt(helper, machine) == ind().district(ids[0]));
                    for (long c : new long[] {storeOnly, storeOnly2, seeded, seeded2})
                        check("detection loaded nothing", level.getChunkSource().getChunkNow(ChunkPos.getX(c), ChunkPos.getZ(c)) == null);
                    record("aSeedlessDistrictDissolvesWithoutLoadingItsUnloadedChunks", "districts left of three (" + engineName() + ")", 2);
                })
                .thenSucceed();
    }
}
