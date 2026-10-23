package com.minecraftindustrialcomplex.mic_rtsim_dh;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/** Who draws a far mover: vanilla, the band, DH, or the band and DH together in DH's vanilla fade. */
class HandOverRuleTest {
    /** Render distance 10 on a 16:9 screen: DH's near clip plane is about 73 blocks, so the fade starts at 134. */
    private static final double START = (73.0 + 16.0) * 1.5;
    private static final double REACH = 10.0;

    @Test
    void theRealThingTakesOverEverywhere() {
        assertFalse(HandOverRule.dh(true, false, 500, REACH, START));
        assertFalse(HandOverRule.dh(true, true, START + 50, REACH, START));
        assertFalse(HandOverRule.band(true, true));
    }

    @Test
    void outsideTheLoadedChunksOnlyDhDraws() {
        assertTrue(HandOverRule.dh(false, false, 40, REACH, START), "near but unloaded, as on a teleport");
        assertTrue(HandOverRule.dh(false, false, 300, REACH, START));
        assertFalse(HandOverRule.band(false, false));
    }

    @Test
    void wellInsideTheFadeStartOnlyTheBandDraws() {
        double near = START - REACH - HandOverRule.SLACK - HandOverRule.PREPARE - 0.5;
        assertFalse(HandOverRule.dh(false, true, near, REACH, START));
        assertTrue(HandOverRule.band(false, true));
    }

    @Test
    void dhDrawsAsSoonAsAnyPartCouldBeInTheFadeWithTheMarginsBefore() {
        // The far end of a carriage 10 blocks long each way reaches the fade start before its midpoint does
        assertTrue(HandOverRule.dh(false, true, START - REACH, REACH, START));
        assertTrue(HandOverRule.dh(false, true, START - REACH - HandOverRule.SLACK - HandOverRule.PREPARE, REACH,
                START), "the prepare margin, for DH's off-thread build of a new group");
        assertTrue(HandOverRule.dh(false, true, START + 30, REACH, START), "past the fade's end, still loaded");
        assertTrue(HandOverRule.band(false, true), "the band draws on beside it, into the fade");
    }

    @Test
    void beforeTheFirstFrameTheRingIsNowhereAndTheOldRuleHolds() {
        // FadeRing.start() is infinite until DH's controller first runs: loaded chunks are the band's alone
        assertFalse(HandOverRule.dh(false, true, 10_000, REACH, Double.POSITIVE_INFINITY));
        assertTrue(HandOverRule.dh(false, false, 10_000, REACH, Double.POSITIVE_INFINITY));
    }

    @Test
    void aDragonsBodyIsCopiedIntoDhWhereDhWouldDrawTheDragon() {
        double edge = START - REACH - HandOverRule.SLACK - HandOverRule.PREPARE;
        assertFalse(HandOverRule.bodyCopied(true, edge - 0.01, REACH, START), "well short of the fade: vanilla's alone");
        assertTrue(HandOverRule.bodyCopied(true, edge, REACH, START), "any part of it could soon be faded");
        assertTrue(HandOverRule.bodyCopied(true, START + 20, REACH, START), "in the ring, as at the client's tracking edge");
        assertTrue(HandOverRule.bodyCopied(false, 40, REACH, START), "its chunk not loaded: vanilla leaves it out");
        assertFalse(HandOverRule.bodyCopied(true, 10_000, REACH, Double.POSITIVE_INFINITY), "no fade: vanilla's alone");
        assertFalse(HandOverRule.band(true, true), "the band never draws it");
    }
}
