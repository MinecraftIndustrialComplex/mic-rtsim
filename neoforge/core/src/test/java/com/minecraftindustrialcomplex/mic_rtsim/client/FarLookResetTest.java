package com.minecraftindustrialcomplex.mic_rtsim.client;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/** A RESET frees, after its grace, only what nobody asked for again (docs/choices-dh-feed.md DHT70). */
class FarLookResetTest {
    private static final long S = 1_000_000_000L;

    @Test
    void whatIsAskedForAgainWithinTheGraceStaysAndTheRestGoes() {
        FarLookReset r = new FarLookReset();
        long t = 5_000 * S;
        r.reset(t);
        assertFalse(r.due(t + S), "not before the grace is over");
        assertTrue(r.due(t + FarLookReset.GRACE_NANOS), "the first frame past it");
        assertFalse(r.due(t + FarLookReset.GRACE_NANOS + 1), "once");
        assertTrue(r.stale(t - S), "drawn before the RESET only");
        assertTrue(r.stale(t), "drawn in the RESET's own frame, before it");
        assertFalse(r.stale(t + 16_000_000L), "drawn again the frame after");
    }

    @Test
    void aSecondResetStartsTheGraceAgainAndNothingIsDueWithoutOne() {
        FarLookReset r = new FarLookReset();
        assertFalse(r.due(Long.MAX_VALUE / 2), "no RESET, nothing due");
        long t = -3 * S; // the clock is System.nanoTime's, which may be negative
        r.reset(t);
        r.reset(t + S);
        assertFalse(r.due(t + FarLookReset.GRACE_NANOS), "the second RESET's grace is not over");
        assertTrue(r.due(t + S + FarLookReset.GRACE_NANOS));
        assertTrue(r.stale(t + S), "measured from the second");
        r.reset(t + 10 * S);
        r.clear();
        assertFalse(r.due(t + 100 * S), "cleared, nothing is due");
    }
}
