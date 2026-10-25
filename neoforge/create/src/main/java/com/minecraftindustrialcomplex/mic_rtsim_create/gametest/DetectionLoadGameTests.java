package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Districts;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.StockAccess;
import com.minecraftindustrialcomplex.mic_rtsim_create.Loaded;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.logistics.vault.ItemVaultBlock;
import com.simibubi.create.content.logistics.vault.ItemVaultBlockEntity;
import com.simibubi.create.infrastructure.config.AllConfigs;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerChunkCache;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.TicketType;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.Comparator;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.YARD;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.check;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.paced;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.record;

/**
 * A district detection never waits for a chunk that is due but still loading (R2-11). The fan links' half is
 * {@code GlassFanLinkGameTests.aLinkNeverWaitsForAChunkStillLoading}; this is the inventories' half.
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class DetectionLoadGameTests {
    private DetectionLoadGameTests() {}

    /** A ticket that holds a chunk FULL (level 33), not block-ticking: its neighbours stay at 34, not loaded. */
    private static final TicketType<ChunkPos> FULL_ONLY = TicketType.create("mic_rtsim_create_detection_full_only", Comparator.comparingLong(ChunkPos::toLong));

    /**
     * Describing a vault never waits for a chunk of it still loading, and describes it as when it is whole
     * (R2-11, RG17). A two-block vault along x across the border of a far chunk A (its controller, A's east
     * edge) and the next chunk N (its other block). Both are let go until both have unloaded, so the controller
     * comes back as a new object without the handler it built; then A alone is held FULL, and N is at level 34. In
     * one tick N gets a FULL ticket and the distance manager's updates run ({@code save(false)}):
     * {@code Level.isLoaded} passes in N while N is not FULL, the state the first detection after a start met.
     * The detection's description of the vault, asked at once, must leave N not FULL (before the fix the
     * controller's handler read the block in N through {@code Level.getBlockEntity}, which waited for N's load
     * and ran every other chunk load due meanwhile) and must equal the description made with both loaded.
     */
    @GameTest(template = YARD, batch = "detection_load_01", timeoutTicks = 4000)
    public static void aVaultIsDescribedWithoutWaitingForAChunkStillLoading(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        ServerChunkCache cache = level.getChunkSource();
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        int ax = (o.getX() >> 4) + 6600, az = (o.getZ() >> 4) + 6600, nx = ax + 1, y = 180, z = az * 16 + 8;
        ChunkPos a = new ChunkPos(ax, az), n = new ChunkPos(nx, az);
        BlockPos controller = new BlockPos(ax * 16 + 15, y, z), part = controller.east();
        BlockState vault = AllBlocks.ITEM_VAULT.getDefaultState().setValue(ItemVaultBlock.HORIZONTAL_AXIS, Direction.Axis.X);
        int slots = 2 * AllConfigs.server().logistics.vaultCapacity.get();
        District.Inventory[] whole = new District.Inventory[1];
        BlockEntity[] old = new BlockEntity[2];
        helper.startSequence()
                .thenExecute(() -> {
                    level.setChunkForced(ax, az, true);
                    level.setChunkForced(nx, az, true);
                })
                .thenWaitUntil(paced(() -> check("A and N load", cache.getChunkNow(ax, az) != null && cache.getChunkNow(nx, az) != null)))
                .thenExecute(() -> {
                    level.setBlock(controller, vault, 3);
                    level.setBlock(part, vault, 3);
                })
                .thenWaitUntil(paced(() -> check("the vault forms across the border, its controller in A",
                        Loaded.blockEntity(level, controller) instanceof ItemVaultBlockEntity c && c.isController() && c.getHeight() == 2
                                && Loaded.blockEntity(level, part) instanceof ItemVaultBlockEntity p && controller.equals(p.getController()))))
                .thenExecute(() -> {
                    check("both loaded, the vault reads whole", StockAccess.loaded(level, controller));
                    whole[0] = Districts.describeInventory(level, controller, 0);
                    record("aVaultIsDescribedWithoutWaitingForAChunkStillLoading", "described whole", whole[0]);
                    check("both loaded, the vault is an inventory of " + slots + " slots: " + whole[0], whole[0] != null
                            && (whole[0].kind() & IndRecords.Inventory.KIND_ITEMS) != 0 && whole[0].itemSlots() == slots);
                    old[0] = Loaded.blockEntity(level, controller);
                    old[1] = Loaded.blockEntity(level, part);
                    level.setChunkForced(ax, az, false);
                    level.setChunkForced(nx, az, false);
                })
                // Unloaded, not only without a visible holder: a holder still waiting in ChunkMap's pendingUnloads
                // comes back FULL, with its old block entities (and the handler the vault built), at a new ticket.
                .thenWaitUntil(paced(() -> check("A and N unload (block entities removed, no chunk holder)", old[0].isRemoved() && old[1].isRemoved()
                        && cache.chunkMap.getVisibleChunkIfPresent(a.toLong()) == null && cache.chunkMap.getVisibleChunkIfPresent(n.toLong()) == null)))
                .thenExecute(() -> cache.addRegionTicket(FULL_ONLY, a, 0, a))
                .thenWaitUntil(paced(() -> check("A loads, a new controller", Loaded.blockEntity(level, controller) instanceof ItemVaultBlockEntity v && v != old[0])))
                .thenExecute(() -> {
                    check("N is away (level 34)", cache.getChunkNow(nx, az) == null && !level.isLoaded(part));
                    cache.addRegionTicket(FULL_ONLY, n, 0, n);
                    cache.save(false);
                    check("N is due and still loading: Level.isLoaded passes, getChunkNow does not",
                            level.isLoaded(part) && cache.getChunkNow(nx, az) == null);
                    boolean readable = StockAccess.loaded(level, controller);
                    District.Inventory got = Districts.describeInventory(level, controller, 0);
                    boolean waited = cache.getChunkNow(nx, az) != null;
                    record("aVaultIsDescribedWithoutWaitingForAChunkStillLoading", "readable, described and N FULL after the look while N loads",
                            readable + ", " + got + ", " + waited);
                    check("the description did not wait for N's load", !waited);
                    check("a vault with a block in a chunk still loading does not read whole (a sweep skips it)", !readable);
                    check("described as when whole: " + got + " against " + whole[0], whole[0].equals(got));
                })
                .thenWaitUntil(paced(() -> check("N loads", cache.getChunkNow(nx, az) != null)))
                .thenExecute(() -> {
                    check("N loaded, the vault reads whole again", StockAccess.loaded(level, controller));
                    District.Inventory got = Districts.describeInventory(level, controller, 0);
                    check("N loaded, described through the capability as before: " + got, whole[0].equals(got));
                    level.setBlock(part, Blocks.AIR.defaultBlockState(), 3);
                    level.setBlock(controller, Blocks.AIR.defaultBlockState(), 3);
                    cache.removeRegionTicket(FULL_ONLY, a, 0, a);
                    cache.removeRegionTicket(FULL_ONLY, n, 0, n);
                })
                .thenSucceed();
    }
}
