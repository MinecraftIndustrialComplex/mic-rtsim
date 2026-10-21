package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeSet;

/**
 * The stores' contents, per good, sampled every tick of a window; rates are least-squares slopes over a
 * stretch of it. A machine's output is a staircase (one step per cycle), so the end-minus-start
 * difference over a window that is not a whole number of cycles is off by up to one batch; the slope
 * over every tick is off by a fraction of that, about (cycle / window)² of the rate.
 */
public final class StockWindow {
    private final List<BlockPos> stores;
    private final int length;
    private final Map<String, long[]> series = new HashMap<>();
    private int ticks;

    public StockWindow(List<BlockPos> stores, int length) {
        this.stores = List.copyOf(stores);
        this.length = length;
    }

    /** Samples the stores once; call every tick of the window, {@code length} times. */
    public void sample(ServerLevel level) {
        if (ticks >= length)
            return;
        Map<String, Long> now = GlassCorpusKit.totals(level, stores);
        for (var e : now.entrySet())
            series.computeIfAbsent(e.getKey(), k -> new long[length])[ticks] = e.getValue();
        ticks++;
    }

    public boolean full() {
        return ticks >= length;
    }

    public int ticks() {
        return ticks;
    }

    public TreeSet<String> goods() {
        return new TreeSet<>(series.keySet());
    }

    /** The count of {@code good} at window tick {@code t} (0 if never seen). */
    public long at(String good, int t) {
        long[] s = series.get(good);
        return s == null ? 0 : s[t];
    }

    /** Least-squares slope of {@code good} over ticks [from, to), per minute. */
    public double slopePerMinute(String good, int from, int to) {
        to = Math.min(to, ticks);
        int n = to - from;
        if (n < 2)
            return 0;
        long[] s = series.get(good);
        if (s == null)
            return 0;
        double mx = (from + to - 1) / 2.0, my = 0;
        for (int t = from; t < to; t++)
            my += s[t];
        my /= n;
        double sxy = 0, sxx = 0;
        for (int t = from; t < to; t++) {
            double dx = t - mx;
            sxy += dx * (s[t] - my);
            sxx += dx * dx;
        }
        return GlassCorpusKit.perMinute(sxy / sxx);
    }

    /** (last - first) / ticks over [from, to), per minute. */
    public double endsPerMinute(String good, int from, int to) {
        to = Math.min(to, ticks);
        if (to - from < 2)
            return 0;
        return GlassCorpusKit.perMinute((at(good, to - 1) - at(good, from)) / (double) (to - 1 - from));
    }

    /** The goods whose count changed in [from, to). */
    public List<String> moving(int from, int to) {
        List<String> out = new ArrayList<>();
        to = Math.min(to, ticks);
        for (var e : series.entrySet()) {
            long[] s = e.getValue();
            for (int t = from + 1; t < to; t++)
                if (s[t] != s[from]) {
                    out.add(e.getKey());
                    break;
                }
        }
        return out;
    }
}
