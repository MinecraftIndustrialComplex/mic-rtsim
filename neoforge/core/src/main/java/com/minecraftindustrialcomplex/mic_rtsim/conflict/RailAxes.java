package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.phys.Vec3;

/**
 * What a mod that lays rail knows and the core does not (M8, MZB-360): which way the rail runs at a point. The core
 * puts a lurking body on the rail's normal ({@link LurkSpots}) and cannot name Create, so the Create plugin registers
 * one ({@code RtsimApi.registerRailAxes}).
 */
@FunctionalInterface
public interface RailAxes {
    /**
     * The direction the rail runs in at {@code point} (the lurk's track point, on the rail), as a horizontal vector
     * with y 0 (any length, either sign), or null when no rail is known there, the chunk is not loaded, or the
     * rail's direction is ambiguous there (a crossing). Called on the server thread, rarely (once per body held).
     */
    Vec3 at(ServerLevel level, Vec3 point);
}
