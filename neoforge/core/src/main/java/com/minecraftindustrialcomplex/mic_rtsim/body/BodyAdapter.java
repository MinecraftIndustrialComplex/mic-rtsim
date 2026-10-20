package com.minecraftindustrialcomplex.mic_rtsim.body;

import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.Vec3;

/**
 * How the bridge drives one kind of body (PLAN.md §1.4): spawn, capture and steer. No Entity ever
 * reaches the engine; the adapter turns entities into captured state and commands into entities.
 *
 * <p>The core holds the vanilla adapters; a plugin registers adapters for its mod's bodies through
 * {@code RtsimApi}. An adapter must never throw on a body it does not fully understand (the Veloren
 * lesson "panicking on bodies it doesn't handle"); it captures what it can.
 */
public interface BodyAdapter {

    /** The body kind's name in the dictionary, e.g. {@code minecraft:wandering_trader}. */
    String name();

    /**
     * Creates the body at {@code pos}, not yet added to the level, with the spawn's health, body bits
     * and cargo applied. Returns null when the body cannot be made (the spawn is then rejected).
     */
    Entity create(ServerLevel level, Vec3 pos, MemberSpawn spawn);

    /** After every member of the group is in the level: wire members together (leash to the leader). */
    default void afterGroupSpawned(Entity self, PuppetGroup group) {}

    /** Captures what the engine keeps: health, position, body bits, cargo, leash and tame state. */
    void capture(Entity entity, PuppetGroup group, MemberCapture out);

    /** Points the body at a target. Only the leader is steered; members follow it. */
    default void steer(Entity entity, Vec3 target, float speed) {}

    /**
     * Called on every puppet pass while embodied: keep the body safe to leave alone (refresh despawn
     * timers). Must be cheap.
     */
    default void maintain(Entity entity, PuppetGroup group) {}

    /**
     * The member has left the sim and is an ordinary entity from now on: undo anything that only made
     * sense while the bridge was looking after it (a despawn timer it kept refreshing, a leash to the
     * group's leader).
     */
    default void onTaken(Entity entity, PuppetGroup group) {}

    /** Thrown by {@link #create} to refuse a spawn with a specific {@code Obs.SpawnRejected} reason. */
    final class SpawnRefused extends RuntimeException {
        public final int reason;

        public SpawnRefused(int reason, String message) {
            super(message, null, false, false);
            this.reason = reason;
        }
    }

    /**
     * Whether a player took this member by some means the bridge only sees by looking (PLAN.md §6.2):
     * a lead (NeoForge 21.1 has no leash event), or a tame that skipped AnimalTameEvent. Returns a
     * {@code Obs.Taken} reason, or 0 when the member is still the sim's.
     */
    default int takenBy(Entity entity, PuppetGroup group) {
        return 0;
    }

    /**
     * The cargo this body itself carries (C21): a trader llama's chest, a ribbit merchant's pack. Null
     * when it carries none. The bridge watches it for robbery (CARGO_CHANGED).
     */
    default com.minecraftindustrialcomplex.mic_rtsim.market.CargoAccess ownCargo(Entity entity) {
        return null;
    }

    /**
     * The cargo this body trades from when it is a merchant: what its sales come out of and its
     * payments go into. A wandering trader trades from its group's llama chests. Default: its own.
     */
    default com.minecraftindustrialcomplex.mic_rtsim.market.CargoAccess tradeCargo(Entity entity, PuppetGroup group) {
        return ownCargo(entity);
    }
}
