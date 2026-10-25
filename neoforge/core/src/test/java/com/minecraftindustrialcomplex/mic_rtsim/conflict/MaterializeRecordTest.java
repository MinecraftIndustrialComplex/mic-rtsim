package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** BAND_MATERIALIZE's one valid record with no stacks (MZB-728). */
class MaterializeRecordTest {
    private static final int HOSTILE = ConflictRecords.Materialize.FLAG_HOSTILE;
    private static final int FIGHT = ConflictRecords.Materialize.FLAG_FIGHT;

    @Test
    void aFightFlaggedRecordWithNoStacksIsTheEmptyFight() {
        assertTrue(ConflictRecords.Materialize.emptyFight(0, FIGHT));
        assertTrue(ConflictRecords.Materialize.emptyFight(0, FIGHT | HOSTILE));
    }

    @Test
    void aRecordWithNoStacksAndNoFightFlagIsStillBad() {
        assertFalse(ConflictRecords.Materialize.emptyFight(0, 0));
        assertFalse(ConflictRecords.Materialize.emptyFight(0, HOSTILE));
    }

    @Test
    void aRecordWithStacksIsNotTheEmptyFight() {
        assertFalse(ConflictRecords.Materialize.emptyFight(1, FIGHT));
        assertFalse(ConflictRecords.Materialize.emptyFight(-1, FIGHT));
    }

    @Test
    void theEmptyRecordIsJustTheHeader() {
        // 120 bytes: nothing follows the speed and the reserved word, and a record of that size passes the length check.
        assertEquals(120, ConflictRecords.Materialize.size(0));
        assertEquals(ConflictRecords.Materialize.STACKS, ConflictRecords.Materialize.size(0));
    }
}
