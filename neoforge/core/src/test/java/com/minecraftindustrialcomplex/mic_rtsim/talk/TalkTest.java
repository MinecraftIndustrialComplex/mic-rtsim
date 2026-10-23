package com.minecraftindustrialcomplex.mic_rtsim.talk;

import java.util.Deque;
import java.util.HashSet;
import java.util.List;
import java.util.Random;
import java.util.Set;
import java.util.UUID;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** TALK's pure logic: the picker's rules (TK4, TK22), the rate limits (TK3), the day cap (TK25) and the name hash (TK13). */
class TalkTest {
    private static Picker.Option option(String topic, int count, double weight) {
        return new Picker.Option(topic, "p." + topic, count, weight, new Object[0]);
    }

    @Test
    void neverTheSameKeyTwiceInARowWhileAnotherVariantIsLeft() {
        Random rnd = new Random(1);
        Deque<String> history = Picker.newHistory();
        List<Picker.Option> options = List.of(option("a", 3, 1));
        String previous = null;
        for (int i = 0; i < 500; i++) {
            Picker.Pick p = Picker.pick(options, List.copyOf(history), rnd);
            assertNotNull(p);
            assertNotEquals(previous, p.key());
            previous = p.key();
            Picker.remember(history, p.key());
        }
    }

    @Test
    void anOnlyLineMaySpeakAgain() {
        Random rnd = new Random(2);
        Picker.Pick p = Picker.pick(List.of(option("a", 1, 1)), List.of("p.a.0"), rnd);
        assertEquals("p.a.0", p.key());
    }

    @Test
    void aSingleLineTopicYieldsToAnotherTopicRightAfterItSpoke() {
        Random rnd = new Random(3);
        List<Picker.Option> options = List.of(option("one", 1, 100), option("other", 4, 0.01));
        for (int i = 0; i < 50; i++)
            assertEquals("other", Picker.pick(options, List.of("p.one.0"), rnd).option().topic());
    }

    @Test
    void variantsOutsideTheHistoryComeFirstThenTheLeastRecent() {
        Random rnd = new Random(4);
        Picker.Option o = option("a", 3, 1);
        for (int i = 0; i < 50; i++)
            assertEquals("p.a.2", Picker.pick(List.of(o), List.of("p.a.0", "p.a.1"), rnd).key());
        // All three heard: the least recent, p.a.1 here (oldest first).
        assertEquals("p.a.1", Picker.pick(List.of(o), List.of("p.a.1", "p.a.2", "p.a.0"), rnd).key());
    }

    @Test
    void aTopicWhoseVariantsWereAllHeardLosesHalfItsWeight() {
        Random rnd = new Random(5);
        List<Picker.Option> options = List.of(option("heard", 2, 1), option("fresh", 2, 1));
        List<String> recent = List.of("p.heard.0", "p.heard.1", "x");
        int heard = 0, n = 6000;
        for (int i = 0; i < n; i++)
            if (Picker.pick(options, recent, rnd).option().topic().equals("heard"))
                heard++;
        // 0.5 against 1: a third of the picks.
        double share = heard / (double) n;
        assertTrue(share > 0.30 && share < 0.37, "share " + share);
    }

    @Test
    void historyKeepsTheLastEight() {
        Deque<String> h = Picker.newHistory();
        for (int i = 0; i < 20; i++)
            Picker.remember(h, "k" + i);
        assertEquals(Picker.HISTORY, h.size());
        assertEquals("k12", h.peekFirst());
        assertEquals("k19", h.peekLast());
        Picker.remember(h, "k15");
        assertEquals("k15", h.peekLast());
        assertEquals(Picker.HISTORY, h.size());
    }

    @Test
    void nothingToSayPicksNothing() {
        assertNull(Picker.pick(List.of(option("a", 0, 1)), List.of(), new Random(6)));
        assertNull(Picker.pick(List.of(), List.of(), new Random(6)));
    }

    @Test
    void theRateLimitsAnswerOnePlayerPerFortyTicksAndAnyonePerTen() {
        RateLimit r = new RateLimit();
        UUID talker = UUID.randomUUID(), ben = UUID.randomUUID(), alex = UUID.randomUUID();
        assertTrue(r.tryAcquire(talker, ben, 1000));
        assertFalse(r.tryAcquire(talker, ben, 1005), "same player inside 40");
        assertFalse(r.tryAcquire(talker, alex, 1005), "anyone inside 10");
        assertTrue(r.tryAcquire(talker, alex, 1010), "another player after 10");
        assertFalse(r.tryAcquire(talker, ben, 1039), "ben still inside 40 of his own last talk");
        assertTrue(r.tryAcquire(talker, ben, 1040));
        assertTrue(r.tryAcquire(UUID.randomUUID(), ben, 1040), "another talker is not limited");
    }

    @Test
    void theRateLimitMapsStayBounded() {
        RateLimit r = new RateLimit();
        UUID player = UUID.randomUUID();
        for (int i = 0; i < 20_000; i++)
            r.tryAcquire(UUID.randomUUID(), player, i * 50L);
        assertTrue(r.size() <= 2 * RateLimit.BOUND + 2, "size " + r.size());
    }

    @Test
    void theDayCapKeepsSixteenPlayersAndDropsTheOldestDay() {
        TalkDays d = TalkDays.EMPTY;
        UUID[] ids = new UUID[20];
        for (int i = 0; i < ids.length; i++) {
            ids[i] = UUID.randomUUID();
            d = d.with(ids[i], 100 + i);
        }
        assertEquals(TalkDays.CAP, d.entries().size());
        for (int i = 0; i < 4; i++)
            assertEquals(-1, d.lastDay(ids[i]), "the four oldest days are gone");
        assertEquals(119, d.lastDay(ids[19]));
        // The same player again replaces his entry.
        d = d.with(ids[10], 500);
        assertEquals(TalkDays.CAP, d.entries().size());
        assertEquals(500, d.lastDay(ids[10]));
        assertTrue(d.creditedOn(ids[10], 500));
        assertFalse(d.creditedOn(ids[10], 501));
        assertTrue(TalkDays.EMPTY.isEmpty());
    }

    @Test
    void theNameHashIsDeterministicAndSpreadsOverBothLists() {
        assertEquals(Names.mix(42), Names.mix(42));
        // SplitMix64 reference: the first output of a generator seeded 0 is the mix of the golden gamma.
        assertEquals(0xE220A8397B1DCDAFL, Names.mix(0));
        Set<Integer> as = new HashSet<>(), bs = new HashSet<>();
        for (long id = 0; id < 2000; id++) {
            int[] ij = Names.indices(id, 42, 40);
            assertTrue(ij[0] >= 0 && ij[0] < 42 && ij[1] >= 0 && ij[1] < 40);
            assertEquals(ij[0], Names.indices(id, 42, 40)[0]);
            as.add(ij[0]);
            bs.add(ij[1]);
        }
        assertEquals(42, as.size());
        assertEquals(40, bs.size());
    }
}
