package com.minecraftindustrialcomplex.mic_rtsim.sites;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** The in and out tracker over a fake chunk source (EQUILIBRIUM.md S5, EQC128). */
class ChunkWatchTest {
    static final Object OVERWORLD = "minecraft:overworld";

    /** A world of chunks with levels and visible entities, edited by the test. */
    static final class FakeWorld implements ChunkWatch.Source {
        final Map<Long, Integer> levels = new HashMap<>();
        final Set<Long> hidden = new HashSet<>();

        void load(ChunkWatch.Area a, int level) {
            for (long c : a.chunks())
                levels.put(c, level);
        }

        @Override
        public int level(Object dimension, long chunk) {
            return levels.getOrDefault(chunk, ChunkWatch.NO_HOLDER);
        }

        @Override
        public boolean entitiesIn(Object dimension, long chunk) {
            return levels.getOrDefault(chunk, ChunkWatch.NO_HOLDER) <= ChunkWatch.FULL && !hidden.contains(chunk);
        }
    }

    static final class Log implements ChunkWatch.Listener {
        final List<String> events = new ArrayList<>();
        long next = 100;

        @Override
        public long in(ChunkWatch.Area area, long now) {
            events.add("in@" + now);
            return next++;
        }

        @Override
        public void out(ChunkWatch.Area area, long now) {
            events.add("out@" + now + "e" + area.epoch());
        }
    }

    private static void run(ChunkWatch w, long from, long to) {
        for (long t = from; t <= to; t++)
            w.tick(t);
    }

    @Test
    void theFootprintIsTheDiscsChunks() {
        // A disc of 48 blocks round the middle of a chunk touches at most 7 by 7 chunks, fewer at the corners.
        long[] f = ChunkWatch.footprint(8.0, 8.0, 48.0);
        assertTrue(f.length <= 49 && f.length >= 37, "" + f.length);
        Set<Long> set = new HashSet<>();
        for (long c : f)
            set.add(c);
        assertTrue(set.contains(ChunkWatch.pack(0, 0)));
        assertTrue(set.contains(ChunkWatch.pack(3, 0)));
        assertFalse(set.contains(ChunkWatch.pack(3, 3)), "the corner is outside the disc");
        assertFalse(set.contains(ChunkWatch.pack(4, 0)));
    }

    @Test
    void anAreaComesInWhenEveryChunkIsFullWithItsEntitiesAndOutWhenOneDrops() {
        FakeWorld world = new FakeWorld();
        Log log = new Log();
        ChunkWatch w = new ChunkWatch(world, log, 200);
        ChunkWatch.Area a = w.declare(7, OVERWORLD, 8.0, 8.0, 48.0);
        run(w, 1, 40);
        assertTrue(log.events.isEmpty(), "nothing is loaded");
        world.load(a, 31);
        run(w, 41, 80);
        assertEquals(List.of("in@60"), log.events.subList(0, 1), "the poll of its slot finds it (a slot every 20 ticks)");
        assertTrue(a.in());
        assertEquals(100, a.epoch());
        // One chunk drops below FULL: out at once, the epoch ending.
        world.levels.put(a.chunks()[3], 40);
        w.onTicket(OVERWORLD, a.chunks()[3], 40);
        run(w, 81, 82);
        assertFalse(a.in());
        assertEquals("out@81e100", log.events.get(1));
    }

    @Test
    void thePollAloneFollowsTheLevelsWhenNoEventIsPosted() {
        FakeWorld world = new FakeWorld();
        Log log = new Log();
        ChunkWatch w = new ChunkWatch(world, log, 0);
        ChunkWatch.Area a = w.declare(1, OVERWORLD, 0.0, 0.0, 16.0);
        world.load(a, 33);
        run(w, 1, 60);
        assertTrue(a.in(), "found by the poll, no event");
        world.load(a, 44);
        run(w, 61, 100);
        assertFalse(a.in(), "gone by the poll, no event");
        assertTrue(w.pollChanges > 0);
    }

    @Test
    void aChunkWhoseEntitiesAreHiddenIsNotLoaded() {
        FakeWorld world = new FakeWorld();
        Log log = new Log();
        ChunkWatch w = new ChunkWatch(world, log, 0);
        ChunkWatch.Area a = w.declare(1, OVERWORLD, 0.0, 0.0, 16.0);
        world.load(a, 33);
        world.hidden.add(a.chunks()[0]);
        run(w, 1, 60);
        assertFalse(a.in(), "hidden entities (MZB-660): a census now would miss the body");
        world.hidden.clear();
        run(w, 61, 70);
        assertTrue(a.in(), "it shows");
    }

    @Test
    void anAreaThatWentOutWaitsTheGapBeforeItComesInAgain() {
        FakeWorld world = new FakeWorld();
        Log log = new Log();
        ChunkWatch w = new ChunkWatch(world, log, 200);
        ChunkWatch.Area a = w.declare(1, OVERWORLD, 0.0, 0.0, 16.0);
        world.load(a, 33);
        run(w, 1, 40);
        assertTrue(a.in());
        world.load(a, 44);
        run(w, 41, 60);
        assertFalse(a.in());
        world.load(a, 33);
        run(w, 61, 200);
        assertFalse(a.in(), "inside the gap, no record storm");
        run(w, 201, 280);
        assertTrue(a.in());
        assertEquals(3, log.events.size());
    }

    @Test
    void aRecountMintsANewEpochWhileTheAreaStaysIn() {
        FakeWorld world = new FakeWorld();
        Log log = new Log();
        ChunkWatch w = new ChunkWatch(world, log, 0);
        ChunkWatch.Area a = w.declare(1, OVERWORLD, 0.0, 0.0, 16.0);
        world.load(a, 33);
        run(w, 1, 40);
        assertEquals(100, a.epoch());
        w.recount(a);
        run(w, 41, 42);
        assertEquals(101, a.epoch());
        assertEquals(2, log.events.size());
    }

    @Test
    void resetPutsEveryAreaOutQuietlyAndTheyComeInAgain() {
        FakeWorld world = new FakeWorld();
        Log log = new Log();
        ChunkWatch w = new ChunkWatch(world, log, 0);
        ChunkWatch.Area a = w.declare(1, OVERWORLD, 0.0, 0.0, 16.0);
        world.load(a, 33);
        run(w, 1, 40);
        w.reset();
        assertFalse(a.in());
        assertEquals(1, log.events.size(), "no out call: the engine voided its lines at its own start");
        run(w, 41, 60);
        assertTrue(a.in());
    }

    @Test
    void removingAnAreaThatIsInSendsItOut() {
        FakeWorld world = new FakeWorld();
        Log log = new Log();
        ChunkWatch w = new ChunkWatch(world, log, 0);
        ChunkWatch.Area a = w.declare(1, OVERWORLD, 0.0, 0.0, 16.0);
        world.load(a, 33);
        run(w, 1, 40);
        w.remove(1, 41);
        assertEquals("out@41e100", log.events.get(1));
        assertEquals(0, w.size());
    }
}
