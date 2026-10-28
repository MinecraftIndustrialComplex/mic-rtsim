package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import java.util.random.RandomGenerator;

/**
 * Where a GameTest builds when a Mowzie's Mobs body walks a path in it (RailFrostmawGameTests): near enough to the origin
 * that the jar's path shortcut stays exact, and far enough from the spawn and from every other test's area.
 *
 * <p>The jar's {@code MMPathNavigateGround.sweep} (Mowzie's Mobs 1.8.2) steps a grid along the line from a mob to a path
 * node in {@code float}, with the world coordinates of the mob's box as the floats. Above 2^22 blocks (4.2 million) a
 * float is a multiple of 0.5, and above 2^21 of 0.25; a trailing edge advanced by sums of 0.28 or so rounds up to 0.5 at
 * every step, runs past the leading edge, and the loops {@code for (z = z0; z != z1; z += step)} then start beyond their
 * end and count through the whole of the int range: a sweep of 40 blocks loads a line of chunks out to the world border
 * (and past it) inside one server tick. Measured (a copy of the algorithm, 400,000 random sweeps of up to 80 blocks at
 * each size, boxes 0.6 to 5 wide): none up to 340,000 blocks from the origin, 310 at 520,000 to 680,000 (the small
 * boxes), thousands at 2 million and more (1 sweep in 25 at 4.5 million). The create GameTest round at the far bases
 * of {@link ConflictKit#farBase} (3 to 7 million) twice stopped its server thread in {@code sweep} for 15 minutes with
 * 35,000 chunks loaded and the heap past 10 GB: the Frostmaw of {@code theIceBallHitRatesAgainstAMovingTrain}.
 *
 * <p>The slots are 1,024 blocks apart, 128 each way: x from -100,000 to -231,000, z from 100,000 to 231,000, all inside
 * {@link #LIMIT}, which is 2^18.
 */
public final class PathSafeBase {
    private PathSafeBase() {}

    /** The first slot, and the slot count and spacing, as {@link ConflictKit#farBase} has them for its far area. */
    public static final int X0 = -100_000, Z0 = 100_000, SLOTS = 128, STEP = 1_024;

    /** No coordinate of a base, or of the slot's margin round it, is farther than this from the origin (2^18). */
    public static final int LIMIT = 262_144;

    /** A chunk-aligned base block {x, z} in a random slot. */
    public static int[] pick(RandomGenerator random) {
        return new int[] {X0 - random.nextInt(SLOTS) * STEP, Z0 + random.nextInt(SLOTS) * STEP};
    }

    /**
     * Whether the chunk (cx, cz) is in the area the bases lie in, with a slot's margin each side: what a test releases
     * before it forces its own (a failed run leaves forced chunks in the reused world).
     */
    public static boolean areaHas(int cx, int cz) {
        int x0 = (X0 - SLOTS * STEP) >> 4, x1 = (X0 + STEP) >> 4;
        int z0 = (Z0 - STEP) >> 4, z1 = (Z0 + SLOTS * STEP + STEP) >> 4;
        return cx >= x0 && cx <= x1 && cz >= z0 && cz <= z1;
    }
}
