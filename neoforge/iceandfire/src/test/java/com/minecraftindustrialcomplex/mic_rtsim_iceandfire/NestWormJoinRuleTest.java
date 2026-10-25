package com.minecraftindustrialcomplex.mic_rtsim_iceandfire;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import net.minecraft.core.BlockPos;
import net.minecraft.nbt.CompoundTag;

import org.junit.jupiter.api.Test;

import java.util.UUID;

/**
 * A nest worm joining a level against its nest's books (RP-242, the slice U audit's D6): each worm carries the count the
 * nest had spawned when it was placed, and a worm the nest does not list is a stale copy (below the nest's count: lose,
 * never double) or one a later session spawned whose count the nest's file lost (at or above it: adopted).
 */
class NestWormJoinRuleTest {
    private static Nests.Nest nest() {
        return new Nests.Nest(0x0001_0000_0000_0007L, "minecraft:overworld", new BlockPos(100, 20, 200));
    }

    /** What the nest's own spawn does, in the order {@code Nests.spawnWorm} does it: list it and count it before it joins. */
    private static UUID spawn(Nests.Nest n) {
        UUID u = UUID.randomUUID();
        n.worms.add(u);
        n.spawned++;
        n.owed--;
        return u;
    }

    @Test
    void aWormTheNestHoldsIsNormalWhateverItsOrdinal() {
        Nests.Nest n = nest();
        n.owed = 2;
        UUID a = spawn(n), b = spawn(n);
        assertEquals(Nests.Nest.Join.NORMAL, n.judge(a, 0));
        assertEquals(Nests.Nest.Join.NORMAL, n.judge(b, 1));
        // Its own join runs the rule before the spawn returns: the worm is listed already, never discarded by its own nest.
        assertEquals(2, n.spawned);
    }

    @Test
    void aWormWithoutAnOrdinalKeepsTodaysBehaviour() {
        Nests.Nest n = nest();
        n.spawned = 5;
        assertEquals(Nests.Nest.Join.NORMAL, n.judge(UUID.randomUUID(), -1));
        assertTrue(n.countsLoss(UUID.randomUUID(), -1), "its death is the nest's loss, as it always was");
    }

    @Test
    void aCopyBelowTheNestsCountIsStale() {
        // The brood's worm 0 was spawned, the entity file kept it, and the nest's file was restored from before: the nest
        // spawned an ordinal 0 again, and now the old file's copy joins.
        Nests.Nest n = nest();
        n.owed = 1;
        UUID second = spawn(n);
        UUID copy = UUID.randomUUID();
        assertEquals(Nests.Nest.Join.STALE, n.judge(copy, 0));
        assertEquals(Nests.Nest.Join.NORMAL, n.judge(second, 0), "the nest's own worm of that ordinal is still the normal case");
        assertEquals(1, n.worms.size());
    }

    @Test
    void aWormThatDiedIsStaleToo() {
        Nests.Nest n = nest();
        n.owed = 1;
        UUID w = spawn(n);
        n.worms.remove(w); // it died: Nests.onDeath took it out of the list; the entity file from before still holds it
        assertEquals(Nests.Nest.Join.STALE, n.judge(w, 0));
    }

    @Test
    void aWormTheNestsFileLostIsAdoptedAndTakesOneFromOwed() {
        // The nest's file is from before the spawn (owed 1, spawned 0); the entity file holds the worm.
        Nests.Nest n = nest();
        n.owed = 1;
        UUID w = UUID.randomUUID();
        assertEquals(Nests.Nest.Join.ADOPT, n.judge(w, 0));
        n.adopt(w, 0);
        assertEquals(0, n.owed, "no worm is spawned again");
        assertEquals(1, n.spawned);
        assertTrue(n.worms.contains(w));
        assertEquals(Nests.Nest.Join.NORMAL, n.judge(w, 0), "listed now: the next join is the normal case");
    }

    @Test
    void owedNeverGoesBelowZeroAndAGapStaysAGap() {
        Nests.Nest n = nest();
        n.owed = 0; // a worm spawned past the file's brood
        UUID w = UUID.randomUUID();
        n.adopt(w, 4);
        assertEquals(0, n.owed);
        assertEquals(5, n.spawned, "spawned = ordinal + 1: ordinals 0 to 3 stay a gap");
        // The gap's copies are stale when they load.
        assertEquals(Nests.Nest.Join.STALE, n.judge(UUID.randomUUID(), 2));
        // An older ordinal adopted after a higher one does not lower the count.
        n.adopt(UUID.randomUUID(), 1);
        assertEquals(5, n.spawned);
    }

    @Test
    void aBroodOfThreeRestoredFromBeforeKeepsTheCountRight() {
        // The file says 3 owed and 0 spawned; the world holds worms 0, 1, 2. Worm 2 joins first.
        Nests.Nest n = nest();
        n.owed = 3;
        UUID w0 = UUID.randomUUID(), w1 = UUID.randomUUID(), w2 = UUID.randomUUID();
        assertEquals(Nests.Nest.Join.ADOPT, n.judge(w2, 2));
        n.adopt(w2, 2);
        assertEquals(Nests.Nest.Join.STALE, n.judge(w0, 0), "worm 0 is below the count now");
        assertEquals(Nests.Nest.Join.STALE, n.judge(w1, 1));
        // The two discarded copies are still owed, so the count (listed + owed) is the 3 the engine asked for.
        assertEquals(2, n.owed);
        assertEquals(3, n.worms.size() + n.owed);
    }

    @Test
    void theNestSpawnsOnlyWhatIsStillOwedAfterTheWormJoinedFirst() {
        // The worm's join (adopt) comes before the nest's tick spawns the owed worm: nothing is owed, nothing is spawned.
        Nests.Nest n = nest();
        n.owed = 1;
        UUID w = UUID.randomUUID();
        n.adopt(w, 0);
        int spawnedNow = 0;
        while (n.owed > 0) {
            spawn(n);
            spawnedNow++;
        }
        assertEquals(0, spawnedNow);
        assertEquals(1, n.worms.size());
    }

    @Test
    void theNestSpawnsFirstAndTheOldCopyIsDiscarded() {
        // The other order: the tick spawns the owed worm before the old copy loads. The copy is stale; one worm in all.
        Nests.Nest n = nest();
        n.owed = 1;
        UUID fresh = spawn(n);
        UUID oldCopy = UUID.randomUUID();
        assertEquals(Nests.Nest.Join.STALE, n.judge(oldCopy, 0));
        assertEquals(Nests.Nest.Join.NORMAL, n.judge(fresh, 0));
        assertEquals(1, n.worms.size());
        assertEquals(0, n.owed);
    }

    @Test
    void anUnlistedWormOfAnOrdinalIsNoLossOfTheNests() {
        Nests.Nest n = nest();
        n.spawned = 3;
        assertFalse(n.countsLoss(UUID.randomUUID(), 1), "a copy the nest never counted dies: journal nothing");
        UUID mine = UUID.randomUUID();
        n.worms.add(mine);
        assertTrue(n.countsLoss(mine, 1));
    }

    @Test
    void theSpawnedCountSurvivesASaveAndAnOldFileLoadsZero() {
        Nests.Data d = new Nests.Data();
        Nests.Nest n = nest();
        n.owed = 2;
        n.spawned = 7;
        n.worms.add(UUID.randomUUID());
        d.placed.put(n.id, n);
        CompoundTag tag = d.save(new CompoundTag(), null);
        Nests.Data back = Nests.Data.load(tag, null);
        assertEquals(7, back.placed.get(n.id).spawned);
        assertEquals(2, back.placed.get(n.id).owed);
        // A file from before the count: no key at all.
        CompoundTag old = d.save(new CompoundTag(), null);
        old.getList("placed", 10).getCompound(0).remove("spawned");
        assertEquals(0, Nests.Data.load(old, null).placed.get(n.id).spawned);
    }
}
