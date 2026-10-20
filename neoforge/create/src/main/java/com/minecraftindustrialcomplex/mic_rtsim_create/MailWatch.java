package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.simibubi.create.content.logistics.box.PackageItem;
import com.simibubi.create.content.trains.station.GlobalPackagePort;
import com.simibubi.create.content.trains.station.GlobalStation;
import net.minecraft.core.BlockPos;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;

/**
 * Mail at unloaded districts (IND_PACKAGE, docs/industry-boundary.md §3.2, F26): Create's
 * {@code GlobalStation.runMailTransfer} swaps packages between a train's carriages and the station's
 * postboxes, and for a postbox whose chunk is not loaded it uses the port's {@code offlineBuffer}
 * (INDUSTRY.md §0). Those buffers are the unloaded district's boundary stock, so a change to them is
 * an IND_PACKAGE: packages in (dropped there) or out (fetched), with their contents and address.
 *
 * <p>Only unloaded districts: for a loaded one the train-side cargo diff is the port (H18), and an
 * IND_PACKAGE as well would count the mail twice. Hooked at the head and the tail of
 * {@code runMailTransfer} (two mixins, PATCHES.md).
 */
public final class MailWatch {
    private MailWatch() {}

    private record Snap(District district, int inventory, List<ItemStack> packages) {}

    /** The ports' buffers at the head of the transfer, by station. One transfer at a time (server thread). */
    private static final Map<GlobalStation, Map<BlockPos, Snap>> BEFORE = new IdentityHashMap<>();

    static long records;

    public static void before(GlobalStation station) {
        try {
            Industry ind = IndustryApi.current();
            if (ind == null || station.connectedPorts.isEmpty() || station.getBlockEntityDimension() == null)
                return;
            ResourceKey<Level> dim = station.getBlockEntityDimension();
            Map<BlockPos, Snap> snaps = new HashMap<>();
            for (Map.Entry<BlockPos, GlobalPackagePort> e : station.connectedPorts.entrySet()) {
                BlockPos pos = e.getKey();
                District d = ind.districtAt(dim, ChunkPos.asLong(pos.getX() >> 4, pos.getZ() >> 4));
                if (d == null || d.loadState() != IndRecords.LoadState.UNLOADED || e.getValue().offlineBuffer == null)
                    continue;
                District.Inventory inv = d.inventoryAt(pos);
                snaps.put(pos, new Snap(d, inv == null ? IndRecords.NO_INVENTORY : inv.index(), packages(e.getValue())));
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
            for (Map.Entry<BlockPos, Snap> e : snaps.entrySet()) {
                GlobalPackagePort port = station.connectedPorts.get(e.getKey());
                if (port == null)
                    continue;
                List<ItemStack> now = packages(port);
                List<ItemStack> gone = new ArrayList<>(e.getValue().packages);
                List<ItemStack> added = new ArrayList<>();
                for (ItemStack p : now) {
                    int i = indexOf(gone, p);
                    if (i >= 0)
                        gone.remove(i);
                    else
                        added.add(p);
                }
                send(ind, e.getValue(), IndRecords.Package.IN, added);
                send(ind, e.getValue(), IndRecords.Package.OUT, gone);
            }
        } catch (RuntimeException ex) {
            MicRtsim.LOGGER.error("[mic_rtsim_create] observing mail failed", ex);
        }
    }

    private static List<ItemStack> packages(GlobalPackagePort port) {
        List<ItemStack> out = new ArrayList<>();
        for (int s = 0; s < port.offlineBuffer.getSlots(); s++) {
            ItemStack st = port.offlineBuffer.getStackInSlot(s);
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
