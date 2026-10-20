package com.minecraftindustrialcomplex.mic_rtsim_endertransmission;

import com.forsteri.createendertransmission.blocks.AbstractMatterTransmitterBlockEntity;
import com.forsteri.createendertransmission.blocks.energyTransmitter.EnergyNetwork;
import com.forsteri.createendertransmission.blocks.energyTransmitter.EnergyTransmitterBlockEntity;
import com.forsteri.createendertransmission.transmitUtil.ITransmitter;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.BlockEntity;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import java.util.WeakHashMap;
import java.util.function.Consumer;

/**
 * Ender Transmission endpoints belong to one district (I13 as corrected: the pack uses Ender
 * Transmission only as a stop-gap for tightly linked machinery, never for long-distance transmission).
 * Every transmitter links to the other transmitters on its channel (the same kind: items, fluids or
 * rotation; the same channel number and password) in the same level, so the district detection joins
 * their chunks into one district, however far apart they are. The channel's buffer is then machinery
 * inside that district, like a pipe's contents: not stock, not a port, not sampled.
 *
 * <p>Item and fluid transmitters are found through a weak set the constructor mixin fills (Ender
 * Transmission keeps only the buffers, not the endpoints). Rotation transmitters are found in Ender
 * Transmission's own {@code EnergyNetwork.ENERGY} lists, read without changing them. A channel with
 * an endpoint in another level cannot join one district: the districts on it carry a diagnostics warning
 * (server log and operator /rtsim district output only, I18; what crosses it is not simulated as a transfer).
 */
public final class ChannelLinks {
    private ChannelLinks() {}

    static final int WARN_PERIOD = 200; // PLACEHOLDER
    static final String WARN_KEY = "endertransmission";

    /** Item and fluid transmitter block entities constructed this run, weakly. */
    private static final Set<AbstractMatterTransmitterBlockEntity<?>> TRANSMITTERS = Collections.newSetFromMap(new WeakHashMap<>());
    /** Districts warned at the last check. */
    private static final Set<Long> WARNED = new HashSet<>();
    /** Each transmitter's channel and password at the last check, weakly: a change is a new detection. */
    private static final Map<BlockEntity, String> SETTINGS = new WeakHashMap<>();

    public static void track(AbstractMatterTransmitterBlockEntity<?> be) {
        synchronized (TRANSMITTERS) {
            TRANSMITTERS.add(be);
        }
    }

    static boolean isTransmitter(BlockEntity be) {
        return be instanceof AbstractMatterTransmitterBlockEntity<?> || be instanceof EnergyTransmitterBlockEntity;
    }

    /** The other live transmitters on {@code be}'s channel, in any level. */
    static List<BlockEntity> peers(BlockEntity be) {
        List<BlockEntity> out = new ArrayList<>();
        if (!(be instanceof ITransmitter t))
            return out;
        int channel = t.getChannel();
        String password = t.getPassword();
        if (be instanceof EnergyTransmitterBlockEntity) {
            List<Map<String, List<KineticBlockEntity>>> channels = EnergyNetwork.ENERGY.channels;
            if (channel < 0 || channel >= channels.size())
                return out;
            List<KineticBlockEntity> list = channels.get(channel).get(password);
            if (list == null)
                return out;
            for (KineticBlockEntity other : List.copyOf(list))
                if (other != be && other instanceof EnergyTransmitterBlockEntity e && sameChannel(e, channel, password))
                    out.add(other);
            return out;
        }
        synchronized (TRANSMITTERS) {
            for (AbstractMatterTransmitterBlockEntity<?> other : TRANSMITTERS)
                if (other != be && other.getClass() == be.getClass() && sameChannel(other, channel, password))
                    out.add(other);
        }
        return out;
    }

    private static boolean sameChannel(BlockEntity other, int channel, String password) {
        return !other.isRemoved() && other.getLevel() != null && !other.getLevel().isClientSide
                && other instanceof ITransmitter t && t.getChannel() == channel && Objects.equals(t.getPassword(), password);
    }

    /** The district detection's links: the peers in the same level. */
    static void links(BlockEntity be, Consumer<BlockPos> out) {
        Level level = be.getLevel();
        if (level == null || level.isClientSide)
            return;
        for (BlockEntity other : peers(be))
            if (other.getLevel() == level)
                out.accept(other.getBlockPos());
    }

    /**
     * Every {@code WARN_PERIOD} ticks: a transmitter whose channel or password changed since the last check
     * marks its chunk for detection (the settings screen is no block edit), and districts whose channel
     * reaches another level get the diagnostics warning.
     */
    static void tick(Industry ind, long now) {
        if (now % WARN_PERIOD == 0)
            warnNow(ind);
    }

    /** Sets and clears the cross-dimension warnings now (GameTests call it too). */
    public static void warnNow(Industry ind) {
        List<BlockEntity> all = new ArrayList<>();
        synchronized (TRANSMITTERS) {
            all.addAll(TRANSMITTERS);
        }
        for (Map<String, List<KineticBlockEntity>> m : EnergyNetwork.ENERGY.channels)
            for (List<KineticBlockEntity> list : m.values())
                all.addAll(List.copyOf(list));
        Map<Long, String> warn = new HashMap<>();
        for (BlockEntity be : all) {
            if (be.isRemoved() || be.getLevel() == null || be.getLevel().isClientSide || !(be instanceof ITransmitter t))
                continue;
            String settings = t.getChannel() + "|" + t.getPassword();
            String before = SETTINGS.put(be, settings);
            if (before != null && !before.equals(settings))
                ind.districts().markDirty(be.getLevel().dimension(), new ChunkPos(be.getBlockPos()).toLong());
            District d = ind.districtAt(be.getLevel(), be.getBlockPos());
            if (d == null)
                continue;
            for (BlockEntity other : peers(be))
                if (other.getLevel() != be.getLevel()) {
                    warn.put(d.id, "Ender Transmission channel " + t.getChannel() + " reaches " + other.getLevel().dimension().location()
                            + ": what crosses it is not simulated");
                    break;
                }
        }
        for (long id : WARNED)
            if (!warn.containsKey(id))
                ind.diagnostics().warn(id, WARN_KEY, null);
        for (Map.Entry<Long, String> e : warn.entrySet())
            ind.diagnostics().warn(e.getKey(), WARN_KEY, e.getValue());
        WARNED.clear();
        WARNED.addAll(warn.keySet());
    }

    /** Forgets this run's warnings (GameTests). */
    public static void forget() {
        WARNED.clear();
        SETTINGS.clear();
    }
}
