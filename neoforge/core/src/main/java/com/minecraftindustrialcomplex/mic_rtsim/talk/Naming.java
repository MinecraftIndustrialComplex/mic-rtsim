package com.minecraftindustrialcomplex.mic_rtsim.talk;

import java.util.random.RandomGenerator;

/**
 * Named or described (TALK.md §6, TK29): whether a ref is told by its name or by what it is and where.
 * Pure: the caller says what makes a ref "always named" and where it is, and brings the random.
 */
public final class Naming {
    private Naming() {}

    /** A test's override of the chance (TK52): every ref that can be described, or every ref that has a name. */
    public enum Force { NONE, NAMED, DESCRIBED }

    /** Under this many blocks a ref is named (a short walk). TUNABLE. */
    public static final int ALWAYS_BLOCKS = 600;
    /** Under this many blocks, and past the one above, a ref is named half the time (a day's ride). */
    public static final int HALF_BLOCKS = 3000;
    public static final double HALF = 0.5, QUARTER = 0.25;

    /**
     * Whether a ref is named. {@code always}: the listener, another player, the holder's own village or camp,
     * anything in a SEEN item. {@code hasPos}, {@code dx}, {@code dz}: where it is from the talker. A ref with no place
     * is as good as far. The random is drawn only when the chance decides.
     */
    public static boolean named(boolean always, boolean hasPos, int dx, int dz, RandomGenerator rnd) {
        if (always)
            return true;
        if (hasPos) {
            double d = TalkTuning.distance(dx, dz);
            if (d < ALWAYS_BLOCKS)
                return true;
            if (d < HALF_BLOCKS)
                return rnd.nextDouble() < HALF;
        }
        return rnd.nextDouble() < QUARTER;
    }

    /** A described ref standing within this many blocks of the report's own place drops its locator. */
    public static final int SAME_PLACE = 24;

    /** Whether a ref's place is the report's place, for dropping the locator. */
    public static boolean atReport(int refDx, int refDz, int newsDx, int newsDz) {
        return TalkTuning.distance(refDx - newsDx, refDz - newsDz) < SAME_PLACE;
    }
}
