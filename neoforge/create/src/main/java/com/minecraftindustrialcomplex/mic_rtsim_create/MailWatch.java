package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.simibubi.create.content.logistics.box.PackageItem;
import com.simibubi.create.content.logistics.packagePort.postbox.PostboxBlockEntity;
import com.simibubi.create.content.trains.entity.Train;
import com.simibubi.create.content.trains.station.GlobalPackagePort;
import com.simibubi.create.content.trains.station.GlobalStation;
import net.minecraft.core.BlockPos;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.neoforged.neoforge.items.IItemHandler;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;

/**
 * Train mail at district postboxes (IND_PACKAGE, docs/industry-boundary.md §3.2, F26): Create's
 * {@code GlobalStation.runMailTransfer} swaps packages between the train at the station and its
 * postboxes, using a postbox's inventory when its position is loaded and the port's {@code offlineBuffer}
 * when not (INDUSTRY.md §0). Every change to either, at a postbox in a district, is an IND_PACKAGE for
 * that district: packages in (dropped there) or out (fetched), with their contents and address.
 *
 * <ul>
 *   <li><b>Unloaded districts</b> (H30): the offline buffer is their boundary stock; the engine moves its
 *       replay's stock and base together.</li>
 *   <li><b>Loaded districts</b> (choice Q6): an ordinary port, so the engine learns the mail rates of a
 *       loaded district's packagers (F26). The mail the train moved at the station is taken out of the
 *       train's cargo port ({@link TrainWatch#mailMoved}), so it is counted once: by the postbox's own
 *       district, not by the station's.</li>
 * </ul>
 * Hooked at the head and the tail of {@code runMailTransfer} (two mixins, PATCHES.md).
 */
public final class MailWatch {
    private MailWatch() {}

    private record Snap(District district, int inventory, List<ItemStack> packages) {}

    /** The ports' handlers at the head of the transfer, by station. One transfer at a time (server thread). */
    private static final Map<GlobalStation, Map<BlockPos, Snap>> BEFORE = new IdentityHashMap<>();

    static long records;

    /** The handler {@code runMailTransfer} uses for a port: the loaded postbox's inventory, else the offline buffer. */
    static IItemHandler handler(ServerLevel level, BlockPos pos, GlobalPackagePort port) {
        if (level != null && level.isLoaded(pos) && level.getBlockEntity(pos) instanceof PostboxBlockEntity pb)
            return pb.inventory;
        return port.offlineBuffer;
    }

    public static void before(GlobalStation station) {
        try {
            Industry ind = IndustryApi.current();
            if (ind == null || station.connectedPorts.isEmpty() || station.getBlockEntityDimension() == null)
                return;
            ResourceKey<Level> dim = station.getBlockEntityDimension();
            ServerLevel level = ind.level(dim);
            Map<BlockPos, Snap> snaps = new HashMap<>();
            for (Map.Entry<BlockPos, GlobalPackagePort> e : station.connectedPorts.entrySet()) {
                BlockPos pos = e.getKey();
                IItemHandler h = handler(level, pos, e.getValue());
                if (h == null)
                    continue;
                District d = ind.districtAt(dim, ChunkPos.asLong(pos.getX() >> 4, pos.getZ() >> 4));
                District.Inventory inv = d == null ? null : d.inventoryAt(pos);
                snaps.put(pos, new Snap(d, inv == null ? IndRecords.NO_INVENTORY : inv.index(), packages(h)));
            }
            if (!snaps.isEmpty())
                BEFORE.put(station, snaps);
        } catch (RuntimeException ex) {
            MicRtsim.LOGGER.error("[mic_rtsim_create] observing mail failed", ex);
        }
    }

    public static void after(GlobalStation station) {
        Map<BlockPos, Snap> snaps = BEFORE.remove(station);
        if (snaps == null)
            return;
        try {
            Industry ind = IndustryApi.current();
            if (ind == null)
                return;
            ServerLevel level = ind.level(station.getBlockEntityDimension());
            Industry.GoodsCounter intoPostboxes = ind.counter();
            for (Map.Entry<BlockPos, Snap> e : snaps.entrySet()) {
                GlobalPackagePort port = station.connectedPorts.get(e.getKey());
                if (port == null)
                    continue;
                IItemHandler h = handler(level, e.getKey(), port);
                if (h == null)
                    continue;
                List<ItemStack> now = packages(h);
                List<ItemStack> gone = new ArrayList<>(e.getValue().packages);
                List<ItemStack> added = new ArrayList<>();
                for (ItemStack p : now) {
                    int i = indexOf(gone, p);
                    if (i >= 0)
                        gone.remove(i);
                    else
                        added.add(p);
                }
                for (ItemStack p : added)
                    ind.countStack(p, p.getCount(), intoPostboxes);
                for (ItemStack p : gone) {
                    Industry.GoodsCounter g = ind.counter();
                    ind.countStack(p, p.getCount(), g);
                    intoPostboxes.addAll(g, -1);
                }
                if (e.getValue().district != null) {
                    send(ind, e.getValue(), IndRecords.Package.IN, added);
                    send(ind, e.getValue(), IndRecords.Package.OUT, gone);
                }
            }
            Train train = station.getPresentTrain();
            if (train != null && !intoPostboxes.isEmpty())
                TrainWatch.mailMoved(train, intoPostboxes);
        } catch (RuntimeException ex) {
            MicRtsim.LOGGER.error("[mic_rtsim_create] observing mail failed", ex);
        }
    }

    private static List<ItemStack> packages(IItemHandler h) {
        List<ItemStack> out = new ArrayList<>();
        for (int s = 0; s < h.getSlots(); s++) {
            ItemStack st = h.getStackInSlot(s);
            if (!st.isEmpty())
                out.add(st.copy());
        }
        return out;
    }

    private static int indexOf(List<ItemStack> list, ItemStack p) {
        for (int i = 0; i < list.size(); i++)
            if (ItemStack.matches(list.get(i), p))
                return i;
        return -1;
    }

    /** One IND_PACKAGE per (direction, address): the packages' count and their contents. */
    private static void send(Industry ind, Snap snap, int direction, List<ItemStack> packages) {
        if (packages.isEmpty())
            return;
        Map<String, List<ItemStack>> byAddress = new HashMap<>();
        for (ItemStack p : packages)
            byAddress.computeIfAbsent(PackageItem.isPackage(p) ? PackageItem.getAddress(p) : "", k -> new ArrayList<>()).add(p);
        for (Map.Entry<String, List<ItemStack>> e : byAddress.entrySet()) {
            Industry.GoodsCounter goods = ind.counter();
            for (ItemStack p : e.getValue())
                ind.countStack(p, p.getCount(), goods);
            int[] ids = goods.nonZeroIds();
            ByteBuffer r = ind.journal(IndRecords.IND_PACKAGE, IndRecords.Package.size(ids.length), ids.length);
            r.putLong(IndRecords.Package.DISTRICT, snap.district.id);
            r.putInt(IndRecords.Package.INVENTORY, snap.inventory);
            r.putInt(IndRecords.Package.DIRECTION, direction);
            r.putInt(IndRecords.Package.ADDRESS, ind.nameId(e.getKey()));
            r.putInt(IndRecords.Package.PACKAGES, e.getValue().size());
            for (int i = 0; i < ids.length; i++) {
                int at = IndRecords.Package.ENTRIES + i * IndRecords.ENTRY16;
                r.putInt(at, ids[i]);
                r.putLong(at + 8, goods.get(ids[i]));
            }
            ind.commit(r);
            records++;
        }
    }
}
