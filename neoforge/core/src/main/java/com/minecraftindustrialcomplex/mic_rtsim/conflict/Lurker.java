package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import net.minecraft.world.entity.Mob;
import net.minecraft.world.phys.Vec3;

/**
 * What a mod's body does while its party lies in wait by a rail (M8, MZB-361). Registered per entity type
 * id with {@code RtsimApi.registerLurker}; a type with none gets {@link Lurkers#DEFAULT}. Core types only
 * (D4): the Frostmaw's code is in the Mowzie plugin. All calls are on the server thread.
 *
 * <p>The core calls {@link #hold} on {@code ServerTickEvent.Post} for each live lurk puppet, after every
 * level ticked (an awake mob with a target re-paths in its own tick, so the hold's path is the one
 * followed next), and {@link #release} when the lurk ends, the puppet is recalled or taken, or the body leaves the
 * world (killed, despawned, the session over: a removed body is released too, so a Lurker's state for it can go). The
 * target rule (a puppet with no target gets the nearest player every 20 ticks, MZB-362) is the core's, for every type.
 *
 * <p>What a Lurker keeps for one body that references the body (a goal it took out has the body as its {@code mob})
 * belongs in the {@link Memo} the core hands it: the core keeps the memo on the body's slot, so it dies with the slot
 * (MZB-361). A map keyed by the body, even a weak one, whose values reach the body is never cleared.
 */
public interface Lurker {
    /**
     * What a Lurker keeps for one body it holds: one slot for any object, kept by the core with the body's slot and
     * dropped by it after {@link Lurker#release}. Empty when the hold has not stored anything (or has released).
     */
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

    /** Each server tick while it lurks: keep the body at {@code spot}. {@code memo} is this body's, the same one each call. */
    void hold(Mob mob, Vec3 spot, Memo memo);

    /**
     * Starts a strike at a carriage point; false if the body is busy. {@code motion} is how far the point moves in
     * one tick (zero for a stopped train): a strike that lands some ticks after it starts aims at where the point
     * will be, not where it is.
     */
    boolean strike(Mob mob, Vec3 at, Vec3 motion);

    /** The lurk ended or the body left it (it may be removed from the world by now): undo what {@link #hold} set, and empty {@code memo}. */
    void release(Mob mob, Memo memo);

    /** A train it robbed is gone: the body announces it (a roar). */
    void roar(Mob mob);

    /**
     * How far from its spot (blocks, horizontal) this Lurker's hold lets a body rest: it walks one back that is
     * farther. The spot-distance contract (MZB-374) reads it: a Lurker that strikes must keep the engine's spot
     * distance plus this under the sampler's strike reach, and its body's near edge off the rail. The default is the
     * core's walk-back distance for a body that never strikes.
     */
    default double stand() {
        return Lurkers.STRAY;
    }

    /**
     * D1 diagnostics (MZB-361): how many path searches this Lurker's holds have asked for since the game started
     * (a {@code moveTo} is a search whether or not its path differs); zero for a Lurker that keeps no count.
     */
    default long pathSearches() {
        return 0L;
    }

    /**
     * D1 diagnostics: how many live paths its holds have dropped ({@code stop()} on a navigation still on its way,
     * a path the body's own goals searched for, which they search for again next tick); zero for a Lurker that
     * keeps no count.
     */
    default long pathsDropped() {
        return 0L;
    }
}
