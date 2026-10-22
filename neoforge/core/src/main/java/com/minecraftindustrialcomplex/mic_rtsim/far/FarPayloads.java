package com.minecraftindustrialcomplex.mic_rtsim.far;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import java.util.function.Consumer;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerPlayer;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.network.event.RegisterPayloadHandlersEvent;
import net.neoforged.neoforge.network.registration.PayloadRegistrar;

/**
 * The far feed's two payloads (research/distant-horizons-2026-10-21.md §6.3, docs/boundary.md §12.2), both
 * optional, so a client without them (no far renderer, or an older mic_rtsim) still joins a server that has
 * them, and the other way round.
 *
 * <ul>
 *   <li>{@code mic_rtsim:far_sub}, client to server: what a far renderer wants (radius, detail tier, budget,
 *       kinds); radius 0 unsubscribes.</li>
 *   <li>{@code mic_rtsim:far}, server to client: a {@link FarWire} message, once a second while subscribed.</li>
 * </ul>
 */
public final class FarPayloads {
    /** The subscription's own version (its first field); the server ignores a subscription of another. */
    public static final int SUB_VERSION = 1;
    /** The largest message a client reads. */
    public static final int MAX_BYTES = 1 << 20;

    private static volatile Consumer<byte[]> clientHandler;

    private FarPayloads() {}

    /** What a far renderer wants. */
    public record Subscribe(int version, int radius, int tier, int budget, int kinds) implements CustomPacketPayload {
        public static final Type<Subscribe> TYPE = new Type<>(ResourceLocation.fromNamespaceAndPath(MicRtsim.MODID, "far_sub"));
        public static final StreamCodec<FriendlyByteBuf, Subscribe> CODEC = StreamCodec.of(
                (buf, p) -> {
                    buf.writeVarInt(p.version);
                    buf.writeVarInt(p.radius);
                    buf.writeVarInt(p.tier);
                    buf.writeVarInt(p.budget);
                    buf.writeVarInt(p.kinds);
                },
                buf -> new Subscribe(buf.readVarInt(), buf.readVarInt(), buf.readVarInt(), buf.readVarInt(), buf.readVarInt()));

        @Override
        public Type<Subscribe> type() {
            return TYPE;
        }
    }

    /** One far feed message ({@link FarWire}). */
    public record Feed(byte[] data) implements CustomPacketPayload {
        public static final Type<Feed> TYPE = new Type<>(ResourceLocation.fromNamespaceAndPath(MicRtsim.MODID, "far"));
        public static final StreamCodec<FriendlyByteBuf, Feed> CODEC = StreamCodec.of(
                (buf, p) -> buf.writeByteArray(p.data),
                buf -> new Feed(buf.readByteArray(MAX_BYTES)));

        @Override
        public Type<Feed> type() {
            return TYPE;
        }
    }

    public static void register(IEventBus modBus) {
        modBus.addListener(FarPayloads::onRegister);
    }

    private static void onRegister(RegisterPayloadHandlersEvent event) {
        PayloadRegistrar r = event.registrar("1").optional();
        r.playToServer(Subscribe.TYPE, Subscribe.CODEC, (p, ctx) -> {
            if (ctx.player() instanceof ServerPlayer sp)
                FarFeed.subscribe(sp, p);
        });
        r.playToClient(Feed.TYPE, Feed.CODEC, (p, ctx) -> {
            Consumer<byte[]> h = clientHandler;
            if (h != null)
                h.accept(p.data);
        });
    }

    /** The client side's handler for far feed messages (the core's client entry sets it). */
    public static void setClientHandler(Consumer<byte[]> handler) {
        clientHandler = handler;
    }
}
