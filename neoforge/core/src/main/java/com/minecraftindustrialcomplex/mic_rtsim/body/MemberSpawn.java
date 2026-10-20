package com.minecraftindustrialcomplex.mic_rtsim.body;

import java.util.List;

/**
 * One member of a SPAWN_GROUP command, decoded: health (0 = full), body bits (-1 = let the body roll
 * them) and the cargo to put in it.
 */
public record MemberSpawn(int index, float health, int bodyA, int bodyB, List<MemberCapture.Stack> items) {}
