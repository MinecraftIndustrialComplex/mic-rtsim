package com.minecraftindustrialcomplex.mic_rtsim.far;

import java.io.ByteArrayOutputStream;
import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.zip.DataFormatException;
import java.util.zip.Deflater;
import java.util.zip.Inflater;

/**
 * What a far object looks like, as the far feed sends it (docs/boundary.md §12.2, docs/choices-dh-feed.md
 * DHT60): the deflated bytes of a kind's own layout (a carriage's blocks, {@link CarriageShape}) and a 64-bit
 * content hash of the raw bytes. The feed's INTRO names an object's shape by the hash alone; the bytes go to
 * each subscriber once, in the {@code mic_rtsim:far_shape} payload, so a hundred carriages of one design cost
 * one copy. Plain Java.
 *
 * <p>Nobody changes {@link #bytes()} after the shape is made.
 */
public final class FarShape {
    /** The most a shape may be on the wire, deflated; a bigger one is not sent (the object is a box). PLACEHOLDER. */
    public static final int MAX_BYTES = 32 * 1024;
    /** The most a shape may inflate to; more is refused as malformed. */
    public static final int MAX_RAW = 1 << 20;

    private final long hash;
    private final byte[] bytes;
    private final int raw;

    private FarShape(long hash, byte[] bytes, int raw) {
        this.hash = hash;
        this.bytes = bytes;
        this.raw = raw;
    }

    /** The shape of {@code raw} (a kind's layout), or null when it deflates to more than {@link #MAX_BYTES}. */
    public static FarShape of(byte[] raw) {
        if (raw.length > MAX_RAW)
            return null;
        byte[] z = deflate(raw);
        return z.length > MAX_BYTES ? null : new FarShape(hash(raw), z, raw.length);
    }

    /**
     * The shapes to send a subscriber this sweep (DHT62): those of {@code chosen} not in {@code sent}, in
     * {@code chosen}'s order (the filter's, nearest first), each once, until their bytes would pass
     * {@code budget} (one bigger than the whole budget still goes, alone). Adds them to {@code sent}.
     */
    public static List<FarShape> pick(List<FarItem> chosen, Set<Long> sent, int budget) {
        List<FarShape> out = new ArrayList<>();
        int left = budget;
        for (FarItem i : chosen) {
            FarShape sh = i.shape();
            if (sh == null || sent.contains(sh.hash()))
                continue;
            int n = sh.bytes().length;
            if (n > left && left < budget)
                break; // the rest next sweep
            out.add(sh);
            sent.add(sh.hash());
            left -= n;
        }
        return out;
    }

    /** The content hash; never 0 (the wire's "no shape"). */
    public long hash() {
        return hash;
    }

    /** The deflated bytes, as the payload carries them. */
    public byte[] bytes() {
        return bytes;
    }

    /** The raw size, for the log. */
    public int rawSize() {
        return raw;
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof FarShape s && s.hash == hash;
    }

    @Override
    public int hashCode() {
        return Long.hashCode(hash);
    }

    @Override
    public String toString() {
        return "FarShape[" + Long.toHexString(hash) + ", " + bytes.length + " bytes]";
    }

    /**
     * The raw layout of shape {@code hash} from its payload's bytes; throws IllegalArgumentException when they
     * do not inflate, inflate past {@link #MAX_RAW}, or do not hash to {@code hash}.
     */
    public static byte[] raw(long hash, byte[] deflated) {
        byte[] raw = inflate(deflated);
        if (hash(raw) != hash)
            throw new IllegalArgumentException("a far shape's bytes hash to " + Long.toHexString(hash(raw)) + ", not "
                    + Long.toHexString(hash));
        return raw;
    }

    /** A 64-bit content hash (FNV-1a, then a finalizer so the low bits mix); 0 becomes 1. */
    public static long hash(byte[] raw) {
        long h = 0xcbf29ce484222325L;
        for (byte x : raw) {
            h ^= x & 0xff;
            h *= 0x100000001b3L;
        }
        h ^= h >>> 33;
        h *= 0xff51afd7ed558ccdL;
        h ^= h >>> 33;
        return h == 0 ? 1 : h;
    }

    static byte[] deflate(byte[] raw) {
        Deflater d = new Deflater(Deflater.BEST_COMPRESSION, true);
        try {
            d.setInput(raw);
            d.finish();
            ByteArrayOutputStream out = new ByteArrayOutputStream(raw.length / 2 + 16);
            byte[] buf = new byte[1024];
            while (!d.finished())
                out.write(buf, 0, d.deflate(buf));
            return out.toByteArray();
        } finally {
            d.end();
        }
    }

    static byte[] inflate(byte[] z) {
        Inflater in = new Inflater(true);
        try {
            in.setInput(z);
            ByteArrayOutputStream out = new ByteArrayOutputStream(Math.min(MAX_RAW, z.length * 4 + 16));
            byte[] buf = new byte[1024];
            while (!in.finished()) {
                int n = in.inflate(buf);
                if (n == 0 && (in.needsInput() || in.needsDictionary()))
                    throw new IllegalArgumentException("a far shape's bytes end early");
                out.write(buf, 0, n);
                if (out.size() > MAX_RAW)
                    throw new IllegalArgumentException("a far shape inflates past " + MAX_RAW + " bytes");
            }
            if (in.getRemaining() > 0)
                throw new IllegalArgumentException(in.getRemaining() + " bytes after a far shape");
            return out.toByteArray();
        } catch (DataFormatException e) {
            throw new IllegalArgumentException("a far shape does not inflate: " + e.getMessage());
        } finally {
            in.end();
        }
    }
}
