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
 * followed next), and {@link #release} when the lurk ends or the puppet is recalled. The target rule
 * (a puppet with no target gets the nearest player every 20 ticks, MZB-362) is the core's, for every type.
 */
public interface Lurker {
    /** Each server tick while it lurks: keep the body at {@code spot}. */
    void hold(Mob mob, Vec3 spot);

    /**
     * Starts a strike at a carriage point; false if the body is busy. {@code motion} is how far the point moves in
     * one tick (zero for a stopped train): a strike that lands some ticks after it starts aims at where the point
     * will be, not where it is.
     */
    boolean strike(Mob mob, Vec3 at, Vec3 motion);

    /** The lurk ended or the body left it: undo what {@link #hold} set. */
    void release(Mob mob);

    /** A train it robbed is gone: the body announces it (a roar). */
    void roar(Mob mob);

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
