package com.minecraftindustrialcomplex.mic_rtsim.market;

import java.util.ArrayDeque;
import java.util.HashMap;
import java.util.Iterator;
import java.util.Map;

/**
 * One village market as the bridge sees it (C13, C14, C23): the engine's last MARKET snapshot plus
 * the trades the bridge itself made since, which the snapshot does not include yet.
 *
 * <p>PLAN.md §8.5: "the bridge updates its cached stock and treasury at once; the engine applies the
 * trade next step". A snapshot says which observations it already reflects ({@code asOfObsSeq}); the
 * bridge keeps each local trade under its observation sequence and replays those above it on top of a
 * new snapshot, so a snapshot computed before a trade never gives the treasury back (choice G5).
 *
 * <p>{@link #offer} mirrors rtsim-trade's {@code Market::offer} exactly: the integer bundle closest to
 * the market's price, "one item for whole coins when an item is worth a coin or more, else whole
 * items for one coin", with uses from the stock (what the market sells) or the treasury (what it
 * pays for). Pure Java, keyed by any good type {@code G} (an {@code Item} in game, a string in tests).
 */
public final class MarketBook<G> {

    /** One good's line in the snapshot. */
    public record Good(long stock, PriceCurve curve) {}

    /** A priced, vanilla-shaped offer: {@code items} of the good for {@code coins}, {@code uses} times. */
    public record Offer(long items, long coins, long uses) {}

    /** Which way an offer goes, from the market's side. */
    public enum Side { MARKET_SELLS, MARKET_BUYS }

    private record Local<G>(long obsSeq, G good, long dStock, long dTreasury) {}

    private final long village;
    private G currency;
    private long asOfObsSeq;
    private long baseTreasury;
    private long treasuryTarget;
    private Ratio worst = new Ratio(0, 1);
    private final Map<G, Good> base = new HashMap<>();
    private final ArrayDeque<Local<G>> locals = new ArrayDeque<>();
    // The view: the snapshot plus every local trade above asOfObsSeq.
    private final Map<G, Long> stockView = new HashMap<>();
    private long treasuryView;
    private boolean known;

    public MarketBook(long village) {
        this.village = village;
    }

    public long village() {
        return village;
    }

    /** True once a snapshot has arrived this session. */
    public boolean known() {
        return known;
    }

    public G currency() {
        return currency;
    }

    public long asOfObsSeq() {
        return asOfObsSeq;
    }

    public Ratio worst() {
        return worst;
    }

    /** Replaces the snapshot. Local trades the snapshot already includes are dropped; the rest are kept. */
    public void snapshot(long asOfObsSeq, G currency, long treasury, long treasuryTarget, Ratio worst, Map<G, Good> goods) {
        this.asOfObsSeq = asOfObsSeq;
        this.currency = currency;
        this.baseTreasury = treasury;
        this.treasuryTarget = treasuryTarget;
        this.worst = worst;
        base.clear();
        base.putAll(goods);
        for (Iterator<Local<G>> it = locals.iterator(); it.hasNext(); )
            if (it.next().obsSeq() <= asOfObsSeq)
                it.remove();
        known = true;
        rebuild();
    }

    private void rebuild() {
        stockView.clear();
        for (Map.Entry<G, Good> e : base.entrySet())
            stockView.put(e.getKey(), e.getValue().stock());
        treasuryView = baseTreasury;
        for (Local<G> l : locals) {
            if (l.good() != null)
                stockView.merge(l.good(), l.dStock(), Long::sum);
            treasuryView += l.dTreasury();
        }
    }

    /**
     * A trade the bridge saw and journaled under {@code obsSeq}: the player bought ({@code MARKET_SELLS})
     * or sold ({@code MARKET_BUYS}) {@code items} whole items for {@code coins} whole coins.
     */
    public void local(long obsSeq, G good, Side side, long items, long coins) {
        long dStock = (side == Side.MARKET_SELLS ? -items : items) * PriceCurve.MILLI;
        long dTreasury = (side == Side.MARKET_SELLS ? coins : -coins) * PriceCurve.MILLI;
        locals.addLast(new Local<>(obsSeq, good, base.containsKey(good) ? dStock : 0, dTreasury));
        if (base.containsKey(good))
            stockView.merge(good, dStock, Long::sum);
        treasuryView += dTreasury;
    }

    public boolean trades(G good) {
        return base.containsKey(good);
    }

    /** {@code p(S)} now, in millionths of a coin per item; 0 for a good the market does not trade. */
    public long priceNow(G good) {
        Good g = base.get(good);
        return g == null ? 0 : g.curve().priceAt(stock(good));
    }

    /** The stock now, in thousandths (snapshot plus local trades). */
    public long stock(G good) {
        return stockView.getOrDefault(good, 0L);
    }

    /** The treasury now, in thousandths. */
    public long treasury() {
        return treasuryView;
    }

    public int pendingLocals() {
        return locals.size();
    }

    /** rtsim-trade {@code level_of(stock, target) == Glut}: more than twice the target. */
    private static boolean glut(long stock, long target) {
        return stock > 2 * Math.max(target, 1);
    }

    /** The treasury is above twice its target: the market refuses cash (PLAN.md §8.4). */
    public boolean cashRefused() {
        return treasuryTarget > 0 && glut(treasuryView, treasuryTarget);
    }

    /**
     * rtsim-trade {@code Market::offer}. {@code markup} is the buy markup for {@code MARKET_SELLS} and
     * the sell markdown for {@code MARKET_BUYS} ({@link Ratio#markupFor}). Null when refused.
     */
    public Offer offer(G good, Side side, Ratio markup) {
        Good g = base.get(good);
        if (g == null || good.equals(currency))
            return null;
        long s = stock(good);
        PriceCurve curve = g.curve();
        if (side == Side.MARKET_SELLS) {
            if (cashRefused() || s < PriceCurve.MILLI)
                return null;
            long one = curve.buyValue(s, PriceCurve.MILLI).scaled(markup).coinsCeil();
            long items, coins;
            if (curve.priceAt(s) >= PriceCurve.PRICE_ONE || one > 1) {
                items = 1;
                coins = Math.max(one, 1);
            } else {
                long lo = 1, hi = s / PriceCurve.MILLI;
                while (lo < hi) {
                    long mid = lo + (hi - lo + 1) / 2;
                    long c = curve.buyValue(s, mid * PriceCurve.MILLI).scaled(markup).coinsCeil();
                    if (c <= 1)
                        lo = mid;
                    else
                        hi = mid - 1;
                }
                items = lo;
                coins = 1;
            }
            long uses = Math.min(s / (items * PriceCurve.MILLI), 0xFFFF_FFFFL);
            return uses > 0 ? new Offer(items, coins, uses) : null;
        }
        if (glut(s, curve.target()))
            return null;
        long m = treasury();
        long one = curve.sellValue(s, PriceCurve.MILLI).scaled(markup).coinsFloor();
        long items, coins;
        if (one >= 1) {
            items = 1;
            coins = one;
        } else {
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
            items = lo;
            coins = 1;
        }
        long uses = m <= 0 ? 0 : Math.min(m / (coins * PriceCurve.MILLI), 0xFFFF_FFFFL);
        return uses > 0 ? new Offer(items, coins, uses) : null;
    }
}
