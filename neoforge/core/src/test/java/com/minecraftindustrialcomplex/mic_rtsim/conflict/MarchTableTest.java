package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import com.minecraftindustrialcomplex.mic_rtsim.conflict.MarchTable.Outcome;
import org.junit.jupiter.api.Test;

import java.util.HashMap;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** The march map's rules (MZB-468): latest wins by revision, ENDED drops the path and the agent's later records. */
class MarchTableTest {
    private static final long A = (ConflictRecords.AGENT_NAMESPACE << 56) | 5;
    private static final long B = (ConflictRecords.AGENT_NAMESPACE << 56) | 6;

    private static MarchPath rev(long agent, int revision, int flags) {
        return new MarchPath(agent, revision, "minecraft:overworld", 77, null, 3, ConflictRecords.March.OUT, flags, 0, 0,
                new HashMap<>(), new int[] {0, 10}, new int[] {0, 0}, new long[] {0, 100});
    }

    @Test
    void aNewerRevisionReplacesAndAnOlderOrEqualOneIsDropped() {
        MarchTable t = new MarchTable();
        MarchPath one = rev(A, 1, 0);
        MarchPath three = rev(A, 3, 0);
        assertEquals(Outcome.KEPT, t.offer(one));
        assertSame(one, t.get(A));
        assertEquals(Outcome.KEPT, t.offer(three));
        assertSame(three, t.get(A));
        assertEquals(Outcome.STALE, t.offer(rev(A, 2, 0)));
        assertEquals(Outcome.STALE, t.offer(rev(A, 3, 0)));
        assertSame(three, t.get(A));
        assertEquals(1, t.size());
    }

    @Test
    void endedDropsThePathAndEveryLaterRecordOfThatAgentOnly() {
        MarchTable t = new MarchTable();
        t.offer(rev(A, 1, 0));
        t.offer(rev(B, 1, 0));
        assertEquals(Outcome.ENDED, t.offer(rev(A, 2, ConflictRecords.March.FLAG_ENDED)));
        assertNull(t.get(A));
        assertTrue(t.ended(A));
        assertFalse(t.ended(B));
        // Later records of the ended agent, newer or not, are dropped; the other agent's march is untouched.
        assertEquals(Outcome.STALE, t.offer(rev(A, 3, 0)));
        assertEquals(Outcome.STALE, t.offer(rev(A, 9, ConflictRecords.March.FLAG_ENDED)));
        assertNull(t.get(A));
        assertEquals(1, t.size());
        assertEquals(Outcome.KEPT, t.offer(rev(B, 2, 0)));
    }

    @Test
    void anEndedRecordOlderThanTheHeldPathDoesNotEnd() {
        MarchTable t = new MarchTable();
        MarchPath three = rev(A, 3, 0);
        t.offer(three);
        assertEquals(Outcome.STALE, t.offer(rev(A, 2, ConflictRecords.March.FLAG_ENDED)));
        assertSame(three, t.get(A));
        assertFalse(t.ended(A));
    }

    @Test
    void anEndedRecordWithNoHeldPathStillRemembersTheAgent() {
        MarchTable t = new MarchTable();
        assertEquals(Outcome.ENDED, t.offer(rev(A, 1, ConflictRecords.March.FLAG_ENDED)));
        assertTrue(t.ended(A));
        assertEquals(Outcome.STALE, t.offer(rev(A, 2, 0)));
    }

    @Test
    void aClearForgetsBothTheMarchesAndTheEnded() {
        MarchTable t = new MarchTable();
        t.offer(rev(A, 1, 0));
        t.offer(rev(B, 1, ConflictRecords.March.FLAG_ENDED));
        assertFalse(t.isEmpty());
        t.clear();
        assertTrue(t.isEmpty());
        assertNull(t.get(A));
        assertFalse(t.ended(B));
        // A new session's revision 1 is kept again.
        assertEquals(Outcome.KEPT, t.offer(rev(A, 1, 0)));
        assertEquals(Outcome.KEPT, t.offer(rev(B, 1, 0)));
    }

    @Test
    void aWrappedRevisionIsNewerInUnsignedOrder() {
        MarchTable t = new MarchTable();
        t.offer(rev(A, Integer.MAX_VALUE, 0));
        assertEquals(Outcome.KEPT, t.offer(rev(A, Integer.MIN_VALUE, 0)));
    }
}
