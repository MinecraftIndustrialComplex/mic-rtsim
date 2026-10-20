package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.industry.Goods;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.minecraftindustrialcomplex.mic_rtsim.industry.StockAccess;
import com.simibubi.create.AllDataComponents;
import com.simibubi.create.content.logistics.box.PackageItem;
import com.simibubi.create.content.logistics.packagePort.PackagePortBlockEntity;
import com.simibubi.create.foundation.item.ItemHelper;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.neoforged.neoforge.items.IItemHandlerModifiable;
import net.neoforged.neoforge.items.ItemStackHandler;

import java.util.ArrayList;
import java.util.List;

/**
 * A postbox's or frogport's inventory as boundary stock (I3): its packages counted by their contents
 * ({@code PackageItem.getContents}), loose items as themselves. Removals take from inside the packages,
 * the last slot first, and rewrite each package's {@code create:package_contents}; a package emptied
 * this way is gone. Additions go in as new packages ({@code PackageItem.containing}) addressed to the
 * port's own filter, while it has a free slot.
 */
final class PostboxAccess implements StockAccess {
    private final ServerLevel level;
    private final BlockPos pos;

    PostboxAccess(ServerLevel level, BlockPos pos) {
        this.level = level;
        this.pos = pos;
    }

    private IItemHandlerModifiable inventory() {
        return level.getBlockEntity(pos) instanceof PackagePortBlockEntity port ? port.inventory : null;
    }

    @Override
    public void read(Industry.GoodsCounter out) {
        IItemHandlerModifiable inv = inventory();
        Industry ind = IndustryApi.current();
        if (inv == null || ind == null)
            return;
        StockAccess.readItems(ind, inv, out);
    }

    @Override
    public long apply(Goods.Name good, long delta) {
        IItemHandlerModifiable inv = inventory();
        Industry ind = IndustryApi.current();
        if (inv == null || ind == null || good.cls() != 'i' || delta == 0)
            return 0;
        ItemStack proto = ind.goods().itemPrototype(good);
        if (proto.isEmpty())
            return 0;
        if (delta < 0) {
            long want = -delta, got = 0;
            for (int s = inv.getSlots() - 1; s >= 0 && got < want; s--) {
                ItemStack st = inv.getStackInSlot(s);
                if (st.isEmpty())
                    continue;
                if (!PackageItem.isPackage(st)) {
                    if (ItemStack.isSameItemSameComponents(st, proto)) {
                        int take = (int) Math.min(want - got, st.getCount());
                        ItemStack copy = st.copy();
                        copy.shrink(take);
                        inv.setStackInSlot(s, copy);
                        got += take;
                    }
                    continue;
                }
                ItemStackHandler contents = PackageItem.getContents(st);
                boolean changed = false;
                boolean any = false;
                for (int i = contents.getSlots() - 1; i >= 0; i--) {
                    ItemStack in = contents.getStackInSlot(i);
                    if (got < want && !in.isEmpty() && ItemStack.isSameItemSameComponents(in, proto)) {
                        int take = (int) Math.min(want - got, in.getCount());
                        contents.extractItem(i, take, false);
                        got += take;
                        changed = true;
                    }
                    if (!contents.getStackInSlot(i).isEmpty())
                        any = true;
                }
                if (!changed)
                    continue;
                if (!any) {
                    inv.setStackInSlot(s, ItemStack.EMPTY);
                } else {
                    ItemStack copy = st.copy();
                    copy.set(AllDataComponents.PACKAGE_CONTENTS, ItemHelper.containerContentsFromHandler(contents));
                    inv.setStackInSlot(s, copy);
                }
            }
            return -got;
        }
        long put = 0;
        String address = level.getBlockEntity(pos) instanceof PackagePortBlockEntity port ? port.addressFilter : null;
        int perStack = Math.max(1, proto.getMaxStackSize());
        while (put < delta) {
            int free = -1;
            for (int s = 0; s < inv.getSlots(); s++)
                if (inv.getStackInSlot(s).isEmpty()) {
                    free = s;
                    break;
                }
            if (free < 0)
                break;
            List<ItemStack> stacks = new ArrayList<>();
            long inThis = 0;
            for (int k = 0; k < PackageItem.SLOTS && put + inThis < delta; k++) {
                int n = (int) Math.min(perStack, delta - put - inThis);
                stacks.add(proto.copyWithCount(n));
                inThis += n;
            }
            ItemStack box = PackageItem.containing(stacks);
            if (address != null && !address.isBlank())
                PackageItem.addAddress(box, address);
            inv.setStackInSlot(free, box);
            put += inThis;
        }
        return put;
    }
}
