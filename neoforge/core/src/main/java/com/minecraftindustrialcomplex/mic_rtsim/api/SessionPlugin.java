package com.minecraftindustrialcomplex.mic_rtsim.api;

import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.AgentTag;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.Level;

import java.nio.ByteBuffer;

/**
 * A plugin with records and puppets of its own beside the core's: a kind range of commands it applies,
 * agents of its own namespace whose bodies it materializes, captures and reports itself, and the
 * session's lifecycle. The Ice and Fire plugin ({@code mic_rtsim_iceandfire}, docs/boundary.md §6) is
 * the first: its dragons are puppets the core's group machinery (C25) does not drive, because their
 * records are the dragons' own (DRAGON_MATERIALIZED and so on), not the core's.
 *
 * <p>Its puppets carry the core's {@code mic_rtsim:agent} tag, so the core's save mixin never lets
 * vanilla save them (PLAN.md §6.1 item 3); the core asks the plugin that owns a tag's agent, not its own
 * registry, whether a tagged entity joining a level is a live puppet or a stale copy.
 *
 * <p>Register with {@link RtsimApi#registerPlugin} during mod construction. Every method runs on the
 * server thread; an exception is logged and does not stop the bridge.
 */
public interface SessionPlugin {

    /** A short name for logs. */
    String name();

    /** True for a command kind this plugin applies (its own range, e.g. 0x8500 to 0x85FF for dragons). */
    default boolean ownsCommand(int kind) {
        return false;
    }

    /** The engine's dictionary for this session, as the core received it. */
    @FunctionalInterface
    interface EngineNames {
        /** The name behind an engine dictionary id, or null. */
        String name(int dictKind, int id);
    }

    /**
     * Applies one command of a kind this plugin owns, read with absolute gets from {@code at}. Return
     * false to leave it, and every command after it, for the next tick (a spawn quota,
     * {@link RtsimSession#spendSpawnBudget}); true when it is done (applied, answered or skipped).
     */
    default boolean accept(RtsimSession session, ByteBuffer b, int at, int kind, int size, int aux, EngineNames names) {
        return true;
    }

    /** True when a puppet agent id is this plugin's (its namespace): the plugin decides about its tagged entities. */
    default boolean ownsAgent(long agent) {
        return false;
    }

    /**
     * A tagged entity whose agent this plugin owns joins a level ({@code EntityJoinLevelEvent}, highest
     * priority). True keeps it (a live puppet of this session); false discards it as a stale copy.
     */
    default boolean admit(RtsimSession session, Entity entity, AgentTag tag, Level level) {
        return false;
    }

    /** A tagged entity whose agent this plugin owns left a level ({@code EntityLeaveLevelEvent}). */
    default void left(RtsimSession session, Entity entity, AgentTag tag) {}

    /**
     * Takes a live puppet of this plugin out of the sim (A10): what {@link RtsimApi#takeFromSim} (Carry On's
     * pickup) and the core's taming hook do for a tagged entity whose agent this plugin owns. {@code how}
     * is {@code Obs.Taken}'s code, {@code player} may be null. True when it was a live puppet and is now
     * an ordinary entity; false leaves it alone.
     */
    default boolean take(RtsimSession session, Entity entity, AgentTag tag, int how, java.util.UUID player) {
        return false;
    }

    /** A session opened: Java's dictionary and the plugin's per-session state start here. */
    default void sessionStarted(RtsimSession session) {}

    /** End of every server tick, after the core's own work and before the hand-over. */
    default void endOfTick(RtsimSession session, long now) {}

    /** The server is stopping, before the world save: capture every puppet (SESSION_END), as the core does. */
    default void sessionStopping(RtsimSession session) {}

    /** The bridge failed and stops for this session: capture and remove every puppet. */
    default void sessionFailed(RtsimSession session) {}
}
