package com.minecraftindustrialcomplex.mic_rtsim.talk;

import org.junit.jupiter.api.Test;

import java.util.SplittableRandom;
import java.util.random.RandomGenerator;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Named or described (TALK.md §6, TK29): the thresholds with a forced random, and the capitals of sentence starts (TK55). */
class NamingTest {
    /** A random that always draws {@code v} and counts the draws. */
    private static final class Fixed implements RandomGenerator {
        final double v;
        int draws;

        Fixed(double v) {
            this.v = v;
        }

        @Override
        public long nextLong() {
            draws++;
            return 0;
        }

        @Override
        public double nextDouble() {
            draws++;
            return v;
        }
    }

    @Test
    void anAlwaysNamedRefIsNamedWithoutADraw() {
        Fixed rnd = new Fixed(0.99);
        assertTrue(Naming.named(true, true, 9000, 9000, rnd));
        assertTrue(Naming.named(true, false, 0, 0, rnd));
        assertEquals(0, rnd.draws);
    }

    @Test
    void underAShortWalkIsAlwaysNamed() {
        Fixed rnd = new Fixed(0.99);
        assertTrue(Naming.named(false, true, 0, 0, rnd));
        assertTrue(Naming.named(false, true, 599, 0, rnd));
        assertTrue(Naming.named(false, true, 0, -420, rnd));
        assertEquals(0, rnd.draws);
        // 600 blocks exactly is already past it.
        assertFalse(Naming.named(false, true, 600, 0, new Fixed(0.99)));
    }

    @Test
    void underADaysRideIsNamedHalfTheTime() {
        assertTrue(Naming.named(false, true, 600, 0, new Fixed(0.49)));
        assertFalse(Naming.named(false, true, 600, 0, new Fixed(0.5)));
        assertTrue(Naming.named(false, true, 2999, 0, new Fixed(0.0)));
        assertFalse(Naming.named(false, true, -2999, 0, new Fixed(0.9)));
    }

    @Test
    void fartherIsNamedAQuarterOfTheTime() {
        assertTrue(Naming.named(false, true, 3000, 0, new Fixed(0.24)));
        assertFalse(Naming.named(false, true, 3000, 0, new Fixed(0.25)));
        assertTrue(Naming.named(false, true, 1_000_000, 1_000_000, new Fixed(0.0)));
    }

    @Test
    void aRefWithNoPlaceIsAsGoodAsFar() {
        assertTrue(Naming.named(false, false, 0, 0, new Fixed(0.24)));
        assertFalse(Naming.named(false, false, 0, 0, new Fixed(0.25)));
        assertFalse(Naming.named(false, false, 0, 0, new Fixed(0.5)));
    }

    @Test
    void theRatesHoldOverManyDraws() {
        SplittableRandom rnd = new SplittableRandom(7);
        int half = 0, quarter = 0, n = 20_000;
        for (int i = 0; i < n; i++) {
            if (Naming.named(false, true, 1500, 0, rnd))
                half++;
            if (Naming.named(false, true, 5000, 0, rnd))
                quarter++;
        }
        assertEquals(0.5, half / (double) n, 0.02);
        assertEquals(0.25, quarter / (double) n, 0.02);
    }

    @Test
    void aDescribedRefAtTheReportsOwnPlaceDropsItsLocator() {
        assertTrue(Naming.atReport(100, 100, 110, 100));
        assertTrue(Naming.atReport(100, 100, 100, 100));
        assertFalse(Naming.atReport(100, 100, 100, 130));
        assertFalse(Naming.atReport(-500, 0, 500, 0));
    }

    @Test
    void sentenceStartsAreFoundWhereAnArgumentOpensASentence() {
        assertEquals("{0}", Sentences.startsAt("%1$s attacked us.").toString());
        assertEquals("{0}", Sentences.startsAt("Did you hear? %1$s attacked us %3$s.").toString());
        assertEquals("{0}", Sentences.startsAt("We were robbed blind %3$s. %1$s.").toString());
        assertEquals("{0}", Sentences.startsAt("Robbed blind, we were, %3$s. %1$s. The market's still bare.").toString());
        assertEquals("{}", Sentences.startsAt("Word is %1$s robbed %2$s %4$s, %3$s.").toString());
        assertEquals("{}", Sentences.startsAt("Heard %2$s lost everything to %1$s %4$s, %3$s.").toString());
        assertEquals("{1}", Sentences.startsAt("Hm. %2$s did it. Not %1$s.").toString());
        assertEquals("{}", Sentences.startsAt("No arguments at all.").toString());
    }

    @Test
    void capitaliseOpensTheFirstLetterOnly() {
        assertEquals("The goblins in the hills", Sentences.capitalise("the goblins in the hills"));
        assertEquals("Oakford", Sentences.capitalise("Oakford"));
        assertEquals("", Sentences.capitalise(""));
        assertEquals("A", Sentences.capitalise("a"));
    }
}
