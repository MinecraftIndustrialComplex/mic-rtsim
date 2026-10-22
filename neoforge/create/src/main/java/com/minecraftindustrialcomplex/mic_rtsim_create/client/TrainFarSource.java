package com.minecraftindustrialcomplex.mic_rtsim_create.client;

import com.minecraftindustrialcomplex.mic_rtsim.far.FarClock;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKey;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarMover;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarObjects;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.simibubi.create.compat.trainmap.TrainMapSync;
import com.simibubi.create.compat.trainmap.TrainMapSyncClient;
import com.simibubi.create.content.trains.entity.CarriageContraptionEntity;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashSet;
import java.util.IdentityHashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import net.createmod.catnip.animation.AnimationTickHolder;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.Level;
import net.neoforged.neoforge.client.event.ClientPlayerNetworkEvent;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.event.entity.EntityJoinLevelEvent;
import net.neoforged.neoforge.event.entity.EntityLeaveLevelEvent;

/**
 * Create trains for the far-object registry, phase 1 (research/distant-horizons-2026-10-21.md §4.1, §13).
 *
 * <p>Create already ships every train's carriage positions to any client that asks
 * ({@code com.simibubi.create.compat.trainmap}, for full-screen map mods). While a far renderer wants movers
 * ({@link FarObjects#wantedRadius()} above 0), this asks: {@link TrainMapSyncClient#requestData()} every client
 * tick, which Create turns into one request every 5 ticks; the server then sends all trains every 5 ticks to
 * requesters only. Each new packet is copied into the registry: one {@link FarKind#CARRIAGE} per carriage in
 * the client's level and within the wanted radius, moving from Create's previous sample to its newest over the
 * 5 ticks after the packet came (Create's own map does the same). When nothing wants movers it stops asking
 * ({@link TrainMapSyncClient#stopRequesting()}), and the server forgets the client 20 ticks later.
 *
 * <p>A carriage whose real entity is on the client is marked {@link FarMover#real()}, so a renderer hides it
 * (§5). No server change and no new payload: this reads Create's sync only.
 */
public final class TrainFarSource {
    public static final String SOURCE = MicRtsimCreate.MODID + ":trains";
    /** A carriage far away: dark grey, like the shadowed side of a train (DHT choices). */
    public static final int COLOUR = 0x3A3B3F;
    /** Create's light packet interval: a sample moves to the next over this many ticks. */
    private static final double INTERVAL_TICKS = TrainMapSync.lightPacketInterval;
    /** Carriages a little past the wanted radius are kept, so they do not pop in at its edge. */
    private static final double RADIUS_MARGIN = 64.0;

    private static final Set<CarriageContraptionEntity> PRESENT = Collections.newSetFromMap(new IdentityHashMap<>());
    private static boolean requesting;
    private static double seenPacket = Double.NaN;
    private static double packetSampleTick;
    private static boolean presenceChanged;

    private TrainFarSource() {}

    static void onClientTick(ClientTickEvent.Post event) {
        Minecraft mc = Minecraft.getInstance();
        ClientLevel level = mc.level;
        double radius = FarObjects.wantedRadius();
        if (level == null || mc.player == null || radius <= 0.0) {
            stop();
            return;
        }
        TrainMapSyncClient.requestData();
        requesting = true;

        double packet = TrainMapSyncClient.lastPacket;
        boolean newPacket = packet != seenPacket;
        if (!newPacket && !presenceChanged)
            return;
        if (newPacket) {
            // How long ago the packet came, in Create's clock, moved onto the far registry's clock
            double age = AnimationTickHolder.getTicks() + AnimationTickHolder.getPartialTicks() - packet;
            packetSampleTick = FarClock.ticks() - Math.max(0.0, Math.min(INTERVAL_TICKS, age));
            seenPacket = packet;
        }
        presenceChanged = false;
        copy(level, mc.player.getX(), mc.player.getZ(), radius + RADIUS_MARGIN);
    }

    /** Copies Create's current train data for {@code level} into the registry. */
    private static void copy(ClientLevel level, double px, double pz, double radius) {
        ResourceKey<Level> dim = level.dimension();
        String dimension = dim.location().toString();
        Set<FarKey> real = realCarriages(level);
        double radiusSq = radius * radius;
        List<FarMover> out = new ArrayList<>();
        for (Map.Entry<UUID, TrainMapSync.TrainMapSyncEntry> e : TrainMapSyncClient.currentData.entrySet()) {
            UUID train = e.getKey();
            TrainMapSync.TrainMapSyncEntry t = e.getValue();
            Float[] cur = t.positions;
            List<ResourceKey<Level>> dims = t.dimensions;
            if (cur == null || dims == null)
                continue;
            Float[] prev = t.prevPositions == null || t.prevPositions.length != cur.length ? cur : t.prevPositions;
            int carriages = Math.min(cur.length / 6, dims.size());
            for (int i = 0; i < carriages; i++) {
                // Create sends no position for a carriage between dimensions (null), and zeros with it
                if (!dim.equals(dims.get(i)))
                    continue;
                double[] c = read(cur, i), p = read(prev, i);
                if (c == null || p == null)
                    continue;
                double mx = (c[0] + c[3]) / 2.0 - px, mz = (c[2] + c[5]) / 2.0 - pz;
                if (mx * mx + mz * mz > radiusSq)
                    continue;
                FarKey key = new FarKey(SOURCE, train, i);
                out.add(new FarMover(key, FarKind.CARRIAGE, dimension, p, c, packetSampleTick, INTERVAL_TICKS,
                        COLOUR, real.contains(key)));
            }
        }
        FarObjects.commit(SOURCE, out);
    }

    private static double[] read(Float[] a, int carriage) {
        double[] out = new double[6];
        for (int k = 0; k < 6; k++) {
            Float f = a[carriage * 6 + k];
            if (f == null || !Float.isFinite(f))
                return null;
            out[k] = f;
        }
        return out;
    }

    /** The carriages whose entities the client has in {@code level}. */
    private static Set<FarKey> realCarriages(ClientLevel level) {
        Set<FarKey> keys = new HashSet<>();
        Iterator<CarriageContraptionEntity> it = PRESENT.iterator();
        while (it.hasNext()) {
            CarriageContraptionEntity cce = it.next();
            if (cce.isRemoved() || cce.level() != level) {
                it.remove();
                continue;
            }
            if (cce.trainId != null)
                keys.add(new FarKey(SOURCE, cce.trainId, cce.carriageIndex));
        }
        return keys;
    }

    static void onJoin(EntityJoinLevelEvent event) {
        if (event.getLevel().isClientSide() && event.getEntity() instanceof CarriageContraptionEntity cce) {
            PRESENT.add(cce);
            presenceChanged = true;
        }
    }

    static void onLeave(EntityLeaveLevelEvent event) {
        if (event.getLevel().isClientSide() && event.getEntity() instanceof CarriageContraptionEntity cce) {
            PRESENT.remove(cce);
            presenceChanged = true;
        }
    }

    static void onLoggingOut(ClientPlayerNetworkEvent.LoggingOut event) {
        PRESENT.clear();
        stop();
    }

    /** Stops asking Create for train data, once, and takes this source's movers out of the registry. */
    private static void stop() {
        if (!requesting)
            return;
        requesting = false;
        seenPacket = Double.NaN;
        // Clears Create's client map too; only full-screen map mods read it, and the pack has none
        TrainMapSyncClient.stopRequesting();
        FarObjects.clear(SOURCE);
    }
}
