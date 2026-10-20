package com.minecraftindustrialcomplex.mic_rtsim.market;

import java.util.ArrayDeque;
import java.util.HashMap;
import java.util.Iterator;
import java.util.Map;

/**
 * One village market as the bridge sees it (C13, C14, C23): the engine's last MARKET plus the trades
 * the bridge itself made since, which the MARKET does not include yet.
 *
 * <p><b>The bridge shows the engine's bundles; it computes no price</b> (Ben's morning review,
 * "Prices": one source of truth). MARKET carries, per good and side, a ladder of ready-made bundles
 * per price level: what a run of trades meets one after another, each priced at the market the
 * trades before it left. The bridge finds where its own trades since the MARKET left the stock, and
 * shows the step there; a stock the ladder does not reach (a sale and a purchase of one good mixed
 * since the MARKET) shows the offer crossed out until the next MARKET, which the engine sends right
 * after a trade (choice PR2). The price level is the player's standing (G31, folded into the sim's
 * quote: vanilla's reputation with that villager, or a ribbit village's sentiment), looked up in the
 * MARKET's thresholds.
 *
 * <p>PLAN.md §8.5: "the bridge updates its cached stock and treasury at once; the engine applies the
 * trade next step". A MARKET says which observations it already reflects ({@code asOfObsSeq}); the
 * bridge keeps each local trade under its observation sequence and replays those above it on top of
 * a new MARKET, so one computed before a trade never gives the treasury back (choice G5). Pure Java,
 * keyed by any good type {@code G} (an {@code Item} in game, a string in tests).
 */
public final class MarketBook<G> {

    /**
     * One good's line in the MARKET: the stock its ladders start from, and the ladders,
     * {@code ladders[side][level]} as pairs {@code (items, coins)} flattened (side 0: the market sells,
     * side 1: it buys).
     */
    public record Good(long stock, long[][][] ladders) {
        public int steps(Side side, int level) {
            long[][] s = ladders[side.ordinal()];
            return level < 0 || level >= s.length ? 0 : s[level].length / 2;
        }

        long items(Side side, int level, int j) {
            return ladders[side.ordinal()][level][2 * j];
        }

        long coins(Side side, int level, int j) {
            return ladders[side.ordinal()][level][2 * j + 1];
        }
    }

    /** A vanilla-shaped offer: {@code items} of the good for {@code coins}, {@code uses} times. */
    public record Offer(long items, long coins, long uses) {}

    /** Which way an offer goes, from the market's side. */
    public enum Side { MARKET_SELLS, MARKET_BUYS }

    private record Local<G>(long obsSeq, G good, long dStock, long dTreasury) {}

    private final long village;
    private G currency;
    private long asOfObsSeq;
    private long baseTreasury;
    private long treasuryTarget;
    private int[] thresholds = {127};
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

    /** True once a MARKET has arrived this session. */
    public boolean known() {
        return known;
    }

    public G currency() {
        return currency;
    }

    public long asOfObsSeq() {
        return asOfObsSeq;
    }

    /** Replaces the snapshot. Local trades it already includes are dropped; the rest are kept. */
    public void snapshot(long asOfObsSeq, G currency, long treasury, long treasuryTarget, int[] thresholds, Map<G, Good> goods) {
        this.asOfObsSeq = asOfObsSeq;
        this.currency = currency;
        this.baseTreasury = treasury;
        this.treasuryTarget = treasuryTarget;
        this.thresholds = thresholds.length == 0 ? new int[] {127} : thresholds.clone();
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

    /** The treasury is above twice its target: the market refuses cash (PLAN.md §8.4). */
    public boolean cashRefused() {
        return treasuryTarget > 0 && PriceOracle.glut(treasuryView, treasuryTarget);
    }

    /** Price levels in the MARKET; level 0 is the fair price. */
    public int levels() {
        return thresholds.length;
    }

    /**
     * The price level for a player of this standing (G31): the deepest level whose threshold the
     * standing is at or below. A lookup in the engine's thresholds, not a formula of the bridge's.
     */
    public int levelOf(int standing) {
        int level = 0;
        for (int k = 1; k < thresholds.length; k++)
            if (standing <= thresholds[k])
                level = k;
        return level;
    }

    /**
     * The engine's bundle for the next trade on {@code side} at {@code level}: the ladder's step where
     * the trades since the MARKET left this good's stock, with uses for the rest of the ladder (the
     * market's sales) or as far as the treasury pays (its purchases). Null when the ladder has no step
     * there: refused, sold out, out of money, or a stock it does not reach.
     */
    public Offer offer(G good, Side side, int level) {
        Good g = base.get(good);
        if (g == null || good.equals(currency))
            return null;
        if (side == Side.MARKET_SELLS && cashRefused())
            return null;
        int n = g.steps(side, level);
        long delta = stock(good) - g.stock(), acc = 0;
        int p = 0;
        if (side == Side.MARKET_SELLS) {
            while (acc > delta && p < n)
                acc -= g.items(side, level, p++) * PriceCurve.MILLI;
        } else {
            while (acc < delta && p < n)
                acc += g.items(side, level, p++) * PriceCurve.MILLI;
        }
        if (acc != delta || p >= n)
            return null;
        long uses;
        if (side == Side.MARKET_SELLS) {
            uses = n - p;
        } else {
            uses = 0;
            long t = treasury();
            for (int j = p; j < n && g.coins(side, level, j) * PriceCurve.MILLI <= t; j++) {
                t -= g.coins(side, level, j) * PriceCurve.MILLI;
                uses++;
            }
        }
        return uses > 0 ? new Offer(g.items(side, level, p), g.coins(side, level, p), uses) : null;
    }

    /**
     * Uses left for an offer whose counts the bridge set earlier (a restock keeps the counts): the
     * uses of whichever level's next step has exactly these counts, else none.
     */
    public long usesFor(G good, Side side, long items, long coins) {
        for (int k = 0; k < levels(); k++) {
            Offer o = offer(good, side, k);
            if (o != null && o.items() == items && o.coins() == coins)
                return o.uses();
        }
        return 0;
    }
}
