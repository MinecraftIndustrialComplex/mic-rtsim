package com.minecraftindustrialcomplex.mic_rtsim.market;

import org.junit.jupiter.api.Test;

import java.util.Map;
import java.util.Random;

import static com.minecraftindustrialcomplex.mic_rtsim.market.PriceCurve.MILLI;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
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

    /** A good's line as MARKET carries it: the oracle's ladders, flattened as the dispatcher does. */
    static MarketBook.Good good(long stock, PriceCurve curve, long treasury, long treasuryTarget, Ratio worst) {
        java.util.List<PriceOracle.Bundle>[][] l = PriceOracle.ladders(curve, stock, treasury, treasuryTarget, worst);
        long[][][] out = new long[2][l[0].length][];
        for (int side = 0; side < 2; side++)
            for (int k = 0; k < l[side].length; k++) {
                out[side][k] = new long[2 * l[side][k].size()];
                for (int j = 0; j < l[side][k].size(); j++) {
                    out[side][k][2 * j] = l[side][k].get(j).items();
                    out[side][k][2 * j + 1] = l[side][k].get(j).coins();
                }
            }
        return new MarketBook.Good(stock, out);
    }

    private static MarketBook<String> book(long village, long asOf, long stock, PriceCurve curve, long treasury) {
        MarketBook<String> m = new MarketBook<>(village);
        m.snapshot(asOf, "emerald", treasury, 20 * MILLI, PriceOracle.thresholds(),
                Map.of("wheat", good(stock, curve, treasury, 20 * MILLI, new Ratio(1, 2))));
        return m;
    }

    private static MarketBook<String> tiny(long stock, long treasury) {
        return book(1, 0, stock, new PriceCurve(50_000, 3, 4, 40 * MILLI), treasury);
    }

    /** rtsim-trade market.rs {@code treasury_limits_what_the_market_pays}: the offer half. */
    @Test
    void treasuryLimitsWhatTheMarketPays() {
        MarketBook.Offer o = tiny(40 * MILLI, 2 * MILLI).offer("wheat", MarketBook.Side.MARKET_BUYS, 0);
        assertNotNull(o);
        assertEquals(1, o.coins());
        assertEquals(2, o.uses());
    }

    /** rtsim-trade market.rs {@code cash_refused_above_twice_target}. */
    @Test
    void cashRefusedAboveTwiceTarget() {
        MarketBook<String> m = tiny(40 * MILLI, 41 * MILLI);
        assertTrue(m.cashRefused());
        assertNull(m.offer("wheat", MarketBook.Side.MARKET_SELLS, 0));
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
     * G31: a player's standing picks the MARKET's price level, the deepest whose threshold it is at
     * or below; a disliked player brings more and is paid less, a liked one gets the fair price.
     */
    @Test
    void standingPicksThePriceLevel() {
        MarketBook<String> m = tiny(40 * MILLI, 9 * MILLI);
        assertEquals(PriceOracle.PRICE_LEVELS.length + 1, m.levels());
        assertEquals(0, m.levelOf(500));
        assertEquals(0, m.levelOf(0));
        assertEquals(0, m.levelOf(-24));
        assertEquals(1, m.levelOf(-25));
        assertEquals(2, m.levelOf(-60));
        assertEquals(m.levels() - 1, m.levelOf(-700));
        MarketBook.Offer fair = m.offer("wheat", MarketBook.Side.MARKET_BUYS, 0);
        MarketBook.Offer hated = m.offer("wheat", MarketBook.Side.MARKET_BUYS, m.levelOf(-700));
        assertTrue(hated.items() > fair.items(), fair + " " + hated);
        MarketBook.Offer fairSale = m.offer("wheat", MarketBook.Side.MARKET_SELLS, 0);
        MarketBook.Offer hatedSale = m.offer("wheat", MarketBook.Side.MARKET_SELLS, m.levelOf(-700));
        assertTrue(hatedSale.items() < fairSale.items() || hatedSale.coins() > fairSale.coins(), fairSale + " " + hatedSale);
    }

    /**
     * Prices: the bridge shows the engine's ladder step by step as its own trades move the stock, and
     * each step is what the oracle (rtsim-trade's math) prices at the stock the trades before left. A
     * stock the ladder does not reach (a sale after a purchase) shows crossed out until the next MARKET.
     */
    @Test
    void localTradesWalkTheLadder() {
        PriceCurve c = new PriceCurve(50_000, 3, 4, 40 * MILLI);
        MarketBook<String> m = book(4, 0, 40 * MILLI, c, 3 * MILLI);
        long seq = 0, stock = 40 * MILLI, treasury = 3 * MILLI;
        for (int i = 0; i < PriceOracle.LADDER_DEPTH; i++) {
            MarketBook.Offer o = m.offer("wheat", MarketBook.Side.MARKET_SELLS, 0);
            boolean refused = PriceOracle.glut(treasury, 20 * MILLI);
            PriceOracle.Bundle want = PriceOracle.bundleAt(c, stock, 40 * MILLI, refused, MarketBook.Side.MARKET_SELLS, Ratio.FAIR);
            if (want == null) {
                assertNull(o);
                break;
            }
            assertNotNull(o, "step " + i);
            assertEquals(want.items(), o.items());
            assertEquals(want.coins(), o.coins());
            m.local(++seq, "wheat", MarketBook.Side.MARKET_SELLS, o.items(), o.coins());
            stock -= o.items() * MILLI;
            treasury += o.coins() * MILLI;
        }
        // A sale back moves the stock off the purchases' ladder: crossed out until the next MARKET.
        MarketBook<String> n = book(5, 0, 40 * MILLI, c, 3 * MILLI);
        MarketBook.Offer first = n.offer("wheat", MarketBook.Side.MARKET_SELLS, 0);
        n.local(1, "wheat", MarketBook.Side.MARKET_SELLS, first.items(), first.coins());
        n.local(2, "wheat", MarketBook.Side.MARKET_BUYS, 1, 0);
        assertNull(n.offer("wheat", MarketBook.Side.MARKET_SELLS, 0));
    }

    /**
     * Walkthrough B: each sale drains the treasury at once, and once it cannot pay for one bundle the
     * offer is gone (vanilla shows it crossed out). A MARKET computed before the sales does not give
     * the money back; one that includes them replaces them.
     */
    @Test
    void localSalesDrainTheTreasuryUntilASnapshotIncludesThem() {
        PriceCurve big = new PriceCurve(50_000, 3, 4, 400 * MILLI);
        MarketBook<String> m = book(3, 10, 400 * MILLI, big, 3 * MILLI);
        long seq = 10;
        int sales = 0;
        MarketBook.Offer o;
        while ((o = m.offer("wheat", MarketBook.Side.MARKET_BUYS, 0)) != null) {
            m.local(++seq, "wheat", MarketBook.Side.MARKET_BUYS, o.items(), o.coins());
            sales++;
            assertTrue(sales < 100);
        }
        assertEquals(3, sales);
        assertEquals(0, m.treasury());
        // A late MARKET (as of before the sales) keeps them.
        m.snapshot(10, "emerald", 3 * MILLI, 20 * MILLI, PriceOracle.thresholds(),
                Map.of("wheat", good(400 * MILLI, big, 3 * MILLI, 20 * MILLI, new Ratio(1, 2))));
        assertEquals(0, m.treasury());
        assertNull(m.offer("wheat", MarketBook.Side.MARKET_BUYS, 0));
        // A caravan bought the surplus: a MARKET that includes the sales and a refill reopens it.
        m.snapshot(seq, "emerald", 5 * MILLI, 20 * MILLI, PriceOracle.thresholds(),
                Map.of("wheat", good(300 * MILLI, big, 5 * MILLI, 20 * MILLI, new Ratio(1, 2))));
        assertEquals(0, m.pendingLocals());
        MarketBook.Offer back = m.offer("wheat", MarketBook.Side.MARKET_BUYS, 0);
        assertNotNull(back);
        assertEquals(5, back.uses());
    }

    /** The market sells cheap goods by the bundle for one coin, dear goods singly for whole coins. */
    @Test
    void bundles() {
        MarketBook<String> m = tiny(40 * MILLI, 10 * MILLI);
        MarketBook.Offer cheap = m.offer("wheat", MarketBook.Side.MARKET_SELLS, 0);
        assertNotNull(cheap);
        assertEquals(1, cheap.coins());
        assertTrue(cheap.items() > 1);
        PriceOracle.Bundle iron = PriceOracle.bundleAt(new PriceCurve(3_000_000, 3, 4, 10 * MILLI), 10 * MILLI, 10 * MILLI,
                false, MarketBook.Side.MARKET_SELLS, Ratio.FAIR);
        assertNotNull(iron);
        assertEquals(1, iron.items());
        assertTrue(iron.coins() >= 3);
    }

    /**
     * The oracle and rtsim-trade agree on one vector the Rust test checks too
     * ({@code market::tests::a_ladder_is_the_offers_of_successive_trades}, stock 40, treasury 9, fair):
     * every step of the ladder of the market's sales.
     */
    @Test
    void theOracleLadderIsTheRustLadder() {
        PriceCurve c = new PriceCurve(50_000, 3, 4, 40 * MILLI);
        java.util.List<PriceOracle.Bundle> l = PriceOracle.ladder(c, 40 * MILLI, 40 * MILLI, 9 * MILLI, 20 * MILLI,
                MarketBook.Side.MARKET_SELLS, Ratio.FAIR, 12);
        assertFalse(l.isEmpty());
        long s = 40 * MILLI;
        for (PriceOracle.Bundle b : l) {
            // Each bundle costs at least the integral of the price over what it takes.
            assertTrue(c.buyValue(s, b.items() * MILLI).coinsCeil() <= b.coins());
            s -= b.items() * MILLI;
        }
    }
}
