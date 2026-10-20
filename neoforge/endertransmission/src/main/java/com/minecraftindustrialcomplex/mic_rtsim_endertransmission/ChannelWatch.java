package com.minecraftindustrialcomplex.mic_rtsim_endertransmission;

import com.forsteri.createendertransmission.CreateEnderTransmission;
import com.forsteri.createendertransmission.blocks.AbstractMatterTransmitterBlockEntity;
import com.forsteri.createendertransmission.blocks.MatterTransmitterNetwork;
import com.forsteri.createendertransmission.blocks.fluidTrasmitter.FluidTransmitterBlockEntity;
import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Goods;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;
import net.neoforged.neoforge.common.util.INBTSerializable;
import net.neoforged.neoforge.fluids.FluidStack;
import net.neoforged.neoforge.fluids.capability.IFluidHandler;
import net.neoforged.neoforge.items.IItemHandler;
import net.neoforged.neoforge.items.IItemHandlerModifiable;

import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;
import java.util.WeakHashMap;

/**
 * Ender Transmission channels as shared stock (I13, INDUSTRY.md §8.6, F8). A channel is the triple
 * (network ITEM or FLUID, channel 0 to 9, password), and its buffer lives in a static map Ender
 * Transmission saves in {@code MatterWorldSavedData} (overworld data, available while no chunk is loaded):
 * one item slot, or one 1,000 mB tank.
 *
 * <ul>
 *   <li><b>Ports:</b> every insert into or extract from a transmitter in a district (not simulated) is an
 *       IND_PORT of kind 7 (channel), counterpart the channel's id, + into the district when it came out
 *       of the channel; summed per district and channel, flushed every {@code PORT_FLUSH} ticks.</li>
 *   <li><b>Samples:</b> IND_CHANNEL_SAMPLE every {@code SAMPLE_PERIOD} ticks for every channel a
 *       district's transmitter uses: its contents and capacity (an item slot holds what Ender
 *       Transmission's slot limit and the item's stack size allow).</li>
 *   <li><b>Deltas:</b> IND_CHANNEL_DELTA applied to the buffer at once, loaded or not; the buffer and the
 *       journal reach disk at the same save, so the journal's applied set is the dedup (as H17).</li>
 * </ul>
 * Channel ids: FNV-1a 64 over "network|channel|password", finalized with SplitMix64, never 0 (Q7).
 */
public final class ChannelWatch {
    private ChannelWatch() {}

    static final int PORT_FLUSH = 20;     // PLACEHOLDER
    static final int SAMPLE_PERIOD = 100; // PLACEHOLDER, the core's sweep period

    private record Key(long district, long channel) {}

    /** Transmitter block entities constructed this run, weakly. */
    private static final Set<AbstractMatterTransmitterBlockEntity<?>> TRANSMITTERS = Collections.newSetFromMap(new WeakHashMap<>());
    /** Ports not yet flushed, by district and channel: good name to amount (+ into the district). */
    private static final Map<Key, Map<String, Long>> PENDING = new HashMap<>();
    /** Channels a district's transmitter used this session. */
    private static final Set<Long> USED = new HashSet<>();

    static long portsSent;
    static long samplesSent;
    static long deltasApplied;

    public static long portsSent() {
        return portsSent;
    }

    public static long samplesSent() {
        return samplesSent;
    }

    public static void track(AbstractMatterTransmitterBlockEntity<?> be) {
        synchronized (TRANSMITTERS) {
            TRANSMITTERS.add(be);
        }
    }

    // ---- ids ------------------------------------------------------------------------------------

    public static long channelId(MatterTransmitterNetwork network, int channel, String password) {
        long x = 0xcbf29ce484222325L;
        for (byte b : (network.name() + "|" + channel + "|" + (password == null ? "" : password)).getBytes(StandardCharsets.UTF_8)) {
            x ^= (b & 0xFF);
            x *= 0x100000001b3L;
        }
        x += 0x9E3779B97F4A7C15L;
        x = (x ^ (x >>> 30)) * 0xBF58476D1CE4E5B9L;
        x = (x ^ (x >>> 27)) * 0x94D049BB133111EBL;
        x ^= x >>> 31;
        return x == 0 ? 1 : x;
    }

    /** The channel id of a transmitter, by the kind of its handler (items or fluids). */
    static long channelOf(AbstractMatterTransmitterBlockEntity<?> be, boolean fluids) {
        return channelId(fluids ? MatterTransmitterNetwork.FLUID : MatterTransmitterNetwork.ITEM, be.getChannel(), be.getPassword());
    }

    // ---- measuring handlers ---------------------------------------------------------------------

    /** Mixin: the handler a capability lookup returns, wrapped so its transfers are measured. */
    public static Object wrap(AbstractMatterTransmitterBlockEntity<?> be, Object handler) {
        if (handler instanceof IItemHandler items)
            return new MeasuredItems(be, items);
        if (handler instanceof IFluidHandler fluids)
            return new MeasuredFluids(be, fluids);
        return handler;
    }

    /** Books a transfer through a transmitter: + into the district (out of the channel). */
    static void moved(AbstractMatterTransmitterBlockEntity<?> be, boolean fluids, String good, long amount) {
        if (amount == 0 || good == null)
            return;
        Level level = be.getLevel();
        if (level == null || level.isClientSide)
            return;
        Industry ind = IndustryApi.current();
        if (ind == null)
            return;
        District d = ind.districtAt(level, be.getBlockPos());
        if (d == null)
            return;
        long ch = channelOf(be, fluids);
        USED.add(ch);
        PENDING.computeIfAbsent(new Key(d.id, ch), k -> new HashMap<>()).merge(good, amount, Long::sum);
    }

    private static final class MeasuredItems implements IItemHandlerModifiable {
        private final AbstractMatterTransmitterBlockEntity<?> be;
        private final IItemHandler inner;

        MeasuredItems(AbstractMatterTransmitterBlockEntity<?> be, IItemHandler inner) {
            this.be = be;
            this.inner = inner;
        }

        @Override
        public int getSlots() {
            return inner.getSlots();
        }

        @Override
        public ItemStack getStackInSlot(int slot) {
            return inner.getStackInSlot(slot);
        }

        @Override
        public ItemStack insertItem(int slot, ItemStack stack, boolean simulate) {
            int before = stack.getCount();
            ItemStack proto = stack.copyWithCount(1);
            ItemStack rest = inner.insertItem(slot, stack, simulate);
            if (!simulate) {
                Industry ind = IndustryApi.current();
                if (ind != null)
                    moved(be, false, ind.goods().name(proto), -(long) (before - rest.getCount()));
            }
            return rest;
        }

        @Override
        public ItemStack extractItem(int slot, int amount, boolean simulate) {
            ItemStack out = inner.extractItem(slot, amount, simulate);
            if (!simulate && !out.isEmpty()) {
                Industry ind = IndustryApi.current();
                if (ind != null)
                    moved(be, false, ind.goods().name(out), out.getCount());
            }
            return out;
        }

        @Override
        public int getSlotLimit(int slot) {
            return inner.getSlotLimit(slot);
        }

        @Override
        public boolean isItemValid(int slot, ItemStack stack) {
            return inner.isItemValid(slot, stack);
        }

        @Override
        public void setStackInSlot(int slot, ItemStack stack) {
            if (!(inner instanceof IItemHandlerModifiable m))
                return;
            ItemStack old = inner.getStackInSlot(slot).copy();
            m.setStackInSlot(slot, stack);
            Industry ind = IndustryApi.current();
            if (ind != null) {
                if (!old.isEmpty())
                    moved(be, false, ind.goods().name(old), old.getCount());
                if (!stack.isEmpty())
                    moved(be, false, ind.goods().name(stack), -stack.getCount());
            }
        }
    }

    private static final class MeasuredFluids implements IFluidHandler {
        private final AbstractMatterTransmitterBlockEntity<?> be;
        private final IFluidHandler inner;

        MeasuredFluids(AbstractMatterTransmitterBlockEntity<?> be, IFluidHandler inner) {
            this.be = be;
            this.inner = inner;
        }

        @Override
        public int getTanks() {
            return inner.getTanks();
        }

        @Override
        public FluidStack getFluidInTank(int tank) {
            return inner.getFluidInTank(tank);
        }

        @Override
        public int getTankCapacity(int tank) {
            return inner.getTankCapacity(tank);
        }

        @Override
        public boolean isFluidValid(int tank, FluidStack stack) {
            return inner.isFluidValid(tank, stack);
        }

        @Override
        public int fill(FluidStack resource, FluidAction action) {
            FluidStack proto = resource.copyWithAmount(1);
            int n = inner.fill(resource, action);
            if (action.execute() && n > 0) {
                Industry ind = IndustryApi.current();
                if (ind != null)
                    moved(be, true, ind.goods().name(proto), -(long) n);
            }
            return n;
        }

        @Override
        public FluidStack drain(FluidStack resource, FluidAction action) {
            FluidStack out = inner.drain(resource, action);
            booked(out, action);
            return out;
        }

        @Override
        public FluidStack drain(int maxDrain, FluidAction action) {
            FluidStack out = inner.drain(maxDrain, action);
            booked(out, action);
            return out;
        }

        private void booked(FluidStack out, FluidAction action) {
            if (action.execute() && !out.isEmpty()) {
                Industry ind = IndustryApi.current();
                if (ind != null)
                    moved(be, true, ind.goods().name(out), out.getAmount());
            }
        }
    }

    // ---- the tick: ports and samples ------------------------------------------------------------

    static void tick(Industry ind, long now) {
        if (now % PORT_FLUSH == 0)
            flush(ind);
        if (now % SAMPLE_PERIOD == 0)
            sample(ind, now);
    }

    /** Journals the pending channel ports now (GameTests call it too). */
    public static void flush(Industry ind) {
        if (PENDING.isEmpty())
            return;
        for (Map.Entry<Key, Map<String, Long>> e : PENDING.entrySet()) {
            District d = ind.district(e.getKey().district);
            if (d == null)
                continue;
            Industry.GoodsCounter goods = ind.counter();
            for (Map.Entry<String, Long> g : e.getValue().entrySet())
                goods.add(g.getKey(), g.getValue());
            if (!goods.isEmpty()) {
                ind.port(d, IndRecords.Port.CHANNEL, IndRecords.NO_INVENTORY, e.getKey().channel, 0, goods);
                portsSent++;
            }
        }
        PENDING.clear();
    }

    /** IND_CHANNEL_SAMPLE for every channel a district's transmitter uses (GameTests call it too). */
    public static void sample(Industry ind, long now) {
        synchronized (TRANSMITTERS) {
            for (AbstractMatterTransmitterBlockEntity<?> be : TRANSMITTERS) {
                if (be.isRemoved() || be.getLevel() == null || be.getLevel().isClientSide)
                    continue;
                if (ind.districtAt(be.getLevel(), be.getBlockPos()) == null)
                    continue;
                USED.add(channelOf(be, be instanceof FluidTransmitterBlockEntity));
            }
        }
        if (USED.isEmpty())
            return;
        for (MatterTransmitterNetwork net : MatterTransmitterNetwork.values()) {
            for (int ch = 0; ch < net.channels.size(); ch++) {
                for (Map.Entry<String, INBTSerializable<CompoundTag>> e : net.channels.get(ch).entrySet()) {
                    long id = channelId(net, ch, e.getKey());
                    if (!USED.contains(id))
                        continue;
                    Industry.GoodsCounter goods = ind.counter();
                    long capacity = contents(ind, e.getValue(), goods);
                    int[] ids = goods.nonZeroIds();
                    int at = ind.latest(IndRecords.IND_CHANNEL_SAMPLE, IndRecords.ChannelSample.size(ids.length), ids.length);
                    if (at < 0)
                        continue;
                    ByteBuffer b = ind.open();
                    b.putLong(at + IndRecords.ChannelSample.GAME_TIME, now);
                    b.putLong(at + IndRecords.ChannelSample.CHANNEL, id);
                    b.putLong(at + IndRecords.ChannelSample.CAPACITY, capacity);
                    for (int i = 0; i < ids.length; i++) {
                        int en = at + IndRecords.ChannelSample.ENTRIES + i * IndRecords.ENTRY16;
                        b.putInt(en, ids[i]);
                        b.putLong(en + 8, goods.get(ids[i]));
                    }
                    samplesSent++;
                }
            }
        }
    }

    /** A buffer's contents into {@code out}; returns its capacity (items or millibuckets). */
    static long contents(Industry ind, INBTSerializable<CompoundTag> buffer, Industry.GoodsCounter out) {
        if (buffer instanceof IItemHandler h) {
            long cap = 0;
            for (int s = 0; s < h.getSlots(); s++) {
                ItemStack st = h.getStackInSlot(s);
                if (!st.isEmpty()) {
                    out.add(ind.goods().name(st), st.getCount());
                    cap += Math.min(h.getSlotLimit(s), st.getMaxStackSize());
                } else {
                    cap += h.getSlotLimit(s);
                }
            }
            return cap;
        }
        if (buffer instanceof IFluidHandler f) {
            long cap = 0;
            for (int t = 0; t < f.getTanks(); t++) {
                FluidStack fs = f.getFluidInTank(t);
                if (!fs.isEmpty())
                    out.add(ind.goods().name(fs), fs.getAmount());
                cap += f.getTankCapacity(t);
            }
            return cap;
        }
        return 0;
    }

    /** The buffer of a channel id, or null when no channel has that id. */
    static INBTSerializable<CompoundTag> buffer(long id) {
        for (MatterTransmitterNetwork net : MatterTransmitterNetwork.values())
            for (int ch = 0; ch < net.channels.size(); ch++)
                for (Map.Entry<String, INBTSerializable<CompoundTag>> e : net.channels.get(ch).entrySet())
                    if (channelId(net, ch, e.getKey()) == id)
                        return e.getValue();
        return null;
    }

    // ---- IND_CHANNEL_DELTA ----------------------------------------------------------------------

    static boolean channelDelta(Industry ind, ByteBuffer b, int at, int size, int n, long cmdSeq) {
        if (size < IndRecords.ChannelDelta.ENTRIES || n < 0 || IndRecords.ChannelDelta.ENTRIES + n * IndRecords.ENTRY16 > size) {
            if (size >= 24)
                ind.commandResult(cmdSeq, b.getLong(at + IndRecords.ChannelDelta.OP_ID), Obs.CommandResult.FAILED);
            return true;
        }
        long opId = b.getLong(at + IndRecords.ChannelDelta.OP_ID);
        if (ind.opApplied(opId)) {
            ind.commandResult(cmdSeq, opId, Obs.CommandResult.DUPLICATE_OP);
            return true;
        }
        INBTSerializable<CompoundTag> buffer = buffer(b.getLong(at + IndRecords.ChannelDelta.CHANNEL));
        if (buffer == null) {
            ind.commandResult(cmdSeq, opId, Obs.CommandResult.FAILED);
            return true;
        }
        // Removals first, as in a reconcile.
        long requested = 0, applied = 0;
        for (int pass = 0; pass < 2; pass++) {
            for (int i = 0; i < n; i++) {
                int e = at + IndRecords.ChannelDelta.ENTRIES + i * IndRecords.ENTRY16;
                long delta = b.getLong(e + 8);
                if ((pass == 0) != (delta < 0))
                    continue;
                Goods.Name good = Goods.parse(ind.engineName(IndRecords.DICT_GOOD, b.getInt(e)));
                requested += delta;
                try {
                    applied += apply(ind, buffer, good, delta);
                } catch (RuntimeException ex) {
                    MicRtsim.LOGGER.error("[mic_rtsim_endertransmission] applying {} of {} to a channel failed", delta, good, ex);
                }
            }
        }
        if (CreateEnderTransmission.savedData != null)
            CreateEnderTransmission.savedData.setDirty();
        if (applied != requested)
            MicRtsim.LOGGER.info("[mic_rtsim_endertransmission] IND_CHANNEL_DELTA: {} of {} applied (the buffer is one slot or 1,000 mB)", applied, requested);
        deltasApplied++;
        ind.commandResult(cmdSeq, opId, Obs.CommandResult.APPLIED);
        return true;
    }

    private static long apply(Industry ind, INBTSerializable<CompoundTag> buffer, Goods.Name good, long delta) {
        if (good == null || delta == 0)
            return 0;
        if (buffer instanceof IItemHandler h && good.cls() == 'i') {
            ItemStack proto = ind.goods().itemPrototype(good);
            if (proto.isEmpty())
                return 0;
            long done = 0;
            if (delta > 0) {
                for (int s = 0; s < h.getSlots() && done < delta; s++) {
                    int want = (int) Math.min(delta - done, proto.getMaxStackSize());
                    ItemStack rest = h.insertItem(s, proto.copyWithCount(want), false);
                    done += want - rest.getCount();
                }
                return done;
            }
            for (int s = 0; s < h.getSlots() && done < -delta; s++) {
                if (!ItemStack.isSameItemSameComponents(h.getStackInSlot(s), proto))
                    continue;
                done += h.extractItem(s, (int) Math.min(-delta - done, Integer.MAX_VALUE), false).getCount();
            }
            return -done;
        }
        if (buffer instanceof IFluidHandler f && good.cls() == 'f') {
            FluidStack proto = ind.goods().fluidPrototype(good);
            if (proto.isEmpty())
                return 0;
            int amount = (int) Math.min(Integer.MAX_VALUE, Math.abs(delta));
            if (delta > 0)
                return f.fill(proto.copyWithAmount(amount), IFluidHandler.FluidAction.EXECUTE);
            return -f.drain(proto.copyWithAmount(amount), IFluidHandler.FluidAction.EXECUTE).getAmount();
        }
        return 0;
    }

    /** Forgets this run's state (GameTests). */
    public static void forget() {
        PENDING.clear();
        USED.clear();
    }
}
