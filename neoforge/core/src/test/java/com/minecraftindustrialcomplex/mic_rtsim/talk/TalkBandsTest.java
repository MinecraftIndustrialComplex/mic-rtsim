package com.minecraftindustrialcomplex.mic_rtsim.talk;

import org.junit.jupiter.api.Test;

import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** The bands and weights news is told with (TALK.md §5.3, §5.6, TK4, TK29, TK50, TK51): all pure. */
class TalkBandsTest {
    private static final long DAY = TalkTuning.DAY;

    @Test
    void whenBandsChangeAtTheirEdges() {
        assertEquals("today", TalkTuning.when(0));
        assertEquals("today", TalkTuning.when(DAY - 1));
        assertEquals("yesterday", TalkTuning.when(DAY));
        assertEquals("yesterday", TalkTuning.when(2 * DAY - 1));
        assertEquals("days", TalkTuning.when(2 * DAY));
        assertEquals("days", TalkTuning.when(4 * DAY - 1));
        assertEquals("week", TalkTuning.when(4 * DAY));
        assertEquals("week", TalkTuning.when(10 * DAY - 1));
        assertEquals("weeks", TalkTuning.when(10 * DAY));
        assertEquals("weeks", TalkTuning.when(20 * DAY - 1));
        assertEquals("long", TalkTuning.when(20 * DAY));
        assertEquals("long", TalkTuning.when(0xFFFF_FFFFL));
    }

    @Test
    void whereBandsChangeAtTheirEdges() {
        assertEquals("here", TalkTuning.where(0, 0));
        assertEquals("here", TalkTuning.where(23, 0));
        assertEquals("close", TalkTuning.where(24, 0));
        assertEquals("close", TalkTuning.where(0, -127));
        assertEquals("walk", TalkTuning.where(128, 0));
        assertEquals("walk", TalkTuning.where(-599, 0));
        assertEquals("ride", TalkTuning.where(600, 0));
        assertEquals("ride", TalkTuning.where(0, 2999));
        assertEquals("far", TalkTuning.where(3000, 0));
        assertEquals("far", TalkTuning.where(Integer.MAX_VALUE, Integer.MIN_VALUE));
        // The distance is Euclidean: 17 each way is 24.04 blocks.
        assertEquals("close", TalkTuning.where(17, 17));
        assertEquals("here", TalkTuning.where(16, 16));
    }

    @Test
    void theCompassIsEightPointsWithNorthAtMinusZ() {
        assertEquals("n", TalkTuning.compass(0, -10));
        assertEquals("ne", TalkTuning.compass(10, -10));
        assertEquals("e", TalkTuning.compass(10, 0));
        assertEquals("se", TalkTuning.compass(10, 10));
        assertEquals("s", TalkTuning.compass(0, 10));
        assertEquals("sw", TalkTuning.compass(-10, 10));
        assertEquals("w", TalkTuning.compass(-10, 0));
        assertEquals("nw", TalkTuning.compass(-10, -10));
        assertEquals("n", TalkTuning.compass(0, 0), "a zero vector reads as north");
        // The edge between north and north-east is 22.5 degrees.
        assertEquals("n", TalkTuning.compass(41, -100));
        assertEquals("ne", TalkTuning.compass(42, -100));
        assertEquals("n", TalkTuning.compass(-41, -100));
        assertEquals("nw", TalkTuning.compass(-42, -100));
    }

    @Test
    void freshnessFallsInBands() {
        assertEquals(1.0, TalkTuning.freshness(0));
        assertEquals(1.0, TalkTuning.freshness(DAY - 1));
        assertEquals(0.75, TalkTuning.freshness(DAY));
        assertEquals(0.75, TalkTuning.freshness(2 * DAY - 1));
        assertEquals(0.5, TalkTuning.freshness(2 * DAY));
        assertEquals(0.5, TalkTuning.freshness(7 * DAY - 1));
        assertEquals(0.25, TalkTuning.freshness(7 * DAY));
    }

    @Test
    void aNewsWeightIsThreeTimesFreshnessTimesTwoForThePlayerTimesOneAndAHalfForUs() {
        assertEquals(3.0, TalkTuning.newsWeight(0, 0), 1e-9);
        assertEquals(6.0, TalkTuning.newsWeight(TalkRecords.YOU_DID, 0), 1e-9);
        assertEquals(6.0, TalkTuning.newsWeight(TalkRecords.TO_YOU, 0), 1e-9);
        assertEquals(4.5, TalkTuning.newsWeight(TalkRecords.TO_US, 0), 1e-9);
        assertEquals(4.5, TalkTuning.newsWeight(TalkRecords.BY_US, 0), 1e-9);
        assertEquals(9.0, TalkTuning.newsWeight(TalkRecords.TO_YOU | TalkRecords.TO_US, 0), 1e-9);
        // Seen is a manner of telling, not a weight.
        assertEquals(3.0, TalkTuning.newsWeight(TalkRecords.SEEN, 0), 1e-9);
        assertEquals(2.25, TalkTuning.newsWeight(0, DAY), 1e-9);
        assertEquals(0.75, TalkTuning.newsWeight(0, 30 * DAY), 1e-9);
        // Both sides of one item (the player did it to us) do not stack past one factor each.
        assertEquals(9.0, TalkTuning.newsWeight(TalkRecords.YOU_DID | TalkRecords.TO_YOU | TalkRecords.TO_US | TalkRecords.BY_US, 0), 1e-9);
    }

    @Test
    void theHoldersSideIsToldBeforeTheListenersAndOtherAlwaysLast() {
        assertEquals(List.of("other"), TalkTuning.perspectives(0));
        assertEquals(List.of("other"), TalkTuning.perspectives(TalkRecords.SEEN));
        assertEquals(List.of("us", "other"), TalkTuning.perspectives(TalkRecords.TO_US));
        assertEquals(List.of("by_us", "other"), TalkTuning.perspectives(TalkRecords.BY_US));
        assertEquals(List.of("you", "other"), TalkTuning.perspectives(TalkRecords.YOU_DID));
        assertEquals(List.of("to_you", "other"), TalkTuning.perspectives(TalkRecords.TO_YOU));
        assertEquals(List.of("us", "you", "other"), TalkTuning.perspectives(TalkRecords.TO_US | TalkRecords.YOU_DID));
        assertEquals(List.of("by_us", "to_you", "other"), TalkTuning.perspectives(TalkRecords.BY_US | TalkRecords.TO_YOU));
        assertEquals("us", TalkTuning.perspective(TalkRecords.TO_US | TalkRecords.YOU_DID));
    }

    @Test
    void everyReportKindHasItsNameAndTheGapsHaveNone() {
        String[] expected = {null, "assault", "killing", "robbery", "theft", "trade", "rescue", "resident_assault",
                "resident_killing", "encroachment", "feud", "pact_sworn", "pact_broken", "raid", "fell", null,
                "port_raided", "blockade", "port_closed", "port_reopened", "haven_fell"};
        for (int k = 0; k < expected.length; k++)
            assertEquals(expected[k], TalkTuning.kindName(k), "kind " + k);
        assertNull(TalkTuning.kindName(21));
        assertNull(TalkTuning.kindName(-1));
        assertNull(TalkTuning.kindName(0xFFFF));
    }

    @Test
    void theSafetyKindsAreTheOnesTheSpecNames() {
        for (int k : new int[]{1, 2, 3, 7, 8, 13, 16})
            assertTrue(TalkTuning.isSafetyKind(k), "kind " + k);
        for (int k : new int[]{0, 4, 5, 6, 9, 10, 11, 12, 14, 15, 17, 18, 19, 20, 21})
            assertTrue(!TalkTuning.isSafetyKind(k), "kind " + k);
    }
}
