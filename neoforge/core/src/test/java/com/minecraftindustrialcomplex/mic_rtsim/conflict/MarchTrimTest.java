package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import com.minecraftindustrialcomplex.mic_rtsim.conflict.MarchTrim.Candidate;
import org.junit.jupiter.api.Test;

import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Which member the trim cuts (MZB-473). */
class MarchTrimTest {
    private static Candidate placed(int stack, int slot, double back, boolean unseen) {
        return new Candidate(stack, slot, back, false, unseen);
    }

    private static Candidate pending(int stack, int slot, double back) {
        return new Candidate(stack, slot, back, true, false);
    }

    @Test
    void nothingToCutPicksNothing() {
        assertEquals(-1, MarchTrim.pick(List.of()));
    }

    @Test
    void aPendingMemberGoesBeforeAnyPlacedOne() {
        List<Candidate> c = List.of(placed(5, 1, 30.0, true), pending(0, 0, 0.0), placed(4, 0, 20.0, true));
        assertEquals(1, MarchTrim.pick(c));
    }

    @Test
    void amongPendingTheRearmostStackThenItsLastMemberGoes() {
        List<Candidate> c = List.of(pending(0, 1, 0.0), pending(2, 0, 3.0), pending(2, 1, 3.0), pending(1, 1, 1.5));
        assertEquals(2, MarchTrim.pick(c)); // stack 2 is the rearmost, and its slot 1 the last
    }

    @Test
    void amongPlacedTheRearmostUnseenMemberGoesAndASeenOneNeverDoes() {
        List<Candidate> c = List.of(placed(0, 0, 0.0, true), placed(3, 0, 4.5, false), placed(2, 0, 3.0, true),
                placed(2, 1, 3.0, true));
        // The rearmost stack's member is in sight: the next rearmost unseen one is cut.
        assertEquals(3, MarchTrim.pick(c));
    }

    @Test
    void everyMemberInSightMeansTheTrimWaits() {
        List<Candidate> c = List.of(placed(0, 0, 0.0, false), placed(1, 0, 1.5, false), placed(1, 1, 1.5, false));
        assertEquals(-1, MarchTrim.pick(c));
    }

    @Test
    void aTieOnBackIsBrokenByTheHigherStackIndexThenTheHigherSlot() {
        // Packs share a back: the pair at one back is two stacks.
        List<Candidate> c = List.of(placed(4, 0, 6.0, true), placed(5, 0, 6.0, true));
        assertEquals(1, MarchTrim.pick(c));
        List<Candidate> d = List.of(placed(5, 0, 6.0, true), placed(5, 1, 6.0, true));
        assertEquals(1, MarchTrim.pick(d));
    }

    @Test
    void sightIsMeasuredOnTheGroundPlaneNotInThreeDimensions() {
        // A player 150 blocks up and 50 out sees a band at a range of 128: the horizontal distance is 50.
        assertTrue(MarchTrim.withinSight(50.0, 0.0, 128.0));
        assertTrue(MarchTrim.withinSight(30.0, 40.0, 50.0), "the edge is in");
        // One far out at ground level does not: 150 blocks off.
        assertFalse(MarchTrim.withinSight(150.0, 0.0, 128.0));
        assertFalse(MarchTrim.withinSight(100.0, 100.0, 128.0));
    }
}
