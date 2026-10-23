package com.minecraftindustrialcomplex.mic_rtsim.client;

import java.util.HashMap;
import java.util.Map;

/**
 * The far shapes the server sent this client since its view last started over (docs/choices-dh-feed.md DHT62):
 * a shape's hash to its deflated bytes, as {@link com.minecraftindustrialcomplex.mic_rtsim.far.FarShape} has
 * them. The server sends each shape once per view and forgets what it sent on a RESET; the store forgets on the
 * RESET too, so the two agree. A far renderer reads a mover's shape here by its hash. The client's main thread
 * only (the payload handler and the render thread are that thread). Plain Java.
 */
public final class FarShapeStore {
    /**
     * The most shapes and bytes kept: above the server's own bound on what it holds a client to have (1,024
     * shapes of at most 32 KiB, and the few more one sweep sends before it checks), so only a misbehaving server
     * reaches it. Past it, a new shape is dropped (its objects stay boxes).
     */
    static final int MAX_SHAPES = 4096, MAX_BYTES = 40 << 20;

    private final Map<Long, byte[]> shapes = new HashMap<>();
    private long bytes;
    long dropped;

    /** Keeps shape {@code hash}; false when it is over the limits (or the hash is 0, "none"). */
    boolean put(long hash, byte[] data) {
        if (hash == 0L)
            return false;
        byte[] had = shapes.get(hash);
        long after = bytes - (had == null ? 0 : had.length) + data.length;
        if ((had == null && shapes.size() >= MAX_SHAPES) || after > MAX_BYTES) {
            dropped++;
            return false;
        }
        shapes.put(hash, data);
        bytes = after;
        return true;
    }

    /** The deflated bytes of shape {@code hash}, or null when this client does not have them (yet). */
    byte[] get(long hash) {
        return shapes.get(hash);
    }

    void clear() {
        shapes.clear();
        bytes = 0;
    }

    int size() {
        return shapes.size();
    }

    long bytes() {
        return bytes;
    }
}
