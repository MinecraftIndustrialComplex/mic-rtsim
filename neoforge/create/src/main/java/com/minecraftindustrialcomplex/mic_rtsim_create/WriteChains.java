package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Goods;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.simibubi.create.content.logistics.box.PackageItem;
import com.simibubi.create.content.logistics.packagePort.frogport.FrogportBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.neoforge.items.IItemHandlerModifiable;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;

/**
 * IND_WRITE_CHAIN (SECTIONS.md §2.5, slice S4b, choice SC113): packages that replays sent along a package
 * store's chain toward a frogport that is loaded, written into the frogport's inventory as real Create
 * packages addressed to its own filter, where Create takes them as it takes a package that came off the
 * chain: {@code tryPushingToAdjacentInventories} moves them into the inventory below, and
 * {@code tryPullingFrom} never sends a package back out whose address matches the filter.
 *
 * <p><b>The books.</b> What was written is one IND_CHAIN_PACKAGE {@code CHAIN_IN} with the engine's op id
 * (its {@code OP_ID} field): the engine takes it from the pool a cluster holds toward the frogport, then
 * from what the store owes it. Nothing written, nothing journaled, and the command is FAILED: the engine
 * still owes it and writes again later. <b>Once:</b> a frogport is saved with its chunk, so the chunk
 * carries the {@code CHAIN_IN} and the COMMAND_RESULT until the engine acknowledges them
 * ({@link Industry#commandResultInChunk(LevelChunk, long, long, int, List)}); a later session journals
 * both again at the chunk's load, and the engine's Replay takes each op once.
 * <b>Room:</b> whole packages into the frogport's free slots (18), one kept free: a package Create is
 * catching lands with {@code insertItem} at the end of its animation and drops into the world when the
 * inventory is full. What does not fit is not written.
 */
public final class WriteChains {
    private WriteChains() {}

    static long written;
    static long failed;

    /** Packages written, for tests. */
    public static long written() {
        return written;
    }

    static boolean accept(Industry ind, ByteBuffer b, int at, int size, int n, long cmdSeq) {
        if (size < IndRecords.WriteChain.ENTRIES || n < 0 || IndRecords.WriteChain.size(n) > size) {
            if (size >= IndRecords.WriteChain.OP_ID + 8)
                ind.commandResult(cmdSeq, b.getLong(at + IndRecords.WriteChain.OP_ID), Obs.CommandResult.FAILED);
            return true;
        }
        long opId = b.getLong(at + IndRecords.WriteChain.OP_ID);
        if (ind.opApplied(opId)) {
            ind.commandResult(cmdSeq, opId, Obs.CommandResult.DUPLICATE_OP);
            return true;
        }
        PackageStores ps = PackageStores.of(ind);
        PackageStores.Store s = ps.store(b.getLong(at + IndRecords.WriteChain.STORE));
        ServerLevel level = s == null ? null : ind.level(s.dim);
        BlockPos pos = new BlockPos(b.getInt(at + IndRecords.WriteChain.X), b.getInt(at + IndRecords.WriteChain.Y), b.getInt(at + IndRecords.WriteChain.Z));
        int index = s == null ? -1 : s.indexOf(pos);
        int count = b.getInt(at + IndRecords.WriteChain.COUNT);
        LevelChunk chunk = level == null || !level.isLoaded(pos) ? null : level.getChunkSource().getChunkNow(pos.getX() >> 4, pos.getZ() >> 4);
        FrogportBlockEntity f = chunk != null && Loaded.blockEntity(level, pos) instanceof FrogportBlockEntity fp ? fp : null;
        if (f != null && ind.chunkCarriesOp(chunk, opId)) {
            ind.commandResult(cmdSeq, opId, Obs.CommandResult.DUPLICATE_OP);
            return true;
        }
        String filter = f == null ? null : f.getFilterString();
        // One package's contents, as stacks of at most their max size, nine to a package (PackageItem.SLOTS).
        List<ItemStack> stacks = new ArrayList<>();
        for (int i = 0; i < n; i++) {
            int e = at + IndRecords.WriteChain.ENTRIES + i * IndRecords.ENTRY16;
            Goods.Name good = Goods.parse(ind.engineName(IndRecords.DICT_GOOD, b.getInt(e)));
            long amount = b.getLong(e + 8);
            ItemStack proto = good == null ? ItemStack.EMPTY : ind.goods().itemPrototype(good);
            if (proto.isEmpty() || amount <= 0)
                continue;
            int max = Math.max(1, proto.getMaxStackSize());
            for (long left = amount; left > 0; left -= max)
                stacks.add(proto.copyWithCount((int) Math.min(max, left)));
        }
        // A frogport that takes no packages (no filter) would send them straight back out on the chain.
        if (f == null || index < 0 || filter == null || stacks.isEmpty() || count <= 0) {
            failed++;
            ind.commandResult(cmdSeq, opId, Obs.CommandResult.FAILED);
            return true;
        }
        IItemHandlerModifiable target = f.inventory;
        int boxes = (stacks.size() + PackageItem.SLOTS - 1) / PackageItem.SLOTS;
        Industry.GoodsCounter goods = ind.counter();
        int done = 0;
        while (done < count && freeSlots(target) > boxes) {
            for (int from = 0; from < stacks.size(); from += PackageItem.SLOTS) {
                List<ItemStack> part = new ArrayList<>();
                for (int i = from; i < Math.min(stacks.size(), from + PackageItem.SLOTS); i++)
                    part.add(stacks.get(i).copy());
                ItemStack box = PackageItem.containing(part);
                if (!filter.isBlank())
                    PackageItem.addAddress(box, filter);
                ind.countStack(box, 1, goods);
                target.setStackInSlot(freeSlot(target), box);
            }
            done++;
        }
        if (done == 0) {
            failed++;
            ind.commandResult(cmdSeq, opId, Obs.CommandResult.FAILED);
            return true;
        }
        written += done;
        f.setChanged();
        District d = ind.districtAt(level, pos);
        District.Inventory inv = d == null ? null : d.inventoryAt(pos);
        ByteBuffer in = PackageStores.journalChain(ind, s, index, pos, d, inv, IndRecords.ChainPackage.CHAIN_IN, index, done, filter, goods, opId);
        ind.commandResultInChunk(chunk, cmdSeq, opId, Obs.CommandResult.APPLIED, List.of(in));
        return true;
    }

    private static int freeSlots(IItemHandlerModifiable h) {
        int n = 0;
        for (int s = 0; s < h.getSlots(); s++)
            if (h.getStackInSlot(s).isEmpty())
                n++;
        return n;
    }

    private static int freeSlot(IItemHandlerModifiable h) {
        for (int s = 0; s < h.getSlots(); s++)
            if (h.getStackInSlot(s).isEmpty())
                return s;
        return -1;
    }
}
