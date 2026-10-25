package com.minecraftindustrialcomplex.mic_rtsim_iceandfire;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import net.minecraft.core.BlockPos;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;

import org.junit.jupiter.api.Test;

/**
 * NEST_RENEW applied once per serial (docs/replay-gate.md §7.1, RP-14, RP-19): the engine numbers each nest's
 * renewals, and a nest owes the worms of a renewal only when its serial is above the last one applied under the
 * same engine life. Today's {@code owed += count} owes a renewal the engine decides again twice (the slice U audit's
 * D5: engine 1 worm, world 2).
 */
class NestRenewalSerialTest {
    private static final long LIFE = 0x5EED_1234L;

    private static Nests.Nest nest() {
        return new Nests.Nest(0x0001_0000_0000_0007L, "minecraft:overworld", new BlockPos(100, 20, 200));
    }

    /** A renewal of {@code count} worms at stage 2, at game time 5,000. */
    private static boolean renew(Nests.Nest n, long life, long serial, int count) {
        return n.renewed(life, serial, count, 2, 5_000);
    }

    @Test
    void aRepeatedSerialAddsNothing() {
        Nests.Nest n = nest();
        assertTrue(renew(n, LIFE, 1, 2), "the first brood");
        assertEquals(2, n.owed);
        // The engine decided it again (a restore, or a record the pages still queue): the same serial.
        assertFalse(renew(n, LIFE, 1, 2));
        assertFalse(renew(n, LIFE, 1, 2));
        assertEquals(2, n.owed, "owed once");
        // A renewal below the last applied is a repeat too: lose, never double.
        assertTrue(renew(n, LIFE, 3, 1));
        assertFalse(renew(n, LIFE, 2, 1));
        assertEquals(3, n.owed);
        assertEquals(3, n.serial);
    }

    @Test
    void aHigherSerialAddsItsCount() {
        Nests.Nest n = nest();
        assertTrue(renew(n, LIFE, 1, 2));
        assertTrue(renew(n, LIFE, 2, 1));
        assertEquals(3, n.owed);
        // A gap is a lost renewal (direction A: the pages newer than the Nests): the next one is still owed.
        assertTrue(renew(n, LIFE, 5, 1));
        assertEquals(4, n.owed);
        assertEquals(5, n.serial);
        assertEquals(LIFE, n.life);
        // The serial is an unsigned 32 bits on the wire.
        assertTrue(renew(n, LIFE, 0xFFFF_FFF0L, 1));
        assertFalse(renew(n, LIFE, 0xFFFF_FFF0L, 1));
        assertEquals(5, n.owed);
    }

    @Test
    void twoRenewalsInOneBatchAreBothOwedAndOnlyInOrder() {
        // The engine's catch-up can decide two of a nest's renewals in one tick (one per replayed day): serials 1 and 2
        // arrive back to back, and both are owed.
        Nests.Nest n = nest();
        assertTrue(renew(n, LIFE, 1, 2));
        assertTrue(renew(n, LIFE, 2, 1));
        assertEquals(3, n.owed);
        // The wire is one ordered queue, so they never arrive the other way round. Were they to, the lower one is
        // dropped, a loss and never a double (RP-233).
        Nests.Nest m = nest();
        assertTrue(renew(m, LIFE, 2, 1));
        assertFalse(renew(m, LIFE, 1, 2));
        assertEquals(1, m.owed);
    }

    @Test
    void aRenewalWithNoSerialIsNeverApplied() {
        Nests.Nest n = nest();
        assertFalse(renew(n, LIFE, 0, 2), "the engine's first serial is 1");
        assertEquals(0, n.owed);
    }

    @Test
    void aNewEngineLifeRestartsTheSerialAndKeepsWhatIsOwed() {
        Nests.Nest n = nest();
        assertTrue(renew(n, LIFE, 6, 2));
        assertFalse(renew(n, LIFE, 6, 2));
        assertEquals(2, n.owed);
        // The pages started over (RG13): the new engine counts this nest's renewals from 1 again.
        long other = LIFE + 1;
        assertTrue(renew(n, other, 1, 1), "serial 1 of a new life is new, though 6 was applied under the old one");
        assertEquals(3, n.owed, "what was owed stays");
        assertEquals(other, n.life);
        assertEquals(1, n.serial);
        assertFalse(renew(n, other, 1, 1));
        assertTrue(renew(n, other, 2, 1));
        assertEquals(4, n.owed);
        // A repeat under a new life still moves the nest to that life, and adds nothing.
        Nests.Nest m = nest();
        assertTrue(renew(m, LIFE, 3, 1));
        assertFalse(renew(m, other, 0, 1));
        assertEquals(other, m.life);
        assertEquals(0, m.serial);
        assertEquals(1, m.owed);
    }

    @Test
    void aRepeatLeavesTheBrokenHeartsReformAlone() {
        Nests.Nest n = nest();
        assertTrue(renew(n, LIFE, 1, 2));
        n.broken = true;
        n.owed = 0;
        assertFalse(renew(n, LIFE, 1, 2));
        assertFalse(n.reform, "a repeat re-forms nothing");
        assertTrue(renew(n, LIFE, 2, 1));
        assertTrue(n.reform, "a new renewal re-forms a broken heart, as before");
    }

    @Test
    void nextSerialIsTheOneAboveTheLastUnderTheSameLife() {
        Nests.Nest n = nest();
        assertEquals(1, n.nextSerial(LIFE));
        assertTrue(renew(n, LIFE, n.nextSerial(LIFE), 1));
        assertEquals(2, n.nextSerial(LIFE));
        assertEquals(1, n.nextSerial(LIFE + 1), "under another life it starts again");
    }

    @Test
    void theSerialAndItsLifeAreSavedWithTheOwedCount() {
        Nests.Data d = new Nests.Data();
        Nests.Nest n = nest();
        assertTrue(renew(n, LIFE, 4, 2));
        d.placed.put(n.id, n);
        CompoundTag tag = d.save(new CompoundTag(), null);
        Nests.Data back = Nests.Data.load(tag, null);
        Nests.Nest m = back.placed.get(n.id);
        assertEquals(2, m.owed);
        assertEquals(4, m.serial);
        assertEquals(LIFE, m.life);
        // After the reload the same renewal is still a repeat (a restart of Java alone), and the next is new.
        assertFalse(renew(m, LIFE, 4, 2));
        assertTrue(renew(m, LIFE, 5, 1));
        assertEquals(3, m.owed);
        // A negative life (every bit of the draw is kept, RG13) round trips.
        Nests.Nest k = nest();
        assertTrue(renew(k, -2L, 1, 1));
        Nests.Data kd = new Nests.Data();
        kd.placed.put(k.id, k);
        assertEquals(-2L, Nests.Data.load(kd.save(new CompoundTag(), null), null).placed.get(k.id).life);
    }

    @Test
    void anOldFileLoadsOwedZeroAndSerialZero() {
        // A file written before the serial: the nest has owed and stage, and no serial or life.
        Nests.Data d = new Nests.Data();
        Nests.Nest n = nest();
        n.owed = 3;
        n.stage = 2;
        n.dueAt = 777;
        d.placed.put(n.id, n);
        CompoundTag tag = d.save(new CompoundTag(), null);
        for (var e : tag.getList("placed", 10)) {
            CompoundTag t = (CompoundTag) e;
            t.remove("serial");
            t.remove("life");
        }
        Nests.Nest m = Nests.Data.load(tag, null).placed.get(n.id);
        assertEquals(0, m.owed, "whatever was owed is dropped, never doubled");
        assertEquals(0, m.serial);
        assertEquals(0, m.life);
        assertEquals(2, m.stage, "the rest of the row is kept");
        assertEquals(777, m.dueAt);
        // Its first renewal under any life is new: the nest owes it once.
        assertTrue(renew(m, LIFE, 1, 2));
        assertEquals(2, m.owed);
        // A file with only one of the two keys is an old file too.
        CompoundTag half = d.save(new CompoundTag(), null);
        for (var e : half.getList("placed", 10))
            ((CompoundTag) e).remove("life");
        assertEquals(0, Nests.Data.load(half, null).placed.get(n.id).owed);
        // No placed list at all loads empty.
        assertTrue(Nests.Data.load(new CompoundTag(), null).placed.isEmpty());
        assertEquals(new ListTag(), new CompoundTag().getList("placed", 10));
    }
}
