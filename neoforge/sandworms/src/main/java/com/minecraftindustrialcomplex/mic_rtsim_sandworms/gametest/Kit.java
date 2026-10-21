package com.minecraftindustrialcomplex.mic_rtsim_sandworms.gametest;

import com.minecraftindustrialcomplex.mic_rtsim_sandworms.MicRtsimSandworms;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.npc.Villager;
import net.minecraft.world.phys.Vec3;
import net.neoforged.fml.ModList;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;

/** Shared helpers of the spike's GameTests. Every result goes out on a {@code [spike]} log line. */
final class Kit {
    private Kit() {}

    static final String ARM = System.getProperty("mic_rtsim_sandworms.arm", "?");
    static final boolean BIOME_DEFAULT = "default".equals(System.getProperty("mic_rtsim_sandworms.biome"));

    static boolean modLoaded() {
        return ModList.get().isLoaded("sandworm_mod");
    }

    /** False (and the test already succeeded) when -PswTests excludes this test, or it needs the mod and the mod is off the server. */
    static boolean run(GameTestHelper helper, String name, boolean needsMod) {
        String only = System.getProperty("mic_rtsim_sandworms.tests");
        boolean skip = (only != null && !name.matches(only)) || (needsMod && !modLoaded());
        if (skip) {
            say(name, "skipped", only != null && !name.matches(only) ? "not selected" : "sandworm_mod not loaded");
            helper.succeed();
        }
        return !skip;
    }

    static void say(String test, String what, Object value) {
        MicRtsimSandworms.LOGGER.info("[spike] {} | {} | {} = {}", ARM, test, what, value);
    }

    static void check(String what, boolean ok) {
        if (!ok)
            throw new GameTestAssertException(what);
    }

    /** A target that never moves: a villager with no AI and a lot of health, so the worm's hits show up as damage. */
    static Villager victim(ServerLevel level, Vec3 at, double hp) {
        Villager v = net.minecraft.world.entity.EntityType.VILLAGER.create(level);
        v.moveTo(at.x, at.y, at.z, 0f, 0f);
        v.setNoAi(true);
        v.setSilent(true);
        v.setPersistenceRequired();
        v.getAttribute(Attributes.MAX_HEALTH).setBaseValue(hp);
        v.setHealth((float) hp);
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
            MicRtsimSandworms.LOGGER.warn("[spike] could not write {}", name, e);
        }
    }

    static String f2(double d) {
        return String.format(java.util.Locale.ROOT, "%.2f", d);
    }

    /**
     * A survival player added to the level the way a login does (tracked, ticking) without a login event. Its connection is the one
     * NeoForge's own game-test mock uses: it acts as if the client had every payload registered, so the worm's screen-shake payloads
     * (Lodestone) go out instead of killing the tick loop ("Payload ... may not be sent to the client!").
     */
    static net.minecraft.server.level.ServerPlayer trackedPlayer(ServerLevel level, Vec3 at, String name) {
        var profile = new com.mojang.authlib.GameProfile(java.util.UUID.randomUUID(), name);
        var cookie = net.minecraft.server.network.CommonListenerCookie.createInitial(profile, false);
        var p = new net.minecraft.server.level.ServerPlayer(level.getServer(), level, profile, cookie.clientInformation());
        var connection = new net.minecraft.network.Connection(net.minecraft.network.protocol.PacketFlow.SERVERBOUND);
        new io.netty.channel.embedded.EmbeddedChannel(connection);
        net.neoforged.neoforge.network.registration.NetworkRegistry.configureMockConnection(connection);
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
