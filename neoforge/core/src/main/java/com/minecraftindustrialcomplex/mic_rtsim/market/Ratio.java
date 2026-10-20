package com.minecraftindustrialcomplex.mic_rtsim.market;

/** A ratio {@code num / den} with {@code den > 0}, as rtsim-trade's {@code tunables::Ratio}. */
public record Ratio(long num, long den) {
    public static final Ratio FAIR = new Ratio(1, 1);

    public Ratio {
        if (den <= 0)
            throw new IllegalArgumentException("ratio denominator " + den);
    }

    /**
     * How much worse a disliked player's prices are: a markup for buying (at least 1) and a markdown
     * for selling (at most 1), never better than fair (rtsim-trade {@code market::markup_for}, the
     * engine's overnight choice E7). {@code worst} is the markup at sentiment -128 (a P18 tunable the
     * engine sends in MARKET).
     */
    public static Ratio[] markupFor(int sentiment, Ratio worst) {
        if (sentiment >= 0 || worst.num() <= 0)
            return new Ratio[] {FAIR, FAIR};
        long s = Math.min(128, -(long) Math.max(sentiment, -128));
        long den = 128 * worst.den();
        return new Ratio[] {new Ratio(den + s * worst.num(), den), new Ratio(Math.max(den - s * worst.num(), 0), den)};
    }
}
