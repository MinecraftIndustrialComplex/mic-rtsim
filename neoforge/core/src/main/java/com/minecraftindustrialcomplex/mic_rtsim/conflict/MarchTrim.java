package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import java.util.List;

/**
 * The trim's choice (MZB-473): which member of a band the engine has already lost is cut, as pure numbers.
 *
 * <p>The books lose members no player saw go (an ambush, a raid resolved while watched from afar, a unit left behind),
 * and the march record says how many of each body the band has. The trim cuts the bridge's extra members, rearmost
 * first. A member of a pending stack was never in the world, so the rearmost of those goes first. Then, among placed
 * stacks, the rearmost member that no player can see: nothing is ever cut in a player's sight. When every member left
 * to cut is in sight there is no choice, and the trim waits.
 *
 * <p>"Rearmost" is by the stack's {@code back} in the column, then its index, then the member's index in the stack,
 * each the highest first.
 */
final class MarchTrim {
    private MarchTrim() {}

    /**
     * One member that might be cut. {@code pending}: its stack is pending (not in the world). {@code unseen}: for a placed
     * member, no non-spectator player is within the view distance of its body (or it has none).
     */
    record Candidate(int stack, int slot, double back, boolean pending, boolean unseen) {}

    /**
     * Whether a player {@code dx, dz} blocks away on the ground plane is within {@code range} of an entity. Vanilla's
     * tracking (and so a client's sight) measures horizontally: a player high above a band sees it as one at its own
     * height would (MZB-753), so the height is no part of the distance.
     */
    static boolean withinSight(double dx, double dz, double range) {
        return dx * dx + dz * dz <= range * range;
    }

    /** The index in {@code candidates} of the member to cut now, or -1 when none may be (the trim waits). */
    static int pick(List<Candidate> candidates) {
        int best = -1;
        for (int i = 0; i < candidates.size(); i++) {
            Candidate c = candidates.get(i);
            if (!c.pending() && !c.unseen())
                continue;
            if (best < 0 || rearer(c, candidates.get(best)))
                best = i;
        }
        return best;
    }

    /** Whether {@code a} is cut before {@code b}: pending before placed, then the rearmost. */
    private static boolean rearer(Candidate a, Candidate b) {
        if (a.pending() != b.pending())
            return a.pending();
        if (a.back() != b.back())
            return a.back() > b.back();
        if (a.stack() != b.stack())
            return a.stack() > b.stack();
        return a.slot() > b.slot();
    }
}
