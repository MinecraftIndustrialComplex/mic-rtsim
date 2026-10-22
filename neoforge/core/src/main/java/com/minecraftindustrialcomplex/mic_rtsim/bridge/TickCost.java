package com.minecraftindustrialcomplex.mic_rtsim.bridge;

import java.util.Arrays;

/**
 * The server thread's time in rtsim's own tick handlers, one sample per game tick: the industry's
 * {@code startOfTick} at {@code ServerTickEvent.Pre} plus everything at {@code ServerTickEvent.Post}
 * (the loaded queues drained, the session's {@code endOfTick}). What a tick of the server pays for
 * rtsim, as {@code /rtsim perf} shows it (mean, p50, p99, p99.9, worst) since the last
 * {@code /rtsim perf reset} (docs/stress-worlds-results.md, choice SG7).
 *
 * <p>Server thread only. Keeps the last {@value #CAP} ticks (about 3.6 hours at 20 TPS); the mean,
 * worst and count run over every tick since the reset.
 */
public final class TickCost {
    private TickCost() {}

    static final int CAP = 1 << 18;
    private static final long[] RING = new long[CAP];
    private static long pending;
    private static long ticks;
    private static long totalNanos;
    private static long worstNanos;
    private static long worstAt = -1;

    /** Time spent at {@code ServerTickEvent.Pre}, added to this tick's sample. */
    public static void pre(long nanos) {
        pending += nanos;
    }

    /** Time spent at {@code ServerTickEvent.Post}: closes this tick's sample. */
    public static void post(long nanos, long gameTime) {
        long t = pending + nanos;
        pending = 0;
        RING[(int) (ticks & (CAP - 1))] = t;
        ticks++;
        totalNanos += t;
        if (t > worstNanos) {
            worstNanos = t;
            worstAt = gameTime;
        }
    }

    public static void reset() {
        pending = 0;
        ticks = 0;
        totalNanos = 0;
        worstNanos = 0;
        worstAt = -1;
    }

    /** One line for {@code /rtsim perf}. */
    public static String summary() {
        if (ticks == 0)
            return "tick cost: no ticks since the reset";
        int n = (int) Math.min(ticks, CAP);
        long[] s = Arrays.copyOf(RING, n);
        Arrays.sort(s);
        return String.format("tick cost: %d ticks since the reset; mean %.1f us, p50 %.1f us, p99 %.1f us, p99.9 %.1f us, worst %.1f us (game tick %d); %.1f ms in all%s",
                ticks, totalNanos / 1000.0 / ticks, us(s, 0.5), us(s, 0.99), us(s, 0.999), worstNanos / 1000.0, worstAt,
                totalNanos / 1e6, ticks > CAP ? " (percentiles over the last " + CAP + ")" : "");
    }

    private static double us(long[] sorted, double q) {
        int i = (int) Math.min(sorted.length - 1, Math.max(0, Math.ceil(q * sorted.length) - 1));
        return sorted[i] / 1000.0;
    }
}
