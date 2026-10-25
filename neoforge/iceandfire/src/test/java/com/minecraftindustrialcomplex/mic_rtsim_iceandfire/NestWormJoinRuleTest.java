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
        return spawnAt(n, null);
    }

    /** The same, returning the ordinal the worm carried in {@code ordinal[0]} (the lowest hole, or the count). */
    private static UUID spawnAt(Nests.Nest n, int[] ordinal) {
        UUID u = UUID.randomUUID();
        int k = n.takeOrdinal();
        if (ordinal != null)
            ordinal[0] = k;
        n.worms.add(u);
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
    void owedNeverGoesBelowZeroAndTheSkippedOrdinalsAreHoles() {
        Nests.Nest n = nest();
        n.owed = 0; // a worm spawned past the file's brood
        UUID w = UUID.randomUUID();
        n.adopt(w, 4);
        assertEquals(0, n.owed);
        assertEquals(5, n.spawned, "spawned = ordinal + 1");
        assertEquals(java.util.List.of(0, 1, 2, 3), java.util.List.copyOf(n.holes), "ordinals 0 to 3 are holes, not stale copies");
        // A hole's worm is adopted when it loads (owed stays at 0), once: the hole is filled.
        UUID late = UUID.randomUUID();
        assertEquals(Nests.Nest.Join.ADOPT, n.judge(late, 2));
        n.adopt(late, 2);
        assertEquals(0, n.owed);
        assertFalse(n.holes.contains(2));
        assertEquals(Nests.Nest.Join.STALE, n.judge(UUID.randomUUID(), 2), "a second copy of the ordinal is stale: the hole is filled");
        // An ordinal below the count that is no hole (it died, or was spawned again) is stale.
        n.holes.clear();
        assertEquals(Nests.Nest.Join.STALE, n.judge(UUID.randomUUID(), 1));
        // An older ordinal adopted after a higher one does not lower the count.
        n.adopt(UUID.randomUUID(), 1);
        assertEquals(5, n.spawned);
    }

    @Test
    void aBroodOfThreeRestoredFromBeforeKeepsAllThreeWhateverTheOrder() {
        // The file says 3 owed and 0 spawned; the world holds worms 0, 1, 2. Worm 2 joins first.
        Nests.Nest n = nest();
        n.owed = 3;
        UUID w0 = UUID.randomUUID(), w1 = UUID.randomUUID(), w2 = UUID.randomUUID();
        assertEquals(Nests.Nest.Join.ADOPT, n.judge(w2, 2));
        n.adopt(w2, 2);
        assertEquals(Nests.Nest.Join.ADOPT, n.judge(w0, 0), "worm 0 is a hole, not a copy below the count");
        n.adopt(w0, 0);
        assertEquals(Nests.Nest.Join.ADOPT, n.judge(w1, 1));
        n.adopt(w1, 1);
        assertEquals(0, n.owed);
        assertEquals(3, n.worms.size(), "all three are kept and none is spawned again");
        assertTrue(n.holes.isEmpty());
        assertEquals(3, n.spawned);
    }

    @Test
    void aWormLoadedBeforeAnOlderOneDoesNotMakeTheOlderStale() {
        // RP-250, the review's P2: N saved at spawned = 3 with 2 owed; worms 3 and 4 were spawned and their entity sections
        // saved without N. Worm 4 loads first and is adopted; worm 3, the only copy, then loads.
        Nests.Nest n = nest();
        n.spawned = 3;
        n.owed = 2;
        UUID w3 = UUID.randomUUID(), w4 = UUID.randomUUID();
        assertEquals(Nests.Nest.Join.ADOPT, n.judge(w4, 4));
        n.adopt(w4, 4);
        assertEquals(5, n.spawned);
        assertEquals(1, n.owed);
        assertEquals(Nests.Nest.Join.ADOPT, n.judge(w3, 3), "the only copy of worm 3 is a hole's, not stale");
        n.adopt(w3, 3);
        assertEquals(0, n.owed, "both worms kept, both taken off what is owed");
        assertEquals(2, n.worms.size());
        assertEquals(5, n.spawned);
        assertTrue(n.holes.isEmpty());
        assertEquals(Nests.Nest.Join.NORMAL, n.judge(w3, 3));
    }

    @Test
    void theNestTakesTheLowestHoleBeforeRaisingTheCount() {
        // A worm 4 adopted, worm 3 not loaded yet, one still owed: the nest spawns it into hole 3, so the old copy of worm 3,
        // should it load now, finds no hole and is stale: the nest holds worms 4 and the new 3, as it was owed two.
        Nests.Nest n = nest();
        n.spawned = 3;
        n.owed = 2;
        n.adopt(UUID.randomUUID(), 4);
        int[] k = {-1};
        UUID fresh = spawnAt(n, k);
        assertEquals(3, k[0], "the lowest hole first");
        assertEquals(5, n.spawned, "the count is not raised while a hole is used");
        assertEquals(0, n.owed);
        assertEquals(Nests.Nest.Join.STALE, n.judge(UUID.randomUUID(), 3), "the old copy of the ordinal spawned again");
        assertEquals(Nests.Nest.Join.NORMAL, n.judge(fresh, 3));
        assertEquals(2, n.worms.size());
        // With no hole left the next spawn raises the count.
        n.owed = 1;
        assertEquals(5, n.takeOrdinal());
        assertEquals(6, n.spawned);
    }

    @Test
    void aSpawnThatCannotJoinGivesItsOrdinalBack() {
        Nests.Nest n = nest();
        n.spawned = 3;
        n.adopt(UUID.randomUUID(), 5); // holes 3, 4; spawned 6
        int k = n.takeOrdinal();
        assertEquals(3, k);
        n.returnOrdinal(k, true);
        assertEquals(java.util.List.of(3, 4), java.util.List.copyOf(n.holes));
        assertEquals(6, n.spawned);
        // With no hole the ordinal is the count, raised, and given back it is the count again.
        n.holes.clear();
        k = n.takeOrdinal();
        assertEquals(6, k);
        assertEquals(7, n.spawned);
        n.returnOrdinal(k, false);
        assertEquals(6, n.spawned);
    }

    @Test
    void theHolesAreBoundedAndTheLowestIsDropped() {
        Nests.Nest n = nest();
        n.adopt(UUID.randomUUID(), 1000);
        assertEquals(1001, n.spawned);
        assertEquals(Nests.Nest.HOLES_MAX, n.holes.size());
        assertEquals(1000 - Nests.Nest.HOLES_MAX, n.holes.first(), "the highest are kept");
        assertEquals(999, n.holes.last());
        // A dropped hole's worm is discarded when it loads: a loss, never a double.
        assertEquals(Nests.Nest.Join.STALE, n.judge(UUID.randomUUID(), 5));
        assertEquals(Nests.Nest.Join.ADOPT, n.judge(UUID.randomUUID(), 999));
        // More holes added later push the lowest out again.
        n.adopt(UUID.randomUUID(), 1010);
        assertEquals(Nests.Nest.HOLES_MAX, n.holes.size());
        assertTrue(n.holes.contains(1009));
    }

    @Test
    void theHolesSurviveASaveAndAnOldFileHasNone() {
        Nests.Data d = new Nests.Data();
        Nests.Nest n = nest();
        n.owed = 2;
        n.adopt(UUID.randomUUID(), 4);
        d.placed.put(n.id, n);
        CompoundTag tag = d.save(new CompoundTag(), null);
        Nests.Nest back = Nests.Data.load(tag, null).placed.get(n.id);
        assertEquals(n.holes, back.holes);
        assertEquals(5, back.spawned);
        // A file without the key (from before it), or one naming an ordinal at or above the count: no hole.
        CompoundTag old = d.save(new CompoundTag(), null);
        old.getList("placed", 10).getCompound(0).remove("holes");
        assertTrue(Nests.Data.load(old, null).placed.get(n.id).holes.isEmpty());
        CompoundTag bad = d.save(new CompoundTag(), null);
        bad.getList("placed", 10).getCompound(0).putIntArray("holes", new int[] {-1, 2, 5, 99});
        assertEquals(java.util.List.of(2), java.util.List.copyOf(Nests.Data.load(bad, null).placed.get(n.id).holes));
    }

    @Test
    void wormsLoadedBeforeTheNestWasRegisteredAreSettledOnce() {
        // RP-251: a nest rebuilt from its chamber (the file lost it): spawned 0, nothing owed. Three worms were loaded with
        // the chunk: ordinals 4, 1 and a second copy of 1.
        Nests.Nest n = nest();
        UUID a = UUID.randomUUID(), b = UUID.randomUUID(), c = UUID.randomUUID();
        Nests.Nest.Settled s = n.settle(java.util.List.of(new Nests.Nest.Loaded(a, 4), new Nests.Nest.Loaded(b, 1),
                new Nests.Nest.Loaded(c, 1)));
        assertEquals(2, s.adopted(), "ordinals 4 and 1, whatever the order");
        assertEquals(java.util.List.of(c), s.stale(), "the second copy of ordinal 1 is discarded");
        assertEquals(2, n.worms.size());
        assertTrue(n.worms.contains(a) && n.worms.contains(b));
        assertEquals(5, n.spawned, "the nest's next worm does not reuse an ordinal a loaded worm carries");
        // The nest's next worm takes a hole (0, 2 or 3): none of those is a loaded worm's.
        int k = n.takeOrdinal();
        assertTrue(k == 0 || k == 2 || k == 3, "got " + k);
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
