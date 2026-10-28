package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs;

/**
 * The one sweep of MZB-950 that is known to run away in the jar's float walk (found by fuzzing a copy of it): a Frostmaw's box,
 * 3.5 wide and 4.5 tall, at the far base of the ice-rates test's second run, a path node 38.6 blocks off. The unit tests show
 * that the copy of the jar's walk counts past 400,000 cells on it and the shipped walk reads under 10,000; the GameTest
 * ({@code PathSweepGameTests}) builds its far place round these numbers, so that the jar's real {@code sweep} meets the same
 * sweep with the mixin on. A float at 2^21 to 2^22 is a multiple of 0.25, and one at 2^22 to 2^23 of 0.5: the sweep runs
 * away at any base moved by whole blocks within those ranges (the float's roundings fall the same way).
 */
public final class PathSweepRunaway {
    private PathSweepRunaway() {}

    /** The box's middle on x and z, its width and height. */
    public static final double X = -3_797_619.272, Z = 4_715_190.381, WIDTH = 3.5, HEIGHT = 4.5;

    /** The vector from the mob to the path node: level, 37.3 blocks west and 10.1 south. */
    public static final double VX = -37.251516399504, VY = 0.0, VZ = 10.136477625167588;
}
