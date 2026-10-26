package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.common.ClientboundCustomPayloadPacket;
import net.minecraft.network.protocol.game.ClientGamePacketListener;
import net.minecraft.network.protocol.game.ClientboundAddEntityPacket;
import net.minecraft.network.protocol.game.ClientboundBundlePacket;
import net.minecraft.network.protocol.game.ClientboundSetEntityDataPacket;
import net.minecraft.network.protocol.game.ClientboundSetEntityLinkPacket;
import net.minecraft.network.protocol.game.ClientboundSetEntityMotionPacket;
import net.minecraft.network.protocol.game.ClientboundSetEquipmentPacket;
import net.minecraft.network.protocol.game.ClientboundSetPassengersPacket;
import net.minecraft.network.protocol.game.ClientboundUpdateAttributesPacket;
import net.minecraft.network.protocol.game.ClientboundUpdateMobEffectPacket;
import net.minecraft.server.level.ServerEntity;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.neoforged.neoforge.network.bundle.PacketAndPayloadAcceptor;
import net.neoforged.neoforge.network.payload.AdvancedAddEntityPayload;

import java.util.ArrayList;
import java.util.List;
import java.util.Set;

/**
 * The re-pair (MZB-813): a body whose clients must see a field their copy only reads at its first tick (the
 * Umvuthi's direction) is sent to its trackers as if it had just come into view, and the client replaces its copy
 * (NeoForge posts the join for the fresh body before it removes the old one, so a client mod that cancels it keeps
 * the old facing, never a gap).
 *
 * <p>A throwaway {@link ServerEntity} is built for the body (its position base is the body's position now, so the
 * body must have held still for 2 ticks: the live tracker's base has caught up, MZB-813), and its
 * {@code sendPairingData} collected into one bundle that goes to every tracker of the body through
 * {@code ServerChunkCache.broadcast}. That is right only when nothing in the bundle is per player: NeoForge's
 * pairing ends with an attachment sync, and a payload of a mod is per player. So a bundle holding anything but the
 * vanilla pairing set and the complex spawn payload is skipped (counted by the bridge as {@code repairsSkipped}),
 * never sent.
 *
 * <p>The complex spawn payload ({@code AdvancedAddEntityPayload}, what an {@code IEntityWithComplexSpawn} adds) is
 * the same bytes for every player, so it is let through (MZB-873): the design skipped every such body, which is
 * every Mowzie mob ({@code MowzieEntity} implements it, with an empty write), so the re-pair would never have run
 * for the one body it is for.
 */
final class Repairs {
    private Repairs() {}

    /** What {@code ServerEntity.sendPairingData} sends for a living mob and nothing a mod adds. */
    private static final Set<Class<?>> VANILLA = Set.of(ClientboundAddEntityPacket.class, ClientboundSetEntityDataPacket.class,
            ClientboundUpdateAttributesPacket.class, ClientboundSetEquipmentPacket.class, ClientboundUpdateMobEffectPacket.class,
            ClientboundSetPassengersPacket.class, ClientboundSetEntityLinkPacket.class, ClientboundSetEntityMotionPacket.class);

    /** What a re-pair did. */
    enum Result {
        /** The bundle went to the body's trackers. */
        SENT,
        /** No player is in the level, so nobody tracks the body: nothing to do. */
        NOBODY,
        /** Something per player is in the pairing data: nothing was sent. */
        SKIPPED
    }

    static Result send(ServerLevel level, Entity body) {
        ServerPlayer any = null;
        for (ServerPlayer p : level.players()) {
            any = p;
            break;
        }
        if (any == null)
            return Result.NOBODY;
        List<Packet<? super ClientGamePacketListener>> list = new ArrayList<>(8);
        new ServerEntity(level, body, 1, true, p -> {}).sendPairingData(any, new PacketAndPayloadAcceptor<>(list::add));
        for (Packet<?> p : list)
            if (!VANILLA.contains(p.getClass())
                    && !(p instanceof ClientboundCustomPayloadPacket c && c.payload() instanceof AdvancedAddEntityPayload))
                return Result.SKIPPED;
        level.getChunkSource().broadcast(body, new ClientboundBundlePacket(list));
        return Result.SENT;
    }
}
