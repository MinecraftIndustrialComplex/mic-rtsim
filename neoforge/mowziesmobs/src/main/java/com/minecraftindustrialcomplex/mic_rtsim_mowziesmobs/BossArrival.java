package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs;

import com.bobmowzie.mowziesmobs.client.particle.ParticleCloud;
import com.bobmowzie.mowziesmobs.client.particle.ParticleOrb;
import com.bobmowzie.mowziesmobs.client.particle.ParticleSnowFlake;
import com.bobmowzie.mowziesmobs.server.entity.effects.EntityCameraShake;
import com.bobmowzie.mowziesmobs.server.sound.MMSounds;
import net.minecraft.core.Holder;
import net.minecraft.core.particles.BlockParticleOption;
import net.minecraft.core.particles.ParticleOptions;
import net.minecraft.core.particles.ParticleTypes;
import net.minecraft.network.protocol.game.ClientboundSoundPacket;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.phys.Vec3;

import java.util.concurrent.atomic.AtomicLong;

/**
 * The return of a boss (S9, FB-S9-3; Ben, 2026-10-30: "just spawn anyway, bonus points if you have some cool effects"): a Frostmaw
 * back at its den or an Umvuthi back on its throne is placed when it is due, with a player beside it or watching, and arrives
 * with a short effect that this class sends from the server, so every client in range sees and hears it. Everything in it is the
 * game's or the Mowzie's Mobs jar's own (no asset of ours, D3): vanilla's particles and sound packets, the jar's particle types
 * ({@link ParticleCloud.Data}, {@link ParticleSnowFlake.Data}, {@link ParticleOrb.Data}, the same records the jars' bosses spawn
 * client-side), its sound events ({@link MMSounds}) and its camera shake ({@link EntityCameraShake#cameraShake}, the entity the
 * jar's roars add). The bosses' own animation is not played: the Frostmaw's ACTIVATE wakes it and arms it against the player
 * beside it, and an arrival must not make the boss hostile; the Frostmaw stays asleep as the den keeps it.
 *
 * <p><b>The Frostmaw</b> (the den's spot): shards of packed ice break out of the ground; the jar's roar ring, two rings of fifteen
 * soft clouds a block up at 0.9 and 0.65 blocks a tick, as {@code EntityFrostmaw.doRoarEffects} spawns them; the jar's snowflakes
 * and vanilla's drift up round it; the jar's frozen crash, its landing thump and its roar at volume 4 (the jar's own for its
 * wake-up roar), and a camera shake of 45 blocks (magnitude 0.03, 20 ticks and 20 of fade, the jar's roar shake kept short).
 *
 * <p><b>The Umvuthi</b> (its throne): the jar's white orbs fly in to it from a sphere of 3.5 blocks, as its attack gathers them;
 * a ring of flames runs out along the ground, a column of smoke rises from the fire pit's place and ash falls round it; its belly
 * drum and its roar at volume 3 (the jar's own for its abilities), and a light camera shake of 28 blocks (magnitude 0.015).
 *
 * <p>Both are one burst in the tick of the arrival and last as long as their particles and samples do (about two seconds), and
 * are sent only when a player is within {@link #RANGE} blocks. A grove's warriors come back as they always did, quietly and out
 * of sight ({@link GroveKind#spawnAt}); only the boss's return is the spectacle.
 */
public final class BossArrival {
    private BossArrival() {}

    /** A player within this many blocks is sent the effect: the roar at volume 4 carries 64 (vanilla's 16 a volume). */
    public static final double RANGE = 64.0;

    /** Arrivals played, by boss (the GameTests read them): a burst is counted when a player was in range to be sent it. */
    public static final AtomicLong FROSTMAWS = new AtomicLong(), UMVUTHIS = new AtomicLong();

    /** The Frostmaw's return at {@code at} (its den's spot). */
    public static void frostmaw(ServerLevel level, Vec3 at) {
        if (!watched(level, at))
            return;
        FROSTMAWS.incrementAndGet();
        double x = at.x, y = at.y, z = at.z;
        level.sendParticles(new BlockParticleOption(ParticleTypes.BLOCK, Blocks.PACKED_ICE.defaultBlockState()), x, y + 0.3, z, 60, 2.5, 0.2, 2.5, 0.15);
        ring(level, new ParticleCloud.Data(0.75f, 0.75f, 1.0f, 40.0f, 22, ParticleCloud.EnumCloudBehavior.GROW, 1.0f), x, y + 1.0, z, 15, 0.9);
        ring(level, new ParticleCloud.Data(0.75f, 0.75f, 1.0f, 35.0f, 22, ParticleCloud.EnumCloudBehavior.GROW, 1.0f), x, y + 1.0, z, 15, 0.65);
        level.sendParticles(new ParticleSnowFlake.Data(40.0f, false), x, y + 1.0, z, 30, 2.0, 1.0, 2.0, 0.12);
        level.sendParticles(ParticleTypes.SNOWFLAKE, x, y + 1.0, z, 90, 3.0, 1.5, 3.0, 0.08);
        level.sendParticles(ParticleTypes.CLOUD, x, y + 0.3, z, 20, 1.5, 0.2, 1.5, 0.05);
        sound(level, at, MMSounds.ENTITY_FROSTMAW_FROZEN_CRASH, 3.0f, 1.0f);
        sound(level, at, MMSounds.ENTITY_FROSTMAW_LAND, 2.0f, 0.8f);
        sound(level, at, MMSounds.ENTITY_FROSTMAW_ROAR, 4.0f, 1.0f);
        EntityCameraShake.cameraShake(level, at, 45.0f, 0.03f, 20, 20);
    }

    /** The Umvuthi's return at {@code at} (where its throne stands). */
    public static void umvuthi(ServerLevel level, Vec3 at) {
        if (!watched(level, at))
            return;
        UMVUTHIS.incrementAndGet();
        double x = at.x, y = at.y, z = at.z;
        ParticleOptions orb = ParticleOrb.Data.create((float) x, (float) (y + 1.0), (float) z, 6.0f);
        for (int i = 0; i < 24; i++) {
            double yaw = level.random.nextDouble() * Math.PI * 2, pitch = Math.acos(2 * level.random.nextDouble() - 1);
            level.sendParticles(orb, x + 3.5 * Math.sin(pitch) * Math.cos(yaw), y + 1.0 + 3.5 * Math.cos(pitch), z + 3.5 * Math.sin(pitch) * Math.sin(yaw),
                    1, 0.0, 0.0, 0.0, 0.0);
        }
        ring(level, ParticleTypes.FLAME, x, y + 0.2, z, 20, 0.22);
        for (int i = 0; i < 5; i++)
            level.sendParticles(ParticleTypes.CAMPFIRE_SIGNAL_SMOKE, x + (level.random.nextDouble() - 0.5) * 1.5, y + 0.5, z + (level.random.nextDouble() - 0.5) * 1.5,
                    0, 0.0, 0.12, 0.0, 1.0);
        level.sendParticles(ParticleTypes.ASH, x, y + 2.0, z, 60, 3.0, 2.0, 3.0, 0.0);
        sound(level, at, MMSounds.ENTITY_UMVUTHI_BELLY, 3.0f, 1.0f);
        sound(level, at, MMSounds.ENTITY_UMVUTHI_ROAR, 3.0f, 1.0f);
        EntityCameraShake.cameraShake(level, at, 28.0f, 0.015f, 10, 20);
    }

    /** Whether a player who is not a spectator stands within {@link #RANGE} blocks of {@code at}: nobody to send the effect to otherwise. */
    private static boolean watched(ServerLevel level, Vec3 at) {
        for (ServerPlayer p : level.players())
            if (!p.isSpectator() && p.distanceToSqr(at) <= RANGE * RANGE)
                return true;
        return false;
    }

    /**
     * {@code count} particles in a ring round (x, y, z), each flying out at {@code speed} blocks a tick: with a count of 0 the
     * client takes the offsets as the one particle's velocity, as {@code ClientPacketListener.handleParticleEvent} does.
     */
    private static void ring(ServerLevel level, ParticleOptions options, double x, double y, double z, int count, double speed) {
        for (int i = 1; i <= count; i++) {
            double yaw = Math.toRadians(i * 360.0 / count);
            level.sendParticles(options, x, y, z, 0, Math.cos(yaw) * speed, 0.0, Math.sin(yaw) * speed, 1.0);
        }
    }

    /**
     * One sound to every player of the level in earshot (vanilla's rule: 16 blocks a volume above 1, else 16), sent to each one's
     * connection over {@code level.players()} (vanilla's {@code playSound} broadcasts over the server's whole player list).
     */
    private static void sound(ServerLevel level, Vec3 at, Holder<SoundEvent> holder, float volume, float pitch) {
        double range = volume > 1.0f ? 16.0 * volume : 16.0;
        long seed = level.random.nextLong();
        for (ServerPlayer p : level.players())
            if (p.distanceToSqr(at) < range * range)
                p.connection.send(new ClientboundSoundPacket(holder, SoundSource.HOSTILE, at.x, at.y, at.z, volume, pitch, seed));
    }
}
