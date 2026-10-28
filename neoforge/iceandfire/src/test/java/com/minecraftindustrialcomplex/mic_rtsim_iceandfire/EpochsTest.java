package com.minecraftindustrialcomplex.mic_rtsim_iceandfire;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/**
 * DRAGON_ADOPT's supersede rule (RP-245, the slice U audit's D8): the puppet an agent holds at a strictly older epoch than
 * the adopt's is a body the engine no longer has, and is released; the same or a newer epoch keeps ALREADY_EMBODIED.
 */
class EpochsTest {
    @Test
    void onlyAStrictlyNewerEpochSupersedes() {
        assertTrue(Epochs.supersedes(1, 2), "e2 above e1");
        assertTrue(Epochs.supersedes(5, 9));
        assertFalse(Epochs.supersedes(2, 2), "the same epoch is a repeat");
        assertFalse(Epochs.supersedes(3, 2), "an older epoch is a late command");
    }

    @Test
    void theEpochIsUnsigned() {
        // The wire's epoch is an unsigned 32 bits: 0x80000000 is above 1, not below it.
        assertTrue(Epochs.supersedes(1, 0x8000_0000));
        assertFalse(Epochs.supersedes(0x8000_0000, 1));
        assertTrue(Epochs.supersedes(0x7FFF_FFFF, 0x8000_0000));
        assertFalse(Epochs.supersedes(-1, -1));
    }
}
