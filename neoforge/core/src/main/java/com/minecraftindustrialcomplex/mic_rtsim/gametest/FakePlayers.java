package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.mojang.authlib.GameProfile;
import io.netty.channel.embedded.EmbeddedChannel;
import net.minecraft.network.Connection;
import net.minecraft.network.protocol.PacketFlow;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.network.CommonListenerCookie;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.world.level.GameType;
import net.minecraft.world.phys.Vec3;

import java.util.UUID;

/**
 * Server players for GameTests that never log in. {@code GameTestHelper.makeMockServerPlayerInLevel}
 * logs its player in through the player list, and mods answer a login with their own payloads (Ribbits'
 * supporter hat, Carry On's script sync), which the mock connection has not negotiated: the payload
 * check throws, and with Carry On the server's tick loop dies. These players have a connection (so
 * vanilla packets, effects for example, go nowhere harmlessly) and stand in the level's player list,
 * which is what {@code Level.getNearbyPlayers} reads, but no login event ever fires for them.
 */
public final class FakePlayers {
    private FakePlayers() {}

    public static ServerPlayer add(ServerLevel level, Vec3 at, String name) {
        GameProfile profile = new GameProfile(UUID.randomUUID(), name);
        CommonListenerCookie cookie = CommonListenerCookie.createInitial(profile, false);
        ServerPlayer p = new ServerPlayer(level.getServer(), level, profile, cookie.clientInformation());
        Connection connection = new Connection(PacketFlow.SERVERBOUND);
        new EmbeddedChannel(connection);
        new ServerGamePacketListenerImpl(level.getServer(), connection, p, cookie);
        p.setGameMode(GameType.SURVIVAL);
        p.moveTo(at.x, at.y, at.z);
        level.players().add(p);
        return p;
    }

    public static void remove(ServerPlayer p) {
        p.serverLevel().players().remove(p);
    }

    private static final java.util.Map<ServerLevel, ServerPlayer> EDITORS = new java.util.WeakHashMap<>();

    /**
     * A server player who places and breaks blocks in GameTests, standing in no player list: what rtsim's
     * edit hooks see from a real player. A NeoForge {@code FakePlayer} ({@code FakePlayerFactory}) is a
     * machine to those hooks (a Create deployer is one), so it makes no edit (INDUSTRY.md §3.3).
     */
    public static ServerPlayer editor(ServerLevel level) {
        return EDITORS.computeIfAbsent(level, l -> {
            GameProfile profile = new GameProfile(UUID.nameUUIDFromBytes("mic_rtsim:gametest_editor".getBytes(java.nio.charset.StandardCharsets.UTF_8)), "rtsim_editor");
            CommonListenerCookie cookie = CommonListenerCookie.createInitial(profile, false);
            ServerPlayer p = new ServerPlayer(l.getServer(), l, profile, cookie.clientInformation());
            Connection connection = new Connection(PacketFlow.SERVERBOUND);
            new EmbeddedChannel(connection);
            new ServerGamePacketListenerImpl(l.getServer(), connection, p, cookie);
            p.setGameMode(GameType.SURVIVAL);
            return p;
        });
    }
}
