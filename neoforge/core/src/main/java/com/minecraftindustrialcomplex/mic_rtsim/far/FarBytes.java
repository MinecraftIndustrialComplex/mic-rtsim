package com.minecraftindustrialcomplex.mic_rtsim.far;

import java.nio.charset.StandardCharsets;
import java.util.Arrays;

/**
 * The far feed's byte writer and reader ({@link FarWire}, {@link CarriageShape}): unsigned LEB128 varints,
 * zigzag varints, little-endian i64s and length-prefixed UTF-8. A reader throws IllegalArgumentException on
 * anything short or out of range, never an index error. Plain Java.
 */
final class FarBytes {
    private FarBytes() {}

    static final class Out {
        private byte[] b;
        private int n;

        Out(int cap) {
            b = new byte[Math.max(16, cap)];
        }

        private void room(int k) {
            if (n + k > b.length)
                b = Arrays.copyOf(b, Math.max(b.length * 2, n + k));
        }

        void u8(int v) {
            room(1);
            b[n++] = (byte) v;
        }

        void bytes(byte[] v) {
            room(v.length);
            System.arraycopy(v, 0, b, n, v.length);
            n += v.length;
        }

        void var(long v) {
            room(10);
            while ((v & ~0x7FL) != 0) {
                b[n++] = (byte) ((v & 0x7F) | 0x80);
                v >>>= 7;
            }
            b[n++] = (byte) v;
        }

        void zig(long v) {
            var((v << 1) ^ (v >> 63));
        }

        void i64(long v) {
            room(8);
            for (int k = 0; k < 8; k++)
                b[n++] = (byte) (v >>> (8 * k));
        }

        void utf(String s) {
            byte[] u = s.getBytes(StandardCharsets.UTF_8);
            var(u.length);
            bytes(u);
        }

        byte[] done() {
            return Arrays.copyOf(b, n);
        }
    }

    static final class In {
        private final byte[] b;
        private final String what;
        int at;

        /** {@code what} names the bytes in errors ("the far feed message"). */
        In(byte[] b, String what) {
            this.b = b;
            this.what = what;
        }

        boolean done() {
            return at == b.length;
        }

        int left() {
            return b.length - at;
        }

        private void need(int k) {
            if (k < 0 || k > b.length - at)
                throw new IllegalArgumentException(what + " ends early, at byte " + at);
        }

        int u8() {
            need(1);
            return b[at++] & 0xFF;
        }

        byte[] bytes(int k) {
            need(k);
            byte[] v = Arrays.copyOfRange(b, at, at + k);
            at += k;
            return v;
        }

        long var() {
            long v = 0;
            for (int shift = 0; shift < 64; shift += 7) {
                int x = u8();
                v |= (long) (x & 0x7F) << shift;
                if ((x & 0x80) == 0)
                    return v;
            }
            throw new IllegalArgumentException("a varint in " + what + " runs past 10 bytes, at byte " + at);
        }

        /** A varint that must be a count, a handle or a length (0 to 2^31 - 1). */
        int len() {
            long v = var();
            if (v < 0 || v > Integer.MAX_VALUE)
                throw new IllegalArgumentException("a count of " + v + " in " + what);
            return (int) v;
        }

        /** A count of at most {@code max}. */
        int len(int max, String of) {
            int v = len();
            if (v > max)
                throw new IllegalArgumentException(v + " " + of + " in " + what + ", at most " + max);
            return v;
        }

        long zig() {
            long v = var();
            return (v >>> 1) ^ -(v & 1);
        }

        /** A zigzag varint that must fit an int. */
        int zigInt() {
            long v = zig();
            if (v < Integer.MIN_VALUE || v > Integer.MAX_VALUE)
                throw new IllegalArgumentException("a value of " + v + " in " + what);
            return (int) v;
        }

        long i64() {
            need(8);
            long v = 0;
            for (int k = 0; k < 8; k++)
                v |= (long) (b[at++] & 0xFF) << (8 * k);
            return v;
        }

        String utf(int maxBytes) {
            return new String(bytes(len(maxBytes, "bytes of text")), StandardCharsets.UTF_8);
        }
    }
}
