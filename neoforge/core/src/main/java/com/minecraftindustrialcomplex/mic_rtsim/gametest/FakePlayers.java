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
        return add(level, at, name, UUID.randomUUID());
    }

    /** A player with a given id: the same player logging in again is a new entity with the old id. */
    public static ServerPlayer add(ServerLevel level, Vec3 at, String name, UUID id) {
        ServerPlayer p = make(level, at, name, id);
        level.players().add(p);
        return p;
    }

    /**
     * A player that every reader sees (review C): joined through {@code level.addNewPlayer}, so it is in the
     * entity manager ({@code getPlayersNearby}, {@code getEntities}) as well as in {@code level.players()}
     * (the goals' {@code NearestAttackableTargetGoal}, {@code LookAtPlayerGoal}), which {@link #add}'s player is
     * not: that one is only in the list. A real player stands in both. It loads chunks round itself as a
     * player does, so remove it with {@link #removeSeen}, not {@link #remove}, and before releasing chunks.
     * UNVERIFIED on a running server (compiled only).
     */
    public static ServerPlayer addSeen(ServerLevel level, Vec3 at, String name) {
        ServerPlayer p = make(level, at, name, UUID.randomUUID());
        level.addNewPlayer(p);
        return p;
    }

    /** Takes out a player {@link #addSeen} joined: out of the entity manager and the list at once. */
    public static void removeSeen(ServerPlayer p) {
        p.serverLevel().removePlayerImmediately(p, net.minecraft.world.entity.Entity.RemovalReason.DISCARDED);
    }

    private static ServerPlayer make(ServerLevel level, Vec3 at, String name, UUID id) {
        GameProfile profile = new GameProfile(id, name);
        CommonListenerCookie cookie = CommonListenerCookie.createInitial(profile, false);
        ServerPlayer p = new ServerPlayer(level.getServer(), level, profile, cookie.clientInformation());
        Connection connection = new Connection(PacketFlow.SERVERBOUND);
        new EmbeddedChannel(connection);
        new ServerGamePacketListenerImpl(level.getServer(), connection, p, cookie);
        p.setGameMode(GameType.SURVIVAL);
        p.moveTo(at.x, at.y, at.z);
        return p;
    }

    /**
     * A server player who records what it is sent: every chat line in {@link Hearer#heard} and every
     * action-bar line in {@link Hearer#overlay}, and (M8, MZB-370) every sound packet in {@link Hearer#sounds}
     * and every title, subtitle and action-bar packet in {@link Hearer#titles}. Otherwise as {@link #add}.
     */
    public static Hearer hearing(ServerLevel level, Vec3 at, String name) {
        Hearer p = newHearer(level, at, name, null);
        level.players().add(p);
        return p;
    }

    /**
     * A {@link Hearer} that keeps every packet it is sent ({@link Hearer#packets}, a bundle flattened into its parts) and that
     * the level tracks entities to (M11, MZB-839): joined through {@code level.addNewPlayer}, as {@link #addSeen}'s is, since a
     * player only in {@code level.players()} is not in the chunk map, and no tracked entity is paired with it.
     * <p>
     * Three traps, all vanilla's. (1) The server's tick sends chunks only to {@code PlayerList}'s players, and this one is in
     * none, so its chunks are never sent unless {@link #pump} sends them; and an entity is paired with a player only once the
     * entity's chunk is tracked and no longer pending to it. (2) {@code PlayerChunkSender} sends no second batch before the
     * first is acknowledged, and nobody acknowledges for a mock connection; {@link #pump} does. (3) An entity is paired when it
     * is within the smaller of its tracking range and {@code viewDistance} chunks, in a loaded chunk the player tracks, so the
     * watcher must stand within that of what it watches (the default view is {@value #WATCH_VIEW} chunks, 64 blocks).
     * Like {@link #addSeen}'s it loads chunks round itself: take it out with {@link #removeSeen}, before releasing chunks.
     * UNVERIFIED on a running server (compiled only).
     */
    public static Hearer watching(ServerLevel level, Vec3 at, String name) {
        return watching(level, at, name, WATCH_VIEW);
    }

    /** The view distance (chunks) of a {@link #watching} player: the range a player's chunks and tracked entities reach. */
    public static final int WATCH_VIEW = 4;

    public static Hearer watching(ServerLevel level, Vec3 at, String name, int viewDistance) {
        Hearer p = newHearer(level, at, name, viewDistance);
        p.keepAll = true; // before it joins: the pairing of what already stands round it is sent by the join
        level.addNewPlayer(p);
        return p;
    }

    /**
     * Gives a {@link #watching} player's connection its turn the server's tick would give a logged-in one: acknowledges every
     * chunk batch it has been sent (the client's {@code ChunkBatchReceived}), sends its next batches (a few rounds, as chunks
     * are loaded), then re-evaluates what it tracks ({@code ServerChunkCache.move}: the entities already standing are paired
     * once their chunks have been sent, which no tick does for a player that never moves). Call it every tick of a wait.
     *
     * @return the number of chunk packets it sent
     */
    public static int pump(Hearer p) {
        net.minecraft.server.network.PlayerChunkSender sender = p.connection.chunkSender;
        int before = p.chunkPackets;
        for (int round = 0; round < PUMP_ROUNDS; round++) {
            while (p.batchesAcknowledged < p.batchesSeen) {
                p.batchesAcknowledged++;
                sender.onChunkBatchReceivedByClient(net.minecraft.server.network.PlayerChunkSender.MAX_CHUNKS_PER_TICK);
            }
            int seen = p.batchesSeen;
            sender.sendNextChunks(p);
            if (p.batchesSeen == seen)
                break;
        }
        p.serverLevel().getChunkSource().move(p);
        return p.chunkPackets - before;
    }

    private static final int PUMP_ROUNDS = 4;

    private static Hearer newHearer(ServerLevel level, Vec3 at, String name, Integer viewDistance) {
        GameProfile profile = new GameProfile(UUID.randomUUID(), name);
        CommonListenerCookie cookie = CommonListenerCookie.createInitial(profile, false);
        net.minecraft.server.level.ClientInformation info = cookie.clientInformation();
        if (viewDistance != null)
            info = new net.minecraft.server.level.ClientInformation(info.language(), viewDistance, info.chatVisibility(), info.chatColors(),
                    info.modelCustomisation(), info.mainHand(), info.textFilteringEnabled(), info.allowsListing());
        Hearer p = new Hearer(level.getServer(), level, profile, info);
        Connection connection = new Connection(PacketFlow.SERVERBOUND);
        new EmbeddedChannel(connection);
        new Hearing(level.getServer(), connection, p, cookie);
        p.setGameMode(GameType.SURVIVAL);
        p.moveTo(at.x, at.y, at.z);
        return p;
    }

    /**
     * A player that remembers the system messages it was sent (still sending them). {@link #heard} keeps
     * the talk lines only (keys under {@code mic_rtsim.talk.}), because a player on the server's list
     * also hears whatever other tests broadcast; {@link #raw} keeps everything.
     */
    public static final class Hearer extends ServerPlayer {
        public final java.util.List<net.minecraft.network.chat.Component> heard = new java.util.ArrayList<>();
        public final java.util.List<net.minecraft.network.chat.Component> raw = new java.util.ArrayList<>();
        public final java.util.List<net.minecraft.network.chat.Component> overlay = new java.util.ArrayList<>();
        /** Every sound packet sent to it (a train's roar, a notice's horn). */
        public final java.util.List<net.minecraft.network.protocol.game.ClientboundSoundPacket> sounds = new java.util.ArrayList<>();
        /** Every title, subtitle and action-bar text packet sent to it. */
        public final java.util.List<net.minecraft.network.chat.Component> titles = new java.util.ArrayList<>();
        /** Every packet sent to it, a bundle as its parts, in order: only for a player from {@link #watching}. */
        public final java.util.List<net.minecraft.network.protocol.Packet<?>> packets = new java.util.ArrayList<>();
        boolean keepAll;
        /** Chunk batches it has been sent and ones {@link #pump} has acknowledged, and chunk packets sent. */
        int batchesSeen, batchesAcknowledged, chunkPackets;

        /** How many packets of this class (or a subclass) it has been sent. */
        public long count(Class<?> type) {
            return packets.stream().filter(type::isInstance).count();
        }

        /** How many times the entity of this id was sent to it as added (a pairing), and how many times as removed. */
        public long added(int entityId) {
            return packets.stream().filter(q -> q instanceof net.minecraft.network.protocol.game.ClientboundAddEntityPacket a && a.getId() == entityId).count();
        }

        public long removed(int entityId) {
            return packets.stream().filter(q -> q instanceof net.minecraft.network.protocol.game.ClientboundRemoveEntitiesPacket r && r.getEntityIds().contains(entityId)).count();
        }

        Hearer(net.minecraft.server.MinecraftServer server, ServerLevel level, GameProfile profile,
               net.minecraft.server.level.ClientInformation info) {
            super(server, level, profile, info);
        }

        @Override
        public void sendSystemMessage(net.minecraft.network.chat.Component message, boolean overlayBar) {
            if (overlayBar) {
                overlay.add(message);
            } else {
                raw.add(message);
                if (message.getContents() instanceof net.minecraft.network.chat.contents.TranslatableContents t
                        && t.getKey().startsWith("mic_rtsim.talk."))
                    heard.add(message);
            }
            super.sendSystemMessage(message, overlayBar);
        }

        private void keep(net.minecraft.network.protocol.Packet<?> packet) {
            if (packet instanceof net.minecraft.network.protocol.BundlePacket<?> bundle) {
                for (net.minecraft.network.protocol.Packet<?> part : bundle.subPackets())
                    keep(part);
            } else {
                packets.add(packet);
            }
        }

        void record(net.minecraft.network.protocol.Packet<?> packet) {
            if (packet instanceof net.minecraft.network.protocol.game.ClientboundChunkBatchFinishedPacket)
                batchesSeen++;
            else if (packet instanceof net.minecraft.network.protocol.game.ClientboundLevelChunkWithLightPacket)
                chunkPackets++;
            if (keepAll)
                keep(packet);
            if (packet instanceof net.minecraft.network.protocol.game.ClientboundSoundPacket sound)
                sounds.add(sound);
            else if (packet instanceof net.minecraft.network.protocol.game.ClientboundSetTitleTextPacket title)
                titles.add(title.text());
            else if (packet instanceof net.minecraft.network.protocol.game.ClientboundSetSubtitleTextPacket subtitle)
                titles.add(subtitle.text());
            else if (packet instanceof net.minecraft.network.protocol.game.ClientboundSetActionBarTextPacket bar)
                titles.add(bar.text());
        }
    }

    /** The listener of a {@link Hearer}: hands every packet to {@link Hearer#record} and sends it as any listener does. */
    private static final class Hearing extends ServerGamePacketListenerImpl {
        Hearing(net.minecraft.server.MinecraftServer server, Connection connection, Hearer player, CommonListenerCookie cookie) {
            super(server, connection, player, cookie);
        }

        @Override
        public void send(net.minecraft.network.protocol.Packet<?> packet) {
            if (this.player instanceof Hearer h)
                h.record(packet);
            super.send(packet);
        }
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
