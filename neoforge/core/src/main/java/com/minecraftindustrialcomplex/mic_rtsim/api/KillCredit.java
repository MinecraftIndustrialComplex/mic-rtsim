package com.minecraftindustrialcomplex.mic_rtsim.api;

import net.minecraft.server.MinecraftServer;
import net.minecraft.server.players.GameProfileCache;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.OwnableEntity;
import net.minecraft.world.entity.player.Player;
import net.neoforged.neoforge.common.util.FakePlayer;

import java.util.UUID;

/**
 * Who is credited with a sim member's death (EQUILIBRIUM.md §2.2, slice S3, MZB-1006 and MZB-1008): one rule for every
 * bridge's death report, so a pet's kill, a death within five seconds of a player's hit and a machine's kill read the same
 * at a camp, a lair, a nest, a herd and a village. The codes are the one table every death record uses ({@code
 * rtsim_kernel::renewal::credit}); the engine maps them to a loss cause, its boss rule included. First match wins:
 *
 * <ol>
 *   <li>the damage names a real player (not a {@link FakePlayer}, a machine, MZB-111): PLAYER, that player;
 *   <li>the damage names a player's pet ({@link OwnableEntity} with a player owner, see {@link #ownerOf}): PLAYER, the owner;
 *   <li>vanilla's {@code getKillCredit} is a real player (their hit within 100 ticks, then a fall or lava; vanilla already
 *       credits a tame animal's hit to its online owner here): PLAYER;
 *   <li>{@code getKillCredit} is a player's pet: PLAYER, the owner;
 *   <li>otherwise the kind of the damage's entity ({@link #kindOf}): a FakePlayer or any other non-mob entity (TNT with no
 *       igniter, a dispenser's arrow) is OTHER, a mob is MOB, and no entity at all is NONE.
 * </ol>
 *
 * <p>Takes (TAKEN) are not deaths and are named by the record that reports them. The guard rule (a player-made golem's or a
 * player's guard's kill is a player's) stays in the nests' own rule (MZB-1021).
 */
public final class KillCredit {
    public static final int NONE = 0, PLAYER = 1, MOB = 2, OTHER = 3, TAKEN = 4;

    /** A death's credit: the code, and the credited player's UUID (null unless PLAYER). */
    public record Credit(int by, UUID player) {
        public static final Credit NOBODY = new Credit(NONE, null);

        public boolean byPlayer() {
            return by == PLAYER;
        }
    }

    private KillCredit() {}

    /** The credit for {@code dead}'s death by {@code source}. */
    public static Credit of(LivingEntity dead, DamageSource source) {
        return of(dead, source.getEntity());
    }

    /**
     * The credit for {@code dead}'s death by a damage whose entity was {@code src} (null for none), for a caller that kept
     * only the entity until the end of the tick; the dead body's {@code getKillCredit} is still set then.
     */
    public static Credit of(LivingEntity dead, Entity src) {
        Player pl = realPlayer(src);
        if (pl != null)
            return new Credit(PLAYER, pl.getUUID());
        MinecraftServer server = dead.getServer();
        UUID owner = ownerOf(src, server);
        if (owner != null)
            return new Credit(PLAYER, owner);
        LivingEntity credit = dead.getKillCredit();
        pl = realPlayer(credit);
        if (pl != null)
            return new Credit(PLAYER, pl.getUUID());
        owner = ownerOf(credit, server);
        if (owner != null)
            return new Credit(PLAYER, owner);
        int by = kindOf(src);
        return by == NONE ? Credit.NOBODY : new Credit(by, null);
    }

    /**
     * The real player a damage source names, itself or as a pet's online owner (rules 1 and 2 for a fight watch, which has a
     * live player to talk to); null for a machine and any other source.
     */
    public static Player playerOf(DamageSource source) {
        Entity src = source.getEntity();
        Player pl = realPlayer(src);
        if (pl != null)
            return pl;
        return src instanceof OwnableEntity o ? realPlayer(o.getOwner()) : null;
    }

    /** {@code e} when it is a player and not a machine's {@link FakePlayer} (MZB-111), else null. */
    public static Player realPlayer(Entity e) {
        return e instanceof Player pl && !(pl instanceof FakePlayer) ? pl : null;
    }

    /**
     * The kind of a damage's entity alone, with no credit: NONE for none, PLAYER for a real player, MOB for a mob, OTHER for
     * a {@link FakePlayer} (a machine, MZB-111) and anything else (a projectile with no owner, primed TNT with no igniter).
     */
    public static int kindOf(Entity e) {
        if (e == null)
            return NONE;
        if (e instanceof FakePlayer)
            return OTHER;
        if (e instanceof Player)
            return PLAYER;
        if (e instanceof Mob)
            return MOB;
        return OTHER;
    }

    /**
     * The player owner of a pet, or null: {@code e} is an {@link OwnableEntity} with an owner UUID whose owner, when it
     * resolves in the world, is a real player, and when it does not (the owner is offline or elsewhere), is a player this
     * server has seen (its profile cache), so a modded minion owned by a mob is not a player's pet. A server with no profile
     * cache takes the UUID as it stands.
     */
    public static UUID ownerOf(Entity e, MinecraftServer server) {
        if (!(e instanceof OwnableEntity o))
            return null;
        UUID id = o.getOwnerUUID();
        if (id == null)
            return null;
        LivingEntity owner = o.getOwner();
        if (owner != null)
            return realPlayer(owner) != null ? id : null;
        GameProfileCache cache = server == null ? null : server.getProfileCache();
        return cache == null || cache.get(id).isPresent() ? id : null;
    }
}
