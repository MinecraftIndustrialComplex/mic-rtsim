package com.minecraftindustrialcomplex.mic_rtsim.industry;

import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.ChestBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.ChestType;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.energy.IEnergyStorage;
import net.neoforged.neoforge.fluids.FluidStack;
import net.neoforged.neoforge.fluids.capability.IFluidHandler;
import net.neoforged.neoforge.items.IItemHandler;
import net.neoforged.neoforge.items.ItemHandlerHelper;

/**
 * Reads and writes one boundary inventory. The default ({@link #capabilities}) goes through NeoForge's
 * item, fluid and energy capabilities at the inventory's position (I3); plugins supply their own for
 * kinds like postboxes ({@link IndustryPlugin#access}).
 */
public interface StockAccess {

    /** Adds the inventory's contents to {@code out}. */
    void read(Industry.GoodsCounter out);

    /**
     * Applies {@code delta} of one good: a removal takes up to what is there, an addition puts in up
     * to what fits. Returns the amount applied, with the sign of {@code delta}.
     */
    long apply(Goods.Name good, long delta);

    /**
     * Whether the inventory at {@code pos} reads only loaded (FULL) chunks: its own chunk is loaded, and its
     * parts are ({@link #partsLoaded}). Loads nothing. The capability reads below go through
     * {@code Level.getBlockEntity}, which waits for a chunk still loading (R2-11): ask this first where a tick's
     * cost must stay bounded.
     */
    static boolean loaded(ServerLevel level, BlockPos pos) {
        LevelChunk chunk = level.getChunkSource().getChunkNow(pos.getX() >> 4, pos.getZ() >> 4);
        if (chunk == null)
            return false;
        BlockEntity be = chunk.getBlockEntities().get(pos);
        return be == null || partsLoaded(level, be);
    }

    /**
     * Whether reading {@code be}'s capabilities reads only loaded (FULL) chunks, {@code be}'s own chunk being
     * loaded (R2-11): a double chest's other half is (NeoForge's chest item handler is the pair's, and
     * {@code ChestBlock.getContainer} reads the other half through {@code Level.getBlockState}, which loads its
     * chunk, or waits for it while it loads), and no plugin says a part is not
     * ({@link IndustryPlugin#partsLoaded}). Loads nothing.
     */
    static boolean partsLoaded(ServerLevel level, BlockEntity be) {
        BlockPos other = chestPair(be);
        if (other != null && level.getChunkSource().getChunkNow(other.getX() >> 4, other.getZ() >> 4) == null)
            return false;
        for (IndustryPlugin plugin : IndustryApi.PLUGINS)
            if (!plugin.partsLoaded(level, be))
                return false;
        return true;
    }

    /** A double chest's other half (vanilla's {@code ChestBlock}, and a mod's that extends it), else null. */
    static BlockPos chestPair(BlockEntity be) {
        BlockState s = be.getBlockState();
        if (!(s.getBlock() instanceof ChestBlock) || !s.hasProperty(ChestBlock.TYPE) || s.getValue(ChestBlock.TYPE) == ChestType.SINGLE)
            return null;
        return be.getBlockPos().relative(ChestBlock.getConnectedDirection(s));
    }

    /** The item handler at a position, the unsided one first, else the first side that has one. */
    static IItemHandler items(ServerLevel level, BlockPos pos) {
        IItemHandler h = level.getCapability(Capabilities.ItemHandler.BLOCK, pos, null);
        if (h != null)
            return h;
        for (Direction d : Direction.values()) {
            h = level.getCapability(Capabilities.ItemHandler.BLOCK, pos, d);
            if (h != null)
                return h;
        }
        return null;
    }

    static IFluidHandler fluids(ServerLevel level, BlockPos pos) {
        IFluidHandler h = level.getCapability(Capabilities.FluidHandler.BLOCK, pos, null);
        if (h != null)
            return h;
        for (Direction d : Direction.values()) {
            h = level.getCapability(Capabilities.FluidHandler.BLOCK, pos, d);
            if (h != null)
                return h;
        }
        return null;
    }

    static IEnergyStorage energy(ServerLevel level, BlockPos pos) {
        IEnergyStorage h = level.getCapability(Capabilities.EnergyStorage.BLOCK, pos, null);
        if (h != null)
            return h;
        for (Direction d : Direction.values()) {
            h = level.getCapability(Capabilities.EnergyStorage.BLOCK, pos, d);
            if (h != null)
                return h;
        }
        return null;
    }

    /** The capability access for an inventory whose kind flags say which capabilities it has. */
    static StockAccess capabilities(Industry industry, ServerLevel level, District.Inventory inv) {
        return new StockAccess() {
            @Override
            public void read(Industry.GoodsCounter out) {
                if ((inv.kind() & IndRecords.Inventory.KIND_ITEMS) != 0) {
                    IItemHandler h = items(level, inv.pos());
                    if (h != null)
                        readItems(industry, h, out);
                }
                if ((inv.kind() & IndRecords.Inventory.KIND_FLUIDS) != 0) {
                    IFluidHandler h = fluids(level, inv.pos());
                    if (h != null) {
                        for (int t = 0, n = h.getTanks(); t < n; t++) {
                            FluidStack fs = h.getFluidInTank(t);
                            if (!fs.isEmpty())
                                out.add(industry.goods().name(fs), fs.getAmount());
                        }
                    }
                }
                if ((inv.kind() & IndRecords.Inventory.KIND_ENERGY) != 0) {
                    IEnergyStorage e = energy(level, inv.pos());
                    if (e != null && e.getEnergyStored() > 0)
                        out.add(Goods.ENERGY_FE, e.getEnergyStored());
                }
            }

            @Override
            public long apply(Goods.Name good, long delta) {
                return switch (good.cls()) {
                    case 'i' -> {
                        IItemHandler h = items(level, inv.pos());
                        yield h == null ? 0 : applyItems(industry, h, industry.goods().itemPrototype(good), delta);
                    }
                    case 'f' -> {
                        IFluidHandler h = fluids(level, inv.pos());
                        yield h == null ? 0 : applyFluid(h, industry.goods().fluidPrototype(good), delta);
                    }
                    case 'e' -> {
                        IEnergyStorage e = energy(level, inv.pos());
                        if (e == null)
                            yield 0;
                        int n = (int) Math.min(Integer.MAX_VALUE, Math.abs(delta));
                        yield delta < 0 ? -e.extractEnergy(n, false) : e.receiveEnergy(n, false);
                    }
                    default -> 0;
                };
            }
        };
    }

    /** Every slot of an item handler, container items (packages) expanded by the plugins. */
    static void readItems(Industry industry, IItemHandler h, Industry.GoodsCounter out) {
        for (int s = 0, n = h.getSlots(); s < n; s++) {
            ItemStack st = h.getStackInSlot(s);
            if (!st.isEmpty())
                industry.countStack(st, st.getCount(), out);
        }
    }

    /**
     * Removes or adds items of one good through a handler. Removals walk the slots from the last (so
     * an inventory filled from the front empties from the back) and take matching stacks; additions
     * insert stacked, one max-size stack at a time, until the handler refuses.
     */
    static long applyItems(Industry industry, IItemHandler h, ItemStack proto, long delta) {
        if (proto.isEmpty() || delta == 0)
            return 0;
        if (delta < 0) {
            long want = -delta, got = 0;
            for (int s = h.getSlots() - 1; s >= 0 && got < want; s--) {
                ItemStack st = h.getStackInSlot(s);
                if (st.isEmpty() || !ItemStack.isSameItemSameComponents(st, proto))
                    continue;
                int take = (int) Math.min(want - got, st.getCount());
                ItemStack out = h.extractItem(s, take, false);
                got += out.getCount();
            }
            return -got;
        }
        long put = 0;
        int max = Math.max(1, proto.getMaxStackSize());
        while (put < delta) {
            int n = (int) Math.min(max, delta - put);
            ItemStack rest = ItemHandlerHelper.insertItemStacked(h, proto.copyWithCount(n), false);
            int in = n - rest.getCount();
            put += in;
            if (in < n)
                break;
        }
        return put;
    }

    static long applyFluid(IFluidHandler h, FluidStack proto, long delta) {
        if (proto.isEmpty() || delta == 0)
            return 0;
        int n = (int) Math.min(Integer.MAX_VALUE, Math.abs(delta));
        if (delta < 0)
            return -h.drain(proto.copyWithAmount(n), IFluidHandler.FluidAction.EXECUTE).getAmount();
        return h.fill(proto.copyWithAmount(n), IFluidHandler.FluidAction.EXECUTE);
    }
}
