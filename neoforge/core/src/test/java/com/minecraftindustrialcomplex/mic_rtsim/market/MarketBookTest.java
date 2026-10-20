package com.minecraftindustrialcomplex.mic_rtsim.market;

import org.junit.jupiter.api.Test;

import java.util.Map;
import java.util.Random;

import static com.minecraftindustrialcomplex.mic_rtsim.market.PriceCurve.MILLI;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The bridge's market quotes mirror rtsim-trade's (crates/rtsim-trade/src/price.rs and market.rs on
 * overnight/phase1): the same vectors give the same numbers, so what a villager offers is what the
 * engine will apply.
 */
class MarketBookTest {

    private static PriceCurve curve() {
        return new PriceCurve(50_000, 3, 4, 40 * MILLI);
    }

    /** rtsim-trade price.rs {@code band_ends}. */
    @Test
    void bandEnds() {
        PriceCurve c = curve();
        assertEquals(87_500, c.priceAt(0));
        assertEquals(50_000, c.priceAt(40 * MILLI));
        assertEquals(12_500, c.priceAt(80 * MILLI));
        assertEquals(12_500, c.priceAt(1_000 * MILLI));
    }

    /** rtsim-trade price.rs {@code integral_of_the_whole_band_is_the_trapezoid}. */
    @Test
    void integralOfTheWholeBandIsTheTrapezoid() {
        PriceCurve.Value v = curve().integral(0, 80 * MILLI);
        assertEquals(4, v.coinsFloor());
        assertEquals(4, v.coinsCeil());
        PriceCurve.Value w = curve().integral(80 * MILLI, 88 * MILLI);
        assertEquals(w.num().multiply(java.math.BigInteger.TEN), w.den().multiply(java.math.BigInteger.valueOf(PriceCurve.PRICE_ONE)));
    }

    /** rtsim-trade price.rs {@code integral_is_additive} and {@code round_trip_never_pays}, sampled. */
    @Test
    void additiveAndNoMoneyPump() {
        Random r = new Random(7);
        for (int i = 0; i < 2_000; i++) {
            long anchor = 1 + r.nextInt(10_000_000), t = 1 + r.nextInt(100_000);
            PriceCurve c = new PriceCurve(anchor, 3, 4, t);
            long a = r.nextInt(200_000), b = a + r.nextInt(200_000), e = b + r.nextInt(200_000);
            PriceCurve.Value whole = c.integral(a, e), left = c.integral(a, b), right = c.integral(b, e);
            assertEquals(whole.den(), left.den());
            assertEquals(whole.num(), left.num().add(right.num()));
            long s = r.nextInt(300_000), q = Math.min(r.nextInt(100_000), s);
            assertTrue(c.sellValue(s - q, q).coinsFloor() <= c.buyValue(s, q).coinsCeil());
        }
    }

    private static MarketBook<String> tiny(long stock, long treasury) {
        MarketBook<String> m = new MarketBook<>(1);
        m.snapshot(0, "emerald", treasury, 20 * MILLI, new Ratio(1, 2),
                Map.of("wheat", new MarketBook.Good(stock, new PriceCurve(50_000, 3, 4, 40 * MILLI))));
        return m;
    }

    /** rtsim-trade market.rs {@code treasury_limits_what_the_market_pays}: the offer half. */
    @Test
    void treasuryLimitsWhatTheMarketPays() {
        MarketBook.Offer o = tiny(40 * MILLI, 2 * MILLI).offer("wheat", MarketBook.Side.MARKET_BUYS, Ratio.FAIR);
        assertNotNull(o);
        assertEquals(1, o.coins());
        assertEquals(2, o.uses());
    }

    /** rtsim-trade market.rs {@code cash_refused_above_twice_target}. */
    @Test
    void cashRefusedAboveTwiceTarget() {
        MarketBook<String> m = tiny(40 * MILLI, 41 * MILLI);
        assertTrue(m.cashRefused());
        assertNull(m.offer("wheat", MarketBook.Side.MARKET_SELLS, Ratio.FAIR));
    }

    /** rtsim-trade market.rs {@code markup_is_never_better_than_fair}. */
    @Test
    void markupIsNeverBetterThanFair() {
        Ratio[] m = Ratio.markupFor(-128, new Ratio(1, 2));
        assertEquals(new Ratio(256 + 128, 256), m[0]);
        assertEquals(new Ratio(256 - 128, 256), m[1]);
        Ratio[] f = Ratio.markupFor(100, new Ratio(1, 2));
        assertEquals(Ratio.FAIR, f[0]);
        assertEquals(Ratio.FAIR, f[1]);
    }

    /**
     * Walkthrough B: each sale drains the treasury at once, and once it cannot pay for one bundle the
     * offer is gone (vanilla shows it crossed out). A snapshot computed before the sales does not give
     * the money back; one that includes them replaces them.
     */
    @Test
    void localSalesDrainTheTreasuryUntilASnapshotIncludesThem() {
        PriceCurve big = new PriceCurve(50_000, 3, 4, 400 * MILLI);
        MarketBook<String> m = new MarketBook<>(3);
        m.snapshot(10, "emerald", 3 * MILLI, 20 * MILLI, new Ratio(1, 2), Map.of("wheat", new MarketBook.Good(400 * MILLI, big)));
        long seq = 10;
        int sales = 0;
        MarketBook.Offer o;
        while ((o = m.offer("wheat", MarketBook.Side.MARKET_BUYS, Ratio.FAIR)) != null) {
            m.local(++seq, "wheat", MarketBook.Side.MARKET_BUYS, o.items(), o.coins());
            sales++;
            assertTrue(sales < 100);
        }
        assertEquals(3, sales);
        assertEquals(0, m.treasury());
        // A late snapshot (as of before the sales) keeps them.
        m.snapshot(10, "emerald", 3 * MILLI, 20 * MILLI, new Ratio(1, 2),
                Map.of("wheat", new MarketBook.Good(400 * MILLI, big)));
        assertEquals(0, m.treasury());
        assertNull(m.offer("wheat", MarketBook.Side.MARKET_BUYS, Ratio.FAIR));
        // A caravan bought the surplus: a snapshot that includes the sales and a refill reopens it.
        m.snapshot(seq, "emerald", 5 * MILLI, 20 * MILLI, new Ratio(1, 2),
                Map.of("wheat", new MarketBook.Good(300 * MILLI, big)));
        assertEquals(0, m.pendingLocals());
        assertNotNull(m.offer("wheat", MarketBook.Side.MARKET_BUYS, Ratio.FAIR));
    }

    /** The market sells cheap goods by the bundle for one coin, dear goods singly for whole coins. */
    @Test
    void bundles() {
        MarketBook<String> m = tiny(40 * MILLI, 10 * MILLI);
        MarketBook.Offer cheap = m.offer("wheat", MarketBook.Side.MARKET_SELLS, Ratio.FAIR);
        assertNotNull(cheap);
        assertEquals(1, cheap.coins());
        assertTrue(cheap.items() > 1);
        MarketBook<String> dear = new MarketBook<>(2);
        dear.snapshot(0, "emerald", 10 * MILLI, 0, new Ratio(0, 1),
                Map.of("iron", new MarketBook.Good(10 * MILLI, new PriceCurve(3_000_000, 3, 4, 10 * MILLI))));
        MarketBook.Offer iron = dear.offer("iron", MarketBook.Side.MARKET_SELLS, Ratio.FAIR);
        assertNotNull(iron);
        assertEquals(1, iron.items());
        assertTrue(iron.coins() >= 3);
    }
}
