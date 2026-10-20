package com.minecraftindustrialcomplex.mic_rtsim.body;

import net.minecraft.world.item.Item;

import java.util.ArrayList;
import java.util.List;

/**
 * What a body adapter captured from one member before it is removed or leaves the sim: what the
 * engine needs to rebuild the body later without re-rolling it (the Veloren lesson, PLAN.md §6.1).
 * Encoded as a MEMBER_STATE observation.
 */
public final class MemberCapture {
    /** One stack: adapter slot, item, count. Item components are not captured yet (see README). */
    public record Stack(int slot, Item item, int count) {}

    public float health;
    public float maxHealth;
    public double x, y, z;
    public int flags;
    public int bodyA = -1;
    public int bodyB = -1;
    public final List<Stack> items = new ArrayList<>();
}
