package com.minecraftindustrialcomplex.mic_rtsim_dh;

import org.lwjgl.opengl.GL15;
import org.lwjgl.opengl.GL33;

/**
 * GPU time of one span per frame, with {@code GL_TIME_ELAPSED} queries in a ring so reading a result never
 * waits on the GPU. {@link #lastNanos} is the newest finished span; spans must not nest across timers.
 */
final class GpuTimer {
    private static final int RING = 8;
    private final int[] ids = new int[RING];
    private final boolean[] pending = new boolean[RING];
    private int next;
    private boolean open;
    long lastNanos = -1, sumNanos, samples;

    void begin() {
        if (ids[0] == 0)
            for (int i = 0; i < RING; i++)
                ids[i] = GL15.glGenQueries();
        collect();
        if (pending[next])
            return; // the ring is full of unread spans: skip this frame rather than wait
        GL15.glBeginQuery(GL33.GL_TIME_ELAPSED, ids[next]);
        open = true;
    }

    void end() {
        if (!open)
            return;
        GL15.glEndQuery(GL33.GL_TIME_ELAPSED);
        pending[next] = true;
        next = (next + 1) % RING;
        open = false;
    }

    /** Reads every finished span. */
    void collect() {
        for (int k = 1; k <= RING; k++) {
            int i = (next + k) % RING;
            if (!pending[i])
                continue;
            if (GL15.glGetQueryObjecti(ids[i], GL15.GL_QUERY_RESULT_AVAILABLE) == 0)
                continue;
            lastNanos = GL33.glGetQueryObjecti64(ids[i], GL15.GL_QUERY_RESULT);
            sumNanos += lastNanos;
            samples++;
            pending[i] = false;
        }
    }

    void resetSums() {
        sumNanos = 0;
        samples = 0;
    }
}
