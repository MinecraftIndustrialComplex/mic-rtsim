package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Districts;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.StockAccess;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerChunkCache;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.TicketType;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.ChestBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.ChestBlockEntity;
import net.minecraft.world.level.block.state.properties.ChestType;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.neoforge.items.IItemHandler;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.Comparator;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.districtAt;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.fastConfig;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.forgetAll;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.ind;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.inChunk;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.playerPlaced;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.record;

/**
 * A district detection never waits for a chunk that is due but still loading (R2-11), for a vanilla double
 * chest across a chunk border: NeoForge's chest item handler is the pair's, and {@code ChestBlock.getContainer}
 * reads the other half through {@code Level.getBlockState}. A vanilla pair across a border is two inventories,
 * each half its own 27 slots, read through the half's own block entity (RP-215), which reads nothing of the other
 * chunk; the two chunks are one district all the same. Create's half (fan links, vaults) is in the create
 * project's {@code DetectionLoadGameTests} and {@code GlassFanLinkGameTests}.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class ChestPairLoadGameTests {
    private ChestPairLoadGameTests() {}

    private static final String YARD = "industry_yard";

    /** A ticket that holds a chunk FULL (level 33), not block-ticking: its neighbours stay at 34, not loaded. */
    private static final TicketType<ChunkPos> FULL_ONLY = TicketType.create("mic_rtsim_chest_pair_full_only", Comparator.comparingLong(ChunkPos::toLong));

    /** A failed wait sleeps a little: the GameTest server ticks unthrottled, and a chunk's unload takes wall time. */
    private static Runnable paced(Runnable check) {
        return () -> {
            try {
                check.run();
            } catch (RuntimeException e) {
                try {
                    Thread.sleep(5);
                } catch (InterruptedException ie) {
                    Thread.currentThread().interrupt();
                }
                throw e;
            }
        };
    }

    /**
     * Classifying, describing and reading a double chest across a chunk border never waits for the other chunk, still
     * loading or not, and each half is its own 27 slots (R2-11, RG17, RP-215). A double chest along x across the
     * border of two far chunks: A holds the lower half, N the other, and each is its own inventory
     * ({@code Districts.canonicalInventory} names the half itself). Both are let go until both have unloaded; then A
     * alone is held FULL, and N is at level 34. In one tick N gets a FULL ticket and the distance manager's updates run
     * ({@code save(false)}): {@code Level.isLoaded} passes in N while N is not FULL, the state the first detection
     * after a start met. The classifier's verdict, the description and the item handler, asked at once, must each
     * leave N not FULL (the pair's handler read the other half through {@code Level.getBlockState}, waited for N's
     * load and ran every other chunk load due meanwhile), and A's half reads whole (it reads none of N) and as when
     * both were loaded: 27 slots.
     */
    @GameTest(template = YARD, batch = "chest_pair_load_01", timeoutTicks = 4000)
    public static void aDoubleChestIsClassifiedAndDescribedWithoutWaitingForAChunkStillLoading(GameTestHelper helper) {
        String t = "aDoubleChestIsClassifiedAndDescribedWithoutWaitingForAChunkStillLoading";
        ServerLevel level = helper.getLevel();
        ServerChunkCache cache = level.getChunkSource();
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        int cx = (o.getX() >> 4) + 6700, cz = (o.getZ() >> 4) + 6700, y = 180, z = cz * 16 + 8;
        // West half points east (LEFT, facing north), east half points west (RIGHT).
        BlockPos west = new BlockPos(cx * 16 + 15, y, z), east = west.east();
        BlockPos canon = west.asLong() < east.asLong() ? west : east, other = canon == west ? east : west;
        ChunkPos a = new ChunkPos(canon), n = new ChunkPos(other);
        District.Inventory[] whole = new District.Inventory[1];
        BlockEntity[] old = new BlockEntity[2];
        helper.startSequence()
                .thenExecute(() -> {
                    level.setChunkForced(a.x, a.z, true);
                    level.setChunkForced(n.x, n.z, true);
                })
                .thenWaitUntil(paced(() -> check("A and N load", cache.getChunkNow(a.x, a.z) != null && cache.getChunkNow(n.x, n.z) != null)))
                .thenExecute(() -> {
                    level.setBlock(west, Blocks.CHEST.defaultBlockState().setValue(ChestBlock.FACING, Direction.NORTH).setValue(ChestBlock.TYPE, ChestType.LEFT), 3);
                    level.setBlock(east, Blocks.CHEST.defaultBlockState().setValue(ChestBlock.FACING, Direction.NORTH).setValue(ChestBlock.TYPE, ChestType.RIGHT), 3);
                })
                .thenExecute(() -> {
                    BlockEntity be = cache.getChunkNow(a.x, a.z).getBlockEntities().get(canon);
                    check("a double chest across the border", be instanceof ChestBlockEntity && other.equals(StockAccess.chestPair(be)));
                    check("each half is its own inventory", canon.equals(Districts.canonicalInventory(level, canon))
                            && other.equals(Districts.canonicalInventory(level, other)));
                    check("both loaded, the chest reads whole", StockAccess.loaded(level, canon));
                    check("both loaded, the chest is industrial", ind().districts().industrialForTest(level, be));
                    whole[0] = Districts.describeInventory(level, canon, 0);
                    record(t, "described whole", whole[0]);
                    check("both loaded, the half is an inventory of 27 slots: " + whole[0], whole[0] != null
                            && whole[0].kind() == IndRecords.Inventory.KIND_ITEMS && whole[0].itemSlots() == 27);
                    District.Inventory farHalf = Districts.describeInventory(level, other, 0);
                    check("and so is the other half: " + farHalf, farHalf != null
                            && farHalf.kind() == IndRecords.Inventory.KIND_ITEMS && farHalf.itemSlots() == 27);
                    old[0] = be;
                    old[1] = cache.getChunkNow(n.x, n.z).getBlockEntities().get(other);
                    level.setChunkForced(a.x, a.z, false);
                    level.setChunkForced(n.x, n.z, false);
                })
                // Unloaded, not only without a visible holder: a holder still waiting in ChunkMap's pendingUnloads
                // comes back FULL, with its old block entities, at a new ticket.
                // old[0] is null when the step before failed: a failed thenExecute does not stop the sequence's next steps
                // in that tick, and an NPE here would crash the server and end the whole round.
                .thenWaitUntil(paced(() -> check("A and N unload (block entities removed, no chunk holder)", old[0] != null && old[0].isRemoved() && old[1].isRemoved()
                        && cache.chunkMap.getVisibleChunkIfPresent(a.toLong()) == null && cache.chunkMap.getVisibleChunkIfPresent(n.toLong()) == null)))
                .thenExecute(() -> cache.addRegionTicket(FULL_ONLY, a, 0, a))
                .thenWaitUntil(paced(() -> {
                    LevelChunk c = cache.getChunkNow(a.x, a.z);
                    check("A loads, a new chest", c != null && c.getBlockEntities().get(canon) instanceof ChestBlockEntity be && be != old[0]);
                }))
                .thenExecute(() -> {
                    check("N is away (level 34)", cache.getChunkNow(n.x, n.z) == null && !level.isLoaded(other));
                    cache.addRegionTicket(FULL_ONLY, n, 0, n);
                    cache.save(false);
                    check("N is due and still loading: Level.isLoaded passes, getChunkNow does not",
                            level.isLoaded(other) && cache.getChunkNow(n.x, n.z) == null);
                    BlockEntity be = cache.getChunkNow(a.x, a.z).getBlockEntities().get(canon);
                    boolean readable = StockAccess.loaded(level, canon);
                    boolean industrial = ind().districts().industrialForTest(level, be);
                    boolean waitedClassifying = cache.getChunkNow(n.x, n.z) != null;
                    District.Inventory got = Districts.describeInventory(level, canon, 0);
                    boolean waitedDescribing = cache.getChunkNow(n.x, n.z) != null;
                    IItemHandler items = StockAccess.items(level, canon);
                    boolean waitedReading = cache.getChunkNow(n.x, n.z) != null;
                    record(t, "readable, industrial, described, N FULL after classifying, describing and reading, while N loads",
                            readable + ", " + industrial + ", " + got + ", " + waitedClassifying + ", " + waitedDescribing + ", " + waitedReading);
                    check("the classifier did not wait for N's load", !waitedClassifying);
                    check("the description did not wait for N's load", !waitedDescribing);
                    check("the item handler did not wait for N's load", !waitedReading);
                    check("a half whose other half is in a chunk still loading reads whole (it reads none of it)", readable);
                    check("classified as when whole (industrial)", industrial);
                    check("described as when whole: " + got + " against " + whole[0], whole[0].equals(got));
                    check("read as its own 27 slots", items != null && items.getSlots() == 27);
                })
                .thenWaitUntil(paced(() -> check("N loads", cache.getChunkNow(n.x, n.z) != null)))
                .thenExecute(() -> {
                    check("N loaded, the chest reads whole again", StockAccess.loaded(level, canon));
                    District.Inventory got = Districts.describeInventory(level, canon, 0);
                    check("N loaded, described as before: " + got, whole[0].equals(got));
                    District.Inventory far = Districts.describeInventory(level, other, 0);
                    check("N loaded, its half is 27 slots: " + far, far != null && far.itemSlots() == 27);
                    level.setBlock(east, Blocks.AIR.defaultBlockState(), 3);
                    level.setBlock(west, Blocks.AIR.defaultBlockState(), 3);
                    cache.removeRegionTicket(FULL_ONLY, a, 0, a);
                    cache.removeRegionTicket(FULL_ONLY, n, 0, n);
                })
                .thenSucceed();
    }

    /**
     * A vanilla double chest across a chunk border is two inventories in one district (RP-215): the west half, a LEFT
     * chest at the last column of one chunk, and the east half, its RIGHT half in the first column of the next, both
     * placed by a player. Each is a boundary inventory of its own, 27 slots, named by its own position; the two chunks
     * are one district all the same, since the chunks would otherwise be two districts that each hold half of what
     * the chest holds. The pair's handler (before) made them one inventory of 54 slots at the lower half.
     */
    @GameTest(template = YARD, batch = "chest_pair_load_02", timeoutTicks = 400)
    public static void aSplitDoubleChestIsTwoInventoriesInOneDistrict(GameTestHelper helper) {
        String t = "aSplitDoubleChestIsTwoInventoriesInOneDistrict";
        fastConfig();
        forgetAll(helper);
        ServerLevel level = helper.getLevel();
        BlockPos west = inChunk(helper, 15, 8), east = west.east();
        // A chest seeds no district (DS-10, the minecraft namespace): a machine in the west half's chunk does, and has no inventory.
        BlockPos machine = inChunk(helper, 4, 8);
        IndustryTestKit.set(helper, machine, TestMachines.MACHINE.get().defaultBlockState());
        playerPlaced(helper, machine);
        IndustryTestKit.set(helper, west, Blocks.CHEST.defaultBlockState().setValue(ChestBlock.FACING, Direction.NORTH).setValue(ChestBlock.TYPE, ChestType.LEFT));
        IndustryTestKit.set(helper, east, Blocks.CHEST.defaultBlockState().setValue(ChestBlock.FACING, Direction.NORTH).setValue(ChestBlock.TYPE, ChestType.RIGHT));
        playerPlaced(helper, west);
        playerPlaced(helper, east);
        helper.startSequence()
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, west);
                    check("the west half's chunk is in a district", d != null);
                    check("the east half's chunk is in the same one", d == districtAt(helper, east));
                    record(t, "chunks, inventories", d.chunks.size() + ", " + d.inventories);
                    check("two chunks: " + d.chunks, d.chunks.size() == 2);
                    check("two inventories, one per half: " + d.inventories, d.inventories.size() == 2);
                    District.Inventory w = d.inventoryAt(west), e = d.inventoryAt(east);
                    check("each half is an inventory at its own position: " + d.inventories, w != null && e != null);
                    check("of 27 slots each: " + w + ", " + e, w.itemSlots() == 27 && e.itemSlots() == 27);
                    check("each half leads to itself", west.equals(Districts.canonicalInventory(level, west)) && east.equals(Districts.canonicalInventory(level, east)));
                })
                .thenExecute(() -> {
                    IndustryTestKit.set(helper, east, Blocks.AIR.defaultBlockState());
                    IndustryTestKit.set(helper, west, Blocks.AIR.defaultBlockState());
                    IndustryTestKit.set(helper, machine, Blocks.AIR.defaultBlockState());
                })
                .thenSucceed();
    }
}
