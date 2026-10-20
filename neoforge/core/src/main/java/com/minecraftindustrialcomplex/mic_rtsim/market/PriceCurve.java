package com.minecraftindustrialcomplex.mic_rtsim.market;

import java.math.BigInteger;

/**
 * The storehouse price curve (C13, PLAN.md §8.4), a mirror of rtsim-trade's {@code price::PriceCurve}
 * so the bridge quotes exactly what the engine will apply.
 *
 * <p>{@code p(S) = p* (1 + r (1 - S / Sbar))} on {@code [0, 2 Sbar]}, and the floor {@code p* (1 - r)}
 * above it. Units are rtsim-trade's: stocks in thousandths of an item ({@link #MILLI}), prices in
 * millionths of a coin per whole item ({@link #PRICE_ONE}). The trade integral is exact: a buyer pays
 * the ceiling in whole coins and a seller gets the floor, so no loop of trades makes money.
 *
 * <p>Pure Java; {@link BigInteger} stands in for Rust's {@code i128}. It runs only when a merchant is
 * repriced (a trade, a new snapshot, a restock), never per tick.
 */
public record PriceCurve(long anchor, long rNum, long rDen, long target) {

    /** Thousandths of an item or coin per whole one. */
    public static final long MILLI = 1_000;
    /** Millionths of a coin per whole item: one coin per item. */
    public static final long PRICE_ONE = 1_000_000;
    /** rtsim-trade's MAX_STOCK. */
    public static final long MAX_STOCK = 1L << 40;

    public PriceCurve {
        if (anchor < 0 || rDen <= 0 || rNum < 0 || rNum >= rDen)
            throw new IllegalArgumentException("bad curve: anchor " + anchor + ", r " + rNum + "/" + rDen);
        target = Math.min(Math.max(target, 1), MAX_STOCK);
    }

    /** An exact non-negative value in millionths of a coin: {@code num / den}. */
    public record Value(BigInteger num, BigInteger den) {
        private static final BigInteger PRICE = BigInteger.valueOf(PRICE_ONE);

        /** Whole coins, rounded up: what a buyer pays. */
        public long coinsCeil() {
            BigInteger d = den.multiply(PRICE);
            return num.add(d).subtract(BigInteger.ONE).divide(d).longValueExact();
        }

        /** Whole coins, rounded down: what a seller gets. */
        public long coinsFloor() {
            return num.divide(den.multiply(PRICE)).longValueExact();
        }

        public Value scaled(Ratio by) {
            return new Value(num.multiply(BigInteger.valueOf(by.num())), den.multiply(BigInteger.valueOf(by.den())));
        }
    }

    /** {@code p(S)}, rounded down to a millionth of a coin. */
    public long priceAt(long stock) {
        BigInteger n = BigInteger.valueOf(rNum), d = BigInteger.valueOf(rDen), t = BigInteger.valueOf(target);
        BigInteger s = BigInteger.valueOf(Math.min(Math.max(stock, 0), 2 * target));
        return BigInteger.valueOf(anchor).multiply(d.add(n).multiply(t).subtract(n.multiply(s))).divide(d.multiply(t)).longValue();
    }

    /** {@code ∫_a^b p(S) dS} for {@code 0 <= a <= b}, exact (rtsim-trade {@code PriceCurve::integral}). */
    public Value integral(long a, long b) {
        if (a < 0 || a > b || b > MAX_STOCK)
            throw new IllegalArgumentException("integral over [" + a + ", " + b + "]");
        BigInteger n = BigInteger.valueOf(rNum), d = BigInteger.valueOf(rDen), t = BigInteger.valueOf(target);
        long twoT = 2 * target;
        long a1 = Math.min(a, twoT), b1 = Math.min(b, twoT);
        BigInteger d1 = BigInteger.valueOf(b1 - a1), s1 = BigInteger.valueOf(b1 + a1);
        BigInteger d2 = BigInteger.valueOf(Math.max(b - Math.max(a, twoT), 0));
        BigInteger two = BigInteger.TWO;
        BigInteger sloped = two.multiply(d.add(n)).multiply(t).multiply(d1).subtract(n.multiply(d1).multiply(s1));
        BigInteger flat = two.multiply(t).multiply(d.subtract(n)).multiply(d2);
        BigInteger num = BigInteger.valueOf(anchor).multiply(sloped.add(flat));
        BigInteger den = two.multiply(d).multiply(t).multiply(BigInteger.valueOf(MILLI));
        return new Value(num, den);
    }

    /** What a buyer of {@code q} thousandths pays from stock {@code s}, before rounding. */
    public Value buyValue(long s, long q) {
        if (q < 0 || q > s)
            throw new IllegalArgumentException("buying " + q + " from a stock of " + s);
        return integral(s - q, s);
    }

    /** What a seller of {@code q} thousandths is paid into stock {@code s}, before rounding. */
    public Value sellValue(long s, long q) {
        if (q < 0 || s < 0)
            throw new IllegalArgumentException("selling " + q + " into a stock of " + s);
        return integral(s, s + q);
    }
}
