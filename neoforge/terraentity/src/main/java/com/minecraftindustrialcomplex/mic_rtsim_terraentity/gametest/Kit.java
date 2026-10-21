package com.minecraftindustrialcomplex.mic_rtsim_terraentity.gametest;

import com.minecraftindustrialcomplex.mic_rtsim_terraentity.MicRtsimTerraEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.MobSpawnType;
import net.minecraft.world.phys.Vec3;
import net.neoforged.fml.ModList;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;

/** Shared helpers of the spike's GameTests. Every result goes out on a {@code [spike]} log line. */
final class Kit {
    private Kit() {}

    static final String ARM = System.getProperty("mic_rtsim_terraentity.arm", "?");

    static boolean terraLoaded() {
        return ModList.get().isLoaded("terra_entity");
    }

    /** False (and the test already succeeded) when -PteTests excludes this test or Terra Entity is off the server. */
    static boolean run(GameTestHelper helper, String name, boolean needsTerra) {
        String only = System.getProperty("mic_rtsim_terraentity.tests");
        boolean skip = (only != null && !name.matches(only)) || (needsTerra && !terraLoaded());
        if (skip) {
            say(name, "skipped", only != null && !name.matches(only) ? "not selected" : "terra_entity not loaded");
            helper.succeed();
        }
        return !skip;
    }

    static void say(String test, String what, Object value) {
        MicRtsimTerraEntity.LOGGER.info("[spike] {} | {} | {} = {}", ARM, test, what, value);
    }

    static void check(String what, boolean ok) {
        if (!ok)
            throw new GameTestAssertException(what);
    }

    static EntityType<?> type(String id) {
        ResourceLocation rl = id.contains(":") ? ResourceLocation.parse(id) : ResourceLocation.fromNamespaceAndPath("terra_entity", id);
        EntityType<?> t = BuiltInRegistries.ENTITY_TYPE.get(rl);
        if (!BuiltInRegistries.ENTITY_TYPE.containsKey(rl))
            throw new GameTestAssertException("no entity type " + rl);
        return t;
    }

    /** Spawns through the type's own spawn path (finalize event included), facing east (+x), persistent. */
    static Entity spawn(ServerLevel level, String id, Vec3 at) {
        Entity e = type(id).spawn(level, BlockPos.containing(at), MobSpawnType.COMMAND);
        if (e == null)
            throw new GameTestAssertException("could not spawn " + id);
        e.moveTo(at.x, at.y, at.z, -90f, 0f);
        e.setYHeadRot(-90f);
        e.setYBodyRot(-90f);
        if (e instanceof Mob m)
            m.setPersistenceRequired();
        return e;
    }

    /** A target that never moves and cannot be hurt: a villager with no AI. */
    static net.minecraft.world.entity.npc.Villager marker(ServerLevel level, Vec3 at) {
        var v = net.minecraft.world.entity.EntityType.VILLAGER.create(level);
        v.moveTo(at.x, at.y, at.z, 0f, 0f);
        v.setNoAi(true);
        v.setInvulnerable(true);
        v.setSilent(true);
        v.setPersistenceRequired();
        level.addFreshEntity(v);
        return v;
    }

    static Path outDir() {
        Path p = Path.of("spike");
        try {
            Files.createDirectories(p);
        } catch (IOException ignored) {
        }
        return p;
    }

    static void csv(String name, List<String> lines) {
        try {
            Files.write(outDir().resolve(ARM + "-" + name + ".csv"), lines);
        } catch (IOException e) {
            MicRtsimTerraEntity.LOGGER.warn("[spike] could not write {}", name, e);
        }
    }

    static boolean isTerra(ResourceLocation id) {
        return id.getNamespace().equals("terra_entity");
    }

    static String f2(double d) {
        return String.format(java.util.Locale.ROOT, "%.2f", d);
    }

    /** A survival player added to the level the way a login does (tracked, ticking), without any login event. */
    static net.minecraft.server.level.ServerPlayer trackedPlayer(ServerLevel level, Vec3 at) {
        var profile = new com.mojang.authlib.GameProfile(java.util.UUID.randomUUID(), "worm-bait");
        var cookie = net.minecraft.server.network.CommonListenerCookie.createInitial(profile, false);
        var p = new net.minecraft.server.level.ServerPlayer(level.getServer(), level, profile, cookie.clientInformation());
        var connection = new net.minecraft.network.Connection(net.minecraft.network.protocol.PacketFlow.SERVERBOUND);
        new io.netty.channel.embedded.EmbeddedChannel(connection);
        new net.minecraft.server.network.ServerGamePacketListenerImpl(level.getServer(), connection, p, cookie);
        p.setGameMode(net.minecraft.world.level.GameType.SURVIVAL);
        p.moveTo(at.x, at.y, at.z);
        level.addNewPlayer(p);
        return p;
    }

    static void removeTrackedPlayer(net.minecraft.server.level.ServerPlayer p) {
        p.serverLevel().removePlayerImmediately(p, Entity.RemovalReason.DISCARDED);
    }

    static String f(double d) {
        return String.format(java.util.Locale.ROOT, "%.1f", d);
    }

    static String v(Vec3 p) {
        return f(p.x) + "," + f(p.y) + "," + f(p.z);
    }
}
