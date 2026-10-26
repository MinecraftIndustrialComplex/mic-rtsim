package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.phys.Vec3;

/**
 * What a mod's body does to travel with its party in place of walking (M11, MZB-807): the Umvuthi's puppet floats
 * aloft with its Invasion. Registered per entity type id with {@code RtsimApi.registerTraveller}; a type with none
 * walks as every body does (the party's follow goal). Core types only (D4): the Mowzie plugin's code is in its own
 * module, and the pure rules are core's {@link AloftRules}. All calls are on the server thread.
 *
 * <p>The core calls {@link #step} once a tick for each live traveller slot, from the end of its tick, after every
 * level has ticked its entities and block entities (a push that lands after them is wiped by the step; one that
 * lands before them is the core's to cancel, MZB-810). A traveller's slot has no follow goal and no navigation.
 *
 * <p>What a Traveller keeps for one body belongs in the {@link Memo} the core hands it: the core keeps the memo on
 * the body's slot, so it dies with the slot (as {@link Lurker.Memo}, MZB-361).
 */
public interface Traveller {
    /** What a Traveller keeps for one body: one slot for any object, kept by the core with the body's slot. */
    final class Memo {
        private Object state;

        /** The stored object if it is a {@code type}, else null. */
        public <T> T get(Class<T> type) {
            return type.isInstance(state) ? type.cast(state) : null;
        }

        /** Stores {@code state} (null empties the memo). */
        public void set(Object state) {
            this.state = state;
        }
    }

    /**
     * Where the party sends one body this tick (a reused object: read it, never keep it). All in blocks, x and z.
     * {@code place} is the body's place in its column now; {@code target} is the point the party's walk is toward
     * (the last steer's), which the attack flag is about.
     */
    final class Way {
        private double placeX, placeZ, targetX, targetZ;
        private boolean attack, recalled;
        private long now;

        void set(double placeX, double placeZ, double targetX, double targetZ, boolean attack, boolean recalled, long now) {
            this.placeX = placeX;
            this.placeZ = placeZ;
            this.targetX = targetX;
            this.targetZ = targetZ;
            this.attack = attack;
            this.recalled = recalled;
            this.now = now;
        }

        public double placeX() {
            return placeX;
        }

        public double placeZ() {
            return placeZ;
        }

        public double targetX() {
            return targetX;
        }

        public double targetZ() {
            return targetZ;
        }

        /** The party's attack flag (BAND_STEER's FLAG_ATTACK). */
        public boolean attack() {
            return attack;
        }

        /** The party is being recalled (it walks off toward the record's point and is despawned). */
        public boolean recalled() {
            return recalled;
        }

        /** The level's game time. */
        public long now() {
            return now;
        }
    }

    /** A fresh memo for a body (the core keeps it on the body's slot). */
    default Memo memo() {
        return new Memo();
    }

    /**
     * The body has just been made for a slot of this kind and is about to join the level: apply the chief's memory
     * ({@code memory} is the engine's CHIEF_BODY for the party, null with none), and start it where it should be
     * (the core has placed it).
     */
    void made(ServerLevel level, Mob body, CampKind.ChiefBody memory, Memo memo);

    /** Each server tick: move the body toward {@code way}, as its rules say. A dying body is never stepped. */
    void step(ServerLevel level, Mob body, Way way, Memo memo);

    /**
     * Each server tick while the body is called home (MZB-822): float to {@code spot}, turn to {@code direction} (the
     * throne's facing, as the body kind numbers it: the jar's 1 to 4) and settle there; true once it has. A body that
     * is homing is not {@link #step}ped.
     */
    boolean home(ServerLevel level, Mob body, Vec3 spot, int direction, Memo memo);

    /**
     * Blocks above the ground at its place the body floats while it travels: added to the ground's height where the
     * core places the body (its spawn, and a displaced body's teleport back, so that a body is not pulled to the
     * ground every check). {@code memo} is null for a body not made yet.
     */
    default double lift(Memo memo) {
        return 0.0;
    }

    /** The body leaves its party alive (taken, picked up, handed back) or its slot ends: undo what makes it float, empty {@code memo}. */
    void release(Mob body, Memo memo);

    /** The body's first death event: let it fall through its death animation (it is never stepped again). */
    void died(Mob body, Memo memo);
}
