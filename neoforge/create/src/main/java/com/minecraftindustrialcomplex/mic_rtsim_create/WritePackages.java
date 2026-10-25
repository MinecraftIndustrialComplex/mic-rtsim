package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Goods;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.simibubi.create.Create;
import com.simibubi.create.content.logistics.box.PackageItem;
import com.simibubi.create.content.logistics.packagePort.postbox.PostboxBlockEntity;
import com.simibubi.create.content.trains.graph.EdgePointType;
import com.simibubi.create.content.trains.graph.TrackGraph;
import com.simibubi.create.content.trains.station.GlobalPackagePort;
import com.simibubi.create.content.trains.station.GlobalStation;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.neoforge.items.IItemHandlerModifiable;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;

/**
 * IND_WRITE_PACKAGE (docs/industry-boundary.md §4.3, F26, INDUSTRY.md §8.2): packages an unloaded
 * district's replay sends, written into its postbox as real Create packages ({@code PackageItem.containing},
 * {@code addAddress}), where a fetch-packages train takes them. Where Create's own mail transfer would put
 * them: the postbox's inventory when its position is loaded and holds a postbox, else the station port's
 * {@code offlineBuffer} (marked {@code primed}, so the postbox takes the buffer back at its next load,
 * as {@code runMailTransfer} does).
 *
 * <p><b>Once:</b> an offline buffer is in {@code create_tracks.dat} (the station's port), a file of its own:
 * the journal's applied set is not a dedup for it, since a crash between the two files' writes leaves the
 * buffer holding the packages and the journal not knowing the op, which is then sent again and written twice
 * (docs/replay-gate.md 9.6, K2). The port keeps its own rings of the ops written through it, saved in its
 * entry beside the buffer ({@link PortRing}, docs/choices-journal-barrier.md RP-206): an offline write is
 * refused when either ring holds the op, the buffer's or the loaded postbox's (whose inventory Create mirrors
 * into the buffer). A loaded postbox is saved with its chunk, so its chunk carries the COMMAND_RESULT until
 * the engine acknowledges it (the pending-record pattern, docs/boundary.md §3.1); a loaded write is refused
 * when the chunk carries the op, the journal knows it or the buffer's ring holds it (packages written offline
 * and taken into the postbox at its load). Without the rings' mixins the dedup is the journal's applied set,
 * and the chunk for a loaded postbox.
 * <b>Room:</b> packages go into free slots; what does not fit is logged and counted, and the command is
 * APPLIED if any package was written (the engine keeps its books as written, W10), FAILED if none was.
 */
public final class WritePackages {
    private WritePackages() {}

    static long written;
    static long overflow;
    static long failed;

    public static long written() {
        return written;
    }

    public static long overflow() {
        return overflow;
    }

    static boolean accept(Industry ind, ByteBuffer b, int at, int size, int n, long cmdSeq) {
        if (size < IndRecords.WritePackage.ENTRIES || n < 0 || IndRecords.WritePackage.size(n) > size) {
            if (size >= 24)
                ind.commandResult(cmdSeq, b.getLong(at + IndRecords.WritePackage.OP_ID), Obs.CommandResult.FAILED);
            return true;
        }
        long opId = b.getLong(at + IndRecords.WritePackage.OP_ID);
        boolean rings = PortRing.available(ind.server());
        if (!rings && ind.opApplied(opId)) {
            ind.commandResult(cmdSeq, opId, Obs.CommandResult.DUPLICATE_OP);
            return true;
        }
        District d = ind.district(b.getLong(at + IndRecords.WritePackage.DISTRICT));
        ServerLevel level = d == null ? ind.server().overworld() : ind.level(d.dim);
        BlockPos pos = new BlockPos(b.getInt(at + IndRecords.WritePackage.X), b.getInt(at + IndRecords.WritePackage.Y), b.getInt(at + IndRecords.WritePackage.Z));
        String address = ind.engineName(IndRecords.DICT_NAME, b.getInt(at + IndRecords.WritePackage.ADDRESS));
        int count = b.getInt(at + IndRecords.WritePackage.COUNT);
        // One package's contents, as stacks of at most their max size, nine to a package (PackageItem.SLOTS).
        List<ItemStack> stacks = new ArrayList<>();
        for (int i = 0; i < n; i++) {
            int e = at + IndRecords.WritePackage.ENTRIES + i * IndRecords.ENTRY16;
            Goods.Name good = Goods.parse(ind.engineName(IndRecords.DICT_GOOD, b.getInt(e)));
            long amount = b.getLong(e + 8);
            ItemStack proto = ind.goods().itemPrototype(good);
            if (proto.isEmpty() || amount <= 0)
                continue;
            int max = Math.max(1, proto.getMaxStackSize());
            for (long left = amount; left > 0; left -= max)
                stacks.add(proto.copyWithCount((int) Math.min(max, left)));
        }
        LevelChunk chunk = level == null ? null : level.getChunkSource().getChunkNow(pos.getX() >> 4, pos.getZ() >> 4);
        PostboxBlockEntity postbox = level != null && level.isLoaded(pos) && level.getBlockEntity(pos) instanceof PostboxBlockEntity pb ? pb : null;
        // The port is looked up for a loaded postbox too: its rings are the dedup of both paths.
        GlobalPackagePort port = level == null ? null : port(level, pos);
        boolean duplicate;
        if (postbox != null)
            duplicate = (chunk != null && ind.chunkCarriesOp(chunk, opId)) || (rings && (ind.opApplied(opId) || PortRing.wroteToBuffer(port, opId)));
        else
            duplicate = rings && (PortRing.wroteToBuffer(port, opId) || PortRing.wroteToPostbox(port, opId));
        if (duplicate) {
            ind.commandResult(cmdSeq, opId, Obs.CommandResult.DUPLICATE_OP);
            return true;
        }
        if (level == null || stacks.isEmpty() || count <= 0) {
            failed++;
            ind.commandResult(cmdSeq, opId, Obs.CommandResult.FAILED);
            return true;
        }
        IItemHandlerModifiable target = postbox != null ? postbox.inventory : port == null ? null : port.offlineBuffer;
        if (target == null) {
            failed++;
            ind.commandResult(cmdSeq, opId, Obs.CommandResult.FAILED);
            return true;
        }
        // A package whose contents need more than nine stacks is written as several boxes; a package is
        // written whole or not at all.
        int boxes = (stacks.size() + PackageItem.SLOTS - 1) / PackageItem.SLOTS;
        int done = 0;
        while (done < count && freeSlots(target) >= boxes) {
            for (int from = 0; from < stacks.size(); from += PackageItem.SLOTS) {
                List<ItemStack> part = new ArrayList<>();
                for (int i = from; i < Math.min(stacks.size(), from + PackageItem.SLOTS); i++)
                    part.add(stacks.get(i).copy());
                ItemStack box = PackageItem.containing(part);
                if (address != null && !address.isBlank())
                    PackageItem.addAddress(box, address);
                target.setStackInSlot(freeSlot(target), box);
            }
            done++;
        }
        written += done;
        if (done < count) {
            overflow += count - done;
            com.minecraftindustrialcomplex.mic_rtsim.MicRtsim.LOGGER.warn(
                    "[mic_rtsim_create] IND_WRITE_PACKAGE: {} of {} packages fitted in the postbox at {}; the rest were not written", done, count, pos);
        }
        if (done == 0) {
            failed++;
            ind.commandResult(cmdSeq, opId, Obs.CommandResult.FAILED);
            return true;
        }
        if (postbox != null) {
            postbox.setChanged(); // Create mirrors the inventory into the port's buffer, unprimed
            if (rings && port != null) {
                PortRing.record(port, opId, true);
                Create.RAILWAYS.markTracksDirty();
            }
            ind.commandResultInChunk(chunk, cmdSeq, opId, Obs.CommandResult.APPLIED);
        } else {
            port.primed = true; // the postbox takes the buffer back when it loads (restoreOfflineBuffer)
            if (rings)
                PortRing.record(port, opId, false);
            Create.RAILWAYS.markTracksDirty();
            ind.commandResult(cmdSeq, opId, Obs.CommandResult.APPLIED);
        }
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

    /** The station port of the postbox at {@code pos} in this level, from every track graph's stations. */
    static GlobalPackagePort port(ServerLevel level, BlockPos pos) {
        for (TrackGraph graph : Create.RAILWAYS.trackNetworks.values()) {
            for (GlobalStation st : graph.getPoints(EdgePointType.STATION)) {
                if (st.getBlockEntityDimension() != level.dimension())
                    continue;
                GlobalPackagePort p = st.connectedPorts.get(pos);
                if (p != null)
                    return p;
            }
        }
        return null;
    }
}
