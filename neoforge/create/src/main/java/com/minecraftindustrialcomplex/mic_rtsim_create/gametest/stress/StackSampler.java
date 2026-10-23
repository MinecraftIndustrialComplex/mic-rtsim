package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.stress;

import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.locks.LockSupport;

/**
 * A sampling profiler for one stretch of work on one thread (the stress generator's MIC_STRESS_PROFILE, SG60):
 * a daemon thread takes the target's stack every {@code intervalNanos}, and keeps the frames above
 * {@code root} (a method name; samples without it are not counted). Safepoint-biased, so read it as shares of
 * the stretch, not as times; the stretch's own wall clock is the time.
 */
final class StackSampler implements AutoCloseable {
    private final Thread target;
    private final String root;
    private final long intervalNanos;
    private volatile boolean stop;
    private final Thread thread;
    private final Map<String, Integer> leaves = new HashMap<>();
    private final Map<String, Integer> inclusive = new HashMap<>();
    private int samples;

    StackSampler(Thread target, String root, long intervalNanos) {
        this.target = target;
        this.root = root;
        this.intervalNanos = intervalNanos;
        this.thread = new Thread(this::run, "stress-stack-sampler");
        thread.setDaemon(true);
        thread.start();
    }

    private void run() {
        while (!stop) {
            StackTraceElement[] st = target.getStackTrace();
            int top = -1;
            for (int i = 0; i < st.length; i++)
                if (st[i].getMethodName().equals(root)) {
                    top = i;
                    break;
                }
            if (top > 0) {
                synchronized (this) {
                    samples++;
                    leaves.merge(key(st[0]), 1, Integer::sum);
                    java.util.Set<String> seen = new java.util.HashSet<>();
                    for (int i = 0; i < top; i++) {
                        String k = key(st[i]);
                        if (seen.add(k))
                            inclusive.merge(k, 1, Integer::sum);
                    }
                }
            }
            LockSupport.parkNanos(intervalNanos);
        }
    }

    private static String key(StackTraceElement e) {
        String c = e.getClassName();
        return c.substring(c.lastIndexOf('.') + 1) + "." + e.getMethodName();
    }

    @Override
    public void close() {
        stop = true;
        LockSupport.unpark(thread);
        try {
            thread.join(2000);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
    }

    /** The samples counted, and the {@code top} most frequent leaf frames and inclusive frames, as log lines. */
    synchronized List<String> report(int top) {
        List<String> out = new java.util.ArrayList<>();
        out.add(samples + " samples above " + root);
        out.add("leaves:");
        lines(out, leaves, top);
        out.add("inclusive:");
        lines(out, inclusive, top * 2);
        return out;
    }

    private void lines(List<String> out, Map<String, Integer> m, int top) {
        m.entrySet().stream().sorted((a, b) -> b.getValue() - a.getValue()).limit(top).forEach(e ->
                out.add(String.format(java.util.Locale.ROOT, "  %5.1f%% %s", 100.0 * e.getValue() / Math.max(1, samples), e.getKey())));
    }
}
