package com.minecraftindustrialcomplex.mic_rtsim_create.client;

import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.simibubi.create.compat.trainmap.TrainMapSync;
import com.simibubi.create.compat.trainmap.TrainMapSyncClient;
import com.simibubi.create.compat.trainmap.TrainMapSyncPacket;
import io.netty.buffer.Unpooled;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.UUID;
import net.createmod.catnip.data.Pair;
import net.minecraft.network.FriendlyByteBuf;
import net.neoforged.neoforge.client.event.ClientTickEvent;

/**
 * The far-view bench's bandwidth meter (docs/choices-dh-trains.md), on only with the bench: each time Create's
 * train map sync brings a new packet, it encodes Create's client copy of it again with Create's own codec and
 * counts the bytes. Single player's connection is in memory, so this is how the bench sees what a server would
 * send. The client copy holds every field the server encoded, with the same values (the light packets' names
 * are the full packets' names), so the count is the packet body exactly; the payload id, the length prefix and
 * compression are not in it.
 */
final class TrainSyncMeter {
    private static final int LOG_EVERY = 100;

    private static double seen = Double.NaN;
    private static int ticks, firstTick = -1, lastTick;
    private static long packets, bytes;
    private static int lastBytes, lastTrains, lastCarriages;

    private TrainSyncMeter() {}

    static void onClientTick(ClientTickEvent.Post event) {
        ticks++;
        double packet = TrainMapSyncClient.lastPacket;
        if (packet != seen && !TrainMapSyncClient.currentData.isEmpty()) {
            seen = packet;
            measure();
        }
        if (ticks % LOG_EVERY == 0 && packets > 0) {
            double seconds = Math.max(1, lastTick - firstTick) / 20.0;
            MicRtsimCreate.LOGGER.info(String.format(Locale.ROOT,
                    "far bench sync: %d trains, %d carriages; last packet %d bytes; %d packets, %.1f bytes each, "
                            + "%.2f packets/s, %.0f bytes/s",
                    lastTrains, lastCarriages, lastBytes, packets, bytes / (double) packets,
                    (packets - 1) / seconds, bytes / (double) packets * (packets - 1) / seconds));
        }
    }

    private static void measure() {
        List<Pair<UUID, TrainMapSync.TrainMapSyncEntry>> entries = new ArrayList<>();
        int carriages = 0;
        for (Map.Entry<UUID, TrainMapSync.TrainMapSyncEntry> e : TrainMapSyncClient.currentData.entrySet()) {
            entries.add(Pair.of(e.getKey(), e.getValue()));
            if (e.getValue().positions != null)
                carriages += e.getValue().positions.length / 6;
        }
        FriendlyByteBuf buf = new FriendlyByteBuf(Unpooled.buffer());
        try {
            TrainMapSyncPacket.STREAM_CODEC.encode(buf, new TrainMapSyncPacket(false, entries));
            lastBytes = buf.readableBytes();
        } finally {
            buf.release();
        }
        lastTrains = entries.size();
        lastCarriages = carriages;
        if (firstTick < 0)
            firstTick = ticks;
        lastTick = ticks;
        packets++;
        bytes += lastBytes;
    }
}
