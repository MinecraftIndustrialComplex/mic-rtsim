package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.Vec3;

/**
 * One war party lying in wait beside a rail (M8, MZB-359): the engine's AMBUSH_SPOT, kept by the bridge
 * until its AMBUSH_REMOVED, the next AMBUSH_SPOT of the agent or the session's end. {@code track} is the
 * sampled track point a train must pass within {@code reach} blocks of; {@code spot} is where the body
 * stands (at the track's y: the bridge finds the floor, MZB-360). Held by agent, never by epoch: a lurk
 * the engine saw (a player came on it) has its passes carry this epoch against the party's next one.
 */
public record Lurk(long agent, int epoch, long faction, ResourceKey<Level> dimension, Vec3 track, Vec3 spot, float reach) {}
