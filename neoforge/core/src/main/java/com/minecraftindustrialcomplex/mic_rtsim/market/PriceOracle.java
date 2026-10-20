package com.minecraftindustrialcomplex.mic_rtsim.market;

import java.util.ArrayList;
import java.util.List;

/**
 * <b>A test oracle, never the live price.</b> The bridge's copy of rtsim-trade's price math
 * ({@code market::bundle_at} and {@code market::ladder}, over {@link PriceCurve}): since Ben's morning
 * review ("Prices": one source of truth) the engine sends ready-made bundles in MARKET and the bridge
 * only shows them ({@link MarketBook}). This copy stays only to check the engine's bundles against an
 * independent computation (the unit tests use rtsim-trade's vectors) and to let the pure-Java stub
 * engine, which simulates nothing, write the MARKET a test scripts (choice PR2).
 */
public final class PriceOracle {
    private PriceOracle() {}

    /** One bundle: {@code items} of the good for {@code coins}. */
    public record Bundle(long items, long coins) {}

    /** rtsim-trade {@code level_of(stock, target) == Glut}: more than twice the target. */
    static boolean glut(long stock, long target) {
        return stock > 2 * Math.max(target, 1);
    }

    /**
     * rtsim-trade {@code market::bundle_at}: the integer bundle closest to the curve's price at
     * {@code stock}, one item for whole coins when an item is worth a coin or more, else whole items for
     * one coin, rounded against the player. {@code target} is the good's (glut above twice it),
     * {@code refused} whether the treasury refuses cash. Null when refused.
     */
    public static Bundle bundleAt(PriceCurve curve, long stock, long target, boolean refused, MarketBook.Side side, Ratio markup) {
        long s = stock;
        if (side == MarketBook.Side.MARKET_SELLS) {
            if (refused || s < PriceCurve.MILLI)
                return null;
            long one = curve.buyValue(s, PriceCurve.MILLI).scaled(markup).coinsCeil();
            if (curve.priceAt(s) >= PriceCurve.PRICE_ONE || one > 1)
                return new Bundle(1, Math.max(one, 1));
            long lo = 1, hi = s / PriceCurve.MILLI;
            while (lo < hi) {
                long mid = lo + (hi - lo + 1) / 2;
                long c = curve.buyValue(s, mid * PriceCurve.MILLI).scaled(markup).coinsCeil();
                if (c <= 1)
                    lo = mid;
                else
                    hi = mid - 1;
            }
            return new Bundle(lo, 1);
        }
        if (glut(s, target))
            return null;
        long one = curve.sellValue(s, PriceCurve.MILLI).scaled(markup).coinsFloor();
        if (one >= 1)
            return new Bundle(1, one);
        long n = 1;
        while (curve.sellValue(s, n * PriceCurve.MILLI).scaled(markup).coinsFloor() < 1) {
            n *= 2;
            if (n > 1 << 20)
                return null;
        }
        long lo = n / 2 + 1, hi = n;
        while (lo < hi) {
            long mid = lo + (hi - lo) / 2;
            if (curve.sellValue(s, mid * PriceCurve.MILLI).scaled(markup).coinsFloor() >= 1)
                hi = mid;
            else
                lo = mid + 1;
        }
        return new Bundle(lo, 1);
    }

    /**
     * rtsim-trade {@code market::ladder}: up to {@code depth} successive bundles on one side, each at
     * the market the ones before left; it ends where the market refuses or the treasury cannot pay.
     */
    public static List<Bundle> ladder(PriceCurve curve, long stock, long target, long treasury, long treasuryTarget,
                                      MarketBook.Side side, Ratio markup, int depth) {
        List<Bundle> out = new ArrayList<>(depth);
        long s = stock, t = treasury;
        while (out.size() < depth) {
            boolean refused = treasuryTarget > 0 && glut(t, treasuryTarget);
            Bundle b = bundleAt(curve, s, target, refused, side, markup);
            if (b == null)
                break;
            if (side == MarketBook.Side.MARKET_SELLS) {
                s -= b.items() * PriceCurve.MILLI;
                t += b.coins() * PriceCurve.MILLI;
            } else {
                if (b.coins() * PriceCurve.MILLI > t)
                    break;
                s += b.items() * PriceCurve.MILLI;
                t -= b.coins() * PriceCurve.MILLI;
            }
            out.add(b);
        }
        return out;
    }

    /** The engine's price levels besides the fair one (rtsim-jni {@code phase1::PRICE_LEVELS}). */
    public static final int[] PRICE_LEVELS = {-25, -50, -75, -100};
    /** Bundles per ladder (rtsim-jni {@code phase1::LADDER_DEPTH}). */
    public static final int LADDER_DEPTH = 8;

    /** Every level's thresholds as MARKET carries them: level 0 (fair) applies to everyone. */
    public static int[] thresholds() {
        int[] t = new int[PRICE_LEVELS.length + 1];
        t[0] = 127;
        System.arraycopy(PRICE_LEVELS, 0, t, 1, PRICE_LEVELS.length);
        return t;
    }

    /**
     * A good's ladders as MARKET carries them, {@code [side][level]}: level 0 fair, level k the markup
     * at its threshold (rtsim-trade {@code markup_for}), as rtsim-jni's {@code market_ladders} builds
     * them.
     */
    public static List<Bundle>[][] ladders(PriceCurve curve, long stock, long treasury, long treasuryTarget, Ratio worst) {
        int[] t = thresholds();
        @SuppressWarnings("unchecked")
        List<Bundle>[][] out = new List[2][t.length];
        for (int k = 0; k < t.length; k++) {
            Ratio[] mk = k == 0 ? new Ratio[] {Ratio.FAIR, Ratio.FAIR} : Ratio.markupFor(t[k], worst);
            out[0][k] = ladder(curve, stock, curve.target(), treasury, treasuryTarget, MarketBook.Side.MARKET_SELLS, mk[0], LADDER_DEPTH);
            out[1][k] = ladder(curve, stock, curve.target(), treasury, treasuryTarget, MarketBook.Side.MARKET_BUYS, mk[1], LADDER_DEPTH);
        }
        return out;
    }
}
